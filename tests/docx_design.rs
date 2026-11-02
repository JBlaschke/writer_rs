//! Word design tokens, section geometry, and reference precedence.

use std::io::{Cursor, Read};
use std::path::Path;

use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::design::Design;
use writer::docx::{DocxOptions, Picture, Renderer, StyleKind, StyleTable};
use writer::error::{Error, Result};
use writer::ir::{Document, MathDialect};
use writer::pipeline::{self, BuildOptions};

#[path = "common/docx.rs"]
mod common_docx;

struct NoRenderer;

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("no equations in this fixture".into()))
    }
    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("no body images in this fixture".into()))
    }
    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("no equations in this fixture".into()))
    }
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
    xml
}

fn write(
    doc: &Document, design: &Design, config: &Config, reference: Option<&Path>,
) -> Vec<u8> {
    writer::docx::write(
        doc,
        &AssetRegistry::new(),
        &mut NoRenderer,
        &DocxOptions {
            design,
            config,
            reference,
            source_date_epoch: None,
            all_styles: false,
        },
    )
    .unwrap()
    .bytes
}

#[test]
fn letter_tokens_and_section_are_generated_and_readable() {
    let input = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/design/letter/letter.md");
    let prepared = pipeline::prepare(&BuildOptions::new(input)).unwrap();
    let bytes =
        write(&prepared.document, &prepared.design, &prepared.config, None);
    let styles = part(&bytes, "word/styles.xml");
    let document = part(&bytes, "word/document.xml");
    insta::assert_snapshot!("design_letter_styles", styles);
    insta::assert_snapshot!(
        "design_letter_section",
        document
            .rsplit_once("<w:sectPr>")
            .unwrap()
            .1
            .split_once("</w:sectPr>")
            .unwrap()
            .0
    );
    assert!(styles.contains("Source Sans 3"));
    assert!(styles.contains("w:line=\"324\""));
    assert!(styles.contains("w:after=\"204\""));
    assert!(document.contains("w:top=\"2835\""));
    assert!(document.contains("w:header=\"964\""));
    assert!(document.contains("w:footer=\"567\""));
    assert!(writer::docx::validate(&bytes, &Default::default()).is_empty());
    assert!(common_docx::read_back(&bytes).paragraphs > 0);
}

#[test]
fn reference_defaults_and_section_win_but_missing_vocabulary_gets_tokens() {
    let input = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/design/letter/letter.md");
    let prepared = pipeline::prepare(&BuildOptions::new(input)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let reference = directory.path().join("reference.docx");
    std::fs::write(
        &reference,
        writer::docx::custom_style::reference_docx().unwrap(),
    )
    .unwrap();
    let bytes = write(
        &prepared.document,
        &prepared.design,
        &prepared.config,
        Some(&reference),
    );
    let styles = part(&bytes, "word/styles.xml");
    let reference_bytes = std::fs::read(&reference).unwrap();
    let reference_styles = part(&reference_bytes, "word/styles.xml");
    let defaults = |xml: &str| {
        xml.split_once("<w:docDefaults>")
            .unwrap()
            .1
            .split_once("</w:docDefaults>")
            .unwrap()
            .0
            .to_owned()
    };
    let language = regex::Regex::new(r"<w:lang[^>]*/>").unwrap();
    assert_eq!(
        language.replace_all(&defaults(&styles), ""),
        language.replace_all(&defaults(&reference_styles), "")
    );
    let section = |xml: &str| {
        xml.rsplit_once("<w:sectPr")
            .unwrap()
            .1
            .split_once("</w:sectPr>")
            .unwrap()
            .0
            .to_owned()
    };
    assert_eq!(
        section(&part(&bytes, "word/document.xml")),
        section(&part(&reference_bytes, "word/document.xml"))
    );
    let notes =
        writer::check::design::docx_shadowed(&prepared.design, &reference);
    assert!(notes.iter().any(|note| note.code == "design-shadowed"
        && note.message.contains("page setup")));

    let minimal = "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:docDefaults/></w:styles>";
    let mut table =
        StyleTable::from_reference_with_design(minimal, &prepared.design)
            .unwrap();
    table.ensure("heading 1", StyleKind::Paragraph);
    assert!(table.to_xml().contains("Source Sans 3"));
    assert!(table.to_xml().contains("<w:docDefaults/>"));
}

#[test]
fn heading_font_covers_every_level_linked_character_title_and_subtitle() {
    use writer::design::{Font, Fonts};
    let design = Design {
        declared: true,
        fonts: Fonts {
            heading: Some(Font {
                family: "Heading Face".into(),
                files: Vec::new(),
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut table = StyleTable::generated(&design);
    let mut ids = Vec::new();
    for level in 1..=6 {
        ids.push(
            table.ensure(&format!("heading {level}"), StyleKind::Paragraph),
        );
        ids.push(
            table
                .ensure(&format!("Heading {level} Char"), StyleKind::Character),
        );
    }
    for (name, kind) in [
        ("Title", StyleKind::Paragraph),
        ("Subtitle", StyleKind::Paragraph),
        ("Title Char", StyleKind::Character),
        ("Subtitle Char", StyleKind::Character),
    ] {
        ids.push(table.ensure(name, kind));
    }
    let generated = table.to_xml();
    for id in ids {
        let start = generated
            .find(&format!("w:styleId=\"{id}\""))
            .unwrap_or_else(|| panic!("missing generated style {id}"));
        let definition = generated[start..].split_once("</w:style>").unwrap().0;
        for attribute in ["w:ascii", "w:hAnsi", "w:cs"] {
            assert!(
                definition.contains(&format!("{attribute}=\"Heading Face\"")),
                "{id} does not use heading font for {attribute}: {definition}"
            );
        }
    }
    // Selecting the heading role alone leaves the independent body role intact.
    let defaults = generated
        .split_once("<w:docDefaults>")
        .unwrap()
        .1
        .split_once("</w:docDefaults>")
        .unwrap()
        .0;
    assert!(defaults.contains("w:ascii=\"Calibri\""));

    // A reference's complete vocabulary keeps its own fonts verbatim.
    let reference = writer::vendor::get_str("docx/styles.xml").unwrap();
    let original = StyleTable::from_reference(&reference).unwrap().to_xml();
    let preserved = StyleTable::from_reference_with_design(&reference, &design)
        .unwrap()
        .to_xml();
    assert_eq!(preserved, original);
}
