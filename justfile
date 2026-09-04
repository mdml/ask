# Verification and development recipes for `ask`.
#
# Run `mise install` once to pin the toolchain and helper binaries.

default:
    @just --list

verify:
    scripts/verify.sh

verify-full *args:
    scripts/verify.sh --full {{ args }}

fmt:
    cargo fmt --all

nightly-deps:
    cargo deny --locked check advisories
    cargo update --dry-run
