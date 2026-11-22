use std::path::Path;

use typst::foundations::{Datetime, Label, Value};
use typst::introspection::{Introspector, MetadataElem};
use typst::layout::{Frame, FrameItem, Point, Transform};
use typst::utils::PicoStr;
use typst_layout::PagedDocument;
use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::design::{Design, Layer, Origin, merge};
use writer::typst::compile::compile_document;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld};

const TOLERANCE: f64 = 0.01;

/// Reports the line box, the leading and the paragraph gap in force where it
/// stands, as JSON: lengths in pt, any other value as its `repr`.
const PROBE: &str = r##"
#let probe-edge(value) = {
    if type(value) == length { value.to-absolute().pt() } else { repr(value) }
}
#let probe = context [#metadata(json.encode((
    top: probe-edge(text.top-edge), bottom: probe-edge(text.bottom-edge),
    leading: par.leading.to-absolute().pt(),
    gap: par.spacing.to-absolute().pt(),
))) <probe>]
"##;

/// Two lines of one paragraph, a second paragraph and the probe.
const LINES: &str = "LINEONE \\\nLINETWO\n\nPARATWO #probe\n";

fn design(table: serde_json::Value, root: &Path) -> Design {
    merge(&[Layer {
        table: table.as_object().unwrap().clone(),
        base_dir: root,
        origin: Origin::FrontMatter,
    }])
    .unwrap()
}

/// Compiles `body` after the package's import line and the probe.
fn compile(root: &Path, design: &Design, body: &str) -> PagedDocument {
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    let mut world = WriterWorld::from_config_design(
        &config,
        design,
        &AssetRegistry::new(),
        root,
        MainFile::Virtual,
        Datetime::from_ymd(2026, 11, 21).unwrap(),
    )
    .unwrap();
    let source = format!(
        "#import \"@writer/doc:{}\": *\n{PROBE}\n{body}",
        writer::typst::WRITER_DOC_VERSION,
    );
    world.set_main_text(&source, SourceMap::default());
    compile_document(&mut world).into_result().unwrap().0
}

/// Every text run of `frame` with its baseline's page position, in pt.
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

/// Checks the baseline of every `(text, y)` pair on page `page` (from 1).
fn assert_baselines(
    document: &PagedDocument, page: usize, expected: &[(&str, f64)],
) {
    let mut found = Vec::new();
    runs(&document.pages()[page - 1].frame, Transform::identity(), &mut found);
    for (marker, expected) in expected {
        let (_, actual) = found
            .iter()
            .find(|(text, _)| text == marker)
            .unwrap_or_else(|| panic!("no {marker} on page {page}: {found:?}"));
        assert!(
            (actual - expected).abs() < TOLERANCE,
            "{marker}: baseline at {actual} pt, expected {expected} pt"
        );
    }
}

/// What the probe reported.
fn probe(document: &PagedDocument) -> serde_json::Value {
    let content = document
        .introspector()
        .query_label(Label::new(PicoStr::intern("probe")).unwrap())
        .unwrap();
    let metadata = content.to_packed::<MetadataElem>().unwrap();
    let Value::Str(text) = &metadata.value else {
        panic!("expected a JSON string, got {:?}", metadata.value)
    };
    serde_json::from_str(text.as_str()).unwrap()
}

/// Checks the numbers the probe reported, each `(key, pt)`.
fn assert_probe(document: &PagedDocument, expected: &[(&str, f64)]) {
    let probe = probe(document);
    for (key, expected) in expected {
        let actual = probe[*key]
            .as_f64()
            .unwrap_or_else(|| panic!("{key} is no number: {probe}"));
        assert!(
            (actual - expected).abs() < TOLERANCE,
            "{key}: {actual} pt, expected {expected} pt"
        );
    }
}

fn spaced(root: &Path) -> Design {
    design(
        serde_json::json!({
            "text": {
                "size": "11pt", "line_height": 1.35,
                "paragraph_spacing": "3.6mm",
            },
            "page": {"margin": {"top": "30mm"}},
            "styles": {
                "Wide": {"space_before": "17pt", "space_after": "23pt"},
                "Narrow": {"space_before": "5pt", "space_after": "4pt"},
                "Flush": {"space_before": "0pt", "space_after": "0pt"},
                "Bare": {"keep_together": true},
                "Heading 1": {
                    "size": "14pt", "space_before": "20pt",
                    "space_after": "6pt",
                },
                "Heading 2": {
                    "size": "12pt", "space_before": "4pt",
                    "space_after": "2pt",
                },
            },
        }),
        root,
    )
}

fn lines(root: &Path, text: serde_json::Value) -> Design {
    design(serde_json::json!({"text": text}), root)
}

const ADJACENT: &str = concat!(
    "#show: writer-doc\n",
    "PARAA\n\n",
    "#writer-div(style: \"Wide\")[WIDEONE\n\nWIDETWO]\n",
    "PARAB\n\n",
    "#writer-div(style: \"Narrow\")[NARROWONE]\n",
    "PARAC\n\n",
    "#writer-div(style: \"Flush\")[FLUSHONE]\n",
    "PARAD\n\n",
    "#writer-div(style: \"Bare\")[BAREONE\n\nBARETWO]\n",
    "PARAE\n\n",
    "#writer-div(style: \"Wide\")[WIDETHREE]\n",
    "#writer-div(style: \"Wide\")[WIDEFOUR]\n",
    "#writer-div(style: \"Narrow\")[NARROWTWO]\n",
    "PARAF\n\n",
    "= HEADONE\n\n",
    "PARAG\n\n",
    "== HEADTWO\n\n",
    "PARAH\n\n",
    "= HEADTHREE\n\n",
    "== HEADFOUR\n\n",
    "PARAI\n",
);
const FIRST_HEADING: &str = concat!(
    "#show: writer-doc\n", "= HEADFIRST\n\n", "PARAA\n\n", "#pagebreak()\n\n",
    "= HEADNEXT\n\n", "PARAB\n",
);
const FIRST_DIV: &str = concat!(
    "#show: writer-doc\n",
    "#writer-div(style: \"Wide\")[WIDEFIRST]\n",
    "PARAA\n",
);
const FIRST_PARAGRAPH: &str = "#show: writer-doc\n\nPARAA\n\nPARAB\n";
const PAGE_RULE: &str = concat!(
    "#show: writer-doc\n\n",
    "#set page(width: 300pt, height: 200pt, margin: 10pt, numbering: none)\n\n",
    "= HEADFIRST\n\n",
    "PARAA\n",
);
const TEMPLATE_RULE: &str = concat!(
    "#let template(..args, body) = {\n",
    "    show: writer-doc.with(..args)\n",
    "    set page(margin: 40pt)\n",
    "    show heading: set text(fill: red)\n",
    "    body\n}\n",
    "#show: template\n\n",
    "= HEADFIRST\n\n",
    "PARAA\n",
);
const COVER: &str = concat!(
    "#show: writer-doc.with(title: [T], title-block: (meta, theme) => ",
    "page(margin: 5pt)[COVER])\n\n",
    "= HEADFIRST\n\n",
    "PARAA\n",
);
const LISTS: &str = concat!(
    "#show: writer-doc\n\n",
    "PARAA\n\n",
    "- TIGHTONE\n- TIGHTTWO\n\n",
    "PARAB\n\n",
    "- LOOSEONE\n\n- LOOSETWO\n\n",
    "PARAC\n\n",
    "#table(columns: 2, [CELLA], [], [], [])\n\n",
    "PARAD\n",
);
const NOTES: &str = concat!(
    "#show: writer-doc\n",
    "PARAA#footnote[NOTEONE \\ NOTETWO #probe]\n\n",
    "PARAB#footnote[NOTETHREE]\n",
);
const EQUATION: &str = concat!(
    "#show: writer-doc\n",
    "PARAA\n\n",
    "$ \"ROWONE\" #probe \\ \"ROWTWO\" $\n\n",
    "PARAB\n",
);

#[test]
fn adjacent_blocks_keep_the_larger_of_their_spaces() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let document = compile(root, &spaced(root), ADJACENT);
    assert_eq!(document.pages().len(), 1);
    assert_baselines(
        &document,
        1,
        &[
            ("PARAA", 94.873),
            ("WIDEONE", 126.723),
            ("WIDETWO", 151.778),
            ("PARAB", 189.628),
            ("NARROWONE", 214.683),
            ("PARAC", 233.533),
            ("FLUSHONE", 258.588),
            ("PARAD", 273.438),
            ("BAREONE", 298.493),
            ("BARETWO", 323.548),
            ("PARAE", 348.603),
            ("WIDETHREE", 380.453),
            ("WIDEFOUR", 418.303),
            ("NARROWTWO", 456.153),
            ("PARAF", 475.003),
            ("HEADONE", 512.535),
            ("PARAG", 534.753),
            ("HEADTWO", 560.702),
            ("PARAH", 578.008),
            ("HEADTHREE", 615.540),
            ("HEADFOUR", 638.652),
            ("PARAI", 655.958),
        ],
    );
}

#[test]
fn the_first_block_keeps_its_space_before_on_page_one_only() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let design = spaced(root);

    let document = compile(root, &design, FIRST_HEADING);
    assert_eq!(document.pages().len(), 2);
    assert_baselines(
        &document,
        1,
        &[("HEADFIRST", 117.555), ("PARAA", 139.773)],
    );
    assert_baselines(&document, 2, &[("HEADNEXT", 97.555), ("PARAB", 119.773)]);

    let document = compile(root, &design, FIRST_DIV);
    assert_eq!(document.pages().len(), 1);
    assert_baselines(
        &document,
        1,
        &[("WIDEFIRST", 111.873), ("PARAA", 149.723)],
    );

    let document = compile(root, &design, FIRST_PARAGRAPH);
    assert_eq!(document.pages().len(), 1);
    assert_baselines(&document, 1, &[("PARAA", 94.873), ("PARAB", 119.928)]);
}

#[test]
fn a_page_rule_at_the_start_of_the_body_adds_no_page() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let design = spaced(root);

    let document = compile(root, &design, PAGE_RULE);
    assert_eq!(document.pages().len(), 1);
    assert_baselines(&document, 1, &[("HEADFIRST", 42.516), ("PARAA", 64.734)]);

    let document = compile(root, &design, TEMPLATE_RULE);
    assert_eq!(document.pages().len(), 1);
    assert_baselines(&document, 1, &[("HEADFIRST", 72.516), ("PARAA", 94.734)]);
}

#[test]
fn a_title_block_function_gets_no_opening_mark() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let document = compile(root, &spaced(root), COVER);
    assert_eq!(document.pages().len(), 2);
    assert_baselines(&document, 1, &[("COVER", 14.834)]);
    assert_baselines(
        &document,
        2,
        &[("HEADFIRST", 97.555), ("PARAA", 119.773)],
    );
}

#[test]
fn lists_and_empty_cells_follow_the_model() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let document = compile(root, &spaced(root), LISTS);
    assert_baselines(
        &document,
        1,
        &[
            ("PARAA", 94.873),
            ("TIGHTONE", 119.928),
            ("TIGHTTWO", 134.778),
            ("PARAB", 159.833),
            ("LOOSEONE", 184.888),
            ("LOOSETWO", 209.943),
            ("PARAC", 234.998),
            ("CELLA", 262.457),
            ("PARAD", 304.766),
        ],
    );
}

#[test]
fn line_boxes_follow_the_line_height() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let body = format!("#show: writer-doc\n{LINES}");

    let document = compile(
        root,
        &lines(
            root,
            serde_json::json!({
                "size": "11pt", "line_height": 1.0, "paragraph_spacing": "3.6mm",
            }),
        ),
        &body,
    );
    assert_probe(
        &document,
        &[
            ("top", 8.626),
            ("bottom", -2.374),
            ("leading", 0.0),
            ("gap", 10.205),
        ],
    );
    assert_baselines(
        &document,
        1,
        &[("LINEONE", 79.492), ("LINETWO", 90.492), ("PARATWO", 111.697)],
    );

    let document = compile(
        root,
        &lines(
            root,
            serde_json::json!({
                "size": "11pt", "line_height": "15pt",
                "paragraph_spacing": "3.6mm",
            }),
        ),
        &body,
    );
    assert_probe(
        &document,
        &[("top", 12.0), ("bottom", -3.0), ("leading", 0.0), ("gap", 10.205)],
    );
    assert_baselines(
        &document,
        1,
        &[("LINEONE", 82.866), ("LINETWO", 97.866), ("PARATWO", 123.071)],
    );

    let document = compile(
        root,
        &lines(
            root,
            serde_json::json!({
                "size": "11pt", "paragraph_spacing": "3.6mm",
            }),
        ),
        &body,
    );
    assert_probe(
        &document,
        &[
            ("top", 9.834),
            ("bottom", -4.554),
            ("leading", 0.0),
            ("gap", 10.205),
        ],
    );
    assert_baselines(
        &document,
        1,
        &[("LINEONE", 80.700), ("LINETWO", 95.088), ("PARATWO", 119.681)],
    );

    let document =
        compile(root, &lines(root, serde_json::json!({"size": "11pt"})), &body);
    assert_probe(
        &document,
        &[("top", 9.834), ("bottom", -4.554), ("leading", 0.0), ("gap", 6.05)],
    );
    assert_baselines(
        &document,
        1,
        &[("LINEONE", 80.700), ("LINETWO", 95.088), ("PARATWO", 115.526)],
    );
}

#[test]
fn an_undeclared_design_keeps_typsts_layout() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let body = format!("#show: writer-doc\n{LINES}");
    let document = compile(root, &Design::default(), &body);
    assert_eq!(probe(&document)["top"], "\"cap-height\"");
    assert_eq!(probe(&document)["bottom"], "\"baseline\"");
    assert_probe(&document, &[("leading", 7.15), ("gap", 13.2)]);
    assert_baselines(
        &document,
        1,
        &[("LINEONE", 78.104), ("LINETWO", 92.492), ("PARATWO", 112.930)],
    );
}

#[test]
fn explicit_arguments_keep_typsts_meaning() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let design = lines(
        root,
        serde_json::json!({
            "size": "11pt", "line_height": 1.35, "paragraph_spacing": "3.6mm",
        }),
    );
    let document_of = |argument: &str| {
        let body = format!("#show: writer-doc.with({argument})\n{LINES}");
        compile(root, &design, &body)
    };

    let document = document_of("leading: 0.65em");
    assert_probe(
        &document,
        &[
            ("top", 9.834),
            ("bottom", -4.554),
            ("leading", 0.0),
            ("gap", 10.205),
        ],
    );
    assert_baselines(
        &document,
        1,
        &[("LINEONE", 80.700), ("LINETWO", 95.088), ("PARATWO", 119.681)],
    );

    let document = document_of("spacing: 20pt");
    assert_probe(&document, &[("bottom", -5.016), ("gap", 12.388)]);
    assert_baselines(
        &document,
        1,
        &[("LINEONE", 80.700), ("LINETWO", 95.550), ("PARATWO", 122.788)],
    );

    let document = document_of("spacing: 0pt");
    assert_probe(&document, &[("bottom", -5.016), ("gap", 0.0)]);
    assert_baselines(
        &document,
        1,
        &[("LINEONE", 80.700), ("LINETWO", 95.550), ("PARATWO", 110.400)],
    );
}

#[test]
fn footnote_lines_are_line_boxes_at_the_notes_size() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let design_of = |line_height: serde_json::Value| {
        lines(
            root,
            serde_json::json!({
                "size": "11pt", "line_height": line_height,
                "paragraph_spacing": "3.6mm",
            }),
        )
    };

    let document = compile(root, &design_of(1.35.into()), NOTES);
    assert_probe(
        &document,
        &[("top", 8.359), ("bottom", -4.264), ("leading", 0.0)],
    );
    assert_baselines(
        &document,
        1,
        &[("NOTEONE", 741.515), ("NOTETWO", 754.138), ("NOTETHREE", 766.760)],
    );

    let document = compile(root, &design_of(1.0.into()), NOTES);
    assert_probe(
        &document,
        &[("top", 7.332), ("bottom", -2.018), ("leading", 0.0)],
    );
    assert_baselines(
        &document,
        1,
        &[("NOTEONE", 748.004), ("NOTETWO", 757.354), ("NOTETHREE", 769.006)],
    );
}

#[test]
fn a_display_equation_keeps_typsts_leading_between_its_rows() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let design_of = |line_height: serde_json::Value| {
        lines(
            root,
            serde_json::json!({
                "size": "11pt", "line_height": line_height,
                "paragraph_spacing": "3.6mm",
            }),
        )
    };

    let document = compile(root, &design_of(1.35.into()), EQUATION);
    assert_probe(&document, &[("leading", 7.612)]);
    assert_baselines(
        &document,
        1,
        &[
            ("PARAA", 80.700),
            ("ROWONE", 103.676),
            ("ROWTWO", 119.285),
            ("PARAB", 139.566),
        ],
    );

    let document = compile(root, &design_of("15pt".into()), EQUATION);
    assert_probe(&document, &[("leading", 7.762)]);
    assert_baselines(&document, 1, &[("ROWONE", 103.826), ("ROWTWO", 119.585)]);
}
