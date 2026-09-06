# `ask`

`ask` is a fast, opinionated terminal lookup tool for asking language models quick questions without starting an agent session.

**Status: pre-alpha.** The query and configure commands are implemented, but interfaces may change before the first `0.1.0` release. The only provider kind currently supported is `openai-compatible`.

## Usage

The following forms each start a new query. Prompt words are joined with single spaces.

```sh
ask "what is 2+2"
ask new "what is 2+2"
ask n "what is 2+2"
```

`ask` reads `$ASK_HOME/config.toml` when `ASK_HOME` is set. Otherwise, it reads `config.toml` from the platform-standard configuration directory for an application named `ask`.

### Creating a configuration

`ask configure` and `ask c` create that file through a line-oriented dialogue. Every prompt and diagnostic is written to stderr, and stdout stays empty. The answers may come from a terminal or from redirected stdin, one answer per line. The dialogue asks for:

1. A provider name, for example `openrouter`.
2. The endpoint base URL, which must use `http://` or `https://`, include a host, and contain no embedded username/password credentials.
3. A model identifier, sent to the provider as typed.
4. The name of the environment variable that will hold the credential. `ask` validates the name only; it never reads or stores the value, and configuring makes no network request.
5. An optional replacement system prompt. The current default system prompt is shown first, and an empty answer keeps it.
6. A profile name, which defaults to `default`. The profile created becomes the default profile.

An invalid answer prints a one-line explanation and asks again. The dialogue then shows the exact TOML it will write and asks for confirmation. Only `y` or `yes` writes the file; any other answer, or end of input at any prompt, cancels without writing and exits 1.

`ask configure` refuses to run when the configuration file already exists and leaves it unchanged. To edit an existing configuration, manage several profiles, or supply a multiline system prompt, edit the TOML directly. Retention settings are not yet implemented.

A completed dialogue writes a file in the schema below, omitting `timeout_ms` and `system_prompt` when the defaults apply.

### Configuration schema

The configuration schema is provisional during 0.x:

```toml
default_profile = "default"

[providers.local]
kind = "openai-compatible"
base_url = "http://127.0.0.1:PORT/v1"
api_key_env = "LOCAL_API_KEY"
# timeout_ms = 30000

[profiles.default]
provider = "local"
model = "fake-model"
# system_prompt = "..."
```

`api_key_env` names the environment variable that supplies the credential; `ask` does not store credential values. The request timeout defaults to 30 seconds. When a profile sets no `system_prompt`, `ask` sends this default system prompt: "Answer briefly in plain Markdown suitable for a terminal." A profile's `system_prompt` replaces it.

The answer is streamed to stdout as unstyled Markdown and ends with exactly one newline. Statistics, warnings, usage errors, and diagnostics are written to stderr. A successful query exits 0, and provider and configuration failures exit 1. If the stdout reader closes early, `ask` exits 0 without a diagnostic. Running `ask` with no prompt words currently prints a usage message and exits 2; the intended behavior, an interactive multiline `>` prompt, is not yet implemented.

## Development

Rules for contributors and coding agents are in `AGENTS.md`. See `CONTRIBUTING.md` for tooling setup, branch flow, and verification gates.

Run the fast gate with `just verify` before every commit. Pull requests must pass `just verify-full`.

## License

Apache-2.0. See `LICENSE`.
