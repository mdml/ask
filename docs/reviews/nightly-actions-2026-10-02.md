# Release action dependency review, 2026-10-02

Reviewed 2026-10-02 UTC for the [nightly release workflow](../../.github/workflows/nightly-release.yml) and the [stable release workflow](../../.github/workflows/stable-release.yml) on branch `chore/action-review` starting at `a4436b83a955a8ee61ff1d90944a2b8cdb50d9a3`, with install-action updated to 2.87.22 as proposed by Dependabot [PR #66](https://github.com/mdml/ask/pull/66). This record supplements rather than rewrites the [2026-09-15 disposition](nightly-actions-2026-09-15.md) and the [2026-09-16 reassessment](nightly-actions-2026-09-16.md). The owner authorized a fresh review and a new expiry on 2026-10-02 after the previous acceptance expired on 2026-09-29. This review sets `ACTION_REVIEW_DEADLINE` to **2026-11-01 UTC**. It adds no dependency, exception, or audit suppression.

## Pins and source identity

| Action | Exact pin | Version | GitHub-generated source archive SHA-256 |
| --- | --- | --- | --- |
| actions/checkout | `3d3c42e5aac5ba805825da76410c181273ba90b1` | 7.0.1 | `b59292069298c7be5ffd9c636431a229faff70ece00e0c24fd31baeb7b309fa3` |
| dtolnay/rust-toolchain | `4360b52568e2003a75bf9bc1d59f33a8e3fc893c` | branch-generated commit; workflows request 1.97.1 | `d784ad40542954c4f6fa3739d6bf5601ff0ce388c54b90cbc045d3252f1b7f15` |
| taiki-e/install-action | `83ac0ad63c0167e6f06796fab0fce28db1bf3db0` | 2.87.22 | `86c581c143fe79416e24606b943c86a176a90ca6e41660e5278be0b6fff2132a` |
| actions/upload-artifact | `043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` | 7.0.1 | `d14fb1cada435a236a66b448fbb370cd126564c2c2d6cb52abd14d20bcbb9748` |
| actions/download-artifact | `3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c` | 8.0.1 | `e31d826b0515c93e5eb0862ebb944bcbb57daa3c575b771163631f7e9583574a` |
| actions/attest-build-provenance | `4d101475d8b20a2381f78447822ac1eab6504dd8` | 4.2.2 | `c0b84cbc0820f13789b830b36fc4d105fa97960a6658de4a219d57e1d3aa0f24` |
| actions/attest, nested by the provenance wrapper | `508db95dd578ae2727ebd6217d5ba78e4fbda05d` | 4.2.1 | `1aebc2d5d397d3cb5026787cb8c8c8484563d4373c9b1ef14da4375de0b0a253` |

Every tag was resolved through the GitHub REST API on 2026-10-02 to a lightweight ref pointing directly at the pinned commit, and each archive was fetched from `codeload.github.com` at the exact commit and hashed. Six of the seven archive hashes are byte-for-byte those recorded on 2026-09-16; only install-action changed. The provenance wrapper's `action.yml` still pins `actions/attest` to `508db95dd578ae2727ebd6217d5ba78e4fbda05d`. GitHub-generated archive hashes are transport observations rather than signed publisher provenance.

Install-action 2.87.22 was published 2026-09-29T16:24:41Z, so its 48-hour quarantine elapsed 2026-10-01T16:24:41Z, before this review. The intermediate pin 2.87.18 (`dfae9bf3d6f6c6f20ef4ebb3486c01a51341ff12`, published 2026-09-22T14:29:17Z, archive `541e01cb0e0e848d04aecc83f7745893669f2e70230378be410f293584f4a5a7`) reached the umbrella through Dependabot commit `0000e04`; this review replaces it. The rust-toolchain pin remains fetchable; its commit is now 1 ahead and 9 behind the upstream generated `stable` branch, so it is still not that branch's ancestor.

### Install-action 2.87.12 to 2.87.22

The extracted 2.87.12, 2.87.18, and 2.87.22 trees differ only in `CHANGELOG.md` and 30 files under `manifests/`. The runtime entrypoints, `action.yml`, `main.sh`, and everything under `tools/`, are byte-identical across all three versions. The two requested manifests, `manifests/cargo-llvm-cov.json` and `manifests/cargo-deny.json`, are also byte-identical; they retain entries for cargo-llvm-cov 0.8.7 and cargo-deny 0.20.2 with per-platform SHA-256 values, including `x86_64_linux_musl` for the `ubuntu-24.04` gate job, so the cargo-binstall fallback is not selected. The changed manifests belong to other tools, among them `cargo-binstall.json`, which the action reads only on the fallback path. The changelog entries for 2.87.18 through 2.87.22 record only `@latest` version updates for other tools.

## Closure and advisory disposition

The review parsed every package entry in the pinned upload, download, and nested attest lockfiles: 654, 649, and 703 distinct name/version pairs respectively, including development packages, the same counts as on 2026-09-16. An OSV npm batch query on 2026-10-02 over the 1,001 distinct pairs returned **79 distinct advisory IDs and no `MAL-` IDs**. Sixty are the package advisories dispositioned on 2026-09-16. None of those sixty was modified after 2026-09-10 or withdrawn. The other **19 IDs were published from 2026-09-28 through 2026-09-29** and are treated from scratch below. No advisory published before 2026-09-16 was modified after it. The action-level [GHSA-cxww-7g56-2vh6](https://github.com/advisories/GHSA-cxww-7g56-2vh6) still affects download-artifact 4.0.0 through 4.1.2 only.

### Carried-forward IDs

The 2026-09-16 dispositions for the sixty earlier IDs (25 development-only, 35 production) are carried forward without restating them. That reuse rests on evidence re-verified today: the upload, download, provenance, and attest archives are byte-identical to those reviewed then, so their lockfiles, shipped bundles, and call sites are unchanged; the advisory records are unmodified; and the workflow inputs those dispositions rely on are unchanged, as traced in the boundary section. Lockfile classification re-confirmed that undici 6.25.0 is production in attest through `@actions/github` and `@actions/http-client`, and undici 8.9.0 is a root `devDependency` only.

### New development-only IDs

These nine IDs apply only to lockfile entries marked `dev`: [GHSA-r3ph-w7gj-g6xm](https://github.com/advisories/GHSA-r3ph-w7gj-g6xm) (js-yaml 5.2.2), [GHSA-253c-mchw-3w2r](https://github.com/advisories/GHSA-253c-mchw-3w2r) (markdown-it 14.3.0), both reached only through `markdownlint-cli`, and [GHSA-rx4f-c7p8-82vq](https://github.com/advisories/GHSA-rx4f-c7p8-82vq), [GHSA-8436-99hf-9mmv](https://github.com/advisories/GHSA-8436-99hf-9mmv), [GHSA-w293-vg96-wgc3](https://github.com/advisories/GHSA-w293-vg96-wgc3), [GHSA-2gqq-gqf2-x968](https://github.com/advisories/GHSA-2gqq-gqf2-x968), [GHSA-2jfj-6hjv-fm6j](https://github.com/advisories/GHSA-2jfj-6hjv-fm6j), [GHSA-3xpg-4rpp-hhhm](https://github.com/advisories/GHSA-3xpg-4rpp-hhhm), and [GHSA-pmjh-fq2x-6v4x](https://github.com/advisories/GHSA-pmjh-fq2x-6v4x) (undici 8.9.0). The shipped attest bundle contains no `MarkdownIt`, `linkify`, `maxTotalMergeKeys`, `WebSocketStream`, or cache-interceptor markers; its `decompress` occurrences are undici 6.x WebSocket permessage-deflate code. Not shipped or reachable.

### New production IDs

| IDs | Reachable input and disposition |
| --- | --- |
| [GHSA-2vr4-cq9g-pvrc](https://github.com/advisories/GHSA-2vr4-cq9g-pvrc), [GHSA-rpw4-54j3-4h4q](https://github.com/advisories/GHSA-rpw4-54j3-4h4q), [GHSA-j6r3-76f7-8jcv](https://github.com/advisories/GHSA-j6r3-76f7-8jcv) | ip-address 10.2.0 is production only in attest. In the shipped bundle, outside the library's own definitions, it is called only by the `socks` client helpers `ipv4ToInt32` and `ipToBuffer`, which convert addresses and never call `isLinkLocal`, `isPrivate`, `isInSubnet`, or `isHostInSubnet`. No code makes a trust decision from these classifiers. The SOCKS agent is selected only when a configured proxy URL uses a `socks` scheme; release jobs on GitHub-hosted runners configure no proxy. Not reachable. |
| [GHSA-h3mg-xc3c-68pw](https://github.com/advisories/GHSA-h3mg-xc3c-68pw) | The unbounded parse diagnostic requires a long attacker-supplied string. `ipToBuffer` constructs `Address4`/`Address6` only after `net.isIPv4`/`net.isIPv6` accepts the input, and only on the unused SOCKS path above. Not reachable. |
| [GHSA-3wwx-pv8p-q78v](https://github.com/advisories/GHSA-3wwx-pv8p-q78v), [GHSA-rfgv-xxqx-mfg5](https://github.com/advisories/GHSA-rfgv-xxqx-mfg5) | WebSocket client faults in undici; 3wwx affects only attest's 6.25.0, rfgv also upload and download's 6.23.0. No shipped bundle constructs a `WebSocket`, and the actions use no WebSocket endpoint, consistent with the 2026-09-16 WebSocket dispositions. Not reachable. |
| [GHSA-r53p-7pc4-xj5r](https://github.com/advisories/GHSA-r53p-7pc4-xj5r) | Response splitting requires undici's retry interceptor or `RetryAgent`. In all three bundles `RetryHandler` is constructed only inside undici's own `RetryAgent` and retry-interceptor definitions; no caller invokes `interceptors.retry` or constructs `RetryAgent`. The `retry` matches elsewhere are the async library's documentation and Azure retry strategies, which do not use undici's handler. The actions also do not forward responses downstream. Not reachable, as for GHSA-8xcm-r25x-g524. |
| [GHSA-6j4f-fj2g-mc7p](https://github.com/advisories/GHSA-6j4f-fj2g-mc7p), [GHSA-qhr7-859c-m2p7](https://github.com/advisories/GHSA-qhr7-859c-m2p7), [GHSA-q2hr-2g5m-vwhr](https://github.com/advisories/GHSA-q2hr-2g5m-vwhr) | Each brace-expansion DoS needs a large crafted pattern, from roughly 15 KB of nested or repeated braces upward. Upload paths and attestation subjects are short, finite, workflow-owned literals without braces; the only expression, `matrix.target`, comes from the fixed four-target matrix and is substituted before the action runs. All sixteen downloads across both workflows use exact names and omit `pattern`. No attacker-selected pattern; not reachable. |

## Workflow boundary

`git diff` from `a822664d98fe969d5fa93a79a12e629086729dad` (the 2026-09-16 head, which is not an ancestor of the rebase-merged umbrella) to the starting revision shows these boundary-relevant changes.

**Stable release workflow.** Commit `5226d9a` added [`stable-release.yml`](../../.github/workflows/stable-release.yml) and `scripts/stable-release.py`; the 2026-09-16 record predates any stable release path. The workflow runs on a push to protected `stable` in the canonical repository, with top-level `contents: read`. Its `prepare` job supplies `GITHUB_TOKEN` only to checked-in `scripts/stable-release.py`, which requires the canonical repository, `refs/heads/stable`, a protected ref, a push event, and a checkout matching the event SHA, enforces the action review deadline, and issues one GitHub REST GET (`git/matching-refs/tags/<tag>`) to refuse an existing stable tag. Its jobs, permissions, pinned actions, checkout options, fixed upload paths, and exact same-run downloads mirror the nightly workflow, with artifact names `stable-<target>`, an added `cargo deny --locked check advisories` step in `verify-full`, and publication as a non-prerelease marked latest. As in nightly, build jobs are read-only, attestation alone receives `id-token: write` and `attestations: write`, and publication alone receives `contents: write`.

**Release helper.** `scripts/nightly-release.py` now accepts the exact stable tag `v<Cargo version>` in `identity` and takes the expected prerelease state as a parameter in `verify_upload`. Neither change touches how downloaded archives are validated. In this change, nightly `prepare` enforces the deadline only when it would publish: on a changed source, when nothing has been published, or on a repair run. An unchanged source with no repair now skips with `publish=false` and a `::warning::` that the review has expired, instead of failing. When the deadline has passed, preparation now performs its existing read-only release listing and tag-ref lookups before failing. Those reads already ran on every in-deadline run, and the job's permissions are unchanged.

**Other workflows.** The install-action pin moved from 2.87.12 to 2.87.18 and, in this change, to 2.87.22 in all five workflows that use it. The separate `nightly.yml` dependency workflow (`contents: read`, cargo-deny only) gained a report-only native SQLite monitor step. That step runs checked-in code and adds no action or permission.

**Write permissions.** In both release workflows only two jobs hold write permissions. `attest` (`id-token: write`, `attestations: write`) runs actions/checkout, actions/download-artifact, and actions/attest-build-provenance with its nested actions/attest. `publish` (`contents: write`) runs actions/checkout and actions/download-artifact. No other third-party action runs in either job.

**Unchanged.** The nightly release workflow is otherwise unchanged: the same eight literal downloads, `digest-mismatch: error`, fixed upload paths, `persist-credentials: false` on every checkout, separated attestation and publication permissions, and draft-then-verify publication. The 2026-09-16 description of nightly `prepare` remains accurate apart from the deadline ordering above.

**Token exposure.** Two observations apply to both release workflows and are unchanged since 2026-09-16. First, install-action passes the `verify-full` job's read-only `GITHUB_TOKEN` to its own script as `DEFAULT_GITHUB_TOKEN` because `fallback` defaults to `cargo-binstall`. The script removes it from its own environment after reading it, which the action's source notes does not prevent another process from reading it through `/proc/*/environ`, and uses it only on the fallback path, which is not taken for these two tools. In this change both release workflows set `fallback: none` on the install step, so the action no longer receives the token there; the pull-request, per-commit, and dependency workflows keep the default. Second, the `verify-full` gate step receives `CS_ACCESS_TOKEN`. No action receives that secret as an input. Earlier steps in the same job, rust-toolchain and install-action, can modify the runner that the gate step later uses, so both remain part of that secret's executable trust boundary, as the previous record states for pinned action code in general.

## Compromise search and residual risk

Searches ran on 2026-10-02 against primary sources, with these results:

- Repository security advisories for all seven pinned repositories: only download-artifact's 2024 GHSA-cxww-7g56-2vh6, which does not affect 8.0.1.
- The GitHub Advisory Database, `actions` ecosystem, filtered by each repository: the same single record.
- OSV `GitHub Actions` ecosystem queries for each repository: the same single record.
- GitHub issue searches in each repository for issues created since 2026-09-16 with each of the terms compromise, malicious, backdoor, hijack, supply-chain, security, and exfiltration: zero results across 49 queries. Control queries confirmed the search returned issues for the same repositories and window. The seven issues opened since 2026-09-16 across checkout, attest, and upload-artifact were read by title. Only checkout [#2582](https://github.com/actions/checkout/issues/2582), a submodule URL rewrite surviving cleanup on persistent self-hosted runners, concerns behavior relevant to security. Release jobs use ephemeral GitHub-hosted runners, check out no submodules, and disable credential persistence, so its prerequisites are absent. Watch item [#2573](https://github.com/actions/checkout/issues/2573) remains open and unchanged since 2026-09-12, and its prerequisite, a second checkout with `path`, is still absent.
- A published ChainDrop and Shai-Hulud 2.0 indicator list, `compromised-packages.json` from the [Shai-Hulud-2.0-Detector](https://github.com/gensecaihq/Shai-Hulud-2.0-Detector) repository: version 2.2.0, last updated 2026-08-09, 1,241 packages, consolidated from vendor IOC sets. No exact name/version match against the 1,001 lockfile pairs. Name-only matches (keyv 4.5.4, flat-cache 4.0.1, file-entry-cache 8.0.0) are development-only and at versions other than the listed compromised ones. The 2026-09-16 record does not identify which ChainDrop list it used, so this may not be the same list.

Three web searches sampled ChainDrop indicator sources, GitHub Actions compromises reported in September 2026, and npm compromises naming closure packages. The September reports concern actions-cool actions re-enabled with malicious tags. Those actions are not used here, and SHA pinning is the stated mitigation. The npm results concern ordinary advisories already in the OSV results, not malicious publications.

None of these sources names a pinned action or an exact production package version as compromised. They can be incomplete or delayed, the indicator list predates 2026-08-10, and the web sampling was not exhaustive, so this review does not establish that no unreported compromise exists.

Residual accepted risk through 2026-11-01 consists of:

- malicious or compromised GitHub, Azure, Sigstore, action, tool-release, or archive-delivery infrastructure;
- the bounded response-side XML and HTTP advisory paths carried forward from 2026-09-16;
- the read token's visibility to protected checked-in preparation code and to install-action;
- `CS_ACCESS_TOKEN`'s exposure to code that earlier pinned actions can influence in the gate job;
- incomplete advisory or compromise data at review time.

The rust-toolchain branch divergence and checkout #2573 remain watch items. This record does not satisfy the separate per-candidate stable checks in `SECURITY.md`.

## Verification limits

Re-verified today: tag-to-commit resolution and release publication times for every pin; archive hashes; byte-level comparison of the install-action runtime and requested manifests across 2.87.12, 2.87.18, and 2.87.22; full lockfile closure parsing and production/dev classification; the 79-ID OSV query with publication and modification dates; call-site inspection in the shipped bundles for every new production ID; the workflow and release-script diff since `a822664`; and the compromise searches above.

Carried forward on the evidence of unchanged archive bytes and unmodified advisory records: the 2026-09-16 call-site analyses for the sixty earlier IDs, including XML, minimatch, lodash, csv-parse, Sigstore, and earlier undici and ip-address dispositions. Those analyses were not repeated line by line.

Not done:

- running any workflow or action, or executing any downloaded code;
- checking the tool release binaries that install-action downloads against their manifest hashes;
- a zizmor scan, because zizmor is not installed on the review host; the 2026-09-16 reassessment ran one, so this review covers less on that point;
- an npm audit-endpoint query.

Out of scope: `Swatinem/rust-cache`, pinned in `full.yml` and `per-commit.yml`, runs only in the pull-request and manually dispatched gate workflows, not in either release workflow, and is not reviewed here.

## Independent review

A second agent in a separate session reviewed this record on 2026-10-02 without changing files. It re-fetched every pin and archive hash, repeated the install-action comparison, re-ran the OSV query over 1,001 name/version pairs (79 IDs, no `MAL-` IDs, the same 19 new IDs with the same production/dev split), checked the new production dispositions against the shipped bundles, traced job permissions and secrets in both release workflows, and traced `prepare` for a changed source, a first publication, a repair run, and an API failure with the deadline expired. It reported no blockers and five notes. Dispositions: the write-permission paragraph and the out-of-scope sentence above were added; the token-exposure wording was corrected and `fallback: none` was set on the release workflows' install steps; the zizmor limit now states the regression from the previous review; and `scripts/nightly-release-test.py` gained a case for a malformed release lookup with the deadline expired. An attempt to run this review in a different agent harness was cancelled by that harness's sandbox before it produced findings.

The independent reviewer could not verify: the upstream-issue search queries beyond the issues opened since 2026-09-16, the indicator list's contents and the web searches, tool binary hashes, publication times for pins other than install-action, and the carried-forward 2026-09-16 call-site analyses.

**Disposition:** no blocker was found. All new production advisories are not reachable in these workflows, and the carried-forward dispositions and residual risks are unchanged in kind. The action set, with install-action at 2.87.22, is accepted through **2026-11-01 UTC**, enforced by `ACTION_REVIEW_DEADLINE`. Nightly preparation fails after that date only when it would publish, and stable preparation always fails after it.
