# T14 — Asset emitter with versioned serve URLs

## Amendments
- 2026-10-26 (coordinator): html-output.md §6.1 records the cache choice and equal-stamp
  caveat, missing-file errors in serve, query/fragment handling, and that T23 must not
  append another version query. These are the behaviors specified in this brief.

| | |
|---|---|
| Milestone | M1 |
| Size | S |
| Depends on | T01 |
| Parallel-safe with | T03, T04, T10, T11, T13, T17, T51 (wave 2) |
| Files you own | `src/assets/mod.rs` (replace stub; HTML scanner lifted from `presenter_rs/src/markdown/render.rs`), `src/assets/emit.rs` (replace stub; lift from `presenter_rs/src/assets/emit.rs`), `tests/assets.rs` (new; lift from `presenter_rs/tests/assets.rs`) |

## Goal
The single place that turns `asset:<id>` placeholders into URLs: `data:` URIs (standalone), content-addressed
files (site) or `/__writer/doc-asset/<id>?v=<content_version>` (serve), plus `register_css_urls` for stylesheets.
New against presenter: serve URLs carry the content version of the asset's bytes (presenter appended an mtime+size
`?v=` afterwards, in `cli/serve.rs`), `content_version` lives here, and `src/assets/mod.rs` holds the HTML start-tag scanner. Consumers:
T12 (scanner for raw HTML), T15 (`Emitter` pass after rendering, `content_version`, `html_count_attr`,
`data_uri`), T16 (`OutputMode`, `EmittedFile`s), T21 (`SERVE_ASSET_PREFIX` route), T1A (equation images, math.md §4.4).

## Read first
- `docs/spec/html-output.md` §6.1 (this is your spec), §6.2 and §2.2 (serve-only values never reach build output),
  §9 row T14, "Open points" (T14) and "Deviations" bullet 1 (scanner and `content_version` move into your files)
- `docs/spec/ir.md` §3 (placeholder grammar, registry API from T01), §5 (`Error::MissingAsset`, `Error::Other`)
- `docs/spec/live-reload.md` §1 routes table (`/__writer/doc-asset/{id}` ignores `v`), §1.1 (content version), §1.2
  item 6 (URL form)
- `docs/spec/vendored-assets.md` §2 (KaTeX files), §5.1 (`vendor::{get, get_str, list}`), §7.3 (crate pins)
- `docs/research/R6-html-and-crate-versions.md` §A.4; `docs/research/R8-presenter-inventory.md` §3.4, §4.3,
  §10 step 9, §11 (`src/assets/emit.rs`, `src/markdown/render.rs` entries)
- presenter_rs (sibling checkout `../presenter_rs`): `src/assets/emit.rs` (1,701 LOC incl. tests),
  `src/markdown/render.rs` lines ≈271–313 and ≈405–560 (scanner) and tests `start_tags_follow_the_html_tokenizer`,
  `math_markup_is_a_start_tag_attribute` (≈2553–2628), `src/render/paths.rs` lines ≈170–181 (`content_version`)
  and ≈224–262 (`VERSIONS`, `disk_version`), `src/cli/serve.rs` lines ≈677–740 (`version_file_assets`: read only,
  this is what your serve URLs replace), `tests/assets.rs` (917 LOC)

## API to implement
```rust
// src/assets/emit.rs — html-output.md §6.1; "unchanged" signatures exactly as R6 §A.4 / R8 §11:
pub const SERVE_ASSET_PREFIX: &str = "/__writer/doc-asset/";
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputMode { Standalone, Site { assets_dir: PathBuf, url_prefix: String }, Serve }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedFile { pub path: PathBuf, pub bytes: Vec<u8> }
#[derive(Debug)]
pub struct Emitter<'a> { pub files: Vec<EmittedFile>, /* registry: &'a AssetRegistry, mode: &'a OutputMode */ }
impl<'a> Emitter<'a> {
    pub fn new(registry: &'a AssetRegistry, mode: &'a OutputMode) -> Self;
    pub fn with_files(registry: &'a AssetRegistry, mode: &'a OutputMode, files: Vec<EmittedFile>) -> Self;
    pub fn resolve_html(&mut self, html: &str) -> Result<String>;          // attribute values + <style>/<script> only
    pub fn resolve_css(&mut self, css: &str) -> Result<String>;
    pub fn resolve_css_in_assets_dir(&mut self, css: &str) -> Result<String>;
    pub fn url_for(&mut self, id: &str) -> Result<String>;                 // Serve: "/__writer/doc-asset/<id>?v=<n>"
}
pub fn data_uri(mime: &str, bytes: &[u8]) -> String;                        // base64 STANDARD, padded
pub fn hashed_file_name(hint: &str, id: &str) -> String;                    // "plot.png" + hash -> "plot-<hash>.png"
pub fn register_css_urls(css: &str, registry: &mut AssetRegistry,
                         resolve_rel: &mut dyn FnMut(&str) -> Option<AssetData>, lenient: bool) -> Result<String>;
pub fn content_version(bytes: &[u8]) -> u64;   // blake3 first 8 bytes, LE (moved here from presenter's render/paths.rs)
pub(crate) fn is_text_attribute(name: &str) -> bool;                       // alt, title, placeholder, label, value, abbr, aria-*

// src/assets/mod.rs — html-output.md §6.1 (scanner moved from presenter's markdown/render.rs, pub(crate))
pub mod emit; pub mod registry;                                            // T00's lines, keep them
pub(crate) fn html_start_tags(html: &str) -> Vec<(String, usize)>;
pub(crate) fn html_attr_values(html: &str) -> Vec<(String, std::ops::Range<usize>)>;
pub(crate) fn html_count_attr(html: &str, attr: &str) -> usize;
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HtmlScan { #[default] Data, Comment, RawText(&'static str) }
impl HtmlScan { pub(crate) fn start_tags<E>(&mut self, html: &str,
    on_tag: impl FnMut(&StartTag) -> std::result::Result<(), E>) -> std::result::Result<(), E>; }
pub(crate) struct StartTag { pub(crate) name: String, pub(crate) attrs: Vec<TagAttr>, pub(crate) end: usize }
pub(crate) struct TagAttr { pub(crate) name: String, pub(crate) value: Option<std::ops::Range<usize>> }
```

## Steps
1. **Check the ground (T00, T01 are merged).** `src/assets/mod.rs` and `emit.rs` are T00 stubs; `mod.rs` already has
   `pub mod emit; pub mod registry;` (keep them, add nothing to `lib.rs`). Open T01's `src/assets/registry.rs`:
   presenter's `content_hash`, `unknown_id` (pub(crate)), `len`, `is_empty`, `normalize_lexically` should be there;
   `read` returns `Result<Vec<u8>>` (ir.md §3) or `Result<Cow<'_, [u8]>>` (presenter). Never edit `registry.rs`: if an
   item is missing, write a private equivalent in your file (copy its body from presenter) and name it in the PR.
2. **Scanner → `src/assets/mod.rs`.** Replace the `//!` stub text with module docs (document assets: the registry
   T01 fills, the emitter, the scanner shared with T12). Copy from presenter `src/markdown/render.rs`:
   `RAW_TEXT_ELEMENTS`, `HtmlScan` + `start_tags`, `StartTag`, `TagAttr`, `raw_text_end`, `start_tag`, `attr_value`,
   `html_start_tags`, `html_attr_values`; make `HtmlScan`, `start_tags`, `StartTag`, `TagAttr` and their fields
   `pub(crate)` (API block; T12 carries one `HtmlScan` across raw HTML snippets). In doc comments replace
   `rewrite_html_assets`/"notebook frontend" with "the Markdown frontend (T12)". Do not lift `has_math_markup`.
3. **`html_count_attr`** in `mod.rs`: number of start tags (outside comments and raw-text elements) that have an
   attribute named `attr` (compare with `attr.to_ascii_lowercase()`; the scanner lowercases names), bare or valued;
   a tag counts once however often the attribute repeats. Build it on `HtmlScan::default().start_tags` like
   presenter's `has_math_markup`. It has no caller until T15, so mark it
   `#[allow(dead_code, reason = "html::document checks the morph root with it (T15)")]` or clippy fails.
4. **Lift `emit.rs`.** Copy presenter `src/assets/emit.rs` whole (tests included). Edits: import
   `crate::assets::{html_attr_values, html_start_tags}` instead of `crate::markdown::render::…`;
   `SERVE_ASSET_PREFIX = "/__writer/doc-asset/"`; `presenter`/`deck` → `writer`/`document` in docs and messages;
   spec references → `docs/spec/ir.md` §3, `docs/spec/html-output.md` §6.1, `docs/spec/live-reload.md` §1; "T42 inlines
   local imports" in `register_css_urls`' docs → "html::styles (T15) hoists absolute `@import`s and rejects relative
   ones first"; delete the "Known limitation: `data-background-video`" paragraph (reveal.js only). Keep every other
   line, including the `Referrer`, query, CSS tokenizer and percent-decoding logic. Do not add a fourth mode.
5. **`content_version`** in `emit.rs`: presenter `render/paths.rs`' function, made `pub`, docs citing live-reload.md
   §1.1 (decimal in URLs). T15 imports it from here (`crate::assets::emit::content_version`).
6. **Versioned serve URLs.** In `url_from`'s `Serve` arm return
   `format!("{SERVE_ASSET_PREFIX}{id}?v={version}")` (inline args: clippy's `uninlined_format_args`) where `version` is: `AssetData::Bytes(b)` → `content_version(b)`;
   `AssetData::File(path)` → a private cache keyed by path, `static VERSIONS: Mutex<BTreeMap<PathBuf, (u64,
   SystemTime, u64)>>` (len, mtime, version), adapted from presenter's `disk_version` (snippet under Hints): stamp
   first, reuse while `(len, mtime)` match, else `self.registry.read(id)?` (so a deleted file is
   `Error::MissingAsset { path, span: None }` in serve too) and remember. This is html-output.md's T14 open point,
   decided. In `resolve_placeholders` set `drop_query` for `Standalone | Serve`: the route ignores the query
   (live-reload.md §1), and a kept `?x=1` would give `…?v=7?x=1`; a fragment is kept. Update the module-doc table.
7. **Unit tests in `emit.rs`.** Change the two serve expectations: `standalone_drops_queries_but_keeps_fragments`
   → `url(/__writer/doc-asset/{id}?v={v}#iefix)`; `a_placeholder_inside_a_dropped_query_goes_with_it` →
   `<a href="/__writer/doc-asset/{a}?v={va}">y</a>` (`v = content_version(&bytes)`). Add the two tests named under
   Acceptance (constants measured, Hints).
8. **`tests/assets.rs`.** Copy presenter's file; `presenter::` → `writer::`. Replace `fixture("md/images/ok.png")`
   and `fixture("md")` with a `tempfile::TempDir` the test holds, holding `images/ok.png` (any PNG-magic bytes), so the
   tests do not depend on T01's image names. Serve expectations gain `?v=<content_version of the bytes>`;
   `deleted_file_is_missing_asset_at_emit_time` expects `MissingAsset` in all three modes. Replace
   `sample_deck_placeholders_resolve_in_every_mode` (presenter IR) with `figure_markup_placeholders_resolve_in_every_mode`.
   Do not lift the two end-to-end tests after the comment "End to end through `presenter build`" (they need T16's CLI).
   Add the new tests of the acceptance list.

## Acceptance criteria
- `cargo test --lib assets::tests`: `start_tags_follow_the_html_tokenizer` (lifted verbatim) and
  `html_count_attr_counts_start_tags_outside_text`: `"<main data-writer-document>\n<p>x</p>\n</main>\n"` → 1;
  `<main data-writer-document="">`, `<MAIN DATA-WRITER-DOCUMENT>` and `<main data-writer-document data-writer-document>`
  → 1 each; two such tags → 2; 0 for `<!-- <main data-writer-document> -->`,
  `<script>'<main data-writer-document>'</script>`, `<p title=" data-writer-document=x">t</p>` and
  `<code>&lt;main data-writer-document&gt;</code>`.
  `html_attr_values_skip_bare_attributes`: `html_attr_values("<p hidden id=a>") == [("id".into(), 13..14)]`.
- `cargo test --lib assets::emit::tests`: every lifted test passes (serve expectations as in step 7), plus
  `content_version_is_the_first_eight_blake3_bytes_little_endian` (`content_version(b"a") == 6_027_058_585_768_261_143`,
  `content_version(b"") == 12_007_152_915_317_330_863`, `content_version(b"aaaa") != content_version(b"bbbb")`) and
  `serve_file_versions_are_remembered_while_len_and_mtime_are_unchanged` (temp file `\x89PNG figure`, 11 bytes →
  `?v=5571118757713761389`; rewrite as `\x89PNG FIGURE` (same length) and restore the old mtime with
  `File::set_modified` → same URL, the documented caveat; rewrite as `\x89PNG edited!` (12 bytes) →
  `?v=6467587972938660898`).
- `cargo test --test assets` (`tests/assets.rs`, html-output.md §9 row T14): the lifted tests (renamed) pass, and
  - `resolve_html_uses_dev_server_routes_in_serve_mode`: each URL is `/__writer/doc-asset/<id>?v=<content_version(bytes)>`,
    `emitter.files` empty; `serve_urls_match_the_live_reload_url_form`: every `url_for` in serve matches
    `^/__writer/doc-asset/[0-9a-f]{12}\?v=[0-9]+$` (live-reload.md §1.2 item 6);
  - `serve_version_changes_with_the_bytes`: file asset with `\x89PNG figure` → `?v=5571118757713761389`; the file
    rewritten as `\x89PNG edited!` → same id, `?v=6467587972938660898`;
  - `build_modes_never_write_a_version`: standalone and site output of the three-asset document contain no `?v=`, no
    `asset:` and no `/__writer/` (html-output.md §2.2, §6.2);
  - `figure_markup_placeholders_resolve_in_every_mode`: html-output.md §1.6's figure (`<img src="asset:ID"
    alt="… asset:ID" style="width:70%">` with a registered file) → `src` resolved in all three modes, `alt` unchanged,
    one `EmittedFile` in site mode only;
  - `katex_css_inlines_woff2_fonts_and_leaves_fallbacks_untouched` via `writer::vendor::{get, get_str, list}`
    (20 `url(data:font/woff2;base64,` after standalone resolution; `.woff`/`.ttf` fallbacks untouched; strict → `MissingAsset`);
  - `data_uri_of_png_magic`: `data_uri("image/png", b"\x89PNG") == "data:image/png;base64,iVBORw=="`.
- `grep -rn -e __presenter -e deck-asset src/assets tests/assets.rs` prints nothing. No snapshot files.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- `src/assets/registry.rs` (T01). Which URLs are local, registering them, raw-HTML rewriting with your scanner (T12, T18).
- Theme `url()` rewriting, helper paths, `/__writer/asset/<path>?v=` URLs and calling `html_count_attr` (T15,
  html-output.md §4.2, §5); the `/__writer/doc-asset/{id}` route and byte ranges (T21); the serve session (T23).
- Writing `EmittedFile`s to disk, `--site`, the size budget of §6.3 (T16). DOCX/typst/LaTeX assets (T41, T32, T61):
  R8 §3.4's suggested fourth mode `Files{dir}` is not part of the spec.

## Hints (verified)
- Pins: vendored-assets.md §7.3. Crates you use: `base64` 0.23.1, `blake3` 1.8.7, `mime_guess` 2.0.5, `regex` 1.13.1,
  `tracing` 0.1.44; dev: `tempfile` 3.27.0. No compiler-backed tests here; the dev profile already optimises
  dependencies (`[profile.dev.package."*"] opt-level = 2`), so `cargo test --test assets` takes seconds once built.
- base64 0.23.1 (presenter `data_uri`; offline probe with the registry crates, 2026-10-25): `use base64::Engine as _;`
  `base64::engine::general_purpose::STANDARD.encode_string(bytes, &mut uri);` → `data:image/png;base64,iVBORw==`.
- blake3 1.8.7 (presenter `render/paths.rs`; every `content_version` number above was printed by that probe):
  ```rust
  pub fn content_version(bytes: &[u8]) -> u64 {
      let hash = blake3::hash(bytes);
      let mut first = [0u8; 8];
      first.copy_from_slice(&hash.as_bytes()[..8]);
      u64::from_le_bytes(first)
  }
  ```
  Site names keep presenter's `content_hash` (`blake3::hash(bytes).to_hex()[..12]`, R8 §11): the probe gives
  `content_hash(b"a") == "17762fddd969"`.
- Cache, adapted from presenter `render/paths.rs` `disk_version` (std only; `Mutex::new` and `BTreeMap::new` are const):
  ```rust
  static VERSIONS: Mutex<BTreeMap<PathBuf, (u64, SystemTime, u64)>> = Mutex::new(BTreeMap::new());
  let stamp = std::fs::metadata(path).ok().and_then(|m| Some((m.len(), m.modified().ok()?)));
  let versions = || VERSIONS.lock().unwrap_or_else(PoisonError::into_inner);
  // hit: stamp == Some((len, time)) of the entry → its version; miss: read, hash, insert with the stamp taken BEFORE reading
  ```
- `let bytes = registry.read(id)?; content_version(&bytes)` compiles for both return types (deref coercion of `&Vec<u8>`
  and `&Cow<[u8]>` to `&[u8]`). The lifted site arm's `bytes.into_owned()` exists only on `Cow`; with a `Vec`, drop it.
- `std::fs::File::set_modified(&self, SystemTime) -> io::Result<()>` (stable since 1.75): open with
  `File::options().write(true).open(p)`. `#[allow(…, reason = "…")]` is stable since 1.81.
- R8 §4.3: ids are blake3 of the absolute path (`register_file`) or of the bytes; site names hash the **bytes**.
  `mime_guess::from_path(p).first_or_octet_stream().essence_str()` gives `font/woff2` for the KaTeX fonts
  (presenter's `register_resolved`, lifted unchanged).

## Notes for the coordinator
- Serve behaviour deltas the spec implies but does not state: (a) serve URLs read the bytes (or the cached version),
  so a file deleted between `prepare` and `render` is `Error::MissingAsset` in serve, where presenter answered 404
  later; the next rebuild reports `missing-asset` as a warning. (b) Serve drops an author's `?query` after a
  placeholder, as standalone does, to keep the single `?v=` of live-reload.md §1.2 item 6; site keeps it.
- T23 must not lift presenter `src/cli/serve.rs`' `version_file_assets`/`versioned_urls`/`file_version` (R8 §10
  step 24 says "verbatim"): the emitter now writes `?v=`, and that pass would append a second one.
- presenter's end-to-end tests at the end of `tests/assets.rs` (`site_builds_do_not_depend_on_the_checkout_location`,
  `literal_placeholders_in_slide_text_build_and_stay_text`) need `writer build --site`; T16 may adopt them.
- ir.md §3 gives `read -> Result<Vec<u8>>`, R6 §A.4 and presenter `Result<Cow<'_, [u8]>>`; ir.md §3 does not list
  `content_hash`, `len`, `unknown_id`, which html-output.md §6.1 and the lifted tests use. Step 1 copes with either.
