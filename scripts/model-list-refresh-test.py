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
import urllib.request

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
LOOPBACK_HOSTS = "127.0.0.1,localhost"
SECRET_FORMS = [form for key in KEYS.values() for form in (key, refresh.encode(key))]
# The last five are listed but not text-only chat models with tool calling in the catalog.
NOT_CHAT = ["speech", "painter", "mixed", "no-tools", "unknown-tools"]
OPENAI_LIST = {"data": [{"id": "model-2024-08-06"}, {"id": "model"}, {"id": "\u001b[2Jevil"},
                        {"id": "x" * 201}, {"id": "model"}, {"id": "mini-0125"}, {"id": "mini"},
                        *({"id": model} for model in NOT_CHAT)]}
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

CHAT = {"modalities": {"input": ["text", "image"], "output": ["text"]}, "tool_call": True}


def catalog_models(*chat):
    """A catalog provider entry describing `chat` as chat models, plus the non-chat cases."""
    models = {model: CHAT for model in chat}
    models.update({
        "painter": {"modalities": {"output": ["image"]}, "tool_call": True},
        "mixed": {"modalities": {"output": ["text", "image"]}, "tool_call": True},
        "no-tools": {"modalities": {"output": ["text"]}, "tool_call": False},
        "unknown-tools": {"modalities": {"output": ["text"]}},
    })
    return {"id": "provider", "models": models}


OPENAI_CHAT = ("model", "mini", "model-2024-08-06", "mini-0125", "\u001b[2Jevil")
CATALOG = {
    "openai": catalog_models(*OPENAI_CHAT),
    "anthropic": catalog_models("claude-a", "claude-b-20250514", "claude-c"),
    # The `gemini` preset is the catalog's `google` provider.
    "google": catalog_models("gemini-pro", "gemini-flash", "embedder"),
    "openrouter": catalog_models("router/text", "router/plain", "router/image"),
    "groq": catalog_models(*OPENAI_CHAT),
    "cerebras": catalog_models(*OPENAI_CHAT),
    # xAI's catalog lacks `mini`, so curation is per provider.
    "xai": catalog_models("model", "model-2024-08-06", "mini-0125"),
}
CURATED = ["model", "mini", "model-2024-08-06", "mini-0125"]


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
        if behavior == "bad-cursor":
            # A lone surrogate, which JSON allows but a URL cannot carry.
            cursor = "after\ud800"
            return self.reply(200, {"data": [{"id": "claude-a"}], "has_more": True, "last_id": cursor,
                                    "models": GEMINI_PAGES[None]["models"], "nextPageToken": cursor})
        if provider in ("catalog", "mirror"):
            if behavior == "invalid":
                return self.reply(200, None, b"{not json")
            if behavior == "error":
                return self.reply(500, {"error": "unavailable"})
            return self.reply(200, self.server.catalog)
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

    def reply(self, status, value, body=None):
        body = json.dumps(value).encode() if body is None else body
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
        self.server.catalog = json.loads(json.dumps(CATALOG))
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        base = f"http://127.0.0.1:{self.server.server_port}"
        # Each provider keeps its endpoint's path under a prefix naming it.
        providers = {name: (shape, f"{base}/{name}{urllib.parse.urlsplit(url).path}", variable)
                     for name, (shape, url, variable) in refresh.PROVIDERS.items()}
        for name, value in (("PROVIDERS", providers), ("CATALOG_URL", f"{base}/catalog/api.json")):
            patcher = patch.object(refresh, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name) / "models.json"

    def run_refresh(self, *argv, keys=KEYS):
        stdout, stderr = io.StringIO(), io.StringIO()
        # The cleared environment drops every ambient proxy variable, and
        # `no_proxy` keeps the loopback requests direct even where urllib would
        # otherwise fall back to the macOS system proxy settings.
        environ = {"PATH": os.defpath, "no_proxy": LOOPBACK_HOSTS, **keys}
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
            curated = [model for model in CURATED if name != "xai" or model != "mini"]
            self.assertEqual(document["providers"][name], curated)
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
        self.assertIn("model catalog: http://127.0.0.1:", report)
        self.assertIn("openai: 4 identifiers of 9 listed", report)

    def test_the_catalog_is_fetched_once_without_any_credential(self):
        code, report, _ = self.run_refresh()
        self.assertEqual(code, 0, report)
        [(path, headers)] = self.requests_for("catalog")
        self.assertEqual(path, "/catalog/api.json")
        self.assertFalse({"authorization", "x-api-key", "anthropic-version"} & set(headers))
        sent = path + json.dumps(headers)
        for form in SECRET_FORMS:
            self.assertNotIn(form, sent)

    def test_the_catalog_request_identifies_the_script(self):
        # The catalog's host rejects the default urllib user agent with HTTP 403.
        code, report, _ = self.run_refresh()
        self.assertEqual(code, 0, report)
        [(_, headers)] = self.requests_for("catalog")
        self.assertEqual(headers.get("user-agent"), refresh.USER_AGENT)
        self.assertNotIn("python", headers["user-agent"].lower())

    def test_every_provider_request_identifies_the_script(self):
        # Some providers' hosts reject the default urllib user agent with HTTP 403.
        code, report, _ = self.run_refresh()
        self.assertEqual(code, 0, report)
        self.assertGreater(len(self.server.requests), len(refresh.PROVIDERS))
        for path, headers in self.server.requests:
            self.assertEqual(headers.get("user-agent"), refresh.USER_AGENT, path)

    def test_only_text_only_chat_models_with_tool_calling_are_kept(self):
        code, report, document = self.run_refresh()
        self.assertEqual(code, 0, report)
        for name in ("openai", "groq", "cerebras", "xai"):
            for model in NOT_CHAT:
                self.assertNotIn(model, document["providers"][name])
        self.assertNotIn("mini", self.server.catalog["xai"]["models"])
        self.assertEqual(document["providers"]["xai"], ["model", "model-2024-08-06", "mini-0125"])

    def test_the_gemini_preset_is_curated_by_the_catalog_google_provider(self):
        self.server.catalog["gemini"] = catalog_models()
        self.server.catalog["google"] = catalog_models("gemini-flash")
        code, report, document = self.run_refresh("--providers", "gemini")
        self.assertEqual(code, 0, report)
        self.assertEqual(document["providers"]["gemini"], ["gemini-flash"])

    def test_a_catalog_url_option_selects_a_mirror(self):
        code, report, _ = self.run_refresh("--providers", "openai", "--catalog-url",
                                           f"http://127.0.0.1:{self.server.server_port}/mirror/models.json")
        self.assertEqual(code, 0, report)
        self.assertEqual([path for path, _ in self.requests_for("mirror")], ["/mirror/models.json"])
        self.assertEqual(self.requests_for("catalog"), [])

    def test_a_catalog_failure_omits_every_provider_without_listing_any(self):
        cases = {"error": "HTTP status 500", "redirect": "HTTP status 302; redirects are not followed",
                 "invalid": "response is not valid JSON"}
        for behavior, reason in cases.items():
            with self.subTest(behavior):
                self.server.requests.clear()
                self.server.behavior = {"catalog": behavior}
                for partial in ((), ("--allow-partial",)):
                    code, report, document = self.run_refresh("--providers", "openai,xai", *partial)
                    self.assertEqual(code, 1)
                    self.assertIn(f"openai: omitted (model catalog unavailable: {reason})", report)
                    self.assertIn(f"xai: omitted (model catalog unavailable: {reason})", report)
                    self.assertIn("nothing was written", report)
                    self.assertIsNone(document)
                self.assertEqual({path.split("/")[1] for path, _ in self.server.requests}, {"catalog"})
                self.assertFalse(any(path.startswith("/stolen") for path, _ in self.server.requests))

    def test_an_oversized_catalog_is_refused(self):
        self.assertEqual(refresh.MAX_CATALOG_BYTES, 32 * 1024 * 1024)
        size = len(json.dumps(self.server.catalog).encode())
        with patch.object(refresh, "MAX_CATALOG_BYTES", size - 1):
            code, report, document = self.run_refresh("--providers", "openai")
        self.assertEqual(code, 1)
        self.assertIn("openai: omitted (model catalog unavailable: response is too large)", report)
        self.assertIsNone(document)
        with patch.object(refresh, "MAX_CATALOG_BYTES", size):
            code, report, _ = self.run_refresh("--providers", "openai")
        self.assertEqual(code, 0, report)

    def test_a_catalog_that_is_not_an_object_omits_every_provider(self):
        self.server.catalog = [CATALOG]
        code, report, document = self.run_refresh("--providers", "openai")
        self.assertEqual(code, 1)
        self.assertIn("openai: omitted (model catalog unavailable: model catalog has an unexpected shape)",
                      report)
        self.assertIsNone(document)

    def test_a_provider_missing_or_malformed_in_the_catalog_is_omitted(self):
        del self.server.catalog["groq"]
        self.server.catalog["cerebras"] = {"models": list(OPENAI_CHAT)}
        self.server.catalog["xai"] = [catalog_models(*OPENAI_CHAT)]
        code, report, document = self.run_refresh("--providers", "openai,groq,cerebras,xai")
        self.assertEqual(code, 1)
        self.assertIn("groq: omitted (missing from the model catalog)", report)
        self.assertIn("cerebras: omitted (unexpected shape in the model catalog)", report)
        self.assertIn("xai: omitted (unexpected shape in the model catalog)", report)
        self.assertIsNone(document)
        code, report, document = self.run_refresh("--providers", "openai,groq,cerebras,xai",
                                                  "--allow-partial")
        self.assertEqual(code, 0, report)
        self.assertEqual(list(document["providers"]), ["openai"])

    def test_malformed_catalog_entries_are_not_chat_models(self):
        self.server.catalog["openai"]["models"].update({
            "model": "chat", "mini": {"modalities": ["text"], "tool_call": True},
            "mini-0125": {"modalities": {"output": "text"}, "tool_call": True},
            "model-2024-08-06": {"modalities": {"output": ["text"]}, "tool_call": "true"},
        })
        self.server.catalog["openai"]["models"]["speech"] = CHAT
        code, report, document = self.run_refresh("--providers", "openai")
        self.assertEqual(code, 0, report)
        self.assertEqual(document["providers"]["openai"], ["speech"])

    def test_a_list_left_empty_by_curation_is_omitted(self):
        self.server.catalog["groq"] = catalog_models("elsewhere")
        code, report, document = self.run_refresh("--providers", "openai,groq")
        self.assertEqual(code, 1)
        self.assertIn("groq: omitted (no listed identifier is a text-only chat model with tool calling "
                      "in the model catalog)", report)
        self.assertIsNone(document)
        code, report, document = self.run_refresh("--providers", "openai,groq", "--allow-partial")
        self.assertEqual(code, 0, report)
        self.assertEqual(list(document["providers"]), ["openai"])

    def test_no_catalog_publishes_the_uncurated_lists_and_says_so(self):
        code, report, document = self.run_refresh("--providers", "openai", "--no-catalog")
        self.assertEqual(code, 0, report)
        self.assertIn("model catalog: not used (--no-catalog); the lists are not curated", report)
        self.assertEqual(document["providers"]["openai"], ["model", "mini", *NOT_CHAT, "model-2024-08-06",
                                                           "mini-0125"])
        self.assertIn("openai: 9 identifiers\n", report)
        self.assertEqual(self.requests_for("catalog"), [])
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            refresh.build_parser().parse_args([str(self.output), "--no-catalog", "--catalog-url", "x"])

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

    def test_a_cursor_that_is_not_valid_text_omits_only_that_provider(self):
        self.server.behavior = {"anthropic": "bad-cursor", "gemini": "bad-cursor"}
        code, report, document = self.run_refresh("--providers", "anthropic,gemini,openai")
        self.assertEqual(code, 1)
        self.assertIn("anthropic: omitted (response has a malformed page cursor)", report)
        self.assertIn("gemini: omitted (response has a malformed page cursor)", report)
        self.assertIn("omitted: anthropic, gemini; nothing was written", report)
        self.assertIsNone(document)
        code, report, document = self.run_refresh("--providers", "anthropic,gemini,openai", "--allow-partial")
        self.assertEqual(code, 0, report)
        self.assertEqual(list(document["providers"]), ["openai"])
        self.assertEqual(len(self.requests_for("anthropic")), 2)
        self.assertEqual(len(self.requests_for("gemini")), 2)

    def test_redirects_are_refused(self):
        self.server.behavior = {"anthropic": "redirect", "gemini": "redirect"}
        code, report, document = self.run_refresh("--providers", "anthropic,gemini")
        self.assertEqual(code, 1)
        self.assertIn("anthropic: omitted (HTTP status 302; redirects are not followed)", report)
        self.assertIn("gemini: omitted (HTTP status 302; redirects are not followed)", report)
        self.assertFalse(any(path.startswith("/stolen") for path, _ in self.server.requests))
        self.assertIsNone(document)

    def test_ambient_and_system_proxies_never_carry_the_loopback_requests(self):
        # A proxy that accepts no connection; any request routed to it fails.
        unreachable = "http://127.0.0.1:9"
        # On macOS, urllib falls back to the system proxy settings when the
        # environment names no proxy; simulate that fallback on every platform.
        system = lambda: urllib.request.getproxies_environment() or {"http": unreachable}
        with patch.dict(os.environ, {"HTTP_PROXY": unreachable, "http_proxy": unreachable}), \
                patch.object(urllib.request, "getproxies", system):
            code, report, document = self.run_refresh("--providers", "openai")
        self.assertEqual(code, 0, report)
        self.assertEqual(list(document["providers"]), ["openai"])

    def test_an_empty_catalog_url_is_a_usage_error(self):
        with contextlib.redirect_stderr(io.StringIO()) as error, self.assertRaises(SystemExit) as exit:
            refresh.build_parser().parse_args([str(self.output), "--catalog-url", ""])
        self.assertEqual(exit.exception.code, 2)
        self.assertIn("--catalog-url", error.getvalue())
        self.assertEqual(self.server.requests, [])

    def test_providers_limits_which_ones_run(self):
        code, report, document = self.run_refresh("--providers", "cerebras")
        self.assertEqual(code, 0, report)
        self.assertEqual(list(document["providers"]), ["cerebras"])
        self.assertEqual({path.split("/")[1] for path, _ in self.server.requests}, {"catalog", "cerebras"})
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            refresh.build_parser().parse_args([str(self.output), "--providers", "mistral"])

    def test_identifiers_are_sanitized_and_ordered_as_ask_does(self):
        self.assertEqual(
            refresh.arrange(["a-0101", "a-1231", "b", "c-2024-02-30", "d-20241301", "1231", "e-dummy-xai-key"],
                            ["dummy-xai-key"]),
            ["b", "d-20241301", "a-0101", "a-1231", "c-2024-02-30", "1231"],
        )
        self.assertEqual(refresh.arrange(["ok", "model\u202etxt", "mo\u200bdel", "tab\tid", ""], []), ["ok"])
        self.assertEqual(refresh.arrange(["ok", json.loads('"lone-\\ud800"')], []), ["ok"])


if __name__ == "__main__":
    unittest.main()
