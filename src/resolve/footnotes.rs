//! Footnote resolution and numbering (references.md §4.6).
//! Owner: T17. Stub created by T00.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Markdown names bind in a file; Typst labels bind throughout the document.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NoteKey {
    scope: Option<PathBuf>,
    label: String,
}

impl NoteKey {
    pub fn new(path: &std::path::Path, typst: bool, label: &str) -> Self {
        Self {
            scope: (!typst).then(|| path.to_path_buf()),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Footnotes {
    pub count: u32,
    numbers: BTreeMap<NoteKey, u32>,
}

impl Footnotes {
    pub fn lookup(&self, key: &NoteKey) -> Option<u32> {
        self.numbers.get(key).copied()
    }

    pub fn next(&mut self, key: Option<NoteKey>) -> u32 {
        self.count += 1;
        if let Some(key) = key {
            self.numbers.insert(key, self.count);
        }
        self.count
    }

    /// Adds an alternative's local identities to the completed primary table.
    pub fn with_alternative(&self, alternative: &Self) -> Self {
        let mut combined = self.clone();
        combined.numbers.extend(alternative.numbers.clone());
        combined
    }
}
