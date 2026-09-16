#!/usr/bin/env python3
"""Generate the stable Homebrew formula from a published SHA256SUMS file."""
import argparse
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = (
    "aarch64-apple-darwin", "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu",
)


def checksums(path):
    values = {}
    for line in path.read_text().splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ask-([A-Za-z0-9_-]+)\.tar\.gz", line)
        if not match or match.group(2) not in TARGETS or match.group(2) in values:
            raise ValueError("invalid SHA256SUMS inventory")
        values[match.group(2)] = match.group(1)
    if set(values) != set(TARGETS):
        raise ValueError("invalid SHA256SUMS inventory")
    return values


def cargo_version():
    value = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", value):
        raise ValueError("Cargo version must be stable semantic version")
    return value


def formula(values, version):
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("formula version must be stable semantic version")
    def source(target, indent):
        url = f"https://github.com/mdml/ask/releases/download/v{version}/ask-{target}.tar.gz"
        return f'{indent}url "{url}"\n{indent}sha256 "{values[target]}"'

    return f'''class Ask < Formula
  desc "Fast, opinionated terminal lookup tool for language models"
  homepage "https://github.com/mdml/ask"
  version "{version}"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
{source("aarch64-apple-darwin", "      ")}
    else
{source("x86_64-apple-darwin", "      ")}
    end
  end

  on_linux do
    if Hardware::CPU.arm?
{source("aarch64-unknown-linux-gnu", "      ")}
    else
{source("x86_64-unknown-linux-gnu", "      ")}
    end
  end

  def install
    bin.install "ask"
  end

  test do
    assert_predicate bin/"ask", :executable?
  end
end
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checksums", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(formula(checksums(args.checksums), cargo_version()))


if __name__ == "__main__":
    main()
