#!/usr/bin/env python3
"""Record and render the deterministic terminal demo."""

import argparse
import codecs
import hashlib
import http.server
import json
import os
import pathlib
import pty
import re
import select
import shutil
import signal
import subprocess
import tempfile
import termios
import threading
import time


AGG_SHA256 = "ddcbf6ca044c8ac3a434dcb9ee89fb9e3be87209982b7c2adb55f782e8f0f390"
FONT = pathlib.Path("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")
FONT_SHA256 = "c805f9436dbc268644c1d9584f01a601a653e028e08fd74b9b949f6cf8304d88"
COLS = 88
ROWS = 24
ANSI = re.compile(rb"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))")


class Provider(http.server.ThreadingHTTPServer):
    allow_reuse_address = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), Handler)
        self.answers = iter((
            "The median latency is 100 ms.",
            "The 80 ms request was faster.",
        ))
        self.requests = []


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_POST(self):
        length = int(self.headers["Content-Length"])
        self.server.requests.append(json.loads(self.rfile.read(length)))
        answer = next(self.server.answers)
        events = [
            {"id": "demo", "object": "chat.completion.chunk",
             "choices": [{"index": 0, "delta": {"content": answer},
                          "finish_reason": None}]},
            {"id": "demo", "object": "chat.completion.chunk",
             "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
             "usage": {"prompt_tokens": 12, "completion_tokens": 7, "total_tokens": 19}},
        ]
        body = "".join(f"data: {json.dumps(event)}\n\n" for event in events)
        encoded = (body + "data: [DONE]\n\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(encoded)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, _format, *_args):
        pass


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def terminate_process_group(process, grace=5):
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=grace)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def read_until(master, events, started, decoder, marker, timeout=10):
    collected = b""
    deadline = time.monotonic() + timeout
    while marker not in collected:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([master], [], [], remaining)[0]:
            raise TimeoutError(f"terminal did not emit {marker!r}")
        chunk = os.read(master, 65536)
        collected += chunk
        text = decoder.decode(chunk)
        if text:
            events.append([round(time.monotonic() - started, 6), "o", text])
    return collected


def type_command(master, command):
    for byte in command.encode():
        os.write(master, bytes((byte,)))
        time.sleep(0.018)
    os.write(master, b"\n")


def enter_command(master, command, events, started, decoder):
    writer = threading.Thread(target=type_command, args=(master, command), daemon=True)
    writer.start()
    try:
        read_until(master, events, started, decoder, b"$ ")
    finally:
        writer.join(timeout=5)
    time.sleep(1.5)


def record(binary, output_dir):
    binary = binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise SystemExit(f"not an executable: {binary}")
    output_dir.mkdir(parents=True, exist_ok=True)
    provider = Provider()
    serving = threading.Thread(target=provider.serve_forever, daemon=True)
    serving.start()
    try:
        with tempfile.TemporaryDirectory(prefix="ask-demo-") as temporary:
            private_bin = pathlib.Path(temporary) / "bin"
            private_bin.mkdir()
            (private_bin / "ask").symlink_to(binary)
            home = pathlib.Path(temporary) / "home"
            home.mkdir()
            config = (f'default_profile = "demo"\n\n[providers.fixture]\n'
                      f'kind = "openai-compatible"\nbase_url = "http://127.0.0.1:{provider.server_port}/v1"\n'
                      'api_key_env = "DEMO_API_KEY"\n\n[profiles.demo]\n'
                      'provider = "fixture"\nmodel = "fixture-model"\nsystem_prompt = "Answer briefly."\n')
            (home / "config.toml").write_text(config, encoding="utf-8")
            env = {"PATH": f"{private_bin}:/usr/bin:/bin", "ASK_HOME": str(home),
                   "DEMO_API_KEY": "local-fixture", "NO_PROXY": "127.0.0.1",
                   "HOME": temporary, "LANG": "C.UTF-8", "TERM": "xterm-256color",
                   "PS1": "$ "}
            if "LLVM_PROFILE_FILE" in os.environ:
                env["LLVM_PROFILE_FILE"] = os.environ["LLVM_PROFILE_FILE"]
            master, slave = pty.openpty()
            termios.tcsetwinsize(slave, (ROWS, COLS))
            process = subprocess.Popen(
                ["/bin/bash", "--noprofile", "--norc", "-i"], stdin=slave,
                stdout=slave, stderr=slave,
                cwd=temporary, env=env, start_new_session=True,
                preexec_fn=lambda: signal.signal(signal.SIGINT, signal.SIG_DFL))
            os.close(slave)
            events = []
            started = time.monotonic()
            decoder = codecs.getincrementaldecoder("utf-8")()
            try:
                read_until(master, events, started, decoder, b"$ ")
                enter_command(master, "# Deterministic local fixture; timings are not provider performance.",
                              events, started, decoder)
                commands = (
                    "printf 'latency: 120ms\\nlatency: 80ms\\n' | ask 'Report the median latency'",
                    "ask reply 'Which request was faster?'",
                    "ask thread",
                )
                for command in commands:
                    enter_command(master, command, events, started, decoder)
                type_command(master, "exit")
                process.wait(timeout=5)
            finally:
                os.close(master)
                terminate_process_group(process)
            tail = decoder.decode(b"", final=True)
            if tail:
                events.append([round(time.monotonic() - started, 6), "o", tail])
            if process.returncode != 0:
                raise RuntimeError(f"demo shell exited {process.returncode}")
            if len(provider.requests) != 2:
                raise RuntimeError(f"expected two fixture requests, got {len(provider.requests)}")
            reply_messages = provider.requests[1]["messages"]
            if not any(message.get("content") == "The median latency is 100 ms."
                       or message.get("content") == [{"type": "text", "text": "The median latency is 100 ms."}]
                       for message in reply_messages):
                raise RuntimeError("reply did not include the first fixture answer")

            header = {"version": 2, "width": COLS, "height": ROWS,
                      "env": {"SHELL": "/bin/bash", "TERM": "xterm-256color"},
                      "ask_binary_sha256": digest(binary),
                      "title": "ask deterministic happy-path demo"}
            cast = output_dir / "demo.cast"
            with cast.open("w", encoding="utf-8", newline="\n") as target:
                target.write(json.dumps(header, separators=(",", ":")) + "\n")
                for event in events:
                    target.write(json.dumps(event, separators=(",", ":")) + "\n")
            stream = b"".join(event[2].encode() for event in events)
            transcript = ANSI.sub(b"", stream).replace(b"\r\n", b"\n").replace(b"\r", b"")
            transcript = b"\n".join(line.rstrip() for line in transcript.splitlines()) + b"\n"
            (output_dir / "demo.txt").write_bytes(transcript)
    finally:
        provider.shutdown()
        provider.server_close()
        serving.join(timeout=5)


def validate_renderer(agg):
    if digest(agg) != AGG_SHA256:
        raise SystemExit(f"agg SHA-256 does not match approved {AGG_SHA256}")
    if digest(FONT) != FONT_SHA256:
        raise SystemExit(f"font SHA-256 does not match recorded {FONT_SHA256}")


def render(agg, output_dir):
    agg = agg.resolve()
    validate_renderer(agg)
    cast = (output_dir / "demo.cast").resolve()
    clean_env = {"PATH": "/usr/bin", "HOME": "/tmp", "LANG": "C.UTF-8"}
    with tempfile.TemporaryDirectory(prefix="ask-demo-render-") as scratch:
        command = ["bwrap", "--die-with-parent", "--unshare-net", "--dir", "/usr",
                   "--dir", "/usr/bin", "--dir", "/usr/share", "--dir", "/etc",
                   "--dir", "/var", "--dir", "/var/cache",
                   "--ro-bind", str(agg), "/usr/bin/agg", "--ro-bind", str(cast), "/input.cast",
                   "--ro-bind", "/usr/share/fonts", "/usr/share/fonts",
                   "--ro-bind", "/etc/fonts", "/etc/fonts",
                   "--ro-bind", "/var/cache/fontconfig", "/var/cache/fontconfig",
                   "--tmpfs", "/tmp", "--dir", "/output", "--bind", scratch, "/output",
                   "/usr/bin/agg", "--quiet", "--font-family", "DejaVu Sans Mono",
                   "--font-size", "16", "--line-height", "1.35", "--theme", "github-dark",
                   "--cols", str(COLS), "--rows", str(ROWS), "--idle-time-limit", "1.5",
                   "--last-frame-duration", "3", "/input.cast", "/output/demo.gif"]
        process = subprocess.Popen(command, env=clean_env, start_new_session=True)
        try:
            returncode = process.wait(timeout=120)
        except subprocess.TimeoutExpired:
            terminate_process_group(process)
            raise
        if returncode != 0:
            raise subprocess.CalledProcessError(returncode, command)
        output_dir.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(dir=output_dir, prefix=".demo-", suffix=".gif",
                                         delete=False) as staged:
            staged_path = pathlib.Path(staged.name)
            with (pathlib.Path(scratch) / "demo.gif").open("rb") as rendered:
                shutil.copyfileobj(rendered, staged)
        staged_path.chmod(0o644)
        os.replace(staged_path, output_dir / "demo.gif")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="already-built ask executable")
    parser.add_argument("--output-dir", type=pathlib.Path,
                        default=pathlib.Path("docs/assets/demo"))
    parser.add_argument("--agg", type=pathlib.Path,
                        help="approved agg 1.9.0 executable; omit to record without rendering")
    args = parser.parse_args()
    if args.agg:
        validate_renderer(args.agg)
    record(args.binary, args.output_dir)
    if args.agg:
        render(args.agg, args.output_dir)


if __name__ == "__main__":
    main()
