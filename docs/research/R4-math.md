# R4 — Math handling across formats (LaTeX math, typst math → HTML / DOCX / PDF / LaTeX)

Research note for the `writer` plan. Written 2026-10-24 on macOS 26.7, Rust 1.99.1, with
pandoc 3.12.1, Google Chrome 155, Microsoft Word installed, no typst CLI.

Every claim is tagged **[verified]** (seen in crate source, docs fetched today, or produced by a
probe run here) or **[unverified]** (recalled / not run). Probe crates live under
`/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/`
(called `$SB` below):

| Probe | What it does | Output file |
|---|---|---|
| `$SB/math-probe` (`cargo run -- mathml`) | 20 LaTeX inputs × {pulldown-latex 0.8.0, latex2mathml 0.2.3, math-core 0.9.0, tex2math 2.4.1} | `$SB/math-probe/mathml-out.txt` |
| `$SB/math-probe/typst-eq` (`typst-eq <outdir> all\|svg\|time\|init\|html`) | typst equation → frame tree/baseline → SVG → PNG (resvg 0.48.1 + typst-render), timings, HTML/MathML export | `$SB/math-probe/typst-eq/out/probe.txt`, `*.svg`, `*@2x.png` |
| `$SB/katex-probe` (release) | `katex` 0.4.6 crate (quick-js) binary size + timing | `$SB/katex-probe/katex-out.txt` |
| `$SB/katexrs-probe` (release) | `katex-rs` 0.3.0 pure-Rust port, 15 inputs, timing | `$SB/katexrs-probe/out.txt` |
| `$SB/omml-probe` (`cargo run -- omml`) | 14 LaTeX inputs × {easydoc-math 0.1.2, tex2word-math 1.0.6, deckmint-math 0.1.7, mt-mathml+mt-omml 0.1.2} | `$SB/omml-probe/omml-out.txt` |
| `…/scratchpad/mathml-test/index.html` | MathML font test page (served on 127.0.0.1:8765, viewed in the Claude browser pane = Chromium 152) | screenshots in the transcript |
| `…/scratchpad/downloads/texmath/OMML.hs` | pandoc/texmath OMML writer source (377 lines, GPL-2+) read in full | — |

Related notes: R1 §5–6 (typst World, HTML export), R2 §4–5 (typst math AST; mitex/tylax/pandoc
measurements — **not repeated here**), R3 §2.11/§4 (DOCX images, OMML inventory), R6 (crate table).

---

## 0. Executive summary

1. **LaTeX → MathML in pure Rust is solved by two maintained crates.** `pulldown-latex` 0.8.0
   (MIT, 2026-07-28, one dep: bumpalo; 741 command names; KaTeX-style coverage incl. `\def`,
   `\newcommand[n]`, colors, `aligned`/`cases`/matrices) and `math-core` 0.9.0 (MIT, 2026-10-12,
   MSRV 1.96; "MathML Core" oriented; built-in macro table; `ignore_unknown_commands`;
   `\newcommand` without `[n]` works). Both produced valid output for 13/13 representative
   inputs **[verified]**. `latex2mathml` 0.2.3 (2020, unmaintained) fails on `aligned`, `\color`,
   macros; `tex2math` 2.4.1 is LGPL-3.0 and emits non-Core markup. Known pulldown-latex bugs:
   `\newcommand{\dd}{…}` **without** `[n]` → "expected an argument" (source `primitives.rs:2253`),
   accents emitted as `<mover><mrow>x</mrow><mi>^</mi></mover>` without `accent="true"` (README
   TODO), `aligned` relies on CSS classes `menv-alignlike` (needs its 3.2 KB `styles.css`).
   math-core emits `&ApplyFunction;` (HTML-only entity; breaks XML parsers) and inline `style=`
   for alignment (self-contained). **Recommendation: math-core 0.9.0 as the MathML engine**
   (fewer CSS dependencies, better macro ergonomics, unknown-command recovery), pulldown-latex
   as the second opinion/oracle in tests.

2. **MathML Core is Baseline "Widely available" (since January 2023)**: Chrome/Edge 109+,
   Firefox (all), Safari 5.1+/iOS; caniuse global usage 96.02% **[verified: MDN + caniuse +
   mdn/browser-compat-data fetched today]**. Not in Core / not in Chrome: `mfenced` (never),
   `menclose` (never), `mtable columnalign/rowlines/frame…` (Chrome: no), `mathvariant` values
   other than `normal`, `mathcolor/mathsize/mathbackground` (deprecated; use CSS), `mo accent`
   (Chrome: no; put `accent="true"` on `mover`). **Fonts are the real caveat**: the UA stylesheet
   sets `math { font-family: math }`; in the Chromium 152 pane on this Mac (STIX Two Math
   *installed*) the default rendering was still bad (no stretchy delimiters, text-size ∑/∫),
   while `@font-face` Latin Modern Math (388 KB woff2, GUST FL) or an explicit
   `font-family: "STIX Two Math"` rendered correctly **[verified: screenshots]**. So MathML output
   must ship a `math { font-family: … }` rule and, for guaranteed rendering, an embedded math font
   (LM Math 388 KB / STIX Two Math 559 KB woff2) — i.e. *more* bytes than KaTeX's fonts.

3. **KaTeX**: npm latest 0.19.1 (2026-10-23; 0.19.0 2026-10-01 — presenter vendored 0.19.0; no
   breaking CSS changes between them). Dist: `katex.min.js` 273,081 B, `katex.min.css` 24,793 B,
   20 woff2 259,792 B → 557,666 B raw **[verified: jsDelivr]**. Output option
   `output: html | mathml | htmlAndMathml` (default `htmlAndMathml`) **[verified: katex.org]**.
   Server-side from Rust without Node: (a) `katex` 0.4.6 crate = KaTeX **0.16.4** in QuickJS
   (last release 2023-02-05; +1.4 MB binary; 37 ms init, 0.48 ms/render) — stale; (b) **`katex-rs`
   0.3.0 = pure-Rust port of KaTeX 0.18.5** (MIT, 2026-09-05, 86 k downloads, upstream-spec test
   suite): 17–33 µs/render, ~1.2 MB binary cost, produces the same `katex-html`+MathML markup as
   KaTeX ≥ 0.18 (CSS class names `katex-base`/`katex-strut` verified against 0.18.5 and 0.19.1
   CSS; 0.16.x CSS is incompatible) **[verified: probe + CSS diff]**.
   **Recommendation for HTML (single file, offline, emailed): server-side `katex-rs` with
   `OutputFormat::HtmlAndMathml`, plus vendored KaTeX 0.19.1 `katex.min.css` + 20 woff2 inlined
   (~285 KB raw / ~380 KB base64; no JS at all, build-time error reporting with source positions,
   accessible MathML, identical rendering everywhere).** Keep presenter's client-side KaTeX JS
   as `math.engine = "katex-js"` fallback (proven path; +273 KB JS) and `"mathml"` (math-core, +0 KB
   or +388 KB with embedded LM Math) as the light option.

4. **Image fallback via embedded typst works and is cheap** **[verified probe]**:
   `#set page(width: auto, height: auto, margin: 0pt, fill: none)` + `#set text(top-edge:
   "bounds", bottom-edge: "bounds")` (without the edges, inline math descenders are **clipped** —
   verified visually) → `typst_svg::svg(page, &SvgOptions::default())` gives an SVG whose glyphs are
   `<symbol><path>` outlines referenced by `<use>` (0 `<text>`, 0 font references); 2.6–12.7 KB per
   equation (inline `x^2+1/2` 4.3 KB, display `∫₀¹x²dx=1/3` 9.2 KB). PNG at 2×: resvg 0.48.1
   0.7–2.3 KB (120–700 µs), or directly `typst_render::render(page, &RenderOptions {
   pixel_per_pt: 2.0.into(), .. })` 0.8–2.5 KB (50–200 µs) — resvg is not needed. **Baseline**:
   the first `GroupItem` frame under the page has `has_baseline() == true` and
   `frame.baseline()` is the distance from the top (e.g. 9.447 pt of 13.242 pt for inline
   `x^2+1/2`; 17.334 of 29.159 pt for the display integral) and equals the y of the main-line
   `TextItem`s, so `descent = height − baseline` feeds `vertical-align: -{descent}pt` in HTML.
   Timing in a warm World: 15–65 µs per equation compile (mean 36 µs over 16 display equations),
   0.6 ms first compile after `World` creation, `World::new` with embedded fonts 1.3 ms.

5. **No Rust crate converts MathML → OMML adequately for Word** **[verified by running all
   candidates]**: deckmint-math targets PowerPoint (`<a14:m>`, DrawingML run props, accents →
   `m:limUpp`); mt-mathml/mt-omml (2 weeks old) loses `\left(…\right)` and n-ary bodies and
   chokes on `&ApplyFunction;`; tex2word-math/easydoc-math are LaTeX→OMML seeds (merged runs, no
   `sty p`). Write writer's own **MathML-Core → OMML** emitter modelled on texmath's
   `Writers/OMML.hs` (377 lines Haskell; mapping table in §6.3): ≈ 1,000–1,200 LOC Rust incl. tests.
   MathML then becomes the hub for both sources (LaTeX via math-core, typst via typst-html).

6. **typst math → MathML via typst 0.15.1 HTML export works for all 16 test equations with zero
   warnings** **[verified probe]**: compile `$ eq $` to `HtmlDocument`, call `typst_html::html`,
   extract `<math …>…</math>`; 62 µs per equation warm. Quirks: identifiers are emitted as
   Unicode math-italic code points (`<mi>𝑥</mi>`), `cal(F)` → `ℱ︀` (U+2131 U+FE00), alignment and
   cases depend on class names (`multiline-equation aligned`, `cases`) defined in typst's
   1,501-byte `EQUATION_CSS` which writer must copy into its stylesheet.

Conversion matrix for v1 is in §8.

---

## 1. Versions, licences, maintenance **[verified: `cargo info` + crates.io API 2026-10-24]**

| crate | latest | licence | MSRV | updated | created | downloads (90 d) | deps (normal) | role |
|---|---|---|---|---|---|---|---|---|
| pulldown-latex | 0.8.0 | MIT | 1.74.1 | 2026-07-28 | 2024-05-01 | 94,270 (57,898) | bumpalo | LaTeX → MathML (pull parser) |
| math-core | 0.9.0 | MIT | **1.96** | 2026-10-12 | 2025-06-16 | 25,477 (22,254) | hashbrown, kstring, math-core-renderer-internal, memchr, phf, rustc-hash, static_assertions, strum | LaTeX → MathML Core |
| latex2mathml | 0.2.3 | MIT | — | **2020-04-27** | 2020-04-05 | 122,143 (35,779) | none | unmaintained |
| tex2math | 2.4.1 | **LGPL-3.0-only** | — | 2026-08-26 | 2026-03-30 | 239 (69) | ? | avoid (licence, non-Core output) |
| mathml-core | 0.1.7 | MPL-2.0 | — | 2024-01-11 | 2023-05-04 | 20,759 (181) | — | MathML *type definitions* only, not a converter |
| latex2mmlc | — | — | — | — | — | — | — | **does not exist on crates.io** (`cargo info` → nothing) |
| katex | 0.4.6 | MIT OR Apache-2.0 | — | **2023-02-05** | 2020-02-01 | 1,068,817 (255,594) | quick-js 0.4.1 (C), derive_builder, itertools, thiserror, cfg-if | KaTeX 0.16.4 in QuickJS |
| katex-rs (lib name `katex`) | 0.3.0 | MIT | — (edition 2024) | 2026-09-05 | 2025-09-23 | 86,169 (66,350) | bon, phf, rapidhash, strum, thiserror (+ build: phf_codegen, serde, serde_json, unicode-normalization) | pure-Rust KaTeX 0.18.5 port |
| tex2word-math | 1.0.6 | MIT | 1.82 | 2026-07-23 | 2026-07-23 | 262 (241) | none | LaTeX → OMML seed |
| easydoc-math | 0.1.2 | Apache-2.0 | 1.88 | 2026-09-17 | 2026-08-25 | 459 (459) | easydoc-core | LaTeX ↔ OMML |
| deckmint-math | 0.1.9 (0.1.7 probed) | MIT | — | 2026-10-24 | 2026-04-02 | 688 (270) | roxmltree?, math-core | MathML → **PowerPoint** OMML |
| mt-mathml / mt-omml | 0.1.2 | MIT OR Apache-2.0 | 1.85 | 2026-10-14 | 2026-10-12 | 41 / 50 | mt-core | MathML ↔ tree ↔ OMML |
| ooxml-omml | 0.1.1-alpha.2 | MIT OR Apache-2.0 | — | 2026-03-19 | 2026-03-02 | 459 (230) | ooxml-* | OMML **reader/types** |
| docmux-transform-math | 0.6.2 | MIT | 1.80 | 2026-10-15 | 2026-10-14 | 76 | — | token rewriter (R2: unusable) |
| typort-math | 0.2.0 | Apache-2.0 | — | 2026-09-01 | 2026-05-19 | 424 (270) | typst | typst `Content` → OMML (R2 §5.5, not run) |
| resvg / usvg | 0.48.1 | Apache-2.0 OR MIT | 1.85.0 | 2026-08-02 | — | 32.6 M | tiny-skia 0.12, fontdb… | SVG → PNG |
| tylax | 0.3.8 | Apache-2.0 | — | 2026-09-14 | 2026-01-05 | 6,928 (2,822) | typst-syntax 0.14, mitex | LaTeX ↔ typst (R2) |
| mitex | 0.2.4 | Apache-2.0 | — | 2024-06-17 | 2024-04-19 | 34,615 (19,567) | — | LaTeX → typst (R2) |

npm: **KaTeX 0.19.1** (2026-10-23; 0.19.0 2026-10-01, 0.18.5 2026-08-31, 0.18.0 2026-07-17,
0.17.0 2026-05-22; MIT; unpacked 4,045,207 B; dep `commander ^15`). **Temml 0.14.0**
(2026-10-08, MIT; `temml.min.js` 167,881 B, `Temml.woff2` 9,404 B, per-font CSS ~9 KB; a
KaTeX-derived *MathML-only* JS renderer, needs a math font exactly like server-side MathML).

Fonts: `latinmodern-math.woff2` **388,212 B** (GUST Font License — "legally equivalent to the LaTeX
Project Public License", per fred-wang/MathFonts `LatinModern/mathfonts.css`), `.woff` 463,460 B;
`STIXTwoMath-Regular.woff2` **558,644 B** (SIL OFL 1.1) **[verified: HTTP HEAD on
fred-wang.github.io/MathFonts]**. `pulldown-latex` ships the same `latinmodern-math.woff2`
(387,976 B) + three LMRoman12 woff2 (≈46 KB each) in its crate `font/` dir and a 3,206-byte
`styles.css` **[verified: crate contents]**. macOS ≥ 13 preinstalls `STIX Two Math`
(`/System/Library/Fonts/Supplemental/STIXTwoMath.otf` present here); Windows has Cambria Math;
Latin Modern Math is in TeX Live (`kpsewhich latinmodern-math.otf` → Homebrew texlive path) but
**not registered with macOS** **[verified]**.

---

## 2. LaTeX → MathML in pure Rust

### 2.1 pulldown-latex 0.8.0 **[verified: source `~/.cargo/registry/src/*/pulldown-latex-0.8.0/`]**

Crate root re-exports (`src/lib.rs`):
```rust
pub mod config; pub mod event; pub mod mathml; pub mod parser;
pub use config::RenderConfig;
pub use event::Event;
pub use mathml::{push_mathml, write_mathml};
pub use parser::{error::ParserError, storage::Storage, Parser};
```
Parser (`src/parser.rs`):
```rust
pub struct Parser<'store> { /* instruction_stack, buffer, macro_context: MacroContext<'store>, storage: &'store bumpalo::Bump, span_stack */ }
impl<'store> Parser<'store> {
    pub fn new<'input>(input: &'input str, storage: &'store Storage) -> Self where 'input: 'store;
}
impl<'store> Iterator for Parser<'store> { type Item = Result<Event<'store>, ParserError>; }
```
Storage (`src/parser/storage.rs`): `pub struct Storage(bumpalo::Bump); pub fn new() -> Self;
pub fn reset(&mut self)` — "only used when user-defined macros are used… zero-overhead otherwise";
reset after each parse.

Renderer (`src/mathml.rs`, lines 1492–1519):
```rust
pub fn push_mathml<'a, I, E>(string: &mut String, parser: I, config: RenderConfig<'a>) -> io::Result<()>
where I: Iterator<Item = Result<Event<'a>, E>>, E: std::error::Error;
pub fn write_mathml<'a, I, W, E>(writer: W, parser: I, config: RenderConfig<'a>) -> io::Result<()>
where I: Iterator<Item = Result<Event<'a>, E>>, W: io::Write, E: std::error::Error;
```
Error handling: the renderer accepts the `Result` stream; an `Err(e)` item is rendered inline as
`<merror style="border-color: #b22222"><mtext>{e}</mtext></merror>` (mathml.rs:578–587) and
rendering continues. If the caller collects the events first (as my probe did) a `ParserError`
aborts the whole equation. `ParserError: Error + Display`; `Display` prints
`parsing error: <kind>\n╭─► context:\n│…\n│ ^^^^\n╰────` (multi-line, with macro-expansion
context). Kinds seen: "unknown primitive command found", "expected an argument", "unbalanced group
found, expected it to be closed with `}`".

Config (`src/config.rs`):
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderConfig<'a> {
    pub display_mode: DisplayMode,      // Inline (default) | Block  → <math display="block">
    pub annotation: Option<&'a str>,    // Some(src) → <semantics>…<annotation encoding="application/x-tex">
    pub error_color: (u8, u8, u8),      // default (178, 34, 34)
    pub xml: bool,                      // add xmlns="http://www.w3.org/1998/Math/MathML" ("helpful for … Microsoft Word")
    pub math_style: MathStyle,          // TeX (default: upright uppercase Greek) | ISO | French | Upright
}
impl<'a> RenderConfig<'a> { pub fn with_annotation(annotation: &'a str) -> Self; }
```
Events (`src/event.rs`): `Event::{Content(Content), Begin(Grouping), End, Visual(Visual),
Script{ty: ScriptType, position: ScriptPosition}, Space{…}, StateChange(StateChange),
EnvironmentFlow(EnvironmentFlow)}`; `Content::{Text, Ordinary{content, stretchy}, Number,
Function, Delimiter{content, size, ty}, Relation, BinaryOp, Punctuation, LargeOp, Identifier…}`;
`Font` (incl. `BoldSymbol`, `DoubleStruckItalic` since 0.8.0). A writer-owned OMML emitter could
consume this stream directly instead of MathML (see §6.4).

Coverage: `src/parser/primitives.rs` matches **741 distinct command names** (listed in the probe
output; includes `frac dfrac tfrac cfrac binom sqrt left right middle big…Bigg operatorname
text textbf textit textrm textsf texttt mathbb mathbf mathcal mathfrak mathit mathrm mathsf
mathtt boldsymbol symbf… color textcolor colorbox fcolorbox hat vec dot ddot bar tilde
widehat overline underline overbrace underbrace overset underset stackrel substack sideset
cancel phantom kern hskip mkern mskip hspace quad qquad def let futurelet newcommand
renewcommand providecommand begin end` + environments `matrix pmatrix bmatrix Bmatrix vmatrix
Vmatrix smallmatrix array darray cases dcases rcases drcases aligned alignedat align alignat
gather gathered equation split multline subarray`, and ~500 symbols). README (0.8.0): "95% of
what KaTeX and the likes support is properly working and minimally tested… not recommended for
large scale production use". Unsupported by design: `\over`, `\atop`, `\hfil(l)`, `\edef`,
`\csname`, catcodes, `\if*`, raise/lower boxes; `\sideset` renders only the base. Macro expansion
capped at depth 64 / 1 MiB (0.8.0). Changelog 0.8.0 (2026-07-27) added `\mathord…\mathinner`,
`\overbracket`, `\substack`, dvipsnames colours, `\textrm…\texttt`, fuzzing, renderer recovery
from malformed streams.

CSS: the crate's `styles.css` (3,206 B) sets `@font-face` for Latin Modern Math/LMRoman12, `m|math
{font-family: Latin Modern Math}` and alignment rules for the classes it emits on `mtable`
(`menv-alignlike`, `menv-align`, `menv-arraylike`, `menv-cells-left/right`, `menv-multline`).
Without that CSS, `aligned` columns are centred instead of right/left (Chrome ignores
`columnalign`, §3.2).

### 2.2 math-core 0.9.0 **[verified: source]**

```rust
pub use mathml_renderer::ast::{CssClassNames, Indentation, Warnings};
pub use self::error::LatexError;                       // pub struct LatexError(pub Range<usize>, pub(crate) LatexErrKind)
pub enum MathDisplay { Inline, Block }
pub enum PrettyPrint { Never, Always, Auto }            // [names recalled from docs; enum verified to exist]
pub enum UnicodeSubstitution { … Never … }
pub struct MaxExpansions(pub u32);
pub enum TaggingStrategy { … }
pub struct MathCoreConfig {
    pub pretty_print: PrettyPrint,
    pub xml_namespace: bool,              // add xmlns=… to <math>
    pub ignore_unknown_commands: bool,    // true → red text (class css_classes.unknown_command = "math-core-unknown-cmd") instead of Err
    pub annotation: bool,                 // wrap in <semantics> + <annotation encoding="application/x-tex">
    pub allow_unreliable_rendering: bool, // allow commands whose MathML Core rendering is unreliable in browsers
    pub global_group: bool,               // \newcommand at top level persists across snippets (KaTeX globalGroup)
    pub unicode_substitution: UnicodeSubstitution,
    pub css_classes: CssClassNames,
    pub indentation: Indentation,
    pub max_expansions: MaxExpansions,
    pub id_prefix: String,                // prefix for generated ids (equation tags)
    pub tags: TaggingStrategy,            // equation numbering
}   // MathCoreConfig: Default
pub struct LatexToMathML { /* emitter_cfg, state: GlobalState, parser_cfg */ }
impl LatexToMathML {
    pub fn new(config: MathCoreConfig, macros: Vec<(String, String)>) -> Result<Self, MacroParseError>;   // macros: (name, replacement) with #1..#9
    pub fn convert_with_global_state(&mut self, latex: &str, display: MathDisplay) -> Result<ConvertResult, Box<LatexError>>;
    pub fn convert_with_local_state(&self, latex: &str, display: MathDisplay) -> Result<ConvertResult, Box<LatexError>>;
    pub fn reset_global_state(&mut self);
    pub fn convert_all<S: AsRef<str>>(&self, snippets: &[(S, MathDisplay)]) -> Vec<Result<ConvertResult, Box<LatexError>>>;
}
pub struct ConvertResult { pub mathml: String, pub warnings: Warnings }
impl LatexError {
    pub fn to_html(&self, latex: &str, display: MathDisplay, css_class: Option<&str>) -> String;  // renders the error as HTML
    pub fn error_message(&self) -> String;   // e.g. `Unknown command "\foo".`, `Expected closing token "}", but reached end of input.`
    pub fn to_message(&self, s: &mut String, input: &str);
    pub fn label(&self) -> &'static str;
    pub fn to_report<'name>(…);              // ariadne-style report
}
```
`LatexError.0` is the byte range in the input → writer can map it to the source position of the
`$…$` span for diagnostics. README (0.9.0): goals "Support all common LaTeX math commands, at least
those that KaTeX supports… Try to avoid CSS hacks… don't use JavaScript… Support many different
math fonts"; "Required CSS" section: markup "work[s] almost without a CSS style sheet", but a math
font is needed for LaTeX-like rendering; it documents Chromium not using `ssty` variants for
scripts (primes) and Safari accent spacing issues, and ships patched fonts (Latin Modern Math,
Libertinus Math, Noto Sans Math) in `tmke8/math-core-fonts`. Also a playground + comparison table
with KaTeX at tmke8.github.io/math-core.

### 2.3 Others
* `latex2mathml` 0.2.3: `pub fn latex_to_mathml(latex: &str, display: DisplayStyle) -> Result<String, LatexError>`
  (`DisplayStyle::{Inline, Block}`); last release 2020 **[verified: crates.io]**. Emits
  `mathvariant="double-struck"/"bold-italic"` (not MathML Core; Chrome ignores all values except
  `normal` — see §3.2) and prints `[PARSE ERROR: …]` as `<mtext>` for unknown commands.
* `tex2math` 2.4.1: `tex2math::convert(input, &ConvertOptions::display()) -> Result<String, _>`;
  **LGPL-3.0-only**; emits `<mstyle mathvariant=…>` and `columnalign` (both non-Core); `\def` and
  `\newcommand` come out as literal `<mi>\def</mi>`. Not recommended.
* `mathml-core` 0.1.7 is a MathML AST/type crate (no LaTeX parser). `latex2mmlc` does not exist.
  `mathml-latex` 0.0.3 ("Convert between MathML and LaTeX") exists but was not inspected.
* KaTeX itself can emit MathML (`output: mathml`) — see §4; `katex-rs` does too (§4.3).
* pandoc 3.12.1 `--math-method=mathml` (texmath) is an available oracle on this machine
  (`echo '$$…$$' | pandoc -t html --math-method=mathml`); it fails hard on unknown commands
  ("Could not convert TeX math \foo{x}, rendering as TeX").

### 2.4 Probe results (display mode) **[verified: `$SB/math-probe/mathml-out.txt`]**

Legend: ✅ valid Core-ish output, ⚠ output with a caveat, ❌ failure.

| # | input | pulldown-latex 0.8.0 | latex2mathml 0.2.3 | math-core 0.9.0 | tex2math 2.4.1 | pandoc/KaTeX-rs (oracles) |
|---|---|---|---|---|---|---|
| 1 | `\frac{a}{b}` | ✅ `<mfrac><mrow><mi>a</mi></mrow><mrow><mi>b</mi></mrow></mfrac>` | ✅ `<mfrac><mi>a</mi><mi>b</mi></mfrac>` | ✅ same as l2m | ✅ | ✅ identical |
| 2 | `\sum_{i=0}^{n} i` | ✅ `<munderover><mo movablelimits="false">∑</mo>…` | ✅ | ✅ `<mo lspace="0">∑</mo>` | ✅ | ✅ |
| 3 | `\int_0^1 x \, \mathrm{d}x` | ✅ `<msubsup><mo movablelimits="false">∫</mo>…<mspace width="0.16666667em"/><mi mathvariant="normal">d</mi>` | ✅ | ✅ (`<mrow><mspace/><mi mathvariant="normal">d</mi></mrow>` — a width-less `<mspace/>` that mt-mathml rejects) | ⚠ `<mstyle mathvariant="normal">` | ✅ |
| 4 | `\sqrt[n]{x}` | ✅ `<mroot><mrow><mi>x</mi></mrow><mi>n</mi></mroot>` | ✅ | ✅ | ✅ | ✅ |
| 5 | `\mathbb{R}` | ✅ `<mi>ℝ</mi>` (U+211D) | ⚠ `<mi mathvariant="double-struck">R</mi>` (non-Core) | ✅ `<mi>ℝ</mi>` | ⚠ `<mstyle mathvariant>` | pandoc `<mi mathvariant="double-struck">ℝ</mi>`; katex-rs `mathvariant="double-struck">R` |
| 6 | `\left(\frac{a}{b}\right)` | ✅ `<mrow><mo stretchy="true">(</mo>…<mo stretchy="true">)</mo></mrow>` | ✅ (+`form="prefix/postfix"`) | ✅ `<mrow><mo>(</mo>…<mo>)</mo></mrow>` (default fence stretching) | ✅ | ✅ |
| 7 | `pmatrix` | ⚠ `<mtable class="menv-arraylike">` (needs its CSS for spacing only) | ✅ | ✅ `<mtable displaystyle="false" scriptlevel="0">` | ⚠ `columnalign="center"` | ✅ |
| 8 | `aligned` | ⚠ `<mtable class="menv-alignlike menv-align">` — **alignment needs styles.css** | ❌ `UnknownEnvironment("aligned")` | ✅ `<mtd style="text-align: right;justify-items: end;padding-right: 0;">` (self-contained) | ⚠ `columnalign="right left"` (ignored by Chrome) | pandoc: inline `style=` like math-core |
| 9 | `\text{if } x > 0` | ⚠ `<mtext>if&nbsp;</mtext>` (`&nbsp;` entity: HTML-only) | ⚠ trailing space lost | ✅ `<mtext>if </mtext>` | ✅ | ✅ |
| 10 | `\hat{x} \vec{v} \dot{y}` | ⚠ `<mover><mrow><mi>x</mi></mrow><mi>^</mi></mover>` — accent as `<mi>`, no `accent="true"` (README TODO) | ✅ `<mover><mi>x</mi><mo accent="true">^</mo></mover>` (`mo accent` unsupported in Chrome) | ✅ `<mover accent="true"><mi>x</mi><mo stretchy="false">ˆ</mo></mover>`, vec → U+20D7 | ✅ | pandoc uses combining U+0302/U+0307 |
| 11 | `\operatorname{argmax}_x f(x)` | ✅ `<msub><mi>argmax</mi><mi>x</mi></msub><mo>⁡</mo><mspace width="0.1667em"/>…<mo symmetric="false" stretchy="false">(</mo>` | ✅ (no ⁡) | ⚠ `<mo lspace="0.1667em">&ApplyFunction;</mo>` — **named entity, not XML** | ⚠ `mathvariant="normal"` on multi-letter mi | ✅ |
| 12 | `\color{red}{x} \textcolor{blue}{y}` | ✅ `<mrow style="color: rgb(255 0 0)">` | ❌ PARSE ERROR text inline | ✅ `<mrow style="color:#FF0000;">` | ⚠ `mathcolor` (deprecated) | — |
| 13 | `\boldsymbol{\alpha} \mathbf{x}` | ✅ `<mi>𝜶</mi><mi>𝐱</mi>` (U+1D6FC bold-italic, U+1D431 bold) | ⚠ `mathvariant="bold-italic"` | ✅ Unicode | ⚠ mstyle | pandoc: `𝜶 𝒙`(bold-italic x!) |
| 14 | `\lim_{x\to\infty} f(x)` | ✅ `<munder><mi>lim</mi>…` | ✅ | ✅ | ✅ | ✅ |
| 15 | `\def\R{\mathbb{R}} \R` | ✅ | ❌ | ✅ | ❌ garbage | — |
| 16 | `\newcommand{\dd}{\mathrm{d}} \dd x` (no `[n]`) | ❌ "expected an argument" (`new_command` requires the optional `[n]`: `lex::optional_argument(..).ok_or(ErrorKind::Argument)?`, primitives.rs:2253) | ❌ | ✅ | ❌ | katex-rs ✅, pandoc ✅ |
| 17 | `\newcommand{\dd}[0]{…}` / `[1]{\frac{#1}{2}} \half{x}` | ✅ / ✅ | ❌ | ✅ / ✅ | ❌ | — |
| 18 | `\foo{x}` | ❌ Err "unknown primitive command found" (inline `<merror>` if the parser is streamed) | ⚠ `<mtext>[PARSE ERROR…]</mtext>` | ❌ Err `Unknown command "\foo".` (or red text with `ignore_unknown_commands`) | ⚠ `<mi>\foo</mi>` silently | katex-rs: red `<mtext>\foo</mtext>` |
| 19 | `\frac{a}{b` | ❌ "unbalanced group…" | ❌ | ❌ "Expected closing token…" | ⚠ `<merror>` inline | — |

Verdict: **math-core** and **pulldown-latex** are both production candidates; math-core needs no
CSS for alignment and handles plain `\newcommand`; pulldown-latex has the stream API and the
bigger symbol table. Neither supports `\mhchem`/`\ce`. Post-process math-core output by
replacing `&ApplyFunction;` → `&#x2061;` when the MathML is fed to an XML parser (OMML pipeline).

---

## 3. MathML Core browser support (2026)

### 3.1 Support data **[verified: MDN `/Web/MathML`, caniuse.com/mathml, mdn/browser-compat-data `main` JSON, all fetched 2026-10-24]**
* MDN: Baseline **"Widely available"** — "available across browsers since January 2023"; MDN uses
  MathML Core as its reference spec; "Developers are highly recommended to switch to MathML Core".
* caniuse: Chrome **109** (24–96 removed/disabled, 97–108 behind flag), Edge 109, Firefox 2 (full),
  Safari 10 (3.1–9.1 partial), Safari iOS 5, Opera 95, Samsung 21; global usage **96.02 %**.
* BCD per element (`chrome / firefox / safari`, first version): `math` 109/4/5.1 (`display` attr
  109/1/5.1), `mfrac` 109/1/5.1 (`linethickness` 109/1/9; `numalign/denomalign` deprecated,
  Chrome no), `mo` 109/1/5 (`stretchy`, `symmetric`, `largeop`, `lspace/rspace`, `minsize/maxsize`,
  `movablelimits`, `form` all 109/1/8–10; **`accent` on mo: Chrome no**), `msubsup` 109/1/6,
  `munderover` 109/1/6 (`accent`/`accentunder` 109/1/10), `mroot` 109/1/6, `mspace` 109/1/7,
  `mtext` 109/1/6, `mstyle` 109/6/5.1 (its legacy attrs deprecated, Chrome no), `mpadded`
  109/1/6, `mphantom` 109/1/6, `semantics` 109/1/5, `mtable` 109/1/5.1 but **`columnalign`,
  `rowalign`, `columnlines`, `rowlines`, `frame`, `columnspacing`, `rowspacing`, `width`: Chrome
  no** (Firefox yes, Safari partly) → alignment must be done with CSS (`text-align`,
  `justify-items`, `padding`) as math-core, pandoc and typst do.
* Not standard / not in Core: `mfenced` (deprecated; Chrome no, Firefox 1–73 removed, Safari 6),
  `menclose` (non-standard; Chrome no), `mlabeledtr`, `maction` (in Core but inert).
* `mi mathvariant`: "The only allowed value in MathML Core is 'normal'" (Chrome 109 implements only
  that; Firefox/Safari still accept legacy values) → emit Unicode math alphanumerics (𝐱 𝜶 ℝ 𝒜)
  for bold/double-struck/script, as pulldown-latex, math-core and typst do.
* Global `mathcolor`, `mathbackground`, `mathsize` deprecated (use CSS `color`, `background`,
  `font-size`); `displaystyle`, `scriptlevel` supported (Chrome 109, Firefox 83/70, Safari 14/—).
* CSS generic `font-family: math`: Chrome 109, Firefox **149**, Safari **26.2** (BCD).
* MathML Core spec (w3c.github.io/mathml-core, fetched): 31 elements; UA stylesheet "should set the
  font-family to the math value on the `<math>` element instead of inheriting it", `display:
  inline math` / `block math`, `math-depth: 0`, `font-size: inherit`, `math-style: compact`
  (normal for `display="block"`); stretchy operators fall back to the `<mtext>` algorithm when no
  stretchy glyph can be shaped.

### 3.2 Rendering quality and fonts **[verified: test page in the Claude browser pane = "Chrome/152.0.7977.167" (Chromium 152) on macOS 26.7, with STIX Two Math installed locally]**
Test page: `…/scratchpad/mathml-test/index.html` (pulldown-latex and math-core output for the 13
inputs in four sections). Observed:
1. **Browser default** (`font-family: math` from the UA sheet, `document.fonts.check('16px "STIX
   Two Math"') == true`): **poor** — `\left(\frac ab\right)` and `pmatrix` parentheses not
   stretched, `∑`/`∫` at text size with cramped limits, `\sqrt[n]{x}` radical index misplaced.
   I.e. Chromium did **not** resolve the `math` generic to the installed STIX Two Math; the
   Chromium commit that introduced the generic hard-codes "Latin Modern Math" on desktop
   (WebSearch result citing chromium.googlesource.com/…/2103612e; a user-configurable default was
   added later) **[unverified beyond the search summary]**. Latin Modern Math is not installed as a
   system font here, so the fallback is the page's serif font.
2. **`@font-face { font-family: LMMathWeb; src: url(latinmodern-math.woff2) }` + `math {
   font-family: LMMathWeb }`**: correct — stretchy delimiters, display-size ∑ ∫ with limits,
   proper radicals/accents (both converters).
3. **`math { font-family: "STIX Two Math" }` (local)**: correct, same quality.
4. **`math { font-family: Georgia, serif }`** (no MATH table): same defects as 1.
Headless `Google Chrome 155 --screenshot` could not be made to run in this sandbox (hangs with
"Trying to load the allocator multiple times"); the Chromium 152 pane result stands.

Consequence: a document tool emitting MathML **must** add
```css
math { font-family: "Latin Modern Math", "STIX Two Math", "Cambria Math", "Libertinus Math", math; }
```
and, to be safe on machines without any of these, embed a math font (`@font-face` with LM Math
388 KB or STIX Two Math 559 KB woff2, base64 ×1.37 when inlined). MDN "Fonts for MathML":
recommends Latin Modern Math + STIX Two Math; Windows has Cambria Math by default ("relatively
good"), macOS ≥ 13 ships STIX Two Math, Linux needs `fonts-lmodern`/`fonts-stix` (Debian ≤ 13's
`fonts-stix` is STIX 1.x, not Two). math-core's README additionally notes Chromium ignores `ssty`
(primes rendered wrong) and Safari accent spacing issues and ships patched fonts.
Firefox (old but complete MathML implementation) and Safari (WebKit) also need a MATH-table font;
not tested here **[unverified]**.

---

## 4. KaTeX

### 4.1 Versions and payload **[verified: registry.npmjs.org, data.jsdelivr.com]**
* Latest **0.19.1** (2026-10-23). presenter_rs vendors 0.19.0 (2026-10-01). 0.19.0 release notes
  list one breaking change (strict-mode `symbolNotInFont` reporting), nothing about CSS/MathML
  **[verified: GitHub release page]**.
* `dist/katex.min.js` 273,081 B; `dist/katex.min.css` 24,793 B; `dist/contrib/auto-render.min.js`
  3,486 B; fonts: 20 × woff2 = 259,792 B (all 60 font files incl. woff/ttf = 1,076,572 B). Needed for
  a single-file HTML: **557,666 B raw** (js+css+woff2); presenter's measured budget 595 KB
  (R6/R8). Without JS (server-side HTML): css + woff2 = **284,585 B raw ≈ 380 KB base64**.
* CSS class names changed between 0.16.22 (`.base`, `.strut`) and 0.18.5/0.19.1 (`.katex-base`,
  `.katex-strut`; both reference the same 20 woff2 files) **[verified: grep of the three CSS files]**.
  → HTML produced by a KaTeX-0.18+ engine needs ≥ 0.18 CSS; 0.16.x CSS will not style it.

### 4.2 Output option **[verified: katex.org/docs/options]**
`output`: `html` (HTML only), `mathml` (MathML only), `htmlAndMathml` (HTML for visual rendering
+ MathML for accessibility; **default**). Other options: `displayMode`, `throwOnError` (default
true; false → shown in `errorColor` #cc0000), `macros` (object, mutated by `\gdef`, reusable),
`globalGroup`, `strict` (default "warn"), `trust` (default false), `maxSize`, `maxExpand` (1000),
`leqno`, `fleqn`, `minRuleThickness`. KaTeX's MathML-only output is accessibility-grade (wraps in
`<span class="katex"><math><semantics>…<annotation>`), e.g. `\mathbb{R}` → `<mi
mathvariant="double-struck">R</mi>` (non-Core) and `\begin{pmatrix}` → `<mtable columnspacing=…
columnalign=…>` with `<mstyle>` per cell — it is not tuned for MathML Core (that is Temml's job).

### 4.3 Running KaTeX from Rust without Node **[verified: probes]**
**(a) `katex` 0.4.6** (xu-cheng/katex-rs; vendors `vendor/katex.min.js` 276,729 B = **KaTeX
0.16.4**; features `quick-js` (default, `quick-js 0.4` C sources, "patched"), `duktape`, `wasm-js`).
```rust
pub fn render(input: &str) -> Result<String>;
pub fn render_with_opts(input: &str, opts: impl AsRef<Opts>) -> Result<String>;
pub const KATEX_VERSION: &str;              // "0.16.4"
pub enum OutputType { Html, Mathml, HtmlAndMathml }
Opts::builder().display_mode(bool).output_type(OutputType).leqno(..).fleqn(..).throw_on_error(..)
    .error_color(String).macros(HashMap<String,String>).min_rule_thickness(f64).max_size(Option<f64>)
    .max_expand(Option<i32>).trust(bool).build()?
```
Release probe: binary 1,408,032 B (opt-level 3, thin LTO, stripped; +~1.1 MB over a hello-world),
first render incl. QuickJS init + loading katex.min.js **36.8 ms**, then **0.475 ms per render**
(thread-local engine). Last crate release 2023-02-05 → stale KaTeX; not recommended.

**(b) `katex-rs` 0.3.0** (lib name `katex`; "Rust re-implementation of the KaTeX rendering engine
… currently tracks KaTeX 0.18.5, upstream main commit 49904aa"; edition 2024; build.rs generates
phf tables from `data/{symbols,font_metrics_data,sigmas_and_xis}.json` (256 KB); 10 k LOC src).
```rust
use katex::{render_to_string, KatexContext, Settings, OutputFormat, ParseError};
pub fn render_to_string(ctx: &KatexContext, expression: &str, settings: &Settings) -> Result<String, ParseError>;
pub enum OutputFormat { HtmlAndMathml, Html, Mathml }
let settings = Settings::builder().display_mode(true).output(OutputFormat::HtmlAndMathml)
    .throw_on_error(false).build();          // bon builder; fields are pub: display_mode, output, leqno, fleqn,
                                             // throw_on_error, error_color: String, macros: RefCell<MacroMap>,
                                             // min_rule_thickness, color_is_text_color, strict: StrictSetting,
                                             // trust: TrustSetting, max_size, max_expand, global_group …
pub const VERSION: &str;                     // "0.3.0"
```
Probe (15 inputs, `OutputFormat::Mathml`, display): all rendered, incl. `\newcommand{\dd}{…}`
without `[n]` and `\foo{x}` (red `<mstyle mathcolor="#cc0000"><mtext>\foo</mtext></mstyle>` with
`throw_on_error(false)`). `KatexContext::default()` 0.67 ms; **33 µs per HTML render, 17 µs per
MathML render**; release binary 1,581,328 B (+~1.2 MB). HTML output uses `katex-display`,
`katex-html`, `katex-base`, `katex-strut`, `mord`, `mfrac`, `vlist-*` classes = KaTeX ≥ 0.18
markup → style with KaTeX 0.18.5–0.19.1 `katex.min.css` + fonts. README claims "Pixel perfect
rendering with KaTeX across all platforms" and a screenshot regression harness; crates.io shows
66 k downloads in 90 days. Risk: young (first release 2025-09), lags upstream by one minor.

### 4.4 Recommendation for the HTML target (single-file, offline, emailed)
Default **`math.engine = "katex"`: server-side `katex-rs` 0.3.0, `OutputFormat::HtmlAndMathml`,
`throw_on_error(false)` + writer diagnostics from `ParseError` at build time**, with vendored KaTeX
0.19.1 `katex.min.css` (paths rewritten to data: URIs) + the 20 woff2 (presenter's
`scripts/vendor.sh` pins → update to 0.19.1 and drop `katex.min.js` from the inlined set). Payload
≈ 380 KB inlined, no JS, no flash-of-unrendered-math, prints correctly, screen-reader MathML
included, `writer serve` can morphdom-patch without re-running KaTeX.
Alternatives kept behind config: `"katex-js"` (presenter's proven client-side path: +273 KB JS,
needed only if katex-rs misrenders something), `"mathml"` (math-core 0.9.0 output + `math {
font-family }` rule; optional `math.font = "embed"` inlining Latin Modern Math 388 KB — larger
than KaTeX's fonts, so this is the *lightweight* option only when no font is embedded), and
`"image"` (typst SVG, §5; no fonts at all, not selectable). Later optimisation: inline only the
KaTeX font files actually referenced by the generated HTML (`KaTeX_Main`, `KaTeX_Math`, `Size1–4`
cover most documents, ≈ 100 KB).

---

## 5. Equation images via the embedded typst compiler **[verified: `$SB/math-probe/typst-eq`]**

### 5.1 Recipe
World: the R1 §5 `MemWorld` with `typst_kit::fonts::embedded()` (includes "New Computer Modern
Math" — glyphs in the SVG are from it, Libertinus Serif for text). Source:
```typst
#set page(width: auto, height: auto, margin: 0pt, fill: none)   // fill: none → no white background rect
#set text(top-edge: "bounds", bottom-edge: "bounds")             // REQUIRED for inline math (see 5.2)
$x^2 + 1/2$          // inline  — or —  $ integral_0^1 x^2 dif x = 1/3 $   // display
```
Pipeline: `typst::compile::<PagedDocument>(&world)` → `doc.pages()[0]` (`Page { frame: Frame, fill:
Smart<Option<Paint>>, … }`) → `typst_svg::svg(&page, &SvgOptions { render_bleed: false, pretty:
false })` → SVG string; PNG via `typst_render::render(&page, &RenderOptions { pixel_per_pt:
2.0.into(), render_bleed: false }).encode_png()` (`RenderOptions::default()` is already 2 px/pt),
or via resvg: `usvg::Tree::from_str(&svg, &usvg::Options::default())`, `Pixmap::new(w, h)`,
`resvg::render(&tree, Transform::from_scale(s, s), &mut pixmap.as_mut())`, `pixmap.encode_png()`.

### 5.2 Measurements
| variant | page (pt) | baseline from top (pt) | SVG bytes | `<symbol>`/`<use>`/`<text>`/"font" | PNG 2× resvg | PNG 2 px/pt typst_render |
|---|---|---|---|---|---|---|
| inline `$x^2 + 1/2$`, default edges | 31.317 × 7.513 | 7.513 (= bottom) | 4,327 | 4 / 5 / 0 / 0 | 84×21 px, 724 B | 63×15 px, 811 B — **denominator clipped** (drawn at y = 11.308 > 7.513) |
| same with `top-edge/bottom-edge: "bounds"` | 31.317 × 13.242 | **9.447** (descent 3.795) | 4,342 | 4 / 5 / 0 / 0 | 84×36 px, 999 B | 63×26 px, 1,076 B — correct |
| `Hg $x^2 + 1/2$ y` default edges | 56.012 × 7.513 | 7.513 | 8,140 | 7 / 8 / 0 / 0 | 150×21, 1,313 B | 112×15 — "g" and denominators clipped (viewed) |
| inline `$sum_(i=0)^n x_i$` default | 37.567 × 7.612 | 7.513 | 7,117 | 6 / 7 / 0 / 0 | 101×21 | 75×15 (limits clipped) |
| display `$ integral_0^1 x^2 dif x = 1/3 $` | 65.719 × 29.159 | **17.334** | 9,173 | 9 / 10 / 0 / 0 | 176×78 px, 2,324 B (363 µs) | 131×58 px, 2,473 B (196 µs) — correct (viewed) |
| display `$ mat(1, 2; 3, 4) $` | 32.692 × 26.312 | 15.906 | 5,272 | 6 / 6 / 0 / 0 | 88×71, 1,840 B | 65×53, 2,070 B |
| inline, `fill: none`, bounds | 31.317 × 13.242 | 9.447 | 4,255 (no `<path fill="#ffffff">`) | | 822 B (transparent) | 780 B |
| inline, 12 pt, `fill: rgb("#1a3a8a")` | 40.982 × 15.272 | 9.704 | 6,950 (`fill="#1a3a8a"` on `<use>`) | | 2,084 B | |
| 16 display equations (frac…attach) | 8–63 × 5–29 | | **2,612–12,707** | | | |

SVG structure (verified by reading `inline-x2.svg`): `<svg viewBox="0 0 W H" width="Wpt"
height="Hpt" xmlns… xmlns:xlink…>` + per glyph `<g transform="matrix(1 0 0 -1 x y)"><use
xlink:href="#g<hash>" fill="#000000"/></g>`, fraction rule as `<path stroke=…>`, and `<defs><symbol
id="g<hash>" overflow="visible"><path d="…"/></symbol>…</defs>`. No `<text>`, no font-family, no
`<image>` → the SVG is self-contained; it renders in browsers, in Word (SVG + PNG fallback, R3
§2.11) and in resvg without any font. Units are `pt`; usvg converts to CSS px (×96/72: 31.317 pt
→ 41.757 px), so resvg's `scale = 2.0` gives 2 px per CSS px = 2.667 px/pt; use `scale = ppt × 0.75`
to match typst_render's `pixel_per_pt`.

**Clipping rule**: typst's default `bottom-edge: "baseline"` / `top-edge: "cap-height"` size the
line box from the text metrics, and an inline equation frame has `size.y = 7.513 pt` with its
denominator placed *outside* the frame (y = 11.308). With `"bounds"` edges the frame grows to the
glyph bounds (13.242 pt) and nothing is clipped. Display (`$ … $`) equations are block frames sized
by the math layout and are not clipped (verified image: ∫ limits intact).

**Baseline** (`typst_library::layout::Frame`, `frame.rs:111`): `pub fn baseline(&self) -> Abs {
self.baseline.unwrap_or(self.size.y) }`, `has_baseline()`, `ascent()` (= baseline), `descent()`
(= height − baseline), `set_baseline`. The page frame itself has no explicit baseline; its first
`FrameItem::Group(GroupItem { frame, transform, clip, label, parent })` (the paragraph line /
the equation block) has `has_baseline() == true`, and its `baseline()` coincides with the `y` of
the `FrameItem::Text` items on the main line (`𝑥`, `+`, `=`, `(`, `)` at exactly that y in every
probe) → it is the typographic baseline. Walk: `for (pos, item) in page.frame.items() { if let
FrameItem::Group(g) = item { if g.frame.has_baseline() { baseline = pos.y + g.frame.baseline() } } }`
(recursive helper `dump` in the probe). Expose as `EquationImage { svg, png, width_pt, height_pt,
baseline_pt }`.

HTML inline alignment: `<img src="data:image/svg+xml;base64,…" style="height:{H}pt;
vertical-align:-{H−baseline}pt" alt="{tex}">` (standard CSS; the img box bottom sits at the text
baseline shifted down by the descent) **[CSS semantics, not re-measured]**. DOCX: inline
`w:drawing` pictures sit on the baseline; shift with `w:rPr/w:position` (half-points, negative =
lower) on the run containing the drawing, or use an anchored drawing with `wp:positionV
relativeFrom="line"` **[unverified — open question §10]**.

### 5.3 Timing (release, Apple Silicon) **[verified]**
* `MemWorld::new` incl. `typst_kit::fonts::embedded()`: 3.8 ms first, 1.3 ms subsequently.
* First compile after World creation: 4.7 ms (first ever), 0.63 ms (later Worlds).
* Warm World, new equation each time (`Source::replace`, `comemo::evict(10)` between): **15–65
  µs per display equation (mean 36 µs over 16)**; unchanged recompile 1–11 µs; SVG export 25–190 µs.
* HTML export (compile `HtmlDocument` + `typst_html::html`): **62 µs mean** per equation.
* resvg parse+render+PNG: 120–700 µs; typst_render + PNG: 50–200 µs.
So rendering every equation of a 500-equation document as SVG costs ≈ 20–50 ms. The 47 MB probe
binary is dominated by the embedded fonts (same as R1).

### 5.4 resvg 0.48.1 API **[verified: source]**
```rust
// resvg re-exports `usvg` and `tiny_skia` (0.12)
pub fn resvg::render(tree: &usvg::Tree, transform: tiny_skia::Transform, pixmap: &mut tiny_skia::PixmapMut);
pub fn resvg::render_node(node: &usvg::Node, transform: tiny_skia::Transform, pixmap: &mut tiny_skia::PixmapMut) -> Option<()>;
impl usvg::Tree { pub fn from_str(text: &str, opt: &usvg::Options) -> Result<Self, usvg::Error>;
                  pub fn from_data(data: &[u8], opt: &usvg::Options) -> Result<Self, usvg::Error>;
                  pub fn size(&self) -> usvg::Size; /* width()/height() f32, CSS px */ pub fn fontdb(&self) -> &Arc<fontdb::Database>; }
tiny_skia::Pixmap::new(w: u32, h: u32) -> Option<Pixmap>; pixmap.as_mut() -> PixmapMut; pixmap.encode_png() -> Result<Vec<u8>, png::EncodingError>;
tiny_skia::Transform::from_scale(sx: f32, sy: f32)
```
Default features `svgz, text, system-fonts, memmap-fonts, raster-images` (R3 §6); the `text`
feature is irrelevant for typst SVGs (no `<text>`), so `default-features = false` is possible if
resvg is used only for equations — but **typst_render already produces the PNG**, so v1 does not
need resvg for math at all (R3 needs it for SVG → PNG fallbacks of user images anyway).

---

## 6. OMML (DOCX math)

### 6.1 Rust crates — measured **[verified: `$SB/omml-probe/omml-out.txt`, sources]**
| crate | API | 14-input behaviour | verdict |
|---|---|---|---|
| tex2word-math 1.0.6 | `tex2word_math::to_omath(latex: &str) -> String` (`<m:oMath>…`, no ns decl) | see R3 §4.2: merged runs (`<m:t>i=0</m:t>`, `<m:t>f(x)</m:t>`), no `sty p` on operators, `aligned` → `m:m`, `\lim` → `sSub`, `\color` → literal "red"/"blue" runs, unknown cmd silently dropped | seed only |
| easydoc-math 0.1.2 | `easydoc_math::latex_to_omml::convert(latex: &str) -> easydoc_core::Result<String>` | byte-identical to tex2word-math on 11/14; better: `aligned` → `<m:eqArr>`, `\lim` → `<m:limLow>`; `\color` → `Err("…不支持的 LaTeX 命令 \color…")`; accents as combining U+0302/U+20D7/U+0307 | seed; Apache-2.0 |
| deckmint-math 0.1.7 (0.1.9 latest) | `deckmint_math::mathml_to_pptx_omml(&str) -> Result<String, MathError>`, `latex_to_omml(&str)` | wraps in `<a14:m><m:oMathPara><m:oMathParaPr><m:jc m:val="centerGroup"/>…`, every run gets `<a:rPr><a:latin typeface="Cambria Math" …/>` (DrawingML, not valid in `w:document`), `mfrac` without `fPr`, accents → `m:limUpp` (wrong), colour dropped, `m:box` for `\mathrm`, fails on `&ApplyFunction;` | PowerPoint only |
| mt-mathml 0.1.2 + mt-omml 0.1.2 | `mt_mathml::parse(xml: &str) -> Result<Math, ParseError>`; `mt_omml::write(&Math) -> String`, `write_paragraph(&Math, align: Option<&str>)`, `mt_omml::NAMESPACE` | nicest structure of the lot: `<m:sty m:val="p"/>` on `=`/digits/parens, `m:mPr`+`m:mcs` matrices, `m:acc`, `m:nor`+`sty p` text runs; but `\left(…\right)` → plain `(` `)` runs (no `m:d`), `∑` → `m:nary` with empty `<m:e/>` and the body outside, `aligned` → matrix, parse errors on math-core's `<mspace/>` ("InvalidAttribute width=0") and `&ApplyFunction;` | too new (2 weeks, 50 downloads); watch |
| ooxml-omml 0.1.1-alpha.2 | `parse_math_zone(&[u8]) -> MathZone`, typed structs (Fraction, Radical, Nary, Delimiter, Matrix, Function, Accent, Bar, BorderBox, EquationArray, GroupChar…) | reader/types for OMML, no MathML/LaTeX input | could lend its type names |
| docmux-transform-math 0.6.2 | `latex_to_omml::latex_to_omml_display(&str) -> String` | not run (its LaTeX→typst was a token rewriter, R2 §5.3) | skip |
| knit-md-docx-rs 0.4.20 | `OMath::from_elements(Vec<OMathElement>)` — Run/Sup/Sub/SubSup/Fraction/Radical/Nary/Delimited/Function only (R3) | | insufficient |
| typort-math 0.2.0 | `equation_to_omml(content: &typst::foundations::Content) -> String` from an evaluated typst `EquationElem` (R2 §5.5) | not run | option for the typst backend |
| Microsoft `MML2OMML.XSL` (ships with Office) | XSLT | not usable from Rust without an XSLT engine | reference only **[unverified]** |

Conclusion: **no adequate MathML → OMML (or LaTeX → OMML) crate exists; write writer's own.**

### 6.2 texmath's OMML writer — what to copy **[verified: `Text/TeXMath/Writers/OMML.hs` read in full, 377 lines, GPL-2+ → re-implement, do not copy code]**
* Entry: `writeOMML :: DisplayType -> [Exp] -> Element`; `DisplayBlock` → `m:oMathPara/
  m:oMathParaPr/m:jc@center` + `m:oMath`; `DisplayInline` → bare `m:oMath`. Pipeline:
  `handleScaledDelims` (a pair of `\bigl( … \bigr)` becomes `EDelimited` → `m:d`, since "OMML cannot
  represent a numeric scale factor for delimiters") → `handleDownupDelims`/`handleDownup`
  (rewrites) → `showExp`.
* **N-ary detection** (`isNary`): `ESymbol Op` with char in ∫ ∬ ∭ ∮ ∯ ∰ ∏ ∐ ∑ (U+222B–2230,
  U+220F, U+2210, U+2211). `handleDownup`: when the base of `EOver/EUnder/EUnderover/ESub/ESuper/
  ESubsup` is n-ary, it is grouped with **exactly the next sibling expression** as the body:
  `EGrouped [EUnderover x y z, next]` → `makeNary "undOvr"`, `EGrouped [ESubsup x y z, next]` →
  `makeNary "subSup"`; missing limits become `EGrouped []` → `subHide/supHide = "on"`. Non-n-ary
  convertible limits (`movablelimits`) in **inline** display become `ESub/ESuper/ESubsup`.
* `makeNary`: `<m:nary><m:naryPr><m:chr m:val="∑"/><m:limLoc m:val="undOvr|subSup"/><m:subHide
  m:val="on|off"/><m:supHide m:val="on|off"/></m:naryPr><m:sub>…</m:sub><m:sup>…</m:sup><m:e>…</m:e></m:nary>`.
* Runs: `str props s = <m:r>[<m:rPr>props</m:rPr>]<m:t>s</m:t></m:r>`. `ENumber` → plain run;
  `EIdentifier`: uppercase Greek → `sty p`; identifier under `sty b` → `sty bi` (#280); otherwise
  plain (Word italicises letters by default). `EMathOperator` (lim, sin, argmax…) → `sty p` run.
  `ESymbol` single symbol/punctuation char → `sty p` (TextNormal) run; multi-char `Op/Bin/Rel`
  → `<m:box><m:boxPr><m:opEmu m:val="on"/></m:boxPr><m:e>run</m:e></m:box>`.
  Empty group / empty identifier → run with U+200B "to avoid the dashed box".
* `setProps`: TextNormal → `sty p`; Bold → `sty b`; Italic → `sty i`; BoldItalic → `sty bi`;
  Monospace → `scr monospace`+`sty p`; SansSerif → `scr sans-serif`+`sty p`; DoubleStruck → `scr
  double-struck`+`sty p`; Script → `scr script`+`sty p`; Fraktur → `scr fraktur`+`sty p`; bold
  variants → same `scr` + `sty b`/`bi`. `EText TextNormal s` → `<m:rPr><m:nor/></m:rPr>` run;
  other text types → `setProps`. `EStyled a es` → children rendered with `setProps a`.
* `ESpace n` (em): ≤0.17 → U+2009 thin; ≤0.23 → U+2005; ≤0.5 → U+2004; ≤1.8 → U+2001; >1.8 →
  U+2001 U+2001; else U+200B.
* Fractions: `<m:f><m:fPr><m:type m:val="bar"/></m:fPr><m:num/><m:den/></m:f>`; `InlineFrac` →
  `lin`; `NoLineFrac` (binom) → `noBar`; `DisplayFrac` = bar.
* `ESqrt` → `<m:rad><m:radPr><m:degHide m:val="on"/></m:radPr><m:deg/><m:e/></m:rad>`; `ERoot i x`
  → `<m:rad><m:deg>i</m:deg><m:e>x</m:e></m:rad>`.
* `ESub/ESuper/ESubsup` → `m:sSub/m:sSup/m:sSubSup` (`m:e`, `m:sub`, `m:sup`).
* `EUnder/EOver`: bar chars (`isBarChar`: U+203E, U+00AF, U+0304, U+0333, `_`) → `<m:bar><m:barPr>
  <m:pos m:val="bot|top"/></m:barPr><m:e/></m:bar>`; `EOver _ x (ESymbol Accent y)` → `<m:acc>
  <m:accPr><m:chr m:val="y"/></m:accPr><m:e/></m:acc>`; `ESymbol TUnder/TOver` (⏟ ⏞ etc.) →
  `<m:groupChr><m:groupChrPr><m:chr/><m:pos m:val="bot|top"/><m:vertJc m:val="top|bot"/>
  </m:groupChrPr><m:e/></m:groupChr>`; otherwise `m:limLow`/`m:limUpp` (`m:e`, `m:lim`);
  `EUnderover` → `limLow(limUpp(x, over), under)`.
* `EDelimited start end xs` → `<m:d><m:dPr><m:begChr m:val="("/><m:sepChr m:val="|"/><m:endChr
  m:val=")"/><m:grow/></m:dPr><m:e>…</m:e>[<m:e>…</m:e>]</m:d>` (one `m:e` per `\middle`-separated
  part; empty `begChr`/`endChr` for `\left.`).
* `EArray`: if the column alignments form an R/L sequence (`isRLSequence`: aligned/align) →
  `<m:eqArr><m:e>row with literal "&" runs at alignment points</m:e>…</m:eqArr>`; else
  `<m:m><m:mPr><m:baseJc m:val="center"/><m:plcHide m:val="on"/><m:mcs><m:mc><m:mcPr><m:count
  m:val="1"/><m:mcJc m:val="left|center|right"/></m:mcPr></m:mc>…</m:mcs></m:mPr><m:mr><m:e/>…
  </m:mr>…</m:m>` (one `m:mc` per column, padded with centre).
* `EPhantom` → `<m:phant><m:phantPr><m:show m:val="off"/></m:phantPr><m:e/></m:phant>`; `EBoxed`
  → `<m:borderBox><m:e/></m:borderBox>`; `ECancel` → `m:borderBox` with `hideTop/Bot/Left/Right =
  1` + `strikeBLTR`/`strikeTLBR`; `EScaled _ x` → x (scale dropped).
* Colour is **not** handled by texmath's OMML writer at all.

### 6.3 MathML-Core element → OMML mapping (for writer's `mathml_to_omml.rs`)
Input: MathML Core as produced by math-core / pulldown-latex / typst-html (§7). Normalise first:
decode `&ApplyFunction;`→U+2061, drop U+2061/U+2062 runs, unwrap single-child `mrow`, treat
`semantics` as its first child (keep `annotation application/x-tex` as alt text), treat
`mstyle`/`mpadded`/`maction` as `mrow` (carrying `mathcolor`/`style` colour).

| MathML | OMML | notes / quirks |
|---|---|---|
| `<math display="block">` | `<w:p><w:pPr/><m:oMathPara><m:oMathParaPr><m:jc m:val="center"/></m:oMathParaPr><m:oMath>…</m:oMath></m:oMathPara></w:p>` | inline `<math>` → `<m:oMath>` as a run-level sibling inside the paragraph (R3 §2.18). Namespace `m` declared on `w:document` (R3 §4) |
| `mrow` | children concatenated; empty → `<m:r><m:t>&#x200B;</m:t></m:r>` | if first child is `mo` fence/stretchy and last child is a matching closer → `m:d` (see below) |
| `mi` (1 letter, incl. 𝑥-style math-italic code points) | `<m:r><m:t>x</m:t></m:r>` | map Unicode math alphanumerics back to base letter + `sty`/`scr` (𝐱→`sty b` *bi for letters per texmath #280*, 𝜶→`sty bi`, ℝ/𝔸→`scr double-struck`+`sty p`, 𝒜/ℱ→`scr script`+`sty p`, 𝔉→`scr fraktur`+`sty p`, 𝖺→`scr sans-serif`, 𝚊→`scr monospace`) **or** keep the code point in `m:t` (tex2word/easydoc do; Cambria Math has the glyphs — rendering in Word not verified); uppercase Greek → `sty p`; `mathvariant="normal"` → `sty p` |
| `mi` (multi-letter: lim, sin, argmax, `\operatorname`) | `<m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>lim</m:t></m:r>` | pandoc emits no `m:func` |
| `mn` | `<m:r><m:t>12</m:t></m:r>` | |
| `mo` (one symbol/punct char) | `<m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>=</m:t></m:r>` | verified pandoc output for `= → ( ) ∞ −`; multi-char operator → `m:box opEmu` |
| `mo` U+2061 (⁡), U+2062 (⁢) | drop | |
| `mtext` | `<m:r><m:rPr><m:nor/></m:rPr><m:t xml:space="preserve">if </m:t></m:r>` | `ms` same with quotes |
| `mspace width="Xem"` | run with U+2009 / U+2005 / U+2004 / U+2001 / U+200B per texmath thresholds | `height`/`depth` ignored |
| `mfrac` | `<m:f><m:fPr><m:type m:val="bar"/></m:fPr><m:num>…</m:num><m:den>…</m:den></m:f>` | `linethickness="0"` → `type noBar`; `bevelled` is not in Core (`\sfrac`) → `lin` |
| `msqrt` | `<m:rad><m:radPr><m:degHide m:val="on"/></m:radPr><m:deg/><m:e>…</m:e></m:rad>` | |
| `mroot` (base, index) | `<m:rad><m:deg>index</m:deg><m:e>base</m:e></m:rad>` | child order differs |
| `msub`/`msup`/`msubsup` | `m:sSub`/`m:sSup`/`m:sSubSup` (`m:e`, `m:sub`, `m:sup`) | **n-ary base** (`mo` ∑∏∐∫∬∭∮∯∰ and `largeop`) → `m:nary` with `limLoc subSup`, hides by emptiness, body = next sibling element (one element; wrap following run until the next relation? texmath: exactly one) |
| `munder`/`mover`/`munderover` | `m:limLow`/`m:limUpp` (nested for both) | n-ary base → `m:nary limLoc="undOvr"`; `mover accent="true"` → `<m:acc><m:accPr><m:chr m:val="̂"/></m:accPr><m:e/></m:acc>` with the **combining** form (map spacing ^ ˆ ˙ ˜ ¯ ˘ ˇ ´ ` → U+0302 U+0307 U+0303 U+0304 U+0306 U+030C U+0301 U+0300; `\vec` → U+20D7; `\ddot` → U+0308); bar/overline/underline chars → `m:bar pos=top/bot`; ⏞ ⏟ ⎴ ⎵ → `m:groupChr`; in **inline** mode with `movablelimits` (lim, max, ∑ not in nary list) → `sSub/sSup` |
| `mrow` = `mo`(open, stretchy/fence) … `mo`(close) | `<m:d><m:dPr><m:begChr m:val="("/><m:sepChr m:val=""/><m:endChr m:val=")"/><m:grow/></m:dPr><m:e>…</m:e></m:d>` | `\middle` → several `m:e` + `sepChr`; `\left.` → `begChr ""`; non-stretchy `(`…`)` (KaTeX `stretchy="false"`) stay plain runs |
| `mtable` with `aligned`/`align` semantics (classes `menv-alignlike`, `aligned`, `multiline-equation`, or alternating right/left `mtd` styles) | `<m:eqArr><m:e>a<m:r><m:t>&amp;</m:t></m:r>=b+c</m:e><m:e>…</m:e></m:eqArr>` | `&` run marks the alignment point (texmath `isRLSequence`) |
| `mtable` (matrix, cases) | `<m:m><m:mPr><m:baseJc m:val="center"/><m:plcHide m:val="on"/><m:mcs>…</m:mcs></m:mPr><m:mr><m:e>…</m:e></m:mr>…</m:m>` | alignment from `mtd style="text-align"`/classes; `cases` → `m:d` with `begChr "{"`, `endChr ""` around `m:eqArr` (pandoc) or `m:m` |
| `mtr`/`mtd` | `m:mr`/`m:e` | |
| `mphantom` | `<m:phant><m:phantPr><m:show m:val="off"/></m:phantPr><m:e/></m:phant>` | |
| `mstyle`/`mrow style="color:…"`, `mathcolor` | `<m:r><w:rPr><w:color w:val="FF0000"/></w:rPr><m:t>x</m:t></m:r>` on every run inside | `w:rPr` inside `m:r` is legal OOXML (CT_R has `w:rPr`) **[unverified: schema recalled]**; texmath drops colour |
| `menclose` (not Core) | `m:borderBox` (+ `strikeBLTR/TLBR` for cancel) | only if a converter emits it |
| `mmultiscripts` + `mprescripts` | `<m:sPre><m:sub/><m:sup/><m:e/></m:sPre>` | rare |
| `merror` | red `w:color` run with the text | |
| `mpadded`, `mglyph`, `maction`, `annotation-xml` | unwrap / drop | |
| `semantics` + `annotation encoding="application/x-tex"` | use the TeX source for the `w:altText`/comment, else drop | |

### 6.4 Effort and architecture
* LOC estimate (Rust, incl. unit + snapshot tests): MathML parser into a small enum tree
  (`roxmltree` or `quick-xml`, ~150), normaliser (fence pairing, n-ary grouping, unicode
  alphanumerics → base + style table ~120 lines of ranges, accent table) ~250, emitter with
  `quick-xml` writer ~450, tests against pandoc's OMML for the same inputs (`pandoc -t docx`,
  unzip, compare normalised) ~250 → **≈ 1,000–1,200 LOC**. (texmath does it in 377 lines of
  Haskell from a richer AST; tex2word-math's whole LaTeX→OMML is 1,364 LOC.)
* Alternative A: LaTeX → OMML straight from pulldown-latex's `Event` stream (~900 LOC) — avoids
  MathML parsing but does not serve typst-sourced math; B: own `MathIr` (R2 §5.6 suggests the
  typst-syntax walker anyway) with emitters for MathML/OMML/LaTeX/typst — largest but cleanest.
  **Recommendation for v1: MathML as the hub** (LaTeX via math-core, typst via typst-html §7),
  one `MathML → OMML` emitter, and the typst image fallback for anything the emitter rejects.
* Validate with the R3 §5 tooling (docx validation) and pandoc's output as oracle (R3 §4.1 has
  verified samples: `E = mc^2`, `\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}`).

---

## 7. typst math → MathML via typst 0.15.1 HTML export **[verified: `typst-eq … html`]**

Mechanics (R1 §6): `Library::builder().with_features([Feature::Html].into_iter().collect())`,
`typst::compile::<HtmlDocument>(&world)`, `typst_html::html(&doc, &HtmlOptions { pretty: false })
-> SourceResult<String>`; equations go through `typst-html/src/mathml.rs` (1,346 lines;
`convert_math_to_nodes` is `pub(crate)`, so the only public route is the full HTML string →
extract `<math …>…</math>` with a simple scan; unsupported items warn "`{name}` was ignored during
MathML export" (`mathml.rs:1344`). All 16 probe equations exported with **zero warnings**, 62 µs
each (warm). Inline `$eq$` → `<math>…</math>` inside `<p>`; display `$ eq $` → `<math
display="block">`. Output samples (exact):

| typst | MathML (display) |
|---|---|
| `frac(a, b)` | `<math display="block"><mfrac><mi>𝑎</mi><mi>𝑏</mi></mfrac></math>` |
| `sum_(i=0)^n i` | `<munderover><mo lspace="0em">∑</mo><mrow><mi>𝑖</mi><mo lspace="0em" rspace="0em">=</mo><mn>0</mn></mrow><mi>𝑛</mi></munderover><mi>𝑖</mi>` (inline: `msubsup`) |
| `integral_0^1 x dif x` | `<msubsup><mo lspace="0em">∫</mo><mn>0</mn><mn>1</mn></msubsup><mi>𝑥</mi><mspace width="0.1667em"/><mo form="prefix" lspace="0em" rspace="0em">d</mo><mi>𝑥</mi>` |
| `root(n, x)` | `<mroot><mi>𝑥</mi><mi>𝑛</mi></mroot>` |
| `RR, bb(R), cal(F)` | `<mi>ℝ</mi><mo>,</mo><mi>ℝ</mi><mo>,</mo><mi>ℱ︀</mi>` (U+2131 U+FE00) |
| `upright(d) x` | `<mi mathvariant="normal">d</mi><mi>𝑥</mi>` |
| `lr((a/b))` | `<mrow><mo>(</mo><mfrac><mi>𝑎</mi><mi>𝑏</mi></mfrac><mo>)</mo></mrow>` |
| `mat(a, b; c, d)` | `<mrow><mo>(</mo><mtable><mtr><mtd><mi>𝑎</mi></mtd><mtd><mi>𝑏</mi></mtd></mtr><mtr>…</mtr></mtable><mo>)</mo></mrow>` |
| `a &= b + c \ &= d` | `<mtable class="multiline-equation aligned"><mtr><mtd><mi>𝑎</mi></mtd><mtd><mo form="infix">=</mo><mi>𝑏</mi><mo>+</mo><mi>𝑐</mi></mtd></mtr><mtr><mtd></mtd><mtd><mo form="infix">=</mo><mi>𝑑</mi></mtd></mtr></mtable>` |
| `"if " x > 0` | `<mtext>if </mtext><mspace width="0.2222em"/><mi>𝑥</mi><mo>></mo><mn>0</mn>` |
| `hat(x) arrow(v) dot(y)` | `<mover accent="true"><mi>𝑥</mi><mo>̂</mo></mover><mover accent="true"><mi>𝑣</mi><mo stretchy="true">⃗</mo></mover><mover accent="true"><mi>𝑦</mi><mo>̇</mo></mover>` (combining marks) |
| `op("argmax", limits: #true)_x f(x)` | `<munder><mo lspace="0em" rspace="0.16666666666666666em">argmax</mo><mi>𝑥</mi></munder><mi>𝑓</mi><mrow><mo>(</mo><mi>𝑥</mi><mo>)</mo></mrow>` |
| `bold(alpha) bold(x)` | `<mi>𝜶</mi><mi>𝒙</mi>` |
| `lim_(x -> oo) f(x)` | `<munder><mo lspace="0em" rspace="0.1667em">lim</mo><mrow><mi>𝑥</mi><mo lspace="0em" rspace="0em" stretchy="false">→</mo><mi>∞</mi></mrow></munder>…` |
| `cases(1 "if" x > 0, 0 "otherwise")` | `<mrow><mo>{</mo><mtable class="cases"><mtr><mtd><mn>1</mn><mspace width="0.2222em"/><mtext>if</mtext>…</mtd></mtr>…</mtable></mrow>` |
| `x^2 + y_1 + a'` | `<msup><mi>𝑥</mi><mn>2</mn></msup><mo>+</mo><msub><mi>𝑦</mi><mn>1</mn></msub><mo>+</mo><msup><mi>𝑎</mi><mo>′</mo></msup>` |

Quirks to handle: (1) identifiers are Unicode math-italic code points (U+1D44E…) — fine for
rendering, but copy/paste yields 𝑥 and the OMML converter must fold them back (§6.3); (2) alignment
classes `multiline-equation`, `aligned`, `cases`, `left-align`, `right-align`, `flushed`… rely on
typst's `EQUATION_CSS` (1,501 B, injected into `<head>` only when the document has equations;
copied in full to `$SB/math-probe/typst-eq/out/probe.txt`) — writer must ship it; (3) `lim`/`op`
are `<mo>` with explicit `lspace/rspace`; (4) `dif` → `<mo form="prefix">d</mo>`; (5) the HTML
compile needs `Feature::Html` in the Library (a second `Library` instance alongside the paged one).

---

## 8. Recommended conversion matrix for v1

| source \ target | **HTML** | **DOCX** | **PDF (typst)** | **LaTeX (BYO template)** |
|---|---|---|---|---|
| **LaTeX math** (Markdown `$…$`, `$$…$$`, ```` ```math ````, ipynb `text/latex`) | **Primary:** `katex-rs` 0.3.0 server-side `HtmlAndMathml` + vendored KaTeX 0.19.1 css/woff2 (§4.4). Fallbacks: `math.engine="katex-js"` (client KaTeX), `"mathml"` (math-core 0.9.0 + font CSS), `"image"` (typst SVG via tylax `latex_to_typst`, §5) | **Primary:** math-core MathML → own MathML→OMML (§6.3). **Fallback** (converter error / unsupported node / `math.docx="image"`): LaTeX → typst (tylax `latex_to_typst`, R2 §5.4) → SVG+PNG picture with baseline (§5) | **Primary:** LaTeX → typst text (tylax `latex_to_typst`, or mitex `convert_math` + the mitex typst scope bundled as a `@writer/…` package, R2 §5.4) compiled natively. Fallback: raw TeX in a `raw` block + warning | **Pass-through verbatim**: inline `$…$`, display `\[ … \]`, keep `\begin{aligned}` etc. untouched (zero risk); `\label`/`\tag` kept |
| **typst math** (heavy backend) | **Primary:** typst-html MathML (§7; `<math>` extracted from a one-equation `HtmlDocument`; ship `EQUATION_CSS` + `math { font-family }` rule + optional embedded LM Math). Alt: typst → LaTeX (R2 §5.6 converter) → katex-rs for KaTeX-quality rendering; `"image"` via `typst_svg` (native) | **Primary:** typst-html MathML → same MathML→OMML. Fallback: native SVG+PNG picture (§5). Later: typort-math (typst `Content` → OMML) | **native** | **Own typst-syntax → LaTeX converter** (R2 §5.6, 500–800 LOC; tylax/pandoc as test oracles) |

Defer to v2: `\color` inside OMML; equation numbering/`\tag`/`\label` cross-refs in DOCX
(`m:oMathPara` has no number — pandoc puts numbers in a table); `mhchem`/`\ce`; MathML →
LaTeX (never needed); OMML reading (`ooxml-omml`); KaTeX font subsetting; Firefox/Safari
MathML screenshots; `katex-rs` upstream lag monitoring (0.18.5 vs npm 0.19.1); Temml.

---

## 9. Verified API hints (copy-paste)

```rust
// ---- pulldown-latex 0.8.0 -------------------------------------------------------- [verified]
use pulldown_latex::{config::{DisplayMode, RenderConfig}, push_mathml, Parser, Storage};
fn latex_to_mathml(src: &str, display: bool) -> String {
    let storage = Storage::new();                       // reset() after each equation if reused
    let parser = Parser::new(src, &storage);            // Iterator<Item = Result<Event, ParserError>>
    let cfg = RenderConfig {
        display_mode: if display { DisplayMode::Block } else { DisplayMode::Inline },
        annotation: Some(src),                          // <semantics><annotation encoding="application/x-tex">
        error_color: (178, 34, 34), xml: false, math_style: Default::default(),
    };
    let mut out = String::new();
    push_mathml(&mut out, parser, cfg).expect("fmt");   // Err events become inline <merror>; no hard failure
    out
}
// Hard-failure variant: collect first — `let events: Result<Vec<_>, ParserError> = Parser::new(..).collect();`

// ---- math-core 0.9.0 ------------------------------------------------------------- [verified]
use math_core::{LatexToMathML, MathCoreConfig, MathDisplay};
let conv = LatexToMathML::new(
    MathCoreConfig { ignore_unknown_commands: true, annotation: true, ..Default::default() },
    vec![("\\RR".to_string(), "\\mathbb{R}".to_string())],   // front-matter macros
).map_err(|e| /* MacroParseError */ e)?;
match conv.convert_with_local_state(r"\frac{a}{b}", MathDisplay::Block) {
    Ok(r) => { let mathml: String = r.mathml; /* r.warnings */ }
    Err(e) => { let msg = e.error_message(); let byte_range = e.0.clone(); }   // map to source span
}
// For XML consumers: mathml.replace("&ApplyFunction;", "&#x2061;")

// ---- katex-rs 0.3.0 (lib name `katex`) ------------------------------------------- [verified]
use katex::{render_to_string, KatexContext, OutputFormat, Settings};
let ctx = KatexContext::default();                       // reuse; 0.7 ms to build
let settings = Settings::builder().display_mode(true)
    .output(OutputFormat::HtmlAndMathml).throw_on_error(false).build();
let html: String = render_to_string(&ctx, r"\sum_{i=0}^{n} i", &settings)?;   // Err(ParseError) only with throw_on_error(true)
// Output starts with <span class="katex-display"><span class="katex"><span class="katex-mathml">…<span class="katex-html" aria-hidden="true">…
// Needs KaTeX >= 0.18.5 katex.min.css (+ 20 woff2); 0.16.x CSS does not match (class names katex-base/katex-strut).

// ---- katex 0.4.6 (QuickJS, KaTeX 0.16.4) — not recommended ----------------------- [verified]
let opts = katex::Opts::builder().display_mode(true).output_type(katex::OutputType::Mathml).build().unwrap();
let s = katex::render_with_opts(r"\int_0^1 x\,dx", &opts)?;   // 37 ms first call, 0.48 ms after

// ---- typst 0.15.1: equation → SVG/PNG + baseline (World from R1 §5.2) ------------ [verified]
use typst::layout::{Abs, Frame, FrameItem, Point};
use typst_layout::PagedDocument;
const PRELUDE: &str = "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n#set text(top-edge: \"bounds\", bottom-edge: \"bounds\")\n";
world.set_text(&format!("{PRELUDE}${eq}$"));              // inline; display: "$ {eq} $"
let typst::diag::Warned { output, warnings } = typst::compile::<PagedDocument>(&world);
let doc = output.map_err(|errs| /* Vec<SourceDiagnostic> */ errs)?;
let page = &doc.pages()[0];                               // typst_layout::Page { frame: Frame, bleed, fill: Smart<Option<Paint>>, numbering, supplement, number }
let (w_pt, h_pt) = (page.frame.width().to_pt(), page.frame.height().to_pt());
let baseline_pt = page.frame.items().find_map(|(pos, item)| match item {
    FrameItem::Group(g) if g.frame.has_baseline() => Some((pos.y + g.frame.baseline()).to_pt()),
    _ => None,
}).unwrap_or(h_pt);                                       // Frame::baseline(): "measured from the top"; descent = h_pt - baseline_pt
let svg: String = typst_svg::svg(page, &typst_svg::SvgOptions { render_bleed: false, pretty: false });
let png: Vec<u8> = typst_render::render(page, &typst_render::RenderOptions { pixel_per_pt: 2.0.into(), render_bleed: false })
    .encode_png().expect("png");                          // tiny_skia::Pixmap
comemo::evict(10);
// Optional resvg 0.48.1 route (resvg re-exports usvg + tiny_skia):
let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())?;
let size = tree.size();                                   // CSS px = pt * 96/72
let scale = 2.0f32;                                       // px per CSS px; for N px/pt use N * 0.75
let mut pm = resvg::tiny_skia::Pixmap::new((size.width()*scale).ceil() as u32, (size.height()*scale).ceil() as u32).unwrap();
resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
let png2 = pm.encode_png()?;

// ---- typst 0.15.1: equation → MathML ---------------------------------------------- [verified]
use typst::{Feature, Features, Library, LibraryExt};
let library = Library::builder().with_features([Feature::Html].into_iter().collect::<Features>()).build();  // separate World for HTML
world.set_text(&format!("$ {eq} $"));
let typst::diag::Warned { output, warnings } = typst::compile::<typst_html::HtmlDocument>(&world);
let html = typst_html::html(&output?, &typst_html::HtmlOptions { pretty: false })?;   // SourceResult<String>
let start = html.find("<math").unwrap(); let end = html[start..].find("</math>").unwrap() + start + 7;
let mathml = &html[start..end];                           // plus typst's EQUATION_CSS (<style> in <head>) once per document
// warnings: ignore the detached "html export is under active development" one; keep "… was ignored during MathML export".
```
HTML stylesheet snippet for MathML output:
```css
math { font-family: "Latin Modern Math", "STIX Two Math", "Cambria Math", "Libertinus Math", math; }
/* optional (math.font = "embed"): */
@font-face { font-family: "Latin Modern Math"; src: url("data:font/woff2;base64,…") format("woff2"); } /* 388,212 B raw, GUST FL */
```

---

## 10. Open questions
1. **Word baseline alignment of inline equation pictures**: does `w:rPr/w:position` (half-points)
   shift an inline `w:drawing`? Alternative: anchored `wp:anchor` with `wp:positionV
   relativeFrom="line"`. Needs a Word test (Word is installed; drive manually or via AppleScript).
2. **Does Word render Unicode math alphanumerics (𝐱, ℝ, 𝒜) inside `m:t` as expected, or must the
   OMML emitter fold them to ASCII + `m:sty`/`m:scr`?** (pandoc folds; tex2word keeps ℝ.) Test in
   Word.
3. **`w:rPr/w:color` inside `m:r`** for `\color` — legal per schema (recalled), rendering unverified.
4. **math-core vs pulldown-latex**: pick after a larger corpus test (e.g. KaTeX's test suite
   strings, `~/.cargo/registry/src/*/katex-rs-0.3.0/tests/`) — both failed nothing here, but
   pulldown-latex's accent markup lacks `accent="true"` and its `\newcommand` without `[n]` bug
   should be reported upstream (Carlosted/pulldown-latex).
5. **katex-rs fidelity**: run KaTeX's screenshotter corpus (`tests/katex_spec.rs` exists in the
   crate) and compare a few hundred equations against client-side KaTeX 0.19.1 before making it the
   default; keep `"katex-js"` as the escape hatch.
6. **Chrome `font-family: math` resolution**: confirm with the real Google Chrome 155 (headless
   screenshot failed in this sandbox) and on Windows (Cambria Math expected to work) — decide
   whether `"mathml"` mode embeds Latin Modern Math by default (+388 KB) or only sets the
   font-family list.
7. **mitex scope bundling**: if the PDF path uses `mitex::convert_math` output, the typst-side
   `mitex-scope` (pure typst, from the `@preview/mitex` package `specs/`) must be vendored as an
   in-memory package (R1 §7 mechanism); licence Apache-2.0. tylax's `latex_to_typst` avoids this
   but loses `\text` spacing and emits `*b o l d*` for `\textbf` (R2 §5.4).
8. **Equation numbering** (`\tag`, `\label` + `@eq`) across HTML (KaTeX `\tag` support / math-core
   `TaggingStrategy`), DOCX (table-based numbering like pandoc) and LaTeX (`equation` env) — not
   researched here; see R5 for cross-refs.
9. **GUST Font License** compatibility with embedding the unmodified `latinmodern-math.woff2` in
   generated HTML: the CSS in fred-wang/MathFonts says it is "legally equivalent to the LPPL";
   redistribution of unmodified files is permitted under LPPL, but confirm before shipping.
