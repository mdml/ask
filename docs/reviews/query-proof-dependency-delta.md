# Query-proof dependency-delta review

Review date: 2026-09-05

Status: evidence record for human review. This report neither recommends acceptance nor rejection of the dependency delta.

## Scope and method

This review covers only the dependency delta between `origin/staging` at `69f1c7e17cf008d6f1848f04205f5b539b75b9ba` and `feat/query-proof` at `754ea944b33db187cd351e6c3348d458a1e08c41`. It does not repeat the Rig closure or provenance findings in [`rig-dependency-closure.md`](rig-dependency-closure.md) and [`rig-provenance.md`](rig-provenance.md).

The exact lockfile diff is preserved in [`cargo-lock.diff`](evidence/query-proof-dependency-delta/cargo-lock.diff). Supported-target reachability was measured with `cargo tree --locked -e normal,build --target <triple>` for each of the four supported triples and filtered to the 12 added packages. Package source, license, custom-build-target, and procedural-macro metadata came from `cargo metadata --locked --format-version 1`. Duplicate package versions in the base and head lockfiles were derived from package/version entries without checking out the base; only HEAD was passed to `cargo tree --locked -e normal,build --duplicates`. The tool versions and commit identities are in [`tool-versions.txt`](evidence/query-proof-dependency-delta/tool-versions.txt).

The crates.io requests used a descriptive `User-Agent` and read `GET /api/v1/crates/{crate}` and `GET /api/v1/crates/{crate}/owners` on 2026-09-05. The combined [normalized response](evidence/query-proof-dependency-delta/crates-io.json) retains only the fields cited here and has a top-level `_normalization` note. Machine-specific paths in command evidence were replaced with `<repo-root>` or `<cargo-home>`; filtered outputs that contained no machine-specific paths say so in their own normalization notes.

## Observed facts

### Exact lockfile delta

`Cargo.lock` increased from 228 to 240 package entries. All 12 added entries use the exact source `registry+https://github.com/rust-lang/crates.io-index`, Cargo's crates.io registry source. No package was removed, and no version of a package already present in the base lockfile changed.

| Added package | Version | Source |
| --- | --- | --- |
| `directories` | 6.0.0 | crates.io registry |
| `dirs-sys` | 0.5.0 | crates.io registry |
| `libredox` | 0.1.23 | crates.io registry |
| `option-ext` | 0.2.0 | crates.io registry |
| `redox_users` | 0.5.2 | crates.io registry |
| `serde_spanned` | 1.1.1 | crates.io registry |
| `tokio-macros` | 2.7.2 | crates.io registry |
| `toml` | 1.1.5+spec-1.1.0 | crates.io registry |
| `toml_datetime` | 1.1.1+spec-1.1.0 | crates.io registry |
| `toml_parser` | 1.1.3+spec-1.1.0 | crates.io registry |
| `toml_writer` | 1.1.2+spec-1.1.0 | crates.io registry |
| `winnow` | 1.0.4 | crates.io registry |

The 12 entries divide by direct cause as follows: `directories` adds itself, `dirs-sys`, `option-ext`, and the Redox-only `redox_users` and `libredox`; the newly enabled Tokio `macros` feature adds `tokio-macros`; and `toml` adds itself, `serde_spanned`, `toml_datetime`, `toml_parser`, `toml_writer`, and `winnow`.

### Direct dependency use and closure cost

| Direct dependency | Repository use | State before the change | Observed lock/feature cost |
| --- | --- | --- | --- |
| `directories` 6.0.0 | [`config_path`](../../src/config.rs#L93) calls `ProjectDirs::from` and uses `config_dir` to resolve the platform-standard configuration path. | Not in the Rig closure. | Truly new; causes five added lock entries, of which three are reachable on supported targets. |
| `futures-util` 0.3.34 with defaults off and `std` | [`EventStream` and `PromptProvider::start`](../../src/provider.rs#L11) use `Stream` and `StreamExt::map`; [`runner::run`](../../src/runner.rs#L41) uses `StreamExt::next`. | The same version was already transitive through Rig. The inverse feature tree shows Rig's existing `futures` path activating `futures-util/std`. | No added package and no newly required feature beyond the existing union. |
| `serde` 1.0.229 with `derive` | [`Config`, `ProviderConfig`, and `ProfileConfig`](../../src/config.rs#L9) derive `Deserialize`; `ProviderConfig::timeout_ms` uses `#[serde(default = ...)]`. | The same version and `serde_derive` were already transitive through Rig; the inverse feature tree identifies Rig as an existing activator of `serde/derive`. | No added package and no newly required feature beyond the existing union. |
| `toml` 1.1.5 | [`config::parse`](../../src/config.rs#L89) returns `toml::de::Error` and calls `toml::from_str`. | Not in the Rig closure. | Truly new; causes six added lock entries, all reachable on supported targets. |
| `tokio` 1.53.1 with `macros`, `rt`, and `time` | [`main`](../../src/main.rs#L3) uses `#[tokio::main(flavor = "current_thread")]`; [`runner::run` and `within`](../../src/runner.rs#L41) use Tokio deadlines, duration, instants, and `timeout_at`. | The same version was already transitive through Rig. Other existing graph paths activate `rt` and `time`. | The newly activated `macros` feature adds `tokio-macros` 2.7.2; the direct crate itself is not a new package. |

The base-presence and HEAD feature evidence is in [`direct-feature-reachability-head.txt`](evidence/query-proof-dependency-delta/direct-feature-reachability-head.txt), while the direct dependency entries and the one changed Tokio dependency list are visible in the exact lockfile diff.

### Supported-target reachability

The four filtered trees are [aarch64 macOS](evidence/query-proof-dependency-delta/tree-aarch64-apple-darwin.txt), [x86-64 macOS](evidence/query-proof-dependency-delta/tree-x86_64-apple-darwin.txt), [aarch64 Linux GNU](evidence/query-proof-dependency-delta/tree-aarch64-unknown-linux-gnu.txt), and [x86-64 Linux GNU](evidence/query-proof-dependency-delta/tree-x86_64-unknown-linux-gnu.txt).

| Added package | aarch64 macOS | x86-64 macOS | aarch64 Linux | x86-64 Linux | Other-target-only status |
| --- | --- | --- | --- | --- | --- |
| `directories` 6.0.0 | Reachable | Reachable | Reachable | Reachable | — |
| `dirs-sys` 0.5.0 | Reachable | Reachable | Reachable | Reachable | — |
| `libredox` 0.1.23 | Absent | Absent | Absent | Absent | Redox only, through `redox_users` |
| `option-ext` 0.2.0 | Reachable | Reachable | Reachable | Reachable | — |
| `redox_users` 0.5.2 | Absent | Absent | Absent | Absent | Redox only, through `dirs-sys`'s `cfg(target_os = "redox")` edge |
| `serde_spanned` 1.1.1 | Reachable | Reachable | Reachable | Reachable | — |
| `tokio-macros` 2.7.2 | Reachable | Reachable | Reachable | Reachable | — |
| `toml` 1.1.5+spec-1.1.0 | Reachable | Reachable | Reachable | Reachable | — |
| `toml_datetime` 1.1.1+spec-1.1.0 | Reachable | Reachable | Reachable | Reachable | — |
| `toml_parser` 1.1.3+spec-1.1.0 | Reachable | Reachable | Reachable | Reachable | — |
| `toml_writer` 1.1.2+spec-1.1.0 | Reachable | Reachable | Reachable | Reachable | — |
| `winnow` 1.0.4 | Reachable | Reachable | Reachable | Reachable | — |

### Sources, licenses, and advisories

The normalized package metadata is in [`package-metadata.tsv`](evidence/query-proof-dependency-delta/package-metadata.tsv). Every added package is sourced from crates.io. Every added package's license expression is satisfied by at least one identifier already in `deny.toml`'s allowlist.

| Added package | License expression | Allowlist result |
| --- | --- | --- |
| `directories` 6.0.0 | MIT OR Apache-2.0 | Within allowlist |
| `dirs-sys` 0.5.0 | MIT OR Apache-2.0 | Within allowlist |
| `libredox` 0.1.23 | MIT | Within allowlist |
| `option-ext` 0.2.0 | MPL-2.0 | Within allowlist |
| `redox_users` 0.5.2 | MIT | Within allowlist |
| `serde_spanned` 1.1.1 | MIT OR Apache-2.0 | Within allowlist |
| `tokio-macros` 2.7.2 | MIT | Within allowlist |
| `toml` 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 | Within allowlist |
| `toml_datetime` 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | Within allowlist |
| `toml_parser` 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | Within allowlist |
| `toml_writer` 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | Within allowlist |
| `winnow` 1.0.4 | MIT | Within allowlist |

The successful policy result recorded in [`cargo-deny.txt`](evidence/query-proof-dependency-delta/cargo-deny.txt) is `advisories ok, bans ok, licenses ok, sources ok`. The output contains no RUSTSEC identifier.

### Build scripts and procedural macros

None of the 12 added packages declares a custom-build target, so this delta adds no `build.rs` execution.

`tokio-macros` 2.7.2 is the only added procedural-macro crate. Its relevant expansion turns the async function marked by `#[tokio::main]` into a synchronous function that constructs the selected Tokio runtime and calls `block_on`; `src/main.rs` selects the current-thread flavor.

### Duplicate versions

The base and HEAD lockfile package inventories have the same nine duplicate name/version sets: `core-foundation` 0.9.4 and 0.10.1; `cpufeatures` 0.2.17 and 0.3.1; `getrandom` 0.2.17, 0.3.4, and 0.4.3; `r-efi` 5.3.0 and 6.0.0; `rand` 0.9.5 and 0.10.2; `rand_core` 0.9.5 and 0.10.1; `syn` 2.0.119 and 3.0.5; `webpki-roots` 0.26.11 and 1.0.9; and `windows-sys` 0.52.0 and 0.61.2. The delta introduces, removes, or changes none of those sets. The derived comparison is in [`duplicates-comparison.txt`](evidence/query-proof-dependency-delta/duplicates-comparison.txt).

At HEAD on the current x86-64 Linux host, `cargo tree --locked -e normal,build --duplicates` reports only `syn` 2.0.119 and 3.0.5 as reachable duplicates. The normalized full inclusion tree is in [`duplicates-head.txt`](evidence/query-proof-dependency-delta/duplicates-head.txt). The new `tokio-macros` parent uses the already-present `syn` 3.0.5 and does not create a new duplicate version.

### crates.io ownership and lifetime downloads

The crates.io `downloads` field is a crate-wide lifetime total, not a count specific to the locked version. The following values and current owner records were returned on 2026-09-05. Owner kinds reproduce the API's `kind` field; the API placed its team-kind records in the `users` response array.

| Crate | crates.io owner records | Lifetime downloads |
| --- | --- | ---: |
| `directories` | `soc` (user) | 68,646,951 |
| `dirs-sys` | `soc` (user) | 328,994,699 |
| `libredox` | `jackpot51` (user), `4lDO2` (user) | 195,858,426 |
| `option-ext` | `soc` (user) | 194,773,913 |
| `redox_users` | `jackpot51` (user), `MggMuggins` (user) | 214,613,751 |
| `serde_spanned` | `epage` (user), `github:toml-rs:maintainers` (team) | 548,820,551 |
| `tokio-macros` | `carllerche` (user), `github:tokio-rs:core` (team) | 767,335,875 |
| `toml` | `epage` (user), `github:toml-rs:maintainers` (team) | 878,458,522 |
| `toml_datetime` | `epage` (user), `github:toml-rs:maintainers` (team) | 755,423,581 |
| `toml_parser` | `epage` (user), `github:toml-rs:maintainers` (team) | 271,419,462 |
| `toml_writer` | `epage` (user), `github:toml-rs:maintainers` (team) | 178,562,168 |
| `winnow` | `epage` (user) | 864,594,820 |

## Labeled interpretation

Interpretation: the supported-target compiled delta is 10 package/version nodes rather than the 12-entry lockfile delta because the two Redox packages are never reached on the four supported release targets. This follows the mental model's distinction between supported-target policy scope and separately reviewable unsupported-target dependencies.

Interpretation: the closure growth comes from two new functional areas—platform-standard configuration path discovery and TOML parsing—plus one procedural macro for the async entry point. Direct access to streaming and deserialization APIs makes `futures-util` and `serde` explicit dependencies without growing the closure. Direct Tokio use grows the closure only through `tokio-macros`.

Interpretation: the delta causes no version churn, no new duplicate-version set, no custom build script, no disallowed source or license, and no advisory reported by the locally available RustSec data. These are bounded observations, not a security or maintenance guarantee.

Interpretation: crates.io shows one current user owner for `directories`, `dirs-sys`, and `option-ext`; the TOML-family packages generally show a user plus a team, while `winnow` shows the same user without that team record. Download totals establish widespread registry use but do not establish source quality, release provenance, or owner-account security.

## Owner decisions

The owner resolved the review's open questions on 2026-09-05:

- Accept the single-owner `directories` family (`directories`, `dirs-sys`, `option-ext`). Reasons given: versions are exactly pinned, updates require human review, and the family adds no build scripts.
- Do not review the Redox-only `redox_users` and `libredox` packages further, because they are unreachable on the supported release targets.
- Accept `tokio-macros`. Its build-time behavior is narrowly understood and the crate is owned by the Tokio team.
- Keep Serde's current features. `std` is required, and the resolved dependency graph does not grow from the direct declaration.

No open questions remain from this review.

## Deviations

The first exact `cargo deny --locked check` attempt could not acquire the advisory database lock because the sandbox exposed `<cargo-home>` read-only. The successful run added `--offline` and used the existing local advisory database: `cargo deny --offline --locked check`. This prevented an unauthorized advisory-database network fetch while performing the requested locked checks. No other deviation occurred.
