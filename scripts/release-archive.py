#!/usr/bin/env python3
"""Deterministic release archives and checksums, using only Python's stdlib."""

import argparse
import gzip
import io
import os
import stat
import tarfile
import tempfile
import time
import zipfile
from pathlib import Path

from release_common import (
    MAX_BINARY_BYTES, TARGETS, archive_name, binary_name, checked_target,
    checked_version, digest, root_name,
)


def payload(binary, readme, license_file, notices, target):
    binary = Path(binary)
    notices = Path(notices)
    if not binary.is_file():
        raise ValueError(f"binary is not a regular file: {binary}")
    size = binary.stat().st_size
    if size > MAX_BINARY_BYTES:
        raise ValueError(f"binary exceeds 64 MiB ceiling: {size} bytes")
    if size == 0:
        raise ValueError("release binary is empty")
    entries = {
        binary_name(target): (binary.read_bytes(), 0o755),
        "README.md": (Path(readme).read_bytes(), 0o644),
        "LICENSE": (Path(license_file).read_bytes(), 0o644),
    }
    required = ["EMBEDDED.txt", "assets", "crates.txt", "crates"]
    if not notices.is_dir() or notices.is_symlink():
        raise ValueError(f"notice directory is missing or linked: {notices}")
    for name in required:
        path = notices / name
        correct_type = path.is_dir() if name in {"assets", "crates"} else path.is_file()
        if not correct_type or path.is_symlink():
            raise ValueError(f"missing or invalid required notice member: {name}")
    for path in sorted(notices.rglob("*"), key=lambda path: path.as_posix()):
        if path.is_symlink():
            raise ValueError(f"notice member is a symbolic link: {path}")
        if path.is_dir():
            continue
        if not path.is_file():
            raise ValueError(f"notice member is not a regular file: {path}")
        relative = path.relative_to(notices).as_posix()
        entries[f"THIRD-PARTY-LICENSES/{relative}"] = (path.read_bytes(), 0o644)
    return entries


def members(entries, root):
    directories = {
        root, f"{root}/THIRD-PARTY-LICENSES",
        f"{root}/THIRD-PARTY-LICENSES/assets",
        f"{root}/THIRD-PARTY-LICENSES/crates",
    }
    for name in entries:
        parent = Path(name).parent
        while parent != Path("."):
            directories.add(f"{root}/{parent.as_posix()}")
            parent = parent.parent
    result = [(f"{directory}/", b"", 0o755) for directory in directories]
    result.extend(
        (f"{root}/{name}", data, mode)
        for name, (data, mode) in entries.items()
    )
    return sorted(result, key=lambda entry: entry[0].encode("utf-8"))


def write_tar(path, entries, epoch):
    with Path(path).open("wb") as raw:
        with gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
                for name, data, mode in entries:
                    entry = tarfile.TarInfo(name)
                    entry.uid = 0
                    entry.gid = 0
                    entry.uname = ""
                    entry.gname = ""
                    entry.mtime = epoch
                    entry.mode = mode
                    entry.type = tarfile.DIRTYPE if name.endswith("/") else tarfile.REGTYPE
                    entry.size = len(data)
                    archive.addfile(entry, io.BytesIO(data))


def write_zip(path, entries, epoch):
    date = time.gmtime(max(epoch, 315_532_800))[:6]
    if date[0] > 2107:
        raise ValueError("ZIP timestamp exceeds year 2107")
    date = (*date[:5], date[5] // 2 * 2)
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data, mode in entries:
            entry = zipfile.ZipInfo(name, date_time=date)
            entry.create_system = 3
            kind = stat.S_IFDIR if name.endswith("/") else stat.S_IFREG
            entry.external_attr = (kind | mode) << 16
            if name.endswith("/"):
                entry.external_attr |= 0x10
                entry.compress_type = zipfile.ZIP_STORED
            else:
                entry.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(entry, data, compresslevel=9)


def pack(binary, readme, license_file, notices, version, target, epoch, out):
    checked_version(version)
    checked_target(target)
    if not isinstance(epoch, int) or not 0 <= epoch <= 0xFFFFFFFF:
        raise ValueError("archive epoch must fit the unsigned gzip timestamp")
    entries = members(
        payload(binary, readme, license_file, notices, target),
        root_name(version, target)
    )
    out = Path(out)
    result = out / archive_name(version, target)
    if result.exists() or result.is_symlink():
        raise ValueError(f"archive already exists: {result}")
    out.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=".writer-archive-", dir=out)
    os.close(descriptor)
    try:
        if target.endswith("-msvc"):
            write_zip(temporary, entries, epoch)
        else:
            write_tar(temporary, entries, epoch)
        os.chmod(temporary, 0o644)
        # Same-directory hard-link publication is exclusive: a concurrent
        # destination is never replaced after the initial preflight.
        os.link(temporary, result)
    finally:
        Path(temporary).unlink(missing_ok=True)
    return result


def checksums(directory, version, verify=False):
    directory = Path(directory)
    expected = {archive_name(version, target) for target in TARGETS}
    archives = {path.name for path in directory.iterdir()
                if path.name.endswith((".tar.gz", ".zip"))}
    if archives != expected:
        raise ValueError(f"expected six target archives; missing={sorted(expected - archives)}, "
                         f"unexpected={sorted(archives - expected)}")
    content = "".join(
        f"{digest(directory / name)}  {name}\n" for name in sorted(expected)
    ).encode("ascii")
    destination = directory / "SHA256SUMS"
    if verify:
        if destination.read_bytes() != content:
            raise ValueError("SHA256SUMS does not match the six archive bytes")
    else:
        destination.write_bytes(content)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    archive = commands.add_parser("pack")
    for name in ["binary", "readme", "license", "notices", "out"]:
        archive.add_argument(f"--{name}", type=Path, required=True)
    archive.add_argument("--version", required=True)
    archive.add_argument("--target", required=True)
    archive.add_argument("--epoch", type=int, required=True)
    sums = commands.add_parser("checksums")
    sums.add_argument("--directory", type=Path, required=True)
    sums.add_argument("--version", required=True)
    sums.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "pack":
            result = pack(
                args.binary, args.readme, args.license, args.notices,
                args.version, args.target, args.epoch, args.out
            )
        else:
            result = checksums(args.directory, args.version, args.verify)
    except (OSError, ValueError, tarfile.TarError, zipfile.BadZipFile) as error:
        parser.exit(1, f"error: {error}\n")
    print(result)


if __name__ == "__main__":
    main()
