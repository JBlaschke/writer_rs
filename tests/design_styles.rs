//! Named paragraph styles share the same selection across output formats.

use std::io::{Cursor, Read};
use std::path::Path;

use writer::config::Target;
use writer::design::{Design, Layer, Origin, merge};
use writer::pipeline::{self, BuildOptions, WorldSlot};

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut text).unwrap();
    text
}

fn design(value: serde_json::Value, root: &Path) -> Design {
    merge(&[Layer {
        table: value.as_object().unwrap().clone(),
        base_dir: root,
        origin: Origin::FrontMatter,
    }])
    .unwrap()
}

#[test]
fn class_and_attribute_routes_share_html_typst_and_word_styles() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("document.md");
    std::fs::write(&input, concat!(
        ":::recipient\nFirst recipient\n:::\n\n",
        ":::x {custom-style=\"recipient\"}\nSecond recipient\n:::\n\n",
        ":::inside-address\nInside address\n:::\n\n",
        "::::closing\n:::signoff {custom-style=\"Closing\"}\nYours sincerely\n:::\n",
        ":::signature\nA Writer\n:::\n::::\n\n",
        ":::date\n1 November 2026\n:::\n\n:::salutation\nDear colleague\n:::\n",
    )).unwrap();
    std::fs::write(
        root.path().join("writer.toml"),
        "[pdf]\nsystem_fonts = false\n",
    )
    .unwrap();
    let mut opts = BuildOptions::new(&input);
    opts.cli.theme = Some(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/design/letter/theme")
            .to_string_lossy()
            .into(),
    );
    let report = pipeline::build_targets(
        &opts,
        &[Target::Html, Target::Docx, Target::Typst, Target::Pdf],
        &mut WorldSlot::new(),
    );
    assert!(report.prepared.is_ok());
    for result in &report.targets {
        assert!(result.written.is_ok(), "{:?}", result.written);
    }
    let html = std::fs::read_to_string(root.path().join("dist/document.html"))
        .unwrap();
    assert!(
        html.contains("<div class=\"recipient\" data-style=\"Recipient\">")
    );
    assert!(html.contains("<div class=\"x\" data-style=\"Recipient\">"));
    let prepared = report.prepared.unwrap();
    insta::assert_snapshot!(
        "named_styles_css",
        writer::html::design::styles_css(&prepared.design)
    );
    let source =
        std::fs::read_to_string(root.path().join("dist/document.typ")).unwrap();
    assert!(source.contains("writer-div(style: \"Recipient\""));
    assert!(source.contains("writer-div(style: \"Closing\""));
    let bytes = std::fs::read(root.path().join("dist/document.docx")).unwrap();
    assert!(writer::docx::validate(&bytes, &Default::default()).is_empty());
    let styles = part(&bytes, "word/styles.xml");
    let document = part(&bytes, "word/document.xml");
    insta::assert_snapshot!("named_styles_word", styles);
    assert_eq!(document.matches("w:pStyle w:val=\"Recipient\"").count(), 2);
    assert!(styles.contains("w:after=\"340\""));
    assert!(styles.contains("w:line=\"242\""));
    assert!(styles.contains("w:contextualSpacing"));
    assert_eq!(styles.matches("w:styleId=\"Date\"").count(), 1);
    let date = styles
        .split_once("<w:name w:val=\"Date\"/>")
        .unwrap()
        .1
        .split_once("</w:style>")
        .unwrap()
        .0;
    assert!(date.contains("<w:sz w:val=\"20\"/>"));
    // The built-in Date definition stays underneath the design's two keys.
    assert!(date.contains("<w:basedOn w:val=\"Title\"/>"));
    assert!(date.contains("<w:spacing w:before=\"0\" w:after=\"170\"/>"));
    assert!(date.contains("w:ascii=\"Source Sans 3\""));
    assert!(date.contains("<w:color w:val=\"59636E\"/>"));
    let findings = writer::check::design::unused_styles(
        &prepared.design, &prepared.document,
    );
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn unused_styles_warn_once_and_unknown_attributes_can_fall_back_to_class() {
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let value = design(
        json!({"styles":{"Postscript":{}, "Recipient":{}}}),
        root.path(),
    );
    let mut attrs = writer::ir::Attrs::default();
    attrs.classes.push("recipient".into());
    attrs.pairs.insert("custom-style".into(), "Other".into());
    assert_eq!(writer::design::style_for(&value, &attrs), Some("Recipient"));
    let mut document: writer::ir::Document = serde_json::from_value(json!({
        "meta": {}, "blocks": [], "features": {}, "assets": [],
        "source": {"path": "document.md", "kind": "markdown", "base_dir": "."},
    }))
    .unwrap();
    document.blocks.push(writer::ir::Block::Div {
        attrs,
        blocks: Vec::new(),
        span: None,
    });
    let findings = writer::check::design::unused_styles(&value, &document);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "design-style-unused");
    assert!(findings[0].message.contains("Postscript"));
    // Writer's own paragraphs carry Word vocabulary names and the header
    // furniture's Header: never unused.
    let value = design(
        json!({"styles":{"Date":{}, "Heading-1":{}, "Header":{}}}),
        root.path(),
    );
    let findings = writer::check::design::unused_styles(&value, &document);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn style_selectors_and_families_are_escaped_and_explicit_false_is_preserved() {
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let value = design(
        json!({"styles":{"A\"B\\C":{
            "font":"Family\"Name", "italic":false, "weight":400, "align":"justify",
        }}}),
        root.path(),
    );
    let css = writer::html::design::styles_css(&value);
    assert!(css.contains("[data-style=\"A\\\"B\\\\C\"]"));
    assert!(css.contains("font-family:\"Family\\\"Name\""));
    assert!(css.contains("font-style:normal"));
    assert!(css.contains("text-align:justify"));
    assert!(css.contains(" p{text-align:inherit}"));
    let styles = writer::docx::StyleTable::generated(&value).to_xml();
    assert!(styles.contains("<w:b w:val=\"0\"/>"));
    assert!(styles.contains("<w:i w:val=\"0\"/>"));
}

#[test]
fn normalized_reference_vocabulary_styles_keep_their_definition() {
    use serde_json::json;
    use writer::docx::{StyleKind, StyleTable};
    let root = tempfile::tempdir().unwrap();
    let value =
        design(json!({"styles":{"Heading-1":{"size":"12pt"}}}), root.path());
    let reference = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:styleId="LocalizedHeading"><w:name w:val="heading 1"/><w:rPr><w:sz w:val="62"/></w:rPr></w:style></w:styles>"#;
    let table =
        StyleTable::from_reference_with_design(reference, &value).unwrap();
    assert_eq!(
        table.id_for("Heading-1", StyleKind::Paragraph),
        Some("LocalizedHeading")
    );
    let xml = table.to_xml();
    assert!(xml.contains("w:val=\"62\""));
    assert!(!xml.contains("w:val=\"24\""));
}
