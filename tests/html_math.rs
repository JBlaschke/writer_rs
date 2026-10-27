//! HTML equation routes, fallbacks, assets and standalone CLI output.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use regex::Regex;
use scraper::{Html, Selector};
use serde_json::json;
use typst::foundations::Datetime;
use writer::assets::registry::{AssetData, AssetRegistry};
use writer::config::{Config, HtmlMath};
use writer::error::{Diagnostics, Level};
use writer::html::emit::{
    EmitOptions, Emitted, MathHook, MathRequest, MathUsage, emit,
};
use writer::html::math::MathRenderer;
use writer::ir::{MathDialect, SourceSpan};
use writer::math::latex_source::macros_from_config;
use writer::pipeline::{self, BuildOptions, WorldSlot};
use writer::typst::world::{MainFile, WorldOptions, WriterWorld, world_inputs};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).into()
}

fn fixture_dir() -> PathBuf {
    root().join("tests/fixtures/math")
}

fn config(route: HtmlMath) -> Config {
    let mut config = Config::default();
    config.math.html = route;
    config.pdf.system_fonts = false;
    config
}

fn world(root: &Path, config: &Config) -> WriterWorld {
    WriterWorld::new(WorldOptions {
        root: root.into(),
        main: MainFile::Virtual,
        today: Datetime::from_ymd(2026, 10, 25).unwrap(),
        system_fonts: false,
        font_paths: Vec::new(),
        package_paths: Vec::new(),
        allow_packages: false,
        inputs: world_inputs(config),
    })
    .unwrap()
}

fn span() -> SourceSpan {
    SourceSpan {
        start_line: 9,
        end_line: 11,
        cell: Some(2),
        file: Some(PathBuf::from("/docs/cells.ipynb")),
    }
}

struct Equation {
    html: String,
    usage: MathUsage,
    assets: AssetRegistry,
    diagnostics: Diagnostics,
}

fn render_one(
    config: &Config, source: &str, dialect: MathDialect, display: bool,
) -> Equation {
    let root = tempfile::tempdir().unwrap();
    let mut world = world(root.path(), config);
    let mut renderer = MathRenderer::new(config, &mut world);
    let mut assets = AssetRegistry::new();
    let mut diagnostics = Diagnostics::new();
    let span = span();
    let html = renderer.equation(
        &MathRequest { source, dialect, display, span: Some(&span) },
        &mut assets,
        &mut diagnostics,
    );
    Equation { html, usage: renderer.usage(), assets, diagnostics }
}

fn expect_findings(eq: &Equation, code: &str, count: usize) {
    assert_eq!(eq.diagnostics.len(), count, "{:?}", eq.diagnostics);
    for finding in eq.diagnostics.iter() {
        assert_eq!(finding.code, code);
        assert_eq!(finding.level, Level::Warning);
        assert_eq!(finding.span, Some(span()));
        assert_eq!(finding.path, span().file);
    }
}

fn snapshot(main: &str, diagnostics: &Diagnostics) -> String {
    let ids = Regex::new(r"asset:[0-9a-f]{12}").unwrap();
    let mut text = ids.replace_all(main, "asset:[id]").into_owned();
    text.push_str("\n--- diagnostics ---\n");
    for finding in diagnostics.iter() {
        text.push_str(&format!(
            "{} {}: {}\n",
            finding.level, finding.code, finding.message
        ));
    }
    text
}

fn math_md(route: HtmlMath) -> (Emitted, AssetRegistry, Diagnostics) {
    let opts = BuildOptions::new(root().join("tests/fixtures/md/math.md"));
    let mut prepared = pipeline::prepare(&opts).unwrap();
    prepared.config.pdf.system_fonts = false;
    let mut config = prepared.config.clone();
    config.math.html = route;
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    let mut renderer = MathRenderer::new(&config, world);
    let mut assets = prepared.assets.clone();
    let mut diagnostics = prepared.diagnostics.clone();
    let emitted = emit(
        &prepared.document,
        &mut EmitOptions {
            config: &config,
            positions: false,
            includes: &prepared.document.includes,
            math: &mut renderer,
            assets: &mut assets,
            diagnostics: &mut diagnostics,
        },
    );
    (emitted, assets, diagnostics)
}

fn assert_math_md(route: HtmlMath, expected: MathUsage) -> String {
    let (emitted, assets, diagnostics) = math_md(route);
    assert_eq!(emitted.math, expected);
    let document = Html::parse_fragment(&emitted.main);
    let selector = Selector::parse("span.math").unwrap();
    let equations: Vec<_> = document.select(&selector).collect();
    // The original six equations are followed by six Markdown edge cases.
    assert_eq!(equations.len(), 12);
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == "math-tag-ignored").count(),
        1
    );
    assert!(!diagnostics.iter().any(|d| d.code == "math-macro-invalid"));
    match route {
        HtmlMath::Katex => {
            assert!(assets.is_empty());
            for eq in equations {
                assert!(eq.value().attr("data-math-style").is_some());
                let source = eq.value().attr("data-tex").unwrap();
                assert_eq!(eq.text().collect::<String>(), source);
                assert!(!source.contains(r"\tag"));
                assert!(!source.contains(r"\label"));
            }
        }
        HtmlMath::Mathml => {
            assert_eq!(emitted.main.matches("<math xmlns=").count(), 12);
            assert!(!emitted.main.contains("data-math-style"));
            assert!(assets.is_empty());
        }
        HtmlMath::Image => {
            assert_eq!(emitted.main.matches("<img src=\"asset:").count(), 12);
            assert_eq!(assets.len(), 12);
            for asset in assets.iter() {
                assert_eq!(asset.mime, "image/svg+xml");
                let AssetData::Bytes(bytes) = &asset.data else {
                    panic!("equation asset must be in memory")
                };
                assert!(std::str::from_utf8(bytes).unwrap().contains("<svg"));
            }
        }
    }
    snapshot(&emitted.main, &diagnostics)
}

#[test]
fn math_md_katex() {
    let text = assert_math_md(
        HtmlMath::Katex,
        MathUsage { katex: true, ..Default::default() },
    );
    insta::assert_snapshot!(text);
}

#[test]
fn math_md_mathml() {
    let text = assert_math_md(
        HtmlMath::Mathml,
        MathUsage { mathml: true, ..Default::default() },
    );
    insta::assert_snapshot!(text);
}

#[test]
fn math_md_image() {
    let text = assert_math_md(
        HtmlMath::Image,
        MathUsage { image: true, ..Default::default() },
    );
    insta::assert_snapshot!(text);
}

#[test]
fn latex_corpus_katex() {
    let mut paths: Vec<_> = fs::read_dir(fixture_dir().join("latex"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "tex"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 37);
    let macros_config =
        Config::load(&fixture_dir().join("macros.toml")).unwrap();
    let mut text = String::new();
    for path in paths {
        let name = path.file_stem().unwrap().to_string_lossy();
        let input = fs::read_to_string(&path).unwrap();
        let (mode, source) = input.split_once('\n').unwrap();
        let display = mode == "% mode: display";
        let mode = if display { "display" } else { "inline" };
        let source = source.strip_suffix('\n').unwrap_or(source);
        let source = source
            .strip_prefix('$')
            .and_then(|s| s.strip_suffix('$'))
            .unwrap_or(source);
        let config = if name.starts_with("31-") {
            macros_config.clone()
        } else {
            config(HtmlMath::Katex)
        };
        let eq = render_one(&config, source, MathDialect::Latex, display);
        assert_eq!(eq.usage, MathUsage { katex: true, ..Default::default() });
        assert!(eq.assets.is_empty());
        text.push_str(&format!("{name} [{mode}]: {}\n", eq.html));
        for finding in eq.diagnostics.iter() {
            text.push_str(&format!(
                "  {} {}: {}\n",
                finding.level, finding.code, finding.message
            ));
        }
    }
    insta::assert_snapshot!(text);
}

#[test]
fn image_geometry() {
    let eq = render_one(
        &config(HtmlMath::Image),
        "x^2 + 1/2",
        MathDialect::Typst,
        false,
    );
    assert_eq!(eq.usage, MathUsage { image: true, ..Default::default() });
    assert!(eq.diagnostics.is_empty(), "{:?}", eq.diagnostics);
    let document = Html::parse_fragment(&eq.html);
    let selector = Selector::parse("img").unwrap();
    let image = document.select(&selector).next().unwrap();
    assert_eq!(image.value().attr("alt"), Some("x^2 + 1/2"));
    let geometry =
        Regex::new(r"^height:([0-9.]+)pt;vertical-align:-([0-9.]+)pt$")
            .unwrap();
    let captures =
        geometry.captures(image.value().attr("style").unwrap()).unwrap();
    let height: f64 = captures[1].parse().unwrap();
    let descent: f64 = captures[2].parse().unwrap();
    assert!((height - 14.45).abs() <= 0.01, "height {height}");
    assert!((descent - 4.14).abs() <= 0.01, "descent {descent}");
    assert_eq!(eq.assets.len(), 1);
    assert_eq!(eq.assets.iter().next().unwrap().file_name_hint, "eq-1.svg");
}

#[test]
fn typst_math_is_mathml() {
    for route in [HtmlMath::Katex, HtmlMath::Mathml] {
        let eq =
            render_one(&config(route), "frac(a, b)", MathDialect::Typst, true);
        assert!(
            eq.html
                .contains("<math display=\"block\"><mfrac><mi>𝑎</mi><mi>𝑏</mi></mfrac></math>"),
            "{}",
            eq.html
        );
        assert!(eq.html.contains("data-tex=\"frac(a, b)\""));
        assert!(!eq.html.contains("data-math-style"));
        assert_eq!(
            eq.usage,
            MathUsage { typst_mathml: true, ..Default::default() }
        );
        assert!(eq.diagnostics.is_empty(), "{:?}", eq.diagnostics);
        assert!(eq.assets.is_empty());
    }
}

#[test]
fn typst_mathml_failure_uses_image() {
    let eq = render_one(
        &config(HtmlMath::Katex),
        "overline(x)",
        MathDialect::Typst,
        false,
    );
    expect_findings(&eq, "math-conversion-failed", 1);
    let finding = eq.diagnostics.iter().next().unwrap();
    assert!(
        finding.message.contains("was ignored during MathML export"),
        "{finding:?}"
    );
    assert!(finding.message.ends_with("used an image"), "{finding:?}");
    assert!(eq.html.contains("<img src=\"asset:"));
    assert_eq!(eq.usage, MathUsage { image: true, ..Default::default() });
    assert_eq!(eq.assets.len(), 1);
}

#[test]
fn double_failure_one_diagnostic() {
    let eq = render_one(
        &config(HtmlMath::Mathml),
        "foo(x)",
        MathDialect::Typst,
        false,
    );
    expect_findings(&eq, "math-conversion-failed", 1);
    assert!(eq.html.contains("<code class=\"math-error\">foo(x)</code>"));
    let finding = eq.diagnostics.iter().next().unwrap();
    assert!(finding.message.contains("unknown variable: foo"), "{finding:?}");
    assert!(finding.message.contains("; image:"), "{finding:?}");
    assert!(finding.message.ends_with("used the source text"), "{finding:?}");
    assert_eq!(eq.usage, MathUsage::default());
    assert!(eq.assets.is_empty());
}

#[test]
fn image_failure_uses_source() {
    let eq = render_one(
        &config(HtmlMath::Image),
        r"\foo{x}",
        MathDialect::Latex,
        true,
    );
    expect_findings(&eq, "math-conversion-failed", 1);
    assert!(eq.html.contains(r#"<code class="math-error">\foo{x}</code>"#));
    assert!(
        eq.diagnostics
            .iter()
            .next()
            .unwrap()
            .message
            .ends_with("used the source text")
    );
    assert_eq!(eq.usage, MathUsage::default());
    assert!(eq.assets.is_empty());
}

#[test]
fn mathml_syntax_error() {
    let eq = render_one(
        &config(HtmlMath::Mathml),
        r"\foo{x}",
        MathDialect::Latex,
        false,
    );
    expect_findings(&eq, "math-syntax", 1);
    assert_eq!(
        eq.diagnostics.iter().next().unwrap().message,
        r#"invalid LaTeX math: Unknown command "\foo". at bytes 0..4 of the equation"#
    );
    assert!(eq.html.contains(r#"<span class="math-error" title="0: Unknown command &quot;\foo&quot;."><code>\foo{x}</code></span>"#));
    assert!(!eq.html.contains("data-math-style"));
    assert!(eq.assets.is_empty());
}

#[test]
fn macro_diagnostics_once() {
    let mut config = config(HtmlMath::Mathml);
    config.math.katex_options =
        serde_json::from_value(json!({"macros": {"\\bad": "\\nosuch{#1}"}}))
            .unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut world = world(root.path(), &config);
    let mut renderer = MathRenderer::new(&config, &mut world);
    let mut assets = AssetRegistry::new();
    let mut diagnostics = Diagnostics::new();
    let span = span();
    for source in ["x", "y"] {
        let html = renderer.equation(
            &MathRequest {
                source,
                dialect: MathDialect::Latex,
                display: false,
                span: Some(&span),
            },
            &mut assets,
            &mut diagnostics,
        );
        assert!(html.contains("<math xmlns="), "{html}");
    }
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let finding = diagnostics.iter().next().unwrap();
    assert_eq!(finding.code, "math-macro-invalid");
    assert_eq!(finding.span, Some(span));
    assert!(assets.is_empty());
}

#[test]
fn mixed_dialects_accumulate_the_representations_actually_emitted() {
    let config = config(HtmlMath::Katex);
    let root = tempfile::tempdir().unwrap();
    let mut world = world(root.path(), &config);
    let mut renderer = MathRenderer::new(&config, &mut world);
    let mut assets = AssetRegistry::new();
    let mut diagnostics = Diagnostics::new();
    for (source, dialect) in [
        (r"\frac{a}{b}", MathDialect::Latex),
        ("frac(a, b)", MathDialect::Typst),
    ] {
        renderer.equation(
            &MathRequest { source, dialect, display: false, span: None },
            &mut assets,
            &mut diagnostics,
        );
    }
    assert_eq!(
        renderer.usage(),
        MathUsage { katex: true, typst_mathml: true, ..Default::default() }
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(assets.is_empty());
}

#[test]
fn katex_keeps_configured_macros_for_client_side_rendering() {
    let mut config = config(HtmlMath::Katex);
    config.math.katex_options =
        serde_json::from_value(json!({"macros": {"\\RR": "\\mathbb{R}"}}))
            .unwrap();
    let original = config.clone();
    let eq = render_one(&config, r"\RR < x & y", MathDialect::Latex, false);
    assert_eq!(config, original);
    assert_eq!(
        macros_from_config(&config.math.katex_options),
        BTreeMap::from([(r"\RR".into(), r"\mathbb{R}".into())])
    );
    assert!(eq.html.contains(r#"data-tex="\RR &lt; x &amp; y""#));
    assert!(eq.html.contains(r">\RR &lt; x &amp; y</span>"));
    assert!(eq.diagnostics.is_empty());
}

#[test]
fn math_js_contract() {
    let script = writer::vendor::get_str("writer/math.js").unwrap();
    assert!(script.len() < 2_048, "{} bytes", script.len());
    for required in [
        "__writerRenderMath", "__writerKatexOptions", ":not([data-rendered])",
        "throwOnError: false", "DOMContentLoaded", "el.textContent",
        "data-rendered", "console.warn",
    ] {
        assert!(script.contains(required), "missing {required}: {script}");
    }
    assert!(!script.contains("presenter"));
}

#[test]
fn build_per_route() {
    for (route, font) in [
        ("katex", "embed"),
        ("mathml", "embed"),
        ("mathml", "system"),
        ("image", "embed"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::copy(
            root().join("tests/fixtures/md/math.md"),
            dir.path().join("math.md"),
        )
        .unwrap();
        fs::write(
            dir.path().join("writer.toml"),
            format!(
                "[math]\nhtml = '{route}'\nmathml_font = '{font}'\n[pdf]\nsystem_fonts = false\n"
            ),
        )
        .unwrap();
        Command::new(env!("CARGO_BIN_EXE_writer"))
            .current_dir(dir.path())
            .env_remove("RUST_LOG")
            .env_remove("NO_COLOR")
            .env_remove("CLICOLOR_FORCE")
            .env_remove("SOURCE_DATE_EPOCH")
            .args(["build", "math.md"])
            .assert()
            .success();
        let page =
            fs::read_to_string(dir.path().join("dist/math.html")).unwrap();
        assert!(!page.contains("asset:"));
        match route {
            "katex" => {
                assert!(page.contains("__writerRenderMath"));
                assert!(page.contains("window.__writerKatexOptions"));
                assert!(!page.contains("<math xmlns"));
            }
            "mathml" => {
                assert_eq!(page.matches("<math xmlns").count(), 12);
                assert!(!page.contains("katex.min"));
                if font == "embed" {
                    assert!(page.contains("data:font/woff2;base64,"));
                } else {
                    assert!(!page.contains("@font-face"));
                }
            }
            "image" => {
                assert_eq!(
                    page.matches("data:image/svg+xml;base64,").count(),
                    12
                );
                assert!(!page.contains("id=\"writer-math\""));
                assert!(!page.contains("__writerRenderMath"));
                assert!(!page.contains("katex.min"));
            }
            _ => unreachable!(),
        }
    }
}
