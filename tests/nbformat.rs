//! Lenient nbformat contract tests, lifted from presenter_rs without slide behavior.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use writer::error::{Diagnostics, Error, Level};
use writer::ir::SourceSpan;
use writer::notebook::nbformat::{
    CellType, MultilineString, Notebook, Output, codes, parse_notebook,
};

fn parse(value: Value) -> (Notebook, Diagnostics) {
    parse_notebook(Path::new("test.ipynb"), &value.to_string()).unwrap()
}

/// (cell index from the span, code, message) of every warning, after checking the parts
/// every warning shares: warning level, the path given to `parse_notebook`, and a
/// whole-cell span (lines 1..=source lines) when the warning is about a cell.
fn summary(
    notebook: &Notebook, warnings: &Diagnostics,
) -> Vec<(Option<usize>, &'static str, String)> {
    warnings
        .iter()
        .map(|warning| {
            assert_eq!(warning.level, Level::Warning, "{warning}");
            assert_eq!(
                warning.path.as_deref(),
                Some(Path::new("test.ipynb")),
                "{warning}"
            );
            let cell = warning.span.as_ref().map(|span| {
                let index =
                    span.cell.expect("cell-level spans name their cell");
                assert_eq!(
                    span,
                    &notebook.cells[index].span(index),
                    "{warning}"
                );
                index
            });
            (cell, warning.code, warning.message.clone())
        })
        .collect()
}

/// A valid nbformat 4.5 notebook around `cells`.
fn notebook_with(cells: Value) -> Value {
    json!({ "cells": cells, "metadata": {}, "nbformat": 4, "nbformat_minor": 5 })
}

fn parse_error(json: &str) -> (PathBuf, Option<usize>, String) {
    match parse_notebook(Path::new("talk.ipynb"), json) {
        Err(Error::Parse { path, line, message }) => (path, line, message),
        other => panic!("expected Error::Parse, got {other:?}"),
    }
}

#[test]
fn source_as_list_and_as_string_are_identical() {
    let cell = |source: Value| json!({ "cell_type": "markdown", "metadata": {}, "source": source });
    let (from_list, _) =
        parse(notebook_with(json!([cell(json!(["# T\n", "\n", "body"]))])));
    let (from_string, _) =
        parse(notebook_with(json!([cell(json!("# T\n\nbody"))])));
    assert_eq!(from_list.cells[0].source, from_string.cells[0].source);
    assert_eq!(from_list.cells[0].source, MultilineString::from("# T\n\nbody"));
}

#[test]
fn unknown_output_type_is_skipped_with_a_warning_and_the_cell_survives() {
    let (notebook, warnings) = parse(notebook_with(json!([
        { "cell_type": "markdown", "metadata": {}, "source": "intro" },
        {
            "cell_type": "code", "id": "c1", "execution_count": 3, "metadata": { "tags": ["t"] },
            "source": ["x = 1\n", "x"],
            "outputs": [
                { "output_type": "stream", "name": "stdout", "text": "before\n" },
                { "output_type": "weird", "data": { "text/plain": "?" } },
                { "output_type": "execute_result", "execution_count": 3, "metadata": {},
                  "data": { "text/plain": "1" } }
            ]
        }
    ])));
    let found = summary(&notebook, &warnings);
    assert_eq!(found.len(), 1, "{warnings:?}");
    let (cell, code, message) = &found[0];
    assert_eq!((*cell, *code), (Some(1), codes::OUTPUT_SKIPPED));
    assert!(
        message
            .starts_with("output 1 (weird) skipped: unknown variant `weird`"),
        "{message}"
    );
    let warning = warnings.iter().next().unwrap();
    assert_eq!(
        warning.span,
        Some(SourceSpan {
            start_line: 1,
            end_line: 2,
            cell: Some(1),
            file: None
        })
    );
    assert!(
        warning.to_string().starts_with(
            "test.ipynb: cell 1, line 1: warning: output 1 (weird) skipped: "
        ),
        "{warning}"
    );
    let cell = &notebook.cells[1];
    assert_eq!(cell.id.as_deref(), Some("c1"));
    assert_eq!(cell.source.as_str(), "x = 1\nx");
    assert_eq!(cell.execution_count, Some(3));
    assert_eq!(cell.tags(), ["t"]);
    assert_eq!(cell.outputs.len(), 2);
    assert_eq!(
        cell.outputs[0],
        Output::Stream { name: "stdout".into(), text: "before\n".into() }
    );
    let Output::ExecuteResult { data, .. } = &cell.outputs[1] else {
        panic!("{:?}", cell.outputs[1]);
    };
    assert_eq!(data["text/plain"], json!("1"));
}

#[test]
fn malformed_known_outputs_and_odd_shapes_are_warnings_too() {
    let (notebook, warnings) = parse(notebook_with(json!([
        {
            "cell_type": "code", "metadata": {}, "source": "",
            "outputs": [
                { "output_type": "stream", "name": "stdout" },
                { "output_type": "error", "ename": "E", "evalue": "v", "traceback": ["t"] }
            ]
        },
        { "cell_type": "code", "metadata": {}, "source": "", "outputs": { "not": "a list" } },
        {
            "cell_type": "markdown", "metadata": {}, "source": "![a](attachment:a.png)",
            "attachments": {
                "a.png": { "image/png": ["iVBORw0K", "GgoA"] },
                "bad.json": { "application/json": { "an": "object" } }
            }
        },
        { "cell_type": "raw", "metadata": {}, "source": "", "attachments": [] }
    ])));
    let expected = [
        (0, codes::OUTPUT_SKIPPED, "output 0 (stream) skipped: missing field `text`"),
        (1, codes::OUTPUT_SKIPPED, "`outputs` is not a list, ignored"),
        (
            2,
            codes::ATTACHMENT_SKIPPED,
            "attachment 'bad.json' skipped: invalid type: map, expected a string or a list of strings",
        ),
        (3, codes::ATTACHMENT_SKIPPED, "`attachments` is not an object, ignored"),
    ]
    .map(|(cell, code, message)| (Some(cell), code, message.to_owned()));
    assert_eq!(summary(&notebook, &warnings), expected);
    assert!(matches!(&notebook.cells[0].outputs[..], [Output::Error { .. }]));
    assert!(notebook.cells[1].outputs.is_empty());
    let attachments = &notebook.cells[2].attachments;
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments["a.png"]["image/png"].as_str(), "iVBORw0KGgoA");
}

#[test]
fn nbformat_3_is_a_parse_error_mentioning_nbformat_4() {
    let v3 = json!({
        "metadata": { "name": "old" }, "nbformat": 3, "nbformat_minor": 0,
        "worksheets": [ { "cells": [], "metadata": {} } ]
    });
    let (path, line, message) = parse_error(&v3.to_string());
    assert_eq!(path, Path::new("talk.ipynb"));
    assert_eq!(line, None);
    assert_eq!(message, "nbformat 4 required"); // spec §1, verbatim
    let (_, _, message) = parse_error(r#"{"cells": [], "nbformat": 2}"#);
    assert_eq!(message, "nbformat 4 required");

    // Not a notebook version at all (spec §1 does not cover these).
    for json in [
        r#"{"cells": [], "metadata": {}}"#,
        r#"{"cells": [], "nbformat": "4"}"#,
        r#"{"cells": [], "nbformat": 4.5}"#,
        r#"{"cells": [], "nbformat": -4}"#,
    ] {
        let (_, _, message) = parse_error(json);
        assert_eq!(
            message,
            "not a Jupyter notebook: `nbformat` is missing or not an integer",
            "{json}"
        );
    }
}

#[test]
fn other_unusable_documents_are_parse_errors() {
    let (_, line, message) = parse_error("{\n \"cells\": [\n  {,\n ]\n}");
    assert_eq!(line, Some(3));
    assert!(message.starts_with("invalid notebook JSON: "), "{message}");

    let (_, _, message) = parse_error("[1, 2]");
    assert_eq!(message, "not a Jupyter notebook: expected a JSON object");
    let (_, _, message) = parse_error(r#"{"nbformat": 4}"#);
    assert_eq!(message, "not a Jupyter notebook: `cells` is missing");
    let (_, _, message) = parse_error(r#"{"nbformat": 4, "cells": {}}"#);
    assert_eq!(message, "`cells` is not a list");
    let cell_error = |cell: Value| {
        let notebook = notebook_with(
            json!([{ "cell_type": "markdown", "source": "" }, cell]),
        );
        parse_error(&notebook.to_string()).2
    };
    assert!(
        cell_error(json!({ "source": "no cell_type" }))
            .starts_with("cell 1: missing field `cell_type`"),
    );
    assert!(
        cell_error(json!({ "cell_type": 3 }))
            .starts_with("cell 1: invalid type: integer `3`")
    );
    assert!(
        cell_error(json!({ "cell_type": "code", "source": 3 }))
            .starts_with("cell 1: invalid type")
    );
    assert!(
        cell_error(json!("not an object"))
            .starts_with("cell 1: invalid type: string")
    );
}

#[test]
fn unknown_cell_type_is_kept_as_unknown_with_a_warning() {
    // nbformat 5.11.1 validates and reads this (an "unrecognized cell" from 4.6).
    let (notebook, warnings) = parse(json!({
        "cells": [
            { "cell_type": "markdown", "id": "a", "metadata": {}, "source": "# A" },
            { "cell_type": "chart", "id": "b", "metadata": {}, "source": ["x\n", "y"] },
            { "cell_type": "heading", "metadata": {}, "source": "", "level": 1 },
            { "cell_type": "code", "id": "c", "metadata": {}, "source": "1", "outputs": [],
              "execution_count": null }
        ],
        "metadata": {},
        "nbformat": 4,
        "nbformat_minor": 6
    }));
    let kinds: Vec<CellType> =
        notebook.cells.iter().map(|cell| cell.cell_type).collect();
    use CellType::{Code, Markdown, Unknown};
    assert_eq!(kinds, [Markdown, Unknown, Unknown, Code]);
    assert_eq!(notebook.cells[1].id.as_deref(), Some("b"));
    assert_eq!(notebook.cells[1].source.as_str(), "x\ny");
    let expected = [
        (1, "unknown cell_type \"chart\", cell ignored"),
        (2, "unknown cell_type \"heading\", cell ignored"),
    ]
    .map(|(cell, message)| {
        (Some(cell), codes::UNKNOWN_CELL_TYPE, message.to_owned())
    });
    assert_eq!(summary(&notebook, &warnings), expected);
}

#[test]
fn newer_nbformat_is_read_with_a_warning() {
    let (notebook, warnings) = parse(json!({ "cells": [], "nbformat": 5 }));
    assert_eq!(notebook.nbformat, 5);
    assert_eq!(notebook.nbformat_minor, 0);
    assert_eq!(
        summary(&notebook, &warnings),
        [(
            None,
            codes::NBFORMAT_NEWER,
            "nbformat 5 is newer than this tool knows; reading it as nbformat 4".to_owned()
        )]
    );
    assert_eq!(
        warnings.iter().next().unwrap().to_string(),
        "test.ipynb: warning: nbformat 5 is newer than this tool knows; reading it as nbformat 4"
    );
}

#[test]
fn unknown_keys_anywhere_are_ignored() {
    let (notebook, warnings) = parse(json!({
        "cells": [
            {
                "cell_type": "code", "id": "x", "source": "1", "execution_count": 1,
                "future_cell_key": [1, 2],
                "metadata": { "custom": { "deep": true }, "collapsed": false, "scrolled": "auto" },
                "outputs": [
                    { "output_type": "display_data", "data": { "text/plain": "1" },
                      "metadata": {}, "transient": { "display_id": "d" }, "future_output_key": 1 },
                    { "output_type": "stream", "name": "stdout", "text": "s", "extra": null }
                ]
            },
            { "cell_type": "markdown", "source": "no metadata, no outputs" }
        ],
        "metadata": { "anything": { "at": ["all"] }, "widgets": {} },
        "nbformat": 4,
        "nbformat_minor": 5,
        "future_top_level_key": "x"
    }));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(notebook.cells.len(), 2);
    assert_eq!(notebook.cells[0].outputs.len(), 2);
    assert_eq!(notebook.cells[1].metadata, Value::Null);
    assert!(notebook.cells[1].tags().is_empty());
}

#[test]
fn null_and_missing_optional_fields_use_defaults() {
    let (notebook, warnings) = parse(json!({
        "cells": [ {
            "cell_type": "code", "id": null, "metadata": null, "source": null,
            "outputs": null, "execution_count": null, "attachments": null
        } ],
        "metadata": null,
        "nbformat": 4
    }));
    assert!(warnings.is_empty(), "{warnings:?}");
    let cell = &notebook.cells[0];
    assert_eq!(cell.id, None);
    assert_eq!(cell.source, MultilineString::default());
    assert!(cell.outputs.is_empty() && cell.attachments.is_empty());
    assert_eq!(notebook.language(), "text");
    assert_eq!(notebook.title(), None);
}

#[test]
fn language_precedence() {
    let language = |metadata: Value| {
        let mut nb = notebook_with(json!([]));
        nb["metadata"] = metadata;
        parse(nb).0.language().to_owned()
    };
    let both = json!({
        "kernelspec": { "name": "ir", "display_name": "R", "language": "R" },
        "language_info": { "name": "julia" }
    });
    assert_eq!(language(both), "julia");
    assert_eq!(language(json!({ "kernelspec": { "language": "R" } })), "R");
    assert_eq!(
        language(json!({ "language_info": { "name": "rust" } })),
        "rust"
    );
    assert_eq!(
        language(
            json!({ "language_info": { "name": "" }, "kernelspec": { "language": "R" } })
        ),
        "R"
    );
    assert_eq!(
        language(json!({ "kernelspec": { "name": "python3" } })),
        "text"
    );
    assert_eq!(language(json!({})), "text");
}

#[test]
fn jupyterlab_4_metadata_parses_without_warnings() {
    // The cell metadata JupyterLab 4's property inspector writes when "-" is selected.
    let metadata = json!({ "editable": true, "slideshow": { "slide_type": "" }, "tags": [] });
    let (notebook, warnings) = parse(notebook_with(json!([
        { "cell_type": "markdown", "id": "m", "metadata": metadata, "source": "# Title" },
        { "cell_type": "code", "id": "c", "metadata": metadata, "source": "1",
          "outputs": [], "execution_count": null }
    ])));
    assert!(warnings.is_empty(), "{warnings:?}");
    for cell in &notebook.cells {
        assert!(cell.tags().is_empty());
    }
}

#[test]
fn non_object_writer_metadata_is_ignored_with_a_warning() {
    let (notebook, warnings) = parse(json!({
        "cells": [ { "cell_type": "markdown", "source": "", "metadata": { "writer": "two-column" } } ],
        "metadata": { "writer": ["x"], "title": 7 },
        "nbformat": 4
    }));
    assert_eq!(notebook.writer_meta(), None);
    assert_eq!(notebook.title(), None);
    assert_eq!(notebook.cells[0].writer_options(), None);
    let message = "metadata.writer is not an object, ignored".to_owned();
    assert_eq!(
        summary(&notebook, &warnings),
        [
            (None, codes::WRITER_NOT_OBJECT, message.clone()),
            (Some(0), codes::WRITER_NOT_OBJECT, message),
        ]
    );
}

#[test]
fn null_writer_metadata_is_absent_without_a_warning() {
    let (notebook, warnings) = parse(json!({
        "cells": [ { "cell_type": "markdown", "source": "", "metadata": { "writer": null } } ],
        "metadata": { "writer": null, "title": null },
        "nbformat": 4
    }));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(notebook.writer_meta(), None);
    assert_eq!(notebook.title(), None);
    assert_eq!(notebook.cells[0].writer_options(), None);
}

#[test]
fn raw_mimetype_falls_back_to_format() {
    let raw = |metadata: Value| json!({ "cell_type": "raw", "source": "<b>x</b>", "metadata": metadata });
    let (notebook, _) = parse(notebook_with(json!([
        raw(json!({ "raw_mimetype": "text/html", "format": "text/latex" })),
        raw(json!({ "format": "text/html" })),
        raw(json!({}))
    ])));
    let mimes: Vec<Option<&str>> =
        notebook.cells.iter().map(|c| c.raw_mimetype()).collect();
    assert_eq!(mimes, [Some("text/html"), Some("text/html"), None]);
}

#[test]
fn metadata_names_and_object_layers_are_read_independently() {
    let (notebook, warnings) = parse(json!({
        "cells": [], "nbformat": 4, "nbformat_minor": null,
        "metadata": {
            "title": "Notebook title",
            "authors": [{"name": "Ada"}, "ignored", {"name": 3}, {"name": "Grace"}],
            "writer": {"title": "Writer title"},
            "presenter": {"title": "Presenter title"}
        }
    }));
    assert!(warnings.is_empty());
    assert_eq!(notebook.nbformat_minor, 0);
    assert_eq!(notebook.title(), Some("Notebook title"));
    assert_eq!(notebook.authors(), ["Ada", "Grace"]);
    assert_eq!(notebook.writer_meta().unwrap()["title"], "Writer title");
    assert_eq!(notebook.presenter_meta().unwrap()["title"], "Presenter title");
}

#[test]
fn only_one_leading_byte_order_mark_is_ignored() {
    let json = notebook_with(json!([])).to_string();
    let path = Path::new("test.ipynb");
    let (plain, _) = parse_notebook(path, &json).unwrap();
    let (with_bom, warnings) =
        parse_notebook(path, &format!("\u{feff}{json}")).unwrap();
    assert_eq!(with_bom, plain);
    assert!(warnings.is_empty());
    for bad in [format!("\u{feff}\u{feff}{json}"), format!(" \u{feff}{json}")] {
        assert!(parse_notebook(path, &bad).is_err());
    }
}
