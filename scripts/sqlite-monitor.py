#!/usr/bin/env python3
"""Report-only native SQLite freshness and security monitoring for the nightly check.

Reads the ledger monitoring/sqlite-native.toml and Cargo.lock offline, then, unless
--offline is given, reads public sources read-only: the crates.io sparse index,
static.crates.io archives of newer libsqlite3-sys releases, SQLite's download page,
and SQLite's CVE page. It changes no dependency, opens no pull request, and needs
no credential. Every finding is a triage item in a Markdown report; the exit
status is nonzero only when the monitor itself cannot run. Output contains only
versions, identifiers, digests, and public upstream URLs.
"""
import argparse
import datetime
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import sys
import tarfile
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "monitoring/sqlite-native.toml"
LOCK = ROOT / "Cargo.lock"
INDEX = "https://index.crates.io"
CRATES = "https://static.crates.io/crates"
DOWNLOADS = "https://sqlite.org/download.html"
CVES = "https://sqlite.org/cves.html"
TIMELINE = "https://sqlite.org/src/timeline?from=version-{0}&to=version-{1}&y=ci"
SMALL = 4 * 1024 * 1024
CRATE_LIMIT = 16 * 1024 * 1024
CRATE_EXPANDED_LIMIT = 64 * 1024 * 1024
TIMEOUT = 30
TIMELINE_CHECK_IN_LIMIT = 100
VERSION = re.compile(r"[0-9]+(?:\.[0-9]+)+")
CVE = re.compile(r"CVE-[0-9]{4}-[0-9]{4,}")
CRATE_NAME = re.compile(r"[a-z0-9_-]+")
SHA256 = re.compile(r"[0-9a-f]{64}")
CHECK_IN = re.compile(r"[0-9a-f]{10,64}")
SOURCE_ID = re.compile(r"[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2} [0-9a-f]{40,64}")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def version_tuple(text):
    return tuple(int(part) for part in text.split("."))


def fetch(url, limit=SMALL):
    """Read one public HTTPS resource, bounded in size; raw diagnostics never leave."""
    request = urllib.request.Request(url, headers={"User-Agent": "ask-sqlite-monitor"})
    with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
        data = response.read(limit + 1)
    require(len(data) <= limit, "response exceeds size limit")
    return data


def read_ledger(path=LEDGER):
    ledger = tomllib.loads(path.read_text())
    for table, keys in {
        "locked": ["rusqlite", "rusqlite_checksum", "libsqlite3_sys", "libsqlite3_sys_checksum"],
        "native": ["sqlite_version", "source_id"],
        "triage": ["last_triaged_sqlite_release", "last_triaged_check_in",
                   "revisit_when_bundled_sqlite_at_least", "review_deadline"],
    }.items():
        require(all(isinstance(ledger.get(table, {}).get(k), str) for k in keys), f"ledger [{table}] incomplete")
    require(VERSION.fullmatch(ledger["native"]["sqlite_version"]), "ledger native version")
    require(SOURCE_ID.fullmatch(ledger["native"]["source_id"]), "ledger native source id")
    require(CHECK_IN.fullmatch(ledger["triage"]["last_triaged_check_in"]), "ledger triaged check-in")
    require(SHA256.fullmatch(ledger["locked"]["rusqlite_checksum"]), "ledger rusqlite checksum")
    require(SHA256.fullmatch(ledger["locked"]["libsqlite3_sys_checksum"]), "ledger libsqlite3-sys checksum")
    validate_cve_triage(ledger)
    return ledger


def validate_cve_triage(ledger):
    """Validate complete, dated CVE dispositions and return them by CVE ID."""
    entries = ledger.get("cve_triage", [])
    require(isinstance(entries, list), "ledger cve_triage must be an array")
    dispositions = {}
    for entry in entries:
        require(isinstance(entry, dict), "ledger CVE disposition must be a table")
        ids = entry.get("ids")
        require(isinstance(ids, list) and ids, "ledger CVE disposition ids")
        require(all(isinstance(cve, str) and CVE.fullmatch(cve) for cve in ids), "ledger CVE disposition id")
        require(len(set(ids)) == len(ids), "duplicate ID within ledger CVE disposition")
        for field in ("disposition", "owner", "recorded_on", "review_deadline"):
            value = entry.get(field)
            require(isinstance(value, str) and value.strip() and len(value) <= 500,
                    f"ledger CVE disposition {field}")
        recorded = datetime.date.fromisoformat(entry["recorded_on"])
        deadline = datetime.date.fromisoformat(entry["review_deadline"])
        require(recorded <= deadline, "ledger CVE disposition deadline predates recording")
        for cve in ids:
            require(cve not in dispositions, "duplicate ledger CVE disposition")
            dispositions[cve] = (recorded, deadline)
    return dispositions


def read_lock(path=LOCK):
    packages = tomllib.loads(path.read_text())["package"]
    locked = {}
    for name in ("rusqlite", "libsqlite3-sys"):
        entries = [p for p in packages if p["name"] == name]
        require(len(entries) == 1, f"expected exactly one locked {name}")
        locked[name] = (entries[0]["version"], entries[0].get("checksum", ""))
    return locked


def check_lock(ledger, lock):
    """Offline: the lockfile still resolves the crates the ledger describes."""
    expected = {"rusqlite": (ledger["locked"]["rusqlite"], ledger["locked"]["rusqlite_checksum"]),
                "libsqlite3-sys": (ledger["locked"]["libsqlite3_sys"], ledger["locked"]["libsqlite3_sys_checksum"])}
    findings = [f"Cargo.lock resolves {name} {lock[name][0]} (checksum {lock[name][1][:16]}…) but the ledger records {expected[name][0]}; update the ledger and the native tests together"
                for name in expected if lock[name] != expected[name]]
    return {"check": "lockfile matches ledger", "observed": {n: v for n, (v, _) in lock.items()}, "findings": findings}


def index_rows(name, get):
    require(CRATE_NAME.fullmatch(name), "crate name")
    prefix = f"{name[:2]}/{name[2:4]}" if len(name) >= 4 else {3: f"3/{name[0]}", 2: "2", 1: "1"}[len(name)]
    rows = [json.loads(line) for line in get(f"{INDEX}/{prefix}/{name}").decode().splitlines() if line.strip()]
    require(all(r.get("name") == name for r in rows), "index rows name mismatch")
    return rows


def latest_stable(rows):
    stable = [r for r in rows if not r["yanked"] and VERSION.fullmatch(r["vers"])]
    require(stable, "no stable registry version")
    return max(stable, key=lambda r: version_tuple(r["vers"]))


def bundled_sqlite(name, version, checksum, get):
    """Read SQLITE_VERSION and SQLITE_SOURCE_ID from a crate archive without extracting it."""
    require(SHA256.fullmatch(checksum), "crate checksum shape")
    data = get(f"{CRATES}/{name}/{name}-{version}.crate", CRATE_LIMIT)
    require(hashlib.sha256(data).hexdigest() == checksum, "crate checksum mismatch")
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as compressed:
        expanded = compressed.read(CRATE_EXPANDED_LIMIT + 1)
    require(len(expanded) <= CRATE_EXPANDED_LIMIT, "crate expansion exceeds size limit")
    with tarfile.open(fileobj=io.BytesIO(expanded), mode="r:") as archive:
        member = archive.getmember(f"{name}-{version}/sqlite3/sqlite3.h")
        require(member.isfile() and member.size <= SMALL, "unexpected header member")
        header_data = archive.extractfile(member).read(member.size + 1)
        require(len(header_data) == member.size, "truncated header member")
        header = header_data.decode(errors="replace")
    sqlite = re.search(r'^#define SQLITE_VERSION\s+"([0-9.]+)"', header, re.M)
    source = re.search(r'^#define SQLITE_SOURCE_ID\s+"([^"]+)"', header, re.M)
    require(sqlite and source, "header lacks version macros")
    require(VERSION.fullmatch(sqlite.group(1)), "header SQLite version")
    require(SOURCE_ID.fullmatch(source.group(1)), "header SQLite source id")
    return sqlite.group(1), source.group(1)


def check_crates(ledger, get, today=None):
    """Registry freshness; a newer libsqlite3-sys is inspected for the SQLite it bundles."""
    observed, findings = {}, []
    revisit = ledger["triage"]["revisit_when_bundled_sqlite_at_least"]
    for name, key in (("rusqlite", "rusqlite"), ("libsqlite3-sys", "libsqlite3_sys")):
        locked = ledger["locked"][key]
        latest = latest_stable(index_rows(name, get))
        observed[name] = {"locked": locked, "latest_stable": latest["vers"]}
        if version_tuple(latest["vers"]) <= version_tuple(locked):
            continue
        note = f"{name} {latest['vers']} is published; {locked} is locked"
        if name == "libsqlite3-sys":
            version, source = bundled_sqlite(name, latest["vers"], latest["cksum"], get)
            observed[name]["bundled_sqlite"] = version
            observed[name]["bundled_source_id"] = source
            note += f"; it bundles SQLite {version}"
            if version_tuple(version) >= version_tuple(revisit):
                note += f", which reaches the acceptance revisit threshold {revisit}: revisit the accepted residual risk in {ledger['triage']['record']}"
        findings.append(note + ". A newer crate number does not by itself mean newer bundled SQLite; review under the dependency quarantine before any update")
    return {"check": "crate registry freshness", "observed": observed, "findings": findings}


def check_sqlite_release(ledger, get, today=None):
    """Upstream release freshness against the last triaged release."""
    page = get(DOWNLOADS).decode(errors="replace")
    versions = {m.group(1) for m in re.finditer(r"^PRODUCT,([0-9.]+),[^,]*sqlite-amalgamation-", page, re.M)}
    require(versions, "download page lists no amalgamation")
    latest = max(versions, key=version_tuple)
    triaged = ledger["triage"]["last_triaged_sqlite_release"]
    observed = {"latest": latest, "last_triaged": triaged, "linked": ledger["native"]["sqlite_version"]}
    findings = []
    if version_tuple(latest) > version_tuple(triaged):
        interval = TIMELINE.format(triaged, latest)
        timeline = get(interval).decode(errors="replace")
        marker = ledger["triage"]["last_triaged_check_in"]
        untriaged, seen, found_marker = [], set(), False
        spans = re.finditer(r'<span[^>]*class=["\'][^"\']*timelineHash[^"\']*["\'][^>]*>(.*?)</span>', timeline, re.S)
        for span in spans:
            match = re.search(r"/info/([0-9a-f]{10,64})", span.group(1))
            if not match:
                continue
            check_in = match.group(1)
            if check_in == marker:
                found_marker = True
                break
            if check_in not in seen:
                require(len(untriaged) < TIMELINE_CHECK_IN_LIMIT,
                        "timeline interval exceeds check-in report limit")
                seen.add(check_in)
                untriaged.append(check_in)
        require(found_marker, "timeline omits last triaged check-in")
        require(untriaged, "new release timeline has no new check-ins")
        observed["untriaged_check_ins"] = untriaged
        findings.append(f"SQLite {latest} is released; check-ins after {marker} (last triaged release {triaged}) need review: {', '.join(untriaged)}. Record one triage item per relevant fix: {interval}")
    return {"check": "SQLite upstream release", "observed": observed, "findings": findings}


def cve_rows(html):
    """Yield CVE IDs, a fixed version, and a fixed classification per upstream row."""
    for row in re.findall(r"<tr>(.*?)</tr>", html, re.S):
        cells = re.findall(r"<td[^>]*>(.*?)</td>", row, re.S)
        ids = sorted(set(CVE.findall(cells[0]))) if cells else []
        if len(cells) < 2 or not ids:
            continue
        fix = re.search(r"releaselog/([0-9]+)_([0-9]+)_([0-9]+)\.html", cells[1])
        text = re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", cells[1])).strip()
        classification = "not_library_bug" if fix is None and text.startswith("Not a bug in") else "unreleased_or_unspecified"
        yield ids, ".".join(fix.groups()) if fix else None, classification


def check_cves(ledger, get, today=None):
    """Upstream CVE list against the linked version and recorded dispositions."""
    today = today or datetime.datetime.now(datetime.timezone.utc).date()
    linked = version_tuple(ledger["native"]["sqlite_version"])
    dispositions = validate_cve_triage(ledger)
    counts = {"rows": 0, "fixed_in_linked_version": 0, "upstream_not_a_library_bug": 0, "ledger_disposition": 0}
    findings = []
    for ids, fix, classification in cve_rows(get(CVES).decode(errors="replace")):
        counts["rows"] += 1
        if fix is not None and version_tuple(fix) <= linked:
            counts["fixed_in_linked_version"] += len(ids)
        elif classification == "not_library_bug":
            counts["upstream_not_a_library_bug"] += len(ids)
        else:
            for cve in ids:
                if cve in dispositions and dispositions[cve][0] <= today <= dispositions[cve][1]:
                    counts["ledger_disposition"] += 1
                else:
                    status = f"fix `{fix}`" if fix else "an unreleased or unspecified upstream fix"
                    if cve in dispositions and today > dispositions[cve][1]:
                        disposition = f"; its ledger disposition expired {dispositions[cve][1].isoformat()}"
                    elif cve in dispositions:
                        disposition = f"; its ledger disposition is recorded for {dispositions[cve][0].isoformat()} or later"
                    else:
                        disposition = " and has no disposition in the ledger"
                    findings.append(f"{cve} shows {status} on {CVES}, is not covered by linked SQLite {ledger['native']['sqlite_version']}{disposition}; triage it")
    require(counts["rows"], "CVE page has no parseable rows")
    return {"check": "SQLite CVE list", "observed": counts, "findings": findings}


def check_review_deadline(ledger, today):
    deadline = datetime.date.fromisoformat(ledger["triage"]["review_deadline"])
    findings = [f"the residual-risk review deadline {deadline.isoformat()} has passed; renew it in the ledger after review"] if today > deadline else []
    return {"check": "residual-risk review deadline", "observed": {"deadline": deadline.isoformat(), "today": today.isoformat()}, "findings": findings}


NETWORK_CHECKS = [("crate registry freshness", check_crates),
                  ("SQLite upstream release", check_sqlite_release),
                  ("SQLite CVE list", check_cves)]


def guarded(name, check, ledger, get, today=None):
    """A network or upstream-format failure is reported as not checked, never raised."""
    try:
        return check(ledger, get, today)
    except Exception as error:
        return {"check": name, "observed": {}, "findings": [], "not_checked": type(error).__name__}


def report(ledger, lock, get, today, offline):
    results = [check_lock(ledger, lock), check_review_deadline(ledger, today)]
    for name, check in NETWORK_CHECKS:
        if offline:
            results.append({"check": name, "observed": {}, "findings": [], "not_checked": "offline"})
        else:
            results.append(guarded(name, check, ledger, get, today))
    return results


def render(ledger, results, today):
    native = ledger["native"]
    lines = ["## Native SQLite monitoring (report only)", "",
             f"Date: {today.isoformat()} UTC. Linked SQLite {native['sqlite_version']} (source id `{native['source_id']}`) via rusqlite {ledger['locked']['rusqlite']} and libsqlite3-sys {ledger['locked']['libsqlite3_sys']}; last triaged upstream release {ledger['triage']['last_triaged_sqlite_release']}, last triaged check-in `{ledger['triage']['last_triaged_check_in']}`.",
             "", "| Check | Observed | Result |", "| --- | --- | --- |"]
    triage = []
    for result in results:
        observed = json.dumps(result["observed"], sort_keys=True) if result["observed"] else "-"
        if "not_checked" in result:
            status = f"not checked ({result['not_checked']})"
        elif result["findings"]:
            status = f"{len(result['findings'])} triage item(s)"
        else:
            status = "no new item"
        lines.append(f"| {result['check']} | `{observed}` | {status} |")
        triage += result["findings"]
    lines.append("")
    if triage:
        lines.append("Triage items (record each in the ledger with ancestry, trigger, reachability, impact, disposition, owner, and review deadline; none of these is an automatic failure or upgrade):")
        lines += [f"- {item}" for item in triage]
    else:
        lines.append("No new triage item was found by the checks that ran. Checks marked 'not checked' provide no clearance.")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description="Report native SQLite dependency freshness and security findings.")
    parser.add_argument("--offline", action="store_true", help="skip every network read")
    parser.add_argument("--ledger", type=Path, default=LEDGER)
    parser.add_argument("--lock", type=Path, default=LOCK)
    parser.add_argument("--today", type=datetime.date.fromisoformat, default=datetime.datetime.now(datetime.timezone.utc).date())
    args = parser.parse_args()
    ledger = read_ledger(args.ledger)
    text = render(ledger, report(ledger, read_lock(args.lock), fetch, args.today, args.offline), args.today)
    sys.stdout.write(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as stream:
            stream.write(text)


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("SQLite monitor could not run; raw diagnostics withheld for hygiene", file=sys.stderr)
        sys.exit(1)
