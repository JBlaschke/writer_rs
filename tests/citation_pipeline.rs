//! Real configured citation loading, target output, lint/reload and CLI safety.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;
use typst::layout::{Frame, FrameItem};
use writer::config::Target;
use writer::error::{Error, Level};
use writer::ir::{self, Block, Inline};
use writer::pipeline::{self, BuildOptions, InputRole, Mode, WorldSlot};
use writer::typst::compile::compile_document;

#[path = "common/math_xml.rs"]
mod math_xml;

const BIB: &[u8] = include_bytes!("fixtures/bib/refs.bib");
const CSL: &[u8] = include_bytes!("fixtures/bib/minimal.csl");
const YAML: &[u8] = include_bytes!("fixtures/bib/refs.yaml");
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const NATIVE: &str = "#let twice(x) = x * 2\nNative #twice(3).\n";

fn settings(style: &str) -> String {
    format!(
        "[build]\nout_dir='out'\n[pdf]\nsystem_fonts=false\nallow_packages=false\n\
         timestamp='2026-10-29T00:00:00Z'\n[references]\nbibliography=['refs.bib']\n\
         style='{style}'\n",
    )
}

fn write(root: &Path, name: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn project(source: &str, style: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "document.md", source);
    write(directory.path(), "refs.bib", BIB);
    write(directory.path(), "writer.toml", settings(style));
    directory
}

fn options(root: &Path) -> BuildOptions {
    let mut options = BuildOptions::new(root.join("document.md"));
    options.config_path = Some(root.join("writer.toml"));
    options
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

fn prepare(root: &Path) -> pipeline::Prepared {
    let prepared = pipeline::prepare(&options(root)).unwrap();
    assert!(!prepared.diagnostics.has_errors(), "{:?}", prepared.diagnostics);
    assert!(prepared.document.invariant_violations().is_empty());
    prepared
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

fn xml_text(xml: &str) -> String {
    let document = math_xml::parse(xml);
    assert_eq!(document.attr("xmlns:w"), Some(W));
    document
        .descendants("w:t")
        .into_iter()
        .map(|node| node.text.as_str())
        .collect()
}

fn compact(text: &str) -> String {
    text.chars().filter(|character| !character.is_whitespace()).collect()
}

fn frame_text(frame: &Frame, text: &mut String) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => frame_text(&group.frame, text),
            FrameItem::Text(run) => text.push_str(&run.text),
            _ => {}
        }
    }
}

fn citation_text(document: &ir::Document) -> String {
    let mut value = None;
    ir::walk_inlines(document, &mut |inline| {
        if value.is_none()
            && let Inline::Cite { resolved: Some(citation), .. } = inline
        {
            let note = citation.inlines.iter().find_map(|inline| {
                let Inline::Footnote { blocks, .. } = inline else {
                    return None;
                };
                let Block::Paragraph { inlines, .. } = &blocks[0] else {
                    return None;
                };
                Some(ir::plain_text(inlines))
            });
            value =
                Some(note.unwrap_or_else(|| ir::plain_text(&citation.inlines)));
        }
    });
    value.expect("one real citation")
}

#[test]
fn all_targets_use_real_citations_and_preserve_bibliography_bytes_in_both_orders()
 {
    let directory = project(
        "# Result {#sec-result}\n\nSee @sec-result and @doe2020.\n\n\
         Read @knuth1984[p. 12].\n\nHidden [@lee2019]{.none}.\n",
        "ieee",
    );
    let root = directory.path();
    let options = options(root);
    let mut worlds = WorldSlot::new();
    for targets in [
        [Target::Pdf, Target::Docx, Target::Html, Target::Typst, Target::Latex],
        [Target::Latex, Target::Typst, Target::Html, Target::Docx, Target::Pdf],
    ] {
        let report = pipeline::build_targets(&options, &targets, &mut worlds);
        assert!(report.prepared.is_ok(), "{:?}", report.prepared);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        assert_eq!(report.targets.len(), targets.len());
        for result in &report.targets {
            assert!(result.written.is_ok(), "{result:?}");
        }
        let html = fs::read_to_string(root.join("out/document.html")).unwrap();
        assert!(html.contains("data-cites=\"doe2020\""), "{html}");
        assert!(html.contains("href=\"#sec-result\""), "{html}");
        assert!(
            html.contains("id=\"refs\"")
                && html.contains("id=\"ref-knuth1984\"")
        );
        assert!(html.contains("[1]") && html.contains("The TeXbook"));
        assert!(!html.contains("[?"));

        let typst = fs::read_to_string(root.join("out/document.typ")).unwrap();
        assert!(
            typst.contains("@doe2020") && typst.contains("@knuth1984[p. 12]")
        );
        assert!(typst.contains("#cite(<lee2019>, form: none)"));
        assert!(typst.contains("#bibliography("));
        let latex = fs::read_to_string(root.join("out/document.tex")).unwrap();
        assert!(latex.contains("\\autocite{doe2020}"), "{latex}");
        assert!(latex.contains("\\autocite[p.~12]{knuth1984}"), "{latex}");
        assert!(latex.contains("\\nocite{lee2019}"), "{latex}");
        assert!(latex.contains("\\printbibliography"));
        assert!(!latex.contains("{[}?"));
        assert_eq!(
            fs::read(root.join("out/document_assets/refs.bib")).unwrap(),
            BIB
        );

        let bytes = fs::read(root.join("out/document.docx")).unwrap();
        assert!(
            writer::docx::validate(
                &bytes,
                &writer::docx::ValidateOptions { builtin_styles: true }
            )
            .is_empty()
        );
        let docx = parts(&bytes);
        let xml = String::from_utf8(docx["word/document.xml"].clone()).unwrap();
        let text = xml_text(&xml);
        assert!(text.contains("[1]") && text.contains("The TeXbook"), "{text}");
        let document = math_xml::parse(&xml);
        assert!(
            document
                .descendants("w:bookmarkStart")
                .iter()
                .any(|node| node.attr("w:name") == Some("_refs"))
        );
        let pdf = fs::read(root.join("out/document.pdf")).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(
            pipeline::pdf::page_count(&pdf).is_some_and(|pages| pages >= 1)
        );
    }
}

#[test]
fn actual_typst_citations_match_resolved_text_for_archived_and_file_styles() {
    for style in ["ieee", "apa", "chicago-notes", "styles/minimal.xml"] {
        let directory = project("A citation @knuth1984[p. 12].\n", style);
        let root = directory.path();
        write(root, "styles/minimal.xml", CSL);
        let prepared = prepare(root);
        let expected = compact(&citation_text(&prepared.document));
        assert!(!expected.is_empty());
        let options = options(root);
        let mut worlds = WorldSlot::new();
        let world = worlds.get(Some(&prepared), &options).unwrap();
        pipeline::load_typst_main(&prepared, &options, world).unwrap();
        let (document, diagnostics) =
            compile_document(world).into_result().unwrap();
        assert!(!diagnostics.has_errors(), "{style}: {diagnostics:?}");
        let mut actual = String::new();
        for page in document.pages() {
            frame_text(&page.frame, &mut actual);
        }
        assert!(
            compact(&actual).contains(&expected),
            "{style}: expected {expected:?} in {actual:?}"
        );
        assert!(compact(&actual).contains("TheTeXbook"), "{style}: {actual}");
    }
}

#[test]
fn generated_note_citations_are_real_html_and_editable_docx_notes_once() {
    let directory = project(
        "Regular[^a], cite @doe2020.\n\nRepeated[^a].\n\nParent[^b].\n\n\
         [^a]: Ordinary note.\n[^b]: Parent with @knuth1984.\n",
        "chicago-notes",
    );
    let root = directory.path();
    let report = pipeline::build_targets(
        &options(root),
        &[Target::Html, Target::Docx],
        &mut WorldSlot::new(),
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(
        report.targets.iter().all(|result| result.written.is_ok()),
        "{:?}",
        report.targets
    );
    let html = fs::read_to_string(root.join("out/document.html")).unwrap();
    for number in 1..=4 {
        assert_eq!(
            html.matches(&format!("id=\"fn-{number}\"")).count(),
            1,
            "{html}"
        );
    }
    assert!(html.contains("id=\"fnref-1-2\""));
    assert!(!html.contains("id=\"fn-5\""));
    let bytes = fs::read(root.join("out/document.docx")).unwrap();
    assert!(
        writer::docx::validate(
            &bytes,
            &writer::docx::ValidateOptions { builtin_styles: true }
        )
        .is_empty()
    );
    let docx = parts(&bytes);
    let xml = String::from_utf8(docx["word/document.xml"].clone()).unwrap();
    let document = math_xml::parse(&xml);
    assert_eq!(document.attr("xmlns:w"), Some(W));
    assert_eq!(document.descendants("w:footnoteReference").len(), 4);
    let notes_xml =
        String::from_utf8(docx["word/footnotes.xml"].clone()).unwrap();
    let notes = math_xml::parse(&notes_xml);
    assert_eq!(notes.attr("xmlns:w"), Some(W));
    let ids: Vec<_> = notes
        .descendants("w:footnote")
        .into_iter()
        .filter_map(|node| node.attr("w:id"))
        .filter(|id| id.parse::<i32>().is_ok_and(|id| id > 0))
        .collect();
    assert_eq!(ids, ["1", "2", "3", "4"]);
    let note_text = xml_text(&notes_xml);
    assert!(
        note_text.contains("A Study of Things")
            && note_text.contains("The TeXbook")
    );
}

#[test]
fn yaml_citations_build_html_docx_and_pdf_while_latex_refuses_its_own_target() {
    let directory = project("Read @k1.\n", "apa");
    let root = directory.path();
    write(root, "refs.yaml", YAML);
    write(
        root,
        "writer.toml",
        settings("apa").replace("refs.bib", "refs.yaml"),
    );
    let report = pipeline::build_targets(
        &options(root),
        &[Target::Latex, Target::Html, Target::Docx, Target::Pdf],
        &mut WorldSlot::new(),
    );
    assert!(report.prepared.is_ok());
    assert_eq!(report.targets.len(), 4);
    assert!(report.targets[0].written.is_err());
    assert!(
        report.targets[1..].iter().all(|target| target.written.is_ok()),
        "{:?}",
        report.targets
    );
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|finding| finding.level == Level::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(errors[0].code, "latex-bib-yaml");
    assert!(!root.join("out/document.tex").exists());
    assert!(
        fs::read_to_string(root.join("out/document.html"))
            .unwrap()
            .contains("Kwan")
    );
    assert_eq!(fs::read(root.join("refs.yaml")).unwrap(), YAML);
}

#[test]
fn build_and_dump_fail_global_resource_errors_without_writing_or_truncating() {
    let directory = project("Read @doe2020.\n", "ieee");
    let root = directory.path();
    write(root, "out/document.html", "Last good HTML");
    for (configuration, expected, category) in [
        (
            settings("ieee").replace("refs.bib", "missing.bib"),
            "missing.bib",
            "bibliography",
        ),
        (settings("missing.xml"), "missing.xml", "citation style"),
        (
            settings("unknown-style"),
            "unknown style: unknown-style",
            "citation style",
        ),
    ] {
        write(root, "writer.toml", configuration);
        let failure = pipeline::prepare(&options(root)).unwrap_err();
        assert!(
            matches!(failure.error.as_ref(), Error::Resource { what, .. } if *what == category)
        );
        let output = writer(root)
            .args(["build", "document.md", "--to", "html"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(stderr.matches(expected).count(), 1, "{stderr}");
        assert_eq!(
            fs::read(root.join("out/document.html")).unwrap(),
            b"Last good HTML"
        );
        let output = writer(root)
            .args(["dump", "document.md", "--ir"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
        assert!(!root.join("out/document_assets").exists());
    }
}

#[test]
fn configured_resource_errors_are_fatal_before_native_only_pdf_authority() {
    let directory = project("unused", "ieee");
    let root = directory.path();
    write(root, "document.typ", NATIVE);
    let mut options = options(root);
    options.input = root.join("document.typ");
    options.cli.out = Some(root.join("never/created/native.pdf"));
    for (configuration, category) in [
        (settings("ieee").replace("refs.bib", "missing.bib"), "bibliography"),
        (settings("missing.xml"), "citation style"),
    ] {
        write(root, "writer.toml", configuration);
        let report = pipeline::build_targets(
            &options,
            &[Target::Pdf],
            &mut WorldSlot::new(),
        );
        let failure = report.prepared.unwrap_err();
        assert!(
            matches!(failure.error.as_ref(), Error::Resource { what, .. } if *what == category)
        );
        assert!(report.targets.is_empty());
        assert!(!root.join("never").exists());
    }
}

#[test]
fn native_preview_retains_global_resource_findings_while_native_pdf_still_builds()
 {
    let directory = project("unused", "missing.xml");
    let root = directory.path();
    write(root, "document.typ", NATIVE);
    for mode in [Mode::Serve { build_id: 1 }, Mode::Check] {
        let mut options = options(root);
        options.input = root.join("document.typ");
        options.mode = mode;
        let report = pipeline::build_targets(
            &options,
            &[Target::Pdf],
            &mut WorldSlot::new(),
        );
        assert!(report.prepared.is_ok());
        assert_eq!(report.targets.len(), 1);
        assert!(report.targets[0].written.is_ok(), "{:?}", report.targets);
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(
            report.diagnostics.iter().next().unwrap().code,
            "citation-style-invalid"
        );
        assert_eq!(
            report.diagnostics.iter().next().unwrap().level,
            Level::Error
        );
    }
}

#[test]
fn check_and_preview_report_bad_styles_without_a_library_then_recover() {
    let directory = project("Read @doe2020.\n", "styles/local.xml");
    let root = directory.path();
    let mut options = options(root);
    options.mode = Mode::Serve { build_id: 1 };
    let prepared = pipeline::prepare(&options).unwrap();
    assert_eq!(
        prepared
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "citation-style-invalid"
                && finding.level == Level::Error)
            .count(),
        1
    );
    assert_eq!(
        prepared
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "unresolved-reference"
                && finding.level == Level::Warning)
            .count(),
        1
    );
    assert!(prepared.document.resolved.bibliography.is_none());
    assert!(
        pipeline::watch_inputs(&prepared).contains(&(
            root.join("styles/local.xml"),
            InputRole::Bibliography,
        ))
    );

    let output = writer(root)
        .args(["check", "document.md", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let findings: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        findings
            .iter()
            .filter(|finding| finding["code"] == "citation-style-invalid"
                && finding["level"] == "error")
            .count(),
        1
    );
    assert_eq!(
        findings
            .iter()
            .filter(|finding| finding["code"] == "unresolved-reference"
                && finding["level"] == "warning")
            .count(),
        1
    );
    assert!(!root.join("out").exists());

    let mut worlds = WorldSlot::new();
    let failed =
        pipeline::build_targets(&options, &[Target::Html], &mut worlds);
    assert!(failed.targets[0].written.is_ok());
    assert!(
        fs::read_to_string(root.join("out/document.html"))
            .unwrap()
            .contains("[?doe2020]")
    );
    write(root, "styles/local.xml", CSL);
    options.mode = Mode::Serve { build_id: 2 };
    let recovered =
        pipeline::build_targets(&options, &[Target::Html], &mut worlds);
    assert!(!recovered.diagnostics.has_errors(), "{:?}", recovered.diagnostics);
    assert!(recovered.targets[0].written.is_ok());
    let html = fs::read_to_string(root.join("out/document.html")).unwrap();
    assert!(!html.contains("[?doe2020]"));
    assert!(html.contains("(1)") && html.contains("id=\"refs\""));
    success(
        &writer(root)
            .args(["check", "document.md", "--format", "json"])
            .output()
            .unwrap(),
    );
}

#[test]
fn duplicate_library_errors_remain_single_check_findings_and_recover_after_edit()
 {
    let directory = project("Read @shared.\n", "ieee");
    let root = directory.path();
    write(root, "a.bib", include_bytes!("fixtures/bib/dup-a.bib"));
    write(root, "b.bib", include_bytes!("fixtures/bib/dup-b.bib"));
    write(
        root,
        "writer.toml",
        settings("ieee").replace("['refs.bib']", "['a.bib','b.bib']"),
    );
    let output = writer(root)
        .args(["check", "document.md", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let findings: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let bibliography: Vec<_> = findings
        .iter()
        .filter(|finding| finding["code"] == "bibliography-error")
        .collect();
    assert_eq!(bibliography.len(), 1);
    assert!(
        bibliography[0]["message"]
            .as_str()
            .unwrap()
            .contains("duplicate bibliography keys: shared")
    );
    write(root, "b.bib", "@book{new,title={Recovered},year={2026}}\n");
    success(
        &writer(root)
            .args(["check", "document.md", "--format", "json"])
            .output()
            .unwrap(),
    );
    assert!(prepare(root).document.resolved.bibliography.is_some());
}

#[test]
fn malformed_library_unknown_format_and_dependent_styles_have_named_findings() {
    let dependent = r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0">
<info><title>Dependent</title><id>https://example.org/dependent</id>
<link href="https://example.org/parent" rel="independent-parent"/>
<updated>2026-10-29T00:00:00+00:00</updated></info></style>"#;
    for (file, contents, configuration, code, message) in [
        (
            "refs.bib",
            "@book{",
            settings("ieee"),
            "bibliography-error",
            "biblatex parse error",
        ),
        (
            "refs.json",
            "{}",
            settings("ieee").replace("refs.bib", "refs.json"),
            "bibliography-error",
            "unknown bibliography format",
        ),
        (
            "local.xml",
            dependent,
            settings("local.xml"),
            "citation-style-invalid",
            "dependent CSL styles are not supported",
        ),
        (
            "local.xml",
            "<bad-style>",
            settings("local.xml"),
            "citation-style-invalid",
            "",
        ),
    ] {
        let directory = project("Read @doe2020.\n", "ieee");
        let root = directory.path();
        write(root, file, contents);
        write(root, "writer.toml", configuration);
        let mut options = options(root);
        assert!(pipeline::prepare(&options).is_err());
        options.mode = Mode::Check;
        let prepared = pipeline::prepare(&options).unwrap();
        let findings: Vec<_> = prepared
            .diagnostics
            .iter()
            .filter(|finding| finding.code == code)
            .collect();
        assert_eq!(findings.len(), 1, "{:?}", prepared.diagnostics);
        assert_eq!(findings[0].level, Level::Error);
        assert_eq!(
            findings[0].path.as_deref(),
            Some(root.join(file).as_path())
        );
        assert!(findings[0].message.contains(message), "{:?}", findings[0]);
        assert!(prepared.document.resolved.bibliography.is_none());
        assert_eq!(
            prepared
                .diagnostics
                .iter()
                .filter(|finding| {
                    finding.code == "unresolved-reference"
                        && finding.level == Level::Warning
                })
                .count(),
            1
        );
        assert!(!root.join("out").exists());
    }
}

#[test]
fn unknown_keys_fail_build_once_and_remain_warnings_in_check_with_a_library() {
    let directory = project("Unknown @not-a-key.\n", "ieee");
    let root = directory.path();
    let output = writer(root)
        .args(["build", "document.md", "--to", "html"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr.matches("does not exist in the document").count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("no bibliography key either"));
    let before = fs::read(root.join("out/document.html")).unwrap();
    assert!(String::from_utf8_lossy(&before).contains("[?not-a-key]"));
    let output = writer(root)
        .args(["check", "document.md", "--format", "json"])
        .output()
        .unwrap();
    success(&output);
    let findings: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["code"], "unresolved-reference");
    assert_eq!(findings[0]["level"], "warning");
    assert_eq!(fs::read(root.join("out/document.html")).unwrap(), before);
}

#[test]
fn xml_styles_follow_config_and_frontmatter_bases_and_are_protected_inputs() {
    let directory = project("Read @doe2020.\n", "styles/local.xml");
    let root = directory.path();
    write(root, "styles/local.xml", CSL);
    for frontmatter in [false, true] {
        if frontmatter {
            write(
                root,
                "nested/document.md",
                "---\ncite_style: ../styles/local.xml\n---\n\nRead @doe2020.\n",
            );
        }
        let mut options = options(root);
        if frontmatter {
            options.input = root.join("nested/document.md");
        }
        let prepared = pipeline::prepare(&options).unwrap();
        let style = root.join("styles/local.xml");
        assert_eq!(Path::new(&prepared.config.references.style), style);
        assert!(
            pipeline::watch_inputs(&prepared)
                .contains(&(style.clone(), InputRole::Bibliography))
        );
        for input in [style, root.join("refs.bib")] {
            let original = fs::read(&input).unwrap();
            options.cli.out = Some(input.clone());
            let report = pipeline::build_targets(
                &options,
                &[Target::Html],
                &mut WorldSlot::new(),
            );
            assert!(report.prepared.is_ok());
            assert_eq!(report.targets.len(), 1);
            let error = report.targets[0].written.as_ref().unwrap_err();
            assert!(error.to_string().contains("refusing to write"), "{error}");
            assert_eq!(fs::read(input).unwrap(), original);
        }
    }
}

#[test]
fn invalid_citation_locale_warns_once_then_uses_document_locale_and_valid_override()
 {
    let directory =
        project("---\nlang: de-AT\n---\n\nRead @doe2020.\n", "local.xml");
    let root = directory.path();
    let style = String::from_utf8(CSL.to_vec()).unwrap().replace(
        "<text variable=\"citation-number\"/>",
        "<text term=\"and\"/><text variable=\"citation-number\"/>",
    );
    write(root, "local.xml", style);
    write(
        root,
        "writer.toml",
        format!("{}locale='x'\n", settings("local.xml")),
    );
    let invalid = prepare(root);
    assert_eq!(
        invalid
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "lang-invalid")
            .count(),
        1
    );
    assert_eq!(citation_text(&invalid.document), "(und1)");
    write(
        root,
        "writer.toml",
        format!("{}locale='en_US'\n", settings("local.xml")),
    );
    let valid = prepare(root);
    assert!(
        valid.diagnostics.iter().all(|finding| finding.code != "lang-invalid")
    );
    assert_eq!(citation_text(&valid.document), "(and1)");
}

#[test]
fn absent_bibliography_does_not_load_an_unused_style_and_unknown_keys_stay_lenient_in_check()
 {
    let directory = project("No citations here.\n", "missing.xml");
    let root = directory.path();
    write(
        root,
        "writer.toml",
        settings("missing.xml").replace("['refs.bib']", "[]"),
    );
    assert!(prepare(root).diagnostics.is_empty());
    write(root, "document.md", "Unknown @not-a-key.\n");
    let output = writer(root)
        .args(["check", "document.md", "--format", "json"])
        .output()
        .unwrap();
    success(&output);
    let findings: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["code"], "unresolved-reference");
    assert_eq!(findings[0]["level"], "warning");
    assert!(
        findings[0]["message"]
            .as_str()
            .unwrap()
            .contains("configure a bibliography")
    );
    assert!(!root.join("out").exists());
}

#[cfg(unix)]
#[test]
fn xml_style_alias_cannot_be_overwritten_through_a_symlink() {
    let directory = project("Read @doe2020.\n", "local.xml");
    let root = directory.path();
    write(root, "local.xml", CSL);
    let alias = root.join("style-alias.html");
    std::os::unix::fs::symlink(root.join("local.xml"), &alias).unwrap();
    let mut options = options(root);
    options.cli.out = Some(alias);
    let report = pipeline::build_targets(
        &options,
        &[Target::Html],
        &mut WorldSlot::new(),
    );
    assert!(report.targets[0].written.is_err());
    assert_eq!(fs::read(root.join("local.xml")).unwrap(), CSL);
}
