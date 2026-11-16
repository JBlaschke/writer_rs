//! Floating frames for div styles in Word output (`styles.<Name>.frame`,
//! design.md §8.6): the frame properties of the style, and divs anchored to
//! the title written right after it.

use std::io::{Cursor, Read};

use writer::docx::{DocxOptions, Picture, Renderer, ValidateOptions};
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
        Err(Error::Other("no equations in this fixture".into()))
    }
    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("no images in this fixture".into()))
    }
    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("no equations in this fixture".into()))
    }
}

const TITLE_BLOCK: &str = concat!(
    "---\ntitle: A Title\nsubtitle: A subtitle\n",
    "authors:\n  - name: An Author\ndate: 1 November 2026\n---\n\n",
);

/// Builds `body` with `config` as `writer.toml`, validating the package.
fn build(config: &str, body: &str) -> Vec<u8> {
    build_with(config, body, None)
}

/// [`build`] with a reference document.
fn build_with(
    config: &str, body: &str, reference: Option<&std::path::Path>,
) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("writer.toml"), config).unwrap();
    let input = dir.path().join("doc.md");
    std::fs::write(&input, body).unwrap();
    let prepared = pipeline::prepare(&BuildOptions::new(input)).unwrap();
    let output = writer::docx::write(
        &prepared.document,
        &prepared.assets,
        &mut NoRenderer,
        &DocxOptions {
            design: &prepared.design,
            config: &prepared.config,
            reference,
            source_date_epoch: Some("0".into()),
            all_styles: false,
        },
    )
    .unwrap();
    let violations = writer::docx::validate(
        &output.bytes,
        &ValidateOptions { builtin_styles: true },
    );
    assert!(violations.is_empty(), "{violations:#?}");
    common_docx::read_back(&output.bytes);
    output.bytes
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
    xml
}

/// The `w:style` element whose `w:styleId` is `id`.
fn style<'x>(styles: &'x str, id: &str) -> &'x str {
    let start = styles
        .find(&format!("w:styleId=\"{id}\""))
        .and_then(|at| styles[..at].rfind("<w:style "))
        .unwrap_or_else(|| panic!("no style {id}"));
    let end = styles[start..].find("</w:style>").unwrap();
    &styles[start..start + end]
}

/// The paragraph styles of the body's paragraphs, in document order.
fn paragraph_styles(document: &str) -> Vec<&str> {
    document
        .split("<w:p>")
        .skip(1)
        .filter_map(|paragraph| {
            let paragraph = paragraph.split("</w:p>").next().unwrap();
            let (_, rest) = paragraph.split_once("<w:pStyle w:val=\"")?;
            rest.split('"').next()
        })
        .collect()
}

#[test]
fn a_frame_sets_the_style_frame_properties_in_schema_order() {
    let bytes = build(
        concat!(
            "[design.styles.Badge]\nalign = \"center\"\nspace_after = \"2mm\"\n",
            "frame = { width = \"16mm\", align = \"right\", gap = \"6mm\", ",
            "offset = \"-9mm\" }\n",
        ),
        ":::badge\nA badge\n:::\n\nBody text.\n",
    );
    let styles = part(&bytes, "word/styles.xml");
    let badge = style(&styles, "Badge");
    // 16 mm, 6 mm and -9 mm in twips; the frame comes before the spacing
    // and the text alignment in `w:pPr`.
    assert!(
        badge.contains(concat!(
            "<w:pPr><w:framePr w:w=\"907\" w:hSpace=\"340\" ",
            "w:wrap=\"around\" w:vAnchor=\"text\" w:hAnchor=\"margin\" ",
            "w:xAlign=\"right\" w:y=\"-510\"/><w:spacing w:after=\"113\"/>",
        )),
        "{badge}"
    );
    assert!(badge.contains("<w:jc w:val=\"center\"/>"));
    // Writer's own styles do not float.
    assert_eq!(styles.matches("<w:framePr").count(), 1);
    // Without `anchor = "title"` the div keeps its place.
    let document = part(&bytes, "word/document.xml");
    assert_eq!(paragraph_styles(&document), ["Badge", "FirstParagraph"]);
}

#[test]
fn an_empty_frame_is_a_left_aligned_frame_fitted_to_its_contents() {
    let bytes =
        build("[design.styles.Aside]\nframe = {}\n", ":::aside\nA note\n:::\n");
    let styles = part(&bytes, "word/styles.xml");
    assert!(style(&styles, "Aside").contains(concat!(
        "<w:framePr w:wrap=\"around\" w:vAnchor=\"text\" ",
        "w:hAnchor=\"margin\" w:xAlign=\"left\" w:y=\"0\"/>",
    )));
}

#[test]
fn a_div_anchored_to_the_title_follows_the_title_paragraph() {
    let config = concat!(
        "[design.styles.Badge]\n",
        "frame = { width = \"16mm\", align = \"right\", anchor = \"title\" }\n",
    );
    let body = concat!(
        "::: {#repository .badge}\n[Repository](https://example.com)\n:::\n\n",
        "Lead paragraph.\n\n",
        "::::outer\n:::badge\nNested badge\n:::\n::::\n",
    );
    let bytes = build(config, &format!("{TITLE_BLOCK}{body}"));
    let document = part(&bytes, "word/document.xml");
    // The top-level div moves above the subtitle, once, with its bookmark;
    // the nested one keeps its place.
    assert_eq!(
        paragraph_styles(&document),
        [
            "Title", "Badge", "Subtitle", "Author", "Date", "FirstParagraph",
            "Badge",
        ]
    );
    let title = document.find("A Title").unwrap();
    let bookmark = document.find("w:name=\"_repository\"").unwrap();
    assert!(title < bookmark);
    assert!(bookmark < document.find("A subtitle").unwrap());
    assert_eq!(document.matches("w:name=\"_repository\"").count(), 1);
    assert_eq!(document.matches(">Repository<").count(), 1);
    // Without a title there is nothing to sit beside: the div stays put.
    let bytes = build(config, body);
    let document = part(&bytes, "word/document.xml");
    assert_eq!(
        paragraph_styles(&document),
        ["Badge", "FirstParagraph", "Badge"]
    );
}

#[test]
fn a_div_beside_the_title_stays_in_the_full_width_title_section() {
    let config = concat!(
        "[design.page]\ncolumns = { count = 2, start = \"body\" }\n",
        "[design.styles.Badge]\n",
        "frame = { width = \"16mm\", align = \"right\", anchor = \"title\" }\n",
    );
    let body = ":::badge\nBadge\n:::\n\nFirst body paragraph.\n";
    let bytes = build(config, &format!("{TITLE_BLOCK}{body}"));
    let document = part(&bytes, "word/document.xml");
    // One break, in the date paragraph: the badge is in the one-column
    // title section and the body paragraph in the columns.
    assert_eq!(document.matches("<w:sectPr>").count(), 2);
    let first_break = document.find("<w:sectPr>").unwrap();
    assert!(document.find(">Badge<").unwrap() < first_break);
    assert!(document.find("w:val=\"Date\"").unwrap() < first_break);
    assert!(first_break < document.find("1 November 2026").unwrap());
    assert!(first_break < document.find("First body paragraph").unwrap());
}

/// The `w:id` of each footnote reference in the body, in document order.
fn footnote_references(document: &str) -> Vec<u32> {
    document
        .split("<w:footnoteReference w:id=\"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap().parse().unwrap())
        .collect()
}

#[test]
fn notes_in_a_div_beside_the_title_are_numbered_in_document_order() {
    let config = concat!(
        "[design.styles.Badge]\n",
        "frame = { width = \"16mm\", align = \"right\", anchor = \"title\" }\n",
    );
    let body = concat!(
        "---\ntitle: A Title\n---\n\n",
        "Intro with a note.[^a]\n\n",
        ":::badge\nRepository[^x]\n:::\n\n",
        "Again.[^a]\n\n",
        "[^a]: Note A.\n",
        "[^x]: Note X, see also[^y].\n",
        "[^y]: Note Y.\n",
    );
    let bytes = build(config, body);
    let document = part(&bytes, "word/document.xml");
    // The badge's notes come first in Word, so they get the first numbers.
    assert_eq!(footnote_references(&document), [1, 2, 3]);
    assert!(document.find(">Repository<") < document.find(">Intro with"));
    // The nested note's number is literal text, and the repeated reference
    // caches the number Word gives note A.
    let notes = part(&bytes, "word/footnotes.xml");
    let x = notes.split("<w:footnote w:id=\"1\">").nth(1).unwrap();
    let x = x.split("</w:footnote>").next().unwrap();
    assert!(x.contains(">Note X, see also<"), "{x}");
    assert!(x.contains("<w:t xml:space=\"preserve\">2</w:t>"), "{x}");
    assert!(notes.contains("<w:footnote w:id=\"3\">"));
    let again = document.split(">Again.<").nth(1).unwrap();
    assert!(again.contains("NOTEREF _Fn000003"), "{again}");
    assert!(again.contains("<w:t xml:space=\"preserve\">3</w:t>"), "{again}");
}

#[test]
fn a_page_break_before_a_moved_div_goes_on_the_next_paragraph() {
    let config = concat!(
        "[design.styles.Badge]\n",
        "frame = { width = \"16mm\", align = \"right\", anchor = \"title\" }\n",
    );
    let body = concat!(
        "---\ntitle: A Title\n---\n\n",
        "Para A.\n\n:::pagebreak\n:::\n\n:::badge\nBadge\n:::\n\nPara B.\n",
    );
    let bytes = build(config, body);
    let document = part(&bytes, "word/document.xml");
    // The badge is beside the title, so the break goes on Para B, as it
    // would without the badge, and no break paragraph is written.
    let b = document.find(">Para B.<").unwrap();
    let paragraph = &document[document[..b].rfind("<w:p>").unwrap()..b];
    assert!(paragraph.contains("<w:pageBreakBefore/>"), "{paragraph}");
    assert!(!document.contains("<w:br w:type=\"page\"/>"));
}

#[test]
fn a_reference_style_of_the_same_name_keeps_the_div_in_place() {
    // A reference document whose Badge style has no frame.
    let directory = tempfile::tempdir().unwrap();
    let reference = directory.path().join("reference.docx");
    let plain =
        build("[design.styles.Badge]\nsize = \"8pt\"\n", ":::badge\nB\n:::\n");
    std::fs::write(&reference, plain).unwrap();
    let config = concat!(
        "[design.styles.Badge]\n",
        "frame = { width = \"16mm\", align = \"right\", anchor = \"title\" }\n",
    );
    let body = format!("{TITLE_BLOCK}:::badge\nBadge\n:::\n\nBody text.\n");
    let bytes = build_with(config, &body, Some(&reference));
    // The reference's definition wins, frame included: Word gets no frame,
    // so the div keeps its place in the body.
    assert!(!part(&bytes, "word/styles.xml").contains("<w:framePr"));
    let document = part(&bytes, "word/document.xml");
    assert_eq!(
        paragraph_styles(&document),
        ["Title", "Subtitle", "Author", "Date", "Badge", "FirstParagraph"]
    );
}

#[test]
fn html_ignores_the_frame() {
    use serde_json::json;
    let base = std::path::Path::new("/");
    let design = |style: serde_json::Value| {
        writer::design::merge(&[writer::design::Layer {
            table: json!({"styles": {"Badge": style}})
                .as_object()
                .unwrap()
                .clone(),
            base_dir: base,
            origin: writer::design::Origin::FrontMatter,
        }])
        .unwrap()
    };
    let plain = design(json!({"size": "8pt", "align": "center"}));
    let framed = design(json!({
        "size": "8pt", "align": "center",
        "frame": {"width": "16mm", "align": "right", "anchor": "title"},
    }));
    assert_eq!(
        writer::html::design::styles_css(&framed),
        writer::html::design::styles_css(&plain)
    );
}
