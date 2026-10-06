#!/usr/bin/env python3
"""Builds the published model list from each hosted provider's own model list.

An operator runs this on a host that holds provider keys; it never runs in CI
and `ask` never runs it. It writes the version 1 document to the given path and
never pushes or publishes anything. Credentials are read from each provider's
standard environment variable and never printed, logged, or written.

Each provider's list is curated against the public models.dev catalog, fetched
without credentials: only identifiers the catalog describes as text-only chat
models with tool calling are kept.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import sys
import tempfile
import unicodedata
import urllib.error
import urllib.parse
import urllib.request

VERSION = 1
TIMEOUT_SECONDS = 10
MAX_PAGES = 10
MAX_RESPONSE_BYTES = 8 * 1024 * 1024
MAX_CATALOG_BYTES = 32 * 1024 * 1024
MAX_IDENTIFIER_BYTES = 200
MAX_ENTRIES = 2000
ANTHROPIC_VERSION = "2023-06-01"
GEMINI_PAGE_SIZE = "1000"

# The hosted presets of `ask init`: list shape, endpoint, and key variable.
PROVIDERS = {
    "openai": ("openai", "https://api.openai.com/v1", "OPENAI_API_KEY"),
    "anthropic": ("anthropic", "https://api.anthropic.com", "ANTHROPIC_API_KEY"),
    "gemini": ("gemini", "https://generativelanguage.googleapis.com", "GEMINI_API_KEY"),
    "openrouter": ("openai", "https://openrouter.ai/api/v1", "OPENROUTER_API_KEY"),
    "groq": ("openai", "https://api.groq.com/openai/v1", "GROQ_API_KEY"),
    "cerebras": ("openai", "https://api.cerebras.ai/v1", "CEREBRAS_API_KEY"),
    "xai": ("openai", "https://api.x.ai/v1", "XAI_API_KEY"),
}

CATALOG_URL = "https://models.dev/api.json"
# The catalog's host and some providers' hosts reject urllib's default user agent,
# so every request names this script.
USER_AGENT = "ask-model-list-refresh (+https://github.com/mdml/ask)"
# The catalog's provider id for each preset whose id differs from the preset name.
CATALOG_IDS = {"gemini": "google"}


class Failure(Exception):
    """A provider that is reported and omitted; its message never holds a credential."""


class RefuseRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, newurl):
        raise Failure(f"HTTP status {code}; redirects are not followed")


def encode(value):
    """Percent-encodes every byte outside RFC 3986's unreserved set, as `ask` does."""
    return urllib.parse.quote(value, safe="-._~")


def page_request(shape, base_url, key, cursor):
    """The URL and headers for one page, following `cursor`."""
    base = base_url.rstrip("/")
    if shape == "anthropic":
        url = f"{base}/v1/models" + (f"?after_id={encode(cursor)}" if cursor else "")
        return url, {"x-api-key": key, "anthropic-version": ANTHROPIC_VERSION}
    if shape == "gemini":
        token = f"&pageToken={encode(cursor)}" if cursor else ""
        return f"{base}/v1beta/models?pageSize={GEMINI_PAGE_SIZE}{token}&key={encode(key)}", {}
    return f"{base}/models", {"Authorization": f"Bearer {key}"}


def fetch_json(url, headers, limit=MAX_RESPONSE_BYTES):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, **headers}, method="GET")
    try:
        # Built per request so proxy settings come from the current environment.
        opener = urllib.request.build_opener(RefuseRedirects)
        with opener.open(request, timeout=TIMEOUT_SECONDS) as response:
            body = response.read(limit + 1)
    except urllib.error.HTTPError as error:
        raise Failure(f"HTTP status {error.code}") from None
    except (urllib.error.URLError, OSError) as error:
        raise Failure(f"request failed ({type(error).__name__})") from None
    if len(body) > limit:
        raise Failure("response is too large")
    try:
        return json.loads(body)
    except ValueError:
        raise Failure("response is not valid JSON") from None


def openai_id(entry):
    """Drops entries whose listed output modalities exclude text, as OpenRouter reports them."""
    outputs = (entry.get("architecture") or {}).get("output_modalities")
    if isinstance(outputs, list) and "text" not in outputs:
        return None
    return entry.get("id")


def gemini_id(entry):
    """Keeps models that serve generateContent, without the `models/` prefix."""
    methods = entry.get("supportedGenerationMethods")
    if not isinstance(methods, list) or "generateContent" not in methods:
        return None
    name = entry.get("name")
    if isinstance(name, str) and name.startswith("models/"):
        return name[len("models/"):]
    return name


def parse_page(shape, value):
    """The page's identifiers and the cursor for the next page, if any."""
    if not isinstance(value, dict):
        raise Failure("response has an unexpected shape")
    entries = value.get("models" if shape == "gemini" else "data")
    pick = gemini_id if shape == "gemini" else openai_id
    if not isinstance(entries, list):
        entries = []
    ids = [pick(entry) for entry in entries if isinstance(entry, dict)]
    if shape == "anthropic":
        cursor = value.get("last_id") if value.get("has_more") is True else None
    elif shape == "gemini":
        cursor = value.get("nextPageToken")
    else:
        cursor = None
    if not isinstance(cursor, str) or not cursor:
        cursor = None
    elif not encodable(cursor):
        raise Failure("response has a malformed page cursor")
    return [model for model in ids if isinstance(model, str)], cursor


def encodable(text):
    """Whether `text` is valid UTF-8 text; JSON allows lone surrogates, which UTF-8 cannot carry."""
    try:
        text.encode()
    except UnicodeEncodeError:
        return False
    return True


def list_models(shape, base_url, key):
    ids, cursor = [], None
    for _ in range(MAX_PAGES):
        url, headers = page_request(shape, base_url, key, cursor)
        page, cursor = parse_page(shape, fetch_json(url, headers))
        ids.extend(page)
        if cursor is None or len(ids) >= MAX_ENTRIES:
            break
    return ids


def usable(model):
    """Whether an identifier is safe to show in a terminal and write to TOML, as in `ask`."""
    try:
        size = len(model.encode())
    except UnicodeEncodeError:  # a lone surrogate, which JSON allows but UTF-8 cannot carry
        return False
    return (bool(model) and size <= MAX_IDENTIFIER_BYTES
            and not any(unicodedata.category(character) == "Cc" or invisible_format(character)
                        for character in model))


# The invisible format characters `ask` rejects (src/recall.rs, is_invisible_format).
INVISIBLE_FORMAT = ((0xAD, 0xAD), (0x61C, 0x61C), (0x180E, 0x180E), (0x200B, 0x200B),
                    (0x200E, 0x200F), (0x202A, 0x202E), (0x2060, 0x2064), (0x2066, 0x206F),
                    (0xFEFF, 0xFEFF), (0xFFF9, 0xFFFB), (0xE0001, 0xE0001), (0xE0020, 0xE007F))


def invisible_format(character):
    return any(low <= ord(character) <= high for low, high in INVISIBLE_FORMAT)


def digits(text, count):
    return len(text) == count and text.isascii() and text.isdigit()


def month_day(month, day):
    return digits(month, 2) and digits(day, 2) and 1 <= int(month) <= 12 and 1 <= int(day) <= 31


def compact_date(text):
    if len(text) == 8:
        return digits(text, 8) and text.startswith("20") and month_day(text[4:6], text[6:])
    return len(text) == 4 and digits(text, 4) and month_day(text[:2], text[2:])


def snapshot(model):
    """Whether the identifier ends in `-YYYYMMDD`, `-YYYY-MM-DD`, `-MMDD`, or `-MM-DD`."""
    parts = model.split("-")[::-1][:2]
    if compact_date(parts[0]):
        return True
    return len(parts) == 2 and month_day(parts[1], parts[0])


def arrange(ids, secrets):
    """Keeps usable, distinct identifiers that contain no credential, dated snapshots last."""
    seen, kept = set(), []
    for model in ids:
        if usable(model) and model not in seen and not any(secret in model for secret in secrets):
            seen.add(model)
            kept.append(model)
        if len(kept) >= MAX_ENTRIES:
            break
    return [model for model in kept if not snapshot(model)] + [model for model in kept if snapshot(model)]


def fetch_catalog(url):
    """The catalog's provider map; fetched with no credentials, as untrusted input."""
    catalog = fetch_json(url, {}, MAX_CATALOG_BYTES)
    if not isinstance(catalog, dict):
        raise Failure("model catalog has an unexpected shape")
    return catalog


def chat_models(catalog, name):
    """The identifiers the catalog describes as text-only chat models with tool calling."""
    entry = catalog.get(CATALOG_IDS.get(name, name))
    if entry is None:
        raise Failure("missing from the model catalog")
    models = entry.get("models") if isinstance(entry, dict) else None
    if not isinstance(models, dict):
        raise Failure("unexpected shape in the model catalog")
    return {model for model, value in models.items()
            if isinstance(value, dict) and isinstance(value.get("modalities"), dict)
            and value["modalities"].get("output") == ["text"] and value.get("tool_call") is True}


def provider_ids(name, key, catalog, secrets):
    """The provider's arranged identifiers and its count before curation."""
    shape, base_url, _ = PROVIDERS[name]
    ids = list_models(shape, base_url, key)
    listed = len(arrange(ids, secrets))
    if not listed:
        raise Failure("no usable identifiers")
    if catalog is None:
        return arrange(ids, secrets), listed
    chat = chat_models(catalog, name)
    kept = arrange([model for model in ids if model in chat], secrets)
    if not kept:
        raise Failure("no listed identifier is a text-only chat model with tool calling in the model catalog")
    return kept, listed


def refresh(names, environ, report, catalog_url=None):
    """Lists each named provider once; returns the lists and the omitted names.

    With a catalog URL, a provider's list keeps only the catalog's chat models,
    and a catalog failure omits every provider; without one, lists are uncurated.
    """
    secrets = [environ[variable] for _, _, variable in PROVIDERS.values() if environ.get(variable)]
    catalog = None
    if catalog_url is not None:
        try:
            catalog = fetch_catalog(catalog_url)
        except Failure as failure:
            for name in names:
                report(f"{name}: omitted (model catalog unavailable: {failure})")
            return {}, list(names)
    listed, omitted = {}, []
    for name in names:
        variable = PROVIDERS[name][2]
        key = environ.get(variable, "")
        try:
            if not key:
                raise Failure(f"{variable} is not set")
            ids, count = provider_ids(name, key, catalog, secrets)
        except Failure as failure:
            report(f"{name}: omitted ({failure})")
            omitted.append(name)
            continue
        curated = f" of {count} listed" if catalog is not None else ""
        report(f"{name}: {len(ids)} identifiers{curated}")
        listed[name] = ids
    return listed, omitted


def document(listed, now):
    generated = now.astimezone(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    return {"version": VERSION, "generated_at": generated, "providers": listed}


def write(path, value):
    """Replaces `path` atomically with the rendered document."""
    path = Path(path)
    rendered = json.dumps(value, indent=2, ensure_ascii=True) + "\n"
    with tempfile.NamedTemporaryFile("w", dir=path.parent, prefix=".models-",
                                     suffix=".json", delete=False, encoding="utf-8") as temporary:
        temporary.write(rendered)
    os.replace(temporary.name, path)


def provider_names(text):
    names = [name.strip() for name in text.split(",") if name.strip()]
    unknown = [name for name in names if name not in PROVIDERS]
    if unknown or not names:
        raise argparse.ArgumentTypeError(
            f"choose from {', '.join(PROVIDERS)}; unknown: {', '.join(unknown) or 'none given'}")
    return list(dict.fromkeys(names))


def catalog_location(text):
    if not text:
        raise argparse.ArgumentTypeError("must not be empty; omit the option to use the default catalog")
    return text


def build_parser():
    parser = argparse.ArgumentParser(
        description="Write the version 1 published model list from each hosted provider's own list.")
    parser.add_argument("output", type=Path, help="path of the JSON document to write")
    parser.add_argument("--providers", type=provider_names, default=list(PROVIDERS),
                        help="comma-separated providers to list (default: all)")
    parser.add_argument("--allow-partial", action="store_true",
                        help="write the document and exit 0 even if some providers were omitted")
    catalog = parser.add_mutually_exclusive_group()
    catalog.add_argument("--catalog-url", type=catalog_location, default=None,
                         help=f"model catalog to curate against (default: {CATALOG_URL})")
    catalog.add_argument("--no-catalog", action="store_true",
                         help="publish each provider's uncurated list")
    return parser


def main(argv=None, environ=None):
    args = build_parser().parse_args(argv)
    report = lambda line: print(line, file=sys.stderr)
    if args.no_catalog:
        catalog_url = None
        report("model catalog: not used (--no-catalog); the lists are not curated")
    else:
        catalog_url = CATALOG_URL if args.catalog_url is None else args.catalog_url
        report(f"model catalog: {catalog_url}")
    listed, omitted = refresh(args.providers, os.environ if environ is None else environ, report,
                              catalog_url)
    if not listed or (omitted and not args.allow_partial):
        report(f"omitted: {', '.join(omitted)}; nothing was written")
        return 1
    write(args.output, document(listed, datetime.datetime.now(datetime.timezone.utc)))
    report(f"wrote {len(listed)} providers to {args.output}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:
        print("model-list-refresh failed; raw diagnostics withheld", file=sys.stderr)
        raise SystemExit(1)
