//! DOCX foundation contracts, independent of the later content emitters.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde_json::json;
use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::docx::styles::VOCABULARY;
use writer::docx::validate::{ValidateOptions, validate};
use writer::docx::{DocxOptions, DocxOutput, Picture, Renderer, write};
use writer::error::{Error, Result};
use writer::ir::{Document, MathDialect};

#[path = "common/docx.rs"]
mod common_docx;

struct NoRenderer;

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("test renderer has no equations".into()))
    }

    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("test renderer has no images".into()))
    }

    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("test renderer has no MathML".into()))
    }
}

fn document() -> Document {
    serde_json::from_value(json!({
        "meta": {
            "title": "DOCX foundations", "authors": [{"name": "A. Author"}]
        },
        "blocks": [
            {
                "type": "heading", "level": 1, "number": "1",
                "attrs": {"id": "sec-intro"},
                "inlines": [{"type": "text", "text": "Introduction"}],
                "span": {"start_line": 1, "end_line": 1}
            },
            {
                "type": "paragraph", "inlines": [
                    {"type": "text", "text": "Text with "},
                    {"type": "emph", "inlines": [
                        {"type": "text", "text": "emphasis"}
                    ]},
                    {"type": "footnote", "label": "note", "number": 1,
                        "blocks": [{"type": "paragraph", "inlines": [
                            {"type": "text", "text": "A note."}
                        ]}]
                    }
                ]
            },
            {"type": "toc", "depth": 2},
            {
                "type": "figure", "kind": "other", "body": [],
                "caption": [{"type": "text", "text": "Figure caption"}],
                "attrs": {}, "number": "1",
                "span": {"start_line": 5, "end_line": 7}
            },
            {
                "type": "table", "columns": [
                    {"align": "left"}, {"align": "right"}
                ], "head": [], "foot": [], "attrs": {},
                "body": [{"cells": [
                    {"inlines": [{"type": "text", "text": "A"}]},
                    {"inlines": [{"type": "text", "text": "1"}]}
                ]}], "span": {"start_line": 9, "end_line": 11}
            },
            {
                "type": "math", "source": "x + y", "dialect": "latex",
                "attrs": {}, "number": "1",
                "span": {"start_line": 13, "end_line": 13}
            },
            {"type": "bibliography"}, {"type": "thematic_break"}
        ],
        "features": {"footnotes": true, "math": true},
        "source": {
            "path": "/synthetic.md", "kind": "markdown", "base_dir": "/"
        },
        "assets": [], "includes": [], "resolved": {"footnotes": 1}
    }))
    .unwrap()
}

fn reference() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/docx/reference.docx")
}

fn output(
    config: &Config, reference: Option<&Path>, epoch: Option<&str>,
    all_styles: bool,
) -> Result<DocxOutput> {
    checked(
        write(
            &document(),
            &AssetRegistry::new(),
            &mut NoRenderer,
            &DocxOptions {
                config,
                reference,
                source_date_epoch: epoch.map(str::to_owned),
                all_styles,
            },
        ),
        reference.is_none(),
    )
}

fn checked(
    result: Result<DocxOutput>, builtin_styles: bool,
) -> Result<DocxOutput> {
    result.inspect(|output| {
        let violations =
            validate(&output.bytes, &ValidateOptions { builtin_styles });
        assert!(violations.is_empty(), "{violations:#?}");
    })
}

fn parts(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
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

#[test]
fn mini_document_validates_and_reads_back() {
    let result = output(&Config::default(), None, None, false).unwrap();
    let violations =
        validate(&result.bytes, &ValidateOptions { builtin_styles: true });
    assert!(violations.is_empty(), "{violations:#?}");
    let read_back = common_docx::read_back(&result.bytes);
    assert!(read_back.paragraphs >= 2);
    assert!(read_back.styles >= 3);
    assert_eq!(read_back.abstract_nums, 2);
    assert_eq!(read_back.images, 0);
}

#[test]
fn two_writes_are_identical() {
    let config = Config::default();
    for reference in [None, Some(reference())] {
        let first = output(&config, reference.as_deref(), None, false).unwrap();
        let second =
            output(&config, reference.as_deref(), None, false).unwrap();
        assert_eq!(first.bytes, second.bytes);
    }
}

#[test]
fn zip_layout() {
    let result = output(&Config::default(), None, None, false).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&result.bytes)).unwrap();
    assert_eq!(
        archive.by_index(0).unwrap().name().unwrap(),
        "[Content_Types].xml"
    );
    for index in 0..archive.len() {
        assert!(!archive.by_index(index).unwrap().is_dir());
    }
}

#[test]
fn core_dates() {
    let config = Config::default();
    let result = output(&config, None, Some("0"), false).unwrap();
    let part = &parts(&result.bytes)["docProps/core.xml"];
    assert_eq!(part.matches("1970-01-01T00:00:00Z").count(), 2);
    assert!(part.contains("<dcterms:created xsi:type=\"dcterms:W3CDTF\">"));
    assert!(part.contains("<dcterms:modified xsi:type=\"dcterms:W3CDTF\">"));
    let result = output(&config, None, None, false).unwrap();
    let part = &parts(&result.bytes)["docProps/core.xml"];
    assert!(!part.contains("<dcterms:created"));
    assert!(!part.contains("<dcterms:modified"));
    let mut config = config;
    config.pdf.timestamp = "yesterday".into();
    assert!(matches!(
        output(&config, None, None, false),
        Err(Error::Config(_))
    ));
}

#[test]
fn update_fields_follows_config() {
    let mut config = Config::default();
    for enabled in [true, false] {
        config.docx.update_fields = enabled;
        let result = output(&config, None, None, false).unwrap();
        let part = &parts(&result.bytes)["word/settings.xml"];
        assert_eq!(part.contains("<w:updateFields"), enabled);
        if enabled {
            assert!(part.contains("<w:updateFields w:val=\"true\"/>"));
        }
    }
}

#[test]
fn all_styles_writes_the_vocabulary() {
    let result = output(&Config::default(), None, None, true).unwrap();
    let part = &parts(&result.bytes)["word/styles.xml"];
    assert_eq!(VOCABULARY.len(), 101);
    for &(name, _) in VOCABULARY {
        let name = writer::docx::xml::escape(name);
        assert!(part.contains(&format!("<w:name w:val=\"{name}\"")), "{name}");
    }
    assert!(
        validate(&result.bytes, &ValidateOptions { builtin_styles: true },)
            .is_empty()
    );
}

#[test]
fn math_stub_diagnostics_keep_their_owner_and_source_span() {
    let result = output(&Config::default(), None, None, false).unwrap();
    let diagnostics: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == "unsupported-construct")
        .collect();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = diagnostics[0];
    assert!(diagnostic.message.contains("T42"));
    assert!(diagnostic.message.starts_with("display math"));
    assert_eq!(diagnostic.span.as_ref().unwrap().start_line, 13);
}

#[test]
fn footnotes_in_resolved_citations_reserve_the_part() {
    let mut doc = document();
    let note = json!({
        "type": "footnote", "label": "note", "number": 1,
        "blocks": [{"type": "paragraph", "inlines": [
            {"type": "text", "text": "A citation note."}
        ]}]
    });
    doc.blocks = vec![
        serde_json::from_value(json!({
            "type": "paragraph", "inlines": [{
                "type": "cite", "items": [], "mode": "normal", "resolved": {
                    "inlines": [note], "note": true
                }
            }]
        }))
        .unwrap(),
    ];
    let config = Config::default();
    let result = write(
        &doc,
        &AssetRegistry::new(),
        &mut NoRenderer,
        &DocxOptions {
            config: &config,
            reference: None,
            source_date_epoch: None,
            all_styles: false,
        },
    )
    .unwrap();
    let parts = parts(&result.bytes);
    assert!(parts.contains_key("word/footnotes.xml"));
    assert!(
        parts["word/_rels/document.xml.rels"]
            .contains("Target=\"footnotes.xml\"")
    );
    assert!(
        validate(&result.bytes, &ValidateOptions { builtin_styles: true },)
            .is_empty()
    );
}

#[test]
fn language_preserves_the_script_subtag() {
    let mut doc = document();
    doc.meta.lang = Some("zh-Hant-TW".into());
    let config = Config::default();
    let result = write(
        &doc,
        &AssetRegistry::new(),
        &mut NoRenderer,
        &DocxOptions {
            config: &config,
            reference: None,
            source_date_epoch: None,
            all_styles: false,
        },
    )
    .unwrap();
    let parts = parts(&result.bytes);
    assert!(
        parts["docProps/core.xml"]
            .contains("<dc:language>zh-Hant-TW</dc:language>")
    );
    assert!(parts["word/styles.xml"].contains("<w:lang w:val=\"zh-Hant-TW\""));
    assert!(
        validate(&result.bytes, &ValidateOptions { builtin_styles: true },)
            .is_empty()
    );
}

fn write_document(
    doc: &Document, assets: &AssetRegistry,
) -> Result<DocxOutput> {
    checked(
        write(
            doc,
            assets,
            &mut NoRenderer,
            &DocxOptions {
                config: &Config::default(),
                reference: None,
                source_date_epoch: None,
                all_styles: false,
            },
        ),
        true,
    )
}

#[test]
fn unknown_assets_fail_even_in_unused_output_candidates() {
    use writer::ir::{Attrs, Block, Cell, CellOutput, Inline, OutputCandidate};
    let image = Block::paragraph(vec![Inline::Image {
        alt: vec![],
        url: "asset:0123456789ab".into(),
        title: None,
        attrs: Attrs::default(),
    }]);
    let assets = AssetRegistry::new();
    let mut doc = document();
    doc.blocks = vec![image.clone()];
    assert!(
        write_document(&doc, &assets)
            .unwrap_err()
            .to_string()
            .contains("unknown asset")
    );
    doc.blocks = vec![Block::Cell(Cell {
        outputs: vec![CellOutput {
            candidates: vec![
                OutputCandidate { mime: "text/plain".into(), blocks: vec![] },
                OutputCandidate {
                    mime: "image/png".into(),
                    blocks: vec![image],
                },
            ],
            ..CellOutput::default()
        }],
        ..Cell::default()
    })];
    assert!(
        write_document(&doc, &assets)
            .unwrap_err()
            .to_string()
            .contains("unknown asset")
    );
}

#[test]
fn notes_in_abstracts_and_unused_candidates_reserve_the_part() {
    use writer::ir::{Block, Cell, CellOutput, Inline, OutputCandidate};
    let note = Block::paragraph(vec![Inline::Footnote {
        blocks: vec![],
        label: None,
        number: Some(1),
    }]);
    let mut doc = document();
    doc.blocks = vec![];
    doc.meta.abstract_ = Some(vec![note.clone()]);
    let assets = AssetRegistry::new();
    let result = write_document(&doc, &assets).unwrap();
    assert!(parts(&result.bytes).contains_key("word/footnotes.xml"));
    doc.meta.abstract_ = None;
    doc.blocks = vec![Block::Cell(Cell {
        outputs: vec![CellOutput {
            candidates: vec![
                OutputCandidate { mime: "text/plain".into(), blocks: vec![] },
                OutputCandidate {
                    mime: "text/html".into(),
                    blocks: vec![note],
                },
            ],
            ..CellOutput::default()
        }],
        ..Cell::default()
    })];
    let result = write_document(&doc, &assets).unwrap();
    assert!(parts(&result.bytes).contains_key("word/footnotes.xml"));
    assert!(
        validate(&result.bytes, &ValidateOptions { builtin_styles: true },)
            .is_empty()
    );
}
