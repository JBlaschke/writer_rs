//! Complete editable-math corpus and independently generated Word equations.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use typst::World;
use typst::foundations::Datetime;
use writer::config::Config;
use writer::math::latex_mathml::{MathError, to_mathml};
use writer::math::latex_source::{macros_from_config, prepare};
use writer::math::omml::{fallback_text, from_mathml};
use writer::typst::html::equation_mathml;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld};

#[path = "common/math_xml.rs"]
mod math_xml;
use math_xml::{Node, parse};

struct Fixture {
    name: String,
    body: String,
    display: bool,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/math")
}

fn fixtures(kind: &str) -> Vec<Fixture> {
    let extension = if kind == "latex" { "tex" } else { "typ" };
    let mut paths: Vec<_> = fs::read_dir(root().join(kind))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), if kind == "latex" { 37 } else { 36 });
    paths
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).unwrap();
            let (mode, body) = text.split_once('\n').unwrap();
            let display = match mode {
                "% mode: display" | "// mode: display" => true,
                "% mode: inline" | "// mode: inline" => false,
                other => panic!("{}: invalid mode {other:?}", path.display()),
            };
            let body = body.strip_suffix('\n').unwrap_or(body);
            let body = if kind == "latex" {
                body.strip_prefix('$')
                    .and_then(|s| s.strip_suffix('$'))
                    .unwrap_or(body)
            } else {
                body
            };
            Fixture {
                name: path.file_stem().unwrap().to_str().unwrap().into(),
                body: body.into(),
                display,
            }
        })
        .collect()
}

fn expectations() -> toml::Table {
    fs::read_to_string(root().join("expectations.toml"))
        .unwrap()
        .parse()
        .unwrap()
}

fn expected<'a>(table: &'a toml::Table, kind: &str, name: &str) -> &'a str {
    table[kind][name]["docx"].as_str().unwrap()
}

fn macros(fixture: &Fixture) -> BTreeMap<String, String> {
    if fixture.name.starts_with("31-") {
        let config = Config::load(&root().join("macros.toml")).unwrap();
        macros_from_config(&config.math.katex_options)
    } else {
        BTreeMap::new()
    }
}

fn assert_fragment(fragment: &str) {
    let root = parse(fragment);
    assert_eq!(root.name, "m:oMath");
    assert_eq!(root.descendants("m:oMath").len(), 1);
    assert!(root.descendants("m:oMathPara").is_empty());
    let mut pending = vec![&root];
    while let Some(node) = pending.pop() {
        assert!(node.name.starts_with("m:") || node.name.starts_with("w:"));
        assert!(!node.attrs.keys().any(|key| key.starts_with("xmlns")));
        pending.extend(&node.children);
    }
    assert!(!fragment.contains("w:rFonts") && !fragment.contains("w:color"));
    assert!(!root.text_content().chars().any(|ch| {
        (0x1d400..=0x1d7ff).contains(&(ch as u32))
            || matches!(ch, '\u{fe00}' | '\u{fe01}')
    }));
}

#[test]
fn expectations_cover_the_exact_37_latex_and_36_typst_sources() {
    let table = expectations();
    for kind in ["latex", "typst"] {
        let sources: BTreeSet<_> =
            fixtures(kind).into_iter().map(|f| f.name).collect();
        let listed: BTreeSet<_> =
            table[kind].as_table().unwrap().keys().cloned().collect();
        assert_eq!(sources, listed, "{kind}");
        for name in listed {
            assert!(
                ["omml", "unsupported", "no-mathml"]
                    .contains(&expected(&table, kind, &name))
            );
        }
    }
}

#[test]
fn complete_latex_corpus_maps_prepared_mathml_to_editable_fragments() {
    let table = expectations();
    for fixture in fixtures("latex") {
        let (prepared, _) = prepare(&fixture.body);
        let mathml =
            match to_mathml(&prepared, fixture.display, &macros(&fixture)) {
                Ok(mathml) => mathml,
                Err(error) => {
                    assert_eq!(
                        expected(&table, "latex", &fixture.name),
                        "no-mathml",
                        "{}: {error}",
                        fixture.name
                    );
                    assert!(matches!(error, MathError::Syntax { .. }));
                    insta::assert_snapshot!(
                        format!("latex_{}", fixture.name),
                        error.to_string()
                    );
                    continue;
                }
            };
        match from_mathml(&mathml) {
            Ok(fragment) => {
                assert_eq!(expected(&table, "latex", &fixture.name), "omml");
                assert_fragment(&fragment);
                insta::assert_snapshot!(
                    format!("latex_{}", fixture.name),
                    math_xml::xml_pretty(&fragment)
                );
            }
            Err(error) => {
                assert_eq!(
                    expected(&table, "latex", &fixture.name),
                    "unsupported",
                    "{}: {error}",
                    fixture.name
                );
                assert!(matches!(error, MathError::Unsupported { .. }));
                assert!(!fallback_text(&mathml).is_empty());
                insta::assert_snapshot!(
                    format!("latex_{}", fixture.name),
                    error.to_string()
                );
            }
        }
    }
}

#[test]
fn complete_typst_corpus_uses_native_mathml_in_one_retained_world() {
    let table = expectations();
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    let mut world = WriterWorld::from_config(
        &config,
        directory.path(),
        MainFile::Virtual,
        Datetime::from_ymd(2026, 10, 28).unwrap(),
    )
    .unwrap();
    world.set_main_text("Retained main source", SourceMap::default());
    for fixture in fixtures("typst") {
        world.reset();
        let mathml =
            match equation_mathml(&mut world, &fixture.body, fixture.display) {
                Ok(mathml) => mathml,
                Err(error) => {
                    assert_eq!(
                        expected(&table, "typst", &fixture.name),
                        "no-mathml",
                        "{}: {error}",
                        fixture.name
                    );
                    insta::assert_snapshot!(
                        format!("typst_{}", fixture.name),
                        error.to_string()
                    );
                    continue;
                }
            };
        assert_eq!(expected(&table, "typst", &fixture.name), "omml");
        let fragment = from_mathml(&mathml).unwrap_or_else(|error| {
            panic!("{}: {error}\n{mathml}", fixture.name)
        });
        assert_fragment(&fragment);
        if fixture.name.starts_with("28-") {
            assert!(
                !parse(&fragment).descendants("m:nor").is_empty(),
                "native #text must remain editable"
            );
        }
        if fixture.name.starts_with("36-") {
            let tree = parse(&fragment);
            let prescripts = tree.descendants("m:sPre");
            assert_eq!(prescripts.len(), 1);
            assert_eq!(
                prescripts[0].child("m:sub").unwrap().text_content(),
                "2"
            );
            assert_eq!(
                prescripts[0].child("m:sup").unwrap().text_content(),
                "1"
            );
            assert_eq!(prescripts[0].child("m:e").unwrap().text_content(), "X");
        }
        assert_eq!(
            world.source(world.main()).unwrap().text(),
            "Retained main source"
        );
        insta::assert_snapshot!(
            format!("typst_{}", fixture.name),
            math_xml::xml_pretty(&fragment)
        );
    }
}

#[test]
fn the_four_foreign_mathml_fixtures_keep_their_declared_outcomes() {
    for (name, supported) in [
        ("mprescripts", true),
        ("mlabeledtr", false),
        ("mmultiscripts-post", false),
        ("merror", false),
    ] {
        let source =
            fs::read_to_string(root().join(format!("mathml/{name}.mml")))
                .unwrap();
        match from_mathml(&source) {
            Ok(fragment) => {
                assert!(supported, "{name}");
                assert_eq!(parse(&fragment).descendants("m:sPre").len(), 1);
            }
            Err(MathError::Unsupported { detail, .. }) => {
                assert!(!supported, "{name}");
                assert!(detail.chars().count() <= 80);
                assert!(!fallback_text(&source).is_empty(), "{name}");
            }
            Err(error) => panic!("{name}: unexpected error {error}"),
        }
    }
}

/// Observed Pandoc 3.12.1 aliases, restricted to the exact named corpus forms.
/// Each rewrite preserves every other node, attribute and character. See
/// math.md §5.2 for the two styling decisions and the additional oracle forms.
fn oracle_form(node: &mut Node, name: &str) {
    let unstyled = match name {
        "03-integral-dx" => Some("\u{2009}"),
        "14-lim" => Some("∞"),
        _ => None,
    };
    if let Some(text) = unstyled
        && *node == parse(&format!("<m:r><m:t>{text}</m:t></m:r>"))
    {
        // A plain thin-space style and a plain infinity glyph are equivalent
        // here; retaining p lets adjacent equal-property runs merge below.
        node.children.insert(0, parse(r#"<m:rPr><m:sty m:val="p"/></m:rPr>"#));
    }
    if ["09-text-if", "20-cases", "35-nested-fences-text"].contains(&name)
        && *node
            == parse("<m:r><m:rPr><m:nor/></m:rPr><m:t>if\u{a0}</m:t></m:r>")
    {
        // math-core preserves TeX text spaces as NBSP; Pandoc uses ASCII.
        node.children[1].text = "if ".into();
    }
    if name == "08-aligned"
        && node.name == "m:eqArr"
        && node.children.len() == 2
        && node.children.iter().all(|row| row.name == "m:e")
        && let Some(first) = node.children[1].children.first_mut()
        && *first == parse("<m:r><m:t>\u{200b}&amp;</m:t></m:r>")
    {
        // Only the empty first cell of this exact row uses the placeholder.
        first.children[0].text = "&".into();
    }
    if name == "23-underbrace-overbrace"
        && node.name == "m:limLow"
        && node.attrs.is_empty()
        && node.text.is_empty()
        && node.children.len() == 2
        && node.children[0].name == "m:e"
        && node.children[1]
            == parse(
                r#"<m:lim><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>⏟</m:t></m:r></m:lim>"#,
            )
    {
        // Pandoc represents the brace as a lower limit. The binding Writer
        // form is groupChr; retain the complete body and the outer label.
        let mut group = parse(
            r#"<m:groupChr><m:groupChrPr><m:chr m:val="⏟"/><m:pos m:val="bot"/><m:vertJc m:val="top"/></m:groupChrPr><m:e/></m:groupChr>"#,
        );
        group.children[1] = node.children[0].clone();
        *node = group;
    }
    if name == "24-bmatrix-vmatrix"
        && matches!(node.name.as_str(), "m:begChr" | "m:endChr")
        && node.attrs.len() == 1
        && node.attr("m:val") == Some("∣")
        && node.children.is_empty()
        && node.text.is_empty()
    {
        // This changes delimiter spelling, never an ordinary relation run.
        node.attrs.insert("m:val".into(), "|".into());
    }
}

/// Preserve the complete OMML tree while normalizing mechanical XML choices,
/// equivalent column-count grouping and the explicitly reviewed corpus forms.
fn canonical(mut node: Node, name: &str) -> Node {
    node.attrs.retain(|key, _| !key.starts_with("xmlns") && key != "xml:space");
    if node.name != "m:t" && node.name != "w:t" && node.text.trim().is_empty() {
        node.text.clear();
    }
    node.children =
        node.children.into_iter().map(|child| canonical(child, name)).collect();
    if node.name == "m:rPr" {
        node.children.sort_by(|a, b| a.name.cmp(&b.name));
    }
    if node.name == "m:r" {
        let text =
            node.child("m:t").map(Node::text_content).unwrap_or_default();
        if !text.is_empty()
            && text.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
        {
            node.children.retain(|child| {
                !(child.name == "m:rPr"
                    && child.children.len() == 1
                    && child.children[0].name == "m:sty"
                    && child.children[0].attr("m:val") == Some("p"))
            });
        }
        // Exact intentional difference from math.md §5.2: this \mathbf{x}
        // run is upright bold in Writer; Pandoc records it as bold italic.
        if name == "13-boldsymbol-mathbf"
            && text == "x"
            && let Some(properties) =
                node.children.iter_mut().find(|n| n.name == "m:rPr")
        {
            for property in &mut properties.children {
                if property.name == "m:sty"
                    && property.attr("m:val") == Some("b")
                {
                    property.attrs.insert("m:val".into(), "bi".into());
                }
            }
        }
    }
    // Exact second intentional difference: the isolated bar accent maps the
    // spacing bar to combining U+0304; Pandoc uses U+203E instead.
    if name == "bar-accent"
        && node.name == "m:chr"
        && node.attr("m:val") == Some("‾")
    {
        node.attrs.insert("m:val".into(), "̄".into());
    }
    if node.name == "m:mcs" {
        let mut expanded = Vec::new();
        for column in node.children {
            let count: usize = column
                .child("m:mcPr")
                .and_then(|n| n.child("m:count"))
                .and_then(|n| n.attr("m:val"))
                .unwrap_or("1")
                .parse()
                .unwrap();
            assert!(count <= 64, "oracle column count bound");
            let mut column = column;
            if let Some(properties) =
                column.children.iter_mut().find(|n| n.name == "m:mcPr")
                && let Some(value) =
                    properties.children.iter_mut().find(|n| n.name == "m:count")
            {
                value.attrs.insert("m:val".into(), "1".into());
            }
            expanded.extend(std::iter::repeat_n(column, count));
        }
        node.children = expanded;
    }
    oracle_form(&mut node, name);
    let mut merged: Vec<Node> = Vec::new();
    for child in node.children {
        let previous = merged.last_mut();
        if child.name == "m:r"
            && let Some(previous) = previous
            && previous.name == "m:r"
            && previous.attrs == child.attrs
            && previous.child("m:rPr") == child.child("m:rPr")
            && previous
                .children
                .iter()
                .all(|n| matches!(n.name.as_str(), "m:rPr" | "m:t"))
            && child
                .children
                .iter()
                .all(|n| matches!(n.name.as_str(), "m:rPr" | "m:t"))
            && let Some(text) =
                previous.children.iter_mut().find(|n| n.name == "m:t")
        {
            text.text.push_str(&child.child("m:t").unwrap().text);
            continue;
        }
        merged.push(child);
    }
    node.children = merged;
    node
}

#[test]
fn pandoc_aliases_require_named_fixtures_and_exact_forms() {
    let forms = [
        (
            "03-integral-dx",
            r#"<m:e><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t> d</m:t></m:r></m:e>"#,
            r#"<m:e><m:r><m:t> </m:t></m:r><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>d</m:t></m:r></m:e>"#,
        ),
        (
            "08-aligned",
            "<m:eqArr><m:e><m:r><m:t>a&amp;</m:t></m:r></m:e><m:e><m:r><m:t>\u{200b}&amp;</m:t></m:r></m:e></m:eqArr>",
            "<m:eqArr><m:e><m:r><m:t>a&amp;</m:t></m:r></m:e><m:e><m:r><m:t>&amp;</m:t></m:r></m:e></m:eqArr>",
        ),
        (
            "09-text-if",
            "<m:r><m:rPr><m:nor/></m:rPr><m:t>if\u{a0}</m:t></m:r>",
            "<m:r><m:rPr><m:nor/></m:rPr><m:t>if </m:t></m:r>",
        ),
        (
            "20-cases",
            "<m:r><m:rPr><m:nor/></m:rPr><m:t>if\u{a0}</m:t></m:r>",
            "<m:r><m:rPr><m:nor/></m:rPr><m:t>if </m:t></m:r>",
        ),
        (
            "35-nested-fences-text",
            "<m:r><m:rPr><m:nor/></m:rPr><m:t>if\u{a0}</m:t></m:r>",
            "<m:r><m:rPr><m:nor/></m:rPr><m:t>if </m:t></m:r>",
        ),
        (
            "14-lim",
            "<m:r><m:t>∞</m:t></m:r>",
            r#"<m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>∞</m:t></m:r>"#,
        ),
        (
            "23-underbrace-overbrace",
            r#"<m:limLow><m:e><m:r><m:t>x</m:t></m:r></m:e><m:lim><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>⏟</m:t></m:r></m:lim></m:limLow>"#,
            r#"<m:groupChr><m:groupChrPr><m:chr m:val="⏟"/><m:pos m:val="bot"/><m:vertJc m:val="top"/></m:groupChrPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:groupChr>"#,
        ),
        (
            "24-bmatrix-vmatrix", r#"<m:begChr m:val="∣"/>"#,
            r#"<m:begChr m:val="|"/>"#,
        ),
        (
            "13-boldsymbol-mathbf",
            r#"<m:r><m:rPr><m:sty m:val="b"/></m:rPr><m:t>x</m:t></m:r>"#,
            r#"<m:r><m:rPr><m:sty m:val="bi"/></m:rPr><m:t>x</m:t></m:r>"#,
        ),
        ("bar-accent", r#"<m:chr m:val="‾"/>"#, r#"<m:chr m:val="̄"/>"#),
    ];
    for (name, writer, pandoc) in forms {
        let expected = canonical(parse(pandoc), name);
        assert_eq!(canonical(parse(writer), name), expected, "{name}");
        assert_ne!(
            canonical(parse(writer), "unrelated-fixture"),
            canonical(parse(pandoc), "unrelated-fixture"),
            "{name}: alias escaped its fixture"
        );
        let mut additional = parse(writer);
        additional
            .attrs
            .insert("unreviewed-property".into(), "preserve".into());
        assert_ne!(
            canonical(additional, name),
            expected,
            "{name}: lost attribute"
        );
    }
    // Almost matching ordinary text, separators and limits stay distinct.
    for (name, left, right) in [
        (
            "08-aligned",
            "<m:eqArr><m:e><m:r><m:t>a&amp;</m:t></m:r></m:e><m:e><m:r><m:t>word\u{200b}&amp;</m:t></m:r></m:e></m:eqArr>",
            "<m:eqArr><m:e><m:r><m:t>a&amp;</m:t></m:r></m:e><m:e><m:r><m:t>word&amp;</m:t></m:r></m:e></m:eqArr>",
        ),
        (
            "03-integral-dx",
            "<m:r><m:t> </m:t></m:r>",
            r#"<m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t> </m:t></m:r>"#,
        ),
        (
            "09-text-if",
            "<m:r><m:rPr><m:nor/></m:rPr><m:t>if\u{a0}x</m:t></m:r>",
            "<m:r><m:rPr><m:nor/></m:rPr><m:t>if x</m:t></m:r>",
        ),
        (
            "14-lim",
            "<m:r><m:t>x</m:t></m:r>",
            r#"<m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>x</m:t></m:r>"#,
        ),
        (
            "23-underbrace-overbrace",
            r#"<m:limLow><m:e><m:r><m:t>x</m:t></m:r></m:e><m:lim><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>⏟m</m:t></m:r></m:lim></m:limLow>"#,
            r#"<m:groupChr><m:groupChrPr><m:chr m:val="⏟"/><m:pos m:val="bot"/><m:vertJc m:val="top"/></m:groupChrPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:groupChr>"#,
        ),
        (
            "23-underbrace-overbrace",
            r#"<m:limLow><m:e><m:limLow><m:e><m:r><m:t>x</m:t></m:r></m:e><m:lim><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>⏟</m:t></m:r></m:lim></m:limLow></m:e><m:lim><m:r><m:t>m</m:t></m:r></m:lim></m:limLow>"#,
            r#"<m:limLow><m:e><m:groupChr><m:groupChrPr><m:chr m:val="⏟"/><m:pos m:val="bot"/><m:vertJc m:val="top"/></m:groupChrPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:groupChr></m:e><m:lim><m:r><m:t>n</m:t></m:r></m:lim></m:limLow>"#,
        ),
        (
            "24-bmatrix-vmatrix", r#"<m:sepChr m:val="∣"/>"#,
            r#"<m:sepChr m:val="|"/>"#,
        ),
        (
            "24-bmatrix-vmatrix", "<m:r><m:t>∣</m:t></m:r>",
            "<m:r><m:t>|</m:t></m:r>",
        ),
        ("bar-accent", "<m:r><m:t>‾</m:t></m:r>", "<m:r><m:t>̄</m:t></m:r>"),
    ] {
        assert_ne!(
            canonical(parse(left), name),
            canonical(parse(right), name),
            "{name}"
        );
    }
}

#[test]
fn installed_pandoc_agrees_on_shared_shapes_with_reviewed_exact_forms() {
    let version =
        match std::process::Command::new("pandoc").arg("--version").output() {
            Ok(output) => {
                assert!(output.status.success());
                String::from_utf8(output.stdout).unwrap()
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "skipping optional Pandoc OMML oracle: pandoc not on PATH"
                );
                return;
            }
            Err(error) => panic!("pandoc --version: {error}"),
        };
    let cases =
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 20, 23, 24, 25, 34, 35, 36];
    let mut selected: Vec<_> = fixtures("latex")
        .into_iter()
        .filter(|fixture| {
            cases.contains(&fixture.name[..2].parse::<usize>().unwrap())
        })
        .collect();
    selected.push(Fixture {
        name: "bar-accent".into(),
        body: r"\bar{x}".into(),
        display: false,
    });
    let mut differences = Vec::new();
    for fixture in selected {
        let source = if fixture.display {
            format!("$$\n{}\n$$\n", fixture.body)
        } else {
            format!("${}$\n", fixture.body)
        };
        let output = assert_cmd::Command::new("pandoc")
            .args(["-f", "markdown+tex_math_dollars", "-t", "docx"])
            .timeout(std::time::Duration::from_secs(10))
            .write_stdin(source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            fixture.name,
            String::from_utf8_lossy(&output.stderr)
        );
        let package = math_xml::zip_parts(&output.stdout);
        let tree = parse(&math_xml::part(&package, "word/document.xml"));
        let equations = tree.descendants("m:oMath");
        assert_eq!(
            equations.len(),
            1,
            "{}: Pandoc equation count",
            fixture.name
        );
        let (prepared, _) = prepare(&fixture.body);
        let mathml =
            to_mathml(&prepared, fixture.display, &BTreeMap::new()).unwrap();
        let writer = from_mathml(&mathml).unwrap();
        let actual = canonical(parse(&writer), &fixture.name);
        let expected = canonical(equations[0].clone(), &fixture.name);
        if actual != expected {
            differences.push(format!(
                "{}\nWriter: {actual:#?}\nPandoc: {expected:#?}",
                fixture.name
            ));
        }
    }
    assert!(differences.is_empty(), "{version}\n{}", differences.join("\n\n"));
}
