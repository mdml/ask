#!/usr/bin/env python3
"""Package and validate nightly archives, including an offline packaged-binary proof."""
import argparse
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = (
    "aarch64-apple-darwin", "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu",
)
MAX_BINARY = 128 * 1024 * 1024
ACTION_REVIEW_DEADLINE = datetime.date(2026, 9, 29)
VALID_CONFIG = b'''default_profile = "offline"

[providers.offline]
kind = "openai-compatible"
base_url = "http://127.0.0.1:1/v1"
api_key_env = "OFFLINE_PROVIDER_KEY"

[profiles.offline]
provider = "offline"
model = "fixture"
'''
INVALID_CONFIG = b"unknown = true\n" + VALID_CONFIG


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def regular(path):
    require(path.is_file() and not path.is_symlink(), "expected regular file")
    return path.read_bytes()


def version():
    value = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", value), "expected Cargo release version")
    return value


def identity(sha, tag):
    require(re.fullmatch(r"[0-9a-f]{40}", sha), "invalid source SHA")
    require(re.fullmatch(rf"v{re.escape(version())}-nightly\.[0-9]{{8}}\.[1-9][0-9]*\.[1-9][0-9]*", tag), "invalid nightly tag")
    return {"source_sha": sha, "tag": tag, "cargo_version": version(),
            "lock_sha256": digest((ROOT / "Cargo.lock").read_bytes()), "rust": "1.97.1"}


def run(args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def linkage(binary, target):
    symbols = run(["nm", str(binary)])
    # Inspect the production binary before strip; sqlite3_open_v2 is used by storage.
    require(re.search(r"\b[Tt] _?sqlite3_open_v2$", symbols, re.M), "SQLite not defined in production binary")
    if "linux" in target:
        output = run(["readelf", "-d", str(binary)])
        libraries = re.findall(r"\(NEEDED\).*\[(.*?)\]", output)
    else:
        output = run(["otool", "-L", str(binary)])
        libraries = [Path(line.strip().split(" (", 1)[0]).name for line in output.splitlines()[1:]]
    require(libraries and all(re.fullmatch(r"[A-Za-z0-9_.+-]+", lib) for lib in libraries), "invalid linkage inventory")
    require(all("sqlite" not in lib.lower() for lib in libraries), "dynamic SQLite dependency")
    return {"sqlite3_open_v2_defined": True, "dynamic_sqlite": False,
            "libraries": sorted(libraries)}


def archive_name(target):
    require(target in TARGETS, "unsupported target")
    return f"ask-{target}.tar.gz"


def package(target, sha, tag, destination):
    info = identity(sha, tag)
    host = run(["rustc", "+1.97.1", "-vV"])
    require(f"host: {target}\n" in host and "release: 1.97.1\n" in host, "native Rust 1.97.1 required")
    binary = ROOT / "target" / target / "release/ask"
    regular(binary)
    native = linkage(binary, target)
    destination.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory() as temp:
        stripped = Path(temp) / "ask"
        shutil.copyfile(binary, stripped)
        run(["strip", str(stripped)])
        data = stripped.read_bytes()
    require(0 < len(data) <= MAX_BINARY, "invalid binary size")
    info.update({"target": target, "binary_sha256": digest(data), "linkage": native})
    files = {"ask": data, "LICENSE": (ROOT / "LICENSE").read_bytes(),
             "manifest.json": (json.dumps(info, sort_keys=True, indent=2) + "\n").encode()}
    name = archive_name(target)
    with tarfile.open(destination / name, "w:gz", format=tarfile.USTAR_FORMAT) as archive:
        for filename, content in files.items():
            member = tarfile.TarInfo(filename)
            member.size = len(content)
            member.mode = 0o755 if filename == "ask" else 0o644
            archive.addfile(member, io.BytesIO(content))
    checksum = digest((destination / name).read_bytes())
    (destination / f"{name}.sha256").write_text(f"{checksum}  {name}\n")
    validate_target(destination, target, sha, tag, prove_executable=True)


def validate_target(directory, target, sha, tag, prove_executable=False):
    name = archive_name(target)
    require(directory.is_dir() and not directory.is_symlink(), "expected target directory")
    require({p.name for p in directory.iterdir()} == {name, name + ".sha256"}, "target inventory mismatch")
    require((directory / name).stat().st_size <= MAX_BINARY, "archive too large")
    data = regular(directory / name)
    checksum = digest(data)
    require(regular(directory / (name + ".sha256")) == f"{checksum}  {name}\n".encode(), "archive digest mismatch")
    contents = {}
    # Read only bounded regular members. Never call extract/extractall.
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive:
            require(member.name in {"ask", "LICENSE", "manifest.json"} and member.name not in contents, "archive inventory mismatch")
            require(member.isfile() and not member.pax_headers, "nonregular archive member")
            limit = MAX_BINARY if member.name == "ask" else 64 * 1024
            require(0 < member.size <= limit, "invalid member size")
            expected_mode = 0o755 if member.name == "ask" else 0o644
            require(member.mode == expected_mode, "invalid member permissions")
            contents[member.name] = archive.extractfile(member).read(limit + 1)
    require(set(contents) == {"ask", "LICENSE", "manifest.json"}, "missing archive member")
    require(contents["LICENSE"] == (ROOT / "LICENSE").read_bytes(), "license mismatch")
    try:
        info = json.loads(contents["manifest.json"])
    except json.JSONDecodeError as error:
        raise ValueError("invalid manifest JSON") from error
    require(isinstance(info, dict), "manifest must be a JSON object")
    expected = identity(sha, tag)
    require(set(info) == set(expected) | {"target", "binary_sha256", "linkage"}, "manifest fields mismatch")
    require(all(info.get(k) == v for k, v in expected.items()), "source identity mismatch")
    require(info["target"] == target and info["binary_sha256"] == digest(contents["ask"]), "binary identity mismatch")
    native = info["linkage"]
    require(isinstance(native, dict) and set(native) == {"sqlite3_open_v2_defined", "dynamic_sqlite", "libraries"}, "linkage fields mismatch")
    require(native["sqlite3_open_v2_defined"] is True and native["dynamic_sqlite"] is False, "invalid SQLite linkage")
    libs = native["libraries"]
    require(isinstance(libs, list) and libs and all(isinstance(lib, str) and re.fullmatch(r"[A-Za-z0-9_.+-]+", lib) and "sqlite" not in lib.lower() for lib in libs), "invalid library inventory")
    if prove_executable:
        packaged_binary_proof(contents["ask"])
    return checksum


def packaged_binary_proof(binary):
    """Run only the just-validated archive bytes in an isolated offline home."""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        executable = root / "ask"
        executable.write_bytes(binary)
        executable.chmod(0o700)
        home = root / "ask-home"
        environment = {"ASK_HOME": str(home)}
        for candidate, expected_status, expected_stderr in [
            (VALID_CONFIG, 0, b"ask: standard input is a valid configuration\n"),
            (INVALID_CONFIG, 1, b"ask: standard input is not a valid configuration: configuration: unknown field at key index 4\n"),
        ]:
            result = subprocess.run([str(executable), "configure", "check", "-"], input=candidate,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=root,
                                    env=environment, check=False, timeout=10)
            require(result.returncode == expected_status, "packaged configure check status mismatch")
            require(result.stdout == b"" and result.stderr == expected_stderr,
                    "packaged configure check stream mismatch")
            require(not home.exists(), "packaged configure check wrote state")


def verify(source, destination, sha, tag):
    require(source.is_dir() and not source.is_symlink(), "expected artifact directory")
    require({p.name for p in source.iterdir()} == set(TARGETS), "missing or unexpected target")
    # Validate everything before creating publishable output.
    checksums = {t: validate_target(source / t, t, sha, tag) for t in TARGETS}
    destination.mkdir(parents=True, exist_ok=False)
    for target in TARGETS:
        name = archive_name(target)
        shutil.copyfile(source / target / name, destination / name)
    (destination / "SHA256SUMS").write_text("".join(f"{checksums[t]}  {archive_name(t)}\n" for t in TARGETS))


def verify_upload(release, directory, sha, tag):
    require(isinstance(release, dict), "release response must be an object")
    required = {"tag_name": str, "target_commitish": str, "draft": bool,
                "prerelease": bool, "assets": list}
    for field, kind in required.items():
        require(field in release and type(release[field]) is kind,
                f"release response has invalid {field}")
    require(release["tag_name"] == tag and release["target_commitish"] == sha,
            "uploaded release identity mismatch")
    require(release["draft"] is True and release["prerelease"] is True,
            "expected draft prerelease")
    expected = {archive_name(t) for t in TARGETS} | {"SHA256SUMS"}
    assets = release["assets"]
    for asset in assets:
        require(isinstance(asset, dict), "release asset must be an object")
        for field, kind in {"name": str, "size": int, "state": str, "digest": str}.items():
            require(field in asset and type(asset[field]) is kind,
                    f"release asset has invalid {field}")
    require(len(assets) == len(expected) and {a["name"] for a in assets} == expected,
            "uploaded asset inventory mismatch")
    for asset in assets:
        data = regular(directory / asset["name"])
        require(asset["state"] == "uploaded" and asset["size"] == len(data)
                and asset["digest"] == "sha256:" + digest(data),
                "uploaded asset digest mismatch")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["prepare", "package", "verify", "verify-upload"])
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--source", type=Path, default=Path("incoming"))
    parser.add_argument("--destination", type=Path, default=Path("release"))
    args = parser.parse_args()
    sha = os.environ["RELEASE_SHA"]
    if args.command == "prepare":
        require(run(["git", "rev-parse", "HEAD"]).strip() == sha, "checkout SHA mismatch")
        today = datetime.datetime.now(datetime.timezone.utc).date()
        require(today <= ACTION_REVIEW_DEADLINE, "action dependency disposition expired; review required")
        date = today.strftime("%Y%m%d")
        tag = f"v{version()}-nightly.{date}.{os.environ['GITHUB_RUN_ID']}.{os.environ['GITHUB_RUN_ATTEMPT']}"
        identity(sha, tag)
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"tag={tag}\n")
    elif args.command == "package":
        require(args.target in TARGETS, "target required")
        package(args.target, sha, os.environ["RELEASE_TAG"], args.destination)
    elif args.command == "verify-upload":
        verify_upload(json.load(sys.stdin), args.destination, sha, os.environ["RELEASE_TAG"])
    else:
        verify(args.source, args.destination, sha, os.environ["RELEASE_TAG"])


if __name__ == "__main__":
    main()
