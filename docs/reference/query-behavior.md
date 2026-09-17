# Query behavior

This page specifies what the query commands (`ask`, `ask new`, `ask n`, `ask reply`, and `ask r`) read, write, and record. For a task-oriented introduction, see [Using `ask` in pipelines](../guides/shell-composition.md) and [Threads and history](../guides/threads-and-history.md).

## Order of checks

Configuration, credential, database, and missing-current-thread problems are reported before any input is read or prompted for. The database is opened before a request is sent, so a database that cannot be opened, or whose schema version is newer than this `ask` supports, fails the command with exit 1 before any request.

## Input

All query commands resolve input the same way:

- Terminal stdin with prompt words uses the words immediately, without reading stdin.
- Terminal stdin without prompt words opens a multiline prompt on stderr and reads the query from the terminal.
- Redirected stdin without prompt words supplies the prompt.
- Redirected stdin with prompt words supplies an input payload, and the words are the instruction.

```sh
printf 'what is 2+2' | ask
printf '> first line\n> second line\n' | ask "remove blockquoting from this text"
ask n
```

Prompt words are joined with single spaces, and surrounding whitespace is trimmed in every mode. Prompt words that are empty or whitespace-only are a usage error whether stdin is a terminal or redirected, and nothing is read.

### Instruction and payload

The instruction and payload form one user message: the instruction, a blank line, and the payload, as `"{instruction}\n\n{payload}"`. The UTF-8 payload is preserved byte for byte, including leading whitespace and trailing newlines. An empty or whitespace-only payload, such as stdin redirected from `/dev/null`, leaves the instruction alone. This is a compositional convenience, not a security boundary: the model receives the instruction and the payload as one message.

When stdin is an open pipe that carries no payload, such as under `ssh` without `-n` or in a job runner, `ask` waits for end of input. Redirect stdin from `/dev/null` in that case.

Stdin must be valid UTF-8 and is read to end of input. `ask` imposes no size cap on stdin. Invalid UTF-8 and input read failures exit 1 with one `ask: ...` diagnostic line on stderr, empty stdout, and no provider request; invalid bytes are never converted lossily.

### Multiline prompt

The multiline prompt prints `You> ` on stderr before the first line and reads until end of input. At the start of a line, Ctrl-D submits the collected text unchanged. On a partially typed line, Ctrl-D first makes the terminal deliver the pending text; a second Ctrl-D submits. Ctrl-C cancels: no provider request is sent, nothing is written to stdout, and nothing is recorded. `ask` installs no signal handler and exits by the default SIGINT disposition (status 130 in most shells).

Empty or whitespace-only input, from either a terminal submission or redirected stdin without prompt words, is rejected with exit 2 and a stderr diagnostic. No provider request is sent.

## Output

The answer is streamed to stdout as unstyled Markdown and ends with exactly one newline. Prompts, the statistics line, warnings, usage errors, and diagnostics are written to stderr. Each diagnostic, warning, and statistics line begins `ask: `; a command-line syntax error also carries the [usage synopsis](commands.md#synopsis).

After an answer, the statistics line has this form:

```text
ask: gpt-5.6-luna · 3.7s wall · 2.5s api · 3.1s to first token · 46 in / 8 out
```

It shows the model, total elapsed time, time in the provider request, time to the first answer text, and input and output token counts. Token counts are `?` when the provider does not report them.

## Exit status

| Status | Meaning |
|:--|:--|
| 0 | The answer completed, or the stdout reader closed early without another failure. |
| 1 | A configuration, credential, input-read, storage, or provider failure, including a refusal, a redirect, and an answer stopped at the output-token limit; or an answer that was delivered but could not be recorded. |
| 2 | A usage error, including blank input. |

A streaming failure or explicit provider refusal preserves any text already delivered on stdout and reports the error on stderr. Provider response endings are described in the [provider reference](providers.md#response-endings).

If the stdout reader closes early, `ask` exits 0 without a diagnostic, unless the provider had already failed or recording the partial turn fails; either of those exits 1 with its diagnostic on stderr.

## Threads and replies

`ask`, `ask new`, and `ask n` start a new thread. `ask reply` and `ask r` continue the current thread from a separate process.

A thread keeps the profile resolved when the thread was created: profile name, provider kind, base URL, model, system prompt, `max_output_tokens` when the profile set one, timeout, and the name of the credential environment variable, never its value. Replies use that snapshot even after the configuration changes, and they work when the installed configuration is missing or invalid; they need only the credential environment variable that the snapshot names. Threads recorded before profiles could set `max_output_tokens` have no limit in their snapshot, so their replies follow the [omitted-limit rule](providers.md#output-token-limit).

A reply sends the system prompt, then each earlier complete turn of the thread in order as a user message followed by an assistant message, then the new prompt. The assistant message is the raw answer text the provider returned, without the final-newline normalization that `ask` applies on stdout. Partial turns are stored but never sent as context.

The current thread is global to the data directory. A thread becomes current when the command that created or continued it records its turn, or when `ask switch` selects it; when commands overlap, the last to finish wins. A reply reads its thread when it starts, before any input prompt, and appends only to that thread, even if `ask switch` selects another thread while the reply waits for input.

`ask reply` with no current thread exits 1 with ``ask: no current thread; start one with `ask new` `` on stderr and sends no request. The same applies when the thread captured at start no longer exists once input is submitted, for example because [history expiry](storage.md#history-expiry) removed it.

## Turn status

Each query becomes a turn with the status complete or partial:

- A successful answer, including an empty one, is a complete turn.
- An explicit content-filter or refusal ending fails the query. Refusal text already written to stdout becomes a partial turn; an empty refusal appends no turn. Reported token usage is retained in either case.
- An answer stopped at the provider's output-token limit is a partial turn, even when no answer text arrived.
- A provider or streaming failure after some answer text records a partial turn with the failure reason. Stdout keeps the partial answer and `ask` exits 1. On `ask new`, the partial turn still creates the thread and makes it current.
- If the stdout reader closes after answer text, `ask` records a partial turn with the reason `output closed` and exits 0 without a diagnostic. If a provider or streaming failure had already stopped the answer, that failure is the recorded reason, it counts as a provider-health failure, and `ask` exits 1.
- A failure before any answer text appends no turn, creates no thread, and leaves the current thread unchanged. Its statistics and provider-health observation are still recorded.
- Ctrl-C during streaming ends the process by the default SIGINT disposition, and nothing is recorded.

What is stored for each turn, and the atomicity of recording, are specified in [local storage](storage.md).
