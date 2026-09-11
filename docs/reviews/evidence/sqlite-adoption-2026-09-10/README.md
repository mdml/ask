# September 10 SQLite adoption evidence

This directory records the dated observations behind the [SQLite adoption record](../../sqlite-adoption-2026-09-10.md). That record is the interpretation, route comparison, and owner handoff; the files here are observations and a reproducible probe, not adoption approval. This follow-up supersedes the applicability inferences in the [September 9 readiness assessment](../../sqlite-readiness-2026-09-09.md), which remains a historical record.

| File | Purpose |
| --- | --- |
| `fix-ancestry.json` | The six named SQLite check-ins with commit dates, comments, and containing releases (3.53.3 and 3.53.4), plus per-release dates and `sqlite3.c` SHA3-256 digests. Records the `bf70dadc2d` correction: it is contained in 3.53.4 only and absent from the candidate 3.53.2 and from 3.53.3. |
| `route-comparison.json` | Registry index observations, candidate versions and checksums, the rusqlite release-lag finding, master header version and source id, the absence of an imminent-release signal, `libsqlite3-sys` build modes, and the dependency-policy implication for a git source. |
| `differential-probe.json` | Output of `scripts/sqlite-differential-probe.py`: per-version library version and source id, indexed-SELECT and integrity-check result codes on one corrupted index cell, and the observed differential. |
| `readonly-wal-path.json` | Source citations (file, function, amalgamation line, quoted condition) from SQLite 3.53.2 behind the record's "Offline diagnostics and the read-only WAL path" section: the diff of check-in `9a79ca31ff`, the conditions that reach the heap-memory WAL-index path, what `SQLITE_OPEN_READONLY` and `immutable=1` do, hot-journal handling on a read-only pager, and per-strategy exclusion with residuals. Read from source and the upstream report; not executed. |
| `verification.txt` | Checks run for this record and their limits. |

## Reproduce the differential probe

Requirements: a native C compiler (`cc`), Python 3.11 or newer, and the three official SQLite amalgamation archives named in `differential-probe.json` (`sqlite-amalgamation-3530200.zip`, `-3530300.zip`, `-3530400.zip`). Download them from SQLite's [download page](https://sqlite.org/download.html) into one directory; do not add the archives to the repository. For example, `curl --fail --silent --show-error --location URL --output FILE` retrieves each archive. Then run:

```sh
python3 scripts/sqlite-differential-probe.py '<input-directory>'
```

`<input-directory>` is a placeholder for the download directory. The script performs no network access. It verifies each archive's SHA-256 against a pinned value and each extracted `sqlite3.c` against SQLite's published per-release SHA3-256, then compiles each amalgamation as a shared library with `cc` using the flags recorded in the output. It builds one small database with a 512-byte page size, one text row at rowid 1, and one index; the index-leaf record encodes with a payload length of 4. It corrupts that single payload-length byte to 100, then opens the same corrupted image with each build and runs an indexed `SELECT` and `PRAGMA integrity_check`. On success stdout contains only result JSON; errors go to stderr and return nonzero. Verification uses unconditional checks rather than `assert`, so results hold under `python3 -O`. Temporary build and database files are created under a private temporary directory and deleted; only basenames, result codes, and content digests are emitted.

The probe runs in well under a minute after compilation; the three amalgamation compiles dominate wall time. Result codes are SQLite's own: `SQLITE_ROW` (a result row was produced), `SQLITE_DONE`, `SQLITE_OK`, and `SQLITE_CORRUPT`.

## Evidence boundaries

- The probe demonstrates one effect: check-in `6826c17021` makes an indexed read reject a cell whose payload length runs off the page, where 3.53.2 returns the row. It establishes a missing rejection on an already damaged file. It does not demonstrate memory disclosure, exploitability, or any healthy-write corruption path.
- `PRAGMA integrity_check` reports the same damage in all three builds. It surfaces the diagnosis as a result row (`SQLITE_ROW`) whose text names the page and cell, not as an error result code. A passing integrity check tests a healthy database only and is not a complete safe parser for arbitrary damaged files.
- The probe builds standalone shared libraries with `cc`. It is not a build through the exact `rusqlite`/`libsqlite3-sys` bundled feature set, and it was executed on one target (`x86_64-unknown-linux-gnu`), not all four supported targets.
- Fix ancestry is established from the upstream timeline, check-in info pages, and change log. It is not a line-by-line audit of each patch, an exhaustive audit of the branch interval, or a CVE review.
- Registry, GitHub, and rusqlite-master observations are HTTPS-source facts, not signed publisher attestations. Publisher-authenticity limits from the [September 9 readiness assessment](../../sqlite-readiness-2026-09-09.md) are unchanged.

## Sanitization performed

The probe emits only version strings, source ids, library basenames, SQLite result codes, integrity-check report text (page and cell numbers only), compile flags, and content digests. It retains no absolute paths, hostnames, user names, environment values, or raw compiler output. Retrieval URLs in the JSON files are public upstream sources, not private session links. The staged diff was scanned for machine paths, hostnames, user names, session URLs, and tokens before commit.
