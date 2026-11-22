//! A div of a design style is one block in Word (design.md §8.5): the
//! style's space keys are its outer margins, and its paragraphs keep their
//! own space between themselves.

use std::io::{Cursor, Read};

use writer::docx::{DocxOptions, Picture, Renderer, ValidateOptions};
use writer::error::{Error, Result};
use writer::ir::MathDialect;
use writer::pipeline::{self, BuildOptions};

const PNG: &[u8] = include_bytes!("fixtures/docx/images/dpi144.png");

const DESIGN: &str = concat!(
    "[design.text]\nparagraph_spacing = '10pt'\n",
    "[design.styles.Closing]\nspace_before = '17pt'\nspace_after = '23pt'\n",
    "[design.styles.Recipient]\nspace_after = '17pt'\n",
    "[design.styles.Plain]\nkeep_together = true\n",
);

struct NoRenderer;

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("no equations in this fixture".into()))
    }
    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("PNG images need no renderer".into()))
    }
    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("no equations in this fixture".into()))
    }
}

/// `word/document.xml` and `word/styles.xml` of `body` under `DESIGN` and
/// `extra` configuration.
fn build(extra: &str, body: &str) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("writer.toml"), format!("{DESIGN}{extra}"))
        .unwrap();
    std::fs::write(dir.path().join("pic.png"), PNG).unwrap();
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
            reference: None,
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
    let mut archive = zip::ZipArchive::new(Cursor::new(&output.bytes)).unwrap();
    let mut part = |name: &str| {
        let mut xml = String::new();
        archive.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
        xml
    };
    (part("word/document.xml"), part("word/styles.xml"))
}

/// The properties of the paragraph whose text starts with `text`, without
/// the enclosing `w:pPr` tags.
fn props<'a>(document: &'a str, text: &str) -> &'a str {
    let marker = format!("xml:space=\"preserve\">{text}");
    let at = document
        .find(&marker)
        .unwrap_or_else(|| panic!("no paragraph starts with {text:?}"));
    let paragraph = &document[document[..at].rfind("<w:p>").unwrap()..at];
    paragraph
        .split_once("<w:pPr>")
        .and_then(|(_, rest)| rest.split_once("</w:pPr>"))
        .map_or("", |(props, _)| props)
}

#[test]
fn paragraphs_of_one_div_keep_body_spacing_between_themselves() {
    let (document, styles) = build(
        "",
        concat!(
            "Before\n\n",
            ":::closing\nFirst\n\nMiddle\n\nLast\n:::\n\n",
            ":::closing\nAlone\n:::\n\n",
            ":::recipient\nName\n\nStreet\n:::\n\n",
            ":::plain\nOne\n\nTwo\n:::\n\nAfter\n",
        ),
    );
    // Body Text has no space before and 10 pt (200 twips) after under
    // `text.paragraph_spacing`. The div's own 17 pt and 23 pt stay on its
    // first and last paragraph, from the style.
    assert_eq!(
        props(&document, "First"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Middle"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\" w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Last"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\"/>"
    );
    // One paragraph is the whole block: the style says it all.
    assert_eq!(props(&document, "Alone"), "<w:pStyle w:val=\"Closing\"/>");
    // A style with one space key is a block too.
    assert_eq!(
        props(&document, "Name"),
        "<w:pStyle w:val=\"Recipient\"/><w:spacing w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Street"),
        "<w:pStyle w:val=\"Recipient\"/><w:spacing w:before=\"0\"/>"
    );
    // A style without space keys inherits Body Text's spacing, as before.
    assert_eq!(props(&document, "One"), "<w:pStyle w:val=\"Plain\"/>");
    assert_eq!(props(&document, "Two"), "<w:pStyle w:val=\"Plain\"/>");
    // Two divs of one style in a row are two blocks, so nothing may drop
    // the space between paragraphs of the same style.
    assert!(!styles.contains("w:contextualSpacing"));
    assert!(styles.contains(concat!(
        "<w:name w:val=\"Closing\"/><w:basedOn w:val=\"BodyText\"/>",
        "<w:qFormat/><w:pPr><w:spacing w:before=\"340\" w:after=\"460\"/>",
        "</w:pPr>",
    )));
}

#[test]
fn tight_list_items_in_a_div_keep_compact_spacing() {
    let (document, styles) =
        build("", ":::closing\nIntro\n\n- Alpha\n- Beta\n\nOutro\n:::\n");
    assert_eq!(
        props(&document, "Intro"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:after=\"200\"/>"
    );
    // Compact's built-in 1.8 pt (36 twips) before and after.
    for item in ["Alpha", "Beta"] {
        let props = props(&document, item);
        assert!(props.starts_with("<w:pStyle w:val=\"Closing\"/><w:numPr>"));
        assert!(
            props.ends_with("<w:spacing w:before=\"36\" w:after=\"36\"/>"),
            "{props}"
        );
    }
    assert_eq!(
        props(&document, "Outro"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\"/>"
    );
    assert!(styles.contains("w:styleId=\"Compact\""));
}

#[test]
fn nested_divs_are_blocks_of_their_own() {
    let (document, _) = build(
        "",
        concat!(
            "::::closing\nOuter one\n\n",
            ":::recipient\nInner one\n\nInner two\n:::\n\n",
            "Outer two\n::::\n",
        ),
    );
    assert_eq!(
        props(&document, "Outer one"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Inner one"),
        "<w:pStyle w:val=\"Recipient\"/><w:spacing w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Inner two"),
        "<w:pStyle w:val=\"Recipient\"/><w:spacing w:before=\"0\"/>"
    );
    assert_eq!(
        props(&document, "Outer two"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\"/>"
    );
}

#[test]
fn a_section_break_after_a_div_stays_in_its_last_paragraph() {
    // The spacing is written once the div has ended, which moves the bytes
    // after it. The break before a full-width figure must still land in the
    // div's last paragraph instead of an empty paragraph of its own.
    let (document, _) = build(
        "[design.page]\ncolumns = 2\n",
        concat!(
            ":::closing\nFirst\n\nLast\n:::\n\n",
            "![A wide figure](pic.png){#fig-wide width=100%}\n\n",
            "After\n",
        ),
    );
    let last = props(&document, "Last");
    assert!(
        last.starts_with(concat!(
            "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\"/>",
            "<w:sectPr>",
        )),
        "{last}"
    );
    assert_eq!(document.matches("<w:sectPr>").count(), 3);
}
