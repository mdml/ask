# Security policy

## Reporting a vulnerability

Report security vulnerabilities privately through [GitHub private vulnerability reporting](https://github.com/mdml/ask/security/advisories/new) on this repository. Do not open public issues for undisclosed security problems.

## Supported versions

`ask` is pre-alpha. No released version is supported for security updates yet. The first supported stable release will be documented here when it ships.

## Supply-chain posture

The project applies a higher-than-usual supply-chain bar because model-provider dependencies are attractive targets:

- Direct dependencies will be pinned exactly when introduced; transitive dependencies are locked in `Cargo.lock`.
- Builds use `--locked` to enforce the lockfile.
- `cargo-deny` enforces license, advisory, ban, and source policy (`deny.toml`).
- GitHub Actions workflows pin third-party actions to full commit SHAs.
- Dependency updates are proposed for human review and are never auto-merged.

When releases exist, release artifacts will carry checksums and GitHub attestations. That machinery is not in place during the pre-alpha bootstrap phase.
