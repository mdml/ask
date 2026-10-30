# Rig 0.45.0 dependency-closure review

Review date: 2026-10-29

Status: evidence record for human review of the 0.42.0 to 0.45.0 migration. It extends the [Rig 0.42.0 dependency-closure review](rig-dependency-closure.md), which stays as the record for the adopted 0.42.0 closure.

## Scope and method

The migration moves `ask` from `rig-core =0.42.0` to `rig-core =0.45.0` and adds `rig-reqwest =0.45.0`. `cargo search` and the crates.io versions API showed 0.45.0 (published 2026-10-19) as the only 0.45 release and 0.46.0-rc.1 (2026-10-23) as a prerelease that was not considered. Rig 0.43 removed the bundled transport from `rig-core`, so a reqwest transport now comes from the separate `rig-reqwest` crate. The `reqwest` and `rustls` features of `rig-core` select it and keep their names.

The dependency declarations are `rig-core = { version = "=0.45.0", default-features = false, features = ["reqwest", "rustls"] }` and `rig-reqwest = { version = "=0.45.0", default-features = false, features = ["rustls"] }`. `rig-reqwest` is a direct dependency because `rig_reqwest::ReqwestClient::from(reqwest::Client)` is how `ask` hands Rig its redirect-refusing client; `rig-core` does not re-export it.

Resolution used Cargo 1.98.1, Rust 1.98.1, and cargo-deny 0.20.2 on aarch64 macOS. The base is `origin/milestone/context` at `ba2b678`, which contains the advisory sweep (tokio 1.53.3, rustls 0.23.46, rustls-webpki 0.103.16). Publication times are the crates.io version `created_at` values, queried on 2026-10-29.

## Lockfile delta

The lockfile went from 248 to 247 packages. All dates are UTC; every added or changed version is more than 48 hours old on the review date, so the quarantine in `SECURITY.md` is satisfied. The earliest permitted merge time for the newest crates (`uuid` 1.29.0, `webpki-root-certs` 1.0.11, both published 2026-10-23) has passed.

| Change | Package | Version | Published | Checksum (SHA-256) |
| --- | --- | --- | --- | --- |
| changed | `rig-core` | 0.42.0 to 0.45.0 | 2026-10-19T17:49:32Z | `0d07ae8e85415d83f620eb4f519d03126227178ac67971b08368c087ce8f9052` |
| added | `rig-http` | 0.45.0 | 2026-10-19T17:48:12Z | `817c761f9949151fd099ec78e2fbacf7174f24c066ced84c9d28807a6b9d4cee` |
| added | `rig-reqwest` | 0.45.0 | 2026-10-19T17:48:55Z | `c9f13e51c8cbe597ab4ed79f4a1211dab83ac3d9ceaa3826d69b5c770274b9fa` |
| added | `rig-tungstenite` | 0.45.0 | 2026-10-19T17:49:04Z | `4ba7906f696aee0da3d24825330bb5ff2cf2af86d8e308120f81d9cf581d1e30` |
| added | `uuid` | 1.29.0 | 2026-10-23T09:59:10Z | `4a0e2d11620c50695fe12bbbfde96b3477fe606e7284c22d505c12ce51214514` |
| added | `serde_path_to_error` | 0.1.20 | 2025-09-15T15:05:54Z | `10a9ff822e371bb5403e391ecd83e182e0e77ba7f6fe0160b795797109d1b457` |
| added | `httpdate` | 1.0.3 | 2023-08-13T10:55:54Z | `df3b46402a9d5adb4c86a0cf463f42e19994e3ee891101b1841f30a545cb49a9` |
| changed | `webpki-root-certs` | 1.0.9 to 1.0.11 | 2026-10-23T08:25:18Z | `e49d0e65abc00c142e8f9428da991da9f1a250c39be97459901eb12b9b49f62a` |
| removed | `as-any` | 0.3.2 | | `b0f477b951e452a0b6b4a10b53ccd569042d1d01729b519e02074a9c0958a063` |
| removed | `eventsource-stream` | 0.2.3 | | `74fef4569247a5f429d9156b9d0a2599914385dd189c539334c625d8099d90ab` |
| removed | `minimal-lexical` | 0.2.1 | | `68354c5c6bd36d73ff3feceb05efa59b6acb7626617f4962be322a825e61f79a` |
| removed | `nom` | 7.1.3 | | `d273983c5a657a70a3e8f2a01329822f3b8c8172b73826411a55751e404a0a4a` |
| removed | `pin-project` | 1.1.13 | | `2466b2336ed02bcdca6b294417127b90ec92038d1d5c4fbeac971a922e0e0924` |
| removed | `pin-project-internal` | 1.1.13 | | `c96395f0a926bc13b1c17622aaddda1ecb55d49c8f1bf9777e4d877800a43f8b` |
| removed | `tracing-futures` | 0.2.5 | | `97d095ae15e245a057c8e8451bab9b3ee1e1f68e9ba2b4fbc18d0ac5237835f2` |

Observed: the normalized package/version nodes of the current-host normal and build closure (`cargo tree --locked -e normal,build --prefix none`, annotations removed, unique lines) went from 166 to 167 including `ask`.

Observed: `rig-tungstenite` is locked as an optional dependency of `rig-core` but is absent from the current-host tree (`cargo tree --locked -e normal,build -i rig-tungstenite` prints nothing). `uuid`, `serde_path_to_error`, and `httpdate` are normal dependencies of `rig-core` (`cargo tree --locked -e normal,build -i <crate>`).

## Webpki root certificates and the advisory

Observed: at 1.0.9, `webpki-root-certs` carried RUSTSEC-2026-0383 (emSign ECC Root CA G3 distrust), which `cargo deny` reported once `rig-reqwest` was added. Version 1.0.11 clears it; 1.0.10 (2026-10-19) was the first fixed version. The lockfile pins 1.0.11.

Observed: before this migration `webpki-root-certs` 1.0.9 was a `wasm32`-specific dependency (see the 0.42.0 review) and outside the native closure. With `rig-reqwest`'s `rustls` feature it is a normal dependency of `rig-reqwest` on native targets: `cargo tree --locked -e normal -i webpki-root-certs` shows `rig-reqwest` as its only dependent. Its license is `CDLA-Permissive-2.0`. On 2026-10-29 the owner allowed that identifier in `deny.toml` with a comment naming the crate.

Observed: `rig-reqwest`'s default client builder falls back to the bundled Mozilla roots only when reqwest cannot load the system root store. `ask` does not use that default client: `provider::http_client()` builds its own `reqwest::Client` with redirects disabled and gives it to Rig through `ReqwestClient::from`. Interpretation: for `ask`, the bundled roots are compiled in but not used on any path the repository exercises.

## cargo-deny

`cargo deny --locked check` on the final tree: `advisories ok, bans ok, licenses ok, sources ok`. Duplicate-version warnings (`base64` 0.22.1 and 0.23.1, `core-foundation` 0.9.4 and 0.10.1, `syn` 2.0.119 and 3.0.5) are warnings under `multiple-versions = "warn"`.

## TLS backend and the AWS-LC decision

[ADR 0001](../decisions/0001-tls-backend-aws-lc.md) accepts the AWS-LC native build surface. Verified on the final lockfile:

- `reqwest` 0.13.5 resolves to the same feature set as in the 0.42.0 review: `__rustls`, `__rustls-aws-lc-rs`, `__tls`, `charset`, `http2`, `json`, `multipart`, `rustls`, `stream`, `system-proxy` (`cargo tree --locked -e normal -f '{p} {f}' -p reqwest`).
- `cargo tree --locked -e normal -i aws-lc-rs` shows `aws-lc-rs` 1.18.1 under `rustls` 0.23.46, reached from `hyper-rustls`, `reqwest`, and `rustls-platform-verifier`. `aws-lc-sys` stays at 0.45.0 in the lockfile.
- `native-tls` and `openssl` do not appear in the closure. `ring` does not appear in the current-host normal tree.
- The root store used by `ask`'s own client is the platform verifier (`rustls-platform-verifier` 0.7.0, reached from `reqwest`), as before.

Conclusion: the ADR still holds. The TLS backend, the AWS-LC native build surface, and the platform root store are unchanged. The one addition is the bundled-root crate described above, which the ADR does not mention and which does not change the decision. A clean-build time was not re-measured, so the ADR's 30.04-second figure is not refreshed here.

## Gemini candidate-free prompt feedback

The 0.42.0 rationale for the direct Gemini adapter was that Rig dropped a prompt-feedback refusal, and its usage, when the response had no candidates. Verified by reading `rig-core` 0.45.0 `providers/gemini/streaming.rs`: the decoder stores `usageMetadata`, then returns a provider error for a `promptFeedback` block (`blocked_prompt_error`) before it reads candidates. A candidate-free block is therefore no longer dropped; it becomes a failed stream, not a refusal ending that carries the usage.

The sentences in `src/provider.rs`, `docs/reference/providers.md`, and `docs/development/beta-delivery.md` were updated to say that. Not established: this was confirmed by reading source only. No test drives a real Rig Gemini stream through a prompt-feedback block, and the adapter's own `provider_proof` fixtures were not rerun against Rig's Gemini path. Whether the adapter can now be removed is a product decision; this change keeps it.

## Wire-behaviour change found during the migration

Observed: Rig 0.45 shapes history for the wire in `completion/history.rs::adapt`. It drops an empty assistant turn and merges adjacent user turns, so a thread with an empty recorded answer sent `[user "q1q2"]` instead of two user turns. `ask` now replays an empty or whitespace-only answer as the assistant text `[no answer]` (`NO_ANSWER_MARKER` in `src/provider.rs`) and keeps the recorded answer empty. The owner chose that marker on 2026-10-29. `continue_proof::empty_answers_are_complete_turns` asserts the wire shape through the fake provider.

## Not established

- Build scripts and procedural macros: established by the promotion security review of 2026-10-30 (PR 109) from the cached crate sources: none of `rig-core`, `rig-http`, `rig-reqwest`, `rig-tungstenite`, `uuid`, `serde_path_to_error`, `httpdate`, or `webpki-root-certs` 1.0.11 has a `build.rs` or declares a procedural macro. The same review confirmed the crates.io `created_at` and checksum values in the table above, and that `rig-core`'s only filesystem I/O is in its OAuth token store, which the configs `ask` uses (`OpenAIConfig::new`, `AnthropicConfig::new`, `GeminiConfig::new`) were not traced to reach.
- crates.io owner rosters, download counts, and the GitHub provenance of the `v0.45.0` tag were not examined.
- The `websocket` feature is not enabled and was not exercised.
