#!/usr/bin/env bash
# Native runtime matrix; no internal engine/unit tests are invoked.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
exec > >(tee /tmp/cleancopy-e2e.log) 2>&1
printf 'CleanCopy desktop E2E %s\n' "$(date -Is)"
for tool in cargo Xvfb xfwm4 xdotool dbus-run-session; do
  command -v "$tool" >/dev/null || { printf 'Missing dependency: %s\n' "$tool"; exit 1; }
done
cargo build -p cleancopy-desktop --bin cleancopy-app --example target --example screenshot
# A Rust ignored integration test is the process-driving harness, not a unit test.
dbus-run-session -- cargo test -p cleancopy-desktop --test desktop_e2e -- --ignored --nocapture --test-threads=1
