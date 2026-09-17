# Stable releases

`v0.1.0` is the first stable release, built from accepted nightly source `3c8ce0e`, with release archives and the `mdml/tap/ask` Homebrew formula. This guide gives the reusable operator sequence for later stable releases.

## Operator sequence

1. **Prepare and freeze the candidate.** Increment the semantic version in `Cargo.toml`; each stable release needs a version whose `v<Cargo version>` tag does not exist. Publish, review, and test a nightly containing that version and the intended changes before selecting the stable candidate. The candidate may add reviewed fixes, which must also be verified. Record the base, candidate SHA, and tree id.
2. **Pre-push checks (required, external to the workflow).** The managing agent runs `just verify-full --all`, `cargo deny --locked check advisories`, and the supervised attack search in [SECURITY.md](../../SECURITY.md) on that exact revision. Record findings, dispositions, advisory results, search time, and owner authorization. Unresolved blockers prevent the push.
3. **Push `stable` once authorized.** Only the managing agent pushes the exact candidate to `mdml/ask` after owner authorization. Do not create tags or releases before authorization.
4. **Workflow publication.** A push to protected `stable` runs [.github/workflows/stable-release.yml](../../.github/workflows/stable-release.yml). It checks out the immutable push SHA, refuses an existing tag, runs the full gate and advisory check, builds and tests four native production targets with Rust 1.97.1 and the locked lockfile, packages and attests archives, verifies a draft release's asset inventory and digests, then publishes a non-prerelease release. Workflow repetition does not replace the pre-push checks.
5. **Homebrew (manual, no cross-repo credential).** After publication, check out the published release revision so `Cargo.toml` matches its version, download `SHA256SUMS` from that same release, and generate the formula locally:

```sh
python3 scripts/homebrew-formula.py SHA256SUMS --output ask.rb
```

Review and commit `ask.rb` to the public [mdml/homebrew-tap](https://github.com/mdml/homebrew-tap) repository by hand.

6. **Install verification.** Install through mise without `prerelease=true`, and through Homebrew after the tap update. Run the archive checks below. Record the checksum, attestation, mise installation, and Homebrew installation results with the release.

## Verify a downloaded archive

Download the archive for your platform and `SHA256SUMS` from the same [stable release](https://github.com/mdml/ask/releases/latest). In their directory, set `archive` to the downloaded filename and run:

```sh
awk -v name="$archive" '$2 == name' SHA256SUMS | shasum -a 256 -c -
gh attestation verify "$archive" --repo mdml/ask \
  --signer-workflow mdml/ask/.github/workflows/stable-release.yml
```

## Trust boundary

The stable workflow mirrors [nightly releases](nightly-releases.md): read-only build jobs, separated attestation and publication permissions, immutable SHA checkouts, exact same-run artifact names with digest errors, draft-then-verify-then-publish, and no tag overwrite. Stable preparation reads tag refs only; it never skips publication for an unchanged source. Nightly skip/repair behavior is unchanged.

Stable and nightly differ after a failed attempt. Nightly `prepare` mints a unique tag on each run, so a failed upload can retry on a fresh tag without touching a published release. Stable uses one tag per Cargo version (`v<Cargo version>`). Preparation fails closed when that tag already exists; publication uses `--verify-tag` and never repoints or deletes an existing tag. A failed stable run may therefore leave an unpublished git tag and/or draft release on GitHub. Inspect that state explicitly, recover or delete the leftover tag and draft by hand if appropriate, and only then rerun; never change a tag that already points at a published non-draft release.

The [dated action dependency disposition](../reviews/nightly-actions-2026-09-16.md) expires after **2026-09-29 UTC**; stable preparation fails thereafter pending review. Stable jobs reuse the approved pinned actions from that disposition.

## Local verification

Run the offline helper suites without publishing anything:

```sh
python3 scripts/stable-release-test.py
python3 scripts/homebrew-formula-test.py
```

`just verify-full --all` also runs them through `tests/nightly_packaging.rs`. Local packaging of a native binary follows the nightly guide, using a stable tag and `scripts/stable-release.py` instead of the nightly helper.
