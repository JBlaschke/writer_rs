//! Document rendering and output-mode-aware template helpers.
//!
//! Lifted from presenter_rs `render/html.rs`. Helpers own a registry behind
//! a mutex because minijinja functions must be `Send + Sync + 'static`.
//! Each helper resolves its own placeholders, sharing the site file list;
//! a final emitter resolves the document's placeholders exactly once.
//! Inlined scripts and styles cannot terminate their elements early.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use minijinja::value::ValueKind;
use minijinja::{Environment, ErrorKind, Value};

use super::emit::MathUsage;
use super::paths::{
    helper_path_exists, normalize_helper_path, resolve_helper_path, serve_url,
    unknown_reason, versioned_serve_url,
};
use super::styles::{self, Targets, UrlRules};
use super::{BuildMode, RenderInput, RenderedDocument, context};
use crate::assets::emit::{EmittedFile, Emitter, OutputMode, data_uri};
use crate::assets::registry::AssetRegistry;
use crate::config::MathmlFont;
use crate::error::{Diagnostic, Diagnostics, Error, Result, template_error};
use crate::ir::Features;
use crate::theme::Theme;

/// URL prefix of the dev server route that serves helper paths (live-reload.md §1).
pub const SERVE_HELPER_PREFIX: &str = "/__writer/asset/";

/// URL of the dev server's theme stylesheet (live-reload.md §1).
pub const SERVE_THEME_CSS: &str = "/__writer/theme.css";

/// URL of the live-reload client script (live-reload.md §1).
pub const SERVE_LIVE_RELOAD_JS: &str = "/__writer/live-reload.js";

/// The text the dev server serves for `css`, the stylesheet at the normalized helper path
/// `key` of `theme`'s chain (a file a template links with `style()`): each relative `url()`
/// that names a file (next to the stylesheet, as `style()` resolves it) becomes that file's
/// absolute dev-server URL, versioned like the theme stylesheet's (module docs, "Content
/// versions"); a vendor reference that names nothing stays as written.
///
/// Errors when a theme stylesheet URL cannot resolve or a file cannot be read.
pub fn serve_stylesheet(theme: &Theme, key: &str, css: &str) -> Result<String> {
    let rules = UrlRules::for_style(key);
    styles::rewrite_urls(theme, key, css, rules, Targets::ServeUrls)
}

/// Renders markup, compiles the theme and resolves all page assets.
///
/// Configuration must already be effective. No output files are written;
/// site-mode files are returned for the caller to write.
pub fn render(input: RenderInput<'_>) -> Result<RenderedDocument> {
    let (rendered, theme_files) =
        crate::theme::recording_reads(|| render_document(input));
    let mut rendered = rendered?;
    rendered.theme_files = theme_files;
    Ok(rendered)
}

fn render_document(mut input: RenderInput<'_>) -> Result<RenderedDocument> {
    let mut diagnostics = Diagnostics::new();
    let (emitted, ctx) = context::build_context(&mut input, &mut diagnostics)?;
    let mut state = HelperState {
        theme: input.theme.clone(),
        mode: input.output.clone(),
        registry: AssetRegistry::new(),
        files: Vec::new(),
        diagnostics: Diagnostics::new(),
        live_reload_called: false,
    };
    let theme_css = state.theme_css(&input.document.features)?;
    let extras = Extras {
        theme_css: Arc::from(theme_css.as_str()),
        math: emitted.math,
        mathml_font: input.config.math.mathml_font,
        katex_options: script_safe_json(&serde_json::to_string(
            &input.config.math.katex_options,
        )?),
        live_reload: input.build.live_reload,
        build_id: input.build.id,
    };
    let state = Arc::new(Mutex::new(state));
    let html = render_template(&input, &state, extras, &ctx)?;
    if crate::assets::html_count_attr(&html, "data-writer-document") != 1 {
        return Err(Error::Template {
            template: "document.html".into(),
            line: None,
            message: "expected exactly one element with data-writer-document"
                .into(),
        });
    }
    let (files, helper_diagnostics) = {
        let mut guard = state.lock().map_err(|_| poisoned())?;
        if input.build.mode == BuildMode::Serve && !guard.live_reload_called {
            diagnostics.push(Diagnostic::warning(
                "theme-template",
                "document.html did not call live_reload(); preview cannot update",
            ));
        }
        (
            std::mem::take(&mut guard.files),
            std::mem::take(&mut guard.diagnostics),
        )
    };
    diagnostics.extend(helper_diagnostics);
    let mut emitter = Emitter::with_files(input.assets, &input.output, files);
    let html = emitter.resolve_html(&html)?;
    Ok(RenderedDocument {
        html,
        theme_css,
        files: emitter.files,
        diagnostics,
        theme_files: Default::default(),
    })
}

fn render_template(
    input: &RenderInput<'_>, state: &Arc<Mutex<HelperState>>, extras: Extras,
    ctx: &serde_json::Value,
) -> Result<String> {
    let mut env =
        input.theme.environment(input.build.mode == BuildMode::Serve)?;
    register_helpers(&mut env, state, extras);
    let template = env
        .get_template("document.html")
        .map_err(|error| template_error(error, env.debug()))?;
    template.render(ctx).map_err(|error| template_error(error, env.debug()))
}

/// JSON safe to place inside an inline script.
fn script_safe_json(json: &str) -> String {
    json.replace('<', "\\u003c")
}

/// State shared by the template helpers of one render.
struct HelperState {
    theme: Theme,
    mode: OutputMode,
    registry: AssetRegistry,
    files: Vec<EmittedFile>,
    diagnostics: Diagnostics,
    live_reload_called: bool,
}

/// Immutable settings captured by minijinja's helper functions.
struct Extras {
    theme_css: Arc<str>,
    math: MathUsage,
    mathml_font: MathmlFont,
    katex_options: String,
    live_reload: bool,
    build_id: u64,
}

impl HelperState {
    /// Runs `f` with an emitter over the helpers' registry that continues this render's
    /// file list.
    fn with_emitter<T>(
        &mut self, f: impl FnOnce(&mut Emitter<'_>) -> Result<T>,
    ) -> Result<T> {
        let files = std::mem::take(&mut self.files);
        let mut emitter =
            Emitter::with_files(&self.registry, &self.mode, files);
        let result = f(&mut emitter);
        self.files = emitter.files;
        result
    }

    /// Registers `bytes` and returns their URL for this mode (site: emits the file
    /// `<name>-<hash>.<ext>` named after `key`'s last segment).
    fn emit_bytes(
        &mut self, bytes: Vec<u8>, mime: &str, key: &str,
    ) -> Result<String> {
        let hint = key.rsplit('/').next().unwrap_or(key);
        let id = self.registry.register_bytes(bytes, mime, hint);
        self.with_emitter(|emitter| emitter.url_for(&id))
    }

    /// The compiled theme stylesheet, resolved for this mode (standalone: `data:` URIs;
    /// site: a file in `assets/`, so bare file names; serve: `/__writer/asset/` URLs).
    fn theme_css(&mut self, features: &Features) -> Result<String> {
        let mut remote = Vec::new();
        let targets = match &self.mode {
            OutputMode::Serve => Targets::ServeUrls,
            _ => Targets::Placeholders(&mut self.registry),
        };
        let css = styles::compile(&self.theme, features, targets, &mut remote)?;
        for (key, url) in remote {
            self.warn_remote_import(
                &format!("theme stylesheet entry '{key}'"),
                &url,
            );
        }
        // Serve: no placeholder is left, except `asset:<id>` text the theme wrote itself,
        // which is an unknown id here exactly as in the other modes.
        self.with_emitter(|e| e.resolve_css_in_assets_dir(&css))
    }

    /// A `remote-import` warning in standalone mode (see the module docs).
    fn warn_remote_import(&mut self, stylesheet: &str, url: &str) {
        if self.mode != OutputMode::Standalone {
            return;
        }
        self.diagnostics.push(Diagnostic::warning(
            "remote-import",
            format!(
                "{stylesheet} imports {url}: the standalone file is not self-contained, it \
                 loads that stylesheet from the network when opened (offline the browser \
                 skips it, e.g. falling back to other fonts)"
            ),
        ));
    }

    /// The normalized helper path `path` names if a file is there, else the "unknown path"
    /// error naming `helper`, `path` and the reason.
    fn lookup(&self, helper: &str, path: &str) -> Result<String> {
        match normalize_helper_path(path) {
            Some(key) if helper_path_exists(&self.theme, &key) => Ok(key),
            _ => Err(self.unknown(helper, path)),
        }
    }

    fn unknown(&self, helper: &str, path: &str) -> Error {
        Error::Other(format!(
            "{helper}(\"{path}\"): unknown path ({})",
            unknown_reason(&self.theme, path)
        ))
    }

    /// The bytes and MIME type at `key` (found by [`HelperState::lookup`]).
    fn read(
        &self, helper: &str, path: &str, key: &str,
    ) -> Result<(Vec<u8>, String)> {
        resolve_helper_path(&self.theme, key)?
            .ok_or_else(|| self.unknown(helper, path))
    }

    /// The text at `key` (UTF-8, a leading byte order mark removed).
    fn text(&self, helper: &str, path: &str, key: &str) -> Result<String> {
        let (bytes, _) = self.read(helper, path, key)?;
        styles::decode_text(bytes).map_err(|e| {
            Error::Other(format!("{helper}(\"{path}\"): not valid UTF-8: {e}"))
        })
    }

    fn style(&mut self, path: &str) -> Result<String> {
        let key = self.lookup("style", path)?;
        let rules = UrlRules::for_style(&key);
        if self.mode == OutputMode::Serve {
            if !rules.is_lenient() {
                // The browser loads the url()s; check them as the other modes do.
                let css = self.text("style", path, &key)?;
                styles::rewrite_urls(
                    &self.theme,
                    &key,
                    &css,
                    rules,
                    Targets::ServeUrls,
                )?;
            }
            return Ok(format!(
                r#"<link rel="stylesheet" href="{}">"#,
                serve_url(&key)
            ));
        }
        let css = self.text("style", path, &key)?;
        for url in styles::remote_imports(&css) {
            self.warn_remote_import(&format!("style(\"{path}\")"), &url);
        }
        let targets = Targets::Placeholders(&mut self.registry);
        let css =
            styles::rewrite_urls(&self.theme, &key, &css, rules, targets)?;
        if matches!(self.mode, OutputMode::Site { .. }) {
            let css =
                self.with_emitter(|e| e.resolve_css_in_assets_dir(&css))?;
            let url = self.emit_bytes(css.into_bytes(), "text/css", &key)?;
            return Ok(format!(
                r#"<link rel="stylesheet" href="{}">"#,
                attr(&url)
            ));
        }
        let css = self.with_emitter(|e| e.resolve_css(&css))?;
        Ok(format!("<style>\n{}</style>", raw_text(&css, "style")))
    }

    fn script(&mut self, path: &str) -> Result<String> {
        let key = self.lookup("script", path)?;
        match &self.mode {
            OutputMode::Serve => {
                Ok(format!(r#"<script src="{}"></script>"#, serve_url(&key)))
            }
            OutputMode::Site { .. } => {
                let (bytes, mime) = self.read("script", path, &key)?;
                let url = self.emit_bytes(bytes, &mime, &key)?;
                Ok(format!(r#"<script src="{}"></script>"#, attr(&url)))
            }
            OutputMode::Standalone => {
                Ok(inline_script(&self.text("script", path, &key)?))
            }
        }
    }

    fn asset(&mut self, path: &str) -> Result<String> {
        let key = self.lookup("asset", path)?;
        match &self.mode {
            OutputMode::Serve => versioned_serve_url(&self.theme, &key)?
                .ok_or_else(|| self.unknown("asset", path)),
            OutputMode::Site { .. } => {
                let (bytes, mime) = self.read("asset", path, &key)?;
                let url = self.emit_bytes(bytes, &mime, &key)?;
                Ok(attr(&url).into_owned())
            }
            OutputMode::Standalone => {
                let (bytes, mime) = self.read("asset", path, &key)?;
                Ok(data_uri(&mime, &bytes))
            }
        }
    }

    fn theme_style(&mut self, css: &str, build_id: u64) -> Result<String> {
        match &self.mode {
            OutputMode::Serve => Ok(format!(
                r#"<link id="writer-theme" rel="stylesheet" href="{SERVE_THEME_CSS}?v={build_id}">"#
            )),
            OutputMode::Site { .. } => {
                let url = self.emit_bytes(
                    css.as_bytes().to_vec(),
                    "text/css",
                    "theme.css",
                )?;
                Ok(format!(
                    r#"<link id="writer-theme" rel="stylesheet" href="{}">"#,
                    attr(&url)
                ))
            }
            OutputMode::Standalone => Ok(format!(
                "<style id=\"writer-theme\">\n{}</style>",
                raw_text(css, "style")
            )),
        }
    }
    fn math_init(
        &mut self, usage: MathUsage, font: MathmlFont, options: &str,
    ) -> Result<String> {
        let mut parts = Vec::new();
        if usage.katex {
            parts.push(self.style("vendor/katex/katex.min.css")?);
            parts.push(format!(
                "<script>window.__writerKatexOptions = {options};</script>"
            ));
            parts.push(self.script("vendor/katex/katex.min.js")?);
            parts.push(self.script("writer/math.js")?);
        }
        if usage.mathml || usage.typst_mathml {
            let mut css = String::from("<style id=\"writer-math\">\n");
            if font == MathmlFont::Embed {
                let url = self
                    .asset("vendor/latin-modern-math/latinmodern-math.woff2")?;
                let _ = writeln!(
                    css,
                    "@font-face {{ font-family: \"Latin Modern Math\"; src: url(\"{url}\") format(\"woff2\"); }}"
                );
            }
            css.push_str(
                "math { font-family: \"Latin Modern Math\", \"STIX Two Math\", \"Cambria Math\", \"Libertinus Math\", math; }"
            );
            if usage.typst_mathml {
                css.push('\n');
                css.push_str(crate::typst::html::EQUATION_CSS);
            }
            css.push_str("\n</style>");
            parts.push(css);
        }
        Ok(parts.join("\n"))
    }
}

/// Registers template helpers and the `attrs` and `json` filters.
fn register_helpers(
    env: &mut Environment<'static>, state: &Arc<Mutex<HelperState>>,
    extras: Extras,
) {
    let Extras {
        theme_css,
        math,
        mathml_font,
        katex_options,
        live_reload,
        build_id,
    } = extras;
    let st = Arc::clone(state);
    env.add_function("style", move |path: String| {
        call(&st, |s| s.style(&path))
    });
    let st = Arc::clone(state);
    env.add_function("script", move |path: String| {
        call(&st, |s| s.script(&path))
    });
    let st = Arc::clone(state);
    env.add_function("asset", move |path: String| {
        call(&st, |s| s.asset(&path))
    });
    let st = Arc::clone(state);
    env.add_function("theme_style", move || {
        call(&st, |s| s.theme_style(&theme_css, build_id))
    });
    let st = Arc::clone(state);
    env.add_function("math_init", move || {
        call(&st, |s| s.math_init(math, mathml_font, &katex_options))
    });
    let st = Arc::clone(state);
    env.add_function("live_reload", move || {
        call(&st, |s| {
            s.live_reload_called = true;
            Ok(if live_reload {
                format!(r#"<script src="{SERVE_LIVE_RELOAD_JS}"></script>"#)
            } else {
                String::new()
            })
        })
    });
    env.add_filter("attrs", attrs_filter);
    env.add_filter("json", minijinja::filters::tojson);
}

/// Runs a helper on the locked state; its HTML result is marked safe.
fn call(
    state: &Mutex<HelperState>,
    helper: impl FnOnce(&mut HelperState) -> Result<String>,
) -> std::result::Result<Value, minijinja::Error> {
    let mut guard =
        state.lock().map_err(|_| helper_error(poisoned().to_string()))?;
    helper(&mut guard)
        .map(Value::from_safe_string)
        .map_err(|err| helper_error(err.to_string()))
}

fn helper_error(message: String) -> minijinja::Error {
    minijinja::Error::new(ErrorKind::InvalidOperation, message)
}

fn poisoned() -> Error {
    Error::Other(
        "template helper state is unusable after a panic in a helper".into(),
    )
}

/// A `<script>` element that runs `js` inline (see the module docs).
fn inline_script(js: &str) -> String {
    let text = raw_text(js, "script");
    if leaves_script_open(&text) {
        let uri = data_uri("text/javascript;charset=utf-8", js.as_bytes());
        return format!(r#"<script src="{uri}"></script>"#);
    }
    format!("<script>\n{text}</script>")
}

/// `text` for inlining in a `<tag>` raw-text element: every `</tag` (ASCII case-insensitive)
/// becomes `<\/tag` (an escaped `/` in JS strings, regexes and CSS strings), and the text
/// ends with a line break.
fn raw_text(text: &str, tag: &str) -> String {
    let mut out = String::with_capacity(text.len() + 1);
    let mut last = 0;
    for (at, _) in text.match_indices("</") {
        let name = text.as_bytes().get(at + 2..at + 2 + tag.len());
        if name.is_some_and(|name| name.eq_ignore_ascii_case(tag.as_bytes())) {
            out.push_str(&text[last..at]);
            out.push_str("<\\/");
            last = at + 2;
        }
    }
    out.push_str(&text[last..]);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// True when the content `text` of a `<script>` element (passed through [`raw_text`], so
/// without `</script`) ends with the HTML tokenizer in the "script data double escaped"
/// state, where the element's own `</script>` does not close it (HTML Living Standard
/// §13.2.5.4 and §13.2.5.20–31): `<!--` enters the escaped state, which `-->` leaves (the
/// dashes of `<!--` count, so `<!-->` leaves at once); in it, `<script` followed by
/// whitespace, `/`, `>` (or the end, as the element's `</script>` follows a line break)
/// enters the double escaped state, which only `-->` leaves. May err on the side of `true`.
fn leaves_script_open(text: &str) -> bool {
    enum State {
        Data,
        Escaped,
        DoubleEscaped,
    }
    // ASCII lowercasing keeps byte offsets.
    let lower = text.to_ascii_lowercase();
    let mut state = State::Data;
    let mut at = 0;
    loop {
        let rest = &lower[at..];
        match state {
            State::Data => match rest.find("<!--") {
                Some(i) => {
                    state = State::Escaped;
                    at += i + 2;
                }
                None => return false,
            },
            State::Escaped => match (rest.find("-->"), script_start(rest)) {
                (Some(exit), Some(enter)) if exit < enter => {
                    state = State::Data;
                    at += exit + 3;
                }
                (_, Some(enter)) => {
                    state = State::DoubleEscaped;
                    at += enter + 7;
                }
                (Some(exit), None) => {
                    state = State::Data;
                    at += exit + 3;
                }
                (None, None) => return false,
            },
            State::DoubleEscaped => match rest.find("-->") {
                Some(exit) => {
                    state = State::Data;
                    at += exit + 3;
                }
                None => return true,
            },
        }
    }
}

/// Byte offset of the first `<script` in the lowercase `text` that is followed by
/// whitespace, `/`, `>` or the end of `text`.
fn script_start(text: &str) -> Option<usize> {
    text.match_indices("<script").map(|(i, _)| i).find(|&i| {
        matches!(
            text.as_bytes().get(i + "<script".len()),
            None | Some(b'\t' | b'\n' | b'\x0c' | b'\r' | b' ' | b'/' | b'>')
        )
    })
}

/// `text` escaped for a double-quoted attribute value.
fn attr(text: &str) -> Cow<'_, str> {
    html_escape::encode_double_quoted_attribute(text)
}

// ---------------------------------------------------------------------------------------
// Filters
// ---------------------------------------------------------------------------------------

/// `attrs`: a map → ` k="v"` pairs in map order, keys and values escaped; `none`/undefined
/// values are skipped and an undefined or `none` map renders nothing.
fn attrs_filter(map: Value) -> std::result::Result<Value, minijinja::Error> {
    let mut out = String::new();
    if map.is_undefined() || map.is_none() {
        return Ok(Value::from_safe_string(out));
    }
    if map.kind() != ValueKind::Map {
        return Err(minijinja::Error::new(
            ErrorKind::InvalidOperation,
            format!("attrs filter expects a map, got {}", map.kind()),
        ));
    }
    for key in map.try_iter()? {
        let value = map.get_item(&key)?;
        if value.is_undefined() || value.is_none() {
            continue;
        }
        let key = text_of(&key);
        let value = text_of(&value);
        let _ = write!(out, " {}=\"{}\"", attr(&key), attr(&value));
    }
    Ok(Value::from_safe_string(out))
}

/// A value's plain text: strings as they are, anything else in its display form.
fn text_of(value: &Value) -> String {
    match value.as_str() {
        Some(text) => text.to_string(),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_text_never_closes_its_element() {
        assert_eq!(
            raw_text("a</script>b</SCRIPT x", "script"),
            "a<\\/script>b<\\/SCRIPT x\n"
        );
        assert_eq!(raw_text("a</scrip", "script"), "a</scrip\n");
        assert_eq!(raw_text("p{}\n", "style"), "p{}\n");
        assert_eq!(raw_text("x='</style>'", "style"), "x='<\\/style>'\n");
    }

    #[test]
    fn scripts_that_would_stay_open_are_detected() {
        let open = [
            "window.x = '<!-- <script>';",
            "var s = '<!--<SCRIPT/';",
            "'<!-- <script>' + 1",
            "<!-- <script\n",
            "<!--<script>",
            "<!-- <script> --> <!-- <script>",
            "<!-- --> <!-- x <script\t-",
        ];
        for text in open {
            assert!(leaves_script_open(&raw_text(text, "script")), "{text}");
        }
        let closed = [
            "", "let a = 1 < 2;", "a <!-- b", "'<script>'",
            "<!-- <script> -->", "<!-- --> <script>",
            "<!-- <scripts> <scriptx",
            // Only whitespace, `/` and `>` end the tag name the tokenizer compares.
            "'<!-- <script' + 1", "<!--><script>", "<!---><script>",
            "<!-- <script> --->",
            // `-- >` does not leave the escaped state; `-->` leaves the double escaped one.
            "<!-- -- > <script>--> a",
        ];
        for text in closed {
            assert!(!leaves_script_open(&raw_text(text, "script")), "{text}");
        }
    }

    #[test]
    fn open_scripts_are_inlined_as_data_uris() {
        let js = "window.x = '<!-- <script>';";
        let tag = inline_script(js);
        let uri = data_uri("text/javascript;charset=utf-8", js.as_bytes());
        assert_eq!(tag, format!(r#"<script src="{uri}"></script>"#));
        assert!(uri.starts_with("data:text/javascript;charset=utf-8;base64,"));
        assert_eq!(inline_script("a()"), "<script>\na()\n</script>");
    }

    #[test]
    fn vendored_scripts_are_inlined_as_text() {
        for path in ["vendor/katex/katex.min.js", "writer/math.js"] {
            let js = crate::vendor::get_str(path).unwrap();
            assert!(!leaves_script_open(&raw_text(&js, "script")), "{path}");
        }
    }

    #[test]
    fn attrs_filter_escapes_and_skips_none() {
        let map = Value::from_serialize(serde_json::json!({
            "data-a": "x\"<y>&",
            "data-b": "",
            "data-c": null,
            "data-n": 3
        }));
        let out = attrs_filter(map).unwrap();
        assert!(out.is_safe());
        assert_eq!(
            out.to_string(),
            r#" data-a="x&quot;&lt;y&gt;&amp;" data-b="" data-n="3""#
        );
        assert_eq!(attrs_filter(Value::UNDEFINED).unwrap().to_string(), "");
        assert!(attrs_filter(Value::from("x")).is_err());
    }
}
