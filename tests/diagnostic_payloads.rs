//! Actual compiler source/package locations through the frozen browser payload.

use std::fs;
use std::path::Path;

use serde_json::json;
use typst::foundations::Datetime;
use writer::config::{Config, Target};
use writer::error::{Diagnostics, Error, Level};
use writer::pipeline::{self, BuildFailure, BuildOptions, Mode, WorldSlot};
use writer::serve::events::{DiagnosticJson, ErrorPayload};
use writer::typst::compile::{Compiled, compile_document};
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WorldOptions, WriterWorld, world_inputs};

fn failure<T: std::fmt::Debug>(compiled: Compiled<T>) -> BuildFailure {
    assert!(compiled.output.is_none(), "unexpected compiler success");
    let diagnostics = compiled.diagnostics.clone();
    BuildFailure {
        error: Box::new(compiled.into_result().unwrap_err()),
        diagnostics,
    }
}

fn world(root: &Path, main: MainFile) -> WriterWorld {
    WriterWorld::new(WorldOptions {
        root: root.into(),
        main,
        today: Datetime::from_ymd(2026, 10, 29).unwrap(),
        system_fonts: false,
        font_paths: vec![],
        package_paths: vec![],
        allow_packages: false,
        inputs: world_inputs(&Config::default()),
    })
    .unwrap()
}

#[test]
fn actual_package_error_uses_outermost_user_call_and_keeps_package_hint() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("native.typ");
    let source = concat!(
        "#import \"@writer/doc:0.1.0\": *\n",
        "#let call() = writer-doc(bibliography: \"refs.bib\")[Text]\n",
        "#call()\n",
    );
    fs::write(&input, source).unwrap();
    let mut world = world(directory.path(), MainFile::Path(input.clone()));
    let stopped = failure(compile_document(&mut world));
    let Error::Compile { path, line, hints, .. } = stopped.error.as_ref()
    else {
        panic!("expected compiler error: {:?}", stopped.error);
    };
    assert_eq!(path, &fs::canonicalize(&input).unwrap());
    assert_eq!(*line, Some(3));
    assert!(
        hints
            .iter()
            .any(|hint| { hint.starts_with("in @writer/doc:0.1.0/lib.typ:") }),
        "{hints:?}"
    );
    let payload = ErrorPayload::for_failure(&stopped, &input);
    assert_eq!(payload.kind, "compile");
    assert_eq!((payload.line, payload.cell), (Some(3), None));
    assert_eq!(payload.excerpt.as_deref(), Some("#call()"));
    assert_eq!(&payload.hints, hints);
    assert!(payload.message.contains("writer-doc: pass bibliography: path"));
}

#[test]
fn generated_included_notebook_error_quotes_the_original_cell_line() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("main.md");
    let included = directory.path().join("included.ipynb");
    fs::write(&input, ":::include included.ipynb\n:::\n").unwrap();
    let offending = "Inline `#panic(\"original-cell-failure\")`{=typst}.";
    let notebook = json!({"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":[
        {"cell_type":"markdown","id":"intro","metadata":{},"source":"Intro.\n"},
        {"cell_type":"markdown","id":"failure","metadata":{},
         "source":format!("Before.\n\n{offending}\n")}
    ]});
    fs::write(&included, notebook.to_string()).unwrap();
    let mut config = Config::default();
    config.build.targets = vec![Target::Pdf];
    config.pdf.system_fonts = false;
    config.pdf.allow_packages = false;
    config.pdf.timestamp = "2026-10-29T00:00:00Z".into();
    let config_path = directory.path().join("writer.toml");
    fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
    let mut opts = BuildOptions::new(&input);
    opts.config_path = Some(config_path);
    opts.mode = Mode::Serve { build_id: 1 };
    let prepared = pipeline::prepare(&opts).unwrap();
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    world.reset();
    pipeline::load_typst_main(&prepared, &opts, world).unwrap();
    let stopped = failure(compile_document(world));
    let primary = stopped
        .diagnostics
        .iter()
        .find(|finding| finding.level == Level::Error)
        .unwrap();
    let span = primary.span.as_ref().unwrap();
    assert_eq!((span.cell, span.start_line), (Some(1), 3));
    let canonical_included = fs::canonicalize(&included).unwrap();
    assert_eq!(span.file.as_deref(), Some(canonical_included.as_path()));
    let payload = ErrorPayload::for_failure(&stopped, &input);
    assert_eq!((payload.cell, payload.line), (Some(1), Some(3)));
    assert_eq!(payload.excerpt.as_deref(), Some(offending));
    assert!(payload.file.as_deref().unwrap().ends_with("included.ipynb"));
    assert!(!payload.message.contains("/__writer-main.typ"));
    assert!(!directory.path().join("dist").exists());
}

#[test]
fn unmapped_generated_scaffolding_does_not_quote_a_guessed_source_line() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("original.md");
    fs::write(&input, "Source text unrelated to generated scaffolding.\n")
        .unwrap();
    let mut world = world(directory.path(), MainFile::Virtual);
    world.set_main_text(
        "#panic(\"scaffolding failure\")\n",
        SourceMap::default(),
    );
    let stopped = failure(compile_document(&mut world));
    let primary = stopped
        .diagnostics
        .iter()
        .find(|finding| finding.level == Level::Error)
        .unwrap();
    assert!(primary.span.is_none());
    let payload = ErrorPayload::for_failure(&stopped, &input);
    assert_eq!(
        (payload.line, payload.cell, payload.excerpt),
        (None, None, None)
    );
}

#[test]
fn warning_payload_keeps_exact_fields_and_prefers_included_origin() {
    let mut finding = writer::error::Diagnostic::warning(
        "typst-warning", "warning\n  hint: suggestion",
    )
    .with_path("main.md");
    finding.span = Some(writer::ir::SourceSpan {
        start_line: 4,
        end_line: 6,
        cell: Some(2),
        file: Some("included.ipynb".into()),
    });
    let payload = DiagnosticJson::new(&finding, None);
    let json = serde_json::to_value(payload).unwrap();
    assert_eq!(
        json,
        json!({
            "level":"warning", "code":"typst-warning",
            "message":"warning\n  hint: suggestion", "file":"included.ipynb",
            "line":4, "cell":2,
        })
    );
    let diagnostics: Diagnostics = vec![finding].into();
    assert!(!diagnostics.has_errors());
}
