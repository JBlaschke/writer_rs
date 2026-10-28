//! Cached Word table-of-contents fields and bibliographies (§4.13–§4.14).

use crate::ir::plain_text;
use crate::resolve::{BIBLIOGRAPHY_ANCHOR, Term, anchor, term};

use super::{Emitter, StyleKind, xml};

impl Emitter<'_> {
    /// One complex TOC field spans its cached entry paragraphs inside an SDT.
    pub(crate) fn toc(&mut self, depth: u8) {
        self.buffer().push_str(
            "<w:sdt><w:sdtPr><w:docPartObj><w:docPartGallery w:val=\"Table of Contents\"/>",
        );
        if !self.fields.toc_seen {
            self.buffer().push_str("<w:docPartUnique/>");
            self.fields.toc_seen = true;
        }
        self.buffer().push_str("</w:docPartObj></w:sdtPr><w:sdtContent>");
        let title = self
            .doc
            .meta
            .vars
            .get("toc-title")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| term(&self.lang, Term::Contents));
        self.text_paragraph(&title, "TOC Heading");
        let instruction = format!("TOC \\o \"1-{depth}\" \\h \\z \\u");
        let (begin, end) = xml::field_parts(&instruction);
        let entries: Vec<_> = self
            .doc
            .resolved
            .toc
            .iter()
            .filter(|entry| entry.level <= depth)
            .collect();
        if entries.is_empty() {
            self.start_paragraph(xml::ParaProps::default());
            self.buffer().push_str(&begin);
            self.buffer().push_str(end);
            self.end_paragraph();
        }
        for (index, entry) in entries.iter().enumerate() {
            let style = self
                .styles
                .ensure(&format!("toc {}", entry.level), StyleKind::Paragraph);
            let width = self.available_width_twips();
            self.start_paragraph(xml::ParaProps {
                style: Some(style),
                tabs: vec![format!(
                    "<w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"{width}\"/>",
                )],
                ..Default::default()
            });
            if index == 0 {
                self.buffer().push_str(&begin);
            }
            let linked = self.has_bookmark(&entry.anchor);
            let bookmark = anchor(&entry.anchor);
            if linked {
                self.buffer().push_str(&format!(
                    "<w:hyperlink w:anchor=\"{bookmark}\" w:history=\"1\">",
                ));
            }
            if let Some(number) = &entry.number {
                self.text_run(number);
                self.text_run(" ");
            }
            let text = if entry.anchor == BIBLIOGRAPHY_ANCHOR {
                self.bibliography_title()
            } else {
                plain_text(&entry.inlines)
            };
            self.text_run(&text);
            if linked {
                self.buffer().push_str("<w:r><w:tab/></w:r>");
                self.buffer().push_str(&xml::field(
                    &format!("PAGEREF {bookmark} \\h"),
                    "",
                ));
                self.buffer().push_str("</w:hyperlink>");
            }
            if index + 1 == entries.len() {
                self.buffer().push_str(end);
            }
            self.end_paragraph();
        }
        self.buffer().push_str("</w:sdtContent></w:sdt>");
    }

    /// The bibliography is already formatted; this pass adds paragraphs only.
    pub(crate) fn bibliography(&mut self) {
        let Some(bibliography) = &self.doc.resolved.bibliography else {
            return;
        };
        let style = self.styles.ensure("heading 1", StyleKind::Paragraph);
        self.start_paragraph(xml::ParaProps {
            style: Some(style),
            ..Default::default()
        });
        let bookmark = self.start_bookmark(BIBLIOGRAPHY_ANCHOR);
        self.text_run(&self.bibliography_title());
        self.end_bookmark(bookmark);
        self.end_paragraph();
        for entry in &bibliography.entries {
            let style =
                self.styles.ensure("Bibliography", StyleKind::Paragraph);
            let prefixed = entry.prefix.is_some();
            self.start_paragraph(xml::ParaProps {
                style: Some(style),
                tabs: if prefixed {
                    vec!["<w:tab w:val=\"left\" w:pos=\"720\"/>".into()]
                } else {
                    Vec::new()
                },
                ind: (prefixed || bibliography.hanging_indent).then(|| {
                    "<w:ind w:left=\"720\" w:hanging=\"720\"/>".into()
                }),
                ..Default::default()
            });
            if let Some(prefix) = &entry.prefix {
                self.inlines(prefix, &xml::RunProps::default());
                self.buffer().push_str("<w:r><w:tab/></w:r>");
            }
            self.inlines(&entry.inlines, &xml::RunProps::default());
            self.end_paragraph();
        }
    }

    fn bibliography_title(&self) -> String {
        self.doc
            .meta
            .vars
            .get("reference-section-title")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| term(&self.lang, Term::Bibliography))
    }
}
