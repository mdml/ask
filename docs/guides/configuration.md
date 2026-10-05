# Configuring profiles and providers

The quickest way to change a configuration is to run `ask init` again. Every other change, and any setup without a dialogue, works on the complete TOML document: edit a copy, validate it with `ask configure check`, and install it with `ask configure apply`. Apart from `ask init`, `ask` has no commands that change individual fields. The [configuration reference](../reference/configuration.md) lists every key and validation rule.

## Change the configuration with `ask init`

Run `ask init` with a configuration already installed. It validates the file, shows the default profile and each profile with its provider and model, and offers one change:

- **Add a provider**: choose a provider and model as in first-time setup, which adds the provider and one profile that uses it.
- **Add a profile on an existing provider**: choose the provider, then a model, an optional system prompt, and a profile name.
- **Change a profile's model**: choose the profile, then the model. Existing threads keep the model they were created with; new threads use the new one.
- **Set the default profile**: choose one of the existing profiles.

As in first-time setup, the key comes from the environment or a hidden prompt and is used only to list models and verify the result. New provider and profile names must differ from existing ones. `ask init` then shows the complete resulting document and installs it only after you confirm; Esc, end of input, or any other answer leaves the file unchanged. Everything else in the file keeps its value, but the document is rewritten from its parsed contents, so comments, key order, and spacing in a hand-edited file are lost; `ask init` warns before asking when that would happen. If the installed file is invalid, `ask init` reports the problem and changes nothing; fix it by hand as described below.

## Change the installed configuration

For any other change, such as output limits, history expiry, timeouts, or a multiline system prompt, or to keep comments and formatting, edit the complete document:

1. Find the installed file. `ask doctor` prints its path on the `config:` line.
2. Copy it and edit the copy. In this example the copy is `candidate.toml`.
3. Validate the copy. `check` writes nothing and contacts no provider.

   ```sh
   ask configure check candidate.toml
   ```

4. Install it. `apply` runs the same validation, then atomically creates or replaces the installed file with the candidate's exact bytes, preserving comments, key order, and spacing.

   ```sh
   ask configure apply candidate.toml
   ```

Both commands also read the document from stdin, which suits scripts and agents:

```sh
ask configure apply - < candidate.toml
```

Both print a one-line result on stderr and leave stdout empty. They exit 0 on success and 1 when the document is invalid or cannot be read. `ask c` is an alias for `ask configure`.

A document that `check` accepts is a document `ask` can run with: query commands validate the installed file with the same strict validator. Unknown keys are rejected at every level, and every provider and profile is validated, not only the default profile's.

## Add a profile

A profile names a provider, a model, and optionally a system prompt and an output-token limit. This document defines two profiles on one provider:

```toml
default_profile = "default"

[providers.openai]
kind = "openai"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[profiles.default]
provider = "openai"
model = "MODEL"

[profiles.terse]
provider = "openai"
model = "MODEL"
system_prompt = "Answer in one short sentence."
```

Replace `MODEL` with a model identifier your provider accepts. Select a profile for a new thread with `--profile` or `-p`:

```sh
ask --profile terse "what is 2+2"
```

A thread keeps the profile it was created with. Replies always use that captured profile, so `--profile` is rejected on `ask reply`, and a configuration change affects only new threads.

## Add a provider

Add a `[providers.NAME]` table and point a profile at it. The [provider reference](../reference/providers.md) lists the supported `kind` values with their usual `base_url` and credential variable. A user-operated or third-party server that implements OpenAI Chat Completions uses `kind = "openai-compatible"`:

```toml
[providers.local]
kind = "openai-compatible"
base_url = "http://127.0.0.1:PORT/v1"
api_key_env = "LOCAL_API_KEY"
```

`api_key_env` names the environment variable that supplies the credential. `ask` never stores credential values; see [Injecting credentials](credentials.md).

For a user-operated server that needs no key, omit `api_key_env`. Only `kind = "openai-compatible"` may omit it. `ask` then reads no credential variable and sends the placeholder `Authorization: Bearer no-key`, which such a server ignores; see [keyless targets](../reference/providers.md#keyless-targets).

## Limit answer length

Set `max_output_tokens` in a profile to cap the answer. When the provider stops an answer at the limit, `ask` keeps the text received so far on stdout, warns on stderr, and exits 1. That turn is recorded as partial and is not sent as context for replies. Because threads keep their captured profile, start a new thread after raising the limit. Per-provider details are in the [provider reference](../reference/providers.md#output-token-limit).

## Expire old history

History is kept indefinitely by default. To remove threads whose newest turn is older than a number of days, add top-level keys before the first table:

```toml
expire_history = true
history_days = 30
```

`history_days` defaults to 90 when `expire_history = true`. Expiry runs only when `ask new` or `ask reply` submits a query. See [Threads and history](threads-and-history.md#expire-old-history).

## Use a self-contained directory

Setting `ASK_HOME` places configuration, data, and cache beneath one directory: `$ASK_HOME/config.toml`, `$ASK_HOME/data/ask.sqlite3`, and `$ASK_HOME/cache`. This is useful for portable setups and for trying a configuration without touching the installed one:

```sh
export ASK_HOME="$(mktemp -d)"
ask configure apply candidate.toml
ask doctor
```

Apart from credentials, `ask` itself reads only `ASK_HOME`, which overrides its paths (an empty value still counts as set); `TERM`, which decides whether menus are used; and `ASK_MODEL_LIST_URL`, which `ask init` uses to override the location of the [published model list](../reference/providers.md#published-model-list) it offers when no key is available (an empty or non-Unicode value disables that request). The platform and HTTP libraries it uses also honor other variables and settings, among them `HOME` and, on Linux, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and `XDG_CACHE_HOME`, which locate the platform-standard directories when `ASK_HOME` is unset; the standard proxy variables `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, and `NO_PROXY`, or their lowercase forms, and on macOS the system proxy settings, which send its requests through a proxy; and on Linux `SSL_CERT_FILE` and `SSL_CERT_DIR`, which change the certificates used to verify `https://` endpoints. The [configuration reference](../reference/configuration.md#locations) gives the directories, and the [provider reference](../reference/providers.md#provider-kinds) says what a proxy can see.
