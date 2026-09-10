# SQLite dependency assessment

Review date: 2026-09-06. Baseline: `cfa8420e570deee7839c88cd1d05e00c0b9216eb`.

Status: focused dependency recommendation for owner review; no dependency adoption or storage design decision. The evidence directory is a historical assessment record, not an application workspace or a replacement for the production lockfile.

## Recommendation

Prefer the following exact dependency for local history, query statistics, and provider-target health observations, subject to the adoption gaps below:

```toml
rusqlite = { version = "=0.40.2", default-features = false, features = ["bundled"] }
```

The probe locks `libsqlite3-sys` 0.38.2. Preserve that transitive resolution if this candidate is adopted; the direct pin alone permits future compatible sys releases. This configuration adds five packages to the existing lockfile, one build script, and no procedural macros. It supplies parameterized SQL and transactions without requiring an ORM, pool, migration package, or asynchronous wrapper. That is a good fit for a small terminal application's local records; this is an assessment, not a performance benchmark.

**Adoption is conditional:** the latest rusqlite/sys releases bundle SQLite 3.53.2, whereas upstream's latest listed release is 3.53.4 (2026-07-24). Subsequent fixes need applicability review or explicit owner acceptance before adoption. A passing RustSec check does not resolve this gap. Source provenance is partially verified, and three supported targets remain unbuilt. The report recommends a package/configuration, not approval to ship it.

Disable defaults deliberately: rusqlite 0.40.2 defaults enable statement caching and a wasm backend. Neither is required for this assessment. Add caching or date/JSON adapters only after an actual need and feature-delta review. Do not enable SQLCipher, extension loading, `modern-full`, macros, or build-time bindgen speculatively. See the [published feature manifest](https://docs.rs/crate/rusqlite/0.40.2/source/Cargo.toml) and [captured registry features](evidence/sqlite-dependency-assessment/registry.json).

## Evidence and method

Isolated probe packages copied the baseline manifest and lockfile, substituted empty application targets, and appended one exact dependency declaration. Cargo metadata resolved the copied lockfiles without an update of existing versions. Each graph was inspected with `cargo tree --locked -e normal,build --target <triple>` on all four supported triples. Counts compare package name/version identities with the baseline; they do not measure binary size, compile time, or runtime latency. Optional lockfile entries are distinguished from reachable build dependencies.

The [evidence guide](evidence/sqlite-dependency-assessment/README.md) documents commands, normalization, files, and limitations. [Closure evidence](evidence/sqlite-dependency-assessment/closure.json) includes added/removed entries, package licenses and build-target kinds, target reachability, and an active Linux feature tree. Exact probe manifests/locks and policy outputs are retained beside it. Registry, GitHub, SQLite, and RustSec sources were retrieved on 2026-09-06. No production manifest, lockfile, policy, workflow, or source was edited.

## Viable options

| Candidate | Verified current release and role | Assessment |
| --- | --- | --- |
| rusqlite | 0.40.2, released 2026-08-08; synchronous wrapper around SQLite's C API | Preferred for the smallest reviewed increment and direct control of short SQL operations. Requires deliberate integration with the async runner. |
| SQLx SQLite | 0.9.0, released 2026-05-21; async API, connection workers, pool and optional SQL checking/migrations | Credible alternative if asynchronous connection management or checked SQL justifies the larger closure. Prefer defaults off with `sqlite-bundled,runtime-tokio`; use ordinary `query` APIs initially. |
| Diesel SQLite | 2.3.13, released 2026-09-04; typed query builder/ORM | Credible maintained option if typed relational modeling becomes an explicit requirement. Its derive/schema model and separate migration tooling add choices not justified by the storage dependency request. No closure probe or build performed. SQLite uses `libsqlite3-sys`; bundled linkage would need explicit feature unification with that dependency. |
| `sqlite` | 0.37.0, released 2025-03-28; smaller synchronous API over `sqlite3-sys` | Plausible alternative, but changes the native wrapper family and has a less recent stable release. `bundled` exists; defaults enable `linkage`. No closure, native version, advisory, or provenance equivalence established. Do not infer that it is unmaintained from release age. |
| `tokio-rusqlite` | 0.8.0, released 2026-09-06; rusqlite async adapter | Consider only if its connection-worker API removes demonstrated integration work. It is independently owned, not the Tokio project's rusqlite component. No resolution/build performed, so compatibility with the recommended pin and incremental cost remain unverified. |
| libSQL / Turso | `libsql` 0.9.30 (2026-03-19), `turso` 0.7.2 (2026-07-30) | libSQL is a SQLite fork; Turso is a Rust SQLite-compatible engine. Both are credible projects, but introduce a different engine/compatibility decision. Remote/replication capabilities are unnecessary for this local-storage requirement. Do not substitute either solely to obtain an async API or avoid C. |

Release and feature facts come from the dated [crates.io snapshot](evidence/sqlite-dependency-assessment/registry.json). Primary descriptions: [Diesel](https://diesel.rs/), [`sqlite` API](https://docs.rs/sqlite/0.37.0/sqlite/), [`tokio-rusqlite`](https://docs.rs/crate/tokio-rusqlite/0.8.0), [libSQL](https://github.com/tursodatabase/libsql), and [Turso](https://github.com/tursodatabase/turso). The alternatives outside rusqlite/SQLx are screened, not adoption-ready dependency reviews.

## Measured incremental closure

All configurations set `default-features = false`. The baseline has 240 lockfile entries including `ask`. No probe removes a baseline package or changes an existing version.

| Probe | Exact version and features | Added lock entries | Added reachable packages on each supported target | New reachable build scripts / proc macros |
| --- | --- | ---: | ---: | ---: |
| rusqlite bundled | 0.40.2: `bundled` | 5 | 5 | 1 / 0 |
| rusqlite system | 0.40.2: none | 5 | 5 | 1 / 0 |
| SQLx bundled alias | 0.9.0: `sqlite,runtime-tokio` | 31 | 25 | 3 / 0 |
| SQLx lean bundled | 0.9.0: `sqlite-bundled,runtime-tokio` | 31 | 25 | 3 / 0 |
| SQLx system | 0.9.0: `sqlite-unbundled,runtime-tokio` | 41 | 34 | 5 / 0 |
| SQLx migration + macros | 0.9.0: `sqlite,runtime-tokio,migrate,macros` | 51 | 30 | 3 / 1 |

The entire rusqlite delta is `rusqlite` 0.40.2, `libsqlite3-sys` 0.38.2, `fallible-iterator` 0.3.0, `fallible-streaming-iterator` 0.1.9, and `vcpkg` 0.2.15. Existing dependencies already supply `bitflags`, `smallvec`, `cc`, `pkg-config`, and the C compiler helper closure. `vcpkg` is compiled as a build dependency even on these Unix targets, although its library-discovery path is conditional on MSVC. No new duplicate-version family or added active feature on a baseline package is introduced by rusqlite.

SQLx adds its core and SQLite driver, channel/synchronization packages, cache/hash packages, and `tokio-stream`. Its `hashbrown` 0.16.1 coexists with baseline 0.17.1. Its system configuration also adds `shlex` 1.3.0 beside baseline 2.0.1. The lean bundled feature keeps the same package counts while avoiding the `sqlite` alias's deserialize, load-extension, and unlock-notify feature bundle. The detailed feature tree, not just package count, matters. All SQLx probes add `tracing/log` and default feature markers for `futures-io`, `futures-executor`, and `form_urlencoded` on shared packages; the migration/macros probe also adds Serde and serde_core `rc`. The measured Tokio feature union does not change.

Minimal SQLx locks `sqlx-macros` and `sqlx-macros-core` without compiling them. The migration/macros probe locks MySQL/Postgres packages and additional crypto packages but does not reach those drivers on the supported target graphs. Its five extra reachable packages over bundled SQLx are `dotenvy`, `heck`, `hex`, `sqlx-macros`, and `sqlx-macros-core`. Do not describe every locked package as compiled, or infer that SQLx's defaults-off configuration eliminates every internal default: `sqlx-core` still enables its own defaults, including migration support. See [SQLx's exact manifest](https://docs.rs/crate/sqlx/0.9.0/source/Cargo.toml) and the captured active trees.

## Build and native surface

The new rusqlite build script is `libsqlite3-sys/build.rs`. Bundled builds compile the included SQLite amalgamation through the existing `cc` crate and copy pregenerated Rust bindings. They require a C compiler and archiver; this configuration does not require bindgen, libclang, CMake, SQLCipher, or another TLS library. The source enables several C capabilities, including FTS3/FTS5, RTREE, URI handling, thread safety, and extension-loading support. Omitting the Rust extension feature does not remove that code from the amalgamation. No application extension-loading behavior is proposed.

The script reads compiler and SQLite-related environment overrides. In particular, `LIBSQLITE3_SYS_USE_PKG_CONFIG` can select linked SQLite despite `bundled`; `LIBSQLITE3_FLAGS` can change compiler definitions. Consequently, a lockfile is insufficient to identify the native output without build-environment evidence. These are inspected build inputs, not a proposed workflow change. [Exact reviewed build script](https://github.com/rusqlite/rusqlite/blob/e88f112bef7899234a497baed5cc3c3d553deeb8/libsqlite3-sys/build.rs).

SQLx bundled resolves `libsqlite3-sys` 0.37.0, because its SQLite driver specifies `>=0.30.1, <0.38.0`; that bundles SQLite 3.51.3. SQLx's extra build scripts are `crossbeam-utils` (target atomic/sanitizer configuration) and `parking_lot_core` (sanitizer configuration), inspected in the published sources. System SQLx activates `buildtime_bindgen`, adding bindgen and clang-sys build scripts plus libclang discovery/header parsing. Bundled rusqlite and SQLx cannot simply coexist at these resolutions: Cargo permits only one package with the native `links = "sqlite3"` identity. [SQLx SQLite manifest](https://docs.rs/crate/sqlx-sqlite/0.9.0/source/Cargo.toml).

SQLx macros are optional build-time Rust execution. Checked queries can consult a database at build time or use prepared offline metadata; migration embedding creates another build input. That is a different release/build surface from runtime `query` calls. No checked-query build, macro expansion audit, or offline-metadata workflow was tested.

## Bundled versus system and portability

Bundling makes the SQLite engine version part of the locked application dependency and avoids requiring a suitable SQLite installation on a user's computer. Updating native fixes then requires an application dependency update/rebuild. System linkage delegates the engine version and patch cadence to the distribution or OS and needs suitable development libraries at build time; it also makes runtime behavior vary by installation. These are distribution tradeoffs, not reasons to change the four supported targets.

| Target | Dependency graph evidence | Build/runtime evidence |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | All six probe graphs resolved | Separate rusqlite bundled smoke compiled and ran with Rust/Cargo 1.97.1; SQLite 3.53.2 reported |
| `aarch64-unknown-linux-gnu` | All six probe graphs resolved | Not built or run |
| `x86_64-apple-darwin` | All six probe graphs resolved | Not built or run |
| `aarch64-apple-darwin` | All six probe graphs resolved | Not built or run |

The smoke checks an in-memory transaction, parameter binding, readback, and `user_version`. It is not an application build or proof of file locking, WAL, crash recovery, upgrade safety, or latency. No SQLx or system-linked binary was built. Cross-compilation still needs target C toolchains/sysroots; macOS needs the appropriate Apple SDK and deployment-target checks. System linkage needs per-target library/header compatibility checks. rusqlite documents SQLite 3.34.1 as its base minimum; feature-specific APIs may require newer engines. SQLx 0.9.0 declares Rust 1.94.0, below the baseline 1.97.1 toolchain. rusqlite declares no numeric MSRV in its package metadata and documents support for the stable Rust release current at release time. [rusqlite release documentation](https://docs.rs/crate/rusqlite/0.40.2), [SQLx SQLite linkage documentation](https://docs.rs/sqlx/latest/sqlx/sqlite/).

## Licenses, advisories, ownership, and provenance

All six unchanged-policy checks returned `advisories ok, bans ok, licenses ok, sources ok`, with duplicate-version and unused-license-allowance warnings. Every added package uses crates.io. The selected rusqlite additions declare MIT or MIT/Apache-2.0; SQLx additions use allowed permissive expressions, including Zlib, and its bindgen branch additionally includes BSD-3-Clause/ISC. Package-by-package declarations are preserved in the closure evidence. SQLite itself is public domain; that upstream dedication is separate from the wrapper's MIT license. No SQLCipher/encryption features were selected, and no license-policy change is recommended. [SQLite copyright statement](https://sqlite.org/copyright.html).

The refreshed RustSec database was `5a0ebedfe8bdd2e295b171f4162f8c977bcad9a5`, dated 2026-09-02; a read-only remote HEAD check on 2026-09-06 matched it. No matching advisory was reported for the checked graphs. Historical [RUSTSEC-2024-0363](https://rustsec.org/advisories/RUSTSEC-2024-0363.html) concerns SQLx's PostgreSQL protocol casts and was fixed in 0.8.1; it is not a finding against this SQLite-only 0.9.0 graph. The screened alternatives did not receive advisory checks.

RustSec does not establish the absence of native SQLite defects. Upstream lists SQLite 3.53.3 and 3.53.4 fixes after the recommended bundle's 3.53.2. Both reviewed amalgamations include the WAL-reset corruption fix identified for 3.51.3/3.53.0, but this does not settle other fixes or workload applicability. No exhaustive CVE-to-code mapping or post-3.53.2 commit audit was performed. [SQLite release history](https://sqlite.org/changes.html) and [upstream security guidance](https://sqlite.org/security.html).

Current crates.io owners are `thomcc` and `gwenn` for both rusqlite and libsqlite3-sys, and `abonander` and `mehcode` for SQLx. These are user accounts, not team records. GitHub reports both repositories unarchived with activity in September 2026. The SQLx published repository URL uses `launchbadge/sqlx`; GitHub now resolves its repository to `transact-rs/sqlx`. This review records the ownership/location evidence without assuming publisher account controls. Diesel has two user owners plus `github:diesel-rs:core`; `sqlite` has one user owner; `tokio-rusqlite` has two user owners. Full dated rosters for screened options appear in the registry snapshot. Release cadence and owner counts are maintenance signals, not security guarantees.

Verified provenance boundaries:

- SHA-256 of downloaded rusqlite 0.40.2, libsqlite3-sys 0.38.2 and 0.37.0, and SQLx 0.9.0 archives matched crates.io's exact-version checksums. These checks establish registry artifact integrity, not independent publisher authenticity.
- rusqlite 0.40.2 and sys 0.38.2 record source commit `e88f112bef7899234a497baed5cc3c3d553deeb8`, matching the rusqlite `v0.40.2` lightweight tag. GitHub reports the source commit unsigned.
- SQLx 0.9.0 records source commit `003b698e99e024f3621b8043a2426fde5b741171`; its `v0.9.0` lightweight tag points to a different commit, `75bc0487eb661da811bb7a3c5d158f1bd463fef4`. The difference was observed but not reconciled. Do not claim tag-to-package correspondence.
- Three file spot checks matched the declared source commits: rusqlite `src/lib.rs`, sys 0.38.2 `build.rs`, and SQLx `src/lib.rs`. This is not a complete source-tree comparison.
- SHA3-256 of each bundled `sqlite3.c` matched its version's hash in SQLite's official release history. The corresponding header source IDs were recorded.

[Archive and commit evidence](evidence/sqlite-dependency-assessment/provenance.json), [tag, source spot-check, amalgamation, and advisory freshness evidence](evidence/sqlite-dependency-assessment/source-checks.json). Publishing workflows, trusted-publisher credentials, release attestations, reproducible builds, all transitive owners, and complete archive/source equivalence were not verified.

## Async and schema implications without architectural adoption

The application currently uses Tokio's current-thread runtime. rusqlite is synchronous: running disk operations or lock waits directly inside an async task can delay provider streaming and timers. A plausible integration is to execute bounded transactions through Tokio `spawn_blocking`, moving owned inputs/results across the boundary; a dedicated connection worker is another option if connection lifetime requires it. Neither is selected here. Tokio's blocking work continues once started even if its awaiting task is aborted, so a query timeout must not be assumed to cancel a database write. [Tokio blocking-task contract](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).

SQLx already moves SQLite work to background threads and provides an async API and pools; SQLite file I/O does not become natively asynchronous. Pool sizing, busy waits, serialization of writes, and cancellation still need application-level behavior tests. No evidence here justifies multiple connections or a pool for `ask`. [SQLx SQLite implementation documentation](https://docs.rs/sqlx/latest/sqlx/sqlite/).

rusqlite does not require a migration framework. Explicit versioned SQL plus transactions is feasible; the smoke establishes only the underlying primitives. SQLx offers migration management, with optional embedding/macros. Either choice still requires version detection, atomic upgrade behavior, handling of newer databases, failure recovery, and migration tests. Selecting a Rust package need not select migration tooling or database topology. [SQLx migration API](https://docs.rs/sqlx/0.9.0/sqlx/migrate/), [SQLite transaction semantics](https://sqlite.org/lang_transaction.html).

Remaining owner/design questions are retention and deletion semantics across history/statistics, provider-target identity when configuration changes, treatment of partial/failed queries, and whether related records must commit together. File layout, schema, connection ownership, journal mode, busy policy, migration/version strategy, and downgrade behavior remain outside this assessment. In particular, offline `doctor` must validate storage without creating/migrating it or recording a new health observation. Any eventual implementation must preserve the distinction between last-observed health and current availability. These implications identify future decisions; they do not settle them.

## Adoption gaps and handoff

1. Accept or resolve SQLite 3.53.2's upstream patch gap before adopting the proposed rusqlite pin. No policy exception or vendored patch is proposed here.
2. Decide whether the observed provenance evidence is sufficient or require complete crate/tag/source and publisher-workflow review. SQLx has an additional unresolved tag/source-commit mismatch.
3. Build and exercise the chosen configuration on all four supported targets, including real-file transaction/locking behavior and required release linkage checks. Only the separate Linux x86-64 smoke is established here.
4. Keep storage semantics and architecture decisions in their own authorized design/implementation work. This report introduces no accepted decision or mental-model edit.
