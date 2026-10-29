#!/usr/bin/env python3
"""Validate an existing revision/tag and record artifact-only build inputs."""

import argparse
import json
import re
import subprocess
import tomllib
from pathlib import Path

from release_common import checked_version


def git(root, *arguments):
    process = subprocess.run(
        ["git", "--no-optional-locks", "-C", str(root), *arguments],
        check=True, capture_output=True, text=True, encoding="utf-8",
    )
    return process.stdout.strip()


def metadata(root, tag=None):
    root = Path(root).resolve()
    package = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    if package["name"] != "writer":
        raise ValueError("release input is not the writer package")
    version = checked_version(package["version"])
    revision = git(root, "rev-parse", "--verify", "HEAD^{commit}")
    if not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", revision):
        raise ValueError("HEAD did not resolve to a full immutable revision")
    if git(root, "status", "--porcelain", "--untracked-files=no"):
        raise ValueError("release checkout has tracked changes")
    epoch = int(git(root, "show", "-s", "--format=%ct", "HEAD"))
    if not 0 <= epoch <= 0xFFFFFFFF:
        raise ValueError("commit timestamp does not fit deterministic gzip metadata")
    if tag:
        if tag != f"v{version}":
            raise ValueError(f"existing tag must equal v{version}, got {tag!r}")
        tagged = git(root, "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
        if tagged != revision:
            raise ValueError("existing release tag does not name the selected revision")
    # Artifact-only revision builds always identify their actual commit, even
    # when an optional existing matching tag was checked. No tag is created.
    return {
        "version": version, "revision": revision, "build_id": revision,
        "epoch": epoch, "tag": tag or None,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("."))
    parser.add_argument("--tag")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    try:
        result = metadata(args.source, args.tag)
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n",
            encoding="utf-8", newline="\n"
        )
        if args.github_output:
            with args.github_output.open("a", encoding="utf-8", newline="\n") as output:
                for key in ["version", "revision", "build_id", "epoch"]:
                    output.write(f"{key}={result[key]}\n")
    except (KeyError, OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"error: {error}\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
