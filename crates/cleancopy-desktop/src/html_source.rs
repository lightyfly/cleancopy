//! HTML clipboard sources (ChatGPT, Gemini, browsers) → Markdown.
//!
//! Chat UIs wrap code blocks in a header with the language name and a copy
//! button. Converting that markup naively glues the header onto the code
//! ("rustfn main()"). Code blocks are therefore lifted out before the generic
//! HTML→Markdown conversion and re-emitted as fenced blocks whose info string
//! is the detected language.

const UI_LABELS: &[&str] = &[
    "copy code",
    "copy",
    "copied!",
    "copied",
    "content_copy",
    "复制代码",
    "复制",
    "已复制",
    "edit",
    "编辑",
    "run",
    "download",
    "下载",
    "expand_less",
    "expand_more",
];

struct Block {
    language: String,
    code: String,
}

/// Convert an HTML clipboard payload to Markdown, preserving code blocks.
pub fn html_to_markdown(raw: &str) -> String {
    let mut blocks = Vec::new();
    let lifted = lift_code_blocks(raw, &mut blocks);
    // Code is still behind placeholders here, so heading rewrites cannot
    // touch fenced code.
    let mut markdown = atx_headings(&html2md::parse_html(&lifted));
    for (index, block) in blocks.iter().enumerate() {
        let fence = fence_for(&block.code);
        let mut code = block.code.clone();
        if !code.ends_with('\n') {
            code.push('\n');
        }
        let fenced = format!("{fence}{}\n{code}{fence}", block.language);
        markdown = markdown.replacen(&placeholder(index), &fenced, 1);
    }
    markdown
}

/// Number of characters a reader sees: tags, comments, scripts and styles are
/// dropped, entities count as one character, and runs of whitespace as one.
pub fn visible_chars(raw: &str) -> usize {
    let text = decode_entities(&strip_tags(&remove_elements(
        raw,
        &["script", "style", "head"],
    )));
    let mut count = 0;
    let mut in_space = true;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !in_space {
                count += 1;
                in_space = true;
            }
        } else {
            count += 1;
            in_space = false;
        }
    }
    if in_space && count > 0 {
        count -= 1;
    }
    count
}

fn placeholder(index: usize) -> String {
    format!("CCCODEBLOCK{index}X")
}

fn fence_for(code: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for ch in code.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Find the end (exclusive) of the element starting at `start` (which points at
/// `<name`), honouring nesting of the same tag name.
fn element_end(html: &str, lower_html: &str, start: usize, name: &str) -> Option<usize> {
    let open = format!("<{name}");
    let close = format!("</{name}");
    let mut depth = 0usize;
    let mut pos = start;
    loop {
        let next_open = lower_html[pos..].find(&open).map(|i| i + pos);
        let next_close = lower_html[pos..].find(&close).map(|i| i + pos)?;
        match next_open {
            Some(o) if o < next_close && is_tag_boundary(lower_html, o + open.len()) => {
                depth += 1;
                pos = o + open.len();
            }
            Some(o) if o < next_close => pos = o + open.len(),
            _ => {
                depth = depth.saturating_sub(1);
                let end = html[next_close..].find('>').map(|i| i + next_close + 1)?;
                if depth == 0 {
                    return Some(end);
                }
                pos = end;
            }
        }
    }
}

fn is_tag_boundary(lower_html: &str, at: usize) -> bool {
    lower_html[at..]
        .chars()
        .next()
        .is_none_or(|c| c == '>' || c == '/' || c.is_whitespace())
}

/// Find the next start of `<name` (as a whole tag name) at or after `from`.
fn find_tag(lower_html: &str, from: usize, name: &str) -> Option<usize> {
    let open = format!("<{name}");
    let mut pos = from;
    while let Some(i) = lower_html[pos..].find(&open) {
        let at = pos + i;
        if is_tag_boundary(lower_html, at + open.len()) {
            return Some(at);
        }
        pos = at + open.len();
    }
    None
}

fn lift_code_blocks(raw: &str, blocks: &mut Vec<Block>) -> String {
    let mut out = String::with_capacity(raw.len());
    let lower_html = lower(raw);
    let mut pos = 0;
    loop {
        // The earliest of a Gemini <code-block> wrapper or a bare <pre>.
        let gemini = find_tag(&lower_html, pos, "code-block");
        let pre = find_tag(&lower_html, pos, "pre");
        let (start, name) = match (gemini, pre) {
            (Some(g), Some(p)) if g < p => (g, "code-block"),
            (Some(g), None) => (g, "code-block"),
            (_, Some(p)) => (p, "pre"),
            (None, None) => break,
        };
        let Some(end) = element_end(raw, &lower_html, start, name) else {
            break;
        };
        let element = &raw[start..end];
        out.push_str(&raw[pos..start]);
        match parse_block(element) {
            Some(block) => {
                out.push_str(&format!("<p>{}</p>", placeholder(blocks.len())));
                blocks.push(block);
            }
            None => out.push_str(element),
        }
        pos = end;
    }
    out.push_str(&raw[pos..]);
    out
}

fn parse_block(element: &str) -> Option<Block> {
    let lower_el = lower(element);
    let code_start = find_tag(&lower_el, 0, "code");
    let pre_start = find_tag(&lower_el, 0, "pre");
    let (header, code_html, class_lang) = match code_start {
        Some(c) => {
            let open_end = element[c..].find('>')? + c + 1;
            let close = lower_el.rfind("</code")?;
            if close < open_end {
                return None;
            }
            let class_lang = language_from_class(&element[c..open_end]);
            // Header: everything before <code>, excluding the <pre ...> tag itself.
            let header_html = &element[..c];
            (header_html, &element[open_end..close], class_lang)
        }
        None => {
            let p = pre_start?;
            let open_end = element[p..].find('>')? + p + 1;
            let close = lower_el.rfind("</pre")?;
            if close < open_end {
                return None;
            }
            (&element[..0], &element[open_end..close], None)
        }
    };
    let header_language = header_language(header);
    let language = class_lang.or(header_language).unwrap_or_default();
    let code = decode_entities(&strip_tags(&br_to_newline(code_html)));
    Some(Block { language, code })
}

fn br_to_newline(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let lower_html = lower(html);
    let mut pos = 0;
    while let Some(i) = find_tag(&lower_html, pos, "br") {
        out.push_str(&html[pos..i]);
        out.push('\n');
        pos = html[i..].find('>').map_or(html.len(), |e| i + e + 1);
    }
    out.push_str(&html[pos..]);
    out
}

fn language_from_class(open_tag: &str) -> Option<String> {
    let lower_tag = lower(open_tag);
    let i = lower_tag.find("language-")? + "language-".len();
    let lang: String = open_tag[i..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '-' | '_' | '.'))
        .collect();
    (!lang.is_empty()).then(|| lang.to_ascii_lowercase())
}

/// Text of the header (language label plus buttons), minus button labels.
fn header_language(header_html: &str) -> Option<String> {
    let without_buttons = remove_elements(header_html, &["button", "mat-icon", "svg"]);
    let text = decode_entities(&strip_tags_spaced(&without_buttons));
    let mut words: Vec<&str> = text.split_whitespace().collect();
    // Drop multi-word and single-word UI labels.
    let joined = words.join(" ").to_lowercase();
    let mut cleaned = joined.clone();
    for label in UI_LABELS {
        cleaned = cleaned.replace(label, " ");
    }
    words = cleaned.split_whitespace().collect();
    let candidate = words.first()?;
    let plausible = candidate.len() <= 24
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '-' | '_' | '.'));
    plausible.then(|| candidate.to_string())
}

fn remove_elements(html: &str, names: &[&str]) -> String {
    let mut current = html.to_string();
    for name in names {
        let lower_html = lower(&current);
        let mut out = String::with_capacity(current.len());
        let mut pos = 0;
        while let Some(start) = find_tag(&lower_html, pos, name) {
            out.push_str(&current[pos..start]);
            match element_end(&current, &lower_html, start, name) {
                Some(end) => pos = end,
                None => {
                    pos = current.len();
                    break;
                }
            }
        }
        out.push_str(&current[pos..]);
        current = out;
    }
    current
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn strip_tags_spaced(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// html2md writes h1/h2 as setext (`Title` + `===`/`---`) and h3+ as
/// `### Title ###`. "Markdown 原样" output always uses plain ATX headings
/// (`## Title`) without closing hashes (report v3 B-4).
fn atx_headings(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let underline = |line: &str, ch: char| {
        let t = line.trim_end();
        t.len() >= 3 && t.chars().all(|c| c == ch)
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let text = line.trim();
        let next = lines.get(i + 1).copied().unwrap_or("");
        let is_text = !text.is_empty()
            && !line.starts_with([' ', '\t'])
            && !text.starts_with(['#', '*', '-', '+', '|', '>', '`'])
            && !text.starts_with(|c: char| c.is_ascii_digit());
        if is_text && underline(next, '=') {
            out.push(format!("# {text}"));
            i += 2;
            continue;
        }
        if is_text && underline(next, '-') {
            out.push(format!("## {text}"));
            i += 2;
            continue;
        }
        let hashes = line.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
            let body = line[hashes..].trim();
            let body = body.trim_end_matches('#');
            // Only strip a closing sequence that is separated by a space.
            let body = if body.len() < line[hashes..].trim().len() && body.ends_with(' ') {
                body.trim_end()
            } else {
                line[hashes..].trim()
            };
            out.push(format!("{} {body}", "#".repeat(hashes)));
        } else {
            out.push(line.to_owned());
        }
        i += 1;
    }
    let mut result = out.join("\n");
    if markdown.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn markdown_output_uses_atx_headings_without_closing_hashes() {
        let html = "<h1>项目</h1><h2>项目周报</h2><h3>主要进展</h3><h6>六</h6>\
                    <p>正文</p><hr><p>后文</p><h2>C# 入门</h2>\
                    <pre><code class=\"language-md\">标题\n---\n## x ##</code></pre>";
        let md = super::html_to_markdown(html);
        assert!(md.contains("# 项目\n"), "{md}");
        assert!(md.contains("## 项目周报\n"), "{md}");
        assert!(md.contains("### 主要进展\n"), "{md}");
        assert!(md.contains("###### 六\n"), "{md}");
        assert!(md.contains("## C# 入门"), "{md}");
        assert!(!md.contains("==="), "{md}");
        assert!(!md.contains("主要进展 #"), "{md}");
        // A thematic break stays a break; fenced code is untouched.
        assert!(md.contains("\n---\n\n后文"), "{md}");
        assert!(md.contains("标题\n---\n## x ##"), "{md}");
    }

    use super::*;

    const CHATGPT: &str = r#"<div class="markdown prose w-full break-words"><h2>项目周报</h2><p>示例代码：</p><pre class="!overflow-visible"><div class="contain-inline-size rounded-md border-[0.5px] border-token-border-medium relative bg-token-sidebar-surface-primary"><div class="flex items-center text-token-text-secondary px-4 py-2 text-xs font-sans justify-between h-9 bg-token-sidebar-surface-primary select-none rounded-t-[5px]">rust</div><div class="sticky top-9"><div class="absolute end-0 bottom-0 flex h-9 items-center pe-2"><div class="bg-token-sidebar-surface-secondary text-token-text-secondary flex items-center rounded-sm px-2 font-sans text-xs"><span class="" data-state="closed"><button class="flex gap-1 items-center select-none px-4 py-1" aria-label="Copy"><svg width="24" height="24" viewBox="0 0 24 24"><path d="M7 5"></path></svg>Copy code</button></span></div></div></div><div class="overflow-y-auto p-4" dir="ltr"><code class="whitespace-pre! language-rust"><span><span class="hljs-keyword">fn</span></span><span> </span><span class="hljs-title function_">main</span><span>() {
    </span><span><span class="hljs-built_in">println!</span></span><span>(</span><span><span class="hljs-string">"a &lt; b &amp;&amp; c"</span></span><span>);
}
</span></code></div></div></pre><p>完毕。</p></div>"#;

    const CHATGPT_NO_CLASS: &str = r#"<pre><div><div>python</div><div><button>Copy code</button></div><div><code>print("hi")
</code></div></div></pre>"#;

    const GEMINI: &str = r#"<p>Here is the code:</p><code-block _nghost-ng-c1 class="ng-star-inserted"><div _ngcontent-ng-c1 class="code-block ng-tns-c1"><div class="code-block-decoration header-formatted gds-title-s ng-star-inserted"><span>Python</span><div class="buttons ng-star-inserted"><button mat-icon-button aria-label="Copy code" class="mdc-icon-button"><span class="mat-mdc-button-persistent-ripple"></span><mat-icon role="img" fonticon="content_copy" class="mat-icon">content_copy</mat-icon><span class="mat-focus-indicator"></span></button></div></div><div class="formatted-code-block-internal-container ng-tns-c1"><div class="animated-opacity ng-tns-c1"><pre class="ng-tns-c1"><code role="text" data-test-id="code-content" class="code-container formatted ng-tns-c1"><span class="hljs-keyword">def</span> <span class="hljs-title function_">greet</span>(name):
    <span class="hljs-keyword">return</span> <span class="hljs-string">f"Hello, <span class="hljs-subst">{name}</span>"</span>
</code></pre></div></div></div></code-block><p>Done.</p>"#;

    #[test]
    fn chatgpt_header_becomes_fence_info_string() {
        let md = html_to_markdown(CHATGPT);
        assert!(
            md.contains("```rust\nfn main() {\n    println!(\"a < b && c\");\n}\n```"),
            "{md}"
        );
        assert!(!md.contains("rustfn"), "{md}");
        assert!(!md.contains("Copy code"), "{md}");
        assert!(md.contains("项目周报") && md.contains("完毕。"), "{md}");
    }

    #[test]
    fn chatgpt_header_without_language_class_uses_header_text() {
        let md = html_to_markdown(CHATGPT_NO_CLASS);
        assert!(md.contains("```python\nprint(\"hi\")\n```"), "{md}");
        assert!(!md.contains("pythonprint"), "{md}");
    }

    #[test]
    fn gemini_decoration_becomes_fence_info_string() {
        let md = html_to_markdown(GEMINI);
        assert!(
            md.contains("```python\ndef greet(name):\n    return f\"Hello, {name}\"\n```"),
            "{md}"
        );
        assert!(
            !md.contains("content_copy") && !md.contains("Copy code"),
            "{md}"
        );
        assert!(!md.contains("Pythondef"), "{md}");
        assert!(
            md.contains("Here is the code:") && md.contains("Done."),
            "{md}"
        );
    }

    #[test]
    fn plain_pre_without_header_keeps_code_and_no_language() {
        let md = html_to_markdown("<pre>a &amp; b\nc</pre>");
        assert!(md.contains("```\na & b\nc\n```"), "{md}");
    }

    #[test]
    fn code_containing_backticks_gets_a_longer_fence() {
        let md = html_to_markdown("<pre><code>```inner```</code></pre>");
        assert!(md.contains("````\n```inner```\n````"), "{md}");
    }

    #[test]
    fn inline_code_is_not_treated_as_a_block() {
        let md = html_to_markdown("<p>use <code>cargo</code> now</p>");
        assert!(md.contains("`cargo`"), "{md}");
        assert!(!md.contains("```"), "{md}");
    }

    #[test]
    fn visible_chars_ignore_tags_and_count_entities_once() {
        assert_eq!(visible_chars("<p>a &amp; b</p>"), 5);
        assert_eq!(
            visible_chars("<h2>项目周报</h2>\n<p><strong>粗</strong>体</p>"),
            7
        );
        assert_eq!(
            visible_chars("<style>p{color:red}</style><script>x()</script><p>hi</p>"),
            2
        );
        assert!(visible_chars(CHATGPT) < CHATGPT.chars().count() / 5);
    }
}
