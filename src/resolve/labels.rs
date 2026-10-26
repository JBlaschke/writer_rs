//! Label collection and anchor generation (references.md §1).
//! Owner: T17. Stub created by T00.

use std::collections::BTreeSet;

use crate::ir::{Inline, Inlines};

/// Pandoc's ASCII bookmark name, shared by DOCX bookmarks and LaTeX labels.
pub fn anchor(label: &str) -> String {
    if !label.is_empty()
        && label.len() <= 39
        && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        format!("_{label}")
    } else {
        let hex = sha1_smol::Sha1::from(label.as_bytes()).digest().to_string();
        format!("_{}", &hex[1..])
    }
}

pub(crate) fn reserved(label: &str) -> bool {
    if label == "refs"
        || label.starts_with("ref-")
        || label.starts_with("writer-")
    {
        return true;
    }
    let rest =
        label.strip_prefix("fn-").or_else(|| label.strip_prefix("fnref-"));
    rest.is_some_and(|rest| {
        let mut parts = rest.split('-');
        let digits = |part: &str| {
            !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit())
        };
        parts.next().is_some_and(digits)
            && parts.next().is_none_or(digits)
            && parts.next().is_none()
    })
}

pub(crate) fn label_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            Inline::Text { text: value }
            | Inline::Code { text: value, .. }
            | Inline::Math { source: value, .. } => text.push_str(value),
            Inline::Ref { target, .. } => text.push_str(target),
            Inline::Cite { items, .. } => text.push_str(
                &items
                    .iter()
                    .map(|item| item.key.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
            Inline::SoftBreak | Inline::LineBreak => text.push(' '),
            Inline::Image { alt, .. } => text.push_str(&label_text(alt)),
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Link { inlines, .. }
            | Inline::Span { inlines, .. } => {
                text.push_str(&label_text(inlines))
            }
            Inline::Footnote { .. }
            | Inline::FootnoteRef { .. }
            | Inline::Raw { .. } => {}
        }
    }
    text
}

pub(crate) fn auto_id(inlines: &[Inline], taken: &BTreeSet<String>) -> String {
    let text: String = label_text(inlines)
        .to_lowercase()
        .chars()
        .filter(|&c| {
            c.is_whitespace()
                || (c.is_alphanumeric() && typst_syntax::is_id_continue(c))
                || matches!(c, '_' | '-' | '.')
        })
        .collect();
    let joined = text.split_whitespace().collect::<Vec<_>>().join("-");
    let base = joined
        .find(char::is_alphabetic)
        .map(|at| &joined[at..])
        .filter(|text| !text.is_empty())
        .unwrap_or("section");
    if !taken.contains(base) && !reserved(base) {
        return base.into();
    }
    let mut suffix = 1u64;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

pub(crate) fn text_inline(text: impl Into<String>) -> Inline {
    Inline::Text { text: text.into() }
}

pub(crate) fn merge_text(inlines: Inlines) -> Inlines {
    let mut merged = Vec::new();
    for inline in inlines {
        match (merged.last_mut(), inline) {
            (Some(Inline::Text { text }), Inline::Text { text: next }) => {
                text.push_str(&next)
            }
            (_, inline) => merged.push(inline),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_table() {
        for (label, expected) in [
            ("Fig_1", "_Fig_1"),
            ("9lives", "_9lives"),
            ("_x", "__x"),
            (
                "abcdefghijklmnopqrstuvwxyzabcdefghijklm",
                "_abcdefghijklmnopqrstuvwxyzabcdefghijklm",
            ),
            (
                "abcdefghijklmnopqrstuvwxyzabcdefghijklmn",
                "_40e479dd08aa3c669b21f890ab9ddf75a57eedc",
            ),
            ("sec-intro", "_fdabef51864cece78e14c461e4ab265d0b74de7"),
            ("fig:plot", "_fd15c33cfa7216ea070647016ea45ecd9b6a972"),
            ("eq.x", "_19d18211de2593daa5a6036a9854b2fbe806b4e"),
            ("größe", "_3c6a3acf1b4a1beba25d8a45a3ccd719721538b"),
        ] {
            assert_eq!(anchor(label), expected, "{label}");
        }
    }

    #[test]
    fn auto_id_examples() {
        let mut taken = BTreeSet::new();
        for (text, expected) in [
            ("1.2 GPU Allocation: Results!", "gpu-allocation-results"),
            ("A & B", "a-b"),
            ("a·b", "ab"),
            ("Intro", "intro"),
            ("Intro", "intro-1"),
            ("42", "section"),
        ] {
            let id = auto_id(&[text_inline(text)], &taken);
            assert_eq!(id, expected);
            taken.insert(id);
        }
        taken.insert("writer".into());
        assert_eq!(auto_id(&[text_inline("Writer")], &taken), "writer-1");
        taken.insert("writer-1".into());
        assert_eq!(auto_id(&[text_inline("Writer")], &taken), "writer-2");
    }

    #[test]
    fn reserved() {
        for label in ["refs", "ref-x", "writer-a", "fn-1", "fnref-2-3"] {
            assert!(super::reserved(label));
        }
        for label in ["fn-x", "refs-1", "fn-1-x", "fnref-1-2-3", "fnre-1"] {
            assert!(!super::reserved(label));
        }
    }
}
