//! Independent X11 selection reader: x11_read TARGETS | x11_read text/html.
//! Does not use the cleancopy clipboard implementation.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        io::Write,
        thread,
        time::{Duration, Instant},
    };
    use x11rb::{
        connection::Connection,
        protocol::{
            xproto::{AtomEnum, ConnectionExt, CreateWindowAux, EventMask, Property, WindowClass},
            Event,
        },
        COPY_DEPTH_FROM_PARENT, CURRENT_TIME,
    };
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "UTF8_STRING".into());
    let (conn, screen) = x11rb::connect(None)?;
    let window = conn.generate_id()?;
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        window,
        conn.setup().roots[screen].root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?
    .check()?;
    let atom = |name: &[u8]| -> Result<u32, Box<dyn std::error::Error>> {
        Ok(conn.intern_atom(false, name)?.reply()?.atom)
    };
    let selection = atom(b"CLIPBOARD")?;
    let target = atom(name.as_bytes())?;
    let property = atom(b"X11_READ_RESULT")?;
    let incr = atom(b"INCR")?;
    if conn.get_selection_owner(selection)?.reply()?.owner == 0 {
        return Err("CLIPBOARD has no owner".into());
    }
    conn.convert_selection(window, selection, target, property, CURRENT_TIME)?;
    conn.flush()?;
    let mut deadline = Instant::now() + Duration::from_secs(5);
    let mut incremental = false;
    let mut notified = false;
    let mut output = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return Err("selection conversion timed out".into());
        }
        let Some(event) = conn.poll_for_event()? else {
            thread::sleep(Duration::from_millis(2));
            continue;
        };
        let read = match event {
            Event::SelectionNotify(event)
                if event.requestor == window && event.target == target =>
            {
                if event.property == 0 {
                    return Err("owner refused target".into());
                }
                notified = true;
                true
            }
            Event::PropertyNotify(event)
                if notified
                    && incremental
                    && event.window == window
                    && event.atom == property
                    && event.state == Property::NEW_VALUE =>
            {
                true
            }
            _ => false,
        };
        if !read {
            continue;
        }
        let reply = conn
            .get_property(true, window, property, AtomEnum::ANY, 0, u32::MAX)?
            .reply()?;
        conn.flush()?;
        if reply.type_ == incr {
            incremental = true;
        } else if reply.type_ != 0 {
            let done = !incremental || reply.value.is_empty();
            output.extend(reply.value);
            if done {
                break;
            }
        }
        deadline = Instant::now() + Duration::from_secs(5);
    }
    let mut stdout = std::io::stdout().lock();
    if name == "TARGETS" {
        if output.len() % 4 != 0 {
            return Err("invalid TARGETS length".into());
        }
        for bytes in output.as_chunks::<4>().0 {
            let atom = u32::from_ne_bytes(*bytes);
            stdout.write_all(&conn.get_atom_name(atom)?.reply()?.name)?;
            stdout.write_all(b"\n")?;
        }
    } else {
        stdout.write_all(&output)?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("x11_read requires Linux X11/XWayland");
    std::process::exit(1);
}
