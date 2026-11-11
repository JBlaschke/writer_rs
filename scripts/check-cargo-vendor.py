#!/usr/bin/env python3
"""Compare the Cargo vendor directory with the crates.io archives.

Stdlib Python 3.11 or newer; never runs Cargo. The workspace builds from the
directory that .cargo/config.toml substitutes for crates.io. Cargo checks a
vendored file only against the .cargo-checksum.json committed beside it, so a
build does not show that a vendored crate is the crate crates.io published.
This helper fetches the archive of every crates.io package in Cargo.lock and
compares each vendored directory with it. docs/dev/cargo-vendor.md says what
a pass proves and what it does not.
"""

import argparse
import collections
from concurrent.futures import ThreadPoolExecutor
import gzip
import hashlib
import http.client
import io
import json
import os
from pathlib import Path
import re
import stat
import sys
import tarfile
import threading
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request
import zlib


ROOT = Path(__file__).resolve().parent.parent
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
DOWNLOAD_ROOT = "https://static.crates.io/crates"
# Names this tool and nothing else: no project, person or address.
USER_AGENT = (
    "cargo-vendor-check/1 "
    "(compares a committed cargo vendor tree with crates.io archives)"
)
RECORD = ".cargo-checksum.json"
NAME = re.compile(r"[A-Za-z0-9_-]+")
VERSION = re.compile(r"[0-9][A-Za-z0-9.+-]*")
SHA256 = re.compile(r"[0-9a-f]{64}")
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
ATTEMPTS = 3
SHOWN = 20

Comparison = collections.namedtuple("Comparison", "problems files size left")


class VendorError(ValueError):
    """The lock file or the vendoring configuration cannot be used."""


class FetchError(Exception):
    """A published archive could not be obtained."""


class Report:
    """What a run compared and every difference it found, by subject."""

    def __init__(self):
        self.problems = {}
        self.crates = 0
        self.failed = 0
        self.skipped = 0
        self.files = 0
        self.size = 0
        self.left = collections.Counter()

    def add(self, subject, lines):
        if lines:
            self.problems.setdefault(subject, []).extend(lines)


def sha256(raw):
    return hashlib.sha256(raw).hexdigest()


def vendor_directory(workspace):
    """Find the directory that this workspace substitutes for crates.io.

    Only the workspace's own .cargo/config.toml counts. Cargo resolves a
    relative `directory` against the parent of .cargo.
    """
    config = Path(workspace) / ".cargo" / "config.toml"
    try:
        settings = tomllib.loads(config.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise VendorError(
            f"unreadable Cargo configuration: {config}: {error}"
        ) from error

    def table(value):
        return value if isinstance(value, dict) else {}

    sources = table(settings.get("source"))
    replacement = table(sources.get("crates-io")).get("replace-with")
    directory = None
    if isinstance(replacement, str):
        directory = table(sources.get(replacement)).get("directory")
    if not isinstance(directory, str):
        raise VendorError(
            f"{config} does not replace crates-io with a directory source"
        )
    return Path(workspace) / directory


def locked_crates(workspace):
    """Map each crates.io package of Cargo.lock to its archive checksum.

    A package without a source is a path package, such as the workspace's
    own, and is not vendored. A package from any other source is returned as
    a problem: there is no crates.io archive to compare its vendored copy
    with.
    """
    lock = Path(workspace) / "Cargo.lock"
    try:
        data = tomllib.loads(lock.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise VendorError(f"unreadable lock file: {lock}: {error}") from error
    packages = data.get("package")
    if not isinstance(packages, list):
        raise VendorError(f"{lock} has no package list")
    locked, foreign = {}, []
    for package in packages:
        fields = package if isinstance(package, dict) else {}
        name, version = fields.get("name"), fields.get("version")
        source, checksum = fields.get("source"), fields.get("checksum")
        if not valid_identity(name, version):
            raise VendorError(f"{lock} has an invalid package identity")
        if source is None:
            continue
        if source != CRATES_IO:
            foreign.append(
                f"{name} {version} comes from {source}: only crates.io "
                "packages can be compared"
            )
            continue
        if not isinstance(checksum, str) or not SHA256.fullmatch(checksum):
            raise VendorError(f"{lock} has no SHA-256 for {name} {version}")
        if (name, version) in locked:
            raise VendorError(f"{lock} lists {name} {version} twice")
        locked[(name, version)] = checksum
    return locked, foreign


def valid_identity(name, version):
    return (
        isinstance(name, str) and NAME.fullmatch(name) is not None
        and isinstance(version, str) and VERSION.fullmatch(version) is not None
    )


def manifest_identity(manifest):
    """Read the package name and version of a vendored Cargo.toml, if any."""
    if manifest.is_symlink() or not manifest.is_file():
        return None
    try:
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    package = data.get("package")
    fields = package if isinstance(package, dict) else {}
    name, version = fields.get("name"), fields.get("version")
    return (name, version) if valid_identity(name, version) else None


def vendored_crates(vendor):
    """Map each package identity to its crate directory under `vendor`.

    The identity comes from the directory's Cargo.toml, as in Cargo's
    directory source. `cargo vendor` names the directory `<name>`, or
    `<name>-<version>` when it vendors a newer version of that crate too.
    Returns the crates, and the problems of each entry that is not one.
    """
    crates, problems = {}, {}
    for entry in sorted(vendor.iterdir()):
        if entry.is_symlink() or not entry.is_dir():
            problems[entry.name] = ["not a crate directory"]
            continue
        identity = manifest_identity(entry / "Cargo.toml")
        if identity is None:
            problems[entry.name] = [
                "no Cargo.toml with a package name and version"
            ]
            continue
        name, version = identity
        if entry.name not in {name, f"{name}-{version}"}:
            problems[entry.name] = [
                f"the directory name does not fit its package, "
                f"{name} {version}"
            ]
        elif identity in crates:
            problems[entry.name] = [
                f"{name} {version} is also in {crates[identity].name}"
            ]
        else:
            crates[identity] = entry
    return crates, problems


def vendored_files(directory):
    """Map each file of a crate directory to its path, without the record.

    A link or a special file is a problem: it could show the build other
    bytes than it shows this check. A directory that cannot be listed raises
    OSError; os.walk() would otherwise pass over it and its files.
    """
    def refuse(error):
        raise error

    files, problems = {}, []
    for current, directories, names in os.walk(directory, onerror=refuse):
        directories.sort()
        for name in directories + sorted(names):
            path     = Path(current) / name
            relative = path.relative_to(directory).as_posix()
            mode     = path.lstat().st_mode
            if stat.S_ISREG(mode):
                files[relative] = path
            elif not stat.S_ISDIR(mode):
                problems.append(f"not a regular file: {relative}")
    files.pop(RECORD, None)
    return files, problems


def left_out(relative):
    """Whether `cargo vendor` leaves an archive member out of the directory.

    Cargo 1.99.1 (5f94df47) unpacks a registry archive straight into the
    vendor directory. `vendor_this` in src/ops/cargo_vendor.rs skips a path
    with a `.git` component and a file named `.gitattributes` or `.gitignore`
    at any depth; `unpack` in src/sources/registry/mod.rs skips a file named
    `.cargo-ok`. Every other member is vendored, including `Cargo.toml.orig`.
    Cargo older than 1.94 kept the two Git files below the top level, so a
    member that may be absent is still compared when it is there.
    """
    parts = relative.split("/")
    return (
        ".git" in parts
        or parts[-1] in {".gitattributes", ".gitignore", ".cargo-ok"}
    )


def path_below(member, root):
    """Return a member's path below `root/`, or None if it has no plain one.

    Cargo refuses an archive with a member outside `<name>-<version>/`.
    """
    if not member.startswith(root + "/"):
        return None
    relative = member[len(root) + 1:]
    if any(part in {"", ".", ".."} for part in relative.split("/")):
        return None
    return relative


def record_differences(record, checksum, digests):
    """Compare a crate's checksum record with Cargo.lock and its files.

    `digests` maps every file of the directory to the SHA-256 of its bytes.
    Cargo builds from a record that lists too few files, so the record must
    list exactly these.
    """
    if record.is_symlink() or not record.is_file():
        return [f"{RECORD} is missing or not a regular file"]
    try:
        listed = json.loads(record.read_bytes())
    except ValueError:
        listed = None
    files = listed.get("files") if isinstance(listed, dict) else None
    if not isinstance(files, dict) or not all(
        isinstance(digest, str) for digest in files.values()
    ):
        return [f"{RECORD} is malformed"]
    problems = []
    if listed.get("package") != checksum:
        problems.append(
            f"{RECORD} gives the package checksum {listed.get('package')}, "
            f"Cargo.lock has {checksum}"
        )
    for relative in sorted(digests.keys() - files.keys()):
        problems.append(f"not listed in {RECORD}: {relative}")
    for relative in sorted(files.keys() - digests.keys()):
        problems.append(f"listed in {RECORD}, but not there: {relative}")
    for relative in sorted(files.keys() & digests.keys()):
        if files[relative] != digests[relative]:
            problems.append(f"{RECORD} has another SHA-256 for: {relative}")
    return problems


def archive_members(source, root, problems):
    """Yield (path, member) for each regular member below `root/`, once.

    Appends to `problems` every member that cannot be compared: one outside
    the package root, which Cargo refuses; a link or special member, which
    Cargo refuses unless it skips the member for its name first; and a second
    member of a path.
    """
    seen = set()
    for member in source:
        relative = path_below(member.name, root)
        if member.isdir() and (relative or member.name == root):
            continue
        if relative is None:
            problems.append(
                f"archive member is not below {root}/: {member.name}"
            )
        elif member.type not in {tarfile.REGTYPE, tarfile.AREGTYPE}:
            problems.append(
                f"archive member is not a regular file: {relative}"
            )
        elif relative in seen:
            problems.append(f"the archive has two members for: {relative}")
        else:
            seen.add(relative)
            yield relative, member


def unusable(reason):
    """The result for a crate that has no archive to compare with."""
    return Comparison([reason], 0, 0, [])


def compare_crate(directory, name, version, checksum, archive):
    """Compare one vendored crate with the bytes of its published archive.

    The archive must have the Cargo.lock checksum. Every file of the
    directory must then equal the archive member of its path, every member
    must be there unless `cargo vendor` leaves it out, and the record must
    fit both. Nothing is unpacked to disk.
    """
    digest = sha256(archive)
    if digest != checksum:
        return unusable(
            f"the archive has SHA-256 {digest}, Cargo.lock has {checksum}"
        )
    present, problems = vendored_files(directory)
    digests, seen, left = {}, set(), []
    size = 0
    try:
        with tarfile.open(
            fileobj=io.BytesIO(archive), mode="r:gz",
            encoding="utf-8", errors="surrogateescape"
        ) as source:
            for relative, member in archive_members(
                source, f"{name}-{version}", problems
            ):
                seen.add(relative)
                if relative in present:
                    raw = present[relative].read_bytes()
                    digests[relative] = sha256(raw)
                    size += len(raw)
                    if source.extractfile(member).read() != raw:
                        problems.append(
                            f"differs from the archive: {relative}"
                        )
                elif left_out(relative):
                    left.append(relative)
                else:
                    problems.append(f"missing, but in the archive: {relative}")
    except (tarfile.TarError, gzip.BadGzipFile, EOFError, zlib.error) as error:
        return unusable(f"the archive cannot be read: {error}")
    for relative in sorted(present.keys() - seen):
        problems.append(f"not in the archive: {relative}")
        digests[relative] = sha256(present[relative].read_bytes())
    problems += record_differences(directory / RECORD, checksum, digests)
    return Comparison(problems, len(present.keys() & seen), size, left)


def check(workspace, fetch, jobs=8):
    """Compare the whole vendored tree with the published archives.

    `fetch(name, version, checksum)` returns the bytes of one archive, or
    raises FetchError. The vendored crates must be exactly the crates.io
    packages of Cargo.lock, and each must pass compare_crate(). Once a fetch
    has failed, the run has failed: no further archive is asked for, and the
    crates that are left are counted as skipped.
    """
    workspace = Path(workspace)
    vendor = vendor_directory(workspace)
    if vendor.is_symlink() or not vendor.is_dir():
        raise VendorError(f"not a real vendor directory: {vendor}")
    locked, foreign = locked_crates(workspace)
    crates, stray = vendored_crates(vendor)

    def subject(directory, identity=None):
        path = Path(os.path.relpath(directory, workspace)).as_posix()
        return f"{path} ({' '.join(identity)})" if identity else path

    halted = threading.Event()

    def compare(identity):
        name, version = identity
        if halted.is_set():
            return None
        try:
            archive = fetch(name, version, locked[identity])
        except FetchError as error:
            halted.set()
            return unusable(f"no archive to compare with: {error}")
        return compare_crate(
            crates[identity], name, version, locked[identity], archive
        )

    report = Report()
    for entry, lines in stray.items():
        report.add(subject(vendor / entry), lines)
    shared = sorted(crates.keys() & locked.keys())
    with ThreadPoolExecutor(max_workers=max(1, jobs)) as pool:
        results = list(pool.map(compare, shared))
    for identity, result in zip(shared, results):
        if result is None:
            report.skipped += 1
            continue
        report.add(subject(crates[identity], identity), result.problems)
        report.crates += 1
        report.failed += bool(result.problems)
        report.files  += result.files
        report.size   += result.size
        report.left.update(left_kind(relative) for relative in result.left)
    for identity in sorted(crates.keys() - locked.keys()):
        report.add(
            subject(crates[identity], identity),
            ["not a crates.io package of Cargo.lock"]
        )
    missing = [
        f"{name} {version} is not vendored"
        for name, version in sorted(locked.keys() - crates.keys())
    ]
    report.add("Cargo.lock", foreign + missing)
    return report


def left_kind(relative):
    """Name the rule of left_out() that an archive member falls under."""
    parts = relative.split("/")
    return ".git" if ".git" in parts else parts[-1]


def archive_url(name, version, root=DOWNLOAD_ROOT):
    """The address of a crate file on crates.io; `+` is sent as %2B."""
    filename = urllib.parse.quote(f"{name}-{version}.crate", safe="")
    return f"{root}/{name}/{filename}"


def download(url, attempts=ATTEMPTS, pause=2.0):
    """Fetch one archive, trying again only where another attempt can help.

    A failed download never passes: after the last attempt, or at once for
    an answer that will not change, this raises FetchError.
    """
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    failure = "no attempt was made"
    for attempt in range(1, attempts + 1):
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                status   = response.status
                expected = response.headers.get("Content-Length")
                blocks   = []
                received = 0
                while received <= MAX_ARCHIVE_BYTES:
                    block = response.read(1024 * 1024)
                    if not block:
                        break
                    blocks.append(block)
                    received += len(block)
        except urllib.error.HTTPError as error:
            error.close()
            failure = f"HTTP status {error.code}"
            if error.code < 500 and error.code not in {408, 429}:
                break
        except (OSError, http.client.HTTPException) as error:
            failure = f"{type(error).__name__}: {error}"
        else:
            if status != 200:
                failure = f"HTTP status {status}"
                break
            if received > MAX_ARCHIVE_BYTES:
                failure = f"more than {MAX_ARCHIVE_BYTES} bytes"
                break
            if expected is None or expected == str(received):
                return b"".join(blocks)
            failure = f"incomplete answer, {received} of {expected} bytes"
        if attempt < attempts:
            time.sleep(pause * attempt)
    raise FetchError(f"{url}: {failure}")


def archive_source(archive_dir=None, origins=None, fetch=None):
    """Return the fetch(name, version, checksum) that check() calls.

    A file `<name>-<version>.crate` in `archive_dir` is used when its SHA-256
    is the Cargo.lock checksum, so a local Cargo registry cache can stand in
    for downloads. Every other archive comes from crates.io, through
    `fetch(url)`, which is download() unless given. `origins` receives where
    each archive came from.
    """
    origins = {} if origins is None else origins
    fetch = download if fetch is None else fetch

    def source(name, version, checksum):
        if archive_dir is not None:
            local = Path(archive_dir) / f"{name}-{version}.crate"
            if local.is_file():
                raw = local.read_bytes()
                if sha256(raw) == checksum:
                    origins[(name, version)] = "local"
                    return raw
        origins[(name, version)] = "downloaded"
        return fetch(archive_url(name, version))

    return source


def say(text, stream):
    """Print without failing on a name that the stream cannot encode."""
    encoding = getattr(stream, "encoding", None) or "utf-8"
    raw = text.encode(encoding, "backslashreplace")
    print(raw.decode(encoding), file=stream)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--workspace", type=Path, default=ROOT,
        help="checkout to check (default: the one that holds this script)"
    )
    parser.add_argument(
        "--archive-dir", type=Path,
        help="directory of <name>-<version>.crate files, such as a Cargo "
        "registry cache; a file is used if it has the Cargo.lock checksum, "
        "and every other archive is downloaded"
    )
    parser.add_argument(
        "--jobs", type=int, default=8,
        help="archives to fetch and compare at a time (default: 8)"
    )
    args = parser.parse_args(argv)
    origins = {}
    try:
        report = check(
            args.workspace, archive_source(args.archive_dir, origins),
            args.jobs
        )
    except (VendorError, OSError) as error:
        say(f"cargo vendor check: {error}", sys.stderr)
        return 1
    for subject, lines in report.problems.items():
        say(f"{subject}:", sys.stderr)
        for line in lines[:SHOWN]:
            say(f"    {line}", sys.stderr)
        if len(lines) > SHOWN:
            say(f"    and {len(lines) - SHOWN} more", sys.stderr)
    counts = collections.Counter(origins.values())
    fetched = (
        f"archives downloaded: {counts['downloaded']}, "
        f"read from --archive-dir: {counts['local']}"
    )
    if report.problems or report.skipped:
        summary = [
            "cargo vendor check: FAILED",
            "vendored crates not shown equal to their crates.io archive: "
            f"{report.failed} of {report.crates}",
            f"vendored crates skipped after a failed fetch: {report.skipped}",
            "other entries with problems: "
            f"{len(report.problems) - report.failed}",
            fetched
        ]
        say("\n    ".join(summary), sys.stderr)
        return 1
    kinds = ", ".join(
        f"{count} {kind}" for kind, count in sorted(report.left.items())
    )
    summary = [
        "cargo vendor check: passed",
        f"vendored crates equal to their crates.io archive: {report.crates}",
        f"files compared: {report.files} ({report.size} bytes)",
        "archive members absent because cargo vendor leaves them out: "
        f"{sum(report.left.values())}" + (f" ({kinds})" if kinds else ""),
        fetched
    ]
    say("\n    ".join(summary), sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
