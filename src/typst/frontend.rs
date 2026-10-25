//! Structured Typst input to document IR (typst-dialect.md §2).
//! Owner: T01 → T50. Stub created by T00.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::ir::{Parsed, SourceInfo};

/// Parses typst input into document IR. Implemented by T50.
pub fn parse(
    source: &SourceInfo, text: &str, config: &Config,
) -> Result<Parsed> {
    let _ = (source, text, config);
    Err(Error::Other("typst support arrives in T50".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::SourceKind;

    #[test]
    fn stub_names_its_owner() {
        let source = SourceInfo {
            path: "source".into(),
            kind: SourceKind::Typst,
            base_dir: ".".into(),
        };
        match parse(&source, "", &Config::default()) {
            Err(Error::Other(message)) => {
                assert_eq!(message, "typst support arrives in T50")
            }
            _ => panic!("expected the T50 frontend stub error"),
        }
    }
}
