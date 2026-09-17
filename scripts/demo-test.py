#!/usr/bin/env python3
"""Smoke-test the terminal demo recorder using an already-built ask binary."""

import codecs
import hashlib
import importlib.util
import json
import os
import pathlib
import shutil
import signal
import subprocess
import sys
import tempfile


if not __debug__:
    sys.exit("demo proof requires Python assertions")

binary = pathlib.Path(sys.argv[1]).resolve()
script = pathlib.Path(__file__).with_name("demo.py")
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("ask_demo", script)
demo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(demo)
chunks = iter((b"\xe2", b"\x80", b"\xa2$ "))
original_read = demo.os.read
original_write = demo.os.write
original_select = demo.select.select
demo.os.read = lambda _fd, _size: next(chunks)
demo.select.select = lambda *_args: ([0], [], [])
split_events = []
try:
    demo.read_until(0, split_events, demo.time.monotonic(),
                    codecs.getincrementaldecoder("utf-8")(), b"$ ")
finally:
    demo.os.read = original_read
    demo.select.select = original_select
assert "".join(event[2] for event in split_events) == "•$ "

# A single event loop must keep draining output while a slow PTY accepts input.
reads = [b"echoed output", b"$ "]
writes = bytearray()
demo.os.read = lambda _fd, _size: reads.pop(0)
demo.os.write = lambda _fd, value: writes.extend(value) or len(value)
demo.select.select = lambda readable, writable, _errors, _timeout: (
    readable if reads and (len(reads) == 2 or writes == b"slow\n") else [], writable, [])
flow_events = []
try:
    demo.enter_command(0, "slow", flow_events, demo.time.monotonic(),
                       codecs.getincrementaldecoder("utf-8")(), timeout=1, key_delay=0)
finally:
    demo.os.read = original_read
    demo.os.write = original_write
    demo.select.select = original_select
assert writes == b"slow\n"
assert "".join(event[2] for event in flow_events) == "echoed output$ "

try:
    demo.enter_command(0, "stalled", [], demo.time.monotonic(),
                       codecs.getincrementaldecoder("utf-8")(), timeout=0)
    raise AssertionError("stalled command did not time out")
except TimeoutError as error:
    assert "command 'stalled': sent 0/8 bytes; output tail=''" in str(error)

def expect_terminal_failure(incoming, expected_error):
    chunks = list(incoming)
    demo.os.read = lambda _fd, _size: chunks.pop(0) if chunks else b""
    demo.os.write = lambda _fd, value: len(value)
    demo.select.select = lambda readable, writable, *_args: (
        readable if chunks else [], writable, [])
    try:
        demo.enter_command(0, "x", [], demo.time.monotonic(),
                           codecs.getincrementaldecoder("utf-8")(), timeout=0.02, key_delay=0)
    except expected_error:
        return
    finally:
        demo.os.read = original_read
        demo.os.write = original_write
        demo.select.select = original_select
    raise AssertionError(f"terminal did not raise {expected_error.__name__}")


failures = []
for incoming, error in [([b"$ "], TimeoutError), ([b""], EOFError)]:
    try:
        expect_terminal_failure(incoming, error)
    except Exception as failure:
        failures.append(f"{error.__name__}: {failure!r}")
assert not failures, failures

with tempfile.TemporaryDirectory(prefix="ask-demo-test-") as temporary:
    renamed_binary = pathlib.Path(temporary) / "renamed-executable"
    shutil.copy2(binary, renamed_binary)
    output = pathlib.Path(temporary) / "assets"
    rejected = pathlib.Path(temporary) / "rejected"
    invalid = subprocess.run(
        [sys.executable, str(script), str(renamed_binary), "--output-dir", str(rejected),
         "--agg", str(renamed_binary)], capture_output=True, timeout=30)
    assert invalid.returncode != 0
    assert not rejected.exists(), "invalid renderer changed recording assets"
    process = subprocess.Popen(
        [sys.executable, str(script), str(renamed_binary), "--output-dir", str(output)],
        start_new_session=True)
    try:
        returncode = process.wait(timeout=45)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGINT)
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        raise
    if returncode != 0:
        raise subprocess.CalledProcessError(returncode, process.args)
    lines = (output / "demo.cast").read_text(encoding="utf-8").splitlines()
    header = json.loads(lines[0])
    assert (header["version"], header["width"], header["height"], header["env"]["SHELL"]) == (2, 88, 24, "/bin/bash")
    assert header["ask_binary_sha256"] == hashlib.sha256(renamed_binary.read_bytes()).hexdigest()
    events = [json.loads(line) for line in lines[1:]]
    assert all(len(event) == 3 and event[1] == "o" for event in events)
    assert all(events[index][0] <= events[index + 1][0] for index in range(len(events) - 1))
    transcript = (output / "demo.txt").read_text(encoding="utf-8")
    for expected in ("Report the median latency", "The median latency is 100 ms.",
                     "Which request was faster?", "The 80 ms request was faster.",
                     "ask thread", "Deterministic local fixture"):
        assert expected in transcript, expected
    assert "\x1b" not in transcript
    assert str(pathlib.Path.home()) not in transcript
print("demo recorder: passed")
