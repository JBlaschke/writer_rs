//! DOCX image and figure rendering (docx-output.md §4.10, §4.16).
//! Owner: T41. Stub created by T00.

use crate::ir::{Attrs, Inlines, plain_text};

use super::Emitter;

impl Emitter<'_> {
    /// Writes image alt text until T41 supplies picture runs.
    #[allow(dead_code)] // Called by the T40 inline emitter when it arrives.
    pub(crate) fn image_run(
        &mut self, url: &str, alt: &Inlines, title: Option<&str>, attrs: &Attrs,
    ) {
        let _ = (url, title, attrs);
        self.text_run(&plain_text(alt));
        self.unsupported("images", "T41", None);
    }
}
