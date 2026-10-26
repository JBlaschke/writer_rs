#![allow(dead_code)]

/// High-level counts from docx-rs's independent reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ReadBackCounts {
    pub paragraphs: usize,
    pub styles: usize,
    pub abstract_nums: usize,
    pub images: usize,
}

/// Read a generated DOCX without decoding media into preview images.
pub fn read_back(bytes: &[u8]) -> ReadBackCounts {
    let options =
        docx_rs::ReadDocxOptions::default().with_image_previews(false);
    let docx = docx_rs::read_docx_with_options(bytes, options)
        .expect("generated DOCX must be readable by docx-rs");
    ReadBackCounts {
        paragraphs: docx
            .document
            .children
            .iter()
            .filter(|child| {
                matches!(child, docx_rs::DocumentChild::Paragraph(_))
            })
            .count(),
        styles: docx.styles.styles.len(),
        abstract_nums: docx.numberings.abstract_nums.len(),
        images: docx.images.len(),
    }
}
