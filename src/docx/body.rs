//! Document body and block rendering (docx-output.md §4.1–§4.7).
//! Owner: T40. Stub created by T00.

use crate::ir::{Block, plain_text};

use super::Emitter;

impl Emitter<'_> {
    /// Writes the T04 plain-text body; full block mapping arrives in T40.
    pub(crate) fn document(&mut self) {
        if let Some(title) = &self.doc.meta.title {
            self.text_paragraph(title, "Title");
        }
        for block in &self.doc.blocks {
            match block {
                Block::Heading { level, inlines, number, .. } => {
                    let text = plain_text(inlines);
                    let text = number.as_ref().map_or(text.clone(), |number| {
                        format!("{number} {text}")
                    });
                    self.text_paragraph(&text, &format!("heading {level}"));
                }
                Block::Paragraph { inlines, .. } => {
                    self.text_paragraph(&plain_text(inlines), "Body Text");
                }
                Block::Figure { .. } => self.figure(block),
                Block::Table { .. } => self.table(block, None),
                Block::Toc { depth, .. } => self.toc(*depth),
                Block::Bibliography { .. } => self.bibliography(),
                Block::Math { .. } => self.math_block(block),
                _ => self.unsupported("block mapping", "T40", block.span()),
            }
        }
    }
}
