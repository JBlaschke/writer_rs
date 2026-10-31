#!/usr/bin/env python3
"""Build the project website using an existing writer binary.

The landing page (docs/site/index.html) shows the latest release's downloads
from --release-json, the GitHub API's releases/latest document. The guide and
examples are rendered by writer; every example is also built to PDF, Word,
LaTeX and Typst, and Ghostscript draws the first page of each PDF.

No deployment, network lookup, Git command or guide-source rewrite occurs.
Run the guide drift checks first; unfinished generated fences are refused.
"""

from __future__ import annotations

import argparse
import base64
from dataclasses import dataclass
import html
from html.parser import HTMLParser
import json
import os
from pathlib import Path, PurePosixPath
import posixpath
import re
import shutil
from string import Template
import struct
import subprocess
import tempfile
import time
from urllib.parse import quote, unquote, urlsplit, urlunsplit
import zipfile


GUIDE_PAGES = (
    "README", "getting-started", "authoring", "notebooks", "typst",
    "outputs", "customization", "configuration", "cli", "troubleshooting",
)
EXAMPLES = ("basic", "notebook", "typst")
# What each example card says the example shows; titles come from the sources.
EXAMPLE_NOTES = {
    "basic": (
        "Markdown report",
        "Numbered figures and tables, callouts, highlighted code, equations "
        "and a footnote.",
    ),
    "notebook": (
        "Saved Jupyter notebook",
        "A plot, a DataFrame and a traceback from the notebook's saved "
        "outputs. No kernel runs.",
    ),
    "typst": (
        "Native Typst",
        "The PDF compiles the original source; the web page and the Word "
        "file use its structured subset.",
    ),
}
# The outputs on the landing page: target, label and short name.
FORMATS = (
    ("html", "Web page", "Web"),
    ("pdf", "Typeset PDF", "PDF"),
    ("docx", "Word document", "Word"),
    ("latex", "LaTeX source", "LaTeX"),
    ("typst", "Typst source", "Typst"),
)
# The landing page shows this many lines of the basic example's source.
EXCERPT_LINES = 21
# The release targets (release_common.TARGETS, checked by the tests) in the
# order of the download table: target, platform and what the archive is for.
PLATFORMS = (
    ("aarch64-apple-darwin", "macOS", "Apple silicon"),
    ("x86_64-apple-darwin", "macOS", "Intel"),
    ("x86_64-unknown-linux-gnu", "Linux", "x86_64, glibc"),
    ("aarch64-unknown-linux-gnu", "Linux", "arm64, glibc"),
    ("x86_64-pc-windows-msvc", "Windows", "x64"),
    ("aarch64-pc-windows-msvc", "Windows", "arm64"),
)
# ZIP timestamps start in 1980.
ZIP_EPOCH = 315_532_800
URL_ATTRIBUTES = {
    "href", "src", "poster", "data", "action", "formaction", "cite",
    "xlink:href", "srcset",
}
ATTRIBUTE = re.compile(
    r'''(?P<name>[^\s"'<>/=]+)(?:\s*=\s*(?:'''
    r'''"(?P<double>[^"]*)"|'(?P<single>[^']*)'|'''
    r'''(?P<bare>[^\s"'=<>`]+)))?'''
)
CSS_URL = re.compile(r'''url\(\s*(['"]?)(.*?)\1\s*\)''', re.I | re.S)
CSS_IMPORT = re.compile(r'''@import\s+(['"])(.*?)\1''', re.I | re.S)
FONT_FACE = re.compile(r"@font-face\{[^{}]*\}")
MILESTONE = re.compile(r"\bT(?:[0-9]{2}|[0-9][A-Z])\b")


class SiteError(ValueError):
    """A source, link or build cannot produce the promised site artifact."""


@dataclass(frozen=True)
class Page:
    key: str
    source: Path
    route: str


@dataclass(frozen=True)
class Output:
    """One built file of an example, for the landing page."""
    target: str
    route: str
    size: int
    pages: int | None = None


@dataclass(frozen=True)
class Example:
    page: Page
    title: str
    outputs: tuple[Output, ...]
    thumbnail: str
    thumbnail_size: tuple[int, int]


def normalize_prefix(value: str) -> str:
    """Empty means relative URLs; a supplied path becomes a rooted mount."""
    if value == "":
        return ""
    parsed = urlsplit(value)
    if parsed.scheme or parsed.netloc or parsed.query or parsed.fragment:
        raise SiteError("prefix must be a URL path without a host or query")
    if "\\" in value or "%" in value or "\x00" in value:
        raise SiteError("prefix must contain ordinary unescaped path segments")
    parts = value.strip("/").split("/")
    if parts == [""]:
        return "/"
    if any(part in ("", ".", "..") for part in parts):
        raise SiteError("prefix must not contain empty, dot or parent segments")
    return "/" + "/".join(quote(part, safe="-._~") for part in parts) + "/"


def repository_url(value: str) -> str:
    parsed = urlsplit(value)
    if (
        parsed.scheme != "https" or not parsed.netloc or parsed.username
        or parsed.password or parsed.query or parsed.fragment
    ):
        raise SiteError("repo-url must be an HTTPS repository URL")
    value = value.rstrip("/")
    return value[:-4] if value.endswith(".git") else value


def revision(value: str) -> str:
    if not re.fullmatch(r"[0-9a-fA-F]{40}|[0-9a-fA-F]{64}", value):
        raise SiteError("revision must be a full immutable Git commit SHA")
    return value.lower()


def source_within(root: Path, path: Path) -> Path:
    try:
        resolved = path.resolve(strict=True)
        resolved.relative_to(root)
    except (OSError, ValueError) as error:
        raise SiteError(f"source path is missing or outside the repository: {path}") from error
    return resolved


def require_complete_guide(root: Path) -> None:
    for name in GUIDE_PAGES:
        path = root / "docs/guide" / f"{name}.md"
        if not path.is_file():
            raise SiteError(f"complete guide prerequisite missing: docs/guide/{name}.md")
        text = path.read_text(encoding="utf-8")
        if MILESTONE.search(text):
            raise SiteError(f"guide still contains an internal milestone identifier: {path.name}")
    cli = (root / "docs/guide/cli.md").read_text(encoding="utf-8")
    if re.search(r"^\$ writer[^\n]*--help\r?\n```", cli, re.M):
        raise SiteError("generate and validate final guide help fences before building the site")
    config = (root / "docs/guide/configuration.md").read_text(encoding="utf-8")
    marker = "<!-- generated: writer dump --default-config -->"
    if marker not in config or re.search(r"```toml\s*```", config):
        raise SiteError("guide needs its actual generated default-config fence")


def pages(root: Path) -> list[Page]:
    result = [Page("home", root / "docs/site/index.html", "index.html")]
    for name in GUIDE_PAGES:
        leaf = "index" if name == "README" else name
        key = "guide" if name == "README" else f"guide-{name}"
        result.append(Page(key, root / "docs/guide" / f"{name}.md", f"guide/{leaf}.html"))
    for name in EXAMPLES:
        filename = {"notebook": "analysis.ipynb", "typst": "document.typ"}.get(name, "document.md")
        result.append(Page(f"example-{name}", root / "examples" / name / filename, f"examples/{name}/index.html"))
    return [Page(page.key, source_within(root, page.source), page.route) for page in result]


def srcset_urls(value: str) -> list[tuple[int, int]]:
    """Locate URL tokens without splitting commas inside data URLs."""
    found = []
    index = 0
    while index < len(value):
        while index < len(value) and (value[index].isspace() or value[index] == ","):
            index += 1
        start = index
        while index < len(value) and not value[index].isspace():
            index += 1
        end = index
        while end > start and value[end - 1] == ",":
            end -= 1
        if end > start:
            found.append((start, end))
        if end != index:
            continue
        depth = 0
        while index < len(value):
            char = value[index]
            index += 1
            if char == "(":
                depth += 1
            elif char == ")":
                depth = max(0, depth - 1)
            elif char == "," and depth == 0:
                break
    return found


def css_urls(value: str, legacy_rules=None) -> list[str]:
    ignored = set()
    if legacy_rules:
        for block in FONT_FACE.finditer(value):
            fallbacks = legacy_rules.get(block[0])
            if fallbacks is None:
                continue
            for match in CSS_URL.finditer(block[0]):
                if match[2] in fallbacks:
                    ignored.add((block.start() + match.start(),
                        block.start() + match.end()))
    return [match[2] for pattern in (CSS_URL, CSS_IMPORT)
        for match in pattern.finditer(value)
        if (match.start(), match.end()) not in ignored]


def katex_legacy_rules(root: Path) -> dict[str, frozenset[str]]:
    """Prove exact inactive fallbacks against canonical CSS and font bytes.

    Standalone Writer HTML preserves KaTeX's legacy .woff/.ttf URLs after
    embedding WOFF2 first (vendored-assets.md section 2.1). Only URL occurrences
    inside the matching complete rule are exempt from artifact lookup.
    """
    css = root / "assets/vendor/katex/katex.min.css"
    if not css.is_file():
        return {}
    text = source_within(root, css).read_bytes().decode("utf-8")
    rules = {}
    for block in FONT_FACE.finditer(text):
        urls = css_urls(block[0])
        if len(urls) != 3:
            continue
        face = re.fullmatch(r"fonts/(KaTeX_[A-Za-z0-9]+-[A-Za-z0-9]+)\.woff2", urls[0])
        if face is None:
            continue
        expected = [f"fonts/{face[1]}.{suffix}" for suffix in ("woff2", "woff", "ttf")]
        if urls != expected:
            continue
        font = source_within(root, css.parent / urls[0]).read_bytes()
        if not font.startswith(b"wOF2"):
            raise SiteError(f"canonical KaTeX font lacks WOFF2 signature: {urls[0]}")
        embedded = "data:font/woff2;base64," + base64.b64encode(font).decode("ascii")
        standalone = block[0].replace(urls[0], embedded, 1)
        rules[standalone] = frozenset(urls[1:])
    return rules


class AttributeRewriter(HTMLParser):
    """Edit only URL attribute value spans, preserving every other character."""

    def __init__(self, text: str, rewrite):
        super().__init__(convert_charrefs=False)
        self.text = text
        self.rewrite = rewrite
        self.starts = [0] + [index + 1 for index, char in enumerate(text) if char == "\n"]
        self.edits = []

    def handle_starttag(self, tag, attrs):
        self.tag(tag)

    def handle_startendtag(self, tag, attrs):
        self.tag(tag)

    def tag(self, tag):
        raw = self.get_starttag_text()
        opening = re.match(r"<[^\s/>]+", raw)
        if opening is None:
            raise SiteError("could not locate emitted HTML tag")
        line, column = self.getpos()
        offset = self.starts[line - 1] + column
        for match in ATTRIBUTE.finditer(raw, opening.end()):
            name = match["name"].lower()
            group = next((key for key in ("double", "single", "bare") if match[key] is not None), None)
            if name not in URL_ATTRIBUTES or group is None:
                continue
            original = html.unescape(match[group])
            if name == "srcset":
                changed = original
                for start, end in reversed(srcset_urls(original)):
                    url = self.rewrite(tag, name, original[start:end])
                    changed = changed[:start] + url + changed[end:]
            else:
                changed = self.rewrite(tag, name, original)
            if changed != original:
                start, end = match.span(group)
                self.edits.append((offset + start, offset + end, html.escape(changed, quote=True)))

    def result(self):
        self.feed(self.text)
        self.close()
        changed = self.text
        for start, end, value in sorted(self.edits, reverse=True):
            changed = changed[:start] + value + changed[end:]
        return changed


class Mapper:
    def __init__(
                self, root: Path, output: Path, all_pages: list[Page],
                repo: str, sha: str, prefix: str
            ):
        self.root = root
        self.output = output
        self.repo = repository_url(repo)
        self.sha = revision(sha)
        self.prefix = normalize_prefix(prefix)
        self.by_key = {page.key: page for page in all_pages}
        self.by_source = {page.source: page for page in all_pages}
        if len(self.by_key) != len(all_pages) or len(self.by_source) != len(all_pages):
            raise SiteError("site page keys and source paths must be unique")
        if len({page.route.casefold() for page in all_pages}) != len(all_pages):
            raise SiteError("site page routes must be distinct on case-insensitive hosts")
        self.claimed = {}
        for page in all_pages:
            self.claim(page.route)
        self.claim("licenses/LICENSE")
        self.claim("licenses/embedded.txt")
        self.claim("site-manifest.json")
        self.claim("site.css")
        self.claim(".nojekyll")

    def claim(self, route: str):
        path = PurePosixPath(route)
        if path.is_absolute() or ".." in path.parts or "\\" in route:
            raise SiteError(f"invalid artifact route: {route}")
        key = route.casefold()
        previous = self.claimed.get(key)
        if previous is not None and previous != route:
            raise SiteError(f"case-only artifact path collision: {previous} and {route}")
        self.claimed[key] = route

    def local(self, route: str, current: Page, query="", fragment="") -> str:
        if route == current.route and fragment and not query:
            return "#" + fragment
        path = (
            self.prefix + quote(route, safe="/") if self.prefix
            else quote(posixpath.relpath(route, posixpath.dirname(current.route) or "."), safe="/")
        )
        return urlunsplit(("", "", path, query, fragment))

    def repository_source(self, path: Path, fragment="", query="") -> str:
        relative = source_within(self.root, path).relative_to(self.root).as_posix()
        kind = "tree" if path.is_dir() else "blob"
        return urlunsplit((
            "https", urlsplit(self.repo).netloc,
            urlsplit(self.repo).path + f"/{kind}/{self.sha}/" + quote(relative, safe="/"),
            query, fragment
        ))

    def resource(self, path: Path) -> str:
        source = source_within(self.root, path)
        if not source.is_file():
            raise SiteError(f"linked resource is not a file: {path}")
        route = "assets/source/" + source.relative_to(self.root).as_posix()
        self.claim(route)
        destination = self.output / route
        if destination.exists():
            if destination.read_bytes() != source.read_bytes():
                raise SiteError(f"conflicting resource bytes: {route}")
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(source.read_bytes())
        return route

    def rewrite(self, current: Page, tag: str, attribute: str, value: str) -> str:
        if value.startswith("writer-site:"):
            key, _, fragment = value.removeprefix("writer-site:").partition("#")
            if key == "source":
                return self.repository_source(current.source)
            if key == "repository":
                return self.repo + "/tree/" + self.sha
            if key in ("license", "embedded-notices"):
                route = "licenses/LICENSE" if key == "license" else "licenses/embedded.txt"
                return self.local(route, current)
            if key.startswith("artifact/"):
                # A file the build writes itself, such as an example's PDF.
                route = key.removeprefix("artifact/")
                if self.claimed.get(route.casefold()) != route:
                    raise SiteError(f"unknown site artifact route: {route}")
                return self.local(route, current, fragment=fragment)
            if key not in self.by_key:
                raise SiteError(f"unknown site navigation key: {key}")
            return self.local(self.by_key[key].route, current, fragment=fragment)
        parsed = urlsplit(value)
        if parsed.scheme or parsed.netloc:
            if parsed.scheme not in ("", "https", "http", "mailto", "data"):
                raise SiteError(f"unresolved or unsupported emitted URL: {value}")
            return value
        if not parsed.path:
            return value
        decoded = unquote(parsed.path)
        if decoded.startswith("/") or "\\" in decoded or "\x00" in decoded:
            raise SiteError(f"unexpected absolute emitted source URL: {value}")
        source = source_within(self.root, current.source.parent / decoded)
        page = self.by_source.get(source)
        if page is None and source.is_dir():
            page = self.by_source.get((source / "README.md").resolve())
        if page is not None:
            return self.local(page.route, current, parsed.query, parsed.fragment)
        if tag == "a" and attribute == "href":
            return self.repository_source(source, parsed.fragment, parsed.query)
        return self.local(self.resource(source), current, parsed.query, parsed.fragment)


class References(HTMLParser):
    def __init__(self, legacy_rules=None):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.urls = []
        self.style = 0
        self.duplicates = set()
        self.legacy_rules = legacy_rules

    def handle_starttag(self, tag, attrs):
        identifiers = set()
        for name, value in attrs:
            if value is None:
                continue
            if name == "id" or (tag == "a" and name == "name"):
                identifiers.add(value)
            if name in URL_ATTRIBUTES:
                if name == "srcset":
                    self.urls.extend(value[start:end] for start, end in srcset_urls(value))
                else:
                    self.urls.append(value)
            if name == "style":
                self.urls.extend(css_urls(value))
        self.duplicates.update(identifiers.intersection(self.ids))
        self.ids.update(identifiers)
        if tag == "style":
            self.style += 1

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)

    def handle_endtag(self, tag):
        if tag == "style":
            self.style = max(0, self.style - 1)

    def handle_data(self, data):
        if self.style:
            self.urls.extend(css_urls(data, self.legacy_rules))


def artifact_target(current: str, value: str, prefix: str) -> tuple[str, str] | None:
    parsed = urlsplit(value)
    if parsed.netloc or parsed.scheme:
        if parsed.scheme not in ("", "https", "http", "mailto", "data"):
            raise SiteError(f"unsupported artifact URL: {value}")
        return None
    supplied = normalize_prefix(prefix)
    mount = unquote(supplied) or "/"
    decoded = unquote(parsed.path)
    if "\\" in decoded or "\x00" in decoded:
        raise SiteError(f"invalid local artifact URL: {value}")
    if not parsed.path:
        route = current
    elif decoded.startswith("/"):
        path = posixpath.normpath(decoded)
        if not supplied or not path.startswith(mount):
            raise SiteError(f"local link escapes configured prefix: {value}")
        route = path[len(mount):]
    else:
        route = posixpath.normpath(posixpath.join(posixpath.dirname(current), decoded))
        if route == ".." or route.startswith("../"):
            raise SiteError(f"local link escapes artifact root: {value}")
    if parsed.path.endswith("/"):
        route = posixpath.join(route, "index.html")
    return route, unquote(parsed.fragment)


def validate_site(
            output: Path, prefix: str, forbidden_paths=(), legacy_rules=None
        ) -> None:
    parsed_pages = {}
    local_css = []
    for path in sorted(output.rglob("*")):
        if path.suffix.lower() == ".css":
            css = path.read_text(encoding="utf-8")
            local_css.extend((path.relative_to(output).as_posix(), url)
                for url in css_urls(css, legacy_rules))
            for forbidden in forbidden_paths:
                if str(forbidden) in html.unescape(unquote(css)):
                    raise SiteError(f"checkout/build path leaked into {path.relative_to(output)}")
        if path.suffix.lower() not in (".html", ".svg"):
            continue
        text = path.read_text(encoding="utf-8")
        decoded = html.unescape(unquote(text))
        for forbidden in forbidden_paths:
            if str(forbidden) in decoded or str(forbidden).replace("\\", "/") in decoded:
                raise SiteError(f"checkout/build path leaked into {path.relative_to(output)}")
        refs = References(legacy_rules)
        refs.feed(text)
        refs.close()
        if refs.duplicates:
            raise SiteError(f"duplicate ids in {path.relative_to(output)}: {sorted(refs.duplicates)}")
        parsed_pages[path.relative_to(output).as_posix()] = refs
    if not any(PurePosixPath(route).suffix.lower() == ".html" for route in parsed_pages):
        raise SiteError("site has no HTML pages")
    references = [(current, value) for current, refs in parsed_pages.items() for value in refs.urls]
    references.extend(local_css)
    for current, value in references:
        target = artifact_target(current, value, prefix)
        if target is None:
            continue
        route, fragment = target
        path = output / route
        try:
            path.resolve().relative_to(output.resolve())
        except ValueError as error:
            raise SiteError(f"local link escapes artifact: {current}: {value}") from error
        if not path.is_file():
            raise SiteError(f"missing local target: {current}: {value}")
        if fragment and path.suffix.lower() in (".html", ".svg"):
            if fragment not in parsed_pages[route].ids:
                raise SiteError(f"missing local fragment: {current}: {value}")


def command(
            binary: Path, args: list[str], root: Path, environment: dict
        ) -> bytes:
    result = subprocess.run(
        [str(binary), *args], cwd=root, env=environment,
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, check=False
    )
    if result.returncode:
        raise SiteError(
            f"writer {' '.join(args)} failed ({result.returncode}):\n"
            + result.stderr.decode("utf-8", errors="replace")
        )
    return result.stdout


def publish_fresh(artifact: Path, output: Path) -> None:
    """Claim a fresh root exclusively before moving validated contents."""
    output.parent.mkdir(parents=True, exist_ok=True)
    # On Unix a directory rename can replace a concurrently created empty
    # destination. Exclusive mkdir preserves every pre-existing destination.
    output.mkdir()
    try:
        for path in sorted(artifact.iterdir()):
            path.rename(output / path.name)
    except BaseException:
        shutil.rmtree(output)
        raise
    artifact.rmdir()


def source_epoch(environment: dict) -> int:
    value = environment.get("SOURCE_DATE_EPOCH", "0")
    if not re.fullmatch(r"[0-9]{1,10}", value):
        raise SiteError("SOURCE_DATE_EPOCH must be a whole number of seconds")
    return int(value)


def pack(directory: Path, destination: Path, name: str, epoch: int) -> None:
    """Write a byte-reproducible ZIP of `directory` under one root folder."""
    stamp = time.gmtime(min(max(epoch, ZIP_EPOCH), 4_102_444_799))[:6]
    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination, "x") as archive:
        for path in sorted(directory.rglob("*")):
            if not path.is_file():
                continue
            relative = path.relative_to(directory).as_posix()
            entry = zipfile.ZipInfo(f"{name}/{relative}", stamp)
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.create_system = 3
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, path.read_bytes())


def png_size(path: Path) -> tuple[int, int]:
    data = path.read_bytes()[:24]
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise SiteError(f"page preview is not a PNG: {path.name}")
    return struct.unpack(">II", data[16:24])


def first_page(gs: str, pdf: Path, png: Path) -> None:
    """Draw the PDF's first page with Ghostscript, for the landing page."""
    result = subprocess.run(
        [
            gs, "-q", "-dSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=png16m",
            "-r100", "-dTextAlphaBits=4", "-dGraphicsAlphaBits=4",
            "-dFirstPage=1", "-dLastPage=1", f"-sOutputFile={png}", str(pdf),
        ],
        stdin=subprocess.DEVNULL, capture_output=True, check=False
    )
    if result.returncode or not png.is_file():
        raise SiteError(
            f"Ghostscript could not draw {pdf.name} ({result.returncode}):\n"
            + result.stderr.decode("utf-8", errors="replace")
        )


def build_example(
            binary: Path, gs: str | None, page: Page, artifact: Path,
            mapper: Mapper, config: Path, work: Path, root: Path,
            environment: dict
        ) -> Example:
    """Build an example's downloads beside its web page."""
    if gs is None:
        raise SiteError(
            "page previews need Ghostscript: install it or pass --gs"
        )
    folder = posixpath.dirname(page.route)
    stem = page.source.stem
    base = ["--config", str(config), "build", str(page.source)]
    html_size = (artifact / page.route).stat().st_size
    outputs = [Output("html", page.route, html_size)]
    for target in ("pdf", "docx"):
        route = f"{folder}/{stem}.{target}"
        mapper.claim(route)
        report = command(
            binary, [*base, "--to", target, "-o", str(artifact / route)],
            root, environment
        )
        pages = re.search(rb"\(([0-9]+) pages?,", report)
        size = (artifact / route).stat().st_size
        outputs.append(Output(target, route, size, pages and int(pages[1])))
    for target, suffix in (("latex", ".tex"), ("typst", ".typ")):
        if suffix == page.source.suffix:
            continue
        export = work / f"{page.key}-{target}"
        command(
            binary, [*base, "--to", target, "-o", str(export / (stem + suffix))],
            root, environment
        )
        route = f"{folder}/{stem}-{target}.zip"
        mapper.claim(route)
        pack(export, artifact / route, f"{stem}-{target}", source_epoch(environment))
        outputs.append(Output(target, route, (artifact / route).stat().st_size))
    thumbnail = f"{folder}/page-1.png"
    mapper.claim(thumbnail)
    first_page(gs, artifact / f"{folder}/{stem}.pdf", artifact / thumbnail)
    context = json.loads(command(
        binary, ["--config", str(config), "dump", str(page.source), "--context"],
        root, environment
    ))
    title = context.get("document", {}).get("meta", {}).get("title")
    return Example(
        page, title if isinstance(title, str) and title else page.source.parent.name,
        tuple(outputs), thumbnail, png_size(artifact / thumbnail)
    )


def size_text(size: int) -> str:
    """Sizes as writer prints them: decimal kB and MB."""
    if size < 1_000_000:
        return f"{size / 1000:.1f} kB"
    return f"{size / 1_000_000:.1f} MB"


def output_note(output: Output) -> str:
    if output.target == "html":
        return f"One self-contained file, {size_text(output.size)}"
    if output.pages:
        unit = "page" if output.pages == 1 else "pages"
        return f"{output.pages} {unit}, {size_text(output.size)}"
    if output.route.endswith(".zip"):
        return f"Source and resources, {size_text(output.size)} zip"
    return size_text(output.size)


def output_link(output: Output) -> str:
    return "writer-site:artifact/" + output.route


def desk_section(examples: list[Example]) -> str:
    """The example PDFs' first pages; the first example lies on top."""
    items = []
    for example in reversed(examples):
        pdf = next(item for item in example.outputs if item.target == "pdf")
        width, height = example.thumbnail_size
        name = html.escape(example.page.source.parent.name)
        items.append(
            f'<a class="page" href="{output_link(pdf)}">'
            f'<img src="writer-site:artifact/{html.escape(example.thumbnail)}" '
            f'alt="The first page of the {name} example\'s PDF" '
            f'width="{width}" height="{height}">'
            '<span class="tag">PDF</span></a>'
        )
    return "\n".join(" " * 12 + item for item in items)


def formats_section(example: Example) -> str:
    built = {output.target: output for output in example.outputs}
    items = []
    for target, label, _ in FORMATS:
        output = built.get(target)
        if output is None:
            continue
        items.append(
            f'<li><a class="fmt-{target}" href="{output_link(output)}">'
            f'<span class="file">{html.escape(target.upper())}</span>'
            f"<strong>{html.escape(label)}</strong>"
            f'<span class="meta">{html.escape(output_note(output))}</span>'
            "</a></li>"
        )
    return "\n".join(" " * 12 + item for item in items)


def excerpt(path: Path) -> str:
    lines = path.read_text(encoding="utf-8").splitlines()
    text = "\n".join(lines[:EXCERPT_LINES])
    if len(lines) > EXCERPT_LINES:
        text += "\n…"
    return html.escape(text, quote=False)


def examples_section(examples: list[Example], mapper: Mapper, root: Path) -> str:
    cards = []
    for example in examples:
        page = example.page
        name = page.source.parent.name
        kind, description = EXAMPLE_NOTES.get(name, ("Example", ""))
        width, height = example.thumbnail_size
        relative = page.source.relative_to(root).as_posix()
        chips = []
        for output in example.outputs:
            short = next(short for target, _, short in FORMATS if target == output.target)
            chips.append(
                f'<li><a class="fmt-{output.target}" href="{output_link(output)}">'
                f"{html.escape(short)}</a></li>"
            )
        source = html.escape(mapper.repository_source(page.source))
        chips.append(f'<li><a class="source-link" href="{source}">Source</a></li>')
        cards.append(
            '<article class="card">\n'
            f'<a class="thumb" href="writer-site:{page.key}" tabindex="-1">'
            f'<img src="writer-site:artifact/{html.escape(example.thumbnail)}" '
            f'alt="" loading="lazy" width="{width}" height="{height}"></a>\n'
            '<div class="body">\n'
            f'<p class="kicker">examples/{html.escape(name)} · {html.escape(kind)}</p>\n'
            f'<h3><a href="writer-site:{page.key}">{html.escape(example.title)}</a></h3>\n'
            f'<p class="description">{html.escape(description)}</p>\n'
            f"<pre><code>writer build {html.escape(relative)} --to pdf</code></pre>\n"
            f'<ul class="chips">{"".join(chips)}</ul>\n'
            "</div>\n"
            "</article>"
        )
    return "\n".join(cards)


def guide_section(root: Path) -> str:
    items = []
    for name in GUIDE_PAGES:
        title = name
        text = (root / "docs/guide" / f"{name}.md").read_text(encoding="utf-8")
        heading = re.search(r"^# (.+)$", text, re.M)
        if heading:
            title = heading[1].strip()
        key = "guide" if name == "README" else f"guide-{name}"
        items.append(f'<li><a href="writer-site:{key}">{html.escape(title)}</a></li>')
    return '<ul class="guide">\n' + "\n".join(items) + "\n</ul>"


def download_section(release: dict | None, repo: str) -> str:
    releases = html.escape(repo + "/releases")
    if not release or not release.get("assets"):
        return (
            "<p class=\"release\">No release has been published yet. "
            f'Releases will appear on the <a href="{releases}">releases page</a>.</p>'
        )
    tag = release["tag_name"]
    assets = {asset["name"]: asset for asset in release["assets"]}
    rows = []
    for target, platform, kind in PLATFORMS:
        extension = "zip" if target.endswith("-msvc") else "tar.gz"
        asset = assets.get(f"writer-{tag}-{target}.{extension}")
        if asset is None:
            continue
        rows.append(
            f"<tr><td>{html.escape(platform)}</td><td>{html.escape(kind)}</td>"
            f'<td><a href="{html.escape(asset["browser_download_url"])}">'
            f'<code>{html.escape(asset["name"])}</code></a></td>'
            f'<td class="num">{size_text(asset["size"])}</td></tr>'
        )
    date = (release.get("published_at") or "")[:10]
    parts = [
        f'<p class="release"><strong>Latest release: '
        f'<a href="{html.escape(release["html_url"])}">writer {html.escape(tag)}</a>'
        "</strong>" + (f", published {html.escape(date)}" if date else "") + ".</p>"
    ]
    if rows:
        parts.append(
            '<table class="downloads">\n<thead><tr><th>Platform</th><th></th>'
            "<th>Archive</th><th>Size</th></tr></thead>\n<tbody>\n"
            + "\n".join(rows) + "\n</tbody>\n</table>"
        )
    notes = []
    sums = assets.get("SHA256SUMS")
    if sums:
        sums_url = html.escape(sums["browser_download_url"])
        notes.append(
            f'Check a download against <a href="{sums_url}"><code>SHA256SUMS</code></a>, '
            "unpack it (<code>tar -xzf</code>, or <code>Expand-Archive</code> on "
            "Windows) and put <code>writer</code> on your <code>PATH</code>."
        )
    notes.append(
        "The binaries are not signed: on macOS, remove the quarantine mark of a "
        "browser download with <code>xattr -d com.apple.quarantine writer</code>."
    )
    notes.append(
        "PDF, Word and the web page need nothing else. Compiling the LaTeX "
        "export needs a TeX installation."
    )
    parts.append(
        '<ul class="notes">\n' + "\n".join(f"<li>{note}</li>" for note in notes)
        + "\n</ul>"
    )
    parts.append(f'<p><a href="{releases}">All releases and their notes</a>.</p>')
    return "\n".join(parts)


def landing_page(
            root: Path, page: Page, examples: list[Example], mapper: Mapper,
            release: dict | None, version: str
        ) -> str:
    template = Template(page.source.read_text(encoding="utf-8"))
    basic = next(
        (example for example in examples if example.page.key == "example-basic"),
        None
    )
    if basic is None:
        raise SiteError("the landing page needs the basic example")
    commit = html.escape(f"{mapper.repo}/commit/{mapper.sha}")
    stamp = (
        f"Built by {html.escape(version)} from commit "
        f'<a href="{commit}"><code>{mapper.sha[:7]}</code></a>.'
        " writer is MIT licensed."
    )
    return template.substitute(
        repo_url=html.escape(mapper.repo),
        desk=desk_section(examples),
        source_name=html.escape(basic.page.source.relative_to(root).as_posix()),
        source_excerpt=excerpt(basic.page.source),
        formats=formats_section(basic),
        download=download_section(release, mapper.repo),
        examples=examples_section(examples, mapper, root),
        guide=guide_section(root),
        stamp=stamp,
    )


def write_page(artifact: Path, mapper: Mapper, page: Page, text: str) -> None:
    text = AttributeRewriter(
        text, lambda tag, name, value: mapper.rewrite(page, tag, name, value)
    ).result()
    destination = artifact / page.route
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(text, encoding="utf-8", newline="")


def build(args) -> None:
    root = args.source.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    if args.out.exists() or args.out.is_symlink():
        raise SiteError("site output destination already exists")
    output = args.out.resolve()
    if not binary.is_file():
        raise SiteError("binary must be an existing writer executable")
    if output.exists() or output.is_symlink():
        raise SiteError("site output destination already exists")
    release = None
    if args.release_json is not None:
        release = json.loads(args.release_json.read_text(encoding="utf-8"))
        if not isinstance(release, dict) or not isinstance(release.get("tag_name"), str):
            raise SiteError("release JSON must be a GitHub release document")
    gs = args.gs or shutil.which("gs")
    require_complete_guide(root)
    all_pages = pages(root)
    legacy_rules = katex_legacy_rules(root)
    theme = source_within(root, root / "docs/site/theme")
    # Keep raw input at normalization boundaries; Mapper owns the encoded form.
    prefix = args.prefix
    environment = dict(os.environ)
    environment.pop("RUST_LOG", None)
    environment["NO_COLOR"] = "1"
    environment.setdefault("SOURCE_DATE_EPOCH", "0")
    ancestor = output.parent
    while not ancestor.exists():
        ancestor = ancestor.parent
    with tempfile.TemporaryDirectory(prefix=".writer-site-", dir=ancestor) as temporary:
        work = Path(temporary)
        artifact = work / "artifact"
        artifact.mkdir()
        mapper = Mapper(root, artifact, all_pages, args.repo_url, args.revision, prefix)
        config = work / "writer.toml"
        config.write_text('[pdf]\nsystem_fonts = false\nallow_packages = false\n', encoding="utf-8")
        home = None
        for index, page in enumerate(all_pages):
            if page.key == "home":
                # Written last: it links to every example's outputs.
                home = page
                continue
            raw = work / f"page-{index}.html"
            command(
                binary,
                ["--config", str(config), "build", str(page.source),
                 "--to", "html", "--standalone", "--theme", str(theme),
                 "-o", str(raw)],
                root, environment
            )
            write_page(artifact, mapper, page, raw.read_bytes().decode("utf-8"))
        examples = [
            build_example(
                binary, gs, page, artifact, mapper, config, work, root,
                environment
            )
            for page in all_pages if page.key.startswith("example-")
        ]
        if home is not None:
            version = command(binary, ["--version"], root, environment)
            version = " ".join(version.decode("utf-8").split()[:2])
            text = landing_page(root, home, examples, mapper, release, version)
            write_page(artifact, mapper, home, text)
            shutil.copyfile(
                source_within(root, root / "docs/site/site.css"),
                artifact / "site.css"
            )
        # GitHub Pages serves the files as they are, without Jekyll.
        (artifact / ".nojekyll").write_bytes(b"")
        notices = artifact / "licenses"
        notices.mkdir(exist_ok=True)
        (notices / "LICENSE").write_bytes((root / "LICENSE").read_bytes())
        (notices / "embedded.txt").write_bytes(
            command(binary, ["--licenses"], root, environment)
        )
        manifest = {
            "revision": mapper.sha, "prefix": mapper.prefix,
            "pages": [page.route for page in all_pages],
            "downloads": sorted(
                output.route for example in examples
                for output in example.outputs if output.target != "html"
            ),
            "release": release["tag_name"] if release else None,
            "notices": {"project": "licenses/LICENSE", "embedded_assets": "licenses/embedded.txt"},
        }
        (artifact / "site-manifest.json").write_text(
            json.dumps(manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        validate_site(artifact, prefix, (root, work), legacy_rules)
        publish_fresh(artifact, output)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--repo-url", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--prefix", default="")
    parser.add_argument(
        "--release-json", type=Path,
        help="the GitHub API's releases/latest document, for the download table"
    )
    parser.add_argument(
        "--gs", help="Ghostscript, for page previews (default: gs on PATH)"
    )
    args = parser.parse_args()
    try:
        build(args)
    except (OSError, ValueError) as error:
        parser.exit(1, f"site: {error}\n")
    print(f"Built site artifact: {args.out}")


if __name__ == "__main__":
    main()
