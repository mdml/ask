#!/usr/bin/env python3
"""Offline packaging safety tests; fixtures are data, never executables."""
import hashlib
import datetime
import os
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import sys
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("nightly", Path(__file__).with_name("nightly-release.py"))
nightly = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nightly)
SHA = "a" * 40
TAG = f"v{nightly.version()}-nightly.20260915.123.1"


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "incoming"
        self.source.mkdir()
        self.target = nightly.TARGETS[0]
        for target in nightly.TARGETS:
            self.fixture(target)

    def fixture(self, target, mutate=None):
        info = nightly.identity(SHA, TAG)
        info.update(target=target, binary_sha256=nightly.digest(b"inert binary fixture"),
                    linkage={"sqlite3_open_v2_defined": True, "dynamic_sqlite": False,
                             "libraries": ["libc.so.6"]})
        files = [("ask", b"inert binary fixture", tarfile.REGTYPE, 0o755),
                 ("LICENSE", (nightly.ROOT / "LICENSE").read_bytes(), tarfile.REGTYPE, 0o644),
                 ("manifest.json", json.dumps(info).encode(), tarfile.REGTYPE, 0o644)]
        if mutate:
            files = mutate(files)
        directory = self.source / target
        directory.mkdir(exist_ok=True)
        archive = directory / nightly.archive_name(target)
        with tarfile.open(archive, "w:gz") as stream:
            for name, data, kind, mode in files:
                member = tarfile.TarInfo(name)
                member.type, member.mode = kind, mode
                member.size = len(data)
                member.linkname = "/tmp/never-extract"
                stream.addfile(member, io.BytesIO(data))
        archive.with_name(archive.name + ".sha256").write_text(f"{nightly.digest(archive.read_bytes())}  {archive.name}\n")
        return archive

    def reject(self):
        with self.assertRaises((ValueError, tarfile.TarError)):
            nightly.verify(self.source, self.root / "release", SHA, TAG)
        self.assertFalse((self.root / "release").exists())

    def test_prepare_uses_cargo_version_run_and_attempt(self):
        output = self.root / "outputs"
        date = datetime.datetime(2026, 9, 15, tzinfo=datetime.timezone.utc)
        with patch.object(nightly.datetime, "datetime") as clock, \
                patch.object(nightly, "run", return_value=SHA + "\n"), \
                patch.object(sys, "argv", ["nightly-release.py", "prepare"]), \
                patch.dict(os.environ, {"RELEASE_SHA": SHA, "GITHUB_RUN_ID": "123",
                                        "GITHUB_RUN_ATTEMPT": "1", "GITHUB_OUTPUT": str(output)}):
            clock.now.return_value = date
            nightly.main()
        self.assertEqual(output.read_text(), f"tag={TAG}\n")

    def test_prepare_rejects_expired_disposition_and_wrong_checkout(self):
        output = self.root / "outputs"
        for sha, date in [(SHA, datetime.datetime(2026, 9, 30, tzinfo=datetime.timezone.utc)),
                          ("b" * 40, datetime.datetime(2026, 9, 15, tzinfo=datetime.timezone.utc))]:
            with self.subTest(sha=sha, date=date):
                with patch.object(nightly.datetime, "datetime") as clock, \
                        patch.object(nightly, "run", return_value=sha + "\n"), \
                        patch.object(sys, "argv", ["nightly-release.py", "prepare"]), \
                        patch.dict(os.environ, {"RELEASE_SHA": SHA, "GITHUB_OUTPUT": str(output)}):
                    clock.now.return_value = date
                    with self.assertRaises(ValueError):
                        nightly.main()
                self.assertFalse(output.exists())

    def test_complete_inventory_and_checksums(self):
        destination = self.root / "release"
        nightly.verify(self.source, destination, SHA, TAG)
        self.assertEqual(len(list(destination.iterdir())), 5)
        for line in (destination / "SHA256SUMS").read_text().splitlines():
            checksum, filename = line.split("  ")
            self.assertEqual(checksum, hashlib.sha256((destination / filename).read_bytes()).hexdigest())

    def test_uploaded_asset_inventory_and_digests(self):
        destination = self.root / "release"
        nightly.verify(self.source, destination, SHA, TAG)
        assets = [{"name": p.name, "size": p.stat().st_size,
                   "state": "uploaded", "digest": "sha256:" + nightly.digest(p.read_bytes())}
                  for p in destination.iterdir()]
        release = {"tag_name": TAG, "target_commitish": SHA,
                   "draft": True, "prerelease": True, "assets": assets}
        nightly.verify_upload(release, destination, SHA, TAG)
        for key, value in [("digest", "sha256:" + "0" * 64), ("size", 0),
                           ("state", "new"), ("name", "../../escape")]:
            with self.subTest(key=key):
                original = assets[0][key]
                assets[0][key] = value
                with self.assertRaises(ValueError):
                    nightly.verify_upload(release, destination, SHA, TAG)
                assets[0][key] = original
        release["assets"] = assets[:-1]
        with self.assertRaises(ValueError):
            nightly.verify_upload(release, destination, SHA, TAG)

    def test_missing_target(self):
        import shutil
        shutil.rmtree(self.source / self.target)
        self.reject()

    def test_unexpected_target(self):
        (self.source / "extra").mkdir()
        self.reject()

    def test_unexpected_artifact_file(self):
        (self.source / self.target / "payload.py").write_text("never execute")
        self.reject()

    def test_archive_digest_mismatch(self):
        archive = self.source / self.target / nightly.archive_name(self.target)
        archive.write_bytes(archive.read_bytes() + b"tampered")
        self.reject()

    def test_missing_checksum(self):
        (self.source / self.target / (nightly.archive_name(self.target) + ".sha256")).unlink()
        self.reject()

    def test_archive_symlink(self):
        archive = self.source / self.target / nightly.archive_name(self.target)
        elsewhere = self.root / "elsewhere"
        archive.rename(elsewhere)
        archive.symlink_to(elsewhere)
        self.reject()

    def test_target_directory_symlink(self):
        directory = self.source / self.target
        elsewhere = self.root / "elsewhere"
        directory.rename(elsewhere)
        directory.symlink_to(elsewhere, target_is_directory=True)
        self.reject()

    def test_archive_member_attacks(self):
        for name, kind in [("../escape", tarfile.REGTYPE), ("/absolute", tarfile.REGTYPE),
                           ("ask", tarfile.SYMTYPE), ("ask", tarfile.LNKTYPE),
                           ("ask", tarfile.FIFOTYPE)]:
            with self.subTest(name=name, kind=kind):
                self.fixture(self.target, lambda files: [(name, b"data", kind, 0o755)] + files[1:])
                self.reject()

    def test_duplicate_member(self):
        self.fixture(self.target, lambda files: files + [files[0]])
        self.reject()

    def test_missing_member(self):
        self.fixture(self.target, lambda files: files[1:])
        self.reject()

    def test_setuid_member(self):
        self.fixture(self.target, lambda files: [("ask", files[0][1], tarfile.REGTYPE, 0o4755)] + files[1:])
        self.reject()

    def test_binary_checksum_mismatch(self):
        self.fixture(self.target, lambda files: [("ask", b"changed", tarfile.REGTYPE, 0o755)] + files[1:])
        self.reject()

    def test_manifest_identity_mismatches(self):
        for key, value in [("source_sha", "b" * 40), ("tag", TAG + "0"),
                           ("target", nightly.TARGETS[1]), ("rust", "1.98.1"),
                           ("cargo_version", "9.0.0"), ("lock_sha256", "0" * 64),
                           ("linkage", {"sqlite3_open_v2_defined": True, "dynamic_sqlite": False,
                                        "libraries": ["libsqlite3.so.0"]})]:
            with self.subTest(key=key):
                def mutate(files):
                    info = json.loads(files[2][1])
                    info[key] = value
                    return files[:2] + [("manifest.json", json.dumps(info).encode(), tarfile.REGTYPE, 0o644)]
                self.fixture(self.target, mutate)
                self.reject()

    def test_linkage_requires_defined_symbol_and_no_dynamic_sqlite(self):
        for symbols, libraries in [("U sqlite3_open_v2\n", "libc.so.6"),
                                   ("0000 T sqlite3_open_v2\n", "libsqlite3.so.0")]:
            with self.subTest(symbols=symbols, libraries=libraries):
                with patch.object(nightly, "run", side_effect=[symbols, f"(NEEDED) [{libraries}]\n"]):
                    with self.assertRaises(ValueError):
                        nightly.linkage(Path("inert"), "x86_64-unknown-linux-gnu")

    def test_native_linkage_formats(self):
        for target, output in [("x86_64-unknown-linux-gnu", "(NEEDED) [libc.so.6]\n"),
                               ("aarch64-apple-darwin", "ask:\n\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0)\n")]:
            with self.subTest(target=target):
                with patch.object(nightly, "run", side_effect=["0000 T _sqlite3_open_v2\n", output]):
                    self.assertFalse(nightly.linkage(Path("inert"), target)["dynamic_sqlite"])

    def test_packager_checks_linkage_before_strip_and_roundtrips(self):
        repo = self.root / "repo"
        repo.mkdir()
        for name in ["Cargo.toml", "Cargo.lock", "LICENSE"]:
            (repo / name).write_bytes((nightly.ROOT / name).read_bytes())
        target = "x86_64-unknown-linux-gnu"
        binary = repo / "target" / target / "release/ask"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"unstripped inert binary")
        calls = []
        def run(args):
            calls.append(args[0])
            if args[0] == "rustc":
                return f"host: {target}\nrelease: 1.97.1\n"
            if args[0] == "nm":
                return "0000 T sqlite3_open_v2\n"
            if args[0] == "readelf":
                return "(NEEDED) [libc.so.6]\n"
            if args[0] == "strip":
                Path(args[1]).write_bytes(b"stripped inert binary")
                return ""
            self.fail("unexpected command")
        with patch.object(nightly, "ROOT", repo), patch.object(nightly, "run", side_effect=run):
            nightly.package(target, SHA, TAG, self.root / "package")
            nightly.validate_target(self.root / "package", target, SHA, TAG)
        self.assertEqual(calls, ["rustc", "nm", "readelf", "strip"])
        self.assertEqual(binary.read_bytes(), b"unstripped inert binary")

    def test_reject_wrong_native_host_before_packaging(self):
        with patch.object(nightly, "run", return_value="host: wrong\nrelease: 1.97.1\n"):
            with self.assertRaises(ValueError):
                nightly.package(self.target, SHA, TAG, self.root / "package")
        self.assertFalse((self.root / "package").exists())


if __name__ == "__main__":
    unittest.main()
