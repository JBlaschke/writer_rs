//! Div attributes survive Typst emission and parsing without changing layout.

use std::path::Path;

use serde_json::json;
use typst::layout::{Frame, FrameItem, Point, Transform};
use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::design::{Design, Layer, Origin, merge};
use writer::ir::{Attrs, Block, SourceInfo, SourceKind};
use writer::pipeline::{self, BuildOptions, WorldSlot};
use writer::typst::compile::compile_document;
use writer::typst::emit::{AssetPaths, EmitOptions, emit};

fn source(kind: SourceKind) -> SourceInfo {
    SourceInfo { path: "document.md".into(), base_dir: ".".into(), kind }
}

#[test]
fn classed_labelled_and_custom_divs_select_the_declared_style_name() {
    let config = Config::default();
    let parsed = writer::markdown::parse(&source(SourceKind::Markdown), concat!(
        ":::recipient {#recipient custom-style=\"recipient\"}\nAddress.\n:::\n\n",
        ":::columns\nColumns.\n:::\n\n",
        ":::unknown {custom-style=\"Reference Style\"}\nFallback.\n:::\n",
    ), &config).unwrap();
    let design = merge(&[Layer {
        table: json!({"styles": {"Recipient": {"keep_together": true}}})
            .as_object()
            .unwrap()
            .clone(),
        base_dir: Path::new("."),
        origin: Origin::FrontMatter,
    }])
    .unwrap();
    let options = EmitOptions {
        config: &config,
        design: &design,
        assets: AssetPaths::Virtual,
        template: None,
        bibliography: Vec::new(),
        style: "ieee".into(),
        footnote_prefix: String::new(),
        fragment: false,
    };
    let emitted = emit(&parsed.document, &parsed.assets, &options).unwrap();
    assert!(
        emitted.text.contains(concat!(
            "#metadata(none) <recipient>\n",
            "#writer-div(style: \"Recipient\", classes: (\"recipient\",))[",
        )),
        "{}",
        emitted.text
    );
    assert!(
        emitted
            .text
            .contains("#writer-div(style: none, classes: (\"columns\",))[",)
    );
    assert!(emitted.text.contains(
        "#writer-div(style: \"Reference Style\", classes: (\"unknown\",))[",
    ));
    let absent = Design::default();
    let raw = emit(
        &parsed.document,
        &parsed.assets,
        &EmitOptions { design: &absent, ..options },
    )
    .unwrap();
    assert!(raw.text.contains("#writer-div(style: \"recipient\""));
}

#[test]
fn wrapper_round_trip_preserves_classes_and_custom_style() {
    let config = Config::default();
    let initial = format!(
        "#import \"@writer/doc:{}\": *\n\
         #writer-div(style: \"X\", classes: (\"a\", \"b\"))[Body.]\n\
         #writer-div(style: none, classes: \"single\")[Other.]\n",
        writer::typst::WRITER_DOC_VERSION,
    );
    let parsed = writer::typst::frontend::parse(
        &source(SourceKind::Typst),
        &initial,
        &config,
    )
    .unwrap();
    assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
    let Block::Div { attrs, .. } = &parsed.document.blocks[0] else {
        panic!("expected a div: {:?}", parsed.document.blocks)
    };
    assert_eq!(attrs.classes, ["a", "b"]);
    assert_eq!(attrs.pairs.get("custom-style").map(String::as_str), Some("X"));
    let Block::Div { attrs: single, .. } = &parsed.document.blocks[1] else {
        panic!("expected a second div")
    };
    assert_eq!(single.classes, ["single"]);
    assert_eq!(single.pairs.get("custom-style"), None);
    let emitted = emit(
        &parsed.document,
        &AssetRegistry::new(),
        &EmitOptions {
            config: &config,
            design: &Design::default(),
            assets: AssetPaths::Virtual,
            template: None,
            bibliography: Vec::new(),
            style: "ieee".into(),
            footnote_prefix: String::new(),
            fragment: false,
        },
    )
    .unwrap();
    let again = writer::typst::frontend::parse(
        &source(SourceKind::Typst),
        &emitted.text,
        &config,
    )
    .unwrap();
    let divs = |blocks: &[Block]| {
        blocks
            .iter()
            .filter_map(|block| {
                if let Block::Div { attrs, .. } = block {
                    Some(attrs.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<Attrs>>()
    };
    assert_eq!(divs(&parsed.document.blocks), divs(&again.document.blocks));
}

fn positions(
    frame: &Frame, transform: Transform, result: &mut Vec<(String, f64, f64)>,
) {
    for (position, item) in frame.items() {
        let transform =
            transform.pre_concat(Transform::translate(position.x, position.y));
        match item {
            FrameItem::Group(group) => positions(
                &group.frame,
                transform.pre_concat(group.transform),
                result,
            ),
            FrameItem::Text(run) => {
                let point = Point::zero().transform(transform);
                result.push((
                    run.text.to_string(),
                    point.x.to_pt(),
                    point.y.to_pt(),
                ));
            }
            _ => {}
        }
    }
}

#[test]
fn identity_wrappers_preserve_pdf_pages_and_text_positions() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("writer.toml"),
        "[pdf]\nsystem_fonts = false\n",
    )
    .unwrap();
    let input = directory.path().join("document.md");
    let opts = BuildOptions::new(&input);
    let mut worlds = WorldSlot::new();
    let mut layouts = Vec::new();
    for content in [
        "First.\n\nSecond.\n",
        ":::center\nFirst.\n:::\n\n:::columns\nSecond.\n:::\n",
    ] {
        std::fs::write(&input, content).unwrap();
        let prepared = pipeline::prepare(&opts).unwrap();
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        pipeline::pdf::load_typst_main(&prepared, &opts, world).unwrap();
        let document = compile_document(world).into_result().unwrap().0;
        assert_eq!(document.pages().len(), 1);
        let mut runs = Vec::new();
        positions(&document.pages()[0].frame, Transform::identity(), &mut runs);
        layouts.push(runs);
    }
    assert_eq!(layouts[0], layouts[1]);
}
