//! Scalar parsers; the resolver adds the owning key and source location.

use serde_json::Value;

use super::{Align, Color, Colors, LineHeight, Pt, Weight};
use crate::error::{Error, Result};

pub(crate) const LENGTH: &str =
    "expected a length with a unit such as \"11pt\" or \"22mm\"";

/// The colour tokens, in resolution order: `[design.colors]` without
/// `callout`.
pub(crate) const TOKENS: [&str; 9] = [
    "text", "accent", "muted", "link", "background", "border", "code_bg",
    "highlight", "error",
];

/// Parses a non-negative finite length in pt, mm, cm, or in.
pub fn parse_length(text: &str) -> Result<Pt> {
    let text = text.trim();
    let units =
        [("pt", 1.0), ("mm", 72.0 / 25.4), ("cm", 720.0 / 25.4), ("in", 72.0)];
    for (unit, factor) in units {
        if let Some(number) = text.strip_suffix(unit)
            && let Ok(number) = number.trim().parse::<f64>()
            && number.is_finite()
            && number >= 0.0
            && (number * factor).is_finite()
        {
            return Ok(Pt(number * factor));
        }
    }
    Err(Error::Config(format!("{LENGTH}, got {text:?}")))
}

/// Parses a non-negative length in pt, mm, cm, in, or em, where one em is
/// `em`. The error is [`parse_length`]'s.
pub(crate) fn parse_length_em(text: &str, em: Pt) -> Result<Pt> {
    if let Some(number) = text.trim().strip_suffix("em")
        && let Ok(number) = number.trim().parse::<f64>()
        && number.is_finite()
        && number >= 0.0
        && (number * em.0).is_finite()
    {
        return Ok(Pt(number * em.0));
    }
    parse_length(text)
}

/// Parses a length that may be negative, such as a frame's `offset`; one
/// em is `em`.
pub(crate) fn parse_signed_length(text: &str, em: Pt) -> Result<Pt> {
    match text.trim().strip_prefix('-') {
        Some(rest) => parse_length_em(rest, em).map(|length| Pt(-length.0)),
        None => parse_length_em(text, em),
    }
    .map_err(|_| {
        Error::Config(format!(
            "expected a length with a unit, such as \"4mm\" or \"-9mm\", got {text:?}"
        ))
    })
}

/// Parses a hex colour or an already resolved colour token.
pub fn parse_color(text: &str, colors: &Colors) -> Result<Color> {
    if let Some(hex) = text.strip_prefix('#') {
        let hex = match hex.len() {
            3 if hex.is_ascii() => hex.chars().flat_map(|c| [c, c]).collect(),
            6 => hex.to_owned(),
            _ => String::new(),
        };
        if hex.len() == 6 && hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Ok(Color([
                u8::from_str_radix(&hex[0..2], 16).unwrap(),
                u8::from_str_radix(&hex[2..4], 16).unwrap(),
                u8::from_str_radix(&hex[4..6], 16).unwrap(),
            ]));
        }
    } else {
        let color = match text {
            "text" => colors.text,
            "accent" => colors.accent,
            "muted" => colors.muted,
            "link" => colors.link,
            "background" => colors.background,
            "border" => colors.border,
            "code_bg" => colors.code_bg,
            "highlight" => colors.highlight,
            "error" => colors.error,
            _ => None,
        };
        if let Some(color) = color {
            return Ok(color);
        }
        if TOKENS.contains(&text) {
            return Err(Error::Config(format!(
                "colour token {text:?} is unset"
            )));
        }
    }
    Err(Error::Config(format!(
        "expected a hex colour or a declared colour token, got {text:?}"
    )))
}

/// Parses a documented name or integer weight (100–900, multiples of 100).
pub fn parse_weight(value: &Value) -> Result<Weight> {
    let weight = match value {
        Value::String(name) => match name.as_str() {
            "normal" => Some(400),
            "medium" => Some(500),
            "semibold" => Some(600),
            "bold" => Some(700),
            _ => None,
        },
        _ => value.as_u64().filter(|n| (100..=900).contains(n) && n % 100 == 0),
    };
    weight.map(|n| Weight(n as u16)).ok_or_else(|| Error::Config(format!(
        "expected normal, medium, semibold, bold, or a multiple of 100 from 100 to 900, got {value}"
    )))
}

/// Parses a paragraph alignment; furniture separately rejects `justify`.
pub fn parse_align(text: &str) -> Result<Align> {
    match text {
        "left" => Ok(Align::Left),
        "center" => Ok(Align::Center),
        "right" => Ok(Align::Right),
        "justify" => Ok(Align::Justify),
        _ => Err(Error::Config(format!(
            "expected left, center, right, or justify, got {text:?}"
        ))),
    }
}

/// A multiple, or an exact length where one em is `em`.
pub(crate) fn line_height(value: &Value, em: Pt) -> Result<LineHeight> {
    match value {
        Value::String(text) => parse_length_em(text, em).map(LineHeight::Exact),
        _ => value.as_f64().filter(|n| n.is_finite() && *n > 0.0)
            .map(LineHeight::Multiple).ok_or_else(|| Error::Config(format!(
                "expected a positive line-height multiple or a length, got {value}"
            ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lengths_are_finite_nonnegative_and_have_supported_units() {
        for (input, points) in [
            ("1in", 72.0),
            ("25.4mm", 72.0),
            ("2.54cm", 72.0),
            ("11pt", 11.0),
            ("0pt", 0.0),
        ] {
            assert!((parse_length(input).unwrap().0 - points).abs() < 1e-10);
        }
        for input in ["11", "12px", "-1pt", "NaNpt", "infpt", "1e999in"] {
            assert!(
                parse_length(input).unwrap_err().to_string().contains(LENGTH)
            );
        }
    }

    #[test]
    fn colors_and_scalar_constraints() {
        assert_eq!(
            parse_color("#aBc", &Colors::default()).unwrap(),
            Color([170, 187, 204])
        );
        assert_eq!(
            parse_color("#242424", &Colors::default()).unwrap(),
            Color([36; 3])
        );
        assert!(parse_color("red", &Colors::default()).is_err());
        assert!(parse_color("accent", &Colors::default()).is_err());
        assert!(parse_color("#ééé", &Colors::default()).is_err());
        for value in [json!(100), json!(900), json!("semibold")] {
            assert!(parse_weight(&value).is_ok());
        }
        for value in [json!(50), json!(550), json!(1000), json!(400.0)] {
            assert!(parse_weight(&value).is_err());
        }
        let em = Pt(10.0);
        assert!(line_height(&json!(0), em).is_err());
        assert_eq!(parse_signed_length("-1in", em).unwrap(), Pt(-72.0));
        assert_eq!(parse_signed_length("2pt", em).unwrap(), Pt(2.0));
        for input in ["--1pt", "-", "-12px", "1pt-"] {
            assert!(parse_signed_length(input, em).is_err(), "{input}");
        }
        assert_eq!(
            line_height(&json!("12.1pt"), em).unwrap(),
            LineHeight::Exact(Pt(12.1))
        );
    }

    #[test]
    fn em_lengths_are_of_the_given_size() {
        let em = Pt(12.0);
        for (input, points) in
            [("1.5em", 18.0), ("0em", 0.0), (" 2 em ", 24.0), ("3pt", 3.0)]
        {
            assert_eq!(parse_length_em(input, em).unwrap(), Pt(points));
        }
        assert_eq!(parse_signed_length("-0.5em", em).unwrap(), Pt(-6.0));
        assert_eq!(
            line_height(&json!("1.25em"), em).unwrap(),
            LineHeight::Exact(Pt(15.0))
        );
        for input in ["-1em", "1rem", "em", "NaNem", "1e999em"] {
            let error = parse_length_em(input, em).unwrap_err().to_string();
            assert!(error.contains(LENGTH), "{input}: {error}");
        }
        // parse_length itself keeps rejecting em.
        assert!(parse_length("1em").is_err());
        assert_eq!(
            parse_color("code_bg", &Colors::default()).unwrap_err().to_string(),
            "config: colour token \"code_bg\" is unset"
        );
    }
}
