# Spec: Intermediate representation (IR)

**Status:** contract. Owned by T01. Changes require a "contract change" note and an update to
`tests/fixtures/ir/sample-document.json`.

The IR is the only thing frontends (Markdown, notebook, typst) produce and the only thing the
resolve pass and the emitters (HTML, typst, DOCX, LaTeX) consume. It is plain data with `serde`
derives; its JSON form is what `writer dump --ir` prints. It is **structured**: blocks and
inlines, never rendered HTML. Where presenter_rs's IR stored `Block::Html` strings, writer
stores trees, because DOCX, typst and LaTeX need them.

Design rules:
- Pandoc-shaped: anyone who knows pandoc's AST recognises it. Simpler where writer does not
  need pandoc's generality (table cells hold inlines, not blocks).
- Everything an emitter needs to print is in the IR after `resolve` ran: numbers, resolved
  references, formatted citations and bibliography entries, footnote numbers, the TOC.
- Source positions on every block (serve's scroll-to-edit, diagnostics) and on references.
- Math keeps its source dialect; conversions happen in emitters through `crate::math`.

## 1. Types (`src/ir.rs`, authoritative)

```rust
use std::collections::BTreeMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One source file, with everything it includes, produces one `Document`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub meta: DocMeta,
    pub blocks: Vec<Block>,
    /// Which optional pieces the outputs need (KaTeX, highlighting CSS, citation styles, …).
    pub features: Features,
    pub source: SourceInfo,
    /// Summary of registered assets without their bytes (for `dump` and `check`).
    pub assets: Vec<AssetInfo>,
    /// Files pulled in by includes (`:::include`, typst `#include`), transitively, absolute,
    /// in first-use order. Serialized only when non-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<PathBuf>,
    /// Filled by the resolve pass (`docs/spec/references.md`). Empty right after parsing.
    #[serde(default, skip_serializing_if = "Resolved::is_empty")]
    pub resolved: Resolved,
}

/// Content metadata. Configuration-like front-matter keys (`toc`, `numbering`, `bibliography`,
/// `cite_style`, `theme`, …) and the table maps (`markdown:`, `html:`, …) stay in `vars` under
/// their front-matter names, after the frontend's layer merge (typst `writer-doc` arguments
/// after typst-dialect.md §3's `-` → `_` rule). `Config::merge_front_matter(&meta)` reads them
/// from there, in the frontend's config clone and again in the pipeline (§4; config.md §3).
/// Every other key stays in `vars` verbatim.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocMeta {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub authors: Vec<Author>,
    /// As written (`2026-10-24`, `October 2026`); emitters never reformat it.
    pub date: Option<String>,
    /// Parsed with the Markdown dialect (notebook/typst frontends produce the same shape).
    pub abstract_: Option<Vec<Block>>,
    pub keywords: Vec<String>,
    pub description: Option<String>,
    /// BCP-47 tag; emitters fall back to `config.document.lang`, then "en".
    pub lang: Option<String>,
    /// Every other front-matter key, config-like keys and table maps included, verbatim
    /// (templates read `meta.vars.<key>`).
    pub vars: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Author {
    pub name: String,
    pub affiliation: Option<String>,
    pub email: Option<String>,
    /// ORCID or any other identifier, verbatim.
    pub id: Option<String>,
}

/// Pandoc-style attributes `{#id .class key=value}`. `id` doubles as the label for
/// cross-references (`docs/spec/references.md`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Attrs {
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub pairs: BTreeMap<String, String>,
}

pub type Inlines = Vec<Inline>;

/// Ordered content of a document, a list item, a callout, a figure, a footnote, …
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// `level` 1–6. `number` is filled by resolve ("1.2") unless unnumbered (`.unnumbered`
    /// class, or numbering off). `attrs.id` is the section label.
    Heading { level: u8, inlines: Inlines, attrs: Attrs,
              #[serde(default, skip_serializing_if = "Option::is_none")] number: Option<String>,
              #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    Paragraph { inlines: Inlines,
                #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    BlockQuote { blocks: Vec<Block>,
                 #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// `tight` lists render items as plain lines (no paragraph spacing). Task lists are
    /// bullet lists whose items have `checked: Some(_)`.
    List { kind: ListKind, tight: bool, items: Vec<ListItem>,
           #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    DefinitionList { items: Vec<DefinitionItem>,
                     #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// A fenced or indented code block. Caption and label come from the enclosing `Figure`
    /// (kind `Listing`); `attrs` carries `.numbered`, `start=N`, `highlight="1-3"` and the
    /// other code attributes of the dialect (markdown-dialect.md §4.6).
    CodeBlock { lang: Option<String>, text: String, attrs: Attrs,
                #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// A captioned, numbered, referenceable block: an image, a table, a listing, or
    /// anything else (`:::figure`). `attrs.id` is the label; `number` is filled by resolve.
    /// `caption: None` with an id is allowed (numbered, caption-less).
    Figure { kind: FigureKind, caption: Option<Inlines>, body: Vec<Block>, attrs: Attrs,
             #[serde(default, skip_serializing_if = "Option::is_none")] number: Option<String>,
             #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// Cells hold inlines. `head` rows are header rows; `foot` rows are footer rows (empty
    /// from Markdown). A table with a caption or a label is always wrapped in a
    /// `Figure { kind: Table }`; a bare `Table` is unnumbered.
    Table { columns: Vec<ColumnSpec>, head: Vec<Row>, body: Vec<Row>, foot: Vec<Row>, attrs: Attrs,
            #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// Display math. `attrs.id` is the label; `number` ("3") is filled by resolve according
    /// to `[numbering] equations`; emitters add the parentheses.
    Math { source: String, dialect: MathDialect, attrs: Attrs,
           #[serde(default, skip_serializing_if = "Option::is_none")] number: Option<String>,
           #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// `kind` is one of `note`, `tip`, `important`, `warning`, `caution` or a custom word;
    /// `title: None` means the emitter uses the kind's default title.
    Callout { kind: String, title: Option<Inlines>, blocks: Vec<Block>, attrs: Attrs,
              #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// A generic container (`:::name`, typst `block`/`align`); HTML emits a `<div>`, DOCX
    /// applies `custom-style`, typst and LaTeX emit the children (classes may map to
    /// environments, latex-output.md §4).
    Div { attrs: Attrs, blocks: Vec<Block>,
          #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// Passthrough for one target: `format` is `html`, `latex`, `typst`, `openxml` or
    /// `markdown`. Every emitter prints only its own format(s) (html emitter: `html`;
    /// latex: `latex`; typst: `typst`; docx: `openxml`) and drops the rest silently.
    Raw { format: String, text: String,
          #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    ThematicBreak { #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    PageBreak { #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// Where the table of contents goes (`:::toc`, typst `#outline()`, or inserted after the
    /// title block by the frontend when `document.toc` is set and no explicit block exists).
    Toc { depth: u8,
          #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// Where the bibliography goes (`:::bibliography`, typst `#bibliography(..)`, or appended
    /// by resolve when citations exist and no explicit block does).
    Bibliography { #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// A notebook code cell with its outputs (notebook-mapping.md).
    Cell(Cell),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FigureKind { Image, Table, Listing, Other }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListKind { Bullet, Ordered { start: u64 } }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListItem {
    /// `Some(true)`/`Some(false)` for task items, `None` otherwise.
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinitionItem { pub term: Inlines, pub definitions: Vec<Vec<Block>> }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColumnSpec {
    pub align: Align,
    /// Relative width (fraction of the text width), when the source gave one.
    pub width: Option<f32>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align { #[default] Default, Left, Center, Right }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row { pub cells: Vec<TableCell> }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableCell {
    pub inlines: Inlines,
    /// `Some` overrides the column alignment.
    pub align: Option<Align>,
    pub colspan: u32,   // ≥ 1
    pub rowspan: u32,   // ≥ 1
    /// A header cell (`<th>`), also possible inside body rows (DataFrame row labels).
    pub header: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathDialect { Latex, Typst }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inline {
    Text { text: String },
    Emph { inlines: Inlines },
    Strong { inlines: Inlines },
    Strikeout { inlines: Inlines },
    Subscript { inlines: Inlines },
    Superscript { inlines: Inlines },
    Underline { inlines: Inlines },
    SmallCaps { inlines: Inlines },
    Code { text: String, attrs: Attrs },
    /// Inline math.
    Math { source: String, dialect: MathDialect },
    /// `url` is as written for remote links, `#label` for internal ones.
    Link { inlines: Inlines, url: String, title: Option<String>, attrs: Attrs },
    /// An inline image (a lone image in a paragraph became a `Figure`). `url` uses the
    /// `asset:<id>` scheme for local files (§3); `attrs.pairs` may hold `width`/`height`.
    Image { alt: Inlines, url: String, title: Option<String>, attrs: Attrs },
    /// A footnote, with its content inlined at the place of its first reference.
    /// `label` is the source name (`[^note]`, typst `<fn-x>`) when it had one; `number` is
    /// filled by resolve (1-based, document order).
    Footnote { blocks: Vec<Block>, label: Option<String>,
               #[serde(default, skip_serializing_if = "Option::is_none")] number: Option<u32> },
    /// A second reference to an existing footnote (`[^note]` used twice, typst `@fn-x`).
    FootnoteRef { label: String,
                  #[serde(default, skip_serializing_if = "Option::is_none")] number: Option<u32> },
    /// A cross-reference to a labelled element (`@fig-plot`, `@fig-plot[Fig.]`).
    /// `resolved` is filled by the resolve pass; emitters print `resolved.text`.
    Ref { target: String, supplement: Option<Inlines>, form: RefForm,
          #[serde(default, skip_serializing_if = "Option::is_none")] resolved: Option<ResolvedRef>,
          #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    /// One citation group (`[@a; @b, p. 12]`, `@key`, typst `@key[p. 12]`). `resolved` is
    /// filled by resolve when a bibliography is configured (hayagriva output as inlines).
    Cite { items: Vec<CiteItem>, mode: CiteMode,
           #[serde(default, skip_serializing_if = "Option::is_none")] resolved: Option<ResolvedCite>,
           #[serde(default, skip_serializing_if = "Option::is_none")] span: Option<SourceSpan> },
    LineBreak,
    /// A soft line break in the source; emitters print a space (or a newline in verbatim
    /// contexts). Kept so that `dump` round-trips and typst/LaTeX output stays readable.
    SoftBreak,
    Span { attrs: Attrs, inlines: Inlines },
    /// Inline passthrough, same `format` values as `Block::Raw`.
    Raw { format: String, text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefForm { #[default] Normal, Page }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CiteItem {
    pub key: String,
    /// Free text, e.g. "p. 12", "chap. 3" (passed to hayagriva as `Locator::Custom`).
    pub locator: Option<String>,
    pub prefix: Option<Inlines>,
    pub suffix: Option<Inlines>,
}

/// Mirrors typst's `cite(form:)`: `normal` "(Doe 2020)" / "[1]", `prose` "Doe (2020)",
/// `year` "(2020)", `author` "Doe", `full` the bibliography entry, `none` hidden (entry
/// still listed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiteMode { #[default] Normal, Prose, Year, Author, Full, None }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRef {
    pub kind: LabelKind,
    /// "1", "1.2", "3"; headings use their heading number.
    pub number: String,
    /// What the emitters print, e.g. `[Text "Figure\u{a0}1"]` (supplement, U+00A0, number;
    /// references.md §4.5) or the explicit supplement + number.
    pub text: Inlines,
    /// The id of the element to link to (HTML anchor, DOCX bookmark, typst label).
    pub anchor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedCite {
    /// The citation as formatted inlines (Text, Emph, Strong, SmallCaps, Superscript, Link…).
    pub inlines: Inlines,
    /// Informational: `true` for note-class styles (chicago-notes), whose `inlines` already
    /// hold the generated `Footnote`s (references.md §5.5). Emitters print `inlines` as they
    /// are and never make a footnote for a citation.
    pub note: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelKind { Heading, Figure, Table, Listing, Equation, Footnote, Other }

/// Results of the resolve pass (`docs/spec/references.md`). Serialized in `dump --ir`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Resolved {
    /// Every label in the document, keyed by id.
    pub labels: BTreeMap<String, LabelInfo>,
    /// Headings in document order, with numbers, for `Block::Toc` and the HTML sidebar.
    pub toc: Vec<TocEntry>,
    /// Present when citations exist and a bibliography is configured.
    pub bibliography: Option<Bibliography>,
    /// Number of footnotes (after numbering).
    pub footnotes: u32,
}
impl Resolved { pub fn is_empty(&self) -> bool { self.labels.is_empty() && self.toc.is_empty() && self.bibliography.is_none() && self.footnotes == 0 } }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelInfo {
    pub kind: LabelKind,
    /// `None` for an unnumbered element (a heading with numbering off); referencing it is a
    /// diagnostic, except headings, which resolve to their text.
    pub number: Option<String>,
    /// Localized word used before the number ("Figure", "Table", "Listing", "Equation",
    /// "Section"), from `config.document.lang`.
    pub supplement: String,
    /// Caption or heading text, plain, for tooltips and the DOCX `REF` cache.
    pub text: Option<String>,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TocEntry { pub level: u8, pub number: Option<String>, pub inlines: Inlines, pub anchor: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bibliography {
    /// The style actually used (name or path), for templates and `check`.
    pub style: String,
    pub hanging_indent: bool,
    /// In the style's order (numeric styles: citation order; author-year: alphabetical).
    pub entries: Vec<BibEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BibEntry {
    pub key: String,
    /// IEEE-style number "[1]" when the style has one; printed in a left margin.
    pub prefix: Option<Inlines>,
    pub inlines: Inlines,
}

/// A notebook code cell (notebook-mapping.md). Markdown cells become ordinary blocks.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cell {
    /// 0-based index in the notebook, counting every cell.
    pub index: usize,
    pub id: Option<String>,
    /// Language for highlighting (`python`, …), from the notebook metadata.
    pub language: String,
    /// Source without the `#|` option lines.
    pub source: String,
    pub execution_count: Option<i64>,
    /// `false` when `echo: false` / `hide-input` / `remove-input`.
    pub show_input: bool,
    /// `false` when `output: false` / `hide-output` / `remove-output`.
    pub show_output: bool,
    /// Already lowered to blocks; see `CellOutput`.
    pub outputs: Vec<CellOutput>,
    /// Classes from `#| classes:` and tags; `attrs.id` from `#| label:` only when the label
    /// targets no captionable output and the outputs are shown (notebook-mapping.md §3.4;
    /// several captioned outputs are grouped in a `Figure { kind: Other }` instead).
    pub attrs: Attrs,
    pub tags: Vec<String>,
    /// `cell.metadata` verbatim (`Value::Null` if absent).
    pub metadata: Value,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CellOutput {
    pub kind: OutputKind,
    /// `Some` only for `execute_result`.
    pub execution_count: Option<i64>,
    /// Alternative renderings of this output in priority order (notebook-mapping.md §4).
    /// An emitter takes the FIRST candidate it can render: a candidate whose blocks are
    /// `Raw` of a format the emitter does not print is skipped. Frontends guarantee the last
    /// candidate is renderable everywhere (text, or an empty list) — see `CellOutput::pick`.
    pub candidates: Vec<OutputCandidate>,
    /// MIME types the bundle offered, for diagnostics and `check`.
    pub mimes: Vec<String>,
}
impl Default for CellOutput { /* kind: Display, execution_count: None, candidates: [], mimes: [] */ }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputKind { Stream { name: String }, Error { ename: String, evalue: String }, Display, Result }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputCandidate {
    /// The MIME type this rendering came from (`text/html`, `image/png`, `text/plain`, …;
    /// `text/x-ansi` for streams and tracebacks).
    pub mime: String,
    /// `Figure`, `Table` (inside a `Figure` when captioned), `Math`, `CodeBlock`, `Raw`,
    /// `Paragraph`/`Div` for `text/markdown`, …
    pub blocks: Vec<Block>,
}

impl CellOutput {
    /// The blocks an emitter for `target` renders: the first candidate without `Raw`
    /// blocks of a foreign format. `target` is one of "html", "typst", "docx", "latex".
    pub fn pick(&self, target: &str) -> &[Block];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Features {
    /// Any math inline/block or `text/latex` output → KaTeX/MathML CSS, `mitex` package.
    pub math: bool,
    /// Any code block or cell → highlighting stylesheet.
    pub code: bool,
    /// Source was a notebook or any `Block::Cell` exists → cell styles.
    pub notebook: bool,
    /// Any `Inline::Cite` → citation styles, bibliography block.
    pub citations: bool,
    /// Any footnote.
    pub footnotes: bool,
}
impl Features { pub fn merge(&mut self, other: &Features); }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Absolute path of the source file.
    pub path: PathBuf,
    pub kind: SourceKind,
    /// Directory relative paths (images, includes, bibliographies) resolve against.
    pub base_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind { Markdown, Notebook, Typst }

/// 1-based inclusive line range. For notebooks, lines are within the cell's source and
/// `cell` is the cell index; otherwise `cell` is `None`. `file` is set when the lines belong
/// to an included file (absolute).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start_line: usize,
    pub end_line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub cell: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetInfo {
    pub id: String,
    pub mime: String,
    /// `"file:<path relative to base_dir>"` or `"inline:<byte length>"`.
    pub origin: String,
}
```

> **Contract change (coordinator, 2026-10-24):** four comments above changed; T01 writes the new
> wording into `src/ir.rs`'s doc comments. `CodeBlock` cites markdown-dialect.md §4.6 (the code
> attribute table), not §5. `ResolvedRef.text` uses U+00A0 between supplement and number
> (references.md §4.5, Deviation 7). `ResolvedCite.note` is informational: resolve puts the
> note-style footnotes into `inlines` and no emitter makes a footnote for a citation
> (references.md §5.5 and Deviation 3; html-output.md §1.12, docx-output.md §4.9).
> `Cell.attrs.id` carries `#| label` only for a label with no captionable output and shown
> outputs; several captioned outputs get a `Figure { kind: Other }` container
> (notebook-mapping.md §3.4 and Deviations).

> **Contract change (coordinator, 2026-10-25):** the `DocMeta` doc comment said config-like
> front-matter keys do not live in `DocMeta`; they do. config.md §3, markdown-dialect.md §2.2,
> typst-dialect.md §3 and §4 below keep them and the table maps in `DocMeta.vars`, because the
> pipeline's second `Config::merge_front_matter(&document.meta)` can only see them there
> (`Parsed` carries no config). T01 writes the new wording into `src/ir.rs`. Every frontend
> keeps them in `vars`, including T18's `metadata.writer` layer (notebook-mapping.md §2.2
> layer 3: config keys and tables go to `vars` too, not only unknown keys). No PLAN §5
> dependency changes.

Helpers T01 provides (used by every emitter): `Block::span() -> Option<&SourceSpan>`,
`Inlines` → plain text (`ir::plain_text(&Inlines) -> String`: text of Text/Code/Math sources,
breaks as one space, `Ref`/`Cite` as their resolved text or `[?]`), `ir::walk_blocks(&Document,
&mut dyn FnMut(&Block))` and `walk_blocks_mut`, `walk_inlines`, and constructors
`Block::paragraph(Inlines)`, `Inline::text(&str)`, `Attrs::with_id(&str)`.

## 2. Invariants (tests in T01 enforce the checkable ones)

1. Headings' `level` is 1–6; `ListItem.blocks` of a tight list contain only `Paragraph`,
   nested `List` and `CodeBlock`; `TableCell.colspan/rowspan ≥ 1`; every `Row` of a `Table`
   covers exactly `columns.len()` grid slots, counting its cells' `colspan` plus the slots
   covered by `rowspan`s of earlier rows of the same section (head, body, foot); covered slots
   have no `TableCell` (the HTML model). T01's invariant checker implements this grid rule.

   > **Contract change (coordinator, 2026-10-24):** the row rule counted only `colspan`, so a
   > row had to list a cell for every column. It now follows the grid model that
   > notebook-mapping.md (§4.4 and Deviations), html-output.md §1.7, docx-output.md §4.12 and
   > latex-output.md §4.6 use, in which a pandas MultiIndex frame's covered slots are absent
   > from later rows.
2. A `Table` with a caption or an id is wrapped in a `Figure { kind: Table }` whose `body` is
   exactly `[Table]`; a `Figure { kind: Image }` has a `body` of exactly one `Paragraph` whose
   only inline is an `Image`; a `Figure { kind: Listing }` wraps exactly one `CodeBlock`.
   `Figure { kind: Other }` may hold anything. Figures do not nest except inside
   `Figure { kind: Other }` (sub-figures).
3. All URLs of `Inline::Image` and image-bearing `Raw` HTML that point at local files use the
   `asset:<id>` scheme (§3). Remote URLs (`http:`, `https:`, `//`, `data:`, `mailto:`) and
   root-absolute paths (`/x.png`) stay as written. Exceptions as in presenter: a missing file
   keeps its URL and raises `missing-asset`; notebook outputs embed only files inside the
   notebook's directory (`output-file-outside`).
4. `Inline::Footnote` appears once per footnote (its first reference); later references are
   `FootnoteRef`. After resolve, every `Footnote.number` is `Some` and numbers are 1..=n in
   document order; `FootnoteRef.number` equals its footnote's.
5. `Inline::Ref.resolved` and `Inline::Cite.resolved` are `None` after parsing. After resolve
   every `Ref` and `Cite` left in the document has `resolved: Some(_)`: a reference or citation
   that cannot be resolved (unknown target or key, no bibliography configured, ambiguous label,
   mixed group) is replaced, in every mode and with a diagnostic, by the placeholder `Span`
   (references.md §4.7). `Block::Heading/Figure/Math.number` are `None` after parsing and
   `Some` after resolve, where numbered.

   > **Contract change (coordinator, 2026-10-24):** a `Cite` no longer stays `resolved: None`
   > without a bibliography; references.md §4.7 and §5.7 replace every failing `Ref`/`Cite`
   > with the `[?…]` placeholder `Span`. Emitters handle `resolved: None` only for unresolved
   > IR (html-output.md §1.12, docx-output.md §4.9).
6. `Features` is derived from content, never set by configuration.
7. JSON serialization is stable: `cargo insta` snapshots of fixtures must not change when
   unrelated code changes. `BTreeMap` only, never `HashMap`.
8. Line numbers in `SourceSpan` are 1-based. Every `Block` produced by a frontend has
   `span: Some(_)` except the `Toc` a frontend inserts at index 0 for `[document] toc`
   (markdown-dialect.md §2.1, notebook-mapping.md §2.1 step 7). Blocks the resolve pass
   inserts have `span: None` too: `Toc`, `Bibliography` (references.md §4.2, §4.9) and the
   `Paragraph` of a note-citation footnote (references.md §5.5).

   > **Contract change (coordinator, 2026-10-24):** the exception named only resolve-inserted
   > blocks, but the Markdown and notebook frontends insert `Toc { span: None }` themselves
   > (as §1's `Toc` comment says); the exception now covers that `Toc`, and the list of
   > resolve-inserted blocks includes the note-citation `Paragraph`.
9. Ids (`Attrs.id`, labels) are non-empty, contain no whitespace, and are unique per document;
   duplicates are a resolve-time error (`duplicate-label`).
10. `Block::Raw.format` and `Inline::Raw.format` are lowercase; unknown formats are kept (an
    emitter that does not know them drops them).

## 3. Asset placeholders and the registry (`src/assets/registry.rs`, lifted from presenter_rs)

Frontends never read image bytes or decide URLs. They *register* assets and write a
placeholder; emitters resolve them: the HTML emitter through `assets::emit` (`data:` URIs in
standalone mode, files in site mode, `/__writer/doc-asset/<id>` in serve), the typst emitter
through the compiler's virtual file system (`/__assets/<id>.<ext>`, typst-dialect.md §6), the
DOCX emitter into `word/media/`, the LaTeX emitter into `<stem>_assets/`.

```rust
pub struct AssetRegistry { /* BTreeMap<String, Asset> */ }
pub struct Asset { pub id: String /* 12 lowercase hex */, pub mime: String, pub data: AssetData, pub file_name_hint: String }
pub enum AssetData { File(PathBuf) /* absolute */, Bytes(Vec<u8>) }
impl AssetRegistry {
    pub fn new() -> Self;
    pub fn register_file(&mut self, abs_path: &Path) -> String;     // id = blake3(path)[..12]; MIME from extension; does not read
    pub fn register_bytes(&mut self, bytes: Vec<u8>, mime: &str, file_name_hint: &str) -> String;  // id = blake3(bytes)[..12]
    pub fn get(&self, id: &str) -> Option<&Asset>;
    pub fn read(&self, id: &str) -> Result<Vec<u8>>;                // reads File data on demand
    pub fn iter(&self) -> impl Iterator<Item = &Asset>;
    pub fn merge(&mut self, other: AssetRegistry);                  // includes
    pub fn infos(&self, base_dir: &Path) -> Vec<AssetInfo>;
    pub fn placeholder(id: &str) -> String { format!("asset:{id}") }
}
```

The API, placeholder grammar (`asset:` + exactly 12 `[0-9a-f]`) and determinism rules are
presenter's (its ir.md §3): in site mode emitted file names hash the **bytes**; text content
is never rewritten, only attribute values; unresolvable placeholders are a build error.

> **Contract change (coordinator, 2026-10-26):** an asset URL may append an author's query
> and/or fragment to the 12-hex placeholder, as markdown-dialect.md §4.3 and T14's emitter
> require. The invariant checker validates the id before `?`/`#`; it must not reject those
> suffixes. Placeholder ids and IR shapes are unchanged.

## 4. Frontend and pass interfaces

```rust
/// Output of any frontend. Defined in `src/ir.rs` (T01).
pub struct Parsed { pub document: Document, pub assets: AssetRegistry, pub diagnostics: Diagnostics }

// src/markdown/mod.rs (T12)
pub fn parse(source: &SourceInfo, text: &str, config: &Config) -> crate::error::Result<Parsed>;
// src/notebook/mod.rs (T18)
pub fn parse(source: &SourceInfo, json: &str, config: &Config) -> crate::error::Result<Parsed>;
// src/typst/frontend.rs (T50)
pub fn parse(source: &SourceInfo, text: &str, config: &Config) -> crate::error::Result<Parsed>;

// src/resolve/mod.rs (T17; citations T70)
pub struct ResolveOptions<'a> { pub config: &'a Config, pub library: Option<&'a cite::Library>, pub lenient: bool }
/// Numbers, references, footnotes, TOC, citations. Never fails; problems are diagnostics
/// (`unresolved-reference`, `duplicate-label`, `ambiguous-label`, `unnumbered-target`).
/// In every mode a failing ref or cite becomes the `[?target]` placeholder
/// (references.md §4.7); `lenient` (serve, check) only makes `unresolved-reference`
/// a warning instead of an error.
pub fn resolve(document: &mut Document, opts: &ResolveOptions) -> Diagnostics;
```

> **Contract change (coordinator, 2026-10-26):** resolve follows references.md §8:
> targets that are neither labels nor library keys raise `unresolved-reference`.
> `unknown-citation-key` is not raised by this pass.

> **Contract change (coordinator, 2026-10-24):** `markdown::parse` is owned by T12 (stub: T01,
> PLAN §3.2), not T11. The `resolve` comment follows references.md §4.1 and §4.7: placeholders
> in every mode, `lenient` for both serve and check, and only the diagnostic level changes.

T01 creates every frontend entry point above as a **stub with its final signature** that
returns `Error::Other("<kind> support arrives in Txx")`; the owning task (T12, T18, T50)
replaces the body only. T01 also provides `ir::detect_kind(path: &Path) -> Result<SourceKind>`
(by extension; lifted from presenter's `pipeline::detect_kind`) so includes (T1B) and the
pipeline (T16) dispatch the same way.

Frontends are pure functions of their inputs except that they may `stat` referenced files to
decide between registering them and reporting `missing-asset`, and read included sources.
Frontends never fail on a missing asset (warning `missing-asset`; `build` escalates it, `serve`
and `check` do not), exactly as in presenter. Front matter: frontends apply
`Config::merge_front_matter(&meta)` to a clone of their config; the pipeline applies it again
(idempotent).

## 5. Error type (`src/error.rs`, lifted from presenter_rs, owned by T01)

```rust
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{}:{} {message}", .path.display(), .line.map(|l| l.to_string()).unwrap_or_default())]
    Parse { path: PathBuf, line: Option<usize>, message: String },
    #[error("front matter: {0}")] FrontMatter(String),
    #[error("config: {0}")] Config(String),
    #[error("theme '{name}': {message}")] Theme { name: String, message: String },
    #[error("template '{template}'{}: {message}", .line.map(|l| format!(" line {l}")).unwrap_or_default())]
    Template { template: String, line: Option<usize>, message: String },
    #[error("style '{file}': {message}")] Style { file: String, message: String },
    #[error("asset not found: {}", .path.display())] MissingAsset { path: PathBuf, span: Option<SourceSpan> },
    /// typst compile errors, already mapped to the user's source where a source map exists.
    #[error("{}:{} {message}", .path.display(), .line.map(|l| l.to_string()).unwrap_or_default())]
    Compile { path: PathBuf, line: Option<usize>, message: String, hints: Vec<String> },
    /// A reference document, LaTeX template or typst template that cannot be used.
    #[error("{what} '{}': {message}", .path.display())] Resource { what: &'static str, path: PathBuf, message: String },
    #[error(transparent)] Io(#[from] std::io::Error),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error("{0}")] Other(String),
}
pub type Result<T> = std::result::Result<T, Error>;

/// `Info` is new in writer (presenter has only `Warning` and `Error`): compatibility and
/// rewrite notes such as `directive-colon-rewritten`, `presenter-separator`, `raw-skipped`.
/// `build` and `serve` print them only with `-v`; `check` lists them under "notes" and
/// `--compat presenter` promotes the compatibility ones to warnings; `--deny-warnings`
/// ignores them.
pub enum Level { Info, Warning, Error }
pub struct Diagnostic { pub level: Level, pub code: &'static str, pub message: String, pub path: Option<PathBuf>, pub span: Option<SourceSpan> }
pub struct Diagnostics(Vec<Diagnostic>);   // push, info, warn, error, iter, has_errors, has_warnings, len, is_empty, extend, into_vec, IntoIterator
```

`From<minijinja::Error>` and `From<toml::de::Error>` live here too (presenter's versions).
`cli/report.rs` (lifted) renders errors and diagnostics with rustc-style excerpts.

## 6. JSON example (`tests/fixtures/ir/sample-document.json` is the canonical, larger version)

> **Contract change (coordinator, 2026-10-24):** resolved ref texts are supplement + U+00A0 +
> number (references.md §4.5 and Deviation 7), so the sample's ref text is `"Figure 1"`
> (written as a JSON escape here; serde writes the character itself). T01 writes
> `sample-document.json` the same way.

```json
{
  "meta": { "title": "GPU allocation", "authors": [{ "name": "J. Blaschke" }], "date": "2026-10-24",
            "abstract_": [{ "type": "paragraph", "inlines": [{ "type": "text", "text": "One paragraph." }] }],
            "lang": "en", "vars": { "venue": "internal" } },
  "blocks": [
    { "type": "heading", "level": 1, "inlines": [{ "type": "text", "text": "Introduction" }],
      "attrs": { "id": "sec-intro" }, "number": "1", "span": { "start_line": 9, "end_line": 9 } },
    { "type": "paragraph", "inlines": [
        { "type": "text", "text": "Demand doubled " },
        { "type": "cite", "items": [{ "key": "knuth1984" }], "mode": "normal",
          "resolved": { "inlines": [{ "type": "text", "text": "[1]" }], "note": false } },
        { "type": "text", "text": ", see " },
        { "type": "ref", "target": "fig-throughput", "supplement": null, "form": "normal",
          "resolved": { "kind": "figure", "number": "1", "text": [{ "type": "text", "text": "Figure 1" }], "anchor": "fig-throughput" } },
        { "type": "text", "text": "." } ],
      "span": { "start_line": 11, "end_line": 12 } },
    { "type": "figure", "kind": "image", "caption": [{ "type": "text", "text": "Throughput per node" }],
      "body": [{ "type": "paragraph", "inlines": [{ "type": "image", "alt": [{ "type": "text", "text": "Throughput per node" }],
                 "url": "asset:3f9a1c2b7e0d", "title": null, "attrs": { "pairs": { "width": "70%" } } }] }],
      "attrs": { "id": "fig-throughput" }, "number": "1", "span": { "start_line": 14, "end_line": 14 } },
    { "type": "callout", "kind": "note", "title": [{ "type": "text", "text": "Reproducibility" }],
      "blocks": [{ "type": "paragraph", "inlines": [{ "type": "text", "text": "Every run used the pinned image." }] }],
      "attrs": {}, "span": { "start_line": 22, "end_line": 23 } },
    { "type": "math", "source": "T(n) = \\frac{W}{n} + c", "dialect": "latex", "attrs": { "id": "eq-amdahl" }, "number": "1",
      "span": { "start_line": 30, "end_line": 30 } },
    { "type": "cell", "index": 3, "language": "python", "source": "df.describe()", "execution_count": 2,
      "show_input": true, "show_output": true, "attrs": {}, "tags": [], "metadata": {},
      "outputs": [{ "kind": { "kind": "result" }, "execution_count": 2, "mimes": ["text/html", "text/plain"],
                    "candidates": [
                      { "mime": "text/html", "blocks": [{ "type": "table", "columns": [{ "align": "default" }, { "align": "right" }],
                          "head": [{ "cells": [{ "inlines": [], "header": true, "colspan": 1, "rowspan": 1 }, { "inlines": [{ "type": "text", "text": "GB/s" }], "header": true, "colspan": 1, "rowspan": 1 }] }],
                          "body": [{ "cells": [{ "inlines": [{ "type": "text", "text": "0" }], "header": true, "colspan": 1, "rowspan": 1 }, { "inlines": [{ "type": "text", "text": "12.3" }], "colspan": 1, "rowspan": 1 }] }],
                          "foot": [], "attrs": {} }] },
                      { "mime": "text/plain", "blocks": [{ "type": "code_block", "lang": null, "text": "   GB/s\n0  12.3", "attrs": { "classes": ["output-text"] } }] } ] }],
      "span": { "start_line": 1, "end_line": 1, "cell": 3 } },
    { "type": "bibliography" }
  ],
  "features": { "math": true, "code": true, "notebook": true, "citations": true, "footnotes": false },
  "source": { "path": "/abs/report/document.md", "kind": "markdown", "base_dir": "/abs/report" },
  "assets": [{ "id": "3f9a1c2b7e0d", "mime": "image/png", "origin": "file:figures/plot.png" }],
  "resolved": {
    "labels": { "sec-intro": { "kind": "heading", "number": "1", "supplement": "Section", "text": "Introduction", "span": { "start_line": 9, "end_line": 9 } },
                "fig-throughput": { "kind": "figure", "number": "1", "supplement": "Figure", "text": "Throughput per node", "span": { "start_line": 14, "end_line": 14 } },
                "eq-amdahl": { "kind": "equation", "number": "1", "supplement": "Equation", "text": null, "span": { "start_line": 30, "end_line": 30 } } },
    "toc": [{ "level": 1, "number": "1", "inlines": [{ "type": "text", "text": "Introduction" }], "anchor": "sec-intro" }],
    "bibliography": { "style": "ieee", "hanging_indent": false,
                      "entries": [{ "key": "knuth1984", "prefix": [{ "type": "text", "text": "[1]" }],
                                    "inlines": [{ "type": "text", "text": "D. E. Knuth, " }, { "type": "emph", "inlines": [{ "type": "text", "text": "The TeXbook" }] }, { "type": "text", "text": ". Addison-Wesley, 1984." }] }] },
    "footnotes": 0
  }
}
```
