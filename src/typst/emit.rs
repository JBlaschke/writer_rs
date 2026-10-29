//! Resolved IR to deterministic Typst source (typst-dialect.md §5).
//! Owner: T30.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::assets::registry::{Asset, AssetData, AssetRegistry, resolve_path};
use crate::config::{
    Config, EquationNumbering, FigureNumbering, HeadingNumbering,
};
use crate::error::{Diagnostic, Diagnostics, Error, Result};
use crate::ir::{
    self, Align, Attrs, Block, Cell, CiteItem, CiteMode, Document, FigureKind,
    Inline, ListItem, ListKind, MathDialect, RefForm, Row, SourceKind,
    SourceSpan, TableCell,
};
use crate::markdown::code::CodeAttrs;
use crate::math::latex_source;
use crate::resolve::l10n::{LangTag, Term, term};

use super::sourcemap::SourceMap;
use super::{MITEX_VERSION, WRITER_DOC_VERSION};

/// Options shared by a complete document and its materialized include parts.
#[derive(Debug, Clone)]
pub struct EmitOptions<'a> {
    /// Effective configuration, after front matter and command-line overrides.
    pub config: &'a Config,
    /// How registered assets are named in the generated source.
    pub assets: AssetPaths,
    /// Optional import path of a custom `writer-doc` template.
    pub template: Option<String>,
    /// Bibliography paths as written in the generated bibliography call.
    pub bibliography: Vec<String>,
    /// Citation style name or CSL path.
    pub style: String,
    /// Scope prefix for generated Markdown/notebook footnote labels.
    pub footnote_prefix: String,
    /// Emit an unresolved include without a document show rule.
    pub fragment: bool,
}

/// Asset path vocabulary used by compilation, export and round trips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetPaths {
    /// Virtual files mounted by the PDF compiler: `/__assets/<id>.<ext>`.
    Virtual,
    /// Export files beneath `dir`, named by their sanitized source hints.
    Export {
        /// Directory path as written into the generated source.
        dir: String,
    },
    /// File paths relative to the document's source directory.
    Source,
}

/// Generated source, line locations and the assets it actually references.
#[derive(Debug)]
pub struct Emitted {
    /// LF-terminated, deterministic Typst source.
    pub text: String,
    /// Location of the innermost source block at each generated line.
    pub map: SourceMap,
    /// Recoverable conversion and unsupported-construct warnings.
    pub diagnostics: Diagnostics,
    /// Asset id to the path written into `text`.
    pub assets: BTreeMap<String, String>,
}

/// A half-open run of top-level blocks emitted into a separate include file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeRun {
    /// First block index, inclusive.
    pub start: usize,
    /// Last block index, exclusive.
    pub end: usize,
    /// Path in the main file's `#include` statement.
    pub file: String,
}

/// Emits one resolved document, or an unresolved fragment when requested.
///
/// An unregistered `asset:` placeholder is an error. Math conversions and
/// constructs Typst cannot render are reported in the returned diagnostics.
pub fn emit(
    document: &Document, assets: &AssetRegistry, opts: &EmitOptions<'_>,
) -> Result<Emitted> {
    let notes = NoteLabels::collect(document, &opts.footnote_prefix);
    let paths = asset_paths(document, assets, &opts.assets);
    let mut emitter = Emitter::new(document, assets, opts, &notes, &paths);
    emitter.header(!opts.fragment)?;
    emitter.blocks(&document.blocks, 0)?;
    Ok(emitter.finish())
}

/// Emits a main document and one part per run, preserving shared identities.
///
/// Runs must be nonempty, ordered, disjoint and within the top-level blocks.
/// Parts omit the show rule while retaining `opts.fragment`'s resolution mode.
pub fn emit_split(
    document: &Document, assets: &AssetRegistry, opts: &EmitOptions<'_>,
    runs: &[IncludeRun],
) -> Result<(Emitted, Vec<Emitted>)> {
    validate_runs(runs, document.blocks.len())?;
    let notes = NoteLabels::collect(document, &opts.footnote_prefix);
    let paths = asset_paths(document, assets, &opts.assets);
    let mut main = Emitter::new(document, assets, opts, &notes, &paths);
    main.header(!opts.fragment)?;
    let mut parts = Vec::with_capacity(runs.len());
    let mut index = 0;
    for run in runs {
        main.blocks(&document.blocks[index..run.start], 0)?;
        main.separator();
        main.write("#include ");
        main.write(&string(&run.file));
        main.write("\n");
        let mut part = Emitter::new(document, assets, opts, &notes, &paths);
        part.header(false)?;
        part.blocks(&document.blocks[run.start..run.end], 0)?;
        parts.push(part.finish());
        index = run.end;
    }
    main.blocks(&document.blocks[index..], 0)?;
    Ok((main.finish(), parts))
}

fn validate_runs(runs: &[IncludeRun], length: usize) -> Result<()> {
    let mut previous = 0;
    for run in runs {
        if run.start < previous || run.start >= run.end || run.end > length {
            return Err(Error::Other(format!(
                "invalid Typst include run {}..{} for {length} blocks",
                run.start, run.end
            )));
        }
        previous = run.end;
    }
    Ok(())
}

#[derive(Default)]
struct Output {
    text: String,
    lines: Vec<Option<SourceSpan>>,
}

impl Output {
    fn push(&mut self, text: &str, span: Option<&SourceSpan>) {
        for piece in text.split_inclusive('\n') {
            if self.text.is_empty() || self.text.ends_with('\n') {
                self.lines.push(span.cloned());
            }
            self.text.push_str(piece);
        }
    }

    fn trim_generated_space(&mut self) {
        let had_line = !self.text.is_empty() && !self.text.ends_with('\n');
        while self.text.ends_with([' ', '\t']) {
            self.text.pop();
        }
        if had_line && (self.text.is_empty() || self.text.ends_with('\n')) {
            self.lines.pop();
        }
    }

    fn line_start(&self) -> bool {
        self.text.rsplit('\n').next().unwrap_or_default().trim().is_empty()
    }
}

struct Emitter<'a> {
    document: &'a Document,
    registry: &'a AssetRegistry,
    opts: &'a EmitOptions<'a>,
    notes: &'a NoteLabels,
    paths: &'a BTreeMap<String, String>,
    macros: BTreeMap<String, String>,
    output: Output,
    span: Option<SourceSpan>,
    scope: PathBuf,
    diagnostics: Diagnostics,
    assets: BTreeMap<String, String>,
}

impl<'a> Emitter<'a> {
    fn new(
        document: &'a Document, registry: &'a AssetRegistry,
        opts: &'a EmitOptions<'a>, notes: &'a NoteLabels,
        paths: &'a BTreeMap<String, String>,
    ) -> Self {
        Self {
            document,
            registry,
            opts,
            notes,
            paths,
            macros: latex_source::macros_from_config(
                &opts.config.math.katex_options,
            ),
            output: Output::default(),
            span: None,
            scope: document.source.path.clone(),
            diagnostics: Diagnostics::new(),
            assets: BTreeMap::new(),
        }
    }

    fn write(&mut self, text: &str) {
        self.output.push(text, self.span.as_ref());
    }

    fn indent(&mut self, indent: usize) {
        self.write(&" ".repeat(indent));
    }

    fn separator(&mut self) {
        if !self.output.text.ends_with("\n\n") {
            if !self.output.text.ends_with('\n') {
                self.write("\n");
            }
            self.write("\n");
        }
    }

    fn finish(mut self) -> Emitted {
        while self.output.text.ends_with("\n\n") {
            self.output.text.pop();
            self.output.lines.pop();
        }
        if !self.output.text.ends_with('\n') {
            self.write("\n");
        }
        Emitted {
            text: self.output.text,
            map: SourceMap::from_lines(self.output.lines),
            diagnostics: self.diagnostics,
            assets: self.assets,
        }
    }

    fn warning(&mut self, code: &'static str, message: String) {
        let mut diagnostic = Diagnostic::warning(code, message);
        diagnostic.path = Some(self.scope.clone());
        diagnostic.span = self.span.clone();
        self.diagnostics.push(diagnostic);
    }

    fn header(&mut self, show: bool) -> Result<()> {
        let name = self
            .document
            .source
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into());
        self.write(&format!(
            "// Generated by writer from {}; edit the source, not this file.\n",
            name.replace(['\r', '\n'], " ")
        ));
        self.write(&format!(
            "#import \"@writer/doc:{WRITER_DOC_VERSION}\": *\n"
        ));
        if has_latex(self.document) {
            self.write(&format!(
                "#import \"@preview/mitex:{MITEX_VERSION}\": mi, mitex\n"
            ));
        }
        if let Some(template) = &self.opts.template {
            self.write(&format!("#import {}: writer-doc\n", string(template)));
        }
        if show {
            self.show_rule()?;
        }
        self.write("\n");
        Ok(())
    }

    fn show_rule(&mut self) -> Result<()> {
        let meta = &self.document.meta;
        self.write("#show: writer-doc.with(\n");
        for (name, value) in
            [("title", &meta.title), ("subtitle", &meta.subtitle)]
        {
            if let Some(value) = value {
                self.write(&format!("  {name}: ["));
                self.text(value, false);
                self.write("],\n");
            }
        }
        if !meta.authors.is_empty() {
            let authors: Vec<_> = meta.authors.iter().map(author).collect();
            self.argument("authors", &array(&authors));
        }
        if let Some(date) = &meta.date {
            self.argument("date", &string(date));
        }
        if let Some(blocks) = &meta.abstract_ {
            self.write("  abstract: [\n");
            self.nested(blocks, 4)?;
            self.write("  ],\n");
        }
        if let Some(title) =
            meta.vars.get("abstract-title").and_then(|v| v.as_str())
        {
            self.argument("abstract-title", &string(title));
        }
        if !meta.keywords.is_empty() {
            self.argument(
                "keywords",
                &array(
                    &meta
                        .keywords
                        .iter()
                        .map(|v| string(v))
                        .collect::<Vec<_>>(),
                ),
            );
        }
        if let Some(description) = &meta.description {
            self.argument("description", &string(description));
        }
        self.argument(
            "lang",
            &string(effective_lang(self.document, self.opts.config)),
        );
        self.argument("numbering", &numbering(self.opts.config));
        self.argument("paper", &string(&self.opts.config.pdf.paper));
        self.argument("toc", "false");
        if !self.opts.config.references.locale.is_empty() {
            self.argument(
                "cite-locale",
                &string(&self.opts.config.references.locale),
            );
        }
        self.write(")\n");
        Ok(())
    }

    fn argument(&mut self, key: &str, value: &str) {
        self.write(&format!("  {key}: {value},\n"));
    }

    fn blocks(&mut self, blocks: &[Block], indent: usize) -> Result<()> {
        for block in blocks {
            if !self.renders(block) {
                continue;
            }
            self.separator();
            self.block(block, indent)?;
        }
        Ok(())
    }

    fn renders(&self, block: &Block) -> bool {
        match block {
            Block::Raw { format, text, .. } => {
                format == "typst" && !text.is_empty()
            }
            Block::Bibliography { .. } => {
                self.document.resolved.bibliography.is_some()
            }
            Block::Div { attrs, blocks, .. } => {
                attrs.id.is_some() || blocks.iter().any(|b| self.renders(b))
            }
            _ => true,
        }
    }

    fn block(&mut self, block: &Block, indent: usize) -> Result<()> {
        let old_span = self.span.clone();
        let old_scope = self.scope.clone();
        if let Some(span) = block.span() {
            self.span = Some(span.clone());
            self.scope = span
                .file
                .clone()
                .unwrap_or_else(|| self.document.source.path.clone());
        }
        if let Block::Div { attrs, blocks, .. } = block
            && attrs.id.is_none()
        {
            self.nested(blocks, indent)?;
        } else {
            self.indent(indent);
            self.block_body(block, indent)?;
        }
        if !self.output.text.ends_with('\n') {
            if !matches!(block, Block::Raw { .. }) {
                self.output.trim_generated_space();
            }
            self.write("\n");
        }
        self.span = old_span;
        self.scope = old_scope;
        Ok(())
    }

    fn block_body(&mut self, block: &Block, indent: usize) -> Result<()> {
        match block {
            Block::Heading { level, inlines, attrs, .. } => {
                self.heading(*level, inlines, attrs, indent)?
            }
            Block::Paragraph { inlines, .. } => {
                if let [Inline::Image { alt, url, attrs, .. }] =
                    inlines.as_slice()
                {
                    self.image(alt, url, attrs, true)?;
                    self.label(attrs, false);
                } else {
                    self.inlines(inlines, indent, false, false)?;
                }
            }
            Block::BlockQuote { blocks, .. } => {
                self.container("#quote(block: true)", blocks, indent)?
            }
            Block::List { kind, tight, items, .. } => {
                self.list(kind, *tight, items, indent)?
            }
            Block::DefinitionList { items, .. } => {
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        self.write("\n");
                        self.indent(indent);
                    }
                    self.write("/ ");
                    self.inlines(&item.term, indent, true, true)?;
                    self.write(": ");
                    for (index, definition) in
                        item.definitions.iter().enumerate()
                    {
                        if index > 0 {
                            self.write("\n");
                            self.indent(indent + 2);
                        }
                        self.first_blocks(definition, indent + 2)?;
                    }
                }
            }
            Block::CodeBlock { lang, text, attrs, .. } => {
                self.code_block(lang.as_deref(), text, attrs, indent)?;
                self.label(attrs, false);
            }
            Block::Figure { kind, caption, body, attrs, .. } => {
                self.figure(*kind, caption.as_deref(), body, attrs, indent)?
            }
            Block::Table { columns, head, body, foot, attrs, .. } => {
                self.table(columns, head, body, foot, indent, true)?;
                self.label(attrs, false);
            }
            Block::Math { source, dialect, attrs, .. } => {
                self.math(source, *dialect, true, class(attrs, "unnumbered"));
                self.label(attrs, false);
            }
            Block::Callout { kind, title, blocks, attrs, .. } => {
                self.write(&format!("#callout(kind: {}", string(kind)));
                if let Some(title) = title {
                    self.write(", title: [");
                    self.inlines(title, indent, false, false)?;
                    self.write("]");
                }
                self.write(")[\n");
                self.nested(blocks, indent + 2)?;
                self.indent(indent);
                self.write("]");
                self.label(attrs, false);
            }
            Block::Div { attrs, blocks, .. } => {
                if let Some(id) = &attrs.id {
                    self.write(&format!("#metadata(none) <{id}>\n"));
                    self.blocks(blocks, indent)?;
                } else if let Some((first, rest)) = blocks.split_first() {
                    self.block_body(first, indent)?;
                    self.blocks(rest, indent)?;
                }
            }
            Block::Raw { format, text, .. } if format == "typst" => {
                self.write(&lf(text))
            }
            Block::Raw { .. } => {}
            Block::ThematicBreak { .. } => self.write("#line(length: 100%)"),
            Block::PageBreak { .. } => self.write("#pagebreak()"),
            Block::Toc { depth, .. } => {
                self.write(&format!("#outline(depth: {depth}"));
                self.title_arg("toc-title");
                self.write(")");
            }
            Block::Bibliography { .. } => self.bibliography(),
            Block::Cell(cell) => self.cell(cell, indent)?,
        }
        Ok(())
    }

    fn heading(
        &mut self, level: u8, inlines: &[Inline], attrs: &Attrs, indent: usize,
    ) -> Result<()> {
        let unnumbered = class(attrs, "unnumbered")
            && self.opts.config.numbering.headings != HeadingNumbering::Off;
        let unlisted = class(attrs, "notoc") || class(attrs, "unlisted");
        if unnumbered || unlisted {
            self.write(&format!("#heading(level: {level}"));
            if unnumbered {
                self.write(", numbering: none");
            }
            if unlisted {
                self.write(", outlined: false");
            }
            self.write(")[");
            self.inlines(inlines, indent, true, false)?;
            self.write("]");
        } else {
            self.write(&"=".repeat(level as usize));
            self.write(" ");
            self.inlines(inlines, indent, true, false)?;
        }
        self.label(attrs, false);
        Ok(())
    }

    fn nested(&mut self, blocks: &[Block], indent: usize) -> Result<()> {
        let mut first = true;
        for block in blocks {
            if self.renders(block) {
                if !first {
                    self.separator();
                }
                self.block(block, indent)?;
                first = false;
            }
        }
        Ok(())
    }

    fn container(
        &mut self, call: &str, blocks: &[Block], indent: usize,
    ) -> Result<()> {
        self.write(call);
        self.write("[\n");
        self.nested(blocks, indent + 2)?;
        self.indent(indent);
        self.write("]");
        Ok(())
    }

    fn first_blocks(&mut self, blocks: &[Block], indent: usize) -> Result<()> {
        if let Some((first, rest)) = blocks.split_first() {
            if let Block::Paragraph { inlines, .. } = first {
                let old = self.enter(first);
                self.inlines(inlines, indent, false, false)?;
                self.write("\n");
                self.leave(old);
            } else {
                // The list/definition prefix ends before a block-only body.
                self.output.trim_generated_space();
                self.write("\n");
                self.block(first, indent)?;
            }
            if !rest.is_empty() {
                self.separator();
                self.nested(rest, indent)?;
            }
        }
        Ok(())
    }

    fn list(
        &mut self, kind: &ListKind, tight: bool, items: &[ListItem],
        indent: usize,
    ) -> Result<()> {
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                if !tight {
                    self.write("\n");
                }
                self.indent(indent);
            }
            match kind {
                ListKind::Bullet => self.write("- "),
                ListKind::Ordered { start } if index == 0 && *start != 1 => {
                    self.write(&format!("{start}. "))
                }
                ListKind::Ordered { .. } => self.write("+ "),
            }
            if let Some(checked) = item.checked {
                self.write(if checked { "☒ " } else { "☐ " });
            }
            self.first_blocks(&item.blocks, indent + 2)?;
        }
        Ok(())
    }

    fn code_block(
        &mut self, lang: Option<&str>, text: &str, attrs: &Attrs, indent: usize,
    ) -> Result<()> {
        let code = CodeAttrs::from_attrs(attrs);
        let listing = code.plain
            || code.numbered
            || code.start != 1
            || !code.highlight.is_empty();
        if listing {
            self.write("#listing(");
            let mut args = Vec::new();
            if code.plain {
                args.push("plain: true".into());
            }
            if code.numbered {
                args.push("numbered: true".into());
            }
            if code.start != 1 {
                args.push(format!("start: {}", code.start));
            }
            if !code.highlight.is_empty() {
                let ranges: Vec<_> = code
                    .highlight
                    .iter()
                    .map(|r| {
                        if r.start() == r.end() {
                            r.start().to_string()
                        } else {
                            format!("{}-{}", r.start(), r.end())
                        }
                    })
                    .collect();
                args.push(format!("highlight: {}", string(&ranges.join(","))));
            }
            self.write(&args.join(", "));
            self.write(")[\n");
            self.indent(indent + 2);
        }
        self.raw_block(lang, text, indent + usize::from(listing) * 2);
        if listing {
            self.write("\n");
            self.indent(indent);
            self.write("]");
        }
        Ok(())
    }

    fn raw_block(&mut self, lang: Option<&str>, text: &str, indent: usize) {
        if let Some(lang) = lang.filter(|lang| !typst_syntax::is_ident(lang)) {
            self.write(&format!(
                "#raw(block: true, lang: {}, {})",
                string(lang),
                string(text)
            ));
            return;
        }
        let fence =
            "`".repeat(longest_backticks(text).saturating_add(1).max(3));
        self.write(&fence);
        self.write(lang.unwrap_or_default());
        self.write("\n");
        let text = lf(text);
        for line in text.split_terminator('\n') {
            if !line.is_empty() {
                self.indent(indent);
                self.write(line);
            }
            self.write("\n");
        }
        self.indent(indent);
        self.write(&fence);
    }

    fn figure(
        &mut self, kind: FigureKind, caption: Option<&[Inline]>,
        body: &[Block], attrs: &Attrs, indent: usize,
    ) -> Result<()> {
        self.write("#figure(");
        let inner = (kind != FigureKind::Other).then(|| body.first()).flatten();
        let old = inner.map(|block| self.enter(block));
        match (kind, body) {
            (FigureKind::Image, [Block::Paragraph { inlines, .. }]) => {
                if let [Inline::Image { alt, url, attrs, .. }] =
                    inlines.as_slice()
                {
                    self.image(alt, url, attrs, false)?;
                }
            }
            (
                FigureKind::Table,
                [Block::Table { columns, head, body, foot, .. }],
            ) => self.table(columns, head, body, foot, indent + 2, false)?,
            (
                FigureKind::Listing,
                [Block::CodeBlock { lang, text, attrs, .. }],
            ) => {
                self.write("[\n");
                self.indent(indent + 2);
                self.code_block(lang.as_deref(), text, attrs, indent + 2)?;
                self.write("\n");
                self.indent(indent);
                self.write("]");
                self.write(", kind: raw");
            }
            _ => {
                self.write("kind: image, [\n");
                self.nested(body, indent + 2)?;
                self.indent(indent);
                self.write("]");
            }
        }
        if let Some(old) = old {
            self.leave(old);
        }
        if let Some(caption) = caption {
            self.write(", caption: [");
            self.inlines(caption, indent, false, false)?;
            self.write("]");
        }
        if class(attrs, "unnumbered") {
            self.write(", numbering: none");
        }
        self.write(")");
        self.label(attrs, false);
        Ok(())
    }

    fn table(
        &mut self, columns: &[ir::ColumnSpec], head: &[Row], body: &[Row],
        foot: &[Row], indent: usize, markup: bool,
    ) -> Result<()> {
        self.write(if markup { "#table(\n" } else { "table(\n" });
        self.indent(indent + 2);
        let widths: Vec<_> = columns
            .iter()
            .map(|c| {
                c.width.map_or_else(
                    || "auto".into(),
                    |w| format!("{}%", decimal(f64::from(w) * 100.0)),
                )
            })
            .collect();
        self.write(&format!("columns: {},\n", array(&widths)));
        if columns.iter().any(|c| c.align != Align::Default) {
            self.indent(indent + 2);
            self.write(&format!(
                "align: {},\n",
                array(
                    &columns
                        .iter()
                        .map(|c| alignment(c.align).into())
                        .collect::<Vec<_>>()
                )
            ));
        }
        self.rows(head, "table.header", indent + 2)?;
        self.rows(body, "", indent + 2)?;
        self.rows(foot, "table.footer", indent + 2)?;
        self.indent(indent);
        self.write(")");
        Ok(())
    }

    fn rows(
        &mut self, rows: &[Row], wrapper: &str, indent: usize,
    ) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        if !wrapper.is_empty() {
            self.indent(indent);
            self.write(wrapper);
            self.write("(\n");
        }
        for row in rows {
            for cell in &row.cells {
                self.indent(indent + usize::from(!wrapper.is_empty()) * 2);
                self.table_cell(cell, indent + 2)?;
                self.write(",\n");
            }
        }
        if !wrapper.is_empty() {
            self.indent(indent);
            self.write("),\n");
        }
        Ok(())
    }

    fn table_cell(&mut self, cell: &TableCell, indent: usize) -> Result<()> {
        let wrapped =
            cell.colspan != 1 || cell.rowspan != 1 || cell.align.is_some();
        if wrapped {
            let mut args = Vec::new();
            if cell.colspan != 1 {
                args.push(format!("colspan: {}", cell.colspan));
            }
            if cell.rowspan != 1 {
                args.push(format!("rowspan: {}", cell.rowspan));
            }
            if let Some(align) = cell.align {
                args.push(format!("align: {}", alignment(align)));
            }
            self.write(&format!("table.cell({})", args.join(", ")));
        }
        self.write("[");
        self.inlines(&cell.inlines, indent, false, false)?;
        self.write("]");
        Ok(())
    }

    fn math(
        &mut self, source: &str, dialect: MathDialect, display: bool,
        unnumbered: bool,
    ) -> bool {
        if dialect == MathDialect::Latex {
            let (prepared, removed) =
                latex_source::for_typst(source, &self.macros);
            if removed {
                self.warning("math-tag-ignored", "\\tag/\\label inside math are ignored for pdf; label the equation with {#eq-x}".into());
            }
            match latex_source::preflight_typst(&prepared) {
                Ok(()) => {
                    self.write(if display { "#mitex(" } else { "#mi(" });
                    self.write(&math_literal(&prepared));
                    if unnumbered {
                        self.write(", numbering: none");
                    }
                    self.write(")");
                    return true;
                }
                Err(message) => self.warning("math-conversion-failed", format!("pdf: cannot convert equation ({message}); used the source text")),
            }
            let fallback = format!("#raw({})", string(source));
            self.typst_math(&fallback, display, unnumbered);
        } else {
            self.typst_math(source, display, unnumbered);
        }
        display && unnumbered
    }

    fn typst_math(&mut self, source: &str, display: bool, unnumbered: bool) {
        if display && unnumbered {
            self.write("#math.equation(block: true, numbering: none, $");
            self.write(source);
            self.write("$)");
        } else {
            self.write(if display { "$ " } else { "$" });
            self.write(source);
            self.write(if display { " $" } else { "$" });
        }
    }

    fn label(&mut self, attrs: &Attrs, inline: bool) {
        if let Some(id) = &attrs.id {
            self.write(if inline { "<" } else { " <" });
            self.write(id);
            self.write(">");
        }
    }

    fn title_arg(&mut self, key: &str) -> bool {
        if let Some(title) =
            self.document.meta.vars.get(key).and_then(|v| v.as_str())
        {
            self.write(&format!(", title: {}", string(title)));
            true
        } else {
            false
        }
    }

    fn bibliography(&mut self) {
        if self.document.resolved.bibliography.is_none() {
            return;
        }
        self.write("#bibliography(");
        self.write(&array(
            &self
                .opts
                .bibliography
                .iter()
                .map(|path| string(path))
                .collect::<Vec<_>>(),
        ));
        self.write(&format!(", style: {}", string(&self.opts.style)));
        if !self.title_arg("reference-section-title")
            && !self.opts.config.references.locale.is_empty()
        {
            let language =
                LangTag::effective(&self.document.meta, self.opts.config)
                    .unwrap_or(LangTag { lang: "en".into(), region: None });
            self.write(", title: [");
            self.text(&term(&language, Term::Bibliography), false);
            self.write("]");
        }
        self.write(")");
    }

    fn cell(&mut self, cell: &Cell, indent: usize) -> Result<()> {
        self.write(&format!("#code-cell(index: {}, count: ", cell.index));
        self.write(
            &cell
                .execution_count
                .map_or_else(|| "none".into(), |count| count.to_string()),
        );
        self.write(&format!(", lang: {}, input: ", string(&cell.language)));
        if cell.show_input {
            self.write("[\n");
            self.indent(indent + 2);
            self.code_block(
                Some(&cell.language),
                &cell.source,
                &cell.attrs,
                indent + 2,
            )?;
            self.write("\n");
            self.indent(indent);
            self.write("]");
        } else {
            self.write("none");
        }
        self.write(")");
        if cell.show_output {
            for output in &cell.outputs {
                self.write("[\n");
                self.nested(output.pick("typst"), indent + 2)?;
                self.indent(indent);
                self.write("]");
            }
        }
        self.label(&cell.attrs, false);
        Ok(())
    }

    fn enter(&mut self, block: &Block) -> (Option<SourceSpan>, PathBuf) {
        let old = (self.span.clone(), self.scope.clone());
        if let Some(span) = block.span() {
            self.span = Some(span.clone());
            self.scope = span
                .file
                .clone()
                .unwrap_or_else(|| self.document.source.path.clone());
        }
        old
    }

    fn leave(&mut self, old: (Option<SourceSpan>, PathBuf)) {
        (self.span, self.scope) = old;
    }

    fn inlines(
        &mut self, inlines: &[Inline], indent: usize, single_line: bool,
        definition: bool,
    ) -> Result<bool> {
        let mut last_expression = false;
        let mut previous = None;
        let mut previous_cite = false;
        for (index, inline) in inlines.iter().enumerate() {
            let next = inlines[index + 1..].iter().find_map(|i| edge(i, true));
            if matches!(inline, Inline::Cite { .. }) && previous_cite {
                self.write("#h(0pt)");
            }
            let expression = self.inline(
                inline, indent, single_line, definition, previous, next,
            )?;
            last_expression = expression;
            if expression && next.is_some_and(|c| matches!(c, '.' | '(' | ';'))
            {
                self.write(";");
            }
            if !citation_space(inline) {
                previous_cite = matches!(inline, Inline::Cite { .. });
            }
            if let Some(last) = edge(inline, false) {
                previous = Some(last);
            }
        }
        Ok(last_expression)
    }

    fn inline(
        &mut self, inline: &Inline, indent: usize, single_line: bool,
        definition: bool, previous: Option<char>, next: Option<char>,
    ) -> Result<bool> {
        match inline {
            Inline::Text { text } => {
                self.text_followed(text, definition, next);
                Ok(false)
            }
            Inline::Emph { inlines } | Inline::Strong { inlines } => {
                let emph = matches!(inline, Inline::Emph { .. });
                let call = previous.is_some_and(char::is_alphanumeric)
                    || next.is_some_and(char::is_alphanumeric);
                if call {
                    self.write(if emph { "#emph[" } else { "#strong[" });
                } else {
                    self.write(if emph { "_" } else { "*" });
                }
                self.inlines(inlines, indent, single_line, definition)?;
                self.write(if call {
                    "]"
                } else if emph {
                    "_"
                } else {
                    "*"
                });
                Ok(call)
            }
            Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines } => {
                let name = match inline {
                    Inline::Strikeout { .. } => "strike",
                    Inline::Subscript { .. } => "sub",
                    Inline::Superscript { .. } => "super",
                    Inline::Underline { .. } => "underline",
                    _ => "smallcaps",
                };
                self.write(&format!("#{name}["));
                self.inlines(inlines, indent, single_line, definition)?;
                self.write("]");
                Ok(true)
            }
            Inline::Code { text, attrs } => {
                let raw = text.is_empty()
                    || text.contains(['`', '\n', '\r'])
                    || !attrs.classes.is_empty();
                if raw {
                    self.write("#raw(");
                    if let Some(lang) = attrs.classes.first() {
                        self.write(&format!("lang: {}, ", string(lang)));
                    }
                    self.write(&string(text));
                    self.write(")");
                } else {
                    self.write("`");
                    self.write(text);
                    self.write("`");
                }
                self.label(attrs, true);
                Ok(raw && attrs.id.is_none())
            }
            Inline::Math { source, dialect } => {
                Ok(self.math(source, *dialect, false, false))
            }
            Inline::Link { inlines, url, attrs, .. } => {
                if let Some(target) = url.strip_prefix('#') {
                    if self.opts.fragment
                        || self.document.resolved.labels.contains_key(target)
                    {
                        self.write(&format!("#link(<{target}>)["));
                        self.inlines(inlines, indent, single_line, definition)?;
                        self.write("]");
                        self.label(attrs, true);
                        return Ok(attrs.id.is_none());
                    }
                    self.inlines(inlines, indent, single_line, definition)?;
                    self.label(attrs, true);
                    return Ok(false);
                }
                let url = self.asset_url(url)?;
                self.write(&format!("#link({})", string(&url)));
                if !matches!(inlines.as_slice(), [Inline::Text { text }] if text == &url)
                {
                    self.write("[");
                    self.inlines(inlines, indent, single_line, definition)?;
                    self.write("]");
                }
                self.label(attrs, true);
                Ok(attrs.id.is_none())
            }
            Inline::Image { alt, url, attrs, .. } => {
                if url.starts_with("asset:") {
                    self.write("#box(");
                    self.image(alt, url, attrs, false)?;
                    self.write(")");
                } else {
                    self.remote_image(alt, url, true)?;
                }
                self.label(attrs, true);
                Ok(attrs.id.is_none())
            }
            Inline::Footnote { blocks, label, .. } => {
                self.write("#footnote[");
                if let [block @ Block::Paragraph { inlines, .. }] =
                    blocks.as_slice()
                {
                    let old = self.enter(block);
                    self.inlines(inlines, indent, single_line, definition)?;
                    self.leave(old);
                } else {
                    self.write("\n");
                    self.nested(blocks, indent + 2)?;
                    self.indent(indent);
                }
                self.write("]");
                if label.is_some()
                    && let Some(final_label) = self
                        .notes
                        .definitions
                        .get(&(inline as *const Inline as usize))
                {
                    self.write(&format!("<{final_label}>"));
                }
                Ok(label.is_none())
            }
            Inline::FootnoteRef { label, number } => {
                let final_label = if self.opts.fragment {
                    self.notes.scopes.get(&(self.scope.clone(), label.clone()))
                } else {
                    number.and_then(|number| self.notes.numbers.get(&number))
                };
                let fallback = generated_note_label(
                    label,
                    &self.opts.footnote_prefix,
                    self.scope_kind(),
                );
                self.write(&format!(
                    "#footnote(<{}>)",
                    final_label.unwrap_or(&fallback)
                ));
                Ok(true)
            }
            Inline::Ref { target, supplement, form, resolved, .. } => self
                .reference(
                    target,
                    supplement.as_deref(),
                    *form,
                    resolved.as_ref(),
                    next,
                    indent,
                ),
            Inline::Cite { items, mode, resolved, .. } => {
                if !self.opts.fragment && resolved.is_none() {
                    self.text(
                        &format!(
                            "[?{}]",
                            items
                                .iter()
                                .map(|i| i.key.as_str())
                                .collect::<Vec<_>>()
                                .join("; ")
                        ),
                        definition,
                    );
                    Ok(false)
                } else {
                    self.citation(items, *mode, indent)
                }
            }
            Inline::LineBreak if single_line => {
                self.write("#linebreak()");
                Ok(true)
            }
            Inline::LineBreak => {
                self.write(" \\\n");
                self.indent(indent);
                Ok(false)
            }
            Inline::SoftBreak if single_line => {
                self.write(" ");
                Ok(false)
            }
            Inline::SoftBreak => {
                self.write("\n");
                self.indent(indent);
                Ok(false)
            }
            Inline::Span { attrs, inlines } => {
                if let Some(id) = &attrs.id {
                    self.write(&format!("#metadata(none)<{id}>"));
                }
                let caption = class(attrs, "subcaption");
                if caption {
                    self.write("#text(size: 0.85em)[");
                }
                let inner_expression =
                    self.inlines(inlines, indent, single_line, definition)?;
                if caption {
                    self.write("]");
                }
                Ok(caption || inner_expression)
            }
            Inline::Raw { format, text } => {
                if format == "typst" {
                    self.write(&lf(text));
                }
                Ok(false)
            }
        }
    }

    fn reference(
        &mut self, target: &str, supplement: Option<&[Inline]>, form: RefForm,
        resolved: Option<&ir::ResolvedRef>, next: Option<char>, indent: usize,
    ) -> Result<bool> {
        let info = self.document.resolved.labels.get(target);
        if !self.opts.fragment && info.is_none() {
            self.text(&format!("[?{target}]"), false);
            Ok(false)
        } else if form != RefForm::Page
            && info.is_some_and(|i| i.number.is_none())
        {
            self.write(&format!("#link(<{target}>)["));
            if let Some(resolved) = resolved {
                self.inlines(&resolved.text, indent, false, false)?;
            } else if let Some(text) = info.and_then(|i| i.text.as_deref()) {
                self.text(text, false);
            } else {
                self.text(target, false);
            }
            self.write("]");
            Ok(true)
        } else if form == RefForm::Page
            || target.ends_with(['.', ':'])
            || next.is_some_and(label_character)
        {
            self.write(&format!("#ref(<{target}>"));
            if form == RefForm::Page {
                self.write(", form: \"page\"");
            }
            if let Some(supplement) = supplement {
                self.write(", supplement: [");
                self.inlines(supplement, indent, false, false)?;
                self.write("]");
            }
            self.write(")");
            Ok(true)
        } else {
            self.write(&format!("@{target}"));
            if let Some(supplement) = supplement {
                self.write("[");
                self.inlines(supplement, indent, false, false)?;
                self.write("]");
            }
            Ok(false)
        }
    }

    fn citation(
        &mut self, items: &[CiteItem], mode: CiteMode, indent: usize,
    ) -> Result<bool> {
        let mut start = 0;
        let mut groups = Vec::new();
        for (index, item) in items.iter().enumerate() {
            if index > start && item.prefix.is_some() {
                groups.push(&items[start..index]);
                start = index;
            }
            if item.suffix.is_some() {
                groups.push(&items[start..=index]);
                start = index + 1;
            }
        }
        if start < items.len() {
            groups.push(&items[start..]);
        }
        let mut expression = false;
        for (group_index, group) in groups.into_iter().enumerate() {
            if group_index > 0 {
                self.write(" ");
            }
            if let Some(prefix) = &group[0].prefix {
                self.inlines(prefix, indent, false, false)?;
                self.write(" ");
            }
            for (index, item) in group.iter().enumerate() {
                if index > 0 {
                    self.write(" ");
                }
                let shorthand =
                    mode == CiteMode::Normal && !item.key.ends_with(['.', ':']);
                expression = !shorthand;
                if shorthand {
                    self.write(&format!("@{}", item.key));
                    if let Some(locator) = &item.locator {
                        self.write("[");
                        self.text(locator, false);
                        self.write("]");
                    }
                } else {
                    self.write(&format!("#cite(<{}>", item.key));
                    if mode != CiteMode::Normal {
                        self.write(&format!(", form: {}", cite_form(mode)));
                    }
                    if let Some(locator) = &item.locator {
                        self.write(", supplement: [");
                        self.text(locator, false);
                        self.write("]");
                    }
                    self.write(")");
                }
            }
            if let Some(suffix) = &group[group.len() - 1].suffix {
                self.write(" ");
                expression = self.inlines(suffix, indent, false, false)?;
            }
        }
        Ok(expression)
    }

    fn image(
        &mut self, alt: &[Inline], url: &str, attrs: &Attrs, markup: bool,
    ) -> Result<()> {
        if !url.starts_with("asset:") {
            return self.remote_image(alt, url, markup);
        }
        let path = self.asset_url(url)?;
        self.write(if markup { "#image(" } else { "image(" });
        self.write(&string(&path));
        for key in ["width", "height"] {
            if let Some(value) = attrs.pairs.get(key) {
                self.write(&format!(", {key}: {}", dimension(value)));
            }
        }
        let alt = ir::plain_text(alt);
        if !alt.is_empty() {
            self.write(&format!(", alt: {}", string(&alt)));
        }
        self.write(")");
        Ok(())
    }

    fn remote_image(
        &mut self, alt: &[Inline], url: &str, markup: bool,
    ) -> Result<()> {
        self.warning(
            "unsupported-construct",
            format!(
                "pdf: cannot fetch image `{url}`; used a link to its source"
            ),
        );
        self.write(if markup { "#link(" } else { "link(" });
        self.write(&string(url));
        self.write(")[");
        self.text(&ir::plain_text(alt), false);
        self.write("]");
        Ok(())
    }

    fn asset_url(&mut self, url: &str) -> Result<String> {
        let Some(id) = url.strip_prefix("asset:") else {
            return Ok(url.into());
        };
        let id = id.split(['?', '#']).next().unwrap_or(id);
        if self.registry.get(id).is_none() {
            return Err(Error::Other(format!(
                "unregistered asset placeholder `asset:{id}`"
            )));
        }
        let path = self.paths[id].clone();
        self.assets.insert(id.into(), path.clone());
        Ok(path)
    }

    fn scope_kind(&self) -> Option<SourceKind> {
        if self.scope == self.document.source.path {
            Some(self.document.source.kind)
        } else {
            ir::detect_kind(&self.scope).ok()
        }
    }

    fn text(&mut self, text: &str, definition: bool) {
        self.text_followed(text, definition, None);
    }

    fn text_followed(
        &mut self, text: &str, definition: bool, following: Option<char>,
    ) {
        let text = lf(text);
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if self.output.line_start() && matches!(c, ' ' | '\t') {
                continue;
            }
            let next = chars.peek().copied().or(following);
            let start = self.output.line_start();
            let line = self
                .output
                .text
                .rsplit('\n')
                .next()
                .unwrap_or_default()
                .trim_start();
            let number_dot = c == '.'
                && !line.is_empty()
                && line.chars().all(|c| c.is_ascii_digit());
            let special = matches!(
                c,
                '\\' | '#'
                    | '*'
                    | '_'
                    | '`'
                    | '$'
                    | '<'
                    | '>'
                    | '@'
                    | '['
                    | ']'
                    | '~'
                    | '\''
                    | '"'
            ) || (definition && c == ':')
                || (start && matches!(c, '=' | '+' | '-' | '/'))
                || (c == '-'
                    && next.is_some_and(|n| {
                        n == '-' || n == '?' || n.is_ascii_digit()
                    }))
                || (c == '.'
                    && next == Some('.')
                    && chars.clone().nth(1).or(following) == Some('.'))
                || (c == '/' && next.is_some_and(|n| n == '/' || n == '*'))
                || number_dot;
            if special {
                self.write("\\");
            }
            self.write(&c.to_string());
        }
    }
}

fn effective_lang<'a>(document: &'a Document, config: &'a Config) -> &'a str {
    document
        .meta
        .lang
        .as_deref()
        .filter(|lang| !lang.is_empty())
        .or_else(|| {
            (!config.document.lang.is_empty())
                .then_some(config.document.lang.as_str())
        })
        .unwrap_or("en")
}

fn numbering(config: &Config) -> String {
    let headings = match &config.numbering.headings {
        HeadingNumbering::Off => "false".into(),
        HeadingNumbering::Pattern(pattern) => string(pattern),
    };
    let figures = match config.numbering.figures {
        FigureNumbering::Document => "document",
        FigureNumbering::Section => "section",
    };
    let equations = match config.numbering.equations {
        EquationNumbering::Labelled => "labelled",
        EquationNumbering::All => "all",
        EquationNumbering::None => "none",
    };
    format!(
        "(headings: {headings}, figures: \"{figures}\", equations: \"{equations}\")"
    )
}

fn author(author: &ir::Author) -> String {
    if author.affiliation.is_none()
        && author.email.is_none()
        && author.id.is_none()
    {
        return string(&author.name);
    }
    let mut fields = vec![format!("name: {}", string(&author.name))];
    for (key, value) in [
        ("affiliation", &author.affiliation),
        ("email", &author.email),
        ("id", &author.id),
    ] {
        if let Some(value) = value {
            fields.push(format!("{key}: {}", string(value)));
        }
    }
    format!("({})", fields.join(", "))
}

pub(crate) fn string(text: &str) -> String {
    let mut result = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{{{:x}}}", c as u32))
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn array(items: &[String]) -> String {
    if items.len() == 1 {
        format!("({},)", items[0])
    } else {
        format!("({})", items.join(", "))
    }
}

fn math_literal(text: &str) -> String {
    if text.contains('`') {
        string(text)
    } else {
        format!("`{text}`")
    }
}

fn longest_backticks(text: &str) -> usize {
    text.split(|c| c != '`').map(str::len).max().unwrap_or_default()
}

fn lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn class(attrs: &Attrs, name: &str) -> bool {
    attrs.classes.iter().any(|class| class == name)
}

fn alignment(align: Align) -> &'static str {
    match align {
        Align::Default => "auto",
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
    }
}

fn decimal(value: f64) -> String {
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn dimension(value: &str) -> String {
    let number = value.strip_suffix("px").unwrap_or(value);
    if let Ok(number) = number.trim().parse::<f64>() {
        format!("{}pt", decimal(number * 0.75))
    } else {
        value.into()
    }
}

fn cite_form(mode: CiteMode) -> &'static str {
    match mode {
        CiteMode::Normal => "\"normal\"",
        CiteMode::Prose => "\"prose\"",
        CiteMode::Year => "\"year\"",
        CiteMode::Author => "\"author\"",
        CiteMode::Full => "\"full\"",
        CiteMode::None => "none",
    }
}

fn label_character(c: char) -> bool {
    typst_syntax::is_id_continue(c) || matches!(c, '-' | ':' | '.')
}

fn citation_space(inline: &Inline) -> bool {
    matches!(inline, Inline::SoftBreak)
        || matches!(inline, Inline::Text { text } if text.chars().all(|c| matches!(c, ' ' | '\t')))
}

fn edge(inline: &Inline, first: bool) -> Option<char> {
    match inline {
        Inline::Text { text }
        | Inline::Code { text, .. }
        | Inline::Raw { text, .. } => {
            if first {
                text.chars().next()
            } else {
                text.chars().next_back()
            }
        }
        Inline::Emph { inlines }
        | Inline::Strong { inlines }
        | Inline::Strikeout { inlines }
        | Inline::Subscript { inlines }
        | Inline::Superscript { inlines }
        | Inline::Underline { inlines }
        | Inline::SmallCaps { inlines }
        | Inline::Span { inlines, .. }
        | Inline::Link { inlines, .. } => {
            if first {
                inlines.iter().find_map(|i| edge(i, first))
            } else {
                inlines.iter().rev().find_map(|i| edge(i, first))
            }
        }
        Inline::SoftBreak | Inline::LineBreak => Some(' '),
        _ => None,
    }
}

/// Reserves the same referenced asset names used by complete and split exports.
pub(crate) fn export_asset_paths(
    document: &Document, registry: &AssetRegistry, dir: &str,
) -> BTreeMap<String, String> {
    crate::assets::export::assets(document, registry)
        .typst
        .into_iter()
        .map(|(id, name)| (id, format!("{}/{name}", dir.trim_end_matches('/'))))
        .collect()
}

/// Allocates a portable resource basename; occupied keys are ASCII-lowercase.
pub(crate) fn export_name(
    hint: &str, suffix_key: &str, occupied: &mut BTreeSet<String>,
) -> String {
    crate::assets::export::export_name(hint, suffix_key, occupied)
}

fn asset_paths(
    document: &Document, registry: &AssetRegistry, mode: &AssetPaths,
) -> BTreeMap<String, String> {
    if let AssetPaths::Export { dir } = mode {
        return export_asset_paths(document, registry, dir);
    }
    let mut referenced = BTreeSet::new();
    visit_rendered(document, &mut |_| {}, &mut |inline| {
        if let Inline::Image { url, .. } | Inline::Link { url, .. } = inline
            && let Some(id) = url.strip_prefix("asset:")
        {
            referenced
                .insert(id.split(['?', '#']).next().unwrap_or(id).to_string());
        }
    });
    // File assets are canonicalized during registration. Source paths must
    // use the same directory identity, including /tmp and other symlink roots.
    let source_base = match mode {
        AssetPaths::Source => resolve_path(&document.source.base_dir),
        _ => document.source.base_dir.clone(),
    };
    let mut paths = BTreeMap::new();
    for asset in registry.iter().filter(|asset| referenced.contains(&asset.id))
    {
        let path = match mode {
            AssetPaths::Virtual => virtual_path(asset),
            AssetPaths::Export { .. } => unreachable!("handled above"),
            AssetPaths::Source => match &asset.data {
                AssetData::File(path) => relative_path(path, &source_base),
                AssetData::Bytes(_) => virtual_path(asset),
            },
        };
        paths.insert(asset.id.clone(), path);
    }
    paths
}

fn virtual_path(asset: &Asset) -> String {
    let extension = match asset.mime.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "application/pdf" => "pdf",
        _ => "bin",
    };
    format!("/__assets/{}.{extension}", asset.id)
}

fn relative_path(path: &Path, base: &Path) -> String {
    let target: Vec<_> = path.components().collect();
    let source: Vec<_> = base.components().collect();
    let shared = target.iter().zip(&source).take_while(|(a, b)| a == b).count();
    let result = if shared == 0 && path.is_absolute() {
        path.to_path_buf()
    } else {
        let mut result = PathBuf::new();
        for component in &source[shared..] {
            if matches!(component, Component::Normal(_)) {
                result.push("..");
            }
        }
        for component in &target[shared..] {
            result.push(component.as_os_str());
        }
        result
    };
    result.to_string_lossy().replace('\\', "/")
}

fn has_latex(document: &Document) -> bool {
    let mut block_math = false;
    let mut inline_math = false;
    visit_rendered(
        document,
        &mut |block| {
            block_math |= matches!(
                block,
                Block::Math { dialect: MathDialect::Latex, .. }
            );
        },
        &mut |inline| {
            inline_math |= matches!(
                inline,
                Inline::Math { dialect: MathDialect::Latex, .. }
            );
        },
    );
    block_math || inline_math
}

fn visit_rendered(
    document: &Document, block: &mut dyn FnMut(&Block),
    inline: &mut dyn FnMut(&Inline),
) {
    if let Some(abstract_) = &document.meta.abstract_ {
        visit_blocks(abstract_, block, inline);
    }
    visit_blocks(&document.blocks, block, inline);
}

fn visit_blocks(
    blocks: &[Block], on_block: &mut dyn FnMut(&Block),
    on_inline: &mut dyn FnMut(&Inline),
) {
    for block in blocks {
        on_block(block);
        match block {
            Block::Heading { inlines, .. }
            | Block::Paragraph { inlines, .. } => {
                visit_inlines(inlines, on_block, on_inline)
            }
            Block::BlockQuote { blocks, .. } | Block::Div { blocks, .. } => {
                visit_blocks(blocks, on_block, on_inline)
            }
            Block::Figure { body, caption, .. } => {
                visit_blocks(body, on_block, on_inline);
                if let Some(caption) = caption {
                    visit_inlines(caption, on_block, on_inline);
                }
            }
            Block::Callout { title, blocks, .. } => {
                if let Some(title) = title {
                    visit_inlines(title, on_block, on_inline);
                }
                visit_blocks(blocks, on_block, on_inline);
            }
            Block::List { items, .. } => {
                for item in items {
                    visit_blocks(&item.blocks, on_block, on_inline);
                }
            }
            Block::DefinitionList { items, .. } => {
                for item in items {
                    visit_inlines(&item.term, on_block, on_inline);
                    for blocks in &item.definitions {
                        visit_blocks(blocks, on_block, on_inline);
                    }
                }
            }
            Block::Table { head, body, foot, .. } => {
                for row in head.iter().chain(body).chain(foot) {
                    for cell in &row.cells {
                        visit_inlines(&cell.inlines, on_block, on_inline);
                    }
                }
            }
            Block::Cell(cell) if cell.show_output => {
                for output in &cell.outputs {
                    visit_blocks(output.pick("typst"), on_block, on_inline);
                }
            }
            _ => {}
        }
    }
}

fn visit_inlines(
    inlines: &[Inline], on_block: &mut dyn FnMut(&Block),
    on_inline: &mut dyn FnMut(&Inline),
) {
    for inline in inlines {
        on_inline(inline);
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
                visit_inlines(inlines, on_block, on_inline)
            }
            Inline::Footnote { blocks, .. } => {
                visit_blocks(blocks, on_block, on_inline)
            }
            Inline::Ref { supplement, resolved, .. } => {
                if let Some(inlines) = supplement {
                    visit_inlines(inlines, on_block, on_inline);
                }
                if let Some(resolved) = resolved {
                    visit_inlines(&resolved.text, on_block, on_inline);
                }
            }
            Inline::Cite { items, .. } => {
                for item in items {
                    if let Some(prefix) = &item.prefix {
                        visit_inlines(prefix, on_block, on_inline);
                    }
                    if let Some(suffix) = &item.suffix {
                        visit_inlines(suffix, on_block, on_inline);
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct NoteLabels {
    definitions: BTreeMap<usize, String>,
    numbers: BTreeMap<u32, String>,
    scopes: BTreeMap<(PathBuf, String), String>,
    used: BTreeMap<String, usize>,
    final_labels: BTreeSet<String>,
}

impl NoteLabels {
    fn collect(document: &Document, prefix: &str) -> Self {
        let mut labels = Self::default();
        if let Some(blocks) = &document.meta.abstract_ {
            labels.blocks(blocks, &document.source.path, document, prefix);
        }
        labels
            .blocks(&document.blocks, &document.source.path, document, prefix);
        labels
    }

    fn blocks(
        &mut self, blocks: &[Block], parent: &Path, document: &Document,
        prefix: &str,
    ) {
        for block in blocks {
            let scope = block.span().map_or_else(
                || parent.to_path_buf(),
                |span| {
                    span.file
                        .clone()
                        .unwrap_or_else(|| document.source.path.clone())
                },
            );
            match block {
                Block::Heading { inlines, .. }
                | Block::Paragraph { inlines, .. } => {
                    self.inlines(inlines, &scope, document, prefix)
                }
                Block::Figure { body, caption, kind, .. } => {
                    if matches!(kind, FigureKind::Table | FigureKind::Listing)
                        && let Some(caption) = caption
                    {
                        self.inlines(caption, &scope, document, prefix);
                    }
                    self.blocks(body, &scope, document, prefix);
                    if matches!(kind, FigureKind::Image | FigureKind::Other)
                        && let Some(caption) = caption
                    {
                        self.inlines(caption, &scope, document, prefix);
                    }
                }
                Block::BlockQuote { blocks, .. }
                | Block::Div { blocks, .. } => {
                    self.blocks(blocks, &scope, document, prefix)
                }
                Block::Callout { title, blocks, .. } => {
                    if let Some(title) = title {
                        self.inlines(title, &scope, document, prefix);
                    }
                    self.blocks(blocks, &scope, document, prefix);
                }
                Block::List { items, .. } => {
                    for item in items {
                        self.blocks(&item.blocks, &scope, document, prefix);
                    }
                }
                Block::DefinitionList { items, .. } => {
                    for item in items {
                        self.inlines(&item.term, &scope, document, prefix);
                        for blocks in &item.definitions {
                            self.blocks(blocks, &scope, document, prefix);
                        }
                    }
                }
                Block::Table { head, body, foot, .. } => {
                    for row in head.iter().chain(body).chain(foot) {
                        for cell in &row.cells {
                            self.inlines(
                                &cell.inlines, &scope, document, prefix,
                            );
                        }
                    }
                }
                Block::Cell(cell) if cell.show_output => {
                    for output in &cell.outputs {
                        self.blocks(
                            output.pick("typst"),
                            &scope,
                            document,
                            prefix,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn inlines(
        &mut self, inlines: &[Inline], scope: &Path, document: &Document,
        prefix: &str,
    ) {
        for inline in inlines {
            match inline {
                Inline::Footnote { blocks, label, number } => {
                    if let Some(label) = label {
                        let kind = if scope == document.source.path {
                            Some(document.source.kind)
                        } else {
                            ir::detect_kind(scope).ok()
                        };
                        let base = generated_note_label(label, prefix, kind);
                        let count = self.used.entry(base.clone()).or_default();
                        *count += 1;
                        let mut final_label = if *count == 1 {
                            base.clone()
                        } else {
                            format!("{base}-{count}")
                        };
                        while !self.final_labels.insert(final_label.clone()) {
                            *count += 1;
                            final_label = format!("{base}-{count}");
                        }
                        self.definitions.insert(
                            inline as *const Inline as usize,
                            final_label.clone(),
                        );
                        if let Some(number) = number {
                            self.numbers
                                .entry(*number)
                                .or_insert_with(|| final_label.clone());
                        }
                        self.scopes
                            .entry((scope.to_path_buf(), label.clone()))
                            .or_insert(final_label);
                    }
                    self.blocks(blocks, scope, document, prefix);
                }
                Inline::Emph { inlines }
                | Inline::Strong { inlines }
                | Inline::Strikeout { inlines }
                | Inline::Subscript { inlines }
                | Inline::Superscript { inlines }
                | Inline::Underline { inlines }
                | Inline::SmallCaps { inlines }
                | Inline::Link { inlines, .. }
                | Inline::Span { inlines, .. } => {
                    self.inlines(inlines, scope, document, prefix)
                }
                Inline::Ref { supplement: Some(inlines), .. } => {
                    self.inlines(inlines, scope, document, prefix)
                }
                Inline::Cite { items, .. } => {
                    for item in items {
                        if let Some(prefix_inlines) = &item.prefix {
                            self.inlines(
                                prefix_inlines, scope, document, prefix,
                            );
                        }
                        if let Some(suffix) = &item.suffix {
                            self.inlines(suffix, scope, document, prefix);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn generated_note_label(
    label: &str, prefix: &str, kind: Option<SourceKind>,
) -> String {
    if kind == Some(SourceKind::Typst) {
        label.into()
    } else {
        let name: String = label
            .chars()
            .map(|c| if label_character(c) { c } else { '_' })
            .collect();
        format!("writer-fn-{prefix}{name}")
    }
}
