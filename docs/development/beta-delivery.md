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
| Release | Nightly prereleases from `main`: four native archives, `SHA256SUMS`, attestations, mise GitHub-backend install. No stable workflow, `stable` branch, Homebrew formula, or stable security-check automation. `stable` deletion ruleset applied. |
| Live checks | None in the repository. Nightly workflow runs advisories and dependency freshness only. |
| Monitoring | SQLite native monitoring was approved at beta kickoff; not implemented. The `sqlite-readiness` workflow and probes remain. |

The owner's own nightly install integration and credential isolation live outside this repository; their absence here is not a gap.

## Delivery progress

On 2026-09-16 the beta umbrella gained durable concurrent-writer proofs through PR #35. The partial-line double Ctrl-D proof is implemented in `query_proof`; the P2 list below describes packet scope, and these two items are complete. The implemented-baseline table above remains a snapshot of `ba658d4`.

## Work packets

Order: P1 → P2 → P3. P4–P7 run in parallel wherever they do not touch the same files.

| ID | Packet | Depends on |
|:--|:--|:--|
| P1 | Provider and setup compatibility: provider kinds for OpenAI, Anthropic, Gemini, OpenRouter, and custom OpenAI-compatible endpoints; `init` selection menu supplying endpoint and credential-variable defaults; `configure` validation of the new kinds; fake-provider fixtures per wire format; credential-supply instructions in `init` and README, including an optional external injection recipe; explicit profile selection and offline help/version inspection (flag spelling is a repository implementation choice). | Dependency delta review if Rig provider modules pull new crates (reviewed closure says no provider feature flags). |
| P2 | Recall and expiry: `thread`/`t` marking partial turns incomplete, `switch`/`s` (interactive and `<id>`), `stats`; retention configuration and whole-thread expiry in history-writing commands only; expired current thread on `reply`; statistics and provider health survive expiry, including cleared-thread counts; durable concurrent-writer proof; partial-line double Ctrl-D PTY proof. | P1 config schema settled (both edit `config.rs`/`validate.rs`). |
| P3 | Diagnostics: `doctor`/`d` offline checks (strict config validation, resolve and display all platform-standard paths including cache, honor `ASK_HOME` for each, without creating them, SQLite without side effects, credential presence only, last observed healthy; distinguish configuration/environment errors, fail nonzero, and report unsafe-check limitations); `--live` and `--live --all`. | P1 (provider targets), P2 (schema and retention). |
| P4 | Stable release preparation (see below). | None for workflow shape; first stable push needs P1–P3, P5 live checks, owner nightly evaluation and release authorization. |
| P5 | Live-provider checks: opt-in credentialed query/reply checks for the four named providers; reports only, never merges. | P1; owner notice that live credentials are ready. |
| P6 | Native SQLite monitoring in the existing nightly check, then migration of readiness probes into the gate and retirement of the separate workflow per the adoption record's criteria. | None. |
| P7 | Nightly action disposition renewal before it expires after 2026-09-29 UTC. | None; nightly release preparation fails without it. |

## Proof and evidence matrix

Acceptance proofs run in this order through the real product boundary. Ordinary verification uses deterministic fake providers; installation exercises published artifacts, and the separate live checks use explicitly supplied credentials. Reuse existing test targets where practical.

| Proof | Existing evidence | Missing |
|:--|:--|:--|
| Install | Nightly packaging tests; offline packaged `configure check` | Repeat published-archive installation for the final candidate; stable and Homebrew installs |
| Configure | `configure_proof` | Provider menu/defaults per supported provider; retention fields |
| Query | `query_proof`, including the partial-line double Ctrl-D PTY proof | Per-provider wire formats (streaming, usage, errors, rate limits, auth) |
| Continue | `continue_proof`, including concurrent writers | Per-provider reply history encoding |
| Recall | None | `thread`, `switch`, `stats`, expiry |
| Diagnose | None | `doctor` offline/side-effect-free, `--live`, `--live --all` against fake provider |
| Compose | Covered inside `query_proof`/`continue_proof` | Confirm coverage for new inspection commands' stdout/stderr |
| Live | None | P5 run records for OpenAI, Anthropic, Gemini, OpenRouter |

Record personally run results in PRs; this table lists proof targets, not results.

## Stable and Homebrew preparation (P4)

Proposed implementation route reusing pinned actions and packaging code where suitable; review the changed stable trust boundary before accepting reuse:

1. Prepare the candidate from a verified recent main nightly plus any reviewed cherry-picked fixes; the Cargo version determines its stable tag. Add a stable trigger (push to `stable`) that reuses the nightly verify, native build/test, package, attest, and draft-verify-publish jobs, with a non-prerelease tag derived from the Cargo version; refuse an existing tag.
2. Before pushing `stable`, the managing agent runs the full gate, advisory check, and supervised attack search required by `SECURITY.md`, recording their results, owner authorization, and exact candidate. After the push, the workflow repeats the full gate and advisory check on that revision before publication; it cannot substitute for the pre-push checks.
3. Keep enforcement in the workflow: publication runs only after the full gate and advisory job pass. The `stable` deletion ruleset is already applied; any further ruleset change is reviewed separately.
4. Homebrew: use the existing public `mdml/homebrew-tap` repository (confirmed empty on 2026-09-16). Prepare a reviewed formula update as part of each authorized stable release, without adding a cross-repository automation credential. The formula selects the four stable archives by URL and SHA-256 from `SHA256SUMS`, updated after publication; no new build path.
5. mise stable install: the existing GitHub backend without `prerelease=true`; document it when the first stable exists.

## External prerequisites

- Live checks wait for the owner to confirm credential provisioning. No provider credentials are needed for ordinary development.
- Any new dependency or changed acceptance boundary discovered during the spike returns to the owner before adoption.

## Completion conditions

- P1–P3 merged through an umbrella and promoted with named proofs passing and three recorded reviews.
- The seven proofs plus live checks have evidence, including a fresh-user walkthrough showing setup and first query succeed using only shipped instructions.
- A nightly containing P1–P3 installed and evaluated by the owner.
- P4 workflow merged and formula prepared; security checks recorded against the exact candidate; owner authorizes the stable push. After publication, update the formula in the tap and verify stable installation through mise and Homebrew.
- P6 native monitoring running; P7 disposition current.
