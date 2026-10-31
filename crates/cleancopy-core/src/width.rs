use unicode_width::UnicodeWidthStr;

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn pad_right(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(display_width(text))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_is_double_width() {
        assert_eq!(display_width("中文Rust"), 8);
        assert_eq!(pad_right("中文", 6), "中文  ");
    }
}
