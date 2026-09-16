# Live provider checks — 2026-09-16

The managing agent personally ran these opt-in compatibility checks against the provider implementation reviewed in `fb29158377f668ffa3da01f43e6dbec850e09c5f`. Its Rust sources matched the built binary; the final amendment changed documentation only. These are dated observations, not claims of ongoing availability or completed beta acceptance.

The binary was built with pinned Rust 1.97.1 without model credentials, after deterministic provider and integration gates passed. Each provider used isolated application state. An external credential wrapper launched the absolute built binary, injecting credentials only into that process. No provider key was exported in the parent shell. Requests were sequential, used a 128-output-token cap, and had no automatic retries. The synthetic arithmetic query and follow-up both returned the expected answers. No raw prompt, transcript, credential, private wrapper path, or private session reference is retained here.

| Provider kind | Model | Query input/output tokens | Reply input/output tokens | Result |
|:--|:--|:--|:--|:--|
| `openai` | `gpt-4.1-nano` | 34 / 2 | 56 / 2 | Both exit 0; complete turns |
| `anthropic` | `claude-haiku-4-5-20251001` | 33 / 5 | 56 / 5 | Both exit 0; complete turns |
| `gemini` | `gemini-3.1-flash-lite` | 24 / 1 | 40 / 1 | Both exit 0; complete turns |
| `openrouter` | `openai/gpt-4.1-nano` | 34 / 2 | 56 / 2 | Both exit 0; complete turns |

An earlier single Gemini query using `gemini-2.5-flash-lite` returned HTTP 404, exit 1, no answer, and no reported usage. It recorded failed statistics and no turn. The cause was not established. Google's [model lifecycle page](https://ai.google.dev/gemini-api/docs/deprecations) still listed that model on the check date. The owner separately authorized the subsequent query and conditional reply using `gemini-3.1-flash-lite`; they were not automatic retries.

Nine requests were made in total: eight successful requests with 333 reported input tokens and 20 reported output tokens, plus the failed request with unknown usage. Local statistics were inspected to verify command, model, outcome, usage, and failure classification. Provider billing was not independently checked.

Custom OpenAI-compatible endpoints remain covered by deterministic fake-provider proofs; this record does not claim a live check against every compatible service. Setup, diagnostics, packaged installation, and the owner's complete-workflow evaluation remain separate acceptance evidence.
