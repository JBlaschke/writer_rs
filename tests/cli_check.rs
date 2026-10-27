//! T1C subprocess acceptance for linting without rendering or writing outputs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn writer() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
    command
        .current_dir(root())
        .env_remove("RUST_LOG")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("SOURCE_DATE_EPOCH");
    command
}

fn check(input: &Path) -> Command {
    let mut command = writer();
    command.arg("check").arg(input);
    command
}

fn quiet_check(input: &Path) -> Command {
    let mut command = writer();
    command.args(["-q", "check"]).arg(input);
    command
}

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn notebook(cells: Value) -> String {
    json!({
        "nbformat": 4, "nbformat_minor": 5,
        "metadata": {"language_info": {"name": "python"}}, "cells": cells,
    })
    .to_string()
}

fn markdown_cell(source: &str) -> Value {
    json!({"cell_type": "markdown", "id": "markdown", "metadata": {}, "source": source})
}

fn json_output(mut command: Command) -> (Output, Vec<Value>) {
    let output = command.args(["--format", "json"]).output().unwrap();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| {
            panic!(
                "invalid JSON output: {error}\n{}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
    for row in &rows {
        let fields: BTreeSet<_> =
            row.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            fields,
            BTreeSet::from([
                "level", "code", "message", "file", "line", "cell"
            ])
        );
        assert!(
            matches!(row["level"].as_str(), Some("info" | "warning" | "error")),
            "{row}"
        );
        assert!(
            row["code"].as_str().is_some_and(|value| !value.is_empty()),
            "{row}"
        );
        assert!(
            row["message"].as_str().is_some_and(|value| !value.is_empty()),
            "{row}"
        );
        assert!(row["file"].is_null() || row["file"].is_string(), "{row}");
        assert!(
            row["line"].is_null()
                || row["line"].as_u64().is_some_and(|line| line >= 1),
            "{row}"
        );
        assert!(row["cell"].is_null() || row["cell"].is_u64(), "{row}");
    }
    (output, rows)
}

fn checked_json(command: Command, status: i32) -> Vec<Value> {
    let (output, rows) = json_output(command);
    assert_eq!(output.status.code(), Some(status), "{rows:#?}");
    rows
}

fn codes(rows: &[Value]) -> Vec<&str> {
    rows.iter().map(|row| row["code"].as_str().unwrap()).collect()
}

fn diagnostic<'a>(rows: &'a [Value], code: &str) -> &'a Value {
    rows.iter()
        .find(|row| row["code"] == code)
        .unwrap_or_else(|| panic!("missing {code}: {rows:#?}"))
}

fn copy_tree(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == "dist" {
            continue;
        }
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn inventory(base: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(
        base: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Option<Vec<u8>>>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(base).unwrap().to_path_buf();
            if path.is_dir() {
                files.insert(relative, None);
                visit(base, &path, files);
            } else {
                files.insert(relative, Some(std::fs::read(path).unwrap()));
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(base, base, &mut files);
    files
}

#[test]
fn clean_examples_do_not_create_outputs_or_change_sources() {
    let dir = tempfile::tempdir().unwrap();
    for (name, entry) in [
        ("basic", "document.md"),
        ("notebook", "analysis.ipynb"),
        ("mixed", "document.md"),
    ] {
        let copied = dir.path().join(name);
        copy_tree(&root().join("examples").join(name), &copied);
        let before = inventory(&copied);
        let mut command = check(&copied.join(entry));
        command.current_dir(&copied);
        assert!(checked_json(command, 0).is_empty(), "{name}");
        assert_eq!(inventory(&copied), before, "{name}: check wrote files");
    }
}

#[test]
fn missing_assets_warn_and_deny_changes_only_the_exit_status() {
    let dir = project(&[("document.md", "![Unavailable](missing.png)\n")]);
    let input = dir.path().join("document.md");
    let ordinary = checked_json(check(&input), 0);
    let mut denied = check(&input);
    denied.arg("--deny-warnings");
    assert_eq!(checked_json(denied, 1), ordinary);
    let row = diagnostic(&ordinary, "missing-asset");
    assert_eq!(row["level"], "warning");
    assert_eq!(row["line"], 1);
    assert!(row["cell"].is_null());
    assert!(row["file"].as_str().unwrap().ends_with("document.md"));
    assert!(row["message"].as_str().unwrap().contains("missing.png"));
}

#[test]
fn unresolved_references_are_lenient_warnings() {
    let dir = project(&[("document.md", "See @sec-missing.\n")]);
    let rows = checked_json(check(&dir.path().join("document.md")), 0);
    let row = diagnostic(&rows, "unresolved-reference");
    assert_eq!(row["level"], "warning");
    assert_eq!(row["line"], 1);
    assert!(!codes(&rows).contains(&"unknown-citation-key"));
}

#[test]
fn info_is_shown_without_verbose_and_does_not_fail_deny_warnings() {
    let dir = project(&[("document.md", "# A report\n\n--\n\nThe end.\n")]);
    let input = dir.path().join("document.md");
    check(&input)
        .arg("--deny-warnings")
        .assert()
        .success()
        .stdout("")
        .stderr(contains("info:").and(contains("presenter separator")));
    let mut command = check(&input);
    command.arg("--deny-warnings");
    let rows = checked_json(command, 0);
    assert_eq!(codes(&rows), ["presenter-separator"]);
    assert_eq!(rows[0]["level"], "info");
}

#[test]
fn quiet_text_hides_nonfailing_findings_but_explains_denied_warnings() {
    let dir =
        project(&[("document.md", "--\n\n![Unavailable](missing.png)\n")]);
    let input = dir.path().join("document.md");
    quiet_check(&input).assert().success().stdout("").stderr("");
    quiet_check(&input)
        .arg("--deny-warnings")
        .assert()
        .code(1)
        .stdout("")
        .stderr(
            contains("missing.png").and(contains("presenter separator").not()),
        );
    let ordinary = checked_json(check(&input), 0);
    assert_eq!(checked_json(quiet_check(&input), 0), ordinary);
    let mut denied = quiet_check(&input);
    denied.arg("--deny-warnings");
    assert_eq!(checked_json(denied, 1), ordinary);
}

#[test]
fn duplicate_labels_are_errors_and_mixed_findings_have_stable_positions() {
    let dir = project(&[(
        "document.md",
        "# One {#sec-x}\n\nSee @sec-missing.\n\n![Missing](missing.png)\n\n--\n\n# Two {#sec-x}\n",
    )]);
    let input = dir.path().join("document.md");
    let first = checked_json(check(&input), 1);
    assert_eq!(
        codes(&first),
        [
            "unresolved-reference", "missing-asset", "presenter-separator",
            "duplicate-label"
        ]
    );
    assert_eq!(
        first
            .iter()
            .map(|row| row["line"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [3, 5, 7, 9]
    );
    assert_eq!(diagnostic(&first, "duplicate-label")["level"], "error");
    assert_eq!(checked_json(check(&input), 1), first);
    assert_eq!(checked_json(quiet_check(&input), 1), first);
}

#[test]
fn fatal_parse_io_config_and_unknown_source_failures_are_json_errors() {
    let dir = project(&[
        ("missing-include.md", ":::include absent.md\n:::\n"),
        ("broken.ipynb", "{\n"),
        ("broken.md", "---\ntitle: [unterminated\n---\n"),
        ("shape.md", "---\nauthors: scalar\n---\n"),
        ("document.md", "# A report\n"),
        ("document.txt", "A report\n"),
        ("broken.toml", "[markdown\n"),
    ]);
    for (name, code) in [
        ("missing-include.md", "parse-error"),
        ("broken.ipynb", "parse-error"),
        ("broken.md", "parse-error"),
        ("shape.md", "parse-error"),
        ("absent.md", "io-error"),
    ] {
        let rows = checked_json(check(&dir.path().join(name)), 1);
        let row = diagnostic(&rows, code);
        assert_eq!(row["level"], "error", "{name}");
        assert!(row["file"].as_str().unwrap().ends_with(name), "{row}");
        assert!(row["cell"].is_null(), "{row}");
    }
    let rows = checked_json(check(&dir.path().join("document.txt")), 1);
    assert!(rows.iter().any(|row| row["level"] == "error"));
    let mut command = writer();
    command
        .arg("--config")
        .arg(dir.path().join("broken.toml"))
        .arg("check")
        .arg(dir.path().join("document.md"));
    let rows = checked_json(command, 1);
    assert_eq!(diagnostic(&rows, "config-error")["level"], "error");
}

#[test]
fn notebook_fatal_errors_keep_zero_based_cells_and_source_lines() {
    let dir = tempfile::tempdir().unwrap();
    for (name, source) in [
        ("path.ipynb", ":::include \"unterminated\n:::\n"),
        ("yaml.ipynb", "---\ntitle: [unterminated\n---\n"),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, notebook(json!([markdown_cell(source)])))
            .unwrap();
        let rows = checked_json(check(&path), 1);
        let row = diagnostic(&rows, "parse-error");
        assert_eq!(row["cell"], 0, "{row}");
        assert!(row["line"].as_u64().is_some_and(|line| line >= 1), "{row}");
        assert!(row["file"].as_str().unwrap().ends_with(name), "{row}");
    }
}

#[test]
fn included_yaml_failure_keeps_the_child_file() {
    let dir = project(&[
        ("root.md", ":::include child.md\n:::\n"),
        ("child.md", "---\ntitle: [unterminated\n---\n"),
    ]);
    let rows = checked_json(check(&dir.path().join("root.md")), 1);
    let row = diagnostic(&rows, "parse-error");
    assert!(row["file"].as_str().unwrap().ends_with("child.md"), "{row}");
    assert!(row["line"].as_u64().is_some_and(|line| line >= 1), "{row}");
}

#[test]
fn included_markdown_and_notebook_diagnostics_keep_paths_and_cells() {
    let dir = project(&[
        (
            "root.md",
            ":::include part.md\n:::\n\n:::include part.ipynb\n:::\n\n:::include part.md\n:::\n",
        ),
        ("part.md", "![Missing](missing-md.png)\n"),
    ]);
    let json = notebook(json!([
        markdown_cell("# Included notebook\n\n![Missing](missing-nb.png)\n"),
        {"cell_type": "code", "id": "saved", "metadata": {}, "source": "1", "execution_count": 1, "outputs": []},
    ]));
    std::fs::write(dir.path().join("part.ipynb"), json).unwrap();
    let input = dir.path().join("root.md");
    let rows = checked_json(check(&input), 0);
    let assets: Vec<_> =
        rows.iter().filter(|row| row["code"] == "missing-asset").collect();
    assert_eq!(assets.len(), 2, "{rows:#?}");
    assert!(assets[0]["file"].as_str().unwrap().ends_with("part.md"));
    assert_eq!(assets[0]["line"], 1);
    assert!(assets[0]["cell"].is_null());
    assert!(assets[1]["file"].as_str().unwrap().ends_with("part.ipynb"));
    assert_eq!(assets[1]["line"], 3);
    assert_eq!(assets[1]["cell"], 0);
    assert_eq!(checked_json(check(&input), 0), rows);
    check(&input).assert().success().stdout("").stderr(
        contains("part.md").and(contains("part.ipynb")).and(contains("cell 0")),
    );
}

#[test]
fn presenter_compatibility_adds_the_three_frozen_warnings_once() {
    let dir = project(&[
        (
            "document.md",
            "---\ntitle: A report\nauthors: [A. Author]\n---\n\nBefore.\n\n:::include part.md\n:::\n\n:::note title=\"Source: details\"\nBody.\n:::\n",
        ),
        ("part.md", "Included text.\n"),
    ]);
    let input = dir.path().join("document.md");
    let ordinary = checked_json(check(&input), 0);
    assert_eq!(codes(&ordinary), ["directive-colon-rewritten"]);
    assert_eq!(ordinary[0]["level"], "info");
    let mut command = check(&input);
    command.args(["--compat", "presenter"]);
    let rows = checked_json(command, 0);
    assert_eq!(
        codes(&rows).into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "presenter-front-matter", "presenter-include-position",
            "directive-colon-rewritten",
        ])
    );
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row["level"] == "warning"));
    let mut command = check(&input);
    command.args(["--compat", "presenter", "--deny-warnings"]);
    assert_eq!(checked_json(command, 1), rows);
}

#[test]
fn explicit_targets_and_typst_keep_their_milestone_stubs() {
    let dir = project(&[
        ("document.md", "# A report\n"),
        ("document.typ", "Hello\n"),
    ]);
    for target in ["html", "html,docx"] {
        check(&dir.path().join("document.md"))
            .args(["--target", target])
            .assert()
            .code(2)
            .stderr(contains("T71"));
    }
    check(&dir.path().join("document.typ"))
        .assert()
        .code(2)
        .stderr(contains("T50"));
}

fn stable_file(file: &str) -> String {
    let normalize = |value: &str| {
        value.replace('\\', "/").replace("//?/UNC/", "//").replace("//?/", "")
    };
    let mut file = normalize(file);
    for path in [std::fs::canonicalize(root()).unwrap(), root()] {
        let prefix = format!("{}/", normalize(&path.to_string_lossy()));
        if let Some(relative) = file.strip_prefix(&prefix) {
            file = relative.into();
        }
    }
    file.trim_start_matches("./").to_owned()
}

#[test]
fn every_notebook_fixture_has_a_check_diagnostic_snapshot() {
    let dir = root().join("tests/fixtures/ipynb");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "ipynb")
        })
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 8);
    for path in paths {
        let (output, rows) = json_output(check(&path));
        let errors = rows.iter().any(|row| row["level"] == "error");
        assert_eq!(
            output.status.code(),
            Some(i32::from(errors)),
            "{}: {rows:#?}",
            path.display()
        );
        let name = path.file_stem().unwrap().to_string_lossy();
        let expected = match &*name {
            "dataframe" => None,
            "footnotes" => Some("footnote-undefined"),
            "frontmatter" => Some("notebook-presenter-metadata"),
            "includes" => Some("include-meta-ignored"),
            "minimal" => Some("notebook-not-run"),
            "options" => Some("cell-options-invalid"),
            "outputs" => Some("output-invalid"),
            "tags" => Some("unknown-cell-type"),
            other => panic!("add an explicit code assertion for {other}"),
        };
        if let Some(code) = expected {
            diagnostic(&rows, code);
        } else {
            assert!(rows.is_empty(), "{name}: {rows:#?}");
        }
        let tuples: Vec<_> = rows
            .iter()
            .map(|row| {
                json!({
                    "level": row["level"], "code": row["code"],
                    "file": row["file"].as_str().map(stable_file),
                    "line": row["line"], "cell": row["cell"],
                })
            })
            .collect();
        insta::assert_json_snapshot!(format!("notebook_{name}_check"), tuples);
    }
}

#[test]
fn presenter_compatibility_accepts_file_and_separator_include_boundaries() {
    let dir = project(&[("part.md", "Included text.\n")]);
    for (name, source) in [
        ("sole.md", "---\nauthor: A. Author\n---\n\n:::include part.md\n:::\n"),
        (
            "rules.md",
            "Before.\n\n---\n\n:::include part.md\n:::\n\n---\n\nAfter.\n",
        ),
        (
            "code.md",
            "```md\n:::include nonexistent.md\n:::\n:::note title=\"Source: code\"\n```\n\n::::comment\n:::include nonexistent.md\n:::\n::::\n",
        ),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, source).unwrap();
        let mut command = check(&path);
        command.args(["--compat", "presenter"]);
        let rows = checked_json(command, 0);
        assert!(
            !codes(&rows).contains(&"presenter-front-matter"),
            "{name}: {rows:#?}"
        );
        assert!(
            !codes(&rows).contains(&"presenter-include-position"),
            "{name}: {rows:#?}"
        );
        assert!(
            !codes(&rows).contains(&"directive-colon-rewritten"),
            "{name}: {rows:#?}"
        );
    }
}

#[test]
fn compatibility_findings_inside_includes_keep_file_and_cell_provenance() {
    let dir = project(&[
        ("root.md", ":::include part.ipynb\n:::\n"),
        ("part.md", ":::note title=\"Origin: details\"\nBody.\n:::\n"),
    ]);
    let source = "---\nauthors: [A. Author]\n---\n\nBefore.\n\n:::include part.md\n:::\n\nAfter.\n";
    std::fs::write(
        dir.path().join("part.ipynb"),
        notebook(json!([markdown_cell(source)])),
    )
    .unwrap();
    let mut command = check(&dir.path().join("root.md"));
    command.args(["--compat", "presenter"]);
    let rows = checked_json(command, 0);
    for code in ["presenter-front-matter", "presenter-include-position"] {
        let row = diagnostic(&rows, code);
        assert!(row["file"].as_str().unwrap().ends_with("part.ipynb"), "{row}");
        assert_eq!(row["cell"], 0, "{row}");
    }
    let row = diagnostic(&rows, "directive-colon-rewritten");
    assert_eq!(row["level"], "warning");
    assert!(row["file"].as_str().unwrap().ends_with("part.md"), "{row}");
    assert!(row["cell"].is_null(), "{row}");
    assert_eq!(
        rows.iter()
            .filter(|row| row["code"] == "directive-colon-rewritten")
            .count(),
        1
    );
}

#[test]
fn invalid_and_reserved_labels_keep_resolver_severities() {
    let dir = project(&[
        ("invalid.md", "# Invalid {#\"bad label\"}\n"),
        ("reserved.md", "# Reserved {#fn-1}\n"),
    ]);
    let rows = checked_json(check(&dir.path().join("invalid.md")), 1);
    assert_eq!(diagnostic(&rows, "label-invalid")["level"], "error");
    let rows = checked_json(check(&dir.path().join("reserved.md")), 0);
    assert_eq!(diagnostic(&rows, "label-reserved")["level"], "warning");
}
