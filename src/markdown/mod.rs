//! Markdown frontend and IR conversion (markdown-dialect.md §1–§2; ir.md §4).
//! Owner: T01 → T12. Stub created by T00.

pub mod attrs;
pub mod code;
pub mod directives;
pub mod frontmatter;
pub mod include;
pub mod options;
pub mod recover;
pub mod refs;
pub mod walker;

#[allow(unused_imports)] // T11
pub use options::*;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::ir::{Parsed, SourceInfo};

/// Parses markdown input into document IR. Implemented by T12.
pub fn parse(
    source: &SourceInfo, text: &str, config: &Config,
) -> Result<Parsed> {
    let _ = (source, text, config);
    Err(Error::Other("markdown support arrives in T12".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::SourceKind;

    #[test]
    fn stub_names_its_owner() {
        let source = SourceInfo {
            path: "source".into(),
            kind: SourceKind::Markdown,
            base_dir: ".".into(),
        };
        match parse(&source, "", &Config::default()) {
            Err(Error::Other(message)) => {
                assert_eq!(message, "markdown support arrives in T12")
            }
            _ => panic!("expected the T12 frontend stub error"),
        }
    }
}
