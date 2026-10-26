//! DOCX equations and fallback images (docx-output.md §4.15; math.md §5).
//! Owner: T42. Stub created by T00.

use crate::ir::{Block, MathDialect};

use super::Emitter;

impl Emitter<'_> {
    /// Writes the source until T42 supplies inline equation mapping.
    #[allow(dead_code)] // Called by the T40 inline emitter when it arrives.
    pub(crate) fn math_inline(&mut self, source: &str, dialect: MathDialect) {
        let _ = dialect;
        self.text_run(source);
        self.unsupported("inline math", "T42", None);
    }

    /// Writes the source until T42 supplies display equation mapping.
    pub(crate) fn math_block(&mut self, b: &Block) {
        if let Block::Math { source, .. } = b {
            self.text_paragraph(source, "Body Text");
        }
        self.unsupported("display math", "T42", b.span());
    }
}
