//! Independent native X11 test peer (no CleanCopy clipboard/backend code).
//! target app <WM_CLASS> <output-dir>
//! target fixture <directory>    # targets.tsv: target<TAB>type<TAB>8|16|32<TAB>file
//! target read <target> [output] # raw bytes; TARGETS prints one atom per line
//! Files ready/window identify the mapped app; events.tsv records native keys and pastes.

use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, Event},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn atom(conn: &RustConnection, name: &str) -> Result<Atom> {
    Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
}
fn name(conn: &RustConnection, id: Atom) -> Result<String> {
    Ok(String::from_utf8(conn.get_atom_name(id)?.reply()?.name)?)
}
fn millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
fn window(conn: &RustConnection, screen: usize, events: EventMask) -> Result<Window> {
    let id = conn.generate_id()?;
    conn.create_window(
        0,
        id,
        conn.setup().roots[screen].root,
        80,
        80,
        900,
        600,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .background_pixel(0x172334)
            .event_mask(events),
    )?
    .check()?;
    Ok(id)
}
fn wait(conn: &RustConnection, deadline: Instant) -> Result<Event> {
    loop {
        if let Some(event) = conn.poll_for_event()? {
            return Ok(event);
        }
        if Instant::now() >= deadline {
            return Err("X11 selection timed out".into());
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn read(conn: &RustConnection, window: Window, target: Atom) -> Result<GetPropertyReply> {
    let clipboard = atom(conn, "CLIPBOARD")?;
    let property = atom(conn, "_CLEANCOPY_TEST_DATA")?;
    let incr = atom(conn, "INCR")?;
    // Optional slow reader (report v8 D-A, WPS starts reading ~1.9 s late and
    // takes seconds per transfer): delay before the request and per chunk.
    let delay = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .map(Duration::from_millis)
            .unwrap_or_default()
    };
    let (read_delay, chunk_delay) = (
        delay("CLEANCOPY_TARGET_READ_DELAY_MS"),
        delay("CLEANCOPY_TARGET_CHUNK_DELAY_MS"),
    );
    thread::sleep(read_delay);
    conn.convert_selection(window, clipboard, target, property, x11rb::CURRENT_TIME)?;
    conn.flush()?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Event::SelectionNotify(event) = wait(conn, deadline)? {
            if event.selection != clipboard || event.target != target {
                continue;
            }
            if event.property == 0 {
                return Err("selection owner refused target".into());
            }
            let mut result = conn
                .get_property(true, window, property, AtomEnum::ANY, 0, u32::MAX)?
                .reply()?;
            if result.type_ != incr {
                return Ok(result);
            }
            result.value.clear();
            result.value_len = 0;
            conn.flush()?;
            loop {
                if let Event::PropertyNotify(event) = wait(conn, deadline)? {
                    if event.window != window
                        || event.atom != property
                        || event.state != Property::NEW_VALUE
                    {
                        continue;
                    }
                    thread::sleep(chunk_delay);
                    let chunk = conn
                        .get_property(true, window, property, AtomEnum::ANY, 0, u32::MAX)?
                        .reply()?;
                    conn.flush()?;
                    if chunk.value.is_empty() {
                        return Ok(result);
                    }
                    result.type_ = chunk.type_;
                    result.format = chunk.format;
                    result.value_len += chunk.value_len;
                    result.value.extend(chunk.value);
                }
            }
        }
    }
}

fn fixture(dir: &Path) -> Result<()> {
    let (conn, screen) = x11rb::connect(None)?;
    let window = window(&conn, screen, EventMask::PROPERTY_CHANGE)?;
    let clipboard = atom(&conn, "CLIPBOARD")?;
    let targets = atom(&conn, "TARGETS")?;
    let mut entries = Vec::new();
    for line in fs::read_to_string(dir.join("targets.tsv"))?.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 4 {
            return Err("targets.tsv requires target, type, format, filename".into());
        }
        let format: u8 = fields[2].parse()?;
        let bytes = fs::read(dir.join(fields[3]))?;
        if ![8, 16, 32].contains(&format) || bytes.len() % usize::from(format / 8) != 0 {
            return Err("invalid native fixture format/length".into());
        }
        entries.push((
            atom(&conn, fields[0])?,
            atom(&conn, fields[1])?,
            format,
            bytes,
        ));
    }
    conn.set_selection_owner(window, clipboard, x11rb::CURRENT_TIME)?
        .check()?;
    conn.flush()?;
    fs::write(dir.join("ready"), window.to_string())?;
    loop {
        match conn.wait_for_event()? {
            Event::SelectionRequest(request) => {
                let property = if request.property == 0 {
                    request.target
                } else {
                    request.property
                };
                let mut accepted = false;
                if request.target == targets {
                    let ids: Vec<_> = std::iter::once(targets)
                        .chain(entries.iter().map(|entry| entry.0))
                        .collect();
                    conn.change_property32(
                        PropMode::REPLACE,
                        request.requestor,
                        property,
                        AtomEnum::ATOM,
                        &ids,
                    )?;
                    accepted = true;
                } else if let Some((_, type_, format, bytes)) =
                    entries.iter().find(|entry| entry.0 == request.target)
                {
                    match format {
                        8 => {
                            conn.change_property8(
                                PropMode::REPLACE,
                                request.requestor,
                                property,
                                *type_,
                                bytes,
                            )?;
                        }
                        16 => {
                            let values: Vec<_> = bytes
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|b| u16::from_ne_bytes(*b))
                                .collect();
                            conn.change_property16(
                                PropMode::REPLACE,
                                request.requestor,
                                property,
                                *type_,
                                &values,
                            )?;
                        }
                        32 => {
                            let values: Vec<_> = bytes
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .map(|b| u32::from_ne_bytes(*b))
                                .collect();
                            conn.change_property32(
                                PropMode::REPLACE,
                                request.requestor,
                                property,
                                *type_,
                                &values,
                            )?;
                        }
                        _ => unreachable!(),
                    }
                    accepted = true;
                }
                conn.send_event(
                    false,
                    request.requestor,
                    EventMask::NO_EVENT,
                    SelectionNotifyEvent {
                        response_type: SELECTION_NOTIFY_EVENT,
                        sequence: 0,
                        time: request.time,
                        requestor: request.requestor,
                        selection: request.selection,
                        target: request.target,
                        property: if accepted { property } else { 0 },
                    },
                )?;
                conn.flush()?;
            }
            // Keep the original fixture process alive, but never reclaim ownership.
            Event::SelectionClear(_) => {}
            _ => {}
        }
    }
}

fn app(class: &str, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let (conn, screen) = x11rb::connect(None)?;
    let window = window(
        &conn,
        screen,
        EventMask::KEY_PRESS | EventMask::EXPOSURE | EventMask::PROPERTY_CHANGE,
    )?;
    let title = format!("CleanCopy native target — {class}");
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        format!("cleancopy-target\0{class}\0").as_bytes(),
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        title.as_bytes(),
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        window,
        atom(&conn, "_NET_WM_NAME")?,
        atom(&conn, "UTF8_STRING")?,
        title.as_bytes(),
    )?;
    let gc = conn.generate_id()?;
    conn.create_gc(
        gc,
        window,
        &CreateGCAux::new().foreground(0xe4edf5).background(0x172334),
    )?;
    conn.map_window(window)?;
    conn.flush()?;
    fs::write(dir.join("window"), window.to_string())?;
    fs::write(dir.join("ready"), b"ready")?;
    let html = class.to_ascii_lowercase().contains("word");
    let target_name = if html { "text/html" } else { "UTF8_STRING" };
    let mut events = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("events.tsv"))?;
    let mut count = 0;
    let mut document = Vec::new();
    let mut history = Vec::new();
    let mut status = "Waiting for native Ctrl+V".to_owned();
    loop {
        match conn.wait_for_event()? {
            Event::Expose(_) => {
                conn.image_text8(
                    window,
                    gc,
                    32,
                    48,
                    b"CleanCopy / independent X11 clipboard target",
                )?;
                conn.image_text8(window, gc, 32, 80, class.as_bytes())?;
                conn.image_text8(window, gc, 32, 112, status.as_bytes())?;
                conn.flush()?;
            }
            Event::KeyPress(key) if key.state.contains(KeyButMask::CONTROL) => {
                let map = conn.get_keyboard_mapping(key.detail, 1)?.reply()?;
                let sym = map.keysyms.first().copied().unwrap_or(0);
                if sym == u32::from(b'v') || sym == u32::from(b'V') {
                    if key.state.contains(KeyButMask::SHIFT) {
                        writeln!(events, "{}\tshifted-paste-rejected", millis())?;
                        events.flush()?;
                        continue;
                    }
                    let started = millis();
                    writeln!(events, "{started}\tpaste-key")?;
                    events.flush()?;
                    let reply = read(&conn, window, atom(&conn, target_name)?)?;
                    history.push(document.clone());
                    document.extend_from_slice(&reply.value);
                    fs::write(dir.join("document.bin"), &document)?;
                    count += 1;
                    fs::write(dir.join(format!("paste-{count}.bin")), &reply.value)?;
                    writeln!(
                        events,
                        "{}\tpaste\t{count}\t{target_name}\t{}",
                        millis(),
                        reply.value.len()
                    )?;
                    status = format!("Paste {count}: {} bytes ({target_name})", reply.value.len());
                    conn.clear_area(true, window, 0, 96, 900, 60)?;
                } else if sym == u32::from(b'z') || sym == u32::from(b'Z') {
                    document = history.pop().unwrap_or_default();
                    fs::write(dir.join("document.bin"), &document)?;
                    writeln!(events, "{}\tundo", millis())?;
                }
                events.flush()?;
                conn.flush()?;
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("app") if args.len() == 3 => app(&args[1], Path::new(&args[2])),
        Some("fixture") if args.len() == 2 => fixture(Path::new(&args[1])),
        Some("read") if (2..=3).contains(&args.len()) => {
            let (conn, screen) = x11rb::connect(None)?;
            let window = window(&conn, screen, EventMask::PROPERTY_CHANGE)?;
            let reply = read(&conn, window, atom(&conn, &args[1])?)?;
            let bytes = if args[1] == "TARGETS" {
                let mut lines = String::new();
                for id in reply.value32().ok_or("TARGETS is not format 32")? {
                    lines.push_str(&name(&conn, id)?);
                    lines.push('\n');
                }
                lines.into_bytes()
            } else {
                reply.value.clone()
            };
            eprintln!("type={} format={}", name(&conn, reply.type_)?, reply.format);
            if args.len() == 3 {
                fs::write(&args[2], bytes)?;
            } else {
                io::stdout().write_all(&bytes)?;
            }
            Ok(())
        }
        _ => {
            Err("usage: target app <WM_CLASS> <dir> | fixture <dir> | read <target> [file]".into())
        }
    }
}
