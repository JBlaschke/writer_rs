//! The shared design reaches HTML tokens, layout, context and copied fonts.

use std::fs;
use std::path::{Path, PathBuf};

use writer::assets::emit::OutputMode;
use writer::design::{Design, Layer, Origin, merge};
use writer::html::design::{styles_css, vocabulary_css};
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
    assert!(css[layout..].contains(concat!(
        ":where([data-writer-document]) p{margin-block:0 10.205pt}\n",
        "[data-writer-document]{text-align:start}\n",
    )));
    assert!(css[layout..].contains(
        "@page{size:A4;margin:141.732pt 62.362pt 107.717pt 62.362pt}"
    ));
    assert!(rendered.html.contains("id=\"writer-theme\""));
    insta::assert_snapshot!("letter_theme", css);
}

fn font_project() -> (tempfile::TempDir, BuildOptions, Prepared, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    let (font, info) = writer::typst::fonts::embedded()
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

fn design(table: serde_json::Value) -> Design {
    merge(&[Layer {
        table: table.as_object().unwrap().clone(),
        base_dir: Path::new(env!("CARGO_MANIFEST_DIR")),
        origin: Origin::FrontMatter,
    }])
    .unwrap()
}

/// One style for every element the vocabulary reaches in HTML, and three
/// (`Recipient`, `Closing`, `Note`) that only divs use.
fn vocabulary_design() -> Design {
    design(serde_json::json!({
        "text": {"size": "11pt"},
        "styles": {
            "Heading 1": {
                "size": "14pt", "weight": 700, "color": "#0a2f41",
                "align": "left", "space_before": "20pt",
                "space_after": "6pt", "keep_with_next": true,
            },
            "Heading 2": {"size": "12pt", "line_height": 1.2},
            "Heading 1 Unlisted": {"size": "18pt"},
            "TOC Heading": {"size": "16pt", "weight": 400},
            "toc 1": {"weight": 700, "space_before": "6pt"},
            "toc 2": {"italic": true},
            "toc 3": {"keep_together": false},
            "Caption": {
                "size": "9pt", "italic": true, "space_before": "4pt",
            },
            "Table Caption": {"weight": 700},
            "Image Caption": {"color": "#575757"},
            "Compact": {
                "size": "9pt", "align": "left", "space_before": "2pt",
                "space_after": "3pt",
            },
            "Title": {
                "size": "30pt", "weight": 400, "align": "left",
                "space_before": "40pt",
            },
            "Subtitle": {"size": "13pt"},
            "Author": {
                "size": "10pt", "align": "left", "space_before": "4pt",
                "space_after": "8pt",
            },
            "Date": {"size": "9pt", "space_after": "12pt"},
            "Recipient": {
                "line_height": "12.1pt", "space_after": "6mm",
                "keep_together": true,
            },
            "Closing": {
                "space_before": "6mm", "space_after": "8mm",
                "keep_with_next": true,
            },
            "Note": {"align": "center"},
        },
    }))
}

#[test]
fn vocabulary_styles_write_scoped_rules_in_a_fixed_order() {
    assert_eq!(
        vocabulary_css(&vocabulary_design()),
        concat!(
            ":where([data-writer-document]) h1:where(:not(.title)),:where([data-writer-document]) .references-title{font-size:14pt;color:#0a2f41;font-weight:700;text-align:left;margin-top:20pt;margin-bottom:6pt;break-after:avoid}\n",
            ":where([data-writer-document]) h2{font-size:12pt;line-height:1.2}\n",
            ":where([data-writer-document]) h1.unlisted,:where([data-writer-document]) h1.notoc,:where([data-writer-document]) .abstract h1{font-size:18pt}\n",
            ":where([data-writer-document]) p.toc-title{font-size:16pt;color:#0a2f41;font-weight:400;text-align:left;margin-top:20pt;margin-bottom:6pt;break-after:avoid}\n",
            ":where([data-writer-document]) nav.toc>ol>li>a{display:block;font-weight:700;margin-top:6pt}\n",
            ":where([data-writer-document]) nav.toc>ol,:where([data-writer-document]) nav.toc>ol>li{margin-top:0}\n",
            ":where([data-writer-document]) nav.toc>ol>li>ol>li>a{display:block;font-style:italic}\n",
            ":where([data-writer-document]) figcaption{font-size:9pt;font-style:italic}\n",
            ":where([data-writer-document]) .kind-table>figcaption{font-weight:700}\n",
            ":where([data-writer-document]) .kind-image>figcaption{color:#575757}\n",
            ":where([data-writer-document]) th,:where([data-writer-document]) td{font-size:9pt;text-align:left;padding-top:2pt;padding-bottom:3pt}\n",
            ":where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:\"\\200b\"}\n",
            ":where([data-writer-document]) .title{font-size:30pt;font-weight:400;text-align:left;margin-top:40pt}\n",
            ":where([data-writer-document]) p.subtitle{font-size:13pt;text-align:left}\n",
            ":where([data-writer-document]) p.author{font-size:10pt;text-align:left;margin-top:0;margin-bottom:0}\n",
            ":where([data-writer-document]) .authors{margin-top:4pt;margin-bottom:8pt;justify-content:flex-start}\n",
            ":where([data-writer-document]) p.date{font-size:9pt;text-align:left;margin-bottom:12pt}\n",
        )
    );
}

#[test]
fn the_contents_title_lays_its_keys_over_heading_1s() {
    let toc = design(serde_json::json!({
        "styles": {"TOC Heading": {"size": "16pt"}}
    }));
    assert_eq!(
        vocabulary_css(&toc),
        concat!(
            ":where([data-writer-document]) p.toc-title{font-size:16pt}\n",
            ":where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:\"\\200b\"}\n",
        )
    );
    let heading = design(serde_json::json!({
        "styles": {"Heading 1": {"size": "14pt", "space_before": "20pt"}}
    }));
    assert_eq!(
        vocabulary_css(&heading),
        concat!(
            ":where([data-writer-document]) h1:where(:not(.title)),:where([data-writer-document]) .references-title{font-size:14pt;margin-top:20pt}\n",
            ":where([data-writer-document]) p.toc-title{font-size:14pt;margin-top:20pt}\n",
            ":where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:\"\\200b\"}\n",
        )
    );
}

#[test]
fn a_design_without_vocabulary_styles_writes_the_empty_cell_rule_alone() {
    let plain = design(serde_json::json!({"text": {"size": "11pt"}}));
    assert_eq!(
        vocabulary_css(&plain),
        ":where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:\"\\200b\"}\n"
    );
}

#[test]
fn vocabulary_names_match_by_key_and_justified_authors_keep_their_row() {
    let named = design(serde_json::json!({
        "styles": {
            "heading-2": {"size": "12pt"},
            "Author": {"align": "justify"},
        }
    }));
    assert_eq!(
        vocabulary_css(&named),
        concat!(
            ":where([data-writer-document]) h2{font-size:12pt}\n",
            ":where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:\"\\200b\"}\n",
            ":where([data-writer-document]) p.author{text-align:justify}\n",
        )
    );
}

#[test]
fn a_divs_spaces_replace_its_first_and_last_childs_margins() {
    let css = styles_css(&vocabulary_design());
    let has = |expected: &str| css.lines().any(|line| line == expected);
    assert!(has(
        "[data-style=\"Closing\"]{margin-top:17.008pt;margin-bottom:22.677pt;break-after:avoid}"
    ));
    assert!(has("[data-style=\"Closing\"]>:first-child{margin-top:0}"));
    assert!(has("[data-style=\"Closing\"]>:last-child{margin-bottom:0}"));
    assert!(has("[data-style=\"Recipient\"]>:last-child{margin-bottom:0}"));
    assert!(has("[data-style=\"Caption\"]>:first-child{margin-top:0}"));
    assert!(has("[data-style=\"Note\"]{text-align:center}"));
    assert!(has("[data-style=\"Note\"] p{text-align:inherit}"));
    assert!(!has("[data-style=\"Recipient\"]>:first-child{margin-top:0}"));
    assert!(!has("[data-style=\"Caption\"]>:last-child{margin-bottom:0}"));
    assert!(!css.contains("[data-style=\"Note\"]>"));
    assert!(!css.contains("[data-style=\"Subtitle\"]>"));
}

#[test]
fn vocabulary_rules_stand_between_named_styles_and_furniture() {
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
    let css = &css[css.find("/* design: layout */").unwrap()..];
    let named = css
        .find("[data-style=\"Signature\"]>:last-child{margin-bottom:0}")
        .unwrap();
    let cell =
        css.find(":where([data-writer-document]) td:empty::before,").unwrap();
    let date = css
        .find(
            ":where([data-writer-document]) p.date{font-size:10pt;margin-bottom:8.504pt}",
        )
        .unwrap();
    let furniture = css.find(".w-furniture{").unwrap();
    assert!(named < cell && cell < date && date < furniture);
}

#[test]
fn title_block_parts_follow_the_titles_alignment() {
    let aligned = design(serde_json::json!({
        "styles": {
            "Title": {"align": "right"},
            "Date": {"align": "center"},
        }
    }));
    assert_eq!(
        vocabulary_css(&aligned),
        concat!(
            ":where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:\"\\200b\"}\n",
            ":where([data-writer-document]) .title{text-align:right}\n",
            ":where([data-writer-document]) p.subtitle{text-align:right}\n",
            ":where([data-writer-document]) p.author{text-align:right}\n",
            ":where([data-writer-document]) .authors{justify-content:flex-end}\n",
            ":where([data-writer-document]) p.date{text-align:center}\n",
        )
    );
}

fn layout(design: &Design) -> String {
    let mut registry = writer::assets::registry::AssetRegistry::new();
    let mut targets =
        writer::html::styles::Targets::Placeholders(&mut registry);
    writer::html::design::layout_css(design, &mut targets).unwrap()
}

/// A selector's specificity as (ids, classes, elements); `:where()` adds
/// none. Enough for the selectors the design and the default theme write.
fn specificity(selector: &str) -> (u32, u32, u32) {
    let chars: Vec<char> = selector.chars().collect();
    let name = |mut at: usize| {
        while at < chars.len()
            && (chars[at].is_alphanumeric() || chars[at] == '-')
        {
            at += 1;
        }
        at
    };
    let group = |mut at: usize| {
        let mut depth = 0;
        while at < chars.len() {
            match chars[at] {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                _ => {}
            }
            at += 1;
            if depth == 0 {
                break;
            }
        }
        at
    };
    let (mut ids, mut classes, mut elements) = (0, 0, 0);
    let mut at = 0;
    while at < chars.len() {
        at = match chars[at] {
            '#' => {
                ids += 1;
                name(at + 1)
            }
            '.' => {
                classes += 1;
                name(at + 1)
            }
            '[' => {
                classes += 1;
                group(at)
            }
            ':' if chars.get(at + 1) == Some(&':') => {
                elements += 1;
                name(at + 2)
            }
            ':' => {
                let end = name(at + 1);
                let pseudo: String = chars[at + 1..end].iter().collect();
                if pseudo != "where" {
                    classes += 1;
                }
                if chars.get(end) == Some(&'(') {
                    group(end)
                } else {
                    end
                }
            }
            c if c.is_alphabetic() => {
                elements += 1;
                name(at)
            }
            _ => at + 1,
        };
    }
    (ids, classes, elements)
}

/// The selectors of a rule, split at the commas outside parentheses.
fn selectors(rule: &str) -> Vec<&str> {
    let list = &rule[..rule.find('{').unwrap()];
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (at, c) in list.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&list[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&list[start..]);
    parts
}

#[test]
fn the_paragraph_margin_is_scoped_and_the_alignment_inherited() {
    let both = design(serde_json::json!({
        "text": {"paragraph_spacing": "6pt", "justify": true}
    }));
    let css = layout(&both);
    assert!(css.starts_with(concat!(
        "/* design: layout */\n",
        ":where([data-writer-document]) p{margin-block:0 6pt}\n",
        "[data-writer-document]{text-align:justify}\n",
    )));
    // No rule of the design's own gives every paragraph an alignment.
    assert!(!css.contains("[data-writer-document] p{"));
    assert_eq!(css.matches("text-align").count(), 1);

    let start =
        layout(&design(serde_json::json!({"text": {"justify": false}})));
    assert!(start.contains("\n[data-writer-document]{text-align:start}\n"));
    assert!(!start.contains("margin-block"));

    let spacing = layout(&design(serde_json::json!({
        "text": {"paragraph_spacing": "3.6mm"}
    })));
    assert!(spacing.contains(
        "\n:where([data-writer-document]) p{margin-block:0 10.205pt}\n"
    ));
    assert!(!spacing.contains("text-align"));
}

#[test]
fn the_themes_class_rules_outrank_the_paragraph_margin() {
    let css = layout(&design(serde_json::json!({
        "text": {"paragraph_spacing": "6pt"}
    })));
    let rule = css.lines().nth(1).unwrap();
    let margin = specificity(selectors(rule)[0]);
    assert_eq!(margin, (0, 0, 1));
    // It comes after the theme, so it replaces the theme's `p` margin.
    assert_eq!(specificity("p"), margin);
    // The default theme's own margins for these paragraphs win over it.
    let theme = include_str!("../assets/themes/default/styles/theme.css");
    for selector in [
        "blockquote > :last-child", ".subtitle", ".author", ".date",
        ".abstract > :last-child", "th > :last-child", "td > :last-child",
        ".callout-title", ".callout > :last-child", ".toc-title",
    ] {
        assert!(theme.contains(selector), "{selector}");
        assert!(specificity(selector) > margin, "{selector}");
    }
}

#[test]
fn vocabulary_rules_follow_the_paragraph_rule_and_outrank_it() {
    let mut full = vocabulary_design();
    full.text.paragraph_spacing = Some(writer::design::Pt(6.0));
    full.text.justify = Some(true);
    let css = layout(&full);
    let margin = ":where([data-writer-document]) p{margin-block:0 6pt}\n";
    let paragraph = css.find(margin).unwrap();
    let vocabulary = vocabulary_css(&full);
    assert!(paragraph + margin.len() <= css.find(&vocabulary).unwrap());
    let floor = specificity(selectors(margin)[0]);
    for rule in vocabulary.lines() {
        for selector in selectors(rule) {
            assert!(specificity(selector) >= floor, "{selector}");
        }
    }
}
