# Nightly releases

The [nightly release workflow](../../.github/workflows/nightly-release.yml) builds installable prereleases from protected `main` at 06:37 UTC daily or through **Run workflow** with `main` selected. Other refs and repositories are skipped. Dependency reporting remains in the separate `nightly` workflow.

Each run checks out the event's immutable commit SHA everywhere, runs the full gate (`scripts/verify.sh --full --all`), then builds and tests the production release profile natively on macOS and Linux, each on arm64 and x86-64, with Rust 1.97.1 and the unchanged lockfile. Moving `main` during a run does not change its selected revision. No revision input, version stamping, or application command is added.

Tags are `v<Cargo version>-nightly.<UTC YYYYMMDD>.<run ID>.<attempt>`. Each release contains four `ask-<Rust target>.tar.gz` archives and `SHA256SUMS`. An archive contains an executable `ask`, `LICENSE`, and `manifest.json`: the Cargo version, tag, source SHA, lockfile SHA-256, Rust version, target, stripped binary SHA-256, and production SQLite linkage evidence. Linux packages use the Ubuntu 24.04 GNU runtime baseline; macOS packages build on macOS 15. Older operating systems are not validated by this workflow.

## Install and inspect

Choose an exact tag from [GitHub Releases](https://github.com/mdml/ask/releases) and substitute it for `TAG`:

```sh
mise use -g 'github:mdml/ask[prerelease=true]@TAG'
```

The [mise GitHub backend](https://mise.jdx.dev/dev-tools/backends/github.html) selects the native archive. Explicit tags make installations reproducible; no rolling tag is maintained. Before manual extraction, download the selected archive and `SHA256SUMS` into an empty directory, verify its matching checksum line, and verify the GitHub attestation:

```sh
# Example target; use the target matching your machine.
archive=ask-x86_64-unknown-linux-gnu.tar.gz
# SHA256SUMS lists all four targets; select only the downloaded archive.
awk -v name="$archive" '$2 == name' SHA256SUMS | shasum -a 256 -c -
gh attestation verify "$archive" --repo mdml/ask \
  --signer-workflow mdml/ask/.github/workflows/nightly-release.yml
```

The manifest identifies the binary without invoking it. Attestations cover all four archives and the checksum list. A checksum alone detects corruption; it does not authenticate the publisher.

## Trust and failure behavior

Build jobs have read-only repository permissions. A separate job receives `id-token: write` and `attestations: write`; only publication receives `contents: write`. All checkouts disable credential persistence. Artifact downloads use four exact same-run names with digest mismatches configured as errors. Each privileged job validates target inventory, archive checksums, bounded regular archive members, binary checksums, and manifest identity. Neither executes downloaded files. Uploads use fixed paths; no registry push or deploy key is used.

Publication creates a new tag with `GITHUB_TOKEN`, uploads a draft prerelease, and checks GitHub's asset inventory, sizes, and SHA-256 digests before making it public. Existing tags fail rather than being replaced. A failed publication can leave a tag or draft; inspect it and start a new complete workflow run. Avoid retrying only failed jobs: same-run artifact names are immutable and a complete retry must regenerate the attempt's tag. No repository settings or tag protections are configured by this workflow.

The [dated action dependency disposition](../reviews/nightly-actions-2026-09-15.md) expires after 2026-09-29 UTC; preparation fails thereafter pending review. The existing SQLite readiness workflow, probe workspace, source-equivalence scripts, and differential probe remain until **all** [adoption retirement criteria](../reviews/sqlite-adoption-2026-09-10.md#proposed-native-monitoring-and-verification-migration) are met. Production identity and release-linkage checks alone do not satisfy those criteria.

## Local verification

`just verify-full --all` runs the repository gate with its pinned Rust toolchain, including the packaging safety tests via `tests/nightly_packaging.rs`. The helper suite can also run directly with `python3 scripts/nightly-release-test.py`. For a native release package:

```sh
export RUSTUP_TOOLCHAIN=1.97.1
export RELEASE_SHA="$(git rev-parse HEAD)"
# Use the current Cargo version and a valid nightly tag for local evidence.
export RELEASE_TAG=v0.1.0-nightly.20260915.123.1
target=x86_64-unknown-linux-gnu
cargo +1.97.1 test --release --locked --target "$target"
cargo +1.97.1 build --release --locked --bin ask --target "$target"
python3 scripts/nightly-release.py package --target "$target" --destination package
```

Packaging requires a new destination directory, checks a defined SQLite symbol and absence of dynamic SQLite linkage before stripping a copy, and validates the resulting archive. Local packaging does not run the full gate or publish anything. Hosted four-target builds, GitHub attestation, and a real mise installation require an actual workflow run.
