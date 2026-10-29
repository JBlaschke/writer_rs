//! Compiler-backed page and equation renditions (typst-dialect.md §9; math.md §3.4).
//! Owner: T03. Stub created by T00.

use typst::layout::FrameItem;
use typst_layout::{Page, PagedDocument};

use crate::error::{Error, Result};
use crate::ir::MathDialect;
use crate::typst::MITEX_VERSION;
use crate::typst::compile::{Compiled, compile_document, map_diagnostic};
use crate::typst::world::WriterWorld;

const PRELUDE: &str = concat!(
    "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n",
    "#set text(top-edge: \"bounds\", bottom-edge: \"bounds\")\n",
);

/// A self-contained rendition with dimensions and baseline in typographic points.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendition {
    /// SVG whose glyphs are paths, independent of fonts on the viewing machine.
    pub svg: Vec<u8>,
    /// A transparent PNG rendered at two pixels per point.
    pub png: Vec<u8>,
    /// A PDF rendition for images; equations leave this unset.
    pub pdf: Option<Vec<u8>>,
    /// Width in typographic points.
    pub width_pt: f64,
    /// Height in typographic points.
    pub height_pt: f64,
    /// Distance from the top of the image to its baseline.
    pub baseline_pt: f64,
}

impl Rendition {
    /// Distance from the baseline to the bottom of the rendition.
    pub fn descent_pt(&self) -> f64 {
        self.height_pt - self.baseline_pt
    }

    /// Pixel dimensions of the PNG, or `(0, 0)` if its bytes were replaced.
    pub fn png_size(&self) -> (u32, u32) {
        imagesize::blob_size(&self.png)
            .map(|size| (size.width as u32, size.height as u32))
            .unwrap_or_default()
    }
}

/// Renders one equation using embedded fonts and preserves the world's main file.
///
/// LaTeX source is passed unchanged to the vendored mitex package. Callers expand
/// their configured macros before calling this helper.
pub fn equation(
    world: &mut WriterWorld, source: &str, dialect: MathDialect, display: bool,
) -> Result<Rendition> {
    let text = equation_source(source, dialect, display);
    world.with_render_main(&text, false, |world| {
        let (document, _) = compile_document(world).into_result()?;
        let page = first_page(&document)?;
        let baseline =
            page.frame.items().find_map(|(position, item)| match item {
                FrameItem::Group(group) if group.frame.has_baseline() => {
                    Some((position.y + group.frame.baseline()).to_pt())
                }
                _ => None,
            });
        let baseline = baseline
            .ok_or_else(|| Error::Other("equation has no baseline".into()))?;
        rendition(page, baseline, None)
    })
}

/// Converts an image to SVG, PNG and PDF while preserving the world's main file.
///
/// Supported MIME types are PNG, JPEG, GIF, WebP, SVG and PDF. When provided,
/// `width_pt` scales the rendition to that width and preserves its aspect ratio.
pub fn image(
    world: &mut WriterWorld, bytes: &[u8], mime: &str, width_pt: Option<f64>,
) -> Result<Rendition> {
    let extension = image_extension(mime)?;
    world.set_render_image(extension, bytes);
    let width = width_pt.map(|width| format!(", width: {width}pt"));
    let text = format!(
        "{PRELUDE}#image(\"/__render/image.{extension}\"{})",
        width.as_deref().unwrap_or_default(),
    );
    world.with_render_main(&text, false, |world| {
        let (document, _) = compile_document(world).into_result()?;
        let page = first_page(&document)?;
        let pdf = typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default());
        let pdf = export_result(world, pdf)?;
        rendition(page, page.frame.height().to_pt(), Some(pdf))
    })
}

fn equation_source(
    source: &str, dialect: MathDialect, display: bool,
) -> String {
    match dialect {
        MathDialect::Typst if display => {
            format!("{PRELUDE}#box($ {source} $)")
        }
        MathDialect::Typst => format!("{PRELUDE}#box(${source}$)"),
        MathDialect::Latex => {
            let function = if display { "mitex" } else { "mi" };
            let literal = latex_literal(source);
            format!(
                "{PRELUDE}#import \"@preview/mitex:{MITEX_VERSION}\": mi, mitex\n#box({function}({literal}))",
            )
        }
    }
}

fn latex_literal(source: &str) -> String {
    if !source.contains('`') {
        return format!("`{source}`");
    }
    let mut literal = String::from("\"");
    for character in source.chars() {
        match character {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write;
                // Writing to a String cannot fail.
                write!(literal, "\\u{{{:x}}}", character as u32).unwrap();
            }
            character => literal.push(character),
        }
    }
    literal.push('"');
    literal
}

pub(crate) fn image_extension(mime: &str) -> Result<&'static str> {
    match mime {
        "image/png" => Ok("png"),
        "image/jpeg" => Ok("jpg"),
        "image/gif" => Ok("gif"),
        "image/webp" => Ok("webp"),
        "image/svg+xml" => Ok("svg"),
        "application/pdf" => Ok("pdf"),
        _ => Err(Error::Other(format!("unsupported image MIME type: {mime}"))),
    }
}

fn first_page(document: &PagedDocument) -> Result<&Page> {
    document
        .pages()
        .first()
        .ok_or_else(|| Error::Other("rendered document has no pages".into()))
}

fn rendition(
    page: &Page, baseline_pt: f64, pdf: Option<Vec<u8>>,
) -> Result<Rendition> {
    let svg = typst_svg::svg(
        page,
        &typst_svg::SvgOptions { render_bleed: false, pretty: false },
    );
    let png = typst_render::render(
        page,
        &typst_render::RenderOptions {
            pixel_per_pt: 2.0.into(),
            render_bleed: false,
        },
    )
    .encode_png()
    .map_err(|error| Error::Other(error.to_string()))?;
    Ok(Rendition {
        svg: svg.into_bytes(),
        png,
        pdf,
        width_pt: page.frame.width().to_pt(),
        height_pt: page.frame.height().to_pt(),
        baseline_pt,
    })
}

fn export_result<T>(
    world: &WriterWorld, output: typst::diag::SourceResult<T>,
) -> Result<T> {
    match output {
        Ok(output) => Ok(output),
        Err(errors) => {
            let diagnostics = errors
                .iter()
                .map(|error| map_diagnostic(world, error))
                .collect();
            Compiled { output: None, diagnostics }
                .into_result()
                .map(|(output, _)| output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latex_literal_preserves_source() {
        assert_eq!(latex_literal(r"\frac{a}{b}"), r"`\frac{a}{b}`");
        assert_eq!(
            latex_literal("`\\\"\n\r\t\0"),
            "\"`\\\\\\\"\\n\\r\\t\\u{0}\"",
        );
    }
}
