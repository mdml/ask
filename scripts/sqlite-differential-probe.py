#!/usr/bin/env python3
"""Differential SQLite index-corruption probe across bundled-candidate versions.

Input contract mirrors scripts/sqlite-source-equivalence.py: one argument names a
directory holding the official SQLite amalgamation ZIPs; the script performs no
network access, verifies every input against pinned checksums, prints sanitized
JSON to stdout, and reports errors to stderr without raw diagnostics. Verification
uses unconditional checks rather than assert so results hold under python -O.

The probe compiles each amalgamation as a shared library with cc, builds one small
database (512-byte page, one text row, one index), corrupts a single index-cell
payload-length byte from 4 to 100, then runs an indexed SELECT and PRAGMA
integrity_check against every build and records the SQLite result codes. It
isolates the check-in 6826c17021 index-bounds behavior: the change is absent from
3.53.2 and present from 3.53.3 onward, so the same damaged file is expected to be
accepted by the 3.53.2 SELECT and rejected by later builds. See the evidence
README for reproduction and boundaries.
"""

import ctypes
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

# Pinned inputs. zip_sha256 is the official amalgamation archive digest; c_sha3_256
# is SQLite's published per-release sqlite3.c SHA3-256 (sqlite.org/changes.html).
VERSIONS = [
    {
        "version": "3.53.2",
        "zip": "sqlite-amalgamation-3530200.zip",
        "member": "sqlite-amalgamation-3530200",
        "zip_sha256": "8a310d0a16c7a90cacd4c884e70faa51c902afed2a89f63aaa0126ab83558a32",
        "c_sha3_256": "44fd61b9f93b4155105cb2d80c957ae6c64a8b5bd6ed51a4992f0dbd438e4e11",
        "has_index_bounds_fix": False,
    },
    {
        "version": "3.53.3",
        "zip": "sqlite-amalgamation-3530300.zip",
        "member": "sqlite-amalgamation-3530300",
        "zip_sha256": "646421e12aac110282ef8cc68f1a62d4bb15fc7b8f09da0b53e29ee690500431",
        "c_sha3_256": "28e484abdaa43630e34040ef6ed92be973a1ad54107803d8af5145b889c23ed7",
        "has_index_bounds_fix": True,
    },
    {
        "version": "3.53.4",
        "zip": "sqlite-amalgamation-3530400.zip",
        "member": "sqlite-amalgamation-3530400",
        "zip_sha256": "1e71ddf93849c6a6ecf58b827c0692073d2dd7ee40196158068f7b29f422e87d",
        "c_sha3_256": "67f423e9ebbbdc473cbc4772c872ee6b89f31fde4ed0279a5c25d5f65c043a16",
        "has_index_bounds_fix": True,
    },
]

# Feature flags approximating the candidate bundled build's core options. The
# probed behavior lives in btree index handling and is independent of these flags;
# they are recorded so the build is described rather than opaque.
COMPILE_FLAGS = [
    "-O1",
    "-fPIC",
    "-shared",
    "-DSQLITE_THREADSAFE=1",
    "-DSQLITE_ENABLE_API_ARMOR",
    "-DSQLITE_DEFAULT_FOREIGN_KEYS=1",
]

PAGE_SIZE = 512
CORRUPT_FROM = 4
CORRUPT_TO = 100

SQLITE_OK = 0
SQLITE_ROW = 100
SQLITE_DONE = 101
CODE_NAMES = {0: "SQLITE_OK", 11: "SQLITE_CORRUPT", 100: "SQLITE_ROW", 101: "SQLITE_DONE"}


def require(condition, message="verification failed"):
    if not condition:
        raise ValueError(message)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def sha3_256(data):
    return hashlib.sha3_256(data).hexdigest()


def name_code(code):
    return CODE_NAMES.get(code, f"code_{code}")


def load_source(inputs, spec):
    """Verify one archive and return its sqlite3.c and sqlite3.h bytes."""
    archive = inputs / spec["zip"]
    data = archive.read_bytes()
    require(sha256(data) == spec["zip_sha256"], "amalgamation archive checksum mismatch")
    with zipfile.ZipFile(archive) as z:
        source = z.read(f"{spec['member']}/sqlite3.c")
        header = z.read(f"{spec['member']}/sqlite3.h")
    require(sha3_256(source) == spec["c_sha3_256"], "sqlite3.c release digest mismatch")
    return source, header


def compile_library(source, header, workdir):
    """Compile one amalgamation into a shared library and return its path."""
    (workdir / "sqlite3.c").write_bytes(source)
    (workdir / "sqlite3.h").write_bytes(header)
    library = workdir / "libsqlite3probe.so"
    command = ["cc", *COMPILE_FLAGS, "sqlite3.c", "-o", library.name, "-lm", "-lpthread"]
    completed = subprocess.run(
        command, cwd=workdir, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    require(completed.returncode == 0, "amalgamation compilation failed")
    return library


class Sqlite:
    """Minimal ctypes binding to one loaded SQLite shared library."""

    def __init__(self, path):
        self.lib = ctypes.CDLL(str(path), mode=ctypes.RTLD_LOCAL)
        self.lib.sqlite3_open.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)]
        self.lib.sqlite3_exec.argtypes = [ctypes.c_void_p] * 2 + [ctypes.c_void_p] * 3
        self.lib.sqlite3_prepare_v2.argtypes = [
            ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int,
            ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(ctypes.c_char_p),
        ]
        self.lib.sqlite3_step.argtypes = [ctypes.c_void_p]
        self.lib.sqlite3_column_text.argtypes = [ctypes.c_void_p, ctypes.c_int]
        self.lib.sqlite3_column_text.restype = ctypes.c_char_p
        self.lib.sqlite3_finalize.argtypes = [ctypes.c_void_p]
        self.lib.sqlite3_close.argtypes = [ctypes.c_void_p]
        self.lib.sqlite3_libversion.restype = ctypes.c_char_p
        self.lib.sqlite3_sourceid.restype = ctypes.c_char_p

    def libversion(self):
        return self.lib.sqlite3_libversion().decode()

    def sourceid(self):
        return self.lib.sqlite3_sourceid().decode()

    def open(self, path):
        handle = ctypes.c_void_p()
        code = self.lib.sqlite3_open(str(path).encode(), ctypes.byref(handle))
        require(code == SQLITE_OK, "sqlite3_open failed")
        return handle

    def execute(self, db, sql):
        code = self.lib.sqlite3_exec(db, sql.encode(), None, None, None)
        require(code == SQLITE_OK, f"sqlite3_exec failed ({name_code(code)})")

    def step_query(self, db, sql):
        stmt = ctypes.c_void_p()
        prepare = self.lib.sqlite3_prepare_v2(
            db, sql.encode(), -1, ctypes.byref(stmt), None
        )
        if prepare != SQLITE_OK:
            return {"prepare_code": name_code(prepare), "step_code": None, "text": None}
        code = self.lib.sqlite3_step(stmt)
        text = None
        if code == SQLITE_ROW:
            raw = self.lib.sqlite3_column_text(stmt, 0)
            text = raw.decode() if raw is not None else None
        self.lib.sqlite3_finalize(stmt)
        return {"prepare_code": name_code(prepare), "step_code": name_code(code), "text": text}

    def close(self, db):
        self.lib.sqlite3_close(db)


def build_pristine_database(sqlite, workdir):
    path = workdir / "pristine.db"
    db = sqlite.open(path)
    # rowid 1 encodes as serial type 9 (zero payload bytes), so the index-leaf
    # record for text 'a' plus the rowid key has payload length 4.
    sqlite.execute(db, f"PRAGMA page_size={PAGE_SIZE};")
    sqlite.execute(db, "PRAGMA journal_mode=DELETE;")
    sqlite.execute(db, "CREATE TABLE t(v TEXT);")
    sqlite.execute(db, "INSERT INTO t(rowid, v) VALUES (1, 'a');")
    sqlite.execute(db, "CREATE INDEX ix ON t(v);")
    sqlite.close(db)
    return path.read_bytes()


def corrupt_index_cell(image):
    """Inflate the first index-leaf cell's payload-length byte from 4 to 100."""
    require(len(image) % PAGE_SIZE == 0, "unexpected database image size")
    for page in range(len(image) // PAGE_SIZE):
        base = page * PAGE_SIZE
        header = base + (100 if page == 0 else 0)
        if image[header] != 0x0A:  # 0x0A marks an index b-tree leaf page.
            continue
        cell_count = int.from_bytes(image[header + 3:header + 5], "big")
        if cell_count < 1:
            continue
        pointer = int.from_bytes(image[header + 8:header + 10], "big")
        target = base + pointer
        require(image[target] == CORRUPT_FROM, "unexpected index-cell payload length")
        mutated = bytearray(image)
        mutated[target] = CORRUPT_TO
        return bytes(mutated), {
            "page": page + 1,
            "page_type": "index-leaf",
            "cell_offset": pointer,
            "payload_length_before": CORRUPT_FROM,
            "payload_length_after": CORRUPT_TO,
        }
    raise ValueError("no index-leaf cell located")


def run_build(spec, corrupt_image, workdir):
    library = compile_library(spec["source"], spec["header"], workdir)
    sqlite = Sqlite(library)
    db_path = workdir / "case.db"
    db_path.write_bytes(corrupt_image)
    db = sqlite.open(db_path)
    select = sqlite.step_query(db, "SELECT v FROM t INDEXED BY ix WHERE v = 'a';")
    integrity = sqlite.step_query(db, "PRAGMA integrity_check;")
    sqlite.close(db)
    return {
        "version": spec["version"],
        "libversion": sqlite.libversion(),
        "sourceid": sqlite.sourceid(),
        "library": library.name,
        "has_index_bounds_fix": spec["has_index_bounds_fix"],
        "indexed_select": select,
        "integrity_check": {
            "step_code": integrity["step_code"],
            "report": integrity["text"],
        },
    }


def main():
    if len(sys.argv) != 2:
        raise ValueError("usage: sqlite-differential-probe.py <input-directory>")
    inputs = Path(sys.argv[1])
    require(inputs.is_dir(), "input directory not found")

    for spec in VERSIONS:
        spec["source"], spec["header"] = load_source(inputs, spec)

    result = {
        "probe": "sqlite-differential-index-corruption",
        "page_size": PAGE_SIZE,
        "corruption": {
            "target": "first index-leaf cell payload-length byte",
            "from": CORRUPT_FROM,
            "to": CORRUPT_TO,
        },
        "compile_flags": COMPILE_FLAGS,
        "indexed_select_sql": "SELECT v FROM t INDEXED BY ix WHERE v = 'a';",
        "builds": [],
    }

    with tempfile.TemporaryDirectory(prefix="sqlite-diff-") as tmp:
        root = Path(tmp)
        # Build the pristine, then corrupt, image once with the first version.
        first = VERSIONS[0]
        setup_dir = root / "setup"
        setup_dir.mkdir()
        setup_lib = compile_library(first["source"], first["header"], setup_dir)
        pristine = build_pristine_database(Sqlite(setup_lib), setup_dir)
        corrupt_image, corruption = corrupt_index_cell(pristine)
        result["corruption"].update(corruption)
        result["database_sha256"] = {
            "pristine": sha256(pristine),
            "corrupt": sha256(corrupt_image),
        }
        for index, spec in enumerate(VERSIONS):
            case_dir = root / f"case{index}"
            case_dir.mkdir()
            result["builds"].append(run_build(spec, corrupt_image, case_dir))

    # Expected differential: only the pre-fix build accepts the damaged cell in
    # the indexed SELECT; every build's integrity_check reports corruption.
    baseline = result["builds"][0]["indexed_select"]["step_code"]
    fixed = [b["indexed_select"]["step_code"] for b in result["builds"] if b["has_index_bounds_fix"]]
    result["summary"] = {
        "pre_fix_select": baseline,
        "post_fix_select": sorted(set(fixed)),
        "integrity_check_codes": sorted(
            {b["integrity_check"]["step_code"] for b in result["builds"]}
        ),
        "differential_observed": baseline == "SQLITE_ROW"
        and all(code == "SQLITE_CORRUPT" for code in fixed),
    }
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print(
            "SQLite differential probe failed; raw diagnostics withheld for hygiene",
            file=sys.stderr,
        )
        sys.exit(1)
