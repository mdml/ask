#!/usr/bin/env python3
"""Offline stable release preparation tests; fixtures are data, never executables."""
import datetime
import importlib.util
import io
import json
import os
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    "stable", Path(__file__).with_name("stable-release.py"))
stable = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stable)
release = stable.release
SHA = "a" * 40
TAG = stable.stable_tag()


class StableReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "incoming"
        self.source.mkdir()
        for target in release.TARGETS:
            self.fixture(target)

    def fixture(self, target):
        info = release.identity(SHA, TAG)
        info.update(target=target, binary_sha256=release.digest(b"inert binary fixture"),
                    linkage={"sqlite3_open_v2_defined": True, "dynamic_sqlite": False,
                             "libraries": ["libc.so.6"]})
        files = [("ask", b"inert binary fixture", 0o755),
                 ("LICENSE", (release.ROOT / "LICENSE").read_bytes(), 0o644),
                 ("manifest.json", json.dumps(info).encode(), 0o644)]
        directory = self.source / target
        directory.mkdir(exist_ok=True)
        archive = directory / release.archive_name(target)
        with tarfile.open(archive, "w:gz") as stream:
            for name, data, mode in files:
                member = tarfile.TarInfo(name)
                member.size = len(data)
                member.mode = mode
                stream.addfile(member, io.BytesIO(data))
        archive.with_name(archive.name + ".sha256").write_text(
            f"{release.digest(archive.read_bytes())}  {archive.name}\n")

    def prepare(self, refs, environment=None):
        output = self.root / "outputs"
        output.unlink(missing_ok=True)
        calls = []

        def run(args):
            calls.append(args)
            if args[0] == "git":
                return SHA + "\n"
            self.assertEqual(args[:2], ["gh", "api"], args)
            self.assertNotIn("--method", args)
            path = args[2]
            self.assertTrue(path.startswith("repos/mdml/ask/git/matching-refs/tags/"), path)
            tag = path.rsplit("/", 1)[1]
            return json.dumps(refs.get(tag, []))

        variables = {
            "RELEASE_SHA": SHA,
            "GITHUB_OUTPUT": str(output),
            "GITHUB_REPOSITORY": "mdml/ask",
            "GITHUB_REF": "refs/heads/stable",
            "GITHUB_REF_PROTECTED": "true",
            "GITHUB_EVENT_NAME": "push",
            **(environment or {}),
        }
        date = datetime.datetime(2026, 9, 15, tzinfo=datetime.timezone.utc)
        with patch.object(release.datetime, "datetime", wraps=datetime.datetime) as clock, \
                patch.object(release, "run", side_effect=run), \
                patch.object(sys, "argv", ["stable-release.py", "prepare"]), \
                patch.dict(os.environ, variables, clear=False):
            clock.now.return_value = date
            stable.main()
        return output.read_text(), calls

    def test_prepare_emits_stable_tag_when_absent(self):
        outputs, calls = self.prepare({TAG: []})
        self.assertEqual(outputs, f"tag={TAG}\n")
        self.assertEqual(calls[0], ["git", "rev-parse", "HEAD"])
        self.assertIn(["gh", "api", f"repos/mdml/ask/git/matching-refs/tags/{TAG}"], calls)

    def test_prepare_refuses_existing_tag(self):
        with self.assertRaises(ValueError):
            self.prepare({TAG: [{"ref": f"refs/tags/{TAG}"}]})
        self.assertFalse((self.root / "outputs").exists())

    def test_prepare_requires_canonical_repository_and_branch(self):
        for key, value in [
            ("GITHUB_REPOSITORY", "other/ask"),
            ("GITHUB_REF", "refs/heads/main"),
            ("GITHUB_REF_PROTECTED", "false"),
            ("GITHUB_EVENT_NAME", "workflow_dispatch"),
        ]:
            with self.subTest(key=key, value=value):
                with self.assertRaises(ValueError):
                    self.prepare({}, {key: value})

    def test_prepare_rejects_expired_disposition_and_wrong_checkout(self):
        output = self.root / "outputs"
        for sha, date in [
            (SHA, datetime.datetime(2026, 9, 30, tzinfo=datetime.timezone.utc)),
            ("b" * 40, datetime.datetime(2026, 9, 15, tzinfo=datetime.timezone.utc)),
        ]:
            with self.subTest(sha=sha, date=date):
                with patch.object(release.datetime, "datetime") as clock, \
                        patch.object(release, "run", return_value=sha + "\n"), \
                        patch.object(sys, "argv", ["stable-release.py", "prepare"]), \
                        patch.dict(os.environ, {
                            "RELEASE_SHA": SHA,
                            "GITHUB_OUTPUT": str(output),
                            "GITHUB_REPOSITORY": "mdml/ask",
                            "GITHUB_REF": "refs/heads/stable",
                            "GITHUB_REF_PROTECTED": "true",
                            "GITHUB_EVENT_NAME": "push",
                        }):
                    clock.now.return_value = date
                    with self.assertRaises(ValueError):
                        stable.main()
                self.assertFalse(output.exists())

    def draft_release(self, destination):
        release.verify(self.source, destination, SHA, TAG)
        return [{"name": p.name, "size": p.stat().st_size,
                 "state": "uploaded", "digest": "sha256:" + release.digest(p.read_bytes())}
                for p in destination.iterdir()]

    def test_verify_upload_requires_non_prerelease_draft(self):
        destination = self.root / "release"
        assets = self.draft_release(destination)
        valid = {"tag_name": TAG, "target_commitish": SHA,
                 "draft": True, "prerelease": False, "assets": assets}
        release.verify_upload(valid, destination, SHA, TAG, prerelease=False)
        with self.assertRaises(ValueError):
            release.verify_upload({**valid, "prerelease": True}, destination, SHA, TAG, prerelease=False)

    def test_verify_upload_rejects_bad_asset_digests_and_inventory(self):
        destination = self.root / "release"
        assets = self.draft_release(destination)
        valid = {"tag_name": TAG, "target_commitish": SHA,
                 "draft": True, "prerelease": False, "assets": assets}
        for key, value in [("digest", "sha256:" + "0" * 64), ("size", 0),
                           ("state", "new"), ("name", "../../escape")]:
            with self.subTest(key=key):
                original = assets[0][key]
                assets[0][key] = value
                with self.assertRaises(ValueError):
                    release.verify_upload(valid, destination, SHA, TAG, prerelease=False)
                assets[0][key] = original
        valid["assets"] = assets[:-1]
        with self.assertRaises(ValueError):
            release.verify_upload(valid, destination, SHA, TAG, prerelease=False)

    def test_workflow_guards_and_publication_path(self):
        workflow = (release.ROOT / ".github/workflows/stable-release.yml").read_text()
        self.assertIn("github.repository == 'mdml/ask'", workflow)
        self.assertIn("refs/heads/stable", workflow)
        self.assertIn("scripts/verify.sh --full --all", workflow)
        self.assertIn("cargo deny --locked check advisories", workflow)
        self.assertIn("scripts/stable-release.py prepare", workflow)
        self.assertIn("-F draft=false -F prerelease=false", workflow)
        self.assertIn("--verify-tag", workflow)
        prepare = workflow.split("  verify-full:", 1)[0]
        self.assertIn("GH_TOKEN: ${{ github.token }}", prepare)
        self.assertNotIn("contents: write", prepare)
        self.assertNotIn("RELEASE_REPAIR", workflow)
        self.assertNotIn("publish=false", workflow)

    def publish_shell(self, response):
        fake_bin = self.root / "bin"
        fake_bin.mkdir()
        calls = self.root / "gh-calls"
        fake_gh = fake_bin / "gh"
        fake_gh.write_text(f'''#!/bin/sh
printf '%s\\n' "$*" >> "$GH_CALLS"
case "$*" in
  *"releases/tags/"*) echo 'not found' >&2; exit 1 ;;
  "release view "*) printf '%s\\n' '123' ;;
  *"releases/123"*) printf '%s\\n' '{response}' ;;
esac
''')
        fake_gh.chmod(fake_gh.stat().st_mode | stat.S_IXUSR)
        workflow = (release.ROOT / ".github/workflows/stable-release.yml").read_text()
        publish = workflow.split("        run: |\n", 1)[1].split("\n      -", 1)[0]
        scripts = self.root / "scripts"
        scripts.mkdir(exist_ok=True)
        for name in ("stable-release.py", "nightly-release.py"):
            (scripts / name).symlink_to(release.ROOT / "scripts" / name)
        environment = {"PATH": f"{fake_bin}:{os.environ['PATH']}", "GH_CALLS": str(calls),
                       "GITHUB_REPOSITORY": "mdml/ask", "RELEASE_TAG": TAG, "RELEASE_SHA": SHA}
        result = subprocess.run(["bash", "-c", publish], cwd=self.root, env=environment,
                                text=True, capture_output=True)
        return result, calls.read_text()

    def test_publish_shell_verifies_draft_by_database_id_before_publication(self):
        destination = self.root / "release"
        assets = self.draft_release(destination)
        response = json.dumps({"tag_name": TAG, "target_commitish": SHA, "draft": True,
                               "prerelease": False, "assets": assets})
        result, recorded = self.publish_shell(response)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"release view {TAG} --repo mdml/ask --json databaseId --jq .databaseId", recorded)
        self.assertIn("api repos/mdml/ask/releases/123", recorded)
        self.assertIn("api --method PATCH repos/mdml/ask/releases/123 -F draft=false -F prerelease=false -f make_latest=true", recorded)
        self.assertNotIn("releases/tags/", recorded)

    def test_publish_shell_aborts_when_verify_upload_fails_on_bad_asset_digest(self):
        destination = self.root / "release"
        assets = self.draft_release(destination)
        assets[0]["digest"] = "sha256:" + "0" * 64
        response = json.dumps({"tag_name": TAG, "target_commitish": SHA, "draft": True,
                               "prerelease": False, "assets": assets})
        result, recorded = self.publish_shell(response)
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("api repos/mdml/ask/releases/123", recorded)
        self.assertNotIn("draft=false", recorded)


if __name__ == "__main__":
    unittest.main()
