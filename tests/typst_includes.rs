//! Compiler-backed Markdown/notebook includes and native cell selection.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use base64::Engine as _;
use serde_json::{Value, json};
use typst::World;
use typst::foundations::Datetime;
use typst::layout::{Frame, FrameItem};
use typst::syntax::{FileId, RootedPath, VirtualPath, VirtualRoot};
use typst_layout::PagedDocument;
use writer::config::{Config, Target};
use writer::docx::{ValidateOptions, validate};
use writer::error::{Level, Result};
use writer::ir::{self, Block, Parsed, SourceInfo};
use writer::pipeline::{self, BuildOptions, WorldSlot};
use writer::typst::compile::{compile_document, oracle};
use writer::typst::frontend;
use writer::typst::includes::MarkdownIncludes;
use writer::typst::world::{
    IncludeProvider, MainFile, Synthesized, WorldOptions, WriterWorld,
    world_inputs,
};

#[path = "common/docx.rs"]
mod common_docx;
#[path = "common/math_xml.rs"]
mod math_xml;

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#526d82"/></svg>"##;
const CONFIG: &str = "[build]\nout_dir='out'\n[pdf]\nsystem_fonts=false\nallow_packages=false\ntimestamp='2026-10-29T00:00:00Z'\n";

fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn config() -> Config {
    toml::from_str(CONFIG).unwrap()
}

fn source(path: &Path) -> SourceInfo {
    let path = fs::canonicalize(path).unwrap();
    SourceInfo {
        base_dir: path.parent().unwrap().to_owned(),
        kind: ir::detect_kind(&path).unwrap(),
        path,
    }
}

fn id(path: &str) -> FileId {
    RootedPath::new(VirtualRoot::Project, VirtualPath::new(path).unwrap())
        .intern()
}

fn world(root: &Path, main: &str) -> WriterWorld {
    let config = config();
    let mut inputs = world_inputs(&config);
    inputs.insert("writer-oracle".into(), "1".into());
    let mut world = WriterWorld::new(WorldOptions {
        root: root.into(),
        main: MainFile::Path(root.join(main)),
        today: Datetime::from_ymd(2026, 10, 29).unwrap(),
        system_fonts: false,
        font_paths: Vec::new(),
        package_paths: Vec::new(),
        allow_packages: false,
        inputs,
    })
    .unwrap();
    world.set_include_provider(Box::new(MarkdownIncludes { config }));
    world
}

fn parse(root: &Path, text: &str) -> Parsed {
    write(root, "main.typ", text);
    let parsed =
        frontend::parse(&source(&root.join("main.typ")), text, &config())
            .unwrap();
    assert!(parsed.document.invariant_violations().is_empty());
    parsed
}

fn frame_text(frame: &Frame, output: &mut String) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => frame_text(&group.frame, output),
            FrameItem::Text(run) => output.push_str(&run.text),
            _ => {}
        }
    }
}

fn visible(document: &PagedDocument) -> String {
    let mut output = String::new();
    for page in document.pages() {
        frame_text(&page.frame, &mut output);
    }
    output.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn compile(world: &mut WriterWorld) -> PagedDocument {
    let compiled = compile_document(world);
    assert!(compiled.output.is_some(), "{:?}", compiled.diagnostics);
    let (document, diagnostics) = compiled.into_result().unwrap();
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    document
}

fn notebook(cells: Value) -> String {
    json!({"nbformat":4,"nbformat_minor":5,
        "metadata":{"language_info":{"name":"python"}},"cells":cells})
    .to_string()
}

fn md(id: &str, text: &str, tags: &[&str]) -> Value {
    json!({"cell_type":"markdown","id":id,"source":text,"metadata":{"tags":tags}})
}

fn code(id: &str, text: &str, output: Value, tags: &[&str]) -> Value {
    json!({"cell_type":"code","id":id,"source":text,"execution_count":1,
        "metadata":{"tags":tags},"outputs":[{"output_type":"display_data",
            "metadata":{},"data":output}]})
}

fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/")
}

fn provider_snapshot(synthesized: &Synthesized, root: &Path) -> Value {
    let lines = (1..=synthesized.map.len()).map(|line| {
        synthesized.map.lookup(line).map(|span| json!({
            "start_line":span.start_line,"end_line":span.end_line,"cell":span.cell,
            "file":span.file.as_ref().map(|file| relative(file,root)),
        }))
    }).collect::<Vec<_>>();
    let diagnostics = synthesized.diagnostics.iter().map(|finding| json!({
        "code":finding.code,"level":finding.level,"message":finding.message,
        "path":finding.path.as_ref().map(|path| relative(path,root)),
        "span":finding.span.as_ref().map(|span| json!({
            "start_line":span.start_line,"end_line":span.end_line,"cell":span.cell,
            "file":span.file.as_ref().map(|path| relative(path,root)),
        })),
    })).collect::<Vec<_>>();
    json!({"text":synthesized.text,"map":lines,"diagnostics":diagnostics,
        "files":synthesized.files.iter().map(|path| relative(path,root)).collect::<Vec<_>>()})
}

#[test]
fn markdown_provider_is_an_unresolved_fragment_with_stable_note_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    let body = concat!(
        "---\ntitle: Child metadata\ntoc: true\n---\n\n",
        "## Child heading\n\nSee @sec-parent and cite @knuth1984. Note[^same].\n\n",
        "[^same]: Child note.\n",
    );
    write(&root, "notes/part.md", body);
    let mut options = config();
    options.pdf.template = root.join("must-not-be-imported.typ");
    let provider = MarkdownIncludes { config: options };
    let synthesized =
        provider.synthesize(&root.join("notes/part.md"), &root).unwrap();
    assert!(!synthesized.text.contains("#show:"));
    assert!(!synthesized.text.contains("#outline"));
    assert!(!synthesized.text.contains("must-not-be-imported"));
    assert!(!synthesized.text.contains("Child metadata"));
    assert!(!synthesized.text.contains("[?"));
    assert!(synthesized.text.contains("@sec-parent"));
    assert!(synthesized.text.contains("#ref(<knuth1984>);. Note"));
    assert!(!synthesized.text.contains("<child-heading>"));
    let prefix = format!("{}-", &blake3::hash(b"notes/part.md").to_hex()[..8]);
    assert!(synthesized.text.contains(&format!("<writer-fn-{prefix}same>")));
    assert_eq!(synthesized.map.len(), synthesized.text.lines().count());
    let metadata = synthesized
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "include-meta-ignored")
        .collect::<Vec<_>>();
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].path.as_ref(), Some(&root.join("notes/part.md")));
    assert_eq!(metadata[0].span.as_ref().unwrap().start_line, 1);
    assert_eq!(metadata[0].span.as_ref().unwrap().cell, None);
    insta::assert_json_snapshot!(
        "markdown_fragment",
        provider_snapshot(&synthesized, &root)
    );
    let alias = provider
        .synthesize(&root.join("notes/../notes/./part.md"), &root)
        .unwrap();
    assert_eq!(alias.text, synthesized.text);
    let relocated = tempfile::tempdir().unwrap();
    let second_root = fs::canonicalize(relocated.path()).unwrap();
    write(&second_root, "notes/part.md", body);
    let relocated = provider
        .synthesize(&second_root.join("notes/part.md"), &second_root)
        .unwrap();
    assert_eq!(relocated.text, synthesized.text);
    write(&root, "explicit.md", "---\ntoc: true\n---\n\n:::toc depth=2\n:::\n");
    let explicit =
        provider.synthesize(&root.join("explicit.md"), &root).unwrap();
    assert_eq!(explicit.text.matches("#outline").count(), 1);
}

#[test]
fn notebook_module_preserves_empty_entries_imports_escapes_and_exact_source_maps()
 {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "helper.typ", "#let message = [RAW-IMPORT-MARKER]\n");
    let cells = json!([
        md("intro\"\\\n\u{1}", "MARKDOWN-CELL-ZERO $x^2$.\n", &["paper","quoted\"\\\n\u{1}"]),
        code("code", "print('CODE-INPUT')", json!({"text/plain":"CODE-OUTPUT"}), &["paper"]),
        md("empty-md", "", &[]),
        {"cell_type":"code","id":"empty-code","source":"","metadata":{},"execution_count":null,"outputs":[]},
        md("removed", "REMOVED-CONTENT", &["remove-cell"]),
        {"cell_type":"mystery","id":"unknown","source":"UNKNOWN-CONTENT","metadata":{}},
        {"cell_type":"raw","id":"raw","source":"#import \"helper.typ\": message\n#message","metadata":{"raw_mimetype":"text/typst"}},
        {"cell_type":"raw","id":"raw-md","source":"RAW-MARKDOWN-CELL-SEVEN","metadata":{"raw_mimetype":"text/markdown"}},
    ]);
    let bytes = notebook(cells);
    write(&root, "module.ipynb", &bytes);
    let synthesized = MarkdownIncludes { config: config() }
        .synthesize(&root.join("module.ipynb"), &root)
        .unwrap();
    assert_eq!(fs::read_to_string(root.join("module.ipynb")).unwrap(), bytes);
    assert_eq!(
        synthesized.text.matches("#import \"@writer/doc:0.1.0\"").count(),
        1
    );
    assert_eq!(
        synthesized.text.matches("#import \"@preview/mitex:0.2.7\"").count(),
        1
    );
    assert_eq!(
        synthesized.text.matches("#import \"helper.typ\": message").count(),
        1
    );
    assert!(synthesized.text.contains("\\u{1}"));
    assert!(!synthesized.text.contains("\\u0001"));
    assert!(!synthesized.text.contains("REMOVED-CONTENT"));
    assert!(!synthesized.text.contains("UNKNOWN-CONTENT"));
    assert!(synthesized.text.contains("id: \"empty-md\""));
    assert!(synthesized.text.contains("id: \"empty-code\""));
    assert_eq!(synthesized.map.len(), synthesized.text.lines().count());
    let entries_start = synthesized
        .text
        .lines()
        .position(|line| line.starts_with("#let writer-cells"))
        .unwrap();
    let mut mapped_zero = 0;
    let mut mapped_seven = 0;
    for (index, line) in synthesized.text.lines().enumerate() {
        if index <= entries_start || line.starts_with("#for ") {
            assert!(synthesized.map.lookup(index + 1).is_none(), "{line}");
        }
        if line.contains("MARKDOWN-CELL-ZERO") {
            mapped_zero += 1;
            let span = synthesized.map.lookup(index + 1).unwrap();
            assert_eq!(span.cell, Some(0));
            assert_eq!(span.start_line, 1);
            assert_eq!(span.file.as_ref(), Some(&root.join("module.ipynb")));
        }
        if line.contains("RAW-MARKDOWN-CELL-SEVEN") {
            mapped_seven += 1;
            let span = synthesized.map.lookup(index + 1).unwrap();
            assert_eq!(span.cell, Some(7));
            assert_eq!(span.start_line, 1);
            assert_eq!(span.file.as_ref(), Some(&root.join("module.ipynb")));
        }
        if line.contains("#import \"helper.typ\"") {
            assert_eq!(
                synthesized.map.lookup(index + 1).unwrap().cell,
                Some(6)
            );
        }
    }
    assert_eq!((mapped_zero, mapped_seven), (1, 1));
    insta::assert_json_snapshot!(
        "notebook_module",
        provider_snapshot(&synthesized, &root)
    );
    write(
        &root,
        "main.typ",
        concat!(
            "#import \"module.ipynb\": writer-cells\n",
            "#assert(writer-cells.map(c => c.index) == (0, 1, 2, 3, 6, 7))\n",
            "#assert(writer-cells.find(c => c.index == 3).input == none)\n",
            "#assert(writer-cells.find(c => c.index == 3).outputs == none)\n",
            "MODULE-IMPORTED\n",
        ),
    );
    let mut world = world(&root, "main.typ");
    assert!(visible(&compile(&mut world)).contains("MODULE-IMPORTED"));
}

#[test]
fn provider_warning_dedup_keeps_distinct_cells_in_first_seen_order() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(
        &root,
        "warnings.ipynb",
        notebook(json!([
            code("first", "x=1", json!({"text/latex":"$\\nosuch{x}$"}), &[]),
            code("second", "x=2", json!({"text/latex":"$\\nosuch{x}$"}), &[]),
        ])),
    );
    let synthesized = MarkdownIncludes { config: config() }
        .synthesize(&root.join("warnings.ipynb"), &root)
        .unwrap();
    let findings = synthesized.diagnostics.iter().collect::<Vec<_>>();
    assert_eq!(findings.len(), 2, "{:?}", synthesized.diagnostics);
    assert!(
        findings.iter().all(|finding| finding.code == "math-conversion-failed")
    );
    assert_eq!(
        findings
            .iter()
            .map(|finding| finding.span.as_ref().unwrap().cell)
            .collect::<Vec<_>>(),
        [Some(0), Some(1)]
    );
    assert_eq!(findings[0].message, findings[1].message);
    assert_ne!(findings[0], findings[1]);
    assert!(
        findings.iter().all(|finding| finding.path.as_ref()
            == Some(&root.join("warnings.ipynb")))
    );
}

struct CountingProvider {
    provider: MarkdownIncludes,
    calls: Arc<AtomicUsize>,
}

impl IncludeProvider for CountingProvider {
    fn synthesize(&self, path: &Path, root: &Path) -> Result<Synthesized> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.provider.synthesize(path, root)
    }
}

#[test]
fn real_provider_cache_keeps_original_read_and_json_bytes_across_reset() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    let markdown = "MARKDOWN-RAW-BYTES.\n";
    let notebook = notebook(json!([code(
        "saved",
        "panic('THIS IS SAVED CODE, NEVER EXECUTED')",
        json!({"text/plain":"SAVED-OUTPUT"}),
        &[]
    ),]));
    write(&root, "part.md", markdown);
    write(&root, "saved.ipynb", &notebook);
    let text = format!(
        concat!(
            "#include \"part.md\"\n#include \"saved.ipynb\"\n",
            "#assert(read(\"part.md\") == {markdown})\n",
            "#assert(json(\"saved.ipynb\").nbformat == 4)\n",
            "#assert(json(\"saved.ipynb\").cells.len() == 1)\n",
            "NATIVE-READ-JSON-OK\n",
        ),
        markdown = serde_json::to_string(markdown).unwrap()
    );
    write(&root, "main.typ", &text);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut world = world(&root, "main.typ");
    world.set_include_provider(Box::new(CountingProvider {
        provider: MarkdownIncludes { config: config() },
        calls: calls.clone(),
    }));
    for name in ["/part.md", "/saved.ipynb"] {
        let first = world.source(id(name)).unwrap();
        assert_eq!(world.source(id(name)).unwrap().text(), first.text());
        assert!(first.text().starts_with("// Generated by writer"));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(&*world.file(id("/part.md")).unwrap(), markdown.as_bytes());
    assert_eq!(&*world.file(id("/saved.ipynb")).unwrap(), notebook.as_bytes());
    let rendered = visible(&compile(&mut world));
    assert!(rendered.contains("MARKDOWN-RAW-BYTES"));
    assert!(rendered.contains("SAVED-OUTPUT"));
    assert!(rendered.contains("NATIVE-READ-JSON-OK"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    world.reset();
    assert!(world.accessed().is_empty());
    assert!(world.include_diagnostics().is_empty());
    assert_eq!(visible(&compile(&mut world)), rendered);
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(&*world.file(id("/saved.ipynb")).unwrap(), notebook.as_bytes());
    assert_eq!(fs::read_to_string(root.join("saved.ipynb")).unwrap(), notebook);
}

#[test]
fn failed_synthesis_is_cached_attempted_and_recovers_only_after_reset() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "main.typ", "#include \"broken.ipynb\"\n");
    write(&root, "broken.ipynb", b"{ invalid notebook bytes\r\n");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut world = world(&root, "main.typ");
    world.set_include_provider(Box::new(CountingProvider {
        provider: MarkdownIncludes { config: config() },
        calls: calls.clone(),
    }));
    assert!(world.source(id("/broken.ipynb")).is_err());
    assert!(world.source(id("/broken.ipynb")).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(world.accessed().contains(&root.join("broken.ipynb")));
    assert_eq!(
        &*world.file(id("/broken.ipynb")).unwrap(),
        b"{ invalid notebook bytes\r\n"
    );
    write(
        &root,
        "broken.ipynb",
        notebook(json!([md("recovered", "RECOVERED", &[])])),
    );
    assert!(world.source(id("/broken.ipynb")).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    world.reset();
    assert!(visible(&compile(&mut world)).contains("RECOVERED"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[cfg(unix)]
#[test]
fn provider_canonical_aliases_are_stable_and_outside_root_symlinks_are_refused()
{
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "notes/part.md", "A note[^same].\n\n[^same]: Scoped.\n");
    write(outside.path(), "outside.md", "Outside root.\n");
    std::os::unix::fs::symlink(
        root.join("notes/part.md"),
        root.join("alias.md"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("outside.md"),
        root.join("escape.md"),
    )
    .unwrap();
    let provider = MarkdownIncludes { config: config() };
    let actual =
        provider.synthesize(&root.join("notes/part.md"), &root).unwrap();
    assert_eq!(
        provider.synthesize(&root.join("alias.md"), &root).unwrap().text,
        actual.text
    );
    let error =
        provider.synthesize(&root.join("escape.md"), &root).err().unwrap();
    assert!(error.to_string().contains("outside"), "{error}");
}

#[test]
fn markdown_provider_keeps_transitive_assets_and_nested_source_spans() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(
        &root,
        "parts/outer.md",
        "Outer.\n\n:::include nested/inner.md\n:::\n",
    );
    write(
        &root,
        "parts/nested/inner.md",
        "![Nested image](../../picture.svg){#fig-inner}\n",
    );
    write(&root, "picture.svg", SVG);
    let synthesized = MarkdownIncludes { config: config() }
        .synthesize(&root.join("parts/outer.md"), &root)
        .unwrap();
    assert_eq!(
        synthesized.files,
        [root.join("parts/nested/inner.md"), root.join("picture.svg"),]
    );
    assert_eq!(synthesized.assets.len(), 1);
    let asset = synthesized.assets.iter().next().unwrap();
    assert_eq!(synthesized.assets.read(&asset.id).unwrap(), SVG.as_bytes());
    let generated = synthesized
        .text
        .lines()
        .position(|line| line.contains("/__assets/"))
        .unwrap()
        + 1;
    let span = synthesized.map.lookup(generated).unwrap();
    assert_eq!(span.file.as_ref(), Some(&root.join("parts/nested/inner.md")));
    assert_eq!(span.start_line, 1);
    write(&root, "main.typ", "#include \"parts/outer.md\"\n");
    let mut world = world(&root, "main.typ");
    compile(&mut world);
    assert!(world.accessed().contains(&root.join("parts/nested/inner.md")));
    assert!(world.accessed().contains(&root.join("picture.svg")));
}

#[test]
fn compiler_error_in_a_later_notebook_cell_uses_its_real_file_cell_and_line() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(
        &root,
        "late.ipynb",
        notebook(json!([
            md("first", "FIRST", &[]), md("second", "SECOND", &[]),
            {"cell_type":"raw","id":"bad","source":"#missing_native_function()","metadata":{"raw_mimetype":"text/typst"}},
        ])),
    );
    write(&root, "main.typ", "#include \"late.ipynb\"\n");
    let mut world = world(&root, "main.typ");
    let compiled = compile_document(&mut world);
    assert!(compiled.output.is_none());
    let finding = compiled
        .diagnostics
        .iter()
        .find(|finding| finding.level == Level::Error)
        .unwrap();
    assert_eq!(finding.path.as_ref(), Some(&root.join("late.ipynb")));
    let span = finding.span.as_ref().unwrap();
    assert_eq!(span.cell, Some(2));
    assert_eq!(span.start_line, 1);
    assert_eq!(span.file.as_ref(), Some(&root.join("late.ipynb")));
    assert!(
        !finding.message.contains("/__render")
            && !finding.message.contains("writer-cells")
    );
}

fn structured_text(blocks: &[Block]) -> String {
    let mut output = String::new();
    for block in blocks {
        match block {
            Block::Paragraph { inlines, .. }
            | Block::Heading { inlines, .. } => {
                output.push_str(&ir::plain_text(inlines));
            }
            Block::CodeBlock { text, .. } => output.push_str(text),
            Block::BlockQuote { blocks, .. }
            | Block::Div { blocks, .. }
            | Block::Callout { blocks, .. } => {
                output.push_str(&structured_text(blocks))
            }
            Block::Figure { body, caption, .. } => {
                if let Some(caption) = caption {
                    output.push_str(&ir::plain_text(caption));
                }
                output.push_str(&structured_text(body));
            }
            Block::Cell(cell) => {
                if cell.show_input {
                    output.push_str(&cell.source);
                }
                if cell.show_output {
                    for candidate in &cell.outputs {
                        output.push_str(&structured_text(
                            candidate.pick("typst"),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    output
}

fn selection_notebook() -> String {
    notebook(json!([
        md("zero", "ZERO-MARKER", &["paper"]),
        code("one", "#| lst-label: lst-one\n#| lst-cap: LISTING-CAPTION\nINPUT-ONE",
            json!({"text/plain":"OUTPUT-ONE"}), &["paper"]),
        md("two", "TWO-MARKER", &["appendix"]),
        md("removed", "REMOVED-MARKER", &["paper","remove-cell"]),
        {"cell_type":"mystery","id":"unknown","source":"UNKNOWN-MARKER","metadata":{}},
        md("empty", "", &["paper"]),
        code("input-hidden", "#| echo: false\nHIDDEN-INPUT", json!({"text/plain":"POLICY-OUTPUT"}), &["paper"]),
        code("output-hidden", "#| output: false\nPOLICY-INPUT", json!({"text/plain":"HIDDEN-OUTPUT"}), &["paper"]),
        {"cell_type":"raw","id":"raw","source":"RAW-MARKER","metadata":{"raw_mimetype":"text/markdown","tags":["raw"]}},
    ]))
}

fn selection_source(arguments: &str) -> String {
    format!(
        concat!(
            "#import \"@writer/doc:0.1.0\": *\n",
            "BEGIN-SELECTION\n\n",
            "#include-notebook(path(\"selection.ipynb\"), {arguments})\n\n",
            "END-SELECTION\n",
        ),
        arguments = arguments
    )
}

#[test]
fn native_and_structured_selection_match_indices_ids_tags_empty_and_visibility()
{
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "selection.ipynb", selection_notebook());
    let markers = [
        "ZERO-MARKER", "INPUT-ONE", "OUTPUT-ONE", "TWO-MARKER",
        "POLICY-OUTPUT", "POLICY-INPUT", "RAW-MARKER", "LISTING-CAPTION",
    ];
    let cases: &[(&str, &[&str])] = &[
        ("cells: auto", &markers),
        ("cells: 1", &["INPUT-ONE", "OUTPUT-ONE", "LISTING-CAPTION"]),
        ("cells: \"one\"", &["INPUT-ONE", "OUTPUT-ONE", "LISTING-CAPTION"]),
        (
            "cells: (2, \"one\", 1, 0, \"zero\")",
            &[
                "ZERO-MARKER", "INPUT-ONE", "OUTPUT-ONE", "TWO-MARKER",
                "LISTING-CAPTION",
            ],
        ),
        (
            "tags: \"paper\"",
            &[
                "ZERO-MARKER", "INPUT-ONE", "OUTPUT-ONE", "POLICY-OUTPUT",
                "POLICY-INPUT", "LISTING-CAPTION",
            ],
        ),
        ("tags: (\"appendix\", \"raw\")", &["TWO-MARKER", "RAW-MARKER"]),
        ("cells: (0, 1, 2), tags: (\"appendix\", \"raw\")", &["TWO-MARKER"]),
        ("cells: (0, 2), tags: \"paper\"", &["ZERO-MARKER"]),
        ("cells: ()", &[]),
        ("tags: ()", &[]),
        ("tags: \"Paper\"", &[]),
        ("cells: \"empty\"", &[]),
        ("cells: 1, hide-input: true", &["OUTPUT-ONE"]),
        ("cells: 1, hide-output: true", &["INPUT-ONE", "LISTING-CAPTION"]),
        ("cells: 1, hide-input: true, hide-output: true", &[]),
        ("cells: 6, hide-input: false", &["POLICY-OUTPUT"]),
        ("cells: 7, hide-output: false", &["POLICY-INPUT"]),
        (
            "cells: (0, 1), hide-input: true, hide-output: true",
            &["ZERO-MARKER"],
        ),
    ];
    for (arguments, expected) in cases {
        let text = selection_source(arguments);
        let parsed = parse(&root, &text);
        assert!(
            !parsed.diagnostics.has_errors(),
            "{arguments}: {:?}",
            parsed.diagnostics
        );
        let structured = structured_text(&parsed.document.blocks);
        let native = visible(&compile(&mut world(&root, "main.typ")));
        for marker in markers {
            assert_eq!(
                structured.matches(marker).count(),
                usize::from(expected.contains(&marker)),
                "structured {arguments}: {marker}: {structured}"
            );
            assert_eq!(
                native.matches(marker).count(),
                usize::from(expected.contains(&marker)),
                "native {arguments}: {marker}: {native}"
            );
        }
        for forbidden in [
            "REMOVED-MARKER", "UNKNOWN-MARKER", "HIDDEN-INPUT", "HIDDEN-OUTPUT",
        ] {
            assert!(
                !structured.contains(forbidden),
                "structured {arguments}: {forbidden}"
            );
            assert!(
                !native.contains(forbidden),
                "native {arguments}: {forbidden}"
            );
        }
        assert!(
            native.contains("BEGIN-SELECTION")
                && native.contains("END-SELECTION")
        );
        if expected.contains(&"ZERO-MARKER") && expected.contains(&"TWO-MARKER")
        {
            assert!(
                structured.find("ZERO-MARKER").unwrap()
                    < structured.find("TWO-MARKER").unwrap(),
                "structured document order {arguments}: {structured}"
            );
            assert!(
                native.find("ZERO-MARKER").unwrap()
                    < native.find("TWO-MARKER").unwrap()
            );
        }
        if arguments.contains("hide-input: true") {
            let mut labels = Vec::new();
            ir::walk_blocks(&parsed.document, &mut |block| {
                if let Block::Figure { attrs, .. } = block {
                    labels.extend(attrs.id.iter().cloned());
                }
            });
            assert!(!labels.iter().any(|label| label == "lst-one"));
        }
    }
}

#[test]
fn missing_removed_unknown_and_invalid_selection_refs_have_distinct_contracts()
{
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "selection.ipynb", selection_notebook());
    for (arguments, evidence) in [
        ("cells: 99", "99"),
        ("cells: \"absent\"", "absent"),
        ("cells: \"removed\"", "removed"),
        ("cells: \"unknown\"", "unknown"),
        ("cells: 99, tags: ()", "99"),
    ] {
        let text = selection_source(arguments);
        let parsed = parse(&root, &text);
        let findings = parsed
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "notebook-cell-not-found")
            .collect::<Vec<_>>();
        assert_eq!(findings.len(), 1, "{arguments}: {:?}", parsed.diagnostics);
        assert_eq!(findings[0].level, Level::Error);
        assert!(
            structured_text(&parsed.document.blocks).contains("END-SELECTION")
        );
        let compiled = compile_document(&mut world(&root, "main.typ"));
        assert!(compiled.output.is_none(), "{arguments}");
        assert!(
            compiled
                .diagnostics
                .iter()
                .any(|finding| finding.level == Level::Error
                    && finding.message.contains(evidence)),
            "{arguments}: {:?}",
            compiled.diagnostics
        );
    }
    for arguments in [
        "cells: -1", "cells: true", "cells: (0, true)", "tags: true",
        "tags: (\"paper\", 1)", "hide-input: 1", "hide-output: \"yes\"",
        "cells: range(2)",
    ] {
        let parsed = parse(&root, &selection_source(arguments));
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|finding| finding.code == "E-TYP-ARG-INVALID"),
            "{arguments}: {:?}",
            parsed.diagnostics
        );
        assert!(
            structured_text(&parsed.document.blocks).contains("END-SELECTION")
        );
    }
    for (arguments, evidence) in [
        ("cells: -1", "invalid cell reference -1"),
        ("cells: true", "invalid cell reference true"),
        ("tags: true", "invalid tags value true"),
        ("hide-input: 1", "invalid hide-input value 1"),
    ] {
        write(&root, "main.typ", selection_source(arguments));
        let compiled = compile_document(&mut world(&root, "main.typ"));
        assert!(compiled.output.is_none(), "{arguments}");
        let finding = compiled
            .diagnostics
            .iter()
            .find(|finding| {
                finding.level == Level::Error
                    && finding.message.contains(evidence)
            })
            .unwrap_or_else(|| {
                panic!("{arguments}: {:?}", compiled.diagnostics)
            });
        assert_eq!(finding.path.as_ref(), Some(&root.join("main.typ")));
        assert_eq!(finding.span.as_ref().unwrap().start_line, 4);
        assert_eq!(finding.span.as_ref().unwrap().cell, None);
    }
    let string_file = concat!(
        "#import \"@writer/doc:0.1.0\": *\n",
        "#include-notebook(\"selection.ipynb\", cells: 0)\n",
    );
    let parsed = parse(&root, string_file);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|finding| finding.code == "W-TYP-PATH-STRING")
    );
    assert!(structured_text(&parsed.document.blocks).contains("ZERO-MARKER"));
    let compiled = compile_document(&mut world(&root, "main.typ"));
    assert!(compiled.output.is_none());
    assert!(
        compiled
            .diagnostics
            .iter()
            .any(|finding| finding.message.contains("pass path(")),
        "{:?}",
        compiled.diagnostics
    );

    let giant = "18446744073709551616";
    let text = selection_source(&format!("cells: {giant}"));
    let parsed = parse(&root, &text);
    // The pinned lexer promotes an overflowing decimal integer to Float.
    let argument = parsed
        .diagnostics
        .iter()
        .find(|finding| finding.code == "E-TYP-ARG-INVALID")
        .unwrap_or_else(|| panic!("{:?}", parsed.diagnostics));
    let line = argument.span.as_ref().unwrap().start_line;
    assert!(text.lines().nth(line - 1).unwrap().contains(giant));
    let native = compile_document(&mut world(&root, "main.typ"));
    assert!(native.output.is_none());
    let finding = native
        .diagnostics
        .iter()
        .find(|finding| finding.level == Level::Error)
        .unwrap();
    assert!(finding.message.contains("invalid cell reference"), "{finding:?}");
    assert!(
        text.lines()
            .nth(finding.span.as_ref().unwrap().start_line - 1)
            .unwrap()
            .contains(giant)
    );

    // Nondecimal overflow stays a parser error before helper execution.
    let giant = "0x10000000000000000";
    let text = selection_source(&format!("cells: {giant}"));
    let parsed = parse(&root, &text);
    let syntax = parsed
        .diagnostics
        .iter()
        .find(|finding| finding.code == "E-TYP-SYNTAX")
        .unwrap_or_else(|| panic!("{:?}", parsed.diagnostics));
    assert!(
        text.lines()
            .nth(syntax.span.as_ref().unwrap().start_line - 1)
            .unwrap()
            .contains(giant)
    );
    let native = compile_document(&mut world(&root, "main.typ"));
    assert!(native.output.is_none());
    let finding = native
        .diagnostics
        .iter()
        .find(|finding| finding.level == Level::Error)
        .unwrap();
    assert!(
        finding.message.contains("invalid hexadecimal number"),
        "{finding:?}"
    );
    assert!(
        text.lines()
            .nth(finding.span.as_ref().unwrap().start_line - 1)
            .unwrap()
            .contains(giant)
    );

    let dynamic = concat!(
        "#import \"@writer/doc:0.1.0\": *\n",
        "#include-notebook(path(\"selection\" + \".ipynb\"), cells: 0)\n",
    );
    let parsed = parse(&root, dynamic);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|finding| finding.code == "E-TYP-INCLUDE-DYNAMIC")
    );
    assert!(
        visible(&compile(&mut world(&root, "main.typ")))
            .contains("ZERO-MARKER")
    );
    let parsed = parse(&root, &selection_source("cells: 0, unexpected: true"));
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|finding| finding.code == "W-TYP-ARG-IGNORED")
    );
    assert!(structured_text(&parsed.document.blocks).contains("ZERO-MARKER"));
}

#[cfg(target_pointer_width = "32")]
#[test]
fn a_valid_typst_integer_that_overflows_usize_is_a_structural_argument_error() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "selection.ipynb", selection_notebook());
    let parsed = parse(&root, &selection_source("cells: 4294967296"));
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|finding| finding.code == "E-TYP-ARG-INVALID")
    );
}

#[test]
fn malformed_duplicate_ids_document_the_first_original_vs_first_retained_difference()
 {
    for first in [
        md("duplicate", "REMOVED-FIRST", &["remove-cell"]),
        json!({"cell_type":"mystery","id":"duplicate","source":"UNKNOWN-FIRST","metadata":{}}),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        write(
            &root,
            "selection.ipynb",
            notebook(json!([
                first,
                md("duplicate", "RETAINED-DUPLICATE", &[]),
            ])),
        );
        let parsed = parse(&root, &selection_source("cells: \"duplicate\""));
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|finding| finding.code == "notebook-cell-not-found")
        );
        assert!(
            !structured_text(&parsed.document.blocks)
                .contains("RETAINED-DUPLICATE")
        );
        let native = visible(&compile(&mut world(&root, "main.typ")));
        assert!(native.contains("RETAINED-DUPLICATE"));
        assert!(
            !native.contains("REMOVED-FIRST")
                && !native.contains("UNKNOWN-FIRST")
        );
    }
}

#[test]
fn selected_cross_cell_notes_rebind_structurally_but_native_excluded_owner_stays_missing()
 {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(
        &root,
        "selection.ipynb",
        notebook(json!([
            md("owner", "FIRST-REFERENCE[^shared].", &[]),
            md("later", "LATER-REFERENCE[^shared].", &[]),
            md("definition", "[^shared]: SHARED-DEFINITION-BODY", &[]),
        ])),
    );
    let parsed = parse(&root, &selection_source("cells: 1"));
    assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
    let value = serde_json::to_string(&parsed.document.blocks).unwrap();
    assert!(value.contains("SHARED-DEFINITION-BODY"));
    assert!(!value.contains("FIRST-REFERENCE"));
    let compiled = compile_document(&mut world(&root, "main.typ"));
    assert!(compiled.output.is_none());
    assert!(
        compiled
            .diagnostics
            .iter()
            .any(|finding| finding.level == Level::Error
                && finding.message.contains("writer-fn-")),
        "{:?}",
        compiled.diagnostics
    );
}

#[test]
fn eager_native_content_can_fail_even_when_the_raw_or_math_cell_is_unselected()
{
    for (raw, evidence) in [
        ("#panic(\"EAGER-UNSELECTED-RAW\")", "EAGER-UNSELECTED-RAW"),
        ("$ #t53_eager_symbol $", "t53_eager_symbol"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        write(
            &root,
            "selection.ipynb",
            notebook(json!([
                md("safe", "SELECTED-SAFE-CONTENT", &[]),
                {"cell_type":"raw","id":"unselected","source":raw,"metadata":{"raw_mimetype":"text/typst"}},
            ])),
        );
        let parsed = parse(&root, &selection_source("cells: 0"));
        assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
        assert!(
            structured_text(&parsed.document.blocks)
                .contains("SELECTED-SAFE-CONTENT")
        );
        let compiled = compile_document(&mut world(&root, "main.typ"));
        assert!(compiled.output.is_none());
        assert!(
            compiled
                .diagnostics
                .iter()
                .any(|finding| finding.level == Level::Error
                    && finding.message.contains(evidence)),
            "{:?}",
            compiled.diagnostics
        );
    }
}

#[test]
fn mixed_typst_markdown_notebook_chain_keeps_outer_meta_toc_and_nested_spans() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "bridge.typ", "#include \"parts/outer.md\"\n");
    write(
        &root,
        "parts/outer.md",
        concat!(
            "---\ntitle: Markdown child\ntoc: true\n---\n\n",
            "MD-BRIDGE.\n\n:::toc depth=1\n:::\n\n:::include nb.ipynb\n:::\n",
        ),
    );
    let mut nb: Value = serde_json::from_str(&notebook(json!([md(
        "intro",
        concat!(
            "NB-MARKER sees @sec-parent and @fig-leaf. Note[^n].\n\n",
            ":::include tail.md\n:::\n\n[^n]: NOTE-BODY.\n",
        ),
        &[]
    ),])))
    .unwrap();
    nb["metadata"]["writer"] = json!({"title":"Notebook child","toc":true});
    write(&root, "parts/nb.ipynb", nb.to_string());
    write(
        &root,
        "parts/tail.md",
        "TAIL-MARKER.\n\n![Leaf](../leaf.svg){#fig-leaf}\n",
    );
    write(&root, "leaf.svg", SVG);
    let text = concat!(
        "#import \"@writer/doc:0.1.0\": *\n",
        "#show: writer-doc.with(title: [OUTER-TITLE], toc: 2, numbering: true)\n",
        "#include \"bridge.typ\"\n\n= Native parent <sec-parent>\n\nROOT-AFTER.\n",
    );
    let parsed = parse(&root, text);
    assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.document.meta.title.as_deref(), Some("OUTER-TITLE"));
    assert_eq!(
        parsed.document.includes,
        [
            root.join("bridge.typ"),
            root.join("parts/outer.md"),
            root.join("parts/nb.ipynb"),
            root.join("parts/tail.md"),
        ]
    );
    assert_eq!(parsed.assets.len(), 1);
    assert!(
        parsed.document.features.notebook && parsed.document.features.footnotes
    );
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "include-meta-ignored")
            .count(),
        2
    );
    let mut tocs = Vec::new();
    let mut locations = BTreeMap::new();
    ir::walk_blocks(&parsed.document, &mut |block| {
        if let Block::Toc { span, .. } = block {
            tocs.push(span.clone());
        }
        if let Block::Paragraph { inlines, span } = block {
            let text = ir::plain_text(inlines);
            for marker in ["MD-BRIDGE", "NB-MARKER", "TAIL-MARKER", "NOTE-BODY"]
            {
                if text.contains(marker) {
                    locations.insert(marker, span.clone().unwrap());
                }
            }
            if text.contains("NB-MARKER") {
                for inline in inlines {
                    if let ir::Inline::Ref { span, .. } = inline {
                        let span = span.as_ref().unwrap();
                        assert_eq!(
                            span.file.as_ref(),
                            Some(&root.join("parts/nb.ipynb"))
                        );
                        assert_eq!(span.cell, Some(0));
                    }
                }
            }
        }
    });
    // An explicit outline anywhere in the walked body suppresses the outer
    // automatic insertion under the existing T50 policy.
    assert_eq!(tocs.len(), 1);
    assert!(tocs[0].is_some());
    let mut effective = config();
    effective.merge_front_matter(&parsed.document.meta).unwrap();
    assert_eq!(effective.document.toc, writer::config::Toc::Depth(2));
    assert_eq!(
        tocs.iter().flatten().next().unwrap().file.as_ref(),
        Some(&root.join("parts/outer.md"))
    );
    assert_eq!(
        locations["MD-BRIDGE"].file.as_ref(),
        Some(&root.join("parts/outer.md"))
    );
    for marker in ["NB-MARKER", "NOTE-BODY"] {
        assert_eq!(
            locations[marker].file.as_ref(),
            Some(&root.join("parts/nb.ipynb"))
        );
        assert_eq!(locations[marker].cell, Some(0));
    }
    assert_eq!(
        locations["TAIL-MARKER"].file.as_ref(),
        Some(&root.join("parts/tail.md"))
    );
    assert_eq!(locations["TAIL-MARKER"].cell, None);
    write(&root, "writer.toml", CONFIG);
    let mut options = BuildOptions::new(root.join("main.typ"));
    options.config_path = Some(root.join("writer.toml"));
    let mut worlds = WorldSlot::new();
    let report = pipeline::build_targets(
        &options,
        &[Target::Pdf, Target::Html, Target::Docx],
        &mut worlds,
    );
    assert!(report.prepared.is_ok(), "{:?}", report.prepared);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(
        report.targets.iter().all(|target| target.written.is_ok()),
        "{:?}",
        report.targets
    );
    let native = visible(&compile(worlds.get(None, &options).unwrap()));
    for marker in
        ["MD-BRIDGE", "NB-MARKER", "TAIL-MARKER", "NOTE-BODY", "ROOT-AFTER"]
    {
        assert!(native.contains(marker), "{marker}: {native}");
    }
    let html = fs::read_to_string(root.join("out/main.html")).unwrap();
    assert!(
        html.contains("MD-BRIDGE")
            && html.contains("NB-MARKER")
            && html.contains("TAIL-MARKER")
    );
    let bytes = fs::read(root.join("out/main.docx")).unwrap();
    assert_eq!(validate(&bytes, &ValidateOptions { builtin_styles: true }), []);
    common_docx::read_back(&bytes);
}

#[test]
fn mixed_chain_cycles_depth_and_failed_descendants_preserve_surrounding_content()
 {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "bridge.typ", "#include \"first.md\"\n");
    write(&root, "first.md", ":::include cycle.ipynb\n:::\n");
    write(
        &root,
        "cycle.ipynb",
        notebook(json!([md("cycle", ":::include first.md\n:::\n", &[]),])),
    );
    let text = "BEFORE.\n\n#include \"bridge.typ\"\n\nAFTER.\n";
    let cycle = parse(&root, text);
    let finding = cycle
        .diagnostics
        .iter()
        .find(|finding| finding.code == "E-TYP-INCLUDE-CYCLE")
        .unwrap();
    for name in ["main.typ", "bridge.typ", "first.md", "cycle.ipynb"] {
        assert!(finding.message.contains(name), "{finding:?}");
    }
    let surrounding = structured_text(&cycle.document.blocks);
    assert!(surrounding.contains("BEFORE") && surrounding.contains("AFTER"));
    write(
        &root,
        "cycle.ipynb",
        notebook(json!([md(
            "missing",
            ":::include missing/child.md\n:::\n",
            &[]
        ),])),
    );
    let missing = parse(&root, text);
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|finding| finding.code == "E-TYP-INCLUDE-MISSING")
    );
    assert!(missing.document.includes.contains(&root.join("missing/child.md")));
    for index in 1..=7 {
        let next = if index + 1 == 2 {
            "n2.ipynb".into()
        } else {
            format!("n{}.md", index + 1)
        };
        let body = if index == 7 {
            "DEPTH-EIGHT".into()
        } else {
            format!(":::include {next}\n:::\n")
        };
        if index == 2 {
            write(
                &root,
                "n2.ipynb",
                notebook(json!([md("depth", &body, &[])])),
            );
        } else {
            write(&root, &format!("n{index}.md"), body);
        }
    }
    write(&root, "bridge.typ", "#include \"n1.md\"\n");
    let eight = parse(&root, text);
    assert!(
        !eight
            .diagnostics
            .iter()
            .any(|finding| finding.code == "E-TYP-INCLUDE-DEPTH"),
        "{:?}",
        eight.diagnostics
    );
    assert!(structured_text(&eight.document.blocks).contains("DEPTH-EIGHT"));
    write(&root, "n7.md", ":::include n8.md\n:::\n");
    write(&root, "n8.md", "TOO-DEEP");
    let nine = parse(&root, text);
    assert!(
        nine.diagnostics
            .iter()
            .any(|finding| finding.code == "E-TYP-INCLUDE-DEPTH"),
        "{:?}",
        nine.diagnostics
    );
    let surrounding = structured_text(&nine.document.blocks);
    assert!(surrounding.contains("BEFORE") && surrounding.contains("AFTER"));
}

#[test]
fn compiler_nested_provider_error_prefers_the_mapped_notebook_file() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "outer.md", ":::include nested.ipynb\n:::\n");
    write(
        &root,
        "nested.ipynb",
        notebook(json!([
            md("safe", "SAFE", &[]),
            {"cell_type":"raw","id":"error","source":"#missing_nested_symbol","metadata":{"raw_mimetype":"text/typst"}},
        ])),
    );
    write(&root, "main.typ", "#include \"outer.md\"\n");
    let compiled = compile_document(&mut world(&root, "main.typ"));
    assert!(compiled.output.is_none());
    let finding = compiled
        .diagnostics
        .iter()
        .find(|finding| finding.level == Level::Error)
        .unwrap();
    assert_eq!(finding.path.as_ref(), Some(&root.join("nested.ipynb")));
    let span = finding.span.as_ref().unwrap();
    assert_eq!(span.file.as_ref(), Some(&root.join("nested.ipynb")));
    assert_eq!(span.cell, Some(1));
    assert_eq!(span.start_line, 1);
}

#[test]
fn fragments_resolve_forward_parent_citations_and_independent_same_name_notes_natively()
 {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    for (name, marker) in [("one.md", "ONE-BODY"), ("two.md", "TWO-BODY")] {
        write(
            &root,
            name,
            format!(
                "See @sec-parent and cite @knuth1984. Note[^same], repeat[^same].\n\n[^same]: {marker}.\n",
            ),
        );
    }
    write(&root, "refs.bib", include_bytes!("fixtures/bib/refs.bib"));
    write(
        &root,
        "main.typ",
        concat!(
            "#import \"@writer/doc:0.1.0\": *\n",
            "#show: writer-doc.with(numbering: true)\n",
            "#include \"one.md\"\n#include \"two.md\"\n",
            "= Native parent <sec-parent>\n",
            "#bibliography(path(\"refs.bib\"), style: \"ieee\")\n",
        ),
    );
    let provider = MarkdownIncludes { config: config() };
    let first = provider.synthesize(&root.join("one.md"), &root).unwrap();
    let second = provider.synthesize(&root.join("two.md"), &root).unwrap();
    let first_label =
        format!("writer-fn-{}-same", &blake3::hash(b"one.md").to_hex()[..8]);
    let second_label =
        format!("writer-fn-{}-same", &blake3::hash(b"two.md").to_hex()[..8]);
    assert_ne!(first_label, second_label);
    assert!(first.text.contains(&first_label));
    assert!(second.text.contains(&second_label));
    assert!(!first.text.contains("[?") && !second.text.contains("[?"));
    let document = compile(&mut world(&root, "main.typ"));
    assert_eq!(
        oracle(&document, "sec-parent").unwrap().number.as_deref(),
        Some("1")
    );
    let rendered = visible(&document);
    assert_eq!(rendered.matches("ONE-BODY").count(), 1, "{rendered}");
    assert_eq!(rendered.matches("TWO-BODY").count(), 1, "{rendered}");
    assert!(
        rendered.contains("Nativeparent") && rendered.contains("[1]"),
        "{rendered}"
    );
}

fn fixture_project(main: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/typ");
    for name in [main, "include/cells.ipynb"] {
        write(directory.path(), name, fs::read(fixtures.join(name)).unwrap());
    }
    write(directory.path(), "writer.toml", CONFIG);
    directory
}

#[test]
fn prescribed_notebook_selection_fixture_builds_native_pdf_html_and_docx_outputs_only()
 {
    let root = fixture_project("include-notebook.typ");
    let mut options =
        BuildOptions::new(root.path().join("include-notebook.typ"));
    options.config_path = Some(root.path().join("writer.toml"));
    let mut worlds = WorldSlot::new();
    let report = pipeline::build_targets(
        &options,
        &[Target::Pdf, Target::Html, Target::Docx],
        &mut worlds,
    );
    assert!(report.prepared.is_ok(), "{:?}", report.prepared);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(
        report.targets.iter().all(|target| target.written.is_ok()),
        "{:?}",
        report.targets
    );
    let native = visible(&compile(worlds.get(None, &options).unwrap()));
    assert!(
        native.contains("SELECTED-TEXT-OUTPUT")
            && native.contains("Selectedplot"),
        "{native}"
    );
    let html =
        fs::read_to_string(root.path().join("out/include-notebook.html"))
            .unwrap();
    assert!(html.contains("SELECTED-TEXT-OUTPUT") && html.contains("fig-plot"));
    assert!(!html.contains("__writerRenderMath"));
    let bytes =
        fs::read(root.path().join("out/include-notebook.docx")).unwrap();
    assert_eq!(validate(&bytes, &ValidateOptions { builtin_styles: true }), []);
    common_docx::read_back(&bytes);
    let parts = math_xml::zip_parts(&bytes);
    let doc = math_xml::parse(&math_xml::part(&parts, "word/document.xml"));
    let text = doc.text_content();
    assert!(
        text.contains("SELECTED-TEXT-OUTPUT") && text.contains("Selected plot")
    );
    assert!(!doc.descendants("wp:inline").is_empty());
    for forbidden in [
        "UNSELECTED-CODE-INPUT", "UNSELECTED-PLOT-INPUT",
        "Unselected Markdown math",
    ] {
        assert!(!native.contains(&forbidden.replace(' ', "")), "{native}");
        assert!(!html.contains(forbidden));
        assert!(!text.contains(forbidden));
    }
}

#[test]
fn prescribed_missing_selection_keeps_ir_outputs_and_fails_native_at_the_callsite()
 {
    let root = fixture_project("include-notebook-missing.typ");
    let mut options =
        BuildOptions::new(root.path().join("include-notebook-missing.typ"));
    options.config_path = Some(root.path().join("writer.toml"));
    let mut worlds = WorldSlot::new();
    let report = pipeline::build_targets(
        &options,
        &[Target::Pdf, Target::Html, Target::Docx],
        &mut worlds,
    );
    assert!(report.prepared.is_ok(), "{:?}", report.prepared);
    let result = |target| {
        &report
            .targets
            .iter()
            .find(|result| result.target == target)
            .unwrap()
            .written
    };
    assert!(result(Target::Pdf).is_err());
    assert!(result(Target::Html).is_ok(), "{:?}", report.targets);
    assert!(result(Target::Docx).is_ok(), "{:?}", report.targets);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.code == "notebook-cell-not-found")
    );
    let native = report
        .diagnostics
        .iter()
        .find(|finding| {
            finding.code == "typst-error" && finding.message.contains("99")
        })
        .unwrap();
    assert_eq!(
        fs::canonicalize(native.path.as_ref().unwrap()).unwrap(),
        fs::canonicalize(&options.input).unwrap()
    );
    assert_eq!(native.span.as_ref().unwrap().start_line, 3);
    let html = fs::read_to_string(
        root.path().join("out/include-notebook-missing.html"),
    )
    .unwrap();
    assert!(html.contains("Content after the missing selection."));
}

#[test]
fn prescribed_plain_includes_suppress_only_automatic_child_tocs() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/typ");
    let input = fixtures.join("includes.typ");
    let parsed = frontend::parse(
        &source(&input),
        &fs::read_to_string(&input).unwrap(),
        &config(),
    )
    .unwrap();
    let mut tocs = 0;
    ir::walk_blocks(&parsed.document, &mut |block| {
        tocs += usize::from(matches!(block, Block::Toc { .. }));
    });
    assert_eq!(tocs, 0);
    assert!(parsed.document.includes.contains(
        &fs::canonicalize(fixtures.join("include/part.md")).unwrap()
    ));
    assert!(parsed.document.includes.contains(
        &fs::canonicalize(fixtures.join("include/nb.ipynb")).unwrap()
    ));
    let provider = MarkdownIncludes { config: config() };
    for name in ["include/part.md", "include/nb.ipynb"] {
        let synthesized =
            provider.synthesize(&fixtures.join(name), &fixtures).unwrap();
        assert!(!synthesized.text.contains("#outline"), "{name}");
        assert!(!synthesized.text.contains("#show:"), "{name}");
    }
}

fn image_count(frame: &Frame) -> usize {
    frame
        .items()
        .map(|(_, item)| match item {
            FrameItem::Image(..) => 1,
            FrameItem::Group(group) => image_count(&group.frame),
            _ => 0,
        })
        .sum()
}

#[test]
fn notebook_module_assets_include_local_files_attachments_and_saved_byte_outputs()
 {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "file.svg", SVG);
    let attachment = base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("fixtures/md/images/plot.png"));
    let mut image_cell = md(
        "images",
        "![File](file.svg)\n\n![Attachment](attachment:plot.png)\n",
        &[],
    );
    image_cell["attachments"] = json!({"plot.png":{"image/png":attachment}});
    let bytes = notebook(json!([
        image_cell,
        code(
            "saved",
            "#| echo: false\npass",
            json!({"image/svg+xml":SVG}),
            &[]
        ),
    ]));
    write(&root, "media.ipynb", &bytes);
    let synthesized = MarkdownIncludes { config: config() }
        .synthesize(&root.join("media.ipynb"), &root)
        .unwrap();
    assert_eq!(synthesized.assets.len(), 3);
    assert_eq!(synthesized.files, [root.join("file.svg")]);
    let all_bytes = synthesized
        .assets
        .iter()
        .map(|asset| synthesized.assets.read(&asset.id).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        all_bytes
            .iter()
            .filter(|data| data.as_slice() == SVG.as_bytes())
            .count(),
        2
    );
    assert_eq!(
        all_bytes
            .iter()
            .filter(|data| data.as_slice()
                == include_bytes!("fixtures/md/images/plot.png"))
            .count(),
        1
    );
    write(&root, "main.typ", "#include \"media.ipynb\"\n");
    let mut world = world(&root, "main.typ");
    let document = compile(&mut world);
    assert_eq!(
        document
            .pages()
            .iter()
            .map(|page| image_count(&page.frame))
            .sum::<usize>(),
        3
    );
    assert_eq!(&*world.file(id("/media.ipynb")).unwrap(), bytes.as_bytes());
    assert_eq!(
        world.accessed(),
        BTreeSet::from([
            root.join("main.typ"),
            root.join("media.ipynb"),
            root.join("file.svg"),
        ])
    );
}

#[test]
fn native_missing_markdown_image_does_not_block_pdf_or_duplicate_ir_missing_findings()
 {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    write(&root, "main.typ", "#include \"part.md\"\n");
    write(
        &root,
        "part.md",
        "MISSING-IMAGE-MARKER.\n\n![Visible alt](not-there.png)\n",
    );
    write(&root, "writer.toml", CONFIG);
    let mut options = BuildOptions::new(root.join("main.typ"));
    options.config_path = Some(root.join("writer.toml"));
    for targets in [
        [Target::Html, Target::Pdf, Target::Docx],
        [Target::Pdf, Target::Docx, Target::Html],
    ] {
        let report =
            pipeline::build_targets(&options, &targets, &mut WorldSlot::new());
        assert!(report.prepared.is_ok(), "{:?}", report.prepared);
        for result in &report.targets {
            assert_eq!(
                result.written.is_ok(),
                result.target == Target::Pdf,
                "{result:?}"
            );
        }
        let missing = report
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "missing-asset")
            .collect::<Vec<_>>();
        assert_eq!(missing.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(missing[0].level, Level::Error);
        assert_eq!(missing[0].path.as_ref(), Some(&root.join("part.md")));
        assert_eq!(missing[0].span.as_ref().unwrap().start_line, 3);
        assert_eq!(
            missing[0].span.as_ref().unwrap().file.as_ref(),
            Some(&root.join("part.md"))
        );
        let pdf = fs::read(root.join("out/main.pdf")).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert_eq!(pipeline::pdf::page_count(&pdf), Some(1));
        assert!(!root.join("out/main.html").exists());
        assert!(!root.join("out/main.docx").exists());
    }
    let report = pipeline::build_targets(
        &options,
        &[Target::Pdf],
        &mut WorldSlot::new(),
    );
    assert!(report.targets[0].written.is_ok());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let missing = report
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "missing-asset")
        .collect::<Vec<_>>();
    assert_eq!(missing.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(missing[0].level, Level::Warning);
    assert_eq!(missing[0].path.as_ref(), Some(&root.join("part.md")));
}
