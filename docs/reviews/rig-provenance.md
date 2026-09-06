# Rig 0.42.0 provenance review

Review date: 2026-09-04

Status: evidence record for human review; this record neither adopts Rig nor recommends adoption or rejection.

## Scope and method

This review completes the ownership, download, release-provenance, and source-correspondence questions that [`rig-dependency-closure.md`](rig-dependency-closure.md) left unasserted for the prospective, exactly pinned `rig-core` 0.42.0 dependency. It also records limited crate-level trust signals for `ref-cast`, `ref-cast-impl`, `zmij`, and `aws-lc-sys`, with a closer look at the readable publication mechanism for `aws-lc-sys` 0.45.0. All observations were made on 2026-09-04. The evidence is under [`evidence/rig-0.42.0-provenance/`](evidence/rig-0.42.0-provenance/).

The crates.io requests used a descriptive `User-Agent` and read `GET /api/v1/crates/{crate}`, `GET /api/v1/crates/{crate}/owners`, `GET /api/v1/crates/rig-core/0.42.0`, `GET /api/v1/crates/aws-lc-sys/0.45.0`, and `GET /api/v1/crates/rig-core/0.42.0/download`. The saved crate, version, and owner responses are the unmodified API response bodies, except `crates-io-aws-lc-sys.json`, which is normalized as described below. The `recent_downloads` values below retain the API field's name because the saved responses do not define its time window.

The GitHub requests used read-only `gh api` calls for repository contents and trees, Git refs and annotated-tag objects, commits, releases, Actions runs and jobs, and the Rig tag tarball. For Rig, the principal endpoints were `repos/0xPlaygrounds/rig/git/ref/tags/v0.42.0`, `repos/0xPlaygrounds/rig/git/tags/{tag-object-sha}`, `repos/0xPlaygrounds/rig/commits/{commit-sha}`, `repos/0xPlaygrounds/rig/contents/.github/workflows/cd.yaml?ref=v0.42.0`, and `repos/0xPlaygrounds/rig/actions/runs/{run-id}/jobs`. The public release-job log was read through `gh api`; only the contiguous five-line publication excerpt needed for this review was retained. For AWS-LC, the principal endpoints were the repository and workflow contents, the `aws-lc-sys/v0.45.0` tag and commit, the recursive tag tree, associated Actions runs, and the two publication-script contents responses.

The `.crate` and GitHub tag archives were extracted under `target/rig-0.42.0-provenance/`. `sha256sum` measured the crate archive. Sorted manifests of relative `src/` file paths and their SHA-256 values were compared with `diff -u`; the tagged source `Cargo.toml` was compared separately with the packaged `Cargo.toml` and `Cargo.toml.orig`. [`tarball-correspondence.txt`](evidence/rig-0.42.0-provenance/tarball-correspondence.txt) records the results. The downloaded archives and extracted trees were deleted after evidence collection.

Machine-specific absolute filesystem paths in retained evidence were normalized to `<repo-root>` or `<cargo-home>` where present. Four GitHub and crates.io listings are retained as normalized evidence rather than raw responses because the raw bodies embedded unrelated file patches and repository metadata: [`github-aws-lc-sys-commit-v0.45.0.json`](evidence/rig-0.42.0-provenance/github-aws-lc-sys-commit-v0.45.0.json) drops per-file patch bodies, [`github-aws-lc-sys-runs-v0.45.0.json`](evidence/rig-0.42.0-provenance/github-aws-lc-sys-runs-v0.45.0.json) keeps only run identity, event, status, conclusion, head ref, timestamps, and actor logins, [`crates-io-aws-lc-sys.json`](evidence/rig-0.42.0-provenance/crates-io-aws-lc-sys.json) trims each version entry to number, timestamps, downloads, yank state, license, checksum, size, publisher, and trusted-publishing fields, and [`github-aws-lc-sys-tree-v0.45.0.json`](evidence/rig-0.42.0-provenance/github-aws-lc-sys-tree-v0.45.0.json) keeps only path, type, and mode per entry. Each of those files carries a top-level `_normalization` field stating exactly what was removed on 2026-09-04; every field cited in this review is retained unchanged. The retained GitHub workflow contains the public secret identifiers `GITHUB_TOKEN` and `CARGO_REGISTRY_TOKEN`, and the GitHub commit objects contain public cryptographic signatures; no credential values, local hostnames, private project names, prompts, transcripts, or agent-session URLs are retained. No repository source was sent over the network.

## Ownership

Observed: the crates.io [`rig-core` owners response](evidence/rig-0.42.0-provenance/crates-io-rig-core-owners.json) lists one user owner, `cvauclair`, and no team owners. The [`rig-core` 0.42.0 version response](evidence/rig-0.42.0-provenance/crates-io-rig-core-0.42.0.json) has a non-null `published_by` object for `cvauclair`.

Observed: the [`rig-derive` owners response](evidence/rig-0.42.0-provenance/crates-io-rig-derive-owners.json) also lists one user owner, `cvauclair`, and no team owners.

Interpretation: crates.io exposes one account with owner authority over each of the two crates and attributes publication of `rig-core` 0.42.0 to that same account. This is an account and authorization observation, not evidence about how many people can control the account or its credentials.

## Downloads and cadence

Observed: the [`rig-core` crate response](evidence/rig-0.42.0-provenance/crates-io-rig-core.json) reports 2,551,067 total downloads, 1,462,949 `recent_downloads`, and 62 versions. Its first ten version records, in the API's returned order, have these publication dates and times:

| Version | Published at (UTC) |
| --- | --- |
| 0.42.0 | 2026-08-17 18:43:08 |
| 0.41.0 | 2026-07-28 12:04:36 |
| 0.40.0 | 2026-07-11 00:33:35 |
| 0.39.0 | 2026-06-19 06:57:49 |
| 0.38.2 | 2026-06-09 21:51:18 |
| 0.38.1 | 2026-06-02 09:00:12 |
| 0.38.0 | 2026-06-02 05:17:48 |
| 0.37.0 | 2026-05-13 04:28:04 |
| 0.36.0 | 2026-04-28 23:07:52 |
| 0.35.0 | 2026-04-13 22:28:54 |

Interpretation: the ten newest records span about four months. Most gaps between distinct release days are 10 to 22 days, and 0.38.0 and 0.38.1 were published on the same day; this is a relatively frequent release cadence during the observed interval.

## Release provenance

Observed: `rig-core` 0.42.0 was published through GitHub Actions in `0xPlaygrounds/rig`. The successful push-triggered Actions run 32054952351 is named “Build & Release,” targets commit `d5a34986a1ad57f1e9c5984b82f8d7438ffc717e`, and contains a successful “Release-plz” job. The retained [job-log excerpt](evidence/rig-0.42.0-provenance/github-rig-release-job-95465531577-excerpt.log) records `published rig-core 0.42.0` at 2026-08-17 18:43:10 UTC, consistent with crates.io's 18:43:08 version timestamp. The [run](evidence/rig-0.42.0-provenance/github-rig-run-32054952351.json), [jobs](evidence/rig-0.42.0-provenance/github-rig-run-32054952351-jobs.json), and [release](evidence/rig-0.42.0-provenance/github-rig-release-v0.42.0.json) responses preserve the surrounding metadata.

Observed: the [`Build & Release` workflow at `v0.42.0`](evidence/rig-0.42.0-provenance/github-rig-cd-v0.42.0.yaml) runs `MarcoIeni/release-plz-action@v0.5` and supplies `CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}`. Its release job grants `pull-requests: write` and `contents: write`, but not `id-token: write`. The workflow therefore uses a stored GitHub Actions secret as the crates.io token, not crates.io trusted publishing through GitHub OIDC.

Observed: `v0.42.0` is an annotated tag whose [tag object](evidence/rig-0.42.0-provenance/github-rig-tag-v0.42.0.json) points to commit `d5a34986a1ad57f1e9c5984b82f8d7438ffc717e`. GitHub reports the tag object's verification as `verified: false`, reason `unsigned`. GitHub reports the target [commit](evidence/rig-0.42.0-provenance/github-rig-commit-v0.42.0.json) as `verified: true`, reason `valid`, with a PGP signature and a GitHub committer identity.

Interpretation: the public run log directly connects the crate publication to the repository's Actions run rather than merely showing a workflow capable of publishing. The release was authenticated by a stored crates.io token. The source commit has a GitHub-verified signature, while the release tag itself does not carry a verified signature.

## Tarball correspondence

Observed: the downloaded `rig-core` 0.42.0 `.crate` archive has SHA-256 `432d83e0facf16749f91fe729cbffca84437e8062d2f4e92f4f12e903693922d`. This exactly matches the `checksum` in the crates.io 0.42.0 version record.

Observed: both the packaged and tagged-source `src/` trees contain 181 regular files. Their sorted relative-path and SHA-256 manifests are identical, and the raw [`src` diff](evidence/rig-0.42.0-provenance/rig-core-0.42.0-src.diff) is empty. No `src/` file differs, exists only in the tarball, or exists only in the tagged source. The two complete manifests are preserved as [`rig-core-0.42.0-tarball-src-sha256.txt`](evidence/rig-0.42.0-provenance/rig-core-0.42.0-tarball-src-sha256.txt) and [`rig-v0.42.0-source-src-sha256.txt`](evidence/rig-0.42.0-provenance/rig-v0.42.0-source-src-sha256.txt).

Observed: the packaged `Cargo.toml` differs from the tagged source `crates/rig-core/Cargo.toml`; the [`Cargo.toml` diff](evidence/rig-0.42.0-provenance/rig-core-0.42.0-Cargo.toml.diff) shows Cargo's generated normalization, including expansion of workspace-inherited fields and dependencies and explicit enumeration of packaged targets. The tarball also contains `Cargo.toml.orig`, which exists only in the archive and is byte-identical to the tagged source manifest; the corresponding [`Cargo.toml.orig` diff](evidence/rig-0.42.0-provenance/rig-core-0.42.0-Cargo.toml.orig.diff) is empty.

Interpretation: the published Rust source corresponds exactly to the `v0.42.0` tag, and the manifest difference has the form Cargo itself identifies in the packaged file as registry normalization. The original manifest retained by Cargo corresponds exactly to the tag.

## Trust signals for flagged crates

Observed: crates.io reported the following owners and total download counts on 2026-09-04. The owner classification follows each response object's `kind`, including the AWS team object that the current API placed in its `users` array while returning `teams: null`.

| Crate | User owners | Team owners | Total downloads |
| --- | --- | --- | ---: |
| `ref-cast` | `dtolnay` | None | 234,158,221 |
| `ref-cast-impl` | `dtolnay` | None | 234,145,239 |
| `zmij` | `dtolnay` | None | 392,048,223 |
| `aws-lc-sys` | `justsmth`, `skmcgrail`, `crypto-alg` | `github:aws:aws-lc-rs-team` | 213,376,698 |

The crate and owner responses (raw, except the normalized `crates-io-aws-lc-sys.json`) are [`crates-io-ref-cast.json`](evidence/rig-0.42.0-provenance/crates-io-ref-cast.json), [`crates-io-ref-cast-owners.json`](evidence/rig-0.42.0-provenance/crates-io-ref-cast-owners.json), [`crates-io-ref-cast-impl.json`](evidence/rig-0.42.0-provenance/crates-io-ref-cast-impl.json), [`crates-io-ref-cast-impl-owners.json`](evidence/rig-0.42.0-provenance/crates-io-ref-cast-impl-owners.json), [`crates-io-zmij.json`](evidence/rig-0.42.0-provenance/crates-io-zmij.json), [`crates-io-zmij-owners.json`](evidence/rig-0.42.0-provenance/crates-io-zmij-owners.json), [`crates-io-aws-lc-sys.json`](evidence/rig-0.42.0-provenance/crates-io-aws-lc-sys.json), and [`crates-io-aws-lc-sys-owners.json`](evidence/rig-0.42.0-provenance/crates-io-aws-lc-sys-owners.json).

Observed for `aws-lc-sys` 0.45.0: the [crates.io version record](evidence/rig-0.42.0-provenance/crates-io-aws-lc-sys-0.45.0.json) has a non-null `published_by` object for `justsmth`. The [`aws-lc-sys/v0.45.0` tag](evidence/rig-0.42.0-provenance/github-aws-lc-sys-tag-v0.45.0.json) points to commit `7943223c99d909bc399bdf1b856821bb04f1f3c5`, whose message is `Prepare aws-lc-sys v0.45.0 (#1220)`.

Observed for the AWS release mechanism: the [workflow directory listing](evidence/rig-0.42.0-provenance/github-aws-lc-rs-workflows.json) and [recursive tag tree](evidence/rig-0.42.0-provenance/github-aws-lc-sys-tree-v0.45.0.json) contain no publication or release workflow. The [Actions runs associated with the tag commit](evidence/rig-0.42.0-provenance/github-aws-lc-sys-runs-v0.45.0.json) are tests, analysis, documentation, and Dependabot activity, with no crate-publish run. The tag tree instead contains `scripts/publish/publish-aws-lc-sys.sh`; its [contents response](evidence/rig-0.42.0-provenance/github-aws-lc-sys-publish-script-v0.45.0.json) shows that it calls helpers from `_publish_tools.sh`, whose [contents response](evidence/rig-0.42.0-provenance/github-aws-lc-sys-publish-tools-v0.45.0.json) shows `cargo publish --allow-dirty` after a dry run. Neither script specifies a token, an OIDC exchange, or another Cargo credential provider.

Interpretation: the readable repository state supports a manual-script release mechanism for `aws-lc-sys` 0.45.0, not a named GitHub Actions publication workflow. It does not establish whether the human publisher's Cargo client obtained a stored crates.io token or another credential mechanism. There is therefore no readable basis to classify this release as trusted publishing or token publishing more narrowly than the observed manual `cargo publish` command.

Interpretation of the flagged-crate signals: the counts establish substantial crates.io usage, and the owner records establish the accounts with current owner authority. Neither signal establishes source quality, owner-account security, or the mechanism used for historical releases.

## Open questions for the human

- Does `ask`'s direct-dependency provenance threshold accept a GitHub-verified commit when its release tag is unsigned and crates.io publication uses a stored Actions secret rather than trusted publishing?
- Is the unreadable credential source for the manual `aws-lc-sys` publication process material enough to require evidence from the upstream maintainers, or is recording the limit of public evidence sufficient for this transitive dependency?
- The crates.io API response does not define the period represented by `recent_downloads`; should a future review be authorized to consult crates.io documentation if that period matters to a decision?
