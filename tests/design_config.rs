use std::path::Path;

use serde_json::json;
use writer::design::{
    Align, BackgroundPages, Fit, Layer, Origin, PathMode, VerticalAlign, merge,
};
use writer::pipeline::{self, BuildOptions, ChangeKind, InputRole};

fn layer(value: serde_json::Value, base: &Path) -> Layer<'_> {
    Layer {
        table: value.as_object().unwrap().clone(),
        base_dir: base,
        origin: Origin::FrontMatter,
    }
}

#[test]
fn letter_resolves_to_shared_json() {
    let input = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/design/letter/letter.md");
    let prepared = pipeline::prepare(&BuildOptions::new(input)).unwrap();
    let mut value = prepared.design.to_json(PathMode::Real);
    for slot in ["header", "footer"] {
        for page in ["first", "rest"] {
            if value["page"][slot][page]["image"].is_object() {
                value["page"][slot][page]["image"]["path"] =
                    json!("<theme>/assets/logo.png");
            }
        }
    }
    insta::assert_json_snapshot!("design_letter", value);
    assert_eq!(prepared.design.text.size.unwrap().0, 11.0);
    assert!(prepared.design.page.header.is_some());
    assert!(prepared.design.styles.contains_key("Recipient"));
}

#[test]
fn project_and_front_matter_preserve_independent_path_origins() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("writer.toml"), concat!(
        "[design.fonts]\nbody = { family = 'Body', files = ['body.ttf'] }\n",
        "[design.text]\nsize = '11pt'\n",
    )).unwrap();
    std::fs::write(dir.path().join("body.ttf"), "fixture").unwrap();
    std::fs::write(dir.path().join("docs/logo.png"), "fixture").unwrap();
    let input = dir.path().join("docs/doc.md");
    std::fs::write(
        &input,
        concat!(
            "---\ndesign:\n  text:\n    size: 12pt\n",
            "  page:\n    header:\n      image: logo.png\n---\n# Test\n",
        ),
    )
    .unwrap();
    let prepared = pipeline::prepare(&BuildOptions::new(input)).unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(prepared.design.text.size.unwrap().0, 12.0);
    assert_eq!(
        prepared.design.fonts.body.as_ref().unwrap().files,
        [root.join("body.ttf")]
    );
    assert!(
        prepared
            .design
            .referenced_files()
            .contains(&root.join("docs/logo.png"))
    );
    let inputs = pipeline::watch_inputs(&prepared);
    assert!(inputs.contains(&(root.join("docs/logo.png"), InputRole::Design)));
    let set = writer::serve::watch::watch_set(&inputs, &[], None);
    assert_eq!(
        writer::serve::watch::classify(&[root.join("docs/logo.png")], &set,),
        ChangeKind::Content
    );
}

#[test]
fn lists_replace_and_empty_furniture_removes_inherited_content() {
    let root = tempfile::tempdir().unwrap();
    let canonical = std::fs::canonicalize(root.path()).unwrap();
    let base = canonical.as_path();
    let design = merge(&[
        layer(
            json!({"fonts":{"body":{"family":"A","files":["a.ttf"]}},
            "page":{"footer":{"lines":["Inherited"]}}}),
            base,
        ),
        layer(
            json!({"fonts":{"body":{"files":["b.ttf"]}},
            "page":{"footer":{}}}),
            base,
        ),
    ])
    .unwrap();
    assert_eq!(design.fonts.body.unwrap().files, [base.join("b.ttf")]);
    assert_eq!(design.page.footer.unwrap().first, None);
}

#[test]
fn undeclared_design_is_null_and_does_not_add_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("plain.md");
    std::fs::write(&input, "# Plain\n").unwrap();
    let prepared = pipeline::prepare(&BuildOptions::new(input)).unwrap();
    assert_eq!(prepared.design.to_json(PathMode::Real), json!(null));
    assert!(prepared.design.referenced_files().is_empty());
}

#[test]
fn theme_paths_cannot_escape_the_theme() {
    let dir = tempfile::tempdir().unwrap();
    let outside = Layer {
        table: json!({"page":{"header":{"image":"../outside.png"}}})
            .as_object()
            .unwrap()
            .clone(),
        base_dir: dir.path(),
        origin: Origin::Theme { file: dir.path().join("theme.toml") },
    };
    let error = merge(&[outside]).unwrap_err().to_string();
    assert!(error.contains("theme-link-outside"), "{error}");
}

#[test]
fn invalid_values_name_the_key_and_offer_unknown_key_hints() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/design/errors");
    for (fixture, expected) in [
        ("bad-unit", "design.text.size: expected a length with a unit"),
        ("unknown-key", "design.text.sise: unknown key (did you mean size?)"),
        (
            "colliding-styles",
            "\"Inside Address\" and \"inside-address\" are the same style name",
        ),
        ("nested-first", "design.page.header.first.rest: unknown key"),
    ] {
        let value: toml::Table =
            std::fs::read_to_string(root.join(format!("{fixture}.toml")))
                .unwrap()
                .parse()
                .unwrap();
        let value = serde_json::to_value(value).unwrap();
        let error = merge(&[layer(value["design"].clone(), &root)])
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{fixture}: {error}");
    }
}

#[test]
fn missing_resources_are_check_findings_and_build_errors() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("document.md");
    std::fs::write(&input, "# Missing\n").unwrap();
    std::fs::write(
        root.path().join("writer.toml"),
        "[design.fonts]\nbody = {family = 'Missing', files = ['missing.ttf']}\n",
    ).unwrap();
    let opts = BuildOptions::new(input);
    let error = pipeline::prepare(&opts).unwrap_err();
    assert!(error.error.to_string().contains("design font file not found:"));
    let findings = writer::check::run(&opts, &Default::default()).unwrap();
    assert!(findings.iter().any(|d| d.code == "design-font-file"));
}

#[test]
fn unavailable_theme_warns_only_for_non_html_targets() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("document.md");
    std::fs::write(&input, "# Missing theme\n").unwrap();
    std::fs::write(
        root.path().join("writer.toml"),
        "[html]\ntheme = 'missing-design-theme'\n",
    )
    .unwrap();
    let mut opts = BuildOptions::new(input);
    opts.mode = pipeline::Mode::Check;
    let html = pipeline::prepare(&opts).unwrap();
    assert!(
        !html.diagnostics.iter().any(|d| d.code == "design-theme-unavailable")
    );
    opts.cli.targets = Some(vec![writer::config::Target::Pdf]);
    let pdf = pipeline::prepare(&opts).unwrap();
    assert!(
        pdf.diagnostics.iter().any(|d| d.code == "design-theme-unavailable")
    );
}

#[test]
fn page_background_resolves_shorthand_fit_align_and_pages() {
    let root = tempfile::tempdir().unwrap();
    let canonical = std::fs::canonicalize(root.path()).unwrap();
    let base = canonical.as_path();
    let cover = json!({"page":{"background":{"image":"cover.jpg"}}});
    // A path alone is a centred cover on the first page.
    let design = merge(&[layer(cover.clone(), base)]).unwrap();
    assert_eq!(
        design.to_json(PathMode::Real)["page"]["background"],
        json!({
            "image": {
                "path": base.join("cover.jpg"),
                "fit": "cover",
                "align": {"horizontal": "center", "vertical": "center"},
            },
            "pages": "first",
        })
    );
    assert!(design.referenced_files().contains(&base.join("cover.jpg")));
    for (align, horizontal, vertical) in [
        ("bottom right", Align::Right, VerticalAlign::Bottom),
        ("right bottom", Align::Right, VerticalAlign::Bottom),
        ("left", Align::Left, VerticalAlign::Center),
        ("top", Align::Center, VerticalAlign::Top),
        ("center left", Align::Left, VerticalAlign::Center),
    ] {
        let design = merge(&[layer(
            json!({"page":{"background":{
                "image":{"src":"cover.jpg","fit":"contain","align":align},
                "pages":"all",
            }}}),
            base,
        )])
        .unwrap();
        let background = design.page.background.unwrap();
        assert_eq!(background.image.path, base.join("cover.jpg"));
        assert_eq!(background.image.fit, Fit::Contain);
        assert_eq!(background.image.align.horizontal, horizontal, "{align}");
        assert_eq!(background.image.align.vertical, vertical, "{align}");
        assert_eq!(background.pages, BackgroundPages::All);
    }
    // An empty table removes an inherited background, as an empty slot does.
    let design = merge(&[
        layer(cover, base),
        layer(json!({"page":{"background":{}}}), base),
    ])
    .unwrap();
    assert!(design.page.background.is_none());
    assert!(design.referenced_files().is_empty());
    for (background, expected) in [
        (
            json!({"image":{"src":"c.jpg","fit":"fill"}}),
            "design.page.background.image.fit: expected cover, contain or stretch, got \"fill\"",
        ),
        (
            json!({"image":"c.jpg","pages":"odd"}),
            "design.page.background.pages: expected first or all, got \"odd\"",
        ),
        (
            json!({"image":{"src":"c.jpg","align":"left right"}}),
            "design.page.background.image.align: expected one or two of top, bottom, left, right, center, got \"left right\"",
        ),
        (
            json!({"image":{"src":"c.jpg","align":"middle"}}),
            "design.page.background.image.align: expected one or two of",
        ),
        (
            json!({"pages":"all"}),
            "design.page.background.image: expected an image path or table",
        ),
        (
            json!({"image":{"fit":"cover"}}),
            "design.page.background.image.src: expected an image path",
        ),
        (
            json!({"image":"c.jpg","opacity":0.5}),
            "design.page.background.opacity: unknown key",
        ),
        (
            json!({"image":{"path":"c.jpg"}}),
            "design.page.background.image.path: unknown key",
        ),
    ] {
        let error =
            merge(&[layer(json!({"page":{"background":background}}), base)])
                .unwrap_err()
                .to_string();
        assert!(error.contains(expected), "{error}");
    }
}
