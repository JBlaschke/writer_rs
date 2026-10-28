//! Live page sets and their HTML shell (live-reload.md §4).
//!
//! T21 supplies the shared types and shell. [`PagesBuilder::build`] deliberately
//! returns an empty set until T24 adds compilation and per-page SVG reuse.

use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;

use crate::error::Diagnostics;
use crate::pipeline::{BuildFailure, BuildOptions, Prepared};
use crate::typst::world::WriterWorld;

/// A page's SVG resource, content hash, dimensions and compiler reuse key.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSvg {
    /// First 16 lowercase hexadecimal characters of the SVG's BLAKE3 hash.
    pub hash: String,
    /// Standalone SVG bytes, shared cheaply between retained page sets.
    pub svg: Bytes,
    /// Width and height in points, rounded to two decimal places by T24.
    pub size_pt: [f64; 2],
    /// Compiler page hash used to reuse an unchanged page's SVG in T24.
    pub key: u128,
}

/// One successful pages build, ordered by one-based page number.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSet {
    /// Build attempt that produced this set.
    pub build_id: u64,
    /// The document's pages in reading order.
    pub pages: Vec<PageSvg>,
}

impl PageSet {
    /// The first page whose content hash equals `hash`.
    pub fn find(&self, hash: &str) -> Option<&PageSvg> {
        self.pages.iter().find(|page| page.hash == hash)
    }

    /// Content hashes in page order, as sent in the live protocol.
    pub fn hashes(&self) -> Vec<String> {
        self.pages.iter().map(|page| page.hash.clone()).collect()
    }

    /// Page dimensions in page order, as sent in the live protocol.
    pub fn sizes(&self) -> Vec<[f64; 2]> {
        self.pages.iter().map(|page| page.size_pt).collect()
    }
}

/// First 16 lowercase hexadecimal characters of the SVG bytes' BLAKE3 hash.
pub fn page_hash(svg: &[u8]) -> String {
    blake3::hash(svg).to_hex()[..16].to_string()
}

/// The source compiled for a pages build using the session's retained World.
pub enum PagesInput<'a> {
    /// Markdown or notebook content already prepared by the pipeline.
    Generated {
        /// Parsed and resolved document, resources and effective config.
        prepared: &'a Prepared,
        /// Session options, including the current serve build ID.
        opts: &'a BuildOptions,
    },
    /// A Typst file compiled directly from disk.
    TypstFile {
        /// The input's path, also used as the World's main file.
        path: &'a Path,
    },
}

/// Session pages builder; T24 adds the previous set for compiler-page reuse.
#[derive(Default)]
pub struct PagesBuilder;

impl PagesBuilder {
    /// Creates a pages builder for one serve session.
    pub fn new() -> Self {
        Self
    }

    /// Returns an empty successful set until T24 implements page compilation.
    ///
    /// The source and World are deliberately untouched: the server's state and
    /// routes can run before the compiler-backed pages milestone is ready.
    pub fn build(
        &mut self, _world: &mut WriterWorld, _input: PagesInput<'_>,
        build_id: u64,
    ) -> std::result::Result<(Arc<PageSet>, Diagnostics), BuildFailure> {
        let set = PageSet { build_id, pages: Vec::new() };
        Ok((Arc::new(set), Diagnostics::default()))
    }
}

/// Styles of the pages shell; page SVGs remain separate immutable resources.
const PAGES_CSS: &str = r#"
body {
    margin: 0;
    padding-top: 80px;
    background: #e5e7eb;
    color: #111827;
    font: 14px/1.4 system-ui, sans-serif;
}
#writer-pages-toolbar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 100;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    background: #f9fafb;
    border-bottom: 1px solid #d1d5db;
}
#writer-pages-toolbar button { font: inherit; cursor: pointer; }
#writer-pages-zoom { min-width: 3em; text-align: center; }
#writer-pages-status { color: #b91c1c; }
.writer-pages-note { font-size: 12px; color: #4b5563; }
#writer-pages {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 16px;
}
.writer-page { background: white; box-shadow: 0 2px 8px #0003; }
.writer-page img { display: block; }
@media (prefers-color-scheme: dark) {
    body { background: #1f2937; color: #f3f4f6; }
    #writer-pages-toolbar { background: #111827; border-color: #374151; }
    .writer-pages-note { color: #d1d5db; }
    #writer-pages-status { color: #fca5a5; }
}
"#;

/// The pages-view shell of live-reload.md §4.2, with an HTML-escaped title.
pub fn shell_html(title: &str) -> String {
    let title = html_escape::encode_text(title);
    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-writer-view="pages">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · pages</title><style>{PAGES_CSS}</style></head>
<body>
<header id="writer-pages-toolbar">
<button type="button" data-zoom="out" title="Zoom out (-)">−</button><span id="writer-pages-zoom">Fit</span>
<button type="button" data-zoom="in" title="Zoom in (+)">+</button><button type="button" data-zoom="fit" title="Fit width (0)">Fit</button>
<span id="writer-pages-counter"></span><span id="writer-pages-status" hidden></span>
<span class="writer-pages-note">Visual preview of the PDF pages: text cannot be selected and links do not work.</span>
</header>
<main id="writer-pages" data-writer-pages></main>
<script src="/__writer/pages.js"></script>
<script src="/__writer/live-reload.js"></script>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(svg: &'static str, size_pt: [f64; 2], key: u128) -> PageSvg {
        PageSvg {
            hash: page_hash(svg.as_bytes()),
            svg: Bytes::from_static(svg.as_bytes()),
            size_pt,
            key,
        }
    }

    #[test]
    fn hashes_are_content_based_and_have_the_wire_format() {
        assert_eq!(page_hash(b""), "af1349b9f5f9a1a6");
        let svg = b"<svg><text>Page 1</text></svg>";
        let hash = page_hash(svg);
        assert_eq!(hash, blake3::hash(svg).to_hex()[..16]);
        assert_eq!(hash.len(), 16);
        assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(hash, hash.to_ascii_lowercase());
        assert_ne!(hash, page_hash(b"<svg><text>Page 2</text></svg>"));
    }

    #[test]
    fn page_set_lookup_and_metadata_keep_page_order() {
        let first = page("<svg>one</svg>", [595.28, 841.89], 4);
        let second = page("<svg>two</svg>", [612.0, 792.0], 5);
        let set = PageSet {
            build_id: 7,
            pages: vec![first.clone(), second.clone(), first.clone()],
        };
        assert_eq!(set.hashes(), [first.hash.clone(), second.hash, first.hash]);
        assert_eq!(set.sizes(), [first.size_pt, second.size_pt, first.size_pt]);
        assert_eq!(set.find(&set.pages[0].hash), Some(&set.pages[0]));
        assert_eq!(set.find("missing"), None);
        let empty = PageSet { build_id: 8, pages: Vec::new() };
        assert!(empty.hashes().is_empty());
        assert!(empty.sizes().is_empty());
        assert_eq!(empty.find("anything"), None);
    }

    #[test]
    fn shell_escapes_title_and_keeps_the_fixed_markup_and_script_order() {
        let html = shell_html("<script> & a title");
        assert!(html.starts_with("<!DOCTYPE html>\n"));
        assert!(
            html.contains(
                "<title>&lt;script&gt; &amp; a title · pages</title>"
            )
        );
        assert!(!html.contains("<script> & a title"));
        for hook in [
            r#"<html lang="en" data-writer-view="pages">"#,
            r#"<header id="writer-pages-toolbar">"#,
            r#"data-zoom="out""#,
            r#"data-zoom="in""#,
            r#"data-zoom="fit""#,
            r#"<span id="writer-pages-counter"></span>"#,
            r#"<span id="writer-pages-status" hidden></span>"#,
            r#"<main id="writer-pages" data-writer-pages></main>"#,
        ] {
            assert!(html.contains(hook), "missing {hook}");
        }
        assert!(html.contains(
            "<script src=\"/__writer/pages.js\"></script>\n\
             <script src=\"/__writer/live-reload.js\"></script>"
        ));
        assert!(!html.contains("<svg"));
        assert!(!html.contains("data-writer-document"));
        assert!(PAGES_CSS.contains("prefers-color-scheme: dark"));
    }

    #[test]
    fn builder_stub_returns_empty_sets_without_compiling_either_input_kind() {
        use crate::config::Config;
        use crate::typst::world::MainFile;
        use typst::foundations::Datetime;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("absent.typ");
        std::fs::write(&path, "#not_compiled()\n").unwrap();
        let mut config = Config::default();
        config.pdf.system_fonts = false;
        let mut world = WriterWorld::from_config(
            &config,
            root.path(),
            MainFile::Path(path.clone()),
            Datetime::from_ymd(2026, 10, 28).unwrap(),
        )
        .unwrap();
        std::fs::remove_file(&path).unwrap();
        let mut builder = PagesBuilder::new();
        for build_id in [1, 3] {
            let (set, diagnostics) = builder
                .build(
                    &mut world,
                    PagesInput::TypstFile { path: &path },
                    build_id,
                )
                .unwrap();
            assert_eq!(set.build_id, build_id);
            assert!(set.pages.is_empty());
            assert!(diagnostics.is_empty());
            assert!(world.accessed().is_empty());
        }

        let source = root.path().join("document.md");
        std::fs::write(&source, "# Prepared document\n").unwrap();
        let opts = BuildOptions::new(&source);
        let prepared = crate::pipeline::prepare(&opts).unwrap();
        std::fs::remove_file(&source).unwrap();
        let (set, diagnostics) = builder
            .build(
                &mut world,
                PagesInput::Generated { prepared: &prepared, opts: &opts },
                4,
            )
            .unwrap();
        assert_eq!(set.build_id, 4);
        assert!(set.pages.is_empty());
        assert!(diagnostics.is_empty());
        assert!(world.accessed().is_empty());
    }
}
