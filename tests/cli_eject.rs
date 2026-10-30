//! Exact resource ejection, safe destinations and usable configuration snippets.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use typst::foundations::Datetime;
use writer::config::Config;
use writer::typst::compile::compile_document;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld};

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

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn guidance(output: &[u8], table: &str, key: &str, path: &Path) {
    let value: toml::Table =
        toml::from_str(std::str::from_utf8(output).unwrap()).unwrap();
    assert_eq!(value.len(), 1);
    assert_eq!(value[table].as_table().unwrap().len(), 1);
    assert_eq!(value[table][key].as_str(), path.to_str());
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                directories.push(path);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    result
}

fn package_bytes() -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for (prefix, directory) in [
        (
            "typst/writer-doc/".to_owned(),
            format!("writer/doc/{}", writer::typst::WRITER_DOC_VERSION),
        ),
        (
            format!(
                "vendor/typst-packages/preview/mitex/{}/",
                writer::typst::MITEX_VERSION
            ),
            format!("preview/mitex/{}", writer::typst::MITEX_VERSION),
        ),
    ] {
        for path in writer::vendor::list(&prefix) {
            result.insert(
                PathBuf::from(&directory).join(&path[prefix.len()..]),
                writer::vendor::get(&path).unwrap().into_owned(),
            );
        }
    }
    result
}

#[test]
fn templates_are_byte_exact_and_quiet_keeps_only_functional_toml() {
    for (command, section, bytes) in [
        (
            "typst-template",
            "pdf",
            writer::vendor::get("typst/writer-doc/template.typ")
                .unwrap()
                .into_owned(),
        ),
        (
            "latex-template",
            "latex",
            writer::latex::template::SAMPLE.as_bytes().to_vec(),
        ),
    ] {
        for quiet in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let file = Path::new("nested/My λ template.file");
            let mut process = writer(root.path());
            if quiet {
                process.arg("-q");
            }
            let output =
                process.args(["eject", command]).arg(file).output().unwrap();
            success(&output);
            guidance(&output.stdout, section, "template", file);
            assert_eq!(fs::read(root.path().join(file)).unwrap(), bytes);
            assert_eq!(output.stderr.is_empty(), quiet);
            let before = fs::metadata(root.path().join(file))
                .unwrap()
                .modified()
                .unwrap();
            let repeat = writer(root.path())
                .args(["eject", command])
                .arg(file)
                .output()
                .unwrap();
            assert_eq!(repeat.status.code(), Some(2));
            assert!(repeat.stdout.is_empty());
            assert_eq!(fs::read(root.path().join(file)).unwrap(), bytes);
            assert_eq!(
                fs::metadata(root.path().join(file))
                    .unwrap()
                    .modified()
                    .unwrap(),
                before
            );
        }
    }
}

#[test]
fn ejected_root_theme_keeps_all_embedded_bytes_and_renders() {
    let root = tempfile::tempdir().unwrap();
    let directory = Path::new("custom theme");
    let output = writer(root.path())
        .args(["eject", "theme"])
        .arg(directory)
        .output()
        .unwrap();
    success(&output);
    guidance(&output.stdout, "html", "theme", directory);
    let expected: BTreeMap<_, _> = writer::vendor::list("themes/default/")
        .into_iter()
        .map(|path| {
            (
                PathBuf::from(path.strip_prefix("themes/default/").unwrap()),
                writer::vendor::get(&path).unwrap().into_owned(),
            )
        })
        .collect();
    assert_eq!(inventory(&root.path().join(directory)), expected);
    let theme = writer::theme::resolve(
        root.path().join(directory).to_str().unwrap(),
        &[],
    )
    .unwrap();
    assert_eq!(theme.chain.len(), 1);
    assert_eq!(theme.name(), "default");
    fs::write(
        root.path().join("document.md"),
        "# Ejected theme\n\nA paragraph.\n",
    )
    .unwrap();
    fs::write(root.path().join("writer.toml"), &output.stdout).unwrap();
    let built = writer(root.path())
        .args(["build", "document.md", "-o", "result.html"])
        .output()
        .unwrap();
    success(&built);
    let html = fs::read_to_string(root.path().join("result.html")).unwrap();
    assert!(
        html.contains("Ejected theme") && html.contains("data-writer-document")
    );
    let css = root.path().join(directory).join("styles/theme.css");
    let mut modified = fs::read_to_string(&css).unwrap();
    modified.push_str("\n:root { --w-measure: 47rem; }\n");
    fs::write(&css, modified).unwrap();
    success(
        &writer(root.path())
            .args(["build", "document.md", "-o", "result.html"])
            .output()
            .unwrap(),
    );
    assert!(
        fs::read_to_string(root.path().join("result.html"))
            .unwrap()
            .contains("47rem")
    );
    let before = inventory(&root.path().join(directory));
    let repeat = writer(root.path())
        .args(["eject", "theme"])
        .arg(directory)
        .output()
        .unwrap();
    assert_eq!(repeat.status.code(), Some(2));
    assert_eq!(inventory(&root.path().join(directory)), before);
}

#[test]
fn ejected_templates_are_usable_by_the_existing_build_flags() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("document.md"),
        "# Custom template\n\nA report.\n",
    )
    .unwrap();
    fs::write(
        root.path().join("writer.toml"),
        "[pdf]\nsystem_fonts=false\nallow_packages=false\n",
    )
    .unwrap();
    for (command, file) in
        [("typst-template", "template.typ"), ("latex-template", "template.tex")]
    {
        success(
            &writer(root.path())
                .args(["eject", command, file])
                .output()
                .unwrap(),
        );
    }
    success(
        &writer(root.path())
            .args([
                "build", "document.md", "--to", "pdf", "--template",
                "template.typ", "-o", "custom.pdf",
            ])
            .output()
            .unwrap(),
    );
    assert!(
        fs::read(root.path().join("custom.pdf")).unwrap().starts_with(b"%PDF-")
    );
    success(
        &writer(root.path())
            .args([
                "build", "document.md", "--to", "latex", "--latex-template",
                "template.tex", "-o", "custom.tex",
            ])
            .output()
            .unwrap(),
    );
    assert!(
        fs::read_to_string(root.path().join("custom.tex"))
            .unwrap()
            .contains("\\documentclass")
    );
}

#[test]
fn package_ejection_preserves_unrelated_files_and_all_package_bytes() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("packages");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("keep"), b"unrelated").unwrap();
    let output = writer(root.path())
        .args(["-q", "eject", "typst-package"])
        .arg(&directory)
        .output()
        .unwrap();
    success(&output);
    assert!(output.stderr.is_empty());
    let value: toml::Table =
        toml::from_str(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
    assert_eq!(
        value["typst"]["package_paths"].as_array().unwrap(),
        &[toml::Value::String(directory.to_str().unwrap().into())]
    );
    let mut expected = package_bytes();
    expected.insert("keep".into(), b"unrelated".to_vec());
    assert_eq!(inventory(&directory), expected);
    let output = writer(root.path())
        .args(["eject", "typst-package"])
        .arg(&directory)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(inventory(&directory), expected);
}

#[test]
fn either_existing_package_version_refuses_before_writing_the_other() {
    for version in [
        format!("writer/doc/{}", writer::typst::WRITER_DOC_VERSION),
        format!("preview/mitex/{}", writer::typst::MITEX_VERSION),
    ] {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("packages");
        fs::create_dir_all(directory.join(&version)).unwrap();
        fs::write(directory.join(&version).join("keep"), b"owned").unwrap();
        let before = inventory(&directory);
        let output = writer(root.path())
            .args(["eject", "typst-package"])
            .arg(&directory)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(inventory(&directory), before);
    }
}

#[test]
fn ejected_packages_compile_from_the_written_local_modules() {
    let root = tempfile::tempdir().unwrap();
    success(
        &writer(root.path())
            .args(["eject", "typst-package", "packages"])
            .output()
            .unwrap(),
    );
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.pdf.allow_packages = false;
    let mut world = WriterWorld::from_config(
        &config,
        root.path(),
        MainFile::Virtual,
        Datetime::from_ymd(2026, 10, 29).unwrap(),
    )
    .unwrap();
    // Local paths deliberately bypass the World's embedded-package priority.
    let source = format!(
        "#import \"packages/writer/doc/{}/lib.typ\": writer-doc\n\
         #import \"packages/preview/mitex/{}/lib.typ\": mi\n\
         #show: writer-doc.with(title: \"Ejected packages\")\n\
         A real formula #mi(\"x^2 + 1\").\n",
        writer::typst::WRITER_DOC_VERSION,
        writer::typst::MITEX_VERSION,
    );
    world.set_main_text(&source, SourceMap::default());
    let compiled = compile_document(&mut world);
    assert!(compiled.output.is_some(), "{:?}", compiled.diagnostics);
    assert!(!compiled.diagnostics.has_errors(), "{:?}", compiled.diagnostics);
}

#[test]
fn templates_ignore_project_config_and_invalid_epoch() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("writer.toml"), "not TOML [\n").unwrap();
    for (command, file) in
        [("typst-template", "template.typ"), ("latex-template", "template.tex")]
    {
        let output = writer(root.path())
            .args(["--config", "missing.toml"])
            .env("SOURCE_DATE_EPOCH", "invalid")
            .args(["eject", command, file])
            .output()
            .unwrap();
        success(&output);
    }
    assert_eq!(
        fs::read(root.path().join("template.tex")).unwrap(),
        writer::latex::template::SAMPLE.as_bytes()
    );
}

#[test]
fn file_and_directory_destinations_are_refused_without_replacement() {
    let root = tempfile::tempdir().unwrap();
    for (index, command) in
        ["theme", "typst-template", "latex-template"].into_iter().enumerate()
    {
        for directory in [false, true] {
            let path = root.path().join(format!("owned-{index}-{directory}"));
            if directory {
                fs::create_dir(&path).unwrap();
            } else {
                fs::write(&path, b"user bytes").unwrap();
            }
            let output = writer(root.path())
                .args(["eject", command])
                .arg(&path)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            if directory {
                assert!(path.is_dir());
            } else {
                assert_eq!(fs::read(path).unwrap(), b"user bytes");
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn install_uses_an_isolated_platform_data_root_instead_of_the_required_dir() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let ignored = root.path().join("unused");
    let output = writer(root.path())
        .env("XDG_DATA_HOME", &data)
        .args(["eject", "typst-package"])
        .arg(&ignored)
        .arg("--install")
        .output()
        .unwrap();
    success(&output);
    assert!(!ignored.exists());
    assert_eq!(inventory(&data.join("typst/packages")), package_bytes());
}

#[cfg(unix)]
#[test]
fn destination_links_and_non_utf8_template_names_refuse_before_new_writes() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    for command in ["theme", "typst-template", "latex-template"] {
        let link = root.path().join(format!("dangling-{command}"));
        let target = root.path().join(format!("missing-{command}"));
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let output = writer(root.path())
            .args(["eject", command])
            .arg(&link)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(fs::read_link(&link).unwrap(), target);
    }
    let path = root
        .path()
        .join("new")
        .join(OsString::from_vec(b"bad-\xff.tex".to_vec()));
    let output = writer(root.path())
        .args(["eject", "latex-template"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!root.path().join("new").exists());
}
