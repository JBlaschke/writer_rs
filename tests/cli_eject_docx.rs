//! The built-in reference is a deterministic file with usable TOML guidance.

use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

use assert_cmd::Command;
use writer::docx::custom_style::reference_docx;
use writer::docx::{ValidateOptions, validate};

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

fn eject(root: &Path, file: &Path, quiet: bool) -> Command {
    let mut command = writer(root);
    if quiet {
        command.arg("-q");
    }
    command.args(["eject", "docx-reference"]).arg(file);
    command
}

fn guidance(output: &[u8], file: &Path) {
    let text = std::str::from_utf8(output).unwrap();
    let value: toml::Table = toml::from_str(text).unwrap_or_else(|error| {
        panic!("configuration guidance is not TOML: {error}: {text:?}");
    });
    assert_eq!(value["docx"]["reference_doc"].as_str(), file.to_str());
    assert_eq!(value.len(), 1);
    assert_eq!(value["docx"].as_table().unwrap().len(), 1);
}

fn assert_reference(file: &Path) {
    let bytes = std::fs::read(file).unwrap();
    assert_eq!(bytes, reference_docx().unwrap());
    assert_eq!(validate(&bytes, &ValidateOptions { builtin_styles: true }), []);
}

#[test]
fn ejects_a_valid_reference_creates_parents_and_prints_only_toml_to_stdout() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("nested/Writer reference λ.docx");
    let output = eject(root.path(), &file, false).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    guidance(&output.stdout, &file);
    assert!(String::from_utf8(output.stderr).unwrap().contains("Written "));
    assert_reference(&file);
}

#[test]
fn quiet_keeps_the_functional_configuration_snippet() {
    let root = tempfile::tempdir().unwrap();
    let file = PathBuf::from("quiet.docx");
    let output = eject(root.path(), &file, true).output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    guidance(&output.stdout, &file);
    assert_reference(&root.path().join(file));
}

#[test]
fn builtin_bytes_ignore_project_config_explicit_config_and_epoch_environment() {
    let root = tempfile::tempdir().unwrap();
    let invalid = root.path().join("invalid.toml");
    std::fs::write(&invalid, "this is not valid TOML [\n").unwrap();
    std::fs::write(root.path().join("writer.toml"), "[docx]\nmath='image'\n")
        .unwrap();
    let file = root.path().join("reference.docx");
    let output = writer(root.path())
        .arg("--config")
        .arg(&invalid)
        .env("SOURCE_DATE_EPOCH", "not-an-epoch")
        .args(["eject", "docx-reference"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    guidance(&output.stdout, &file);
    assert_reference(&file);
}

#[test]
fn existing_file_and_directory_are_usage_errors_and_remain_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("existing.docx");
    std::fs::write(&file, b"a user-owned reference").unwrap();
    let output = eject(root.path(), &file, false).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(&file).unwrap(), b"a user-owned reference");
    let directory = root.path().join("directory.docx");
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("sentinel"), "keep").unwrap();
    let output = eject(root.path(), &directory, false).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        std::fs::read_to_string(directory.join("sentinel")).unwrap(),
        "keep"
    );
}

#[test]
fn repeat_ejection_refuses_without_rewriting_the_reference() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("reference.docx");
    assert!(eject(root.path(), &file, true).output().unwrap().status.success());
    let bytes = std::fs::read(&file).unwrap();
    let metadata = std::fs::metadata(&file).unwrap();
    let output = eject(root.path(), &file, true).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(&file).unwrap().modified().unwrap(),
        metadata.modified().unwrap()
    );
}

#[cfg(unix)]
#[test]
fn quotes_backslashes_and_newlines_are_toml_escaped_without_changing_the_path()
{
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("quote \" slash \\ line\n.docx");
    let output = eject(root.path(), &file, false).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    guidance(&output.stdout, &file);
    assert_reference(&file);
}

#[cfg(unix)]
#[test]
fn symlinks_and_dangling_symlinks_are_not_followed_or_replaced() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let owned = root.path().join("owned.docx");
    let missing = root.path().join("missing.docx");
    std::fs::write(&owned, b"owned bytes").unwrap();
    for target in [&owned, &missing] {
        let link = root.path().join(if target == &owned {
            "link.docx"
        } else {
            "dangling.docx"
        });
        symlink(target, &link).unwrap();
        let output = eject(root.path(), &link, false).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(std::fs::read_link(&link).unwrap(), *target);
        assert!(
            std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink()
        );
    }
    assert_eq!(std::fs::read(&owned).unwrap(), b"owned bytes");
    assert!(!missing.exists());
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_are_rejected_before_any_parent_or_file_is_created() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let file = root
        .path()
        .join("new-parent")
        .join(OsString::from_vec(b"invalid-\xff.docx".to_vec()));
    let output = eject(root.path(), &file, false).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn parent_files_remain_intact_when_directory_creation_fails() {
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("parent");
    std::fs::write(&parent, b"keep this parent").unwrap();
    let output = eject(root.path(), &parent.join("reference.docx"), false)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(&parent).unwrap(), b"keep this parent");
}

#[test]
fn two_concurrent_ejections_have_exactly_one_winner() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("reference.docx");
    let child = || {
        ProcessCommand::new(env!("CARGO_BIN_EXE_writer"))
            .current_dir(root.path())
            .env_remove("RUST_LOG")
            .env_remove("SOURCE_DATE_EPOCH")
            .env("NO_COLOR", "1")
            .args(["eject", "docx-reference"])
            .arg(&file)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let first = child();
    let second = child();
    let outputs =
        [first.wait_with_output().unwrap(), second.wait_with_output().unwrap()];
    let mut statuses = outputs
        .iter()
        .map(|output| output.status.code().unwrap())
        .collect::<Vec<_>>();
    statuses.sort();
    assert_eq!(statuses, [0, 2]);
    for output in &outputs {
        if output.status.success() {
            guidance(&output.stdout, &file);
        } else {
            assert!(output.stdout.is_empty());
        }
    }
    assert_reference(&file);
}

#[test]
fn other_eject_resources_retain_the_t72_stub() {
    let root = tempfile::tempdir().unwrap();
    for resource in
        ["theme", "typst-template", "typst-package", "latex-template"]
    {
        let destination = root.path().join(resource);
        let output = writer(root.path())
            .arg("eject")
            .arg(resource)
            .arg(&destination)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8(output.stderr).unwrap().contains("T72"));
        assert!(!destination.exists());
    }
}
