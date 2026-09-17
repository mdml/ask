# Task contracts for delegated work

Reusable contracts the managing agent gives to delegated workers through `sub`, in any supported harness (Claude Code, Codex, Cursor). The managing agent owns decomposition, review, integration, verification, and commits. Contracts are text; paste one into a `sub` launch prompt with the placeholders filled in. Harness binary, model, and permission mode come from the user's `sub` defaults unless a launch overrides them.

## Common terms

Every contract includes these terms. Workers must not:

- delegate further or spawn subagents;
- commit, push, stash, rebase, open pull requests, or change GitHub settings;
- settle product, public-interface, architecture, or process questions owned by the human; stop and report the question instead;
- edit the human-owned mental model;
- record private filesystem paths, hostnames, credentials, raw transcripts, or agent-session links in any file that may be committed.

Every worker returns, in at most the word budget the contract names:

- the worktree path and the revision it started from (`git rev-parse HEAD`);
- the files changed, enumerated with `git status --short`, not from harness metadata;
- tests and checks it personally ran, with the exact commands and their outcomes, listed separately from anything it only read or was told;
- open questions and anything it left undone;
- a final one-sentence verdict or result statement as the last line.

The managing agent reviews the diff, reruns verification, and records revisions, findings, dispositions, and reruns with the change. Normalize worktree paths to `<repo-root>` before publishing evidence.

## Implementation worker

```text
Outcome: <one user-visible or verification-visible result>.
Worktree: <repo-root>/.worktrees/<branch>, branch <branch>, starting at <revision>.
Scope: <files or areas>. Do not change <excluded files or behavior>.
Constraints: common terms above; keep changes scoped; follow AGENTS.md; add a failing test first for bug fixes; no new dependencies without listing them as an open question.
Verification: run `direnv exec . just verify` and any named proofs; report each command and outcome. Do not claim checks you did not run.
Leave all changes uncommitted. Return <= 300 words.
```

## Independent reviewers

Before a promotion PR exists, three reviewers examine the same frozen promotion candidate: the combined revision built locally as described under Promotion candidate in `CONTRIBUTING.md`, checked out in a worktree the reviewer does not modify. Run them as separate `sub` tasks so their findings are independent. Each reviewer reads only; it changes no files. The managing agent disposes of every finding (fixed with a commit reference, accepted with a reason, or deferred with an owner decision), reruns verification and all three reviewers on every revised candidate, and records findings, dispositions, and reruns in the promotion PR description.

### Security reviewer

```text
Outcome: a security review of promotion candidate <revision> in worktree <path>, the merge of main at <base revision> and PR head <head revision>, from milestone/<name> at <source revision> with any promotion-branch conflict resolution included.
Read: `git diff <base revision>..<revision>`, SECURITY.md, deny.toml, Cargo.lock changes, .github/workflows.
Check: credential handling and logging; data sent outside the selected provider endpoint; new or changed dependencies, build scripts, and procedural macros; action pinning; file permissions; input handling at process and network boundaries.
Report each finding as: file:line, severity (blocker/should-fix/note), evidence, and a suggested fix. Read-only; change nothing. Return <= 400 words ending with one sentence: "Verdict: <blockers found | no blockers>".
```

### Code reviewer

```text
Outcome: a correctness and quality review of promotion candidate <revision> in worktree <path>, the merge of main at <base revision> and PR head <head revision>, from milestone/<name> at <source revision> with any promotion-branch conflict resolution included.
Read: `git diff <base revision>..<revision>` and the tests that cover it.
Check: behavior against the documented command contract in docs/reference/ (commands.md and query-behavior.md), README.md, and CONTRIBUTING.md; error paths; concurrency and atomicity of storage writes; test coverage of changed behavior; duplication and unnecessary complexity.
Report each finding as: file:line, severity (blocker/should-fix/note), evidence, and a suggested fix. Read-only; change nothing. Return <= 400 words ending with one sentence: "Verdict: <blockers found | no blockers>".
```

### Stale-documentation reviewer

```text
Outcome: a review of whether documentation on promotion candidate <revision> in worktree <path> describes the implementation on that revision.
Read: README.md, CONTRIBUTING.md, AGENTS.md, SECURITY.md, docs/ (excluding dated review records), and the command parser. Do not assume `--help` exists: invoke only confirmed offline inspection paths, with isolated application state and no provider credentials.
Check: commands, flags, configuration fields, file locations, gate descriptions, branch names, and workflow triggers against the code and .github/workflows. Do not treat dated review records as stale.
Report each stale claim as: file:line, the claim, what the code does instead. Read-only; change nothing. Return <= 400 words ending with one sentence: "Verdict: <stale claims found | documentation current>".
```
