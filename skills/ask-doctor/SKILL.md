---
name: ask-doctor
description: Help install, configure, and troubleshoot the ask terminal lookup tool (github.com/mdml/ask), including provider/profile setup, credential availability, and offline diagnostics.
---

# Set up and diagnose `ask`

Help the user reach a working installation. Preserve their chosen provider, credential manager, and existing configuration. This skill supplies product knowledge; the user's task and existing authorization determine what you may change.

## Inspect the installed version

Start with offline inspection:

```sh
command -v ask
ask version
ask help
ask doctor
```

`ask version` currently prints a package version, not the nightly tag. For mise installations, inspect `mise ls github:mdml/ask` to identify the active release. Match documentation to that tag or source revision at `https://github.com/mdml/ask/tree/<TAG>`. Do not assume `main` describes the installed version. If the precise revision cannot be established, state that limit and corroborate commands with installed help. Older releases keep reference material in their README; newer ones link to `docs/reference/` and task guides.

Read only the relevant version's documentation: installation, configuration, providers, or troubleshooting. Help is a command summary, not an exhaustive specification. Investigate disagreements rather than silently preferring one source.

## Install or upgrade

Choose an exact release compatible with the user's platform from <https://github.com/mdml/ask/releases>. The supported platforms are macOS and Linux, arm64 and x86-64. Nightlies are prereleases. For an authorized mise installation:

```sh
mise use -g 'github:mdml/ask[prerelease=true]@TAG'
```

Explain the selected tag and global configuration change. Use existing authorization; ask only when the desired release or scope is unresolved. Follow the selected release's installation guide for other installation methods, archive checksums, and attestations. Do not present a stable channel or package as available without checking. Finish with offline inspection.

## Configure without losing existing settings

`ask init` creates a first configuration interactively and refuses to overwrite one. For agent-managed setup or updates, use a complete TOML candidate:

1. Locate the configuration through `ask doctor`. Start from the existing document when present, preserving unrelated profiles, providers, and comments. Configuration is intended to contain credential-variable names, never key values; do not echo sensitive contents encountered in a user-edited file.
2. Make the requested change in a scratch copy. Read the installed version's configuration reference for fields and provider conventions. Model identifiers are free text; use the user's choice or resolve that choice with them.
3. Run `ask configure check CANDIDATE`. Correct validation errors before applying.
4. Review the diff for scope and secret values. If the change is already authorized, run `ask configure apply CANDIDATE`; otherwise present the concrete diff for approval.
5. Run offline `ask doctor` and report what remains unverified.

`ASK_HOME` redirects configuration, data, and cache into one directory; use a temporary directory for rehearsals. Replies retain the profile captured when the thread began. A changed profile applies to new threads.

## Diagnose credentials and health

Offline `ask doctor` contacts no provider and does not modify state. Current exit statuses are 0 for successful checks, 1 for configuration problems, 2 for usage errors, and 3 for environmental readiness problems or incomplete storage checks. Read the actual diagnostics rather than inferring everything from the code.

“Last observed healthy” is historical evidence, not a live health check. A missing key may reflect different environments in the agent and user's terminal. Ask the user to run diagnostics through their usual credential launcher when necessary.

Never request keys in chat, print environment values, or put secrets into `ask` configuration or shell history. Use the selected release's `docs/guides/credentials.md` for user-operated hidden prompts or process-scoped credential-manager injection. Existing credential-manager workflows remain external to `ask`.

Every query and `ask doctor --live` sends a provider request and may incur cost. `--live --all` checks all configured targets. Before a live check, establish authorization for its target, purpose, and request count; existing explicit authorization can satisfy this. Otherwise offer the command or ask. Do not retry paid failures automatically. Live checks record statistics and health, and can create or migrate the store.

Authentication, model availability, and endpoint connectivity remain unverified after offline checks alone. Inspect redacted errors and version-matched docs to distinguish those failures; do not assume every timeout has the same cause.

## Handoff

Briefly report the installed version/revision, changes made, personally verified offline and live results separately, and remaining steps. If no live request was made, say so instead of claiming the provider works.
