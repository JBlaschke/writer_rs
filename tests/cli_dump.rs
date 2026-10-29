//! T16 command-line document IR and HTML template context inspection.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use writer::{config::Config, ir::Document};

fn writer() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_writer"));
    cmd.current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .env_remove("RUST_LOG")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("SOURCE_DATE_EPOCH");
    cmd
}

fn dump(flags: &[&str]) -> String {
    let output = writer()
        .args(["dump", "examples/basic/document.md"])
        .args(flags)
        .assert()
        .success()
        .get_output()
        .clone();
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn dump_ir_is_resolved() {
    let document: Document = serde_json::from_str(&dump(&[])).unwrap();
    assert!(!document.resolved.toc.is_empty());
    for label in ["sec-intro", "sec-method", "sec-next"] {
        assert!(document.resolved.labels[label].number.is_some(), "{label}");
    }
    assert!(document.invariant_violations().is_empty());
}

#[test]
fn dump_no_resolve() {
    let json: Value = serde_json::from_str(&dump(&["--no-resolve"])).unwrap();
    assert!(json.get("resolved").is_none());
    let _: Document = serde_json::from_value(json).unwrap();
}

#[test]
fn dump_context_keys() {
    let json: Value = serde_json::from_str(&dump(&["--context"])).unwrap();
    let keys: Vec<_> =
        json.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "document", "meta", "features", "config", "html", "math", "title",
            "lang", "theme", "build",
        ]
    );
    assert!(
        json["html"]["main"].as_str().unwrap().contains("data-writer-document")
    );
}

#[test]
fn dump_default_config() {
    writer()
        .args(["dump", "--default-config"])
        .assert()
        .success()
        .stdout(Config::default_toml());
    writer()
        .args(["--config", "missing.toml", "dump", "--default-config"])
        .assert()
        .success()
        .stdout(Config::default_toml());
    writer()
        .args(["dump", "examples/basic/document.md", "--default-config"])
        .assert()
        .code(2);
    for flag in ["--ir", "--typst", "--latex", "--context", "--no-resolve"] {
        writer().args(["dump", "--default-config", flag]).assert().code(2);
    }
}

#[test]
fn dump_latex_is_deterministic_source_without_writes() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("document.md"),
        "A dumped paragraph.\n",
    )
    .unwrap();
    let run = || {
        writer()
            .current_dir(directory.path())
            .args(["dump", "document.md", "--latex"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    let first = run();
    assert_eq!(first, run());
    let source = String::from_utf8(first).unwrap();
    assert!(source.contains("\\documentclass"), "{source}");
    assert!(source.contains("\\begin{document}"), "{source}");
    assert!(source.contains("A dumped paragraph."), "{source}");
    let files: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(files, [std::ffi::OsString::from("document.md")]);
}

#[test]
fn dump_flags_conflict() {
    for flags in [
        ["--ir", "--context"],
        ["--typst", "--latex"],
        ["--no-resolve", "--typst"],
        ["--no-resolve", "--context"],
    ] {
        writer()
            .args(["dump", "examples/basic/document.md"])
            .args(flags)
            .assert()
            .code(2);
    }
}

#[test]
fn missing_input_and_unknown_extension_have_distinct_codes() {
    writer()
        .args(["dump", "missing.md"])
        .assert()
        .code(1)
        .stderr(contains("missing.md"));
    writer()
        .args(["dump", "document.txt"])
        .assert()
        .code(2)
        .stderr(contains("document.txt"));
}

#[test]
fn error_diagnostics_still_print_ir_and_exit_1() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("document.md");
    std::fs::write(&input, "# One {#same}\n\n# Two {#same}\n").unwrap();
    let output =
        writer().arg("dump").arg(&input).assert().code(1).get_output().clone();
    let _: Document = serde_json::from_slice(&output.stdout).unwrap();
    assert!(String::from_utf8(output.stderr).unwrap().contains("label"));
}

#[test]
fn dump_typst_is_deterministic_source_without_pdf_writes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("document.md");
    std::fs::write(
        &input,
        "# Dump source\n\n```{=typst}\n#panic(\"not compiled\")\n```\n",
    )
    .unwrap();
    let run = || {
        writer()
            .arg("dump")
            .arg(&input)
            .arg("--typst")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    let first = run();
    assert_eq!(first, run());
    let text = String::from_utf8(first).unwrap();
    assert!(text.contains("writer-doc"));
    assert!(text.contains("not compiled"));
    assert!(!dir.path().join("dist").exists());
}

#[test]
fn dump_typst_warnings_use_stderr_and_keep_source_on_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("document.md");
    std::fs::write(&input, "![Remote image](https://example.org/image.svg)\n")
        .unwrap();
    let output = writer()
        .arg("dump")
        .arg(&input)
        .arg("--typst")
        .assert()
        .success()
        .get_output()
        .clone();
    let source = String::from_utf8(output.stdout).unwrap();
    let findings = String::from_utf8(output.stderr).unwrap();
    assert!(source.contains("https://example.org/image.svg"));
    assert!(!source.contains("cannot fetch image"));
    assert!(findings.contains("cannot fetch image"), "{findings}");
    assert!(!dir.path().join("dist").exists());
}
