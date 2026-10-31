#!/usr/bin/env python3
"""Check paste representations, not GUI apps, on an isolated Xvfb DISPLAY."""
import difflib
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "target/debug/cleancopy"
READER = ROOT / "target/debug/examples/x11_read"


def run(args):
    return subprocess.run(args, check=True, capture_output=True, timeout=30).stdout


assert os.environ.get("DISPLAY"), "Run under xvfb-run -a"
source = (ROOT / "fixtures/kitchen-sink.md").read_bytes()
# This fixture has no ANSI sequences or CRLF; Markdown mode trims its final LF.
expected = source.strip()
rendered = run([CLI, "--print", "markdown", ROOT / "fixtures/kitchen-sink.md"])
assert rendered == expected
run([CLI, "--markdown", ROOT / "fixtures/kitchen-sink.md"])
markdown = run([READER, "UTF8_STRING"])
assert markdown == rendered
with tempfile.TemporaryDirectory(prefix="cleancopy-w4-") as directory:
    saved = Path(directory) / "pasted.md"
    saved.write_bytes(markdown)
    result = subprocess.run(
        ["diff", "-u", str(ROOT / "fixtures/kitchen-sink.md"), str(saved)],
        capture_output=True, timeout=30,
    )
    assert result.returncode == 1  # Expected final-newline-only difference.
    print("Raw fixture diff (expected final newline removal):")
    print(result.stdout.decode(), end="")
assert not list(difflib.unified_diff(expected.decode().splitlines(True), markdown.decode().splitlines(True)))
print(f"PASS Markdown normalized diff: {len(markdown)} bytes sha256={hashlib.sha256(markdown).hexdigest()}")
plain = run([CLI, "--print", "plain", ROOT / "fixtures/kitchen-sink.md"])
run([CLI, "--plain", ROOT / "fixtures/kitchen-sink.md"])
actual = run([READER, "UTF8_STRING"])
assert actual == plain
assert actual.endswith(b"\n")
assert "1. 第一项 apple" in actual.decode()
print(f"PASS terminal plain representation: {len(actual)} bytes sha256={hashlib.sha256(actual).hexdigest()}")
print("No GUI editor or terminal paste interaction was exercised.")
