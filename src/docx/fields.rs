//! Word fields and caption field rendering (docx-output.md §4.10–§4.11).
//! Owner: T41. Stub created by T00.

use crate::ir::{Block, plain_text};

use super::Emitter;

impl Emitter<'_> {
    /// Writes a figure caption until T41 supplies figure and caption mapping.
    pub(crate) fn figure(&mut self, b: &Block) {
        if let Block::Figure { caption, .. } = b {
            let text = caption
                .as_ref()
                .map_or_else(String::new, |caption| plain_text(caption));
            self.text_paragraph(&text, "Body Text");
        }
        self.unsupported("figures", "T41", b.span());
    }
}
