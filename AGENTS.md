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

- Substantial changes are developed on feature branches in isolated git worktrees under `.worktrees/` at the repository root, never directly on `main` or `staging`. Use Conventional Commits.
- Feature PRs target `staging` and are rebase-merged to keep its history linear. `staging` rejects direct pushes, force-pushes, and deletion, and requires the fast gate checks.
- `main` is the nightly channel. It receives only fully gated promotion PRs from `staging`, merged by merge commit after a named proof or milestone passes. `main` rejects direct pushes, force-pushes, and deletion, and requires the full gate check. Stable releases are tags from `main`.
- Delegated work is bounded by a task contract: an explicit outcome, scope, constraints, and the verification evidence the worker must return. A delegated worker leaves its changes uncommitted, does not delegate further, and does not settle human-owned questions. The managing agent reviews the diff and reruns verification before committing.
- The repository exposes one verification entrypoint, `just verify` (fast gate) and `just verify-full` (full gate), that works in Claude Code, Codex, Cursor, and ordinary local or CI shells. GitHub Actions runs the same entrypoint rather than defining a second verification process. Every commit passes the fast gate.
- Documentation describes the current state of the repository on `staging` and `main` and ships in the same change as the behavior it describes.

## Public repository hygiene

- Before committing generated evidence or diagnostics, remove credentials, absolute filesystem paths, hostnames, private project names, raw prompts or transcripts, and agent-session URLs.
- Replace necessary machine-specific paths with stable placeholders such as `<repo-root>` and `<cargo-home>`, and document any normalization in the artifact that carries it.
- Do not publish links to private agent sessions unless the owner explicitly requests it.
- Review the complete staged diff for accidental disclosure before committing.

## Handoff

Reload the relevant part of the mental model before reporting. Explain what changed in the mental model's vocabulary, and report any new gap between the mental model and the repository as a candidate change to the mental model, not an edit.
