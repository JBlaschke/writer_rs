# T1A — Math in HTML (KaTeX, MathML, image routes)

| | |
|---|---|
| Milestone | M1 |
| Size | M |
| Depends on | T03, T15, T16, T30 |
| Parallel-safe with | T19, T1B, T20, T21, T25, T32, T41, T75, T77 (wave 5) |
| Files you own | `src/math/latex_mathml.rs` (replace stub), `src/html/math.rs` (replace stub), `assets/writer/math.js` (replace T00's placeholder; lift from presenter_rs/assets/presenter/math.js); `tests/fixtures/math/latex/*.tex` (new: the 37 files `01-frac.tex` … `37-sideset.tex` of step 2), `tests/fixtures/math/macros.toml` (new); `tests/latex_mathml.rs`, `tests/html_math.rs` (new); `tests/snapshots/latex_mathml__latex_corpus.snap`, `tests/snapshots/html_math__latex_corpus_katex.snap`, `tests/snapshots/html_math__math_md_{katex,mathml,image}.snap` (new); (additive: the body of `math_hook` in T16's `src/pipeline/html.rs`, and the T13/T16 snapshots this changes, html-output.md §1.8) |

## Goal
Every HTML equation goes through `html::math::MathRenderer` (T13's `MathHook`): LaTeX math as KaTeX spans rendered by
writer's `math.js` (default), as `math-core` MathML (`[math] html = "mathml"`) or as compiler SVG (`"image"`); typst
math as the compiler's MathML, falling back to an image per equation. `math::latex_mathml` is the one LaTeX→MathML
converter (T42, `check`); `tests/fixtures/math/latex/` is the corpus T33, T42, T60 read. T16's pipeline then uses you.

## Read first
- `docs/spec/math.md` **§3.1, §4.1–§4.5 — this is your spec**; §1–§2 (latex/typst→HTML rows, rules 1–4), §3.4–§3.6
  (T03/T30 APIs you call), §9 (`math-syntax`, `math-conversion-failed`, `math-tag-ignored`, `math-macro-invalid`), §10
  (`latex/`, "Documents"), §11, Fixtures rows 1, 5–7, Open point 3, Deviations 2–3, 9–10.
- `docs/spec/html-output.md` §1.1, §1.8, §1.14, §4.3 (T15's `math_init()`: what each `MathUsage` flag pulls in),
  §6.2–§6.3, §8, §9 row T1A, Deviations "Math hook", "`math.js` placeholder", "Math assets in standalone files".
- `docs/spec/live-reload.md` §1.2 item 5, §3.4 (step 5, guard 3), §5.2 (`prepare`, `WorldSlot`), §9 item 11 (T23).
- `docs/spec/typst-dialect.md` §6.1, §9; `docs/spec/config.md` §1 (tables merge key by key), §2 `[math]`, §4;
  `docs/spec/ir.md` §1 (`MathDialect`), §3 (`register_bytes`), §5; `docs/spec/vendored-assets.md` §2.1, §4, §7.3.
- `docs/research/R4-math.md` §2.2, §2.4, §3.2, §4.2, §5.2, §7, §9 (math.md §3.1 corrects §9's macro names);
  R8 §3.5 (`math.js` row), §10 step 25; `docs/tasks/T16-pipeline-build-cli.md` step 5 (`math_hook`).
- presenter_rs: `assets/presenter/math.js` (28 LOC, the only lifted file).

## API to implement
```rust
// src/math/latex_mathml.rs (math.md §3.1; math-core = "=0.9.0")
use std::collections::BTreeMap;
use math_core::{LatexToMathML, MathCoreConfig, MathDisplay, PrettyPrint};
pub fn to_mathml(source: &str, display: bool, macros: &BTreeMap<String, String>) -> Result<String, MathError>;
pub fn macro_diagnostics(macros: &BTreeMap<String, String>) -> Vec<Diagnostic>;
pub struct Converter { inner: LatexToMathML }
impl Converter {
    pub fn new(macros: &BTreeMap<String, String>) -> (Converter, Vec<Diagnostic>);
    /// `display` → `<math xmlns="http://www.w3.org/1998/Math/MathML" display="block">`.
    pub fn to_mathml(&self, source: &str, display: bool) -> Result<String, MathError>;
}
#[derive(Debug, thiserror::Error)]
pub enum MathError {
    #[error("{message}")] Syntax { message: String, range: Option<std::ops::Range<usize>>, html: String },
    #[error("macro {name}: {message}")] Macro { name: String, message: String },
    #[error("unsupported {element}: {detail}")] Unsupported { element: String, detail: String },  // raised by T42
}
// src/html/math.rs (html-output.md §1.8): the image and typst routes compile through the borrowed World
pub struct MathRenderer<'a> { /* private */ }
impl<'a> MathRenderer<'a> { pub fn new(config: &'a Config, world: &'a mut WriterWorld) -> MathRenderer<'a>; }   // + impl MathHook
```
Consumed: `html::emit::{MathHook, MathRequest, MathUsage, escape}` (T13), `math::latex_source::{prepare, for_typst,
macros_from_config}` (T30), `typst::render::equation`, `typst::html::equation_mathml` (T03). `assets/writer/math.js`
defines `window.__writerRenderMath(root)` and reads `window.__writerKatexOptions` (math.md §4.2).

## Steps
1. **Check inputs on `main`:** `grep -n "pub fn prepare\|pub fn for_typst\|pub fn macros_from_config"
   src/math/latex_source.rs`; `grep -n "pub fn equation" src/typst/{render,html}.rs`; `grep -n "MathHook\|MathRequest\|pub
   fn escape" src/html/emit.rs`; `grep -n "fn math_hook" src/pipeline/html.rs`; `grep -rn "struct WriterWorld\|fn
   with_span" src/`. T00 wrote the `mod` lines; never edit module lists, `Cargo.toml` or T30's `latex_source.rs`.
2. **Corpus** `tests/fixtures/math/latex/<name>.tex`: line 1 `% mode: inline` for `10`, `22`, `25`, `26`, `29`, `31`,
   `32`, else `% mode: display`; line 2 the source; one final newline (`30-empty`: line 1 only). Sources (math.md §10;
   unnamed ones from R4's probe, see Notes): `01-frac` `\frac{a}{b}` · `02-sum-limits` `\sum_{i=0}^{n} i` ·
   `03-integral-dx` `\int_0^1 x \, \mathrm{d}x` · `04-root-index` `\sqrt[n]{x}` · `05-mathbb` `\mathbb{R}` ·
   `06-left-right-frac` `\left(\frac{a}{b}\right)` · `07-pmatrix` `\begin{pmatrix} a & b \\ c & d \end{pmatrix}` ·
   `08-aligned` `\begin{aligned} a &= b + c \\ &= d \end{aligned}` · `09-text-if` `\text{if } x > 0` · `10-accents`
   `\hat{x} \vec{v} \dot{y}` · `11-operatorname-argmax` `\operatorname{argmax}_x f(x)` · `12-color` `\color{red}{x}
   \textcolor{blue}{y}` · `13-boldsymbol-mathbf` `\boldsymbol{\alpha} \mathbf{x}` · `14-lim` `\lim_{x\to\infty} f(x)` ·
   `15-def-macro` `\def\R{\mathbb{R}} \R` · `16-newcommand-no-arity` `\newcommand{\dd}{\mathrm{d}} \dd x` ·
   `17-newcommand-arity` `\newcommand{\half}[1]{\frac{#1}{2}} \half{x}` · `18-unknown-command` `\foo{x}` ·
   `19-unbalanced-brace` `\frac{a}{b` · `20-cases` `\begin{cases} 1 & \text{if } x>0 \\ 0 & \text{otherwise}
   \end{cases}` · `21-middle-delimiter` `\left(\frac{a}{b}\middle|\frac{c}{d}\right)` · `22-nary-inline`
   `\sum_{i=1}^n x_i` · `23-underbrace-overbrace` `\overbrace{a+b}^{n} + \underbrace{c+d}_{m}` · `24-bmatrix-vmatrix`
   `\begin{bmatrix} 1 & 2 \\ 3 & 4 \end{bmatrix} \begin{vmatrix} a \end{vmatrix}` · `25-primes` `f'(x) + x''` ·
   `26-spaces` `a\,b\;c\quad d\qquad e` · `27-tag-label` `x = 1 \tag{3} \label{eq:t}` · `28-align-env`
   `\begin{align} a &= b \\ c &= d \end{align}` · `29-unicode-source` `α ≤ β` · `30-empty` · `31-macro-from-config`
   `\RR^n + \half{x}` · `32-ce` `\ce{H2O}` · `33-sympy-displaystyle` `$\displaystyle \frac{1}{2}$` ·
   `34-empty-base-scripts` `{}_1^2 X` · `35-nested-fences-text` `\left\{ x \in \mathbb{R} \mid \text{if } x > 0
   \right\}` · `36-left-dot-right-bar` `\left. \frac{df}{dx} \right|_{x=0}` · `37-sideset` `\sideset{_1^2}{_3^4}\sum`.
   `macros.toml`: `[math]` + `katex_options = { macros = { "\\RR" = "\\mathbb{R}", "\\half" = "\\frac{#1}{2}" } }`.
   The test loader takes line 2 on without the final newline and strips one enclosing `$…$` pair (only `33`).
3. `Converter::new` (`src/math/latex_mathml.rs`): per `(key, replacement)` in key order, a key without a leading `\`,
   or whose rest fails math-core's name rule (one character, or ASCII letters only), raises `math-macro-invalid` and
   is skipped; the rest go to `LatexToMathML::new` as `(key minus the backslash, replacement)` with math.md §3.1's
   fixed `MathCoreConfig` (Hints). On `Err((err, index, _))` drop entry `index`, record `math-macro-invalid` with
   `err.error_message()`, retry (at most entries + 1 calls; an empty list cannot fail). Message:
   `[math] katex_options.macros["<key>"]: <message>`, warning, no span.
4. `Converter::to_mathml`: `self.inner.convert_with_local_state(source, MathDisplay::Block | Inline)`; `Ok(r)` →
   `r.mathml` with `&ApplyFunction;`→`&#x2061;`, `&InvisibleTimes;`→`&#x2062;`, `&InvisibleComma;`→`&#x2063;`,
   `&nbsp;`→`&#xA0;` and nothing else; `Err(e)` → `MathError::Syntax { message: e.error_message(), range:
   Some(e.0.clone()), html: e.to_html(source, MathDisplay::Inline, Some("math-error")) }`.
5. Cache: `static CACHE: std::sync::Mutex<Option<(BTreeMap<String, String>, Arc<Converter>, Vec<Diagnostic>)>>`;
   the free `to_mathml`/`macro_diagnostics` rebuild it when the map differs, clone the `Arc` (or the diagnostics) and
   drop the lock before converting; a poisoned lock is recovered with `PoisonError::into_inner`. Unit tests beside it.
6. `MathRenderer` (`src/html/math.rs`): fields `config`, `world`, `macros` (`macros_from_config(&config.math.
   katex_options)`, in `new`), `usage: MathUsage`, `images: usize`, `macro_diags_done: bool`. `equation` returns one
   `<span class="math inline|display" …>` (no id, no positions; values through `escape`); `data-tex` =
   `prepare(source).0` (latex) or `source` (typst). When `prepare`/`for_typst` report a removal: one warning
   `math-tag-ignored`, `\tag/\label inside math are ignored for html; label the equation with {#eq-x}`. Every
   diagnostic gets `eq.span` (cloned) when `Some`. Doc comments on public items; one helper per route (≤ ~80 lines).
7. Routes on `(eq.dialect, config.math.html)`:
   - latex + `Katex`: `<span class="math inline" data-math-style="inline" data-tex="T">T</span>` (`display` alike);
     `usage.katex`. No build-time conversion.
   - latex + `Mathml`: on first use extend `diagnostics` with `macro_diagnostics(&macros)` (once per document); `Ok` →
     the `<math>` is the content, `usage.mathml`; `Err(Syntax)` → content `html` + warning `math-syntax`: `invalid
     LaTeX math: <message> at bytes a..b of the equation`.
   - latex + `Image`: step 8 with `for_typst(source, &macros).0`, `MathDialect::Latex`.
   - typst + `Katex`/`Mathml`: `equation_mathml(world, source, display)` → content, `usage.typst_mathml`; `Err` →
     step 8 with `source`. typst + `Image`: step 8 with `source`.
8. Image form: `typst::render::equation(world, text, dialect, display)`; `Ok(r)` → `images += 1`, `id =
   assets.register_bytes(r.svg.clone(), "image/svg+xml", &format!("eq-{images}.svg"))`, content `<img
   src="asset:<id>" alt="<IR source>" style="height:<H>pt;vertical-align:-<D>pt">`, `H = height_pt × 12/11`, `D =
   (height_pt − baseline_pt) × 12/11`, at most two decimals with trailing zeros and dot trimmed; `usage.image`. `Err` →
   `<code class="math-error">SRC</code>`. One `math-conversion-failed` per equation (rule 4), `html: cannot convert
   equation (<reason>); used an image` (MathML failed) or `…; used the source text` (image failed; after a failed
   MathML, reason = `<mathml reason>; image: <image reason>`). Reason = `Error::Compile`'s `message`, else `Display`.
9. `assets/writer/math.js` ← presenter's: globals `__presenterRenderMath`/`__presenterKatexOptions` →
   `__writerRenderMath`/`__writerKatexOptions`, `[presenter] katex` → `[writer] katex`, header comment ("set by
   math_init()"); selector `'[data-math-style]:not([data-rendered])'`. Keep the `!window.katex` return, `el.textContent`,
   `displayMode: el.dataset.mathStyle === 'display'`, `Object.assign({ displayMode, throwOnError: false },
   window.__writerKatexOptions || {})`, try/catch + `console.warn`, `el.dataset.rendered = '1'`, the
   `DOMContentLoaded`-or-now start. Open point 3: synchronous (presenter's); say so in the PR. Under 2 KB.
10. `src/pipeline/html.rs` (T16's, additive): `math_hook`'s body becomes `Box::new(crate::html::math::MathRenderer::
    new(config, world))`; delete its `let _ = &world;`. Touch nothing else.
11. Tests (criteria below). A bare World: `WriterWorld::from_config(&config, tmp, MainFile::Virtual,
    Datetime::from_ymd(2026, 10, 25).unwrap())`; `md/math.md`: `pipeline::prepare(&BuildOptions::new(path))`,
    `WorldSlot::new().get(Some(&prepared), &opts)`, a clone of `prepared.config` with `math.html` per route, then
    `html::emit::emit` (positions off). Snapshot = `Emitted.main` + `--- diagnostics ---` + `level code: message` lines,
    `asset:[0-9a-f]{12}` replaced by `asset:[id]` (`regex`).
12. `cargo test`: a T13/T16 snapshot that changes only because the pipeline renders math through you (prepared
    `data-tex` of `\tag`/`\label`/`align` sources) is re-blessed with `cargo insta review` and named in the PR. Manual:
    `cargo run -- build tests/fixtures/md/math.md --out <tmp>`, open it in a browser (typeset, no console errors).

## Acceptance criteria
- `tests/latex_mathml.rs` (fixtures `math/latex/*.tex`, `math/macros.toml`, T1A): `latex_corpus` writes
  `tests/snapshots/latex_mathml__latex_corpus.snap`, one line per fixture in name order, `<name> [<mode>]: <mathml>` or
  `… ERR <message> at bytes a..b`, converting `prepare(src).0` with `macros.toml`'s map for `31` only; exactly `18`,
  `19`, `32` are `ERR`. `display_and_inline_forms` and `syntax_error_shape` (`\foo{x}`) equal the probe strings of
  Hints; `no_html_only_entities` (none of the four named entities anywhere; `11` holds `&#x2061;`);
  `macro_names_and_retry` (`\RR`, `\half`, `\bad` = `\nosuch{#1}`, `nobackslash` = `x`, `\R R` = `x` → three
  `math-macro-invalid` naming the last three; `\RR^n + \half{x}` still gives `<mi>ℝ</mi>` and
  `<mfrac><mi>x</mi><mn>2</mn></mfrac>`); `cache_follows_the_macro_map` (maps A, B, A: `\RR` = `\mathbb{R}` → `ℝ`,
  `\mathbf{R}` → `𝐑`); `converter_is_send_sync` (compile-time).
- `tests/html_math.rs` on `md/math.md` (T01): `math_md_katex`, `math_md_mathml`, `math_md_image` →
  `tests/snapshots/html_math__math_md_{katex,mathml,image}.snap`; each: six math spans, exactly one `math-tag-ignored`,
  `Emitted.math` with only `katex` / `mathml` / `image` set; katex spans have `data-math-style` and content ==
  `data-tex` without `\tag`; mathml has six `<math xmlns=`, no `data-math-style`, no `math-macro-invalid`; image has six
  `<img src="asset:` and six `image/svg+xml` registry entries.
- `tests/html_math.rs`: `latex_corpus_katex` → `tests/snapshots/html_math__latex_corpus_katex.snap`; `image_geometry`
  (typst inline `x^2 + 1/2`: height 14.45 ± 0.01 pt, vertical-align −4.14 ± 0.01 pt); `typst_math_is_mathml`
  (`frac(a, b)` display under `katex` and `mathml` → `<math display="block"><mfrac><mi>𝑎</mi><mi>𝑏</mi></mfrac></math>`,
  `data-tex="frac(a, b)"`, `typst_mathml`); `typst_mathml_failure_uses_image` (`overline(x)`: `<img`, one
  `math-conversion-failed` with `was ignored during MathML export` and `used an image`);
  `double_failure_one_diagnostic` (typst `foo(x)`: `<code class="math-error">`, one diagnostic, `used the source
  text`); `image_failure_uses_source` (latex `\foo{x}`, `image`); `mathml_syntax_error` (`\foo{x}`: `math-error` span,
  `invalid LaTeX math: Unknown command "\foo". at bytes 0..4 of the equation`); `macro_diagnostics_once` (bad macro,
  two equations → one `math-macro-invalid`); `math_js_contract` (< 2,048 B; has `__writerRenderMath`,
  `__writerKatexOptions`, `:not([data-rendered])`, `throwOnError: false`, `DOMContentLoaded`; no `presenter`).
- `tests/html_math.rs` `build_per_route` (assert_cmd; temp dir with a copy of `md/math.md` and a `writer.toml` per
  route): `writer build math.md` exits 0; `dist/math.html` — katex: `__writerRenderMath`,
  `window.__writerKatexOptions`, no `<math xmlns`; mathml: six `<math xmlns`, `data:font/woff2;base64,`, no
  `katex.min`; mathml + `mathml_font = "system"`: no `@font-face`; image: six `data:image/svg+xml;base64,`, no
  `writer-math`, no KaTeX.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
`latex_source` (T30); renditions, `equation_mathml`, `EQUATION_CSS` (T03); `math_init()`, the `writer-math` style,
font embedding (T15); `.equation`/`.math`/`.math-error` CSS (T19); the morphdom guard (T22); the browser test and
`serve math-image.md` (T23); OMML, DOCX math, `math/mathml/*`, `expectations.toml` (T42); PDF corpus and `check`
(T33, T71); `math/typst/*` (T51); `.typ` inputs (T52); LaTeX (T60). No `katex-rs` (math.md Deviations).

## Hints (verified)
- math-core 0.9.0 (`~/.cargo/registry/src/index.crates.io-*/math-core-0.9.0/`): `lib.rs:194` `MathCoreConfig`
  (`Debug, Default, Clone`); `:275 pub type MacroParseError = (Box<LatexError>, usize, String);`; `:310 pub fn
  new(config: MathCoreConfig, macros: Vec<(String, String)>) -> Result<Self, MacroParseError>`; `:374 pub fn
  convert_with_local_state(&self, latex: &str, display: MathDisplay) -> Result<ConvertResult, Box<LatexError>>`
  (`ConvertResult { mathml, warnings }`); `error.rs:15 pub struct LatexError(pub Range<usize>, …)`, `:371 to_html(&self,
  latex, display, css_class: Option<&str>) -> String`, `:390 error_message(&self) -> String`; `custom_cmds.rs:248` the
  `pub(crate)` name rule. Config (math.md §11): `MathCoreConfig { pretty_print: PrettyPrint::Never, xml_namespace:
  true, ignore_unknown_commands: false, annotation: false, ..Default::default() }`; names without `\` (R4 §9's `"\\RR"`
  fails: `Invalid macro name`).
- Probe (math-core 0.9.0, `scratchpad/t1a-probe`, 2026-10-25): `\frac{a}{b}` inline →
  `<math xmlns="http://www.w3.org/1998/Math/MathML"><mfrac><mi>a</mi><mi>b</mi></mfrac></math>`; display adds
  ` display="block"` after `xmlns`; `""` → `<math xmlns="http://www.w3.org/1998/Math/MathML"></math>`; `\foo{x}` →
  `Unknown command "\foo".`, `0..4`, `to_html` = `<span class="math-error" title="0: Unknown command
  &quot;\foo&quot;."><code>\foo{x}</code></span>`; `\frac{a}{b` → `10..10`; `$\displaystyle \frac{1}{2}$` → `Unexpected
  "$".` (fine without `$`); corpus: only `18`, `19`, `32` fail (`37` → `mmultiscripts`); `11` emits `&ApplyFunction;`;
  `17` fails when `half` is in the macro map (`This command is already defined.`); `new` with `[RR, bad =
  \nosuch{#1}, half]` → `Err((_, 1, "\\nosuch{#1}"))`, `Unknown command "\nosuch".`; `LatexToMathML: Send + Sync`.
- typst 0.15.1: `typst::foundations::Datetime::from_ymd(i32, u8, u8) -> Option<Datetime>` (typst-library
  `foundations/datetime.rs:156`). math.md §3.4: inline typst `x^2 + 1/2` → 31.317 × 13.242 pt, baseline 9.447 (H 14.446,
  D 4.140 at 12/11). R4 §7: display `frac(a, b)` → `<math display="block"><mfrac><mi>𝑎</mi><mi>𝑏</mi></mfrac></math>`;
  `overline(x)` warns (`Err`); `foo(x)` is "unknown variable: foo"; a latex `\foo{x}` rendition is `Error::Compile`.
- KaTeX 0.19.1 (R4 §4.2): `throwOnError: false` draws errors in `#cc0000`; ≈ 645 KB inlined only with `usage.katex`
  (T16 asserts `math.md` ≤ 700,000 B). insta 1.49.0: `insta::assert_snapshot!("latex_corpus", text)` in
  `tests/latex_mathml.rs` → `tests/snapshots/latex_mathml__latex_corpus.snap`; `cargo insta review`.
- Dev profile: `[profile.dev.package."*"] opt-level = 2` (T00) because unoptimised typst is 8–60× slower; keep
  image-route tests to a few equations and one World per test (the first mitex compile loads its WASM plugin).

## Notes for the coordinator
- Implementation starts after checked T30 in PR #13 (`1ea66e3`), with a PR
  targeting `main` and T30 listed as its unmerged dependency. CI proceeds
  independently. The existing `md/math.md` corpus contains 12 equations;
  tests retain all 12 rather than the original six assumed above.
- math.md §10 names but does not spell out `03`–`07`, `09`, `11`–`17`, `20`, `23`, `24`, nor any mode. Step 2 uses
  R4 §2.4's probe inputs (`scratchpad/build/math-probe/src/main.rs`) and the spec review's (`review-math-probe`,
  fx20–fx24); modes follow `mathspec-probe` and `md/math.md`. T33, T42, T60 read these files as written.
- `33-sympy-displaystyle` is spelled with `$…$`, which math-core rejects, yet `expectations.toml` gives it `omml`:
  the file keeps the `$` and T1A's loaders strip one enclosing pair; T33, T42 and T60 need the same rule.
- math.md §4.4 leaves open the precision of `H`/`D`, the `n` of `eq-{n}.svg` and the `alt` source; this task uses
  html-output.md §1.7's number rule, a 1-based count per renderer and the IR source. A typst equation whose MathML
  and image both fail raises one `math-conversion-failed` naming both reasons (§2 rule 4).
- html-output.md §1.8 says T1A updates T13/T16 snapshots, but those harnesses build `KatexOnly` themselves; only
  outputs through `src/pipeline/html.rs` (`cli_build` snapshots, `dump --context`) can change.

## Amendments

- Coordinator integration (2026-10-27): browser checks of the new routes
  revealed black SVG ink on dark backgrounds and MathML overhang creating
  vertical scrollbars. Add vertical equation padding, a scoped dark image
  filter and its print reset to T19's styles, document them in its README,
  and extend its existing math/print tests. No markup or token changes.
- Existing HTML helper tests now check the real math client globals and
  JavaScript MIME type rather than T00's placeholder comment. No existing
  snapshots need updates; five new T1A snapshots cover all three routes and
  the shared LaTeX corpus.
