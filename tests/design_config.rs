use std::path::Path;

use serde_json::json;
use writer::design::{
    Align, BackgroundPages, ColumnStart, Columns, Fit, Frame, FrameAnchor,
    Layer, Origin, PathMode, Pt, VerticalAlign, merge,
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
fn image_gap_defaults_to_the_paragraph_spacing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/design/letter/theme");
    let gap = |design: serde_json::Value| {
        let design = merge(&[layer(design, &root)]).unwrap();
        let header = design.page.header.unwrap().first.unwrap();
        header.image.unwrap().gap.map(|gap| gap.0)
    };
    let spacing = 3.6 * 72.0 / 25.4;
    // The string form and a table without `gap` take the paragraph spacing.
    for image in [
        json!("assets/logo.png"),
        json!({"src": "assets/logo.png", "width": "50mm"}),
    ] {
        let gap = gap(json!({
            "text": {"paragraph_spacing": "3.6mm"},
            "page": {"header": {"image": image}},
        }))
        .unwrap();
        assert!((gap - spacing).abs() < 1e-9, "{gap}");
    }
    let explicit = gap(json!({
        "text": {"paragraph_spacing": "3.6mm"},
        "page": {"header": {"image": {"src": "assets/logo.png", "gap": "3mm"}}},
    }))
    .unwrap();
    assert!((explicit - 3.0 * 72.0 / 25.4).abs() < 1e-9, "{explicit}");
    // Without paragraph spacing the backends keep their own.
    assert_eq!(
        gap(json!({"page": {"header": {"image": "assets/logo.png"}}})),
        None
    );
    let error = merge(&[layer(
        json!({"page": {"header": {"image": {"src": "assets/logo.png", "gap": "3"}}}}),
        &root,
    )])
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("design.page.header.image.gap: expected a length"),
        "{error}"
    );
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

#[test]
fn page_columns_resolve_a_count_or_a_table() {
    let base = Path::new("/");
    let page = |columns| json!({"page": {"columns": columns}});
    // A bare count keeps the backend's gap and starts at the body.
    let design = merge(&[layer(page(json!(2)), base)]).unwrap();
    assert_eq!(
        design.page.columns,
        Some(Columns { count: 2, gap: None, start: ColumnStart::Body })
    );
    let table = json!({"count": 3, "gap": "7mm", "start": "after-rule"});
    let design = merge(&[layer(page(table), base)]).unwrap();
    assert_eq!(
        design.page.columns,
        Some(Columns {
            count: 3,
            gap: Some(Pt(7.0 * 72.0 / 25.4)),
            start: ColumnStart::AfterRule,
        })
    );
    assert_eq!(
        design.to_json(PathMode::Real)["page"]["columns"],
        json!({"count": 3, "gap": 19.843, "start": "after-rule"})
    );
    // Tables merge key by key, so a document can change only the gap.
    let design = merge(&[
        layer(page(json!({"count": 2, "start": "after-rule"})), base),
        layer(page(json!({"gap": "5mm"})), base),
    ])
    .unwrap();
    let columns = design.page.columns.unwrap();
    assert_eq!((columns.count, columns.start), (2, ColumnStart::AfterRule));
    assert_eq!(columns.gap, Some(Pt(5.0 * 72.0 / 25.4)));
    // Unset columns stay null in the shared JSON.
    let design =
        merge(&[layer(json!({"page": {"paper": "a4"}}), base)]).unwrap();
    assert_eq!(design.to_json(PathMode::Real)["page"]["columns"], json!(null));
    for (columns, expected) in [
        (
            json!(0),
            "design.page.columns: expected a column count from 1 to 16, got 0",
        ),
        (
            json!(17),
            "design.page.columns: expected a column count from 1 to 16, got 17",
        ),
        (
            json!("2"),
            "design.page.columns: expected a column count from 1 to 16, got \"2\"",
        ),
        (
            json!({"gap": "7mm"}),
            "design.page.columns.count: expected a column count",
        ),
        (
            json!({"count": 2.5}),
            "design.page.columns.count: expected a column count from 1 to 16",
        ),
        (
            json!({"count": 2, "start": "rule"}),
            "design.page.columns.start: expected body or after-rule, got \"rule\"",
        ),
        (
            json!({"count": 2, "gap": "7"}),
            "design.page.columns.gap: expected a length",
        ),
        (
            json!({"count": 2, "span": true}),
            "design.page.columns.span: unknown key",
        ),
    ] {
        let error =
            merge(&[layer(page(columns), base)]).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn style_frames_resolve_with_defaults_and_signed_offsets() {
    let base = Path::new("/");
    let style = |frame| json!({"styles": {"Badge": {"frame": frame}}});
    // An empty table is a left-aligned frame at the div's own place.
    let design = merge(&[layer(style(json!({})), base)]).unwrap();
    assert_eq!(
        design.styles["Badge"].frame,
        Some(Frame {
            width: None,
            align: Align::Left,
            gap: None,
            anchor: FrameAnchor::Div,
            offset: Pt(0.0),
        })
    );
    let frame = json!({
        "width": "16mm", "align": "right", "gap": "6mm", "anchor": "title",
        "offset": "-9mm",
    });
    let design = merge(&[layer(style(frame), base)]).unwrap();
    assert_eq!(
        design.styles["Badge"].frame,
        Some(Frame {
            width: Some(Pt(16.0 * 72.0 / 25.4)),
            align: Align::Right,
            gap: Some(Pt(6.0 * 72.0 / 25.4)),
            anchor: FrameAnchor::Title,
            offset: Pt(-9.0 * (72.0 / 25.4)),
        })
    );
    assert_eq!(
        design.to_json(PathMode::Real)["styles"]["Badge"]["frame"],
        json!({
            "width": 45.354, "align": "right", "gap": 17.008,
            "anchor": "title", "offset": -25.512,
        })
    );
    // Tables merge key by key, so a document can move a theme's frame.
    let design = merge(&[
        layer(style(json!({"width": "16mm", "anchor": "title"})), base),
        layer(style(json!({"offset": "2pt"})), base),
    ])
    .unwrap();
    let frame = design.styles["Badge"].frame.unwrap();
    assert_eq!(frame.anchor, FrameAnchor::Title);
    assert_eq!(frame.offset, Pt(2.0));
    // A style without a frame keeps a null in the shared JSON.
    let design =
        merge(&[layer(json!({"styles": {"Badge": {}}}), base)]).unwrap();
    assert_eq!(
        design.to_json(PathMode::Real)["styles"]["Badge"]["frame"],
        json!(null)
    );
    for (frame, expected) in [
        (
            json!({"align": "justify"}),
            "design.styles.Badge.frame.align: expected left, center or right for a frame, got \"justify\"",
        ),
        (
            json!({"align": "middle"}),
            "design.styles.Badge.frame.align: expected left, center or right for a frame, got \"middle\"",
        ),
        (
            json!({"anchor": "page"}),
            "design.styles.Badge.frame.anchor: expected div or title, got \"page\"",
        ),
        (
            json!({"offset": "-9"}),
            "design.styles.Badge.frame.offset: expected a length with a unit",
        ),
        (
            json!({"offset": -9}),
            "design.styles.Badge.frame.offset: expected a length with a unit, such as \"4mm\" or \"-9mm\", got -9",
        ),
        (
            json!({"width": "-16mm"}),
            "design.styles.Badge.frame.width: expected a length",
        ),
        (
            json!({"wrap": "none"}),
            "design.styles.Badge.frame.wrap: unknown key",
        ),
        (json!(true), "design.styles.Badge.frame: expected a table"),
    ] {
        let error =
            merge(&[layer(style(frame), base)]).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
    // Writer's own paragraph styles cannot float.
    for name in ["Title", "Body Text", "heading-1", "Header", "footer"] {
        let table = json!({"styles": {name: {"frame": {}}}});
        let error = merge(&[layer(table, base)]).unwrap_err().to_string();
        assert!(
            error.contains(&format!(
                "design.styles.{name}.frame: only a div style can have a frame"
            )),
            "{error}"
        );
    }
}
