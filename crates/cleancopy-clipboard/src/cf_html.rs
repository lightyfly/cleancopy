//! Windows HTML Clipboard Format serialization, available on every platform.
//!
//! Offsets address UTF-8 bytes from the beginning of the ASCII header. The
//! returned string excludes the terminating NUL required by clipboard storage.
use crate::{error, Result};

const START: &str = "<!--StartFragment-->";
const END: &str = "<!--EndFragment-->";

/// Encode a complete HTML document (or an HTML fragment) as CF_HTML.
///
/// Existing fragment markers are respected. Otherwise markers surround the
/// body's contents, or the entire input when it has no body element. SourceURL
/// must be ASCII and cannot contain line breaks or NULs (header injection).
pub fn encode(html: &str, source_url: Option<&str>) -> Result<String> {
    if source_url
        .is_some_and(|url| !url.is_ascii() || url.bytes().any(|b| matches!(b, b'\r' | b'\n' | 0)))
    {
        return Err(error(
            "CF_HTML SourceURL must be ASCII without line breaks or NULs",
        ));
    }
    let document = if let Some(start) = html.find(START) {
        if !html[start + START.len()..].contains(END) {
            return Err(error("CF_HTML has an unmatched fragment marker"));
        }
        html.to_owned()
    } else {
        let lower = html.to_ascii_lowercase();
        let body_start = lower.match_indices("<body").find_map(|(index, _)| {
            let suffix = &lower[index + 5..];
            if suffix.starts_with('>') || suffix.starts_with(char::is_whitespace) {
                lower[index..].find('>').map(|end| index + end + 1)
            } else {
                None
            }
        });
        if let Some((start, end)) = body_start
            .zip(lower.rfind("</body>"))
            .filter(|(s, e)| s <= e)
        {
            format!(
                "{}{START}{}{END}{}",
                &html[..start],
                &html[start..end],
                &html[end..]
            )
        } else {
            format!("<html><body>{START}{html}{END}</body></html>")
        }
    };
    let header = |start_html, end_html, start_fragment, end_fragment| {
        let mut header = format!(
            "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
        );
        if let Some(url) = source_url {
            header.push_str(&format!("SourceURL:{url}\r\n"));
        }
        header
    };
    let start_html = header(0, 0, 0, 0).len();
    let end_html = start_html + document.len();
    if end_html as u64 > 9_999_999_999 {
        return Err(error("CF_HTML exceeds the ten-digit offset limit"));
    }
    let fragment_start = document.find(START).expect("fragment marker inserted") + START.len();
    let fragment_end = fragment_start
        + document[fragment_start..]
            .find(END)
            .expect("fragment end checked");
    Ok(header(
        start_html,
        end_html,
        start_html + fragment_start,
        start_html + fragment_end,
    ) + &document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_offsets(fragment: &str, source: Option<&str>) {
        let html = format!("<html><body>{fragment}</body></html>");
        let encoded = encode(&html, source).unwrap();
        let offset = |key: &str| -> usize {
            let line = encoded.lines().find(|line| line.starts_with(key)).unwrap();
            let value = line.split_once(':').unwrap().1;
            assert_eq!(value.len(), 10);
            value.parse().unwrap()
        };
        let sh = offset("StartHTML:");
        let eh = offset("EndHTML:");
        let sf = offset("StartFragment:");
        let ef = offset("EndFragment:");
        assert_eq!(eh, encoded.len());
        assert_eq!(&encoded.as_bytes()[sf..ef], fragment.as_bytes());
        assert_eq!(&encoded[sh..sf], format!("<html><body>{START}"));
        assert_eq!(&encoded[ef..eh], format!("{END}</body></html>"));
        assert!(sh <= sf && sf <= ef && ef <= eh);
        if let Some(url) = source {
            assert!(encoded[..sh].contains(&format!("SourceURL:{url}\r\n")));
        }
    }

    #[test]
    fn ascii_byte_offsets() {
        assert_offsets("<p>Hello world</p>", None);
    }
    #[test]
    fn chinese_byte_offsets() {
        assert_offsets("<p>中文剪贴板</p>", None);
    }
    #[test]
    fn emoji_byte_offsets() {
        assert_offsets("<p>Hi 🦀👨‍👩‍👧‍👦</p>", None);
    }
    #[test]
    fn empty_byte_offsets() {
        assert_offsets("", None);
        let encoded = encode("", None).unwrap();
        assert!(encoded.ends_with(&format!("<html><body>{START}{END}</body></html>")));
    }
    #[test]
    fn source_url_byte_offsets() {
        assert_offsets("<p>中文 🦀</p>", Some("https://example.com/a?q=1"));
    }
    #[test]
    fn rejects_source_url_header_injection() {
        for url in ["https://example.com/\r\nEndHTML:0", "a\0b", "中文"] {
            assert!(encode("", Some(url)).is_err());
        }
    }
    #[test]
    fn respects_existing_fragment_and_body_attributes() {
        let html = format!("<HTML><BODY lang=\"zh\">{START}中文{END}</BODY></HTML>");
        let encoded = encode(&html, None).unwrap();
        assert!(encoded.ends_with(&html));
        assert_eq!(encoded.matches(START).count(), 1);
        assert!(encode("<!--StartFragment-->broken", None).is_err());
        assert!(encode("<BODY lang=\"zh\">中文</BODY>", None)
            .unwrap()
            .ends_with(&format!("<BODY lang=\"zh\">{START}中文{END}</BODY>")));
    }
}
