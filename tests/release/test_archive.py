"""Pure release helpers: tiny payloads, no Cargo/compiler or native execution."""

import importlib.util
import os
from pathlib import Path
import stat
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts"
sys.path.insert(0, str(SCRIPTS))


def module(name):
    specification = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    value = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(value)
    return value


archive = module("release-archive")
metadata = module("release-metadata")
smoke = module("release-smoke")
from release_common import MAX_BINARY_BYTES, TARGETS, archive_name, root_name


class ReleaseArchive(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / "binary"
        self.binary.write_bytes(b"tiny archive payload")
        self.readme = self.root / "README"
        self.readme.write_bytes(b"README\r\n")
        self.license = self.root / "LICENSE"
        self.license.write_bytes(b"MIT\n")
        self.notices = self.root / "notices"
        (self.notices / "assets/licenses").mkdir(parents=True)
        (self.notices / "crates/example-1.0.0").mkdir(parents=True)
        (self.notices / "EMBEDDED.txt").write_bytes(b"embedded\r\n")
        (self.notices / "crates.txt").write_bytes(b"example 1.0.0: MIT (https://crates.io)\n")
        (self.notices / "assets/licenses/example").write_bytes(b"notice\r\n")
        (self.notices / "crates/example-1.0.0/LICENSE").write_bytes(b"crate license\n")

    def packed(self, target, directory):
        return archive.pack(
            self.binary, self.readme, self.license, self.notices,
            "0.1.0", target, 1_761_782_400, self.root / directory
        )

    def test_archive_bytes_are_reproducible_despite_source_metadata_changes(self):
        for target in ["x86_64-unknown-linux-gnu", "aarch64-pc-windows-msvc"]:
            first = self.packed(target, f"first-{target}")
            os.utime(self.binary, (1, 1))
            self.binary.chmod(0o600)
            second = self.packed(target, f"second-{target}")
            self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_tar_layout_modes_timestamps_and_exact_notice_bytes(self):
        target = "aarch64-apple-darwin"
        packed = self.packed(target, "tar")
        root = root_name("0.1.0", target)
        with tarfile.open(packed) as source:
            members = source.getmembers()
            names = [item.name.rstrip("/") + ("/" if item.isdir() else "")
                     for item in members]
            self.assertEqual(names, sorted(names, key=lambda name: name.encode()))
            for item in members:
                self.assertEqual((item.uid, item.gid, item.uname, item.gname), (0, 0, "", ""))
                self.assertEqual(item.mtime, 1_761_782_400)
                expected = 0o755 if item.isdir() or item.name == f"{root}/writer" else 0o644
                self.assertEqual(item.mode, expected)
            self.assertEqual(source.extractfile(f"{root}/README.md").read(), b"README\r\n")
            self.assertEqual(source.extractfile(
                f"{root}/THIRD-PARTY-LICENSES/assets/licenses/example"
            ).read(), b"notice\r\n")

    def test_zip_layout_and_binary_executable_mode_survive_unpacking(self):
        target = "x86_64-pc-windows-msvc"
        packed = self.packed(target, "zip")
        root = root_name("0.1.0", target)
        with zipfile.ZipFile(packed) as source:
            binary = source.getinfo(f"{root}/writer.exe")
            self.assertEqual(binary.create_system, 3)
            self.assertEqual(stat.S_IMODE(binary.external_attr >> 16), 0o755)
            self.assertIsNone(source.testzip())
        expanded = smoke.unpack(packed, self.root / "unpacked", root)
        self.assertEqual((expanded / "writer.exe").read_bytes(), self.binary.read_bytes())
        self.assertEqual((expanded / "THIRD-PARTY-LICENSES/EMBEDDED.txt").read_bytes(),
                         b"embedded\r\n")

    def test_oversize_and_incomplete_notice_tree_fail_before_new_output(self):
        with self.binary.open("wb") as stream:
            stream.truncate(MAX_BINARY_BYTES + 1)
        with self.assertRaisesRegex(ValueError, "ceiling"):
            self.packed(TARGETS[0], "not-created")
        self.assertFalse((self.root / "not-created").exists())
        self.binary.write_bytes(b"small")
        (self.notices / "EMBEDDED.txt").unlink()
        with self.assertRaisesRegex(ValueError, "notice member"):
            self.packed(TARGETS[0], "also-not-created")
        self.assertFalse((self.root / "also-not-created").exists())

    def test_checksums_require_exactly_six_named_archives_and_detect_changes(self):
        for target in TARGETS:
            self.packed(target, "all")
        output = self.root / "all"
        sums = archive.checksums(output, "0.1.0")
        self.assertEqual(len(sums.read_text().splitlines()), 6)
        archive.checksums(output, "0.1.0", verify=True)
        member = output / archive_name("0.1.0", TARGETS[0])
        member.write_bytes(member.read_bytes() + b"changed")
        with self.assertRaisesRegex(ValueError, "does not match"):
            archive.checksums(output, "0.1.0", verify=True)
        member.unlink()
        with self.assertRaisesRegex(ValueError, "six target archives"):
            archive.checksums(output, "0.1.0")

    def test_unsafe_or_duplicate_archive_members_refuse(self):
        for names in [
            ["root/../outside"], ["/root/file"], ["root/file", "root/./file"],
            ["root/.. /outside"], ["root/file:stream"], ["root/CON.txt"],
            ["root/COM1"], ["root/name."], ["root/A", "root/a"],
            ["root/café", "root/cafe\u0301"],
        ]:
            with self.assertRaises(ValueError):
                smoke.safe_members(names, "root")

    def test_zip_nonfile_types_refuse_before_any_member_is_written(self):
        for kind in [stat.S_IFLNK, stat.S_IFIFO, stat.S_IFSOCK,
                     stat.S_IFCHR, stat.S_IFBLK]:
            with self.subTest(kind=kind):
                packed = self.root / f"nonfile-{kind}.zip"
                with zipfile.ZipFile(packed, "w") as source:
                    source.writestr("root/first.txt", b"ordinary first member")
                    entry = zipfile.ZipInfo("root/nonfile")
                    entry.create_system = 3
                    entry.external_attr = (kind | 0o644) << 16
                    source.writestr(entry, b"not a regular file")
                destination = self.root / f"rejected-{kind}"
                with self.assertRaisesRegex(ValueError, "symbolic link|non-file"):
                    smoke.unpack(packed, destination, "root")
                self.assertFalse(destination.exists())

    def test_zip_explicit_and_untyped_regular_members_remain_portable(self):
        packed = self.root / "regular-members.zip"
        with zipfile.ZipFile(packed, "w") as source:
            for name, mode in [
                ("untyped", 0o644), ("regular", stat.S_IFREG | 0o755),
            ]:
                entry = zipfile.ZipInfo(f"root/{name}")
                entry.create_system = 3
                entry.external_attr = mode << 16
                source.writestr(entry, name.encode())
        expanded = smoke.unpack(packed, self.root / "regular-output", "root")
        self.assertEqual((expanded / "untyped").read_bytes(), b"untyped")
        self.assertEqual((expanded / "regular").read_bytes(), b"regular")

    def test_failed_disposable_smoke_never_moves_or_removes_checkout_assets(self):
        target = "x86_64-apple-darwin"
        self.binary.write_bytes(b"\xcf\xfa\xed\xfe" + (0x01000007).to_bytes(4, "little"))
        self.packed(target, "smoke")
        assets = self.root / "checkout/assets"
        assets.mkdir(parents=True)
        (assets / "original").write_bytes(b"owned source")
        unexpected = assets / "appeared-during-smoke"

        def fake_run(_binary, arguments, _cwd, _epoch):
            if arguments == ["--version"]:
                return f"writer 0.1.0 ({'a' * 40})\n".encode()
            if arguments == ["--help"]:
                return b"Usage: writer\n"
            if arguments == ["--licenses"]:
                return (self.notices / "EMBEDDED.txt").read_bytes()
            raise AssertionError(f"unexpected command: {arguments}")

        def failed_outputs(_binary, temporary, _epoch):
            self.assertNotEqual(temporary, self.root)
            self.assertEqual((assets / "original").read_bytes(), b"owned source")
            unexpected.write_bytes(b"new source data")
            raise ValueError("render failure")

        with patch.object(smoke, "run", side_effect=fake_run):
            with patch.object(smoke, "outputs", side_effect=failed_outputs):
                with self.assertRaisesRegex(ValueError, "render failure"):
                    smoke.smoke(
                        self.root / "smoke", "0.1.0", target,
                        "a" * 40, 1_761_782_400
                    )
        self.assertEqual((assets / "original").read_bytes(), b"owned source")
        self.assertEqual(unexpected.read_bytes(), b"new source data")

    def test_concurrent_archive_destination_is_never_overwritten(self):
        real_link = archive.os.link

        def claim_first(source, destination):
            Path(destination).write_bytes(b"concurrent owner")
            real_link(source, destination)

        with patch.object(archive.os, "link", side_effect=claim_first):
            with self.assertRaises(FileExistsError):
                self.packed(TARGETS[0], "race")
        result = self.root / "race" / archive_name("0.1.0", TARGETS[0])
        self.assertEqual(result.read_bytes(), b"concurrent owner")
        self.assertFalse(list(result.parent.glob(".writer-archive-*")))

    def test_architecture_check_distinguishes_each_frozen_target(self):
        for target in TARGETS:
            arm = target.startswith("aarch64-")
            data = bytearray(128)
            if "-linux-" in target:
                data[:6] = b"\x7fELF\x02\x01"
                data[18:20] = (183 if arm else 62).to_bytes(2, "little")
            elif "-apple-" in target:
                data[:4] = b"\xcf\xfa\xed\xfe"
                data[4:8] = (0x0100000C if arm else 0x01000007).to_bytes(4, "little")
            else:
                data[:2] = b"MZ"
                data[60:64] = (80).to_bytes(4, "little")
                data[80:84] = b"PE\0\0"
                data[84:86] = (0xAA64 if arm else 0x8664).to_bytes(2, "little")
            self.binary.write_bytes(data)
            smoke.verify_architecture(self.binary, target)
            wrong = target.replace("aarch64-", "x86_64-") if arm else target.replace("x86_64-", "aarch64-")
            with self.assertRaisesRegex(ValueError, "architecture"):
                smoke.verify_architecture(self.binary, wrong)


class ReleaseMetadata(unittest.TestCase):
    def test_manual_revision_identity_and_existing_tag_are_checked_without_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[package]\nname="writer"\nversion="0.1.0"\n')
            calls = []
            def fake_git(_root, *arguments):
                calls.append(arguments)
                if arguments[0] == "status":
                    return ""
                if arguments[0] == "show":
                    return "1761782400"
                return "a" * 40
            with patch.object(metadata, "git", side_effect=fake_git):
                result = metadata.metadata(root, "v0.1.0")
            self.assertEqual(result["build_id"], "a" * 40)
            self.assertEqual(result["tag"], "v0.1.0")
            self.assertTrue(all(call[0] in {"rev-parse", "status", "show"} for call in calls))
            with patch.object(metadata, "git", side_effect=fake_git):
                with self.assertRaisesRegex(ValueError, "must equal"):
                    metadata.metadata(root, "v9.0.0")
            with patch.object(metadata, "git", side_effect=["a" * 40, "", "1761782400", "b" * 40]):
                with self.assertRaisesRegex(ValueError, "selected revision"):
                    metadata.metadata(root, "v0.1.0")


if __name__ == "__main__":
    unittest.main()
