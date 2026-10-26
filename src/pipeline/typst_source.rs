//! Pdf target integration; implemented by T52.

use std::path::{Path, PathBuf};

use super::{BuildOptions, Prepared, not_implemented};
use crate::error::{Diagnostics, Result};
use crate::typst::world::WriterWorld;

/// Builds this target; T52 replaces the pending implementation.
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)> {
    let _ = (prepared, opts, out, world);
    Err(not_implemented("pdf", "T52"))
}
