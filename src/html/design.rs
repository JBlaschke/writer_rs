//! CSS generated from the shared design, around the theme's own entries.

use std::fmt::Write as _;
use std::path::Path;

use typst::text::{FontInfo, FontStyle};

use super::styles::Targets;
use crate::assets::emit::{Emitter, OutputMode};
use crate::assets::registry::AssetRegistry;
use crate::design::{Color, Design, LineHeight, Pt};
use crate::error::Result;

/// Font faces and CSS tokens that form the theme's optional baseline.
pub fn tokens_css(
    design: &Design, targets: &mut Targets<'_>,
) -> Result<String> {
    if !design.declared {
        return Ok(String::new());
    }
    let mut css = String::from("/* design: tokens */\n");
    for font in [&design.fonts.body, &design.fonts.heading, &design.fonts.mono]
        .into_iter()
        .flatten()
    {
        for file in &font.files {
            let bytes = std::fs::read(file)?;
            let url = file_url(file, targets)?;
            let format = match file.extension().and_then(|ext| ext.to_str()) {
                Some(ext) if ext.eq_ignore_ascii_case("ttf") => "truetype",
                Some(ext) if ext.eq_ignore_ascii_case("otf") => "opentype",
                Some(ext) if ext.eq_ignore_ascii_case("woff") => "woff",
                Some(ext) if ext.eq_ignore_ascii_case("woff2") => "woff2",
                _ => "",
            };
            let faces: Vec<_> = FontInfo::iter(&bytes).collect();
            for face in
                faces.iter().map(Some).chain(faces.is_empty().then_some(None))
            {
                let _ = write!(
                    css,
                    "@font-face{{font-family:{};src:url({})",
                    css_string(&font.family),
                    url,
                );
                if !format.is_empty() {
                    let _ = write!(css, " format(\"{format}\")");
                }
                css.push(';');
                if let Some(face) = face {
                    let style = match face.variant.style {
                        FontStyle::Normal => "normal",
                        FontStyle::Italic => "italic",
                        FontStyle::Oblique => "oblique",
                    };
                    let _ = write!(
                        css,
                        "font-weight:{};font-style:{style};",
                        face.variant.weight.to_number(),
                    );
                }
                css.push_str("font-display:swap}\n");
            }
        }
    }
    let mut tokens = Vec::new();
    for (font, property, fallback) in [
        (&design.fonts.body, "--w-font-body", "system-ui,sans-serif"),
        (&design.fonts.heading, "--w-font-heading", "system-ui,sans-serif"),
        (&design.fonts.mono, "--w-font-mono", "ui-monospace,monospace"),
    ] {
        if let Some(font) = font {
            tokens.push(format!(
                "{property}:{},{fallback}",
                css_string(&font.family)
            ));
        }
    }
    if let Some(size) = design.text.size {
        tokens.push(format!("--w-font-size:{}", css_pt(size)));
    }
    if let Some(height) = design.text.line_height {
        tokens.push(format!("--w-line-height:{}", css_line_height(height)));
    }
    tokens.extend(color_tokens(design));
    if !tokens.is_empty() {
        let _ = writeln!(css, ":root{{{}}}", tokens.join(";"));
    }
    Ok(css)
}

/// The declared `colors.*` as `--w-*` declarations, in §5.2's order.
fn color_tokens(design: &Design) -> Vec<String> {
    [
        (design.colors.text, "--w-text"),
        (design.colors.background, "--w-bg"),
        (design.colors.muted, "--w-muted"),
        (design.colors.accent, "--w-accent"),
        (design.colors.link, "--w-link"),
    ]
    .into_iter()
    .filter_map(|(color, property)| {
        color.map(|color| format!("{property}:{}", css_color(color)))
    })
    .collect()
}

/// The declared colours once more for print, placed after the default
/// theme's print sheet, whose light palette and `body { background: #fff;
/// color: #000 }` would otherwise replace them on paper.
pub fn print_colors_css(design: &Design) -> String {
    let tokens = color_tokens(design);
    if !design.declared || tokens.is_empty() {
        return String::new();
    }
    let mut body = Vec::new();
    if design.colors.background.is_some() {
        body.push("background:var(--w-bg)");
    }
    if design.colors.text.is_some() {
        body.push("color:var(--w-text)");
    }
    let body = if body.is_empty() {
        String::new()
    } else {
        format!("body{{{}}}", body.join(";"))
    };
    format!(
        "/* design: print colors */\n@media print{{:root{{{}}}{body}}}\n",
        tokens.join(";")
    )
}

/// Explicit geometry and paragraph layout, after the theme's generic rules.
pub fn layout_css(
    design: &Design, _targets: &mut Targets<'_>,
) -> Result<String> {
    if !design.declared {
        return Ok(String::new());
    }
    let mut css = String::from("/* design: layout */\n");
    let mut paragraph = Vec::new();
    if let Some(spacing) = design.text.paragraph_spacing {
        paragraph.push(format!("margin-block:0 {}", css_pt(spacing)));
    }
    if let Some(justify) = design.text.justify {
        paragraph.push(format!(
            "text-align:{}",
            if justify { "justify" } else { "start" }
        ));
    }
    if !paragraph.is_empty() {
        let _ = writeln!(
            css,
            "[data-writer-document] p{{{}}}",
            paragraph.join(";")
        );
    }
    let mut page = Vec::new();
    if let Some(paper) = &design.page.paper {
        page.push(format!("size:{}", css_size(paper)));
    }
    let margin = &design.page.margin;
    match (margin.top, margin.right, margin.bottom, margin.left) {
        (Some(top), Some(right), Some(bottom), Some(left)) => {
            page.push(format!(
                "margin:{} {} {} {}",
                css_pt(top),
                css_pt(right),
                css_pt(bottom),
                css_pt(left),
            ));
        }
        _ => {
            for (side, value) in [
                ("top", margin.top),
                ("right", margin.right),
                ("bottom", margin.bottom),
                ("left", margin.left),
            ] {
                if let Some(value) = value {
                    page.push(format!("margin-{side}:{}", css_pt(value)));
                }
            }
        }
    }
    if !page.is_empty() {
        let _ = writeln!(css, "@page{{{}}}", page.join(";"));
    }
    // Named styles, furniture and the page background append their CSS here.
    css.push_str(&styles_css(design));
    css.push_str(&furniture_css(design, _targets)?);
    css.push_str(&background_css(design, _targets)?);
    Ok(css)
}

/// A named CSS paper size, or dimensions from Typst's full paper table.
pub fn css_size(paper: &str) -> String {
    match paper.to_ascii_lowercase().as_str() {
        "a3" => "A3".into(),
        "a4" => "A4".into(),
        "a5" => "A5".into(),
        "iso-b4" => "B4".into(),
        "iso-b5" => "B5".into(),
        "jis-b4" => "JIS-B4".into(),
        "jis-b5" => "JIS-B5".into(),
        "us-letter" => "letter".into(),
        "us-legal" => "legal".into(),
        "us-tabloid" => "ledger".into(),
        _ => match paper.parse::<typst::layout::Paper>() {
            Ok(paper) => format!(
                "{}mm {}mm",
                number(paper.width().to_mm()),
                number(paper.height().to_mm()),
            ),
            Err(_) => paper.to_string(),
        },
    }
}

pub(crate) fn css_pt(pt: Pt) -> String {
    format!("{}pt", number(pt.0))
}

pub(crate) fn css_line_height(height: LineHeight) -> String {
    match height {
        LineHeight::Multiple(value) => value.to_string(),
        LineHeight::Exact(value) => css_pt(value),
    }
}

pub(crate) fn css_color(Color([r, g, b]): Color) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub(crate) fn css_string(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' | '\u{000c}' => {
                let _ = write!(out, "\\{:x} ", character as u32);
            }
            '\0' => out.push('\u{fffd}'),
            value => out.push(value),
        }
    }
    out.push('"');
    out
}

fn number(value: f64) -> String {
    ((value * 1000.0).round() / 1000.0).to_string()
}

pub(crate) fn file_url(
    file: &Path, targets: &mut Targets<'_>,
) -> Result<String> {
    match targets {
        Targets::Placeholders(registry) => {
            let id = registry.register_file(file);
            Ok(AssetRegistry::placeholder(&id))
        }
        Targets::ServeUrls => {
            // Design files may be outside a theme. The existing document-asset
            // route safely serves only registered files and versions their bytes.
            let mut registry = AssetRegistry::new();
            let id = registry.register_file(file);
            Emitter::new(&registry, &OutputMode::Serve).url_for(&id)
        }
    }
}

/// Paragraph style rules selected by the resolved design's declared names.
pub fn styles_css(design: &Design) -> String {
    use crate::design::{Align, FontRef};
    let mut css = String::new();
    for (name, style) in &design.styles {
        let mut declarations = Vec::new();
        if let Some(font) = &style.font {
            let value = match font {
                FontRef::Body => "var(--w-font-body)".into(),
                FontRef::Heading => "var(--w-font-heading)".into(),
                FontRef::Mono => "var(--w-font-mono)".into(),
                FontRef::Family(family) => css_string(family),
            };
            declarations.push(format!("font-family:{value}"));
        }
        if let Some(size) = style.size {
            declarations.push(format!("font-size:{}", css_pt(size)));
        }
        if let Some(height) = style.line_height {
            declarations
                .push(format!("line-height:{}", css_line_height(height)));
        }
        if let Some(color) = style.color {
            declarations.push(format!("color:{}", css_color(color)));
        }
        if let Some(weight) = style.weight {
            declarations.push(format!("font-weight:{}", weight.0));
        }
        if let Some(italic) = style.italic {
            declarations.push(format!(
                "font-style:{}",
                if italic { "italic" } else { "normal" }
            ));
        }
        if let Some(align) = style.align {
            let value = match align {
                Align::Left => "left",
                Align::Center => "center",
                Align::Right => "right",
                Align::Justify => "justify",
            };
            declarations.push(format!("text-align:{value}"));
        }
        if let Some(space) = style.space_before {
            declarations.push(format!("margin-top:{}", css_pt(space)));
        }
        if let Some(space) = style.space_after {
            declarations.push(format!("margin-bottom:{}", css_pt(space)));
        }
        if style.keep_together {
            declarations.push("break-inside:avoid".into());
        }
        if style.keep_with_next {
            declarations.push("break-after:avoid".into());
        }
        let _ = writeln!(
            css,
            "[data-style={}]{{{}}}",
            css_string(name),
            declarations.join(";")
        );
        if style.align.is_some() {
            let _ = writeln!(
                css,
                "[data-style={}] p{{text-align:inherit}}",
                css_string(name)
            );
        }
    }
    css
}

/// A page furniture slot exposed to HTML templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FurnitureSlot {
    /// The page header.
    Header,
    /// The page footer.
    Footer,
}

/// Which page class a furniture helper renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FurnitureVariant {
    /// The first printed page or continuous screen page.
    First,
    /// The continuation pages.
    Rest,
}

impl FurnitureSlot {
    fn name(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::Footer => "footer",
        }
    }

    fn slot(self, design: &Design) -> Option<&crate::design::Slot> {
        match self {
            Self::Header => design.page.header.as_ref(),
            Self::Footer => design.page.footer.as_ref(),
        }
    }
}

impl FurnitureVariant {
    fn name(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Rest => "rest",
        }
    }

    fn furniture(
        self, slot: &crate::design::Slot,
    ) -> Option<&crate::design::Furniture> {
        match self {
            Self::First => slot.first.as_ref(),
            Self::Rest => slot.rest.as_ref(),
        }
    }
}

/// Escaped screen markup for one declared furniture variant.
pub fn furniture_html(
    design: &Design, slot: FurnitureSlot, variant: FurnitureVariant,
    targets: &mut Targets<'_>,
) -> Result<String> {
    use super::emit::escape;
    use crate::design::RulePosition;
    if !design.declared {
        return Ok(String::new());
    }
    let Some(furniture) =
        slot.slot(design).and_then(|slot| variant.furniture(slot))
    else {
        return Ok(String::new());
    };
    let mut props = vec![
        format!("font-weight:{}", furniture.weight.0),
        format!(
            "font-style:{}",
            if furniture.italic { "italic" } else { "normal" }
        ),
    ];
    if let Some(size) = furniture.size {
        props.push(format!("font-size:{}", css_pt(size)));
    } else if slot == FurnitureSlot::Footer {
        props.push("font-size:0.75em".into());
    }
    if let Some(color) = furniture.color {
        props.push(format!("color:{}", css_color(color)));
    }
    let mut html = format!(
        "<div class=\"w-furniture w-{} w-{}-{}\" data-align=\"{}\" style=\"{}\">",
        slot.name(),
        slot.name(),
        variant.name(),
        furniture_align(furniture.align),
        escape(&props.join(";")),
    );
    if let Some(rule) =
        furniture.rule.filter(|rule| rule.position == RulePosition::Above)
    {
        html.push_str(&rule_html(rule));
    }
    if let Some(image) = &furniture.image {
        let url = file_url(&image.path, targets)?;
        let mut style = Vec::new();
        if let Some(width) = image.width {
            style.push(format!("width:{}", css_pt(width)));
        }
        if let Some(align) = image.align {
            style.push(format!("justify-self:{}", furniture_align(align)));
        }
        style.push(format!("margin-bottom:{}", image_gap(image.gap)));
        let _ = write!(
            html,
            "<img class=\"w-furniture-image\" src=\"{}\" alt=\"\"",
            escape(&url)
        );
        if !style.is_empty() {
            let _ = write!(html, " style=\"{}\"", escape(&style.join(";")));
        }
        html.push('>');
    }
    for (lines, class) in [
        (&furniture.lines, "w-furniture-lines"),
        (&furniture.right_lines, "w-furniture-lines w-furniture-right"),
    ] {
        if lines.is_empty() {
            continue;
        }
        let _ = write!(html, "<div class=\"{class}\">");
        for line in lines {
            let mut props = Vec::new();
            if let Some(weight) = line.weight {
                props.push(format!("font-weight:{}", weight.0));
            }
            if let Some(italic) = line.italic {
                props.push(format!(
                    "font-style:{}",
                    if italic { "italic" } else { "normal" }
                ));
            }
            if let Some(size) = line.size {
                props.push(format!("font-size:{}", css_pt(size)));
            }
            if let Some(color) = line.color {
                props.push(format!("color:{}", css_color(color)));
            }
            html.push_str("<p class=\"w-furniture-line\"");
            if !props.is_empty() {
                let _ = write!(html, " style=\"{}\"", escape(&props.join(";")));
            }
            html.push('>');
            for segment in crate::design::segments(&line.text) {
                html.push_str(&match segment {
                    crate::design::Segment::Text(text) => escape(&text),
                    crate::design::Segment::Page
                    | crate::design::Segment::Pages => "1".into(),
                });
            }
            html.push_str("</p>");
        }
        html.push_str("</div>");
    }
    if let Some(rule) =
        furniture.rule.filter(|rule| rule.position == RulePosition::Below)
    {
        html.push_str(&rule_html(rule));
    }
    html.push_str("</div>");
    Ok(html)
}

// The lines below an image are a paragraph of their own: an unset gap is the
// backend's paragraph spacing, which a stylesheet can only express as 1em.
fn image_gap(gap: Option<Pt>) -> String {
    gap.map_or_else(|| "1em".into(), css_pt)
}

fn rule_html(rule: crate::design::Rule) -> String {
    format!(
        "<hr class=\"w-furniture-rule\" style=\"border-color:{};border-width:{}\">",
        css_color(rule.color),
        css_pt(rule.width),
    )
}

fn furniture_align(align: crate::design::Align) -> &'static str {
    match align {
        crate::design::Align::Left => "left",
        crate::design::Align::Center => "center",
        crate::design::Align::Right => "right",
        crate::design::Align::Justify => "justify",
    }
}

/// Screen furniture and either print margin boxes or fixed copies.
pub fn furniture_css(
    design: &Design, targets: &mut Targets<'_>,
) -> Result<String> {
    if !design.declared
        || design.page.header.is_none() && design.page.footer.is_none()
    {
        return Ok(String::new());
    }
    let mut css = String::from(
        ".w-furniture{display:grid;grid-template-columns:1fr auto;gap:0 1em}\n\
         .w-furniture[data-align=left]{text-align:left}\n\
         .w-furniture[data-align=center]{text-align:center}\n\
         .w-furniture[data-align=right]{text-align:right}\n\
         .w-furniture-lines{grid-column:1}\n\
         .w-furniture-right{grid-column:2;text-align:right}\n\
         .w-furniture>.w-furniture-lines:only-of-type{grid-column:1/-1}\n\
         .w-furniture-image,.w-furniture-rule{grid-column:1/-1}\n\
         .w-furniture-image{justify-self:center}\n\
         .w-furniture[data-align=left] .w-furniture-image{justify-self:left}\n\
         .w-furniture[data-align=right] .w-furniture-image{justify-self:right}\n\
         .w-furniture-line{margin:0}\n\
         .w-furniture-rule{width:100%;margin:0;border-style:solid none none}\n\
         .w-furniture[data-print-copy]{display:none}\n",
    );
    match design.page.print {
        crate::design::PrintMode::MarginBoxes => {
            css.push_str("@media print{.w-furniture{display:none}}\n");
            for slot in [FurnitureSlot::Header, FurnitureSlot::Footer] {
                let Some(resolved) = slot.slot(design) else {
                    continue;
                };
                for variant in [FurnitureVariant::Rest, FurnitureVariant::First]
                {
                    let page = if variant == FurnitureVariant::First {
                        " :first"
                    } else {
                        ""
                    };
                    let _ = write!(css, "@page{page}{{");
                    let mut occupied = Vec::new();
                    if let Some(furniture) = variant.furniture(resolved) {
                        let both = !furniture.right_lines.is_empty();
                        let position = if both {
                            "left"
                        } else {
                            furniture_align(furniture.align)
                        };
                        let name = box_name(slot, position);
                        let _ = write!(
                            css,
                            "@{name}{{{}}}",
                            margin_box(
                                design, slot, furniture, &furniture.lines,
                                true, targets
                            )?,
                        );
                        occupied.push(position);
                        if both {
                            let name = box_name(slot, "right");
                            let _ = write!(
                                css,
                                "@{name}{{{}}}",
                                margin_box(
                                    design, slot, furniture,
                                    &furniture.right_lines, false, targets
                                )?,
                            );
                            occupied.push("right");
                        }
                    }
                    // Clear every unused box, including the default theme's
                    // centred page number, when this slot was declared empty.
                    for position in ["left", "center", "right"] {
                        if !occupied.contains(&position) {
                            let _ = write!(
                                css,
                                "@{}{{content:none}}",
                                box_name(slot, position)
                            );
                        }
                    }
                    css.push_str("}\n");
                }
            }
        }
        crate::design::PrintMode::Fixed => {
            css.push_str("@media print{.w-furniture[data-print-copy]{display:grid}.w-header-rest,.w-footer-rest{position:fixed;left:0;right:0}");
            for slot in [FurnitureSlot::Header, FurnitureSlot::Footer] {
                if slot.slot(design).is_none() {
                    continue;
                }
                let (edge, offset) = match slot {
                    FurnitureSlot::Header => ("top", design.page.header_offset),
                    FurnitureSlot::Footer => {
                        ("bottom", design.page.footer_offset)
                    }
                };
                let _ = write!(
                    css,
                    ".w-{}-rest{{{edge}:{}}}",
                    slot.name(),
                    css_pt(offset.unwrap_or(Pt(0.0))),
                );
            }
            css.push_str("}\n");
            if design.page.footer.is_some() {
                css.push_str("@page{@bottom-left{content:none}@bottom-center{content:none}@bottom-right{content:none}}\n");
            }
            if design.page.header.is_some() {
                css.push_str("@page{@top-left{content:none}@top-center{content:none}@top-right{content:none}}\n");
            }
        }
    }
    Ok(css)
}

/// The page background image behind the first sheet on screen, printed once
/// on the first page or repeated as a fixed box on every page.
pub fn background_css(
    design: &Design, targets: &mut Targets<'_>,
) -> Result<String> {
    use crate::design::{Align, BackgroundPages, Fit, VerticalAlign};
    let Some(background) = &design.page.background else {
        return Ok(String::new());
    };
    if !design.declared {
        return Ok(String::new());
    }
    let image = &background.image;
    let url = file_url(&image.path, targets)?;
    let size = match image.fit {
        Fit::Cover => "cover",
        Fit::Contain => "contain",
        Fit::Stretch => "100% 100%",
    };
    let horizontal = match image.align.horizontal {
        Align::Left => "left",
        Align::Right => "right",
        Align::Center | Align::Justify => "center",
    };
    let vertical = match image.align.vertical {
        VerticalAlign::Top => "top",
        VerticalAlign::Center => "center",
        VerticalAlign::Bottom => "bottom",
    };
    let height = css_pt(paper_height(design.page.paper.as_deref()));
    let position = match background.pages {
        BackgroundPages::First => "absolute",
        BackgroundPages::All => "fixed",
    };
    // Browsers ignore `background` on `@page :first`. A block before the
    // content paints the first sheet on screen; in print an absolute box
    // appears once on page one and a fixed box on every page, both confined
    // to the page area.
    let mut css = format!(
        "body::before{{content:\"\";display:block;height:{height};margin-bottom:-{height};background:url({url}) no-repeat {horizontal} {vertical}/{size};print-color-adjust:exact;-webkit-print-color-adjust:exact}}\n"
    );
    let _ = writeln!(
        css,
        "@media print{{body::before{{position:{position};top:0;left:0;width:100%;height:100%;margin:0}}}}"
    );
    Ok(css)
}

/// The paper height in points; A4 for an unset or unknown paper name.
fn paper_height(paper: Option<&str>) -> Pt {
    paper
        .and_then(|paper| paper.parse::<typst::layout::Paper>().ok())
        .map_or(Pt(841.89), |paper| Pt(paper.height().to_pt()))
}

fn box_name(slot: FurnitureSlot, position: &str) -> String {
    let edge = if slot == FurnitureSlot::Header {
        "top"
    } else {
        "bottom"
    };
    format!("{edge}-{position}")
}

fn margin_box(
    design: &Design, slot: FurnitureSlot, furniture: &crate::design::Furniture,
    lines: &[crate::design::Line], image_and_rule: bool,
    targets: &mut Targets<'_>,
) -> Result<String> {
    let mut props = vec![format!("content:{}", furniture_content(lines))];
    props.push("white-space:pre-line".into());
    if image_and_rule && let Some(image) = &furniture.image {
        let url = file_url(&image.path, targets)?;
        let bytes = std::fs::read(&image.path)?;
        let dimensions = furniture_image_dimensions(&bytes);
        let width = image
            .width
            .or_else(|| dimensions.map(|(width, _)| Pt(width * 0.75)));
        let align = furniture_align(image.align.unwrap_or(furniture.align));
        props.push(format!(
            "background:url({url}) no-repeat top {align}/{} auto",
            width.map_or_else(|| "auto".into(), css_pt),
        ));
        // A CSS background has no intrinsic width. Keep an image-only box
        // wide enough for its logo without fixing wider text to that width.
        if let Some(width) = width {
            props.push(format!("min-width:{}", css_pt(width)));
        }
        if let (Some(width), Some((pixel_width, pixel_height))) =
            (width, dimensions)
        {
            let height = width.0 * pixel_height / pixel_width;
            let extra = furniture
                .rule
                .filter(|rule| {
                    rule.position == crate::design::RulePosition::Above
                })
                .map_or(0.0, |_| 4.0);
            let padding = Pt(height + extra);
            props.push(match image.gap {
                Some(gap) => {
                    format!("padding-top:{}", css_pt(Pt(padding.0 + gap.0)))
                }
                None => format!("padding-top:calc({} + 1em)", css_pt(padding)),
            });
        }
    }
    let size = furniture.size.map_or_else(
        || {
            if slot == FurnitureSlot::Footer {
                "calc(var(--w-font-size,11pt)*0.75)".into()
            } else {
                "var(--w-font-size,11pt)".into()
            }
        },
        css_pt,
    );
    let height = design
        .text
        .line_height
        .map_or_else(|| "var(--w-line-height,normal)".into(), css_line_height);
    let family = design.fonts.body.as_ref().map_or_else(
        || "var(--w-font-body,system-ui,sans-serif)".into(),
        |font| format!("{},system-ui,sans-serif", css_string(&font.family)),
    );
    props.push(format!(
        "font:{} {}{size}/{height} {family}",
        furniture.weight.0,
        if furniture.italic { "italic " } else { "" },
    ));
    if let Some(color) = furniture.color {
        props.push(format!("color:{}", css_color(color)));
    }
    let align = if furniture.right_lines.is_empty() {
        furniture_align(furniture.align)
    } else if image_and_rule {
        "left"
    } else {
        "right"
    };
    props.push(format!("text-align:{align}"));
    let (edge, offset) = if slot == FurnitureSlot::Header {
        ("top", design.page.header_offset)
    } else {
        ("bottom", design.page.footer_offset)
    };
    props.push(format!("vertical-align:{edge}"));
    if let Some(offset) = offset {
        props.push(format!("margin-{edge}:{}", css_pt(offset)));
    }
    // Both columns need the same border so a two-column footer's rule is
    // continuous. The image remains in the left column only.
    if let Some(rule) = furniture.rule {
        let side = match rule.position {
            crate::design::RulePosition::Above => "top",
            crate::design::RulePosition::Below => "bottom",
        };
        props.push(format!(
            "border-{side}:{} solid {}",
            css_pt(rule.width),
            css_color(rule.color)
        ));
        if !(side == "top" && image_and_rule && furniture.image.is_some()) {
            props.push(format!("padding-{side}:4pt"));
        }
    }
    Ok(props.join(";"))
}

// Raster metadata is cheap; Typst's existing SVG decoder also handles
// viewBox-only documents and compressed SVG without another parser.
fn furniture_image_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
    let dimensions = imagesize::blob_size(bytes)
        .ok()
        .map(|size| (size.width as f64, size.height as f64))
        .or_else(|| {
            typst::visualize::SvgImage::new(typst::foundations::Bytes::new(
                bytes.to_owned(),
            ))
            .ok()
            .map(|image| (image.width(), image.height()))
        });
    dimensions.filter(|(width, height)| {
        width.is_finite() && height.is_finite() && *width > 0.0 && *height > 0.0
    })
}

fn furniture_content(lines: &[crate::design::Line]) -> String {
    use crate::design::Segment;
    let mut parts = Vec::new();
    let mut text = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        for segment in crate::design::segments(&line.text) {
            match segment {
                Segment::Text(value) => text.push_str(&value),
                Segment::Page | Segment::Pages => {
                    if !text.is_empty() {
                        parts.push(css_string(&text).replace("\\a ", "\\A "));
                        text.clear();
                    }
                    parts.push(
                        if segment == Segment::Page {
                            "counter(page)"
                        } else {
                            "counter(pages)"
                        }
                        .into(),
                    );
                }
            }
        }
    }
    if !text.is_empty() || parts.is_empty() {
        parts.push(css_string(&text).replace("\\a ", "\\A "));
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_and_strings_keep_their_css_meaning() {
        assert_eq!(css_pt(Pt(11.0)), "11pt");
        assert_eq!(css_pt(Pt(141.732283)), "141.732pt");
        assert_eq!(css_string("A\"B\\C\nD"), "\"A\\\"B\\\\C\\a D\"");
    }

    #[test]
    fn paper_names_and_the_full_typst_table_are_supported() {
        for (name, css) in [
            ("a3", "A3"),
            ("a4", "A4"),
            ("a5", "A5"),
            ("iso-b4", "B4"),
            ("iso-b5", "B5"),
            ("jis-b4", "JIS-B4"),
            ("jis-b5", "JIS-B5"),
            ("us-letter", "letter"),
            ("us-legal", "legal"),
            ("us-tabloid", "ledger"),
            ("a6", "105mm 148mm"),
        ] {
            assert_eq!(css_size(name), css);
        }
    }

    #[test]
    fn an_undeclared_design_emits_no_css() {
        let mut registry = AssetRegistry::new();
        let mut targets = Targets::Placeholders(&mut registry);
        assert_eq!(tokens_css(&Design::default(), &mut targets).unwrap(), "");
        assert_eq!(layout_css(&Design::default(), &mut targets).unwrap(), "");
        assert!(registry.is_empty());
    }

    #[test]
    fn partial_margins_do_not_reset_the_other_sides() {
        let mut design = Design { declared: true, ..Design::default() };
        design.page.margin.left = Some(Pt(22.0));
        let mut registry = AssetRegistry::new();
        let css =
            layout_css(&design, &mut Targets::Placeholders(&mut registry))
                .unwrap();
        assert!(css.contains("@page{margin-left:22pt}"));
        assert!(!css.contains("margin-top") && !css.contains("size:"));
    }
}
