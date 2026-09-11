# `ask`

`ask` is a fast, opinionated terminal lookup tool for asking language models quick questions without starting an agent session.

**Status: pre-alpha.** The query, reply, initialization, and configuration commands are implemented, but interfaces may change before the first `0.1.0` release. The only provider kind currently supported is `openai-compatible`.

## Usage

The following forms each start a new query. Prompt words are joined with single spaces.

```sh
ask "what is 2+2"
ask new "what is 2+2"
ask n "what is 2+2"
```

`ask reply` and `ask r` continue the current thread; see [Threads and replies](#threads-and-replies).

`ask` reads `$ASK_HOME/config.toml` when `ASK_HOME` is set. Otherwise, it reads `config.toml` from the platform-standard configuration directory for an application named `ask`.

### Creating a first configuration interactively

`ask init` and `ask i` create that file through a line-oriented dialogue. Every prompt and diagnostic is written to stderr, and stdout stays empty. The answers may come from a terminal or from redirected stdin, one answer per line. The dialogue asks for:

1. A provider name, for example `openrouter`.
2. The endpoint base URL, which must use `http://` or `https://`, include a host, contain no embedded username/password credentials, and have no query or fragment component (including an empty trailing `?` or `#`).
3. A model identifier, sent to the provider after trimming surrounding whitespace.
4. The name of the environment variable that will hold the credential. `ask` validates the name only; it never reads or stores the value, and initialization makes no network request.
5. An optional replacement system prompt. The current default system prompt is shown first, and an empty answer keeps it.
6. A profile name, which defaults to `default`. The profile created becomes the default profile.

Answers are trimmed and limited to one line. For a multiline system prompt, an explicitly empty system prompt, or significant surrounding whitespace, prepare the complete TOML document and use `ask configure check/apply`.

An invalid answer prints a one-line explanation and asks again. The dialogue then shows the exact TOML it will write and asks for confirmation. Only `y` or `yes` writes the file; any other answer, or end of input at any prompt, cancels without writing and exits 1.

`ask init` has no noninteractive all-default mode, because a provider target cannot be inferred. Answering it from redirected stdin still requires an answer for every prompt.

`ask init` refuses to run when the configuration file already exists and leaves it unchanged; use `ask configure apply` to replace a regular file. Both commands refuse symlink destinations, including dangling symlinks; manage those paths explicitly before publishing a configuration. It requires hard-link support on the filesystem containing the configuration directory, including when `ASK_HOME` selects that directory: it publishes the fully written file with a hard link so that a configuration appearing during the dialogue is never overwritten. Both initialization and `apply` creation fail safely if the filesystem does not support hard links.

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

### Query input and output

`ask`, `ask new`, `ask n`, `ask reply`, and `ask r` resolve query input the same way:

- Redirected stdin without prompt words supplies the prompt.
- Prompt words with redirected stdin supply an instruction and an input payload, respectively. When stdin is an open pipe that carries no payload, such as under `ssh` without `-n` or in a job runner, `ask` waits for EOF; redirect stdin from `/dev/null` in that case.
- Terminal stdin with prompt words uses the words immediately, without reading stdin. Configuration, credential, database, and missing-current-thread problems are reported before any input is read or prompted for.
- Terminal stdin without prompt words opens a multiline prompt on stderr and reads the query from the terminal.

```sh
printf 'what is 2+2' | ask
printf '> first line\n> second line\n' | ask "remove blockquoting from this text"
ask n
```

The composition rule joins the instruction, a blank line, and the payload as `"{instruction}\n\n{payload}"`. Prompt words are joined with single spaces and surrounding whitespace is trimmed in every mode; the UTF-8 payload is preserved byte for byte, including leading whitespace and trailing newlines. An empty or whitespace-only payload, such as stdin redirected from `/dev/null`, leaves the instruction alone. Prompt words that are empty or whitespace-only are a usage error whether stdin is a terminal or redirected, and nothing is read. This is a compositional convenience, not a security boundary.

The multiline prompt prints `ask> ` on stderr before the first line and reads until EOF. At the start of a line, Ctrl-D sends EOF and submits the collected text unchanged. On a partially typed line, Ctrl-D first makes the terminal deliver the pending text; a second Ctrl-D submits. Ctrl-C cancels the multiline prompt: no provider request is sent and nothing is written to stdout. `ask` installs no signal handler and exits by the default SIGINT disposition (status 130 in most shells). Empty or whitespace-only input from either a terminal submission or redirected stdin without prompt words is rejected with exit 2 and a one-line stderr diagnostic; no provider request is sent.

Stdin must be valid UTF-8 and is read to EOF. There is no application-imposed size cap for stdin in 0.1.0. Invalid UTF-8 and input read failures exit 1 with one `ask: ...` stderr diagnostic line, empty stdout, and no provider request; invalid bytes are never converted lossily.

The answer is streamed to stdout as unstyled Markdown and ends with exactly one newline. Prompts, statistics, warnings, usage errors, and diagnostics are written to stderr. A successful query exits 0, usage errors exit 2, and provider and configuration failures exit 1. A streaming failure preserves any partial answer and reports the error on stderr. If the stdout reader closes early, `ask` exits 0 without a diagnostic, unless recording the partial turn fails.

### Threads and replies

`ask`, `ask new`, and `ask n` start a new thread. `ask reply` and `ask r` continue the current thread from a separate process:

```sh
ask "who was u.s. president in 1846"
ask r "who succeeded him"
```

A thread keeps the profile resolved when the thread was created: profile name, provider kind, base URL, model, system prompt, timeout, and the name of the credential environment variable, never its value. Replies use that snapshot even after the configuration's default profile changes, and they work when the installed configuration is missing or invalid; they need only the credential environment variable the snapshot names. A reply sends the system prompt, then each earlier complete turn of the thread in order as a user message followed by an assistant message, then the new prompt. The assistant message is the raw answer text the provider returned, without the final-newline normalization `ask` applies on stdout.

The current thread is global to the data directory. A thread becomes current when the command that created or continued it records its turn; when commands overlap, the last to finish wins. A reply reads its thread when it starts and appends only to that thread. `ask reply` with no current thread exits 1 with ``ask: no current thread; start one with `ask new` `` on stderr and sends no request.

Each query becomes a turn with the status complete or partial:

- A successful answer, including an empty one, is a complete turn.
- A provider or streaming failure after some answer text records a partial turn with the failure reason. Stdout keeps the partial answer and `ask` exits 1. On `ask new`, the partial turn still creates the thread and makes it current.
- If the stdout reader closes after answer text, `ask` records a partial turn with the reason `output closed` and exits 0 without a diagnostic.
- A failure before any answer text appends no turn, creates no thread, and leaves the current thread unchanged.
- Ctrl-C during streaming ends the process by the default SIGINT disposition, and nothing is recorded.

Partial turns are stored but never sent as context.

### Local history and statistics

`ask new` and `ask reply` store history and statistics in one SQLite database: `$ASK_HOME/data/ask.sqlite3` when `ASK_HOME` is set, otherwise `ask.sqlite3` in the platform-standard data directory for an application named `ask`. They create a missing data directory with mode `0700` and a missing database with mode `0600`, and leave existing permissions alone. `ask init` and `ask configure` never open the database. Both query commands open it before sending a request, so a database that cannot be opened, or whose schema version is newer than this `ask` supports, fails the command with exit 1 before any request.

The database records:

- threads, each with its profile snapshot;
- turns: the prompt, the raw answer text, the status, and the reason for a partial turn;
- one statistics row per query sent to the provider: start time, command, profile name, provider kind, base URL, model, outcome (complete, partial, or failed), error class (provider, timeout, or output), wall, API, and time-to-first-token durations, and token counts when the provider reports them. Statistics rows contain no prompt or answer text;
- provider health: for each provider target (provider kind, base URL, and model), only the time of the latest success and the time and error class of the latest failure. Output failures are not provider-health observations.

Credential values are never stored. History has no expiry yet; it is kept until the database is removed.

Everything one query records is written after the answer finishes or fails, in one transaction: the thread (for `ask new`), the turn, the statistics row, the provider-health update, and the current-thread change. Nothing is written while the answer streams. If the answer was delivered to stdout but that transaction fails, stdout keeps the answer, `ask` exits 1, and stderr reports `ask: answer was delivered but not recorded: <cause>` without repeating the prompt. If a query failed before any answer text and its statistics cannot be recorded, stderr adds `ask: query statistics were not recorded: <cause>`.

## Development

Rules for contributors and coding agents are in `AGENTS.md`. See `CONTRIBUTING.md` for tooling setup, branch flow, and verification gates.

Run the fast gate with `just verify` before every commit. Pull requests must pass `just verify-full`.

## License

Apache-2.0. See `LICENSE`.
