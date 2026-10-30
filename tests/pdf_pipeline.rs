//! Generated PDF pipeline contracts; no external compiler or browser is needed.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use typst::foundations::Datetime;
use writer::config::Target;
use writer::error::Level;
use writer::ir::{Attrs, Block, Inline, SourceSpan};
use writer::pipeline::{self, BuildOptions, Prepared, WorldSlot, pdf};
use writer::typst::compile::{PdfOptions, compile_document, compile_pdf};

const CONFIG: &str = "[build]\nout_dir='out'\n[pdf]\nsystem_fonts=false\ntimestamp='2026-10-24T12:30:00Z'\n";
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#537da5"/></svg>"##;

fn project(text: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("document.md"), text).unwrap();
    fs::write(dir.path().join("writer.toml"), CONFIG).unwrap();
    dir
}

fn options(root: &Path) -> BuildOptions {
    BuildOptions::new(root.join("document.md"))
}

fn successful_pdf(report: &pipeline::BuildReport) -> Vec<u8> {
    assert!(report.prepared.is_ok(), "{:?}", report.prepared);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let target =
        report.targets.iter().find(|t| t.target == Target::Pdf).unwrap();
    let paths = target.written.as_ref().unwrap();
    assert_eq!(paths.len(), 1);
    let bytes = fs::read(&paths[0]).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(pdf::page_count(&bytes).is_some_and(|count| count > 0));
    bytes
}

fn build_pdf(root: &Path, worlds: &mut WorldSlot) -> pipeline::BuildReport {
    pipeline::build_targets(&options(root), &[Target::Pdf], worlds)
}

#[test]
fn markdown_pdf_is_real_and_matches_the_compiler_page_count() {
    let dir = project(
        "# First page\n\nHello.\n\n```{=typst}\n#pagebreak()\n```\n\n# Second page\n",
    );
    let opts = options(dir.path());
    let mut worlds = WorldSlot::new();
    let report = pipeline::build_targets(&opts, &[Target::Pdf], &mut worlds);
    let bytes = successful_pdf(&report);
    assert_eq!(pdf::page_count(&bytes), Some(2));
    let prepared = report.prepared.as_ref().unwrap();
    let world = worlds.get(Some(prepared), &opts).unwrap();
    world.reset();
    pipeline::load_typst_main(prepared, &opts, world).unwrap();
    let compiled = compile_pdf(
        world,
        &PdfOptions {
            timestamp: Datetime::from_ymd_hms(2026, 10, 24, 12, 30, 0).unwrap(),
            ident: None,
        },
    );
    assert!(!compiled.diagnostics.has_errors(), "{:?}", compiled.diagnostics);
    assert_eq!(pdf::page_count(&bytes), Some(compiled.output.unwrap().pages));
}

#[test]
fn fixed_timestamp_is_byte_identical_across_fresh_and_retained_worlds() {
    let dir = project("# Stable PDF\n\nEmbedded font text with $x^2$.\n");
    let mut retained = WorldSlot::new();
    let first = successful_pdf(&build_pdf(dir.path(), &mut retained));
    let second = successful_pdf(&build_pdf(dir.path(), &mut retained));
    let third = successful_pdf(&build_pdf(dir.path(), &mut WorldSlot::new()));
    assert_eq!(first, second);
    assert_eq!(first, third);
    let text = String::from_utf8_lossy(&first);
    assert!(
        text.contains("2026-10-24T12:30:00")
            || text.contains("D:20261024123000")
    );
}

#[test]
fn html_and_pdf_targets_succeed_independently_in_one_world() {
    let dir = project("# Shared build\n\nText and $a+b$.\n");
    let report = pipeline::build_targets(
        &options(dir.path()),
        &[Target::Html, Target::Pdf],
        &mut WorldSlot::new(),
    );
    successful_pdf(&report);
    assert!(report.targets.iter().all(|target| target.written.is_ok()));
    assert!(
        fs::read_to_string(dir.path().join("out/document.html"))
            .unwrap()
            .contains("Shared build")
    );
}

#[test]
fn compiler_failure_keeps_html_and_every_diagnostic_without_writing_pdf() {
    let dir = project(
        "# HTML survives\n\n```{=typst}\n#panic(\"pdf target failed\")\n```\n",
    );
    let report = pipeline::build_targets(
        &options(dir.path()),
        &[Target::Html, Target::Pdf],
        &mut WorldSlot::new(),
    );
    assert!(report.prepared.is_ok());
    assert!(report.targets[0].written.is_ok());
    assert!(report.targets[1].written.is_err());
    assert!(dir.path().join("out/document.html").is_file());
    assert!(!dir.path().join("out/document.pdf").exists());
    let failures = report
        .diagnostics
        .iter()
        .filter(|d| {
            d.level == Level::Error && d.message.contains("pdf target failed")
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1, "{:?}", report.diagnostics);
    let error = report.targets[1].written.as_ref().unwrap_err().to_string();
    assert!(error.contains("document.md"), "{error}");
    assert!(!error.contains("__writer-main.typ"), "{error}");
    assert_eq!(failures[0].span.as_ref().unwrap().start_line, 3);
}

#[test]
fn failed_compile_preserves_an_existing_output_and_creates_no_new_parents() {
    let dir = project("# Initially valid\n");
    let mut worlds = WorldSlot::new();
    let original = successful_pdf(&build_pdf(dir.path(), &mut worlds));
    fs::write(
        dir.path().join("document.md"),
        "```{=typst}\n#panic(\"stop before write\")\n```\n",
    )
    .unwrap();
    let report = build_pdf(dir.path(), &mut worlds);
    assert!(report.targets[0].written.is_err());
    assert_eq!(
        fs::read(dir.path().join("out/document.pdf")).unwrap(),
        original
    );
    let mut opts = options(dir.path());
    opts.cli.out = Some(dir.path().join("absent/deep/output.pdf"));
    let report = pipeline::build_targets(&opts, &[Target::Pdf], &mut worlds);
    assert!(report.targets[0].written.is_err());
    assert!(!dir.path().join("absent").exists());
}

#[test]
fn pdf_output_cannot_replace_the_input_or_an_input_asset() {
    let dir = project("# Protected input\n");
    let input = dir.path().join("document.md");
    let original = fs::read(&input).unwrap();
    let mut opts = options(dir.path());
    opts.cli.out = Some(input.clone());
    let report =
        pipeline::build_targets(&opts, &[Target::Pdf], &mut WorldSlot::new());
    assert!(report.targets[0].written.is_err());
    assert_eq!(fs::read(&input).unwrap(), original);
    let image = dir.path().join("plot.svg");
    fs::write(&image, SVG).unwrap();
    fs::write(&input, "![Plot](plot.svg)\n").unwrap();
    opts.cli.out = Some(image.clone());
    let report =
        pipeline::build_targets(&opts, &[Target::Pdf], &mut WorldSlot::new());
    assert!(report.targets[0].written.is_err());
    assert_eq!(fs::read(image).unwrap(), SVG);
}

#[test]
fn notebook_saved_text_and_byte_image_outputs_build_a_pdf() {
    let dir = project("Unused Markdown\n");
    let notebook = serde_json::json!({
        "nbformat": 4, "nbformat_minor": 5, "metadata": {}, "cells": [
            {"cell_type":"markdown", "metadata":{}, "source":["# Notebook PDF\n"]},
            {"cell_type":"code", "metadata":{}, "execution_count":1,
             "source":["print('saved result')"], "outputs":[
                {"output_type":"stream", "name":"stdout", "text":["saved result\n"]},
                {"output_type":"display_data", "metadata":{}, "data":{
                    "image/svg+xml":[String::from_utf8(SVG.to_vec()).unwrap()]
                }}
             ]}
        ]
    });
    let path = dir.path().join("notebook.ipynb");
    fs::write(&path, serde_json::to_vec(&notebook).unwrap()).unwrap();
    let mut opts = options(dir.path());
    opts.input = path;
    successful_pdf(&pipeline::build_targets(
        &opts,
        &[Target::Pdf],
        &mut WorldSlot::new(),
    ));
}

fn raw_prepared(root: &Path) -> (BuildOptions, Prepared) {
    let opts = options(root);
    let prepared = pipeline::prepare(&opts).unwrap();
    (opts, prepared)
}

#[test]
fn generated_assets_mount_file_and_byte_data_and_track_real_reads() {
    let dir = project("# Assets\n\n![Local](local.svg)\n");
    let local = dir.path().join("local.svg");
    fs::write(&local, SVG).unwrap();
    let (opts, mut prepared) = raw_prepared(dir.path());
    let id = prepared.assets.register_bytes(
        SVG.to_vec(),
        "image/svg+xml",
        "bytes.svg",
    );
    prepared.document.blocks.push(Block::paragraph(vec![Inline::Image {
        alt: vec![Inline::text("Bytes")],
        url: format!("asset:{id}"),
        title: None,
        attrs: Attrs::default(),
    }]));
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    world.reset();
    let (written, diagnostics) =
        pdf::build(&prepared, &opts, &dir.path().join("mounted.pdf"), world)
            .unwrap();
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    assert!(fs::read(&written[0]).unwrap().starts_with(b"%PDF-"));
    assert!(world.accessed().contains(&fs::canonicalize(local).unwrap()));
}

#[test]
fn custom_template_resolves_relative_imports_and_retained_world_replacement() {
    let dir = project("# Template mounting\n");
    for name in ["first", "second"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        fs::write(
            root.join("helper.typ"),
            format!("#let marker = \"{name}\"\n"),
        )
        .unwrap();
        fs::write(root.join("template.typ"), "#import \"helper.typ\": marker\n#import \"@writer/doc:0.1.0\": writer-doc as base\n#let writer-doc(..args) = base(..args)\n").unwrap();
    }
    let mut worlds = WorldSlot::new();
    for name in ["first", "second"] {
        let config = format!("{CONFIG}template='{name}/template.typ'\n");
        fs::write(dir.path().join("writer.toml"), config).unwrap();
        successful_pdf(&build_pdf(dir.path(), &mut worlds));
        let (opts, prepared) = raw_prepared(dir.path());
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        assert!(
            world.accessed().contains(
                &fs::canonicalize(dir.path().join(name).join("helper.typ"))
                    .unwrap()
            )
        );
    }
    fs::write(dir.path().join("writer.toml"), CONFIG).unwrap();
    successful_pdf(&build_pdf(dir.path(), &mut worlds));
    let (opts, prepared) = raw_prepared(dir.path());
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    assert!(!world.accessed().iter().any(|path| path.ends_with("helper.typ")));
}

// Both structural and native raw citations exercise the real resource mounts.
fn bibliography_pdf(
    root: &Path, worlds: &mut WorldSlot,
) -> (BuildOptions, Prepared) {
    let (opts, prepared) = raw_prepared(root);
    assert!(!prepared.diagnostics.has_errors(), "{:?}", prepared.diagnostics);
    assert!(prepared.document.features.citations);
    assert!(
        prepared
            .document
            .resolved
            .bibliography
            .as_ref()
            .is_some_and(|bibliography| !bibliography.entries.is_empty())
    );
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    world.reset();
    let output = root.join("out/document.pdf");
    let (_, diagnostics) =
        pdf::build(&prepared, &opts, &output, world).unwrap();
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    let bytes = fs::read(output).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(pdf::page_count(&bytes).is_some_and(|count| count > 0));
    (opts, prepared)
}

#[test]
fn bibliography_mounts_keep_identical_basenames_separate() {
    let dir = project(
        "# References\n\n[@alpha; @beta].\n\n```{=typst}\n#cite(<alpha>) #cite(<beta>)\n```\n",
    );
    for (name, key) in [("first", "alpha"), ("second", "beta")] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        fs::write(root.join("refs.bib"), format!("@book{{{key}, author={{Writer, A.}}, title={{{name}}}, year={{2026}}}}\n")).unwrap();
    }
    fs::write(dir.path().join("writer.toml"), format!("{CONFIG}[references]\nbibliography=['first/refs.bib','second/refs.bib']\nstyle='ieee'\n")).unwrap();
    let mut worlds = WorldSlot::new();
    let (opts, prepared) = bibliography_pdf(dir.path(), &mut worlds);
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    for name in ["first", "second"] {
        assert!(world.accessed().contains(
            &fs::canonicalize(dir.path().join(name).join("refs.bib")).unwrap()
        ));
    }
    let (source, diagnostics) = pdf::dump(&prepared, &opts).unwrap();
    assert!(!diagnostics.has_errors());
    assert!(source.contains("/__writer/bib/0/refs.bib"));
    assert!(source.contains("/__writer/bib/1/refs.bib"));
}

#[test]
fn compiler_diagnostics_preserve_include_and_notebook_provenance() {
    let dir = project("# Mapped failure\n");
    let included = dir.path().join("chapter.md");
    fs::write(&included, "source provenance\n").unwrap();
    let (opts, mut prepared) = raw_prepared(dir.path());
    prepared.document.blocks.push(Block::Raw {
        format: "typst".into(),
        text: "#panic(\"mapped cell failure\")".into(),
        span: Some(SourceSpan {
            start_line: 7,
            end_line: 8,
            cell: Some(4),
            file: Some(included.clone()),
        }),
    });
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    world.reset();
    pipeline::load_typst_main(&prepared, &opts, world).unwrap();
    let compiled = compile_document(world);
    assert!(compiled.output.is_none());
    let diagnostic = compiled
        .diagnostics
        .iter()
        .find(|d| d.message.contains("mapped cell failure"))
        .unwrap();
    let span = diagnostic.span.as_ref().unwrap();
    assert_eq!(span.start_line, 7);
    assert_eq!(span.cell, Some(4));
    assert_eq!(span.file.as_ref(), Some(&included));
    assert!(!dir.path().join("out").exists());
}

#[test]
fn dump_does_not_compile_or_write_when_raw_typst_would_fail() {
    let dir = project("```{=typst}\n#panic(\"dump must not compile\")\n```\n");
    let (opts, prepared) = raw_prepared(dir.path());
    let (first, _) = pdf::dump(&prepared, &opts).unwrap();
    let (second, _) = pdf::dump(&prepared, &opts).unwrap();
    assert_eq!(first, second);
    assert!(first.contains("dump must not compile"));
    assert!(!dir.path().join("out").exists());
}

#[test]
fn pdf_page_count_rejects_unsupported_or_malformed_bytes() {
    for bytes in [
        b"".as_slice(),
        b"plain text",
        b"%PDF-1.7\n/Page /Page",
        b"<xmpTPg:NPages>99999999999999999999999</xmpTPg:NPages>",
    ] {
        assert_eq!(pdf::page_count(bytes), None);
    }
}

#[test]
fn cli_pdf_summary_reports_pages_size_and_timestamp_policy() {
    let dir = project("# CLI PDF\n");
    fs::write(dir.path().join("writer.toml"), "[pdf]\nsystem_fonts=false\n")
        .unwrap();
    let out = dir.path().join("published.pdf");
    let output = Command::new(env!("CARGO_BIN_EXE_writer"))
        .current_dir(dir.path())
        .env("SOURCE_DATE_EPOCH", "0")
        .env_remove("RUST_LOG")
        .env_remove("CLICOLOR_FORCE")
        .args(["build", "document.md", "--to", "pdf", "-o"])
        .arg(&out)
        .assert()
        .success()
        .get_output()
        .clone();
    let bytes = fs::read(out).unwrap();
    assert_eq!(pdf::page_count(&bytes), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("1 page"), "{stdout}");
    assert!(stdout.contains("Wrote ") && stdout.contains("B)"), "{stdout}");
    let pdf = String::from_utf8_lossy(&bytes);
    assert!(
        pdf.contains("1970-01-01T00:00:00") || pdf.contains("D:19700101000000")
    );
}

const CSL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text">
  <info><title>Writer test style</title><id>https://example.org/writer-test</id>
    <updated>2026-10-24T00:00:00+00:00</updated></info>
  <citation><layout prefix="(" suffix=")"><text variable="citation-number"/></layout></citation>
  <bibliography><layout><text variable="title"/></layout></bibliography>
</style>"#;

#[test]
fn csl_path_mount_is_distinct_from_a_named_style_and_is_watched() {
    // Keep the native raw call while resolving a real source Cite as well.
    let dir = project(
        "# CSL resource\n\n@alpha.\n\n```{=typst}\n#cite(<alpha>)\n```\n",
    );
    fs::write(dir.path().join("refs.bib"), "@book{alpha, author={Writer, A.}, title={CSL resource}, year={2026}}\n").unwrap();
    fs::create_dir(dir.path().join("styles")).unwrap();
    let csl = dir.path().join("styles/local.csl");
    fs::write(&csl, CSL).unwrap();
    fs::write(dir.path().join("writer.toml"), format!("{CONFIG}[references]\nbibliography=['refs.bib']\nstyle='styles/local.csl'\n")).unwrap();
    let mut worlds = WorldSlot::new();
    let (opts, prepared) = bibliography_pdf(dir.path(), &mut worlds);
    let (source, _) = pdf::dump(&prepared, &opts).unwrap();
    assert!(source.contains("/__writer/csl/local.csl"));
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    assert!(world.accessed().contains(&fs::canonicalize(&csl).unwrap()));
    fs::write(
        dir.path().join("writer.toml"),
        format!(
            "{CONFIG}[references]\nbibliography=['refs.bib']\nstyle='ieee'\n"
        ),
    )
    .unwrap();
    let (opts, prepared) = bibliography_pdf(dir.path(), &mut worlds);
    let (source, _) = pdf::dump(&prepared, &opts).unwrap();
    assert!(!source.contains("/__writer/csl/"));
    assert!(source.contains("ieee"));
    assert!(
        !worlds
            .get(Some(&prepared), &opts)
            .unwrap()
            .accessed()
            .contains(&fs::canonicalize(csl).unwrap())
    );
}

#[test]
fn template_errors_keep_real_locations_and_scaffolding_errors_name_the_input() {
    let dir = project("# Failure locations\n");
    let template = dir.path().join("template.typ");
    fs::write(
        dir.path().join("writer.toml"),
        format!("{CONFIG}template='template.typ'\n"),
    )
    .unwrap();
    for (body, scaffolding) in [
        ("#panic(\"template failed\")\n", false),
        ("#let unrelated = 1\n", true),
    ] {
        fs::write(&template, body).unwrap();
        let report = build_pdf(dir.path(), &mut WorldSlot::new());
        let error = report.targets[0].written.as_ref().unwrap_err();
        let writer::error::Error::Compile { path, line, .. } = error else {
            panic!("expected mapped compiler failure: {error:?}");
        };
        if scaffolding {
            assert_eq!(path, &dir.path().join("document.md"));
            assert_eq!(*line, None);
        } else {
            assert_eq!(path, &fs::canonicalize(&template).unwrap());
            assert_eq!(*line, Some(1));
        }
        assert!(!dir.path().join("out").exists());
    }
}

#[test]
fn package_refusal_keeps_the_project_location_and_download_hint() {
    let dir = project(
        "```{=typst}\n#import \"@preview/writer-missing-test:0.0.0\": *\n```\n",
    );
    let report = build_pdf(dir.path(), &mut WorldSlot::new());
    let error = report.targets[0].written.as_ref().unwrap_err();
    let writer::error::Error::Compile { path, line, hints, .. } = error else {
        panic!("expected package compiler failure: {error:?}");
    };
    assert_eq!(path, &dir.path().join("document.md"));
    assert_eq!(*line, Some(1));
    assert!(
        hints.iter().any(|hint| hint.contains("allow_packages")),
        "{hints:?}"
    );
    assert!(!dir.path().join("out").exists());
}

#[test]
fn pdf_output_directory_is_refused_without_touching_its_contents() {
    let dir = project("# Directory output\n");
    let output = dir.path().join("output.pdf");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("keep"), "unchanged").unwrap();
    let mut opts = options(dir.path());
    opts.cli.out = Some(output.clone());
    let report =
        pipeline::build_targets(&opts, &[Target::Pdf], &mut WorldSlot::new());
    assert!(report.targets[0].written.is_err());
    assert_eq!(fs::read_to_string(output.join("keep")).unwrap(), "unchanged");
}

#[test]
fn a_fake_xmp_packet_in_the_pdf_title_cannot_override_the_page_count() {
    let marker = r#"<?xpacket begin="fake"?><xmpTPg:NPages>99</xmpTPg:NPages><?xpacket end="r"?>"#;
    let dir = project(&format!("---\ntitle: '{marker}'\n---\n\nOne page.\n"));
    let bytes = successful_pdf(&build_pdf(dir.path(), &mut WorldSlot::new()));
    assert!(bytes.windows(marker.len()).any(|part| part == marker.as_bytes()));
    assert_eq!(pdf::page_count(&bytes), Some(1));
}

#[test]
fn cli_mixed_build_prints_a_mapped_compiler_error_once_even_when_quiet() {
    let marker = "unique-cli-pdf-failure-marker";
    for quiet in [false, true] {
        let dir = project(&format!(
            "# HTML survives\n\n```{{=typst}}\n#panic(\"{marker}\")\n```\n"
        ));
        let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
        command
            .current_dir(dir.path())
            .env_remove("RUST_LOG")
            .env_remove("CLICOLOR_FORCE")
            .env("NO_COLOR", "1");
        if quiet {
            command.arg("-q");
        }
        let output = command
            .args(["build", "document.md", "--to", "html,pdf"])
            .assert()
            .code(1)
            .get_output()
            .clone();
        assert!(dir.path().join("out/document.html").is_file());
        assert!(!dir.path().join("out/document.pdf").exists());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(stderr.matches(marker).count(), 1, "{stderr}");
        let headlines: Vec<_> = stderr
            .lines()
            .filter(|line| line.contains(marker) && line.contains(": error:"))
            .collect();
        assert_eq!(headlines.len(), 1, "{stderr}");
        assert!(headlines[0].starts_with("document.md:3:"), "{stderr}");
        assert_eq!(stderr.matches("3 | ```{=typst}").count(), 1, "{stderr}");
        assert!(!stderr.contains("/__writer-main.typ"), "{stderr}");
    }
}
