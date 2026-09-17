# Local storage

`ask` keeps conversation history, query statistics, and historical provider-health observations in one local SQLite database. Nothing in it is sent anywhere except the thread context that a reply sends to the thread's provider. Credential values are never stored.

## Location and permissions

The database is `$ASK_HOME/data/ask.sqlite3` when `ASK_HOME` is set, and otherwise `ask.sqlite3` in the platform-standard data directory for an application named `ask`. `ask doctor` prints the resolved path.

| Command | Database access |
|:--|:--|
| `ask new`, `ask reply` | Create a missing data directory with mode `0700` and a missing database with mode `0600`; leave existing permissions alone. |
| `ask thread`, `ask switch`, `ask stats` | Open the database only when it already exists; never create it. |
| `ask doctor` | Inspects read-only; never creates or migrates. |
| `ask doctor --live` | May create or migrate the database to record the live-check observation. |
| `ask init`, `ask configure`, `ask help`, `ask version` | Never open the database. |

## What is recorded

- Threads, each with its [profile snapshot](query-behavior.md#threads-and-replies).
- Turns: the prompt, the raw answer text, the status, and the reason for a partial turn.
- One statistics row per query sent to the provider: start time, command, profile name, provider kind, base URL, model, outcome (complete, partial, or failed), error class (`provider`, `timeout`, `output`, or `output_limit`), wall, API, and time-to-first-token durations, and token counts when the provider reports them. Statistics rows contain no prompt or answer text.
- Provider health: for each provider target (provider kind, base URL, and model), the time and source of the latest success, and the time, source, and error class of the latest failure. The source is `query` for ordinary queries and `live-check` for `ask doctor --live`. Output failures are not provider-health observations, and an answer stopped at the output-token limit counts as a success.
- The total number of threads removed by history expiry, the time of the latest removal, and the highest thread id observed before expiry, so removed ids are never reused.

## Atomic recording

Everything one query records is written after the answer finishes or fails, in one transaction: the thread (for `ask new`), the turn, the statistics row, the provider-health update, and the current-thread change. Nothing is written while the answer streams.

- If the answer was delivered to stdout but that transaction fails, stdout keeps the answer, `ask` exits 1, and stderr reports `ask: answer was delivered but not recorded: <cause>` without repeating the prompt.
- If a query failed before any answer text and its statistics cannot be recorded, stderr adds `ask: query statistics were not recorded: <cause>`.

## History expiry

History is kept indefinitely unless the installed configuration sets `expire_history = true`; see the [configuration reference](configuration.md#top-level-keys).

With expiry enabled, `ask new` and `ask reply` remove every whole thread whose newest turn, complete or partial, is more than `history_days` days (default 90) older than the present. A thread continued within the period is kept however old its first turn is.

- The command reads the expiry settings when it starts.
- Removal runs in its own transaction, only after valid query input is submitted and before the request is sent. A cancelled prompt (Ctrl-C), blank input, or any other usage or input error leaves history unchanged.
- Removal deletes each expired thread's turns and, if the current thread is removed, the current-thread selection.
- Thread ids are never reused: a new thread's id is higher than any id that expiry has removed.
- Statistics rows and provider health survive expiry. `ask stats` reports the number of threads cleared.
- `ask thread`, `ask switch`, `ask stats`, `ask doctor`, `ask init`, and `ask configure` never remove history, so an old thread stays visible and selectable until the next `ask new` or `ask reply`.

### Replies and expiry

`ask reply` captures the current thread when it starts. If that thread no longer exists once input is submitted, whether this reply's expiry or another command removed it, the reply reports ``ask: no current thread; start one with `ask new` `` and sends no request. It does not start a new thread.

`ask reply` reads the installed configuration only for the two expiry settings. When the configuration is missing or invalid, the reply cannot read them: once input is submitted it reports `ask: history expiry skipped: <cause>` on stderr, removes nothing, and continues the current thread with its captured profile.

Another `ask new` or `ask reply` that applies expiry while a reply is streaming may remove that reply's thread, for example when the thread's newest turn ages past the retention period in the meantime or the other command uses a shorter period. The reply's record transaction then fails as a whole: stdout keeps the answer, `ask` exits 1 with `ask: answer was delivered but not recorded: <cause>`, and neither its turn, its statistics row, its provider-health observation, nor a current-thread change is recorded.

## Schema versions

The database schema is version 4. A database from an earlier `ask` is upgraded in place, in one transaction, when `ask new`, `ask reply`, `ask thread`, `ask switch`, `ask stats`, or `ask doctor --live` opens it:

| Version | Added |
|:--|:--|
| 1 | Threads, turns, statistics, and provider health. |
| 2 | History expiry. |
| 3 | Output-token limits in profile snapshots. |
| 4 | Provider-health observation sources: nullable `last_success_source` and `last_failure_source` columns. |

Offline `ask doctor` never migrates storage; it reports an older schema as a limited check. A schema version newer than this `ask` supports is refused. An unpublished prototype layout that marked version 2 with only an output-token column is also refused.
