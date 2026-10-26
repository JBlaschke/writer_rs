//! HTML markup, document templates and output-mode-aware theme helpers.
//!
//! `render` combines the resolved IR with a theme and effective configuration.
//! It returns HTML and site assets without writing files to disk.

pub mod context;
pub mod document;
pub mod emit;
pub mod highlight;
pub mod math;
pub mod paths;
pub mod styles;

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::assets::emit::{EmittedFile, OutputMode};
use crate::assets::registry::AssetRegistry;
use crate::config::Config;
use crate::error::Diagnostics;
use crate::ir::Document;
use crate::theme::Theme;

pub use context::context_json;
pub use document::{
    SERVE_HELPER_PREFIX, SERVE_THEME_CSS, render, serve_stylesheet,
};
pub use paths::{normalize_helper_path, resolve_helper_path};
pub use styles::compile_theme_css;

/// Which command is rendering: `writer build` or `writer serve`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildMode {
    /// A one-off build, serialized as `"build"`.
    Build,
    /// A live preview, with template source excerpts in errors.
    Serve,
}

/// The template context's `build` object (html-output.md §4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildInfo {
    /// Build or serve.
    pub mode: BuildMode,
    /// Whether this is a single self-contained file.
    pub standalone: bool,
    /// Whether `live_reload()` emits the preview client script.
    pub live_reload: bool,
    /// Serve rebuild counter; zero in build mode.
    pub id: u64,
    /// The writer version.
    pub version: &'static str,
}

impl BuildInfo {
    /// A one-off build, with no rebuild id or live reload.
    pub fn build(standalone: bool) -> Self {
        Self {
            mode: BuildMode::Build,
            standalone,
            live_reload: false,
            id: 0,
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    /// A preview render at rebuild number `id`.
    pub fn serve(id: u64) -> Self {
        Self {
            mode: BuildMode::Serve,
            standalone: false,
            live_reload: true,
            id,
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

/// Everything needed to render a document with its theme.
pub struct RenderInput<'a> {
    /// Resolved document IR, with asset placeholders in its URLs.
    pub document: &'a Document,
    /// Document assets, including images registered by the math hook.
    pub assets: &'a mut AssetRegistry,
    /// Resolved theme chain.
    pub theme: &'a Theme,
    /// Effective configuration; the renderer applies no further merges.
    pub config: &'a Config,
    /// How helpers and document placeholders become URLs.
    pub output: OutputMode,
    /// Values exposed as the template's `build` object.
    pub build: BuildInfo,
    /// Equation renderer used while emitting document fragments.
    pub math: &'a mut dyn emit::MathHook,
}

/// A rendered page and the files and diagnostics produced alongside it.
pub struct RenderedDocument {
    /// Final HTML, with asset placeholders resolved for the output mode.
    pub html: String,
    /// Theme stylesheet, ready to inline, write or serve.
    pub theme_css: String,
    /// Site-mode files, in first-use order; empty in other modes.
    pub files: Vec<EmittedFile>,
    /// Non-fatal findings from markup, math and template helpers.
    pub diagnostics: Diagnostics,
    /// Files read from disk themes, for watching and build inputs.
    pub theme_files: BTreeSet<PathBuf>,
}
