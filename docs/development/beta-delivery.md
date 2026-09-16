# Tool-less beta delivery tracker

Internal delivery plan for the approved tool-less beta. Product scope, exclusions, and acceptance criteria are owned outside this repository; this tracker records work packets, evidence, dependencies, and completion conditions only. Update it in the same change that lands or reshapes a packet.

## Implemented baseline (`ba658d4`, 2026-09-16)

| Area | State at baseline |
|:--|:--|
| Commands | `new`/`n` (and bare `ask`), `reply`/`r`, `init`/`i`, `configure`/`c` `check`/`apply`. No `thread`, `switch`, `stats`, or `doctor`. |
| Providers | One `kind`, `openai-compatible`, via Rig's OpenAI Chat Completions client. `init` asks for free-text provider name, base URL, and credential variable name; no supported-provider menu or defaults. |
| Query input/output | Stdin composition, multiline prompt, stdout/stderr contract, partial turns, early pipe closure, and record-failure exit implemented and proven. The PTY proof submits with a single EOF after complete lines; the partial-line double Ctrl-D path is documented but not proven. |
| Storage | SQLite schema v1: threads with profile snapshots, turns, text-free statistics, provider health, current thread; one transaction per query; rollback journal, `synchronous = FULL`, 1000 ms busy timeout. No retention settings or expiry. No proof of many concurrent writers beyond one held reply racing one `new`. |
| Paths | `ASK_HOME` selects configuration and data; no cache directory is resolved. |
| Release | Nightly prereleases from `main`: four native archives, `SHA256SUMS`, attestations, mise GitHub-backend install. Stable workflow, branch, releases, and Homebrew formula are prepared in P4 but not live until owner authorization; see [stable releases](../guides/stable-releases.md). `stable` deletion ruleset applied. |
| Live checks | None in the repository. Nightly workflow runs advisories and dependency freshness only. |
| Monitoring | SQLite native monitoring was approved at beta kickoff; not implemented. The `sqlite-readiness` workflow and probes remain. |

The owner's own nightly install integration and credential isolation live outside this repository; their absence here is not a gap.

## Packet progress

| Packet | Landed on this branch | Still open |
|:--|:--|:--|
| P1 | Provider kinds `openai` (Responses), `anthropic`, `openrouter`, and `openai-compatible` through Rig provider modules, plus a scoped direct Gemini GenerateContent streaming adapter because Rig 0.42 drops candidate-free prompt feedback and usage; redirects disabled; `configure` validation of the kinds; profile `max_output_tokens` with the output-limit truncation policy; schema version 3 output-token snapshot column with version 1 and recall version 2 migration; percent-encoded Gemini model path and credential, and encoded-credential redaction; OpenRouter in-band stream errors as partial turns; `provider_proof` wire-format fixtures; `init` provider menu and defaults, credential-supply instructions and injection recipe, `--profile`/`-p` on new queries, offline `help`/`version` and `--help`/`-h`/`--version`/`-V`. | None for the packet scope; integration with P2 is verified through schema version 3. |
| P2 | `thread`/`t`, `switch`/`s`, `stats`; configurable whole-thread expiry with highwater thread ids; schema version 2 history expiry with version 1 migration; `recall_proof` coverage including expiry, captured-thread races, and invalid-configuration fallback; durable concurrent-writer proof; partial-line double Ctrl-D PTY proof in `query_proof`. | None for the packet scope; integration with P1 is verified through schema version 3. |

On 2026-09-16 the beta umbrella gained durable concurrent-writer proofs through PR #35. The implemented-baseline table above remains a snapshot of `ba658d4`.

## Work packets

Order: P1 → P2 → P3. P4–P7 run in parallel wherever they do not touch the same files.

| ID | Packet | Depends on |
|:--|:--|:--|
| P1 | Provider and setup compatibility: provider kinds for OpenAI, Anthropic, Gemini, OpenRouter, and custom OpenAI-compatible endpoints; `init` selection menu supplying endpoint and credential-variable defaults; `configure` validation of the new kinds; fake-provider fixtures per wire format; credential-supply instructions in `init` and README, including an optional external injection recipe; explicit profile selection and offline help/version inspection (flag spelling is a repository implementation choice). | Dependency delta review if Rig provider modules pull new crates (reviewed closure says no provider feature flags). |
| P2 | Recall and expiry: `thread`/`t` marking partial turns incomplete, `switch`/`s` (interactive and `<id>`), `stats`; retention configuration and whole-thread expiry in history-writing commands only; expired current thread on `reply`; statistics and provider health survive expiry, including cleared-thread counts; durable concurrent-writer proof; partial-line double Ctrl-D PTY proof. | P1 config schema settled (both edit `config.rs`/`validate.rs`). |
| P3 | Diagnostics: `doctor`/`d` offline checks (strict config validation, resolve and display all platform-standard paths including cache, honor `ASK_HOME` for each, without creating them, SQLite without side effects, credential presence only, last observed healthy; distinguish configuration/environment errors, fail nonzero, and report unsafe-check limitations); `--live` and `--live --all`. | P1 (provider targets), P2 (schema and retention). |
| P4 | Stable release preparation (see below). | None for workflow shape; first stable push needs P1–P3, P5 live checks, owner nightly evaluation and release authorization. |
| P5 | Live-provider checks: opt-in credentialed query/reply checks for the four named providers; reports only, never merges. | P1; owner notice that live credentials are ready. |
| P6 | Report-only [native SQLite monitoring](sqlite-monitoring.md) and production identity, compile-option, damaged-file, and busy-timeout checks are implemented. The separate readiness workflow remains pending four-target real-release evidence and a complete probe-equivalence review. | Retirement requires the adoption record's remaining evidence. |
| P7 | Nightly action disposition [reassessed on 2026-09-16](../reviews/nightly-actions-2026-09-16.md) with no new blocker; acceptance and the enforced deadline remain unchanged and expire after 2026-09-29 UTC. | A later deadline requires a separately authorized review and code change; nightly release preparation fails after the current deadline. |
| P8 | Nightly unchanged-source skipping: scheduled and ordinary manual runs skip tagging, building, and publishing when `main` equals the most recent successfully published nightly source, judged from published-release evidence rather than a tag or draft; a failed unpublished attempt retries; a same-source republish needs the `repair` dispatch input. Implemented in `nightly-release.yml`, `scripts/nightly-release.py`, and the [nightly guide](../guides/nightly-releases.md). | None. |

## Proof and evidence matrix

Acceptance proofs run in this order through the real product boundary. Ordinary verification uses deterministic fake providers; installation exercises published artifacts, and the separate live checks use explicitly supplied credentials. Reuse existing test targets where practical.

| Proof | Existing evidence | Missing |
|:--|:--|:--|
| Install | Nightly packaging tests; offline packaged `configure check` | Repeat published-archive installation for the final candidate; stable and Homebrew installs |
| Configure | `configure_proof` (init menu/defaults, help/version offline, apply/check); retention validation in unit tests and `recall_proof` | None after P1 setup UX |
| Query | `query_proof`, including the partial-line double Ctrl-D PTY proof; `provider_proof` covers supported wire formats, usage, errors, limits, and refusals | None for deterministic provider coverage |
| Continue | `continue_proof`, including concurrent writers; `provider_proof` covers reply history for each supported kind | None for deterministic provider coverage |
| Recall | `recall_proof`: thread output, switching, statistics, expiry, cancellation, captured-thread races, invalid-configuration fallback, and schema migration | None after schema version 3 integration |
| Diagnose | None | `doctor` offline/side-effect-free, `--live`, `--live --all` against fake provider |
| Compose | `query_proof`/`continue_proof`; inspection stdout/stderr assertions in `recall_proof` | Recheck with diagnostics |
| Live | [2026-09-16 query/reply checks](../reviews/live-provider-checks-2026-09-16.md) passed for OpenAI, Anthropic, Gemini, and OpenRouter | Scheduled change-aware checks and complete installed-workflow evidence |

Record personally run results in PRs; this table lists proof targets, not results.

## Stable and Homebrew preparation (P4)

Proposed implementation route reusing pinned actions and packaging code where suitable; review the changed stable trust boundary before accepting reuse:

1. Prepare the candidate from a verified recent main nightly plus any reviewed cherry-picked fixes; the Cargo version determines its stable tag. Add a stable trigger (push to `stable`) that reuses the nightly verify, native build/test, package, attest, and draft-verify-publish jobs, with a non-prerelease tag derived from the Cargo version; refuse an existing tag.
2. Before pushing `stable`, the managing agent runs the full gate, advisory check, and supervised attack search required by `SECURITY.md`, recording their results, owner authorization, and exact candidate. After the push, the workflow repeats the full gate and advisory check on that revision before publication; it cannot substitute for the pre-push checks.
3. Keep enforcement in the workflow: publication runs only after the full gate and advisory job pass. The `stable` deletion ruleset is already applied; any further ruleset change is reviewed separately.
4. Homebrew: use the existing public `mdml/homebrew-tap` repository (confirmed empty on 2026-09-16). Prepare a reviewed formula update as part of each authorized stable release, without adding a cross-repository automation credential. The formula selects the four stable archives by URL and SHA-256 from `SHA256SUMS`, updated after publication; no new build path.
5. mise stable install: the existing GitHub backend without `prerelease=true`; see [stable releases](../guides/stable-releases.md).

## External prerequisites

- The owner provisioned live-check credentials on 2026-09-16. Ordinary development and gates need no provider credentials; opt-in checks use process-scoped injection.
- Any new dependency or changed acceptance boundary discovered during the spike returns to the owner before adoption.

## Completion conditions

- P1–P3 merged through an umbrella and promoted with named proofs passing and three recorded reviews.
- The seven proofs plus live checks have evidence, including a fresh-user walkthrough showing setup and first query succeed using only shipped instructions.
- A nightly containing P1–P3 installed and evaluated by the owner.
- P4 workflow merged and formula prepared; security checks recorded against the exact candidate; owner authorizes the stable push. After publication, update the formula in the tap and verify stable installation through mise and Homebrew.
- P6 native monitoring running; P7 disposition current; P8 skip behavior observed on a scheduled run against an unchanged `main`.
