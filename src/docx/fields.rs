//! Figures, caption bookmarks and Word SEQ fields (docx-output.md §4.10–§4.11).

use std::collections::BTreeMap;

use crate::config::{FigureNumbering, HeadingNumbering};
use crate::ir::{Attrs, Block, FigureKind, Inlines, LabelKind};
use crate::resolve::{caption_separator, supplement};

use super::{Container, Emitter, StyleKind, xml};

/// Word's sequence counters follow emission order, including nested figures.
#[derive(Default)]
pub(crate) struct FieldState {
    sequences: BTreeMap<&'static str, u32>,
    pub(crate) toc_seen: bool,
}

impl Emitter<'_> {
    /// Word resets section-scoped SEQ counters at actual Heading 1 paragraphs.
    pub(crate) fn heading_one(&mut self) {
        if self.section_sequences() {
            self.fields.sequences.clear();
        }
    }

    /// Writes the figure body and caption in their kind-specific order.
    pub(crate) fn figure(&mut self, block: &Block) {
        let Block::Figure { kind, caption, body, attrs, number, .. } = block
        else {
            return;
        };
        let captioned = caption.is_some() || number.is_some();
        let bookmark = if captioned {
            None
        } else {
            attrs.id.as_deref().and_then(|id| self.start_bookmark(id))
        };
        if matches!(kind, FigureKind::Table | FigureKind::Listing) {
            self.caption(*kind, caption, attrs, number.as_deref());
        }
        match kind {
            FigureKind::Image => {
                let name = if captioned {
                    "Captioned Figure"
                } else {
                    "Figure"
                };
                let style = self.styles.ensure(name, StyleKind::Paragraph);
                self.containers.push(Container::Style(style));
                self.blocks(body);
                self.containers.pop();
            }
            FigureKind::Table => {
                for child in body {
                    if let Block::Table { attrs, .. } = child {
                        let previous_span = self.content.span.clone();
                        if let Some(span) = child.span() {
                            self.content.span = Some(span.clone());
                        }
                        let table_bookmark = attrs
                            .id
                            .as_deref()
                            .and_then(|id| self.start_bookmark(id));
                        self.separate_table();
                        self.table(child, Some(block));
                        self.end_bookmark(table_bookmark);
                        self.content.span = previous_span;
                    } else {
                        self.blocks(std::slice::from_ref(child));
                    }
                }
            }
            FigureKind::Listing | FigureKind::Other => self.blocks(body),
        }
        if matches!(kind, FigureKind::Image | FigureKind::Other) {
            self.caption(*kind, caption, attrs, number.as_deref());
        }
        self.end_bookmark(bookmark);
    }

    fn caption(
        &mut self, kind: FigureKind, caption: &Option<Inlines>, attrs: &Attrs,
        number: Option<&str>,
    ) {
        if caption.is_none() && number.is_none() {
            return;
        }
        let (style, label_kind, sequence) = match kind {
            FigureKind::Image => ("Image Caption", LabelKind::Figure, "Figure"),
            FigureKind::Table => ("Table Caption", LabelKind::Table, "Table"),
            FigureKind::Listing => ("Caption", LabelKind::Listing, "Listing"),
            FigureKind::Other => ("Caption", LabelKind::Figure, "Figure"),
        };
        let style = self.styles.ensure(style, StyleKind::Paragraph);
        self.start_paragraph(xml::ParaProps {
            style: Some(style),
            ..Default::default()
        });
        let bookmark =
            attrs.id.as_deref().and_then(|id| self.start_bookmark(id));
        if let Some(number) = number {
            let supplement = attrs
                .id
                .as_ref()
                .and_then(|id| self.doc.resolved.labels.get(id))
                .map(|label| label.supplement.clone())
                .unwrap_or_else(|| supplement(label_kind, &self.lang));
            self.text_run(&supplement);
            self.text_run("\u{a0}");
            self.sequence_number(sequence, number);
            self.end_bookmark(bookmark);
            if let Some(caption) = caption {
                self.text_run(caption_separator(&self.lang));
                self.inlines(caption, &xml::RunProps::default());
            }
        } else {
            if let Some(caption) = caption {
                self.inlines(caption, &xml::RunProps::default());
            }
            self.end_bookmark(bookmark);
        }
        self.end_paragraph();
    }

    fn section_sequences(&self) -> bool {
        self.config.numbering.figures == FigureNumbering::Section
            && self.config.numbering.headings != HeadingNumbering::Off
    }

    /// Keeps the cached number and Word's next recomputed value in agreement.
    pub(crate) fn sequence_number(
        &mut self, identifier: &'static str, number: &str,
    ) {
        let section = self.section_sequences();
        let (chapter, counter) = if section {
            number
                .rsplit_once('.')
                .map_or((None, number), |(chapter, n)| (Some(chapter), n))
        } else {
            (None, number)
        };
        let Ok(value) = counter.parse::<u32>() else {
            // Resolved numbers are numeric; retain malformed hand-written IR.
            self.text_run(number);
            return;
        };
        if let Some(chapter) = chapter {
            self.text_run(chapter);
            self.text_run(".");
        }
        let next = self
            .fields
            .sequences
            .get(identifier)
            .copied()
            .unwrap_or(0)
            .checked_add(1);
        let mut instruction = format!("SEQ {identifier} \\* ARABIC");
        if section {
            instruction.push_str(" \\s 1");
        }
        if next != Some(value) {
            instruction.push_str(&format!(" \\r {value}"));
        }
        self.fields.sequences.insert(identifier, value);
        let cached = format!(
            "<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r>",
            xml::escape(counter),
        );
        self.buffer().push_str(&xml::field(&instruction, &cached));
    }
}
