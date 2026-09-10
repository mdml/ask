# September 9 SQLite readiness evidence

This directory uses the September 9 local assessment label; source retrieval and CI execution occurred on 2026-09-10 UTC. It records the follow-up, separate from the September 6 evidence. The [assessment](../../sqlite-readiness-2026-09-09.md) is the interpretation and owner handoff; these files are dated observations, not adoption approval.

| File | Purpose |
| --- | --- |
| `source-equivalence.json` | Current registry version/checksum results; every package payload and source-only path hashed; source and normalized manifest checks; publisher-lock inventory; official SQLite amalgamation comparison. |
| `publication.json` | Allowlisted GitHub release/tag/signature/workflow observations and unsuccessful crates.io API refresh. |
| `post-bundle-fixes.json` | All 80 returned branch check-in summaries, including endpoints; assessment inference is a separate field. SQLite's published material is public domain. |
| `linux-x86_64.json` | Local isolated release probe output, native options, linkage and hashes. Independent CI results cover all four targets. |
| `ci-native-targets.json` | Four successful native release jobs, tested PR merge commit, branch head, job URLs, and sanitized probe results. |
| `verification.txt` | Task-local checks and shared gate result with applicable limits. |

## Reproduce the runtime evidence

From the repository root, with Rust 1.97.1, rustfmt, Clippy, Python 3.11 or newer, a native C compiler/archiver, `nm`, and either Linux `readelf` or macOS `otool`:

```sh
python3 scripts/sqlite-readiness.py x86_64-unknown-linux-gnu
```

Use the native target triple on the other supported runners. The script requires its target to equal `rustc`'s host. It builds an isolated copy with its dedicated lockfile, not the root application. Cargo may need registry network access on an empty cache. Compiler and SQLite/Rust flag overrides are not passed into child commands; only PATH, home/toolchain/cache locations, temporary-directory location, and SYSTEMROOT are inherited. A fresh working directory prevents repository Cargo configuration inheritance, but Cargo-home configuration and installed toolchains remain trusted build inputs. This is reproducible procedure, not a hermetic or bit-reproducible build claim. Rustfmt/Clippy execute in the same isolated copy before the release build.

stdout contains only result JSON on success. Errors go to stderr and return nonzero. Raw build/native subprocess output is captured in memory and withheld on failure; no environment dump, raw database path/content, or provider credentials are emitted. Database rows are fixed synthetic fixtures. Temporary build/database files are deleted. Library paths are reduced to basenames; hostnames, user names, machine paths and raw compiler logs are not retained. Native compile options are character-checked before emission. Binary and lock hashes identify the tested artifacts without publishing the executable. The parent runner captures the internal process handshake; it is not part of public output.

The dedicated workflow emits the same sanitized JSON in its job log and GitHub step summary. It does not upload raw artifacts or add action pins. The four successful results are retained in `ci-native-targets.json` with the tested PR merge checkout, branch head, and public run/job URLs. The assessment lists runner labels. Normalization extracted only the probe step's JSON, checkout commit, job identity and conclusion from the completed job logs; runner setup, paths, environment and unrelated output were omitted.

## Reproduce source correspondence

Download the following public inputs into an empty temporary directory. Filenames are the comparison script's input contract; do not retain raw upstream archives or raw HTTP diagnostics in the public repository.

| Filename | Primary source URL |
| --- | --- |
| `sqlite-rusqlite-index` | <https://index.crates.io/ru/sq/rusqlite> |
| `sqlite-sys-index` | <https://index.crates.io/li/bs/libsqlite3-sys> |
| `sqlite-rusqlite.crate` | <https://static.crates.io/crates/rusqlite/rusqlite-0.40.2.crate> |
| `sqlite-sys.crate` | <https://static.crates.io/crates/libsqlite3-sys/libsqlite3-sys-0.38.2.crate> |
| `sqlite-source.tar.gz` | <https://codeload.github.com/rusqlite/rusqlite/tar.gz/e88f112bef7899234a497baed5cc3c3d553deeb8> |
| `sqlite-tag.json` | <https://api.github.com/repos/rusqlite/rusqlite/git/refs/tags/v0.40.2> |
| `sqlite-amalgamation.zip` | <https://sqlite.org/2026/sqlite-amalgamation-3530200.zip> |

For example, `curl --fail --silent --show-error --location URL --output FILE` retrieves each input without verbose headers. Then run:

```sh
python3 scripts/sqlite-source-equivalence.py '<input-directory>'
```

`<input-directory>` is a placeholder for the temporary download directory. The script compares archive entries without extracting them, resolves upstream symlink contents, checks registry archive hashes, validates the tag and package VCS records, checks every payload byte, reconstructs parsed manifest normalization, inventories all source omissions and publisher-generated lock identities, and compares three official SQLite amalgamation files, including the C file against SQLite's published 3.53.2 SHA3-256. Both verification scripts use unconditional checks that remain active under Python optimization. The script is specific to this dated candidate; a future run may report a different latest registry version. The observation-date field records the UTC execution date; download fresh inputs for each new assessment and retain a separate dated record for future retrievals. It does not reproduce the publisher's Cargo.lock resolution or claim cryptographic publisher authentication.

Publication endpoints are the [GitHub release](https://api.github.com/repos/rusqlite/rusqlite/releases/tags/v0.40.2), [commit](https://api.github.com/repos/rusqlite/rusqlite/commits/e88f112bef7899234a497baed5cc3c3d553deeb8), and [exact-commit Actions query](https://api.github.com/repos/rusqlite/rusqlite/actions/runs?head_sha=e88f112bef7899234a497baed5cc3c3d553deeb8&per_page=100). The workflow reviewed is `.github/workflows/main.yml` in the downloaded source archive; only its digest and review conclusion are retained. Publication JSON retains public repository identity, release-author handle, timestamps, counts and signature status; it omits email addresses, HTTP headers, raw API payloads and unrelated account metadata. Public upstream URLs are provenance, not private session links.

Fix evidence comes from the [bounded upstream timeline](https://sqlite.org/src/timeline?from=version-3.53.2&to=version-3.53.4&y=ci). Its comment text was extracted from `timelineSimpleComment` elements, whitespace collapsed, and paired with `timelineHash` identifiers. Navigation, scripts, user attribution and unrelated page content were removed. Classifications are manual assessment inferences informed by the candidate build script and runtime options; they are not upstream assertions about `ask`. The response contained 80 entries and the inventory includes each exactly once. It does not claim line-by-line patch review.

## Optimized-verification regression

After downloading the source-comparison inputs, run `python3 scripts/sqlite-source-equivalence-test.py '<input-directory>'`. It first requires valid input to produce identical JSON with no stderr at optimization levels 0, 1 and 2. It then copies the inputs, alters one archive byte, and requires rejection with empty stdout and sanitized stderr at Python optimization levels 0, 1 and 2. The regression failed at level 1 before verification assertions were replaced with unconditional checks, and passed afterward. Valid source comparison also passes under optimization. The scripts do not rely on Python assertions for integrity or native checks.
