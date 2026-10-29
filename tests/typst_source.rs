//! Native-file PDF builds, target independence and consumed-input safety.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use typst::World;
use typst::foundations::Datetime;
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;
use writer::config::Target;
use writer::docx::{ValidateOptions, validate};
use writer::error::{Error, Level};
use writer::pipeline::{self, BuildOptions, BuildReport, Prepared, WorldSlot};
use writer::typst::compile::{PdfOptions, compile_document, compile_pdf};
use writer::typst::world::{MainFile, WriterWorld};

const CONFIG: &str = "[build]\nout_dir='out'\n[pdf]\nsystem_fonts=false\nallow_packages=false\ntimestamp='2026-10-29T10:34:56Z'\n";
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#526d82"/></svg>"##;
const NATIVE_LOOP: &str = "= Native loop\n\n#for item in range(3) [Loop #item ]\n\n#pagebreak()\nSecond page.\n";

fn write(root: &Path, name: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn project(source: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "document.typ", source);
    write(directory.path(), "writer.toml", CONFIG);
    directory
}

fn options(root: &Path) -> BuildOptions {
    let mut options = BuildOptions::new(root.join("document.typ"));
    options.config_path = Some(root.join("writer.toml"));
    options
}

fn build(
    root: &Path, targets: &[Target], worlds: &mut WorldSlot,
) -> BuildReport {
    pipeline::build_targets(&options(root), targets, worlds)
}

fn prepared(report: &BuildReport) -> &Prepared {
    report.prepared.as_ref().unwrap()
}

fn target_paths(report: &BuildReport, target: Target) -> &[PathBuf] {
    report
        .targets
        .iter()
        .find(|result| result.target == target)
        .unwrap()
        .written
        .as_ref()
        .unwrap()
}

fn pdf_bytes(report: &BuildReport) -> Vec<u8> {
    let paths = target_paths(report, Target::Pdf);
    assert_eq!(paths.len(), 1);
    let bytes = fs::read(&paths[0]).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(pipeline::pdf::page_count(&bytes).is_some_and(|pages| pages > 0));
    bytes
}

fn collect_text(frame: &Frame, text: &mut String) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Text(run) => text.push_str(&run.text),
            FrameItem::Group(group) => collect_text(&group.frame, text),
            _ => {}
        }
    }
}

fn contents(document: &PagedDocument) -> String {
    let mut text = String::new();
    for page in document.pages() {
        collect_text(&page.frame, &mut text);
    }
    text.chars().filter(|character| !character.is_whitespace()).collect()
}

fn docx_body(path: &Path) -> String {
    let bytes = fs::read(path).unwrap();
    assert!(validate(&bytes, &ValidateOptions::default()).is_empty());
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut body = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut body)
        .unwrap();
    body
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

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_only_pdf_suppresses_subset_findings_without_erasing_preparation() {
    let directory = project(NATIVE_LOOP);
    let opts = options(directory.path());
    let mut worlds = WorldSlot::new();
    let report = pipeline::build_targets(&opts, &[Target::Pdf], &mut worlds);
    assert!(prepared(&report).diagnostics.iter().any(|finding| {
        finding.level == Level::Error && finding.code.starts_with("E-TYP-")
    }));
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|finding| finding.code.starts_with("E-TYP-"))
    );
    assert_eq!(pipeline::pdf::page_count(&pdf_bytes(&report)), Some(2));
    let world = worlds.get(Some(prepared(&report)), &opts).unwrap();
    assert_eq!(world.source(world.main()).unwrap().text(), NATIVE_LOOP);
    let (document, _) = compile_document(world).into_result().unwrap();
    assert_eq!(document.pages().len(), 2);
    assert!(contents(&document).contains("Loop0Loop1Loop2"));
}

#[test]
fn native_control_flow_and_computed_labels_do_not_gate_native_pdf() {
    let cases = [
        "#set heading(numbering: \"1.\")\n#if true [= Computed target <computed>]\n\nSee @computed.\n",
        "#if false [#image(\"not-present.svg\")]\n\nNative content survives.\n",
    ];
    for source in cases {
        let directory = project(source);
        let report =
            build(directory.path(), &[Target::Pdf], &mut WorldSlot::new());
        assert!(prepared(&report).diagnostics.has_errors());
        if source.contains("@computed") {
            assert!(
                prepared(&report)
                    .diagnostics
                    .iter()
                    .any(|finding| { finding.code == "unresolved-reference" })
            );
        }
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        pdf_bytes(&report);
    }
}

#[test]
fn fixed_native_timestamp_is_identical_for_fresh_retained_and_edited_sessions()
{
    let directory = project(NATIVE_LOOP);
    let mut worlds = WorldSlot::new();
    let first =
        pdf_bytes(&build(directory.path(), &[Target::Pdf], &mut worlds));
    let second =
        pdf_bytes(&build(directory.path(), &[Target::Pdf], &mut worlds));
    let fresh = pdf_bytes(&build(
        directory.path(),
        &[Target::Pdf],
        &mut WorldSlot::new(),
    ));
    assert_eq!(first, second);
    assert_eq!(first, fresh);
    let text = String::from_utf8_lossy(&first);
    assert!(
        text.contains("2026-10-29T10:34:56")
            || text.contains("D:20261029103456")
    );
    write(
        directory.path(),
        "document.typ",
        "= Edited native source\n\nOne page now.\n",
    );
    let edited =
        pdf_bytes(&build(directory.path(), &[Target::Pdf], &mut worlds));
    assert_ne!(edited, first);
    assert_eq!(pipeline::pdf::page_count(&edited), Some(1));
}

#[test]
fn subset_failure_does_not_block_native_pdf_in_either_target_order() {
    for targets in [
        vec![Target::Html, Target::Docx, Target::Pdf],
        vec![Target::Pdf, Target::Docx, Target::Html],
    ] {
        let directory = project(NATIVE_LOOP);
        let report = build(directory.path(), &targets, &mut WorldSlot::new());
        assert!(report.diagnostics.has_errors());
        assert_eq!(pipeline::pdf::page_count(&pdf_bytes(&report)), Some(2));
        for target in [Target::Html, Target::Docx] {
            let result = report
                .targets
                .iter()
                .find(|result| result.target == target)
                .unwrap();
            assert!(result.written.is_err());
        }
        assert!(!directory.path().join("out/document.html").exists());
        assert!(!directory.path().join("out/document.docx").exists());
    }
}

#[test]
fn native_math_helpers_restore_the_real_main_before_and_after_pdf() {
    let source = "= Shared native document\n\nNative math $a^2 + b^2 = c^2$.\n\n$ #text[kept code] $\n";
    for targets in [
        vec![Target::Html, Target::Docx, Target::Pdf],
        vec![Target::Pdf, Target::Docx, Target::Html],
    ] {
        let directory = project(source);
        let opts = options(directory.path());
        let mut worlds = WorldSlot::new();
        let report = pipeline::build_targets(&opts, &targets, &mut worlds);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        assert!(report.targets.iter().all(|result| result.written.is_ok()));
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|finding| finding.code == "E-TYP-MATH-CODE")
        );
        let original = pdf_bytes(&report);
        let html = fs::read_to_string(&target_paths(&report, Target::Html)[0])
            .unwrap();
        assert!(html.contains("<math") && html.contains("kept code"));
        let body = docx_body(&target_paths(&report, Target::Docx)[0]);
        assert!(body.contains("<m:oMath") && body.contains("kept code"));
        let world = worlds.get(Some(prepared(&report)), &opts).unwrap();
        assert_eq!(world.source(world.main()).unwrap().text(), source);
        let out = directory.path().join("native-after-math.pdf");
        writer::pipeline::typst_source::build(
            prepared(&report),
            &opts,
            &out,
            world,
        )
        .unwrap();
        assert_eq!(fs::read(out).unwrap(), original);
    }
}

#[test]
fn native_math_failure_keeps_html_docx_and_the_previous_pdf_in_either_order() {
    for targets in [
        vec![Target::Html, Target::Docx, Target::Pdf],
        vec![Target::Pdf, Target::Docx, Target::Html],
    ] {
        let directory = project("= Previous good PDF\n");
        let mut worlds = WorldSlot::new();
        let original =
            pdf_bytes(&build(directory.path(), &[Target::Pdf], &mut worlds));
        write(
            directory.path(),
            "document.typ",
            "= Recovery\n\n$ undefinedNativeSymbol(x) $\n",
        );
        let report = build(directory.path(), &targets, &mut worlds);
        assert!(
            !prepared(&report).diagnostics.has_errors(),
            "{:?}",
            prepared(&report).diagnostics
        );
        assert!(
            report
                .targets
                .iter()
                .find(|result| result.target == Target::Pdf)
                .unwrap()
                .written
                .is_err()
        );
        assert_eq!(
            fs::read(directory.path().join("out/document.pdf")).unwrap(),
            original
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|finding| finding.code == "typst-error")
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|finding| finding.code == "math-conversion-failed")
        );
        let html = fs::read_to_string(&target_paths(&report, Target::Html)[0])
            .unwrap();
        assert!(html.contains("undefinedNativeSymbol"));
        assert!(
            docx_body(&target_paths(&report, Target::Docx)[0])
                .contains("undefinedNativeSymbol")
        );
    }
}

#[test]
fn native_compiler_hints_and_real_included_locations_survive_without_new_outputs()
 {
    let directory = project("#include \"child.typ\"\n");
    write(
        directory.path(),
        "child.typ",
        "Child first line.\n#text(body: [bad positional argument])\n",
    );
    let mut opts = options(directory.path());
    opts.cli.out = Some(directory.path().join("not-created/deep/result.pdf"));
    let mut worlds = WorldSlot::new();
    let report = pipeline::build_targets(&opts, &[Target::Pdf], &mut worlds);
    let error = report.targets[0].written.as_ref().unwrap_err();
    let Error::Compile { path, line, hints, .. } = error else {
        panic!("expected native compiler failure: {error:?}");
    };
    let child = fs::canonicalize(directory.path().join("child.typ")).unwrap();
    assert_eq!(path, &child);
    assert_eq!(*line, Some(2));
    assert!(
        hints.iter().any(|hint| hint.contains("removing `body:`")),
        "{hints:?}"
    );
    assert!(!directory.path().join("not-created").exists());
    let world = worlds.get(Some(prepared(&report)), &opts).unwrap();
    world.reset();
    let compiled = compile_pdf(
        world,
        &PdfOptions {
            timestamp: Datetime::from_ymd_hms(2026, 10, 29, 10, 34, 56)
                .unwrap(),
            ident: None,
        },
    );
    assert!(compiled.output.is_none());
    assert_eq!(compiled.diagnostics, report.diagnostics);
    assert!(report.diagnostics.iter().all(|finding| finding.path.as_deref()
        != Some(Path::new("/__writer-main.typ"))));
}

#[test]
fn configured_pdf_template_is_a_native_target_warning_on_success_and_failure() {
    let directory = project("= Native ignores the configured wrapper\n");
    write(
        directory.path(),
        "writer.toml",
        format!("{CONFIG}template='absent-wrapper.typ'\n"),
    );
    let mut worlds = WorldSlot::new();
    let report = build(directory.path(), &[Target::Pdf], &mut worlds);
    let source = directory.path().join("document.typ");
    let warnings = report
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "W-TYP-TEMPLATE-IGNORED")
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].level, Level::Warning);
    assert_eq!(warnings[0].path.as_deref(), Some(source.as_path()));
    let original = pdf_bytes(&report);
    let mut opts = options(directory.path());
    opts.cli.strict = Some(true);
    let strict = pipeline::build_targets(&opts, &[Target::Pdf], &mut worlds);
    assert!(
        strict
            .diagnostics
            .iter()
            .any(|finding| finding.code == "W-TYP-TEMPLATE-IGNORED"
                && finding.level == Level::Error)
    );
    assert_eq!(pdf_bytes(&strict), original);
    write(
        directory.path(),
        "document.typ",
        "#panic(\"native-template-failure\")\n",
    );
    let failed = build(directory.path(), &[Target::Pdf], &mut worlds);
    assert!(failed.targets[0].written.is_err());
    assert!(
        failed
            .diagnostics
            .iter()
            .any(|finding| finding.code == "W-TYP-TEMPLATE-IGNORED")
    );
    assert!(
        failed.diagnostics.iter().any(|finding| finding.code == "typst-error")
    );
    assert_eq!(
        fs::read(directory.path().join("out/document.pdf")).unwrap(),
        original
    );
}

fn dynamic_input(root: &Path, image: bool) -> (PathBuf, Vec<u8>) {
    if image {
        write(root, "document.typ", "#image(\"dynamic\" + \".svg\")\n");
        write(root, "dynamic.svg", SVG);
        (root.join("dynamic.svg"), SVG.to_vec())
    } else {
        write(root, "document.typ", "#include (\"dynamic\" + \".typ\")\n");
        let text = b"Native dynamic include.\n";
        write(root, "dynamic.typ", text);
        (root.join("dynamic.typ"), text.to_vec())
    }
}

fn refused_dynamic_output(root: &Path, output: PathBuf) -> BuildReport {
    let mut opts = options(root);
    opts.cli.out = Some(output);
    let report =
        pipeline::build_targets(&opts, &[Target::Pdf], &mut WorldSlot::new());
    let error = report.targets[0].written.as_ref().unwrap_err();
    assert!(pipeline::refused_clash(error).is_some(), "{error:?}");
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|finding| finding.code.starts_with("E-TYP-"))
    );
    report
}

#[test]
fn dynamic_includes_and_images_cannot_be_overwritten_even_through_new_parent_aliases()
 {
    for image in [false, true] {
        let directory = project("unused\n");
        let (consumed, bytes) = dynamic_input(directory.path(), image);
        let parsed = pipeline::prepare(&options(directory.path())).unwrap();
        assert!(!parsed.document.includes.contains(&consumed));
        assert!(parsed.assets.is_empty());
        let report = refused_dynamic_output(directory.path(), consumed.clone());
        let error = report.targets[0].written.as_ref().unwrap_err();
        let clash = pipeline::refused_clash(error).unwrap();
        assert_eq!(
            clash.role,
            if image {
                pipeline::InputRole::Asset
            } else {
                pipeline::InputRole::Include
            }
        );
        assert_eq!(fs::read(&consumed).unwrap(), bytes);
        let alias = directory
            .path()
            .join("not-created/..")
            .join(consumed.file_name().unwrap());
        refused_dynamic_output(directory.path(), alias);
        assert!(!directory.path().join("not-created").exists());
        assert!(!directory.path().join("out").exists());
        assert_eq!(fs::read(consumed).unwrap(), bytes);
    }
}

#[cfg(unix)]
#[test]
fn dynamic_consumed_file_symlinks_and_hard_links_are_protected() {
    for image in [false, true] {
        for hard_link in [false, true] {
            let directory = project("unused\n");
            let (consumed, bytes) = dynamic_input(directory.path(), image);
            let alias = directory.path().join("output-alias.pdf");
            if hard_link {
                fs::hard_link(&consumed, &alias).unwrap();
            } else {
                std::os::unix::fs::symlink(&consumed, &alias).unwrap();
            }
            refused_dynamic_output(directory.path(), alias.clone());
            assert_eq!(fs::read(&consumed).unwrap(), bytes);
            assert_eq!(fs::read(alias).unwrap(), bytes);
            assert!(!directory.path().join("out").exists());
        }
    }
}

#[test]
fn a_disk_font_selected_in_the_retained_session_cannot_be_overwritten() {
    let directory = project("Native font safety.\n");
    let (embedded, _) = typst_kit::fonts::embedded().next().unwrap();
    let font = directory.path().join("fonts/copied.otf");
    write(directory.path(), "fonts/copied.otf", embedded.data());
    write(
        directory.path(),
        "writer.toml",
        format!("{CONFIG}font_paths=['fonts']\n"),
    );
    let opts = options(directory.path());
    let prepared = pipeline::prepare(&opts).unwrap();
    let mut world = WriterWorld::from_config(
        &prepared.config,
        directory.path(),
        MainFile::Path(opts.input.clone()),
        Datetime::from_ymd_hms(2026, 10, 29, 10, 34, 56).unwrap(),
    )
    .unwrap();
    // Select the copied face through the public World API. Identical embedded
    // families take precedence during ordinary layout, so merely adding a copy
    // to font_paths would not select the disk font or prove consumed-file safety.
    let index = typst_kit::fonts::embedded().count();
    assert!(world.font(index).is_some());
    assert!(world.accessed().contains(&fs::canonicalize(&font).unwrap()));
    let original = fs::read(&font).unwrap();
    let error = writer::pipeline::typst_source::build(
        &prepared, &opts, &font, &mut world,
    )
    .unwrap_err();
    let clash = pipeline::refused_clash(&error).unwrap();
    assert_eq!(clash.role, pipeline::InputRole::Asset);
    assert_eq!(fs::read(font).unwrap(), original);
    assert!(!directory.path().join("out").exists());
}

#[test]
fn cli_defaults_dedup_format_override_named_output_and_quiet_are_native() {
    let directory = project(NATIVE_LOOP);
    let result = writer(directory.path())
        .args(["build", "document.typ"])
        .output()
        .unwrap();
    success(&result);
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    assert!(stdout.contains("document.pdf") && stdout.contains("2 pages"));
    let first = fs::read(directory.path().join("out/document.pdf")).unwrap();
    let named = directory.path().join("custom/report.final.pdf");
    let result = writer(directory.path())
        .args(["build", "document.typ", "--to", "pdf,pdf", "-o"])
        .arg(&named)
        .output()
        .unwrap();
    success(&result);
    assert_eq!(String::from_utf8(result.stdout).unwrap().lines().count(), 1);
    assert_eq!(fs::read(&named).unwrap(), first);
    assert!(!named.with_file_name("document.pdf").exists());
    write(directory.path(), "native.source", NATIVE_LOOP);
    let result = writer(directory.path())
        .args(["build", "native.source", "--format", "typ", "-o", "format.pdf"])
        .output()
        .unwrap();
    success(&result);
    assert_eq!(
        pipeline::pdf::page_count(
            &fs::read(directory.path().join("format.pdf")).unwrap()
        ),
        Some(2)
    );
    let result = writer(directory.path())
        .args([
            "build", "document.typ", "--engine", "chrome", "-o",
            "native-engine.pdf",
        ])
        .output()
        .unwrap();
    success(&result);
    assert_eq!(
        pipeline::pdf::page_count(
            &fs::read(directory.path().join("native-engine.pdf")).unwrap()
        ),
        Some(2)
    );
    let result = writer(directory.path())
        .args(["-q", "build", "document.typ"])
        .output()
        .unwrap();
    success(&result);
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn cli_mixed_subset_failure_writes_pdf_and_reports_each_failed_target() {
    for targets in ["html,docx,pdf", "pdf,docx,html"] {
        let directory = project(NATIVE_LOOP);
        let result = writer(directory.path())
            .args(["build", "document.typ", "--to", targets, "-o", "mixed"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        let output = directory.path().join("mixed");
        assert_eq!(
            pipeline::pdf::page_count(
                &fs::read(output.join("document.pdf")).unwrap()
            ),
            Some(2)
        );
        assert!(!output.join("document.html").exists());
        assert!(!output.join("document.docx").exists());
        let stdout = String::from_utf8(result.stdout).unwrap();
        assert_eq!(stdout.lines().count(), 1);
        assert!(stdout.contains("document.pdf"));
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(
            stderr.contains("structured subset") || stderr.contains("E-TYP-"),
            "{stderr}"
        );
    }
}

#[test]
fn cli_native_compiler_errors_are_reported_once_with_real_file_and_hints() {
    let directory = project("#include \"child.typ\"\n");
    write(
        directory.path(),
        "child.typ",
        "#text(body: [bad positional argument])\n",
    );
    let result = writer(directory.path())
        .args(["build", "document.typ"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert_eq!(
        stderr.matches("the argument `body` is positional").count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("child.typ:1"), "{stderr}");
    assert!(stderr.contains("try removing `body:`"), "{stderr}");
    assert!(!stderr.contains("/__writer-main.typ"));
    assert!(!directory.path().join("out").exists());
}

#[test]
fn cli_template_warning_is_visible_but_quiet_suppresses_only_the_warning() {
    let directory = project("Native template warning.\n");
    write(
        directory.path(),
        "writer.toml",
        format!("{CONFIG}template='unused.typ'\n"),
    );
    let result = writer(directory.path())
        .args(["build", "document.typ"])
        .output()
        .unwrap();
    success(&result);
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("template is ignored")
    );
    let result = writer(directory.path())
        .args(["-q", "build", "document.typ"])
        .output()
        .unwrap();
    success(&result);
    assert!(result.stderr.is_empty());
    let result = writer(directory.path())
        .args(["build", "document.typ", "--strict"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("template is ignored")
    );
    assert!(directory.path().join("out/document.pdf").is_file());
}

#[test]
fn cli_dynamic_consumed_output_refusal_preserves_original_bytes() {
    let directory = project("unused\n");
    let (consumed, original) = dynamic_input(directory.path(), false);
    let result = writer(directory.path())
        .args(["build", "document.typ", "-o"])
        .arg(&consumed)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8(result.stderr).unwrap().contains("refusing to write")
    );
    assert_eq!(fs::read(consumed).unwrap(), original);
}

#[test]
fn native_source_cannot_be_exported_to_typst() {
    let directory = project("Native input.\n");
    let result = writer(directory.path())
        .args(["build", "document.typ", "--to", "typst"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("writer build part.md --to typst")
    );
    assert!(!directory.path().join("out").exists());
}

#[test]
fn missing_subset_image_is_retained_once_while_native_pdf_reaches_the_compiler()
{
    for targets in [
        vec![Target::Html, Target::Docx, Target::Pdf],
        vec![Target::Pdf, Target::Docx, Target::Html],
    ] {
        let directory = project("#image(\"missing.svg\")\n");
        let report = build(directory.path(), &targets, &mut WorldSlot::new());
        assert!(report.prepared.is_ok());
        let missing = report
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "missing-asset")
            .collect::<Vec<_>>();
        assert_eq!(missing.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(missing[0].level, Level::Error);
        for result in &report.targets {
            let error = result.written.as_ref().unwrap_err();
            if result.target == Target::Pdf {
                assert!(matches!(error, Error::Compile { .. }), "{error:?}");
            } else {
                assert!(
                    matches!(error, Error::MissingAsset { .. }),
                    "{error:?}"
                );
            }
        }
        assert!(
            report
                .diagnostics
                .iter()
                .any(|finding| finding.code == "typst-error")
        );
        assert!(!directory.path().join("out").exists());
    }
}

#[test]
fn cli_mixed_missing_image_reports_the_missing_finding_once_and_native_failure_separately()
 {
    let directory = project("#image(\"missing.svg\")\n");
    let result = writer(directory.path())
        .args(["build", "document.typ", "--to", "html,pdf,docx"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8(result.stderr).unwrap();
    let missing_errors = stderr
        .lines()
        .filter(|line| {
            line.contains("error:")
                && (line.contains("looked for")
                    || line.contains("asset not found:"))
        })
        .count();
    assert_eq!(missing_errors, 1, "{stderr}");
    assert_eq!(
        stderr
            .lines()
            .filter(|line| {
                line.contains("error:") && line.contains("file not found")
            })
            .count(),
        1,
        "{stderr}"
    );
    assert!(!directory.path().join("out").exists());
}

#[test]
fn native_example_builds_pdf_html_and_editable_docx_offline() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/typst");
    let directory = tempfile::tempdir().unwrap();
    for name in ["document.typ", "writer.toml", "figures/flow.svg"] {
        write(directory.path(), name, fs::read(example.join(name)).unwrap());
    }
    let report = build(
        directory.path(),
        &[Target::Html, Target::Docx, Target::Pdf],
        &mut WorldSlot::new(),
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.targets.iter().all(|result| result.written.is_ok()));
    pdf_bytes(&report);
    let html =
        fs::read_to_string(&target_paths(&report, Target::Html)[0]).unwrap();
    assert!(html.contains("A native Typst document") && html.contains("<math"));
    let body = docx_body(&target_paths(&report, Target::Docx)[0]);
    assert!(
        body.contains("A native Typst document") && body.contains("<m:oMath")
    );
}

#[test]
fn cli_typst_format_override_keeps_markdown_and_notebook_mains_native() {
    for name in ["document.md", "document.ipynb"] {
        let directory = project(NATIVE_LOOP);
        let root = directory.path();
        write(root, name, NATIVE_LOOP);
        let output = root.join("native-override.pdf");
        let result = writer(root)
            .arg("build")
            .arg(root.join(name))
            .args(["--format", "typ", "--to", "pdf", "-o"])
            .arg(&output)
            .output()
            .unwrap();
        success(&result);
        assert!(
            result.stderr.is_empty(),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let bytes = fs::read(&output).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "{name}");
        assert_eq!(pipeline::pdf::page_count(&bytes), Some(2), "{name}");
        assert_eq!(fs::read_to_string(root.join(name)).unwrap(), NATIVE_LOOP);
    }
}

#[test]
fn overridden_native_main_survives_html_docx_math_helpers_before_pdf() {
    let source = "= Overridden native document\n\nNative math $a^2 + b^2 = c^2$.\n\n$ #text[kept code] $\n";
    for name in ["document.md", "document.ipynb"] {
        let directory = project(source);
        let root = directory.path();
        write(root, name, source);
        let mut opts = options(root);
        opts.input = root.join(name);
        opts.format = Some(writer::ir::SourceKind::Typst);
        let mut worlds = WorldSlot::new();
        let report = pipeline::build_targets(
            &opts,
            &[Target::Html, Target::Docx, Target::Pdf],
            &mut worlds,
        );
        assert!(
            !report.diagnostics.has_errors(),
            "{name}: {:?}",
            report.diagnostics
        );
        assert!(
            report.targets.iter().all(|result| result.written.is_ok()),
            "{name}: {:?}",
            report.targets
        );
        assert_eq!(
            prepared(&report).document.source.kind,
            writer::ir::SourceKind::Typst
        );
        let original = pdf_bytes(&report);
        assert_eq!(pipeline::pdf::page_count(&original), Some(1), "{name}");
        let html = fs::read_to_string(&target_paths(&report, Target::Html)[0])
            .unwrap();
        assert!(html.contains("<math") && html.contains("kept code"), "{name}");
        let body = docx_body(&target_paths(&report, Target::Docx)[0]);
        assert!(
            body.contains("<m:oMath") && body.contains("kept code"),
            "{name}"
        );

        let world = worlds.get(Some(prepared(&report)), &opts).unwrap();
        assert_eq!(
            world.source(world.main()).unwrap().text(),
            source,
            "{name}"
        );
        let (document, _) = compile_document(world).into_result().unwrap();
        let text = contents(&document);
        assert!(text.contains("Overriddennativedocument"), "{name}: {text}");
        assert!(text.contains("keptcode"), "{name}: {text}");
        let out = root.join("native-after-overridden-math.pdf");
        writer::pipeline::typst_source::build(
            prepared(&report),
            &opts,
            &out,
            world,
        )
        .unwrap();
        assert_eq!(fs::read(out).unwrap(), original, "{name}");
        assert_eq!(fs::read_to_string(root.join(name)).unwrap(), source);
    }
}
