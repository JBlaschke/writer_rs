//! Native, converted and fallback DOCX image contracts.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

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

const PNG: &[u8] = include_bytes!("fixtures/docx/images/dpi144.png");
const JPEG: &[u8] = include_bytes!("fixtures/docx/images/jfif300.jpg");
const SVG: &[u8] = include_bytes!("fixtures/docx/images/drawing.svg");
const GIF: &[u8] = include_bytes!("fixtures/docx/images/anim.gif");
const WEBP: &[u8] = include_bytes!("fixtures/docx/images/pic.webp");
const WIDTH: u64 = 9_026 * 635;
const HEIGHT: u64 = 13_958 * 635;

#[derive(Default)]
struct FakeRenderer {
    calls: Vec<(String, Option<f64>)>,
    fail_natural: bool,
    fail_display: bool,
}

impl Renderer for FakeRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("equations are outside these image tests".into()))
    }

    fn image(
        &mut self, _: &[u8], mime: &str, width: Option<f64>,
    ) -> Result<Picture> {
        self.calls.push((mime.into(), width));
        if (width.is_none() && self.fail_natural)
            || (width.is_some() && self.fail_display)
        {
            return Err(Error::Other(
                "deliberate image conversion failure".into(),
            ));
        }
        Ok(Picture {
            png: PNG.to_vec(),
            svg: None,
            width_pt: 240.0,
            height_pt: 120.0,
            baseline_pt: 120.0,
        })
    }

    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("MathML is outside these image tests".into()))
    }
}

fn document(blocks: Vec<Value>) -> Document {
    serde_json::from_value(json!({
        "meta": {}, "blocks": blocks, "features": {}, "assets": [],
        "source": {"path": "/images.md", "kind": "markdown", "base_dir": "/"}
    }))
    .unwrap()
}

fn image(url: &str, pairs: Value) -> Value {
    json!({
        "type": "image", "url": url,
        "alt": [{"type": "text", "text": "  Rich\n image\t alt & description  "}],
        "title": "Image \"title\"", "attrs": {"pairs": pairs}
    })
}

fn paragraph(inlines: Vec<Value>) -> Value {
    json!({
        "type": "paragraph", "inlines": inlines,
        "span": {"start_line": 7, "end_line": 8, "file": "/included.md", "cell": 3}
    })
}

fn register(assets: &mut AssetRegistry, bytes: &[u8], mime: &str) -> String {
    let id = assets.register_bytes(bytes.to_vec(), mime, "test-image");
    format!("asset:{id}")
}

fn emit(
    doc: &Document, assets: &AssetRegistry, renderer: &mut FakeRenderer,
) -> DocxOutput {
    let output = write(
        doc,
        assets,
        renderer,
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

fn parts(output: &DocxOutput) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(&output.bytes)).unwrap();
    let mut result = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        result.insert(name, bytes);
    }
    result
}

fn xml(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8(parts[name].clone()).unwrap()
}

fn attributes(source: &str, name: &str, keys: &[&str]) -> Vec<Vec<String>> {
    let mut reader = quick_xml::Reader::from_str(source);
    let mut result = Vec::new();
    loop {
        match reader.read_event().unwrap() {
            Event::Start(node) | Event::Empty(node)
                if node.name().as_ref() == name =>
            {
                result.push(
                    keys.iter()
                        .map(|key| {
                            let attribute = node
                                .attributes()
                                .map(|a| a.unwrap())
                                .find(|attribute| {
                                    attribute.key.as_ref() == *key
                                })
                                .unwrap();
                            attribute
                                .normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                )
                                .unwrap()
                                .into_owned()
                        })
                        .collect(),
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    result
}

fn extents(source: &str) -> Vec<(u64, u64)> {
    attributes(source, "wp:extent", &["cx", "cy"])
        .into_iter()
        .map(|values| (values[0].parse().unwrap(), values[1].parse().unwrap()))
        .collect()
}

#[test]
fn native_bytes_dpi_and_accessible_drawing_properties() {
    let mut assets = AssetRegistry::new();
    let png = register(&mut assets, PNG, "image/png");
    let jpeg = register(&mut assets, JPEG, "image/jpeg");
    let doc = document(vec![paragraph(vec![
        image(&png, json!({})),
        image(&jpeg, json!({})),
    ])]);
    let mut renderer = FakeRenderer::default();
    let output = emit(&doc, &assets, &mut renderer);
    assert!(renderer.calls.is_empty());
    assert!(output.diagnostics.is_empty());
    let parts = parts(&output);
    assert_eq!(parts["word/media/w1.png"], PNG);
    assert_eq!(parts["word/media/w2.jpeg"], JPEG);
    let document = xml(&parts, "word/document.xml");
    assert_eq!(extents(&document), [(1_828_800, 914_400), (914_400, 457_200)]);
    assert_eq!(
        attributes(&document, "wp:docPr", &["id", "descr", "title"]),
        [
            ["1", "Rich image alt & description", "Image \"title\""],
            ["2", "Rich image alt & description", "Image \"title\""],
        ]
    );
}

#[test]
fn dimensions_support_every_unit_ratio_and_both_axes() {
    let mut assets = AssetRegistry::new();
    let url = register(&mut assets, PNG, "image/png");
    let mut inlines: Vec<_> =
        ["192", "192px", "144pt", "5.08cm", "50.8mm", "2in", "12em"]
            .into_iter()
            .map(|width| image(&url, json!({"width": width})))
            .collect();
    inlines.extend([
        image(&url, json!({"height": "1in"})),
        image(&url, json!({"width": "100pt", "height": "25pt"})),
        image(&url, json!({"width": "50%"})),
        image(&url, json!({"height": "25%"})),
        image(&url, json!({"width": "200%"})),
        image(&url, json!({"width": "0"})),
        image(&url, json!({"width": "1in", "height": "100in"})),
    ]);
    let output = emit(
        &document(vec![paragraph(inlines)]),
        &assets,
        &mut FakeRenderer::default(),
    );
    let parts = parts(&output);
    let sizes = extents(&xml(&parts, "word/document.xml"));
    assert_eq!(&sizes[..8], &[(1_828_800, 914_400); 8]);
    assert_eq!(sizes[8], (1_270_000, 317_500));
    assert_eq!(sizes[9], (WIDTH / 2, WIDTH / 4 + 1));
    assert_eq!(sizes[10], (HEIGHT / 2, HEIGHT / 4 + 1));
    assert_eq!(sizes[11], (WIDTH, WIDTH / 2));
    assert_eq!(sizes[12], (1, 1));
    assert_eq!(sizes[13], ((HEIGHT as f64 / 100.0).round() as u64, HEIGHT));
    assert_eq!(
        parts.keys().filter(|name| name.starts_with("word/media/")).count(),
        1
    );
}

#[test]
fn vector_and_converted_formats_use_display_width() {
    let mut assets = AssetRegistry::new();
    let mut images = Vec::new();
    for (bytes, mime) in [
        (SVG, "image/svg+xml"),
        (GIF, "image/gif"),
        (WEBP, "image/webp"),
        (b"%PDF-1.7\nfixture".as_slice(), "application/pdf"),
    ] {
        let url = register(&mut assets, bytes, mime);
        images.push(image(&url, json!({"width": "2in"})));
    }
    let mut renderer = FakeRenderer::default();
    let output =
        emit(&document(vec![paragraph(images)]), &assets, &mut renderer);
    let parts = parts(&output);
    assert!(output.diagnostics.is_empty());
    assert_eq!(parts["word/media/w2.svg"], SVG);
    for name in ["w1.png", "w3.png", "w4.png", "w5.png"] {
        assert_eq!(parts[&format!("word/media/{name}")], PNG);
    }
    let document = xml(&parts, "word/document.xml");
    assert!(document.contains("<asvg:svgBlip"));
    assert_eq!(extents(&document), [(1_828_800, 914_400); 4]);
    assert_eq!(
        renderer.calls.iter().filter(|(_, width)| width.is_none()).count(),
        2
    );
    assert!(
        renderer
            .calls
            .iter()
            .filter_map(|(_, width)| *width)
            .all(|width| width == 144.0)
    );
}

#[test]
fn svg_fallback_failure_keeps_vector_and_warns_once_at_source() {
    let mut assets = AssetRegistry::new();
    let url = register(&mut assets, SVG, "image/svg+xml");
    let mut renderer = FakeRenderer {
        fail_natural: true,
        fail_display: true,
        ..Default::default()
    };
    let output = emit(
        &document(vec![paragraph(vec![image(&url, json!({}))])]),
        &assets,
        &mut renderer,
    );
    let parts = parts(&output);
    assert_eq!(parts["word/media/w1.svg"], SVG);
    assert!(!parts.contains_key("word/media/w2.png"));
    assert_eq!(
        extents(&xml(&parts, "word/document.xml")),
        [(5_486_400, 3_657_600)]
    );
    let warnings: Vec<_> = output.diagnostics.iter().collect();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, "docx-svg-no-fallback");
    assert_eq!(warnings[0].path.as_deref(), Some(Path::new("/included.md")));
    let span = warnings[0].span.as_ref().unwrap();
    assert_eq!((span.start_line, span.cell), (7, Some(3)));
}

#[test]
fn non_svg_bytes_use_alt_text_while_svg_preambles_keep_the_vector_fallback() {
    let mut assets = AssetRegistry::new();
    let invalid =
        register(&mut assets, b"<html>not SVG</html>", "image/svg+xml");
    let valid = [
        b"\xef\xbb\xbf<?xml version=\"1.0\"?><!-- test --><!DOCTYPE svg>"
            .as_slice(),
        SVG,
    ]
    .concat();
    let svg = register(&mut assets, &valid, "image/svg+xml");
    let doc = document(vec![paragraph(vec![
        image(&invalid, json!({})),
        image(&svg, json!({})),
    ])]);
    let mut renderer = FakeRenderer {
        fail_natural: true,
        fail_display: true,
        ..Default::default()
    };
    let output = emit(&doc, &assets, &mut renderer);
    let parts = parts(&output);
    assert_eq!(parts["word/media/w1.svg"], valid);
    assert_eq!(renderer.calls.len(), 2);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        ["unsupported-construct", "docx-svg-no-fallback"]
    );
    assert_eq!(extents(&xml(&parts, "word/document.xml")).len(), 1);
}

#[test]
fn failed_conversion_and_invalid_native_images_become_located_alt_text() {
    let mut assets = AssetRegistry::new();
    let mut inlines = Vec::new();
    for (bytes, mime) in [
        (b"broken png".as_slice(), "image/png"),
        (b"broken jpeg", "image/jpeg"),
        (b"broken gif", "image/gif"),
        (b"broken webp", "image/webp"),
        (GIF, "image/gif"),
        (WEBP, "image/webp"),
        (b"%PDF-1.7\nfixture", "application/pdf"),
        (b"other image", "image/avif"),
    ] {
        let url = register(&mut assets, bytes, mime);
        inlines.push(image(&url, json!({})));
    }
    let mut renderer =
        FakeRenderer { fail_display: true, ..Default::default() };
    let output =
        emit(&document(vec![paragraph(inlines)]), &assets, &mut renderer);
    let parts = parts(&output);
    assert!(!parts.keys().any(|name| name.starts_with("word/media/")));
    assert_eq!(output.diagnostics.len(), 8);
    assert!(output.diagnostics.iter().all(|warning| {
        warning.code == "unsupported-construct"
            && warning.span.as_ref().is_some_and(|span| {
                span.start_line == 7 && span.cell == Some(3)
            })
    }));
    assert_eq!(
        xml(&parts, "word/document.xml")
            .matches("Rich image alt &amp; description")
            .count(),
        8
    );
}

#[test]
fn remote_images_are_hyperlinks_without_renderer_calls() {
    let doc = document(vec![paragraph(vec![
        image("https://example.invalid/image.png?x=1&y=2", json!({})),
        json!({"type": "image", "url": "other:image", "alt": [], "attrs": {}}),
    ])]);
    let mut renderer = FakeRenderer::default();
    let output = emit(&doc, &AssetRegistry::new(), &mut renderer);
    assert!(renderer.calls.is_empty());
    assert_eq!(output.diagnostics.len(), 2);
    let parts = parts(&output);
    let document = xml(&parts, "word/document.xml");
    assert_eq!(document.matches("<w:hyperlink ").count(), 2);
    assert!(document.contains(">other:image</w:t>"));
    assert!(document.contains("w:tooltip=\"Image &quot;title&quot;\""));
    assert!(xml(&parts, "word/_rels/document.xml.rels")
        .contains("Target=\"https://example.invalid/image.png?x=1&amp;y=2\" TargetMode=\"External\""));
}

#[test]
fn repeated_media_share_bytes_but_relationships_and_drawing_ids_are_part_local()
{
    let mut assets = AssetRegistry::new();
    let url = register(&mut assets, PNG, "image/png");
    let doc = document(vec![paragraph(vec![
        image(&url, json!({})),
        image(&url, json!({"width": "1in"})),
        json!({"type": "footnote", "number": 1, "blocks": [paragraph(vec![image(&url, json!({}))])]}),
    ])]);
    let output = emit(&doc, &assets, &mut FakeRenderer::default());
    let parts = parts(&output);
    assert_eq!(
        parts.keys().filter(|name| name.starts_with("word/media/")).count(),
        1
    );
    for rels in
        ["word/_rels/document.xml.rels", "word/_rels/footnotes.xml.rels"]
    {
        assert_eq!(
            xml(&parts, rels).matches("Target=\"media/w1.png\"").count(),
            1
        );
    }
    let body = xml(&parts, "word/document.xml");
    let note = xml(&parts, "word/footnotes.xml");
    assert_eq!(attributes(&body, "wp:docPr", &["id"]), [["1"], ["2"]]);
    assert_eq!(attributes(&note, "wp:docPr", &["id"]), [["3"]]);
}

#[test]
fn table_images_use_the_cell_content_width() {
    let mut assets = AssetRegistry::new();
    let url = register(&mut assets, PNG, "image/png");
    let doc = document(vec![json!({
        "type": "table", "columns": [{"width": 0.5}, {"width": 0.5}],
        "head": [], "foot": [],
        "body": [{"cells": [{"inlines": [image(&url, json!({"width": "100%"}))]},
            {"inlines": []}]}], "attrs": {}
    })]);
    let output = emit(&doc, &assets, &mut FakeRenderer::default());
    let parts = parts(&output);
    let width = (4_513 - 216) * 635;
    assert_eq!(
        extents(&xml(&parts, "word/document.xml")),
        [(width, width / 2 + 1)]
    );
}

#[test]
fn missing_registered_files_preserve_the_first_error_and_source_span() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.png");
    let second = directory.path().join("second.png");
    std::fs::write(&first, PNG).unwrap();
    std::fs::write(&second, PNG).unwrap();
    let expected = std::fs::canonicalize(&first).unwrap();
    let mut assets = AssetRegistry::new();
    let first_url = format!("asset:{}", assets.register_file(&first));
    let second_url = format!("asset:{}", assets.register_file(&second));
    std::fs::remove_file(&first).unwrap();
    std::fs::remove_file(&second).unwrap();
    let doc = document(vec![paragraph(vec![
        image(&first_url, json!({})),
        image(&second_url, json!({})),
    ])]);
    let result = write(
        &doc,
        &assets,
        &mut FakeRenderer::default(),
        &DocxOptions {
            config: &Config::default(),
            reference: None,
            source_date_epoch: Some("0".into()),
            all_styles: false,
        },
    );
    let Err(Error::MissingAsset { path, span }) = result else {
        panic!("expected missing asset error")
    };
    assert_eq!(path, expected);
    let span = span.unwrap();
    assert_eq!((span.start_line, span.cell), (7, Some(3)));
    assert_eq!(span.file, Some(PathBuf::from("/included.md")));
}
