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
BASE="${ASK_VERIFY_BASE:-origin/staging}"
CS_ALL=0
COVERAGE_MIN="${ASK_COVERAGE_MIN:-90}"

usage() {
    cat <<'EOF'
Usage: scripts/verify.sh [--full] [--base REF] [--all]

  (default)     Fast gate: fmt, clippy, build, doc, coverage, CodeScene on staged files
  --full        Full gate: fast gate plus cargo deny and CodeScene on files changed from base
  --base REF    Base ref for --full CodeScene diff (default: origin/staging, or ASK_VERIFY_BASE)
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

require_cmd cargo
require_cmd python3

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
cargo fmt --all -- --check

echo "==> cargo clippy"
require_cmd clippy-driver
cargo clippy --all-targets --all-features --locked -- -D warnings

echo "==> cargo build"
cargo build --all-targets --locked

echo "==> cargo doc"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked

echo "==> cargo llvm-cov (minimum ${COVERAGE_MIN}% line coverage)"
require_cmd cargo-llvm-cov
cargo llvm-cov --all-features --locked --fail-under-lines "$COVERAGE_MIN" --summary-only

run_codescene_fast

if [ "$FULL" -eq 1 ]; then
    echo "==> cargo deny"
    require_cmd cargo-deny
    cargo deny --locked check

    run_codescene_full
fi

echo "verify.sh: all checks passed"
