"""Fixed release artifact vocabulary; no publishing operations."""

import hashlib
import re
from pathlib import Path

TARGETS = (
    "aarch64-apple-darwin", "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc",
)
MAX_BINARY_BYTES = 67_108_864
VERSION = re.compile(
    r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?"
)


def checked_version(value):
    if not VERSION.fullmatch(value):
        raise ValueError(f"invalid Cargo package version: {value!r}")
    return value


def checked_target(value):
    if value not in TARGETS:
        raise ValueError(f"unsupported release target: {value!r}")
    return value


def root_name(version, target):
    return f"writer-v{checked_version(version)}-{checked_target(target)}"


def binary_name(target):
    return "writer.exe" if checked_target(target).endswith("-msvc") else "writer"


def archive_name(version, target):
    extension = ".zip" if checked_target(target).endswith("-msvc") else ".tar.gz"
    return root_name(version, target) + extension


def digest(path):
    result = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()
