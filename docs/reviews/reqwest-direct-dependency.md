# Reqwest direct-dependency review

Review date: 2026-09-16

Status: evidence record for the direct `reqwest` dependency added by the provider-redirect fix. The owner approved `reqwest = { version = "=0.13.4", default-features = false }` as a direct dependency before implementation.

## Purpose

`ask` sends provider credentials and query content only to the provider endpoint the user selected. Rig 0.42.0 builds its OpenAI-compatible client on `reqwest::Client::default()`, which follows up to ten redirects. Before the fix, a fake provider answering `307` to a different origin on loopback received the full request body, including the prompt, at the redirect target; later hops in the redirect chain also carried the bearer credential. [`RigProvider::start`](../../src/provider.rs) now injects a `reqwest::Client` built with `reqwest::redirect::Policy::none()` through Rig's `ClientBuilder::http_client`, so a redirect response is returned to Rig as a non-success status and reported as a provider failure. [`provider_redirects_are_refused_without_forwarding_the_request`](../../tests/query_proof.rs) covers cross-origin `307` and `308` and same-origin `307` against the offline fake provider.

## Observed facts

The implementation worker made the following observations locally and offline on 2026-09-16 with cargo 1.98.1; the managing agent subsequently ran pinned Rust 1.97.1 verification and the publication check below.

- `Cargo.lock` already contained `reqwest` 0.13.4 from the crates.io registry, with checksum `219c5811de6525e5416c7d5d53bb656d3afdbc6c5af816e0802bcfa42dbdc1c3`, as a transitive dependency of `rig-core` 0.42.0 since commit `69f1c7e` (2026-09-04). The only lockfile change is the added `reqwest` edge in `ask`'s own dependency list; no package was added, removed, or changed version.
- The SHA-256 of the locally cached `reqwest-0.13.4.crate` equals the lockfile checksum.
- `cargo tree --locked --offline -e normal,build -f '{p} {f}'` for `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin`, compared before and after the change, differs only by the duplicate marker on the new `ask -> reqwest` edge. The resolved `reqwest` features are unchanged: `__rustls`, `__rustls-aws-lc-rs`, `__tls`, `charset`, `http2`, `json`, `multipart`, `rustls`, `stream`, and `system-proxy`, all activated by `rig-core`. The direct dependency enables no features.
- `cargo metadata` reports license `MIT OR Apache-2.0` and a single `lib` target: no build script and no procedural macro.
- `cargo deny --locked --offline check licenses bans sources` and `cargo deny --locked --offline check advisories` passed. The advisory check used the locally cached advisory database, which was not refreshed.

## Quarantine

The version was not updated. The managing-agent publication check below establishes that the existing version has also passed the required quarantine.

## Limits

- Only the OpenAI-compatible provider path that exists now is covered. Future provider kinds must build their HTTP clients the same way.
- The diagnostic includes the redirect response body as Rig reports it. The body comes from the endpoint the user selected; the credential is redacted from it, but other content is shown as received.
- Proxy behavior from Reqwest's `system-proxy` feature is unchanged by this fix.

## Managing-agent publication verification

On 2026-09-16 the managing agent queried the [crates.io version API](https://crates.io/api/v1/crates/reqwest/0.13.4): publication was 2026-05-25T17:12:48.317444Z, the version was not yanked, and SHA-256 was `219c5811de6525e5416c7d5d53bb656d3afdbc6c5af816e0802bcfa42dbdc1c3`, matching the lockfile. The 48-hour quarantine ended 2026-05-27T17:12:48.317444Z; no exception is needed.
