//! T15 document templates, helper paths and assets in all three output modes.
//! Helpers and their edge cases are lifted from presenter_rs `tests/render.rs`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::json;
use writer::assets::emit::{OutputMode, content_version, data_uri};
use writer::assets::registry::AssetRegistry;
use writer::config::{Config, MathmlFont};
use writer::error::{Diagnostics, Error};
use writer::html::emit::{KatexOnly, MathHook, MathRequest, MathUsage};
use writer::html::{
    self, BuildInfo, BuildMode, RenderInput, RenderedDocument, context_json,
};
use writer::ir::Document;
use writer::theme::{self, Theme};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sample() -> (Document, AssetRegistry) {
    let mut registry = AssetRegistry::new();
    let bytes =
        std::fs::read(root().join("tests/fixtures/md/images/a.png")).unwrap();
    let id = registry.register_bytes(bytes, "image/png", "ok.png");
    let text = std::fs::read_to_string(
        root().join("tests/fixtures/ir/sample-document.json"),
    )
    .unwrap();
    let text = text.replace("32debefb8003", &id);
    (serde_json::from_str(&text).unwrap(), registry)
}

fn child_theme() -> Theme {
    theme::resolve(
        root().join("tests/fixtures/themes/child-theme").to_str().unwrap(),
        &[],
    )
    .unwrap()
}

fn default_theme() -> Theme {
    theme::resolve("default", &[]).unwrap()
}

fn site() -> OutputMode {
    OutputMode::Site {
        assets_dir: "out/assets".into(),
        url_prefix: "assets/".into(),
    }
}

fn build_info(output: &OutputMode) -> BuildInfo {
    match output {
        OutputMode::Standalone => BuildInfo::build(true),
        OutputMode::Site { .. } => BuildInfo::build(false),
        OutputMode::Serve => BuildInfo::serve(7),
    }
}

fn try_render_with(
    document: &Document, theme: &Theme, config: &Config, output: OutputMode,
) -> writer::Result<RenderedDocument> {
    let (_, mut assets) = sample();
    let mut math = KatexOnly::default();
    let build = build_info(&output);
    html::render(RenderInput {
        document,
        assets: &mut assets,
        theme,
        config,
        output,
        build,
        math: &mut math,
    })
}

fn render_with(
    document: &Document, theme: &Theme, config: &Config, output: OutputMode,
) -> RenderedDocument {
    try_render_with(document, theme, config, output).unwrap()
}

fn render_sample(theme: &Theme, output: OutputMode) -> RenderedDocument {
    render_with(&sample().0, theme, &Config::default(), output)
}

fn render_error(theme: &Theme, output: OutputMode) -> Error {
    match try_render_with(&sample().0, theme, &Config::default(), output) {
        Ok(_) => panic!("render should fail"),
        Err(error) => error,
    }
}

fn disk_theme(dir: &Path, manifest: &str, files: &[(&str, &[u8])]) -> Theme {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("theme.toml"), manifest).unwrap();
    for (rel, bytes) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    theme::resolve(dir.to_str().unwrap(), &[]).unwrap()
}

fn fixture_copy() -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &to.join(entry.file_name()));
            } else {
                std::fs::copy(entry.path(), to.join(entry.file_name()))
                    .unwrap();
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    copy(&root().join("tests/fixtures/themes/child-theme"), dir.path());
    dir
}

fn copied_theme(dir: &tempfile::TempDir) -> Theme {
    theme::resolve(dir.path().to_str().unwrap(), &[]).unwrap()
}

fn body_end(dir: &tempfile::TempDir, text: &str) {
    std::fs::write(
        dir.path().join("templates/document.html"),
        format!(
            "{{% extends '@default/document.html' %}}\n\
         {{% block body_end %}}{text}{{% endblock %}}",
        ),
    )
    .unwrap();
}

fn versioned(path: &str, bytes: &[u8]) -> String {
    format!("/__writer/asset/{path}?v={}", content_version(bytes))
}

fn network_references(html: &str) -> BTreeSet<String> {
    let reference = Regex::new(
        r#"(?i)(?:\b(?:src|href|poster|srcset|action)\s*=\s*["']?|url\(\s*["']?|@import\s+["'])\s*((?:https?:)?//[^"'\s)>]*)"#,
    ).unwrap();
    reference.captures_iter(html).map(|caps| caps[1].to_owned()).collect()
}

#[test]
fn context_has_the_spec_shape() {
    let (document, mut assets) = sample();
    let theme = child_theme();
    let config = Config::default();
    let mut math = KatexOnly::default();
    let ctx = context_json(&mut RenderInput {
        document: &document,
        assets: &mut assets,
        theme: &theme,
        config: &config,
        output: OutputMode::Standalone,
        build: BuildInfo::build(true),
        math: &mut math,
    })
    .unwrap();
    let keys: Vec<&str> =
        ctx.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "document", "meta", "features", "config", "html", "math", "title",
            "lang", "theme", "build",
        ]
    );
    assert_eq!(ctx["document"], serde_json::to_value(&document).unwrap());
    assert_eq!(ctx["meta"], serde_json::to_value(&document.meta).unwrap());
    assert_eq!(ctx["config"], serde_json::to_value(&config).unwrap());
    assert_eq!(
        ctx["theme"],
        json!({
            "name": "child-theme", "chain": ["child-theme", "default"],
            "code_theme": "Nord", "code_theme_dark": "OneHalfDark",
            "vars": {"accent": "#aa3300"},
            "scripts": ["@child-theme/scripts/child.js"],
        })
    );
    assert_eq!(
        ctx["html"].as_object().unwrap().keys().collect::<Vec<_>>(),
        ["main", "title_block", "body", "footnotes", "toc"]
    );
    assert!(
        ctx["html"]["main"]
            .as_str()
            .unwrap()
            .starts_with("<main data-writer-document>")
    );
    assert!(ctx["html"]["main"].as_str().unwrap().contains("asset:"));
    assert_eq!(
        ctx["math"],
        json!({
            "katex": true, "mathml": false, "typst_mathml": false, "image": false,
        })
    );
    insta::assert_json_snapshot!("context", ctx, {
        ".build.version" => "[VERSION]",
    });
    assert_eq!(BuildInfo::serve(4).mode, BuildMode::Serve);
}

#[test]
fn context_title_and_language_fallbacks_and_source_positions() {
    let (mut document, mut assets) = sample();
    document.meta.title = None;
    document.meta.lang = None;
    let theme = default_theme();
    let mut config = Config::default();
    config.document.lang = "de".into();
    config.markdown.sourcepos = true;
    let mut math = KatexOnly::default();
    let ctx = context_json(&mut RenderInput {
        document: &document,
        assets: &mut assets,
        theme: &theme,
        config: &config,
        output: OutputMode::Standalone,
        build: BuildInfo::build(true),
        math: &mut math,
    })
    .unwrap();
    assert_eq!(
        ctx["title"],
        document.source.path.file_stem().unwrap().to_str().unwrap()
    );
    assert_eq!(ctx["lang"], "de");
    assert!(ctx["html"]["main"].as_str().unwrap().contains("data-sourcepos"));
}

#[test]
fn helpers_in_three_modes() {
    let dir = fixture_copy();
    std::fs::write(
        dir.path().join("styles/helper.css"),
        ".helper { background: url(../assets/logo.svg); }\n",
    )
    .unwrap();
    body_end(
        &dir,
        r#"
<div id="helper-style">{{ style('@child-theme/styles/helper.css') }}</div>
<div id="helper-script">{{ script('@child-theme/scripts/child.js') }}</div>
<img id="helper-asset" src="{{ asset('logo.svg') }}">
<div id="helper-theme">{{ theme_style() }}</div>
<div id="helper-live">{{ live_reload() }}</div>
"#,
    );
    let theme = copied_theme(&dir);
    let logo_url = Regex::new(r"url\(logo-[0-9a-f]{12}\.svg\)").unwrap();
    for mode in [OutputMode::Standalone, site(), OutputMode::Serve] {
        let doc = render_sample(&theme, mode.clone());
        match mode {
            OutputMode::Standalone => {
                assert!(doc.html.contains("<style>\n.helper { background: url(data:image/svg+xml;base64,"));
                assert!(doc.html.contains("<script>\nwindow.writerChild"));
                assert!(doc.html.contains(
                    "<img id=\"helper-asset\" src=\"data:image/svg+xml;base64,"
                ));
                assert!(doc.html.contains(
                    "<div id=\"helper-theme\"><style id=\"writer-theme\">"
                ));
                assert!(doc.html.contains("<div id=\"helper-live\"></div>"));
            }
            OutputMode::Site { .. } => {
                for name in ["helper", "child", "logo", "theme"] {
                    let pattern = format!(r"assets/{name}-[0-9a-f]{{12}}\.");
                    assert!(Regex::new(&pattern).unwrap().is_match(&doc.html));
                }
                assert!(doc.html.contains("<div id=\"helper-live\"></div>"));
                assert_eq!(
                    doc.files
                        .iter()
                        .filter(|file| file
                            .path
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .starts_with("theme-"))
                        .count(),
                    1
                );
                let css = doc
                    .files
                    .iter()
                    .find(|file| {
                        file.path
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .starts_with("helper-")
                    })
                    .unwrap();
                let css = std::str::from_utf8(&css.bytes).unwrap();
                assert!(logo_url.is_match(css));
            }
            OutputMode::Serve => {
                assert!(doc.html.contains("<link rel=\"stylesheet\" href=\"/__writer/asset/@child-theme/styles/helper.css\">"));
                assert!(doc.html.contains("<script src=\"/__writer/asset/@child-theme/scripts/child.js\"></script>"));
                let logo =
                    std::fs::read(dir.path().join("assets/logo.svg")).unwrap();
                assert!(doc.html.contains(&format!(
                    "<img id=\"helper-asset\" src=\"{}\">",
                    versioned("logo.svg", &logo),
                )));
                assert!(doc.html.contains("<div id=\"helper-theme\"><link id=\"writer-theme\" rel=\"stylesheet\" href=\"/__writer/theme.css?v=7\">"));
                assert!(doc.html.contains("<div id=\"helper-live\"><script src=\"/__writer/live-reload.js\"></script></div>"));
            }
        }
    }
}

#[test]
fn standalone_output_is_self_contained() {
    let doc = render_sample(&child_theme(), OutputMode::Standalone);
    assert!(!doc.html.contains("asset:"));
    assert!(!doc.html.contains("/__writer/"));
    assert!(doc.html.contains("<meta name=\"x-child\" content=\"#aa3300\">"));
    assert!(doc.html.contains("<title>GPU allocation</title>"));
    assert!(
        doc.html.contains("<meta name=\"author\" content=\"J. Blaschke\">")
    );
    assert!(!doc.html.contains("name=\"generator\""));
    assert!(doc.files.is_empty());
    // Author-written links may still use the network; no helper adds a request.
    assert_eq!(
        network_references(&doc.html),
        BTreeSet::from(["https://example.com".into()])
    );
}

#[test]
fn site_output_writes_hashed_files() {
    let doc = render_sample(&child_theme(), site());
    let names: Vec<&str> = doc
        .files
        .iter()
        .map(|file| file.path.file_name().unwrap().to_str().unwrap())
        .collect();
    for prefix in ["theme-", "logo-", "child-", "ok-"] {
        assert!(names.iter().any(|name| name.starts_with(prefix)), "{names:?}");
    }
    assert!(!doc.html.contains("asset:"));
    assert!(
        Regex::new(r"assets/theme-[0-9a-f]{12}\.css")
            .unwrap()
            .is_match(&doc.html)
    );
    assert!(
        Regex::new(r"url\(logo-[0-9a-f]{12}\.svg\)")
            .unwrap()
            .is_match(&doc.theme_css)
    );
    assert!(doc.files.iter().all(|file| file.path.starts_with("out/assets")));
}

#[test]
fn serve_output_uses_dev_server_urls() {
    let doc = render_sample(&child_theme(), OutputMode::Serve);
    assert!(doc.html.contains("<link id=\"writer-theme\" rel=\"stylesheet\" href=\"/__writer/theme.css?v=7\">"));
    assert!(doc.html.contains("data-sourcepos=\"3-3\" data-source=\"0\""));
    assert!(
        doc.html.contains("<script src=\"/__writer/live-reload.js\"></script>")
    );
    assert!(doc.html.contains("/__writer/doc-asset/"));
    assert!(
        doc.theme_css
            .contains("/__writer/asset/@child-theme/assets/logo.svg?v=")
    );
    assert!(!doc.theme_css.contains("doc-asset"));
    assert!(doc.files.is_empty());
}

#[test]
fn code_css_only_with_features_code() {
    let (mut document, _) = sample();
    let theme = child_theme();
    document.features.code = false;
    let doc = render_with(
        &document,
        &theme,
        &Config::default(),
        OutputMode::Standalone,
    );
    assert!(!doc.theme_css.contains("/* code theme:"));
    assert!(!doc.theme_css.contains("/* code theme (dark):"));
    document.features.code = true;
    let doc = render_with(
        &document,
        &theme,
        &Config::default(),
        OutputMode::Standalone,
    );
    assert!(doc.theme_css.contains("/* code theme: Nord */\n@media not all and (prefers-color-scheme: dark) {\n"));
    assert!(doc.theme_css.contains("/* code theme (dark): OneHalfDark */\n@media (prefers-color-scheme: dark) {\n"));
}

struct FakeMath(MathUsage);

impl MathHook for FakeMath {
    fn equation(
        &mut self, _eq: &MathRequest<'_>, _assets: &mut AssetRegistry,
        _diagnostics: &mut Diagnostics,
    ) -> String {
        "<math></math>".into()
    }

    fn usage(&self) -> MathUsage {
        self.0
    }
}

fn render_math(
    usage: MathUsage, font: MathmlFont, output: OutputMode,
) -> RenderedDocument {
    let (document, mut assets) = sample();
    let theme = default_theme();
    let mut config = Config::default();
    config.math.mathml_font = font;
    config
        .math
        .katex_options
        .insert("macros".into(), json!({"\\x": "</script>"}));
    let mut math = FakeMath(usage);
    let build = build_info(&output);
    html::render(RenderInput {
        document: &document,
        assets: &mut assets,
        theme: &theme,
        config: &config,
        output,
        build,
        math: &mut math,
    })
    .unwrap()
}

#[test]
fn math_init_follows_math_usage() {
    let doc = render_math(
        MathUsage::default(),
        MathmlFont::Embed,
        OutputMode::Standalone,
    );
    assert!(!doc.html.contains("writer-math"));
    assert!(!doc.html.contains("katex.min"));
    assert!(!doc.html.contains("__writerKatexOptions"));
    let usage = MathUsage { katex: true, ..MathUsage::default() };
    let doc = render_math(usage, MathmlFont::Embed, OutputMode::Standalone);
    assert!(doc.html.contains(
        r#"window.__writerKatexOptions = {"macros":{"\\x":"\u003c/script>"}};"#
    ));
    assert!(doc.html.contains("window.__writerRenderMath"));
    assert!(!doc.html.contains("writer-math"));
    let doc = render_math(usage, MathmlFont::Embed, OutputMode::Serve);
    assert!(doc.html.contains("/__writer/asset/vendor/katex/katex.min.css"));
    assert!(doc.html.contains("/__writer/asset/vendor/katex/katex.min.js"));
    assert!(doc.html.contains("/__writer/asset/writer/math.js"));
    let usage = MathUsage { mathml: true, ..MathUsage::default() };
    let doc = render_math(usage, MathmlFont::Embed, OutputMode::Standalone);
    assert!(doc.html.contains("<style id=\"writer-math\">\n@font-face"));
    assert!(doc.html.contains("data:font/woff2;base64,"));
    assert!(!doc.html.contains("__writerKatexOptions"));
    let doc = render_math(usage, MathmlFont::System, OutputMode::Standalone);
    assert!(!doc.html.contains("@font-face"));
    assert!(doc.html.contains("\"Latin Modern Math\", \"STIX Two Math\""));
    let usage = MathUsage { typst_mathml: true, ..MathUsage::default() };
    let doc = render_math(usage, MathmlFont::System, OutputMode::Standalone);
    assert!(doc.html.contains(writer::typst::html::EQUATION_CSS));
    assert!(!doc.html.contains("__writerKatexOptions"));
    let usage = MathUsage { image: true, ..MathUsage::default() };
    let doc = render_math(usage, MathmlFont::Embed, OutputMode::Standalone);
    assert!(!doc.html.contains("writer-math"));
    assert!(!doc.html.contains("__writerKatexOptions"));
}

#[test]
fn stylesheet_imports_and_entries() {
    for bad_css in ["@import 'local.css';", ".x { background: url(nope.svg); }"]
    {
        let dir = fixture_copy();
        std::fs::write(dir.path().join("styles/child.css"), bad_css).unwrap();
        for output in [OutputMode::Standalone, site(), OutputMode::Serve] {
            assert!(matches!(
                render_error(&copied_theme(&dir), output),
                Error::Style { .. }
            ));
        }
    }
    let dir = fixture_copy();
    std::fs::write(
        dir.path().join("styles/child.css"),
        "@import url(https://fonts.example/x.css);\n.x { color: red; }",
    )
    .unwrap();
    for output in [OutputMode::Standalone, site(), OutputMode::Serve] {
        let doc = render_sample(&copied_theme(&dir), output.clone());
        assert!(doc.theme_css.starts_with("/* hoisted from @child-theme/styles/child.css */\n@import url(https://fonts.example/x.css);\n"));
        assert_eq!(
            doc.diagnostics
                .iter()
                .filter(|d| d.code == "remote-import")
                .count(),
            usize::from(output == OutputMode::Standalone)
        );
    }
    let dir = fixture_copy();
    let manifest = std::fs::read_to_string(dir.path().join("theme.toml"))
        .unwrap()
        .replace("styles/child.css", "styles/child.scss");
    std::fs::write(dir.path().join("theme.toml"), manifest).unwrap();
    std::fs::write(dir.path().join("styles/child.scss"), ".x {}\n").unwrap();
    assert!(matches!(
        render_error(&copied_theme(&dir), OutputMode::Standalone),
        Error::Theme { .. }
    ));
}

#[test]
fn tmtheme_code_theme_is_compiled() {
    let dir = fixture_copy();
    let manifest = std::fs::read_to_string(dir.path().join("theme.toml"))
        .unwrap()
        .replace(
            "code_theme = \"Nord\"",
            "code_theme = \"styles/code.tmTheme\"",
        );
    std::fs::write(dir.path().join("theme.toml"), manifest).unwrap();
    std::fs::write(dir.path().join("styles/code.tmTheme"), r##"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>name</key><string>Tiny</string><key>settings</key><array><dict><key>settings</key><dict><key>background</key><string>#FFFFFF</string><key>foreground</key><string>#111111</string></dict></dict><dict><key>scope</key><string>comment</string><key>settings</key><dict><key>foreground</key><string>#112233</string></dict></dict></array></dict></plist>"##).unwrap();
    let doc = render_sample(&copied_theme(&dir), OutputMode::Standalone);
    assert!(
        doc.theme_css.contains("code theme: @child-theme/styles/code.tmTheme")
    );
    assert!(doc.theme_css.contains("#112233"));
}

#[test]
fn document_html_needs_exactly_one_morph_root() {
    for source in ["<p>no root</p>", "{{ html.main|safe }}{{ html.main|safe }}"]
    {
        let dir = fixture_copy();
        std::fs::write(dir.path().join("templates/document.html"), source)
            .unwrap();
        let error = render_error(&copied_theme(&dir), OutputMode::Standalone);
        match error {
            Error::Template { template, message, .. } => {
                assert_eq!(template, "document.html");
                assert!(message.contains("exactly one"));
            }
            error => panic!("expected template error, got {error:?}"),
        }
    }
    let dir = fixture_copy();
    std::fs::write(dir.path().join("templates/document.html"),
        "<!-- <main data-writer-document> -->\n<script>'<main data-writer-document>'</script>\n{{ html.main|safe }}").unwrap();
    render_sample(&copied_theme(&dir), OutputMode::Standalone);
}

#[test]
fn serve_render_without_live_reload_warns() {
    let dir = fixture_copy();
    std::fs::write(dir.path().join("templates/document.html"),
        "{% extends '@default/document.html' %}{% block scripts %}{% endblock %}").unwrap();
    let theme = copied_theme(&dir);
    let doc = render_sample(&theme, OutputMode::Serve);
    assert_eq!(
        doc.diagnostics.iter().filter(|d| d.code == "theme-template").count(),
        1
    );
    let doc = render_sample(&theme, OutputMode::Standalone);
    assert!(!doc.diagnostics.iter().any(|d| d.code == "theme-template"));
}

#[test]
fn inlined_scripts_never_close_early() {
    let dir = fixture_copy();
    std::fs::write(
        dir.path().join("scripts/child.js"),
        "window.x = '</script> </SCRIPT>';\n",
    )
    .unwrap();
    let doc = render_sample(&copied_theme(&dir), OutputMode::Standalone);
    assert!(doc.html.contains("window.x = '<\\/script> <\\/SCRIPT>';"));
    let js = "window.x = '<!-- <script>';";
    std::fs::write(dir.path().join("scripts/child.js"), js).unwrap();
    let doc = render_sample(&copied_theme(&dir), OutputMode::Standalone);
    let uri = data_uri("text/javascript;charset=utf-8", js.as_bytes());
    assert!(doc.html.contains(&format!("<script src=\"{uri}\"></script>")));
    assert!(!doc.html.contains(js));
}

#[test]
fn unknown_helper_path_is_a_template_error() {
    let cases = [
        (
            "{{ style('nope.css') }}", "style(\"nope.css\")",
            "assets/ directories",
        ),
        (
            "{{ script('vendor/nope.js') }}", "script(\"vendor/nope.js\")",
            "vendor tree",
        ),
        (
            "{{ asset('@other/x.png') }}", "asset(\"@other/x.png\")",
            "no theme 'other'",
        ),
        (
            "{{ script('../escape.js') }}", "script(\"../escape.js\")",
            "climbs out",
        ),
        ("{{ style('./') }}", "style(\"./\")", "path is empty"),
    ];
    for (source, call, reason) in cases {
        let dir = fixture_copy();
        body_end(&dir, source);
        for output in [OutputMode::Standalone, site(), OutputMode::Serve] {
            match render_error(&copied_theme(&dir), output) {
                Error::Template { template, message, .. } => {
                    assert_eq!(template, "document.html");
                    assert!(message.contains(call), "{message}");
                    assert!(message.contains(reason), "{message}");
                }
                error => panic!("expected template error, got {error:?}"),
            }
        }
    }
}

#[test]
fn render_is_deterministic() {
    let theme = child_theme();
    for mode in [OutputMode::Standalone, site(), OutputMode::Serve] {
        let first = render_sample(&theme, mode.clone());
        let second = render_sample(&theme, mode);
        assert_eq!(first.html, second.html);
        assert_eq!(first.theme_css, second.theme_css);
        assert_eq!(first.files, second.files);
        assert_eq!(first.theme_files, second.theme_files);
        for path in [
            "styles/child.css", "assets/logo.svg",
            "templates/partials/head.html",
        ] {
            let path = root()
                .join("tests/fixtures/themes/child-theme")
                .join(path)
                .canonicalize()
                .unwrap();
            assert!(first.theme_files.contains(&path));
        }
    }
}

#[test]
fn attrs_and_json_filters_in_templates() {
    let dir = tempfile::tempdir().unwrap();
    let theme = disk_theme(dir.path(), "name = \"filters\"\nextends = \"default\"\n", &[
        ("templates/document.html", br#"{{ html.main|safe }}|<x{{ {'data-q': 'a"b', 'data-n': none}|attrs }}>|{{ {'b': '</script>', 'a': 1}|json }}|{{ theme.chain|json(indent=2) }}"#),
    ]);
    let doc = render_sample(&theme, OutputMode::Standalone);
    assert!(doc.html.contains(
        "|<x data-q=\"a&quot;b\">|{\"a\":1,\"b\":\"\\u003c/script\\u003e\"}|[\n  \"filters\",\n  \"default\"\n]"
    ));
}

#[test]
fn byte_order_marks_are_dropped() {
    let dir = fixture_copy();
    let css = "\u{feff}.x { color: red; }\n";
    let js = "\u{feff}window.bom = 1;";
    std::fs::write(dir.path().join("styles/child.css"), css).unwrap();
    std::fs::write(dir.path().join("scripts/child.js"), js).unwrap();
    std::fs::write(dir.path().join("styles/helper.css"), css).unwrap();
    body_end(&dir, "{{ style('@child-theme/styles/helper.css') }}");
    let theme = copied_theme(&dir);
    for output in [OutputMode::Standalone, site(), OutputMode::Serve] {
        let doc = render_sample(&theme, output.clone());
        assert!(doc.theme_css.contains(
            "/* @child-theme/styles/child.css */\n.x { color: red; }"
        ));
        assert!(!doc.theme_css.contains('\u{feff}'));
        assert!(!doc.html.contains('\u{feff}'));
        if output == OutputMode::Standalone {
            assert!(doc.html.contains("<script>\nwindow.bom = 1;"));
            assert!(doc.html.contains("<style>\n.x { color: red; }"));
        }
        if matches!(output, OutputMode::Site { .. }) {
            let css = doc
                .files
                .iter()
                .find(|file| {
                    file.path
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with("helper-")
                })
                .unwrap();
            assert_eq!(css.bytes, b".x { color: red; }\n");
        }
    }
}

#[test]
fn backslash_style_entries_resolve_urls_next_to_the_stylesheet() {
    let tmp = tempfile::tempdir().unwrap();
    let theme = disk_theme(
        tmp.path(),
        "name = \"c\"\nextends = \"default\"\n[styles]\nentries = [\"styles\\\\x.css\"]\n",
        &[
            ("styles/x.css", b".x { background: url(img/b.png); }"),
            ("styles/img/b.png", b"b-bytes"),
        ],
    );
    let doc = render_sample(&theme, OutputMode::Standalone);
    assert!(doc.theme_css.contains(
        "/* @c/styles/x.css */\n.x { background: url(data:image/png;base64,Yi1ieXRlcw==); }"
    ));
    let doc = render_sample(&theme, OutputMode::Serve);
    let url = versioned("@c/styles/img/b.png", b"b-bytes");
    assert!(
        doc.theme_css.contains(&format!(".x {{ background: url({url}); }}"))
    );
}

#[test]
fn style_urls_resolve_to_the_same_files_in_every_mode() {
    let dir = tempfile::tempdir().unwrap();
    let theme = disk_theme(dir.path(), "name = \"m\"\nextends = \"default\"\n", &[
        ("templates/document.html", b"{{ html.main|safe }}{{ style('@m/styles/linked.css') }}{{ live_reload() }}"),
        ("styles/linked.css", b".x { background: url(img/x.png); }"),
        ("styles/img/x.png", b"x-bytes"),
    ]);
    let doc = render_sample(&theme, OutputMode::Standalone);
    assert!(doc.html.contains(
        ".x { background: url(data:image/png;base64,eC1ieXRlcw==); }"
    ));
    let doc = render_sample(&theme, site());
    let css = doc
        .files
        .iter()
        .find(|file| {
            file.path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("linked-")
        })
        .unwrap();
    let css = std::str::from_utf8(&css.bytes).unwrap();
    let name = Regex::new(r"url\((x-[0-9a-f]{12}\.png)\)")
        .unwrap()
        .captures(css)
        .unwrap()[1]
        .to_owned();
    assert_eq!(
        doc.files
            .iter()
            .find(|file| file.path.ends_with(name.as_str()))
            .unwrap()
            .bytes,
        b"x-bytes"
    );
    let doc = render_sample(&theme, OutputMode::Serve);
    assert!(doc.html.contains("<link rel=\"stylesheet\" href=\"/__writer/asset/@m/styles/linked.css\">"));
    let css = html::serve_stylesheet(
        &theme,
        "@m/styles/linked.css",
        ".x { background: url(img/x.png); }",
    )
    .unwrap();
    assert!(css.contains(&versioned("@m/styles/img/x.png", b"x-bytes")));

    // Helper styles resolve next to their own paths in every mode. Theme
    // entries alone permit the chain's assets/ fallback.
    let theme = disk_theme(
        &dir.path().join("n"),
        "name = \"n\"\nextends = \"default\"\n",
        &[
            (
                "templates/document.html",
                b"{{ html.main|safe }}{{ style('@n/styles/linked.css') }}",
            ),
            ("styles/linked.css", b".x { background: url(logo.png); }"),
            ("assets/logo.png", b"logo-bytes"),
        ],
    );
    for output in [OutputMode::Standalone, site(), OutputMode::Serve] {
        match render_error(&theme, output) {
            Error::Template { message, .. } => {
                assert!(message.contains("url(logo.png)"));
                assert!(message.contains("not found next to the stylesheet"));
            }
            error => panic!("expected template error, got {error:?}"),
        }
    }
    assert!(matches!(
        html::serve_stylesheet(
            &theme,
            "@n/styles/linked.css",
            ".x { background: url(logo.png); }"
        ),
        Err(Error::Style { .. })
    ));
}

#[test]
fn theme_file_urls_carry_their_content_version_in_serve_mode() {
    let dir = fixture_copy();
    body_end(&dir, "<img class=\"logo\" src=\"{{ asset('logo.svg') }}\">");
    let theme = copied_theme(&dir);
    let old = std::fs::read(dir.path().join("assets/logo.svg")).unwrap();
    let first = render_sample(&theme, OutputMode::Serve);
    assert!(first.html.contains(&versioned("logo.svg", &old)));
    std::fs::write(dir.path().join("assets/logo.svg"), "<svg>changed</svg>")
        .unwrap();
    let second = render_sample(&theme, OutputMode::Serve);
    assert!(
        second.html.contains(&versioned("logo.svg", b"<svg>changed</svg>"))
    );
    assert!(!second.html.contains(&versioned("logo.svg", &old)));
    assert!(second.theme_css.contains(&versioned(
        "@child-theme/assets/logo.svg", b"<svg>changed</svg>",
    )));
    let doc = render_sample(&theme, OutputMode::Standalone);
    assert!(!doc.html.contains("?v="));
}

#[test]
fn resolve_helper_path_forms() {
    let dir = tempfile::tempdir().unwrap();
    let theme = disk_theme(
        dir.path(),
        "name = \"t\"\nextends = \"default\"\n",
        &[("assets/img/logo.svg", b"<svg/>"), ("notes.txt", b"root file")],
    );
    let resolve = |path: &str| html::resolve_helper_path(&theme, path).unwrap();
    assert!(resolve("vendor/katex/katex.min.js").unwrap().0.len() > 100_000);
    let (script, mime) = resolve("writer/math.js").unwrap();
    let script = String::from_utf8(script).unwrap();
    assert!(script.contains("__writerRenderMath"));
    assert!(script.contains("__writerKatexOptions"));
    assert_eq!(mime, "text/javascript");
    assert_eq!(
        resolve("vendor/katex/fonts/KaTeX_Main-Regular.woff2").unwrap().1,
        "font/woff2"
    );
    assert_eq!(
        resolve("img/logo.svg").unwrap(),
        (b"<svg/>".to_vec(), "image/svg+xml".into())
    );
    assert_eq!(resolve("./img//logo.svg").unwrap().0, b"<svg/>");
    assert_eq!(resolve("@t/notes.txt").unwrap().0, b"root file");
    assert_eq!(resolve("@t/assets/img/logo.svg").unwrap().0, b"<svg/>");
    assert!(resolve("@default/theme.toml").is_some());
    for missing in [
        "vendor/nope.js",
        "@nope/theme.toml",
        "@t/../t/notes.txt",
        "@t/../../etc/passwd",
        "../t/notes.txt",
        "notes.txt",
        "vendor/../themes/default/theme.toml",
        "",
        "/",
    ] {
        assert_eq!(resolve(missing), None, "{missing}");
    }
}

#[test]
fn site_math_assets_are_emitted_as_hashed_files() {
    let usage = MathUsage { katex: true, mathml: true, ..MathUsage::default() };
    let doc = render_math(usage, MathmlFont::Embed, site());
    assert!(!doc.html.contains("asset:"));
    assert!(!doc.html.contains("data:font/"));
    assert!(
        Regex::new(r"assets/latinmodern-math-[0-9a-f]{12}\.woff2")
            .unwrap()
            .is_match(&doc.html)
    );
    for prefix in ["katex.min-", "math-", "latinmodern-math-"] {
        assert!(doc.files.iter().any(|file| {
            file.path.file_name().unwrap().to_str().unwrap().starts_with(prefix)
        }));
    }
}

#[test]
fn equation_images_remain_registered_for_the_serve_route() {
    struct ImageMath;
    impl MathHook for ImageMath {
        fn equation(
            &mut self, _eq: &MathRequest<'_>, assets: &mut AssetRegistry,
            _diagnostics: &mut Diagnostics,
        ) -> String {
            let id = assets.register_bytes(
                b"equation".to_vec(),
                "image/svg+xml",
                "eq.svg",
            );
            format!("<img src=\"asset:{id}\">")
        }
        fn usage(&self) -> MathUsage {
            MathUsage { image: true, ..MathUsage::default() }
        }
    }
    let (document, mut assets) = sample();
    let theme = default_theme();
    let mut math = ImageMath;
    let doc = html::render(RenderInput {
        document: &document,
        assets: &mut assets,
        theme: &theme,
        config: &Config::default(),
        output: OutputMode::Serve,
        build: BuildInfo::serve(3),
        math: &mut math,
    })
    .unwrap();
    let id =
        assets.register_bytes(b"equation".to_vec(), "image/svg+xml", "eq.svg");
    assert_eq!(assets.read(&id).unwrap(), b"equation");
    assert!(doc.html.contains(&format!("/__writer/doc-asset/{id}?v=")));
    assert!(!doc.html.contains("asset:"));
}
