//! Physical Typst export trees remain usable after their input project is gone.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use typst::World;
use typst::foundations::Datetime;
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;
use writer::config::Config;
use writer::error::Diagnostics;
use writer::ir::{Attrs, Block, Inline};
use writer::pipeline::{self, BuildOptions, Prepared};
use writer::typst::compile::{compile_document, oracle};
use writer::typst::package;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld, world_inputs};

const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#526d82"/></svg>"##;
const CONFIG: &str = "[build]\nout_dir='out'\ntargets=['typst']\n[pdf]\nsystem_fonts=false\nallow_packages=false\ntimestamp='2026-10-29T00:00:00Z'\n";
const TEMPLATE: &str = concat!(
    "#import \"@writer/doc:0.1.0\" as base\n",
    "#let writer-doc(..args, body) = {\n",
    "  [TEMPLATE-MARKER]\n",
    "  base.writer-doc(..args, body)\n",
    "}\n",
);

fn write(root: &Path, name: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn project(source: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "document.md", source);
    write(root.path(), "writer.toml", CONFIG);
    root
}

fn prepare(root: &Path, name: &str) -> (BuildOptions, Prepared) {
    let mut options = BuildOptions::new(root.join(name));
    options.config_path = Some(root.join("writer.toml"));
    let prepared = pipeline::prepare(&options).unwrap();
    assert!(!prepared.diagnostics.has_errors(), "{:?}", prepared.diagnostics);
    (options, prepared)
}

fn world(root: &Path, main: MainFile) -> WriterWorld {
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.pdf.allow_packages = false;
    config.typst.package_paths = vec![root.join("packages")];
    WriterWorld::from_config(
        &config,
        root,
        main,
        Datetime::from_ymd(2026, 10, 29).unwrap(),
    )
    .unwrap()
}

fn export(
    options: &BuildOptions, prepared: &Prepared, output: &Path,
) -> (Vec<PathBuf>, Diagnostics) {
    let original = prepared.clone();
    let mut world =
        world(&prepared.document.source.base_dir, MainFile::Virtual);
    world.set_main_text("Retained main sentinel", SourceMap::default());
    let main = world.main();
    let source = world.source(main).unwrap();
    let accessed = world.accessed();
    let result =
        pipeline::typst::build(prepared, options, output, &mut world).unwrap();
    assert_eq!(world.main(), main);
    assert_eq!(world.source(main).unwrap().text(), source.text());
    assert_eq!(
        world.accessed(),
        accessed,
        "export must not compile or reset the session"
    );
    assert_eq!(prepared.document, original.document);
    assert_eq!(prepared.assets, original.assets);
    assert_eq!(prepared.config, original.config);
    assert!(!result.1.has_errors(), "{:?}", result.1);
    result
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
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    files
}

fn frame_contents(frame: &Frame, text: &mut String, images: &mut usize) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                frame_contents(&group.frame, text, images)
            }
            FrameItem::Text(run) => text.push_str(&run.text),
            FrameItem::Image(..) => *images += 1,
            _ => {}
        }
    }
}

fn contents(document: &PagedDocument) -> (String, usize) {
    let mut text = String::new();
    let mut images = 0;
    for page in document.pages() {
        frame_contents(&page.frame, &mut text, &mut images);
    }
    (
        text.chars().filter(|character| !character.is_whitespace()).collect(),
        images,
    )
}

fn compile_tree(root: &Path, main: &str) -> PagedDocument {
    package::eject(&root.join("packages")).unwrap();
    assert!(root.join("packages/writer/doc/0.1.0/typst.toml").is_file());
    assert!(root.join("packages/preview/mitex/0.2.7/typst.toml").is_file());
    let mut world = world(root, MainFile::Path(root.join(main)));
    let mut inputs = world_inputs(&Config::default());
    inputs.insert("writer-oracle".into(), "1".into());
    world.set_inputs(inputs);
    // Deliberately no mounts, assets, IncludeProvider or virtual main source.
    let compiled = compile_document(&mut world);
    assert!(compiled.output.is_some(), "{:?}", compiled.diagnostics);
    let (document, diagnostics) = compiled.into_result().unwrap();
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    let canonical = fs::canonicalize(root).unwrap();
    assert!(
        world.accessed().iter().all(|path| path.starts_with(&canonical)),
        "non-exported file read: {:?}",
        world.accessed()
    );
    document
}

fn assert_portable_sources(root: &Path) {
    for (path, bytes) in inventory(root) {
        if path.extension().is_some_and(|extension| extension == "typ") {
            let source = String::from_utf8(bytes).unwrap();
            assert!(source.ends_with('\n'), "{}", path.display());
            assert!(!source.contains('\r'), "{}", path.display());
            for forbidden in [
                "/__assets/", "/__writer/", "asset:",
                "#include \"document.md\"",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{}: {forbidden}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn nested_include_note_asset_and_forward_reference_compile_after_relocation() {
    let source = project(concat!(
        "# Root\n\nRoot marker. See @fig-nested.\n\n",
        ":::include chapters/part-a.md\n:::\n\nRoot tail.\n",
    ));
    write(
        source.path(),
        "chapters/part-a.md",
        concat!(
            "Part A marker with a note.[^one]\n\n",
            ":::include part-b.md\n:::\n\n",
            "[^one]: Scoped note marker.\n",
        ),
    );
    write(
        source.path(),
        "chapters/part-b.md",
        "Part B marker.\n\n![Nested picture](../image.svg){#fig-nested}\n",
    );
    write(source.path(), "image.svg", SVG);
    let destination = tempfile::tempdir().unwrap();
    let export_root = destination.path().join("exported");
    let output = export_root.join("renamed.final.typ");
    let (options, prepared) = prepare(source.path(), "document.md");
    let (written, _) = export(&options, &prepared, &output);
    assert_eq!(
        written,
        [
            output.clone(),
            export_root.join("renamed.final_part-a.typ"),
            export_root.join("renamed.final_assets")
        ]
    );
    let main = fs::read_to_string(&output).unwrap();
    let part = fs::read_to_string(&written[1]).unwrap();
    assert!(main.contains("#include \"renamed.final_part-a.typ\""));
    assert!(!main.contains("Part A marker") && !main.contains("Part B marker"));
    assert!(part.contains("Part A marker") && part.contains("Part B marker"));
    assert!(!part.contains("#show:"));
    assert_eq!(main.matches("#show:").count(), 1);
    assert_portable_sources(&export_root);
    let before = inventory(&export_root);
    export(&options, &prepared, &output);
    assert_eq!(inventory(&export_root), before);
    source.close().unwrap();
    let moved = destination.path().join("moved");
    fs::rename(&export_root, &moved).unwrap();
    let compiled = compile_tree(&moved, "renamed.final.typ");
    let (text, images) = contents(&compiled);
    for expected in [
        "Rootmarker", "PartAmarker", "PartBmarker", "Scopednotemarker",
        "Roottail",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(images > 0);
    assert_eq!(
        oracle(&compiled, "fig-nested").unwrap().number.as_deref(),
        Some("1")
    );
}

#[test]
fn duplicate_include_stems_are_siblings_and_adjacent_same_file_runs_coalesce() {
    let source = project(concat!(
        ":::include first/part.md\n:::\n\nBetween one.\n\n",
        ":::include second/part.md\n:::\n\nBetween two.\n\n",
        ":::include first/part.md\n:::\n\n:::include first/part.md\n:::\n",
    ));
    write(source.path(), "first/part.md", "FIRST-MARKER\n");
    write(source.path(), "second/part.md", "SECOND-MARKER\n");
    let destination = tempfile::tempdir().unwrap();
    let main = destination.path().join("out.typ");
    let (options, prepared) = prepare(source.path(), "document.md");
    let (written, _) = export(&options, &prepared, &main);
    assert_eq!(
        written,
        [
            main,
            destination.path().join("out_part.typ"),
            destination.path().join("out_part-2.typ"),
            destination.path().join("out_part-3.typ")
        ]
    );
    assert_eq!(
        fs::read_to_string(&written[3])
            .unwrap()
            .matches("FIRST-MARKER")
            .count(),
        2
    );
    source.close().unwrap();
    let (text, _) = contents(&compile_tree(destination.path(), "out.typ"));
    assert_eq!(text.matches("FIRST-MARKER").count(), 3);
    assert_eq!(text.matches("SECOND-MARKER").count(), 1);
}

#[test]
fn notebook_scanning_honors_markdown_raw_markdown_removed_cells_and_non_source_text()
 {
    let source = project("unused\n");
    write(source.path(), "parts/a.md", "MD-INCLUDE-ONLY\n");
    write(source.path(), "parts/raw.md", "RAW-INCLUDE-ONLY\n");
    let notebook = json!({"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":[
        {"cell_type":"markdown","metadata":{},"source":"Notebook root.\n\n:::include parts/a.md\n:::\n"},
        {"cell_type":"raw","metadata":{"raw_mimetype":"text/markdown"},"source":"Raw root.\n\n:::include parts/raw.md\n:::\n"},
        {"cell_type":"markdown","metadata":{"tags":["remove-cell"]},"source":":::include missing-removed.md\n:::\n"},
        {"cell_type":"raw","metadata":{"raw_mimetype":"text/html"},"source":":::include missing-html.md\n:::\n"},
        {"cell_type":"code","metadata":{},"source":":::include missing-code.md\n:::\n","execution_count":1,
         "outputs":[{"output_type":"display_data","metadata":{},"data":{"text/plain":":::include missing-output.md\n:::\n"}}]}
    ]});
    write(source.path(), "analysis.ipynb", notebook.to_string());
    let destination = tempfile::tempdir().unwrap();
    let output = destination.path().join("analysis.typ");
    let (options, prepared) = prepare(source.path(), "analysis.ipynb");
    let (written, _) = export(&options, &prepared, &output);
    assert!(written.contains(&destination.path().join("analysis_a.typ")));
    assert!(written.contains(&destination.path().join("analysis_raw.typ")));
    assert_eq!(
        written
            .iter()
            .filter(|path| path
                .extension()
                .is_some_and(|extension| extension == "typ"))
            .count(),
        3
    );
    let main = fs::read_to_string(&output).unwrap();
    assert!(!main.contains("MD-INCLUDE-ONLY"));
    assert!(!main.contains("RAW-INCLUDE-ONLY"));
    source.close().unwrap();
    let (text, _) = contents(&compile_tree(destination.path(), "analysis.typ"));
    assert_eq!(text.matches("MD-INCLUDE-ONLY").count(), 1, "{text}");
    assert_eq!(text.matches("RAW-INCLUDE-ONLY").count(), 1, "{text}");
}

fn svg(seed: usize) -> Vec<u8> {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><!--{seed}--><rect width="24" height="12" fill="#526d82"/></svg>"##).into_bytes()
}

fn image(id: &str) -> Block {
    Block::paragraph(vec![Inline::Image {
        url: format!("asset:{id}"),
        alt: vec![Inline::text("Exported SVG")],
        title: None,
        attrs: Attrs::default(),
    }])
}

#[test]
fn referenced_asset_id_order_resolves_adversarial_suffix_collisions_and_ignores_unused_assets()
 {
    let source = project("# Asset allocation\n");
    let (options, mut prepared) = prepare(source.path(), "document.md");
    let mut candidates = (0..8)
        .map(|seed| {
            let bytes = svg(seed);
            let id = blake3::hash(&bytes).to_hex()[..12].to_owned();
            (id, bytes)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    let references = &candidates[..3];
    let unused = (8..10_000)
        .find_map(|seed| {
            let bytes = svg(seed);
            let id = blake3::hash(&bytes).to_hex()[..12].to_owned();
            (id < references[0].0).then_some((id, bytes))
        })
        .expect(
            "find a deterministic unused asset sorting before referenced IDs",
        );
    prepared.assets.register_bytes(
        unused.1.clone(),
        "image/svg+xml",
        "plot.svg",
    );
    let reserved = format!("plot-{}.svg", &references[2].0[..6]);
    for (index, (expected_id, bytes)) in references.iter().enumerate() {
        let id = prepared.assets.register_bytes(
            bytes.clone(),
            "image/svg+xml",
            if index == 0 { &reserved } else { "plot.svg" },
        );
        assert_eq!(&id, expected_id);
    }
    // Reverse source order to distinguish it from deterministic asset-ID order.
    prepared
        .document
        .blocks
        .extend(references.iter().rev().map(|(id, _)| image(id)));
    let destination = tempfile::tempdir().unwrap();
    let output = destination.path().join("allocation.typ");
    let (_, _) = export(&options, &prepared, &output);
    let assets = inventory(&destination.path().join("allocation_assets"));
    assert_eq!(assets.len(), 3);
    assert_eq!(assets[Path::new(&reserved)], references[0].1);
    assert_eq!(assets[Path::new("plot.svg")], references[1].1);
    assert_eq!(
        assets[Path::new(&format!("plot-{}-2.svg", &references[2].0[..6]))],
        references[2].1
    );
    assert!(!assets.values().any(|bytes| bytes == &unused.1));
    source.close().unwrap();
    let (_, images) =
        contents(&compile_tree(destination.path(), "allocation.typ"));
    assert_eq!(images, 3);
}

const CSL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text">
<info><title>Portable test</title><id>https://example.org/portable-test</id><updated>2026-10-29T00:00:00+00:00</updated></info>
<citation><layout prefix="(" suffix=")"><text variable="citation-number"/></layout></citation>
<bibliography><layout><text variable="title"/></layout></bibliography>
</style>"#;

#[test]
fn bibliography_csl_template_and_attachment_collisions_compile_from_exact_copies()
 {
    let source = project(concat!(
        "---\ntitle: PORTABLE-TITLE\n---\n\n# Resources\n\n",
        "[@alpha; @beta].\n\n```{=typst}\n#cite(<alpha>) #cite(<beta>)\n```\n",
    ));
    let first =
        b"@book{alpha, author={Writer, A.}, title={ALPHA-BOOK}, year={2026}}\n";
    let second =
        b"@book{beta, author={Writer, B.}, title={BETA-BOOK}, year={2026}}\n";
    write(source.path(), "first/refs.bib", first);
    write(source.path(), "second/refs.bib", second);
    write(source.path(), "styles/local.csl", CSL);
    write(source.path(), "templates/template.typ", TEMPLATE);
    write(
        source.path(),
        "writer.toml",
        format!(
            "{CONFIG}template='templates/template.typ'\n[references]\nbibliography=['first/refs.bib','second/refs.bib']\nstyle='styles/local.csl'\n"
        ),
    );
    let (options, mut prepared) = prepare(source.path(), "document.md");
    assert!(prepared.document.features.citations);
    assert_eq!(
        prepared.document.resolved.bibliography.as_ref().unwrap().entries.len(),
        2
    );
    assert_eq!(
        prepared
            .document
            .blocks
            .iter()
            .filter(|block| matches!(block, Block::Bibliography { .. }))
            .count(),
        1
    );
    let attachment = prepared.assets.register_bytes(
        b"attachment bytes".to_vec(),
        "application/octet-stream",
        "refs.bib",
    );
    prepared.document.blocks.push(Block::paragraph(vec![Inline::Link {
        url: format!("asset:{attachment}"),
        title: None,
        inlines: vec![Inline::text("Download attachment")],
        attrs: Attrs::default(),
    }]));
    let destination = tempfile::tempdir().unwrap();
    let output = destination.path().join("book.typ");
    let (written, _) = export(&options, &prepared, &output);
    assert_eq!(
        written,
        [output.clone(), destination.path().join("book_assets")]
    );
    let assets = inventory(&destination.path().join("book_assets"));
    assert_eq!(assets.len(), 5);
    let names = assets
        .keys()
        .map(|path| path.to_str().unwrap().to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    assert_eq!(names.len(), 5);
    assert_eq!(assets[Path::new("refs.bib")], b"attachment bytes");
    let assigned = |bytes: &[u8]| {
        let matching = assets
            .iter()
            .filter(|(_, value)| value.as_slice() == bytes)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        format!("book_assets/{}", matching[0].0.to_str().unwrap())
    };
    let first_path = assigned(first);
    let second_path = assigned(second);
    let csl_path = assigned(CSL.as_bytes());
    let template_path = assigned(TEMPLATE.as_bytes());
    let main = fs::read_to_string(&output).unwrap();
    for path in [&first_path, &second_path, &csl_path, &template_path] {
        assert!(main.contains(path), "missing {path}: {main}");
        assert!(!Path::new(path).is_absolute());
    }
    assert!(main.find(&first_path).unwrap() < main.find(&second_path).unwrap());
    assert_portable_sources(destination.path());
    // T32's mounted path must use the same BYO function and metadata before
    // the original project is removed. This is a real generated PDF build.
    let mut pdf_world = world(source.path(), MainFile::Virtual);
    let pdf_path = destination.path().join("template.pdf");
    writer::pipeline::pdf::build(
        &prepared, &options, &pdf_path, &mut pdf_world,
    )
    .unwrap();
    assert!(fs::read(&pdf_path).unwrap().starts_with(b"%PDF-"));
    let (pdf_document, _) =
        compile_document(&mut pdf_world).into_result().unwrap();
    let (pdf_text, _) = contents(&pdf_document);
    assert!(
        pdf_text.contains("TEMPLATE-MARKER")
            && pdf_text.contains("PORTABLE-TITLE")
    );
    source.close().unwrap();
    let compiled = compile_tree(destination.path(), "book.typ");
    let (text, _) = contents(&compiled);
    for marker in
        ["TEMPLATE-MARKER", "PORTABLE-TITLE", "ALPHA-BOOK", "BETA-BOOK"]
    {
        assert!(text.contains(marker), "missing {marker}: {text}");
    }
}

#[test]
fn canonical_repeated_resources_share_one_copy_and_named_styles_are_not_files()
{
    let source = project("# Copy once\n");
    let bibliography = b"@book{a,author={Writer},title={Shared},year={2026}}\n";
    write(source.path(), "refs.bib", bibliography);
    write(
        source.path(),
        "writer.toml",
        format!(
            "{CONFIG}[references]\nbibliography=['refs.bib','./refs.bib']\nstyle='ieee'\n"
        ),
    );
    // This public prepared-input contract isolates the resource-copy planner.
    // Real prepare intentionally rejects repeated nonempty bibliography inputs.
    let mut options = BuildOptions::new(source.path().join("document.md"));
    options.config_path = Some(source.path().join("writer.toml"));
    let prepared = pipeline::parse_only(&options).unwrap();
    assert!(!prepared.diagnostics.has_errors());

    let destination = tempfile::tempdir().unwrap();
    let output = destination.path().join("once.typ");
    let (written, _) = export(&options, &prepared, &output);
    assert_eq!(written, [output, destination.path().join("once_assets")]);
    let files = inventory(&destination.path().join("once_assets"));
    assert_eq!(files.len(), 1);
    assert_eq!(files[Path::new("refs.bib")], bibliography);
}

#[test]
fn resource_read_or_destination_clash_failure_preserves_the_complete_existing_tree()
 {
    let source = project("# Preflight\n");
    write(
        source.path(),
        "writer.toml",
        format!("{CONFIG}[references]\nbibliography=['missing/refs.bib']\n"),
    );
    write(
        source.path(),
        "missing/refs.bib",
        b"@book{saved,title={Saved},year={2026}}\n",
    );
    let (options, prepared) = prepare(source.path(), "document.md");
    fs::remove_file(source.path().join("missing/refs.bib")).unwrap();
    let destination = tempfile::tempdir().unwrap();
    write(destination.path(), "sentinel.typ", b"previous generated source");
    let output = destination.path().join("new/nested.typ");
    let before = inventory(destination.path());
    let mut world = world(source.path(), MainFile::Virtual);
    assert!(
        pipeline::typst::build(&prepared, &options, &output, &mut world)
            .is_err()
    );
    assert_eq!(inventory(destination.path()), before);
    assert!(!destination.path().join("new").exists());
    write(source.path(), "writer.toml", CONFIG);
    let (options, prepared) = prepare(source.path(), "document.md");
    let input = source.path().join("document.md");
    let before = inventory(source.path());
    assert!(
        pipeline::typst::build(&prepared, &options, &input, &mut world)
            .is_err()
    );
    assert_eq!(inventory(source.path()), before);
}

#[test]
fn existing_figures_fixture_exports_file_assets_and_compiles_from_disk() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md");
    let source =
        project(&fs::read_to_string(fixtures.join("figures.md")).unwrap());
    for name in ["plot.png", "diagram.svg"] {
        write(
            source.path(),
            &format!("images/{name}"),
            fs::read(fixtures.join("images").join(name)).unwrap(),
        );
    }
    let (options, prepared) = prepare(source.path(), "document.md");
    let destination = tempfile::tempdir().unwrap();
    let output = destination.path().join("figures.typ");
    export(&options, &prepared, &output);
    let copied = inventory(&destination.path().join("figures_assets"));
    assert_eq!(copied.len(), 2);
    for name in ["plot.png", "diagram.svg"] {
        assert_eq!(
            copied[Path::new(name)],
            fs::read(fixtures.join("images").join(name)).unwrap()
        );
    }
    source.close().unwrap();
    let document = compile_tree(destination.path(), "figures.typ");
    let (_, images) = contents(&document);
    assert!(images >= 2);
    assert!(oracle(&document, "fig-panels").is_some());
}

#[derive(Debug, PartialEq, Eq)]
enum TreeEntry {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

// Do not follow symlinks: a snapshot must retain aliases and empty directories,
// and must never recurse into an output directory that links to its parent.
fn tree_snapshot(root: &Path) -> BTreeMap<PathBuf, TreeEntry> {
    let mut tree = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).unwrap();
        let entry = if metadata.file_type().is_symlink() {
            TreeEntry::Symlink(fs::read_link(&path).unwrap())
        } else if metadata.is_dir() {
            for child in fs::read_dir(&path).unwrap() {
                pending.push(child.unwrap().path());
            }
            TreeEntry::Directory
        } else {
            assert!(metadata.is_file(), "{}", path.display());
            TreeEntry::File(fs::read(&path).unwrap())
        };
        tree.insert(path.strip_prefix(root).unwrap().to_owned(), entry);
    }
    tree
}

struct SidecarCase {
    document: &'static str,
    input: &'static str,
    bytes: &'static [u8],
    config: &'static str,
    output: &'static str,
    role: pipeline::InputRole,
}

fn sidecar_cases() -> [SidecarCase; 5] {
    use pipeline::InputRole;

    [
        SidecarCase {
            document: "Root.\n\n:::include inputs/chapter.md\n:::\n",
            input: "inputs/chapter.md",
            bytes: b"Included chapter.\n",
            config: "",
            output: "report_chapter.typ",
            role: InputRole::Include,
        },
        SidecarCase {
            document: "![Protected image](inputs/image.svg)\n",
            input: "inputs/image.svg",
            bytes: SVG,
            config: "",
            output: "report_assets/image.svg",
            role: InputRole::Asset,
        },
        SidecarCase {
            document: "Bibliography resource.\n",
            input: "inputs/refs.bib",
            bytes:
                b"@book{book, title={Protected bibliography}, year={2026}}\n",
            config: "[references]\nbibliography=['inputs/refs.bib']\n",
            output: "report_assets/refs.bib",
            role: InputRole::Bibliography,
        },
        SidecarCase {
            document: "Style resource.\n",
            input: "inputs/local.csl",
            bytes: CSL.as_bytes(),
            config: "[references]\nstyle='inputs/local.csl'\n",
            output: "report_assets/local.csl",
            role: InputRole::Bibliography,
        },
        SidecarCase {
            document: "Template resource.\n",
            input: "inputs/template.typ",
            bytes: TEMPLATE.as_bytes(),
            config: "template='inputs/template.typ'\n",
            output: "report_assets/template.typ",
            role: InputRole::PdfTemplate,
        },
    ]
}

#[cfg(unix)]
fn assert_sidecar_aliases(make_alias: impl Fn(&Path, &Path)) {
    for case in sidecar_cases() {
        // One temporary root keeps hard links on the same filesystem.
        let sandbox = tempfile::tempdir().unwrap();
        let source = sandbox.path().join("source");
        let destination = sandbox.path().join("export");
        write(&source, "document.md", case.document);
        write(&source, case.input, case.bytes);
        write(&source, "writer.toml", format!("{CONFIG}{}", case.config));
        write(&destination, "report.typ", b"previous main output");
        write(&destination, "unrelated/note.txt", b"keep unrelated bytes");
        fs::create_dir_all(destination.join("unrelated/empty")).unwrap();
        let sidecar = destination.join(case.output);
        fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
        make_alias(&source.join(case.input), &sidecar);
        let (options, prepared) = prepare(&source, "document.md");
        let before = tree_snapshot(sandbox.path());
        let mut world = world(&source, MainFile::Virtual);
        let error = pipeline::typst::build(
            &prepared,
            &options,
            &destination.join("report.typ"),
            &mut world,
        )
        .unwrap_err();
        let clash = pipeline::refused_clash(&error)
            .unwrap_or_else(|| panic!("{}: {error}", case.output));
        assert_eq!(clash.output, sidecar);
        assert_eq!(clash.role, case.role);
        assert!(pipeline::same_file(&clash.input, &source.join(case.input)));
        assert_eq!(tree_snapshot(sandbox.path()), before, "{}", case.output);
    }
}

#[cfg(unix)]
#[test]
fn real_build_rejects_symlinked_sidecars_before_replacing_existing_main() {
    assert_sidecar_aliases(|input, output| {
        std::os::unix::fs::symlink(input, output).unwrap();
    });
}

// The shared safety API documents inode-based hard-link identity on Unix.
// Direct-path cases below exercise the portable guard on Windows as well.
#[cfg(unix)]
#[test]
fn real_build_rejects_hardlinked_sidecars_before_replacing_existing_main() {
    assert_sidecar_aliases(|input, output| {
        fs::hard_link(input, output).unwrap();
    });
}

#[test]
fn real_build_rejects_resources_already_at_their_output_paths_on_every_os() {
    for case in sidecar_cases() {
        if case.role == pipeline::InputRole::Include {
            continue;
        }
        let document = case.document.replace("inputs/", "report_assets/");
        let source = project(&document);
        let input = case.input.replace("inputs/", "report_assets/");
        let config = case.config.replace("inputs/", "report_assets/");
        write(source.path(), &input, case.bytes);
        write(source.path(), "writer.toml", format!("{CONFIG}{config}"));
        write(source.path(), "report.typ", b"previous main output");
        fs::create_dir_all(source.path().join("keep/empty")).unwrap();
        let (options, prepared) = prepare(source.path(), "document.md");
        let before = tree_snapshot(source.path());
        let mut world = world(source.path(), MainFile::Virtual);
        let error = pipeline::typst::build(
            &prepared,
            &options,
            &source.path().join("report.typ"),
            &mut world,
        )
        .unwrap_err();
        let clash = pipeline::refused_clash(&error)
            .unwrap_or_else(|| panic!("{input}: {error}"));
        assert_eq!(clash.output, source.path().join(case.output));
        assert_eq!(clash.role, case.role);
        assert_eq!(tree_snapshot(source.path()), before, "{input}");
    }
}

#[test]
fn missing_parent_dotdot_alias_is_refused_without_creating_a_directory() {
    let source = project("Protected main source.\n");
    let (options, prepared) = prepare(source.path(), "document.md");
    let output = source.path().join("new/../document.md");
    let before = tree_snapshot(source.path());
    let mut world = world(source.path(), MainFile::Virtual);
    let error =
        pipeline::typst::build(&prepared, &options, &output, &mut world)
            .unwrap_err();
    let clash = pipeline::refused_clash(&error).unwrap();
    assert_eq!(clash.role, pipeline::InputRole::Source);
    assert_eq!(tree_snapshot(source.path()), before);
    assert!(!source.path().join("new").exists());
}

fn copy_original_include_inputs(root: &Path) {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md");
    for name in [
        "include/part-a.md", "include/part-b.md", "include/cycle-a.md",
        "include/cycle-b.md", "images/plot.png",
    ] {
        write(root, name, fs::read(fixtures.join(name)).unwrap());
    }
}

#[test]
fn original_include_fixture_derivative_keeps_nested_note_asset_and_compiles() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md");
    let original = fs::read_to_string(fixtures.join("includes.md")).unwrap();
    let deliberate_errors = concat!(
        "\n:::include include/cycle-a.md\n:::\n\n",
        "- A list item with a forbidden nested include:\n\n",
        "  :::include include/part-a.md\n  :::\n",
    );
    // Preserve every other byte of the actual root fixture, and copy every
    // included fragment unchanged. The original is tested separately below.
    let positive = original.strip_suffix(deliberate_errors).unwrap();
    let source = project(positive);
    copy_original_include_inputs(source.path());
    let (options, prepared) = prepare(source.path(), "document.md");
    let destination = tempfile::tempdir().unwrap();
    let export_root = destination.path().join("original-location");
    let output = export_root.join("includes.typ");
    let (written, _) = export(&options, &prepared, &output);
    assert_eq!(
        written,
        [
            output.clone(),
            export_root.join("includes_part-a.typ"),
            export_root.join("includes_assets"),
        ]
    );
    let main = fs::read_to_string(&output).unwrap();
    let part = fs::read_to_string(&written[1]).unwrap();
    assert!(main.contains("#include \"includes_part-a.typ\""));
    assert!(!main.contains("Part A keeps its own"));
    assert!(part.contains("Part A keeps its own file-scoped note"));
    assert!(part.contains("This note belongs to part-a.md"));
    assert!(!part.contains("#show:"));
    let assets = inventory(&export_root.join("includes_assets"));
    assert_eq!(assets.len(), 1);
    assert_eq!(
        assets[Path::new("plot.png")],
        fs::read(fixtures.join("images/plot.png")).unwrap(),
    );
    assert_portable_sources(&export_root);
    source.close().unwrap();
    let moved = destination.path().join("relocated");
    fs::rename(&export_root, &moved).unwrap();
    let document = compile_tree(&moved, "includes.typ");
    let (text, images) = contents(&document);
    assert!(text.contains("Includeddocuments"), "{text}");
    assert!(text.contains("PartAkeepsitsownfile-scopednote"), "{text}");
    assert!(text.contains("Thisnotebelongstopart-a.md"), "{text}");
    assert!(text.contains("PartB"), "{text}");
    assert_eq!(images, 1);
    assert_eq!(
        oracle(&document, "fig-part-b").unwrap().number.as_deref(),
        Some("1"),
    );
}

#[test]
fn unchanged_original_include_fixture_fails_without_an_export_tree() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md");
    let original = fs::read(fixtures.join("includes.md")).unwrap();
    let source = project(std::str::from_utf8(&original).unwrap());
    copy_original_include_inputs(source.path());
    let mut options = BuildOptions::new(source.path().join("document.md"));
    options.config_path = Some(source.path().join("writer.toml"));
    options.cli.out = Some(source.path().join("new/export/includes.typ"));
    let before = tree_snapshot(source.path());
    let report = pipeline::build_targets(
        &options,
        &[writer::config::Target::Typst],
        &mut pipeline::WorldSlot::new(),
    );
    let failure = report.prepared.unwrap_err();
    assert!(failure.error.to_string().contains("include cycle"), "{failure:?}");
    assert!(report.targets.is_empty());
    assert_eq!(fs::read(source.path().join("document.md")).unwrap(), original);
    assert_eq!(tree_snapshot(source.path()), before);
    assert!(!source.path().join("new").exists());
}
