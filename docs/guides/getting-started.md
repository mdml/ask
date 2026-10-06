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

1. Offers a provider menu: OpenAI, Anthropic, Gemini, OpenRouter, Groq, Cerebras, xAI, a local model server, or a custom OpenAI-compatible endpoint. For the named providers, `ask init` supplies the endpoint, listed in the [provider reference](../reference/providers.md#initialization-presets), and then asks for the credential environment-variable name with the provider's usual name in brackets, such as `Credential variable [OPENAI_API_KEY]: `. Press Enter to keep it, or type another name, such as `ASK_ANTHROPIC_API_KEY` if another tool changes its behavior when `ANTHROPIC_API_KEY` is set; `ask` then reads the key from that variable. For a custom endpoint, you enter a provider name, an endpoint base URL, and a credential variable name. A local model server is covered in [Use a local model server](#use-a-local-model-server).
2. Looks for the key. If the provider's variable is set, `ask init` uses it and says so without showing the value. Otherwise, when stdin and stderr are both terminals, whatever `TERM` is set to, it offers a hidden prompt where you can paste the key; nothing you type is echoed, and Enter skips. The key is used only during `ask init`, to list models and verify the setup, and is never written anywhere.
3. With a key, requests the provider's model list and offers it as a menu. Without a key, for a named provider (or a custom endpoint with a named provider's kind and endpoint), it offers the [model list the `ask` project publishes](../reference/providers.md#published-model-list) instead: it prints the URL, sends no credentials, and shows the date the list was generated. Type to narrow the list (case-insensitive), Backspace to edit, arrow keys to move, and Enter to choose; the last entry lets you type an identifier instead. If the list is unavailable, `ask init` says why and asks for the model identifier as free text. For a custom endpoint without a key, or with `ASK_MODEL_LIST_URL` set to an empty value, which disables the published list, it asks for the identifier as free text without another notice.
4. Shows the default system prompt and accepts an optional one-line replacement. An empty answer keeps the default.
5. Asks for a profile name, defaulting to `default`. This first profile becomes the default profile.
6. With a key, verifies the setup with one minimal request, the same one `ask doctor --live` sends, after a cost notice; a keyless target is verified the same way without the notice. If verification fails, `ask init` shows the reason and asks whether to write the configuration anyway; [Recipes](recipes.md#when-ask-init-cannot-verify-the-setup) lists common causes.
7. Asks whether to add another provider. Each additional provider repeats these steps and gets one profile, named after the provider by default.
8. Shows the exact TOML it will write and asks for confirmation. Only `y` or `yes` writes the file. It then prints next steps: how to supply the key and a first question to ask.

On an attended terminal, choose the provider with the arrow keys and Enter; Esc cancels. When stdin or stderr is redirected, or `TERM` is unset or `dumb`, the menus are numbered and you type the number. With redirected stdin, `ask init` never reads a key; it uses the key only if the variable is already set. An invalid answer prints an explanation and asks again. Esc at a menu or the hidden prompt, any other confirmation answer, or end of input at any prompt cancels without writing and exits 1. Run `ask init` again later to add a provider or a profile, change a profile's model, or set the default profile; see [Configuring profiles and providers](configuration.md).

`ask init` prints the path it writes. The file is `$ASK_HOME/config.toml` when `ASK_HOME` is set, and otherwise `config.toml` in the platform-standard configuration directory for an application named `ask`. `ask doctor` shows every resolved path.

`ask init` has no noninteractive all-default mode, because a provider target cannot be inferred. To set up `ask` without a dialogue, such as from a script or an agent, write a complete TOML document and install it with `ask configure apply`; see [Configuring profiles and providers](configuration.md).

## Use a local model server

`ask` connects to a server you already run: Ollama, LM Studio, or a llama.cpp server. It does not start servers or download models. Start the server and load a model first, then run `ask init`, choose `Local model server`, and choose your server. Press Enter to accept its default endpoint (Ollama `http://localhost:11434/v1`, LM Studio `http://localhost:1234/v1`, llama.cpp `http://localhost:8080/v1`) or type another base URL.

No key is involved. `ask init` asks the server for its models, offers them in the menu, and verifies the choice with one minimal request. If the server is unreachable, it says so, names how that server is usually started (for example `ollama serve`), and asks for the model identifier as free text. The provider uses a 120-second timeout (`timeout_ms = 120000`) because the first request often waits for the model to load. Then ask a question as usual; no credential setup is needed.

If you use a proxy (through `HTTP_PROXY`, `HTTPS_PROXY`, or `ALL_PROXY`, or on macOS the system settings), requests to a local server go to the proxy too unless `NO_PROXY` covers its host; set `NO_PROXY=localhost,127.0.0.1` so they reach the server directly ([recipe](recipes.md#reach-a-local-model-server-behind-a-corporate-proxy)). The [provider reference](../reference/providers.md) describes what a proxy can see.

## Supply a credential and ask a question

`ask` reads the provider key from the environment variable named in the configuration and never stores its value. A key pasted during `ask init` is not kept, so later commands need it again. `ask init` prints the variable name for your provider and this command in its next steps. In bash or zsh, this hidden prompt supplies the key to a single `ask` process, keeping it out of shell history and out of the parent shell:

```sh
( printf 'API key: ' >&2; IFS= read -rs OPENAI_API_KEY </dev/tty || exit; printf '\n' >&2; export OPENAI_API_KEY; exec ask "what is 2+2" )
```

Replace `OPENAI_API_KEY` with your provider's variable name, and paste the key only at the hidden prompt.

The key exists for that one command only. Every later command that contacts a provider (`ask`, `ask new`, `ask reply`, and `ask doctor --live`) needs the key again. For everyday use, have an external credential manager inject the key each time `ask` launches; [Injecting credentials](credentials.md) gives a recipe. To read the key from a variable other than the provider's standard one, see [Recipes](recipes.md#read-a-key-from-a-different-variable). The query examples in the rest of this guide and in the other guides assume `ask` runs through such a credential launcher.

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

`ask doctor` (alias `ask d`) validates the installed configuration, shows every resolved path, inspects the history database read-only, and, for the default profile's provider target, reports whether its credential variable is present, never its value (a keyless `openai-compatible` target reports `credential: not required`), and when that target was last observed healthy. It contacts no provider and changes nothing. Exit status 0 means no problem was found, 1 means a configuration problem, and 3 means an environmental-readiness problem such as a missing credential variable for that target. Run it in the same way you run `ask`, for example through the same credential-manager launcher, so that it sees the same environment.

`ask doctor --live` additionally sends one minimal request to the default profile's provider target to confirm that the provider answers. It may incur provider cost, and `ask` warns on stderr before sending. `ask doctor --live --all` checks every configured provider target. Details are in the [command reference](../reference/commands.md#ask-doctor).
