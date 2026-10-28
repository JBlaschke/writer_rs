//! Live page sets and their HTML shell (live-reload.md §4).
//!
//! The session compiles each document through its retained World and reuses
//! unchanged standalone SVG resources across successful builds.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use typst_layout::{Page, PagedDocument};

use crate::error::{Diagnostics, Error};
use crate::pipeline::{BuildFailure, BuildOptions, Prepared};
use crate::typst::compile::{Compiled, compile_document};
use crate::typst::world::WriterWorld;

/// A page's SVG resource, content hash, dimensions and compiler reuse key.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSvg {
    /// First 16 lowercase hexadecimal characters of the SVG's BLAKE3 hash.
    pub hash: String,
    /// Standalone SVG bytes, shared cheaply between retained page sets.
    pub svg: Bytes,
    /// Width and height in points, rounded to two decimal places.
    pub size_pt: [f64; 2],
    /// Compiler page hash used to reuse an unchanged page's SVG.
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

/// Retains the last successful page set for compiler-page SVG reuse.
#[derive(Default)]
pub struct PagesBuilder {
    previous: Option<Arc<PageSet>>,
}

impl PagesBuilder {
    /// Creates a pages builder for one serve session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compiles through the session's World and exports separate page SVGs.
    /// Failed compilation or export leaves the previous successful set intact.
    pub fn build(
        &mut self, world: &mut WriterWorld, input: PagesInput<'_>,
        build_id: u64,
    ) -> std::result::Result<(Arc<PageSet>, Diagnostics), BuildFailure> {
        let (document, diagnostics) = compile_input(world, input)?;
        let previous = self
            .previous
            .as_deref()
            .map(|set| set.pages.iter().map(|page| (page.key, page)).collect())
            .unwrap_or_default();
        let pages = document
            .pages()
            .iter()
            .map(|page| export_page(page, &previous))
            .collect();
        let set = Arc::new(PageSet { build_id, pages });
        self.previous = Some(set.clone());
        Ok((set, diagnostics))
    }
}

fn compile_input(
    world: &mut WriterWorld, input: PagesInput<'_>,
) -> std::result::Result<(PagedDocument, Diagnostics), BuildFailure> {
    let (path, diagnostics) = match input {
        PagesInput::Generated { prepared, opts } => {
            let diagnostics = crate::pipeline::pdf::load_with_diagnostics(
                prepared, opts, world,
            )?;
            (prepared.document.source.path.as_path(), diagnostics)
        }
        PagesInput::TypstFile { path } => (path, Diagnostics::default()),
    };
    // compile_document already includes the include provider's diagnostics.
    compiler_result(compile_document(world), diagnostics, path)
}

fn compiler_result<T>(
    compiled: Compiled<T>, mut diagnostics: Diagnostics, input: &Path,
) -> std::result::Result<(T, Diagnostics), BuildFailure> {
    let Compiled { output, diagnostics: found } = compiled;
    diagnostics.extend(found.into_iter().map(|mut diagnostic| {
        diagnostic.path = diagnostic
            .span
            .as_ref()
            .and_then(|span| span.file.clone())
            .or(diagnostic.path)
            .or_else(|| Some(input.to_path_buf()));
        diagnostic
    }));
    let result =
        Compiled { output, diagnostics: diagnostics.clone() }.into_result();
    result.map_err(|mut error| {
        if let Error::Compile { path, line, .. } = &mut error
            && path == Path::new("/__writer-main.typ")
        {
            *path = input.to_path_buf();
            *line = None;
        }
        BuildFailure { error: Box::new(error), diagnostics }
    })
}

fn export_page(page: &Page, previous: &HashMap<u128, &PageSvg>) -> PageSvg {
    let key = typst::utils::hash128(page);
    if let Some(cached) = previous.get(&key) {
        return (**cached).clone();
    }
    let svg =
        typst_svg::svg(page, &typst_svg::SvgOptions::default()).into_bytes();
    PageSvg {
        hash: page_hash(&svg),
        svg: Bytes::from(svg),
        size_pt: [page.frame.width().to_pt(), page.frame.height().to_pt()]
            .map(|size| (size * 100.0).round() / 100.0),
        key,
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
    use std::fs;

    use typst::foundations::Datetime;

    use crate::assets::registry::AssetRegistry;
    use crate::config::Config;
    use crate::error::{Diagnostic, Level};
    use crate::typst::sourcemap::SourceMap;
    use crate::typst::world::{IncludeProvider, MainFile, Synthesized};

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

    fn world(root: &Path, main: MainFile) -> WriterWorld {
        let mut config = Config::default();
        config.pdf.system_fonts = false;
        WriterWorld::from_config(
            &config,
            root,
            main,
            Datetime::from_ymd(2026, 10, 28).unwrap(),
        )
        .unwrap()
    }

    fn prepared(path: &Path, text: &str) -> (BuildOptions, Prepared) {
        fs::write(path, text).unwrap();
        let opts = BuildOptions::new(path);
        let mut prepared = crate::pipeline::prepare(&opts).unwrap();
        prepared.config.pdf.system_fonts = false;
        (opts, prepared)
    }

    #[test]
    fn generated_document_compiles_to_separate_content_addressed_pages() {
        let root = tempfile::tempdir().unwrap();
        let (opts, prepared) = prepared(
            &root.path().join("document.md"),
            "# First page\n\nFirst.\n\n```{=typst}\n#pagebreak()\n```\n\n# Second page\n",
        );
        let mut world = world(root.path(), MainFile::Virtual);
        let mut builder = PagesBuilder::new();
        let (set, diagnostics) = builder
            .build(
                &mut world,
                PagesInput::Generated { prepared: &prepared, opts: &opts },
                4,
            )
            .unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(set.build_id, 4);
        assert_eq!(set.pages.len(), 2);
        for page in &set.pages {
            assert!(
                std::str::from_utf8(&page.svg).unwrap().starts_with("<svg")
            );
            assert_eq!(page.hash, page_hash(&page.svg));
            assert_eq!(page.size_pt, [595.28, 841.89]);
        }
        assert!(Arc::ptr_eq(builder.previous.as_ref().unwrap(), &set));
    }

    #[test]
    fn typst_file_uses_disk_main_tracks_reads_and_rounds_page_sizes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.typ");
        fs::write(&path,
            "#set page(width: 123.456pt, height: 234.567pt, margin: 0pt)\nDirect Typst.\n",
        ).unwrap();
        let mut world = world(root.path(), MainFile::Path(path.clone()));
        let (set, diagnostics) = PagesBuilder::new()
            .build(&mut world, PagesInput::TypstFile { path: &path }, 8)
            .unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(set.pages.len(), 1);
        assert_eq!(set.pages[0].size_pt, [123.46, 234.57]);
        assert!(world.accessed().contains(&fs::canonicalize(path).unwrap()));
    }

    #[test]
    fn identical_pages_reuse_exact_bytes_even_after_previous_order_changes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.typ");
        fs::write(&path, "First.\n#pagebreak()\nSecond.\n").unwrap();
        let mut world = world(root.path(), MainFile::Path(path.clone()));
        let mut builder = PagesBuilder::new();
        let (first, _) = builder
            .build(&mut world, PagesInput::TypstFile { path: &path }, 1)
            .unwrap();
        assert_eq!(first.pages.len(), 2);
        world.reset();
        let (same, _) = builder
            .build(&mut world, PagesInput::TypstFile { path: &path }, 2)
            .unwrap();
        for (before, after) in first.pages.iter().zip(&same.pages) {
            assert_eq!(before.key, after.key);
            assert_eq!(before.hash, after.hash);
            assert_eq!(before.svg.as_ptr(), after.svg.as_ptr());
        }
        // Previous indices do not participate in the compiler-key lookup.
        builder.previous = Some(Arc::new(PageSet {
            build_id: 2,
            pages: vec![same.pages[1].clone(), same.pages[0].clone()],
        }));
        world.reset();
        let (moved, _) = builder
            .build(&mut world, PagesInput::TypstFile { path: &path }, 3)
            .unwrap();
        for (before, after) in first.pages.iter().zip(&moved.pages) {
            assert_eq!(before.svg.as_ptr(), after.svg.as_ptr());
            assert_eq!(before.hash, after.hash);
        }
    }

    #[test]
    fn changing_a_later_page_keeps_the_first_pages_storage() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.typ");
        fs::write(&path, "First.\n#pagebreak()\nSecond.\n").unwrap();
        let mut world = world(root.path(), MainFile::Path(path.clone()));
        let mut builder = PagesBuilder::new();
        let (before, _) = builder
            .build(&mut world, PagesInput::TypstFile { path: &path }, 1)
            .unwrap();
        fs::write(&path, "First.\n#pagebreak()\nChanged.\n").unwrap();
        world.reset();
        let (after, _) = builder
            .build(&mut world, PagesInput::TypstFile { path: &path }, 2)
            .unwrap();
        assert_eq!(after.pages.len(), 2);
        assert_eq!(before.pages[0].svg.as_ptr(), after.pages[0].svg.as_ptr());
        assert_eq!(before.pages[0].hash, after.pages[0].hash);
        assert_ne!(before.pages[1].svg.as_ptr(), after.pages[1].svg.as_ptr());
        assert_ne!(before.pages[1].hash, after.pages[1].hash);
    }

    #[test]
    fn failed_generated_compile_keeps_last_good_set_and_emitter_findings() {
        use crate::ir::{Attrs, Block, Inline, SourceSpan};

        let root = tempfile::tempdir().unwrap();
        let (opts, mut prepared) =
            prepared(&root.path().join("document.md"), "A valid page.\n");
        let mut world = world(root.path(), MainFile::Virtual);
        let mut builder = PagesBuilder::new();
        let (good, _) = builder
            .build(
                &mut world,
                PagesInput::Generated { prepared: &prepared, opts: &opts },
                1,
            )
            .unwrap();
        prepared.document.blocks.push(Block::paragraph(vec![Inline::Image {
            alt: vec![Inline::text("Remote image")],
            url: "https://example.org/image.svg".into(),
            title: None,
            attrs: Attrs::default(),
        }]));
        prepared.document.blocks.push(Block::Raw {
            format: "typst".into(),
            text: "#unknown_writer_pages_value()".into(),
            span: Some(SourceSpan {
                start_line: 7,
                end_line: 7,
                cell: None,
                file: Some(prepared.document.source.path.clone()),
            }),
        });
        world.reset();
        let failure = builder
            .build(
                &mut world,
                PagesInput::Generated { prepared: &prepared, opts: &opts },
                2,
            )
            .unwrap_err();
        assert!(Arc::ptr_eq(builder.previous.as_ref().unwrap(), &good));
        assert_eq!(
            failure
                .diagnostics
                .iter()
                .filter(|finding| { finding.code == "unsupported-construct" })
                .count(),
            1
        );
        let errors = failure
            .diagnostics
            .iter()
            .filter(|finding| finding.level == Level::Error)
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1, "{:?}", failure.diagnostics);
        assert_eq!(errors[0].path, Some(prepared.document.source.path.clone()));
        assert_eq!(errors[0].span.as_ref().unwrap().start_line, 7);
    }

    struct WarningInclude;

    impl IncludeProvider for WarningInclude {
        fn synthesize(
            &self, _: &Path, _: &Path,
        ) -> crate::error::Result<Synthesized> {
            Ok(Synthesized {
                text: "Included source.".into(),
                map: SourceMap::default(),
                assets: AssetRegistry::new(),
                files: Vec::new(),
                diagnostics: [Diagnostic::warning(
                    "pages-include-test", "a retained include finding",
                )]
                .into_iter()
                .collect(),
            })
        }
    }

    #[test]
    fn include_provider_findings_are_returned_exactly_once() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.typ");
        fs::write(&path, "#include \"part.md\"\n").unwrap();
        fs::write(root.path().join("part.md"), "Included source.").unwrap();
        let mut world = world(root.path(), MainFile::Path(path.clone()));
        world.set_include_provider(Box::new(WarningInclude));
        let (_, diagnostics) = PagesBuilder::new()
            .build(&mut world, PagesInput::TypstFile { path: &path }, 1)
            .unwrap();
        assert_eq!(
            diagnostics
                .iter()
                .filter(|finding| { finding.code == "pages-include-test" })
                .count(),
            1,
            "{diagnostics:?}"
        );
    }

    struct PanicInclude;

    impl IncludeProvider for PanicInclude {
        fn synthesize(
            &self, _: &Path, _: &Path,
        ) -> crate::error::Result<Synthesized> {
            panic!("expected pages include panic");
        }
    }

    #[test]
    fn compile_panic_does_not_replace_last_good_reuse_state() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.typ");
        fs::write(&path, "A successful page.\n").unwrap();
        let mut world = world(root.path(), MainFile::Path(path.clone()));
        let mut builder = PagesBuilder::new();
        let (good, _) = builder
            .build(&mut world, PagesInput::TypstFile { path: &path }, 1)
            .unwrap();
        fs::write(&path, "#include \"part.md\"\n").unwrap();
        world.set_include_provider(Box::new(PanicInclude));
        world.reset();
        let failed =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                builder.build(
                    &mut world,
                    PagesInput::TypstFile { path: &path },
                    2,
                )
            }));
        assert!(failed.is_err());
        assert!(Arc::ptr_eq(builder.previous.as_ref().unwrap(), &good));
    }

    #[test]
    fn failed_compile_retains_every_finding_and_prefers_mapped_source_paths() {
        use crate::ir::SourceSpan;

        let source = Path::new("/project/document.md");
        let included = Path::new("/project/chapter.ipynb");
        let mut first = Diagnostic::error(
            "typst-error",
            "failed\n  hint: in @local/test:1.0.0/lib.typ:3",
        );
        first.path = Some("/compiler/main.typ".into());
        first.span = Some(SourceSpan {
            start_line: 7,
            end_line: 8,
            cell: Some(4),
            file: Some(included.into()),
        });
        let mut later = Diagnostic::error("typst-error", "another failure");
        later.path = Some("/templates/template.typ".into());
        let compiled = Compiled::<()> {
            output: None,
            diagnostics: [
                Diagnostic::warning("typst-warning", "compiler warning"),
                first,
                later,
            ]
            .into_iter()
            .collect(),
        };
        let before =
            [Diagnostic::warning("emitter-warning", "emitter warning")]
                .into_iter()
                .collect();
        let failure = compiler_result(compiled, before, source).unwrap_err();
        assert_eq!(failure.diagnostics.len(), 4);
        let findings = failure.diagnostics.iter().collect::<Vec<_>>();
        assert_eq!(findings[0].code, "emitter-warning");
        assert_eq!(findings[1].path.as_deref(), Some(source));
        assert_eq!(findings[2].path.as_deref(), Some(included));
        assert_eq!(findings[2].span.as_ref().unwrap().cell, Some(4));
        assert_eq!(
            findings[3].path.as_deref(),
            Some(Path::new("/templates/template.typ"))
        );
        let Error::Compile { path, line, hints, .. } = *failure.error else {
            panic!("compile error");
        };
        assert_eq!(path, included);
        assert_eq!(line, Some(7));
        assert_eq!(hints, ["in @local/test:1.0.0/lib.typ:3"]);
        let missing = compiler_result(
            Compiled::<()> {
                output: None,
                diagnostics: Diagnostics::default(),
            },
            Diagnostics::default(),
            source,
        )
        .unwrap_err();
        assert!(
            matches!(*missing.error, Error::Compile { path, line: None, .. } if path == source)
        );
    }

    #[test]
    fn notebook_compile_error_keeps_the_markdown_cell_location() {
        let root = tempfile::tempdir().unwrap();
        let notebook = serde_json::json!({
            "nbformat": 4, "nbformat_minor": 5, "metadata": {},
            "cells": [
                {"cell_type": "markdown", "metadata": {}, "source": ["# Notebook\n"]},
                {"cell_type": "markdown", "metadata": {}, "source": ["```{=typst}\n#unknown_writer_pages_value()\n```\n"]},
            ],
        });
        let path = root.path().join("document.ipynb");
        let (opts, prepared) =
            prepared(&path, &serde_json::to_string(&notebook).unwrap());
        let mut world = world(root.path(), MainFile::Virtual);
        let failure = PagesBuilder::new()
            .build(
                &mut world,
                PagesInput::Generated { prepared: &prepared, opts: &opts },
                1,
            )
            .unwrap_err();
        let first = failure
            .diagnostics
            .iter()
            .find(|finding| finding.level == Level::Error)
            .unwrap();
        assert_eq!(first.path.as_ref(), Some(&prepared.document.source.path));
        let span = first.span.as_ref().unwrap();
        assert_eq!(span.cell, Some(1));
        assert_eq!(span.start_line, 1);
    }
}
