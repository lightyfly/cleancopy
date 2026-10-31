#!/usr/bin/env python3
"""Drive the built CLI and independent X11 reader on an isolated DISPLAY."""
import hashlib
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "target/debug/cleancopy"
READER = ROOT / "target/debug/examples/x11_read"


def run(argv, data=b"", ok=True):
    result = subprocess.run(argv, input=data, capture_output=True, timeout=30)
    if ok and result.returncode:
        raise RuntimeError(f"{argv}: {result.returncode}: {result.stderr.decode()}")
    return result


def render(kind, data):
    return run([CLI, "--print", kind], data).stdout


def read(target):
    return run([READER, target]).stdout


def compare(target, expected):
    actual = read(target)
    assert actual == expected, (target, len(actual), len(expected))
    print(f"PASS {target}: {len(actual)} bytes sha256={hashlib.sha256(actual).hexdigest()}")


def write(data, mode=None):
    result = run([CLI] + ([mode] if mode else []), data)
    assert result.returncode == 0
    print(f"CLI exited 0 ({mode or 'Multi'}); subsequent requests use a separate process")


def targets():
    names = set(read("TARGETS").decode().splitlines())
    print("TARGETS: " + ", ".join(sorted(names)))
    return names


assert os.environ.get("DISPLAY"), "Run under xvfb-run -a"
small = "# 标题 😀\n\n- **First**\n- [x] 完成\n\nSecond paragraph.\n".encode()
plain = render("plain", small)
html = render("html", small)
markdown = render("markdown", small)
write(small)
names = targets()
required = {"TARGETS", "text/html", "UTF8_STRING", "text/plain;charset=utf-8", "TEXT", "text/markdown"}
assert required <= names, names
for name in names - {"TARGETS"}:
    expected = html if name == "text/html" else markdown if name == "text/markdown" else plain
    compare(name, expected)
print(run([CLI, "inspect"]).stdout.decode(), end="")

# An unsupported conversion must be refused without killing the holder.
refused = run([READER, "application/x-cleancopy-unknown"], ok=False)
assert refused.returncode != 0
print(f"PASS unsupported target refused: {refused.stderr.decode().strip()}")
compare("UTF8_STRING", plain)

# A rejected empty invocation must not replace the current clipboard.
empty = run([CLI], b" \n\t", ok=False)
assert empty.returncode == 1
print(f"PASS empty input exit 1: {empty.stderr.decode().strip()}")
compare("text/markdown", markdown)

for mode, want_html, expected in [
    ("--plain", False, plain),
    ("--html", True, plain),
    ("--markdown", False, markdown),
]:
    write(small, mode)
    names = targets()
    assert ("text/html" in names) == want_html
    assert "text/markdown" not in names
    for name in names - {"TARGETS", "text/html"}:
        compare(name, expected)
    if want_html:
        compare("text/html", html)

# Drive transfers larger than an ordinary X11 property through the real CLI.
large = ("中文 😀 **large**\n\n" * 25000).encode()
write(large)
for name, kind in [("text/html", "html"), ("UTF8_STRING", "plain"), ("text/markdown", "markdown")]:
    compare(name, render(kind, large))
print(run([CLI, "inspect"]).stdout.decode(), end="")

# Replacing a large clipboard must discard previous formats/content.
write(small, "--plain")
assert "text/html" not in targets()
compare("UTF8_STRING", plain)
print("PASS W3 runtime probe: persistence, all advertised targets, mode replacement, refusal, empty input, large transfers")
