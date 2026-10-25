# Spec: Live reload (`writer serve`)

**Status:** contract. Implemented by T20 (watch set, watcher, classification, `build --watch`),
T21 (server, routes, events, state, the page-set types and the pages shell), T22 (client
`assets/writer/live-reload.js`, `docs/dev/live-reload-qa.md`, the mock server), T23 (`serve`
command, session and rebuild loop, integration tests), T24 (pages view: `PagesBuilder` in
`src/serve/pages.rs`, `assets/writer/pages.js`; the `build --watch` `pdf` test, §9), T73 (error
payload quality). Consumers: T16 and
T32 (provide the pipeline functions of §5.2), T03 and typst-dialect.md (the serve World of §5.3),
T14/T15 and html-output.md (the serve-mode document hooks of §1.2), T1A and math.md (the
KaTeX guard of §3.4), T25 (browser tests), T52 (`.typ` inputs), T74 (guide).

Lifted from presenter_rs (PLAN §6 step 6; R8 §3.5), `/__presenter/` → `/__writer/`: `src/serve/
{server,events,files,watch}.rs`, `src/cli/serve.rs`, `assets/presenter/live-reload.js`,
`docs/dev/live-reload-qa.md`, `tests/fixtures/live-reload/*`, `tests/{server,watch,cli_serve}.rs`.
presenter's live-reload.md (Host checks, versioned theme assets, SSE, lifecycle, watch mechanics,
debouncing) applies unless this document says otherwise. New: two views (`web`, the HTML;
`pages`, the typeset pages as SVG), per-view status in every event, a morphdom update that keeps
the reader's place, and one long-lived typst World per session.

## 1. Routes — T21 (page bodies: T24)

axum 0.8. Every route answers `403` to a disallowed `Host` (§1.1); unknown paths and ids → `404`.

| route | response | `Cache-Control` |
|---|---|---|
| `GET /` | `?view=pages` → the pages shell (§4.2); `?view=web` → the web document; no or another `view` value → the session's default view (`--view`, `[serve] view`, else `pages` for `.typ` inputs and `web` otherwise; cli.md). The web document is the latest good serve-mode HTML (§1.2), or the fallback page (§1.4) while no web build has succeeded. `text/html; charset=utf-8` | `no-store` |
| `GET /__writer/theme.css` | theme stylesheet of the latest good web build, `text/css; charset=utf-8` | `no-store` |
| `GET /__writer/asset/{*path}` | `vendor/…`, `writer/…`, `@<theme>/…` or the chain's `assets/` (lifted `resolve_helper_path`); a theme-chain stylesheet has its relative `url()`s rewritten to versioned URLs (lifted) | `max-age=3600` for `vendor/…`, else `no-store` |
| `GET /__writer/doc-asset/{id}` | asset `id` of the latest good web build (`AssetRegistry`; file assets streamed; single byte ranges → `206`/`416`, lifted `files.rs`); the `v` query is ignored | `no-store` |
| `GET /__writer/events` | `text/event-stream`: `hello` at once, then one `build` per build attempt (§2); keep-alive comment every 15 s | axum default (`no-cache`) |
| `GET /__writer/live-reload.js` | the morphdom wrapper plus the client (§1.3), `text/javascript; charset=utf-8` | `no-store` |
| `GET /__writer/pages` | the pages shell (§4.2) | `no-store` |
| `GET /__writer/pages/{file}` | `file` = `<n>.svg`, `n` matching `[1-9][0-9]*` (1-based page number); lookup in §4.1; `image/svg+xml` | §4.1 |
| `GET /__writer/pages.js` | `assets/writer/pages.js`, `text/javascript; charset=utf-8` | `no-store` |
| `GET /__writer/state` | the `hello` object (§2.2) as JSON | `no-store` |
| `GET /favicon.ico` | `204 No Content` | — |

Before any good web build `/__writer/theme.css` is `404`. A missing embedded `writer/pages.js` is
`500` with `embedded writer/pages.js is missing` (§1.3); T21 creates `assets/writer/pages.js` as
the one-line placeholder `/* pages view: T24 */`, which T24 replaces.

### 1.1 Host check and versioned theme assets (lifted verbatim)
Allowed `Host`s (`AllowedHosts`): the listening address (any IP when `0.0.0.0`/`[::]`), `localhost`,
`127.0.0.1`, `[::1]` and the `--host`/`[serve] host` value, each with the bound port (none for
80); IPv4-mapped IPv6 = IPv4; no `Host` passes; anything else → `403` on every route. Theme-file
URLs carry `?v=<content version>` (first 8 bytes of the file's BLAKE3 hash in decimal, remembered
while size and mtime are unchanged); vendored files and `writer/…` files are not versioned.

### 1.2 What the protocol requires of the serve-mode document
html-output.md owns the markup; the client relies on exactly these hooks (T15/T14 emit them in
serve mode; the position attributes also with `[markdown] sourcepos = true`):
1. `<link id="writer-theme" rel="stylesheet" href="/__writer/theme.css?v=<build_id>">` (markup and
   attribute order: html-output.md §4.2), `<build_id>` = the build that rendered the page (`Mode::Serve`).
2. Exactly one element with the attribute `data-writer-document`: the morph root, containing
   everything document-derived (title block, abstract, TOC, body, footnotes, bibliography).
3. The client script list (html-output.md §4.2 `live_reload()`) is exactly
   `["/__writer/live-reload.js"]`, i.e. `<script src="/__writer/live-reload.js"></script>`.
4. Position attributes on blocks: `data-sourcepos="<start>-<end>"` (1-based lines, inclusive,
   regex `^\d+-\d+$`), `data-source="<n>"` (`0` = the main input, `k ≥ 1` = `Document.includes[k-1]`),
   and on notebook blocks `data-cell="<c>"` (`SourceSpan.cell`, 0-based, ir.md).
5. KaTeX elements carry `data-tex="<source>"`; `math.js` sets `data-rendered="1"` after rendering
   and exposes `window.__writerRenderMath(root)` (presenter's `__presenterRenderMath`; math.md).
6. URLs `/__writer/doc-asset/<id>?v=<content_version>` and `/__writer/asset/<path>?v=<n>`, `v` ∈ `[0-9A-Za-z]+`.

### 1.3 `/__writer/live-reload.js`
The body is exactly: the line `/* morphdom 2.7.8 (MIT), vendored; see THIRD-PARTY-LICENSES */`,
`(function (define, module, exports) {`, the bytes of the embedded vendored morphdom UMD file
(`vendor/morphdom/morphdom-umd.min.js`, path per vendored-assets.md), `}).call(window);`, then
the bytes of `writer/live-reload.js`, lines joined with `\n`. The wrapper hides AMD/CommonJS
loaders (a notebook output may define `define`), so the UMD sets `window.morphdom`. A missing
embedded file is `500` with `embedded <key> is missing` (lifted).

### 1.4 Fallback page
While no web build has succeeded, `GET /` (web) is presenter's `fallback_page` for the error case
only: title `writer: build error`, lead `The document could not be built. Fix the error and save:
this page reloads by itself.`, the client script tag, and `#writer-overlay` pre-rendered with the
current errors in the client's format (§3.6). presenter's `building` variant is dropped: build 1
finishes before the port is bound (§5.5). The page has no `[data-writer-document]`, so the first
good build reloads it (§3.4 step 2).

## 2. Events — T21 (types), T23 (values), T73 (payload quality)

### 2.1 Wire format
```
event: build
data: {"build_id":18,"status":"ok","kind":"content","duration_ms":84,"changed":{"file":"chapters/method.md","source":2,"line":41},"web":{"status":"ok"},"pages":{"status":"ok","count":2,"hashes":["9c1e0b7a44d2f310","7f30c2d9e1a05b68"],"size_pt":[[595.28,841.89],[595.28,841.89]]},"warnings":[{"level":"warning","code":"unresolved-reference","message":"label `<fig-x>` does not exist in the document","file":"document.md","line":12,"cell":null}]}

event: build
data: {"build_id":19,"status":"error","kind":"content","duration_ms":31,"changed":{"file":"document.md","source":0,"line":7},"web":{"status":"ok"},"pages":{"status":"error","error":{"message":"document.md:7: unknown variable: foo","file":"document.md","line":7,"cell":null,"kind":"compile","excerpt":"#foo()"},"count":2,"hashes":["9c1e0b7a44d2f310","7f30c2d9e1a05b68"],"size_pt":[[595.28,841.89],[595.28,841.89]]},"warnings":[]}
```
(A `css` build after build 19 reports `"pages":{"status":"skipped",…}` with the same set and
`"status":"error"`, because the pages slot still holds the error.) `hello` is the same object
without `kind`, `duration_ms` and `changed`. One `data:` line per event (compact JSON). A browser
that lags more than `EVENT_CAPACITY` (64) events skips the oldest (lifted). Messages in these
examples are illustrative; the owning specs fix their text (here references.md §8, hint omitted).

### 2.2 Types (`src/serve/events.rs`)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "lowercase")] pub enum Status { Ok, Error }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "lowercase")] pub enum ViewStatus { Ok, Error, Skipped }
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WebView { pub status: ViewStatus, #[serde(skip_serializing_if = "Option::is_none")] pub error: Option<ErrorPayload> }
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PagesView { pub status: ViewStatus, #[serde(skip_serializing_if = "Option::is_none")] pub error: Option<ErrorPayload>,
    pub count: usize, pub hashes: Vec<String>, pub size_pt: Vec<[f64; 2]> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Changed { pub file: String, pub source: usize,       // exactly one of line / cell is Some
    #[serde(skip_serializing_if = "Option::is_none")] pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")] pub cell: Option<usize> }
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuildEvent { pub build_id: u64, pub status: Status, pub kind: ChangeKind, pub duration_ms: u64,
    pub changed: Option<Changed>, pub web: WebView, pub pages: PagesView, pub warnings: Vec<DiagnosticJson> }
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hello { pub build_id: u64, pub status: Status, pub web: WebView, pub pages: PagesView, pub warnings: Vec<DiagnosticJson> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorPayload { pub message: String, pub file: Option<String>, pub line: Option<usize>, pub cell: Option<usize>, pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub excerpt: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")] pub hints: Vec<String> }
impl ErrorPayload { pub fn new(err: &Error, base: Option<&Path>, input: Option<&Path>) -> Self;  // lifted, + cell, hints
                    pub fn for_input(err: &Error, input: &Path) -> Self;                         // new(err, cwd, Some(input))
                    pub fn for_failure(failure: &BuildFailure, input: &Path) -> Self; }          // for_input + §2.4 compile location
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticJson { pub level: &'static str, pub code: String, pub message: String,
    pub file: Option<String>, pub line: Option<usize>, pub cell: Option<usize> }
impl DiagnosticJson { pub fn new(diagnostic: &Diagnostic, base: Option<&Path>) -> Self; }
```

| field | rule |
|---|---|
| `build_id` | one per attempt, failed ones included; 1 is the first build (before bind); `hello` carries the latest |
| `status` | `"error"` iff, after this build, the web slot or the pages slot holds an error (a skipped view keeps its earlier state) |
| `kind` | §2.5 |
| `duration_ms` | wall time from the start of the rebuild (after debouncing) to `apply_outcome` |
| `changed` | §2.3; `null` for `css` and `pages` builds and when no document source text changed |
| `web.status` | `ok`: rebuilt and now served; `error`: this attempt failed, the last good document stays served, `error` set; `skipped`: not rebuilt by this build (`pages` kind) |
| `pages.status` | same meanings; `skipped` for `css` builds |
| `pages.count/hashes/size_pt` | always the **served** page set (the last good one; empty lists and `0` before any success), also when `status` is `error` or `skipped`; `hashes[i]` and `size_pt[i]` belong to page `i+1` |
| `warnings` | §2.6 |
| `hello` | the latest attempt's state: `web`/`pages` status are `ok` or `error` (never `skipped`), with the slot's error; `GET /__writer/state` returns the same object |

### 2.3 How the server computes `changed` (T23)
The session keeps `sources: BTreeMap<PathBuf, String>`, the text of every document source as it
last read it, and `text_sources: Vec<PathBuf>`, the main input followed by `Document.includes`
of the last build whose `prepare` succeeded (canonical paths). A document source is a file with
extension `md`, `markdown`, `ipynb` or `typ`. At the start of a `content` rebuild:
1. For each path of `text_sources` (in that order) that is in the batch: read it (unreadable →
   skip, keep the stored text), compare with `sources[path]`, store the new text. The first file
   that gives a position (rules 2–3) wins; the remaining files are still read and stored. A path
   without a stored text is read and stored and gives no position.
2. Text files: when `old.lines()` equals `new.lines()` (Rust `str::lines`) the file gives no
   position (CRLF↔LF or final-newline-only edits are no change); otherwise `i` = first index where
   they differ, or the shorter length when one is a prefix of the other; `line = min(i + 1,
   max(1, new line count))`.
3. Notebooks: parse both as `serde_json::Value`; `cells` arrays; equal arrays (`==`: only
   notebook metadata changed, or the same cells were rewritten) give no position; otherwise
   `i` = first index whose cell objects differ (whole-object equality), or the shorter length
   when one is a prefix; clamp to `new.len() - 1`; `cell = i`. A side that does not parse as
   JSON, or an empty new `cells`, gives no position for that file.
4. After the build, the position becomes `Changed { file, source, line | cell }` when `prepare`
   succeeded and the file is the main input (`source: 0`) or the new document's include with
   `canonical(Document.includes[k-1]) == path` (`source: k`, §6.4); otherwise `changed` is `null`.
   `file`: relative to the current directory when below it, else absolute, `/`-separated (lifted `display_path`).
5. After every build, each document source of the new document without a stored text is read
   and stored (a new include gets a baseline; build 1 only stores).

### 2.4 Error payload (T21 lifted; T73 completes)
`ErrorPayload::new` is presenter's, plus `cell` and `hints` (`Error::Compile { hints }`). `cell`
(`line` is then the line within the cell): `Error::MissingAsset`'s `span.cell`; for `Error::Compile`
(no span, ir.md §5) `for_failure` takes `file`, `line` and `cell` from `path`/`span` of the first
`Level::Error` diagnostic of `failure.diagnostics` (§4.1); else `null` (a notebook parse error
names its cell in `message`, notebook-mapping.md §1). `kind`: the `Error` variant in kebab-case
(`parse`, `front-matter`, `config`, `theme`, `template`, `style`, `missing-asset`, `compile`,
`resource`, `io`, `json`, `other`); a located error's `message` is `<file>:<line>: <message>`
(lifted; `<file> cell <c>:<line>: ` with a cell). T73 makes compile errors exact: generated typst
lines map to the Markdown/notebook line through the source map (T30 emits it, T03's
`map_diagnostic` applies it, typst-dialect.md §8.2) and are quoted from that source; an error in a
package is located at the outermost `diag.trace` span in the user's file, the package span
becoming the hint typst-dialect.md §8.2 defines (e.g. `in @preview/mitex:0.2.7/mitex.typ:26`);
`excerpt` is the offending line (lifted `Error::excerpt`). A failure shared by both views (config,
parse) is one payload; for a `.typ` input a `prepare` failure is the web view's only (§5.4 step 3,
cli.md).

### 2.5 `kind`
From the batch's classification (§6.2): `css` (web rebuilt, pages skipped), `pages` (pages
rebuilt, web skipped), `content` (both). Lifted upgrade: a `css` build whose serve-mode document
changed a versioned `/__writer/asset/…?v=` URL is reported as `content` (pages stay skipped).
Build 1 is `content`. On a reconnect the client applies `hello` as a `content` build (§3.2); the
server does not rebuild.

### 2.6 `warnings`
Every diagnostic with level `Warning` or `Error` that did not stop a view (in serve, error-level
diagnostics such as `duplicate-label` do not fail a view, except that a subset error, the first
`Level::Error` diagnostic whose code starts with `E-TYP-` or `W-TYP-` (`subset_error`, §5.2;
typst-dialect.md §2.1), fails the web view through `web_page`'s `Err`; otherwise only an `Err`
from a stage does; the one a payload was built from, the compile location of §2.4 or that subset
error, is not repeated: T23 drops it from the prepare list), as `DiagnosticJson`, whose keys and
value rules equal cli.md's `check --format json` object: `level` `"warning"`/`"error"`, `file` =
`span.file` when set, else `Diagnostic.path`, displayed like `ErrorPayload.file`; `line` =
`span.start_line`, `cell` = `span.cell`, absent values `null`. Kept per stage (§5.1): `prepare`'s,
the web build's (`WebPage.diagnostics`), the pages build's (compiler warnings as T03's
`map_diagnostic` converts them, typst-dialect.md §8.2); a stage that did not run keeps its list (a
`css` build keeps the pages warnings; a failed `prepare` of a `.md`/`.ipynb` input keeps the web
and pages lists). A stage that ran and failed stores the `Warning`/`Error` entries of its
`BuildFailure.diagnostics` minus the one its payload took the location from (§2.4);
`Outcome.{prepare,web,pages}_warnings` is `Some` for every stage that ran, failed or not.
`warnings` = prepare ++ web ++ pages, exact duplicates (all six fields) dropped. `Level::Info` is
never sent (stderr shows it with `-v`).

## 3. Client — T22 (`assets/writer/live-reload.js`)

Plain ES2020 in one IIFE with `'use strict'`; no dependency besides `window.morphdom` (§1.3).
Lifted from presenter's client, keeping its structure and names (`getDoc`, `signature`, `store`,
`fullReload`, `swapLink`, `swapCss`, `outside`, `refreshOutside`, `swapContent`, `apply`,
`onHello`, `onBuild`, `handler`, `disconnect`, `connect`), with reveal.js removed.

### 3.1 State, storage, debugging
- `state = { buildId, connected, retryMs, lastStatus /* ok|error|stale */, errorId, view /* web|pages */,
  views: { web: {status, error}, pages: {status, error} }, warnings: [] }`;
  `window.__writerLiveReload = { connect, disconnect, state, setView }`.
- `view` = `'pages'` when the page has `[data-writer-pages]`, else `'web'`.
- sessionStorage keys (every access in `try`/`catch`): `'writer:build_id:' + location.pathname +
  location.search` (lifted), `'writer:view'`, `'writer:warnings-open'`.
- Logging only via `console.debug('[writer]', …)`; no handler ever throws (lifted `handler`
  wrapper; a failed update falls back to `fullReload`). The web view sets
  `document.documentElement.style.overflowAnchor = 'none'` once (no browser scroll anchoring).

### 3.2 Connection, `hello`, reconnect
`connect()` opens `EventSource('/__writer/events')`; on `error` it retries after 0.5 s, doubling
to at most 5 s (lifted). `onHello(d)`: `again = state.buildId !== null`; `prev = state.buildId ??
stored id`; reset `retryMs`; then
1. `prev !== null && +prev !== d.build_id`: when `again`, apply `{ ...d, kind: 'content', changed:
   null }` as a build (§3.3: in-place, no reload); otherwise `fullReload` (lifted; the id is stored
   first so the reloaded page does not reload).
2. else, when `again`: apply `{ ...d, kind: 'content', changed: null }` (the server may have
   restarted with the same id; lifted).
3. else (fresh page): store the id; copy `d.web`/`d.pages` into `state.views`; render the overlay
   (§3.3 step 1), warnings and badges; then, in the web view, when `d.web.status === 'ok'` and
   `#writer-theme` is missing or its `?v=` differs from `d.build_id` (an older build rendered the
   page), apply `{ ...d, kind: 'content', changed: null }` in place (a no-op morph after pages-only builds).

### 3.3 Applying a build (`onBuild` queues `apply(d)` on one promise chain, lifted)
1. Update `state.views.web` and `state.views.pages` from `d` for every view whose status is not
   `skipped`; re-render the overlay's content, warnings panel and toggle badge. The overlay is
   hidden when no view has an error; it is made visible only when a view of `d` has status `error`
   and `d.build_id !== state.errorId`, which sets `state.errorId = d.build_id`; a click hides it
   and leaves `errorId` unchanged (a reconnect to the same failing build keeps it dismissed).
2. Only when `state.view === 'web'`: if `d.web.status === 'ok'`: when `d.kind !== 'css' ||
   state.lastStatus !== 'ok'`, `await swapContent(d.build_id, d.changed)` and stop on `false` (id
   not stored); then `await swapCss(d.build_id)` (content builds can change it too) and stop on
   `false`; set `lastStatus = 'ok'`. If `d.web.status === 'error'`: `lastStatus = 'error'`. If
   `skipped` and `lastStatus === 'stale'`: stop without storing.
3. Only when `state.view === 'pages'`: if `d.pages.status !== 'skipped'`, call
   `window.__writerPages.apply(d.pages, d.build_id)` (log when `__writerPages` is missing).
4. `store(d.build_id)` (only once applied, lifted).

### 3.4 Content morph (`swapContent(id, changed)`)
1. `next = await getDoc()` where `getDoc` fetches `'/?view=web'` with `cache: 'no-store'`; on
   failure `lastStatus = 'stale'`, return `false` (lifted).
2. `fresh = next.querySelector('[data-writer-document]')`, `root =
   document.querySelector('[data-writer-document]')`; either missing, or `typeof window.morphdom
   !== 'function'` → `fullReload('cannot update in place')`.
3. `sig = signature(next)`: clone `documentElement`; remove `title` and `meta`; in the clone's
   root remove all children and every attribute except `data-writer-document`; `outerHTML` with
   `/\?v=[0-9A-Za-z]+/g` removed. `sig !== baseline` or `fresh.querySelector('script')` →
   `fullReload('page changed outside the document')`. The baseline is computed as in presenter:
   from the page's own `GET /?view=web` at load when its `#writer-theme` href equals the live
   one, else `''`.
4. `anchor = takeAnchor(root)` (below).
5. In `try`: `morphdom(root, fresh, { childrenOnly: true, onBeforeElUpdated: guard })`; copy the
   root's attributes from `fresh` (set changed ones, remove missing ones); if `window.katex`,
   `window.__writerRenderMath?.(root)`. An exception → `fullReload`.
6. `restoreAnchor(anchor)`, then `scrollToChanged(root, changed)`, `refreshOutside(next)`
   (lifted: `?v=` of theme files loaded outside the root, scripts excluded), `document.title =
   next.title` when different, `baseline = sig`, return `true`.

`guard(from, to)`, in this order:
1. `from.tagName === 'DETAILS'` → `to.toggleAttribute('open', from.open)` (the reader's choice wins).
2. `from.isEqualNode(to)` → `false` (skip the unchanged subtree).
3. KaTeX: `from.dataset.rendered === '1'` and `from.getAttribute('data-tex') ===
   to.getAttribute('data-tex')` → `syncAttrs(from, to, ['data-rendered'])`, `false` (the rendered
   node is kept; only position attributes change).
4. `from.tagName === 'IMG'` and equal `src` attributes → `syncAttrs(from, to, [])`, `false` (an
   image is re-requested only when its `?v=` changed).
5. otherwise `true`. `syncAttrs(from, to, keep)` sets every attribute of `to` whose value differs
   on `from` and removes every attribute of `from` that `to` lacks, except the names in `keep`.

Anchor keeping. `takeAnchor(root)`: `list = root.querySelectorAll('[data-sourcepos]')` in
document order; the anchor is the first element with `0 <= rect.top < innerHeight`, else the
last with `rect.top < 0 < rect.bottom`, else none. Record `{ el, top: rect.top, index (in list),
tag, source, cell, text: el.textContent.slice(0, 200) }`. `restoreAnchor(a)`: in the new list,
among elements with the same `tagName`, `data-source`, `data-cell` and text prefix, take the one
whose index is closest to `a.index`; none → `a.el` if it is still inside `root`; else the element
at `min(a.index, list.length - 1)`; then `scrollBy(0, target.getBoundingClientRect().top - a.top)`.
`restoreAnchor(null)` and an empty new list do nothing (no scroll).

`scrollToChanged(root, c)`: `c` null → nothing. With `c.cell != null`: the first `[data-source="<s>"]
[data-cell="<c>"]`. With `c.line != null`: among `[data-source="<s>"][data-sourcepos]` without
`data-cell`, those whose range covers the line, the one with the smallest `end - start` (ties:
the last in document order); none → the one with the largest `start <= line`; none → nothing.
The element is scrolled with `scrollIntoView({ block: 'center' })` only when it is off screen
(`rect.bottom < 0 || rect.top > innerHeight`).

### 3.5 CSS swap (lifted)
`swapCss(id)` swaps `link#writer-theme` to `/__writer/theme.css?v=<id>` and re-requests every
`link[rel~=stylesheet][href^="/__writer/asset/"]` not under `/__writer/asset/vendor`, adding the
new `<link>` after the old one and removing the old one only once the new one has loaded (no
flash). No `#writer-theme` → `fullReload`. The fallback page and the pages shell never call it.

### 3.6 Overlay, badges, warnings panel
- `#writer-overlay` (presenter's style string, top right, ≤ 50 % width, monospace, z-index 10000;
  click hides it). Errors come from `state.views`; identical payloads (JSON-equal) are merged.
  One block per payload, the current view's first, blocks separated by an empty line:
  `[<views>] build error (<kind>)` (`<views>` = `web`, `pages` or `web, pages`), the `message`,
  the location (`<file>` + ` cell <cell>` when set + `:<line>` when set; omitted without `file`),
  the `excerpt` in a `<pre>` (colour `#fc6`), one `hint: <text>` line per hint; then
  `(click to dismiss)`. No error → hidden. The fallback page (§1.4) renders the same text.
- `#writer-view-toggle`: a `<button type="button">` appended to `body`, fixed bottom right (z-index
  9999), label `Pages` in the web view and `Web` in the pages view, `title="Switch view (p)"`; when
  the other view's state is `error` it gets `data-status="error"`, a red border and the suffix ` ●`.
- `#writer-warnings`: fixed bottom left; hidden when `warnings` is empty; collapsed it shows the
  non-zero counts `<n> warning(s)` and `<m> error(s)` joined by `, `; a click expands an
  `<ol>` of `<code>: <message> — <file>[ cell <c>]:<line>` (max-height 40vh, scrolls); the
  expanded state persists in `writer:warnings-open`. Updated on every `hello` and `build`.

### 3.7 View toggle
- `setView(v)`: store `v` in `writer:view`, then `location.assign('/?view=' + v)`.
- The button and the `p` key (keydown, no Ctrl/Meta/Alt, not `defaultPrevented`, target not an
  `input`, `textarea`, `select` or `isContentEditable`) call `setView(other)`.
- On load of `/` without a `view` parameter, when `writer:view` holds the other view:
  `location.replace('/?view=' + stored)`. A `view` parameter is stored in `writer:view`.

## 4. Pages view — T24 (types, shell and route: T21)

### 4.1 Page sets and the SVG route (`src/serve/pages.rs`)
```rust
pub struct PageSvg { pub hash: String, pub svg: axum::body::Bytes, pub size_pt: [f64; 2], pub key: u128 }
pub struct PageSet { pub build_id: u64, pub pages: Vec<PageSvg> }
impl PageSet { pub fn find(&self, hash: &str) -> Option<&PageSvg>; pub fn hashes(&self) -> Vec<String>; pub fn sizes(&self) -> Vec<[f64; 2]>; }
pub fn page_hash(svg: &[u8]) -> String;            // first 16 lowercase hex chars of blake3::hash(svg)
pub fn shell_html(title: &str) -> String;          // §4.2
pub enum PagesInput<'a> { Generated { prepared: &'a Prepared, opts: &'a BuildOptions }, TypstFile { path: &'a Path } }
pub struct PagesBuilder { /* the previous PageSet, for reuse */ }
impl PagesBuilder {
    pub fn new() -> Self;
    /// Compiles with the session's World (§5.3) and exports every page.
    pub fn build(&mut self, world: &mut WriterWorld, input: PagesInput<'_>, build_id: u64)
        -> Result<(Arc<PageSet>, Diagnostics), BuildFailure>;
}
```
- T21 writes the types, `find`/`hashes`/`sizes`, `page_hash`, `shell_html`, and `build` as a stub
  returning `Ok((Arc::new(PageSet { build_id, pages: Vec::new() }), Diagnostics::default()))` (pages
  ok, `count` 0, so T23's tests pass without T24); T24 replaces the body.
- Compile (T24): `Generated` → `load_typst_main(prepared, opts, world)` (§5.2); then
  `crate::typst::compile::compile_document(world)`; no output → `Err(BuildFailure { error, diagnostics })`
  (`into_result`'s `Error::Compile`; all the compile's diagnostics, mapped by `map_diagnostic`,
  typst-dialect.md §8.2), else its warnings plus `world.include_diagnostics()`. Serve never uses
  `compile_pages` or `crate::typst::compile::PageSvg`; the wire hash is always `page_hash`.
- Export: for each page of the `PagedDocument`, `key = typst::utils::hash128(page)`
  (`typst_layout::Page: Hash`); a page whose key equals a page of the previous set reuses that
  `PageSvg` (bytes and hash, no export); else `svg = typst_svg::svg(page,
  &SvgOptions::default())`, `hash = page_hash(svg)`. `size_pt = [w, h]` of `page.frame.width()/
  height().to_pt()`, each rounded to 2 decimals. Never `svg_merged`, never inline SVG in HTML.
- The server keeps the current set and the previous one (`PagesSlot`, §5.1). Lookup for
  `/__writer/pages/<n>.svg?v=<h>`: `h` equals the hash of a page in the current or the previous
  set → those bytes, `200`, `Cache-Control: public, max-age=31536000, immutable`; else page `n`
  of the current set → its bytes with `no-store`; else `404`. Every SVG response also carries
  `X-Content-Type-Options: nosniff` and `Content-Security-Policy: default-src 'none'; img-src
  data:; style-src 'unsafe-inline'`. A `file` that is not `<n>.svg` → `404`.

### 4.2 The shell (`shell_html`, T21)
```html
<!DOCTYPE html>
<html lang="en" data-writer-view="pages">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · pages</title><style>{PAGES_CSS}</style></head>
<body>
<header id="writer-pages-toolbar">
<button type="button" data-zoom="out" title="Zoom out (-)">−</button><span id="writer-pages-zoom">Fit</span>
<button type="button" data-zoom="in" title="Zoom in (+)">+</button><button type="button" data-zoom="fit" title="Fit width (0)">Fit</button>
<span id="writer-pages-counter"></span><span id="writer-pages-status" hidden></span>
<span class="writer-pages-note">Visual preview of the PDF pages: text cannot be selected and links do not work.</span>
</header>
<main id="writer-pages" data-writer-pages></main>
<script src="/__writer/pages.js"></script>
<script src="/__writer/live-reload.js"></script>
</body>
</html>
```
`{title}` = `DocMeta.title` of the latest successful `prepare`, else the input's file name,
HTML-escaped. `PAGES_CSS` (a constant): grey page background (dark under
`prefers-color-scheme: dark`), the toolbar fixed at the top, `#writer-pages` one centred column
with 16 px gaps and 16 px side padding, white page backgrounds with a shadow.

### 4.3 `assets/writer/pages.js` (T24)
- Exposes `window.__writerPages = { apply(pages, buildId), state }`; on load fetches
  `/__writer/state` (`cache: 'no-store'`) and applies its `pages` with its `build_id`. `apply`
  called from live-reload.js is always applied (its queue orders the calls; ids restart at 1
  when the server restarts); the response of pages.js's own `/__writer/state` fetch is ignored
  when `apply` was called since that fetch started. Never throws; logs with `[writer]`.
- DOM per page `i` (0-based): `<div class="writer-page" data-page="i+1" data-hash="…"><img
  alt="Page i+1" loading="lazy" decoding="async" draggable="false" width="W" height="H"
  src="/__writer/pages/<i+1>.svg?v=<hash>"></div>`, with `W`/`H` = `round(size_pt * 4/3)`; the
  image's CSS width and height are `size_pt * 4/3 * zoom` px (1 pt = 4/3 CSS px), so the layout
  is final before any image loads.
- `apply(p, id)`: record the anchor page (first page whose `rect.bottom > 0`; offset fraction
  `-rect.top / rect.height`); for `i < p.count`, create missing pages, set `src` and `data-hash`
  only where `data-hash !== p.hashes[i]` (unchanged pages are never re-requested; the old image
  stays visible until the new one has loaded), update sizes; remove pages beyond `p.count`; then
  scroll so the anchor page (clamped to the new count) has the same offset fraction.
- `p.status === 'error'`: the (last good) pages stay; `#writer-pages-status` shows `pages: build
  error, showing the last good pages` (`pages: build error, no pages yet` when `count` is 0);
  `ok` hides it. `count` 0 → `<p class="writer-pages-empty">No pages yet.</p>`.
- Zoom levels 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3 and `fit` (default):
  `fit = min(2, max(0.25, (clientWidth - 32) / (max page width in px)))`, recomputed on resize.
  Buttons and keys `+`/`=` (next level), `-` (previous), `0` (fit), with the key rules of §3.7;
  the choice persists in sessionStorage `writer:pages-zoom`; changes keep the anchor page. From
  `fit`, `+` selects the smallest level greater than the current fit factor and `-` the largest
  level smaller than it; at either end nothing changes. `#writer-pages-zoom` shows `Fit` or
  `<round(zoom × 100)>%`.
- `#writer-pages-counter` shows `Page <k> / <count>` for the anchor page, updated on scroll.

## 5. Server runtime and state — T21 (state), T23 (session, CLI), T24 (pages)

### 5.1 State (`src/serve/server.rs`)
```rust
pub const EVENT_CAPACITY: usize = 64;
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
#[derive(Clone)] pub struct WebBuild { pub html: String, pub theme_css: String, pub assets: AssetRegistry, pub theme: Theme }
#[derive(Default)] pub struct WebSlot { pub good: Option<WebBuild>, pub error: Option<ErrorPayload>, pub warnings: Vec<DiagnosticJson> }
#[derive(Default)] pub struct PagesSlot { pub current: Option<Arc<PageSet>>, pub previous: Option<Arc<PageSet>>, pub error: Option<ErrorPayload>, pub warnings: Vec<DiagnosticJson> }
pub struct ServeState { pub build_id: u64, pub default_view: ServeView, pub title: String,
                        pub web: WebSlot, pub pages: PagesSlot, pub prepare_warnings: Vec<DiagnosticJson> }
impl ServeState { pub fn new(default_view: ServeView, title: String) -> Self; pub fn hello(&self) -> Hello;
                  pub fn warnings(&self) -> Vec<DiagnosticJson>; }   // prepare ++ web ++ pages, exact duplicates dropped (§2.6)
pub type Shared = Arc<RwLock<ServeState>>;              // std lock, never held across .await (lifted)
pub enum ViewOutcome<T> { Built(T), Failed(ErrorPayload), Skipped }
pub struct Outcome { pub kind: ChangeKind, pub duration_ms: u64, pub changed: Option<Changed>, pub title: Option<String>,
                     pub web: ViewOutcome<WebBuild>, pub pages: ViewOutcome<Arc<PageSet>>,   // warnings: None = stage did not run (§2.6)
                     pub prepare_warnings: Option<Vec<DiagnosticJson>>, pub web_warnings: Option<Vec<DiagnosticJson>>, pub pages_warnings: Option<Vec<DiagnosticJson>> }
pub fn event_channel() -> broadcast::Sender<BuildEvent>;                          // capacity EVENT_CAPACITY
pub fn publish(events: &broadcast::Sender<BuildEvent>, event: BuildEvent) -> usize;
pub fn next_build_id(state: &Shared) -> u64;                                      // render with this id
pub fn apply_outcome(state: &Shared, outcome: Outcome) -> BuildEvent;            // stores, bumps build_id
pub fn router(state: Shared, events: broadcast::Sender<BuildEvent>) -> Router;
pub async fn serve_until<F>(listener: TcpListener, router: Router, shutdown: F) -> Result<()> where F: Future<Output = ()> + Send + 'static;
pub async fn serve_until_allowing<F>(listener: TcpListener, router: Router, shutdown: F, hosts: AllowedHosts) -> Result<()>
    where F: Future<Output = ()> + Send + 'static;                               // non-allowed Host → 403
impl AllowedHosts { pub fn new(addr: SocketAddr) -> Self; pub fn with_name(self, name: &str) -> Self; pub fn allows(&self, host: Option<&str>) -> bool; }
```
`apply_outcome`: `Built(b)` → `web.good = Some(b)`, `web.error = None`; `Failed(p)` →
`web.error = Some(p)` (the good build stays); `Skipped` → unchanged. Pages: `Built(s)` →
`previous = current.take()`, `current = Some(s)`, `error = None`; `Failed(p)` → `error = Some(p)`;
`Skipped` → unchanged. Then `build_id += 1`; each `Some` warnings list replaces its stored list
(`prepare_warnings`, `web.warnings`, `pages.warnings`); `title` replaced when `Some`; the returned
event follows §2.2 (the views' event status is the outcome's: `ok`, `error`, `skipped`;
`warnings` = `ServeState::warnings`). Before the first build `build_id` is 0 and both slots are empty.

### 5.2 Pipeline functions serve uses (T16 provides them in `src/pipeline/mod.rs`; T32 `load_typst_main`'s body)
```rust
pub enum Mode { Build, Serve { build_id: u64 }, Check }
pub struct BuildOptions { pub input: PathBuf, pub format: Option<SourceKind>, pub config_path: Option<PathBuf>,
                          pub cli: CliOverrides, pub mode: Mode }
pub struct Prepared { pub document: Document, pub assets: AssetRegistry, pub config: Config,
                      pub config_path: Option<PathBuf>, pub diagnostics: Diagnostics }
pub struct BuildFailure { pub error: Error, pub diagnostics: Diagnostics }
pub enum InputRole { Source, Include, Config, Asset, MissingAsset, Theme, Bibliography, PdfTemplate, ReferenceDoc, LatexTemplate }
/// Config, frontend, resolve. Serve: lenient resolve, missing assets are warnings.
pub fn prepare(opts: &BuildOptions) -> Result<Prepared, BuildFailure>;
/// Every input of the build with its role. `Theme` entries are directories: the chain's disk
/// directories (resolved from `prepared.config`, T15), or every directory an unresolved theme may be in (lifted).
pub fn watch_inputs(prepared: &Prepared) -> Vec<(PathBuf, InputRole)>;
pub struct WebPage { pub html: String, pub theme_css: String, pub theme: Theme, pub diagnostics: Diagnostics,
                     pub assets: AssetRegistry /* prepared.assets plus every asset the render registered (equation images) */ }
/// The serve-mode (or build-mode) HTML document; typst math and equation images use `world`.
/// When `subset_error(&prepared.diagnostics)` is `Some(d)` (typst-dialect.md §2.1: `E-TYP-*` errors
/// fail the `html` target; the frontend returned `Ok`, for `E-TYP-SYNTAX` with `blocks: []`), it renders
/// nothing and returns `Err(BuildFailure { error: Error::Parse { path: d.span's file, else d.path, else
/// opts.input; line: d.span's start_line; message: d.message }, diagnostics: Diagnostics::default() })`;
/// serve keeps the last good page (§5.1) and reports `web` error (`kind` `parse`).
pub fn web_page(prepared: &Prepared, opts: &BuildOptions, world: &mut WriterWorld) -> Result<WebPage, BuildFailure>;
/// The first `Level::Error` diagnostic whose code starts with `E-TYP-` or `W-TYP-` (`--strict` /
/// `[typst] strict` raise `W-TYP-*` to errors), in order; `None` otherwise.
pub fn subset_error(diagnostics: &Diagnostics) -> Option<&Diagnostic>;
/// Loads a Markdown/notebook document into `world` exactly as `build --to pdf` does (typst-dialect.md §8.4):
/// `crate::typst::emit::emit` (`AssetPaths::Virtual`), the bibliography/style/template `mount`s, `set_inputs`,
/// `set_assets(prepared.assets.clone())`, `set_main_text(&emitted.text, emitted.map)`. T16 writes it as a stub
/// (`Err`, `pdf: not implemented yet (T32)`); T32 replaces the body and uses it for `build --to pdf`.
pub fn load_typst_main(prepared: &Prepared, opts: &BuildOptions, world: &mut WriterWorld) -> Result<(), BuildFailure>;
pub struct TargetResult { pub target: Target, pub written: std::result::Result<Vec<PathBuf>, Error> }  // main output first
pub struct BuildReport { pub prepared: Result<Prepared, BuildFailure>, pub targets: Vec<TargetResult>, pub diagnostics: Diagnostics }
/// What `writer build` does: one prepare, `worlds.get(Some(&prepared), opts)` and `reset()`, then each
/// target rendered and written; a `get` failure is reported as `prepared: Err` (no targets).
pub fn build_targets(opts: &BuildOptions, targets: &[Target], worlds: &mut WorldSlot) -> BuildReport;
/// The World of one `build`, `build --watch` or `serve` session; creation and recreation rules: §5.3.
pub struct WorldSlot { /* Option<WriterWorld> + the values it was built from */ }
impl WorldSlot {
    pub fn new() -> Self;
    /// Right after each `prepare` (`None`: it failed or was skipped): the World, (re)created first,
    /// with its inputs and include provider set (§5.3).
    pub fn get(&mut self, prepared: Option<&Prepared>, opts: &BuildOptions) -> Result<&mut WriterWorld, BuildFailure>;
    pub fn current(&mut self) -> Option<&mut WriterWorld>;   // `accessed()` after a build (§7)
    pub fn discard(&mut self);                                // after a panic; the next `get` creates one
}
/// What a batch of changed paths rebuilds (§6.2); here so that T20 and T21 share it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")]
pub enum ChangeKind { Css, Pages, Content }
```

### 5.3 The World: one per session (T16's `WorldSlot`; T23 owns the session's, T24 and the web build use it)
- typst-dialect.md §6.1 defines `WriterWorld` and every operation named here. `WorldSlot::get`
  (§5.2), called right after each `prepare`, creates the World when the slot is empty:
  `WriterWorld::from_config(&prepared.config, <input's directory>, main, today)`, `main` =
  `MainFile::Path(input)` for `.typ` inputs, else `MainFile::Virtual`, `today` = `pdf_timestamp(
  &config.pdf, &document.meta, SOURCE_DATE_EPOCH, now)` (typst-dialect.md §8.1, §8.4); with
  `prepared: None`, from the `--config` or discovered `writer.toml` plus the CLI overrides (default
  config when none loads) and now. It recreates the World when `prepared` is `Some` and the World
  was built without a `prepare` or with other `[pdf] font_paths`, `system_fonts`, `allow_packages`
  or `[typst] package_paths`; otherwise it returns the kept one, after `world.set_inputs(..)` with
  the `writer-version`/`writer-config` inputs of `prepared.config` (typst-dialect.md §6.2, the ones
  `from_config` starts with; a no-op that keeps the Libraries unless they changed) when `prepared` is
  `Some`; with `None` it keeps the inputs of the last successful `prepare`. So a `writer.toml` edit
  of `[document]`, `[numbering]`, `[pdf] paper` or `[references] locale` reaches a `.typ` input's
  pages (serve) and PDF (`build --watch`, T52) through the package's `auto` arguments (typst-dialect.md
  §4.2) without a new World; Markdown/notebook inputs get the same inputs again from
  `load_typst_main`. Every `get` with `Some(prepared)` (create, recreate or keep) also calls
  `world.set_include_provider(Box::new(crate::typst::includes::MarkdownIncludes { config:
  prepared.config.clone() }))`; a World created with `None` gets one with the config it was created
  from, and with `None` a kept World keeps its provider (typst-dialect.md §6.4). T16 adds the
  `MarkdownIncludes` stub to T53's `src/typst/includes.rs` (additive; `synthesize` returns `Err`,
  `including .md/.ipynb from typst arrives in T53`), and T53 replaces only that body. A failing
  `pdf_timestamp` (invalid
  `[pdf] timestamp`: `Error::Config`) or `from_config` leaves the slot empty and is `get`'s `Err`
  (no diagnostics): every view that needs the World fails with its `for_failure` payload, and the
  next build tries again (no fallback config). The World is passed as `&mut` to `web_page` (for
  `crate::typst::html::equation_mathml` and `crate::typst::render::equation`) and
  `PagesBuilder::build`. The pages view always uses the embedded compiler (`[pdf] engine` only
  affects `build --to pdf`).
- Per build: (a) before the first compile or render, `world.reset()`: each disk file is re-read on
  its next access, unchanged text makes no `Source` edit (comemo still hits), changed text is
  applied with `Source::replace`; in debug builds `reset()` also drops the cached `@writer/doc`
  files, which rust-embed then reads from `assets/typst/writer-doc/` (Deviations); (b)
  Markdown/notebook inputs: `load_typst_main` (§5.2) inside `PagesBuilder::build`; `.typ` inputs:
  the World's main is the input on disk (cli.md), its `sys.inputs` those `WorldSlot::get` set
  (above); (c) `crate::typst::{compile,render,html}` never
  call `comemo::evict`; callers evict once per build (serve: §5.4 step 7; `build --watch`: §7):
  each call ages every cached result by one (comemo 0.5.1 `evict` docs), so per-render calls would
  evict the page layout between builds; (d) after the last compile, `world.accessed()` minus files
  below a package root (`[typst] package_paths`, `FsPackages::system_data()`/`system_cache()`),
  mounted files included, feeds the watch set.

### 5.4 One rebuild (`Session::rebuild`, T23; runs on `spawn_blocking`)
1. Merge the queued batches (paths sorted, deduplicated) and classify them (§6.2) → `kind`.
   `id = next_build_id(&state)`; `opts.mode = Mode::Serve { build_id: id }`; start the clock.
2. `content` only: compute the source position (§2.3).
3. `prepare`, unless `kind == Pages` and the input is `.typ`; then `world = worlds.get(prepared,
   &opts)` and `world.reset()` (§5.3; a `get` failure fails this build's views with its payload).
   For a `.md`/`.ipynb` input a `prepare` failure becomes one `ErrorPayload::for_failure` used for
   every view that needs it. For a `.typ` input only the web view needs `prepare`: its failure
   fails the web view, and step 5 still compiles `TypstFile { path }` with the session World (from
   the discovered config while no `prepare` has succeeded, §5.3).
4. Web (`css`, `content`): `web_page` → `Built(WebBuild { html, theme_css, assets: page.assets,
   theme })` or `Failed` (`for_failure`; also a subset error, §5.2, with the pages still built in
   step 5); `pages` kind → `Skipped`.
5. Pages (`pages`, `content`): `PagesBuilder::build(world, input, id)`, `input` = `TypstFile { path }`
   (`.typ`) or `Generated { prepared, opts }`; `Err` → `Failed` (`for_failure`); `css` → `Skipped`.
6. Steps 3–5 each run inside `catch_unwind`; a panic becomes `Error::Other("the build panicked
   (a bug in writer): <message>")` for that view (lifted) and `worlds.discard()`.
7. `comemo::evict(10)` (the build's only eviction, §5.3); the `css` → `content` upgrade (§2.5);
   the per-stage warnings (§2.6); `changed` (§2.3); `event = apply_outcome(…)`.
8. Watch set (§6.1). The session keeps `accessed_web` and `accessed_pages`: after step 4 runs,
   `accessed_web = world.accessed()`; after step 5 runs, `accessed_pages = world.accessed()`; a
   step that did not run keeps its set (a `css` build still watches what only the pages compile
   read). `accessed` = their union (§5.3 (d)). `prepare` succeeded: `watch_set(&watch_inputs(
   prepared), &accessed, None)`; failed: `watch_set_before_build` ∪ the last good set (lifted; the
   set of the last build whose `prepare` succeeded) ∪ `accessed`; skipped: the current set ∪
   `accessed`. `watcher.update(&set)` (an error → `tracing::warn!`).
9. stderr (not with `-q`): `rebuilt in <n> ms (<kind>)` when no view failed, else `rebuilt in <n>
   ms (<kind>; web failed)`, `(<kind>; pages failed)` or `(<kind>; web and pages failed)`, then
   each distinct error through `report.rs` (always printed). Diagnostics as `writer build` prints
   them. Then `publish(&events, event)`.

### 5.5 Lifecycle (lifted from presenter's `cli/serve.rs`)
Ctrl-C handler first; the watcher starts on `watch_set_before_build`; the first build (id 1,
kind `content`, both views) runs **before** the port is bound, so `GET /` is never empty, and a
failed first build does not stop the command. Then bind `host:port` (`--host`/`--port`, else
`[serve]`, else `127.0.0.1:3000`; port 0 picks a free one; a non-zero port is probed first);
print `Serving http://<address>:<port>/` as the first stdout line (also with `-q`). A port in use
exits 1 with the hint `use --port 0 to pick a free port`; other bind failures exit 1 with a hint
at `--host`. `--open` or `[serve] open = true` opens the URL (loopback when listening on an
unspecified address). The rebuild loop receives batches over a `tokio::sync::mpsc` channel,
merges those queued during a rebuild and runs one rebuild at a time on
`tokio::task::spawn_blocking` (the session moves into the job and back); runtime threads have 8
MiB stacks. `[serve]` host, port, open and view are read once; a later build whose `[serve]`
address differs prints warning `serve-config` once per change. Ctrl-C shuts down gracefully
(event streams end, `SHUTDOWN_GRACE`), exit 0.

## 6. Watch set and watcher — T20 (`src/serve/watch.rs`)

### 6.1 The set
Files: the input (`Source`), every `Document.includes` entry (`Include`; for a `.md` input that
does not parse, `include::scan_includes(input)`), the World's accessed disk files (§5.3 (d)),
every file asset and every missing asset (`Asset`, `MissingAsset`: creating it rebuilds), the bibliography
files and a `.csl` style path (`Bibliography`), the `writer.toml` in use (`--config` or
discovered, also when invalid; `Config`), and `[pdf] template` (`PdfTemplate`). Watched through
their parent directories, non-recursively (lifted `dirs_flat`). Directories watched recursively:
each disk theme directory of the HTML theme chain (`Theme`) and, in debug builds
(`cfg(debug_assertions)`), `<CARGO_MANIFEST_DIR>/assets/themes/default/` and
`<CARGO_MANIFEST_DIR>/assets/typst/writer-doc/`. **Not** watched in serve: `[docx]
reference_doc` and `[latex] template` (`ReferenceDoc`, `LatexTemplate`); package files outside
the debug directory (`@preview` cache, `[typst] package_paths`).

### 6.2 Classification
```rust
pub use crate::pipeline::ChangeKind;   // §5.2 (T16), shared with `serve::events`
pub fn classify(paths: &[PathBuf], set: &WatchSet) -> ChangeKind;
```
`Css` when every path is a stylesheet (`.css`, `.scss`, `.sass`, any case) or lies under the
`styles/` or `assets/` directory of one of `set.theme_dirs` (lifted). `Pages` when every path
is in `set.pages_paths` or below one of them (the `[pdf] template` file; the debug
`assets/typst/writer-doc/`). `Content` otherwise, also for a mixture of css and pages paths and
for no paths. `theme.toml`, templates, `writer.toml`, sources, includes, images, bibliographies
are content.

### 6.3 Mechanics (lifted verbatim)
A batch is sent once no relevant event has arrived for `DEBOUNCE` (150 ms), at the latest
`MAX_BATCH_WAIT` (2 s) after its first event; one coalescing thread `writer-watch` on `notify` 8.2
(no debouncer crate). Every event kind counts (rename-replace editors); watches are re-added for
recreated paths. Below recursive directories only `.git/` and editor files (`*~`, `.#*`, `#*#`,
`*.swp`, `*.swx`, `.*.sw[a-p]`, `4913`, `.DS_Store`) are ignored; `dist/`, `target/` count.
Flat-watched directories count only listed files. macOS only (FSEvents replays recent changes): an
event for a path whose inode change time precedes the last sent batch (at first, the watcher's
start), a directory's creation included, is dropped; whole-second change times never qualify.

### 6.4 API
```rust
pub const DEBOUNCE: Duration = Duration::from_millis(150);
pub const MAX_BATCH_WAIT: Duration = Duration::from_secs(2);
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Debounce { pub quiet: Duration, pub max_wait: Duration }  // Default: the two above
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchSet { pub files: Vec<PathBuf>, pub dirs_recursive: Vec<PathBuf>, pub dirs_flat: Vec<PathBuf>,
                      pub theme_dirs: Vec<PathBuf>, pub pages_paths: Vec<PathBuf> }   // canonical, sorted, deduplicated
impl WatchSet { pub fn merge(&mut self, other: &WatchSet); }
/// `targets: None` = serve; `Some(t)` = build --watch (ReferenceDoc iff Docx ∈ t, LatexTemplate iff Latex ∈ t).
pub fn watch_set(inputs: &[(PathBuf, InputRole)], accessed: &[PathBuf], targets: Option<&[Target]>) -> WatchSet;
pub fn watch_set_before_build(opts: &BuildOptions) -> WatchSet;   // input, config, scan_includes (.md), debug dirs
#[derive(Debug, Clone, PartialEq, Eq)] pub struct ChangeBatch { pub paths: Vec<PathBuf>, pub kind: ChangeKind }
pub fn is_ignored(rel: &Path) -> bool;   pub fn canonical(path: &Path) -> PathBuf;
impl Watcher { pub fn start(set: &WatchSet, tx: std::sync::mpsc::Sender<ChangeBatch>) -> Result<Watcher>;
    pub fn start_with(set: &WatchSet, tx: std::sync::mpsc::Sender<ChangeBatch>, debounce: Debounce) -> Result<Watcher>;
    pub fn update(&mut self, set: &WatchSet) -> Result<()>;   pub fn watched_dirs(&self) -> Vec<(PathBuf, bool)>; }
```

## 7. `build --watch` — T20 (`run_build_watch_with`, lifted), wired into `cli/build.rs` additively
```rust
pub enum WatchStatus<'a> { Built(&'a BuildReport), Watching(&'a WatchSet), Changed(&'a ChangeBatch) }
pub fn run_build_watch_with(opts: &BuildOptions, targets: &[Target],
                            report: impl FnMut(WatchStatus<'_>) -> ControlFlow<()>) -> Result<()>;
```
- T16's `build --watch` branch is a stub (`build --watch: not implemented yet (T20)`, exit 2);
  T20 replaces it with a call to `run_build_watch_with` (an additive edit, called out in its PR).
- Lifted order: the watcher starts on `watch_set_before_build`; the first build runs every
  requested target (`build_targets`); `Watching`; then `Changed` + `Built` per batch. Returns `Ok`
  when `report` breaks; errors only when the watcher cannot start.
- Targets per batch: `css` → `html`, plus `pdf` when `[pdf] engine = "chrome"` (printed from the
  HTML); `pages` → `typst`, plus `pdf` when the engine is `typst`; `content` → all; intersected
  with the requested targets; an empty intersection ignores the batch. One `WorldSlot` for the
  loop, passed to `build_targets` (which gets and resets the World after its prepare, §5.3);
  `comemo::evict(10)` after each build; mode `Build`.
- Watch set `watch_set(inputs, accessed, Some(targets))`, `accessed` = `worlds.current()`'s
  `accessed()` after the build ∪, when the batch did not run `pdf`, the accessed set of the last
  build that did; after a failed `prepare`, `watch_set_before_build` ∪ the last good set. Paths
  written by the last build (and anything below a written directory such as `<stem>_assets/`) are
  ignored.
- Output: each build prints what `writer build` prints; stderr gets `Watching for changes (Ctrl-C
  to stop)` after the first build and `Rebuilding (<kind>): <up to 3 paths>[, and <n> more]`
  before each rebuild (neither with `-q`).

## 8. Diagnostics and QA — T23, T73, T22

| code / text | level | where |
|---|---|---|
| `serve-config` | warning | a build's `[serve]` host/port differs from the running server's (§5.5), lifted |
| `the build panicked (a bug in writer): …` | error (`Error::Other`, kind `other`) | §5.4 step 6 |
| `use --port 0 to pick a free port` | exit 1 hint | §5.5 |
| `embedded <key> is missing` | HTTP 500 | §1.3 |

Every other problem reaches the browser as an `ErrorPayload` (§2.4) or a `warnings` entry
(§2.6) with the frontends' and emitters' own codes. T22 writes `docs/dev/live-reload-qa.md`
(lifted format: mock-server and `writer serve` setups, checklist, results; T22 records the
mock-server results, T23 (items 1–10, 12–16) and T24 (11, 17–19; item 11 needs real pages)
the `writer serve` ones) with at least these items (expected result after the colon; T24 adds
17–19):
1. Theme CSS edit: no reload (`window.__qaMarker` survives), `#writer-theme` gets `?v=<id>`, no unstyled flash.
2. Edit a visible paragraph: no reload; the first visible block keeps its screen offset (±1 px).
3. Edit a paragraph above or below the viewport: it is scrolled to the centre.
4. Set `[document] toc = true` in `writer.toml` while reading below the TOC (a content build
   without a position that grows the page above the viewport), or delete the anchor paragraph:
   the reading place stays (±1 px).
5. Edit an included file / notebook cell 3: `changed.source`/`cell` set; scrolled to when off screen.
6. Unchanged equations keep their DOM nodes (mark one with `el.__qa = 1`); an edited one re-renders.
7. Replace an image file: the new image shows without reload; no other image is re-requested.
8. Change the title: `document.title` follows. Edit `document.html` outside the root: one reload.
9. A raw HTML block with `<script>`: every content build reloads in full.
10. Parse error: `[web, pages] build error (parse)` with location and excerpt; click dismisses.
11. Pages-only error (`{=typst}` calling an unknown function): `[pages]` overlay, web still updates,
    toggle shows ` ●`; fixing clears both; a CSS edit meanwhile swaps CSS and keeps the error.
12. `@nope`: the warnings panel shows `1 warning(s)` with `unresolved-reference`; fixing clears it.
13. Stop the server: retries after 0.5, 1, 2, 4, then every 5 s, no reload. Restart with builds
    missed: in-place update; a new tab with a stale stored id reloads once.
14. `p` toggles views (not inside inputs); `?view=pages` opens the pages; `/` returns to the last view.
15. Before any good web build: fallback overlay; the first good build reloads into the document;
    a dismissed overlay stays dismissed across a reconnect with the same failing build.
16. Console: no uncaught exceptions; every message starts with `[writer]`.
17. Pages: sized before loading; only pages near the viewport are requested; zoom keys and fit.
18. Edit text on page 5 while viewing page 7: only changed pages re-requested; the view stays on page 7.
19. Pages error: the last good pages stay with the status text. `.typ` input: pages by default.

## 9. Tests

**Unit.** T16: `WorldSlot::get` (create, keep, recreate on a `font_paths` change and after a World
built without a prepare, `discard`, invalid `[pdf] timestamp` → `Err` and an empty slot, a kept
World after a `[numbering]` change carries the new `writer-config` (§5.3), a World returned by
`get` (created and kept) answers `source()` of a `.md` file id with the stub's `arrives in T53`
error, not `no include provider`); `subset_error` and
`web_page` with an `E-TYP-SYNTAX` error in `prepared.diagnostics` → `Err` with `Error::Parse` at
its line, a `W-TYP-*` warning → `Ok`. T20:
`classify` (css, pages, content, mixtures, empty), `watch_set` per role and `targets`, lifted
`is_ignored`/debounce/rename/ctime tests. T21: status, `Content-Type`, `Cache-Control` per route
(`theme.css` before a good web build → `404`); `?view=` with both defaults; SVG lookup (current,
previous, mismatch, `0.svg`, `x.svg`, out of range); the §1.3 bytes; `hello`/`BuildEvent` JSON
snapshots (`DiagnosticJson` keys `level, code, message, file, line, cell`); `apply_outcome` for every web ×
pages outcome, warnings kept per stage; fallback overlay text; Host check (lifted). T23: §2.3
(first/middle/last line, appended lines, equal text, CRLF-only edit → no position, cell edit, cell
insert, identical cells with other notebook `metadata` → no position, invalid JSON, include order,
symlinked include, a path without a stored text), batch merging, stderr lines. T24: an
unchanged page reuses its `PageSvg` (same `Bytes` pointer), `page_hash`, `size_pt` rounding,
`shell_html`, last good set kept on error, a notebook compile error's payload has `cell`.

**Integration** (`tests/cli_serve.rs`, T23; steps marked (P) added by T24). Copy
`tests/fixtures/serve/` to a temp dir, run `writer serve document.md --port 0` (killed on drop),
read the URL from the first stdout line, talk through a ureq 3 agent and an SSE reader thread
(both lifted); every wait ≤ 10 s:
1. `GET /`: `200`, `no-store`, contains `data-writer-document`, `data-sourcepos="`, `data-source="0"`,
   `id="writer-theme"`, `/__writer/live-reload.js`, a `/__writer/doc-asset/<12 hex>?v=` URL.
2. `/__writer/state`: `build_id` 1, `status` ok, web ok; (P) `pages.count ≥ 2`, `hashes.len() == count`.
   `/__writer/live-reload.js` starts with the §1.3 comment. SSE: the first event `hello` equals the state.
3. Edit the last paragraph of `document.md` (line `L`) → `build`: `kind` content, `changed ==
   {file: "document.md", source: 0, line: L}`, web ok; (P) pages ok, the first page's hash
   unchanged. Edit line 3 of `chapter.md` → `changed == {file: "chapter.md", source: 1, line: 3}`.
4. Edit `theme/styles/extra.css` → `kind` css, `changed` null, pages skipped with unchanged hashes;
   `/__writer/theme.css` has the new rule. (P) Edit `template.typ` → `kind` pages, web skipped.
5. (P) Append a `{=typst}` block calling an unknown function → `status` error, web ok, pages error
   (`kind` compile), hashes unchanged, an old hash URL still `200` immutable.
6. Add `:::include missing.md` → equal web and pages payloads (`kind` parse); `GET /` serves the
   previous document. Fix → `status` ok.
7. Replace `figures/plot.png` → the doc-asset URL's `v` changes and its body is the new bytes.
8. `Host: evil.example` → `403`; `/?view=pages` → the shell; with `--view pages`, `/` → the shell.
9. (P) `/__writer/pages/1.svg?v=<hash>` → `200`, `image/svg+xml`, immutable, body hash == `<hash>`;
   `?v=bogus` → `200` `no-store`; `0.svg`, `999.svg`, `x.svg` → `404`.
10. `writer serve notebook.ipynb`, change cell 2's source → `changed == {file: "notebook.ipynb",
    source: 0, cell: 2}`. A second server on a used port → exit 1 with `use --port 0`; SIGINT → 0.
11. `writer serve math-image.md`: every `/__writer/doc-asset/` URL in `GET /` → `200` (equation SVGs).
T20 adds `tests/watch.rs` (lifted) plus `build --watch --to html`: a css edit and a content edit
each rewrite the HTML; a `[pdf] template` edit (kind `pages`) is ignored (empty target
intersection, mtime unchanged). T24 (after T23, hence T20, and after T32) adds, additively to
`tests/watch.rs`, `build --watch --to html,pdf`: a css edit leaves the PDF's mtime unchanged, a
content edit rewrites both; T32 adds no watch test. T52 adds `writer serve` of a
`.typ` input: `/` is the pages shell; a subset error (`E-TYP-*`) → web error, pages ok.

**Browser** (feature `browser-tests`, T25's DevTools client; `tests/browser_serve.rs`): T23 —
no console errors; items 2, 3, 4, 6 and 7 of §8 asserted through `evaluate`; T24 — pages render
with their sizes before load, an edit re-requests only changed page URLs (network log), the
anchor page is kept, `p` switches views, `apply(p, 18)` then `apply(q, 1)` renders `q` (restart).

## 10. Verified API hints
- axum 0.8.9 / matchit 0.8.4 ("Dynamic suffixes are not currently supported"): the page route is
  `/__writer/pages/{file}`, the handler parses `<n>.svg`; `Query` needs axum's `query` feature (on
  by default), `RawQuery` is always available; the `?v=` and `?view=` handlers may use either.
  SSE, `spawn_blocking`, `broadcast::channel(64)` and shutdown as presenter's `src/serve/server.rs`
  (`event_stream`/`build_stream`/`serve_until_allowing`; R8 §11's `oneshot::Receiver` form of
  `serve_until` is stale, §5.1's generic `F: Future<Output = ()> + Send + 'static` governs):
  `Sse::new(BroadcastStream::new(rx)…).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))`.
- typst 0.15.1 (crate sources): `#[derive(Debug, Clone, Hash)] pub struct Page`;
  `typst::utils::hash128<T: Hash + ?Sized>(&T) -> u128`; `typst_svg::svg(&Page, &SvgOptions) ->
  String`; `Frame::width()/height() -> Abs`, `Abs::to_pt`; World and `comemo`: typst-dialect.md §11.
  R1 §4: 9.2–14.2 ms recompile after an edit of a 25-page document (retained `Source`); SVG export
  0.65–0.83 ms for R1's 1-page test document (88 KB); per-page SVG cost of real documents not measured.
- `blake3::hash(b).to_hex()` (take `[..16]`); morphdom 2.7.8 (R6 §A.2):
  `onBeforeElUpdated(from, to)` returning `false` skips the subtree; ureq 3 agent and SSE reader
  as presenter's tests.

## Fixtures
- `tests/fixtures/serve/` (T23): `document.md` (title, `:::include chapter.md`, ≥ 60 paragraphs
  over ≥ 2 pages, figure `figures/plot.png`, inline and display LaTeX math, a footnote),
  `chapter.md` (≥ 5 lines), `figures/plot.png`, `figures/plot-v2.png`, `theme/theme.toml` (`name =
  "fixture"`, `extends = "default"`, `[styles] entries = ["styles/extra.css"]`),
  `theme/styles/extra.css`, `writer.toml` (`[html] theme = "theme"`), `notebook.ipynb` (cell 0
  markdown, cells 1–3 code with outputs), `math-image.md` (front matter `math: {html: image}`, one
  inline and one display equation); T24 adds `template.typ` and `[pdf] template` (honoured by
  T32's `load_typst_main`, which mounts and imports it per typst-dialect.md §5.3/§8.4; T24 does
  not wait for T34). Tests copy it to a temp dir (PLAN §7).
- T20's `build --watch` test (wave 5, before T23's fixture exists) builds its own temp dir:
  `document.md`, `theme/theme.toml` + `theme/styles/x.css`, `template.typ`, `writer.toml` with
  `[html] theme = "theme"` and `[pdf] template = "template.typ"`.
- `tests/fixtures/live-reload/{mock-server.py,qa.mjs}` (T22, lifted and adapted to these routes and
  events; morphdom from `assets/vendor/`, T00); T24 adds the page routes. `.typ` serve fixtures: T52.

## Open points (local implementation choices)
- T22: inline CSS of the toggle and warnings panel beyond §3.6. T24: `PAGES_CSS` details; scroll
  handler or `IntersectionObserver` for the page counter. T23: the session struct's layout, wording
  beyond the quoted strings. T20: helpers inside `watch_set` (lifted `ParsedInputs` may stay).

## Deviations from PLAN.md and the brief
- `changed` also carries `source` (the `data-source` index): the client cannot map a file name to
  the index-based `data-source` attribute otherwise; `{file, line}`/`{file, cell}` keep their meaning.
- PLAN §3.2 gives `docs/dev/*.md` to T74; `docs/dev/live-reload-qa.md` is T22's (T24 additive), as
  in presenter, because the checklist ships with the client.
- `src/serve/pages.rs` and `assets/writer/pages.js` are T24's, but T21 writes the types,
  `shell_html`, the `PagesBuilder::build` stub (an empty successful set) and a placeholder
  `pages.js` (§1) first: T21's routes need them, and T23 runs before T24. The session World is
  T23's, not T24's (PLAN T24): T23's web build needs it.
- §5.2 fixes the serve-facing pipeline API that PLAN leaves to T16, including `WorldSlot` and
  `ChangeKind` (T20's `serve::watch` and T21's `serve::events` share it and run in the same wave);
  T16's task file must copy it. `load_typst_main` is a T16 stub whose body T32 writes and shares
  with `build --to pdf`. `WorldSlot::get` also installs typst-dialect.md §6.4's `MarkdownIncludes`
  (§5.3), whose stub T16 adds to T53's `src/typst/includes.rs`; the PLAN §3.2/§5 notes for that
  are typst-dialect.md's CROSS item (dependencies unchanged: T16 already depends on T03).
- Dependencies (waves unchanged unless said): T23 + T25 (DevTools client); T16 + T03 (§5.2 takes
  `WriterWorld`); T73 + T21 (`src/serve/events.rs`: T21 → T73 for the `ErrorPayload` bodies); T22 +
  T00 (vendored morphdom, `scripts/vendor.sh`), so T22 moves to wave 1; T24 + T23 (T24 extends
  `tests/cli_serve.rs`, `tests/browser_serve.rs`, `tests/watch.rs` and `tests/fixtures/serve/`,
  and runs `writer serve`): T24 → wave 7, T52 → wave 8, T53/T71/T78 → wave 9, T74 → wave 10.
  T20's watch test covers `html` only (`pdf` is T16's stub until T32, which shares T20's wave and
  does not depend on it); T24, which depends on both through T23 and T32, adds the `pdf` case to
  T20's `tests/watch.rs` (no PLAN dependency changes).
- New fixture directories beyond PLAN §7: `tests/fixtures/serve/`, `tests/fixtures/live-reload/`.
- The brief's "`comemo::evict(10)` after each compile" becomes once per build (§5.3 (c)). For the
  coordinator: typst-dialect.md §6.1 and math.md §11's render snippet drop their `evict` calls;
  typst-dialect.md §6.1/§6.5 let `reset()` drop the cached `@writer/doc` files in debug builds;
  typst-dialect.md §8.1 drops `compile_pages` and its `PageSvg` (replaced by §4.1's export with
  reuse and the BLAKE3 `page_hash`).
- The debug `assets/typst/writer-doc/` classifies as `pages`, like `[pdf] template` (the brief
  names only the template). `build --watch` rebuilds only the targets a batch affects and watches
  the reference document and LaTeX template when those targets are requested (serve never does).
- presenter's `BuildEvent` (an enum tagged by `status`) becomes a struct with per-view objects;
  `ErrorPayload` gains `cell` and `hints`.
- Length: about 840 lines, above the brief's 250–600. The spec serves six tasks (T20–T24, T73)
  and three languages (Rust, the client JS, the protocol), and its unit lists and QA checklist
  are acceptance criteria the task files copy.
