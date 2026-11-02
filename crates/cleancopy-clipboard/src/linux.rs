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

/// Lossless X11 clipboard data. Protocol targets are regenerated, not replayed.
/// Targets are sorted by their raw atom names when captured. No text aliases are added.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NativeSnapshot {
    pub targets: Vec<NativeTarget>,
}

/// One target's property, including its native type and element width.
/// Atom names need not be UTF-8. For formats 16/32, `data` uses host byte order
/// (as returned by x11rb); snapshots are intended for restoration on this host
/// and X server (embedded atom/window IDs are not translated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeTarget {
    pub target: Vec<u8>,
    pub type_name: Vec<u8>,
    pub format: u8,
    pub data: Vec<u8>,
}

fn protocol_target(name: &[u8]) -> bool {
    matches!(
        name,
        b"TARGETS"
            | b"MULTIPLE"
            | b"SAVE_TARGETS"
            | b"DELETE"
            | b"INSERT_SELECTION"
            | b"INSERT_PROPERTY"
            | b"TIMESTAMP"
    )
}

fn validate_target(target: &NativeTarget) -> Result<()> {
    if target.target.is_empty()
        || target.type_name.is_empty()
        || target.target.len() > usize::from(u16::MAX)
        || target.type_name.len() > usize::from(u16::MAX)
        || protocol_target(&target.target)
        || target.type_name == b"INCR"
        || !matches!(target.format, 8 | 16 | 32)
        || !target
            .data
            .len()
            .is_multiple_of(usize::from(target.format / 8))
    {
        return Err(error("invalid native X11 snapshot target"));
    }
    Ok(())
}

// X servers reset when their last client disconnects. Keep an independent client
// alive across test setup/teardown, and serialize tests sharing CLIPBOARD. This
// belongs to the test fixture: production must not suppress genuine X11 errors.
#[cfg(test)]
fn native_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static CONNECTION: std::sync::OnceLock<Inspector> = std::sync::OnceLock::new();
    let lock = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    CONNECTION.get_or_init(|| Inspector::new().unwrap());
    lock
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(format: u8, data: Vec<u8>) -> NativeTarget {
        NativeTarget {
            target: b"application/x-test".to_vec(),
            type_name: b"CUSTOM_TYPE".to_vec(),
            format,
            data,
        }
    }

    #[test]
    fn validates_native_targets_without_a_display() {
        for (format, width) in [(8, 1), (16, 2), (32, 4)] {
            assert!(validate_target(&target(format, vec![])).is_ok());
            assert!(validate_target(&target(format, vec![0; width])).is_ok());
            if width > 1 {
                assert!(validate_target(&target(format, vec![0; width - 1])).is_err());
            }
        }
        for format in [0, 1, 7, 24, 64] {
            assert!(validate_target(&target(format, vec![])).is_err());
        }
        for name in [
            b"TARGETS".as_slice(),
            b"MULTIPLE",
            b"TIMESTAMP",
            b"DELETE",
            b"SAVE_TARGETS",
            b"INSERT_SELECTION",
            b"INSERT_PROPERTY",
            b"",
        ] {
            let mut item = target(8, vec![]);
            item.target = name.to_vec();
            assert!(validate_target(&item).is_err());
        }
        let mut item = target(8, vec![0, 255]);
        item.target = vec![255, 128];
        item.type_name = vec![254];
        assert!(validate_target(&item).is_ok());
        for name in [b"INCR".as_slice(), b""] {
            item.type_name = name.to_vec();
            assert!(validate_target(&item).is_err());
        }
        let duplicate = target(8, vec![]);
        assert!(Owner::from_snapshot(&NativeSnapshot {
            targets: vec![duplicate.clone(), duplicate]
        })
        .is_err());
    }

    #[test]
    fn rejects_missing_partial_or_misaligned_properties() {
        use x11rb::protocol::xproto::GetPropertyReply;
        for (type_, format, bytes_after, value) in [
            (0, 8, 0, vec![]),
            (1, 0, 0, vec![]),
            (1, 16, 0, vec![0]),
            (1, 32, 0, vec![0; 3]),
            (1, 8, 4, vec![0]),
        ] {
            assert!(property_data(GetPropertyReply {
                type_,
                format,
                bytes_after,
                value,
                ..Default::default()
            })
            .is_err());
        }
        for format in [8, 16, 32] {
            let result = property_data(GetPropertyReply {
                type_: 1,
                format,
                value: vec![0; 4],
                ..Default::default()
            })
            .unwrap();
            assert_eq!(result.type_, 1);
            assert_eq!(result.format, format);
            assert_eq!(&*result.data, &[0; 4]);
        }
    }

    #[test]
    #[ignore = "requires an isolated X11 display; replaces clipboard contents"]
    fn snapshot_roundtrip_raw_targets_and_incr() {
        let _guard = native_test_guard();
        let clipboard = NativeClipboard::new().unwrap();
        assert_eq!(
            clipboard.read_snapshot().unwrap(),
            NativeSnapshot::default()
        );
        let mut snapshot = NativeSnapshot {
            targets: vec![
                target(8, vec![0, 255, 128]),
                NativeTarget {
                    target: vec![255, 128],
                    type_name: vec![254, 127],
                    format: 8,
                    data: vec![],
                },
            ],
        };
        for format in [8, 16, 32] {
            for len in [4, CHUNK_BYTES, CHUNK_BYTES + 4, CHUNK_BYTES * 2 + 12] {
                snapshot.targets.push(NativeTarget {
                    target: format!("application/x-raw-{format}-{len}").into_bytes(),
                    type_name: format!("TYPE_{format}").into_bytes(),
                    format,
                    data: (0..len).map(|i| (i % 251) as u8).collect(),
                });
            }
        }
        snapshot.targets.sort_by(|a, b| a.target.cmp(&b.target));
        clipboard.write_snapshot(&snapshot).unwrap();
        assert_eq!(clipboard.read_snapshot().unwrap(), snapshot);
        let mut invalid = snapshot.clone();
        invalid.targets.push(target(0, vec![]));
        assert!(clipboard.write_snapshot(&invalid).is_err());
        assert_eq!(clipboard.read_snapshot().unwrap(), snapshot);
        let replacement = NativeClipboard::new().unwrap();
        replacement
            .write_mode(&Payload::default(), WriteMode::Plain)
            .unwrap();
        drop(clipboard);
        replacement.write_snapshot(&snapshot).unwrap();
        assert_eq!(replacement.read_snapshot().unwrap(), snapshot);
        replacement
            .write_snapshot(&NativeSnapshot::default())
            .unwrap();
        assert_eq!(
            replacement.read_snapshot().unwrap(),
            NativeSnapshot::default()
        );
        let reader = NativeClipboard::new().unwrap();
        drop(replacement);
        assert_eq!(reader.read_snapshot().unwrap(), NativeSnapshot::default());
    }
}

#[cfg(test)]
mod retirement_tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated X11 display; replaces clipboard contents"]
    fn retired_owner_drains_routed_requests_and_incr() {
        let _guard = native_test_guard();
        for (len, stopped) in [
            (4, false),
            (4, true),
            (CHUNK_BYTES * 3 + 12, false),
            (CHUNK_BYTES * 3 + 12, true),
        ] {
            let payload = Payload {
                plain: "x".repeat(len),
                ..Default::default()
            };
            let mut owner = Owner::new(&payload, WriteMode::Plain).unwrap();
            let reader = Inspector::new().unwrap();
            let target = reader.atom(b"UTF8_STRING").unwrap();
            reader
                .conn
                .convert_selection(
                    reader.window,
                    reader.selection,
                    target,
                    reader.property,
                    CURRENT_TIME,
                )
                .unwrap();
            // Round trip guarantees the conversion was routed to the original owner.
            reader.owner().unwrap();
            if len > CHUNK_BYTES {
                // Start INCR before ownership changes, but leave its handshake pending.
                loop {
                    if let Event::SelectionRequest(request) = owner
                        .x
                        .event(Instant::now() + Duration::from_secs(3))
                        .unwrap()
                    {
                        owner.request(request).unwrap();
                        break;
                    }
                }
            }
            let replacement = Owner::new(&Payload::default(), WriteMode::Plain).unwrap();
            // The small request is still queued; the large one is an active INCR.
            let worker = thread::spawn(move || owner.run(&AtomicBool::new(stopped)));
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Event::SelectionNotify(event) = reader
                    .event(deadline)
                    .unwrap_or_else(|error| panic!("len={len}, stopped={stopped}: {error}"))
                {
                    assert_ne!(event.property, 0);
                    break;
                }
            }
            let reply = reader
                .conn
                .get_property(
                    true,
                    reader.window,
                    reader.property,
                    AtomEnum::ANY,
                    0,
                    u32::MAX,
                )
                .unwrap()
                .reply()
                .unwrap();
            let mut bytes = Vec::new();
            if reply.type_ == reader.incr {
                reader.conn.flush().unwrap();
                loop {
                    if let Event::PropertyNotify(event) = reader.event(deadline).unwrap() {
                        if event.atom != reader.property || event.state != Property::NEW_VALUE {
                            continue;
                        }
                        let part = reader
                            .conn
                            .get_property(
                                true,
                                reader.window,
                                reader.property,
                                AtomEnum::ANY,
                                0,
                                u32::MAX,
                            )
                            .unwrap()
                            .reply()
                            .unwrap();
                        reader.conn.flush().unwrap();
                        if part.type_ == 0 || part.type_ == reader.incr {
                            continue;
                        }
                        assert_eq!(part.type_, target);
                        assert_eq!(part.format, 8);
                        if part.value.is_empty() {
                            break;
                        }
                        bytes.extend(part.value);
                    }
                }
            } else {
                bytes = reply.value;
            }
            assert_eq!(bytes, payload.plain.as_bytes());
            worker.join().unwrap();
            drop(replacement);
        }
    }
}

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
        self.start_owner(Owner::new(payload, mode)?)
    }
    /// Capture every advertised data target, or fail without returning a partial snapshot.
    /// A missing selection owner is represented by an empty snapshot.
    pub fn read_snapshot(&self) -> Result<NativeSnapshot> {
        let reader = Inspector::new()?;
        let owner = reader.owner()?;
        if owner == 0 {
            return Ok(NativeSnapshot::default());
        }
        let targets_atom = reader.atom(b"TARGETS")?;
        let targets = reader.convert_property(targets_atom)?;
        if targets.type_ != AtomEnum::ATOM.into() || targets.format != 32 {
            return Err(error("malformed X11 TARGETS property: expected ATOM/32"));
        }
        let mut snapshot = NativeSnapshot::default();
        let mut seen = std::collections::HashSet::new();
        for bytes in targets.data.as_chunks::<4>().0 {
            let atom = u32::from_ne_bytes(*bytes);
            let name = reader.atom_name(atom)?;
            if protocol_target(&name) || !seen.insert(atom) {
                continue;
            }
            let property = reader.convert_property(atom).map_err(|cause| {
                error(format!(
                    "cannot snapshot X11 target {}: {cause}",
                    String::from_utf8_lossy(&name)
                ))
            })?;
            let target = NativeTarget {
                target: name,
                type_name: reader.atom_name(property.type_)?,
                format: property.format,
                data: property.data.to_vec(),
            };
            validate_target(&target)?;
            snapshot.targets.push(target);
            if reader.owner()? != owner {
                return Err(error("X11 clipboard owner changed during snapshot"));
            }
        }
        if reader.owner()? != owner {
            return Err(error("X11 clipboard owner changed during snapshot"));
        }
        snapshot.targets.sort_by(|a, b| a.target.cmp(&b.target));
        Ok(snapshot)
    }
    /// Restore exactly the captured data targets; does not synthesize text aliases.
    /// An empty snapshot owns the selection with no data targets; it does not clear ownership.
    /// Keep this handle alive while the restored clipboard must remain available.
    pub fn write_snapshot(&self, snapshot: &NativeSnapshot) -> Result<()> {
        self.start_owner(Owner::from_snapshot(snapshot)?)
    }
    fn start_owner(&self, owner: Owner) -> Result<()> {
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

/// INCR chunk size. Stays below the core X11 maximum request (256 KiB) with
/// room for the ChangeProperty header, so no BIG-REQUESTS extension is needed.
const CHUNK_BYTES: usize = 240 * 1024;

/// Short adaptive sleep for nonblocking X11 polling: INCR transfers exchange
/// one round trip per chunk, so a fixed 2 ms sleep dominated large reads.
#[derive(Default)]
struct Backoff(u32);
impl Backoff {
    fn wait(&mut self) {
        // 50 µs doubling to a 2 ms ceiling.
        let micros = (50u64 << self.0.min(6)).min(2000);
        self.0 = self.0.saturating_add(1);
        thread::sleep(Duration::from_micros(micros));
    }
}
#[derive(Clone)]
struct PropertyData {
    type_: Atom,
    format: u8,
    data: Arc<[u8]>,
}
impl PropertyData {
    fn write(&self, x: &Inspector, window: Window, property: Atom, data: &[u8]) -> Result<()> {
        x.conn
            .change_property(
                x11rb::protocol::xproto::PropMode::REPLACE,
                window,
                property,
                self.type_,
                self.format,
                u32::try_from(data.len() / usize::from(self.format / 8)).map_err(error)?,
                data,
            )
            .map_err(error)?
            .check()
            .map_err(error)
    }
}
struct Transfer {
    property: PropertyData,
    offset: usize,
    touched: Instant,
}
struct Owner {
    x: Inspector,
    targets: Atom,
    data: HashMap<Atom, PropertyData>,
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
        let data = data
            .into_iter()
            .map(|(atom, data)| {
                (
                    atom,
                    PropertyData {
                        type_: atom,
                        format: 8,
                        data,
                    },
                )
            })
            .collect();
        Self::claim(x, targets, data)
    }
    fn from_snapshot(snapshot: &NativeSnapshot) -> Result<Self> {
        // Validate everything before taking ownership, including duplicate names.
        let mut seen = std::collections::HashSet::new();
        for target in &snapshot.targets {
            validate_target(target)?;
            if !seen.insert(&target.target) {
                return Err(error("duplicate native X11 snapshot target"));
            }
        }
        let x = Inspector::new()?;
        let targets = x.atom(b"TARGETS")?;
        let mut data = HashMap::new();
        for target in &snapshot.targets {
            data.insert(
                x.atom(&target.target)?,
                PropertyData {
                    type_: x.atom(&target.type_name)?,
                    format: target.format,
                    data: Arc::from(target.data.as_slice()),
                },
            );
        }
        Self::claim(x, targets, data)
    }
    fn claim(x: Inspector, targets: Atom, data: HashMap<Atom, PropertyData>) -> Result<Self> {
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
                if data.data.len() <= CHUNK_BYTES {
                    data.write(&self.x, request.requestor, property, &data.data)?;
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
                            &[u32::try_from(data.data.len()).map_err(error)?],
                        )
                        .map_err(error)?
                        .check()
                        .map_err(error)?;
                    self.transfers.insert(
                        (request.requestor, property),
                        Transfer {
                            property: data.clone(),
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
            let end = (transfer.offset + CHUNK_BYTES).min(transfer.property.data.len());
            transfer.property.write(
                &self.x,
                window,
                property,
                &transfer.property.data[transfer.offset..end],
            )?;
            let done = transfer.offset == transfer.property.data.len();
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
        let mut retiring = None;
        let mut backoff = Backoff::default();
        let mut drain_synchronized = false;
        loop {
            if stop.load(Ordering::Acquire) && retiring.is_none() {
                // Synchronize with the server before draining: requests already routed
                // to this owner may still be in flight when its replacement is ready.
                if self.x.owner().is_err() {
                    break;
                }
                retiring = Some(Instant::now());
            }
            if retiring.is_some_and(|since: Instant| since.elapsed() >= Duration::from_secs(3)) {
                break;
            }
            let polled = self.x.conn.poll_for_event();
            if matches!(polled, Ok(Some(_))) {
                backoff = Backoff::default();
            }
            match polled {
                Ok(Some(Event::SelectionClear(event))) if event.selection == self.x.selection => {
                    // Previously accepted INCR transfers still belong to this connection.
                    retiring.get_or_insert_with(Instant::now);
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
                Ok(None) if retiring.is_some() && self.transfers.is_empty() => {
                    if drain_synchronized {
                        break;
                    }
                    // A nonblocking poll can consume a pending reply and return
                    // None while later SelectionRequest packets remain unread.
                    // A round trip pulls all pre-retirement events into x11rb's
                    // queue before an empty poll is allowed to finish draining.
                    if self.x.owner().is_err() {
                        break;
                    }
                    drain_synchronized = true;
                }
                Ok(None) => backoff.wait(),
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
        let mut backoff = Backoff::default();
        loop {
            if let Some(event) = self.conn.poll_for_event().map_err(error)? {
                return Ok(event);
            }
            if Instant::now() >= deadline {
                return Err(error("X11 clipboard conversion timed out"));
            }
            backoff.wait();
        }
    }
    fn atom_name(&self, atom: Atom) -> Result<Vec<u8>> {
        Ok(self
            .conn
            .get_atom_name(atom)
            .map_err(error)?
            .reply()
            .map_err(error)?
            .name)
    }
    fn convert(&self, target: Atom) -> Result<Vec<u8>> {
        Ok(self.convert_property(target)?.data.to_vec())
    }
    fn convert_property(&self, target: Atom) -> Result<PropertyData> {
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
                if event.requestor == self.window
                    && event.target == target
                    && event.selection == self.selection
                {
                    if event.property == 0 {
                        return Err(error("X11 owner refused clipboard target"));
                    }
                    if event.property != self.property {
                        return Err(error("X11 owner returned an unexpected clipboard property"));
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
            return property_data(reply);
        }
        if reply.format != 32 || reply.value.len() != 4 || reply.bytes_after != 0 {
            return Err(error("malformed X11 INCR announcement"));
        }
        self.conn.flush().map_err(error)?;
        let mut metadata = None;
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
                    let part = property_data(part)?;
                    let current = (part.type_, part.format);
                    if metadata.is_some_and(|previous| previous != current) {
                        return Err(error("X11 INCR property type or format changed"));
                    }
                    metadata = Some(current);
                    if part.data.is_empty() {
                        return Ok(PropertyData {
                            type_: part.type_,
                            format: part.format,
                            data: bytes.into(),
                        });
                    }
                    bytes.extend_from_slice(&part.data);
                    deadline = Instant::now() + Duration::from_secs(3);
                }
            }
        }
    }
}
fn property_data(reply: x11rb::protocol::xproto::GetPropertyReply) -> Result<PropertyData> {
    if reply.type_ == 0
        || !matches!(reply.format, 8 | 16 | 32)
        || reply.bytes_after != 0
        || !reply
            .value
            .len()
            .is_multiple_of(usize::from(reply.format / 8))
    {
        return Err(error("malformed or incomplete X11 clipboard property"));
    }
    Ok(PropertyData {
        type_: reply.type_,
        format: reply.format,
        data: reply.value.into(),
    })
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
