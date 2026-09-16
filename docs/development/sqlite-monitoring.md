# Native SQLite monitoring

`ask` links the SQLite amalgamation bundled by its exact `rusqlite` and `libsqlite3-sys` versions. The report-only monitor makes changes in that native dependency visible even when Rust advisory data has no corresponding entry. It does not update a dependency, accept a risk, or clear a version for release.

## Source of truth

[`monitoring/sqlite-native.toml`](../../monitoring/sqlite-native.toml) is the current native ledger. Its `[locked]` table records the two crate versions and registry checksums in `Cargo.lock`. Its `[native]` table records the linked SQLite version, numeric version, source ID, amalgamation digest, compile options, and the expected result of the damaged-index-cell differential. The compile-option list omits only `COMPILER`, whose value varies by target.

The `[triage]` table points to the dated adoption record that owns the accepted residual-risk reasoning. It records the acceptance date, version trigger, review deadline, last completely triaged SQLite release, and last triaged check-in. Each `[[triage.items]]` entry restates one disposition from that record with its ancestry, trigger, reachability, impact, owner, and review condition. A `[[cve_triage]]` entry is added only after a review has supplied a disposition; it requires a nonempty `ids` list, `disposition`, `owner`, ISO `recorded_on` date, and ISO `review_deadline` date. Malformed or incomplete entries are local-input errors. A disposition suppresses a matching finding only from its recording date through its review deadline; outside that interval the report leaves the CVE untriaged and states why. Changing the ledger cannot create or extend an acceptance; update the dated review or obtain the required owner decision first.

The current acceptance is SQLite 3.53.2 through `rusqlite` 0.40.2 and `libsqlite3-sys` 0.38.2. Its scope and five accepted residual risks are in the [2026-09-10 adoption record](../reviews/sqlite-adoption-2026-09-10.md). That record also establishes that check-in `bf70dadc2d` fixes a regression introduced in SQLite 3.53.3 and does not apply to 3.53.2. The acceptance must be revisited when a released crate pair bundles SQLite 3.53.3 or later, or by 2026-10-10, whichever comes first. More specific triggers in the ledger remain binding.

## Nightly report

The `nightly` workflow runs `python3 scripts/sqlite-monitor.py` after the Rust advisory and dependency-freshness steps. It reads the ledger and `Cargo.lock`, then reads these public upstream sources without credentials:

- the crates.io sparse-index rows for `rusqlite` and `libsqlite3-sys`;
- the checksummed archive of a newer stable `libsqlite3-sys`, when one exists, to read its bundled SQLite version and source ID;
- SQLite's download page for a release newer than the last fully triaged release, then the bounded release-to-release Fossil timeline to enumerate at most 100 distinct check-ins before the last triaged marker and ignore older entries after that marker;
- SQLite's CVE page for rows that are neither fixed at or before the linked version, classified upstream as outside the SQLite library, nor covered by a complete, current ledger disposition. Report text uses a fixed classification for upstream fix cells that do not name a release version.

The report is written to stdout and the GitHub job summary. New versions, a reached revisit threshold, an expired review deadline, untriaged CVE rows, and drift between the lockfile and ledger are triage items. Findings do not fail the step and never trigger an update. A network or upstream-format error is shown as `not checked`; it supplies no clearance. The command exits nonzero only when its local inputs or report machinery cannot run, and its failure diagnostic omits raw exception text.

Run the deterministic fixture suite and an offline report with:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/sqlite-monitor-test.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/sqlite-monitor.py --offline
```

`just nightly-deps` runs the live public-source report after the existing advisory and dependency-freshness commands. It can stop before the monitor if an earlier command fails; the GitHub workflow uses a separate `if: !cancelled()` step so the report still runs after an earlier failure.

## Regression checks

The standard Rust gate includes these native checks:

- `tests/sqlite_identity.rs` compares the linked version, numeric version, source ID, and target-independent compile options with the ledger.
- `tests/sqlite_damaged_file.rs` recreates the adoption record's damaged index-cell differential, requires the ledger's expected result, verifies that `PRAGMA integrity_check` detects damage, and confirms the check did not modify the database bytes.
- `tests/sqlite_monitoring.rs` runs the Python monitor fixture suite offline.
- `src/store_tests.rs` covers transaction rollback, reopen and isolation behavior; `a_held_write_delays_a_second_writer_and_hides_nothing_from_readers` additionally proves the configured 1000 ms busy timeout, reader visibility of committed state during a held write, rollback recovery, and a successful later write.

The release workflow separately builds and tests the production binary natively on all four supported targets. Packaging requires `sqlite3_open_v2` to be defined in the unstripped binary and rejects a dynamic SQLite library before recording the sanitized library inventory in `manifest.json`.

## Triage and updates

For a new report item, inspect primary upstream release, check-in, CVE, crate-index, and crate-archive evidence as applicable. Record affected-version ancestry, the actual trigger, compiled-feature and workload reachability, integrity or correctness impact, disposition, owner, and review deadline. A dependency update still follows the quarantine and review requirements in `SECURITY.md`; the monitor never authorizes it.

When an accepted crate or native version changes, update the lockfile, ledger identity, compile options, damaged-file expectation, triage record, and relevant tests in one reviewed change. Run the full gate and the four native release jobs against the exact candidate. Do not infer the bundled SQLite version from a crate version number; inspect the checksummed archive.

## Readiness workflow remains

The separate `sqlite-readiness` workflow, probe workspace, and scripts remain. The production gate now has native identity, compile-option, damaged-file, rollback, reopen, isolation, and busy-timeout checks, while the release workflow defines four real-binary linkage checks. Retirement still lacks concrete successful real-release evidence for the exact candidate on all four supported targets and a review showing that the production checks reproduce every applicable transaction and journal-mode behavior from the isolated readiness probe. Until those proofs and the monitoring prerequisites are recorded together, the separate workflow remains the preserved comparison boundary.
