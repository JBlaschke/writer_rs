//! Known LaTeX template failures retain the template's own path and line.

use std::path::{Path, PathBuf};

use writer::check::{CheckOptions, run};
use writer::config::{Config, Target};
use writer::error::{Diagnostics, Level};
use writer::pipeline::BuildOptions;

fn project() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("document.md"),
        "Body 😀.\n\n```{=html}\n<b>Foreign to Typst</b>\n```\n",
    )
    .unwrap();
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.pdf.timestamp = "compiler setup must remain unused".into();
    config.latex.template = "broken.tex".into();
    std::fs::write(
        directory.path().join("writer.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    directory
}

fn findings(root: &Path) -> Diagnostics {
    let mut options = BuildOptions::new(root.join("document.md"));
    options.config_path = Some(root.join("writer.toml"));
    options.cli.targets = Some(vec![Target::Latex, Target::Typst]);
    run(&options, &CheckOptions::default()).unwrap_or_else(|failure| {
        panic!("{}: {:?}", failure.error, failure.diagnostics)
    })
}

fn template_error(
    diagnostics: &Diagnostics, path: PathBuf, line: Option<usize>,
) {
    let errors = diagnostics
        .iter()
        .filter(|finding| finding.level == Level::Error)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1, "{diagnostics:?}");
    assert_eq!(errors[0].path.as_deref(), Some(path.as_path()));
    assert_eq!(errors[0].span.as_ref().map(|span| span.start_line), line);
    assert_eq!(errors[0].span.as_ref().and_then(|span| span.cell), None);
    assert!(errors[0].message.contains("broken.tex"));
    assert!(
        diagnostics.iter().any(|finding| {
            finding.code == "raw-skipped"
                && finding.message.starts_with("typst:")
        }),
        "later targets still run: {diagnostics:?}"
    );
}

#[test]
fn malformed_latex_template_is_located_in_its_own_file() {
    let directory = project();
    let template = directory.path().join("broken.tex");
    std::fs::write(&template, "\\VAR{preamble}\n\\VAR{body}\n\\VAR{").unwrap();
    let diagnostics = findings(directory.path());
    template_error(&diagnostics, template, Some(3));
    assert!(diagnostics.iter().any(|finding| finding.code == "template-error"));
    assert!(!diagnostics.iter().any(|finding| finding.code == "latex-unicode"));
    assert!(!directory.path().join("dist").exists());
}

#[test]
fn undefined_latex_variable_keeps_template_location_and_prior_body_findings() {
    let directory = project();
    let template = directory.path().join("broken.tex");
    std::fs::write(
        &template,
        "\\VAR{preamble}\n\\VAR{body}\n\\VAR{vars.absent}",
    )
    .unwrap();
    let diagnostics = findings(directory.path());
    template_error(&diagnostics, template, Some(3));
    let prior = diagnostics
        .iter()
        .filter(|finding| finding.code == "latex-unicode")
        .collect::<Vec<_>>();
    assert_eq!(prior.len(), 1, "{diagnostics:?}");
    assert_eq!(prior[0].level, Level::Warning);
    assert_eq!(
        prior[0].path.as_deref(),
        Some(directory.path().join("document.md").as_path())
    );
    assert_eq!(prior[0].span.as_ref().unwrap().start_line, 1);
    assert!(!directory.path().join("dist").exists());
}

#[test]
fn unreadable_latex_template_keeps_resource_path_without_an_invented_line() {
    let directory = project();
    let diagnostics = findings(directory.path());
    template_error(&diagnostics, directory.path().join("broken.tex"), None);
    assert!(diagnostics.iter().any(|finding| finding.code == "resource-error"));
    assert!(!directory.path().join("dist").exists());
}
