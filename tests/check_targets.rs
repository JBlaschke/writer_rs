//! Target-aware checking retains source findings and creates no target outputs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use writer::check::{CheckOptions, run};
use writer::config::{Config, Target};
use writer::error::{Diagnostic, Diagnostics, Level};
use writer::pipeline::BuildOptions;

fn project(source: &str, config: &Config) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("document.md"), source).unwrap();
    std::fs::write(
        directory.path().join("writer.toml"),
        toml::to_string(config).unwrap(),
    )
    .unwrap();
    directory
}

fn check(root: &Path, targets: Option<Vec<Target>>) -> Diagnostics {
    let mut opts = BuildOptions::new(root.join("document.md"));
    opts.config_path = Some(root.join("writer.toml"));
    opts.cli.targets = targets;
    run(&opts, &CheckOptions::default()).unwrap_or_else(|failure| {
        panic!("{}: {:?}", failure.error, failure.diagnostics)
    })
}

fn findings<'a>(
    diagnostics: &'a Diagnostics, code: &str,
) -> Vec<&'a Diagnostic> {
    diagnostics.iter().filter(|finding| finding.code == code).collect()
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut output = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_owned();
            if path.is_dir() {
                output.insert(relative, None);
                pending.push(path);
            } else {
                output.insert(relative, Some(std::fs::read(path).unwrap()));
            }
        }
    }
    output
}

#[test]
fn every_target_and_their_union_leave_the_complete_project_unchanged() {
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    let directory = project("# A report\n\nText.\n", &config);
    let before = inventory(directory.path());
    for targets in [
        vec![Target::Html],
        vec![Target::Pdf],
        vec![Target::Docx],
        vec![Target::Latex],
        vec![Target::Typst],
        vec![
            Target::Html,
            Target::Pdf,
            Target::Docx,
            Target::Latex,
            Target::Typst,
        ],
    ] {
        let diagnostics = check(directory.path(), Some(targets));
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(inventory(directory.path()), before);
    }
}

#[test]
fn configured_targets_and_explicit_override_control_raw_losses() {
    let mut config = Config::default();
    config.build.targets = vec![Target::Typst];
    let directory = project("```{=html}\n<b>Raw</b>\n```\n", &config);
    let configured = check(directory.path(), None);
    let raw = findings(&configured, "raw-skipped");
    assert_eq!(raw.len(), 1, "{configured:?}");
    assert!(raw[0].message.starts_with("typst:"));
    assert_eq!(raw[0].level, Level::Info);
    let explicit = check(directory.path(), Some(vec![Target::Html]));
    assert!(findings(&explicit, "raw-skipped").is_empty(), "{explicit:?}");
    let repeated =
        check(directory.path(), Some(vec![Target::Typst, Target::Typst]));
    assert_eq!(findings(&repeated, "raw-skipped").len(), 1);
}

#[test]
fn pdf_and_typst_share_one_preflight_using_first_selected_spelling() {
    let directory = project("$\\foo{x}$\n", &Config::default());
    for targets in
        [vec![Target::Typst, Target::Pdf], vec![Target::Pdf, Target::Typst]]
    {
        let expected = if targets[0] == Target::Pdf {
            "pdf:"
        } else {
            "typst:"
        };
        let diagnostics = check(directory.path(), Some(targets));
        assert_eq!(findings(&diagnostics, "math-syntax").len(), 1);
        let conversion = findings(&diagnostics, "math-conversion-failed");
        assert_eq!(conversion.len(), 1, "{diagnostics:?}");
        assert!(conversion[0].message.starts_with(expected));
        assert_eq!(conversion[0].level, Level::Warning);
    }
}

#[test]
fn baseline_source_checks_survive_an_empty_target_list() {
    let mut config = Config::default();
    config.build.targets = vec![];
    let directory = project("$\\foo{x}$\n\n![Absent](missing.png)\n", &config);
    let diagnostics = check(directory.path(), Some(vec![]));
    assert_eq!(findings(&diagnostics, "math-syntax").len(), 1);
    let missing = findings(&diagnostics, "missing-asset");
    assert_eq!(missing.len(), 1, "{diagnostics:?}");
    assert_eq!(missing[0].level, Level::Warning);
    assert!(findings(&diagnostics, "math-conversion-failed").is_empty());
}

#[test]
fn target_template_errors_do_not_prevent_other_target_checks() {
    let mut config = Config::default();
    config.latex.template = "missing.tex".into();
    let directory = project("```{=html}\n<p>HTML</p>\n```\n", &config);
    let diagnostics =
        check(directory.path(), Some(vec![Target::Latex, Target::Typst]));
    assert_eq!(findings(&diagnostics, "resource-error").len(), 1);
    let raw = findings(&diagnostics, "raw-skipped");
    assert_eq!(raw.len(), 1, "{diagnostics:?}");
    assert!(raw[0].message.starts_with("typst:"));
}

#[test]
fn native_pdf_ignored_template_is_reported_without_reading_or_compiling_it() {
    let mut config = Config::default();
    config.pdf.template = "unread-template.typ".into();
    config.pdf.font_paths = vec!["unavailable-font-directory".into()];
    config.pdf.system_fonts = false;
    let directory = project("unused\n", &config);
    let input = directory.path().join("native.typ");
    std::fs::write(&input, "Hello\n").unwrap();
    let before = inventory(directory.path());
    let mut opts = BuildOptions::new(&input);
    opts.config_path = Some(directory.path().join("writer.toml"));
    opts.cli.targets = Some(vec![Target::Pdf]);
    let diagnostics = run(&opts, &CheckOptions::default()).unwrap();
    let ignored = findings(&diagnostics, "W-TYP-TEMPLATE-IGNORED");
    assert_eq!(ignored.len(), 1, "{diagnostics:?}");
    assert_eq!(
        ignored[0].message,
        "[pdf] template is ignored for native Typst input; apply the template in the source"
    );
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    assert_eq!(inventory(directory.path()), before);
    opts.cli.targets = Some(vec![Target::Typst]);
    let diagnostics = run(&opts, &CheckOptions::default()).unwrap();
    assert!(findings(&diagnostics, "W-TYP-TEMPLATE-IGNORED").is_empty());
}

#[test]
fn html_page_reference_is_target_specific_and_keeps_its_source_line() {
    let directory = project(
        "# Section {#sec-one}\n\nNormal @sec-one and page @sec-one{.page}.\n",
        &Config::default(),
    );
    let html = check(directory.path(), Some(vec![Target::Html]));
    let unsupported = findings(&html, "unsupported-construct");
    assert_eq!(unsupported.len(), 1, "{html:?}");
    assert!(unsupported[0].message.starts_with("html:"));
    assert_eq!(unsupported[0].span.as_ref().unwrap().start_line, 3);
    let pdf = check(directory.path(), Some(vec![Target::Pdf]));
    assert!(findings(&pdf, "unsupported-construct").is_empty(), "{pdf:?}");
}

#[test]
fn typst_image_and_source_free_bibliography_losses_are_per_target() {
    let directory = project(
        "![Remote](https://example.invalid/image.png)\n\n:::bibliography\n:::\n",
        &Config::default(),
    );
    let diagnostics =
        check(directory.path(), Some(vec![Target::Pdf, Target::Typst]));
    let unsupported = findings(&diagnostics, "unsupported-construct");
    assert_eq!(unsupported.len(), 4, "{diagnostics:?}");
    for target in ["pdf:", "typst:"] {
        let selected: Vec<_> = unsupported
            .iter()
            .filter(|finding| finding.message.starts_with(target))
            .collect();
        assert_eq!(selected.len(), 2, "{diagnostics:?}");
        assert!(
            selected
                .iter()
                .any(|finding| finding.message.contains("cannot fetch"))
        );
        assert!(
            selected
                .iter()
                .any(|finding| finding.message.contains("no sources"))
        );
    }
}

#[test]
fn selected_html_reports_static_theme_problems_without_building_styles() {
    let mut config = Config::default();
    config.html.theme = "broken-theme".into();
    let directory = project("Text.\n", &config);
    let theme = directory.path().join("broken-theme");
    std::fs::create_dir_all(&theme).unwrap();
    std::fs::write(
        theme.join("theme.toml"),
        concat!(
            "name = 'broken-theme'\nextends = 'default'\n",
            "[styles]\nentries = ['missing.css']\n",
        ),
    )
    .unwrap();
    let before = inventory(directory.path());
    let html = check(directory.path(), Some(vec![Target::Html]));
    let missing = findings(&html, "theme-style-entry");
    assert_eq!(missing.len(), 1, "{html:?}");
    assert_eq!(missing[0].level, Level::Error);
    assert_eq!(
        missing[0].path.as_deref(),
        Some(
            std::fs::canonicalize(theme.join("theme.toml")).unwrap().as_path()
        )
    );
    let pdf = check(directory.path(), Some(vec![Target::Pdf]));
    assert!(findings(&pdf, "theme-style-entry").is_empty(), "{pdf:?}");
    assert_eq!(inventory(directory.path()), before);
}

#[test]
fn directive_frontend_findings_are_not_scanned_again_per_target() {
    for (source, expected) in [
        ("Some text\n:::odd: x\n", 1),
        ("```text\n:::odd: x\n```\n", 0),
        ("<div>\n:::odd: x\n</div>\n", 0),
        ("\\:::odd: x\n", 0),
    ] {
        let directory = project(source, &Config::default());
        let diagnostics = check(
            directory.path(),
            Some(vec![Target::Html, Target::Pdf, Target::Latex, Target::Typst]),
        );
        assert_eq!(
            findings(&diagnostics, "invalid-directive").len(),
            expected,
            "{source:?}: {diagnostics:?}"
        );
    }
}

#[test]
fn native_html_checks_actual_visible_equations_without_a_document_pdf_build() {
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    let directory = project("unused\n", &config);
    let input = directory.path().join("native.typ");
    std::fs::write(&input, "$ unknown-function(x) $\n").unwrap();
    let before = inventory(directory.path());
    let mut opts = BuildOptions::new(&input);
    opts.config_path = Some(directory.path().join("writer.toml"));
    opts.cli.targets = Some(vec![Target::Html]);
    let html = run(&opts, &CheckOptions::default()).unwrap();
    let conversion = findings(&html, "math-conversion-failed");
    assert_eq!(conversion.len(), 1, "{html:?}");
    assert!(conversion[0].message.starts_with("html:"));
    assert_eq!(conversion[0].level, Level::Warning);
    assert_eq!(conversion[0].path.as_deref(), Some(input.as_path()));
    assert_eq!(conversion[0].span.as_ref().unwrap().start_line, 1);
    opts.cli.targets = Some(vec![Target::Pdf]);
    let pdf = run(&opts, &CheckOptions::default()).unwrap();
    assert!(findings(&pdf, "math-conversion-failed").is_empty(), "{pdf:?}");
    assert_eq!(inventory(directory.path()), before);
}

#[test]
fn latex_retains_its_distinct_macro_grammar_and_metadata_only_findings() {
    let mut config = Config::default();
    config
        .math
        .katex_options
        .insert("macros".into(), serde_json::json!({"\\bad-name":"x"}));
    let directory = project("$x$\n", &config);
    let diagnostics = check(directory.path(), Some(vec![Target::Latex]));
    let macros = findings(&diagnostics, "math-macro-invalid");
    assert_eq!(macros.len(), 2, "{diagnostics:?}");
    assert_ne!(macros[0].message, macros[1].message);
    assert!(macros.iter().all(|finding| finding.level == Level::Warning));
    std::fs::write(
        directory.path().join("document.md"),
        "---\ntitle: α\n---\n\nPlain body.\n",
    )
    .unwrap();
    let metadata = check(directory.path(), Some(vec![Target::Latex]));
    let macros = findings(&metadata, "math-macro-invalid");
    assert_eq!(macros.len(), 1, "{metadata:?}");
    assert!(macros[0].span.is_none());
    assert_eq!(
        macros[0].path.as_deref(),
        Some(directory.path().join("document.md").as_path())
    );
}

#[test]
fn real_docx_raw_validation_is_selected_without_writing_a_package() {
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    let directory = project("```{=openxml}\n<w:p><w:r></w:p>\n```\n", &config);
    let before = inventory(directory.path());
    let docx = check(directory.path(), Some(vec![Target::Docx]));
    assert_eq!(findings(&docx, "docx-raw-invalid").len(), 1, "{docx:?}");
    let html = check(directory.path(), Some(vec![Target::Html]));
    assert!(findings(&html, "docx-raw-invalid").is_empty(), "{html:?}");
    assert_eq!(findings(&html, "raw-skipped").len(), 1);
    assert_eq!(inventory(directory.path()), before);
}
