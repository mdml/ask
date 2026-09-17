# Verification and development recipes for `ask`.
#
# Run `mise install` once to pin the toolchain and helper binaries.

default:
    @just --list

verify:
    scripts/verify.sh

verify-full *args:
    scripts/verify.sh --full {{ args }}

# Toolchain regression (also included in both gates).
verify-toolchain-test:
    scripts/verify-toolchain-test.sh

# Run the offline owner walkthrough against an already-built executable.
acceptance binary:
    python3 scripts/offline-acceptance.py {{ quote(binary) }}

fmt:
    cargo fmt --all

nightly-deps:
    cargo deny --locked check advisories
    cargo update --dry-run
    python3 scripts/sqlite-monitor.py
