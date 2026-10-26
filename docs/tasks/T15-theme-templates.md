# T15 — Theme chain, document template, context, helpers, styles

| | |
|---|---|
| Milestone | M1 |
| Size | M |
| Depends on | T03, T13, T14 |
| Parallel-safe with | T12, T30, T31, T40 (wave 3) |
| Files you own | `src/theme/mod.rs`, `src/theme/manifest.rs`, `src/theme/resolve.rs`, `src/theme/templates.rs` (replace stubs; lift from presenter_rs/src/theme/{mod,manifest,resolve,templates}.rs); `src/html/mod.rs`, `src/html/context.rs`, `src/html/document.rs`, `src/html/paths.rs`, `src/html/styles.rs` (replace stubs; lift from presenter_rs/src/render/{mod,context,html,paths,styles}.rs); `assets/themes/default/theme.toml`, `assets/themes/default/templates/document.html`, `assets/themes/default/templates/partials/head.html`, `assets/themes/default/templates/partials/scripts.html` (new, or replace T00's stub; T19 takes them over); `tests/fixtures/themes/child-theme/**` (new: `theme.toml`, `templates/partials/head.html`, `styles/child.css`, `assets/logo.svg`, `scripts/child.js`); `tests/theme_resolve.rs`, `tests/html_render.rs`, `tests/snapshots/html_render__context.snap` (new) |

## Goal
`writer::theme` resolves an HTML theme (built-in `default` or a directory) into its `extends` chain with symlink safety,
code themes and `check` problems; `writer::html::render` wraps T13's markup in the theme's `document.html` (minijinja 2.24)
with the helpers `style/script/asset/theme_style/math_init/live_reload`, compiles the theme stylesheet and resolves every
`asset:` placeholder per output mode. Consumers: T16 (pipeline, `dump --context`), T19, T1A, T21, T1C, T72.

## Read first
- `docs/spec/html-output.md` **§3, §4, §5, §6.2 — this is your spec**; §2 (inlining, determinism), §7 first bullet, §8
  (`remote-import`, `theme-*` rows), §9 T15 row (your tests), §10.2–§10.3 and the Deviations on `math.js`, math assets,
  strict theme `url()`s, code CSS only with `features.code`, no `markdown` filter. APIs you call: §1.2, §1.5
  (`builtin_code_theme`, `code_theme_css`), §1.8 (`MathHook`, `MathUsage`, `KatexOnly`), §1.15 (`emit`), §6.1 (T14).
- `live-reload.md` §1.1 (`?v=`), §1.2 (serve hooks 1–3, 6), §6.1; `math.md` §4.3; `config.md` §2 `[html]`/`[math]`, §4;
  `ir.md` §3, §5; `vendored-assets.md` §2, §2.1, §4 (themes row), §5.1–§5.2 (`vendor::get/list`, disk in debug), §7.3.
- `docs/research/R8-presenter-inventory.md` §3.6, §5, §10 steps 18–19, §11; `R6-html-and-crate-versions.md` §A.1, §A.4.
- presenter_rs: `src/theme/{mod,manifest,resolve,templates}.rs`, `src/theme/styles.rs` (only `CSS_IMPORT`/`ImportRule`/`SCHEME`,
  lines 1344–1405, + tests), `src/render/{mod,context,html,paths,styles}.rs`, `tests/theme_resolve.rs`, `tests/render.rs`.

## API to implement
```rust
// src/theme/manifest.rs (§3.2; every table #[serde(default, deny_unknown_fields)]); from_toml_str(text, theme, origin) lifted
pub struct ThemeManifest { pub name: String, pub extends: Option<String>, pub description: Option<String>, pub styles: StylesSection,
    pub scripts: ScriptsSection /* { entries: Vec<String> } */, pub vars: serde_json::Map<String, serde_json::Value> }
pub struct StylesSection { pub entries: Vec<String>, pub code_theme: Option<String>, pub code_theme_dark: Option<String> }
// src/theme/{mod,resolve,templates}.rs (T15); ThemeDir/ThemeFile/ThemeSource as presenter's   — html-output.md §3.3
pub const REQUIRED_TEMPLATES: [&str; 1] = ["document.html"];   // MAX_CHAIN_LEN = 8, MANIFEST_FILE = "theme.toml"
pub fn resolve(spec: &str, search_roots: &[&Path]) -> Result<Theme>;   pub fn builtin_names() -> Vec<String>;
pub struct Theme { pub chain: Vec<ThemeDir> }   // child → root
impl Theme { pub fn name(&self) -> &str; pub fn vars(&self) -> serde_json::Map<String, serde_json::Value>;
    pub fn style_entries(&self) -> Result<Vec<ThemeFile>>; pub fn script_entries(&self) -> Result<Vec<ThemeFile>>;   // parent-first
    pub fn code_themes(&self) -> Result<(Option<CodeTheme>, Option<CodeTheme>)>;   // (light, dark); None = "none"
    pub fn read_template(&self, name: &str) -> Result<Option<(String, String)>>; pub fn read_asset(&self, rel: &str) -> Result<Option<Vec<u8>>>;
    pub fn watch_dirs(&self) -> Vec<PathBuf>; pub fn problems(&self) -> Vec<Diagnostic>;
    pub fn environment(&self, debug: bool) -> Result<minijinja::Environment<'static>>; }
pub struct CodeTheme { pub label: String /* name or @theme/path */, pub theme: syntect::highlighting::Theme }
pub fn recording_reads<R>(f: impl FnOnce() -> R) -> (R, BTreeSet<PathBuf>);   // theme files read (watch set)
pub mod codes { /* THEME_STYLE_ENTRY, THEME_SCRIPT_ENTRY, THEME_CODE_THEME = "theme-code-theme", THEME_TEMPLATE, THEME_LINK_OUTSIDE */ }
// src/html/mod.rs (T15; lifted render/mod.rs): the types below, BuildMode { Build, Serve } (serde lowercase) and
// BuildInfo { mode, standalone, live_reload, id: u64, version: &'static str } with build(standalone)/serve(id) as presenter's,
// plus presenter's re-exports and `pub use document::{render, serve_stylesheet, SERVE_HELPER_PREFIX, SERVE_THEME_CSS};`.
pub struct RenderInput<'a> { pub document: &'a Document, pub assets: &'a mut AssetRegistry, pub theme: &'a Theme,
    pub config: &'a Config /* effective */, pub output: OutputMode, pub build: BuildInfo, pub math: &'a mut dyn MathHook }
pub struct RenderedDocument { pub html: String, pub theme_css: String, pub files: Vec<EmittedFile>,
    pub diagnostics: Diagnostics, pub theme_files: BTreeSet<PathBuf> }
pub fn render(input: RenderInput<'_>) -> Result<RenderedDocument>;   // emit → context → compile CSS → document.html → resolve
pub fn context_json(input: &mut RenderInput<'_>) -> Result<serde_json::Value>;   // dump --context (html/context.rs)
pub const SERVE_HELPER_PREFIX: &str = "/__writer/asset/";   pub const SERVE_THEME_CSS: &str = "/__writer/theme.css";
pub fn serve_stylesheet(theme: &Theme, key: &str, css: &str) -> Result<String>;   // a style() file in serve mode
pub fn resolve_helper_path(theme: &Theme, path: &str) -> Result<Option<(Vec<u8>, String)>>;   // html/paths.rs (lifted)
pub fn compile_theme_css(theme: &Theme, features: &Features, registry: &mut AssetRegistry) -> Result<String>;   // + serve variant
```

## Steps
1. **Check your inputs on `main`** with `grep -n`: `fn emit`, `trait MathHook`, `struct KatexOnly` (src/html/emit.rs);
   `fn builtin_code_theme`, `fn code_theme_css` (src/html/highlight.rs); `fn content_version`, `fn register_css_urls`
   (src/assets/emit.rs); `fn html_count_attr` (src/assets/mod.rs); `EQUATION_CSS` (src/typst/html.rs); `fn template_error`
   (src/error.rs); `fn unknown_key_hint`, `fn with_unknown_key_hint` (src/config.rs). If the last two are absent or private,
   copy presenter's (with `closest`, `levenshtein`) privately into `src/theme/manifest.rs` and say so in the PR. Keep
   T00's `mod` lines in both `mod.rs` files as they are.
2. `src/theme/manifest.rs` ← presenter's (451 LOC): delete `REVEAL_THEMES`, `reveal`, `layouts`, `reveal_theme`,
   `highlight_style`, the reveal-theme check and its two tests; add `code_theme`, `code_theme_dark`. In `from_toml_str`,
   when the text parses as a `toml::Table`, reject presenter keys first with `Error::Theme`: `[styles] highlight_style`
   → a message naming `[styles] code_theme`; `[styles] reveal_theme`, `[reveal]`, `[layouts]` → "… has no meaning in
   writer". Keep `check_theme_name`, `json_map`, `line_of_value`/`line_of_key` and their tests.
3. `src/theme/resolve.rs` ← presenter's (414 LOC) verbatim, minus `not_found_hint`'s `REVEAL_THEMES` branch; root check
   uses the new `REQUIRED_TEMPLATES` (R8 §10 step 18's four templates are superseded by §3.3). `src/theme/templates.rs`
   ← presenter's (460 LOC) verbatim (`environment`, loader, `reached_templates`, `loaded_names`, tests).
4. `src/theme/mod.rs` ← presenter's (1,281 LOC). `THEME_HIGHLIGHT_STYLE` → `THEME_CODE_THEME`; delete `THEME_PLUGINS`,
   `DEFAULT_*`, `BUILTIN_HIGHLIGHT_STYLES`, `reveal_theme`, `highlight_style`, `reveal_defaults`, `highlight_style_file`/
   `_lookup`, `plugin(s)_problem`, `deck_problems`/`deck_templates`, `unknown_layout_message`, `has_layout`, `layouts`,
   `is_layout_name`, `note_read_elsewhere`, the `Config` import; "presenter ignores" → "writer ignores". Keep symlink
   safety, `recording_reads`, `deep_merge`, `closest_name`, `normalize_rel`, `broken_templates`, `template_origin`,
   `entry_problem` (T1C/T72 lift their callers).
5. Same file, `code_themes()`: per key (`code_theme`, `code_theme_dark`) the first theme child → root that sets it wins;
   unset or `"none"` → `None`; a value ending in `.tmTheme` (ASCII case-insensitive) is a path under the entry rules
   (`entry_path_problem`, `ThemeDir::read`, `missing_reason`) read with `ThemeSet::load_from_reader`, label
   `@<theme>/<normalized path>`; any other value must pass `crate::html::highlight::builtin_code_theme` (label = value),
   else `Error::Theme`: "terminal-only theme; its colours are ANSI palette indices" for `ansi`/`base16`/`base16-256`,
   otherwise a `closest_name` hint over the 29 usable two-face names. `problems()` = presenter's `problems_with` with
   `reached = reached_templates(self, REQUIRED_TEMPLATES)` (broken and reached: error; else warning "the build does not
   load it"), a `theme-code-theme` error per failing key (line: `line_of_value(text, "styles", key, value)`) and
   `theme-style-entry` for a style entry not ending in `.css` ("SCSS themes arrive with T79").
6. `src/html/paths.rs` ← `render/paths.rs` (411 LOC): builtin segment `presenter` → `writer`; `content_version` comes from
   `crate::assets::emit` (moved by T14); `SERVE_HELPER_PREFIX` from `super::document`. Keep `VERSIONS`, `serve_url`, tests.
7. `src/html/styles.rs` ← `render/styles.rs` (563 LOC) minus the `reveal_fallback` candidates/messages. New
   `compile(theme, features, targets, remote)`: per style entry, parent-first: not `.css` → `Error::Theme` (T79 note);
   `decode_text`; scan top-level `@import`s with the copied `CSS_IMPORT`/`ImportRule`/`SCHEME` (comments, strings,
   `url()`s, blocks skipped; an import inside a block stays, with presenter's `tracing::warn!`): a URL matching
   `remote_imports`' pattern is cut out, pushed to `remote` and hoisted to the top as `/* hoisted from <key> */\n<rule>;\n`;
   any other → `Error::Style { file: key, message: "line N: @import url(U): local imports are not inlined; list the file
   in [styles] entries" }`; then `rewrite_urls(.., UrlRules::THEME_ENTRY, ..)`, written as `/* <key> */\n` + text (+ `\n`
   if missing). Last, only with `features.code`, §5's code CSS with exactly its strings: light alone `/* code theme: L */\n`
   + css + `\n`; dark `/* code theme (dark): L */\n@media (prefers-color-scheme: dark) {\n` + css + `}\n`; with both, the
   light css sits in `@media not all and (prefers-color-scheme: dark) {\n` … `}\n`.
8. `src/html/mod.rs` ← `render/mod.rs` (148 LOC): `BuildMode`/`BuildInfo` verbatim; the two structs above (no `Clone`/`Debug`).
9. `src/html/context.rs` ← `render/context.rs`, rewritten around one builder shared with `render`:
   `pub(crate) fn build_context(input: &mut RenderInput<'_>, diagnostics: &mut Diagnostics) -> Result<(Emitted, Value)>`
   runs `html::emit::emit` (`positions` = `build.mode == Serve || config.markdown.sourcepos`, `includes` =
   `&document.includes`) and inserts §4.1's keys in order: `document`, `meta`, `features`, `config`, `html` (`main`,
   `title_block`, `body`, `footnotes`, `toc`), `math`, `title` (`meta.title`, else the source stem), `lang` (`meta.lang`,
   else `config.document.lang`, else `"en"`), `theme` (`name`, `chain`, `code_theme`/`code_theme_dark` = labels of
   `code_themes()?` or `"none"`, `vars`, `scripts` = `@<theme>/<rel_path>` of `script_entries()?`), `build`.
10. `src/html/document.rs` ← `render/html.rs` (886 LOC): `presenter-theme` → `writer-theme`, `/__presenter/` →
    `/__writer/`; `SERVE_LIVE_RELOAD_JS` stays public (T21's server imports it). Delete `reveal_init`, `init_script`,
    `check_background_videos`, `slide_count`, the `markdown` filter and its tests; keep the rest (`HelperState`,
    helpers, `attrs`, `json`, `inline_script`, `raw_text`, `leaves_script_open`, `remote-import`). Add `math_init()` exactly
    as §4.3 (state's `style`/`script`/`asset`, `script_safe_json`, `crate::typst::html::EQUATION_CSS`) and a flag set by
    `live_reload()`. `render` in `recording_reads` (→ `theme_files`): `build_context` → `theme_css(&document.features)` →
    environment (`debug` = serve) + helpers → `document.html` → `html_count_attr(&html, "data-writer-document") != 1` ⇒
    `Error::Template` for `document.html` → serve without `live_reload()` ⇒ warning `theme-template` → one `Emitter` pass.
11. Default theme (§7, §4.4): `theme.toml` = `name = "default"`, `[styles] entries = []`, `code_theme =
    "InspiredGitHub"`, `code_theme_dark = "OneHalfDark"`, `[scripts] entries = []`. `partials/head.html`: charset,
    viewport, `<title>{{ title }}</title>`, description/author/keywords metas only when set (no generator meta).
    `partials/scripts.html`: `{{ script(path) }}` per `theme.scripts`, then `{{ live_reload() }}`. `document.html`:
    `<!DOCTYPE html>`, `<html lang="{{ lang }}">`, blocks in this order: `head` (includes `partials/head.html`),
    `styles` (`{{ math_init() }}`, then `{{ theme_style() }}`, the last stylesheet; both in `<head>`), `body_start`, `main`
    (`{{ html.main|safe }}`, once), `scripts` (includes `partials/scripts.html`), `body_end`.
12. Fixture `tests/fixtures/themes/child-theme/` (§9): `theme.toml` (`name = "child-theme"`, `extends = "default"`, `[styles]
    entries = ["styles/child.css"]`, `code_theme = "Nord"`, `[scripts] entries = ["scripts/child.js"]`, `[vars] accent =
    "#aa3300"`); `templates/partials/head.html` = `{% include "@default/partials/head.html" %}` + `<meta name="x-child"
    content="{{ theme.vars.accent }}">`; `styles/child.css` with `url(logo.svg)` (via `assets/`); `assets/logo.svg`; `scripts/child.js`.
13. Tests: lift presenter's `tests/theme_resolve.rs` and `tests/render.rs` (→ `tests/html_render.rs`) minus layout,
    highlight-style, reveal, plugin, markdown-filter and video tests; add the criteria below. Document: T01's
    `ir/sample-document.json` with `asset:32debefb8003` replaced by the placeholder of bytes the test registers; hook
    `KatexOnly { usage: MathUsage::default() }` or a test-local `FakeMath(MathUsage)`; variants are temp copies of the
    fixture, helpers called from a child `document.html` extending `@default/document.html` (block `body_end`).

## Acceptance criteria
- `tests/theme_resolve.rs` (fixture `themes/child-theme`, T15): `child_theme_chain_and_lookup` (chain
  `["child-theme","default"]`; `partials/head.html` from the child, `document.html` and `@default/partials/head.html` from
  default; entries `styles/child.css`/`scripts/child.js`; `vars()["accent"] == "#aa3300"`; labels `("Nord","OneHalfDark")`;
  `watch_dirs()` = the child's canonical dir); `builtin_default_resolves_without_search_roots` (`InspiredGitHub`/
  `OneHalfDark`, `builtin_names() == ["default"]`); `resolution_errors_are_theme_errors` (unknown name lists tried dirs
  and `built-in themes: default`; cycle; 9 themes; duplicate names; root without `templates/document.html`);
  `presenter_keys_are_rejected`; `code_theme_values_follow_the_rules` (`.tmTheme`/`.TMTHEME` → label
  `@t/styles/code.tmTheme`; `"none"`; `Nrod` → "did you mean 'Nord'?"; `ansi`, `base16`, `base16-256` → "terminal-only
  theme"; missing file); `symbolic_links_out_of_a_theme_are_refused` (`#[cfg(unix)]`); `problems_find_what_a_build_fails_on`
  (missing and `.scss` style entry, missing script, bad code theme, broken `document.html`: errors located in
  `theme.toml`/the template; broken unreached partial, link out of the theme: warnings); `disk_themes_are_read_fresh_and_recorded`.
- `tests/html_render.rs`: `context_has_the_spec_shape` (§4.1 key order; `theme.scripts == ["@child-theme/scripts/child.js"]`;
  `html.main` starts `<main data-writer-document>` and keeps `asset:`) + `tests/snapshots/html_render__context.snap`;
  `helpers_in_three_modes` (each row of §4.2; serve `live_reload()` = `<script src="/__writer/live-reload.js"></script>`);
  `standalone_output_is_self_contained` (no `asset:`, no `/__writer/`, has `<meta name="x-child" content="#aa3300">`);
  `site_output_writes_hashed_files` (`assets/theme-<hash>.css`, logo, `child-<hash>.js`); `serve_output_uses_dev_server_urls`
  (`BuildInfo::serve(7)` → `<link id="writer-theme" rel="stylesheet" href="/__writer/theme.css?v=7">`, `data-sourcepos`,
  `theme_css` with `/__writer/asset/@child-theme/assets/logo.svg?v=`); `code_css_only_with_features_code` (none without
  it; with it Nord wrapped in `@media not all and (prefers-color-scheme: dark) {`); `math_init_follows_math_usage` (none →
  no `writer-math`/KaTeX; katex → `window.__writerKatexOptions = …;` with `<` written `\u003c`; mathml + `embed` →
  `data:font/woff2;base64,`; `system` → no `@font-face`; typst_mathml → `EQUATION_CSS`; serve →
  `/__writer/asset/writer/math.js`); `stylesheet_imports_and_entries` (relative `@import`, bad `url()` → `Error::Style`;
  absolute `@import` hoisted, `remote-import` only in standalone; `.scss` → `Error::Theme`); `tmtheme_code_theme_is_compiled`;
  `document_html_needs_exactly_one_morph_root` (0 and 2 → `Error::Template` for `document.html`);
  `serve_render_without_live_reload_warns`; `inlined_scripts_never_close_early` (`</script` → `<\/script`);
  `unknown_helper_path_is_a_template_error`; `render_is_deterministic` (two renders byte-identical; `theme_files` has
  `child.css`, `logo.svg`, `head.html`).
- Lifted unit tests (`cargo test --lib theme::`, `cargo test --lib html::`), `cargo fmt --check`, `cargo clippy
  --all-targets --all-features -- -D warnings`, `cargo test`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
Markup (T13), the asset emitter and `html_count_attr` (T14), `MathRenderer` and `assets/writer/math.js` (T1A), theme CSS,
README and `tests/theme_tokens.rs` (T19), `pipeline::resolve_theme`, `dump --context`, fixture snapshots, size budgets
(T16), routes (T21), `check` (T1C), `eject theme` (T72), SCSS and minijinja 3 (T79). No `markdown` filter, no `toc()`.

## Hints (verified)
- minijinja 2.24.0 (`src/environment.rs`): `set_loader(move |name: &str| -> Result<Option<String>, minijinja::Error>)`,
  `add_function`, `add_filter`, `set_undefined_behavior`, `set_auto_escape_callback`, `set_debug`, `get_template`;
  `Value::from_safe_string(String)`, `Value::from_serialize(T)`, `minijinja::filters::tojson`. Functions are
  `Send + Sync + 'static`, hence presenter's `Arc<Mutex<HelperState>>` (render/html.rs module docs).
- syntect 5.3.0: `ThemeSet::load_from_reader<R: BufRead + Seek>(r: &mut R) -> Result<Theme, LoadingError>` (feature
  `plist-load`, in `default-fancy`), called with `&mut std::io::Cursor::new(bytes)`. Probe (2026-10-25): this file parses
  (`.hl-code { color: #111111; … }` + `.hl-comment` via `css_for_theme_with_class_style`), garbage is `Err`; tests' tmTheme:
  `<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>name</key><string>Tiny</string><key>settings</key><array><dict><key>settings</key><dict><key>background</key><string>#FFFFFF</string><key>foreground</key><string>#111111</string></dict></dict><dict><key>scope</key><string>comment</string><key>settings</key><dict><key>foreground</key><string>#888888</string></dict></dict></array></dict></plist>`
- two-face 0.5.2: `two_face::theme::EmbeddedLazyThemeSet::theme_names() -> &'static [EmbeddedThemeName]` (`Copy`),
  `as_name(self) -> &'static str`: 32 names (R6 §A.1), terminal ones `ansi`, `base16`, `base16-256`. CSS via T13 only.
- `script_safe_json(json) = json.replace('<', "\\u003c")` (§10.3); `serde_json::to_string(&config.math.katex_options)` is
  `{}` when empty; serde_json 1.0.154 with `preserve_order` (vendored-assets.md §7.3) keeps §4.1's key order in a `Map`.
- Vendor paths: `vendor/katex/katex.min.css`, `vendor/katex/katex.min.js`, `vendor/latin-modern-math/latinmodern-math.woff2`;
  `writer/math.js` is T00's placeholder. `style()` of a `vendor/…` sheet is lenient, so KaTeX's `.woff`/`.ttf` fallbacks stay.
- rust-embed 8.13.0 reads `assets/` from disk in debug builds, exact-case keys (vendored-assets.md §5.2): edits to
  `assets/themes/default/*` show in tests without rebuilding. Dev profile `[profile.dev.package."*"] opt-level = 2` (T00);
  two-face loads in ≈35 ms in debug (R6 §A.1), so share one resolved theme per test; no test here runs the compiler.
- insta 1.49.0: `insta::assert_json_snapshot!("context", value, { ".build.version" => "[version]",
  ".document.source.path" => "[path]", ".document.source.base_dir" => "[path]" })`; review with `cargo insta review`.

## Notes for the coordinator
- html-output.md §4.1's JSON lists `build.version` before `build.id`; §4.4's `BuildInfo { …, id, version }` serializes
  `id` first: the task keeps the struct order. §5 appends `\n` after each entry's text; presenter's lifted `compile` only
  when missing: the task keeps the lifted rule. `SERVE_LIVE_RELOAD_JS` stays public (T21 imports it; §4.4 lists two).
- `theme.code_theme` comes from `code_themes()?` (as presenter's context read `highlight_style_file()?`), so a broken
  `code_theme` fails every HTML build, not only documents with code; only the CSS depends on `features.code`.
- `config::{unknown_key_hint, with_unknown_key_hint}` are not in config.md §4; step 1 falls back to a private copy.

## Amendments
- **2026-10-26 (coordinator):** the user authorized stacked feature PRs while CI
  and merging proceed. T15 starts from completed T13/T17 (`9b315d0`) without
  waiting for PR #7 to merge; its changes remain in a separate feature PR.
- **2026-10-26 (coordinator):** confirmed lifted `BuildInfo` field order (`id`
  before `version`) and CSS entry newline handling (append only when missing);
  updated html-output.md to agree. Corrected the stale example asset id to the
  canonical sample-document.json placeholder.
- **2026-10-26 (coordinator):** keep the lifted helper error boundary: a failing
  template `style()` call is `Error::Template` containing its underlying CSS
  detail; direct stylesheet compilation remains `Error::Style`. The spec now
  states the distinction.
- **2026-10-26 (coordinator):** review refinements to lifted code retain guarded
  manifest reads during diagnostics, decode static template string literals for
  reachability, and record guarded CSS asset version-cache reads in serve mode.
  The read-recording seam is crate-private and preserves cache hits.
