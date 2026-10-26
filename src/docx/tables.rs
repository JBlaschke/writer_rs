//! DOCX table rendering (docx-output.md §4.12).
//! Owner: T41. Stub created by T00.

use crate::ir::{Block, plain_text};

use super::Emitter;

impl Emitter<'_> {
    /// Writes each table row as plain text until T41 supplies table mapping.
    pub(crate) fn table(&mut self, b: &Block, figure: Option<&Block>) {
        let _ = figure;
        if let Block::Table { head, body, foot, .. } = b {
            for row in head.iter().chain(body).chain(foot) {
                let text = row
                    .cells
                    .iter()
                    .map(|cell| plain_text(&cell.inlines))
                    .collect::<Vec<_>>()
                    .join("\t");
                self.text_paragraph(&text, "Body Text");
            }
        }
        self.unsupported("tables", "T41", b.span());
    }
}
