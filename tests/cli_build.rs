//! T16 end-to-end HTML builds, output safety and milestone stubs.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use regex::Regex;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn writer() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_writer"));
    cmd.current_dir(root())
        .env_remove("RUST_LOG")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("SOURCE_DATE_EPOCH");
    cmd
}

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    dir
}

fn copy_example(dir: &Path) -> PathBuf {
    let src = root().join("examples/basic");
    let dst = dir.join("basic");
    std::fs::create_dir_all(dst.join("figures")).unwrap();
    for file in ["document.md", "writer.toml", "figures/plot.png"] {
        std::fs::copy(src.join(file), dst.join(file)).unwrap();
    }
    dst
}

#[test]
fn example_builds_standalone() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("d.html");
    let output = writer()
        .args(["build", "examples/basic/document.md", "-o"])
        .arg(&out)
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        Regex::new(r"^Wrote .*d\.html \([0-9.]+ (B|kB|MB)\)\n$")
            .unwrap()
            .is_match(&stdout),
        "{stdout}"
    );
    let html = std::fs::read_to_string(out).unwrap();
    assert_eq!(html.matches("data-writer-document").count(), 1);
    assert!(!html.contains("asset:"));
    assert!(!html.contains("/__writer/"));
    let start = html.find("<main data-writer-document").unwrap();
    let end = start + html[start..].find("</main>").unwrap() + "</main>".len();
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r#"data:[^"']+"#, "[data]");
    settings.bind(|| {
        insta::assert_snapshot!("example_basic_main", &html[start..end])
    });
}

#[test]
fn example_builds_into_dist() {
    let dir = tempfile::tempdir().unwrap();
    let example = copy_example(dir.path());
    writer()
        .current_dir(&example)
        .args(["build", "document.md"])
        .assert()
        .success();
    assert!(example.join("dist/document.html").is_file());
}

#[test]
fn site_mode_writes_index_and_assets() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("site");
    let output = writer()
        .args(["build", "examples/basic/document.md", "--site", "-o"])
        .arg(&out)
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(out.join("index.html").is_file());
    assert!(out.join("assets").is_dir());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 2, "{stdout}");
    assert!(stdout.contains("index.html ("), "{stdout}");
    assert!(stdout.contains(" files, "), "{stdout}");
    let html = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(!html.contains("asset:"));
    assert!(html.contains("assets/"));
}

#[test]
fn missing_input_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    writer()
        .arg("build")
        .arg(dir.path().join("missing.md"))
        .assert()
        .code(1)
        .stderr(contains("missing.md"));
}

#[test]
fn unknown_theme_exits_1() {
    writer()
        .args(["build", "examples/basic/document.md", "--theme", "nope"])
        .assert()
        .code(1)
        .stderr(contains("theme 'nope'"));
}

#[test]
fn unknown_extension_exits_2() {
    writer()
        .args(["build", "document.txt"])
        .assert()
        .code(2)
        .stderr(contains("--format md|ipynb|typ"));
}

#[test]
fn output_over_input_is_refused() {
    let dir = project(&[("document.md", "# Unchanged\n")]);
    let input = dir.path().join("document.md");
    let original = std::fs::read(&input).unwrap();
    writer()
        .arg("build")
        .arg(&input)
        .arg("-o")
        .arg(&input)
        .assert()
        .code(2)
        .stderr(contains("refusing to write"));
    assert_eq!(std::fs::read(input).unwrap(), original);
}

#[test]
fn stub_targets_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    for (target, task) in
        [("pdf", "T32"), ("docx", "T41"), ("latex", "T61"), ("typst", "T34")]
    {
        writer()
            .args(["build", "examples/basic/document.md", "--to", target, "-o"])
            .arg(dir.path().join(format!("out.{target}")))
            .assert()
            .code(2)
            .stderr(contains(format!(
                "{target}: not implemented yet ({task})"
            )));
    }
    writer()
        .args([
            "build", "examples/basic/document.md", "--to", "pdf", "--engine",
            "chrome", "-o",
        ])
        .arg(dir.path().join("chrome.pdf"))
        .assert()
        .code(2)
        .stderr(contains("pdf --engine chrome: not implemented yet (T77)"));
    let mixed = dir.path().join("mixed");
    writer()
        .args(["build", "examples/basic/document.md", "--to", "html,pdf", "-o"])
        .arg(&mixed)
        .assert()
        .code(2)
        .stderr(contains("(T32)"));
    assert!(mixed.join("document.html").is_file());
    assert!(!mixed.join("document.pdf").exists());
}

#[test]
fn typst_target_on_typ_input_exits_2() {
    let dir = project(&[("document.typ", "Hello\n")]);
    writer()
        .arg("build")
        .arg(dir.path().join("document.typ"))
        .args(["--to", "typst"])
        .assert()
        .code(2)
        .stderr(contains("writer build part.md --to typst"));
}

#[test]
fn watch_is_a_stub() {
    writer()
        .args(["build", "examples/basic/document.md", "--watch"])
        .assert()
        .code(2)
        .stderr(contains("build --watch: not implemented yet (T20)"));
}

#[test]
fn command_stubs_exit_2() {
    for (args, task) in [
        (vec!["serve", "examples/basic/document.md"], "T23"),
        (vec!["check", "examples/basic/document.md"], "T1C"),
        (vec!["init"], "T72"),
        (vec!["eject", "theme", "theme"], "T72"),
    ] {
        writer()
            .args(args)
            .assert()
            .code(2)
            .stderr(contains(format!("not implemented yet ({task})")));
    }
}

#[test]
fn licenses_flag() {
    writer()
        .arg("--licenses")
        .assert()
        .success()
        .stdout(writer::vendor::licenses());
    writer().assert().code(2);
}

#[test]
fn version_flag() {
    writer()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::starts_with("writer 0.1.0"));
}

#[test]
fn duplicate_targets_build_once() {
    let dir = tempfile::tempdir().unwrap();
    let output = writer()
        .args([
            "build", "examples/basic/document.md", "--to", "html,html", "-o",
        ])
        .arg(dir.path().join("out.html"))
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(String::from_utf8(output.stdout).unwrap().lines().count(), 1);
}

#[test]
fn config_errors_exit_2_and_quiet_hides_success() {
    writer()
        .args([
            "--config", "missing.toml", "build", "examples/basic/document.md",
        ])
        .assert()
        .code(2)
        .stderr(contains("config:"));
    let dir = tempfile::tempdir().unwrap();
    writer()
        .args(["-q", "build", "examples/basic/document.md", "-o"])
        .arg(dir.path().join("quiet.html"))
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn front_matter_controls_existing_directory_output() {
    let dir = project(&[
        ("document.md", "---\nhtml: {standalone: false}\n---\n# Site\n"),
        ("writer.toml", "[pdf]\nsystem_fonts = false\n"),
    ]);
    let out = dir.path().join("site");
    std::fs::create_dir(&out).unwrap();
    writer()
        .arg("build")
        .arg(dir.path().join("document.md"))
        .arg("-o")
        .arg(&out)
        .assert()
        .success();
    assert!(out.join("index.html").is_file());

    std::fs::write(
        dir.path().join("document.md"),
        "---\nhtml: {standalone: true}\n---\n# Standalone\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("writer.toml"),
        "[html]\nstandalone = false\n[pdf]\nsystem_fonts = false\n",
    )
    .unwrap();
    writer()
        .arg("build")
        .arg(dir.path().join("document.md"))
        .arg("-o")
        .arg(&out)
        .assert()
        .code(2)
        .stderr(contains("names a directory"));
}
