# Guided setup and local models delivery tracker

Internal delivery plan for the guided-setup release, version `0.2.0`, developed on `milestone/setup`. Product scope, exclusions, and acceptance criteria are owned outside this repository; this tracker records what landed, the named proofs, external prerequisites, and completion conditions. The [tool-less beta tracker](beta-delivery.md) records the previous release. Update this file in the same change that lands or reshapes a packet.

## Landed on `milestone/setup`

| Packet | Pull request | Outcome |
|:--|:--|:--|
| S1 | [#62](https://github.com/mdml/ask/pull/62) | Guided `ask init`: hosted presets including Groq, Cerebras, and xAI; key from the environment or a hidden prompt; the provider's model list in a type-to-filter menu; verification before writing; several providers in one run. |
| S2 | [#63](https://github.com/mdml/ask/pull/63) | `api_key_env` optional for `openai-compatible`; keyless targets send the placeholder credential; schema version 5. |
| S3 | [#65](https://github.com/mdml/ask/pull/65) | Local model server presets (Ollama, LM Studio, llama.cpp) with keyless listing and verification. |
| S4 | [#67](https://github.com/mdml/ask/pull/67) | Re-running `ask init` changes an existing configuration: add a provider, add a profile, change a model, set the default. |
| S5 | [#68](https://github.com/mdml/ask/pull/68) | Published model list for a hosted preset without a key, `ASK_MODEL_LIST_URL`, and the operator refresh script. |
| S6 | [#69](https://github.com/mdml/ask/pull/69) | A leading inline `<think>` block is stripped from answers. The owner recorded this decision as provisional on 2026-10-02. |
| S7 | [#70](https://github.com/mdml/ask/pull/70) | Menus and the hidden prompt hold raw mode throughout; `libc` is a direct dependency and `src/terminal/attributes.rs` is the only `unsafe` code. |
| S8 | [#61](https://github.com/mdml/ask/pull/61) | README header with slogan and badges; re-recorded demo. |
| S9 | [#71](https://github.com/mdml/ask/pull/71) | Action dependency review renewed through 2026-11-01; an unchanged nightly skips when the review has expired. |
| Dependencies | [#59](https://github.com/mdml/ask/pull/59), [#60](https://github.com/mdml/ask/pull/60) | reqwest 0.13.5; install-action 2.87.18, later 2.87.22 in S9. |

## Named proofs

The promotion to `main` requires every deterministic proof below to pass on the frozen candidate. They run through the real binary against fake providers and need no credentials or network access.

| Proof | Test target | What it establishes for this release |
|:--|:--|:--|
| Guided setup | `configure_proof` | Preset, local, and custom providers; key from the environment and the hidden prompt; list, filter, and manual entry; verification; cancellation; redirected-stdin fallback; terminal attributes never echo and are restored. |
| Edit | `init_edit_proof` | The four changes to an existing configuration, preserved values, the reformatting warning, and refusal to overwrite a changed or invalid file. |
| Published list | `published_list_proof`, `model_list_refresh` | The no-key fallback, its bounds and failure paths, that no credential is sent, that no proof contacts the network, and the operator script's behavior offline. |
| Keyless | `keyless_proof` | Queries, replies, and `doctor` on a target with no credential; separate-field reasoning never reaches stdout. |
| Reasoning | `inline_reasoning_proof` | A leading inline reasoning block is neither printed nor recorded; other text is unchanged. |
| Unsafe boundary | `unsafe_boundary` | `unsafe` code is denied everywhere except the terminal attributes module. |
| Carried forward | `query_proof`, `continue_proof`, `recall_proof`, `provider_proof`, `doctor_proof`, `offline_acceptance`, `demo_proof`, `nightly_packaging` | The tool-less beta's behavior is unchanged except where this release documents a change. |

Record personally run results in pull requests; this table lists proof targets, not results.

## Acceptance checks that need the owner's host

These use real services and run outside the gates, on the owner's host through the credential wrapper described in [live-provider checks](../guides/live-provider-checks.md). As of 2026-10-02 none has run for this release.

| Check | Needs | Status on 2026-10-02 |
|:--|:--|:--|
| One query and one reply per hosted provider: OpenAI, Anthropic, Gemini, OpenRouter, Groq, Cerebras, xAI | A key for each in the owner's provider store | Not run. |
| One query and one reply against a real Ollama server, including a model that emits reasoning | Ollama installed with a small model | Not run. |
| `ask init` model listing against each hosted provider's live list endpoint | The same keys | Not run. The xAI and Cerebras list response shapes are assumed to match OpenAI's and are unconfirmed. |
| First publication of the model list | The same keys; see [publishing the model list](../guides/model-list-publishing.md) | Not published. Until then a hosted preset without a key falls back to manual entry. |

## Completion conditions

- The frozen promotion candidate passes `just verify-full` and the three independent reviews in [agent-contracts.md](agent-contracts.md), with findings and dispositions recorded in the promotion pull request.
- The acceptance checks above have recorded results.
- A nightly built from `main` at version `0.2.0` is published, which also confirms the renewed action review and the `fallback: none` install steps on the release workflow.
- The owner evaluates that nightly and authorizes the stable release; the [stable release guide](../guides/stable-releases.md) lists the remaining operator steps.
- The README demo is recorded from the release candidate build.
