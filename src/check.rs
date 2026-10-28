//! Document lints and presenter compatibility (markdown-dialect.md §9–10).
//! Frontend and resolver findings are reused; no output is rendered or written.
//! Error conversion and position ordering are adapted from presenter_rs/check.rs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, parse_document};
use serde_json::Value;

use crate::config::Config;
use crate::error::{Diagnostic, Diagnostics, Error, Level, Result};
use crate::ir::{SourceKind, SourceSpan, detect_kind};
use crate::markdown::directives::{
    DIRECTIVE_COLON_REWRITTEN, colon_prepass, parse_directive_info,
};
use crate::markdown::frontmatter::split_front_matter;
use crate::notebook::nbformat::{self, CellType};
use crate::pipeline::{self, BuildFailure, BuildOptions, Mode};

/// Optional checks beyond the frontend and reference resolver.
#[derive(Debug, Clone, Copy, Default)]
pub struct CheckOptions {
    /// Warn about source syntax that presenter reads differently.
    pub compat_presenter: bool,
}

/// Checks a document and every included file without producing output.
///
/// Resolution is always lenient, regardless of the supplied build mode. A
/// stopped preparation retains every finding collected before its failure.
pub fn run(
    opts: &BuildOptions, check: &CheckOptions,
) -> std::result::Result<Diagnostics, BuildFailure> {
    let mut opts = opts.clone();
    opts.mode = Mode::Check;
    let prepared = pipeline::prepare(&opts)?;
    let mut diagnostics = prepared.diagnostics.into_vec();
    if check.compat_presenter {
        for diagnostic in &mut diagnostics {
            if diagnostic.code == DIRECTIVE_COLON_REWRITTEN {
                diagnostic.level = Level::Warning;
            }
        }
        let root = &prepared.document.source;
        let mut seen = BTreeSet::new();
        for path in
            std::iter::once(&root.path).chain(&prepared.document.includes)
        {
            let identity =
                std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            if !seen.insert(identity) {
                continue;
            }
            let result = source_compatibility(
                path,
                if path == &root.path {
                    root.kind
                } else {
                    detect_kind(path).unwrap_or(SourceKind::Markdown)
                },
                &prepared.config,
                &mut diagnostics,
            );
            if let Err(error) = result {
                return Err(BuildFailure {
                    error: Box::new(error),
                    diagnostics: diagnostics.into(),
                });
            }
        }
    }
    sort_by_position(
        &mut diagnostics, &prepared.document.source.path,
        &prepared.document.includes,
    );
    Ok(diagnostics.into())
}

/// Converts a stopped check to a located error finding for text or JSON reports.
pub fn error_diagnostic(error: &Error, input: &Path) -> Diagnostic {
    let code = match error {
        Error::Parse { .. } | Error::FrontMatter(_) | Error::Json(_) => {
            "parse-error"
        }
        Error::Config(_) => "config-error",
        Error::Io(_) => "io-error",
        Error::MissingAsset { .. } => "missing-asset",
        Error::Theme { .. } => "theme-error",
        Error::Template { .. } => "template-error",
        Error::Style { .. } => "style-error",
        Error::Compile { .. } => "compile-error",
        Error::Resource { .. } => "resource-error",
        Error::Other(_) => "error",
    };
    let span = error.span();
    let path = error
        .path()
        .or_else(|| span.as_ref().and_then(|span| span.file.as_deref()))
        .unwrap_or(input);
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut diagnostic =
        Diagnostic::error(code, error.message().into_owned()).with_path(path);
    diagnostic.span = span;
    diagnostic
}

/// Stable source order, adapted from presenter's check implementation.
fn sort_by_position(
    diagnostics: &mut [Diagnostic], root: &Path, includes: &[PathBuf],
) {
    diagnostics.sort_by_key(|diagnostic| {
        let position =
            diagnostic.span.as_ref().map(|span| (span.cell, span.start_line));
        let path = diagnostic.path.as_deref()?;
        if path == root {
            return Some((0, position));
        }
        let index = includes.iter().position(|included| included == path)?;
        Some((index + 1, position))
    });
}

fn source_compatibility(
    path: &Path, kind: SourceKind, config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        Error::Io(std::io::Error::new(
            error.kind(),
            format!(
                "cannot read {} for compatibility checks: {error}",
                path.display()
            ),
        ))
    })?;
    if kind != SourceKind::Notebook {
        author_list_compatibility(path, &text, None, diagnostics);
        markdown_compatibility(path, &text, config, None, true, diagnostics);
        return Ok(());
    }
    let (notebook, _) = nbformat::parse_notebook(path, &text)?;
    let first = notebook
        .cells
        .iter()
        .position(|cell| cell.cell_type == CellType::Markdown);
    for (index, cell) in notebook.cells.iter().enumerate() {
        let raw_markdown = cell.cell_type == CellType::Raw
            && matches!(
                cell.raw_mimetype()
                    .unwrap_or("")
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str(),
                "markdown" | "text/markdown" | "text/x-markdown"
            );
        if cell.cell_type == CellType::Markdown || raw_markdown {
            let first_header = first == Some(index);
            let header = cell.cell_type == CellType::Markdown
                && (first_header
                    || split_front_matter(cell.source.as_str()).is_some_and(
                        |(yaml, _, _)| {
                            crate::notebook::convert::later_metadata_header(
                                yaml,
                            )
                        },
                    ));
            if first_header {
                author_list_compatibility(
                    path,
                    cell.source.as_str(),
                    Some(index),
                    diagnostics,
                );
            }
            markdown_compatibility(
                path,
                cell.source.as_str(),
                config,
                Some(index),
                header,
                diagnostics,
            );
        }
    }
    Ok(())
}

fn markdown_compatibility(
    path: &Path, text: &str, config: &Config, cell: Option<usize>,
    front_matter: bool, diagnostics: &mut Vec<Diagnostic>,
) {
    let mut options = crate::markdown::comrak_options(&config.markdown);
    options.extension.front_matter_delimiter =
        front_matter.then(|| "---".into());
    let arena = Arena::new();
    let (rewritten, _) = colon_prepass(text, &options);
    let root = parse_document(&arena, &rewritten, &options);
    let lines: Vec<_> = text.lines().collect();
    let nodes: Vec<_> =
        root.children().filter(|node| significant(node)).collect();
    let boundary = |node: &AstNode<'_>| {
        let data = node.data();
        matches!(data.value, NodeValue::ThematicBreak)
            && lines
                .get(data.sourcepos.start.line - 1)
                .is_some_and(|line| line.trim() == "---")
    };
    for (index, node) in nodes.iter().enumerate() {
        let data = node.data();
        if !matches!(&data.value, NodeValue::BlockDirective(directive)
            if parse_directive_info(&directive.info).name == "include")
        {
            continue;
        }
        let before = index == 0 || boundary(nodes[index - 1]);
        let after = index + 1 == nodes.len() || boundary(nodes[index + 1]);
        if before && after && !node.children().any(|node| significant(node)) {
            continue;
        }
        diagnostics.push(
            Diagnostic::warning(
                "presenter-include-position",
                "presenter requires an include to be the only content between `---` rules; put a rule before and after it",
            )
            .with_path(path)
            .with_span(SourceSpan {
                start_line: data.sourcepos.start.line,
                end_line: data.sourcepos.end.line,
                cell,
                file: None,
            }),
        );
    }
}

fn significant(node: &AstNode<'_>) -> bool {
    match &node.data().value {
        NodeValue::FrontMatter(_) | NodeValue::FootnoteDefinition(_) => false,
        NodeValue::HtmlBlock(html) => !only_comments(&html.literal),
        NodeValue::BlockDirective(directive) => !matches!(
            parse_directive_info(&directive.info).name.as_str(),
            "notes" | "comment"
        ),
        _ => true,
    }
}

fn only_comments(mut text: &str) -> bool {
    while let Some(rest) = text.trim_start().strip_prefix("<!--") {
        let Some(end) = rest.find("-->") else {
            return true;
        };
        text = &rest[end + 3..];
    }
    text.trim().is_empty()
}

fn author_list_compatibility(
    path: &Path, text: &str, cell: Option<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((yaml, _, closing)) = split_front_matter(text) else {
        return;
    };
    let Ok(Value::Object(meta)) = serde_norway::from_str(yaml) else {
        return;
    };
    let Some(key) = ["author", "authors"]
        .into_iter()
        .find(|key| meta.get(*key).is_some_and(Value::is_array))
    else {
        return;
    };
    let mut offset = 0;
    let line = yaml.split_inclusive('\n').position(|line| {
        let prefix = &yaml[..offset];
        offset += line.len();
        !line.starts_with([' ', '\t'])
            && line.split_once(':').is_some_and(|(name, _)| {
                name.trim_end().trim_matches(['\'', '"']) == key
            })
            && matches!(
                serde_norway::from_str::<Value>(prefix),
                Ok(Value::Null | Value::Object(_))
            )
    });
    diagnostics.push(
        Diagnostic::warning(
            "presenter-front-matter",
            "presenter accepts a scalar `author`, not an author list",
        )
        .with_path(path)
        .with_span(SourceSpan {
            start_line: line.map_or(1, |line| line + 2),
            end_line: line.map_or(closing, |line| line + 2),
            cell,
            file: None,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn compatibility(text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        author_list_compatibility(
            Path::new("source.md"),
            text,
            None,
            &mut diagnostics,
        );
        markdown_compatibility(
            Path::new("source.md"),
            text,
            &Config::default(),
            None,
            true,
            &mut diagnostics,
        );
        diagnostics
    }

    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        for (name, text) in files {
            let path = directory.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        directory
    }

    #[test]
    fn include_positions_use_real_rules_and_file_edges() {
        for text in [
            ":::include part.md\n:::\n",
            "---\n:::include part.md\n:::\n",
            ":::include part.md\n:::\n\n---\n",
            "Before.\n\n---\n\n:::include part.md\n:::\n\n---\n\nAfter.",
            "<!-- comment -->\n\n:::notes\nIgnored.\n:::\n\n:::include part.md\n:::\n\n:::comment\nIgnored.\n:::\n",
            "---\ntitle: Header\n---\n\n:::include part.md\n:::\n",
        ] {
            assert!(compatibility(text).is_empty(), "{text}");
        }
        for text in [
            "Before.\n\n:::include part.md\n:::\n",
            ":::include part.md\n:::\n\nAfter.",
            ":::include part.md\nBody.\n:::\n",
            "Title\n---\n\n:::include part.md\n:::\n",
            "```md\n---\n```\n\n:::include part.md\n:::\n",
            "* * *\n\n:::include part.md\n:::\n",
            "- ---\n\n:::include part.md\n:::\n",
            "<div>\nVisible HTML.\n</div>\n\n:::include part.md\n:::\n",
        ] {
            let diagnostics = compatibility(text);
            assert_eq!(diagnostics.len(), 1, "{text}: {diagnostics:?}");
            assert_eq!(diagnostics[0].code, "presenter-include-position");
        }
    }

    #[test]
    fn compatibility_ignores_nested_includes_and_code_or_notes() {
        let text = "> :::include part.md\n> :::\n\n- :::include part.md\n  :::\n\n:::note\n:::include part.md\n:::\n\n```md\n:::include part.md\n:::\n```\n\n<!-- :::include part.md -->\n\n:::notes\n:::include part.md\n:::\n";
        assert!(compatibility(text).is_empty());
        for text in [
            "![A figure](image.png)\n\nTable: Caption\n\n@fig-x\n",
            ":::toc\n:::\n\n```{=latex}\n\\clearpage\n```\n\nInline ^[note].",
        ] {
            assert!(compatibility(text).is_empty());
        }
    }

    #[test]
    fn author_compatibility_distinguishes_scalars_and_both_list_keys() {
        assert!(compatibility("---\nauthor: Ada\n---\n\nText.").is_empty());
        for field in [
            "author: [Ada, Grace]", "authors: [Ada]", "authors: []",
            "'authors':\n  - name: Ada",
        ] {
            let diagnostics = compatibility(&format!("---\n{field}\n---\n"));
            assert_eq!(diagnostics.len(), 1, "{field}");
            assert_eq!(diagnostics[0].code, "presenter-front-matter");
            assert_eq!(diagnostics[0].span.as_ref().unwrap().start_line, 2);
        }
    }

    #[test]
    fn author_locations_skip_nested_keys_and_fall_back_for_complex_headers() {
        let diagnostics = compatibility(
            "---\ndescription: |\n  authors: literal text\ncustom:\n  authors: [Nested]\nauthors: [Ada]\n---\n",
        );
        assert_eq!(diagnostics.len(), 1);
        let span = diagnostics[0].span.as_ref().unwrap();
        assert_eq!((span.start_line, span.end_line), (6, 6));
        for yaml in ["  authors: [Ada]", "{authors: [Ada], title: Flow}"] {
            let diagnostics = compatibility(&format!("---\n{yaml}\n---\n"));
            assert_eq!(diagnostics.len(), 1);
            let span = diagnostics[0].span.as_ref().unwrap();
            assert_eq!((span.start_line, span.end_line), (1, 3));
        }
    }

    #[test]
    fn checks_force_lenient_resolution_and_leave_outputs_unwritten() {
        let directory = project(&[(
            "main.md",
            "See @missing.\n\n![Missing image](missing.png)\n",
        )]);
        let opts = BuildOptions::new(directory.path().join("main.md"));
        let diagnostics = run(&opts, &CheckOptions::default()).unwrap();
        assert_eq!(opts.mode, Mode::Build);
        assert!(diagnostics.iter().any(|d| d.code == "unresolved-reference"));
        assert!(diagnostics.iter().any(|d| d.code == "missing-asset"));
        assert!(diagnostics.iter().all(|d| d.level == Level::Warning));
        assert!(!directory.path().join("dist").exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn compat_promotes_frontend_colon_findings_once_and_checks_included_files()
    {
        let directory = project(&[
            (
                "main.md",
                ":::include part.md\n:::\n\n---\n\n:::include part.md\n:::\n",
            ),
            (
                "part.md",
                "---\nauthors: [Ada]\n---\n\n:::callout title=\"Figure: one\"\nBody.\n:::\n",
            ),
        ]);
        let opts = BuildOptions::new(directory.path().join("main.md"));
        let ordinary = run(&opts, &CheckOptions::default()).unwrap();
        let diagnostics =
            run(&opts, &CheckOptions { compat_presenter: true }).unwrap();
        let original: Vec<_> = ordinary
            .iter()
            .filter(|d| d.code == DIRECTIVE_COLON_REWRITTEN)
            .collect();
        assert_eq!(original.len(), 1);
        assert_eq!(original[0].level, Level::Info);
        let promoted: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == DIRECTIVE_COLON_REWRITTEN)
            .collect();
        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].level, Level::Warning);
        let authors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "presenter-front-matter")
            .collect();
        assert_eq!(authors.len(), 1);
        assert_eq!(
            authors[0].path,
            Some(
                std::fs::canonicalize(directory.path().join("part.md"))
                    .unwrap()
            )
        );
    }

    #[test]
    fn notebook_compatibility_scans_source_cells_and_only_the_first_header() {
        let source = json!({"nbformat": 4, "metadata": {"authors": ["Metadata"]}, "cells": [
            {"cell_type": "code", "source": ":::include ignored.md\n:::\n", "execution_count": 1},
            {"cell_type": "markdown", "source": "---\nauthors: [Ada]\n---\n"},
            {"cell_type": "markdown", "source": "---\nauthors: [Ignored]\n---\n"},
            {"cell_type": "markdown", "metadata": {"tags": ["remove-cell"]}, "source": "Before.\n\n:::include removed.md\n:::\n"},
            {"cell_type": "raw", "metadata": {"raw_mimetype": "text/markdown; charset=utf-8"}, "source": "Before.\n\n:::include raw.md\n:::\n"},
            {"cell_type": "raw", "metadata": {"raw_mimetype": "text/html"}, "source": "Before.\n\n:::include ignored.md\n:::\n"},
        ]}).to_string();
        let directory = project(&[("cells.ipynb", &source)]);
        let path = directory.path().join("cells.ipynb");
        let mut diagnostics = Vec::new();
        source_compatibility(
            &path,
            SourceKind::Notebook,
            &Config::default(),
            &mut diagnostics,
        )
        .unwrap();
        let locations: Vec<_> = diagnostics
            .iter()
            .map(|d| {
                let span = d.span.as_ref().unwrap();
                (d.code, span.cell, span.start_line)
            })
            .collect();
        assert_eq!(
            locations,
            [
                ("presenter-front-matter", Some(1), 2),
                ("presenter-include-position", Some(3), 3),
                ("presenter-include-position", Some(4), 3),
            ]
        );
    }

    #[test]
    fn later_known_headers_hide_scalar_contents_but_unknown_and_raw_stay_literal()
     {
        let header = "---\nabstract: |\n  Context.\n\n  :::include hidden.md\n  :::\n---\n\nAfter.";
        let source = json!({"nbformat": 4, "cells": [
            {"cell_type": "markdown", "source": "First."},
            {"cell_type": "markdown", "source": header},
        ]})
        .to_string();
        let directory = project(&[("cells.ipynb", &source)]);
        let path = directory.path().join("cells.ipynb");
        let diagnostics = run(
            &BuildOptions::new(&path),
            &CheckOptions { compat_presenter: true },
        )
        .unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "notebook-front-matter-ignored")
        );
        assert!(!diagnostics.iter().any(|d| d.code.starts_with("presenter-")));

        let unknown = header.replace("abstract:", "custom:");
        let source = json!({"nbformat": 4, "cells": [
            {"cell_type": "markdown", "source": "First."},
            {"cell_type": "markdown", "source": header},
            {"cell_type": "markdown", "source": unknown},
            {"cell_type": "raw", "metadata": {"raw_mimetype": "text/markdown"}, "source": header},
        ]}).to_string();
        std::fs::write(&path, source).unwrap();
        let mut diagnostics = Vec::new();
        source_compatibility(
            &path,
            SourceKind::Notebook,
            &Config::default(),
            &mut diagnostics,
        )
        .unwrap();
        let locations: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| {
                let span = diagnostic.span.as_ref().unwrap();
                (diagnostic.code, span.cell, span.start_line)
            })
            .collect();
        assert_eq!(
            locations,
            [
                ("presenter-include-position", Some(2), 5),
                ("presenter-include-position", Some(3), 5),
            ]
        );
        assert!(crate::notebook::convert::later_metadata_header(
            "title: Later"
        ));
        assert!(crate::notebook::convert::later_metadata_header(
            "markdown: {references: false}"
        ));
        assert!(!crate::notebook::convert::later_metadata_header(
            "custom: Unknown"
        ));
        assert!(!crate::notebook::convert::later_metadata_header(
            "authors: invalid-shape"
        ));
    }

    #[test]
    fn fatal_error_conversion_keeps_notebook_and_included_locations() {
        let input = Path::new("root.ipynb");
        let error = Error::Parse {
            path: PathBuf::from("included.ipynb"),
            line: None,
            message: "cell 4, line 3: malformed include".into(),
        };
        let diagnostic = error_diagnostic(&error, input);
        assert_eq!(diagnostic.code, "parse-error");
        assert_eq!(diagnostic.message, "malformed include");
        assert_eq!(diagnostic.span.as_ref().unwrap().cell, Some(4));
        assert_eq!(diagnostic.span.as_ref().unwrap().start_line, 3);
        assert!(diagnostic.path.unwrap().ends_with("included.ipynb"));

        let error = Error::FrontMatter(
            "cell 2: invalid YAML at line 5 column 7".into(),
        );
        let diagnostic = error_diagnostic(&error, input);
        assert_eq!(diagnostic.code, "parse-error");
        assert_eq!(diagnostic.span.as_ref().unwrap().cell, Some(2));
        assert_eq!(diagnostic.span.as_ref().unwrap().start_line, 5);

        let error = Error::MissingAsset {
            path: PathBuf::from("missing.png"),
            span: Some(SourceSpan {
                start_line: 8,
                end_line: 8,
                cell: None,
                file: Some(PathBuf::from("included.md")),
            }),
        };
        let diagnostic = error_diagnostic(&error, input);
        assert_eq!(diagnostic.code, "missing-asset");
        assert!(diagnostic.path.unwrap().ends_with("included.md"));
    }

    #[test]
    fn failures_keep_library_errors_available_to_the_command() {
        let directory = project(&[("main.md", ":::include missing.md\n:::\n")]);
        let opts = BuildOptions::new(directory.path().join("main.md"));
        let failure = run(&opts, &CheckOptions::default()).unwrap_err();
        assert!(matches!(*failure.error, Error::Parse { line: Some(1), .. }));
        let diagnostic = error_diagnostic(&failure.error, &opts.input);
        assert_eq!(diagnostic.code, "parse-error");
        assert!(diagnostic.message.contains("file not found"));
    }

    #[test]
    fn source_order_is_stable_for_unlocated_root_and_included_findings() {
        let located = |code, path: &str, cell, line| {
            Diagnostic::warning(code, code).with_path(path).with_span(
                SourceSpan {
                    start_line: line,
                    end_line: line,
                    cell,
                    file: None,
                },
            )
        };
        let mut diagnostics = vec![
            located("child-later", "child.md", None, 8),
            located("root-cell", "main.ipynb", Some(1), 1),
            located("tie-first", "main.ipynb", Some(0), 3),
            located("tie-second", "main.ipynb", Some(0), 3),
            Diagnostic::info("unlocated", "unlocated").with_path("main.ipynb"),
            located("child-earlier", "child.md", None, 2),
        ];
        sort_by_position(
            &mut diagnostics,
            Path::new("main.ipynb"),
            &[PathBuf::from("child.md")],
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            [
                "unlocated", "tie-first", "tie-second", "root-cell",
                "child-earlier", "child-later"
            ]
        );
    }
}
