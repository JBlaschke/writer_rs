//! Reference scanning through the real Markdown AST and shared fragment APIs.

use std::path::Path;

use writer::config::Config;
use writer::ir::{self, Inline, Parsed, SourceInfo, SourceKind};

fn parse(text: &str, config: &Config) -> Parsed {
    let base_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md");
    let source = SourceInfo {
        path: base_dir.join("reference-scanner.md"),
        kind: SourceKind::Markdown,
        base_dir,
    };
    writer::markdown::parse(&source, text, config).unwrap()
}

fn references(parsed: &Parsed) -> Vec<Inline> {
    let mut found = Vec::new();
    ir::walk_inlines(&parsed.document, &mut |inline| {
        if matches!(inline, Inline::Ref { .. } | Inline::Cite { .. }) {
            found.push(inline.clone());
        }
    });
    found
}

#[test]
fn scanner_preserves_ast_escape_and_link_boundaries() {
    let parsed = parse(
        concat!(
            "@start (@paren) [@a; @b, p. 12] @tail.:\n\n",
            "\\@literal \\@@no \\!@no \\! @yes foo@no ",
            "me@example.com [@link](https://example.com) ",
            "`@code` $@math$ `@raw`{=html}\n",
        ),
        &Config::default(),
    );
    let nodes = references(&parsed);
    let targets: Vec<_> = nodes
        .iter()
        .filter_map(|inline| {
            if let Inline::Ref { target, .. } = inline {
                Some(target.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(targets, ["start", "paren", "tail", "yes"]);
    assert_eq!(
        nodes
            .iter()
            .filter(|inline| matches!(inline, Inline::Cite { .. }))
            .count(),
        1
    );
    assert!(parsed.diagnostics.is_empty());
    assert!(parsed.document.invariant_violations().is_empty());
}

#[test]
fn references_are_scanned_in_all_document_inline_contexts() {
    let parsed = parse(
        concat!(
            "---\nabstract: Abstract @sec-target.\n---\n\n",
            "# Target {#sec-target}\n\n# Heading @sec-target\n\n",
            "Paragraph @sec-target.\n\nTable: Caption @sec-target\n\n",
            "| Cell @sec-target |\n| --- |\n| value |\n\n",
            ":::note {title='Title @sec-target'}\nBody @sec-target.\n:::\n\n",
            "Note[^n].\n\n[^n]: Note @sec-target.\n",
        ),
        &Config::default(),
    );
    let nodes = references(&parsed);
    assert_eq!(nodes.len(), 8, "{nodes:#?}");
    for inline in nodes {
        let Inline::Ref { target, span, .. } = inline else {
            panic!("expected bare reference");
        };
        assert_eq!(target, "sec-target");
        assert!(span.is_some());
    }
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(parsed.document.features.footnotes);
    assert!(!parsed.document.features.citations);
    assert!(parsed.document.invariant_violations().is_empty());
}

#[test]
fn reference_setting_applies_to_shared_fragments_and_features() {
    let mut config = Config::default();
    config.markdown.references = false;
    let parsed = parse(
        "# @heading\n\n@ref [@a; @b] \\@literal\n\n:::note {title='@title'}\n@body\n:::\n",
        &config,
    );
    assert!(references(&parsed).is_empty());
    assert!(!parsed.document.features.citations);
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn notebook_and_markdown_output_fragments_use_the_scanner() {
    let base_dir = std::env::temp_dir();
    let source = SourceInfo {
        path: base_dir.join("inline-notebook.ipynb"),
        kind: SourceKind::Notebook,
        base_dir,
    };
    let json = serde_json::json!({
        "nbformat": 4, "nbformat_minor": 5,
        "metadata": {"kernelspec": {"language": "python"}},
        "cells": [
            {"cell_type": "markdown", "metadata": {}, "source": "Cell @sec-x."},
            {"cell_type": "code", "metadata": {}, "source": "display(x)",
             "execution_count": 1,
             "outputs": [{"output_type": "display_data", "metadata": {},
                          "data": {"text/markdown": "Output [@a; @b]."}}]}
        ]
    });
    let parsed =
        writer::notebook::parse(&source, &json.to_string(), &Config::default())
            .unwrap();
    let nodes = references(&parsed);
    assert_eq!(nodes.len(), 2, "{nodes:#?}");
    assert!(parsed.document.features.citations);
    for inline in nodes {
        let span = match inline {
            Inline::Ref { span, .. } | Inline::Cite { span, .. } => {
                span.unwrap()
            }
            _ => unreachable!(),
        };
        assert!(span.cell.is_some());
        assert!(span.file.is_none());
    }
    assert!(parsed.document.invariant_violations().is_empty());
}
