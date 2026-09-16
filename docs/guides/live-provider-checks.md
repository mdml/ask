# Live-provider checks

Live-provider checks are opt-in compatibility exercises against real model services. They complement deterministic fake-provider proofs in the verification gate. They produce reviewable reports only; they never merge code, change configuration, or publish releases.

This guide documents repository-local **support only**. Support is **not activated**: there is no schedule, cron job, hosted workflow, or CI wiring in this repository. The managing agent arranges operational scheduling separately on an authorized host with owner-provisioned credentials when the owner enables it.

## Relationship to verification

`just verify` and `just verify-full` run without provider credentials. The helper [`scripts/live-provider-check.py`](../../scripts/live-provider-check.py) is also designed to run offline in tests using synthetic stand-ins for the `ask` binary and credential wrapper. Only a manual operator run on an authorized host invokes the real wrapper and binary against live services.

## Preconditions

A run requires all of the following:

- An absolute path to the built or installed `ask` binary under test.
- The 40-character Git commit the operator asserts that binary matches (`run.source_revision`). The helper compares the current checkout to that revision and skips when only documentation changed since it. Adjacent `manifest.json` from a nightly archive is **mandatory**: `source_sha` must equal `run.source_revision` and `binary_sha256` must match the binary bytes. Runs stop when manifest evidence is missing or mismatched.
- An absolute path to an external credential wrapper executable. The wrapper injects provider keys only into the `ask` process it launches; the helper never stores or logs credential values.
- A configured host identifier (`run.host_id`) that matches `--host` or the `LIVE_CHECK_HOST` environment variable on the machine performing the run.
- Explicit budget availability through `run.budget_available = true` in the check configuration or `--budget-available` on the command line.
- Template `ASK_HOME` directories per target, each containing a validated `ask` configuration whose default profile aims at one supported provider with `max_output_tokens = 128`. The helper copies an immutable snapshot from each template into a private temporary directory for every attempt and target; it never mutates operator configuration or reuses conversation history. **Check-specific policy:** the materialized temporary profile always uses a fixed minimal system prompt (`Answer briefly.`) rather than the operator template's `system_prompt`. The fingerprint still records the template's resolved `system_prompt` so operator prompt changes remain visible to scheduling, but live requests never inherit a long or custom template prompt.
- A clean relevant source tree when manifest evidence is evaluated; uncommitted changes under `Cargo.toml`, `Cargo.lock`, `scripts/live-provider-check.py`, `src/`, or `tests/` block the run.
- Offline credential readiness for every target through the credential wrapper invoking `ask doctor` against the materialized temporary home (no shell, Python, or Cargo wrapper for that preflight).

Missing host, wrapper, binary, manifest, keys (detected when credential environment variables are already exported in the helper process, or when `ask doctor` fails), or budget yields `status: "notrun"` with a sanitized reason. `run.budget_available` must be a strict boolean; truthy non-booleans are rejected at load time. Duplicate `targets[].id` values are rejected. Failures do not retry automatically for the same fingerprint unless the operator passes `--force`, and `--force` never bypasses safety preconditions such as manifest verification, dirty relevant source, or doctor failure. A fixed request cap applies across all targets; each target receives one short query and, only when that query succeeds, one short reply. Query and reply steps require exact answer markers (`live-check-ok` and `live-check-reply-ok` respectively); nonempty stdout alone is insufficient. When `ask` reports token usage on stderr, the helper records separate `query_usage` and `reply_usage` objects; when usage is absent or unparsable, those fields are `null` and do not fail an otherwise successful marker match. The helper holds an exclusive lock from readiness through the attempt and records a durable `started` state before the first provider request; state writes fsync the file and containing directory.

## Configuration

Copy and edit the example below. Paths must be absolute on the operator machine. Do not commit operator-specific paths or host names into the public repository.

```toml
version = 1

[run]
ask_binary = "/absolute/path/to/ask"
source_revision = "0123456789abcdef0123456789abcdef01234567"
credential_wrapper = "/absolute/path/to/credential-wrapper"
host_id = "operator-host"
budget_available = false
state_path = "/absolute/path/to/check.state.json"

[policy]
max_requests = 8
max_answer_chars = 64
query_prompt = "Reply with exactly: live-check-ok"
reply_prompt = "Reply with exactly: live-check-reply-ok"

[[targets]]
id = "openai"
ask_home = "/absolute/path/to/ask-home-openai"

[[targets]]
id = "anthropic"
ask_home = "/absolute/path/to/ask-home-anthropic"

[[targets]]
id = "gemini"
ask_home = "/absolute/path/to/ask-home-gemini"

[[targets]]
id = "openrouter"
ask_home = "/absolute/path/to/ask-home-openrouter"
```

The relevant fingerprint hashes check policy, each target's resolved profile snapshot (`model`, `provider`, `max_output_tokens`, `system_prompt`, and credential variable name only—never secret values), and non-documentation source under `Cargo.toml`, `Cargo.lock`, `scripts/live-provider-check.py`, `src/`, and `tests/`. Documentation-only commits do not change it. The helper writes a `started` attempt record to `state_path` before issuing provider requests so an interrupted, crashed, or failed fingerprint is not retried unless the operator passes `--force`.

## Commands

Print the fingerprint for the current tree and configuration:

```sh
python3 scripts/live-provider-check.py fingerprint --config /absolute/path/to/check.toml
```

Run checks when readiness preconditions pass:

```sh
export LIVE_CHECK_HOST=operator-host
python3 scripts/live-provider-check.py run \
  --config /absolute/path/to/check.toml \
  --budget-available
```

Successful runs emit compact JSON on stdout: per-target `pass` / `fail` / `skipped`, aggregate `request_count`, separate `query_usage` and `reply_usage` (or `null` when unknown), and `binary_source.verified` when manifest evidence matches. Prompts, stderr, and raw provider errors are not included. `request_count` counts attempted provider invocations (query and reply steps); a timeout or subprocess failure increments the count when the invocation was attempted but does not establish billing usage—`query_usage` / `reply_usage` stay `null` and the helper records a sanitized failure reason instead of raw diagnostics.

## Local offline tests

The verification gate runs synthetic tests through `tests/live_provider_check.rs`:

```sh
python3 scripts/live-provider-check-test.py
```

These tests mock the wrapper and `ask` binary, assert fingerprint and retry policy, enforce the request cap, confirm subprocess invocation never uses a shell, reject duplicate target IDs and non-boolean budgets, block dirty relevant source, require exact answer markers, report separate query/reply usage when present and `null` when absent, and verify TOML snapshot escaping for materialized temporary homes.
