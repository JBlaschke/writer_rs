//! Pdf target integration; implemented by T32.

use std::path::{Path, PathBuf};

use super::{BuildOptions, Prepared, not_implemented};
use crate::error::{Diagnostics, Result};
use crate::typst::world::WriterWorld;

/// Builds this target; T32 replaces the pending implementation.
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)> {
    let _ = (prepared, opts, out, world);
    Err(not_implemented("pdf", "T32"))
}

/// Generated source for `dump --typst`; implemented by T32.
pub fn dump(
    prepared: &Prepared, opts: &BuildOptions,
) -> Result<(String, Diagnostics)> {
    let _ = (prepared, opts);
    Err(not_implemented("dump --typst", "T32"))
}

/// Loads generated Typst into the shared world; implemented by T32.
pub fn load_typst_main(
    prepared: &Prepared, opts: &BuildOptions, world: &mut WriterWorld,
) -> std::result::Result<(), super::BuildFailure> {
    let _ = (prepared, opts, world);
    Err(not_implemented("pdf", "T32").into())
}

/// Page count when supported; T32 replaces this placeholder.
pub fn page_count(pdf: &[u8]) -> Option<usize> {
    let _ = pdf;
    None
}
