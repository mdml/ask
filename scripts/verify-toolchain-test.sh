#!/bin/sh
# Exercise the real gate on a dependency-free crate; only CodeScene and this
# regression's nested invocation are replaced, so the test cannot recurse.
set -eu
ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
python3 - "$ROOT" <<'PY'
import os
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib

root = Path(sys.argv[1])
pin = tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
with tempfile.TemporaryDirectory(prefix="ask-toolchain-") as tmp:
    parent = Path(tmp)
    fixture = parent / "crate"
    scripts = fixture / "scripts"
    scripts.mkdir(parents=True)
    shutil.copy2(root / "scripts/verify.sh", scripts / "verify.sh")
    shutil.copy2(root / "rust-toolchain.toml", fixture / "rust-toolchain.toml")
    for name in ("codescene.sh", "verify-toolchain-test.sh"):
        path = scripts / name
        path.write_text("#!/bin/sh\nexit 0\n")
        path.chmod(0o755)
    (fixture / "Cargo.toml").write_text('[package]\nname="pin-proof"\nversion="0.0.0"\nedition="2021"\n')
    (fixture / "src").mkdir()
    (fixture / "src/lib.rs").write_text('#[test]\nfn proof() {\n    assert_eq!(2 + 2, 4);\n}\n')
    # Run real tools from inside Cargo's build environment, not a shell-only guard.
    versions = {}
    for tool in ("cargo", "rustc", "rustdoc", "clippy-driver"):
        binary = subprocess.check_output(["rustup", "which", "--toolchain", pin, tool], text=True).strip()
        versions[tool] = subprocess.check_output([binary, "--version"], text=True).strip()
    pairs = ",".join(f"({json.dumps(tool)}, {json.dumps(version)})" for tool, version in versions.items())
    (fixture / "build.rs").write_text('''fn main() {
    for (tool, expected) in [''' + pairs + '''] {
        let output = std::process::Command::new(tool).arg("--version").output().unwrap();
        assert!(output.status.success(), "{tool} failed");
        let version = String::from_utf8(output.stdout).unwrap();
        assert_eq!(version.trim(), expected, "{tool}");
    }
}
''')
    env = os.environ.copy()
    # Tests control overrides independently of the gate's already-pinned env.
    overrides = ("RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
                 "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC", "CARGO_BUILD_RUSTC_WRAPPER",
                 "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "RUSTFMT", "CLIPPY_DRIVER",
                 "LLVM_COV", "LLVM_PROFDATA")
    for key in (*overrides, "RUSTUP_TOOLCHAIN", "CARGO_TARGET_DIR", "CARGO_ENCODED_RUSTFLAGS", "RUSTFLAGS"):
        env.pop(key, None)
    pinned_cargo = subprocess.check_output(["rustup", "which", "--toolchain", pin, "cargo"], env=env, text=True).strip()
    subprocess.run([pinned_cargo, "generate-lockfile", "--offline"], cwd=fixture, env=env, check=True, capture_output=True)
    subprocess.run([pinned_cargo, "fmt", "--all"], cwd=fixture,
                   env=env | {"RUSTUP_TOOLCHAIN": pin}, check=True, capture_output=True)
    shadow = parent / "bin"
    shadow.mkdir()
    marker = parent / "wrong-tool"
    for tool in ("cargo", "rustc", "rustdoc", "cargo-clippy", "clippy-driver", "cargo-fmt", "rustfmt"):
        path = shadow / tool
        path.write_text(f'#!/bin/sh\necho wrong > "{marker}"\nexit 91\n')
        path.chmod(0o755)
    cargo_only = parent / "cargo-only"
    cargo_only.mkdir()
    shutil.copy2(shadow / "cargo", cargo_only / "cargo")
    (shadow / "cargo-deny").write_text(f'#!/bin/sh\necho wrong > "{marker}"\nexit 91\n')
    (shadow / "cargo-deny").chmod(0o755)
    failures = []

    def check(label, changes=None, reject=False, args=()):
        marker.unlink(missing_ok=True)
        shutil.rmtree(fixture / "target", ignore_errors=True)
        result = subprocess.run([str(scripts / "verify.sh"), *args], cwd=fixture,
                                env=env | (changes or {}), text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        good = (result.returncode == 2 and "toolchain override" in result.stdout) if reject else (
            result.returncode == 0 and "verify.sh: all checks passed" in result.stdout)
        if not good or marker.exists():
            failures.append(label)
            print(f"FAIL [{label}] (exit {result.returncode})\n{result.stdout}")
        else:
            print(f"ok [{label}]", flush=True)

    check("baseline")
    check("bogus ambient rustup override", {"RUSTUP_TOOLCHAIN": "ask-nonexistent-toolchain"})
    check("shadow Cargo alone on PATH", {"PATH": f'{cargo_only}:{env["PATH"]}'})
    check("shadow Cargo and Rust tools on PATH", {"PATH": f'{shadow}:{env["PATH"]}'})
    for key in overrides:
        check(key, {key: str(shadow / "rustc")}, reject=True)
    for location in (fixture / ".cargo", parent / ".cargo", parent / "cargo-home"):
        location.mkdir(exist_ok=True)
        for section, key in (("build", "rustc"), ("build", "rustdoc"), ("build", "rustc-wrapper"),
                             ("build", "rustc-workspace-wrapper"), ("env", "RUSTC"), ("env", "PATH")):
            config = location / "config.toml"
            value = f'"{shadow / "rustc"}"' if section == "build" else '{ value = "bogus", force = true }'
            config.write_text(f'[{section}]\n{key} = {value}\n')
            changes = {"CARGO_HOME": str(location)} if location.name == "cargo-home" else {}
            check(f"{location.relative_to(parent)}/{section}.{key}", changes, reject=True)
            config.unlink()
    for location in (fixture / ".cargo", parent / ".cargo", parent / "cargo-home"):
        location.mkdir(exist_ok=True)
        config = location / "config.toml"
        changes = {"CARGO_HOME": str(location)} if location.name == "cargo-home" else {}
        config.write_text('[alias]\nllvm-cov = "!true"\n')
        check(f"{location.relative_to(parent)}/alias.llvm-cov", changes, reject=True)
        config.unlink()
    check("CARGO_ALIAS_LLVM_COV", {"CARGO_ALIAS_LLVM_COV": "!true"}, reject=True)
    check("full gate CARGO_ALIAS_DENY", {"CARGO_ALIAS_DENY": "!true", "PATH": f'{shadow}:{env["PATH"]}'}, reject=True,
          args=("--full", "--base", "HEAD"))
    if failures:
        sys.exit(1)
PY
