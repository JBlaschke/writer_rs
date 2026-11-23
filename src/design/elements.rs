//! Element tables: geometry that no Word paragraph style holds (R13 §4.2).
//! Every key is optional; em lengths are of `text.size`.

use serde::Serialize;
use serde_json::{Map, Value};

use super::resolve::{
    bad, boolean, color, keys, length, object, optional, sides, string,
};
use super::{Color, Colors, LineHeight, Margin, Pt, Weight, parse};
use crate::error::{Error, Result};

/// List geometry (`[design.lists]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Lists {
    /// Space between the items of a tight list.
    pub tight_spacing: Option<Pt>,
    /// Indent of each level.
    pub indent: Option<Pt>,
    /// Space between a marker and its item.
    pub marker_gap: Option<Pt>,
    /// Bullet characters by nesting level, repeating.
    pub bullets: Option<Vec<String>>,
}

/// Table rules and insets (`[design.table]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Tables {
    /// Rule widths.
    pub rules: TableRules,
    /// Rule colour.
    pub rule_color: Option<Color>,
    /// Space left and right of a cell's text.
    pub inset_x: Option<Pt>,
    /// Weight of the header row.
    pub header_weight: Option<Weight>,
    /// Background of the header row.
    pub header_fill: Option<Color>,
    /// How wide a table is.
    pub width: Option<TableWidth>,
}

/// The widths of a table's rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct TableRules {
    /// Above and below the table.
    pub outer: Option<Pt>,
    /// Below the header row.
    pub header: Option<Pt>,
}

/// How wide a table is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TableWidth {
    /// The text width.
    Full,
    /// As wide as its content needs.
    Content,
}

/// Code blocks (`[design.code]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Code {
    /// Block background.
    pub background: Option<Color>,
    /// Space between the background's edge and the code.
    pub padding: Option<Margin>,
    /// Corner radius.
    pub radius: Option<Pt>,
    /// Line pitch.
    pub line_height: Option<LineHeight>,
    /// Syntax highlighting theme.
    pub theme: Option<String>,
}

/// Callout boxes (`[design.callout]`); the colours are `colors.callout`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Callouts {
    /// Width of the kind-coloured left border.
    pub border_width: Option<Pt>,
    /// Tint of the kind colour behind the box, from 0 (none) to 1.
    pub fill: Option<f64>,
    /// Space between the border and the content.
    pub padding: Option<Margin>,
    /// Corner radius.
    pub radius: Option<Pt>,
}

/// Image sizing (`[design.image]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Images {
    /// Pixels per inch of an image without its own resolution.
    pub dpi: Option<f64>,
}

/// Footnotes in paged output (`[design.footnotes]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Footnotes {
    /// The rule above the footnotes.
    pub separator: Option<Separator>,
    /// Space between footnotes.
    pub gap: Option<Pt>,
    /// Space between the body and the separator.
    pub clearance: Option<Pt>,
}

/// The footnote separator rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Separator {
    /// Length as a fraction of the text width.
    pub length: Option<f64>,
    /// Stroke width.
    pub width: Option<Pt>,
    /// Stroke colour.
    pub color: Option<Color>,
}

/// Links (`[design.links]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Links {
    /// Whether links are underlined.
    pub underline: Option<bool>,
    /// Whether printed HTML shows each link's URL after it.
    pub print_url: Option<bool>,
}

/// Resolves the element tables of `table` into `design`; `em` is the body
/// size.
pub(super) fn resolve(
    table: &Map<String, Value>, design: &mut super::Design, em: Pt,
) -> Result<()> {
    let colors = design.colors.clone();
    if let Some(value) = table.get("lists") {
        design.lists = lists(value, em)?;
    }
    if let Some(value) = table.get("table") {
        design.table = tables(value, em, &colors)?;
    }
    if let Some(value) = table.get("code") {
        design.code = code(value, em, &colors)?;
    }
    if let Some(value) = table.get("callout") {
        design.callout = callouts(value, em)?;
    }
    if let Some(value) = table.get("image") {
        let path = "design.image";
        let map = object(value, path)?;
        keys(map, path, &["dpi"])?;
        design.image = Images { dpi: positive(map, "dpi", path)? };
    }
    if let Some(value) = table.get("footnotes") {
        design.footnotes = footnotes(value, em, &colors)?;
    }
    if let Some(value) = table.get("links") {
        let path = "design.links";
        let map = object(value, path)?;
        keys(map, path, &["underline", "print_url"])?;
        design.links = Links {
            underline: boolean(map, "underline", path)?,
            print_url: boolean(map, "print_url", path)?,
        };
    }
    Ok(())
}

fn lists(value: &Value, em: Pt) -> Result<Lists> {
    let path = "design.lists";
    let map = object(value, path)?;
    keys(map, path, &["tight_spacing", "indent", "marker_gap", "bullets"])?;
    let bullets = map
        .get("bullets")
        .map(|value| {
            let bad_list = || {
                Error::Config(format!(
                    "{path}.bullets: expected a list of marker strings, got {value}"
                ))
            };
            value
                .as_array()
                .ok_or_else(bad_list)?
                .iter()
                .map(|marker| marker.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(bad_list)
        })
        .transpose()?;
    Ok(Lists {
        tight_spacing: length(map, "tight_spacing", path, em)?,
        indent: length(map, "indent", path, em)?,
        marker_gap: length(map, "marker_gap", path, em)?,
        bullets,
    })
}

fn tables(value: &Value, em: Pt, colors: &Colors) -> Result<Tables> {
    let path = "design.table";
    let map = object(value, path)?;
    keys(
        map,
        path,
        &[
            "rules", "rule_color", "inset_x", "header_weight", "header_fill",
            "width",
        ],
    )?;
    let rules = match map.get("rules") {
        Some(value) => {
            let path = format!("{path}.rules");
            let map = object(value, &path)?;
            keys(map, &path, &["outer", "header"])?;
            TableRules {
                outer: length(map, "outer", &path, em)?,
                header: length(map, "header", &path, em)?,
            }
        }
        None => TableRules::default(),
    };
    let width = match string(map, "width", path)?.as_deref() {
        None => None,
        Some("full") => Some(TableWidth::Full),
        Some("content") => Some(TableWidth::Content),
        Some(other) => {
            return bad(
                &format!("{path}.width"),
                &format!("expected full or content, got {other:?}"),
            );
        }
    };
    Ok(Tables {
        rules,
        rule_color: color(map, "rule_color", path, colors)?,
        inset_x: length(map, "inset_x", path, em)?,
        header_weight: optional(
            map,
            "header_weight",
            path,
            parse::parse_weight,
        )?,
        header_fill: color(map, "header_fill", path, colors)?,
        width,
    })
}

fn code(value: &Value, em: Pt, colors: &Colors) -> Result<Code> {
    let path = "design.code";
    let map = object(value, path)?;
    keys(
        map,
        path,
        &["background", "padding", "radius", "line_height", "theme"],
    )?;
    Ok(Code {
        background: color(map, "background", path, colors)?,
        padding: padding(map, path, em)?,
        radius: length(map, "radius", path, em)?,
        line_height: optional(map, "line_height", path, |value| {
            parse::line_height(value, em)
        })?,
        theme: string(map, "theme", path)?,
    })
}

fn callouts(value: &Value, em: Pt) -> Result<Callouts> {
    let path = "design.callout";
    let map = object(value, path)?;
    keys(map, path, &["border_width", "fill", "padding", "radius"])?;
    let fill = optional(map, "fill", path, |value| {
        value.as_f64().filter(|tint| (0.0..=1.0).contains(tint)).ok_or_else(
            || {
                Error::Config(format!(
                    "expected a tint from 0 to 1, got {value}"
                ))
            },
        )
    })?;
    Ok(Callouts {
        border_width: length(map, "border_width", path, em)?,
        fill,
        padding: padding(map, path, em)?,
        radius: length(map, "radius", path, em)?,
    })
}

fn footnotes(value: &Value, em: Pt, colors: &Colors) -> Result<Footnotes> {
    let path = "design.footnotes";
    let map = object(value, path)?;
    keys(map, path, &["separator", "gap", "clearance"])?;
    let separator = match map.get("separator") {
        Some(value) => {
            let path = format!("{path}.separator");
            let map = object(value, &path)?;
            keys(map, &path, &["length", "width", "color"])?;
            Some(Separator {
                length: optional(map, "length", &path, fraction)?,
                width: length(map, "width", &path, em)?,
                color: color(map, "color", &path, colors)?,
            })
        }
        None => None,
    };
    Ok(Footnotes {
        separator,
        gap: length(map, "gap", path, em)?,
        clearance: length(map, "clearance", path, em)?,
    })
}

fn padding(
    map: &Map<String, Value>, path: &str, em: Pt,
) -> Result<Option<Margin>> {
    map.get("padding")
        .map(|value| sides(value, &format!("{path}.padding"), em))
        .transpose()
}

/// A percentage such as `"33%"`, as a fraction from 0 to 1.
fn fraction(value: &Value) -> Result<f64> {
    value
        .as_str()
        .and_then(|text| text.trim().strip_suffix('%'))
        .and_then(|number| number.trim().parse::<f64>().ok())
        .filter(|percent| (0.0..=100.0).contains(percent))
        .map(|percent| percent / 100.0)
        .ok_or_else(|| {
            Error::Config(format!(
                "expected a percentage from 0% to 100%, got {value}"
            ))
        })
}

pub(super) fn positive(
    map: &Map<String, Value>, key: &str, path: &str,
) -> Result<Option<f64>> {
    optional(map, key, path, |value| {
        value
            .as_f64()
            .filter(|number| number.is_finite() && *number > 0.0)
            .ok_or_else(|| {
                Error::Config(format!(
                    "expected a positive number, got {value}"
                ))
            })
    })
}
