use crate::{error, ClipboardFormat, Payload, Result, WriteMode};
use std::{
    cell::RefCell,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            Atom, AtomEnum, ConnectionExt, CreateWindowAux, EventMask, Property, Window,
            WindowClass,
        },
        Event,
    },
    rust_connection::RustConnection,
    COPY_DEPTH_FROM_PARENT, CURRENT_TIME,
};

pub struct NativeClipboard {
    clipboard: RefCell<arboard::Clipboard>,
}
impl NativeClipboard {
    pub fn new() -> Result<Self> {
        Ok(Self {
            clipboard: RefCell::new(arboard::Clipboard::new().map_err(error)?),
        })
    }
    pub fn write_mode(&self, payload: &Payload, mode: WriteMode) -> Result<()> {
        let mut clipboard = self.clipboard.borrow_mut();
        match mode {
            WriteMode::Multi | WriteMode::Html => {
                clipboard.set_html(payload.html.clone(), Some(payload.plain.clone()))
            }
            WriteMode::Plain => clipboard.set_text(payload.plain.clone()),
            WriteMode::Markdown => clipboard.set_text(payload.markdown.clone()),
        }
        .map_err(error)
    }
    pub(crate) fn read_native_text(&self) -> Result<String> {
        self.clipboard.borrow_mut().get_text().map_err(error)
    }
    /// Query the real X11 TARGETS, then request each target's native bytes.
    pub fn list_formats(&self) -> Result<Vec<ClipboardFormat>> {
        inspect()
    }
}

struct Inspector {
    conn: RustConnection,
    window: Window,
    selection: Atom,
    property: Atom,
    incr: Atom,
}
impl Inspector {
    fn new() -> Result<Self> {
        let (conn, screen) = x11rb::connect(None).map_err(error)?;
        let window = conn.generate_id().map_err(error)?;
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
        )
        .map_err(error)?
        .check()
        .map_err(error)?;
        let atom = |name: &[u8]| -> Result<Atom> {
            Ok(conn
                .intern_atom(false, name)
                .map_err(error)?
                .reply()
                .map_err(error)?
                .atom)
        };
        let selection = atom(b"CLIPBOARD")?;
        let property = atom(b"CLEANCOPY_INSPECT")?;
        let incr = atom(b"INCR")?;
        Ok(Self {
            conn,
            window,
            selection,
            property,
            incr,
        })
    }
    fn owner(&self) -> Result<Window> {
        Ok(self
            .conn
            .get_selection_owner(self.selection)
            .map_err(error)?
            .reply()
            .map_err(error)?
            .owner)
    }
    fn event(&self, deadline: Instant) -> Result<Event> {
        loop {
            if let Some(event) = self.conn.poll_for_event().map_err(error)? {
                return Ok(event);
            }
            if Instant::now() >= deadline {
                return Err(error("X11 clipboard conversion timed out"));
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn convert(&self, target: Atom) -> Result<Vec<u8>> {
        self.conn
            .delete_property(self.window, self.property)
            .map_err(error)?;
        self.conn
            .convert_selection(
                self.window,
                self.selection,
                target,
                self.property,
                CURRENT_TIME,
            )
            .map_err(error)?;
        self.conn.flush().map_err(error)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Event::SelectionNotify(event) = self.event(deadline)? {
                if event.requestor == self.window && event.target == target {
                    if event.property == 0 {
                        return Err(error("X11 owner refused clipboard target"));
                    }
                    break;
                }
            }
        }
        let reply = self
            .conn
            .get_property(true, self.window, self.property, AtomEnum::ANY, 0, u32::MAX)
            .map_err(error)?
            .reply()
            .map_err(error)?;
        if reply.type_ != self.incr {
            return Ok(reply.value);
        }
        self.conn.flush().map_err(error)?;
        let mut bytes = Vec::new();
        loop {
            if let Event::PropertyNotify(event) = self.event(deadline)? {
                if event.window == self.window
                    && event.atom == self.property
                    && event.state == Property::NEW_VALUE
                {
                    let part = self
                        .conn
                        .get_property(true, self.window, self.property, AtomEnum::ANY, 0, u32::MAX)
                        .map_err(error)?
                        .reply()
                        .map_err(error)?;
                    self.conn.flush().map_err(error)?;
                    // The initial INCR announcement can have a queued notification.
                    if part.type_ == self.incr || part.type_ == 0 {
                        continue;
                    }
                    if part.value.is_empty() {
                        return Ok(bytes);
                    }
                    bytes.extend(part.value);
                }
            }
        }
    }
}
impl Drop for Inspector {
    fn drop(&mut self) {
        let _ = self.conn.destroy_window(self.window);
        let _ = self.conn.flush();
    }
}
fn inspect() -> Result<Vec<ClipboardFormat>> {
    let inspector = Inspector::new()?;
    if inspector.owner()? == 0 {
        return Ok(Vec::new());
    }
    let targets = inspector
        .conn
        .intern_atom(false, b"TARGETS")
        .map_err(error)?
        .reply()
        .map_err(error)?
        .atom;
    let raw = inspector.convert(targets)?;
    let mut formats = Vec::new();
    // x11rb's connection byte order is the host's native byte order.
    for bytes in raw.as_chunks::<4>().0 {
        let atom = u32::from_ne_bytes(*bytes);
        let name = String::from_utf8_lossy(
            &inspector
                .conn
                .get_atom_name(atom)
                .map_err(error)?
                .reply()
                .map_err(error)?
                .name,
        )
        .into_owned();
        // Protocol control targets are not clipboard data and some have side effects.
        if matches!(
            name.as_str(),
            "MULTIPLE" | "SAVE_TARGETS" | "DELETE" | "INSERT_SELECTION" | "INSERT_PROPERTY"
        ) {
            continue;
        }
        let bytes = if atom == targets {
            raw.len()
        } else {
            match inspector.convert(atom) {
                Ok(data) => data.len(),
                // arboard 3.6.1 advertises these aliases but its X11 request
                // handler only matches the stored UTF8_STRING atom.
                Err(_)
                    if matches!(
                        name.as_str(),
                        "text/plain" | "text/plain;charset=utf-8" | "text/plain;charset=UTF-8"
                    ) =>
                {
                    let utf8 = inspector
                        .conn
                        .intern_atom(false, b"UTF8_STRING")
                        .map_err(error)?
                        .reply()
                        .map_err(error)?
                        .atom;
                    inspector.convert(utf8)?.len()
                }
                Err(cause) => return Err(error(format!("cannot read X11 target {name}: {cause}"))),
            }
        };
        formats.push(ClipboardFormat { name, bytes });
    }
    formats.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(formats)
}

/// Re-exec the CLI rather than forking a process which may already have threads.
/// The executable must dispatch `--clipboard-holder` to [`run_holder`].
pub fn spawn_holder(executable: &Path, payload: &Payload, mode: WriteMode) -> Result<()> {
    for value in [&payload.html, &payload.plain, &payload.markdown] {
        if value.len() > 256 * 1024 * 1024 {
            return Err(error("holder payload exceeds 256 MiB per format"));
        }
    }
    let mut child = Command::new(executable)
        .arg("--clipboard-holder")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(error)?;
    let result = (|| {
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| error("missing holder stdin"))?;
        input
            .write_all(&[match mode {
                WriteMode::Multi => 0,
                WriteMode::Plain => 1,
                WriteMode::Html => 2,
                WriteMode::Markdown => 3,
            }])
            .map_err(error)?;
        for value in [&payload.html, &payload.plain, &payload.markdown] {
            input
                .write_all(&(value.len() as u64).to_le_bytes())
                .map_err(error)?;
            input.write_all(value.as_bytes()).map_err(error)?;
        }
        drop(input);
        let mut output = child
            .stdout
            .take()
            .ok_or_else(|| error("missing holder stdout"))?;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut ready = [0];
            let result = output.read_exact(&mut ready).map(|()| ready).map_err(error);
            let _ = tx.send(result);
        });
        let ready = rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| error(format!("clipboard holder readiness failed: {e}")))??;
        if ready != [1] {
            return Err(error("clipboard holder rejected payload"));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    // Dropping Child does not terminate it; it lives until ownership changes.
    result
}

/// Hidden child entry point. Signals readiness only after X11 owns the selection.
pub fn run_holder() -> Result<()> {
    let mut input = std::io::stdin().lock();
    let mut mode = [0];
    input.read_exact(&mut mode).map_err(error)?;
    let mode = match mode[0] {
        0 => WriteMode::Multi,
        1 => WriteMode::Plain,
        2 => WriteMode::Html,
        3 => WriteMode::Markdown,
        _ => return Err(error("invalid holder mode")),
    };
    let mut read_string = || -> Result<String> {
        let mut len = [0; 8];
        input.read_exact(&mut len).map_err(error)?;
        let len = usize::try_from(u64::from_le_bytes(len)).map_err(error)?;
        if len > 256 * 1024 * 1024 {
            return Err(error("holder payload exceeds 256 MiB per format"));
        }
        let mut bytes = vec![0; len];
        input.read_exact(&mut bytes).map_err(error)?;
        String::from_utf8(bytes).map_err(error)
    };
    let payload = Payload {
        html: read_string()?,
        plain: read_string()?,
        markdown: read_string()?,
    };
    let clipboard = NativeClipboard::new()?;
    let inspector = Inspector::new()?;
    let previous_owner = inspector.owner()?;
    clipboard.write_mode(&payload, mode)?;
    // The writer and inspector use different X connections; flush alone does
    // not guarantee that the server processed SetSelectionOwner yet.
    let deadline = Instant::now() + Duration::from_secs(3);
    let owner = loop {
        let owner = inspector.owner()?;
        if owner != 0 && owner != previous_owner {
            break owner;
        }
        if Instant::now() >= deadline {
            return Err(error("clipboard holder did not acquire ownership"));
        }
        thread::sleep(Duration::from_millis(2));
    };
    {
        let mut output = std::io::stdout().lock();
        output.write_all(&[1]).map_err(error)?;
        output.flush().map_err(error)?;
    }
    // Arboard serves data on its own thread. Observe ownership without another set,
    // which would create a race between the readiness signal and replacement.
    while inspector.owner()? == owner {
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
