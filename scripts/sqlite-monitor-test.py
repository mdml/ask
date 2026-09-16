#!/usr/bin/env python3
"""Offline tests for the native SQLite monitor; upstream sources are in-memory fixtures."""
import datetime
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True

SCRIPT = Path(__file__).with_name("sqlite-monitor.py")
spec = importlib.util.spec_from_file_location("monitor", SCRIPT)
monitor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(monitor)
TODAY = datetime.date(2026, 9, 16)


def index(name, versions, checksums=None):
    rows = [{"name": name, "vers": v, "yanked": False, "cksum": (checksums or {}).get(v, "0" * 64)} for v in versions]
    return "\n".join(json.dumps(r) for r in rows).encode()


def crate(name, version, sqlite_version, source_id):
    header = f'#define SQLITE_VERSION        "{sqlite_version}"\n#define SQLITE_SOURCE_ID      "{source_id}"\n'.encode()
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        member = tarfile.TarInfo(f"{name}-{version}/sqlite3/sqlite3.h")
        member.size = len(header)
        archive.addfile(member, io.BytesIO(header))
    return buffer.getvalue()


def downloads(*versions):
    lines = ["<html>", "PRODUCT,VERSION,RELATIVE-URL,SIZE-IN-BYTES,SHA3-HASH"]
    for v in versions:
        compact = "".join(f"{int(p):02d}" for p in v.split("."))
        lines.append(f"PRODUCT,{v},2026/sqlite-amalgamation-3{compact[1:]}.zip,1,abc")
        lines.append(f"PRODUCT,{v},2026/sqlite-doc-3{compact[1:]}.zip,1,abc")
    return "\n".join(lines).encode()


def timeline(*check_ins):
    entries = [f'<span class="timelineHash"><a href="/src/info/{check_in}">{check_in}</a></span>' for check_in in check_ins]
    return "\n".join(entries).encode()


def cves(rows):
    body = ""
    for ids, fix in rows:
        anchors = "<br>".join(f"<a href='https://nvd.nist.gov/vuln/detail/{i}'>{i}</a>" for i in ids)
        body += f"<tr><td valign='top'>{anchors}</td><td valign='top'>{fix}</td><td>comment</td></tr>\n"
    return f"<table><thead><tr><th>CVE Number</th><th>Fix</th><th>Comments</th></tr></thead><tbody>{body}</tbody></table>".encode()


def fixed(version):
    return f'<a href="releaselog/{version.replace(".", "_")}.html">{version}</a><br>(2026-01-01)'


class Sources:
    """A fake fetcher keyed by URL; unknown URLs fail like the network would."""

    def __init__(self, **pages):
        self.pages = pages
        self.urls = []

    def __call__(self, url, limit=monitor.SMALL):
        self.urls.append(url)
        if url not in self.pages:
            raise OSError("unavailable")
        return self.pages[url]


class MonitorTests(unittest.TestCase):
    def setUp(self):
        self.ledger = monitor.read_ledger()
        self.lock = monitor.read_lock()
        self.locked_rusqlite = self.ledger["locked"]["rusqlite"]
        self.locked_sys = self.ledger["locked"]["libsqlite3_sys"]

    def sources(self, **overrides):
        pages = {
            f"{monitor.INDEX}/ru/sq/rusqlite": index("rusqlite", ["0.39.0", self.locked_rusqlite]),
            f"{monitor.INDEX}/li/bs/libsqlite3-sys": index("libsqlite3-sys", [self.locked_sys]),
            monitor.DOWNLOADS: downloads(self.ledger["triage"]["last_triaged_sqlite_release"]),
            monitor.CVES: cves([(["CVE-2026-11822", "CVE-2026-11824"], fixed("3.53.2")),
                                (["CVE-2025-3277"], fixed("3.49.1")),
                                (["CVE-2026-51296"], "Not a bug in SQLite"),
                                (["CVE-2022-46908"], "Not a bug in the core SQLite library")]),
        }
        pages.update(overrides)
        return Sources(**pages)

    def findings(self, results, check):
        return next(r for r in results if r["check"] == check)

    def test_ledger_matches_the_lockfile_and_a_quiet_upstream_reports_no_item(self):
        results = monitor.report(self.ledger, self.lock, self.sources(), TODAY, offline=False)
        self.assertEqual([r["findings"] for r in results], [[]] * 5)
        self.assertTrue(all("not_checked" not in r for r in results))
        text = monitor.render(self.ledger, results, TODAY)
        self.assertIn("No new triage item", text)
        self.assertIn(self.ledger["native"]["source_id"], text)

    def test_lockfile_drift_from_the_ledger_is_a_triage_item(self):
        lock = dict(self.lock, rusqlite=("0.41.0", "f" * 64))
        result = monitor.check_lock(self.ledger, lock)
        self.assertEqual(len(result["findings"]), 1)
        self.assertIn("rusqlite 0.41.0", result["findings"][0])

    def test_newer_rusqlite_is_reported_without_downloading_anything(self):
        sources = self.sources(**{f"{monitor.INDEX}/ru/sq/rusqlite": index("rusqlite", [self.locked_rusqlite, "0.41.0", "0.42.0-beta.1"])})
        result = monitor.check_crates(self.ledger, sources)
        self.assertEqual(result["observed"]["rusqlite"]["latest_stable"], "0.41.0")
        self.assertEqual(len(result["findings"]), 1)
        self.assertIn("rusqlite 0.41.0 is published", result["findings"][0])
        self.assertFalse(any(url.startswith(monitor.CRATES) for url in sources.urls))

    def test_newer_sys_crate_is_inspected_for_its_bundled_sqlite(self):
        source_id = "2026-07-24 19:02:57 " + "b" * 64
        archive = crate("libsqlite3-sys", "0.39.0", "3.53.4", source_id)
        checksum = hashlib.sha256(archive).hexdigest()
        sources = self.sources(**{
            f"{monitor.INDEX}/li/bs/libsqlite3-sys": index("libsqlite3-sys", [self.locked_sys, "0.39.0"], {"0.39.0": checksum}),
            f"{monitor.CRATES}/libsqlite3-sys/libsqlite3-sys-0.39.0.crate": archive,
        })
        result = monitor.check_crates(self.ledger, sources)
        observed = result["observed"]["libsqlite3-sys"]
        self.assertEqual((observed["bundled_sqlite"], observed["bundled_source_id"]), ("3.53.4", source_id))
        self.assertEqual(len(result["findings"]), 1)
        self.assertIn("bundles SQLite 3.53.4", result["findings"][0])
        self.assertIn("revisit", result["findings"][0])

    def test_sys_crate_with_wrong_checksum_is_not_checked(self):
        archive = crate("libsqlite3-sys", "0.39.0", "3.53.4", "id")
        sources = self.sources(**{
            f"{monitor.INDEX}/li/bs/libsqlite3-sys": index("libsqlite3-sys", [self.locked_sys, "0.39.0"]),
            f"{monitor.CRATES}/libsqlite3-sys/libsqlite3-sys-0.39.0.crate": archive,
        })
        result = monitor.guarded("crate registry freshness", monitor.check_crates, self.ledger, sources)
        self.assertEqual(result["not_checked"], "ValueError")
        self.assertEqual(result["findings"], [])

    def test_sys_crate_source_id_is_validated_before_reporting(self):
        archive = crate("libsqlite3-sys", "0.39.0", "3.53.4", "id\n| injected")
        with self.assertRaises(ValueError):
            monitor.bundled_sqlite(
                "libsqlite3-sys",
                "0.39.0",
                hashlib.sha256(archive).hexdigest(),
                Sources(**{
                    f"{monitor.CRATES}/libsqlite3-sys/libsqlite3-sys-0.39.0.crate": archive,
                }),
            )

    def test_sys_crate_expansion_is_bounded_before_tar_parsing(self):
        archive = crate("libsqlite3-sys", "0.39.0", "3.53.4", "id")
        with patch.object(monitor, "CRATE_EXPANDED_LIMIT", 512, create=True):
            with self.assertRaises(ValueError):
                monitor.bundled_sqlite(
                    "libsqlite3-sys",
                    "0.39.0",
                    hashlib.sha256(archive).hexdigest(),
                    Sources(**{
                        f"{monitor.CRATES}/libsqlite3-sys/libsqlite3-sys-0.39.0.crate": archive,
                    }),
                )

    def test_newer_upstream_release_lists_the_untriaged_interval(self):
        interval = monitor.TIMELINE.format("3.53.4", "3.54.0")
        result = monitor.check_sqlite_release(self.ledger, self.sources(**{
            monitor.DOWNLOADS: downloads("3.53.4", "3.54.0"),
            interval: timeline("c" * 10, "b" * 10, self.ledger["triage"]["last_triaged_check_in"]),
        }))
        self.assertEqual(result["observed"]["latest"], "3.54.0")
        self.assertEqual(result["observed"]["untriaged_check_ins"], ["c" * 10, "b" * 10])
        self.assertEqual(len(result["findings"]), 1)
        self.assertIn("timeline?from=version-3.53.4&to=version-3.54.0", result["findings"][0])
        self.assertIn("cccccccccc, bbbbbbbbbb", result["findings"][0])

    def test_release_timeline_stops_at_the_triaged_marker(self):
        interval = monitor.TIMELINE.format("3.53.4", "3.54.0")
        result = monitor.check_sqlite_release(self.ledger, self.sources(**{
            monitor.DOWNLOADS: downloads("3.53.4", "3.54.0"),
            interval: timeline("c" * 10, self.ledger["triage"]["last_triaged_check_in"], "a" * 10),
        }))
        self.assertEqual(result["observed"]["untriaged_check_ins"], ["c" * 10])
        self.assertNotIn("a" * 10, result["findings"][0])

    def test_release_timeline_output_is_bounded(self):
        interval = monitor.TIMELINE.format("3.53.4", "3.54.0")
        check_ins = [f"{value:010x}" for value in range(monitor.TIMELINE_CHECK_IN_LIMIT + 1)]
        with self.assertRaises(ValueError):
            monitor.check_sqlite_release(self.ledger, self.sources(**{
                monitor.DOWNLOADS: downloads("3.53.4", "3.54.0"),
                interval: timeline(*check_ins, self.ledger["triage"]["last_triaged_check_in"]),
            }))

    def test_cves_fixed_at_or_below_the_linked_version_or_declared_not_bugs_need_no_item(self):
        result = monitor.check_cves(self.ledger, self.sources())
        self.assertEqual(result["findings"], [])
        self.assertEqual(result["observed"], {"rows": 4, "fixed_in_linked_version": 3, "upstream_not_a_library_bug": 2, "ledger_disposition": 0})

    def test_cves_fixed_after_the_linked_version_need_a_disposition(self):
        page = cves([(["CVE-2026-90001"], fixed("3.53.3")), (["CVE-2026-90002"], "Fixed on trunk"), (["CVE-2026-90003"], fixed("3.60.0"))])
        ledger = dict(self.ledger, cve_triage=[{
            "ids": ["CVE-2026-90003"],
            "disposition": "reviewed",
            "owner": "repository owner",
            "recorded_on": "2026-09-15",
            "review_deadline": "2026-10-10",
        }])
        result = monitor.check_cves(ledger, self.sources(**{monitor.CVES: page}), TODAY)
        self.assertEqual(result["observed"]["ledger_disposition"], 1)
        self.assertEqual([f.split(" ")[0] for f in result["findings"]], ["CVE-2026-90001", "CVE-2026-90002"])
        self.assertIn("fix `3.53.3`", result["findings"][0])
        self.assertIn("unreleased or unspecified upstream fix", result["findings"][1])

    def test_incomplete_or_malformed_cve_dispositions_are_local_input_errors(self):
        complete = {
            "ids": ["CVE-2026-90003"],
            "disposition": "reviewed",
            "owner": "repository owner",
            "recorded_on": "2026-09-15",
            "review_deadline": "2026-10-10",
        }
        for change in (
            {"disposition": ""},
            {"owner": ""},
            {"recorded_on": "September 15"},
            {"review_deadline": ""},
            {"ids": []},
        ):
            entry = dict(complete, **change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                monitor.validate_cve_triage({"cve_triage": [entry]})

    def test_expired_cve_disposition_does_not_suppress_a_finding(self):
        page = cves([(["CVE-2026-90003"], fixed("3.60.0"))])
        ledger = dict(self.ledger, cve_triage=[{
            "ids": ["CVE-2026-90003"],
            "disposition": "reviewed",
            "owner": "repository owner",
            "recorded_on": "2026-09-01",
            "review_deadline": "2026-09-15",
        }])
        result = monitor.check_cves(ledger, self.sources(**{monitor.CVES: page}), TODAY)
        self.assertEqual(result["observed"]["ledger_disposition"], 0)
        self.assertEqual(len(result["findings"]), 1)
        self.assertIn("expired 2026-09-15", result["findings"][0])

    def test_unclassified_cve_text_is_not_copied_into_the_report(self):
        unsafe = "Fixed **later**\n| injected " + "x" * 5000
        page = cves([(["CVE-2026-90003"], unsafe)])
        result = monitor.check_cves(self.ledger, self.sources(**{monitor.CVES: page}), TODAY)
        self.assertEqual(len(result["findings"]), 1)
        self.assertIn("unreleased or unspecified upstream fix", result["findings"][0])
        self.assertNotIn("injected", result["findings"][0])
        self.assertLess(len(result["findings"][0]), 300)

    def test_an_unreachable_source_is_reported_not_checked(self):
        results = monitor.report(self.ledger, self.lock, Sources(), TODAY, offline=False)
        network = [r for r in results if r["check"] in dict(monitor.NETWORK_CHECKS)]
        self.assertEqual([r["not_checked"] for r in network], ["OSError"] * 3)
        text = monitor.render(self.ledger, results, TODAY)
        self.assertIn("not checked (OSError)", text)
        self.assertNotIn("unavailable", text)

    def test_offline_reads_nothing(self):
        sources = Sources()
        results = monitor.report(self.ledger, self.lock, sources, TODAY, offline=True)
        self.assertEqual(sources.urls, [])
        self.assertEqual(sum("not_checked" in r for r in results), 3)

    def test_expired_review_deadline_is_a_triage_item(self):
        deadline = datetime.date.fromisoformat(self.ledger["triage"]["review_deadline"])
        self.assertEqual(monitor.check_review_deadline(self.ledger, deadline)["findings"], [])
        late = monitor.check_review_deadline(self.ledger, deadline + datetime.timedelta(days=1))
        self.assertEqual(len(late["findings"]), 1)
        self.assertIn(deadline.isoformat(), late["findings"][0])

    def test_render_lists_every_triage_item(self):
        interval = monitor.TIMELINE.format("3.53.4", "3.54.0")
        results = monitor.report(self.ledger, self.lock, self.sources(**{
            monitor.DOWNLOADS: downloads("3.54.0"),
            interval: timeline("c" * 10, self.ledger["triage"]["last_triaged_check_in"]),
        }), TODAY, offline=False)
        text = monitor.render(self.ledger, results, TODAY)
        self.assertIn("| SQLite upstream release |", text)
        self.assertIn("1 triage item(s)", text)
        self.assertIn("- SQLite 3.54.0 is released", text)


class CommandTests(unittest.TestCase):
    def run_script(self, *args, optimize="0", env=None):
        environment = dict(os.environ, PYTHONOPTIMIZE=optimize, PYTHONDONTWRITEBYTECODE="1")
        environment.pop("GITHUB_STEP_SUMMARY", None)
        environment.update(env or {})
        return subprocess.run([sys.executable, str(SCRIPT), *args], capture_output=True, text=True, env=environment, timeout=60)

    def test_offline_report_is_identical_under_python_optimization(self):
        outputs = set()
        for level in ("0", "1", "2"):
            result = self.run_script("--offline", "--today", TODAY.isoformat(), optimize=level)
            self.assertEqual((result.returncode, result.stderr), (0, ""), level)
            outputs.add(result.stdout)
        self.assertEqual(len(outputs), 1)
        self.assertIn("not checked (offline)", outputs.pop())

    def test_offline_report_is_appended_to_the_step_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            summary = Path(temporary) / "summary.md"
            summary.write_text("existing\n")
            result = self.run_script("--offline", "--today", TODAY.isoformat(), env={"GITHUB_STEP_SUMMARY": str(summary)})
            self.assertEqual(result.returncode, 0)
            self.assertEqual(summary.read_text(), "existing\n" + result.stdout)

    def test_a_broken_ledger_fails_without_raw_diagnostics(self):
        with tempfile.TemporaryDirectory() as temporary:
            ledger = Path(temporary) / "ledger.toml"
            ledger.write_text('[locked]\nrusqlite = "0.40.2"\n')
            for level in ("0", "1"):
                result = self.run_script("--offline", "--ledger", str(ledger), optimize=level)
                self.assertEqual(result.returncode, 1)
                self.assertEqual(result.stdout, "")
                self.assertEqual(result.stderr, "SQLite monitor could not run; raw diagnostics withheld for hygiene\n")


if __name__ == "__main__":
    unittest.main()
