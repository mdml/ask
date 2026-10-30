# Release action dependency review, 2026-10-30

Reviewed 2026-10-30 UTC for the [nightly release workflow](../../.github/workflows/nightly-release.yml) and the [stable release workflow](../../.github/workflows/stable-release.yml) on branch `chore/action-review-2026-10-30`, cut from `origin/main` at `737ad4c`. This record supplements rather than rewrites the [2026-10-02 review](nightly-actions-2026-10-02.md), which in turn supplements the [2026-09-15 disposition](nightly-actions-2026-09-15.md) and the [2026-09-16 reassessment](nightly-actions-2026-09-16.md). The owner authorized a fresh review and a new expiry on 2026-10-30, two days before the 2026-10-02 acceptance expires. This review sets `ACTION_REVIEW_DEADLINE` to **2026-11-30 UTC**. It adds no dependency, exception, or audit suppression.

## What changed since 2026-10-02

Between the renewal commit `ce59967` and the starting revision, the only change to any workflow file is the four Dependabot pin bumps below; every changed line in `.github/workflows/` is a `uses:` line, and `scripts/nightly-release.py`, `scripts/stable-release.py`, and their tests are byte-identical. Permissions, job structure, `persist-credentials: false`, `fallback: none`, and the exact same-run downloads are therefore as described on 2026-10-02. Each bump was merged on 2026-10-29 after its own 48-hour quarantine, recorded on the pull request:

| Bump | Pull request | Release published | Quarantine recorded |
| --- | --- | --- | --- |
| taiki-e/install-action 2.87.22 to 2.87.29 | [#96](https://github.com/mdml/ask/pull/96) | 2026-10-16T16:10:45Z | 2026-10-29 |
| actions/download-artifact 8.0.1 to 8.0.2 | [#94](https://github.com/mdml/ask/pull/94) | 2026-10-07T10:09:51Z | 2026-10-29 |
| actions/upload-artifact 7.0.1 to 7.0.2 | [#91](https://github.com/mdml/ask/pull/91) | 2026-10-07T10:08:14Z | 2026-10-29 |
| Swatinem/rust-cache 2.9.2 to 2.9.3 | [#98](https://github.com/mdml/ask/pull/98) | 2026-10-20T13:47:43Z | 2026-10-29 |

Publication times were re-read from the GitHub releases API today and match. All four pins are older than 48 hours on the review date.

## Pins and source identity

Every `uses:` in the two release workflows, and the nested action, with the other workflows noted for completeness:

| Action | Exact pin | Version | Uses (release workflows) | GitHub-generated source archive SHA-256 |
| --- | --- | --- | --- | --- |
| actions/checkout | `3d3c42e5aac5ba805825da76410c181273ba90b1` | 7.0.1 | 5 + 5 | `b59292069298c7be5ffd9c636431a229faff70ece00e0c24fd31baeb7b309fa3` |
| dtolnay/rust-toolchain | `4360b52568e2003a75bf9bc1d59f33a8e3fc893c` | branch-generated commit; workflows request 1.97.1 | 2 + 2 | `d784ad40542954c4f6fa3739d6bf5601ff0ce388c54b90cbc045d3252f1b7f15` |
| taiki-e/install-action | `8cca0918f8471c33515202b73f56ab27f2bed283` | 2.87.29 | 1 + 1 | `9d7c27ba3886cb111a3f36a6ede2986584b24dacb152fa2cf625005f2b54f541` |
| actions/upload-artifact | `cf430e030ddbb5b0abf93d22962f4752f3646cd9` | 7.0.2 | 1 + 1 | `082ff3f390265f902386d7fc59c52d91fdd8e9d36d4bb5fdc94f44a70ec04b75` |
| actions/download-artifact | `9000827ccba6bdab643e8b6fd33ac0654aef8333` | 8.0.2 | 8 + 8 | `0469682bf9b16a2ec8fe730420bef575b829dd5c6ab89e37748a361bcad45358` |
| actions/attest-build-provenance | `4d101475d8b20a2381f78447822ac1eab6504dd8` | 4.2.2 | 1 + 1 | `c0b84cbc0820f13789b830b36fc4d105fa97960a6658de4a219d57e1d3aa0f24` |
| actions/attest, nested by the provenance wrapper | `508db95dd578ae2727ebd6217d5ba78e4fbda05d` | 4.2.1 | nested | `1aebc2d5d397d3cb5026787cb8c8c8484563d4373c9b1ef14da4375de0b0a253` |
| Swatinem/rust-cache (not in a release workflow) | `d1527c8f67ef9af357290475d4af4b0fab8f12e0` | 2.9.3 | 0; `full.yml` x2, `per-commit.yml` | `add01aec14f2bf6424238fb080d0148cbf51752ad699fa5cba78c82f0a9a4356` |

Tag-to-commit resolution used `gh api repos/<owner>/<repo>/git/ref/tags/<tag>` on 2026-10-30. Checkout v7.0.1, install-action v2.87.29, upload-artifact v7.0.2, download-artifact v8.0.2, attest-build-provenance v4.2.2, and attest v4.2.1 are lightweight tags pointing directly at the pinned commit. The rust-cache v2.9.3 tag is an annotated tag object (`2201cc584a476d1b508be977bbe4b7d1c7a52e9c`), which dereferences to the pinned commit. The wrapper's `action.yml` still pins `actions/attest` to `508db95dd578ae2727ebd6217d5ba78e4fbda05d`. Each archive was downloaded with `curl -fsSL` from `https://github.com/<owner>/<repo>/archive/<sha>.tar.gz` (which redirects to codeload) and hashed with `shasum -a 256`; nothing from an archive was executed. Archives were extracted only for text comparison.

The archive hashes for checkout, rust-toolchain, attest-build-provenance, and attest are byte-for-byte those recorded on 2026-10-02. I also re-fetched the previous pins of the four bumped actions (install-action 2.87.22 `83ac0ad6…`, upload-artifact 7.0.1 `043fb46d…`, download-artifact 8.0.1 `3e5f45b2…`) and reproduced their 2026-10-02 hashes exactly, so the hashing method matches the previous review. GitHub-generated archive hashes are transport observations rather than signed publisher provenance.

The rust-toolchain pin (committed 2026-08-05) remains fetchable. The upstream generated `stable` branch is now at `b725eda525362e7cf34af9a7243cac1e3f01a7ec` (2026-10-16); the pin is 1 commit ahead and 13 behind it, so it is still not an ancestor of that branch. Upstream issue [dtolnay/rust-toolchain#188](https://github.com/dtolnay/rust-toolchain/issues/188), opened 2026-10-27, notes that the action installs rustup through `curl | sh` without a checksum; the reporter calls it a false positive on GitHub-hosted images, which preinstall rustup. This is a property of the pinned action, unchanged by this review, and is recorded as a watch item.

### Source differences between the previous and current pins

For the three Node actions and install-action I extracted the previous and current archives and compared them. Neither the file lists nor the contents were executed.

- **upload-artifact 7.0.1 to 7.0.2.** Seven files differ: three license records, `package.json` and `package-lock.json` (`@actions/artifact` `^6.2.0` to `^6.3.1`), and the bundles `dist/upload/index.js` and `dist/merge/index.js` (the same 54 added and 7 removed lines in each). `action.yml` is unchanged.
- **download-artifact 8.0.1 to 8.0.2.** `dist/index.js`, `README.md`, `package.json`, `package-lock.json` (`@actions/artifact` `^6.2.1` to `^6.3.1`), and license records differ. The license records show `typescript` and protobuf code-generation packages removed from the dependency list and `json-with-bigint` added, and `@actions/http-client` moves from 4.0.0 to 4.0.1. `action.yml` is unchanged.
- **install-action 2.87.22 to 2.87.29.** The runtime entrypoints, `action.yml`, `main.sh`, and every file under `tools/` except `tools/codegen/` are byte-identical. The differences are `CHANGELOG.md`, the action's own three CI workflow files, `tools/codegen/` (Cargo.toml and `main.rs`, build-time manifest generation that the action never runs), and 32 files under `manifests/`. The requested manifests, `manifests/cargo-llvm-cov.json` and `manifests/cargo-deny.json`, are byte-identical, so the per-platform SHA-256 entries the release gate relies on are unchanged, and `fallback: none` keeps the cargo-binstall path unselected in both release workflows.
- **rust-cache 2.9.2 to 2.9.3.** Not in a release workflow; compared for completeness. Changes are dependency bumps in `package.json` (`@actions/cache` `^6.3.0`, `smol-toml`), regenerated bundles, and a small change to `src/cleanup.ts` and `src/config.ts`.

Network calls: I extracted every `http(s)://` host string from the shipped `dist/` bundles of upload-artifact, download-artifact, and rust-cache before and after. The host sets are unchanged for upload and download. rust-cache loses `http://site.com` (a documentation string) and gains `https://bugzilla.mozilla.org` (a URL in a bundled library's comments). This is a string-level check; it does not prove that no new code path calls an existing host.

## Closure and advisory disposition

I parsed every package entry in the pinned upload, download, and nested attest lockfiles: 650, 646, and 706 distinct name/version pairs respectively (993 distinct overall, including development packages), compared with 654, 649, 703, and 1,001 on 2026-10-02. An OSV npm batch query on 2026-10-30 over those 993 pairs returned **98 distinct advisory IDs and no `MAL-` IDs**, against 79 on 2026-10-02. Production or development status was taken from the `dev` flag in each lockfile entry. The action-level [GHSA-cxww-7g56-2vh6](https://github.com/advisories/GHSA-cxww-7g56-2vh6) still affects download-artifact 4.0.0 through 4.1.2 only.

Eighty-one of the 98 IDs were published before 2026-10-02 and 17 on or after it. I did not keep the full 2026-10-02 ID list, only its 19 newest IDs and the 60 before them, so I cannot reconcile the 81 against 79: two more IDs pre-date 2026-10-02 than that record counted, among them GHSA-gcqx-w48m-w2h4 (deepmerge 4.3.1, development only, published 2026-09-18, modified 2026-10-27). That is a gap in this review's accounting rather than evidence of reachability.

### Carried-forward IDs

The 81 pre-2026-10-02 dispositions are carried forward from the 2026-09-16 and 2026-10-02 records. That reuse rests on evidence from today: the checkout, attest-build-provenance, and attest archives are byte-identical to those reviewed on 2026-10-02, the upload and download bundles changed only as described above, and the workflow inputs those dispositions rely on are unchanged. Two of the 81 were modified after 2026-10-02: the lodash 4.17.23 advisories [GHSA-f23m-r3pf-42rh](https://github.com/advisories/GHSA-f23m-r3pf-42rh) (`_.unset` and `_.omit` prototype pollution) and [GHSA-r5fr-rjxr-66jc](https://github.com/advisories/GHSA-r5fr-rjxr-66jc) (`_.template` imports key names), both modified 2026-10-17. I did not obtain the diff of what the record changed. Their affected and fixed versions are unchanged in form (fixed in 4.18.0), and the shipped upload, download, and attest bundles contain no `_.template`, `lodash.template`, or `_.unset`/`_.omit` call on the lodash package: the `omit(` matches are a local helper in the proxy agent code and the `lodash` strings are bundled helper-library identifiers and comments. Not reachable on that evidence.

### New development-only IDs

These eleven IDs, published from 2026-10-05 through 2026-10-27, apply only to lockfile entries marked `dev`: [GHSA-238p-pmpm-9mq7](https://github.com/advisories/GHSA-238p-pmpm-9mq7) (katex), [GHSA-8r5x-fm3f-whwj](https://github.com/advisories/GHSA-8r5x-fm3f-whwj), [GHSA-p8wg-vrv2-v86f](https://github.com/advisories/GHSA-p8wg-vrv2-v86f), [GHSA-xw65-4hp5-5hc7](https://github.com/advisories/GHSA-xw65-4hp5-5hc7) (handlebars 4.7.8), [GHSA-r4xh-jqrq-34v2](https://github.com/advisories/GHSA-r4xh-jqrq-34v2) (smol-toml 1.7.0), [GHSA-gfr2-rv44-r23c](https://github.com/advisories/GHSA-gfr2-rv44-r23c), [GHSA-r7fv-28h4-cvq7](https://github.com/advisories/GHSA-r7fv-28h4-cvq7), [GHSA-vfgh-jcmw-g37q](https://github.com/advisories/GHSA-vfgh-jcmw-g37q) (markdown-it 14.3.0), and [GHSA-4w7w-5xrc-9h26](https://github.com/advisories/GHSA-4w7w-5xrc-9h26), [GHSA-55wc-3pg8-jfw6](https://github.com/advisories/GHSA-55wc-3pg8-jfw6), [GHSA-f72v-6w45-q472](https://github.com/advisories/GHSA-f72v-6w45-q472) (undici 8.9.0, a root `devDependency` only). Development packages are not shipped in the action bundles. Not reachable.

### New production IDs

| IDs | Reachable input and disposition |
| --- | --- |
| [GHSA-2m5v-pch6-74hx](https://github.com/advisories/GHSA-2m5v-pch6-74hx) | undici drops TLS options passed to `Agent` (fixed in 6.29.1), weakening certificate validation. Affects undici 6.23.0 in upload and download. The actions construct undici's `ProxyAgent` only when a proxy URL is configured, and pass no custom TLS options to it; release jobs on GitHub-hosted runners configure no proxy. `rejectUnauthorized: false` appears only in `@actions/http-client`'s opt-in `ignoreSslError` path, which these actions do not enable. Not reachable. |
| [GHSA-4qv8-2p9j-2685](https://github.com/advisories/GHSA-4qv8-2p9j-2685) | Unbounded memory in the undici `EventSource` parser. The bundles contain undici's `EventSource` class definition but no caller constructs one, and no action uses a server-sent-events endpoint. Not reachable. |
| [GHSA-9fm3-97px-rjf6](https://github.com/advisories/GHSA-9fm3-97px-rjf6) | Unbounded PONG buffering in the undici WebSocket client. No bundle constructs a `WebSocket`, consistent with the earlier WebSocket dispositions. Not reachable. |
| [GHSA-qrxm-fqhp-jwvm](https://github.com/advisories/GHSA-qrxm-fqhp-jwvm), [GHSA-xjj6-f33q-929c](https://github.com/advisories/GHSA-xjj6-f33q-929c) | Denial of service through TLS 1.2 certificate-changing renegotiation and through excessive `content-encoding` values. Both require a malicious server response. The only peers are GitHub and Azure artifact endpoints, which are inside the infrastructure trust boundary already listed as residual risk, and the consequence is a failed step in an ephemeral job. Accepted as residual risk. |
| [GHSA-rxp7-6hxx-x8q5](https://github.com/advisories/GHSA-rxp7-6hxx-x8q5) | Quadratic-time `Address6` zone-identifier parsing in ip-address 10.2.0 (fixed in 10.7.4). The library is production only in attest and reached only through the SOCKS client helpers, which run only when a `socks` proxy URL is configured; none is. Not reachable, as for the other ip-address IDs on 2026-10-02. |

The 2026-10-02 dispositions for undici 6.x WebSocket, retry-interceptor, brace-expansion, and ip-address IDs rest on the same call sites, which I spot-checked again with string searches over the current bundles: no `new WebSocket`, `interceptors.retry`, `new RetryAgent`, or `renegotiat` match in any of the three, and `socks` appears only in the attest bundle, as before.

## Workflow boundary

`git diff ce59967 origin/main -- .github/workflows` shows only `uses:` line changes: the four pin bumps in `nightly-release.yml`, `stable-release.yml`, `full.yml`, `per-commit.yml`, and `nightly.yml`. Release scripts are unchanged. All boundary properties recorded on 2026-10-02 therefore hold:

- Only `attest` (`id-token: write`, `attestations: write`) and `publish` (`contents: write`) hold write permissions in either release workflow. `attest` runs actions/checkout, actions/download-artifact, and actions/attest-build-provenance with its nested actions/attest. `publish` runs actions/checkout and actions/download-artifact. No other third-party action runs in either job.
- Every checkout uses `persist-credentials: false`; downloads use exact names and omit `pattern`.
- Both install steps set `fallback: none`. The `verify-full` gate step still receives `CS_ACCESS_TOKEN`, and earlier steps in that job, rust-toolchain and install-action, remain part of that secret's executable trust boundary, as on 2026-10-02.

The workflow use counts in the pin table were taken with `grep` over `.github/workflows/` on the starting revision: checkout 15, download-artifact 16, rust-toolchain 9, install-action 5, upload-artifact 2, attest-build-provenance 2, rust-cache 3, across all workflows.

## Compromise search and residual risk

Searches ran on 2026-10-30 against primary sources:

- Repository security advisories (`repos/<repo>/security-advisories`) for all eight pinned repositories: only download-artifact's 2024 GHSA-cxww-7g56-2vh6, which does not affect 8.0.2.
- The GitHub Advisory Database, `actions` ecosystem, filtered by each repository: the same single record.
- GitHub issue searches (`is:issue created:>=2026-10-02`) in each repository for compromise, malicious, backdoor, hijack, supply-chain, security, and exfiltration: 56 queries, three hits. The control queries returned issues for the same window (4 for checkout, 1 for install-action). The hits are checkout [#2601](https://github.com/actions/checkout/issues/2601), a sparse-checkout `blob:none` filter persisting into later full checkouts on self-hosted runners, and dtolnay/rust-toolchain #188 (matching both "supply-chain" and "security"), described above. Release jobs use ephemeral GitHub-hosted runners, check out no sparse trees, and disable credential persistence, so the checkout prerequisites are absent. I read the titles of the other issues opened since 2026-10-02 in the pinned repositories (checkout #2596, #2597, #2600; upload-artifact #820, #824; download-artifact #491; attest #518; install-action #2029; rust-cache #391, #393; attest-build-provenance #869, #870); none concerns security of the pinned code, and several are unrelated or spam-like content.

I did not repeat the web searches or the ChainDrop and Shai-Hulud indicator-list comparison from 2026-10-02. None of the sources searched names a pinned action or an exact production package version as compromised. They can be incomplete or delayed, so this review does not establish that no unreported compromise exists.

Residual accepted risk through 2026-11-30 consists of:

- malicious or compromised GitHub, Azure, Sigstore, action, tool-release, or archive-delivery infrastructure, including the peer-side denial-of-service paths in the undici advisories above;
- the bounded response-side XML and HTTP advisory paths carried forward from 2026-09-16;
- the read token's visibility to protected checked-in preparation code and to install-action;
- `CS_ACCESS_TOKEN`'s exposure to code that earlier pinned actions can influence in the gate job;
- rust-toolchain's unchecked rustup bootstrap (watch item, issue #188);
- incomplete advisory or compromise data at review time.

The rust-toolchain branch divergence and checkout [#2573](https://github.com/actions/checkout/issues/2573) remain watch items. This record does not satisfy the separate per-candidate stable checks in `SECURITY.md`.

## Verification limits

Done today: tag-to-commit resolution and release publication times for every pin; archive hashes, including reproduction of the 2026-10-02 hashes for the previous pins; source comparison of upload-artifact, download-artifact, install-action, and rust-cache between their previous and current pins; a string-level host comparison of the shipped bundles; full lockfile closure parsing and production/development classification; the 98-ID OSV query with publication and modification dates; call-site string searches in the shipped bundles for each new production ID; the workflow and release-script diff since the 2026-10-02 renewal; and the advisory and issue searches above.

Not done:

- running any workflow or action, or executing any downloaded code;
- a line-by-line read of the bundled JavaScript diffs for upload-artifact and download-artifact (host strings and call-site searches only), or of the rust-cache source change;
- checking the tool release binaries that install-action downloads against their manifest hashes;
- a zizmor scan, because zizmor is not installed on the review host; the 2026-09-16 reassessment ran one, so this review covers less on that point, as the 2026-10-02 review did;
- re-running the 2026-10-02 web-search sampling or the indicator-list comparison;
- re-deriving the carried-forward 2026-09-16 call-site analyses line by line, or retrieving what changed in the two lodash records on 2026-10-17;
- reconciling the 81 pre-2026-10-02 IDs against the 79 counted on 2026-10-02;
- an npm audit-endpoint query;
- an independent second-agent review, which the 2026-10-02 review obtained.

**Disposition:** no blocker was found. The four bumped actions changed only dependency versions and rebuilt bundles with no new network hosts, install-action's runtime and the two manifests the gate relies on are unchanged, all new production advisories are either unreachable in these workflows or require a malicious response from GitHub or Azure infrastructure, and the carried-forward dispositions and residual risks are unchanged in kind. The action set is accepted through **2026-11-30 UTC**, enforced by `ACTION_REVIEW_DEADLINE`. Nightly preparation fails after that date only when it would publish, and stable preparation always fails after it.
