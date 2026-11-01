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
    for (color, property) in [
        (design.colors.text, "--w-text"),
        (design.colors.background, "--w-bg"),
        (design.colors.muted, "--w-muted"),
        (design.colors.accent, "--w-accent"),
        (design.colors.link, "--w-link"),
    ] {
        if let Some(color) = color {
            tokens.push(format!("{property}:{}", css_color(color)));
        }
    }
    if !tokens.is_empty() {
        let _ = writeln!(css, ":root{{{}}}", tokens.join(";"));
    }
    Ok(css)
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
    // Named styles and furniture append their CSS here in the later tasks.
    css.push_str(&styles_css(design));
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
