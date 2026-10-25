//! Jupyter notebook frontend and IR conversion (notebook-mapping.md §2).
//! Owner: T01 → T18. Stub created by T00.

pub mod ansi;
pub mod convert;
pub mod nbformat;
pub mod options;
pub mod outputs;
pub mod tables;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::ir::{Parsed, SourceInfo};

/// Parses notebook input into document IR. Implemented by T18.
pub fn parse(
    source: &SourceInfo, json: &str, config: &Config,
) -> Result<Parsed> {
    let _ = (source, json, config);
    Err(Error::Other("notebook support arrives in T18".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::SourceKind;

    #[test]
    fn stub_names_its_owner() {
        let source = SourceInfo {
            path: "source".into(),
            kind: SourceKind::Notebook,
            base_dir: ".".into(),
        };
        match parse(&source, "", &Config::default()) {
            Err(Error::Other(message)) => {
                assert_eq!(message, "notebook support arrives in T18")
            }
            _ => panic!("expected the T18 frontend stub error"),
        }
    }
}
