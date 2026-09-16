# Stable releases

Stable publication is **not live yet**. The `stable` branch, stable GitHub releases, and the Homebrew formula do not exist until the owner authorizes the first push. This guide describes the prepared workflow and operator steps; it does not claim any stable asset is published.

## Operator sequence

1. **Freeze the candidate.** Choose the exact revision to release. The Cargo version in `Cargo.toml` determines the stable tag (`v<Cargo version>`). Record base, candidate SHA, and tree id. The first authorized stable push should target a revision whose source is already the most recent successfully published nightly on `main`, with P4 review fixes applied and verified on that candidate.
2. **Pre-push checks (required, external to the workflow).** The managing agent runs `just verify-full --all`, `cargo deny --locked check advisories`, and the supervised attack search in [SECURITY.md](../../SECURITY.md) on that exact revision. Record findings, dispositions, advisory results, search time, and owner authorization. Unresolved blockers prevent the push.
3. **Create and push `stable` once authorized.** Only the managing agent pushes to `mdml/ask` after owner authorization. Do not create the branch, tags, or releases before authorization.
4. **Workflow publication.** A push to protected `stable` runs [.github/workflows/stable-release.yml](../../.github/workflows/stable-release.yml). It checks out the immutable push SHA, refuses an existing tag, runs the full gate and advisory check, builds and tests four native production targets with Rust 1.97.1 and the locked lockfile, packages and attests archives, verifies a draft release's asset inventory and digests, then publishes a non-prerelease release. Workflow repetition does not replace the pre-push checks.
5. **Homebrew (manual, no cross-repo credential).** After publication, check out the published release revision so `Cargo.toml` matches its version, download `SHA256SUMS` from that same release, and generate the formula locally:

```sh
python3 scripts/homebrew-formula.py SHA256SUMS --output ask.rb
```

Review and commit `ask.rb` to the public [mdml/homebrew-tap](https://github.com/mdml/homebrew-tap) repository by hand. As of 2026-09-16 the tap is empty and no stable download URLs exist until step 4 completes.

6. **Install verification.** Install through mise without `prerelease=true`, and through Homebrew after the tap update. Repeat published-archive checksum and attestation checks as for nightlies.

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
