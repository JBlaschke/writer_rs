//! Scalar parsers; the resolver adds the owning key and source location.

use serde_json::Value;

use super::{Align, Color, Colors, LineHeight, Pt, Weight};
use crate::error::{Error, Result};

pub(crate) const LENGTH: &str =
    "expected a length with a unit such as \"11pt\" or \"22mm\"";

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
            _ => None,
        };
        if let Some(color) = color {
            return Ok(color);
        }
        if ["text", "accent", "muted", "link", "background"].contains(&text) {
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

pub(crate) fn line_height(value: &Value) -> Result<LineHeight> {
    match value {
        Value::String(text) => parse_length(text).map(LineHeight::Exact),
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
        assert!(line_height(&json!(0)).is_err());
        assert_eq!(
            line_height(&json!("12.1pt")).unwrap(),
            LineHeight::Exact(Pt(12.1))
        );
    }
}
