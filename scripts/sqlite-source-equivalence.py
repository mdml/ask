#!/usr/bin/env python3
"""Compare downloaded exact candidate archives. Inputs and retrieval commands: assessment."""
import copy
from datetime import datetime, timezone
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import tomllib
import zipfile

COMMIT = "e88f112bef7899234a497baed5cc3c3d553deeb8"
# SQLite 3.53.2 release history's sqlite3.c digest.
SQLITE_SHA3 = "44fd61b9f93b4155105cb2d80c957ae6c64a8b5bd6ed51a4992f0dbd438e4e11"


def require(condition, message="verification failed"):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def archive(data):
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        require(all(m.isfile() or m.isdir() or m.issym() for m in tar.getmembers()))
        return {m.name.split("/", 1)[1]: tar.extractfile(m).read()
                for m in tar.getmembers() if m.isfile() or m.issym()}


def normalize(original, source):
    result = copy.deepcopy(original)
    package = result["package"]
    for key in ["autolib", "autobins", "autoexamples", "autotests", "autobenches"]:
        package[key] = False
    package.setdefault("build", False)
    if "README.md" in source:
        package.setdefault("readme", "README.md")
    workspace = result.pop("workspace", None)
    if result.get("lints") == {"workspace": True}:
        result["lints"] = workspace["lints"]
    def deps(table):
        for kind in ["dependencies", "dev-dependencies", "build-dependencies"]:
            for name, value in table.get(kind, {}).items():
                if isinstance(value, str):
                    value = {"version": value}
                    table[kind][name] = value
                value.pop("path", None)
    deps(result)
    for table in result.get("target", {}).values():
        deps(table)
    result.setdefault("lib", {"name": package["name"].replace("-", "_")})["path"] = "src/lib.rs"
    for kind, directory in [("example", "examples"), ("test", "tests"), ("bench", "benches")]:
        targets = {t["name"]: t for t in result.get(kind, [])}
        for name in source:
            parts = Path(name).parts
            if len(parts) == 2 and parts[0] == directory and name.endswith(".rs"):
                targets.setdefault(Path(name).stem, {"name": Path(name).stem})["path"] = name
            elif len(parts) == 3 and parts[0] == directory and parts[-1] == "main.rs":
                targets.setdefault(parts[1], {"name": parts[1]})["path"] = name
        if targets:
            result[kind] = sorted(targets.values(), key=lambda t: t["name"])
    return result


def main():
    inputs = Path(sys.argv[1])
    upstream_data = (inputs / "sqlite-source.tar.gz").read_bytes()
    upstream = archive(upstream_data)
    tag = json.loads((inputs / "sqlite-tag.json").read_text())
    require(tag["object"] == {
        "sha": COMMIT, "type": "commit",
        "url": f"https://api.github.com/repos/rusqlite/rusqlite/git/commits/{COMMIT}"})
    result = {"retrieved": datetime.now(timezone.utc).date().isoformat(), "commit": COMMIT,
              "source_archive_sha256": digest(upstream_data), "crates": []}
    for short, name, version, sub in [
        ("rusqlite", "rusqlite", "0.40.2", ""),
        ("sys", "libsqlite3-sys", "0.38.2", "libsqlite3-sys/"),
    ]:
        rows = [json.loads(line) for line in (inputs / f"sqlite-{short}-index").read_text().splitlines()]
        selected = next(row for row in rows if row["vers"] == version)
        stable = [r["vers"] for r in rows if not r["yanked"] and "-" not in r["vers"]]
        latest = max(stable, key=lambda v: tuple(map(int, v.split("."))))
        data = (inputs / f"sqlite-{short}.crate").read_bytes()
        require(digest(data) == selected["cksum"] and not selected["yanked"])
        package = archive(data)
        source = {p[len(sub):]: d for p, d in upstream.items() if p.startswith(sub)}
        vcs = json.loads(package[".cargo_vcs_info.json"])
        require(vcs["git"]["sha1"] == COMMIT and not vcs["git"].get("dirty", False))
        require(vcs["path_in_vcs"] == sub.rstrip("/"))
        require(package["Cargo.toml.orig"] == source["Cargo.toml"])
        original = tomllib.loads(package["Cargo.toml.orig"].decode())
        normalized = tomllib.loads(package["Cargo.toml"].decode())
        require(normalize(original, source) == normalized)
        generated = {"Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json"}
        inventory = []
        for path, content in sorted(package.items()):
            if path not in generated:
                require(source[path] == content, path)
            inventory.append({"path": path, "sha256": digest(content),
                              "status": "packaging metadata" if path in generated else "byte-identical"})
        lock = tomllib.loads(package["Cargo.lock"].decode())
        # These are publisher-generated resolution records, not our probe lock.
        require(all(not p.get("source") or p["source"] == "registry+https://github.com/rust-lang/crates.io-index"
                   for p in lock["package"]))
        result["crates"].append({
            "name": name, "version": version, "latest_non_yanked_stable": latest,
            "archive_sha256": digest(data), "registry_checksum_matches": True,
            "registry_index_sha256": digest((inputs / f"sqlite-{short}-index").read_bytes()),
            "vcs": vcs, "original_manifest_byte_identical": True,
            "normalized_manifest_reconstructed_equal": True,
            "files": inventory,
            "source_only": [{"path": p, "sha256": digest(d)} for p, d in sorted(source.items()) if p not in package],
            "publisher_lock_packages": [{k: p[k] for k in ["name", "version", "source", "checksum"] if k in p}
                                        for p in lock["package"]],
        })
    with zipfile.ZipFile(inputs / "sqlite-amalgamation.zip") as z:
        comparisons = []
        for name in ["sqlite3.c", "sqlite3.h", "sqlite3ext.h"]:
            official = z.read("sqlite-amalgamation-3530200/" + name)
            if name == "sqlite3.c":
                require(hashlib.sha3_256(official).hexdigest() == SQLITE_SHA3)
            bundled = upstream["libsqlite3-sys/sqlite3/" + name]
            require(official == bundled)
            comparisons.append({"path": name, "sha256": digest(bundled), "byte_identical": True})
    result["official_amalgamation_archive_sha256"] = digest((inputs / "sqlite-amalgamation.zip").read_bytes())
    result["sqlite3_c_release_sha3_256"] = SQLITE_SHA3
    result["official_amalgamation"] = comparisons
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("SQLite source correspondence failed; raw diagnostics withheld for hygiene", file=sys.stderr)
        sys.exit(1)
