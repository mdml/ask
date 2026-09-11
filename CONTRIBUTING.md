# Contributing to `ask`

Thank you for helping build `ask`. This document describes the repository workflow and verification gates that apply to every change.

## Tooling

Install Rust 1.97.1 with [rustup](https://rustup.rs/) or rely on the pinned toolchain in `rust-toolchain.toml`.

Clean builds require a C toolchain and CMake because Rig's Rustls path builds the bundled AWS-LC C and assembly sources and `libsqlite3-sys` compiles the bundled SQLite amalgamation.

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

The real-binary configuration and query proofs also require `python3` (Python 3 standard library only). Install it through the platform package manager if it is absent. The helpers refuse `PYTHONOPTIMIZE` settings that disable assertions; run the proofs with assertions enabled.

## Branch flow

Development follows the process in `AGENTS.md`:

Feature work happens on branches in isolated git worktrees under `<repo-root>/.worktrees/`. Open pull requests into protected `staging`. Promotion to protected `main` happens through a reviewed, fully gated pull request when a named proof or milestone passes. Direct pushes, force-pushes, and branch deletion are blocked on `staging` and `main`. Feature PRs use rebase merges into `staging`; promotion PRs use merge commits into `main`.

GitHub requires `verify-full` and all four supported-target checks for `main`, with “Require branches to be up to date before merging” disabled. This allows promotion without rewriting protected `staging` or adding merge commits to its linear history. Do not use the promotion PR’s branch-update merge or rebase actions on `staging`. The combined promotion revision must pass verification against the current `main`; refresh that revision and its checks if either branch moves before owner merge.

## Verification gates

The single verification entrypoint is `scripts/verify.sh`, exposed through `just`:

- `just verify` — fast gate. Every commit must pass this before landing.
- `just verify-full` — full gate. Required before merge to `staging` or `main`.

The fast gate runs formatting, Clippy (warnings denied), build, documentation (warnings denied), line-coverage threshold (default 90%), and CodeScene on staged Rust files. The full gate adds `cargo deny` policy checks and CodeScene on Rust files changed relative to the base branch (default `origin/staging`).

Override the coverage minimum with `ASK_COVERAGE_MIN` and the full-gate base ref with `ASK_VERIFY_BASE` when needed.

For a promotion, fetch the current branches and check out the PR’s combined merge revision in an isolated worktree. Run `just verify-full --base origin/main` through the repository’s direnv wiring; the default `origin/staging` base would omit the promoted changes from CodeScene review. Record the source, base, and combined revisions with the verification evidence.

CI runs the full gate and native builds/tests on macOS and Linux, each on arm64 and x86-64, for PRs into `staging` and `main`. Promotion CI uses `origin/main` as its comparison base. The per-commit workflow can also be dispatched manually to run the fast gate on a branch.

## Proofs

Run the end-to-end proofs through the built binary with:

```sh
cargo test --test configure_proof
cargo test --test query_proof
cargo test --test continue_proof
```

The configure proof drives `ask init` with redirected stdin, validates and applies candidate files and stdin, and queries a fake provider using the installed configuration. Sentinel cases verify that validation diagnostics never echo candidate-controlled text. Python standard-library helpers in `tests/support/` use a PTY to prove terminal usage errors, FIFO rendezvous to prove destination changes during candidate reading and concurrent writer exclusion, and the flushed initialization confirmation prompt to prove interaction with `apply`. They also exercise invalid UTF-8 from files and stdin. On Linux, a `ptrace` helper pauses the real binary at the final link/rename syscall and temporarily denies directory writes, proving creation and replacement failures preserve the destination and clean up temporary files. The same syscall boundary proves that creation never clobbers a destination appearing after the snapshot check. This requires permission to trace a child process; it is not a production test hook. File-size limits separately exercise temporary-write failures. The query proof exercises response, streaming, error, and timeout behavior, plus stdin-only queries and exact instruction/payload composition at the fake-provider boundary. It checks UTF-8 byte preservation, empty and unreadable input, invalid UTF-8, stdout/stderr separation, the final newline, provider and streaming failures with piped input, and quiet early pipe closure. A Python PTY helper proves that terminal prompt words never wait for input and that the multiline prompt uses stderr, accepts multiple lines through EOF, rejects empty submissions without contacting the provider, and, with SIGINT delivered directly as the terminal driver does for Ctrl-C, terminates by that signal with empty stdout and no provider request. Both proofs use deterministic fixtures and a programmable HTTP provider on loopback; they need permission to bind local sockets but require no external network access, provider credentials, or paid services. Shared helpers live in `tests/support/`.

The continue proof runs `ask new` and `ask reply` as separate processes of the real binary against a fake provider that answers successive requests with successive scenarios and records every request. It asserts the exact messages, order, system prompt, and model each reply sends; that replies keep the thread's profile snapshot after the configuration changes, becomes invalid, or is removed; the command aliases; that a reply held at the provider while `ask new` creates another thread still sends and appends to the thread it started with and becomes current when it finishes last; piped reply input; the missing-current-thread error without a request; that a failure before answer text leaves the current thread unchanged; that partial turns are recorded but not replayed and empty answers are complete turns; quiet early pipe closure with a recorded partial turn; and the database location, permissions, and absence of the credential value. A gated fake-provider response holds the answer after the request arrives while the proof makes the data directory and database read-only, proving that a delivered answer that cannot be recorded exits 1 with stdout preserved, a one-line diagnostic, and the database unchanged. That case skips when the current user can write despite the permissions, for example as root.

## Local storage

`src/store.rs` owns the SQLite database. Only `ask new` and `ask reply` open it, with one connection per process and synchronous calls outside the streaming section. Schema version 1 is created in one transaction that sets `PRAGMA user_version = 1`. An unversioned file is accepted only when it is empty, and a newer or unrecognized version is refused before anything is written. Foreign keys are on.

The database uses the rollback journal (`journal_mode = DELETE`), `synchronous = FULL`, and a 1000 ms busy timeout. Each query performs one short write transaction after its answer, so write-ahead logging's concurrent-reader benefit does not matter for `ask`. The rollback journal leaves no persistent `-wal` or `-shm` files next to the database, and with no `-wal` file present SQLite never opens a WAL connection, which keeps a future side-effect-free offline diagnostic away from the read-only WAL recovery path described in the [SQLite adoption record](docs/reviews/sqlite-adoption-2026-09-10.md#offline-diagnostics-and-the-read-only-wal-path). Full synchronization makes each recorded turn durable once the command exits.

Storage unit tests in `src/store_tests.rs` cover paths and permissions, schema creation, version refusal, reopening, turn order, snapshot round trips, rollback of every table on an injected failure, provider-health identity, and text-free statistics. An ignored test measures open cost for first creation and warm reopen; run it locally with `cargo test --release --lib store::tests::startup -- --ignored --nocapture`.

## Dependency updates

Dependency updates are proposed for human review through Dependabot and nightly advisory checks. They are never auto-merged. Dependabot version-update pull requests target `staging`, like other feature changes, so those updates reach `main` only through promotion. Dependabot reads that setting from `main`, so it applies only after it has been promoted. Dependabot security-update pull requests always target the default branch, `main`, and do not follow `target-branch`; apply such an update through a pull request to `staging` rather than merging it into `main` directly.

## Questions

If a change would conflict with the human-owned mental model or freeze a product decision not yet represented there, stop and ask before proceeding. Repository documentation describes what exists now; it does not duplicate the mental model.
