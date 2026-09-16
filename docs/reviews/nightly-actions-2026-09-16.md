# Nightly action dependency reassessment

Reassessed 2026-09-16 UTC for the [nightly release workflow](../../.github/workflows/nightly-release.yml) at `a822664d98fe969d5fa93a79a12e629086729dad`, the head of [PR #39](https://github.com/mdml/ask/pull/39). This record supplements rather than rewrites the [2026-09-15 disposition](nightly-actions-2026-09-15.md). Its acceptance still expires after **2026-09-29 UTC**, as enforced by `ACTION_REVIEW_DEADLINE`; this reassessment grants no extension or new exception and adds no dependency.

The source reassessment examined `main` at `ba658d4474dd6fb651ec69519e84fb59d24f0ca9`, where the nightly workflow still used install-action 2.87.8. The current workflow uses install-action 2.87.12 at `3f74d7c16a4242f1c95561e98edc25d36adb4375`. That pin received a separate dependency review and satisfied the 48-hour quarantine in [PR #25](https://github.com/mdml/ask/pull/25), then reached `main` through the independently reviewed frozen promotion in [PR #36](https://github.com/mdml/ask/pull/36). PR #39 reuses that exact pin in `nightly-release.yml`; it does not select the later 2.87.13 release.

## Pins and source identity

| Action | Exact pin | Version | GitHub-generated source archive SHA-256 |
| --- | --- | --- | --- |
| actions/checkout | `3d3c42e5aac5ba805825da76410c181273ba90b1` | 7.0.1 | `b59292069298c7be5ffd9c636431a229faff70ece00e0c24fd31baeb7b309fa3` |
| dtolnay/rust-toolchain | `4360b52568e2003a75bf9bc1d59f33a8e3fc893c` | branch-generated commit; workflow requests 1.97.1 | `d784ad40542954c4f6fa3739d6bf5601ff0ce388c54b90cbc045d3252f1b7f15` |
| taiki-e/install-action | `3f74d7c16a4242f1c95561e98edc25d36adb4375` | 2.87.12 | `ae012315b956fcc2badee7cb5b967bf919fccb3da09d5936b72972b57c9f3c59` |
| actions/upload-artifact | `043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` | 7.0.1 | `d14fb1cada435a236a66b448fbb370cd126564c2c2d6cb52abd14d20bcbb9748` |
| actions/download-artifact | `3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c` | 8.0.1 | `e31d826b0515c93e5eb0862ebb944bcbb57daa3c575b771163631f7e9583574a` |
| actions/attest-build-provenance | `4d101475d8b20a2381f78447822ac1eab6504dd8` | 4.2.2 | `c0b84cbc0820f13789b830b36fc4d105fa97960a6658de4a219d57e1d3aa0f24` |
| actions/attest, nested by the provenance wrapper | `508db95dd578ae2727ebd6217d5ba78e4fbda05d` | 4.2.1 | `1aebc2d5d397d3cb5026787cb8c8c8484563d4373c9b1ef14da4375de0b0a253` |

The source reassessment resolved the release tags to the exact commits, identified the shipped JavaScript entrypoints, and hashed their lockfiles and bundles. An independent security reviewer re-fetched the exact-pin archives, verified their SHA identities and lockfile closures, checked all 60 advisory IDs described below, corrected the attestation closure's production/dev classification, and independently traced the workflow principals and reachable inputs. The archive hashes above were fetched again while preparing this record. GitHub-generated archive hashes are transport observations rather than signed publisher provenance.

The rust-toolchain commit remains fetchable and selects Rust 1.97.1 explicitly, although the upstream generated `stable` branch has moved and the pin is no longer its ancestor. Install-action 2.87.12 was published 2026-09-12T11:43:48Z, so its quarantine ended 2026-09-14T11:43:48Z before PR #25 merged. PR #25 records review of the unchanged runtime entrypoints and requested tool manifests, searches for relevant advisories and upstream reports, and the successful hosted checks. No exception was used.

## Closure and advisory disposition

The reassessment parsed every package entry in the pinned upload, download, and nested attest lockfiles: 654, 649, and 703 name/version pairs respectively, including development packages. An OSV npm query on 2026-09-16 returned 60 distinct advisory IDs and no `MAL-` IDs. No returned advisory was published after 2026-09-15. Lockfile membership alone does not establish that an API ships or is reachable.

The attestation correction is material: **undici 6.25.0 is in the production closure through `@actions/github` and `@actions/http-client`; undici 8.9.0 is development-only.**

### Development-only IDs

The following 25 IDs apply only to lockfile entries marked development-only, and corresponding package/API markers were absent from the shipped bundles. They are not shipped or reachable: GHSA-2qvq-rjwj-gvw9, GHSA-2w6w-674q-4c4q, GHSA-3mfm-83xf-c92r, GHSA-442j-39wm-28r2, GHSA-7rx3-28cr-v5wh, GHSA-9cx6-37pm-9jff, GHSA-xhpv-hc6g-r9c6, GHSA-xjpj-3mr7-gcpf, GHSA-395f-4hp3-45gv, GHSA-w7jw-789q-3m8p, GHSA-25h7-pfq9-p65f, GHSA-rf6f-7fwh-wjgh, GHSA-3v7f-55p6-f55p, GHSA-c2c7-rcm5-vvqj, GHSA-2883-xcg3-v3hh, GHSA-52cp-r559-cp3m, GHSA-5p4m-2wfm-xmqj, GHSA-h67p-54hq-rp68, GHSA-4x5r-pxfx-6jf8, GHSA-p498-v437-472g, GHSA-2g4f-4pwh-qvx6, GHSA-w5vr-8v7q-w6rv, GHSA-73wf-gq98-2v4g, GHSA-c83g-rgw3-j3cx, and GHSA-7w5x-hrqm-74c2. Development copies of brace-expansion and minimatch are covered separately from the shipped copies below.

### Production XML and builder IDs

| IDs | Reachable input and disposition |
| --- | --- |
| [GHSA-8gc5-j5rx-235r](https://github.com/advisories/GHSA-8gc5-j5rx-235r) | Upload can parse an Azure HTTPS error response through fast-xml-parser 5.4.1. The response body comes from the trusted Azure service rather than archive content. This is reachable only through a malicious or compromised service response and remains a bounded accepted risk. Download's 5.3.4 parser is uncalled. |
| [GHSA-m7jm-9gc2-mpf2](https://github.com/advisories/GHSA-m7jm-9gc2-mpf2), [GHSA-jmr7-xgp7-cmfj](https://github.com/advisories/GHSA-jmr7-xgp7-cmfj) | Download streams the signed artifact URL through the Actions HTTP client and ZIP stream; it does not call the bundled Azure XML parser. Not reachable. |
| [GHSA-gh4j-gqv2-49f6](https://github.com/advisories/GHSA-gh4j-gqv2-49f6), [GHSA-5wm8-gmm8-39j9](https://github.com/advisories/GHSA-5wm8-gmm8-39j9) | Upload's XML serializer receives SDK-generated base64 block identifiers, does not receive comment or CDATA nodes, and retains entity processing. Download does not call the builder. No attacker-controlled builder input; not reachable. |
| [GHSA-jp2q-39xq-3w4g](https://github.com/advisories/GHSA-jp2q-39xq-3w4g) | The Azure parser options do not set either affected limit to zero. Required precondition absent; not reachable. |
| [GHSA-fj3w-jwp8-x2g3](https://github.com/advisories/GHSA-fj3w-jwp8-x2g3) | Download does not call the Azure XML builder, and the serializer does not enable `preserveOrder`. Not reachable. |

### Production pattern and utility IDs

| IDs | Reachable input and disposition |
| --- | --- |
| [GHSA-23c5-xmqv-rm74](https://github.com/advisories/GHSA-23c5-xmqv-rm74), [GHSA-3ppc-4f35-3m26](https://github.com/advisories/GHSA-3ppc-4f35-3m26), [GHSA-7r86-cg39-jmmj](https://github.com/advisories/GHSA-7r86-cg39-jmmj), [GHSA-7h2j-956f-4vf2](https://github.com/advisories/GHSA-7h2j-956f-4vf2) | Download constructs minimatch only when `pattern` is supplied without `name`. All eight downloads use exact workflow-owned names and omit pattern, cross-run, cross-repository, and token inputs. Not reachable. |
| [GHSA-rgw5-rvv9-x895](https://github.com/advisories/GHSA-rgw5-rvv9-x895), [GHSA-3jxr-9vmj-r5cp](https://github.com/advisories/GHSA-3jxr-9vmj-r5cp), [GHSA-f886-m6hf-6m8v](https://github.com/advisories/GHSA-f886-m6hf-6m8v), [GHSA-mh99-v99m-4gvg](https://github.com/advisories/GHSA-mh99-v99m-4gvg), [GHSA-jxxr-4gwj-5jf2](https://github.com/advisories/GHSA-jxxr-4gwj-5jf2) | Upload and attestation glob only finite workflow-owned literal paths without braces. Download does not glob, and cacache uses internal cache paths. No attacker-selected pattern; not reachable. |
| [GHSA-r5fr-rjxr-66jc](https://github.com/advisories/GHSA-r5fr-rjxr-66jc), [GHSA-f23m-r3pf-42rh](https://github.com/advisories/GHSA-f23m-r3pf-42rh) | The affected lodash template and property-path mutation APIs are absent from the shipped helper call paths. Not reachable. |
| [GHSA-jfc7-64v2-mr8c](https://github.com/advisories/GHSA-jfc7-64v2-mr8c) | Attestation signs with a fixed ASCII in-toto payload type. The non-ASCII payload-type mutation condition is absent, and verification is performed by consumers outside this JavaScript signing library. Not reachable in the signing path. |
| [GHSA-8cw4-87c7-c6xx](https://github.com/advisories/GHSA-8cw4-87c7-c6xx) | Attestation parses the workflow-owned subject list with `columns: false`; the affected grouped named-column mode is disabled. Not reachable. |
| [GHSA-22jq-vg5j-6vgg](https://github.com/advisories/GHSA-22jq-vg5j-6vgg), [GHSA-4xrf-jv44-h6hh](https://github.com/advisories/GHSA-4xrf-jv44-h6hh), [GHSA-mwp4-54f8-5fhr](https://github.com/advisories/GHSA-mwp4-54f8-5fhr) | The affected ip-address package is behind SOCKS proxy selection. GitHub-hosted release jobs configure no SOCKS proxy and make no trust decision from user-supplied addresses. Not reachable under the workflow environment. |

### Production undici IDs

| IDs | Reachable input and disposition |
| --- | --- |
| [GHSA-35p6-xmwp-9g52](https://github.com/advisories/GHSA-35p6-xmwp-9g52), [GHSA-2mjp-6q6p-2qxm](https://github.com/advisories/GHSA-2mjp-6q6p-2qxm), [GHSA-m8rv-5g2x-5cg5](https://github.com/advisories/GHSA-m8rv-5g2x-5cg5) | The nested attest action reaches undici through Octokit calls to GitHub attestation services. It sends JSON string bodies rather than attacker-controlled Blob-like MIME values. HTTP response behavior still crosses the trusted GitHub HTTPS boundary; queue poisoning or framing requires a malicious or compromised upstream. Bounded accepted risk. |
| [GHSA-4992-7rv2-5pvq](https://github.com/advisories/GHSA-4992-7rv2-5pvq), [GHSA-8xcm-r25x-g524](https://github.com/advisories/GHSA-8xcm-r25x-g524) | No caller supplies the affected upgrade option or constructs the affected retry interceptor/agent. Not reachable. |
| [GHSA-g8m3-5g58-fq7m](https://github.com/advisories/GHSA-g8m3-5g58-fq7m), [GHSA-p88m-4jfj-68fv](https://github.com/advisories/GHSA-p88m-4jfj-68fv), [GHSA-v3r7-h72x-cjcm](https://github.com/advisories/GHSA-v3r7-h72x-cjcm) | The actions do not parse, construct, or forward cookie values. Not reachable. |
| [GHSA-f269-vfmq-vjvj](https://github.com/advisories/GHSA-f269-vfmq-vjvj), [GHSA-v9p9-hfj2-hcw8](https://github.com/advisories/GHSA-v9p9-hfj2-hcw8), [GHSA-vrm6-8vpv-qv8q](https://github.com/advisories/GHSA-vrm6-8vpv-qv8q), [GHSA-vxpw-j846-p89q](https://github.com/advisories/GHSA-vxpw-j846-p89q) | The actions create no WebSocket and use no WebSocket endpoint. Not reachable. |

The action-level [GHSA-cxww-7g56-2vh6](https://github.com/advisories/GHSA-cxww-7g56-2vh6) affects download-artifact 4.0.0 through 4.1.2, not the pinned 8.0.1.

## Current workflow boundary

PR #39 adds a `prepare` step that supplies the repository `GITHUB_TOKEN` only to checked-in `scripts/nightly-release.py`, under the workflow's top-level `contents: read` permission. The helper invokes only GitHub REST GET requests through `gh api`: paginated release listings and an exact tag-ref lookup. It accepts only nightly tags matching the strict project pattern, exact 40-character commit targets, the complete five-asset inventory in uploaded state, and a tag ref that resolves to the same commit. Twenty full pages fail closed. JSON values are parsed as data and are not interpolated into a shell command. Responses can cause publication to proceed, skip, or fail; they cannot grant write permission or execute downloaded content. The `repair` input can request same-source publication, but all later verification, build, attestation, and draft-verification gates still run.

The token principal is the workflow run for this repository. The step runs only for protected `main` in the canonical repository on a schedule or manual dispatch, after checkout of the immutable event SHA with persisted credentials disabled. The step does not expose the token to third-party actions. Its remaining risks are GitHub API availability/integrity, a compromised protected source revision, or a defect in the checked-in parser. PR #39's independent security review reported no blockers for this boundary; code and documentation reviews also reported no blockers after fixes. That review covers the PR head, not this new prose.

The remaining release boundary is unchanged: build jobs have read-only contents access; attestation alone receives `id-token: write` and `attestations: write`; publication alone receives `contents: write`; artifact inputs are exact same-run names and fixed paths; build packaging materializes only the validated `ask` member for the offline executable proof, while attestation and publication validate bounded downloaded archive members without materializing or executing them; uploaded asset inventory, size, digest, and source identity are checked before a draft becomes public. The install action runs in `verify-full` and installs the two exact requested tool versions. Its pinned code and the upstream release service remain executable supply-chain trust boundaries.

## Compromise search and residual risk

Primary-source checks covered the repositories' published security advisories, the GitHub Advisory Database for the pinned action repositories, OSV's GitHub Actions records, upstream issues created since 2026-08-01 using compromise, malicious, backdoor, hijack, supply-chain, security, and exfiltration terms, and the published ChainDrop IOC package list. They found no report naming a pinned action or an exact production package/version as compromised. The ChainDrop comparison found no exact lockfile match. The one relevant watched report, actions/checkout issue [#2573](https://github.com/actions/checkout/issues/2573), concerns a later checkout following a symlink planted by an earlier checkout; every release job performs one checkout without `path` or `repository`, so its prerequisite is absent.

The broader web search sampled three queries and secondary reports about 2026 Actions and npm compromises. It was not an exhaustive enumeration of Mini Shai-Hulud packages. The assessment relies on the dated primary-source queries, the published ChainDrop IOC list, and OSV's absence of `MAL-` matches; those sources can be incomplete or delayed. It therefore does not establish that no unreported compromise exists.

Residual accepted risk through 2026-09-29 consists of malicious or compromised GitHub, Azure, Sigstore, action, tool-release, or archive-delivery infrastructure; the bounded response-side XML and HTTP advisory paths above; the read token's visibility to protected checked-in preparation code; and the possibility that advisory or compromise data was incomplete at the assessment time. The rust-toolchain branch-divergence observation and checkout #2573 remain watch items. No stable release path exists, so this record does not satisfy the separate stable-candidate checks in `SECURITY.md`.

## Verification limits and disposition

The source reassessment ran exact-pin resolution, closure parsing, the 60-ID OSV query, call-site inspection, compromise searches, workflow inspection, and an offline zizmor 1.28.0 scan with no findings and ten default-persona suppressions. The independent security review verified archive identities, closure classification, all 60 advisory dispositions, and principal reachability. PR #25 and PR #36 provide the public quarantine, review, hosted-check, and frozen-promotion evidence for install-action 2.87.12. PR #39 records successful `just verify-full` on its candidate and independent security, code, and documentation reviews.

This documentation change did not rerun the full build, hosted workflow, dynamic action execution, or a live-provider check. PR #39 merged into the beta umbrella on 2026-09-16 after all five required checks passed; promotion and scheduled unchanged-source skipping on `main` remain unverified. No independent review of this new writing is claimed.

**Disposition:** no new security blocker was found. The current action set remains within the existing time-bounded acceptance through 2026-09-29 UTC, with the production advisory dispositions and residual risks above. A later date requires a separately authorized review and code change; this reassessment does not provide one.
