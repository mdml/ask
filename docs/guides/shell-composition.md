# Using `ask` in pipelines

`ask` follows one contract so that it composes with other terminal programs: a query command writes only the answer to stdout, and writes prompts, statistics, warnings, and diagnostics to stderr. `ask` has no output hooks or formatter settings of its own; pipe its output to the program you want. Exact rules are in [query behavior](../reference/query-behavior.md).

## Send input

Query commands (`ask`, `ask new`, `ask n`, `ask reply`, `ask r`) choose their input from what they are given:

| Prompt words | Stdin | Result |
|:--|:--|:--|
| present | terminal | The words are the prompt. Stdin is not read. |
| present | pipe or file | The words are the instruction and stdin is the material it acts on. |
| absent | pipe or file | Stdin is the prompt. |
| absent | terminal | A multiline `You> ` prompt opens on stderr. Ctrl-D at the start of a line submits; Ctrl-C cancels without sending anything. |

```sh
printf 'what is 2+2' | ask
printf '> first line\n> second line\n' | ask "remove blockquoting from this text"
ask "summarize this file in three bullets" < notes.txt
```

With both an instruction and piped material, `ask` sends one message: the instruction, a blank line, then the material exactly as received. Stdin must be valid UTF-8 and is read to the end; `ask` applies no size cap of its own. Blank input is a usage error (exit 2) and sends nothing.

In a job runner, or under `ssh` without `-n`, stdin can be an open pipe that never closes, and `ask` waits for it. Redirect stdin from `/dev/null` there:

```sh
ask "what is 2+2" < /dev/null
```

## Use the output

The answer is unstyled Markdown with no ANSI styling and ends with exactly one newline, so it can be captured, redirected, or formatted:

```sh
answer="$(ask "what is the capital of France? reply with the city only")"
ask "write a haiku about pipes" > haiku.md
ask "explain tar flags in a table" | less
```

Any Markdown renderer that reads stdin can format the answer; `ask` needs no configuration for it. The statistics line is on stderr, so it stays on the terminal and out of the pipe. To silence it, redirect stderr: `ask "what is 2+2" 2>/dev/null`. That also hides warnings and errors, so rely on the exit status.

If the reader closes the pipe early, as `head` does, `ask` stops quietly and exits 0. The part of the answer that was delivered is recorded as a partial turn.

`ask thread` and `ask stats` write their requested output to stdout in the same way, and also exit 0 quietly when the reader closes early.

## Check the exit status

| Status | Meaning |
|:--|:--|
| 0 | The answer completed. |
| 1 | A configuration, credential, storage, or provider failure; an answer stopped by a refusal or the output-token limit; or an answer that was delivered but could not be recorded. |
| 2 | A usage error, including blank input. |
| 3 | `ask doctor` only: an environmental-readiness problem. |

Interrupting `ask` with Ctrl-C ends the process by the default SIGINT disposition, which most shells report as status 130.

A failure during streaming can leave partial answer text on stdout, with the error on stderr and a nonzero status. A script that must not act on a partial answer should check the status before using the output:

```sh
if answer="$(ask "what is 2+2" < /dev/null)"; then
  printf '%s\n' "$answer"
fi
```

Supply the provider credential to the script's `ask` process in the same way as for interactive use; see [Injecting credentials](credentials.md).
