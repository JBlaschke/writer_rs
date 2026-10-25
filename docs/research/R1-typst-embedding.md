# R1 — Embedding the typst compiler as a library (typst 0.15.1)

Research notes for the `writer` plan. Date: 2026-10-24. Machine: macOS (Darwin 25.6.0),
Apple M4 Max (14 cores, 36 GiB), Rust 1.99.1 (Homebrew cargo), network available.

**Verification legend.** Everything below marked *(verified: source)* was read in the crate
sources under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-0.15.1/`,
*(verified: run)* was observed as output of a scratch program built and run on this machine,
*(verified: docs)* was fetched from typst.app / GitHub on 2026-10-24, and *(unverified)* is
inference. Scratch crates live under
`/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/`
(`typst-embed/`, `typst-embed-nofonts/`, shared `target/`). Downloaded typst-cli v0.15.1
sources are in `.../scratchpad/downloads/typst-cli-v0.15.1/`.

---

## 0. Executive summary (what the plan should assume)

1. **Embedding works and is small to write.** A complete in-memory `World` is ~60 lines
   (§5). `typst::compile::<PagedDocument>(&world)` → `typst_pdf::pdf`, `typst_svg::svg`,
   `typst_render::render`; `typst::compile::<HtmlDocument>(&world)` → `typst_html::html`.
   All verified by building and running (§5.3).
2. **Measured cost.** Cold `cargo build --release` of a crate depending on typst + typst-pdf +
   typst-svg + typst-render + typst-html + typst-kit(embedded-fonts, scan-fonts) +
   typst-assets(fonts): **58.0 s wall (404 s CPU), 295 crates**. Release binary with
   `strip = true`, `lto = "thin"`: **47,355,536 B (45.2 MiB)** with embedded fonts,
   **37,581,792 B (35.8 MiB)** without → fonts cost **9,773,744 B (9.32 MiB)**. Incremental
   rebuild after touching only our own code: 14–62 s (thin-LTO relink dominates).
3. **Compile speed.** Tiny document (1 page, heading/para/equation/table/raw): 20–22 ms cold,
   **29 µs** unchanged recompile, **~0.45 ms** after a one-word edit. 25-page document
   (40 sections, 120 `lorem(90)` paragraphs, 40 numbered equations, outline): **69–76 ms
   cold, 0.25 ms unchanged, 9–10 ms after a paragraph edit, 12–14 ms after appending a
   section**, 54–62 ms after `comemo::evict(0)`. PDF export 7–9 ms for 25 pages (256 KB).
   All *(verified: run)*. The cache is a process-global comemo cache; the only thing the
   World must do is return the *same* `Source` (or an in-place `Source::replace`d one) for
   unchanged files and call `comemo::evict(10)` after each compile like `typst watch` does.
4. **typst-kit 0.15.1 was "completely reworked"** (changelog). The task's assumed names
   (`FontSearcher`, `PackageStorage`, features `fonts`/`packages`/`downloads`) **do not
   exist**. Real API: `typst_kit::fonts::{FontStore, FontPath, FontSource, embedded(), scan(),
   system()}`, `typst_kit::files::{FileStore, FileLoader, FsRoot, SystemFiles}`,
   `typst_kit::packages::{SystemPackages, FsPackages, UniversePackages}`,
   `typst_kit::downloader::{Downloader, SystemDownloader, ProgressDownloader}`,
   `typst_kit::watcher::Watcher`, `typst_kit::server::HttpServer`, `typst_kit::datetime::Time`,
   `typst_kit::diagnostics::{emit, DiagnosticWorld, DiagnosticFormat}`. Features:
   `embedded-fonts`, `scan-fonts`, `system-files`, `system-packages`, `universe-packages`,
   `system-downloader`, `emit-diagnostics`, `datetime`, `watcher`, `timer`, `http-server`,
   `bundle`, `vendor-openssl`. Default features: none. *(verified: source)*
5. **HTML export** is library-usable but gated by a *runtime* library feature
   (`Library::builder().with_features([Feature::Html].into_iter().collect())`), not a Cargo
   feature. It always emits a warning "html export is under active development and
   incomplete". Equations come out as **MathML** (`<math>`), figures as
   `<figure>/<figcaption>`, tables as `<table>`, raw as `<pre><code data-lang="rust">` with
   inline `style="color: …"` spans, footnotes as `<section role="doc-endnotes">`, images as
   base64 `data:` URLs. No CSS is generated except a `<style>` block for MathML alignment.
   `page`, shapes, most layout are ignored/unsupported. *(verified: run + source + docs)*
6. **Custom package namespace `@writer/...` works with no registry**: the compiler just
   asks `World::file()` for `typst.toml` and the entrypoint under
   `VirtualRoot::Package(spec)`. Namespace must be a typst identifier (`writer` ok,
   `writer.rs` rejected). An unknown `@preview/...` with no storage yields
   "package not found (searched for @preview/cetz:0.3.4)". *(verified: run)*

---

## 1. Crates, versions, licenses, features

All checked with `cargo info <crate>` on 2026-10-24 and by reading the registry `Cargo.toml`.

| crate | latest | license | rust-version | notes |
|---|---|---|---|---|
| typst | 0.15.1 | Apache-2.0 | 1.92 | **no Cargo features**. Re-exports `typst_library::*`, `pub use typst_syntax as syntax`, `pub use typst_utils as utils`, `pub extern crate comemo; pub extern crate ecow;`. Depends on typst-eval, typst-html, typst-layout, typst-library, typst-macros, typst-realize, typst-syntax, typst-timing, typst-utils. |
| typst-pdf | 0.15.1 | Apache-2.0 | 1.92 | no features; uses krilla 0.8.2 (`raster-images, comemo, rayon, pdf`), krilla-svg 0.8.1, image 0.25.5 (png/jpeg/gif/webp), flate2 (zlib-rs). |
| typst-svg | 0.15.1 | Apache-2.0 | 1.92 | no features. |
| typst-render | 0.15.1 | Apache-2.0 | 1.92 | no features; tiny-skia 0.12 (PNG encoding available: `Pixmap::encode_png()` used by typst-cli and verified in scratch). |
| typst-html | 0.15.1 | Apache-2.0 | 1.92 | no features; always built as a dep of `typst`. |
| typst-layout | 0.15.1 | Apache-2.0 | 1.92 | **must be a direct dependency**: `typst_layout::{PagedDocument, Page}` are *not* re-exported by `typst` (checked `typst/src/lib.rs`, `typst-library/src/lib.rs`). |
| typst-kit | 0.15.1 | Apache-2.0 | 1.92 | features below. `default = []`. |
| typst-assets | 0.15.1 | Apache-2.0 | unknown | feature `fonts = []`. |
| typst-syntax | 0.15.1 | Apache-2.0 | 1.92 | parser, `FileId`, `Source`, `package::PackageSpec`. |
| typst-library | 0.15.1 | Apache-2.0 | 1.92 | `World`, `Library`, diag types; "fully reexported by the `typst` crate". |
| typst-utils | 0.15.1 | Apache-2.0 | 1.92 | `LazyHash`, `hash128`, `version()`. Has a `build.rs` that sets `TYPST_VERSION` from `CARGO_PKG_VERSION` if unset and runs `git rev-parse HEAD` to populate `TYPST_COMMIT_SHA` (harmless; will pick up *writer_rs*'s HEAD). *(verified: source)* |
| comemo | 0.5.1 | MIT OR Apache-2.0 | 1.88 | `default = ["macros"]`; `pub use crate::memoize::evict;`. |
| ecow | 0.2.6 (typst pins `0.2.6`, `features=["serde"]`) | — | — | `EcoString`, `EcoVec`, `eco_format!`. Available as `typst::ecow`. |

**typst-kit 0.15.1 features** (from registry `Cargo.toml`, *(verified: source)*):

```toml
bundle            = ["dep:typst-bundle"]
datetime          = ["dep:chrono"]                       # typst_kit::datetime::Time
default           = []
embedded-fonts    = ["dep:typst-assets", "typst-assets/fonts"]   # typst_kit::fonts::embedded()
emit-diagnostics  = ["dep:codespan-reporting"]           # typst_kit::diagnostics::emit (codespan-reporting 0.11)
http-server       = ["dep:tiny_http", "dep:infer", "dep:percent-encoding"]  # typst_kit::server::HttpServer
scan-fonts        = ["dep:fontdb", "fontdb/memmap", "fontdb/fontconfig", "dep:dirs"]  # fonts::scan(), fonts::system()
system-downloader = ["dep:env_proxy", "dep:native-tls", "dep:ureq", "dep:openssl"]    # downloader::SystemDownloader (ureq 2)
system-files      = ["system-packages"]                  # files::SystemFiles
system-packages   = ["dep:dirs", "universe-packages"]    # packages::SystemPackages, FsPackages::system_data/cache
timer             = []
universe-packages = ["dep:flate2", "dep:tar", "dep:fastrand"]   # packages::UniversePackages
vendor-openssl    = ["openssl/vendored"]
watcher           = ["dep:notify", "dep:same-file"]      # watcher::Watcher (notify 8)
```

Non-optional typst-kit deps: ecow, once_cell, parking_lot, rustc-hash, serde, serde_json, url,
typst-library, typst-syntax, typst-timing, typst-utils.

typst-cli v0.15.1 enables: `bundle, embedded-fonts, scan-fonts, system-packages,
universe-packages, datetime, emit-diagnostics, system-downloader, watcher, timer` and (by its
own default features) `http-server`. *(verified: downloaded `crates/typst-cli/Cargo.toml`)*

**Edition/MSRV.** All typst crates are `edition = "2024"`, `rust-version = "1.92"`. Our scratch
crate used `edition = "2021"` and compiled fine with Rust 1.99.1.

**Changelog facts** *(verified: docs, typst.app/docs/changelog)*: 0.14.0 released
2025-10-24 (MSRV 1.88; `Default` for `Library` removed → `use typst::LibraryExt`;
`PdfOptions.tagged` added; "many more built-in elements in HTML export"; typed HTML API).
0.15.0 released 2026-06-15 (MSRV 1.92; "typst-kit … completely reworked"; MathML export;
`target()` usable without the html flag; HTML minified by default, `--pretty`; experimental
*bundle* export; diagnostic hints can carry spans). 0.15.1 released 2026-07-17 (New Computer
Modern fonts updated to 8.1.1; `html.frame` SVG respects pretty printing; bundle watch server
MIME fixes; `typst eval` exit code).

---

## 2. Core API, exact signatures (typst 0.15.1)

### 2.1 `typst::World` (defined in `typst-library/src/lib.rs`, re-exported) *(verified: source)*

```rust
#[comemo::track]
pub trait World: Send + Sync {
    /// The standard library. Can be created through `Library::build()`.
    fn library(&self) -> &LazyHash<Library>;
    /// Metadata about all known fonts.
    fn book(&self) -> &LazyHash<FontBook>;
    /// Get the file id of the main source file.
    fn main(&self) -> FileId;
    /// Try to access the specified file location as a source file.
    fn source(&self, id: FileId) -> FileResult<Source>;
    /// Try to access the specified file. For file locations for which `source` succeeds,
    /// this should also succeed. Bytes can be cheaply created from a Source via Bytes::from_string.
    fn file(&self, id: FileId) -> FileResult<Bytes>;
    /// Try to access the font with the given index in the font book.
    /// NOTE: index is not guaranteed to be in bounds (may come from an outdated font book
    /// during incremental compilation validation) -> return None, never panic.
    fn font(&self, index: usize) -> Option<Font>;
    /// Get the current date. If no offset is specified, the local date should be chosen.
    /// Returning None makes typst's `datetime.today()` error.
    fn today(&self, offset: Option<Duration>) -> Option<Datetime>;
}
```
`World` is implemented for `Box<W>`, `Arc<W>`, `&W`. Trait doc: "All loading functions
(`main`, `source`, `file`, `font`) should perform internal caching so that they are
relatively cheap on repeated invocations with the same argument. `Source`, `Bytes`, and `Font`
are all reference-counted and thus cheap to clone." and "Advanced clients like language servers
can also retain the source files and edit them in-place to benefit from better incremental
performance."

```rust
pub trait WorldExt {                      // impl for all T: World + ?Sized
    /// Byte range for a span. None if the span does not point into any file.
    fn range(&self, span: impl Into<DiagSpan>) -> Option<Range<usize>>;
}
```

### 2.2 Library construction *(verified: source)*

```rust
pub trait LibraryExt {                    // typst::LibraryExt (in typst/src/lib.rs)
    fn default() -> Library;              // == Self::builder().build()
    fn builder() -> LibraryBuilder;
}
impl LibraryBuilder {
    pub fn with_inputs(mut self, inputs: Dict) -> Self;       // visible as `sys.inputs`
    pub fn with_features(mut self, features: Features) -> Self;
    pub fn build(self) -> Library;
}
pub struct Features(SmallBitSet);   // Default = none; FromIterator<Feature>; ::all(); ::none(); is_enabled(Feature)
#[non_exhaustive] pub enum Feature { Html, Bundle, A11yExtras }
```
`Library` is `#[non_exhaustive]` with pub fields `routines, global, math, styles, rules, std,
features`. The `html` module is only defined in the global scope when `Feature::Html` is enabled
(`if features.is_enabled(Feature::Html) { global.define("html", (routines.html_module)()); }`).
typst-cli builds it as:
```rust
let inputs: Dict = world_args.inputs.iter().map(|(k, v)| (k.as_str().into(), v.as_str().into_value())).collect();
let features = process_args.features.iter().copied().map(Into::into).collect();
Library::builder().with_inputs(inputs).with_features(features).build()
```

### 2.3 Compilation entry points *(verified: source, `typst/src/lib.rs`)*

```rust
/// Compiles sources into an output. Supported outputs: PagedDocument (typst_layout),
/// HtmlDocument (typst_html) [and typst_bundle::Bundle].
pub fn compile<T>(world: &dyn World) -> Warned<SourceResult<T>> where T: Output;
pub fn trace<T>(world: &dyn World, span: Span) -> EcoVec<(Value, Option<Styles>)> where T: Output;
```
Internally: `T::target()` selects `Target::Paged | Html | Bundle`; for Html without
`Feature::Html` it **bails** with
`"html export is only available when `--features html` is passed"`; with the feature it
**warns** `"html export is under active development and incomplete"` (hints: "its behaviour may
change at any time", "do not rely on this feature for production use cases",
"see https://github.com/typst/typst/issues/5512 for more information"). It then evals the main
file (`typst_eval::eval`, memoized), then lays out / converts in a loop of at most
`MAX_ITERS` (5) iterations until introspection stabilises (`comemo::Constraint::validate`).
`Output` trait: `typst::foundations::Output { fn target() -> Target; fn create(engine, content, styles) -> SourceResult<Self>; fn introspector(&self) -> &dyn Introspector; }`.

### 2.4 Diagnostics *(verified: source, `typst-library/src/diag.rs`)*

```rust
pub type SourceResult<T> = Result<T, EcoVec<SourceDiagnostic>>;
pub type StrResult<T> = Result<T, EcoString>;
pub type HintedStrResult<T> = Result<T, HintedString>;
pub type FileResult<T> = Result<T, FileError>;

pub struct Warned<T> { pub output: T, pub warnings: EcoVec<SourceDiagnostic> }

pub struct SourceDiagnostic {
    pub severity: Severity,                       // enum Severity { Error, Warning }
    pub span: DiagSpan,                           // typst::syntax::DiagSpan
    pub message: EcoString,
    pub trace: EcoVec<Spanned<Tracepoint>>,       // call trace (Tracepoint::Call/Show/Import/Include)
    pub hints: EcoVec<Spanned<EcoString, DiagSpan>>,  // hint.v text; hint.span usually detached
}
impl SourceDiagnostic {
    pub fn error(span: impl Into<DiagSpan>, message: impl Into<EcoString>) -> Self;
    pub fn warning(span: impl Into<DiagSpan>, message: impl Into<EcoString>) -> Self;
    pub fn hint(&mut self, hint: impl Into<EcoString>);
    pub fn with_hint(self, hint: impl Into<EcoString>) -> Self;
    pub fn with_hints(self, hints: impl IntoIterator<Item = EcoString>) -> Self;
}
pub enum FileError { NotFound(PathBuf), AccessDenied, IsDirectory, NotSource, InvalidUtf8,
                     Realize(RealizeError), Package(PackageError), Other(Option<EcoString>) }
impl FileError { pub fn from_io(err: io::Error, path: &Path) -> Self; }
pub enum PackageError { NotFound(PackageSpec), VersionNotFound(PackageSpec, PackageVersion),
                        NetworkFailed(Option<EcoString>), MalformedArchive(Option<EcoString>),
                        Other(Option<EcoString>) }
```
`DiagSpan` (typst-syntax `span.rs`): `pub fn id(self) -> Option<FileId>`,
`pub fn get(self) -> DiagSpanKind` where
`enum DiagSpanKind { Detached, Number { id, num, sub_range }, Range { id, range } }`,
`is_detached()`. `Span: Into<DiagSpan>`.

**Getting file + line/col (the snippet used in the scratch crate, works):**
```rust
use typst::{World, WorldExt};
fn describe(world: &dyn World, diag: &SourceDiagnostic) -> String {
    let loc = diag.span.id().and_then(|id| {
        let range = world.range(diag.span)?;                 // byte range in that file
        let source = world.source(id).ok()?;
        let (line, col) = source.lines().byte_to_line_column(range.start)?; // 0-based
        Some(format!("{}:{}:{}", id.vpath().get_without_slash(), line + 1, col + 1))
    });
    format!("{}: {:?}: {}", loc.unwrap_or_else(|| "<detached>".into()), diag.severity, diag.message)
}
// prints e.g. `main.typ:3:12: Error: unknown variable: undefined_function`  (verified: run)
```
`Lines<String>` helpers (typst-syntax `lines.rs`): `byte_to_line`, `byte_to_column` (chars),
`byte_to_line_column`, `byte_to_utf16`, `line_to_range`, `line_column_to_byte`, `len_lines`,
`text()`. `Source::range(num: SpanNumber, sub_range: Option<SubRange>) -> Option<Range<usize>>`
is the low-level form; prefer `WorldExt::range`.

Pretty terminal rendering: `typst_kit::diagnostics::emit(dest: &mut dyn WriteColor, world: &dyn DiagnosticWorld, diagnostics: impl IntoIterator<Item=&SourceDiagnostic>, format: DiagnosticFormat) -> Result<(), codespan_reporting::files::Error>` (feature `emit-diagnostics`; needs `impl DiagnosticWorld for W { fn name(&self, id: FileId) -> String }`; `typst_kit::diagnostics::termcolor` is re-exported; `termcolor::Buffer::no_color()` / `.into_inner()` (termcolor 1.4.1) gives plain text for a browser overlay). *(verified: source)*

### 2.5 Files, ids, sources *(verified: source, `typst-syntax/src/path.rs`, `source.rs`)*

```rust
pub enum VirtualRoot { Project, Package(PackageSpec) }
pub struct RootedPath { root: VirtualRoot, vpath: VirtualPath }
impl RootedPath {
    pub fn new(root: VirtualRoot, vpath: VirtualPath) -> Self;
    pub fn intern(self) -> FileId;
    pub fn root(&self) -> &VirtualRoot;
    pub fn vpath(&self) -> &VirtualPath;
    pub fn map(&self, f: impl FnOnce(&VirtualPath) -> VirtualPath) -> Self;
}
pub struct FileId(NonZeroU16);     // Copy; globally interned; Deref<Target = RootedPath>  (so id.vpath(), id.root() work)
impl FileId { pub fn new(path: RootedPath) -> Self; pub fn unique(path: RootedPath) -> Self; pub fn get(&self) -> &'static RootedPath; }
pub struct VirtualPath(Segments);  // absolute, normalized path *within* a root
impl VirtualPath {
    pub fn new(path: impl AsRef<str>) -> Result<Self, PathError>;     // "main.typ" or "/main.typ"; rejects escapes/backslashes
    pub fn virtualize(root: &Path, path: &Path) -> Result<Self, VirtualizeError>; // real path -> vpath (replaces deprecated within_root)
    pub fn realize(&self, root: &Path) -> Result<PathBuf, RealizeError>;        // vpath -> real path (replaces deprecated resolve)
    pub fn get_with_slash(&self) -> &str;  pub fn get_without_slash(&self) -> &str;
    pub fn extension(&self) -> Option<&str>; pub fn with_extension(&self, ext: &str) -> Self;
    pub fn join(&self, path: &str) -> Result<Self, PathError>;
}
```
Max 65535 distinct `FileId`s per process ("out of file ids" panic) — fine for writer.

```rust
pub struct Source(Arc<LazyHash<SourceInner>>);   // Clone is cheap; Send + Sync
impl Source {
    pub fn new(id: FileId, text: String) -> Self;
    pub fn detached(text: impl Into<String>) -> Self;
    pub fn id(&self) -> FileId;  pub fn text(&self) -> &str;  pub fn root(&self) -> &SyntaxNode;
    pub fn lines(&self) -> &Lines<String>;
    /// Fully replace the source text (naive prefix/suffix diff -> single edit -> incremental reparse).
    pub fn replace(&mut self, new: &str) -> Range<usize>;
    pub fn edit(&mut self, replace: Range<usize>, with: &str) -> Range<usize>;
}
impl AsRef<str> for Source;   // => Bytes::from_string(source.clone()) works
```
```rust
impl Bytes {   // typst::foundations::Bytes (Arc'd, cheap clone)
    pub fn new<T: AsRef<[u8]> + Send + Sync + 'static>(data: T) -> Self;
    pub fn from_string<T: AsRef<str> + Send + Sync + 'static>(data: T) -> Self;  // no copy for String/EcoString/Source
    pub fn as_slice(&self) -> &[u8];  pub fn as_str(&self) -> Result<&str, Utf8Error>;
    pub fn into_string(self) -> Result<String, IntoStringError>;
    pub fn lines(&self) -> Result<Lines<String>, Utf8Error>;
}
```

### 2.6 Fonts *(verified: source)*

```rust
// typst::text
impl Font { pub fn new(data: Bytes, index: u32) -> Option<Self>;   // parse one face (index in collection)
            pub fn iter(data: Bytes) -> impl Iterator<Item = Self>; // all faces in a .ttc/.otf
            pub fn data(&self) -> &Bytes; pub fn index(&self) -> u32; pub fn info(&self) -> &FontInfo; }
impl FontBook { pub fn new() -> Self; pub fn from_infos(infos: impl IntoIterator<Item = FontInfo>) -> Self;
                pub fn push(&mut self, info: FontInfo); pub fn info(&self, index: usize) -> Option<&FontInfo>;
                pub fn families(&self) -> impl Iterator<Item = (&str, impl Iterator<Item = usize>)>;
                pub fn select(&self, family: &str, variant: FontVariant) -> Option<usize>;
                pub fn select_family(&self, family: &str) -> impl Iterator<Item = usize>;
                pub fn select_fallback(...) }
pub struct FontInfo { pub family: String, pub variant: FontVariant, pub flags: FontFlags, pub axes: Vec<FontAxis>, pub coverage: Coverage }

// typst_kit::fonts
pub struct FontStore { book: LazyHash<FontBook>, slots: Vec<FontSlot> }
impl FontStore {
    pub fn new() -> Self;
    pub fn push(&mut self, entry: (impl FontSource, FontInfo));
    pub fn extend<T: FontSource>(&mut self, entries: impl IntoIterator<Item = (T, FontInfo)>);
    pub fn book(&self) -> &LazyHash<FontBook>;      // "Can directly be used to implement World::book"
    pub fn font(&self, index: usize) -> Option<Font>; // lazy-loads via OnceLock; out-of-range -> None
    pub fn source(&self, index: usize) -> Option<&dyn FontSource>;
}
pub trait FontSource: Send + Sync + Any { fn load(&self) -> Option<Font>; }   // impl for Font and FontPath
pub struct FontPath { pub path: PathBuf, pub index: u32 }                     // loads with fs::read on first use
#[cfg(feature = "embedded-fonts")] pub fn embedded() -> impl Iterator<Item = (Font, FontInfo)>;
#[cfg(feature = "scan-fonts")]     pub fn system()   -> impl Iterator<Item = (FontPath, FontInfo)>;  // OS font dirs (+ Adobe Fonts on macOS/Windows)
#[cfg(feature = "scan-fonts")]     pub fn scan(path: &Path) -> impl Iterator<Item = (FontPath, FontInfo)>; // recursive
```
Font indices = insertion order into the store; `World::font(i)` must map to the same `i` as the
book — `FontStore` guarantees this.

### 2.7 PDF export *(verified: source, `typst-pdf/src/lib.rs`, `metadata.rs`)*

```rust
pub fn pdf(document: &PagedDocument, options: &PdfOptions) -> SourceResult<Vec<u8>>;
pub fn pdf_in_bundle(document, options, anchors: &[(Location, EcoString)], link_resolver: Tracked<LateLinkResolver>) -> SourceResult<Vec<u8>>;

#[derive(Debug, Hash)]
pub struct PdfOptions {
    /// Stable document identifier; if Auto, a hash of title+author is used. "If you cannot
    /// provide such a stable identifier, just pass Smart::Auto".
    pub ident: Smart<String>,
    /// `/Creator` metadata; Auto => "Typst $version".
    pub creator: Smart<Option<String>>,
    /// Creation timestamp. "It will only be used if `set document(date: ..)` is `auto`."
    pub timestamp: Option<Timestamp>,
    /// Which pages to export; None => all.
    pub page_ranges: Option<PageRanges>,
    /// PDF standards to enforce.
    pub standards: PdfStandards,
    /// Tagged PDF (accessibility). Default true. (typst-cli forces false when --pages is used.)
    pub tagged: bool,
    /// Human-readable PDF.
    pub pretty: bool,
}
impl Default for PdfOptions { // ident: Auto, creator: Auto, timestamp: None, page_ranges: None,
                              // standards: PdfStandards::default() (= PDF 1.7), tagged: true, pretty: false }
pub struct PdfStandards { .. }  // Clone; PdfStandards::new(list: &[PdfStandard]) -> HintedStrResult<Self>
#[non_exhaustive] pub enum PdfStandard { V_1_4, V_1_5, V_1_6, V_1_7, V_2_0, A_1b, A_1a, A_2b, A_2u, A_2a, A_3b, A_3u, A_3a, A_4, A_4f, A_4e, Ua_1 }
pub struct Timestamp { datetime: Datetime, timezone: Timezone }
impl Timestamp { pub fn new_utc(datetime: Datetime) -> Self;
                 pub fn new_local(datetime: Datetime, whole_minute_offset: i32) -> Option<Self>; }
pub enum Timezone { UTC, Local { hour_offset: i8, minute_offset: u8 } }
```
`Datetime::from_ymd(year: i32, month: u8, day: u8) -> Option<Datetime>`,
`Datetime::from_ymd_hms(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Option<Datetime>`.

**Reproducibility (verified: run).** With `timestamp: Some(Timestamp::new_utc(Datetime::from_ymd_hms(2026,10,24,0,0,0).unwrap()))`
two compiles of the same source produced byte-identical PDFs
(`hash128` equal: `154620534068593583043533786292127592023` both times; file contains
`/CreationDate(D:20261024000000Z)` and `/Creator(writer)`). typst-cli derives the timestamp from
`SOURCE_DATE_EPOCH` / `--creation-timestamp` (UTC) or `chrono::Local::now()` (local offset) —
see `pdf_options()` in `compile.rs`. Also keep `World::today` deterministic (fixed date) for
documents that call `datetime.today()`. PDF header is `%PDF-1.7`.

### 2.8 SVG / PNG export *(verified: source)*

```rust
// typst_svg
pub fn svg(page: &Page, opts: &SvgOptions) -> String;
pub fn svg_merged(document: &PagedDocument, opts: &SvgOptions, gap: Abs) -> String;  // all pages stacked vertically
pub fn svg_in_bundle(page, opts, anchors: &[(Point, EcoString)], link_resolver: Tracked<LateLinkResolver>) -> String;
pub fn svg_in_html(...)   // used by html.frame
#[derive(Default)] pub struct SvgOptions { pub render_bleed: bool, pub pretty: bool }

// typst_render
pub fn render(page: &Page, opts: &RenderOptions) -> tiny_skia::Pixmap;   // .encode_png() -> Result<Vec<u8>, _>
pub fn render_merged(document: &PagedDocument, opts: &RenderOptions, gap: Abs, fill: Option<Color>) -> tiny_skia::Pixmap;
pub struct RenderOptions { pub pixel_per_pt: Scalar, pub render_bleed: bool }
impl Default for RenderOptions { pixel_per_pt: Scalar::new(2.0), render_bleed: false }  // typst-cli: Scalar::new(ppi / 72.0), default ppi 144
```
`typst_layout::PagedDocument { pub fn pages(&self) -> &[Page]; pub fn info_mut(&mut self) -> &mut DocumentInfo; pub fn introspector(&self) -> &Arc<PagedIntrospector>; }`;
`typst_layout::Page { pub frame: Frame, pub bleed: Sides<Abs>, pub fill: Smart<Option<Paint>>, pub numbering: Option<Numbering>, pub supplement: Content, pub number: u64 }`.
`DocumentInfo { pub title: Option<EcoString>, pub author: Vec<EcoString>, pub description: Option<EcoString>, pub keywords: Vec<EcoString>, pub date: Smart<Option<Datetime>>, .. }`.
SVG text is emitted as glyph outlines in `<symbol id="g…">` + `<use>` (0 `<text>` elements in
the sample) — not selectable/searchable text. *(verified: run)*

### 2.9 HTML export API *(verified: source, `typst-html/src/{lib,encode,document,dom}.rs`)*

```rust
// typst_html
pub fn html(document: &HtmlDocument, options: &HtmlOptions) -> SourceResult<String>;   // emits "<!DOCTYPE html>" + tree
pub fn html_in_bundle(root: &HtmlElement, options: &HtmlOptions, link_resolver: Tracked<LateLinkResolver>) -> SourceResult<String>;
#[derive(Default)] pub struct HtmlOptions { pub pretty: bool }
pub struct HtmlDocument { output: HtmlOutput, info: DocumentInfo, introspector: Arc<HtmlIntrospector> }
impl HtmlDocument { pub fn new(output: HtmlOutput, info: DocumentInfo) -> Self; pub fn root(&self) -> &HtmlElement;
                    pub fn root_mut(&mut self) -> &mut HtmlElement; pub fn root_node(&self) -> &HtmlNode;
                    pub fn info_mut(&mut self) -> &mut DocumentInfo; pub fn introspector(&self) -> &Arc<HtmlIntrospector>; }
pub enum HtmlNode { Tag(..), Text(EcoString, Span), Element(HtmlElement), Frame(HtmlFrame) }  // (variants per dom.rs; Text/Element/Frame verified)
pub struct HtmlElement { pub tag: HtmlTag, pub attrs: HtmlAttrs, pub css: css::Properties, pub children: EcoVec<HtmlNode>,
                         pub parent: Option<Location>, pub span: Span, pub pre_span: bool }
pub struct HtmlAttrs(pub EcoVec<(HtmlAttr, EcoString)>);   // HtmlTag::intern(&str), HtmlAttr::intern(&str)
pub struct HtmlFrame { pub inner: Frame, pub text_size: Abs, pub id: Option<EcoString>, pub css: css::Properties, pub anchors: EcoVec<(Point, EcoString)>, pub span: Span }
pub fn module() -> Module;   // the `html` scope: HtmlElem (`html.elem`), FrameElem (`html.frame`), typed constructors (html.div, ...)
```
The DOM is public, so writer could walk `HtmlDocument::root()` (e.g. for a future
"typst → LaTeX via semantic HTML" path) — but see §6 limitations.

---

## 3. How typst-cli v0.15.1 implements `SystemWorld` *(verified: downloaded source)*

Files: `.../scratchpad/downloads/typst-cli-v0.15.1/{world.rs,compile.rs,watch.rs,fonts.rs,packages.rs,download.rs,args.rs,main.rs}`.

```rust
pub struct SystemWorld {
    workdir: Option<PathBuf>,
    library: LazyHash<Library>,
    fonts: LazyLock<FontStore, Box<dyn Fn() -> FontStore + Send + Sync>>,  // scanned lazily on first use
    files: FileStore<SystemFiles>,          // typst_kit::files::FileStore<L: FileLoader>
    now: Time,                              // typst_kit::datetime::Time (fixed or system, memoized per compile)
}
impl World for SystemWorld {
    fn library(&self) -> &LazyHash<Library> { &self.library }
    fn book(&self) -> &LazyHash<FontBook> { self.fonts.book() }
    fn main(&self) -> FileId { self.files.loader().main }
    fn source(&self, id: FileId) -> FileResult<Source> { self.files.source(id) }
    fn file(&self, id: FileId) -> FileResult<Bytes> { self.files.file(id) }
    fn font(&self, index: usize) -> Option<Font> { self.fonts.font(index) }
    fn today(&self, offset: Option<Duration>) -> Option<Datetime> { self.now.today(offset) }
}
impl SystemWorld {
    pub fn dependencies(&mut self) -> impl Iterator<Item = PathBuf> + '_ { let (loader, deps) = self.files.dependencies(); deps.filter_map(|id| loader.resolve(id).ok()) }
    pub fn reset(&mut self) { self.files.reset(); self.now.reset(); }   // between compilations
    pub fn scan_fonts(&mut self) { LazyLock::force(&self.fonts); }
}
struct SystemFiles { main: FileId, project: FsRoot, packages: SystemPackages }
impl FileLoader for SystemFiles {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        if id == *EMPTY_ID { Ok(Bytes::new([])) } else if id == *STDIN_ID { read_from_stdin().map(Bytes::new) }
        else { self.root(id)?.load(id.vpath()) }
    }
}
fn root(&self, id: FileId) -> FileResult<FsRoot> {
    Ok(match id.root() { VirtualRoot::Project => self.project.clone(),
                         VirtualRoot::Package(spec) => self.packages.obtain(spec)? })
}
```
Main file: `RootedPath::new(VirtualRoot::Project, VirtualPath::virtualize(&root, &input_path)?).intern()`;
project root = `--root` or the input file's parent dir (canonicalized). stdin/empty use
`FileId::unique(RootedPath::new(VirtualRoot::Project, VirtualPath::new("<stdin>").unwrap()))`.

**`typst_kit::files::FileStore`** (the "file slots"): `FileStore::new(loader)`, `source(id)`,
`file(id)`, `dependencies() -> (&L, impl Iterator<Item=FileId>)`, `reset()`. `reset()` marks all
slots stale; on the next `source()` the stale `Source` is updated with `Source::replace(new)`
("source files will be edited in place with updated data, leading to improved incremental
compilation performance"). A `file()` after `source()` is served from the same buffer
(`Bytes::from_string(source)`); a UTF-8 BOM is stripped for the source only.
`pub trait FileLoader { fn load(&self, id: FileId) -> FileResult<Bytes>; }` — "match on the
`root()` of the id to check whether the file should be loaded from the project or a package".
`FsRoot::new(PathBuf)`, `FsRoot::resolve(&VirtualPath) -> FileResult<PathBuf>`,
`FsRoot::load(&VirtualPath) -> FileResult<Bytes>` (denies escaping the root lexically; "It can
still escape via symlinks").

**Packages.** `SystemPackages::new(downloader)` = `from_parts(FsPackages::system_data(), FsPackages::system_cache(), UniversePackages::new(downloader))`.
`obtain(&spec)` order: data dir → cache dir → **download only if `spec.namespace == "preview"`**
(`UniversePackages::NAMESPACE`), else `Err(PackageError::NotFound(spec))`. Directory layout
`FsPackages::obtain`: `{path}/{namespace}/{name}/{version}` (plus `typst.toml` inside, read by the
compiler). Default dirs: data `~/Library/Application Support/typst/packages` (macOS),
`$XDG_DATA_HOME/typst/packages` or `~/.local/share/typst/packages` (Linux),
`%APPDATA%/typst/packages` (Windows); cache `~/Library/Caches/typst/packages` (macOS),
`$XDG_CACHE_HOME/typst/packages` or `~/.cache/typst/packages`, `%LOCALAPPDATA%/typst/packages`.
CLI overrides: `--package-path`/`TYPST_PACKAGE_PATH`, `--package-cache-path`/`TYPST_PACKAGE_CACHE_PATH`.
Registry URL `https://packages.typst.org/{preview}/{name}-{version}.tar.gz`, index
`https://packages.typst.org/preview/index.json` (format "not specified or stabilized").
Downloader: `pub trait Downloader: Send + Sync + 'static { fn stream(&self, key: &dyn Any, url: &str) -> io::Result<(Option<usize>, Box<dyn Read>)>; fn download(..) -> io::Result<Vec<u8>> {default} }`;
`SystemDownloader::new(user_agent)` (ureq 2 + native-tls, honours proxy env; feature
`system-downloader`). typst-cli wraps it in `ProgressDownloader::new(system, |key| PrintProgress(..))`.

**How to disable the network / serve `@writer` ourselves.** Three verified options:
(a) don't use `SystemPackages` at all — implement `FileLoader`/`World::file` and match
`VirtualRoot::Package(spec)` yourself (what the scratch `pkg.rs` does from memory, §7);
(b) `SystemPackages::from_parts(Some(FsPackages::new(dir)), None, UniversePackages::new(NoNet))`
with a `Downloader` that returns `Err(io::ErrorKind::NotFound.into())` (that is exactly what
typst-kit's own unit test `DummyDownloader` does) — then `@preview` yields `NotFound`;
(c) just avoid the `universe-packages`/`system-downloader` features: `FsPackages` alone has no
network code. Dependency tracking: files inside packages are *not* in `dependencies()` output
unless they resolve on disk (`loader.resolve(id).ok()`).

**Watch loop** (`watch.rs`): `Watcher::new(Some(output))?` → initial `compile_once` → loop
`{ watcher.update(world.dependencies())?; watcher.wait()?; world.reset(); compile_once(...)?; comemo::evict(10); }`.
`Watcher` (feature `watcher`, notify 8): batches events for 100 ms (max 500 ms), polls missing
files every 300 ms, ignores the output path, re-watches files after remove/rename. Eagerly calls
`world.scan_fonts()` for paged output "so that it's not counted as part of the displayed
compilation time".

**HTTP live-reload server** (`typst_kit::server::HttpServer`, feature `http-server`, tiny_http):
`HttpServer::new(title, port: Option<u16>, live: bool)`, `addr()`, `set_html(String)`,
`set_router(Fn(&str) -> Option<HttpBody>)`; ports 3000..=3005; injects
`<script>new EventSource("/__events").addEventListener("reload", () => location.reload())</script>`
before `</body>`; `/__events` is an SSE stream. (writer already has axum + its own live-reload in
presenter_rs; this is just a reference.)

**Fonts** (`fonts.rs::discover_fonts`): order is `system()` (unless `--ignore-system-fonts`),
then `embedded()` (unless `--ignore-embedded-fonts`), then `scan(path)` for each `--font-path`
(`TYPST_FONT_PATHS`, `:`-separated).

---

## 4. Incremental compilation (comemo 0.5.1) *(verified: source + run)*

* `World` is `#[comemo::track]`ed: every call made on the `Tracked<dyn World>` inside a
  memoized function is recorded as a *constraint* (hash of the call's return value). Memoized
  results are reused if replaying the recorded calls on the current world gives the same hashes
  (`Constraint::validate`). So changing `main.typ` invalidates `typst_eval::eval` (memoized,
  keyed on `&Source`) for that file, but memoized sub-results whose constraints still hold
  (layout of unchanged paragraphs, font lookups, other modules) are reused. Memoized top-level
  stages: `typst_eval::eval`, `typst_eval::eval_string`, `typst_layout::layout_document_impl`,
  `typst_html::html_document_impl`, plus many fine-grained ones (per-block layout, shaping).
* The cache is **process-global and static** (`Cache<C,Out>(LazyLock<RwLock<..>>)` per memoized
  fn + a global `EVICTORS` list). Nothing about the World object needs to be kept alive for
  caching; what matters is that `World::source(id)`/`file(id)`/`font(i)` return values that hash
  equal to the previous compile when unchanged (same `Source` instance or an in-place
  `Source::replace`), and that `World::library()`/`book()` are stable (`LazyHash` caches the
  hash). A new `Library` or a re-scanned `FontBook` each compile would invalidate everything.
* `pub fn evict(max_age: usize)`: "removes all memoized results from the cache whose age is
  larger than or equal to `max_age`. The age of a result grows by one during each eviction and
  is reset to zero when the result produces a cache hit. Set `max_age` to zero to completely
  clear the cache." typst-cli calls `comemo::evict(10)` after every watch recompile. Use the
  same in `writer serve`; call `evict(0)` for a one-shot `build` if memory matters (not needed).
* **Measured** (release, M4 Max, `big.rs`, 25-page document; two runs):

  | step | compile | pdf export |
  |---|---|---|
  | cold (fresh process) | 75.7 ms / 68.7 ms | 9.2 ms / 8.4 ms (255,953 B) |
  | unchanged source | 0.262 ms / 0.249 ms | 7.4 ms / 7.3 ms |
  | one paragraph edited (`Source::replace`) | 9.6 ms / 9.3 ms | 8.2 ms / 7.5 ms |
  | edited again | 10.3 ms / 9.2 ms | 8.8 ms / 7.8 ms |
  | append a section (+1 of 40) | 14.2 ms / 12.2 ms | 7.8 ms / 7.7 ms |
  | after `comemo::evict(0)` (no edit) | 53.7 ms / 61.7 ms | — |

  Tiny 1-page document (`main.rs`): cold 21.8/19.9 ms, unchanged 29/32 µs, edited 0.41–0.55 ms;
  PDF 2.5 ms cold then 0.5–0.9 ms; SVG 0.65–0.83 ms (87,891 B); PNG @2px/pt 5.5–7.6 ms
  (44,020 B). Building the embedded `FontStore` (17 faces, 4 families): 1.4–4.8 ms.
  Whole-process wall time for `typst-embed` (5 paged compiles + exports + HTML + error case):
  0.34 s on the 2nd launch (1.10 s on the very first launch of the fresh binary — macOS
  first-run verification). Expect `writer serve` recompiles of typical documents to be
  **≤ 15 ms** plus export; export cost (PDF ≈ 0.3 ms/page, PNG ≈ 6 ms/page at 2 px/pt) is the
  larger part.
* Multithreading: typst uses rayon internally (typst-pdf `rayon` feature; CLI sets
  `--jobs` via `rayon::ThreadPoolBuilder`). User CPU for the cold build was ~7× wall.

---

## 5. Reference scratch crate (copy this)

Location: `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/typst-embed/`
(`Cargo.toml`, `src/main.rs`, `src/bin/big.rs`, `src/bin/pkg.rs`, outputs in `out/`).

### 5.1 `Cargo.toml` *(verified: builds)*
```toml
[package]
name = "typst-embed"
version = "0.1.0"
edition = "2021"

[dependencies]
typst = "0.15.1"
typst-layout = "0.15.1"          # PagedDocument, Page (not re-exported by `typst`)
typst-html = "0.15.1"            # HtmlDocument, HtmlOptions, html()
typst-pdf = "0.15.1"
typst-svg = "0.15.1"
typst-render = "0.15.1"
typst-kit = { version = "0.15.1", default-features = false, features = ["embedded-fonts", "scan-fonts"] }
typst-assets = { version = "0.15.1", features = ["fonts"] }   # optional: typst-kit/embedded-fonts already enables it
comemo = "0.5.1"                 # or use typst::comemo
ecow = "0.2"                     # or use typst::ecow

[profile.release]
strip = true
lto = "thin"
```
(presenter_rs additionally uses `opt-level = 3`, `codegen-units = 1`; expect a longer link.)

### 5.2 `src/main.rs` — minimal in-memory World *(verified: builds and runs)*
```rust
//! Minimal in-memory typst World: one main file, embedded fonts, no packages.
use std::time::Instant;

use typst::diag::{FileError, FileResult, SourceDiagnostic, Warned};
use typst::foundations::{Bytes, Datetime, Duration, Smart};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Features, Library, LibraryExt, World, WorldExt};
use typst_html::{HtmlDocument, HtmlOptions};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;
use typst_pdf::{PdfOptions, Timestamp};
use typst_render::RenderOptions;
use typst_svg::SvgOptions;

/// A world that serves exactly one in-memory source file.
struct MemWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    main: FileId,
    source: Source,
}

impl MemWorld {
    fn new(text: &str, html: bool) -> Self {
        let mut fonts = FontStore::new();
        fonts.extend(typst_kit::fonts::embedded());
        let features: Features = if html { [Feature::Html].into_iter().collect() } else { Features::none() };
        let library = Library::builder().with_features(features).build();
        let main = RootedPath::new(VirtualRoot::Project, VirtualPath::new("main.typ").unwrap()).intern();
        Self {
            library: LazyHash::new(library),
            fonts,
            main,
            source: Source::new(main, text.to_owned()),
        }
    }

    /// Replace the main file's text in place (incremental reparse).
    fn set_text(&mut self, text: &str) {
        self.source.replace(text);
    }
}

impl World for MemWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main {
            Ok(Bytes::from_string(self.source.clone()))
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        Datetime::from_ymd(2026, 10, 24)
    }
}

/// Format a diagnostic as `file:line:col: severity: message` (1-based).
fn describe(world: &dyn World, diag: &SourceDiagnostic) -> String {
    let loc = diag.span.id().and_then(|id| {
        let range = world.range(diag.span)?;
        let source = world.source(id).ok()?;
        let (line, col) = source.lines().byte_to_line_column(range.start)?;
        Some(format!("{}:{}:{}", id.vpath().get_without_slash(), line + 1, col + 1))
    });
    let hints: Vec<String> = diag.hints.iter().map(|h| h.v.to_string()).collect();
    format!(
        "{}: {:?}: {}{}",
        loc.unwrap_or_else(|| "<detached>".into()),
        diag.severity,
        diag.message,
        if hints.is_empty() { String::new() } else { format!(" (hints: {})", hints.join(" | ")) }
    )
}

const DOC: &str = r#"
#set page(width: 210mm, height: 297mm)
#set text(font: "Libertinus Serif")

= A heading

This is a paragraph with *bold* and _italic_ text, an inline equation $x^2 + alpha$,
and a footnote.#footnote[Footnote text.] See @tab.

$ integral_0^1 x^2 dif x = 1/3 $

#figure(
  table(
    columns: 3,
    [a], [b], [c],
    [1], [2], [3],
  ),
  caption: [A small table],
) <tab>

```rust
fn main() {
    println!("hi");
}
```
"#;

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());

    // --- Paged compile loop: cold, unchanged, then three edits. ---
    let mut world = MemWorld::new(DOC, false);
    let mut pdf_hashes = Vec::new();
    for i in 0..5 {
        if i >= 2 {
            let edited = DOC.replace("This is a paragraph", &format!("This is paragraph #{i}"));
            world.set_text(&edited);
        }
        let t = Instant::now();
        let Warned { output, warnings } = typst::compile::<PagedDocument>(&world);
        let compile_time = t.elapsed();
        for w in &warnings {
            eprintln!("warning: {}", describe(&world, w));
        }
        match output {
            Ok(doc) => {
                let t = Instant::now();
                let options = PdfOptions {
                    ident: Smart::Auto,
                    creator: Smart::Custom(Some("writer".into())),
                    timestamp: Some(Timestamp::new_utc(Datetime::from_ymd_hms(2026, 10, 24, 0, 0, 0).unwrap())),
                    page_ranges: None,
                    standards: Default::default(),
                    tagged: true,
                    pretty: false,
                };
                let pdf = typst_pdf::pdf(&doc, &options).expect("pdf export");
                let pdf_time = t.elapsed();
                pdf_hashes.push(typst::utils::hash128(&pdf));
                let t = Instant::now();
                let svg = typst_svg::svg(&doc.pages()[0], &SvgOptions::default());
                let svg_time = t.elapsed();
                let t = Instant::now();
                let png = typst_render::render(&doc.pages()[0], &RenderOptions::default())
                    .encode_png()
                    .expect("png");
                let png_time = t.elapsed();
                println!(
                    "compile {i} ({}): compile={:?} pages={} warnings={} | pdf={:?} ({} B) svg={:?} ({} B) png={:?} ({} B)",
                    match i { 0 => "cold", 1 => "unchanged", _ => "edited" },
                    compile_time, doc.pages().len(), warnings.len(),
                    pdf_time, pdf.len(), svg_time, svg.len(), png_time, png.len()
                );
                if i == 0 {
                    std::fs::write(format!("{out_dir}/out.pdf"), &pdf).unwrap();
                    std::fs::write(format!("{out_dir}/page-1.svg"), &svg).unwrap();
                    std::fs::write(format!("{out_dir}/page-1.png"), &png).unwrap();
                }
            }
            Err(errors) => {
                for e in &errors {
                    eprintln!("error: {}", describe(&world, e));
                }
            }
        }
        comemo::evict(10);
    }
    println!("pdf hashes (compile 0 and 1 should match: fixed timestamp): {:?}", pdf_hashes);

    // --- HTML compile (needs Feature::Html in the Library). ---
    let world = MemWorld::new(DOC, true);
    let t = Instant::now();
    let Warned { output, warnings } = typst::compile::<HtmlDocument>(&world);
    let compile_time = t.elapsed();
    for w in &warnings {
        eprintln!("html warning: {}", describe(&world, w));
    }
    match output {
        Ok(doc) => {
            let html = typst_html::html(&doc, &HtmlOptions { pretty: true }).expect("html");
            println!("html compile: {:?} ({} B)", compile_time, html.len());
            std::fs::write(format!("{out_dir}/out.html"), &html).unwrap();
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("html error: {}", describe(&world, e));
            }
        }
    }

    // --- Error reporting check. ---
    let world = MemWorld::new("= Title\n\nSome text #undefined_function() here.\n", false);
    let Warned { output, .. } = typst::compile::<PagedDocument>(&world);
    match output {
        Ok(_) => println!("unexpected: error doc compiled"),
        Err(errors) => {
            for e in &errors {
                println!("expected error: {}", describe(&world, e));
            }
        }
    }
}
```

### 5.3 Measured results *(verified: run, 2026-10-24)*

```
$ /usr/bin/time -p cargo build --release      # cold, fresh CARGO_TARGET_DIR, 295 crates
    Finished `release` profile [optimized] target(s) in 57.74s
real 58.02   user 403.97   sys 24.49
$ stat -f %z target/release/typst-embed
47355536                                      # 45.16 MiB, strip=true, lto=thin, embedded fonts
$ target/release/typst-embed out/
compile 0 (cold): compile=21.808291ms pages=1 warnings=0 | pdf=2.527583ms (31033 B) svg=757.25µs (87891 B) png=6.963583ms (44020 B)
compile 1 (unchanged): compile=29.333µs pages=1 warnings=0 | pdf=511.916µs (31033 B) svg=641.167µs (87891 B) png=5.638583ms (44020 B)
compile 2 (edited): compile=490.542µs pages=1 warnings=0 | pdf=750.625µs (31053 B) svg=651.917µs (87890 B) png=5.836792ms (44010 B)
compile 3 (edited): compile=459.458µs pages=1 warnings=0 | pdf=752.708µs (31053 B) svg=641.542µs (87890 B) png=5.488167ms (44013 B)
compile 4 (edited): compile=433µs pages=1 warnings=0 | pdf=706.167µs (31162 B) svg=683.584µs (88566 B) png=5.642708ms (43993 B)
pdf hashes (compile 0 and 1 should match: fixed timestamp): [154620534068593583043533786292127592023, 154620534068593583043533786292127592023, 339622553650625754927544923577127920251, 254102300484691052753699018344454108911, 15625278629955662810761295442026218983]
html warning: <detached>: Warning: html export is under active development and incomplete (hints: its behaviour may change at any time | do not rely on this feature for production use cases | see https://github.com/typst/typst/issues/5512 for more information)
html warning: main.typ:2:2: Warning: page set rule was ignored during HTML export
html compile: 1.163292ms (3660 B)
expected error: main.typ:3:12: Error: unknown variable: undefined_function
real 1.10 (first launch) / real 0.34 (second launch)
```
Other builds against the same (warm) target dir: adding `src/bin/big.rs` → 18.7 s wall
(170 s CPU); adding `src/bin/pkg.rs` → 14.5 s; editing `big.rs` → 62 s (thin-LTO relink of
two bins); separate crate `typst-embed-nofonts` (typst-kit `scan-fonts` only, no typst-assets
fonts) → 33.7 s wall, binary **37,581,792 B**.

---

## 6. HTML export status in 0.15.1

### 6.1 Mechanics *(verified: source + run)*
* Not a Cargo feature. `typst_html` is always linked. Gate = `Feature::Html` in the `Library`
  (`Library::builder().with_features(Features::from_iter([Feature::Html]))`). The CLI maps
  `--features html` / `TYPST_FEATURES=html` to it (`args.rs`: `Feature { Html, Bundle, A11yExtras }`).
* `typst::compile::<HtmlDocument>(&world)` then `typst_html::html(&doc, &HtmlOptions { pretty })`.
  Every HTML compile carries the detached warning quoted in §2.3. Output is minified unless
  `pretty: true` (0.15.0 changelog: "HTML is minified by default").
* `html_document_common` (typst-html `document.rs`): realizes the content, converts to nodes,
  `finalize_dom` wraps in `<html>/<head>/<body>` unless the user emitted them with `html.elem`,
  resolves inline CSS, and — only if the document contains equations — appends a
  `<style>` with `EQUATION_CSS_STYLES` (MathML alignment/table/fraction/accent rules, ~70
  lines) to `<head>`. `<html lang="en">` from `text.lang`; `<meta charset>` and viewport meta
  are added.
* `page` set rules are ignored with a warning ("page set rule was ignored during HTML export").

### 6.2 What is converted (the registered HTML show rules, `typst-html/src/rules.rs`, verified: source)
`par, strong, emph, list, enum, terms, link, direct-link, divider, title (→<h1>), heading
(→<h2>…), figure (→<figure>), figure.caption (→<figcaption>), quote, footnote, footnote.marker,
footnote.container (→<section role="doc-endnotes"><ol>), footnote.entry, outline,
outline.entry, ref, cite-group, bibliography, csl-light, csl-indent, table (→<table>),
table.cell, sub, super, underline, overline, strike, highlight, smallcaps, raw (→<code
data-lang=…>, block: <pre><code>; syntax colours as <span style="color:#…">), raw.line, image
(→<img src="data:…;base64,…" width height alt>), equation (→<math> / <math display="block">
via MathML)`; `FrameElem` (`html.frame`) under the paged target just yields its body.
Equations: `EQUATION_RULE` calls `resolve_equation` then `convert_math_to_nodes` → MathML
(`typst-html/src/mathml.rs`). Verified sample output (pretty):
```html
<p>This is a paragraph with <strong>bold</strong> and <em>italic</em> text, an inline equation <math><msup><mi>𝑥</mi><mn>2</mn></msup><mo>+</mo><mi>𝛼</mi></math>, and a footnote.<sup id="loc-1" role="doc-noteref"><a href="#loc-2">1</a></sup> See <a href="#tab">Table 1</a>.</p>
<math display="block"><msubsup><mo lspace="0em">∫</mo><mn>0</mn><mn>1</mn></msubsup>…<mfrac><mn>1</mn><mn>3</mn></mfrac></math>
<figure id="tab"><table><tr><td>a</td><td>b</td><td>c</td></tr>…</table><figcaption>Table 1: A small table</figcaption></figure>
<pre><code data-lang="rust"><span style="color: #d73948">fn</span> <span style="color: #4b69c6">main</span>() {…}</code></pre>
<section role="doc-endnotes"><ol style="list-style-type: none"><li id="loc-2"><sup role="doc-backlink"><a href="#loc-1">1</a></sup>Footnote text.</li></ol></section>
```
Labels become `id="<label>"` on the element (`<figure id="tab">`); unlabeled link targets get
`id="loc-N"` (0.14: "Added support for intra-doc link targets"). Images are **inlined as
base64 data URLs** (`typst_svg::WebImage::new(&image).to_base64_url()`), with `width`/`height`
attributes always emitted (0.15). Tables: `<table><tr><td>` (header cells via `table.header`
not checked).

### 6.3 Official docs quotes *(verified: docs, typst.app/docs/reference/html/ and sub-pages, 2026-10-24)*
* HTML export is "currently under active development" and "only available for experimentation
  behind a feature flag." Not for production use.
* "Typst aims to capture a document's structure and produce semantic, accessible,
  human-readable HTML." "You control the HTML output by checking the `target` function and,
  when the target is HTML, generating raw HTML elements, mainly in templates and show rules."
* "The `html` format outputs a single standalone HTML file. The `bundle` target produces
  multi-document websites with assets. Fragment output is planned."
* "Typst does not currently output CSS. Automatic CSS generation is planned."
* Enabling: CLI `--features html` or `TYPST_FEATURES=html`; `--format html` or `.html` output.
  `typst watch` starts a live-reloading server (`--port`, `--no-reload`, `--no-serve`).
* `html.elem(tag: str, attrs: dictionary = (:), body: none | content = none) → content`:
  "Typst's HTML export automatically generates the appropriate tags for most elements.
  However, sometimes, it is desirable to retain more control." "Normally, Typst will generate
  `html`, `head`, and `body` tags for you. If you instead create them with this function, Typst
  will omit its own tags." Example: `#html.elem("div", attrs: (style: "background: aqua"))[A div with _Typst content_ inside!]`.
* `html.frame(body: content) → content`: "lays out its content as an inline SVG" using "the
  Typst layout engine that would also be used for PDF, SVG, and PNG export"; for "plots and
  other content that relies on positioning and styling".
* `target() → str`: returns `"paged"` (PDF/PNG/SVG, and inside an `html.frame`), `"html"`,
  `"bundle"`; contextual; usable without the html feature since 0.15.0.
* 0.15.0 changelog: "Mathematical equations are now automatically exported to MathML."
  (recommended over `html.frame` SVG for accessibility); `box`/`block` semantics aligned with
  paged export (breaking); paragraph grouping rules changed; `html.script`/`html.style` accept
  only strings; `<pre>` uses raw newlines.
* Tracking issue #5512 (body updated 2026-06-29, state open) — milestones: done: feature flag,
  realization/show rules/context, introspection (except positions), minified + basic pretty
  output, `target` (string form), `html.elem`, raw `<html>/<head>/<body>`, typed `html.*`
  constructors, raw bodies for `pre/script/style`, frame API, single-file output (CLI, watch,
  tests), directory output (CLI, watch, `asset`/`document` APIs), linking (labels, locations,
  frames, across files). **Not done**: spec-compliant nesting, prettier output, finalizing the
  `target` API, auto-generated asset paths, **fragment output**, safety levels, semantic-vs-visual
  control, accessibility audit. Library table: Model (bibliography, list, cite, document, emph,
  figure, footnote, heading, link, enum, outline, par, parbreak, quote, ref, strong, table,
  terms, title) all ✅; Text (highlight, linebreak, lower, overline, raw, smallcaps, smartquote,
  strike, sub, super, text, underline, upper) all ✅; Layout: box ⚠️1, block ⚠️1, rest "...";
  Visualize: image ✅, circle/curve/ellipse/line/polygon/rect/square ❌; Math table empty
  ("...") in the issue body (but MathML export shipped in 0.15.0 and is verified above).
  Scope statement: "Our primary focus in this initial phase of work will be on **generating
  semantic HTML with no CSS.**" Future: "CSS and EPUB support".

### 6.4 Implications for writer
* HTML from typst is semantic-only: writer must supply its own stylesheet (fine — presenter_rs
  already ships CSS). Headings start at `<h2>` (title → `<h1>`).
* Anything layout-ish (columns, place, rect, grid, page) needs `html.frame` (inline SVG) or a
  target-aware show rule in writer's typst prelude package.
* MathML renders natively in current Chrome/Firefox/Safari; no KaTeX needed for the typst
  backend's HTML. (Bundle export exists but is experimental; not needed.)
* The `writer` "typst backend → LaTeX" idea cannot reuse the HTML DOM for math (MathML, not
  TeX source); it must work from writer's own IR / the typst syntax tree.

---

## 7. Package namespaces (`#import "@writer/doc:0.1.0"`) *(verified: source + run)*

* Parsing (`typst-syntax/src/package.rs`): `PackageSpec::from_str` requires `@`, then
  namespace until `/`, name until `:`, then `major.minor.patch` (three `u32`s, no more, no
  less). Namespace and name must satisfy `typst_syntax::is_ident`: first char `XID_Start` or
  `_`, following chars `XID_Continue`, `_` or `-`. So `writer`, `writer-doc`, `local` are
  valid; `writer.rs`, `writer_rs.x` are not (`"\`writer.rs\` is not a valid package namespace"`).
  `Display` is `@{namespace}/{name}:{version}`. `VersionlessPackageSpec` + `PackageVersion`
  (Ord) also exist; `PackageVersion::compiler()` = 0.15.1.
* Resolution (`typst-eval/src/import.rs`): for a string starting with `@`:
  `spec.parse::<PackageSpec>()` → `resolve_package`: reads
  `RootedPath::new(VirtualRoot::Package(spec.clone()), VirtualPath::new("typst.toml").unwrap()).intern()`
  via **`World::file`**, parses `PackageManifest` (toml), `manifest.validate(&spec)` (name and
  version must match; `compiler = "0.15"` bound is checked with `PackageVersion::matches_ge`),
  then imports `manifest.package.entrypoint` resolved relative to the manifest (same package
  root). Nested `#import "util.typ"` inside the package resolves within the same
  `VirtualRoot::Package(spec)`. **No registry, directory or network is involved** — the World
  decides. The namespace string is not special-cased anywhere except `UniversePackages::NAMESPACE
  == "preview"` in typst-kit.
* Minimal manifest that validates (`PackageInfo` required fields): 
  ```toml
  [package]
  name = "doc"
  version = "0.1.0"
  entrypoint = "lib.typ"
  compiler = "0.15"    # optional VersionBound
  ```
* Verified with `src/bin/pkg.rs` (in-memory `HashMap<FileId, Bytes>` World, no typst-kit
  packages at all):
  ```
  custom namespace + in-memory image: OK, 1 page(s) in 5.87875ms   # #import "@writer/doc:0.1.0": callout ; image("assets/fig.png")
  expected error: "/main.typ":1:9: Error: package not found (searched for @preview/cetz:0.3.4)
  expected error: "/main.typ":1:9: Error: `writer.rs` is not a valid package namespace
  system font scan: 382 families in 54.847291ms
  ```
* typst-kit on-disk layout (`FsPackages::obtain`): `{dir}/{namespace}/{name}/{version}/typst.toml`
  with `dir` = `FsPackages::new(path)` or the system data dir (§3). typst/packages README:
  "You can create an arbitrary `{namespace}`. A good namespace for system-local packages is
  `local`." and "Packages in the data directory have precedence over ones in the cache
  directory." *(verified: docs, raw README.md of typst/packages, 2026-10-24)*
* Recommendation: embed writer's prelude package with `rust-embed` and serve it from
  `World::file` under `VirtualRoot::Package(PackageSpec { namespace: "writer", name: "doc", version })`;
  also honour `FsPackages::system_data()` (`@local/...`) for user packages; make `@preview`
  opt-in (`--allow-network`) via `SystemPackages` + `SystemDownloader`, otherwise `NotFound`.

---

## 8. Fonts *(verified: source + run)*

* `typst_assets::fonts()` (feature `fonts`) yields 17 files, 9,683,068 B (9.23 MiB) total:
  Libertinus Serif (Regular 337,132; Bold 293,908; Italic 332,312; BoldItalic 259,176;
  Semibold 291,640; SemiboldItalic 343,148), New Computer Modern Math (Regular 1,306,268;
  Book 1,432,068; Bold 1,232,148), NewCM10 (Regular 717,588; Bold 629,288; Italic 721,176;
  BoldItalic 609,000), DejaVu Sans Mono (Regular 340,712; Bold 331,992; Oblique 251,932;
  BoldOblique 253,580). `typst_kit::fonts::embedded()` turns them into 4 families:
  "Libertinus Serif", "New Computer Modern", "New Computer Modern Math", "DejaVu Sans Mono"
  (typst's default text/math/raw fonts, so a document with no `set text(font:)` renders
  without system fonts). typst-assets also always carries 14 Foxit PDF base-14 `.pfb`
  (264,741 B) for PDF, ICC profiles, ICU segmentation data (32 K), html/mathml data.
* **Binary size impact: +9,773,744 B** (47,355,536 with vs 37,581,792 without embedded fonts,
  both `strip=true, lto=thin`). Startup cost of parsing the 17 embedded faces: 1.4–4.8 ms.
* Adding fonts from a directory: `fonts.extend(typst_kit::fonts::scan(Path::new(dir)))`
  (feature `scan-fonts`; recursive; faces load lazily on first `World::font`). Scanning the
  typst-assets font dir (31 files): 7 families enumerated in the nofonts run. System fonts:
  `fonts.extend(typst_kit::fonts::system())` — **382 families in 54.8 ms** on this Mac (fontdb
  `load_system_fonts` + Adobe Fonts dirs). Order matters: `FontBook::select` picks the first
  matching face, so push embedded fonts *before* system fonts if you want deterministic output
  across machines (typst-cli pushes system first, then embedded, then `--font-path`).
* `World::font` must tolerate out-of-range indices (returns `None`) — `FontStore::font` does.

---

## 9. Verified API hints (snippets to copy)

```rust
// Build a library with html enabled and sys.inputs
use typst::{Feature, Features, Library, LibraryExt};
use typst::foundations::{Dict, IntoValue};
let inputs: Dict = [("theme", "dark")].into_iter().map(|(k, v)| (k.into(), v.into_value())).collect();
let features: Features = [Feature::Html].into_iter().collect();
let library = Library::builder().with_inputs(inputs).with_features(features).build();

// File ids
use typst::syntax::{FileId, RootedPath, VirtualPath, VirtualRoot};
use typst::syntax::package::PackageSpec;
let main: FileId = RootedPath::new(VirtualRoot::Project, VirtualPath::new("doc.typ").unwrap()).intern();
let spec: PackageSpec = "@writer/doc:0.1.0".parse().unwrap();
let manifest_id = RootedPath::new(VirtualRoot::Package(spec.clone()), VirtualPath::new("typst.toml").unwrap()).intern();
match id.root() { VirtualRoot::Project => {/* project file at id.vpath() */}, VirtualRoot::Package(spec) => {/* spec.namespace, spec.name, spec.version */} }
let real = id.vpath().realize(Path::new("/project/root"))?;             // Result<PathBuf, RealizeError>
let vpath = VirtualPath::virtualize(Path::new("/project/root"), Path::new("/project/root/a/b.typ"))?;

// Sources / bytes
let mut source = Source::new(main, text);           // keep it; on change: source.replace(&new_text);
let bytes = Bytes::from_string(source.clone());    // for World::file of a .typ
let bytes = Bytes::new(std::fs::read(path)?);      // for images/data

// Compile + export
use typst::diag::Warned; use typst_layout::PagedDocument; use typst_html::{HtmlDocument, HtmlOptions};
let Warned { output, warnings } = typst::compile::<PagedDocument>(&world);
let doc = output?;                                             // Err(EcoVec<SourceDiagnostic>)
let pdf: Vec<u8> = typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default())?;
let svg: String = typst_svg::svg(&doc.pages()[0], &typst_svg::SvgOptions::default());
let all: String = typst_svg::svg_merged(&doc, &typst_svg::SvgOptions::default(), typst::layout::Abs::pt(10.0));
let png: Vec<u8> = typst_render::render(&doc.pages()[0], &typst_render::RenderOptions { pixel_per_pt: typst::utils::Scalar::new(144.0 / 72.0), render_bleed: false }).encode_png()?;
let Warned { output, warnings } = typst::compile::<HtmlDocument>(&world);   // needs Feature::Html
let html: String = typst_html::html(&output?, &HtmlOptions { pretty: false })?;

// Reproducible PDF
use typst_pdf::{PdfOptions, PdfStandards, PdfStandard, Timestamp}; use typst::foundations::{Datetime, Smart};
let opts = PdfOptions { timestamp: Some(Timestamp::new_utc(Datetime::from_ymd_hms(2026, 1, 1, 0, 0, 0).unwrap())),
                        creator: Smart::Custom(Some("writer".into())), standards: PdfStandards::new(&[PdfStandard::A_2b])?, ..Default::default() };

// Watch loop skeleton (typst-cli watch.rs)
loop { watcher.update(world.dependencies())?; watcher.wait()?; world.reset(); compile_once(&mut world, &mut config)?; comemo::evict(10); }

// typst-kit system world pieces
use typst_kit::files::{FileStore, FileLoader, FsRoot}; use typst_kit::packages::{FsPackages, SystemPackages, UniversePackages};
use typst_kit::fonts::{FontStore, embedded, scan, system}; use typst_kit::datetime::Time;
let packages = SystemPackages::from_parts(FsPackages::system_data(), FsPackages::system_cache(), UniversePackages::new(downloader));
let root: typst_kit::files::FsRoot = packages.obtain(&spec)?;   // PackageResult<FsRoot>; downloads only for @preview
let now = Time::fixed_timestamp(1_700_000_000)?;  // or Time::system(); now.today(offset) implements World::today; now.reset() between compiles
```

---

## 10. Open questions

1. **Thin-LTO relink cost** (14–62 s per change to our own crate in release) will slow the
   fleet's edit-test loop; use `cargo build` (dev) or `lto = false` for a `dev-fast` profile and
   keep `lto = "thin"` only for release. Not measured for dev profile.
2. **`codegen-units = 1` + `opt-level = 3`** (presenter_rs profile) were not measured; expect a
   longer cold build than 58 s and a slightly smaller binary.
3. **Memory** of the comemo cache in a long `serve` session was not measured; typst-cli's
   `evict(10)` policy is the reference.
4. `typst-html` `HtmlNode` variants beyond `Text/Element/Frame` (a `Tag` variant for
   introspection exists, lines 101–120 of `dom.rs`) were not enumerated exactly — check before
   pattern-matching the DOM exhaustively.
5. Whether `table.header`/`table.footer` produce `<thead>/<th>` in HTML was not tested (the
   `TABLE_RULE` is ~115 lines in `rules.rs`).
6. Bundle export (`typst::compile::<typst_bundle::Bundle>`, crate `typst-bundle` 0.15.1,
   typst-kit feature `bundle`) was not investigated; it is experimental and not needed.
7. `typst_kit::fonts::system()` uses fontdb with `fontconfig` on Linux — cross-platform
   behaviour/time not measured here (macOS only: 382 families / 55 ms).
8. The version string typst reports (`sys.version`) comes from `TYPST_VERSION` at build time
   (defaults to the typst-utils crate version 0.15.1); the `git rev-parse HEAD` in
   typst-utils' build.rs will embed *writer_rs*'s commit as `TYPST_COMMIT_SHA` — cosmetic.
9. Web fetches of typst.app docs are summarised by a model, so the quotes above marked
   *(verified: docs)* are faithful but possibly shortened; the crate sources and the GitHub
   issue body (fetched raw via the API) are exact.
