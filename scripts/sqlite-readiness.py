#!/usr/bin/env python3
"""Run only the isolated SQLite assessment; emit allowlisted evidence, never raw logs."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin", "aarch64-apple-darwin",
}


def require(condition, message="verification failed"):
    if not condition:
        raise ValueError(message)


def run(args, cwd, env, timeout=600):
    result = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed (exit {result.returncode}); raw diagnostics withheld")
    return result.stdout


def main():
    target = sys.argv[1]
    require(target in TARGETS)
    # A fresh workspace and target directory avoid inherited repository Cargo config
    # and cached native objects. Native/compiler/Rust flag overrides are not inherited.
    keys = ["PATH", "HOME", "CARGO_HOME", "RUSTUP_HOME", "TMPDIR", "SYSTEMROOT"]
    env = {k: os.environ[k] for k in keys if k in os.environ}
    env.update({"CARGO_TERM_COLOR": "never", "RUST_BACKTRACE": "0"})
    with tempfile.TemporaryDirectory(prefix="sqlite-readiness-") as temp:
        work = Path(temp)
        shutil.copytree(ROOT / "probes/sqlite-readiness", work / "probe")
        probe = work / "probe"
        env["CARGO_TARGET_DIR"] = str(work / "target")
        host = run(["rustc", "-vV"], work, env)
        require(f"host: {target}\n" in host, "native target runner required")
        require("release: 1.97.1\n" in host, "Rust 1.97.1 required")
        lock = tomllib.loads((probe / "Cargo.lock").read_text())
        versions = {p["name"]: p["version"] for p in lock["package"]}
        require(versions["rusqlite"] == "0.40.2")
        require(versions["libsqlite3-sys"] == "0.38.2")
        manifest = tomllib.loads((probe / "Cargo.toml").read_text())
        require(manifest["dependencies"] == {"rusqlite": {
            "version": "=0.40.2", "default-features": False, "features": ["bundled"]}})
        for command in [
            ["cargo", "fmt", "--", "--check"],
            ["cargo", "clippy", "--locked", "--target", target, "--", "-D", "warnings"],
            ["cargo", "build", "--locked", "--release", "--target", target],
        ]:
            run(command, probe, env)
        binary = work / "target" / target / "release/sqlite-readiness-probe"
        db = work / "database"
        db.mkdir()
        output = run([str(binary), str(db)], work, env, timeout=30)
        # Only the probe's fixed records and SQLite numeric compile options survive.
        for line in output.splitlines():
            require(re.fullmatch(r"[A-Za-z0-9_ =:.,/()-]+", line), "unexpected probe output")
        symbols = run(["nm", str(binary)], work, env)
        require(re.search(r"\b[Tt] _?sqlite3_libversion$", symbols, re.M))
        if "linux" in target:
            linkage = run(["readelf", "-d", str(binary)], work, env)
            libraries = re.findall(r"\(NEEDED\).*\[(.*?)\]", linkage)
            require(libraries)
        else:
            linkage = run(["otool", "-L", str(binary)], work, env)
            libraries = [Path(line.strip().split(" (", 1)[0]).name for line in linkage.splitlines()[1:]]
            require(libraries)
        require(all("sqlite" not in name.lower() for name in libraries))
        require(all(re.fullmatch(r"[A-Za-z0-9_.+-]+", name) for name in libraries))
        evidence = {
            "target": target, "rust": "1.97.1", "profile": "release",
            "lock_sha256": hashlib.sha256((probe / "Cargo.lock").read_bytes()).hexdigest(),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "linked_libraries_basenames": libraries,
            "sqlite3_libversion_defined_in_binary": True,
            "dynamic_sqlite_dependency": False,
            "fmt_clippy_build": "PASS", "probe_output": output.splitlines(),
        }
        print(json.dumps(evidence, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (Exception, KeyboardInterrupt):
        print("SQLite readiness verification failed; raw diagnostics withheld for hygiene", file=sys.stderr)
        sys.exit(1)
