//! Inline content to Word runs (docx-output.md §4.2).
//! Owner: T40.

use crate::ir::{Attrs, Inline, RefForm};
use crate::resolve::{Term, anchor, term};

use super::{Emitter, StyleKind, xml};

/// Text and tabs waiting for the next boundary between Word runs.
#[derive(Default)]
struct PendingRun {
    properties: String,
    text: String,
}

impl PendingRun {
    fn text(
        &mut self, emitter: &mut Emitter<'_>, text: &str, props: &xml::RunProps,
    ) {
        if text.is_empty() {
            return;
        }
        let properties = props.to_xml();
        if properties != self.properties {
            self.flush(emitter);
            self.properties = properties;
        }
        self.text.push_str(text);
    }

    fn flush(&mut self, emitter: &mut Emitter<'_>) {
        if self.text.is_empty() {
            return;
        }
        let contents = text_contents(&self.text);
        emitter
            .buffer()
            .push_str(&format!("<w:r>{}{contents}</w:r>", self.properties,));
        self.text.clear();
    }
}

impl Emitter<'_> {
    /// Writes inline content, coalescing neighbouring text with equal rPr.
    pub(crate) fn inlines(
        &mut self, inlines: &[Inline], props: &xml::RunProps,
    ) {
        let mut pending = PendingRun::default();
        self.inline_sequence(inlines, props, &mut pending);
        pending.flush(self);
    }

    fn inline_sequence(
        &mut self, inlines: &[Inline], props: &xml::RunProps,
        pending: &mut PendingRun,
    ) {
        for inline in inlines {
            self.inline(inline, props, pending);
        }
    }

    fn inline(
        &mut self, inline: &Inline, props: &xml::RunProps,
        pending: &mut PendingRun,
    ) {
        match inline {
            Inline::Text { text } => pending.text(self, text, props),
            Inline::SoftBreak => pending.text(self, " ", props),
            Inline::Emph { .. }
            | Inline::Strong { .. }
            | Inline::Strikeout { .. }
            | Inline::Subscript { .. }
            | Inline::Superscript { .. }
            | Inline::Underline { .. }
            | Inline::SmallCaps { .. } => {
                self.formatted_inline(inline, props, pending);
            }
            Inline::Code { text, attrs } => {
                if attrs.id.is_some() {
                    pending.flush(self);
                }
                let bookmark = self.inline_bookmark(attrs);
                let mut props = props.clone();
                props.style = Some(
                    self.styles.ensure("Verbatim Char", StyleKind::Character),
                );
                pending.text(self, text, &props);
                if bookmark.is_some() {
                    pending.flush(self);
                    self.end_bookmark(bookmark);
                }
            }
            Inline::Span { inlines, attrs } => {
                self.span_inline(inlines, attrs, props, pending);
            }
            Inline::Link { inlines, url, title, attrs } => {
                self.link_inline(
                    inlines,
                    url,
                    title.as_deref(),
                    attrs,
                    props,
                    pending,
                );
            }
            Inline::Ref { .. } => self.reference_inline(inline, props, pending),
            Inline::Cite { .. } => self.citation_inline(inline, props, pending),
            Inline::Raw { format, .. } if format != "openxml" => {}
            _ => {
                pending.flush(self);
                self.separate_inline(inline);
            }
        }
    }

    fn formatted_inline(
        &mut self, inline: &Inline, props: &xml::RunProps,
        pending: &mut PendingRun,
    ) {
        let mut props = props.clone();
        let inlines = match inline {
            Inline::Emph { inlines } => {
                props.italic = true;
                inlines
            }
            Inline::Strong { inlines } => {
                props.bold = true;
                inlines
            }
            Inline::Strikeout { inlines } => {
                props.strike = true;
                inlines
            }
            Inline::Subscript { inlines } => {
                props.vert_align = Some("subscript");
                inlines
            }
            Inline::Superscript { inlines } => {
                props.vert_align = Some("superscript");
                inlines
            }
            Inline::Underline { inlines } => {
                props.underline = true;
                inlines
            }
            Inline::SmallCaps { inlines } => {
                props.small_caps = true;
                inlines
            }
            _ => return,
        };
        self.inline_sequence(inlines, &props, pending);
    }

    fn inline_bookmark(&mut self, attrs: &Attrs) -> Option<u32> {
        attrs.id.as_deref().and_then(|label| self.start_bookmark(label))
    }

    fn span_inline(
        &mut self, inlines: &[Inline], attrs: &Attrs, props: &xml::RunProps,
        pending: &mut PendingRun,
    ) {
        if attrs.id.is_some() {
            pending.flush(self);
        }
        let bookmark = self.inline_bookmark(attrs);
        let mut props = props.clone();
        if let Some(style) = attrs.pairs.get("custom-style") {
            props.style =
                Some(self.styles.custom(style, StyleKind::Character, None));
        }
        self.inline_sequence(inlines, &props, pending);
        if bookmark.is_some() {
            pending.flush(self);
            self.end_bookmark(bookmark);
        }
    }

    fn link_inline(
        &mut self, inlines: &[Inline], url: &str, title: Option<&str>,
        attrs: &Attrs, props: &xml::RunProps, pending: &mut PendingRun,
    ) {
        let attribute = if let Some(label) = url.strip_prefix('#') {
            if self.has_bookmark(label) {
                Some(format!("w:anchor=\"{}\"", anchor(label)))
            } else {
                if label == "refs"
                    || self.doc.resolved.labels.contains_key(label)
                {
                    self.unknown_anchor(label);
                }
                None
            }
        } else {
            let rel = self.package.hyperlink_rel(self.part, url);
            Some(format!("r:id=\"{}\"", xml::escape(&rel)))
        };
        if attrs.id.is_some() || attribute.is_some() {
            pending.flush(self);
        }
        let bookmark = self.inline_bookmark(attrs);
        if let Some(attribute) = attribute {
            let tooltip = if url.starts_with('#') {
                String::new()
            } else {
                title.map_or_else(String::new, |title| {
                    format!(" w:tooltip=\"{}\"", xml::escape(title))
                })
            };
            self.buffer().push_str(&format!(
                "<w:hyperlink {attribute}{tooltip} w:history=\"1\">",
            ));
            let mut props = props.clone();
            props.style =
                Some(self.styles.ensure("Hyperlink", StyleKind::Character));
            self.inline_sequence(inlines, &props, pending);
            pending.flush(self);
            self.buffer().push_str("</w:hyperlink>");
        } else {
            self.inline_sequence(inlines, props, pending);
        }
        if bookmark.is_some() {
            pending.flush(self);
            self.end_bookmark(bookmark);
        }
    }

    fn reference_inline(
        &mut self, inline: &Inline, props: &xml::RunProps,
        pending: &mut PendingRun,
    ) {
        let Inline::Ref { target, supplement, form, resolved, .. } = inline
        else {
            return;
        };
        let Some(reference) = resolved else {
            pending.text(self, &format!("[?{target}]"), props);
            return;
        };
        if !self.has_bookmark(&reference.anchor) {
            self.unknown_anchor(&reference.anchor);
            self.inline_sequence(&reference.text, props, pending);
            return;
        }
        pending.flush(self);
        let anchor = anchor(&reference.anchor);
        self.buffer().push_str(&format!(
            "<w:hyperlink w:anchor=\"{anchor}\" w:history=\"1\">",
        ));
        if *form == RefForm::Page {
            self.page_reference(supplement.as_deref(), &anchor, props, pending);
        } else {
            self.inline_sequence(&reference.text, props, pending);
        }
        pending.flush(self);
        self.buffer().push_str("</w:hyperlink>");
    }

    fn page_reference(
        &mut self, supplement: Option<&[Inline]>, anchor: &str,
        props: &xml::RunProps, pending: &mut PendingRun,
    ) {
        match supplement {
            Some([]) => {}
            Some(inlines) => {
                self.inline_sequence(inlines, props, pending);
                pending.text(self, "\u{a0}", props);
            }
            None => {
                let page = term(&self.lang, Term::Page);
                pending.text(self, &page, props);
                pending.text(self, "\u{a0}", props);
            }
        }
        pending.flush(self);
        let result = format!(
            "<w:r>{}<w:t xml:space=\"preserve\">?</w:t></w:r>",
            props.to_xml(),
        );
        self.buffer()
            .push_str(&xml::field(&format!("PAGEREF {anchor} \\h"), &result));
    }

    fn citation_inline(
        &mut self, inline: &Inline, props: &xml::RunProps,
        pending: &mut PendingRun,
    ) {
        let Inline::Cite { items, resolved, .. } = inline else {
            return;
        };
        if let Some(citation) = resolved {
            self.inline_sequence(&citation.inlines, props, pending);
        } else {
            let keys = items
                .iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            pending.text(self, &format!("[?{keys}]"), props);
        }
    }

    fn separate_inline(&mut self, inline: &Inline) {
        match inline {
            Inline::LineBreak => self.buffer().push_str("<w:r><w:br/></w:r>"),
            Inline::Math { source, dialect } => {
                self.math_inline(source, *dialect)
            }
            Inline::Image { url, alt, title, attrs } => {
                let bookmark = self.inline_bookmark(attrs);
                self.image_run(url, alt, title.as_deref(), attrs);
                self.end_bookmark(bookmark);
            }
            Inline::Footnote { .. } => self.footnote_inline(inline),
            Inline::FootnoteRef { number, .. } => {
                self.footnote_reference(*number)
            }
            Inline::Raw { format, text } if format == "openxml" => {
                self.raw_openxml(text, true);
            }
            _ => {}
        }
    }

    fn unknown_anchor(&mut self, label: &str) {
        self.content_warning(
            "docx-unknown-anchor",
            format!("DOCX bookmark for {label:?} is not emitted"),
        );
    }
}

fn text_contents(text: &str) -> String {
    text.split('\t')
        .map(|part| {
            format!(
                "<w:t xml:space=\"preserve\">{}</w:t>",
                xml::escape(&part.replace('\n', " ")),
            )
        })
        .collect::<Vec<_>>()
        .join("<w:tab/>")
}

#[cfg(test)]
mod tests {
    use super::text_contents;

    #[test]
    fn run_text_preserves_spaces_and_emits_tabs() {
        assert_eq!(
            text_contents(" a&<\tb\nc\t"),
            concat!(
                "<w:t xml:space=\"preserve\"> a&amp;&lt;</w:t><w:tab/>",
                "<w:t xml:space=\"preserve\">b c</w:t><w:tab/>",
                "<w:t xml:space=\"preserve\"></w:t>",
            ),
        );
    }
}
