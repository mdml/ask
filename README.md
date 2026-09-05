# `ask`

`ask` is a fast, opinionated terminal lookup tool for asking language models quick questions without starting an agent session.

**Status: pre-alpha.** The query command is implemented, but interfaces may change before the first `0.1.0` release. The only provider kind currently supported is `openai-compatible`.

## Usage

The following forms each start a new query. Prompt words are joined with single spaces.

```sh
ask "what is 2+2"
ask new "what is 2+2"
ask n "what is 2+2"
```

`ask` reads `$ASK_HOME/config.toml` when `ASK_HOME` is set. Otherwise, it reads `config.toml` from the platform-standard configuration directory for an application named `ask`.

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

`api_key_env` names the environment variable that supplies the credential; `ask` does not store credential values. The request timeout defaults to 30 seconds. A profile can replace the built-in terminal-oriented system prompt.

The answer is streamed to stdout as unstyled Markdown and ends with exactly one newline. Statistics, warnings, usage errors, and diagnostics are written to stderr. A successful query exits 0, provider and configuration failures exit 1, and a missing prompt exits 2. If the stdout reader closes early, `ask` exits 0 without a diagnostic.

## Development

Rules for contributors and coding agents are in `AGENTS.md`. See `CONTRIBUTING.md` for tooling setup, branch flow, and verification gates.

Run the fast gate with `just verify` before every commit. Pull requests must pass `just verify-full`.

## License

Apache-2.0. See `LICENSE`.
