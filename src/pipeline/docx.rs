//! Word target integration; implemented by T41.

use crate::assets::registry::AssetRegistry;
use crate::config::Config;
use crate::error::{Diagnostics, Result};
use crate::ir::Document;
use crate::typst::world::WriterWorld;
use std::path::Path;

/// Builds an editable Word document; T41 replaces the pending implementation.
pub fn build(
    doc: &Document, assets: &AssetRegistry, config: &Config, out: &Path,
    world: &mut WriterWorld,
) -> Result<Diagnostics> {
    let _ = (doc, assets, config, out, world);
    Err(super::not_implemented("docx", "T41"))
}
