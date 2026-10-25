//! Code fence attributes and language handling (markdown-dialect.md §4.6).
//! Owner: T01; T11 adds invalid-code-attr. Stub created by T00.

use std::ops::RangeInclusive;

use crate::ir::Attrs;

/// Shared rendering options for fenced code and notebook input cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAttrs {
    /// Display line numbers (`.numbered`, or a valid `lines=` value).
    pub numbered: bool,
    /// Disable syntax highlighting (`.plain`).
    pub plain: bool,
    /// The first displayed line number, defaulting to one.
    pub start: u64,
    /// Inclusive ranges of displayed line numbers to emphasize.
    pub highlight: Vec<RangeInclusive<u64>>,
}

impl CodeAttrs {
    /// Reads classes and pairs; invalid values are ignored, never an error.
    pub fn from_attrs(attrs: &Attrs) -> CodeAttrs {
        let mut code = CodeAttrs {
            numbered: attrs.classes.iter().any(|class| class == "numbered"),
            plain: attrs.classes.iter().any(|class| class == "plain"),
            start: attrs
                .pairs
                .get("start")
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|&number| number >= 1)
                .unwrap_or(1),
            highlight: Vec::new(),
        };
        if let Some(ranges) =
            attrs.pairs.get("highlight").and_then(|value| parse_ranges(value))
        {
            code.highlight.extend(ranges);
        }
        if let Some(ranges) =
            attrs.pairs.get("lines").and_then(|value| parse_ranges(value))
        {
            code.numbered = true;
            code.highlight.extend(ranges);
        }
        code
    }

    /// Whether `line` is emphasized, using its displayed number (`start + i`).
    pub fn is_highlighted(&self, line: u64) -> bool {
        self.highlight.iter().any(|range| range.contains(&line))
    }
}

/// Reads the whole value atomically: one malformed item invalidates every range.
fn parse_ranges(value: &str) -> Option<Vec<RangeInclusive<u64>>> {
    value
        .split([',', '|'])
        .map(|item| {
            let item = item.trim();
            let (start, end) = item.split_once('-').unwrap_or((item, item));
            let start = positive_number(start.trim())?;
            let end = positive_number(end.trim())?;
            (start <= end).then_some(start..=end)
        })
        .collect()
}

fn positive_number(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok().filter(|&number| number >= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, &str)]) -> Attrs {
        Attrs {
            pairs: pairs
                .iter()
                .map(|&(key, value)| (key.into(), value.into()))
                .collect(),
            ..Attrs::default()
        }
    }

    #[test]
    fn defaults() {
        let code = CodeAttrs::from_attrs(&Attrs::default());
        assert!(!code.numbered);
        assert!(!code.plain);
        assert_eq!(code.start, 1);
        assert!(code.highlight.is_empty());
    }

    #[test]
    fn plain_and_numbered_classes() {
        let code = CodeAttrs::from_attrs(&Attrs {
            classes: vec!["plain".into(), "numbered".into(), "other".into()],
            ..Attrs::default()
        });
        assert!(code.plain);
        assert!(code.numbered);
    }

    #[test]
    fn start_valid_and_invalid() {
        for (value, expected) in
            [("10", 10), ("0", 1), ("x", 1), ("18446744073709551616", 1)]
        {
            assert_eq!(
                CodeAttrs::from_attrs(&attrs(&[("start", value)])).start,
                expected
            );
        }
    }

    #[test]
    fn highlight_ranges() {
        let code = CodeAttrs::from_attrs(&attrs(&[("highlight", "1-3,5")]));
        assert_eq!(code.highlight, [1..=3, 5..=5]);
        assert!(!code.numbered);
        let code = CodeAttrs::from_attrs(&attrs(&[
            ("highlight", "1-3, 5"),
            ("lines", "7|9-10"),
        ]));
        assert_eq!(code.highlight, [1..=3, 5..=5, 7..=7, 9..=10]);
    }

    #[test]
    fn lines_alias_numbers_and_highlights() {
        let code = CodeAttrs::from_attrs(&attrs(&[("lines", "1-3|5")]));
        assert!(code.numbered);
        assert_eq!(code.highlight, [1..=3, 5..=5]);
    }

    #[test]
    fn invalid_values_are_ignored() {
        for value in [
            "3-1", "a", "1,,2", "", "0", "1-0", "1-2-3", "+1",
            "18446744073709551616",
        ] {
            let code = CodeAttrs::from_attrs(&attrs(&[
                ("highlight", value),
                ("lines", value),
            ]));
            assert!(!code.numbered, "{value}");
            assert!(code.highlight.is_empty(), "{value}");
        }
        let code = CodeAttrs::from_attrs(&attrs(&[
            ("highlight", "2"),
            ("lines", "3-1"),
        ]));
        assert_eq!(code.highlight.len(), 1);
        assert_eq!(code.highlight[0], 2..=2);
        assert!(!code.numbered);
        let numbered = Attrs {
            classes: vec!["numbered".into()],
            ..attrs(&[("lines", "a")])
        };
        assert!(CodeAttrs::from_attrs(&numbered).numbered);
    }

    #[test]
    fn is_highlighted_uses_displayed_numbers() {
        let code = CodeAttrs::from_attrs(&attrs(&[
            ("start", "10"),
            ("highlight", "11"),
        ]));
        assert_eq!(code.start, 10);
        assert!(!code.is_highlighted(1));
        assert!(!code.is_highlighted(10));
        assert!(code.is_highlighted(11));
        assert!(!code.is_highlighted(12));
    }
}
