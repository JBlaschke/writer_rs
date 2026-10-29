//! Real native Typst input across HTML, editable Word and retained PDF builds.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use regex::Regex;
use scraper::{Html, Selector};
use typst::World;
use writer::assets::emit::OutputMode;
use writer::config::Target;
use writer::docx::{ValidateOptions, validate};
use writer::error::{Diagnostic, Level};
use writer::html::BuildInfo;
use writer::pipeline::{self, BuildOptions, BuildReport, Prepared, WorldSlot};
use writer::typst::html::EQUATION_CSS;

#[path = "common/docx.rs"]
mod common_docx;
#[path = "common/math_xml.rs"]
mod math_xml;
use math_xml::{Node, parse};

const CONFIG: &str = concat!(
    "[build]\nout_dir='out'\n",
    "[pdf]\nsystem_fonts=false\nallow_packages=false\n",
    "timestamp='2026-10-29T00:00:00Z'\n",
);

struct Fixture {
    name: String,
    source: String,
    display: bool,
    expected: String,
    line: usize,
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/math")
}

fn corpus() -> Vec<Fixture> {
    let expectations: toml::Table =
        fs::read_to_string(fixture_root().join("expectations.toml"))
            .unwrap()
            .parse()
            .unwrap();
    let expected = expectations["typst"].as_table().unwrap();
    let mut paths = fs::read_dir(fixture_root().join("typst"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "typ"))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), 36);
    assert_eq!(expected.len(), 36);
    paths
        .into_iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
            let text = fs::read_to_string(&path).unwrap();
            let (mode, source) = text.split_once('\n').unwrap();
            let display = match mode {
                "// mode: inline" => false,
                "// mode: display" => true,
                other => panic!("{}: {other}", path.display()),
            };
            let expected = expected[&name]["docx"].as_str().unwrap();
            assert!(["omml", "no-mathml"].contains(&expected));
            Fixture {
                name,
                source: source.strip_suffix('\n').unwrap_or(source).into(),
                display,
                expected: expected.into(),
                line: 0,
            }
        })
        .collect()
}

fn equation_source(fixtures: &mut [Fixture]) -> String {
    let mut source = String::new();
    for fixture in fixtures {
        source.push_str(&format!("= CASE-{}\n\n", fixture.name));
        fixture.line = source.lines().count() + 1;
        if fixture.display {
            source.push_str(&format!("$ {} $\n\n", fixture.source));
        } else {
            source.push_str(&format!("${}$\n\n", fixture.source));
        }
    }
    source
}

fn project(source: &str, settings: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("document.typ"), source).unwrap();
    fs::write(root.path().join("writer.toml"), format!("{CONFIG}{settings}"))
        .unwrap();
    root
}

fn options(root: &Path) -> BuildOptions {
    let mut options = BuildOptions::new(root.join("document.typ"));
    options.config_path = Some(root.join("writer.toml"));
    options
}

fn assert_success(report: &BuildReport) -> &Prepared {
    let prepared = report.prepared.as_ref().unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(!prepared.diagnostics.has_errors(), "{:?}", prepared.diagnostics);
    for target in &report.targets {
        assert!(target.written.is_ok(), "{target:?}");
    }
    prepared
}

fn output(report: &BuildReport, target: Target) -> PathBuf {
    report
        .targets
        .iter()
        .find(|result| result.target == target)
        .unwrap()
        .written
        .as_ref()
        .unwrap()[0]
        .clone()
}

fn docx_parts(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let bytes = fs::read(path).unwrap();
    assert_eq!(validate(&bytes, &ValidateOptions { builtin_styles: true }), []);
    common_docx::read_back(&bytes);
    math_xml::zip_parts(&bytes)
}

fn style_id(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    let styles = parse(&math_xml::part(parts, "word/styles.xml"));
    styles
        .children
        .iter()
        .find(|node| {
            node.name == "w:style"
                && node.child("w:name").and_then(|name| name.attr("w:val"))
                    == Some(name)
        })
        .unwrap_or_else(|| panic!("missing style {name}"))
        .attr("w:styleId")
        .unwrap()
        .to_owned()
}

fn sections(document: &Node) -> BTreeMap<String, Vec<&Node>> {
    let mut sections: BTreeMap<String, Vec<&Node>> = BTreeMap::new();
    let mut marker = None;
    for node in &document.child("w:body").unwrap().children {
        let text = node.text_content();
        if text.starts_with("CASE-") {
            marker = Some(text);
        } else if let Some(marker) = &marker {
            sections.entry(marker.clone()).or_default().push(node);
        }
    }
    sections
}

fn assert_location(finding: &Diagnostic, source: &Path, line: usize) {
    assert_eq!(finding.level, Level::Warning, "{finding:?}");
    assert_eq!(finding.code, "math-conversion-failed", "{finding:?}");
    let canonical = fs::canonicalize(source).unwrap();
    assert_eq!(
        fs::canonicalize(finding.path.as_ref().unwrap()).unwrap(),
        canonical,
    );
    let span = finding.span.as_ref().unwrap();
    assert_eq!(span.start_line, line, "{finding:?}");
    assert_eq!(span.end_line, line, "{finding:?}");
    assert_eq!(span.cell, None);
    assert_eq!(
        fs::canonicalize(span.file.as_ref().unwrap()).unwrap(),
        canonical
    );
}

fn assert_no_katex(page: &str) {
    for forbidden in [
        "katex.min", "__writerRenderMath", "__writerKatexOptions",
        "data-math-style=",
    ] {
        assert!(!page.contains(forbidden), "unexpected {forbidden}");
    }
}

fn assert_native_css(page: &str, embed: bool) {
    let document = Html::parse_document(page);
    let styles = Selector::parse("style#writer-math").unwrap();
    let nodes = document.select(&styles).collect::<Vec<_>>();
    assert_eq!(nodes.len(), 1);
    let css = nodes[0].text().collect::<String>();
    assert_eq!(css.matches(EQUATION_CSS).count(), 1);
    assert_eq!(css.matches("@font-face").count(), usize::from(embed));
    assert_eq!(
        css.matches("data:font/woff2;base64,").count(),
        usize::from(embed)
    );
    assert!(css.contains(
        "\"Latin Modern Math\", \"STIX Two Math\", \"Cambria Math\""
    ));
}

#[test]
fn native_typst_corpus_uses_mathml_for_both_html_knobs_and_editable_docx() {
    let mut fixtures = corpus();
    let source = equation_source(&mut fixtures);
    assert_eq!(
        fixtures.iter().filter(|case| case.expected == "omml").count(),
        32
    );
    let failures = fixtures
        .iter()
        .filter(|case| case.expected == "no-mathml")
        .map(|case| case.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        failures,
        ["21-styles", "29-unknown-fn", "30-unknown-sym", "34-cancel-overline",]
    );
    for (route, font, targets) in [
        ("katex", "embed", vec![Target::Html, Target::Docx]),
        ("mathml", "system", vec![Target::Html]),
    ] {
        let root = project(
            &source,
            &format!("[math]\nhtml='{route}'\nmathml_font='{font}'\n"),
        );
        let options = options(root.path());
        let report =
            pipeline::build_targets(&options, &targets, &mut WorldSlot::new());
        assert_success(&report);
        let findings = report
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "math-conversion-failed")
            .collect::<Vec<_>>();
        assert_eq!(
            findings.len(),
            4 * targets.len(),
            "{:?}",
            report.diagnostics
        );
        assert_eq!(
            report.diagnostics.len(),
            findings.len(),
            "{:?}",
            report.diagnostics
        );
        for fixture in
            fixtures.iter().filter(|case| case.expected == "no-mathml")
        {
            let located = findings
                .iter()
                .filter(|finding| {
                    finding
                        .span
                        .as_ref()
                        .is_some_and(|span| span.start_line == fixture.line)
                })
                .collect::<Vec<_>>();
            assert_eq!(located.len(), targets.len(), "{}", fixture.name);
            for finding in located {
                assert_location(finding, &options.input, fixture.line);
            }
        }
        let page = fs::read_to_string(output(&report, Target::Html)).unwrap();
        assert_no_katex(&page);
        assert_native_css(&page, font == "embed");
        let html = Html::parse_document(&page);
        let selector = Selector::parse("span.math").unwrap();
        let equations = html.select(&selector).collect::<Vec<_>>();
        assert_eq!(equations.len(), 36);
        for (equation, fixture) in equations.iter().zip(&fixtures) {
            assert_eq!(
                equation.value().attr("data-tex"),
                Some(fixture.source.as_str())
            );
            let math =
                equation.select(&Selector::parse("math").unwrap()).count();
            let pictures =
                equation.select(&Selector::parse("img").unwrap()).count();
            let errors = equation
                .select(&Selector::parse("code.math-error").unwrap())
                .count();
            if fixture.expected == "omml" {
                assert_eq!(
                    (math, pictures, errors),
                    (1, 0, 0),
                    "{}",
                    fixture.name
                );
            } else if fixture.name.starts_with("21-")
                || fixture.name.starts_with("34-")
            {
                assert_eq!(
                    (math, pictures, errors),
                    (0, 1, 0),
                    "{}",
                    fixture.name
                );
            } else {
                assert_eq!(
                    (math, pictures, errors),
                    (0, 0, 1),
                    "{}",
                    fixture.name
                );
                assert_eq!(equation.text().collect::<String>(), fixture.source);
            }
        }
        assert!(
            html.select(&Selector::parse("mtable.aligned").unwrap()).count()
                > 0
        );
        if targets.contains(&Target::Docx) {
            let parts = docx_parts(&output(&report, Target::Docx));
            let document = parse(&math_xml::part(&parts, "word/document.xml"));
            assert_eq!(document.descendants("m:oMath").len(), 32);
            assert!(document.descendants("wp:inline").is_empty());
            let verbatim = style_id(&parts, "Verbatim Char");
            let sections = sections(&document);
            for fixture in &fixtures {
                let nodes = &sections[&format!("CASE-{}", fixture.name)];
                let equations = nodes
                    .iter()
                    .flat_map(|node| node.descendants("m:oMath"))
                    .collect::<Vec<_>>();
                assert_eq!(
                    equations.len(),
                    usize::from(fixture.expected == "omml"),
                    "{}",
                    fixture.name
                );
                let display = nodes
                    .iter()
                    .flat_map(|node| node.descendants("m:oMathPara"))
                    .count();
                assert_eq!(
                    display,
                    usize::from(fixture.display && fixture.expected == "omml"),
                    "{}",
                    fixture.name
                );
                if fixture.expected == "no-mathml" {
                    assert!(
                        nodes
                            .iter()
                            .flat_map(|node| node.descendants("w:r"))
                            .any(|run| run.text_content() == fixture.source
                                && run
                                    .child("w:rPr")
                                    .and_then(|props| props.child("w:rStyle"))
                                    .and_then(|style| style.attr("w:val"))
                                    == Some(verbatim.as_str())),
                        "{}",
                        fixture.name
                    );
                }
                if fixture.name.starts_with("28-") {
                    assert!(!equations[0].descendants("m:nor").is_empty());
                    assert!(equations[0].text_content().contains('x'));
                }
                if fixture.name.starts_with("13-") {
                    let styles = equations[0].descendants("m:sty");
                    assert_eq!(styles.len(), 2);
                    assert!(
                        styles
                            .iter()
                            .all(|style| style.attr("m:val") == Some("bi"))
                    );
                }
                for equation in equations {
                    assert!(!equation.text_content().chars().any(
                        |character| {
                            (0x1d400..=0x1d7ff).contains(&(character as u32))
                        }
                    ));
                }
            }
        }
    }
}

#[test]
fn native_display_failures_use_pictures_or_source_with_one_final_located_warning()
 {
    let mut fixtures = corpus()
        .into_iter()
        .filter(|fixture| fixture.expected == "no-mathml")
        .collect::<Vec<_>>();
    for fixture in &mut fixtures {
        fixture.display = true;
    }
    let source = equation_source(&mut fixtures);
    let root = project(&source, "[math]\nhtml='mathml'\n");
    let options = options(root.path());
    let mut worlds = WorldSlot::new();
    let report = pipeline::build_targets(
        &options,
        &[Target::Html, Target::Docx],
        &mut worlds,
    );
    assert_success(&report);
    assert_eq!(report.diagnostics.len(), 8, "{:?}", report.diagnostics);
    for fixture in &fixtures {
        let findings = report
            .diagnostics
            .iter()
            .filter(|finding| {
                finding
                    .span
                    .as_ref()
                    .is_some_and(|span| span.start_line == fixture.line)
            })
            .collect::<Vec<_>>();
        assert_eq!(findings.len(), 2, "{}", fixture.name);
        for finding in findings {
            assert_location(finding, &options.input, fixture.line);
            if fixture.name.starts_with("29-")
                || fixture.name.starts_with("30-")
            {
                assert!(
                    finding.message.ends_with("used the source text")
                        || finding.message.ends_with("used source text"),
                    "{finding:?}"
                );
                let reason_separator = if finding.message.starts_with("html:") {
                    "; image:"
                } else {
                    "; rendition failed:"
                };
                assert!(
                    finding.message.contains(reason_separator),
                    "{finding:?}"
                );
            } else {
                assert!(
                    finding.message.ends_with("used an image")
                        || finding
                            .message
                            .ends_with("used an equation picture"),
                    "{finding:?}"
                );
                assert!(
                    finding
                        .message
                        .contains("was ignored during MathML export"),
                    "{finding:?}"
                );
            }
        }
    }
    let page = fs::read_to_string(output(&report, Target::Html)).unwrap();
    assert_no_katex(&page);
    let html = Html::parse_document(&page);
    assert_eq!(html.select(&Selector::parse(".math img").unwrap()).count(), 2);
    assert_eq!(
        html.select(&Selector::parse(".math code.math-error").unwrap()).count(),
        2
    );
    let parts = docx_parts(&output(&report, Target::Docx));
    let document = parse(&math_xml::part(&parts, "word/document.xml"));
    let sections = sections(&document);
    let source_style = style_id(&parts, "Source Code");
    assert!(document.descendants("m:oMath").is_empty());
    assert_eq!(document.descendants("wp:inline").len(), 2);
    assert_eq!(document.descendants("asvg:svgBlip").len(), 2);
    assert!(document.descendants("w:position").is_empty());
    for fixture in &fixtures {
        let nodes = &sections[&format!("CASE-{}", fixture.name)];
        if fixture.name.starts_with("29-") || fixture.name.starts_with("30-") {
            let paragraphs =
                nodes.iter().flat_map(|node| node.descendants("w:p"));
            assert!(
                paragraphs.into_iter().any(|node| {
                    node.text_content() == fixture.source
                        && node
                            .child("w:pPr")
                            .and_then(|props| props.child("w:pStyle"))
                            .and_then(|style| style.attr("w:val"))
                            == Some(source_style.as_str())
                }),
                "{}",
                fixture.name
            );
        } else {
            let pictures = nodes
                .iter()
                .flat_map(|node| node.descendants("wp:docPr"))
                .collect::<Vec<_>>();
            assert_eq!(pictures.len(), 1, "{}", fixture.name);
            assert_eq!(
                pictures[0].attr("descr"),
                Some(fixture.source.as_str())
            );
        }
    }
    let world = worlds.get(None, &options).unwrap();
    assert_eq!(world.source(world.main()).unwrap().text(), source);
}

fn image_geometry(style: &str) -> (f64, f64) {
    let pattern =
        Regex::new(r"^height:([0-9.]+)pt;vertical-align:-([0-9.]+)pt$")
            .unwrap();
    let captures = pattern.captures(style).unwrap();
    (captures[1].parse().unwrap(), captures[2].parse().unwrap())
}

fn assert_glyph_svg(bytes: &[u8]) {
    let svg = std::str::from_utf8(bytes).unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("<path"));
    assert!(!svg.contains("<text"));
    assert!(!svg.contains("font-family"));
}

fn assert_inline_html_geometry(style: &str) {
    // math.md §3.4/§4.2 freezes the 11 pt native rendition at height 13.242 pt,
    // baseline 9.447 pt; the HTML body scales both dimensions by 12/11.
    let (height, descent) = image_geometry(style);
    assert!((height - 14.45).abs() <= 0.01, "height {height}");
    assert!((descent - 4.14).abs() <= 0.01, "descent {descent}");
}

#[test]
fn native_image_modes_materialize_svg_png_and_keep_inline_baselines() {
    let source = concat!(
        "Inline $x^2 + 1/2$.\n\n",
        "$ integral_0^1 x^2 dif x = 1/3 $\n",
    );
    let root = project(source, "[math]\nhtml='image'\n[docx]\nmath='image'\n");
    let options = options(root.path());
    let mut worlds = WorldSlot::new();
    let report = pipeline::build_targets(
        &options,
        &[Target::Html, Target::Docx],
        &mut worlds,
    );
    let prepared = assert_success(&report);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let page = fs::read_to_string(output(&report, Target::Html)).unwrap();
    assert_no_katex(&page);
    assert!(!page.contains("id=\"writer-math\""));
    let html = Html::parse_document(&page);
    let images =
        html.select(&Selector::parse(".math img").unwrap()).collect::<Vec<_>>();
    assert_eq!(images.len(), 2);
    let mut standalone = BTreeSet::new();
    for image in &images {
        let encoded = image
            .value()
            .attr("src")
            .unwrap()
            .strip_prefix("data:image/svg+xml;base64,")
            .unwrap();
        let bytes =
            base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        assert_glyph_svg(&bytes);
        standalone.insert(bytes);
    }
    assert_eq!(standalone.len(), 2);
    assert_eq!(images[0].value().attr("alt"), Some("x^2 + 1/2"));
    assert_inline_html_geometry(images[0].value().attr("style").unwrap());
    let parts = docx_parts(&output(&report, Target::Docx));
    let document = parse(&math_xml::part(&parts, "word/document.xml"));
    assert!(document.descendants("m:oMath").is_empty());
    let drawings = document.descendants("wp:inline");
    assert_eq!(drawings.len(), 2);
    assert_eq!(document.descendants("asvg:svgBlip").len(), 2);
    let extents = document.descendants("wp:extent");
    assert_eq!(extents.len(), 2);
    for (extent, (width, height)) in
        extents.iter().zip([(31.317, 13.242), (65.719, 29.159)])
    {
        let actual_width =
            extent.attr("cx").unwrap().parse::<f64>().unwrap() / 12700.0;
        let actual_height =
            extent.attr("cy").unwrap().parse::<f64>().unwrap() / 12700.0;
        assert!((actual_width - width).abs() <= 0.02, "width {actual_width}");
        assert!(
            (actual_height - height).abs() <= 0.02,
            "height {actual_height}"
        );
    }
    let positions = document.descendants("w:position");
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].attr("w:val"), Some("-8"));
    let relationships =
        parse(&math_xml::part(&parts, "word/_rels/document.xml.rels"));
    let png_id = drawings[0].descendants("a:blip")[0].attr("r:embed").unwrap();
    let png_target = relationships
        .children
        .iter()
        .find(|node| node.attr("Id") == Some(png_id))
        .unwrap()
        .attr("Target")
        .unwrap();
    assert!(png_target.starts_with("media/"));
    let size =
        imagesize::blob_size(&parts[&format!("word/{png_target}")]).unwrap();
    assert_eq!((size.width, size.height), (63, 26));
    assert_eq!(
        parts
            .keys()
            .filter(|name| name.starts_with("word/media/")
                && name.ends_with(".png"))
            .count(),
        2
    );
    assert_eq!(
        parts
            .keys()
            .filter(|name| name.starts_with("word/media/")
                && name.ends_with(".svg"))
            .count(),
        2
    );

    let theme = pipeline::resolve_theme(prepared, &options).unwrap();
    let site_root = root.path().join("site");
    let world = worlds.get(None, &options).unwrap();
    let main = world.main();
    let (site, _) = pipeline::html::render(
        prepared,
        &theme,
        OutputMode::Site {
            assets_dir: site_root.join("assets"),
            url_prefix: "assets/".into(),
        },
        BuildInfo::build(false),
        world,
    )
    .unwrap();
    assert!(site.diagnostics.is_empty());
    assert_eq!(world.main(), main);
    let html = Html::parse_document(&site.html);
    let mut site_equations = BTreeSet::new();
    for image in html.select(&Selector::parse(".math img").unwrap()) {
        let url = image.value().attr("src").unwrap();
        assert!(url.starts_with("assets/eq-") && url.ends_with(".svg"));
        let path = site_root.join(url);
        let file = site.files.iter().find(|file| file.path == path).unwrap();
        assert_glyph_svg(&file.bytes);
        site_equations.insert(file.bytes.clone());
    }
    assert_eq!(site_equations, standalone);
    let (served, assets) = pipeline::html::render(
        prepared,
        &theme,
        OutputMode::Serve,
        BuildInfo::serve(7),
        world,
    )
    .unwrap();
    assert!(served.diagnostics.is_empty());
    assert!(served.files.is_empty());
    assert_eq!(world.main(), main);
    assert_eq!(world.source(main).unwrap().text(), source);
    let html = Html::parse_document(&served.html);
    let mut serve_equations = BTreeSet::new();
    for image in html.select(&Selector::parse(".math img").unwrap()) {
        let url = image.value().attr("src").unwrap();
        let (id, version) = url
            .strip_prefix("/__writer/doc-asset/")
            .unwrap()
            .split_once("?v=")
            .unwrap();
        assert_eq!(id.len(), 12);
        assert!(!version.is_empty());
        let bytes = assets.read(id).unwrap();
        assert_glyph_svg(&bytes);
        serve_equations.insert(bytes);
    }
    assert_eq!(serve_equations, standalone);
    assert!(prepared.assets.is_empty());
}

fn mixed_source() -> String {
    let macros = BTreeMap::from([(r"\RR".into(), r"\mathbb{Z}".into())]);
    // The native compiler needs the definitions in its actual mi argument.
    // The subset frontend removes precisely the configured leading block.
    let latex = writer::math::latex_source::for_typst(r"\RR", &macros).0;
    let literal = serde_json::to_string(&latex).unwrap();
    format!(
        concat!(
            "#import \"@preview/mitex:0.2.7\": mi\n",
            "Native-start-marker.\n\n",
            "Native code $#text[kept code] + x$.\n\n",
            "Native symbol $RR$.\n\n",
            "LaTeX macro #mi({literal}).\n\n",
            "#pagebreak()\nNative-end-marker.\n",
        ),
        literal = literal
    )
}

fn mixed_settings(html: &str, docx: &str) -> String {
    format!(
        concat!(
            "[math]\nhtml='{html}'\n",
            "[math.katex_options.macros]\n'\\RR'='\\mathbb{{Z}}'\n",
            "[docx]\nmath='{docx}'\n",
        ),
        html = html,
        docx = docx
    )
}

#[test]
fn native_and_latex_macros_keep_separate_mathml_and_word_routes() {
    let root = project(&mixed_source(), &mixed_settings("mathml", "omml"));
    let options = options(root.path());
    let report = pipeline::build_targets(
        &options,
        &[Target::Html, Target::Docx],
        &mut WorldSlot::new(),
    );
    assert_success(&report);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let page = fs::read_to_string(output(&report, Target::Html)).unwrap();
    assert_no_katex(&page);
    assert_native_css(&page, true);
    let html = Html::parse_document(&page);
    let equations =
        html.select(&Selector::parse(".math").unwrap()).collect::<Vec<_>>();
    assert_eq!(equations.len(), 3);
    assert_eq!(
        equations[0].value().attr("data-tex"),
        Some("#text[kept code] + x")
    );
    assert_eq!(equations[1].value().attr("data-tex"), Some("RR"));
    assert_eq!(equations[2].value().attr("data-tex"), Some(r"\RR"));
    assert_eq!(equations[1].text().collect::<String>(), "ℝ");
    assert_eq!(equations[2].text().collect::<String>(), "ℤ");
    assert_eq!(html.select(&Selector::parse(".math math").unwrap()).count(), 3);
    let parts = docx_parts(&output(&report, Target::Docx));
    let document = parse(&math_xml::part(&parts, "word/document.xml"));
    let equations = document.descendants("m:oMath");
    assert_eq!(equations.len(), 3);
    assert!(equations[0].text_content().contains("kept code"));
    assert!(!equations[0].descendants("m:nor").is_empty());
    assert_eq!(equations[1].text_content(), "R");
    assert_eq!(equations[2].text_content(), "Z");
    for equation in &equations[1..] {
        assert!(
            equation
                .descendants("m:scr")
                .iter()
                .any(|style| style.attr("m:val") == Some("double-struck"))
        );
    }
    assert!(document.descendants("wp:inline").is_empty());
}

#[test]
fn both_target_orders_retain_native_main_for_subsequent_pdf_after_math_rendering()
 {
    let source = mixed_source();
    for (html, docx) in [("katex", "omml"), ("image", "image")] {
        let root = project(&source, &mixed_settings(html, docx));
        let options = options(root.path());
        let mut pdfs = Vec::new();
        for targets in [
            [Target::Html, Target::Docx, Target::Pdf],
            [Target::Pdf, Target::Docx, Target::Html],
        ] {
            let initial = pipeline::prepare(&options).unwrap();
            assert!(
                !initial.diagnostics.has_errors(),
                "{:?}",
                initial.diagnostics
            );
            let mut worlds = WorldSlot::new();
            let main = worlds.get(Some(&initial), &options).unwrap().main();
            let report =
                pipeline::build_targets(&options, &targets, &mut worlds);
            let prepared = assert_success(&report);
            assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
            assert_eq!(
                report
                    .targets
                    .iter()
                    .map(|result| result.target)
                    .collect::<Vec<_>>(),
                targets
            );
            let page =
                fs::read_to_string(output(&report, Target::Html)).unwrap();
            let html_tree = Html::parse_document(&page);
            assert_eq!(
                html_tree.select(&Selector::parse(".math").unwrap()).count(),
                3
            );
            if html == "katex" {
                assert_native_css(&page, true);
                assert_eq!(
                    html_tree
                        .select(&Selector::parse(".math math").unwrap())
                        .count(),
                    2
                );
                let client = html_tree
                    .select(&Selector::parse(".math[data-math-style]").unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(client.len(), 1);
                assert_eq!(client[0].value().attr("data-tex"), Some(r"\RR"));
                assert_eq!(
                    page.matches("window.__writerKatexOptions =").count(),
                    1
                );
                assert_eq!(
                    page.matches("window.__writerRenderMath =").count(),
                    1
                );
                assert!(page.contains(r#""\\RR":"\\mathbb{Z}""#));
            } else {
                assert_no_katex(&page);
                assert_eq!(
                    html_tree
                        .select(&Selector::parse(".math img").unwrap())
                        .count(),
                    3
                );
                assert!(!page.contains("id=\"writer-math\""));
            }
            let parts = docx_parts(&output(&report, Target::Docx));
            let document = parse(&math_xml::part(&parts, "word/document.xml"));
            if docx == "omml" {
                assert_eq!(document.descendants("m:oMath").len(), 3);
                assert!(document.descendants("wp:inline").is_empty());
                assert!(!document.descendants("m:nor").is_empty());
            } else {
                assert!(document.descendants("m:oMath").is_empty());
                assert_eq!(document.descendants("wp:inline").len(), 3);
            }
            let pdf = fs::read(output(&report, Target::Pdf)).unwrap();
            assert!(pdf.starts_with(b"%PDF-"));
            assert_eq!(pipeline::pdf::page_count(&pdf), Some(2));
            let world = worlds.get(None, &options).unwrap();
            assert_eq!(world.main(), main);
            assert_eq!(world.source(main).unwrap().text(), source);
            let subsequent = root.path().join("subsequent.pdf");
            let (written, diagnostics) = pipeline::typst_source::build(
                prepared, &options, &subsequent, world,
            )
            .unwrap();
            assert_eq!(written.as_slice(), std::slice::from_ref(&subsequent));
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            assert_eq!(fs::read(&subsequent).unwrap(), pdf);
            assert_eq!(world.main(), main);
            assert_eq!(world.source(main).unwrap().text(), source);
            pdfs.push(pdf);
        }
        assert_eq!(pdfs[0], pdfs[1], "{html}/{docx}");
    }
}
