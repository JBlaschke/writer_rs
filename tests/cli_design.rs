//! Design inspection and portable theme ejection through the public CLI.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;

fn writer(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
    command
        .current_dir(root)
        .env_remove("RUST_LOG")
        .env_remove("SOURCE_DATE_EPOCH");
    command
}

fn design(root: &Path, input: &str) -> Value {
    let output = writer(root)
        .args(["dump", "--design", input])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stdout.ends_with(b"\n"));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn dump_design_respects_config_and_front_matter_without_writing_outputs() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("plain.md"), "# Plain\n").unwrap();
    assert_eq!(design(root.path(), "plain.md"), Value::Null);
    fs::write(
        root.path().join("settings.toml"),
        "[design.text]\nsize = '11pt'\nline_height = 1.35\n",
    )
    .unwrap();
    fs::write(
        root.path().join("document.md"),
        "---\ndesign:\n  text:\n    size: 12pt\n---\n# Document\n",
    )
    .unwrap();
    let output = writer(root.path())
        .args(["--config", "settings.toml", "dump", "--design", "document.md"])
        .assert()
        .success()
        .get_output()
        .clone();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["text"]["size"], 12.0);
    assert_eq!(value["text"]["line_height"]["multiple"], 1.35);
    assert!(!root.path().join("dist").exists());
}

#[test]
fn design_flag_conflicts_with_other_dump_modes() {
    let root = tempfile::tempdir().unwrap();
    for mode in [
        "--ir", "--context", "--typst", "--latex", "--no-resolve",
        "--default-config",
    ] {
        writer(root.path())
            .args(["dump", "--design", mode, "document.md"])
            .assert()
            .code(2);
    }
}

#[test]
fn ejected_design_preserves_values_and_copies_colliding_asset_names() {
    let root = tempfile::tempdir().unwrap();
    for directory in ["first", "rest"] {
        fs::create_dir(root.path().join(directory)).unwrap();
        fs::write(
            root.path().join(directory).join("logo.svg"),
            format!("<svg xmlns='http://www.w3.org/2000/svg'><title>{directory}</title></svg>"),
        )
        .unwrap();
    }
    fs::write(root.path().join("document.md"), "# Letter\n").unwrap();
    fs::write(
        root.path().join("writer.toml"),
        "[design.fonts]\nbody = 'Source Sans 3'\n\
         [design.text]\nsize = '11pt'\nline_height = 1.35\n\
         [design.page.header.first]\nimage = 'first/logo.svg'\n\
         [design.page.header.rest]\nimage = {src = 'rest/logo.svg', width = '20mm'}\n\
         [design.styles.Recipient]\nspace_after = '4mm'\n",
    )
    .unwrap();
    let mut before = design(root.path(), "document.md");
    writer(root.path())
        .args(["eject", "theme", "portable", "--design", "document.md"])
        .assert()
        .success();
    assert_eq!(
        fs::read(root.path().join("portable/assets/design/logo.svg")).unwrap(),
        fs::read(root.path().join("first/logo.svg")).unwrap()
    );
    assert_eq!(
        fs::read(root.path().join("portable/assets/design/logo-1.svg"))
            .unwrap(),
        fs::read(root.path().join("rest/logo.svg")).unwrap()
    );
    fs::write(root.path().join("writer.toml"), "[html]\ntheme = 'portable'\n")
        .unwrap();
    let mut after = design(root.path(), "document.md");
    for variant in ["first", "rest"] {
        let original = before["page"]["header"][variant]["image"]["path"]
            .as_str()
            .unwrap();
        let copied =
            after["page"]["header"][variant]["image"]["path"].as_str().unwrap();
        assert_eq!(fs::read(original).unwrap(), fs::read(copied).unwrap());
        before["page"]["header"][variant]["image"]["path"] = Value::Null;
        after["page"]["header"][variant]["image"]["path"] = Value::Null;
    }
    assert_eq!(before, after);
}
