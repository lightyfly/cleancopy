# W4 local regression checks

Executed **2026-11-01** on Linux x86_64; no commit, push, tag, publication, or
release was performed. See [agent verification](verification.md),
[compatibility evidence](compat.md), and [distribution planning](distribution.md).

## Successful required checks

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Also passed `cargo test --workspace --all-targets --all-features`: **38 tests
passed**, two graphical tests ignored by default. The ignored tests were then
run explicitly on isolated Xvfb with server reset disabled:

```sh
cargo build --workspace --example x11_read
xvfb-run -a -s '-screen 0 1280x1024x24 -noreset' sh -c '
  cargo test -p cleancopy-clipboard --test native -- --ignored &&
  cargo test -p cleancopy-cli --test clipboard -- --ignored &&
  python3 scripts/w3-x11-probe.py &&
  python3 scripts/w4-compat-probe.py
'
```

Both ignored tests passed (one each), as did both probes. `git diff --check`
also passed. Source installation was verified without replacing a global binary:

```sh
cargo install --path crates/cleancopy-cli --root /tmp/cleancopy-w4-install --locked
/tmp/cleancopy-w4-install/bin/cleancopy --version
# cleancopy 0.1.0
```

## Initial Xvfb failures and resolution

Initial runs using default `xvfb-run -a` intermittently failed at native test
`clipboard.write` with `failed to read whole buffer`, or at CLI holder readiness
with `failed to fill whole buffer`. A startup delay did not resolve the latter.
These failures are disclosed rather than counted as passes.

A bounded repeated comparison found native test 37/40 and CLI test 28/30 passing
on default Xvfb, versus 40/40 and 30/30 with `-noreset`. An independent libX11
atom probe created a custom atom, disconnected the last client, and reconnected:
the atom disappeared under default Xvfb but persisted with `-noreset`.

The native constructor briefly opens then drops an inspection connection before
the writer opens its own connection. With no other clients, default Xvfb can
reset between those connections. The CLI's holder can exit during this setup,
leaving readiness-pipe EOF (`failed to fill whole buffer`), not corrupted
clipboard bytes. Using an isolated non-resetting server resolved the test
harness issue; **no Rust source changes were made**.

No new macOS/Windows runtime evidence was obtained. Their earlier W3
cross-target checks remain compile/static evidence only. No GUI editor or
terminal paste session was exercised in W4.
