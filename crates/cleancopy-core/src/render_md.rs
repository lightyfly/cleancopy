/// Keep the Markdown source intact apart from terminal escapes, edge whitespace,
/// and platform-native line endings.
pub fn render_markdown(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut clean = MarkdownText(String::new());
    vte::Parser::new().advance(&mut clean, normalized.as_bytes());
    crate::native_newlines(clean.0.trim())
}

struct MarkdownText(String);

impl vte::Perform for MarkdownText {
    fn print(&mut self, character: char) {
        self.0.push(character);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' || byte == b'\t' {
            self.0.push(char::from(byte));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_without_reformatting() {
        let actual = render_markdown(" \x1b[31m# 中文\x1b[0m\r\n\r\n- **保留**\r结束 \n");
        assert_eq!(actual, crate::native_newlines("# 中文\n\n- **保留**\n结束"));
    }

    #[test]
    fn preserves_tabs_inside_code() {
        assert_eq!(render_markdown("```\n\tcode  \n```"), crate::native_newlines("```\n\tcode  \n```"));
    }

    #[test]
    fn removes_osc_hyperlinks() {
        assert_eq!(render_markdown("\x1b]8;;https://example.com\x1b\\text\x1b]8;;\x1b\\"), "text");
    }
}
