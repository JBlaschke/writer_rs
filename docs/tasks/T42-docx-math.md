# T42 — DOCX math: editable OMML, numbered equations and image recovery

Preparation only, 2026-10-28. No tracked implementation, Cargo, Git or formatting
has run for this brief. Public contracts below are frozen by the specifications;
private seams and implementation choices are recommendations for root to freeze.

| | |
|---|---|
| Milestone / size | M4 / M; execute after T51, before T33/T43/T52 |
| Formal dependencies | T03, T1A, T41, T51; all implemented on the feature stack |
| Primary ownership | `src/math/omml.rs`, `src/docx/math.rs` |
| New fixtures | `tests/fixtures/math/mathml/*.mml`, `math/expectations.toml`, `docx/ir/math.json` |
| Suggested tests | `tests/omml.rs`, `tests/docx_math.rs`; existing DOCX snapshot harness picks up new IR |
| Binding sources | PLAN T42/§7; math.md §3.2, §5, §9–11; docx-output.md §1.1, §4.11, §4.15–16, Fixtures |

## Goal and boundaries

Replace both empty/stub math modules. Convert math-core or Typst compiler MathML
to editable Word equations. Preserve every equation through the specified text
or picture recovery when conversion fails. Numbered display equations use the
existing reference/bookmark/SEQ machinery and a fixed three-column frame.

T42 never routes DOCX through T51's Typst→LaTeX converter. Typst DOCX stays native
compiler MathML→OMML, so `#text[x]` is valid. T51 is a fixture dependency, supplying
the complete 36-equation Typst corpus. Leave `.typ` frontend/build dispatch T52,
check-target reporting T71, PDF corpus/preflight T33, reference-document eject
and manual Word baseline validation T43, and settings.xml T04 alone. Do not add
dependencies, public module exports, a World, an XML transform executable or a
third-party OMML converter.

## Frozen APIs and usable existing seams

`src/math/omml.rs` adds only:

```rust
pub fn from_mathml(mathml: &str) -> Result<String, MathError>;
pub fn fallback_text(mathml: &str) -> String;
```

Import `MathError` from `crate::math::latex_mathml`; it already has
`Unsupported { element: String, detail: String }`. The detail is offending markup
of at most 80 characters. Conversion always produces exactly one
`<m:oMath>…</m:oMath>`, regardless of the input display mode. Only `m:`/`w:`
prefixes; no `xmlns` in the returned fragment. Unknown/malformed foreign markup
must return Unsupported, never panic. `fallback_text` always returns a string.

Existing frozen emitter entry points stay unchanged:

```rust
pub(crate) fn math_inline(&mut self, source: &str, dialect: MathDialect);
pub(crate) fn math_block(&mut self, b: &Block);
```

Useful existing code:

- `math::latex_source::{prepare, macros_from_config, for_typst}` and
  `latex_mathml::{to_mathml, macro_diagnostics}` are implemented. LaTeX MathML
  uses prepare(source); every equation rendition uses for_typst(source, macros)
  for LaTeX and unchanged source for Typst.
- `docx::Renderer::{equation, typst_mathml}` is implemented by the session's
  WorldRenderer. Both borrow the session World. Fake renderers support isolated
  tests. `Picture` has PNG, optional SVG, point dimensions and top-to-baseline.
- `Emitter::picture_run(key, &picture, alt, baseline)` already writes SVG+PNG
  media/relationships, scales by reference Normal size/11, clamps to available
  content bounds, and lowers inline pictures by the scaled descent. Reuse it;
  do not duplicate drawing XML. `baseline` is true only for forced inline image
  mode. `alt`/`wp:docPr descr` is the original source.
- `Emitter::{start_bookmark, end_bookmark, sequence_number}` already exists.
  `sequence_number("Equation", number)` handles literal chapter prefixes,
  `\s 1`, `\r n` correction, cached number and shared counter tracking.
  Effective section policy is figures=section AND heading numbering enabled,
  exactly as figures; equations=labelled/all/none selects which are numbered.
- `available_width_twips()` respects nested table cells. Use it for frame grid
  width instead of blindly reading document layout. A frame must be separated
  from adjacent tables using existing block/table helpers as appropriate.
- `content_warning(code, message)` sets inherited source span and path.
  `content.span` is maintained by the body walker, including footnote parts.
  `start_paragraph`, `end_paragraph`, `buffer`, XML escaping/property builders,
  style-name resolution and custom-style container state already exist.
- `body::inline_paragraph` already appends U+200B when the paragraph is exactly
  one Inline::Math. Do not add a second trailing run. No per-run math font;
  settings.xml already contains Word's math settings.
- Add a private per-document macro/state field to Emitter if helpful, initialise
  it once in prepare_emitter, and append macro_diagnostics once per document
  using math-core (default OMML route). Do not repeat warnings per equation.

## Recommended private split before delegation

Root owns DOCX integration, frame, diagnostics and small shared-state wiring.
One agent can own the standalone OMML converter and folding/flattening. A second
agent can own independent corpus/expected outcomes and fake-renderer integration
tests, plus actual pandoc preparation. Freeze only narrow private seams first.

For converter size, private children under `math/omml/` are reasonable, e.g.
`parse`, `render`, `text`; no new public modules. Recommended private tree:
element local name, ordered attributes, child nodes, decoded leaf text and
source range for error details. Preserve source order, whitespace in mtext/ms,
and root inline/display state. Do not trim all text globally. Build/normalise once
and share it between rendering and fallback, with a best-effort parse path for
fallback's total API. Make nested parsing/rendering bounded or iterative to
satisfy “never panics on foreign markup”; root should freeze any practical cap.

Existing `docx::validate::{parse_xml, XmlNode}` is crate-private and demonstrates
quick-xml's well-formedness/entity checks, but is tied to DOCX parsing and keeps
direct text separately from child order. Prefer a small math-owned parser when
namespace/local-name handling or tolerant flattening requires it. Do not turn a
DOCX validator refactor into T42 scope.

## Parse and normalise in the specified order

1. Decode numeric and the five XML named entities. Reject every other named
   entity with Unsupported naming `entity &foo;`. No DTD/entity expansion.
2. Semantics takes its first child; ignore annotations. mstyle/mpadded/maction
   become mrow; silently drop colour/style, never emit w:color.
3. Drop mspace without width or with non-positive width (including style-only
   negative spacing). Positive widths use the fixed Unicode spaces below.
4. Identify empty fence mo (empty text or U+2063) only at an mrow edge paired
   with another fence or empty fence. Drop other mo U+2061/U+2062/U+2063.
5. Unwrap single-child mrow, then strip U+FE00/U+FE01. Fold math alphanumerics
   through the codex reverse table, including Letterlike holes, into base text
   plus Word style/script properties.

Unknown elements, mfenced, mglyph, merror, mlabeledtr and mtd rowspan/columnspan
are Unsupported. Validate structural arities and table shape; foreign malformed
inputs must never index unchecked or panic. mo attributes except stretchy and
movablelimits are ignored (largeop/minsize/maxsize etc. do not change mapping).

## Complete OMML mapping checklist

Apply math.md §3.2 rows in order; the more specific cases precede generic scripts.

- mi single letter: default italic run; uppercase Greek and mathvariant=normal
  use sty p. Multi-letter identifiers use sty p, never m:func. mn/mo use plain
  sty p. Multi-character mo uses m:box with opEmu around the run.
- mtext/ms: m:nor; preserve edge spaces with xml:space. ms adds quotes.
- Positive mspace width Xem: U+2009 at ≤.17, U+2005 at ≤.23, U+2004 at ≤.5,
  U+2001 at ≤1.8, otherwise two U+2001. Height/depth are ignored.
- mfrac→m:f with type bar; linethickness=0→noBar. msqrt→m:rad with degHide on
  and empty degree; mroot swaps index/base into deg/e.
- msub/msup/msubsup→sSub/sSup/sSubSup unless n-ary (below).
- Stretchy over/underline accents U+203E/00AF/0332/0305/_→m:bar top/bot.
  Braces/brackets ⏞⏟⎴⎵→m:groupChr with fixed chr/pos/vertJc; outer labels become
  limLow/limUpp. Other mover accent=true→m:acc with spacing→combining map from
  §3.2 (bar U+0304, vector U+20D7, hat U+0302, dot U+0307 etc.).
- Other munder/mover/munderover→limLow/limUpp; nested order is
  limLow(limUpp(base, over), under). Inline math with movablelimits base uses
  sub/sup instead. N-ary takes precedence.
- Fence mrow→m:d with begChr/sepChr/endChr/grow. Empty fence is empty attr.
  Both ends must not have stretchy=false. Inner stretchy |/‖/∣ splits m:e parts
  and sets sepChr; stretchy=false fence pairs remain plain runs.
- Aligned mtable (class aligned/multiline-equation/menv-alignlike, or every row
  alternates mtd right/left styles)→m:eqArr; & run marks each alignment point,
  first cell right-aligned. Other tables→m:m with baseJc center/plcHide on/mcs,
  m:mr/m:e cells. Read text-align styles and left-align/right-align classes.
  Prefer one m:mc group per consecutive equal alignment; one per column is also
  permitted by the spec. Cases class or left-brace+table→delimiter around matrix.
- mphantom→m:phant show off. menclose→borderBox, including the specified hidden
  sides and diagonal strike properties for updiagonalstrike.
- mmultiscripts supports exactly one mprescripts pair and no real postscripts;
  pairs of empty mrow count as none. Emit sPre with sub/sup/e. All other shapes
  are Unsupported (latex sideset fixture37 has real postscripts).
- Empty math/mrow/cell/script emits U+200B run, avoiding Word's dashed box.
- N-ary operators ∫∬∭∮∯∰∏∐∑ become m:nary, sub/supHide flags and limLoc
  undOvr or subSup as appropriate. **Consume exactly the next sibling as body**,
  with U+200B if absent. Bare integral also gets nary with both limits hidden.
  The sequence renderer must consume that sibling once, not duplicate it.

Folding uses one LazyLock reverse map generated by
`codex::styling::to_style(base, MathStyle).next()` over A–Z/a–z/Α–Ω/α–ω/0–9,
all 13 styles in math.md §5.2. Strip selectors first. Preserve base/style
ambiguities deterministically (do not let unsupported identity results overwrite
real styled entries). Required mappings: Bold sty b; Italic default; BoldItalic
bi; Script scr=script+p; BoldScript script+b; Fraktur fraktur+p; BoldFraktur
fraktur+b; DoubleStruck double-struck+p; Sans and its variants sans-serif with
p/b/i/bi; Monospace monospace+p. Include Letterlike ℝ/ℂ/ℱ/ℎ etc. No raw styled
alphanumeric survives in m:t when it has a reverse entry.

## DOCX route, fallbacks and placement

Default OMML path obtains prepared LaTeX MathML or native Typst MathML. It never
evaluates Typst math via the T51 converter. Preserve success as editable OMML.

| Failure/mode | Inline | Display | Diagnostic |
|---|---|---|---|
| LaTeX MathML fails | original source, Verbatim Char | original source in centred Source Code paragraph | math-syntax |
| Typst MathML fails | source, Verbatim Char | attempt rendition picture | math-conversion-failed |
| OMML Unsupported | plain fallback_text(MathML) run | attempt rendition picture | math-conversion-failed naming element |
| Rendition also fails | source, Verbatim Char | source in centred Source Code paragraph | math-conversion-failed |
| Forced image | picture with baseline=true | picture with baseline=false | only failure warns |

For each fallback transition preserve the actual failure reason and final form;
do not append duplicate warnings with misleading intermediate fallback claims.
Diagnostic code/level/message is math.md §9: math-syntax is a warning and is not
in the CLI strict list (the older docx-output diagnostics row says otherwise;
the binding fallback section explicitly defers to math.md). Conversion failures
are warning initially, escalated by pipeline strict handling. Tag/label stripping
raises math-tag-ignored; no lost inline source. Attach enclosing span/path.

Inline OMML is directly beside w:r inside w:p. Unnumbered display gets Equation
paragraph and oMathPara with jc center. Numbered display gets only the fixed
borderless three-column table from docx-output.md §4.15: pct widths500/4000/500,
grid tw/10, tw−2*(tw/10), tw/10; cantSplit; vAlign center; Compact empty left;
Equation middle; Equation/right-aligned right. Middle contains oMathPara,
picture or Source Code recovery as applicable. Parenthesis runs surround a
bookmark around **number only**, then sequence_number("Equation", number).
Do not bookmark parentheses. Use attrs.id only for label, number from resolved
IR, and existing custom-style Div handling; arbitrary math attrs are ignored.

Fallback_text deterministic flattening: folded leaf mi/mn/mo/mtext; sub/sup
_s/^s (parenthesise multi-token s); sub then sup; fraction (num)/(den); sqrt √(…);
mroot (index)√(…); under/over base_(…)/base^(…); table cells ', ', rows '; ';
mspace one space; fences retained. No escape sequences or OMML markup in text.

## Acceptance and fixture inventory

1. Full 37 LaTeX corpus through prepare + configured macros (fixture31 only):
   docx omml for01–17,20–31,33–36; unsupported37; no-mathml18/19/32. Every
   successful fragment snapshot and editable shape assertion. Tag27/env28/
   empty30/source macros31 must use existing shared preparation, not forks.
2. Full 36 Typst corpus through real equation_mathml: no-mathml21/29/30/34;
   all others omml, including hash28 and sPre36. Real compiler tests preserve
   session World and use embedded fonts. Expectation file stores docx for every
   latex/typst fixture; T33 will later add pdf keys, so do not own them now.
3. Hand-authored mathml fixtures mprescripts (sPre), mlabeledtr,
   mmultiscripts-post, merror (Unsupported), plus focused tests for every
   mapping and normalisation above. Unknown entity, malformed arity/XML,
   foreign root/namespace, numeric entities, CDATA, deeply nested and mixed
   content must not panic or silently drop content.
4. Folding tests cover every generated style/base family and Letterlike holes,
   selectors, uppercase Greek, mathvariant normal, multi-letter plain runs,
   bold upright b versus bi, spaces, accents and no font/color attrs.
5. Fake-renderer DOCX tests: both dialects, inline/display, OMML/image modes,
   every failure cascade, exact call prepared sources, display bool, call counts,
   once-only macro diagnostics, span/path, original description and text.
6. Picture assertions: wp:inline, SVG part plus PNG blip fallback, EMU size and
   normal-reference scale, forced-inline position = negative rounded twice
   scaled descent, no inline pictures on default unsupported path, zero duplicate
   warnings and exact final recovery when renderer also fails. Check footnotes
   use their own rels/media and IDs stay unique.
7. Numbered frame tests: 10/80/10 widths and XML order, numbered recovery in
   middle cell, label and REF target, Equation SEQ cached number/section policy/
   corrections, no-number case, nested cell available width, custom-style Div.
8. Add resolved docx/ir/math.json. Existing docx_snapshots.rs auto-discovers it,
   validates all parts, and does independent docx-rs readback. Regenerate only
   reviewed snapshots whose stub math changed, plus the new fixture's parts.
   Run real pipeline builds for math.md, typ/math.typ library-route as available,
   mixed-target independence, byte determinism and validator.
9. When pandoc exists, extract its OMML from generated DOCX for shared LaTeX
   shapes, normalise XML/property/run differences narrowly and compare structure.
   Document the two intentional styling differences: writer bold-upright sty b
   where pandoc uses bi, and bar accent U+0304 where pandoc uses U+203E. The
   exact observed representation aliases are recorded below. Do not copy
   GPL texmath source. Pandoc is an optional test oracle only.
10. Root runs focused units/integrations, all debug/release tests, configured
    rustfmt, all-target/all-feature Clippy and denied-warning docs; update task
    board and build-time measurements. No browser QA or screenshot requirement.

## Available source/API facts (read locally, not inferred)

- quick-xml0.42 is already a normal dependency. `Reader::from_str`, read_event,
  Start/Empty/End/Text/CData/GeneralRef/Decl/DocType/PI/Comment/Eof are available.
  **Entity references are separate Event::GeneralRef events in this version**;
  BytesRef::resolve_char_ref handles numeric references, not named XML entities.
  Attributes normalized_value(XmlVersion::Implicit1_0) handles decoding and
  duplicate checks are available through attributes().with_checks(true).
  NsReader::read_resolved_event is available if resolving default/prefixed
  namespaces is easier than a local namespace stack. No need to add serde or
  encoding crate features just for this parser.
- codex0.3.0 has default styling feature enabled. `MathStyle` and public
  `to_style(c, style)->ToStyle` are in `codex::styling`; private conversion helpers
  are not callable. Unicode version in that module is17.0.0.
- DOCX XML escaper/property builders are in `docx::xml` (inside mod.rs);
  RunProps already has position. Existing math namespace declarations and mathPr
  are installed by package construction. Do not duplicate either.
- Existing tests/latex_mathml.rs demonstrates corpus mode parsing/macros31 and
  already verifies math-core failures18/19/32. tests/docx_images.rs demonstrates
  fake renderers, ZIP part extraction, attribute checks and package validation.

## Decisions for coordinator freeze

Only private representation/split, bounded foreign-markup behaviour, exact
fallback handling for malformed XML, matrix grouping and test normalisation
need root choices. The mapping, diagnostic taxonomy, fallback cascade, public
signatures, source preparation and picture baseline formula are fixed.


## Coordinator freeze and installed private seams

Private `omml/{parse,fold,render,text}.rs` children back the two frozen public
functions. The math-owned parser preserves mixed content and accepts absent or
MathML element namespaces, rejecting foreign XML/DTD/entities. Iterative parse
bounds are 8 MiB input, 100,000 nodes and 256 element levels; source excerpts
retain at most 80 Unicode characters. Unsupported markup uses the existing
MathError variant. Tolerant text recovery is total and never expands entities.
Renderer normalization operates separately from fallback flattening, so dropped
nonpositive/widthless spaces still flatten as one source space. Matrix columns
group consecutive equal alignments. Generic menclose follows the specified
borderBox approximation; unrepresentable positive non-em spacing is Unsupported.

One private per-document DOCX math state holds the configured macros, the
once-only math-core diagnostic flag and an explicit custom-Div style override.
Ordinary quote/callout/abstract/definition styles do not override Equation.
Numbered frame paragraphs have their own cell context, avoiding outer list
indentation; picture width is scoped to the middle cell and restored afterward.
The existing picture, bookmark, table-boundary and sequence builders are reused.
The content pre-pass now reserves math labels for forward references. No World,
public module, dependency, configuration or IR schema is added.


## Recorded oracle refinements

The two intentional styling decisions above remain `\mathbf{x}` as upright
bold (`b` versus Pandoc `bi`) and the bar accent U+0304 versus U+203E. The T42
comparison also records the following **observed representation aliases** from
21 actual Pandoc 3.12.1 probes, executed with
`pandoc -f markdown+tex_math_dollars -t docx`. Every probe exited successfully
and contained one `m:oMath`; exact inputs/commands/results were retained in the
T42 preparation artifacts. These refine the prior assertion that there were
only two comparison exceptions. They change comparison only, not Writer output.

| Exact fixture/form | Comparison alias and reason |
|---|---|
| `03-integral-dx`, standalone U+2009 run | Default versus plain p space style; after this alias, ordinary adjacent equal-style runs merge. No whitespace character is removed or changed. |
| `08-aligned`, row two beginning with U+200B then alignment `&` | The required empty-cell placeholder is absent from Pandoc; only that first run is compared without U+200B. |
| `09-text-if`, `20-cases`, `35-nested-fences-text`, exact normal-text `if` followed by NBSP | math-core's literal NBSP is compared with Pandoc's ASCII text space. Production preserves the MathML text exactly. |
| `14-lim`, standalone infinity run | math-core's mi infinity has default style; Pandoc writes plain p. Other identifier styles are compared unchanged. |
| `23-underbrace-overbrace`, exact lower-limit node whose whole limit is one plain U+23DF run | Pandoc's nested limLow underbrace is compared with Writer's required groupChr(chr=U+23DF,pos=bot,vertJc=top). The complete brace body, outer label and all other limits remain compared. This is an additional observed structural oracle exception. |
| `24-bmatrix-vmatrix`, begChr/endChr U+2223 | Pandoc's delimiter character is compared with Writer's U+007C. Ordinary relation text and separator attributes are unchanged. |

Aliases are restricted to the named fixture and exact form. The comparator
retains all other nodes, attributes, characters, delimiters and limits; negative
regressions enforce that unrelated names, nearly matching text, added
attributes, ordinary vertical-bar relations, separator characters and changed
outer labels still differ. No fixture or construct is exempted from comparison.

Fixture35's inner `\mid` separator was a production bug, not an oracle alias:
its bare `<mo>∣</mo>` must stay an ordinary interior relation. Only explicit
`stretchy="true"` inner bars (such as `\middle`) split delimiter parts. Outer
fence ends continue to allow omitted stretchy, as specified by §3.2.


## Final acceptance

Formatting, all-target/all-feature Clippy and denied-warning default/feature
rustdoc pass. Complete suites pass 1,475 debug and 1,474 release tests. All 14
private units, 44 focused public/package tests and 20 package snapshot tests
pass. All 73 OMML and 19 DOCX package goldens were reviewed independently.
All 21 actual Pandoc probes pass the exact documented comparison. See
build-times.md for timings, complete route coverage and final repairs.
