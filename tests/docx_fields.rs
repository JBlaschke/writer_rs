//! Rich captions, Word sequence fields, TOC caches and bibliography contracts.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{Value, json};
use writer::assets::registry::AssetRegistry;
use writer::config::{Config, FigureNumbering, HeadingNumbering};
use writer::docx::{
    DocxOptions, DocxOutput, Picture, Renderer, ValidateOptions, validate,
    write,
};
use writer::error::{Error, Result};
use writer::ir::{Document, MathDialect};
use writer::pipeline::{self, BuildOptions, Mode};
use writer::resolve::anchor;

const PNG: &[u8] = include_bytes!("fixtures/md/images/plot.png");

struct FakeRenderer;

impl Renderer for FakeRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("equations belong to T42".into()))
    }

    fn image(
        &mut self, _: &[u8], _: &str, width: Option<f64>,
    ) -> Result<Picture> {
        let width = width.unwrap_or(72.0);
        Ok(Picture {
            png: PNG.to_vec(),
            svg: None,
            width_pt: width,
            height_pt: width / 2.0,
            baseline_pt: width / 2.0,
        })
    }

    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("MathML belongs to T42".into()))
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture(name: &str) -> Document {
    serde_json::from_slice(
        &std::fs::read(fixtures().join(format!("docx/ir/{name}.json")))
            .unwrap(),
    )
    .unwrap()
}

fn assets() -> AssetRegistry {
    let mut assets = AssetRegistry::new();
    assets.register_bytes(PNG.to_vec(), "image/png", "plot.png");
    let mut paths: Vec<_> = std::fs::read_dir(fixtures().join("docx/images"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    for path in paths {
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
        assets.register_bytes(
            std::fs::read(&path).unwrap(),
            mime.essence_str(),
            path.file_name().unwrap().to_str().unwrap(),
        );
    }
    assets
}

fn emit(document: &Document, config: &Config) -> DocxOutput {
    emit_with_assets(document, config, &assets())
}

fn emit_with_assets(
    document: &Document, config: &Config, assets: &AssetRegistry,
) -> DocxOutput {
    let output = write(
        document,
        assets,
        &mut FakeRenderer,
        &DocxOptions {
            config,
            reference: None,
            source_date_epoch: Some("0".into()),
            all_styles: false,
        },
    )
    .unwrap();
    assert_eq!(
        validate(&output.bytes, &ValidateOptions { builtin_styles: true }),
        [],
    );
    output
}

fn part(output: &DocxOutput, name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(&output.bytes)).unwrap();
    let mut text = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut text).unwrap();
    text
}

fn document(blocks: Vec<Value>) -> Document {
    serde_json::from_value(json!({
        "meta": {}, "blocks": blocks, "assets": [], "features": {},
        "source": {"path": "/fields.md", "base_dir": "/", "kind": "markdown"},
    }))
    .unwrap()
}

fn text(value: &str) -> Value {
    json!({"type": "text", "text": value})
}

fn figure(number: Option<&str>, caption: &str, body: Vec<Value>) -> Value {
    json!({
        "type": "figure", "kind": "other", "number": number,
        "caption": [text(caption)], "attrs": {}, "body": body,
    })
}

fn heading(number: Option<&str>, classes: &[&str]) -> Value {
    json!({
        "type": "heading", "level": 1, "number": number,
        "attrs": {"classes": classes}, "inlines": [text("Chapter")],
    })
}

fn instructions(xml: &str) -> Vec<String> {
    Regex::new(r"<w:instrText[^>]*>(.*?)</w:instrText>")
        .unwrap()
        .captures_iter(xml)
        .map(|capture| capture[1].trim().replace("&quot;", "\""))
        .collect()
}

fn sequences(xml: &str) -> Vec<String> {
    instructions(xml)
        .into_iter()
        .filter(|value| value.starts_with("SEQ "))
        .collect()
}

fn paragraphs(xml: &str) -> Vec<&str> {
    xml.match_indices("<w:p>")
        .map(|(start, _)| {
            let end = xml[start..].find("</w:p>").unwrap() + start + 6;
            &xml[start..end]
        })
        .collect()
}

#[test]
fn figure_layouts_caption_bookmarks_and_rich_notes_match_the_spec() {
    let output = emit(&fixture("figures"), &Config::default());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let xml = part(&output, "word/document.xml");
    let paragraphs = paragraphs(&xml);
    let image_caption =
        paragraphs.iter().find(|p| p.contains("Image caption")).unwrap();
    assert!(image_caption.contains("w:pStyle w:val=\"ImageCaption\""));
    assert!(image_caption.contains("Exhibit"));
    assert!(image_caption.contains("<w:i/>"));
    assert!(
        image_caption.find("w:bookmarkEnd").unwrap()
            < image_caption.find("Image caption").unwrap()
    );
    assert!(
        xml.find("w:pStyle w:val=\"CaptionedFigure\"").unwrap()
            < xml.find("Image caption").unwrap()
    );
    assert!(xml.find("Table caption").unwrap() < xml.find("<w:tbl>").unwrap());
    assert!(xml.contains("<w:tblCaption w:val=\"Table caption\"/>"));
    assert!(xml.contains(&format!("w:name=\"{}\"", anchor("tbl-inner"))));
    assert!(
        xml.find("Listing caption").unwrap()
            < xml.find("w:pStyle w:val=\"SourceCode\"").unwrap()
    );
    assert!(
        xml.find("Child B").unwrap()
            < xml.find("Parent group caption").unwrap()
    );
    let unnumbered = paragraphs
        .iter()
        .find(|p| p.contains("An unnumbered caption"))
        .unwrap();
    assert!(
        unnumbered.find("w:bookmarkStart").unwrap()
            < unnumbered.find("An unnumbered caption").unwrap()
    );
    assert!(
        unnumbered.find("w:bookmarkEnd").unwrap()
            > unnumbered.find("An unnumbered caption").unwrap()
    );
    assert!(xml.contains(&format!("w:anchor=\"{}\"", anchor("fig-image"))));
    assert_eq!(xml.matches("<w:footnoteReference ").count(), 1);
    let notes = part(&output, "word/footnotes.xml");
    assert!(notes.contains("A note in the image caption."));
    assert!(xml.contains(&format!("w:name=\"{}\"", anchor("fig-bare"))));
    assert_eq!(
        sequences(&xml)
            .iter()
            .filter(|s| s.starts_with("SEQ Listing "))
            .count(),
        2
    );
}

#[test]
fn uncaptioned_image_uses_figure_style_and_a_block_bookmark() {
    let doc = document(vec![json!({
        "type": "figure", "kind": "image", "attrs": {"id": "bare"},
        "body": [{"type": "paragraph", "inlines": [{
            "type": "image", "url": "asset:917b8e6615f3",
            "alt": [text("Bare image")], "attrs": {},
        }]}],
    })]);
    let output = emit(&doc, &Config::default());
    let xml = part(&output, "word/document.xml");
    assert!(xml.contains("w:pStyle w:val=\"Figure\""));
    assert!(!xml.contains("w:pStyle w:val=\"ImageCaption\""));
    assert!(
        xml.find("w:bookmarkStart").unwrap() < xml.find("w:drawing").unwrap()
    );
    assert!(
        xml.find("w:bookmarkEnd").unwrap() > xml.find("w:drawing").unwrap()
    );
}

#[test]
fn nested_figures_reset_fields_to_the_cached_document_numbers() {
    let doc = document(vec![
        figure(
            Some("1"),
            "Parent",
            vec![
                figure(Some("2"), "Child one", vec![]),
                figure(Some("3"), "Child two", vec![]),
            ],
        ),
        figure(Some("4"), "Following", vec![]),
    ]);
    let output = emit(&doc, &Config::default());
    assert_eq!(
        sequences(&part(&output, "word/document.xml")),
        [
            "SEQ Figure \\* ARABIC \\r 2", "SEQ Figure \\* ARABIC",
            "SEQ Figure \\* ARABIC \\r 1", "SEQ Figure \\* ARABIC \\r 4",
        ]
    );
}

#[test]
fn section_fields_handle_unnumbered_and_unlisted_headings() {
    let doc = document(vec![
        heading(Some("1"), &[]),
        figure(Some("1.1"), "First", vec![]),
        figure(Some("1.2"), "Second", vec![]),
        heading(None, &["unnumbered"]),
        figure(Some("1.3"), "Interlude", vec![]),
        heading(Some("2"), &["notoc"]),
        figure(Some("2.1"), "Unlisted chapter", vec![]),
        figure(Some("2.2"), "Last", vec![]),
    ]);
    let mut config = Config::default();
    config.numbering.figures = FigureNumbering::Section;
    config.numbering.headings = HeadingNumbering::Pattern("1.1".into());
    let output = emit(&doc, &config);
    let xml = part(&output, "word/document.xml");
    assert_eq!(
        sequences(&xml),
        [
            "SEQ Figure \\* ARABIC \\s 1",
            "SEQ Figure \\* ARABIC \\s 1",
            "SEQ Figure \\* ARABIC \\s 1 \\r 3",
            "SEQ Figure \\* ARABIC \\s 1 \\r 1",
            "SEQ Figure \\* ARABIC \\s 1",
        ]
    );
    assert!(
        paragraphs(&xml)
            .iter()
            .find(|p| p.contains("Interlude"))
            .unwrap()
            .contains(">1</w:t>")
    );
}

#[test]
fn section_policy_without_heading_numbers_uses_document_fields() {
    let doc = document(vec![
        heading(None, &[]),
        figure(Some("1"), "First", vec![]),
        heading(None, &[]),
        figure(Some("2"), "Second", vec![]),
    ]);
    let mut config = Config::default();
    config.numbering.figures = FigureNumbering::Section;
    let output = emit(&doc, &config);
    assert_eq!(
        sequences(&part(&output, "word/document.xml")),
        ["SEQ Figure \\* ARABIC", "SEQ Figure \\* ARABIC"]
    );
}

#[test]
fn numbering_fixture_interlude_has_the_required_explicit_reset() {
    let mut opts = BuildOptions::new(fixtures().join("md/numbering.md"));
    opts.mode = Mode::Check;
    let prepared = pipeline::prepare(&opts).unwrap();
    let output = emit_with_assets(
        &prepared.document, &prepared.config, &prepared.assets,
    );
    let xml = part(&output, "word/document.xml");
    assert!(
        sequences(&xml).contains(&"SEQ Figure \\* ARABIC \\s 1 \\r 3".into())
    );
}

#[test]
fn toc_caches_depth_filtered_entries_and_only_known_bookmarks() {
    let output = emit(&fixture("toc"), &Config::default());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let xml = part(&output, "word/document.xml");
    assert_eq!(xml.matches("<w:docPartUnique/>").count(), 1);
    assert_eq!(xml.matches("<w:sdt>").count(), 2);
    assert!(instructions(&xml).contains(&"TOC \\o \"1-2\" \\h \\z \\u".into()));
    assert!(instructions(&xml).contains(&"TOC \\o \"1-1\" \\h \\z \\u".into()));
    assert!(
        !instructions(&xml).iter().any(|i| i.contains(&anchor("hidden-toc")))
    );
    assert!(xml.contains("w:pStyle w:val=\"TOC2\""));
    assert!(!xml.contains("w:pStyle w:val=\"TOC3\""));
    assert_eq!(xml.matches(">Overview</w:t>").count(), 2);
    assert_eq!(xml.matches(">Sources</w:t>").count(), 3);
    assert!(!xml.contains("Stale bibliography title"));
    assert!(xml.contains("w:leader=\"dot\""));
    assert!(xml.contains("PAGEREF _refs \\h"));
    assert!(xml.contains(&format!("PAGEREF {} \\h", anchor("sec-method"))));
}

#[test]
fn empty_tocs_still_have_balanced_fields_and_one_unique_marker() {
    let doc = document(vec![
        json!({"type": "toc", "depth": 3}),
        json!({"type": "toc", "depth": 1}),
    ]);
    let output = emit(&doc, &Config::default());
    let xml = part(&output, "word/document.xml");
    assert_eq!(
        instructions(&xml).iter().filter(|i| i.starts_with("TOC ")).count(),
        2
    );
    assert_eq!(xml.matches("<w:docPartUnique/>").count(), 1);
    assert_eq!(xml.matches(">Contents</w:t>").count(), 2);
    assert!(!xml.contains("PAGEREF"));
}

#[test]
fn bibliography_has_named_target_prefix_tabs_rich_runs_and_notes() {
    let doc = fixture("bibliography");
    let output = emit(&doc, &Config::default());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let xml = part(&output, "word/document.xml");
    assert!(xml.contains("w:name=\"_refs\""));
    assert_eq!(xml.matches(">Selected sources</w:t>").count(), 2);
    assert!(!xml.contains("Stale title"));
    assert!(xml.contains("PAGEREF _refs \\h"));
    let paragraphs = paragraphs(&xml);
    let first = paragraphs.iter().find(|p| p.contains("First title.")).unwrap();
    let second =
        paragraphs.iter().find(|p| p.contains("Second title.")).unwrap();
    assert!(first.contains("<w:b/>"));
    assert!(first.contains("w:pStyle w:val=\"Bibliography\""));
    assert!(first.contains("w:tab w:val=\"left\" w:pos=\"720\""));
    assert!(first.contains("w:ind w:left=\"720\" w:hanging=\"720\""));
    assert!(second.contains("<w:i/>"));
    assert!(second.contains("w:ind w:left=\"720\" w:hanging=\"720\""));
    assert!(!second.contains("<w:tabs>"));
    assert_eq!(xml.matches("<w:footnoteReference ").count(), 1);
    assert!(
        part(&output, "word/footnotes.xml")
            .contains("A note in a bibliography entry.")
    );
    assert!(
        part(&output, "word/_rels/document.xml.rels")
            .contains("https://example.org/first")
    );
}

#[test]
fn bibliography_hanging_indent_is_optional_and_none_emits_nothing() {
    let mut doc = fixture("bibliography");
    doc.resolved.bibliography.as_mut().unwrap().hanging_indent = false;
    let output = emit(&doc, &Config::default());
    let xml = part(&output, "word/document.xml");
    let paragraphs = paragraphs(&xml);
    assert!(
        !paragraphs
            .iter()
            .find(|p| p.contains("Second title."))
            .unwrap()
            .contains("w:hanging")
    );
    assert!(
        paragraphs
            .iter()
            .find(|p| p.contains("First title."))
            .unwrap()
            .contains("w:hanging")
    );
    let doc = document(vec![json!({"type": "bibliography"})]);
    let output = emit(&doc, &Config::default());
    let xml = part(&output, "word/document.xml");
    assert!(!xml.contains("Bibliography"));
    assert!(!xml.contains("w:name=\"_refs\""));
}
