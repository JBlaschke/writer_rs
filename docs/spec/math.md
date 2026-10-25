# Spec: Math

**Status:** contract. Implemented by T1A (`src/math/latex_mathml.rs`, `src/html/math.rs`,
`assets/writer/math.js`), T03 (the math rules of §3.4–3.5 in `src/typst/render.rs`,
`src/typst/html.rs`, whose signatures typst-dialect.md §9 owns), T30 (`src/math/latex_source.rs`,
`#mi`/`#mitex` emission), T33 (the `check` pre-flight, the PDF corpus), T42 (`src/math/omml.rs`,
`src/docx/math.rs`), T51 (`src/math/typst_to_latex.rs`, `assets/math/unicode-latex.toml`), T60
(LaTeX rules), T19 (the `.equation`, `.math`, `.math-error` rules of the default theme), T52 (typst
documents), T71 (lints). Consumers: every emitter, `html-output.md`, `docx-output.md`,
`latex-output.md`, `typst-dialect.md`, `notebook-mapping.md`, `references.md`, `vendored-assets.md`.

The IR keeps every equation as its source text plus a dialect (`Block::Math { source, dialect,
attrs, number }`, `Inline::Math { source, dialect }`, `MathDialect { Latex, Typst }`, ir.md §1).
Nothing is converted before emission. This document fixes, per source dialect and target, the
conversion route, its fallback, the configuration knob and the diagnostic, and the APIs of the
`crate::math` modules and the two compiler helpers that every emitter calls. The facts it rests
on are R4 (all sections), R2 §4–§7, R3 §4, R7 §7/§9 and R6; where a note's recommendation
differs from this spec (R4 §4.4 preferred server-side `katex-rs`), the spec wins.

## 1. Dialects and where they appear

| source | construct | IR | dialect | frontend rule (owning spec) |
|---|---|---|---|---|
| Markdown | `$…$`, `` $`…`$ `` | `Inline::Math` | `latex` | comrak `math_dollars`/`math_code`; `$$…$$` mid-paragraph is also an `Inline::Math` (comrak gives an inline node; R9 §10); markdown-dialect.md §6 |
| Markdown | `$$…$$` as a paragraph's sole child, ```` ```math ```` fence | `Block::Math` | `latex` | trailing `{#eq-x …}` text → `attrs` (brief: display-math labels); `source` = `literal.trim()` (all surrounding whitespace; markdown-dialect.md §4.5) |
| notebook | markdown cells | as Markdown | `latex` | notebook-mapping.md |
| notebook | `text/latex` output | `Block::Math` candidate | `latex` | `strip_math_delimiters` (one `$$…$$`, `\[…\]`, `\(…\)` or `$…$` pair, SymPy's `$\displaystyle …$` included); a payload that is not one formula → `Raw { format: "latex" }` (notebook-mapping.md §4) |
| typst | `$…$` / `$ … $` | `Inline::Math` / `Block::Math` | `typst` | `Equation::block()` decides; `source` = body text trimmed; a following `<label>` → `attrs.id` (typst-dialect.md §2) |
| typst | `#mi(..)` / `#mitex(..)` | `Inline::Math` / `Block::Math` | `latex` | allow-listed calls (typst-dialect.md §2.4); the argument is a raw literal (its text: lines joined by `\n`) or a string literal (its value); any other argument → `E-TYP-ARG-INVALID` (typst-dialect.md §2.8); `numbering: none` on `mitex` → class `unnumbered` (typst-dialect.md §2.4, references.md §2.4: the emitter writes ``#mitex(`…`, numbering: none)``); any other named argument → `W-TYP-ARG-IGNORED`; a leading copy of the configured macro lines is removed (§6 round trip) |

`Features.math` is true when any `Math` exists (ir.md §1); it only says "some math". The HTML
emitter decides what to include from the dialects it actually met (§4.1), so a typst document
never pays for KaTeX and a Markdown document never ships typst's equation CSS.

## 2. Conversion matrix

| source → target | primary route | fallback (per equation) | knob | diagnostics |
|---|---|---|---|---|
| latex → HTML | KaTeX client-side: `span.math[data-math-style]` holding the prepared source (rules 2–3) + vendored KaTeX 0.19.1 + `math.js` (§4.2) | KaTeX's own red rendering (`throwOnError: false`) | `[math] html = "katex"` (default), `katex_options` | `math-tag-ignored`; `math-syntax` only from `check` (§9) |
| latex → HTML | `mathml`: math-core 0.9.0 → `<math>` + font rule + optional embedded font (§4.3) | `LatexError::to_html` (`span.math-error` with the source) | `[math] html = "mathml"`, `mathml_font` | `math-tag-ignored`, `math-syntax` |
| latex → HTML | `image`: compiler rendition through `mitex` → `<img>` (§4.4) | `<code class="math-error">` with the source | `[math] html = "image"` | `math-tag-ignored`, `math-conversion-failed` |
| latex → DOCX | math-core MathML → `math::omml` (§5) | display: compiler rendition picture; inline: Unicode text run (`omml::fallback_text`); math-core failure: source text run | `[docx] math = "omml"` (default) / `"image"` | `math-tag-ignored`, `math-syntax`, `math-conversion-failed` |
| latex → PDF | `#mi(..)` / `#mitex(..)` from the vendored `@preview/mitex` package, converted by its WASM plugin at compile time (§6) | the emitter's mitex pre-flight fails → `$#raw("…")$` with the source; a plugin error the pre-flight missed is a compile error of the pdf target (other targets still build) | — | `math-tag-ignored`, `math-conversion-failed`, `Error::Compile` |
| latex → LaTeX | verbatim passthrough (§7) | none | `[latex] unicode` adds `\newunicodechar` preamble lines only (§7.1) | `math-tag-ignored` is never raised for LaTeX |
| typst → HTML | compiler MathML export of one equation (`typst::html::equation_mathml`) + `typst::html::EQUATION_CSS` + the font rule (§4.5) | `image` route (native `$…$` rendition) when `equation_mathml` fails, an ignored construct included (§3.5) | `[math] html = "image"` only; `katex`/`mathml` both mean MathML here | `math-conversion-failed` |
| typst → DOCX | compiler MathML → `math::omml` | as latex → DOCX (picture / text run) | `[docx] math` | `math-conversion-failed` |
| typst → PDF | native | — | — | compiler diagnostics |
| typst → LaTeX | `math::typst_to_latex` (R2 §4, `codex`, `assets/math/unicode-latex.toml`) (§7.2) | per equation, only for `#` code: `\text{[typst math: …]}` | — | `W-TYP-MATH-*`, `E-TYP-MATH-CODE`, `math-latex-package` (info) |
| any → image | `typst::render::equation` (§3.4): the single rasteriser for HTML `image`, DOCX pictures | — | — | `math-conversion-failed` |

Rules that apply to every route:

1. **Macros propagate.** `[math] katex_options.macros` (`{"\\RR": "\\mathbb{R}", "\\half": "\\frac{#1}{2}"}`;
   keys keep their backslash) reaches KaTeX verbatim (§4.2), math-core through
   `latex_mathml::to_mathml(.., &macros)` (names passed without the backslash, §3.1), every typst
   compile of LaTeX math — the emitted `#mi`/`#mitex` argument, the renderer (§3.4) and the
   `check` pre-flight — as `\newcommand{\RR}[n]{…}` lines prepended to the source (`n` = the
   highest `#k` in the replacement, omitted when 0), and the LaTeX preamble as
   `\providecommand{\RR}[n]{…}` (latex-output.md §preamble). `math::latex_source` (§3.6, T30) is
   the one implementation all of them use. (`\R`, `\N`, `\Z` are built into KaTeX, math-core and
   mitex alike, so examples and fixtures use other names.)
2. **`\label{…}`, `\tag{…}`, `\tag*{…}`, `\nonumber`, `\notag`** at brace depth 0 of a LaTeX
   source are stripped before every non-LaTeX conversion, KaTeX included
   (`latex_source::strip_tags`), with one `math-tag-ignored` warning per equation and target;
   writer numbers equations itself (§8). KaTeX 0.19.1 rejects `\label` ("Undefined control
   sequence") and draws `\tag{3}` as a visible "(3)"; math-core rejects both outside a numbered
   environment. The LaTeX target keeps them verbatim.
3. **Top-level display environments** are normalised before KaTeX, math-core, mitex and the
   renderer (`latex_source::normalise_env`): `align(*)` → `aligned`, `gather(*)` → `gathered`,
   `multline(*)` → `gathered`, `equation(*)` → its body, `eqnarray(*)` → `aligned`. Only the
   LaTeX target receives the source unchanged: KaTeX 0.19.1 has no `multline`/`eqnarray` ("No
   such environment") and numbers `equation`/`align`/`gather` itself (its own `.eqn-num`),
   math-core numbers `align` rows ("(1)", "(2)"), mitex rejects `multline`/`eqnarray` ("unknown
   environment"). Every non-LaTeX route converts `latex_source::prepare(source)` (= rule 2 then
   rule 3, §3.6).
4. Fallbacks are per equation, never per document, and each one raises exactly one diagnostic
   with the equation's span (`Block`/`Inline` position from the enclosing block) and the target
   name in the message. `--strict` turns `math-conversion-failed`, every `W-TYP-*` and
   `E-TYP-MATH-CODE` into errors (cli.md's list; `math-syntax` and `math-tag-ignored` stay
   warnings; the info note `math-latex-package`, §7.2, is never escalated).
5. Outputs are deterministic: the converters are pure; renditions use typst's default families
   (Libertinus Serif, New Computer Modern Math), which the World serves from the embedded fonts
   first (§3.4), so a byte-identical build never depends on `[pdf] system_fonts`.

## 3. Module API

### 3.1 `math::latex_mathml` (T1A; `math-core = "0.9.0"`)

```rust
use std::collections::BTreeMap;
use math_core::{LatexToMathML, MathCoreConfig, MathDisplay, PrettyPrint};

/// The cross-spec entry point (brief). `source` is already prepared (`latex_source::prepare`,
/// §3.6); `macros` is `latex_source::macros_from_config(..)` (keys with their backslash). Uses a
/// process-wide `Converter` cached per distinct macro map (rebuilt when the map changes).
pub fn to_mathml(source: &str, display: bool, macros: &BTreeMap<String, String>) -> Result<String, MathError>;
/// The `math-macro-invalid` warnings of that cached converter; called once per document by every
/// emitter that uses math-core (HTML `mathml` route, DOCX) and by `check`.
pub fn macro_diagnostics(macros: &BTreeMap<String, String>) -> Vec<Diagnostic>;

/// What the cache holds; public for callers that keep their own. `&self` methods, `Sync`.
pub struct Converter { inner: LatexToMathML }
impl Converter {
    pub fn new(macros: &BTreeMap<String, String>) -> (Converter, Vec<Diagnostic>);
    /// `display` → `<math xmlns="http://www.w3.org/1998/Math/MathML" display="block">`.
    pub fn to_mathml(&self, source: &str, display: bool) -> Result<String, MathError>;
}

#[derive(Debug, thiserror::Error)]
pub enum MathError {
    /// math-core could not parse the source. `range` is a byte range in `source` (from
    /// `LatexError.0`); diagnostics print it as `at bytes a..b of the equation` (§9). `html` is
    /// `LatexError::to_html(source, MathDisplay::Inline, Some("math-error"))`: always
    /// `<span class="math-error" title="…"><code>source</code></span>` (the `Block` form is a `<p>`,
    /// which may not sit inside `span.math`), ready for the HTML `mathml` route.
    #[error("{message}")] Syntax { message: String, range: Option<std::ops::Range<usize>>, html: String },
    #[error("macro {name}: {message}")] Macro { name: String, message: String },
    /// The OMML writer met something it does not map; `element` names it (`mmultiscripts`,
    /// `mlabeledtr`, `merror`, `entity &foo;`), `detail` is the offending markup (≤ 80 chars).
    #[error("unsupported {element}: {detail}")] Unsupported { element: String, detail: String },
}
```

Macro names: `Converter::new` passes each name with its leading `\` stripped (`\RR` → `RR`) to
`LatexToMathML::new`, because math-core 0.9.0 accepts only a single character or all-ASCII-letter
names (`custom_cmds.rs:248` `is_valid_macro_name`; `"\\R"` → `Invalid macro name`, the crate's own
example is `("d", r"\mathrm{d}")`). A name without the leading `\`, or one that fails that test
after stripping, raises `math-macro-invalid` and is skipped. `LatexToMathML::new` stops at the
first failing entry and returns its index (`MacroParseError = (Box<LatexError>, usize, String)`,
also for a replacement that uses an unknown command), so `Converter::new` removes that entry,
records one `math-macro-invalid` and retries until the call returns `Ok` (at most one retry per
macro). Fixed `MathCoreConfig` (every other field at its `Default`): `pretty_print:
PrettyPrint::Never` (byte-stable output), `xml_namespace: true` (the same string feeds HTML and
the XML-parsing OMML writer; `xmlns` is harmless in HTML), `ignore_unknown_commands: false`
(unknown commands are `MathError::Syntax`, never a silent red `<merror>`; `check` reports them),
`annotation: false` (the source already travels in `data-tex`, and the OMML writer must not see
`<semantics>`), `global_group: false`, `allow_unreliable_rendering: false`. Conversion uses
`convert_with_local_state` (no state leaks between equations). Post-fix applied to every result,
because math-core emits HTML-only named entities (R4 §2.4 row 11; 0.9.0 writes only the first two,
`ast.rs:339–341`, the rest are defensive): `&ApplyFunction;` → `&#x2061;`, `&InvisibleTimes;` →
`&#x2062;`, `&InvisibleComma;` → `&#x2063;`, `&nbsp;` → `&#xA0;`; any other `&name;` that is not
one of the five XML entities is left for the OMML writer to reject. U+2063 also appears as a
literal character (the empty delimiter of `\left.`/`\right.` and of `cases`, §3.2).

### 3.2 `math::omml` (T42; parser `quick-xml` 0.42, the DOCX crate set)

```rust
/// MathML Core (math-core or typst-html output) → OMML. Always returns exactly one
/// `<m:oMath>…</m:oMath>` (brief), whatever `display` says; `src/docx/math.rs` adds the
/// `m:oMathPara` of display equations (§5.1). Prefixes `m:`/`w:` only; the namespaces are
/// declared on `w:document` (docx-output.md). Never emits `xmlns`.
pub fn from_mathml(mathml: &str) -> Result<String, MathError>;
/// The inline fallback: the equation flattened to Unicode text (§5.3).
pub fn fallback_text(mathml: &str) -> String;
```

Normalisation before mapping (R4 §6.3), in this order: decode the numeric entities; `semantics` →
its first child; `mstyle`, `mpadded`, `maction` → `mrow` (colour and other styling are dropped
silently; v1 never emits `w:color`, R4 §10 Q3 is unverified); drop every `mspace` that has no
`width`, a width ≤ 0 or only a `style` (math-core writes a width-less `<mspace/>` before every
`\mathrm`, `\mathcal`, `\operatorname`, `\lim`, `\sin`, … and `\!` as `<mspace
style="margin-left:-0.16666667em;"/>`; texmath writes U+200B for non-positive widths, R4 §2.4 row
3); mark empty fences: an `mo` whose text is empty or U+2063 and that is the first or last child of
an `mrow` whose other end is a fence `mo` (table below) or another such `mo` is an empty fence
(math-core writes `\left.`/`\right.` and the open side of `cases` as `<mo>⁣</mo>`, U+2063); drop every other `mo`
U+2061/U+2062/U+2063; unwrap single-child `mrow`; strip the U+FE00/U+FE01 variation selectors;
fold math-alphanumeric code points to ASCII/Greek + `m:sty`/`m:scr` (§5.2). Attributes of `mo`
other than `stretchy` and `movablelimits` (`minsize`, `maxsize`, `lspace`, `rspace`, `form`,
`symmetric`, `fence`, `largeop`) are ignored: the character becomes a plain `sty p` run (math-core
writes `minsize`/`maxsize` for every `\big(`, `\Bigl[`). Then map; rows are tried in table order
and the first match wins:

| MathML Core | OMML | rule |
|---|---|---|
| `mi` one letter (`x`, `𝑥`) | `<m:r><m:t>x</m:t></m:r>` | no `m:sty`: Word italicises letters; uppercase Greek and `mathvariant="normal"` → `<m:sty m:val="p"/>` |
| `mi` multi-letter (`lim`, `argmax`, `sin`) | `<m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>lim</m:t></m:r>` | never `m:func` (pandoc emits none) |
| `mn` | plain run | |
| `mo` one char | run with `<m:sty m:val="p"/>` | multi-char operator → `<m:box><m:boxPr><m:opEmu m:val="on"/></m:boxPr><m:e>run</m:e></m:box>` |
| `mtext`, `ms` | `<m:r><m:rPr><m:nor/></m:rPr><m:t xml:space="preserve">if </m:t></m:r>` | `ms` adds its quotes |
| `mspace width="Xem"`, X > 0 | run of U+2009 (≤0.17em) / U+2005 (≤0.23) / U+2004 (≤0.5) / U+2001 (≤1.8) / U+2001 U+2001 (>1.8) | other `mspace`s were dropped by normalisation; `height`/`depth` ignored |
| `mfrac` | `<m:f><m:fPr><m:type m:val="bar"/></m:fPr><m:num>…</m:num><m:den>…</m:den></m:f>` | `linethickness="0"` → `noBar` |
| `msqrt` | `<m:rad><m:radPr><m:degHide m:val="on"/></m:radPr><m:deg/><m:e>…</m:e></m:rad>` | |
| `mroot` (base, index) | `<m:rad><m:deg>index</m:deg><m:e>base</m:e></m:rad>` | child order swaps |
| `msub` / `msup` / `msubsup` | `m:sSub` / `m:sSup` / `m:sSubSup` (`m:e`, `m:sub`, `m:sup`) | n-ary base → `m:nary` `limLoc subSup` |
| `mover`/`munder` whose `mo` has `stretchy="true"` and text U+203E, U+00AF, U+0332, U+0305 or `_` | `<m:bar><m:barPr><m:pos m:val="top|bot"/></m:barPr><m:e>…</m:e></m:bar>` | `top` for `mover`, `bot` for `munder`: math-core's `\overline` (`mover accent="true"` + stretchy U+0332) and `\underline` |
| `mover`/`munder` with ⏞ ⏟ ⎴ ⎵ | `<m:groupChr><m:groupChrPr><m:chr m:val="⏟"/><m:pos m:val="bot|top"/><m:vertJc m:val="top|bot"/></m:groupChrPr><m:e>…</m:e></m:groupChr>` | math-core wraps them in `mover accent="true"`; the label becomes the outer `limLow`/`limUpp` |
| any other `mover accent="true"` (incl. `¯` with `stretchy="false"` from `\bar`) | `<m:acc><m:accPr><m:chr m:val="&#x302;"/></m:accPr><m:e>…</m:e></m:acc>` | spacing → combining: `^ ˆ`→U+0302, `˙ .`→U+0307, `~ ˜`→U+0303, `¯`→U+0304, `˘`→U+0306, `ˇ`→U+030C, `´`→U+0301, `` ` ``→U+0300, `¨`→U+0308, `⃗ →`→U+20D7; combining marks pass through |
| any other `munder` / `mover` / `munderover` | `m:limLow` / `m:limUpp` (nested `limLow(limUpp(e, over), under)`) | n-ary base → `m:nary` `limLoc undOvr`; inline `<math>` + `movablelimits` base (`lim`, `max`, …) → `sSub`/`sSup` |
| `mrow` = fence `mo` … fence `mo` | `<m:d><m:dPr><m:begChr m:val="("/><m:sepChr m:val=""/><m:endChr m:val=")"/><m:grow/></m:dPr><m:e>…</m:e></m:d>` | fences: `( ) [ ] { } ⟨ ⟩ ⌊ ⌋ ⌈ ⌉ ⟦ ⟧ \| ‖`, both ends without `stretchy="false"`; an inner stretchy `\|`/`‖`/`∣` splits `m:e` parts and sets `sepChr`; an empty fence (normalisation) → `begChr ""`/`endChr ""` (`\left. \frac{df}{dx} \right\|_{x=0}` → `begChr ""`, `endChr "\|"`, as pandoc); `stretchy="false"` pairs stay runs |
| `mtable` aligned (class contains `aligned`, `multiline-equation` or `menv-alignlike`, or every row's `mtd`s alternate `text-align: right`/`left` styles) | `<m:eqArr><m:e>a<m:r><m:t>&amp;</m:t></m:r>=b</m:e>…</m:eqArr>` | the `&` run marks the alignment point; the first cell of every row is right-aligned |
| `mtable` other | `<m:m><m:mPr><m:baseJc m:val="center"/><m:plcHide m:val="on"/><m:mcs><m:mc><m:mcPr><m:count m:val="N"/><m:mcJc m:val="center"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr><m:e>…</m:e></m:mr>…</m:m>` | `mcJc` from `mtd` `style="text-align"`/classes `left-align`/`right-align`; one `m:mc` per distinct run of alignments; `cases` (class `cases`, or `{` fence + table) → `m:d` with `begChr "{"`, `endChr ""` around the `m:m` |
| `mphantom` | `<m:phant><m:phantPr><m:show m:val="off"/></m:phantPr><m:e>…</m:e></m:phant>` | |
| `menclose` | `<m:borderBox><m:e>…</m:e></m:borderBox>` (+ `hideTop/Bot/Left/Right` + `strikeBLTR`/`strikeTLBR` for `updiagonalstrike`) | only if a converter emits it |
| `mmultiscripts` with one `mprescripts` pair and no postscripts (pairs of empty `<mrow/>` count as none) | `<m:sPre><m:sub/><m:sup/><m:e/></m:sPre>` | typst `attach(X, tl: 1, bl: 2)` → `<mmultiscripts><mi>𝑋</mi><mrow></mrow><mrow></mrow><mprescripts/><mn>2</mn><mn>1</mn></mmultiscripts>`; math-core's `{}_1^2 X` is an `msubsup` with an empty base instead (→ `sSubSup` with the U+200B base, as pandoc); anything else (`\sideset` has real postscripts) → `Unsupported` |
| empty `mrow`, empty cell, empty script, empty `<math>` | `<m:r><m:t>&#x200B;</m:t></m:r>` | texmath: "avoid the dashed box" |

N-ary detection (texmath `isNary`): an `mo` whose text is one of `∫ ∬ ∭ ∮ ∯ ∰ ∏ ∐ ∑` (U+222B–U+2230,
U+220F, U+2210, U+2211). Its scripts become `<m:nary><m:naryPr><m:chr m:val="∑"/><m:limLoc
m:val="undOvr|subSup"/><m:subHide m:val="on|off"/><m:supHide m:val="on|off"/></m:naryPr><m:sub>…
</m:sub><m:sup>…</m:sup><m:e>…</m:e></m:nary>`; the body is exactly the next sibling element
(none → the U+200B run), as texmath does. `∫` as a bare `mo` without scripts → `m:nary` with both
`*Hide="on"`. Everything else not in the table (`mlabeledtr`, `mfenced`, `mglyph`, `mtd`
`rowspan`/`columnspan`, `merror` — math-core writes it only with `ignore_unknown_commands`, which
writer turns off — unknown elements, unknown entities) → `MathError::Unsupported`. The function
never panics on foreign markup.

### 3.3 `math::typst_to_latex` (T51; `typst-syntax = "0.15.1"`, `codex = "0.3.0"`)

```rust
use typst_syntax::ast;
/// The cross-spec entry point (brief): one equation body (`Math.source`) → the LaTeX body of
/// `\(…\)`, `\[…\]` or `equation` (§7.2). Never fails: an equation that contains `#` code becomes
/// `\text{[typst math: <source escaped>]}` as a whole, with one `E-TYP-MATH-CODE` as its only
/// diagnostic. Other diagnostics are `W-TYP-MATH-*`/`W-TYP-ARG-IGNORED` (§9). ir.md's `Diagnostic`
/// has no column, so positions go into the message (`at bytes a..b of the equation`); the caller
/// sets `path`/`span` from the enclosing block.
pub fn convert(source: &str) -> (String, Vec<Diagnostic>);
/// The walker behind `convert` (same rules), for a `Math` node already in hand
/// (`Equation::body()` or `typst_syntax::parse_math(src).cast::<ast::Math>()`).
pub fn convert_ast(math: ast::Math<'_>) -> (String, Vec<Diagnostic>);
/// `name` → LaTeX via the symbol table (`assets/math/unicode-latex.toml`, embedded).
pub fn symbol(path: &str) -> Option<&'static Sym>;         // "arrow.r.long", "RR"
pub fn char_command(c: char) -> Option<&'static Sym>;     // '⟶', '≤'
pub struct Sym { pub tex: &'static str, pub package: Option<&'static str> /* beyond amsmath+amssymb */ }
/// `name` = one control word with its backslash (`"\\mathscr"`) → the package defining it beyond
/// amsmath+amssymb (`Some("mathrsfs")`), else `None`; from `COMMAND_PACKAGES` below.
pub fn command_package(name: &str) -> Option<&'static str>;
```

`command_package` is the one package lookup of the LaTeX target (§7.1–7.2), for latex-dialect
sources and `convert` output alike. It reads `const COMMAND_PACKAGES: &[(&str, &str)]` (command →
package, in `src/math/typst_to_latex.rs`, T51), the union of: the packaged rows of the function
table below (`\mathscr` → `mathrsfs`; `\overbracket`, `\underbracket` → `mathtools`; `\cancel` →
`cancel`), every `[packages]` entry of `scripts/unicode-latex-overrides.toml` (vendored-assets.md
§3.7 step 9, the source of every `Sym.package`), and the latex-source commands of latex-output.md
§4.8's list (`\bm` → `bm`; `\bcancel`, `\xcancel` → `cancel`; `\mathclap`, `\xmapsto` →
`mathtools`; `\Mapsfrom`, `\longmapsfrom` → `stmaryrd`). Two T51 unit tests keep it in step: one
parses the overrides' `[packages]` table (`toml`, path from `CARGO_MANIFEST_DIR`) and asserts that
every (command, package) pair is in `COMMAND_PACKAGES`; one calls `char_command(c)` for every
`char` and `symbol(name)` for every table name and asserts that each `Sym` with `package: Some(p)`
has a control word in its `tex` (`\` + the maximal run of ASCII letters) that `command_package`
maps to `p`. So scanning an output string's control words finds every package `convert` relied on.

Walk (R2 §7.2 and §9 snippet 3): `Math` → children; `MathText` grapheme → `char_command` or the
ASCII char; number → digits verbatim; `MathIdent`/`MathFieldAccess` (`to_untyped().full_text()`
as the path) → `symbol()` first, else `codex::SYM.get(name)` + `Symbol::get(ModifierSet::
from_raw_dotted(mods))` → `char_command`, else the operator table (`lim`, `sin`, … → `\lim`,
`\sin`; `sinc`, `tr`, `id`, … → `\operatorname{…}`; `liminf`/`limsup` → `\liminf`/`\limsup`), else
`dif`/`Dif` → `\mathrm{d}`/`\mathrm{D}`, spacings `thin med thick quad wide` → `\, \: \; \quad
\qquad`, else `W-TYP-MATH-SYM-UNKNOWN` + `\mathit{name}`; `MathShorthand` → `char_command(get())`;
`MathAlignPoint` → `&`; `Linebreak` → `\\`; `Str` → `\text{…}` (escaping `\ { } $ & # % _ ^ ~`);
`Escape` → `char_command` or the char; `Space` → one space; `MathAttach` → `base_{…}^{…}`
(always braced) + primes `'`×count; `MathFrac` → `\frac{num}{den}`; `MathRoot` → `\sqrt{…}`,
`\sqrt[3]{…}`, `\sqrt[4]{…}`; `MathDelimited` → `\left<open> … \right<close>` (mismatched pairs
allowed; `{`/`}` escaped; `|`→`|`, `‖`→`\|`); `MathPrimes` on their own → `'`; `MathCall`: callee
in the function table below → mapped (rows split on `MathArg::ends_in_semicolon`; named args
`delim:`, `limits:` honoured, others `W-TYP-ARG-IGNORED`); callee resolving through codex to a
char in typst's `ACCENTS` (R2 §4.5) → the accent command; callee resolving to any other symbol →
symbol followed by `\left( … \right)`; unknown callee → `W-TYP-MATH-FN-UNKNOWN` +
`\operatorname{name}(…)`; `Hash` + code → the whole-equation fallback above, except literal
named-arg values (`#true`, `#false`, `#none`, numbers).

| typst | LaTeX |
|---|---|
| `frac(a, b)`, `binom(n, k)`, `sqrt(x)`, `root(n, x)` | `\frac{a}{b}`, `\binom{n}{k}`, `\sqrt{x}`, `\sqrt[n]{x}` |
| `vec(1, 2, 3)` | `\begin{pmatrix}1 \\ 2 \\ 3\end{pmatrix}` (`delim:` as for `mat`) |
| `mat(a, b; c, d)` | `\begin{pmatrix}a & b \\ c & d\end{pmatrix}`; `delim: "["`→`bmatrix`, `"{"`→`Bmatrix`, `"\|"`→`vmatrix`, `"‖"`→`Vmatrix`, `#none`→`matrix` |
| `cases(1 "if" x > 0, 0 "else")` | `\begin{cases}1 \text{if} x > 0 \\ 0 \text{else}\end{cases}` |
| `abs(x)`, `norm(v)`, `floor(x)`, `ceil(x)`, `round(x)` | `\left\lvert x\right\rvert`, `\left\lVert v\right\rVert`, `\left\lfloor x\right\rfloor`, `\left\lceil x\right\rceil`, `\left\lfloor x\right\rceil` |
| `lr(( a/b ])`, `lr(x)` | `\left( \frac{a}{b} \right]`; body without delimiters → unchanged |
| `hat tilde grave acute breve caron macron diaer dot dot.double arrow arrow.l arrow.l.r circle` | `\hat \tilde \grave \acute \breve \check \bar \ddot \dot \ddot \vec \overleftarrow \overleftrightarrow \mathring` |
| `overline(x)`, `underline(x)`, `overbrace(x, l)`, `underbrace(x, l)`, `overbracket`/`underbracket` | `\overline{x}`, `\underline{x}`, `\overbrace{x}^{l}`, `\underbrace{x}_{l}`, `\overbracket{x}^{l}`/`\underbracket{x}_{l}` (package `mathtools`, §7.2) |
| `bold upright italic bb cal frak sans mono scr` | `\mathbf \mathrm \mathit \mathbb \mathcal \mathfrak \mathsf \mathtt \mathscr` (`\mathscr`: package `mathrsfs`, §7.2) |
| `op("x")`, `op("x", limits: #true)` | `\operatorname{x}`, `\operatorname*{x}` |
| `limits(x)`, `scripts(x)` | `\mathop{x}\limits`, `\mathop{x}\nolimits` (`A\limits` is the TeX error "Limit controls must follow a math operator", and typst allows `limits(arrow.r)^f`); when `x` converts to a command that already is an operator (`\sum`, `\prod`, `\coprod`, `\int`, `\oint`, `\bigcup`, `\bigcap`, `\lim`, `\max`, `\min`, `\sup`, `\inf`, `\det`, `\operatorname{…}`, `\operatorname*{…}`, …) the modifier follows it directly (`\sum\limits`) |
| `display(x) inline(x) script(x) sscript(x)` | `{\displaystyle x}` `{\textstyle x}` `{\scriptstyle x}` `{\scriptscriptstyle x}` |
| `cancel(x)` | `\cancel{x}` (package `cancel`, §7.2) |
| `text(..)[x]`, `class(..)`, `stretch(..)`, `attach(..)`, `primes(n)`, `mid(x)` | `x` with `W-TYP-MATH-FN-UNKNOWN` (arguments other than the body ignored); `mid(x)` → `\middle x` |

Spacing: one space between adjacent tokens where the source had `Space`; none elsewhere
(`x_1^2 + x_(i j)` → `x_{1}^{2} + x_{i j}`). An equation containing `&` or `\` is wrapped in
`\begin{aligned} … \end{aligned}`. `assets/math/unicode-latex.toml` is derived from tylax 0.3.8's
`TYPST_TO_TEX` (532 typst names → commands, Apache-2.0, license shipped per vendored-assets.md)
with hand overrides consulted first: `[names]` (typst symbol path → `{ tex, package? }`) and
`[chars]` (code point → `{ tex, package? }`, incl. every shorthand of `MathShorthand::LIST`,
Greek U+0391–U+03C9 → `\Gamma … \omega` with plain letters for the Latin-shaped capitals, the
math alphanumerics U+1D400–U+1D7FF → `\mathbf{A}`-style forms). Output never contains a
non-ASCII character except a grapheme with no mapping, which passes through with
`W-TYP-MATH-SYM-UNKNOWN`.

### 3.4 `typst::render::equation` (T03; signatures and pipeline: typst-dialect.md §9)

```rust
// typst-dialect.md §9 = the brief's cross-spec names (repeated here, not redefined); Result = crate::error::Result.
pub struct Rendition { pub svg: Vec<u8>, pub png: Vec<u8> /* 2 px/pt */, pub pdf: Option<Vec<u8>> /* None for equations */,
                       pub width_pt: f64, pub height_pt: f64, pub baseline_pt: f64 }
pub fn equation(world: &mut WriterWorld, source: &str, dialect: MathDialect, display: bool) -> Result<Rendition>;
// helpers T03 adds next to it:
impl Rendition { pub fn descent_pt(&self) -> f64 /* height - baseline */; pub fn png_size(&self) -> (u32, u32); }
```

`source` is compiled as given: for `dialect: latex` the caller passes `latex_source::for_typst(src,
&macros).0` (§3.6), the exact text the typst emitter writes, so the renderer never strips,
normalises or adds macros. The compiled main file is typst-dialect.md §9's preamble plus one
boxed equation (R4 §5.1; probed with typst 0.15.1):

```typst
#set page(width: auto, height: auto, margin: 0pt, fill: none)
#set text(top-edge: "bounds", bottom-edge: "bounds")
// latex dialect additionally: #import "@preview/mitex:0.2.7": mi, mitex
#box($x^2 + 1/2$)           // inline typst  |  #box($ … $) display typst
#box(mi(`…`))               // inline latex  |  #box(mitex(`…`)) display latex (§6 literal rule)
```

`top-edge`/`bottom-edge: "bounds"` is mandatory: without it inline descenders are clipped (R4
§5.2). The `#box(…)` is mandatory too: without it typst places the items of many equations
directly in the page frame with no `Group` (`$a/b$`, `$x$`, `$ a/b $`, `#mi(`\frac{a}{b}`)`), so
a baseline lookup finds nothing and a fallback to the height would lift inline pictures (3.9 pt
for `a/b`); the box yields one `FrameItem::Group` with `has_baseline()` at the same size.
`baseline_pt` = `pos.y + g.frame.baseline()` of that group; a missing group is an `Err`
(`Error::Other("equation has no baseline")`), never the height. Size and fonts are typst's
defaults (11 pt, Libertinus Serif, New Computer Modern Math, all embedded via typst-kit
`embedded-fonts` and served first), so renditions are byte-identical on every machine; a caller
that needs another size scales the picture (§4.4, §5.3). Measured anchors for the tests: inline
`x^2 + 1/2` → 31.317 × 13.242 pt, baseline 9.447 (descent 3.795), PNG 63×26 px; inline `a/b` →
6.974 × 11.602 pt, baseline 7.73; inline `x` → 6.292 × 4.983 pt, baseline 4.862; display
`integral_0^1 x^2 dif x = 1/3` → 65.719 × 29.159 pt, baseline 17.334; the SVG holds glyphs as
`<symbol><path>` (no `<text>`, no font reference), 2.6–12.7 KB; 15–65 µs per equation warm. A
compile error is `Error::Compile` (typst-dialect.md §9; the first diagnostic, the user's call as
location); every caller maps any `Err` to its fallback + `math-conversion-failed`.

### 3.5 `typst::html::equation_mathml` (T03; signatures: typst-dialect.md §9)

```rust
// typst-dialect.md §9 (the brief's names; `display` is that spec's listed deviation):
pub const EQUATION_CSS: &str = "/* Alignment */\n…";   // typst-html 0.15.1 EQUATION_CSS_STYLES, expanded
pub fn equation_mathml(world: &mut WriterWorld, source: &str, display: bool) -> Result<String>;   // `<math …>…</math>`
```

`typst_html::EQUATION_CSS_STYLES` is `pub(crate)` (`typst-html-0.15.1/src/mathml.rs:99`), so T03
copies its expanded text into `src/typst/html.rs` with typst's Apache-2.0 notice: 1,494 B between
`<style>` and `</style>` in 0.15.1 (R4's 1,501 B counted the opening tag). T03's test compiles `$ x
$` with the HTML library and asserts the `<style>` text equals `EQUATION_CSS`
(`typst_equation_css.snap`), so a typst upgrade fails loudly. `math_init()` prints it once per
document (§4.5). `equation_mathml` returns `Err` for a compile error and for every warning except
the detached "html export is under active development" one, in particular `… was ignored during
MathML export` (`mathml.rs:1344`; raised for `overline`, `underline`, `cancel` and `skewed
fraction`, `mathml.rs:594–608`): the MathML would silently lack the construct, so callers take the
dialect's fallback (§4.5, §5.3) with `math-conversion-failed` naming it. Identifiers come out as
math-italic code points (`<mi>𝑥</mi>`), `cal(F)` as `ℱ︀` (U+2131 U+FE00) and `scr(L)` as `ℒ︁`
(U+2112 U+FE01): fine for browsers, folded by `math::omml` (§5.2).

### 3.6 `math::latex_source` (T30; `src/math/latex_source.rs`)

Every route needs the same source transformations, and T30 (wave 3) is their first user, before
T1A (wave 5, html-output.md Deviations), T33, T42 and T60 (wave 6):

```rust
pub fn strip_tags(src: &str) -> (String, bool);       // §2 rule 2; bool = something was removed
pub fn normalise_env(src: &str) -> String;            // §2 rule 3
/// `(normalise_env(&strip_tags(src).0), removed)`: what KaTeX, math-core and `check` convert.
pub fn prepare(src: &str) -> (String, bool);
/// String entries of `katex_options["macros"]`, keys with their backslash (`"\\RR"`); others ignored.
pub fn macros_from_config(opts: &serde_json::Map<String, serde_json::Value>) -> BTreeMap<String, String>;
pub fn macro_arity(replacement: &str) -> u8;          // highest `#k` (1–9), 0 when none
pub fn macros_as_newcommands(macros: &BTreeMap<String, String>) -> String;  // "\newcommand{\RR}{\mathbb{R}}\n…", key order
/// `macros_as_newcommands(macros) + prepare(src).0`: the `#mi`/`#mitex` argument, the renderer's
/// input and the mitex pre-flight's input (§2 rule 1).
pub fn for_typst(src: &str, macros: &BTreeMap<String, String>) -> (String, bool);
```

Callers raise `math-tag-ignored` when the returned `bool` is true. `MathError` stays in
`latex_mathml` (T1A); nothing in this module fails.

## 4. HTML (`src/html/math.rs`, T1A; markup repeated in html-output.md)

### 4.1 Markup (all routes)

```html
<span class="math inline" data-math-style="inline" data-tex="x^2">x^2</span>       <!-- Inline::Math, katex route -->
<span class="math inline" data-tex="x^2"><math …>…</math></span>                  <!-- mathml/typst/image routes and fallbacks: no data-math-style -->
<div id="eq-amdahl" class="equation numbered">                                     <!-- Block::Math, one line in the output (html-output.md §1.8, W3) -->
  <span class="math display" data-math-style="display" data-tex="T(n) = \frac{W}{n} + c">T(n) = \frac{W}{n} + c</span>
  <span class="equation-number">(1)</span>                                          <!-- only when numbered -->
</div>
```

`data-tex` is the text the span was produced from, attribute-escaped: `latex_source::prepare(source).0`
for `dialect: latex` (tags stripped, environments normalised, §2 rules 2–3) and `source` for
`dialect: typst`; it is the live-reload morph key. Only `katex`-route spans carry
`data-math-style` (`inline`/`display`), and their content is the same text; `math.js` renders
exactly those (§4.2), so a span whose content is written at build time (a `<math>` from math-core
or the compiler, an `<img>`, a fallback) is never touched in the browser. `src/html/math.rs`
implements html-output.md §1.8's `MathHook` (`html::math::MathRenderer`); html-output.md owns
`div.equation` (`id` = `attrs.id`; classes `equation`, `numbered` when `number` is `Some`, then
`attrs.classes`; `attrs.pairs` as pass-through attributes, `custom-style` skipped) and the number
span. The default theme (T19) styles `.equation` (flex row, number pushed right),
`.math.display` (centred) and `.math-error`. The hook reports html-output.md's `MathUsage {
katex, mathml, typst_mathml, image }`, which drives `math_init()` (html-output.md §4.3): `katex`
when a latex equation went through the `katex` route, `mathml` when math-core wrote a `<math>`,
`typst_mathml` when the compiler did, `image` when a rendition was used.

### 4.2 `katex` route (default)

- Vendored KaTeX **0.19.1**: `katex.min.js` (273,081 B), `katex.min.css` (24,793 B), `fonts/*.woff2`
  (20 files, 259,792 B), `LICENSE` (vendored-assets.md; presenter had 0.19.0; no CSS change
  between them, R4 §4.1). `math_init()` (html-output.md §4.3) includes the CSS, the options
  script, the JS and `writer/math.js` only when `math.katex` — ≈ 645 KB inlined (js 273,081 B and
  css 24,793 B as text, the woff2 files as base64 ≈ 346 KB; PLAN §2 quotes ≈650 KB).
- `assets/writer/math.js` is adapted from presenter's `math.js`, which renders every
  `[data-math-style]` from `el.textContent` with `displayMode: el.dataset.mathStyle ===
  'display'`. writer keeps that selector, source and display test, adds `:not([data-rendered])`
  and sets `data-rendered="1"` after rendering, renames the globals
  (`window.__writerRenderMath(root)`, `window.__writerKatexOptions`) and keeps presenter's
  try/catch with `console.warn`: `katex.render(el.textContent, el, Object.assign({ displayMode,
  throwOnError: false }, window.__writerKatexOptions || {}))`; runs on `DOMContentLoaded` (or
  immediately). `window.__writerKatexOptions = <json>` is written by `math_init()` from `[math]
  katex_options` with presenter's `script_safe_json` (`<` escaped); `macros` go through verbatim,
  so KaTeX-only keys (`fleqn`, `leqno`, `strict`, `trust`, `maxExpand`, `output`) work.
- Live-reload guard (live-reload.md §3): `morphdom(root, fresh, { onBeforeElUpdated(from, to) {
  if (from.classList.contains('math') && from.dataset.rendered === '1' && from.dataset.tex ===
  to.dataset.tex) return false; … } })`, then `window.__writerRenderMath(root)` renders new and
  changed equations (a changed `data-tex` is morphed, loses `data-rendered`, re-renders). Rendered
  equations are never re-rendered unchanged (browser test, PLAN §7).
- No build-time conversion: invalid LaTeX renders red in the browser (`throwOnError: false`,
  `errorColor` `#cc0000`). The build raises `math-tag-ignored` when `prepare` removed something;
  `check` still reports `math-syntax` from math-core (§9).

### 4.3 `mathml` route

`latex_mathml::to_mathml(&prepare(source).0, display, &macros)` output replaces the span's content
(`math-tag-ignored` when `prepare` removed something; `macro_diagnostics` once per document).
`math_init()` (html-output.md §4.3, T15) writes once per document, when `math.mathml` or
`math.typst_mathml`, `<style id="writer-math">` holding (`[math] mathml_font = "embed"`) the
`@font-face`, then the font rule, then (`typst_mathml`) `EQUATION_CSS`:

```css
math { font-family: "Latin Modern Math", "STIX Two Math", "Cambria Math", "Libertinus Math", math; }
```

because the UA default (`font-family: math`) rendered badly in Chromium 152 even with STIX Two
Math installed (R4 §3.2). The `@font-face { font-family: "Latin Modern Math"; src:
url("data:font/woff2;base64,…") format("woff2"); }` comes from the vendored
`latinmodern-math.woff2` (388,212 B, GUST FL; source and license per vendored-assets.md) — +≈ 518
KB base64 (388,212 × 4/3 = 517,616 B), so `"mathml"` is only lighter than KaTeX with
`mathml_font = "system"`. `MathError::Syntax` → the span's content is `err.html` (a
`span.math-error` whose `title` is the message and whose `<code>` is the source) and
`math-syntax` is raised; the default theme colours `.math-error` `#cc0000`. Alignment is
self-contained (`mtd style=` inline, R4 §2.4 row 8); no extra CSS.

### 4.4 `image` route

`typst::render::equation(world, text, dialect, display)` (latex: `text` =
`latex_source::for_typst(source, &macros).0`, through mitex; typst: `source`, native) → the SVG is
registered (`assets.register_bytes(r.svg.clone(), "image/svg+xml", &format!("eq-{n}.svg"))`, ir.md
§3) and the span holds `<img src="asset:<id>" alt="<source>"
style="height:<H>pt;vertical-align:-<D>pt">` with `H = height_pt × s`, `D = (height_pt −
baseline_pt) × s` (R4 §5.2) and `s = 12/11` (the theme's 16 px = 12 pt body text over the
rendition's 11 pt), so the picture's baseline sits on the text baseline; `assets::emit` resolves
`asset:` as for any image (data URI standalone, file in site mode, `/__writer/doc-asset/<id>?v=…`
in serve). A render failure → `<code class="math-error">source</code>` + `math-conversion-failed`.

### 4.5 typst math

Typst equations render as MathML from `typst::html::equation_mathml(world, source, display)`
(`[math] html = "image"` is the only other option; `katex`/`mathml` are indistinguishable for
typst): the `<math>` replaces the span's content, the font rule and font embedding of §4.3 apply,
and `math_init()` appends `EQUATION_CSS` once (`math.typst_mathml`). The classes
`multiline-equation`, `aligned`, `cases`, `left-align`, `right-align`, `flushed`, `left-flush`,
`right-flush` are typst's; writer never renames them. `equation_mathml` → `Err` (a compile
error, or a construct the MathML export ignores: `overline`, `underline`, `cancel`, skewed
fractions; §3.5) → that equation takes the `image` route + `math-conversion-failed` naming the
reason.

## 5. DOCX (`src/docx/math.rs`, T42; docx-output.md owns the paragraph/table XML)

### 5.1 Placement

- `Inline::Math` → `<m:oMath>` as a run-level sibling of `w:r` inside the `w:p` (R3 §2.18). A
  paragraph consisting only of inline math gets a trailing U+200B run (pandoc, verified).
- `Block::Math` unnumbered → its own `<w:p>` (style per docx-output.md §4.15) in which
  `src/docx/math.rs` wraps `from_mathml`'s `<m:oMath>` (which never carries it) in
  `<m:oMathPara><m:oMathParaPr><m:jc m:val="center"/></m:oMathParaPr><m:oMath>…</m:oMath></m:oMathPara>`.
- `Block::Math` numbered → the 3-column borderless table (10/80/10 %) of docx-output.md: the
  `m:oMathPara` in the middle cell, `(n)` in the right cell wrapped in the bookmark named
  `resolve::anchor(label)` (references.md), so `REF` fields and internal links resolve.
- DOCX reads no `Block::Math.attrs` beyond `id` (and resolve's `unnumbered`, §8); a style is set
  by wrapping the equation in a `custom-style` `Div` (docx-output.md).
- `settings.xml`'s `<m:mathPr>` is docx-output.md §6.4's: Word's own template value (verified
  from Word's `Modern.dotx`; it has `defJc centerGroup` and `wrapIndent 1440`, no `wrapRight`),
  written by T04. T42 writes no settings. No per-run font: Word's math font applies.

### 5.2 Writer rules beyond §3.2

- Multi-letter identifiers get `<m:sty m:val="p"/>`; uppercase Greek `sty p`; everything else
  plain (Word's default italic).
- Math-alphanumeric folding (R4 §10 Q2, decided: fold; pandoc folds, Word's rendering of raw
  code points is unverified): strip U+FE00 and U+FE01 (the Chancery/Roundhand selectors:
  math-core's `\mathcal` and typst's `cal` give U+FE00, typst's `scr` U+FE01), then look the code
  point up in a reverse table built at first use from `codex::styling::to_style(c, s).next()`
  (feature `styling`, on by default; `to_bold` and the other per-style functions are private in
  `mod conversions`, `styling.rs:415`) for `s` in `MathStyle::{Bold, Italic, BoldItalic, Script,
  BoldScript, Fraktur, BoldFraktur, DoubleStruck, SansSerif, SansSerifBold, SansSerifItalic,
  SansSerifBoldItalic, Monospace}` over `A–Z a–z Α–Ω α–ω 0–9` (Chancery and Roundhand are Script
  + a selector, so Script covers them once the selector is gone; this reproduces the Letterlike
  holes `ℝ ℂ ℕ ℙ ℚ ℤ ℍ ℱ ℋ ℬ ℰ ℐ ℒ ℳ ℛ ℎ ℭ ℌ ℑ ℜ ℨ` exactly as typst produces them). Style →
  props: bold `sty b`; italic none; bold italic `sty bi`; script `scr script`+`sty p`; bold
  script `scr script`+`sty b`; fraktur `scr fraktur`+`sty p`; double-struck `scr
  double-struck`+`sty p`; sans `scr sans-serif`+`sty p` (+`b`/`i`/`bi` variants); monospace `scr
  monospace`+`sty p`. pandoc (texmath) agrees on `\mathrm{d}` → `sty p` and `\mathcal{F}` → `scr
  script`+`sty p`, but writes `\mathbf{A}` as `sty bi` and `\bar{z}` as `m:acc` with `chr ‾`
  (U+203E) (R3 §4.1, pandoc 3.12.1); writer deliberately writes `sty b` (math-core's `𝐀` U+1D400
  is bold upright) and U+0304. T42's pandoc comparison lists exactly these two as exceptions.
- Text runs (`mtext`) → `m:nor`; `xml:space="preserve"` whenever the text has leading/trailing
  spaces.

### 5.3 Fallback cascade (`[docx] math = "omml"`)

1. No MathML. latex: `latex_mathml::to_mathml` fails → one run with the source text verbatim
   (inline: `Verbatim Char` style; display: a centred paragraph in `Source Code` style) +
   `math-syntax`. typst: `typst::html::equation_mathml` fails (a compile error or an ignored
   construct, §3.5) → display: step 2's picture, inline: the source-text run; +
   `math-conversion-failed`.
2. `omml::from_mathml` → `MathError::Unsupported` → **display**: `typst::render::equation`
   rendition (latex input as in §4.4) as a centred inline picture (`wp:inline`, SVG part + PNG
   fallback blip exactly as figures, R3 §2.11; extent = `width_pt × s × 12700` EMU with `s` below;
   `wp:docPr descr` = the source) + `math-conversion-failed` naming the element; **inline**: a
   plain run of `omml::fallback_text(mathml)` + `math-conversion-failed`. No inline pictures in
   this mode, so the unverified baseline question (R4 §10 Q1: `w:rPr/w:position` on an inline
   `w:drawing`) is not on the default path.
3. The rendition fails too → step 1's source-text form + `math-conversion-failed`.

`fallback_text` flattening: `mi/mn/mo/mtext` text in order (math-alphanumerics folded to base
letters), `msup` → `base^s` (`base^(s)` when `s` is more than one token), `msub` → `_`,
`msubsup` → `_…^…`, `mfrac` → `(num)/(den)`, `msqrt` → `√(…)`, `mroot` → `(index)√(…)`,
`munder/mover` → `base_(…)`/`base^(…)`, `mtable` rows joined by `; ` with cells by `, `,
`mspace` → one space, fences kept. Deterministic; the only Unicode comes from the source.

`[docx] math = "image"`: every equation is a rendition picture; display as in step 2; inline as
an inline `w:drawing` in a run whose `w:rPr` carries `<w:position w:val="-<round(2·descent_pt·s)>"/>`
(half-points; the R4 §5.2 suggestion, chosen because it is a run property writer already
builds). T42 implements exactly this; T43 adds the item "an inline equation picture sits on the
text baseline" to the manual Word checklist in `docs/dev/docx-validation.md`, and a failure there
is a bug filed against T42 after the release, not a T42 decision. Picture size: the 11 pt
rendition scaled by `s = <body font size of the reference's Normal style, 11 default> / 11`
(extent, `w:position`; the PNG keeps its 2 px/pt pixels).

## 6. PDF (typst emitter T30; corpus and pre-flight T33)

- `Math { dialect: Latex }` → `#mi(..)` (inline) / `#mitex(..)` (display) from the vendored
  `@preview/mitex` package (version pinned in vendored-assets.md; 0.2.7 was probed: `typst.toml`,
  `lib.typ`, `mitex.typ`, `mitex.wasm` 280,797 B, `specs/{mod,prelude}.typ`,
  `specs/latex/standard.typ`, `LICENSE` are the served files). The argument text is
  `latex_source::for_typst(source, &macros).0` (§3.6: the macro lines, then the rule 2 stripped
  and rule 3 normalised source). `mi`/`mitex` accept a string or any content with a `text` field
  (`get-elem-text` in `mitex.typ`); the emitter follows typst-dialect.md §5.2's literal rule:
  `` `text` `` when the text has no backtick (a single-backtick raw may span lines and its text is
  the content verbatim), else a string literal with §5.2's string escaping. Two-backtick fences are
  never written: typst lexes ``` `` ``` as an empty raw (`typst-syntax` `lexer.rs:199`), so
  ``` #mitex(``a`b``) ``` is a syntax error ("unclosed delimiter"). Both forms read back
  byte-identically.
- **Pre-flight and fallback (T30).** Before writing a call the emitter runs
  `mitex::convert_math(&text, None)` (crate `mitex = "0.2.4"`; `Result<String, String>`). `Err(msg)`
  → the equation is written as `$#raw("<source>")$` (inline) / `$ #raw("<source>") $` (display,
  label and numbering as below; the IR source, string-escaped per §5.2; such an equation reads
  back as typst math, not as itself) + `math-conversion-failed` (`pdf: cannot convert equation
  (unknown command: \foo); used the source text`), so one bad equation never fails the PDF (§2
  rule 4). mitex, crate 0.2.4 and package 0.2.7 alike, rejects unknown commands and environments:
  `\foo{x}` → `error: unknown command: \foo`, `\ce{H2O}` → `unknown command: \ce`, `\sideset` too
  (compiled directly: `plugin errored with: error: unknown command: \foo`). It silently accepts
  unbalanced braces (`\frac{a}{b` → `frac(a ,b )`, rendered as a/b), which only math-core's
  `math-syntax` (from `check`) reports. The crate drops `\label` (writer strips it first anyway);
  `\textbf` → `#textbf[…]` and `\text` spacing are package-side; the crate's output is not
  standalone typst (needs `mitex-scope`), so the pre-flight validates parsing only.
- **Round trip (md → typ → IR).** The typst frontend (T50, typst-dialect.md §2.4) reads the
  argument (§1) and removes a leading copy of `macros_as_newcommands(&macros_from_config(..))`
  of the effective config when present. After md → typ → IR, `dialect` is unchanged and `source
  == latex_source::prepare(original).0`: byte identity for every source without tags,
  `\nonumber`/`\notag` or a top-level environment; the round-trip golden asserts exactly this
  (see Deviations).
- The import `#import "@preview/mitex:<pinned>": mi, mitex` is emitted only when the document has
  a latex-dialect equation; typst-only documents do not depend on the package. The World serves
  the vendored version; any other version is `NotFound` unless `[pdf] allow_packages`.
- A labelled/numbered display equation: `#mitex(..) <eq-x>` — the returned
  `math.equation(block: true)` takes the label; numbering comes from the `@writer/doc` policy (§8).
- `writer build --to typst` writes the same calls; the exported tree compiles with the plain CLI
  because the CLI downloads the same package (and `eject typst-package` writes `preview/mitex/<ver>/`).
- A plugin error the pre-flight missed (crate and package versions differ) is `Error::Compile`
  for the pdf target, located at the `#mi`/`#mitex` call through the source map (T32); other
  targets still build. The compile is the truth.
- `check --target pdf` runs the same pre-flight on the same text per latex equation and reports
  `math-conversion-failed` with the message.

## 7. LaTeX export (T60; latex-output.md owns the surrounding environments)

### 7.1 `dialect: latex` — passthrough

| IR | LaTeX |
|---|---|
| `Inline::Math` | `\(source\)`, source verbatim (no escaping, no Unicode mapping, no `\label` stripping) |
| `Block::Math` unnumbered | `\[\nsource\n\]`; when labelled, preceded by the line `\phantomsection\label{<resolve::anchor(label)>}` (latex-output.md §4.3, §4.8), so `@eq-x` to it (`unnumbered-target`, references.md §4.5) still links with `\hyperref` (latex-output.md §4.10) |
| `Block::Math` numbered | `\begin{equation}\nsource\n\label{<resolve::anchor(label)>}\n\end{equation}` (label only when labelled; references.md) |
| source whose first non-blank token is `\begin{align}`, `align*`, `gather(*)`, `multline(*)`, `equation(*)`, `eqnarray(*)` | emitted bare (no `\[`/`equation`); the starred/unstarred form follows `number`, touching only the `\begin`/`\end` names: `Some` → unstarred, `None` → starred (`align` ↔ `align*`, `gather` ↔ `gather*`, `multline` ↔ `multline*`, `equation` ↔ `equation*`, `eqnarray` ↔ `eqnarray*`), so LaTeX numbers exactly the equations the IR numbers; when labelled, `number: Some` inserts `\label{<resolve::anchor(label)>}` directly after the `\begin{<env>}` token (it labels the first row, also when that row shares the `\begin` line; `check` lints `math-env-label` when the environment has `\\`), and `number: None` puts `\phantomsection\label{<resolve::anchor(label)>}` on a line before the environment (latex-output.md §4.8) |
| `aligned`, `cases`, `pmatrix`, … inside | untouched |
| `Raw { format: "latex" }` from a multi-formula `text/latex` output | verbatim (notebook-mapping.md) |

The generated preamble (latex-output.md) loads `amsmath` and `amssymb` whenever `Features.math`
(R7 §4.1), plus the packages of §7.2's control-word scan (which covers passthrough sources too:
a source using `\mathscr` gets `mathrsfs`), and holds the `\providecommand` lines for `[math]
katex_options.macros` (§2 rule 1). Math source stays verbatim (brief: passthrough), but when the
effective `[latex] unicode` policy is `map` (`auto` with `engine = "pdflatex"`, or `map`; config.md) the preamble also gets
`\usepackage{newunicodechar}` and one `\newunicodechar{<c>}{\ensuremath{<char_command(c).tex>}}`
(plus its `Sym.package`, §7.2) per distinct non-ASCII character of the latex-dialect math
sources, in first-use order: pdflatex stops with "Unicode character … not set up for use with
LaTeX" on `≥ ≠ α ∞` without them (R7 §7.1, PLAN §9; fixture `29`). A character without a
`char_command` entry raises `math-conversion-failed` (target `latex`) naming it; with `keep`
nothing is added.

### 7.2 `dialect: typst` — converted

`typst_to_latex::convert(source)` → the same forms as §7.1 with the converted body (`aligned`
wrapping when `&`/`\` occur; a numbered equation keeps `equation` around it); an equation with `#`
code gets the `\text{[typst math: <source escaped>]}` body and its one `E-TYP-MATH-CODE`.

Packages: `convert` returns no package list. T60 scans every control word (`\` + the maximal run of
ASCII letters) of each emitted math string, a latex-dialect source as written or `convert`'s output,
through `typst_to_latex::command_package` (§3.3; latex-output.md §4.8), and each package it returns
(`mathrsfs`, `mathtools`, `cancel`, `stmaryrd`, `esint`, `mathdots`, `gensymb`, `eurosym`, `bm`, or
any further `[packages]` entry T51 commits) adds `\usepackage{<package>}` to the generated
preamble (deduplicated, first-use order; the brief's "only the packages the body uses"); each
`char_command(c).package` of a §7.1 `\newunicodechar` entry joins the same list. Each package that a typst-dialect equation
adds raises one `math-latex-package` info note (§9) on that equation's span: nothing is lost, so it
is not a warning, and neither `--strict` nor `[typst] strict` escalates it (it is not a `W-TYP-*`
code).

## 8. Numbering, labels, tags

- Resolve fills `Block::Math.number` per `[numbering] equations` (`labelled` default: only
  equations with `attrs.id`; `all`; `none`) and the document/section scope of references.md; the
  class `unnumbered` on a display equation keeps `number: None` under every policy
  (references.md); `Inline::Math` is never numbered. Emitters print `(number)` verbatim and
  never recompute: HTML `span.equation-number`, DOCX the right table cell, LaTeX `equation`
  (LaTeX numbers it itself; `[numbering] equations = "all"` → `equation` for every display
  equation, `none` → `\[ \]` always; a labelled unnumbered equation keeps
  `\phantomsection\label{<resolve::anchor(label)>}` on the line before it (§7.1, latex-output.md
  §4.8), so `@eq-x` to it, which references.md §4.5 resolves with an `unnumbered-target` warning,
  still links with `\hyperref` (latex-output.md §4.10)).
- PDF: the `@writer/doc` package implements the same policy (`set math.equation(numbering:
  "(1)")` for `all`; for `labelled` a show rule that removes the number from unlabelled block
  equations and rewinds the counter; per-section resets at level-1 headings, R5 §5.2), so
  T32's oracle test compares `writer dump` numbers with the compiled `writer-oracle-<label>`
  metadata. For typst documents, `set math.equation(numbering: …)` maps to `[numbering]
  equations = "all"` (typst-dialect.md §3).
- References: `@eq-x` → `Equation 1` (supplement + number, typst's rule; R5 §4.1), DOCX `REF`
  to the bookmark around the number, LaTeX `Equation~\eqref{…}` / `\cref` per latex-output.md.
- `\label`/`\tag` in LaTeX sources: §2 rule 2 (stripped + `math-tag-ignored`) for HTML (the
  default KaTeX route included), DOCX, PDF; kept for LaTeX. `\tag` therefore never produces a
  visible number outside LaTeX; the documented way to number an equation is a label plus
  `[numbering] equations`.

## 9. Diagnostics

| code | level | raised by | message shape |
|---|---|---|---|
| `math-syntax` | warning (cli.md's `--strict` list does not include it) | math-core failure (HTML `mathml`, DOCX; `check` for every LaTeX equation regardless of route, message says "KaTeX may still render it" when the route is `katex`) | `invalid LaTeX math: <LatexError::error_message()> at bytes a..b of the equation` |
| `math-conversion-failed` | warning (`--strict`: error) | OMML `Unsupported`, rendition failure, mitex pre-flight (emitter and `check`), typst MathML failure (an ignored construct included), a math character without a LaTeX mapping (§7.1) | `<target>: cannot convert equation (<reason>); used <fallback>` |
| `math-tag-ignored` | warning | `latex_source::prepare`/`for_typst` removed something on a non-LaTeX target | `\tag/\label inside math are ignored for <target>; label the equation with {#eq-x}` |
| `math-macro-invalid` | warning | `latex_mathml::macro_diagnostics` (once per document per emitter using math-core, and `check`) | `[math] katex_options.macros["\RR"]: <message>` |
| `math-env-label` | warning (`check`) | §7.1: a labelled multi-line display environment | `\label applies to the first line of the align environment` |
| `W-TYP-MATH-FN-UNKNOWN`, `W-TYP-MATH-SYM-UNKNOWN`, `W-TYP-ARG-IGNORED` | warning | `typst_to_latex` (R2 §7.3) | per construct, with the name |
| `math-latex-package` | info; never escalated (`--strict` and `[typst] strict` raise only `W-TYP-*` and `E-TYP-MATH-CODE`) | T60's package scan (§7.2), once per package a typst-dialect equation adds | `added \usepackage{<package>} for \<command>` |
| `E-TYP-MATH-CODE` | warning; error with `[typst] strict = true` (config.md) or `--strict` | `typst_to_latex::convert` only: the LaTeX target (HTML/DOCX compile the equation, PDF is native) | `#` code inside math (R2 §7.3); the equation's one diagnostic |

Every diagnostic's `span` is the enclosing block's `SourceSpan` (ir.md §5 has no column); a
position inside the equation goes into the message as `at bytes a..b of the equation`. `check`
runs, per selected target: html(`mathml`)/docx → `macro_diagnostics` and math-core with the
configured macros, exactly as the build (+ `from_mathml` for docx, reporting what would fall
back); html(`katex`) → math-core the same way (`math-syntax` only); pdf → the mitex pre-flight on
the `for_typst` text; latex → `typst_to_latex` diagnostics and §7.1's character check; html/docx
with typst math → `equation_mathml` failures. All codes appear in `check --format json`.

## 10. Fixture corpus (`tests/fixtures/math/`)

One equation per file; line 1 is `% mode: display|inline` (`.tex`) or `// mode: display|inline`
(`.typ`); the rest is the source. Oracles are checked in next to the fixture so no test needs
pandoc or tylax: `<name>.pandoc.tex` for every typst fixture pandoc parses (it rejects `18`'s `lr((
… ])`) is generated once by T51 with pandoc 3.12.1 (`pandoc -f typst -t latex`; the command is in
`tests/fixtures/math/typst/README`) and must agree with R2 §5.1's recorded output for `17`,
`19`–`26`; `<name>.tylax.tex` exists only for `17`–`24` (R2 §5.2's run on R2's 15 equations;
tylax is never run again). R4 §7 recorded only MathML for `01`–`16`. A test that finds `pandoc`
on `PATH` additionally regenerates and compares.

`latex/` (R4 §2.4 rows 1–19, then edge cases): `01-frac` (`\frac{a}{b}`), `02-sum-limits`
(`\sum_{i=0}^{n} i`), `03-integral-dx`, `04-root-index`, `05-mathbb`, `06-left-right-frac`,
`07-pmatrix`, `08-aligned` (`\begin{aligned} a &= b + c \\ &= d \end{aligned}`), `09-text-if`,
`10-accents` (`\hat{x} \vec{v} \dot{y}`), `11-operatorname-argmax`, `12-color`,
`13-boldsymbol-mathbf`, `14-lim`, `15-def-macro`, `16-newcommand-no-arity`,
`17-newcommand-arity`, `18-unknown-command` (`\foo{x}`: `math-syntax`), `19-unbalanced-brace`
(`\frac{a}{b`: `math-syntax`), `20-cases`, `21-middle-delimiter` (`\left(\frac{a}{b}\middle|\frac{c}{d}\right)`),
`22-nary-inline` (inline `\sum_{i=1}^n x_i`), `23-underbrace-overbrace`, `24-bmatrix-vmatrix`,
`25-primes` (`f'(x) + x''`), `26-spaces` (`a\,b\;c\quad d\qquad e`), `27-tag-label` (`x = 1
\tag{3} \label{eq:t}`: stripped + `math-tag-ignored`), `28-align-env` (`\begin{align} a &= b \\ c
&= d \end{align}`: normalised), `29-unicode-source` (`α ≤ β`), `30-empty`, `31-macro-from-config`
(`\RR^n + \half{x}` with `tests/fixtures/math/macros.toml`: `[math] katex_options = { macros = {
"\\RR" = "\\mathbb{R}", "\\half" = "\\frac{#1}{2}" } }`, loaded by the tests with `Config::load`;
not named `writer.toml`, PLAN §7), `32-ce` (`\ce{H2O}`: unsupported everywhere but LaTeX),
`33-sympy-displaystyle` (`$\displaystyle \frac{1}{2}$`, delimiter stripping),
`34-empty-base-scripts` (`{}_1^2 X`: math-core's `msubsup` with an empty base → `m:sSubSup` whose
`m:e` is the U+200B run, as pandoc), `35-nested-fences-text` (`\left\{ x \in \mathbb{R} \mid
\text{if } x > 0 \right\}`), `36-left-dot-right-bar` (`\left. \frac{df}{dx} \right|_{x=0}`: `m:d`
with `begChr ""`, `endChr "|"`, as pandoc), `37-sideset` (`\sideset{_1^2}{_3^4}\sum`:
`mmultiscripts` with postscripts → `Unsupported`; mitex rejects it too).

`mathml/` (T42; hand-written MathML fed straight to `from_mathml`): `mprescripts.mml` (→ `m:sPre`),
`mlabeledtr.mml`, `mmultiscripts-post.mml`, `merror.mml` (→ `Unsupported`).

`typst/` (R4 §7's 16, then R2 §4.2 and edge cases): `01-frac`, `02-sum-attach`,
`03-integral-dif`, `04-root`, `05-bb-cal` (`RR, bb(R), cal(F)`), `06-upright`, `07-lr-frac`,
`08-mat`, `09-aligned` (`a &= b + c \ &= d`), `10-text-if`, `11-accents` (`hat(x) arrow(v) dot(y)`),
`12-op-argmax-limits`, `13-bold`, `14-lim-oo`, `15-cases`, `16-attach-primes`;
`17-primes-fcall` (`x' + x'' + f'(x)`), `18-abs-norm-lr-mismatch` (`abs(x) + norm(v) + lr(( a/b ])`),
`19-symbols-fieldaccess` (`alpha + beta + arrow.r + arrow.l.long + dot + RR + NN`), `20-vec-binom`
(`vec(1, 2, 3) + binom(n, k)`), `21-styles` (`hat(x) + tilde(y) + overline(z) + bold(v) + bb(R) +
cal(F)`), `22-text-op-lim` (`"text" + op("foo") + lim_(x -> 0)`), `23-attach-multi`
(`x_1^2 + x_(i j) + e^(i pi)`), `24-alignpoints` (`a & = b \ c & = d`), `25-lr-floor-ceil` (R2 x1),
`26-limits-scripts-overbrace` (R2 x2), `27-mat-delim-named` (`mat(delim: "[", 1, 2; 3, 4)`),
`28-hash-code` (`#text[x] + y`: MathML `<mtext>x</mtext>…` for HTML/DOCX; LaTeX: `E-TYP-MATH-CODE`
+ the fallback body), `29-unknown-fn` (`foo(x)`; the compiler: "unknown variable: foo"),
`30-unknown-sym` (`planck.reduce`; the compiler: "unknown symbol modifier"), `31-shorthands` (`a
-> b <= c != d |-> e`), `32-spacing` (`a thin b med c thick d quad e wide f`), `33-llbracket` (`[|
x |]`: `\usepackage{stmaryrd}` + the `math-latex-package` note), `34-cancel-overline`
(`cancel(x) + overline(y)`: `equation_mathml` → `Err`), `35-empty`, `36-prescripts`
(`attach(X, tl: 1, bl: 2)`: OMML `m:sPre`).

Documents: `tests/fixtures/md/math.md` (T01, wave 1, so the sources are spelled out here and T1A's
corpus files must hold the same strings): front matter `math: { katex_options: { macros: {
"\\RR": "\\mathbb{R}", "\\half": "\\frac{#1}{2}" } } }`; inline `$\frac{a}{b}$` (01); `$$\sum_{i=0}^{n}
i$$ {#eq-sum}` (02, labelled, referenced as `@eq-sum`); a ```` ```math ```` fence with
`\begin{aligned} a &= b + c \\ &= d \end{aligned}` (08); inline `$\hat{x} \vec{v} \dot{y}$` (10);
`$$x = 1 \tag{3} \label{eq:t}$$` (27); inline `$\RR^n + \half{x}$` (31).
`tests/fixtures/typ/math.typ` (T50) holds what typst-dialect.md's Fixtures list (R2 §4.2's
equations, `mat(delim: #none, …)`) plus `#mi`/`#mitex` calls.

`tests/fixtures/math/expectations.toml` (T42; T33 adds the `pdf` key) gives every fixture `docx` =
`omml` (`from_mathml` succeeds), `unsupported` (`from_mathml` → `MathError::Unsupported`, §5.3 step
2) or `no-mathml` (§5.3 step 1), and every latex fixture `pdf` = `compile` or `preflight-error`.
latex `docx`: `01`–`17`, `20`–`31`, `33`–`36` → `omml` (`27` after `strip_tags`, `28` after
`normalise_env`, `30` → the U+200B run, `31` with `macros.toml`); `37` → `unsupported`; `18`,
`19`, `32` → `no-mathml` (math-core: `Unknown command "\foo"`, `Expected closing token "}"`,
`Unknown command "\ce"`). latex `pdf`: `18`, `32`, `37` → `preflight-error`; all others → `compile`
(`19` included: mitex accepts the unbalanced brace). typst `docx`: `21`, `29`, `30`, `34` →
`no-mathml` (an ignored `overline`/`cancel`, or a compile error); all others → `omml` (`28`
included). The `mathml/` files have their own expectations in T42's tests.

## 11. Verified API hints (copy-paste; R4 §9, R2 §9, sources read 2026-10-24)

```rust
// ---- math-core 0.9.0 -------------------------------------------------------------- [verified]
use math_core::{LatexToMathML, MathCoreConfig, MathDisplay, PrettyPrint};
let conv = LatexToMathML::new(
    MathCoreConfig { pretty_print: PrettyPrint::Never, xml_namespace: true, ignore_unknown_commands: false,
                     annotation: false, ..Default::default() },
    // config key "\\RR" minus its backslash: math-core rejects "\\RR" ("Invalid macro name")
    vec![("RR".to_string(), r"\mathbb{R}".to_string())],
).map_err(|(err, index, def): (Box<math_core::LatexError>, usize, String)| /* drop macros[index], math-macro-invalid, retry */ err)?;
match conv.convert_with_local_state(r"\frac{a}{b}", MathDisplay::Block) {
    Ok(r) => { let mathml: String = r.mathml.replace("&ApplyFunction;", "&#x2061;"); /* r.warnings */ }
    Err(e) => { let msg = e.error_message(); let range = e.0.clone();
                let html = e.to_html(r"\frac{a}{b}", MathDisplay::Inline, Some("math-error")); }  // always the <span> form
}

// ---- mitex 0.2.4 (emitter + check pre-flight; output needs the package's mitex-scope) --- [verified]
let typ: Result<String, String> = mitex::convert_math(r"\frac{a}{b}", None);   // Ok("frac(a ,b )")
// mitex::convert_math(r"\foo{x}", None) == Err("error: unknown command: \\foo"); r"\frac{a}{b" → Ok("frac(a ,b )")

// ---- typst-syntax 0.15.1 + codex 0.3.0 ------------------------------------------------ [verified]
use typst_syntax::ast::{self, AstNode};
let root = typst_syntax::parse_math("x^2 + frac(1, 2)");            // same subtree as `$ x^2 + frac(1, 2) $`.body()
let math: ast::Math = root.cast().expect("Math");
for e in math.exprs() { /* R2 §9 snippet 3: MathText/MathIdent/MathFieldAccess/MathShorthand/MathAlignPoint/
                           Linebreak/Str/MathCall/MathDelimited/MathAttach/MathFrac/MathRoot/Space/Escape */ }
fn symbol(path: &str) -> Option<&'static str> {                      // "arrow.r.long" -> Some("⟶")
    let (name, mods) = path.split_once('.').unwrap_or((path, ""));
    match codex::SYM.get(name)?.def {
        codex::Def::Symbol(sym) => sym.get(codex::ModifierSet::from_raw_dotted(mods)).map(|(s, _dep)| s),
        codex::Def::Module(_) => None,
    }
}
// MathCall rows: for a in c.args().arg_items() { match a.arg { ast::Arg::Pos(e) => …, ast::Arg::Named(n) => …, ast::Arg::Spread(_) => {} }
//                                               if a.ends_in_semicolon { rows.push(vec![]); } }

// ---- typst 0.15.1: equation → SVG/PNG + baseline (World from R1 §5.2) --------------- [verified]
use typst::layout::{Abs, Frame, FrameItem, Point};
use typst_layout::PagedDocument;
const PRELUDE: &str = "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n#set text(top-edge: \"bounds\", bottom-edge: \"bounds\")\n";  // = §3.4
// main text = format!("{PRELUDE}#box(${eq}$)")             // inline; display: "#box($ {eq} $)"; latex: "#box(mi(`…`))"
// (R1 probe World's set_text; in writer: the `/__render/main.typ` scratch source of typst-dialect.md §9)
let typst::diag::Warned { output, warnings } = typst::compile::<PagedDocument>(&world);
let doc = output.map_err(|errs| /* Vec<SourceDiagnostic> */ errs)?;
let page = &doc.pages()[0];                               // typst_layout::Page { frame: Frame, fill: Smart<Option<Paint>>, … }
let (w_pt, h_pt) = (page.frame.width().to_pt(), page.frame.height().to_pt());
let baseline_pt = page.frame.items().find_map(|(pos, item)| match item {
    FrameItem::Group(g) if g.frame.has_baseline() => Some((pos.y + g.frame.baseline()).to_pt()),
    _ => None,
}).ok_or_else(|| Error::Other("equation has no baseline".into()))?;   // the box guarantees the group; never unwrap_or(h_pt)
// Frame::baseline() is measured from the top; descent = h_pt - baseline_pt. `#box($a/b$)`: 6.974 × 11.602, baseline 7.73
let svg: Vec<u8> = typst_svg::svg(page, &typst_svg::SvgOptions { render_bleed: false, pretty: false }).into_bytes();
let png: Vec<u8> = typst_render::render(page, &typst_render::RenderOptions { pixel_per_pt: 2.0.into(), render_bleed: false })
    .encode_png().expect("png");                          // tiny_skia::Pixmap; 2 px/pt is also RenderOptions::default()
// no eviction here: render helpers never evict; callers run `typst::comemo::evict(10)` once per build
// (live-reload.md §5.3 (c); `comemo` is not a direct dependency, vendored-assets.md §7.3)

// ---- typst 0.15.1: equation → MathML --------------------------------------------------- [verified]
use typst::{Feature, Features, Library, LibraryExt};
let library = Library::builder().with_features([Feature::Html].into_iter().collect::<Features>()).build();  // WriterWorld's HTML library
// main text = format!("$ {eq} $")                           // display; inline: "${eq}$"; no preamble (typst-dialect.md §9)
// (R1 probe World's set_text; in writer: the `/__render/main.typ` scratch source of typst-dialect.md §9)
let typst::diag::Warned { output, warnings } = typst::compile::<typst_html::HtmlDocument>(&world);
let html = typst_html::html(&output?, &typst_html::HtmlOptions { pretty: false })?;   // SourceResult<String>
let start = html.find("<math").unwrap(); let end = html[start..].find("</math>").unwrap() + start + 7;
let mathml = &html[start..end];                           // `<style>…</style>` in <head>: 1,494 B between the tags = EQUATION_CSS
// warnings: drop the detached "html export is under active development"; any other one ("… was ignored during MathML export") → Err.
```

```css
math { font-family: "Latin Modern Math", "STIX Two Math", "Cambria Math", "Libertinus Math", math; }
@font-face { font-family: "Latin Modern Math"; src: url("data:font/woff2;base64,…") format("woff2"); } /* mathml_font = "embed" */
```

## Fixtures

| fixture | owner | used by |
|---|---|---|
| `tests/fixtures/math/latex/*.tex`, `tests/fixtures/math/macros.toml` (the `31` macros; no `writer.toml` may sit in an ancestor of fixture directories, PLAN §7). T30's `latex_source` unit tests use inline strings: the sources of §10's `27-tag-label`, `28-align-env` and `31-macro-from-config`, and the `macros.toml` map built as a `serde_json::Map`. They never read these files. | T1A | T1A (math-core MathML snapshots, KaTeX markup snapshot, macro retry), T42 (OMML snapshots + `expectations.toml` `docx`, pandoc comparison when installed), T33 (`pdf` key: the `compile` entries emitted into one document and compiled to one PDF, page count + byte identity; each `preflight-error` entry emitted alone gives `$#raw(..)$` + one `math-conversion-failed`, and compiled directly as `#mi(..)` gives `Error::Compile`), T60 (passthrough snapshots, `29`'s `\newunicodechar` preamble) |
| `tests/fixtures/math/mathml/*.mml` | T42 | T42 (`from_mathml` on hand-written MathML; display and inline end-to-end tests asserting the picture — `wp:inline`, `descr` = the source — the `fallback_text` run and one `math-conversion-failed` naming the element, also via latex `37`) |
| `tests/fixtures/math/typst/*.typ` (+ `.pandoc.tex`, `.tylax.tex` oracles per §10) | T51 | T51 (LaTeX snapshots; oracle diff: `01`–`17`, `19`–`27` must equal the `.pandoc.tex` after deleting whitespace, `{` and `}`; exceptions are (fixture, pandoc form, writer form, reason) entries in the test, never a whole fixture, and the known ones are `\limits` after `\lim`, pandoc's `\left.`…`\right.` wrappers, `\rightarrow` vs `\to`, `d` vs `\mathrm{d}` for `dif`, `\overset{…}{\overbrace{…}}` vs `\overbrace{…}^{…}`), T52 (`equation_mathml` succeeds without warnings for every fixture whose `docx` is `omml`, HTML snapshots, the image fallback of `34`), T42 (OMML from typst MathML, folding tests, `36`) |
| `tests/fixtures/math/expectations.toml` | T42 (`pdf` key: T33, additive) | T42, T52, T33 |
| `tests/snapshots/typst_equation_css.snap`, rendition tests (`x^2 + 1/2`: 31.317 × 13.242 pt, baseline 9.447 ± 0.01, PNG 63×26; inline `a/b`: 6.974 × 11.602, baseline 7.73; display integral 65.719 × 29.159, baseline 17.334; SVG without `<text>`/`font`) | T03 | T03, T1A (`image` route), T42 (pictures) |
| `tests/fixtures/md/math.md`, `tests/fixtures/typ/math.typ` | T01, T50 | end-to-end snapshots of every emitter; the md → typ → IR round trip keeps `dialect` and gives `source == latex_source::prepare(original).0` (§6) |
| browser test (feature `browser-tests`, T25's DevTools client; `tests/browser_serve.rs` on live-reload.md's `tests/fixtures/serve/` fixture): KaTeX renders (every `[data-math-style]` element gets `data-rendered="1"`, no console errors), and a live edit leaves untouched equations' `data-rendered` nodes identical while an edited one re-renders (live-reload.md §8 item 6) | T23 | T22, T1A |

## Open points

1. T42 decides the internal MathML tree representation (the parser is fixed: `quick-xml` 0.42,
   §3.2); the public API and the mapping are fixed.
2. T51 decides the exact override list in `assets/math/unicode-latex.toml` beyond the rules in
   §3.3 (e.g. whether `arrow.r` is `\rightarrow` or `\to`); the oracle diff gates it.
3. T1A decides whether `math.js` renders synchronously on `DOMContentLoaded` or in an idle
   callback for documents with hundreds of equations; the guard semantics are fixed.
4. T42 decides the `m:mc` grouping heuristic for matrices with mixed alignments (one column
   group per run of equal alignment, or one per column).
5. T03 may cache renditions by `(source, dialect, display)` within a build; the output must not
   depend on cache hits.

## Deviations from PLAN.md

- `src/typst/html.rs` is a new T03-owned file (PLAN §3.2 lists `src/typst/{mod,world,compile,package,render}.rs`;
  typst-dialect.md lists it too); the MathML extraction needs the `Feature::Html` library and does
  not belong in `render.rs`.
- `src/math/latex_source.rs` (§3.6) is a new file owned by T30 (PLAN §3.2 lists only
  `latex_mathml.rs`, `omml.rs`, `typst_to_latex.rs` under `src/math/`): T30 (wave 3) is the first
  task that needs the shared transformations, before T1A (PLAN's wave 4); `src/math/mod.rs` gains
  `pub mod latex_source;` (additive). T1A, T33, T42 and T60 consume it, so T1A's dependency line
  becomes T03, T15, T16, T30; T1A is in wave 5 (html-output.md Deviations).
- New edge T1A → T23 (cross-spec review, 2026-10-25): the math browser test (Fixtures) is T23's
  `tests/browser_serve.rs`, not T25's (T25 runs in wave 5, before `writer serve` exists and
  beside T1A, which replaces T00's placeholder `math.js`), and it needs T1A's `math.js`. T23's
  dependency line becomes T1A, T20, T21, T22, T25; T23 stays in wave 6 (1 + max wave 5 of
  T1A/T20/T21/T25), so no wave changes. For the coordinator: PLAN §5's T23 row and its list of
  added edges gain T1A → T23, and docs/tasks/README.md must be regenerated.
- Additions to the brief's cross-spec names, none of which changes them:
  `latex_mathml::{macro_diagnostics, Converter}`, `typst_to_latex::{convert_ast, command_package}`,
  `Rendition::{descent_pt, png_size}`. `command_package` (§3.3) replaces latex-output.md §4.8's
  `MATH_PACKAGES` copy and its two T60 tests, which move to T51 (latex-output.md Deviations
  invited this).
- R2 §7.3's `W-TYP-MATH-LATEX-PKG` is renamed `math-latex-package` (§9): it is an info note, and
  typst-dialect.md §2.1, cli.md and §2 rule 4 raise every `W-TYP-*` to an error under `--strict`
  or `[typst] strict`, which would fail strict LaTeX builds on a harmless note. For the
  coordinator: latex-output.md §4.8 and §8 use the new name.
- The brief's "md→IR→typ→IR round trip must be identity modulo spans" holds for LaTeX math except
  where §2 rules 2–3 change the source: the typst emitter must hand mitex the prepared text, so after
  the round trip `source == latex_source::prepare(original).0` (tags, `\nonumber`/`\notag` and
  top-level environment names are not recoverable; `math-tag-ignored` is raised on the way).
- `E-TYP-MATH-CODE` (R2 §7.3 makes every `E-*` fail the HTML/DOCX/LaTeX exports) is raised only by
  `typst_to_latex` for the LaTeX target, at warning level unless `[typst] strict`/`--strict`
  (config.md: outside-subset constructs are warnings by default): HTML and DOCX use the compiler's
  MathML, which evaluates `#text[x]`, and a non-self-contained equation fails only its own compile
  and falls back per equation.
- `tests/fixtures/math/` (with `latex/`, `typst/`, `mathml/`, `macros.toml`,
  `expectations.toml`) is a new fixture directory (PLAN §7 lists only `md/math.md` and
  `typ/math.typ`); ownership as in Fixtures above.
- `latinmodern-math.woff2` (388,212 B, GUST Font License) is vendored for `[math] mathml_font =
  "embed"` (config.md names it); it is not in PLAN §1's vendored list and must be added to
  vendored-assets.md (T00) with its license file.
- R4 §4.4 recommended server-side `katex-rs`; the plan's binding choice (client-side KaTeX 0.19.1)
  stands. `katex-rs` is not a dependency.
