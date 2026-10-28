//! Word target integration over the build session's shared compiler world.

use std::path::Path;

use crate::assets::registry::AssetRegistry;
use crate::config::Config;
use crate::docx::{DocxOptions, Picture, Renderer};
use crate::error::{Diagnostics, Result};
use crate::ir::{Document, MathDialect};
use crate::typst::render::Rendition;
use crate::typst::world::WriterWorld;

/// Borrows the existing compiler session to render DOCX image fallbacks.
pub struct WorldRenderer<'a> {
    world: &'a mut WriterWorld,
}

impl<'a> WorldRenderer<'a> {
    /// Creates an adapter; the emitter applies configuration before rendering.
    pub fn new(_config: &'a Config, world: &'a mut WriterWorld) -> Self {
        Self { world }
    }
}

impl Renderer for WorldRenderer<'_> {
    fn equation(
        &mut self, source: &str, dialect: MathDialect, display: bool,
    ) -> Result<Picture> {
        crate::typst::render::equation(self.world, source, dialect, display)
            .map(picture)
    }

    fn image(
        &mut self, bytes: &[u8], mime: &str, width_pt: Option<f64>,
    ) -> Result<Picture> {
        crate::typst::render::image(self.world, bytes, mime, width_pt)
            .map(picture)
    }

    fn typst_mathml(&mut self, source: &str, display: bool) -> Result<String> {
        crate::typst::html::equation_mathml(self.world, source, display)
    }
}

fn picture(rendition: Rendition) -> Picture {
    Picture {
        png: rendition.png,
        svg: Some(rendition.svg),
        width_pt: rendition.width_pt,
        height_pt: rendition.height_pt,
        baseline_pt: rendition.baseline_pt,
    }
}

/// Builds an editable Word document and writes it through output safety helpers.
pub fn build(
    doc: &Document, assets: &AssetRegistry, config: &Config, out: &Path,
    world: &mut WriterWorld,
) -> Result<Diagnostics> {
    let reference = (!config.docx.reference_doc.as_os_str().is_empty())
        .then_some(config.docx.reference_doc.as_path());
    let options = DocxOptions {
        config,
        reference,
        source_date_epoch: std::env::var("SOURCE_DATE_EPOCH").ok(),
        all_styles: false,
    };
    let mut renderer = WorldRenderer::new(config, world);
    let output = crate::docx::write(doc, assets, &mut renderer, &options)?;
    super::write_file(out, &output.bytes)?;
    Ok(output.diagnostics)
}
