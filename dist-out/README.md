# dist-out (local build, not published)

Built 2026-11-01 21:11 UTC+8 with `dist build --artifacts=local --target x86_64-unknown-linux-gnu` (cargo-dist 0.32.0, source commit 69e9e06).

4eca58dc69e8ebce9c8607ce44b91cef07410ef0ed36a6c64c4383b49fb0c753 *cleancopy-cli-x86_64-unknown-linux-gnu.tar.xz

95cb2bf009cd5ea41210912e73db23d772b50b41b58471fae4abd203fd24de5d *cleancopy-desktop-x86_64-unknown-linux-gnu.tar.xz

- Linux x86_64: produced, sha256 verified, extracted binaries run (`cleancopy --print plain`, `cleancopy-app --help`).
- Windows x86_64-msvc: NOT produced — dist requires `cargo-xwin` (downloads MSVC CRT/Windows SDK, needs Microsoft license acceptance); not installed.
- macOS: NOT produced — dist refuses Linux→macOS cross-compilation. Build on a Mac: `dist build --artifacts=local`.
Tarballs are git-ignored; only this README is committed.
