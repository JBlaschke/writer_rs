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
