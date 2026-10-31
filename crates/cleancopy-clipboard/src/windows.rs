use crate::{cf_html, error, ClipboardFormat, Payload, Result, WriteMode};
use std::{marker::PhantomData, ptr, rc::Rc, thread, time::Duration};
use windows_sys::Win32::{
    Foundation::{GetLastError, GlobalFree, SetLastError, HGLOBAL, HWND},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
            GetClipboardFormatNameW, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
        },
        Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT},
    },
    UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE},
};

const CF_UNICODETEXT: u32 = 13;

/// Clipboard access with a non-null, thread-affine message-only owner window.
/// All data is eagerly rendered, so no window message loop is required.
#[derive(Debug)]
pub struct NativeClipboard {
    owner: HWND,
    // Windows must be used and destroyed on the thread that created them.
    _thread_affine: PhantomData<Rc<()>>,
}

impl NativeClipboard {
    pub fn new() -> Result<Self> {
        // SAFETY: STATIC is a predefined system class; strings are NUL terminated.
        // A message-only window is invisible and valid as a clipboard owner.
        let owner = unsafe {
            CreateWindowExW(
                0,
                windows_sys::core::w!("STATIC"),
                windows_sys::core::w!("CleanCopy"),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if owner.is_null() {
            return Err(last_error("CreateWindowExW"));
        }
        Ok(Self {
            owner,
            _thread_affine: PhantomData,
        })
    }

    pub fn write_mode(&self, payload: &Payload, mode: WriteMode) -> Result<()> {
        // Prepare every allocation and registered format before clearing existing
        // content. Failed SetClipboardData calls cannot roll back a native write.
        let text = if mode == WriteMode::Markdown {
            &payload.markdown
        } else {
            &payload.plain
        };
        let mut formats = vec![(CF_UNICODETEXT, GlobalMemory::new(&unicode_text(text))?)];
        if matches!(mode, WriteMode::Multi | WriteMode::Html) {
            let html = cf_html::encode(&payload.html, None)?;
            formats.push((
                register("HTML Format")?,
                GlobalMemory::new(&nul_terminated(&html))?,
            ));
        }
        if mode == WriteMode::Multi {
            formats.push((
                register("text/markdown")?,
                GlobalMemory::new(&nul_terminated(&crlf(&payload.markdown)))?,
            ));
        }
        let _clipboard = ClipboardGuard::open(self.owner)?;
        // SAFETY: the clipboard is open with our valid, non-null owner window.
        if unsafe { EmptyClipboard() } == 0 {
            return Err(last_error("EmptyClipboard"));
        }
        for (format, mut allocation) in formats {
            // SAFETY: allocation is unlocked, movable global memory. On success
            // the system takes ownership; on failure our RAII guard frees it.
            if unsafe { SetClipboardData(format, allocation.0) }.is_null() {
                return Err(last_error("SetClipboardData"));
            }
            allocation.0 = ptr::null_mut();
        }
        Ok(())
    }

    pub(crate) fn read_native_text(&self) -> Result<String> {
        let _clipboard = ClipboardGuard::open(self.owner)?;
        // SAFETY: clipboard is open; borrowed data stays valid until close.
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
        if handle.is_null() {
            return Err(last_error("GetClipboardData(CF_UNICODETEXT)"));
        }
        // SAFETY: CF_UNICODETEXT is global memory, not a GDI handle.
        let bytes = unsafe { GlobalSize(handle) };
        if bytes < 2 || bytes % 2 != 0 {
            return Err(error("invalid CF_UNICODETEXT allocation size"));
        }
        let locked = LockedMemory::new(handle)?;
        // SAFETY: GlobalLock provides aligned memory, bounded by GlobalSize.
        let units = unsafe { std::slice::from_raw_parts(locked.pointer.cast::<u16>(), bytes / 2) };
        let end = units
            .iter()
            .position(|&unit| unit == 0)
            .ok_or_else(|| error("CF_UNICODETEXT is not NUL terminated"))?;
        String::from_utf16(&units[..end]).map_err(error)
    }

    pub fn list_formats(&self) -> Result<Vec<ClipboardFormat>> {
        let _clipboard = ClipboardGuard::open(self.owner)?;
        let mut formats = Vec::new();
        let mut previous = 0;
        loop {
            // SAFETY: open clipboard; reset last error to distinguish end-of-list.
            unsafe {
                SetLastError(0);
            }
            let format = unsafe { EnumClipboardFormats(previous) };
            if format == 0 {
                if unsafe { GetLastError() } != 0 {
                    return Err(last_error("EnumClipboardFormats"));
                }
                break;
            }
            previous = format;
            let name = format_name(format);
            // Some predefined formats contain GDI/private handles rather than
            // HGLOBALs. Do not pass such handles to GlobalSize. Their byte size
            // is unavailable through the clipboard API and is reported as zero.
            let bytes = if is_global_memory_format(format) {
                let handle = unsafe { GetClipboardData(format) };
                if handle.is_null() {
                    return Err(last_error("GetClipboardData"));
                }
                unsafe { GlobalSize(handle) }
            } else {
                0
            };
            formats.push(ClipboardFormat { name, bytes });
        }
        Ok(formats)
    }
}

impl Drop for NativeClipboard {
    fn drop(&mut self) {
        // SAFETY: our window is thread-affine and no clipboard guard remains.
        unsafe {
            DestroyWindow(self.owner);
        }
    }
}

struct ClipboardGuard;
impl ClipboardGuard {
    fn open(owner: HWND) -> Result<Self> {
        for attempt in 0..10 {
            // SAFETY: owner remains live for the duration of this guard.
            if unsafe { OpenClipboard(owner) } != 0 {
                return Ok(Self);
            }
            let failure = last_error("OpenClipboard (10 attempts, 20ms apart)");
            if attempt == 9 {
                return Err(failure);
            }
            thread::sleep(Duration::from_millis(20));
        }
        unreachable!()
    }
}
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: created only following a successful OpenClipboard.
        unsafe {
            CloseClipboard();
        }
    }
}

struct GlobalMemory(HGLOBAL);
impl GlobalMemory {
    fn new(bytes: &[u8]) -> Result<Self> {
        // SAFETY: clipboard data must use moveable global memory.
        let memory = Self(unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()) });
        if memory.0.is_null() {
            return Err(last_error("GlobalAlloc"));
        }
        let locked = LockedMemory::new(memory.0)?;
        // SAFETY: allocation is locked and exactly bytes.len() bytes or larger.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), locked.pointer.cast(), bytes.len());
        }
        drop(locked);
        Ok(memory)
    }
}
impl Drop for GlobalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: only untransferred, unlocked allocations remain non-null.
            unsafe {
                GlobalFree(self.0);
            }
        }
    }
}
struct LockedMemory {
    handle: HGLOBAL,
    pointer: *mut std::ffi::c_void,
}
impl LockedMemory {
    fn new(handle: HGLOBAL) -> Result<Self> {
        // SAFETY: caller supplies an HGLOBAL while its owner remains alive.
        let pointer = unsafe { GlobalLock(handle) };
        if pointer.is_null() {
            return Err(last_error("GlobalLock"));
        }
        Ok(Self { handle, pointer })
    }
}
impl Drop for LockedMemory {
    fn drop(&mut self) {
        // SAFETY: balances this guard's successful GlobalLock; memory is still owned.
        unsafe {
            GlobalUnlock(self.handle);
        }
    }
}

fn last_error(operation: &str) -> crate::ClipboardError {
    error(format!("{operation}: {}", std::io::Error::last_os_error()))
}
fn register(name: &str) -> Result<u32> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: wide is a live, NUL-terminated string.
    let format = unsafe { RegisterClipboardFormatW(wide.as_ptr()) };
    if format == 0 {
        Err(last_error("RegisterClipboardFormatW"))
    } else {
        Ok(format)
    }
}
fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}
fn unicode_text(text: &str) -> Vec<u8> {
    crlf(text)
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}
fn nul_terminated(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}
fn is_global_memory_format(format: u32) -> bool {
    matches!(format, 1 | 4..=8 | 10..=13 | 15..=17 | 0x81 | 0xc000..=0xffff)
}
fn format_name(format: u32) -> String {
    let standard = match format {
        1 => "CF_TEXT",
        2 => "CF_BITMAP",
        3 => "CF_METAFILEPICT",
        4 => "CF_SYLK",
        5 => "CF_DIF",
        6 => "CF_TIFF",
        7 => "CF_OEMTEXT",
        8 => "CF_DIB",
        9 => "CF_PALETTE",
        10 => "CF_PENDATA",
        11 => "CF_RIFF",
        12 => "CF_WAVE",
        13 => "CF_UNICODETEXT",
        14 => "CF_ENHMETAFILE",
        15 => "CF_HDROP",
        16 => "CF_LOCALE",
        17 => "CF_DIBV5",
        0x80 => "CF_OWNERDISPLAY",
        0x81 => "CF_DSPTEXT",
        0x82 => "CF_DSPBITMAP",
        0x83 => "CF_DSPMETAFILEPICT",
        0x8e => "CF_DSPENHMETAFILE",
        _ => "",
    };
    if !standard.is_empty() {
        return standard.to_owned();
    }
    let mut name = [0u16; 256];
    // SAFETY: name is writable for exactly the specified capacity.
    let count = unsafe { GetClipboardFormatNameW(format, name.as_mut_ptr(), name.len() as i32) };
    if count > 0 {
        String::from_utf16_lossy(&name[..count as usize])
    } else {
        format!("format/{format}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_crlf_and_terminator() {
        let text = "中文 🦀\na\r\nb\rc";
        let bytes = unicode_text(text);
        let units: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(units.last(), Some(&0));
        assert_eq!(
            String::from_utf16(&units[..units.len() - 1]).unwrap(),
            "中文 🦀\r\na\r\nb\r\nc"
        );
    }
}
