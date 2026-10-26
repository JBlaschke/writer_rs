//! Compiled theme stylesheets and stylesheet `url()` resolution.
//!
//! Theme entries resolve references next to their stylesheet, then in the
//! chain's `assets/` directories, then among `vendor/` and `writer/` files.
//! Helper stylesheets resolve next to their own helper path; only embedded
//! helpers may retain missing references, such as KaTeX's legacy font fallbacks.
//! The same lookup applies in every output mode: builds register placeholders,
//! and serve mode writes versioned `/__writer/asset/` URLs.
//!
//! Top-level remote imports are hoisted before all other rules. Local imports
//! must instead be listed as separate `[styles] entries` (html-output.md §5).

use std::fmt::Write as _;
use std::string::FromUtf8Error;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::highlight::code_theme_css;
use super::paths::{
    is_builtin, join_relative, normalize_helper_path, resolve_helper_path,
    versioned_serve_url,
};
use crate::assets::emit::register_css_urls;
use crate::assets::registry::{AssetData, AssetRegistry};
use crate::error::{Error, Result};
use crate::ir::Features;
use crate::theme::Theme;

/// How a stylesheet's relative `url()` references are resolved.
#[derive(Debug, Clone, Copy)]
pub(super) struct UrlRules {
    /// Leave missing references as written instead of failing.
    lenient: bool,
    /// Also search the theme chain's `assets/` directories.
    assets_fallback: bool,
}

impl UrlRules {
    /// The rules for a theme `[styles] entries` file.
    const THEME_ENTRY: UrlRules =
        UrlRules { lenient: false, assets_fallback: true };

    /// Helper stylesheets resolve next to themselves; built-ins are lenient.
    pub(super) fn for_style(key: &str) -> UrlRules {
        UrlRules { lenient: is_builtin(key), assets_fallback: false }
    }

    /// Whether missing references remain as written.
    pub(super) fn is_lenient(self) -> bool {
        self.lenient
    }
}

/// What resolved references become.
pub(super) enum Targets<'r> {
    /// Registered `asset:<id>` placeholders for standalone and site output.
    Placeholders(&'r mut AssetRegistry),
    /// Versioned helper URLs; no bytes are copied into the registry.
    ServeUrls,
}

impl Targets<'_> {
    /// A shorter-lived copy for processing several stylesheets.
    fn reborrow(&mut self) -> Targets<'_> {
        match self {
            Targets::Placeholders(registry) => Targets::Placeholders(registry),
            Targets::ServeUrls => Targets::ServeUrls,
        }
    }
}

/// Compiles entries parent-first and appends code CSS when the document uses it.
/// Relative URLs become registered placeholders; the caller resolves those for
/// its output mode. Invalid entry extensions are [`Error::Theme`]; local imports,
/// invalid UTF-8 and missing URL targets are [`Error::Style`].
pub fn compile_theme_css(
    theme: &Theme, features: &Features, registry: &mut AssetRegistry,
) -> Result<String> {
    compile(theme, features, Targets::Placeholders(registry), &mut Vec::new())
}

/// Compiles entries with the requested URL targets and records remote imports.
pub(super) fn compile(
    theme: &Theme, features: &Features, mut targets: Targets<'_>,
    remote: &mut Vec<(String, String)>,
) -> Result<String> {
    let mut css = String::new();
    let mut imports = Vec::new();
    for entry in theme.style_entries()? {
        let written = format!("@{}/{}", entry.theme, entry.rel_path);
        let key = normalize_helper_path(&written).unwrap_or(written);
        if !key.ends_with(".css") {
            return Err(Error::Theme {
                name: entry.theme,
                message: format!(
                    "style entry '{}' must end in .css; SCSS themes arrive with T79",
                    entry.rel_path
                ),
            });
        }
        let text = decode_text(entry.bytes).map_err(|err| Error::Style {
            file: key.clone(),
            message: format!("stylesheet is not UTF-8: {err}"),
        })?;
        let text = take_imports(&key, &text, &mut imports, remote)?;
        let text = rewrite_urls(
            theme,
            &key,
            &text,
            UrlRules::THEME_ENTRY,
            targets.reborrow(),
        )?;
        let _ = writeln!(css, "/* {key} */");
        css.push_str(&text);
        if !css.ends_with('\n') {
            css.push('\n');
        }
    }
    if features.code {
        append_code_css(theme, &mut css)?;
    }
    let mut head = String::new();
    for (key, rule) in imports {
        let _ = writeln!(head, "/* hoisted from {key} */\n{rule}");
    }
    Ok(head + &css)
}

/// Writes the two code themes in disjoint colour-scheme queries when both exist.
fn append_code_css(theme: &Theme, css: &mut String) -> Result<()> {
    let (light, dark) = theme.code_themes()?;
    if let Some(light) = light {
        let _ = writeln!(css, "/* code theme: {} */", light.label);
        if dark.is_some() {
            css.push_str("@media not all and (prefers-color-scheme: dark) {\n");
        }
        css.push_str(&code_theme_css(&light.theme));
        if dark.is_some() {
            css.push_str("}\n");
        } else {
            css.push('\n');
        }
    }
    if let Some(dark) = dark {
        let _ = writeln!(
            css,
            "/* code theme (dark): {} */\n@media (prefers-color-scheme: dark) {{",
            dark.label
        );
        css.push_str(&code_theme_css(&dark.theme));
        css.push_str("}\n");
    }
    Ok(())
}

/// Removes valid top-level remote imports; nested imports stay as written.
fn take_imports(
    key: &str, css: &str, imports: &mut Vec<(String, String)>,
    remote: &mut Vec<(String, String)>,
) -> Result<String> {
    let mut out = String::with_capacity(css.len());
    let mut last = 0;
    let mut depth = 0usize;
    for caps in CSS_IMPORT.captures_iter(css) {
        let Some(whole) = caps.get(0) else { continue };
        if let Some(brace) = caps.name("brace") {
            depth = match brace.as_str() {
                "{" => depth + 1,
                _ => depth.saturating_sub(1),
            };
            continue;
        }
        let Some(import) = ImportRule::of(&caps) else {
            continue;
        };
        if depth > 0 {
            tracing::warn!(
                stylesheet = key,
                "{}: an @import inside a block is ignored by browsers; left as written",
                import.text.trim_end()
            );
            continue;
        }
        let urls = remote_imports(import.text);
        if import.is_local() || urls.is_empty() {
            let line = css[..whole.start()].matches('\n').count() + 1;
            return Err(Error::Style {
                file: key.to_string(),
                message: format!(
                    "line {line}: @import url({}): local imports are not inlined; list the file in [styles] entries",
                    import.url
                ),
            });
        }
        out.push_str(&css[last..whole.start()]);
        let rule = format!("{};", import.text.trim_end_matches(';'));
        imports.push((key.to_string(), rule));
        remote.extend(urls.into_iter().map(|url| (key.to_string(), url)));
        last = whole.end();
    }
    out.push_str(&css[last..]);
    Ok(out)
}

/// `bytes` as UTF-8 text without a leading byte order mark (see the module docs).
pub(super) fn decode_text(
    bytes: Vec<u8>,
) -> std::result::Result<String, FromUtf8Error> {
    let mut text = String::from_utf8(bytes)?;
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    Ok(text)
}

/// Rewrites the relative `url()`s of the stylesheet `css` at helper path `key` following
/// `rules` (see the module docs).
///
/// Errors: [`Error::Style`] naming `key` for a reference that resolves to nothing (unless
/// lenient); a read error of an existing file.
pub(super) fn rewrite_urls(
    theme: &Theme, key: &str, css: &str, rules: UrlRules, targets: Targets<'_>,
) -> Result<String> {
    let chain: Vec<&str> =
        theme.chain.iter().map(|dir| dir.name.as_str()).collect();
    let lookup = Lookup { key, rules, chain: &chain };
    match targets {
        Targets::Placeholders(registry) => {
            let mut fetch = |path: &str| {
                Ok(resolve_helper_path(theme, path)?.map(|(bytes, _)| bytes))
            };
            // Register first in a scratch registry, then standardize font MIME
            // types. Ids hash bytes only, so placeholders stay valid.
            let mut found = AssetRegistry::new();
            let css = register_urls(lookup, css, &mut found, &mut fetch)?;
            for asset in found.iter() {
                match &asset.data {
                    AssetData::Bytes(bytes) => {
                        let mime = standard_font_mime(&asset.mime);
                        registry.register_bytes(
                            bytes.clone(),
                            mime,
                            &asset.file_name_hint,
                        )
                    }
                    AssetData::File(path) => registry.register_file(path),
                };
            }
            Ok(css)
        }
        Targets::ServeUrls => {
            // A scratch registry is the id → URL table: each target is registered with its
            // versioned URL as its "bytes" (ids are content hashes, so one URL is one id),
            // and the placeholders are then replaced by the URLs.
            let mut table = AssetRegistry::new();
            let mut fetch = |path: &str| {
                Ok(versioned_serve_url(theme, path)?.map(String::into_bytes))
            };
            let mut css = register_urls(lookup, css, &mut table, &mut fetch)?;
            for asset in table.iter() {
                if let AssetData::Bytes(url) = &asset.data {
                    let placeholder = AssetRegistry::placeholder(&asset.id);
                    css = replace_placeholder(
                        &css,
                        &placeholder,
                        &String::from_utf8_lossy(url),
                    );
                }
            }
            Ok(css)
        }
    }
}

/// `css` with every `placeholder` replaced by `url`. When `url` has a query and the
/// reference its own (the suffix after the placeholder, `?x=1#y`), they are joined:
/// `<url>&x=1#y`.
fn replace_placeholder(css: &str, placeholder: &str, url: &str) -> String {
    let mut out = String::with_capacity(css.len() + url.len());
    let mut rest = css;
    while let Some(at) = rest.find(placeholder) {
        out.push_str(&rest[..at]);
        out.push_str(url);
        rest = &rest[at + placeholder.len()..];
        if url.contains('?')
            && let Some(query) = rest.strip_prefix('?')
        {
            out.push('&');
            rest = query;
        }
    }
    out.push_str(rest);
    out
}

/// The RFC 8081 type for the legacy font MIME types `mime_guess` reports
/// (`application/font-woff` for `.woff`, `application/font-sfnt` for `.otf`);
/// any other type is unchanged.
fn standard_font_mime(mime: &str) -> &str {
    match mime {
        "application/font-woff" => "font/woff",
        "application/font-sfnt" => "font/otf",
        other => other,
    }
}

/// Where the references of one stylesheet are looked up.
#[derive(Clone, Copy)]
struct Lookup<'l> {
    /// The stylesheet's normalized helper path.
    key: &'l str,
    /// Its rules.
    rules: UrlRules,
    /// The names of the chain's themes, child first.
    chain: &'l [&'l str],
}

impl Lookup<'_> {
    /// The helper paths the reference `rel` may name, in lookup order: `rel` joined to the
    /// stylesheet's directory, then (with the `assets/` fallback) `@<theme>/assets/<rel>`
    /// for each theme of the chain, child first, and last, for a `vendor/…` or
    /// `writer/…` `rel`, that builtin helper path (html-output.md §5: such names come
    /// from the embedded tree; a theme's own `assets/vendor/…` file comes first).
    fn candidates(self, rel: &str) -> Vec<String> {
        let mut paths: Vec<String> =
            join_relative(self.key, rel).into_iter().collect();
        let fallback = self
            .rules
            .assets_fallback
            .then(|| normalize_helper_path(rel))
            .flatten();
        if let Some(rel) = fallback {
            let builtin = is_builtin(&rel).then(|| rel.clone());
            let chain =
                self.chain.iter().map(|name| format!("@{name}/assets/{rel}"));
            for path in chain.chain(builtin) {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        paths
    }

    /// Where a reference `rel` that resolved to nothing was looked for, for the error.
    fn looked_in(self, rel: &str) -> &'static str {
        if !self.rules.assets_fallback {
            ""
        } else if normalize_helper_path(rel).is_some_and(|rel| is_builtin(&rel))
        {
            ", in the theme chain's assets/ directories or among the built-in files"
        } else {
            " or in the theme chain's assets/ directories"
        }
    }
}

/// [`register_css_urls`] with a resolver that tries the candidate helper paths of each
/// reference in order and registers what `fetch` returns for the first one it finds.
fn register_urls(
    lookup: Lookup<'_>, css: &str, registry: &mut AssetRegistry,
    fetch: &mut dyn FnMut(&str) -> Result<Option<Vec<u8>>>,
) -> Result<String> {
    let mut failure: Option<Error> = None;
    let result = {
        let mut resolve = |rel: &str| -> Option<AssetData> {
            for candidate in lookup.candidates(rel) {
                match fetch(&candidate) {
                    Ok(Some(bytes)) => return Some(AssetData::Bytes(bytes)),
                    Ok(None) => {}
                    Err(err) => {
                        failure.get_or_insert(err);
                        return None;
                    }
                }
            }
            None
        };
        register_css_urls(css, registry, &mut resolve, lookup.rules.lenient)
    };
    if let Some(err) = failure {
        return Err(err);
    }
    result.map_err(|err| match err {
        Error::MissingAsset { path, .. } => Error::Style {
            file: lookup.key.to_string(),
            message: format!(
                "url({}) not found next to the stylesheet{}",
                path.display(),
                lookup.looked_in(&path.to_string_lossy())
            ),
        },
        other => other,
    })
}

/// Absolute targets of `@import` rules (`https://…`, `http://…` or `//…`, quoted or in
/// `url()`), in order. Comments are not skipped (a commented-out import is reported too).
pub(super) fn remote_imports(css: &str) -> Vec<String> {
    static REMOTE_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
        // A constant pattern covered by the unit tests: compiling it cannot fail.
        Regex::new(
            r#"(?i)@import\s+(?:url\(\s*)?["']?\s*((?:https?:)?//[^"'\s);]+)"#,
        )
        .expect("remote import regex is valid")
    });
    REMOTE_IMPORT.captures_iter(css).map(|caps| caps[1].to_string()).collect()
}

/// An `@import` rule found by [`CSS_IMPORT`].
struct ImportRule<'t> {
    /// The rule as written, up to and including its `;`.
    text: &'t str,
    /// The URL as written (quotes removed, trimmed).
    url: &'t str,
}

impl<'t> ImportRule<'t> {
    /// The import of a [`CSS_IMPORT`] match, if it is one.
    fn of(caps: &Captures<'t>) -> Option<ImportRule<'t>> {
        let whole = caps.get(0)?;
        let url = ["dq", "sq", "uq", "sdq", "ssq"]
            .iter()
            .find_map(|group| caps.name(group))?
            .as_str();
        Some(ImportRule { text: whole.as_str(), url: url.trim() })
    }

    /// True if the URL is relative (not empty, without a scheme, not `/…` or `//…`).
    fn is_local(&self) -> bool {
        !self.url.is_empty()
            && !self.url.starts_with(['/', '\\'])
            && !SCHEME.is_match(self.url)
    }
}

/// A URL scheme (`https:`, `data:`, …) at the start of a reference.
static SCHEME: LazyLock<Regex> = LazyLock::new(|| {
    // A constant pattern covered by the unit tests: compiling it cannot fail.
    Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").expect("scheme regex is valid")
});

/// The CSS tokens [`take_imports`] cares about, tried in this order at each position: a
/// comment (to the end of the text when unterminated) or a string literal, both matched to
/// be skipped; an `@import` rule up to its `;` (or the end of the text) whose URL is in
/// `url()` (`dq`/`sq` quoted, `uq` unquoted) or a string (`sdq`/`ssq`), followed by an
/// optional condition (`cond`); an unquoted `url(…)`, skipped (its argument may contain
/// braces); a `brace`, `{` or `}`, to tell top-level rules from nested ones.
static CSS_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    // A constant pattern covered by the unit tests: compiling it cannot fail.
    Regex::new(concat!(
        r"(?s)/\*.*?(?:\*/|\z)",
        r#"|"(?:[^"\\\n]|\\.)*(?:"|\n|\z)|'(?:[^'\\\n]|\\.)*(?:'|\n|\z)"#,
        r"|(?i:@import)\s*(?:(?i:url)\(\s*",
        r#"(?:"(?P<dq>[^"\\\n]*)"|'(?P<sq>[^'\\\n]*)'|(?P<uq>[^"'()\\\s]*))\s*\)"#,
        r#"|"(?P<sdq>[^"\\\n]*)"|'(?P<ssq>[^'\\\n]*)')"#,
        r"(?P<cond>[^;{}]*)(?:;|\z)",
        r#"|(?i:url)\((?:[^"'()\\]|\\.)*\)"#,
        r"|(?P<brace>[{}])",
    ))
    .expect("CSS import regex is valid")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_imports_are_found_in_every_form() {
        let css = r#"@import url("a.css");
@IMPORT url( 'b.css' ) screen and (min-width: 10px);
@import url(./c.css);
@import "d.css" print;
@import 'e.css'
;/* @import url(comment.css); */
a::before { content: "@import url(string.css);" }
@import url(https://fonts.example/x.css);
@import "//cdn.example/y.css";
@import url(/root.css);
@import url(data:text/css,a%7Bcolor:red%7D);
@import url("");
@import url(last.css)"#;
        let imports: Vec<_> = CSS_IMPORT
            .captures_iter(css)
            .filter_map(|caps| ImportRule::of(&caps))
            .filter(ImportRule::is_local)
            .map(|import| import.url)
            .collect();
        assert_eq!(
            imports,
            ["a.css", "b.css", "./c.css", "d.css", "e.css", "last.css"]
        );
    }

    #[test]
    fn top_level_remote_imports_are_taken_out_and_nested_ones_left() {
        let css = "/* @import url(c.css); */\n\
                   @import url(https://a.example/x.css) print;\n\
                   a { content: \"@import 'no';\"; background: url(data:x,{) }\n\
                   @media print { @import url(https://b.example/y.css); @import url(n.css); }\n\
                   @IMPORT \"//c.example/z.css\"";
        let mut imports = Vec::new();
        let mut remote = Vec::new();
        let key = "@t/styles/a.css";
        let out = take_imports(key, css, &mut imports, &mut remote).unwrap();
        assert_eq!(
            imports,
            [
                (
                    key.into(),
                    "@import url(https://a.example/x.css) print;".into()
                ),
                (key.into(), "@IMPORT \"//c.example/z.css\";".into())
            ]
        );
        assert_eq!(
            remote,
            [
                (key.into(), "https://a.example/x.css".into()),
                (key.into(), "//c.example/z.css".into())
            ]
        );
        assert_eq!(
            out,
            "/* @import url(c.css); */\n\na { content: \"@import 'no';\"; background: \
             url(data:x,{) }\n@media print { @import url(https://b.example/y.css); \
             @import url(n.css); }\n"
        );
    }

    #[test]
    fn only_http_and_protocol_relative_imports_are_hoisted() {
        for url in [
            "a.css", "../a.css", "/a.css", "", "data:text/css,a{}",
            "ftp://example.org/a.css",
        ] {
            let css = format!("a {{ color: red }}\n@import \"{url}\";");
            let err = take_imports(
                "@t/styles/a.css",
                &css,
                &mut Vec::new(),
                &mut Vec::new(),
            )
            .unwrap_err();
            assert!(
                matches!(err, Error::Style { ref file, .. } if file == "@t/styles/a.css")
            );
            assert!(err.to_string().contains("line 2:"), "{err}");
            assert!(
                err.to_string().contains("list the file in [styles] entries"),
                "{err}"
            );
        }
    }

    #[test]
    fn remote_imports_are_found_in_every_form() {
        let css = r#"@import "https://fonts.example/a?family=X:400,700";
@IMPORT url( 'http://x.example/b.css' ) screen;
@import url(//cdn.example/c.css);
@import "local.css";
@import url(fonts/d.css);
a { background: url(https://x.example/not-an-import.png) }"#;
        assert_eq!(
            remote_imports(css),
            [
                "https://fonts.example/a?family=X:400,700",
                "http://x.example/b.css",
                "//cdn.example/c.css"
            ]
        );
    }

    #[test]
    fn placeholders_take_the_versioned_url_and_keep_their_own_query() {
        let placeholder = "asset:0123456789ab";
        let css = format!(
            "a {{ b: url({placeholder}); c: url('{placeholder}?x=1#y'); d: url({placeholder}#z) }}"
        );
        assert_eq!(
            replace_placeholder(&css, placeholder, "/u.png?v=5"),
            "a { b: url(/u.png?v=5); c: url('/u.png?v=5&x=1#y'); d: url(/u.png?v=5#z) }"
        );
        // A URL without a query (a builtin file) keeps the reference's query as it is.
        assert_eq!(
            replace_placeholder(&css, placeholder, "/v.woff"),
            "a { b: url(/v.woff); c: url('/v.woff?x=1#y'); d: url(/v.woff#z) }"
        );
    }

    #[test]
    fn legacy_font_mime_types_are_standardized() {
        assert_eq!(standard_font_mime("application/font-woff"), "font/woff");
        assert_eq!(standard_font_mime("application/font-sfnt"), "font/otf");
        assert_eq!(standard_font_mime("font/woff2"), "font/woff2");
        assert_eq!(standard_font_mime("image/png"), "image/png");
    }

    #[test]
    fn decode_text_drops_one_leading_bom() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFa{}".to_vec()).unwrap(), "a{}");
        assert_eq!(
            decode_text(b"\xEF\xBB\xBF\xEF\xBB\xBFa".to_vec()).unwrap(),
            "\u{feff}a"
        );
        assert_eq!(
            decode_text(b"a\xEF\xBB\xBF".to_vec()).unwrap(),
            "a\u{feff}"
        );
        assert!(decode_text(vec![0xff]).is_err());
    }

    #[test]
    fn candidates_follow_the_rules() {
        let chain = ["t", "default"];
        let lookup = |key, rules| Lookup { key, rules, chain: &chain };
        let entry = lookup("@t/styles/a.css", UrlRules::THEME_ENTRY);
        assert_eq!(
            entry.candidates("logo.png"),
            [
                "@t/styles/logo.png", "@t/assets/logo.png",
                "@default/assets/logo.png"
            ]
        );
        // Every `rel` falls back, named in the theme-root form whatever its first segment;
        // a builtin-form `rel` then names the embedded file (§3).
        assert_eq!(
            entry.candidates("vendor/x.png"),
            [
                "@t/styles/vendor/x.png", "@t/assets/vendor/x.png",
                "@default/assets/vendor/x.png", "vendor/x.png"
            ]
        );
        assert_eq!(
            entry.candidates("./writer/../writer/y.js"),
            [
                "@t/styles/writer/y.js", "@t/assets/writer/y.js",
                "@default/assets/writer/y.js", "writer/y.js"
            ]
        );
        assert_eq!(
            entry.looked_in("logo.png"),
            " or in the theme chain's assets/ directories"
        );
        assert_eq!(
            entry.looked_in("./vendor/x.png"),
            ", in the theme chain's assets/ directories or among the built-in files"
        );
        assert_eq!(
            entry.candidates("./writer/../@u\\x.png"),
            [
                "@t/styles/@u/x.png", "@t/assets/@u/x.png",
                "@default/assets/@u/x.png"
            ]
        );
        assert_eq!(
            entry.candidates("fonts/a/a.woff"),
            [
                "@t/styles/fonts/a/a.woff", "@t/assets/fonts/a/a.woff",
                "@default/assets/fonts/a/a.woff"
            ]
        );
        // A `rel` that climbs out of its root has no fallback; duplicates are dropped.
        assert_eq!(entry.candidates("../x.png"), ["@t/x.png"]);
        assert_eq!(
            lookup("@t/assets/a.css", UrlRules::THEME_ENTRY)
                .candidates("x.png"),
            ["@t/assets/x.png", "@default/assets/x.png"]
        );
        let style = UrlRules::for_style("@t/styles/a.css");
        assert!(!style.is_lenient());
        assert_eq!(
            lookup("@t/styles/a.css", style).candidates("logo.png"),
            ["@t/styles/logo.png"]
        );
        assert_eq!(
            lookup("@t/styles/a.css", style).candidates("vendor/x.png"),
            ["@t/styles/vendor/x.png"]
        );
        assert_eq!(
            lookup("@t/styles/a.css", style).looked_in("vendor/x.png"),
            ""
        );
        assert!(UrlRules::for_style("vendor/katex/katex.min.css").is_lenient());
    }
}
