//! Selected output branches for check-only target facts.
//!
//! This borrows the original or cached nodes actually interpreted by a target.
//! Alt text, DOCX TOCs and bare LaTeX URLs are flattened, so their structured
//! children cannot request equation rendering or acquire raw-loss findings.

use std::collections::BTreeSet;

use crate::config::{Config, LatexCite, Target};
use crate::ir::{Block, CiteMode, Document, FigureKind, Inline, RefForm};

use super::MathLocation;

#[derive(Clone, Copy)]
pub(super) enum Node<'a> {
    Block(&'a Block),
    Inline(&'a Inline),
}

pub(super) fn walk<'a>(
    document: &'a Document, target: Target, config: &Config,
    mut visit: impl FnMut(Node<'a>, &MathLocation),
) {
    let root = MathLocation { path: document.source.path.clone(), span: None };
    let mut walker = Walker {
        document,
        target,
        config,
        visit: &mut visit,
        html_notes: BTreeSet::new(),
        docx_bookmarks: if target == Target::Docx {
            crate::docx::visible_bookmarks(document)
        } else {
            BTreeSet::new()
        },
    };
    if let Some(blocks) = &document.meta.abstract_ {
        walker.blocks(blocks, &root, false);
    }
    walker.blocks(&document.blocks, &root, false);
}

struct Walker<'a, 'c, 'v> {
    document: &'a Document,
    target: Target,
    config: &'c Config,
    visit: &'v mut dyn FnMut(Node<'a>, &MathLocation),
    html_notes: BTreeSet<u32>,
    docx_bookmarks: BTreeSet<String>,
}

impl<'a> Walker<'a, '_, '_> {
    fn blocks(
        &mut self, blocks: &'a [Block], inherited: &MathLocation,
        latex_group: bool,
    ) {
        for block in blocks {
            let location = inherited.nested(block.span());
            (self.visit)(Node::Block(block), &location);
            match block {
                Block::Heading { inlines, .. }
                | Block::Paragraph { inlines, .. } => {
                    self.inlines(inlines, &location, latex_group);
                }
                Block::BlockQuote { blocks, .. }
                | Block::Div { blocks, .. } => {
                    self.blocks(blocks, &location, latex_group);
                }
                Block::List { items, .. } => {
                    for item in items {
                        self.blocks(&item.blocks, &location, latex_group);
                    }
                }
                Block::DefinitionList { items, .. } => {
                    for item in items {
                        self.inlines(&item.term, &location, latex_group);
                        for blocks in &item.definitions {
                            self.blocks(blocks, &location, latex_group);
                        }
                    }
                }
                Block::Figure { kind, caption, body, .. } => {
                    self.figure(
                        *kind,
                        caption.as_deref(),
                        body,
                        &location,
                        latex_group,
                    );
                }
                Block::Callout { title, blocks, .. } => {
                    if let Some(title) = title {
                        self.inlines(title, &location, latex_group);
                    }
                    self.blocks(blocks, &location, latex_group);
                }
                Block::Table { head, body, foot, .. } => {
                    let cells = match self.target {
                        Target::Docx => {
                            crate::docx::tables::rendered_cells(block)
                        }
                        Target::Latex => {
                            crate::latex::emit::rendered_table_cells(block)
                        }
                        _ => head
                            .iter()
                            .chain(body)
                            .chain(foot)
                            .flat_map(|row| &row.cells)
                            .collect(),
                    };
                    for cell in cells {
                        self.inlines(&cell.inlines, &location, latex_group);
                    }
                }
                Block::Cell(cell) if cell.show_output => {
                    let target = match self.target {
                        Target::Html => "html",
                        Target::Docx => "docx",
                        Target::Latex => "latex",
                        Target::Pdf | Target::Typst => "typst",
                    };
                    for output in &cell.outputs {
                        self.blocks(
                            output.pick(target),
                            &location,
                            latex_group,
                        );
                    }
                }
                Block::Bibliography { .. }
                    if matches!(self.target, Target::Html | Target::Docx) =>
                {
                    if let Some(bibliography) =
                        &self.document.resolved.bibliography
                    {
                        for entry in &bibliography.entries {
                            if let Some(prefix) = &entry.prefix {
                                self.inlines(prefix, &location, latex_group);
                            }
                            self.inlines(
                                &entry.inlines, &location, latex_group,
                            );
                        }
                    }
                }
                Block::Toc { depth, .. } if self.target == Target::Html => {
                    for entry in &self.document.resolved.toc {
                        if entry.level > *depth {
                            continue;
                        }
                        let title_override = entry.anchor == "refs"
                            && self
                                .document
                                .meta
                                .vars
                                .get("reference-section-title")
                                .is_some_and(serde_json::Value::is_string);
                        if !title_override {
                            self.inlines(
                                &entry.inlines, &location, latex_group,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn figure(
        &mut self, kind: FigureKind, caption: Option<&'a [Inline]>,
        body: &'a [Block], location: &MathLocation, latex_group: bool,
    ) {
        let caption_first = match self.target {
            Target::Html | Target::Docx => {
                matches!(kind, FigureKind::Table | FigureKind::Listing)
            }
            Target::Latex if !latex_group => matches!(
                (kind, body),
                (FigureKind::Table, [Block::Table { .. }])
                    | (FigureKind::Listing, [Block::CodeBlock { .. }]),
            ),
            _ => false,
        };
        if caption_first && let Some(caption) = caption {
            self.inlines(caption, location, latex_group);
        }
        // The native emitter's specialized image figure accepts only its
        // sole image. A malformed sole paragraph is not a generic body branch.
        let dropped = matches!(self.target, Target::Pdf | Target::Typst)
            && kind == FigureKind::Image
            && matches!(body, [Block::Paragraph { inlines, .. }]
                if !matches!(inlines.as_slice(), [Inline::Image { .. }]));
        if !dropped {
            let group = self.target == Target::Latex
                && (latex_group || kind == FigureKind::Other);
            self.blocks(body, location, group);
        }
        if !caption_first && let Some(caption) = caption {
            self.inlines(caption, location, latex_group);
        }
    }

    fn inlines(
        &mut self, inlines: &'a [Inline], inherited: &MathLocation,
        latex_group: bool,
    ) {
        for inline in inlines {
            let span = match inline {
                Inline::Ref { span, .. } | Inline::Cite { span, .. } => {
                    span.as_ref()
                }
                _ => None,
            };
            let location = inherited.nested(span);
            (self.visit)(Node::Inline(inline), &location);
            match inline {
                Inline::Emph { inlines }
                | Inline::Strong { inlines }
                | Inline::Strikeout { inlines }
                | Inline::Subscript { inlines }
                | Inline::Superscript { inlines }
                | Inline::Underline { inlines }
                | Inline::SmallCaps { inlines }
                | Inline::Span { inlines, .. } => {
                    self.inlines(inlines, &location, latex_group);
                }
                Inline::Link { inlines, url, .. } => {
                    let bare = if self.target == Target::Latex {
                        let plain = crate::ir::plain_text(inlines);
                        !url.starts_with('#')
                            && (plain == *url
                                || url.strip_prefix("mailto:")
                                    == Some(plain.as_str()))
                    } else {
                        matches!(self.target, Target::Pdf | Target::Typst)
                            && !url.starts_with('#')
                            && matches!(inlines.as_slice(), [Inline::Text { text }] if text == url)
                    };
                    if !bare {
                        self.inlines(inlines, &location, latex_group);
                    }
                }
                Inline::Footnote { blocks, number, .. } => {
                    let show = self.target != Target::Html
                        || number.is_some_and(|number| {
                            self.html_notes.insert(number)
                        });
                    if show {
                        self.blocks(blocks, &location, latex_group);
                    }
                }
                Inline::Ref { target, supplement, form, resolved, .. } => {
                    let info = self.document.resolved.labels.get(target);
                    let numbered =
                        info.is_none_or(|info| info.number.is_some());
                    let cached = resolved
                        .as_ref()
                        .map(|reference| reference.text.as_slice());
                    let docx_page = *form == RefForm::Page
                        && resolved.as_ref().is_some_and(|reference| {
                            self.docx_bookmarks.contains(
                                &crate::resolve::anchor(&reference.anchor),
                            )
                        });
                    let children = match self.target {
                        Target::Html => cached,
                        Target::Docx if docx_page => supplement.as_deref(),
                        Target::Docx => cached,
                        Target::Latex
                            if *form != RefForm::Page && !numbered =>
                        {
                            cached
                        }
                        Target::Latex if resolved.is_some() => {
                            supplement.as_deref()
                        }
                        Target::Latex => None,
                        Target::Pdf | Target::Typst => {
                            if info.is_none() {
                                None
                            } else if *form != RefForm::Page && !numbered {
                                cached
                            } else {
                                supplement.as_deref()
                            }
                        }
                    };
                    if let Some(children) = children {
                        self.inlines(children, &location, latex_group);
                    }
                }
                Inline::Cite { items, mode, resolved, .. } => {
                    if matches!(self.target, Target::Html | Target::Docx) {
                        if let Some(resolved) = resolved {
                            self.inlines(
                                &resolved.inlines, &location, latex_group,
                            );
                        }
                        continue;
                    }
                    if self.target == Target::Latex {
                        if *mode == CiteMode::None
                            || self.config.latex.cite == LatexCite::Natbib
                                && *mode == CiteMode::Author
                        {
                            continue;
                        }
                    } else if resolved.is_none() {
                        continue;
                    }
                    for item in items {
                        if let Some(prefix) = &item.prefix {
                            self.inlines(prefix, &location, latex_group);
                        }
                        if let Some(suffix) = &item.suffix {
                            self.inlines(suffix, &location, latex_group);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Align, Attrs, BibEntry, Bibliography, Cell, CellOutput, CiteItem,
        ColumnSpec, DocMeta, Features, LabelInfo, LabelKind, MathDialect,
        OutputCandidate, Resolved, ResolvedCite, ResolvedRef, Row, SourceInfo,
        SourceKind, SourceSpan, TableCell, TocEntry,
    };

    const TARGETS: [Target; 5] =
        [Target::Html, Target::Docx, Target::Pdf, Target::Typst, Target::Latex];

    fn document(blocks: Vec<Block>) -> Document {
        Document {
            meta: DocMeta::default(),
            blocks,
            features: Features::default(),
            source: SourceInfo {
                path: "/book/main.md".into(),
                base_dir: "/book".into(),
                kind: SourceKind::Markdown,
            },
            assets: Vec::new(),
            includes: Vec::new(),
            resolved: Resolved::default(),
        }
    }

    fn raw(marker: &str) -> Inline {
        Inline::Raw { format: "foreign".into(), text: marker.into() }
    }

    fn math(marker: &str) -> Inline {
        Inline::Math { source: marker.into(), dialect: MathDialect::Typst }
    }

    fn markers(
        document: &Document, target: Target, config: &Config,
    ) -> Vec<String> {
        let mut found = Vec::new();
        walk(document, target, config, |node, _| match node {
            Node::Block(Block::Raw { text, .. })
            | Node::Inline(Inline::Raw { text, .. }) => {
                found.push(text.clone())
            }
            Node::Inline(Inline::Math { source, .. }) => {
                found.push(source.clone())
            }
            _ => {}
        });
        found
    }

    fn heading(id: &str) -> Block {
        Block::Heading {
            level: 1,
            inlines: Vec::new(),
            attrs: Attrs::with_id(id),
            number: Some("1".into()),
            span: None,
        }
    }

    fn label(document: &mut Document, key: &str, numbered: bool) {
        document.resolved.labels.insert(
            key.into(),
            LabelInfo {
                kind: LabelKind::Heading,
                number: numbered.then(|| "1".into()),
                supplement: "Section".into(),
                text: Some(key.into()),
                span: None,
            },
        );
    }

    fn reference(target: &str, form: RefForm, marker: &str) -> Inline {
        Inline::Ref {
            target: target.into(),
            form,
            supplement: Some(vec![raw(&format!("source-{marker}"))]),
            resolved: Some(ResolvedRef {
                kind: LabelKind::Heading,
                number: "1".into(),
                anchor: target.into(),
                text: vec![raw(&format!("cache-{marker}"))],
            }),
            span: None,
        }
    }

    fn citation(mode: CiteMode, marker: &str, resolved: bool) -> Inline {
        Inline::Cite {
            mode,
            items: vec![CiteItem {
                key: marker.into(),
                prefix: Some(vec![raw(&format!("pre-{marker}"))]),
                suffix: Some(vec![raw(&format!("post-{marker}"))]),
                ..CiteItem::default()
            }],
            resolved: resolved.then(|| ResolvedCite {
                inlines: vec![raw(&format!("cache-{marker}"))],
                note: false,
            }),
            span: None,
        }
    }

    fn table(
        columns: usize, head: Vec<TableCell>, body: Vec<TableCell>,
    ) -> Block {
        Block::Table {
            columns: vec![
                ColumnSpec { align: Align::Default, width: None };
                columns
            ],
            head: vec![Row { cells: head }],
            body: vec![Row { cells: body }],
            foot: vec![Row {
                cells: vec![TableCell {
                    inlines: vec![raw("FOOT")],
                    ..TableCell::default()
                }],
            }],
            attrs: Attrs::default(),
            span: None,
        }
    }

    #[test]
    fn selected_candidates_and_abstract_keep_preorder_and_do_not_mutate_source()
    {
        let output = CellOutput {
            candidates: ["html", "openxml", "typst", "latex"]
                .map(|format| OutputCandidate {
                    mime: format!("text/{format}"),
                    blocks: vec![Block::Raw {
                        format: format.into(),
                        text: format.into(),
                        span: None,
                    }],
                })
                .into(),
            ..CellOutput::default()
        };
        let visible = Cell {
            show_output: true,
            outputs: vec![output],
            ..Cell::default()
        };
        let hidden = Cell { show_output: false, ..visible.clone() };
        let mut document =
            document(vec![Block::Cell(visible), Block::Cell(hidden)]);
        document.meta.abstract_ =
            Some(vec![Block::paragraph(vec![raw("ABSTRACT")])]);
        let before = document.clone();
        for (target, expected) in [
            (Target::Html, "html"),
            (Target::Docx, "openxml"),
            (Target::Pdf, "typst"),
            (Target::Typst, "typst"),
            (Target::Latex, "latex"),
        ] {
            assert_eq!(
                markers(&document, target, &Config::default()),
                ["ABSTRACT", expected]
            );
        }
        assert_eq!(document, before);
    }

    #[test]
    fn inherited_file_cell_and_ref_span_follow_the_chosen_payload() {
        let mut reference = reference("target", RefForm::Normal, "ref");
        let Inline::Ref { span, .. } = &mut reference else {
            unreachable!()
        };
        *span = Some(SourceSpan {
            start_line: 9,
            end_line: 9,
            cell: None,
            file: None,
        });
        let document = document(vec![Block::Div {
            attrs: Attrs::default(),
            span: Some(SourceSpan {
                start_line: 2,
                end_line: 12,
                cell: Some(4),
                file: Some("/book/child.ipynb".into()),
            }),
            blocks: vec![Block::paragraph(vec![reference])],
        }]);
        let mut found = Vec::new();
        walk(&document, Target::Html, &Config::default(), |node, location| {
            if let Node::Inline(Inline::Raw { text, .. }) = node {
                found.push((
                    text.clone(),
                    location.path.clone(),
                    location.span.clone(),
                ));
            }
        });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "cache-ref");
        assert_eq!(found[0].1, std::path::Path::new("/book/child.ipynb"));
        let span = found[0].2.as_ref().unwrap();
        assert_eq!(
            (span.start_line, span.end_line, span.cell),
            (9, 9, Some(4))
        );
        assert_eq!(
            span.file.as_deref(),
            Some(std::path::Path::new("/book/child.ipynb"))
        );
    }

    #[test]
    fn citation_caches_and_source_affixes_are_mutually_exclusive() {
        let document = document(vec![Block::paragraph(vec![
            citation(CiteMode::Normal, "normal", true),
            citation(CiteMode::None, "hidden", true),
            citation(CiteMode::Author, "author", true),
            citation(CiteMode::Normal, "missing", false),
        ])]);
        let mut config = Config::default();
        for target in [Target::Html, Target::Docx] {
            assert_eq!(
                markers(&document, target, &config),
                ["cache-normal", "cache-hidden", "cache-author",]
            );
        }
        for target in [Target::Pdf, Target::Typst] {
            assert_eq!(
                markers(&document, target, &config),
                [
                    "pre-normal", "post-normal", "pre-hidden", "post-hidden",
                    "pre-author", "post-author",
                ]
            );
        }
        assert_eq!(
            markers(&document, Target::Latex, &config),
            [
                "pre-normal", "post-normal", "pre-author", "post-author",
                "pre-missing", "post-missing",
            ]
        );
        config.latex.cite = LatexCite::Natbib;
        assert_eq!(
            markers(&document, Target::Latex, &config),
            ["pre-normal", "post-normal", "pre-missing", "post-missing",]
        );
    }

    #[test]
    fn references_use_cached_text_or_the_exact_page_number_supplement_branch() {
        let mut document = document(vec![
            heading("numbered"),
            heading("unnumbered"),
            Block::paragraph(vec![
                reference("numbered", RefForm::Normal, "numbered"),
                reference("unnumbered", RefForm::Normal, "unnumbered"),
                reference("numbered", RefForm::Page, "page"),
                reference("not-emitted", RefForm::Page, "absent"),
            ]),
        ]);
        label(&mut document, "numbered", true);
        label(&mut document, "unnumbered", false);
        let config = Config::default();
        assert_eq!(
            markers(&document, Target::Html, &config),
            [
                "cache-numbered", "cache-unnumbered", "cache-page",
                "cache-absent",
            ]
        );
        assert_eq!(
            markers(&document, Target::Docx, &config),
            [
                "cache-numbered", "cache-unnumbered", "source-page",
                "cache-absent",
            ]
        );
        assert_eq!(
            markers(&document, Target::Latex, &config),
            [
                "source-numbered", "cache-unnumbered", "source-page",
                "source-absent",
            ]
        );
        for target in [Target::Pdf, Target::Typst] {
            assert_eq!(
                markers(&document, target, &config),
                ["source-numbered", "cache-unnumbered", "source-page",]
            );
        }
    }

    #[test]
    fn typst_known_labels_can_emit_source_supplements_without_a_cache() {
        let mut source = reference("known", RefForm::Normal, "known");
        let Inline::Ref { resolved, .. } = &mut source else {
            unreachable!()
        };
        *resolved = None;
        let mut document = document(vec![Block::paragraph(vec![source])]);
        label(&mut document, "known", true);
        for target in [Target::Pdf, Target::Typst] {
            assert_eq!(
                markers(&document, target, &Config::default()),
                ["source-known"]
            );
        }
        for target in [Target::Html, Target::Docx, Target::Latex] {
            assert!(markers(&document, target, &Config::default()).is_empty());
        }
    }

    #[test]
    fn alt_children_are_flattened_and_bare_latex_links_do_not_render_math() {
        let link = Inline::Link {
            url: "https://example.org/x".into(),
            title: None,
            attrs: Attrs::default(),
            inlines: vec![
                Inline::text("https://example.org/"),
                math("x"),
                raw("URL-RAW"),
            ],
        };
        let image = Inline::Image {
            url: "https://example.org/image.png".into(),
            title: None,
            attrs: Attrs::default(),
            alt: vec![raw("ALT-RAW"), math("ALT-MATH")],
        };
        let document = document(vec![Block::paragraph(vec![link, image])]);
        for target in [Target::Html, Target::Docx, Target::Pdf, Target::Typst] {
            assert_eq!(
                markers(&document, target, &Config::default()),
                ["x", "URL-RAW"]
            );
        }
        assert!(
            markers(&document, Target::Latex, &Config::default()).is_empty()
        );
    }

    #[test]
    fn exact_grid_plans_drop_only_unrendered_cells_and_keep_section_boundaries()
    {
        let table = table(
            1,
            vec![TableCell {
                inlines: vec![raw("HEAD")],
                rowspan: 3,
                ..TableCell::default()
            }],
            vec![
                TableCell {
                    inlines: vec![raw("DROP")],
                    colspan: u32::MAX,
                    ..TableCell::default()
                },
                TableCell {
                    inlines: vec![raw("KEEP")],
                    ..TableCell::default()
                },
            ],
        );
        let document = document(vec![table]);
        for target in [Target::Docx, Target::Latex] {
            assert_eq!(
                markers(&document, target, &Config::default()),
                ["HEAD", "KEEP", "FOOT"]
            );
        }
        for target in [Target::Html, Target::Pdf, Target::Typst] {
            assert_eq!(
                markers(&document, target, &Config::default()),
                ["HEAD", "DROP", "KEEP", "FOOT"],
            );
        }
    }

    #[test]
    fn docx_page_refs_cannot_use_a_bookmark_only_present_in_a_dropped_cell() {
        let dropped = Inline::Span {
            attrs: Attrs::with_id("ghost"),
            inlines: vec![raw("DROP")],
        };
        let grid = table(
            1,
            Vec::new(),
            vec![
                TableCell {
                    inlines: vec![dropped],
                    colspan: u32::MAX,
                    ..TableCell::default()
                },
                TableCell {
                    inlines: vec![raw("KEEP")],
                    ..TableCell::default()
                },
            ],
        );
        let mut document = document(vec![
            grid,
            Block::paragraph(vec![reference("ghost", RefForm::Page, "ghost")]),
        ]);
        label(&mut document, "ghost", true);
        assert_eq!(
            markers(&document, Target::Docx, &Config::default()),
            ["KEEP", "FOOT", "cache-ghost",]
        );
    }

    #[test]
    fn zero_column_latex_fallback_keeps_source_cells_while_docx_drops_them() {
        let document = document(vec![table(
            0,
            Vec::new(),
            vec![TableCell {
                inlines: vec![raw("BODY")],
                ..TableCell::default()
            }],
        )]);
        assert!(
            markers(&document, Target::Docx, &Config::default()).is_empty()
        );
        for target in [Target::Html, Target::Pdf, Target::Typst, Target::Latex]
        {
            assert_eq!(
                markers(&document, target, &Config::default()),
                ["BODY", "FOOT"],
            );
        }
    }

    #[test]
    fn figure_caption_order_follows_the_target_and_latex_subfigure_context() {
        let inner = Block::Figure {
            kind: FigureKind::Table,
            caption: Some(vec![raw("CAPTION")]),
            body: vec![table(
                1,
                Vec::new(),
                vec![TableCell {
                    inlines: vec![raw("BODY")],
                    ..TableCell::default()
                }],
            )],
            attrs: Attrs::default(),
            number: None,
            span: None,
        };
        let standalone = document(vec![inner.clone()]);
        for target in [Target::Html, Target::Docx, Target::Latex] {
            assert_eq!(
                markers(&standalone, target, &Config::default()),
                ["CAPTION", "BODY", "FOOT"],
            );
        }
        for target in [Target::Pdf, Target::Typst] {
            assert_eq!(
                markers(&standalone, target, &Config::default()),
                ["BODY", "FOOT", "CAPTION"],
            );
        }
        let nested = document(vec![Block::Figure {
            kind: FigureKind::Other,
            caption: Some(vec![raw("OUTER")]),
            body: vec![inner],
            attrs: Attrs::default(),
            number: None,
            span: None,
        }]);
        assert_eq!(
            markers(&nested, Target::Latex, &Config::default()),
            ["BODY", "FOOT", "CAPTION", "OUTER"],
        );
    }

    #[test]
    fn html_note_bodies_are_numbered_and_unique_while_docx_keeps_all_bodies() {
        let note = |marker: &str, number| Inline::Footnote {
            blocks: vec![Block::paragraph(vec![raw(marker)])],
            label: None,
            number,
        };
        let document = document(vec![Block::paragraph(vec![
            note("FIRST", Some(1)),
            note("REPEATED", Some(1)),
            note("UNNUMBERED", None),
        ])]);
        assert_eq!(
            markers(&document, Target::Html, &Config::default()),
            ["FIRST"]
        );
        for target in [Target::Docx, Target::Pdf, Target::Typst, Target::Latex]
        {
            assert_eq!(
                markers(&document, target, &Config::default()),
                ["FIRST", "REPEATED", "UNNUMBERED"],
            );
        }
    }

    #[test]
    fn only_explicit_html_toc_and_rich_bibliographies_interpret_generated_inlines()
     {
        let mut document = document(vec![Block::Bibliography { span: None }]);
        document.resolved.toc = vec![TocEntry {
            level: 1,
            number: None,
            inlines: vec![raw("TOC")],
            anchor: "heading".into(),
        }];
        document.resolved.bibliography = Some(Bibliography {
            style: "ieee".into(),
            hanging_indent: false,
            entries: vec![BibEntry {
                key: "book".into(),
                prefix: Some(vec![raw("BIB-PREFIX")]),
                inlines: vec![raw("BIB-BODY")],
            }],
        });
        assert_eq!(
            markers(&document, Target::Html, &Config::default()),
            ["BIB-PREFIX", "BIB-BODY"],
        );
        document.blocks.insert(0, Block::Toc { depth: 1, span: None });
        assert_eq!(
            markers(&document, Target::Html, &Config::default()),
            ["TOC", "BIB-PREFIX", "BIB-BODY"],
        );
        assert_eq!(
            markers(&document, Target::Docx, &Config::default()),
            ["BIB-PREFIX", "BIB-BODY"],
        );
        for target in [Target::Pdf, Target::Typst, Target::Latex] {
            assert!(markers(&document, target, &Config::default()).is_empty());
        }
        document.resolved.toc[0].anchor = "refs".into();
        document.meta.vars.insert(
            "reference-section-title".into(),
            "Plain custom title".into(),
        );
        assert_eq!(
            markers(&document, Target::Html, &Config::default()),
            ["BIB-PREFIX", "BIB-BODY"],
        );
    }

    #[test]
    fn equal_equations_remain_distinct_nodes_and_parents_are_visited_first() {
        let document =
            document(vec![Block::paragraph(vec![math("x"), math("x")])]);
        for target in TARGETS {
            let mut order = Vec::new();
            let mut addresses = Vec::new();
            walk(&document, target, &Config::default(), |node, _| match node {
                Node::Block(Block::Paragraph { .. }) => order.push("paragraph"),
                Node::Inline(inline @ Inline::Math { .. }) => {
                    order.push("math");
                    addresses.push(std::ptr::from_ref(inline));
                }
                _ => {}
            });
            assert_eq!(order, ["paragraph", "math", "math"]);
            assert_ne!(addresses[0], addresses[1]);
        }
    }
}
