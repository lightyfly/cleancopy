# G-05: native X11 clipboard flake investigation

Date: 2026-11-02 (UTC). Scope: `cleancopy-clipboard` only; no desktop/core changes.
Original report: `/workspace/cleancopy-test/report-v1.md`, G-05 / D-07.

## Findings and fixes

1. **Fixture lifetime / X server reset:** an isolated Xvfb resets when its last client disconnects. `NativeClipboard::new()` probes the server with a temporary connection, but does not itself retain a connection before a write. Test setup, teardown, and retirement-loop iterations could therefore reconnect while Xvfb was resetting. Reproduced the reported `Connection reset by peer (os error 104)` without external clipboard utilities. The native test binaries now retain an independent X11 connection for their entire process lifetime. A mutex serializes tests that manipulate the same global CLIPBOARD selection, including the default parallel harness. This is a test fixture fix, not a production retry that hides a broken server connection.
2. **Actual owner retirement race:** after fixing the reset, the retirement regression still intermittently timed out for a four-byte request, with both `stopped=true` and `stopped=false`. A single x11rb nonblocking event poll returning `None` is not proof that all incoming events have been drained: the poll may consume a pending reply while subsequent event packets remain unread. `Owner::run` now performs one final server round trip at the empty retirement queue, then drains again before exiting. Existing transfer completion and the three-second retirement bound remain. No arbitrary sleep or timeout increase was added. The panic now includes payload size and stop state for future diagnosis.
3. **HTML-only native integration:** added `html_only_snapshot_has_no_synthetic_text_targets` through the public clipboard crate API. A real X11 owner advertises exactly `text/html` plus protocol `TARGETS`, no plain/Markdown aliases. Separate reader handles verify byte size, refusal of plain text, exact snapshot capture and restore for a short Unicode document and a 200,007-byte HTML document (real INCR). HTML-to-Markdown conversion remains the engine's responsibility and is not claimed by this test.

## Results

All repetitions below are actual executed commands; raw combined stdout/stderr is embedded after this summary, not reconstructed from expected results.

| Stage | Command / display | Runs | Outcome |
|---|---|---:|---|
| Unmodified baseline | `cargo test -p cleancopy-clipboard --lib -- --ignored --test-threads=1`, isolated default resetting Xvfb | 10 | 2 passed, 8 failed (6 connection resets, 2 conversion timeouts) |
| Intermediate anchor-only fix | complete ignored suite, serial, isolated Xvfb | 10 | 8 passed, 2 failed (retirement conversion timeout; preserved in raw capture) |
| Final standard | `cargo test -p cleancopy-clipboard` | 10 | 10/10 exit 0; each: 10 ordinary unit tests passed, 4 native tests ignored across unit/integration binaries |
| Final native serial | `cargo test -p cleancopy-clipboard -- --ignored --test-threads=1`, isolated default resetting Xvfb | 10 | 10/10 exit 0; all 4 native tests passed each run |
| Final native parallel | `cargo test -p cleancopy-clipboard -- --ignored`, a separate isolated default resetting Xvfb | 10 | 10/10 exit 0; all 4 native tests passed each run |
| Clipboard strict lint | `cargo clippy -p cleancopy-clipboard --all-targets -- -D warnings` | 1 | exit 0 |

Final native totals: 80 native test executions passed, none failed. Ten standard runs do **not** exercise ignored native tests; they are reported separately on purpose. Repetition reduces evidence of this flake, not proof that arbitrary future native/X11 races are impossible. macOS and Windows were not tested.

Environment: Linux x86_64; rustc `1.99.1 (58cf8f9c3 2026-10-13)`; cargo `1.99.1 (5f94df478 2026-08-27)`; Xvfb Debian package `2:21.1.16-1.3+deb13u4`. `xvfb-run -a` was used without `-noreset` and without a window manager/clipboard manager. No main desktop display was touched. Each serial/parallel group used its own Xvfb server, shared for its ten process executions. No commits were made by this task.

Capture recipe (all shell runs were from `/workspace/cleancopy`):

```sh
mkdir -p target/g05-evidence
# Baseline before edits:
xvfb-run -a sh -c 'for i in $(seq 1 10); do printf "\n=== baseline %s ===\n" "$i"; cargo test -p cleancopy-clipboard --lib -- --ignored --test-threads=1; printf "EXIT=%s\n" "$?"; done' > target/g05-evidence/baseline.log 2>&1
# Final after edits:
xvfb-run -a sh -c 'for i in $(seq 1 10); do printf "\n=== native serial final %s ===\n" "$i"; cargo test -p cleancopy-clipboard -- --ignored --test-threads=1; printf "EXIT=%s\n" "$?"; done' > target/g05-evidence/native-serial-final.log 2>&1
xvfb-run -a sh -c 'for i in $(seq 1 10); do printf "\n=== native parallel final %s ===\n" "$i"; cargo test -p cleancopy-clipboard -- --ignored; printf "EXIT=%s\n" "$?"; done' > target/g05-evidence/native-parallel-final.log 2>&1
for i in $(seq 1 10); do printf '\n=== standard final %s ===\n' "$i"; cargo test -p cleancopy-clipboard; printf 'EXIT=%s\n' "$?"; done > target/g05-evidence/standard-final.log 2>&1
```

Raw logs are also available locally in `/workspace/cleancopy/target/g05-evidence/`. Embedded copies below preserve the evidence even when the ignored target directory is cleaned.

## Raw capture: baseline.log

SHA-256: `77dffe61a49b42200216f7f9387f09c5aa3d809afe1ad39e33499dc896d02fde`

```text

=== baseline 1 ===
   Compiling rustix v1.1.6
   Compiling gethostname v1.1.0
   Compiling x11rb v0.13.2
   Compiling arboard v3.6.1
   Compiling cleancopy-clipboard v0.1.0 (/workspace/cleancopy/crates/cleancopy-clipboard)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.83s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (112909) panicked at crates/cleancopy-clipboard/src/linux.rs:240:68:
called `Result::unwrap()` on an `Err` value: ClipboardError("Connection reset by peer (os error 104)")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.38s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== baseline 2 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (112941) panicked at crates/cleancopy-clipboard/src/linux.rs:240:68:
called `Result::unwrap()` on an `Err` value: ClipboardError("Connection reset by peer (os error 104)")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.37s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== baseline 3 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (112970) panicked at crates/cleancopy-clipboard/src/linux.rs:273:79:
called `Result::unwrap()` on an `Err` value: ClipboardError("X11 clipboard conversion timed out")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 3.38s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== baseline 4 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (113022) panicked at crates/cleancopy-clipboard/src/linux.rs:240:68:
called `Result::unwrap()` on an `Err` value: ClipboardError("Connection reset by peer (os error 104)")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.37s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== baseline 5 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (113060) panicked at crates/cleancopy-clipboard/src/linux.rs:240:68:
called `Result::unwrap()` on an `Err` value: ClipboardError("Connection reset by peer (os error 104)")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.46s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== baseline 6 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (113101) panicked at crates/cleancopy-clipboard/src/linux.rs:273:79:
called `Result::unwrap()` on an `Err` value: ClipboardError("X11 clipboard conversion timed out")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 3.40s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== baseline 7 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.48s

EXIT=0

=== baseline 8 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.08s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (113205) panicked at crates/cleancopy-clipboard/src/linux.rs:240:68:
called `Result::unwrap()` on an `Err` value: ClipboardError("Connection reset by peer (os error 104)")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.37s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== baseline 9 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (113234) panicked at crates/cleancopy-clipboard/src/linux.rs:240:68:
called `Result::unwrap()` on an `Err` value: ClipboardError("Connection reset by peer (os error 104)")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.38s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== baseline 10 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.09s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.47s

EXIT=0

```

## Raw capture: native-serial.log

SHA-256: `1719356086405fecb52dd33a8a4de272f9f3a6685cd616a1ea57033ec0cc189b`

```text

=== native serial 1 ===
   Compiling cleancopy-clipboard v0.1.0 (/workspace/cleancopy/crates/cleancopy-clipboard)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.95s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial 2 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (115131) panicked at crates/cleancopy-clipboard/src/linux.rs:287:79:
called `Result::unwrap()` on an `Err` value: ClipboardError("X11 clipboard conversion timed out")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 3.29s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== native serial 3 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

failures:

---- linux::retirement_tests::retired_owner_drains_routed_requests_and_incr stdout ----

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (115171) panicked at crates/cleancopy-clipboard/src/linux.rs:287:79:
called `Result::unwrap()` on an `Err` value: ClipboardError("X11 clipboard conversion timed out")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 3.31s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`
EXIT=101

=== native serial 4 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.34s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial 5 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.58s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial 6 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.35s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial 7 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial 8 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial 9 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial 10 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.31s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.58s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

```

## Raw capture: diagnostic.log

SHA-256: `a87694395d45afcb98fbc8aafff753508d5a9f47abc36f2e8ab29b2fa7afff54`

```text
   Compiling cleancopy-clipboard v0.1.0 (/workspace/cleancopy/crates/cleancopy-clipboard)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.41s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 1 test
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.03s

    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 1 test
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.03s

    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 1 test

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (115924) panicked at crates/cleancopy-clipboard/src/linux.rs:289:45:
len=4, stopped=true: X11 clipboard conversion timed out
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED

failures:

failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 11 filtered out; finished in 3.01s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`

```

## Raw capture: diagnostic2.log

SHA-256: `8721c169ea2e55669f6084edbc5bbe20c03a0b74714f5aeaf16434e2b1647425`

```text
   Compiling cleancopy-clipboard v0.1.0 (/workspace/cleancopy/crates/cleancopy-clipboard)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.42s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 1 test
retired owner 4194304 empty

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (117406) panicked at crates/cleancopy-clipboard/src/linux.rs:289:45:
len=4, stopped=false: X11 clipboard conversion timed out
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED

failures:

failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 11 filtered out; finished in 3.00s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`

```

## Raw capture: diagnostic3.log

SHA-256: `9c736d9b8ab1c56b6b0e7c7f61a46b001ac0ca36cbaad6dfac6e024f18d0147c`

```text
   Compiling cleancopy-clipboard v0.1.0 (/workspace/cleancopy/crates/cleancopy-clipboard)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.60s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 1 test
retired owner 4194304 empty
retired owner 4194304 empty
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
retired owner 4194304 empty
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
retired owner 4194304 empty
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.03s

    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 1 test
retired owner 4194304 empty
retired owner 4194304 empty
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
retired owner 4194304 empty
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
retired owner 4194304 empty
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.04s

    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.09s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 1 test
retired owner 4194304 empty
retired owner 4194304 empty
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
retired owner 4194304 empty
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
retired owner 4194304 empty
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.06s

    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 1 test
retired owner 4194304 empty
retired owner 4194304 empty
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
retired owner 4194304 empty
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
owner 4194304 other PropertyNotify(PropertyNotifyEvent { .. })
retired owner 4194304 empty
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.03s

    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 1 test
retired owner 4194304 empty

thread 'linux::retirement_tests::retired_owner_drains_routed_requests_and_incr' (122283) panicked at crates/cleancopy-clipboard/src/linux.rs:289:45:
len=4, stopped=false: X11 clipboard conversion timed out
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... FAILED

failures:

failures:
    linux::retirement_tests::retired_owner_drains_routed_requests_and_incr

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 11 filtered out; finished in 3.00s

error: test failed, to rerun pass `-p cleancopy-clipboard --lib`

```

## Raw capture: standard-final.log

SHA-256: `3e571f20be7b6398701dded5073bcf8e7c64d1404cd62c6ccc930ba75018ec04`

```text

=== standard final 1 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok
test cf_html::tests::empty_byte_offsets ... ok
test cf_html::tests::source_url_byte_offsets ... ok
test cf_html::tests::ascii_byte_offsets ... ok
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::chinese_byte_offsets ... ok
test tests::payload_and_mode_defaults ... ok
test linux::tests::validates_native_targets_without_a_display ... ok
test cf_html::tests::rejects_source_url_header_injection ... ok
test cf_html::tests::emoji_byte_offsets ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== standard final 2 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test cf_html::tests::chinese_byte_offsets ... ok
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::ascii_byte_offsets ... ok
test cf_html::tests::rejects_source_url_header_injection ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::source_url_byte_offsets ... ok
test cf_html::tests::emoji_byte_offsets ... ok
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test cf_html::tests::empty_byte_offsets ... ok
test tests::payload_and_mode_defaults ... ok
test linux::tests::validates_native_targets_without_a_display ... ok
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== standard final 3 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::chinese_byte_offsets ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::rejects_source_url_header_injection ... ok
test cf_html::tests::ascii_byte_offsets ... ok
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test cf_html::tests::source_url_byte_offsets ... ok
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok
test cf_html::tests::empty_byte_offsets ... ok
test cf_html::tests::emoji_byte_offsets ... ok
test tests::payload_and_mode_defaults ... ok
test linux::tests::validates_native_targets_without_a_display ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== standard final 4 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.05s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::ascii_byte_offsets ... ok
test cf_html::tests::rejects_source_url_header_injection ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::emoji_byte_offsets ... ok
test cf_html::tests::chinese_byte_offsets ... ok
test cf_html::tests::source_url_byte_offsets ... ok
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok
test cf_html::tests::empty_byte_offsets ... ok
test tests::payload_and_mode_defaults ... ok
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test linux::tests::validates_native_targets_without_a_display ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== standard final 5 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::rejects_source_url_header_injection ... ok
test cf_html::tests::emoji_byte_offsets ... ok
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok
test cf_html::tests::empty_byte_offsets ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test cf_html::tests::ascii_byte_offsets ... ok
test cf_html::tests::chinese_byte_offsets ... ok
test tests::payload_and_mode_defaults ... ok
test linux::tests::validates_native_targets_without_a_display ... ok
test cf_html::tests::source_url_byte_offsets ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== standard final 6 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::empty_byte_offsets ... ok
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok
test cf_html::tests::source_url_byte_offsets ... ok
test cf_html::tests::chinese_byte_offsets ... ok
test cf_html::tests::ascii_byte_offsets ... ok
test cf_html::tests::emoji_byte_offsets ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test linux::tests::validates_native_targets_without_a_display ... ok
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test cf_html::tests::rejects_source_url_header_injection ... ok
test tests::payload_and_mode_defaults ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== standard final 7 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::emoji_byte_offsets ... ok
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::ascii_byte_offsets ... ok
test cf_html::tests::source_url_byte_offsets ... ok
test cf_html::tests::chinese_byte_offsets ... ok
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok
test linux::tests::validates_native_targets_without_a_display ... ok
test cf_html::tests::empty_byte_offsets ... ok
test tests::payload_and_mode_defaults ... ok
test cf_html::tests::rejects_source_url_header_injection ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== standard final 8 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test cf_html::tests::chinese_byte_offsets ... ok
test cf_html::tests::emoji_byte_offsets ... ok
test cf_html::tests::empty_byte_offsets ... ok
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::ascii_byte_offsets ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::source_url_byte_offsets ... ok
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test cf_html::tests::rejects_source_url_header_injection ... ok
test tests::payload_and_mode_defaults ... ok
test linux::tests::validates_native_targets_without_a_display ... ok
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== standard final 9 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.05s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test cf_html::tests::chinese_byte_offsets ... ok
test cf_html::tests::ascii_byte_offsets ... ok
test cf_html::tests::emoji_byte_offsets ... ok
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::source_url_byte_offsets ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::rejects_source_url_header_injection ... ok
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test tests::payload_and_mode_defaults ... ok
test cf_html::tests::empty_byte_offsets ... ok
test linux::tests::validates_native_targets_without_a_display ... ok
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== standard final 10 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 12 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::emoji_byte_offsets ... ok
test cf_html::tests::ascii_byte_offsets ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ignored, requires an isolated X11 display; replaces clipboard contents
test cf_html::tests::source_url_byte_offsets ... ok
test cf_html::tests::chinese_byte_offsets ... ok
test cf_html::tests::rejects_source_url_header_injection ... ok
test cf_html::tests::respects_existing_fragment_and_body_attributes ... ok
test cf_html::tests::empty_byte_offsets ... ok
test linux::tests::rejects_missing_partial_or_misaligned_properties ... ok
test tests::payload_and_mode_defaults ... ok
test linux::tests::validates_native_targets_without_a_display ... ok

test result: ok. 10 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ignored, requires an isolated X11 display; replaces clipboard contents
test native_modes_and_byte_sizes ... ignored, requires a graphical clipboard; replaces its contents

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

```

## Raw capture: native-serial-final.log

SHA-256: `66c7184c6761d3f531e1c0857597131531b083cc57acc6749d8b8b620c3ebe3d`

```text

=== native serial final 1 ===
   Compiling cleancopy-clipboard v0.1.0 (/workspace/cleancopy/crates/cleancopy-clipboard)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.66s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.32s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial final 2 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.56s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial final 3 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial final 4 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.34s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial final 5 ===
    Blocking waiting for file lock on build directory
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.34s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.48s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.99s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial final 6 ===
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on package cache
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.10s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.34s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial final 7 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.08s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.32s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial final 8 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.34s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial final 9 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.56s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native serial final 10 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.34s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.58s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

```

## Raw capture: native-parallel-final.log

SHA-256: `798ffa90d017db58dbb85cb6719ec5709b0ae3bcd57ba7190970f10896924045`

```text

=== native parallel final 1 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.08s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.32s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native parallel final 2 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test native_modes_and_byte_sizes ... ok
test html_only_snapshot_has_no_synthetic_text_targets ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.58s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native parallel final 3 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.34s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native parallel final 4 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native parallel final 5 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.32s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native parallel final 6 ===
    Blocking waiting for file lock on build directory
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.29s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.33s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test native_modes_and_byte_sizes ... ok
test html_only_snapshot_has_no_synthetic_text_targets ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native parallel final 7 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.32s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test native_modes_and_byte_sizes ... ok
test html_only_snapshot_has_no_synthetic_text_targets ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native parallel final 8 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.32s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test native_modes_and_byte_sizes ... ok
test html_only_snapshot_has_no_synthetic_text_targets ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native parallel final 9 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.32s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test native_modes_and_byte_sizes ... ok
test html_only_snapshot_has_no_synthetic_text_targets ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

=== native parallel final 10 ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running unittests src/lib.rs (target/debug/deps/cleancopy_clipboard-821457396223addd)

running 2 tests
test linux::tests::snapshot_roundtrip_raw_targets_and_incr ... ok
test linux::retirement_tests::retired_owner_drains_routed_requests_and_incr ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.32s

     Running tests/native.rs (target/debug/deps/native-e3c9cb54038564bd)

running 2 tests
test html_only_snapshot_has_no_synthetic_text_targets ... ok
test native_modes_and_byte_sizes ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

   Doc-tests cleancopy_clipboard

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0

```

## Raw capture: clippy.log

SHA-256: `e8c639820f160b395e88acce04ee4c614855131de1ea14038406b97e9f2e9f1b`

```text
    Checking linux-raw-sys v0.12.1
    Checking x11rb-protocol v0.13.2
    Checking smallvec v1.16.4
    Checking log v0.4.35
    Checking parking_lot_core v0.9.12
    Checking parking_lot v0.12.5
    Checking rustix v1.1.6
    Checking gethostname v1.1.0
    Checking x11rb v0.13.2
    Checking arboard v3.6.1
    Checking cleancopy-clipboard v0.1.0 (/workspace/cleancopy/crates/cleancopy-clipboard)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.66s

```
