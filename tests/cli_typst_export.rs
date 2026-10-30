//! Real CLI source export, output reporting and target independence.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::json;

const CONFIG: &str = "[build]\nout_dir='out'\n[pdf]\nsystem_fonts=false\nallow_packages=false\ntimestamp='2026-10-29T00:00:00Z'\n";
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#526d82"/></svg>"##;

fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn project(source: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "document.md", source);
    write(directory.path(), "writer.toml", CONFIG);
    directory
}

fn writer(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
    command
        .current_dir(root)
        .env_remove("RUST_LOG")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("SOURCE_DATE_EPOCH");
    command
}

fn build(root: &Path, input: &str) -> Command {
    let mut command = writer(root);
    command.arg("build").arg(root.join(input)).args(["--to", "typst"]);
    command
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        if !directory.exists() {
            continue;
        }
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    result
}

#[test]
fn default_and_named_dotted_export_paths_have_one_status_per_file_or_directory()
{
    let directory = project("Root.\n\n:::include chapter.md\n:::\n");
    write(directory.path(), "chapter.md", "Chapter.\n\n![Figure](image.svg)\n");
    write(directory.path(), "image.svg", SVG);
    let output = build(directory.path(), "document.md").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let default = directory.path().join("out");
    assert!(default.join("document.typ").is_file());
    assert!(default.join("document_chapter.typ").is_file());
    assert!(default.join("document_assets/image.svg").is_file());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 3, "{stdout}");
    assert!(stdout.lines().all(|line| line.starts_with("Wrote ")));
    assert!(stdout.contains("document_assets") && stdout.contains("1 files"));
    let target = directory.path().join("custom/renamed.final.typ");
    let output = build(directory.path(), "document.md")
        .arg("-o")
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.is_file());
    assert!(target.with_file_name("renamed.final_chapter.typ").is_file());
    assert!(target.with_file_name("renamed.final_assets/image.svg").is_file());
    let text = fs::read_to_string(target).unwrap();
    assert!(text.contains("renamed.final_chapter.typ"));
    assert!(!text.contains("document_assets"));
}

#[test]
fn quiet_and_repeated_exports_keep_identical_bytes_and_preserve_unrelated_files()
 {
    let directory = project("# Stable\n\n![Picture](image.svg)\n");
    write(directory.path(), "image.svg", SVG);
    assert!(
        build(directory.path(), "document.md")
            .output()
            .unwrap()
            .status
            .success()
    );
    let output = directory.path().join("out");
    write(&output, "document_assets/unrelated.bin", b"keep unrelated");
    write(&output, "unrelated.typ", b"keep stale source");
    let before = inventory(&output);
    let result = writer(directory.path())
        .args(["-q", "build", "document.md", "--to", "typst"])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
    assert_eq!(inventory(&output), before);
}

#[test]
fn mixed_targets_treat_out_as_a_directory_and_preserve_other_successes_on_export_failure()
 {
    let directory = project("# Independent targets\n\nNormal content.\n");
    let output = directory.path().join("mixed");
    let result = writer(directory.path())
        .args(["build", "document.md", "--to", "typst,html,pdf", "-o"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for path in ["document.typ", "document.html", "document.pdf"] {
        assert!(output.join(path).is_file(), "{path}");
    }
    // A target-local output obstruction is a usage error (exit 2), while the
    // independent HTML/PDF targets still succeed. Bibliography errors are global.
    let failed = directory.path().join("one-fails");
    write(&failed, "document.typ/sentinel", b"keep this directory");
    let result = writer(directory.path())
        .args(["build", "document.md", "--to", "typst,html,pdf", "-o"])
        .arg(&failed)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(failed.join("document.html").is_file());
    assert!(failed.join("document.pdf").is_file());
    assert!(failed.join("document.typ").is_dir());
    assert_eq!(
        fs::read(failed.join("document.typ/sentinel")).unwrap(),
        b"keep this directory"
    );
    assert!(!failed.join("document_assets").exists());
    assert!(String::from_utf8(result.stderr).unwrap().contains("document.typ"));

    write(
        directory.path(),
        "writer.toml",
        format!("{CONFIG}[references]\nbibliography=['absent/refs.bib']\n"),
    );
    let global = directory.path().join("global-failure");
    let result = writer(directory.path())
        .args(["build", "document.md", "--to", "typst,html,pdf", "-o"])
        .arg(&global)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(
        !global.exists(),
        "global resource failure must precede all targets"
    );
    assert!(String::from_utf8(result.stderr).unwrap().contains("refs.bib"));
}

#[test]
fn notebook_exports_materialize_sources_and_explicit_markdown_format_is_supported()
 {
    let directory = project("unused\n");
    write(directory.path(), "part.md", "Notebook included part.\n");
    let notebook = json!({"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":[
        {"cell_type":"markdown","metadata":{},"source":"Notebook.\n\n:::include part.md\n:::\n"}
    ]});
    write(directory.path(), "analysis.ipynb", notebook.to_string());
    let result = build(directory.path(), "analysis.ipynb").output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(directory.path().join("out/analysis.typ").is_file());
    assert!(directory.path().join("out/analysis_part.typ").is_file());
    write(directory.path(), "report.source", "# Explicit Markdown\n");
    let result = build(directory.path(), "report.source")
        .args(["--format", "md"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(directory.path().join("out/report.typ").is_file());
}

#[test]
fn configured_resources_are_loaded_then_copied_without_compiling_the_template()
{
    let directory = project("# Configured resources\n");
    write(
        directory.path(),
        "refs.bib",
        "@book{a,title={A},author={Writer},year={2026}}\n",
    );
    write(
        directory.path(),
        "style.CSL",
        include_str!("fixtures/bib/minimal.csl"),
    );
    write(directory.path(), "template.typ", "#let writer-doc =\n");
    write(
        directory.path(),
        "writer.toml",
        format!(
            "{CONFIG}template='template.typ'\n[references]\nbibliography=['refs.bib']\nstyle='style.CSL'\n"
        ),
    );
    let result = build(directory.path(), "document.md").output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for resource in ["refs.bib", "style.CSL", "template.typ"] {
        assert_eq!(
            fs::read(
                directory.path().join("out/document_assets").join(resource)
            )
            .unwrap(),
            fs::read(directory.path().join(resource)).unwrap()
        );
    }
    // Preparation parses the real CSL, then export copies its exact bytes.
    // The intentionally invalid template remains uncompiled by source export;
    // physical compilation with valid resources is covered in typst_export.
}

#[test]
fn source_safety_parse_and_global_resource_failures_create_no_partial_tree() {
    let directory = project("# Keep source\n");
    let source = directory.path().join("document.md");
    let original = fs::read(&source).unwrap();
    let result = build(directory.path(), "document.md")
        .arg("-o")
        .arg(&source)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(!directory.path().join("out").exists());
    write(directory.path(), "document.md", ":::include cycle.md\n:::\n");
    write(directory.path(), "cycle.md", ":::include document.md\n:::\n");
    let result = build(directory.path(), "document.md").output().unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(!directory.path().join("out").exists());
    write(directory.path(), "document.md", "# Missing copy\n");
    write(
        directory.path(),
        "writer.toml",
        format!("{CONFIG}[references]\nbibliography=['missing.bib']\n"),
    );
    let result = build(directory.path(), "document.md").output().unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(!directory.path().join("out").exists());
}

#[test]
fn strict_math_fallback_still_writes_source_but_tag_only_warnings_do_not_fail()
{
    let directory = project("$\\foo{x}$\n");
    let result = build(directory.path(), "document.md")
        .arg("--strict")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    let text =
        fs::read_to_string(directory.path().join("out/document.typ")).unwrap();
    assert!(text.contains("#raw(") && text.contains("foo"));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("cannot convert equation")
    );
    write(directory.path(), "document.md", "$$x=1\\tag{3}$$\n");
    let result = build(directory.path(), "document.md")
        .arg("--strict")
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(
        String::from_utf8(result.stderr).unwrap().contains("ignored for pdf")
    );
}

#[test]
fn native_typst_refusal_and_source_only_dump_keep_their_existing_contracts() {
    let directory = project("# Dump only\n");
    let result = writer(directory.path())
        .args(["dump", "document.md", "--typst"])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(
        String::from_utf8(result.stdout)
            .unwrap()
            .contains("#show: writer-doc.with(")
    );
    assert!(!directory.path().join("out").exists());
    write(directory.path(), "native.typ", "Native source\n");
    let result = build(directory.path(), "native.typ").output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("writer build part.md --to typst")
    );
    assert!(!directory.path().join("out").exists());
}

#[cfg(unix)]
#[test]
fn chosen_non_utf8_output_stem_is_rejected_before_writing() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let directory = project("# Valid input\n");
    let parent = directory.path().join("not-created");
    let output = parent.join(OsString::from_vec(b"invalid-\xff.typ".to_vec()));
    let result = build(directory.path(), "document.md")
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(!parent.exists());
}
