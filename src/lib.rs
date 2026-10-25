//! Document publishing from Markdown, Jupyter notebooks, and Typst.
//!
//! The pipeline parses source files into a shared document IR and asset registry,
//! resolves labels, numbering, references, footnotes, and citations, then emits
//! HTML, PDF, DOCX, LaTeX, or Typst source. Build commands write the outputs, while
//! the development server rebuilds them in memory as source files change.
//!
//! Architecture: PLAN.md §3.1. Owner: T00.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod assets;
pub mod check;
pub mod cite;
pub mod cli;
pub mod config;
pub mod docx;
pub mod error;
pub mod html;
pub mod ir;
pub mod latex;
pub mod markdown;
pub mod math;
pub mod notebook;
pub mod pdf;
pub mod pipeline;
pub mod resolve;
pub mod serve;
pub mod theme;
pub mod typst;
pub mod vendor;

pub use error::{Error, Result};
