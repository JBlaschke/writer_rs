//! Real PDF math corpus, per-equation recovery and prepared-source round trips.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use typst::foundations::Datetime;
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;
use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::error::{Error, Level};
use writer::ir::{
    self, Attrs, Block, Document, Inline, MathDialect, SourceInfo, SourceKind,
    SourceSpan,
};
use writer::math::latex_source::{for_typst, macros_from_config, prepare};
use writer::pipeline::{self, BuildOptions, Prepared, WorldSlot, pdf};
use writer::resolve::{ResolveOptions, resolve};
use writer::typst::compile::{PdfOptions, compile_document, compile_pdf};
use writer::typst::emit::{AssetPaths, EmitOptions, Emitted, emit};
use writer::typst::frontend;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld};

const TIMESTAMP: &str = "2026-10-24T12:30:00Z";

struct Fixture {
    name: String,
    path: PathBuf,
    body: String,
    display: bool,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/math")
}

fn fixtures() -> Vec<Fixture> {
    let mut paths = fs::read_dir(root().join("latex"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "tex"))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), 37);
    paths
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).unwrap();
            let (mode, body) = text.split_once('\n').unwrap();
            let display = match mode {
                "% mode: inline" => false,
                "% mode: display" => true,
                _ => panic!("{}: invalid mode {mode}", path.display()),
            };
            let body = body.strip_suffix('\n').unwrap_or(body);
            let body = body
                .strip_prefix('$')
                .and_then(|s| s.strip_suffix('$'))
                .unwrap_or(body);
            Fixture {
                name: path.file_stem().unwrap().to_str().unwrap().into(),
                path,
                body: body.into(),
                display,
            }
        })
        .collect()
}

fn expectations() -> toml::Table {
    fs::read_to_string(root().join("expectations.toml"))
        .unwrap()
        .parse()
        .unwrap()
}

fn config(fixture: Option<&Fixture>) -> Config {
    let mut config =
        if fixture.is_some_and(|fixture| fixture.name.starts_with("31-")) {
            Config::load(&root().join("macros.toml")).unwrap()
        } else {
            Config::default()
        };
    config.pdf.system_fonts = false;
    config.pdf.allow_packages = false;
    config.pdf.timestamp = TIMESTAMP.into();
    config
}

fn macros(config: &Config) -> BTreeMap<String, String> {
    macros_from_config(&config.math.katex_options)
}

fn span(fixture: &Fixture) -> SourceSpan {
    SourceSpan {
        start_line: 2,
        end_line: 2,
        cell: None,
        file: Some(fixture.path.clone()),
    }
}

fn equation(fixture: &Fixture, display: bool, source: &str) -> Block {
    if display {
        Block::Math {
            source: source.into(),
            dialect: MathDialect::Latex,
            attrs: Attrs::default(),
            number: None,
            span: Some(span(fixture)),
        }
    } else {
        Block::Paragraph {
            inlines: vec![Inline::Math {
                source: source.into(),
                dialect: MathDialect::Latex,
            }],
            span: Some(span(fixture)),
        }
    }
}

fn document(
    directory: &Path, config: Config, blocks: Vec<Block>,
) -> (BuildOptions, Prepared) {
    let input = directory.join("document.md");
    fs::write(&input, "The corpus is supplied as resolved IR by this test.\n")
        .unwrap();
    let mut document = Document {
        meta: Default::default(),
        blocks,
        features: Default::default(),
        source: SourceInfo {
            path: input.clone(),
            base_dir: directory.into(),
            kind: SourceKind::Markdown,
        },
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Default::default(),
    };
    let diagnostics = resolve(
        &mut document,
        &ResolveOptions { config: &config, library: None, lenient: true },
    );
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    (
        BuildOptions::new(input),
        Prepared {
            document,
            assets: AssetRegistry::new(),
            config,
            config_path: None,
            diagnostics,
        },
    )
}

fn emitted(prepared: &Prepared) -> Emitted {
    emit(
        &prepared.document,
        &prepared.assets,
        &EmitOptions {
            config: &prepared.config,
            assets: AssetPaths::Virtual,
            template: None,
            bibliography: Vec::new(),
            style: "ieee".into(),
            footnote_prefix: String::new(),
            fragment: false,
        },
    )
    .unwrap()
}

fn pdf_options() -> PdfOptions {
    PdfOptions {
        timestamp: Datetime::from_ymd_hms(2026, 10, 24, 12, 30, 0).unwrap(),
        ident: None,
    }
}

fn load<'a>(
    worlds: &'a mut WorldSlot, opts: &BuildOptions, prepared: &Prepared,
) -> &'a mut WriterWorld {
    let world = worlds.get(Some(prepared), opts).unwrap();
    world.reset();
    pipeline::load_typst_main(prepared, opts, world).unwrap();
    world
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

fn visible(document: &PagedDocument) -> String {
    let mut text = String::new();
    for page in document.pages() {
        frame_text(&page.frame, &mut text);
    }
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn math_nodes(document: &Document) -> Vec<(bool, MathDialect, String)> {
    let mut nodes = Vec::new();
    ir::walk_blocks(document, &mut |block| {
        if let Block::Math { source, dialect, .. } = block {
            nodes.push((true, *dialect, source.clone()));
        }
    });
    ir::walk_inlines(document, &mut |inline| {
        if let Inline::Math { source, dialect } = inline {
            nodes.push((false, *dialect, source.clone()));
        }
    });
    nodes
}

fn string_values(node: &typst_syntax::SyntaxNode, values: &mut Vec<String>) {
    if let Some(value) = node.cast::<typst_syntax::ast::Str>() {
        values.push(value.get().to_string());
    }
    for child in node.children() {
        string_values(child, values);
    }
}

fn parse_generated(
    directory: &Path, text: &str, config: &Config,
) -> ir::Parsed {
    let parsed = frontend::parse(
        &SourceInfo {
            path: directory.join("generated.typ"),
            base_dir: directory.into(),
            kind: SourceKind::Typst,
        },
        text,
        config,
    )
    .unwrap();
    assert!(
        !parsed.diagnostics.has_errors(),
        "{text}\n{:?}",
        parsed.diagnostics
    );
    parsed
}

#[test]
fn all_37_latex_fixtures_have_the_exact_reviewed_pdf_classification() {
    let table = expectations();
    let known_errors =
        BTreeSet::from(["18-unknown-command", "32-ce", "37-sideset"]);
    let mut compile = 0;
    let mut failures = BTreeSet::new();
    for fixture in fixtures() {
        let expected = table["latex"][&fixture.name]["pdf"].as_str().unwrap();
        let (source, _) =
            for_typst(&fixture.body, &macros(&config(Some(&fixture))));
        match mitex::convert_math(&source, None) {
            Ok(_) => {
                assert_eq!(expected, "compile", "{}", fixture.name);
                compile += 1;
            }
            Err(message) => {
                assert_eq!(
                    expected, "preflight-error",
                    "{}: {message}",
                    fixture.name
                );
                failures.insert(fixture.name);
            }
        }
    }
    assert_eq!(compile, 34);
    let expected =
        known_errors.into_iter().map(str::to_owned).collect::<BTreeSet<_>>();
    assert_eq!(failures, expected);
}

#[test]
fn all_34_compile_entries_share_one_real_deterministic_pdf() {
    let directory = tempfile::tempdir().unwrap();
    let table = expectations();
    let blocks = fixtures()
        .into_iter()
        .filter(|fixture| {
            table["latex"][&fixture.name]["pdf"].as_str() == Some("compile")
        })
        .map(|fixture| {
            // Only31 needs configured macros. Materialize that equation's prefix
            // locally so it cannot collide with fixture17's independent half macro.
            let source = if fixture.name.starts_with("31-") {
                for_typst(&fixture.body, &macros(&config(Some(&fixture)))).0
            } else {
                fixture.body.clone()
            };
            equation(&fixture, fixture.display, &source)
        })
        .collect::<Vec<_>>();
    assert_eq!(blocks.len(), 34);
    let (opts, prepared) = document(directory.path(), config(None), blocks);
    let source = emitted(&prepared);
    assert!(!source.diagnostics.has_errors(), "{:?}", source.diagnostics);
    assert!(
        !source.diagnostics.iter().any(|d| d.code == "math-conversion-failed")
    );
    let mut worlds = WorldSlot::new();
    let first =
        compile_pdf(load(&mut worlds, &opts, &prepared), &pdf_options())
            .into_result()
            .unwrap()
            .0;
    let second =
        compile_pdf(load(&mut worlds, &opts, &prepared), &pdf_options())
            .into_result()
            .unwrap()
            .0;
    let third = compile_pdf(
        load(&mut WorldSlot::new(), &opts, &prepared),
        &pdf_options(),
    )
    .into_result()
    .unwrap()
    .0;
    assert!(first.bytes.starts_with(b"%PDF-"));
    assert!(first.pages > 0);
    assert_eq!(pdf::page_count(&first.bytes), Some(first.pages));
    assert_eq!(first, second);
    assert_eq!(first, third);
}

#[test]
fn each_preflight_rejection_keeps_source_labels_and_numbering_in_a_pdf() {
    let table = expectations();
    for fixture in fixtures().into_iter().filter(|fixture| {
        table["latex"][&fixture.name]["pdf"].as_str() == Some("preflight-error")
    }) {
        for display in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut block = equation(&fixture, display, &fixture.body);
            if let Block::Math { attrs, .. } = &mut block {
                attrs.id = Some("eq-fallback".into());
            }
            let (opts, prepared) =
                document(directory.path(), config(Some(&fixture)), vec![block]);
            let source = emitted(&prepared);
            let failures = source
                .diagnostics
                .iter()
                .filter(|d| d.code == "math-conversion-failed")
                .collect::<Vec<_>>();
            assert_eq!(
                failures.len(),
                1,
                "{}: {:?}",
                fixture.name,
                source.diagnostics
            );
            assert_eq!(failures[0].level, Level::Warning);
            assert_eq!(failures[0].span.as_ref(), Some(&span(&fixture)));
            assert!(source.text.contains("#raw("), "{}", source.text);
            let syntax = typst_syntax::Source::detached(&source.text);
            let mut strings = Vec::new();
            string_values(syntax.root(), &mut strings);
            assert!(
                strings.contains(&fixture.body),
                "{}: {strings:?}",
                fixture.name
            );
            assert!(!source.text.contains("#mi("));
            assert!(!source.text.contains("#mitex("));
            if display {
                assert!(source.text.contains("$ #raw("), "{}", source.text);
                assert!(source.text.contains(" <eq-fallback>"));
                assert!(
                    matches!(&prepared.document.blocks[0], Block::Math { number: Some(number), .. } if number == "1")
                );
            } else {
                assert!(source.text.contains("$#raw("), "{}", source.text);
            }
            let mut worlds = WorldSlot::new();
            let world = load(&mut worlds, &opts, &prepared);
            let compiled = compile_document(world).into_result().unwrap().0;
            let original: String =
                fixture.body.chars().filter(|c| !c.is_whitespace()).collect();
            assert!(
                visible(&compiled).contains(&original),
                "{}: {}",
                fixture.name,
                visible(&compiled)
            );
            if display {
                assert!(
                    visible(&compiled).contains("(1)"),
                    "{}",
                    visible(&compiled)
                );
            }
            let pdf =
                compile_pdf(world, &pdf_options()).into_result().unwrap().0;
            assert!(pdf.bytes.starts_with(b"%PDF-"));
            assert_eq!(pdf::page_count(&pdf.bytes), Some(pdf.pages));
            let reparsed = parse_generated(
                directory.path(),
                &source.text,
                &prepared.config,
            );
            let math = math_nodes(&reparsed.document);
            assert_eq!(math.len(), 1);
            assert_eq!(math[0].0, display);
            assert_eq!(math[0].1, MathDialect::Typst);
            assert!(math[0].2.contains("#raw("));
        }
    }
}

#[test]
fn the_plugin_rejects_the_same_bad_commands_when_preflight_is_bypassed() {
    let table = expectations();
    let directory = tempfile::tempdir().unwrap();
    let config = config(None);
    let mut world = WriterWorld::from_config(
        &config,
        directory.path(),
        MainFile::Virtual,
        Datetime::from_ymd_hms(2026, 10, 24, 12, 30, 0).unwrap(),
    )
    .unwrap();
    for fixture in fixtures().into_iter().filter(|fixture| {
        table["latex"][&fixture.name]["pdf"].as_str() == Some("preflight-error")
    }) {
        let source = for_typst(&fixture.body, &macros(&config)).0;
        let function = if fixture.display { "mitex" } else { "mi" };
        let text = format!(
            "#import \"@preview/mitex:0.2.7\": mi, mitex\n#{function}({})\n",
            serde_json::to_string(&source).unwrap()
        );
        world.reset();
        world.set_main_text(
            &text,
            SourceMap::from_lines(vec![None, Some(span(&fixture))]),
        );
        let compiled = compile_document(&mut world);
        assert!(compiled.output.is_none(), "{}", fixture.name);
        let error = compiled
            .diagnostics
            .iter()
            .find(|d| d.level == Level::Error)
            .unwrap();
        assert!(
            error.message.contains("unknown command"),
            "{}: {error:?}",
            fixture.name
        );
        assert_eq!(
            error.span.as_ref().and_then(|span| span.file.as_ref()),
            Some(&fixture.path)
        );
        assert!(matches!(
            compiled.into_result().unwrap_err(),
            Error::Compile { .. }
        ));
    }
}

#[test]
fn compile_entries_round_trip_as_prepared_latex_with_effective_macros() {
    let table = expectations();
    for fixture in fixtures().into_iter().filter(|fixture| {
        table["latex"][&fixture.name]["pdf"].as_str() == Some("compile")
    }) {
        let directory = tempfile::tempdir().unwrap();
        let (opts, prepared) = document(
            directory.path(),
            config(Some(&fixture)),
            vec![equation(&fixture, fixture.display, &fixture.body)],
        );
        let source = pdf::dump(&prepared, &opts).unwrap().0;
        let parsed =
            parse_generated(directory.path(), &source, &prepared.config);
        let math = math_nodes(&parsed.document);
        assert_eq!(
            math,
            vec![(
                fixture.display,
                MathDialect::Latex,
                prepare(&fixture.body).0
            )],
            "{}: {source}",
            fixture.name
        );
    }
}

#[test]
fn raw_and_escaped_string_literals_both_round_trip_without_byte_changes() {
    for body in
        [r"\frac{a}{b}", "\\text{a`b}", "\\text{first}\n+ \\text{second}"]
    {
        let directory = tempfile::tempdir().unwrap();
        let fixture = Fixture {
            name: "literal".into(),
            path: directory.path().join("literal.tex"),
            body: body.into(),
            display: false,
        };
        let (opts, prepared) = document(
            directory.path(),
            config(None),
            vec![equation(&fixture, false, body)],
        );
        let source = pdf::dump(&prepared, &opts).unwrap().0;
        assert_eq!(source.contains("#mi(\""), body.contains('`'), "{source}");
        assert!(!source.contains("#mi(``"), "{source}");
        let parsed =
            parse_generated(directory.path(), &source, &prepared.config);
        assert_eq!(
            math_nodes(&parsed.document),
            vec![(false, MathDialect::Latex, body.into())]
        );
    }
}

#[test]
fn configured_macros_and_unnumbered_recovery_use_the_real_pipeline() {
    let directory = tempfile::tempdir().unwrap();
    let fixture =
        fixtures().into_iter().find(|f| f.name.starts_with("31-")).unwrap();
    let (opts, prepared) = document(
        directory.path(),
        config(Some(&fixture)),
        vec![equation(&fixture, fixture.display, &fixture.body)],
    );
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    world.reset();
    let (written, diagnostics) = pdf::build(
        &prepared,
        &opts,
        &directory.path().join("configured.pdf"),
        world,
    )
    .unwrap();
    assert!(!diagnostics.has_errors());
    assert!(!diagnostics.iter().any(|d| d.code == "math-conversion-failed"));
    assert!(fs::read(&written[0]).unwrap().starts_with(b"%PDF-"));
    let bad =
        fixtures().into_iter().find(|f| f.name.starts_with("18-")).unwrap();
    let mut block = equation(&bad, true, &bad.body);
    if let Block::Math { attrs, .. } = &mut block {
        attrs.classes.push("unnumbered".into());
    }
    let (opts, prepared) =
        document(directory.path(), config(None), vec![block]);
    let source = pdf::dump(&prepared, &opts).unwrap().0;
    assert!(
        source.contains("#math.equation(block: true, numbering: none, $#raw("),
        "{source}"
    );
    let world = load(&mut worlds, &opts, &prepared);
    let pdf = compile_pdf(world, &pdf_options()).into_result().unwrap().0;
    assert!(pdf.bytes.starts_with(b"%PDF-"));
}

#[test]
fn strict_mixed_builds_keep_both_outputs_and_escalate_only_conversion_findings()
{
    for strict in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("writer.toml"), format!("[build]\nout_dir='out'\n[pdf]\nsystem_fonts=false\ntimestamp='{TIMESTAMP}'\n")).unwrap();
        fs::write(
            directory.path().join("document.md"),
            "# Per-equation recovery\n\n$\\foo{x}$\n",
        )
        .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
        command
            .current_dir(directory.path())
            .env("NO_COLOR", "1")
            .env_remove("RUST_LOG")
            .env_remove("CLICOLOR_FORCE")
            .args(["build", "document.md", "--to", "html,pdf"]);
        if strict {
            command.arg("--strict");
        }
        let output = command
            .assert()
            .code(if strict { 1 } else { 0 })
            .get_output()
            .clone();
        assert!(directory.path().join("out/document.html").is_file());
        let bytes =
            fs::read(directory.path().join("out/document.pdf")).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            stderr.matches("pdf: cannot convert equation").count(),
            1,
            "{stderr}"
        );
        assert!(
            stderr.contains(if strict {
                "error: pdf:"
            } else {
                "warning: pdf:"
            }),
            "{stderr}"
        );
    }
}

#[test]
fn tag_warnings_keep_base_severity_under_strict_policy() {
    let fixture =
        fixtures().into_iter().find(|f| f.name.starts_with("27-")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (_, prepared) = document(
        directory.path(),
        config(None),
        vec![equation(&fixture, fixture.display, &fixture.body)],
    );
    let source = emitted(&prepared);
    let findings = pipeline::escalate_strict(
        source.diagnostics,
        Some(true),
        &prepared.config,
    );
    let tags = findings
        .iter()
        .filter(|d| d.code == "math-tag-ignored")
        .collect::<Vec<_>>();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].level, Level::Warning);
    assert!(!findings.has_errors(), "{findings:?}");
}
