# SQLite adoption record

Assessment date: 2026-09-10 UTC. Scope is resolving which SQLite route `ask` should adopt for its local history, statistics, and provider-health storage, with one concrete recommendation and the narrow owner decision it needs. This record supersedes the applicability inferences in the [September 9 readiness assessment](sqlite-readiness-2026-09-09.md), which remains a historical assessment, and extends the [September 6 dependency assessment](sqlite-dependency-assessment.md). It does not adopt a dependency, change production dependencies, adopt system SQLite, maintain a fork, patch a production dependency, select storage semantics, implement storage, or amend the human-owned mental model. The [evidence guide](evidence/sqlite-adoption-2026-09-10/README.md) supplies reproduction and evidence boundaries.

## Candidate and route versions

The reviewed candidate is unchanged: `rusqlite = { version = "=0.40.2", default-features = false, features = ["bundled"] }`, resolving `libsqlite3-sys` 0.38.2, which bundles SQLite 3.53.2. Fresh registry index reads on 2026-09-10 UTC list rusqlite 0.40.2 and libsqlite3-sys 0.38.2 as the highest non-yanked stable versions. SQLite's latest release is 3.53.4 (2026-07-24), following 3.53.3 (2026-06-26); the candidate contains 3.53.2 (2026-06-03).

A newer crate version number does not imply newer bundled SQLite. rusqlite v0.40.2 was published on 2026-08-08, after SQLite 3.53.4 and after rusqlite's own 3.53.4 bump commit `901f9946` (2026-07-25). That bump is not an ancestor of the v0.40.2 tag commit `e88f112`: the GitHub comparison reports the tag diverged and behind by 28 commits. The v0.40.2 release line forked before the 3.53.4 bump landed on master, so the newest crate release still bundles 3.53.2. rusqlite master (`726c921`, 2026-09-06) carries `libsqlite3-sys/sqlite3/sqlite3.h` with `SQLITE_VERSION` "3.53.4" and source id "2026-07-24 19:02:57 bf7c7f30…", but no crate release contains it.

The exact versions and native versions for each considered route are:

| Route | Crate versions | Native SQLite |
| --- | --- | --- |
| Accept the bundled candidate | rusqlite 0.40.2, libsqlite3-sys 0.38.2 (bundled) | 3.53.2 |
| Wait for a crate release bundling 3.53.4 | unreleased; master has the bump | 3.53.4 target |
| Pin a rusqlite git commit | rusqlite/libsqlite3-sys at master `726c921` | 3.53.4 |
| Non-bundled external-library link | rusqlite 0.40.2, libsqlite3-sys 0.38.2 (no bundled feature) | ask-built 3.53.4 amalgamation |

## Fix ancestry for the named check-ins

Ancestry is established from the SQLite branch timelines for 3.53.2 to 3.53.3 and 3.53.3 to 3.53.4, each check-in's info page, and the change log. Presence in the 3.53 branch timeline interval ending at a release tag means the check-in is an ancestor of that release and of the later 3.53 branch release verified here; containment on other branches and later reversions were not checked. Details and per-release `sqlite3.c` digests are in [fix-ancestry.json](evidence/sqlite-adoption-2026-09-10/fix-ancestry.json).

| Check-in | Concern | In 3.53.3 | In 3.53.4 | In candidate 3.53.2 |
| --- | --- | :--: | :--: | :--: |
| `6826c17021` | Reject index cells extending past the page end during an index search | yes | yes | no |
| `1a888ea60b` | Detect corrupt freelist chains on btree pages | yes | yes | no |
| `9471fa2c9c` | Limit crafted hot-journal arbitrary file deletion during rollback | yes | yes | no |
| `9a79ca31ff` | Handle a corrupt page size in a read-only WAL | yes | yes | no |
| `aa756c8038` | Prevent a super-journal buffer overrun with a large-`mxPathname` VFS | yes | yes | no |
| `bf70dadc2d` | Roll back a hot journal when a crash zeroes the super-journal name and checksum | no | yes | no |

`bf70dadc2d` corrects the September 9 assessment. That record inferred the zeroed-super-journal recovery regression applied to the candidate. It does not. The [upstream forum report](https://sqlite.org/forum/info/2026-07-20T18:27:00Z) places the regression in 3.53.3 and excludes 3.53.2: the 3.53.3 refactor of `readSuperJournal()` introduced a path that returns a non-null pointer to an empty name, while 3.53.2 `pager_playback()` tests `zSuper[0]` and takes the rollback path. The regression was introduced in 3.53.3 and fixed in 3.53.4; the candidate 3.53.2 is not exposed to it. The five other check-ins first shipped in 3.53.3 and are genuinely absent from the candidate.

## Applicability of the absent fixes

The absent fixes separate into damaged-file handling and other conditional surfaces. None of them is a demonstrated healthy-write corruption path. The distinction between a demonstrated effect and possible exploitability is preserved below.

Damaged-file handling is the material gap. `6826c17021` and `1a888ea60b` harden index-page and freelist reads against already damaged database pages. These are reachable with ordinary SQL once a file on disk is damaged, independent of user-supplied SQL shape. The [differential probe](evidence/sqlite-adoption-2026-09-10/differential-probe.json) demonstrates the `6826c17021` effect directly: a database with one text row and one index, on a 512-byte page, with a single index-cell payload-length byte inflated from 4 to 100, returns the row from an indexed `SELECT` under 3.53.2 and returns `SQLITE_CORRUPT` under 3.53.3 and 3.53.4. This establishes a missing rejection on a damaged file. It does not establish memory disclosure, exploitability, or a mechanism by which healthy writes produce the damage. `PRAGMA integrity_check` reports the damage in all three builds, naming the page and cell; the successful integrity check in the readiness probe tested a healthy database only and does not make integrity_check a complete safe parser for arbitrary damaged files.

The remaining four fixes are conditional and not demonstrated here. `9471fa2c9c` is a file-deletion trust boundary that matters only if untrusted files can enter the database directory or be restored or imported as database state; ordinary prompt text bound into SQL does not create the required file structure, and private application-created state reduces the threat substantially without eliminating it for future import or restore. `9a79ca31ff` is conditional on a read-only WAL with an uninitializable shared-memory path and damaged WAL data, not on ordinary writable recovery; whether offline diagnostics must open damaged read-only WAL state is undecided. `aa756c8038` triggers only with a custom VFS whose `mxPathname` exceeds the small-page buffer assumption; the stock Unix VFS uses `mxPathname` 512 and is outside the trigger. `bf70dadc2d` does not apply to the candidate at all, as established above.

The September 9 assessment's broader classification of the full branch interval, including SQL-correctness, FTS, RTREE, JSONB, backup, and URI-parsing changes, still holds: their presence in the amalgamation is not evidence that ordinary bound-text insertion reaches them, and they require a query and feature audit when actual storage SQL is proposed. Nothing in this record selects storage SQL or features.

## Route comparison

Facts for each route are in [route-comparison.json](evidence/sqlite-adoption-2026-09-10/route-comparison.json). Costs below are dependency, distribution, and ongoing maintenance.

**Accept the bundled candidate (rusqlite 0.40.2 / libsqlite3-sys 0.38.2, SQLite 3.53.2).** Dependency cost is lowest: two exactly pinned crates already reviewed for source correspondence, with the bundled amalgamation compiled from crate-internal source. Distribution cost is lowest: the amalgamation compiles as part of the normal crate build on all four targets with no external library. Maintenance cost is a standing native-fix gap: the adopter carries the absent damaged-file hardening (`6826c17021`, `1a888ea60b`) and the conditional fixes until a later crate release, and must accept those residual risks explicitly. Residual risk is the demonstrated index-bounds acceptance on damaged files plus the conditional recovery and file-deletion surfaces above.

**Wait for a crate release bundling 3.53.4.** Dependency and distribution cost are identical to the bundled candidate once such a release exists, and the native gap closes. The cost is schedule uncertainty: no crate release currently bundles 3.53.4, and on 2026-09-10 UTC there is no open milestone, pull request, or issue signaling a planned or imminent release. The last release, v0.40.2, is dated 2026-08-08; master is three commits ahead of it and carries the 3.53.4 bump. A release is plausible but unscheduled, and its timing is outside `ask`'s control.

**Pin a rusqlite git commit (master, SQLite 3.53.4).** This closes the native gap now. Its costs are policy and provenance rather than tracking: `deny.toml` sets `unknown-git = "deny"`, so a git source needs an explicit policy exception approved by the owner; the pinned commit is not a published, checksummed registry artifact, so its provenance must be reviewed directly; the commit carries unreleased rusqlite changes beyond the SQLite bump; and every later move to a newer commit or back to a registry release is a deliberate, reviewed update. A commit pin does not follow the moving branch, and a locked build does not require vendoring. The mental model does not prohibit a reviewed git pin, but its dependency posture makes the exception an owner decision.

**Non-bundled external-library link (ask-built 3.53.4 amalgamation).** libsqlite3-sys 0.38.2 supports linking an external library through `SQLITE3_LIB_DIR` and `SQLITE3_INCLUDE_DIR`, with `SQLITE3_STATIC` selecting static linking. This lets `ask` compile the official 3.53.4 amalgamation itself and link it through the sys crate without a bundled feature. Dependency cost stays on registry crates, avoiding a git source. Distribution and maintenance cost is high and recurring: `ask` must build and pin the amalgamation for all four supported targets, wire the build and environment into the normal crate build and CI, keep static linkage verified so no dynamic system SQLite leaks in, and re-cut the vendored amalgamation on every SQLite update. This reproduces much of what the bundled feature already does, for the benefit of a version the crate has not yet released.

## Recommendation

Adopt the bundled candidate, rusqlite 0.40.2 with libsqlite3-sys 0.38.2 and SQLite 3.53.2, as the lowest-maintenance practical route, on the strength of the corrected ancestry: the one regression previously feared (`bf70dadc2d`) does not apply, and the genuine gap is damaged-file hardening whose only demonstrated effect is a missing rejection on an already corrupt file, not a healthy-write corruption path. The alternatives close the hardening gap but cost more now: waiting depends on an unscheduled upstream release, a git pin needs a dependency-policy exception, and a hand-built external amalgamation carries high recurring four-target build and maintenance cost to reach a version the crate has not released. Pair adoption with the native-monitoring proposal below so a crate release bundling 3.53.4 becomes a tracked triage item rather than a silent lag.

The narrow owner decision this needs: accept SQLite 3.53.2 through the bundled candidate with its documented damaged-file hardening gap as an explicit residual-risk acceptance, or defer adoption until a crate release bundles 3.53.4.

## Proposed native monitoring and verification migration

Do not build the following now. It is a proposal for owner approval and is not a prerequisite for query-input or storage work.

Native monitoring should extend the existing nightly dependency check rather than add a parallel system. Alongside the current `cargo deny` advisory and Rust dependency-freshness checks, the nightly should track SQLite release and maintenance check-ins and security announcements, and record the locked crate versions, the native source id, the enabled compile options, and the last triaged check-in. A new upstream SQLite version creates a triage item, not an automatic failure or upgrade. For each relevant fix, the triage item records affected-version ancestry, the actual trigger, compiled-feature and workload reachability, integrity or correctness impact, disposition, owner, and a review deadline, and revisits accepted exclusions when SQL, open flags, VFS, or Rust features change. Absence of a Rust advisory is not native clearance.

After adoption, the useful probes should move into the standard verification entrypoint rather than remaining a separate permanent gate. The transaction, rollback, reopen, busy, and isolation checks should become ordinary integration tests against the production storage boundary so `just verify` and `just verify-full` run them; native version and source-id checks and release-linkage checks belong in the full gate against the real release artifact; a differential damaged-file check like this record's probe belongs with them so a future version regression is caught. Once these are covered, retire the separate `sqlite-readiness` workflow, the isolated probe workspace, and the standalone scripts rather than keeping a permanent parallel gate.

## Limits

This record establishes fix ancestry, one demonstrated damaged-file differential, and route facts. It does not audit each patch line by line, exhaust the branch interval or CVEs, reproduce the differential through the exact bundled feature build or on all four targets, or establish exploitability or frequency. It does not resolve storage semantics: retention and deletion across history and statistics, provider-target identity under configuration changes, partial or failed query recording, atomicity between related records, connection ownership and async blocking boundaries, file layout, schema and migration and downgrade behavior, journal and busy policy, and side-effect-free offline doctor validation all remain owner and repository decisions taken separately. Publisher-authenticity limits from the September 9 assessment are unchanged. Production adoption and storage implementation remain separate authorizations, and no promotion is proposed here.
