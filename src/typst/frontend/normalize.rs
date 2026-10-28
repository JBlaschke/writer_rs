//! Final syntax-frontend normalization after figure inference and include expansion.

use std::collections::BTreeSet;

use crate::ir::{self, Attrs, Block, Document, FigureKind, Inline};

/// Keeps labelled tables invariant-safe and resolves forward footnote references.
/// Existing note labels have already had one writer prefix removed by the walker.
pub(super) fn finish_document(document: &mut Document) {
    let mut notes = BTreeSet::new();
    ir::walk_inlines(document, &mut |inline| {
        if let Inline::Footnote { label: Some(label), .. } = inline {
            notes.insert(label.clone());
        }
    });
    if let Some(abstract_) = &mut document.meta.abstract_ {
        blocks(abstract_, false, &notes);
    }
    blocks(&mut document.blocks, false, &notes);
}

/// Returns whether this sequence contains a figure, including inside a note.
fn blocks(
    blocks: &mut [Block], parent_table: bool, notes: &BTreeSet<String>,
) -> bool {
    let mut figure = false;
    for block in blocks {
        let nested = block_children(block, notes);
        if matches!(block, Block::Table { attrs, .. } if attrs.id.is_some()) {
            if nested {
                wrap_nested_table(block);
            } else if !parent_table {
                let span = block.span().cloned();
                let table = std::mem::replace(
                    block,
                    Block::Paragraph { inlines: Vec::new(), span: None },
                );
                *block = Block::Figure {
                    kind: FigureKind::Table,
                    caption: None,
                    body: vec![table],
                    attrs: Attrs {
                        classes: vec!["unnumbered".into()],
                        ..Default::default()
                    },
                    number: None,
                    span,
                };
            }
        }
        figure |= nested || matches!(block, Block::Figure { .. });
    }
    figure
}

/// A nested figure cannot live inside a Table figure. Keep the table's Other
/// label on a Div instead, preserving the grid, other table attrs and its span.
fn wrap_nested_table(block: &mut Block) {
    let Block::Table { attrs, .. } = block else {
        return;
    };
    let id = attrs.id.take();
    let span = block.span().cloned();
    let table = std::mem::replace(
        block,
        Block::Paragraph { inlines: Vec::new(), span: None },
    );
    *block = Block::Div {
        attrs: Attrs { id, ..Default::default() },
        blocks: vec![table],
        span,
    };
}

fn block_children(block: &mut Block, notes: &BTreeSet<String>) -> bool {
    match block {
        Block::Heading { inlines: content, .. }
        | Block::Paragraph { inlines: content, .. } => inlines(content, notes),
        Block::BlockQuote { blocks: children, .. }
        | Block::Div { blocks: children, .. } => blocks(children, false, notes),
        Block::Callout { title, blocks: children, .. } => {
            let title =
                title.as_mut().is_some_and(|title| inlines(title, notes));
            blocks(children, false, notes) || title
        }
        Block::Figure { kind, caption, body, .. } => {
            let caption =
                caption.as_mut().is_some_and(|caption| inlines(caption, notes));
            let nested =
                blocks(body, *kind == FigureKind::Table, notes) || caption;
            if nested && *kind != FigureKind::Other {
                *kind = FigureKind::Other;
                // A formerly direct labelled table now needs its own wrapper.
                // Tables that contain nested figures already use a Div label.
                blocks(body, false, notes);
            }
            true
        }
        Block::List { items, .. } => {
            let mut figure = false;
            for item in items {
                figure |= blocks(&mut item.blocks, false, notes);
            }
            figure
        }
        Block::DefinitionList { items, .. } => {
            let mut figure = false;
            for item in items {
                figure |= inlines(&mut item.term, notes);
                for definition in &mut item.definitions {
                    figure |= blocks(definition, false, notes);
                }
            }
            figure
        }
        Block::Table { head, body, foot, .. } => {
            let mut figure = false;
            for row in head.iter_mut().chain(body).chain(foot) {
                for cell in &mut row.cells {
                    figure |= inlines(&mut cell.inlines, notes);
                }
            }
            figure
        }
        Block::Cell(cell) => {
            let mut figure = false;
            for output in &mut cell.outputs {
                for candidate in &mut output.candidates {
                    figure |= blocks(&mut candidate.blocks, false, notes);
                }
            }
            figure
        }
        Block::CodeBlock { .. }
        | Block::Math { .. }
        | Block::Raw { .. }
        | Block::ThematicBreak { .. }
        | Block::PageBreak { .. }
        | Block::Toc { .. }
        | Block::Bibliography { .. } => false,
    }
}

fn inlines(content: &mut [Inline], notes: &BTreeSet<String>) -> bool {
    let mut figure = false;
    for inline in content {
        if let Inline::Ref { target, .. } = inline {
            let label = target.strip_prefix("writer-fn-").unwrap_or(target);
            if notes.contains(label) {
                *inline = Inline::FootnoteRef {
                    label: label.to_string(),
                    number: None,
                };
                continue;
            }
        }
        figure |= match inline {
            Inline::Emph { inlines: content }
            | Inline::Strong { inlines: content }
            | Inline::Strikeout { inlines: content }
            | Inline::Subscript { inlines: content }
            | Inline::Superscript { inlines: content }
            | Inline::Underline { inlines: content }
            | Inline::SmallCaps { inlines: content }
            | Inline::Link { inlines: content, .. }
            | Inline::Span { inlines: content, .. } => inlines(content, notes),
            Inline::Image { alt, .. } => inlines(alt, notes),
            Inline::Footnote { blocks: content, .. } => {
                blocks(content, false, notes)
            }
            Inline::Ref { supplement: Some(content), .. } => {
                inlines(content, notes)
            }
            Inline::Cite { items, .. } => {
                let mut figure = false;
                for item in items {
                    if let Some(prefix) = &mut item.prefix {
                        figure |= inlines(prefix, notes);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        figure |= inlines(suffix, notes);
                    }
                }
                figure
            }
            _ => false,
        };
    }
    figure
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Cell, CellOutput, ColumnSpec, DefinitionItem, ListItem, ListKind,
        OutputCandidate, RefForm, Row, SourceInfo, SourceKind, SourceSpan,
        TableCell,
    };

    fn span() -> Option<SourceSpan> {
        Some(SourceSpan {
            start_line: 3,
            end_line: 4,
            cell: None,
            file: Some("included.typ".into()),
        })
    }

    fn text(text: &str) -> Inline {
        Inline::Text { text: text.into() }
    }

    fn paragraph(inlines: Vec<Inline>) -> Block {
        Block::Paragraph { inlines, span: span() }
    }

    fn table(id: Option<&str>) -> Block {
        Block::Table {
            columns: vec![ColumnSpec::default()],
            head: Vec::new(),
            foot: Vec::new(),
            body: vec![Row {
                cells: vec![TableCell {
                    inlines: vec![text("cell")],
                    ..Default::default()
                }],
            }],
            attrs: Attrs { id: id.map(str::to_string), ..Default::default() },
            span: span(),
        }
    }

    fn figure(kind: FigureKind, body: Vec<Block>) -> Block {
        Block::Figure {
            kind,
            caption: None,
            body,
            attrs: Attrs::default(),
            number: None,
            span: span(),
        }
    }

    fn note(label: &str, blocks: Vec<Block>) -> Inline {
        Inline::Footnote { label: Some(label.into()), blocks, number: None }
    }

    fn reference(target: &str) -> Inline {
        Inline::Ref {
            target: target.into(),
            supplement: None,
            form: RefForm::Normal,
            resolved: None,
            span: span(),
        }
    }

    fn document(blocks: Vec<Block>) -> Document {
        Document {
            meta: Default::default(),
            blocks,
            features: Default::default(),
            source: SourceInfo {
                path: "main.typ".into(),
                kind: SourceKind::Typst,
                base_dir: ".".into(),
            },
            assets: Vec::new(),
            includes: vec!["included.typ".into()],
            resolved: Default::default(),
        }
    }

    fn assert_valid(document: &Document) {
        assert!(
            document.invariant_violations().is_empty(),
            "{:?}",
            document.invariant_violations()
        );
    }

    #[test]
    fn bare_table_wrapper_keeps_the_table_label_span_and_other_attributes() {
        let mut original = table(Some("tbl-bare"));
        if let Block::Table { attrs, .. } = &mut original {
            attrs.classes.push("custom".into());
            attrs.pairs.insert("width".into(), "90%".into());
        }
        let mut doc = document(vec![original.clone(), table(None)]);
        finish_document(&mut doc);
        let Block::Figure {
            kind,
            caption,
            body,
            attrs,
            number,
            span: wrapper_span,
        } = &doc.blocks[0]
        else {
            panic!("table wrapper");
        };
        assert_eq!(*kind, FigureKind::Table);
        assert_eq!(*caption, None);
        assert_eq!(body, &[original]);
        assert_eq!(attrs.id, None);
        assert_eq!(attrs.classes, ["unnumbered"]);
        assert_eq!(*number, None);
        assert_eq!(*wrapper_span, span());
        assert!(matches!(doc.blocks[1], Block::Table { .. }));
        assert_valid(&doc);
        let once = doc.clone();
        finish_document(&mut doc);
        assert_eq!(doc, once, "normalization must be idempotent");
    }

    #[test]
    fn existing_table_figure_is_not_wrapped_and_other_figure_tables_are_wrapped()
     {
        let existing =
            figure(FigureKind::Table, vec![table(Some("inner-table"))]);
        let mut doc = document(vec![
            existing.clone(),
            figure(FigureKind::Other, vec![table(Some("bare-other"))]),
        ]);
        finish_document(&mut doc);
        assert_eq!(doc.blocks[0], existing);
        let Block::Figure { kind: FigureKind::Other, body, .. } =
            &doc.blocks[1]
        else {
            panic!("outer figure");
        };
        assert!(matches!(
            body.as_slice(),
            [Block::Figure { kind: FigureKind::Table, .. }]
        ));
        assert_valid(&doc);
    }

    #[test]
    fn normalization_reaches_abstract_lists_definitions_and_every_output_candidate()
     {
        let output = |id| OutputCandidate {
            mime: "text/x-typst".into(),
            blocks: vec![table(Some(id))],
        };
        let mut doc = document(vec![
            Block::List {
                kind: ListKind::Bullet,
                tight: false,
                items: vec![ListItem {
                    checked: None,
                    blocks: vec![table(Some("in-list"))],
                }],
                span: span(),
            },
            Block::DefinitionList {
                items: vec![DefinitionItem {
                    term: vec![note("term-note", vec![table(Some("in-term"))])],
                    definitions: vec![vec![table(Some("in-definition"))]],
                }],
                span: span(),
            },
            Block::Cell(Cell {
                outputs: vec![CellOutput {
                    candidates: vec![
                        output("first-output"),
                        output("second-output"),
                    ],
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ]);
        doc.meta.abstract_ = Some(vec![table(Some("in-abstract"))]);
        finish_document(&mut doc);
        let mut wrappers = 0;
        ir::walk_blocks(&doc, &mut |block| {
            if matches!(block, Block::Figure { kind: FigureKind::Table, .. }) {
                wrappers += 1;
            }
        });
        assert_eq!(wrappers, 6);
        assert_valid(&doc);
    }

    #[test]
    fn a_table_in_a_caption_note_downgrades_the_enclosing_non_other_figure() {
        let mut outer = figure(
            FigureKind::Listing,
            vec![Block::CodeBlock {
                lang: None,
                text: "x".into(),
                attrs: Attrs::default(),
                span: span(),
            }],
        );
        if let Block::Figure { caption, attrs, number, .. } = &mut outer {
            *caption = Some(vec![note(
                "caption-note",
                vec![table(Some("note-table"))],
            )]);
            attrs.id = Some("listing".into());
            *number = Some("2".into());
        }
        let mut doc = document(vec![outer]);
        finish_document(&mut doc);
        let Block::Figure { kind, attrs, number, span: outer_span, .. } =
            &doc.blocks[0]
        else {
            panic!("figure");
        };
        assert_eq!(*kind, FigureKind::Other);
        assert_eq!(attrs.id.as_deref(), Some("listing"));
        assert_eq!(number.as_deref(), Some("2"));
        assert_eq!(*outer_span, span());
        assert_valid(&doc);
    }

    #[test]
    fn downgrading_a_table_figure_caption_rewraps_its_former_direct_table() {
        let mut outer =
            figure(FigureKind::Table, vec![table(Some("direct-table"))]);
        if let Block::Figure { caption, .. } = &mut outer {
            *caption =
                Some(vec![note("caption", vec![table(Some("caption-table"))])]);
        }
        let mut doc = document(vec![outer]);
        finish_document(&mut doc);
        let Block::Figure { kind: FigureKind::Other, body, .. } =
            &doc.blocks[0]
        else {
            panic!("downgraded outer figure");
        };
        assert!(matches!(
            body.as_slice(),
            [Block::Figure { kind: FigureKind::Table, .. }]
        ));
        assert_valid(&doc);
        let once = doc.clone();
        finish_document(&mut doc);
        assert_eq!(doc, once);
    }

    #[test]
    fn forward_cross_include_notes_convert_refs_but_leave_ordinary_refs_untouched()
     {
        let ordinary = reference("sec-heading");
        let mut doc = document(vec![
            paragraph(vec![
                reference("writer-fn-note"),
                reference("note"),
                ordinary.clone(),
            ]),
            paragraph(vec![note(
                "note",
                vec![paragraph(vec![text("definition")])],
            )]),
        ]);
        finish_document(&mut doc);
        let Block::Paragraph { inlines, .. } = &doc.blocks[0] else {
            panic!("paragraph");
        };
        assert_eq!(
            inlines[0],
            Inline::FootnoteRef { label: "note".into(), number: None }
        );
        assert_eq!(
            inlines[1],
            Inline::FootnoteRef { label: "note".into(), number: None }
        );
        assert_eq!(inlines[2], ordinary);
        assert_valid(&doc);
    }

    #[test]
    fn note_labels_and_existing_note_refs_are_not_stripped_a_second_time() {
        let existing = Inline::FootnoteRef {
            label: "writer-fn-two".into(),
            number: Some(7),
        };
        let mut doc = document(vec![paragraph(vec![
            reference("writer-fn-writer-fn-two"),
            existing.clone(),
            note("writer-fn-two", vec![paragraph(vec![text("definition")])]),
        ])]);
        finish_document(&mut doc);
        let Block::Paragraph { inlines, .. } = &doc.blocks[0] else {
            panic!("paragraph");
        };
        assert_eq!(
            inlines[0],
            Inline::FootnoteRef { label: "writer-fn-two".into(), number: None }
        );
        assert_eq!(inlines[1], existing);
        assert!(
            matches!(&inlines[2], Inline::Footnote { label: Some(label), .. } if label == "writer-fn-two")
        );
    }
}

#[cfg(test)]
mod nested_table_tests {
    use super::*;
    use crate::ir::{ColumnSpec, Row, SourceInfo, SourceKind, TableCell};

    fn labelled_table(label: &str, inlines: Vec<Inline>) -> Block {
        Block::Table {
            columns: vec![ColumnSpec::default()],
            head: Vec::new(),
            foot: Vec::new(),
            body: vec![Row {
                cells: vec![TableCell { inlines, ..Default::default() }],
            }],
            attrs: Attrs {
                id: Some(label.into()),
                classes: vec!["keep".into()],
                ..Default::default()
            },
            span: None,
        }
    }

    #[test]
    fn nested_labelled_tables_use_a_div_label_without_rewrapping_or_counter_changes()
     {
        let inner =
            labelled_table("inner", vec![Inline::Text { text: "cell".into() }]);
        let outer = labelled_table(
            "outer",
            vec![Inline::Footnote {
                label: Some("note".into()),
                blocks: vec![inner],
                number: None,
            }],
        );
        let mut document = Document {
            meta: Default::default(),
            blocks: vec![outer],
            features: Default::default(),
            source: SourceInfo {
                path: "main.typ".into(),
                kind: SourceKind::Typst,
                base_dir: ".".into(),
            },
            assets: Vec::new(),
            includes: Vec::new(),
            resolved: Default::default(),
        };
        finish_document(&mut document);
        let Block::Div { attrs, blocks, .. } = &document.blocks[0] else {
            panic!("Other label container");
        };
        assert_eq!(attrs.id.as_deref(), Some("outer"));
        let Block::Table { attrs, .. } = &blocks[0] else {
            panic!("retained outer table");
        };
        assert_eq!(attrs.id, None);
        assert_eq!(attrs.classes, ["keep"]);
        assert!(
            document.invariant_violations().is_empty(),
            "{:?}",
            document.invariant_violations()
        );
        let once = document.clone();
        finish_document(&mut document);
        assert_eq!(document, once);
        let config = crate::config::Config::default();
        crate::resolve::resolve(
            &mut document,
            &crate::resolve::ResolveOptions {
                config: &config,
                library: None,
                lenient: true,
            },
        );
        assert_eq!(
            document.resolved.labels["outer"].kind,
            crate::ir::LabelKind::Other
        );
        assert_eq!(
            document.resolved.labels["inner"].kind,
            crate::ir::LabelKind::Other
        );
        assert_eq!(document.resolved.labels["outer"].number, None);
        assert_eq!(document.resolved.labels["inner"].number, None);
    }
}
