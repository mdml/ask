#!/usr/bin/env python3
"""Repeatable offline owner walkthrough for an already-built ask binary."""

import argparse
import http.server
import json
import os
import pathlib
import pty
import signal
import subprocess
import sys
import tempfile
import threading


if not __debug__:
    raise SystemExit("offline acceptance requires Python assertions")


def timed_out(_signal, _frame):
    raise TimeoutError("offline acceptance exceeded 60 seconds")


signal.signal(signal.SIGALRM, timed_out)
signal.alarm(60)


ANSWERS = [
    "first answer",
    "multiline answer",
    "captured profile reply",
    "second thread answer",
    "switched thread reply",
    "#!/bin/sh\nprintf 'fixture ran\\n'",
    "piped answer",
]


class Provider(http.server.ThreadingHTTPServer):
    allow_reuse_address = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), Handler)
        self.requests = []
        self.authorizations = []
        self.answers = iter(ANSWERS)


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_POST(self):
        length = int(self.headers["Content-Length"])
        request = json.loads(self.rfile.read(length))
        self.server.requests.append(request)
        self.server.authorizations.append(self.headers.get("Authorization"))
        answer = next(self.server.answers)
        events = [
            {"id": "offline", "object": "chat.completion.chunk",
             "choices": [{"index": 0, "delta": {"content": answer}, "finish_reason": None}]},
            {"id": "offline", "object": "chat.completion.chunk",
             "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
             "usage": {"prompt_tokens": 2, "completion_tokens": 2, "total_tokens": 4}},
        ]
        body = "".join(f"data: {json.dumps(event)}\n\n" for event in events)
        body += "data: [DONE]\n\n"
        encoded = body.encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(encoded)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, _format, *_args):
        pass


def run(binary, env, *args, stdin=None, code=0):
    result = subprocess.run([binary, *args], input=stdin, capture_output=True, env=env,
                            timeout=10)
    assert result.returncode == code, (args, result.returncode, result.stdout, result.stderr)
    return result


def terminal_query(binary, env):
    master, slave = pty.openpty()
    child = None
    try:
        child = subprocess.Popen([binary, "new", "--profile", "terse"], stdin=slave,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env,
                                 preexec_fn=lambda: signal.signal(signal.SIGINT, signal.SIG_DFL))
        assert child.stderr.read(5) == b"ask> "
        os.write(master, b"first line\nsecond line\n\x04")
        stdout, stderr = child.communicate(timeout=10)
        assert child.returncode == 0, stderr
        assert stdout == b"multiline answer\n", stdout
    finally:
        os.close(slave)
        os.close(master)
        if child is not None and child.poll() is None:
            child.kill()
            child.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="built ask executable")
    args = parser.parse_args()
    binary = str(args.binary.resolve())
    assert os.path.isfile(binary) and os.access(binary, os.X_OK), f"not executable: {binary}"

    provider = Provider()
    serving = threading.Thread(target=provider.serve_forever, daemon=True)
    serving.start()
    try:
        with tempfile.TemporaryDirectory(prefix="ask-offline-acceptance-") as temporary:
            home = pathlib.Path(temporary)
            env = {"PATH": os.defpath, "ASK_HOME": str(home),
                   "LOCAL_API_KEY": "offline-fixture", "NO_PROXY": "127.0.0.1"}
            if "LLVM_PROFILE_FILE" in os.environ:
                env["LLVM_PROFILE_FILE"] = os.environ["LLVM_PROFILE_FILE"]
            answers = (f"5\nlocal\nhttp://127.0.0.1:{provider.server_port}/v1\n"
                       "LOCAL_API_KEY\nterse-model\nBe terse.\n\ny\n").encode()
            initialized = run(binary, env, "init", stdin=answers)
            assert not initialized.stdout
            candidate = home / "config.toml"
            checked = run(binary, env, "configure", "check", str(candidate))
            assert not checked.stdout and b"valid configuration" in checked.stderr
            doctor = run(binary, env, "doctor")
            assert b"configuration: valid" in doctor.stdout and not doctor.stderr

            config = (f"default_profile = \"terse\"\n\n[providers.local]\n"
                      "kind = \"openai-compatible\"\n"
                      f"base_url = \"http://127.0.0.1:{provider.server_port}/v1\"\n"
                      "api_key_env = \"LOCAL_API_KEY\"\n\n"
                      "[profiles.terse]\nprovider = \"local\"\nmodel = \"terse-model\"\n"
                      "system_prompt = \"Be terse.\"\n\n"
                      "[profiles.detailed]\nprovider = \"local\"\nmodel = \"detailed-model\"\n"
                      "system_prompt = \"Be detailed.\"\n")
            applied = run(binary, env, "configure", "apply", "-", stdin=config.encode())
            assert not applied.stdout

            first = run(binary, env, "new", "--profile", "detailed", "first question")
            assert first.stdout == b"first answer\n"
            terminal_query(binary, env)
            changed = config.replace("terse-model", "changed-model")
            run(binary, env, "configure", "apply", "-", stdin=changed.encode())
            reply = run(binary, env, "reply", "uses captured profile")
            assert reply.stdout == b"captured profile reply\n"
            assert provider.requests[0]["model"] == "detailed-model"
            assert provider.requests[2]["model"] == "terse-model"

            run(binary, env, "new", "second thread")
            run(binary, env, "switch", "1")
            switched = run(binary, env, "reply", "after switch")
            assert switched.stdout == b"switched thread reply\n"
            messages = provider.requests[4]["messages"]
            assert any(message.get("content") == "first question" for message in messages)
            assert any(message.get("role") == "assistant" and
                       (message.get("content") == "first answer" or
                        message.get("content") == [{"type": "text", "text": "first answer"}])
                       for message in messages), messages
            thread = run(binary, env, "thread")
            assert b"first question" in thread.stdout and b"after switch" in thread.stdout

            fixture = home / "answer-fixture.sh"
            with fixture.open("wb") as output:
                saved = subprocess.run([binary, "new", "write a benign fixture"], stdout=output,
                                       stderr=subprocess.PIPE, env=env, timeout=10)
            assert saved.returncode == 0, saved.stderr
            executed = subprocess.run(["/bin/sh", str(fixture)], capture_output=True,
                                      timeout=10, env=env)
            assert executed.returncode == 0 and executed.stdout == b"fixture ran\n"

            query = subprocess.Popen([binary, "new", "pipe check"], stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, env=env)
            try:
                consumer = subprocess.run(
                    [sys.executable, "-c", "import sys; sys.stdout.buffer.write(sys.stdin.buffer.read())"],
                    stdin=query.stdout, capture_output=True, timeout=10, env=env)
                query.stdout.close()
                stderr = query.communicate(timeout=10)[1]
            finally:
                if query.poll() is None:
                    query.kill()
                    query.communicate()
            assert query.returncode == 0 and consumer.stdout == b"piped answer\n"
            assert b"ask: " in stderr and b"piped answer" not in stderr

            assert len(provider.requests) == len(ANSWERS), len(provider.requests)
            assert provider.authorizations == ["Bearer offline-fixture"] * len(ANSWERS)
            print("offline acceptance: passed")
            print("follow-up: arrow-key selection and interactive conversation display await presentation changes")
    finally:
        provider.shutdown()
        provider.server_close()
        serving.join(timeout=5)


if __name__ == "__main__":
    main()
