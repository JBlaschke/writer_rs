//! Chrome print target integration; implemented by T77.

use crate::error::{Diagnostics, Result};
use crate::pipeline::{BuildOptions, Prepared, not_implemented};
use crate::typst::world::WriterWorld;
use std::path::{Path, PathBuf};

/// Prints a document with Chrome; T77 replaces the pending implementation.
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)> {
    let _ = (prepared, opts, out, world);
    Err(not_implemented("pdf --engine chrome", "T77"))
}
