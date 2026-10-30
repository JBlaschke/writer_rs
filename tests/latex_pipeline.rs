//! Public LaTeX rendering, physical sidecars, CLI behavior and write safety.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use typst::World;
use typst::foundations::Datetime;
use writer::config::{Config, Target};
use writer::error::{Error, Level};
use writer::latex::{self, FileSource, PlannedFile};
use writer::pipeline::{self, BuildOptions, Prepared, WorldSlot};
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld};

const SETTINGS: &str = "[build]\nout_dir='out'\ntargets=['latex']\n[pdf]\nsystem_fonts=false\nallow_packages=false\ntimestamp='2026-10-29T00:00:00Z'\n";
const PNG: &[u8] = include_bytes!("fixtures/latex/images/plot.png");
const SVG: &[u8] = include_bytes!("fixtures/latex/images/diagram.svg");
const GIF: &[u8] = include_bytes!("fixtures/latex/images/anim.gif");
const BIB: &[u8] =
    b"@book{saved,author={Writer},title={Copied bibliography},year={2026}}\r\n";
const MINIMAL: &str = "\\documentclass{article}\\VAR{preamble}\\begin{document}\\VAR{body}\\end{document}\n";

fn write(root: &Path, name: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn project(source: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "document.md", source);
    write(root.path(), "writer.toml", SETTINGS);
    root
}

fn options(root: &Path) -> BuildOptions {
    let mut options = BuildOptions::new(root.join("document.md"));
    options.config_path = Some(root.join("writer.toml"));
    options
}

fn prepare(root: &Path) -> (BuildOptions, Prepared) {
    let options = options(root);
    let prepared = pipeline::prepare(&options).unwrap();
    assert!(!prepared.diagnostics.has_errors(), "{:?}", prepared.diagnostics);
    (options, prepared)
}

fn world(root: &Path) -> WriterWorld {
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.pdf.allow_packages = false;
    let mut world = WriterWorld::from_config(
        &config,
        root,
        MainFile::Virtual,
        Datetime::from_ymd(2026, 10, 29).unwrap(),
    )
    .unwrap();
    world.set_main_text("Retained main sentinel", SourceMap::default());
    world
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

fn inventory(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
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
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    files
}

fn plan(files: &[PlannedFile]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|file| {
            let source = match &file.source {
                FileSource::Asset { id, render } => {
                    format!("asset:{id}:{render}")
                }
                FileSource::Bib(path) => format!("bib:{}", path.display()),
            };
            (file.name.clone(), source)
        })
        .collect()
}

#[test]
fn pure_render_materialise_and_dump_preserve_inputs_and_write_nothing() {
    let root = project("# Pure source\n\n![Plot](plot.png)\n");
    write(root.path(), "plot.png", PNG);
    write(root.path(), "refs.bib", BIB);
    write(
        root.path(),
        "writer.toml",
        format!("{SETTINGS}[references]\nbibliography=['refs.bib']\n",),
    );
    let (options, prepared) = prepare(root.path());
    let original = prepared.clone();
    let before = inventory(root.path());
    let rendered = latex::render(
        &prepared.document, &prepared.assets, &prepared.config, "document",
    )
    .unwrap();
    assert!(!rendered.diagnostics.has_errors(), "{:?}", rendered.diagnostics);
    let planned = plan(&rendered.files);
    assert!(!latex::needs_world(&rendered.files));
    let bytes =
        latex::materialise(&rendered.files, &prepared.assets, None).unwrap();
    assert_eq!(bytes["plot.png"], PNG);
    assert_eq!(bytes["refs.bib"], BIB);
    assert_eq!(plan(&rendered.files), planned);
    let repeated = latex::render(
        &prepared.document, &prepared.assets, &prepared.config, "document",
    )
    .unwrap();
    assert_eq!(repeated.tex, rendered.tex);
    assert_eq!(plan(&repeated.files), planned);
    let (dump, diagnostics) =
        pipeline::latex::dump(&prepared, &options).unwrap();
    assert_eq!(dump, rendered.tex);
    assert!(!diagnostics.has_errors());
    assert_eq!(prepared.document, original.document);
    assert_eq!(prepared.assets, original.assets);
    assert_eq!(prepared.config, original.config);
    assert_eq!(inventory(root.path()), before);
}

#[test]
fn full_tour_sample_snapshot_preserves_the_current_t70_placeholders() {
    let root = project("Unused main.\n");
    let mut options = options(root.path());
    options.input = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/latex/tour.md");
    let prepared = pipeline::prepare(&options).unwrap();
    let errors: Vec<_> = prepared
        .diagnostics
        .iter()
        .filter(|finding| finding.level == Level::Error)
        .collect();
    assert_eq!(errors.len(), 8, "{:?}", prepared.diagnostics);
    assert!(errors.iter().all(|finding| {
        finding.code == "unresolved-reference"
            && ["doe2020", "lee2019", "knuth1984"]
                .iter()
                .any(|key| finding.message.contains(&format!("`<{key}>`")))
    }));
    let output = latex::render(
        &prepared.document, &prepared.assets, &prepared.config, "tour",
    )
    .unwrap();
    assert!(!output.diagnostics.has_errors(), "{:?}", output.diagnostics);
    assert!(output.tex.contains("{[}?doe2020; lee2019{]}"));
    assert_eq!(output.tex.matches("{[}?knuth1984{]}").count(), 6);
    insta::assert_snapshot!("tour_sample", output.tex);
    assert!(!root.path().join("out").exists());
}

#[test]
fn pure_pipeline_build_preserves_the_supplied_world_and_omits_empty_sidecars() {
    let root = project("# Plain source\n\nNo resources or compiler work.\n");
    let (options, prepared) = prepare(root.path());
    let mut world = world(root.path());
    let main = world.main();
    let original = world.source(main).unwrap().text().to_string();
    let accessed = world.accessed();
    let output = root.path().join("named/plain.tex");
    let (written, diagnostics) =
        pipeline::latex::build(&prepared, &options, &output, &mut world)
            .unwrap();
    assert!(!diagnostics.has_errors());
    assert_eq!(written, std::slice::from_ref(&output));
    assert!(fs::read_to_string(&output).unwrap().contains("Plain source"));
    assert!(!output.with_file_name("plain_assets").exists());
    assert_eq!(world.main(), main);
    assert_eq!(world.source(main).unwrap().text(), original);
    assert_eq!(world.accessed(), accessed);
}

#[test]
fn physical_renditions_and_bibliography_are_exact_complete_and_deterministic() {
    let root = project(
        "![Raster](plot.png)\n\n![Vector](diagram.svg)\n\n![Animation](anim.gif)\n",
    );
    for (name, bytes) in
        [("plot.png", PNG), ("diagram.svg", SVG), ("anim.gif", GIF)]
    {
        write(root.path(), name, bytes);
    }
    write(root.path(), "refs.bib", BIB);
    write(
        root.path(),
        "writer.toml",
        format!("{SETTINGS}[references]\nbibliography=['refs.bib']\n",),
    );
    let (options, prepared) = prepare(root.path());
    let mut world = world(root.path());
    let main = world.main();
    let output = root.path().join("out/document.tex");
    let (written, diagnostics) =
        pipeline::latex::build(&prepared, &options, &output, &mut world)
            .unwrap();
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    assert_eq!(
        written,
        [output.clone(), root.path().join("out/document_assets")]
    );
    let files = inventory(&root.path().join("out/document_assets"));
    assert_eq!(files.len(), 4);
    assert_eq!(files[Path::new("plot.png")], PNG);
    assert_eq!(files[Path::new("refs.bib")], BIB);
    for name in ["diagram.svg.pdf", "anim.gif.pdf"] {
        assert!(files[Path::new(name)].starts_with(b"%PDF-"));
        assert_eq!(pipeline::pdf::page_count(&files[Path::new(name)]), Some(1));
    }
    let tex = fs::read_to_string(&output).unwrap();
    for name in ["plot.png", "diagram.svg.pdf", "anim.gif.pdf"] {
        assert!(tex.contains(&format!("document_assets/{name}")));
    }
    assert_eq!(world.main(), main);
    assert_eq!(world.source(main).unwrap().text(), "Retained main sentinel");
    let before = inventory(&root.path().join("out"));
    pipeline::latex::build(&prepared, &options, &output, &mut world).unwrap();
    assert_eq!(inventory(&root.path().join("out")), before);
}

#[test]
fn dump_never_decodes_images_or_reads_bibliography_and_cli_writes_no_files() {
    let root = project("![Invalid vector](broken.svg)\n");
    write(root.path(), "broken.svg", b"not SVG bytes");
    write(
        root.path(),
        "writer.toml",
        format!("{SETTINGS}[references]\nbibliography=['missing/refs.bib']\n",),
    );
    let (options, prepared) = prepare(root.path());
    let before = inventory(root.path());
    let (dump, diagnostics) =
        pipeline::latex::dump(&prepared, &options).unwrap();
    assert!(!diagnostics.has_errors());
    assert!(dump.contains("document_assets/broken.svg.pdf"));
    let result = writer(root.path())
        .args(["dump", "document.md", "--latex"])
        .output()
        .unwrap();
    success(&result);
    assert_eq!(String::from_utf8(result.stdout).unwrap(), dump);
    assert_eq!(inventory(root.path()), before);
    assert!(!root.path().join("out").exists());
}

#[test]
fn renderer_and_copy_errors_preserve_last_good_outputs_and_new_parent_absence()
{
    let root = project("![Vector](diagram.svg)\n");
    write(root.path(), "diagram.svg", SVG);
    let (options, prepared) = prepare(root.path());
    let output = root.path().join("out/document.tex");
    let mut world = world(root.path());
    pipeline::latex::build(&prepared, &options, &output, &mut world).unwrap();
    write(root.path(), "out/document_assets/unrelated.bin", b"keep");
    let previous = inventory(&root.path().join("out"));
    write(root.path(), "diagram.svg", b"invalid SVG");
    let (_, invalid) = prepare(root.path());
    let error = pipeline::latex::build(&invalid, &options, &output, &mut world)
        .unwrap_err();
    assert!(matches!(error, Error::Resource { what: "image", .. }));
    assert_eq!(inventory(&root.path().join("out")), previous);
    let fresh = root.path().join("unwritten/deep/result.tex");
    assert!(
        pipeline::latex::build(&invalid, &options, &fresh, &mut world).is_err()
    );
    assert!(!root.path().join("unwritten").exists());
    write(root.path(), "document.md", "Plain source.\n");
    write(
        root.path(),
        "writer.toml",
        format!("{SETTINGS}[references]\nbibliography=['missing.bib']\n",),
    );
    let (options, missing) = prepare(root.path());
    assert!(matches!(
        pipeline::latex::build(&missing, &options, &fresh, &mut world),
        Err(Error::Resource { what: "bibliography", .. })
    ));
    assert!(!root.path().join("unwritten").exists());
    assert_eq!(inventory(&root.path().join("out")), previous);
}

#[test]
fn template_slot_and_emission_errors_refuse_writes_while_dump_keeps_useful_tex()
{
    let root = project("```text\nBefore.\n\\end{Verbatim}\nAfter.\n```\n");
    let (options, prepared) = prepare(root.path());
    let output = root.path().join("unwritten/deep/result.tex");
    let (dump, diagnostics) =
        pipeline::latex::dump(&prepared, &options).unwrap();
    assert!(dump.contains("\\begin{writercode}"));
    assert!(diagnostics.iter().any(|finding| {
        finding.code == "latex-verbatim-end" && finding.level == Level::Error
    }));
    assert!(
        pipeline::latex::build(
            &prepared,
            &options,
            &output,
            &mut world(root.path())
        )
        .is_err()
    );
    assert!(!root.path().join("unwritten").exists());
    let result = writer(root.path())
        .args(["dump", "document.md", "--latex"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(String::from_utf8(result.stdout).unwrap(), dump);
    write(root.path(), "document.md", "Plain source.\n");
    write(
        root.path(),
        "missing-slot.tex",
        "\\documentclass{article}\\begin{document}\\VAR{body}\\end{document}\n",
    );
    write(
        root.path(),
        "writer.toml",
        format!("{SETTINGS}[latex]\ntemplate='missing-slot.tex'\n",),
    );
    let (options, prepared) = prepare(root.path());
    let rendered = latex::render(
        &prepared.document, &prepared.assets, &prepared.config, "result",
    )
    .unwrap();
    assert!(rendered.tex.is_empty());
    assert!(rendered.diagnostics.iter().any(|finding| {
        finding.code == "latex-template-slot-missing"
            && finding.level == Level::Error
    }));
    assert!(
        pipeline::latex::build(
            &prepared,
            &options,
            &output,
            &mut world(root.path())
        )
        .is_err()
    );
    assert!(!root.path().join("unwritten").exists());
    let result = writer(root.path())
        .args(["build", "document.md", "--to", "latex", "-o"])
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert!(!root.path().join("unwritten").exists());
}

#[test]
fn source_include_asset_bibliography_and_template_inputs_cannot_be_overwritten()
{
    for kind in ["source", "include", "asset", "bib", "template"] {
        let root = project("Source.\n");
        let output = match kind {
            "source" => root.path().join("new/../document.md"),
            "include" => {
                write(root.path(), "document.md", ":::include child.md\n:::\n");
                write(root.path(), "child.md", "Included.\n");
                root.path().join("child.md")
            }
            "asset" => {
                write(
                    root.path(),
                    "document.md",
                    "![Plot](report_assets/plot.png)\n",
                );
                write(root.path(), "report_assets/plot.png", PNG);
                root.path().join("report.tex")
            }
            "bib" => {
                write(root.path(), "report_assets/refs.bib", BIB);
                write(
                    root.path(),
                    "writer.toml",
                    format!(
                        "{SETTINGS}[references]\nbibliography=['report_assets/refs.bib']\n",
                    ),
                );
                root.path().join("report.tex")
            }
            "template" => {
                write(root.path(), "wrapper.tex", MINIMAL);
                write(
                    root.path(),
                    "writer.toml",
                    format!("{SETTINGS}[latex]\ntemplate='wrapper.tex'\n",),
                );
                root.path().join("wrapper.tex")
            }
            _ => unreachable!(),
        };
        let (options, prepared) = prepare(root.path());
        let previous = inventory(root.path());
        let error = pipeline::latex::build(
            &prepared,
            &options,
            &output,
            &mut world(root.path()),
        )
        .unwrap_err();
        assert!(pipeline::refused_clash(&error).is_some(), "{kind}: {error}");
        assert_eq!(inventory(root.path()), previous, "{kind}");
        assert!(!root.path().join("new").exists());
    }
}

#[cfg(unix)]
#[test]
fn output_symlink_and_hardlink_aliases_preserve_inputs_and_shared_output_aliases()
 {
    let root = project("Source.\n");
    let (options, prepared) = prepare(root.path());
    for (name, hardlink) in [("symbolic.tex", false), ("hard.tex", true)] {
        let output = root.path().join(name);
        if hardlink {
            fs::hard_link(root.path().join("document.md"), &output).unwrap();
        } else {
            std::os::unix::fs::symlink(
                root.path().join("document.md"),
                &output,
            )
            .unwrap();
        }
        let error = pipeline::latex::build(
            &prepared,
            &options,
            &output,
            &mut world(root.path()),
        )
        .unwrap_err();
        assert!(pipeline::refused_clash(&error).is_some());
        assert_eq!(
            fs::read_to_string(root.path().join("document.md")).unwrap(),
            "Source.\n"
        );
    }
    write(root.path(), "document.md", "![Plot](figures/result.png)\n");
    write(root.path(), "figures/result.png", PNG);
    std::os::unix::fs::symlink(root.path(), root.path().join("result_assets"))
        .unwrap();
    let (options, prepared) = prepare(root.path());
    let output = root.path().join("result.png");
    assert!(
        pipeline::latex::build(
            &prepared,
            &options,
            &output,
            &mut world(root.path())
        )
        .is_err()
    );
    assert!(!output.exists());
    assert_eq!(fs::read(root.path().join("figures/result.png")).unwrap(), PNG);
}

#[test]
fn cli_defaults_named_dotted_outputs_quiet_and_dump_use_the_same_source_bytes()
{
    let root = project("![Raster](plot.png)\n\n![Vector](diagram.svg)\n");
    write(root.path(), "plot.png", PNG);
    write(root.path(), "diagram.svg", SVG);
    let result =
        writer(root.path()).args(["build", "document.md"]).output().unwrap();
    success(&result);
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 2, "{stdout}");
    assert!(stdout.lines().all(|line| line.starts_with("Wrote ")));
    assert!(
        stdout.contains("document.tex") && stdout.contains("document_assets")
    );
    assert!(stdout.contains("2 files"));
    let first = inventory(&root.path().join("out"));
    let result = writer(root.path())
        .args(["-q", "build", "document.md"])
        .output()
        .unwrap();
    success(&result);
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
    assert_eq!(inventory(&root.path().join("out")), first);
    let dumped = writer(root.path())
        .args(["dump", "document.md", "--latex"])
        .output()
        .unwrap();
    success(&dumped);
    assert_eq!(
        dumped.stdout,
        fs::read(root.path().join("out/document.tex")).unwrap()
    );
    let named = root.path().join("named/paper.final.tex");
    let result = writer(root.path())
        .args(["build", "document.md", "--to", "latex,latex", "-o"])
        .arg(&named)
        .output()
        .unwrap();
    success(&result);
    assert!(named.is_file());
    assert!(
        named.parent().unwrap().join("paper.final_assets/plot.png").is_file()
    );
    let tex = fs::read_to_string(&named).unwrap();
    assert!(tex.contains("paper.final_assets/diagram.svg.pdf"));
    assert!(!tex.contains("document_assets"));
}

#[test]
fn cli_template_override_and_configuration_use_the_dedicated_latex_template() {
    let root = project("Body marker.\n");
    write(root.path(), "configured.tex", format!("CONFIGURED\n{MINIMAL}"));
    write(root.path(), "override.tex", format!("OVERRIDE\n{MINIMAL}"));
    write(
        root.path(),
        "writer.toml",
        format!("{SETTINGS}[latex]\ntemplate='configured.tex'\n",),
    );
    let result =
        writer(root.path()).args(["build", "document.md"]).output().unwrap();
    success(&result);
    assert!(
        fs::read_to_string(root.path().join("out/document.tex"))
            .unwrap()
            .starts_with("CONFIGURED")
    );
    let result = writer(root.path())
        .args(["build", "document.md", "--latex-template", "override.tex"])
        .output()
        .unwrap();
    success(&result);
    assert!(
        fs::read_to_string(root.path().join("out/document.tex"))
            .unwrap()
            .starts_with("OVERRIDE")
    );
}

#[test]
fn cli_dump_preserves_custom_templates_without_a_final_line_feed() {
    let root = project("Body marker.\n");
    write(root.path(), "without-lf.tex", MINIMAL.strip_suffix('\n').unwrap());
    write(
        root.path(),
        "writer.toml",
        format!("{SETTINGS}[latex]\ntemplate='without-lf.tex'\n",),
    );
    let built =
        writer(root.path()).args(["build", "document.md"]).output().unwrap();
    success(&built);
    let bytes = fs::read(root.path().join("out/document.tex")).unwrap();
    assert!(!bytes.ends_with(b"\n"));
    let dumped = writer(root.path())
        .args(["dump", "document.md", "--latex"])
        .output()
        .unwrap();
    success(&dumped);
    assert_eq!(dumped.stdout, bytes);
}

#[test]
fn yaml_bibliography_and_malformed_templates_fail_before_new_output_parents() {
    let root = project("Plain content.\n");
    write(
        root.path(),
        "refs.YAML",
        "saved:\n  type: Book\n  title: Saved title\n",
    );
    write(
        root.path(),
        "writer.toml",
        format!("{SETTINGS}[references]\nbibliography=['refs.YAML']\n",),
    );
    let (options, prepared) = prepare(root.path());
    let rendered = latex::render(
        &prepared.document, &prepared.assets, &prepared.config, "result",
    )
    .unwrap();
    assert!(!rendered.tex.is_empty());
    assert!(rendered.diagnostics.iter().any(|finding| {
        finding.code == "latex-bib-yaml" && finding.level == Level::Error
    }));
    assert!(rendered.files.is_empty());
    let output = root.path().join("unwritten/deep/result.tex");
    assert!(
        pipeline::latex::build(
            &prepared,
            &options,
            &output,
            &mut world(root.path())
        )
        .is_err()
    );
    assert!(!root.path().join("unwritten").exists());
    write(root.path(), "broken.tex", format!("{MINIMAL}\\VAR{{"));
    write(
        root.path(),
        "writer.toml",
        format!("{SETTINGS}[latex]\ntemplate='broken.tex'\n",),
    );
    let (options, prepared) = prepare(root.path());
    let error = pipeline::latex::dump(&prepared, &options).unwrap_err();
    assert!(matches!(error, Error::Template { .. }), "{error:?}");
    assert!(
        pipeline::latex::build(
            &prepared,
            &options,
            &output,
            &mut world(root.path())
        )
        .is_err()
    );
    assert!(!root.path().join("unwritten").exists());
}

#[test]
fn mixed_exports_share_one_directory_and_common_names_always_hold_the_same_bytes()
 {
    let mut previous = None;
    for targets in
        [[Target::Latex, Target::Typst], [Target::Typst, Target::Latex]]
    {
        let root = project("![Plot](plot.png)\n\n![Vector](diagram.svg)\n");
        write(root.path(), "plot.png", PNG);
        write(root.path(), "diagram.svg", SVG);
        write(root.path(), "refs.bib", BIB);
        write(
            root.path(),
            "writer.toml",
            format!("{SETTINGS}[references]\nbibliography=['refs.bib']\n",),
        );
        let report = pipeline::build_targets(
            &options(root.path()),
            &targets,
            &mut WorldSlot::new(),
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        assert!(
            report.targets.iter().all(|result| result.written.is_ok()),
            "{:?}",
            report.targets
        );
        let shared = inventory(&root.path().join("out/document_assets"));
        assert_eq!(shared.len(), 4);
        assert_eq!(shared[Path::new("plot.png")], PNG);
        assert_eq!(shared[Path::new("diagram.svg")], SVG);
        assert_eq!(shared[Path::new("refs.bib")], BIB);
        assert!(shared[Path::new("diagram.svg.pdf")].starts_with(b"%PDF-"));
        for target in ["document.tex", "document.typ"] {
            assert!(root.path().join("out").join(target).is_file());
        }
        if let Some(previous) = &previous {
            assert_eq!(&shared, previous);
        }
        previous = Some(shared);
    }
}

#[test]
fn mixed_target_failures_leave_other_targets_successful_in_both_orders() {
    for targets in ["latex,html,typst", "typst,html,latex"] {
        let root = project("```text\n\\end{Verbatim}\n```\n");
        let output = root.path().join("mixed");
        let result = writer(root.path())
            .args(["build", "document.md", "--to", targets, "-o"])
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(output.join("document.html").is_file());
        assert!(output.join("document.typ").is_file());
        assert!(!output.join("document.tex").exists());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("\\end{Verbatim}")
        );
    }
}

#[test]
fn native_pdf_authority_remains_independent_of_the_latex_subset_gate() {
    for targets in [[Target::Latex, Target::Pdf], [Target::Pdf, Target::Latex]]
    {
        let root = project("unused Markdown main.\n");
        write(
            root.path(),
            "native.typ",
            "#for item in range(2) [Native #item ]\n",
        );
        let mut options = options(root.path());
        options.input = root.path().join("native.typ");
        let report =
            pipeline::build_targets(&options, &targets, &mut WorldSlot::new());
        assert!(report.prepared.is_ok());
        for result in &report.targets {
            assert_eq!(
                result.written.is_ok(),
                result.target == Target::Pdf,
                "{result:?}"
            );
        }
        assert!(report.diagnostics.iter().any(|finding| {
            finding.code.starts_with("E-TYP-") && finding.level == Level::Error
        }));
        let pdf = fs::read(root.path().join("out/native.pdf")).unwrap();
        assert_eq!(pipeline::pdf::page_count(&pdf), Some(1));
        assert!(!root.path().join("out/native.tex").exists());
    }
}
