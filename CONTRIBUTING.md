# Contributing to `ask`

Thank you for helping build `ask`. This document describes the repository workflow and verification gates that apply to every change.

## Tooling

Install Rust 1.97.1 with [rustup](https://rustup.rs/) or rely on the pinned toolchain in `rust-toolchain.toml`.

Clean builds require a C toolchain and CMake because Rig's Rustls path builds the bundled AWS-LC C and assembly sources.

Install pinned helper tools with [mise](https://mise.jdx.dev/):

```sh
mise install
```

This provides `just`, `cargo-llvm-cov`, and `cargo-deny` at the versions recorded in `mise.toml`.

CodeScene checks require `CS_ACCESS_TOKEN`. Load it through [direnv](https://direnv.net/) from a `.envrc` that is git-ignored locally:

```sh
export CS_ACCESS_TOKEN="$(< "$HOME/.codescene/pat")"
```

Run verification with `direnv exec . just verify` so the token is available without exporting it manually.

## Branch flow

Development follows the process in `AGENTS.md`:

Feature work happens on branches in isolated git worktrees under `<repo-root>/.worktrees/`. Open pull requests into protected `staging`. Promotion to protected `main` happens through a reviewed, fully gated pull request when a named proof or milestone passes. Direct pushes, force-pushes, and branch deletion are blocked on `staging` and `main`.

## Verification gates

The single verification entrypoint is `scripts/verify.sh`, exposed through `just`:

- `just verify` — fast gate. Every commit must pass this before landing.
- `just verify-full` — full gate. Required before merge to `staging` or `main`.

The fast gate runs formatting, Clippy (warnings denied), build, documentation (warnings denied), line-coverage threshold (default 90%), and CodeScene on staged Rust files. The full gate adds `cargo deny` policy checks and CodeScene on Rust files changed relative to the base branch (default `origin/staging`).

Override the coverage minimum with `ASK_COVERAGE_MIN` and the full-gate base ref with `ASK_VERIFY_BASE` when needed.

CI runs the full gate on pull requests into `staging`; the per-commit workflow can also be dispatched manually to run the fast gate on a branch.

## Dependency updates

Dependency updates are proposed for human review through Dependabot and nightly advisory checks. They are never auto-merged.

## Questions

If a change would conflict with the human-owned mental model or freeze a product decision not yet represented there, stop and ask before proceeding. Repository documentation describes what exists now; it does not duplicate the mental model.
