# SQLite assessment evidence

Captured 2026-09-06 for the [SQLite dependency assessment](../../sqlite-dependency-assessment.md). These files record an experiment; the manifests and lockfiles are evidence, not production dependencies. Do not invoke Cargo in this directory as part of an application build.

## Contents and normalization

- `closure.json`: baseline commit/lock hash; added and removed packages; licenses, sources, repositories, build-script and proc-macro target flags; per-target reachable additions; active Linux normal/build feature trees; and feature additions to baseline packages. Intersect package target flags with each target's `added_reachable` list before counting executed build dependencies.
- `registry.json`: selected crates.io stable releases, timestamps, features, licenses, repositories, and owner logins/kinds for eight screened packages. Publisher metadata is reduced to its public login. Downloads are crate-wide lifetime values, not release-integrity evidence.
- `provenance.json`: four archive checksums, declared VCS commits, GitHub repository identity/activity, and commit verification status. Unneeded account metadata and signature payloads are omitted.
- `source-checks.json`: tag objects, three file correspondence checks, native source IDs/SHA3 hashes, and RustSec remote HEAD. File correspondence does not establish complete source-tree equivalence.
- Six configuration subdirectories: exact probe manifest, lockfile, and normalized full `deny.txt`. Empty application targets were `src/lib.rs` and `src/main.rs` containing `fn main() {}`. These probes resolved dependencies without compiling the application.
- `native-smoke/`: independent manifest, lockfile, `main.rs`, and successful output. To reconstruct, place `main.rs` at `src/main.rs` in a temporary package. This smaller manifest does not reproduce the full application's feature union.
- `verification.txt`: commands, results, deviations, and limits.

Machine paths are replaced with `<probe-root>`, `<cargo-home>`, or `<repo-root>`. Raw Cargo metadata containing local manifest paths is omitted. Registry and public GitHub URLs remain intact. JSON is selected/derived evidence, not an unmodified API response.

## Reproduction

Use a temporary directory and Cargo home. Copy the manifest and lockfile from baseline `cfa8420e570deee7839c88cd1d05e00c0b9216eb`; its lockfile SHA-256 is recorded in `closure.json`. Create empty application targets as above and append the exact declaration from a preserved probe manifest. Resolve with `cargo metadata`, retaining compatible baseline versions. Do not use a fresh lockfile to claim an incremental comparison if unrelated versions change.

```sh
cargo metadata --format-version 1
cargo tree --locked -e normal,build --target aarch64-apple-darwin --prefix none --format '{p}'
cargo tree --locked -e normal,build --target x86_64-apple-darwin --prefix none --format '{p}'
cargo tree --locked -e normal,build --target aarch64-unknown-linux-gnu --prefix none --format '{p}'
cargo tree --locked -e normal,build --target x86_64-unknown-linux-gnu --prefix none --format '{p}'
cargo tree --locked --offline -e normal,build --target x86_64-unknown-linux-gnu --prefix none --format '{p} features={f}'
cargo deny --locked --config <copied-baseline-deny.toml> check
```

The policy is the baseline's unchanged `deny.toml`, including its four supported targets and all-features setting. The probe root defines no extra features, so this does not activate every feature on the dependency. Strip Cargo's display annotations, deduplicate name/version pairs, and intersect target trees with lockfile additions. Compare active tree features separately; unfiltered metadata can include optional dependencies that do not compile.

Primary requests used crates.io crate, version, and owner APIs; GitHub repository, commit, and tag-ref APIs; raw GitHub files at recorded commits; SQLite release history; and `git ls-remote https://github.com/RustSec/advisory-db HEAD`. HTTP requests used a descriptive assessment user agent. Archive SHA-256 was compared with registry checksums, and native `sqlite3.c` SHA3-256 with official SQLite release hashes.
