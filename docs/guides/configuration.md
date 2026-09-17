# Configuring profiles and providers

`ask init` creates a first configuration and never changes an existing one. Every later change, and any setup without a dialogue, works on the complete TOML document: edit a copy, validate it with `ask configure check`, and install it with `ask configure apply`. `ask` has no commands that change individual fields. The [configuration reference](../reference/configuration.md) lists every key and validation rule.

## Change the installed configuration

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
