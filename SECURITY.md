# Security policy

## Reporting a vulnerability

Report security vulnerabilities privately through [GitHub private vulnerability reporting](https://github.com/mdml/ask/security/advisories/new) on this repository. Do not open public issues for undisclosed security problems.

## Supported versions

`ask` is pre-alpha. No released version is supported for security updates yet. The first supported stable release will be documented here when it ships.

## Supply-chain posture

The project applies a higher-than-usual supply-chain bar because model-provider dependencies are attractive targets:

- Direct dependencies are pinned exactly; transitive dependencies are locked in `Cargo.lock`.
- Builds use `--locked` to enforce the lockfile.
- `cargo-deny` enforces license, advisory, ban, and source policy (`deny.toml`).
- GitHub Actions workflows pin third-party actions to full commit SHAs.
- Dependency updates are reviewed and merged by the managing agent under the process below; no unattended automation merges them.

When releases exist, release artifacts will carry checksums and GitHub attestations. That machinery is not in place during the pre-alpha bootstrap phase.

## Dependency update process

- **Review and merge.** Every dependency update, whether a Dependabot version or security update or a change made by hand, is reviewed and integrated by the managing agent as described in `CONTRIBUTING.md`. Nightly advisory checks report only; nothing merges unattended.
- **48-hour quarantine.** A new version of a crate or a pinned GitHub Action is not merged until 48 hours after its upstream publication. Publication time is the crates.io version `created_at` for crates and the release or tag date for actions, not the time the update PR opened. The merging PR records each changed version, its upstream publication URL and timestamp, and the earliest permitted merge time. Check transitive updates too; if publication time cannot be established, hold the update or record an expiring exception.
- **Exceptions expire.** Merging before the quarantine elapses, or ignoring an advisory in `deny.toml`, requires a written exception with its reason and an explicit expiry date, recorded in the merging PR or beside the `deny.toml` entry. There are no blanket or open-ended exceptions. An expired exception is a policy failure until it is renewed with a new expiry or removed.
- **Stable releases.** The owner authorizes each stable release; the managing agent performs the push after checking the exact release candidate. A push to `stable` must pass the full gate, an automated advisory check (`cargo deny --locked check advisories`) against the exact lockfile, and an agent-supervised search for reported attacks on the supported-target dependency closure and pinned build/release actions, including reported compromises without a CVE. Record search time, queries, sources, findings and dispositions, advisory results, and the exact candidate revision with the release. Unresolved security blockers prevent the push. For the first stable release, review the complete closure; for subsequent releases, include unchanged dependencies as well as updates. The `stable` branch and its release workflow do not exist yet; the advisory check currently runs nightly on `main`. The applied stable ruleset only blocks deletion; it does not implement the pre-push authorization, full-gate, or security checks. These checks must be in place before the first stable push.
