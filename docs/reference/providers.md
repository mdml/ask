# Provider reference

A provider table's `kind` selects the HTTP API that `ask` speaks to the endpoint in `base_url`. The provider set is compiled into `ask`; model identifiers are free text. Configuration keys are in the [configuration reference](configuration.md#providers).

## Provider kinds

Each kind uses its provider's own HTTP API with streaming over server-sent events. `ask` opens no WebSocket connection. `Cargo.lock` lists WebSocket crates as optional dependencies, but no enabled feature selects them, and `cargo tree --locked --target all -i tokio-tungstenite` finds them absent from the build. `base_url` is the prefix to which `ask` appends the request path. The table shows each provider's public endpoint; any endpoint serving the same API may be configured instead.

| `kind` | API | Typical `base_url` | Request path | Credential sent as |
|:--|:--|:--|:--|:--|
| `openai` | OpenAI Responses, with `store: false` | `https://api.openai.com/v1` | `/responses` | `Authorization: Bearer` header |
| `anthropic` | Anthropic Messages | `https://api.anthropic.com` | `/v1/messages` | `x-api-key` header |
| `gemini` | Gemini GenerateContent | `https://generativelanguage.googleapis.com` | `/v1beta/models/{model}:streamGenerateContent?alt=sse&key={credential}` | `key` URL query parameter |
| `openrouter` | OpenRouter Chat Completions | `https://openrouter.ai/api/v1` | `/chat/completions` | `Authorization: Bearer` header |
| `openai-compatible` | OpenAI Chat Completions | the server's OpenAI-compatible prefix, for example `http://127.0.0.1:PORT/v1` | `/chat/completions` | `Authorization: Bearer` header |

`ask` sends the credential and query content only to the configured endpoint. No kind follows HTTP redirects; see [redirects](#redirects).

## Initialization presets

`ask init` offers these presets and writes the listed values, so choosing one requires entering only a model identifier:

| Menu entry | Provider name and `kind` | `base_url` | `api_key_env` |
|:--|:--|:--|:--|
| OpenAI | `openai` | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| Anthropic | `anthropic` | `https://api.anthropic.com` | `ANTHROPIC_API_KEY` |
| Gemini | `gemini` | `https://generativelanguage.googleapis.com` | `GEMINI_API_KEY` |
| OpenRouter | `openrouter` | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` |

The fifth menu entry, a custom OpenAI-compatible endpoint, writes `kind = "openai-compatible"` with a provider name, `base_url`, and `api_key_env` that you enter.

## Model identifiers

Model identifiers are never checked against a catalog. Every kind except `gemini` sends the identifier verbatim in the request body. Gemini carries the model in the URL path, so `ask` percent-encodes every byte outside letters, digits, `-`, `.`, `_`, and `~`. The identifier stays one path segment (a `/`, `?`, `#`, or space cannot change the endpoint), and the provider receives it unchanged after decoding.

## Credentials in diagnostics

The Gemini credential is percent-encoded in the query string in the same way as the model identifier. Because that credential is part of the request URL, transport diagnostics could quote it. `ask` replaces every occurrence of a credential in a diagnostic with `[redacted]`, whether literal or percent-encoded.

## Output-token limit

A profile may set `max_output_tokens`. An explicit value is sent to every kind in that API's field:

| `kind` | Field |
|:--|:--|
| `openai` | `max_output_tokens` |
| `anthropic`, `openrouter`, `openai-compatible` | `max_tokens`. The Rig library may respell it `max_completion_tokens` for some OpenAI model identifiers. |
| `gemini` | `generationConfig.maxOutputTokens` |

When the profile omits it, `anthropic` requests, which require a limit, send `4096`. Every other kind sends no limit and keeps the provider's default.

When the provider reports that the answer stopped at the output-token limit:

- stdout keeps the answer received so far;
- stderr carries the one-line warning `ask: warning: answer stopped at the provider's output-token limit; set a larger max_output_tokens in the profile`;
- `ask` exits 1;
- the turn is recorded as partial, even when no answer text arrived, so replies do not send it as context;
- the statistics row has the outcome partial and the error class `output_limit`, and the provider target is recorded as healthy because it answered normally.

Changing the profile affects new threads only. Replies continue to use the profile captured when their thread was created, so start a new thread to use a larger limit.

## Response endings

- An ordinary successful ending is a complete answer, even with no text.
- A content-filter or refusal ending is a provider failure. Refusal text already delivered remains on stdout and is recorded only as a partial turn; an empty refusal records no turn. Refused turns are excluded from reply context.
- A stream that ends with the in-band `error` finish reason, which is OpenRouter's mid-stream error chunk, is a provider failure and never a complete answer.
- An output-token-limit ending is handled as described in [output-token limit](#output-token-limit).

How each ending is recorded is specified in [query behavior](query-behavior.md#turn-status).

## Redirects

`ask` does not follow HTTP redirects from the provider endpoint, whether they point to another origin or the same one. A redirect response is a provider failure: `ask` exits 1 with a diagnostic naming the status, the credential and query are not resent anywhere, no turn is recorded, and the failure is recorded in statistics and provider health.

## Streaming implementation

The Rig library normalizes streaming responses for every kind except `gemini`. Gemini uses an adapter scoped to GenerateContent server-sent events, because Rig 0.42 drops `promptFeedback` and usage metadata from responses that have no candidates. The adapter handles the first candidate's text parts, excludes `thought: true` parts, retains the latest usage metadata, and maps the terminal reasons that the text-only contract needs. Malformed records and unknown, error, or protocol terminal reasons fail the request.