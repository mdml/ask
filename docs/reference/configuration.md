# Configuration reference

`ask` is configured by one TOML document. The schema is provisional during 0.x. To create, change, and install the document, see [Configuring profiles and providers](../guides/configuration.md) and [`ask configure`](commands.md#ask-configure).

## Locations

| Item | With `ASK_HOME` set | Otherwise |
|:--|:--|:--|
| Configuration | `$ASK_HOME/config.toml` | `config.toml` in the platform-standard configuration directory for an application named `ask` |
| History and statistics database | `$ASK_HOME/data/ask.sqlite3` | `ask.sqlite3` in the platform-standard data directory |
| Cache | `$ASK_HOME/cache` | the platform-standard cache directory |

`ask doctor` prints every resolved path. Environment variables are used only for credentials and for the `ASK_HOME` path override; no environment variable changes any other setting.

## Example

```toml
default_profile = "default"
# expire_history = true
# history_days = 90

[providers.local]
kind = "openai-compatible"
base_url = "http://127.0.0.1:PORT/v1"
api_key_env = "LOCAL_API_KEY"
# timeout_ms = 30000

[profiles.default]
provider = "local"
model = "fake-model"
# system_prompt = "..."
# max_output_tokens = 4096
```

## Top-level keys

| Key | Type | Required | Meaning |
|:--|:--|:--|:--|
| `default_profile` | string | yes | The profile that new queries use unless `--profile` selects another. It must name a configured profile. |
| `expire_history` | boolean | no | Enables [history expiry](storage.md#history-expiry). Defaults to `false`, which keeps history indefinitely. |
| `history_days` | positive integer | no | Age in days after which a thread expires. Defaults to 90 when `expire_history = true`, and is rejected when expiry is not enabled. |
| `providers` | table | yes | Named provider tables. |
| `profiles` | table | yes | Named profile tables. |

In TOML, top-level keys must appear before the first table header.

## Providers

Each `[providers.NAME]` table configures one endpoint. `NAME` must not be empty.

| Key | Type | Required | Meaning |
|:--|:--|:--|:--|
| `kind` | string | yes | One of `openai`, `anthropic`, `gemini`, `openrouter`, or `openai-compatible`; see the [provider reference](providers.md). |
| `base_url` | string | yes | The prefix to which `ask` appends the API path. It must be an `http://` or `https://` URL with a host, no embedded username or password, and no query or fragment component, including an empty trailing `?` or `#`. |
| `api_key_env` | string | yes | The name of the environment variable that supplies the credential: letters, digits, and underscores, not starting with a digit. `ask` never stores credential values. |
| `timeout_ms` | positive integer | no | Request timeout in milliseconds. Defaults to `30000` (30 seconds). |

## Profiles

Each `[profiles.NAME]` table is a named combination of provider, model, and prompt. `NAME` must not be empty.

| Key | Type | Required | Meaning |
|:--|:--|:--|:--|
| `provider` | string | yes | The name of a configured provider. |
| `model` | string | yes | The model identifier sent to the provider. It is nonempty free text and is never checked against a catalog. |
| `system_prompt` | string | no | Replaces the default system prompt. An explicit empty string is a replacement too. |
| `max_output_tokens` | positive integer | no | Output-token limit; see [output-token limit](providers.md#output-token-limit). |

When a profile sets no `system_prompt`, `ask` sends this default system prompt: "Answer briefly in plain Markdown suitable for a terminal."

A thread captures its resolved profile when it is created, so configuration changes affect new threads only; see [threads and replies](query-behavior.md#threads-and-replies).

Display settings are not implemented.

## Validation

`ask configure check`, `ask configure apply`, `ask doctor`, and ordinary query loading all apply the same strict validator. The validator:

- rejects any key that is not part of the schema, at every level of the document;
- validates every provider and every profile, not only the ones the default profile selects;
- requires every profile to reference a configured provider, and `default_profile` to name a configured profile;
- enforces the type, range, and format rules in the tables above.

Validation diagnostics contain only schema field names and error categories, with one-based entry and key indexes in sorted key order. They never echo candidate-controlled keys, provider or profile names, reference values, or parser messages. Invalid TOML syntax reports a one-based line and column instead.

Validation never reads a credential value and never contacts a provider.
