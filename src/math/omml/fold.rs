//! Reverse the mathematical alphabets emitted by Typst and math-core.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use codex::styling::{MathStyle, to_style};

pub(super) struct Folded {
    pub text: String,
    pub style: Option<&'static str>,
    pub script: Option<&'static str>,
}

#[derive(Clone, Copy)]
struct Styled {
    base: char,
    style: Option<&'static str>,
    script: Option<&'static str>,
}

const STYLES: &[(MathStyle, Option<&str>, Option<&str>)] = &[
    (MathStyle::Bold, Some("b"), None),
    (MathStyle::Italic, None, None),
    (MathStyle::BoldItalic, Some("bi"), None),
    (MathStyle::Script, Some("p"), Some("script")),
    (MathStyle::BoldScript, Some("b"), Some("script")),
    (MathStyle::Fraktur, Some("p"), Some("fraktur")),
    (MathStyle::BoldFraktur, Some("b"), Some("fraktur")),
    (MathStyle::DoubleStruck, Some("p"), Some("double-struck")),
    (MathStyle::SansSerif, Some("p"), Some("sans-serif")),
    (MathStyle::SansSerifBold, Some("b"), Some("sans-serif")),
    (MathStyle::SansSerifItalic, Some("i"), Some("sans-serif")),
    (MathStyle::SansSerifBoldItalic, Some("bi"), Some("sans-serif")),
    (MathStyle::Monospace, Some("p"), Some("monospace")),
];

static REVERSE: LazyLock<BTreeMap<char, Styled>> = LazyLock::new(|| {
    let mut result = BTreeMap::new();
    for base in bases() {
        for &(style, word_style, script) in STYLES {
            let Some(value) = to_style(base, style).next() else {
                continue;
            };
            if value != base {
                result.entry(value).or_insert(Styled {
                    base,
                    style: word_style,
                    script,
                });
            }
        }
    }
    result
});

fn bases() -> impl Iterator<Item = char> {
    ('A'..='Z')
        .chain('a'..='z')
        .chain('Α'..='Ω')
        .chain('α'..='ω')
        .chain('0'..='9')
}

pub(super) fn character(value: char) -> Folded {
    match REVERSE.get(&value) {
        Some(value) => Folded {
            text: value.base.to_string(),
            style: value.style,
            script: value.script,
        },
        None => Folded { text: value.to_string(), style: None, script: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styled_alphabets_and_letterlike_holes_fold_to_word_properties() {
        for (value, text, style, script) in [
            ('𝐀', "A", Some("b"), None),
            ('𝑨', "A", Some("bi"), None),
            ('𝑥', "x", None, None),
            ('ℎ', "h", None, None),
            ('ℝ', "R", Some("p"), Some("double-struck")),
            ('ℱ', "F", Some("p"), Some("script")),
            ('ℌ', "H", Some("p"), Some("fraktur")),
            ('𝘼', "A", Some("bi"), Some("sans-serif")),
            ('𝟶', "0", Some("p"), Some("monospace")),
        ] {
            let actual = character(value);
            assert_eq!(actual.text, text, "{value}");
            assert_eq!(actual.style, style, "{value}");
            assert_eq!(actual.script, script, "{value}");
        }
    }

    #[test]
    fn every_supported_style_and_base_round_trips_without_identity_pollution() {
        for base in bases() {
            let plain = character(base);
            assert_eq!(plain.text, base.to_string());
            assert_eq!(plain.style, None);
            assert_eq!(plain.script, None);
            for &(style, word_style, script) in STYLES {
                let value = to_style(base, style).next().unwrap();
                if value == base {
                    continue;
                }
                let actual = character(value);
                assert_eq!(actual.text, base.to_string(), "{style:?}: {value}");
                assert_eq!(actual.style, word_style, "{style:?}: {value}");
                assert_eq!(actual.script, script, "{style:?}: {value}");
            }
        }
    }
}
