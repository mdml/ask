#!/usr/bin/env python3
"""Offline tests for the published model list refresh; dummy keys and a loopback server only."""
import contextlib
import http.server
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch
import urllib.parse

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    "model_list_refresh", Path(__file__).with_name("model-list-refresh.py")
)
refresh = importlib.util.module_from_spec(spec)
spec.loader.exec_module(refresh)

KEYS = {
    "OPENAI_API_KEY": "dummy-openai-key",
    "ANTHROPIC_API_KEY": "dummy-anthropic-key",
    "GEMINI_API_KEY": "dummy gemini/key+1",
    "OPENROUTER_API_KEY": "dummy-openrouter-key",
    "GROQ_API_KEY": "dummy-groq-key",
    "CEREBRAS_API_KEY": "dummy-cerebras-key",
    "XAI_API_KEY": "dummy-xai-key",
}
SECRET_FORMS = [form for key in KEYS.values() for form in (key, refresh.encode(key))]
OPENAI_LIST = {"data": [{"id": "model-2024-08-06"}, {"id": "model"}, {"id": "\u001b[2Jevil"},
                        {"id": "x" * 201}, {"id": "model"}, {"id": "mini-0125"}, {"id": "mini"}]}
ANTHROPIC_PAGES = {
    None: {"data": [{"id": "claude-b-20250514"}, {"id": "claude-a"}], "has_more": True,
           "last_id": "claude-a"},
    "claude-a": {"data": [{"id": "claude-c"}], "has_more": False, "last_id": "claude-c"},
}
GEMINI_PAGES = {
    None: {"models": [{"name": "models/gemini-pro", "supportedGenerationMethods": ["generateContent"]},
                      {"name": "models/embedder", "supportedGenerationMethods": ["embedContent"]}],
           "nextPageToken": "next/1"},
    "next/1": {"models": [{"name": "models/gemini-flash",
                           "supportedGenerationMethods": ["generateContent", "countTokens"]}]},
}
OPENROUTER_LIST = {"data": [
    {"id": "router/text", "architecture": {"output_modalities": ["text"]}},
    {"id": "router/image", "architecture": {"output_modalities": ["image"]}},
    {"id": "router/plain"},
]}


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.server.requests.append((self.path, {k.lower(): v for k, v in self.headers.items()}))
        url = urllib.parse.urlsplit(self.path)
        query = urllib.parse.parse_qs(url.query)
        provider = url.path.split("/")[1]
        behavior = self.server.behavior.get(provider, "ok")
        if behavior == "redirect":
            self.send_response(302)
            self.send_header("Location", f"http://127.0.0.1:{self.server.server_port}/stolen")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if behavior == "error":
            # The body echoes the request, credential included; it must never be shown.
            return self.reply(500, {"error": self.path, "headers": dict(self.headers)})
        if provider == "anthropic":
            return self.reply(200, ANTHROPIC_PAGES[query.get("after_id", [None])[0]])
        if provider == "gemini":
            return self.reply(200, GEMINI_PAGES[query.get("pageToken", [None])[0]])
        if provider == "openrouter":
            return self.reply(200, OPENROUTER_LIST)
        return self.reply(200, OPENAI_LIST)

    def reply(self, status, value):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _format, *_args):
        pass


class ModelListRefreshTests(unittest.TestCase):
    def setUp(self):
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.requests = []
        self.server.behavior = {}
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        base = f"http://127.0.0.1:{self.server.server_port}"
        # Each provider keeps its endpoint's path under a prefix naming it.
        providers = {name: (shape, f"{base}/{name}{urllib.parse.urlsplit(url).path}", variable)
                     for name, (shape, url, variable) in refresh.PROVIDERS.items()}
        patcher = patch.object(refresh, "PROVIDERS", providers)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name) / "models.json"

    def run_refresh(self, *argv, keys=KEYS):
        stdout, stderr = io.StringIO(), io.StringIO()
        environ = {"PATH": os.defpath, **keys}
        with patch.dict(os.environ, environ, clear=True), \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = refresh.main([str(self.output), *argv], environ)
        written = self.output.read_text() if self.output.exists() else ""
        for form in SECRET_FORMS:
            for stream in (stdout.getvalue(), stderr.getvalue(), written):
                self.assertNotIn(form, stream)
        return code, stderr.getvalue(), json.loads(written) if written else None

    def requests_for(self, name):
        return [(path, headers) for path, headers in self.server.requests
                if path.startswith(f"/{name}/")]

    def test_every_provider_is_listed_with_its_own_endpoint_and_credential(self):
        code, report, document = self.run_refresh()
        self.assertEqual(code, 0, report)
        self.assertEqual(document["version"], 1)
        self.assertRegex(document["generated_at"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        for name in ("openai", "groq", "cerebras", "xai"):
            [(path, headers)] = self.requests_for(name)
            self.assertTrue(path.endswith("/models"), path)
            variable = refresh.PROVIDERS[name][2]
            self.assertEqual(headers["authorization"], f"Bearer {KEYS[variable]}")
            self.assertEqual(document["providers"][name], ["model", "mini", "model-2024-08-06", "mini-0125"])
        [(first, headers), (second, _)] = self.requests_for("anthropic")
        self.assertEqual((first, second), ("/anthropic/v1/models", "/anthropic/v1/models?after_id=claude-a"))
        self.assertEqual(headers["x-api-key"], KEYS["ANTHROPIC_API_KEY"])
        self.assertEqual(headers["anthropic-version"], "2023-06-01")
        self.assertNotIn("authorization", headers)
        self.assertEqual(document["providers"]["anthropic"], ["claude-a", "claude-c", "claude-b-20250514"])
        [(first, headers), (second, _)] = self.requests_for("gemini")
        key = "dummy%20gemini%2Fkey%2B1"
        self.assertEqual(first, f"/gemini/v1beta/models?pageSize=1000&key={key}")
        self.assertEqual(second, f"/gemini/v1beta/models?pageSize=1000&pageToken=next%2F1&key={key}")
        self.assertNotIn("authorization", headers)
        self.assertEqual(document["providers"]["gemini"], ["gemini-pro", "gemini-flash"])
        self.assertEqual(document["providers"]["openrouter"], ["router/text", "router/plain"])

    def test_a_missing_key_omits_the_provider_and_fails_unless_partial_is_allowed(self):
        keys = {name: value for name, value in KEYS.items() if name != "XAI_API_KEY"}
        code, report, document = self.run_refresh(keys=keys)
        self.assertEqual(code, 1)
        self.assertIn("xai: omitted (XAI_API_KEY is not set)", report)
        self.assertIsNone(document)
        code, report, document = self.run_refresh("--allow-partial", keys=keys)
        self.assertEqual(code, 0, report)
        self.assertNotIn("xai", document["providers"])
        self.assertIn("openai", document["providers"])
        self.assertEqual(self.requests_for("xai"), [])

    def test_a_failed_request_is_reported_once_without_its_body_or_a_retry(self):
        self.server.behavior = {"gemini": "error", "groq": "error"}
        code, report, document = self.run_refresh("--providers", "gemini,groq,openai", "--allow-partial")
        self.assertEqual(code, 0, report)
        self.assertIn("gemini: omitted (HTTP status 500)", report)
        self.assertIn("groq: omitted (HTTP status 500)", report)
        self.assertEqual(len(self.requests_for("gemini")), 1)
        self.assertEqual(len(self.requests_for("groq")), 1)
        self.assertEqual(list(document["providers"]), ["openai"])

    def test_redirects_are_refused(self):
        self.server.behavior = {"anthropic": "redirect", "gemini": "redirect"}
        code, report, document = self.run_refresh("--providers", "anthropic,gemini")
        self.assertEqual(code, 1)
        self.assertIn("anthropic: omitted (HTTP status 302; redirects are not followed)", report)
        self.assertIn("gemini: omitted (HTTP status 302; redirects are not followed)", report)
        self.assertFalse(any(path.startswith("/stolen") for path, _ in self.server.requests))
        self.assertIsNone(document)

    def test_providers_limits_which_ones_run(self):
        code, report, document = self.run_refresh("--providers", "cerebras")
        self.assertEqual(code, 0, report)
        self.assertEqual(list(document["providers"]), ["cerebras"])
        self.assertEqual({path.split("/")[1] for path, _ in self.server.requests}, {"cerebras"})
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            refresh.build_parser().parse_args([str(self.output), "--providers", "mistral"])

    def test_identifiers_are_sanitized_and_ordered_as_ask_does(self):
        self.assertEqual(
            refresh.arrange(["a-0101", "a-1231", "b", "c-2024-02-30", "d-20241301", "1231", "e-dummy-xai-key"],
                            ["dummy-xai-key"]),
            ["b", "d-20241301", "a-0101", "a-1231", "c-2024-02-30", "1231"],
        )
        self.assertEqual(refresh.arrange(["ok", "model\u202etxt", "mo\u200bdel", "tab\tid", ""], []), ["ok"])


if __name__ == "__main__":
    unittest.main()
