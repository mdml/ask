# Publishing the model list

`ask init` offers model identifiers from a list the `ask` project publishes when no key is available for a hosted provider. The [provider reference](../reference/providers.md#published-model-list) specifies how `ask` fetches and validates it. This guide is for the operator who regenerates and publishes that list. Publishing is a manual operator step; no workflow, schedule, or CI job runs it, and `ask` never runs the refresh script.

As of 2026-10-02 the list has not been published: the `models` branch does not exist, so `ask init` reports `HTTP status 404 Not Found` and falls back to free-text entry until the first publication below.

## What is published

The list is the file `v1/models.json` on the `models` branch of the `mdml/ask` repository, served at `https://raw.githubusercontent.com/mdml/ask/models/v1/models.json`. It is a version 1 document: `version`, the UTC `generated_at` time, and one array of identifiers per hosted preset provider name. The `models` branch holds only this file and shares no history with `main`.

## Generate the document

[`scripts/model-list-refresh.py`](../../scripts/model-list-refresh.py) uses only the Python standard library. Run it on a host that holds the provider keys, through the external credential wrapper that injects them into the process it launches, never with keys exported in the shell:

```sh
<credential-wrapper> python3 scripts/model-list-refresh.py "$(mktemp -d)/models.json"
```

For each hosted provider, the script reads the key from the provider's standard variable (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `OPENROUTER_API_KEY`, `GROQ_API_KEY`, `CEREBRAS_API_KEY`, `XAI_API_KEY`) and requests the provider's model list with the same endpoints, credential placement, and pagination as [`ask init`](../reference/providers.md#provider-model-lists). It makes one request sequence per provider with no retries and never follows redirects. It keeps only entries the API marks as usable for text generation where it marks them, applies the same sanitizing as `ask` (dropping empty identifiers, identifiers longer than 200 bytes or containing control characters or invisible Unicode format characters, duplicates, and any identifier containing a key), keeps at most 2,000 per provider, and orders dated snapshots after the others.

The script then curates each list so it holds common chat models only. Before any provider request, it fetches the public [models.dev](https://models.dev) catalog (`https://models.dev/api.json`, MIT-licensed) once, with no credentials, refusing redirects and reading at most 32 MiB. Every request the script makes, to the catalog and to providers, sends a user agent that names the script, because the catalog's host and some providers' hosts reject the default one of Python's HTTP library. The catalog is a third-party source consulted only at publication time; `ask` never contacts it. The script keeps an identifier only if the provider's own list returned it and the catalog's entry for that provider and that exact identifier has `modalities.output` equal to exactly `["text"]` and `tool_call` equal to `true`. The catalog's provider ids are the preset names, except that the `gemini` preset is the catalog's `google` provider. The script contains no model names, patterns, allow lists, or deny lists, so a model the catalog does not yet describe is left out until the catalog adds it.

The catalog never widens a list, and a catalog problem never publishes an uncurated one. If the catalog cannot be fetched, is too large, is not valid JSON, or is not a JSON object, every requested provider is omitted with `model catalog unavailable` and its reason, and no provider is requested. A provider missing from the catalog, or whose catalog entry has no `models` object, is omitted. A provider whose list had usable identifiers but none that the catalog describes as text-only chat models with tool calling is omitted rather than published empty. A catalog model entry without exactly those values is treated as not a chat model. `--catalog-url <URL>` curates against another copy of the catalog, such as a mirror or a local file served on loopback. `--no-catalog` skips the catalog and publishes the uncurated lists, and says so on stderr; the two options cannot be combined.

The script reports the catalog it uses on stderr, then one line per provider: the identifier count with the count the provider listed before curation, or `omitted` with a reason such as a missing key, an HTTP status, or a catalog problem. It never prints, logs, or writes a credential, including Gemini's, which travels in the request URL. If any requested provider was omitted, it writes nothing and exits 1; `--allow-partial` writes the document without the omitted providers and exits 0. `--providers openai,groq` limits the run to the named providers. The script writes the document with owner-only permissions (mode `0600`), replacing any file at the output path. That mode does not matter once the file is committed to the `models` branch, because git records only whether a file is executable. The script makes no git change and publishes nothing.

## Review the output

Before publishing, read the document and compare it with the published one:

```sh
python3 -m json.tool <output>/models.json | less
git diff --no-index <models-worktree>/v1/models.json <output>/models.json
```

Check the size and contents of each provider's list before publishing: every provider is present, its identifiers are plausible chat models, its count is reasonable against the count listed before curation on stderr, and no removal is unexpected. A large drop can mean the catalog changed or lags behind a provider's new models. To try it in `ask init` before publishing, serve the directory on loopback and point an isolated configuration at it:

```sh
python3 -m http.server --bind 127.0.0.1 --directory <output> 8000 &
ASK_HOME="$(mktemp -d)" ASK_MODEL_LIST_URL=http://127.0.0.1:8000/models.json ask init
```

Choose a hosted provider whose key is not set, confirm the menu and the `generated` date, then cancel with Esc and stop the server.

## Publish

The first publication creates the `models` branch as an orphan in a separate worktree:

```sh
git fetch origin
git worktree add --detach .worktrees/models
cd .worktrees/models
git switch --orphan models
mkdir v1
cp <output>/models.json v1/models.json
git add v1/models.json
git commit -m "chore(models): publish the model list generated <YYYY-MM-DD>"
git push origin models
```

Later publications update the same file:

```sh
git fetch origin
git worktree add .worktrees/models models
cd .worktrees/models
git pull --ff-only
cp <output>/models.json v1/models.json
git add v1/models.json
git commit -m "chore(models): publish the model list generated <YYYY-MM-DD>"
git push origin models
```

Remove the worktree afterwards with `git worktree remove .worktrees/models` from the repository root. GitHub's raw file service may serve the previous file for a few minutes after the push.
