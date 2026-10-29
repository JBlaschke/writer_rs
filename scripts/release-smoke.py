#!/usr/bin/env python3
"""Validate an unpacked native release artifact without external renderers."""

import argparse
import datetime
import json
import os
import platform
import re
import stat
import subprocess
import tarfile
import tempfile
import unicodedata
import zipfile
from pathlib import Path, PurePosixPath

from release_common import (
    MAX_BINARY_BYTES, archive_name, binary_name, checked_target,
    checked_version, digest, root_name,
)


def safe_members(names, root):
    seen = set()
    devices = re.compile(r"(?:CON|PRN|AUX|NUL|COM[1-9¹²³]|LPT[1-9¹²³])", re.I)
    for name in names:
        parts = name.split("/")
        if parts[-1:] == [""]:
            parts.pop()
        if not parts or parts[0] != root or PurePosixPath(name).is_absolute():
            raise ValueError(f"unexpected archive path: {name!r}")
        for part in parts:
            stem = part.split(".", 1)[0].rstrip(" ")
            if (not part or part in {".", ".."} or part.endswith((".", " "))
                    or any(char in '<>:"\\|?*' or ord(char) < 32 for char in part)
                    or devices.fullmatch(stem)):
                raise ValueError(f"nonportable archive path: {name!r}")
        canonical = unicodedata.normalize("NFC", "/".join(parts)).casefold()
        if canonical in seen:
            raise ValueError(f"duplicate portable archive member: {name!r}")
        seen.add(canonical)
    return seen


def unpack(archive, destination, root):
    destination = Path(destination)
    if str(archive).endswith(".zip"):
        with zipfile.ZipFile(archive) as source:
            safe_members(source.namelist(), root)
            entries = source.infolist()
            for entry in entries:
                kind = stat.S_IFMT(entry.external_attr >> 16)
                expected = stat.S_IFDIR if entry.is_dir() else stat.S_IFREG
                if kind == stat.S_IFLNK:
                    raise ValueError("release ZIP contains a symbolic link")
                if kind not in {0, expected}:
                    raise ValueError("release ZIP contains a non-file member")
            for entry in entries:
                mode = entry.external_attr >> 16
                path = destination / entry.filename
                if entry.is_dir():
                    path.mkdir(parents=True, exist_ok=True)
                else:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(source.read(entry))
                    path.chmod(stat.S_IMODE(mode))
    else:
        with tarfile.open(archive, "r:gz") as source:
            safe_members([entry.name for entry in source.getmembers()], root)
            for entry in source.getmembers():
                if not (entry.isdir() or entry.isfile()):
                    raise ValueError("release tar contains a non-file member")
                path = destination / entry.name
                if entry.isdir():
                    path.mkdir(parents=True, exist_ok=True)
                else:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    stream = source.extractfile(entry)
                    if stream is None:
                        raise ValueError(f"unreadable archive member: {entry.name}")
                    path.write_bytes(stream.read())
                    path.chmod(entry.mode)
    return destination / root


def run(binary, arguments, cwd, epoch):
    env = os.environ.copy()
    for key in ["RUST_LOG", "CLICOLOR_FORCE", "WRITER_BLESS"]:
        env.pop(key, None)
    env.update({"SOURCE_DATE_EPOCH": str(epoch), "NO_COLOR": "1"})
    result = subprocess.run(
        [str(binary), *arguments], cwd=cwd, env=env, capture_output=True,
        timeout=180, check=False,
    )
    if result.returncode:
        raise ValueError(f"writer {' '.join(arguments)} failed ({result.returncode}):\n"
                         + result.stderr.decode("utf-8", errors="replace"))
    return result.stdout


def fixed_config(project, epoch):
    timestamp = datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ"
    )
    (project / "writer.toml").write_text(
        "[build]\nout_dir='dist'\n[html]\ntheme='theme'\nstandalone=true\n"
        "[pdf]\nsystem_fonts=false\nallow_packages=false\n"
        f"timestamp='{timestamp}'\n", encoding="utf-8", newline="\n",
    )


def output_inventory(root):
    return {path.relative_to(root).as_posix(): path.read_bytes()
            for path in root.rglob("*") if path.is_file()}


def outputs(binary, temporary, epoch):
    run(binary, ["init", "project", "--bib"], temporary, epoch)
    project = temporary / "project"
    fixed_config(project, epoch)
    target_list = "html,pdf,docx,latex,typst"
    check = run(binary, ["check", "document.md", "--target", target_list,
                         "--format", "json", "--deny-warnings"], project, epoch)
    if any(finding["level"] == "error" for finding in json.loads(check)):
        raise ValueError("scaffold check contains an error")
    run(binary, ["build", "document.md", "--to", target_list], project, epoch)
    result = project / "dist"
    expected = {
        "document.html": b"<!DOCTYPE html", "document.pdf": b"%PDF-",
        "document.docx": b"PK", "document.tex": b"\\documentclass",
        "document.typ": b"#import",
    }
    for name, magic in expected.items():
        data = (result / name).read_bytes()
        if name.endswith((".tex", ".typ")):
            valid = magic in data
        elif name.endswith(".html"):
            valid = data.lstrip().lower().startswith(magic.lower())
        else:
            valid = data.startswith(magic)
        if not valid:
            raise ValueError(f"unexpected output bytes: {name}")
    if not (result / "document_assets").is_dir():
        raise ValueError("source-export sidecar directory was not written")
    first = output_inventory(result)
    run(binary, ["build", "document.md", "--to", target_list], project, epoch)
    if output_inventory(result) != first:
        raise ValueError("identical fixed-input output bytes changed on rebuild")
    run(binary, ["init", "native", "--typst", "--bib"], temporary, epoch)
    native = temporary / "native"
    fixed_config(native, epoch)
    run(binary, ["build", "document.typ"], native, epoch)
    if not (native / "dist/document.pdf").read_bytes().startswith(b"%PDF-"):
        raise ValueError("native Typst PDF smoke did not produce a PDF")


def verify_architecture(binary, target):
    expected_arm = target.startswith("aarch64-")
    with Path(binary).open("rb") as stream:
        header = stream.read(64)
        if "-linux-" in target:
            if len(header) < 20 or header[:6] != b"\x7fELF\x02\x01":
                raise ValueError("release target requires a little-endian ELF64 binary")
            machine = int.from_bytes(header[18:20], "little")
            expected = 183 if expected_arm else 62
        elif target.endswith("-apple-darwin"):
            if len(header) < 8 or header[:4] != b"\xcf\xfa\xed\xfe":
                raise ValueError("release target requires a thin little-endian Mach-O64 binary")
            machine = int.from_bytes(header[4:8], "little")
            expected = 0x0100000C if expected_arm else 0x01000007
        else:
            if len(header) < 64 or header[:2] != b"MZ":
                raise ValueError("release target requires a Windows PE binary")
            offset = int.from_bytes(header[60:64], "little")
            stream.seek(offset)
            pe = stream.read(6)
            if len(pe) != 6 or pe[:4] != b"PE\0\0":
                raise ValueError("release executable has no PE signature")
            machine = int.from_bytes(pe[4:6], "little")
            expected = 0xAA64 if expected_arm else 0x8664
        if machine != expected:
            raise ValueError(f"binary architecture does not match {target}")


def linux_runtime(binary):
    result = {}
    for key, command in [
        ("version_info", ["readelf", "--version-info", str(binary)]),
        ("runtime_libraries", ["ldd", str(binary)]),
    ]:
        process = subprocess.run(
            command, check=True, capture_output=True,
            text=True, encoding="utf-8", timeout=30
        )
        result[key] = process.stdout
    symbols = set(re.findall(r"\bGLIBC_[0-9][0-9.]*\b", result["version_info"]))
    result["required_glibc_symbols"] = sorted(
        symbols, key=lambda symbol: tuple(map(int, symbol[6:].split(".")))
    )
    # Measured symbols/libraries only: this is not a declared compatibility floor.
    return result


def smoke(directory, version, target, build_id, epoch):
    checked_version(version)
    checked_target(target)
    archive = Path(directory) / archive_name(version, target)
    root = root_name(version, target)
    with tempfile.TemporaryDirectory(prefix="writer-release-smoke-") as temporary:
        temporary = Path(temporary)
        expanded = unpack(archive, temporary / "unpacked", root)
        if {path.name for path in expanded.iterdir()} != {
            binary_name(target), "LICENSE", "README.md", "THIRD-PARTY-LICENSES",
        }:
            raise ValueError("archive top-level layout differs from the release contract")
        binary = expanded / binary_name(target)
        if binary.stat().st_size > MAX_BINARY_BYTES:
            raise ValueError("unpacked binary exceeds the 64 MiB ceiling")
        verify_architecture(binary, target)
        notices = expanded / "THIRD-PARTY-LICENSES/EMBEDDED.txt"
        version_output = run(binary, ["--version"], temporary, epoch)
        expected = f"writer {version} ({build_id})\n".encode("utf-8")
        if version_output != expected:
            raise ValueError(f"unexpected artifact version: {version_output!r}")
        if b"Usage:" not in run(binary, ["--help"], temporary, epoch):
            raise ValueError("archived executable has no CLI help")
        if run(binary, ["--licenses"], temporary, epoch) != notices.read_bytes():
            raise ValueError("archived executable notices differ from EMBEDDED.txt")
        outputs(binary, temporary, epoch)
        return {
            "version": version, "target": target, "build_id": build_id,
            "binary_bytes": binary.stat().st_size, "binary_sha256": digest(binary),
            "archive_sha256": digest(archive), "python": platform.python_version(),
            "runner_system": platform.platform(),
            "linux_runtime": linux_runtime(binary) if "-linux-" in target else None,
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--build-id", required=True)
    parser.add_argument("--epoch", type=int, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = smoke(
            args.directory, args.version, args.target, args.build_id, args.epoch
        )
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n",
            encoding="utf-8", newline="\n"
        )
    except (KeyError, OSError, ValueError, subprocess.SubprocessError,
            tarfile.TarError, zipfile.BadZipFile) as error:
        parser.exit(1, f"error: {error}\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
