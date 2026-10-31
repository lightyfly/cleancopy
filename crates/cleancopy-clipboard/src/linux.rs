use crate::{error, ClipboardFormat, Payload, Result, WriteMode};
use std::{
    cell::RefCell,
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt, CreateWindowAux, EventMask,
            Property, SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
            SELECTION_NOTIFY_EVENT,
        },
        Event,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
    COPY_DEPTH_FROM_PARENT, CURRENT_TIME,
};

// A connection and its event loop belong to exactly one thread. Dropping the
// handle stops that loop and closes the connection (releasing the selection).
pub struct NativeClipboard {
    owner: RefCell<Option<OwnerThread>>,
}
struct OwnerThread {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Drop for OwnerThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl NativeClipboard {
    pub fn new() -> Result<Self> {
        // Fail at construction when no X11/XWayland display is available.
        Inspector::new()?;
        Ok(Self {
            owner: RefCell::new(None),
        })
    }
    pub fn write_mode(&self, payload: &Payload, mode: WriteMode) -> Result<()> {
        let owner = Owner::new(payload, mode)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("cleancopy-x11".into())
            .spawn(move || owner.run(&flag))
            .map_err(error)?;
        *self.owner.borrow_mut() = Some(OwnerThread {
            stop,
            thread: Some(thread),
        });
        Ok(())
    }
    pub(crate) fn read_native_text(&self) -> Result<String> {
        let reader = Inspector::new()?;
        if reader.owner()? == 0 {
            return Err(error("X11 clipboard has no owner"));
        }
        for name in [
            "UTF8_STRING",
            "text/plain;charset=utf-8",
            "text/plain",
            "TEXT",
            "STRING",
        ] {
            let target = reader.atom(name.as_bytes())?;
            if let Ok(bytes) = reader.convert(target) {
                // ICCCM STRING is ISO-8859-1, not UTF-8.
                return if name == "STRING" {
                    Ok(bytes.into_iter().map(char::from).collect())
                } else {
                    String::from_utf8(bytes).map_err(error)
                };
            }
        }
        Err(error("X11 clipboard has no readable text target"))
    }
    pub fn list_formats(&self) -> Result<Vec<ClipboardFormat>> {
        inspect()
    }
}

const CHUNK_BYTES: usize = 64 * 1024;
struct Transfer {
    target: Atom,
    data: Arc<[u8]>,
    offset: usize,
    touched: Instant,
}
struct Owner {
    x: Inspector,
    targets: Atom,
    data: HashMap<Atom, Arc<[u8]>>,
    transfers: HashMap<(Window, Atom), Transfer>,
}
impl Owner {
    fn new(payload: &Payload, mode: WriteMode) -> Result<Self> {
        let x = Inspector::new()?;
        let targets = x.atom(b"TARGETS")?;
        let mut data = HashMap::new();
        let text: Arc<[u8]> = Arc::from(if mode == WriteMode::Markdown {
            payload.markdown.as_bytes()
        } else {
            payload.plain.as_bytes()
        });
        for name in [
            "UTF8_STRING",
            "text/plain;charset=utf-8",
            "text/plain;charset=UTF-8",
            "text/plain",
            "TEXT",
        ] {
            data.insert(x.atom(name.as_bytes())?, Arc::clone(&text));
        }
        if matches!(mode, WriteMode::Multi | WriteMode::Html) {
            data.insert(x.atom(b"text/html")?, Arc::from(payload.html.as_bytes()));
        }
        if mode == WriteMode::Multi {
            data.insert(
                x.atom(b"text/markdown")?,
                Arc::from(payload.markdown.as_bytes()),
            );
        }
        x.conn
            .set_selection_owner(x.window, x.selection, CURRENT_TIME)
            .map_err(error)?
            .check()
            .map_err(error)?;
        // This round trip is the readiness barrier, on the writer connection.
        if x.owner()? != x.window {
            return Err(error(
                "X11 clipboard ownership was replaced before readiness",
            ));
        }
        Ok(Self {
            x,
            targets,
            data,
            transfers: HashMap::new(),
        })
    }
    fn request(&mut self, request: SelectionRequestEvent) -> Result<()> {
        let property = if request.property == 0 {
            request.target
        } else {
            request.property
        };
        let mut accepted = false;
        if request.selection == self.x.selection {
            if request.target == self.targets {
                let mut targets: Vec<_> = self.data.keys().copied().collect();
                targets.push(self.targets);
                targets.sort_unstable();
                self.x
                    .conn
                    .change_property32(
                        x11rb::protocol::xproto::PropMode::REPLACE,
                        request.requestor,
                        property,
                        AtomEnum::ATOM,
                        &targets,
                    )
                    .map_err(error)?
                    .check()
                    .map_err(error)?;
                accepted = true;
            } else if let Some(data) = self.data.get(&request.target) {
                if data.len() <= CHUNK_BYTES {
                    self.x
                        .conn
                        .change_property8(
                            x11rb::protocol::xproto::PropMode::REPLACE,
                            request.requestor,
                            property,
                            request.target,
                            data,
                        )
                        .map_err(error)?
                        .check()
                        .map_err(error)?;
                } else {
                    // Subscribe before announcing INCR so no delete acknowledgement is lost.
                    self.x
                        .conn
                        .change_window_attributes(
                            request.requestor,
                            &ChangeWindowAttributesAux::new()
                                .event_mask(EventMask::PROPERTY_CHANGE),
                        )
                        .map_err(error)?
                        .check()
                        .map_err(error)?;
                    self.x
                        .conn
                        .change_property32(
                            x11rb::protocol::xproto::PropMode::REPLACE,
                            request.requestor,
                            property,
                            self.x.incr,
                            &[u32::try_from(data.len()).map_err(error)?],
                        )
                        .map_err(error)?
                        .check()
                        .map_err(error)?;
                    self.transfers.insert(
                        (request.requestor, property),
                        Transfer {
                            target: request.target,
                            data: Arc::clone(data),
                            offset: 0,
                            touched: Instant::now(),
                        },
                    );
                }
                accepted = true;
            }
        }
        self.x
            .conn
            .send_event(
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
            )
            .map_err(error)?;
        self.x.conn.flush().map_err(error)
    }
    fn chunk(&mut self, window: Window, property: Atom) -> Result<()> {
        let key = (window, property);
        if let Some(transfer) = self.transfers.get_mut(&key) {
            let end = (transfer.offset + CHUNK_BYTES).min(transfer.data.len());
            self.x
                .conn
                .change_property8(
                    x11rb::protocol::xproto::PropMode::REPLACE,
                    window,
                    property,
                    transfer.target,
                    &transfer.data[transfer.offset..end],
                )
                .map_err(error)?
                .check()
                .map_err(error)?;
            let done = transfer.offset == transfer.data.len();
            transfer.offset = end;
            transfer.touched = Instant::now();
            if done {
                self.transfers.remove(&key);
            }
            self.x.conn.flush().map_err(error)?;
        }
        Ok(())
    }
    fn run(mut self, stop: &AtomicBool) {
        while !stop.load(Ordering::Acquire) {
            match self.x.conn.poll_for_event() {
                Ok(Some(Event::SelectionClear(event))) if event.selection == self.x.selection => {
                    break;
                }
                Ok(Some(Event::SelectionRequest(event))) => {
                    // A disappearing requestor must not take down the clipboard owner.
                    let _ = self.request(event);
                }
                Ok(Some(Event::PropertyNotify(event))) if event.state == Property::DELETE => {
                    if self.chunk(event.window, event.atom).is_err() {
                        self.transfers.remove(&(event.window, event.atom));
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => thread::sleep(Duration::from_millis(2)),
                Err(_) => break,
            }
            self.transfers
                .retain(|_, t| t.touched.elapsed() < Duration::from_secs(30));
        }
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
    fn atom(&self, name: &[u8]) -> Result<Atom> {
        Ok(self
            .conn
            .intern_atom(false, name)
            .map_err(error)?
            .reply()
            .map_err(error)?
            .atom)
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
        let mut deadline = Instant::now() + Duration::from_secs(3);
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
                    deadline = Instant::now() + Duration::from_secs(3);
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
    if raw.len() % 4 != 0 {
        return Err(error("malformed X11 TARGETS property"));
    }
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
    clipboard.write_mode(&payload, mode)?;
    {
        let mut output = std::io::stdout().lock();
        output.write_all(&[1]).map_err(error)?;
        output.flush().map_err(error)?;
    }
    // SelectionClear terminates the owner's event thread after replacement.
    while clipboard.owner.borrow().as_ref().is_some_and(|owner| {
        owner
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
    }) {
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
