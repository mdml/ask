# Getting started

This guide takes a new installation of `ask` to a first answer: install the stable release, create a configuration, supply a provider credential, ask a question, and check the installation. Exact behavior for every command is in the [command reference](../reference/commands.md).

## Install the stable release

Install `v0.1.0` on macOS or Linux, arm64 or x86-64, with Homebrew:

```sh
brew install mdml/tap/ask
```

Or install the exact version with [mise](https://mise.jdx.dev/):

```sh
mise use -g 'github:mdml/ask@v0.1.0'
```

Confirm the installed version without reading configuration or contacting a provider:

```sh
ask version
```

Before installing an archive by hand, follow the checksum and GitHub attestation checks in [Stable releases](stable-releases.md#verify-a-downloaded-archive). [Nightly releases](nightly-releases.md) are also available for testing newer builds.

## Create a configuration

```sh
ask init
```

`ask init` (alias `ask i`) is a dialogue on stderr. It:

1. Offers a provider menu: OpenAI, Anthropic, Gemini, OpenRouter, or a custom OpenAI-compatible endpoint. For the four named providers, `ask init` supplies the endpoint and the credential environment-variable name, which are listed in the [provider reference](../reference/providers.md#initialization-presets). For a custom endpoint, you enter a provider name, an endpoint base URL, and a credential variable name.
2. Asks for a model identifier. This is free text sent to the provider; `ask` has no model catalog.
3. Shows the default system prompt and accepts an optional one-line replacement. An empty answer keeps the default.
4. Asks for a profile name, defaulting to `default`. This profile becomes the default profile.
5. Shows the exact TOML it will write and asks for confirmation. Only `y` or `yes` writes the file.

On an attended terminal, choose the provider with the arrow keys and Enter; Esc cancels. When stdin or stderr is redirected, or `TERM` is unset or `dumb`, the menu is numbered and you type the number. An invalid answer prints an explanation and asks again. Esc at the menu, any other confirmation answer, or end of input at any prompt cancels without writing and exits 1. `ask init` never replaces an existing configuration; to change one, see [Configuring profiles and providers](configuration.md).

`ask init` prints the path it writes. The file is `$ASK_HOME/config.toml` when `ASK_HOME` is set, and otherwise `config.toml` in the platform-standard configuration directory for an application named `ask`. `ask doctor` shows every resolved path.

`ask init` has no noninteractive all-default mode, because a provider target cannot be inferred. To set up `ask` without a dialogue, such as from a script or an agent, write a complete TOML document and install it with `ask configure apply`; see [Configuring profiles and providers](configuration.md).

## Supply a credential and ask a question

`ask` reads the provider key from the environment variable named in the configuration and never stores its value. `ask init` prints the variable name for your provider. In bash or zsh, this hidden prompt supplies the key to a single `ask` process, keeping it out of shell history and out of the parent shell:

```sh
( printf 'API key: ' >&2; IFS= read -rs OPENAI_API_KEY </dev/tty || exit; printf '\n' >&2; export OPENAI_API_KEY; exec ask "what is 2+2" )
```

Replace `OPENAI_API_KEY` with your provider's variable name, and paste the key only at the hidden prompt.

The key exists for that one command only. Every later command that contacts a provider (`ask`, `ask new`, `ask reply`, and `ask doctor --live`) needs the key again. For everyday use, have an external credential manager inject the key each time `ask` launches; [Injecting credentials](credentials.md) gives a recipe. The query examples in the rest of this guide and in the other guides assume `ask` runs through such a credential launcher.

The answer streams to stdout. A one-line statistics summary follows on stderr:

```text
4
ask: gpt-5.6-luna · 3.7s wall · 2.5s api · 3.1s to first token · 46 in / 8 out
```

The summary shows the model, total elapsed time, time spent in the provider request, time until the first answer text, and input and output token counts. Token counts are `?` when the provider does not report them.

## Continue, review, and compose

```sh
ask r "and doubled?"
ask thread
printf 'what is 2+2' | ask
```

`ask reply` (alias `ask r`) sends the current thread as context and, like every query, needs the credential. `ask thread` prints the thread, labelling each prompt `You:` and each answer `Assistant:`, and needs no credential. Piped stdin supplies the prompt, or the material an instruction acts on when prompt words are also present. See [Threads and history](threads-and-history.md) and [Using `ask` in pipelines](shell-composition.md).

## Check the installation

```sh
ask doctor
```

`ask doctor` (alias `ask d`) validates the installed configuration, shows every resolved path, inspects the history database read-only, and reports whether each required credential variable is present, never its value. It contacts no provider and changes nothing. Exit status 0 means no problem was found, 1 means a configuration problem, and 3 means an environmental-readiness problem such as a missing credential variable. Run it in the same way you run `ask`, for example through the same credential-manager launcher, so that it sees the same environment.

`ask doctor --live` additionally sends one minimal request to the default profile's provider target to confirm that the provider answers. It may incur provider cost, and `ask` warns on stderr before sending. `ask doctor --live --all` checks every configured provider target. Details are in the [command reference](../reference/commands.md#ask-doctor).
