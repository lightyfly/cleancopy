# Clipboard verification — W2

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
