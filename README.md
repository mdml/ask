# `ask`

`ask` is a fast, opinionated terminal lookup tool for asking language models quick questions without starting an agent session.

**Status: pre-alpha.** The query, initialization, and configuration commands are implemented, but interfaces may change before the first `0.1.0` release. The only provider kind currently supported is `openai-compatible`.

## Usage

The following forms each start a new query. Prompt words are joined with single spaces.

```sh
ask "what is 2+2"
ask new "what is 2+2"
ask n "what is 2+2"
```

`ask` reads `$ASK_HOME/config.toml` when `ASK_HOME` is set. Otherwise, it reads `config.toml` from the platform-standard configuration directory for an application named `ask`.

### Creating a first configuration interactively

`ask init` and `ask i` create that file through a line-oriented dialogue. Every prompt and diagnostic is written to stderr, and stdout stays empty. The answers may come from a terminal or from redirected stdin, one answer per line. The dialogue asks for:

1. A provider name, for example `openrouter`.
2. The endpoint base URL, which must use `http://` or `https://`, include a host, contain no embedded username/password credentials, and have no query or fragment component (including an empty trailing `?` or `#`).
3. A model identifier, sent to the provider as typed.
4. The name of the environment variable that will hold the credential. `ask` validates the name only; it never reads or stores the value, and initialization makes no network request.
5. An optional replacement system prompt. The current default system prompt is shown first, and an empty answer keeps it.
6. A profile name, which defaults to `default`. The profile created becomes the default profile.

An invalid answer prints a one-line explanation and asks again. The dialogue then shows the exact TOML it will write and asks for confirmation. Only `y` or `yes` writes the file; any other answer, or end of input at any prompt, cancels without writing and exits 1.

`ask init` has no noninteractive all-default mode, because a provider target cannot be inferred. Answering it from redirected stdin still requires an answer for every prompt.

`ask init` refuses to run when the configuration file already exists and leaves it unchanged; use `ask configure apply` to install a replacement. It requires hard-link support on the filesystem containing the configuration directory, including when `ASK_HOME` selects that directory: it publishes the fully written file with a hard link so that a configuration appearing during the dialogue is never overwritten. Both initialization and `apply` creation fail safely if the filesystem does not support hard links.

A completed dialogue writes a file in the schema below, omitting `timeout_ms` and `system_prompt` when the defaults apply.

### Managing a complete configuration document

`ask configure check [FILE|-]` and `ask configure apply [FILE|-]`, aliased `ask c check` and `ask c apply`, read one complete TOML document and validate it. `FILE` names a file; `-` and an omitted argument both read the document from standard input. Omitting the argument while stdin is a terminal is a usage error; name a file or redirect a file or pipe into the command. An explicit `-` selects stdin.

Both commands apply the same strict validator that ordinary query loading applies to the installed file, so a document `ask configure check` accepts is a document `ask` can run with. The validator:

- rejects any key that is not part of the schema, at every level of the document;
- validates every provider and every profile, not only the ones the default profile selects;
- requires every profile to reference a configured provider, and `default_profile` to name a configured profile;
- requires each provider's `kind` to be supported, its `base_url` to satisfy the endpoint rule above, its `api_key_env` to be a valid environment variable name, and its `timeout_ms` to be greater than zero;
- requires a nonempty `model` and nonempty provider and profile names. An omitted `system_prompt` selects the default; an explicit string replaces it, including an empty string.

`ask configure check` writes nothing: it never creates, replaces, or removes a file. Both commands exit 0 on success with a one-line message on stderr and exit 1 with a one-line diagnostic on stderr when the document is invalid or cannot be read; stdout stays empty in every case. Neither command reads a credential value or contacts a provider.

Validation diagnostics contain only schema field names and error categories, with one-based entry/key indexes in sorted key order. They never echo candidate-controlled keys, provider/profile names, reference values, or parser messages. Invalid TOML syntax reports a one-based line and column instead.

`ask configure apply` installs the candidate's exact bytes, preserving comments, key order, and spacing. It reports whether it created or replaced the configuration:

1. It acquires an exclusive advisory lock on `.ask-config.lock` in the configuration directory, then snapshots the destination before reading the candidate. A competing `ask` writer fails promptly with a lock diagnostic. `init` takes the same lock when publishing its confirmed document.
2. It validates the candidate, then writes and syncs a temporary file in that directory. New files use owner-only permissions (`0600`, subject to umask); replacements retain the destination's Unix read/write/execute permission bits. Symlink and nonregular destinations are refused.
3. It checks the destination's contents, device/inode identity, permissions, and change timestamp against the snapshot. A detected change aborts publication.
4. It atomically renames the temporary file over an existing destination, or hard-links it to an absent destination without overwriting a file that appeared concurrently. Readers see a complete old or new document. Failed temporary writes and failed publication leave the destination intact; temporary files are removed on normal error returns when directory permissions permit cleanup.

The lock file is persistent and must not be removed while writers may be running: unlinking it could give overlapping writers different lock inodes. An unsuccessful `apply` can leave the configuration directory and this empty lock file behind. Cooperating `ask` writers cannot overlap publication. External editors that ignore the advisory lock can still change the file or directory after the snapshot check and before rename; this is not an operating-system compare-and-swap guarantee. The configuration directory and lock inode must remain stable while writers run. Abrupt termination may leave a temporary file, and syncing the temporary file does not guarantee directory-entry durability across a crash.

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

`api_key_env` names the environment variable that supplies the credential; `ask` does not store credential values. The request timeout defaults to 30 seconds and must be greater than zero. When a profile sets no `system_prompt`, `ask` sends this default system prompt: "Answer briefly in plain Markdown suitable for a terminal." A profile's `system_prompt` replaces it. Retention and display settings are not yet implemented.

The answer is streamed to stdout as unstyled Markdown and ends with exactly one newline. Statistics, warnings, usage errors, and diagnostics are written to stderr. A successful query exits 0, and provider and configuration failures exit 1. If the stdout reader closes early, `ask` exits 0 without a diagnostic. Running `ask` with no prompt words currently prints a usage message and exits 2; the intended behavior, an interactive multiline `>` prompt, is not yet implemented.

## Development

Rules for contributors and coding agents are in `AGENTS.md`. See `CONTRIBUTING.md` for tooling setup, branch flow, and verification gates.

Run the fast gate with `just verify` before every commit. Pull requests must pass `just verify-full`.

## License

Apache-2.0. See `LICENSE`.
