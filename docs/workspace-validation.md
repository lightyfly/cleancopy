# Workspace validation

Validated on Linux x86_64 on 2026-11-02, on branch `fix/desktop-e2e`.
Source checked: `1ad6966` (no Rust changes were needed).

Commands executed successfully:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Formatting produced no changes; Clippy produced no warnings. Workspace test
summaries total **151 passed, 0 failed, 6 ignored** (including doc-test suites,
which contained no tests). The six ignored tests were not executed by this run:

- `clipboard_survives_cli_exit_and_modes_replace_formats`
- `linux::retirement_tests::retired_owner_drains_routed_requests_and_incr`
- `linux::tests::snapshot_roundtrip_raw_targets_and_incr`
- `native_modes_and_byte_sizes`
- `desktop_runtime_matrix`
- `hotkey_pastes_formatted_text_and_restores_all_native_bytes`

These require a graphical clipboard or isolated native X11 harness. Historical
native execution evidence is recorded separately in `desktop-e2e-run.log`; it
must not be counted as execution of ignored tests in this workspace run.

No macOS/Windows build or real Word/WPS/Notepad/TextEdit validation was run here.
