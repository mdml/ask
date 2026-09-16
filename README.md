# `ask`

`ask` is a fast, opinionated terminal lookup tool for asking language models quick questions without starting an agent session.

**Status: pre-alpha.** The query, reply, recall (`thread`, `switch`, `stats`), initialization, and configuration commands are implemented, but interfaces may change before the first `0.1.0` release. Supported provider kinds are `openai`, `anthropic`, `gemini`, `openrouter`, and `openai-compatible`; `ask init` still creates only an `openai-compatible` provider, so write other kinds with `ask configure apply`.

## Install a nightly

The [nightly pipeline](docs/guides/nightly-releases.md) publishes checksummed, attested prereleases for macOS and Linux on arm64 and x86-64 after the full gate. Choose a tag from [GitHub Releases](https://github.com/mdml/ask/releases), then install with `mise use -g 'github:mdml/ask[prerelease=true]@TAG'`. These builds contain the implemented pre-alpha command surface described below.

## Usage

The following forms each start a new query. Prompt words are joined with single spaces.

```sh
ask "what is 2+2"
ask new "what is 2+2"
ask n "what is 2+2"
```

`ask reply` and `ask r` continue the current thread; see [Threads and replies](#threads-and-replies). `ask thread`, `ask switch`, and `ask stats` inspect and select history; see [Recall](#recall).

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
- requires a nonempty `model` and nonempty provider and profile names. An omitted `system_prompt` selects the default; an explicit string replaces it, including an empty string;
- requires a profile's `max_output_tokens`, when present, to be an integer greater than zero.

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
# max_output_tokens = 4096
```

History expiry is optional and off by default. To enable it, add top-level keys before the first table:

```toml
expire_history = true
# history_days = 90
```

`api_key_env` names the environment variable that supplies the credential; `ask` does not store credential values.

### Provider kinds

Each `kind` uses its provider's own HTTP API with provider-specific streaming normalization over server-sent events. `base_url` is the prefix `ask` appends the API path to; the provider documentation's public endpoint is shown for each kind, and any endpoint serving the same API may be configured instead.

| `kind` | API | Typical `base_url` | Request path | Credential sent as |
|:--|:--|:--|:--|:--|
| `openai` | OpenAI Responses, with `store: false` | `https://api.openai.com/v1` | `/responses` | `Authorization: Bearer` header |
| `anthropic` | Anthropic Messages | `https://api.anthropic.com` | `/v1/messages` | `x-api-key` header |
| `gemini` | Gemini GenerateContent | `https://generativelanguage.googleapis.com` | `/v1beta/models/{model}:streamGenerateContent?alt=sse&key={credential}` | `key` URL query parameter |
| `openrouter` | OpenRouter Chat Completions | `https://openrouter.ai/api/v1` | `/chat/completions` | `Authorization: Bearer` header |
| `openai-compatible` | OpenAI Chat Completions | the server's OpenAI-compatible prefix, for example `http://127.0.0.1:PORT/v1` | `/chat/completions` | `Authorization: Bearer` header |

Model identifiers are free text and are never checked against a catalog. Every kind except `gemini` sends the identifier verbatim in the request body. Gemini carries the model in the URL path, so `ask` percent-encodes every byte outside letters, digits, `-`, `.`, `_`, and `~`; the identifier stays one path segment (a `/`, `?`, `#`, or space cannot change the endpoint) and the provider receives it unchanged after decoding. The Gemini credential is percent-encoded the same way in the query string. Because that credential is part of the request URL, transport diagnostics can quote it; `ask` replaces every occurrence of a credential in a diagnostic with `[redacted]`, whether literal or percent-encoded.

No kind follows HTTP redirects. Rig normalizes streaming responses for every kind except Gemini. Gemini uses a scoped adapter for GenerateContent SSE because Rig 0.42 drops `promptFeedback` and usage metadata from responses without candidates. The adapter handles the first candidate's text parts, excludes `thought: true` parts, retains the latest usage metadata, and maps the terminal reasons needed by the current text-only contract; malformed records and unknown, error, or protocol terminal reasons fail the request.

The provider reporting its output-token limit is handled as described in [Output-token limit](#output-token-limit), and a stream that ends with the in-band `error` finish reason (OpenRouter's mid-stream error chunk) is a provider failure, never a complete answer. A content-filter or refusal ending is also a provider failure. Refusal text already delivered remains on stdout and is recorded only as a partial turn; an empty refusal records no turn. Refused turns are excluded from reply context.

### Output-token limit

A profile may set `max_output_tokens`. An explicit value is sent to every kind in that API's field: `max_output_tokens` (`openai`), `max_tokens` (`anthropic`, `openrouter`, and `openai-compatible`; Rig may respell it `max_completion_tokens` for some OpenAI model identifiers), or `generationConfig.maxOutputTokens` (`gemini`). When the profile omits it, `anthropic` requests, which require a limit, send `4096`; every other kind sends no limit and keeps the provider's default.

When the provider reports that the answer stopped at the output-token limit, stdout keeps the answer received so far, stderr carries the one-line warning `ask: warning: answer stopped at the provider's output-token limit; set a larger max_output_tokens in the profile`, and `ask` exits 1. The turn is recorded as partial, even when no answer text arrived, so replies do not send it as context. Its statistics row has the outcome partial and the error class `output_limit`, and the provider target is recorded as healthy because it answered normally. Changing the profile affects new threads; replies continue using the profile snapshot captured when their thread was created, so start a new thread to use the larger limit. The request timeout defaults to 30 seconds and must be greater than zero. When a profile sets no `system_prompt`, `ask` sends this default system prompt: "Answer briefly in plain Markdown suitable for a terminal." A profile's `system_prompt` replaces it. `expire_history` defaults to `false`, which keeps history indefinitely. `history_days` must be a positive integer, defaults to 90 when `expire_history = true`, and is rejected when expiry is not enabled; see [History expiry](#history-expiry). Display settings are not yet implemented.

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

The answer is streamed to stdout as unstyled Markdown and ends with exactly one newline. Prompts, statistics, warnings, usage errors, and diagnostics are written to stderr. A successful query exits 0, usage errors exit 2, and provider and configuration failures exit 1. A streaming failure or explicit provider refusal preserves any text already delivered on stdout and reports the error on stderr. An ordinary successful answer with no text is still complete. `ask` does not follow HTTP redirects from the provider endpoint, whether they point to another origin or the same one: a redirect response is a provider failure that exits 1 with a diagnostic naming the status, the credential and query are not resent anywhere, no turn is recorded, and the failure is recorded in statistics and provider health. If the stdout reader closes early, `ask` exits 0 without a diagnostic, unless the provider had already failed or recording the partial turn fails; either of those exits 1 with its diagnostic on stderr.

### Threads and replies

`ask`, `ask new`, and `ask n` start a new thread. `ask reply` and `ask r` continue the current thread from a separate process:

```sh
ask "who was u.s. president in 1846"
ask r "who succeeded him"
```

A thread keeps the profile resolved when the thread was created: profile name, provider kind, base URL, model, system prompt, `max_output_tokens` when the profile set one, timeout, and the name of the credential environment variable, never its value. Threads recorded before profiles could set `max_output_tokens` have no limit in their snapshot, so their replies follow the omitted-limit rule above. Replies use that snapshot even after the configuration's default profile changes, and they work when the installed configuration is missing or invalid; they need only the credential environment variable the snapshot names. When the configuration cannot be read, a reply first reports `ask: history expiry skipped: <cause>` on stderr; see [History expiry](#history-expiry). A reply sends the system prompt, then each earlier complete turn of the thread in order as a user message followed by an assistant message, then the new prompt. The assistant message is the raw answer text the provider returned, without the final-newline normalization `ask` applies on stdout.

The current thread is global to the data directory. A thread becomes current when the command that created or continued it records its turn, or when `ask switch` selects it; when commands overlap, the last to finish wins. A reply reads its thread when it starts, before any input prompt, and appends only to that thread, even if `ask switch` selects another thread while the reply waits for input. `ask reply` with no current thread exits 1 with ``ask: no current thread; start one with `ask new` `` on stderr and sends no request.

Each query becomes a turn with the status complete or partial:

- A successful answer, including an empty one, is a complete turn.
- An explicit content-filter or refusal ending fails the query. Refusal text already written to stdout becomes a partial turn; an empty refusal appends no turn. Reported token usage is retained in either case.
- A provider or streaming failure after some answer text records a partial turn with the failure reason. Stdout keeps the partial answer and `ask` exits 1. On `ask new`, the partial turn still creates the thread and makes it current.
- If the stdout reader closes after answer text, `ask` records a partial turn with the reason `output closed` and exits 0 without a diagnostic. If a provider or streaming failure had already stopped the answer, that failure is the recorded reason, it counts as a provider-health failure, and `ask` exits 1 as above.
- A failure before any answer text appends no turn, creates no thread, and leaves the current thread unchanged.
- Ctrl-C during streaming ends the process by the default SIGINT disposition, and nothing is recorded.

Partial turns are stored but never sent as context.

### Local history and statistics

`ask new` and `ask reply` store history and statistics in one SQLite database: `$ASK_HOME/data/ask.sqlite3` when `ASK_HOME` is set, otherwise `ask.sqlite3` in the platform-standard data directory for an application named `ask`. They create a missing data directory with mode `0700` and a missing database with mode `0600`, and leave existing permissions alone. `ask init` and `ask configure` never open the database. `ask thread`, `ask switch`, and `ask stats` open it only when it already exists and never create it. Both query commands open it before sending a request, so a database that cannot be opened, or whose schema version is newer than this `ask` supports, fails the command with exit 1 before any request.

The database records:

- threads, each with its profile snapshot;
- turns: the prompt, the raw answer text, the status, and the reason for a partial turn;
- one statistics row per query sent to the provider: start time, command, profile name, provider kind, base URL, model, outcome (complete, partial, or failed), error class (provider, timeout, output, or output_limit), wall, API, and time-to-first-token durations, and token counts when the provider reports them. Statistics rows contain no prompt or answer text;
- provider health: for each provider target (provider kind, base URL, and model), only the time of the latest success and the time and error class of the latest failure. Output failures are not provider-health observations, and an answer stopped at the output-token limit counts as a success;
- the total number of threads removed by history expiry, the time of the latest removal, and the highest thread id observed before expiry, so removed ids are never reused.

Credential values are never stored. The database schema is version 3; a version 1 database from an earlier `ask` is upgraded in place through version 2 (history expiry) to version 3 (output-token snapshots), in one transaction, the first time any command opens it. A version 2 database with history expiry but no output-token snapshot column is upgraded to version 3 the same way. An unpublished prototype layout that marked version 2 with only an output-token column is refused.

Everything one query records is written after the answer finishes or fails, in one transaction: the thread (for `ask new`), the turn, the statistics row, the provider-health update, and the current-thread change. Nothing is written while the answer streams. If the answer was delivered to stdout but that transaction fails, stdout keeps the answer, `ask` exits 1, and stderr reports `ask: answer was delivered but not recorded: <cause>` without repeating the prompt. If a query failed before any answer text and its statistics cannot be recorded, stderr adds `ask: query statistics were not recorded: <cause>`.

### History expiry

History is kept indefinitely unless the installed configuration sets `expire_history = true`. Then `ask new` and `ask reply` remove every whole thread whose newest turn, complete or partial, is more than `history_days` days (default 90) older than the present. The command reads these settings when it starts. Removal runs in its own transaction only after valid query input is submitted and before the request is sent; a cancelled prompt (Ctrl-C), blank input, or any other usage or input error leaves history unchanged. It removes each thread's turns and, if the current thread is removed, the current-thread selection. A thread continued within the period is kept however old its first turn is. Thread ids are never reused: a new thread's id is higher than any id expiry has removed.

`ask reply` captures the current thread when it starts. If that thread no longer exists once input is submitted, whether this reply's expiry or another command removed it, the reply reports ``ask: no current thread; start one with `ask new` `` and sends no request.

Statistics rows and provider health survive expiry. `ask stats` reports the number of threads cleared. `ask thread`, `ask switch`, `ask stats`, `ask init`, and `ask configure` never remove history, so an old thread stays visible and selectable until the next `ask new` or `ask reply`.

`ask reply` reads the installed configuration only for these two settings. When the configuration is missing or invalid, the reply cannot read them: once input is submitted it reports `ask: history expiry skipped: <cause>` on stderr, removes nothing, and continues the current thread with its captured profile.

Another `ask new` or `ask reply` that applies expiry while a reply is streaming may remove that reply's thread, for example when the thread's newest turn ages past the retention period in the meantime or the other command uses a shorter period. The reply's record transaction then fails as a whole: stdout keeps the answer, `ask` exits 1 with `ask: answer was delivered but not recorded: <cause>`, and neither its turn, its statistics row, its provider-health observation, nor a current-thread change is recorded.

## Recall

`ask thread` and `ask t` write the full current thread to stdout and exit 0. The first line is `thread <id> · profile <profile> · model <model>`. Each turn follows after a blank line: the prompt with every line quoted as Markdown (`> `), a blank line, and the raw answer without trailing line endings. A partial turn ends with a line `[incomplete: <reason>]`; an empty answer prints nothing for the answer. Blocks are separated by one blank line, and the output ends with a newline. With no current thread, or no database, `ask thread` exits 1 with ``ask: no current thread; start one with `ask new` `` on stderr. Answer text is printed as stored, like query output.

`ask switch <id>` and `ask s <id>` make thread `<id>` current and report `ask: current thread is now <id>` on stderr; stdout stays empty. The id is the number `ask thread` and the `ask switch` menu show, written as decimal digits. A missing id exits 1 with `ask: no thread with id <id>`; an id that is not a positive decimal integer is a usage error (exit 2).

`ask switch` and `ask s` without an id list up to 10 threads on stderr, most recently continued first, then read one line from stdin, from a terminal or redirected:

```text
 1. thread 3 (current) · 2026-09-16 14:02 UTC · 2 turns · default · fake-model · who was u.s. president in 1846
 2. thread 1 · 2026-09-15 09:30 UTC · 1 turn · default · fake-model · remove blockquoting from this text
select a thread [1-2]:
```

Each entry shows the thread id, the time of its newest turn in UTC, the turn count, the profile, the model, and up to 60 characters of the first line of its first prompt; in the profile, model, and prompt, control characters and invisible Unicode format characters are replaced by spaces: bidirectional controls (U+061C, U+200E, U+200F, U+202A–U+202E, U+2066–U+2069), zero-width and other invisible characters (U+00AD, U+180E, U+200B, U+2060–U+2064, U+206A–U+206F, U+FEFF), interlinear annotation (U+FFF9–U+FFFB), and tag characters (U+E0001, U+E0020–U+E007F). The zero-width joiner and non-joiner (U+200C, U+200D), which shape emoji and scripts such as Persian, are kept, as is all other visible text. Entering a listed number selects that thread. Empty input, EOF, or anything else exits 2 with `ask: selection must be a number from 1 to <n>; current thread unchanged`. With no threads, `ask switch` exits 1 with ``ask: no threads; start one with `ask new` ``. Selection does not contact a provider, read the configuration, or remove expired history. The next `ask reply` continues the selected thread with its captured profile.

`ask stats` writes a local summary to stdout and exits 0. It has no one-character alias. It reads only the database, contains no prompt or answer text, and never contacts a provider:

```text
queries: 12 · 10 complete · 1 partial · 1 failed
tokens: 1480 in / 322 out · reported by 11 queries
median complete query: 2.1s wall · 0.8s to first token
history: 4 threads · 9 turns · 3 threads cleared by expiry

provider targets (historical observations, not a current check):
openai-compatible · https://openrouter.ai/api/v1 · openai/gpt-5.6-luna
  12 queries · last observed healthy 2026-09-16 14:02 UTC · last failure 2026-09-12 08:11 UTC (timeout)
```

Query counts include every recorded query, including those later removed from history. Token totals sum the counts providers reported, and `reported by` counts the queries that reported them. Medians are lower medians over complete queries, truncated to tenths of a second, and `-` when there is none. Each provider target line shows its recorded queries, its latest success as `last observed healthy` (or `never observed healthy`), and its latest failure with its error class (or `no failures observed`). These are historical observations, not a health check. Without a database, `ask stats` prints zero counts and creates nothing.

`ask thread` and `ask stats` take no arguments; extra arguments are a usage error. If a reader closes their stdout early, they exit 0 without a diagnostic.

## Development

Rules for contributors and coding agents are in `AGENTS.md`. See `CONTRIBUTING.md` for tooling setup, branch flow, and verification gates.

Run the fast gate with `just verify` before every commit. Pull requests must pass `just verify-full`.

## License

Apache-2.0. See `LICENSE`.
