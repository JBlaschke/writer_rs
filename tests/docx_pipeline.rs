//! Rich Word target integration, renderer adaptation and timestamp parity.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use assert_cmd::Command;
use predicates::str::contains;
use typst::World;
use typst::foundations::Datetime;
use writer::config::Config;
use writer::docx::{Renderer, ValidateOptions, core_timestamp, validate};
use writer::ir::{DocMeta, MathDialect};
use writer::pipeline;
use writer::typst::compile::pdf_timestamp;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WorldOptions, WriterWorld};

#[path = "common/docx.rs"]
mod common_docx;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn writer() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
    command
        .current_dir(root())
        .env("SOURCE_DATE_EPOCH", "0")
        .env_remove("RUST_LOG")
        .env_remove("CLICOLOR_FORCE");
    command
}

fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut part = archive.by_index(index).unwrap();
            let name = part.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            part.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}

fn valid_file(path: &Path, reference: bool) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        validate(&bytes, &ValidateOptions { builtin_styles: !reference }),
        [],
        "{}",
        path.display()
    );
    assert!(common_docx::read_back(&bytes).paragraphs > 0);
    bytes
}

fn project(text: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("document.md"), text).unwrap();
    std::fs::write(
        directory.path().join("writer.toml"),
        "[pdf]\nsystem_fonts = false\n",
    )
    .unwrap();
    directory
}

#[test]
fn markdown_and_notebook_corpus_builds_editable_word() {
    let directory = tempfile::tempdir().unwrap();
    for (kind, names) in [
        (
            "md",
            &["figures", "tables", "math", "footnotes", "raw", "numbering"][..],
        ),
        ("ipynb", &["outputs", "dataframe"][..]),
    ] {
        for name in names {
            let extension = if kind == "md" { "md" } else { "ipynb" };
            let mut input = format!("tests/fixtures/{kind}/{name}.{extension}");
            if *name == "outputs" {
                // The canonical fixture intentionally contains a missing file.
                // Supply it in a copy to exercise all remaining saved outputs.
                let copy = directory.path().join("outputs.ipynb");
                let notebook = std::fs::read_to_string(root().join(&input))
                    .unwrap()
                    .replace("../md/images/plot.png", "images/pixel.png");
                std::fs::write(&copy, notebook).unwrap();
                std::fs::create_dir_all(directory.path().join("images"))
                    .unwrap();
                for image in ["pixel.png", "missing.png"] {
                    std::fs::copy(
                        root().join("tests/fixtures/ipynb/images/pixel.png"),
                        directory.path().join("images").join(image),
                    )
                    .unwrap();
                }
                input = copy.to_string_lossy().into_owned();
            }
            let out = directory.path().join(format!("{name}.docx"));
            writer()
                .args(["build", &input, "--to", "docx", "-o"])
                .arg(&out)
                .assert()
                .success();
            valid_file(&out, false);
        }
    }
}

#[test]
fn reference_corpus_reports_label_errors_and_preserves_valid_word_structure() {
    let directory = tempfile::tempdir().unwrap();
    // This fixture intentionally contains duplicate and reserved labels.
    let out = directory.path().join("references.docx");
    writer()
        .args([
            "build",
            "tests/fixtures/md/references.md",
            "--to",
            "docx",
            "-o",
        ])
        .arg(&out)
        .assert()
        .code(1)
        .stderr(contains("occurs multiple times"));
    valid_file(&out, false);
}

#[test]
fn missing_selected_notebook_image_fails_without_writing() {
    let directory = tempfile::tempdir().unwrap();
    let out = directory.path().join("outputs.docx");
    writer()
        .args([
            "build",
            "tests/fixtures/ipynb/outputs.ipynb",
            "--to",
            "docx",
            "-o",
        ])
        .arg(&out)
        .assert()
        .code(1)
        .stderr(contains("asset not found:"));
    assert!(!out.exists());
}

#[test]
fn all_examples_build_and_repeated_packages_are_identical() {
    let directory = tempfile::tempdir().unwrap();
    for (name, input) in [
        ("basic", "examples/basic/document.md"),
        ("notebook", "examples/notebook/analysis.ipynb"),
        ("mixed", "examples/mixed/document.md"),
    ] {
        let out = directory.path().join(format!("{name}.docx"));
        writer()
            .args(["build", input, "--to", "docx", "-o"])
            .arg(&out)
            .assert()
            .success();
        let first = valid_file(&out, false);
        writer()
            .args(["build", input, "--to", "docx", "-o"])
            .arg(&out)
            .assert()
            .success();
        assert_eq!(first, valid_file(&out, false), "{name}");
        let package = parts(&first);
        let date = if name == "basic" {
            "2026-10-24T00:00:00Z"
        } else {
            "1970-01-01T00:00:00Z"
        };
        assert!(
            String::from_utf8_lossy(&package["docProps/core.xml"])
                .contains(date)
        );
    }
}

#[test]
fn default_output_and_custom_reference_work_without_a_theme() {
    let directory = project("# Editable\n\nWord body.\n");
    writer()
        .current_dir(directory.path())
        .args([
            "build", "document.md", "--to", "docx", "--theme", "absent-theme",
        ])
        .assert()
        .success();
    let default = directory.path().join("dist/document.docx");
    valid_file(&default, false);
    let reference = root().join("tests/fixtures/docx/reference.docx");
    let out = directory.path().join("reference.docx");
    writer()
        .current_dir(directory.path())
        .args(["build", "document.md", "--to", "docx", "--reference-doc"])
        .arg(&reference)
        .arg("-o")
        .arg(&out)
        .assert()
        .success();
    let first = valid_file(&out, true);
    let package = parts(&first);
    assert!(
        String::from_utf8_lossy(&package["word/header1.xml"])
            .contains("REFERENCE HEADER")
    );
    writer()
        .current_dir(directory.path())
        .args(["build", "document.md", "--to", "docx", "--reference-doc"])
        .arg(&reference)
        .arg("-o")
        .arg(&out)
        .assert()
        .success();
    assert_eq!(first, valid_file(&out, true));
}

#[test]
fn html_failure_does_not_prevent_word_target() {
    let directory = project("# Independent targets\n");
    let out = directory.path().join("outputs");
    writer()
        .current_dir(directory.path())
        .args([
            "build", "document.md", "--to", "html,docx", "--theme",
            "absent-theme", "-o",
        ])
        .arg(&out)
        .assert()
        .code(1)
        .stderr(contains("absent-theme"));
    assert!(!out.join("document.html").exists());
    valid_file(&out.join("document.docx"), false);
}

#[test]
fn unsupported_images_warn_and_strict_escalates() {
    let directory =
        project("![Remote image](https://example.invalid/picture.png)\n");
    let out = directory.path().join("remote.docx");
    writer()
        .current_dir(directory.path())
        .args(["build", "document.md", "--to", "docx", "-o"])
        .arg(&out)
        .assert()
        .success()
        .stderr(contains("warning:"));
    let package = parts(&valid_file(&out, false));
    assert!(
        String::from_utf8_lossy(&package["word/_rels/document.xml.rels"])
            .contains("https://example.invalid/picture.png")
    );
    writer()
        .current_dir(directory.path())
        .args(["build", "document.md", "--to", "docx", "--strict", "-o"])
        .arg(&out)
        .assert()
        .code(1)
        .stderr(contains("error:"));
    valid_file(&out, false);
}

#[test]
fn word_output_clashes_preserve_source_asset_and_reference() {
    let directory = project("![Plot](plot.png)\n");
    let image = directory.path().join("plot.png");
    std::fs::copy(root().join("tests/fixtures/md/images/plot.png"), &image)
        .unwrap();
    let reference = directory.path().join("reference.docx");
    std::fs::copy(
        root().join("tests/fixtures/docx/reference.docx"),
        &reference,
    )
    .unwrap();
    for protected in
        [directory.path().join("document.md"), image, reference.clone()]
    {
        let before = std::fs::read(&protected).unwrap();
        writer()
            .current_dir(directory.path())
            .args(["build", "document.md", "--to", "docx", "--reference-doc"])
            .arg(&reference)
            .arg("-o")
            .arg(&protected)
            .assert()
            .code(2)
            .stderr(contains("refusing to write"));
        assert_eq!(std::fs::read(protected).unwrap(), before);
    }
}

#[test]
fn invalid_timestamp_or_reference_does_not_write_output() {
    let directory = project("# No output\n");
    let out = directory.path().join("out.docx");
    std::fs::write(
        directory.path().join("writer.toml"),
        "[pdf]\nsystem_fonts = false\ntimestamp = 'yesterday'\n",
    )
    .unwrap();
    writer()
        .current_dir(directory.path())
        .args(["build", "document.md", "--to", "docx", "-o"])
        .arg(&out)
        .assert()
        .code(2)
        .stderr(contains("invalid [pdf] timestamp"));
    assert!(!out.exists());
    std::fs::write(
        directory.path().join("writer.toml"),
        "[pdf]\nsystem_fonts = false\n",
    )
    .unwrap();
    writer()
        .current_dir(directory.path())
        .args([
            "build", "document.md", "--to", "docx", "--reference-doc",
            "missing.docx", "-o",
        ])
        .arg(&out)
        .assert()
        .code(1)
        .stderr(contains("reference document"));
    assert!(!out.exists());
}

fn timestamp(value: Datetime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        value.year().unwrap(),
        value.month().unwrap(),
        value.day().unwrap(),
        value.hour().unwrap_or(0),
        value.minute().unwrap_or(0),
        value.second().unwrap_or(0)
    )
}

#[test]
fn word_and_pdf_timestamps_agree_for_shared_inputs() {
    for (explicit, date, epoch) in [
        ("2026-10-25T12:00:00.5+02:00", None, Some("0")),
        ("2026-01-01T00:00:00-05:30", None, None),
        ("", Some("2024-02-29"), Some("0")),
        ("", Some("October 2026"), Some("1700000000")),
        ("", Some("2026-02-30"), Some("0")),
        ("", None, Some("0")),
        ("yesterday", None, None),
        ("2026-10-25T25:00:00Z", None, None),
    ] {
        let mut config = Config::default();
        config.pdf.timestamp = explicit.into();
        let meta =
            DocMeta { date: date.map(str::to_owned), ..Default::default() };
        let word = core_timestamp(explicit, date, epoch);
        let pdf = pdf_timestamp(&config.pdf, &meta, epoch, UNIX_EPOCH);
        match (word, pdf) {
            (Ok(word), Ok(pdf)) => assert_eq!(word, Some(timestamp(pdf))),
            (Err(word), Err(pdf)) => {
                assert_eq!(word.to_string(), pdf.to_string())
            }
            pair => panic!(
                "timestamp policies differ for {explicit:?}/{date:?}/{epoch:?}: {pair:?}"
            ),
        }
    }
    // DOCX omits absent/invalid fallback values and requires four-digit UTC years.
    assert_eq!(core_timestamp("", None, None).unwrap(), None);
    assert_eq!(core_timestamp("", None, Some("invalid")).unwrap(), None);
    assert!(core_timestamp("0000-01-01T00:00:00+01:00", None, None).is_err());
}

#[test]
fn renderer_borrows_world_and_restores_main_after_each_route() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = WriterWorld::new(WorldOptions {
        root: directory.path().to_path_buf(),
        main: MainFile::Virtual,
        today: Datetime::from_ymd(2026, 10, 27).unwrap(),
        system_fonts: false,
        font_paths: Vec::new(),
        package_paths: Vec::new(),
        allow_packages: false,
        inputs: BTreeMap::new(),
    })
    .unwrap();
    world.set_main_text("Retained main", SourceMap::default());
    let main = world.main();
    let config = Config::default();
    {
        let mut renderer =
            pipeline::docx::WorldRenderer::new(&config, &mut world);
        let picture =
            renderer.equation("x^2", MathDialect::Typst, false).unwrap();
        assert!(picture.svg.as_deref().unwrap().starts_with(b"<svg"));
        assert!(picture.height_pt >= picture.baseline_pt);
        let picture = renderer
            .image(
                include_bytes!("fixtures/docx/images/drawing.svg"),
                "image/svg+xml",
                Some(40.0),
            )
            .unwrap();
        assert!((picture.width_pt - 40.0).abs() < 0.01);
        assert!(picture.png.starts_with(b"\x89PNG"));
        assert!(
            renderer.typst_mathml("x^2", false).unwrap().contains("<msup>")
        );
    }
    assert_eq!(world.main(), main);
    assert_eq!(world.source(main).unwrap().text(), "Retained main");
    let mut renderer = pipeline::docx::WorldRenderer::new(&config, &mut world);
    assert!(
        renderer.equation("\\frac{a}{b}", MathDialect::Latex, true).is_ok()
    );
    assert_eq!(world.main(), main);
    assert!(world.accessed().is_empty());
}
