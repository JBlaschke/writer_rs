# Spec: DOCX output

**Status:** contract. Implemented by T04 (§1 package and the §1.1 cross-file stubs, §2 styles, §3
numbering, §5 ordered builders, §6 reference merging, §7.1 validator, §7.2 read-back helper), T40
(§4.1–4.9 body, inlines, footnotes, bookmarks and internal links, code, callouts, cells), T41
(§4.10–4.14 figures, captions and fields, tables, TOC, bibliography, images; `src/pipeline/docx.rs`),
T42 (§4.15 math incl. the numbered-equation frame: `src/docx/math.rs`, and `src/math/omml.rs`
whose MathML→OMML mapping is math.md's), T43 (§6.5 eject, §7.3–7.4 smoke tests and
`docs/dev/docx-validation.md`, `src/docx/custom_style.rs`). Consumers: T16 (the `docx` arm of
`pipeline/mod.rs`), T71 (`check --target docx`), T72 (`eject docx-reference` wiring), T74 (guide).

Design rules: hand-written OOXML (no docx-rs at run time, R3 §0); styles looked up **by name** in
pandoc's vocabulary, so pandoc reference documents work; property elements written through ordered
builders (§5); byte-identical output for identical inputs (§1.5); every number and reference text
from the resolved IR (ir.md, references.md), with field instructions whose recomputation by Word agrees (§4.11).

## 1. Package — T04 (`src/docx/{mod,package}.rs`)

### 1.1 Entry points
```rust
// src/docx/mod.rs (T04). The inline module `docx::xml` (escape, ordered builders §5, `field` §4.11) lives in this file too.
pub struct Picture { pub png: Vec<u8>, pub svg: Option<Vec<u8>>, pub width_pt: f64, pub height_pt: f64, pub baseline_pt: f64 }
/// Implemented over `typst::render::{equation, image}` and `typst::html::equation_mathml` by
/// `pipeline::docx::WorldRenderer<'a>` (T41; `pub struct WorldRenderer<'a> { /* private */ }`,
/// `WorldRenderer::new(config: &'a Config, world: &'a mut WriterWorld) -> WorldRenderer<'a>`, like html-output.md §1.8's
/// `MathRenderer`): it borrows the session's World (see `build` below) and never creates one; tests use a fake.
pub trait Renderer {
    /// `source` as the typst emitter writes it: LaTeX → `latex_source::for_typst(src, &macros).0` (math.md §3.4).
    fn equation(&mut self, source: &str, dialect: MathDialect, display: bool) -> Result<Picture>;
    /// `width_pt: Some(w)` renders at that display width (PNG at 2 px/pt); `None` at natural size.
    fn image(&mut self, bytes: &[u8], mime: &str, width_pt: Option<f64>) -> Result<Picture>;
    fn typst_mathml(&mut self, source: &str, display: bool) -> Result<String>;   // typst::html::equation_mathml
}
/// `reference`: absolute (config.md §3 rules); `source_date_epoch`: the env value, read by the pipeline;
/// `all_styles`: every §2.3 style goes into styles.xml (true only in `reference_docx`, §6.5).
pub struct DocxOptions<'a> { pub config: &'a Config, pub reference: Option<&'a Path>, pub source_date_epoch: Option<String>, pub all_styles: bool }
pub struct DocxOutput { pub bytes: Vec<u8>, pub diagnostics: Diagnostics }
/// Never fails on content. Errors: bad reference, its ZIP errors included (`Error::Resource { what: "reference document", .. }`),
/// unknown `asset:` id, invalid `[pdf] timestamp` (§1.4), I/O; ZIP write errors → `Error::Io` via `.map_err(std::io::Error::from)`.
pub fn write(doc: &Document, assets: &AssetRegistry, renderer: &mut dyn Renderer, opts: &DocxOptions) -> Result<DocxOutput>;
/// Shared state; body.rs, inline.rs, footnotes.rs (T40), tables.rs, images.rs, fields.rs, toc.rs (T41), math.rs (T42) add `impl Emitter` blocks.
/// `lang` = `resolve::LangTag::effective(&doc.meta, config)`, `None` → `en`; `body`/`notes` = the XML being built for
/// `w:body` and `footnotes.xml` (`part` says which one receives output).
pub struct Emitter<'a> { pub doc: &'a Document, pub config: &'a Config, pub assets: &'a AssetRegistry,
    pub renderer: &'a mut dyn Renderer, pub styles: StyleTable, pub numbering: Numbering, pub layout: PageLayout,
    pub package: Package, pub ids: Ids, pub diagnostics: Diagnostics, pub containers: Vec<Container>, pub part: Part,
    pub lang: LangTag, pub body: String, pub notes: String }
pub struct PageLayout { pub sect_pr: String, pub text_width_twips: u32, pub text_height_twips: u32 }
pub struct Ids { pub bookmark: u32, pub doc_pr: u32, pub footnote: u32 }   // start at 1, or above the copied parts' ids (§6.2); +1 on use
pub enum Part { Document, Footnotes }                                        // which rels table links/images go to
/// §4.1's container stack. `Style` holds a resolved paragraph style id.
pub enum Container { Style(String), TightList, TableCell, CellOutput, Footnote }
/// Generated parts (path → bytes, content type), copied reference parts, `Rels` for document and footnotes, media.
pub struct Package { /* BTreeMap<String, (Vec<u8>, String)>, doc_rels: Rels, note_rels: Rels, media: Media */ }
// Cross-file entry points, fixed signatures. T04 writes each as a stub in its owner's file that emits the plain text
// (`ir::plain_text`) and pushes `unsupported-construct` naming the owner task; the owner replaces the body (PLAN §6.5).
impl Emitter<'_> {
    pub(crate) fn image_run(&mut self, url: &str, alt: &Inlines, title: Option<&str>, attrs: &Attrs); // images.rs, T41 (§4.16)
    pub(crate) fn figure(&mut self, b: &Block);                          // fields.rs, T41 (§4.10)
    pub(crate) fn table(&mut self, b: &Block, figure: Option<&Block>);   // tables.rs, T41; `figure` = the enclosing Figure{kind: Table}
    pub(crate) fn toc(&mut self, depth: u8);                             // toc.rs, T41 (§4.13)
    pub(crate) fn bibliography(&mut self);                               // toc.rs, T41 (§4.14)
    pub(crate) fn math_inline(&mut self, source: &str, dialect: MathDialect);  // math.rs, T42 (§4.15)
    pub(crate) fn math_block(&mut self, b: &Block);                      // math.rs, T42 (§4.15, numbered frame included)
}
```
> **Coordinator clarification (2026-10-27):** `Emitter` additionally holds
> crate-private content state for the bookmark/note prepass and paragraph/list
> context. Construct it through `write`; cross-file renderers share that state.
> Deferred note bodies retain borrowed IR, preserving first logical bookmark
> ownership across parts. T04's whole-IR asset validation remains independent
> of the emitted-content prepass.

`pipeline/docx.rs` (T41): `pub fn build(doc: &Document, assets: &AssetRegistry, config: &Config, out: &Path, world: &mut WriterWorld) -> Result<Diagnostics>`
writes `<stem>.docx` (cli.md) through `WorldRenderer::new(config, world)`. `world` is the build session's `WriterWorld`
(one per session, typst-dialect.md §6.1): T16's `build_targets` gets it from `WorldSlot::get` and `reset()`s it after
its prepare, and every target of a `build --to pdf,docx,…` shares it (live-reload.md §5.2–§5.3). `pipeline/docx.rs`
never creates a World. `check --target docx` (T71) passes its check session's World the same way (one `WorldSlot::get`
and `reset()` after its prepare, shared by every target it checks), calls `write` with a `WorldRenderer` over it and
discards the bytes.

### 1.2 Parts (generated unless marked copied, §6)
| part | when | content type (Override unless "Default") |
|---|---|---|
| `[Content_Types].xml` | always | — |
| `_rels/.rels` | always | Default `rels` = `application/vnd.openxmlformats-package.relationships+xml` |
| `docProps/core.xml` | always | `application/vnd.openxmlformats-package.core-properties+xml` |
| `docProps/app.xml` | always | `application/vnd.openxmlformats-officedocument.extended-properties+xml` |
| `word/document.xml` | always | `application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml` |
| `word/_rels/document.xml.rels` | always | Default `rels` |
| `word/styles.xml` / `numbering.xml` / `settings.xml` | always | `…wordprocessingml.styles+xml` / `…numbering+xml` / `…settings+xml` |
| `word/footnotes.xml` (+ `word/_rels/footnotes.xml.rels` when it has rels) | ≥ 1 footnote | `…wordprocessingml.footnotes+xml` |
| `word/footer{k}.xml` (k = 1 unless a copied part has that name, §6.2) | the built-in sectPr is used (§6.3) | `…wordprocessingml.footer+xml` |
| `word/media/w<n>.<png\|jpeg\|svg>` | per image (§4.16) | Default by extension: `image/png`, `image/jpeg`, `image/svg+xml` |
| theme, fontTable, webSettings, headers/footers, their rels, fonts, media | copied from a reference (§6.2) | the reference's own Override/Default for that part |

No `comments.xml`, no `docProps/custom.xml`. Defaults (`rels`, `xml` = `application/xml`, one per
media extension) sorted by `Extension`, then Overrides sorted by `PartName`. `_rels/.rels` is exactly
`rId1` officeDocument → `word/document.xml`, `rId2` core-properties → `docProps/core.xml`, `rId3`
extended-properties → `docProps/app.xml` (type URIs as R3 §2.3). `document.xml.rels`: rels copied
from a reference keep their `Id`; writer's get `rId<k>`, k from 1 + the largest numeric suffix of
kept `rId\d+` ids, in order styles, numbering, settings, footnotes, the footer (when present), then
images and external hyperlinks in first-use order (one rel per distinct target and part;
hyperlinks `TargetMode="External"`); `footnotes.xml.rels` the same from `rId1`. Every XML part is
`<?xml version="1.0" encoding="UTF-8" standalone="yes"?>` + the root (no newline); `document.xml`
and `footnotes.xml` roots declare pandoc's namespaces `w`, `m`, `r`, `o`, `v`, `w10`, `a`, `pic`,
`wp` (URIs as R3 §2.6). Text goes through the lifted `escape`/`clean` (`& < > "`, non-XML-1.0
characters dropped); every `w:t`, `w:instrText` and `m:t` has `xml:space="preserve"`.

### 1.3 `docProps/core.xml` and `app.xml`
`<cp:coreProperties>` with the five namespace declarations of R3 §2.5 and, in this order, only
non-empty children: `dc:title` (meta.title), `dc:creator` (author names joined `"; "`),
`cp:keywords` (keywords joined `", "`), `dc:description` (meta.description), `dc:language`
(effective lang: `meta.lang`, else `config.document.lang`), `dcterms:created` and
`dcterms:modified` (`xsi:type="dcterms:W3CDTF"`, same value, §1.4). `app.xml` is always writer's:
`<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>writer</Application></Properties>`.

### 1.4 Timestamp (the PDF policy, config.md `[pdf] timestamp`, with "omit" instead of "now")
`pub fn core_timestamp(pdf_timestamp: &str, meta_date: Option<&str>, source_date_epoch: Option<&str>) -> Result<Option<String>>`
follows typst-dialect.md §8.4 exactly and returns `YYYY-MM-DDTHH:MM:SSZ`: (1) a non-empty `[pdf]
timestamp` must be RFC 3339 (`YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`, converted to UTC, fraction
dropped), else `Error::Config` with the PDF's message; else (2) `meta.date` matching
`^\d{4}-\d{2}-\d{2}$` → `T00:00:00Z` (any other date is ignored); else (3) `SOURCE_DATE_EPOCH`
matching `^\d+$` (seconds since 1970 UTC); else `Ok(None)` and both dcterms elements are omitted
(only this last step differs from the PDF's "now"). Civil-date conversion is Howard Hinnant's
`days_from_civil`/`civil_from_days` (no date crate). T41 (both exist by its wave) adds a test that
runs the same inputs through `core_timestamp` and `pdf_timestamp`: same instant or same error.

> **Contract change (coordinator, 2026-10-26):** DOCX timestamps require a
> four-digit UTC year (0000–9999). A valid explicit RFC 3339 timestamp that
> converts outside that range returns `Error::Config("[pdf] timestamp is outside
> the DOCX date range")`; for example, `0000-01-01T00:00:00+01:00` converts to a
> negative UTC year. This representability check is specific to DOCX. Explicit
> parse and UTC conversion failures otherwise keep T03's existing messages.

### 1.5 ZIP (zip `=9.0.5`, `default-features = false, features = ["deflate"]`: vendored-assets.md §7.1; level 6 goes through flate2/zlib-rs)
Entry options: `last_modified_time(DateTime::default())` (1980-01-01 00:00:00),
`system(System::Unix)`, `unix_permissions(0o644)`; PNG/JPEG media `Stored` with
`compression_level(None)` (a level with `Stored` is the error "Unsupported compression level",
verified), everything else `Deflated` level 6. Entry order: `[Content_Types].xml`, `_rels/.rels`,
`docProps/core.xml`, `docProps/app.xml`, `word/document.xml`, `word/_rels/document.xml.rels`,
`word/styles.xml`, `word/numbering.xml`, `word/settings.xml`, `word/footnotes.xml`,
`word/_rels/footnotes.xml.rels`, `word/footer{k}.xml`, then copied reference parts sorted by path,
then `word/media/w<n>.*` by n. No directory entries. Determinism tests (T04, T41): `write` twice →
identical bytes, with and without the reference fixture; ids (rels, bookmarks, docPr, footnotes,
numIds, media `n`) follow emission order; "version made by" is `0x14 0x03` for Deflated entries
and `0x0A 0x03` for Stored (PNG/JPEG) entries on every OS (zip writes the entry's version-needed,
`write.rs:709`; the high byte 3 is `System::Unix`);
`SOURCE_DATE_EPOCH=0` gives `1970-01-01T00:00:00Z`, nothing set gives no dcterms.

## 2. Styles — T04 (`src/docx/styles.rs`, `assets/docx/styles.xml`)

### 2.1 API (name-keyed; names compare case-insensitively, as pandoc's `hasStyleName`)
```rust
pub enum StyleKind { Paragraph, Character, Table }
pub struct StyleTable { /* (kind, lowercase name) → id; ids in use; appended <w:style> XML; the source styles.xml */ }
impl StyleTable {
    pub fn builtin() -> StyleTable;                                    // assets/docx/styles.xml (all of §2.3)
    pub fn from_reference(styles_xml: &str) -> Result<StyleTable>;     // the reference's styles; builtin definitions as the fallback source
    pub fn id_for(&self, name: &str, kind: StyleKind) -> Option<&str>; // pure lookup
    /// §2.3 names: the id, appending writer's definition (+ those its basedOn/next/link name) when missing.
    pub fn ensure(&mut self, name: &str, kind: StyleKind) -> String;
    /// User names (`custom-style`, custom callout kinds): the id, else a new definition basedOn `based_on`,
    /// default "Body Text" / "Body Text Char" / "Table" by kind (pandoc's rule for paragraph and character styles;
    /// Table is writer's). `ensure` of an unknown name → this.
    pub fn custom(&mut self, name: &str, kind: StyleKind, based_on: Option<&str>) -> String;
    pub fn set_lang(&mut self, lang: &str);                            // w:lang w:val in w:docDefaults/w:rPrDefault/w:rPr
    pub fn to_xml(&self) -> String;
}
```
Emitters never write a style id literal: every `w:pStyle`/`w:rStyle`/`w:tblStyle` value comes from
`ensure` or `custom`. Appended built-in definitions keep their §2.3 id; a `custom` style's id is its
name without characters outside `[A-Za-z0-9]` (`"Callout Danger"` → `CalloutDanger`; empty → `Style`);
a taken id gets `1`, `2`, … appended. When
a name exists in the table with another kind, writer adds its own under `"<name> Paragraph"` /
`"<name> Char"` / `"<name> Table"` and warns `docx-style-kind-mismatch`. Appended definitions have
their `basedOn`/`next`/`link` rewritten from writer's ids to the table's ids for those names.
`from_reference` also appends writer's definition of every §2.3 id that a reference style's
`basedOn`/`next`/`link` names but the reference lacks (pandoc's default reference: `Table` basedOn
the missing `TableNormal`), and an appended definition never carries `w:default="1"` when the
table already has a default style of that type (V14).

### 2.2 Built-in `styles.xml`: explicit fonts and hex colours, no theme attributes
No `asciiTheme`/`hAnsiTheme`/`eastAsiaTheme`/`cstheme`/`themeColor`/`themeShade`/`themeTint`/`themeFill*`
(V17), no theme part. `rPrDefault` = `<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="{lang}"/>`,
`pPrDefault` = `<w:spacing w:after="0" w:line="264" w:lineRule="auto"/>`; fonts H = `Calibri Light`, M =
`Consolas` (`ascii`/`hAnsi`/`cs`); one `w:default="1"` per type (Normal, Default Paragraph Font, Normal Table).

### 2.3 Vocabulary (pandoc's names; writer's own marked †). `sp a/b` = `w:spacing before/after` (twips), `sz` half-points
| w:name | styleId | type | basedOn | next | link | custom | pPr / rPr / tblPr |
|---|---|---|---|---|---|---|---|
| Normal | Normal | p | — | — | — | | default, qFormat |
| Body Text | BodyText | p | Normal | — | BodyTextChar | | sp 120/120; qFormat |
| First Paragraph | FirstParagraph | p | BodyText | BodyText | — | ✓ | qFormat |
| Compact | Compact | p | BodyText | — | — | ✓ | sp 36/36 |
| Title | Title | p | Normal | BodyText | TitleChar | | keepNext keepLines, jc center, sp 240/120; font H, sz 52, color 1F2328 |
| Subtitle | Subtitle | p | Title | BodyText | SubtitleChar | | sp 0/120; sz 30, color 59636E |
| Author | Author | p | Title | BodyText | — | ✓ | sp 0/0; Calibri, sz 24 |
| Date | Date | p | Title | BodyText | — | | sp 0/240; Calibri, sz 22, color 59636E |
| Abstract Title | AbstractTitle | p | Normal | Abstract | — | ✓ | keepNext, jc center, sp 240/60; b bCs, sz 20 |
| Abstract | Abstract | p | Normal | BodyText | — | ✓ | ind left 720 right 720, sp 60/120; sz 20 |
| Bibliography | Bibliography | p | Normal | Bibliography | — | | sp 0/120 |
| heading 1 … heading 6 | Heading1 … Heading6 | p | Normal | BodyText | Heading*N*Char | | keepNext keepLines, outlineLvl N−1, sp 360/120, 280/80, 240/60, 200/40 (4–6); b bCs (3–6); font H (1–2) else Calibri; color 0F4761; sz 36, 30, 26, 24, 22, 22; uiPriority 9, qFormat |
| Title Char, Subtitle Char, Heading 1 Char … Heading 6 Char | TitleChar, SubtitleChar, Heading1Char … | c | Default Paragraph Font | — | the paragraph style | | the paragraph style's rPr |
| Block Text | BlockText | p | BodyText | BodyText | — | | pBdr left single sz 18 space 8 color D0D7DE, ind left 480 right 480, sp 100/100; color 59636E |
| Footnote Text | FootnoteText | p | Normal | FootnoteText | — | | sp 0/60; sz 18 szCs 18 |
| Footnote Block Text | FootnoteBlockText | p | FootnoteText | FootnoteText | — | | ind left 480 right 480 |
| Definition Term | DefinitionTerm | p | Normal | Definition | — | ✓ | keepNext keepLines, sp 120/0; b bCs |
| Definition | Definition | p | Normal | — | — | ✓ | ind left 360, sp 0/120 |
| Caption | Caption | p | Normal | — | — | | sp 60/120; i iCs, sz 20, color 59636E |
| Table Caption / Image Caption | TableCaption / ImageCaption | p | Caption | — | — | ✓ | keepNext / jc center |
| Figure / Captioned Figure | Figure / CaptionedFigure | p | Normal / Figure | — | — | ✓ | jc center, sp 120/60 / keepNext |
| TOC Heading | TOCHeading | p | Heading1 | BodyText | — | | outlineLvl 9 |
| toc 1 … toc 6 | TOC1 … TOC6 | p | Normal | BodyText | — | | ind left (N−1)×240, sp 0/60; uiPriority 39 |
| Source Code | SourceCode | p | Normal | — | VerbatimChar | ✓ | wordWrap off, shd clear fill F6F8FA, sp 120/120; font M, sz 20 |
| footer | Footer | p | Normal | — | — | | jc center; sz 18, color 59636E |
| Default Paragraph Font | DefaultParagraphFont | c | — | — | — | | default, semiHidden, unhideWhenUsed, uiPriority 1 |
| Body Text Char | BodyTextChar | c | DefaultParagraphFont | — | BodyText | | — |
| Verbatim Char | VerbatimChar | c | BodyTextChar | — | — | ✓ | font M, sz 20 szCs 20 |
| Section Number | SectionNumber | c | BodyTextChar | — | — | ✓ | — |
| Footnote Reference | FootnoteReference | c | BodyTextChar | — | — | | vertAlign superscript |
| Hyperlink | Hyperlink | c | BodyTextChar | — | — | | color 0969DA, u single |
| Normal Table | TableNormal | t | — | — | — | | default, semiHidden, unhideWhenUsed, uiPriority 99; tblInd 0 dxa; tblCellMar top 0, left 108, bottom 0, right 108 (dxa) |
| Table | Table | t | TableNormal | — | — | | qFormat; tblBorders top + bottom single sz 8 color 1F2328; tblStylePr firstRow: rPr b bCs, tcPr tcBorders bottom single sz 4 color 1F2328, vAlign bottom |
| 31 token styles | = name | c | VerbatimChar | — | — | ✓ | §2.4 |
| Callout † | Callout | p | BodyText | — | — | ✓ | pBdr left single sz 24 space 8 color 8C959F, shd clear fill F6F8FA, ind left 240, sp 60/60 |
| Callout Note/Tip/Important/Warning/Caution † | CalloutNote … | p | Callout | — | — | ✓ | pBdr left color / shd fill: 0969DA/DDF4FF, 1A7F37/DAFBE1, 8250DF/FBEFFF, 9A6700/FFF8C5, CF222E/FFEBE9 |
| Callout Title † | CalloutTitle | c | DefaultParagraphFont | — | — | ✓ | b bCs |
| Equation † | Equation | p | Normal | — | — | ✓ | jc center, sp 120/120 |
| Horizontal Rule † | HorizontalRule | p | Normal | — | — | ✓ | pBdr bottom single sz 6 space 1 color D0D7DE, sp 120/120 |
| Cell Prompt † | CellPrompt | p | Normal | — | — | ✓ | keepNext, sp 120/0; font M, sz 18, color 8C959F |
| Cell Output † | CellOutput | p | SourceCode | — | — | ✓ | shd clear fill FFFFFF, pBdr left single sz 8 space 8 color D0D7DE |
| Code Line Number † | CodeLineNumber | c | VerbatimChar | — | — | ✓ | color 8C959F |
| Heading 1 Unlisted … Heading 6 Unlisted † | Heading1Unlisted … Heading6Unlisted | p | Heading*N* | BodyText | — | ✓ | outlineLvl 9 (body text: out of Word's TOC, §4.4) |

`custom ✓` = `w:customStyle="1"`. Borders: `w:val`, `w:sz` (eighths of a point), `w:space`, `w:color`; shading `<w:shd w:val="clear" w:color="auto" w:fill="…"/>`.

### 2.4 Token styles (T04) and the syntect mapping (T40, `src/docx/body.rs`)
`<w:rPr>` per id (pandoc's default style, verified in pandoc 3.12.1 output): KeywordTok b 007020 ·
DataTypeTok 902000 · DecValTok, BaseNTok, FloatTok 40A070 · ConstantTok 880000 · CharTok,
SpecialCharTok, StringTok, VerbatimStringTok 4070A0 · SpecialStringTok BB6688 · ImportTok b 008000 ·
CommentTok i 60A0B0 · DocumentationTok i BA2121 · AnnotationTok, CommentVarTok, InformationTok,
WarningTok b i 60A0B0 · OtherTok 007020 · FunctionTok 06287E · VariableTok 19177C · ControlFlowTok b
007020 · OperatorTok 666666 · BuiltInTok 008000 · PreprocessorTok BC7A00 · AttributeTok 7D9029 ·
AlertTok, ErrorTok b FF0000 · ExtensionTok, RegionMarkerTok, NormalTok none (`b` = `<w:b/>`, `i` = `<w:i/>`, hex = `<w:color w:val>`).

Syntax = `html::highlight::resolve_syntax(lang, plain)` over `html::highlight::SYNTAXES`
(html-output.md §1.5: aliases; `None`, `.plain`, `text|plain|txt|none|output|ps1|powershell` →
`Plain`); `Plain` and `Unknown` → §4.6's unhighlighted form, no diagnostic. With `Syntax(s)`: per
line (`LinesWithEndings`) `ParseState::parse_line` → `ScopeRegionIterator`, one `ScopeStack`
carried across lines. A region's style: walk its stack from the innermost scope outwards; the
first scope that some row's prefix matches (`Scope::is_prefix_of`, rows tried in order) decides; no
match → NormalTok. A parse error on any line → the whole block unhighlighted, as in HTML.
`SCOPE_TOKENS` lives in `src/docx/body.rs` (T40):
```rust
pub const SCOPE_TOKENS: &[(&str, &str)] = &[("comment.block.documentation","DocumentationTok"),("comment.line.documentation","DocumentationTok"),
 ("comment","CommentTok"),("string.regexp","SpecialStringTok"),("constant.character.escape","SpecialCharTok"),("constant.character","CharTok"),
 ("string","StringTok"),("constant.numeric.float","FloatTok"),("constant.numeric.integer.hexadecimal","BaseNTok"),("constant.numeric.integer.octal","BaseNTok"),
 ("constant.numeric.integer.binary","BaseNTok"),("constant.numeric","DecValTok"),("constant","ConstantTok"),("keyword.control.import","ImportTok"),
 ("keyword.control.include","ImportTok"),("meta.import","ImportTok"),("meta.preprocessor","PreprocessorTok"),("keyword.control.directive","PreprocessorTok"),
 ("keyword.control","ControlFlowTok"),("keyword.operator","OperatorTok"),("keyword","KeywordTok"),("storage.modifier","KeywordTok"),("entity.name.tag","KeywordTok"),
 ("meta.annotation","AnnotationTok"),("meta.decorator","AnnotationTok"),("storage.type.annotation","AnnotationTok"),("storage.type","DataTypeTok"),
 ("entity.name.type","DataTypeTok"),("entity.name.class","DataTypeTok"),("support.type","DataTypeTok"),("support.class","DataTypeTok"),
 ("support.function.builtin","BuiltInTok"),("support.function","FunctionTok"),("entity.name.function","FunctionTok"),
 ("entity.other.attribute-name","AttributeTok"),("variable","VariableTok"),("invalid","ErrorTok")];
```

## 3. Numbering — T04 (`src/docx/numbering.rs`, `assets/docx/numbering.xml`)
Two abstract definitions, both `<w:multiLevelType w:val="multilevel"/>`, nine levels `ilvl` 0–8,
`<w:lvlJc w:val="left"/>`, `<w:pPr><w:ind w:left="{720×(ilvl+1)}" w:hanging="360"/></w:pPr>`:
- `w:abstractNumId="991"`, `<w:nsid w:val="0000A991"/>`, bullets: `numFmt bullet`; `ilvl % 3 == 0` →
  `lvlText` U+F0B7 with `<w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol" w:cs="Symbol" w:hint="default"/></w:rPr>`;
  `== 1` → `o` in `Courier New`; `== 2` → U+F0A7 in `Wingdings`.
- `w:abstractNumId="99411"`, `<w:nsid w:val="00A99411"/>`, `<w:start w:val="1"/>`, `numFmt decimal`, `lvlText` `%{ilvl+1}.`.
```rust
pub struct Numbering { /* reference XML kept, next numId, writer's <w:num>s */ }
impl Numbering { pub fn builtin() -> Numbering; pub fn from_reference(numbering_xml: Option<&str>) -> Result<Numbering>;
    pub fn bullet(&mut self) -> u32;  pub fn ordered(&mut self, start: u64) -> u32;  pub fn to_xml(&self) -> String; }
```
numIds count from 1001, one per `Block::List` (nested lists get their own), in emission order;
`bullet` points at 991; `ordered` at 99411 with `<w:lvlOverride w:ilvl="L"><w:startOverride
w:val="{start}"/></w:lvlOverride>` for L = 0…8. With a reference: its `numbering.xml` root start tag and every child are kept
verbatim except `w:abstractNum` with `abstractNumId ≥ 990` and `w:num` with `numId ≥ 1000`
(pandoc's filter); output order is `numPicBullet*`, kept abstractNums, writer's two, kept nums,
writer's nums, `numIdMacAtCleanup?`. No reference numbering → writer's root with `xmlns:w` only.
`numbering.xml` is always written (with no `w:num` when there are no lists).

## 4. Body mapping — T40, T41, T42

### 4.1 Paragraph styles by context (T40)
Within one block sequence (document body, list item, quote, callout, div, footnote, cell, abstract),
ignoring `Raw` blocks, a `Paragraph` is **First Paragraph** when it has no preceding sibling or that
sibling is not a `Paragraph`, else **Body Text**. The container stack (§1.1) decides first, walked
from the innermost entry outwards: `Style(id)` → that style; `Footnote` → Footnote Text;
`TableCell` → **Compact**; `TightList` is skipped, and when the walk ends without a decision a
passed `TightList` gives **Compact** (a tight list inside a callout keeps the callout style). `Style`
entries: `BlockQuote` → Block Text (Footnote Block Text inside a footnote); abstract → Abstract;
`Callout` → `Callout <Kind>`; `Div` with `custom-style` → that style (`custom(name, Paragraph,
None)`); definition bodies → Definition; bibliography → Bibliography.
Headings, code, captions, figures, equations and cells keep their own styles. Empty paragraphs
(no inlines) are not emitted. A paragraph whose only inline is `Math` gets a trailing run
`U+200B` (pandoc #11674). Between two sibling `w:tbl` elements an empty `<w:p/>` is emitted; a
body, footnote or cell whose last element would be a `w:tbl` gets a trailing `<w:p/>`.

### 4.2 Inlines → runs (T40, `src/docx/inline.rs`)
| Inline | XML |
|---|---|
| `Text` | `<w:r>[rPr]<w:t xml:space="preserve">…</w:t></w:r>`; `\t` → `<w:tab/>`, `\n` → space; adjacent runs with identical rPr merge |
| `Emph`/`Strong`/`Strikeout`/`Underline`/`SmallCaps`/`Subscript`/`Superscript` | rPr `i iCs` / `b bCs` / `strike` / `u single` / `smallCaps` / `vertAlign subscript\|superscript`, accumulated through nesting |
| `Code` · `Math` | rStyle Verbatim Char (text as `Text`) · §4.15 |
| `Link` (url not `#…`) | `<w:hyperlink r:id="{rel}" [w:tooltip="{title}"] w:history="1">` + runs with rStyle Hyperlink |
| `Link` (`#x`) | `<w:hyperlink w:anchor="{resolve::anchor(x)}" w:history="1">` + runs in rStyle Hyperlink; no bookmark → §4.9 |
| `Image` · `FootnoteRef` · `Ref`, `Cite` | `image_run` (§4.16) · `NOTEREF` field (§4.8) · §4.9 |
| `Footnote` | `<w:r><w:rPr><w:rStyle w:val="{Footnote Reference}"/></w:rPr><w:footnoteReference w:id="{id}"/></w:r>` (§4.8; nested in a footnote: hoisted, §4.8) |
| `LineBreak` / `SoftBreak` | `<w:r><w:br/></w:r>` / a space |
| `Span` | `custom-style` pair → rStyle `custom(name, Character, None)`; `id` → bookmark around its runs (so do `Code`, `Link`, `Image` with an `id`, §4.9) |
| `Raw { format: "openxml" }` | inserted verbatim inside the paragraph (§4.7 check); other formats dropped |

The innermost character style wins (Code → Verbatim Char, Link → Hyperlink, Span, Callout Title);
direct flags accumulate.

### 4.3 Blocks (T40 unless noted)
| Block | DOCX |
|---|---|
| `Heading` · `Paragraph` · `List`, `DefinitionList` · `CodeBlock` · `Callout`, `Cell` | §4.4 · §4.1 · §4.5 · §4.6 · §4.7 |
| `Figure` · `Table` · `Toc` · `Bibliography` (T41) · `Math` (T42) | §4.10 · §4.12 · §4.13 · §4.14 · §4.15 |
| `BlockQuote`, `Div` | children in the §4.1 container; a `Div`/`Callout`/`Cell`/`CodeBlock`/bare `Table` with `attrs.id` gets a block-level bookmark (`w:bookmarkStart` before its first element, `w:bookmarkEnd` after its last; pandoc's `wrapBookmark`) |
| `Raw` | `openxml`: verbatim at block level (§4.7); others dropped (`check` reports `raw-skipped`) |
| `ThematicBreak` | `<w:p><w:pPr><w:pStyle w:val="{Horizontal Rule}"/></w:pPr></w:p>` |
| `PageBreak` | `<w:p><w:r><w:br w:type="page"/></w:r></w:p>` |

### 4.4 Title block and headings
Before the body, from `DocMeta` (plain text runs; DocMeta strings are not Markdown): `Title`,
`Subtitle`, one `Author` paragraph per author (`name`, then `<w:br/>` + affiliation, then
`<w:br/>` + email as a `mailto:` hyperlink, each when present), `Date`, then, when `abstract_` is
`Some`, an `Abstract Title` paragraph (`meta.vars["abstract-title"]` when a string, else
`term(&lang, Term::Abstract)`, html-output.md §1.3) and the abstract's blocks in the Abstract container.
A `Heading` is `<w:p>` with pStyle `heading N`, except a heading that `resolved.toc` omits (inside
`meta.abstract_`, or class `notoc`/`unlisted`; references.md §4.8), which uses pStyle
`ensure("Heading N Unlisted", Paragraph)` (§2.3): the TOC field's `\o` lists only the built-in
heading styles and `\u` only outline levels 1–9, so Word's update leaves it out as the pre-rendered
entries do (§4.13; LaTeX `\writernotoc`, typst `outlined: false`). Test (T40, `blocks.json`): a
`notoc` level-2 heading and an abstract heading get `Heading2Unlisted` / `Heading{N}Unlisted`.
Inside the heading paragraph, in this order: the label bookmark start
(§4.9; resolve gives every heading an `attrs.id`, which the TOC links to, §4.13), when `number` is
`Some` a run with rStyle Section Number holding the number and a `<w:r><w:tab/></w:r>`, the inline
runs, then the bookmark end.

### 4.5 Lists
Each `List` takes a new numId (`bullet()` / `ordered(start)`); `ilvl` = number of enclosing lists
(capped at 8, deeper → `unsupported-construct`). An item's first `w:p` carries `<w:numPr><w:ilvl
w:val="{ilvl}"/><w:numId w:val="{numId}"/></w:numPr>` (an item starting with a table, or empty, gets
an empty marker paragraph first); its later paragraphs without own numPr get `<w:ind
w:left="{720×(ilvl+1)}" w:firstLine="0"/>`. A task item (`checked: Some(_)`) has no numPr: its first
paragraph gets `<w:ind w:left="{720×(ilvl+1)}" w:hanging="360"/>` and starts with a run `☐` (U+2610)
or `☒` (U+2612) in `<w:rFonts w:ascii="Segoe UI Symbol" w:hAnsi="Segoe UI Symbol"/>`, then
`<w:r><w:tab/></w:r>`. `DefinitionList`: per item a Definition Term paragraph, then each definition in the Definition container.

### 4.6 Code blocks
`code = CodeAttrs::from_attrs(&attrs)` (markdown-dialect.md §4.6); `CodeAttrs` carries no language
(html-output.md Deviations): `lang` = `CodeBlock.lang` (for a cell input, `Some(&cell.language)` when non-empty, else
`None`, as html-output.md §1.9). One paragraph per block, pStyle
Source Code (Cell Output inside cell outputs), lines separated by `<w:r><w:br/></w:r>` (the
trailing newline dropped). One run per token region with rStyle = its token style when §2.4's
`resolve_syntax(lang.as_deref(), code.plain)` is `Syntax` and the block has none of the classes
`output-text`, `output-stream-*`, `output-error`, `output-json` (notebook outputs are never
highlighted); else one run per line with rStyle Verbatim Char. The `ansi` pair is consumed, never
printed; T18 adds its run arm in `body.rs` (notebook-mapping.md §5). `.numbered`: each
line starts with a run, rStyle Code Line Number, holding the line number (`start` + index)
right-aligned with spaces to the width of the last number, plus one space. `highlight` lines: every
run of the line gets `<w:shd w:val="clear" w:color="auto" w:fill="FFF3BF"/>`.

### 4.7 Callouts, cells, raw XML
`Callout`: Kind = `kind` with its first character uppercased; title = `title` inlines, else
`term(&lang, Term::Note|Tip|Important|Warning|Caution)` for the five kinds, else Kind
(html-output.md §1.4); style `Callout <Kind>` (`ensure` for the five, else `custom(name, Paragraph, Some("Callout"))`). Output: a title paragraph (container style +
`<w:keepNext/>`, runs in rStyle Callout Title), then the blocks in that container; Word joins
adjacent paragraphs with equal borders into one box (code, tables, equations keep their styles).
`Cell`: if `show_input` — with `config.notebook.show_prompts` a Cell Prompt paragraph `In [n]:`
(`In [ ]:` without a count) — then the source as a code block in `language`; if `show_output`, per
output (a Cell Prompt `Out [n]:` first for a counted `Result` with prompts on) the blocks of
`output.pick("docx")`, code blocks in Cell Output style. Raw `openxml` (block/inline) must parse as
`<w:body {§1.2 namespaces}>{text}</w:body>` (inline: `<w:p …>`), else it is dropped with `docx-raw-invalid`.

### 4.8 Footnotes (T40, `src/docx/footnotes.rs`)
`word/footnotes.xml` = root (§1.2 namespaces), `<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>`,
`<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>`,
then one `<w:footnote w:id="{id}">` per `Inline::Footnote`, ids (`ids.footnote` from 1) in the order
of the `footnoteReference` runs in `document.xml`; Word numbers notes by that order, so `Footnote.number`
is not written. Blocks in the Footnote Text container, the first paragraph starting with
`<w:r><w:rPr><w:rStyle w:val="{Footnote Reference}"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> </w:t></w:r>`
(an empty Footnote Text paragraph is inserted first when the first block is no paragraph). A
`Footnote` nested in a footnote (e.g. a note citation inside a note, references.md §5.5) is
**hoisted**, since Word cannot nest notes: its `footnoteReference` run goes into `document.xml`
right after its parent's (recursively, in document order), and the parent's text keeps, at its
position, a run in rStyle Footnote Reference holding the nested note's id. Word's numbers then equal
the ids and resolve's `Footnote.number` (cells: when `pick("docx")` is `candidates[0]`); no
diagnostic. A footnote whose `label` is a label (references.md §1.2), or that a `FootnoteRef`
targets, has its reference run wrapped in a bookmark: `resolve::anchor(label)`, else `_Fn{id:06}`. A
`FootnoteRef` (in any part) → the field `NOTEREF {that bookmark} \f \h` whose cached result is one
run in rStyle Footnote Reference holding the target's id (target = the `Inline::Footnote` with the
same `number`; `\f` keeps that style when Word updates the field). Images and links in notes use `footnotes.xml.rels`.

### 4.9 Bookmarks, internal links, references, citations (T40; caption bookmarks T41, equation T42)
Bookmark names: labels → `resolve::anchor(label)` (references.md §1.4 is its only definition; of
labels sharing an anchor, resolve's `anchor-collision`, only the first in document order gets a
bookmark); the bibliography heading → `_refs` (§4.14); internal `_Fn{n:06}` (§4.8; on a clash with
a label bookmark append `x` until unique). `w:id` from `ids.bookmark`, one counter for all parts;
start and end in the same part. Where: headings inside the paragraph (§4.4); figures, tables,
listings around the caption's supplement + number (§4.10); numbered equations around the number
(§4.15); unnumbered labelled figures around the whole caption paragraph content; labelled figures
without caption paragraph and other labelled blocks block-level (§4.3); inline `Span`, `Code`,
`Link`, `Image` with `attrs.id` around their runs; footnote reference runs (§4.8). A pre-pass over what will be emitted (cells:
`pick("docx")`) collects the bookmark set and the footnote ids, so links are checked before emission.
`Ref` with `resolved: Some(r)`, `A = resolve::anchor(&r.anchor)`: `<w:hyperlink w:anchor="{A}"
w:history="1">` containing the runs of `r.text` (no Hyperlink style: cross-references look like
Word's own). `RefForm::Page` (typst's "page N"): inside that hyperlink, `Ref.supplement` `Some(E)`
non-empty → E's runs and a run U+00A0, `Some` empty → nothing, `None` → a run `term(&lang,
Term::Page)` and a run U+00A0; then the field `PAGEREF {A} \h` with the cached result run `?`
(§4.11; Word fills in the page). A `Ref` or `#x` `Link` whose anchor is not in the bookmark set (a label inside
an unpicked cell candidate, an unknown `#x`) renders its runs without hyperlink or field (`Page`:
`r.text`); a `Ref` raises `docx-unknown-anchor`, a `Link` only when `x` is a label or `refs` (else
resolve already raised `link-target-missing`). `resolved: None` (unresolved IR only; resolve turns
failures into references.md §4.7's placeholder `Span`) → text `[?{target}]`. `Cite` with
`resolved: Some(c)`: the runs of `c.inlines`; a note citation's `Inline::Footnote` inside them is an
ordinary footnote (§4.8); `c.note` is not read and the emitter never makes footnotes of its own
(references.md §5.5). `resolved: None` → text `[?{key1}; {key2}]`, the placeholder's text.

### 4.10 Figures (T41, `src/docx/{fields,images}.rs`)
| kind | layout |
|---|---|
| `Image` | paragraph Captioned Figure (Figure when neither caption nor number) with the drawing, then the caption paragraph (Image Caption) |
| `Table` | caption paragraph (Table Caption) first, then the table with `<w:tblCaption w:val="{plain caption}"/>` |
| `Listing` | caption paragraph (Caption) first, then the code paragraph |
| `Other` | the body blocks (sub-figures are ordinary figures; their SEQ fields precede the parent's, §4.11 `\r`), then the caption paragraph (Caption) |

Caption paragraph content (html-output.md §1.6's text): when `number` is `Some`: bookmark start (if
labelled), runs `SUP` and U+00A0, the number fields of §4.11, bookmark end, then (if caption) a run
`resolve::caption_separator(&lang)` and the caption runs; when `number` is `None`: the caption runs
(bookmark around them if labelled); no caption and no number → no caption paragraph. `SUP` =
`doc.resolved.labels[id].supplement` for a labelled figure, else `resolve::supplement(kind, &lang)`
(`Image`/`Other` → `LabelKind::Figure`, `Table` → `Table`, `Listing` → `Listing`).

### 4.11 Fields (one builder, `docx::xml::field(instr: &str, result_runs: &str) -> String`, T04; no `w:dirty` anywhere)
Every field is the complex form, one element per run: `<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {INSTR} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{result runs}<w:r><w:fldChar w:fldCharType="end"/></w:r>`.

| use | instruction | cached result |
|---|---|---|
| figure (Image, Other), effective policy `document` | `SEQ Figure \* ARABIC` | the number |
| figure, effective policy `section` | run with the chapter part as plain text, run `.`, then `SEQ Figure \* ARABIC \s 1` | the part after the last `.` |
| Table / Listing | as Figure with identifier `Table` / `Listing` | as Figure |
| numbered equation | `SEQ Equation \* ARABIC` (`\s 1` and the literal chapter part under the effective `section`), wrapped in `(` `)` runs | the number |
| TOC | `TOC \o "1-{depth}" \h \z \u` | the pre-rendered entries (§4.13) |
| TOC entry page | `PAGEREF {resolve::anchor(&entry.anchor)} \h` | empty (no result run) |
| page reference (`RefForm::Page`, after the supplement and U+00A0, §4.9) | `PAGEREF {A} \h` | `?` |
| repeated footnote (`FootnoteRef`, §4.8) | `NOTEREF {bookmark} \f \h` | the footnote's id, run in rStyle Footnote Reference |
| built-in footer | `PAGE` | `1` |

The **effective policy** is resolve's (references.md §2.3, equations alike §2.4): `section` iff
`[numbering] figures = "section"` and `[numbering] headings` is not `Off` (an invalid pattern counts
as on: resolve falls back to `"1.1"`); otherwise `document`, so under `figures = "section"` with
heading numbering off (`numbering-section-without-headings`) the `document` rows apply, without
chapter run or `\s 1`. The chapter part of `"1.2"` is `1`, the counter `2` (split at the last `.`). The SEQ identifier
names the counter references.md keeps per kind (images and Other share `Figure`). Literal chapter
numbers instead of `STYLEREF 1 \s`: see Deviations. **Word's recomputation** (`[docx]
update_fields = true` is the default) numbers SEQ fields in emission order and, with `\s 1`, resets
at every `heading 1` paragraph, numbered or not, but not at a `Heading 1 Unlisted` one (§4.4);
resolve numbers sub-figures after their parent (references.md §2.3) and resets at every numbered
level-1 heading, listed or not. So T41 tracks, per SEQ identifier, the
value Word computes in emission order (previous + 1; 0 after each paragraph with the `heading 1`
style while `\s 1` applies); when it differs from `n` (the counter part of `number`), the
instruction gains `\r {n}` (`SEQ Figure \* ARABIC \s 1 \r 3`) and tracking continues from `n`.
Tests: `numbering.md` under `section` (`fig-int` → `\r 3`); `numbering.md` under `(false, section,
labelled)` (resolve's fallback) writes `SEQ Figure \* ARABIC` with no `\s 1` and no literal chapter
run; a `:::figure` with two sub-figures
(document policy: child `\r 2`, parent `\r 1`, the next figure `\r 4`). `update_fields = true` writes
`<w:updateFields w:val="true"/>` into settings (Word asks once on open and recomputes); `false`
omits it and Word shows the cached values.

### 4.12 Tables (T41, `src/docx/tables.rs`)
`tw` = available width in twips (§4.16). Grid: when every `ColumnSpec.width` is `Some(f)`: `s = Σf`,
column i = `round(f_i × min(1, 1/s) × tw)`, `<w:tblW w:w="{round(min(s,1)×5000)}" w:type="pct"/>`,
`<w:tblLayout w:type="fixed"/>`; otherwise every column `tw / n` (the last takes the remainder) and
`<w:tblW w:w="0" w:type="auto"/>`. `tblPr` = tblStyle (`custom-style` pair of `Table.attrs`, else of
the enclosing `Figure{kind: Table}`'s attrs, where Markdown puts it → `custom(name, Table, None)`;
else `ensure("Table")`), tblW, tblLayout, `<w:tblLook w:firstRow="{head?1:0}" w:lastRow="{foot?1:0}"
w:firstColumn="0" w:lastColumn="0" w:noHBand="0" w:noVBand="0" w:val="{hex4(head?0x20:0 | foot?0x40:0)}"/>`
(pandoc's `0020` for head only), tblCaption (in a Figure with a caption). Rows: head rows (with `<w:trPr><w:tblHeader/></w:trPr>`),
body, foot. Rows follow the HTML model (cells covered by a rowspan from above are absent); the
emitter keeps an occupancy grid: a cell takes the next free column, `colspan > 1` →
`<w:gridSpan w:val>`, `rowspan > 1` → `<w:vMerge w:val="restart"/>` and, in each covered row, a
`<w:tc>` with `<w:vMerge/>` (continue), the same gridSpan and an empty paragraph. A row that leaves
grid columns free is padded with empty `w:tc`s (one empty Compact paragraph each); cells beyond the
grid are dropped with `unsupported-construct` (V15 holds for ragged notebook tables). `tcPr` always has
`<w:tcW w:w="{Σ spanned grid}" w:type="dxa"/>`. Cell content: one Compact paragraph with
`<w:jc w:val="left|center|right"/>` from `cell.align`, else the column's, none for `Default`;
`header: true` cells in body rows get direct `b bCs`.

### 4.13 Table of contents (T41, `src/docx/toc.rs`)
`Block::Toc { depth }` → `<w:sdt><w:sdtPr><w:docPartObj><w:docPartGallery w:val="Table of Contents"/>[<w:docPartUnique/>]</w:docPartObj></w:sdtPr><w:sdtContent>…</w:sdtContent></w:sdt>`
(`docPartUnique` on the first TOC only): a TOC Heading paragraph (`meta.vars["toc-title"]` when a
string, else `term(&lang, Term::Contents)`), then per entry of `doc.resolved.toc` with `level ≤
depth` a paragraph, pStyle `toc {level}`, `<w:tabs><w:tab w:val="right" w:leader="dot"
w:pos="{tw}"/></w:tabs>`, holding `<w:hyperlink w:anchor="{B}" w:history="1">` around number +
space (when numbered), the plain text of `entry.inlines` (footnotes already removed; the `refs`
entry: §4.14's heading text), `<w:r><w:tab/></w:r>` and the field `PAGEREF {B} \h`. `B` =
`resolve::anchor(&entry.anchor)`: the heading's label bookmark (§4.4) or `_refs` (§4.14); an entry
whose `B` is not in the pre-pass bookmark set (§4.9; a heading in an unpicked cell candidate) is
plain text without hyperlink and field. The TOC field's begin/instruction/separate runs open the first
entry paragraph, its end run closes the last (no entries: one paragraph with the empty field).
Headings missing from `resolved.toc` carry the `Heading N Unlisted` styles (§4.4), so Word's update
of the field lists the same headings.

### 4.14 Bibliography (T41)
`Block::Bibliography` with `doc.resolved.bibliography = Some(b)`: first a `heading 1` paragraph
without number holding the bookmark `_refs` (`resolve::anchor(resolve::BIBLIOGRAPHY_ANCHOR)`, the
target of `Resolved.toc`'s entry and of `#refs` links; references.md §6) around one run:
`meta.vars["reference-section-title"]` when a string, else `term(&lang, Term::Bibliography)`
(html-output.md §1.12). Then one Bibliography paragraph per entry: with `prefix` → prefix runs,
`<w:r><w:tab/></w:r>`, entry runs, pPr
`<w:tabs><w:tab w:val="left" w:pos="720"/></w:tabs><w:ind w:left="720" w:hanging="720"/>`; else
entry runs, plus that `w:ind` when `b.hanging_indent`. Links are external hyperlinks. `None` → nothing.

### 4.15 Math (T42, `src/docx/math.rs`)
Once per document: `macros = math::latex_source::macros_from_config(&config.math.katex_options)`,
and `latex_mathml::macro_diagnostics(&macros)` is pushed (`math-macro-invalid`). `[docx] math =
"omml"`: MathML from LaTeX `latex_mathml::to_mathml(&src, display, &macros)` with `(src, removed) =
latex_source::prepare(source)` (`removed` → `math-tag-ignored`), or from typst
`renderer.typst_mathml(source, display)`; then `math::omml::from_mathml(&mathml)` →
`<m:oMath>…</m:oMath>` (math.md owns the mapping). Every `renderer.equation` call passes `S` =
`latex_source::for_typst(source, &macros).0` for LaTeX, the source for typst (math.md §3.4).
Inline: the `m:oMath` is a sibling of the runs in the paragraph. Display: an Equation paragraph
with `<m:oMathPara><m:oMathParaPr><m:jc m:val="center"/></m:oMathParaPr>{oMath}</m:oMathPara>`.
Numbered display (`number: Some`) → a borderless 3-column table, nothing else in the paragraph:
```xml
<w:tbl><w:tblPr><w:tblStyle w:val="{Normal Table}"/><w:tblW w:w="5000" w:type="pct"/>
  <w:tblBorders><w:top w:val="nil"/><w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/><w:insideV w:val="nil"/></w:tblBorders>
  <w:tblLayout w:type="fixed"/><w:tblLook w:firstRow="0" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="1" w:noVBand="1" w:val="0600"/></w:tblPr>
 <w:tblGrid><w:gridCol w:w="{tw/10}"/><w:gridCol w:w="{tw−2×(tw/10)}"/><w:gridCol w:w="{tw/10}"/></w:tblGrid>
 <w:tr><w:trPr><w:cantSplit/></w:trPr>
  <w:tc><w:tcPr><w:tcW w:w="500" w:type="pct"/><w:vAlign w:val="center"/></w:tcPr><w:p><w:pPr><w:pStyle w:val="{Compact}"/></w:pPr></w:p></w:tc>
  <w:tc><w:tcPr><w:tcW w:w="4000" w:type="pct"/><w:vAlign w:val="center"/></w:tcPr><w:p><w:pPr><w:pStyle w:val="{Equation}"/></w:pPr>{oMathPara or picture run}</w:p></w:tc>
  <w:tc><w:tcPr><w:tcW w:w="500" w:type="pct"/><w:vAlign w:val="center"/></w:tcPr><w:p><w:pPr><w:pStyle w:val="{Equation}"/><w:jc w:val="right"/></w:pPr>
   <w:r><w:t xml:space="preserve">(</w:t></w:r>{bookmark start}{number runs: under the effective "section" policy the literal chapter part and ".", then the SEQ Equation field (§4.11)}{bookmark end}<w:r><w:t xml:space="preserve">)</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
```
Fallback cascade (math.md §5.3, binding; message = the error, span = the block's): (1) LaTeX
`to_mathml` fails → the source-text form (inline: a run in rStyle Verbatim Char; display: a Source
Code paragraph with `<w:jc w:val="center"/>`, in the middle cell when numbered) + `math-syntax`;
typst `typst_mathml` fails → display: step 2's picture, inline: the source-text run; +
`math-conversion-failed`. (2) `from_mathml` fails → display: `renderer.equation(S, dialect, true)`
as a picture run (§4.16; `descr` = the source) in the Equation paragraph or middle cell; inline: one
plain run of `math::omml::fallback_text(&mathml)`; + `math-conversion-failed`. (3) The picture fails
too → step 1's source-text form + `math-conversion-failed`. `[docx] math = "image"`: every equation
is a picture (display as step 2); an inline picture's run gets `<w:position
w:val="{−round(2 × (height_pt − baseline_pt) × s)}"/>` (half-points, onto the baseline; `s` §4.16);
a renderer error → step 1's form + `math-conversion-failed`.

### 4.16 Images (T41, `src/docx/images.rs`)
Sources: `asset:<id>` → `assets.read(id)` with the asset's MIME; remote/other URLs are never
fetched → a hyperlink run (alt text, or the URL) + `unsupported-construct`. Formats: PNG and JPEG
embedded as is; SVG embedded as is **plus** a PNG fallback from `renderer.image(svg, mime,
Some(display_width_pt))` (render error → SVG-only blip, `docx-svg-no-fallback`); GIF, WebP and
PDF converted with `renderer.image(…, Some(display_width_pt))` and embedded as PNG; any other MIME
→ alt-text run + `unsupported-construct`. An equation `Picture` embeds its PNG (+ `svgBlip` when `svg`
is `Some`) at `width_pt × s × 12 700` by `height_pt × s × 12 700`, clamped as below; `s` = the
Normal style's `w:sz` (else `w:docDefaults`' `w:rPrDefault`, else 22) / 22, i.e. the body size over
the renditions' 11 pt (math.md §5.3; built-in styles: 1). Media parts are deduplicated
by (asset id or content hash, variant) and named `w{n}.{png|jpeg|svg}` in first-use order.
Size (EMU): intrinsic = pixels (`imagesize::blob_size`) × 914 400 / dpi, dpi from PNG `pHYs`
(unit 1: px per metre × 0.0254, rounded) or JPEG JFIF APP0 (units 1 dpi, 2 dpcm × 2.54), else 96;
values outside 10–2400 ignored; SVG/PDF intrinsic = `renderer.image(…, None)`'s `width_pt/height_pt`
× 12 700 (render error: 6 × 4 in). `attrs.pairs` `width`/`height` (markdown-dialect.md grammar): px × 9 525, pt × 12 700,
cm × 360 000, mm × 36 000, in × 914 400, em × 152 400 (12 pt), `%` of the available width (width)
or text height (height). One given → the other keeps the aspect ratio; none → intrinsic. Clamp:
scale both down so width ≤ available width, then so height ≤ text height; round to integers ≥ 1.
Available width = `layout.text_width_twips × 635` EMU, inside a table cell its spanned grid width
minus 216 twips × 635. Drawing (inline only; `{n}` = `ids.doc_pr`, unique across parts):
```xml
<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>
 <wp:docPr id="{n}" name="Picture {n}" descr="{alt: plain text, whitespace collapsed}"[ title="{title}"]/>
 <wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr>
 <a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic>
  <pic:nvPicPr><pic:cNvPr id="0" name="{media file name}"/><pic:cNvPicPr><a:picLocks noChangeAspect="1" noChangeArrowheads="1"/></pic:cNvPicPr></pic:nvPicPr>
  <pic:blipFill><a:blip r:embed="{png or jpeg rel}">{SVG only: <a:extLst><a:ext uri="{96DAC541-7B7A-43D3-8B79-37D633B846F1}"><asvg:svgBlip xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main" r:embed="{svg rel}"/></a:ext></a:extLst>}</a:blip><a:stretch><a:fillRect/></a:stretch></pic:blipFill>
  <pic:spPr bwMode="auto"><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln w="9525"><a:noFill/><a:headEnd/><a:tailEnd/></a:ln></pic:spPr>
 </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
```

> **Contract clarification (coordinator, 2026-10-27):** asset-registry read
> failures remain errors, including a file disappearing after registration;
> a private first-error slot defers them through the unit-returning run API
> and `write` returns the error before packaging. Corrupt PNG/JPEG/GIF/WebP
> content and failed GIF/WebP/PDF conversions produce alt text and a located
> `unsupported-construct` warning. SVG retains the natural-size and failed
> PNG-fallback rules above when its first XML element is `svg`, using the
> validator's BOM/declaration/comment/DOCTYPE handling; other bytes produce
> alt text and `unsupported-construct`. No external URL is fetched.

## 5. Child order and ordered builders — T04 (`docx::xml` in `src/docx/mod.rs`)
Source: python-docx 1.2.0 `_tag_seq` (`docx/oxml/{text/parfmt,text/font,table,section,styles,settings}.py`,
read 2026-10-24); `w:lvl`, `w:abstractNum`, `w:numbering`, `wp:inline`, borders and `m:*` from
ECMA-376 Part 1 (§17.9, §20.4, §22.1, recalled), the `m:*Pr` rows cross-checked against the
element emission order of Word's own `mathml2omml.xsl` (in Word.app's Resources).
```rust
pub const PPR: &[&str] = &["w:pStyle","w:keepNext","w:keepLines","w:pageBreakBefore","w:framePr","w:widowControl","w:numPr","w:suppressLineNumbers","w:pBdr","w:shd","w:tabs","w:suppressAutoHyphens","w:kinsoku","w:wordWrap","w:overflowPunct","w:topLinePunct","w:autoSpaceDE","w:autoSpaceDN","w:bidi","w:adjustRightInd","w:snapToGrid","w:spacing","w:ind","w:contextualSpacing","w:mirrorIndents","w:suppressOverlap","w:jc","w:textDirection","w:textAlignment","w:textboxTightWrap","w:outlineLvl","w:divId","w:cnfStyle","w:rPr","w:sectPr","w:pPrChange"];
pub const RPR: &[&str] = &["w:rStyle","w:rFonts","w:b","w:bCs","w:i","w:iCs","w:caps","w:smallCaps","w:strike","w:dstrike","w:outline","w:shadow","w:emboss","w:imprint","w:noProof","w:snapToGrid","w:vanish","w:webHidden","w:color","w:spacing","w:w","w:kern","w:position","w:sz","w:szCs","w:highlight","w:u","w:effect","w:bdr","w:shd","w:fitText","w:vertAlign","w:rtl","w:cs","w:em","w:lang","w:eastAsianLayout","w:specVanish","w:oMath"];
pub const TBLPR: &[&str] = &["w:tblStyle","w:tblpPr","w:tblOverlap","w:bidiVisual","w:tblStyleRowBandSize","w:tblStyleColBandSize","w:tblW","w:jc","w:tblCellSpacing","w:tblInd","w:tblBorders","w:shd","w:tblLayout","w:tblCellMar","w:tblLook","w:tblCaption","w:tblDescription","w:tblPrChange"];
pub const TRPR: &[&str] = &["w:cnfStyle","w:divId","w:gridBefore","w:gridAfter","w:wBefore","w:wAfter","w:cantSplit","w:trHeight","w:tblHeader","w:tblCellSpacing","w:jc","w:hidden","w:ins","w:del","w:trPrChange"];
pub const TCPR: &[&str] = &["w:cnfStyle","w:tcW","w:gridSpan","w:hMerge","w:vMerge","w:tcBorders","w:shd","w:noWrap","w:tcMar","w:textDirection","w:tcFitText","w:vAlign","w:hideMark","w:headers","w:cellIns","w:cellDel","w:cellMerge","w:tcPrChange"];
pub const SECTPR: &[&str] = &["w:headerReference","w:footerReference","w:footnotePr","w:endnotePr","w:type","w:pgSz","w:pgMar","w:paperSrc","w:pgBorders","w:lnNumType","w:pgNumType","w:cols","w:formProt","w:vAlign","w:noEndnote","w:titlePg","w:textDirection","w:bidi","w:rtlGutter","w:docGrid","w:printerSettings","w:sectPrChange"];   // header/footerReference repeat, interleaved
pub const STYLE: &[&str] = &["w:name","w:aliases","w:basedOn","w:next","w:link","w:autoRedefine","w:hidden","w:uiPriority","w:semiHidden","w:unhideWhenUsed","w:qFormat","w:locked","w:personal","w:personalCompose","w:personalReply","w:rsid","w:pPr","w:rPr","w:tblPr","w:trPr","w:tcPr","w:tblStylePr"];
pub const LVL: &[&str] = &["w:start","w:numFmt","w:lvlRestart","w:pStyle","w:isLgl","w:suff","w:lvlText","w:lvlPicBulletId","w:legacy","w:lvlJc","w:pPr","w:rPr"];
pub const SETTINGS: &[&str] = &["w:writeProtection","w:view","w:zoom","w:removePersonalInformation","w:removeDateAndTime","w:doNotDisplayPageBoundaries","w:displayBackgroundShape","w:printPostScriptOverText","w:printFractionalCharacterWidth","w:printFormsData","w:embedTrueTypeFonts","w:embedSystemFonts","w:saveSubsetFonts","w:saveFormsData","w:mirrorMargins","w:alignBordersAndEdges","w:bordersDoNotSurroundHeader","w:bordersDoNotSurroundFooter","w:gutterAtTop","w:hideSpellingErrors","w:hideGrammaticalErrors","w:activeWritingStyle","w:proofState","w:formsDesign","w:attachedTemplate","w:linkStyles","w:stylePaneFormatFilter","w:stylePaneSortMethod","w:documentType","w:mailMerge","w:revisionView","w:trackRevisions","w:doNotTrackMoves","w:doNotTrackFormatting","w:documentProtection","w:autoFormatOverride","w:styleLockTheme","w:styleLockQFSet","w:defaultTabStop","w:autoHyphenation","w:consecutiveHyphenLimit","w:hyphenationZone","w:doNotHyphenateCaps","w:showEnvelope","w:summaryLength","w:clickAndTypeStyle","w:defaultTableStyle","w:evenAndOddHeaders","w:bookFoldRevPrinting","w:bookFoldPrinting","w:bookFoldPrintingSheets","w:drawingGridHorizontalSpacing","w:drawingGridVerticalSpacing","w:displayHorizontalDrawingGridEvery","w:displayVerticalDrawingGridEvery","w:doNotUseMarginsForDrawingGridOrigin","w:drawingGridHorizontalOrigin","w:drawingGridVerticalOrigin","w:doNotShadeFormData","w:noPunctuationKerning","w:characterSpacingControl","w:printTwoOnOne","w:strictFirstAndLastChars","w:noLineBreaksAfter","w:noLineBreaksBefore","w:savePreviewPicture","w:doNotValidateAgainstSchema","w:saveInvalidXml","w:ignoreMixedContent","w:alwaysShowPlaceholderText","w:doNotDemarcateInvalidXml","w:saveXmlDataOnly","w:useXSLTWhenSaving","w:saveThroughXslt","w:showXMLTags","w:alwaysMergeEmptyNamespace","w:updateFields","w:hdrShapeDefaults","w:footnotePr","w:endnotePr","w:compat","w:docVars","w:rsids","m:mathPr","w:attachedSchema","w:themeFontLang","w:clrSchemeMapping","w:doNotIncludeSubdocsInStats","w:doNotAutoCompressPictures","w:forceUpgrade","w:captions","w:readModeInkLockDown","w:smartTagType","sl:schemaLibrary","w:shapeDefaults","w:doNotEmbedSmartTags","w:decimalSymbol","w:listSeparator"];
```
Further sequences (`ORDERS`, used by the validator; a child not listed is a violation unless outside `w`/`m`/`wp`/`a`/`pic`;
`w:tbl`, `w:tr`, `w:tc`, `w:p`, `w:r` only fix their properties element first and `w:body` its `w:sectPr` last):
```rust
pub const ORDERS: &[(&str, &[&str])] = &[("w:styles", &["w:docDefaults","w:latentStyles","w:style"]), ("w:docDefaults", &["w:rPrDefault","w:pPrDefault"]),
 ("w:numbering", &["w:numPicBullet","w:abstractNum","w:num","w:numIdMacAtCleanup"]), ("w:abstractNum", &["w:nsid","w:multiLevelType","w:tmpl","w:name","w:styleLink","w:numStyleLink","w:lvl"]),
 ("w:num", &["w:abstractNumId","w:lvlOverride"]), ("w:lvlOverride", &["w:startOverride","w:lvl"]), ("w:numPr", &["w:ilvl","w:numId"]),
 ("w:pBdr", &["w:top","w:left","w:bottom","w:right","w:between","w:bar"]), ("w:tblBorders", &["w:top","w:left","w:bottom","w:right","w:insideH","w:insideV"]),
 ("w:tcBorders", &["w:top","w:left","w:bottom","w:right","w:insideH","w:insideV","w:tl2br","w:tr2bl"]), ("w:tblCellMar", &["w:top","w:left","w:bottom","w:right"]),
 ("w:footnotePr", &["w:pos","w:numFmt","w:numStart","w:numRestart"]), ("w:sdt", &["w:sdtPr","w:sdtEndPr","w:sdtContent"]),
 ("wp:inline", &["wp:extent","wp:effectExtent","wp:docPr","wp:cNvGraphicFramePr","a:graphic"]), ("m:r", &["m:rPr","w:rPr","m:t"]),
 ("m:rPr", &["m:lit","m:nor","m:scr","m:sty","m:brk","m:aln"]), ("m:naryPr", &["m:chr","m:limLoc","m:grow","m:subHide","m:supHide","m:ctrlPr"]),
 ("m:dPr", &["m:begChr","m:sepChr","m:endChr","m:grow","m:shp","m:ctrlPr"]), ("m:mPr", &["m:baseJc","m:plcHide","m:rSpRule","m:cGpRule","m:rSp","m:cSp","m:cGp","m:mcs","m:ctrlPr"]),
 ("m:mcPr", &["m:count","m:mcJc"]), ("m:groupChrPr", &["m:chr","m:pos","m:vertJc","m:ctrlPr"]), ("m:phantPr", &["m:show","m:zeroWid","m:zeroAsc","m:zeroDesc","m:transp","m:ctrlPr"]),
 ("m:borderBoxPr", &["m:hideTop","m:hideBot","m:hideLeft","m:hideRight","m:strikeH","m:strikeV","m:strikeBLTR","m:strikeTLBR","m:ctrlPr"]),
 ("m:fPr", &["m:type","m:ctrlPr"]), ("m:radPr", &["m:degHide","m:ctrlPr"]), ("m:accPr", &["m:chr","m:ctrlPr"]), ("m:barPr", &["m:pos","m:ctrlPr"]),
 ("m:boxPr", &["m:opEmu","m:noBreak","m:diff","m:brk","m:aln","m:ctrlPr"]), ("m:eqArrPr", &["m:baseJc","m:maxDist","m:objDist","m:rSpRule","m:rSp","m:ctrlPr"]),
 ("m:f", &["m:fPr","m:num","m:den"]), ("m:rad", &["m:radPr","m:deg","m:e"]), ("m:nary", &["m:naryPr","m:sub","m:sup","m:e"]), ("m:sSubSup", &["m:sSubSupPr","m:e","m:sub","m:sup"]),
 ("m:sPre", &["m:sPrePr","m:sub","m:sup","m:e"]), ("m:limLow", &["m:limLowPr","m:e","m:lim"]), ("m:limUpp", &["m:limUppPr","m:e","m:lim"]),
 ("m:mathPr", &["m:mathFont","m:brkBin","m:brkBinSub","m:smallFrac","m:dispDef","m:lMargin","m:rMargin","m:defJc","m:preSp","m:postSp","m:interSp","m:intraSp","m:wrapIndent","m:wrapRight","m:intLim","m:naryLim"])];
```
```rust
/// Children are kept as (position in `order`, insertion seq, xml) and serialized sorted: call order never matters.
/// A name missing from `order` is a bug: debug_assert!, and the child is dropped with tracing::error!.
pub struct Props { tag: &'static str, attrs: String, order: &'static [&'static str], children: Vec<(usize, usize, String)> }
impl Props { pub fn new(tag: &'static str, order: &'static [&'static str]) -> Props;
    pub fn set(&mut self, name: &'static str, xml: impl Into<String>) -> &mut Self;   // replaces a child of that name
    pub fn push(&mut self, name: &'static str, xml: impl Into<String>) -> &mut Self;  // repeatable children
    pub fn is_empty(&self) -> bool;  pub fn to_xml(&self) -> String; }               // "" when empty
pub struct ParaProps { pub style: Option<String>, pub keep_next: bool, pub num: Option<(u8, u32)>, pub border: Option<String>, pub shading: Option<String>,
    pub tabs: Vec<String>, pub spacing: Option<String>, pub ind: Option<String>, pub jc: Option<&'static str> }          // to_xml via Props("w:pPr", PPR)
pub struct RunProps { pub style: Option<String>, pub fonts: Option<String>, pub bold: bool, pub italic: bool, pub small_caps: bool, pub strike: bool,
    pub color: Option<String>, pub position: Option<i32>, pub underline: bool, pub shading: Option<String>, pub vert_align: Option<&'static str> } // bold → <w:b/><w:bCs/>
// likewise TableProps/RowProps/CellProps (TBLPR/TRPR/TCPR); styles.rs, numbering.rs, reference.rs use Props with STYLE/LVL/SETTINGS/SECTPR.
```

## 6. Reference documents — T04 (`src/docx/reference.rs`), T43 (§6.5)

### 6.1 Reading
`[docx] reference_doc` (`--reference-doc`, front matter `reference_doc`) names a `.docx` that must hold
`[Content_Types].xml`, `word/document.xml`, `word/styles.xml`, `word/_rels/document.xml.rels`; a missing
part, a non-ZIP file, malformed XML, or WordprocessingML/relationships namespaces not bound to `w`/`r`
→ `Error::Resource { what: "reference document", path, message }`. XML is read with quick-xml 0.42 (§9).

### 6.2 Parts copied verbatim vs regenerated
Copied byte-for-byte: the targets of the reference's `document.xml.rels` relationships of types
`…/theme`, `…/fontTable`, `…/webSettings`, `…/header`, `…/footer`; for each copied part
`<dir>/<file>` its relationships part `<dir>/_rels/<file>.rels` when present (OPC; e.g.
`word/theme/_rels/theme1.xml.rels`) and, transitively, the internal targets it names, resolved
relative to `<dir>` (`../media/x.png` from `word/theme/` → `word/media/x.png`): fonts, media, ….
`reference.rs` scans every copied XML part for `wp:docPr/@id` and `w:bookmarkStart/@w:id`;
`Ids.doc_pr` and `Ids.bookmark` start at 1 + the largest value found (1 when none), so V08/V11 hold
with a header picture. The built-in footer is `word/footer{k}.xml`, k the smallest ≥ 1 not taken by
a copied part. The copied rels keep their `Id`s in writer's `document.xml.rels`; content types are taken from the
reference's `[Content_Types].xml` (Override, else Default by extension). Everything else —
`[Content_Types].xml`, `_rels/.rels`, `docProps/*`, `document.xml`, its rels, footnotes, comments,
`customXml`, the reference's body — is regenerated or dropped. `styles.xml`, `numbering.xml` and
`settings.xml` are merged (§6.3, §3, §6.4). No theme relationship → no theme part.

### 6.3 Styles and section
Styles: the reference's root start tag and children verbatim; `set_lang` edits only `w:lang` in
`w:docDefaults/w:rPrDefault/w:rPr` (inserted when absent); writer's definitions for vocabulary
names the reference lacks are appended before `</w:styles>`, as are those its styles' basedOn/next/link
name (§2.1). A reference style with a writer name wins entirely (its formatting, id and basedOn are used). sectPr: the `w:sectPr` that
is a **direct child of `w:body`**, copied as the raw XML slice (its `r:id`s point at the kept
header/footer rels); no reference or no body-level sectPr → the built-in one (with the footer). Text width = `pgSz/@w:w − pgMar/@w:left −
pgMar/@w:right − pgMar/@w:gutter`, text height = `pgSz/@w:h − |top| − |bottom|`; a missing `pgSz`
element or attribute takes the built-in page value, a missing `pgMar` element or
`left`/`right`/`top`/`bottom` attribute the built-in 1440, a missing gutter 0 (pandoc's default
reference has neither: a4 → 11906 − 2880 = 9026 twips, a T04 unit test; pandoc assumes 420 pt). Built-in sectPr (page from `[pdf] paper`:
`a4` 11906×16838, `a5` 8391×11906, `us-letter` 12240×15840, `us-legal` 12240×20160, anything else A4):
`<w:sectPr><w:footerReference w:type="default" r:id="{footer rel}"/><w:footnotePr><w:numRestart w:val="eachSect"/></w:footnotePr><w:pgSz w:w="…" w:h="…"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/><w:cols w:space="720"/></w:sectPr>`.
`word/footer{k}.xml`: `<w:ftr xmlns:w=… xmlns:r=…><w:p><w:pPr><w:pStyle w:val="{footer}"/></w:pPr>{PAGE field}</w:p></w:ftr>`.

### 6.4 Settings
Built-in `assets/docx/settings.xml` children: `<w:zoom w:percent="100"/>`,
`<w:defaultTabStop w:val="720"/>`, `<w:characterSpacingControl w:val="doNotCompress"/>`,
`<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>`,
`<m:mathPr><m:mathFont m:val="Cambria Math"/><m:brkBin m:val="before"/><m:brkBinSub m:val="--"/><m:smallFrac m:val="off"/><m:dispDef/><m:lMargin m:val="0"/><m:rMargin m:val="0"/><m:defJc m:val="centerGroup"/><m:wrapIndent m:val="1440"/><m:intLim m:val="subSup"/><m:naryLim m:val="undOvr"/></m:mathPr>`
(Word's own default, verbatim: `word/settings.xml` of Word.app's `Contents/Resources/QuickStyles/Modern.dotx`,
read 2026-10-25; supersedes math.md §5.1's list, Deviations),
`<w:decimalSymbol w:val="."/>`, `<w:listSeparator w:val=","/>`; root namespaces `w`, `m`, `r`, `o`, `v`,
`w10`, `sl`. Merge (after pandoc's `copyChildren`): for each name of `SETTINGS` in order, all occurrences
(pandoc copies only the first; writer keeps repeatable ones such as `w:activeWritingStyle`, `w:smartTagType`)
from the reference when the name is in pandoc's `settingsElementNames` (= `SETTINGS` minus
`w:footnotePr`, `w:endnotePr`, `m:mathPr`, `sl:schemaLibrary`) and the reference has it, else
writer's own element if any; `w:updateFields` always follows `[docx] update_fields`. The root
declares writer's namespaces plus every other `xmlns:*` of the reference root (sorted by prefix,
writer's URIs win), without `mc:Ignorable`.

### 6.5 `writer eject docx-reference FILE` (T43, `src/docx/custom_style.rs`)
`pub fn showcase_document() -> (Document, AssetRegistry)` is a resolved IR (`resolved.toc`,
`labels`, `bibliography` filled by hand) using every §2.3 style it can reach, each paragraph naming
its style ("This paragraph uses Body Text."): title block and abstract, TOC, numbered headings 1–6
and a `notoc` heading per level 1–6,
every list kind, quote, definition list, highlighted code, link, footnote, a captioned figure (a
built-in 64×32 grey PNG constant through `register_bytes`), table, listing, numbered LaTeX
equation, the five callouts and a custom one, a bibliography, a cell, a horizontal rule.
`pub fn reference_docx() -> Result<Vec<u8>>` = `write` of it with `Config::default()`, no
reference, `all_styles: true` (before serialising, `write` calls `styles.ensure(name, kind)` for
every §2.3 name, all 31 token styles included), a renderer that returns errors and no timestamp
(deterministic, validator-clean). Acceptance: every §2.3 name is in `styles.xml`, and every one is
used in `document.xml`, `footnotes.xml` or the footer except the 8 token styles `SCOPE_TOKENS` never
yields (VerbatimStringTok, OtherTok, AlertTok, InformationTok, WarningTok, CommentVarTok,
ExtensionTok, RegionMarkerTok) and the pure basedOn/link targets (Normal, Default Paragraph Font,
Body Text Char, the `* Char` styles). eject prints the config lines `[docx]` and `reference_doc = "<FILE>"`.

## 7. Validation — T04 (validator, read-back helper), T43 (smoke, checklist)

### 7.1 Structural validator (`src/docx/validate.rs`)
```rust
pub struct Violation { pub rule: &'static str, pub part: String, pub message: String }
pub struct ValidateOptions { pub builtin_styles: bool }
pub fn validate(docx: &[u8], opts: &ValidateOptions) -> Vec<Violation>;
```
Every DOCX a test produces is validated and must give `[]`. **V01** ZIP readable, first entry
`[Content_Types].xml`, unique names, no directories, all mtimes 1980-01-01 00:00:00. **V02** non-media
parts well-formed; reading to `Eof` without error is not enough (quick-xml 0.42 accepts `<a><b>`,
`<a/><b/>`, `<a>&foo;</a>`, duplicate or unquoted attributes, U+0001, unbound prefixes, probe), so the
validator also keeps an element stack (every Start closed; exactly one root; only whitespace,
comments and PIs outside it), iterates every element's attributes with `with_checks(true)` and
`normalized_value(XmlVersion::Implicit1_0)?`, accepts an `Event::GeneralRef` only for
`amp`/`lt`/`gt`/`quot`/`apos` or a char ref whose `resolve_char_ref()` is an XML 1.0 char, checks
every text and attribute char with `is_xml_char`, and resolves every element and attribute prefix
against the in-scope `xmlns:*` (`xml` predeclared); one failing fixture per sub-check. **V03** one Default/Override per part, Overrides name parts,
`/word/document.xml` has the main type. **V04** `_rels/.rels` → `word/document.xml`. **V05** rel Ids
unique per file, internal targets exist, hyperlinks `TargetMode="External"`, every `r:id`/`r:embed`/
`r:link` resolves in its part's rels. **V06** child order (§5) in document, footnotes, styles,
numbering, settings. **V07** one body-level `w:sectPr`, last in `w:body`. **V08** bookmarks paired by
id within a part, ids unique across parts, names unique, ≤ 40 chars of `[A-Za-z0-9_]`. **V09** every
`w:hyperlink/@w:anchor` and `REF`/`PAGEREF`/`NOTEREF` target is a bookmark. **V10** field chars
balanced per part (begin, optional separate, end; nesting allowed). **V11** `wp:docPr/@id` unique
across parts. **V12** footnotes -1/0 are separator/continuationSeparator; each `footnoteReference`
id ≥ 1 exists and is referenced once. **V13** used numIds have a `w:num`, each `w:num` an
abstractNum, no duplicate ids, `ilvl` 0–8. **V14** pStyle/rStyle/tblStyle values are styleIds of that
type; styleIds unique; basedOn/next/link resolve; one default per type. **V15** each `w:tc` ends with
`w:p`; gridSpans per row sum to the `gridCol` count; `vMerge` continue only below restart/continue.
**V16** `w:t`/`w:instrText`/`m:t` with edge whitespace have `xml:space="preserve"`. **V17**
(`builtin_styles`) no theme attribute in `styles.xml`. **V18** media are referenced and their bytes
match the type (`\x89PNG`, `\xFF\xD8`; SVG: after an optional BOM, XML declaration, comments and
DOCTYPE, the first element is `<svg`). **V19** no `w:footnotePr`/`w:endnotePr` in settings.
**V20** `dcterms:created`/`modified` both present (W3CDTF `…Z`) or both absent.

### 7.2 Read-back (helper by T04, wave 2; used by T40–T43) and 7.3 smoke (T43, `tests/docx_smoke.rs`)
Dev-dependency `docx-rs = { version = "=0.4.22", default-features = false }`. T04 writes `tests/common/docx.rs`:
`pub fn read_back(bytes: &[u8]) -> ReadBackCounts { paragraphs, styles, abstract_nums, images }` —
`docx_rs::read_docx_with_options(&bytes, ReadDocxOptions::default().with_image_previews(false))` must be `Ok`;
the counts of `document.children` paragraphs, `styles.styles`, `numberings.abstract_nums` and `images` are
asserted as an `insta` snapshot per fixture (`docx_snapshots__<fixture>__readback.snap`). Smoke (each tool
detected, else print `skipping`; no Word automation, osascript hangs, R3 §5): `textutil -convert txt -stdout out.docx`
exits 0 and contains every heading; `python3 -I -c "import docx,sys; d=docx.Document(sys.argv[1]); print(len(d.paragraphs), len(d.tables), len(d.inline_shapes))" out.docx`
equals the triple written in the test (python-docx from a venv, never installed by the test); `qlmanage -t -s 1000 -o <tmp> out.docx` PNG kept as an artefact.

### 7.4 Manual Word checklist (`docs/dev/docx-validation.md`, T43; run before each release on Word for Mac and Windows)
For `examples/*`, the showcase and `numbering.md` with a sub-figure group, an abstract heading and a
numbered `notoc` level-1 heading: no repair prompt;
`update_fields = true` → one "update fields" prompt, TOC and page-reference numbers appear, the
updated TOC leaves out `notoc` and abstract headings, every
figure/table/listing/equation number (both policies and the `(false, section)` fallback; sub-figures;
a figure after an unnumbered level-1 heading and after a numbered `notoc` one) and `NOTEREF` (still superscript) stays as cached, a hoisted nested footnote keeps
its number; `false` → no prompt; cross-reference and TOC links jump; SVG crisp, PNG
fallback in an older viewer; OMML editable, equation pictures on the baseline; lists restart, ☐/☒,
Symbol/Courier New/Wingdings bullets; callouts form one box; token colours; restyle test (change
Heading 1 colour and Callout Note fill in the ejected reference, rebuild: changes, header/footer and
page size come through); the file opens in LibreOffice and Google Docs.

## 8. Diagnostics
| code | level | raised by |
|---|---|---|
| `unsupported-construct` | warning (error with `--strict`) | remote/unsupported images, lists deeper than 9, table cells beyond the grid (T40/T41); §1.1 stubs |
| `math-conversion-failed` | warning (error with `--strict`) | §4.15 (T42) |
| `math-syntax`, `math-tag-ignored`, `math-macro-invalid` | warning (owner math.md §9) | §4.15 (T42) |
| `raw-skipped` | info, `check --target docx` only | non-`openxml` `Raw` (T71, markdown-dialect.md §10) |
| `docx-raw-invalid` | warning | malformed `openxml` raw text, dropped (T40) |
| `docx-unknown-anchor` | warning | a `Ref`, or a `#x` link to a label or `refs`, whose bookmark is not emitted (§4.9, T40) |
| `docx-style-kind-mismatch` | warning | a style name exists with another type (T04) |
| `docx-svg-no-fallback` | warning | SVG fallback PNG failed; SVG-only blip (T41) |

Diagnostics carry the enclosing block's span; an invalid reference document is an error, not a diagnostic.

## 9. Verified API hints
```rust
// zip 9.0.5 (probes 2026-10-24/25: byte-identical twice, version-made-by 0x14 0x03 Deflated / 0x0A 0x03 Stored, mode 0o100644)
use zip::{write::SimpleFileOptions, CompressionMethod, DateTime, System, ZipWriter, ZipArchive};
let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).compression_level(Some(6))
    .last_modified_time(DateTime::default()).system(System::Unix).unix_permissions(0o644);
let stored = deflated.compression_method(CompressionMethod::Stored).compression_level(None); // Some(_) → "Unsupported compression level"
let mut zw = ZipWriter::new(std::io::Cursor::new(Vec::new()));   // + use std::io::Write
zw.start_file("[Content_Types].xml", deflated)?; zw.write_all(xml.as_bytes())?;
let bytes = zw.finish()?.into_inner();
let mut ar = ZipArchive::new(std::io::Cursor::new(&bytes[..]))?;   // ar.by_name(name)?, ar.by_index(i)?.name()? → ZipResult<Cow<str>>
// quick-xml 0.42.0: names and values are &str now (QName(pub &str)); `unescape_value` is deprecated
use quick_xml::{events::Event, name::QName, Reader, XmlVersion};
let mut r = Reader::from_str(xml);
let (mut open, mut body_sect): (Vec<String>, Option<&str>) = (Vec::new(), None);  // open element names; the last body-level sectPr
loop { let start = r.buffer_position() as usize;
  match r.read_event()? {   // only a sectPr opened directly in w:body counts (a w:pPr/w:sectPr section break does not, §6.3)
    Event::Start(e) if e.name().as_ref() == "w:sectPr" && open == ["w:document", "w:body"] => { let n = e.name().as_ref().to_owned();
        r.read_to_end(QName(&n))?; body_sect = Some(&xml[start..r.buffer_position() as usize]); /* verbatim slice */ }
    Event::Start(e) => open.push(e.name().as_ref().to_owned()),
    Event::End(_) => { open.pop(); }
    Event::Empty(e) => for a in e.attributes() { let a = a?; let (k, v) = (a.key.as_ref(), a.normalized_value(XmlVersion::Implicit1_0)?); }
    Event::Eof => break, _ => {} } }
// imagesize 0.15.0: imagesize::blob_size(&bytes) -> ImageResult<ImageSize { width: usize, height: usize }> (PNG/JPEG/GIF/WebP; SVG → Err)
// syntect 5.3: state.parse_line(line, &ss) -> Result<Vec<(usize, ScopeStackOp)>, ParsingError>; for (text, op) in
//   syntect::easy::ScopeRegionIterator::new(&ops, line) { stack.apply(op)?; if text.is_empty() { continue } /* stack.as_slice() */ }
// presenter_rs src/pptx.rs (lift): fn escape(text: &str) -> String (l. 1860); fn clean(text: &str) -> String; fn is_xml_char(c: char) -> bool;
//   fn single_line(text: &str) -> String; XML_HEAD, NS_R, REL_IMAGE, REL_HYPERLINK, EMU_PER_INCH = 914_400, EMU_PER_POINT = 12_700;
//   test two_writes_give_the_same_bytes (l. 2688). Not `entries` (stored-only ZIPs): tests read parts with
//   ZipArchive::new(Cursor::new(&bytes[..])) + by_name(..)/by_index(..) + read_to_end.
// units: 1 in = 914 400 EMU = 1 440 twips; 1 twip = 635 EMU; 1 pt = 12 700 EMU = 20 twips; w:sz = half-points; border w:sz = 1/8 pt.
```

## Fixtures (PLAN §7)
- T04: `tests/fixtures/docx/reference.docx`, assembled from hand-written XML by the `#[ignore]`d
  test `regenerate_reference_fixture` (`tests/docx_reference.rs`; a normal test asserts the file
  equals its output): styles with heading 1 colour `FF0000` and a redefined `Callout`; a
  paragraph-level sectPr (ignored) and a body-level A5 sectPr referencing `header1.xml` ("REFERENCE
  HEADER") as `rId100`; settings with `w:updateFields w:val="false"`, `w:rsids`, `w:compat`,
  `w:footnotePr`, `w14:docId`; numbering with abstractNum/num 5 (kept) and 991/1000 (dropped);
  `theme/theme1.xml`, `fontTable.xml`, `webSettings.xml`, `docProps/app.xml`. Unit tests per module;
  `validate.rs` has one failing input per rule (V02: per sub-check); `tests/common/docx.rs` (§7.2).
- Hand-written resolved-IR inputs (no frontend needed) under `tests/fixtures/docx/ir/`: T40 owns
  `{blocks,lists,code,callouts,footnotes,refs,cells}.json` and creates `tests/docx_snapshots.rs`, which
  runs over every `ir/*.json` plus `tests/fixtures/ir/sample-document.json` (T01): every generated
  part pretty-printed (`docx_snapshots__<fixture>__<part>.snap`), validator, read-back; T41 owns
  `{figures,tables,toc,bibliography}.json`, T42 `math.json` (they only add files). The harness
  registers each `tests/fixtures/docx/images/*` with `AssetRegistry::register_bytes` (content-hash
  ids, the same on every machine) and the JSON uses those ids. T41: `tests/fixtures/docx/images/{dpi144.png
  (pHYs 5669 ppm), jfif300.jpg, drawing.svg, anim.gif, pic.webp}` (`pic.webp` made once with
  image-webp 0.2.4's `WebPEncoder` in a throwaway program) and end-to-end builds of
  `tests/fixtures/md/{figures,tables,math,references,footnotes,raw,numbering}.md`.
- T43: `reference.docx` regenerated with a header image and a theme picture fill
  (`word/theme/_rels/theme1.xml.rels` → `../media/…`), tested with a body figure → `validate() == []`
  (§6.2 ids); `reference-localized.docx` (`heading 1` has id `berschrift1`); the showcase snapshot;
  when `pandoc` is installed, its default reference (`pandoc -o <tmp>/ref.docx
  --print-default-data-file reference.docx`, generated at test time, never vendored) as the
  showcase's reference → `validate() == []`.

## Open points (local choices)
- T04: whether `StyleTable` parses `assets/docx/styles.xml` once into a `LazyLock`; message wording.
- T40: run-merging implementation; whether code tokens are computed per block or cached per (lang, text).
- T43: the showcase's sample texts and the grey PNG's bytes.

## Deviations from PLAN.md and the brief
- **Per-section captions** use the chapter number as literal text + `SEQ … \s 1`, not the brief's
  `STYLEREF 1 \s`: that switch returns a heading's *list* number, but writer's heading numbers are
  Section Number text (pandoc's form, also binding), so Word's recomputation (on by default) would
  print an error or nothing. The literal part stays right unless a figure is moved to another chapter;
  where Word's own count would differ (sub-figures, unnumbered or unlisted level-1 headings) the field gets `\r n` (§4.11).
- **Task split** (PLAN §5): bookmarks and internal links (§4.9) are T40's and the numbered-equation
  frame (§4.15) T42's, not T41's; T04 also writes the §1.1 cross-file stubs and the read-back helper
  (§7.2; vendored-assets.md's docx-rs row says "read-back, T43").
- **`m:mathPr`** (§6.4) is Word's own template value (verified), not math.md §5.1's pandoc list
  (`wrapRight`, no `defJc`/`wrapIndent`); math.md §5.1's list and its "not written" sentence are
  superseded, and T42 writes no settings.
- `w:updateFields` is in pandoc's copied-settings list but follows `[docx] update_fields`.
- Copied reference parts = targets of its theme/fontTable/webSettings/header/footer relationships
  (transitively), not pandoc's fixed paths; `app.xml` regenerated, no `custom.xml`; the sectPr is the
  body-level one (pandoc takes the first found, possibly a paragraph-level section break).
- New runtime dependency `imagesize` 0.15.0 (MIT; R3 §6, PLAN §9) for pixel sizes; T00 adds it.
- `docx::xml` lives inline in `src/docx/mod.rs` (PLAN §3.2 has no `xml.rs`); `custom_style.rs`
  (T43's only source file) hosts the eject showcase, while `custom-style` is honoured by T40
  through `StyleTable::custom` (T04). "Callout Title" is a character style so the title shares the box.
