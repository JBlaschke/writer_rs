# T04 — DOCX package core: ZIP, parts, styles, numbering, builders, reference merge, validator

| | |
|---|---|
| Milestone | M0 |
| Size | L |
| Depends on | T01 |
| Parallel-safe with | T03, T10, T11, T13, T14, T17, T51 (wave 2) |
| Files you own | `src/docx/mod.rs`, `src/docx/package.rs`, `src/docx/styles.rs`, `src/docx/numbering.rs`, `src/docx/reference.rs`, `src/docx/validate.rs` (replace stubs); `assets/docx/styles.xml`, `assets/docx/numbering.xml`, `assets/docx/settings.xml` (new); `tests/fixtures/docx/reference.docx` (new; T43 regenerates it later); `tests/common/docx.rs` (new, read-back helper); `tests/docx_reference.rs`, `tests/docx_package.rs` (new); additive: the §1.1 stubs in `src/docx/{images,fields,tables,toc}.rs` (T41) and `src/docx/math.rs` (T42), and the `Emitter::document` stub in `src/docx/body.rs` (T40; see Notes) |

## Goal
Everything of a `.docx` except the body mapping: `docx::write` builds a deterministic, deflated package (parts, rels, content types, core/app properties, timestamp policy), the
name-keyed `StyleTable` over writer's own `styles.xml`, list numbering, `settings.xml`, pandoc-style reference documents, the ordered property builders, the structural validator V01–V20
and the docx-rs read-back helper. T40–T43 fill the body through `impl Emitter` blocks; until then your stubs emit plain text, so every package already validates.

## Read first
- `docs/spec/docx-output.md` — **this is your spec**: §1 (1.1–1.5), §2 (2.1–2.4; §2.4's `w:rPr` list only), §3, §4.11 (the `field` builder), §5, §6.1–6.4 (§6.5 for `all_styles`),
  §7.1–7.2, §8, §9, "Fixtures" (T04 bullet), "Open points", "Deviations"; context: §4.1, §4.8, §4.16.
- `docs/spec/math.md` §5.1 last bullet (`m:mathPr` is yours); `docs/spec/typst-dialect.md` §8.4 (timestamp policy); `docs/spec/ir.md` §1 (types, `ir::plain_text`, `CellOutput::pick`),
  §3, §5 (`Error`, `Diagnostics`); `docs/spec/config.md` §2 (`[docx]`, `[pdf] paper`, `[pdf] timestamp`, `[document] lang`); `docs/spec/references.md` §4.1, §4.5 (`LangTag`).
- `docs/spec/vendored-assets.md` §4, §5.1 (`vendor::get_str`), §7.1 (ZIP decision), §7.3 (pins: zip, quick-xml, time, docx-rs dev).
- `docs/research/R3-docx.md` §2.2–2.6, §2.9, §2.16, §3.1, §5, §6, §7; `docs/research/R8-presenter-inventory.md` §3.8, §7.
- `presenter_rs/src/pptx.rs` l. 42–45, 920–1064, 1803–1829, 1860–1891 (helpers to lift), tests l. 2677–2679, 2688.

## API to implement
```rust
// src/docx/mod.rs — docx-output.md §1.1 verbatim (copy its doc comments too)
pub struct Picture { pub png: Vec<u8>, pub svg: Option<Vec<u8>>, pub width_pt: f64, pub height_pt: f64, pub baseline_pt: f64 }
pub trait Renderer {
    fn equation(&mut self, source: &str, dialect: MathDialect, display: bool) -> Result<Picture>;
    fn image(&mut self, bytes: &[u8], mime: &str, width_pt: Option<f64>) -> Result<Picture>;
    fn typst_mathml(&mut self, source: &str, display: bool) -> Result<String>; }
pub struct DocxOptions<'a> { pub config: &'a Config, pub reference: Option<&'a Path>, pub source_date_epoch: Option<String>, pub all_styles: bool }
pub struct DocxOutput { pub bytes: Vec<u8>, pub diagnostics: Diagnostics }
pub fn write(doc: &Document, assets: &AssetRegistry, renderer: &mut dyn Renderer, opts: &DocxOptions) -> Result<DocxOutput>;
pub struct Emitter<'a> { pub doc: &'a Document, pub config: &'a Config, pub assets: &'a AssetRegistry,
    pub renderer: &'a mut dyn Renderer, pub styles: StyleTable, pub numbering: Numbering, pub layout: PageLayout,
    pub package: Package, pub ids: Ids, pub diagnostics: Diagnostics, pub containers: Vec<Container>, pub part: Part,
    pub lang: LangTag, pub body: String, pub notes: String }
pub struct PageLayout { pub sect_pr: String, pub text_width_twips: u32, pub text_height_twips: u32 }
pub struct Ids { pub bookmark: u32, pub doc_pr: u32, pub footnote: u32 }   // start at 1, or above the copied parts' ids (§6.2); +1 on use
pub enum Part { Document, Footnotes }    pub enum Container { Style(String), TightList, TableCell, CellOutput, Footnote }
pub struct Package { /* BTreeMap<String, (Vec<u8>, String)>, doc_rels: Rels, note_rels: Rels, media: Media */ } // in package.rs, `pub use`d here
impl Emitter<'_> {   // §1.1 cross-file stubs, each written in its owner's file
    pub(crate) fn image_run(&mut self, url: &str, alt: &Inlines, title: Option<&str>, attrs: &Attrs); // images.rs, T41
    pub(crate) fn figure(&mut self, b: &Block);  pub(crate) fn table(&mut self, b: &Block, figure: Option<&Block>); // fields.rs, tables.rs, T41
    pub(crate) fn toc(&mut self, depth: u8);  pub(crate) fn bibliography(&mut self);                                // toc.rs, T41
    pub(crate) fn math_inline(&mut self, source: &str, dialect: MathDialect);  pub(crate) fn math_block(&mut self, b: &Block); } // math.rs, T42
// docx::xml — `pub mod xml { … }` inline in src/docx/mod.rs (§4.11, §5)
pub fn field(instr: &str, result_runs: &str) -> String;
pub const PPR: &[&str] /* RPR, TBLPR, TRPR, TCPR, SECTPR, STYLE, LVL, SETTINGS */;  pub const ORDERS: &[(&str, &[&str])];  // values verbatim from §5
pub struct Props { tag: &'static str, attrs: String, order: &'static [&'static str], children: Vec<(usize, usize, String)> }
impl Props { pub fn new(tag: &'static str, order: &'static [&'static str]) -> Props;
    pub fn set(&mut self, name: &'static str, xml: impl Into<String>) -> &mut Self;   // replaces a child of that name
    pub fn push(&mut self, name: &'static str, xml: impl Into<String>) -> &mut Self;  // repeatable children
    pub fn is_empty(&self) -> bool;  pub fn to_xml(&self) -> String; }               // "" when empty
pub struct ParaProps { pub style: Option<String>, pub keep_next: bool, pub num: Option<(u8, u32)>, pub border: Option<String>, pub shading: Option<String>,
    pub tabs: Vec<String>, pub spacing: Option<String>, pub ind: Option<String>, pub jc: Option<&'static str> }
pub struct RunProps { pub style: Option<String>, pub fonts: Option<String>, pub bold: bool, pub italic: bool, pub small_caps: bool, pub strike: bool,
    pub color: Option<String>, pub position: Option<i32>, pub underline: bool, pub shading: Option<String>, pub vert_align: Option<&'static str> }
// likewise TableProps / RowProps / CellProps (TBLPR / TRPR / TCPR): fields in step 3
pub fn core_timestamp(pdf_timestamp: &str, meta_date: Option<&str>, source_date_epoch: Option<&str>) -> Result<Option<String>>; // src/docx/package.rs — §1.4 (`pub use`d)
pub enum StyleKind { Paragraph, Character, Table }  pub struct StyleTable { /* … */ }  pub struct Numbering { /* … */ }  // styles.rs §2.1, numbering.rs §3
impl StyleTable { pub fn builtin() -> StyleTable;  pub fn from_reference(styles_xml: &str) -> Result<StyleTable>;
    pub fn id_for(&self, name: &str, kind: StyleKind) -> Option<&str>;  pub fn ensure(&mut self, name: &str, kind: StyleKind) -> String;
    pub fn custom(&mut self, name: &str, kind: StyleKind, based_on: Option<&str>) -> String;
    pub fn set_lang(&mut self, lang: &str);  pub fn to_xml(&self) -> String; }
impl Numbering { pub fn builtin() -> Numbering; pub fn from_reference(numbering_xml: Option<&str>) -> Result<Numbering>;
    pub fn bullet(&mut self) -> u32;  pub fn ordered(&mut self, start: u64) -> u32;  pub fn to_xml(&self) -> String; }
pub struct Violation { pub rule: &'static str, pub part: String, pub message: String }    pub struct ValidateOptions { pub builtin_styles: bool }
pub fn validate(docx: &[u8], opts: &ValidateOptions) -> Vec<Violation>;                          // src/docx/validate.rs — §7.1
pub fn read_back(bytes: &[u8]) -> ReadBackCounts;  // tests/common/docx.rs §7.2; ReadBackCounts { pub paragraphs, pub styles, pub abstract_nums, pub images: usize }
// T04's additions (not in the spec, see Notes): Emitter::document (body.rs stub), Package::{hyperlink_rel, image_rel},
// styles::VOCABULARY: &[(&str, StyleKind)] (the 101 rows of §2.3), StyleTable::take_diagnostics (pub(crate)), Props::with_attrs.
```

## Steps
1. **`docx::xml` helpers** (`src/docx/mod.rs`): lift from `presenter_rs/src/pptx.rs` `escape`, `clean`, `is_xml_char`, `single_line` (l. 1860–1891), `XML_HEAD`, `NS_R`, `NS_RELS`, the
   `REL_*`/`CT_*` of officeDocument, core, app, image, hyperlink and rels (l. 953–994), `EMU_PER_INCH`, `EMU_PER_POINT` (l. 42–45), all `pub`; add the namespaces `w`, `m`, `r`, `o`, `v`,
   `w10`, `a`, `pic`, `wp` (R3 §2.6), `sl`, and the rel and content types of §1.2/§6.2. Do **not** lift `Zip`, `crc32` or the test-side `entries()` (vendored-assets.md §7.1). `field` is
   §4.11's string (instruction through `escape`; empty `result_runs` keeps the `separate` run). Lift l. 2677–2679 as a test.
2. **Order tables and `Props`** (`docx::xml`): the ten §5 constants verbatim; `Props` keeps `(index in order, insertion seq, xml)` and sorts on output; an unknown name hits
   `debug_assert!`, logs `tracing::error!` and is dropped; add `with_attrs(self, &str)` (the `attrs` field has no setter).
3. **Typed builders** (`docx::xml`, `#[derive(Debug, Clone, Default)]`, `to_xml(&self) -> String` via `Props`): an `Option<String>` field named after an element holds that whole element
   (`border` = `<w:pBdr>…</w:pBdr>`, `shading` = `<w:shd …/>`, `spacing`, `ind`, `fonts`); `style` is the id; `tabs` the `<w:tab …/>` items (wrapped in `<w:tabs>`); `jc`, `vert_align`,
   `color` the `w:val`; `bold` → `<w:b/><w:bCs/>`, `italic` → `<w:i/><w:iCs/>`, `underline` → `<w:u w:val="single"/>`. For §4.12/§4.15:
   `TableProps { style, width, borders, layout_fixed: bool, look, caption }`, `RowProps { cant_split: bool, header: bool }`,
   `CellProps { width, grid_span: Option<u32>, v_merge: Option<&'static str> /* "restart" | "continue" */, shading, v_align }`.
4. **Types** (`src/docx/mod.rs`): the §1.1 items (`Part`: `Debug, Clone, Copy, PartialEq, Eq`), private helpers for the current part's buffer (`body`/`notes` by `self.part`) and a
   plain-text paragraph.
5. **Stubs** (additive, owners' files, PLAN §6.5): each §1.1 function writes the plain text (`ir::plain_text`; figure → caption, table → cell texts row by row, math → source,
   toc/bibliography → nothing) as a Body Text paragraph or a run and pushes `unsupported-construct` "… in DOCX arrives in T41" (or T42) with the block's span. In `src/docx/body.rs`,
   `pub(crate) fn document(&mut self)` writes the title (Title), then per top-level block `Heading` → `heading N` (number, space, text), `Paragraph` → Body Text, `Figure`, `Table`,
   `Toc`, `Bibliography`, `Math` → the stubs, anything else → `unsupported-construct` "… arrives in T40".
6. **`assets/docx/styles.xml`**: the 101 styles of §2.3 (31 token styles with §2.4's rPr) and §2.2's `w:docDefaults` (`w:lang w:val="en"`), one `w:style` per line, children in
   `STYLE`/`PPR`/`RPR`/`TBLPR` order, explicit fonts and hex colours only (V17); font H = `<w:rFonts w:ascii="Calibri Light" w:hAnsi="Calibri Light" w:cs="Calibri Light"/>`.
7. **`StyleTable`** (`src/docx/styles.rs`): parse a styles part with quick-xml into root start tag, raw child slices and per `w:style` (type, id, name, basedOn, next, link, default);
   writer's definitions parsed once into a `LazyLock` (open point). `builtin()` holds the asset's `w:docDefaults` and three `w:default="1"` styles and appends any other definition on
   first `ensure`; `from_reference` keeps the reference verbatim and appends what its basedOn/next/link name but lack. Implement §2.1/§6.3 exactly: case-insensitive names; the `ensure`
   closure over basedOn/next/link (reserve the id before recursing: a style and its `* Char` link each other); ids rewritten to the table's; `custom` ids and default bases; the
   kind-mismatch rename + warning (kept until `write` calls `take_diagnostics`); no second `w:default="1"` per type; a taken §2.3 id gets `1`, `2`, …; `set_lang` per §6.3.
8. **Numbering** (`assets/docx/numbering.xml`, `src/docx/numbering.rs`): the asset is a complete part (root with `xmlns:w`, §3's two abstractNums of nine levels, no `w:num`) that
   `Numbering` slices; numIds from 1001 in call order; `ordered(start)` overrides L = 0…8; the reference filter and output order of §3.
9. **Settings** (`assets/docx/settings.xml`; merge in `src/docx/reference.rs`): §6.4's children (no `w:updateFields`), root namespaces `w`, `m`, `r`, `o`, `v`, `w10`, `sl`. Merge: walk
   `SETTINGS`; copyable names (all but `w:footnotePr`, `w:endnotePr`, `m:mathPr`, `sl:schemaLibrary`) take every reference occurrence, else writer's; `<w:updateFields w:val="true"/>` iff
   `config.docx.update_fields`; names outside `SETTINGS` (`w14:docId`) dropped; root = union of `xmlns:*` sorted by prefix, writer's URI wins, no `mc:Ignorable`.
10. **Reference reading** (`src/docx/reference.rs`): §6.1's checks (required parts, ZIP, well-formed XML, root prefix `w` bound to the WordprocessingML URI and `r` to the relationships
    URI) → `Error::Resource { what: "reference document", path, message }`; read `word/numbering.xml` and `word/settings.xml` by path when present; copy §6.2's parts transitively (with
    their `_rels/*.rels`, targets relative to the part's directory) keeping the reference's content types and rel Ids; `Ids` start above the largest `wp:docPr/@id` and
    `w:bookmarkStart/@w:id` found; take only the body-level `w:sectPr` slice (§9 snippet); `PageLayout` per §6.3; the built-in sectPr per `config.pdf.paper` with `word/footer{k}.xml` (k
    smallest free) holding `field("PAGE", …)` with result `1`, styled `ensure("footer")`.
11. **Timestamp** (`package.rs`): §1.4. Step 1 parses with `time` like `pdf_timestamp` (Hints); error: `Error::Config(format!("[pdf] timestamp {v:?} is not an RFC 3339 date-time: {e}"))`
    (Notes); steps 2–3 use Hinnant's `civil_from_days`; an epoch past `i64` or year 9999 does not match.
12. **Package** (`package.rs`): `Rels` (kept rels first, then writer's `rId<k>` in §1.2's order: styles, numbering, settings, footnotes and footer (when present), then images and
    hyperlinks in first-use order, one per distinct target and part; footnotes rels from `rId1`); `Media` (lift l. 920–949, keyed by the caller's dedup key, `w{n}.{png|jpeg|svg}`);
    `hyperlink_rel(part, url) -> String`; `image_rel(part, key, ext, bytes: Vec<u8>) -> (String, String)` (rel id, media file name). Lift `content_types`, `package_rels`,
    `core_properties` (l. 1004–1050) and `slide_rels`' loop (l. 1803–1829) adapted to §1.2–1.3: no newline after `XML_HEAD`, Defaults sorted by Extension, then Overrides by PartName,
    `core.xml` children in §1.3's order (keep `single_line`), `app.xml` = §1.3's string.
13. **ZIP** (`package.rs`): §1.5's options and entry order, PNG/JPEG media `Stored`, no directories, `.map_err(std::io::Error::from)`.
14. **`write`** (`mod.rs`): `core_timestamp` (errors first) → reference → `StyleTable` + `set_lang` (§1.3's effective lang) → `Numbering` → `Package` (decide "has footnotes" before
    emission by walking all blocks and inlines, `meta.abstract_`, `Cite.resolved.inlines` and every cell candidate) → `PageLayout` → `Emitter` (`lang` = `LangTag::effective(..)`, `None`
    → `en`) → `document()` → with `all_styles` `ensure` every `VOCABULARY` row → footer → styles serialised **after** the last `ensure` → `document.xml` (§1.2 namespaces,
    `<w:body>{body}{sect_pr}</w:body>`), `footnotes.xml` when `notes` is non-empty (root, §4.8's -1/0 separators, then `notes`) → ZIP; diagnostics += `styles.take_diagnostics()`.
15. **Validator** (`src/docx/validate.rs`; tests: a hand-written `VALID` package zipped with §1.5's options, each rule test mutating one part with `str::replace`): one pass per XML part
    does V02 (element stack, `with_checks(true)` + `normalized_value`, `GeneralRef` only `amp`/`lt`/`gt`/`quot`/`apos` or an XML char ref, `is_xml_char` on text and values, prefixes
    resolved against the in-scope `xmlns:*`, `xml` predeclared) and builds a small owned tree; V01 and V03–V20 run over the trees. V06 checks the children of every element the §5 tables
    and `ORDERS` name, in document, footnotes, styles, numbering and settings (`w:headerReference`/`w:footerReference` share a rank; children outside `w`/`m`/`wp`/`a`/`pic` are ignored).
16. **Read-back helper** (`tests/common/docx.rs`, `#![allow(dead_code)]`): asserts the docx-rs read is `Ok`, then counts; `ReadBackCounts` derives
    `Debug, Clone, Copy, PartialEq, Eq, serde::Serialize`; tests use `#[path = "common/docx.rs"] mod common_docx;`.
17. **Reference fixture** (`tests/docx_reference.rs`): the `#[ignore]`d `regenerate_reference_fixture` zips hand-written XML (`zip` crate, §1.5 options) per the spec's Fixtures bullet
    into `tests/fixtures/docx/reference.docx`; A5 `pgMar` left/right 1134 (text width 8391 − 2268 = 6123), `Table` basedOn the missing `TableNormal`, `mc:Ignorable="w14"` in settings.
18. **Integration tests** (`tests/docx_package.rs`): a resolved `Document` as inline JSON (title, authors, a numbered heading with id, paragraphs with emphasis and a footnote, a `Toc`, a
    captioned `Figure { kind: Other }`, a bare `Table`, a `Math` block, no images) and a fake `Renderer` returning `Err`; assert only properties (still true after T40–T42).
19. Run the four checks below (no compiler-backed tests here; the dev profile has `[profile.dev.package."*"] opt-level = 2`).

## Acceptance criteria
- `src/docx/mod.rs`: `escape_and_clean_match_presenter`; `field_has_the_complex_form` (`field("PAGE", r)` = §4.11's string); `props_sort_by_order` (out-of-order `set`, `set` replaces,
  `push` repeats, empty → `""`); `unknown_child_panics_in_debug` (`#[cfg(debug_assertions)] #[should_panic]`); `order_tables_have_no_duplicates`; `builders_follow_child_order`.
- `src/docx/styles.rs`: `builtin_asset_matches_vocabulary` (101 styles; name, id, type, basedOn, next, link, `customStyle` as §2.3; `VOCABULARY` equal),
  `builtin_has_no_theme_attributes`, `ensure_appends_the_closure` (`First Paragraph` adds BodyText, BodyTextChar), `custom_ids` (`"Callout Danger"` → `CalloutDanger`, `""` → `Style`,
  taken → `CalloutDanger1`), `names_are_case_insensitive`, `kind_mismatch_renames_and_warns`, `reference_style_wins`, `missing_based_on_is_appended` (`TableNormal`), `no_second_default`,
  `localized_ids_are_rewritten` (`heading 1` = `berschrift1` → appended `Heading 1 Char` links it), `set_lang_edits_or_inserts`.
- `src/docx/numbering.rs`: `builtin_levels` (ind 720×(ilvl+1) hanging 360; U+F0B7 Symbol, `o` Courier New, U+F0A7 Wingdings, `%{ilvl+1}.`), `num_ids_from_1001`,
  `ordered_overrides_nine_levels`, `reference_filter_and_order`, `no_lists_no_num`.
- `src/docx/reference.rs`: `pandoc_default_reference_text_width` (no pgSz/pgMar, a4 → 9026), `builtin_sect_pr_per_paper` (a4, a5, us-letter, us-legal, `"b5"` → A4),
  `paragraph_level_sect_pr_is_ignored`, `settings_merge`, `reference_errors` (no `word/styles.xml`, not a ZIP, malformed XML, `w` bound elsewhere → `Error::Resource` with
  `what == "reference document"`), `ids_start_above_copied_parts`, `footer_name_avoids_copied_parts`.
- `src/docx/package.rs`: `core_timestamp_policy` (`("", None, Some("0"))` → `1970-01-01T00:00:00Z`; `("", None, None)` → `None`; `"2026-10-25T12:00:00.5+02:00"` → `2026-10-25T10:00:00Z`;
  `("", Some("2026-10-24"), Some("5"))` → `2026-10-24T00:00:00Z`; `("", Some("October 2026"), Some("86400"))` → `1970-01-02T00:00:00Z`; `("", None, Some("12a"))` → `None`; `"yesterday"`
  → `Error::Config`), `content_types_sorted`, `rel_ids_follow_kept_max` (kept `rId7`, `rId12` → writer's from `rId13`; one hyperlink rel per url and part), `media_dedup_and_names`,
  `zip_entries` (§1.5 order; Deflated `version_made_by() == (2, 0)`, a `Stored` PNG `(1, 0)`, `System::Unix`, mode `0o100644`, `last_modified()` = `Some(DateTime::default())`).
- `src/docx/validate.rs`: `valid_package_passes`; one failing input per rule: `v01_zip`, `v03_content_types`, `v04_package_rels`, `v05_rels`, `v06_child_order`, `v07_sect_pr`,
  `v08_bookmarks`, `v09_anchors`, `v10_fields`, `v11_doc_pr`, `v12_footnotes`, `v13_numbering`, `v14_styles`, `v15_tables`, `v16_space`, `v17_theme_attributes`, `v18_media`,
  `v19_settings_notes`, `v20_dcterms`; V02 per sub-check: `v02_unclosed`, `v02_two_roots`, `v02_unknown_entity`, `v02_duplicate_attribute`, `v02_unquoted_attribute`, `v02_control_char`,
  `v02_bad_char_ref`, `v02_unbound_prefix`, `v02_text_outside_root`.
- `tests/docx_package.rs`: `mini_document_validates_and_reads_back` (`builtin_styles: true` → `[]`; `read_back` Ok), `two_writes_are_identical` (with and without `reference.docx`),
  `zip_layout` (first entry `[Content_Types].xml`, no directories), `core_dates` (`Some("0")` → both dcterms `1970-01-01T00:00:00Z`; `None` → none; a bad `[pdf] timestamp` →
  `Err(Error::Config(_))`), `update_fields_follows_config`, `all_styles_writes_the_vocabulary` (every `VOCABULARY` name in `styles.xml`, still `[]`), `stub_diagnostics_name_their_owner`
  (every `unsupported-construct` message names T40, T41 or T42).
- `tests/docx_reference.rs`: `reference_fixture_is_up_to_date` (regenerated bytes == committed file); `reference_is_honoured` (heading 1 `FF0000`; the reference's `Callout`;
  `word/header1.xml` byte-identical, `rId100` kept; the A5 sectPr; no footer part; `w:updateFields w:val="true"`, the reference's `w:rsids` and `w:compat`, no `w:footnotePr`, no
  `w14:docId`, `xmlns:w14` kept, no `mc:Ignorable`; abstractNum/num 5 kept, the reference's 991/1000 dropped (writer's 991 once, no num 1000); theme, fontTable, webSettings copied;
  writer's `app.xml`); `reference_output_validates`.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.

## Out of scope
Body mapping §4.1–4.10 and syntect runs (T40); figures, tables, captions, TOC, bibliography, images, media sizing, `WorldRenderer`, `pipeline/docx.rs`, `build --to docx`, the
`core_timestamp`/`pdf_timestamp` equivalence test (T41); OMML and the numbered-equation frame (T42); `eject docx-reference`, the showcase, smoke tests, `docs/dev/docx-validation.md`
(T43); `check --target docx` (T71); `--strict` escalation (T16). Never edit `Cargo.toml`, `lib.rs` or a `mod` list.

## Hints (verified)
- zip 9.0.5 (§9; probe): `SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)`
  `.compression_level(Some(6)).last_modified_time(DateTime::default()).system(System::Unix).unix_permissions(0o644)`; Stored =
  `.compression_method(CompressionMethod::Stored).compression_level(None)`. Read back: `ZipArchive::new(Cursor::new(&bytes[..]))?`, `by_index(i)?` → `name()?` (`Cow<str>`),
  `compression()`, `version_made_by()` (byte decoded `/10, %10`: 0x14 → `(2, 0)`, 0x0A → `(1, 0)`; `read/zipfile.rs:129`), `system()`, `unix_mode()`, `last_modified()`, `is_dir()`;
  `impl From<ZipError> for io::Error` (`result.rs:94`).
- quick-xml 0.42.0 (probe): no error for `<a><b>`, `<a/><b/>`, `<a>&foo;</a>`, `<a>\u{1}</a>`, `<p:a/>`, `<a>&#1;</a>`. With `let mut at = e.attributes(); at.with_checks(true);` items
  are `Err` ("duplicated attribute", "attribute value must be enclosed in …"). `Event::GeneralRef(g)`: `&*g` is `"foo"` or `"#1"`; `g.resolve_char_ref()` → `Ok(None)` (named) /
  `Ok(Some('\u{1}'))`. `e.name().as_ref()` is `&str`; `Reader::buffer_position() -> u64`; `read_to_end(QName) -> Result<Span>`; sectPr slicing: §9's snippet.
- time 0.3.55 (`parsing`, `formatting`; probe): `OffsetDateTime::parse(v, &Rfc3339)?.to_offset(UtcOffset::UTC)`, then `year()`, `u8::from(month())`, `day()`, `hour()`, `minute()`,
  `second()`; accepts lowercase `t`/`z`, fractions, second 60 (→ `:59`); rejects `"2026-10-25"` ("the 'separator' component could not be parsed") and a missing offset.
- Hinnant's `civil_from_days(z: i64) -> (i64, u32, u32)`, `z` = days since 1970-01-01 (rustc probe: 0 → 1970-01-01, 951782400 s → 2000-02-29, 1761350400 s → 2025-10-25; inverse
  round-trips): `let z = z + 719_468; let era = z.div_euclid(146_097); let doe = z.rem_euclid(146_097);`
  `let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);`
  `let mp = (5 * doy + 2) / 153; let d = (doy - (153 * mp + 2) / 5 + 1) as u32; let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;` `(yoe + era * 400 + i64::from(m <= 2), m, d)`
- docx-rs 0.4.22 (dev; `default-features = false` drops `image`):
  `docx_rs::read_docx_with_options(bytes, docx_rs::ReadDocxOptions::default().with_image_previews(false)) -> Result<Docx, ReaderError>`; `docx.document.children`
  (`DocumentChild::Paragraph(_)`), `docx.styles.styles`, `docx.numberings.abstract_nums`, `docx.images: Vec<(String, String, Image, Png)>`.
- insta 1.49.0 has only `json`, `filters`, `redactions` here (no `yaml`, no `glob`). `crate::vendor::get_str("docx/styles.xml") -> Result<String>` (vendored-assets.md §5.1). presenter's
  `Diagnostics::warn(&mut self, code: &'static str, message: impl Into<String>) -> &mut Diagnostic` (`error.rs:731`) is what T01 lifts; confirm in `src/error.rs`.
- Units (§9): 1 in = 914 400 EMU = 1 440 twips; 1 twip = 635 EMU; 1 pt = 12 700 EMU = 20 twips; `w:sz` half-points; border `w:sz` 1/8 pt.

## Notes for the coordinator
- docx-output.md §1.1 names no function through which `write` reaches the body; T04 adds the stub `Emitter::document(&mut self)` to T40's `src/docx/body.rs`; T40 replaces its body.
- T04 assembles `document.xml` and `footnotes.xml` (root, §4.8's -1/0 separators) around `Emitter.body`/`notes`; T40 only appends `<w:footnote w:id=…>` elements to `notes`. T40/T41 use
  T04's `Package::{hyperlink_rel, image_rel}` and step 3's builder fields.
- `core_timestamp`'s error must equal T03's `pdf_timestamp` message for T41's "same error" test (both wave 2); wording in step 11.
- Local readings to confirm: `builtin()` writes only ensured styles plus the three defaults; a taken §2.3 id is uniquified like a custom one; "has footnotes" comes from a pre-walk;
  `tests/docx_package.rs` is new; vendored-assets.md §7.1/§7.3 supersede R3 §6's zip feature and R8 §7's `Zip` lift.
