# T21 — Dev server routes, events, and per-view state

Frozen implementation brief, coordinator, 2026-10-28.

| | |
|---|---|
| Milestone / size | M2 / M |
| Depends on | T16, already present |
| Primary ownership | `src/serve/{mod,server,files,events,pages}.rs` |
| Additional owned asset | `assets/writer/pages.js` (one-line T24 placeholder) |
| New tests | `tests/server.rs`; module-local event/file/page/state tests and JSON snapshots as useful |
| Binding sources | PLAN §3.2/§5 T21/§6–7; `docs/spec/live-reload.md` §1–2, §4.1–4.2, §5.1, §9–10 and Deviations; HTML route helpers from html-output.md |
| Lift sources | `../presenter_rs/src/serve/{server,files,events}.rs`, `../presenter_rs/tests/server.rs` |

## Goal and boundaries

Implement the HTTP/SSE library server, retained last-good web/pages state,
per-view event protocol, asset/file responses, Host protection and bounded
shutdown. Expose the frozen interfaces T23 needs. T21 is independently
exercised through real HTTP on port 0; it does not wire `writer serve`, build a
session, add a module file, render compiler pages, or change pipeline/config/IR.
T23 owns the CLI and session; T24 replaces the explicit empty page-builder
stub and the placeholder client. T73 later improves compiler source mappings.
Do not copy presenter's Deck/Build or tagged BuildEvent interfaces.

T20 is adjacent work inherited on this branch, not a new hard dependency.
Create one T21 PR against main; list any open inherited dependency PRs and
merge order. Continue to the next feature while CI runs.

## Existing seams checked in this tree

- All five owned serve files are T00 module-comment stubs; their `mod`
  declarations already exist. `assets/writer/pages.js` does not yet exist.
- `pipeline::{ChangeKind,Prepared,BuildOptions,BuildFailure,WebPage,web_page}`,
  `config::ServeView`, `AssetRegistry`, `Theme`, and Error/Diagnostics exist.
- Reuse `html::{normalize_helper_path,resolve_helper_path,serve_stylesheet}`
  and `html::{SERVE_HELPER_PREFIX,SERVE_THEME_CSS}` (the live-reload URL is local to the server).
  `assets::emit::SERVE_ASSET_PREFIX` names `/__writer/doc-asset/`.
- AssetRegistry distinguishes in-memory and file-backed data; `vendor::get`
  supplies embedded `vendor/...` and `writer/...` bytes. T22's existing client
  and vendored morphdom are already present. No new dependency is needed.
- Error has Parse, FrontMatter, Config, Theme, Template, Style, MissingAsset,
  Compile (with hints), Resource, Io, Json and Other variants. Its helpers
  `path`, `span`, `message`, `excerpt`, `quoted_line` and `source_line` are
  available. A span can name an included file and a notebook cell.
- Keep the stable repository rustfmt layout and AGENTS.md Rust refinements;
  do not reformat unrelated code. Root serializes Cargo/rustfmt/git commands.

## Frozen events API (`events.rs`)

Copy the exact public fields and derives from live-reload.md §2.2:

```rust
pub enum Status { Ok, Error }
pub enum ViewStatus { Ok, Error, Skipped }
pub struct WebView { pub status: ViewStatus, pub error: Option<ErrorPayload> }
pub struct PagesView {
    pub status: ViewStatus, pub error: Option<ErrorPayload>,
    pub count: usize, pub hashes: Vec<String>, pub size_pt: Vec<[f64; 2]>,
}
pub struct Changed {
    pub file: String, pub source: usize,
    pub line: Option<usize>, pub cell: Option<usize>,
}
pub struct BuildEvent {
    pub build_id: u64, pub status: Status, pub kind: ChangeKind,
    pub duration_ms: u64, pub changed: Option<Changed>,
    pub web: WebView, pub pages: PagesView, pub warnings: Vec<DiagnosticJson>,
}
pub struct Hello {
    pub build_id: u64, pub status: Status,
    pub web: WebView, pub pages: PagesView, pub warnings: Vec<DiagnosticJson>,
}
pub struct ErrorPayload {
    pub message: String, pub file: Option<String>, pub line: Option<usize>,
    pub cell: Option<usize>, pub kind: String,
    pub excerpt: Option<String>, pub hints: Vec<String>,
}
impl ErrorPayload {
    pub fn new(err: &Error, base: Option<&Path>, input: Option<&Path>) -> Self;
    pub fn for_input(err: &Error, input: &Path) -> Self;
    pub fn for_failure(failure: &BuildFailure, input: &Path) -> Self;
}
pub struct DiagnosticJson {
    pub level: &'static str, pub code: String, pub message: String,
    pub file: Option<String>, pub line: Option<usize>, pub cell: Option<usize>,
}
impl DiagnosticJson {
    pub fn new(diagnostic: &Diagnostic, base: Option<&Path>) -> Self;
}
```

- Status/ViewStatus lowercase serde. The structs derive Debug/Clone/PartialEq
  and Serialize; Eq for Changed/ErrorPayload/DiagnosticJson, not f64-bearing
  structs. Status/ViewStatus also Copy/Eq.
- Omit absent per-view `error`, Changed.line/cell, absent excerpt and empty
  hints. Keep ErrorPayload.file/line/cell and DiagnosticJson location keys
  explicit null; `changed` remains null when absent. Exactly one Changed.line
  or Changed.cell is populated by T23; T21 serializes without inventing it.
- Lift error conversion/excerpt/path/config handling from presenter, adapted
  to all Writer variants. `for_input` passes cwd as base and input context.
  Use relative paths only below base, including filesystem-canonical fallback
  for symlink spelling; show `/` separators on every OS.
- MissingAsset location prefers span.file, else supplied input; its cell and
  line are retained (line is within cell). Front matter uses input. Parse
  handles notebook message-derived span/cell. Template/Style keep their named
  location, Compile and Resource must have real kebab-case kind mappings.
- Located message is `<file>:<line>: <message>`, or `<file> cell <c>:<line>:`
  for cell locations, with reasonable omission when no line is known.
  Missing asset message still names the missing resource relative to base.
- `for_failure`: for Compile only, use path/span of the first Level::Error
  diagnostic for file/line/cell, with span.file taking precedence over path;
  preserve compile hints and quote the located source if available. Preserve
  `for_input` fallback if there is no error diagnostic. This is implemented
  now; T73 improves upstream source mapping/package traces later.
- DiagnosticJson: span.file first, else diagnostic.path; line=span.start_line,
  cell=span.cell. level strings info/warning/error. T23 filters Info; T21's
  constructor must represent every level faithfully.
- Optional compatibility conveniences From<&Error>/relative_to/build_id are
  allowed if useful; do not change the frozen fields or add a tagged enum.

## Frozen pages foundations (`pages.rs`)

```rust
pub struct PageSvg {
    pub hash: String, pub svg: axum::body::Bytes,
    pub size_pt: [f64; 2], pub key: u128,
}
pub struct PageSet { pub build_id: u64, pub pages: Vec<PageSvg> }
impl PageSet {
    pub fn find(&self, hash: &str) -> Option<&PageSvg>;
    pub fn hashes(&self) -> Vec<String>;
    pub fn sizes(&self) -> Vec<[f64; 2]>;
}
pub fn page_hash(svg: &[u8]) -> String;
pub fn shell_html(title: &str) -> String;
pub enum PagesInput<'a> {
    Generated { prepared: &'a Prepared, opts: &'a BuildOptions },
    TypstFile { path: &'a Path },
}
pub struct PagesBuilder { /* T24 retains the previous PageSet */ }
impl PagesBuilder {
    pub fn new() -> Self;
    pub fn build(
        &mut self, world: &mut WriterWorld, input: PagesInput<'_>, build_id: u64,
    ) -> Result<(Arc<PageSet>, Diagnostics), BuildFailure>;
}
```

- `page_hash` is first 16 lowercase hex characters of BLAKE3(svg bytes).
- Implement PageSet lookup/metadata methods. Clone/Debug derives and Default
  for builder are sensible additive conveniences.
- `shell_html` reproduces live-reload.md §4.2 markup, HTML-escapes title,
  loads `/__writer/pages.js` before `/__writer/live-reload.js`. Fixed toolbar,
  zoom buttons, counter/status/note and empty data-writer-pages main are exact.
- PAGES_CSS is local choice within spec: grey/dark system page background,
  fixed top toolbar, centred column, 16px gaps/side padding, white pages/shadow.
- **Only deliberate T21 stub:** PagesBuilder::build returns
  `Ok((Arc::new(PageSet { build_id, pages: Vec::new() }), Diagnostics::default()))`.
  No compiler or SVG export; do not call compile_pages or add session logic.
- Add `assets/writer/pages.js` with exactly `/* pages view: T24 */` plus newline.

## Frozen state/runtime API (`server.rs`)

Implement public API verbatim from live-reload.md §5.1:

```rust
pub const EVENT_CAPACITY: usize = 64;
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
#[derive(Clone)]
pub struct WebBuild {
    pub html: String, pub theme_css: String,
    pub assets: AssetRegistry, pub theme: Theme,
}
#[derive(Default)]
pub struct WebSlot {
    pub good: Option<WebBuild>, pub error: Option<ErrorPayload>,
    pub warnings: Vec<DiagnosticJson>,
}
#[derive(Default)]
pub struct PagesSlot {
    pub current: Option<Arc<PageSet>>, pub previous: Option<Arc<PageSet>>,
    pub error: Option<ErrorPayload>, pub warnings: Vec<DiagnosticJson>,
}
pub struct ServeState {
    pub build_id: u64, pub default_view: ServeView, pub title: String,
    pub web: WebSlot, pub pages: PagesSlot,
    pub prepare_warnings: Vec<DiagnosticJson>,
}
impl ServeState {
    pub fn new(default_view: ServeView, title: String) -> Self;
    pub fn hello(&self) -> Hello;
    pub fn warnings(&self) -> Vec<DiagnosticJson>;
}
pub type Shared = Arc<std::sync::RwLock<ServeState>>;
pub enum ViewOutcome<T> { Built(T), Failed(ErrorPayload), Skipped }
pub struct Outcome {
    pub kind: ChangeKind, pub duration_ms: u64, pub changed: Option<Changed>,
    pub title: Option<String>,
    pub web: ViewOutcome<WebBuild>, pub pages: ViewOutcome<Arc<PageSet>>,
    pub prepare_warnings: Option<Vec<DiagnosticJson>>,
    pub web_warnings: Option<Vec<DiagnosticJson>>,
    pub pages_warnings: Option<Vec<DiagnosticJson>>,
}
pub fn event_channel() -> broadcast::Sender<BuildEvent>;
pub fn publish(events: &broadcast::Sender<BuildEvent>, event: BuildEvent) -> usize;
pub fn next_build_id(state: &Shared) -> u64;
pub fn apply_outcome(state: &Shared, outcome: Outcome) -> BuildEvent;
pub fn router(state: Shared, events: broadcast::Sender<BuildEvent>) -> Router;
pub async fn serve_until<F>(listener: TcpListener, router: Router, shutdown: F)
    -> Result<()> where F: Future<Output = ()> + Send + 'static;
pub async fn serve_until_allowing<F>(
    listener: TcpListener, router: Router, shutdown: F, hosts: AllowedHosts,
) -> Result<()> where F: Future<Output = ()> + Send + 'static;
impl AllowedHosts {
    pub fn new(addr: SocketAddr) -> Self;
    pub fn with_name(self, name: &str) -> Self;
    pub fn allows(&self, host: Option<&str>) -> bool;
}
```

- Start build_id 0; next_build_id peeks +1. apply_outcome increments once per
  attempt, even both Failed/Skipped; title Some replaces it.
- Web Built replaces good/clears error; Failed replaces error/retains good;
  Skipped preserves everything. Pages Built moves current to previous then
  installs new current/clears error; Failed/Skipped retain both page sets.
- Some stage warnings replaces that stage, including Some(empty); None keeps.
  warnings is prepare ++ web ++ pages with stable exact six-field dedup.
- Event statuses reflect this attempt's outcomes. Hello statuses derive slot
  errors and never Skipped. Global status Error iff either slot holds error,
  including old errors retained on a skipped view. Served page metadata is
  from current even when event pages status Failed/Skipped.
- event_channel capacity64; publish returns receiver count, harmless if none.
- std RwLock with poisoned-lock recovery, held briefly only for copying or
  swapping state; never across await, file IO, formatting, rendering/compile.
- Lift generic shutdown lifecycle: stop accepts, signal all SSE streams to
  end, await completion bounded by SHUTDOWN_GRACE=2s for stalled requests.
- AllowedHosts faithfully lifts listening address, localhost/127.0.0.1/[::1],
  added names ignoring ASCII case; exact bound port, implicit80 only; mapped
  IPv6 canonicalized; wildcard listening permits any IP but no arbitraryname.
  No Host passes, URI authority fallback applies for absolute-form requests.
  A present invalid-UTF8 Host is rejected (Some("")), never treated as absent.
  Middleware covers known/unknown routes, assets, events, state and favicon.

## Routes, file responses and cache rules

Read the exact route table live-reload.md §1 and lift handlers from presenter,
renaming `/__presenter/` to `/__writer/`, deck assets to document assets.

- `/`: recognized view=web/pages, else default_view. Web is latest good HTML
  or fallback error page; pages is the current-title shell. no-store/textHTML.
- theme.css: latest good web CSS, no-store/textCSS; 404 before good web build.
- helper assets: normalize and resolve vendor/writer/@theme/theme-chain paths;
  vendor cache max-age=3600, all other assets no-store. Nonbuiltin theme CSS
  uses existing serve_stylesheet url rewriting/version cache; query v ignored.
  Builtin vendor/writer assets resolve even before a good web build.
- doc-asset/{id}: latest-good registry only, unknown404; v ignored. File data
  streams64KiB chunks; in-memory bytes use range responder. Directories404 on
  Unix and Windows. No complete read of large file-backed media.
- files.rs: lift whole_response/bytes_response/file_response and tests.
  Accept-Ranges bytes; single `bytes=a-b`, `a-`, `-n`:206 with exact length/range;
  unsatisfiable416 and `Content-Range: bytes */LEN`; malformed/multiple/unknown
  unit/If-Range requests get full200. Whole document/CSS needs no range support.
- live-reload.js: exact comment `/* morphdom 2.7.8 (MIT), vendored; see
  THIRD-PARTY-LICENSES */`, newline `(function (define, module, exports) {`,
  embedded vendor/morphdom/morphdom-umd.min.js, newline `}).call(window);`,
  newline existing writer/live-reload.js. Missing embed500 `embedded <key> is
  missing`. JavaScript utf8/no-store. Never silently empty fallback.
- pages.js: existing placeholder bytes, same JavaScript/cache headers; if
  embed missing500 `embedded writer/pages.js is missing`.
- pages: shell; pages/{file}: parse strict `[1-9][0-9]*.svg` (not axum suffix
  syntax, unsupported by matchit). Invalid/out-of-range404. Requested v hash
  matching ANY page of current or previous set gives those bytes immutable
  public,max-age31536000; otherwise current nth page no-store; otherwise404.
  SVG Content-Type image/svg+xml, nosniff and exact CSP
  `default-src 'none'; img-src data:; style-src 'unsafe-inline'`.
- state: Json(state.hello()), no-store. favicon204.
- events: subscribe BEFORE reading hello, compact single data line namedhello
  immediately then each build; skip lagged oldest broadcasts, keep-alive15s,
  text/event-stream and axum default no-cache. Shutdown closes streams.
- Fallback: title `writer: build error`; lead exactly `The document could not
  be built. Fix the error and save: this page reloads by itself.`; clienttag,
  #writer-overlay pre-rendered in T22 client's per-view error format inclcell,
  excerpt/hints; HTML escaped. No building variant/no data-writer-document.

## Implementation split and coordinator decisions

Root owns server.rs state/router/lifecycle/tests, serve/mod.rs docs, task board,
brief and acceptance/build-time records. Event agent owns events.rs conversion,
serde and units. File/page agent owns files.rs faithful lift and tests, pages.rs
foundations/shell/stub and placeholder. Integration agent owns tests/server.rs
real HTTP/SSE harness and routes/lifecycle/Host coverage. Avoid concurrent file
edits; root alone runs Cargo/rustfmt/git. Public item docs are required.

Crate-private route/helper organization is a local implementation choice. It
may use crate-private handler support shared within existing owned files; no
new source module is needed. Do not alter Cargo.toml, config, IR, pipeline or
CLI modules. No coordinator contract amendment is needed for implementing the
already-frozen APIs; document an actual conflict before widening ownership.

## Acceptance and validation

- Event JSON: complete Hello/BuildEvent examples, null/omission rules, every
  kebab Error kind, hints/cells/excerpts, TOML provenance and symlink-relative
  paths, first compile error diagnostic, DiagnosticJson source precedence.
- State: all 3×3 web/pages outcome combinations, last-good retention and page
  rotation, incremented attempt IDs, skipped-view old errors/globalstatus,
  Some(empty)/None warning semantics and exact stable dedup/title changes.
- Pages: PageSet find/hashes/sizes, fixed hash and escaped shell, both builder
  input variants return requested-id empty set without compiling; stub explicit.
- Real HTTP: every route's status/contenttype/cache, all helper path forms and
  every generated URL/CSS url resolves, latest-good assets after failure,
  prebuild theme404/builtinworks, both view defaults/recognized/unknown queries,
  range variants against file/in-memory/helper assets, dirs404, versionsignore.
- SVG: current/previous hashes, hash match independent of n, mismatch fallback,
  missing sets, invalid0/leading0/nondigits/overflow/out-of-range grammar,
  immutable/no-store and security headers.
- SSE: hello equals state, compact build events and no receivers, independent
  subscribers, dropped subscriber, lagged receiver still advances, graceful
  SSE close, dropped shutdown sender and stalled request deadline. Port0 tests
  use bounded requests and shutdown Drop; no arbitrary sleeps >10s waits.
- Host: mappedIPv6/wildcards/case/port/noHost/explicitnames/invalidheaders and
  every route including unknown403. Lift presenter's cases faithfully.
- Integration harness: tokio runtime on dedicated thread, 127.0.0.1:0, ureq3
  proxiesdisabled, ≤10s request/wait deadlines, shutdown sender and bounded done
  receiver in Drop; join only after done. Synthetic PageSets for SVG tests;
  real Mode::Serve web_page fixtures where useful. No T24 compiler fixture.
- Root serial final checks: repository fmt, alltargets/allfeatures Clippy with
  denied warnings, debug and release suite, denied-warning rustdoc, basic/mixed
  build smoke. Native socket/watch access may need local sandbox escalation;
  preserve service behavior. Record measured times and honest deferred scope.

## Acceptance refinement (coordinator, 2026-10-28)

Full-suite validation reproduced concurrent native watch-child startup timeouts
in the inherited T20 harness. The same case passes alone; serializing child
lifetimes makes the suite deterministic without extending its ten-second
operation deadlines. Approve the narrow `tests/watch_loop.rs` harness change
on this branch; no watcher or build production behavior changes. State this
additional touched ownership in the PR and report the initial failed run.


## Acceptance result (2026-10-28)

Formatting, all-target/all-feature Clippy, denied-warning rustdoc, 1,235 debug
and 1,234 release tests pass. Nineteen real HTTP/SSE regressions and 34 module
tests exercise the frozen routes, protocol, state transitions, error payloads,
streaming/ranges and lifecycle contracts. Example smoke builds retain their
previous sizes. Explicit event JSON golden assertions are reviewed in source.
The inherited watch-child harness refinement and measured runs are recorded
in docs/dev/build-times.md. Pages compilation/client and CLI/session wiring
remain explicitly T24 and T23 respectively. Remote CI runs independently.
