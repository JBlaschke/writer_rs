//! Generated Word header/footer parts, fields, media and reference precedence.

use std::io::{Cursor, Read, Write};
use std::path::Path;

use writer::docx::xml::{
    CT_DOCUMENT, CT_RELS, CT_SETTINGS, CT_STYLES, NS_R, NS_RELS, NS_W,
    REL_OFFICE_DOCUMENT, REL_SETTINGS, REL_STYLES,
};
use writer::docx::{DocxOptions, Picture, Renderer};
use writer::error::{Error, Result};
use writer::ir::MathDialect;
use writer::pipeline::{self, BuildOptions};

#[path = "common/docx.rs"]
mod common_docx;

struct NoRenderer;
impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("no equations in fixture".into()))
    }
    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("PNG furniture needs no renderer".into()))
    }
    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("no equations in fixture".into()))
    }
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
    xml
}

fn names(bytes: &[u8]) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            archive.by_index(index).unwrap().name().unwrap().into_owned()
        })
        .collect()
}

fn write(prepared: &pipeline::Prepared, reference: Option<&Path>) -> Vec<u8> {
    let options = DocxOptions {
        config: &prepared.config,
        design: &prepared.design,
        reference,
        source_date_epoch: None,
        all_styles: false,
    };
    let output = writer::docx::write(
        &prepared.document, &prepared.assets, &mut NoRenderer, &options,
    )
    .unwrap();
    let violations = writer::docx::validate(&output.bytes, &Default::default());
    assert!(violations.is_empty(), "{violations:#?}");
    common_docx::read_back(&output.bytes);
    output.bytes
}

fn letter() -> pipeline::Prepared {
    let input = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/design/letter/letter.md");
    pipeline::prepare(&BuildOptions::new(input)).unwrap()
}

fn reference_without_section(path: &Path, even_headers: &str) {
    let overrides = [
        ("word/document.xml", CT_DOCUMENT),
        ("word/styles.xml", CT_STYLES),
        ("word/settings.xml", CT_SETTINGS),
    ]
    .into_iter()
    .map(|(name, content_type)| {
        format!(
            r#"<Override PartName="/{name}" ContentType="{content_type}"/>"#
        )
    })
    .collect::<String>();
    let parts = [
        (
            "[Content_Types].xml",
            format!(
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="{CT_RELS}"/><Default Extension="xml" ContentType="application/xml"/>{overrides}</Types>"#
            ),
        ),
        (
            "_rels/.rels",
            format!(
                r#"<Relationships xmlns="{NS_RELS}"><Relationship Id="rId1" Type="{REL_OFFICE_DOCUMENT}" Target="word/document.xml"/></Relationships>"#
            ),
        ),
        (
            "word/document.xml",
            format!(
                r#"<w:document xmlns:w="{NS_W}" xmlns:r="{NS_R}"><w:body><w:p/></w:body></w:document>"#
            ),
        ),
        (
            "word/_rels/document.xml.rels",
            format!(
                r#"<Relationships xmlns="{NS_RELS}"><Relationship Id="rId1" Type="{REL_STYLES}" Target="styles.xml"/><Relationship Id="rId2" Type="{REL_SETTINGS}" Target="settings.xml"/></Relationships>"#
            ),
        ),
        (
            "word/styles.xml",
            format!(
                r#"<w:styles xmlns:w="{NS_W}"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="ReferenceNormal"><w:name w:val="Normal"/><w:rPr><w:sz w:val="32"/></w:rPr></w:style></w:styles>"#
            ),
        ),
        (
            "word/settings.xml",
            format!(
                r#"<w:settings xmlns:w="{NS_W}">{even_headers}</w:settings>"#
            ),
        ),
    ];
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, xml) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    std::fs::write(path, zip.finish().unwrap().into_inner()).unwrap();
}

fn reference_id(document: &str, kind: &str, page: &str) -> Option<String> {
    let start = format!(r#"<w:{kind}Reference w:type="{page}" r:id=""#);
    let (_, rest) = document.split_once(&start)?;
    Some(rest.split_once('"')?.0.into())
}

#[test]
fn first_header_shared_footer_and_media_are_valid_parts() {
    let bytes = write(&letter(), None);
    let first = part(&bytes, "word/header1.xml");
    let rest = part(&bytes, "word/header2.xml");
    let footer = part(&bytes, "word/footer1.xml");
    let document = part(&bytes, "word/document.xml");
    for (name, xml) in [
        ("design_furniture_header_first", &first),
        ("design_furniture_header_rest", &rest),
        ("design_furniture_footer", &footer),
        (
            "design_furniture_document_rels",
            &part(&bytes, "word/_rels/document.xml.rels"),
        ),
        (
            "design_furniture_header_rels",
            &part(&bytes, "word/_rels/header1.xml.rels"),
        ),
    ] {
        insta::assert_snapshot!(name, xml);
    }
    assert!(first.contains("cx=\"1800000\""));
    assert!(first.contains("Generative Biology Institute"));
    assert!(first.contains("A96D4B"));
    assert!(rest.contains("EIT Oxford | Generative Biology Institute"));
    assert!(!rest.contains("wp:inline"));
    assert!(footer.contains("<w:top w:val=\"single\" w:sz=\"6\""));
    assert!(footer.contains("w:pos=\"9412\""));
    assert!(!footer.contains(" PAGE "));
    assert!(document.contains("<w:titlePg/>"));
    assert!(document.contains("<w:headerReference w:type=\"first\""));
    assert!(document.contains("<w:footerReference w:type=\"first\""));
    assert!(document.contains("<w:footerReference w:type=\"default\""));
    assert_eq!(
        names(&bytes)
            .iter()
            .filter(|name| name.starts_with("word/media/"))
            .count(),
        1
    );
}

#[test]
fn placeholders_are_fields_and_empty_footer_removes_builtin_numbering() {
    let mut prepared = letter();
    let mut footer =
        prepared.design.page.footer.as_ref().unwrap().first.clone().unwrap();
    footer.lines[0].text = "Page {page} of {pages}".into();
    let slot = prepared.design.page.footer.as_mut().unwrap();
    slot.first = Some(footer.clone());
    slot.rest = Some(footer);
    let bytes = write(&prepared, None);
    let footer = part(&bytes, "word/footer1.xml");
    assert!(footer.contains(" PAGE "));
    assert!(footer.contains(" NUMPAGES "));
    prepared.design.page.footer = Some(Default::default());
    let bytes = write(&prepared, None);
    let footer = part(&bytes, "word/footer1.xml");
    assert!(footer.contains("<w:p/>"));
    assert!(!footer.contains(" PAGE "));
}

#[test]
fn reference_body_section_keeps_its_own_furniture_verbatim() {
    let reference = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/docx/reference.docx");
    let bytes = write(&letter(), Some(&reference));
    let original = std::fs::read(&reference).unwrap();
    let original_names = names(&original);
    for name in names(&bytes).into_iter().filter(|name| {
        name.starts_with("word/header") || name.starts_with("word/footer")
    }) {
        assert!(
            original_names.contains(&name),
            "unexpected generated part {name}"
        );
        assert_eq!(part(&bytes, &name), part(&original, &name));
    }
}

#[test]
fn reference_normal_size_supplies_unset_furniture_sizes_without_section() {
    let directory = tempfile::tempdir().unwrap();
    let reference = directory.path().join("reference.docx");
    reference_without_section(&reference, "");
    let mut prepared = letter();
    prepared.design.text.size = None;
    for slot in
        [&mut prepared.design.page.header, &mut prepared.design.page.footer]
            .into_iter()
            .flatten()
    {
        for furniture in [&mut slot.first, &mut slot.rest].into_iter().flatten()
        {
            furniture.size = None;
        }
    }
    let bytes = write(&prepared, Some(&reference));
    let styles = part(&bytes, "word/styles.xml");
    assert!(styles.contains(r#"<w:sz w:val="22"/>"#));
    assert!(styles.contains(r#"<w:sz w:val="32"/>"#));
    for name in ["word/header1.xml", "word/header2.xml"] {
        assert!(part(&bytes, name).contains(r#"<w:sz w:val="32"/>"#));
    }
    let footer = part(&bytes, "word/footer1.xml");
    assert!(footer.contains(r#"<w:sz w:val="24"/>"#));
    assert!(!footer.contains(r#"<w:sz w:val="17"/>"#));
}

#[test]
fn retained_even_header_setting_aliases_rest_furniture_only_when_enabled() {
    let directory = tempfile::tempdir().unwrap();
    let reference = directory.path().join("reference.docx");
    let mut prepared = letter();
    for (setting, enabled) in [
        ("", false),
        (r#"<w:evenAndOddHeaders w:val="false"/>"#, false),
        (r#"<w:evenAndOddHeaders w:val="0"/>"#, false),
        (r#"<w:evenAndOddHeaders w:val="off"/>"#, false),
        ("<w:evenAndOddHeaders/>", true),
        (r#"<w:evenAndOddHeaders w:val="true"/>"#, true),
        (r#"<w:evenAndOddHeaders w:val="1"/>"#, true),
        (r#"<w:evenAndOddHeaders w:val="on"/>"#, true),
    ] {
        reference_without_section(&reference, setting);
        let bytes = write(&prepared, Some(&reference));
        let document = part(&bytes, "word/document.xml");
        if !setting.is_empty() {
            assert!(part(&bytes, "word/settings.xml").contains(setting));
        }
        for kind in ["header", "footer"] {
            let default = reference_id(&document, kind, "default").unwrap();
            let even = reference_id(&document, kind, "even");
            assert_eq!(even, enabled.then_some(default));
        }
        assert_ne!(
            reference_id(&document, "header", "first"),
            reference_id(&document, "header", "default"),
        );
        assert_eq!(
            names(&bytes)
                .iter()
                .filter(|name| name.starts_with("word/media/"))
                .count(),
            1,
        );
    }
    prepared.design.page.footer = None;
    let bytes = write(&prepared, Some(&reference));
    let document = part(&bytes, "word/document.xml");
    let default = reference_id(&document, "footer", "default").unwrap();
    assert_eq!(
        reference_id(&document, "footer", "even"),
        Some(default.clone()),
    );
    assert_eq!(reference_id(&document, "footer", "first"), Some(default));
    assert!(part(&bytes, "word/footer1.xml").contains(" PAGE "));
}
