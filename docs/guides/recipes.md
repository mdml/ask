# Recipes and FAQ

Short answers to questions that come up after a first `ask init`. Each recipe gives a little context and a command or TOML fragment to copy. The query examples assume `ask` runs through a credential launcher, as in [Getting started](getting-started.md#supply-a-credential-and-ask-a-question). Exact behavior is in the [command reference](../reference/commands.md) and the [configuration reference](../reference/configuration.md).

## Recipes

### Read a key from a different variable

A preset suggests the provider's standard variable, such as `ANTHROPIC_API_KEY`. Some other tools change their behavior when that variable is set; Claude Code, for example, bills the API instead of a subscription. To keep it unset and give `ask` its own name, type the name at the `Credential variable [ANTHROPIC_API_KEY]: ` question when `ask init` sets up the provider. The name may contain letters, digits, and underscores and must not start with a digit.

For a provider that is already configured, change `api_key_env` in a copy of the installed configuration and install it with `ask configure apply`; [Configuring profiles and providers](configuration.md#change-the-installed-configuration) gives the steps.

```toml
[providers.anthropic]
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ASK_ANTHROPIC_API_KEY"
```

```sh
ask configure check candidate.toml
ask configure apply candidate.toml
```

Then have your launcher inject `ASK_ANTHROPIC_API_KEY`; `ask doctor` reports `credential ASK_ANTHROPIC_API_KEY: present` when it is set. A thread keeps the variable name it was created with, so `ask reply` on a thread started before the change still reads `ANTHROPIC_API_KEY`; start a new thread. The published model list is still offered for the provider without a key, because `ask init` recognizes a hosted preset by its kind and endpoint, whatever its credential variable.

### Share one key across several profiles

Profiles on the same provider share its `api_key_env`, so one key serves them all. Give each profile its own model, system prompt, or output limit, and choose one per new thread with `--profile` or `-p`. Re-running `ask init` and choosing **Add a profile on an existing provider** writes a profile like these; edit the document as above for keys `ask init` does not ask about, such as `max_output_tokens`.

```toml
default_profile = "default"

[providers.openai]
kind = "openai"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[profiles.default]
provider = "openai"
model = "FAST_MODEL"

[profiles.deep]
provider = "openai"
model = "LARGER_MODEL"
system_prompt = "Answer thoroughly, with a short example when it helps."
max_output_tokens = 8192
```

```sh
ask -p deep "how does TCP slow start work"
```

Replace `FAST_MODEL` and `LARGER_MODEL` with identifiers your provider accepts.

### Reach a local model server behind a corporate proxy

When a proxy is configured through `HTTP_PROXY`, `HTTPS_PROXY`, or `ALL_PROXY` (or, on macOS, the system settings), `ask` sends requests to `localhost` through it too, unless `NO_PROXY` covers the host. Exempt the loopback hosts so requests reach the server directly, for `ask init` as well as for queries:

```sh
export NO_PROXY=localhost,127.0.0.1
ask init
```

Put the `export` in your shell profile, or prefix single commands with `NO_PROXY=localhost,127.0.0.1`. Requests to hosted providers still use the proxy. The [provider reference](../reference/providers.md#provider-kinds) says what a proxy can see.

### Use a custom endpoint that needs no key

A server that implements OpenAI Chat Completions and needs no key can omit `api_key_env`. In `ask init`, choose **Custom OpenAI-compatible endpoint** and leave the credential variable name empty. By hand, write:

```toml
[providers.lab]
kind = "openai-compatible"
base_url = "http://SERVER:8000/v1"
timeout_ms = 120000
```

Replace `SERVER:8000` with your server's host and port. `ask` then reads no credential variable and sends the placeholder `Authorization: Bearer no-key`; see [keyless targets](../reference/providers.md#keyless-targets). Only `kind = "openai-compatible"` may omit `api_key_env`. `timeout_ms` is optional; the default is 30 seconds.

### Pipe the clipboard or a file, and save the answer

With prompt words and piped or redirected stdin, the words are the instruction and stdin is the material. Stdout holds only the answer, so redirecting it saves just the answer; the statistics line stays on the terminal.

```sh
pbpaste | ask "remove blockquoting from this text"
ask "summarize this file in three bullets" < notes.txt
git diff --staged | ask "write a commit message for this diff" > message.txt
```

`pbpaste` is the macOS clipboard command; on Linux, `wl-paste` or `xclip -o -selection clipboard` play the same role. [Using `ask` in pipelines](shell-composition.md) covers exit statuses and early pipe closure.

### Launch `ask` through a credential manager

Every command that contacts a provider needs the key each time, so put the injection in a small launcher script and forward arguments with `"$@"`. [Injecting credentials](credentials.md#example-for-existing-1password-cli-users) gives the 1Password CLI recipe; with it, a launcher saved as `~/bin/ask-op` and made executable looks like this:

```sh
#!/bin/sh
exec op run --env-file="$HOME/.ask-secrets.env" -- ask "$@"
```

Do not name the script `ask` if it would call itself. Run `ask doctor` through the same launcher so that it sees the same environment.

### When `ask init` cannot verify the setup

After choosing a model, `ask init` sends one minimal request. On failure it prints `Verification failed: <cause>` and asks `Write the configuration anyway? [y/N]: `. Answer `n` to cancel and fix the cause, or `y` to keep the configuration and fix it later by re-running `ask init`. Afterwards, `ask doctor --live` sends the same request.

- **Wrong or revoked key.** The cause reports the provider's rejection. Correct the key in your credential manager, then re-run `ask init`.
- **Wrong model identifier.** A typed identifier is sent exactly as written, and the provider rejects one it does not serve, often with a 404 status. Re-run `ask init`, choose **Change a profile's model**, and pick from the list.
- **A listed model that returns 404.** See [the FAQ entry below](#why-can-a-model-from-the-providers-own-list-fail-with-404); choose another model.
- **No key available.** `ask init` sends no request and says the setup will not be verified. Run `ask doctor --live` once the key is supplied.
- **Local server unreachable.** `ask init` names the endpoint and how the server is usually started, such as `ollama serve`, and asks for the identifier as free text. Start the server and re-run `ask init`.

A reasoning model can spend the whole 128-token budget of the verification request thinking. Verification still succeeds, because the provider answered. Ordinary queries can hit the same wall: if an answer ends with `ask: warning: answer stopped at the provider's output-token limit; set a larger max_output_tokens in the profile`, raise `max_output_tokens` in the profile and start a new thread; see [Limit answer length](configuration.md#limit-answer-length).

### Change the default profile or a profile's model

Run `ask init` again. With a configuration installed, it offers one change per run, including **Set the default profile** and **Change a profile's model**, then shows the complete resulting document and writes it only after you confirm.

```sh
ask init
```

A model change applies to new threads; existing threads keep the model they were created with. `ask init` rewrites the file from its parsed contents, so comments and formatting in a hand-edited file are lost, and it warns before asking. To keep them, edit the file and use `ask configure apply` instead. To use another profile for one new thread without changing the default, pass `--profile NAME`.

## FAQ

### Why does `ask` never store the key?

By design, `ask` leaves credential storage to external credential managers, so its configuration, history, and diagnostics never hold a key. `ask` reads the key from the environment variable named by `api_key_env` when it runs; a key pasted during `ask init` is used only to list models and verify the setup. [Injecting credentials](credentials.md) shows how to supply it per process.

### Why does `ask r` reject `--profile`?

A thread captures its profile, including the provider, model, and system prompt, when it is created, and every reply uses that captured profile, even after the configuration changes. `ask r --profile NAME` is therefore a usage error. To use another profile, start a new thread: `ask -p NAME "..."`.

### Why can a model from the provider's own list fail with 404?

`ask init` shows what the provider's list endpoint returns, dropping only unsafe or repeated identifiers and entries the API marks as unable to generate text. Providers can list models that the API `ask` uses does not serve, or that your account cannot use, and those fail when queried. The [published model list](../reference/providers.md#published-model-list) is narrower: it keeps only models a public catalog describes as text-only chat models with tool calling. Any identifier can still be typed.

### Why is the first request to a local server slow?

The server loads the model into memory on the first request, which can take a while for a large model. Local-server presets therefore write `timeout_ms = 120000`, a 120-second timeout, instead of the default 30 seconds. If loading takes longer, raise `timeout_ms` for that provider with `ask configure apply`.

### Why does stdout hold only the answer?

So that `ask` composes with pipes: `ask "..." > answer.md` and `ask "..." | less` receive the answer and nothing else. The statistics line, prompts, warnings, and errors go to stderr. Hide the statistics line with `2>/dev/null`, which also hides errors, so check the exit status.

### Why isn't a reasoning model's thinking shown?

Reasoning is not part of the answer. Reasoning that a provider streams in a separate field never reaches stdout or the recorded thread, and a `<think>...</think>` block at the start of the answer is removed; see [inline reasoning](../reference/query-behavior.md#inline-reasoning). The reasoning still counts toward the output-token limit.

### How do I see the published model list without a key?

`ask init` offers it automatically for a hosted provider when no key is available. To read it yourself:

```sh
curl -fsSL https://raw.githubusercontent.com/mdml/ask/models/v1/models.json
```

`ASK_MODEL_LIST_URL` points `ask init` at another copy of the list. Setting it to an empty value, as in `ASK_MODEL_LIST_URL= ask init`, disables the request: without a key, `ask init` asks for the identifier as free text.
