//! Line pitch and paragraph gaps of the Typst PDF follow the design's
//! `text.line_height` and `text.paragraph_spacing` with the geometry of HTML
//! and Word (design.md §6.2): a multiple `n` is a baseline pitch of `n × size`,
//! and a paragraph break adds the paragraph spacing to a full pitch, as HTML's
//! `margin-block: 0 <spacing>` and Word's `w:after` do.

use std::path::Path;

use typst::foundations::{Datetime, Label, Value};
use typst::introspection::{Introspector, MetadataElem};
use typst::layout::{Abs, Frame, FrameItem, Point, Transform};
use typst::text::{
    FontInstance, FontStretch, FontStyle, FontVariant, FontVariations,
    FontWeight,
};
use typst::utils::PicoStr;
use typst_layout::PagedDocument;
use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::design::{Design, Layer, Origin, merge};
use writer::typst::compile::compile_document;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld};

/// The §11 letter's body size, line height and paragraph spacing, in pt.
const SIZE: f64 = 11.0;
const LINE_HEIGHT: f64 = 1.35;
const SPACING: f64 = 3.6 * 72.0 / 25.4;
const TOLERANCE: f64 = 0.05;

/// A two-paragraph body whose last paragraph reports the resolved leading.
const BODY: &str = concat!(
    "LINEONE \\\nLINETWO\n\n",
    "PARATWO #context [#metadata(par.leading.to-absolute().pt()) <leading>]\n",
);

/// The embedded `family`'s regular face, instantiated at the body size: the
/// metrics Typst lays its lines out with live on the instance.
fn embedded(family: &str) -> FontInstance {
    let regular = FontVariant::new(
        FontStyle::Normal,
        FontWeight::REGULAR,
        FontStretch::NORMAL,
    );
    writer::typst::fonts::embedded()
        .find(|(_, info)| info.family == family)
        .map(|(font, _)| font)
        .expect("embedded family")
        .instantiate(regular, Abs::pt(SIZE), &FontVariations::default())
}

fn design(table: serde_json::Value, root: &Path) -> Design {
    merge(&[Layer {
        table: table.as_object().unwrap().clone(),
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
        Datetime::from_ymd(2026, 11, 9).unwrap(),
    )
    .unwrap();
    let source = format!(
        "#import \"@writer/doc:{}\": *\n#show: writer-doc\n{body}",
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

/// The baseline of the first run on the first page containing `marker`.
fn baseline(document: &PagedDocument, marker: &str) -> f64 {
    let mut found = Vec::new();
    runs(&document.pages()[0].frame, Transform::identity(), &mut found);
    found
        .iter()
        .find(|(text, _)| text.contains(marker))
        .unwrap_or_else(|| panic!("missing marker {marker}: {found:?}"))
        .1
}

/// The `par.leading` the package resolved, reported by `BODY`.
fn leading(document: &PagedDocument) -> f64 {
    let content = document
        .introspector()
        .query_label(Label::new(PicoStr::intern("leading")).unwrap())
        .unwrap();
    let metadata = content.to_packed::<MetadataElem>().unwrap();
    let Value::Float(leading) = metadata.value else {
        panic!("expected a resolved leading, got {:?}", metadata.value)
    };
    leading
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "{what}: {actual} pt, expected {expected} pt"
    );
}

/// Compiles `BODY` under `fonts` with the letter's text values and checks the
/// line pitch and the paragraph gap; returns the resolved leading.
fn geometry(root: &Path, fonts: serde_json::Value) -> f64 {
    let design = design(
        serde_json::json!({
            "fonts": fonts,
            "text": {
                "size": "11pt", "line_height": LINE_HEIGHT,
                "paragraph_spacing": "3.6mm",
            },
        }),
        root,
    );
    let document = compile(root, &design, BODY);
    let first = baseline(&document, "LINEONE");
    let second = baseline(&document, "LINETWO");
    let next = baseline(&document, "PARATWO");
    assert_close(second - first, LINE_HEIGHT * SIZE, "line pitch");
    assert_close(next - second, LINE_HEIGHT * SIZE + SPACING, "paragraph gap");
    leading(&document)
}

#[test]
fn a_declared_font_takes_the_design_pitch_and_paragraph_gap() {
    let directory = tempfile::tempdir().unwrap();
    let font = embedded("Libertinus Serif");
    let path = directory.path().join("libertinus.otf");
    std::fs::write(&path, font.font().data()).unwrap();
    let leading = geometry(
        directory.path(),
        serde_json::json!({"body": {
            "family": "Libertinus Serif", "files": [path],
        }}),
    );
    // Typst's line box runs from the cap height to the baseline, so the
    // leading is the pitch less the font's OS/2 cap height at the size.
    let metrics = font.metrics();
    assert!(metrics.cap_height < metrics.ascender, "a cap height metric");
    assert_close(
        leading,
        LINE_HEIGHT * SIZE - metrics.cap_height.get() * SIZE,
        "leading",
    );
}

#[test]
fn a_font_without_a_cap_height_metric_keeps_the_pitch() {
    // DejaVu Sans Mono's OS/2 table predates `sCapHeight`; Typst lays it out
    // with its ascender as the line box, and the measured leading follows.
    let directory = tempfile::tempdir().unwrap();
    let font = embedded("DejaVu Sans Mono");
    let metrics = font.metrics();
    assert_eq!(metrics.cap_height, metrics.ascender, "no cap height metric");
    let leading = geometry(
        directory.path(),
        serde_json::json!({"body": "DejaVu Sans Mono"}),
    );
    assert_close(
        leading,
        LINE_HEIGHT * SIZE - metrics.ascender.get() * SIZE,
        "leading",
    );
}

#[test]
fn a_missing_family_keeps_the_pitch_with_the_fallback_font() {
    let directory = tempfile::tempdir().unwrap();
    let leading = geometry(
        directory.path(),
        serde_json::json!({"body": "Writer Unknown Font Family"}),
    );
    let fallback = embedded("Libertinus Serif").metrics().cap_height.get();
    assert_close(leading, LINE_HEIGHT * SIZE - fallback * SIZE, "leading");
}

#[test]
fn styles_and_furniture_lines_take_their_own_pitch() {
    let directory = tempfile::tempdir().unwrap();
    let design = design(
        serde_json::json!({
            "text": {
                "size": "11pt", "line_height": LINE_HEIGHT,
                "paragraph_spacing": "3.6mm",
            },
            "styles": {
                "Recipient": {"line_height": "12.1pt"},
                "Date": {"size": "10pt"},
            },
            "page": {"header": {
                "lines": ["HONE", "HTWO", "HTHREE"], "size": "7.5pt",
            }},
        }),
        directory.path(),
    );
    let body = concat!(
        "#writer-div(style: \"Recipient\")[RONE \\\nRTWO\n\nRTHREE]\n",
        "#writer-div(style: \"Date\")[DONE \\\nDTWO]\n",
        "BODYLINE\n",
    );
    let document = compile(directory.path(), &design, body);
    let at = |marker| baseline(&document, marker);
    // An exact pitch holds, and the paragraph spacing still comes on top.
    assert_close(at("RTWO") - at("RONE"), 12.1, "Recipient pitch");
    assert_close(at("RTHREE") - at("RTWO"), 12.1 + SPACING, "Recipient gap");
    // A multiple follows the paragraph's own size, as HTML's unitless
    // `line-height` and Word's `auto` lines do.
    assert_close(at("DTWO") - at("DONE"), LINE_HEIGHT * 10.0, "Date pitch");
    // Furniture lines are a multiple of their size too.
    assert_close(at("HTWO") - at("HONE"), LINE_HEIGHT * 7.5, "header pitch");
    assert_close(at("HTHREE") - at("HTWO"), LINE_HEIGHT * 7.5, "header pitch");
}
