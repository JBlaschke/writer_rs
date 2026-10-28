//! DOCX footnote parts (docx-output.md §4.8).
//! Owner: T40.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{
    Block, Document, FigureKind, Inline, RefForm, SourceKind, SourceSpan,
};
use crate::resolve::{BIBLIOGRAPHY_ANCHOR, anchor};

use super::{Container, Emitter, Ids, Part, StyleKind, body::BodyState, xml};

pub(crate) struct ContentState<'a> {
    pub(crate) body: BodyState,
    pub(crate) span: Option<SourceSpan>,
    pub(crate) paragraph_prefix: String,
    plan: ContentPlan<'a>,
    claimed: BTreeSet<String>,
}

#[derive(Default)]
struct ContentPlan<'a> {
    // The original label's address identifies its first logical occurrence.
    // Notes render in another part later; emission order cannot choose owners.
    bookmarks: BTreeMap<String, usize>,
    notes: Vec<Note<'a>>,
    inline_notes: BTreeMap<usize, usize>,
    numbers: BTreeMap<u32, usize>,
    references: BTreeSet<u32>,
    targets: Option<BTreeSet<String>>,
}

#[derive(Clone)]
struct Note<'a> {
    id: u32,
    blocks: &'a [Block],
    span: Option<SourceSpan>,
    parent: Option<usize>,
    label: Option<&'a str>,
    bookmark: Option<String>,
}

impl<'a> ContentState<'a> {
    pub(crate) fn new(doc: &'a Document, ids: &mut Ids) -> Self {
        // Discover forward targets before choosing Page-reference content.
        // Resolved text may be a clone of the supplement; only one is emitted.
        let mut provisional_ids = *ids;
        let potential = ContentPlan::collect(doc, &mut provisional_ids, None);
        let targets = potential.bookmarks.into_keys().collect();
        let mut plan = ContentPlan::collect(doc, ids, Some(targets));
        let mut reserved: BTreeSet<_> =
            plan.bookmarks.keys().cloned().collect();
        for index in 0..plan.notes.len() {
            let targeted = plan
                .references
                .iter()
                .any(|number| plan.numbers.get(number) == Some(&index));
            let note = &mut plan.notes[index];
            note.bookmark = match note.label {
                Some(label) => Some(anchor(label)),
                None if targeted => {
                    let mut name = format!("_Fn{:06}", note.id);
                    while reserved.contains(&name) {
                        name.push('x');
                    }
                    reserved.insert(name.clone());
                    Some(name)
                }
                None => None,
            };
        }
        Self {
            body: BodyState::default(),
            span: None,
            paragraph_prefix: String::new(),
            plan,
            claimed: BTreeSet::new(),
        }
    }
}

impl<'a> ContentPlan<'a> {
    fn collect(
        doc: &'a Document, ids: &mut Ids, targets: Option<BTreeSet<String>>,
    ) -> Self {
        let mut plan = Self { targets, ..Default::default() };
        if let Some(blocks) = &doc.meta.abstract_ {
            plan.blocks(blocks, doc, None, None, ids);
        }
        plan.blocks(&doc.blocks, doc, None, None, ids);
        plan
    }

    fn label(&mut self, label: Option<&'a str>) {
        if let Some(label) = label {
            self.bookmarks
                .entry(anchor(label))
                .or_insert(label.as_ptr() as usize);
        }
    }

    fn blocks(
        &mut self, blocks: &'a [Block], doc: &'a Document,
        inherited: Option<&SourceSpan>, parent: Option<usize>, ids: &mut Ids,
    ) {
        for block in blocks {
            let span = block.span().or(inherited);
            match block {
                Block::Heading { inlines, attrs, .. } => {
                    self.label(attrs.id.as_deref());
                    self.inlines(inlines, doc, span, parent, ids);
                }
                Block::Paragraph { inlines, .. } => {
                    self.inlines(inlines, doc, span, parent, ids);
                }
                Block::BlockQuote { blocks, .. } => {
                    self.blocks(blocks, doc, span, parent, ids);
                }
                Block::Div { blocks, attrs, .. } => {
                    self.label(attrs.id.as_deref());
                    self.blocks(blocks, doc, span, parent, ids);
                }
                Block::List { items, .. } => {
                    for item in items {
                        self.blocks(&item.blocks, doc, span, parent, ids);
                    }
                }
                Block::DefinitionList { items, .. } => {
                    for item in items {
                        self.inlines(&item.term, doc, span, parent, ids);
                        for blocks in &item.definitions {
                            self.blocks(blocks, doc, span, parent, ids);
                        }
                    }
                }
                Block::Callout { title, blocks, attrs, .. } => {
                    self.label(attrs.id.as_deref());
                    if let Some(title) = title {
                        self.inlines(title, doc, span, parent, ids);
                    }
                    self.blocks(blocks, doc, span, parent, ids);
                }
                Block::Cell(cell) => {
                    self.label(cell.attrs.id.as_deref());
                    if cell.show_output {
                        for output in &cell.outputs {
                            self.blocks(
                                output.pick("docx"),
                                doc,
                                span,
                                parent,
                                ids,
                            );
                        }
                    }
                }
                Block::Figure { kind, caption, body, attrs, .. } => {
                    self.label(attrs.id.as_deref());
                    let caption_first =
                        matches!(kind, FigureKind::Table | FigureKind::Listing);
                    if !caption_first {
                        self.blocks(body, doc, span, parent, ids);
                    }
                    if let Some(caption) = caption {
                        self.inlines(caption, doc, span, parent, ids);
                    }
                    if caption_first {
                        self.blocks(body, doc, span, parent, ids);
                    }
                }
                Block::Table { attrs, .. } => {
                    self.label(attrs.id.as_deref());
                    for cell in super::tables::rendered_cells(block) {
                        self.inlines(&cell.inlines, doc, span, parent, ids);
                    }
                }
                Block::CodeBlock { attrs, .. } => {
                    self.label(attrs.id.as_deref());
                }
                Block::Bibliography { .. } => {
                    if let Some(bibliography) = &doc.resolved.bibliography {
                        self.label(Some(BIBLIOGRAPHY_ANCHOR));
                        for entry in &bibliography.entries {
                            if let Some(prefix) = &entry.prefix {
                                self.inlines(prefix, doc, span, parent, ids);
                            }
                            self.inlines(
                                &entry.inlines, doc, span, parent, ids,
                            );
                        }
                    }
                }
                Block::Math { attrs, .. } => self.label(attrs.id.as_deref()),
                _ => {}
            }
        }
    }

    fn inlines(
        &mut self, inlines: &'a [Inline], doc: &'a Document,
        span: Option<&SourceSpan>, parent: Option<usize>, ids: &mut Ids,
    ) {
        for inline in inlines {
            match inline {
                Inline::Footnote { blocks, label, number } => {
                    let index = self.notes.len();
                    let kind = span
                        .and_then(|span| span.file.as_deref())
                        .filter(|path| *path != doc.source.path)
                        .map_or(Some(doc.source.kind), |path| {
                            crate::ir::detect_kind(path).ok()
                        });
                    let label = label
                        .as_deref()
                        .filter(|_| kind == Some(SourceKind::Typst));
                    self.label(label);
                    self.inline_notes
                        .insert(inline as *const Inline as usize, index);
                    if let Some(number) = number {
                        self.numbers.entry(*number).or_insert(index);
                    }
                    self.notes.push(Note {
                        id: ids.footnote,
                        blocks,
                        span: span.cloned(),
                        parent,
                        label,
                        bookmark: None,
                    });
                    ids.footnote += 1;
                    self.blocks(blocks, doc, span, Some(index), ids);
                }
                Inline::FootnoteRef { number: Some(number), .. } => {
                    self.references.insert(*number);
                }
                Inline::Emph { inlines }
                | Inline::Strong { inlines }
                | Inline::Strikeout { inlines }
                | Inline::Subscript { inlines }
                | Inline::Superscript { inlines }
                | Inline::Underline { inlines }
                | Inline::SmallCaps { inlines } => {
                    self.inlines(inlines, doc, span, parent, ids);
                }
                Inline::Span { inlines, attrs }
                | Inline::Link { inlines, attrs, .. } => {
                    self.label(attrs.id.as_deref());
                    self.inlines(inlines, doc, span, parent, ids);
                }
                Inline::Code { attrs, .. } | Inline::Image { attrs, .. } => {
                    self.label(attrs.id.as_deref());
                }
                Inline::Cite { resolved: Some(citation), .. } => {
                    self.inlines(&citation.inlines, doc, span, parent, ids);
                }
                Inline::Ref {
                    supplement,
                    form,
                    resolved: Some(reference),
                    ..
                } => {
                    let page = *form == RefForm::Page;
                    let known = self.targets.as_ref().map(|targets| {
                        targets.contains(&anchor(&reference.anchor))
                    });
                    if !page || known != Some(true) {
                        self.inlines(&reference.text, doc, span, parent, ids);
                    }
                    if page
                        && known != Some(false)
                        && let Some(inlines) = supplement
                    {
                        self.inlines(inlines, doc, span, parent, ids);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Emitter<'_> {
    pub(crate) fn has_bookmark(&self, label: &str) -> bool {
        self.content.plan.bookmarks.contains_key(&anchor(label))
    }

    pub(crate) fn start_bookmark(&mut self, label: &str) -> Option<u32> {
        let name = anchor(label);
        if self.content.plan.bookmarks.get(&name).copied()
            != Some(label.as_ptr() as usize)
        {
            return None;
        }
        self.bookmark_named(&name)
    }

    fn bookmark_named(&mut self, name: &str) -> Option<u32> {
        if !self.content.claimed.insert(name.to_owned()) {
            return None;
        }
        let id = self.ids.bookmark;
        self.ids.bookmark += 1;
        self.buffer().push_str(&format!(
            "<w:bookmarkStart w:id=\"{id}\" w:name=\"{}\"/>",
            xml::escape(name),
        ));
        Some(id)
    }

    pub(crate) fn end_bookmark(&mut self, id: Option<u32>) {
        if let Some(id) = id {
            self.buffer().push_str(&format!("<w:bookmarkEnd w:id=\"{id}\"/>"));
        }
    }

    pub(crate) fn content_warning(
        &mut self, code: &'static str, message: impl Into<String>,
    ) {
        let diagnostic = self.diagnostics.warn(code, message);
        diagnostic.span = self.content.span.clone();
        diagnostic.path = Some(
            self.content
                .span
                .as_ref()
                .and_then(|span| span.file.clone())
                .unwrap_or_else(|| self.doc.source.path.clone()),
        );
    }

    fn note_run(&mut self, contents: &str) -> String {
        let style =
            self.styles.ensure("Footnote Reference", StyleKind::Character);
        let props = xml::RunProps { style: Some(style), ..Default::default() };
        format!("<w:r>{}{contents}</w:r>", props.to_xml())
    }

    pub(crate) fn footnote_inline(&mut self, inline: &Inline) {
        let Some(&index) = self
            .content
            .plan
            .inline_notes
            .get(&(inline as *const Inline as usize))
        else {
            return;
        };
        let note = self.content.plan.notes[index].clone();
        if self.part == Part::Footnotes {
            let run = self.note_run(&format!(
                "<w:t xml:space=\"preserve\">{}</w:t>",
                note.id
            ));
            self.buffer().push_str(&run);
            return;
        }
        self.note_reference_run(index);
        // Preorder is the logical numbering order. All descendants immediately
        // follow the parent reference, since Word cannot nest footnotes.
        for child in index + 1..self.content.plan.notes.len() {
            let mut parent = self.content.plan.notes[child].parent;
            while let Some(ancestor) = parent {
                if ancestor == index {
                    self.note_reference_run(child);
                    break;
                }
                parent = self.content.plan.notes[ancestor].parent;
            }
        }
    }

    fn note_reference_run(&mut self, index: usize) {
        let note = self.content.plan.notes[index].clone();
        let bookmark = match (note.label, note.bookmark.as_deref()) {
            (Some(label), _) => self.start_bookmark(label),
            (None, Some(name)) => self.bookmark_named(name),
            _ => None,
        };
        let run = self
            .note_run(&format!("<w:footnoteReference w:id=\"{}\"/>", note.id));
        self.buffer().push_str(&run);
        self.end_bookmark(bookmark);
    }

    pub(crate) fn footnote_reference(&mut self, number: Option<u32>) {
        let note = number
            .and_then(|number| self.content.plan.numbers.get(&number))
            .map(|&index| self.content.plan.notes[index].clone());
        let Some(note) = note else {
            let run = self.note_run("<w:t xml:space=\"preserve\">?</w:t>");
            self.buffer().push_str(&run);
            return;
        };
        let run = self.note_run(&format!(
            "<w:t xml:space=\"preserve\">{}</w:t>",
            note.id
        ));
        if let Some(bookmark) = note.bookmark {
            self.buffer().push_str(&xml::field(
                &format!("NOTEREF {bookmark} \\f \\h"),
                &run,
            ));
        } else {
            self.buffer().push_str(&run);
        }
    }

    pub(crate) fn finish_footnotes(&mut self) {
        let notes = self.content.plan.notes.clone();
        let part = std::mem::replace(&mut self.part, Part::Footnotes);
        let containers =
            std::mem::replace(&mut self.containers, vec![Container::Footnote]);
        let body = std::mem::take(&mut self.content.body);
        let span = self.content.span.take();
        for note in notes {
            self.content.body = BodyState::default();
            self.content.span = note.span;
            self.notes.push_str(&format!("<w:footnote w:id=\"{}\">", note.id));
            let marker = self.note_run("<w:footnoteRef/>");
            self.content.paragraph_prefix = format!(
                "{marker}<w:r><w:t xml:space=\"preserve\"> </w:t></w:r>",
            );
            if !matches!(note.blocks.first(), Some(Block::Paragraph { inlines, .. }) if !inlines.is_empty())
            {
                let style =
                    self.styles.ensure("Footnote Text", StyleKind::Paragraph);
                self.start_paragraph(xml::ParaProps {
                    style: Some(style),
                    ..Default::default()
                });
                self.end_paragraph();
            }
            self.blocks(note.blocks);
            self.finish_block_sequence();
            self.notes.push_str("</w:footnote>");
        }
        self.part = part;
        self.containers = containers;
        self.content.body = body;
        self.content.span = span;
    }
}
