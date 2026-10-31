"""Pure site mapping/HTML/link tests; no Writer, compiler or network calls."""

import base64
import importlib.util
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "site.py"
SPEC = importlib.util.spec_from_file_location("writer_site", SCRIPT)
site = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = site
SPEC.loader.exec_module(site)

SHA = "0123456789abcdef" * 2 + "01234567"
REPO = "https://github.com/example/writer"


class SiteTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.source = self.root / "source"
        self.output = self.root / "artifact"
        self.source.mkdir()
        self.output.mkdir()

    def write(self, relative, text, root=None):
        path = (root or self.source) / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path.resolve()

    def mapper(self, prefix=""):
        home = self.write("docs/site/index.html", "<!DOCTYPE html>\n<p>Home</p>\n")
        guide = self.write("docs/guide/README.md", "# Guide\n")
        authoring = self.write("docs/guide/authoring.md", "# Authoring\n")
        pages = [site.Page("home", home, "index.html"),
            site.Page("guide", guide, "guide/index.html"),
            site.Page("guide-authoring", authoring, "guide/authoring.html")]
        return site.Mapper(self.source, self.output, pages, REPO, SHA, prefix), pages

    def test_rewriter_preserves_code_scripts_entities_and_tag_layout(self):
        original = '''<!DOCTYPE html>\n<a HREF = 'page.md#one' data-note="&amp;">go</a>
<pre><code>&lt;a href="page.md#one"&gt; [x](page.md)</code></pre>
<script>const sample = 'href="page.md#one"';</script>
<img src="pic.png" alt="literal page.md &amp; pi">
'''
        calls = []

        def rewrite(tag, attr, value):
            calls.append((tag, attr, value))
            return {"page.md#one": "next.html?a=1&b=2#one", "pic.png": "assets/pic.png"}[value]

        changed = site.AttributeRewriter(original, rewrite).result()
        expected = original.replace("HREF = 'page.md#one'", "HREF = 'next.html?a=1&amp;b=2#one'")
        expected = expected.replace('src="pic.png"', 'src="assets/pic.png"')
        self.assertEqual(changed, expected)
        self.assertEqual(calls, [("a", "href", "page.md#one"), ("img", "src", "pic.png")])

    def test_multiline_unquoted_and_srcset_values_are_actual_attributes(self):
        original = '<a\n href=one.md>one</a><img srcset="data:image/png;base64,AAAA 1x, two.png 2x">'
        changed = site.AttributeRewriter(original, lambda tag, attr, value:
            value if value.startswith("data:") else "mapped/" + value).result()
        self.assertEqual(changed,
            '<a\n href=mapped/one.md>one</a><img srcset="data:image/png;base64,AAAA 1x, mapped/two.png 2x">')

    def test_guide_links_map_locally_and_source_links_use_supplied_revision(self):
        mapper, pages = self.mapper()
        page = pages[2]
        self.write("docs/spec/config.md", "# Configuration\n")
        self.assertEqual(mapper.rewrite(page, "a", "href", "README.md"), "index.html")
        self.assertEqual(mapper.rewrite(page, "a", "href", "authoring.md#notes"), "#notes")
        self.assertEqual(mapper.rewrite(page, "a", "href", "../spec/config.md#paths"),
            f"{REPO}/blob/{SHA}/docs/spec/config.md#paths")
        self.assertEqual(mapper.rewrite(page, "a", "href", "writer-site:home"), "../index.html")
        self.assertEqual(mapper.rewrite(page, "a", "href", "writer-site:source"),
            f"{REPO}/blob/{SHA}/docs/guide/authoring.md")

    def test_source_bytes_are_untouched_and_local_asset_is_copied(self):
        mapper, pages = self.mapper()
        image = self.source / "docs/guide/plot one.png"
        image.write_bytes(b"\x89PNG\r\n\x1a\nfixture")
        original = pages[2].source.read_bytes()
        result = mapper.rewrite(pages[2], "img", "src", "plot%20one.png")
        self.assertEqual(result, "../assets/source/docs/guide/plot%20one.png")
        self.assertEqual((self.output / "assets/source/docs/guide/plot one.png").read_bytes(), image.read_bytes())
        self.assertEqual(pages[2].source.read_bytes(), original)

    def test_prefixes_are_portable_and_do_not_assume_a_hostname(self):
        mapper, pages = self.mapper("reports/v1")
        self.assertEqual(mapper.rewrite(pages[2], "a", "href", "writer-site:guide"),
            "/reports/v1/guide/index.html")
        self.assertEqual(site.normalize_prefix("/reports/v1/"), "/reports/v1/")
        self.assertEqual(site.normalize_prefix(""), "")
        self.assertEqual(site.normalize_prefix("/"), "/")
        self.assertEqual(site.artifact_target("guide/authoring.html", "../index.html", "/reports/v1/"),
            ("index.html", ""))
        self.assertEqual(site.artifact_target("guide/authoring.html", "/reports/v1/guide/index.html#one", "/reports/v1/"),
            ("guide/index.html", "one"))
        for prefix in ("https://site.invalid/docs", "//site.invalid/docs", "a/../b", "a//b", "a%2fb", "docs?x=1"):
            with self.subTest(prefix=prefix), self.assertRaises(site.SiteError):
                site.normalize_prefix(prefix)

    def test_escaping_prefix_unknown_schemes_and_case_collisions_fail(self):
        for value, prefix in (("../../outside.html", ""), ("/guide/index.html", ""),
                ("/other/index.html", "/reports/"), ("/reports/../outside.html", "/reports/"),
                ("%2e%2e/%2e%2e/outside.html", "/reports/"), ("asset:unresolved", "")):
            with self.subTest(value=value), self.assertRaises(site.SiteError):
                site.artifact_target("guide/index.html", value, prefix)
        first = self.write("one.md", "# One")
        second = self.write("two.md", "# Two")
        with self.assertRaisesRegex(site.SiteError, "case-insensitive"):
            site.Mapper(self.source, self.output,
                [site.Page("one", first, "Guide.html"), site.Page("two", second, "guide.html")], REPO, SHA, "")

    def test_all_local_links_assets_svg_and_fragments_are_validated(self):
        self.write("index.html", '<a href="guide/page.html#one">guide</a><img src="assets/pixel.png">', self.output)
        self.write("guide/page.html", '<h1 id="one">One</h1><a href="../index.html">home</a><svg><use href="../assets/shape.svg#mark"></use></svg>', self.output)
        self.write("assets/shape.svg", '<svg><g id="mark"></g></svg>', self.output)
        pixel = self.output / "assets/pixel.png"
        pixel.write_bytes(b"png fixture")
        site.validate_site(self.output, "")
        pixel.unlink()
        with self.assertRaisesRegex(site.SiteError, "missing local target"):
            site.validate_site(self.output, "")
        pixel.write_bytes(b"restored")
        self.write("assets/shape.svg", '<svg><g id="different"></g></svg>', self.output)
        with self.assertRaisesRegex(site.SiteError, "missing local fragment"):
            site.validate_site(self.output, "")

    def test_prefixed_site_duplicate_ids_css_and_path_leaks_are_checked(self):
        self.write("index.html", '<a href="/docs/guide/page.html#one">guide</a>', self.output)
        self.write("guide/page.html", '<h1 id="one">One</h1>', self.output)
        site.validate_site(self.output, "/docs/")
        self.write("guide/page.html", '<h1 id="one">One</h1><p id="one">duplicate</p>', self.output)
        with self.assertRaisesRegex(site.SiteError, "duplicate ids"):
            site.validate_site(self.output, "/docs/")
        self.write("guide/page.html", '<h1 id="one">One</h1><style>body{background:url(missing.png)}</style>', self.output)
        with self.assertRaisesRegex(site.SiteError, "missing local target"):
            site.validate_site(self.output, "/docs/")
        self.write("guide/page.html", f'<h1 id="one">One</h1><pre>{self.source}</pre>', self.output)
        with self.assertRaisesRegex(site.SiteError, "path leaked"):
            site.validate_site(self.output, "/docs/", (self.source,))

    def test_unfinished_guide_is_refused_before_any_writer_call(self):
        for name in site.GUIDE_PAGES:
            self.write(f"docs/guide/{name}.md", "# Finished prose\n")
        self.write("docs/guide/cli.md", "# CLI\n```text\n$ writer --help\n```\n")
        self.write("docs/guide/configuration.md", "# Config\n<!-- generated: writer dump --default-config -->\n```toml\n[document]\nlang = \"en\"\n```\n")
        with self.assertRaisesRegex(site.SiteError, "help fences"):
            site.require_complete_guide(self.source)
        self.write("docs/guide/cli.md", "# CLI\n```text\n$ writer --help\nUsage: writer\n```\n")
        site.require_complete_guide(self.source)
        self.write("docs/guide/typst.md", "# Typst\nPending T74\n")
        with self.assertRaisesRegex(site.SiteError, "milestone"):
            site.require_complete_guide(self.source)

    def test_immutable_source_revision_and_missing_source_are_required(self):
        for invalid in ("main", "v0.1.0", "abc123", "g" * 40):
            with self.subTest(invalid=invalid), self.assertRaises(site.SiteError):
                site.revision(invalid)
        mapper, pages = self.mapper()
        with self.assertRaisesRegex(site.SiteError, "missing or outside"):
            mapper.rewrite(pages[2], "a", "href", "absent.md")
        with self.assertRaisesRegex(site.SiteError, "unknown site navigation"):
            mapper.rewrite(pages[2], "a", "href", "writer-site:unknown")

    def test_unicode_space_prefix_and_emitted_bytes_survive_the_full_build(self):
        _, pages = self.mapper()
        self.write("docs/site/theme/theme.toml", 'extends = "default"\n')
        self.write("docs/site/site.css", "body { color: black; }\n")
        self.write("LICENSE", "Project license\r\n")
        binary = self.write("writer", "mocked executable\n", self.root)
        destination = self.root / "published-site"
        args = SimpleNamespace(
            source=self.source, binary=binary, out=destination,
            repo_url=REPO, revision=SHA, prefix="résumé docs",
            release_json=None, gs=None,
        )
        originals = {
            path: path.read_bytes() for path in self.source.rglob("*")
            if path.is_file()
        }
        emitted = (
            b'<a href="writer-site:home" data-note="&amp;">Home</a>\r\n'
            b'<pre><code>&lt;a href="writer-site:home"&gt;literal</code></pre>\r\n'
            b'<script>const sample = \'href="writer-site:home"\';</script>\r\n'
        )
        landing = (
            '<link rel="stylesheet" href="writer-site:artifact/site.css">'
            '<a href="writer-site:guide">Guide</a>'
        )

        def command(_binary, arguments, _root, _environment):
            if arguments == ["--licenses"]:
                return b"Embedded notices\r\n"
            if arguments == ["--version"]:
                return b"writer 0.1.0 (mocked)\n"
            Path(arguments[-1]).write_bytes(emitted)
            return b""

        with patch.object(site, "require_complete_guide"):
            with patch.object(site, "pages", return_value=pages):
                with patch.object(site, "command", side_effect=command):
                    with patch.object(site, "landing_page", return_value=landing):
                        site.build(args)
        manifest = site.json.loads((destination / "site-manifest.json").read_text())
        self.assertEqual(manifest["prefix"], "/r%C3%A9sum%C3%A9%20docs/")
        self.assertEqual(manifest["downloads"], [])
        self.assertIsNone(manifest["release"])
        expected = emitted.replace(
            b'href="writer-site:home"',
            b'href="/r%C3%A9sum%C3%A9%20docs/index.html"', 1,
        )
        for page in pages[1:]:
            self.assertEqual((destination / page.route).read_bytes(), expected)
        self.assertEqual((destination / "index.html").read_text(), (
            '<link rel="stylesheet" href="/r%C3%A9sum%C3%A9%20docs/site.css">'
            '<a href="/r%C3%A9sum%C3%A9%20docs/guide/index.html">Guide</a>'
        ))
        self.assertEqual((destination / ".nojekyll").read_bytes(), b"")
        self.assertEqual((destination / "site.css").read_bytes(),
            (self.source / "docs/site/site.css").read_bytes())
        site.validate_site(destination, args.prefix)
        self.assertEqual(originals, {
            path: path.read_bytes() for path in self.source.rglob("*")
            if path.is_file()
        })

    def test_artifact_links_name_claimed_routes_and_keys_keep_fragments(self):
        mapper, pages = self.mapper()
        route = "examples/basic/document.pdf"
        with self.assertRaisesRegex(site.SiteError, "unknown site artifact"):
            mapper.rewrite(pages[2], "a", "href", f"writer-site:artifact/{route}")
        mapper.claim(route)
        self.assertEqual(mapper.rewrite(pages[2], "a", "href",
            f"writer-site:artifact/{route}"), "../examples/basic/document.pdf")
        self.assertEqual(mapper.rewrite(pages[0], "a", "href",
            f"writer-site:artifact/{route}"), route)
        self.assertEqual(mapper.rewrite(pages[2], "a", "href",
            "writer-site:home#download"), "../index.html#download")
        self.assertEqual(mapper.rewrite(pages[0], "a", "href",
            "writer-site:home#download"), "#download")
        with self.assertRaisesRegex(site.SiteError, "unknown site artifact"):
            mapper.rewrite(pages[2], "a", "href",
                "writer-site:artifact/Examples/basic/document.pdf")

    def test_source_zip_is_reproducible_rooted_and_portable(self):
        export = self.root / "export"
        self.write("document.tex", "\\section{One}\n", export)
        (export / "document_assets").mkdir()
        (export / "document_assets/plot.png").write_bytes(b"\x89PNG fixture")
        first = self.root / "first.zip"
        second = self.root / "second.zip"
        site.pack(export, first, "document-latex", 0)
        site.pack(export, second, "document-latex", 0)
        self.assertEqual(first.read_bytes(), second.read_bytes())
        with site.zipfile.ZipFile(first) as archive:
            self.assertIsNone(archive.testzip())
            entries = archive.infolist()
        self.assertEqual([entry.filename for entry in entries], [
            "document-latex/document.tex",
            "document-latex/document_assets/plot.png",
        ])
        for entry in entries:
            self.assertEqual(entry.date_time, (1980, 1, 1, 0, 0, 0))
            self.assertEqual(entry.external_attr >> 16, 0o100644)
        with self.assertRaises(FileExistsError):
            site.pack(export, first, "document-latex", 0)

    def test_download_table_follows_release_targets_and_platform_order(self):
        common = importlib.util.spec_from_file_location(
            "release_common", SCRIPT.parent / "release_common.py"
        )
        release_common = importlib.util.module_from_spec(common)
        common.loader.exec_module(release_common)
        self.assertEqual([target for target, _, _ in site.PLATFORMS],
            list(release_common.TARGETS))
        base = "https://github.com/example/writer/releases/download/v0.2.0/"
        names = [release_common.archive_name("0.2.0", target)
            for target in reversed(release_common.TARGETS)]
        release = {
            "tag_name": "v0.2.0", "published_at": "2026-11-02T09:00:00Z",
            "html_url": "https://github.com/example/writer/releases/tag/v0.2.0",
            "assets": [
                {"name": name, "size": 28_068_371, "browser_download_url": base + name}
                for name in [*names, "SHA256SUMS", "writer-v0.1.0-x86_64-apple-darwin.tar.gz"]
            ],
        }
        table = site.download_section(release, REPO)
        rows = site.re.findall(r"<code>(writer-[^<]+)</code>", table)
        self.assertEqual(rows, list(reversed(names)))
        self.assertIn("writer v0.2.0</a></strong>, published 2026-11-02.", table)
        self.assertIn(f'href="{base}SHA256SUMS"', table)
        self.assertIn("28.1 MB", table)
        for missing in (None, {"tag_name": "v0.2.0", "assets": []}):
            with self.subTest(release=missing):
                text = site.download_section(missing, REPO)
                self.assertIn("No release has been published yet", text)
                self.assertIn(f'href="{REPO}/releases"', text)

    def test_landing_template_fills_and_validates_with_example_outputs(self):
        template = SCRIPT.parents[1] / "docs/site/index.html"
        home = self.write("docs/site/index.html", template.read_text(encoding="utf-8"))
        guides = []
        for name in site.GUIDE_PAGES:
            key = "guide" if name == "README" else f"guide-{name}"
            leaf = "index" if name == "README" else name
            source = self.write(f"docs/guide/{name}.md", f"# {name.title()}\n")
            guides.append(site.Page(key, source, f"guide/{leaf}.html"))
        basic = self.write("examples/basic/document.md", "---\ntitle: <Basic>\n---\n")
        notebook = self.write("examples/notebook/analysis.ipynb", "{}")
        examples = [
            site.Page("example-basic", basic, "examples/basic/index.html"),
            site.Page("example-notebook", notebook, "examples/notebook/index.html"),
        ]
        all_pages = [site.Page("home", home, "index.html"), *guides, *examples]
        mapper = site.Mapper(self.source, self.output, all_pages, REPO, SHA, "")
        built = []
        for page in examples:
            folder = site.posixpath.dirname(page.route)
            stem = page.source.stem
            outputs = [site.Output("html", page.route, 690_300)]
            for route in (f"{stem}.pdf", f"{stem}.docx", f"{stem}-latex.zip", f"{stem}-typst.zip"):
                mapper.claim(f"{folder}/{route}")
                target = route.removesuffix(".zip").rpartition("-")[2] if route.endswith(".zip") else route.rpartition(".")[2]
                outputs.append(site.Output(target, f"{folder}/{route}", 50_800, 3 if target == "pdf" else None))
            mapper.claim(f"{folder}/page-1.png")
            built.append(site.Example(page, f"{stem} & title", tuple(outputs), f"{folder}/page-1.png", (827, 1169)))
        for page in all_pages[1:]:
            self.write(page.route, "<h1>Page</h1>", self.output)
        for example in built:
            for output in example.outputs[1:]:
                self.write(output.route, "built", self.output)
            self.write(example.thumbnail, "png", self.output)
        for route in ("site.css", "licenses/LICENSE", "licenses/embedded.txt"):
            self.write(route, "", self.output)
        text = site.landing_page(self.source, all_pages[0], built, mapper, None, "writer 0.1.0")
        self.assertNotIn("${", text)
        self.assertIn("examples/basic/document.md", text)
        self.assertIn("&lt;Basic&gt;", text)
        self.assertIn("3 pages, 50.8 kB", text)
        self.assertIn("document &amp; title", text)
        self.assertIn("No release has been published yet", text)
        site.write_page(self.output, mapper, all_pages[0], text)
        result = (self.output / "index.html").read_text(encoding="utf-8")
        self.assertNotIn('="writer-site:', result)
        self.assertIn('href="examples/basic/document.pdf"', result)
        self.assertIn(f'href="{REPO}/blob/{SHA}/examples/basic/document.md"', result)
        site.validate_site(self.output, "")
        with self.assertRaisesRegex(site.SiteError, "basic example"):
            site.landing_page(self.source, all_pages[0], built[1:], mapper, None, "writer")

    def test_fresh_publication_preserves_a_racing_directory(self):
        artifact = self.root / "prepared"
        artifact.mkdir()
        (artifact / "index.html").write_bytes(b"validated artifact")
        destination = self.root / "published-site"
        original_mkdir = Path.mkdir

        def raced(path, *arguments, **keywords):
            if path == destination:
                original_mkdir(path)
                (path / "sentinel").write_bytes(b"concurrent owner")
                raise FileExistsError("output was claimed concurrently")
            return original_mkdir(path, *arguments, **keywords)

        with patch.object(Path, "mkdir", raced):
            with self.assertRaises(FileExistsError):
                site.publish_fresh(artifact, destination)
        self.assertEqual((destination / "sentinel").read_bytes(),
            b"concurrent owner")
        self.assertEqual((artifact / "index.html").read_bytes(),
            b"validated artifact")

    def test_fresh_publication_moves_only_validated_artifact_bytes(self):
        artifact = self.root / "prepared"
        (artifact / "guide").mkdir(parents=True)
        (artifact / "index.html").write_bytes(b"home\r\n")
        (artifact / "guide/page.html").write_bytes(b"guide\n")
        destination = self.root / "published-site"
        site.publish_fresh(artifact, destination)
        self.assertFalse(artifact.exists())
        self.assertEqual((destination / "index.html").read_bytes(), b"home\r\n")
        self.assertEqual((destination / "guide/page.html").read_bytes(), b"guide\n")

    def katex_fixture(self):
        font = b"wOF2known-source-font-fixture"
        uri = "data:font/woff2;base64," + base64.b64encode(font).decode("ascii")
        original = (
            '@font-face{font-display:block;font-family:KaTeX_AMS;'
            'font-style:normal;font-weight:400;'
            'src:url(fonts/KaTeX_AMS-Regular.woff2) format("woff2"),'
            'url(fonts/KaTeX_AMS-Regular.woff) format("woff"),'
            'url(fonts/KaTeX_AMS-Regular.ttf) format("truetype")}'
        )
        self.write("assets/vendor/katex/katex.min.css", original)
        path = self.source / "assets/vendor/katex/fonts/KaTeX_AMS-Regular.woff2"
        path.parent.mkdir(parents=True)
        path.write_bytes(font)
        standalone = original.replace("fonts/KaTeX_AMS-Regular.woff2", uri, 1)
        return original, standalone, uri, path

    def test_only_exact_rule_fallback_occurrences_are_exempted(self):
        _, standalone, _, _ = self.katex_fixture()
        rules = site.katex_legacy_rules(self.source)
        body = f"<style>{standalone}</style>\r\n<p>Exact body</p>\r\n"
        path = self.write("index.html", body, self.output)
        original = path.read_bytes()
        with self.assertRaisesRegex(site.SiteError, "missing local target"):
            site.validate_site(self.output, "")
        site.validate_site(self.output, "", legacy_rules=rules)
        self.assertEqual(path.read_bytes(), original)
        extras = [
            '<style>.user{src:url(fonts/KaTeX_AMS-Regular.woff)}</style>',
            '<img src="fonts/KaTeX_AMS-Regular.woff">',
            '<style>.user{background:url(missing.png)}</style>',
            '<style>.user{src:url(custom.woff)}</style>',
        ]
        for extra in extras:
            with self.subTest(extra=extra):
                path.write_text(body + extra, encoding="utf-8", newline="")
                with self.assertRaisesRegex(site.SiteError, "missing local target"):
                    site.validate_site(self.output, "", legacy_rules=rules)

    def test_fallback_rule_requires_exact_same_face_source_and_order(self):
        original, standalone, uri, _ = self.katex_fixture()
        rules = site.katex_legacy_rules(self.source)
        wrong_face = "data:font/woff2;base64," + base64.b64encode(
            b"wOF2another-source-font"
        ).decode("ascii")
        variants = {
            "missing embedded source": original,
            "invalid embedded source": standalone.replace(uri, "data:font/woff2;base64,AA=="),
            "wrong embedded font": standalone.replace(uri, wrong_face),
            "fallback before embedded source": standalone.replace(
                "src:", 'src:url(fonts/KaTeX_AMS-Regular.woff) format("woff"),', 1
            ),
            "unknown face": standalone.replace("KaTeX_AMS", "KaTeX_Unknown"),
            "changed fallback": standalone.replace("Regular.woff)", "Regular-missing.woff)"),
            "changed rule": standalone.replace("font-weight:400", "font-weight:700"),
            "unpaired fallback": '@font-face{src:url(fonts/KaTeX_AMS-Regular.woff)}',
        }
        for name, css in variants.items():
            with self.subTest(name=name):
                self.write("index.html", f"<style>{css}</style>", self.output)
                with self.assertRaisesRegex(site.SiteError, "missing local target"):
                    site.validate_site(self.output, "", legacy_rules=rules)

    def test_canonical_font_source_must_exist_and_have_woff2_signature(self):
        _, _, _, font = self.katex_fixture()
        font.write_bytes(b"not a WOFF2 file")
        with self.assertRaisesRegex(site.SiteError, "WOFF2 signature"):
            site.katex_legacy_rules(self.source)
        font.unlink()
        with self.assertRaisesRegex(site.SiteError, "missing or outside"):
            site.katex_legacy_rules(self.source)


if __name__ == "__main__":
    unittest.main()
