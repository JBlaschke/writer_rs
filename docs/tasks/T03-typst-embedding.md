# T03 — typst compiler embedding: World, fonts, packages, compile, renditions, MathML

| | |
|---|---|
| Milestone | M0 |
| Size | L |
| Depends on | T01 |
| Parallel-safe with | T04, T10, T11, T13, T14, T17, T51 (wave 2, see the board) |
| Files you own | `src/typst/mod.rs` (replace stub; keep T00's `pub mod` lines), `src/typst/world.rs`, `src/typst/compile.rs`, `src/typst/render.rs`, `src/typst/html.rs`, `src/typst/sourcemap.rs` (replace stubs), `src/typst/package.rs` (replace stub; serving only, T31 later adds `eject`/`install_dir`), `tests/typst_world.rs`, `tests/typst_render.rs`, `tests/snapshots/typst_equation_css.snap` (new) |

## Goal
`crate::typst` embeds the typst 0.15.1 compiler: one long-lived in-memory `WriterWorld` per session (retained sources,
virtual files, embedded-first fonts, `@writer/doc` and the vendored `@preview/mitex` without network, an opt-in
downloader, the `IncludeProvider` hook), PDF compilation with diagnostics mapped to the user's lines, the oracle query,
equation/image renditions with baseline, and typst math → MathML. Consumers: T15/T1A, T16, T24, T30/T53, T31, T32,
T41/T42/T61, T73.

## Read first
- `docs/spec/typst-dialect.md` §1, §6.1–6.6, §8.1–8.3, §9, §11, Open points (T03), Deviations (Eviction,
  `equation_mathml`, `src/typst/html.rs`) — **this is your spec**; also §5.1 (`SourceMap` only), §4.1/§4.5 (package
  contents), §8.4 (the timestamp policy `pdf_timestamp` implements)
- `docs/spec/math.md` §3.4, §3.5, §11 (the two "typst 0.15.1" snippets), Fixtures (T03 row), Open point 5;
  `docs/spec/references.md` §7 (copy `oracle`)
- `docs/spec/vendored-assets.md` §3.1, §5.1, §5.3, §7.2, §7.3, Fixtures (T03's mitex test); `docs/spec/live-reload.md`
  §5.3 (how T16/T23 drive the World), §4.1, §2.4; `docs/spec/ir.md` §3, §5; `docs/spec/config.md` §2 (`[pdf]`,
  `[typst]`), §4. Consumers to skim: notebook-mapping.md §7.3, latex-output.md §1.1, docx-output.md §1.1, html-output.md §4.3
- `docs/research/R1-typst-embedding.md` §2–§5.2, §7–§9; `docs/research/R4-math.md` §5, §7, §9. Nothing is lifted
  from presenter_rs (R8 §10 has no typst row).

## API to implement
```rust
// src/typst/mod.rs (typst-dialect.md §1)
pub const WRITER_DOC_VERSION: &str = "0.1.0"; pub use crate::vendor::MITEX_VERSION;
// src/typst/sourcemap.rs (§5.1; plus `pub fn is_empty(&self) -> bool`, see Notes)
pub struct SourceMap { /* one Option<SourceSpan> per emitted line */ }
impl SourceMap { pub fn lookup(&self, line: usize /* 1-based */) -> Option<&SourceSpan>; pub fn len(&self) -> usize;
                 pub fn from_lines(lines: Vec<Option<SourceSpan>>) -> SourceMap; }
// src/typst/world.rs (§6.1, §6.4)
pub struct WriterWorld { /* LazyHash<Library> ×2 (paged, html), FontStore, slots, mounts, packages, … */ }
pub enum MainFile { Path(PathBuf) /* a .typ under root */, Virtual /* /__writer-main.typ */ }
pub struct WorldOptions { pub root: PathBuf, pub main: MainFile, pub today: Datetime,
    pub system_fonts: bool, pub font_paths: Vec<PathBuf>, pub package_paths: Vec<PathBuf>,
    pub allow_packages: bool, pub inputs: BTreeMap<String, String> }
impl WriterWorld {
    pub fn new(opts: WorldOptions) -> Result<Self>;
    pub fn from_config(config: &Config, root: &Path, main: MainFile, today: Datetime) -> Result<Self>; // inputs: world_inputs(config)
    pub fn set_main_text(&mut self, text: &str, map: SourceMap);      // Virtual main: Source::replace
    pub fn set_assets(&mut self, assets: AssetRegistry);
    pub fn mount(&mut self, prefix: &str, dir: &Path);                 // §6.3
    pub fn set_inputs(&mut self, inputs: BTreeMap<String, String>);    // rebuilds the Libraries only if changed
    pub fn set_include_provider(&mut self, provider: Box<dyn IncludeProvider>);
    pub fn reset(&mut self);                                           // before every rebuild
    pub fn accessed(&self) -> BTreeSet<PathBuf>;              pub fn include_diagnostics(&self) -> Diagnostics;
    pub fn source_map(&self, id: FileId) -> Option<SourceMap>; pub fn real_path(&self, id: FileId) -> Option<PathBuf>;
}
impl typst::World for WriterWorld { /* library, book, main, source, file, font, today */ }
pub fn world_inputs(config: &Config) -> BTreeMap<String, String>;
pub trait IncludeProvider: Send + Sync { fn synthesize(&self, path: &Path, root: &Path) -> Result<Synthesized>; }
pub struct Synthesized { pub text: String, pub map: SourceMap, pub assets: AssetRegistry,
                         pub files: Vec<PathBuf>, pub diagnostics: Diagnostics }
// src/typst/compile.rs (§8.1; references.md §7)
pub struct Compiled<T> { pub output: Option<T>, pub diagnostics: Diagnostics }
impl<T> Compiled<T> { pub fn into_result(self) -> Result<(T, Diagnostics)>; }
pub struct PdfOptions { pub timestamp: Datetime /* UTC */, pub ident: Option<String> }
pub struct Pdf { pub bytes: Vec<u8>, pub pages: usize }
pub struct OracleEntry { pub kind: String, pub number: Option<String> }
pub fn compile_document(world: &mut WriterWorld) -> Compiled<PagedDocument>;
pub fn compile_pdf(world: &mut WriterWorld, opts: &PdfOptions) -> Compiled<Pdf>;
pub fn oracle(document: &PagedDocument, label: &str) -> Option<OracleEntry>;
pub fn map_diagnostic(world: &WriterWorld, diag: &SourceDiagnostic) -> Diagnostic;
pub fn pdf_timestamp(pdf: &PdfConfig, meta: &DocMeta, source_date_epoch: Option<&str>, now: SystemTime) -> Result<Datetime>;
// src/typst/render.rs (§9; math.md §3.4)
pub struct Rendition { pub svg: Vec<u8>, pub png: Vec<u8> /* 2 px/pt */, pub pdf: Option<Vec<u8>>,
                       pub width_pt: f64, pub height_pt: f64, pub baseline_pt: f64 }
impl Rendition { pub fn descent_pt(&self) -> f64 /* height - baseline */; pub fn png_size(&self) -> (u32, u32); }
pub fn equation(world: &mut WriterWorld, source: &str, dialect: MathDialect, display: bool) -> Result<Rendition>;
pub fn image(world: &mut WriterWorld, bytes: &[u8], mime: &str, width_pt: Option<f64>) -> Result<Rendition>;
// src/typst/html.rs (§9; math.md §3.5)
pub const EQUATION_CSS: &str = "/* Alignment */\n…";   // typst-html 0.15.1 EQUATION_CSS_STYLES, expanded
pub fn equation_mathml(world: &mut WriterWorld, source: &str, display: bool) -> Result<String>;
```

## Steps
1. Check T00/T01 on `main`: the `src/typst/*.rs` stubs, `crate::vendor`, the vendored mitex 0.2.7 package under
   `assets/vendor/typst-packages/`, T01's `Config`, `DocMeta`, `SourceSpan`, `MathDialect`, `AssetRegistry`, `Error`,
   `Diagnostics`. The first `cargo test` builds ~300 crates at `opt-level = 2`: run it in the background (no `timeout`).
2. `mod.rs`: module doc, the two constants (no second `"0.2.7"` literal). `sourcemap.rs`: `#[derive(Debug, Clone,
   Default, PartialEq)]` over `Vec<Option<SourceSpan>>`; `lookup(0)` and lines past the end are `None`; add `is_empty`.
3. `package.rs` (`pub(crate)` store): §6.5's order, returning bytes and `Option<PathBuf>` (real path for disk roots
   only): (1) `vendor::get("typst/writer-doc/<vpath>")` for `writer/doc` at `WRITER_DOC_VERSION`,
   `vendor::typst_package_dir("preview", ..)` + vpath; (2) `package_paths` roots; (3) `SystemPackages::from_parts(..)`
   with `WriterDownloader` (ureq, user agent `writer/<CARGO_PKG_VERSION>`) under `allow_packages`, else `NoDownloader`.
   Another `writer/doc` version found nowhere → `PackageError::VersionNotFound(spec, v)`, `v` =
   `WRITER_DOC_VERSION.parse::<PackageVersion>()` (`FromStr`, package.rs:432; a commented `expect`: it is a constant).
4. `world.rs` types; `world_inputs`: `writer-version` = `env!("CARGO_PKG_VERSION")`, `writer-config` compact JSON in
   §6.2's key order (`serde_json::json!` keeps it: `preserve_order` is on, vendored-assets.md §7.3).
5. `new`: canonicalize `root` and a `MainFile::Path` (`VirtualPath::virtualize` is a lexical prefix test: `/var` vs
   `/private/var` breaks it); main id = `RootedPath::new(VirtualRoot::Project, VirtualPath::virtualize(&root,
   &input)?).intern()` or `/__writer-main.typ`; fonts per §6.6; both Libraries from one `Dict`. `from_config` maps
   `config.pdf.{system_fonts, font_paths, allow_packages}`, `config.typst.package_paths` (relative → `root`, Notes).
6. Slots: own `Mutex<HashMap<FileId, Slot>>` (std; `FileId` is not `Ord`) modelled on typst-kit's `FileSlot`: a slot
   keeps its `Source` across `reset()`, which marks it stale; the next access re-reads and calls `Source::replace`
   (no-op for equal text); `file()` of a `.typ` id is `Bytes::from_string(source.clone())`. Never rebuild
   `library()`/`book()` per compile.
7. Project vpaths in §6.3's order: `/__writer-main.typ` (never disk), `/__render/*` (step 11), `/__assets/<id>.<ext>`
   (the `set_assets` registry, then include assets), mounts (prefix normalised to a leading `/` without trailing `/`;
   an equal prefix is replaced; `VirtualPath::new(rest)?.realize(dir)`), `.md`/`.ipynb` (step 8), else
   `vpath.realize(root)` + `FileError::from_io`. Each disk read (project, mount, `File` asset, disk package) adds its
   real path to `accessed()`; embedded and virtual files never do.
8. Includes (§6.4): `source()` of a `.md`/`.ipynb` id calls `synthesize(&real, &root)` once per `reset()` cycle, merges
   `assets`, adds the include and `files` to `accessed()`, keeps `diagnostics` and `map`, retains the `Source`; `Err(e)`
   → `FileError::Other(Some(e.to_string().into()))`; no provider → `FileError::Other(Some("including .md/.ipynb from
   typst arrives in T53".into()))`. `file()` returns the disk bytes.
9. `set_assets` drops `/__assets/` slots; `set_inputs` rebuilds the Libraries only for a different map (count builds in
   a private field for the test); `reset` marks disk slots stale, clears `accessed` and the include state, and under
   `cfg(debug_assertions)` marks `@writer/doc` slots stale (`vendor::get` reads the disk in debug). `today(_)` =
   `Datetime::from_ymd` of `opts.today`. Restore the `pub(crate)` HTML-mode flag and main override (steps 11–12) on
   every return path, errors included.
10. `compile.rs`: `compile_document` = `typst::compile::<PagedDocument>(&*world)`, all warnings/errors through
    `map_diagnostic`; `compile_pdf` adds `typst_pdf::pdf` with §8.1's options; `into_result` builds `Error::Compile
    { path, line, message, hints }` from the first `Level::Error` (split at `"\n  hint: "`). `map_diagnostic` is §8.2
    (codes, trace rule via §11's snippet, `in @<ns>/<name>:<ver>/<file>:<line>` hint, package path form, preview hint
    verbatim); virtual-main and include lines go through their `SourceMap` (path: the include's real path, `None` for
    the virtual main, Notes); real files: `SourceSpan { start_line: l, end_line: l, cell: None, file: None }`. `oracle`
    is references.md §7's code. `pdf_timestamp` is §8.4; local choices to name in the PR: an unparsable
    `SOURCE_DATE_EPOCH` is `Error::Config`, a `^\d{4}-\d{2}-\d{2}$` `meta.date` that is no real date falls through.
11. `render.rs`: retained `/__render/main.typ` = math.md §11's `PRELUDE` + `#box($src$)`/`#box($ src $)`, or the mitex
    import + `#box(mi(<lit>))`/`#box(mitex(<lit>))` (`<lit>` = `` `src` `` without backticks, else a §5.2 string
    literal: a private helper, T30 comes later); page 1 → SVG, PNG at 2 px/pt, size, baseline per math.md §11 (none →
    `Error::Other("equation has no baseline")`), `pdf: None`. `image`: MIME → png/jpg/gif/webp/svg/pdf (other →
    `Error::Other`), `#image("/__render/image.<ext>", width: <w>pt)` (width only when given), `pdf: Some` with
    `timestamp: None`, baseline = height. Compile errors → `Error::Compile`. Never `comemo::evict` in `src/typst/`.
12. `html.rs`: print the `<style>` text `typst_html::html` emits for `$ x $` once (throwaway test), paste it as a raw
    string (1,494 B) under a comment crediting typst-html 0.15.1 `src/mathml.rs:99` (Apache-2.0). `equation_mathml`
    per §9: HTML mode, `${src}$`/`$ {src} $`, no preamble, first `<math`…`</math>`; drop only the detached "under
    active development" warning, any other → `Err(Error::Other(message))`. Then the tests; doc comments; ≤ ~80-line fns.

## Acceptance criteria
Worlds in tests use `system_fonts: false` and no network; inputs are inline or in `tempfile` dirs (PNGs from `render::equation`).
- `src/typst/sourcemap.rs` `tests::lookup_is_one_based`. `src/typst/world.rs`: `tests::world_inputs_default_config`
  (exactly `{"lang":"en","toc":0,"numbering":{"headings":false,"figures":"document","equations":"labelled"},"paper":"a4","cite_locale":""}`,
  `writer-version` `0.1.0`); `tests::world_inputs_follow_config` (`toc = true` → 3, `headings = true` → `"1.1"`,
  `figures = "section"`, locale `de`); `tests::set_inputs_unchanged_keeps_libraries`; `tests::mitex_from_embedded_store`
  (`#import "@preview/mitex:0.2.7": mi` + `#mi("\frac{a}{b}")` compiles with `allow_packages: false`, `accessed()`
  empty); `tests::embedded_fonts_first` (the four §6.6 families); `tests::md_include_without_provider` (`arrives in T53`).
- `tests/typst_world.rs`: `virtual_main_edits`; `typ_main_reset_and_accessed` (`MainFile::Path` + `#include "part.typ"`
  + `#image("fig.png")`: `accessed()` = the three canonical paths, empty after `reset()`; a disk edit compiles after
  it); `sandbox_rejects_parent` (`#read("../x.txt")`); `mounts_and_assets` (both accessed); `include_provider_synthesizes`
  (fake provider called once per cycle, `include_diagnostics()`, `source_map(id)`, `#read("part.md")` = disk bytes, its
  asset resolves); `include_provider_error`; `package_paths_root` (`@local/hello:0.1.0`); `preview_without_download`
  (`@preview/writer-no-such-package:0.0.1` → §8.2's hint); `writer_doc_other_version` (`@writer/doc:9.9.9` →
  `latest is 0.1.0`); `writer_doc_embedded` (compiles `#import "@writer/doc:0.1.0": *` when
  `vendor::get("typst/writer-doc/typst.toml")` is `Some`, else prints `skipping`); `font_paths_are_scanned`;
  `diagnostics_map_through_source_map` (virtual main line 3 mapped to 12 → `Error::Compile { line: Some(12), .. }`);
  `package_trace_located_at_call` (`.typ` main, `#mi("\foo{x}")` on line 2 → line 2, hint `in @preview/mitex:0.2.7/`);
  `pdf_is_reproducible` (two runs, fixed timestamp → equal bytes containing `writer 0.1.0`); `oracle_query` (inline
  `#metadata((kind: "figure", label: "fig-a", number: "1")) <writer-oracle-fig-a>`, a `none` number, unknown label →
  `None`); `pdf_timestamp_policy` (`2026-10-24T12:30:00+02:00` → 10:30:00 UTC; invalid → `Error::Config`; `meta.date`
  `2026-10-24` → 00:00:00; `SOURCE_DATE_EPOCH` `1700000000` → 2023-11-14 22:13:20; else `now`).
- `tests/typst_render.rs` (math.md Fixtures, ±0.01 pt): `equation_inline_anchors` (`x^2 + 1/2` 31.317 × 13.242, baseline
  9.447, PNG 63×26; `a/b` 6.974 × 11.602, 7.73; `x` 6.292 × 4.983, 4.862); `equation_display_anchor` (`integral_0^1 x^2
  dif x = 1/3` 65.719 × 29.159, 17.334); `equation_latex_via_mitex` (`\frac{a}{b}` `Ok`, `\foo{x}` → `Error::Compile`);
  `renditions_deterministic_self_contained` (byte-equal twice; SVG without `<text` and `font`); `image_renditions`
  (SVG at `Some(100.0)` → width 100, `%PDF`, `png_size().0 == 200`; a 1×1 GIF; a PNG; `image/bmp` → `Error::Other`);
  `render_restores_main`; `equation_mathml_forms` (display `frac(a, b)` → `<math display="block"><mfrac><mi>𝑎</mi><mi>𝑏</mi></mfrac></math>`,
  inline starts `<math>`); `equation_mathml_warning_is_err` (`overline(x)` → `Err` containing `was ignored during MathML
  export`); `equation_css_matches_typst` (equals `EQUATION_CSS`; snapshot `tests/snapshots/typst_equation_css.snap`).
- `grep -rn "evict" src/typst/` prints nothing. `cargo fmt --check`, `cargo clippy --all-targets --all-features --
  -D warnings`, `cargo test`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
`@writer/doc` files and `package::{eject, install_dir}` (T31); the emitter and escaping (T30); the frontend (T50);
`MarkdownIncludes` (T16 stub, T53 body); `WorldSlot`, `load_typst_main`, `--to pdf`, the fixture oracle test (T16, T32);
page SVG export and hashes (T24); eviction (T20/T23); the mitex pre-flight (T33); Cargo.toml and module lists.

## Hints (verified)
- Crates (vendored-assets.md §7.3): typst, typst-layout (`PagedDocument`, `Page`), typst-pdf, typst-svg, typst-render,
  typst-html, typst-kit `=0.15.1`; ureq `3.5.0`; time `0.3.55`; `typst::comemo`/`typst::ecow`. World skeleton, Library
  builder, ids, `Bytes`, exports: R1 §5.2, §9. Fonts: `FontStore::{new, extend, book, font}`, `typst_kit::fonts::
  {embedded(), scan(&Path), system()}` (typst-kit src/fonts.rs:31–163). Baseline/MathML: math.md §11 (its `ok_or_else`
  supersedes R4 §9's `unwrap_or(h_pt)`; R1/R4's `evict` calls are superseded). Trace: typst-dialect.md §11.
- `FileId` derives only `Copy, Clone, Eq, PartialEq, Hash` (typst-syntax 0.15.1 path.rs:97). `Source::replace` returns
  without editing for unchanged text (source.rs:85–90). typst-kit `FileStore`/`FileSlot` (files.rs:36–240) models
  stale-slot reuse but serves `source()` and `file()` from one buffer, which `.md`/`.ipynb` ids cannot use.
- typst-kit packages.rs: `SystemPackages::from_parts(Option<FsPackages>, Option<FsPackages>, UniversePackages)`,
  `obtain(&spec) -> PackageResult<FsRoot>` (data, cache, download only for `preview`); `FsPackages::{new,
  system_data(), system_cache(), obtain(&spec) -> Option<FsRoot>}`; `FsRoot::load(&VirtualPath) -> FileResult<Bytes>`.
  A downloader answering `ErrorKind::NotFound` yields `PackageError::NotFound(spec)`. Messages: `package not found
  (searched for @preview/x:1.0.0)`; `package found, but version 9.9.9 does not exist (latest is 0.1.0)`.
- `pub trait Downloader: Send + Sync + 'static { fn stream(&self, key: &dyn Any, url: &str) -> io::Result<(Option<usize>,
  Box<dyn Read>)>; }` (downloader.rs:34). ureq 3.5.0: `ureq::Agent::config_builder().user_agent(ua).build().into()`;
  `agent.get(url).call()` → `Result<http::Response<ureq::Body>, ureq::Error>`; `ureq::Error::StatusCode(404)` →
  `ErrorKind::NotFound`, others `io::Error::other(e)`; `body.content_length()`, `body.into_reader()`.
- time 0.3.55: `OffsetDateTime::parse(v, &Rfc3339)?.to_offset(UtcOffset::UTC)`, `OffsetDateTime::from_unix_timestamp(i64)`,
  `OffsetDateTime::from(SystemTime)`, `u8::from(dt.month())`. typst: `Datetime::from_ymd_hms(i32, u8, u8, u8, u8, u8) ->
  Option<Datetime>`, `Datetime::{year, month, day}() -> Option<_>`.
- `SourceDiagnostic { severity, span: DiagSpan, message, trace: EcoVec<Spanned<Tracepoint>>, hints }`; line =
  `world.range(span)` then `source.lines().byte_to_line(start)? + 1` (`use typst::WorldExt`). Warnings: `html export is
  under active development and incomplete` (typst src/lib.rs:252); `{name} was ignored during MathML export`
  (typst-html mathml.rs:1344; overline, underline, cancel, skewed fraction).
- typst strings keep unknown escapes (`"\frac"` stays, typst-syntax ast.rs:1436). 1×1 GIF (base64):
  `R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw==`. insta 1.49.0: `let mut s =
  insta::Settings::clone_current(); s.set_prepend_module_to_snapshot(false); s.bind(|| insta::assert_snapshot!(
  "typst_equation_css", css));` writes `tests/snapshots/typst_equation_css.snap`.

## Notes for the coordinator
- typst-dialect.md's Open point offers `Mutex<BTreeMap<FileId, Slot>>`; `FileId` is not `Ord`, so T03 uses a `HashMap`.
  `SourceMap` gains `is_empty()` beyond §5.1 (clippy `len_without_is_empty` under `-D warnings`).
- §8.2: unmapped lines "report the input path", but a `MainFile::Virtual` World never learns the Markdown input: T03
  leaves `Diagnostic.path = None` there (`into_result` uses `/__writer-main.typ`); T32, T24 and T73 should set
  `path = Some(input)` on `Compiled.diagnostics` before `into_result()`, or `MainFile::Virtual` gains a path.
- `Config` has no origin for config.md's "relative to `writer.toml`": T03 joins relative `[pdf] font_paths`/`[typst]
  package_paths` to `root`; T01/T16 should make them absolute at load. PLAN §5's T03 row still says "PDF/SVG pages"
  (live-reload.md dropped `compile_pages`). `@writer/doc` (T31) lands later: `writer_doc_embedded` skips until then.
