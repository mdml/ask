#!/bin/sh
# CodeScene Code Health gate for eligible Rust source files.
# Requires CS_ACCESS_TOKEN (never printed). Exit 3 when unset.
#
# Usage:
#   scripts/codescene.sh --all
#   scripts/codescene.sh --staged
#   scripts/codescene.sh --commit [REF]
#   scripts/codescene.sh --base REF
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CS_INSTALL_DIR="$ROOT/target/codescene"
MODE=""
BASE_REF=""
COMMIT_REF="HEAD"

if [ -z "${CS_ACCESS_TOKEN:-}" ]; then
    echo "codescene.sh: CS_ACCESS_TOKEN is not set." >&2
    echo "codescene.sh: obtain a PAT from the CodeScene CLI and load it via direnv (.envrc)." >&2
    exit 3
fi

usage() {
    cat <<'EOF'
Usage: scripts/codescene.sh --all | --staged | --commit [REF] | --base REF
EOF
}

ensure_cs() {
    if command -v cs >/dev/null 2>&1; then
        CS_CMD="cs"
        return 0
    fi

    if [ -x "$CS_INSTALL_DIR/.local/bin/cs" ]; then
        CS_CMD="$CS_INSTALL_DIR/.local/bin/cs"
        return 0
    fi

    echo "codescene.sh: cs not found on PATH; installing to $CS_INSTALL_DIR" >&2
    mkdir -p "$CS_INSTALL_DIR"
    curl -fsSL https://downloads.codescene.io/enterprise/cli/install-cs-tool.sh |
        HOME="$CS_INSTALL_DIR" sh -s -- -y

    if [ -x "$CS_INSTALL_DIR/.local/bin/cs" ]; then
        CS_CMD="$CS_INSTALL_DIR/.local/bin/cs"
    else
        echo "codescene.sh: cs installation failed" >&2
        exit 127
    fi
}

list_files() {
    case "$MODE" in
        all)
            git ls-files '*.rs'
            ;;
        staged)
            git diff --cached --name-only --diff-filter=ACMR -- '*.rs'
            ;;
        commit)
            git diff-tree --no-commit-id --name-only -r "$COMMIT_REF" -- '*.rs' 2>/dev/null ||
                git show --name-only --pretty=format: "$COMMIT_REF" -- '*.rs'
            ;;
        base)
            if ! git rev-parse --verify "$BASE_REF" >/dev/null 2>&1; then
                echo "codescene.sh: base ref not found: $BASE_REF" >&2
                exit 2
            fi
            git diff --name-only "$BASE_REF"...HEAD -- '*.rs'
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
}

review_file() {
    _file="$1"
    _json=$("$CS_CMD" review --output-format json "$_file" 2>/dev/null) || {
        echo "codescene.sh: cs review failed for $_file" >&2
        return 1
    }

    set +e
    _result=$(printf '%s' "$_json" | python3 -c '
import json, sys

data = json.load(sys.stdin)
score = data.get("score")
if score is None:
    print("ineligible")
    sys.exit(0)
value = float(score)
print(f"{value:g}")
sys.exit(0 if value >= 10.0 else 1)
')
    _status=$?
    set -e

    if [ "$_status" -eq 0 ] && [ "$_result" = "ineligible" ]; then
        echo "  $_file: ineligible (pass)"
        return 0
    fi

    if [ "$_status" -eq 0 ]; then
        echo "  $_file: score $_result"
        return 0
    fi

    echo "  $_file: score $_result"
    return 1
}

if [ $# -lt 1 ]; then
    usage >&2
    exit 2
fi

case "$1" in
    --all)
        MODE="all"
        shift
        ;;
    --staged)
        MODE="staged"
        shift
        ;;
    --commit)
        MODE="commit"
        shift
        if [ $# -gt 0 ] && [ "${1#-}" = "$1" ]; then
            COMMIT_REF="$1"
            shift
        fi
        ;;
    --base)
        MODE="base"
        BASE_REF="${2:?--base requires a ref}"
        shift 2
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac

if [ $# -gt 0 ]; then
    echo "codescene.sh: unexpected arguments: $*" >&2
    exit 2
fi

ensure_cs

_file_count=0
_failed=0
echo "codescene.sh: reviewing Rust files ($MODE)"
while IFS= read -r _f; do
    [ -n "$_f" ] || continue
    [ -f "$_f" ] || continue
    _file_count=$((_file_count + 1))
    if ! review_file "$_f"; then
        _failed=1
    fi
done <<EOF
$(list_files)
EOF

if [ "$_file_count" -eq 0 ]; then
    echo "codescene.sh: no Rust files to check"
    exit 0
fi

if [ "$_failed" -ne 0 ]; then
    echo "codescene.sh: one or more files scored below 10" >&2
    exit 1
fi

echo "codescene.sh: all eligible files passed"
