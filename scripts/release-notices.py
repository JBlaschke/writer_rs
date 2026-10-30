#!/usr/bin/env python3
"""Assemble verified release notices using stdlib Python 3.11 or newer.

The supplied executable is the binary that will enter the archive. Cargo must
produce metadata and the target's normal/no-proc-macro tree beforehand, from
the same locked checkout and build configuration; this helper never runs Cargo.
"""

import argparse
import hashlib
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


HEADER = (
    "writer {version}: third-party notices for the files and data embedded in this binary.\n"
    "Licenses of the linked Rust crates: THIRD-PARTY-LICENSES/ in the release archive, or each crate's source on crates.io.\n\n"
)
CRATES_IO = {
    "registry+https://github.com/rust-lang/crates.io-index",
    "registry+https://index.crates.io/",
    "sparse+https://index.crates.io/",
}
REQUIRED_NOTICES = {
    "licenses/README.md", "licenses/hayagriva-styles.md",
    "licenses/katex-fonts-OFL.txt", "licenses/two-face-0.4.md",
    "licenses/two-face.md", "vendor/VERSIONS.toml",
    "vendor/crates/tylax/LICENSE", "vendor/crates/typst-assets/LICENSE",
    "vendor/crates/typst-assets/NOTICE", "vendor/katex/LICENSE",
    "vendor/latin-modern-math/GUST-FONT-LICENSE.txt",
    "vendor/latin-modern-math/MANIFEST-Latin-Modern-Math.txt",
    "vendor/latin-modern-math/README-Latin-Modern-Math.txt",
    "vendor/morphdom/LICENSE",
    "vendor/typst-packages/preview/mitex/0.2.7/LICENSE",
}
MITEX_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
MITEX_REPOSITORY = "https://github.com/mitex-rs/mitex"
MITEX_REVISION = "f69eb43b52372e141b35661c049ce522567adbf0"
MITEX_NOTICE = "vendor/typst-packages/preview/mitex/0.2.7/LICENSE"
MITEX_NOTICE_SHA256 = "95bd3988beee069fa2848f648dab43cc6e0b2add2ad6bcb17360caf749802bcc"
MITEX_CHECKSUMS = {
    ("mitex", "0.2.4"): "81f1eb466efdd212fe624f8bf24083f21d582f2d08e3b879bc4a059b25969d6c",
    ("mitex-glob", "0.2.4"): "c3074187856f137e3392e906daefa2fdc993175d1c7d55ff1348e3bb9321d415",
    ("mitex-lexer", "0.2.4"): "0fd0a245774eed6cbf0df25bc3a9c6cc64445a5ea2c3a241f1c9691d83ae214b",
    ("mitex-parser", "0.2.4"): "1a42a2f86c46d250802262387dfb1d6bb18affd2d99328bbe4dbd12ce294c3a4",
    ("mitex-spec", "0.2.4"): "12ecaa5f36d60e2d64ea5508d1be44b1c19daa50518f5b8425f36697fdc60509",
    ("mitex-spec-gen", "0.2.4"): "a1e9ef918ec9c2a14e60ebb20275684b483d23afb5c3314127c9eb46ce74ae68",
}
TYPST_CHECKSUMS = {
    ("typst", "0.15.1"): "d1cb79435d128de5d5443a058a983f6dcc16738760fa55ed703762bc278b1fd3",
    ("typst-eval", "0.15.1"): "a8823e1822bd181d8bee47b05632d16bf3a0a8ab042e01d815826350d7ae16c3",
    ("typst-html", "0.15.1"): "f41bcfbe480b640c4447d8bf9bb59020da610f239aa032d53328651a03572c09",
    ("typst-kit", "0.15.1"): "27ee52e27dfab0da5ac3d19e7e43a21fbc7ff83af9301119468bae89fa857a8b",
    ("typst-layout", "0.15.1"): "1916c407baf8f8815e5d50030eed6d5ac6a0702d927b3c156e38db7a618499e2",
    ("typst-library", "0.15.1"): "12becd02464849f1507f3c4ef3f401a4d28d5938cc74b6acc6f1d703458bf393",
    ("typst-pdf", "0.15.1"): "3aef841ff3e606542f3057ce5dd0ff264d53a439e00ed4a91ec00ec8568b3b7f",
    ("typst-realize", "0.15.1"): "a8f2eb73ab25add88cab03b26effabc657977fe850ff81556b28c6b137ff4bf1",
    ("typst-render", "0.15.1"): "de8a657a5fa5b9b7d3d458f482978674de910b54889d46f3bb55ca89f570f83e",
    ("typst-svg", "0.15.1"): "5df6dc785695aae085678bd6657aa04a1d26058a76ed042e738658078bca681e",
    ("typst-syntax", "0.15.1"): "92a20d79df5d10c7fb0b4f7c18e4b2671edfc21e5ed9ad9894e18cccefd1c70f",
    ("typst-timing", "0.15.1"): "ab6ee5110f671a23164c61caa85dbc476043d503103dedd36f16d93f970f3f59",
    ("typst-utils", "0.15.1"): "b33ad1675497b45ae9d960a16e95207c35c2c46330d1a6249a9a977670c8c160",
}
RELEASE_NOTICE_ROOT = Path(__file__).resolve().parent.parent
PINNED_NOTICE_GROUPS = {
    "mitex": {
        "source": MITEX_SOURCE, "repository": MITEX_REPOSITORY,
        "license": "Apache-2.0", "revision": MITEX_REVISION,
        "files": [{
            "name": "LICENSE", "kind": "embedded", "path": MITEX_NOTICE,
            "sha256": MITEX_NOTICE_SHA256,
        }],
    },
    "roman-numerals-rs": {
        "source": MITEX_SOURCE,
        "repository": "https://github.com/AA-Turner/roman-numerals/",
        "license": "0BSD OR CC0-1.0",
        "revision": "ec81337f99d8b557a6fce10f674c054050551444",
        "files": [{
            "name": "LICENCE.rst", "kind": "release",
            "path": "docs/release/notice-sources/roman-numerals-rs-3.1.0/LICENCE.rst",
            "sha256": "c744ffc7e120b22e6620b613298dc003a2613c0ae489ee414fb1d473f24f916f",
        }],
    },
    "typst": {
        "source": MITEX_SOURCE, "repository": "https://github.com/typst/typst",
        "license": "Apache-2.0",
        "revision": "9dfd3a08500b7896045f907433cf7b4b02434fad",
        "files": [
            {
                "name": "LICENSE", "kind": "embedded",
                "path": "vendor/crates/typst-assets/LICENSE",
                "sha256": "62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a",
            },
            {
                "name": "NOTICE", "kind": "release",
                "path": "docs/release/notice-sources/typst-0.15.1/NOTICE",
                "sha256": "1778244777547c281b6f5fa9fc0c18ab21f8d4491c803f64e09046800f5fcb26",
            },
        ],
    },
    "selectors": {
        "source": MITEX_SOURCE, "repository": "https://github.com/servo/stylo",
        "license": "MPL-2.0",
        "revision": "34d25f741765a0bdb894f8271852e633299352d8",
        "source_archive_notice": True,
        "files": [
            {
                "name": "LICENSE-MPL-2.0.txt", "kind": "release",
                "path": "docs/release/notice-sources/selectors-0.41.0/LICENSE-MPL-2.0.txt",
                "sha256": "fab3dd6bdab226f1c08630b1dd917e11fcb4ec5e1e020e2c16f83a0a13863e85",
            },
            {
                "name": "SOURCE-HEADER.txt", "kind": "release",
                "path": "docs/release/notice-sources/selectors-0.41.0/SOURCE-HEADER.txt",
                "sha256": "766674381e88a468d55e07de3596987391cf0e8beebe51baf4fddf11078785b3",
            },
            {
                "name": "PROVENANCE.json", "kind": "release",
                "path": "docs/release/notice-sources/selectors-0.41.0/PROVENANCE.json",
                "sha256": "49d2ecec5ead108ad3745ecaed6adf65f9054231d113d9f1bd04e8c9bb89cb8d",
            },
        ],
    },
}
PINNED_PACKAGES = {
    **{key: {
        "group": "mitex", "checksum": checksum,
        "vcs_path": f"crates/{key[0]}",
    } for key, checksum in MITEX_CHECKSUMS.items()},
    **{key: {
        "group": "typst", "checksum": checksum,
        "vcs_path": f"crates/{key[0]}",
    } for key, checksum in TYPST_CHECKSUMS.items()},
    ("roman-numerals-rs", "3.1.0"): {
        "group": "roman-numerals-rs", "vcs_path": "rust",
        "checksum": "c85cd47a33a4510b1424fe796498e174c6a9cf94e606460ef022a19f3e4ff85e",
    },
    ("selectors", "0.41.0"): {
        "group": "selectors", "vcs_path": "selectors",
        "checksum": "8322aebea94a6cc288ff884e8a49bdbceef60206c400c6c46d22e62fc4fd91a6",
        "source_url": "https://static.crates.io/crates/selectors/selectors-0.41.0.crate",
    },
}
PACKAGE_LINE = re.compile(
    r"([A-Za-z0-9_-]+) v([0-9][A-Za-z0-9.+-]*)(?: \((.*)\))?"
)
TREE_PREFIX = re.compile(
    r"(?:(?:│   |    )*(?:├── |└── )|(?:\|   |    )*(?:\|-- |`-- ))"
)
SPDX_ID = r"[A-Za-z0-9][A-Za-z0-9.-]*"
SPDX_TOKEN = re.compile(
    rf"(?:DocumentRef-{SPDX_ID}:)?LicenseRef-{SPDX_ID}|{SPDX_ID}\+?|[()/]"
)
MIT_TEXT = b"""MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""


class NoticeError(ValueError):
    """A release input is incomplete, ambiguous or inconsistent."""


def excluded(name):
    return (
        name == ".DS_Store" or name.startswith("._")
        or name == "Thumbs.db" or name.endswith((".swp", "~", ".map"))
    )


def files_beneath(root, embedded=False):
    """Enumerate real files deterministically; do not conceal linked trees."""
    if not root.is_dir() or root.is_symlink():
        raise NoticeError(f"not a real source directory: {root}")
    for current, directories, files in os.walk(root, followlinks=False):
        current = Path(current)
        directories[:] = sorted(
            name for name in directories if not embedded or not excluded(name)
        )
        for name in directories:
            if (current / name).is_symlink():
                raise NoticeError(f"symlinked source directory: {current / name}")
        for name in sorted(files):
            if embedded and excluded(name):
                continue
            path = current / name
            if path.is_symlink():
                raise NoticeError(f"symlinked source file: {path}")
            if not path.is_file():
                raise NoticeError(f"non-regular source file: {path}")
            yield path


def embedded_notices(assets):
    """Mirror vendor::license_paths, including rust-embed's junk exclusions."""
    notices = {}
    for directory in ("licenses", "vendor"):
        for path in files_beneath(assets / directory, embedded=True):
            relative = path.relative_to(assets).as_posix()
            name = path.name.lower()
            selected = (
                directory == "licenses" or relative == "vendor/VERSIONS.toml"
                or any(part in name for part in ("licen", "notice", "copying"))
                or name.startswith(("manifest-", "readme-"))
            )
            if selected:
                notices[relative] = path.read_bytes()
    missing = REQUIRED_NOTICES - notices.keys()
    if missing:
        raise NoticeError(
            "source notice inventory omits required paths: "
            + ", ".join(sorted(missing))
        )
    return dict(sorted(notices.items()))


def embedded_bytes(version, notices):
    text = HEADER.format(version=version)
    for path, raw in sorted(notices.items()):
        notice = raw.decode("utf-8", errors="replace")
        text += f"==> {path} <==\n" + notice
        if not notice.endswith("\n"):
            text += "\n"
        text += "\n"
    return text.encode("utf-8")


def verify_embedded(actual, version, notices):
    expected = embedded_bytes(version, notices)
    if actual != expected:
        first = next((index for index, (left, right) in enumerate(
            zip(actual, expected)
        ) if left != right), min(len(actual), len(expected)))
        raise NoticeError(
            "archived binary --licenses differs from the source notice set "
            f"at byte {first} (expected {len(expected)} bytes, got {len(actual)})"
        )


def checked_metadata(data):
    if not isinstance(data, dict) or not isinstance(data.get("packages"), list):
        raise NoticeError("cargo metadata must contain a packages array")
    if not isinstance(data.get("workspace_root"), str):
        raise NoticeError("cargo metadata has no workspace_root")
    by_identity = {}
    by_id = {}
    for package in data["packages"]:
        if not isinstance(package, dict) or not all(
            isinstance(package.get(field), str)
            for field in ("id", "name", "version", "manifest_path")
        ):
            raise NoticeError("invalid package identity in cargo metadata")
        if package["id"] in by_id:
            raise NoticeError(f"duplicate metadata package id: {package['id']}")
        if not re.fullmatch(r"[A-Za-z0-9_-]+", package["name"]):
            raise NoticeError("invalid metadata package name")
        if not re.fullmatch(r"[0-9][A-Za-z0-9.+-]*", package["version"]):
            raise NoticeError("invalid metadata package version")
        for field in ("source", "license", "license_file"):
            if package.get(field) is not None and not isinstance(
                package[field], str
            ):
                raise NoticeError(f"invalid metadata {field}: {package['name']}")
        by_id[package["id"]] = package
        key = (package["name"], package["version"])
        by_identity.setdefault(key, []).append(package)
    members = data.get("workspace_members")
    if not isinstance(members, list):
        raise NoticeError("cargo metadata has no workspace_members array")
    roots = [package for package in by_id.values() if (
        package["name"] == "writer" and package["id"] in members
    )]
    if len(roots) != 1:
        raise NoticeError("metadata must identify exactly one workspace writer")
    return roots[0], by_identity


def linked_packages(data, tree):
    root, identities = checked_metadata(data)
    linked = {}
    for line in tree.splitlines():
        if not line.strip():
            continue
        package_line = line[:-4] if line.endswith(" (*)") else line
        prefix = TREE_PREFIX.match(package_line)
        if prefix:
            package_line = package_line[prefix.end():]
        match = PACKAGE_LINE.fullmatch(package_line)
        if not match:
            raise NoticeError(f"unexpected cargo tree decoration: {line!r}")
        name, version, origin = match.groups()
        candidates = identities.get((name, version), [])
        if len(candidates) != 1:
            raise NoticeError(
                f"tree package {name} {version} has {len(candidates)} "
                "metadata matches; exactly one is required"
            )
        package = candidates[0]
        if package.get("source") is None:
            expected = Path(package["manifest_path"]).parent.resolve()
            if origin is None or Path(origin).resolve() != expected:
                raise NoticeError(f"tree path does not match metadata: {name}")
        elif origin is not None:
            raise NoticeError(f"unexpected registry origin in cargo tree: {line!r}")
        linked[package["id"]] = package
    if root["id"] not in linked:
        raise NoticeError("target's linked tree omits the workspace writer")
    return root, sorted(
        linked.values(), key=lambda package: (package["name"], package["version"])
    )


def permits_mit_only(expression):
    """Evaluate a Cargo license expression with only plain MIT available.

    Cargo also accepts legacy '/' as OR. Other licenses and WITH-qualified
    licenses require their own notices; an AND branch cannot drop them.
    """
    if not isinstance(expression, str) or not expression.strip():
        return False
    matches = list(SPDX_TOKEN.finditer(expression))
    tokens = [match.group() for match in matches]
    if "".join(tokens) != "".join(expression.split()):
        return False
    for match in matches:
        token = match.group()
        if token not in {"AND", "OR", "WITH"}:
            continue
        before = expression[match.start() - 1:match.start()]
        after = expression[match.end():match.end() + 1]
        if token == "WITH":
            if not before.isspace() or not after.isspace():
                return False
        elif not all(
            char.isspace() or char in {"(", ")"} for char in (before, after)
        ):
            return False
    position = 0

    def primary():
        nonlocal position
        if position >= len(tokens):
            return None
        token = tokens[position]
        position += 1
        if token == "(":
            value = either()
            if value is None or position >= len(tokens):
                return None
            if tokens[position] != ")":
                return None
            position += 1
            return value
        if token in {"AND", "OR", "WITH", ")", "/"}:
            return None
        value = token.lower() == "mit"
        if position < len(tokens) and tokens[position] == "WITH":
            position += 1
            if position >= len(tokens) or tokens[position] in {
                "AND", "OR", "WITH", "(", ")", "/",
            }:
                return None
            if not re.fullmatch(SPDX_ID, tokens[position]):
                return None
            position += 1
            value = False
        return value

    def both():
        nonlocal position
        value = primary()
        if value is None:
            return None
        while position < len(tokens) and tokens[position] == "AND":
            position += 1
            right = primary()
            if right is None:
                return None
            value = value and right
        return value

    def either():
        nonlocal position
        value = both()
        if value is None:
            return None
        while position < len(tokens) and tokens[position] in {"OR", "/"}:
            position += 1
            right = both()
            if right is None:
                return None
            value = value or right
        return value

    return either() is True and position == len(tokens)


def pinned_crate_notices(package, embedded, workspace):
    """Reuse exact reviewed notices only after immutable crate source guards."""
    name, version = package["name"], package["version"]
    pin = PINNED_PACKAGES.get((name, version))
    if pin is None:
        return None
    group = PINNED_NOTICE_GROUPS[pin["group"]]
    expected = pin["checksum"]

    def require(condition, detail):
        if not condition:
            raise NoticeError(
                f"pinned {pin['group']} notice guard: {name} {version}: {detail}"
            )

    require(package.get("source") == group["source"], "registry source changed")
    require(package.get("repository") == group["repository"], "repository changed")
    require(package.get("license") == group["license"], "license declaration changed")
    require(
        package.get("license_file") is None, "license_file declaration changed"
    )
    locked = tomllib.loads(
        (Path(workspace) / "Cargo.lock").read_text(encoding="utf-8")
    )
    matches = [item for item in locked.get("package", []) if (
        item.get("name") == name and item.get("version") == version
        and item.get("source") == group["source"]
    )]
    require(len(matches) == 1, "locked package identity missing or ambiguous")
    require(matches[0].get("checksum") == expected, "locked checksum changed")
    root = Path(package["manifest_path"]).parent.resolve(strict=True)
    require(
        root.name == f"{name}-{version}" and root.parent.parent.name == "src",
        "unsupported registry source/cache layout"
    )
    archive = (
        root.parent.parent.parent / "cache" / root.parent.name
        / f"{name}-{version}.crate"
    )
    require(
        archive.is_file() and not archive.is_symlink(),
        "verified cached crate archive is missing or linked"
    )
    digest = hashlib.sha256()
    with archive.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    require(digest.hexdigest() == expected, "cached crate checksum changed")
    member_name = f"{name}-{version}/.cargo_vcs_info.json"
    with tarfile.open(archive, "r:gz") as source:
        members = [
            member for member in source.getmembers() if member.name == member_name
        ]
        require(
            len(members) == 1 and members[0].isfile(),
            "crate VCS record is missing, duplicate or non-regular"
        )
        stream = source.extractfile(members[0])
        require(stream is not None, "crate VCS record is unreadable")
        vcs_bytes = stream.read()
    vcs_file = root / ".cargo_vcs_info.json"
    require(
        vcs_file.is_file() and not vcs_file.is_symlink(),
        "source VCS record is missing or linked"
    )
    require(
        vcs_file.read_bytes() == vcs_bytes, "source VCS record differs from archive"
    )
    vcs = json.loads(vcs_bytes)
    require(
        isinstance(vcs, dict) and isinstance(vcs.get("git"), dict),
        "malformed VCS record"
    )
    require(vcs["git"].get("sha1") == group["revision"], "VCS revision changed")
    require(vcs["git"].get("dirty", False) is False, "VCS record is dirty")
    require(vcs.get("path_in_vcs") == pin["vcs_path"], "VCS package path changed")
    notices = {}
    for source in group["files"]:
        output = source["name"]
        require(
            output not in {"", ".", ".."} and "/" not in output and "\\" not in output,
            "invalid archive notice name"
        )
        if source["kind"] == "embedded":
            raw = embedded.get(source["path"])
            require(isinstance(raw, bytes), "existing embedded notice is missing")
        elif source["kind"] == "release":
            relative = Path(source["path"])
            require(
                not relative.is_absolute() and ".." not in relative.parts,
                "release notice path is not confined"
            )
            path = RELEASE_NOTICE_ROOT / relative
            require(
                path.is_file() and not path.is_symlink(),
                "release notice source is missing or linked"
            )
            resolved = path.resolve(strict=True)
            try:
                resolved.relative_to(RELEASE_NOTICE_ROOT.resolve(strict=True))
            except ValueError:
                require(False, "release notice source escapes helper checkout")
            raw = resolved.read_bytes()
        else:
            require(False, "unreviewed notice source kind")
        require(
            hashlib.sha256(raw).hexdigest() == source["sha256"],
            f"{source['kind']} notice checksum changed"
        )
        require(output not in notices, "duplicate archive notice name")
        notices[output] = raw
    require(bool(notices), "reviewed notice group is empty")
    return notices


def crate_notices(package, embedded, workspace):
    root = Path(package["manifest_path"]).parent.resolve(strict=True)
    notices = {}
    # Workspace/path sources may contain target/, tests and separately copied
    # embedded assets. Their own notices are root files or license_file;
    # registry packages are the published source trees and are searched fully.
    paths = files_beneath(root) if package.get("source") else root.iterdir()
    for path in paths:
        if path.name.upper().startswith((
            "LICENSE", "COPYING", "NOTICE", "UNLICENSE",
        )):
            if path.is_symlink():
                raise NoticeError(f"symlinked source notice: {path}")
            if path.is_dir():
                continue
            if not path.is_file():
                raise NoticeError(f"non-regular source notice: {path}")
            notices[path.relative_to(root).as_posix()] = path.read_bytes()
    explicit = package.get("license_file")
    if explicit:
        path = Path(explicit)
        if not path.is_absolute():
            path = root / path
        resolved = path.resolve(strict=True)
        try:
            relative = resolved.relative_to(root)
        except ValueError as error:
            raise NoticeError(
                f"license_file escapes package: {package['name']}"
            ) from error
        if not resolved.is_file():
            raise NoticeError(f"license_file is not a file: {package['name']}")
        notices[relative.as_posix()] = resolved.read_bytes()
    if not notices:
        pinned = pinned_crate_notices(package, embedded, workspace)
        if pinned is not None:
            notices.update(pinned)
        elif permits_mit_only(package.get("license")):
            notices["LICENSE-MIT"] = MIT_TEXT
        else:
            raise NoticeError(
                f"linked crate {package['name']} {package['version']} has no "
                "notice files and no plain MIT-only license choice"
            )
    return dict(sorted(notices.items()))


def package_location(package, workspace):
    source = package.get("source")
    if source in CRATES_IO:
        pin = PINNED_PACKAGES.get((package["name"], package["version"]))
        if pin is not None:
            group = PINNED_NOTICE_GROUPS[pin["group"]]
            if group.get("source_archive_notice"):
                expected = (
                    f"https://static.crates.io/crates/{package['name']}/"
                    f"{package['name']}-{package['version']}.crate"
                )
                if source != group["source"] or pin.get("source_url") != expected:
                    raise NoticeError("pinned complete source archive attribution is missing or changed")
                return pin["source_url"]
        return (
            f"https://crates.io/crates/{package['name']}/{package['version']}"
        )
    if source is not None:
        raise NoticeError(f"unsupported package source: {source}")
    root = Path(package["manifest_path"]).parent.resolve(strict=True)
    relative = os.path.relpath(root, Path(workspace).resolve(strict=True))
    return Path(relative).as_posix()


def notice_payload(data, tree, assets, binary_stdout):
    root, packages = linked_packages(data, tree)
    notices = embedded_notices(assets)
    verify_embedded(binary_stdout, root["version"], notices)
    payload = {"EMBEDDED.txt": binary_stdout}
    payload.update({f"assets/{path}": raw for path, raw in notices.items()})
    lines = []
    for package in packages:
        name, version = package["name"], package["version"]
        license = package.get("license") or "UNSPECIFIED"
        if any(character in license for character in "\r\n"):
            raise NoticeError(f"multiline license expression: {name}")
        location = package_location(package, data["workspace_root"])
        lines.append(f"{name} {version}: {license} ({location})\n")
        for path, raw in crate_notices(
            package, notices, data["workspace_root"]
        ).items():
            payload[f"crates/{name}-{version}/{path}"] = raw
    payload["crates.txt"] = "".join(lines).encode("utf-8")
    return payload


def write_fresh(out, payload):
    """Prepare fully, then claim a fresh output without mixing stale files."""
    if os.path.lexists(out):
        raise NoticeError(f"output already exists: {out}")
    out.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=".writer-notices-", dir=out.parent))
    claimed = False
    try:
        for relative, raw in sorted(payload.items()):
            path = temporary / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        # mkdir is exclusive even on Unix, where rename may replace an empty
        # destination directory. Only this invocation's new directory is moved
        # into or removed after a failure.
        out.mkdir()
        claimed = True
        for path in temporary.iterdir():
            path.rename(out / path.name)
    except BaseException:
        if claimed:
            shutil.rmtree(out)
        raise
    finally:
        shutil.rmtree(temporary)


def assemble(binary, target, metadata_json, tree_text, assets, out):
    if not re.fullmatch(r"[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)+", target):
        raise NoticeError("invalid Rust target triple")
    if os.path.lexists(out):
        raise NoticeError(f"output already exists: {out}")
    data = json.loads(metadata_json.read_text(encoding="utf-8"))
    tree = tree_text.read_text(encoding="utf-8")
    result = subprocess.run(
        [str(binary.resolve(strict=True)), "--licenses"],
        capture_output=True, timeout=60, check=False,
    )
    if result.returncode != 0 or result.stderr:
        raise NoticeError(
            f"archived binary --licenses failed (status {result.returncode}): "
            + result.stderr.decode("utf-8", errors="replace")
        )
    write_fresh(out, notice_payload(data, tree, assets, result.stdout))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "metadata-json", "tree-text", "assets", "out"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--target", required=True)
    args = parser.parse_args(argv)
    try:
        assemble(
            args.binary, args.target, args.metadata_json, args.tree_text,
            args.assets, args.out,
        )
    except (NoticeError, OSError, ValueError, subprocess.SubprocessError,
            tarfile.TarError) as error:
        print(f"release notices: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
