#!/usr/bin/env python3
"""Pure tests of the Cargo vendor check.

Archives are built in memory and vendored trees in a temporary directory.
The download tests talk to a server on the loopback interface. Nothing here
reaches crates.io or reads the checkout's own vendor directory.
"""

import contextlib
import gzip
import hashlib
import http.server
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import socket
import sys
import tarfile
import tempfile
import threading
import unittest
from unittest import mock


sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "check_cargo_vendor", Path(__file__).with_name("check-cargo-vendor.py")
)
VENDOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VENDOR)

RECORD = ".cargo-checksum.json"
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
CONFIG = b"""\
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
"""
ALPHA = "vendor/alpha (alpha 1.0.0)"
BETA = "vendor/beta (beta 2.0.0)"


def sha256(raw):
    return hashlib.sha256(raw).hexdigest()


def write(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw)


def manifest(name, version):
    return f'[package]\nname = "{name}"\nversion = "{version}"\n'.encode()


def sources(name, version):
    """The members of a small published crate, by path below its root."""
    return {
        ".cargo_vcs_info.json": b'{"git":{"sha1":"0"},"path_in_vcs":""}',
        "Cargo.toml":           manifest(name, version),
        "Cargo.toml.orig":      b"[package]\n",
        "README.md":            b"Read me\r\n",
        "src/lib.rs":           b"pub fn answer() -> u32 { 42 }\n"
    }


def member(name, raw=b"", kind=tarfile.REGTYPE, target=""):
    """One archive member: its header and its bytes."""
    info = tarfile.TarInfo(name)
    info.type     = kind
    info.size     = len(raw)
    info.linkname = target
    return info, raw


def crate_archive(name, version, files, others=()):
    """Build the bytes of a .crate file: a gzip tar below `<name>-<version>/`.

    `others` are members added after the files, exactly as they are given.
    """
    members = [
        member(f"{name}-{version}/{path}", raw) for path, raw in files.items()
    ]
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        for info, raw in members + list(others):
            archive.addfile(info, io.BytesIO(raw))
    return buffer.getvalue()


def digests(directory):
    """The SHA-256 of every file of a crate directory except its record."""
    return {
        path.relative_to(directory).as_posix(): sha256(path.read_bytes())
        for path in sorted(directory.rglob("*"))
        if path.is_file() and path != directory / RECORD
    }


def edit_record(directory, **changes):
    listed = json.loads((directory / RECORD).read_bytes())
    listed.update(changes)
    write(directory / RECORD, json.dumps(listed).encode())


def relist(directory):
    """Make the record fit the files again, which is all that Cargo checks."""
    edit_record(directory, files=digests(directory))


def lock_text(packages):
    """A Cargo.lock that holds the given fields of each package."""
    blocks = ["version = 4\n"]
    for package in packages:
        fields = [
            f'{key} = "{value}"' for key, value in package.items()
            if value is not None
        ]
        blocks.append("\n".join(["[[package]]", *fields]) + "\n")
    return "\n".join(blocks).encode()


class VendorCase(unittest.TestCase):
    """A workspace that vendors alpha 1.0.0 exactly as it was published."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.workspace = Path(temporary.name) / "workspace"
        self.vendor    = self.workspace / "vendor"
        self.archives  = {}
        self.packages  = {}
        self.asked     = []
        write(self.workspace / ".cargo" / "config.toml", CONFIG)
        self.vendor.mkdir()
        self.alpha = self.add("alpha", "1.0.0")

    def publish(self, name, version, files=None, others=()):
        """Give the registry an archive of the crate; return its files."""
        files = sources(name, version) if files is None else files
        self.archives[(name, version)] = crate_archive(
            name, version, files, others
        )
        return files

    def lock(self, name, version, source=CRATES_IO, checksum=None):
        """Put a package into Cargo.lock; None leaves a field out."""
        self.packages[(name, version)] = {
            "name": name, "version": version,
            "source": source, "checksum": checksum
        }
        own = {"name": "workspace-root", "version": "0.1.0"}
        write(
            self.workspace / "Cargo.lock",
            lock_text([own, *self.packages.values()])
        )

    def place(self, name, version, files, directory=None, omit=()):
        """Vendor a published crate without the files named in `omit`."""
        target = self.vendor / (directory or name)
        for path, raw in files.items():
            if path not in omit:
                write(target / path, raw)
        checksum = sha256(self.archives[(name, version)])
        write(target / RECORD, json.dumps({"package": checksum}).encode())
        relist(target)
        return target

    def add(self, name, version, files=None, others=(), **placing):
        """Publish, lock and vendor a crate; return its directory."""
        files = self.publish(name, version, files, others)
        self.lock(
            name, version, checksum=sha256(self.archives[(name, version)])
        )
        return self.place(name, version, files, **placing)

    def fetch(self, name, version, checksum):
        self.asked.append((name, version, checksum))
        if (name, version) not in self.archives:
            raise VENDOR.FetchError(f"{name} {version}: HTTP status 404")
        return self.archives[(name, version)]

    def check(self):
        return VENDOR.check(self.workspace, self.fetch, jobs=2)

    def assert_problems(self, expected):
        self.assertEqual(self.check().problems, expected)


class FileTests(VendorCase):
    def test_identical_tree_passes(self):
        report = self.check()
        files  = sources("alpha", "1.0.0")
        self.assertEqual(report.problems, {})
        self.assertEqual((report.crates, report.failed), (1, 0))
        self.assertEqual(report.files, len(files))
        self.assertEqual(report.size, sum(map(len, files.values())))
        self.assertEqual(dict(report.left), {})
        self.assertEqual(
            self.asked,
            [("alpha", "1.0.0", sha256(self.archives[("alpha", "1.0.0")]))]
        )

    def test_file_edited_together_with_its_record_fails(self):
        """Cargo builds this tree: the record fits the edited file."""
        write(self.alpha / "src/lib.rs", b"pub fn answer() -> u32 { 41 }\n")
        relist(self.alpha)
        self.assert_problems({
            ALPHA: ["differs from the archive: src/lib.rs"]
        })

    def test_file_edited_without_its_record_fails(self):
        write(self.alpha / "README.md", b"Read me\n")
        self.assert_problems({
            ALPHA: [
                "differs from the archive: README.md",
                f"{RECORD} has another SHA-256 for: README.md"
            ]
        })

    def test_extra_file_that_the_record_does_not_list_fails(self):
        """Cargo builds this tree: it checks only the files a record lists."""
        write(self.alpha / "src/extra.rs", b"pub fn more() {}\n")
        self.assert_problems({
            ALPHA: [
                "not in the archive: src/extra.rs",
                f"not listed in {RECORD}: src/extra.rs"
            ]
        })

    def test_extra_file_that_the_record_lists_fails(self):
        write(self.alpha / "build.rs", b"fn main() {}\n")
        relist(self.alpha)
        report = self.check()
        self.assertEqual(
            report.problems, {ALPHA: ["not in the archive: build.rs"]}
        )
        self.assertEqual(report.files, len(sources("alpha", "1.0.0")))

    def test_missing_file_fails(self):
        (self.alpha / "src/lib.rs").unlink()
        self.assert_problems({
            ALPHA: [
                "missing, but in the archive: src/lib.rs",
                f"listed in {RECORD}, but not there: src/lib.rs"
            ]
        })

    def test_file_removed_together_with_its_record_entry_fails(self):
        (self.alpha / "Cargo.toml.orig").unlink()
        relist(self.alpha)
        self.assert_problems({
            ALPHA: ["missing, but in the archive: Cargo.toml.orig"]
        })

    def test_link_in_a_crate_directory_fails(self):
        """A link could show the build other bytes than it shows the check."""
        readme = self.alpha / "README.md"
        write(self.workspace / "elsewhere.md", readme.read_bytes())
        readme.unlink()
        os.symlink(self.workspace / "elsewhere.md", readme)
        os.symlink("src", self.alpha / "more")
        self.assert_problems({
            ALPHA: [
                "not a regular file: more",
                "not a regular file: README.md",
                "missing, but in the archive: README.md",
                f"listed in {RECORD}, but not there: README.md"
            ]
        })


class LeftOutTests(VendorCase):
    def test_gitattributes_omission_is_allowed(self):
        files = sources("beta", "2.0.0") | {".gitattributes": b"* text\n"}
        self.add("beta", "2.0.0", files, omit={".gitattributes"})
        report = self.check()
        self.assertEqual(report.problems, {})
        self.assertEqual(dict(report.left), {".gitattributes": 1})

    def test_every_member_that_cargo_vendor_leaves_out_may_be_absent(self):
        left = {
            ".gitignore":            b"target\n",
            "tests/data/.gitignore": b"*.tmp\n",
            "tests/.gitattributes":  b"*.bin binary\n",
            ".git/config":           b"[core]\n",
            "sub/.git/HEAD":         b"ref: refs/heads/main\n",
            ".cargo-ok":             b"",
            "sub/.cargo-ok":         b"ok"
        }
        self.add("beta", "2.0.0", sources("beta", "2.0.0") | left, omit=left)
        report = self.check()
        self.assertEqual(report.problems, {})
        self.assertEqual(report.files, 2 * len(sources("beta", "2.0.0")))
        self.assertEqual(
            dict(report.left),
            {".gitignore": 2, ".gitattributes": 1, ".git": 2, ".cargo-ok": 2}
        )

    def test_no_other_member_may_be_absent(self):
        kept = {
            ".cargo_vcs_info.json", "Cargo.toml.orig", ".gitmodules",
            ".gitkeep", ".github/workflows/ci.yml", ".cargo/config.toml",
            "src/lib.rs.orig", "src/fix.rej", "sub/gitignore",
            "sub/.gitignore.bak", "git/config"
        }
        files = sources("beta", "2.0.0")
        files.update({path: b"kept\n" for path in sorted(kept)})
        self.add("beta", "2.0.0", files, omit=kept)
        self.assert_problems({
            BETA: [
                f"missing, but in the archive: {path}"
                for path in files if path in kept
            ]
        })

    def test_left_out_member_that_is_present_must_equal_the_archive(self):
        """Cargo older than 1.94 vendored the Git files of subdirectories."""
        files = sources("beta", "2.0.0") | {"tests/.gitignore": b"*.tmp\n"}
        beta = self.add("beta", "2.0.0", files)
        report = self.check()
        self.assertEqual(report.problems, {})
        self.assertEqual(dict(report.left), {})
        write(beta / "tests/.gitignore", b"*\n")
        relist(beta)
        self.assert_problems({
            BETA: ["differs from the archive: tests/.gitignore"]
        })

    def test_git_file_that_the_archive_lacks_is_an_extra_file(self):
        write(self.alpha / ".gitignore", b"target\n")
        relist(self.alpha)
        self.assert_problems({ALPHA: ["not in the archive: .gitignore"]})


class RecordTests(VendorCase):
    def test_record_package_checksum_must_be_the_lock_checksum(self):
        other = "0" * 64
        edit_record(self.alpha, package=other)
        checksum = sha256(self.archives[("alpha", "1.0.0")])
        self.assert_problems({
            ALPHA: [
                f"{RECORD} gives the package checksum {other}, "
                f"Cargo.lock has {checksum}"
            ]
        })

    def test_record_with_an_emptied_file_table_fails(self):
        """Cargo builds from this record; it is how vendored crates get
        patched in place."""
        edit_record(self.alpha, files={})
        self.assert_problems({
            ALPHA: [
                f"not listed in {RECORD}: {path}"
                for path in sorted(sources("alpha", "1.0.0"))
            ]
        })

    def test_missing_record_fails(self):
        (self.alpha / RECORD).unlink()
        self.assert_problems({
            ALPHA: [f"{RECORD} is missing or not a regular file"]
        })

    def test_malformed_record_fails(self):
        malformed = [
            b"", b"\xff", b"[]", b'{"package": null}', b'{"files": []}',
            b'{"files": {"src/lib.rs": 1}}'
        ]
        for raw in malformed:
            with self.subTest(raw=raw):
                write(self.alpha / RECORD, raw)
                self.assert_problems({ALPHA: [f"{RECORD} is malformed"]})


class ArchiveTests(VendorCase):
    def test_wrong_archive_checksum_fails(self):
        """The registry answers with other bytes than Cargo.lock names."""
        locked = sha256(self.archives[("alpha", "1.0.0")])
        files  = sources("alpha", "1.0.0") | {"src/lib.rs": b"// other\n"}
        self.publish("alpha", "1.0.0", files)
        served = sha256(self.archives[("alpha", "1.0.0")])
        reason = f"the archive has SHA-256 {served}, Cargo.lock has {locked}"
        report = self.check()
        self.assertEqual(report.problems, {ALPHA: [reason]})
        self.assertEqual((report.crates, report.failed), (1, 1))
        self.assertEqual(report.files, 0)

    def test_lock_checksum_changed_together_with_the_record_fails(self):
        """A lock and a record that agree still have to fit the archive."""
        other = "0" * 64
        self.lock("alpha", "1.0.0", checksum=other)
        edit_record(self.alpha, package=other)
        served = sha256(self.archives[("alpha", "1.0.0")])
        reason = f"the archive has SHA-256 {served}, Cargo.lock has {other}"
        self.assert_problems({ALPHA: [reason]})

    def test_archive_that_cannot_be_fetched_fails(self):
        del self.archives[("alpha", "1.0.0")]
        reason = "no archive to compare with: alpha 1.0.0: HTTP status 404"
        report = self.check()
        self.assertEqual(report.problems, {ALPHA: [reason]})
        self.assertEqual((report.crates, report.failed), (1, 1))

    def test_no_archive_is_fetched_after_a_failed_fetch(self):
        self.add("beta", "2.0.0")
        self.add("gamma", "3.0.0")
        del self.archives[("beta", "2.0.0")]
        reason = "no archive to compare with: beta 2.0.0: HTTP status 404"
        report = VENDOR.check(self.workspace, self.fetch, jobs=1)
        self.assertEqual(report.problems, {BETA: [reason]})
        self.assertEqual([name for name, _, _ in self.asked], ["alpha", "beta"])
        self.assertEqual(
            (report.crates, report.failed, report.skipped), (2, 1, 1)
        )

    def test_archive_that_cannot_be_read_fails(self):
        whole = self.archives[("alpha", "1.0.0")]
        unreadable = {
            "not gzip":  b"not an archive",
            "truncated": whole[:len(whole) // 2],
            "not tar":   gzip.compress(b"x" * 2048)
        }
        for name, raw in unreadable.items():
            with self.subTest(archive=name):
                self.archives[("alpha", "1.0.0")] = raw
                self.lock("alpha", "1.0.0", checksum=sha256(raw))
                edit_record(self.alpha, package=sha256(raw))
                problems = self.check().problems
                self.assertEqual(list(problems), [ALPHA])
                self.assertEqual(len(problems[ALPHA]), 1)
                self.assertRegex(
                    problems[ALPHA][0], "^the archive cannot be read: "
                )

    def test_archive_member_outside_the_package_root_fails(self):
        outside = ["beta-2.0.0/../alpha/src/lib.rs", "gamma-2.0.0/src/lib.rs"]
        moved   = [member(name, b"moved\n") for name in outside]
        self.add("beta", "2.0.0", others=moved)
        self.assert_problems({
            BETA: [
                f"archive member is not below beta-2.0.0/: {name}"
                for name in outside
            ]
        })

    def test_archive_member_that_is_a_link_fails(self):
        links = [
            member("beta-2.0.0/soft.rs", kind=tarfile.SYMTYPE, target="x.rs"),
            member("beta-2.0.0/hard.rs", kind=tarfile.LNKTYPE, target="x.rs")
        ]
        self.add("beta", "2.0.0", others=links)
        self.assert_problems({
            BETA: [
                "archive member is not a regular file: soft.rs",
                "archive member is not a regular file: hard.rs"
            ]
        })

    def test_second_archive_member_of_a_path_fails(self):
        second = member("beta-2.0.0/src/lib.rs", b"// the second one\n")
        self.add("beta", "2.0.0", others=[second])
        self.assert_problems({
            BETA: ["the archive has two members for: src/lib.rs"]
        })

    def test_directory_members_are_not_files(self):
        directories = [
            member("beta-2.0.0", kind=tarfile.DIRTYPE),
            member("beta-2.0.0/src", kind=tarfile.DIRTYPE)
        ]
        self.add("beta", "2.0.0", others=directories)
        self.assert_problems({})


class CrateSetTests(VendorCase):
    def test_locked_crate_that_is_not_vendored_fails(self):
        self.publish("beta", "2.0.0")
        self.lock(
            "beta", "2.0.0",
            checksum=sha256(self.archives[("beta", "2.0.0")])
        )
        report = self.check()
        self.assertEqual(
            report.problems, {"Cargo.lock": ["beta 2.0.0 is not vendored"]}
        )
        self.assertEqual((report.crates, report.failed), (1, 0))

    def test_vendored_crate_that_is_not_locked_fails(self):
        files = self.publish("beta", "2.0.0")
        self.place("beta", "2.0.0", files)
        report = self.check()
        self.assertEqual(
            report.problems,
            {BETA: ["not a crates.io package of Cargo.lock"]}
        )
        self.assertEqual((report.crates, report.failed), (1, 0))

    def test_another_version_lives_in_a_versioned_directory(self):
        self.add("alpha", "0.9.0", directory="alpha-0.9.0")
        self.add("gamma", "0.1.0+build.5")
        report = self.check()
        self.assertEqual(report.problems, {})
        self.assertEqual(report.crates, 3)

    def test_directory_name_must_fit_the_package(self):
        self.alpha.rename(self.vendor / "alpha-2.0.0")
        self.assert_problems({
            "vendor/alpha-2.0.0": [
                "the directory name does not fit its package, alpha 1.0.0"
            ],
            "Cargo.lock": ["alpha 1.0.0 is not vendored"]
        })

    def test_package_vendored_twice_fails(self):
        shutil.copytree(self.alpha, self.vendor / "alpha-1.0.0")
        self.assert_problems({
            "vendor/alpha-1.0.0": ["alpha 1.0.0 is also in alpha"]
        })

    def test_entry_that_is_not_a_crate_fails(self):
        write(self.vendor / "README.md", b"notes\n")
        (self.vendor / "empty").mkdir()
        write(self.vendor / "broken" / "Cargo.toml", b"[package\n")
        write(self.vendor / "nameless" / "Cargo.toml", b"[package]\n")
        write(self.vendor / "odd" / "Cargo.toml", manifest("../odd", "1.0.0"))
        os.symlink("alpha", self.vendor / "link")
        unnamed = "no Cargo.toml with a package name and version"
        self.assert_problems({
            "vendor/README.md": ["not a crate directory"],
            "vendor/link":      ["not a crate directory"],
            "vendor/empty":     [unnamed],
            "vendor/broken":    [unnamed],
            "vendor/nameless":  [unnamed],
            "vendor/odd":       [unnamed]
        })

    def test_package_from_another_source_fails(self):
        git   = "git+https://example.invalid/delta?rev=1#0123456"
        other = "registry+https://example.invalid/index"
        self.lock("delta", "3.0.0", source=git)
        self.lock("epsilon", "4.0.0", source=other, checksum="1" * 64)
        self.place("delta", "3.0.0", self.publish("delta", "3.0.0"))
        reason = "only crates.io packages can be compared"
        self.assert_problems({
            "vendor/delta (delta 3.0.0)": [
                "not a crates.io package of Cargo.lock"
            ],
            "Cargo.lock": [
                f"delta 3.0.0 comes from {git}: {reason}",
                f"epsilon 4.0.0 comes from {other}: {reason}"
            ]
        })


class WorkspaceTests(VendorCase):
    def refused(self, pattern):
        return self.assertRaisesRegex(VENDOR.VendorError, pattern)

    def test_unusable_lock_file_is_refused(self):
        alpha = {"name": "alpha", "version": "1.0.0", "source": CRATES_IO}
        whole = alpha | {"checksum": "0" * 64}
        short = {**alpha, "version": "1.0", "checksum": "0" * 63}
        odd   = whole | {"name": "al/pha"}
        unusable = {
            "has no SHA-256 for alpha 1.0.0": lock_text([alpha]),
            "has no SHA-256 for alpha 1.0": lock_text([short]),
            "lists alpha 1.0.0 twice": lock_text([whole, whole]),
            "has an invalid package identity": lock_text([odd]),
            "has no package list": b"version = 4\n",
            "unreadable lock file": b"[[package]\n"
        }
        for pattern, raw in unusable.items():
            with self.subTest(pattern=pattern), self.refused(pattern):
                write(self.workspace / "Cargo.lock", raw)
                self.check()
        (self.workspace / "Cargo.lock").unlink()
        with self.refused("unreadable lock file"):
            self.check()

    def test_configuration_must_replace_crates_io_with_a_directory(self):
        config = self.workspace / ".cargo" / "config.toml"
        registry = CONFIG.replace(b"directory", b"registry")
        pattern = "does not replace crates-io with a directory source"
        for raw in (b"", b"[build]\njobs = 2\n", registry):
            with self.subTest(raw=raw), self.refused(pattern):
                write(config, raw)
                self.check()
        config.unlink()
        with self.refused("unreadable Cargo configuration"):
            self.check()

    def test_vendor_directory_comes_from_the_configuration(self):
        moved = self.workspace / "third-party" / "crates"
        moved.parent.mkdir()
        self.vendor.rename(moved)
        with self.refused("not a real vendor directory"):
            self.check()
        os.symlink(moved, self.vendor)
        with self.refused("not a real vendor directory"):
            self.check()
        write(
            self.workspace / ".cargo" / "config.toml",
            CONFIG.replace(b'"vendor"', b'"third-party/crates"')
        )
        self.assert_problems({})
        write(moved / "alpha" / "src/lib.rs", b"// edited\n")
        self.assert_problems({
            "third-party/crates/alpha (alpha 1.0.0)": [
                "differs from the archive: src/lib.rs",
                f"{RECORD} has another SHA-256 for: src/lib.rs"
            ]
        })


class ArchiveSourceTests(unittest.TestCase):
    ADDRESS = "https://static.crates.io/crates/alpha/alpha-1.0.0.crate"

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.cache   = Path(temporary.name)
        self.fetched = []
        self.origins = {}

    def fetch(self, url):
        self.fetched.append(url)
        return b"downloaded"

    def test_local_archive_with_the_lock_checksum_is_used(self):
        write(self.cache / "alpha-1.0.0.crate", b"local")
        source = VENDOR.archive_source(self.cache, self.origins, self.fetch)
        self.assertEqual(source("alpha", "1.0.0", sha256(b"local")), b"local")
        self.assertEqual(self.fetched, [])
        self.assertEqual(self.origins, {("alpha", "1.0.0"): "local"})

    def test_local_archive_with_another_checksum_is_not_used(self):
        write(self.cache / "alpha-1.0.0.crate", b"stale")
        source = VENDOR.archive_source(self.cache, self.origins, self.fetch)
        self.assertEqual(
            source("alpha", "1.0.0", sha256(b"downloaded")), b"downloaded"
        )
        self.assertEqual(self.fetched, [self.ADDRESS])
        self.assertEqual(self.origins, {("alpha", "1.0.0"): "downloaded"})

    def test_archive_is_downloaded_without_a_local_directory(self):
        source = VENDOR.archive_source(None, self.origins, self.fetch)
        self.assertEqual(source("alpha", "1.0.0", "0" * 64), b"downloaded")
        self.assertEqual(self.fetched, [self.ADDRESS])
        self.assertEqual(self.origins, {("alpha", "1.0.0"): "downloaded"})

    def test_plus_of_a_version_is_escaped_in_the_address(self):
        self.assertEqual(
            VENDOR.archive_url("wasi", "0.11.1+wasi-snapshot-preview1"),
            "https://static.crates.io/crates/wasi/"
            "wasi-0.11.1%2Bwasi-snapshot-preview1.crate"
        )


class Answers(http.server.BaseHTTPRequestHandler):
    """Answer each request from the server's script; the last answer stays.

    An answer is a status, a body and the Content-Length to declare, where
    None declares the length of the body.
    """

    def do_GET(self):
        script = self.server.script
        status, body, length = script.pop(0) if len(script) > 1 else script[0]
        self.server.seen.append(
            (self.path, self.headers.get_all("User-Agent"))
        )
        self.send_response(status)
        self.send_header(
            "Content-Length", str(len(body) if length is None else length)
        )
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *arguments):
        pass


class DownloadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Answers)
        cls.thread = threading.Thread(
            target=cls.server.serve_forever, kwargs={"poll_interval": 0.01},
            daemon=True
        )
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.server.script = []
        self.server.seen   = []
        # A proxy from the environment must not get the loopback requests.
        direct = mock.patch.dict(
            os.environ, {"no_proxy": "127.0.0.1", "NO_PROXY": "127.0.0.1"}
        )
        direct.start()
        self.addCleanup(direct.stop)
        port = self.server.server_address[1]
        self.url = f"http://127.0.0.1:{port}/crates/alpha/alpha-1.0.0.crate"

    def answer(self, *script):
        self.server.script = list(script)

    def download(self, **options):
        return VENDOR.download(self.url, pause=0, **options)

    def test_download_returns_the_body_and_names_only_the_tool(self):
        body = bytes(range(256)) * 10_000
        self.answer((200, body, None))
        self.assertEqual(self.download(), body)
        agent = (
            "cargo-vendor-check/1 "
            "(compares a committed cargo vendor tree with crates.io archives)"
        )
        self.assertEqual(
            self.server.seen, [("/crates/alpha/alpha-1.0.0.crate", [agent])]
        )

    def test_download_tries_again_after_a_passing_failure(self):
        for status in (408, 429, 500, 503):
            with self.subTest(status=status):
                self.server.seen = []
                self.answer((status, b"later", None), (200, b"archive", None))
                self.assertEqual(self.download(), b"archive")
                self.assertEqual(len(self.server.seen), 2)

    def test_download_gives_up_after_the_last_attempt(self):
        self.answer((503, b"later", None))
        with self.assertRaisesRegex(VENDOR.FetchError, "HTTP status 503$"):
            self.download()
        self.assertEqual(len(self.server.seen), VENDOR.ATTEMPTS)

    def test_download_waits_longer_before_each_further_attempt(self):
        self.answer((503, b"later", None))
        with mock.patch.object(VENDOR.time, "sleep") as sleep:
            with self.assertRaises(VENDOR.FetchError):
                VENDOR.download(self.url)
        self.assertEqual(sleep.call_args_list, [mock.call(2.0), mock.call(4.0)])

    def test_download_does_not_ask_again_for_a_lasting_failure(self):
        for status in (403, 404):
            with self.subTest(status=status):
                self.server.seen = []
                self.answer((status, b"no", None), (200, b"archive", None))
                with self.assertRaisesRegex(
                    VENDOR.FetchError, f"HTTP status {status}$"
                ):
                    self.download()
                self.assertEqual(len(self.server.seen), 1)

    def test_download_refuses_a_truncated_answer(self):
        self.answer((200, b"only a part", 100))
        with self.assertRaisesRegex(VENDOR.FetchError, "(?i)incomplete"):
            self.download(attempts=2)
        self.assertEqual(len(self.server.seen), 2)

    def test_download_refuses_an_oversized_answer(self):
        self.answer((200, b"too long", None))
        with mock.patch.object(VENDOR, "MAX_ARCHIVE_BYTES", 4):
            with self.assertRaisesRegex(VENDOR.FetchError, "more than 4 bytes"):
                self.download()
        self.assertEqual(len(self.server.seen), 1)

    def test_download_reports_a_refused_connection(self):
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        with self.assertRaisesRegex(VENDOR.FetchError, "URLError"):
            VENDOR.download(
                f"http://127.0.0.1:{port}/alpha.crate", attempts=2, pause=0
            )


class MainTests(VendorCase):
    def download(self, url):
        """Stand in for crates.io: answer by the address of each archive."""
        for (name, version), archive in self.archives.items():
            if url == VENDOR.archive_url(name, version):
                return archive
        raise VENDOR.FetchError(f"{url}: HTTP status 404")

    def run_main(self, *arguments, download=None):
        """Run main() on the workspace; return its status and both streams."""
        out, err = io.StringIO(), io.StringIO()
        with (
            mock.patch.object(VENDOR, "download", download or self.download),
            contextlib.redirect_stdout(out), contextlib.redirect_stderr(err)
        ):
            status = VENDOR.main([
                "--workspace", str(self.workspace), *arguments
            ])
        return status, out.getvalue(), err.getvalue()

    def test_identical_tree_exits_zero(self):
        size = sum(map(len, sources("alpha", "1.0.0").values()))
        out = (
            "cargo vendor check: passed\n"
            "    vendored crates equal to their crates.io archive: 1\n"
            f"    files compared: 5 ({size} bytes)\n"
            "    archive members absent because cargo vendor leaves them "
            "out: 0\n"
            "    archives downloaded: 1, read from --archive-dir: 0\n"
        )
        self.assertEqual(self.run_main(), (0, out, ""))

    def test_difference_exits_one_and_is_named(self):
        write(self.alpha / "src/lib.rs", b"// edited\n")
        self.add("beta", "2.0.0")
        write(self.vendor / "notes.txt", b"stray\n")
        err = (
            "vendor/notes.txt:\n"
            "    not a crate directory\n"
            "vendor/alpha (alpha 1.0.0):\n"
            "    differs from the archive: src/lib.rs\n"
            f"    {RECORD} has another SHA-256 for: src/lib.rs\n"
            "cargo vendor check: FAILED\n"
            "    vendored crates not shown equal to their crates.io "
            "archive: 1 of 2\n"
            "    vendored crates skipped after a failed fetch: 0\n"
            "    other entries with problems: 1\n"
            "    archives downloaded: 2, read from --archive-dir: 0\n"
        )
        self.assertEqual(self.run_main("--jobs", "0"), (1, "", err))

    def test_failed_download_exits_one(self):
        def refuse(url):
            raise VENDOR.FetchError(f"{url}: HTTP status 503")

        self.add("beta", "2.0.0")
        address = "https://static.crates.io/crates/alpha/alpha-1.0.0.crate"
        err = (
            "vendor/alpha (alpha 1.0.0):\n"
            f"    no archive to compare with: {address}: HTTP status 503\n"
            "cargo vendor check: FAILED\n"
            "    vendored crates not shown equal to their crates.io "
            "archive: 1 of 1\n"
            "    vendored crates skipped after a failed fetch: 1\n"
            "    other entries with problems: 0\n"
            "    archives downloaded: 1, read from --archive-dir: 0\n"
        )
        result = self.run_main("--jobs", "1", download=refuse)
        self.assertEqual(result, (1, "", err))

    def test_archive_directory_stands_in_for_downloads(self):
        self.add("beta", "2.0.0")
        cache = self.workspace.parent / "cache"
        write(cache / "alpha-1.0.0.crate", self.archives[("alpha", "1.0.0")])
        write(cache / "beta-2.0.0.crate", b"not the published archive")
        asked = []

        def download(url):
            asked.append(url)
            return self.download(url)

        status, out, err = self.run_main(
            "--archive-dir", str(cache), download=download
        )
        self.assertEqual((status, err), (0, ""))
        self.assertIn(
            "    archives downloaded: 1, read from --archive-dir: 1\n", out
        )
        self.assertEqual(asked, [VENDOR.archive_url("beta", "2.0.0")])

    def test_left_out_members_are_counted_by_kind(self):
        left = {
            ".gitattributes":   b"* text\n",
            ".gitignore":       b"target\n",
            "tests/.gitignore": b"*.tmp\n"
        }
        self.add("beta", "2.0.0", sources("beta", "2.0.0") | left, omit=left)
        status, out, err = self.run_main()
        self.assertEqual((status, err), (0, ""))
        self.assertIn(
            "    archive members absent because cargo vendor leaves them "
            "out: 3 (1 .gitattributes, 2 .gitignore)\n",
            out
        )

    def test_long_list_of_problems_is_cut(self):
        for number in range(VENDOR.SHOWN + 5):
            write(self.alpha / f"extra/{number:02}.rs", b"")
        relist(self.alpha)
        status, out, err = self.run_main()
        self.assertEqual((status, out), (1, ""))
        self.assertEqual(err.count("    not in the archive: "), VENDOR.SHOWN)
        self.assertIn("    and 5 more\n", err)

    def test_unusable_workspace_exits_one(self):
        (self.workspace / "Cargo.lock").unlink()
        status, out, err = self.run_main()
        self.assertEqual((status, out), (1, ""))
        self.assertRegex(err, "^cargo vendor check: unreadable lock file: ")

    @unittest.skipIf(
        getattr(os, "geteuid", lambda: 0)() == 0,
        "needs a user that a directory's mode can lock out"
    )
    def test_directory_that_cannot_be_listed_exits_one(self):
        """Its files are not compared, so the tree must not pass."""
        closed = self.alpha / "closed"
        write(closed / "extra.rs", b"pub fn more() {}\n")
        closed.chmod(0)
        self.addCleanup(closed.chmod, 0o700)
        status, out, err = self.run_main()
        self.assertEqual((status, out), (1, ""))
        self.assertRegex(err, "^cargo vendor check: .*vendor/alpha/closed")

    def test_name_that_the_stream_cannot_encode_is_escaped(self):
        stream = io.TextIOWrapper(io.BytesIO(), encoding="ascii")
        VENDOR.say("not in the archive: café/\udcff.rs", stream)
        stream.flush()
        self.assertEqual(
            stream.buffer.getvalue(),
            b"not in the archive: caf\\xe9/\\udcff.rs\n"
        )


if __name__ == "__main__":
    unittest.main()
