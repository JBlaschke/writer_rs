//! IR serialization, traversal, and structural contract tests (ir.md §1–§6).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use writer::assets::registry::AssetRegistry;
use writer::ir::*;

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(relative)
}

fn sample_text() -> String {
    std::fs::read_to_string(fixture_path("ir/sample-document.json")).unwrap()
}

fn sample() -> Document {
    serde_json::from_str(&sample_text()).unwrap()
}

fn block(value: Value) -> Block {
    serde_json::from_value(value).unwrap()
}

fn inline(value: Value) -> Inline {
    serde_json::from_value(value).unwrap()
}

fn paragraph(text: &str) -> Block {
    Block::paragraph(vec![Inline::text(text)])
}

fn document(blocks: Vec<Block>) -> Document {
    Document {
        meta: DocMeta::default(),
        blocks,
        features: Features::default(),
        source: SourceInfo {
            path: "/abs/document.md".into(),
            kind: SourceKind::Markdown,
            base_dir: "/abs".into(),
        },
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Resolved::default(),
    }
}

#[test]
fn asset_url_invariants_validate_ids_before_url_suffixes() {
    for (id, valid) in [
        ("012345abcdef", true),
        ("abc", false),
        ("012345abcdef0", false),
        ("012345ABCDEF", false),
        ("012345abcdeg", false),
    ] {
        for suffix in ["", "?v=2", "#panel", "?v=2#panel"] {
            let url = format!("asset:{id}{suffix}");
            let document = document(vec![
                Block::paragraph(vec![Inline::Image {
                    alt: Vec::new(),
                    url: url.clone(),
                    title: None,
                    attrs: Attrs::default(),
                }]),
                Block::Raw {
                    format: "html".into(),
                    text: format!("<img src=\"{url}\">"),
                    span: None,
                },
            ]);
            let problems = document.invariant_violations();
            assert_eq!(
                problems.len(),
                if valid { 0 } else { 2 },
                "{url}: {problems:?}"
            );
        }
    }
}

fn block_tag(block: &Block) -> &'static str {
    match block {
        Block::Heading { .. } => "heading",
        Block::Paragraph { .. } => "paragraph",
        Block::BlockQuote { .. } => "block_quote",
        Block::List { .. } => "list",
        Block::DefinitionList { .. } => "definition_list",
        Block::CodeBlock { .. } => "code_block",
        Block::Figure { .. } => "figure",
        Block::Table { .. } => "table",
        Block::Math { .. } => "math",
        Block::Callout { .. } => "callout",
        Block::Div { .. } => "div",
        Block::Raw { .. } => "raw",
        Block::ThematicBreak { .. } => "thematic_break",
        Block::PageBreak { .. } => "page_break",
        Block::Toc { .. } => "toc",
        Block::Bibliography { .. } => "bibliography",
        Block::Cell(_) => "cell",
    }
}

fn inline_tag(inline: &Inline) -> &'static str {
    match inline {
        Inline::Text { .. } => "text",
        Inline::Emph { .. } => "emph",
        Inline::Strong { .. } => "strong",
        Inline::Strikeout { .. } => "strikeout",
        Inline::Subscript { .. } => "subscript",
        Inline::Superscript { .. } => "superscript",
        Inline::Underline { .. } => "underline",
        Inline::SmallCaps { .. } => "small_caps",
        Inline::Code { .. } => "code",
        Inline::Math { .. } => "math",
        Inline::Link { .. } => "link",
        Inline::Image { .. } => "image",
        Inline::Footnote { .. } => "footnote",
        Inline::FootnoteRef { .. } => "footnote_ref",
        Inline::Ref { .. } => "ref",
        Inline::Cite { .. } => "cite",
        Inline::LineBreak => "line_break",
        Inline::SoftBreak => "soft_break",
        Inline::Span { .. } => "span",
        Inline::Raw { .. } => "raw",
    }
}

#[test]
fn sample_is_canonical_pretty_json() {
    let parsed = sample();
    let pretty =
        format!("{}\n", serde_json::to_string_pretty(&parsed).unwrap());
    assert_eq!(pretty, sample_text());
    assert_eq!(serde_json::from_str::<Document>(&pretty).unwrap(), parsed);
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        serde_json::from_str::<Value>(&sample_text()).unwrap()
    );
}

#[test]
fn sample_covers_every_variant() {
    let sample = sample();
    let mut blocks = BTreeSet::new();
    let mut inlines = BTreeSet::new();
    let mut figures = BTreeSet::new();
    let mut lists = BTreeSet::new();
    let mut outputs = BTreeSet::new();
    let mut task = false;
    let mut page_ref = false;
    walk_blocks(&sample, &mut |block| {
        blocks.insert(block_tag(block));
        match block {
            Block::Figure { kind, .. } => {
                figures.insert(match kind {
                    FigureKind::Image => "image",
                    FigureKind::Table => "table",
                    FigureKind::Listing => "listing",
                    FigureKind::Other => "other",
                });
            }
            Block::List { kind, items, .. } => {
                lists.insert(match kind {
                    ListKind::Bullet => "bullet",
                    ListKind::Ordered { .. } => "ordered",
                });
                task |= items.iter().any(|item| item.checked.is_some());
            }
            Block::Cell(cell) => {
                for output in &cell.outputs {
                    outputs.insert(match output.kind {
                        OutputKind::Stream { .. } => "stream",
                        OutputKind::Error { .. } => "error",
                        OutputKind::Display => "display",
                        OutputKind::Result => "result",
                    });
                }
            }
            _ => {}
        }
    });
    walk_inlines(&sample, &mut |inline| {
        inlines.insert(inline_tag(inline));
        page_ref |= matches!(inline, Inline::Ref { form: RefForm::Page, .. });
    });
    assert_eq!(blocks.len(), 17, "{blocks:?}");
    assert_eq!(inlines.len(), 20, "{inlines:?}");
    assert_eq!(figures, BTreeSet::from(["image", "table", "listing", "other"]));
    assert_eq!(lists, BTreeSet::from(["bullet", "ordered"]));
    assert_eq!(
        outputs,
        BTreeSet::from(["stream", "error", "display", "result"])
    );
    assert!(task && page_ref);
    assert!(sample_text().contains("Figure\u{a0}1"));
}

#[test]
fn sample_satisfies_invariants() {
    assert_eq!(sample().invariant_violations(), Vec::<String>::new());
}

#[test]
fn sample_is_resolved() {
    let sample = sample();
    let mut numbers = Vec::new();
    let mut notes = BTreeMap::new();
    let mut references = Vec::new();
    walk_inlines(&sample, &mut |inline| match inline {
        Inline::Ref { resolved, .. } => assert!(resolved.is_some()),
        Inline::Cite { resolved, .. } => assert!(resolved.is_some()),
        Inline::Footnote { label, number, .. } => {
            let number = number.expect("resolved footnote");
            numbers.push(number);
            if let Some(label) = label {
                notes.insert(label.clone(), number);
            }
        }
        Inline::FootnoteRef { label, number } => {
            references.push((label.clone(), *number))
        }
        _ => {}
    });
    assert_eq!(numbers, (1..=sample.resolved.footnotes).collect::<Vec<_>>());
    for (label, number) in references {
        assert_eq!(number, notes.get(&label).copied());
    }
    walk_blocks(&sample, &mut |block| match block {
        Block::Heading { number, .. }
        | Block::Figure { number, .. }
        | Block::Math { number, .. } => assert!(number.is_some()),
        _ => {}
    });
}

#[test]
fn sample_asset_id_is_reproducible_from_plot_png() {
    let sample = sample();
    let bytes = std::fs::read(fixture_path("md/images/plot.png")).unwrap();
    let mut assets = AssetRegistry::new();
    assert_eq!(bytes.len(), 70);
    assert_eq!(
        assets.register_bytes(bytes, "image/png", "plot.png"),
        "32debefb8003"
    );
    assert_eq!(assets.infos(&sample.source.base_dir), sample.assets);
}

#[test]
fn spec_example_deserializes() {
    let spec = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/spec/ir.md"),
    )
    .unwrap();
    let example =
        spec.split("```json\n").nth(1).unwrap().split("\n```").next().unwrap();
    let document: Document = serde_json::from_str(example).unwrap();
    assert_eq!(document.invariant_violations(), Vec::<String>::new());
    assert_eq!(document.meta.title.as_deref(), Some("GPU allocation"));
}

#[test]
fn json_shapes_match_the_spec() {
    let code = block(json!({
        "type": "code_block", "lang": null, "text": "x", "attrs": {},
    }));
    assert_eq!(serde_json::to_value(code).unwrap()["type"], "code_block");
    assert_eq!(
        serde_json::to_value(Block::ThematicBreak { span: None }).unwrap(),
        json!({"type": "thematic_break"})
    );
    assert_eq!(
        serde_json::to_value(Inline::SoftBreak).unwrap(),
        json!({"type": "soft_break"})
    );
    assert_eq!(
        serde_json::to_value(ListKind::Ordered { start: 1 }).unwrap(),
        json!({"kind": "ordered", "start": 1})
    );
    assert_eq!(
        serde_json::to_value(OutputKind::Result).unwrap(),
        json!({"kind": "result"})
    );
    let heading = block(
        json!({"type": "heading", "level": 1, "inlines": [], "attrs": {}}),
    );
    let value = serde_json::to_value(heading).unwrap();
    assert!(value.get("span").is_none() && value.get("number").is_none());
    let value = serde_json::to_value(document(Vec::new())).unwrap();
    assert!(value.get("includes").is_none() && value.get("resolved").is_none());
    let cell = serde_json::to_value(Block::Cell(Cell::default())).unwrap();
    assert_eq!(cell["type"], "cell");
    assert!(cell.get("index").is_some());
    assert_eq!(
        serde_json::to_value([
            SourceKind::Markdown,
            SourceKind::Notebook,
            SourceKind::Typst
        ])
        .unwrap(),
        json!(["markdown", "notebook", "typst"])
    );
    assert!(Resolved::default().is_empty());
    assert!(!Resolved { footnotes: 1, ..Resolved::default() }.is_empty());
}

#[test]
fn table_cell_defaults_to_span_one() {
    let cell: TableCell = serde_json::from_str("{}").unwrap();
    assert_eq!(cell, TableCell::default());
    assert_eq!((cell.colspan, cell.rowspan), (1, 1));
    let output: CellOutput = serde_json::from_str("{}").unwrap();
    assert_eq!(output.kind, OutputKind::Display);
    let _: ListItem = serde_json::from_str("{}").unwrap();
    let _: ColumnSpec = serde_json::from_str("{}").unwrap();
    let _: CiteItem = serde_json::from_str("{}").unwrap();
}

#[test]
fn features_merge_is_fieldwise_or() {
    let mut features = Features::default();
    for (i, field) in ["math", "code", "notebook", "citations", "footnotes"]
        .iter()
        .enumerate()
    {
        let incoming: Features =
            serde_json::from_value(json!({(*field): true})).unwrap();
        features.merge(&incoming);
        let enabled = serde_json::to_value(features)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .filter(|v| **v == Value::Bool(true))
            .count();
        assert_eq!(enabled, i + 1);
        features.merge(&Features::default());
    }
    assert_eq!(
        features,
        Features {
            math: true,
            code: true,
            notebook: true,
            citations: true,
            footnotes: true
        }
    );
}

#[test]
fn doc_meta_vars_preserve_order() {
    let meta: DocMeta =
        serde_json::from_str(r#"{"vars":{"zeta":1,"alpha":2,"mid":3}}"#)
            .unwrap();
    assert_eq!(
        meta.vars.keys().map(String::as_str).collect::<Vec<_>>(),
        ["zeta", "alpha", "mid"]
    );
    let attrs = Attrs::with_id("figure");
    let _: &BTreeMap<String, String> = &attrs.pairs;
    assert_eq!(attrs.id.as_deref(), Some("figure"));
}

fn table(rows: Value) -> Block {
    block(json!({
        "type": "table", "columns": [{}, {}, {}], "head": [], "body": rows,
        "foot": [], "attrs": {},
    }))
}

#[test]
fn invariant_violations_are_detected() {
    let cases = [
        (
            json!({"type": "heading", "level": 0, "inlines": [], "attrs": {}}),
            "blocks[0].level",
        ),
        (
            json!({
                "type": "list", "kind": {"kind": "bullet"}, "tight": true,
                "items": [{"blocks": [{"type": "page_break"}]}],
            }),
            "blocks[0].items[0].blocks[0]",
        ),
        (
            json!({
                "type": "figure", "kind": "image", "caption": null, "body": [],
                "attrs": {},
            }),
            "blocks[0].body",
        ),
        (
            json!({
                "type": "figure", "kind": "table", "caption": null, "body": [],
                "attrs": {},
            }),
            "blocks[0].body",
        ),
        (
            json!({
                "type": "figure", "kind": "listing", "caption": null,
                "body": [], "attrs": {},
            }),
            "blocks[0].body",
        ),
        (
            json!({
                "type": "paragraph",
                "inlines": [
                    {
                        "type": "image", "alt": [], "url": "asset:ABCDEF012345",
                        "title": null, "attrs": {},
                    },
                ],
            }),
            "blocks[0].inlines[0].url",
        ),
        (
            json!({
                "type": "raw", "format": "html",
                "text": "<img src='asset:abc'>",
            }),
            "blocks[0].text",
        ),
        (
            json!({"type": "raw", "format": "HTML", "text": ""}),
            "blocks[0].format",
        ),
        (
            json!({
                "type": "paragraph",
                "inlines": [{"type": "raw", "format": "LaTeX", "text": ""}],
            }),
            "blocks[0].inlines[0].format",
        ),
        (
            json!({"type": "div", "blocks": [], "attrs": {"id": "two words"}}),
            "blocks[0].attrs.id",
        ),
        (
            json!({
                "type": "paragraph",
                "inlines": [{"type": "footnote_ref", "label": ""}],
            }),
            "blocks[0].inlines[0].label",
        ),
        (
            json!({
                "type": "paragraph", "inlines": [],
                "span": {"start_line": 0, "end_line": 1},
            }),
            "blocks[0].span",
        ),
    ];
    for (broken, expected_path) in cases {
        let violations = document(vec![block(broken)]).invariant_violations();
        assert!(
            violations.iter().any(|v| v.starts_with(expected_path)),
            "{expected_path}: {violations:?}"
        );
    }
    for field in ["colspan", "rowspan"] {
        let mut broken = json!([{"cells": [{"colspan": 3}]}]);
        broken[0]["cells"][0][field] = json!(0);
        let problems = document(vec![table(broken)]).invariant_violations();
        assert!(
            problems.iter().any(|p| p.contains(&format!(".cells[0].{field}"))),
            "{problems:?}"
        );
    }
    let labelled = block(json!({
        "type": "table", "columns": [], "head": [], "body": [], "foot": [],
        "attrs": {"id": "tbl-alone"},
    }));
    assert!(
        document(vec![labelled])
            .invariant_violations()
            .iter()
            .any(|p| p.contains("wrapped"))
    );
    // Missing local files and valid unregistered placeholders remain permitted.
    for url in [
        "missing.png", "http://example.com/a", "https://example.com/a",
        "//example.com/a", "data:image/png,abc", "/a.png",
        "asset:012345abcdef",
    ] {
        let image = block(json!({
            "type": "paragraph",
            "inlines": [
                {
                    "type": "image", "alt": [], "url": url, "title": null,
                    "attrs": {},
                },
            ],
        }));
        assert!(
            document(vec![image]).invariant_violations().is_empty(),
            "{url}"
        );
    }
    let duplicate =
        block(json!({"type": "div", "attrs": {"id": "same"}, "blocks": []}));
    assert!(
        document(vec![duplicate.clone(), duplicate])
            .invariant_violations()
            .is_empty()
    );
}

#[test]
fn rowspan_covered_slots_are_valid() {
    let valid = table(json!([
        {"cells": [{"rowspan": 2}, {"colspan": 2}]}, {"cells": [{}, {}]},
    ]));
    assert!(document(vec![valid]).invariant_violations().is_empty());
    let overlap = table(json!([
        {"cells": [{}, {"rowspan": 2}, {}]}, {"cells": [{"colspan": 2}]},
    ]));
    let problems = document(vec![overlap]).invariant_violations();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("blocks[0].body[1]"));
    let mut separate =
        serde_json::to_value(table(json!([{"cells": [{}, {}]}]))).unwrap();
    separate["head"] = json!([{"cells": [{"rowspan": 2}, {"colspan": 2}]}]);
    separate["foot"] = json!([{"cells": [{}, {}, {}]}]);
    let problems = document(vec![block(separate)]).invariant_violations();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("blocks[0].body[0]"));
}

#[test]
fn caption_footnotes_obey_figure_nesting_rules() {
    for kind in ["image", "table", "listing", "other"] {
        let body = match kind {
            "image" => {
                json!([
                    {
                        "type": "paragraph",
                        "inlines": [
                            {
                                "type": "image", "alt": [], "url": "/image.png",
                                "title": null, "attrs": {},
                            },
                        ],
                    },
                ])
            }
            "table" => {
                json!([
                    {
                        "type": "table", "columns": [], "head": [], "body": [],
                        "foot": [], "attrs": {},
                    },
                ])
            }
            "listing" => {
                json!([
                    {
                        "type": "code_block", "lang": null, "text": "",
                        "attrs": {},
                    },
                ])
            }
            _ => json!([]),
        };
        let outer = block(json!({
            "type": "figure", "kind": kind, "body": body, "attrs": {},
            "caption": [
                {
                    "type": "footnote", "label": null,
                    "blocks": [
                        {
                            "type": "figure", "kind": "other",
                            "caption": null, "body": [], "attrs": {},
                        },
                    ],
                },
            ],
        }));
        let problems = document(vec![outer]).invariant_violations();
        if kind == "other" {
            assert!(problems.is_empty());
        } else {
            assert!(
                problems.iter().any(|p| p.contains("caption[0].blocks[0]")
                    && p.contains("nest")),
                "{kind}: {problems:?}"
            );
        }
    }
}

#[test]
fn plain_text_rules() {
    let inlines: Inlines = serde_json::from_value(json!([
        {"type": "text", "text": "A"},
        {"type": "code", "text": "B", "attrs": {}},
        {"type": "math", "source": "C", "dialect": "latex"},
        {
            "type": "strong",
            "inlines": [
                {"type": "emph", "inlines": [{"type": "text", "text": "D"}]},
            ],
        },
        {
            "type": "link", "inlines": [{"type": "text", "text": "E"}],
            "url": "x", "title": null, "attrs": {},
        },
        {
            "type": "image", "alt": [{"type": "text", "text": "F"}], "url": "x",
            "title": null, "attrs": {},
        },
        {"type": "line_break"}, {"type": "soft_break"},
        {
            "type": "span", "attrs": {},
            "inlines": [{"type": "text", "text": "G"}],
        },
        {"type": "ref", "target": "x", "supplement": null, "form": "normal"},
        {"type": "cite", "items": [], "mode": "normal"},
        {
            "type": "ref", "target": "x", "supplement": null, "form": "normal",
            "resolved": {
                "kind": "figure", "number": "1",
                "text": [{"type": "text", "text": "Figure 1"}], "anchor": "x",
            },
        },
        {
            "type": "cite", "items": [], "mode": "normal",
            "resolved": {
                "inlines": [{"type": "text", "text": "[2]"}], "note": false,
            },
        },
        {
            "type": "footnote", "label": null,
            "blocks": [
                {
                    "type": "paragraph",
                    "inlines": [{"type": "text", "text": "ignored"}],
                },
            ],
        },
        {"type": "footnote_ref", "label": "x"},
        {"type": "raw", "format": "html", "text": "ignored"},
    ]))
    .unwrap();
    assert_eq!(plain_text(&inlines), "ABCDEF  G[?][?]Figure 1[2]");
    assert_eq!(Block::paragraph(inlines).span(), None);
}

fn note(marker: &str) -> Value {
    json!({
        "type": "footnote", "label": null,
        "blocks": [paragraph(marker)],
    })
}

fn traversal_document() -> Document {
    let row = |marker: &str| json!([{"cells": [{"inlines": [note(marker)]}]}]);
    let mut document = document(vec![
        block(json!({
            "type": "figure", "kind": "other",
            "caption": [note("caption-note")],
            "body": [
                {
                    "type": "paragraph",
                    "inlines": [{"type": "text", "text": "figure-body"}],
                },
            ],
            "attrs": {},
        })),
        block(json!({
            "type": "callout", "kind": "note",
            "title": [note("title-note")],
            "blocks": [
                {
                    "type": "paragraph",
                    "inlines": [{"type": "text", "text": "callout-body"}],
                },
            ],
            "attrs": {},
        })),
        block(json!({
            "type": "definition_list",
            "items": [
                {
                    "term": [note("term-note")],
                    "definitions": [
                        [
                            {
                                "type": "paragraph",
                                "inlines": [
                                    {
                                        "type": "text",
                                        "text": "definition-body",
                                    },
                                ],
                            },
                        ],
                    ],
                },
            ],
        })),
        block(json!({
            "type": "table", "columns": [{}], "head": row("head-note"),
            "body": row("body-note"), "foot": row("foot-note"), "attrs": {},
        })),
        block(json!({
            "type": "paragraph",
            "inlines": [
                {
                    "type": "strong",
                    "inlines": [
                        {
                            "type": "emph",
                            "inlines": [Inline::text("nested-text")],
                        },
                    ],
                },
                {
                    "type": "link", "inlines": [note("link-note")], "url": "x",
                    "title": null, "attrs": {},
                },
                {
                    "type": "image", "alt": [note("alt-note")], "url": "x",
                    "title": null, "attrs": {},
                },
                {"type": "span", "attrs": {}, "inlines": [note("span-note")]},
                {
                    "type": "ref", "target": "x", "form": "normal",
                    "supplement": [note("supplement-note")],
                    "resolved": {
                        "kind": "other", "number": "1",
                        "text": [note("ignored-ref")], "anchor": "x",
                    },
                },
                {
                    "type": "cite",
                    "items": [
                        {
                            "key": "k", "prefix": [note("prefix-note")],
                            "suffix": [note("suffix-note")],
                        },
                    ],
                    "mode": "normal",
                    "resolved": {
                        "inlines": [note("ignored-cite")], "note": true,
                    },
                },
            ],
        })),
        Block::Cell(Cell {
            outputs: vec![CellOutput {
                candidates: vec![
                    OutputCandidate {
                        mime: "text/html".into(),
                        blocks: vec![paragraph("candidate-one")],
                    },
                    OutputCandidate {
                        mime: "text/plain".into(),
                        blocks: vec![paragraph("candidate-two")],
                    },
                ],
                ..CellOutput::default()
            }],
            ..Cell::default()
        }),
        block(json!({
            "type": "block_quote", "blocks": [{
                "type": "list", "kind": {"kind": "bullet"}, "tight": false,
                "items": [{"blocks": [{
                    "type": "div", "attrs": {},
                    "blocks": [paragraph("nested-block")],
                }]}],
            }],
        })),
    ]);
    document.meta.abstract_ = Some(vec![Block::paragraph(vec![
        Inline::text("abstract"),
        inline(note("abstract-note")),
    ])]);
    document.resolved.toc.push(TocEntry {
        level: 1,
        number: None,
        inlines: vec![inline(note("ignored-summary"))],
        anchor: "x".into(),
    });
    document
}

fn block_markers(document: &Document) -> Vec<String> {
    let mut markers = Vec::new();
    walk_blocks(document, &mut |block| {
        markers.push(match block {
            Block::Paragraph { inlines, .. } => {
                format!("p:{}", plain_text(inlines))
            }
            _ => block_tag(block).into(),
        });
    });
    markers
}

#[test]
fn walks_are_preorder_and_reach_footnotes_and_candidates() {
    let document = traversal_document();
    assert_eq!(
        block_markers(&document),
        [
            "p:abstract", "p:abstract-note", "figure", "p:caption-note",
            "p:figure-body", "callout", "p:title-note", "p:callout-body",
            "definition_list", "p:term-note", "p:definition-body", "table",
            "p:head-note", "p:body-note", "p:foot-note", "p:nested-text",
            "p:link-note", "p:alt-note", "p:span-note", "p:supplement-note",
            "p:prefix-note", "p:suffix-note", "cell", "p:candidate-one",
            "p:candidate-two", "block_quote", "list", "div", "p:nested-block",
        ]
    );
    let mut visited = Vec::new();
    walk_inlines(&document, &mut |inline| {
        visited.push(match inline {
            Inline::Text { text } => text.clone(),
            _ => inline_tag(inline).to_owned(),
        });
    });
    assert_eq!(&visited[..3], ["abstract", "footnote", "abstract-note"]);
    let strong = visited.iter().position(|s| s == "strong").unwrap();
    assert_eq!(&visited[strong..strong + 3], ["strong", "emph", "nested-text"]);
    for expected in [
        "caption-note", "title-note", "term-note", "head-note", "body-note",
        "foot-note", "link-note", "alt-note", "span-note", "supplement-note",
        "prefix-note", "suffix-note", "candidate-one", "candidate-two",
        "nested-block",
    ] {
        assert!(
            visited.iter().any(|s| s == expected),
            "{expected}: {visited:?}"
        );
    }
    assert!(!visited.iter().any(|s| s.starts_with("ignored")));
}

#[test]
fn nested_footnote_blocks_are_traversed_once() {
    let inner = inline(note("inner"));
    let outer = Inline::Footnote {
        blocks: vec![Block::paragraph(vec![Inline::text("outer"), inner])],
        label: None,
        number: None,
    };
    let document = document(vec![Block::paragraph(vec![outer])]);
    assert_eq!(block_markers(&document), ["p:", "p:outer", "p:inner"]);
    let mut text = Vec::new();
    walk_inlines(&document, &mut |inline| {
        if let Inline::Text { text: value } = inline {
            text.push(value.clone());
        }
    });
    assert_eq!(text, ["outer", "inner"]);
}

#[test]
fn walk_blocks_mut_edits_in_place() {
    let mut document = traversal_document();
    let before = block_markers(&document);
    let mut visits = 0;
    walk_blocks_mut(&mut document, &mut |block| {
        visits += 1;
        if let Block::Paragraph { inlines, .. } = block {
            inlines.insert(0, Inline::text("edited:"));
        }
    });
    assert_eq!(visits, before.len());
    let after = block_markers(&document);
    for (before, after) in before.iter().zip(&after) {
        if let Some(text) = before.strip_prefix("p:") {
            assert_eq!(*after, format!("p:edited:{text}"));
        } else {
            assert_eq!(before, after);
        }
    }
    let resolved = serde_json::to_string(&document.resolved).unwrap();
    assert!(resolved.contains("ignored-summary"));
    assert!(!resolved.contains("edited:"));
}

#[test]
fn block_spans_cover_every_variant() {
    let document = sample();
    walk_blocks(&document, &mut |block| {
        if !matches!(block, Block::Toc { .. } | Block::Bibliography { .. }) {
            assert!(block.span().is_some(), "{}", block_tag(block));
        }
    });
}

fn candidate(format: &str) -> OutputCandidate {
    OutputCandidate {
        mime: format!("text/{format}"),
        blocks: vec![Block::Raw {
            format: format.into(),
            text: format.into(),
            span: None,
        }],
    }
}

#[test]
fn pick_takes_the_first_renderable_candidate() {
    let image = block(json!({
        "type": "paragraph",
        "inlines": [
            {
                "type": "image", "alt": [], "url": "/image.png",
                "title": null, "attrs": {},
            },
        ],
    }));
    let output = CellOutput {
        candidates: vec![
            candidate("html"),
            OutputCandidate { mime: "image/png".into(), blocks: vec![image] },
            OutputCandidate {
                mime: "text/plain".into(),
                blocks: vec![block(json!({
                    "type": "code_block", "lang": null, "text": "fallback",
                    "attrs": {},
                }))],
            },
        ],
        ..CellOutput::default()
    };
    assert_eq!(output.pick("html"), output.candidates[0].blocks);
    for target in ["typst", "docx", "latex"] {
        assert_eq!(output.pick(target), output.candidates[1].blocks);
    }
    for (target, format) in [
        ("html", "html"),
        ("typst", "typst"),
        ("docx", "openxml"),
        ("latex", "latex"),
    ] {
        let output = CellOutput {
            candidates: vec![candidate(format)],
            ..CellOutput::default()
        };
        assert_eq!(output.pick(target), output.candidates[0].blocks);
    }
    // Raw blocks in footnotes and every nested cell candidate count.
    for nested in [
        block(json!({
            "type": "paragraph",
            "inlines": [
                {
                    "type": "footnote", "label": null,
                    "blocks": [
                        {"type": "raw", "format": "html", "text": "foreign"},
                    ],
                },
            ],
        })),
        Block::Cell(Cell {
            outputs: vec![CellOutput {
                candidates: vec![candidate("html")],
                ..CellOutput::default()
            }],
            ..Cell::default()
        }),
    ] {
        let output = CellOutput {
            candidates: vec![
                OutputCandidate {
                    mime: "text/markdown".into(),
                    blocks: vec![nested],
                },
                OutputCandidate {
                    mime: "text/plain".into(),
                    blocks: vec![paragraph("fallback")],
                },
            ],
            ..CellOutput::default()
        };
        assert_eq!(output.pick("docx"), output.candidates[1].blocks);
    }
    let output = CellOutput {
        candidates: vec![candidate("html"), candidate("latex")],
        ..CellOutput::default()
    };
    assert!(output.pick("typst").is_empty());
}

#[test]
fn pick_ignores_inline_raw() {
    let output = CellOutput {
        candidates: vec![OutputCandidate {
            mime: "text/markdown".into(),
            blocks: vec![Block::paragraph(vec![Inline::Raw {
                format: "html".into(),
                text: "inline".into(),
            }])],
        }],
        ..CellOutput::default()
    };
    for target in ["html", "typst", "docx", "latex"] {
        assert_eq!(output.pick(target), output.candidates[0].blocks);
    }
}

#[test]
fn pick_of_no_candidates_is_empty() {
    assert!(CellOutput::default().pick("html").is_empty());
    let output = CellOutput {
        candidates: vec![
            candidate("html"),
            OutputCandidate { mime: String::new(), blocks: Vec::new() },
        ],
        ..CellOutput::default()
    };
    assert!(output.pick("docx").is_empty());
}

#[test]
fn detect_kind_by_extension() {
    for (path, kind) in [
        ("a.MD", SourceKind::Markdown),
        ("b.markdown", SourceKind::Markdown),
        ("c.ipynb", SourceKind::Notebook),
        ("d.typ", SourceKind::Typst),
        ("e.TYP", SourceKind::Typst),
    ] {
        assert_eq!(detect_kind(Path::new(path)).unwrap(), kind);
    }
    for name in ["e.txt", "document", "a.md.txt"] {
        let error = detect_kind(Path::new(name)).unwrap_err().to_string();
        for expected in [name, ".md", ".markdown", ".ipynb", ".typ"] {
            assert!(error.contains(expected), "{error}");
        }
    }
}
