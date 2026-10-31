# Clipboard verification — W2 / W3

> The W2 record below is historical. W3 replaces Linux's arboard writer and alias-size workaround with a native x11rb owner, adds Windows native access, and adds portable CF_HTML. See the W3 section for the current implementation and evidence.

Verified **2026-11-01**. Authoritative requirements: `docs/design.md` §§4.4, 5.3, 6.

## Scope and environment

Linux x86_64, isolated Xvfb display, Rust toolchain in this workspace. Dependency pinned to **arboard 3.6.1** (not an assertion about newer releases); x11rb 0.13.2, objc2-app-kit/objc2-foundation 0.3.2, objc2 0.6.5. Source references below are crate-relative paths from the crates.io source archives, reproducible in `$CARGO_HOME/registry/src/*/<crate>-<version>/`. Source inspection is explicitly distinguished from runtime verification.

macOS was **cross-checked, not executed**. No Word, Feishu, Notes, Obsidian, Typora, Windows desktop, native Wayland compositor, or clipboard manager compatibility result is claimed. The design's application matrix remains unverified.

## Arboard investigation

| Question | Evidence and result |
| --- | --- |
| Does HTML writing clear earlier formats on macOS? | **Yes, by source inspection.** `arboard-3.6.1/src/platform/osx.rs:301–322`: `html()` invokes `clipboard.clear()` at 302, writes HTML, then optional plain text. `clear()` calls `clearContents` at 125. Calling separate arboard setters is not an append transaction. CleanCopy instead owns the native macOS transaction: one `clearContents`, followed by three `setString:forType:` calls in Multi mode. |
| Does HTML replace existing X11 data? | **Yes, source and Xvfb runtime.** `src/platform/linux/x11.rs:991–1020` builds one vector containing optional UTF8_STRING plus HTML. `write():240–264`, especially 255, replaces the entire stored vector and reasserts selection ownership. A later text write removes HTML; the native regression test confirms this. No separate clear is needed on X11. |
| Does Windows HTML clear? | **Yes, by source inspection only.** `src/platform/windows.rs:688–700` calls `clipboard_win::raw::set_string` for the fallback, then `set_without_clear` for HTML. `clipboard-win-5.4.3/src/raw.rs:584–585` selects `options::DoClear::EMPTY_FN`. CleanCopy's Windows backend is still an explicit unsupported stub in W2. |
| Can arboard append arbitrary custom formats? | **No public arbitrary-format/append API in this pinned version.** `src/lib.rs:175–253` exposes fixed text, HTML, image and file-list getters/setters, not raw custom MIME/UTI setters. Its platform backends have internal custom atoms/format registrations, but they are not a public raw-format API. Linux `set_html` only stores HTML plus optional plain text; macOS and Windows HTML setters only add their predefined HTML/text representations. CleanCopy therefore uses NSPasteboard directly for `net.daringfireball.markdown` on macOS. Linux Multi intentionally offers only HTML + plain, not `text/markdown`. |
| Does arboard support Wayland HTML? | **Yes in upstream source, conditionally; not enabled or runtime-tested here.** `src/platform/linux/wayland.rs:147–181` builds `MimeSource` entries for optional text and `text/html`, then calls `copy_multi`. `src/platform/linux/mod.rs:253–262` dispatches HTML to that backend under feature `wayland-data-control`. CleanCopy disables arboard default features and does not enable that feature; W2 is X11/XWayland only. Native Wayland support is not implied by the upstream capability. `wl-copy` multi-MIME behavior was not investigated in W2. |
| Does the X11 owner need to stay alive? | **Yes, source and CLI runtime.** `src/platform/linux/mod.rs:301–327` documents lifetime and `wait()`. CleanCopy uses safe `Command` re-exec of the CLI's hidden `--clipboard-holder`, not an unsafe fork after arboard has started threads. The child receives length-framed UTF-8 payloads, writes once, signals readiness after acquiring ownership, and retains the arboard object while polling ownership until replacement. Parent readiness is bounded to 10 seconds after payload transfer; conversion requests have a 3-second deadline. |

### Discovered upstream X11 advertised-alias defect

The first full CLI probe wrote successfully but `inspect` failed with `X11 owner refused clipboard target`. Source explains why: `src/platform/linux/x11.rs:612–619` advertises `UTF8_STRING` plus `text/plain;charset=utf-8` and `text/plain;charset=UTF-8`, but `:663` only serves exact stored format atoms. The MIME aliases are advertised without corresponding stored data.

CleanCopy inspection requests each native target. If one of those plain-text aliases is refused, it reports the byte size read from UTF8_STRING instead; this is a compatible representation size, **not evidence the owner can actually serve the refused alias**. Other target read failures remain errors. Protocol-only/side-effect targets such as SAVE_TARGETS, MULTIPLE and DELETE are not requested as payloads. TARGETS itself is listed with its native atom-array byte size. Thus `inspect` is a diagnostic of advertised formats, not a guarantee every advertised target is functional in another application.

## Implemented mode contract

| Mode | macOS | Linux X11 |
| --- | --- | --- |
| Multi | public.html + public.utf8-plain-text + net.daringfireball.markdown | text/html + UTF8_STRING and advertised MIME aliases |
| Html | HTML + rendered plain fallback | HTML + rendered plain fallback |
| Plain | rendered plain text only | rendered plain text only |
| Markdown | original Markdown in plain-text channel only | original Markdown in plain-text channel only |

Markdown mode deliberately avoids HTML and does not depend on an editor supporting a custom Markdown format. macOS inspection enumerates actual NSPasteboard types and reads NSData lengths, not string character counts or guessed types. Linux inspection queries TARGETS and requests data, including support for incremental INCR transfers. Inspection is not an atomic snapshot: an owner changing mid-inspection may cause an error or results from different owners.

## Executed checks

All passed on 2026-11-01:

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
xvfb-run -a cargo test -p cleancopy-cli --test clipboard -- --ignored
cargo test -p cleancopy-clipboard
xvfb-run -a cargo test -p cleancopy-clipboard -- --include-ignored
cargo clippy -p cleancopy-clipboard --all-targets -- -D warnings
cargo check --target x86_64-apple-darwin -p cleancopy-clipboard -p cleancopy-cli
cargo clippy --target x86_64-apple-darwin -p cleancopy-clipboard -p cleancopy-cli --all-targets -- -D warnings
cargo check --target aarch64-apple-darwin -p cleancopy-clipboard -p cleancopy-cli
cargo check --target x86_64-pc-windows-msvc -p cleancopy-clipboard -p cleancopy-cli
```

`crates/cleancopy-clipboard/tests/native.rs` is ignored by default because it replaces the real clipboard. Under isolated Xvfb it exercised:

- Multi: HTML and plain coexist; Chinese and emoji bytes round-trip exactly.
- Plain and Markdown: correct plain-text representation; earlier HTML is removed.
- Html: HTML with rendered plain fallback, no dependence on Markdown custom types.
- Native inspection correctly reports UTF-8 byte lengths, including **1,050,007-byte HTML** and **1,200,000-byte plain text** payloads. Arboard's owner serves these using large properties; this test does **not** independently verify the INCR branch against an INCR-producing external owner.

The coordinated full CLI runtime probe also passed under Xvfb after the alias fix: stdin write exits successfully, a separate process inspects persistent HTML/plain content, all selected modes replace the previous format set, and terminal/no-argument input reads the current clipboard and writes converted content. This validates persistence beyond the original CLI process lifetime. macOS clipboard and CLI cross-target clippy passed, but linking/running macOS and target-application paste testing require a macOS host.

## Remaining limitations / follow-up

- Run the ignored native test and the design §11 application matrix on a real macOS host. In particular, custom Markdown UTI consumer behavior is not yet established.
- Windows native writer, CF_HTML offsets/retries, and Windows runtime tests remain W3.
- Linux native Wayland is unsupported in this implementation; XWayland needs a usable DISPLAY and compositor bridging behavior remains untested.
- Test INCR with an external incremental owner, clipboard-manager handoff/history, hostile/unresponsive owners, and rapid competing owners. The holder exits after ownership changes; it does not implement a clipboard-manager SAVE_TARGETS handoff.
- Byte sizes for advertised but refused arboard plain aliases use the UTF8_STRING fallback as documented above.

---

# W3 — native Windows and three-format X11

Verified **2026-11-01** on this Linux x86_64 box. Requirements: `docs/design.md` §§4.3, 5.3, 7, 11. No `xclip`/`wl-copy` installation or desktop clipboard was needed; runtime work used isolated Xvfb.

## Current implementation

- `cf_html.rs` is always compiled and tested, including on Linux. CF_HTML uses `Version:0.9`, CRLF header lines, ten-digit zero-padded byte offsets, a complete HTML context, and fragment markers. Optional ASCII `SourceURL` contributes to all offsets; CR/LF/NUL injection is rejected. Existing markers are respected. Tests cover ASCII, Chinese, emoji, empty content, source URLs, existing markers and body attributes. They slice encoded UTF-8 bytes at the offsets, check exact content and surrounding markers, and assert EndHTML equals the encoded byte length.
- Windows uses native `windows-sys` APIs, one `EmptyClipboard`, registered `HTML Format` / `text/markdown`, UTF-16 NUL-terminated CF_UNICODETEXT with CRLF, and CF_HTML UTF-8. OpenClipboard retries ten attempts, 20 ms between failures. The implementation owns a hidden window, uses moveable global memory with explicit transfer of ownership, and closes the clipboard via RAII. Inspection enumerates actual formats and reports native allocation sizes (including terminators/padding, not UTF-8 character counts).
- X11 now uses **x11rb directly**, not arboard in production. The event-thread owner offers HTML, plain aliases and Markdown in one ownership acquisition; it confirms ownership before signalling readiness. CLI re-exec remains the background-holding mechanism. SelectionClear ends the holder after replacement. Dropping a library clipboard stops its thread without releasing a newer owner's selection.
- Every advertised X11 data target is actually served. The W2 advertised-but-unserved arboard alias defect and inspection's guessed UTF8_STRING size fallback are gone. Arboard remains a dev dependency for independent interoperability checks only.
- Data over 64 KiB uses INCR with delete-acknowledged chunks and a zero-length final property. Idle outgoing transfers expire after 30 seconds; readers use progress-reset idle deadlines. Unsupported targets are refused without terminating the owner. TEXT carries UTF-8. MULTIPLE, TIMESTAMP, and SAVE_TARGETS are not advertised or implemented.

| Mode | Windows | X11 |
| --- | --- | --- |
| Multi | CF_UNICODETEXT + HTML Format + text/markdown | TARGETS + text/html + text/markdown + five plain aliases |
| Html | CF_UNICODETEXT + HTML Format | TARGETS + text/html + five plain aliases |
| Plain | CF_UNICODETEXT | TARGETS + five plain aliases |
| Markdown | Markdown in CF_UNICODETEXT | TARGETS + Markdown in five plain aliases |

X11 plain aliases: `UTF8_STRING`, `TEXT`, `text/plain`, `text/plain;charset=utf-8`, `text/plain;charset=UTF-8`.

## Tested here — independent reader against the exited CLI

Reproducible commands from the repository root (`$HOME/.cargo/bin` on PATH):

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --workspace
cargo build --workspace --example x11_read
xvfb-run -a python3 scripts/w3-x11-probe.py
```

The script invokes the built CLI, waits for its **exit status 0**, then starts a separate `x11_read` process for each selection request. The example uses x11rb directly and does not call the clipboard library. `x11_read TARGETS` prints atom names; `x11_read text/html`, `x11_read UTF8_STRING`, and `x11_read text/markdown` emit unmodified bytes, equivalent to `xclip -o -t <target>`. Expected payloads come from the public `cleancopy --print` interface, not internal function calls.

Observed output (selected lines; the script checks every advertised alias):

```text
CLI exited 0 (Multi); subsequent requests use a separate process
TARGETS: TARGETS, TEXT, UTF8_STRING, text/html, text/markdown, text/plain, text/plain;charset=UTF-8, text/plain;charset=utf-8
PASS text/html: 137 bytes sha256=fd58ee5cbac800b421f626ee79e0ed583f457822bd489c186c912a6e5f4d1d40
PASS UTF8_STRING: 52 bytes sha256=401b0574a5bedc8ae0a1f740e8bedfccb7c2759ecb5e9c58175a45d8b1f85692
PASS text/markdown: 58 bytes sha256=39a6f0fb57c22c2a74dc481303cbb08f4cd8923e1e065350fc60296666168341
PASS unsupported target refused: Error: "owner refused target"
PASS empty input exit 1: cleancopy: input is empty or whitespace only
PASS text/html: 1050028 bytes sha256=9b57db6adfb1ebcfecbd76c02933ead49c57c3de7ba1cc250df5766eacdf06d4
PASS UTF8_STRING: 474999 bytes sha256=43632577fec68ddb48bb9bb1f9de89602afb84f4baee57631b1a2e2022786249
PASS text/markdown: 574998 bytes sha256=b33859a445f1cd47ce2f97ace9cfe436cf13183927703a959316ced4e51ad025
PASS W3 runtime probe: persistence, all advertised targets, mode replacement, refusal, empty input, large transfers
```

All comparisons were exact byte equality, including Chinese and emoji. `inspect` reported TARGETS as 32 bytes, HTML as 137, plain aliases as 52, and Markdown as 58 for the small payload; large sizes matched the three figures above. Every `--plain`, `--html`, and `--markdown` invocation replaced the previous format set rather than retaining stale Markdown/HTML. After an unsupported target and after rejected empty input, the existing clipboard remained readable and unchanged. Replacing the large Multi clipboard with small Plain data removed HTML and returned the original 52 plain bytes.

## Automated checks executed here

All commands below passed. These are build/static/unit/integration checks, separately from the CLI runtime evidence above:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo check --workspace --target x86_64-pc-windows-msvc
cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
cargo check --workspace --target x86_64-apple-darwin
cargo clippy --workspace --all-targets --target x86_64-apple-darwin -- -D warnings
cargo check --workspace --target aarch64-apple-darwin
cargo clippy --workspace --all-targets --target aarch64-apple-darwin -- -D warnings
xvfb-run -a cargo test -p cleancopy-clipboard --test native -- --ignored
xvfb-run -a cargo test -p cleancopy-cli --test clipboard -- --ignored
```

`cargo test`: **38 passed**, two graphical integration tests ignored by default. The two isolated Xvfb commands then each passed their one ignored test. The native integration test also exercises an independent arboard reader, large payloads, all advertised alias sizes, releasing ownership on Drop, and dropping a superseded handle without affecting a newer owner. Windows-only UTF-16/CRLF unit test code was compiled by target Clippy but cannot execute here.

## Not testable here / not claimed

- **Windows runtime:** allocation ownership, retry contention, Windows clipboard readback, and real application paste need a Windows host. Cross-target compilation and portable CF_HTML tests are not substitutes for this runtime evidence.
- **macOS runtime:** NSPasteboard and the design §11 Word/Feishu/Notes/editor matrix need a macOS host. No macOS runtime success is claimed.
- **Wayland:** no native compositor/data-control runtime verification; this backend requires a usable X11/XWayland DISPLAY. XWayland-to-native-Wayland bridging was not exercised.
- **wl-copy multi-MIME:** binary absent and behavior not tested; no claim that it can or cannot publish several MIME types. W3's solution is native X11 ownership, not multiple wl-copy calls.
- **Desktop/application compatibility:** no Word, WPS, WeChat, Feishu, Obsidian or Typora paste result, clipboard-manager SAVE_TARGETS handoff, or custom Markdown consumer behavior is established by Xvfb. The design §11 application matrix remains open.
