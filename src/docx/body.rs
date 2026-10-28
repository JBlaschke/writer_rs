//! Document body and block rendering (docx-output.md §4.1–§4.7).
//! Owner: T40.

use std::sync::LazyLock;

use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference};
use syntect::util::LinesWithEndings;

use crate::html::highlight::{SYNTAXES, SyntaxChoice, resolve_syntax};
use crate::ir::{
    Attrs, Block, Cell, DefinitionItem, Inline, ListItem, ListKind, OutputKind,
};
use crate::markdown::code::CodeAttrs;
use crate::resolve::{Term, term};

use super::xml::{self, ParaProps, RunProps};
use super::{Container, Emitter, StyleKind};

/// Ordered scope prefixes mapped to pandoc's character-style vocabulary.
/// Innermost matching scopes take priority, then rows in this order.
pub const SCOPE_TOKENS: &[(&str, &str)] = &[
    ("comment.block.documentation", "DocumentationTok"),
    ("comment.line.documentation", "DocumentationTok"),
    ("comment", "CommentTok"),
    ("string.regexp", "SpecialStringTok"),
    ("constant.character.escape", "SpecialCharTok"),
    ("constant.character", "CharTok"),
    ("string", "StringTok"),
    ("constant.numeric.float", "FloatTok"),
    ("constant.numeric.integer.hexadecimal", "BaseNTok"),
    ("constant.numeric.integer.octal", "BaseNTok"),
    ("constant.numeric.integer.binary", "BaseNTok"),
    ("constant.numeric", "DecValTok"),
    ("constant", "ConstantTok"),
    ("keyword.control.import", "ImportTok"),
    ("keyword.control.include", "ImportTok"),
    ("meta.import", "ImportTok"),
    ("meta.preprocessor", "PreprocessorTok"),
    ("keyword.control.directive", "PreprocessorTok"),
    ("keyword.control", "ControlFlowTok"),
    ("keyword.operator", "OperatorTok"),
    ("keyword", "KeywordTok"),
    ("storage.modifier", "KeywordTok"),
    ("entity.name.tag", "KeywordTok"),
    ("meta.annotation", "AnnotationTok"),
    ("meta.decorator", "AnnotationTok"),
    ("storage.type.annotation", "AnnotationTok"),
    ("storage.type", "DataTypeTok"),
    ("entity.name.type", "DataTypeTok"),
    ("entity.name.class", "DataTypeTok"),
    ("support.type", "DataTypeTok"),
    ("support.class", "DataTypeTok"),
    ("support.function.builtin", "BuiltInTok"),
    ("support.function", "FunctionTok"),
    ("entity.name.function", "FunctionTok"),
    ("entity.other.attribute-name", "AttributeTok"),
    ("variable", "VariableTok"),
    ("invalid", "ErrorTok"),
];

static TOKEN_SCOPES: LazyLock<Vec<(Scope, &'static str)>> =
    LazyLock::new(|| {
        SCOPE_TOKENS
            .iter()
            .map(|&(prefix, style)| {
                // The literal scope prefixes above are valid syntect scopes.
                (Scope::new(prefix).expect("valid built-in scope"), style)
            })
            .collect()
    });

/// Context that is temporarily replaced when emitting a footnote part.
#[derive(Default)]
pub(crate) struct BodyState {
    lists: Vec<ListFrame>,
    in_abstract: bool,
}

struct ListFrame {
    level: u8,
    number: u32,
    first: bool,
    checked: Option<bool>,
}

impl Emitter<'_> {
    /// Writes metadata before the body in the fixed title-block order.
    pub(crate) fn document(&mut self) {
        let meta = &self.doc.meta;
        for (text, style) in [
            (meta.title.as_deref(), "Title"),
            (meta.subtitle.as_deref(), "Subtitle"),
        ] {
            if let Some(text) = text {
                self.body_text_paragraph(text, style);
            }
        }
        for author in &meta.authors {
            let style = self.styles.ensure("Author", StyleKind::Paragraph);
            self.start_paragraph(ParaProps {
                style: Some(style),
                ..Default::default()
            });
            self.body_text(&author.name, &RunProps::default());
            if let Some(affiliation) = &author.affiliation {
                self.buffer().push_str("<w:r><w:br/></w:r>");
                self.body_text(affiliation, &RunProps::default());
            }
            if let Some(email) = &author.email {
                self.buffer().push_str("<w:r><w:br/></w:r>");
                self.inlines(
                    &[Inline::Link {
                        url: format!("mailto:{email}"),
                        title: None,
                        inlines: vec![Inline::Text { text: email.clone() }],
                        attrs: Attrs::default(),
                    }],
                    &RunProps::default(),
                );
            }
            self.end_paragraph();
        }
        if let Some(date) = &meta.date {
            self.body_text_paragraph(date, "Date");
        }
        if let Some(blocks) = &meta.abstract_ {
            let title = meta
                .vars
                .get("abstract-title")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| term(&self.lang, Term::Abstract));
            self.body_text_paragraph(&title, "Abstract Title");
            let style = self.styles.ensure("Abstract", StyleKind::Paragraph);
            self.containers.push(Container::Style(style));
            self.content.body.in_abstract = true;
            self.blocks(blocks);
            self.content.body.in_abstract = false;
            self.containers.pop();
        }
        self.blocks(&self.doc.blocks);
        self.finish_block_sequence();
    }

    /// Writes siblings, preserving paragraph adjacency across raw blocks.
    pub(crate) fn blocks(&mut self, blocks: &[Block]) {
        let mut preceding_paragraph = false;
        for block in blocks {
            let previous_span = self.content.span.clone();
            if let Some(span) = block.span() {
                self.content.span = Some(span.clone());
            }
            self.block(block, preceding_paragraph);
            self.content.span = previous_span;
            if !matches!(block, Block::Raw { .. }) {
                preceding_paragraph = matches!(block, Block::Paragraph { .. });
            }
        }
    }

    fn block(&mut self, block: &Block, preceding_paragraph: bool) {
        match block {
            Block::Paragraph { inlines, .. } => {
                if !inlines.is_empty() {
                    let style = self.paragraph_style(preceding_paragraph);
                    self.inline_paragraph(inlines, style, false);
                }
            }
            Block::Heading { level, inlines, attrs, number, .. } => {
                self.heading(*level, inlines, attrs, number.as_deref());
            }
            Block::BlockQuote { blocks, .. } => {
                let style = if self.containers.contains(&Container::Footnote) {
                    "Footnote Block Text"
                } else {
                    "Block Text"
                };
                self.styled_blocks(blocks, style, false);
            }
            Block::List { kind, tight, items, .. } => {
                self.list(kind, *tight, items);
            }
            Block::DefinitionList { items, .. } => self.definitions(items),
            Block::CodeBlock { lang, text, attrs, .. } => {
                let bookmark =
                    attrs.id.as_deref().and_then(|id| self.start_bookmark(id));
                self.code_block(lang.as_deref(), text, attrs);
                self.end_bookmark(bookmark);
            }
            Block::Callout { kind, title, blocks, attrs, .. } => {
                self.callout(kind, title.as_deref(), blocks, attrs);
            }
            Block::Div { blocks, attrs, .. } => self.div(blocks, attrs),
            Block::Cell(cell) => self.cell(cell),
            Block::Raw { format, text, .. } if format == "openxml" => {
                self.raw_openxml(text, false);
            }
            Block::Raw { .. } => {}
            Block::ThematicBreak { .. } => {
                let style =
                    self.styles.ensure("Horizontal Rule", StyleKind::Paragraph);
                self.start_paragraph(ParaProps {
                    style: Some(style),
                    ..Default::default()
                });
                self.end_paragraph();
            }
            Block::PageBreak { .. } => {
                self.start_paragraph(ParaProps::default());
                self.buffer().push_str("<w:r><w:br w:type=\"page\"/></w:r>");
                self.end_paragraph();
            }
            Block::Figure { .. } => self.figure(block),
            Block::Table { attrs, .. } => {
                let bookmark =
                    attrs.id.as_deref().and_then(|id| self.start_bookmark(id));
                self.separate_table();
                self.table(block, None);
                self.end_bookmark(bookmark);
            }
            Block::Math { .. } => self.math_block(block),
            Block::Toc { depth, .. } => self.toc(*depth),
            Block::Bibliography { .. } => self.bibliography(),
        }
    }

    fn paragraph_style(&mut self, preceding_paragraph: bool) -> String {
        let mut tight = false;
        for container in self.containers.iter().rev() {
            let name = match container {
                Container::Style(style) => return style.clone(),
                Container::Footnote => "Footnote Text",
                Container::TableCell => "Compact",
                Container::TightList => {
                    tight = true;
                    continue;
                }
                Container::CellOutput => continue,
            };
            return self.styles.ensure(name, StyleKind::Paragraph);
        }
        let name = if tight {
            "Compact"
        } else if preceding_paragraph {
            "Body Text"
        } else {
            "First Paragraph"
        };
        self.styles.ensure(name, StyleKind::Paragraph)
    }

    fn heading(
        &mut self, level: u8, inlines: &[Inline], attrs: &Attrs,
        number: Option<&str>,
    ) {
        let unlisted = self.content.body.in_abstract
            || attrs.classes.iter().any(|c| c == "notoc" || c == "unlisted");
        let name = if unlisted {
            format!("Heading {level} Unlisted")
        } else {
            format!("heading {level}")
        };
        let style = self.styles.ensure(&name, StyleKind::Paragraph);
        self.start_paragraph(ParaProps {
            style: Some(style),
            ..Default::default()
        });
        let bookmark =
            attrs.id.as_deref().and_then(|id| self.start_bookmark(id));
        if let Some(number) = number {
            let style =
                self.styles.ensure("Section Number", StyleKind::Character);
            self.body_text(
                number,
                &RunProps { style: Some(style), ..Default::default() },
            );
            self.buffer().push_str("<w:r><w:tab/></w:r>");
        }
        self.inlines(inlines, &RunProps::default());
        self.end_bookmark(bookmark);
        self.end_paragraph();
    }

    fn inline_paragraph(
        &mut self, inlines: &[Inline], style: String, keep_next: bool,
    ) {
        self.start_paragraph(ParaProps {
            style: Some(style),
            keep_next,
            ..Default::default()
        });
        self.inlines(inlines, &RunProps::default());
        if matches!(inlines, [Inline::Math { .. }]) {
            self.body_text("\u{200b}", &RunProps::default());
        }
        self.end_paragraph();
    }

    fn body_text(&mut self, text: &str, props: &RunProps) {
        self.inlines(&[Inline::Text { text: text.into() }], props);
    }

    fn body_text_paragraph(&mut self, text: &str, name: &str) {
        let style = self.styles.ensure(name, StyleKind::Paragraph);
        self.inline_paragraph(
            &[Inline::Text { text: text.into() }],
            style,
            false,
        );
    }

    fn styled_blocks(&mut self, blocks: &[Block], name: &str, custom: bool) {
        let style = if custom {
            self.styles.custom(name, StyleKind::Paragraph, None)
        } else {
            self.styles.ensure(name, StyleKind::Paragraph)
        };
        self.containers.push(Container::Style(style));
        self.blocks(blocks);
        self.containers.pop();
    }

    fn definitions(&mut self, items: &[DefinitionItem]) {
        for item in items {
            let style =
                self.styles.ensure("Definition Term", StyleKind::Paragraph);
            self.inline_paragraph(&item.term, style, false);
            for blocks in &item.definitions {
                self.styled_blocks(blocks, "Definition", false);
            }
        }
    }

    /// Starts a paragraph, including deferred list and footnote markers.
    pub(crate) fn start_paragraph(&mut self, mut props: ParaProps) {
        if props.style.as_deref().is_some_and(|style| {
            self.styles.id_for("heading 1", StyleKind::Paragraph) == Some(style)
        }) {
            self.heading_one();
        }
        let marker = if let Some(frame) = self.content.body.lists.last_mut() {
            let left = 720 * (u32::from(frame.level) + 1);
            if frame.first {
                frame.first = false;
                if let Some(checked) = frame.checked {
                    props.ind = Some(format!(
                        "<w:ind w:left=\"{left}\" w:hanging=\"360\"/>"
                    ));
                    Some(checked)
                } else {
                    props.num.get_or_insert((frame.level, frame.number));
                    None
                }
            } else {
                if props.num.is_none() && props.ind.is_none() {
                    props.ind = Some(format!(
                        "<w:ind w:left=\"{left}\" w:firstLine=\"0\"/>"
                    ));
                }
                None
            }
        } else {
            None
        };
        self.buffer().push_str(&format!("<w:p>{}", props.to_xml()));
        let prefix = std::mem::take(&mut self.content.paragraph_prefix);
        self.buffer().push_str(&prefix);
        if let Some(checked) = marker {
            let props = RunProps {
                fonts: Some(
                    concat!(
                        "<w:rFonts w:ascii=\"Segoe UI Symbol\" ",
                        "w:hAnsi=\"Segoe UI Symbol\"/>"
                    )
                    .into(),
                ),
                ..Default::default()
            };
            self.body_text(if checked { "☒" } else { "☐" }, &props);
            self.buffer().push_str("<w:r><w:tab/></w:r>");
        }
    }

    /// Closes the current paragraph.
    pub(crate) fn end_paragraph(&mut self) {
        self.buffer().push_str("</w:p>");
    }

    fn list(&mut self, kind: &ListKind, tight: bool, items: &[ListItem]) {
        // A nested list still needs its parent's otherwise-empty marker.
        self.ensure_list_marker();
        let depth = self.content.body.lists.len();
        if depth > 8 {
            self.content_warning(
                "unsupported-construct",
                "DOCX list nesting is capped at nine levels",
            );
        }
        let level = depth.min(8) as u8;
        let number = match kind {
            ListKind::Bullet => self.numbering.bullet(),
            ListKind::Ordered { start } => self.numbering.ordered(*start),
        };
        if tight {
            self.containers.push(Container::TightList);
        }
        for item in items {
            self.content.body.lists.push(ListFrame {
                level,
                number,
                first: true,
                checked: item.checked,
            });
            self.blocks(&item.blocks);
            self.ensure_list_marker();
            self.content.body.lists.pop();
        }
        if tight {
            self.containers.pop();
        }
    }

    fn ensure_list_marker(&mut self) {
        if self.content.body.lists.last().is_some_and(|frame| frame.first) {
            let style = self.paragraph_style(false);
            self.start_paragraph(ParaProps {
                style: Some(style),
                ..Default::default()
            });
            self.end_paragraph();
        }
    }

    fn div(&mut self, blocks: &[Block], attrs: &Attrs) {
        let bookmark =
            attrs.id.as_deref().and_then(|id| self.start_bookmark(id));
        if let Some(name) = attrs.pairs.get("custom-style") {
            self.styled_blocks(blocks, name, true);
        } else {
            self.blocks(blocks);
        }
        self.end_bookmark(bookmark);
    }

    fn callout(
        &mut self, kind: &str, title: Option<&[Inline]>, blocks: &[Block],
        attrs: &Attrs,
    ) {
        let bookmark =
            attrs.id.as_deref().and_then(|id| self.start_bookmark(id));
        let name = capitalized(kind);
        let style_name = format!("Callout {name}");
        let localized = match kind {
            "note" => Some(Term::Note),
            "tip" => Some(Term::Tip),
            "important" => Some(Term::Important),
            "warning" => Some(Term::Warning),
            "caution" => Some(Term::Caution),
            _ => None,
        };
        let style = if localized.is_some() {
            self.styles.ensure(&style_name, StyleKind::Paragraph)
        } else {
            self.styles.custom(
                &style_name,
                StyleKind::Paragraph,
                Some("Callout"),
            )
        };
        self.containers.push(Container::Style(style.clone()));
        self.start_paragraph(ParaProps {
            style: Some(style),
            keep_next: true,
            ..Default::default()
        });
        let title_style =
            self.styles.ensure("Callout Title", StyleKind::Character);
        let props = RunProps { style: Some(title_style), ..Default::default() };
        if let Some(title) = title {
            self.inlines(title, &props);
        } else {
            let title = localized.map_or(name, |term_| term(&self.lang, term_));
            self.body_text(&title, &props);
        }
        self.end_paragraph();
        self.blocks(blocks);
        self.containers.pop();
        self.end_bookmark(bookmark);
    }

    fn cell(&mut self, cell: &Cell) {
        let bookmark =
            cell.attrs.id.as_deref().and_then(|id| self.start_bookmark(id));
        if cell.show_input {
            if self.config.notebook.show_prompts {
                self.cell_prompt("In", cell.execution_count);
            }
            let language =
                (!cell.language.is_empty()).then_some(cell.language.as_str());
            // The outer cell already owns this label's bookmark.
            let mut attrs = cell.attrs.clone();
            attrs.id = None;
            self.code_block(language, &cell.source, &attrs);
        }
        if cell.show_output {
            self.containers.push(Container::CellOutput);
            for output in &cell.outputs {
                if self.config.notebook.show_prompts
                    && matches!(output.kind, OutputKind::Result)
                    && output.execution_count.is_some()
                {
                    self.cell_prompt("Out", output.execution_count);
                }
                self.blocks(output.pick("docx"));
            }
            self.containers.pop();
        }
        self.end_bookmark(bookmark);
    }

    fn cell_prompt(&mut self, name: &str, count: Option<i64>) {
        let count = count.map_or_else(|| " ".into(), |n| n.to_string());
        self.body_text_paragraph(&format!("{name} [{count}]:"), "Cell Prompt");
    }

    fn code_block(&mut self, lang: Option<&str>, text: &str, attrs: &Attrs) {
        let code = CodeAttrs::from_attrs(attrs);
        let output = self.containers.contains(&Container::CellOutput);
        let unhighlighted = output
            || attrs.classes.iter().any(|class| {
                class == "output-text"
                    || class.starts_with("output-stream-")
                    || class == "output-error"
                    || class == "output-json"
            });
        let syntax = resolve_syntax(lang, code.plain || unhighlighted);
        let normalized = text.replace("\r\n", "\n");
        let text = normalized.strip_suffix('\n').unwrap_or(&normalized);
        let highlighted = match syntax {
            SyntaxChoice::Syntax(syntax) => highlight(syntax, text),
            _ => None,
        };
        let lines = highlighted.unwrap_or_else(|| {
            text.split('\n')
                .map(|line| vec![("Verbatim Char", line.to_owned())])
                .collect()
        });
        let name = if output { "Cell Output" } else { "Source Code" };
        let style = self.styles.ensure(name, StyleKind::Paragraph);
        self.start_paragraph(ParaProps {
            style: Some(style),
            ..Default::default()
        });
        if let Some(pair) = attrs.pairs.get("ansi") {
            self.ansi_code_lines(pair, &code);
        } else {
            self.code_lines(&lines, &code);
        }
        self.end_paragraph();
    }

    fn ansi_code_lines(&mut self, pair: &str, code: &CodeAttrs) {
        use crate::notebook::ansi;

        let spans = ansi::parse(pair);
        let mut lines = vec![Vec::new()];
        for span in &spans {
            for (index, text) in span.text.split('\n').enumerate() {
                if index > 0 {
                    lines.push(Vec::new());
                }
                if !text.is_empty() {
                    // The initial line exists; subsequent lines are pushed above.
                    lines
                        .last_mut()
                        .expect("output line exists")
                        .push((span.style, text));
                }
            }
        }
        if spans.last().is_some_and(|span| span.text.ends_with('\n')) {
            lines.pop();
        }
        let last =
            code.start.saturating_add(lines.len().saturating_sub(1) as u64);
        let width = last.to_string().len();
        let style = self.styles.ensure("Verbatim Char", StyleKind::Character);
        for (index, segments) in lines.iter().enumerate() {
            if index > 0 {
                self.buffer().push_str("<w:r><w:br/></w:r>");
            }
            let number = code.start.saturating_add(index as u64);
            let highlight = code.is_highlighted(number).then(|| {
                "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"FFF3BF\"/>".into()
            });
            if code.numbered {
                let style = self
                    .styles
                    .ensure("Code Line Number", StyleKind::Character);
                self.code_run(
                    &format!("{number:>width$} "),
                    RunProps {
                        style: Some(style),
                        shading: highlight.clone(),
                        ..Default::default()
                    },
                );
            }
            for (ansi_style, text) in segments {
                self.code_run(
                    text,
                    ansi_run_props(ansi_style, &style, highlight.clone()),
                );
            }
        }
    }

    fn code_lines(&mut self, lines: &[Vec<(&str, String)>], code: &CodeAttrs) {
        let last =
            code.start.saturating_add(lines.len().saturating_sub(1) as u64);
        let width = last.to_string().len();
        for (index, tokens) in lines.iter().enumerate() {
            if index > 0 {
                self.buffer().push_str("<w:r><w:br/></w:r>");
            }
            let number = code.start.saturating_add(index as u64);
            let shading = code.is_highlighted(number).then(|| {
                "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"FFF3BF\"/>".into()
            });
            if code.numbered {
                let style = self
                    .styles
                    .ensure("Code Line Number", StyleKind::Character);
                self.code_run(
                    &format!("{number:>width$} "),
                    RunProps {
                        style: Some(style),
                        shading: shading.clone(),
                        ..Default::default()
                    },
                );
            }
            for (name, text) in tokens {
                let style = self.styles.ensure(name, StyleKind::Character);
                self.code_run(
                    text,
                    RunProps {
                        style: Some(style),
                        shading: shading.clone(),
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn code_run(&mut self, text: &str, props: RunProps) {
        // Code keeps tabs and line boundaries; inline text normalizes newlines.
        let mut body = String::new();
        for (index, part) in text.split('\t').enumerate() {
            if index > 0 {
                body.push_str("<w:tab/>");
            }
            if !part.is_empty() {
                body.push_str(&format!(
                    "<w:t xml:space=\"preserve\">{}</w:t>",
                    xml::escape(part),
                ));
            }
        }
        self.buffer()
            .push_str(&format!("<w:r>{}{body}</w:r>", props.to_xml(),));
    }

    /// Inserts a well-formed raw fragment, validating against the output root.
    pub(crate) fn raw_openxml(&mut self, text: &str, inline: bool) {
        let tag = if inline { "w:p" } else { "w:body" };
        let namespaces = super::document_namespaces(None);
        let wrapped = format!("<{tag}{namespaces}>{text}</{tag}>");
        match super::validate::parse_xml(&wrapped) {
            Ok(tree) => {
                if !inline {
                    self.ensure_list_marker();
                    if tree
                        .children
                        .iter()
                        .find(|node| {
                            !matches!(
                                node.name.as_str(),
                                "w:bookmarkStart" | "w:bookmarkEnd"
                            )
                        })
                        .is_some_and(|node| node.name == "w:tbl")
                    {
                        self.separate_table();
                    }
                }
                self.buffer().push_str(text);
            }
            Err(error) => self.content_warning(
                "docx-raw-invalid",
                format!("invalid raw OpenXML: {error}"),
            ),
        }
    }

    pub(crate) fn separate_table(&mut self) {
        self.ensure_list_marker();
        if last_element_is_table(self.buffer()) {
            self.buffer().push_str("<w:p/>");
        }
    }

    /// OOXML bodies, footnotes and cells must not end with a table.
    pub(crate) fn finish_block_sequence(&mut self) {
        if last_element_is_table(self.buffer()) {
            self.buffer().push_str("<w:p/>");
        }
    }
}

fn capitalized(kind: &str) -> String {
    let mut chars = kind.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn last_element_is_table(mut text: &str) -> bool {
    loop {
        text = text.trim_end();
        if text.ends_with("-->") {
            let Some(start) = text.rfind("<!--") else {
                return false;
            };
            text = &text[..start];
            continue;
        }
        if text.ends_with("?>") {
            let Some(start) = text.rfind("<?") else {
                return false;
            };
            text = &text[..start];
            continue;
        }
        if !text.ends_with("/>") {
            return text.ends_with("</w:tbl>");
        }
        let Some(start) = text.rfind('<') else {
            return false;
        };
        let end = &text[start..];
        if empty_tag_is(end, "w:bookmarkEnd")
            || empty_tag_is(end, "w:bookmarkStart")
        {
            text = &text[..start];
        } else {
            return empty_tag_is(end, "w:tbl");
        }
    }
}

fn empty_tag_is(text: &str, name: &str) -> bool {
    text.strip_prefix('<').and_then(|text| text.strip_prefix(name)).is_some_and(
        |suffix| suffix.starts_with(|c: char| c.is_whitespace() || c == '/'),
    )
}

fn ansi_run_props(
    style: &anstyle::Style, character_style: &str, highlight: Option<String>,
) -> RunProps {
    use crate::notebook::ansi;
    use anstyle::Effects;

    let effects = style.get_effects();
    let inverse = effects.contains(Effects::INVERT);
    let color = |color: Option<anstyle::Color>, default: &str| {
        color
            .map(|color| {
                let (red, green, blue) = ansi::rgb(color);
                format!("{red:02X}{green:02X}{blue:02X}")
            })
            .or_else(|| inverse.then(|| default.to_owned()))
    };
    let (foreground, background) = ansi::colors(style);
    let shading = color(background, "000000").map(|color| {
        format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{color}\"/>")
    });
    RunProps {
        style: Some(character_style.into()),
        bold: effects.contains(Effects::BOLD),
        italic: effects.contains(Effects::ITALIC),
        strike: effects.contains(Effects::STRIKETHROUGH),
        color: color(foreground, "FFFFFF"),
        underline: effects.contains(Effects::UNDERLINE),
        shading: shading.or(highlight),
        ..Default::default()
    }
}

fn highlight(
    syntax: &SyntaxReference, text: &str,
) -> Option<Vec<Vec<(&'static str, String)>>> {
    if text.is_empty() {
        return Some(vec![Vec::new()]);
    }
    let mut parser = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut lines = Vec::new();
    // Keep newline-aware lexing active while excluding terminators from runs.
    for line in LinesWithEndings::from(&format!("{text}\n")) {
        let operations = parser.parse_line(line, &SYNTAXES).ok()?;
        let body = line.strip_suffix('\n').unwrap_or(line);
        let mut tokens: Vec<(&'static str, String)> = Vec::new();
        for (region, operation) in ScopeRegionIterator::new(&operations, line) {
            stack.apply(operation).ok()?;
            let region = region.strip_suffix('\n').unwrap_or(region);
            if region.is_empty() {
                continue;
            }
            let style = token_style(&stack);
            if let Some((previous, text)) = tokens.last_mut()
                && *previous == style
            {
                text.push_str(region);
            } else {
                tokens.push((style, region.to_owned()));
            }
        }
        if body.is_empty() {
            tokens.clear();
        }
        lines.push(tokens);
    }
    Some(lines)
}

fn token_style(stack: &ScopeStack) -> &'static str {
    stack
        .as_slice()
        .iter()
        .rev()
        .find_map(|scope| {
            TOKEN_SCOPES.iter().find_map(|(prefix, style)| {
                prefix.is_prefix_of(*scope).then_some(*style)
            })
        })
        .unwrap_or("NormalTok")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_properties_use_rpr_order_and_rendered_colours() {
        let span = &crate::notebook::ansi::parse("\x1b[1;2;3;4;7;8;9;31mX")[0];
        assert_eq!(
            ansi_run_props(&span.style, "VerbatimChar", None).to_xml(),
            concat!(
                "<w:rPr><w:rStyle w:val=\"VerbatimChar\"/><w:b/><w:bCs/>",
                "<w:i/><w:iCs/><w:strike/><w:color w:val=\"FFFFFF\"/>",
                "<w:u w:val=\"single\"/><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"B22B31\"/></w:rPr>",
            )
        );
        let span = &crate::notebook::ansi::parse("\x1b[7mX")[0];
        let props = ansi_run_props(&span.style, "V", None);
        assert_eq!(props.color.as_deref(), Some("FFFFFF"));
        assert!(props.shading.unwrap().contains("w:fill=\"000000\""));
        let span = &crate::notebook::ansi::parse("\x1b[38;2;10;20;30mX")[0];
        let props = ansi_run_props(&span.style, "V", Some("<w:shd/>".into()));
        assert_eq!(props.color.as_deref(), Some("0A141E"));
        assert_eq!(props.shading.as_deref(), Some("<w:shd/>"));
    }

    #[test]
    fn highlighting_carries_multiline_scopes_and_keeps_exact_source() {
        let SyntaxChoice::Syntax(syntax) =
            resolve_syntax(Some("python"), false)
        else {
            panic!("embedded Python grammar is present");
        };
        let source =
            "def f():\n    \"\"\"Docs\n\n    continued\"\"\"\n    return 1";
        let lines = highlight(syntax, source).unwrap();
        assert_eq!(lines.len(), 5);
        assert!(lines[2].is_empty());
        assert!(lines[3].iter().any(|(style, text)| {
            *style == "DocumentationTok" && text.contains("continued")
        }));
        assert!(lines[4].iter().any(|(style, text)| {
            *style == "ControlFlowTok" && text == "return"
        }));
        assert_eq!(
            lines
                .iter()
                .map(|line| {
                    line.iter()
                        .map(|(_, text)| text.as_str())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n"),
            source
        );
    }

    #[test]
    fn scope_mapping_prefers_inner_scope_then_specific_prefix() {
        let mut stack = ScopeStack::from_vec(vec![
            Scope::new("source.python").unwrap(),
            Scope::new("comment.block.documentation.python").unwrap(),
        ]);
        assert_eq!(token_style(&stack), "DocumentationTok");
        stack.push(Scope::new("constant.character.escape.python").unwrap());
        assert_eq!(token_style(&stack), "SpecialCharTok");
        assert_eq!(token_style(&ScopeStack::new()), "NormalTok");
    }

    #[test]
    fn table_boundary_ignores_bookmarks_comments_and_processing_instructions() {
        for suffix in [
            "",
            " ",
            "<w:bookmarkEnd w:id=\"1\"/>",
            "<w:bookmarkStart\nw:id=\"2\" w:name=\"x\"/>",
            "<!-- kept --><w:bookmarkEnd w:id=\"1\"/><?kept data?>",
        ] {
            assert!(last_element_is_table(&format!(
                "<w:tbl><w:tr/></w:tbl>{suffix}"
            )));
        }
        assert!(last_element_is_table("<w:tbl\n/>"));
        for xml in [
            "",
            "<w:p/>",
            "<w:tbl><w:tr/></w:tbl><w:p/>",
            "<w:tbl><w:tr/></w:tbl><w:tblPr/>",
        ] {
            assert!(!last_element_is_table(xml));
        }
    }
}
