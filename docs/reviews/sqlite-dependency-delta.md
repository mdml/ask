# SQLite dependency delta

Review date: 2026-09-11 UTC. Base: `origin/staging` at `3db04ce`.

This record covers the production dependency change that adds SQLite to `ask`. The route, the native residual risk, and the owner acceptance are in the [SQLite adoption record](sqlite-adoption-2026-09-10.md); this record does not restate or reopen them. The change adds a dependency only. `ask` has no schema, database file, command, or runtime SQLite use, so the `ask` binary does not yet link SQLite.

## Declaration

`Cargo.toml` declares `rusqlite = { version = "=0.40.2", default-features = false, features = ["bundled"] }`. `Cargo.lock` resolves `libsqlite3-sys` 0.38.2, which bundles SQLite 3.53.2.

## Lockfile delta

`Cargo.lock` grew from 240 to 245 package entries. No package was removed and no existing package changed version; the only change to an existing entry is `rusqlite` in the `ask` dependency list. Every added entry uses Cargo's crates.io registry source, `registry+https://github.com/rust-lang/crates.io-index`.

| Added package | Version | License | Build script | Checksum (SHA-256) |
| --- | --- | --- | --- | --- |
| `fallible-iterator` | 0.3.0 | MIT/Apache-2.0 | no | `2acce4a10f12dc2fb14a218589d4f1f62ef011b2d0cc4b3cb1bba8e94da14649` |
| `fallible-streaming-iterator` | 0.1.9 | MIT/Apache-2.0 | no | `7360491ce676a36bf9bb3c56c1aa791658183a54d2744120f27285738d90465a` |
| `libsqlite3-sys` | 0.38.2 | MIT | yes (`links = "sqlite3"`) | `f1d20bef17f513b9b3004532233187769cd072d790971f4e4da0e346eb6401e8` |
| `rusqlite` | 0.40.2 | MIT | no | `23f2a97da3e3873c73cb2a2e71b35c40ff95e0b1eefa8d72d8499a6928c3b5b3` |
| `vcpkg` | 0.2.15 | MIT/Apache-2.0 | no | `accd4ea62f7bb7a82fe23066fb0957d48ef677f6eeb8215f372f52e48bb32426` |

This is exactly the closure measured for the bundled candidate in the [dependency assessment](sqlite-dependency-assessment.md#measured-incremental-closure): the same five names and versions, and the same sources and checksums as its [probe lockfile](evidence/sqlite-dependency-assessment/rusqlite-bundled/Cargo.lock). Licenses and build-script kinds match its [closure evidence](evidence/sqlite-dependency-assessment/closure.json). No package adds a procedural macro. All five packages are reachable in `cargo tree --locked -e normal,build --target <triple>` on each of the four supported targets.

## Features

`cargo tree -e features -i rusqlite` shows `bundled`, requested by `ask`, and `modern_sqlite`, enabled by `bundled`. `cargo tree -e features -i libsqlite3-sys` shows `bundled`, `bundled_bindings`, and `cc` from the bundled path, plus the sys crate's default features (`default`, `min_sqlite_version_3_34_1`, `pkg-config`, and `vcpkg`) because `rusqlite` depends on it with defaults enabled. The accepted closure recorded the same active sys features. `pkg-config` and `vcpkg` are build dependencies for library discovery; with `bundled`, the build script compiles the amalgamation unless the build environment sets `LIBSQLITE3_SYS_USE_PKG_CONFIG`, as described in the [assessment's native-surface review](sqlite-dependency-assessment.md#build-and-native-surface). A comparison of the complete feature trees before and after the change shows no feature newly enabled on a package that was already in the graph.

## Policy

`cargo deny --locked check` with the unchanged `deny.toml` reports `advisories ok, bans ok, licenses ok, sources ok`. Its warnings are the same as on the base lockfile: duplicate `core-foundation` and `syn` versions, and allowlisted licenses not encountered (`Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `Zlib`). The advisory result does not cover native SQLite fixes; that gap is the residual risk accepted in the adoption record.

## Native identity

[`tests/sqlite_identity.rs`](../../tests/sqlite_identity.rs) asserts that the linked SQLite reports version 3.53.2 (`3053002`) and source id `2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24`. That source id is the one recorded for the official 3.53.2 amalgamation in the [adoption fix-ancestry evidence](evidence/sqlite-adoption-2026-09-10/fix-ancestry.json) and the [assessment source checks](evidence/sqlite-dependency-assessment/source-checks.json), and it equals `SQLITE_SOURCE_ID` in the `sqlite3/sqlite3.h` shipped in the `libsqlite3-sys` 0.38.2 crate. The test is part of the ordinary suite, so `just verify` runs it, and the CI target matrix runs it natively on all four supported targets through `cargo test --locked`. A build that silently links a different SQLite, for example through a build-environment override, fails this test.

## Release linkage

Release linkage of the `ask` binary cannot be checked meaningfully yet, because no production code uses SQLite and the linker has nothing to link. The separate [`sqlite-readiness` workflow](../../.github/workflows/sqlite-readiness.yml) and its isolated [probe](../../probes/sqlite-readiness/Cargo.toml) remain the release-linkage coverage: on all four supported targets they build the same declaration in release mode, exercise real-file transactions in DELETE and WAL journal modes, assert the version and source id, and reject a dynamic SQLite dependency. The probe has its own lockfile and does not read the root `Cargo.toml` or `Cargo.lock`, so its path filters are unchanged. On 2026-09-11 a local run of `python3 scripts/sqlite-readiness.py x86_64-unknown-linux-gnu` passed: format, Clippy, and release build passed; both journal-mode scenarios passed; the source id matched; `sqlite3_libversion` is defined in the binary; and the dynamic libraries are only `libgcc_s.so.1`, `libm.so.6`, `libc.so.6`, and `ld-linux-x86-64.so.2`.

## Remaining work

The following belongs with the production storage implementation and its tests, as proposed in the adoption record's [native monitoring and verification migration](sqlite-adoption-2026-09-10.md#proposed-native-monitoring-and-verification-migration):

- Native monitoring in the nightly dependency check, so a released `rusqlite` and `libsqlite3-sys` pair bundling SQLite 3.53.3 or later becomes a triage item that revisits the accepted residual risk.
- Transaction, rollback, reopen, busy, and isolation tests against the production storage boundary.
- Release-linkage and native-identity checks against the real `ask` release artifact in the full gate, and a differential damaged-file check.
- Retirement of the `sqlite-readiness` workflow, the probe workspace, and its script once those checks cover the same ground.
