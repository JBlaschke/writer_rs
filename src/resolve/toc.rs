//! Table of contents model construction (references.md §4.8).
//! Owner: T17. Stub created by T00.

use crate::ir::{Inline, Inlines};

/// Removes note markers from a heading copy without changing the actual heading.
pub(crate) fn without_footnotes(inlines: &[Inline]) -> Inlines {
    let mut out = Vec::new();
    for inline in inlines {
        let mut inline = inline.clone();
        match &mut inline {
            Inline::Footnote { .. } | Inline::FootnoteRef { .. } => continue,
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Link { inlines, .. }
            | Inline::Span { inlines, .. } => {
                *inlines = without_footnotes(inlines)
            }
            Inline::Image { alt, .. } => *alt = without_footnotes(alt),
            Inline::Ref { supplement, resolved, .. } => {
                if let Some(supplement) = supplement {
                    *supplement = without_footnotes(supplement);
                }
                if let Some(reference) = resolved {
                    reference.text = without_footnotes(&reference.text);
                }
            }
            Inline::Cite { items, resolved, .. } => {
                for item in items {
                    if let Some(prefix) = &mut item.prefix {
                        *prefix = without_footnotes(prefix);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        *suffix = without_footnotes(suffix);
                    }
                }
                if let Some(cite) = resolved {
                    cite.inlines = without_footnotes(&cite.inlines);
                    if cite.note && cite.inlines.is_empty() {
                        continue;
                    }
                }
            }
            _ => {}
        }
        out.push(inline);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{LabelKind, RefForm, ResolvedRef};

    #[test]
    fn reference_supplements_and_resolved_text_have_no_toc_notes() {
        let note = Inline::Footnote {
            blocks: Vec::new(),
            label: None,
            number: Some(1),
        };
        let source = vec![Inline::Ref {
            target: "target".into(),
            form: RefForm::Normal,
            span: None,
            supplement: Some(vec![Inline::Emph {
                inlines: vec![note.clone()],
            }]),
            resolved: Some(ResolvedRef {
                kind: LabelKind::Heading,
                number: "1".into(),
                anchor: "target".into(),
                text: vec![Inline::Strong { inlines: vec![note] }],
            }),
        }];
        let stripped = without_footnotes(&source);
        let Inline::Ref {
            supplement: Some(supplement),
            resolved: Some(resolved),
            ..
        } = &stripped[0]
        else {
            panic!("resolved reference expected");
        };
        assert!(
            matches!(&supplement[0], Inline::Emph { inlines } if inlines.is_empty())
        );
        assert!(
            matches!(&resolved.text[0], Inline::Strong { inlines } if inlines.is_empty())
        );
        assert!(
            matches!(&source[0], Inline::Ref { resolved: Some(resolved), .. }
            if matches!(&resolved.text[0], Inline::Strong { inlines } if inlines.len() == 1))
        );
    }
}
