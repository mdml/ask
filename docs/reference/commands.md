# Command reference

This page specifies every `ask` command. Interfaces may change during 0.x. Input, output, and recording rules shared by the query commands are in [query behavior](query-behavior.md); configuration keys are in the [configuration reference](configuration.md).

## Synopsis

```text
ask [--profile NAME | -p NAME] [new|n] [prompt words...]
ask [reply|r] [prompt words...]
ask [thread|t]
ask [switch|s] [ID]
ask stats
ask [doctor|d] [--live] [--all]
ask [init|i]
ask [configure|c] check [FILE|-]
ask [configure|c] apply [FILE|-]
ask help
ask version
```

| Command | Alias | Purpose |
|:--|:--|:--|
| `ask new` | `ask`, `ask n` | Ask a question in a new thread. |
| `ask reply` | `ask r` | Ask a question with the current thread as context. |
| `ask thread` | `ask t` | Show the current thread. |
| `ask switch` | `ask s` | Select the current thread. |
| `ask stats` | none | Show local query and provider-health statistics. |
| `ask doctor` | `ask d` | Diagnose the installed system. |
| `ask init` | `ask i` | Create the first configuration interactively. |
| `ask configure` | `ask c` | Validate or install a complete configuration document. |
| `ask help` | `ask --help`, `ask -h` | Print the command summary. |
| `ask version` | `ask --version`, `ask -V` | Print `ask <version>`. |

`ask stats` has no one-character alias because `s` belongs to `switch`. A first argument that is not a command name or alias starts a new query, so `ask "what is 2+2"` is `ask new "what is 2+2"`.

Usage errors exit 2 with an `ask: ...` diagnostic on stderr. A command-line syntax error names the problem, when there is a specific one, and appends the full usage synopsis.

An attended terminal, in this reference, means that stdin and stderr are both terminals and `TERM` is set to a value other than `dumb`. `ask switch` and `ask init` present arrow-key menus only on an attended terminal and otherwise fall back to numbered, line-oriented input. `ask thread`, `ask stats`, `ask init`, `ask help`, and `ask version` take no arguments; extra arguments are a usage error.

## `ask new`

`ask`, `ask new`, and `ask n` start a new thread using the default profile. These forms are equivalent:

```sh
ask "what is 2+2"
ask new "what is 2+2"
ask n "what is 2+2"
```

`--profile NAME` or `-p NAME`, given before or directly after `new`, selects a configured profile instead of the default. When the flag is repeated, the last value applies. The flag requires a nonempty name.

```sh
ask --profile terse "what is 2+2"
```

Prompt words are joined with single spaces. Without prompt words, the prompt comes from redirected stdin or from a multiline terminal prompt; see [query input](query-behavior.md#input).

## `ask reply`

`ask reply` and `ask r` continue the current thread. The request uses the profile captured when the thread was created, and sends the thread's earlier complete turns as context; see [threads and replies](query-behavior.md#threads-and-replies). `--profile` and `-p` are rejected on a reply as a usage error. With no current thread, `ask reply` exits 1 with ``ask: no current thread; start one with `ask new` `` on stderr and sends no request.

## `ask thread`

`ask thread` and `ask t` write the full current thread to stdout and exit 0.

- The first line is `thread <id> · profile <profile> · model <model>`.
- Each turn follows after a blank line: a `You:` line, the prompt as stored without trailing line endings, a blank line, an `Assistant:` line, and the raw answer without trailing line endings. Answer text is printed as stored, like query output.
- A partial turn ends with a line `[incomplete: <reason>]`. An empty answer prints only the `Assistant:` line.
- Blocks are separated by one blank line, and the output ends with a newline.

With no current thread, or no database, `ask thread` exits 1 with ``ask: no current thread; start one with `ask new` `` on stderr. If the stdout reader closes early, it exits 0 without a diagnostic. It opens the database only when it already exists and never creates it.

## `ask switch`

`ask switch <id>` and `ask s <id>` make thread `<id>` current and report `ask: current thread is now <id>` on stderr; stdout stays empty. The id is the number that `ask thread` and the `ask switch` menu show, written as decimal digits. A missing id exits 1 with `ask: no thread with id <id>`. An id that is not a positive decimal integer is a usage error.

`ask switch` and `ask s` without an id offer up to 10 threads on stderr, most recently continued first.

On an attended terminal, the threads form a menu under the line `Select a thread (arrow keys, Enter; Esc cancels):`. The Up and Down arrow keys move the `>` marker and wrap at either end. Each entry shows its thread id, current marker, turn count, and opening prompt preview. Each entry is cut to the terminal width, and a menu taller than the terminal scrolls to keep the marked entry visible. Enter makes the marked thread current and then writes that thread to stdout exactly as `ask thread` does; this automatic display happens only after a menu selection, and nothing is reported on stderr. Esc exits 1 with `ask: selection cancelled; current thread unchanged` and an empty stdout. Ctrl-C ends the process by the default SIGINT disposition and leaves the current thread unchanged. Enter and Esc clear the menu; keyboard Ctrl-C restores the terminal mode before signaling. Terminals under three rows report an error before drawing the menu.

Otherwise, `ask switch` lists the threads, then reads one line from stdin:

```text
 1. thread 3 (current) · 2026-09-16 14:02 UTC · 2 turns · default · fake-model · who was u.s. president in 1846
 2. thread 1 · 2026-09-15 09:30 UTC · 1 turn · default · fake-model · remove blockquoting from this text
select a thread [1-2]:
```

Each line-oriented entry shows the thread id, the time of its newest turn in UTC, the turn count, the profile, the model, and up to 60 characters of the first line of its first prompt. In the line-oriented form, entering a listed number selects that thread and reports `ask: current thread is now <id>` on stderr; stdout stays empty. Empty input, end of input, or anything else exits 2 with `ask: selection must be a number from 1 to <n>; current thread unchanged`. With no threads, `ask switch` exits 1 with ``ask: no threads; start one with `ask new` ``.

In the profile, model, and prompt fields of the menu, control characters and invisible Unicode format characters are replaced by spaces so that stored text cannot disguise a menu entry:

- bidirectional controls: U+061C, U+200E, U+200F, U+202A–U+202E, U+2066–U+2069;
- zero-width and other invisible characters: U+00AD, U+180E, U+200B, U+2060–U+2064, U+206A–U+206F, U+FEFF;
- interlinear annotation: U+FFF9–U+FFFB;
- tag characters: U+E0001, U+E0020–U+E007F.

The zero-width joiner and non-joiner (U+200C, U+200D), which shape emoji and scripts such as Persian, are kept, as is all other visible text.

Selection does not contact a provider, read the configuration, or remove expired history. The next `ask reply` continues the selected thread with its captured profile.

## `ask stats`

`ask stats` writes a local summary to stdout and exits 0. It reads only the database, contains no prompt or answer text, and never contacts a provider.

```text
queries: 12 · 10 complete · 1 partial · 1 failed
tokens: 1480 in / 322 out · reported by 11 queries
median complete query: 2.1s wall · 0.8s to first token
history: 4 threads · 9 turns · 3 threads cleared by expiry

provider targets (historical observations, not a current check):
openrouter · https://openrouter.ai/api/v1 · openai/gpt-5.6-luna
  12 queries · last observed healthy 2026-09-16 14:02 UTC (from query) · last failure 2026-09-12 08:11 UTC (timeout) (from live check)
```

- Query counts include every recorded query, including those whose threads history expiry later removed.
- Token totals sum the counts providers reported; `reported by` counts the queries that reported them.
- Medians are lower medians over complete queries, truncated to tenths of a second, and `-` when there is none.
- Each provider target (provider kind, base URL, and model) shows its recorded queries, its latest success as `last observed healthy` or `never observed healthy`, and its latest failure with its error class or `no failures observed`. `(from query)` and `(from live check)` name the source of each observation. The provider-target section is omitted when nothing has been observed.

These are historical observations, not a health check. Without a database, `ask stats` prints zero counts and creates nothing. If the stdout reader closes early, it exits 0 without a diagnostic.

## `ask doctor`

`ask doctor` and `ask d` diagnose the installed system. Without `--live`, the command is offline and side-effect-free: it contacts no provider and creates, migrates, and changes nothing. It:

- validates the installed configuration with the same strict validator as `ask configure check`;
- resolves and displays every path: `ASK_HOME` when set, and the configuration file, database, and cache locations, each with its state (`present`, `absent`, `inaccessible`, or a wrong-type note);
- reports the default profile and the history-expiry setting;
- inspects the history database read-only;
- for the default profile's provider target, reports whether the credential environment variable is present, never its value, and the target's historical health as `last observed healthy`, including whether each observation came from an ordinary query or a live check.

Historical health is never presented as current health.

The report goes to stdout; a one-line summary of any problem, and warnings, go to stderr.

| Exit status | Meaning |
|:--|:--|
| 0 | No problem found. |
| 1 | A configuration problem: the file is missing, unreadable, or invalid. This takes precedence over status 3. |
| 3 | An environmental-readiness problem: a missing or non-Unicode credential variable, a path that is not ready, a database that is unusable or inaccessible or allows only a limited check, or a failed live check. |

A missing database file is reported as absent and is not a problem. An existing but unreadable file is reported as inaccessible rather than absent. When deeper storage validation cannot run safely without side effects (WAL-format headers, companion sidecar files, or an older schema that would require migration), `doctor` reports a limited check and exits 3 rather than claiming the store is healthy.

`ask doctor --live` additionally sends one fixed minimal request to the default profile's provider target: the prompt `Reply with exactly: ok`, the system prompt `Reply with exactly one word.`, and a 128-token output cap, regardless of profile settings. It may incur provider cost, and `ask` prints `ask: warning: live check sends a minimal provider request that may incur cost` on stderr before sending. The result appears in the report as `live: ok` or `live: failed (<cause>)`. Each live check, successful or failed, is recorded as a provider-health observation with the source `live-check`; to record it, live mode may create or migrate the database. If the observation cannot be recorded, `doctor` reports `ask: live health was not recorded: <cause>` and exits 3.

`ask doctor --live --all` checks every distinct provider target across all profiles, and reports credential presence and historical health for each. `--all` without `--live` is a usage error.

## `ask init`

`ask init` and `ask i` create the configuration file through a dialogue. Every prompt and diagnostic is written to stderr, and stdout stays empty. Answers may come from a terminal or from redirected stdin, one answer per line; redirected stdin still requires an answer for every prompt. On an attended terminal, the provider is chosen from an arrow-key menu (Up, Down, Enter; Esc cancels) and each typed prompt begins with `? `; otherwise the provider menu is numbered and answered by number. `ask init` has no noninteractive all-default mode, because a provider target cannot be inferred.

The dialogue asks for:

1. A provider, from a menu of the [initialization presets](providers.md#initialization-presets) plus a custom OpenAI-compatible endpoint. A preset supplies the provider name, endpoint, and credential variable name. The custom option asks for a provider name, an endpoint base URL that satisfies the [endpoint rule](configuration.md#providers), and a credential variable name, which is validated but never read.
2. A model identifier: free text sent to the provider.
3. An optional replacement system prompt. The default system prompt is shown first, and an empty answer keeps it.
4. A profile name, which defaults to `default`. The profile created becomes the default profile.

After the provider is chosen, the dialogue prints the credential variable name and a hidden-prompt command for supplying the key to a first query. Answers are trimmed and limited to one line. For a multiline system prompt, an explicitly empty system prompt, or significant surrounding whitespace, prepare the complete TOML document and use `ask configure apply`.

An invalid answer prints an explanation and asks again. The dialogue then shows the exact TOML it will write and asks for confirmation. Only `y` or `yes` writes the file; any other answer, Esc at the provider menu, or end of input at any prompt cancels without writing and exits 1. The written document omits `timeout_ms`, `system_prompt`, and the other optional keys when the defaults apply.

`ask init` refuses to run when the configuration file already exists and leaves it unchanged; use `ask configure apply` to replace a regular file. It publishes the fully written file with a hard link, so a configuration that appears during the dialogue is never overwritten. It shares the locking, permission, and destination rules of [`ask configure apply`](#installation-procedure). `ask init` never opens the database.

## `ask configure`

`ask configure check [FILE|-]` and `ask configure apply [FILE|-]`, aliased `ask c check` and `ask c apply`, read one complete TOML document and validate it with the [validation rules](configuration.md#validation). `FILE` names a file; `-` and an omitted argument both read the document from stdin. Omitting the argument while stdin is a terminal is a usage error: name a file, or redirect a file or pipe into the command.

Both commands exit 0 on success with a one-line message on stderr, and exit 1 with a one-line diagnostic on stderr when the document is invalid or cannot be read. Stdout stays empty in every case. Neither command reads a credential value, contacts a provider, or opens the database.

`ask configure check` writes nothing: it never creates, replaces, or removes a file.

`ask configure apply` installs the candidate's exact bytes, preserving comments, key order, and spacing, and reports whether it created or replaced the configuration.

### Installation procedure

1. `apply` acquires an exclusive advisory lock on `.ask-config.lock` in the configuration directory, then snapshots the destination before reading the candidate. A competing `ask` writer fails promptly with a lock diagnostic. `ask init` takes the same lock when publishing its confirmed document.
2. It validates the candidate, then writes and syncs a temporary file in that directory. New files use owner-only permissions (`0600`, subject to umask); replacements retain the destination's Unix read/write/execute permission bits.
3. It checks the destination's contents, device and inode identity, permissions, and change timestamp against the snapshot. A detected change aborts publication.
4. It atomically renames the temporary file over an existing destination, or hard-links it to an absent destination without overwriting a file that appeared concurrently. Readers see a complete old or new document.

Failed temporary writes and failed publication leave the destination intact; temporary files are removed on normal error returns when directory permissions permit cleanup.

### Limits of the installation procedure

- Symlink destinations, including dangling symlinks, and nonregular destinations are refused by both `apply` and `init`. Manage those paths explicitly before installing a configuration.
- Creating a configuration requires hard-link support on the filesystem that holds the configuration directory, including when `ASK_HOME` selects that directory. Both `init` and `apply` fail safely without it.
- The lock file is persistent and must not be removed while writers may be running: unlinking it could give overlapping writers different lock inodes. An unsuccessful `apply` can leave the configuration directory and this empty lock file behind.
- Cooperating `ask` writers cannot overlap publication. External editors that ignore the advisory lock can still change the file or directory after the snapshot check and before the rename; this is not an operating-system compare-and-swap guarantee. The configuration directory and lock inode must remain stable while writers run.
- Abrupt termination may leave a temporary file, and syncing the temporary file does not guarantee directory-entry durability across a crash.

## `ask help` and `ask version`

`ask help` (also `ask --help` and `ask -h`) prints the command summary to stdout. `ask version` (also `ask --version` and `ask -V`) prints `ask <version>`. Neither reads configuration, opens the database, or contacts a provider.
