//! Bibliography library loading and merging (references.md §5.1).
//! Owner: T70. Stub created by T00.

use crate::error::{Error, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Citation library stub; bodies and private fields are replaced by T70.
#[derive(Debug, Default)]
pub struct Library {
    style_spec: String,
    files: Vec<PathBuf>,
}

impl Library {
    /// Stub: bibliography loading arrives in T70.
    pub fn load(_paths: &[PathBuf]) -> Result<Library> {
        Err(Error::Other("citations arrive in T70".into()))
    }
    /// Stub: records the requested style without loading a citation engine.
    pub fn set_style(&mut self, spec: &str, _base_dir: &Path) -> Result<()> {
        self.style_spec = spec.into();
        Ok(())
    }
    /// Stub: no bibliography keys are available before T70.
    pub fn keys(&self) -> BTreeSet<String> {
        BTreeSet::new()
    }
    /// Stub: note styles become available in T70.
    pub fn is_note_style(&self) -> bool {
        false
    }
    /// The style specification recorded by the stub.
    pub fn style_spec(&self) -> &str {
        &self.style_spec
    }
    /// Stub bibliography dependencies, empty before T70.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
}
