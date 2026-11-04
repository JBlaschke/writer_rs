//! The shared design reaches HTML tokens, layout, context and copied fonts.

use std::fs;
use std::path::{Path, PathBuf};

use writer::assets::emit::OutputMode;
use writer::html::{self, BuildInfo, RenderInput, RenderedDocument};
use writer::pipeline::{self, BuildOptions, Prepared};

fn render(
    prepared: &Prepared, opts: &BuildOptions, output: OutputMode,
    build: BuildInfo,
) -> (RenderedDocument, writer::assets::registry::AssetRegistry) {
    let theme = pipeline::resolve_theme(prepared, opts).unwrap();
    let mut assets = prepared.assets.clone();
    let result = html::render(RenderInput {
        document: &prepared.document,
        assets: &mut assets,
        theme: &theme,
        config: &prepared.config,
        design: &prepared.design,
        output,
        build,
        math: &mut html::emit::KatexOnly::default(),
    })
    .unwrap();
    (result, assets)
}

#[test]
fn letter_tokens_precede_theme_entries_and_layout_follows_them() {
    let input = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/design/letter/letter.md");
    let opts = BuildOptions::new(input);
    let prepared = pipeline::prepare(&opts).unwrap();
    let (rendered, _) = render(
        &prepared,
        &opts,
        OutputMode::Standalone,
        BuildInfo::build(true),
    );
    let css = &rendered.theme_css;
    assert!(css.starts_with("/* design: tokens */\n"));
    assert!(css.contains(
        ":root{--w-font-body:\"Source Sans 3\",system-ui,sans-serif;--w-font-heading:\"Source Sans 3\",system-ui,sans-serif;--w-font-size:11pt;--w-line-height:1.35;--w-text:#242424;--w-muted:#575757;--w-accent:#a96d4b;--w-link:#a96d4b}"
    ));
    let entries = css.find("/* @default/").unwrap();
    let layout = css.find("/* design: layout */").unwrap();
    assert!(entries < layout);
    // Built-in base tokens are a lower-priority baseline, while the theme's
    // dark palette remains unlayered and explicit custom entries can win.
    let layer = css.find("@layer writer-default {\n:root {").unwrap();
    let dark = css.find("@media (prefers-color-scheme: dark)").unwrap();
    assert!(entries < layer && layer < dark && dark < layout);
    assert!(css[layer..dark].ends_with("}\n\n"));
    assert!(css[layout..].contains(
        "[data-writer-document] p{margin-block:0 10.205pt;text-align:start}"
    ));
    assert!(css[layout..].contains(
        "@page{size:A4;margin:141.732pt 62.362pt 107.717pt 62.362pt}"
    ));
    assert!(rendered.html.contains("id=\"writer-theme\""));
    insta::assert_snapshot!("letter_theme", css);
}

fn font_project() -> (tempfile::TempDir, BuildOptions, Prepared, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    let (font, info) = typst_kit::fonts::embedded()
        .find(|(_, info)| {
            info.variant.style == typst::text::FontStyle::Normal
                && info.variant.weight.to_number() == 400
        })
        .unwrap();
    let bytes = font.data().as_slice().to_vec();
    fs::write(root.path().join("body.ttf"), &bytes).unwrap();
    let config = serde_json::json!({
        "design": {
            "fonts": {"body": {"family": info.family, "files": ["body.ttf"]}},
            "text": {"size": "12pt"},
        },
        "pdf": {"system_fonts": false},
    });
    fs::write(
        root.path().join("writer.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    fs::write(root.path().join("document.md"), "# Font document\n").unwrap();
    let opts = BuildOptions::new(root.path().join("document.md"));
    let prepared = pipeline::prepare(&opts).unwrap();
    (root, opts, prepared, bytes)
}

#[test]
fn font_faces_use_file_descriptors_and_copy_bytes_in_site_mode() {
    let (root, opts, prepared, bytes) = font_project();
    let (rendered, _) = render(
        &prepared,
        &opts,
        OutputMode::Site {
            assets_dir: root.path().join("site/assets"),
            url_prefix: "assets/".into(),
        },
        BuildInfo::build(false),
    );
    let css = &rendered.theme_css;
    assert!(css.contains("@font-face{font-family:"));
    assert!(css.contains(" format(\"truetype\");font-weight:400;font-style:normal;font-display:swap}"));
    assert!(css.contains("src:url(body-"));
    assert!(rendered.files.iter().any(|file| file.bytes == bytes));
    assert!(!css.contains("asset:"));
}

#[test]
fn preview_fonts_are_versioned_registered_document_assets() {
    let (_root, opts, prepared, bytes) = font_project();
    let (rendered, assets) =
        render(&prepared, &opts, OutputMode::Serve, BuildInfo::serve(1));
    let font = prepared.design.fonts.body.as_ref().unwrap();
    let mut expected = writer::assets::registry::AssetRegistry::new();
    let id = expected.register_file(&font.files[0]);
    assert_eq!(assets.read(&id).unwrap(), bytes);
    assert!(
        rendered.theme_css.contains(&format!("/__writer/doc-asset/{id}?v="))
    );
}

#[test]
fn dump_context_exposes_the_same_resolved_design() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input = root.join("tests/fixtures/design/letter/letter.md");
    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"))
        .current_dir(&root)
        .args(["dump", "--context"])
        .arg(&input)
        .assert()
        .success()
        .get_output()
        .clone();
    let context: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap();
    let prepared = pipeline::prepare(&BuildOptions::new(input)).unwrap();
    assert_eq!(
        context["theme"]["design"],
        prepared.design.to_json(writer::design::PathMode::Real)
    );
}

#[test]
fn explicit_theme_tokens_override_the_design_after_hoisted_imports() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("theme")).unwrap();
    fs::write(
        root.path().join("theme/theme.toml"),
        "name = 'override'\nextends = 'default'\n\
         [styles]\nentries = ['custom.css']\n\
         [design.colors]\ntext = '#123456'\n",
    )
    .unwrap();
    fs::write(
        root.path().join("theme/custom.css"),
        "@import url('https://example.com/fonts.css');\n:root{--w-text:#abcdef}\n",
    )
    .unwrap();
    fs::write(root.path().join("writer.toml"), "[html]\ntheme='theme'\n")
        .unwrap();
    fs::write(root.path().join("document.md"), "# Document\n").unwrap();
    let opts = BuildOptions::new(root.path().join("document.md"));
    let prepared = pipeline::prepare(&opts).unwrap();
    let (rendered, _) = render(
        &prepared,
        &opts,
        OutputMode::Standalone,
        BuildInfo::build(true),
    );
    let css = rendered.theme_css;
    let import = css.find("@import").unwrap();
    let design = css.find(":root{--w-text:#123456}").unwrap();
    let defaults = css.find("@layer writer-default").unwrap();
    let explicit = css.find(":root{--w-text:#abcdef}").unwrap();
    assert!(import < design && design < defaults && defaults < explicit);
    assert!(css[explicit..].starts_with(":root{--w-text:#abcdef}"));
}

#[test]
fn an_untouched_ejected_default_theme_keeps_the_design_baseline() {
    let (root, _, prepared, _) = font_project();
    let mut writer = assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"));
    writer
        .current_dir(root.path())
        .args(["eject", "theme", "portable", "--design", "document.md"])
        .assert()
        .success();
    fs::write(
        root.path().join("writer.toml"),
        "[html]\ntheme='portable'\n[pdf]\nsystem_fonts=false\n",
    )
    .unwrap();
    let build = || {
        assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"))
            .current_dir(root.path())
            .args(["build", "document.md", "--to", "html", "-o", "result.html"])
            .assert()
            .success();
        fs::read_to_string(root.path().join("result.html")).unwrap()
    };
    let html = build();
    assert!(html.contains("@layer writer-default {"));
    assert!(html.contains("--w-font-size:12pt"));
    let family = &prepared.design.fonts.body.as_ref().unwrap().family;
    assert!(html.contains(&format!("--w-font-body:\"{family}\"")));
    let css_path = root.path().join("portable/styles/theme.css");
    let mut css = fs::read_to_string(&css_path).unwrap();
    css.push_str("\n:root{--w-font-size:16pt}\n");
    fs::write(&css_path, css).unwrap();
    let customized = build();
    assert!(!customized.contains("@layer writer-default {"));
    assert!(customized.contains(":root{--w-font-size:16pt}"));
}

#[test]
fn design_colors_follow_the_default_print_sheet_until_it_is_edited() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("writer.toml"),
        "[design.colors]\ntext = '#123456'\nbackground = '#fffefb'\n\
         accent = '#a96d4b'\n[pdf]\nsystem_fonts = false\n",
    )
    .unwrap();
    fs::write(root.path().join("document.md"), "# Document\n").unwrap();
    let opts = BuildOptions::new(root.path().join("document.md"));
    let prepared = pipeline::prepare(&opts).unwrap();
    let (rendered, _) = render(
        &prepared,
        &opts,
        OutputMode::Standalone,
        BuildInfo::build(true),
    );
    let css = rendered.theme_css;
    // The default print sheet sets its own light palette and black text;
    // the declared colours follow it, before the design's layout rules.
    let print = css.find("/* @default/styles/print.css */").unwrap();
    let colors = css.find("/* design: print colors */\n").unwrap();
    let layout = css.find("/* design: layout */").unwrap();
    assert!(print < colors && colors < layout);
    assert!(!css[print + 1..colors].contains("/* @"));
    let block = css[colors..].lines().nth(1).unwrap();
    assert_eq!(
        block,
        "@media print{:root{--w-text:#123456;--w-bg:#fffefb;\
         --w-accent:#a96d4b}body{background:var(--w-bg);\
         color:var(--w-text)}}"
    );

    // Without declared colours the print sheet keeps its own palette.
    fs::write(
        root.path().join("writer.toml"),
        "[design.text]\nsize = '12pt'\n[pdf]\nsystem_fonts = false\n",
    )
    .unwrap();
    let prepared = pipeline::prepare(&opts).unwrap();
    let (rendered, _) = render(
        &prepared,
        &opts,
        OutputMode::Standalone,
        BuildInfo::build(true),
    );
    assert!(!rendered.theme_css.contains("design: print colors"));
}

#[test]
fn an_edited_ejected_print_sheet_keeps_its_own_print_colors() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("writer.toml"),
        "[design.colors]\ntext = '#123456'\n[pdf]\nsystem_fonts = false\n",
    )
    .unwrap();
    fs::write(root.path().join("document.md"), "# Document\n").unwrap();
    assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"))
        .current_dir(root.path())
        .args(["eject", "theme", "portable", "--design", "document.md"])
        .assert()
        .success();
    let mut config =
        fs::read_to_string(root.path().join("writer.toml")).unwrap();
    config.push_str("[html]\ntheme = 'portable'\n");
    fs::write(root.path().join("writer.toml"), config).unwrap();
    let build = || {
        assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"))
            .current_dir(root.path())
            .args(["build", "document.md", "--to", "html", "-o", "result.html"])
            .assert()
            .success();
        fs::read_to_string(root.path().join("result.html")).unwrap()
    };
    // An untouched copy of the default print sheet is treated as the default.
    assert!(build().contains("@media print{:root{--w-text:#123456}"));
    let print = root.path().join("portable/styles/print.css");
    let mut css = fs::read_to_string(&print).unwrap();
    css.push_str("\n@media print{body{color:#000}}\n");
    fs::write(&print, css).unwrap();
    let edited = build();
    assert!(!edited.contains("design: print colors"));
    assert!(edited.contains("@media print{body{color:#000}}"));
}
