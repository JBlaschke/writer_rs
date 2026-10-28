//! Include composition contracts, adapted from presenter_rs/tests/include.rs.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use writer::config::Config;
use writer::error::{Error, Level, Result};
use writer::ir::{
    self, Block, Document, Inline, Parsed, SourceInfo, SourceKind,
};
use writer::markdown;
use writer::notebook;
use writer::resolve::{ResolveOptions, resolve};

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (name, text) in files {
        let path = directory.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    directory
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

fn source(path: &Path) -> SourceInfo {
    let path = canonical(path);
    SourceInfo {
        base_dir: path.parent().unwrap().to_path_buf(),
        kind: ir::detect_kind(&path).unwrap_or(SourceKind::Markdown),
        path,
    }
}

fn parse(path: &Path) -> Result<Parsed> {
    parse_with(path, &Config::default())
}

fn parse_with(path: &Path, config: &Config) -> Result<Parsed> {
    let source = source(path);
    let text = std::fs::read_to_string(&source.path).unwrap();
    let parsed = match source.kind {
        SourceKind::Notebook => notebook::parse(&source, &text, config),
        _ => markdown::parse(&source, &text, config),
    }?;
    assert!(parsed.document.invariant_violations().is_empty());
    Ok(parsed)
}

fn notebook(text: &str, metadata: Value) -> String {
    json!({
        "nbformat": 4, "nbformat_minor": 5, "metadata": metadata,
        "cells": [{"cell_type": "markdown", "source": text, "metadata": {}}],
    })
    .to_string()
}

fn text(document: &Document) -> String {
    let mut text = Vec::new();
    ir::walk_blocks(document, &mut |block| match block {
        Block::Heading { inlines, .. } | Block::Paragraph { inlines, .. } => {
            text.push(ir::plain_text(inlines))
        }
        _ => {}
    });
    text.join("\n")
}

fn error(directory: &Path, markdown: &str) -> (PathBuf, Option<usize>, String) {
    let path = directory.join("main.md");
    std::fs::write(&path, markdown).unwrap();
    match parse(&path) {
        Err(Error::Parse { path, line, message }) => (path, line, message),
        Err(other) => panic!("expected located parse error: {other}"),
        Ok(_) => panic!("expected include failure"),
    }
}

#[test]
fn markdown_includes_at_any_top_level_position_and_merges_in_order() {
    let directory = project(&[
        (
            "main.md",
            ":::include first.md\n:::\n\nRoot paragraph.\n\n# Root heading\n\n:::include parts/middle.md\n:::\n\nLast paragraph.\n\n:::include last.markdown\n:::\n",
        ),
        ("first.md", "# First\n"),
        ("parts/middle.md", "Middle.\n\n:::include nested.md\n:::\n"),
        ("parts/nested.md", "Nested.\n"),
        ("last.markdown", "Last include.\n"),
    ]);
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    assert_eq!(
        text(&parsed.document),
        "First\nRoot paragraph.\nRoot heading\nMiddle.\nNested.\nLast paragraph.\nLast include."
    );
    let expected: Vec<_> =
        ["first.md", "parts/middle.md", "parts/nested.md", "last.markdown"]
            .iter()
            .map(|name| canonical(&directory.path().join(name)))
            .collect();
    assert_eq!(parsed.document.includes, expected);
    ir::walk_blocks(&parsed.document, &mut |block| {
        let span = block.span().unwrap();
        if let Some(file) = &span.file {
            assert!(parsed.document.includes.contains(file));
        }
    });
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn quoted_and_absolute_paths_resolve_at_the_including_directory() {
    let directory = project(&[
        ("main.md", ""),
        ("parts/with spaces.md", "Quoted.\n\n:::include nested.md\n:::\n"),
        ("parts/nested.md", "Nested relative path.\n"),
        ("absolute.md", "Absolute.\n"),
    ]);
    let absolute = canonical(&directory.path().join("absolute.md"));
    let source = format!(
        ":::include \"parts/with spaces.md\"\n:::\n\n:::include \"{}\"\n:::\n",
        absolute.display()
    );
    std::fs::write(directory.path().join("main.md"), source).unwrap();
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    assert_eq!(
        text(&parsed.document),
        "Quoted.\nNested relative path.\nAbsolute."
    );
}

#[test]
fn included_assets_and_spans_keep_each_files_base() {
    let directory = project(&[
        ("main.md", ":::include parts/part.md\n:::\n"),
        (
            "parts/part.md",
            "# Part {#sec-part}\n\n![Image](img/pixel.png)\n\nSee @sec-part.\n",
        ),
    ]);
    std::fs::create_dir(directory.path().join("parts/img")).unwrap();
    std::fs::write(
        directory.path().join("parts/img/pixel.png"),
        include_bytes!("fixtures/md/images/plot.png"),
    )
    .unwrap();
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    assert_eq!(parsed.assets.len(), 1);
    let included = canonical(&directory.path().join("parts/part.md"));
    ir::walk_blocks(&parsed.document, &mut |block| {
        assert_eq!(block.span().unwrap().file.as_ref(), Some(&included));
    });
    ir::walk_inlines(&parsed.document, &mut |inline| {
        if let Inline::Ref { span, .. } = inline {
            assert_eq!(span.as_ref().unwrap().file.as_ref(), Some(&included));
        }
    });
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn markdown_and_notebook_includes_share_one_chain_and_file_scopes() {
    let middle = notebook(
        "Notebook[^1].\n\n[^1]: Notebook note.\n\n:::include part.md\n:::\n",
        json!({}),
    );
    let directory = project(&[
        (
            "main.md",
            "Main[^1].\n\n[^1]: Main note.\n\n:::include middle.ipynb\n:::\n",
        ),
        ("middle.ipynb", &middle),
        ("part.md", "Part[^1].\n\n[^1]: Part note.\n"),
    ]);
    let mut parsed = parse(&directory.path().join("main.md")).unwrap();
    assert_eq!(
        parsed.document.includes,
        [
            canonical(&directory.path().join("middle.ipynb")),
            canonical(&directory.path().join("part.md"))
        ]
    );
    assert!(parsed.document.features.notebook);
    let diagnostics = resolve(
        &mut parsed.document,
        &ResolveOptions {
            config: &Config::default(),
            library: None,
            lenient: true,
        },
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(parsed.document.resolved.footnotes, 3);
    assert!(text(&parsed.document).contains("Part note"));
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn include_cycles_cross_markdown_and_notebooks() {
    let middle = notebook(":::include main.md\n:::\n", json!({}));
    let directory = project(&[("middle.ipynb", &middle)]);
    let (path, line, message) =
        error(directory.path(), ":::include middle.ipynb\n:::\n");
    assert_eq!(path, canonical(&directory.path().join("middle.ipynb")));
    assert_eq!(line, None);
    assert!(message.contains("cell 0, line 1: include cycle"));
    assert!(message.contains("main.md → middle.ipynb → main.md"), "{message}");
}

#[test]
fn included_metadata_is_ignored_but_child_parsing_tables_apply() {
    let directory = project(&[
        (
            "main.md",
            "---\ntitle: Parent\ntoc: 2\nmarkdown: {superscript: false}\n---\n\n:::include part.md\n:::\n",
        ),
        (
            "part.md",
            "---\ntitle: Child\ntoc: 5\nmarkdown: {superscript: true}\n---\n\n# Child section\n\nx^2^\n\n:::toc\n:::\n",
        ),
    ]);
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    assert_eq!(parsed.document.meta.title.as_deref(), Some("Parent"));
    let mut toc = Vec::new();
    let mut superscript = false;
    ir::walk_blocks(&parsed.document, &mut |block| {
        if let Block::Toc { depth, .. } = block {
            toc.push(*depth);
        }
    });
    ir::walk_inlines(&parsed.document, &mut |inline| {
        superscript |= matches!(inline, Inline::Superscript { .. })
    });
    assert_eq!(toc, [5], "only the child's explicit TOC remains");
    assert!(superscript);
    let ignored: Vec<_> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == "include-meta-ignored")
        .collect();
    assert_eq!(ignored.len(), 1);
    assert!(ignored[0].message.contains("`title`"));
    assert!(!ignored[0].message.contains("`markdown`"));
    let directory = project(&[
        ("main.md", ":::include part.md\n:::\n"),
        ("part.md", "---\ntoc: true\n---\n\nChild.\n"),
    ]);
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    assert!(
        !parsed
            .document
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Toc { .. }))
    );
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn explicit_empty_lists_are_differing_metadata() {
    let directory = project(&[
        (
            "main.md",
            "---\nauthors: [Ada]\nkeywords: [paper]\n---\n\n:::include part.md\n:::\n",
        ),
        ("part.md", "---\nauthors: []\nkeywords: []\n---\n\nPart.\n"),
    ]);
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    let ignored: Vec<_> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == "include-meta-ignored")
        .collect();
    assert_eq!(ignored.len(), 1);
    assert!(ignored[0].message.contains("`authors`"));
    assert!(ignored[0].message.contains("`keywords`"));
    assert_eq!(parsed.document.meta.authors[0].name, "Ada");
}

#[test]
fn matching_metadata_and_only_config_overrides_are_silent() {
    let directory = project(&[
        (
            "main.md",
            "---\ntitle: Shared\nevent: Conference\n---\n\n:::include part.md\n:::\n",
        ),
        (
            "part.md",
            "---\ntitle: Shared\nevent: Conference\nmarkdown: {superscript: true}\n---\n\nPart.\n",
        ),
    ]);
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
}

#[test]
fn in_memory_notebook_metadata_is_passed_to_its_hook() {
    let directory =
        project(&[("part.md", "---\ntitle: Same title\n---\n\nPart.\n")]);
    let source = SourceInfo {
        path: directory.path().join("not-on-disk.ipynb"),
        base_dir: directory.path().to_path_buf(),
        kind: SourceKind::Notebook,
    };
    let json = notebook(
        ":::include part.md\n:::\n",
        json!({"writer": {"title": "Same title"}}),
    );
    let parsed = notebook::parse(&source, &json, &Config::default()).unwrap();
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.document.meta.title.as_deref(), Some("Same title"));
    assert_eq!(text(&parsed.document), "Part.");
}

#[test]
fn repeated_includes_preserve_content_and_report_source_diagnostics_once() {
    let directory = project(&[
        ("main.md", ":::include part.md\n:::\n\n:::include part.md\n:::\n"),
        ("part.md", "# Part {#bad!}\n\nPart content.\n"),
    ]);
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    assert_eq!(text(&parsed.document).matches("Part content").count(), 2);
    assert_eq!(parsed.document.includes.len(), 1);
    assert_eq!(
        parsed.diagnostics.iter().filter(|d| d.code == "label-invalid").count(),
        1
    );
    let finding =
        parsed.diagnostics.iter().find(|d| d.code == "label-invalid").unwrap();
    assert_eq!(
        finding.path.as_ref(),
        Some(&canonical(&directory.path().join("part.md")))
    );
    assert_eq!(finding.span.as_ref().unwrap().file, finding.path);
}

#[test]
fn nesting_is_rejected_in_each_fragment_container() {
    for text in [
        "- Item\n\n  :::include missing.md\n  :::\n",
        "> :::include missing.md\n> :::\n",
        "::::note\n:::include missing.md\n:::\n::::\n",
        "Note[^1].\n\n[^1]: Footnote.\n\n    :::include missing.md\n    :::\n",
        "---\nabstract: |\n  :::include missing.md\n  :::\n---\nBody.\n",
    ] {
        let directory = project(&[("main.md", text)]);
        let parsed = parse(&directory.path().join("main.md")).unwrap();
        let errors: Vec<_> = parsed
            .diagnostics
            .iter()
            .filter(|d| d.code == "include-not-top-level")
            .collect();
        assert_eq!(errors.len(), 1, "{text}");
        assert_eq!(errors[0].level, Level::Error);
        assert!(parsed.document.includes.is_empty());
    }
    let directory = project(&[(
        "main.ipynb",
        &notebook("- Item\n\n  :::include missing.md\n  :::\n", json!({})),
    )]);
    let parsed = parse(&directory.path().join("main.ipynb")).unwrap();
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|d| d.code == "include-not-top-level"
                && d.level == Level::Error)
    );
}

#[test]
fn a_leaf_body_follows_the_included_blocks() {
    let directory = project(&[
        (
            "main.md",
            "Before.\n\n:::include part.md\nBody follows.\n:::\n\nAfter.\n",
        ),
        ("part.md", "Included.\n"),
    ]);
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    assert_eq!(
        text(&parsed.document),
        "Before.\nIncluded.\nBody follows.\nAfter."
    );
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|d| d.code == "invalid-directive")
            .count(),
        1
    );
}

#[test]
fn failures_are_located_at_the_directive() {
    let directory =
        project(&[("part.typ", "Typst"), ("directory.md/placeholder", "")]);
    for (target, expected) in [
        ("missing.md", "file not found"),
        ("part.typ", "only Markdown"),
        ("https://example.org/a.md", "not URLs"),
        ("directory.md", "cannot read"),
        ("", "needs the path"),
        ("\"unterminated.md", "unbalanced"),
    ] {
        let (path, line, message) = error(
            directory.path(),
            &format!("Before.\n\n:::include {target}\n:::\n"),
        );
        assert_eq!(path, canonical(&directory.path().join("main.md")));
        assert_eq!(line, Some(3));
        assert!(message.contains(expected), "{message}");
    }
    std::fs::write(directory.path().join("invalid.md"), [0xff, 0xfe]).unwrap();
    let (_, line, message) =
        error(directory.path(), ":::include invalid.md\n:::\n");
    assert_eq!(line, Some(1));
    assert!(message.contains("not valid UTF-8"));
}

#[test]
fn errors_within_a_file_keep_that_files_path() {
    let directory = project(&[("part.md", "---\nauthors: not-a-list\n---\n")]);
    let (path, line, message) =
        error(directory.path(), ":::include part.md\n:::\n");
    assert_eq!(path, canonical(&directory.path().join("part.md")));
    assert_eq!(line, Some(2));
    assert!(message.contains("authors"));
}

#[test]
fn eight_include_levels_are_allowed_and_nine_are_rejected() {
    let directory = project(&[]);
    for index in 0..=8 {
        let extension = if index % 2 == 0 { "md" } else { "ipynb" };
        let next_extension = if index % 2 == 0 { "ipynb" } else { "md" };
        let text = if index == 8 {
            "Bottom.".to_owned()
        } else {
            format!(":::include level{}.{}\n:::\n", index + 1, next_extension)
        };
        let text = if extension == "ipynb" {
            notebook(&text, json!({}))
        } else {
            text
        };
        std::fs::write(
            directory.path().join(format!("level{index}.{extension}")),
            text,
        )
        .unwrap();
    }
    let parsed = parse(&directory.path().join("level0.md")).unwrap();
    assert_eq!(parsed.document.includes.len(), 8);
    std::fs::write(
        directory.path().join("level8.md"),
        ":::include level9.md\n:::\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("level9.md"), "Too deep.").unwrap();
    match parse(&directory.path().join("level0.md")) {
        Err(Error::Parse { path, line, message }) => {
            assert_eq!(path, canonical(&directory.path().join("level8.md")));
            assert_eq!(line, Some(1));
            assert!(message.contains("more than 8"));
        }
        _ => panic!("ninth level must fail"),
    }
}

#[test]
fn global_labels_still_collide_across_files() {
    let directory = project(&[
        ("main.md", "# Root {#same}\n\n:::include part.md\n:::\n"),
        ("part.md", "# Part {#same}\n"),
    ]);
    let mut parsed = parse(&directory.path().join("main.md")).unwrap();
    let diagnostics = resolve(
        &mut parsed.document,
        &ResolveOptions {
            config: &Config::default(),
            library: None,
            lenient: true,
        },
    );
    assert!(diagnostics.iter().any(|d| d.code == "duplicate-label"));
}

#[test]
fn watcher_scans_notebook_cells_and_raw_markdown_but_not_code_or_outputs() {
    let json = json!({"nbformat": 4, "cells": [
        {"cell_type": "markdown", "source": ":::include part.md\n:::\n"},
        {"cell_type": "raw", "metadata": {"format": "text/markdown"}, "source": ":::include raw.md\n:::\n"},
        {"cell_type": "code", "source": ":::include code.md\n:::\n", "outputs": [{"output_type": "display_data", "data": {"text/markdown": ":::include output.md\n:::\n"}}]},
    ]}).to_string();
    let directory = project(&[
        (
            "main.txt",
            ":::include cells.ipynb\n:::\n\n:::include missing.md\n:::\n",
        ),
        ("cells.ipynb", &json),
        ("part.md", ":::include nested.md\n:::\n"),
        ("raw.md", "Raw."),
        ("nested.md", "Nested."),
    ]);
    let root = canonical(directory.path());
    let paths = markdown::include::scan_includes(&root.join("main.txt"));
    assert_eq!(
        paths,
        ["cells.ipynb", "part.md", "nested.md", "raw.md", "missing.md"]
            .map(|path| root.join(path))
    );
}

#[cfg(unix)]
#[test]
fn directory_symlink_parent_components_keep_filesystem_semantics() {
    let body = ":::include shortcut/../part.md\n:::\n";
    let directory = project(&[
        ("main.md", body),
        ("main.ipynb", &notebook(body, json!({}))),
        ("part.md", "Wrong lexical target."),
        ("actual/part.md", "Actual target.\n\n:::include nested.md\n:::\n"),
        ("actual/nested.md", "Nested actual target."),
        ("actual/deep/placeholder", ""),
    ]);
    let root = canonical(directory.path());
    std::os::unix::fs::symlink(root.join("actual/deep"), root.join("shortcut"))
        .unwrap();
    let expected = [root.join("actual/part.md"), root.join("actual/nested.md")];
    for extension in ["md", "ipynb"] {
        let path = root.join(format!("main.{extension}"));
        let parsed = parse(&path).unwrap();
        assert_eq!(
            text(&parsed.document),
            "Actual target.\nNested actual target."
        );
        assert_eq!(parsed.document.includes, expected);
        assert_eq!(markdown::include::scan_includes(&path), expected);
    }
    assert_eq!(
        markdown::include::scan_includes(&root.join("shortcut/../part.md")),
        [root.join("actual/nested.md")]
    );

    std::fs::write(
        root.join("actual/part.md"),
        ":::include ../shortcut/../part.md\n:::\n",
    )
    .unwrap();
    match parse(&root.join("main.md")) {
        Err(Error::Parse { path, line, message }) => {
            assert_eq!(path, root.join("actual/part.md"));
            assert_eq!(line, Some(1));
            assert!(message.contains("include cycle"));
        }
        _ => panic!("directory alias cycle must fail"),
    }
    assert_eq!(
        markdown::include::scan_includes(&root.join("main.md")),
        [root.join("actual/part.md")]
    );
}

#[cfg(unix)]
#[test]
fn canonical_paths_detect_symlink_cycles() {
    let directory = project(&[("main.md", ":::include alias.md\n:::\n")]);
    std::os::unix::fs::symlink(
        directory.path().join("main.md"),
        directory.path().join("alias.md"),
    )
    .unwrap();
    match parse(&directory.path().join("main.md")) {
        Err(Error::Parse { line, message, .. }) => {
            assert_eq!(line, Some(1));
            assert!(message.contains("include cycle"));
        }
        _ => panic!("symlink cycle must fail"),
    }
}

#[test]
fn repeated_note_files_resolve_to_one_named_note() {
    let directory = project(&[
        ("main.md", ":::include part.md\n:::\n\n:::include part.md\n:::\n"),
        ("part.md", "Repeated note[^1].\n\n[^1]: The same file's note.\n"),
    ]);
    let mut parsed = parse(&directory.path().join("main.md")).unwrap();
    let diagnostics = resolve(
        &mut parsed.document,
        &ResolveOptions {
            config: &Config::default(),
            library: None,
            lenient: true,
        },
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(parsed.document.resolved.footnotes, 1);
    let mut definitions = 0;
    let mut references = 0;
    ir::walk_inlines(&parsed.document, &mut |inline| match inline {
        Inline::Footnote { .. } => definitions += 1,
        Inline::FootnoteRef { .. } => references += 1,
        _ => {}
    });
    assert_eq!((definitions, references), (1, 1));
}

#[test]
fn unknown_metadata_variables_keep_their_own_namespace_and_bytes() {
    let directory = project(&[
        (
            "main.md",
            "---\nabstract_: old\nsettings: {span: old}\n---\n\n:::include part.md\n:::\n",
        ),
        (
            "part.md",
            "---\nabstract_: new\nsettings: {span: new}\n---\n\nPart.\n",
        ),
    ]);
    let parsed = parse(&directory.path().join("main.md")).unwrap();
    let ignored: Vec<_> = parsed
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "include-meta-ignored")
        .collect();
    assert_eq!(ignored.len(), 1);
    assert!(ignored[0].message.contains("`abstract_`"));
    assert!(ignored[0].message.contains("`settings`"));
}

#[test]
fn included_yaml_errors_preserve_lines_and_notebook_cells() {
    let markdown = "---\nauthors: [Ada\n---\n";
    let json = notebook(markdown, json!({}));
    let directory =
        project(&[("bad.md", markdown), ("bad.ipynb", &json), ("main.md", "")]);
    for name in ["bad.md", "bad.ipynb"] {
        let path = directory.path().join(name);
        let expected = match parse(&path) {
            Err(error) => error.span(),
            Ok(_) => panic!("invalid YAML must fail"),
        };
        assert!(expected.is_some());
        std::fs::write(
            directory.path().join("main.md"),
            format!(":::include {name}\n:::\n"),
        )
        .unwrap();
        match parse(&directory.path().join("main.md")) {
            Err(error) => {
                assert_eq!(error.path(), Some(canonical(&path).as_path()));
                assert_eq!(error.span(), expected);
            }
            Ok(_) => panic!("included invalid YAML must fail"),
        }
    }
}

#[test]
fn notebook_include_failures_keep_the_cell_location() {
    let directory = project(&[(
        "main.ipynb",
        &notebook("Before.\n\n:::include missing.md\n:::\n", json!({})),
    )]);
    match parse(&directory.path().join("main.ipynb")) {
        Err(error) => {
            assert_eq!(
                error.path(),
                Some(canonical(&directory.path().join("main.ipynb")).as_path())
            );
            let span = error.span().unwrap();
            assert_eq!(span.cell, Some(0));
            assert_eq!((span.start_line, span.end_line), (3, 3));
        }
        Ok(_) => panic!("missing notebook include must fail"),
    }
}

#[test]
fn notebook_include_paths_reject_extra_tokens_and_malformed_quotes() {
    let directory = project(&[("part.md", "Part."), ("main.ipynb", "")]);
    for (info, expected) in [
        ("include part.md extra.md", "unexpected"),
        ("include \"part.md", "unbalanced"),
        ("include", "needs the path"),
        ("{.include}", "attribute form"),
    ] {
        let json = notebook(&format!("Before.\n\n:::{info}\n:::\n"), json!({}));
        std::fs::write(directory.path().join("main.ipynb"), json).unwrap();
        match parse(&directory.path().join("main.ipynb")) {
            Err(error) => {
                assert!(error.message().contains(expected), "{error}");
                let span = error.span().unwrap();
                assert_eq!(span.cell, Some(0));
                assert_eq!(span.start_line, 3);
            }
            Ok(_) => {
                panic!("invalid notebook include path was accepted: {info}")
            }
        }
    }
}

#[test]
fn notebook_include_paths_preserve_quoted_spaces() {
    let directory = project(&[
        ("parts/with spaces.md", "Quoted child."),
        (
            "main.ipynb",
            &notebook(
                "Before.\n\n:::include \"parts/with spaces.md\"\n:::\n\nAfter.\n",
                json!({}),
            ),
        ),
    ]);
    let parsed = parse(&directory.path().join("main.ipynb")).unwrap();
    assert_eq!(text(&parsed.document), "Before.\nQuoted child.\nAfter.");
    assert_eq!(
        parsed.document.includes,
        [canonical(&directory.path().join("parts/with spaces.md"))]
    );
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn unclosed_or_short_include_fences_preserve_the_leaf_body() {
    let directory = project(&[
        ("part.md", "Included."),
        ("main.md", ""),
        ("main.ipynb", ""),
    ]);
    for body in [
        ":::include part.md\nBody survives.\n",
        "::::include part.md\nBody survives.\n:::\n",
    ] {
        for extension in ["md", "ipynb"] {
            let source = if extension == "ipynb" {
                notebook(body, json!({}))
            } else {
                body.to_owned()
            };
            let path = directory.path().join(format!("main.{extension}"));
            std::fs::write(&path, source).unwrap();
            let parsed = parse(&path).unwrap();
            let visible = text(&parsed.document);
            assert!(visible.contains("Included."), "{extension}: {body}");
            assert!(visible.contains("Body survives."), "{extension}: {body}");
            assert_eq!(
                parsed
                    .diagnostics
                    .iter()
                    .filter(|d| d.code == "unclosed-directive")
                    .count(),
                1,
                "{extension}: {body}"
            );
            assert!(
                parsed
                    .diagnostics
                    .iter()
                    .any(|d| d.code == "invalid-directive"),
                "{extension}: leaf body warns"
            );
        }
    }
}
