//! LaTeX-to-MathML contracts and the shared 37-equation corpus.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use writer::config::Config;
use writer::error::Level;
use writer::math::latex_mathml::{
    Converter, MathError, macro_diagnostics, to_mathml,
};
use writer::math::latex_source::{macros_from_config, prepare};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/math")
}

fn macros() -> BTreeMap<String, String> {
    let config = Config::load(&fixture_dir().join("macros.toml")).unwrap();
    macros_from_config(&config.math.katex_options)
}

fn fixtures() -> Vec<(String, bool, String)> {
    let mut paths: Vec<_> = fs::read_dir(fixture_dir().join("latex"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "tex"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 37, "keep the complete shared LaTeX corpus");
    paths
        .iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let text = fs::read_to_string(path).unwrap();
            assert!(text.ends_with('\n'), "{name}: final newline");
            let (mode, body) = text.split_once('\n').unwrap();
            let display = match mode {
                "% mode: inline" => false,
                "% mode: display" => true,
                other => panic!("{name}: invalid mode {other:?}"),
            };
            let body = body.strip_suffix('\n').unwrap_or(body);
            let body = body
                .strip_prefix('$')
                .and_then(|s| s.strip_suffix('$'))
                .unwrap_or(body);
            (name, display, body.to_owned())
        })
        .collect()
}

fn convert(
    name: &str, source: &str, display: bool,
) -> Result<String, MathError> {
    let macros = if name.starts_with("31-") {
        macros()
    } else {
        BTreeMap::new()
    };
    to_mathml(&prepare(source).0, display, &macros)
}

#[test]
fn latex_corpus() {
    let mut snapshot = String::new();
    let mut failed = Vec::new();
    for (name, display, source) in fixtures() {
        let mode = if display { "display" } else { "inline" };
        let result = match convert(&name, &source, display) {
            Ok(mathml) => mathml,
            Err(MathError::Syntax { message, range: Some(range), .. }) => {
                failed.push(name.clone());
                format!("ERR {message} at bytes {}..{}", range.start, range.end)
            }
            Err(error) => panic!("{name}: unexpected error shape: {error:?}"),
        };
        snapshot.push_str(&format!("{name} [{mode}]: {result}\n"));
    }
    assert_eq!(failed, ["18-unknown-command", "19-unbalanced-brace", "32-ce",]);
    insta::assert_snapshot!(snapshot);
}

#[test]
fn display_and_inline_forms() {
    let macros = BTreeMap::new();
    let inline = to_mathml(r"\frac{a}{b}", false, &macros).unwrap();
    let display = to_mathml(r"\frac{a}{b}", true, &macros).unwrap();
    assert_eq!(
        inline,
        "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mfrac><mi>a</mi><mi>b</mi></mfrac></math>"
    );
    assert_eq!(
        display,
        "<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\"><mfrac><mi>a</mi><mi>b</mi></mfrac></math>"
    );
    assert_eq!(
        to_mathml("", false, &macros).unwrap(),
        "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"></math>"
    );
}

#[test]
fn syntax_error_shape() {
    let error = to_mathml(r"\foo{x}", true, &BTreeMap::new()).unwrap_err();
    let MathError::Syntax { message, range, html } = error else {
        panic!("expected syntax error");
    };
    assert_eq!(message, r#"Unknown command "\foo"."#);
    assert_eq!(range, Some(0..4));
    assert_eq!(
        html,
        r#"<span class="math-error" title="0: Unknown command &quot;\foo&quot;."><code>\foo{x}</code></span>"#
    );
    let error = to_mathml(r"\frac{a}{b", false, &BTreeMap::new()).unwrap_err();
    assert!(
        matches!(error, MathError::Syntax { range: Some(range), .. } if range == (10..10))
    );
}

#[test]
fn no_html_only_entities() {
    for (name, display, source) in fixtures() {
        if let Ok(mathml) = convert(&name, &source, display) {
            for entity in [
                "&ApplyFunction;", "&InvisibleTimes;", "&InvisibleComma;",
                "&nbsp;",
            ] {
                assert!(!mathml.contains(entity), "{name}: {entity}: {mathml}");
            }
            if name.starts_with("11-") {
                assert!(mathml.contains("&#x2061;"), "{mathml}");
            }
            let mut reader = quick_xml::Reader::from_str(&mathml);
            loop {
                match reader.read_event() {
                    Ok(quick_xml::events::Event::Eof) => break,
                    Ok(_) => {}
                    Err(error) => {
                        panic!("{name}: non-XML MathML: {error}: {mathml}")
                    }
                }
            }
        }
    }
}

#[test]
fn macro_names_and_retry() {
    let macros = BTreeMap::from([
        (r"\RR".into(), r"\mathbb{R}".into()),
        (r"\half".into(), r"\frac{#1}{2}".into()),
        (r"\bad".into(), r"\nosuch{#1}".into()),
        ("nobackslash".into(), "x".into()),
        (r"\R R".into(), "x".into()),
    ]);
    let (converter, diagnostics) = Converter::new(&macros);
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
    for finding in &diagnostics {
        assert_eq!(finding.code, "math-macro-invalid");
        assert_eq!(finding.level, Level::Warning);
        assert_eq!(finding.span, None);
        assert!(finding.message.starts_with("[math] katex_options.macros["));
    }
    for key in [r"\bad", "nobackslash", r"\R R"] {
        assert!(
            diagnostics.iter().any(|d| d.message.contains(key)),
            "missing {key}: {diagnostics:?}"
        );
    }
    let mathml = converter.to_mathml(r"\RR^n + \half{x}", false).unwrap();
    assert!(mathml.contains("<mi>ℝ</mi>"), "{mathml}");
    assert!(mathml.contains("<mfrac><mi>x</mi><mn>2</mn></mfrac>"), "{mathml}");
    assert_eq!(macro_diagnostics(&macros), diagnostics);
}

#[test]
fn cache_follows_the_macro_map() {
    for (replacement, expected) in
        [(r"\mathbb{R}", "ℝ"), (r"\mathbf{R}", "𝐑"), (r"\mathbb{R}", "ℝ")]
    {
        let macros = BTreeMap::from([(r"\RR".into(), replacement.into())]);
        let mathml = to_mathml(r"\RR", false, &macros).unwrap();
        assert!(mathml.contains(expected), "{mathml}");
        assert!(macro_diagnostics(&macros).is_empty());
    }
}

#[test]
fn local_equation_definitions_do_not_leak_through_the_cached_converter() {
    let (converter, diagnostics) = Converter::new(&BTreeMap::new());
    assert!(diagnostics.is_empty());
    let mathml =
        converter.to_mathml(r"\def\mylocal{x} \mylocal", false).unwrap();
    assert!(mathml.contains("<mi>x</mi>"), "{mathml}");
    assert!(matches!(
        converter.to_mathml(r"\mylocal", false),
        Err(MathError::Syntax { .. })
    ));
}

#[test]
fn converter_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Converter>();
}
