#!/usr/bin/env python3
"""Offline Homebrew formula generator tests; fixtures are data only."""
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    "homebrew", Path(__file__).with_name("homebrew-formula.py"))
homebrew = importlib.util.module_from_spec(spec)
spec.loader.exec_module(homebrew)
FIXTURE = Path(__file__).resolve().parent / "fixtures" / "stable-sha256sums.txt"


class HomebrewFormulaTests(unittest.TestCase):
    def test_fixture_checksum_inventory(self):
        values = homebrew.checksums(FIXTURE)
        self.assertEqual(set(values), set(homebrew.TARGETS))

    def test_formula_uses_published_urls_and_checksums(self):
        values = homebrew.checksums(FIXTURE)
        version = homebrew.cargo_version()
        text = homebrew.formula(values, version)
        self.assertIn(f'version "{version}"', text)
        for target in homebrew.TARGETS:
            self.assertIn(
                f'https://github.com/mdml/ask/releases/download/v{version}/ask-{target}.tar.gz',
                text)
            self.assertIn(f'sha256 "{values[target]}"', text)

    def test_rejects_incomplete_or_duplicate_inventory(self):
        for body in [
            "aa" * 32 + "  ask-x86_64-unknown-linux-gnu.tar.gz\n",
            FIXTURE.read_text() + FIXTURE.read_text().splitlines()[0] + "\n",
        ]:
            with self.subTest(body=body[:40]):
                with tempfile.NamedTemporaryFile("w", delete=False) as handle:
                    handle.write(body)
                    path = Path(handle.name)
                try:
                    with self.assertRaises(ValueError):
                        homebrew.checksums(path)
                finally:
                    path.unlink(missing_ok=True)

    def test_version_must_be_stable_semver(self):
        values = homebrew.checksums(FIXTURE)
        with self.assertRaises(ValueError):
            homebrew.formula(values, "0.1.0-nightly.1")
        with mock.patch.object(
                homebrew.tomllib, "loads", return_value={"package": {"version": "0.1.0-nightly.1"}}):
            with self.assertRaises(ValueError):
                homebrew.cargo_version()

    def test_generator_writes_requested_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "ask.rb"
            with mock.patch.object(sys, "argv", [
                    "homebrew-formula.py", str(FIXTURE), "--output", str(output)]):
                homebrew.main()
            self.assertTrue(output.is_file())
            self.assertIn("class Ask < Formula", output.read_text())


if __name__ == "__main__":
    unittest.main()
