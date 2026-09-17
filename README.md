# `ask`

intelligence in your terminal

`ask` is a fast, opinionated terminal lookup tool for asking language models quick questions without starting an agent session. Query answers go to stdout as plain Markdown; prompts and diagnostics go to stderr, so `ask` composes with pipes.

![A terminal session piping input into ask, replying, and viewing the thread.](docs/assets/demo/demo.gif)

Recorded with local fixture responses; timings do not represent provider performance. [Text version](docs/assets/demo/demo.txt).

**Status: beta, distributed as nightly builds.** No stable release has been published. Interfaces may change during 0.x. `ask` works with OpenAI, Anthropic, Gemini, OpenRouter, and custom OpenAI-compatible endpoints on macOS and Linux (arm64 and x86-64).

## Install

Choose a nightly tag from [GitHub Releases](https://github.com/mdml/ask/releases) and install it with [mise](https://mise.jdx.dev/), substituting the tag for `TAG`:

```sh
mise use -g 'github:mdml/ask[prerelease=true]@TAG'
```

Nightly archives are checksummed and carry GitHub attestations; [Nightly releases](docs/guides/nightly-releases.md) explains how to verify them.

## First use

1. Create a configuration. `ask init` asks for a provider, a model identifier, an optional system prompt, and a profile name, shows the TOML it will write, and writes it only after you confirm.

   ```sh
   ask init
   ```

2. Ask a first question. `ask` reads the provider key from an environment variable, which `ask init` names, and never stores it. In bash or zsh, this hidden prompt keeps the key out of shell history and out of the parent shell:

   ```sh
   ( printf 'API key: ' >&2; IFS= read -rs OPENAI_API_KEY </dev/tty || exit; printf '\n' >&2; export OPENAI_API_KEY; exec ask "what is 2+2" )
   ```

   Replace `OPENAI_API_KEY` with your provider's variable name, and paste the key only at the hidden prompt. The key exists for that one command only.

3. Set up a credential launcher for everyday use. Every later `ask` command that contacts a provider needs the key again, so have a credential manager inject it each time `ask` launches; [Injecting credentials](docs/guides/credentials.md) gives recipes. The examples below assume `ask` runs through such a launcher.

[Getting started](docs/guides/getting-started.md) covers these steps in more detail.

## Examples

```sh
ask "what is 2+2"                       # ask a question in a new thread
ask r "and doubled?"                    # reply with the current thread as context
ask n                                   # no prompt words: type a multiline prompt at You>, Ctrl-D to submit
pbpaste | ask "remove blockquoting from this text"   # arguments instruct, stdin is the material
ask thread                              # show the current thread
ask switch                              # choose a recent thread to continue
```

`pbpaste` is the macOS clipboard command; any program that writes text to stdout works in its place. `ask thread`, `ask switch`, `ask stats`, and `ask help` need no key. `ask doctor` runs offline diagnostics; run it through your credential launcher so that it sees the same environment as your queries.

## Setting up with an agent?

[Setting up `ask` with a coding agent](docs/guides/agent-setup.md) explains how to give a coding agent the [`ask-doctor` skill](skills/ask-doctor/SKILL.md), a readable Markdown file of instructions for installing, configuring, and diagnosing `ask`.

## Documentation

Guides, by task:

- [Getting started](docs/guides/getting-started.md): install, configure, supply a credential, ask, and diagnose.
- [Injecting credentials](docs/guides/credentials.md): hidden prompt and credential-manager recipes.
- [Setting up `ask` with a coding agent](docs/guides/agent-setup.md): install the `ask-doctor` skill in a coding agent.
- [Configuring profiles and providers](docs/guides/configuration.md): edit, validate, and install a configuration; add profiles; set output limits and history expiry.
- [Threads and history](docs/guides/threads-and-history.md): reply, review, switch threads, read statistics, and expire old history.
- [Using `ask` in pipelines](docs/guides/shell-composition.md): piped input, output streams, and exit statuses.

Reference, for exact behavior:

- [Commands](docs/reference/commands.md)
- [Configuration](docs/reference/configuration.md)
- [Providers](docs/reference/providers.md)
- [Query behavior](docs/reference/query-behavior.md)
- [Local storage](docs/reference/storage.md)

Release operation: [nightly releases](docs/guides/nightly-releases.md), [stable releases](docs/guides/stable-releases.md) (prepared, not yet published), and [live-provider checks](docs/guides/live-provider-checks.md).

## Development

Rules for contributors and coding agents are in [AGENTS.md](AGENTS.md). [CONTRIBUTING.md](CONTRIBUTING.md) covers tooling setup, branch flow, and the verification gates (`just verify` before every commit, `just verify-full` for pull requests). [SECURITY.md](SECURITY.md) covers the dependency and release security policy.

## License

Apache-2.0. See [LICENSE](LICENSE).
