#!/usr/bin/env python3
"""Offline synthetic tests for live-provider check support; no credentials or network."""
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import textwrap
import multiprocessing
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    "live_check", Path(__file__).with_name("live-provider-check.py")
)
live = importlib.util.module_from_spec(spec)
spec.loader.exec_module(live)


def _concurrent_run_worker(config_path, repo_path, output_queue):
    module_path = Path(__file__).with_name("live-provider-check.py")
    worker_spec = importlib.util.spec_from_file_location("live_check_worker", module_path)
    live_mod = importlib.util.module_from_spec(worker_spec)
    worker_spec.loader.exec_module(live_mod)
    argv = [
        "live-provider-check.py",
        "run",
        "--config",
        str(config_path),
        "--host",
        "test-host",
        "--budget-available",
    ]
    scrubbed = {
        key: value
        for key, value in os.environ.items()
        if not (key.endswith("_API_KEY") or key.endswith("_KEY"))
    }
    scrubbed["PYTHONDONTWRITEBYTECODE"] = "1"
    buffer = io.StringIO()
    with patch.object(live_mod, "ROOT", Path(repo_path)), patch.object(
        sys, "argv", argv
    ), patch.dict(os.environ, scrubbed, clear=True), patch.object(sys, "stdout", buffer):
        code = live_mod.main()
    output_queue.put((code, json.loads(buffer.getvalue())))


class LiveProviderCheckTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self._init_repo()
        self.wrapper = self._write_executable(
            "wrapper",
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import os
                import subprocess
                import sys
                ask = sys.argv[1]
                env = dict(os.environ)
                env["WRAPPED"] = "1"
                raise SystemExit(subprocess.run([ask, *sys.argv[2:]], env=env, shell=False).returncode)
                """
            ),
        )
        self.ask = self._write_executable(
            "ask",
            textwrap.dedent(
                f"""\
                #!/usr/bin/env python3
                import os
                import sys
                cmd = sys.argv[1] if len(sys.argv) > 1 else ""
                if cmd == "doctor":
                    if os.environ.get("LIVE_CHECK_MISSING_CREDS") == "1":
                        print("missing credentials", file=sys.stderr)
                        raise SystemExit(1)
                    raise SystemExit(0)
                if cmd not in {{"new", "reply"}}:
                    print("unsupported", file=sys.stderr)
                    raise SystemExit(2)
                if cmd == "new" and os.environ.get("LIVE_CHECK_FAIL_QUERY") == "1":
                    print("provider failure", file=sys.stderr)
                    raise SystemExit(1)
                if "fail-query" in " ".join(sys.argv[2:]):
                    print("provider failure", file=sys.stderr)
                    raise SystemExit(1)
                if "fail-reply" in " ".join(sys.argv[2:]):
                    print("provider failure", file=sys.stderr)
                    raise SystemExit(1)
                if os.environ.get("LIVE_CHECK_WRONG_MARKER") == "1":
                    print("wrong-marker")
                elif cmd == "new":
                    print({live.QUERY_ANSWER_MARKER!r})
                else:
                    print({live.REPLY_ANSWER_MARKER!r})
                print("fixture · 0.1s wall · 0.1s api · 0.0s to first token · 4 in / 2 out", file=sys.stderr)
                raise SystemExit(0)
                """
            ),
        )
        self.config_path = self.root / "check.toml"
        self.state_path = self.root / "check.toml.state.json"
        self.lock_path = self.root / "check.toml.state.json.lock"
        self.homes = {
            "openai": self.root / "home-openai",
            "anthropic": self.root / "home-anthropic",
        }
        for home in self.homes.values():
            home.mkdir()
            self._write_ask_config(home, api_key_env="OPENAI_API_KEY")

    def _init_repo(self):
        for rel in ("Cargo.toml", "Cargo.lock", "scripts/live-provider-check.py", "src/lib.rs"):
            path = self.repo / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"{rel}\n")
        subprocess.run(["git", "init"], cwd=self.repo, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=self.repo, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "test"], cwd=self.repo, check=True, capture_output=True)
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "init"], cwd=self.repo, check=True, capture_output=True)

    def _write_executable(self, name, body):
        path = self.root / name
        path.write_text(body)
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        return path

    def _revision(self):
        return subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=self.repo,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def _write_manifest(self):
        manifest = {
            "source_sha": self._revision(),
            "binary_sha256": live.digest(self.ask.read_bytes()),
        }
        (self.ask.parent / "manifest.json").write_text(json.dumps(manifest, sort_keys=True) + "\n")

    def _write_ask_config(self, home, api_key_env="OPENAI_API_KEY", max_output_tokens=128, model="gpt-test"):
        home.mkdir(parents=True, exist_ok=True)
        text = textwrap.dedent(
            f"""\
            default_profile = "live"

            [providers.openai]
            kind = "openai"
            base_url = "https://example.test/v1"
            api_key_env = "{api_key_env}"

            [profiles.live]
            provider = "openai"
            model = "{model}"
            max_output_tokens = {max_output_tokens}
            system_prompt = "live-check"
            """
        )
        (home / "config.toml").write_text(text)

    def _write_config(self, **overrides):
        revision = self._revision()
        self._write_manifest()
        lines = [
            "version = 1",
            "",
            "[run]",
            f'ask_binary = "{self.ask}"',
            f'source_revision = "{revision}"',
            f'credential_wrapper = "{self.wrapper}"',
            'host_id = "test-host"',
            "budget_available = true",
            f'state_path = "{self.state_path}"',
            f'lock_path = "{self.lock_path}"',
            "",
            "[policy]",
            "max_requests = 4",
            "max_answer_chars = 64",
            f'query_prompt = "{live.DEFAULT_QUERY}"',
            f'reply_prompt = "{live.DEFAULT_REPLY}"',
            "",
        ]
        for target_id, home in self.homes.items():
            lines.extend(
                [
                    "[[targets]]",
                    f'id = "{target_id}"',
                    f'ask_home = "{home}"',
                    "",
                ]
            )
        self.config_path.write_text("\n".join(lines))
        return live.load_config(self.config_path)

    def _run(self, extra=None, env=None):
        argv = ["live-provider-check.py", "run", "--config", str(self.config_path), "--host", "test-host", "--budget-available"]
        if extra:
            argv.extend(extra)
        merged = {"PYTHONDONTWRITEBYTECODE": "1"}
        if env:
            merged.update(env)
        scrubbed = os.environ.copy()
        for key in list(scrubbed):
            if key.endswith("_API_KEY") or key.endswith("_KEY"):
                scrubbed.pop(key, None)
        scrubbed.update(merged)
        with patch.object(live, "ROOT", self.repo), patch.object(sys, "argv", argv), patch.dict(os.environ, scrubbed, clear=True):
            return live.main()

    def _capture_report(self, code, extra=None, env=None):
        buffer = io.StringIO()
        with patch.object(sys, "stdout", buffer):
            exit_code = self._run(extra=extra, env=env)
        self.assertEqual(exit_code, code)
        return json.loads(buffer.getvalue())

    def test_config_change_fingerprint(self):
        config = self._write_config()
        with patch.object(live, "ROOT", self.repo):
            _, snapshots = live.validate_targets(config)
            before = live.config_fingerprint(config, snapshots)
        self._write_ask_config(self.homes["openai"], model="gpt-changed")
        with patch.object(live, "ROOT", self.repo):
            updated = live.load_config(self.config_path)
            _, snapshots = live.validate_targets(updated)
            after = live.config_fingerprint(updated, snapshots)
        self.assertNotEqual(before, after)

    def test_fingerprint_ignores_docs_only_changes(self):
        config = self._write_config()
        with patch.object(live, "ROOT", self.repo):
            _, snapshots = live.validate_targets(config)
            before = live.source_fingerprint(config, snapshots)
        (self.repo / "docs/guide.md").parent.mkdir(parents=True)
        (self.repo / "docs/guide.md").write_text("docs only\n")
        subprocess.run(["git", "add", "docs/guide.md"], cwd=self.repo, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "docs"], cwd=self.repo, check=True, capture_output=True)
        with patch.object(live, "ROOT", self.repo):
            after = live.source_fingerprint(config, snapshots)
        self.assertEqual(before, after)

    def test_fingerprint_changes_when_source_changes(self):
        config = self._write_config()
        with patch.object(live, "ROOT", self.repo):
            _, snapshots = live.validate_targets(config)
            before = live.source_fingerprint(config, snapshots)
        (self.repo / "src/lib.rs").write_text("changed\n")
        subprocess.run(["git", "add", "src/lib.rs"], cwd=self.repo, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "code"], cwd=self.repo, check=True, capture_output=True)
        with patch.object(live, "ROOT", self.repo):
            after = live.source_fingerprint(config, snapshots)
        self.assertNotEqual(before, after)

    def test_mandatory_manifest_mismatch_is_notrun(self):
        self._write_config()
        manifest = json.loads((self.ask.parent / "manifest.json").read_text())
        manifest["source_sha"] = "0" * 40
        (self.ask.parent / "manifest.json").write_text(json.dumps(manifest) + "\n")
        report = self._capture_report(2)
        self.assertEqual(report["status"], "notrun")
        self.assertIn("binary manifest not verified", report["reason"])
        self.assertEqual(report["request_count"], 0)

    def test_missing_credentials_zero_query(self):
        self._write_config()
        report = self._capture_report(2, env={"LIVE_CHECK_MISSING_CREDS": "1"})
        self.assertEqual(report["status"], "notrun")
        self.assertIn("doctor preflight failed", report["reason"])
        self.assertEqual(report["request_count"], 0)
        self.assertFalse(self.state_path.is_file())

    def test_max_output_limit_is_required(self):
        self._write_config()
        self._write_ask_config(self.homes["openai"], max_output_tokens=64)
        report = self._capture_report(2)
        self.assertEqual(report["status"], "notrun")
        self.assertIn("max_output_tokens must be 128", report["reason"])

    def test_attempt_is_recorded_before_requests(self):
        self._write_config()
        calls = []
        seen_provider = []

        def invoke(wrapper, ask_binary, ask_home, args, timeout=120):
            calls.append(list(args))
            if args[0] in {"new", "reply"} and not seen_provider:
                seen_provider.append(True)
                state = json.loads(self.state_path.read_text())
                self.assertEqual(state["attempts"][-1]["status"], "started")
            if args == ["doctor"]:
                return subprocess.CompletedProcess(args, 0, "", "")
            marker = live.QUERY_ANSWER_MARKER if args[0] == "new" else live.REPLY_ANSWER_MARKER
            return subprocess.CompletedProcess(
                args,
                0,
                marker,
                "fixture · 0.1s wall · 0.1s api · 0.0s to first token · 4 in / 2 out",
            )

        with patch.object(live, "ROOT", self.repo), patch.object(live, "invoke", side_effect=invoke):
            code = self._run()
        self.assertEqual(code, 0)
        self.assertEqual(calls[0], ["doctor"])
        self.assertEqual(calls[1], ["doctor"])
        state = json.loads(self.state_path.read_text())
        self.assertEqual(state["attempts"][-1]["status"], "pass")

    def test_failed_fingerprint_blocks_retry_without_force(self):
        config = self._write_config()
        with patch.object(live, "ROOT", self.repo):
            _, snapshots = live.validate_targets(config)
            fingerprint = live.source_fingerprint(config, snapshots)
        self.state_path.write_text(
            json.dumps(
                {
                    "attempts": [
                        {
                            "fingerprint": fingerprint,
                            "status": "fail",
                            "request_count": 1,
                        }
                    ]
                }
            )
            + "\n"
        )
        report = self._capture_report(2)
        self.assertEqual(report["status"], "notrun")
        self.assertIn("prior failed attempt", report["reason"])

    def test_crashed_started_fingerprint_blocks_retry_without_force(self):
        config = self._write_config()
        with patch.object(live, "ROOT", self.repo):
            _, snapshots = live.validate_targets(config)
            fingerprint = live.source_fingerprint(config, snapshots)
        self.state_path.write_text(
            json.dumps(
                {
                    "attempts": [
                        {
                            "fingerprint": fingerprint,
                            "status": "started",
                            "request_count": 0,
                        }
                    ]
                }
            )
            + "\n"
        )
        report = self._capture_report(2)
        self.assertEqual(report["status"], "notrun")
        self.assertIn("prior failed attempt", report["reason"])

    def test_force_allows_retry_after_failure_but_not_manifest_mismatch(self):
        config = self._write_config()
        with patch.object(live, "ROOT", self.repo):
            _, snapshots = live.validate_targets(config)
            fingerprint = live.source_fingerprint(config, snapshots)
        self.state_path.write_text(json.dumps({"attempts": [{"fingerprint": fingerprint, "status": "fail"}]}) + "\n")
        report = self._capture_report(0, extra=["--force"])
        self.assertEqual(report["status"], "pass")
        self.assertEqual(report["request_count"], 4)
        manifest = json.loads((self.ask.parent / "manifest.json").read_text())
        manifest["source_sha"] = "0" * 40
        (self.ask.parent / "manifest.json").write_text(json.dumps(manifest) + "\n")
        report = self._capture_report(2, extra=["--force"])
        self.assertEqual(report["status"], "notrun")
        self.assertIn("binary manifest not verified", report["reason"])

    def test_reply_skipped_when_query_fails(self):
        self._write_config()
        homes = {"only": self.root / "home-only"}
        homes["only"].mkdir()
        self._write_ask_config(homes["only"])
        self.config_path.write_text(
            textwrap.dedent(
                f"""\
                version = 1
                [run]
                ask_binary = "{self.ask}"
                source_revision = "{self._revision()}"
                credential_wrapper = "{self.wrapper}"
                host_id = "test-host"
                budget_available = true
                state_path = "{self.state_path}"
                lock_path = "{self.lock_path}"
                [policy]
                max_requests = 2
                query_prompt = "{live.DEFAULT_QUERY}"
                reply_prompt = "{live.DEFAULT_REPLY}"
                [[targets]]
                id = "only"
                ask_home = "{homes['only']}"
                """
            )
        )
        report = self._capture_report(1, env={"LIVE_CHECK_FAIL_QUERY": "1"})
        self.assertEqual(report["targets"][0]["query"], "fail")
        self.assertEqual(report["targets"][0]["reply"], "skipped")
        self.assertEqual(report["request_count"], 1)

    def test_request_cap_is_enforced(self):
        self._write_config()
        text = self.config_path.read_text().replace("max_requests = 4", "max_requests = 1")
        self.config_path.write_text(text)
        report = self._capture_report(1)
        self.assertEqual(report["request_count"], 1)
        self.assertEqual(report["targets"][0]["query"], "pass")
        self.assertEqual(report["targets"][1]["query"], "skipped")

    def test_report_is_sanitized_and_subprocess_never_uses_shell(self):
        self._write_config()
        with patch.object(live.subprocess, "run", wraps=live.subprocess.run) as runner:
            report = self._capture_report(0)
        for call in runner.call_args_list:
            self.assertFalse(call.kwargs.get("shell"))
        blob = json.dumps(report)
        self.assertNotIn(live.DEFAULT_QUERY, blob)
        self.assertNotIn("provider failure", blob)
        self.assertIn("request_count", blob)

    def test_doc_only_revision_mismatch_is_notrun(self):
        self._write_config()
        pinned_revision = self._revision()
        (self.repo / "docs/note.md").parent.mkdir(parents=True)
        (self.repo / "docs/note.md").write_text("note\n")
        subprocess.run(["git", "add", "docs/note.md"], cwd=self.repo, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "docs"], cwd=self.repo, check=True, capture_output=True)
        self.assertNotEqual(self._revision(), pinned_revision)
        report = self._capture_report(0)
        self.assertEqual(report["status"], "notrun")
        self.assertIn("documentation-only change", report["reason"])

    def test_code_revision_mismatch_is_notrun(self):
        self._write_config()
        pinned_revision = self._revision()
        (self.repo / "src/lib.rs").write_text("changed\n")
        subprocess.run(["git", "add", "src/lib.rs"], cwd=self.repo, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "code"], cwd=self.repo, check=True, capture_output=True)
        self.assertNotEqual(self._revision(), pinned_revision)
        report = self._capture_report(2)
        self.assertEqual(report["status"], "notrun")
        self.assertIn("checkout revision differs", report["reason"])

    def test_toml_snapshot_escapes_arbitrary_strings_and_omits_none(self):
        snapshot = {
            "profile": 'live"profile',
            "provider": "openai/weird",
            "kind": "openai",
            "base_url": 'https://example.test/v1"\n',
            "api_key_env": "OPENAI_API_KEY",
            "model": 'gpt-"test"',
            "max_output_tokens": 128,
            "system_prompt": "operator template must not appear",
        }
        rendered = live.render_ask_home_toml(snapshot)
        self.assertNotIn("operator template must not appear", rendered)
        self.assertIn(live.LIVE_CHECK_SYSTEM_PROMPT, rendered)
        self.assertIn('[providers."openai/weird"]', rendered)
        self.assertIn('[profiles."live\\"profile"]', rendered)
        self.assertIn('base_url = "https://example.test/v1\\"\\n"', rendered)
        self.assertNotIn("None", rendered)
        home = live.write_ask_home_from_snapshot(self.root, snapshot)
        self.addCleanup(lambda: __import__("shutil").rmtree(home, ignore_errors=True))
        document = __import__("tomllib").loads((home / "config.toml").read_text())
        self.assertEqual(document["profiles"]['live"profile']["system_prompt"], live.LIVE_CHECK_SYSTEM_PROMPT)

    def test_marker_comparison_is_independent_of_report_length(self):
        for marker in (live.QUERY_ANSWER_MARKER, live.REPLY_ANSWER_MARKER):
            result = subprocess.CompletedProcess([], 0, marker + "\n", "")
            self.assertTrue(live.command_succeeded(result, 8, marker))

    def test_toml_control_characters_round_trip(self):
        value = "model" + chr(0x1f642) + "".join(chr(code) for code in range(32)) + chr(127)
        rendered = live.toml_basic_string(value)
        document = __import__("tomllib").loads("value = " + rendered)
        self.assertEqual(document["value"], value)

    def test_budget_available_rejects_non_boolean(self):
        self._write_config()
        text = self.config_path.read_text().replace("budget_available = true", "budget_available = 1")
        self.config_path.write_text(text)
        with self.assertRaisesRegex(ValueError, "run.budget_available must be a boolean"):
            live.load_config(self.config_path)

    def test_duplicate_target_ids_are_rejected(self):
        self._write_config()
        text = self.config_path.read_text() + '\n[[targets]]\nid = "openai"\nask_home = "{home}"\n'.format(
            home=self.homes["anthropic"]
        )
        self.config_path.write_text(text)
        with self.assertRaisesRegex(ValueError, "duplicate target id openai"):
            live.load_config(self.config_path)

    def test_dirty_relevant_source_is_notrun(self):
        self._write_config()
        (self.repo / "src/lib.rs").write_text("dirty working tree\n")
        report = self._capture_report(2)
        self.assertEqual(report["status"], "notrun")
        self.assertIn("relevant source tree is dirty", report["reason"])
        self.assertEqual(report["request_count"], 0)

    def test_wrong_answer_marker_fails_query(self):
        self._write_config()
        report = self._capture_report(1, env={"LIVE_CHECK_WRONG_MARKER": "1"})
        self.assertEqual(report["status"], "fail")
        self.assertEqual(report["targets"][0]["query"], "fail")
        self.assertEqual(report["targets"][0]["reply"], "skipped")

    def test_query_and_reply_usage_are_reported_separately(self):
        self._write_config()
        report = self._capture_report(0)
        self.assertEqual(report["status"], "pass")
        for target in report["targets"]:
            self.assertEqual(target["query_usage"], {"input_tokens": 4, "output_tokens": 2})
            self.assertEqual(target["reply_usage"], {"input_tokens": 4, "output_tokens": 2})

    def test_missing_usage_metadata_reports_null(self):
        self._write_config()

        def invoke_without_usage(wrapper, ask_binary, ask_home, args, timeout=120):
            if args == ["doctor"]:
                return subprocess.CompletedProcess(args, 0, "", "")
            marker = live.QUERY_ANSWER_MARKER if args[0] == "new" else live.REPLY_ANSWER_MARKER
            return subprocess.CompletedProcess(args, 0, marker, "fixture without usage")

        with patch.object(live, "ROOT", self.repo), patch.object(live, "invoke", side_effect=invoke_without_usage):
            report = self._capture_report(0)
        self.assertEqual(report["status"], "pass")
        for target in report["targets"]:
            self.assertIsNone(target["query_usage"])
            self.assertIsNone(target["reply_usage"])

    def _single_target_config(self, max_requests=2):
        self._write_manifest()
        home = self.root / "home-only"
        home.mkdir()
        self._write_ask_config(home)
        self.config_path.write_text(
            textwrap.dedent(
                f"""\
                version = 1
                [run]
                ask_binary = "{self.ask}"
                source_revision = "{self._revision()}"
                credential_wrapper = "{self.wrapper}"
                host_id = "test-host"
                budget_available = true
                state_path = "{self.state_path}"
                lock_path = "{self.lock_path}"
                [policy]
                max_requests = {max_requests}
                query_prompt = "{live.DEFAULT_QUERY}"
                reply_prompt = "{live.DEFAULT_REPLY}"
                [[targets]]
                id = "only"
                ask_home = "{home}"
                """
            )
        )

    def test_query_timeout_records_attempted_count_and_blocks_retry(self):
        self._single_target_config()
        timed_out = []

        def invoke_timeout(wrapper, ask_binary, ask_home, args, timeout=120):
            if args == ["doctor"]:
                return subprocess.CompletedProcess(args, 0, "", "")
            if args[0] == "new" and not timed_out:
                timed_out.append(True)
                raise subprocess.TimeoutExpired(cmd=["ask", "new"], timeout=timeout)
            marker = live.QUERY_ANSWER_MARKER if args[0] == "new" else live.REPLY_ANSWER_MARKER
            return subprocess.CompletedProcess(
                args,
                0,
                marker,
                "fixture · 0.1s wall · 0.1s api · 0.0s to first token · 4 in / 2 out",
            )

        with patch.object(live, "ROOT", self.repo), patch.object(live, "invoke", side_effect=invoke_timeout):
            report = self._capture_report(1)
        self.assertEqual(report["status"], "fail")
        self.assertEqual(report["request_count"], 1)
        self.assertEqual(report["targets"][0]["query"], "fail")
        self.assertEqual(report["targets"][0]["reply"], "skipped")
        self.assertNotIn("ask", json.dumps(report))
        state = json.loads(self.state_path.read_text())
        self.assertEqual(state["attempts"][-1]["status"], "fail")
        self.assertEqual(state["attempts"][-1]["request_count"], 1)
        retry = self._capture_report(2)
        self.assertEqual(retry["status"], "notrun")
        self.assertIn("prior failed attempt", retry["reason"])

    def test_invocation_os_error_is_redacted_in_report(self):
        self._single_target_config()
        secret_path = str(self.root / "secret" / "credentials")

        def invoke_os_error(wrapper, ask_binary, ask_home, args, timeout=120):
            if args == ["doctor"]:
                return subprocess.CompletedProcess(args, 0, "", "")
            if args[0] == "new":
                raise OSError(2, "No such file or directory", secret_path)
            marker = live.REPLY_ANSWER_MARKER
            return subprocess.CompletedProcess(args, 0, marker, "")

        with patch.object(live, "ROOT", self.repo), patch.object(live, "invoke", side_effect=invoke_os_error):
            report = self._capture_report(1)
        self.assertEqual(report["status"], "fail")
        self.assertEqual(report["request_count"], 1)
        blob = json.dumps(report)
        self.assertNotIn(secret_path, blob)
        self.assertNotIn("credentials", blob)
        self.assertNotIn(live.DEFAULT_QUERY, blob)
        self.assertEqual(report["targets"][0]["query"], "fail")
        self.assertIsNone(report["targets"][0].get("query_usage"))

    def test_concurrent_invoke_allows_one_attempt(self):
        self._write_config()
        output_queue = multiprocessing.Queue()
        processes = [
            multiprocessing.Process(
                target=_concurrent_run_worker,
                args=(self.config_path, self.repo, output_queue),
            )
            for _ in range(2)
        ]
        for process in processes:
            process.start()
        for process in processes:
            process.join(timeout=30)
            self.assertFalse(process.is_alive())
        results = [output_queue.get(timeout=5) for _ in range(2)]
        codes = sorted(result[0] for result in results)
        self.assertEqual(codes, [0, 2])
        statuses = sorted(result[1]["status"] for result in results)
        self.assertIn("pass", statuses)
        self.assertIn("notrun", statuses)
        started = [result[1] for result in results if result[1].get("request_count", 0) > 0]
        self.assertEqual(len(started), 1)


if __name__ == "__main__":
    unittest.main()
