//! DOCX table of contents rendering (docx-output.md §4.13).
//! Owner: T41. Stub created by T00.

use super::Emitter;

impl Emitter<'_> {
    /// Reports the placeholder until T41 supplies table-of-contents mapping.
    pub(crate) fn toc(&mut self, depth: u8) {
        let _ = depth;
        self.unsupported("table of contents", "T41", None);
    }

    /// Reports the placeholder until T41 supplies bibliography mapping.
    pub(crate) fn bibliography(&mut self) {
        self.unsupported("bibliographies", "T41", None);
    }
}
