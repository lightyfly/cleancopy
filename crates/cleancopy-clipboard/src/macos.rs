use crate::{error, ClipboardFormat, Payload, Result, WriteMode};
use objc2::rc::{autoreleasepool, Retained};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeString};
use objc2_foundation::NSString;

pub struct NativeClipboard {
    pasteboard: Retained<NSPasteboard>,
}
impl NativeClipboard {
    pub fn new() -> Result<Self> {
        Ok(Self {
            pasteboard: NSPasteboard::generalPasteboard(),
        })
    }
    pub fn write_mode(&self, payload: &Payload, mode: WriteMode) -> Result<()> {
        autoreleasepool(|_| {
            let markdown_type = NSString::from_str("net.daringfireball.markdown");
            // Clear once only: a subsequent clear would discard earlier representations.
            self.pasteboard.clearContents();
            let set = |value: &str, kind: &NSString| -> Result<()> {
                let value = NSString::from_str(value);
                if self.pasteboard.setString_forType(&value, kind) {
                    Ok(())
                } else {
                    Err(error(format!("NSPasteboard rejected {}", kind)))
                }
            };
            match mode {
                WriteMode::Multi => {
                    set(&payload.html, unsafe { NSPasteboardTypeHTML })?;
                    set(&payload.plain, unsafe { NSPasteboardTypeString })?;
                    set(&payload.markdown, &markdown_type)
                }
                WriteMode::Html => {
                    set(&payload.html, unsafe { NSPasteboardTypeHTML })?;
                    set(&payload.plain, unsafe { NSPasteboardTypeString })
                }
                WriteMode::Plain => set(&payload.plain, unsafe { NSPasteboardTypeString }),
                // Markdown editors actually consume the plain text channel.
                WriteMode::Markdown => set(&payload.markdown, unsafe { NSPasteboardTypeString }),
            }
        })
    }
    pub(crate) fn read_native_text(&self) -> Result<String> {
        autoreleasepool(|_| {
            self.pasteboard
                .stringForType(unsafe { NSPasteboardTypeString })
                .map(|value| value.to_string())
                .ok_or_else(|| error("clipboard has no UTF-8 plain text"))
        })
    }
    pub fn list_formats(&self) -> Result<Vec<ClipboardFormat>> {
        autoreleasepool(|_| {
            let mut result = Vec::new();
            if let Some(types) = self.pasteboard.types() {
                for index in 0..types.count() {
                    let kind = types.objectAtIndex(index);
                    let data = self
                        .pasteboard
                        .dataForType(&kind)
                        .ok_or_else(|| error(format!("cannot read pasteboard type {kind}")))?;
                    result.push(ClipboardFormat {
                        name: kind.to_string(),
                        bytes: data.len(),
                    });
                }
            }
            result.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(result)
        })
    }
}
