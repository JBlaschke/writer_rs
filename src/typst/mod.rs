//! Typst frontend, emitter, and embedded compiler (typst-dialect.md §1).
//! Owner: T03. Stub created by T00.

/// Version of the bundled `@writer/doc` Typst package.
pub const WRITER_DOC_VERSION: &str = "0.1.0";

pub use crate::vendor::MITEX_VERSION;

pub mod compile;
pub mod emit;
pub mod frontend;
pub mod html;
pub mod includes;
pub mod package;
pub mod render;
pub mod sourcemap;
pub mod world;
