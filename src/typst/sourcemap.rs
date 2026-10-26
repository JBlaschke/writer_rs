//! Generated Typst source mapping (typst-dialect.md §5.7).
//! Owner: T03. Stub created by T00.

use crate::ir::SourceSpan;

/// Source locations corresponding to each line of generated Typst.
///
/// A missing entry denotes generated scaffolding with no source location.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceMap {
    lines: Vec<Option<SourceSpan>>,
}

impl SourceMap {
    /// Builds a map from one optional source span per generated line.
    pub fn from_lines(lines: Vec<Option<SourceSpan>>) -> SourceMap {
        SourceMap { lines }
    }

    /// Looks up a generated line, numbered from one.
    pub fn lookup(&self, line: usize) -> Option<&SourceSpan> {
        self.lines.get(line.checked_sub(1)?)?.as_ref()
    }

    /// Number of generated lines represented by this map.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the map has no generated lines.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_one_based() {
        let span = SourceSpan {
            start_line: 12,
            end_line: 14,
            cell: Some(2),
            file: Some("/docs/part.md".into()),
        };
        let map = SourceMap::from_lines(vec![None, Some(span.clone())]);
        assert_eq!(map.lookup(0), None);
        assert_eq!(map.lookup(1), None);
        assert_eq!(map.lookup(2), Some(&span));
        assert_eq!(map.lookup(3), None);
        assert_eq!(map.len(), 2);
        assert!(!map.is_empty());
        assert_eq!(map.clone(), map);
        assert!(SourceMap::default().is_empty());
    }
}
