//! Screen furniture, print margin boxes, fixed copies and safe helper input.

use std::fs;
use std::path::Path;

use writer::assets::emit::OutputMode;
use writer::assets::registry::AssetRegistry;
use writer::html::design::{
    FurnitureSlot, FurnitureVariant, furniture_css, furniture_html,
};
use writer::html::styles::Targets;
use writer::html::{self, BuildInfo, RenderInput, RenderedDocument};
use writer::pipeline::{self, BuildOptions, Prepared};

fn render(
    prepared: &Prepared, opts: &BuildOptions,
) -> writer::Result<RenderedDocument> {
    let theme = pipeline::resolve_theme(prepared, opts)?;
    html::render(RenderInput {
        document: &prepared.document,
        assets: &mut prepared.assets.clone(),
        theme: &theme,
        config: &prepared.config,
        design: &prepared.design,
        output: OutputMode::Standalone,
        build: BuildInfo::build(true),
        math: &mut html::emit::KatexOnly::default(),
    })
}

fn stable_assets(text: &str) -> String {
    regex::Regex::new(r"asset:[0-9a-f]{12}")
        .unwrap()
        .replace_all(text, "asset:LOGO")
        .into_owned()
}

fn letter() -> (BuildOptions, Prepared) {
    let opts = BuildOptions::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/design/letter/letter.md"),
    );
    let prepared = pipeline::prepare(&opts).unwrap();
    (opts, prepared)
}

#[test]
fn the_letter_has_screen_furniture_and_distinct_print_headers() {
    let (opts, prepared) = letter();
    let mut registry = AssetRegistry::new();
    let mut targets = Targets::Placeholders(&mut registry);
    let header = furniture_html(
        &prepared.design,
        FurnitureSlot::Header,
        FurnitureVariant::First,
        &mut targets,
    )
    .unwrap();
    let footer = furniture_html(
        &prepared.design,
        FurnitureSlot::Footer,
        FurnitureVariant::First,
        &mut targets,
    )
    .unwrap();
    let css = furniture_css(&prepared.design, &mut targets).unwrap();
    insta::assert_snapshot!("letter_screen_header", stable_assets(&header));
    insta::assert_snapshot!("letter_screen_footer", stable_assets(&footer));
    insta::assert_snapshot!("letter_print_furniture", stable_assets(&css));
    assert!(header.contains("w-furniture w-header w-header-first"));
    assert!(header.contains("width:141.732pt"));
    assert!(css.contains(
        "@page :first{@top-center{content:\"Generative Biology Institute\""
    ));
    assert!(css.contains("padding-top:141.732pt"));
    assert!(css.contains("@page{@top-center{content:\"EIT Oxford | Generative Biology Institute\""));
    assert!(css.contains("border-top:0.709pt solid #a96d4b"));
    assert!(css.contains(
        "@bottom-right{content:\"+44 1865 649525\\A oxford@eit.org\""
    ));
    assert!(css.contains("@bottom-center{content:none}"));
    let output = render(&prepared, &opts).unwrap();
    let header = output.html.find("w-header-first").unwrap();
    let main = output.html.find("<main").unwrap();
    let footer = output.html.find("w-footer-first").unwrap();
    assert!(header < main && main < footer);
    assert!(!output.html.contains("asset:"));
    assert!(
        !output
            .html
            .contains(prepared.document.source.base_dir.to_str().unwrap())
    );
}

fn project(
    design: serde_json::Value,
) -> (tempfile::TempDir, BuildOptions, Prepared) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("document.md"), "# Letter\n\nBody.\n").unwrap();
    fs::write(
        root.path().join("writer.toml"),
        toml::to_string(&serde_json::json!({
            "design": design,
        }))
        .unwrap(),
    )
    .unwrap();
    let opts = BuildOptions::new(root.path().join("document.md"));
    let prepared = pipeline::prepare(&opts).unwrap();
    (root, opts, prepared)
}

#[test]
fn a_single_screen_column_spans_the_full_furniture_width() {
    let (_root, _opts, prepared) = project(serde_json::json!({
        "page": {"header": {"lines": ["Centered header"], "align": "center"}},
    }));
    let mut registry = AssetRegistry::new();
    let mut targets = Targets::Placeholders(&mut registry);
    let header = furniture_html(
        &prepared.design,
        FurnitureSlot::Header,
        FurnitureVariant::First,
        &mut targets,
    )
    .unwrap();
    let css = furniture_css(&prepared.design, &mut targets).unwrap();
    let selector = ".w-furniture>.w-furniture-lines:only-of-type";
    assert!(css.contains(&format!("{selector}{{grid-column:1/-1}}")));
    let markup = scraper::Html::parse_fragment(&header);
    let matched = scraper::Selector::parse(selector).unwrap();
    assert_eq!(markup.select(&matched).count(), 1);
    assert!(!header.contains("w-furniture-right"));
}

#[test]
fn image_only_svg_margin_boxes_reserve_the_logo_width_and_aspect_ratio() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("document.md"), "Body.\n").unwrap();
    fs::write(
        root.path().join("logo.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 20"><rect width="200" height="20"/></svg>"#,
    ).unwrap();
    fs::write(
        root.path().join("writer.toml"),
        concat!(
            "[design.page.header]\n",
            "image = { src = 'logo.svg', width = '20mm' }\n",
        ),
    )
    .unwrap();
    let opts = BuildOptions::new(root.path().join("document.md"));
    let prepared = pipeline::prepare(&opts).unwrap();
    let mut registry = AssetRegistry::new();
    let css = furniture_css(
        &prepared.design,
        &mut Targets::Placeholders(&mut registry),
    )
    .unwrap();
    assert!(css.contains("content:\"\""));
    assert!(css.contains("min-width:56.693pt"));
    assert!(css.contains("padding-top:5.669pt"));
    assert!(css.contains("/56.693pt auto"));
    let output = render(&prepared, &opts).unwrap();
    assert!(output.html.contains("data:image/svg+xml"));
    assert!(!output.html.contains("asset:"));
}

#[test]
fn furniture_text_is_escaped_and_page_placeholders_use_css_counters() {
    let (_root, opts, prepared) = project(serde_json::json!({
        "page": {"footer": {"lines": [
            "Page {page} of {pages}", "<b>literal</b> {date} \"quote\"",
            "</style><script id=furniture-injected>bad()</script>",
        ]}},
    }));
    let mut registry = AssetRegistry::new();
    let mut targets = Targets::Placeholders(&mut registry);
    let css = furniture_css(&prepared.design, &mut targets).unwrap();
    assert!(
        css.contains("content:\"Page \" counter(page) \" of \" counter(pages)")
    );
    let output = render(&prepared, &opts).unwrap();
    assert!(output.html.contains("Page 1 of 1"));
    assert!(output.html.contains("&lt;b&gt;literal&lt;/b&gt; {date}"));
    let dom = scraper::Html::parse_document(&output.html);
    let selector =
        scraper::Selector::parse(".w-furniture b, #furniture-injected")
            .unwrap();
    assert!(dom.select(&selector).next().is_none());
}

#[test]
fn an_empty_footer_suppresses_the_default_theme_page_number() {
    let (_root, opts, prepared) =
        project(serde_json::json!({"page":{"footer":{}}}));
    let mut registry = AssetRegistry::new();
    let css = furniture_css(
        &prepared.design,
        &mut Targets::Placeholders(&mut registry),
    )
    .unwrap();
    assert!(css.contains("@page{@bottom-left{content:none}@bottom-center{content:none}@bottom-right{content:none}}"));
    assert!(!css.contains("counter(page)"));
    let output = render(&prepared, &opts).unwrap();
    assert!(!output.html.contains("class=\"w-furniture"));
    let original = output.theme_css.find("content: counter(page)").unwrap();
    let override_number =
        output.theme_css.rfind("@bottom-center{content:none}").unwrap();
    assert!(override_number > original);
}

#[test]
fn fixed_mode_has_a_hidden_screen_copy_and_repeated_print_copy() {
    let (_root, opts, prepared) = project(serde_json::json!({
        "page": {
            "print": "fixed", "header_offset": "17mm", "footer_offset":"10mm",
            "header": {"first":{"lines":["First"]}, "rest":{"lines":["Rest"]}},
            "footer": {"lines": [
                {"text":"Legal", "weight":"bold"}, "Page {page}",
            ]},
        },
    }));
    let output = render(&prepared, &opts).unwrap();
    assert!(output.html.contains("w-header-first"));
    assert!(output.html.contains("w-header-rest\" data-print-copy"));
    assert!(output.html.contains("font-weight:700"));
    assert!(output.theme_css.contains("position:fixed;left:0;right:0"));
    assert!(output.theme_css.contains(".w-header-rest{top:48.189pt}"));
    assert!(output.theme_css.contains(".w-footer-rest{bottom:28.346pt}"));
    assert!(!output.theme_css.contains("content:\"First\""));
    assert!(output.html.contains("Page 1"));
    insta::assert_snapshot!(
        "fixed_furniture_css",
        output.theme_css.split("/* design: layout */").nth(1).unwrap()
    );
}

#[test]
fn bad_helper_arguments_are_actionable_template_errors() {
    for (slot, variant, expected) in [
        ("side", "first", "slot must be header or footer"),
        ("footer", "other", "variant must be first or rest"),
    ] {
        let (root, opts, mut prepared) = project(serde_json::json!({}));
        fs::create_dir_all(root.path().join("theme/templates")).unwrap();
        fs::write(
            root.path().join("theme/theme.toml"),
            "name='bad-helper'\nextends='default'\n",
        )
        .unwrap();
        fs::write(root.path().join("theme/templates/document.html"), format!(
            "{{{{ design_furniture('{slot}', '{variant}') }}}}<main data-writer-document></main>"
        )).unwrap();
        prepared.config.html.theme =
            root.path().join("theme").to_string_lossy().into_owned();
        let message = render(&prepared, &opts).err().unwrap().to_string();
        assert!(
            message.contains("design_furniture") && message.contains(expected),
            "{message}"
        );
    }
}

#[test]
fn an_undeclared_design_adds_no_furniture_markup_or_css() {
    let mut registry = AssetRegistry::new();
    let mut targets = Targets::Placeholders(&mut registry);
    let design = writer::design::Design::default();
    assert!(furniture_css(&design, &mut targets).unwrap().is_empty());
    for slot in [FurnitureSlot::Header, FurnitureSlot::Footer] {
        for variant in [FurnitureVariant::First, FurnitureVariant::Rest] {
            assert!(
                furniture_html(&design, slot, variant, &mut targets)
                    .unwrap()
                    .is_empty()
            );
        }
    }
    assert!(registry.is_empty());
}

#[cfg(all(feature = "browser-tests", unix))]
#[test]
fn chromium_prints_first_and_continuation_furniture_on_two_pages() {
    use std::time::Duration;
    use writer::pdf::devtools::{Page, find_chrome, readable_temp_dir};
    let Some(chrome) = find_chrome(None) else {
        assert!(
            std::env::var_os("WRITER_REQUIRE_CHROME").is_none(),
            "Chrome required"
        );
        eprintln!("skipping furniture print smoke: Chrome not installed");
        return;
    };
    let base = readable_temp_dir(&chrome)
        .expect("Chrome-readable temporary directory");
    let root = tempfile::tempdir_in(base).unwrap();
    let config = serde_json::json!({
        "design": {
            "text": {"size":"11pt"},
            "page": {
                "paper":"a4", "margin":"25mm", "header_offset":"5mm", "footer_offset":"5mm",
                "header": {
                    "first":{"image":{"src":"logo.png", "width":"5mm"}, "lines":["First page letterhead"]},
                    "rest":{"lines":["Continuation letterhead"]},
                },
                "footer":{"lines":["Legal footer", "Page {page} of {pages}"], "rule":true},
            },
        },
    });
    fs::write(
        root.path().join("writer.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    fs::write(
        root.path().join("document.md"),
        "Page one.\n\n<p style=\"break-before:page\">Page two.</p>\n",
    )
    .unwrap();
    let logo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/design/letter/theme/assets/logo.png");
    fs::copy(logo, root.path().join("logo.png")).unwrap();
    let opts = BuildOptions::new(root.path().join("document.md"));
    let prepared = pipeline::prepare(&opts).unwrap();
    let output = render(&prepared, &opts).unwrap();
    let html_path = root.path().join("printed.html");
    fs::write(&html_path, output.html).unwrap();
    let mut page = Page::launch(&chrome, Duration::from_secs(60)).unwrap();
    page.set_timeout(Duration::from_secs(30));
    let url = html_path
        .to_string_lossy()
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23");
    page.navigate(&format!("file://{url}")).unwrap();
    let ready = page.evaluate("Promise.all([document.fonts.ready, ...[...document.images].map(image => image.decode())]).then(() => true)").unwrap();
    assert_eq!(ready, true);
    let pdf = page.print_pdf().unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    let pages = regex::Regex::new(r"/Type\s*/Page\b")
        .unwrap()
        .find_iter(&String::from_utf8_lossy(&pdf))
        .count();
    assert_eq!(pages, 2, "the fixture should print exactly two pages");
    let pdf_path = root.path().join("printed.pdf");
    fs::write(&pdf_path, &pdf).unwrap();
    #[cfg(target_os = "macos")]
    {
        let script = root.path().join("inspect.swift");
        fs::write(
            &script,
            r#"import Foundation
import PDFKit
let document = PDFDocument(url: URL(fileURLWithPath: CommandLine.arguments[1]))!
let pages = (0..<document.pageCount).map { document.page(at: $0)?.string ?? "" }
let data = try! JSONSerialization.data(withJSONObject: pages)
print(String(data: data, encoding: .utf8)!)
"#,
        )
        .unwrap();
        match std::process::Command::new("swift")
            .env("SWIFT_MODULECACHE_PATH", "/tmp/writer-swift-cache")
            .arg(&script)
            .arg(&pdf_path)
            .output()
        {
            Ok(output) => {
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let text: Vec<String> =
                    serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(text.len(), 2);
                assert!(
                    text[0].contains("First page letterhead"),
                    "{}",
                    text[0]
                );
                assert!(!text[0].contains("Continuation letterhead"));
                assert!(
                    text[1].contains("Continuation letterhead"),
                    "{}",
                    text[1]
                );
                assert!(!text[1].contains("First page letterhead"));
                for (index, page) in text.iter().enumerate() {
                    assert!(page.contains("Legal footer"));
                    assert!(
                        page.contains(&format!("Page {} of 2", index + 1)),
                        "{page}"
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("Swift unavailable; PDF content inspection skipped");
            }
            Err(error) => panic!("could not inspect printed PDF: {error}"),
        }
    }
}
