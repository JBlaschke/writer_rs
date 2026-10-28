#!/usr/bin/env python3
"""Standard-library writer live-reload protocol mock (live-reload.md §1–3).

POST /__mock/document|css|head|body|bodyend: stage UTF-8 markup (no event).
GET /__mock/document: staged document-root children.
POST /__mock/asset?path=NAME: stage a theme-chain asset.
POST /__mock/image?name=ID: change SVG image bytes.
GET /__mock/emit?kind=content|css|pages[&web=ok|error|skipped][&pages=...]
    [&line=N|&cell=N][&source=N][&file=NAME][&warning=1|0]
    [&message=TEXT][&error_kind=parse|compile][&nodoc=1]
    broadcasts a build; skipped views retain their previous error state.
GET /__mock/title?t=TEXT, /lang?l=TAG: stage metadata.
GET /__mock/down?ms=N: drop SSE streams and refuse reconnects temporarily.
GET /__mock/restart: restart at build 1 with staged content and drop streams.
GET /__mock/failroot?on=1|0: fail content fetches without stopping SSE.
GET /__mock/early?on=1|0: stage the client script in head, before body.
GET /__mock/reset: reset build 1; /log: return and clear request log.
POST /__mock/pages: stage {hashes:[16-hex],size_pt:[[w,h]]}; emit pages to apply.
GET /?view=pages&client=1: the real pages client; default remains the T22 spy.
GET /__mock/pages/hold|release?hash=HASH: gate a page response for revision tests.

WRITER_ASSETS overrides the repository assets directory; WRITER_CLIENT overrides
live-reload.js, useful while testing a scratch implementation. No dependencies.
"""

import argparse
import html
import json
import mimetypes
import os
import re
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, unquote, urlsplit

ASSETS = Path(os.environ.get(
    "WRITER_ASSETS", str(Path(__file__).resolve().parents[3] / "assets")
)).resolve()
CLIENT = Path(os.environ.get(
    "WRITER_CLIENT", str(ASSETS / "writer/live-reload.js")
)).resolve()
PAGES_SCRIPT = Path(os.environ.get(
    "WRITER_PAGES_CLIENT", str(ASSETS / "writer/pages.js")
)).resolve()

INITIAL_CSS = """html { scroll-behavior: auto; }
body { margin: 0; color: #123; background: white; font: 16px/1.5 sans-serif; }
[data-writer-document] { max-width: 760px; margin: auto; padding: 24px; }
p[data-sourcepos] { min-height: 64px; margin: 12px 0; }
img { width: 80px; height: 40px; }
"""


def initial_document():
    blocks = ["<h1>Writer live reload</h1>"]
    for index in range(1, 61):
        content = f"Paragraph {index}: stable text for reading-place checks."
        if index == 12:
            content += '<span id="math" data-tex="x^2" data-math-style="inline">x^2</span>'
        if index == 20:
            content += '<img id="image1" src="/__writer/doc-asset/000000000001?v=1" alt="First">'
            content += '<img id="image2" src="/__writer/doc-asset/000000000002?v=1" alt="Second">'
        blocks.append(
            f'<p id="p{index}" data-source="0" '
            f'data-sourcepos="{index * 10}-{index * 10 + 5}">{content}</p>'
        )
        if index == 8:
            blocks.append(
                '<details id="details"><summary>Details</summary>Body</details>'
            )
        if index == 42:
            blocks.append(
                '<section id="include" data-source="1" data-sourcepos="1-90">'
                '<p id="included" data-source="1" data-sourcepos="40-45">'
                'Included paragraph.</p>'
                '<p id="included-narrow" data-source="1" data-sourcepos="41-41">'
                'Narrow included paragraph.</p></section>'
            )
        if index == 52:
            blocks.append(
                '<div id="cell3" data-source="2" data-cell="3" '
                'data-sourcepos="1-4"><p>Notebook cell 3</p></div>'
            )
    return "\n".join(blocks)


def svg(color):
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40">'
        f'<rect width="80" height="40" fill="{color}"/></svg>'
    )


PAGE = """<!DOCTYPE html>
<html lang="{lang}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link id="writer-theme" rel="stylesheet" href="/__writer/theme.css?v={build_id}">
<link rel="stylesheet" href="/__writer/asset/extra.css?v=1">
{head}</head><body>{body}
<main data-writer-document data-version="{build_id}">{document}</main>
<script src="/__writer/asset/vendor/katex/katex.min.js"></script>
<script src="/__writer/asset/writer/math.js"></script>
<script src="/__writer/live-reload.js"></script>{body_end}
</body></html>
"""

# Deliberately a spy, not the T24 pages implementation. T22 owns delegation only.
PAGES = """<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">
<title>Mock pages</title></head><body><main data-writer-pages></main>
<script>window.__qaPages = []; window.__writerPages = {
apply(p, id) { window.__qaPages.push({pages: p, id}); }
};</script><script src="/__writer/live-reload.js"></script></body></html>
"""


# T22's default spy shell stays unchanged. client=1 opts into the T24 client.
PAGES_LIVE = """<!DOCTYPE html><html lang="en" data-writer-view="pages">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Mock compiler pages</title><style>
body { margin:0; padding-top:80px; background:#ddd; }
#writer-pages-toolbar { position:fixed; inset:0 0 auto; z-index:100;
    display:flex; gap:8px; padding:12px 16px; background:white; }
#writer-pages { display:flex; flex-direction:column; align-items:center;
    gap:16px; padding:16px; }
.writer-page { background:white; box-shadow:0 2px 8px #0003; }
.writer-page img { display:block; }
</style></head><body>
<header id="writer-pages-toolbar">
<button type="button" data-zoom="out">−</button><span id="writer-pages-zoom">Fit</span>
<button type="button" data-zoom="in">+</button><button type="button" data-zoom="fit">Fit</button>
<span id="writer-pages-counter"></span><span id="writer-pages-status" hidden></span>
</header><main id="writer-pages" data-writer-pages></main>
<script src="/__writer/pages.js"></script>
<script src="/__writer/live-reload.js"></script></body></html>
"""


def page_svg(index, size, digest):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{size[0]}pt" '
        f'height="{size[1]}pt"><rect width="100%" height="100%" '
        f'fill="#{digest[-6:]}"/><text x="20" y="40">'
        f'Page {index}: {digest}</text></svg>'
    )


def error_payload(query):
    line = query.get("line", ["41"])[0]
    cell = query.get("cell", [""])[0]
    return {
        "message": query.get("message", ["The document cannot be parsed"])[0],
        "file": query.get("file", ["document.md"])[0],
        "line": int(line) if line else None,
        "cell": int(cell) if cell else None,
        "kind": query.get("error_kind", ["parse"])[0],
        "excerpt": ":::include <missing> & file.md",
        "hints": ["Fix <the> source & save", "See the terminal"],
    }


def fallback(error):
    location = error["file"]
    if error.get("cell") is not None:
        location += f' cell {error["cell"]}'
    if error.get("line") is not None:
        location += f':{error["line"]}'
    text = (
        f'[web, pages] build error ({error["kind"]})\n'
        f'{error["message"]}\n{location}\n{error["excerpt"]}\n'
        + "\n".join(f"hint: {hint}" for hint in error["hints"])
        + "\n(click to dismiss)"
    )
    return (
        '<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">'
        '<title>writer: build error</title></head><body>'
        '<p>The document could not be built. Fix the error and save: '
        'this page reloads by itself.</p>'
        f'<pre id="writer-overlay">{html.escape(text)}</pre>'
        '<script src="/__writer/live-reload.js"></script></body></html>'
    )


class Mock:
    """State guarded by cond; immutable snapshots represent last good HTML."""

    def __init__(self):
        self.cond = threading.Condition()
        self.generation = 0
        self.reset()

    def reset(self):
        for held in getattr(self, "page_holds", {}).values():
            held.set()
        self.page_holds = {}
        self.page_versions = {}
        self.stage_pages({
            "hashes": ["0000000000000001", "0000000000000002"],
            "size_pt": [[595.28, 841.89], [595.28, 841.89]],
        })
        self.build_id = 1
        self.document = initial_document()
        self.css = INITIAL_CSS
        self.title = "Mock writer document"
        self.lang = "en"
        self.head = ""
        self.body = ""
        self.body_end = ""
        self.early = False
        self.assets = {"extra.css": "body { --qa-extra: 1; }"}
        self.images = {
            "000000000001": svg("red"), "000000000002": svg("blue")
        }
        self.views = {"web": {"status": "ok"}, "pages": self.pages("ok")}
        self.warnings = []
        self.events = []
        self.down_until = 0
        self.fail_root = False
        self.log = []
        self.served = self.snapshot()

    def stage_pages(self, spec):
        hashes, sizes = spec["hashes"], spec["size_pt"]
        if len(hashes) != len(sizes) or any(
            not isinstance(digest, str) or not re.fullmatch("[0-9a-f]{16}", digest)
            or len(size) != 2 or any(not isinstance(n, (int, float)) or n <= 0 for n in size)
            for digest, size in zip(hashes, sizes)
        ):
            raise ValueError("invalid mock pages")
        self.page_data = {"count": len(hashes), "hashes": list(hashes),
                          "size_pt": [list(size) for size in sizes]}
        for index, (digest, size) in enumerate(zip(hashes, sizes), 1):
            self.page_versions.setdefault(digest, page_svg(index, size, digest))

    def pages(self, status):
        return {"status": status, **self.page_data}

    def snapshot(self):
        return {"document": self.document, "css": self.css,
                "title": html.escape(self.title), "lang": html.escape(self.lang),
                "head": self.head, "body": self.body, "body_end": self.body_end,
                "assets": dict(self.assets), "build_id": self.build_id,
                "early": self.early}

    def hello(self):
        return {"build_id": self.build_id,
                "status": "error" if any(v["status"] == "error"
                                         for v in self.views.values()) else "ok",
                **self.views, "warnings": self.warnings}

    def emit(self, kind, query):
        self.build_id += 1
        event_views = {}
        for name in ("web", "pages"):
            skipped = kind == ("pages" if name == "web" else "css")
            status = query.get(name, ["skipped" if skipped else "ok"])[0]
            view = self.pages(status) if name == "pages" else {"status": status}
            if status == "error":
                view["error"] = error_payload(query)
            if status != "skipped":
                self.views[name] = view
            event_views[name] = view
        if event_views["web"]["status"] == "ok":
            self.served = self.snapshot()
        if query.get("nodoc", ["0"])[0] == "1":
            self.served = None
        if "warning" in query:
            self.warnings = [] if query["warning"][0] == "0" else [{
                "level": "warning", "code": "unresolved-reference",
                "message": "label <nope> does not exist & remains unresolved",
                "file": "document.md", "line": 12, "cell": None,
            }, {
                "level": "error", "code": "duplicate-label",
                "message": "label <twice> occurs twice", "file": "book.ipynb",
                "line": 2, "cell": 3,
            }]
        changed = None
        if kind == "content" and ("line" in query or "cell" in query):
            changed = {"file": query.get("file", ["document.md"])[0],
                       "source": int(query.get("source", ["0"])[0])}
            position = "cell" if "cell" in query else "line"
            changed[position] = int(query[position][0])
        event = {**self.hello(), **event_views, "kind": kind,
                 "duration_ms": 5, "changed": changed}
        self.events.append(event)
        self.cond.notify_all()
        return event

    def drop_clients(self):
        self.generation += 1
        self.cond.notify_all()


MOCK = Mock()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"

    def log_message(self, *_args):
        pass

    def send(self, status, body, ctype, cache="no-store"):
        data = body.encode() if isinstance(body, str) else body
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", cache)
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_POST(self):
        url = urlsplit(self.path)
        query = parse_qs(url.query)
        body = self.rfile.read(int(self.headers.get("Content-Length", 0))).decode()
        field = {"document": "document", "css": "css", "head": "head",
                 "body": "body", "bodyend": "body_end"}.get(url.path.split("/")[-1])
        with MOCK.cond:
            if url.path == "/__mock/pages":
                try:
                    MOCK.stage_pages(json.loads(body))
                except (ValueError, KeyError, TypeError):
                    return self.send(400, "invalid mock pages", "text/plain")
            elif url.path == "/__mock/asset":
                MOCK.assets[query["path"][0]] = body
            elif url.path == "/__mock/image":
                MOCK.images[query["name"][0]] = body
            elif field and url.path.startswith("/__mock/"):
                setattr(MOCK, field, body)
            else:
                return self.send(404, "unknown control", "text/plain")
        self.send(200, "ok", "text/plain")

    def do_GET(self):
        url = urlsplit(self.path)
        path = unquote(url.path)
        query = parse_qs(url.query, keep_blank_values=True)
        if path.startswith("/__mock/"):
            return self.control(path, query)
        with MOCK.cond:
            MOCK.log.append(f"GET {self.path} [{self.headers.get('Sec-Fetch-Dest', '-')}]")
            served = MOCK.served
            state = MOCK.hello()
            fail_root = MOCK.fail_root
            early = MOCK.early
        if path == "/__writer/events":
            return self.events()
        if path == "/favicon.ico":
            return self.send(204, "", "image/x-icon")
        if path == "/":
            if fail_root:
                return self.send(503, "deliberate content failure", "text/plain")
            if query.get("view", ["web"])[0] == "pages":
                page = PAGES_LIVE if query.get("client", ["0"])[0] == "1" else PAGES
            elif served:
                page = PAGE.format(**served)
                early = served["early"]
            else:
                page = fallback(state["web"]["error"])
            if early:
                tag = '<script src="/__writer/live-reload.js"></script>'
                page = page.replace(tag, "").replace("<head>", "<head>" + tag, 1)
            return self.send(200, page, "text/html; charset=utf-8")
        if path == "/__writer/state":
            return self.send(200, json.dumps(state), "application/json")
        if path == "/__writer/pages.js":
            return self.send(200, PAGES_SCRIPT.read_text(), "text/javascript")
        match = re.fullmatch(r"/__writer/pages/([1-9][0-9]*)\.svg", path)
        if match:
            index = int(match.group(1)) - 1
            digest = query.get("v", [""])[0]
            with MOCK.cond:
                image = MOCK.page_versions.get(digest)
                immutable = image is not None
                if image is None and index < len(MOCK.page_data["hashes"]):
                    digest = MOCK.page_data["hashes"][index]
                    image = MOCK.page_versions[digest]
                held = MOCK.page_holds.get(digest)
            if image is None:
                return self.send(404, "no such page", "text/plain")
            # Wait outside cond: release/reset and other SSE/control requests
            # must remain available while one image response is held.
            if held and not held.wait(10):
                return self.send(504, "mock page hold timed out", "text/plain")
            return self.send(200, image, "image/svg+xml", (
                "public, max-age=31536000, immutable" if immutable else "no-store"
            ))
        if path == "/__writer/theme.css" and served:
            return self.send(200, served["css"], "text/css")
        if path == "/__writer/live-reload.js":
            morphdom = ASSETS / "vendor/morphdom/morphdom-umd.min.js"
            script = "\n".join([
                "/* morphdom 2.7.8 (MIT), vendored; see THIRD-PARTY-LICENSES */",
                "(function (define, module, exports) {", morphdom.read_text(),
                "}).call(window);", CLIENT.read_text(),
            ])
            return self.send(200, script, "text/javascript")
        if path.startswith("/__writer/doc-asset/"):
            with MOCK.cond:
                image = MOCK.images.get(path.split("/")[-1])
            if image:
                return self.send(200, image, "image/svg+xml")
        if path.startswith("/__writer/asset/"):
            relative = path.removeprefix("/__writer/asset/")
            target = (ASSETS / relative).resolve()
            if relative.split("/")[0] in ("vendor", "writer") and ASSETS in target.parents:
                if target.is_file():
                    mime = mimetypes.guess_type(target.name)[0]
                    return self.send(200, target.read_bytes(), mime or "application/octet-stream")
            if served and relative in served["assets"]:
                mime = mimetypes.guess_type(relative)[0] or "text/plain"
                return self.send(200, served["assets"][relative], mime)
        self.send(404, "not found", "text/plain")

    def control(self, path, query):
        with MOCK.cond:
            if path == "/__mock/pages/hold":
                digest = query.get("hash", [""])[0]
                MOCK.page_holds[digest] = threading.Event()
                return self.send(200, "ok", "text/plain")
            elif path == "/__mock/pages/release":
                held = MOCK.page_holds.pop(query.get("hash", [""])[0], None)
                if held:
                    held.set()
                return self.send(200, "ok", "text/plain")
            elif path == "/__mock/emit":
                event = MOCK.emit(query.get("kind", ["content"])[0], query)
                return self.send(200, json.dumps(event), "application/json")
            if path == "/__mock/document":
                return self.send(200, MOCK.document, "text/html")
            if path == "/__mock/log":
                entries, MOCK.log = MOCK.log, []
                return self.send(200, json.dumps(entries), "application/json")
            if path == "/__mock/reset":
                MOCK.drop_clients()
                MOCK.reset()
            elif path == "/__mock/restart":
                MOCK.build_id = 1
                MOCK.events = []
                MOCK.views = {"web": {"status": "ok"}, "pages": MOCK.pages("ok")}
                MOCK.served = MOCK.snapshot()
                MOCK.drop_clients()
            elif path == "/__mock/down":
                MOCK.down_until = time.monotonic() + int(query.get("ms", ["3000"])[0]) / 1000
                MOCK.drop_clients()
            elif path == "/__mock/early":
                MOCK.early = query.get("on", ["1"])[0] == "1"
            elif path == "/__mock/failroot":
                MOCK.fail_root = query.get("on", ["1"])[0] == "1"
            elif path == "/__mock/title":
                MOCK.title = query.get("t", [""])[0]
            elif path == "/__mock/lang":
                MOCK.lang = query.get("l", ["en"])[0]
            else:
                return self.send(404, "unknown control", "text/plain")
        self.send(200, "ok", "text/plain")

    def events(self):
        with MOCK.cond:
            if time.monotonic() < MOCK.down_until:
                return self.send(503, "deliberate outage", "text/plain")
            generation = MOCK.generation
            seen = len(MOCK.events)
            hello = MOCK.hello()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        try:
            self.wfile.write(f"event: hello\ndata: {json.dumps(hello)}\n\n".encode())
            self.wfile.flush()
            while True:
                with MOCK.cond:
                    MOCK.cond.wait_for(
                        lambda: MOCK.generation != generation or len(MOCK.events) > seen, 15
                    )
                    if MOCK.generation != generation:
                        return
                    fresh, seen = MOCK.events[seen:], len(MOCK.events)
                chunk = "".join(
                    f"event: build\ndata: {json.dumps(event)}\n\n" for event in fresh
                ) or ": keep-alive\n\n"
                self.wfile.write(chunk.encode())
                self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--port", type=int, default=8765)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.daemon_threads = True
    print(f"mock writer serve on http://127.0.0.1:{server.server_address[1]}/", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
