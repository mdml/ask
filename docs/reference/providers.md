# Provider reference

A provider table's `kind` selects the HTTP API that `ask` speaks to the endpoint in `base_url`. The provider set is compiled into `ask`; model identifiers are free text, which `ask init` can choose from the provider's own model list or the list the `ask` project publishes; see [model lists](#model-lists). Configuration keys are in the [configuration reference](configuration.md#providers).

## Provider kinds

Each kind uses its provider's own HTTP API with streaming over server-sent events. `ask` opens no WebSocket connection. `Cargo.lock` lists WebSocket crates as optional dependencies, but no enabled feature selects them, and `cargo tree --locked --target all -i tokio-tungstenite` finds them absent from the build. `base_url` is the prefix to which `ask` appends the request path. The table shows each provider's public endpoint; any endpoint serving the same API may be configured instead.

| `kind` | API | Typical `base_url` | Request path | Credential sent as |
|:--|:--|:--|:--|:--|
| `openai` | OpenAI Responses, with `store: false` | `https://api.openai.com/v1` | `/responses` | `Authorization: Bearer` header |
| `anthropic` | Anthropic Messages | `https://api.anthropic.com` | `/v1/messages` | `x-api-key` header |
| `gemini` | Gemini GenerateContent | `https://generativelanguage.googleapis.com` | `/v1beta/models/{model}:streamGenerateContent?alt=sse&key={credential}` | `key` URL query parameter |
| `openrouter` | OpenRouter Chat Completions | `https://openrouter.ai/api/v1` | `/chat/completions` | `Authorization: Bearer` header |
| `openai-compatible` | OpenAI Chat Completions | the server's OpenAI-compatible prefix, for example `http://127.0.0.1:PORT/v1` | `/chat/completions` | `Authorization: Bearer` header |

`ask` sends the credential and query content only to the configured endpoint. When a proxy is configured through the standard proxy variables (or, on macOS, the system settings), it carries those requests: an `https://` endpoint is tunneled, so the proxy sees only its host, and an `http://` endpoint, including its credential and content, is visible to the proxy in full. There is no exemption for loopback addresses: with a proxy configured, a request to a local server such as `http://localhost:11434/v1` also goes to the proxy unless `NO_PROXY` covers the host, so set `NO_PROXY=localhost,127.0.0.1` when you use a local server behind a proxy. No kind follows HTTP redirects; see [redirects](#redirects).

## Keyless targets

A provider of `kind = "openai-compatible"` may omit `api_key_env`; every other kind requires it. Queries, replies, and `ask doctor --live` on such a target read no environment variable, so credential variables that happen to be set never reach the request. Because the client library always sends an `Authorization` header, a keyless target sends the fixed placeholder `Authorization: Bearer no-key`. The placeholder is not a secret and is never redacted from diagnostics. Servers that need no key ignore it; a server that requires a real key rejects the request, so configure `api_key_env` for it instead.

## Initialization presets

`ask init` offers these presets and writes the listed values, so choosing one requires no endpoint or variable name:

| Menu entry | Provider name | `kind` | `base_url` | `api_key_env` |
|:--|:--|:--|:--|:--|
| OpenAI | `openai` | `openai` | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| Anthropic | `anthropic` | `anthropic` | `https://api.anthropic.com` | `ANTHROPIC_API_KEY` |
| Gemini | `gemini` | `gemini` | `https://generativelanguage.googleapis.com` | `GEMINI_API_KEY` |
| OpenRouter | `openrouter` | `openrouter` | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` |
| Groq | `groq` | `openai-compatible` | `https://api.groq.com/openai/v1` | `GROQ_API_KEY` |
| Cerebras | `cerebras` | `openai-compatible` | `https://api.cerebras.ai/v1` | `CEREBRAS_API_KEY` |
| xAI | `xai` | `openai-compatible` | `https://api.x.ai/v1` | `XAI_API_KEY` |

The menu then offers `Local model server`, which opens a second menu of servers you already run. `ask` connects to them; it does not start servers, download models, or manage accelerators. Each entry writes `kind = "openai-compatible"` with no `api_key_env` (a [keyless](#keyless-targets) target) and `timeout_ms = 120000`, because the first request often waits for the model to load. Hosted presets keep the default timeout. After you choose, Enter accepts the default endpoint, or you may type another base URL, which must satisfy the endpoint rule in the [configuration reference](configuration.md#providers):

| Menu entry | Provider name | Default `base_url` | Usually started with |
|:--|:--|:--|:--|
| Ollama | `ollama` | `http://localhost:11434/v1` | `ollama serve` |
| LM Studio | `lmstudio` | `http://localhost:1234/v1` | `lms server start` |
| llama.cpp server | `llamacpp` | `http://localhost:8080/v1` | `llama-server -m <model.gguf>` |

The last menu entry, a custom OpenAI-compatible endpoint, writes `kind = "openai-compatible"` with a provider name, `base_url`, and `api_key_env` that you enter. An empty answer to the credential-variable question omits `api_key_env`, which makes the target [keyless](#keyless-targets).

## Model lists

`ask init` offers model identifiers from the first available of these sources:

1. With a key, or for a [keyless](#keyless-targets) provider, the selected provider's own model list, described below.
2. Without a key, for the seven hosted [initialization presets](#initialization-presets) only, the [published model list](#published-model-list).
3. Otherwise, or when the chosen source fails or lists nothing, free-text entry.

A custom endpoint uses the published list only when its kind, `base_url`, and `api_key_env` equal a hosted preset's. Whatever the source, the menu always ends with an entry for typing any identifier.

### Provider model lists

When a key is available, or the provider is [keyless](#keyless-targets), `ask init` requests the selected provider's model list to offer identifiers in a menu. The request goes only to the provider's `base_url`, carries the credential the same way queries do for that kind, never follows redirects, and times out after 10 seconds:

| `kind` | Request | Credential sent as | Pagination |
|:--|:--|:--|:--|
| `openai`, `openrouter`, `openai-compatible` | `GET {base_url}/models` | `Authorization: Bearer` header | none |
| `anthropic` | `GET {base_url}/v1/models`, then `?after_id={last_id}` | `x-api-key` header, with `anthropic-version: 2023-06-01` | while `has_more` is true |
| `gemini` | `GET {base_url}/v1beta/models?pageSize=1000&key={credential}`, then with `&pageToken={nextPageToken}` | `key` URL query parameter | while `nextPageToken` is nonempty |

`ask init` reads at most 10 pages of at most 8 MiB each and keeps at most 2,000 identifiers. It treats the response as untrusted: it drops identifiers that are empty, longer than 200 bytes, or contain control characters or invisible Unicode format characters, and drops repeated identifiers. It also drops entries the API marks as unusable for text generation: Gemini models whose `supportedGenerationMethods` lacks `generateContent` (the `models/` prefix is removed from the rest), and entries whose `architecture.output_modalities` excludes `text`, as OpenRouter reports them. The remaining identifiers keep the provider's order, except that identifiers ending in a date-like snapshot suffix (`-YYYYMMDD`, `-YYYY-MM-DD`, `-MMDD`, or `-MM-DD`) follow the others. `ask` compiles in no model names or recommended defaults; the menu always includes an entry for typing any identifier. If the request fails, the dialogue shows the redacted reason and asks for the identifier as free text.

For a keyless provider, the same `GET {base_url}/models` request carries the placeholder `Authorization: Bearer no-key` and no user credential, even when credential variables are set. If the server cannot be reached or lists nothing, the dialogue prints one line that names the endpoint and, for a [local server](commands.md#ask-init) as `ask init` step 3 defines it, how that server is usually started, then asks for the identifier as free text. It never starts a server. `ask init` then verifies a keyless target before writing with the same minimal request as `ask doctor --live`, saying that it is sending a minimal request to the endpoint and omitting the cost notice. On failure it asks whether to write anyway.

### Published model list

The `ask` project publishes a model list at `https://raw.githubusercontent.com/mdml/ask/models/v1/models.json`, the file `v1/models.json` on the repository's `models` branch. An operator regenerates it from each provider's own list as described in [Publishing the model list](../guides/model-list-publishing.md); nothing model-specific is compiled into `ask`.

`ask init` requests it only for a hosted preset (OpenAI, Anthropic, Gemini, OpenRouter, Groq, Cerebras, or xAI) when no key is available for that provider. It first prints `Requesting the published model list from <URL>; no credentials are sent.` The request is a plain `GET` with no `Authorization`, `x-api-key`, or key parameter and no query content, even when credential variables for other providers are set. It never follows redirects, times out after 5 seconds, and reads at most 1 MiB. A provider target that `ask init` writes from a custom endpoint whose kind, `base_url`, and `api_key_env` all equal a hosted preset's is treated as that preset.

The environment variable `ASK_MODEL_LIST_URL` overrides the location. An empty value, or one that is not valid Unicode, disables the published list: `ask init` makes no request and asks for the identifier as free text. A value that is not valid Unicode is first reported as `ASK_MODEL_LIST_URL is not valid Unicode; skipping the published model list.`

Version 1 of the document is a JSON object:

```json
{
  "version": 1,
  "generated_at": "2026-10-01T06:00:00Z",
  "providers": {
    "openai": ["model-a", "model-b"],
    "anthropic": ["model-c"]
  }
}
```

`generated_at` is an RFC 3339 UTC time. `providers` maps preset provider names (`openai`, `anthropic`, `gemini`, `openrouter`, `groq`, `cerebras`, `xai`) to identifiers. `ask init` treats the response as untrusted. It accepts only `"version": 1` with a `generated_at` that begins with a `YYYY-MM-DD` date followed by `T` or `t`, and shows only that date: `Published model list generated <date>; any identifier can still be entered.` Identifiers are sanitized, de-duplicated, bounded to 2,000, and ordered exactly as a provider's own list is. A redirect, another HTTP status, a timeout, an oversized or invalid body, an unknown version, or no usable identifiers for the provider prints `Cannot use the published model list (<reason>); enter the identifier manually.` and continues with free-text entry; the published list never fails `ask init`. Without a key there is still no verification request.

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

## Reasoning output

Reasoning models expose their thinking in one of two ways. Some stream it in a separate field of the chat-completions delta, `reasoning` or `reasoning_content`, beside `content`; for `openai-compatible` targets, text in those fields is never written to stdout and is not recorded in the turn. Others, mostly local models, put it inline at the start of the answer text as `<think>...</think>`. For every provider kind, `ask` removes a leading inline block before the text reaches stdout, the recorded turn, or reply context; text that merely contains `<think>` elsewhere is unchanged. The exact rule is in [inline reasoning](query-behavior.md#inline-reasoning).

## Streaming implementation

The Rig library normalizes streaming responses for every kind except `gemini`. Gemini uses an adapter scoped to GenerateContent server-sent events, because Rig 0.42 drops `promptFeedback` and usage metadata from responses that have no candidates. The adapter handles the first candidate's text parts, excludes `thought: true` parts, retains the latest usage metadata, and maps the terminal reasons that the text-only contract needs. Malformed records and unknown, error, or protocol terminal reasons fail the request.