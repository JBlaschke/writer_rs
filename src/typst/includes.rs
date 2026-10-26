//! Markdown and notebook includes from Typst (typst-dialect.md §6.4).
//! T16 installs this provider; T53 replaces its synthesis body.

use super::world::{IncludeProvider, Synthesized};
use crate::config::Config;
use crate::error::{Error, Result};
use std::path::Path;

/// Effective config used when synthesizing a Markdown or notebook include.
pub struct MarkdownIncludes {
    /// Parent document's effective settings.
    pub config: Config,
}

impl IncludeProvider for MarkdownIncludes {
    fn synthesize(&self, path: &Path, root: &Path) -> Result<Synthesized> {
        let _ = (path, root);
        Err(Error::Other(
            "including .md/.ipynb from typst arrives in T53".into(),
        ))
    }
}
