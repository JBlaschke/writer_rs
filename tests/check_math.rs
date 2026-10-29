//! Cheap math checking uses source provenance and target-visible notebook data.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use writer::check::{CheckOptions, run};
use writer::config::{Config, HtmlMath, Target};
use writer::error::{Diagnostic, Diagnostics, Level};
use writer::pipeline::{self, BuildOptions, Mode};
use writer::typst::emit::{AssetPaths, EmitOptions, emit};

fn project(source: &str, config: &Config) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "document.md", source);
    write(directory.path(), "writer.toml", &toml::to_string(config).unwrap());
    directory
}

fn write(root: &Path, name: &str, source: &str) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

fn options(
    root: &Path, name: &str, targets: Option<Vec<Target>>,
) -> BuildOptions {
    let mut options = BuildOptions::new(root.join(name));
    options.config_path = Some(root.join("writer.toml"));
    options.cli.targets = targets;
    options
}

fn checked(options: &BuildOptions) -> Diagnostics {
    run(options, &CheckOptions::default()).unwrap_or_else(|failure| {
        panic!("check failed: {}: {:#?}", failure.error, failure.diagnostics);
    })
}

fn findings<'a>(
    diagnostics: &'a Diagnostics, code: &str,
) -> Vec<&'a Diagnostic> {
    diagnostics.iter().filter(|diagnostic| diagnostic.code == code).collect()
}

fn math_only(diagnostics: &Diagnostics) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.starts_with("math-"))
        .collect()
}

fn pdf_config() -> Config {
    let mut config = Config::default();
    config.build.targets = vec![Target::Pdf];
    config.pdf.system_fonts = false;
    config
}

fn notebook(cells: Vec<Value>) -> String {
    json!({"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":cells})
        .to_string()
}

fn code_cell(tags: &[&str], data: Value) -> Value {
    json!({
        "cell_type":"code", "id":"result", "metadata":{"tags":tags},
        "source":"result = 1\n", "execution_count":1,
        "outputs":[{"output_type":"display_data","metadata":{},"data":data}]
    })
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_owned();
            if path.is_dir() {
                result.insert(relative, None);
                pending.push(path);
            } else {
                result.insert(relative, Some(std::fs::read(path).unwrap()));
            }
        }
    }
    result
}

#[test]
fn baseline_syntax_is_checked_for_every_html_route_without_pdf_preflight() {
    for route in [HtmlMath::Katex, HtmlMath::Mathml, HtmlMath::Image] {
        let mut config = Config::default();
        config.math.html = route;
        let directory = project("$\\foo{x}$\n", &config);
        let diagnostics =
            checked(&options(directory.path(), "document.md", None));
        let syntax = findings(&diagnostics, "math-syntax");
        assert_eq!(syntax.len(), 1, "{route:?}: {diagnostics:?}");
        assert!(syntax[0].message.contains("foo"));
        assert!(syntax[0].message.contains("at bytes "));
        assert_eq!(
            syntax[0].message.contains("KaTeX may still render it"),
            route == HtmlMath::Katex
        );
        let conversions = findings(&diagnostics, "math-conversion-failed");
        if route == HtmlMath::Image {
            assert_eq!(conversions.len(), 1, "{diagnostics:?}");
            assert!(conversions[0].message.starts_with("html:"));
            assert_eq!(conversions[0].level, Level::Warning);
            assert_eq!(conversions[0].path, syntax[0].path);
            assert_eq!(conversions[0].span, syntax[0].span);
        } else {
            assert!(conversions.is_empty(), "{diagnostics:?}");
        }
    }
}

#[test]
fn effective_pdf_defaults_and_cli_overrides_choose_preflight_exactly_once() {
    let directory = project("$\\foo{x}$\n", &pdf_config());
    for targets in
        [None, Some(vec![Target::Pdf]), Some(vec![Target::Pdf, Target::Pdf])]
    {
        let diagnostics =
            checked(&options(directory.path(), "document.md", targets));
        assert_eq!(findings(&diagnostics, "math-syntax").len(), 1);
        assert_eq!(findings(&diagnostics, "math-conversion-failed").len(), 1);
        assert!(
            math_only(&diagnostics)
                .iter()
                .all(|diagnostic| diagnostic.level == Level::Warning)
        );
    }
    let diagnostics = checked(&options(
        directory.path(),
        "document.md",
        Some(vec![Target::Html]),
    ));
    assert_eq!(findings(&diagnostics, "math-syntax").len(), 1);
    assert!(findings(&diagnostics, "math-conversion-failed").is_empty());
}

#[test]
fn all_parser_rejections_and_the_tolerated_unbalanced_brace_keep_both_diagnoses()
 {
    let source = concat!(
        "$\\foo{x}$\n\n",
        "$\\ce{H2O}$\n\n",
        "$\\sideset{_1^2}{_3^4}\\sum$\n\n",
        "$\\frac{a}{b$\n",
    );
    let directory = project(source, &pdf_config());
    let diagnostics = checked(&options(directory.path(), "document.md", None));
    // Sideset is valid math-core input even though the PDF parser rejects it.
    assert_eq!(
        findings(&diagnostics, "math-syntax").len(),
        3,
        "{diagnostics:?}"
    );
    let pdf = findings(&diagnostics, "math-conversion-failed");
    assert_eq!(pdf.len(), 3, "{diagnostics:?}");
    assert_eq!(
        pdf.iter()
            .map(|diagnostic| diagnostic.span.as_ref().unwrap().start_line)
            .collect::<Vec<_>>(),
        [1, 3, 5]
    );
    assert!(pdf.iter().all(|diagnostic| {
        diagnostic.message.starts_with("pdf: cannot convert equation (")
            && diagnostic.message.ends_with("; used the source text")
    }));
    assert!(diagnostics.iter().all(|diagnostic| !matches!(
        diagnostic.code,
        "compile-error" | "parse-error"
    )));
}

#[test]
fn configured_macros_tags_and_environment_preparation_match_the_pdf_emitter() {
    let mut config = pdf_config();
    config.math.katex_options.insert(
        "macros".into(),
        json!({
            "\\RR":"\\mathbb{R}", "\\half":"\\frac{#1}{2}"
        }),
    );
    let source = concat!(
        "$\\RR^n + \\half{x}$\n\n",
        "$$x = 1 \\tag{3} \\label{eq:inside}$$\n\n",
        "$$\\begin{align}a &= b \\\\ c &= d\\end{align}$$\n\n",
        "$\\foo{`x`}$\n",
    );
    let directory = project(source, &config);
    let options = options(directory.path(), "document.md", None);
    let diagnostics = checked(&options);
    assert_eq!(
        findings(&diagnostics, "math-syntax").len(),
        1,
        "{diagnostics:?}"
    );
    assert_eq!(findings(&diagnostics, "math-tag-ignored").len(), 1);
    assert!(findings(&diagnostics, "math-macro-invalid").is_empty());
    let mut preparation = options.clone();
    preparation.mode = Mode::Check;
    let prepared = pipeline::prepare(&preparation).unwrap();
    let emitted = emit(
        &prepared.document,
        &prepared.assets,
        &EmitOptions {
            config: &prepared.config,
            assets: AssetPaths::Virtual,
            template: None,
            bibliography: Vec::new(),
            style: "ieee".into(),
            footnote_prefix: String::new(),
            fragment: false,
        },
    )
    .unwrap();
    for code in ["math-conversion-failed", "math-tag-ignored"] {
        let actual = findings(&diagnostics, code);
        let expected = findings(&emitted.diagnostics, code);
        assert!(!actual.is_empty(), "fixture must exercise {code}");
        assert_eq!(actual, expected, "{code}");
    }
}

#[test]
fn invalid_macro_warnings_are_once_only_and_equation_local_definitions_do_not_leak()
 {
    let mut config = pdf_config();
    config
        .math
        .katex_options
        .insert("macros".into(), json!({"\\bad-name":"x"}));
    let directory = project("$x$\n\n$x$\n\n$x$\n", &config);
    let diagnostics = checked(&options(directory.path(), "document.md", None));
    let macro_findings = findings(&diagnostics, "math-macro-invalid");
    assert_eq!(macro_findings.len(), 1, "{diagnostics:?}");
    assert_eq!(macro_findings[0].span.as_ref().unwrap().start_line, 1);
    assert!(macro_findings[0].message.contains("\\bad-name"));
    let directory =
        project("$\\newcommand{\\zz}{x}\\zz$\n\n$\\zz$\n", &pdf_config());
    let diagnostics = checked(&options(directory.path(), "document.md", None));
    for code in ["math-syntax", "math-conversion-failed"] {
        let findings = findings(&diagnostics, code);
        assert_eq!(findings.len(), 1, "{diagnostics:?}");
        assert_eq!(findings[0].span.as_ref().unwrap().start_line, 3);
    }
}

#[test]
fn equal_source_strings_at_distinct_nodes_are_not_deduplicated() {
    let directory = project("$\\foo{x}$ and **$\\foo{x}$**.\n", &pdf_config());
    let diagnostics = checked(&options(
        directory.path(),
        "document.md",
        Some(vec![Target::Pdf, Target::Docx, Target::Pdf]),
    ));
    assert_eq!(findings(&diagnostics, "math-syntax").len(), 2);
    assert_eq!(findings(&diagnostics, "math-conversion-failed").len(), 2);
}

#[test]
fn nested_source_content_and_notes_are_checked_once_in_their_enclosing_blocks()
{
    let source = concat!(
        "---\nabstract: |\n  Abstract $\\zzAbstract{x}$.\n---\n\n",
        "# Heading $\\zzHeading{x}$\n\n",
        "Strong **$\\zzStrong{x}$** and [link $\\zzLink{x}$](https://example.com).\n\n",
        "> Quote $\\zzQuote{x}$.\n\n",
        "- List $\\zzList{x}$.\n\n",
        ":::note\nCallout $\\zzCallout{x}$.\n:::\n\n",
        "Term $\\zzTerm{x}$\n: Definition $\\zzDefinition{x}$.\n\n",
        "| Header $\\zzTable{x}$ |\n| --- |\n| Cell $\\zzCell{x}$ |\n\n",
        "Table: Caption $\\zzCaption{x}$.\n\n",
        "Note^[Footnote $\\zzFootnote{x}$.].\n",
    );
    let directory = project(source, &pdf_config());
    let diagnostics = checked(&options(directory.path(), "document.md", None));
    let expected_path =
        std::fs::canonicalize(directory.path().join("document.md")).unwrap();
    for name in [
        "zzAbstract", "zzHeading", "zzStrong", "zzLink", "zzQuote", "zzList",
        "zzCallout", "zzTerm", "zzDefinition", "zzTable", "zzCell",
        "zzCaption", "zzFootnote",
    ] {
        for code in ["math-syntax", "math-conversion-failed"] {
            let found = findings(&diagnostics, code)
                .into_iter()
                .filter(|diagnostic| diagnostic.message.contains(name))
                .collect::<Vec<_>>();
            assert_eq!(found.len(), 1, "{name}/{code}: {diagnostics:?}");
            assert!(found[0].span.is_some());
            let actual_path =
                std::fs::canonicalize(found[0].path.as_ref().unwrap()).unwrap();
            assert_eq!(actual_path, expected_path);
        }
    }
}

#[test]
fn included_markdown_and_notebook_locations_keep_file_line_and_cell() {
    let directory =
        project(":::include parts/section.md\n:::\n", &pdf_config());
    write(directory.path(), "parts/section.md", "Included.\n\n$\\foo{x}$\n");
    let diagnostics = checked(&options(directory.path(), "document.md", None));
    let expected =
        std::fs::canonicalize(directory.path().join("parts/section.md"))
            .unwrap();
    for code in ["math-syntax", "math-conversion-failed"] {
        let found = findings(&diagnostics, code);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path.as_ref(), Some(&expected));
        let span = found[0].span.as_ref().unwrap();
        assert_eq!(span.start_line, 3);
        assert_eq!(span.cell, None);
        assert_eq!(span.file.as_ref(), Some(&expected));
    }
    let book = notebook(vec![
        json!({"cell_type":"markdown","metadata":{},"source":"First.\n"}),
        json!({"cell_type":"markdown","metadata":{},"source":"Cell two.\n\n$\\foo{x}$\n"}),
    ]);
    write(directory.path(), "cells.ipynb", &book);
    let diagnostics = checked(&options(directory.path(), "cells.ipynb", None));
    for code in ["math-syntax", "math-conversion-failed"] {
        let found = findings(&diagnostics, code);
        assert_eq!(found.len(), 1);
        let span = found[0].span.as_ref().unwrap();
        assert_eq!((span.cell, span.start_line), (Some(1), 3));
        assert!(found[0].path.as_ref().unwrap().ends_with("cells.ipynb"));
    }
}

#[test]
fn notebook_candidate_union_is_target_visible_and_deduplicates_shared_nodes() {
    let mut config = pdf_config();
    config.notebook.mime_priority =
        vec!["text/markdown".into(), "text/plain".into(), "text/latex".into()];
    let directory = project("unused", &config);
    let data = json!({
        "text/markdown":"$\\foo{x}$\n\n```{=html}\n<p>HTML only</p>\n```\n",
        "text/plain":"$\\ce{H2O}$", "text/latex":"\\sideset{_1^2}{_3^4}\\sum"
    });
    let mut visible = code_cell(&[], data.clone());
    visible["metadata"]["writer"] = json!({"output":"asis"});
    write(directory.path(), "outputs.ipynb", &notebook(vec![visible]));
    for (targets, syntax_count, pdf_count, has_foo, has_ce) in [
        (vec![Target::Html], 1, 0, true, false),
        (vec![Target::Pdf], 1, 1, false, true),
        (vec![Target::Html, Target::Pdf], 2, 1, true, true),
        (vec![Target::Docx, Target::Pdf, Target::Pdf], 1, 1, false, true),
        (Vec::new(), 0, 0, false, false),
    ] {
        let diagnostics = checked(&options(
            directory.path(),
            "outputs.ipynb",
            Some(targets.clone()),
        ));
        let syntax = findings(&diagnostics, "math-syntax");
        assert_eq!(syntax.len(), syntax_count, "{targets:?}: {diagnostics:?}");
        assert_eq!(
            findings(&diagnostics, "math-conversion-failed").len(),
            pdf_count,
            "{targets:?}"
        );
        assert_eq!(
            syntax
                .iter()
                .any(|diagnostic| diagnostic.message.contains("\\foo")),
            has_foo
        );
        assert_eq!(
            syntax.iter().any(|diagnostic| diagnostic.message.contains("\\ce")),
            has_ce
        );
        for diagnostic in &syntax {
            assert_eq!(
                diagnostic.message.contains("KaTeX may still render it"),
                diagnostic.message.contains("\\foo")
                    && targets.contains(&Target::Html)
            );
        }
        assert!(
            findings(&diagnostics, "math-conversion-failed")
                .iter()
                .all(|diagnostic| diagnostic.message.contains("ce")
                    && !diagnostic.message.contains("foo"))
        );
        assert!(math_only(&diagnostics).iter().all(|diagnostic| {
            diagnostic.span.as_ref().unwrap().cell == Some(0)
        }));
    }
    let hidden = code_cell(&["hide-output"], data);
    write(directory.path(), "hidden.ipynb", &notebook(vec![hidden]));
    let diagnostics = checked(&options(
        directory.path(),
        "hidden.ipynb",
        Some(vec![Target::Html, Target::Pdf]),
    ));
    assert!(math_only(&diagnostics).is_empty(), "{diagnostics:?}");
}

#[test]
fn empty_target_source_syntax_and_latex_only_tag_preservation_are_independent()
{
    let directory = project("$\\foo{x}$\n\n$$x=1\\tag{3}$$\n", &pdf_config());
    for targets in [vec![Target::Latex], Vec::new()] {
        let diagnostics =
            checked(&options(directory.path(), "document.md", Some(targets)));
        assert_eq!(findings(&diagnostics, "math-syntax").len(), 1);
        assert!(findings(&diagnostics, "math-conversion-failed").is_empty());
        assert!(findings(&diagnostics, "math-tag-ignored").is_empty());
    }
    for (target, name) in [(Target::Html, "html"), (Target::Docx, "docx")] {
        let diagnostics = checked(&options(
            directory.path(),
            "document.md",
            Some(vec![target]),
        ));
        let tags = findings(&diagnostics, "math-tag-ignored");
        assert_eq!(tags.len(), 1);
        assert!(tags[0].message.contains(&format!("ignored for {name}")));
    }
    let diagnostics = checked(&options(
        directory.path(),
        "document.md",
        Some(vec![Target::Pdf, Target::Html]),
    ));
    let tags = findings(&diagnostics, "math-tag-ignored");
    assert_eq!(tags.len(), 1);
    assert!(tags[0].message.contains("ignored for pdf"));
}

#[test]
fn native_typst_equations_are_not_sent_to_either_latex_parser_or_compiler() {
    let mut config = pdf_config();
    config.pdf.template = "cannot-load-this-template.typ".into();
    let directory = project("unused", &config);
    write(directory.path(), "native.typ", "$ foo(x) + planck.reduce $\n");
    let before = inventory(directory.path());
    let diagnostics = checked(&options(directory.path(), "native.typ", None));
    assert!(math_only(&diagnostics).is_empty(), "{diagnostics:?}");
    assert_eq!(inventory(directory.path()), before);
}

#[test]
fn checking_pdf_math_needs_no_compiler_resources_and_writes_nothing() {
    let mut config = pdf_config();
    config.pdf.template = "unavailable/template.typ".into();
    config.pdf.font_paths = vec!["unavailable/fonts".into()];
    config.typst.package_paths = vec!["unavailable/packages".into()];
    config.build.out_dir = "unavailable/output".into();
    let directory = project("$x^2$\n\n$\\foo{x}$\n", &config);
    let mut options = options(directory.path(), "document.md", None);
    options.cli.out = Some(directory.path().join("unavailable/override"));
    let before = inventory(directory.path());
    let first = checked(&options);
    let second = checked(&options);
    assert_eq!(first, second);
    assert_eq!(options.mode, Mode::Build);
    assert_eq!(findings(&first, "math-conversion-failed").len(), 1);
    assert_eq!(inventory(directory.path()), before);
    assert!(!directory.path().join("dist").exists());
    assert!(!directory.path().join("unavailable").exists());
}
