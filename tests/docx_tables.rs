//! Editable DOCX tables and malformed-grid recovery contracts.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use quick_xml::events::Event;
use serde_json::{Value, json};
use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::docx::{
    DocxOptions, DocxOutput, Picture, Renderer, ValidateOptions, validate,
    write,
};
use writer::error::{Error, Result};
use writer::ir::{Document, MathDialect};
use writer::resolve::anchor;

const PNG: &[u8] = include_bytes!("fixtures/md/images/plot.png");

struct NoRenderer;

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("table tests do not render equations".into()))
    }

    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("table tests only use native PNG images".into()))
    }

    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("table tests do not render MathML".into()))
    }
}

fn document(blocks: Vec<Value>) -> Document {
    serde_json::from_value(json!({
        "meta": {}, "blocks": blocks, "features": {},
        "source": {
            "path": "/tables.md", "kind": "markdown", "base_dir": "/"
        },
        "assets": [], "includes": [], "resolved": {}
    }))
    .unwrap()
}

fn text(text: &str) -> Value {
    json!({"type": "text", "text": text})
}

fn cell(text_value: &str) -> Value {
    json!({"inlines": [text(text_value)]})
}

fn table(columns: Vec<Value>, rows: Vec<Vec<Value>>) -> Value {
    let rows: Vec<_> =
        rows.into_iter().map(|cells| json!({"cells": cells})).collect();
    json!({
        "type": "table", "columns": columns,
        "head": [], "body": rows, "foot": [], "attrs": {}
    })
}

fn note(label: &str, number: u32, contents: &str) -> Value {
    json!({
        "type": "footnote", "label": label, "number": number,
        "blocks": [{"type": "paragraph", "inlines": [text(contents)]}]
    })
}

fn image() -> Value {
    json!({
        "type": "image", "url": "asset:32debefb8003",
        "alt": [text("plot")], "attrs": {"pairs": {"width": "100%"}}
    })
}

fn emit(doc: &Document) -> DocxOutput {
    let mut assets = AssetRegistry::new();
    assert_eq!(
        assets.register_bytes(PNG.to_vec(), "image/png", "plot.png"),
        "32debefb8003"
    );
    let output = write(
        doc,
        &assets,
        &mut NoRenderer,
        &DocxOptions {
            config: &Config::default(),
            reference: None,
            source_date_epoch: Some("0".into()),
            all_styles: false,
        },
    )
    .unwrap();
    let violations =
        validate(&output.bytes, &ValidateOptions { builtin_styles: true });
    assert!(violations.is_empty(), "{violations:#?}");
    output
}

fn fixture() -> Document {
    serde_json::from_str(include_str!("fixtures/docx/ir/tables.json")).unwrap()
}

fn xml_parts(output: &DocxOutput) -> BTreeMap<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(&output.bytes)).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let mut part = archive.by_index(index).unwrap();
        let name = part.name().unwrap().into_owned();
        if name.ends_with(".xml") || name.ends_with(".rels") {
            let mut xml = String::new();
            part.read_to_string(&mut xml).unwrap();
            parts.insert(name, xml);
        }
    }
    parts
}

fn elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    xml.split(&start)
        .skip(1)
        .map(|rest| rest.split_once(&end).unwrap().0)
        .collect()
}

fn values(xml: &str, tag: &str, attribute: &str) -> Vec<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut values = Vec::new();
    loop {
        match reader.read_event().unwrap() {
            Event::Start(element) | Event::Empty(element)
                if element.name().as_ref() == tag =>
            {
                for value in element.attributes() {
                    let value = value.unwrap();
                    if value.key.as_ref() == attribute {
                        values.push(
                            value
                                .normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                )
                                .unwrap()
                                .into_owned(),
                        );
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    values
}

fn numbers(xml: &str, tag: &str, attribute: &str) -> Vec<u32> {
    values(xml, tag, attribute)
        .iter()
        .map(|value| value.parse().unwrap())
        .collect()
}

fn usable_width(xml: &str) -> u32 {
    numbers(xml, "w:pgSz", "w:w")[0]
        - numbers(xml, "w:pgMar", "w:left")[0]
        - numbers(xml, "w:pgMar", "w:right")[0]
        - numbers(xml, "w:pgMar", "w:gutter")[0]
}

#[test]
fn table_fixture_is_valid_resolved_ir_and_emits_without_limitations() {
    let doc = fixture();
    assert_eq!(doc.invariant_violations(), Vec::<String>::new());
    let output = emit(&doc);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let parts = xml_parts(&output);
    assert_eq!(elements(&parts["word/document.xml"], "w:tbl").len(), 5);
    assert!(parts["word/footnotes.xml"].contains("Cell note text"));
}

#[test]
fn fractional_grids_normalize_only_when_the_sum_exceeds_one() {
    let doc = document(vec![
        table(
            vec![json!({"width": 0.25}), json!({"width": 0.5})],
            vec![vec![cell("quarter"), cell("half")]],
        ),
        table(
            vec![json!({"width": 2.0}), json!({"width": 1.0})],
            vec![vec![cell("two thirds"), cell("one third")]],
        ),
    ]);
    let output = emit(&doc);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let width = f64::from(usable_width(xml));
    let tables = elements(xml, "w:tbl");
    assert_eq!(
        numbers(tables[0], "w:gridCol", "w:w"),
        vec![(0.25 * width).round() as u32, (0.5 * width).round() as u32,]
    );
    assert_eq!(
        numbers(tables[1], "w:gridCol", "w:w"),
        vec![(2.0 / 3.0 * width).round() as u32, (width / 3.0).round() as u32,]
    );
    for (table, percent) in tables.iter().zip([3750, 5000]) {
        assert!(
            table.contains(&format!(
                "<w:tblW w:w=\"{percent}\" w:type=\"pct\"/>"
            ))
        );
        assert!(table.contains("<w:tblLayout w:type=\"fixed\"/>"));
        assert_eq!(
            numbers(table, "w:gridCol", "w:w"),
            numbers(table, "w:tcW", "w:w"),
        );
    }
}

#[test]
fn missing_fraction_uses_equal_columns_and_last_column_remainder() {
    let doc = document(vec![table(
        vec![json!({"width": 0.9}), json!({}), json!({}), json!({})],
        vec![vec![cell("A"), cell("B"), cell("C"), cell("D")]],
    )]);
    let output = emit(&doc);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let width = usable_width(xml);
    let grid = numbers(xml, "w:gridCol", "w:w");
    assert_eq!(
        grid,
        vec![width / 4, width / 4, width / 4, width / 4 + width % 4]
    );
    assert_eq!(grid.iter().sum::<u32>(), width);
    assert!(xml.contains("<w:tblW w:w=\"0\" w:type=\"auto\"/>"));
    assert!(!xml.contains("<w:tblLayout"));
}

#[test]
fn table_styles_and_plain_caption_follow_precedence() {
    let output = emit(&fixture());
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    assert_eq!(
        values(xml, "w:tblStyle", "w:val"),
        ["CellGrid", "FigureGrid", "Table", "Table", "Table",]
    );
    let tables = elements(xml, "w:tbl");
    assert!(
        tables[0]
            .contains("<w:tblCaption w:val=\"Values &amp; &lt;units&gt;\"/>")
    );
    assert!(tables[1].contains("<w:tblCaption w:val=\"Inherited style\"/>"));
    assert!(!tables[2..].iter().any(|table| table.contains("w:tblCaption")));
    for style in ["CellGrid", "FigureGrid"] {
        assert!(parts["word/styles.xml"].contains(&format!(
            "w:type=\"table\" w:customStyle=\"1\" w:styleId=\"{style}\""
        )));
    }
    let doc = document(vec![json!({
        "type": "figure", "kind": "table", "caption": null,
        "attrs": {}, "body": [table(vec![json!({})], vec![vec![cell("No caption")]])]
    })]);
    let output = emit(&doc);
    assert!(!xml_parts(&output)["word/document.xml"].contains("w:tblCaption"));
}

#[test]
fn headers_footers_spans_alignment_and_compact_cells_are_editable() {
    let output = emit(&fixture());
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let table = elements(xml, "w:tbl")[0];
    assert_eq!(values(table, "w:tblLook", "w:val"), ["0060"]);
    assert_eq!(values(table, "w:tblLook", "w:firstRow"), ["1"]);
    assert_eq!(values(table, "w:tblLook", "w:lastRow"), ["1"]);
    assert_eq!(table.matches("<w:tblHeader/>").count(), 1);
    let rows = elements(table, "w:tr");
    assert_eq!(rows.len(), 5);
    let cells: Vec<_> = rows.iter().map(|row| elements(row, "w:tc")).collect();
    assert_eq!(cells.iter().map(Vec::len).collect::<Vec<_>>(), [3, 2, 2, 3, 1]);
    assert!(cells[1][0].contains("<w:gridSpan w:val=\"2\"/>"));
    assert!(cells[1][0].contains("<w:vMerge w:val=\"restart\"/>"));
    assert!(cells[2][0].contains("<w:gridSpan w:val=\"2\"/>"));
    assert!(cells[2][0].contains("<w:vMerge/>"));
    assert!(!cells[2][0].contains("<w:r>"));
    let grid = numbers(table, "w:gridCol", "w:w");
    assert_eq!(numbers(cells[1][0], "w:tcW", "w:w"), [grid[0] + grid[1]]);
    assert_eq!(
        numbers(cells[4][0], "w:tcW", "w:w"),
        [grid.iter().sum::<u32>()]
    );
    assert_eq!(values(rows[0], "w:jc", "w:val"), ["left", "center", "right"]);
    assert_eq!(values(cells[1][0], "w:jc", "w:val"), ["center"]);
    assert!(cells[1][0].contains("<w:b/><w:bCs/>"));
    assert!(cells[3][0].contains("<w:b/><w:bCs/>"));
    assert!(!rows[0].contains("<w:b/>"));
    assert!(!rows[4].contains("<w:b/>"));
    assert!(!cells[3][2].contains("<w:jc"));
    for cell in cells.into_iter().flatten() {
        assert_eq!(values(cell, "w:pStyle", "w:val"), ["Compact"]);
        assert!(cell.ends_with("</w:p>"));
    }
}

#[test]
fn rich_cell_content_preserves_links_notes_images_and_forward_bookmarks() {
    let output = emit(&fixture());
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let target = anchor("tbl-rich");
    assert!(xml.contains(&format!("w:anchor=\"{target}\"")));
    assert!(xml.contains(&format!("w:name=\"{target}\"")));
    assert!(xml.contains("<w:footnoteReference w:id=\"1\"/>"));
    assert!(xml.contains("<w:drawing>"));
    assert!(
        parts["word/_rels/document.xml.rels"]
            .contains("Target=\"https://example.com/table?a=1&amp;b=2\"")
    );
    assert!(
        parts["word/footnotes.xml"].contains(&format!("w:anchor=\"{target}\""))
    );
    assert!(!parts["word/_rels/document.xml.rels"].contains("Target=\"#"));
}

#[test]
fn ragged_rows_pad_around_rowspans_until_the_merge_expires() {
    let doc = document(vec![table(
        vec![json!({}); 3],
        vec![
            vec![
                json!({"inlines": [text("three rows")], "rowspan": 3}),
                json!({"inlines": [text("two columns")], "colspan": 2}),
            ],
            vec![cell("middle")],
            vec![],
            vec![json!({"inlines": [text("whole row")], "colspan": 3})],
        ],
    )]);
    let output = emit(&doc);
    assert!(output.diagnostics.is_empty());
    let parts = xml_parts(&output);
    let rows = elements(&parts["word/document.xml"], "w:tr");
    assert_eq!(
        rows.iter().map(|row| elements(row, "w:tc").len()).collect::<Vec<_>>(),
        [2, 3, 3, 1]
    );
    assert_eq!(rows[0].matches("<w:vMerge w:val=\"restart\"/>").count(), 1);
    assert_eq!(rows[1].matches("<w:vMerge/>").count(), 1);
    assert_eq!(rows[2].matches("<w:vMerge/>").count(), 1);
    assert!(!rows[3].contains("w:vMerge"));
    for row in &rows[1..3] {
        for cell in elements(row, "w:tc") {
            assert!(cell.contains("<w:pStyle w:val=\"Compact\"/>"));
        }
    }
}

#[test]
fn dropped_cells_do_not_allocate_notes_or_bookmarks_and_warn_at_the_table() {
    let mut malformed = table(
        vec![json!({}); 2],
        vec![vec![
            json!({"inlines": [text("retained")], "colspan": 2}),
            json!({"inlines": [
                {"type": "span", "attrs": {"id": "discarded"},
                    "inlines": [text("DISCARDED CELL")]},
                note("discarded-note", 1, "DISCARDED NOTE")
            ]}),
        ]],
    );
    malformed["span"] = json!({
        "start_line": 34, "end_line": 38, "cell": 2,
        "file": "/included/tables.md"
    });
    let doc = document(vec![
        malformed,
        json!({
            "type": "paragraph", "inlines": [note("retained-note", 2, "Retained note")]
        }),
    ]);
    let output = emit(&doc);
    assert_eq!(output.diagnostics.len(), 1);
    let warning = output.diagnostics.iter().next().unwrap();
    assert_eq!(warning.code, "unsupported-construct");
    assert_eq!(warning.span.as_ref().unwrap().start_line, 34);
    assert_eq!(warning.span.as_ref().unwrap().cell, Some(2));
    assert_eq!(
        warning.path.as_deref(),
        Some(std::path::Path::new("/included/tables.md"))
    );
    let parts = xml_parts(&output);
    assert!(!parts["word/document.xml"].contains("DISCARDED"));
    assert!(!parts["word/document.xml"].contains(&anchor("discarded")));
    assert_eq!(
        parts["word/document.xml"].matches("<w:footnoteReference ").count(),
        1
    );
    assert!(parts["word/footnotes.xml"].contains("Retained note"));
    assert!(!parts["word/footnotes.xml"].contains("DISCARDED NOTE"));
}

#[test]
fn crossing_a_rowspan_drops_only_the_conflicting_cell() {
    let doc = document(vec![table(
        vec![json!({}); 3],
        vec![
            vec![
                cell("left"),
                json!({"inlines": [text("merge")], "rowspan": 2}),
                cell("right"),
            ],
            vec![
                json!({"inlines": [text("DROPPED OVERLAP")], "colspan": 2}),
                cell("next left"),
                cell("next right"),
            ],
        ],
    )]);
    let output = emit(&doc);
    assert_eq!(output.diagnostics.len(), 1);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    assert!(!xml.contains("DROPPED OVERLAP"));
    let rows = elements(xml, "w:tr");
    let cells = elements(rows[1], "w:tc");
    assert_eq!(cells.len(), 3);
    assert!(cells[0].contains("next left"));
    assert!(cells[1].contains("<w:vMerge/>"));
    assert!(cells[2].contains("next right"));
}

#[test]
fn rowspans_do_not_cross_head_body_or_foot_groups() {
    let mut value = table(
        vec![json!({})],
        vec![vec![json!({
            "inlines": [text("body")], "rowspan": 2
        })]],
    );
    value["head"] =
        json!([{"cells": [{"inlines": [text("head")], "rowspan": 2}]}]);
    value["foot"] = json!([{"cells": [cell("foot")]}]);
    let doc = document(vec![value]);
    assert!(doc.invariant_violations().is_empty());
    let output = emit(&doc);
    assert!(output.diagnostics.is_empty());
    let parts = xml_parts(&output);
    let rows = elements(&parts["word/document.xml"], "w:tr");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].matches("w:vMerge").count(), 1);
    assert_eq!(rows[1].matches("w:vMerge").count(), 1);
    assert!(!rows[2].contains("w:vMerge"));
}

#[test]
fn table_look_bits_and_repeated_header_rows_match_each_row_group() {
    let mut blocks = Vec::new();
    for (head, foot) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut value = table(vec![json!({})], vec![vec![cell("body")]]);
        if head {
            value["head"] = json!([
                {"cells": [cell("first header")]},
                {"cells": [cell("second header")]},
            ]);
        }
        if foot {
            value["foot"] = json!([{"cells": [cell("footer")]}]);
        }
        blocks.push(value);
    }
    let output = emit(&document(blocks));
    let parts = xml_parts(&output);
    let tables = elements(&parts["word/document.xml"], "w:tbl");
    for (table, expected, headers) in [
        (tables[0], "0000", 0),
        (tables[1], "0020", 2),
        (tables[2], "0040", 0),
        (tables[3], "0060", 2),
    ] {
        assert_eq!(values(table, "w:tblLook", "w:val"), [expected]);
        assert_eq!(table.matches("<w:tblHeader/>").count(), headers);
        assert_eq!(values(table, "w:tblLook", "w:firstColumn"), ["0"]);
        assert_eq!(values(table, "w:tblLook", "w:lastColumn"), ["0"]);
        assert_eq!(values(table, "w:tblLook", "w:noHBand"), ["0"]);
        assert_eq!(values(table, "w:tblLook", "w:noVBand"), ["0"]);
    }
}

#[test]
fn excessive_colspan_drops_without_expanding_the_column_grid() {
    let doc = document(vec![table(
        vec![json!({}); 2],
        vec![vec![
            json!({"inlines": [text("DROPPED HUGE SPAN")], "colspan": u32::MAX}),
            cell("first"),
            cell("second"),
        ]],
    )]);
    let output = emit(&doc);
    assert_eq!(output.diagnostics.len(), 1);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    assert_eq!(values(xml, "w:gridCol", "w:w").len(), 2);
    assert_eq!(elements(xml, "w:tc").len(), 2);
    assert!(!xml.contains("DROPPED HUGE SPAN"));
    assert!(xml.contains("first"));
    assert!(xml.contains("second"));
}

#[test]
fn cell_images_use_spanned_width_minus_margins_and_restore_page_width() {
    let doc = document(vec![
        table(
            vec![json!({}); 3],
            vec![vec![
                json!({"inlines": [image()], "colspan": 2}),
                cell("last"),
            ]],
        ),
        json!({"type": "paragraph", "inlines": [image()]}),
    ]);
    let output = emit(&doc);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let grid = numbers(xml, "w:gridCol", "w:w");
    let widths = numbers(xml, "wp:extent", "cx");
    assert_eq!(
        widths,
        [(grid[0] + grid[1] - 216) * 635, usable_width(xml) * 635]
    );
}

#[test]
fn table_cells_do_not_inherit_outer_list_numbering_and_tables_end_in_paragraphs()
 {
    let value = table(vec![json!({})], vec![vec![cell("listed table")]]);
    let doc = document(vec![
        json!({"type": "list", "kind": {"kind": "ordered", "start": 1}, "tight": false,
            "items": [{"blocks": [value.clone()]}]}),
        value.clone(),
        value.clone(),
        json!({"type": "paragraph", "inlines": [{
            "type": "footnote", "label": "table-note", "number": 1,
            "blocks": [value]
        }]}),
    ]);
    let output = emit(&doc);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    assert!(xml.contains("<w:numPr>"));
    for table in elements(xml, "w:tbl") {
        assert!(!table.contains("<w:numPr>"));
        assert!(!table.contains("<w:ind"));
        assert!(table.contains("<w:pStyle w:val=\"Compact\"/>"));
    }
    assert!(xml.contains("</w:tbl><w:p/><w:tbl>"));
    assert!(
        parts["word/footnotes.xml"].contains("</w:tbl><w:p/></w:footnote>")
    );
    let trailing =
        emit(&document(vec![table(vec![json!({})], vec![vec![cell("last")]])]));
    assert!(
        xml_parts(&trailing)["word/document.xml"]
            .contains("</w:tbl><w:p/><w:sectPr>")
    );
}
