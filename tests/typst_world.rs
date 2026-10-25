//! Compiler-world contracts: retained inputs, resources, and diagnostics.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use typst::World;
use typst::foundations::Datetime;
use typst::syntax::{FileId, RootedPath, VirtualPath, VirtualRoot};
use writer::assets::registry::AssetRegistry;
use writer::config::{Config, PdfConfig};
use writer::error::{Diagnostic, Error, Result};
use writer::ir::{DocMeta, MathDialect, SourceSpan};
use writer::typst::compile::{
    PdfOptions, compile_document, compile_pdf, oracle, pdf_timestamp,
};
use writer::typst::render::equation;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{
    IncludeProvider, MainFile, Synthesized, WorldOptions, WriterWorld,
    world_inputs,
};

fn today() -> Datetime {
    Datetime::from_ymd_hms(2026, 10, 24, 12, 30, 0).unwrap()
}

fn options(root: &Path, main: MainFile) -> WorldOptions {
    WorldOptions {
        root: root.into(),
        main,
        today: today(),
        system_fonts: false,
        font_paths: Vec::new(),
        package_paths: Vec::new(),
        allow_packages: false,
        inputs: world_inputs(&Config::default()),
    }
}

fn world(root: &Path) -> WriterWorld {
    WriterWorld::new(options(root, MainFile::Virtual)).unwrap()
}

fn id(path: &str) -> FileId {
    RootedPath::new(VirtualRoot::Project, VirtualPath::new(path).unwrap())
        .intern()
}

fn set_text(world: &mut WriterWorld, text: &str) {
    world.set_main_text(text, SourceMap::default());
}

fn png(world: &mut WriterWorld) -> Vec<u8> {
    equation(world, "x", MathDialect::Typst, false).unwrap().png
}

#[test]
fn virtual_main_edits() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = world(dir.path());
    let main = world.main();
    set_text(&mut world, "First");
    let first = compile_document(&mut world).into_result().unwrap().0;
    assert_eq!(first.pages().len(), 1);
    world.reset();
    set_text(&mut world, "First\n#pagebreak()\nSecond");
    let second = compile_document(&mut world).into_result().unwrap().0;
    assert_eq!(second.pages().len(), 2);
    assert_eq!(world.main(), main);
    assert!(world.accessed().is_empty());
    assert_eq!(world.today(None), Datetime::from_ymd(2026, 10, 24));
}

#[test]
fn typ_main_reset_and_accessed() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let mut world_for_image = world(&root);
    fs::write(root.join("fig.png"), png(&mut world_for_image)).unwrap();
    fs::write(root.join("part.typ"), "Part one").unwrap();
    fs::write(
        root.join("main.typ"),
        "#include \"part.typ\"\n#image(\"fig.png\")",
    )
    .unwrap();
    let mut world = WriterWorld::new(options(
        dir.path(),
        MainFile::Path(dir.path().join("main.typ")),
    ))
    .unwrap();
    let first = compile_document(&mut world).into_result().unwrap().0;
    assert_eq!(first.pages().len(), 1);
    let paths = BTreeSet::from([
        root.join("main.typ"),
        root.join("part.typ"),
        root.join("fig.png"),
    ]);
    assert_eq!(world.accessed(), paths);
    world.reset();
    assert!(world.accessed().is_empty());
    fs::write(root.join("part.typ"), "Part one\n#pagebreak()\nPart two")
        .unwrap();
    let second = compile_document(&mut world).into_result().unwrap().0;
    assert_eq!(second.pages().len(), 2);
    assert_eq!(world.accessed(), paths);
}

#[test]
fn sandbox_rejects_parent() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("root")).unwrap();
    fs::write(dir.path().join("x.txt"), "outside").unwrap();
    let mut world = world(&dir.path().join("root"));
    set_text(&mut world, "#read(\"../x.txt\")");
    let error = compile_document(&mut world).into_result().unwrap_err();
    assert!(error.to_string().contains("escape the project root"));
    assert!(world.accessed().is_empty());
}

#[test]
fn mounts_and_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mounted = tempfile::tempdir().unwrap();
    let mut world = world(dir.path());
    let image = mounted.path().join("fig.png");
    let text = mounted.path().join("value.txt");
    fs::write(&image, png(&mut world)).unwrap();
    fs::write(&text, "mounted").unwrap();
    let mut assets = AssetRegistry::new();
    let asset = assets.register_file(&image);
    world.set_assets(assets);
    world.mount("__writer/template/", mounted.path());
    set_text(
        &mut world,
        &format!(
            "#read(\"/__writer/template/value.txt\")\n\
         #image(\"/__assets/{asset}.png\")"
        ),
    );
    compile_document(&mut world).into_result().unwrap();
    assert_eq!(
        world.accessed(),
        BTreeSet::from([
            fs::canonicalize(image).unwrap(),
            fs::canonicalize(text).unwrap(),
        ])
    );
    let replacement = tempfile::tempdir().unwrap();
    fs::write(replacement.path().join("value.txt"), "replaced").unwrap();
    world.mount("/__writer/template", replacement.path());
    assert_eq!(
        &*world.file(id("/__writer/template/value.txt")).unwrap(),
        b"replaced"
    );
    world.set_assets(AssetRegistry::new());
    assert!(world.file(id(&format!("/__assets/{asset}.png"))).is_err());
}

struct FakeInclude {
    calls: Arc<AtomicUsize>,
    image: PathBuf,
    dependency: PathBuf,
    root: PathBuf,
}

impl IncludeProvider for FakeInclude {
    fn synthesize(&self, path: &Path, root: &Path) -> Result<Synthesized> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(root, self.root);
        assert_eq!(path, self.root.join("part.md"));
        let mut assets = AssetRegistry::new();
        let image = assets.register_file(&self.image);
        Ok(Synthesized {
            text: format!("Included\n#image(\"/__assets/{image}.png\")"),
            map: SourceMap::from_lines(vec![Some(SourceSpan {
                start_line: 12,
                end_line: 12,
                cell: Some(2),
                file: Some(path.into()),
            })]),
            assets,
            files: vec![self.dependency.clone()],
            diagnostics: [Diagnostic::warning("include-test", "from include")]
                .into_iter()
                .collect(),
        })
    }
}

#[test]
fn include_provider_synthesizes() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let mut world = world(&root);
    let image = root.join("image.png");
    let dependency = root.join("nested.md");
    fs::write(&image, png(&mut world)).unwrap();
    fs::write(&dependency, "Nested").unwrap();
    fs::write(root.join("part.md"), "original bytes").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    world.set_include_provider(Box::new(FakeInclude {
        calls: calls.clone(),
        image: image.clone(),
        dependency: dependency.clone(),
        root: root.clone(),
    }));
    set_text(&mut world, "#include \"part.md\"\n#include \"part.md\"");
    compile_document(&mut world).into_result().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(world.include_diagnostics().len(), 1);
    assert_eq!(
        world.include_diagnostics().iter().next().unwrap().code,
        "include-test"
    );
    let map = world.source_map(id("/part.md")).unwrap();
    assert_eq!(map.lookup(1).unwrap().start_line, 12);
    assert_eq!(map.lookup(1).unwrap().cell, Some(2));
    assert_eq!(world.real_path(id("/part.md")), Some(root.join("part.md")));
    assert_eq!(&*world.file(id("/part.md")).unwrap(), b"original bytes");
    assert_eq!(
        world.accessed(),
        BTreeSet::from([root.join("part.md"), image.clone(), dependency,])
    );
    world.reset();
    assert!(world.accessed().is_empty());
    assert!(world.include_diagnostics().is_empty());
    compile_document(&mut world).into_result().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(world.accessed().contains(&image));
}

struct FailingInclude(Arc<AtomicUsize>);

impl IncludeProvider for FailingInclude {
    fn synthesize(&self, _: &Path, _: &Path) -> Result<Synthesized> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(Error::Other("include provider failed".into()))
    }
}

#[test]
fn include_provider_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = world(dir.path());
    let calls = Arc::new(AtomicUsize::new(0));
    world.set_include_provider(Box::new(FailingInclude(calls.clone())));
    set_text(&mut world, "#include \"part.md\"");
    let error = compile_document(&mut world).into_result().unwrap_err();
    assert!(error.to_string().contains("include provider failed"));
    assert!(world.source(id("/part.md")).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    world.reset();
    assert!(world.source(id("/part.md")).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn package_paths_root() {
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("packages/local/hello/0.1.0");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("typst.toml"),
        concat!(
            "[package]\nname = 'hello'\nversion = '0.1.0'\n",
            "entrypoint = 'lib.typ'\n"
        ),
    )
    .unwrap();
    fs::write(package.join("lib.typ"), "#let greeting = [Hello]").unwrap();
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.typst.package_paths = vec!["packages".into()];
    let mut world = WriterWorld::from_config(
        &config,
        dir.path(),
        MainFile::Virtual,
        today(),
    )
    .unwrap();
    set_text(&mut world, "#import \"@local/hello:0.1.0\": greeting\n#greeting");
    compile_document(&mut world).into_result().unwrap();
    assert_eq!(
        world.accessed(),
        BTreeSet::from([
            fs::canonicalize(package.join("typst.toml")).unwrap(),
            fs::canonicalize(package.join("lib.typ")).unwrap(),
        ])
    );
}

#[test]
fn preview_without_download() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = world(dir.path());
    set_text(
        &mut world,
        "#import \"@preview/writer-no-such-package:0.0.1\": *",
    );
    let error = compile_document(&mut world).into_result().unwrap_err();
    let Error::Compile { hints, .. } = error else {
        panic!("expected compile error")
    };
    assert!(hints.iter().any(|hint| hint
        == concat!(
            "writer downloads @preview packages only with ",
            "`[pdf] allow_packages = true` or `--allow-packages`"
        )));
}

#[test]
fn writer_doc_other_version() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = world(dir.path());
    set_text(&mut world, "#import \"@writer/doc:9.9.9\": *");
    let error = compile_document(&mut world).into_result().unwrap_err();
    assert!(error.to_string().contains("latest is 0.1.0"));
}

#[test]
fn writer_doc_embedded() {
    if writer::vendor::get("typst/writer-doc/typst.toml").is_none() {
        eprintln!("skipping: @writer/doc is introduced by T31");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut world = world(dir.path());
    set_text(&mut world, "#import \"@writer/doc:0.1.0\": *");
    compile_document(&mut world).into_result().unwrap();
    assert!(world.accessed().is_empty());
}

#[test]
fn font_paths_are_scanned() {
    let dir = tempfile::tempdir().unwrap();
    let original = world(dir.path());
    let embedded_count = original
        .book()
        .families()
        .map(|(_, fonts)| fonts.count())
        .sum::<usize>();
    let fonts = dir.path().join("fonts");
    fs::create_dir(&fonts).unwrap();
    fs::write(fonts.join("extra.otf"), original.font(0).unwrap().data())
        .unwrap();
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.pdf.font_paths = vec!["fonts".into()];
    let scanned = WriterWorld::from_config(
        &config,
        dir.path(),
        MainFile::Virtual,
        today(),
    )
    .unwrap();
    let total = scanned
        .book()
        .families()
        .map(|(_, fonts)| fonts.count())
        .sum::<usize>();
    assert!(total > embedded_count);
    assert_eq!(scanned.book().info(0), original.book().info(0));
}

#[test]
fn diagnostics_map_through_source_map() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = world(dir.path());
    world.set_main_text(
        "Scaffolding\n\n#unknown()",
        SourceMap::from_lines(vec![
            None,
            None,
            Some(SourceSpan {
                start_line: 12,
                end_line: 12,
                cell: None,
                file: None,
            }),
        ]),
    );
    let error = compile_document(&mut world).into_result().unwrap_err();
    assert!(matches!(error, Error::Compile { line: Some(12), .. }));
}

#[test]
fn package_trace_located_at_call() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("main.typ"),
        concat!("#import \"@preview/mitex:0.2.7\": mi\n", "#mi(\"\\foo{x}\")"),
    )
    .unwrap();
    let mut world = WriterWorld::new(options(
        dir.path(),
        MainFile::Path("main.typ".into()),
    ))
    .unwrap();
    let error = compile_document(&mut world).into_result().unwrap_err();
    let Error::Compile { path, line, hints, .. } = error else {
        panic!("expected compile error")
    };
    assert_eq!(path, fs::canonicalize(dir.path().join("main.typ")).unwrap());
    assert_eq!(line, Some(2));
    assert!(
        hints.iter().any(|hint| hint.starts_with("in @preview/mitex:0.2.7/"))
    );
}

#[test]
fn pdf_is_reproducible() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = world(dir.path());
    set_text(&mut world, "= Reproducible\nHello, world.");
    let options = PdfOptions { timestamp: today(), ident: Some("test".into()) };
    let first = compile_pdf(&mut world, &options).into_result().unwrap().0;
    world.reset();
    let second = compile_pdf(&mut world, &options).into_result().unwrap().0;
    assert_eq!(first, second);
    assert_eq!(first.pages, 1);
    assert!(
        first
            .bytes
            .windows(b"writer 0.1.0".len())
            .any(|bytes| bytes == b"writer 0.1.0")
    );
}

#[test]
fn oracle_query() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = world(dir.path());
    let mut inputs = world_inputs(&Config::default());
    inputs.insert("writer-oracle".into(), "1".into());
    world.set_inputs(inputs);
    set_text(
        &mut world,
        concat!(
            "#metadata((kind: \"figure\", label: \"fig-a\", number: \"1\")) ",
            "<writer-oracle-fig-a>\n",
            "#metadata((kind: \"heading\", label: \"sec-a\", number: none)) ",
            "<writer-oracle-sec-a>"
        ),
    );
    let document = compile_document(&mut world).into_result().unwrap().0;
    let figure = oracle(&document, "fig-a").unwrap();
    assert_eq!(figure.kind, "figure");
    assert_eq!(figure.number.as_deref(), Some("1"));
    assert_eq!(oracle(&document, "sec-a").unwrap().number, None);
    assert!(oracle(&document, "missing").is_none());
}

#[test]
fn pdf_timestamp_policy() {
    let mut pdf = PdfConfig::default();
    let mut meta = DocMeta::default();
    let now = UNIX_EPOCH + Duration::from_secs(1_700_000_001);
    pdf.timestamp = "2026-10-24T12:30:00+02:00".into();
    assert_eq!(
        pdf_timestamp(&pdf, &meta, None, now).unwrap(),
        Datetime::from_ymd_hms(2026, 10, 24, 10, 30, 0).unwrap()
    );
    pdf.timestamp = "not a timestamp".into();
    assert!(matches!(
        pdf_timestamp(&pdf, &meta, None, now),
        Err(Error::Config(_))
    ));
    pdf.timestamp.clear();
    meta.date = Some("2026-10-24".into());
    assert_eq!(
        pdf_timestamp(&pdf, &meta, Some("1700000000"), now).unwrap(),
        Datetime::from_ymd_hms(2026, 10, 24, 0, 0, 0).unwrap()
    );
    meta.date = Some("2026-02-30".into());
    assert_eq!(
        pdf_timestamp(&pdf, &meta, Some("1700000000"), now).unwrap(),
        Datetime::from_ymd_hms(2023, 11, 14, 22, 13, 20).unwrap()
    );
    meta.date = None;
    assert!(matches!(
        pdf_timestamp(&pdf, &meta, Some("invalid"), now),
        Err(Error::Config(_))
    ));
    assert_eq!(
        pdf_timestamp(&pdf, &meta, None, now).unwrap(),
        Datetime::from_ymd_hms(2023, 11, 14, 22, 13, 21).unwrap()
    );
}
