# Live provider checks — 2026-10-05

The managing agent personally ran these opt-in acceptance checks for the guided-setup release (`0.2.0`) on the owner's host. They are dated observations, not claims of ongoing availability. The [2026-09-16 record](live-provider-checks-2026-09-16.md) covers the previous release.

The binary was built with pinned Rust 1.97.1 without model credentials from `milestone/setup` at `353a803`, after the deterministic gates passed. Each run used isolated application state and an empty `ASK_MODEL_LIST_URL`. For hosted providers an external credential wrapper launched the absolute built binary, injecting credentials only into that process; no provider key was exported in the parent shell. Requests were sequential with no automatic retries. No raw transcript, credential, private wrapper path, or private session reference is retained here.

## Model listing through `ask init`

On 2026-10-05, `ask init` with redirected stdin and the key from the environment requested each hosted preset's model list and was cancelled at the model menu, before any verification request. Every provider's list was parsed and shown. The xAI and Cerebras response shapes, which were unconfirmed until this check, match the OpenAI shape.

| Preset | Identifiers offered |
|:--|:--|
| OpenAI | 133 |
| Anthropic | 13 |
| Gemini | 44 |
| OpenRouter | 464 |
| Groq | 11 |
| Cerebras | 2 |
| xAI | 14 |

Several lists include identifiers that are not chat models, such as speech, transcription, image, embedding, and moderation models, because those providers do not mark them in the list response. The type-to-filter menu and the manual-entry row are the remedy on the client; the published list is curated, as described below.

## Hosted query and reply

On 2026-10-05, each hosted provider answered one synthetic arithmetic query and one follow-up reply, with a 256-output-token profile limit.

| Preset | Model | Query input/output tokens | Reply input/output tokens | Result |
|:--|:--|:--|:--|:--|
| OpenAI | `gpt-4.1-nano` | 33 / 2 | 48 / 2 | Both exit 0; expected answers |
| Anthropic | `claude-haiku-4-5-20251001` | 30 / 5 | 44 / 5 | Both exit 0; expected answers |
| Gemini | `gemini-3.1-flash-lite` | 23 / 1 | 32 / 1 | Both exit 0; expected answers |
| OpenRouter | `openai/gpt-oss-20b` | 96 / 86 | 113 / 24 | Both exit 0; expected answers |
| Groq | `openai/gpt-oss-20b` | 96 / 47 | 113 / 31 | Both exit 0; expected answers |
| Cerebras | `gpt-oss-120b` | 96 / 243 | 113 / 48 | Both exit 0; expected answers |
| xAI | `grok-4.20-0309-non-reasoning` | 209 / 1 | 224 / 1 | Both exit 0; the query answer was as expected and the reply answer was arithmetically wrong |

For the three open-weight reasoning models, stdout carried only the answer although the providers reported more output tokens than the answer holds. `ask thread` showed that the xAI reply was sent with the first turn as context, so its wrong answer is the model's, not a context error.

A first Gemini query used `gemini-2.5-flash-lite`, an identifier the provider's own list offered. It returned HTTP 404 with exit 1, no answer, no turn, and failed statistics, the same result the 2026-09-16 record reports for that model. The reply that followed in the scripted sequence therefore continued the previous current thread and was answered by Anthropic (58 / 5 tokens): `ask` behaved as documented, and the extra request was an oversight in the check's sequencing. The managing agent then changed the profile to `gemini-3.1-flash-lite` and ran the query and reply in the table; these were deliberate, not automatic retries.

Sixteen requests were made in total: fifteen successful requests with 1,328 reported input tokens and 502 reported output tokens, plus the failed request with unknown usage. Local statistics were inspected to verify outcomes and usage. Provider billing was not independently checked.

An earlier attempt at the same sequence sent nothing: the commands inherited an open socket as standard input, and `ask` waited for piped input as its query contract specifies. Local statistics showed zero queries before the run above.

## Local server (Ollama)

On 2026-10-03, against Ollama 0.35.1 with the reasoning model `qwen3.5:4b` and a build of the earlier candidate `2ea6a52`, `ask init` listed the server's models and wrote a keyless `ollama` provider with `timeout_ms = 120000`, and a query and reply returned the expected answers with no reasoning text on stdout. Verification and `ask doctor --live` reported a failure for this working target because the model spent the check's 128-token output budget before writing answer text. [Pull request #76](https://github.com/mdml/ask/pull/76) fixed that.

On 2026-10-04, the same sequence on `353a803` passed end to end: init printed `Verified: the provider answered a minimal request.`, the query (37 / 213 tokens) and reply (54 / 318 tokens) returned the expected answers, and `ask doctor --live` printed `credential: not required` and `live: ok`.

## Init verification against hosted providers

On 2026-10-05, after the owner approved one more minimal paid request per provider, `ask init` ran with piped answers at `353a803` for each hosted preset: it took the key from the environment, listed the provider's models, accepted a typed identifier at the manual-entry row (the models in the table above), and sent the verification request. All seven printed `Verified: the provider answered a minimal request.`, wrote the configuration, exited 0, and left stdout empty. No key value appeared in any transcript or written file. Seven requests were made; usage is not reported by the verification step.

## Published model list

On 2026-10-05 the model list was generated on the owner's host and published for the first time. Afterwards, `ask init` at `353a803` with no key and the default location printed the request line and `Published model list generated 2026-10-05; any identifier can still be entered.` and offered the published identifiers for the OpenAI preset (38) and the Groq preset (4). Both runs were cancelled at the model menu and sent no provider request.

## Not covered

- `ask init` on an attended terminal against a real service; the menus and hidden prompt are covered by the PTY proofs against fake providers.
- LM Studio and llama.cpp servers, which are covered only by fake-provider proofs.
