#!/usr/bin/env python3
"""Prepare and validate stable releases without changing GitHub state."""
import argparse
import datetime
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("release_packaging", Path(__file__).with_name("nightly-release.py"))
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


def stable_tag():
    return f"v{release.version()}"


def prepare(sha):
    release.require(os.environ.get("GITHUB_REPOSITORY") == "mdml/ask", "stable release requires canonical repository")
    release.require(os.environ.get("GITHUB_REF") == "refs/heads/stable", "stable release requires stable branch")
    release.require(os.environ.get("GITHUB_REF_PROTECTED") == "true", "stable branch must be protected")
    release.require(os.environ.get("GITHUB_EVENT_NAME") == "push", "stable release requires push event")
    release.require(release.run(["git", "rev-parse", "HEAD"]).strip() == sha, "checkout SHA mismatch")
    release.require(datetime.datetime.now(datetime.timezone.utc).date() <= release.ACTION_REVIEW_DEADLINE,
                    "action dependency disposition expired; review required")
    tag = stable_tag()
    refs = release.github_read("mdml/ask", f"git/matching-refs/tags/{tag}")
    release.require(isinstance(refs, list) and all(isinstance(ref, dict) for ref in refs),
                    "tag lookup must return a list of refs")
    exact = f"refs/tags/{tag}"
    release.require(not any(ref.get("ref") == exact for ref in refs), "stable tag already exists")
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"tag={tag}\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["prepare", "package", "verify", "verify-upload"])
    parser.add_argument("--target", choices=release.TARGETS)
    parser.add_argument("--source", type=Path, default=Path("incoming"))
    parser.add_argument("--destination", type=Path, default=Path("release"))
    args = parser.parse_args()
    sha = os.environ["RELEASE_SHA"]
    tag = os.environ.get("RELEASE_TAG", stable_tag())
    release.require(tag == stable_tag(), "stable tag must match Cargo version")
    if args.command == "prepare":
        prepare(sha)
    elif args.command == "package":
        release.require(args.target in release.TARGETS, "target required")
        release.package(args.target, sha, tag, args.destination)
    elif args.command == "verify-upload":
        release.verify_upload(json.load(sys.stdin), args.destination, sha, tag, prerelease=False)
    else:
        release.verify(args.source, args.destination, sha, tag)


if __name__ == "__main__":
    main()
