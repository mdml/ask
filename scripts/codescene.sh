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

# Pinned CodeScene CLI. Scores differ between CLI versions (src/lib.rs scored
# 10 on 1.0.52 and 9.68 on 1.0.55), so bumping CS_VERSION is a deliberate,
# reviewed change, like the Rust toolchain pin. CS_BUILD is the build hash for
# that version: curl -fsSL "https://devtools.codescene.io/api/cli/cs?version=$CS_VERSION"
CS_VERSION="1.0.55"
CS_BUILD="370e26a4a5a678155957c0db933efc76019eb0f2"
# Never let the CLI auto-update or nag, for every cs invocation below.
export CS_DISABLE_VERSION_CHECK=1

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

# Print the version of the cs binary at $1, or nothing if it is unreadable.
cs_version_of() {
    CS_DISABLE_VERSION_CHECK=1 "$1" version 2>/dev/null |
        awk 'NR == 1 && $1 == "cs" && $2 == "version" { print $3 }' || true
}

install_cs() {
    case "$(uname -s)" in
        Darwin) _os=macos ;;
        Linux) _os=linux ;;
        *)
            echo "codescene.sh: unsupported OS: $(uname -s)" >&2
            exit 127
            ;;
    esac
    case "$(uname -m)" in
        arm64 | aarch64) _arch=aarch64 ;;
        x86_64 | amd64) _arch=amd64 ;;
        *)
            echo "codescene.sh: unsupported architecture: $(uname -m)" >&2
            exit 127
            ;;
    esac

    _tmp=$(mktemp -d "${TMPDIR:-/tmp}/codescene-cs.XXXXXX")
    trap 'rm -rf "$_tmp"' EXIT HUP INT TERM
    _url="https://downloads.codescene.io/enterprise/cli/cs-$_os-$_arch-$CS_BUILD.zip"
    echo "codescene.sh: installing cs $CS_VERSION ($_os/$_arch) to $CS_INSTALL_DIR" >&2
    curl -fsSL -o "$_tmp/cs.zip" "$_url"
    unzip -q "$_tmp/cs.zip" -d "$_tmp/out"
    chmod 0755 "$_tmp/out/cs"
    _got=$(cs_version_of "$_tmp/out/cs")
    if [ "$_got" != "$CS_VERSION" ]; then
        echo "codescene.sh: downloaded cs is ${_got:-unreadable}, not the pinned $CS_VERSION" >&2
        exit 127
    fi
    mkdir -p "$CS_INSTALL_DIR/.local/bin"
    install -m 0755 "$_tmp/out/cs" "$CS_INSTALL_DIR/.local/bin/cs"
}

ensure_cs() {
    _path_cs=$(command -v cs 2>/dev/null || true)
    if [ -n "$_path_cs" ]; then
        _path_version=$(cs_version_of "$_path_cs")
        if [ "$_path_version" = "$CS_VERSION" ]; then
            CS_CMD="$_path_cs"
            return 0
        fi
        echo "codescene.sh: cs on PATH is ${_path_version:-unreadable}, not the pinned $CS_VERSION; using the pinned install in $CS_INSTALL_DIR" >&2
    fi

    CS_CMD="$CS_INSTALL_DIR/.local/bin/cs"
    if [ -x "$CS_CMD" ] && [ "$(cs_version_of "$CS_CMD")" = "$CS_VERSION" ]; then
        return 0
    fi

    install_cs
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
