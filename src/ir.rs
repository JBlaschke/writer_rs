//! Shared document intermediate representation (ir.md §1–§2).
//! Owner: T01. Stub created by T00.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::assets::registry::AssetRegistry;
use crate::error::{Diagnostics, Error, Result};

/// One source file, with everything it includes, produces one `Document`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Content metadata and free-form front-matter variables.
    pub meta: DocMeta,
    /// Nested blocks in source order.
    pub blocks: Vec<Block>,
    /// Which optional pieces the outputs need (KaTeX, highlighting CSS,
    /// citation styles, …).
    pub features: Features,
    /// Source identity or verbatim source content.
    pub source: SourceInfo,
    /// Summary of registered assets without their bytes (for `dump` and
    /// `check`).
    pub assets: Vec<AssetInfo>,
    /// Files pulled in by includes (`:::include`, typst `#include`),
    /// transitively, absolute, in first-use order. Serialized only when
    /// non-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<PathBuf>,
    /// Filled by the resolve pass (`docs/spec/references.md`). Empty right
    /// after parsing.
    #[serde(default, skip_serializing_if = "Resolved::is_empty")]
    pub resolved: Resolved,
}

/// Content metadata. Configuration-like front-matter keys (`toc`, `numbering`,
/// `bibliography`, `cite_style`, `theme`, …) and the table maps (`markdown:`,
/// `html:`, …) stay in `vars` under their front-matter names, after the
/// frontend's layer merge (typst `writer-doc` arguments after typst-dialect.md
/// §3's `-` → `_` rule). `Config::merge_front_matter(&meta)` reads them from
/// there, in the frontend's config clone and again in the pipeline (§4;
/// config.md §3). Every other key stays in `vars` verbatim.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocMeta {
    /// The optional title as written in the source.
    pub title: Option<String>,
    /// The optional subtitle as written in the source.
    pub subtitle: Option<String>,
    /// Authors in their source order.
    pub authors: Vec<Author>,
    /// As written (`2026-10-24`, `October 2026`); emitters never reformat it.
    pub date: Option<String>,
    /// Parsed with the Markdown dialect (notebook/typst frontends produce the
    /// same shape).
    pub abstract_: Option<Vec<Block>>,
    /// Content keywords in source order.
    pub keywords: Vec<String>,
    /// An optional plain-text content description.
    pub description: Option<String>,
    /// BCP-47 tag; emitters fall back to `config.document.lang`, then "en".
    pub lang: Option<String>,
    /// Every other front-matter key, config-like keys and table maps included,
    /// verbatim (templates read `meta.vars.<key>`).
    pub vars: Map<String, Value>,
}

/// An author and optional affiliation or contact details.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Author {
    /// The source name.
    pub name: String,
    /// An optional institution or organizational affiliation.
    pub affiliation: Option<String>,
    /// An optional contact email address.
    pub email: Option<String>,
    /// ORCID or any other identifier, verbatim.
    pub id: Option<String>,
}

/// Pandoc-style attributes `{#id .class key=value}`. `id` doubles as the label
/// for cross-references (`docs/spec/references.md`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Attrs {
    /// The optional source identifier.
    pub id: Option<String>,
    /// Classes in their source order.
    pub classes: Vec<String>,
    /// Named attributes, sorted by key.
    pub pairs: BTreeMap<String, String>,
}

/// Inline content in source order.
pub type Inlines = Vec<Inline>;

/// Ordered content of a document, a list item, a callout, a figure, a footnote,
/// …
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// `level` 1–6. `number` is filled by resolve ("1.2") unless unnumbered
    /// (`.unnumbered` class, or numbering off). `attrs.id` is the section
    /// label.
    Heading {
        /// Heading depth, from 1 through 6.
        level: u8,
        /// Inline content in source order.
        inlines: Inlines,
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The number assigned by reference resolution.
        number: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// A paragraph of inline content.
    Paragraph {
        /// Inline content in source order.
        inlines: Inlines,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// A quoted sequence of blocks.
    BlockQuote {
        /// Nested blocks in source order.
        blocks: Vec<Block>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// `tight` lists render items as plain lines (no paragraph spacing). Task
    /// lists are bullet lists whose items have `checked: Some(_)`.
    List {
        /// The content or numbering category.
        kind: ListKind,
        /// Whether item paragraphs omit paragraph spacing.
        tight: bool,
        /// Items in source order.
        items: Vec<ListItem>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// Terms with one or more definitions.
    DefinitionList {
        /// Items in source order.
        items: Vec<DefinitionItem>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// A fenced or indented code block. Caption and label come from the
    /// enclosing `Figure` (kind `Listing`); `attrs` carries `.numbered`,
    /// `start=N`, `highlight="1-3"` and the other code attributes of the
    /// dialect (markdown-dialect.md §4.6).
    CodeBlock {
        /// An optional code language for highlighting.
        lang: Option<String>,
        /// Verbatim text content.
        text: String,
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// A captioned, numbered, referenceable block: an image, a table, a
    /// listing, or anything else (`:::figure`). `attrs.id` is the label;
    /// `number` is filled by resolve. `caption: None` with an id is allowed
    /// (numbered, caption-less).
    Figure {
        /// The content or numbering category.
        kind: FigureKind,
        /// The optional figure caption.
        caption: Option<Inlines>,
        /// Body content in source order.
        body: Vec<Block>,
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The number assigned by reference resolution.
        number: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// Cells hold inlines. `head` rows are header rows; `foot` rows are footer
    /// rows (empty from Markdown). A table with a caption or a label is always
    /// wrapped in a `Figure { kind: Table }`; a bare `Table` is unnumbered.
    Table {
        /// Column alignment and width specifications.
        columns: Vec<ColumnSpec>,
        /// Header rows in source order.
        head: Vec<Row>,
        /// Body content in source order.
        body: Vec<Row>,
        /// Footer rows in source order.
        foot: Vec<Row>,
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// Display math. `attrs.id` is the label; `number` ("3") is filled by
    /// resolve according to `[numbering] equations`; emitters add the
    /// parentheses.
    Math {
        /// Source identity or verbatim source content.
        source: String,
        /// The equation source dialect.
        dialect: MathDialect,
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The number assigned by reference resolution.
        number: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// `kind` is one of `note`, `tip`, `important`, `warning`, `caution` or a
    /// custom word; `title: None` means the emitter uses the kind's default
    /// title.
    Callout {
        /// The content or numbering category.
        kind: String,
        /// The optional title as written in the source.
        title: Option<Inlines>,
        /// Nested blocks in source order.
        blocks: Vec<Block>,
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// A generic container (`:::name`, typst `block`/`align`); HTML emits a
    /// `<div>`, DOCX applies `custom-style`, typst and LaTeX emit the children
    /// (classes may map to environments, latex-output.md §4).
    Div {
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
        /// Nested blocks in source order.
        blocks: Vec<Block>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// Passthrough for one target: `format` is `html`, `latex`, `typst`,
    /// `openxml` or `markdown`. Every emitter prints only its own format(s)
    /// (html emitter: `html`; latex: `latex`; typst: `typst`; docx: `openxml`)
    /// and drops the rest silently.
    Raw {
        /// The lowercase target format for passthrough.
        format: String,
        /// Verbatim text content.
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// A thematic separator.
    ThematicBreak {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// An explicit page break.
    PageBreak {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// Where the table of contents goes (`:::toc`, typst `#outline()`, or
    /// inserted after the title block by the frontend when `document.toc` is
    /// set and no explicit block exists).
    Toc {
        /// The maximum heading depth to include.
        depth: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// Where the bibliography goes (`:::bibliography`, typst
    /// `#bibliography(..)`, or appended by resolve when citations exist and no
    /// explicit block does).
    Bibliography {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// A notebook code cell with its outputs (notebook-mapping.md).
    Cell(Cell),
}

/// The caption and body category of a figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FigureKind {
    /// A single image paragraph.
    Image,
    /// A single table.
    Table,
    /// A single code block.
    Listing,
    /// An arbitrary figure body, including subfigures.
    Other,
}

/// Bullet or ordered list numbering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListKind {
    /// An unordered bullet list.
    Bullet,
    /// An ordered list with an explicit starting number.
    Ordered {
        /// The first ordered item number.
        start: u64,
    },
}

/// One list item, optionally marked as a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ListItem {
    /// `Some(true)`/`Some(false)` for task items, `None` otherwise.
    pub checked: Option<bool>,
    /// Nested blocks in source order.
    pub blocks: Vec<Block>,
}

/// A term with one or more block definitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinitionItem {
    /// The term being defined.
    pub term: Inlines,
    /// Alternative definitions, each a sequence of blocks.
    pub definitions: Vec<Vec<Block>>,
}

/// Alignment and optional relative width of a table column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ColumnSpec {
    /// Horizontal alignment, or the column default.
    pub align: Align,
    /// Relative width (fraction of the text width), when the source gave one.
    pub width: Option<f32>,
}
/// Horizontal alignment of table content.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    /// Use the renderer or column default.
    #[default]
    Default,
    /// Align to the left.
    Left,
    /// Center horizontally.
    Center,
    /// Align to the right.
    Right,
}

/// A table row; cells covered by earlier rowspans are absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// Cells in grid order, excluding slots covered by rowspans.
    pub cells: Vec<TableCell>,
}

/// Inline table content and its rectangular grid span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableCell {
    /// Inline content in source order.
    pub inlines: Inlines,
    /// `Some` overrides the column alignment.
    pub align: Option<Align>,
    /// The number of columns occupied, at least 1.
    pub colspan: u32, // ≥ 1
    /// The number of rows occupied, at least 1.
    pub rowspan: u32, // ≥ 1
    /// A header cell (`<th>`), also possible inside body rows (DataFrame row
    /// labels).
    pub header: bool,
}

impl Default for TableCell {
    fn default() -> Self {
        Self {
            inlines: Vec::new(),
            align: None,
            colspan: 1,
            rowspan: 1,
            header: false,
        }
    }
}

/// The source language of an equation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathDialect {
    /// LaTeX equation source.
    Latex,
    /// Typst equation source.
    Typst,
}

/// Structured inline content, including references and footnotes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inline {
    /// Plain text.
    Text {
        /// Verbatim text content.
        text: String,
    },
    /// Emphasized inline content.
    Emph {
        /// Inline content in source order.
        inlines: Inlines,
    },
    /// Strongly emphasized content.
    Strong {
        /// Inline content in source order.
        inlines: Inlines,
    },
    /// Struck-through content.
    Strikeout {
        /// Inline content in source order.
        inlines: Inlines,
    },
    /// Subscript content.
    Subscript {
        /// Inline content in source order.
        inlines: Inlines,
    },
    /// Superscript content.
    Superscript {
        /// Inline content in source order.
        inlines: Inlines,
    },
    /// Underlined content.
    Underline {
        /// Inline content in source order.
        inlines: Inlines,
    },
    /// Small-capital content.
    SmallCaps {
        /// Inline content in source order.
        inlines: Inlines,
    },
    /// Verbatim inline code with attributes.
    Code {
        /// Verbatim text content.
        text: String,
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
    },
    /// Inline math.
    Math {
        /// Source identity or verbatim source content.
        source: String,
        /// The equation source dialect.
        dialect: MathDialect,
    },
    /// `url` is as written for remote links, `#label` for internal ones.
    Link {
        /// Inline content in source order.
        inlines: Inlines,
        /// The source URL or asset placeholder.
        url: String,
        /// The optional title as written in the source.
        title: Option<String>,
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
    },
    /// An inline image (a lone image in a paragraph became a `Figure`). `url`
    /// uses the `asset:<id>` scheme for local files (§3); `attrs.pairs` may
    /// hold `width`/`height`.
    Image {
        /// Alternative text as structured inlines.
        alt: Inlines,
        /// The source URL or asset placeholder.
        url: String,
        /// The optional title as written in the source.
        title: Option<String>,
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
    },
    /// A footnote, with its content inlined at the place of its first
    /// reference. `label` is the source name (`[^note]`, typst `<fn-x>`) when
    /// it had one; `number` is filled by resolve (1-based, document order).
    Footnote {
        /// Nested blocks in source order.
        blocks: Vec<Block>,
        /// The source footnote label.
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The number assigned by reference resolution.
        number: Option<u32>,
    },
    /// A second reference to an existing footnote (`[^note]` used twice, typst
    /// `@fn-x`).
    FootnoteRef {
        /// The source footnote label.
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The number assigned by reference resolution.
        number: Option<u32>,
    },
    /// A cross-reference to a labelled element (`@fig-plot`,
    /// `@fig-plot[Fig.]`). `resolved` is filled by the resolve pass; emitters
    /// print `resolved.text`.
    Ref {
        /// The label being referenced.
        target: String,
        /// An optional explicit reference supplement.
        supplement: Option<Inlines>,
        /// The requested reference form.
        form: RefForm,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The resolved display payload, absent after parsing.
        resolved: Option<ResolvedRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// One citation group (`[@a; @b, p. 12]`, `@key`, typst `@key[p. 12]`).
    /// `resolved` is filled by resolve when a bibliography is configured
    /// (hayagriva output as inlines).
    Cite {
        /// Items in source order.
        items: Vec<CiteItem>,
        /// The requested citation rendering form.
        mode: CiteMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The resolved display payload, absent after parsing.
        resolved: Option<ResolvedCite>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// The source location, when available.
        span: Option<SourceSpan>,
    },
    /// An explicit line break.
    LineBreak,
    /// A soft line break in the source; emitters print a space (or a newline in
    /// verbatim contexts). Kept so that `dump` round-trips and typst/LaTeX
    /// output stays readable.
    SoftBreak,
    /// A generic attributed inline container.
    Span {
        /// Attributes and the optional cross-reference label.
        attrs: Attrs,
        /// Inline content in source order.
        inlines: Inlines,
    },
    /// Inline passthrough, same `format` values as `Block::Raw`.
    Raw {
        /// The lowercase target format for passthrough.
        format: String,
        /// Verbatim text content.
        text: String,
    },
}

/// The reference form requested by the source.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RefForm {
    #[default]
    /// Print the normal supplement and number.
    Normal,
    /// Print the page containing the target.
    Page,
}

/// One cited bibliography key with optional locator and context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CiteItem {
    /// The bibliography entry key.
    pub key: String,
    /// Free text, e.g. "p. 12", "chap. 3" (passed to hayagriva as
    /// `Locator::Custom`).
    pub locator: Option<String>,
    /// Optional content printed before the cited item or entry.
    pub prefix: Option<Inlines>,
    /// Optional content printed after the cited item.
    pub suffix: Option<Inlines>,
}

/// Mirrors typst's `cite(form:)`: `normal` "(Doe 2020)" / `[1]`, `prose` "Doe
/// (2020)", `year` "(2020)", `author` "Doe", `full` the bibliography entry,
/// `none` hidden (entry still listed).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum CiteMode {
    #[default]
    /// Print the style's normal citation.
    Normal,
    /// Print a prose citation.
    Prose,
    /// Print only the year.
    Year,
    /// Print only the author.
    Author,
    /// Print the full bibliography entry.
    Full,
    /// Hide the citation while retaining its bibliography entry.
    None,
}

/// The text and link destination supplied by reference resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRef {
    /// The content or numbering category.
    pub kind: LabelKind,
    /// "1", "1.2", "3"; headings use their heading number.
    pub number: String,
    /// What the emitters print, e.g. `[Text "Figure\u{a0}1"]` (supplement,
    /// U+00A0, number; references.md §4.5) or the explicit supplement + number.
    pub text: Inlines,
    /// The id of the element to link to (HTML anchor, DOCX bookmark, typst
    /// label).
    pub anchor: String,
}

/// A formatted citation produced by bibliography resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedCite {
    /// The citation as formatted inlines (Text, Emph, Strong, SmallCaps,
    /// Superscript, Link…).
    pub inlines: Inlines,
    /// Informational: `true` for note-class styles (chicago-notes), whose
    /// `inlines` already hold the generated `Footnote`s (references.md §5.5).
    /// Emitters print `inlines` as they are and never make a footnote for a
    /// citation.
    pub note: bool,
}

/// The kind of object identified by a document label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelKind {
    /// A document heading.
    Heading,
    /// An image figure.
    Figure,
    /// A table figure.
    Table,
    /// A code listing.
    Listing,
    /// A display equation.
    Equation,
    /// A footnote.
    Footnote,
    /// Another labelled container.
    Other,
}

/// Results of the resolve pass (`docs/spec/references.md`). Serialized in `dump
/// --ir`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Resolved {
    /// Every label in the document, keyed by id.
    pub labels: BTreeMap<String, LabelInfo>,
    /// Headings in document order, with numbers, for `Block::Toc` and the HTML
    /// sidebar.
    pub toc: Vec<TocEntry>,
    /// Present when citations exist and a bibliography is configured.
    pub bibliography: Option<Bibliography>,
    /// Number of footnotes (after numbering).
    pub footnotes: u32,
}
impl Resolved {
    /// Whether the resolve pass has supplied no summary data.
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
            && self.toc.is_empty()
            && self.bibliography.is_none()
            && self.footnotes == 0
    }
}

/// Resolved numbering and display metadata for a label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelInfo {
    /// The content or numbering category.
    pub kind: LabelKind,
    /// `None` for an unnumbered element (a heading with numbering off);
    /// referencing it is a diagnostic, except headings, which resolve to their
    /// text.
    pub number: Option<String>,
    /// Localized word used before the number ("Figure", "Table", "Listing",
    /// "Equation", "Section"), from `config.document.lang`.
    pub supplement: String,
    /// Caption or heading text, plain, for tooltips and the DOCX `REF` cache.
    pub text: Option<String>,
    /// The source location, when available.
    pub span: Option<SourceSpan>,
}

/// One heading in the resolved table of contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TocEntry {
    /// Heading depth, from 1 through 6.
    pub level: u8,
    /// The number assigned by reference resolution.
    pub number: Option<String>,
    /// Inline content in source order.
    pub inlines: Inlines,
    /// The target anchor identifier.
    pub anchor: String,
}

/// A formatted bibliography and its display policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bibliography {
    /// The style actually used (name or path), for templates and `check`.
    pub style: String,
    /// Whether entries use a hanging indent.
    pub hanging_indent: bool,
    /// In the style's order (numeric styles: citation order; author-year:
    /// alphabetical).
    pub entries: Vec<BibEntry>,
}

/// One formatted bibliography entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BibEntry {
    /// The bibliography entry key.
    pub key: String,
    /// IEEE-style number `[1]` when the style has one; printed in a left
    /// margin.
    pub prefix: Option<Inlines>,
    /// Inline content in source order.
    pub inlines: Inlines,
}

/// A notebook code cell (notebook-mapping.md). Markdown cells become ordinary
/// blocks.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cell {
    /// 0-based index in the notebook, counting every cell.
    pub index: usize,
    /// The optional source identifier.
    pub id: Option<String>,
    /// Language for highlighting (`python`, …), from the notebook metadata.
    pub language: String,
    /// Source without the `#|` option lines.
    pub source: String,
    /// The notebook execution counter, when present.
    pub execution_count: Option<i64>,
    /// `false` when `echo: false` / `hide-input` / `remove-input`.
    pub show_input: bool,
    /// `false` when `output: false` / `hide-output` / `remove-output`.
    pub show_output: bool,
    /// Already lowered to blocks; see `CellOutput`.
    pub outputs: Vec<CellOutput>,
    /// Classes from `#| classes:` and tags; `attrs.id` from `#| label:` only
    /// when the label targets no captionable output and the outputs are shown
    /// (notebook-mapping.md §3.4; several captioned outputs are grouped in a
    /// `Figure { kind: Other }` instead).
    pub attrs: Attrs,
    /// Notebook tags in source order.
    pub tags: Vec<String>,
    /// `cell.metadata` verbatim (`Value::Null` if absent).
    pub metadata: Value,
    /// The source location, when available.
    pub span: Option<SourceSpan>,
}

/// One notebook output with alternative renderings in priority order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CellOutput {
    /// The content or numbering category.
    pub kind: OutputKind,
    /// `Some` only for `execute_result`.
    pub execution_count: Option<i64>,
    /// Alternative renderings of this output in priority order
    /// (notebook-mapping.md §4). An emitter takes the FIRST candidate it can
    /// render: a candidate whose blocks are `Raw` of a format the emitter does
    /// not print is skipped. Frontends guarantee the last candidate is
    /// renderable everywhere (text, or an empty list) — see `CellOutput::pick`.
    pub candidates: Vec<OutputCandidate>,
    /// MIME types the bundle offered, for diagnostics and `check`.
    pub mimes: Vec<String>,
}
impl Default for CellOutput {
    fn default() -> Self {
        Self {
            kind: OutputKind::Display,
            execution_count: None,
            candidates: Vec::new(),
            mimes: Vec::new(),
        }
    }
}

/// The nbformat category of an output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputKind {
    /// Standard output or standard error.
    Stream {
        /// The source name.
        name: String,
    },
    /// An exception and traceback.
    Error {
        /// The exception type name.
        ename: String,
        /// The exception message.
        evalue: String,
    },
    /// A display_data output.
    Display,
    /// An execute_result output.
    Result,
}

/// One alternative rendering of a notebook output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputCandidate {
    /// The MIME type this rendering came from (`text/html`, `image/png`,
    /// `text/plain`, …; `text/x-ansi` for streams and tracebacks).
    pub mime: String,
    /// `Figure`, `Table` (inside a `Figure` when captioned), `Math`,
    /// `CodeBlock`, `Raw`, `Paragraph`/`Div` for `text/markdown`, …
    pub blocks: Vec<Block>,
}

impl CellOutput {
    /// The blocks an emitter for `target` renders: the first candidate without
    /// `Raw` blocks of a foreign format. `target` is one of "html", "typst",
    /// "docx", "latex".
    pub fn pick(&self, target: &str) -> &[Block] {
        let format = match target {
            "html" => "html",
            "typst" => "typst",
            "docx" => "openxml",
            "latex" => "latex",
            _ => "",
        };
        self.candidates
            .iter()
            .find(|candidate| {
                let mut renderable = true;
                visit_blocks(&candidate.blocks, &mut |block| {
                    if let Block::Raw { format: raw, .. } = block {
                        renderable &= raw == format;
                    }
                });
                renderable
            })
            .map_or(&[], |candidate| candidate.blocks.as_slice())
    }
}

/// Optional content features that output renderers need.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(default)]
pub struct Features {
    /// Any math inline/block or `text/latex` output → KaTeX/MathML CSS, `mitex`
    /// package.
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
impl Features {
    /// Adds every optional output feature required by `other`.
    pub fn merge(&mut self, other: &Features) {
        self.math |= other.math;
        self.code |= other.code;
        self.notebook |= other.notebook;
        self.citations |= other.citations;
        self.footnotes |= other.footnotes;
    }
}

/// Source identity and the base directory for relative resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Absolute path of the source file.
    pub path: PathBuf,
    /// The content or numbering category.
    pub kind: SourceKind,
    /// Directory relative paths (images, includes, bibliographies) resolve
    /// against.
    pub base_dir: PathBuf,
}

/// The frontend used to read a document source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A Markdown source file.
    Markdown,
    /// A Jupyter notebook.
    Notebook,
    /// A Typst source file.
    Typst,
}

/// 1-based inclusive line range. For notebooks, lines are within the cell's
/// source and `cell` is the cell index; otherwise `cell` is `None`. `file` is
/// set when the lines belong to an included file (absolute).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    /// The first source line, numbered from 1.
    pub start_line: usize,
    /// The last source line, inclusive.
    pub end_line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// The zero-based notebook cell index, when applicable.
    pub cell: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// The absolute included-file path, when applicable.
    pub file: Option<PathBuf>,
}

/// Asset metadata exposed in dumps without the asset bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetInfo {
    /// The optional source identifier.
    pub id: String,
    /// The asset or output MIME type.
    pub mime: String,
    /// `"file:<path relative to base_dir>"` or `"inline:<byte length>"`.
    pub origin: String,
}

/// Output of any frontend, before reference resolution.
pub struct Parsed {
    /// Parsed content and source metadata.
    pub document: Document,
    /// Assets registered while parsing, with their lazy data sources.
    pub assets: AssetRegistry,
    /// Recoverable issues encountered by the frontend.
    pub diagnostics: Diagnostics,
}

impl Block {
    /// Returns this block's source location, when one is available.
    pub fn span(&self) -> Option<&SourceSpan> {
        match self {
            Self::Heading { span, .. }
            | Self::Paragraph { span, .. }
            | Self::BlockQuote { span, .. }
            | Self::List { span, .. }
            | Self::DefinitionList { span, .. }
            | Self::CodeBlock { span, .. }
            | Self::Figure { span, .. }
            | Self::Table { span, .. }
            | Self::Math { span, .. }
            | Self::Callout { span, .. }
            | Self::Div { span, .. }
            | Self::Raw { span, .. }
            | Self::ThematicBreak { span }
            | Self::PageBreak { span }
            | Self::Toc { span, .. }
            | Self::Bibliography { span } => span.as_ref(),
            Self::Cell(cell) => cell.span.as_ref(),
        }
    }

    /// Constructs a paragraph without a source location.
    pub fn paragraph(inlines: Inlines) -> Block {
        Self::Paragraph { inlines, span: None }
    }
}

impl Inline {
    /// Constructs a plain text inline.
    pub fn text(text: &str) -> Inline {
        Self::Text { text: text.to_owned() }
    }
}

impl Attrs {
    /// Constructs attributes with an id and no classes or key-value pairs.
    pub fn with_id(id: &str) -> Attrs {
        Self { id: Some(id.to_owned()), ..Self::default() }
    }
}

/// Extracts visible text, including resolved references and citations.
///
/// Breaks become spaces; footnotes, repeated footnote references, and raw
/// passthrough contribute nothing. Unresolved references and citations give
/// `[?]`.
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            Inline::Text { text: value }
            | Inline::Code { text: value, .. }
            | Inline::Math { source: value, .. } => text.push_str(value),
            Inline::LineBreak | Inline::SoftBreak => text.push(' '),
            Inline::Ref { resolved, .. } => match resolved {
                Some(reference) => text.push_str(&plain_text(&reference.text)),
                None => text.push_str("[?]"),
            },
            Inline::Cite { resolved, .. } => match resolved {
                Some(citation) => text.push_str(&plain_text(&citation.inlines)),
                None => text.push_str("[?]"),
            },
            Inline::Footnote { .. }
            | Inline::FootnoteRef { .. }
            | Inline::Raw { .. } => {}
            _ => inline_children(inline, &mut |children| {
                text.push_str(&plain_text(children));
            }),
        }
    }
    text
}

/// Visits blocks in pre-order, starting with the abstract and then the body.
///
/// Footnote blocks are visited where their note appears in a block's own
/// inlines, before that block's child blocks. Every output candidate is
/// visited; resolved reference, citation, and document summaries are never
/// traversed.
pub fn walk_blocks(document: &Document, f: &mut dyn FnMut(&Block)) {
    if let Some(abstract_) = &document.meta.abstract_ {
        visit_blocks(abstract_, f);
    }
    visit_blocks(&document.blocks, f);
}

/// Mutably visits the same pre-order block tree as [`walk_blocks`].
///
/// Each callback runs before the traversal inspects that block's children.
pub fn walk_blocks_mut(document: &mut Document, f: &mut dyn FnMut(&mut Block)) {
    if let Some(abstract_) = &mut document.meta.abstract_ {
        visit_blocks_mut(abstract_, f);
    }
    visit_blocks_mut(&mut document.blocks, f);
}

/// Visits inlines in pre-order throughout the abstract and body.
///
/// This includes nested footnote blocks, reference supplements, citation item
/// prefixes and suffixes, and every output candidate, but no resolved payloads.
pub fn walk_inlines(document: &Document, f: &mut dyn FnMut(&Inline)) {
    if let Some(abstract_) = &document.meta.abstract_ {
        visit_block_inlines(abstract_, f);
    }
    visit_block_inlines(&document.blocks, f);
}

fn inline_children(inline: &Inline, f: &mut dyn FnMut(&[Inline])) {
    match inline {
        Inline::Emph { inlines }
        | Inline::Strong { inlines }
        | Inline::Strikeout { inlines }
        | Inline::Subscript { inlines }
        | Inline::Superscript { inlines }
        | Inline::Underline { inlines }
        | Inline::SmallCaps { inlines }
        | Inline::Link { inlines, .. }
        | Inline::Span { inlines, .. } => f(inlines),
        Inline::Image { alt, .. } => f(alt),
        Inline::Ref { supplement: Some(inlines), .. } => f(inlines),
        Inline::Cite { items, .. } => {
            for item in items {
                if let Some(prefix) = &item.prefix {
                    f(prefix);
                }
                if let Some(suffix) = &item.suffix {
                    f(suffix);
                }
            }
        }
        _ => {}
    }
}

fn inline_children_mut(inline: &mut Inline, f: &mut dyn FnMut(&mut Inlines)) {
    match inline {
        Inline::Emph { inlines }
        | Inline::Strong { inlines }
        | Inline::Strikeout { inlines }
        | Inline::Subscript { inlines }
        | Inline::Superscript { inlines }
        | Inline::Underline { inlines }
        | Inline::SmallCaps { inlines }
        | Inline::Link { inlines, .. }
        | Inline::Span { inlines, .. } => f(inlines),
        Inline::Image { alt, .. } => f(alt),
        Inline::Ref { supplement: Some(inlines), .. } => f(inlines),
        Inline::Cite { items, .. } => {
            for item in items {
                if let Some(prefix) = &mut item.prefix {
                    f(prefix);
                }
                if let Some(suffix) = &mut item.suffix {
                    f(suffix);
                }
            }
        }
        _ => {}
    }
}

fn block_own_inlines(block: &Block, f: &mut dyn FnMut(&[Inline])) {
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph { inlines, .. } => {
            f(inlines)
        }
        Block::Figure { caption: Some(inlines), .. }
        | Block::Callout { title: Some(inlines), .. } => f(inlines),
        Block::DefinitionList { items, .. } => {
            for item in items {
                f(&item.term);
            }
        }
        Block::Table { head, body, foot, .. } => {
            for row in head.iter().chain(body).chain(foot) {
                for cell in &row.cells {
                    f(&cell.inlines);
                }
            }
        }
        _ => {}
    }
}

fn block_own_inlines_mut(block: &mut Block, f: &mut dyn FnMut(&mut Inlines)) {
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph { inlines, .. } => {
            f(inlines)
        }
        Block::Figure { caption: Some(inlines), .. }
        | Block::Callout { title: Some(inlines), .. } => f(inlines),
        Block::DefinitionList { items, .. } => {
            for item in items {
                f(&mut item.term);
            }
        }
        Block::Table { head, body, foot, .. } => {
            for row in head.iter_mut().chain(body).chain(foot) {
                for cell in &mut row.cells {
                    f(&mut cell.inlines);
                }
            }
        }
        _ => {}
    }
}

fn block_children(block: &Block, f: &mut dyn FnMut(&[Block])) {
    match block {
        Block::BlockQuote { blocks, .. }
        | Block::Callout { blocks, .. }
        | Block::Div { blocks, .. } => f(blocks),
        Block::Figure { body, .. } => f(body),
        Block::List { items, .. } => {
            for item in items {
                f(&item.blocks);
            }
        }
        Block::DefinitionList { items, .. } => {
            for item in items {
                for definition in &item.definitions {
                    f(definition);
                }
            }
        }
        Block::Cell(cell) => {
            for output in &cell.outputs {
                for candidate in &output.candidates {
                    f(&candidate.blocks);
                }
            }
        }
        _ => {}
    }
}

fn block_children_mut(block: &mut Block, f: &mut dyn FnMut(&mut Vec<Block>)) {
    match block {
        Block::BlockQuote { blocks, .. }
        | Block::Callout { blocks, .. }
        | Block::Div { blocks, .. } => f(blocks),
        Block::Figure { body, .. } => f(body),
        Block::List { items, .. } => {
            for item in items {
                f(&mut item.blocks);
            }
        }
        Block::DefinitionList { items, .. } => {
            for item in items {
                for definition in &mut item.definitions {
                    f(definition);
                }
            }
        }
        Block::Cell(cell) => {
            for output in &mut cell.outputs {
                for candidate in &mut output.candidates {
                    f(&mut candidate.blocks);
                }
            }
        }
        _ => {}
    }
}

fn visit_blocks(blocks: &[Block], f: &mut dyn FnMut(&Block)) {
    for block in blocks {
        f(block);
        block_own_inlines(block, &mut |inlines| visit_note_blocks(inlines, f));
        block_children(block, &mut |children| visit_blocks(children, f));
    }
}

fn visit_note_blocks(inlines: &[Inline], f: &mut dyn FnMut(&Block)) {
    for inline in inlines {
        if let Inline::Footnote { blocks, .. } = inline {
            visit_blocks(blocks, f);
        } else {
            inline_children(inline, &mut |children| {
                visit_note_blocks(children, f)
            });
        }
    }
}

fn visit_blocks_mut(blocks: &mut [Block], f: &mut dyn FnMut(&mut Block)) {
    for block in blocks {
        f(block);
        block_own_inlines_mut(block, &mut |inlines| {
            visit_note_blocks_mut(inlines, f)
        });
        block_children_mut(block, &mut |children| {
            visit_blocks_mut(children, f)
        });
    }
}

fn visit_note_blocks_mut(
    inlines: &mut [Inline], f: &mut dyn FnMut(&mut Block),
) {
    for inline in inlines {
        if let Inline::Footnote { blocks, .. } = inline {
            visit_blocks_mut(blocks, f);
        } else {
            inline_children_mut(inline, &mut |children| {
                visit_note_blocks_mut(children, f)
            });
        }
    }
}

fn visit_block_inlines(blocks: &[Block], f: &mut dyn FnMut(&Inline)) {
    for block in blocks {
        block_own_inlines(block, &mut |inlines| visit_inlines(inlines, f));
        block_children(block, &mut |children| visit_block_inlines(children, f));
    }
}

fn visit_inlines(inlines: &[Inline], f: &mut dyn FnMut(&Inline)) {
    for inline in inlines {
        f(inline);
        if let Inline::Footnote { blocks, .. } = inline {
            visit_block_inlines(blocks, f);
        } else {
            inline_children(inline, &mut |children| visit_inlines(children, f));
        }
    }
}

/// Detects Markdown, notebook, or Typst input by ASCII case-insensitive
/// extension.
///
/// Other extensions produce an error naming the file and the accepted
/// extensions.
pub fn detect_kind(path: &Path) -> Result<SourceKind> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("md" | "markdown") => Ok(SourceKind::Markdown),
        Some("ipynb") => Ok(SourceKind::Notebook),
        Some("typ") => Ok(SourceKind::Typst),
        _ => Err(Error::Other(format!(
            "{}: cannot tell the input format from the file name (expected a .md, .markdown, .ipynb or .typ file)",
            path.display()
        ))),
    }
}

impl Document {
    /// Reports structural contract violations, each prefixed by its IR path.
    ///
    /// Duplicate labels belong to resolution. Missing local images may retain
    /// their original URLs, so only `asset:` URLs have their grammar checked.
    pub fn invariant_violations(&self) -> Vec<String> {
        let mut checker = InvariantChecker { problems: Vec::new() };
        if let Some(abstract_) = &self.meta.abstract_ {
            checker.blocks(abstract_, "meta.abstract_", None, None);
        }
        checker.blocks(&self.blocks, "blocks", None, None);
        checker.problems
    }
}

struct InvariantChecker {
    problems: Vec<String>,
}

impl InvariantChecker {
    fn problem(&mut self, path: &str, message: &str) {
        self.problems.push(format!("{path}: {message}"));
    }

    fn id(&mut self, id: &str, path: &str) {
        if id.is_empty() || id.chars().any(char::is_whitespace) {
            self.problem(
                path,
                "id must be non-empty and contain no whitespace",
            );
        }
    }

    fn attrs(&mut self, attrs: &Attrs, path: &str) {
        if let Some(id) = &attrs.id {
            self.id(id, &format!("{path}.id"));
        }
    }

    fn asset_url(&mut self, url: &str, path: &str) {
        if let Some(rest) = url.strip_prefix("asset:") {
            let end = rest.find(['?', '#']).unwrap_or(rest.len());
            let id = &rest[..end];
            if id.len() != 12
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                self.problem(path, "expected asset:<12 lowercase hex digits>");
            }
        }
    }

    fn raw(&mut self, format: &str, text: &str, path: &str) {
        if format != format.to_lowercase() {
            self.problem(
                &format!("{path}.format"),
                "raw format must be lowercase",
            );
        }
        if format.eq_ignore_ascii_case("html") {
            // The constant expression is valid; compile it once for all checks.
            static URLS: std::sync::OnceLock<regex::Regex> =
                std::sync::OnceLock::new();
            let urls = URLS.get_or_init(|| regex::Regex::new(
                r#"(?i)\b(?:src|poster)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#
            ).expect("constant HTML attribute pattern is valid"));
            for capture in urls.captures_iter(text) {
                if let Some(url) = capture
                    .get(1)
                    .or_else(|| capture.get(2))
                    .or_else(|| capture.get(3))
                {
                    self.asset_url(url.as_str(), &format!("{path}.text"));
                }
            }
        }
    }

    fn blocks(
        &mut self, blocks: &[Block], path: &str,
        enclosing_figure: Option<FigureKind>,
        parent_figure: Option<FigureKind>,
    ) {
        for (index, block) in blocks.iter().enumerate() {
            self.block(
                block,
                &format!("{path}[{index}]"),
                enclosing_figure,
                parent_figure,
            );
        }
    }

    fn block(
        &mut self, block: &Block, path: &str,
        enclosing_figure: Option<FigureKind>,
        parent_figure: Option<FigureKind>,
    ) {
        self.inline_span(block.span(), path);
        self.block_shape(block, path, enclosing_figure, parent_figure);
        let figure = if let Block::Figure { kind, .. } = block {
            Some(*kind)
        } else {
            None
        };
        for (inlines, location) in located_inlines(block, path) {
            self.inlines(inlines, &location, figure.or(enclosing_figure));
        }
        for (children, location) in located_blocks(block, path) {
            self.blocks(
                children,
                &location,
                figure.or(enclosing_figure),
                figure,
            );
        }
    }

    fn block_shape(
        &mut self, block: &Block, path: &str,
        enclosing_figure: Option<FigureKind>,
        parent_figure: Option<FigureKind>,
    ) {
        let attrs = match block {
            Block::Heading { attrs, .. }
            | Block::CodeBlock { attrs, .. }
            | Block::Figure { attrs, .. }
            | Block::Table { attrs, .. }
            | Block::Math { attrs, .. }
            | Block::Callout { attrs, .. }
            | Block::Div { attrs, .. } => Some(attrs),
            Block::Cell(cell) => Some(&cell.attrs),
            _ => None,
        };
        if let Some(attrs) = attrs {
            self.attrs(attrs, &format!("{path}.attrs"));
        }
        match block {
            Block::Heading { level, .. } if !(1..=6).contains(level) => {
                self.problem(
                    &format!("{path}.level"),
                    "heading level must be 1–6",
                );
            }
            Block::List { tight: true, items, .. } => {
                self.tight_list(items, path)
            }
            Block::Figure { kind, body, .. } => {
                self.figure(*kind, body, path, enclosing_figure)
            }
            Block::Table { columns, head, body, foot, attrs, .. } => {
                if attrs.id.is_some()
                    && parent_figure != Some(FigureKind::Table)
                {
                    self.problem(
                        path,
                        "a labelled table must be wrapped in a table figure",
                    );
                }
                for (name, rows) in
                    [("head", head), ("body", body), ("foot", foot)]
                {
                    self.table_section(
                        rows,
                        columns.len(),
                        &format!("{path}.{name}"),
                    );
                }
            }
            Block::Raw { format, text, .. } => self.raw(format, text, path),
            _ => {}
        }
    }

    fn tight_list(&mut self, items: &[ListItem], path: &str) {
        for (i, item) in items.iter().enumerate() {
            for (j, child) in item.blocks.iter().enumerate() {
                if !matches!(
                    child,
                    Block::Paragraph { .. }
                        | Block::List { .. }
                        | Block::CodeBlock { .. }
                ) {
                    self.problem(
                        &format!("{path}.items[{i}].blocks[{j}]"),
                        "tight list allows only paragraphs, lists, and code blocks",
                    );
                }
            }
        }
    }

    fn figure(
        &mut self, kind: FigureKind, body: &[Block], path: &str,
        enclosing: Option<FigureKind>,
    ) {
        if enclosing.is_some_and(|parent| parent != FigureKind::Other) {
            self.problem(
                path,
                "figures may nest only inside a figure of kind other",
            );
        }
        let valid = match (kind, body) {
            (FigureKind::Other, _) => true,
            (FigureKind::Image, [Block::Paragraph { inlines, .. }]) => {
                matches!(inlines.as_slice(), [Inline::Image { .. }])
            }
            (FigureKind::Table, [Block::Table { .. }])
            | (FigureKind::Listing, [Block::CodeBlock { .. }]) => true,
            _ => false,
        };
        if !valid {
            self.problem(
                &format!("{path}.body"),
                "figure body does not match its kind",
            );
        }
    }

    fn table_section(&mut self, rows: &[Row], columns: usize, path: &str) {
        let mut remaining = vec![0u32; columns];
        for (i, row) in rows.iter().enumerate() {
            let row_path = format!("{path}[{i}]");
            let mut occupied: Vec<bool> =
                remaining.iter().map(|span| *span > 0).collect();
            let mut next: Vec<u32> =
                remaining.iter().map(|span| span.saturating_sub(1)).collect();
            let mut valid = true;
            for (j, cell) in row.cells.iter().enumerate() {
                for (field, span) in
                    [("colspan", cell.colspan), ("rowspan", cell.rowspan)]
                {
                    if span == 0 {
                        self.problem(
                            &format!("{row_path}.cells[{j}].{field}"),
                            "table cell span must be at least 1",
                        );
                    }
                }
                let start = occupied
                    .iter()
                    .position(|filled| !filled)
                    .unwrap_or(columns);
                let end = start.saturating_add(cell.colspan as usize);
                if cell.colspan > 0
                    && (end > columns
                        || occupied[start..end.min(columns)]
                            .iter()
                            .any(|filled| *filled))
                {
                    valid = false;
                }
                for column in start..end.min(columns) {
                    occupied[column] = true;
                    next[column] =
                        next[column].max(cell.rowspan.saturating_sub(1));
                }
            }
            if !valid || occupied.iter().any(|filled| !filled) {
                self.problem(
                    &row_path,
                    &format!("row must cover {columns} slots without overlap"),
                );
            }
            remaining = next;
        }
    }

    fn inlines(
        &mut self, inlines: &[Inline], path: &str, figure: Option<FigureKind>,
    ) {
        for (i, inline) in inlines.iter().enumerate() {
            let path = format!("{path}[{i}]");
            match inline {
                Inline::Code { attrs, .. }
                | Inline::Span { attrs, .. }
                | Inline::Link { attrs, .. }
                | Inline::Image { attrs, .. } => {
                    self.attrs(attrs, &format!("{path}.attrs"))
                }
                Inline::Footnote { label: Some(label), .. }
                | Inline::FootnoteRef { label, .. } => {
                    self.id(label, &format!("{path}.label"))
                }
                Inline::Ref { target, span, .. } => {
                    self.id(target, &format!("{path}.target"));
                    self.inline_span(span.as_ref(), &path);
                }
                Inline::Cite { span, .. } => {
                    self.inline_span(span.as_ref(), &path)
                }
                Inline::Raw { format, text } => self.raw(format, text, &path),
                _ => {}
            }
            if let Inline::Image { url, .. } = inline {
                self.asset_url(url, &format!("{path}.url"));
            }
            if let Inline::Footnote { blocks, .. } = inline {
                self.blocks(blocks, &format!("{path}.blocks"), figure, None);
            }
            for (children, location) in located_inline_children(inline, &path) {
                self.inlines(children, &location, figure);
            }
        }
    }

    fn inline_span(&mut self, span: Option<&SourceSpan>, path: &str) {
        if span.is_some_and(|span| {
            span.start_line == 0 || span.end_line < span.start_line
        }) {
            self.problem(
                &format!("{path}.span"),
                "source span must be a 1-based inclusive range",
            );
        }
    }
}

fn located_inlines<'a>(
    block: &'a Block, path: &str,
) -> Vec<(&'a [Inline], String)> {
    let mut result = Vec::new();
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph { inlines, .. } => {
            result.push((inlines.as_slice(), format!("{path}.inlines")))
        }
        Block::Figure { caption: Some(inlines), .. } => {
            result.push((inlines.as_slice(), format!("{path}.caption")))
        }
        Block::Callout { title: Some(inlines), .. } => {
            result.push((inlines.as_slice(), format!("{path}.title")))
        }
        Block::DefinitionList { items, .. } => {
            for (i, item) in items.iter().enumerate() {
                result.push((
                    item.term.as_slice(),
                    format!("{path}.items[{i}].term"),
                ));
            }
        }
        Block::Table { head, body, foot, .. } => {
            for (name, rows) in [("head", head), ("body", body), ("foot", foot)]
            {
                for (i, row) in rows.iter().enumerate() {
                    for (j, cell) in row.cells.iter().enumerate() {
                        result.push((
                            cell.inlines.as_slice(),
                            format!("{path}.{name}[{i}].cells[{j}].inlines"),
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    result
}

fn located_blocks<'a>(
    block: &'a Block, path: &str,
) -> Vec<(&'a [Block], String)> {
    let mut result = Vec::new();
    match block {
        Block::BlockQuote { blocks, .. }
        | Block::Callout { blocks, .. }
        | Block::Div { blocks, .. } => {
            result.push((blocks.as_slice(), format!("{path}.blocks")))
        }
        Block::Figure { body, .. } => {
            result.push((body.as_slice(), format!("{path}.body")))
        }
        Block::List { items, .. } => {
            for (i, item) in items.iter().enumerate() {
                result.push((
                    item.blocks.as_slice(),
                    format!("{path}.items[{i}].blocks"),
                ));
            }
        }
        Block::DefinitionList { items, .. } => {
            for (i, item) in items.iter().enumerate() {
                for (j, definition) in item.definitions.iter().enumerate() {
                    result.push((
                        definition.as_slice(),
                        format!("{path}.items[{i}].definitions[{j}]"),
                    ));
                }
            }
        }
        Block::Cell(cell) => {
            for (i, output) in cell.outputs.iter().enumerate() {
                for (j, candidate) in output.candidates.iter().enumerate() {
                    result.push((
                        candidate.blocks.as_slice(),
                        format!("{path}.outputs[{i}].candidates[{j}].blocks"),
                    ));
                }
            }
        }
        _ => {}
    }
    result
}

fn located_inline_children<'a>(
    inline: &'a Inline, path: &str,
) -> Vec<(&'a [Inline], String)> {
    let mut result = Vec::new();
    match inline {
        Inline::Emph { inlines }
        | Inline::Strong { inlines }
        | Inline::Strikeout { inlines }
        | Inline::Subscript { inlines }
        | Inline::Superscript { inlines }
        | Inline::Underline { inlines }
        | Inline::SmallCaps { inlines }
        | Inline::Link { inlines, .. }
        | Inline::Span { inlines, .. } => {
            result.push((inlines.as_slice(), format!("{path}.inlines")))
        }
        Inline::Image { alt, .. } => {
            result.push((alt.as_slice(), format!("{path}.alt")))
        }
        Inline::Ref { supplement: Some(inlines), .. } => {
            result.push((inlines.as_slice(), format!("{path}.supplement")))
        }
        Inline::Cite { items, .. } => {
            for (i, item) in items.iter().enumerate() {
                if let Some(prefix) = &item.prefix {
                    result.push((
                        prefix.as_slice(),
                        format!("{path}.items[{i}].prefix"),
                    ));
                }
                if let Some(suffix) = &item.suffix {
                    result.push((
                        suffix.as_slice(),
                        format!("{path}.items[{i}].suffix"),
                    ));
                }
            }
        }
        _ => {}
    }
    result
}
