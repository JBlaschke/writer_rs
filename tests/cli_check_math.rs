//! T33 command acceptance: PDF parser checks remain cheap and write no output.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};
use writer::config::{Config, Target};

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

fn command(root: &Path, name: &str, quiet: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
    command
        .current_dir(root)
        .env_remove("RUST_LOG")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("SOURCE_DATE_EPOCH");
    if quiet {
        command.arg("-q");
    }
    command.arg("check").arg(root.join(name));
    command
}

fn json_rows(mut command: Command, status: i32) -> Vec<Value> {
    let output = command.args(["--format", "json"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(status),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for row in &rows {
        assert_eq!(
            row.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "level", "code", "message", "file", "line", "cell"
            ])
        );
    }
    rows
}

fn selected(root: &Path, targets: &str, quiet: bool) -> Command {
    let mut command = command(root, "document.md", quiet);
    command.args(["--target", targets]);
    command
}

fn codes(rows: &[Value]) -> Vec<&str> {
    rows.iter().map(|row| row["code"].as_str().unwrap()).collect()
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_owned();
            if path.is_dir() {
                result.insert(relative, Vec::new());
                pending.push(path);
            } else {
                result.insert(relative, std::fs::read(path).unwrap());
            }
        }
    }
    result
}

#[test]
fn explicit_pdf_lists_and_configured_pdf_defaults_run_without_output() {
    let directory = project("$\\frac{a}{b}$\n", &Config::default());
    let before = inventory(directory.path());
    assert!(json_rows(selected(directory.path(), "pdf", false), 0).is_empty());
    assert!(
        json_rows(selected(directory.path(), "pdf,pdf", false), 0).is_empty()
    );
    assert_eq!(inventory(directory.path()), before);
    let mut config = Config::default();
    config.build.targets = vec![Target::Pdf];
    let directory = project("$\\foo{x}$\n", &config);
    let rows = json_rows(command(directory.path(), "document.md", false), 0);
    assert_eq!(codes(&rows), ["math-syntax", "math-conversion-failed"]);
}

#[test]
fn json_warning_reports_are_complete_and_unchanged_by_quiet_or_deny() {
    let directory = project("Before.\n\n$\\foo{x}$\n", &Config::default());
    let ordinary = json_rows(selected(directory.path(), "pdf", false), 0);
    assert_eq!(codes(&ordinary), ["math-syntax", "math-conversion-failed"]);
    for row in &ordinary {
        assert_eq!(row["level"], "warning");
        assert_eq!(row["line"], 3);
        assert!(row["cell"].is_null());
        assert!(row["file"].as_str().unwrap().ends_with("document.md"));
    }
    for quiet in [false, true] {
        assert_eq!(
            json_rows(selected(directory.path(), "pdf", quiet), 0),
            ordinary
        );
        let mut denied = selected(directory.path(), "pdf", quiet);
        denied.arg("--deny-warnings");
        assert_eq!(json_rows(denied, 1), ordinary);
    }
    assert!(!directory.path().join("dist").exists());
}

#[test]
fn text_reports_use_stderr_and_quiet_deny_still_explains_failure() {
    let directory = project("$\\foo{x}$\n", &Config::default());
    let output = selected(directory.path(), "pdf", false).output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cannot convert equation"));
    assert!(stderr.contains("invalid LaTeX math"));
    let quiet = selected(directory.path(), "pdf", true).output().unwrap();
    assert!(quiet.status.success());
    assert!(quiet.stdout.is_empty() && quiet.stderr.is_empty());
    let mut denied = selected(directory.path(), "pdf", true);
    denied.arg("--deny-warnings");
    let output = denied.output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cannot convert equation")
    );
}

#[test]
fn explicit_targets_and_mixed_lists_check_valid_math_without_outputs() {
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.pdf.allow_packages = false;
    let directory = project("$x$\n", &config);
    let before = inventory(directory.path());
    for targets in ["html", "docx", "typst", "latex", "pdf,html", "pdf,docx"] {
        let rows = json_rows(selected(directory.path(), targets, false), 0);
        assert!(rows.is_empty(), "{targets}: {rows:?}");
        assert_eq!(inventory(directory.path()), before, "{targets}");
    }
}

#[test]
fn tolerated_pdf_braces_still_get_syntax_warning_and_configured_macros_are_valid()
 {
    let directory = project("$\\frac{a}{b$\n", &Config::default());
    let rows = json_rows(selected(directory.path(), "pdf", false), 0);
    assert_eq!(codes(&rows), ["math-syntax"]);
    assert!(rows[0]["message"].as_str().unwrap().contains("at bytes "));
    let mut config = Config::default();
    config.math.katex_options.insert(
        "macros".into(),
        json!({
            "\\RR":"\\mathbb{R}", "\\half":"\\frac{#1}{2}"
        }),
    );
    let directory =
        project("$\\RR^n + \\half{x}$\n\n$$x=1\\tag{3}$$\n", &config);
    let rows = json_rows(selected(directory.path(), "pdf", false), 0);
    assert_eq!(codes(&rows), ["math-tag-ignored"]);
    assert!(rows[0]["message"].as_str().unwrap().contains("ignored for pdf"));
    assert_eq!(rows[0]["line"], 3);
}

#[test]
fn json_math_locations_keep_included_files_and_zero_based_notebook_cells() {
    let directory = project(":::include child.md\n:::\n", &Config::default());
    std::fs::write(directory.path().join("child.md"), "Child.\n\n$\\foo{x}$\n")
        .unwrap();
    let rows = json_rows(selected(directory.path(), "pdf", false), 0);
    assert_eq!(codes(&rows), ["math-syntax", "math-conversion-failed"]);
    for row in &rows {
        assert_eq!(row["line"], 3);
        assert!(row["cell"].is_null());
        assert!(row["file"].as_str().unwrap().ends_with("child.md"));
    }
    let notebook = json!({"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":[
        {"cell_type":"markdown","metadata":{},"source":"First.\n"},
        {"cell_type":"markdown","metadata":{},"source":"Cell.\n\n$\\foo{x}$\n"}
    ]});
    std::fs::write(directory.path().join("cells.ipynb"), notebook.to_string())
        .unwrap();
    let mut notebook_check = command(directory.path(), "cells.ipynb", false);
    notebook_check.args(["--target", "pdf"]);
    let rows = json_rows(notebook_check, 0);
    assert_eq!(codes(&rows), ["math-syntax", "math-conversion-failed"]);
    for row in &rows {
        assert_eq!(row["cell"], 1);
        assert_eq!(row["line"], 3);
        assert!(row["file"].as_str().unwrap().ends_with("cells.ipynb"));
    }
}

#[test]
fn explicit_pdf_check_ignores_unavailable_compiler_resources_and_preserves_inventory()
 {
    let mut config = Config::default();
    config.pdf.template = "missing/template.typ".into();
    config.pdf.font_paths = vec!["missing/fonts".into()];
    config.typst.package_paths = vec!["missing/packages".into()];
    config.build.out_dir = "missing/dist".into();
    let directory = project("$x^2$\n", &config);
    let before = inventory(directory.path());
    assert!(json_rows(selected(directory.path(), "pdf", false), 0).is_empty());
    assert_eq!(inventory(directory.path()), before);
    assert!(!directory.path().join("missing").exists());
}
