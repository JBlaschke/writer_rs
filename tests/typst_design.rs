//! Compiler-backed contracts for design inputs, typography, and page geometry.

use std::path::Path;

use typst::foundations::{Datetime, Label, Value};
use typst::introspection::{Introspector, MetadataElem};
use typst::utils::PicoStr;
use writer::assets::registry::AssetRegistry;
use writer::config::{Config, Target};
use writer::design::{Design, Layer, Origin, PathMode, merge};
use writer::pipeline::{self, BuildOptions, WorldSlot};
use writer::typst::compile::compile_document;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld, world_inputs};

fn design(value: serde_json::Value, root: &Path) -> Design {
    merge(&[Layer {
        table: value.as_object().unwrap().clone(),
        base_dir: root,
        origin: Origin::FrontMatter,
    }])
    .unwrap()
}

#[test]
fn inputs_are_absent_without_design_and_virtualize_furniture_images() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let prepared = pipeline::prepare(&BuildOptions::new(
        root.join("tests/fixtures/design/letter/letter.md"),
    ))
    .unwrap();
    let mut assets = prepared.assets.clone();
    prepared.design.register_images(&mut assets);
    let inputs = world_inputs(&prepared.config, &prepared.design, &assets);
    assert_eq!(
        inputs.keys().map(String::as_str).collect::<Vec<_>>(),
        ["writer-config", "writer-design", "writer-version"]
    );
    let value: serde_json::Value =
        serde_json::from_str(&inputs["writer-design"]).unwrap();
    assert_eq!(value, prepared.design.to_json(PathMode::Virtual(&assets)));
    assert!(
        value["page"]["header"]["first"]["image"]["path"]
            .as_str()
            .unwrap()
            .starts_with("/__assets/")
    );
    let mut snapshot = value.clone();
    snapshot["page"]["header"]["first"]["image"]["path"] =
        serde_json::json!("/__assets/<logo>.png");
    insta::assert_json_snapshot!("typst_design_input", snapshot);
    let absent = world_inputs(
        &Config::default(),
        &Design::default(),
        &AssetRegistry::new(),
    );
    assert!(!absent.contains_key("writer-design"));
}

#[test]
fn design_font_file_adds_faces_to_the_world() {
    let directory = tempfile::tempdir().unwrap();
    let (font, info) = typst_kit::fonts::embedded().next().unwrap();
    let path = directory.path().join("fixture.ttf");
    std::fs::write(&path, font.data()).unwrap();
    let design = design(
        serde_json::json!({"fonts": {"body": {
            "family": info.family, "files": [path],
        }}}),
        directory.path(),
    );
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    let world = WriterWorld::from_config_design(
        &config,
        &design,
        &AssetRegistry::new(),
        directory.path(),
        MainFile::Virtual,
        Datetime::from_ymd(2026, 11, 1).unwrap(),
    )
    .unwrap();
    assert!(
        typst::World::book(&world).contains_family(&info.family.to_lowercase())
    );
    let baseline = WriterWorld::from_config(
        &config,
        directory.path(),
        MainFile::Virtual,
        Datetime::from_ymd(2026, 11, 1).unwrap(),
    )
    .unwrap();
    let faces = |world: &WriterWorld| {
        typst::World::book(world)
            .families()
            .map(|(_, faces)| faces.count())
            .sum::<usize>()
    };
    assert!(faces(&world) > faces(&baseline));
    assert_eq!(
        typst::World::font(&world, faces(&baseline)).unwrap().data(),
        font.data(),
    );
}

#[test]
fn delegating_template_receives_design_defaults_and_explicit_arguments_win() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let template = format!(
        r#"#import "@writer/doc:{}" as base
#let writer-doc(..args, body) = base.writer-doc(..args)[
    #block[
    #context [#metadata((
        size: text.size.pt(), font: text.font,
        leading: par.leading.pt(),
        justify: par.justify,
    )) <design-settings>]
    #body
    ] <design-body>
]
"#,
        writer::typst::WRITER_DOC_VERSION
    );
    std::fs::write(root.join("template.typ"), template).unwrap();
    std::fs::write(root.join("writer.toml"), concat!(
        "[pdf]\nsystem_fonts = false\npaper = 'us-letter'\ntemplate = 'template.typ'\n",
        "[design.fonts]\nbody = 'DejaVu Sans Mono'\n",
        "[design.text]\nsize = '11pt'\nline_height = 1.35\njustify = false\n",
        "[design.page]\npaper = 'a4'\n",
        "margin = {top = '50mm', left = '22mm', bottom = '38mm'}\n",
    )).unwrap();
    let input = root.join("letter.md");
    std::fs::write(&input, "Body paragraph.\n").unwrap();
    let opts = BuildOptions::new(input);
    let prepared = pipeline::prepare(&opts).unwrap();
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    pipeline::pdf::load_typst_main(&prepared, &opts, world).unwrap();
    let (document, _) = compile_document(world).into_result().unwrap();
    let content = document
        .introspector()
        .query_label(Label::new(PicoStr::intern("design-settings")).unwrap())
        .unwrap();
    let block = document
        .introspector()
        .query_label(Label::new(PicoStr::intern("design-body")).unwrap())
        .unwrap();
    let position =
        document.introspector().position(block.location().unwrap()).unwrap();
    assert!((position.point.y.to_pt() - 50.0 * 72.0 / 25.4).abs() < 1.0);
    assert!((position.point.x.to_pt() - 22.0 * 72.0 / 25.4).abs() < 1.0);
    let metadata = content.to_packed::<MetadataElem>().unwrap();
    let Value::Dict(settings) = &metadata.value else {
        panic!("expected settings dictionary")
    };
    assert_eq!(settings.get("size").unwrap(), &Value::Float(11.0));
    assert_eq!(settings.get("justify").unwrap(), &Value::Bool(false));
    let Value::Float(leading) = settings.get("leading").unwrap() else {
        panic!("expected an absolute leading value")
    };
    assert!((leading - 6.05).abs() < 0.01);
    let matches_family = |family: &Value| {
        matches!(family,
            Value::Str(family) if family.as_str().eq_ignore_ascii_case("DejaVu Sans Mono")
        )
    };
    let font = settings.get("font").unwrap();
    let selected = match font {
        Value::Array(families) => families.iter().any(matches_family),
        family => matches_family(family),
    };
    assert!(selected, "expected DejaVu Sans Mono, got {font:?}");
    assert!(
        (document.pages()[0].frame.width().to_pt() - 210.0 * 72.0 / 25.4).abs()
            < 0.01
    );
    world.set_main_text(
        &format!(
            concat!(
                "#import \"@writer/doc:{}\": writer-doc\n",
                "#show: writer-doc.with(size: 15pt, paper: \"us-letter\")\n",
                "#context assert(text.size == 15pt)\nExplicit overrides.\n",
            ),
            writer::typst::WRITER_DOC_VERSION
        ),
        SourceMap::default(),
    );
    let (explicit, _) = compile_document(world).into_result().unwrap();
    assert!((explicit.pages()[0].frame.width().to_pt() - 612.0).abs() < 0.01);
}

#[test]
fn unchanged_design_keeps_libraries_and_changes_rebuild_once() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("document.md");
    std::fs::write(&input, "Body.\n").unwrap();
    let config = directory.path().join("writer.toml");
    std::fs::write(
        &config,
        "[pdf]\nsystem_fonts = false\n[design.text]\nsize = '11pt'\n",
    )
    .unwrap();
    let opts = BuildOptions::new(input);
    let mut worlds = WorldSlot::new();
    let first = pipeline::prepare(&opts).unwrap();
    let count = worlds.get(Some(&first), &opts).unwrap().library_builds();
    std::fs::write(
        &config,
        concat!(
            "[html]\nstandalone = false\n[pdf]\nsystem_fonts = false\n",
            "[design.text]\nsize = '11pt'\n",
        ),
    )
    .unwrap();
    let unrelated = pipeline::prepare(&opts).unwrap();
    assert_eq!(
        worlds.get(Some(&unrelated), &opts).unwrap().library_builds(),
        count
    );
    std::fs::write(
        &config,
        "[pdf]\nsystem_fonts = false\n[design.text]\nsize = '12pt'\n",
    )
    .unwrap();
    let changed = pipeline::prepare(&opts).unwrap();
    assert_eq!(
        worlds.get(Some(&changed), &opts).unwrap().library_builds(),
        count + 1
    );
    assert_eq!(
        worlds.get(Some(&changed), &opts).unwrap().library_builds(),
        count + 1
    );
}

#[test]
fn pdf_check_reports_unknown_families_and_non_delegating_templates() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("writer.toml"),
        concat!(
            "[pdf]\nsystem_fonts = false\ntemplate = 'template.typ'\n",
            "[design.fonts]\nbody = 'Writer Unknown Font Family'\n",
        ),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("template.typ"),
        "#let writer-doc(..args, body) = body\n",
    )
    .unwrap();
    let input = directory.path().join("document.md");
    std::fs::write(&input, "Body.\n").unwrap();
    let mut opts = BuildOptions::new(input);
    opts.cli.targets = Some(vec![Target::Pdf]);
    let findings = writer::check::run(&opts, &Default::default()).unwrap();
    assert!(
        findings.iter().any(|finding| finding.code == "design-font-family")
    );
    assert!(findings.iter().any(|finding| finding.code == "design-shadowed"));
}

#[test]
fn known_display_case_family_is_available_without_files() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("writer.toml"),
        concat!(
            "[pdf]\nsystem_fonts = false\n[design.fonts]\n",
            "body = 'Libertinus Serif'\n",
        ),
    )
    .unwrap();
    let input = directory.path().join("document.md");
    std::fs::write(&input, "Body.\n").unwrap();
    let mut opts = BuildOptions::new(input);
    opts.cli.targets = Some(vec![Target::Pdf]);
    let findings = writer::check::run(&opts, &Default::default()).unwrap();
    assert!(
        !findings.iter().any(|finding| finding.code == "design-font-family")
    );
}

#[test]
fn native_typography_arguments_stay_with_the_compiler() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("writer.toml"),
        "[pdf]\nsystem_fonts = false\n",
    )
    .unwrap();
    let input = directory.path().join("native.typ");
    std::fs::write(
        &input,
        format!(
            concat!(
                "#import \"@writer/doc:{}\": writer-doc\n",
                "#show: writer-doc.with(size: 15pt, leading: 3pt,\n",
                "  font: \"Libertinus Serif\", margin: (x: 20mm, y: 30mm),\n",
                "  justify: false, text-fill: rgb(\"#123456\"),\n",
                "  design: (text: (size: \"99pt\",)))\n",
                "Native typography.\n",
            ),
            writer::typst::WRITER_DOC_VERSION
        ),
    )
    .unwrap();
    let opts = BuildOptions::new(input);
    let prepared = pipeline::prepare(&opts).unwrap();
    assert!(!prepared.diagnostics.has_errors(), "{:?}", prepared.diagnostics);
    assert!(!prepared.design.declared);
    assert!(!prepared.document.meta.vars.contains_key("size"));
    assert!(!prepared.document.meta.vars.contains_key("design"));
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    let (document, _) = compile_document(world).into_result().unwrap();
    assert_eq!(document.pages().len(), 1);
}
#[test]
fn listing_fonts_use_design_defaults_and_explicit_mono_font_overrides() {
    use typst::layout::{Frame, FrameItem};

    fn family(frame: &Frame, marker: &str) -> Option<String> {
        for (_, item) in frame.items() {
            match item {
                FrameItem::Group(group) => {
                    if let Some(family) = family(&group.frame, marker) {
                        return Some(family);
                    }
                }
                FrameItem::Text(run) if run.text.contains(marker) => {
                    return Some(run.font.info().family.clone());
                }
                _ => {}
            }
        }
        None
    }

    for (configured, explicit, expected) in [
        (None, None, "DejaVu Sans Mono"),
        (Some("Libertinus Serif"), None, "Libertinus Serif"),
        (
            Some("DejaVu Sans Mono"),
            Some("Libertinus Serif"),
            "Libertinus Serif",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let design = configured.map_or(String::new(), |family| {
            format!("[design.fonts]\nmono = {family:?}\n",)
        });
        std::fs::write(
            root.join("writer.toml"),
            format!("[pdf]\nsystem_fonts = false\n{design}",),
        )
        .unwrap();
        let args = explicit
            .map_or(String::new(), |family| format!("mono-font: {family:?}",));
        let input = root.join("document.typ");
        std::fs::write(&input, format!(
            concat!(
                "#import \"@writer/doc:{}\": *\n",
                "#show: writer-doc.with({args})\n",
                "#listing(numbered: true)[\n```text\nNUMBEREDMARKER\n```\n]\n",
                "#listing(highlight: \"1\")[\n```text\nHIGHLIGHTMARKER\n```\n]\n",
            ),
            writer::typst::WRITER_DOC_VERSION,
            args = args,
        )).unwrap();
        let opts = BuildOptions::new(input);
        let prepared = pipeline::prepare(&opts).unwrap();
        let mut worlds = WorldSlot::new();
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        let (document, _) = compile_document(world).into_result().unwrap();
        for marker in ["NUMBEREDMARKER", "HIGHLIGHTMARKER"] {
            assert_eq!(
                family(&document.pages()[0].frame, marker).as_deref(),
                Some(expected),
                "configured={configured:?}, explicit={explicit:?}, marker={marker}"
            );
        }
    }
}

#[test]
fn an_explicit_design_argument_beats_the_input_and_none_keeps_the_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let declared =
        design(serde_json::json!({"text": {"size": "14pt"}}), directory.path());
    assert!(declared.declared);
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    for (argument, expected) in
        [("auto", 14.0), ("none", 11.0), ("(text: (size: 20,))", 20.0)]
    {
        let input = directory.path().join("native.typ");
        std::fs::write(
            &input,
            format!(
                concat!(
                    "#import \"@writer/doc:{}\": writer-doc\n",
                    "#show: writer-doc.with(design: {})\n",
                    "#context [#metadata(text.size.pt()) <size>]\nSized.\n",
                ),
                writer::typst::WRITER_DOC_VERSION,
                argument
            ),
        )
        .unwrap();
        let mut world = WriterWorld::from_config_design(
            &config,
            &declared,
            &AssetRegistry::new(),
            directory.path(),
            MainFile::Path(input),
            Datetime::from_ymd(2026, 11, 9).unwrap(),
        )
        .unwrap();
        let (document, _) = compile_document(&mut world).into_result().unwrap();
        let content = document
            .introspector()
            .query_label(Label::new(PicoStr::intern("size")).unwrap())
            .unwrap();
        let metadata = content.to_packed::<MetadataElem>().unwrap();
        assert_eq!(
            metadata.value,
            Value::Float(expected),
            "design: {argument}"
        );
    }
}
