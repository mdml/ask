# Threads and history

Every query is recorded locally as a turn in a thread. This guide covers continuing a thread, reviewing it, switching between threads, reading statistics, and expiring old history. Query examples assume `ask` runs through a credential launcher; see [Injecting credentials](credentials.md). Exact formats and edge cases are in the [command reference](../reference/commands.md), [query behavior](../reference/query-behavior.md), and [local storage](../reference/storage.md).

## Continue the current thread

`ask`, `ask new`, and `ask n` start a new thread, which becomes the current thread. `ask reply` and `ask r` continue the current thread, sending its earlier complete turns as context:

```sh
ask "who was u.s. president in 1846"
ask r "who succeeded him"
```

A thread keeps the profile it was created with: provider, endpoint, model, system prompt, output-token limit, timeout, and the name of the credential variable. Replies use that captured profile even after the configuration changes, and they work when the installed configuration is missing or invalid. With no current thread, `ask reply` exits 1 with ``ask: no current thread; start one with `ask new` `` and sends nothing.

The current thread is shared by every shell that uses the same data directory. When commands overlap, the last one to finish sets it.

An answer that was cut short by a provider failure, a refusal, the output-token limit, or a closed pipe is recorded as a partial turn. Partial turns appear in `ask thread` marked `[incomplete: <reason>]` and are never sent as reply context.

## Review the current thread

```sh
ask thread
```

`ask thread` (alias `ask t`) writes the whole current thread to stdout: a header line, then each prompt under a `You:` label and each answer under an `Assistant:` label:

```text
thread 3 · profile default · model fake-model

You:
who was u.s. president in 1846

Assistant:
James K. Polk
```

Because it writes to stdout, it composes with a pager or formatter, for example `ask thread | less`.

## Switch threads

```sh
ask switch
```

`ask switch` (alias `ask s`) offers up to 10 threads on stderr, most recently continued first. On an attended terminal it is an arrow-key menu:

```text
Select a thread (arrow keys, Enter; Esc cancels):
> 1. thread 3 (current) · 2 turns · who was u.s. president in 1846
  2. thread 1 · 1 turn · remove blockquoting from this text
```

Enter makes the highlighted thread current and prints it to stdout, as `ask thread` would; the next `ask reply` continues it. Esc leaves the current thread unchanged and exits 1.

When stdin or stderr is redirected, or `TERM` is unset or `dumb`, `ask switch` instead prints a numbered list with the profile, model, and latest-turn time as well, ends with `select a thread [1-2]: `, and reads a number from one line of stdin. A listed number makes that thread current and reports `ask: current thread is now <id>` on stderr; anything else leaves the current thread unchanged and exits 2. This form does not print the thread.

When you already know the thread id, which `ask thread` and the menu both show, select it directly. This confirms on stderr and does not print the thread:

```sh
ask switch 1
```

Switching reads only the local database. It contacts no provider and does not read the configuration.

## Read statistics

```sh
ask stats
```

`ask stats` writes a local summary to stdout: query counts by outcome, token totals, median timings, history size, and, for each provider target, when it was last observed healthy and when it last failed.

```text
queries: 12 · 10 complete · 1 partial · 1 failed
tokens: 1480 in / 322 out · reported by 11 queries
median complete query: 2.1s wall · 0.8s to first token
history: 4 threads · 9 turns · 3 threads cleared by expiry

provider targets (historical observations, not a current check):
openrouter · https://openrouter.ai/api/v1 · openai/gpt-5.6-luna
  12 queries · last observed healthy 2026-09-16 14:02 UTC (from query) · last failure 2026-09-12 08:11 UTC (timeout) (from live check)
```

These are historical observations from past queries and live checks, not a current health check. Statistics contain no prompt or answer text. To test a provider now, use `ask doctor --live`.

## Expire old history

History is kept indefinitely unless the configuration sets `expire_history = true`; see [Configuring profiles and providers](configuration.md#expire-old-history). With expiry enabled:

- `ask new` and `ask reply` remove every whole thread whose newest turn is more than `history_days` days old (default 90). A thread you keep replying to is kept, however old its first turn is.
- Removal happens only after you submit valid query input. A cancelled prompt or blank input removes nothing. `ask thread`, `ask switch`, `ask stats`, `ask doctor`, `ask init`, and `ask configure` never remove history, so an old thread stays visible until the next query.
- If the current thread expires, `ask reply` reports that there is no current thread rather than starting one.
- Statistics and provider-health observations survive expiry, and `ask stats` reports how many threads were cleared. Thread ids are never reused.
