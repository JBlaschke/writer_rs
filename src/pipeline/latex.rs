//! Latex target integration; implemented by T61.

use std::path::{Path, PathBuf};

use super::{BuildOptions, Prepared, not_implemented};
use crate::error::{Diagnostics, Result};
use crate::typst::world::WriterWorld;

/// Builds this target; T61 replaces the pending implementation.
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)> {
    let _ = (prepared, opts, out, world);
    Err(not_implemented("latex", "T61"))
}

/// Generated source for `dump --latex`; implemented by T61.
pub fn dump(
    prepared: &Prepared, opts: &BuildOptions,
) -> Result<(String, Diagnostics)> {
    let _ = (prepared, opts);
    Err(not_implemented("dump --latex", "T61"))
}
