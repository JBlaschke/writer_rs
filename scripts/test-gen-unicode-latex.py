#!/usr/bin/env python3
"""Stdlib generator tests; optional local-archive and pdflatex acceptance."""

import hashlib
import importlib.util
import io
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parent.parent
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "unicode_latex_generator", ROOT / "scripts/gen-unicode-latex.py"
)
GEN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GEN)
WORK = ROOT / "target/task-prep/unicode-latex-tests"
WORK.mkdir(parents=True, exist_ok=True)


def temporary():
    return tempfile.TemporaryDirectory(dir=WORK)


class PureTests(unittest.TestCase):
    def test_rust_and_toml_strings_preserve_unicode_controls_and_slashes(self):
        self.assertEqual(GEN.rust_string(r"α\\\"\u{03B2}"), 'α\\"β')
        value = 'α\\"\0\n\r\t\x7f'
        self.assertEqual(tomllib.loads("value = " + GEN.quote(value))["value"], value)
        with self.assertRaises(ValueError):
            GEN.rust_string(r"\q")

    def test_codex_minimum_modifier_ties_and_variation_selectors(self):
        symbols = {"test": [
            (frozenset({"a", "first"}), "x\ufe0e"),
            (frozenset({"a", "later"}), "y"),
            (frozenset({"a"}), "z"),
        ]}
        self.assertEqual(GEN.resolve("test.a", symbols, {}), "z")
        symbols["test"].pop()
        self.assertEqual(GEN.resolve("test.a", symbols, {}), "x")
        self.assertEqual(GEN.resolve("->", {}, {"->": "→"}), "→")
        for path in ["test..a", ".test", "test.", "test.missing"]:
            self.assertIsNone(GEN.resolve(path, symbols, {}))
        self.assertEqual(GEN.codex_value(r"x\vs{text}"), "x\ufe0e")
        self.assertEqual(GEN.codex_value(r"\u{2192}\vs{emoji}"), "→\ufe0f")
        with self.assertRaises(ValueError):
            GEN.codex_value(r"x\vs{17}")

    def test_overrides_reject_non_strings_bad_scalars_and_package_words(self):
        for data in [{"names": {"a": 3}}, {"unknown": {}},
                     {"packages": {r"\foo-bar": "test"}}]:
            with self.assertRaises(ValueError):
                GEN.override_tables(data)
        for key in ["U+xyz", "U+D800", "U+110000"]:
            with self.assertRaises(ValueError):
                GEN.char_key(key)

    def test_output_is_atomic_and_check_does_not_rewrite(self):
        with temporary() as directory:
            output = Path(directory) / "table.toml"
            GEN.write_output(output, "first\n", False)
            stamp = output.stat().st_mtime_ns
            GEN.write_output(output, "first\n", True)
            self.assertEqual(output.stat().st_mtime_ns, stamp)
            with self.assertRaises(ValueError):
                GEN.write_output(output, "drift\n", True)
            self.assertEqual(output.read_text(encoding="utf-8"), "first\n")
            with patch.object(GEN.os, "replace", side_effect=OSError("refused")):
                with self.assertRaises(OSError):
                    GEN.write_output(output, "replacement\n", False)
            self.assertEqual(output.read_text(encoding="utf-8"), "first\n")
            self.assertEqual(list(output.parent.glob(".unicode-latex-*")), [])

    def test_hash_verification_precedes_archive_parsing(self):
        with temporary() as directory:
            root = Path(directory)
            (root / "tylax-0.3.8.crate").write_bytes(b"bad archive")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                GEN.read_sources(root, directory)

    def test_only_exact_regular_archive_members_are_accepted(self):
        with temporary() as directory:
            root = Path(directory)
            buf = io.BytesIO()
            with tarfile.open(fileobj=buf, mode="w:gz") as archive:
                member = tarfile.TarInfo("tiny-1/src/data.txt")
                member.type = tarfile.SYMTYPE
                member.linkname = "../../outside"
                archive.addfile(member)
            data = buf.getvalue()
            (root / "tiny-1.crate").write_bytes(data)
            pins = {"tiny": ("1", hashlib.sha256(data).hexdigest())}
            with patch.dict(GEN.PINS, pins, clear=True):
                with patch.dict(GEN.MEMBERS, {"tiny": ["src/data.txt"]}, clear=True):
                    with self.assertRaisesRegex(ValueError, "invalid archive member"):
                        GEN.read_sources(root, directory)
            self.assertFalse((root.parent / "outside").exists())


class SourceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        configured = os.environ.get("WRITER_UNICODE_ARCHIVE_DIR")
        roots = [Path(configured)] if configured else sorted(
            (Path.home() / ".cargo/registry/cache").glob("*")
        )
        root = next((root for root in roots if all(
            (root / f"{name}-{version}.crate").is_file()
            for name, (version, _) in GEN.PINS.items()
        )), None)
        if root is None:
            raise unittest.SkipTest("skipping pinned source tests: archives unavailable")
        with temporary() as directory:
            cls.sources = GEN.read_sources(root, directory)

    def test_empty_override_counts_and_merge_provenance_match_the_frozen_baseline(self):
        names, chars, report = GEN.generate(self.sources, GEN.override_tables({}))
        self.assertEqual((len(names), len(chars)), (532, 1292))
        self.assertEqual((report["base_chars"], report["alnum"]), (405, 887))
        self.assertEqual((len(report["conflicts"]), len(report["unresolved"])), (17, 87))
        self.assertEqual(chars["𝐀"]["from"], "alnum")
        self.assertEqual(chars["α"]["from"], "chars")

    def test_override_add_replace_delete_and_alnum_deletion(self):
        overrides = GEN.override_tables({
            "names": {"alpha": "A", "added-test": "B", "beta": ""},
            "chars": {"U+1D400": "", "U+2605": r"\bigstar"},
        })
        names, chars, _ = GEN.generate(self.sources, overrides)
        self.assertEqual(names["alpha"]["tex"], "A")
        self.assertEqual(names["added-test"]["tex"], "B")
        self.assertNotIn("beta", names)
        self.assertNotIn("𝐀", chars)
        self.assertEqual(chars["★"]["from"], "override")

    def test_package_detection_uses_maximal_control_words_and_rejects_conflicts(self):
        overrides = GEN.override_tables({
            "names": {"test": r"\fooextra"}, "packages": {r"\foo": "first"},
        })
        names, _, _ = GEN.generate(self.sources, overrides)
        self.assertNotIn("package", names["test"])
        overrides["names"]["test"] = r"\foo\bar"
        overrides["packages"][r"\bar"] = "second"
        with self.assertRaisesRegex(ValueError, "two packages"):
            GEN.generate(self.sources, overrides)

    def test_committed_output_round_trips_and_matches_regeneration(self):
        overrides = GEN.override_tables(tomllib.loads(
            (ROOT / "scripts/unicode-latex-overrides.toml").read_text(encoding="utf-8")
        ))
        names, chars, report = GEN.generate(self.sources, overrides)
        self.assertEqual((len(names), len(chars)), (527, 1296))
        self.assertEqual((report["base_chars"], report["alnum"]), (404, 892))
        self.assertEqual((len(report["conflicts"]), len(report["unresolved"])), (18, 86))
        shorthands = GEN.parse_shorthands(
            self.sources[("typst-syntax", "src/ast.rs")]
        )
        self.assertTrue(all(char in chars for char in shorthands.values()))
        output = GEN.serialize(names, chars)
        self.assertEqual(
            output,
            (ROOT / "assets/math/unicode-latex.toml").read_text(encoding="utf-8")
        )
        data = tomllib.loads(output)
        self.assertEqual(data["names"], names)
        self.assertEqual(len(data["chars"]), len(chars))
        self.assertFalse(any(line.rstrip() != line for line in output.splitlines()))
        self.assertNotIn("\r", output)


class LatexTests(unittest.TestCase):
    def test_every_distinct_value_under_only_its_declared_package(self):
        tool = shutil.which("pdflatex")
        if tool is None:
            self.skipTest("skipping table compilation: pdflatex unavailable")
        table = tomllib.loads(
            (ROOT / "assets/math/unicode-latex.toml").read_text(encoding="utf-8")
        )
        grouped = {}
        for row in [*table["names"].values(), *table["chars"].values()]:
            grouped.setdefault(row.get("package"), set()).add(row["tex"])
        for package, values in grouped.items():
            with self.subTest(package=package), temporary() as directory:
                values = sorted(values)
                lines = [r"\documentclass{article}", r"\usepackage{amsmath,amssymb}"]
                if package:
                    lines.append(r"\usepackage{" + package + "}")
                lines.append(r"\begin{document}")
                for index, tex in enumerate(values):
                    lines.extend([
                        r"\typeout{WRITER-ROW:" + str(index) + "}",
                        "$x " + tex + r" y$\par"
                    ])
                lines.append(r"\end{document}")
                Path(directory, "table.tex").write_text("\n".join(lines) + "\n")
                run = subprocess.run(
                    [tool, "-interaction=nonstopmode", "-no-shell-escape", "table.tex"],
                    cwd=directory, capture_output=True, timeout=60,
                )
                log = (run.stdout + run.stderr).decode("utf-8", "replace")
                if run.returncode:
                    current = None
                    failures = []
                    for line in log.splitlines():
                        if match := re.match(r"WRITER-ROW:(\d+)", line):
                            current = int(match[1])
                        if line.startswith("!"):
                            value = values[current] if current is not None else "preamble"
                            failures.append(f"{value}: {line}")
                    self.fail("\n".join(failures) or log[-4000:])


if __name__ == "__main__":
    unittest.main()
