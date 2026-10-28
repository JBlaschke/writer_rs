//! Notebook IR, diagnostics and HTML snapshots, plus the T18 frontend contracts.
//! Stable asset labels follow presenter_rs's notebook snapshot harness.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{Value, json};
use writer::config::Config;
use writer::docx::{DocxOptions, Picture, Renderer, ValidateOptions};
use writer::error::{Diagnostics, Error, Level, Result};
use writer::html::emit::{EmitOptions as HtmlOptions, KatexOnly};
use writer::ir::{
    self, Block, Cell, Document, FigureKind, Inline, MathDialect, Parsed,
    SourceInfo, SourceKind, SourceSpan,
};
use writer::notebook::{self, CellIncludes, CellRef, ParseOptions, Selection};
use writer::resolve::{ResolveOptions, resolve};
use writer::typst::emit::{AssetPaths, EmitOptions as TypstOptions};

const FIXTURES: [&str; 7] = [
    "minimal", "outputs", "options", "tags", "frontmatter", "dataframe",
    "footnotes",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn fixture_dir() -> PathBuf {
    root().join("tests/fixtures/ipynb")
}

fn source_in(dir: &Path, name: &str) -> SourceInfo {
    SourceInfo {
        path: dir.join(format!("{name}.ipynb")),
        kind: SourceKind::Notebook,
        base_dir: dir.to_path_buf(),
    }
}

fn fixture_names() -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ipynb"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for fixture in FIXTURES {
        assert!(names.iter().any(|name| name == fixture), "missing {fixture}");
    }
    names
}

fn parse_fixture(name: &str) -> Parsed {
    let source = source_in(&fixture_dir(), name);
    let json = std::fs::read_to_string(&source.path).unwrap();
    notebook::parse(&source, &json, &Config::default()).unwrap()
}

fn notebook_json(cells: Value, metadata: Value) -> String {
    json!({"cells": cells, "metadata": metadata, "nbformat": 4, "nbformat_minor": 5}).to_string()
}

fn python() -> Value {
    json!({"language_info": {"name": "python"}})
}

fn md(source: &str) -> Value {
    json!({"cell_type": "markdown", "source": source})
}

fn code(id: &str, source: &str, tags: &[&str]) -> Value {
    json!({
        "cell_type": "code", "id": id, "source": source,
        "metadata": {"tags": tags}, "execution_count": 1,
        "outputs": [{"output_type": "stream", "name": "stdout", "text": "Output\n"}],
    })
}

fn parse_inline(cells: Value) -> Parsed {
    let source = source_in(&fixture_dir(), "inline");
    notebook::parse(
        &source,
        &notebook_json(cells, python()),
        &Config::default(),
    )
    .unwrap()
}

fn stable_json(document: &Document) -> Value {
    let ids: Vec<_> =
        document.assets.iter().map(|asset| asset.id.as_str()).collect();
    let mut labels = BTreeMap::new();
    // File hashes include their absolute path. Seed labels from content before
    // visiting the asset inventory, whose hash ordering varies by checkout.
    let mut content =
        serde_json::to_value((&document.meta.abstract_, &document.blocks))
            .unwrap();
    relabel(&mut content, &ids, &mut labels);
    let mut value = serde_json::to_value(document).unwrap();
    relabel(&mut value, &ids, &mut labels);
    if let Some(assets) = value.get_mut("assets").and_then(Value::as_array_mut)
    {
        for asset in assets.iter_mut() {
            if let Some(Value::String(origin)) = asset.get_mut("origin") {
                *origin = origin.replace('\\', "/");
            }
        }
        assets.sort_by_key(|asset| asset["id"].as_str().map(str::to_owned));
    }
    value
}

fn relabel(
    value: &mut Value, ids: &[&str], labels: &mut BTreeMap<String, String>,
) {
    match value {
        Value::String(text) => {
            let hex = Regex::new("[0-9a-f]{12}").unwrap();
            *text = hex
                .replace_all(text, |caps: &regex::Captures<'_>| {
                    let id = &caps[0];
                    if !ids.contains(&id) {
                        return id.to_owned();
                    }
                    let next = format!("{:012x}", labels.len() + 1);
                    labels.entry(id.to_owned()).or_insert(next).clone()
                })
                .into_owned();
        }
        Value::Array(items) => {
            items.iter_mut().for_each(|item| relabel(item, ids, labels))
        }
        Value::Object(map) => {
            map.values_mut().for_each(|item| relabel(item, ids, labels))
        }
        _ => {}
    }
}

fn redact(text: &str) -> String {
    text.replace(&*root().to_string_lossy(), "[root]").replace('\\', "/")
}

fn effective_config(document: &Document) -> Config {
    let mut config = Config::default();
    config.merge_front_matter(&document.meta).unwrap();
    config
}

fn html(parsed: &Parsed) -> String {
    let mut document = parsed.document.clone();
    let config = effective_config(&document);
    resolve(
        &mut document,
        &ResolveOptions { config: &config, library: None, lenient: true },
    );
    let mut assets = parsed.assets.clone();
    let mut diagnostics = Diagnostics::new();
    writer::html::emit::emit(
        &document,
        &mut HtmlOptions {
            config: &config,
            positions: false,
            includes: &document.includes,
            math: &mut KatexOnly::default(),
            assets: &mut assets,
            diagnostics: &mut diagnostics,
        },
    )
    .main
}

fn all_blocks(document: &Document) -> Vec<Block> {
    let mut blocks = Vec::new();
    ir::walk_blocks(document, &mut |block| blocks.push(block.clone()));
    blocks
}

fn cells(document: &Document) -> Vec<Cell> {
    all_blocks(document)
        .into_iter()
        .filter_map(|block| match block {
            Block::Cell(cell) => Some(cell),
            _ => None,
        })
        .collect()
}

fn cell(document: &Document, id: &str) -> Cell {
    cells(document)
        .into_iter()
        .find(|cell| cell.id.as_deref() == Some(id))
        .unwrap_or_else(|| panic!("missing code cell {id}"))
}

fn codes(parsed: &Parsed) -> Vec<&'static str> {
    parsed.diagnostics.iter().map(|finding| finding.code).collect()
}

fn figure(document: &Document, id: &str) -> Block {
    all_blocks(document).into_iter().find(|block| {
        matches!(block, Block::Figure { attrs, .. } if attrs.id.as_deref() == Some(id))
    }).unwrap_or_else(|| panic!("missing figure {id}"))
}

#[test]
fn every_fixture_is_snapshotted() {
    for name in fixture_names() {
        let parsed = parse_fixture(&name);
        insta::assert_json_snapshot!(name.clone(), stable_json(&parsed.document), {
            ".source.path" => "[path]", ".source.base_dir" => "[path]",
        });
        let diagnostics: Vec<_> = parsed
            .diagnostics
            .iter()
            .map(|finding| {
                let at = finding.span.as_ref().map_or_else(
                    || "notebook".to_owned(),
                    |span| {
                        format!(
                            "cell {}, lines {}-{}",
                            span.cell.unwrap(),
                            span.start_line,
                            span.end_line
                        )
                    },
                );
                format!(
                    "{at} [{}] {}: {}",
                    finding.code,
                    finding.level,
                    redact(&finding.message)
                )
            })
            .collect();
        insta::assert_json_snapshot!(
            format!("{name}_diagnostics"),
            diagnostics
        );
        insta::with_settings!({ filters => vec![(r"asset:[0-9a-f]{12}", "asset:[id]")] }, {
            insta::assert_snapshot!(format!("{name}_html"), html(&parsed));
        });
    }
}

#[test]
fn invariants_and_spans() {
    for name in fixture_names() {
        let parsed = parse_fixture(&name);
        assert_eq!(
            parsed.document.invariant_violations(),
            Vec::<String>::new(),
            "{name}"
        );
        assert!(parsed.document.features.notebook);
        let relabelled: Document =
            serde_json::from_value(stable_json(&parsed.document)).unwrap();
        assert!(
            relabelled.invariant_violations().is_empty(),
            "{name}: relabelled"
        );
        for diagnostic in &parsed.diagnostics {
            assert!(diagnostic.path.is_some(), "{name}: {diagnostic}");
        }
        for block in all_blocks(&parsed.document) {
            if matches!(block, Block::Toc { .. }) {
                continue;
            }
            let span = block
                .span()
                .unwrap_or_else(|| panic!("{name}: missing span: {block:?}"));
            assert!(
                span.start_line >= 1 && span.end_line >= span.start_line,
                "{name}: {span:?}"
            );
        }
    }
}

#[test]
fn empty_front_matter_only_warns_for_keyless_heading_comments() {
    for (text, expected) in [
        ("---\n# Intro\n---\nBody", 1),
        ("---\n# title: Draft\n---\nBody", 0),
        ("---\ntitle: Real\n\n# Intro\n\n---\nBody", 0),
    ] {
        let parsed = parse_inline(json!([md(text)]));
        assert_eq!(
            codes(&parsed)
                .into_iter()
                .filter(|code| *code == "front-matter-empty")
                .count(),
            expected,
            "{text}"
        );
    }
}

#[test]
fn noncode_cells_honour_only_removal_tags() {
    let parsed = parse_inline(json!([
        {"cell_type": "markdown", "source": "#| include: false\nMarkdown kept.", "metadata": {"writer": {"include": false}, "tags": ["hide-input"]}},
        {"cell_type": "raw", "source": "Raw kept.", "metadata": {"raw_mimetype": "text/plain", "writer": {"include": false}, "tags": ["hide-output"]}},
        {"cell_type": "markdown", "source": "Markdown removed.", "metadata": {"tags": ["remove_cell"]}},
        {"cell_type": "raw", "source": "Raw removed.", "metadata": {"raw_mimetype": "text/plain", "tags": ["remove-cell"]}}
    ]));
    let value = serde_json::to_string(&parsed.document.blocks).unwrap();
    assert!(value.contains("Markdown kept.") && value.contains("Raw kept."));
    assert!(
        !value.contains("Markdown removed.") && !value.contains("Raw removed.")
    );
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn minimal_is_not_run() {
    let parsed = parse_fixture("minimal");
    assert_eq!(codes(&parsed), ["notebook-not-run"]);
    assert!(parsed.diagnostics.iter().next().unwrap().span.is_none());
    assert_eq!(cells(&parsed.document).len(), 1);
    assert_eq!(cell(&parsed.document, "unrun").source, "print(\"hello\")");
}

#[test]
fn outputs_fixture() {
    let parsed = parse_fixture("outputs");
    let inline_pngs = parsed
        .document
        .assets
        .iter()
        .filter(|asset| {
            asset.mime == "image/png" && asset.origin.starts_with("inline:")
        })
        .count();
    assert_eq!(
        inline_pngs, 1,
        "identical PNG outputs share one registered asset"
    );
    for expected in [
        "output-invalid", "output-script", "output-unsupported",
        "output-file-outside", "missing-asset", "output-include",
    ] {
        assert!(codes(&parsed).contains(&expected), "missing {expected}");
    }
    assert!(all_blocks(&parsed.document).iter().any(|block| {
        matches!(block, Block::Raw { format, text, .. } if format == "latex" && text == "$x$ and $y$")
    }));
    let numbered = cell(&parsed.document, "numbered");
    assert_eq!(numbered.outputs[0].candidates.len(), 1);
    assert!(matches!(
        numbered.outputs[0].candidates[0].blocks[0],
        Block::Heading { .. }
    ));
    for cell in cells(&parsed.document) {
        for output in cell.outputs {
            if let Some(last) = output.candidates.last() {
                let mut test = parsed.document.clone();
                test.meta.abstract_ = None;
                test.blocks = last.blocks.clone();
                assert!(
                    !all_blocks(&test)
                        .iter()
                        .any(|block| matches!(block, Block::Raw { .. })),
                    "last candidate of cell {}",
                    cell.index
                );
            }
        }
    }
    let streams = cell(&parsed.document, "streams");
    assert_eq!(streams.outputs.len(), 2);
    let Block::CodeBlock { text, .. } =
        &streams.outputs[0].candidates[0].blocks[0]
    else {
        panic!("stream is code")
    };
    assert_eq!(text, "a\nb\n100%");
    let include = cell(&parsed.document, "include");
    let text = serde_json::to_string(&include.outputs).unwrap();
    assert!(text.contains("After output include"));
    assert!(!text.contains(":::include"));
}

struct NoRenderer;

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("no equation renderer in frontend tests".into()))
    }
    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("no image renderer in frontend tests".into()))
    }
    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("no MathML renderer in frontend tests".into()))
    }
}

#[test]
fn no_escape_byte_in_emitters() {
    let parsed = parse_fixture("outputs");
    let markup = html(&parsed);
    assert!(!markup.contains('\u{1b}'));
    assert!(!markup.contains("data-ansi"));
    assert!(markup.contains("ansi-red-intense-fg ansi-bold"));
    assert!(markup.contains("ansi-default-inverse-fg ansi-red-bg"));
    let config = effective_config(&parsed.document);
    let mut document = parsed.document.clone();
    resolve(
        &mut document,
        &ResolveOptions { config: &config, library: None, lenient: true },
    );
    let docx = writer::docx::write(
        &document,
        &parsed.assets,
        &mut NoRenderer,
        &DocxOptions {
            config: &config,
            reference: None,
            source_date_epoch: Some("0".into()),
            all_styles: false,
        },
    )
    .unwrap();
    assert!(
        writer::docx::validate(
            &docx.bytes,
            &ValidateOptions { builtin_styles: true }
        )
        .is_empty()
    );
    let mut zip = zip::ZipArchive::new(Cursor::new(docx.bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(!xml.contains('\u{1b}'));
    assert!(xml.contains("<w:color w:val=\"B22B31\"/>"));
    let typst = writer::typst::emit::emit(
        &document,
        &parsed.assets,
        &TypstOptions {
            config: &config,
            assets: AssetPaths::Virtual,
            template: None,
            bibliography: Vec::new(),
            style: "ieee".into(),
            footnote_prefix: String::new(),
            fragment: false,
        },
    )
    .unwrap();
    assert!(!typst.text.contains('\u{1b}'));
}

#[test]
fn options_fixture_rows() {
    let parsed = parse_fixture("options");
    assert!(matches!(
        figure(&parsed.document, "fig-one"),
        Block::Figure { kind: FigureKind::Image, .. }
    ));
    assert!(matches!(
        figure(&parsed.document, "fig-group"),
        Block::Figure { kind: FigureKind::Other, .. }
    ));
    for id in ["fig-list-1", "fig-list-2", "fig:x"] {
        assert!(matches!(
            figure(&parsed.document, id),
            Block::Figure { kind: FigureKind::Image, .. }
        ));
    }
    assert!(matches!(
        figure(&parsed.document, "tbl-values"),
        Block::Figure { kind: FigureKind::Table, .. }
    ));
    let blocks = all_blocks(&parsed.document);
    assert!(blocks.iter().any(|block| matches!(block, Block::Figure { kind: FigureKind::Other, attrs, .. } if attrs.id.as_deref() == Some("tbl-both"))));
    let listing_position = parsed
        .document
        .blocks
        .iter()
        .position(|block| {
            matches!(block, Block::Figure { kind: FigureKind::Listing, .. })
        })
        .unwrap();
    assert!(
        matches!(&parsed.document.blocks[listing_position + 1], Block::Cell(cell) if cell.id.as_deref() == Some("listing") && !cell.show_input)
    );
    assert_eq!(
        cell(&parsed.document, "anchor").attrs.id.as_deref(),
        Some("cell-anchor")
    );
    let hidden = cell(&parsed.document, "hidden-output");
    assert!(!hidden.show_output && hidden.outputs.is_empty());
    assert!(hidden.attrs.id.is_none());
    assert!(
        !cells(&parsed.document)
            .iter()
            .any(|cell| cell.id.as_deref() == Some("omitted"))
    );
    let filtered = cell(&parsed.document, "filtered");
    assert_eq!(
        filtered.outputs.len(),
        1,
        "filtered stderr/errors do not break stream merging"
    );
    assert!(all_blocks(&parsed.document).iter().any(|block| matches!(block, Block::Heading { inlines, .. } if ir::plain_text(inlines) == "Stream heading")));
    assert!(codes(&parsed).contains(&"label-kind-mismatch"));
    assert!(codes(&parsed).contains(&"cell-options-invalid"));
    assert!(codes(&parsed).contains(&"cell-option-unsupported"));
    let table = blocks
        .iter()
        .find_map(|block| match block {
            Block::Table { columns, span: Some(span), .. }
                if span.cell == Some(5) =>
            {
                Some(columns)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        table.iter().map(|column| column.width).collect::<Vec<_>>(),
        [Some(0.2), Some(0.4), Some(0.4)]
    );
}

#[test]
fn tags_fixture_precedence() {
    let parsed = parse_fixture("tags");
    let cells = cells(&parsed.document);
    for id in ["remove-cell", "hide-cell", "skip", "remove_cell", "hide_cell"] {
        assert!(
            !cells.iter().any(|cell| cell.id.as_deref() == Some(id)),
            "{id}"
        );
    }
    for id in [
        "remove-input", "hide-input", "remove_input", "hide_input",
        "source-hidden", "metadata",
    ] {
        let cell = cell(&parsed.document, id);
        assert!(!cell.show_input && cell.show_output, "{id}");
    }
    for id in [
        "remove-output", "hide-output", "remove_output", "hide_output",
        "outputs-hidden",
    ] {
        let cell = cell(&parsed.document, id);
        assert!(
            cell.show_input && !cell.show_output && cell.outputs.is_empty(),
            "{id}"
        );
    }
    let override_ = cell(&parsed.document, "override");
    assert!(override_.show_input && override_.show_output);
    if let Some(both) =
        cells.iter().find(|cell| cell.id.as_deref() == Some("both-hidden"))
    {
        assert!(
            !both.show_input && !both.show_output && both.outputs.is_empty()
        );
    }
    assert!(codes(&parsed).contains(&"unknown-cell-type"));
}

#[test]
fn frontmatter_fixture_layers() {
    let parsed = parse_fixture("frontmatter");
    assert_eq!(
        parsed.document.meta.title.as_deref(),
        Some("Title from front matter")
    );
    assert_eq!(
        parsed.document.meta.subtitle.as_deref(),
        Some("Subtitle from front matter")
    );
    assert_eq!(parsed.document.meta.authors[0].name, "Front matter author");
    assert!(parsed.document.meta.abstract_.is_some());
    assert!(matches!(parsed.document.blocks[0], Block::Toc { .. }));
    assert!(!parsed.document.meta.vars.contains_key("abstract"));
    assert!(!parsed.document.meta.vars.contains_key("event"));
    for expected in [
        "notebook-presenter-metadata", "notebook-front-matter-ignored",
        "raw-cell-skipped", "notebook-attachment-skipped",
        "missing-attachment",
    ] {
        assert!(codes(&parsed).contains(&expected), "missing {expected}");
    }
    let formats: BTreeSet<_> = all_blocks(&parsed.document)
        .iter()
        .filter_map(|block| match block {
            Block::Raw { format, .. } => Some(format.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        formats,
        BTreeSet::from([
            "html".into(),
            "latex".into(),
            "typst".into(),
            "openxml".into()
        ])
    );
    assert!(all_blocks(&parsed.document).iter().any(|block| matches!(block, Block::CodeBlock { attrs, .. } if attrs.classes.iter().any(|class| class == "raw-cell"))));
    assert_eq!(
        parsed
            .document
            .assets
            .iter()
            .filter(|asset| asset.mime == "image/png")
            .count(),
        1
    );
    assert!(html(&parsed).contains("Note: this matters"));
}

#[test]
fn dataframe_fixture_tables() {
    let parsed = parse_fixture("dataframe");
    let blocks = all_blocks(&parsed.document);
    let tables: Vec<_> = blocks
        .iter()
        .filter(|block| matches!(block, Block::Table { .. }))
        .collect();
    assert_eq!(tables.len(), 7);
    assert!(tables.iter().any(|block| matches!(block, Block::Table { head, columns, .. } if head.len() == 3 && columns.len() == 5)));
    assert!(blocks.iter().any(|block| matches!(block, Block::Paragraph { inlines, .. } if ir::plain_text(inlines) == "100 rows × 2 columns")));
    assert!(blocks.iter().any(|block| matches!(block, Block::Figure { kind: FigureKind::Table, caption: Some(caption), .. } if ir::plain_text(caption) == "Styled values")));
    let unescaped = cell(&parsed.document, "unescaped");
    assert!(matches!(
        unescaped.outputs[0].candidates[0].blocks[0],
        Block::Raw { .. }
    ));
}

#[test]
fn footnotes_fixture() {
    let parsed = parse_fixture("footnotes");
    let mut definitions = BTreeMap::<String, usize>::new();
    let mut references = Vec::new();
    let mut inline_notes = 0;
    ir::walk_inlines(&parsed.document, &mut |inline| match inline {
        Inline::Footnote { label: Some(label), .. } => {
            *definitions.entry(label.clone()).or_default() += 1
        }
        Inline::Footnote { label: None, .. } => inline_notes += 1,
        Inline::FootnoteRef { label, .. } => references.push(label.clone()),
        _ => {}
    });
    assert!(definitions.values().all(|count| *count == 1), "{definitions:?}");
    assert!(references.contains(&"shared".into()));
    assert_eq!(definitions.get("Note"), Some(&1));
    assert!(
        !definitions.contains_key("note") && !definitions.contains_key("NOTE")
    );
    assert_eq!(inline_notes, 2);
    assert_eq!(definitions.get("1"), Some(&1));
    assert_eq!(definitions.get("cell11-out0-1"), Some(&1));
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "missing-asset")
            .count(),
        1
    );
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "footnote-duplicate")
            .count(),
        1
    );
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "footnote-unreferenced")
            .count(),
        1
    );
    let undefined: BTreeSet<_> = parsed
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "footnote-undefined")
        .filter_map(|finding| finding.span.as_ref().and_then(|span| span.cell))
        .collect();
    assert_eq!(undefined, BTreeSet::from([6, 7, 8, 9]));
    let code: Vec<_> = all_blocks(&parsed.document)
        .into_iter()
        .filter_map(|block| match block {
            Block::CodeBlock { text, span: Some(span), .. }
                if matches!(span.cell, Some(7 | 9)) =>
            {
                Some(text)
            }
            _ => None,
        })
        .collect();
    assert!(code.iter().any(|text| text == "print(\"unchanged\")"));
    assert!(code.iter().any(|text| text == "print(\"still unchanged\")"));
}

struct NoIncludes;

impl CellIncludes for NoIncludes {
    fn expand(
        &mut self, _: &str, _: &SourceSpan, _: &SourceInfo, _: &Config,
    ) -> Result<Parsed> {
        panic!("selection test has no include directives")
    }
}

#[test]
fn selection_and_parts() {
    let cells = json!([
        md("First[^shared]."),
        code(
            "listing",
            "#| lst-label: lst-sample\n#| lst-cap: Listing\nx = 1",
            &["paper"]
        ),
        code("second", "x = 2", &["paper", "other"]),
        md("[^shared]: Shared definition in an unselected cell."),
        code("omitted", "#| include: false\nx = 3", &["paper"]),
    ]);
    let source = source_in(&fixture_dir(), "selection");
    let json = notebook_json(
        cells,
        json!({"language_info": {"name": "python"}, "writer": {"toc": true}}),
    );
    let selections = [
        (
            Selection {
                cells: Some(vec![
                    CellRef::Index(2),
                    CellRef::Id("listing".into()),
                ]),
                ..Selection::default()
            },
            vec![1, 2],
        ),
        (
            Selection {
                tags: Some(vec!["paper".into()]),
                ..Selection::default()
            },
            vec![1, 2],
        ),
        (
            Selection {
                cells: Some(vec![CellRef::Index(0), CellRef::Index(2)]),
                tags: Some(vec!["paper".into()]),
                ..Selection::default()
            },
            vec![2],
        ),
    ];
    for (selection, expected) in selections {
        let (parsed, parts) = notebook::parse_cells(
            &source,
            &json,
            &Config::default(),
            ParseOptions { includes: &mut NoIncludes, nested: true, selection },
        )
        .unwrap();
        assert_eq!(
            parts.iter().map(|part| part.index).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            parts.into_iter().flat_map(|part| part.body).collect::<Vec<_>>(),
            parsed.document.blocks
        );
        assert!(
            !parsed
                .document
                .blocks
                .iter()
                .any(|block| matches!(block, Block::Toc { .. }))
        );
    }
    let (parsed, parts) = notebook::parse_cells(
        &source,
        &json,
        &Config::default(),
        ParseOptions {
            includes: &mut NoIncludes,
            nested: true,
            selection: Selection {
                cells: Some(vec![CellRef::Index(1)]),
                hide_input: true,
                ..Selection::default()
            },
        },
    )
    .unwrap();
    assert!(!all_blocks(&parsed.document).iter().any(|block| matches!(
        block,
        Block::Figure { kind: FigureKind::Listing, .. }
    )));
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].outputs, parts[0].body);
    assert_eq!(parts[0].kind, notebook::CellKind::Code);
    let parsed = notebook::parse_with(
        &source,
        &json,
        &Config::default(),
        ParseOptions {
            includes: &mut NoIncludes,
            nested: true,
            selection: Selection {
                cells: Some(vec![
                    CellRef::Index(99),
                    CellRef::Id("omitted".into()),
                ]),
                ..Selection::default()
            },
        },
    )
    .unwrap();
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "notebook-cell-not-found")
            .count(),
        2
    );
    assert!(
        parsed
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "notebook-cell-not-found")
            .all(|finding| finding.level == Level::Error)
    );
    let parsed = notebook::parse_with(
        &source,
        &json,
        &Config::default(),
        ParseOptions {
            includes: &mut NoIncludes,
            nested: true,
            selection: Selection {
                cells: Some(vec![CellRef::Index(0)]),
                ..Selection::default()
            },
        },
    )
    .unwrap();
    assert!(
        serde_json::to_string(&parsed.document.blocks)
            .unwrap()
            .contains("Shared definition in an unselected cell")
    );
}

struct FakeIncludes {
    paths: Vec<String>,
}

impl CellIncludes for FakeIncludes {
    fn expand(
        &mut self, path: &str, _: &SourceSpan, notebook: &SourceInfo,
        config: &Config,
    ) -> Result<Parsed> {
        self.paths.push(path.to_owned());
        let included = notebook.base_dir.join(path);
        let source = SourceInfo {
            path: included.clone(),
            base_dir: notebook.base_dir.clone(),
            kind: SourceKind::Markdown,
        };
        let mut parsed =
            writer::markdown::parse(&source, "Included paragraph.", config)?;
        ir::walk_blocks_mut(&mut parsed.document, &mut |block| {
            if let Block::Paragraph { span: Some(span), .. } = block {
                span.file = Some(included.clone())
            }
        });
        parsed.document.includes.push(included);
        Ok(parsed)
    }
}

#[test]
fn cell_include_hook_splices() {
    let source = source_in(&fixture_dir(), "includes");
    let json = notebook_json(
        json!([md(
            "Before.\n\n:::include part.md\n:::\n\nAfter.\n\n:::include part.md\n:::"
        )]),
        python(),
    );
    let mut includes = FakeIncludes { paths: Vec::new() };
    let parsed = notebook::parse_with(
        &source,
        &json,
        &Config::default(),
        ParseOptions {
            includes: &mut includes,
            nested: true,
            selection: Selection::default(),
        },
    )
    .unwrap();
    assert_eq!(includes.paths, ["part.md", "part.md"]);
    assert_eq!(parsed.document.includes, [source.base_dir.join("part.md")]);
    let blocks = &parsed.document.blocks;
    assert_eq!(blocks.len(), 4);
    assert_eq!(blocks[0].span().unwrap().start_line, 1);
    assert_eq!(blocks[2].span().unwrap().start_line, 6);
    assert!(blocks[1].span().unwrap().file.is_some());
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn language_specific_options_are_applied_by_the_notebook_kernel() {
    for (language, comment) in [("python", "#|"), ("R", "#|"), ("c", "/*|")] {
        let source = source_in(&fixture_dir(), "markers");
        let line = if language == "c" {
            format!("{comment} echo: false */")
        } else {
            format!("{comment} echo: false")
        };
        let json = notebook_json(
            json!([code("cell", &format!("{line}\nx"), &[])]),
            json!({"language_info": {"name": language}}),
        );
        let parsed =
            notebook::parse(&source, &json, &Config::default()).unwrap();
        let cell = cell(&parsed.document, "cell");
        assert!(!cell.show_input && cell.show_output);
        assert_eq!(cell.source, "x");
    }
}

#[test]
fn example_notebook_is_clean() {
    let source = source_in(&root().join("examples/notebook"), "analysis");
    let json = std::fs::read_to_string(&source.path).unwrap();
    let mut parsed =
        notebook::parse(&source, &json, &Config::default()).unwrap();
    let config = effective_config(&parsed.document);
    parsed.diagnostics.extend(resolve(
        &mut parsed.document,
        &ResolveOptions { config: &config, library: None, lenient: true },
    ));
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert!(parsed.document.resolved.labels.contains_key("fig-rolling"));
}

#[test]
fn hidden_outputs_create_no_assets_or_output_diagnostics() {
    let source = source_in(&fixture_dir(), "hidden");
    let json = notebook_json(
        json!([{
            "cell_type": "code", "source": "#| output: false\nx", "execution_count": 1,
            "outputs": [{"output_type": "display_data", "data": {"image/png": "invalid!?"}, "metadata": {}}]
        }]),
        python(),
    );
    let parsed = notebook::parse(&source, &json, &Config::default()).unwrap();
    assert!(parsed.document.assets.is_empty());
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn output_notes_are_namespaced_from_main_cell_notes() {
    let parsed = parse_inline(json!([
        md("Main[^1].\n\n[^1]: Main note."),
        {"cell_type": "code", "source": "x", "execution_count": 1, "outputs": [{
            "output_type": "display_data", "data": {"text/markdown": "Output[^1].\n\n[^1]: Output note."}, "metadata": {}
        }]},
    ]));
    let mut labels = Vec::new();
    ir::walk_inlines(&parsed.document, &mut |inline| {
        if let Inline::Footnote { label: Some(label), .. } = inline {
            labels.push(label.clone())
        }
    });
    assert_eq!(labels, ["1", "cell1-out0-1"]);
}

#[test]
fn group_caption_is_parsed_once_and_does_not_become_panel_alt() {
    const PIXEL: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mPQqnr0HwAE3gKGkp+jugAAAABJRU5ErkJggg==";
    let output = json!({
        "output_type": "display_data",
        "data": {"image/png": PIXEL}, "metadata": {},
    });
    let parsed = parse_inline(json!([{
        "cell_type": "code", "id": "group", "execution_count": 1,
        "source": "#| fig-cap: 'Shared ![caption](images/missing-group-caption.png)'\nx",
        "outputs": [output.clone(), output],
    }]));
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "missing-asset")
            .count(),
        1,
        "the group caption is parsed once, independently of its panels",
    );
    let output_cell = cells(&parsed.document)
        .into_iter()
        .find(|cell| !cell.outputs.is_empty())
        .unwrap();
    let mut panels = parsed.document.clone();
    panels.meta.abstract_ = None;
    panels.blocks = output_cell
        .outputs
        .into_iter()
        .flat_map(|output| {
            output.candidates.into_iter().flat_map(|candidate| candidate.blocks)
        })
        .collect();
    let mut image_alts = Vec::new();
    ir::walk_inlines(&panels, &mut |inline| {
        if let Inline::Image { alt, .. } = inline {
            image_alts.push(alt.clone());
        }
    });
    assert_eq!(image_alts, [Vec::<Inline>::new(), Vec::new()]);
}

#[test]
fn metadata_config_aliases_keep_layer_precedence_when_reapplied() {
    let source = source_in(&fixture_dir(), "metadata-aliases");
    let json = notebook_json(
        json!([md(
            "---\nhtml: {theme: new-theme}\ncsl: apa\nnumbering: false\n---\nBody.",
        )]),
        json!({"writer": {
            "theme": "old-theme", "cite_style": "ieee",
            "numbering": {"headings": "1.1", "figures": "section"},
        }}),
    );
    let parsed = notebook::parse(&source, &json, &Config::default()).unwrap();
    let config = effective_config(&parsed.document);
    assert_eq!(config.html.theme, "new-theme");
    assert_eq!(config.references.style, "apa");
    assert_eq!(
        config.numbering.headings,
        writer::config::HeadingNumbering::Off
    );
    assert_eq!(
        config.numbering.figures,
        writer::config::FigureNumbering::Section
    );
}

#[test]
fn later_front_matter_with_default_metadata_is_still_ignored() {
    let parsed = parse_inline(json!([
        md("First markdown cell."),
        md("---\nauthors: []\n---\nAfter ignored metadata."),
    ]));
    assert_eq!(codes(&parsed), ["notebook-front-matter-ignored"]);
    assert!(
        !parsed
            .document
            .blocks
            .iter()
            .any(|block| matches!(block, Block::ThematicBreak { .. }))
    );
    assert!(html(&parsed).contains("After ignored metadata"));
}

#[test]
fn empty_cells_do_not_create_listing_figures() {
    let parsed = parse_inline(json!([{
        "cell_type": "code", "execution_count": 1,
        "source": "#| lst-label: lst-empty\n#| lst-cap: Empty listing\n \n",
        "outputs": [],
    }]));
    assert!(parsed.document.blocks.is_empty());
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn hidden_output_captions_are_dropped_without_validation_noise() {
    let parsed = parse_inline(json!([{
        "cell_type": "code", "execution_count": 1,
        "source": "#| output: false\n#| fig-cap: [First, Second]\n#| fig-subcap: [Left, Right]\n#| label: fig-hidden\nx",
        "outputs": [],
    }]));
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let cells = cells(&parsed.document);
    assert_eq!(cells.len(), 1);
    assert!(!cells[0].show_output && cells[0].outputs.is_empty());
    assert!(cells[0].attrs.id.is_none());
}

#[test]
fn every_config_alias_yields_to_a_later_table_field() {
    let source = source_in(&fixture_dir(), "metadata-tables");
    let front = "---\nreferences: {bibliography: [new.bib], style: apa, locale: de-DE}\nhtml: {theme: new-theme}\npdf: {template: new.typ, paper: letter}\ndocx: {reference_doc: new.docx}\nlatex: {template: new.tex}\nnumbering: {figures: section}\n---\nBody.";
    let json = notebook_json(
        json!([md(front)]),
        json!({"writer": {
            "bibliography": "old.bib", "csl": "ieee", "cite_locale": "en-US",
            "theme": "old-theme", "template": "old.typ", "paper": "a4",
            "reference_doc": "old.docx", "latex_template": "old.tex",
            "numbering": true,
        }}),
    );
    let parsed = notebook::parse(&source, &json, &Config::default()).unwrap();
    let config = effective_config(&parsed.document);
    assert_eq!(config.references.bibliography, [PathBuf::from("new.bib")]);
    assert_eq!(config.references.style, "apa");
    assert_eq!(config.references.locale, "de-DE");
    assert_eq!(config.html.theme, "new-theme");
    assert_eq!(config.pdf.template, Path::new("new.typ"));
    assert_eq!(config.pdf.paper, "letter");
    assert_eq!(config.docx.reference_doc, Path::new("new.docx"));
    assert_eq!(config.latex.template, Path::new("new.tex"));
    assert_eq!(
        config.numbering.headings,
        writer::config::HeadingNumbering::Pattern("1.1".into())
    );
    assert_eq!(
        config.numbering.figures,
        writer::config::FigureNumbering::Section
    );
    assert_eq!(
        parsed.document.meta.vars["numbering"],
        json!({"figures": "section", "headings": true})
    );
    assert_eq!(
        parsed.document.meta.vars["html"],
        json!({"theme": "new-theme"})
    );
    assert!(!parsed.document.meta.vars.contains_key("theme"));
}

#[test]
fn later_config_aliases_override_tables_without_dropping_other_keys() {
    let source = source_in(&fixture_dir(), "metadata-shorthand");
    let json = notebook_json(
        json!([md(
            "---\ntheme: new-theme\ncite_style: apa\nnumbering: false\n---\nBody.",
        )]),
        json!({"writer": {
            "html": {"theme": "old-theme", "standalone": false},
            "references": {"style": "ieee", "locale": "de-DE"},
            "numbering": {"headings": "1.1", "equations": "all"},
        }}),
    );
    let parsed = notebook::parse(&source, &json, &Config::default()).unwrap();
    let config = effective_config(&parsed.document);
    assert_eq!(config.html.theme, "new-theme");
    assert!(!config.html.standalone);
    assert_eq!(config.references.style, "apa");
    assert_eq!(config.references.locale, "de-DE");
    assert_eq!(
        config.numbering.headings,
        writer::config::HeadingNumbering::Off
    );
    assert_eq!(
        config.numbering.equations,
        writer::config::EquationNumbering::All
    );
}

#[test]
fn stable_asset_labels_follow_content_instead_of_absolute_path_hashes() {
    let original = parse_fixture("outputs").document;
    let position = original
        .assets
        .iter()
        .position(|asset| asset.origin.starts_with("file:"))
        .unwrap();
    let old_id = &original.assets[position].id;
    let new_id = if position == 0 {
        "ffffffffffff"
    } else {
        "000000000000"
    };
    assert!(!original.assets.iter().any(|asset| asset.id == new_id));
    let json =
        serde_json::to_string(&original).unwrap().replace(old_id, new_id);
    let mut relocated: Document = serde_json::from_str(&json).unwrap();
    relocated.assets.sort_by(|left, right| left.id.cmp(&right.id));
    let new_position =
        relocated.assets.iter().position(|asset| asset.id == new_id).unwrap();
    assert_ne!(
        new_position, position,
        "the changed file hash reorders inventory"
    );
    assert_eq!(stable_json(&original), stable_json(&relocated));
}
