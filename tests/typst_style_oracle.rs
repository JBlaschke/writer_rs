//! Named paragraph styles checked against actual compiled page layout.

use std::path::Path;

use typst::foundations::{Datetime, Label, Value};
use typst::introspection::{Introspector, MetadataElem};
use typst::layout::{Frame, FrameItem, Point, Transform};
use typst::utils::PicoStr;
use typst_layout::PagedDocument;
use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::design::{Design, Layer, Origin, Pt, merge};
use writer::typst::compile::compile_document;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld};

fn letter_styles(root: &Path) -> Design {
    merge(&[Layer {
        table: serde_json::json!({
            "text": {"size": "11pt"},
            "styles": {
                "Recipient": {"line_height": "12.1pt", "space_after": "6mm",
                    "keep_together": true},
                "Closing": {"space_before": "6mm", "space_after": "8mm",
                    "keep_with_next": true},
                "Signature": {"line_height": "12.1pt", "space_before": "0mm",
                    "space_after": "0mm", "keep_together": true},
            },
        })
        .as_object()
        .unwrap()
        .clone(),
        base_dir: root,
        origin: Origin::FrontMatter,
    }])
    .unwrap()
}

fn compile(root: &Path, design: &Design, body: &str) -> PagedDocument {
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    let mut world = WriterWorld::from_config_design(
        &config,
        design,
        &AssetRegistry::new(),
        root,
        MainFile::Virtual,
        Datetime::from_ymd(2026, 11, 1).unwrap(),
    )
    .unwrap();
    let source = format!(
        "#import \"@writer/doc:{}\": *\n#show: writer-doc.with(spacing: 0pt)\n{body}",
        writer::typst::WRITER_DOC_VERSION,
    );
    world.set_main_text(&source, SourceMap::default());
    compile_document(&mut world).into_result().unwrap().0
}

fn runs(frame: &Frame, transform: Transform, result: &mut Vec<(String, f64)>) {
    for (position, item) in frame.items() {
        let transform =
            transform.pre_concat(Transform::translate(position.x, position.y));
        match item {
            FrameItem::Group(group) => runs(
                &group.frame,
                transform.pre_concat(group.transform),
                result,
            ),
            FrameItem::Text(run) => result.push((
                run.text.to_string(),
                Point::zero().transform(transform).y.to_pt(),
            )),
            _ => {}
        }
    }
}

fn marker(document: &PagedDocument, marker: &str) -> (usize, f64) {
    for (index, page) in document.pages().iter().enumerate() {
        let mut found = Vec::new();
        runs(&page.frame, Transform::identity(), &mut found);
        if let Some((_, baseline)) =
            found.iter().find(|(text, _)| text.contains(marker))
        {
            return (index + 1, *baseline);
        }
    }
    panic!("missing marker {marker}");
}

#[test]
fn recipient_uses_exact_leading_and_six_mm_of_following_space() {
    let directory = tempfile::tempdir().unwrap();
    let design = letter_styles(directory.path());
    let body = r#"#writer-div(style: "Recipient")[
#context [#metadata(par.leading.pt()) <recipient-leading>]
RECIPIENTFIRST\
RECIPIENTLAST
]
AFTERRECIPIENT
"#;
    let with_spacing = compile(directory.path(), &design, body);
    let content = with_spacing
        .introspector()
        .query_label(Label::new(PicoStr::intern("recipient-leading")).unwrap())
        .unwrap();
    let metadata = content.to_packed::<MetadataElem>().unwrap();
    let Value::Float(leading) = &metadata.value else {
        panic!("expected resolved leading metadata")
    };
    assert!((leading - 3.3).abs() < 0.001);
    let mut no_spacing = design.clone();
    no_spacing.styles.get_mut("Recipient").unwrap().space_after = Some(Pt(0.0));
    let without_spacing = compile(directory.path(), &no_spacing, body);
    let last = marker(&with_spacing, "RECIPIENTLAST");
    let following = marker(&with_spacing, "AFTERRECIPIENT");
    let control_last = marker(&without_spacing, "RECIPIENTLAST");
    let control_following = marker(&without_spacing, "AFTERRECIPIENT");
    assert_eq!(
        (last.0, following.0, control_last.0, control_following.0),
        (1, 1, 1, 1)
    );
    let extra_space =
        (following.1 - last.1) - (control_following.1 - control_last.1);
    assert!(
        (extra_space - 6.0 * 72.0 / 25.4).abs() < 0.01,
        "actual additional space: {extra_space}"
    );
    assert!(following.1 - last.1 >= 17.0);
}

#[test]
fn recipient_moves_both_lines_to_the_next_page_when_they_do_not_fit() {
    let directory = tempfile::tempdir().unwrap();
    let design = letter_styles(directory.path());
    let mut breakable = design.clone();
    breakable.styles.get_mut("Recipient").unwrap().keep_together = false;
    let mut boundary = None;
    for filler in (70..100).step_by(2) {
        let body = format!(
            r#"#set page(width: 200pt, height: 120pt, margin: 10pt, numbering: none)
#block(height: {filler}pt, above: 0pt, below: 0pt)[FILLER]
#writer-div(style: "Recipient")[RECIPIENTFIRST

RECIPIENTLAST]
"#
        );
        let control = compile(directory.path(), &breakable, &body);
        if marker(&control, "RECIPIENTFIRST").0 == 1
            && marker(&control, "RECIPIENTLAST").0 == 2
        {
            boundary = Some(body);
            break;
        }
    }
    let body = boundary
        .expect("the breakable control must straddle a natural boundary");
    let together = compile(directory.path(), &design, &body);
    assert_eq!(marker(&together, "RECIPIENTFIRST").0, 2);
    assert_eq!(marker(&together, "RECIPIENTLAST").0, 2);
}

#[test]
fn closing_stays_with_signature_at_a_natural_page_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let design = letter_styles(directory.path());
    let mut detached = design.clone();
    detached.styles.get_mut("Closing").unwrap().keep_with_next = false;
    let mut boundary = None;
    for filler in (50..125).step_by(3) {
        let body = format!(
            r#"#set page(width: 220pt, height: 160pt, margin: 10pt, numbering: none)
#block(height: {filler}pt, above: 0pt, below: 0pt)[FILLER]
#writer-div(style: "Closing")[CLOSINGTEXT]
#writer-div(style: "Signature")[SIGNATUREFIRST\
SIGNATURELAST]
"#
        );
        let control = compile(directory.path(), &detached, &body);
        if marker(&control, "CLOSINGTEXT").0 == 1
            && marker(&control, "SIGNATUREFIRST").0 == 2
        {
            boundary = Some(body);
            break;
        }
    }
    let body = boundary
        .expect("the detached control must straddle a natural boundary");
    let together = compile(directory.path(), &design, &body);
    assert_eq!(marker(&together, "CLOSINGTEXT").0, 2);
    assert_eq!(marker(&together, "SIGNATUREFIRST").0, 2);
    assert_eq!(marker(&together, "SIGNATURELAST").0, 2);
}
