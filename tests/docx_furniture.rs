//! Generated Word header/footer parts, fields, media and reference precedence.

use std::io::{Cursor, Read, Write};
use std::path::Path;

use writer::design::{
    Align, Background, BackgroundAlign, BackgroundImage, BackgroundPages,
    Color, Fit, FurnitureImage, Pt, Rule, RulePosition, VerticalAlign,
};
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
    // The image paragraph takes the header style with nothing before it, a
    // single line and, without a gap, the letter's paragraph spacing after;
    // its mark is one point so it adds no line of its own.
    assert!(first.contains(concat!(
        "<w:pPr><w:pStyle w:val=\"Header\"/>",
        "<w:spacing w:before=\"0\" w:after=\"204\" w:line=\"240\" ",
        "w:lineRule=\"auto\"/><w:contextualSpacing w:val=\"0\"/>",
        "<w:jc w:val=\"center\"/><w:rPr><w:sz w:val=\"2\"/>",
        "<w:szCs w:val=\"2\"/></w:rPr></w:pPr>",
    )));
    assert!(first.contains("Generative Biology Institute"));
    assert!(first.contains("A96D4B"));
    assert!(rest.contains("EIT Oxford | Generative Biology Institute"));
    assert!(!rest.contains("wp:inline"));
    // The rule paragraph is a one-point footer line with no space around it.
    assert!(footer.contains(concat!(
        "<w:p><w:pPr><w:pStyle w:val=\"Footer\"/><w:pBdr>",
        "<w:top w:val=\"single\" w:sz=\"6\" w:space=\"1\" ",
        "w:color=\"A96D4B\"/></w:pBdr>",
        "<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" ",
        "w:lineRule=\"exact\"/><w:contextualSpacing w:val=\"0\"/>",
        "<w:rPr><w:sz w:val=\"2\"/><w:szCs w:val=\"2\"/></w:rPr></w:pPr>",
        "<w:r><w:rPr><w:sz w:val=\"2\"/><w:szCs w:val=\"2\"/></w:rPr><w:t/>",
        "</w:r></w:p>",
    )));
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
fn image_gap_sets_the_spacing_after_the_image_paragraph() {
    fn image(prepared: &mut pipeline::Prepared) -> &mut FurnitureImage {
        let header = prepared.design.page.header.as_mut().unwrap();
        header.first.as_mut().unwrap().image.as_mut().unwrap()
    }
    let mut prepared = letter();
    image(&mut prepared).gap = Some(Pt(3.0 * 72.0 / 25.4));
    let first = part(&write(&prepared, None), "word/header1.xml");
    assert!(first.contains(
        "<w:spacing w:before=\"0\" w:after=\"170\" w:line=\"240\" \
         w:lineRule=\"auto\"/>"
    ));
    assert!(!first.contains("w:after=\"204\""));
    // Without a resolved gap the paragraph takes the document defaults'
    // space after (the paragraph spacing), stated on the paragraph so that
    // the header style's own spacing does not replace it.
    image(&mut prepared).gap = None;
    let first = part(&write(&prepared, None), "word/header1.xml");
    assert!(first.contains(
        "<w:spacing w:before=\"0\" w:after=\"204\" w:line=\"240\" \
         w:lineRule=\"auto\"/>"
    ));
    prepared.design.text.paragraph_spacing = None;
    let first = part(&write(&prepared, None), "word/header1.xml");
    assert!(first.contains(
        "<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"240\" \
         w:lineRule=\"auto\"/>"
    ));
}

#[test]
fn rule_below_the_header_lines_is_a_tight_header_paragraph() {
    let mut prepared = letter();
    let header = prepared.design.page.header.as_mut().unwrap();
    header.rest.as_mut().unwrap().rule = Some(Rule {
        position: RulePosition::Below,
        color: Color([128, 128, 128]),
        width: Pt(0.5),
    });
    let rest = part(&write(&prepared, None), "word/header2.xml");
    let lines = rest.find("EIT Oxford | Generative Biology Institute").unwrap();
    let rule = rest.find("<w:bottom w:val=\"single\" w:sz=\"4\"").unwrap();
    assert!(lines < rule);
    assert!(rest.contains(concat!(
        "<w:p><w:pPr><w:pStyle w:val=\"Header\"/><w:pBdr>",
        "<w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"1\" ",
        "w:color=\"808080\"/></w:pBdr>",
        "<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" ",
        "w:lineRule=\"exact\"/><w:contextualSpacing w:val=\"0\"/>",
    )));
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
    // Word formats a recomputed field result like the field's runs, so every
    // run of both fields carries the line's colour and size, not just the
    // cached result.
    let field_runs = footer
        .split("<w:r>")
        .filter(|run| {
            run.contains("<w:fldChar") || run.contains("<w:instrText")
        })
        .collect::<Vec<_>>();
    assert_eq!(field_runs.len(), 8);
    for run in field_runs {
        assert!(run.starts_with("<w:rPr>"), "{run}");
        assert!(
            run.contains("<w:color w:val=\"575757\"/><w:sz w:val=\"15\"/>"),
            "{run}"
        );
    }
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

/// A PNG signature and header of the given size: the writer reads the size
/// and copies the bytes, and the readers do not decode them.
fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn background(
    path: &Path, fit: Fit, align: (Align, VerticalAlign),
    pages: BackgroundPages,
) -> Option<Background> {
    Some(Background {
        image: BackgroundImage {
            path: path.into(),
            fit,
            align: BackgroundAlign { horizontal: align.0, vertical: align.1 },
        },
        pages,
    })
}

fn anchors(xml: &str) -> Vec<String> {
    regex::Regex::new(
        r"<w:r><w:drawing><wp:anchor .*?</wp:anchor></w:drawing></w:r>",
    )
    .unwrap()
    .find_iter(xml)
    .map(|found| found.as_str().into())
    .collect()
}

fn media(bytes: &[u8]) -> usize {
    names(bytes).iter().filter(|name| name.starts_with("word/media/")).count()
}

#[test]
fn page_background_is_a_behind_text_picture_in_the_first_page_header() {
    use BackgroundPages::{All, First};
    let directory = tempfile::tempdir().unwrap();
    let wide = directory.path().join("wide.png");
    let tall = directory.path().join("tall.png");
    std::fs::write(&wide, png_header(300, 200)).unwrap();
    std::fs::write(&tall, png_header(100, 300)).unwrap();
    let centre = (Align::Center, VerticalAlign::Center);
    let plain = write(&letter(), None);

    // The cover: page one's header gains one anchored run in its first
    // paragraph and is otherwise unchanged; the continuation header is not.
    let mut prepared = letter();
    prepared.design.page.background =
        background(&wide, Fit::Cover, centre, First);
    let bytes = write(&prepared, None);
    let first = part(&bytes, "word/header1.xml");
    insta::assert_snapshot!("design_background_header_first", first);
    let found = anchors(&first);
    assert_eq!(found.len(), 1);
    assert!(first.contains(&format!("</w:pPr>{}", found[0])));
    assert_eq!(first.replace(&found[0], ""), part(&plain, "word/header1.xml"));
    for expected in [
        r#"behindDoc="1""#,
        r#"<wp:positionH relativeFrom="page"><wp:posOffset>0</wp:posOffset></wp:positionH>"#,
        r#"<wp:positionV relativeFrom="page"><wp:posOffset>0</wp:posOffset></wp:positionV>"#,
        r#"<wp:extent cx="7560310" cy="10692130"/>"#,
        "<wp:wrapNone/>",
        r#"<a:srcRect l="26430" r="26430"/>"#,
    ] {
        assert!(found[0].contains(expected), "{expected}");
    }
    assert_eq!(
        part(&bytes, "word/header2.xml"),
        part(&plain, "word/header2.xml")
    );
    assert_eq!(
        part(&bytes, "word/_rels/header1.xml.rels")
            .matches("relationships/image")
            .count(),
        2
    );
    assert!(part(&bytes, "word/document.xml").contains("<w:titlePg/>"));
    assert_eq!(media(&bytes), 2);

    // Every page: each header part carries the picture, the footer does not.
    prepared.design.page.background =
        background(&wide, Fit::Cover, centre, All);
    let bytes = write(&prepared, None);
    for name in ["word/header1.xml", "word/header2.xml"] {
        assert_eq!(anchors(&part(&bytes, name)).len(), 1, "{name}");
    }
    assert!(anchors(&part(&bytes, "word/footer1.xml")).is_empty());
    assert_eq!(media(&bytes), 2);

    // A slot whose variants agree is split so only page one has the picture.
    let slot = prepared.design.page.header.as_mut().unwrap();
    slot.first = slot.rest.clone();
    prepared.design.page.background =
        background(&wide, Fit::Cover, centre, First);
    let bytes = write(&prepared, None);
    let document = part(&bytes, "word/document.xml");
    let first_id = reference_id(&document, "header", "first").unwrap();
    let default_id = reference_id(&document, "header", "default").unwrap();
    assert_ne!(first_id, default_id);
    let (first, rest) =
        (part(&bytes, "word/header1.xml"), part(&bytes, "word/header2.xml"));
    let found = anchors(&first);
    assert_eq!(found.len(), 1);
    assert!(anchors(&rest).is_empty());
    assert_eq!(first.replace(&found[0], ""), rest);

    // Without a header slot the picture gets a part of its own: page one only
    // for `first`, every page for `all`; the built-in page number stays.
    prepared.design.page.header = None;
    prepared.design.page.footer = None;
    let bytes = write(&prepared, None);
    let document = part(&bytes, "word/document.xml");
    assert!(document.contains("<w:titlePg/>"));
    assert!(reference_id(&document, "header", "first").is_some());
    assert_eq!(reference_id(&document, "header", "default"), None);
    let header = part(&bytes, "word/header1.xml");
    assert_eq!(anchors(&header).len(), 1);
    assert!(header.contains(
        r#"<w:spacing w:before="0" w:after="0" w:line="14" w:lineRule="exact"/>"#
    ));
    let footer = reference_id(&document, "footer", "first").unwrap();
    assert_eq!(reference_id(&document, "footer", "default"), Some(footer));
    assert!(part(&bytes, "word/footer1.xml").contains(" PAGE "));
    prepared.design.page.background =
        background(&wide, Fit::Cover, centre, All);
    let bytes = write(&prepared, None);
    let document = part(&bytes, "word/document.xml");
    assert!(!document.contains("<w:titlePg/>"));
    assert!(reference_id(&document, "header", "default").is_some());
    assert_eq!(reference_id(&document, "header", "first"), None);
    assert!(part(&bytes, "word/footer1.xml").contains(" PAGE "));

    // `cover` crops the overflow on the aligned side, `contain` shrinks the
    // extent and offsets it, `stretch` fills the page of the design paper.
    for (path, fit, align, paper, expected) in [
        (
            &wide,
            Fit::Cover,
            (Align::Left, VerticalAlign::Center),
            None,
            vec![r#"<a:srcRect l="0" r="52861"/>"#],
        ),
        (
            &tall,
            Fit::Cover,
            (Align::Center, VerticalAlign::Top),
            None,
            vec![r#"<a:srcRect t="0" b="52859"/>"#],
        ),
        (
            &wide,
            Fit::Contain,
            (Align::Center, VerticalAlign::Bottom),
            None,
            vec![
                r#"<wp:extent cx="7560310" cy="5040207"/>"#,
                r#"<wp:positionV relativeFrom="page"><wp:posOffset>5651923</wp:posOffset>"#,
                r#"<wp:positionH relativeFrom="page"><wp:posOffset>0</wp:posOffset>"#,
            ],
        ),
        (
            &tall,
            Fit::Contain,
            (Align::Right, VerticalAlign::Center),
            None,
            vec![
                r#"<wp:extent cx="3564043" cy="10692130"/>"#,
                r#"<wp:positionH relativeFrom="page"><wp:posOffset>3996267</wp:posOffset>"#,
            ],
        ),
        (
            &tall,
            Fit::Stretch,
            (Align::Right, VerticalAlign::Top),
            Some("us-letter"),
            vec![r#"<wp:extent cx="7772400" cy="10058400"/>"#],
        ),
    ] {
        prepared.design.page.paper = paper.map(Into::into);
        prepared.design.page.background = background(path, fit, align, First);
        let bytes = write(&prepared, None);
        let found = anchors(&part(&bytes, "word/header1.xml"));
        assert_eq!(found.len(), 1);
        for expected in expected {
            assert!(found[0].contains(expected), "{expected} in {}", found[0]);
        }
        assert_eq!(
            found[0].contains("a:srcRect"),
            fit == Fit::Cover,
            "{fit:?}"
        );
    }
}
