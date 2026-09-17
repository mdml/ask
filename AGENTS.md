# Agent instructions

These instructions apply to every coding agent working in this repository (`claude`, `codex`, `cursor-agent`) and to human contributors using them. This file is canonical. `CLAUDE.md` imports it, and the `.cursor/` and `.codex/` directories point here; do not duplicate content between them.

## The mental model

`ask` has a human-owned mental model that lives outside this repository. It states product intent, the decisions already made, and the hypotheses under test. This repository describes the implementation that exists now; it does not duplicate the mental model's product content.

- It reaches you as a skill named `ask-mental-model`, loaded from the owner's user scope. Invoke it and read the model in full before any consequential product, public-interface, architecture, or process decision: commands, configuration, profiles, provider support, storage, tools, MCP, verification, and releases.
- If the `ask-mental-model` skill is absent from your session, stop and ask for the mental model before making any such decision. Do not reconstruct it from this repository or proceed on inference.
- Never edit the mental model. Propose changes in your handoff instead.

## Assumption boundary

Assume the owner knows only what is in the current session and the mental model. Repository documents are authoritative about the implemented system, but their contents are not part of the owner's working memory. When a task needs a concept that is in neither place, name it before proceeding.

## When to stop

Return the decision to the owner when:

- The implementation would conflict with the mental model.
- The task would freeze a product, public-interface, architecture, or process decision the mental model does not represent.
- A sequence of locally reasonable changes is moving the project's main ideas.
- You are inferring product intent from implementation.
- The mental model appears to have omitted something unintentionally.

Ordinary implementation choices belong to the repository. Decide those, document them here, and move on.

## Process

- Substantial changes are developed on feature branches in isolated git worktrees under `.worktrees/` at the repository root, never directly on `main`, an umbrella branch, or `stable`. Use Conventional Commits.
- Each milestone has one umbrella branch named `milestone/<name>`, created from `main`. Feature PRs target the umbrella and are rebase-merged to keep its history linear. The applied umbrella ruleset `23480411` in `docs/development/rulesets/milestone.json` rejects force-pushes, allows only rebase merges, and requires the full gate: `verify-full` plus the four supported-target checks, on a branch that is up to date with the umbrella. The managing agent reviews live settings before applying future ruleset changes under the owner's standing authorization to manage branch rules; `docs/development/rulesets/README.md` records their status.
- `main` is the nightly channel. It receives only promotion PRs from an umbrella branch (or its conflict-resolution promotion branch), merged by merge commit by the managing agent after the milestone's named proofs pass, without per-PR owner approval. `main` rejects direct pushes, force-pushes, and deletion, and requires `verify-full` plus the four supported-target checks. Its up-to-date ancestry requirement is deliberately disabled: promotion merge commits belong on `main`, and umbrella branches are never rebased or force-pushed to catch up with `main`.
- Before opening a promotion PR, the managing agent freezes the candidate: it records the base (the `origin/main` commit) and the source (the umbrella tip), then builds the combined revision locally by merging the frozen source into the frozen main base in a detached worktree that is never pushed. The umbrella is never rebased, merged into, or force-pushed to catch up with `main`. `just verify-full --base <base-commit>` and three independent reviews, for security, code quality, and stale documentation, run on that combined revision using the contracts in `docs/development/agent-contracts.md`. Findings, their disposition, and any review reruns are recorded in the promotion PR.
- The promotion PR description records the base, source, and combined revisions and the combined tree id; GitHub's merge ref for the PR must reproduce that tree. If `main`, the umbrella, or a conflict-resolution PR head moves before merge, the candidate is stale: freeze a new one, rerun verification and reviews, and re-record. If the umbrella no longer merges cleanly into `main`, resolve conflicts on a disposable `promote/<name>` branch from the frozen umbrella tip, merge the frozen main base into that branch, and prepare and review its combined candidate before opening the promotion PR. Never rewrite the protected umbrella. `CONTRIBUTING.md` gives the commands.
- Promotion to `stable` requires the owner's authorization for each release; the managing agent performs it once authorized. `v0.1.0` is the first stable release. `docs/development/rulesets/stable.json` records deletion protection, and `SECURITY.md` and `docs/guides/stable-releases.md` list the checks and operator steps.
- Dependabot PRs open against `main`. The managing agent retargets each one to the active umbrella and merges it there after review and the dependency quarantine in `SECURITY.md`. Unattended auto-merge is never enabled, and Dependabot PRs never merge directly into `main`.
- Delegated work is bounded by a task contract: an explicit outcome, scope, constraints, and the verification evidence the worker must return. A delegated worker leaves its changes uncommitted, does not delegate further, and does not settle human-owned questions. The managing agent reviews the diff, reruns verification, and commits and integrates the work.
- The repository exposes one verification entrypoint, `just verify` (fast gate) and `just verify-full` (full gate), that works in Claude Code, Codex, Cursor, and ordinary local or CI shells. GitHub Actions runs the same entrypoint rather than defining a second verification process. Every commit passes the fast gate; every PR, including a feature PR into an umbrella, passes the full gate.
- Documentation describes the current state of the repository on `main` and the active umbrella branches and ships in the same change as the behavior it describes.

## Public repository hygiene

- Before committing generated evidence or diagnostics, remove credentials, absolute filesystem paths, hostnames, private project names, raw prompts or transcripts, and agent-session URLs.
- Replace necessary machine-specific paths with stable placeholders such as `<repo-root>` and `<cargo-home>`, and document any normalization in the artifact that carries it.
- Do not publish links to private agent sessions unless the owner explicitly requests it.
- Review the complete staged diff for accidental disclosure before committing.

## Handoff

Reload the relevant part of the mental model before reporting. Explain what changed in the mental model's vocabulary, and report any new gap between the mental model and the repository as a candidate change to the mental model, not an edit.
