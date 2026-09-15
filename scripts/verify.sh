#!/bin/sh
# Single verification entrypoint for local development and CI.
# Usage:
#   scripts/verify.sh              Fast gate (default)
#   scripts/verify.sh --full         Full gate
#   scripts/verify.sh --full --base REF
#   scripts/verify.sh --full --all   CodeScene on entire tree
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FULL=0
BASE="${ASK_VERIFY_BASE:-origin/main}"
CS_ALL=0
COVERAGE_MIN="${ASK_COVERAGE_MIN:-90}"

usage() {
    cat <<'EOF'
Usage: scripts/verify.sh [--full] [--base REF] [--all]

  (default)     Fast gate: fmt, clippy, build, doc, coverage, CodeScene on staged files
  --full        Full gate: fast gate plus cargo deny and CodeScene on files changed from base
  --base REF    Base ref for --full CodeScene diff (default: origin/main, or ASK_VERIFY_BASE)
  --all         With --full, run CodeScene on the entire tree instead of diff from base
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --full)
            FULL=1
            shift
            ;;
        --base)
            BASE="${2:?--base requires a ref}"
            shift 2
            ;;
        --all)
            CS_ALL=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "verify.sh: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "verify.sh: required command not found: $1" >&2
        echo "verify.sh: install pinned tools with: mise install" >&2
        exit 127
    fi
}

require_cmd rustup
require_cmd python3

# Reject compiler and wrapper overrides before Cargo can execute them. Inspect
# both config names at every Cargo search location, without changing any files.
python3 - <<'PYTHON'
import os
from pathlib import Path
import re
import sys
import tomllib

selection = {
    "RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC", "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "RUSTFMT", "CLIPPY_DRIVER",
    "LLVM_COV", "LLVM_PROFDATA",
}
helper_aliases = {"fmt", "clippy", "llvm-cov", "deny"}
target_override = re.compile(r"CARGO_TARGET_[A-Z0-9_]+_(?:LINKER|RUSTFLAGS|RUNNER)$")
def reject(source):
    print(f"verify.sh: toolchain override in {source}; remove it for pinned verification", file=sys.stderr)
    sys.exit(2)

for key in sorted(selection):
    if os.environ.get(key):
        reject(key)
for key in sorted(os.environ):
    if key in {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"} or target_override.fullmatch(key):
        reject(key)
for alias in helper_aliases:
    key = f"CARGO_ALIAS_{alias.upper().replace('-', '_')}"
    if os.environ.get(key):
        reject(key)
root = Path.cwd()
locations = [p / ".cargo" for p in (root, *root.parents)]
locations.append(Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")))
for directory in locations:
    for name in ("config", "config.toml"):
        path = directory / name
        if not path.is_file():
            continue
        try:
            config = tomllib.loads(path.read_text())
        except (ValueError, OSError):
            reject("unreadable Cargo config")
        if "include" in config:
            reject("Cargo config include")
        for key in ("rustc", "rustdoc", "rustc-wrapper", "rustc-workspace-wrapper"):
            if key in config.get("build", {}):
                reject(f"Cargo config build.{key}")
        if "rustflags" in config.get("build", {}):
            reject("Cargo config build.rustflags")
        for target, values in config.get("target", {}).items():
            if isinstance(values, dict):
                for key in ("linker", "rustflags", "runner"):
                    if key in values:
                        reject(f"Cargo config target.{target}.{key}")
        for key in config.get("env", {}):
            if key in selection or key == "PATH" or key.startswith(("RUST", "CARGO", "CLIPPY", "LLVM")):
                reject(f"Cargo config env.{key}")
        for alias in helper_aliases:
            if alias in config.get("alias", {}):
                reject(f"Cargo config alias.{alias}")
PYTHON

PINNED_TOOLCHAIN="$(python3 -c 'import tomllib; print(tomllib.load(open("rust-toolchain.toml", "rb"))["toolchain"]["channel"])')"
export RUSTUP_TOOLCHAIN="$PINNED_TOOLCHAIN"
CARGO="$(rustup which --toolchain "$PINNED_TOOLCHAIN" cargo)"
RUSTC="$(rustup which --toolchain "$PINNED_TOOLCHAIN" rustc)"
RUSTDOC="$(rustup which --toolchain "$PINNED_TOOLCHAIN" rustdoc)"
RUSTFMT="$(rustup which --toolchain "$PINNED_TOOLCHAIN" rustfmt)"
CLIPPY_DRIVER="$(rustup which --toolchain "$PINNED_TOOLCHAIN" clippy-driver)"
# Cargo subcommands and their children must also resolve the pinned binaries.
for tool in cargo-fmt cargo-clippy; do
    rustup which --toolchain "$PINNED_TOOLCHAIN" "$tool" >/dev/null
done
PATH="$(dirname "$CARGO"):$PATH"
SYSROOT="$("$RUSTC" --print sysroot)"
HOST="$("$RUSTC" -vV | sed -n 's/^host: //p')"
LLVM_COV="$SYSROOT/lib/rustlib/$HOST/bin/llvm-cov"
LLVM_PROFDATA="$SYSROOT/lib/rustlib/$HOST/bin/llvm-profdata"
for tool in "$LLVM_COV" "$LLVM_PROFDATA"; do
    if [ ! -x "$tool" ]; then
        echo "verify.sh: pinned llvm-tools missing; run rustup component add --toolchain $PINNED_TOOLCHAIN llvm-tools" >&2
        exit 2
    fi
done
export CARGO RUSTC RUSTDOC RUSTFMT CLIPPY_DRIVER PATH LLVM_COV LLVM_PROFDATA
echo "==> toolchain $PINNED_TOOLCHAIN (rust-toolchain.toml)"

echo "==> pinned toolchain regression"
scripts/verify-toolchain-test.sh

run_codescene_fast() {
    echo "==> CodeScene (fast gate)"
    if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
        scripts/codescene.sh --commit HEAD
    else
        scripts/codescene.sh --staged
    fi
}

run_codescene_full() {
    echo "==> CodeScene (full gate)"
    if [ "$CS_ALL" -eq 1 ]; then
        scripts/codescene.sh --all
    else
        scripts/codescene.sh --base "$BASE"
    fi
}

echo "==> cargo fmt --check"
require_cmd rustfmt
"$CARGO" fmt --all -- --check

echo "==> cargo clippy"
require_cmd clippy-driver
"$CARGO" clippy --all-targets --all-features --locked -- -D warnings

echo "==> cargo build"
"$CARGO" build --all-targets --locked

echo "==> cargo doc"
RUSTDOCFLAGS="-D warnings" "$CARGO" doc --no-deps --locked

echo "==> cargo llvm-cov (minimum ${COVERAGE_MIN}% line coverage)"
require_cmd cargo-llvm-cov
"$CARGO" llvm-cov --all-features --locked --fail-under-lines "$COVERAGE_MIN" --summary-only

run_codescene_fast

if [ "$FULL" -eq 1 ]; then
    echo "==> cargo deny"
    require_cmd cargo-deny
    "$CARGO" deny --locked check

    run_codescene_full
fi

echo "verify.sh: all checks passed"
