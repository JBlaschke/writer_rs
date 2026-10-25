# R6 — HTML output, live preview, and verified crate versions

**Status:** research notes for the writer plan (2026-10-24). Everything marked **[verified]** was
seen in crate sources under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`, in `cargo info`
output, in a running probe, or in a fetched page/API; **[unverified]** means recalled or inferred
and must be checked before a task depends on it.

Machine: macOS, rustc/cargo 1.99.1 (Homebrew), Google Chrome 155.0.8059.86 (`PlistBuddy` on
`/Applications/Google Chrome.app`), pandoc 3.12.1, no typst CLI.

Scratch crates (kept, re-runnable):

- `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/syntect-probe/`
  (`Cargo.toml`, `src/main.rs`; build with
  `CARGO_TARGET_DIR=/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/target cargo run --release`).
- Probe output: `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/probe-output.txt`
- `cargo info` dump for every crate: `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/cargo-info.txt`

---

## A.1 syntect: server-side syntax highlighting

### Versions and features [verified: `cargo info`, crate `Cargo.toml`, probe `Cargo.lock`]

| crate | latest | license | rust-version | notes |
|---|---|---|---|---|
| syntect | **5.3.0** | MIT | not declared (edition 2021) | default feature = `default-onig` (C dependency `onig` 6.5.3 / `onig_sys`) |
| two-face | **0.5.2+bat-0.26.1** | MIT OR Apache-2.0 | 1.79.0 | repo moved to `https://codeberg.org/CosmicHarper/two-face`; default feature = `syntect-onig` |
| fancy-regex (pulled by `regex-fancy`) | 0.16.2 | — | — | pure Rust |

syntect's feature table (from its `Cargo.toml`):

```
default        = ["default-onig"]
default-fancy  = ["parsing","default-syntaxes","default-themes","html","plist-load","yaml-load","dump-load","dump-create","regex-fancy"]
default-onig   = [same list but "regex-onig"]
default-syntaxes = ["parsing","dump-load"]
default-themes = ["dump-load"]
html           = ["parsing"]
parsing        = ["regex-syntax","fnv","dump-create","dump-load"]
regex-fancy    = ["fancy-regex"]
regex-onig     = ["onig"]
```

two-face's feature table: `default = ["syntect-onig"]`, `syntect-fancy = ["syntect/regex-fancy"]`,
`syntect-default-fancy = ["syntect-fancy","syntect/default-fancy"]`, and a `compile_error!` if neither
`syntect-onig` nor `syntect-fancy` is set. two-face picks its `generated/syntaxes-fancy-newlines.bin`
dump when `syntect-onig` is **off** (`#[cfg(not(feature = "syntect-onig"))]` in `src/syntax.rs`), so
the two crates must agree.

**The dependency lines that build without `onig`** (probe `Cargo.lock` contains zero `onig` entries):

```toml
syntect  = { version = "5.3.0", default-features = false, features = ["default-fancy"] }
two-face = { version = "0.5.2", default-features = false, features = ["syntect-fancy"] }
# if comrak's built-in adapter is used (see below):
comrak   = { version = "0.57.0", default-features = false, features = ["attributes", "syntect-fancy"] }
```

`default-fancy` also drags in `plist`, `yaml-rust`, `serde_json` (for loading `.sublime-syntax`/`.tmTheme`
from disk). A smaller set `["default-syntaxes","default-themes","html","regex-fancy"]` should work
(`html` implies `parsing`) but was **not compiled** here [unverified].

comrak 0.57.0's own syntect dependency is `syntect = "5"`, `default-features = false`,
`features = ["default-themes","default-syntaxes","html"]` (non-wasm targets); it unifies with 5.3.0
(one `syntect 5.3.0` in the probe lock), so a two-face `SyntaxSet` can be passed to comrak's adapter.

### Verified API (copied from `syntect-5.3.0/src/html.rs`, `easy.rs`, `highlighting/style.rs`, `parsing/syntax_set.rs`, `dumps.rs`)

```rust
// syntect::html
pub struct ClassedHTMLGenerator<'a> { /* private */ }
impl<'a> ClassedHTMLGenerator<'a> {
    pub fn new_with_class_style(syntax_reference: &'a SyntaxReference, syntax_set: &'a SyntaxSet, style: ClassStyle) -> ClassedHTMLGenerator<'a>;
    /// requires `line` to end with '\n' and a `load_defaults_newlines()` syntax set
    pub fn parse_html_for_line_which_includes_newline(&mut self, line: &str) -> Result<(), Error>;
    pub fn finalize(mut self) -> String;              // closes open <span>s
}
#[non_exhaustive]
pub enum ClassStyle { Spaced, SpacedPrefixed { prefix: &'static str } }   // prefix MUST be &'static str
pub fn css_for_theme_with_class_style(theme: &Theme, style: ClassStyle) -> Result<String, Error>;
pub fn highlighted_html_for_string(s: &str, ss: &SyntaxSet, syntax: &SyntaxReference, theme: &Theme) -> Result<String, Error>; // whole <pre style="background-color:#..">…</pre>
pub fn start_highlighted_html_snippet(t: &Theme) -> (String, Color);     // "<pre style=\"background-color:#rrggbb;\">\n"
pub enum IncludeBackground { No, Yes, IfDifferent(Color) }
pub fn styled_line_to_highlighted_html(v: &[(Style, &str)], bg: IncludeBackground) -> Result<String, Error>;
pub fn append_highlighted_html_for_styled_line(v: &[(Style, &str)], bg: IncludeBackground, s: &mut String) -> Result<(), Error>;

// syntect::easy
pub struct HighlightLines<'a> { /* private */ }
impl<'a> HighlightLines<'a> {
    pub fn new(syntax: &SyntaxReference, theme: &'a Theme) -> HighlightLines<'a>;
    pub fn highlight_line<'b>(&mut self, line: &'b str, syntax_set: &SyntaxSet) -> Result<Vec<(Style, &'b str)>, Error>;
    pub fn from_state(theme: &'a Theme, highlight_state: HighlightState, parse_state: ParseState) -> HighlightLines<'a>;
    pub fn state(self) -> (HighlightState, ParseState);
}

// syntect::highlighting
pub struct Style { pub foreground: Color, pub background: Color, pub font_style: FontStyle }
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
pub struct FontStyle { /* bits */ }   // FontStyle::BOLD, ::UNDERLINE, ::ITALIC; .contains(FontStyle::BOLD)
pub struct ThemeSet { pub themes: BTreeMap<String, Theme> }
impl ThemeSet { pub fn load_defaults() -> ThemeSet; pub fn get_theme<P: AsRef<Path>>(path: P) -> Result<Theme, LoadingError>; }
// theme.settings.background: Option<Color>, theme.settings.foreground: Option<Color>, theme.name: Option<String>

// syntect::parsing
impl SyntaxSet {
    pub fn load_defaults_newlines() -> SyntaxSet;      // use this one with the html module
    pub fn load_defaults_nonewlines() -> SyntaxSet;
    pub fn syntaxes(&self) -> &[SyntaxReference];
    pub fn find_syntax_by_name<'a>(&'a self, name: &str) -> Option<&'a SyntaxReference>;
    pub fn find_syntax_by_extension<'a>(&'a self, extension: &str) -> Option<&'a SyntaxReference>;
    pub fn find_syntax_by_token<'a>(&'a self, s: &str) -> Option<&'a SyntaxReference>;   // name (case-insensitive) or extension
    pub fn find_syntax_by_first_line<'a>(&'a self, s: &str) -> Option<&'a SyntaxReference>;
    pub fn find_syntax_plain_text(&self) -> &SyntaxReference;
}
pub struct SyntaxReference { pub name: String, pub file_extensions: Vec<String>, /* … */ }

// syntect::util
pub struct LinesWithEndings<'a>; // LinesWithEndings::from(text) yields lines including their '\n'

// syntect::Error (thiserror): LoadingError, ParsingError, ScopeError, Fmt, Io
```

### two-face API [verified: `two-face-0.5.2+bat-0.26.1/src/{syntax.rs,theme/mod.rs,theme/core_types.rs,acknowledgement/*}`]

```rust
pub fn two_face::syntax::extra_newlines() -> SyntaxSet;      // include_bytes! of generated/syntaxes-fancy-newlines.bin (940 KiB on disk)
pub fn two_face::syntax::extra_no_newlines() -> SyntaxSet;
pub fn two_face::theme::extra() -> EmbeddedLazyThemeSet;     // generated/themes.bin (64 KiB)
impl EmbeddedLazyThemeSet {
    pub fn get(&self, name: EmbeddedThemeName) -> &Theme;    // infallible, lazily deserialized
    pub fn theme_names() -> &'static [EmbeddedThemeName];
}
impl EmbeddedThemeName { pub fn as_name(self) -> &'static str; }   // e.g. Github => "GitHub", InspiredGithub => "InspiredGitHub", Nord => "Nord"
pub struct LazyThemeSet; impl LazyThemeSet { pub fn get(&self, name: &str) -> Option<&Theme>; pub fn theme_names(&self) -> impl Iterator<Item=&str>; }
pub fn two_face::acknowledgement::listing() -> Acknowledgements;   // .to_md() -> String: the licenses of the bundled syntaxes/themes (ship it in `writer --licenses`)
pub mod two_face::re_exports { pub use syntect; }
```

### Probe results [verified: ran `syntect-probe`]

- syntect defaults: **75 syntaxes, 7 themes** (`InspiredGitHub`, `Solarized (dark)`, `Solarized (light)`,
  `base16-eighties.dark`, `base16-mocha.dark`, `base16-ocean.dark`, `base16-ocean.light`).
- two-face (fancy): **213 syntaxes, 32 themes** (`ansi`, `base16`, `base16-256`, `Catppuccin Frappe/Latte/Macchiato/Mocha`,
  `Coldark-Cold/Dark`, `DarkNeon`, `Dracula`, `GitHub`, `gruvbox-dark/light`, `InspiredGitHub`, `1337`, `Monokai Extended`
  (+Bright/Light/Origin), `Nord`, `OneHalfDark/Light`, `Solarized (dark/light)`, `Sublime Snazzy`, `TwoDark`, `zenburn`, and the 7 above).
- Load time, release build (`opt-level=3`, thin LTO): syntect defaults **0.67–0.71 ms**, two-face **1.5–3.2 ms**, themes 0.2–0.4 ms.
  Debug build: 15.5 ms / 34.8 ms. Load once into a `std::sync::LazyLock<SyntaxSet>`.
- Binary size (release, `strip = true`, thin LTO, `codegen-units = 1`, probe links syntect defaults **and** two-face):
  **3,928,224 B with two-face vs 2,854,768 B without = +1,073,456 B**. That delta includes two-face's syntax dump
  (937 KiB) + themes (61 KiB) + code; a writer that loads only two-face (not syntect's defaults) pays ≈ +0.6 MiB net
  over syntect-defaults-only (two-face README: 937 KiB vs 360 KiB) [the 0.6 MiB figure is from the README, not measured].

`find_syntax_by_token` lookups (token → syntax name [extensions]); `-` = not found:

| token | syntect 5.3.0 defaults | two-face 0.5.2 (fancy) |
|---|---|---|
| python / py | Python | Python |
| rust / rs | Rust | Rust |
| julia / jl | **-** | Julia [jl] |
| toml | **-** | TOML [toml,tml,Cargo.lock,…] |
| yaml / yml | YAML | YAML |
| bash / sh / zsh | Bourne Again Shell (bash) | Bourne Again Shell (bash) |
| fish | → bash (fish is a bash extension there) | Fish [fish] |
| json | JSON | JSON (also `ipynb`) |
| c | C | C |
| cpp / c++ | C++ | C++ |
| javascript / js | JavaScript | JavaScript |
| typescript / ts | **-** | TypeScript [ts,mts,cts] |
| tsx | **-** | TypeScriptReact |
| jsx | **-** | **-** (JavaScript (Babel) is excluded under fancy) |
| sql | SQL | SQL |
| r | R | R |
| matlab | MATLAB | MATLAB |
| typst / typ | **-** | Typst [typ] |
| fortran | **-** | **-** (token); `f90` → Fortran (Modern) [f90,F90,f95,f03,f08] |
| go, java, html, css, markdown/md, tex/latex, bibtex/bib, makefile, diff, xml, scala, haskell, lua, perl, php, ruby, txt | present | present |
| scss, ini, csv, dockerfile, kotlin, swift, nix, zig, cmake, graphql, proto, nginx, terraform/hcl, verilog, vhdl, wgsl, glsl, asm, awk, jinja/jinja2 | **-** | present |
| powershell / ps1 | **-** | **-** (excluded under fancy) |
| text, console, shell | **-** | **-** |
| h, m | Objective-C | Objective-C |

Consequences for writer: alias `console`/`shell`/`sh` → `bash`, `text`/`plain` → plain text, `jsx` → `javascript`,
`fortran` → `f90`, `ps1`/`powershell` → plain text; unknown tokens fall back to `find_syntax_plain_text()`
(comrak's adapter additionally tries `find_syntax_by_first_line`).

Class-mode output for Python, prefix `hl-` (probe; note nested spans and that the newline stays inside the last span):

```html
<span class="hl-source hl-python"><span class="hl-meta hl-function hl-python"><span class="hl-storage hl-type hl-function hl-python">def</span> <span class="hl-entity hl-name hl-function hl-python"><span class="hl-meta hl-generic-name hl-python">f</span></span></span>…
    <span class="hl-comment hl-line hl-number-sign hl-python"><span class="hl-punctuation hl-definition hl-comment hl-python">#</span> comment
</span>    <span class="hl-keyword hl-control hl-flow hl-return hl-python">return</span> …
```

A missing language (typst via syntect defaults) yields `<span class="hl-text hl-plain">…</span>` (escaped, no spans);
via two-face: `<span class="hl-text hl-typst"><span class="hl-meta hl-expression hl-typst">…`.

Inline-style output (`highlighted_html_for_string`, theme `InspiredGitHub`, Rust):

```html
<pre style="background-color:#ffffff;">
<span style="font-weight:bold;color:#a71d5d;">pub fn </span><span style="font-weight:bold;color:#795da3;">main</span><span style="color:#323232;">() {
</span>…<span style="font-style:italic;color:#969896;">// c
</span>…</pre>
```

Generated stylesheet (`css_for_theme_with_class_style(InspiredGitHub, SpacedPrefixed{prefix:"hl-"})`, 15,772 bytes):

```css
/*
 * theme "GitHub" generated by syntect
 */

.hl-code {
 color: #323232;
 background-color: #ffffff;
}

.hl-comment {
 color: #969896;
font-style: italic;
}
.hl-string {
 color: #183691;
}
…
```

**Gotcha:** the root rule is `.{prefix}code` (`.hl-code`), so writer's `<pre>`/`<code>` wrapper must carry class
`hl-code` for the theme's foreground/background to apply; the generator itself never writes that class.

Tokens API for DOCX runs (`HighlightLines::highlight_line`), Rust, `(fg, bold/italic, text)` per region:

```
[#a71d5d B "pub"] [#323232 " "] [#a71d5d B "fn"] [#323232 " "] [#795da3 B "main"] [#323232 "("] [#323232 ")"] [#323232 " "] [#323232 "{"] [#323232 "\n"]
[#323232 "    "] [#a71d5d B "let"] [#323232 " s "] [#a71d5d B "="] [#323232 " "] [#183691 "\""] [#183691 "hi"] [#183691 "\""] [#323232 ";"] [#323232 " "] [#969896 I "//"] [#969896 I " c\n"]
```

Regions include the trailing `"\n"`; adjacent regions with equal `Style` are *not* merged (merge them before
emitting `<w:r>` runs; `append_highlighted_html_for_styled_line` does that merge for HTML).

### comrak's built-in syntect adapter [verified: `comrak-0.57.0/src/plugins/syntect.rs`, `src/adapters.rs`]

```rust
// feature "syntect" / "syntect-fancy" / "syntect-onig"
pub struct comrak::plugins::syntect::SyntectAdapter;           // implements SyntaxHighlighterAdapter
impl SyntectAdapter { pub fn new(theme: Option<&str>) -> Self; } // None => CSS classes (ClassStyle::Spaced, no prefix)
pub struct SyntectAdapterBuilder;
impl SyntectAdapterBuilder {
    pub fn new() -> Self;
    pub fn theme(mut self, s: &str) -> Self;                      // inline styles, theme looked up by name in the ThemeSet (panics on unknown name: `self.theme_set.themes[theme]`)
    pub fn css(mut self) -> Self;                                 // classes, ClassStyle::Spaced
    pub fn css_with_class_prefix(self, prefix: &'static str) -> Self;   // classes, SpacedPrefixed
    pub fn syntax_set(mut self, s: SyntaxSet) -> Self;            // e.g. two_face::syntax::extra_newlines()
    pub fn theme_set(mut self, s: ThemeSet) -> Self;
    pub fn build(self) -> SyntectAdapter;                         // defaults: theme None, load_defaults_newlines(), ThemeSet::load_defaults()
}
pub trait comrak::adapters::SyntaxHighlighterAdapter: Send + Sync {
    fn write_highlighted(&self, output: &mut dyn fmt::Write, lang: Option<&str>, code: &str) -> fmt::Result;
    fn write_pre_tag(&self, output: &mut dyn fmt::Write, attributes: HashMap<&'static str, Cow<'_, str>>) -> fmt::Result;
    fn write_code_tag(&self, output: &mut dyn fmt::Write, attributes: HashMap<&'static str, Cow<'_, str>>) -> fmt::Result;
}
pub trait comrak::adapters::CodefenceRendererAdapter: Send + Sync {   // per-language fence renderer (mermaid, typst, …)
    fn write(&self, output: &mut dyn fmt::Write, lang: &str, meta: &str, code: &str, sourcepos: Option<Sourcepos>) -> fmt::Result;
}
// comrak::options::Plugins { render: RenderPlugins { codefence_renderers: HashMap<String, &'p dyn CodefenceRendererAdapter>,
//                                                     codefence_syntax_highlighter: Option<&'p dyn SyntaxHighlighterAdapter>, .. } }
pub fn comrak::markdown_to_html_with_plugins(md: &str, options: &Options, plugins: &options::Plugins) -> String;
pub fn comrak::parse_document<'a>(arena: &'a Arena<'a>, md: &str, options: &Options) -> Node<'a>;
pub fn comrak::format_html_with_plugins(root, options, &mut out, plugins) -> fmt::Result;
```

**Gotcha (verified in source):** in CSS-class mode `SyntectAdapter::write_pre_tag` ignores the attribute map and
writes `<pre class="syntax-highlighting">` — dropping comrak's `data-sourcepos` and `lang` attributes (comrak does
pass them: `highlighter.write_pre_tag(context, pre_attributes)` after inserting `data-sourcepos`). In theme mode
it keeps them and prepends `background-color:…` to `style`. `write_highlighted` splits the info string on `,`
(`rust,ignore` → `rust`). Recommendation: implement writer's own 3-method `SyntaxHighlighterAdapter` (copy comrak's,
keep the attribute map, add the `hl-code` class and writer's token aliases) rather than use the built-in one; or
bypass the adapter entirely by rendering fences from writer's own IR (then `render.sourcepos` is still available on
the node: `node.data().sourcepos`).

---

## A.2 Live preview: DOM patching and source-position mapping

### Candidates [verified: npm registry `/latest`, jsdelivr file listing and byte counts on 2026-10-24]

| library | latest (published) | license | browser file | bytes (gzip -9) | matching | maintenance |
|---|---|---|---|---|---|---|
| morphdom | 2.7.8 (2026-01-14) | MIT | `dist/morphdom-umd.min.js` | **12,396 (3,085)**; ESM `dist/morphdom-esm.js` 28,936 (7,149) | sibling-level `id` (`getNodeKey`) | active |
| idiomorph | 0.8.0 (2026-09-05) | 0BSD (npm field) | `dist/idiomorph.min.js` | 10,587 (3,772); ESM 53,363 unminified | id-sets (descendant ids count), `morphStyle: innerHTML/outerHTML`, head merging, `restoreFocus` | active (htmx project) |
| nanomorph | 5.4.3 (2021-02-18) | MIT | none — CommonJS `index.js` 4,424 B + `lib/`, requires `nanoassert` | needs a bundler | `isSameNode()` opt-out, `id` | no release since 2021 |
| replace `<main>` + restore `scrollY` | — | — | 0 | — | none | — |

morphdom README (fetched): `morphdom(fromNode, toNode | htmlString, options) -> Node`; options `getNodeKey(node)`
(default: `id`), `onBeforeElUpdated(fromEl, toEl) -> false` to skip a subtree, `childrenOnly`, `onBeforeNodeAdded`,
`onNodeAdded`, `onBeforeNodeDiscarded`, `onNodeDiscarded`, `onBeforeElChildrenUpdated`, `addChild`, `skipFromChildren`;
FAQ trick `onBeforeElUpdated: (a, b) => !a.isEqualNode(b)` skips unchanged subtrees. Benchmarks in README are Chrome 72
("large" 2.88 ms). idiomorph README says it is "not designed to be as fast as either morphdom or nanomorph" (~10 % slower
on large morphs).

**Recommendation: vendor morphdom 2.7.8 `dist/morphdom-umd.min.js` (12 KB, MIT)** and morph `document.querySelector('main')`
with `childrenOnly: true` + the `isEqualNode` short-circuit. Why not the simpler replace-`<main>`: a morph keeps
`<details open>`, text selection, scroll positions of overflowing `<pre>`/tables, `<img>` elements (no reload flash)
and the reader's `scrollY` without bookkeeping. idiomorph's extra id-set smarts matter for apps that move keyed
subtrees; a rendered document has few ids (headings) and gains nothing for +25 % gzip and a 0.x version. nanomorph is
unbundled and dormant. Keep a full `location.reload()` fallback (as presenter does) when the page outside `<main>`
changed (signature compare, lifted from `assets/presenter/live-reload.js`'s `signature()`).

Caveat with `data-sourcepos` (below): an edit near the top shifts the line numbers of every later block, so their
attributes differ and the `isEqualNode` short-circuit no longer fires for them (morphdom then walks them and patches
only the attribute). Acceptable; if it ever matters, compare with a clone stripped of `data-sourcepos`.

### Scrolling the changed block into view [verified: comrak 0.57.0 output]

`comrak::Options::render.sourcepos = true` emits `data-sourcepos="L1:C1-L2:C2"` (1-based lines; columns in UTF-8
bytes, or characters with `parse.sourcepos_chars = true`) — `Sourcepos`'s `Display` is `"{}:{}-{}:{}"` of
`start.line, start.column, end.line, end.column` (`comrak-0.57.0/src/nodes.rs:904`). Probe output:

```html
<h1 data-sourcepos="1:1-1:7">Title</h1>
<p data-sourcepos="3:1-3:33">A paragraph with <em data-sourcepos="3:18-3:21">em</em> and <code data-sourcepos="3:27-3:32">code</code>.</p>
<ul data-sourcepos="5:1-6:10">
<li data-sourcepos="5:1-5:10">item one</li>
<li data-sourcepos="6:1-7:0">item two</li>
</ul>
<pre data-sourcepos="8:1-10:3"><code class="language-python">print(1)
</code></pre>
<blockquote data-sourcepos="12:1-12:7">
<p data-sourcepos="12:3-12:7">quote</p>
</blockquote>
<table data-sourcepos="14:1-16:9">
<thead>
<tr data-sourcepos="14:1-14:9">
<th data-sourcepos="14:2-14:4">a</th>
…
<p data-sourcepos="18:1-18:28">Last para with footnote<sup data-sourcepos="18:24-18:27" class="footnote-ref">…</sup>.</p>
<p data-sourcepos="22:1-22:7"><span data-math-style="display" data-sourcepos="22:1-22:7">x^2</span></p>
<section data-sourcepos="20:1-20:11" class="footnotes" data-footnotes>
```

Emitted on: `h1–h6`, `p`, `ul`/`ol`, `li`, `pre` (fenced/indented code; also handed to a `SyntaxHighlighterAdapter`
and, as `Option<Sourcepos>`, to a `CodefenceRendererAdapter`), `blockquote`, `table`, `tr`, `th`, `td`, `em`,
`strong`, `code`, `a`, `img`, `hr`, `br`, `sup`/`sub`, `del`, `mark`, `ins`, `dl`/`dt`/`dd`, tasklist `input`,
`section.footnotes`, math `span`, block-directive `div` (`render_block_directive`). Not on `thead`/`tbody`. An end column of `0` (`6:1-7:0`) means
"up to the end of the previous line" (comrak 0.56 fix "Zero-end columns adopt the widest end position").

Trick for `serve`: the rebuild task keeps the previous expanded source text; after a `content` rebuild it diffs
old vs new (`similar::TextDiff::from_lines`, or a plain first-differing-line scan) and puts `changed_line: L`
(1-based, in the **expanded** source) into the SSE `build` event. The client, after morphing, picks the innermost
block element whose range covers `L` and scrolls it:

```js
function blockAt(line) {
  let best = null, span = Infinity;
  for (const el of document.querySelectorAll('main [data-sourcepos]')) {
    const m = /^(\d+):\d+-(\d+):\d+$/.exec(el.dataset.sourcepos);
    if (!m) continue;
    const a = +m[1], b = +m[2] || a;              // end line 0 never occurs; end col 0 is handled by the regex
    if (a <= line && line <= b && b - a < span) { best = el; span = b - a; }
  }
  return best;
}
blockAt(ev.changed_line)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
```

Design note: writer expands `:::include` (and notebooks) before comrak sees the text, so positions are in the expanded
buffer; keep a line map `expanded line → (file, line)` from the include step (presenter's `src/markdown/compat.rs`
already reads `node.data().sourcepos.start.line` for a similar purpose) so error messages and the editor-side
"jump to source" stay correct. Alternatively emit `data-src="<file>:<line>"` from writer's own renderer.

---

## A.3 Document CSS defaults and print CSS

### Candidate stylesheets [verified: npm registry, jsdelivr byte counts and grep of the CSS on 2026-10-24]

| name | latest (date) | license | size (min / gzip) | fonts | what it styles | print rules |
|---|---|---|---|---|---|---|
| **latex.css** | 1.14.0 (2026-05-13) | MIT | `style.css` 16,406 B (4,138 gz); `style.min.css` **3,175,575 B** because the 12 Latin Modern `.ttf/.woff/.woff2` files are inlined as `data:` URIs | 10 `@font-face` (Latin Modern) referenced as `./fonts/LM-*.{ttf,woff,woff2}` in `style.css` | `.abstract`, `.author`, `.theorem`/`.lemma`/`.definition`/`.proof` with CSS counters, `.sidenote` + `.sidenote-number`/`.sidenote-toggle`, `.footnotes`, tables (`th`/`td`, `caption`), `figure`/`figcaption`, `kbd`, `dl`, `nav` (TOC list), 12-column grid helpers (`.col-N-{l,c,r}`), `.latex-dark`/`.latex-dark-auto` (`prefers-color-scheme`), `lang/*.css` for localized theorem names | none (`@media print`/`@page`: 0; only `.break-all` = `word-break`) |
| **water.css** | 2.1.1 (2021-08-11) | MIT | `out/water.min.css` 22,668 B (3,557 gz); `light.min.css`/`dark.min.css` ≈ 10 KB each | system fonts | classless: body/typography, tables, `pre`/`code`, `blockquote`, `details`/`summary`, `kbd`, `dl`, `hr`, `mark`, heavy on forms/buttons/inputs, dark mode via `prefers-color-scheme` + CSS variables | one `@media print` block (white backgrounds for body/pre/code/…) |
| **tufte-css** | 1.9.0 (2026-06-25) | MIT | `tufte.min.css` 6,946 B (1,745 gz) + ET Book fonts (20 files, eot/svg/ttf/woff, no woff2, ≈ 1.6 MB) | 4 `@font-face` (ET Book) | `.sidenote`, `.marginnote`, `.margin-toggle` (checkbox hack for narrow screens), `.sidenote-number` counters, `figure`/`figcaption`, `.fullwidth`, `.epigraph`, `.newthought`, `.sans`, `.table-wrapper`, `.iframe-wrapper`, minimal tables, `code`; layout is 55 % text column with wide right margin | none |
| **new.css** | 1.1.3 (2020-05-20) | MIT | `new.min.css` 4,789 B (1,809 gz) | system | classless: tables, `blockquote`, `pre`/`code`, `kbd`, `details`, forms, `--nc-*` variables, dark via `prefers-color-scheme`; themes `night.css`/`terminal.css` | none |
| **pico** (classless) | 2.1.1 (2025-03-15) | MIT | `pico.classless.min.css` 71,040 B (10,315 gz) | system | component framework (nav, cards, forms, 939 variables); `figure`/`figcaption`; not document-oriented | none |

None of them styles callouts/admonitions (0 hits for `callout`/`admonition` in all five), none sets `@page`,
`orphans`/`widows` or `break-inside` for figures/tables, and none styles comrak's footnote markup
(`section.footnotes`, `.footnote-ref`, `.footnote-backref`) or GitHub-style alerts (`div.markdown-alert-note`)
out of the box (latex.css styles its own `.footnotes` class name, which happens to match comrak's section class).

**Recommendation: write writer's own stylesheet (target ≤ 10 KB unminified, CSS variables, light/dark via
`prefers-color-scheme`, system font stack by default, no embedded fonts) and borrow:**

- from **latex.css**: measure/width and vertical rhythm, the booktabs-like table rules (top/bottom rules, no vertical
  lines), `figure`/`figcaption` ("Figure N:" via counters), the theorem/abstract/author blocks, `nav` TOC list style,
  `.latex-dark-auto` variable scheme;
- from **tufte-css**: the sidenote/margin-note mechanics (`.margin-toggle` + `input[type=checkbox]` hack, `.sidenote-number`
  counters, `.fullwidth`), as the pattern for footnotes-as-sidenotes on wide screens;
- from **water.css**: the `@media print` reset and its light/dark variable layout;
- generate the code theme with `css_for_theme_with_class_style` (≈ 16 KB per theme with prefix; ship one light and
  one dark theme under `@media (prefers-color-scheme: dark)`, or inline-style mode for standalone output);
- add what none has: callouts (`.callout.note/.tip/.warning/.danger` + comrak `.markdown-alert-*`), footnote
  section + backrefs, `break-inside: avoid` on `figure`, `table`, `pre`, `.callout`, and the `@page` rules below.

### Print CSS facts [verified: MDN browser-compat-data `main` JSON, fetched 2026-10-24]

| feature | Chrome | Safari | Firefox |
|---|---|---|---|
| `@page` | 2 | 18.2 | 19 |
| `@page { size: A4 / letter / landscape }` | 15 (`landscape`/`portrait` 15) | 18.2 (no landscape/portrait keywords) | 95 |
| `@page { page-orientation }` | 85 | no | 122 |
| page margin boxes `@top-center`, `@bottom-right`, … (running heads, `content: counter(page)`) | **131** | no | no |
| `page` property (named pages: `@page wide { size: landscape }` + `.x { page: wide }`) | 85 | 1 | 110 |
| `break-before/after: page`, `avoid-page`, `left`, `right`, `recto`, `verso` | 50 (recto/verso in paged context 87) | 10 | 65 (no avoid-page/recto/verso) |
| `break-before/after: always` | **not supported** (use `page`) | no | 65 |
| `break-before/after: avoid` | 50 | recognized, no effect | recognized, no effect |
| `break-inside: avoid` / `avoid-page` | 50 / 51 | 10 | 65 / 92 |
| `orphans`, `widows` | 25 | 1.3 | **no** |
| `print-color-adjust: exact` (unprefixed) | 136 (`-webkit-print-color-adjust` earlier) | 15.4 | 97 |
| `target-counter(attr(href url), page)` for TOC page numbers | **no entry in BCD; no browser implements it** — a Chromium design doc (July 2026) only *proposes* `target-counter()`/`target-counters()` [status unverified beyond the search result]; paged.js polyfills it (`TargetCounters` handler) | no | no |

So for Chrome-printed PDFs: page size/margins, running headers/footers with page numbers (Chrome ≥ 131, local
Chrome is 155), named pages, forced/avoided breaks and orphans/widows all work; a **TOC with page numbers does
not** (needs paged.js or typst). Legacy `page-break-*` is an alias (`always` → `page`).

A minimal print block for writer's default stylesheet:

```css
@page { size: A4; margin: 20mm 18mm; @bottom-center { content: counter(page); font: 9pt system-ui; } }
@page :first { @bottom-center { content: none; } }
@media print {
  html { font-size: 11pt; } body { max-width: none; background: #fff; color: #000; }
  h1, h2, h3 { break-after: avoid; } h1 { break-before: page; }
  figure, table, pre, .callout, li { break-inside: avoid; }
  p { orphans: 3; widows: 3; } a[href^="http"]::after { content: " (" attr(href) ")"; font-size: 85%; }
  nav.toc, .no-print { display: none; } pre { white-space: pre-wrap; }
  * { print-color-adjust: exact; -webkit-print-color-adjust: exact; }
}
```

### Chrome print-to-PDF as an optional engine [verified: `presenter_rs/src/pdf.rs`, `src/pdf/devtools.rs`]

presenter's `src/pdf.rs` (99 KB, no extra crates) drives headless Chrome over the DevTools protocol with
`--remote-debugging-pipe` (NUL-delimited JSON on fds 3/4, flat sessions, no network port): private temp profile,
`Target.createTarget` + `Target.attachToTarget {flatten:true}`, `Page.navigate file://…`, `Runtime.evaluate`
of a readiness script (awaits `document.fonts.ready` + two animation frames), then
`Page.printToPDF {preferCSSPageSize: true, printBackground: true, margins 0, ReturnAsStream}` read with `IO.read`,
`Browser.close`, a `/bin/sh` watchdog that removes the profile if presenter dies. Public pieces: `find_chrome`,
`html_to_pdf`, `pdf::capture::html_to_pages` (PPTX path), and `pdf::devtools::Page { launch(chrome, timeout),
set_timeout, navigate(url), evaluate(expr) -> Value, errors() }` (the latter only under the `browser-tests` feature).
macOS/Linux only. **Keep it** as `writer export pdf --engine chrome` (default `typst`): it prints exactly what
the HTML preview shows (syntect classes, callouts, KaTeX/MathML, running heads via `@page` margin boxes), which the
typst path cannot reproduce 1:1; lift the module and replace the reveal-specific readiness script (`Reveal.isReady`,
`?print-pdf`) with `document.fonts.ready` + a `writer:ready` promise. `pdf/scene.js` and `capture.rs` are PPTX-only
and can stay behind.

---

## A.4 Single-file HTML: presenter_rs pieces to lift [verified: sources read 2026-10-24]

### `src/assets/emit.rs` (76 KB) public API

```rust
pub const SERVE_ASSET_PREFIX: &str = "/__presenter/deck-asset/";   // rename to /__writer/asset/
pub enum OutputMode {
    Standalone,                                                     // data:<mime>;base64,… inlined
    Site { assets_dir: PathBuf, url_prefix: String },               // content-addressed files, e.g. assets/plot-5e0c7b1f9a2d.png
    Serve,                                                          // /__presenter/deck-asset/<id>, bytes not read
}
pub struct EmittedFile { pub path: PathBuf, pub bytes: Vec<u8> }
pub struct Emitter<'a> { pub files: Vec<EmittedFile>, /* registry: &'a AssetRegistry, mode: &'a OutputMode */ }
impl<'a> Emitter<'a> {
    pub fn new(registry: &'a AssetRegistry, mode: &'a OutputMode) -> Self;
    pub fn with_files(registry: &'a AssetRegistry, mode: &'a OutputMode, files: Vec<EmittedFile>) -> Self;
    pub fn resolve_html(&mut self, html: &str) -> Result<String>;          // placeholders in attribute values + <style>/<script> only
    pub fn resolve_css(&mut self, css: &str) -> Result<String>;            // whole text; site URLs relative to the document
    pub fn resolve_css_in_assets_dir(&mut self, css: &str) -> Result<String>; // bare file names for a stylesheet written into assets_dir
    pub fn url_for(&mut self, id: &str) -> Result<String>;
}
pub fn data_uri(mime: &str, bytes: &[u8]) -> String;                        // base64 STANDARD, padded, SVG too
pub fn hashed_file_name(hint: &str, id: &str) -> String;                    // "plot.png" + hash -> "plot-<hash>.png"
pub fn register_css_urls(css: &str, registry: &mut AssetRegistry,
                         resolve_rel: &mut dyn FnMut(&str) -> Option<AssetData>, lenient: bool) -> Result<String>;
```

Placeholder grammar: `asset:` + exactly 12 lowercase hex (`AssetRegistry::placeholder(id)`); emit.rs imports
`crate::assets::registry::{AssetData, AssetRegistry, content_hash, unknown_id}`, `crate::error::{Error, Result}`
(`Error::Other`, `Error::MissingAsset { path, span }`), and the HTML tokenizer helpers
`crate::markdown::render::{html_start_tags, html_attr_values}` (`pub(crate) fn html_start_tags(html: &str) -> Vec<(String, usize)>`,
`pub(crate) fn html_attr_values(html: &str) -> Vec<(String, Range<usize>)>`, both built on an `HtmlScan` type in
`src/markdown/render.rs`) plus `base64::Engine` and `regex`. Lift `emit.rs` + `registry.rs` + the `HtmlScan` part of
`render.rs` together (or replace the tokenizer with `lol_html`, already a dependency).

### `src/assets/registry.rs` public API

```rust
pub const ID_LEN: usize = 12;
pub enum AssetData { File(PathBuf), Bytes(Vec<u8>) }
pub struct Asset { pub id: String, pub mime: String, pub data: AssetData, pub file_name_hint: String }
pub struct AssetRegistry { /* BTreeMap<String, Asset> */ }
impl AssetRegistry {
    pub fn new() -> Self;
    pub fn register_file(&mut self, abs_path: &Path) -> String;                               // id = hash of absolute path
    pub fn register_bytes(&mut self, bytes: Vec<u8>, mime: &str, file_name_hint: &str) -> String;
    pub fn get(&self, id: &str) -> Option<&Asset>;
    pub fn iter(&self) -> impl Iterator<Item = &Asset>;
    pub fn infos(&self, base_dir: &Path) -> Vec<AssetInfo>;
    pub fn placeholder(id: &str) -> String;                                                   // "asset:<id>"
    pub fn merge(&mut self, other: AssetRegistry);
    pub fn len(&self) -> usize; pub fn is_empty(&self) -> bool;
    pub fn read(&self, id: &str) -> Result<Cow<'_, [u8]>>;
}
pub fn content_hash(bytes: &[u8]) -> String;   // blake3, 12 hex
pub fn resolve_path(path: &Path) -> PathBuf; pub fn normalize_lexically(path: &Path) -> PathBuf;
```

### Vendor embedding (`src/vendor.rs`, `build.rs`, `scripts/vendor.sh`)

```rust
#[derive(rust_embed::Embed)]
#[folder = "assets/"]
#[include = "vendor/**/*"] #[include = "presenter/**/*"] #[include = "themes/**/*"] #[include = "scaffold/**/*"]
#[exclude = "**/*.map"] #[exclude = "**/.DS_Store"] #[exclude = "**/._*"] #[exclude = "**/Thumbs.db"] #[exclude = "**/*.swp"] #[exclude = "**/*~"]
pub struct Assets;
pub fn get(path: &str) -> Option<Cow<'static, [u8]>>;   // lexical normalization, exact case-sensitive key in debug and release
pub fn get_str(path: &str) -> Result<String>;
pub fn list(prefix: &str) -> Vec<String>;                // sorted
```

- `rust-embed = { version = "8.13", features = ["include-exclude"] }`; debug builds read `assets/` from disk at
  runtime (edit-and-refresh), release builds embed. `build.rs` prints `cargo::rerun-if-changed=assets` only when
  `CARGO_CFG_DEBUG_ASSERTIONS` is unset (so added/removed files re-embed) and sets `PRESENTER_GIT_SHA` from
  `git describe` for `--version`; 305 lines, lift as is with the env var renamed.
- `scripts/vendor.sh` (bash; curl, tar, shasum): pinned npm tarball URLs + SHA-256 per package, copies an explicit
  file list into a staging dir, writes `assets/vendor/VERSIONS.toml` (`[[package]] name/version/license/url/sha256/date/path`),
  swaps it in only on success; idempotent (CI diff check). For writer: entries for `katex` (0.19.0 pinned there,
  `katex.min.js`, `katex.min.css`, `fonts/*.woff2` only) and `morphdom` (`dist/morphdom-umd.min.js`, `LICENSE`).
- presenter's single-file size budget (`docs/spec/vendored-assets.md` §3): KaTeX adds 595 KB, images ×1.37 (base64).

---

## B. Verified crate versions (`cargo info`, 2026-10-24)

"latest" is what `cargo info` printed (it prints pre-releases when they are the newest; marked ⚠). "presenter"
is `presenter_rs/Cargo.lock`. rust-version "—" = not declared.

| crate | latest | license | rust-version | presenter lock | change |
|---|---|---|---|---|---|
| anyhow | 1.0.104 | MIT OR Apache-2.0 | 1.68 | 1.0.104 | = |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | 1.77 | 2.0.21 | = |
| clap | 4.6.7 | MIT OR Apache-2.0 | 1.85 | 4.6.7 | = |
| serde | 1.0.229 | MIT OR Apache-2.0 | 1.56 | 1.0.229 | = |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | 1.71 | 1.0.151 | patch |
| toml | 1.1.8+spec-1.1.0 | MIT OR Apache-2.0 | 1.85 | 1.1.7 | patch |
| serde_norway | 0.9.42 | MIT OR Apache-2.0 | 1.71.1 | 0.9.42 | = |
| comrak | 0.57.0 | BSD-2-Clause | 1.89 | 0.56.0 | minor (0.57.0, 2026-10-21: wikilink embeds, `ofm_comments` ext; no API removals listed) |
| minijinja | **3.0.3** | Apache-2.0 | 1.70 (CLI needs 1.88) | 2.24.0 | **MAJOR** — see below |
| rust-embed | 8.13.0 | MIT | 1.80.0 | 8.13.0 | = |
| notify | ⚠ 9.0.0-rc.6 (2026-10-17); **max stable 8.2.0** | CC0-1.0 | 1.88 | 8.2.0 | pin `8.2` |
| notify-debouncer-full | ⚠ 0.8.0-rc.2; **max stable 0.7.0** (depends on `notify ^8.2.0`) | MIT OR Apache-2.0 | 1.88 | not used (presenter has its own debouncer; the spec says the full debouncer dropped rename-replace events) | — |
| tokio | 1.54.1 | MIT | 1.85 | 1.53.2 | minor |
| axum | 0.8.9 | MIT | 1.80 | 0.8.9 | = |
| tokio-stream | 0.1.19 | MIT | 1.71 | 0.1.19 | = |
| tracing | 0.1.44 | MIT | 1.65.0 | 0.1.44 | = |
| tracing-subscriber | 0.3.23 | MIT | 1.65.0 | 0.3.23 | = |
| base64 | 0.23.1 | MIT OR Apache-2.0 | 1.71.0 | 0.23.1 | = |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | — | 1.8.7 | = |
| mime_guess | 2.0.5 | MIT | — | 2.0.5 | = |
| html-escape | **0.3.0** (2026-10-12) | MIT | 1.58 | 0.2.15 | **0.x minor = breaking** — see below |
| ansi-to-html | 0.2.4 | MIT OR Apache-2.0 | 1.85.1 | 0.2.4 | = |
| regex | 1.13.1 | MIT OR Apache-2.0 | 1.65 | 1.13.1 | = |
| walkdir | 2.5.0 | Unlicense/MIT | — | 2.5.0 | = |
| open | 5.4.5 | MIT | 1.62 | 5.4.4 | patch |
| lol_html | 3.0.2 | BSD-3-Clause | 1.85 | 3.0.1 | patch |
| grass | 0.13.4 | MIT | 1.70 | 0.13.4 | = |
| codemap | 0.1.3 | MIT/Apache-2.0 | — | 0.1.3 | = |
| insta | 1.49.0 | Apache-2.0 | 1.66.0 | 1.49.0 | = (depends on `similar ^2.1.0`) |
| assert_cmd | 2.2.2 | MIT OR Apache-2.0 | 1.85 | 2.2.2 | = |
| predicates | 3.1.4 | MIT OR Apache-2.0 | 1.74 | 3.1.4 | = |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | 1.63 | 3.27.0 | = |
| ureq | 3.5.0 | MIT OR Apache-2.0 | 1.85 | 3.4.2 | minor |
| zip | 9.0.5 (2026-10-17) | MIT | 1.88 | — | new; `default = [aes-crypto, bzip2, deflate64, deflate, lzma, ppmd, time, zstd, xz]` (crates.io API) — for `.docx` use `default-features = false, features = ["deflate"]` |
| quick-xml | 0.42.0 | MIT | 1.86 | — | new (`serialize`, `escape-html` features) |
| syntect | 5.3.0 | MIT | — | — | new |
| two-face | 0.5.2+bat-0.26.1 | MIT OR Apache-2.0 | 1.79.0 | — | new |
| hayagriva | 0.10.1 | MIT OR Apache-2.0 | — | — | new (features `biblatex`, `csl-json`, `archive`) |
| mitex | 0.2.4 | Apache-2.0 | — | — | new |
| pulldown-latex | 0.8.0 | MIT | 1.74.1 | — | new |
| latex2mathml | 0.2.3 | MIT | — | — | new |
| typst | 0.15.1 | Apache-2.0 | **1.92** | — | new |
| typst-pdf | 0.15.1 | Apache-2.0 | 1.92 | — | new |
| typst-svg | 0.15.1 | Apache-2.0 | 1.92 | — | new |
| typst-render | 0.15.1 | Apache-2.0 | 1.92 | — | new |
| typst-html | 0.15.1 | Apache-2.0 | 1.92 | — | new |
| typst-kit | 0.15.1 | Apache-2.0 | 1.92 | — | new (features: `embedded-fonts`, `scan-fonts`, `system-packages`, `universe-packages`, `system-downloader` (pulls `ureq`, `native-tls`, `openssl`), `vendor-openssl`, `watcher` (pulls `notify`), `http-server`, `emit-diagnostics`, `datetime`, `timer`, `bundle`) |
| typst-assets | 0.15.1 | Apache-2.0 | — | — | new (feature `fonts`) |
| typst-syntax | 0.15.1 | Apache-2.0 | 1.92 | — | new |
| typst-library | 0.15.1 | Apache-2.0 | 1.92 | — | new |
| comemo | 0.5.1 | MIT OR Apache-2.0 | 1.88 | — | new |
| codex | 0.3.0 | Apache-2.0 | 1.88 | — | new |
| resvg | 0.48.1 | Apache-2.0 OR MIT | 1.85.0 | — | new (`text`, `system-fonts`, `raster-images`) |
| usvg | 0.48.1 | Apache-2.0 OR MIT | 1.85.0 | — | new |
| svg2pdf | 0.13.0 | MIT OR Apache-2.0 | — | — | new (`text`, `filters`, `image`) |
| imagesize | 0.15.0 | MIT | — | — | new |
| image | 0.25.10 | MIT OR Apache-2.0 | 1.88.0 | — | new (`default-formats` incl. avif/exr; trim with `default-features = false, features = ["png","jpeg","gif","webp"]`) |
| scraper | 0.29.0 | ISC | — | — | new |
| html5ever | 0.40.1 | MIT OR Apache-2.0 | 1.85 | — | new |
| cansi | 2.2.1 | MIT | — | — | new |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 | 1.66.0 | — | new |
| csv | 1.4.0 | Unlicense/MIT | 1.73 | — | new |
| strsim | 0.11.1 | MIT | 1.56 | 0.11.1 (transitive) | = |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 | 1.66 | — | new |
| pathdiff | 0.2.3 | MIT/Apache-2.0 | — | — | new |
| similar | **3.2.0** | Apache-2.0 | 1.85 | 2.7.0 (transitive via insta) | MAJOR vs insta's `^2.1.0` → two copies if writer pins 3; pin `similar = "2"` or avoid |
| time | 0.3.55 | MIT OR Apache-2.0 | 1.88.0 | — | new |
| chrono | 0.4.45 | MIT OR Apache-2.0 | 1.62.0 | — | new |
| tungstenite | 0.30.0 | MIT OR Apache-2.0 | 1.85 | — | new (SSE via axum needs none of it) |
| ecow | 0.3.1 | MIT OR Apache-2.0 | 1.73 | — | new |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | 1.85 | 2.14.2 | = |

MSRV consequence: with typst 0.15.1 the workspace `rust-version` is **1.92** (comrak alone needs 1.89); the machine
has 1.99.1.

### minijinja 3.0.x breaking changes that touch presenter-derived code [verified: `cargo add` test, 3.0.3 sources, CHANGELOG]

- `cargo add minijinja@3.0.3 --features loader,json,loop_controls` → **`error: unrecognized feature for crate minijinja: loader`**
  (the no-op `loader` and `key_interning` features were removed). Use `features = ["json", "loop_controls"]`
  (both exist; `json` no longer needs serde).
- `serde` is now an optional feature, off by default; `Value::from_serialize` is **gone** (no definition in 3.0.3
  `src/`), presenter uses it once. Replacement: `minijinja::value::Serde` wrapper (`pub struct Serde` in `src/value/mod.rs`)
  with `features = ["serde"]` [exact call shape unverified].
- Still present (grep of 3.0.3 `src/environment.rs`, `loader.rs`, `filters.rs`, `error.rs`): `Environment::new`,
  `add_template`, `add_template_owned`, `set_loader`, `path_loader`, `get_template`, `render_str`, `render_named_str`,
  `add_filter`, `add_function`, `add_global`, `set_debug`/`debug`, `set_undefined_behavior`, `set_auto_escape_callback`,
  `set_syntax`, `filters::tojson`, `ErrorKind::{TemplateNotFound, InvalidOperation}`.
- New: `Environment::set_auto_reload(bool)` — automatic template reloading is **on by default** in 3.0; keep it on for `serve`.
- Other 3.0 changes (CHANGELOG): `render_and_return_state`, `render_to_write`, `eval_to_state` removed; `trim_blocks`/
  `lstrip_blocks`/`keep_trailing_newline` moved to `SyntaxConfig`; `format_filter` → `formatting::format`; `AutoEscape`
  not `Copy`; `context!`/`args!` take `Into<Value>`; implicit kwarg values rejected; 3.0.2: names starting with `_`
  are private in `{% import %}`.

### html-escape 0.3.0 [verified: source diff 0.2.15 → 0.3.0]

Same `rust-version` (1.58) and the same generated function names presenter uses (`encode_text`,
`encode_double_quoted_attribute`, `encode_safe`, `decode_html_entities`, `encode_text_to_string`, …). What changed:
the `<script>`/`<style>` `*_quoted_text` encoders now escape every backslash (and their decoders decode `\\` and hex
escapes), new `quoted.rs` modules, and a faster `<`-scanning path in the element encoders/decoders. Nothing presenter
calls is renamed; semantics of `encode_text`/`decode_html_entities` are not claimed changed by the crate docs
[behavioural equality unverified — run presenter's snapshot tests after bumping].

### comrak 0.56 → 0.57 [verified: CHANGELOG in the crate]

0.57.0 (2026-10-21): wikilink embeds, `ofm_comments` extension, dependency bumps. No removed APIs. 0.56.0 removed the
`tagfilter` extension and raised MSRV to 1.89; fixed many sourcepos bugs (multiline inlines, tables, indented code).
`Options<'c> { extension: Extension<'c>, parse: Parse<'c>, render: Render }` in `comrak::options`; `render.sourcepos: bool`,
`parse.sourcepos_chars: bool`.

---

## Verified API hints (ready to paste)

```rust
// --- highlighting setup (writer::highlight) ---
use std::sync::LazyLock;
use syntect::parsing::SyntaxSet;
use syntect::html::{ClassedHTMLGenerator, ClassStyle, css_for_theme_with_class_style};
use syntect::util::LinesWithEndings;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines); // 213 syntaxes, ~1.5–3 ms once
static THEMES: LazyLock<two_face::theme::EmbeddedLazyThemeSet> = LazyLock::new(two_face::theme::extra);

const ALIASES: &[(&str, &str)] = &[("console", "bash"), ("shell", "bash"), ("text", "txt"), ("plain", "txt"),
                                   ("jsx", "js"), ("fortran", "f90"), ("ipynb", "json")];

pub fn highlight_classes(lang: &str, code: &str) -> String {
    let token = ALIASES.iter().find(|(a, _)| *a == lang).map(|(_, t)| *t).unwrap_or(lang);
    let ss = &*SYNTAXES;
    let syntax = ss.find_syntax_by_token(token).unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut g = ClassedHTMLGenerator::new_with_class_style(syntax, ss, ClassStyle::SpacedPrefixed { prefix: "hl-" });
    for line in LinesWithEndings::from(code) {            // every line keeps its '\n' (required)
        if g.parse_html_for_line_which_includes_newline(line).is_err() { return html_escape::encode_text(code).into_owned(); }
    }
    g.finalize()
}
// wrap as: <pre class="hl-code"><code class="language-{lang}">{html}</code></pre>  (".hl-code" is the theme's root rule)

pub fn theme_css(name: two_face::theme::EmbeddedThemeName) -> String {
    css_for_theme_with_class_style(THEMES.get(name), ClassStyle::SpacedPrefixed { prefix: "hl-" }).expect("theme css")
}

// --- DOCX colored runs ---
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style};
pub fn highlight_runs<'a>(lang: &str, code: &'a str, theme: &syntect::highlighting::Theme) -> Vec<Vec<(Style, &'a str)>> {
    let ss = &*SYNTAXES;
    let syntax = ss.find_syntax_by_token(lang).unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);
    LinesWithEndings::from(code).map(|line| h.highlight_line(line, ss).unwrap_or_default()).collect()
}
// per region: style.foreground.{r,g,b} -> <w:color w:val="A71D5D"/>; style.font_style.contains(FontStyle::BOLD) -> <w:b/>;
// ITALIC -> <w:i/>; strip the trailing '\n' of the last region and emit <w:br/> between lines.

// --- comrak with source positions ---
let mut o = comrak::Options::default();
o.extension.table = true; o.extension.footnotes = true; o.extension.math_dollars = true;
o.extension.header_attributes = true; // needs comrak feature "attributes"
o.render.sourcepos = true;            // data-sourcepos="L:C-L:C"
let html = comrak::markdown_to_html(md, &o);

// --- optional: comrak's adapter with two-face ---
// comrak features ["syntect-fancy"]; keep in mind write_pre_tag drops attributes in css mode (see A.1)
let adapter = comrak::plugins::syntect::SyntectAdapterBuilder::new()
    .css_with_class_prefix("hl-")
    .syntax_set(two_face::syntax::extra_newlines())
    .build();
let mut plugins = comrak::options::Plugins::default();
plugins.render.codefence_syntax_highlighter = Some(&adapter);
let html = comrak::markdown_to_html_with_plugins(md, &o, &plugins);
```

```toml
# Cargo.toml lines (verified to resolve without onig)
syntect  = { version = "5.3.0", default-features = false, features = ["default-fancy"] }
two-face = { version = "0.5.2", default-features = false, features = ["syntect-fancy"] }
comrak   = { version = "0.57", default-features = false, features = ["attributes"] }   # + "syntect-fancy" if the adapter is used
minijinja = { version = "3.0", features = ["json", "loop_controls"] }                    # no "loader" feature in 3.x
notify = "8.2"                                                                           # 9.0 is still rc
html-escape = "0.3"
```

```js
// serve client: morph + scroll (vendored morphdom UMD exposes window.morphdom)
async function swapContent(ev) {
  const html = await (await fetch('/', { cache: 'no-store' })).text();
  const next = new DOMParser().parseFromString(html, 'text/html');
  const main = document.querySelector('main'), fresh = next.querySelector('main');
  if (!main || !fresh) return location.reload();
  morphdom(main, fresh, { childrenOnly: true, onBeforeElUpdated: (a, b) => !a.isEqualNode(b) });
  if (next.title) document.title = next.title;
  if (window.katex && window.__writerRenderMath) window.__writerRenderMath(main);   // if math is client-side
  if (ev.changed_line) blockAt(ev.changed_line)?.scrollIntoView({ block: 'nearest' });
}
```

---

## Open questions

1. The minimal syntect feature set (`["default-syntaxes","default-themes","html","regex-fancy"]`, dropping
   `plist`/`yaml-rust`) was not compiled — try it in the probe crate before pinning; `default-fancy` is the verified fallback.
2. Exact binary cost of two-face when syntect's own dumps are *not* linked (expected ≈ +0.6 MiB per two-face README);
   the measured +1.07 MiB links both.
3. `minijinja::value::Serde` / `features = ["serde"]`: confirm the call shape that replaces `Value::from_serialize`
   (presenter uses it once) and that `context!` still accepts serde structs.
4. html-escape 0.3.0: run presenter's snapshot tests after the bump to confirm `encode_text`/`decode_html_entities`
   output is byte-identical (only script/style quoted-text functions are documented as changed).
5. `target-counter()` in Chrome: no BCD entry; a 2026 Chromium design doc proposes it. Re-check
   chromestatus.com before promising TOC page numbers from the Chrome PDF engine; otherwise paged.js or typst.
6. typst-kit's `watcher` feature pulls `notify` — which major? If it is 8.x it unifies with writer's; if 9.x, two copies.
   Also `system-downloader` pulls `openssl`; prefer `ureq` with rustls or `vendor-openssl` and verify what builds on macOS.
7. (resolved) `zip` 9.0.5 defaults include `deflate` plus aes/bzip2/lzma/ppmd/zstd/xz; pin `default-features = false,
   features = ["deflate"]` to keep the dependency tree small.
8. How writer maps `data-sourcepos` lines back through `:::include`/notebook expansion (line map vs. per-block
   `data-src` attribute) is a design decision for the IR task.
9. Whether comrak's `render.sourcepos` is needed in `build` output too (it adds ≈ 25 bytes per block; harmless but
   could be `serve`-only).
