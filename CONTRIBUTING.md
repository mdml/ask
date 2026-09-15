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

Feature work happens on branches in isolated git worktrees under `<repo-root>/.worktrees/`. Each milestone has one umbrella branch, `milestone/<name>`, created from `main`. Open feature pull requests into the umbrella; they are rebase-merged so the umbrella stays linear. Umbrella branches are never rebased, merged into, or force-pushed to catch up with `main`; do not use a promotion PR’s branch-update merge or rebase actions on the umbrella. Promotion to protected `main` happens through a fully gated pull request from the umbrella (or the conflict-resolution promotion branch described below), merged by merge commit by the managing agent after the milestone's named proofs pass, without per-PR owner approval.

Branch protection lives in GitHub rulesets. The `main` ruleset is active: it blocks direct pushes, force-pushes, and deletion, allows only merge-commit pull requests, requires `verify-full` and the four supported-target checks, and leaves “Require branches to be up to date before merging” disabled so a promotion merge lands without touching the umbrella. The applied umbrella ruleset in `docs/development/rulesets/milestone.json` blocks force-pushes, requires rebase-only pull requests, and requires the same checks on an up-to-date branch. The managing agent reviews and applies rulesets under the owner's standing authorization; `docs/development/rulesets/README.md` distinguishes proposed settings from recorded applied settings.

Promotion to `stable` requires the owner's authorization for each release; the managing agent performs the push once authorized. The `stable` branch is not created until the first authorized release. Its applied deletion-block ruleset is in `docs/development/rulesets/stable.json`, and `SECURITY.md` lists the checks a push to `stable` must pass. The release workflow does not exist yet.

### Promotion candidate

Verification and the three independent reviews happen before the promotion PR exists, on a candidate the managing agent builds locally:

1. Fetch, then record immutable commit IDs for the base (`git rev-parse origin/main`) and source (`git rev-parse origin/milestone/<name>`). Use those IDs in subsequent commands, not moving branch refs.
2. Build the combined revision in a detached worktree: `git worktree add --detach .worktrees/promote-<name>-candidate <base>`, then `git merge --no-ff <source>` inside it. This worktree is never pushed; the umbrella is untouched. If this merge conflicts, follow the conflict route below.
3. Record the combined revision and tree (`git rev-parse HEAD HEAD^{tree}`). In that worktree run `direnv exec . just verify-full --base <base>`, the milestone's named proofs, and the three independent reviews in [agent-contracts.md](docs/development/agent-contracts.md). Dispose of every finding before proceeding. A changed candidate requires the full gate and all three reviews again.
4. Open the promotion PR into `main` with the base, umbrella source, PR head (normally the source), combined revision and tree, verification evidence, and review findings, dispositions, and reruns. After opening, fetch `refs/pull/<n>/merge` from origin and compare `git rev-parse FETCH_HEAD^{tree}` with the recorded tree; confirm the merge ref's parents match the recorded base and PR head. Wait for the full gate and all four supported-target checks on this PR candidate.
5. Immediately before merging, fetch again and confirm that `main`, the umbrella, and the PR head still match the recorded IDs and that GitHub's merge tree still matches. The managing agent merges by merge commit after all gates and named proofs pass, without per-PR owner approval. Use `gh pr merge <n> --merge --match-head-commit <pr-head>`; do not enable auto-merge or bypass required checks.

If `main`, the umbrella, or the PR head moves before merge, the candidate is stale: freeze a new candidate, rerun verification and all three reviews, and replace the recorded evidence. Never use GitHub's branch-update merge or rebase actions on the umbrella. Serialize promotions to `main` during the final comparison and merge; if the resulting merge differs from the recorded candidate, stop release work and investigate.

#### Conflict route

If the local merge conflicts, abort it with `git merge --abort`. Create a disposable branch from the recorded umbrella source in a separate worktree: `git worktree add -b promote/<name> .worktrees/promote-<name> <source>`. There, run `git merge --no-ff <base>`, resolve conflicts, stage the resolution, run the fast gate, and commit the merge with a Conventional Commit message. Do not rebase, force-push, or merge main into the protected umbrella.

Record this resolution commit as the PR head. Recreate the detached candidate from the same base and merge that PR head with `git merge --no-ff <pr-head>`. Complete step 3 on this combined revision, then push only the promotion branch and follow steps 4–5 for its PR into `main`. The PR records both the original umbrella source and the resolution commit, including the conflict-resolution diff. If either frozen branch moves, prepare a fresh promotion branch and candidate; do not rewrite the umbrella or reuse stale review evidence.

## Verification gates

The single verification entrypoint is `scripts/verify.sh`, exposed through `just`:

- `just verify` — fast gate. Every commit must pass this before landing.
- `just verify-full` — full gate. Required for every pull request, including feature PRs into an umbrella branch and promotion PRs into `main`.

The fast gate runs formatting, Clippy (warnings denied), build, documentation (warnings denied), line-coverage threshold (default 90%), and CodeScene on staged Rust files. The full gate adds `cargo deny` policy checks and CodeScene on Rust files changed relative to the base branch (default `origin/main`).

Override the coverage minimum with `ASK_COVERAGE_MIN` and the full-gate base ref with `ASK_VERIFY_BASE` or `--base` when needed. For a feature PR, `just verify-full --base origin/milestone/<name>` limits CodeScene review to the feature's changes; the default `origin/main` base also covers them and is the baseline for umbrella candidates and promotions.

For a promotion, run the full gate on the locally built combined revision as described under Promotion candidate above, and record the base, source, and combined revisions with the evidence.

CI runs the full gate and native builds/tests on macOS and Linux, each on arm64 and x86-64, for PRs into `main` and `milestone/*` branches, comparing against the PR base. The per-commit workflow can be dispatched manually to run the fast gate on a branch.

## Transition from `staging`

Until 2026-09-15 feature PRs targeted `staging` and promotion PRs went from `staging` to `main`. The `staging` ruleset (rebase-only, `verify` plus the four target checks, up to date required) remains active and the branch is untouched. The first umbrella, `milestone/dev-environment`, was created from `main` on 2026-09-15; its ruleset was applied and read back on the same date. The per-commit workflow still runs on PRs into `staging` while existing pull requests are retargeted or closed; no new work lands on `staging`. Remaining steps, owned by the managing agent:

1. Retarget or close the open Dependabot PRs against `staging`. Verify replacement updates against `main` after this configuration reaches `main`; recreate any still-needed update on a feature branch if necessary.
2. Review the remaining `staging` diff against `main`; carry forward any still-needed changes through a feature PR into the umbrella, and close superseded work.
3. Remove the `staging` trigger from `.github/workflows/per-commit.yml` and retire the `staging` ruleset.

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

Dependency updates arrive as Dependabot pull requests, for both version and security updates, against the default branch, `main`, where the full gate runs; nightly advisory checks only report. Unattended bot integration and GitHub auto-merge are never enabled; the managing agent explicitly merges reviewed, fully gated updates. The managing agent integrates each update after review and the quarantine and exception rules in `SECURITY.md`: retarget it to the active umbrella with `gh pr edit <n> --base milestone/<name>` and rebase-merge it there. If Dependabot cannot rebase onto the umbrella, recreate the update on a feature branch. Dependabot PRs never merge directly into `main`.

## Questions

If a change would conflict with the human-owned mental model or freeze a product decision not yet represented there, stop and ask before proceeding. Repository documentation describes what exists now; it does not duplicate the mental model.
