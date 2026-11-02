//! Contextual validation and conversion of the merged design map.

use serde_json::{Map, Value};

use super::*;
use crate::error::{Error, Result};

const FURNITURE: &[&str] = &[
    "image", "lines", "right_lines", "rule", "align", "size", "color",
    "weight", "italic", "first", "rest",
];
const STYLE: &[&str] = &[
    "font", "size", "line_height", "color", "weight", "italic", "align",
    "space_before", "space_after", "keep_together", "keep_with_next",
];

pub(crate) fn resolve(table: &Map<String, Value>) -> Result<Design> {
    keys(table, "design", &["fonts", "text", "colors", "page", "styles"])?;
    let mut design =
        Design { declared: !table.is_empty(), ..Design::default() };
    if let Some(value) = table.get("fonts") {
        let map = object(value, "design.fonts")?;
        keys(map, "design.fonts", &["body", "heading", "mono"])?;
        design.fonts = Fonts {
            body: map
                .get("body")
                .map(|v| font(v, "design.fonts.body"))
                .transpose()?,
            heading: map
                .get("heading")
                .map(|v| font(v, "design.fonts.heading"))
                .transpose()?,
            mono: map
                .get("mono")
                .map(|v| font(v, "design.fonts.mono"))
                .transpose()?,
        };
    }
    if let Some(value) = table.get("text") {
        let map = object(value, "design.text")?;
        keys(
            map,
            "design.text",
            &["size", "line_height", "paragraph_spacing", "justify"],
        )?;
        design.text = Text {
            size: length(map, "size", "design.text")?,
            line_height: optional(
                map,
                "line_height",
                "design.text",
                parse::line_height,
            )?,
            paragraph_spacing: length(map, "paragraph_spacing", "design.text")?,
            justify: boolean(map, "justify", "design.text")?,
        };
    }
    if let Some(value) = table.get("colors") {
        let map = object(value, "design.colors")?;
        let names = ["text", "accent", "muted", "link", "background"];
        keys(map, "design.colors", &names)?;
        for name in names {
            let color = color_token(map, name, &mut Vec::new())?;
            match name {
                "text" => design.colors.text = color,
                "accent" => design.colors.accent = color,
                "muted" => design.colors.muted = color,
                "link" => design.colors.link = color,
                "background" => design.colors.background = color,
                _ => unreachable!(),
            }
        }
    }
    if let Some(value) = table.get("page") {
        let map = object(value, "design.page")?;
        keys(
            map,
            "design.page",
            &[
                "paper", "margin", "header_offset", "footer_offset", "print",
                "header", "footer",
            ],
        )?;
        let print = match string(map, "print", "design.page")?.as_deref() {
            None | Some("margin-boxes") => PrintMode::MarginBoxes,
            Some("fixed") => PrintMode::Fixed,
            Some(other) => {
                return bad(
                    "design.page.print",
                    &format!("expected margin-boxes or fixed, got {other:?}"),
                );
            }
        };
        design.page = Page {
            paper: string(map, "paper", "design.page")?,
            margin: map
                .get("margin")
                .map(margin)
                .transpose()?
                .unwrap_or_default(),
            header_offset: length(map, "header_offset", "design.page")?,
            footer_offset: length(map, "footer_offset", "design.page")?,
            print,
            header: map
                .get("header")
                .map(|v| slot(v, true, &design))
                .transpose()?,
            footer: map
                .get("footer")
                .map(|v| slot(v, false, &design))
                .transpose()?,
        };
    }
    if let Some(value) = table.get("styles") {
        let map = object(value, "design.styles")?;
        let mut names = BTreeMap::new();
        // BTreeMap ordering also fixes collision diagnostics independently of
        // the JSON/TOML map's insertion order.
        let sorted: BTreeMap<_, _> = map.iter().collect();
        for (name, value) in sorted {
            let normal = normalise_style_key(name);
            if let Some(previous) = names.insert(normal, name) {
                return bad(
                    "design.styles",
                    &format!(
                        "{previous:?} and {name:?} are the same style name"
                    ),
                );
            }
            let path = format!("design.styles.{name}");
            let map = object(value, &path)?;
            keys(map, &path, STYLE)?;
            let font = string(map, "font", &path)?.map(|family| {
                match family.as_str() {
                    "body" => FontRef::Body,
                    "heading" => FontRef::Heading,
                    "mono" => FontRef::Mono,
                    _ => FontRef::Family(family),
                }
            });
            design.styles.insert(
                name.clone(),
                Style {
                    font,
                    size: length(map, "size", &path)?,
                    line_height: optional(
                        map,
                        "line_height",
                        &path,
                        parse::line_height,
                    )?,
                    color: color(map, "color", &path, &design.colors)?,
                    weight: optional(map, "weight", &path, parse_weight)?,
                    italic: boolean(map, "italic", &path)?,
                    align: alignment(map, "align", &path, false)?,
                    space_before: length(map, "space_before", &path)?,
                    space_after: length(map, "space_after", &path)?,
                    keep_together: boolean(map, "keep_together", &path)?
                        .unwrap_or(false),
                    keep_with_next: boolean(map, "keep_with_next", &path)?
                        .unwrap_or(false),
                },
            );
        }
    }
    Ok(design)
}

fn font(value: &Value, path: &str) -> Result<Font> {
    if let Value::String(family) = value {
        return Ok(Font { family: family.clone(), files: Vec::new() });
    }
    let map = object(value, path)?;
    keys(map, path, &["family", "files"])?;
    let family = string(map, "family", path)?.ok_or_else(|| {
        Error::Config(format!("{path}.family: expected a family name"))
    })?;
    let files = match map.get("files") {
        None => Vec::new(),
        Some(Value::Array(files)) => files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let path = format!("{path}.files[{index}]");
                text(file, &path).map(PathBuf::from)
            })
            .collect::<Result<_>>()?,
        Some(value) => {
            return bad(
                &format!("{path}.files"),
                &format!("expected a list of font paths, got {value}"),
            );
        }
    };
    Ok(Font { family, files })
}

fn color_token(
    map: &Map<String, Value>, name: &str, visiting: &mut Vec<String>,
) -> Result<Option<Color>> {
    let Some(value) = map.get(name) else {
        return Ok(None);
    };
    let path = format!("design.colors.{name}");
    let value = text(value, &path)?;
    if value.starts_with('#') {
        return context(parse_color(value, &Colors::default()), &path)
            .map(Some);
    }
    if visiting.iter().any(|previous| previous == name) {
        return bad(&path, "cyclic colour token reference");
    }
    visiting.push(name.into());
    if !["text", "accent", "muted", "link", "background"].contains(&value) {
        return context(parse_color(value, &Colors::default()), &path)
            .map(Some);
    }
    let result = color_token(map, value, visiting)?;
    visiting.pop();
    result.map(Some).ok_or_else(|| {
        Error::Config(format!("{path}: colour token {value:?} is unset"))
    })
}

fn margin(value: &Value) -> Result<Margin> {
    if let Value::String(value) = value {
        let value = Some(context(parse_length(value), "design.page.margin")?);
        return Ok(Margin {
            top: value,
            right: value,
            bottom: value,
            left: value,
        });
    }
    let path = "design.page.margin";
    let map = object(value, path)?;
    keys(map, path, &["top", "right", "bottom", "left"])?;
    Ok(Margin {
        top: length(map, "top", path)?,
        right: length(map, "right", path)?,
        bottom: length(map, "bottom", path)?,
        left: length(map, "left", path)?,
    })
}

fn slot(value: &Value, header: bool, design: &Design) -> Result<Slot> {
    let path = if header {
        "design.page.header"
    } else {
        "design.page.footer"
    };
    let map = object(value, path)?;
    keys(map, path, FURNITURE)?;
    // Validate the base even when both explicit variants replace it.
    let base = furniture(map, path, header, design)?;
    let variant = |name: &str| -> Result<Option<Furniture>> {
        let Some(value) = map.get(name) else {
            return Ok(base.clone());
        };
        let path = format!("{path}.{name}");
        let map = object(value, &path)?;
        keys(map, &path, &FURNITURE[..FURNITURE.len() - 2])?;
        furniture(map, &path, header, design)
    };
    Ok(Slot { first: variant("first")?, rest: variant("rest")? })
}

fn furniture(
    map: &Map<String, Value>, path: &str, header: bool, design: &Design,
) -> Result<Option<Furniture>> {
    let align = alignment(map, "align", path, true)?.unwrap_or(if header {
        Align::Center
    } else {
        Align::Left
    });
    let size = length(map, "size", path)?.or_else(|| {
        design.text.size.map(
            |size| {
                if header { size } else { Pt(size.0 * 0.75) }
            },
        )
    });
    let color = color(map, "color", path, &design.colors)?.or(if header {
        design.colors.text
    } else {
        design.colors.muted.or(design.colors.text)
    });
    let weight =
        optional(map, "weight", path, parse_weight)?.unwrap_or(Weight(400));
    let italic = boolean(map, "italic", path)?.unwrap_or(false);
    let image = map
        .get("image")
        .map(|v| image(v, &format!("{path}.image")))
        .transpose()?;
    let right_lines = lines(map, "right_lines", path, &design.colors)?;
    let lines = lines(map, "lines", path, &design.colors)?;
    let rule = map
        .get("rule")
        .map(|v| rule(v, &format!("{path}.rule"), header, &design.colors))
        .transpose()?
        .flatten();
    if image.is_none()
        && lines.is_empty()
        && right_lines.is_empty()
        && rule.is_none()
    {
        return Ok(None);
    }
    Ok(Some(Furniture {
        image,
        lines,
        right_lines,
        rule,
        align,
        size,
        color,
        weight,
        italic,
    }))
}

fn image(value: &Value, path: &str) -> Result<FurnitureImage> {
    if let Value::String(value) = value {
        return Ok(FurnitureImage {
            path: value.into(),
            width: None,
            align: None,
        });
    }
    let map = object(value, path)?;
    keys(map, path, &["src", "width", "align"])?;
    let source = string(map, "src", path)?.ok_or_else(|| {
        Error::Config(format!("{path}.src: expected an image path"))
    })?;
    Ok(FurnitureImage {
        path: source.into(),
        width: length(map, "width", path)?,
        align: alignment(map, "align", path, true)?,
    })
}

fn lines(
    map: &Map<String, Value>, key: &str, path: &str, colors: &Colors,
) -> Result<Vec<Line>> {
    let path = format!("{path}.{key}");
    let Some(value) = map.get(key) else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        Error::Config(format!(
            "{path}: expected a list of text lines, got {value}"
        ))
    })?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let path = format!("{path}[{index}]");
            if let Value::String(text) = value {
                return Ok(Line {
                    text: text.clone(),
                    weight: None,
                    italic: None,
                    size: None,
                    color: None,
                });
            }
            let map = object(value, &path)?;
            keys(map, &path, &["text", "weight", "italic", "size", "color"])?;
            let text = string(map, "text", &path)?.ok_or_else(|| {
                Error::Config(format!("{path}.text: expected a string"))
            })?;
            Ok(Line {
                text,
                weight: optional(map, "weight", &path, parse_weight)?,
                italic: boolean(map, "italic", &path)?,
                size: length(map, "size", &path)?,
                color: color(map, "color", &path, colors)?,
            })
        })
        .collect()
}

fn rule(
    value: &Value, path: &str, header: bool, colors: &Colors,
) -> Result<Option<Rule>> {
    if value == &Value::Bool(false) {
        return Ok(None);
    }
    let mut result = Rule {
        position: if header {
            RulePosition::Below
        } else {
            RulePosition::Above
        },
        color: colors.accent.or(colors.text).unwrap_or(Color([0; 3])),
        width: Pt(0.5),
    };
    if value == &Value::Bool(true) {
        return Ok(Some(result));
    }
    let map = object(value, path)?;
    keys(map, path, &["position", "color", "width"])?;
    if let Some(position) = string(map, "position", path)? {
        result.position = match position.as_str() {
            "above" => RulePosition::Above,
            "below" => RulePosition::Below,
            _ => {
                return bad(
                    &format!("{path}.position"),
                    "expected above or below",
                );
            }
        };
    }
    if let Some(value) = color(map, "color", path, colors)? {
        result.color = value;
    }
    if let Some(value) = length(map, "width", path)? {
        result.width = value;
    }
    Ok(Some(result))
}

pub(crate) fn keys(
    map: &Map<String, Value>, path: &str, expected: &[&str],
) -> Result<()> {
    for key in map.keys() {
        if !expected.contains(&key.as_str()) {
            let serde_message = format!(
                "unknown field `{key}`, expected {}",
                expected
                    .iter()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let hint = crate::config::unknown_key_hint(&serde_message)
                .map_or(String::new(), |hint| {
                    format!(" (did you mean {hint}?)")
                });
            return bad(
                &format!("{path}.{key}"),
                &format!("unknown key{hint}"),
            );
        }
    }
    Ok(())
}

fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| {
        Error::Config(format!("{path}: expected a table, got {value}"))
    })
}

fn text<'a>(value: &'a Value, path: &str) -> Result<&'a str> {
    value.as_str().ok_or_else(|| {
        Error::Config(format!("{path}: expected a string, got {value}"))
    })
}

fn optional<T>(
    map: &Map<String, Value>, key: &str, path: &str,
    parse: impl FnOnce(&Value) -> Result<T>,
) -> Result<Option<T>> {
    map.get(key)
        .map(|value| context(parse(value), &format!("{path}.{key}")))
        .transpose()
}

fn string(
    map: &Map<String, Value>, key: &str, path: &str,
) -> Result<Option<String>> {
    map.get(key)
        .map(|v| text(v, &format!("{path}.{key}")).map(str::to_owned))
        .transpose()
}

fn length(
    map: &Map<String, Value>, key: &str, path: &str,
) -> Result<Option<Pt>> {
    optional(map, key, path, |value| match value.as_str() {
        Some(value) => parse_length(value),
        None => Err(Error::Config(format!("{}, got {value}", parse::LENGTH))),
    })
}

fn boolean(
    map: &Map<String, Value>, key: &str, path: &str,
) -> Result<Option<bool>> {
    optional(map, key, path, |value| {
        value.as_bool().ok_or_else(|| {
            Error::Config(format!("expected a boolean, got {value}"))
        })
    })
}

fn color(
    map: &Map<String, Value>, key: &str, path: &str, colors: &Colors,
) -> Result<Option<Color>> {
    map.get(key)
        .map(|value| {
            let path = format!("{path}.{key}");
            context(parse_color(text(value, &path)?, colors), &path)
        })
        .transpose()
}

fn alignment(
    map: &Map<String, Value>, key: &str, path: &str, furniture: bool,
) -> Result<Option<Align>> {
    map.get(key)
        .map(|value| {
            let path = format!("{path}.{key}");
            let align = context(parse_align(text(value, &path)?), &path)?;
            if furniture && align == Align::Justify {
                return bad(&path, "justify is not supported in furniture");
            }
            Ok(align)
        })
        .transpose()
}

fn context<T>(result: Result<T>, path: &str) -> Result<T> {
    result.map_err(|error| match error {
        Error::Config(message) => Error::Config(format!("{path}: {message}")),
        other => other,
    })
}

fn bad<T>(path: &str, message: &str) -> Result<T> {
    Err(Error::Config(format!("{path}: {message}")))
}
