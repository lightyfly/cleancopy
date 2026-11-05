# Contributing to CleanCopy

Thank you for helping make Markdown easier to paste. Useful contributions include renderer edge cases, reproducible clipboard bugs, and actual application compatibility reports. Please distinguish observed behavior from assumptions in the [design](docs/design.md).

## Local development

Install a Rust toolchain with Cargo, rustfmt, and Clippy. From the repository root:

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace contains `cleancopy-core` (parsing/rendering), `cleancopy-clipboard` (native formats/access), `cleancopy-cli` (the CLI binary), and `cleancopy-desktop` (the `cleancopy-app` desktop app). Do not assume the manifest version denotes a published release.

## Tests and clipboard safety

Prefer `cleancopy --print plain|html|markdown` with an explicit input file for non-destructive renderer investigation. A clipboard write replaces its previous contents; clipboard history managers may retain test data. Do not use secrets in fixtures or bug reports.

Graphical clipboard tests are ignored by default. On Linux, install Xvfb and its platform prerequisites, then use a separate display with reset disabled to avoid reset between short-lived X11 clients:

```sh
xvfb-run -a -s '-screen 0 1280x720x24 -noreset' cargo test -p cleancopy-cli --test clipboard -- --ignored
xvfb-run -a -s '-screen 0 1280x720x24 -noreset' cargo test -p cleancopy-clipboard --test native -- --ignored
```

Do not run these against your normal desktop unless you intentionally accept clipboard replacement. macOS/Windows native tests need a real graphical session; successful cross-compilation is **not** a clipboard runtime result. See [verification](docs/verification.md) for executed commands and current limitations.

For renderer changes, add a minimal case in `fixtures/` and/or a focused assertion in the core tests. Golden snapshots are in `tests/snapshots/`. Review snapshot changes, including CJK widths, blank lines, code whitespace, and list numbering. If using the optional `cargo-insta` developer tool, review with `cargo insta review`; never accept unrelated snapshots blindly. Keep the [plain-text rules](docs/plain-text-rules.md) synchronized with intentional behavior changes.

## Reporting bugs and application compatibility

Include:

- OS/version, architecture, desktop/display protocol (X11, XWayland, or Wayland), application/version, and clipboard manager if present.
- CleanCopy revision and exact command/mode; whether input came from file, stdin, or clipboard.
- A small sanitized UTF-8 Markdown sample, expected output, actual output, and paste method.
- `cleancopy --print plain sample.md` / `--print html` output where relevant, and `cleancopy inspect` format names/sizes. Inspection itself can fail if the owner changes or refuses requests.
- Whether the symptom reproduces in `--markdown`, `--plain`, and multi mode. Multi's plain channel contains rendered text, so a plain-text paste shortcut does not recover Markdown source.

For editor Markdown fidelity, paste and save the result, then compare it with `cleancopy --print markdown sample.md` (allow for intentional cleanup/line endings). Record actual results rather than assigning a pass based on advertised formats. See [compatibility](docs/compat.md). No macOS/Windows runtime success or Word/WeChat/etc. paste behavior should be inferred from Linux Xvfb tests.

## Submitting changes

Keep changes focused, describe the reason and checks performed, and identify checks you could not run. Do not turn an unverified platform assumption into a compatibility guarantee. Update user-facing docs and the Unreleased changelog for behavior changes. Integration configuration and especially clipboard-overwriting hooks should remain explicit, reviewed, and opt-in. Never include real transcripts, credentials, or private clipboard data in examples.

## Licensing

The project is dual licensed under **MIT OR Apache-2.0**. Unless you explicitly state otherwise, contributions intentionally submitted for inclusion are provided under both licenses, without additional terms or conditions. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE). Keep appropriate attribution for third-party material and do not submit code or assets you lack permission to contribute.
