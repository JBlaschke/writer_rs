//! Markdown AST to document IR (markdown-dialect.md §2, §4–§6).
//! Owner: T12. Module skeleton created by T00.

use std::cell::{OnceCell, RefCell};
use std::collections::hash_map::Entry as HashEntry;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};

use crate::assets::HtmlScan;
use crate::assets::registry::{AssetRegistry, normalize_lexically};
use crate::config::{Config, Toc};
use crate::error::{Diagnostic, Diagnostics, Error, Result};
use crate::ir::{
    Align, Attrs, Block, ColumnSpec, DefinitionItem, FigureKind, Inline,
    Inlines, ListItem, ListKind, MathDialect, Row, SourceSpan, TableCell,
};
use crate::markdown::attrs::{
    ElementAttrs, ParsedAttrs, ir_attrs, parse_attr_block,
};
use crate::markdown::code::check_code_attrs;
use crate::markdown::directives::{DirectiveInfo, parse_directive_info};
use crate::markdown::recover::{UNPARSED_ATTR_BLOCK, fence_attrs};

/// Walks a parsed and recovered fragment, retaining the original source lines.
pub(crate) fn walk<'a>(
    root: &'a AstNode<'a>, original: &str, config: &Config,
    ctx: &mut super::Fragment<'_>, top_level: bool,
) -> Result<Vec<Block>> {
    walk_with_includes(root, original, config, ctx, top_level, None)
}

/// The document-only include state does not alter public fragment literals.
pub(crate) fn walk_with_includes<'a>(
    root: &'a AstNode<'a>, original: &str, config: &Config,
    ctx: &mut super::Fragment<'_>, top_level: bool,
    includes: Option<&mut super::include::State>,
) -> Result<Vec<Block>> {
    let mut walker = Walker {
        config,
        ctx,
        lines: super::source_lines(original),
        raw_lines: super::recover::split_lines(original).1,
        footnotes: BTreeMap::new(),
        emitted_footnotes: BTreeSet::new(),
        span_override: None,
        figure_depth: 0,
        bibliography_seen: false,
        html_scan: HtmlScan::default(),
        includes,
    };
    walker.collect_footnotes(root);
    walker.children(root, top_level)
}

struct Walker<'a, 't, 'w, 'f> {
    config: &'w Config,
    ctx: &'w mut super::Fragment<'f>,
    lines: Vec<&'t str>,
    raw_lines: Vec<&'t str>,
    footnotes: BTreeMap<String, &'a AstNode<'a>>,
    emitted_footnotes: BTreeSet<String>,
    span_override: Option<SourceSpan>,
    figure_depth: usize,
    bibliography_seen: bool,
    html_scan: HtmlScan,
    includes: Option<&'w mut super::include::State>,
}

impl<'a> Walker<'a, '_, '_, '_> {
    fn span(&self, node: &AstNode<'_>) -> SourceSpan {
        if let Some(span) = &self.span_override {
            return span.clone();
        }
        let pos = node.data().sourcepos;
        let start = pos.start.line.max(1);
        SourceSpan {
            start_line: start,
            end_line: pos
                .end
                .line
                .saturating_sub(usize::from(pos.end.column == 0))
                .max(start),
            cell: self.ctx.cell,
            file: self.ctx.file.map(Path::to_path_buf),
        }
    }

    fn report(&mut self, mut diagnostic: Diagnostic, span: &SourceSpan) {
        diagnostic.span = Some(span.clone());
        diagnostic.path = self.ctx.file.map(Path::to_path_buf);
        self.ctx.diagnostics.push(diagnostic);
    }

    fn warn(
        &mut self, code: &'static str, message: impl Into<String>,
        span: &SourceSpan,
    ) {
        self.report(Diagnostic::warning(code, message), span);
    }

    fn attrs(&mut self, parsed: ParsedAttrs, span: &SourceSpan) -> Attrs {
        let mut diagnostics = Diagnostics::new();
        let attrs = ir_attrs(parsed, Some(span), &mut diagnostics);
        for diagnostic in diagnostics.into_vec() {
            self.report(diagnostic, span);
        }
        attrs
    }

    fn node_attrs(&mut self, node: &AstNode<'_>, span: &SourceSpan) -> Attrs {
        let parsed = node
            .data()
            .attrs
            .as_deref()
            .map(ParsedAttrs::from)
            .unwrap_or_default();
        self.attrs(parsed, span)
    }

    fn collect_footnotes(&mut self, root: &'a AstNode<'a>) {
        for node in root.descendants() {
            let data = node.data();
            if let NodeValue::FootnoteDefinition(definition) = &data.value {
                self.footnotes.insert(definition.name.clone(), node);
                if definition.total_references == 0 {
                    let span = self.span(node);
                    self.warn(
                        "footnote-unreferenced",
                        format!(
                            "footnote `{}` is never referenced",
                            definition.name
                        ),
                        &span,
                    );
                }
            }
        }
    }

    fn children(
        &mut self, parent: &'a AstNode<'a>, top_level: bool,
    ) -> Result<Vec<Block>> {
        let nodes: Vec<_> = parent.children().collect();
        let mut captions = BTreeMap::new();
        for (index, node) in nodes.iter().enumerate() {
            if !matches!(node.data().value, NodeValue::Table(_)) {
                continue;
            }
            let before =
                index.checked_sub(1).filter(|&i| is_table_caption(nodes[i]));
            let after = (index + 1 < nodes.len()
                && is_table_caption(nodes[index + 1])
                && !nodes.get(index + 2).is_some_and(|n| {
                    matches!(n.data().value, NodeValue::Table(_))
                }))
            .then_some(index + 1);
            if let Some(caption) = before.or(after) {
                captions.insert(index, caption);
            }
        }
        let bound: BTreeSet<_> = captions.values().copied().collect();
        let mut blocks = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            if bound.contains(&index) {
                continue;
            }
            if let Some(&caption) = captions.get(&index) {
                blocks.push(self.captioned_table(node, nodes[caption])?);
            } else {
                if is_table_caption(node) {
                    let span = self.span(node);
                    self.warn(
                        "table-caption-orphan",
                        "`Table:` caption has no adjacent table",
                        &span,
                    );
                }
                blocks.extend(self.block(node, top_level)?);
            }
        }
        Ok(blocks)
    }

    fn block(
        &mut self, node: &'a AstNode<'a>, top_level: bool,
    ) -> Result<Vec<Block>> {
        let span = self.span(node);
        let data = node.data();
        let block = match &data.value {
            NodeValue::Document => return self.children(node, top_level),
            NodeValue::FrontMatter(_) | NodeValue::FootnoteDefinition(_) => {
                return Ok(Vec::new());
            }
            NodeValue::Paragraph => return self.paragraph(node, &span),
            NodeValue::Heading(heading) => {
                let mut attrs = self.node_attrs(node, &span);
                self.drop_presenter_attrs(&mut attrs, &span);
                Block::Heading {
                    level: heading.level,
                    inlines: self.inlines(node, &span)?,
                    attrs,
                    number: None,
                    span: Some(span),
                }
            }
            NodeValue::BlockQuote => Block::BlockQuote {
                blocks: self.children(node, false)?,
                span: Some(span),
            },
            NodeValue::Alert(alert) => {
                let title = alert
                    .title
                    .as_deref()
                    .map(|title| self.nested_inlines(title, &span))
                    .transpose()?;
                Block::Callout {
                    kind: alert.alert_type.default_title().to_ascii_lowercase(),
                    title,
                    blocks: self.children(node, false)?,
                    attrs: Attrs::default(),
                    span: Some(span),
                }
            }
            NodeValue::List(list) => {
                return self
                    .list(node, list.list_type, list.start, list.tight, span);
            }
            NodeValue::DescriptionList => return self.definitions(node, span),
            NodeValue::CodeBlock(code) => {
                return self.code_block(node, code, span);
            }
            NodeValue::Table(_) => self.table(node, &span)?,
            NodeValue::ThematicBreak => {
                Block::ThematicBreak { span: Some(span) }
            }
            NodeValue::HtmlBlock(html) if html.block_type == 2 => {
                return Ok(Vec::new());
            }
            NodeValue::HtmlBlock(html) => {
                let mut text = html.literal.clone();
                rewrite_html_snippet(
                    &mut text,
                    Some(&span),
                    self.ctx,
                    &mut self.html_scan,
                )?;
                Block::Raw { format: "html".into(), text, span: Some(span) }
            }
            NodeValue::BlockDirective(directive) => {
                return self.directive(node, &directive.info, span, top_level);
            }
            unexpected => return Err(unexpected_node(unexpected)),
        };
        Ok(vec![block])
    }

    fn paragraph(
        &mut self, node: &'a AstNode<'a>, span: &SourceSpan,
    ) -> Result<Vec<Block>> {
        if span.start_line == span.end_line
            && self
                .lines
                .get(span.start_line - 1)
                .is_some_and(|line| presenter_separator(line))
        {
            self.report(
                Diagnostic::info(
                    "presenter-separator", "presenter separator is omitted",
                ),
                span,
            );
            return Ok(Vec::new());
        }
        if let Some(block) = self.display_math(node, span)? {
            return Ok(vec![block]);
        }
        self.misplaced_equation_label(node, span);
        let mut inlines = self.inlines(node, span)?;
        if let [Inline::Image { alt, attrs, .. }] = inlines.as_mut_slice()
            && (attrs.id.is_some()
                || (self.figure_depth == 0 && !alt.is_empty()))
        {
            let caption = (!alt.is_empty()).then(|| alt.clone());
            remove_alt_footnotes(alt);
            let attrs = Attrs {
                id: attrs.id.take(),
                classes: std::mem::take(&mut attrs.classes),
                pairs: BTreeMap::new(),
            };
            return Ok(vec![Block::Figure {
                kind: FigureKind::Image,
                caption,
                attrs,
                number: None,
                span: Some(span.clone()),
                body: vec![Block::Paragraph {
                    inlines,
                    span: Some(span.clone()),
                }],
            }]);
        }
        Ok(vec![Block::Paragraph { inlines, span: Some(span.clone()) }])
    }

    fn list(
        &mut self, node: &'a AstNode<'a>, kind: ListType, start: usize,
        tight: bool, span: SourceSpan,
    ) -> Result<Vec<Block>> {
        let kind = match kind {
            ListType::Bullet => ListKind::Bullet,
            ListType::Ordered => ListKind::Ordered { start: start as u64 },
        };
        let mut items = Vec::new();
        for item in node.children() {
            let checked = match &item.data().value {
                NodeValue::TaskItem(task) => Some(task.symbol.is_some()),
                NodeValue::Item(_) => None,
                unexpected => return Err(unexpected_node(unexpected)),
            };
            items.push(ListItem {
                checked,
                blocks: self.children(item, false)?,
            });
        }
        let tight = tight
            && items.iter().all(|item| {
                item.blocks.iter().all(|block| {
                    matches!(
                        block,
                        Block::Paragraph { .. }
                            | Block::List { .. }
                            | Block::CodeBlock { .. }
                    )
                })
            });
        Ok(vec![Block::List { kind, tight, items, span: Some(span) }])
    }

    fn definitions(
        &mut self, node: &'a AstNode<'a>, span: SourceSpan,
    ) -> Result<Vec<Block>> {
        let mut items: Vec<DefinitionItem> = Vec::new();
        for item in node.children() {
            let mut term = Vec::new();
            let mut definitions = Vec::new();
            let mut has_term = false;
            for child in item.children() {
                match child.data().value {
                    NodeValue::DescriptionTerm => {
                        has_term = true;
                        if let Some(paragraph) = child.first_child() {
                            term =
                                self.inlines(paragraph, &self.span(paragraph))?;
                        }
                    }
                    NodeValue::DescriptionDetails => {
                        definitions.push(self.children(child, false)?)
                    }
                    ref unexpected => return Err(unexpected_node(unexpected)),
                }
            }
            if !has_term && let Some(previous) = items.last_mut() {
                previous.definitions.extend(definitions);
                continue;
            }
            items.push(DefinitionItem { term, definitions });
        }
        Ok(vec![Block::DefinitionList { items, span: Some(span) }])
    }

    fn display_math(
        &mut self, node: &'a AstNode<'a>, span: &SourceSpan,
    ) -> Result<Option<Block>> {
        let children: Vec<_> = node.children().collect();
        let Some(first) = children.first() else {
            return Ok(None);
        };
        let data = first.data();
        let NodeValue::Math(math) = &data.value else {
            return Ok(None);
        };
        if !math.display_math {
            return Ok(None);
        }
        let mut trailing = &children[1..];
        if trailing
            .first()
            .is_some_and(|n| matches!(n.data().value, NodeValue::SoftBreak))
        {
            trailing = &trailing[1..];
        }
        let parsed = match trailing {
            [] => Some(ParsedAttrs::default()),
            [label] => match &label.data().value {
                NodeValue::Text(text) if attribute_text(text) => {
                    Some(self.parse_attrs(text, span))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(parsed) = parsed {
            return Ok(Some(Block::Math {
                source: math.literal.trim().into(),
                dialect: MathDialect::Latex,
                attrs: self.attrs(parsed, span),
                number: None,
                span: Some(span.clone()),
            }));
        }
        Ok(None)
    }

    fn misplaced_equation_label(
        &mut self, node: &'a AstNode<'a>, span: &SourceSpan,
    ) {
        let math = node.children().any(|n| matches!(&n.data().value, NodeValue::Math(math) if math.display_math));
        let label = node.children().any(|n| matches!(&n.data().value, NodeValue::Text(text) if text.contains("{#")));
        if math && label {
            self.warn(
                "equation-label-misplaced",
                "place the equation and its label in a paragraph of their own",
                span,
            );
        }
    }

    fn parse_attrs(&mut self, text: &str, span: &SourceSpan) -> ParsedAttrs {
        match parse_attr_block(text) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.warn(UNPARSED_ATTR_BLOCK, error.to_string(), span);
                ParsedAttrs::default()
            }
        }
    }

    fn table(
        &mut self, node: &'a AstNode<'a>, span: &SourceSpan,
    ) -> Result<Block> {
        let data = node.data();
        let NodeValue::Table(table) = &data.value else {
            return Err(unexpected_node(&data.value));
        };
        let columns = table
            .alignments
            .iter()
            .map(|align| ColumnSpec {
                align: match align {
                    TableAlignment::None => Align::Default,
                    TableAlignment::Left => Align::Left,
                    TableAlignment::Center => Align::Center,
                    TableAlignment::Right => Align::Right,
                },
                width: None,
            })
            .collect();
        let mut head = Vec::new();
        let mut body = Vec::new();
        for row in node.children() {
            let header = match row.data().value {
                NodeValue::TableRow(header) => header,
                ref unexpected => return Err(unexpected_node(unexpected)),
            };
            let mut cells = Vec::new();
            for cell in row.children() {
                cells.push(TableCell {
                    inlines: self.inlines(cell, &self.span(cell))?,
                    header,
                    ..TableCell::default()
                });
            }
            let rows = if header { &mut head } else { &mut body };
            rows.push(Row { cells });
        }
        if body.last().and_then(|row| row.cells.first()).is_some_and(|cell| {
            matches!(cell.inlines.first(), Some(Inline::Text { text }) if text.starts_with(": "))
        }) {
            self.warn("table-caption-pandoc-colon", "use a separate `Table:` paragraph for a table caption", span);
        }
        Ok(Block::Table {
            columns,
            head,
            body,
            foot: Vec::new(),
            attrs: self.node_attrs(node, span),
            span: Some(span.clone()),
        })
    }

    fn captioned_table(
        &mut self, table: &'a AstNode<'a>, paragraph: &'a AstNode<'a>,
    ) -> Result<Block> {
        let mut span = self.span(table);
        let caption_span = self.span(paragraph);
        span.start_line = span.start_line.min(caption_span.start_line);
        span.end_line = span.end_line.max(caption_span.end_line);
        let mut caption = self.inlines(paragraph, &caption_span)?;
        if let Some(Inline::Text { text }) = caption.first_mut() {
            *text =
                text.strip_prefix("Table:").unwrap_or(text).trim_start().into();
        }
        let parsed = self.caption_attrs(&mut caption, &caption_span);
        caption.retain(|inline| !matches!(inline, Inline::Text { text } if text.is_empty()));
        let caption = (!caption.is_empty()).then_some(caption);
        let attrs = self.attrs(parsed, &caption_span);
        let table = self.table(table, &self.span(table))?;
        if caption.is_none() && attrs.id.is_none() {
            return Ok(table);
        }
        Ok(Block::Figure {
            kind: FigureKind::Table,
            caption,
            body: vec![table],
            attrs,
            number: None,
            span: Some(span),
        })
    }

    fn caption_attrs(
        &mut self, caption: &mut Inlines, span: &SourceSpan,
    ) -> ParsedAttrs {
        let Some(Inline::Text { text }) = caption.last_mut() else {
            return ParsedAttrs::default();
        };
        let trimmed = text.trim_end();
        if !trimmed.ends_with('}') {
            return ParsedAttrs::default();
        }
        let mut problem = None;
        for (start, _) in trimmed.match_indices('{').rev() {
            match parse_attr_block(&trimmed[start..]) {
                Ok(parsed) => {
                    text.truncate(start);
                    *text = text.trim_end().into();
                    return parsed;
                }
                Err(error) => {
                    problem.get_or_insert(error);
                }
            }
        }
        if let Some(error) = problem {
            self.warn(UNPARSED_ATTR_BLOCK, error.to_string(), span);
        }
        ParsedAttrs::default()
    }

    fn code_block(
        &mut self, node: &'a AstNode<'a>, code: &comrak::nodes::NodeCodeBlock,
        span: SourceSpan,
    ) -> Result<Vec<Block>> {
        if let Some(format) = raw_format(&code.info) {
            self.check_raw_format(&format, &span);
            return Ok(vec![Block::Raw {
                format,
                text: code.literal.clone(),
                span: Some(span),
            }]);
        }
        let mut info = code.info.as_str();
        let mut attrs = self.node_attrs(node, &span);
        if code.fenced
            && node.data().attrs.is_none()
            && let Some((rest, _, reading)) = fence_attrs(info)
        {
            match reading {
                ElementAttrs::Attrs(parsed) => {
                    attrs = self.attrs(parsed, &span);
                    info = rest;
                }
                ElementAttrs::Invalid(reason) => {
                    self.warn(UNPARSED_ATTR_BLOCK, reason, &span)
                }
                ElementAttrs::Text => {}
            }
        }
        let lang = code
            .fenced
            .then(|| info.split_whitespace().next().map(str::to_string))
            .flatten();
        if lang.as_deref() == Some("math") {
            return Ok(vec![Block::Math {
                source: code.literal.trim().into(),
                dialect: MathDialect::Latex,
                attrs,
                number: None,
                span: Some(span),
            }]);
        }
        let mut diagnostics = Diagnostics::new();
        check_code_attrs(&attrs, Some(&span), &mut diagnostics);
        for diagnostic in diagnostics.into_vec() {
            self.report(diagnostic, &span);
        }
        let caption = attrs
            .pairs
            .remove("caption")
            .map(|text| self.nested_inlines(&text, &span))
            .transpose()?;
        let id = attrs.id.take();
        let block = Block::CodeBlock {
            lang,
            text: code
                .literal
                .strip_suffix('\n')
                .unwrap_or(&code.literal)
                .into(),
            attrs,
            span: Some(span.clone()),
        };
        if caption.is_some() || id.is_some() {
            return Ok(vec![Block::Figure {
                kind: FigureKind::Listing,
                caption,
                body: vec![block],
                attrs: Attrs { id, ..Attrs::default() },
                number: None,
                span: Some(span),
            }]);
        }
        Ok(vec![block])
    }

    fn check_raw_format(&mut self, format: &str, span: &SourceSpan) {
        if !matches!(
            format,
            "html" | "latex" | "typst" | "openxml" | "markdown"
        ) {
            self.warn(
                "raw-unknown-format",
                format!("unknown raw format `{format}`"),
                span,
            );
        }
    }

    fn drop_presenter_attrs(&mut self, attrs: &mut Attrs, span: &SourceSpan) {
        attrs.pairs.retain(|key, _| {
            let key = key.to_ascii_lowercase();
            let dropped = matches!(
                key.as_str(),
                "layout"
                    | "background"
                    | "transition"
                    | "transition-speed"
                    | "visibility"
                    | "state"
                    | "timing"
                    | "autoslide"
                    | "menu-title"
            ) || key.starts_with("background-")
                || key.starts_with("auto-animate");
            if dropped {
                self.warn(
                    "presenter-attr-dropped",
                    format!("presenter heading attribute `{key}` is omitted"),
                    span,
                );
            }
            !dropped
        });
    }

    fn nested_inlines(
        &mut self, text: &str, span: &SourceSpan,
    ) -> Result<Inlines> {
        let mut diagnostics = Diagnostics::new();
        let mut ctx = super::Fragment {
            assets: self.ctx.assets,
            base_dir: self.ctx.base_dir,
            diagnostics: &mut diagnostics,
            file: self.ctx.file,
            cell: self.ctx.cell,
            missing_asset_is_error: self.ctx.missing_asset_is_error,
        };
        let result = super::parse_inlines(text, self.config, &mut ctx);
        for diagnostic in diagnostics.into_vec() {
            self.report(diagnostic, span);
        }
        result.map_err(|error| match error {
            Error::MissingAsset { path, .. } => {
                Error::MissingAsset { path, span: Some(span.clone()) }
            }
            other => other,
        })
    }

    fn directive(
        &mut self, node: &'a AstNode<'a>, text: &str, span: SourceSpan,
        top_level: bool,
    ) -> Result<Vec<Block>> {
        let mut info = parse_directive_info(text);
        if matches!(info.name.as_str(), "notes" | "comment") {
            return Ok(Vec::new());
        }
        if info.name == "include" {
            let start = node.data().sourcepos.start;
            let raw = self
                .raw_lines
                .get(start.line - 1)
                .and_then(|line| {
                    super::include::raw_directive_info(line, start.column)
                })
                .unwrap_or(text)
                .to_owned();
            return self.include_directive(node, &raw, span, top_level);
        }
        for problem in &info.problems {
            self.warn("invalid-directive", problem, &span);
        }
        self.check_directive_closed(&span);
        if info.name.is_empty() {
            self.warn(
                "invalid-directive",
                "a directive needs a name; its children are kept",
                &span,
            );
            return self.children(node, false);
        }
        if info.name == "raw" {
            return self.raw_directive(node, info, span);
        }
        if info.name == "figure" {
            return self.figure_directive(node, info, span);
        }
        let mut attrs = self.directive_attrs(&info, &span);
        if matches!(info.name.as_str(), "pagebreak" | "toc" | "bibliography") {
            return self.leaf_directive(node, &info.name, &mut attrs, span);
        }
        let block = match info.name.as_str() {
            "callout" | "note" | "tip" | "important" | "warning"
            | "caution" => {
                let kind = if info.name == "callout" {
                    attrs.pairs.remove("kind").unwrap_or_else(|| {
                        if attrs.classes.is_empty() {
                            "note".into()
                        } else {
                            attrs.classes.remove(0)
                        }
                    })
                } else {
                    info.name
                };
                let title = attrs
                    .pairs
                    .remove("title")
                    .map(|text| self.nested_inlines(&text, &span))
                    .transpose()?;
                Block::Callout {
                    kind,
                    title,
                    blocks: self.children(node, false)?,
                    attrs,
                    span: Some(span),
                }
            }
            _ => {
                attrs.classes.insert(0, std::mem::take(&mut info.name));
                Block::Div {
                    attrs,
                    blocks: self.children(node, false)?,
                    span: Some(span),
                }
            }
        };
        Ok(vec![block])
    }

    fn directive_attrs(
        &mut self, info: &DirectiveInfo, span: &SourceSpan,
    ) -> Attrs {
        self.attrs(
            ParsedAttrs {
                id: info.id.clone(),
                classes: info.classes.clone(),
                pairs: info.args.clone(),
            },
            span,
        )
    }

    fn check_directive_closed(&mut self, span: &SourceSpan) {
        let closed = self
            .lines
            .get(span.end_line - 1)
            .is_some_and(|line| closing_fence(line));
        if !closed {
            self.warn(
                "unclosed-directive",
                "directive has no closing `:::` fence",
                span,
            );
        }
    }

    fn leaf_directive(
        &mut self, node: &'a AstNode<'a>, name: &str, attrs: &mut Attrs,
        span: SourceSpan,
    ) -> Result<Vec<Block>> {
        let block = match name {
            "pagebreak" => Some(Block::PageBreak { span: Some(span.clone()) }),
            "toc" => {
                let default = match self.config.document.toc {
                    Toc::Depth(depth) => depth,
                    Toc::Off => 3,
                };
                let depth = match attrs.pairs.remove("depth") {
                    None => default,
                    Some(value) => match value
                        .parse::<u8>()
                        .ok()
                        .filter(|depth| (1..=6).contains(depth))
                    {
                        Some(depth) => depth,
                        None => {
                            self.warn(
                                "invalid-directive-arg",
                                "toc depth must be an integer from 1 to 6",
                                &span,
                            );
                            default
                        }
                    },
                };
                Some(Block::Toc { depth, span: Some(span.clone()) })
            }
            "bibliography" if self.bibliography_seen => {
                self.warn(
                    "invalid-directive",
                    "only one bibliography directive is allowed",
                    &span,
                );
                None
            }
            "bibliography" => {
                self.bibliography_seen = true;
                Some(Block::Bibliography { span: Some(span.clone()) })
            }
            _ => None,
        };
        let mut blocks: Vec<_> = block.into_iter().collect();
        let body = self.children(node, false)?;
        if !body.is_empty() {
            self.warn(
                "invalid-directive",
                format!(
                    "`:::{name}` is a leaf directive; its body is kept after it"
                ),
                &span,
            );
            blocks.extend(body);
        }
        Ok(blocks)
    }

    fn figure_directive(
        &mut self, node: &'a AstNode<'a>, info: DirectiveInfo, span: SourceSpan,
    ) -> Result<Vec<Block>> {
        let mut attrs = self.directive_attrs(&info, &span);
        let explicit = attrs.pairs.remove("kind");
        let mut caption = attrs
            .pairs
            .remove("caption")
            .map(|text| self.nested_inlines(&text, &span))
            .transpose()?;
        self.figure_depth += 1;
        let body = self.children(node, false);
        self.figure_depth -= 1;
        let mut body = body?;
        if caption.is_none()
            && body.len() >= 2
            && matches!(body.last(), Some(Block::Paragraph { .. }))
            && let Some(Block::Paragraph { inlines, .. }) = body.pop()
        {
            caption = Some(inlines);
        }
        let inferred = figure_body_kind(&body);
        let mut kind = match explicit.as_deref() {
            None => inferred,
            Some("image") => FigureKind::Image,
            Some("table") => FigureKind::Table,
            Some("listing") => FigureKind::Listing,
            Some("other") => FigureKind::Other,
            Some(value) => {
                self.warn("invalid-directive-arg", format!("unknown figure kind `{value}`; use image, table, listing or other"), &span);
                FigureKind::Other
            }
        };
        if kind != FigureKind::Other && kind != inferred {
            self.warn(
                "invalid-directive-arg",
                "figure body does not match its kind; using `other`",
                &span,
            );
            kind = FigureKind::Other;
        }
        Ok(vec![Block::Figure {
            kind,
            caption,
            body,
            attrs,
            number: None,
            span: Some(span),
        }])
    }

    fn raw_directive(
        &mut self, node: &'a AstNode<'a>, info: DirectiveInfo, span: SourceSpan,
    ) -> Result<Vec<Block>> {
        let mut attrs = self.directive_attrs(&info, &span);
        let format = attrs
            .pairs
            .remove("format")
            .or_else(|| info.classes.first().cloned());
        let Some(format) = format.filter(|format| !format.is_empty()) else {
            self.warn(
                "invalid-directive-arg",
                "raw directive needs a format, e.g. `:::raw html`",
                &span,
            );
            return self.children(node, false);
        };
        let format = format.to_ascii_lowercase();
        self.check_raw_format(&format, &span);
        let closed = self
            .lines
            .get(span.end_line - 1)
            .is_some_and(|line| closing_fence(line));
        let end = if closed {
            span.end_line - 1
        } else {
            span.end_line
        };
        let mut text = String::new();
        for line in self.lines.get(span.start_line..end).unwrap_or_default() {
            text.push_str(line);
            text.push('\n');
        }
        Ok(vec![Block::Raw { format, text, span: Some(span) }])
    }

    fn include_directive(
        &mut self, node: &'a AstNode<'a>, info: &str, span: SourceSpan,
        top_level: bool,
    ) -> Result<Vec<Block>> {
        let closed = {
            let data = node.data();
            let last = self.lines.get(data.sourcepos.end.line - 1);
            match (&data.value, last) {
                (NodeValue::BlockDirective(directive), Some(last)) => {
                    let rest = last.trim_start_matches([' ', '\t']);
                    let prefix = &last.as_bytes()[..last.len() - rest.len()];
                    let indent = prefix.iter().fold(0_usize, |column, byte| {
                        column + if *byte == b'\t' { 4 - column % 4 } else { 1 }
                    });
                    let count = rest.bytes().take_while(|b| *b == b':').count();
                    data.sourcepos.start.line != data.sourcepos.end.line
                        && indent <= 3
                        && count >= directive.fence_length
                        && rest[count..]
                            .bytes()
                            .all(|b| matches!(b, b' ' | b'\t' | b'\r'))
                }
                _ => false,
            }
        };
        if !closed {
            self.warn(
                "unclosed-directive",
                "directive has no closing `:::` fence",
                &span,
            );
        }
        if !top_level || self.includes.is_none() {
            self.report(
                Diagnostic::error(
                    "include-not-top-level",
                    ":::include must be a direct child of the document",
                ),
                &span,
            );
            return Ok(Vec::new());
        }
        let Some(includes) = self.includes.as_deref_mut() else {
            return Ok(Vec::new());
        };
        let path = super::include::include_path(info)
            .map_err(|message| includes.error(&span, message))?;
        let parsed = includes.expand(&path, &span, self.config)?;
        self.ctx.assets.merge(parsed.assets);
        self.ctx.diagnostics.extend(parsed.diagnostics);
        let mut blocks = parsed.document.blocks;
        let body = self.children(node, false)?;
        if !body.is_empty() {
            self.warn(
                "invalid-directive",
                ":::include is a leaf directive; its body follows the included content",
                &span,
            );
            blocks.extend(body);
        }
        Ok(blocks)
    }

    fn inlines(
        &mut self, parent: &'a AstNode<'a>, span: &SourceSpan,
    ) -> Result<Inlines> {
        let children: Vec<_> = parent.children().collect();
        let mut out = Vec::new();
        let mut strip_next = 0;
        for (index, node) in children.iter().enumerate() {
            let data = node.data();
            if strip_next > 0 {
                if let NodeValue::Text(text) = &data.value
                    && !text[strip_next..].is_empty()
                {
                    out.extend(self.text_inlines(
                        &text[strip_next..],
                        node,
                        span,
                    )?);
                }
                strip_next = 0;
                continue;
            }
            if let NodeValue::Code(code) = &data.value
                && let Some(next) = children.get(index + 1)
                && let NodeValue::Text(text) = &next.data().value
                && let Some((format, consumed)) = raw_prefix(text)
            {
                self.check_raw_format(&format, span);
                out.push(Inline::Raw { format, text: code.literal.clone() });
                strip_next = consumed;
                continue;
            }
            out.extend(self.inline(node, span)?);
        }
        Ok(out)
    }

    fn inline(
        &mut self, node: &'a AstNode<'a>, span: &SourceSpan,
    ) -> Result<Inlines> {
        let data = node.data();
        let inline = match &data.value {
            NodeValue::Text(text) => {
                return self.text_inlines(text, node, span);
            }
            NodeValue::SoftBreak => Inline::SoftBreak,
            NodeValue::LineBreak => Inline::LineBreak,
            NodeValue::Emph => {
                Inline::Emph { inlines: self.inlines(node, span)? }
            }
            NodeValue::Strong => {
                Inline::Strong { inlines: self.inlines(node, span)? }
            }
            NodeValue::Strikethrough => {
                Inline::Strikeout { inlines: self.inlines(node, span)? }
            }
            NodeValue::Superscript => {
                Inline::Superscript { inlines: self.inlines(node, span)? }
            }
            NodeValue::Subscript => {
                Inline::Subscript { inlines: self.inlines(node, span)? }
            }
            NodeValue::Escaped => return self.inlines(node, span),
            NodeValue::Code(code) => Inline::Code {
                text: code.literal.clone(),
                attrs: self.node_attrs(node, span),
            },
            NodeValue::Math(math) => Inline::Math {
                source: math.literal.clone(),
                dialect: MathDialect::Latex,
            },
            NodeValue::Link(link) => {
                let mut attrs = self.node_attrs(node, span);
                let title = take_title(&link.title, &mut attrs);
                Inline::Link {
                    inlines: self.inlines(node, span)?,
                    url: link.url.clone(),
                    title,
                    attrs,
                }
            }
            NodeValue::Image(image) => {
                let mut attrs = self.node_attrs(node, span);
                let title = take_title(&image.title, &mut attrs);
                self.dimensions(&mut attrs, span);
                let url =
                    local_asset(&image.url, "image", Some(span), self.ctx)?
                        .unwrap_or_else(|| image.url.clone());
                Inline::Image {
                    alt: self.inlines(node, span)?,
                    url,
                    title,
                    attrs,
                }
            }
            NodeValue::FootnoteReference(reference) => {
                return self.footnote(&reference.name, span);
            }
            NodeValue::HtmlInline(html) if html.starts_with("<!--") => {
                return Ok(Vec::new());
            }
            NodeValue::HtmlInline(html) => {
                let mut text = html.clone();
                rewrite_html_snippet(
                    &mut text,
                    Some(span),
                    self.ctx,
                    &mut self.html_scan,
                )?;
                Inline::Raw { format: "html".into(), text }
            }
            unexpected => return Err(unexpected_node(unexpected)),
        };
        Ok(vec![inline])
    }

    fn text_inlines(
        &mut self, text: &str, node: &'a AstNode<'a>, span: &SourceSpan,
    ) -> Result<Inlines> {
        let excluded = node.ancestors().any(|parent| {
            matches!(
                parent.data().value,
                NodeValue::Escaped | NodeValue::Link(_)
            )
        });
        if excluded || !self.config.markdown.references {
            return Ok(vec![Inline::text(text)]);
        }
        let start_allowed = node.previous_sibling().is_none_or(|previous| {
            matches!(
                previous.data().value,
                NodeValue::SoftBreak | NodeValue::LineBreak
            )
        });
        super::refs::scan(text, start_allowed, span, self.config, self.ctx)
    }

    fn dimensions(&mut self, attrs: &mut Attrs, span: &SourceSpan) {
        for key in ["width", "height"] {
            if let Some(value) = attrs.pairs.get(key)
                && !valid_dimension(value)
            {
                self.warn("invalid-dimension", format!("invalid image {key} `{value}`; use a number with %, px, pt, cm, mm, in or em"), span);
                attrs.pairs.remove(key);
            }
        }
    }

    fn footnote(&mut self, name: &str, span: &SourceSpan) -> Result<Inlines> {
        if !self.emitted_footnotes.insert(name.to_string()) {
            return Ok(vec![Inline::FootnoteRef {
                label: name.into(),
                number: None,
            }]);
        }
        let definition =
            self.footnotes.get(name).copied().ok_or_else(|| {
                Error::Other(format!(
                    "missing parsed footnote definition `{name}`"
                ))
            })?;
        let synthetic = name.strip_prefix("__inline_").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|c| c.is_ascii_digit())
        });
        let previous_span = self.span_override.clone();
        self.span_override = synthetic.then(|| span.clone());
        let previous_depth = self.figure_depth;
        self.figure_depth = 0;
        let blocks = self.children(definition, false);
        self.figure_depth = previous_depth;
        self.span_override = previous_span;
        Ok(vec![Inline::Footnote {
            blocks: blocks?,
            label: (!synthetic).then(|| name.to_string()),
            number: None,
        }])
    }
}

fn is_table_caption(node: &AstNode<'_>) -> bool {
    matches!(node.data().value, NodeValue::Paragraph)
        && node.first_child().is_some_and(|first| matches!(&first.data().value, NodeValue::Text(text) if text.starts_with("Table:")))
}

fn presenter_separator(line: &str) -> bool {
    let line = line.trim();
    line == "--"
        || ["---", "--"].iter().any(|prefix| {
            line.strip_prefix(prefix).is_some_and(|rest| {
                let rest = rest.trim();
                rest.starts_with('{') && rest.ends_with('}')
            })
        })
}

fn attribute_text(text: &str) -> bool {
    let text = text.trim();
    text.starts_with('{')
        && text.ends_with('}')
        && !text[1..text.len() - 1].contains('}')
}

fn raw_format(info: &str) -> Option<String> {
    let rest = info.strip_prefix("{=")?.strip_suffix('}')?;
    (!rest.is_empty()
        && rest
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')))
    .then(|| rest.to_ascii_lowercase())
}

fn raw_prefix(text: &str) -> Option<(String, usize)> {
    let end = text.find('}')?;
    Some((raw_format(&text[..=end])?, end + 1))
}

fn closing_fence(line: &str) -> bool {
    let indent =
        line.len() - line.trim_start_matches(char::is_whitespace).len();
    let rest = &line[indent..];
    let fence = rest.bytes().take_while(|&c| c == b':').count();
    line[..indent].chars().count() <= 3
        && fence >= 3
        && rest[fence..].trim().is_empty()
}

fn figure_body_kind(body: &[Block]) -> FigureKind {
    match body {
        [Block::Paragraph { inlines, .. }]
            if matches!(inlines.as_slice(), [Inline::Image { .. }]) =>
        {
            FigureKind::Image
        }
        [Block::Table { .. }] => FigureKind::Table,
        [Block::CodeBlock { .. }] => FigureKind::Listing,
        _ => FigureKind::Other,
    }
}

/// Captions own notes from implicit images; alt text keeps its ordinary words.
fn remove_alt_footnotes(inlines: &mut Inlines) {
    inlines.retain(|inline| {
        !matches!(inline, Inline::Footnote { .. } | Inline::FootnoteRef { .. })
    });
    for inline in inlines {
        match inline {
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Link { inlines, .. }
            | Inline::Span { inlines, .. } => remove_alt_footnotes(inlines),
            Inline::Image { alt, .. } => remove_alt_footnotes(alt),
            Inline::Ref { supplement: Some(inlines), .. } => {
                remove_alt_footnotes(inlines);
            }
            Inline::Cite { items, .. } => {
                for item in items {
                    if let Some(prefix) = &mut item.prefix {
                        remove_alt_footnotes(prefix);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        remove_alt_footnotes(suffix);
                    }
                }
            }
            _ => {}
        }
    }
}

fn take_title(markdown: &str, attrs: &mut Attrs) -> Option<String> {
    let fallback = attrs.pairs.remove("title");
    if markdown.is_empty() {
        fallback
    } else {
        Some(markdown.into())
    }
}

fn valid_dimension(value: &str) -> bool {
    let number = value
        .strip_suffix('%')
        .or_else(|| {
            ["px", "pt", "cm", "mm", "in", "em"]
                .iter()
                .find_map(|unit| value.strip_suffix(unit))
        })
        .unwrap_or(value);
    let mut parts = number.split('.');
    let digits = |part: &str| {
        !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit())
    };
    parts.next().is_some_and(digits)
        && parts.next().is_none_or(digits)
        && parts.next().is_none()
}

fn unexpected_node(node: &NodeValue) -> Error {
    Error::Other(format!(
        "unexpected comrak node `{}` in Markdown walker",
        node.xml_node_name()
    ))
}
// Local asset helpers lifted from presenter_rs/src/markdown/render.rs (T12).
// Paths and warning semantics: markdown-dialect.md §4.3; HTML scanner: T14.

/// Diagnostic code of a missing local file.
const MISSING_ASSET: &str = "missing-asset";

/// Diagnostic code of a local file found only by letter case.
const ASSET_CASE_MISMATCH: &str = "asset-case-mismatch";

/// Separates the URL from the resolved path in a `missing-asset` message.
const LOOKED_FOR: &str = " (looked for ";

/// The missing file a `missing-asset` diagnostic of this module is about (resolved, normally
/// absolute), or `None` for any other diagnostic.
///
/// Such a diagnostic's `path` is the *source* file (the document or notebook, set by the
/// frontend) and its span the referencing lines; the missing file is only in the message,
/// whose format is frozen for this function: ``<what> `<url as written>` not found (looked
/// for <resolved path>)``. The pipeline escalates with
/// `Error::MissingAsset { path: missing_asset_path(d)?, span: d.span.clone() }`.
pub fn missing_asset_path(diagnostic: &Diagnostic) -> Option<PathBuf> {
    if diagnostic.code != MISSING_ASSET {
        return None;
    }
    let (_, path) =
        diagnostic.message.strip_suffix(')')?.rsplit_once(LOOKED_FOR)?;
    Some(PathBuf::from(path))
}

/// Rewrites relative `src` and `poster` URLs in raw HTML scanned in document order:
/// `scan` carries an open comment or raw-text element from one node to the next.
pub(crate) fn rewrite_html_snippet(
    html: &mut String, span: Option<&SourceSpan>, ctx: &mut super::Fragment,
    scan: &mut crate::assets::HtmlScan,
) -> Result<()> {
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let source: &str = html;
    scan.start_tags(source, |tag| {
        for attr in &tag.attrs {
            let Some(range) = attr.value.clone() else {
                continue;
            };
            if !matches!(attr.name.as_str(), "src" | "poster") {
                continue;
            }
            let url = html_escape::decode_html_entities(&source[range.clone()]);
            if let Some(asset) = local_asset(&url, "file", span, ctx)? {
                edits.push((
                    range,
                    html_escape::encode_quoted_attribute(&asset).into_owned(),
                ));
            }
        }
        Ok::<(), Error>(())
    })?;
    for (range, text) in edits.into_iter().rev() {
        html.replace_range(range, &text);
    }
    Ok(())
}

/// Resolves one URL (dialect §6). `Some("asset:<id>…")` when it names an existing local
/// file; `None` when it is not relative or the file is missing (then reported).
pub(crate) fn local_asset(
    url: &str, what: &str, span: Option<&SourceSpan>, ctx: &mut super::Fragment,
) -> Result<Option<String>> {
    if !is_relative_url(url) {
        return Ok(None);
    }
    let (path, suffix) =
        url.split_at(url.find(['?', '#']).unwrap_or(url.len()));
    let decoded = percent_decode(path);
    let file = ctx.base_dir.join(&decoded);
    if file.is_file() {
        if let Some(on_disk) = case_mismatch(ctx.base_dir, &decoded) {
            let message = format!(
                "{what} `{url}` matches `{on_disk}` only by letter case: it is found on this \
                 file system, but not on a case-sensitive one (Linux); write `{on_disk}`"
            );
            let diagnostic = ctx.diagnostics.warn(ASSET_CASE_MISMATCH, message);
            diagnostic.span = span.cloned();
            diagnostic.path = ctx.file.map(Path::to_path_buf);
        }
        let id = ctx.assets.register_file(&file);
        return Ok(Some(format!(
            "{}{suffix}",
            AssetRegistry::placeholder(&id)
        )));
    }
    let resolved = normalize_lexically(&file);
    if ctx.missing_asset_is_error {
        return Err(Error::MissingAsset {
            path: resolved,
            span: span.cloned(),
        });
    }
    // The format is frozen: `missing_asset_path` parses it.
    let message =
        format!("{what} `{url}` not found{LOOKED_FOR}{})", resolved.display());
    let diagnostic = ctx.diagnostics.warn(MISSING_ASSET, message);
    diagnostic.span = span.cloned();
    diagnostic.path = ctx.file.map(Path::to_path_buf);
    Ok(None)
}

/// The spelling on disk of `relative` (a path under `base` that exists) when some segment
/// of it matches the file system's entry only by letter case, which a case-insensitive file
/// system (macOS, Windows) accepts and a case-sensitive one (Linux) does not: each segment
/// is compared exactly with the names `read_dir` lists, as theme files are (A3). `.` is
/// skipped and `..` goes up unchecked. `None` when every segment matches exactly (or a
/// directory cannot be listed). A directory that finds a name by its exact spelling only
/// (case-sensitive) is not listed ([`exact_lookup`]). Listings are cached while a
/// [`DirListings`] guard is open.
fn case_mismatch(base: &Path, relative: &str) -> Option<String> {
    let mut dir = base.to_path_buf();
    let mut on_disk: Vec<String> = Vec::new();
    let mut mismatch = false;
    for component in Path::new(relative).components() {
        let name = match component {
            std::path::Component::CurDir => continue,
            std::path::Component::ParentDir => OsString::from(".."),
            std::path::Component::Normal(name) => {
                let spelled = DirListings::spelling(&dir, name)?;
                mismatch |= spelled != name;
                spelled
            }
            _ => return None,
        };
        on_disk.push(name.to_string_lossy().into_owned());
        dir.push(name);
    }
    mismatch.then(|| on_disk.join("/"))
}

/// `name` in lowercase, for comparisons that ignore letter case.
fn folded(name: &OsStr) -> String {
    name.to_string_lossy().to_lowercase()
}

/// What a lookup of `name` in `dir` tells without listing `dir` ([`case_mismatch`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lookup {
    /// `name` has no letter case (`2024`): a name it matches only by letter case would be
    /// itself, so it is spelled as on disk.
    Uncased,
    /// `dir` has `name`, and not `name` with its ASCII letters in the other case: `dir`
    /// finds names by their exact spelling (a case-sensitive file system, Linux), so every
    /// name it has is spelled as on disk.
    ExactOnly,
    /// Only a listing tells (a case-insensitive file system, macOS; or `dir` has both
    /// spellings, or `name` is not there).
    Unknown,
}

/// [`Lookup`] of `name` in `dir`, with at most two `lstat` calls: on a case-sensitive file
/// system the check of a reference needs no listing (wave-8 review of the A3 fixes: a
/// `figures/` folder of thousands of files was listed for each build).
fn exact_lookup(dir: &Path, name: &OsStr) -> Lookup {
    let Some(text) = name.to_str() else {
        return Lookup::Unknown;
    };
    if text.to_lowercase() == text && text.to_uppercase() == text {
        return Lookup::Uncased;
    }
    if !text.bytes().any(|byte| byte.is_ascii_alphabetic()) {
        return Lookup::Unknown;
    }
    // Swapping ASCII letters gives a name every case-insensitive file system takes for
    // `name`: there a lookup of it always succeeds.
    let swapped: String = text
        .chars()
        .map(|c| match c.is_ascii_uppercase() {
            true => c.to_ascii_lowercase(),
            false => c.to_ascii_uppercase(),
        })
        .collect();
    let has = |name: &str| std::fs::symlink_metadata(dir.join(name)).is_ok();
    match has(text) && !has(&swapped) {
        true => Lookup::ExactOnly,
        false => Lookup::Unknown,
    }
}

/// The entry names of a directory, for [`case_mismatch`].
struct Listing {
    /// Every name as listed.
    exact: HashSet<OsString>,
    /// Lowercased name → the smallest listed name with that lowercase form: built when the
    /// first name misses `exact`, so a document whose names all match exactly never lowercases
    /// a listing.
    folded: OnceCell<HashMap<String, OsString>>,
}

impl Listing {
    fn read(dir: &Path) -> Option<Listing> {
        count_directory_read();
        let names = std::fs::read_dir(dir).ok()?.flatten();
        Some(Listing {
            exact: names.map(|entry| entry.file_name()).collect(),
            folded: OnceCell::new(),
        })
    }

    /// The listed name that `name` names: itself when listed, else the smallest listed name
    /// that differs only by letter case; `None` when there is none.
    fn spelling(&self, name: &OsStr) -> Option<OsString> {
        if self.exact.contains(name) {
            return Some(name.to_os_string());
        }
        let folded_names = self.folded.get_or_init(|| {
            let mut map: HashMap<String, OsString> = HashMap::new();
            for listed in &self.exact {
                match map.entry(folded(listed)) {
                    HashEntry::Occupied(mut kept) if listed < kept.get() => {
                        kept.insert(listed.clone());
                    }
                    HashEntry::Occupied(_) => {}
                    HashEntry::Vacant(slot) => {
                        slot.insert(listed.clone());
                    }
                }
            }
            map
        });
        folded_names.get(&folded(name)).cloned()
    }

    /// [`Listing::spelling`] of `name` in `dir` without keeping a listing (no guard is
    /// open): the scan stops at an exact match, and only a name that misses compares the
    /// entries in lowercase.
    fn scan(dir: &Path, name: &OsStr) -> Option<OsString> {
        count_directory_read();
        let mut listed: Vec<OsString> = Vec::new();
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let entry = entry.file_name();
            if entry == name {
                return Some(entry);
            }
            listed.push(entry);
        }
        let wanted = folded(name);
        listed.into_iter().filter(|e| folded(e) == wanted).min()
    }
}

#[cfg(test)]
thread_local! {
    /// How many directories [`Listing`] read on this thread (tests count them).
    static DIRECTORY_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts one directory listing read (`directory_reads`, in tests); nothing outside tests.
fn count_directory_read() {
    #[cfg(test)]
    DIRECTORY_READS.set(DIRECTORY_READS.get() + 1);
}

/// How many directories the `asset-case-mismatch` check listed on this thread so far.
#[cfg(test)]
pub(crate) fn directory_reads() -> usize {
    DIRECTORY_READS.get()
}

/// What the open [`DirListings`] guards keep of a directory.
#[derive(Clone)]
enum Known {
    /// It finds names by their exact spelling ([`Lookup::ExactOnly`]): not listed.
    ExactOnly,
    /// Its listing (`None`: it cannot be listed).
    Listed(Option<Rc<Listing>>),
}

/// The open [`DirListings`] guards on a thread and what they know of each directory.
type ListingCache = (usize, HashMap<PathBuf, Known>);

thread_local! {
    /// This thread's [`ListingCache`].
    static LISTINGS: RefCell<ListingCache> = RefCell::new((0, HashMap::new()));
}

/// While a guard is alive, [`case_mismatch`] lists each directory once on this thread
/// instead of once per reference and path segment (a document next to a large `figures/`
/// folder would otherwise list it for every image), and a directory that finds names by
/// their exact spelling ([`exact_lookup`]) is probed once and never listed. The outermost
/// guard forgets what it learnt when dropped, so a rebuild sees the files as they are then.
/// A document parse and a notebook parse hold one for the whole document,
/// and each fragment holds one of its own. Without a guard, each lookup probes
/// the directory and, when that does not tell, scans it up to an exact match.
pub(crate) struct DirListings(());

impl DirListings {
    /// Opens a guard (nested guards share the outermost one's listings).
    pub(crate) fn enter() -> DirListings {
        LISTINGS.with_borrow_mut(|(open, _)| *open += 1);
        DirListings(())
    }

    /// The name in `dir` that `name` names ([`Listing::spelling`]): `name` itself when a
    /// lookup tells ([`exact_lookup`]), else from the listing cached while a guard is open
    /// (read now if it is not yet), else from a scan of `dir`. `None` when there is none or
    /// the directory cannot be listed.
    fn spelling(dir: &Path, name: &OsStr) -> Option<OsString> {
        let (open, cached) = LISTINGS.with_borrow(|(open, listings)| {
            (*open > 0, listings.get(dir).cloned())
        });
        let listing = match cached {
            Some(Known::ExactOnly) => return Some(name.to_os_string()),
            Some(Known::Listed(listing)) => listing,
            None => {
                let lookup = exact_lookup(dir, name);
                if lookup != Lookup::Unknown {
                    if open && lookup == Lookup::ExactOnly {
                        Self::remember(dir, Known::ExactOnly);
                    }
                    return Some(name.to_os_string());
                }
                if !open {
                    return Listing::scan(dir, name);
                }
                let listing = Listing::read(dir).map(Rc::new);
                Self::remember(dir, Known::Listed(listing.clone()));
                listing
            }
        };
        listing?.spelling(name)
    }

    /// Keeps `known` for `dir` until the outermost guard is dropped.
    fn remember(dir: &Path, known: Known) {
        LISTINGS.with_borrow_mut(|(_, listings)| {
            listings.insert(dir.to_path_buf(), known);
        });
    }
}

impl Drop for DirListings {
    fn drop(&mut self) {
        LISTINGS.with_borrow_mut(|(open, listings)| {
            *open = open.saturating_sub(1);
            if *open == 0 {
                listings.clear();
            }
        });
    }
}

/// A local reference: not empty, no URL scheme (`https:`, `data:`, `asset:`, …), not
/// root-absolute or protocol-relative (`/…`, `//…`) and not just a `#fragment` or `?query`.
pub(crate) fn is_relative_url(url: &str) -> bool {
    let scheme = url.find(':').is_some_and(|at| {
        let mut chars = url[..at].chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')
            })
    });
    !(url.is_empty() || scheme || url.starts_with(['/', '\\', '#', '?']))
}

/// Decodes `%XX` escapes (exactly two hex digits: `%+F` stays as written); the input
/// unchanged when the result is not UTF-8. Also used for notebook attachment names.
pub(crate) fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .filter(|h| h.iter().all(u8::is_ascii_hexdigit))
            .and_then(|h| std::str::from_utf8(h).ok());
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(byte) if bytes[i] == b'%' => {
                out.push(byte);
                i += 3;
            }
            _ => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_string())
}

#[cfg(test)]
mod asset_tests {
    use super::*;
    use crate::error::Diagnostics;

    fn asset_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("images")).unwrap();
        let png = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/md/images/plot.png");
        std::fs::copy(png, dir.path().join("images/plot.png")).unwrap();
        dir
    }

    /// A3 fix round: directory listings for `asset-case-mismatch` are read once while a
    /// guard is open, and read again after the outermost guard is dropped (a rebuild). A
    /// case-sensitive file system lists nothing ([`exact_lookups_need_no_listing`]).
    #[test]
    fn case_listings_are_cached_while_a_guard_is_open() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Figs")).unwrap();
        std::fs::write(dir.path().join("Figs/a.png"), b"x").unwrap();
        if !dir.path().join("FIGS/A.PNG").exists() {
            // Case-sensitive: names are found by their exact spelling (`figs/a.png` is not
            // there), so the check of a file that exists needs no listing.
            let _document = DirListings::enter();
            let before = directory_reads();
            assert_eq!(case_mismatch(dir.path(), "Figs/./a.png"), None);
            assert_eq!(case_mismatch(dir.path(), "Figs/a.png"), None);
            assert_eq!(directory_reads() - before, 0);
            return;
        }
        assert_eq!(
            case_mismatch(dir.path(), "figs/./a.png").as_deref(),
            Some("Figs/a.png")
        );
        {
            let _document = DirListings::enter();
            let before = directory_reads();
            assert_eq!(case_mismatch(dir.path(), "Figs/a.png"), None);
            assert_eq!(
                case_mismatch(dir.path(), "figs/a.png").as_deref(),
                Some("Figs/a.png")
            );
            assert_eq!(
                case_mismatch(dir.path(), "figs/./a.png").as_deref(),
                Some("Figs/a.png")
            );
            assert_eq!(
                directory_reads() - before,
                2,
                "each directory is listed once"
            );
            std::fs::write(dir.path().join("Figs/B.png"), b"x").unwrap();
            // the listing read above is reused, and it has no `B.png`
            assert_eq!(case_mismatch(dir.path(), "Figs/b.png"), None);
            drop(DirListings::enter());
            assert_eq!(case_mismatch(dir.path(), "Figs/b.png"), None);
        }
        assert_eq!(
            case_mismatch(dir.path(), "Figs/b.png").as_deref(),
            Some("Figs/B.png")
        );
        assert_eq!(case_mismatch(dir.path(), "Figs/../Figs/B.png"), None);
    }

    /// Wave-8 review of the A3 fixes: a name without letter case, and every name of a
    /// directory that finds names by their exact spelling (a case-sensitive file system),
    /// is checked without listing the directory; on a case-insensitive file system each
    /// directory is listed once (per guard).
    #[test]
    fn exact_lookups_need_no_listing() {
        let dir = tempfile::tempdir().unwrap();
        let year = dir.path().join("2024");
        std::fs::create_dir_all(year.join("Figs")).unwrap();
        std::fs::write(year.join("01"), b"x").unwrap();
        std::fs::write(year.join("Figs/a.png"), b"x").unwrap();
        let insensitive = year.join("FIGS/A.PNG").exists();
        assert_eq!(
            exact_lookup(dir.path(), OsStr::new("2024")),
            Lookup::Uncased
        );
        assert_eq!(exact_lookup(&year, OsStr::new("01")), Lookup::Uncased);
        let expected = match insensitive {
            true => Lookup::Unknown,
            false => Lookup::ExactOnly,
        };
        assert_eq!(exact_lookup(&year, OsStr::new("Figs")), expected);
        // A name that is not there is never taken for exact; nor one without ASCII letters.
        assert_eq!(exact_lookup(&year, OsStr::new("gone")), Lookup::Unknown);
        assert_eq!(exact_lookup(&year, OsStr::new("é")), Lookup::Unknown);
        {
            let _document = DirListings::enter();
            let before = directory_reads();
            assert_eq!(case_mismatch(dir.path(), "2024/01"), None);
            assert_eq!(
                directory_reads() - before,
                0,
                "names without letter case"
            );
            for _ in 0..3 {
                assert_eq!(
                    case_mismatch(dir.path(), "2024/./Figs/a.png"),
                    None
                );
            }
            let listed = directory_reads() - before;
            assert_eq!(
                listed,
                if insensitive { 2 } else { 0 },
                "2024/ and Figs/, once"
            );
        }
        if !insensitive {
            // Both spellings in one directory: only its listing tells them apart.
            std::fs::write(year.join("Figs/A.PNG"), b"x").unwrap();
            let figs = year.join("Figs");
            assert_eq!(
                exact_lookup(&figs, OsStr::new("a.png")),
                Lookup::Unknown
            );
            let before = directory_reads();
            assert_eq!(case_mismatch(dir.path(), "2024/Figs/A.PNG"), None);
            assert_eq!(directory_reads() - before, 1);
        }
    }

    #[test]
    fn missing_asset_path_reads_the_frozen_message() {
        let dir = asset_fixture();
        let source = dir.path().join("document.md");
        let span = SourceSpan {
            start_line: 1,
            end_line: 1,
            cell: None,
            file: Some(source.clone()),
        };
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let mut ctx = super::super::Fragment {
            assets: &mut assets,
            base_dir: dir.path(),
            diagnostics: &mut diagnostics,
            file: Some(&source),
            cell: None,
            missing_asset_is_error: false,
        };
        let mut html = "<img src=\"a b (1).png\">".to_string();
        rewrite_html_snippet(
            &mut html,
            Some(&span),
            &mut ctx,
            &mut crate::assets::HtmlScan::default(),
        )
        .unwrap();
        assert_eq!(html, "<img src=\"a b (1).png\">");
        assert_eq!(
            local_asset("sub/../gone%29.png", "image", Some(&span), &mut ctx)
                .unwrap(),
            None
        );
        let paths: Vec<_> =
            ctx.diagnostics.iter().map(missing_asset_path).collect();
        assert_eq!(
            paths,
            [
                Some(dir.path().join("a b (1).png")),
                Some(dir.path().join("gone).png")),
            ]
        );
        assert!(ctx.diagnostics.iter().all(|diagnostic| {
            diagnostic.code == MISSING_ASSET
                && diagnostic.path.as_deref() == Some(source.as_path())
                && diagnostic.span.as_ref() == Some(&span)
        }));
        let other =
            Diagnostic::warning("invalid-directive", "x (looked for y)");
        assert_eq!(missing_asset_path(&other), None);
    }

    #[test]
    fn image_urls_keep_query_and_fragment_and_are_percent_decoded() {
        let dir = asset_fixture();
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let mut ctx = super::super::Fragment {
            assets: &mut assets,
            base_dir: dir.path(),
            diagnostics: &mut diagnostics,
            file: None,
            cell: None,
            missing_asset_is_error: false,
        };
        let id = ctx.assets.register_file(&dir.path().join("images/plot.png"));
        assert_eq!(
            local_asset("images/plot.png?x=1#f", "image", None, &mut ctx)
                .unwrap(),
            Some(format!("asset:{id}?x=1#f"))
        );
        assert_eq!(
            local_asset("images/%70lot.png", "image", None, &mut ctx).unwrap(),
            Some(format!("asset:{id}"))
        );
        assert_eq!(
            local_asset("images", "image", None, &mut ctx).unwrap(),
            None
        );
        assert_eq!(local_asset("#top", "image", None, &mut ctx).unwrap(), None);
        assert_eq!(ctx.assets.len(), 1);
        let codes: Vec<_> = ctx.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, [MISSING_ASSET]);
    }

    #[test]
    fn relative_url_classification() {
        for url in [
            "a.png", "./a.png", "../a.png", "dir/a:b.png", "a.png?x", "%20.png",
        ] {
            assert!(is_relative_url(url), "{url}");
        }
        for url in [
            "", "/a.png", "//host/a.png", "#frag", "?q", "https://h/a.png",
            "data:image/png;base64,AA", "asset:0123456789ab", "mailto:a@b",
            "attachment:img.png", "\\\\server\\a.png",
        ] {
            assert!(!is_relative_url(url), "{url}");
        }
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
        assert_eq!(percent_decode("%C3%A9"), "é");
        assert_eq!(percent_decode("%FF"), "%FF", "not UTF-8: unchanged");
    }
}

#[cfg(test)]
mod implicit_image_note_tests {
    use super::*;
    use crate::assets::registry::AssetRegistry;

    fn image(text: &str) -> (Inlines, Inlines) {
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let base =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md");
        let mut ctx = super::super::Fragment {
            assets: &mut assets,
            base_dir: &base,
            diagnostics: &mut diagnostics,
            file: None,
            cell: None,
            missing_asset_is_error: false,
        };
        let blocks =
            super::super::parse_blocks(text, &Config::default(), &mut ctx)
                .unwrap();
        let [Block::Figure { caption: Some(caption), body, .. }] =
            blocks.as_slice()
        else {
            panic!("expected one captioned image: {blocks:?}");
        };
        let [Block::Paragraph { inlines, .. }] = body.as_slice() else {
            panic!("expected image paragraph: {body:?}");
        };
        let [Inline::Image { alt, .. }] = inlines.as_slice() else {
            panic!("expected one image: {inlines:?}");
        };
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(crate::ir::plain_text(alt), crate::ir::plain_text(caption));
        (caption.clone(), alt.clone())
    }

    #[test]
    fn named_notes_belong_to_implicit_image_captions() {
        let (caption, alt) = image(
            "![Caption *note[^a]* again[^a]](images/plot.png)\n\n[^a]: A named note.\n",
        );
        assert!(matches!(&caption[1], Inline::Emph { inlines }
            if matches!(&inlines[1], Inline::Footnote { label: Some(label), .. } if label == "a")));
        assert!(
            matches!(caption.last(), Some(Inline::FootnoteRef { label, .. }) if label == "a")
        );
        assert!(matches!(&alt[1], Inline::Emph { inlines }
            if inlines == &[Inline::Text { text: "note".into() }]));
        let mut stripped = caption;
        remove_alt_footnotes(&mut stripped);
        assert_eq!(alt, stripped);
    }

    #[test]
    fn anonymous_notes_belong_to_implicit_image_captions() {
        // Comrak keeps anonymous-note syntax literal inside image brackets.
        // Exercise the caption-copy operation with its synthetic IR form.
        let caption = vec![
            Inline::Text { text: "Caption ".into() },
            Inline::Strong {
                inlines: vec![
                    Inline::Text { text: "note".into() },
                    Inline::Footnote {
                        blocks: vec![Block::Paragraph {
                            inlines: vec![Inline::Text {
                                text: "A note.".into(),
                            }],
                            span: Some(SourceSpan {
                                start_line: 1,
                                end_line: 1,
                                cell: None,
                                file: None,
                            }),
                        }],
                        label: None,
                        number: None,
                    },
                ],
            },
        ];
        let mut alt = caption.clone();
        remove_alt_footnotes(&mut alt);
        assert!(matches!(&alt[1], Inline::Strong { inlines }
            if inlines == &[Inline::Text { text: "note".into() }]));
        assert!(matches!(&caption[1], Inline::Strong { inlines }
            if matches!(inlines.get(1), Some(Inline::Footnote { label: None, .. }))));
        assert_eq!(
            crate::ir::plain_text(&alt),
            crate::ir::plain_text(&caption)
        );
    }

    #[test]
    fn dimensions_require_ascii_css_numbers() {
        for value in ["١٠px", "１.５%", "1٠cm"] {
            assert!(!valid_dimension(value), "{value}");
        }
        for value in ["10px", "1.5%", "10cm"] {
            assert!(valid_dimension(value), "{value}");
        }
    }

    #[test]
    fn named_notes_inside_anonymous_notes_keep_their_definition_span() {
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let base = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut ctx = super::super::Fragment {
            assets: &mut assets,
            base_dir: base,
            diagnostics: &mut diagnostics,
            file: None,
            cell: None,
            missing_asset_is_error: false,
        };
        let blocks = super::super::parse_blocks(
            "Text^[Anonymous with named[^n]].\n\n[^n]: Named note.\n",
            &Config::default(),
            &mut ctx,
        )
        .unwrap();
        let [Block::Paragraph { inlines, .. }] = blocks.as_slice() else {
            panic!("expected one paragraph: {blocks:?}");
        };
        let Inline::Footnote { blocks: anonymous, label: None, .. } =
            &inlines[1]
        else {
            panic!("expected anonymous note: {inlines:?}");
        };
        let [Block::Paragraph { inlines, span: Some(span) }] =
            anonymous.as_slice()
        else {
            panic!("expected anonymous paragraph: {anonymous:?}");
        };
        assert_eq!(span.start_line, 1);
        let Inline::Footnote { blocks: named, label: Some(label), .. } =
            &inlines[1]
        else {
            panic!("expected named note: {inlines:?}");
        };
        assert_eq!(label, "n");
        assert_eq!(named[0].span().unwrap().start_line, 3);
        assert!(diagnostics.is_empty());
    }
}
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::assets::registry::AssetRegistry;
    use crate::config::{Config, Toc};
    use crate::error::{Diagnostics, Error, Level};
    use crate::ir::{
        Align, Block, FigureKind, Inline, Inlines, ListKind, MathDialect,
        Parsed, SourceInfo, SourceKind, plain_text, walk_blocks, walk_inlines,
    };
    use crate::markdown::{
        Fragment, code::CodeAttrs, parse, parse_blocks, parse_inlines,
    };

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md")
    }

    fn source() -> SourceInfo {
        let base_dir = fixture_dir();
        SourceInfo {
            path: base_dir.join("unit-document.md"),
            kind: SourceKind::Markdown,
            base_dir,
        }
    }

    fn parsed(text: &str) -> Parsed {
        parse(&source(), text, &Config::default()).unwrap()
    }

    fn codes(parsed: &Parsed) -> Vec<&str> {
        parsed.diagnostics.iter().map(|diagnostic| diagnostic.code).collect()
    }

    fn paragraph(block: &Block) -> &Inlines {
        let Block::Paragraph { inlines, .. } = block else {
            panic!("expected paragraph, got {block:?}");
        };
        inlines
    }

    fn only_image(block: &Block) -> &Inline {
        let inlines = paragraph(block);
        assert_eq!(inlines.len(), 1);
        assert!(matches!(inlines[0], Inline::Image { .. }));
        &inlines[0]
    }

    fn figure(
        block: &Block,
    ) -> (FigureKind, Option<&Inlines>, &[Block], &crate::ir::Attrs) {
        let Block::Figure { kind, caption, body, attrs, number, .. } = block
        else {
            panic!("expected figure, got {block:?}");
        };
        assert!(number.is_none(), "parsing must leave numbering to resolve");
        (*kind, caption.as_ref(), body, attrs)
    }

    fn table_grid(block: &Block) {
        let Block::Table { columns, head, body, foot, .. } = block else {
            panic!("expected table, got {block:?}");
        };
        assert_eq!(
            columns.iter().map(|column| column.align).collect::<Vec<_>>(),
            [Align::Default, Align::Center, Align::Right]
        );
        assert!(columns.iter().all(|column| column.width.is_none()));
        assert_eq!((head.len(), body.len(), foot.len()), (1, 1, 0));
        for row in head.iter().chain(body) {
            assert_eq!(row.cells.len(), 3);
            for cell in &row.cells {
                assert_eq!(
                    (cell.colspan, cell.rowspan, cell.align),
                    (1, 1, None)
                );
            }
        }
        assert!(head[0].cells.iter().all(|cell| cell.header));
        assert!(body[0].cells.iter().all(|cell| !cell.header));
    }

    const TABLE: &str = "| a | b | c |\n|---|:---:|---:|\n| 1 | 2 | 3 |\n";

    #[test]
    fn implicit_figures() {
        let result = parsed(
            "![Caption with *emph*](images/plot.png \"Title\"){#fig-x .wide width=50%}\n",
        );
        let (kind, caption, body, attrs) = figure(&result.document.blocks[0]);
        assert_eq!(kind, FigureKind::Image);
        assert_eq!(plain_text(caption.unwrap()), "Caption with emph");
        assert!(
            caption
                .unwrap()
                .iter()
                .any(|inline| matches!(inline, Inline::Emph { .. }))
        );
        assert_eq!(attrs.id.as_deref(), Some("fig-x"));
        assert_eq!(attrs.classes, ["wide"]);
        assert!(attrs.pairs.is_empty());
        let Inline::Image { alt, url, title, attrs } = only_image(&body[0])
        else {
            unreachable!()
        };
        assert_eq!(alt, caption.unwrap());
        assert!(url.starts_with("asset:"));
        assert_eq!(title.as_deref(), Some("Title"));
        assert_eq!(attrs.pairs["width"], "50%");
        assert!(attrs.id.is_none() && attrs.classes.is_empty());
        assert_eq!(result.document.assets.len(), 1);
        let result = parsed("![](images/plot.png){#fig-empty}\n");
        assert!(figure(&result.document.blocks[0]).1.is_none());
        for text in [
            "![](images/plot.png)\n",
            "![One](images/plot.png) ![Two](images/plot.png)\n",
            "![One](images/plot.png) trailing text\n",
            "[![One](images/plot.png)](https://example.org)\n",
        ] {
            assert!(
                matches!(
                    parsed(text).document.blocks[0],
                    Block::Paragraph { .. }
                ),
                "{text}"
            );
        }
        let result =
            parsed(":::figure\n![Inner caption](images/plot.png)\n:::\n");
        let (kind, _, body, _) = figure(&result.document.blocks[0]);
        assert_eq!(kind, FigureKind::Image);
        only_image(&body[0]);
        let result =
            parsed(":::figure\n![Subfigure](images/plot.png){#fig-sub}\n:::\n");
        let (kind, _, body, _) = figure(&result.document.blocks[0]);
        assert_eq!(kind, FigureKind::Other);
        assert_eq!(figure(&body[0]).3.id.as_deref(), Some("fig-sub"));
    }

    #[test]
    fn image_assets_and_dimensions() {
        let result = parsed(
            "![Plot](images/plot.png?v=2#panel){width=12.5cm height=80%}\n",
        );
        let (_, _, body, _) = figure(&result.document.blocks[0]);
        let Inline::Image { url, attrs, .. } = only_image(&body[0]) else {
            unreachable!()
        };
        assert!(url.starts_with("asset:") && url.ends_with("?v=2#panel"));
        assert_eq!(attrs.pairs["width"], "12.5cm");
        assert_eq!(attrs.pairs["height"], "80%");
        assert_eq!(result.document.assets.len(), 1);
        for value in ["12", "12px", "1pt", "2mm", "3in", "4em", "0%"] {
            let result =
                parsed(&format!("![](images/plot.png){{width={value}}}\n"));
            let Inline::Image { attrs, .. } =
                only_image(&result.document.blocks[0])
            else {
                unreachable!()
            };
            assert_eq!(attrs.pairs["width"], value);
            assert!(!codes(&result).contains(&"invalid-dimension"));
        }
        let result = parsed(
            "![](images/plot.png){width='calc(100% - 1em)' height=-2px}\n",
        );
        let Inline::Image { attrs, .. } =
            only_image(&result.document.blocks[0])
        else {
            unreachable!()
        };
        assert!(
            !attrs.pairs.contains_key("width")
                && !attrs.pairs.contains_key("height")
        );
        assert_eq!(
            codes(&result)
                .iter()
                .filter(|&&code| code == "invalid-dimension")
                .count(),
            2
        );
        let result = parsed("![](missing-unit-image.png)\n");
        assert!(codes(&result).contains(&"missing-asset"));
        let Inline::Image { url, .. } = only_image(&result.document.blocks[0])
        else {
            unreachable!()
        };
        assert_eq!(url, "missing-unit-image.png");
        for url in [
            "https://example.org/plot.png", "data:image/png;base64,AA==",
            "/root/plot.png",
        ] {
            let result = parsed(&format!("![]({url})\n"));
            assert!(result.document.assets.is_empty());
            let Inline::Image { url: actual, .. } =
                only_image(&result.document.blocks[0])
            else {
                unreachable!()
            };
            assert_eq!(actual, url);
        }
        assert!(parsed("[Plot](images/plot.png)\n").document.assets.is_empty());
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let base_dir = fixture_dir();
        let mut ctx = Fragment {
            assets: &mut assets,
            base_dir: &base_dir,
            diagnostics: &mut diagnostics,
            file: None,
            cell: Some(7),
            missing_asset_is_error: true,
        };
        let err = parse_blocks(
            "![](missing-unit-image.png)",
            &Config::default(),
            &mut ctx,
        )
        .unwrap_err();
        let Error::MissingAsset { path, span } = err else {
            panic!("expected missing asset")
        };
        assert_eq!(path, base_dir.join("missing-unit-image.png"));
        assert_eq!(span.unwrap().cell, Some(7));
    }

    #[test]
    fn table_captions() {
        let before =
            parsed(&format!("Table: Before *emph* {{#tbl-before}}\n\n{TABLE}"));
        let (kind, caption, body, attrs) = figure(&before.document.blocks[0]);
        assert_eq!(kind, FigureKind::Table);
        assert_eq!(plain_text(caption.unwrap()), "Before emph");
        assert_eq!(attrs.id.as_deref(), Some("tbl-before"));
        table_grid(&body[0]);
        let span = before.document.blocks[0].span().unwrap();
        assert_eq!((span.start_line, span.end_line), (1, 5));
        let after = parsed(&format!("{TABLE}\nTable: After {{#tbl-after}}\n"));
        assert_eq!(after.document.blocks.len(), 1);
        assert_eq!(
            figure(&after.document.blocks[0]).3.id.as_deref(),
            Some("tbl-after")
        );
        let between = parsed(&format!(
            "{TABLE}\nTable: Between {{#tbl-next}}\n\n{TABLE}"
        ));
        assert_eq!(between.document.blocks.len(), 2);
        assert!(matches!(between.document.blocks[0], Block::Table { .. }));
        assert_eq!(
            figure(&between.document.blocks[1]).3.id.as_deref(),
            Some("tbl-next")
        );
        let empty = parsed(&format!("Table: {{#tbl-only-id}}\n\n{TABLE}"));
        assert!(figure(&empty.document.blocks[0]).1.is_none());
        let orphan =
            parsed("Table: Orphan {#tbl-orphan}\n\nUnrelated paragraph.\n");
        assert_eq!(
            plain_text(paragraph(&orphan.document.blocks[0])),
            "Table: Orphan {#tbl-orphan}"
        );
        assert!(codes(&orphan).contains(&"table-caption-orphan"));
        let pandoc = parsed(&format!("{TABLE}: Caption as a row\n"));
        assert!(codes(&pandoc).contains(&"table-caption-pandoc-colon"));
        let Block::Table { body, .. } = &pandoc.document.blocks[0] else {
            panic!("table expected")
        };
        assert_eq!(body.len(), 2);
        assert!(plain_text(&body[1].cells[0].inlines).starts_with(": Caption"));
        let invalid =
            parsed(&format!("Table: Caption {{width='unfinished}}\n\n{TABLE}"));
        assert!(codes(&invalid).contains(&"unparsed-attr-block"));
        assert!(
            plain_text(figure(&invalid.document.blocks[0]).1.unwrap())
                .contains("{width=")
        );
        let quoted = parsed(&format!(
            "Table: Caption {{#tbl-x title='a {{b}}'}}\n\n{TABLE}"
        ));
        let (_, caption, _, attrs) = figure(&quoted.document.blocks[0]);
        assert_eq!(plain_text(caption.unwrap()), "Caption");
        assert_eq!(attrs.id.as_deref(), Some("tbl-x"));
        assert_eq!(attrs.pairs["title"], "a {b}");
        assert!(!codes(&quoted).contains(&"unparsed-attr-block"));
    }

    #[test]
    fn display_math() {
        for text in [
            "$$ x + y $$ {#eq-one}\n",
            "$$\n x + y \n$$\n{#eq-one}\n",
            "```math {#eq-one}\nx + y\n```\n",
        ] {
            let result = parsed(text);
            let Block::Math { source, dialect, attrs, number, .. } =
                &result.document.blocks[0]
            else {
                panic!("expected display math for {text:?}");
            };
            assert_eq!(source, "x + y");
            assert_eq!(*dialect, MathDialect::Latex);
            assert_eq!(attrs.id.as_deref(), Some("eq-one"));
            assert!(number.is_none());
            assert!(result.document.features.math);
        }
        assert!(matches!(
            parsed("$$x$$\n").document.blocks[0],
            Block::Math { .. }
        ));
        let result = parsed("Before $$x$$ after {#eq-misplaced}\n");
        assert!(paragraph(&result.document.blocks[0]).iter().any(|inline| matches!(inline, Inline::Math { source, .. } if source == "x")));
        assert!(codes(&result).contains(&"equation-label-misplaced"));
        let result = parsed("- $$x$$\n\n> $$y$$\n");
        let mut math = 0;
        walk_blocks(&result.document, &mut |block| {
            math += usize::from(matches!(block, Block::Math { .. }))
        });
        assert_eq!(math, 2);
        let inline = parsed("$x$ and $`y`$\n");
        assert!(inline.document.features.math);
        assert_eq!(
            paragraph(&inline.document.blocks[0])
                .iter()
                .filter(|node| matches!(node, Inline::Math { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn code_fences() {
        let result = parsed(
            "```rust {#lst-x .plain .numbered start=7 highlight='7-8,10' lines='12|14' caption='A *listing*' include=sample.rs trim=true escape=false}\nlet x = 1;\n\n```\n",
        );
        let (kind, caption, body, attrs) = figure(&result.document.blocks[0]);
        assert_eq!(kind, FigureKind::Listing);
        assert_eq!(attrs.id.as_deref(), Some("lst-x"));
        assert_eq!(plain_text(caption.unwrap()), "A listing");
        let Block::CodeBlock { lang, text, attrs, .. } = &body[0] else {
            panic!("code expected")
        };
        assert_eq!(lang.as_deref(), Some("rust"));
        assert_eq!(text, "let x = 1;\n");
        assert!(attrs.id.is_none());
        assert!(!attrs.pairs.contains_key("caption"));
        assert_eq!(attrs.pairs["include"], "sample.rs");
        assert_eq!(attrs.pairs["trim"], "true");
        assert_eq!(attrs.pairs["escape"], "false");
        let code = CodeAttrs::from_attrs(attrs);
        assert!(code.numbered && code.plain);
        assert_eq!(code.start, 7);
        for line in [7, 8, 10, 12, 14] {
            assert!(code.is_highlighted(line));
        }
        assert!(!code.is_highlighted(9));
        assert!(result.document.features.code);
        let labelled = parsed("```python {#lst-only}\nx\n```\n");
        assert!(figure(&labelled.document.blocks[0]).1.is_none());
        let invalid =
            parsed("```txt {start=0 highlight='1-' lines='a|b'}\nx\n```\n");
        assert_eq!(
            codes(&invalid)
                .iter()
                .filter(|&&code| code == "invalid-code-attr")
                .count(),
            3
        );
        let Block::CodeBlock { attrs, .. } = &invalid.document.blocks[0] else {
            panic!("code expected")
        };
        assert_eq!(attrs.pairs["start"], "0");
        assert_eq!(CodeAttrs::from_attrs(attrs).start, 1);
        for text in ["```\ncode\n```\n", "    code\n"] {
            let result = parsed(text);
            let Block::CodeBlock { lang, attrs, text, .. } =
                &result.document.blocks[0]
            else {
                panic!("code expected")
            };
            assert!(lang.is_none());
            assert_eq!(attrs, &crate::ir::Attrs::default());
            assert_eq!(text, "code");
        }
    }

    #[test]
    fn raw_blocks() {
        for format in
            ["HTML", "latex", "typst", "openxml", "markdown", "mystery"]
        {
            let result =
                parsed(&format!("```{{={format}}}\n<raw>& untouched\n```\n"));
            let Block::Raw { format: actual, text, .. } =
                &result.document.blocks[0]
            else {
                panic!("raw expected")
            };
            assert_eq!(actual, &format.to_ascii_lowercase());
            assert_eq!(text, "<raw>& untouched\n");
            assert_eq!(
                codes(&result).contains(&"raw-unknown-format"),
                format == "mystery"
            );
        }
        let result = parsed(
            "<!-- whole comment -->\n\n<div><img src='images/plot.png'></div>\n\nA <!-- inline comment --> <b>bold</b>.\n",
        );
        assert_eq!(result.document.blocks.len(), 2);
        let Block::Raw { format, text, .. } = &result.document.blocks[0] else {
            panic!("html expected")
        };
        assert_eq!(format, "html");
        assert!(text.contains("asset:") && !text.contains("images/plot.png"));
        assert_eq!(result.document.assets.len(), 1);
        let inlines = paragraph(&result.document.blocks[1]);
        assert!(!inlines.iter().any(|inline| matches!(inline, Inline::Raw { text, .. } if text.starts_with("<!--"))));
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Raw { format, text } if format == "html" && text == "<b>")));
    }

    #[test]
    fn alerts() {
        for kind in ["note", "tip", "important", "warning", "caution"] {
            let result = parsed(&format!(
                "> [!{}] A *title*\n> Body.\n",
                kind.to_ascii_uppercase()
            ));
            let Block::Callout { kind: actual, title, blocks, attrs, .. } =
                &result.document.blocks[0]
            else {
                panic!("callout expected")
            };
            assert_eq!(actual, kind);
            assert_eq!(plain_text(title.as_ref().unwrap()), "A title");
            assert_eq!(plain_text(paragraph(&blocks[0])), "Body.");
            assert_eq!(attrs, &crate::ir::Attrs::default());
        }
        let result = parsed("> [!note]\n> Body\n");
        assert!(matches!(
            &result.document.blocks[0],
            Block::Callout { title: None, .. }
        ));
        assert!(matches!(
            &parsed("> [!UNKNOWN] Title\n> Body\n").document.blocks[0],
            Block::BlockQuote { .. }
        ));
        for rule in ["***", "___", "- - -", "---"] {
            let result = parsed(&format!("Intro\n\n{rule}\n"));
            assert!(matches!(
                result.document.blocks[1],
                Block::ThematicBreak { .. }
            ));
        }
    }

    #[test]
    fn lists() {
        let result = parsed("3. First\n4. Second\n");
        let Block::List { kind, tight, items, .. } = &result.document.blocks[0]
        else {
            panic!("list expected")
        };
        assert_eq!(*kind, ListKind::Ordered { start: 3 });
        assert!(*tight);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| item.checked.is_none()));
        let result =
            parsed("- [x] Done\n- [ ] Pending\n- Ordinary\n  - Nested\n");
        let Block::List { kind, tight, items, .. } = &result.document.blocks[0]
        else {
            panic!("list expected")
        };
        assert_eq!(*kind, ListKind::Bullet);
        assert!(*tight);
        assert_eq!(
            items.iter().map(|item| item.checked).collect::<Vec<_>>(),
            [Some(true), Some(false), None]
        );
        assert!(
            items[2]
                .blocks
                .iter()
                .any(|block| matches!(block, Block::List { .. }))
        );
        let result = parsed("- One\n\n- Two\n");
        assert!(matches!(
            result.document.blocks[0],
            Block::List { tight: false, .. }
        ));
        let result = parsed("- > Quote\n- Text\n");
        let Block::List { tight, items, .. } = &result.document.blocks[0]
        else {
            panic!("list expected")
        };
        assert!(!tight, "tight list invariants require loosening block quotes");
        assert!(matches!(items[0].blocks[0], Block::BlockQuote { .. }));
        assert!(result.document.invariant_violations().is_empty());
    }

    #[test]
    fn definition_lists() {
        let result = parsed(
            "Term *one*\n\n: First definition\n\n: Second definition\n\nAnother\n\n: A paragraph\n\n    Another paragraph\n",
        );
        let Block::DefinitionList { items, .. } = &result.document.blocks[0]
        else {
            panic!("definitions expected")
        };
        assert_eq!(items.len(), 2);
        assert_eq!(plain_text(&items[0].term), "Term one");
        assert!(
            items[0]
                .term
                .iter()
                .any(|inline| matches!(inline, Inline::Emph { .. }))
        );
        assert_eq!(items[0].definitions.len(), 2);
        assert_eq!(
            plain_text(paragraph(&items[0].definitions[0][0])),
            "First definition"
        );
        assert_eq!(
            plain_text(paragraph(&items[0].definitions[1][0])),
            "Second definition"
        );
        assert_eq!(items[1].definitions[0].len(), 2);
        assert_eq!(
            plain_text(paragraph(&items[1].definitions[0][1])),
            "Another paragraph"
        );
    }

    #[test]
    fn directive_table() {
        for kind in ["note", "tip", "important", "warning", "caution"] {
            let result = parsed(&format!(
                ":::{kind} {{#box .wide title='A *title*' custom=x}}\nBody\n:::\n"
            ));
            let Block::Callout { kind: actual, title, attrs, blocks, .. } =
                &result.document.blocks[0]
            else {
                panic!("callout expected")
            };
            assert_eq!(actual, kind);
            assert_eq!(plain_text(title.as_ref().unwrap()), "A title");
            assert_eq!(attrs.id.as_deref(), Some("box"));
            assert_eq!(attrs.classes, ["wide"]);
            assert_eq!(attrs.pairs["custom"], "x");
            assert!(!attrs.pairs.contains_key("title"));
            assert_eq!(plain_text(paragraph(&blocks[0])), "Body");
        }
        for (info, expected) in [
            ("callout", "note"),
            ("callout custom", "custom"),
            ("callout kind=special", "special"),
        ] {
            assert!(
                matches!(&parsed(&format!(":::{info}\nBody\n:::\n")).document.blocks[0],
                Block::Callout { kind, .. } if kind == expected)
            );
        }
        for (info, class) in [
            ("columns widths='1fr 2fr'", "columns"),
            ("column width=50%", "column"),
            ("fragment fade index=2", "fragment"),
            ("center", "center"),
            ("unknown custom=x", "unknown"),
        ] {
            let result = parsed(&format!(":::{info}\nBody\n:::\n"));
            let Block::Div { attrs, blocks, .. } = &result.document.blocks[0]
            else {
                panic!("div expected")
            };
            assert_eq!(attrs.classes[0], class);
            assert_eq!(plain_text(paragraph(&blocks[0])), "Body");
            assert!(result.diagnostics.iter().next().is_none());
            if class == "unknown" {
                assert_eq!(attrs.pairs["custom"], "x");
            }
        }
        let result = parsed(
            ":::pagebreak\n:::\n\n:::toc depth=2\n:::\n\n:::bibliography\n:::\n",
        );
        assert_eq!(result.document.blocks.len(), 3);
        assert!(matches!(result.document.blocks[0], Block::PageBreak { .. }));
        assert!(matches!(
            result.document.blocks[1],
            Block::Toc { depth: 2, .. }
        ));
        assert!(matches!(
            result.document.blocks[2],
            Block::Bibliography { .. }
        ));
        for info in ["raw format=LaTeX", "raw LaTeX"] {
            let result = parsed(&format!(
                ":::{info}\n\\section{{Raw}}\n\n  untouched\n:::\n"
            ));
            assert!(
                matches!(&result.document.blocks[0], Block::Raw { format, text, .. }
                if format == "latex" && text == "\\section{Raw}\n\n  untouched\n")
            );
        }
        let result =
            parsed(":::notes\nHidden\n:::\n\n:::comment\nAlso hidden\n:::\n");
        assert!(result.document.blocks.is_empty());
        assert!(result.diagnostics.iter().next().is_none());
        let result = parsed("::: {.note title='Pandoc'}\nBody\n:::\n");
        assert!(
            matches!(&result.document.blocks[0], Block::Callout { kind, title: Some(title), .. }
            if kind == "note" && plain_text(title) == "Pandoc")
        );
        let mut config = Config::default();
        config.document.toc = Toc::Depth(5);
        let result = parse(&source(), ":::toc\n:::\n", &config).unwrap();
        assert!(matches!(
            result.document.blocks[0],
            Block::Toc { depth: 5, .. }
        ));
    }

    #[test]
    fn directive_figures_infer_kinds_and_consume_only_their_arguments() {
        for (content, expected) in [
            ("![Alt](images/plot.png)\n".to_owned(), FigureKind::Image),
            (TABLE.to_owned(), FigureKind::Table),
            ("```txt\ncode\n```\n".to_owned(), FigureKind::Listing),
            ("Arbitrary paragraph\n".to_owned(), FigureKind::Other),
        ] {
            let result = parsed(&format!(
                ":::figure {{#fig-wrap .wide caption='Caption *text*' custom=x}}\n{content}:::\n"
            ));
            let (kind, caption, _, attrs) = figure(&result.document.blocks[0]);
            assert_eq!(kind, expected);
            assert_eq!(plain_text(caption.unwrap()), "Caption text");
            assert_eq!(attrs.id.as_deref(), Some("fig-wrap"));
            assert_eq!(attrs.classes, ["wide"]);
            assert_eq!(attrs.pairs["custom"], "x");
            assert!(
                !attrs.pairs.contains_key("caption")
                    && !attrs.pairs.contains_key("kind")
            );
        }
        let result = parsed(
            ":::figure kind=other\n```txt\ncode\n```\n\nLast *paragraph*\n:::\n",
        );
        let (kind, caption, body, _) = figure(&result.document.blocks[0]);
        assert_eq!(kind, FigureKind::Other);
        assert_eq!(plain_text(caption.unwrap()), "Last paragraph");
        assert_eq!(body.len(), 1);
        let nested = parsed("::::outer\n:::note\nNested\n:::\n::::\n");
        let Block::Div { blocks, .. } = &nested.document.blocks[0] else {
            panic!("div expected")
        };
        assert!(matches!(blocks[0], Block::Callout { .. }));
    }

    #[test]
    fn directive_diagnostics() {
        let bare = parsed(":::\nUnwrapped body\n:::\n");
        assert_eq!(
            plain_text(paragraph(&bare.document.blocks[0])),
            "Unwrapped body"
        );
        assert!(codes(&bare).contains(&"invalid-directive"));
        let leaf = parsed(":::pagebreak\nUnexpected body\n:::\n");
        assert_eq!(leaf.document.blocks.len(), 2);
        assert!(matches!(leaf.document.blocks[0], Block::PageBreak { .. }));
        assert_eq!(
            plain_text(paragraph(&leaf.document.blocks[1])),
            "Unexpected body"
        );
        assert!(codes(&leaf).contains(&"invalid-directive"));
        for (text, code) in [
            (":::note\nUnclosed\n", "unclosed-directive"),
            ("- item\n  :::note: x\n", "invalid-directive"),
            (":::toc depth=9\n:::\n", "invalid-directive-arg"),
            (":::figure kind=bogus\nBody\n:::\n", "invalid-directive-arg"),
            (":::figure kind=image\nText only\n:::\n", "invalid-directive-arg"),
            (":::raw\nBody remains\n:::\n", "invalid-directive-arg"),
        ] {
            let result = parsed(text);
            let finding = result
                .diagnostics
                .iter()
                .find(|finding| finding.code == code)
                .unwrap_or_else(|| panic!("missing {code} for {text:?}"));
            assert_eq!(finding.level, Level::Warning);
            assert!(finding.span.is_some());
            assert_eq!(finding.path.as_ref(), Some(&source().path));
        }
        let invalid_figure = parsed(":::figure kind=bogus\nBody\n:::\n");
        assert_eq!(
            figure(&invalid_figure.document.blocks[0]).0,
            FigureKind::Other
        );
        let raw = parsed(":::raw\nBody remains\n:::\n");
        assert_eq!(
            plain_text(paragraph(&raw.document.blocks[0])),
            "Body remains"
        );
        let bibliography =
            parsed(":::bibliography\n:::\n\n:::bibliography\n:::\n");
        assert_eq!(bibliography.document.blocks.len(), 1);
        assert!(codes(&bibliography).contains(&"invalid-directive"));
    }

    #[test]
    fn colon_prepass_info() {
        let result =
            parsed(":::callout {title=\"Fig 1: x\"}\nBody\n:::\n\nAfter\n");
        let Block::Callout { title, span, .. } = &result.document.blocks[0]
        else {
            panic!("callout expected")
        };
        assert_eq!(plain_text(title.as_ref().unwrap()), "Fig 1: x");
        assert_eq!(
            (
                span.as_ref().unwrap().start_line,
                span.as_ref().unwrap().end_line
            ),
            (1, 3)
        );
        assert_eq!(plain_text(paragraph(&result.document.blocks[1])), "After");
        let findings: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "directive-colon-rewritten")
            .collect();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].level, Level::Info);
        assert_eq!(findings[0].span.as_ref().unwrap().start_line, 1);
        assert!(!codes(&result).contains(&"unclosed-directive"));
        let result = parsed(
            ":::figure {#fig:x caption=\"A: figure\"}\n![](images/plot.png)\n:::\n",
        );
        assert_eq!(
            figure(&result.document.blocks[0]).3.id.as_deref(),
            Some("fig:x")
        );
    }

    #[test]
    fn footnotes() {
        let result = parsed(
            "First[^a], again[^a], inline^[Inline *text*].\n\n[^a]: Named body with another[^b].\n\n[^b]: Nested body.\n\n[^unused]: Unused.\n",
        );
        assert_eq!(
            result.document.blocks.len(),
            1,
            "definitions are never body blocks"
        );
        let mut named = Vec::new();
        let mut references = Vec::new();
        let mut anonymous = 0;
        walk_inlines(&result.document, &mut |inline| match inline {
            Inline::Footnote { label, number, blocks } => {
                assert!(number.is_none());
                assert!(!blocks.is_empty());
                if let Some(label) = label {
                    named.push(label.clone());
                } else {
                    anonymous += 1;
                }
            }
            Inline::FootnoteRef { label, number } => {
                assert!(number.is_none());
                references.push(label.clone());
            }
            _ => {}
        });
        assert_eq!(named, ["a", "b"]);
        assert_eq!(references, ["a"]);
        assert_eq!(anonymous, 1);
        assert!(result.document.features.footnotes);
        assert_eq!(
            codes(&result)
                .iter()
                .filter(|&&code| code == "footnote-unreferenced")
                .count(),
            1
        );
        let result = parsed("Self[^s].\n\n[^s]: A self reference[^s].\n");
        let mut first = 0;
        let mut repeated = 0;
        walk_inlines(&result.document, &mut |inline| match inline {
            Inline::Footnote { label, .. } if label.as_deref() == Some("s") => {
                first += 1
            }
            Inline::FootnoteRef { label, .. } if label == "s" => repeated += 1,
            _ => {}
        });
        assert_eq!((first, repeated), (1, 1));
        assert!(result.document.invariant_violations().is_empty());
        let result = parsed("Paragraph\n\nInline^[Body] on line three.\n");
        let mut found = false;
        walk_inlines(&result.document, &mut |inline| {
            if let Inline::Footnote { label: None, blocks, .. } = inline {
                found = true;
                let span = blocks[0].span().unwrap();
                assert_eq!((span.start_line, span.end_line), (3, 3));
            }
        });
        assert!(found);
        assert_eq!(
            plain_text(paragraph(
                &parsed("Undefined[^missing].\n").document.blocks[0]
            )),
            "Undefined[^missing]."
        );
    }

    #[test]
    fn inline_nodes() {
        let result = parsed("\\@key and \\@@other\n");
        let inlines = paragraph(&result.document.blocks[0]);
        assert_eq!(inlines[0], Inline::text("@"));
        assert!(
            matches!(&inlines[1], Inline::Text { text } if text.starts_with("key and "))
        );
        assert!(
            inlines
                .iter()
                .filter(|inline| **inline == Inline::text("@"))
                .count()
                >= 2
        );
        let mut config = Config::default();
        config.markdown.superscript = true;
        config.markdown.subscript = true;
        let result = parse(&source(), "*em* **strong** ~~strike~~ x^2^ H~2~O `code`{.plain}\nsoft  \nhard\n", &config).unwrap();
        let inlines = paragraph(&result.document.blocks[0]);
        for expected in [
            "emph", "strong", "strikeout", "superscript", "subscript", "code",
            "soft_break", "line_break",
        ] {
            let json = serde_json::to_value(inlines).unwrap();
            assert!(
                json.as_array()
                    .unwrap()
                    .iter()
                    .any(|inline| inline["type"] == expected),
                "{expected}: {json}"
            );
        }
        let result = parsed(
            "[link](https://example.org \"Markdown\"){title=attribute .wide} [anchor](#fig-x)\n",
        );
        let links: Vec<_> = paragraph(&result.document.blocks[0])
            .iter()
            .filter_map(|inline| {
                if let Inline::Link { url, title, attrs, .. } = inline {
                    Some((url, title, attrs))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(links[0].0, "https://example.org");
        assert_eq!(links[0].1.as_deref(), Some("Markdown"));
        assert!(!links[0].2.pairs.contains_key("title"));
        assert_eq!(links[1].0, "#fig-x");
        let result =
            parsed("[link](url){title=Attribute} `<b>x</b>`{=html} trailing\n");
        let inlines = paragraph(&result.document.blocks[0]);
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Link { title, .. } if title.as_deref() == Some("Attribute"))));
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Raw { format, text } if format == "html" && text == "<b>x</b>")));
        assert!(plain_text(inlines).ends_with(" trailing"));
    }

    #[test]
    fn labels() {
        let result = parsed(
            "# Heading {#sec-intro .unnumbered}\n\n![Alt](images/plot.png){#fig:x}\n\n$$x$$ {#eq-x}\n\n:::center {#größe}\nBody\n:::\n",
        );
        let Block::Heading { attrs, number, .. } = &result.document.blocks[0]
        else {
            panic!("heading expected")
        };
        assert_eq!(attrs.id.as_deref(), Some("sec-intro"));
        assert_eq!(attrs.classes, ["unnumbered"]);
        assert!(number.is_none());
        assert_eq!(
            figure(&result.document.blocks[1]).3.id.as_deref(),
            Some("fig:x")
        );
        let Block::Div { attrs, .. } = &result.document.blocks[3] else {
            panic!("div expected")
        };
        assert_eq!(attrs.id.as_deref(), Some("größe"));
        let result = parsed("# Heading {#bad!}\n");
        let Block::Heading { attrs, .. } = &result.document.blocks[0] else {
            panic!("heading expected")
        };
        assert!(attrs.id.is_none());
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|finding| finding.code == "label-invalid")
            .expect("invalid label reported");
        assert_eq!(diagnostic.level, Level::Error);
        let result = parsed("# No automatic label\n");
        assert!(
            matches!(&result.document.blocks[0], Block::Heading { attrs, .. } if attrs.id.is_none())
        );
    }

    #[test]
    fn presenter_compat() {
        let result = parsed(
            "--\n\n--- {layout=title}\n\n-- {background=red}\n\nNote: Ordinary text\n\n<!-- .slide: hidden -->\n\n<!-- .element: hidden -->\n\n:::notes\nHidden notes\n:::\n\n:::comment\nHidden comment\n:::\n",
        );
        assert_eq!(result.document.blocks.len(), 1);
        assert_eq!(
            plain_text(paragraph(&result.document.blocks[0])),
            "Note: Ordinary text"
        );
        let separators: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "presenter-separator")
            .collect();
        assert_eq!(separators.len(), 3);
        assert!(separators.iter().all(|finding| finding.level == Level::Info));
        let result = parsed(
            "# Heading {LAYOUT=title background=red background-image=photo transition=fade transition-speed=fast auto-animate=true auto-animate-id=x visibility=hidden state=ready timing=2 autoslide=5 menu-title=Menu data-state=keep aria-label=Accessible custom=keep}\n",
        );
        let Block::Heading { attrs, .. } = &result.document.blocks[0] else {
            panic!("heading expected")
        };
        assert_eq!(attrs.pairs.len(), 3);
        assert_eq!(attrs.pairs["data-state"], "keep");
        assert_eq!(attrs.pairs["aria-label"], "Accessible");
        assert_eq!(attrs.pairs["custom"], "keep");
        assert!(codes(&result).contains(&"presenter-attr-dropped"));
        let mut config = Config::default();
        config.markdown.smart_punctuation = true;
        let result = parse(&source(), "--\n\nKept\n", &config).unwrap();
        assert_eq!(result.document.blocks.len(), 1);
        assert!(codes(&result).contains(&"presenter-separator"));
        let result = parsed("Intro\n\n---\n");
        assert!(matches!(
            result.document.blocks[1],
            Block::ThematicBreak { .. }
        ));
        assert!(!codes(&result).contains(&"presenter-separator"));
    }

    #[test]
    fn front_matter_and_toc() {
        let config = Config::default();
        let before = serde_json::to_value(&config).unwrap();
        let result = parse(&source(), "---\ntitle: Title\nauthor: [A, B]\ntoc: 2\nmarkdown: {superscript: true}\nabstract: |\n  Abstract $x$ and note^[A note].\n---\n\n# Heading\n\nx^2^\n", &config).unwrap();
        assert_eq!(serde_json::to_value(&config).unwrap(), before);
        assert_eq!(result.document.meta.title.as_deref(), Some("Title"));
        assert_eq!(result.document.meta.authors.len(), 2);
        assert!(!result.document.meta.vars.contains_key("abstract"));
        let abstract_ = result.document.meta.abstract_.as_ref().unwrap();
        assert_eq!(abstract_[0].span().unwrap().start_line, 1);
        assert!(
            result.document.features.math && result.document.features.footnotes
        );
        assert!(matches!(
            result.document.blocks[0],
            Block::Toc { depth: 2, span: None }
        ));
        assert!(
            paragraph(&result.document.blocks[2])
                .iter()
                .any(|inline| matches!(inline, Inline::Superscript { .. }))
        );
        assert!(codes(&result).contains(&"front-matter-author-list"));
        assert!(
            result.document.includes.is_empty()
                && result.document.resolved.is_empty()
        );
        let result = parsed("---\ntoc: 2\n---\n\n:::toc depth=4\n:::\n");
        assert_eq!(result.document.blocks.len(), 1);
        assert!(matches!(
            result.document.blocks[0],
            Block::Toc { depth: 4, span: Some(_) }
        ));
        let result =
            parsed("---\ntoc: 2\n---\n\n::::center\n:::toc\n:::\n::::\n");
        assert!(
            !matches!(result.document.blocks[0], Block::Toc { .. }),
            "a nested explicit toc suppresses automatic insertion"
        );
        let result = parsed("---\n---\n\nBody\n\n---\n\nAfter\n");
        assert_eq!(result.document.blocks.len(), 3);
        assert_eq!(plain_text(paragraph(&result.document.blocks[0])), "Body");
        assert!(matches!(
            result.document.blocks[1],
            Block::ThematicBreak { .. }
        ));
        assert_eq!(plain_text(paragraph(&result.document.blocks[2])), "After");
        assert_eq!(result.document.meta, crate::ir::DocMeta::default());
        assert!(parsed("# A heading\n").document.meta.title.is_none());
        let result = parsed("---\n\n# Swallowed heading\n\n---\n\nBody\n");
        assert!(codes(&result).contains(&"front-matter-empty"));
        let result = parsed("---\ntitle: T\n\n# Lost heading\n\n---\n");
        assert!(codes(&result).contains(&"front-matter-unclosed"));
        assert!(matches!(
            parse(&source(), "---\ntitle: [bad]\n---\n", &config),
            Err(Error::FrontMatter(_))
        ));
    }

    #[test]
    fn features_include_titles_captions_and_abstract() {
        for text in [
            "---\nabstract: '$x$'\n---\n\nBody\n",
            ":::note title='$x$'\nBody\n:::\n",
            "```txt {caption='$x$'}\ncode\n```\n",
            "Table: $x$\n\n| a |\n|---|\n| b |\n",
        ] {
            assert!(parsed(text).document.features.math, "{text}");
        }
        let result =
            parsed("---\nabstract: |\n  ```txt\n  code\n  ```\n---\n\nBody\n");
        assert!(result.document.features.code);
    }

    #[test]
    fn spans() {
        let text = "# Heading\n\nParagraph\ncontinues\n\n```txt\ncode\n```\n\n> Quote\n\n:::note\nBody\n:::\n";
        let result = parsed(text);
        let found: Vec<_> = result
            .document
            .blocks
            .iter()
            .map(|block| {
                let span = block.span().expect("every source block has a span");
                assert_eq!(span.file, None);
                assert_eq!(span.cell, None);
                (span.start_line, span.end_line)
            })
            .collect();
        assert_eq!(found, [(1, 1), (3, 4), (6, 8), (10, 10), (12, 14)]);
        walk_blocks(&result.document, &mut |block| {
            let span = block.span().unwrap();
            assert!(span.start_line >= 1 && span.end_line >= span.start_line);
            assert!(span.end_line <= text.lines().count());
        });
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let base_dir = fixture_dir();
        let file = base_dir.join("included.md");
        let mut ctx = Fragment {
            assets: &mut assets,
            base_dir: &base_dir,
            diagnostics: &mut diagnostics,
            file: Some(&file),
            cell: Some(8),
            missing_asset_is_error: false,
        };
        let blocks = parse_blocks(
            "# Local\n\n:::note title='# Not inline'\nBody\n:::\n",
            &Config::default(),
            &mut ctx,
        )
        .unwrap();
        for block in &blocks {
            let span = block.span().unwrap();
            assert_eq!(span.file.as_ref(), Some(&file));
            assert_eq!(span.cell, Some(8));
        }
        let finding = diagnostics
            .iter()
            .find(|finding| finding.code == "fragment-not-inline")
            .expect("title warning");
        let span = finding.span.as_ref().unwrap();
        assert_eq!((span.start_line, span.end_line), (3, 5));
        assert_eq!(span.file.as_ref(), Some(&file));
        assert_eq!(span.cell, Some(8));
        assert_eq!(finding.path.as_ref(), Some(&file));
    }

    #[test]
    fn fragments() {
        let config = Config::default();
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let base_dir = fixture_dir();
        let file = base_dir.join("fragment.md");
        let mut ctx = Fragment {
            assets: &mut assets,
            base_dir: &base_dir,
            diagnostics: &mut diagnostics,
            file: Some(&file),
            cell: Some(4),
            missing_asset_is_error: false,
        };
        assert!(parse_inlines("", &config, &mut ctx).unwrap().is_empty());
        assert!(ctx.diagnostics.iter().next().is_none());
        let inlines = parse_inlines("a\n\nb", &config, &mut ctx).unwrap();
        assert_eq!(plain_text(&inlines), "a");
        let finding = ctx
            .diagnostics
            .iter()
            .find(|finding| finding.code == "fragment-not-inline")
            .unwrap();
        assert_eq!(finding.span.as_ref().unwrap().cell, Some(4));
        assert_eq!(finding.path.as_ref(), Some(&file));
        let inline =
            parse_inlines("one *paragraph*", &config, &mut ctx).unwrap();
        assert_eq!(plain_text(&inline), "one paragraph");
        assert_eq!(
            ctx.diagnostics
                .iter()
                .filter(|finding| finding.code == "fragment-not-inline")
                .count(),
            1
        );
        let blocks =
            parse_blocks("---\ntitle: Ordinary text\n---\n", &config, &mut ctx)
                .unwrap();
        assert!(matches!(blocks[0], Block::ThematicBreak { .. }));
        assert!(blocks.iter().any(|block| matches!(block, Block::Heading { inlines, .. } if plain_text(inlines) == "title: Ordinary text")));
        let _ = parse_blocks(
            ":::callout title='A: title'\nBody\n:::\n\n# Bad {#bad!}\n",
            &config,
            &mut ctx,
        )
        .unwrap();
        for finding in ctx.diagnostics.iter() {
            assert_eq!(finding.path.as_ref(), Some(&file));
            let span = finding.span.as_ref().unwrap();
            assert_eq!(span.cell, Some(4));
            assert_eq!(span.file.as_ref(), Some(&file));
        }
    }

    #[test]
    fn include_errors_are_located_and_fragments_cannot_include() {
        match parse(
            &source(),
            "Before\n\n:::include definitely-not-present.md\n:::\n\nAfter\n",
            &Config::default(),
        ) {
            Err(Error::Parse { path, line, message }) => {
                assert_eq!(path, source().path);
                assert_eq!(line, Some(3));
                assert!(message.contains("file not found"));
            }
            _ => panic!("missing include must be a parse error"),
        }
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let base_dir = fixture_dir();
        let mut ctx = Fragment {
            assets: &mut assets,
            base_dir: &base_dir,
            diagnostics: &mut diagnostics,
            file: None,
            cell: None,
            missing_asset_is_error: false,
        };
        assert!(
            parse_blocks(
                ":::include missing.md\n:::\n",
                &Config::default(),
                &mut ctx
            )
            .unwrap()
            .is_empty()
        );
        let finding = diagnostics.iter().next().unwrap();
        assert_eq!(finding.code, "include-not-top-level");
        assert_eq!(finding.level, Level::Error);
    }

    #[test]
    fn disabled_extensions_never_reach_the_walker() {
        let result = parsed(
            "__u__\n\n==h==\n\n++i++\n\n||s||\n\n>>> quoted\n\n-# x\n\n%%c%%\n\n[[w]]\n",
        );
        assert!(
            matches!(&paragraph(&result.document.blocks[0])[0], Inline::Strong { inlines } if plain_text(inlines) == "u")
        );
        for (index, text) in [
            (1, "==h=="),
            (2, "++i++"),
            (3, "||s||"),
            (5, "-# x"),
            (6, "%%c%%"),
            (7, "[[w]]"),
        ] {
            assert_eq!(
                plain_text(paragraph(&result.document.blocks[index])),
                text
            );
        }
        let Block::BlockQuote { blocks, .. } = &result.document.blocks[4]
        else {
            panic!("nested quote expected")
        };
        let Block::BlockQuote { blocks, .. } = &blocks[0] else {
            panic!("second quote expected")
        };
        assert!(matches!(blocks[0], Block::BlockQuote { .. }));
        assert!(result.diagnostics.iter().next().is_none());
        assert!(result.document.invariant_violations().is_empty());
    }

    #[test]
    fn unexpected_nodes_are_errors() {
        let options =
            crate::markdown::comrak_options(&Config::default().markdown);
        let arena = comrak::Arena::new();
        let root = comrak::parse_document(&arena, "Text\n", &options);
        root.first_child().unwrap().data_mut().value =
            comrak::nodes::NodeValue::Raw("injected".into());
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let base_dir = fixture_dir();
        let mut ctx = Fragment {
            assets: &mut assets,
            base_dir: &base_dir,
            diagnostics: &mut diagnostics,
            file: None,
            cell: None,
            missing_asset_is_error: false,
        };
        let err =
            super::walk(root, "Text\n", &Config::default(), &mut ctx, true)
                .unwrap_err();
        assert!(matches!(err, Error::Other(_)), "{err}");
    }
}
