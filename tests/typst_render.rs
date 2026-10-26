use std::collections::BTreeMap;
use std::path::Path;

use base64::Engine;
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Library, LibraryExt, World};
use writer::error::Error;
use writer::ir::MathDialect;
use writer::typst::compile::compile_document;
use writer::typst::html::{EQUATION_CSS, equation_mathml};
use writer::typst::render::{Rendition, equation, image};
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WorldOptions, WriterWorld};

const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="6"><rect width="10" height="6" fill="#123456"/></svg>"##;

fn world(root: &Path) -> WriterWorld {
    WriterWorld::new(WorldOptions {
        root: root.to_path_buf(),
        main: MainFile::Virtual,
        today: Datetime::from_ymd(2026, 10, 24).unwrap(),
        system_fonts: false,
        font_paths: Vec::new(),
        package_paths: Vec::new(),
        allow_packages: false,
        inputs: BTreeMap::new(),
    })
    .unwrap()
}

fn assert_pt(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 0.01,
        "{actual:.6} pt differs from {expected:.3} pt",
    );
}

fn assert_anchor(
    rendition: &Rendition, width: f64, height: f64, baseline: f64,
) {
    assert_pt(rendition.width_pt, width);
    assert_pt(rendition.height_pt, height);
    assert_pt(rendition.baseline_pt, baseline);
    assert_pt(rendition.descent_pt(), height - baseline);
    assert!(rendition.pdf.is_none());
}

#[test]
fn equation_inline_anchors() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world(directory.path());
    for (source, width, height, baseline) in [
        ("x^2 + 1/2", 31.317, 13.242, 9.447),
        ("a/b", 6.974, 11.602, 7.73),
        ("x", 6.292, 4.983, 4.862),
    ] {
        let rendition =
            equation(&mut world, source, MathDialect::Typst, false).unwrap();
        assert_anchor(&rendition, width, height, baseline);
        if source == "x^2 + 1/2" {
            assert_eq!(rendition.png_size(), (63, 26));
        }
    }
}

#[test]
fn equation_display_anchor() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world(directory.path());
    let rendition = equation(
        &mut world,
        "integral_0^1 x^2 dif x = 1/3",
        MathDialect::Typst,
        true,
    )
    .unwrap();
    assert_anchor(&rendition, 65.719, 29.159, 17.334);
}

#[test]
fn equation_latex_via_mitex() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world(directory.path());
    for display in [false, true] {
        let rendition =
            equation(&mut world, r"\frac{a}{b}", MathDialect::Latex, display)
                .unwrap();
        assert!(rendition.width_pt > 0.0);
        assert!(rendition.height_pt > rendition.baseline_pt);
    }
    let error = equation(&mut world, r"\foo{x}", MathDialect::Latex, false)
        .unwrap_err();
    assert!(matches!(error, Error::Compile { .. }), "{error:?}");
    assert!(world.accessed().is_empty());
}

#[test]
fn renditions_deterministic_self_contained() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world(directory.path());
    let first =
        equation(&mut world, "x^2 + 1/2", MathDialect::Typst, false).unwrap();
    let second =
        equation(&mut world, "x^2 + 1/2", MathDialect::Typst, false).unwrap();
    assert_eq!(first, second);
    let svg = std::str::from_utf8(&first.svg).unwrap();
    assert!(svg.contains("<path"));
    assert!(!svg.contains("<text"));
    assert!(!svg.contains("font"));
    assert!(first.png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(world.accessed().is_empty());
}

#[test]
fn image_renditions() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world(directory.path());
    let svg = image(&mut world, SVG, "image/svg+xml", Some(100.0)).unwrap();
    assert_pt(svg.width_pt, 100.0);
    assert_pt(svg.height_pt, 60.0);
    assert_eq!(svg.baseline_pt, svg.height_pt);
    assert_eq!(svg.descent_pt(), 0.0);
    assert_eq!(svg.png_size(), (200, 120));
    assert!(svg.pdf.as_ref().unwrap().starts_with(b"%PDF"));
    assert_eq!(
        svg,
        image(&mut world, SVG, "image/svg+xml", Some(100.0)).unwrap(),
    );

    let gif = base64::prelude::BASE64_STANDARD
        .decode("R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw==")
        .unwrap();
    let gif = image(&mut world, &gif, "image/gif", None).unwrap();
    assert!(gif.width_pt > 0.0);
    assert!(gif.pdf.unwrap().starts_with(b"%PDF"));

    let png = image(&mut world, &svg.png, "image/png", Some(100.0)).unwrap();
    assert_pt(png.width_pt, 100.0);
    assert_pt(png.height_pt, 60.0);
    let pdf = image(
        &mut world,
        svg.pdf.as_ref().unwrap(),
        "application/pdf",
        Some(100.0),
    )
    .unwrap();
    assert_pt(pdf.width_pt, 100.0);
    assert_pt(pdf.height_pt, 60.0);

    let error = image(&mut world, b"bmp", "image/bmp", None).unwrap_err();
    assert!(matches!(error, Error::Other(_)));
    assert!(world.accessed().is_empty());
}

fn assert_original_main(world: &mut WriterWorld, main: FileId, text: &str) {
    assert_eq!(world.main(), main);
    assert_eq!(world.source(main).unwrap().text(), text);
    let (document, _) = compile_document(world).into_result().unwrap();
    assert_pt(document.pages()[0].frame.width().to_pt(), 123.0);
    assert_pt(document.pages()[0].frame.height().to_pt(), 45.0);
}

#[test]
fn render_restores_main() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world(directory.path());
    let text = "#set page(width: 123pt, height: 45pt, margin: 0pt)\nOriginal";
    world.set_main_text(text, SourceMap::default());
    let main = world.main();
    let library = world.library() as *const _;

    equation(&mut world, "x", MathDialect::Typst, false).unwrap();
    assert_original_main(&mut world, main, text);
    equation(&mut world, "frac(", MathDialect::Typst, false).unwrap_err();
    assert_original_main(&mut world, main, text);
    image(&mut world, SVG, "image/svg+xml", None).unwrap();
    assert_original_main(&mut world, main, text);
    image(&mut world, b"invalid", "image/png", None).unwrap_err();
    assert_original_main(&mut world, main, text);

    equation_mathml(&mut world, "frac(a, b)", true).unwrap();
    assert_original_main(&mut world, main, text);
    equation_mathml(&mut world, "frac(", false).unwrap_err();
    assert_original_main(&mut world, main, text);
    equation_mathml(&mut world, "overline(x)", false).unwrap_err();
    assert_original_main(&mut world, main, text);
    assert!(std::ptr::eq(world.library(), library));
}

#[test]
fn render_restores_path_main() {
    let directory = tempfile::tempdir().unwrap();
    let text = "#set page(width: 123pt, height: 45pt, margin: 0pt)\nOriginal";
    let path = directory.path().join("document.typ");
    std::fs::write(&path, text).unwrap();
    let mut world = WriterWorld::new(WorldOptions {
        root: directory.path().to_path_buf(),
        main: MainFile::Path(path),
        today: Datetime::from_ymd(2026, 10, 24).unwrap(),
        system_fonts: false,
        font_paths: Vec::new(),
        package_paths: Vec::new(),
        allow_packages: false,
        inputs: BTreeMap::new(),
    })
    .unwrap();
    let main = world.main();
    equation(&mut world, "x", MathDialect::Typst, false).unwrap();
    assert_original_main(&mut world, main, text);
    equation_mathml(&mut world, "overline(x)", false).unwrap_err();
    assert_original_main(&mut world, main, text);
}

#[test]
fn equation_mathml_forms() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world(directory.path());
    assert_eq!(
        equation_mathml(&mut world, "frac(a, b)", true).unwrap(),
        "<math display=\"block\"><mfrac><mi>𝑎</mi><mi>𝑏</mi></mfrac></math>",
    );
    let inline = equation_mathml(&mut world, "x", false).unwrap();
    assert_eq!(inline, "<math><mi>𝑥</mi></math>");
    let error = equation_mathml(&mut world, "frac(", false).unwrap_err();
    assert!(matches!(error, Error::Compile { .. }));
}

#[test]
fn equation_mathml_warning_is_err() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world(directory.path());
    for source in ["overline(x)", "underline(x)", "cancel(x)"] {
        let error = equation_mathml(&mut world, source, false).unwrap_err();
        assert!(matches!(error, Error::Other(_)), "{error:?}");
        assert!(error.to_string().contains("was ignored during MathML export"));
    }
}

// Use the public compiler API with an HTML library to obtain the upstream CSS.
// The production World's temporary library switch deliberately stays private.
struct HtmlWorld {
    inner: WriterWorld,
    library: LazyHash<Library>,
}

impl World for HtmlWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.inner.book()
    }

    fn main(&self) -> FileId {
        self.inner.main()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.inner.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.inner.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.inner.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.inner.today(offset)
    }
}

#[test]
fn equation_css_matches_typst() {
    let directory = tempfile::tempdir().unwrap();
    let mut inner = world(directory.path());
    inner.set_main_text("$ x $", SourceMap::default());
    let world = HtmlWorld {
        inner,
        library: LazyHash::new(
            Library::builder()
                .with_features([Feature::Html].into_iter().collect())
                .build(),
        ),
    };
    let document =
        typst::compile::<typst_html::HtmlDocument>(&world).output.unwrap();
    let html =
        typst_html::html(&document, &typst_html::HtmlOptions { pretty: false })
            .unwrap();
    let start = html.find("<style>").unwrap() + "<style>".len();
    let end = start + html[start..].find("</style>").unwrap();
    let css = &html[start..end];
    assert_eq!(css.len(), 1494);
    assert_eq!(css, EQUATION_CSS);
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| insta::assert_snapshot!("typst_equation_css", css));
}
