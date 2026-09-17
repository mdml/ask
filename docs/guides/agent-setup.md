# Setting up `ask` with a coding agent

`ask-doctor` is an agent skill that lets a coding agent install, configure, and troubleshoot `ask` for you. This guide shows how to add it to Claude Code, Codex, and Cursor, how to call it in each, and how to use it from any other agent.

The skill is one Markdown file, [`skills/ask-doctor/SKILL.md`](../../skills/ask-doctor/SKILL.md). Every installation below delivers that same file. Read it before installing: it is the complete set of instructions your agent receives, and it contains no scripts.

Choose one installation method. The commands below use the current skill content from `main`. To keep a reviewed skill revision fixed, replace `main` in the download URL with its full commit SHA; for a local checkout at that revision, copy `skills/ask-doctor/` into your agent's skills directory. Update it deliberately when you want newer guidance. The `v0.1.0` tag contains the older skill accepted with that release.

## What the agent will and will not do

With the skill loaded, the agent:

- starts from the installed binary: `ask version`, `ask help`, and the offline `ask doctor`. It reads documentation that matches your installed release, because `main` can describe behavior your copy does not have yet;
- changes configuration only by copying your existing file, editing the copy, validating the complete document with `ask configure check`, reviewing the difference, and installing it with `ask configure apply` within your authorized scope;
- never asks for, reads, or stores a provider key. It tells you which environment variable `ask` expects and points you to [Injecting credentials](credentials.md);
- never sends a provider request on its own initiative. `ask doctor --live` and every query can incur provider cost, so the agent establishes authorization first or hands you the command;
- reports offline results and live results separately. Passing offline checks show that the configuration is valid and the credential variable is set, not that the provider accepts the key or the model.

## Claude Code

This repository is a Claude Code plugin marketplace named `ask` that offers one plugin, also named `ask`. In a Claude Code session:

```text
/plugin marketplace add mdml/ask
/plugin install ask@ask
```

From a shell, the equivalent commands are `claude plugin marketplace add mdml/ask` and `claude plugin install ask@ask`.

Call the skill with `/ask:ask-doctor`, or describe the task ("set up ask for me") and let Claude Code load it. Plugin skills carry the plugin name as a prefix, so the command is not `/ask-doctor`.

To update, run `claude plugin marketplace update ask` and then `claude plugin update ask@ask`. To remove, run `claude plugin uninstall ask@ask` and `claude plugin marketplace remove ask`.

## Codex and Cursor

Codex reads personal skills from `~/.agents/skills`. Cursor's documentation lists the same directory among the locations it reads. Place the file there:

```sh
mkdir -p ~/.agents/skills/ask-doctor
curl -fsSL https://raw.githubusercontent.com/mdml/ask/main/skills/ask-doctor/SKILL.md \
  -o ~/.agents/skills/ask-doctor/SKILL.md
```

To share the skill with everyone who works in one repository, use `.agents/skills/ask-doctor/` at that repository's root instead. Start a new session afterward so the agent rescans its skills.

- **Codex:** type `$ask-doctor` in the prompt, choose the skill from `/skills`, or describe the task.
- **Cursor:** type `/` in Agent chat and choose `ask-doctor`, or describe the task.

To update, run the `curl` command again. To remove, delete the `ask-doctor` directory.

## Any other agent

An agent that can read a URL or a file can follow the skill without installing anything. Give it this link and ask it to follow the instructions:

```text
https://raw.githubusercontent.com/mdml/ask/main/skills/ask-doctor/SKILL.md
```

Agents that support the `SKILL.md` convention load the file from their own skills directory; see that agent's documentation for the location and for how skills are called.

## Which installations have been verified

Checked on 2026-09-17 against Claude Code 2.1.273 and Codex CLI 0.154.0, using temporary configuration directories and no model requests:

| Installation | Evidence |
|:--|:--|
| Claude Code plugin | `claude plugin validate --strict .` passes. Adding a local checkout as a marketplace and running `claude plugin install ask@ask` installs the plugin, and `claude plugin details ask@ask` lists the `ask-doctor` skill. The `mdml/ask` GitHub form of `marketplace add` has not been exercised. |
| Codex, `~/.agents/skills` | With the file in place, `codex debug prompt-input` lists `ask-doctor` among the available skills. |
| Cursor, `~/.agents/skills` | From [Cursor's skills documentation](https://cursor.com/docs/context/skills) only; not exercised. |

Calling the skill inside a live session (`/ask:ask-doctor`, `$ask-doctor`, and Cursor's `/` menu) follows each harness's documentation and has not been exercised: [Claude Code skills](https://code.claude.com/docs/en/skills), [Claude Code plugin marketplaces](https://code.claude.com/docs/en/plugin-marketplaces), and [Codex skills](https://developers.openai.com/plugins/build/skills).
