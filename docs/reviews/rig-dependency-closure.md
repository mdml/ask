# Rig 0.42.0 dependency-closure review

Review date: 2026-09-04

Status: evidence record for human review; this record neither adopts Rig nor recommends adoption or rejection.

## Scope and method

This review examines the crates.io release `rig-core` 0.42.0 as a prospective, exactly pinned implementation dependency of `ask`. `cargo search rig-core --limit 1` reported 0.42.0 as the latest release on 2026-09-04. Resolution used Cargo 1.97.1 and Rust 1.97.1 on `x86_64-unknown-linux-gnu`; policy checking used cargo-deny 0.20.2. The temporary dependency and lockfile changes were reverted after evidence collection.

Observed feature mapping from the published `rig-core` manifest and source:

- OpenAI, Anthropic, Gemini, and Rig's OpenAI-compatible provider abstraction have no `rig-core` feature names; their modules are compiled unconditionally.
- The `http_client` module and the `reqwest` dependency are unconditional. The `reqwest` feature does not gate the client; it adds Reqwest's `charset`, `http2`, and `system-proxy` features. The `rustls` and `native-tls` features select HTTPS implementations.
- HTTP/SSE streaming is unconditional through Reqwest's always-enabled `stream` feature and the unconditional `eventsource-stream` dependency. The `websocket` feature aliases `websocket-rustls` and adds `tokio-tungstenite` for OpenAI Responses websocket mode; `websocket-native-tls` is the alternative native-TLS form.
- No general MCP feature or MCP client dependency is declared by `rig-core` 0.42.0. Some provider wire types mention provider-hosted MCP toolsets, but that is not Rig's MCP client integration.
- The default feature set is `reqwest`, `derive`, and `rustls`. `derive` adds the optional `rig-derive` procedural-macro crate and is not required for the provider transports reviewed here.

The minimal set was tested with `rig-core = { version = "=0.42.0", default-features = false, features = ["reqwest", "rustls"] }`. The all-provider-capabilities set added `websocket` because it is the only optional feature tied to a reviewed provider capability: `features = ["reqwest", "rustls", "websocket"]`. There are no OpenAI, Anthropic, Gemini, OpenAI-compatible, or MCP feature flags to add.

The raw commands were `cargo generate-lockfile`; `cargo tree -e normal --locked`; `cargo tree -e build --locked`; `cargo tree -e normal,build --locked --duplicates`; `cargo tree -e normal,build --locked --prefix none --format "{p} {l}" | sort -u`; `cargo deny --locked check`; `cargo tree -e normal,build --locked --prefix none | sort -u | wc -l`; `cargo vendor --locked <temporary-directory>` followed by `du -sh`; filesystem inspection of each current-host package's custom-build target; and `cargo metadata --locked --format-version 1` filtered to current-host package/version nodes whose target kind contains `proc-macro`. The all-provider-capabilities tree and count repeated the corresponding tree/count commands after enabling `websocket`.

Evidence is under [`evidence/rig-0.42.0/`](evidence/rig-0.42.0/). Filesystem paths in the evidence files were normalized before committing: the review worktree is written as `<repo-root>` and the Cargo home directory as `<cargo-home>`; no other content was altered. `tree-normal.txt`, `tree-build.txt`, `duplicates.txt`, `licenses.txt`, `cargo-deny.txt`, and `tree-all-providers.txt` are direct command outputs. `cargo-info.txt` and `cargo-search.txt` preserve release and feature discovery; `build-scripts.txt`, `proc-macros.txt`, `licenses-deny.tsv`, `counts-and-size.txt`, and `tool-versions.txt` preserve derived inventories and measurements; the two GitHub JSON files preserve the public repository and tag metadata used below.

## Closure summary

| Feature set | Requested count command | Normalized package/version nodes | Third-party nodes | Vendored locked-resolution size |
| --- | ---: | ---: | ---: | ---: |
| Minimal: `reqwest,rustls` | 183 rendered unique lines | 144 including `ask` | 143 | 297M |
| All provider capabilities: `reqwest,rustls,websocket` | 198 rendered unique lines | 156 including `ask` | 155 | 297M |

Observed: the contract's exact count command counts distinct rendered lines, so a package can appear both with and without Cargo's `(*)` marker. Removing `(*)` and `(proc-macro)` annotations produces the normalized package/version counts shown separately. `Cargo.lock` had 228 entries including `ask`; Cargo reported locking 227 packages.

Observed: enabling `websocket` adds 12 normalized package/version nodes: `data-encoding` 2.11.1, `getrandom` 0.3.4, `ppv-lite86` 0.2.21, `rand` 0.9.5, `rand_chacha` 0.9.0, `rand_core` 0.9.5, `sha1` 0.10.7, `tokio-tungstenite` 0.29.0, `tungstenite` 0.29.0, `webpki-roots` 0.26.11 and 1.0.9, and `zerocopy` 0.8.56. The normalized delta is +12; the exact rendered-line count delta is +15.

Observed: `cargo vendor --locked` vendors the complete locked resolution, including optional and target-specific packages, rather than only current-host active nodes. Both feature sets produce the same lockfile, so the one measured 297M vendor directory applies to both. The temporary directory and Cargo's generated configuration suggestion were deleted, and no `.cargo/config.toml` was created.

Interpretation: the 297M measurement is a reproducible packaging-footprint measure of this lockfile, not an installed-binary-size estimate. The largest conspicuous unpacked package was `aws-lc-sys` 0.45.0 at 69M; the unpacked `rig-core` package was 5.1M.

## Build scripts and proc-macros

The minimal current-host normal/build closure has 18 custom-build targets and 12 procedural-macro crates. Seventeen custom-build targets use a file named `build.rs`; `aws-lc-sys` declares the nonstandard `builder/main.rs`. “Well-known” below is an interpretation of ecosystem role and project provenance, not an audit result or a substitute for maintainer review.

### Custom-build targets

| Crate | Observed build action | Well-known? |
| --- | --- | --- |
| `aws-lc-rs` 1.18.1 | Validates mutually exclusive FIPS features, selects the sys crate, forwards its include/link metadata, and exposes test cfgs. | Yes; the established AWS-LC Rust wrapper. |
| `aws-lc-sys` 0.45.0 | Discovers a system AWS-LC or compiles the bundled C/assembly library using CMake/`cc`/NASM paths, optionally generates bindings, probes compilers, and emits native link metadata. | Yes; AWS-maintained, but it is the closure's most consequential native build surface. |
| `generic-array` 0.14.7 | Checks the Rust version and enables `relaxed_coherence` on Rust 1.41 or newer. | Yes; a longstanding RustCrypto ecosystem dependency. |
| `httparse` 1.10.1 | Runs `rustc --version`, inspects target features and environment switches, and enables supported SIMD cfgs. | Yes; a longstanding HTTP parser used by Hyper. |
| `icu_normalizer_data` 2.3.0 | Detects `ICU4X_DATA_DIR` and enables the custom-data cfg. | Yes; an official ICU4X component. |
| `icu_properties_data` 2.3.0 | Detects `ICU4X_DATA_DIR` and enables the custom-data cfg. | Yes; an official ICU4X component. |
| `libc` 0.2.189 | Probes Rust/compiler and target OS, ABI, architecture, and selected environment controls to emit platform-specific cfgs. | Yes; foundational Rust ecosystem crate. |
| `mime_guess` 2.0.5 | Generates extension-to-MIME and optional reverse lookup tables in `OUT_DIR`. | Yes; established MIME utility. |
| `num-traits` 0.2.19 | Uses `autocfg` to compile-probe `f64::total_cmp` and emits `has_total_cmp`. | Yes; foundational Rust numeric crate. |
| `proc-macro2` 1.0.107 | Checks the compiler version and compile-probes stable/nightly procedural-macro span APIs, writing temporary probe artifacts under `OUT_DIR`. | Yes; foundational procedural-macro infrastructure. |
| `quote` 1.0.47 | Checks the compiler version and configures diagnostic-namespace compatibility. | Yes; foundational procedural-macro infrastructure. |
| `ref-cast` 1.0.27 | Generates a versioned private module in `OUT_DIR` and configures compatibility from the compiler version. | Specialized rather than broadly recognizable; maintained in the established dtolnay crate family. |
| `rustls` 0.23.43 | Enables the nightly `read_buf` cfg only when that feature and compiler channel permit it; otherwise the script is a no-op. | Yes; widely used Rust TLS implementation. |
| `serde` 1.0.229 | Generates a versioned private module and emits compiler-version compatibility cfgs. | Yes; foundational serialization crate. |
| `serde_core` 1.0.229 | Generates a versioned private module and emits compiler/target compatibility cfgs for atomics and core APIs. | Yes; foundational Serde component. |
| `serde_json` 1.0.151 | Selects 32- or 64-bit arithmetic limbs from target architecture and pointer width. | Yes; foundational JSON crate. |
| `thiserror` 2.0.20 | Generates a versioned private module, compile-probes generic member access, and emits compiler compatibility cfgs. | Yes; widely used error-derive crate. |
| `zmij` 1.0.23 | Checks compiler version and optimization level to configure float-conversion implementation choices. | Specialized and less broadly recognizable; it is used by `serde_json` and authored in the established dtolnay crate family. |

### Procedural-macro crates

| Crate | Observed macro action | Well-known? |
| --- | --- | --- |
| `async-stream-impl` 0.3.6 | Expands the internal forms of `stream!` and `try_stream!`. | Yes; implementation crate for the established `async-stream` package. |
| `displaydoc` 0.2.7 | Derives `Display` from doc-comment format strings. | Yes; established and used in the ICU4X stack. |
| `futures-macro` 0.3.34 | Expands Futures join, try-join, select, stream-select, and async-test macros. | Yes; official Futures project component. |
| `pin-project-internal` 1.1.13 | Expands pin projections and pinned-drop support. | Yes; implementation crate for the established `pin-project` package. |
| `ref-cast-impl` 1.0.27 | Derives and validates transparent reference casts. | Specialized rather than broadly recognizable; implementation crate for `ref-cast`. |
| `schemars_derive` 1.2.2 | Derives JSON Schema implementations, including `repr` handling. | Yes; implementation crate for the established `schemars` package. |
| `serde_derive` 1.0.229 | Derives `Serialize` and `Deserialize`. | Yes; foundational Serde component. |
| `thiserror-impl` 2.0.20 | Derives `Error`, including display, source, and conversion behavior. | Yes; implementation crate for the widely used `thiserror` package. |
| `tracing-attributes` 0.1.31 | Expands `#[instrument]` tracing spans around functions. | Yes; official Tokio tracing component. |
| `yoke-derive` 0.8.2 | Derives ICU4X `Yokeable` implementations. | Yes within ICU4X; ecosystem-specific outside it. |
| `zerofrom-derive` 0.1.7 | Derives ICU4X `ZeroFrom` conversions. | Yes within ICU4X; ecosystem-specific outside it. |
| `zerovec-derive` 0.11.6 | Derives `ULE`/`VarULE` and expands zero-copy vector representation attributes. | Yes within ICU4X; ecosystem-specific outside it. |

Observed: `rig-derive` is absent because both reviewed sets disable defaults and omit `derive`. Enabling Rig's default features would add that procedural-macro crate.

## Licenses

Observed license identifiers in cargo-deny's locked graph were Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-3-Clause, CC0-1.0, CDLA-Permissive-2.0, ISC, LGPL-2.1-or-later, MIT, MIT-0, Unicode-3.0, and Unlicense. `licenses-deny.tsv` maps every package to the identifiers cargo-deny detected; `licenses.txt` preserves the contract's exact current-host `cargo tree` license expressions.

The identifiers outside `deny.toml`'s allowlist are CDLA-Permissive-2.0 (`webpki-root-certs` 1.0.9), LGPL-2.1-or-later (`r-efi` 6.0.0), MIT-0 (`aws-lc-sys` 0.45.0 and `dunce` 1.0.5), and Unlicense (`memchr` 2.8.3, `same-file` 1.0.6, `walkdir` 2.5.0, and `winapi-util` 0.1.11). Cargo-deny can satisfy the latter three identifiers through an allowed alternative in each crate's license expression. CDLA-Permissive-2.0 has no allowed alternative and is the rejection.

Observed: `webpki-root-certs` is a `wasm32`-specific dependency of `rustls-platform-verifier`; it is locked and evaluated by the unfiltered cargo-deny graph but is not in the current Linux tree or either supported native release-target family. `deny.toml`'s allowed BSD-2-Clause, MPL-2.0, and Zlib licenses were not encountered, producing non-failing warnings.

`cargo deny --locked check` exited with status 4. Its result line was: `advisories ok, bans ok, licenses FAILED, sources ok`.

## Advisories

Observed: cargo-deny reported `advisories ok`. The output contains no RUSTSEC identifiers.

## Duplicate versions

Observed in the minimal current-host `cargo tree --duplicates` output: `syn` occurs at 2.0.119 and 3.0.5. Cargo-deny additionally reported `core-foundation` 0.9.4 and 0.10.1 because its graph includes Apple-target dependencies. Enabling `websocket` adds a current-host duplicate pair, `webpki-roots` 0.26.11 and 1.0.9; `syn` remains duplicated.

Interpretation: the `syn` split follows procedural macros that have not all moved to the same major version. The `core-foundation` split comes from `system-configuration` versus the Rustls platform verifier/security-framework path. The `webpki-roots` 0.26 compatibility package in the Tokio-Tungstenite path depends on `webpki-roots` 1.0, producing the two-version pair.

## Unusual sources or observations

Observed: every locked external package source is `registry+https://github.com/rust-lang/crates.io-index`; there are no git dependencies. Cargo-deny reported `sources ok` and `bans ok`, so it found neither a disallowed source nor a wildcard dependency under the unchanged policy.

Observed: provider selection cannot reduce `rig-core`'s compiled provider modules because provider integrations are unconditional. Feature selection only changes transport, derive, document, media, middleware, and related optional facilities.

Observed: selecting `rustls` uses Reqwest 0.13's AWS-LC-backed Rustls path. This pulls `aws-lc-rs` and `aws-lc-sys`; the latter contains a large bundled native cryptography implementation and a build program capable of compiler, CMake, assembly, system-library, and optional binding-generation work. No build scripts were executed during this read-and-report review.

Observed: the public GitHub repository metadata on 2026-09-04 showed the repository as active and unarchived, with 8,522 stars and 952 forks. The `v0.42.0` tag is an annotated tag created by `github-actions[bot]` on 2026-08-17, points to commit `d5a34986a1ad57f1e9c5984b82f8d7438ffc717e`, and GitHub reports the tag as unsigned.

Not established: crates.io download counts and owner rosters are not present in Cargo's registry-index or crate-tarball metadata. The network boundary for this review did not authorize crates.io API queries, and Cargo manifest `authors` fields are not maintainer rosters. This review therefore makes no claim that any crate has few downloads or a single maintainer.

## Open questions for the human

- Should the eventual dependency policy evaluate all Cargo target-specific edges, as the current cargo-deny invocation does, or only `ask`'s supported macOS and Linux release targets? This determines whether the wasm-only CDLA-Permissive-2.0 dependency is in policy scope.
- If all-target evaluation remains in scope, should CDLA-Permissive-2.0 be allowed, or should the TLS feature/dependency path be changed? This record does not make that licensing decision.
- Does `ask` need OpenAI Responses websocket transport initially? Omitting `websocket` removes 12 normalized current-host package/version nodes; ordinary HTTP/SSE streaming remains available.
- Which Rig package or integration should provide the mental model's future MCP boundary? `rig-core` 0.42.0 has no general MCP feature or client, so any separate MCP package needs its own closure review before use.
- Is the AWS-LC native build and distribution surface acceptable for `ask`'s supported macOS/Linux architectures, or should the human request a separate TLS-backend/build-reproducibility comparison?
- Should a later review be authorized to query crates.io ownership/download metadata and inspect release provenance more deeply? Those questions were intentionally left unasserted here.
