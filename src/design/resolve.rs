//! Contextual validation and conversion of the merged design map.

use serde_json::{Map, Value};

use super::parse::TOKENS;
use super::*;
use crate::error::{Error, Result};

const FURNITURE: &[&str] = &[
    "image", "lines", "right_lines", "rule", "align", "size", "color",
    "weight", "italic", "first", "rest",
];
const STYLE: &[&str] = &[
    "font", "size", "line_height", "color", "weight", "italic", "align",
    "space_before", "space_after", "keep_together", "keep_with_next", "frame",
    "case", "letter_spacing", "indent", "border", "fill", "padding",
];
const TOP: &[&str] = &[
    "fonts", "text", "colors", "page", "styles", "lists", "table", "code",
    "callout", "image", "footnotes", "links",
];
const TEXT: &[&str] = &[
    "size", "line_height", "paragraph_spacing", "justify", "hyphenate",
    "widows", "orphans", "kerning", "ligatures", "measure", "screen_scale",
];

/// Resolves a merged table; em lengths are of the style's size inside a
/// style, else of `text.size`, else of layer 0's `text.size`.
pub(crate) fn resolve(table: &Map<String, Value>) -> Result<Design> {
    keys(table, "design", TOP)?;
    let declared = !table.is_empty();
    let mut design =
        Design { declared, user_declared: declared, ..Design::default() };
    if let Some(value) = table.get("fonts") {
        design.fonts = fonts(value)?;
    }
    if let Some(value) = table.get("text") {
        design.text = text_table(value)?;
    }
    let body = design.text.size.unwrap_or_else(defaults::text_size);
    if let Some(value) = table.get("colors") {
        design.colors = colors(value)?;
    }
    if let Some(value) = table.get("page") {
        design.page = page(value, &design, body)?;
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
            let style = style(name, value, &design.colors, body)?;
            design.styles.insert(name.clone(), style);
        }
    }
    super::elements::resolve(table, &mut design, body)?;
    Ok(design)
}

fn fonts(value: &Value) -> Result<Fonts> {
    let map = object(value, "design.fonts")?;
    keys(map, "design.fonts", &["body", "heading", "mono", "math"])?;
    let role = |name: &str| {
        map.get(name)
            .map(|value| font(value, &format!("design.fonts.{name}")))
            .transpose()
    };
    Ok(Fonts {
        body: role("body")?,
        heading: role("heading")?,
        mono: role("mono")?,
        math: role("math")?,
    })
}

fn text_table(value: &Value) -> Result<Text> {
    let path = "design.text";
    let map = object(value, path)?;
    keys(map, path, TEXT)?;
    if let Some(size) = map.get("size").and_then(Value::as_str)
        && size.trim().ends_with("em")
    {
        return bad(
            "design.text.size",
            &format!(
                "em is relative to text.size; expected a length with an \
                 absolute unit such as \"11pt\", got {size:?}"
            ),
        );
    }
    let size = length(map, "size", path, Pt(0.0))?;
    let em = size.unwrap_or_else(defaults::text_size);
    let lines = |key: &str| {
        optional(map, key, path, |value| {
            value
                .as_u64()
                .filter(|lines| *lines >= 1)
                .and_then(|lines| u8::try_from(lines).ok())
                .ok_or_else(|| {
                    Error::Config(format!(
                        "expected a number of lines from 1 to 255, got {value}"
                    ))
                })
        })
    };
    Ok(Text {
        size,
        line_height: optional(map, "line_height", path, |value| {
            parse::line_height(value, em)
        })?,
        paragraph_spacing: length(map, "paragraph_spacing", path, em)?,
        justify: boolean(map, "justify", path)?,
        hyphenate: boolean(map, "hyphenate", path)?,
        widows: lines("widows")?,
        orphans: lines("orphans")?,
        kerning: boolean(map, "kerning", path)?,
        ligatures: boolean(map, "ligatures", path)?,
        measure: length(map, "measure", path, em)?,
        screen_scale: super::elements::positive(map, "screen_scale", path)?,
    })
}

fn colors(value: &Value) -> Result<Colors> {
    let path = "design.colors";
    let map = object(value, path)?;
    let mut names = TOKENS.to_vec();
    names.push("callout");
    keys(map, path, &names)?;
    let mut colors = Colors::default();
    for name in TOKENS {
        let color = color_token(map, name, &mut Vec::new())?;
        match name {
            "text" => colors.text = color,
            "accent" => colors.accent = color,
            "muted" => colors.muted = color,
            "link" => colors.link = color,
            "background" => colors.background = color,
            "border" => colors.border = color,
            "code_bg" => colors.code_bg = color,
            "highlight" => colors.highlight = color,
            "error" => colors.error = color,
            _ => unreachable!(),
        }
    }
    if let Some(value) = map.get("callout") {
        let path = "design.colors.callout";
        let map = object(value, path)?;
        keys(
            map,
            path,
            &["note", "tip", "important", "warning", "caution", "other"],
        )?;
        colors.callout = CalloutColors {
            note: color(map, "note", path, &colors)?,
            tip: color(map, "tip", path, &colors)?,
            important: color(map, "important", path, &colors)?,
            warning: color(map, "warning", path, &colors)?,
            caution: color(map, "caution", path, &colors)?,
            other: color(map, "other", path, &colors)?,
        };
    }
    Ok(colors)
}

fn page(value: &Value, design: &Design, em: Pt) -> Result<Page> {
    let map = object(value, "design.page")?;
    keys(
        map,
        "design.page",
        &[
            "paper", "margin", "header_offset", "footer_offset", "print",
            "columns", "background", "header", "footer",
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
    Ok(Page {
        paper: string(map, "paper", "design.page")?,
        margin: map
            .get("margin")
            .map(|value| sides(value, "design.page.margin", em))
            .transpose()?
            .unwrap_or_default(),
        header_offset: length(map, "header_offset", "design.page", em)?,
        footer_offset: length(map, "footer_offset", "design.page", em)?,
        print,
        columns: map.get("columns").map(|v| columns(v, em)).transpose()?,
        background: map
            .get("background")
            .map(background)
            .transpose()?
            .flatten(),
        header: map
            .get("header")
            .map(|v| slot(v, true, design, em))
            .transpose()?,
        footer: map
            .get("footer")
            .map(|v| slot(v, false, design, em))
            .transpose()?,
    })
}

/// One named style; `body` is the body size, what the style's own `size`
/// is relative to. Its other em lengths are of the style's size.
fn style(
    name: &str, value: &Value, colors: &Colors, body: Pt,
) -> Result<Style> {
    let path = format!("design.styles.{name}");
    let map = object(value, &path)?;
    keys(map, &path, STYLE)?;
    let font =
        string(map, "font", &path)?.map(|family| match family.as_str() {
            "body" => FontRef::Body,
            "heading" => FontRef::Heading,
            "mono" => FontRef::Mono,
            _ => FontRef::Family(family),
        });
    let size = length(map, "size", &path, body)?;
    let em = size.unwrap_or(body);
    let case = match string(map, "case", &path)?.as_deref() {
        None => None,
        Some("upper") => Some(Case::Upper),
        Some("smallcaps") => Some(Case::SmallCaps),
        Some(other) => {
            return bad(
                &format!("{path}.case"),
                &format!("expected upper or smallcaps, got {other:?}"),
            );
        }
    };
    Ok(Style {
        font,
        size,
        line_height: optional(map, "line_height", &path, |value| {
            parse::line_height(value, em)
        })?,
        color: color(map, "color", &path, colors)?,
        weight: optional(map, "weight", &path, parse_weight)?,
        italic: boolean(map, "italic", &path)?,
        align: alignment(map, "align", &path, false)?,
        space_before: length(map, "space_before", &path, em)?,
        space_after: length(map, "space_after", &path, em)?,
        keep_together: boolean(map, "keep_together", &path)?.unwrap_or(false),
        keep_with_next: boolean(map, "keep_with_next", &path)?.unwrap_or(false),
        frame: map
            .get("frame")
            .map(|value| frame(value, name, &path, em))
            .transpose()?,
        case,
        letter_spacing: signed_length(map, "letter_spacing", &path, em)?,
        indent: map
            .get("indent")
            .map(|value| indent(value, &format!("{path}.indent"), em))
            .transpose()?,
        border: map
            .get("border")
            .map(|value| border(value, &format!("{path}.border"), colors, em))
            .transpose()?,
        fill: color(map, "fill", &path, colors)?,
        padding: map
            .get("padding")
            .map(|value| sides(value, &format!("{path}.padding"), em))
            .transpose()?,
    })
}

/// One length for both sides, or a table with `left` and `right`.
fn indent(value: &Value, path: &str, em: Pt) -> Result<Indent> {
    if let Value::String(text) = value {
        let both = Some(context(parse::parse_length_em(text, em), path)?);
        return Ok(Indent { left: both, right: both });
    }
    let map = object(value, path)?;
    keys(map, path, &["left", "right"])?;
    Ok(Indent {
        left: length(map, "left", path, em)?,
        right: length(map, "right", path, em)?,
    })
}

fn border(
    value: &Value, path: &str, colors: &Colors, em: Pt,
) -> Result<Border> {
    let map = object(value, path)?;
    keys(map, path, &["side", "width", "color", "gap"])?;
    let side = match string(map, "side", path)?.as_deref() {
        None | Some("all") => BorderSide::All,
        Some("top") => BorderSide::Top,
        Some("bottom") => BorderSide::Bottom,
        Some("left") => BorderSide::Left,
        Some("right") => BorderSide::Right,
        Some(other) => {
            return bad(
                &format!("{path}.side"),
                &format!(
                    "expected top, bottom, left, right or all, got {other:?}"
                ),
            );
        }
    };
    Ok(Border {
        side,
        width: length(map, "width", path, em)?,
        color: color(map, "color", path, colors)?,
        gap: length(map, "gap", path, em)?,
    })
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
    if !TOKENS.contains(&value) {
        return context(parse_color(value, &Colors::default()), &path)
            .map(Some);
    }
    let result = color_token(map, value, visiting)?;
    visiting.pop();
    result.map(Some).ok_or_else(|| {
        Error::Config(format!("{path}: colour token {value:?} is unset"))
    })
}

/// One length for every side, or a table of optional sides: a page margin
/// or a padding.
pub(super) fn sides(value: &Value, path: &str, em: Pt) -> Result<Margin> {
    if let Value::String(value) = value {
        let value = Some(context(parse::parse_length_em(value, em), path)?);
        return Ok(Margin {
            top: value,
            right: value,
            bottom: value,
            left: value,
        });
    }
    let map = object(value, path)?;
    keys(map, path, &["top", "right", "bottom", "left"])?;
    Ok(Margin {
        top: length(map, "top", path, em)?,
        right: length(map, "right", path, em)?,
        bottom: length(map, "bottom", path, em)?,
        left: length(map, "left", path, em)?,
    })
}

/// A bare count keeps the default gap and starts at the body.
fn columns(value: &Value, em: Pt) -> Result<Columns> {
    let path = "design.page.columns";
    if !value.is_object() {
        return Ok(Columns {
            count: column_count(value, path)?,
            gap: None,
            start: ColumnStart::Body,
        });
    }
    let map = object(value, path)?;
    keys(map, path, &["count", "gap", "start"])?;
    let count = match map.get("count") {
        Some(count) => column_count(count, &format!("{path}.count"))?,
        None => {
            return bad(&format!("{path}.count"), "expected a column count");
        }
    };
    let start = match string(map, "start", path)?.as_deref() {
        None | Some("body") => ColumnStart::Body,
        Some("after-rule") => ColumnStart::AfterRule,
        Some(other) => {
            return bad(
                &format!("{path}.start"),
                &format!("expected body or after-rule, got {other:?}"),
            );
        }
    };
    Ok(Columns { count, gap: length(map, "gap", path, em)?, start })
}

fn frame(value: &Value, name: &str, path: &str, em: Pt) -> Result<Frame> {
    let path = format!("{path}.frame");
    // Frames hold a div's paragraphs. A style that writer selects for its own
    // paragraphs is the base of others or recurs on every page in Word.
    if crate::docx::styles::writer_selects(name) {
        return bad(
            &path,
            &format!(
                "only a div style can have a frame; writer uses {name:?} \
                 for its own paragraphs"
            ),
        );
    }
    let map = object(value, &path)?;
    keys(map, &path, &["width", "align", "gap", "anchor", "offset"])?;
    let align = match string(map, "align", &path)?.as_deref() {
        None | Some("left") => Align::Left,
        Some("center") => Align::Center,
        Some("right") => Align::Right,
        Some(other) => {
            return bad(
                &format!("{path}.align"),
                &format!(
                    "expected left, center or right for a frame, got {other:?}"
                ),
            );
        }
    };
    let anchor = match string(map, "anchor", &path)?.as_deref() {
        None | Some("div") => FrameAnchor::Div,
        Some("title") => FrameAnchor::Title,
        Some(other) => {
            return bad(
                &format!("{path}.anchor"),
                &format!("expected div or title, got {other:?}"),
            );
        }
    };
    let offset = signed_length(map, "offset", &path, em)?;
    Ok(Frame {
        width: length(map, "width", &path, em)?,
        align,
        gap: length(map, "gap", &path, em)?,
        anchor,
        offset: offset.unwrap_or(Pt(0.0)),
    })
}

fn column_count(value: &Value, path: &str) -> Result<u8> {
    value
        .as_u64()
        .filter(|count| (1..=16).contains(count))
        .and_then(|count| u8::try_from(count).ok())
        .ok_or_else(|| {
            Error::Config(format!(
                "{path}: expected a column count from 1 to 16, got {value}"
            ))
        })
}

fn slot(value: &Value, header: bool, design: &Design, em: Pt) -> Result<Slot> {
    let path = if header {
        "design.page.header"
    } else {
        "design.page.footer"
    };
    let map = object(value, path)?;
    keys(map, path, FURNITURE)?;
    // Validate the base even when both explicit variants replace it.
    let base = furniture(map, path, header, design, em)?;
    let variant = |name: &str| -> Result<Option<Furniture>> {
        let Some(value) = map.get(name) else {
            return Ok(base.clone());
        };
        let path = format!("{path}.{name}");
        let map = object(value, &path)?;
        keys(map, &path, &FURNITURE[..FURNITURE.len() - 2])?;
        furniture(map, &path, header, design, em)
    };
    Ok(Slot { first: variant("first")?, rest: variant("rest")? })
}

fn furniture(
    map: &Map<String, Value>, path: &str, header: bool, design: &Design, em: Pt,
) -> Result<Option<Furniture>> {
    let align = alignment(map, "align", path, true)?.unwrap_or(if header {
        Align::Center
    } else {
        Align::Left
    });
    let size = length(map, "size", path, em)?.or_else(|| {
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
        .map(|v| {
            let spacing = design.text.paragraph_spacing;
            image(v, &format!("{path}.image"), spacing, em)
        })
        .transpose()?;
    let right_lines = lines(map, "right_lines", path, &design.colors, em)?;
    let lines = lines(map, "lines", path, &design.colors, em)?;
    let rule = map
        .get("rule")
        .map(|v| rule(v, &format!("{path}.rule"), header, &design.colors, em))
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

/// An empty table is an explicit "no background"; anything else needs an image.
fn background(value: &Value) -> Result<Option<Background>> {
    let path = "design.page.background";
    let map = object(value, path)?;
    keys(map, path, &["image", "pages"])?;
    if map.is_empty() {
        return Ok(None);
    }
    let image = map.get("image").ok_or_else(|| {
        Error::Config(format!("{path}.image: expected an image path or table"))
    })?;
    let image = background_image(image, &format!("{path}.image"))?;
    let pages = match string(map, "pages", path)?.as_deref() {
        None | Some("first") => BackgroundPages::First,
        Some("all") => BackgroundPages::All,
        Some(other) => {
            return bad(
                &format!("{path}.pages"),
                &format!("expected first or all, got {other:?}"),
            );
        }
    };
    Ok(Some(Background { image, pages }))
}

fn background_image(value: &Value, path: &str) -> Result<BackgroundImage> {
    if let Value::String(value) = value {
        return Ok(BackgroundImage {
            path: value.into(),
            fit: Fit::Cover,
            align: BackgroundAlign::default(),
        });
    }
    let map = object(value, path)?;
    keys(map, path, &["src", "fit", "align"])?;
    let source = string(map, "src", path)?.ok_or_else(|| {
        Error::Config(format!("{path}.src: expected an image path"))
    })?;
    let fit = match string(map, "fit", path)?.as_deref() {
        None | Some("cover") => Fit::Cover,
        Some("contain") => Fit::Contain,
        Some("stretch") => Fit::Stretch,
        Some(other) => {
            return bad(
                &format!("{path}.fit"),
                &format!("expected cover, contain or stretch, got {other:?}"),
            );
        }
    };
    let align = match string(map, "align", path)? {
        Some(text) => {
            context(background_align(&text), &format!("{path}.align"))?
        }
        None => BackgroundAlign::default(),
    };
    Ok(BackgroundImage { path: source.into(), fit, align })
}

/// One or two of `top`, `bottom`, `left`, `right`, `center`, in any order.
fn background_align(text: &str) -> Result<BackgroundAlign> {
    let invalid = || {
        Error::Config(format!(
            "expected one or two of top, bottom, left, right, center, got {text:?}"
        ))
    };
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || words.len() > 2 {
        return Err(invalid());
    }
    let mut horizontal = None;
    let mut vertical = None;
    for word in words {
        let repeated = match word {
            "left" => horizontal.replace(Align::Left).is_some(),
            "right" => horizontal.replace(Align::Right).is_some(),
            "top" => vertical.replace(VerticalAlign::Top).is_some(),
            "bottom" => vertical.replace(VerticalAlign::Bottom).is_some(),
            "center" => false,
            _ => return Err(invalid()),
        };
        if repeated {
            return Err(invalid());
        }
    }
    Ok(BackgroundAlign {
        horizontal: horizontal.unwrap_or(Align::Center),
        vertical: vertical.unwrap_or(VerticalAlign::Center),
    })
}

// `spacing` is the design's paragraph spacing, the default gap between the
// image and the lines below it so that they are spaced like two paragraphs.
fn image(
    value: &Value, path: &str, spacing: Option<Pt>, em: Pt,
) -> Result<FurnitureImage> {
    if let Value::String(value) = value {
        return Ok(FurnitureImage {
            path: value.into(),
            width: None,
            align: None,
            gap: spacing,
        });
    }
    let map = object(value, path)?;
    keys(map, path, &["src", "width", "align", "gap"])?;
    let source = string(map, "src", path)?.ok_or_else(|| {
        Error::Config(format!("{path}.src: expected an image path"))
    })?;
    Ok(FurnitureImage {
        path: source.into(),
        width: length(map, "width", path, em)?,
        align: alignment(map, "align", path, true)?,
        gap: length(map, "gap", path, em)?.or(spacing),
    })
}

fn lines(
    map: &Map<String, Value>, key: &str, path: &str, colors: &Colors, em: Pt,
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
                size: length(map, "size", &path, em)?,
                color: color(map, "color", &path, colors)?,
            })
        })
        .collect()
}

fn rule(
    value: &Value, path: &str, header: bool, colors: &Colors, em: Pt,
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
    if let Some(value) = length(map, "width", path, em)? {
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

pub(super) fn object<'a>(
    value: &'a Value, path: &str,
) -> Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| {
        Error::Config(format!("{path}: expected a table, got {value}"))
    })
}

fn text<'a>(value: &'a Value, path: &str) -> Result<&'a str> {
    value.as_str().ok_or_else(|| {
        Error::Config(format!("{path}: expected a string, got {value}"))
    })
}

pub(super) fn optional<T>(
    map: &Map<String, Value>, key: &str, path: &str,
    parse: impl FnOnce(&Value) -> Result<T>,
) -> Result<Option<T>> {
    map.get(key)
        .map(|value| context(parse(value), &format!("{path}.{key}")))
        .transpose()
}

pub(super) fn string(
    map: &Map<String, Value>, key: &str, path: &str,
) -> Result<Option<String>> {
    map.get(key)
        .map(|v| text(v, &format!("{path}.{key}")).map(str::to_owned))
        .transpose()
}

/// A non-negative length; `em` is what one em is.
pub(super) fn length(
    map: &Map<String, Value>, key: &str, path: &str, em: Pt,
) -> Result<Option<Pt>> {
    optional(map, key, path, |value| match value.as_str() {
        Some(value) => parse::parse_length_em(value, em),
        None => Err(Error::Config(format!("{}, got {value}", parse::LENGTH))),
    })
}

/// A length that may be negative; `em` is what one em is.
fn signed_length(
    map: &Map<String, Value>, key: &str, path: &str, em: Pt,
) -> Result<Option<Pt>> {
    optional(map, key, path, |value| match value.as_str() {
        Some(text) => parse::parse_signed_length(text, em),
        None => Err(Error::Config(format!(
            "expected a length with a unit, such as \"4mm\" or \"-9mm\", got {value}"
        ))),
    })
}

pub(super) fn boolean(
    map: &Map<String, Value>, key: &str, path: &str,
) -> Result<Option<bool>> {
    optional(map, key, path, |value| {
        value.as_bool().ok_or_else(|| {
            Error::Config(format!("expected a boolean, got {value}"))
        })
    })
}

pub(super) fn color(
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

pub(super) fn bad<T>(path: &str, message: &str) -> Result<T> {
    Err(Error::Config(format!("{path}: {message}")))
}
