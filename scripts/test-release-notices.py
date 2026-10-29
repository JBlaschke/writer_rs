#!/usr/bin/env python3
"""Pure notice tests: mocked builds plus exact real release-payload staging."""

import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch


sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "release_notices", Path(__file__).with_name("release-notices.py")
)
NOTICES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NOTICES)
REVIEWED_PACKAGE_PINS = copy.deepcopy(NOTICES.PINNED_PACKAGES)
REVIEWED_NOTICE_GROUPS = copy.deepcopy(NOTICES.PINNED_NOTICE_GROUPS)
REVIEWED_RELEASE_ROOT = Path(NOTICES.RELEASE_NOTICE_ROOT)


def write(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw)


class ReleaseNoticeDataTests(unittest.TestCase):
    def test_real_release_payloads_are_regular_confined_and_hash_exact(self):
        """Check actual source data, independently of patched fixture globals."""
        root = REVIEWED_RELEASE_ROOT.resolve(strict=True)
        checked = set()
        for group in REVIEWED_NOTICE_GROUPS.values():
            for source in group["files"]:
                if source["kind"] != "release":
                    continue
                relative = Path(source["path"])
                with self.subTest(path=source["path"]):
                    self.assertFalse(relative.is_absolute())
                    self.assertNotIn("..", relative.parts)
                    path = root / relative
                    resolved = path.resolve(strict=True)
                    self.assertTrue(
                        resolved.is_relative_to(root),
                        f"release payload escapes helper checkout: {path}"
                    )
                    self.assertTrue(
                        stat.S_ISREG(path.lstat().st_mode),
                        f"release payload is not a regular file: {path}"
                    )
                    self.assertEqual(
                        hashlib.sha256(path.read_bytes()).hexdigest(),
                        source["sha256"]
                    )
                    checked.add(source["path"])
        self.assertIn("docs/release/notice-sources/typst-0.15.1/NOTICE", checked)


class NoticeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.workspace = self.root / "workspace"
        self.assets = self.workspace / "assets"
        self.binary = self.root / "archive-input" / "writer"
        write(self.workspace / "Cargo.toml", b"[package]\n")
        write(self.workspace / "LICENSE", b"Writer notice\r\n")
        write(self.binary, b"not executed by pure tests")
        for relative in NOTICES.REQUIRED_NOTICES:
            write(self.assets / relative, (relative + "\r\n").encode())
        self.writer = self.package("writer", "0.1.0", self.workspace, None)
        self.metadata = {
            "workspace_root": str(self.workspace),
            "workspace_members": [self.writer["id"]],
            "packages": [self.writer],
        }
        self.tree = f"writer v0.1.0 ({self.workspace})\n"

    def package(
                self, name, version, directory,
                source="registry+https://github.com/rust-lang/crates.io-index",
                license="MIT", license_file=None
            ):
        write(directory / "Cargo.toml", b"[package]\n")
        return {
            "id": f"{source or 'path+' + str(directory)}#{name}@{version}",
            "name": name, "version": version, "source": source,
            "manifest_path": str(directory / "Cargo.toml"),
            "license": license, "license_file": license_file,
        }

    def dependency(self, name="linked", version="1.2.3", **kwargs):
        package = self.package(
            name, version, self.root / "registry" / f"{name}-{version}",
            **kwargs
        )
        self.metadata["packages"].append(package)
        self.tree += f"{name} v{version}\n"
        return package

    def stdout(self):
        return NOTICES.embedded_bytes(
            "0.1.0", NOTICES.embedded_notices(self.assets)
        )

    def payload(self, stdout=None):
        if stdout is None:
            stdout = self.stdout()
        return NOTICES.notice_payload(
            self.metadata, self.tree, self.assets, stdout
        )

    def pinned_fixture(
                self, group_name, names, version,
                vcs_change=None, member="regular"
            ):
        self.metadata["packages"] = [self.writer]
        self.tree = f"writer v0.1.0 ({self.workspace})\n"
        packages, pins, archives = [], {}, {}
        registry = self.root / "cargo/registry"
        group = copy.deepcopy(NOTICES.PINNED_NOTICE_GROUPS[group_name])
        locked = "version = 4\n"
        for name in names:
            pin = copy.deepcopy(REVIEWED_PACKAGE_PINS[(name, version)])
            directory = registry / "src/test-index" / f"{name}-{version}"
            package = self.package(
                name, version, directory, source=group["source"],
                license=group["license"],
            )
            package["repository"] = group["repository"]
            self.metadata["packages"].append(package)
            self.tree += f"{name} v{version}\n"
            vcs = {
                "git": {"sha1": group["revision"]},
                "path_in_vcs": pin["vcs_path"],
            }
            if vcs_change:
                vcs_change(vcs)
            raw = json.dumps(vcs, sort_keys=True).encode()
            write(directory / ".cargo_vcs_info.json", raw)
            archive = registry / "cache/test-index" / f"{name}-{version}.crate"
            archive.parent.mkdir(parents=True, exist_ok=True)
            with tarfile.open(archive, "w:gz") as target:
                for _ in range(2 if member == "duplicate" else 1):
                    if member == "missing":
                        continue
                    entry = tarfile.TarInfo(f"{name}-{version}/.cargo_vcs_info.json")
                    if member == "symlink":
                        entry.type = tarfile.SYMTYPE
                        entry.linkname = "../../outside"
                        target.addfile(entry)
                    else:
                        entry.size = len(raw)
                        target.addfile(entry, io.BytesIO(raw))
            checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
            pin["checksum"] = checksum
            pins[(name, version)] = pin
            locked += (
                f'\n[[package]]\nname = "{name}"\nversion = "{version}"\n'
                f'source = "{group["source"]}"\nchecksum = "{checksum}"\n'
            )
            packages.append(package)
            archives[name] = archive
        write(self.workspace / "Cargo.lock", locked.encode())
        notice_bytes = {}
        source_root = self.root / "helper-checkout"
        for source in group["files"]:
            raw = f"Pinned {source['name']} fixture\r\n".encode()
            notice_bytes[source["name"]] = raw
            parent = self.assets if source["kind"] == "embedded" else source_root
            write(parent / source["path"], raw)
            source["sha256"] = hashlib.sha256(raw).hexdigest()
        pin_patch = patch.dict(NOTICES.PINNED_PACKAGES, pins, clear=True)
        pin_patch.start()
        self.addCleanup(pin_patch.stop)
        notice_patch = patch.dict(
            NOTICES.PINNED_NOTICE_GROUPS, {group_name: group}
        )
        notice_patch.start()
        self.addCleanup(notice_patch.stop)
        root_patch = patch.object(NOTICES, "RELEASE_NOTICE_ROOT", source_root)
        root_patch.start()
        self.addCleanup(root_patch.stop)
        return packages, archives, notice_bytes

    def mitex_fixture(self, names=("mitex",), vcs_change=None, member="regular"):
        packages, archives, notices = self.pinned_fixture(
            "mitex", names, "0.2.4", vcs_change=vcs_change, member=member
        )
        return packages, archives, notices["LICENSE"]

    def test_known_binary_encoding_preserves_crlf_final_lf_and_lossy_text(self):
        notices = {
            "licenses/z": b"last\r\n",
            "licenses/a": b"raw \xff\n\n",
            "licenses/m": b"no final newline",
        }
        expected = (
            "writer 0.1.0: third-party notices for the files and data embedded in this binary.\n"
            "Licenses of the linked Rust crates: THIRD-PARTY-LICENSES/ in the release archive, or each crate's source on crates.io.\n\n"
            "==> licenses/a <==\nraw \ufffd\n\n\n"
            "==> licenses/m <==\nno final newline\n\n"
            "==> licenses/z <==\nlast\r\n\n"
        ).encode()
        self.assertEqual(NOTICES.embedded_bytes("0.1.0", notices), expected)
        NOTICES.verify_embedded(expected, "0.1.0", notices)

    def test_inventory_matches_predicate_and_excludes_embed_junk(self):
        extras = {
            "licenses/new-note.txt": b"new notice",
            "vendor/new/coPyIng.extra": b"copying",
            "vendor/new/readme-data.txt": b"readme",
            "vendor/new/MANIFEST-data": b"manifest",
            "vendor/new/acknowledgement-notice.txt": b"notice",
        }
        for relative, raw in extras.items():
            write(self.assets / relative, raw)
        for relative in (
            "licenses/.DS_Store", "licenses/._notice", "licenses/Thumbs.db",
            "licenses/notice.swp", "licenses/notice~", "licenses/notice.map",
            "vendor/new/README", "vendor/new/data.txt",
        ):
            write(self.assets / relative, b"not embedded notice")
        actual = NOTICES.embedded_notices(self.assets)
        self.assertEqual(set(actual), NOTICES.REQUIRED_NOTICES | extras.keys())
        self.assertEqual(list(actual), sorted(actual))
        for relative, raw in extras.items():
            self.assertEqual(actual[relative], raw)

    def test_required_source_notice_omission_fails_before_output(self):
        (self.assets / "vendor/crates/typst-assets/NOTICE").unlink()
        with self.assertRaisesRegex(NOTICES.NoticeError, "typst-assets/NOTICE"):
            NOTICES.embedded_notices(self.assets)

    def test_binary_omitted_altered_extra_or_wrong_version_notices_fail(self):
        exact = self.stdout()
        marker = b"==> licenses/two-face.md <==\n"
        begin = exact.index(marker)
        end = exact.index(b"==> ", begin + len(marker))
        for changed in (
            exact[:begin] + exact[end:],
            exact.replace(b"two-face.md\r\n", b"two-face.md\n", 1),
            exact + b"unexpected extra output\n",
            exact.replace(b"writer 0.1.0:", b"writer 0.2.0:", 1),
        ):
            with self.subTest(changed=changed[:50]):
                with self.assertRaisesRegex(NOTICES.NoticeError, "differs"):
                    self.payload(changed)

    def test_assets_and_embedded_stdout_are_copied_without_normalization(self):
        relative = "licenses/two-face.md"
        raw = b"Copyright\r\n\r\nLast line  \r\n"
        write(self.assets / relative, raw)
        exact = self.stdout()
        payload = self.payload(exact)
        self.assertEqual(payload[f"assets/{relative}"], raw)
        self.assertEqual(payload["EMBEDDED.txt"], exact)
        self.assertEqual(payload["crates/writer-0.1.0/LICENSE"],
            b"Writer notice\r\n")

    def test_linked_tree_deduplicates_and_copies_only_linked_normal_packages(self):
        linked = self.dependency()
        write(Path(linked["manifest_path"]).parent / "NOTICE", b"linked notice")
        unused = self.package(
            "unused-host-or-dev", "9.0.0", self.root / "unused"
        )
        self.metadata["packages"].append(unused)
        self.tree += "linked v1.2.3 (*)\nlinked v1.2.3\n"
        payload = self.payload()
        self.assertEqual(payload["crates.txt"].decode(), (
            "linked 1.2.3: MIT (https://crates.io/crates/linked/1.2.3)\n"
            "writer 0.1.0: MIT (.)\n"
        ))
        self.assertFalse(any("unused" in path for path in payload))
        self.assertNotIn(str(self.root).encode(), payload["crates.txt"])
        self.assertEqual(payload["crates/linked-1.2.3/NOTICE"], b"linked notice")

    def test_actual_nested_cargo_prefixes_and_ascii_equivalents_keep_identities(self):
        # Literal forms from the preserved real aarch64-apple-darwin capture;
        # the workspace path remains this test's tiny portable fixture.
        captured = (
            "├── anstyle v1.0.14\n"
            "├── anstyle-parse v1.0.0\n"
            "│   └── utf8parse v0.2.2\n"
            "├── axum v0.8.9\n"
            "│   ├── axum-core v0.5.6\n"
            "│   │   ├── bytes v1.12.1\n"
            "│   │   ├── http v1.5.0\n"
            "│   │   │   └── itoa v1.0.18\n"
            "│   │   ├── http-body v1.1.0\n"
            "│   │   │   └── http v1.5.0 (*)\n"
            "│   │       ├── log v0.4.34\n"
            "│   │           └── once_cell v1.21.4\n"
        )
        versions = (
            ("anstyle", "1.0.14"), ("anstyle-parse", "1.0.0"),
            ("utf8parse", "0.2.2"), ("axum", "0.8.9"),
            ("axum-core", "0.5.6"), ("bytes", "1.12.1"),
            ("http", "1.5.0"), ("itoa", "1.0.18"),
            ("http-body", "1.1.0"), ("log", "0.4.34"),
            ("once_cell", "1.21.4"),
        )
        for name, version in versions:
            self.dependency(name, version)
        root = self.tree.splitlines()[0] + "\n"
        ascii_tree = captured.translate(str.maketrans({
            "│": "|", "├": "|", "└": "`", "─": "-",
        }))
        expected = sorted([("writer", "0.1.0"), *versions])
        for tree in (root + captured, root + ascii_tree, self.tree):
            with self.subTest(tree=tree):
                writer, packages = NOTICES.linked_packages(self.metadata, tree)
                self.assertEqual(writer["id"], self.writer["id"])
                self.assertEqual([
                    (package["name"], package["version"])
                    for package in packages
                ], expected)

    def test_missing_ambiguous_or_decorated_tree_identity_fails(self):
        self.dependency()
        for tree in (
            self.tree + "unknown v1.0.0\n",
            self.tree + "+-- linked v1.2.3\n",
            self.tree + "linked v1.2.3 [build-dependencies]\n",
            self.tree + "│  └── linked v1.2.3\n",
            self.tree + "|   └── linked v1.2.3\n",
            self.tree + "├─── linked v1.2.3\n",
            self.tree + "\x1b[31m├── linked v1.2.3\x1b[0m\n",
            self.tree + "├── \x1b[31mlinked v1.2.3\x1b[0m\n",
            "",
        ):
            with self.subTest(tree=tree):
                with self.assertRaises(NOTICES.NoticeError):
                    NOTICES.linked_packages(self.metadata, tree)
        duplicate = copy.deepcopy(self.metadata["packages"][-1])
        duplicate["id"] += "-other-source"
        self.metadata["packages"].append(duplicate)
        with self.assertRaisesRegex(NOTICES.NoticeError, "2 metadata matches"):
            NOTICES.linked_packages(self.metadata, self.tree)

    def test_workspace_path_must_match_the_metadata_identity(self):
        for origin in ("", f" ({self.root / 'different'})"):
            with self.assertRaisesRegex(NOTICES.NoticeError, "does not match"):
                NOTICES.linked_packages(
                    self.metadata, f"writer v0.1.0{origin}\n"
                )

    def test_workspace_nested_build_tests_and_assets_are_not_crate_notices(self):
        for path in (
            "target/recovery/LICENSE", "tests/fixtures/NOTICE",
            ".git/NOTICE", "assets/vendor/other/LICENSE",
        ):
            write(self.workspace / path, b"not writer's own notice")
        payload = self.payload()
        actual = [path for path in payload if path.startswith("crates/writer-")]
        self.assertEqual(actual, ["crates/writer-0.1.0/LICENSE"])

    def test_registry_nested_notices_and_explicit_custom_license_are_exact(self):
        package = self.dependency(license=None, license_file="legal/terms.txt")
        root = Path(package["manifest_path"]).parent
        write(root / "legal/terms.txt", b"custom terms\r\n ")
        write(root / "data/NOTICE-fonts", b"font attribution\r\n")
        write(root / "UNLICENSE", b"public domain\n")
        write(root / "LICENSE.map", b"legal notice despite asset suffix\n")
        payload = self.payload()
        prefix = "crates/linked-1.2.3/"
        self.assertEqual(payload[prefix + "legal/terms.txt"], b"custom terms\r\n ")
        self.assertEqual(payload[prefix + "data/NOTICE-fonts"],
            b"font attribution\r\n")
        self.assertEqual(payload[prefix + "UNLICENSE"], b"public domain\n")
        self.assertEqual(payload[prefix + "LICENSE.map"],
            b"legal notice despite asset suffix\n")
        self.assertIn(b"linked 1.2.3: UNSPECIFIED", payload["crates.txt"])

    def test_explicit_missing_or_escaping_license_file_is_not_silently_ignored(self):
        package = self.dependency(license_file="missing-custom.txt")
        with self.assertRaises(FileNotFoundError):
            self.payload()
        outside = self.root / "outside-terms.txt"
        write(outside, b"outside package")
        package["license_file"] = str(outside)
        with self.assertRaisesRegex(NOTICES.NoticeError, "escapes package"):
            self.payload()

    def test_mit_fallback_requires_a_plain_mit_only_license_choice(self):
        for expression in (
            "MIT", "MIT OR Apache-2.0", "Apache-2.0/MIT", "MIT/Apache-2.0",
            "mit", "mIt/Apache-2.0", "(mit)OR(Apache-2.0)",
            "MIT AND MIT", "(MIT OR Apache-2.0) AND MIT",
            "(MIT/Apache-2.0) AND (BSD-2-Clause OR MIT)",
            "MIT OR Apache-2.0 AND BSD-2-Clause",
            "(MIT AND Apache-2.0) OR MIT",
            "Apache-2.0 WITH LLVM-exception OR MIT",
        ):
            self.assertTrue(NOTICES.permits_mit_only(expression), expression)
        for expression in (
            None, "", "MIT-0", "mit-0", "LicenseRef-MIT",
            "DocumentRef-X:LicenseRef-MIT", "LicenseRef-mit",
            "MIT-advertising", "Apache-2.0 WITH MIT", "MIT WITH Custom-exception",
            "MIT AND Apache-2.0", "(BSD-2-Clause AND MIT)",
            "Apache-2.0 OR MIT AND BSD-2-Clause",
            "(MIT OR Apache-2.0) AND BSD-2-Clause",
            "MIT AND (Apache-2.0 OR BSD-2-Clause)",
            "MIT OR", "MIT AND (Apache-2.0", "MIT Apache-2.0", "MIT; BSD",
            "MIT//Apache-2.0", "MIT/", "/MIT", "MIT OR / Apache-2.0",
            "MIT/Apache-2.0 AND", "\x1b[31mMIT\x1b[0m",
            "MI+T OR MIT", "MIT++ OR MIT", "MIT+OR MIT",
            "MIT_WITH_foo OR MIT", "MIT WITH(Apache-2.0) OR MIT",
            "MIT or Apache-2.0", "MIT And MIT", "MIT with Custom-exception",
            "MIT+AND MIT", "MIT ANDMIT", "MIT OR (Apache-2.0",
            "MIT OR Apache-2.0 garbage",
        ):
            self.assertFalse(NOTICES.permits_mit_only(expression), expression)
        package = self.dependency(license="Apache-2.0 OR MIT")
        payload = self.payload()
        generated = payload["crates/linked-1.2.3/LICENSE-MIT"]
        self.assertIn(b"Permission is hereby granted", generated)
        self.assertNotIn(b"Copyright (c)", generated)
        package["license"] = "MIT-0"
        with self.assertRaisesRegex(NOTICES.NoticeError, "no plain MIT-only"):
            self.payload()

    def test_actual_fxhash_legacy_slash_metadata_gets_only_mit_fallback(self):
        package = self.dependency("fxhash", "0.2.1", license="Apache-2.0/MIT")
        self.assertIsNone(package["license_file"])
        self.assertEqual(list(Path(package["manifest_path"]).parent.iterdir()),
            [Path(package["manifest_path"])])
        payload = self.payload()
        prefix = "crates/fxhash-0.2.1/"
        self.assertEqual([path for path in payload if path.startswith(prefix)],
            [prefix + "LICENSE-MIT"])
        self.assertIn(
            b"fxhash 0.2.1: Apache-2.0/MIT (https://crates.io/crates/fxhash/0.2.1)\n",
            payload["crates.txt"],
        )
        self.assertEqual(package["license"], "Apache-2.0/MIT")
        self.assertNotIn(b"Copyright (c)", payload[prefix + "LICENSE-MIT"])

    def test_and_obligations_do_not_get_incomplete_generated_notices(self):
        package = self.dependency(license="MIT AND Apache-2.0")
        for expression in (
            "MIT AND Apache-2.0", "MIT AND (Apache-2.0 OR BSD-2-Clause)",
            "(MIT/Apache-2.0) AND BSD-2-Clause", "Apache-2.0", None,
        ):
            package["license"] = expression
            with self.subTest(expression=expression):
                with self.assertRaisesRegex(NOTICES.NoticeError, "no plain MIT-only"):
                    self.payload()

    def test_six_pinned_sources_keep_six_attributions_and_exact_upstream_bytes(self):
        names = (
            "mitex", "mitex-glob", "mitex-lexer", "mitex-parser",
            "mitex-spec", "mitex-spec-gen",
        )
        packages, _, notice = self.mitex_fixture(names)
        payload = self.payload()
        for package in packages:
            name = package["name"]
            self.assertEqual(payload[f"crates/{name}-0.2.4/LICENSE"], notice)
            self.assertIn(
                f"{name} 0.2.4: Apache-2.0 (https://crates.io/crates/{name}/0.2.4)\n".encode(),
                payload["crates.txt"],
            )
        self.assertFalse(any(path.endswith("LICENSE-MIT") for path in payload))
        self.assertEqual(payload[f"assets/{NOTICES.MITEX_NOTICE}"], notice)

    def test_roman_release_notice_keeps_its_exact_bytes_and_declared_alternatives(self):
        _, _, notices = self.pinned_fixture(
            "roman-numerals-rs", ("roman-numerals-rs",), "3.1.0"
        )
        payload = self.payload()
        self.assertEqual(payload["crates/roman-numerals-rs-3.1.0/LICENCE.rst"],
            notices["LICENCE.rst"])
        self.assertIn(
            b"roman-numerals-rs 3.1.0: 0BSD OR CC0-1.0 (https://crates.io/crates/roman-numerals-rs/3.1.0)\n",
            payload["crates.txt"],
        )
        self.assertFalse(any(path.endswith("LICENSE-MIT") for path in payload))

    def test_selectors_source_notice_requires_the_exact_full_archive_attribution(self):
        _, _, notices = self.pinned_fixture("selectors", ("selectors",), "0.41.0")
        payload = self.payload()
        for name, raw in notices.items():
            self.assertEqual(payload[f"crates/selectors-0.41.0/{name}"], raw)
        expected = (
            b"selectors 0.41.0: MPL-2.0 "
            b"(https://static.crates.io/crates/selectors/selectors-0.41.0.crate)\n"
        )
        self.assertIn(expected, payload["crates.txt"])
        pin = NOTICES.PINNED_PACKAGES[("selectors", "0.41.0")]
        for changed in (None, "https://crates.io/crates/selectors/0.41.0"):
            pin["source_url"] = changed
            with self.subTest(source_url=changed):
                with self.assertRaisesRegex(NOTICES.NoticeError, "complete source archive attribution"):
                    self.payload()

    def test_all_thirteen_typst_attributions_require_both_license_and_notice(self):
        names = (
            "typst", "typst-eval", "typst-html", "typst-kit", "typst-layout",
            "typst-library", "typst-pdf", "typst-realize", "typst-render",
            "typst-svg", "typst-syntax", "typst-timing", "typst-utils",
        )
        _, _, notices = self.pinned_fixture("typst", names, "0.15.1")
        payload = self.payload()
        for name in names:
            prefix = f"crates/{name}-0.15.1/"
            self.assertEqual(payload[prefix + "LICENSE"], notices["LICENSE"])
            self.assertEqual(payload[prefix + "NOTICE"], notices["NOTICE"])
            self.assertIn(
                f"{name} 0.15.1: Apache-2.0 (https://crates.io/crates/{name}/0.15.1)\n".encode(),
                payload["crates.txt"],
            )
        source = NOTICES.PINNED_NOTICE_GROUPS["typst"]["files"][1]
        (NOTICES.RELEASE_NOTICE_ROOT / source["path"]).unlink()
        with self.assertRaisesRegex(NOTICES.NoticeError, "release notice source is missing"):
            self.payload()

    def test_release_notice_hash_presence_and_canonical_confinement_are_required(self):
        packages, _, notices = self.pinned_fixture(
            "roman-numerals-rs", ("roman-numerals-rs",), "3.1.0"
        )
        source = NOTICES.PINNED_NOTICE_GROUPS["roman-numerals-rs"]["files"][0]
        path = NOTICES.RELEASE_NOTICE_ROOT / source["path"]
        raw = notices["LICENCE.rst"]
        path.write_bytes(raw + b"changed")
        with self.assertRaisesRegex(NOTICES.NoticeError, "release notice checksum changed"):
            NOTICES.pinned_crate_notices(packages[0], {}, self.workspace)
        path.unlink()
        with self.assertRaisesRegex(NOTICES.NoticeError, "release notice source is missing"):
            NOTICES.pinned_crate_notices(packages[0], {}, self.workspace)
        path.write_bytes(raw)
        original = source["path"]
        source["path"] = "../outside-notice"
        with self.assertRaisesRegex(NOTICES.NoticeError, "release notice path is not confined"):
            NOTICES.pinned_crate_notices(packages[0], {}, self.workspace)
        source["path"] = original
        outside = self.root / "outside-release-source"
        outside.mkdir()
        (outside / path.name).write_bytes(raw)
        held = self.root / "held-release-source"
        path.parent.rename(held)
        try:
            path.parent.symlink_to(outside, target_is_directory=True)
        except (OSError, NotImplementedError):
            self.skipTest("symlink creation unavailable")
        with self.assertRaisesRegex(NOTICES.NoticeError, "escapes helper checkout"):
            NOTICES.pinned_crate_notices(packages[0], {}, self.workspace)

    def test_pinned_metadata_guards_reject_each_changed_declaration(self):
        packages, _, notice = self.mitex_fixture()
        package = packages[0]
        for field, value in (
            ("source", "registry+https://example.test/index"),
            ("repository", "https://github.com/other/mitex"),
            ("license", "MIT"), ("license_file", "other-license.txt"),
        ):
            changed = dict(package, **{field: value})
            with self.subTest(field=field):
                with self.assertRaisesRegex(NOTICES.NoticeError, "pinned mitex notice guard"):
                    NOTICES.pinned_crate_notices(
                        changed, {NOTICES.MITEX_NOTICE: notice}, self.workspace
                    )
        for changed in (
            dict(package, name="mitex-other"), dict(package, version="0.2.5"),
        ):
            with self.assertRaisesRegex(NOTICES.NoticeError, "no plain MIT-only"):
                NOTICES.crate_notices(changed, {}, self.workspace)

    def test_pinned_locked_checksum_and_cached_archive_are_independent_guards(self):
        packages, archives, notice = self.mitex_fixture()
        package = packages[0]
        lock = self.workspace / "Cargo.lock"
        original = lock.read_bytes()
        checksum = NOTICES.PINNED_PACKAGES[("mitex", "0.2.4")]["checksum"]
        lock.write_bytes(original.replace(checksum.encode(), b"0" * 64))
        with self.assertRaisesRegex(NOTICES.NoticeError, "locked checksum changed"):
            NOTICES.pinned_crate_notices(
                package, {NOTICES.MITEX_NOTICE: notice}, self.workspace
            )
        lock.write_bytes(original)
        archive = archives["mitex"]
        archive.write_bytes(archive.read_bytes() + b"changed")
        with self.assertRaisesRegex(NOTICES.NoticeError, "cached crate checksum changed"):
            NOTICES.pinned_crate_notices(
                package, {NOTICES.MITEX_NOTICE: notice}, self.workspace
            )
        archive.unlink()
        with self.assertRaisesRegex(NOTICES.NoticeError, "cached crate archive is missing"):
            NOTICES.pinned_crate_notices(
                package, {NOTICES.MITEX_NOTICE: notice}, self.workspace
            )

    def test_pinned_source_vcs_record_must_match_the_verified_archive(self):
        packages, _, notice = self.mitex_fixture()
        package = packages[0]
        vcs = Path(package["manifest_path"]).parent / ".cargo_vcs_info.json"
        vcs.write_bytes(vcs.read_bytes() + b" ")
        with self.assertRaisesRegex(NOTICES.NoticeError, "source VCS record differs"):
            NOTICES.pinned_crate_notices(
                package, {NOTICES.MITEX_NOTICE: notice}, self.workspace
            )
        vcs.unlink()
        with self.assertRaisesRegex(NOTICES.NoticeError, "source VCS record is missing"):
            NOTICES.pinned_crate_notices(
                package, {NOTICES.MITEX_NOTICE: notice}, self.workspace
            )

    def test_pinned_lock_identity_and_registry_layout_must_be_exact(self):
        packages, _, notice = self.mitex_fixture()
        lock = self.workspace / "Cargo.lock"
        original = lock.read_text()
        duplicated = original + "\n[[package]]" + original.split("[[package]]", 1)[1]
        for changed in ("version = 4\n", duplicated):
            lock.write_text(changed)
            with self.assertRaisesRegex(NOTICES.NoticeError, "identity missing or ambiguous"):
                NOTICES.pinned_crate_notices(
                    packages[0], {NOTICES.MITEX_NOTICE: notice}, self.workspace
                )
        lock.write_text(original)
        changed = dict(packages[0], manifest_path=str(self.workspace / "Cargo.toml"))
        with self.assertRaisesRegex(NOTICES.NoticeError, "unsupported registry source/cache layout"):
            NOTICES.pinned_crate_notices(
                changed, {NOTICES.MITEX_NOTICE: notice}, self.workspace
            )

    def test_pinned_vcs_revision_path_and_dirty_state_are_independent_guards(self):
        changes = (
            (lambda vcs: vcs["git"].update(sha1="0" * 40), "revision changed"),
            (lambda vcs: vcs.update(path_in_vcs="crates/other"), "path changed"),
            (lambda vcs: vcs["git"].update(dirty=True), "record is dirty"),
        )
        for change, message in changes:
            with self.subTest(message=message):
                packages, _, notice = self.mitex_fixture(vcs_change=change)
                with self.assertRaisesRegex(NOTICES.NoticeError, message):
                    NOTICES.pinned_crate_notices(
                        packages[0], {NOTICES.MITEX_NOTICE: notice}, self.workspace
                    )

    def test_pinned_archive_vcs_member_is_exact_and_regular(self):
        for kind in ("missing", "duplicate", "symlink"):
            with self.subTest(kind=kind):
                packages, _, notice = self.mitex_fixture(member=kind)
                with self.assertRaisesRegex(NOTICES.NoticeError, "VCS record is missing, duplicate or non-regular"):
                    NOTICES.pinned_crate_notices(
                        packages[0], {NOTICES.MITEX_NOTICE: notice}, self.workspace
                    )

    def test_pinned_asset_notice_must_exist_and_match_its_verified_hash(self):
        packages, _, notice = self.mitex_fixture()
        for embedded, message in (
            ({}, "embedded notice is missing"),
            ({NOTICES.MITEX_NOTICE: notice + b"changed"}, "notice checksum changed"),
        ):
            with self.subTest(message=message):
                with self.assertRaisesRegex(NOTICES.NoticeError, message):
                    NOTICES.pinned_crate_notices(packages[0], embedded, self.workspace)

    def test_unsupported_source_or_multiline_license_is_explicit_failure(self):
        package = self.dependency()
        for source in (
            "git+https://example.test/crate#1234",
            "registry+https://example.test/index",
        ):
            package["source"] = source
            with self.assertRaisesRegex(NOTICES.NoticeError, "unsupported package source"):
                self.payload()
        package["source"] = next(iter(NOTICES.CRATES_IO))
        package["license"] = "MIT\nextra"
        with self.assertRaisesRegex(NOTICES.NoticeError, "multiline license"):
            self.payload()

    def test_existing_file_directory_and_dangling_symlink_are_never_reused(self):
        payload = {"EMBEDDED.txt": b"notice"}
        file = self.root / "existing-file"
        write(file, b"sentinel")
        directory = self.root / "existing-directory"
        directory.mkdir()
        write(directory / "sentinel", b"keep")
        for out in (file, directory):
            with self.assertRaisesRegex(NOTICES.NoticeError, "already exists"):
                NOTICES.write_fresh(out, payload)
        self.assertEqual(file.read_bytes(), b"sentinel")
        self.assertEqual((directory / "sentinel").read_bytes(), b"keep")
        link = self.root / "dangling-output"
        try:
            link.symlink_to(self.root / "absent")
        except (OSError, NotImplementedError):
            self.skipTest("symlink creation unavailable")
        with self.assertRaisesRegex(NOTICES.NoticeError, "already exists"):
            NOTICES.write_fresh(link, payload)
        self.assertTrue(link.is_symlink())

    def test_atomic_output_claim_preserves_a_racing_existing_directory(self):
        out = self.root / "new-output"
        original = Path.mkdir

        def raced(path, *args, **kwargs):
            if path == out:
                original(path)
                (path / "sentinel").write_bytes(b"other writer")
                raise FileExistsError("another invocation claimed output")
            return original(path, *args, **kwargs)

        with patch.object(Path, "mkdir", raced):
            with self.assertRaises(FileExistsError):
                NOTICES.write_fresh(out, {"EMBEDDED.txt": b"notice"})
        self.assertEqual((out / "sentinel").read_bytes(), b"other writer")
        self.assertEqual(list(self.root.glob(".writer-notices-*")), [])

    def test_binary_is_invoked_once_and_output_is_a_complete_fresh_notice_tree(self):
        metadata = self.root / "metadata.json"
        tree = self.root / "tree.txt"
        write(metadata, json.dumps(self.metadata).encode())
        write(tree, self.tree.encode())
        out = self.root / "THIRD-PARTY-LICENSES"
        stdout = self.stdout()
        result = subprocess.CompletedProcess([], 0, stdout, b"")
        with patch.object(NOTICES.subprocess, "run", return_value=result) as run:
            NOTICES.assemble(
                self.binary, "x86_64-unknown-linux-gnu", metadata, tree,
                self.assets, out,
            )
        run.assert_called_once_with(
            [str(self.binary.resolve()), "--licenses"],
            capture_output=True, timeout=60, check=False,
        )
        actual = {
            path.relative_to(out).as_posix(): path.read_bytes()
            for path in out.rglob("*") if path.is_file()
        }
        self.assertEqual(actual, self.payload(stdout))
        self.assertEqual(list(self.root.glob(".writer-notices-*")), [])

    def test_binary_failure_timeout_and_drift_leave_no_output(self):
        metadata = self.root / "metadata.json"
        tree = self.root / "tree.txt"
        write(metadata, json.dumps(self.metadata).encode())
        write(tree, self.tree.encode())
        out = self.root / "THIRD-PARTY-LICENSES"
        for result in (
            subprocess.CompletedProcess([], 1, b"", b"failure"),
            subprocess.CompletedProcess([], 0, self.stdout(), b"unexpected"),
            subprocess.CompletedProcess([], 0, b"drift", b""),
        ):
            with patch.object(NOTICES.subprocess, "run", return_value=result):
                with self.assertRaises(NOTICES.NoticeError):
                    NOTICES.assemble(
                        self.binary, "x86_64-unknown-linux-gnu", metadata, tree,
                        self.assets, out,
                    )
            self.assertFalse(out.exists())
        with patch.object(NOTICES.subprocess, "run", side_effect=
            subprocess.TimeoutExpired("writer --licenses", 60)
        ):
            with self.assertRaises(subprocess.TimeoutExpired):
                NOTICES.assemble(
                    self.binary, "x86_64-unknown-linux-gnu", metadata, tree,
                    self.assets, out,
                )
        self.assertFalse(out.exists())

    def test_outside_package_symlink_notice_is_not_copied(self):
        package = self.dependency()
        root = Path(package["manifest_path"]).parent
        outside = self.root / "outside-LICENSE"
        write(outside, b"outside")
        try:
            (root / "LICENSE").symlink_to(outside)
        except (OSError, NotImplementedError):
            self.skipTest("symlink creation unavailable")
        with self.assertRaisesRegex(NOTICES.NoticeError, "symlinked"):
            self.payload()


if __name__ == "__main__":
    unittest.main()
