//! T105: layer 0, the built-in design, behind the `APPLIED` switch.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};
use writer::design::{
    APPLIED, Border, BorderSide, Case, Design, Domains, Indent, Layer,
    LineHeight, Origin, PathMode, Pt, defaults, merge,
};
use writer::pipeline::{self, BuildOptions};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn layer(value: Value, origin: Origin) -> Layer<'static> {
    Layer {
        table: value.as_object().unwrap().clone(),
        base_dir: Path::new(env!("CARGO_MANIFEST_DIR")),
        origin,
    }
}

fn front(value: Value) -> Layer<'static> {
    layer(value, Origin::FrontMatter)
}

fn close(actual: Option<Pt>, expected: f64) {
    let actual = actual.expect("a length").0;
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

fn files(dir: &Path, extensions: &[&str], into: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files(&path, extensions, into);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
        {
            into.push(path);
        }
    }
}

/// The groups each task switches on, in the planned order (R13 §6.2).
const TASKS: [(&str, Domains); 11] = [
    (
        "T108",
        Domains::TEXT
            .union(Domains::FONTS)
            .union(Domains::COLORS)
            .union(Domains::PAGE),
    ),
    ("T109", Domains::HEADINGS),
    ("T110", Domains::CAPTIONS.union(Domains::IMAGES)),
    ("T111", Domains::PALETTE.union(Domains::LINKS)),
    ("T112", Domains::LISTS.union(Domains::QUOTES)),
    ("T113", Domains::CODE.union(Domains::CALLOUTS).union(Domains::MATH_FONT)),
    ("T114", Domains::TABLES),
    ("T115", Domains::FOOTNOTES),
    ("T116", Domains::PAGE_NUMBERS),
    ("T117", Domains::DIV_CLASSES),
    ("T118", Domains::TYPOGRAPHY),
];

#[test]
fn the_tasks_switch_on_every_group() {
    let all =
        TASKS.iter().fold(Domains::NONE, |all, (_, groups)| all | *groups);
    assert_eq!(all, Domains::ALL);
}

/// Criterion 2, documents: the pipeline's design is the view of `APPLIED`,
/// and the view without any group equals the design merged without layer
/// 0, `declared` included. While `APPLIED` is empty, the pipeline's design
/// is that design too. The checks hold for any `APPLIED`, so the domain
/// tasks keep them as they switch groups on.
#[test]
fn the_view_without_domains_is_the_design_without_layer_0() {
    let mut inputs = Vec::new();
    for dir in ["tests/fixtures", "examples"] {
        files(&root().join(dir), &["md", "ipynb", "typ"], &mut inputs);
    }
    let mut declared = 0;
    for input in &inputs {
        let Ok(prepared) = pipeline::prepare(&BuildOptions::new(input)) else {
            continue;
        };
        let design = &prepared.design;
        let name = input.display();
        let without = merge(&design.layers()).unwrap();
        assert_eq!(design.view(Domains::NONE).unwrap(), without, "{name}");
        assert_eq!(design.view(APPLIED).unwrap(), *design, "{name}");
        assert_eq!(design.user_declared, without.declared, "{name}");
        if APPLIED.is_empty() {
            assert_eq!(design, &without, "{name}");
            assert_eq!(design.declared, without.declared, "{name}");
            assert!(design.origins.values().all(|o| *o != Origin::Builtin));
        }
        declared += usize::from(without.declared);
    }
    assert!(inputs.len() > 100, "{} inputs", inputs.len());
    assert!(declared >= 2, "{declared} declared designs");
}

/// Criterion 2, design tables: every `[design]` in the fixtures, examples
/// and bundled themes merges the same with layer 0 present but unapplied,
/// errors included; with all of layer 0 merged first, its view still is.
#[test]
fn every_fixture_design_is_unchanged_by_an_unapplied_layer_0() {
    let mut tables = Vec::new();
    for dir in ["tests/fixtures", "examples", "assets/themes"] {
        files(&root().join(dir), &["toml"], &mut tables);
    }
    let mut seen = 0;
    for file in tables {
        let text = std::fs::read_to_string(&file).unwrap();
        let Ok(parsed) = toml::from_str::<toml::Table>(&text) else {
            continue;
        };
        let Some(design) = parsed.get("design") else {
            continue;
        };
        seen += 1;
        let origin = if file.ends_with("theme.toml") {
            Origin::Theme { file: file.clone() }
        } else {
            Origin::ProjectFile(file.clone())
        };
        let user = Layer {
            table: serde_json::to_value(design)
                .unwrap()
                .as_object()
                .unwrap()
                .clone(),
            base_dir: file.parent().unwrap(),
            origin,
        };
        let without = merge(std::slice::from_ref(&user));
        let unapplied = merge(&[defaults::layer(Domains::NONE), user.clone()]);
        match (&without, &unapplied) {
            (Ok(without), Ok(unapplied)) => {
                assert_eq!(without, unapplied, "{}", file.display());
                let full = merge(&[defaults::layer(Domains::ALL), user])
                    .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
                assert_eq!(full.view(Domains::NONE).unwrap(), *without);
                assert!(full.declared && full.user_declared);
            }
            (Err(without), Err(unapplied)) => {
                assert_eq!(without.to_string(), unapplied.to_string());
            }
            _ => panic!("{}: {without:?} vs {unapplied:?}", file.display()),
        }
    }
    assert!(seen >= 6, "{seen} design tables");
}

#[test]
fn every_prefix_of_the_task_order_resolves() {
    let mut applied = Domains::NONE;
    for (task, groups) in TASKS {
        applied = applied | groups;
        assert!(applied.contains(applied.needs()), "{task}");
        let design = merge(&[defaults::layer(applied)])
            .unwrap_or_else(|error| panic!("after {task}: {error}"));
        assert!(design.declared && !design.user_declared, "{task}");
        assert!(design.sources == Default::default());
    }
    for (name, group) in Domains::GROUPS {
        let applied = group | group.needs();
        merge(&[defaults::layer(applied)])
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn layer_0_resolves_to_r13s_proposed_values() {
    let design = Design::default().view(Domains::ALL).unwrap();
    assert!(design.declared);
    assert!(!design.user_declared);
    close(design.text.size, 11.0);
    assert_eq!(design.text.line_height, Some(LineHeight::Multiple(1.6)));
    close(design.text.paragraph_spacing, 12.65);
    close(design.text.measure, 462.0);
    assert_eq!(design.text.widows, Some(2));
    let math = design.fonts.math.as_ref().unwrap();
    assert_eq!(math.family, "New Computer Modern Math");
    assert_eq!(design.colors.callout.other, design.colors.muted);
    close(design.page.margin.left, 25.0 * 72.0 / 25.4);
    let footer = design.page.footer.as_ref().unwrap();
    let footer = footer.first.as_ref().unwrap();
    close(footer.size, 8.25);
    assert_eq!(footer.color, design.colors.muted);
    let heading = &design.styles["Heading 1"];
    close(heading.size, 20.35);
    close(heading.space_before, 40.7);
    close(heading.space_after, 0.65 * 20.35);
    assert!(heading.keep_with_next);
    let abstract_title = &design.styles["Abstract Title"];
    assert_eq!(abstract_title.case, Some(Case::Upper));
    close(abstract_title.letter_spacing, 0.08 * 0.85 * 11.0);
    let quote = &design.styles["Block Text"];
    assert_eq!(
        quote.border,
        Some(Border {
            side: BorderSide::Left,
            width: Some(Pt(1.5)),
            color: design.colors.border,
            gap: Some(Pt(1.25 * 11.0)),
        })
    );
    let zero = Some(Pt(0.0));
    assert_eq!(quote.indent, Some(Indent { left: zero, right: zero }));
    close(design.styles["toc 3"].indent.unwrap().left, 26.4);
    close(design.code.padding.unwrap().top, 9.35);
    // Image and table captions are 0.6em of Caption's size from the body.
    let caption = design.styles["Caption"].size.unwrap().0;
    close(design.styles["Image Caption"].space_before, 0.6 * caption);
    close(design.styles["Table Caption"].space_after, 0.6 * caption);
    // No key holds R13's hanging indent yet, and a left indent is not one.
    assert_eq!(design.styles["Bibliography"].indent, None);
    assert_eq!(design.code.background, design.colors.code_bg);
    assert_eq!(design.footnotes.separator.unwrap().length, Some(0.33));
    assert_eq!(design.image.dpi, Some(96.0));
    assert_eq!(design.links.print_url, Some(false));
    let json = design.to_json(PathMode::Real);
    let keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        [
            "fonts", "text", "colors", "page", "styles", "lists", "table",
            "code", "callout", "image", "footnotes", "links"
        ]
    );
    assert_eq!(json["styles"]["Abstract Title"]["case"], "upper");
    assert_eq!(json["table"]["width"], "full");
}

/// A layer above that sets one side of a margin, padding or indent keeps
/// layer 0's other sides: layer 0 writes them as tables, which merge.
#[test]
fn a_users_side_keeps_layer_0s_other_sides() {
    let design = merge(&[front(json!({
        "page": {"margin": {"top": "50mm"}},
        "code": {"padding": {"left": "2pt"}},
        "styles": {"Abstract": {"indent": {"right": "1em"}},
            "Block Text": {"indent": {"left": "1em"}}},
    }))])
    .unwrap()
    .view(Domains::ALL)
    .unwrap();
    let margin = design.page.margin;
    close(margin.top, 50.0 * 72.0 / 25.4);
    for side in [margin.right, margin.bottom, margin.left] {
        close(side, 25.0 * 72.0 / 25.4);
    }
    let padding = design.code.padding.unwrap();
    close(padding.left, 2.0);
    for side in [padding.top, padding.right, padding.bottom] {
        close(side, 0.85 * 11.0);
    }
    let indent = design.styles["Abstract"].indent.unwrap();
    close(indent.left, 3.5 * 11.0);
    close(indent.right, 11.0);
    let indent = design.styles["Block Text"].indent.unwrap();
    close(indent.left, 11.0);
    close(indent.right, 0.0);
    let top = design.origin(&["page", "margin", "top"]);
    assert_eq!(top, Some(&Origin::FrontMatter));
    assert!(design.is_builtin(&["page", "margin", "left"]));
}

/// Criterion 4: em at the top level is of `text.size`, else of layer 0's.
#[test]
fn em_lengths_at_the_top_level_are_of_the_text_size() {
    let design = merge(&[front(json!({
        "text": {"size": "12pt", "paragraph_spacing": "1.5em",
            "line_height": "1.25em", "measure": "40em"},
        "page": {"margin": {"top": "2em"}, "header_offset": "1em",
            "footer": {"lines": ["x"], "size": "0.5em",
                "rule": {"width": "0.1em"}}},
        "lists": {"indent": "2em"},
        "callout": {"padding": "1em"},
    }))])
    .unwrap();
    close(design.text.paragraph_spacing, 18.0);
    assert_eq!(design.text.line_height, Some(LineHeight::Exact(Pt(15.0))));
    close(design.text.measure, 480.0);
    close(design.page.margin.top, 24.0);
    assert_eq!(design.page.margin.left, None);
    close(design.page.header_offset, 12.0);
    let footer = design.page.footer.unwrap().first.unwrap();
    close(footer.size, 6.0);
    close(Some(footer.rule.unwrap().width), 1.2);
    close(design.lists.indent, 24.0);
    close(design.callout.padding.unwrap().right, 12.0);
    let no_size = front(json!({"text": {"paragraph_spacing": "1em"}}));
    let no_size = merge(&[no_size]).unwrap();
    close(no_size.text.paragraph_spacing, 11.0);
    assert_eq!(no_size.text.size, None);
}

/// Criterion 4: inside a style, em is of the style's size; the style's own
/// size is of `text.size`.
#[test]
fn em_lengths_inside_styles_are_of_the_style_size() {
    let design = merge(&[front(json!({
        "text": {"size": "10pt"},
        "styles": {
            "Big": {"size": "2em", "space_before": "0.5em",
                "line_height": "1.5em", "letter_spacing": "-0.1em",
                "indent": "1em", "padding": {"left": "0.25em"},
                "border": {"width": "0.05em", "gap": "1em"},
                "frame": {"width": "10em", "offset": "-1em"}},
            "Plain": {"space_after": "1em", "indent": {"right": "2em"}},
        },
    }))])
    .unwrap();
    let big = &design.styles["Big"];
    close(big.size, 20.0);
    close(big.space_before, 10.0);
    assert_eq!(big.line_height, Some(LineHeight::Exact(Pt(30.0))));
    close(big.letter_spacing, -2.0);
    assert_eq!(
        big.indent,
        Some(Indent { left: Some(Pt(20.0)), right: Some(Pt(20.0)) })
    );
    close(big.padding.unwrap().left, 5.0);
    let border = big.border.unwrap();
    assert_eq!(border.side, BorderSide::All);
    close(border.width, 1.0);
    close(border.gap, 20.0);
    let frame = big.frame.unwrap();
    close(frame.width, 200.0);
    close(Some(frame.offset), -20.0);
    let plain = &design.styles["Plain"];
    close(plain.space_after, 10.0);
    assert_eq!(
        plain.indent,
        Some(Indent { left: None, right: Some(Pt(20.0)) })
    );
}

#[test]
fn em_errors_name_the_key() {
    for (value, expected) in [
        (
            json!({"text": {"size": "1.2em"}}),
            "design.text.size: em is relative to text.size; expected a length with an absolute unit such as \"11pt\", got \"1.2em\"",
        ),
        (
            json!({"styles": {"X": {"space_after": "-1em"}}}),
            "design.styles.X.space_after: expected a length with a unit such as \"11pt\" or \"22mm\", got \"-1em\"",
        ),
        (
            json!({"styles": {"X": {"case": "lower"}}}),
            "design.styles.X.case: expected upper or smallcaps, got \"lower\"",
        ),
        (
            json!({"colors": {"text": "border"}}),
            "design.colors.text: colour token \"border\" is unset",
        ),
        (
            json!({"footnotes": {"separator": {"length": "120%"}}}),
            "design.footnotes.separator.length: expected a percentage from 0% to 100%, got \"120%\"",
        ),
    ] {
        let error = merge(&[front(value)]).unwrap_err();
        assert_eq!(error.to_string(), format!("config: {expected}"));
    }
}

#[test]
fn origins_tell_built_in_values_from_the_users() {
    let theme = layer(
        json!({"colors": {"accent": "#112233"}}),
        Origin::Theme { file: "/themes/t/theme.toml".into() },
    );
    let project = layer(
        json!({"text": {"size": "12pt"},
            "styles": {"heading-1": {"size": "2em"}}}),
        Origin::ProjectFile("/p/writer.toml".into()),
    );
    let user = [theme, project, front(json!({"page": {"footer": {}}}))];
    let design = merge(&user).unwrap();
    assert!(design.user_declared && design.declared);
    let full = design.view(Domains::ALL).unwrap();
    assert!(full.user_declared);
    assert_eq!(
        full.origin(&["text", "size"]),
        Some(&Origin::ProjectFile("/p/writer.toml".into()))
    );
    assert_eq!(full.origin(&["text", "line_height"]), Some(&Origin::Builtin));
    assert!(full.is_builtin(&["text", "line_height"]));
    assert!(!full.is_builtin(&["text"]));
    assert!(full.is_builtin(&["lists"]));
    assert!(!full.is_builtin(&["page", "footer"]));
    assert!(!full.is_builtin(&["nothing"]));
    // The empty footer clears layer 0's page number.
    assert_eq!(full.page.footer, Some(Default::default()));
    assert_eq!(full.origin(&["page", "footer"]), Some(&Origin::FrontMatter));
    // The user's spelling of a built-in style merges into it.
    assert!(!full.styles.contains_key("Heading 1"));
    let heading = &full.styles["heading-1"];
    close(heading.size, 24.0);
    assert_eq!(heading.weight.map(|weight| weight.0), Some(700));
    assert!(full.is_builtin(&["styles", "heading-1", "weight"]));
    let origins = full.origins_json();
    assert_eq!(origins["colors"]["accent"], "theme /themes/t/theme.toml");
    assert_eq!(origins["colors"]["muted"], "builtin");
    assert_eq!(origins["page"]["footer"], "front matter");
    assert_eq!(
        origins["styles"]["heading-1"]["size"],
        "project /p/writer.toml"
    );
}

fn writer() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_writer"));
    cmd.current_dir(root())
        .env_remove("RUST_LOG")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE");
    cmd
}

fn leaves(value: &Value, path: &mut Vec<String>, into: &mut Vec<Vec<String>>) {
    match value.as_object() {
        Some(map) if !map.is_empty() => {
            for (key, child) in map {
                path.push(key.clone());
                leaves(child, path, into);
                path.pop();
            }
        }
        _ => into.push(path.clone()),
    }
}

/// Criterion 3.
#[test]
fn dump_design_defaults_lists_every_key_with_its_origin() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("doc.md");
    std::fs::write(
        &input,
        "---\ndesign:\n  text:\n    size: 12pt\n---\n# Test\n",
    )
    .unwrap();
    let output = writer()
        .args(["dump", "--design", "--defaults"])
        .arg(&input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    let keys: Vec<_> = value.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["design", "origins"]);
    assert_eq!(value["design"]["text"]["size"], 12.0);
    assert_eq!(value["design"]["styles"]["Heading 1"]["size"], 22.2);
    let origins = &value["origins"];
    assert_eq!(origins["text"]["size"], "front matter");
    let mut paths = Vec::new();
    leaves(
        &Value::Object(defaults::table().clone()),
        &mut Vec::new(),
        &mut paths,
    );
    for path in paths {
        let origin = path.iter().fold(origins, |node, key| &node[key]);
        let expected = if path == ["text", "size"] {
            "front matter"
        } else {
            "builtin"
        };
        assert_eq!(origin, expected, "{path:?}");
    }
    let mut paths = Vec::new();
    leaves(origins, &mut Vec::new(), &mut paths);
    assert!(paths.iter().all(|path| {
        let mut node = &Value::Object(defaults::table().clone());
        path.iter().all(|key| {
            node = &node[key];
            !node.is_null()
        })
    }));
}

/// Criterion 3: without the flag, the dump is the view the backends read:
/// `null` for an undeclared design while `APPLIED` is empty, as before.
#[test]
fn dump_design_without_defaults_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("doc.md");
    std::fs::write(&input, "# Test\n").unwrap();
    let output = writer()
        .args(["dump", "--design"])
        .arg(&input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();
    let design = pipeline::prepare(&BuildOptions::new(&input)).unwrap().design;
    let json = design.to_json(PathMode::Real);
    let expected = serde_json::to_string_pretty(&json).unwrap();
    assert_eq!(output, format!("{expected}\n"));
    if APPLIED.is_empty() {
        assert_eq!(output, "null\n");
    }
    writer().args(["dump", "--defaults"]).arg(&input).assert().code(2);
}

/// Criterion 5: the backends and the JSON read the view's `declared`;
/// `writer check` reads `user_declared` (R13 §1.4).
#[test]
fn each_declared_reader_uses_one_flag() {
    let declared = regex::Regex::new(r"\.declared\b").unwrap();
    let mut backends = Vec::new();
    for dir in ["src/html", "src/docx", "src/typst", "src/assets", "src/pdf"] {
        files(&root().join(dir), &["rs"], &mut backends);
    }
    backends.push(root().join("src/design/json.rs"));
    for file in &backends {
        let text = std::fs::read_to_string(file).unwrap();
        assert!(!text.contains("user_declared"), "{}", file.display());
    }
    let json = std::fs::read_to_string(root().join("src/design/json.rs"));
    assert!(declared.is_match(&json.unwrap()));
    for file in ["src/check/design.rs", "src/check/targets.rs"] {
        let text = std::fs::read_to_string(root().join(file)).unwrap();
        let text = text.replace("user_declared", "");
        assert!(!declared.is_match(&text), "{file}");
    }
    let design = std::fs::read_to_string(root().join("src/check/design.rs"));
    assert_eq!(design.unwrap().matches(".user_declared").count(), 2);
    let targets = std::fs::read_to_string(root().join("src/check/targets.rs"));
    assert_eq!(targets.unwrap().matches(".user_declared").count(), 1);
}

/// Ejection writes the user's design only, whatever `APPLIED` holds: the
/// pipeline leaves layer 0 out of `merged_design_table`.
#[test]
fn ejection_leaves_layer_0_out() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("doc.md");
    std::fs::write(
        &input,
        "---\ndesign:\n  text:\n    size: 12pt\n---\n# Test\n",
    )
    .unwrap();
    let options = BuildOptions::new(&input);
    let prepared = pipeline::prepare(&options).unwrap();
    let table = pipeline::merged_design_table(&prepared, &options).unwrap();
    assert_eq!(Value::Object(table), json!({"text": {"size": "12pt"}}));
    // merged_table itself keeps every layer it is given.
    let table = writer::design::merged_table(&[
        defaults::layer(Domains::TEXT),
        front(json!({"text": {"size": "12pt"}})),
    ])
    .unwrap();
    assert_eq!(table["text"]["size"], "12pt");
    assert_eq!(table["text"]["line_height"], 1.6);
}

/// Ejection copies the files of every font role, `fonts.math` included,
/// and points the ejected theme at the copies.
#[test]
fn ejection_rewrites_the_files_of_every_font_role() {
    let dir = tempfile::tempdir().unwrap();
    let roles = ["body", "heading", "mono", "math"];
    let mut source = String::from("---\ndesign:\n  fonts:\n");
    for role in roles {
        std::fs::write(dir.path().join(format!("{role}.ttf")), role).unwrap();
        source.push_str(&format!(
            "    {role}:\n      family: X {role}\n      files: [{role}.ttf]\n"
        ));
    }
    source.push_str("---\n# Test\n");
    std::fs::write(dir.path().join("doc.md"), source).unwrap();
    writer()
        .current_dir(dir.path())
        .args(["eject", "theme", "out", "--design", "doc.md"])
        .assert()
        .success();
    let out = dir.path().join("out");
    let theme = std::fs::read_to_string(out.join("theme.toml")).unwrap();
    let theme: toml::Table = toml::from_str(&theme).unwrap();
    let fonts = &theme["design"]["fonts"];
    for role in roles {
        let file = format!("assets/design/{role}.ttf");
        let files = fonts[role]["files"].as_array().unwrap();
        assert_eq!(files[0].as_str(), Some(file.as_str()), "{role}");
        let copy = std::fs::read(out.join(&file)).unwrap();
        assert_eq!(copy, role.as_bytes());
    }
}

/// `writer check` reports what the user declared: layer 0's styles are
/// never unused and its values never shadowed, whatever `APPLIED` holds.
#[test]
fn check_findings_leave_layer_0_out() {
    use writer::check::design::{docx_shadowed, unused_styles};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("doc.md");
    std::fs::write(&input, "# Test\n").unwrap();
    let document =
        pipeline::prepare(&BuildOptions::new(&input)).unwrap().document;
    let reference = dir.path().join("reference.docx");
    let bytes = writer::docx::custom_style::reference_docx().unwrap();
    std::fs::write(&reference, bytes).unwrap();
    let with_layer_0 = |user: Value| {
        merge(&[defaults::layer(Domains::ALL), front(user)]).unwrap()
    };

    let design = with_layer_0(json!({
        "colors": {"accent": "#112233"},
        "styles": {"mine": {"size": "2em"}},
    }));
    let unused: Vec<_> = unused_styles(&design, &document)
        .into_iter()
        .map(|finding| finding.message)
        .collect();
    assert_eq!(unused, ["design style \"mine\" is not selected by any div"]);
    let notes = docx_shadowed(&design, &reference);
    assert!(notes.is_empty(), "{notes:?}");

    let design = with_layer_0(json!({
        "page": {"margin": "1in"},
        "text": {"size": "12pt"},
        "styles": {"Heading 1": {"size": "2em"}},
    }));
    let notes = docx_shadowed(&design, &reference);
    assert_eq!(notes.len(), 1, "{notes:?}");
    let message = &notes[0].message;
    assert!(message.contains("page setup"), "{message}");
    assert!(!message.contains("header/footer"), "{message}");
    assert!(message.contains("document defaults"), "{message}");
    assert!(message.contains("styles: Heading 1"), "{message}");
}
