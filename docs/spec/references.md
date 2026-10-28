# Spec: Labels, numbering, cross-references, citations, footnotes and the table of contents

**Status:** contract. Implemented by T17 (`src/resolve/{mod,labels,numbering,footnotes,toc}.rs`:
§1, §2, §4, §8), T01 (`src/resolve/l10n.rs` and `assets/l10n/en.toml`: §4.5's localisation items,
§4.1) and T70 (`src/cite/{mod,library,render}.rs`: §5). T17 writes
the whole walk of §4, including §4.4 (with the stub's empty `keys` every key branch is inactive),
the §5.4 merge and sub-groups, the note branch of phase 3(c) (inactive while `is_note_style()` is false); T70
replaces the stub bodies, implements phase 4, §5.5's footnote construction and §5.6, and owns the
tests of every key and note branch (the "resolve hook" of PLAN §3.2: it may edit `src/resolve/`
for them). Consumers: every frontend (label sources §1.2), T30/T31 (typst emission §3, §6),
T13/T15 (HTML §6), T40/T41 and T60/T61 (names of §1.4, §6), T03/T32 (numbering oracle §7),
T1C/T71 (`check`, §8), the fixtures of the last section.

The resolve pass turns the syntactic shapes the frontends emit (ir.md §1, markdown-dialect.md §7)
into a document every emitter prints without further thought: numbers on headings, figures and
equations, resolved `Ref`/`Cite` inlines, footnote numbers, bibliography entries and the TOC.
Rules: one pass per build, never fails (problems are diagnostics), deterministic, and **typst
parity by construction**: every numbering rule below is copied from typst 0.15.1's sources
(`typst-library/src/model/{numbering,figure,reference,bibliography}.rs`,
`introspection/counter.rs`) and was checked with the embedded typst 0.15.1 compiler (probe
crates; there is no typst CLI here) on 2026-10-25, so HTML/DOCX (writer's numbers) and PDF
(typst's counters driven by §3) agree. LaTeX numbers and cites itself (§6).

## 1. Labels — T17 (`resolve/labels.rs`)

### 1.1 Alphabet
A label is a non-empty string of typst's label alphabet `[\p{XID_Continue}_\-:.]+`, tested with
`typst_syntax::is_valid_label_literal_id(&str)` (typst-syntax 0.15.1, exported; `fig:x.y-z_1`
and `größe` pass, `a b` and `a/b` fail). Convention `kind-name` (`sec-`, `fig-`, `tbl-`, `lst-`,
`eq-`, `fn-`); `fig:x` is accepted. Frontends reject invalid ids (`label-invalid`,
markdown-dialect.md §3.3); resolve re-checks every id from every frontend and drops an invalid
one with the same error.

### 1.2 Sources and kinds
| IR element carrying the label | source syntax | `LabelKind` |
|---|---|---|
| `Heading.attrs.id` | `# T {#sec-x}` · notebook markdown cells · typst `= T <sec-x>` · auto id (§1.3) | `Heading` |
| `Figure{kind: Image \| Other}.attrs.id` | implicit figure `{#fig-x}`, `:::figure`, notebook `#\| label:`, typst `#figure(..) <fig-x>` | `Figure` |
| `Figure{kind: Table}.attrs.id` | `Table: … {#tbl-x}`, typst `#figure(table(..))` | `Table` |
| `Figure{kind: Listing}.attrs.id` | ```` ```py {#lst-x} ````, typst `#figure(raw)` | `Listing` |
| `Block::Math.attrs.id` | `$$…$$ {#eq-x}`, ```` ```math {#eq-x} ````, typst `$ … $ <eq-x>` | `Equation` |
| `Inline::Footnote.label` from a **typst** source | typst `#footnote[..] <fn-x>` | `Footnote` |
| any other `attrs.id` (`Div`, `Callout`, `Cell`, bare `Table`, `CodeBlock`, inline `Code`/`Link`/`Image`/`Span`) | `{#x}`, `:::name {#x}`, typst | `Other` |

Markdown footnote names (`[^note]`) are **not** labels: they bind per file (markdown-dialect.md
§8). The **scope** of an inline is the file of the nearest enclosing block with a span
(`SourceSpan.file`: `Some(p)` → `p`; `None`, or no span at all → `Document.source.path`); its kind
is `ir::detect_kind(&p).ok()` for an included `p`, `Some(document.source.kind)` for the main file.
A `Footnote.label` is a label iff that kind is `Some(SourceKind::Typst)`.

### 1.3 Uniqueness, reserved names, auto identifiers
- Labels are global across includes. A second definition is `duplicate-label` (error) at the
  second site with a note naming the first; the first wins.
- Reserved for generated anchors (§6): `refs`, and labels starting with `ref-`, `writer-`,
  `fn-<digits>` or `fnref-<digits>` (regex `^(refs|ref-.*|writer-.*|fn(?:ref)?-[0-9]+(-[0-9]+)?)$`).
  An explicit label matching it is the warning `label-reserved` (kept; HTML ids may clash).
- Every `Heading` without `attrs.id` (also `.unnumbered`/`.notoc`/abstract headings) gets an
  **auto identifier**, written into `attrs.id` and registered as an ordinary `Heading` label
  (referenceable, emitted everywhere). Algorithm (pandoc's `auto_identifiers` within the label
  alphabet): take `label_text(inlines)` (§4.5); lowercase; keep whitespace and each `c` with
  `(c.is_alphanumeric() && typst_syntax::is_id_continue(c)) || matches!(c, '_' | '-' | '.')`, drop
  the rest; join the whitespace-separated words with `-`; drop everything before the first
  `char::is_alphabetic` character; empty → `section`. If the result is an explicit label, a
  library key, reserved, or an earlier auto id, append `-1`, `-2`, … (first free among explicit labels,
  keys and earlier auto ids; reservedness is checked only on the base). Examples
  (pandoc 3.12.1 agrees): `1.2 GPU Allocation: Results!` → `gpu-allocation-results`; `A & B` →
  `a-b`; `a·b` → `ab`; `Intro` twice → `intro`, `intro-1`; `42` → `section`.

> **Contract change (coordinator, 2026-10-26):** the reserved regex now matches
> `fn-1` and `fnref-1` as intended; the former `fnref?` typo matched `fnre-1`
> instead. Auto-id suffix candidates avoid explicit labels, library keys and
> earlier auto ids, while reservedness applies only to the initial base. This
> makes repeated headings such as "Writer" terminate as `writer`, then `writer-1`.

### 1.4 `resolve::anchor` — DOCX bookmark names and LaTeX `\label` keys
```rust
/// pandoc's toBookmarkName (Writers/Docx/OpenXML.hs), ASCII-restricted. Cargo: sha1_smol = "1.0.1".
pub fn anchor(label: &str) -> String {
    if !label.is_empty() && label.len() <= 39
        && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        format!("_{label}")
    } else {
        let hex = sha1_smol::Sha1::from(label.as_bytes()).digest().to_string(); // 40 lowercase hex
        format!("_{}", &hex[1..])                                                // 39 digits → 40 chars
    }
}
```
HTML uses the label verbatim (`id`, `href="#label"`); typst uses it verbatim (`<label>`).
Examples (ASCII rows byte-identical to pandoc 3.12.1's bookmarks; unit test table):

| label | anchor |
|---|---|
| `Fig_1` · `9lives` · `_x` | `_Fig_1` · `_9lives` · `__x` |
| `abcdefghijklmnopqrstuvwxyzabcdefghijklm` (39) | `_abcdefghijklmnopqrstuvwxyzabcdefghijklm` |
| `abcdefghijklmnopqrstuvwxyzabcdefghijklmn` (40) | `_40e479dd08aa3c669b21f890ab9ddf75a57eedc` |
| `sec-intro` | `_fdabef51864cece78e14c461e4ab265d0b74de7` |
| `fig:plot` | `_fd15c33cfa7216ea070647016ea45ecd9b6a972` |
| `eq.x` | `_19d18211de2593daa5a6036a9854b2fbe806b4e` |
| `größe` | `_3c6a3acf1b4a1beba25d8a45a3ccd719721538b` (pandoc keeps `_größe`) |

Collisions (a 39-hex-digit label equal to another label's hash tail): two labels with the same
anchor are the error `anchor-collision` naming both; the later one gets no bookmark/`\label`.

## 2. Numbering policy — T17 (`resolve/numbering.rs`)

Configuration is config.md's `[numbering]` (`HeadingNumbering::{Off, Pattern}`, `true` ≡ `"1.1"`;
`FigureNumbering::{Document, Section}`; `EquationNumbering::{Labelled, All, None}`).

### 2.1 Numbering patterns (typst's `NumberingPattern`, exact port)
```rust
pub struct NumberingPattern { pieces: Vec<(String, codex::numeral_systems::NamedNumeralSystem)>, suffix: String }
impl NumberingPattern {
    /// typst's FromStr: every char for which `NamedNumeralSystem::from_shorthand` is Some is a counting
    /// symbol; the text before it (since the previous symbol) is its prefix; the rest is the suffix.
    /// None when there is no counting symbol (`numbering-invalid`).
    pub fn parse(pattern: &str) -> Option<NumberingPattern>;
    /// Display form (`trimmed = false`) or reference form (`trimmed = true`), plus
    /// the numbers that could not be represented (each a `numbering-unrepresentable` warning).
    pub fn apply(&self, numbers: &[u64], trimmed: bool) -> (String, Vec<u64>);
}
```
`apply`: for `i`, `((prefix, sys), n)` in `pieces.zip(numbers)`: push `prefix` unless `i == 0 &&
trimmed`, push `repr(sys, n)`; for each remaining number `n` (pieces exhausted), with `(lp, ls)` =
the last piece: push `suffix` if `lp` is empty else `lp`, then `repr(ls, n)`; finally push
`suffix` unless `trimmed`. `repr` = `sys.system().represent(n)`; on `Err` (zero in latin/roman,
too large) use `n.to_string()` (typst's fallback). codex 0.3.0 is the crate typst 0.15.1 uses, so
all typst shorthands work; `1 a A i I` are the documented, tested ones. Verified examples:

| pattern | numbers | display | reference (trimmed) |
|---|---|---|---|
| `1.1` | `[1]` · `[1,2]` | `1` · `1.2` | `1` · `1.2` |
| `1.` | `[1]` · `[1,1,1]` | `1.` · `1.1.1.` | `1` · `1.1.1` |
| `A.1` | `[1]` · `[1,0,1]` | `A` · `A.0.1` | `A` · `A.0.1` |
| `I.` | `[1,1]` | `I.I.` | `I.I` |
| `1.a` | `[2]` · `[2,0,1]` | `2` · `2.0.a` (+ warning) | `2` · `2.0.a` |
| `(i)` | `[2]` · `[2,1]` | `(ii)` · `(ii(i)` | `ii` · `ii(i` |
| `(1)` | `[3]` | `(3)` | `3` |

### 2.2 Headings
A heading is **numbered** iff headings numbering is `Pattern(p)`, it lacks the class
`unnumbered`, and it is not inside `DocMeta.abstract_`. Only numbered headings touch the heading
counter (typst `Count for HeadingElem`). Counter state `Vec<u64>`, stepped exactly like typst's
`CounterState::step`: for level `L`, push `0` while `len < L`, then `state[L-1] += 1`, then
`truncate(L)`. `Heading.number = Some(p.apply(&state, false).0)`; unnumbered → `None`. typst
mapping (both directions; typst-dialect.md, Deviation 9): class `unnumbered` ↔ `#heading(level: L,
numbering: none)` (emitted only while heading numbering is on); class `notoc` ↔ `outlined: false`.

### 2.3 Figures
- Every `Figure` is numbered unless it has the class `unnumbered` (typst numbers every figure;
  caption and label are not required). Frontends that want an unnumbered figure add the class
  (typst `#figure(.., numbering: none)` ↔ `unnumbered`); the typst emitter writes `numbering: none`
  (Deviation 9).
- Three counters (typst counts per kind): **image** (`FigureKind::Image` and `Other`), **table**,
  **listing**. `Other` shares the image counter and the supplement "Figure" and is emitted to typst
  as `figure(kind: image, …)`. A typst *source* figure with a custom `kind:` string is counted by
  typst in its own counter; writer counts it as `Other` (image counter) — a known divergence the
  typst frontend lints as `W-TYP-FIGURE-KIND` (Deviation 9).
- A figure nested in a `Figure{kind: Other}` (sub-figure) is numbered in its own kind's counter
  **after** its parent (typst: parent `Figure 1`, children `Figure 2`, `Figure 3`, verified).
- `Document`: number `"{n}"`. `Section`: `"{c}.{n}"` where `c` = `heading_state.first()` (0 before
  the first numbered level-1 heading, so `0.1`) printed in arabic digits whatever the heading
  pattern; the image, table, listing and equation counters reset to 0 at every **numbered** level-1
  heading (an `.unnumbered` level-1 heading neither steps `c` nor resets).
- `Section` with headings `Off` cannot work in typst (the heading counter never steps): resolve
  warns `numbering-section-without-headings` once and uses `Document`; the package does the same (§3).

### 2.4 Equations
Only `Block::Math` (display) is numbered; inline math never. `Labelled`: numbered iff it has a
(valid) id; `All`: every display equation; `None`: none. The class `unnumbered` always excludes.
One counter; number string as figures (`"{n}"` / `"{c}.{n}"`, same reset rule); `Math.number` has
no parentheses (emitters print `(1.2)`; the reference text is `Equation 1.2`). A display equation
with the class `unnumbered` is emitted to typst as `#math.equation(block: true, numbering: none,
$…$)` or ``#mitex(`…`, numbering: none)`` (mitex forwards named arguments to `math.equation`;
Deviation 9). Equations unnumbered only by the policy (`labelled` without a label, `none`) are
emitted plainly (`$ … $`, `#mitex(…)`), and the package's rules of §3 leave them unnumbered (so
the typst frontend reads them back without the class, as typst-dialect.md §7's round trip needs).

### 2.5 Footnotes
One counter from 1 in document order (§4.3): every `Inline::Footnote`, including the generated
footnotes of note-style citations (§5.5) and footnotes nested in footnotes (they follow their
parent: typst numbers a cite inside footnote 2 as footnote 3, verified).

### 2.6 What is stored where
| field | form | example (headings `"1."`, figures `section`) |
|---|---|---|
| `Heading.number`, `TocEntry.number` | display form | `"1."`, `"1.2."` |
| `Figure.number`, `Math.number` | `n` or `c.n` (display = reference form) | `"1.2"` |
| `Footnote.number`, `FootnoteRef.number` | `u32` | `3` |
| `LabelInfo.number`, `ResolvedRef.number` | reference form (trimmed); `None` / `""` when unnumbered | `"1.2"` |

## 3. The `@writer/doc` rules — T31 (`assets/typst/writer-doc/{numbering,oracle}.typ`)

The package (API in typst-dialect.md §4) reproduces §2 with the function below; `writer-doc`
calls it with its `numbering:` argument (`headings: false | true | "<pattern>"`, `figures:
"document" | "section"`, `equations: "labelled" | "all" | "none"`). The code is normative for
behaviour (it is the verified extension of R5 §5.2); T31 may restructure it, T32's oracle (§7)
decides. The package must also: apply these rules to the title block and abstract as well as the
body (abstract headings report `none` to the oracle); render the abstract under `set
heading(numbering: none, outlined: false)`; set `page(numbering: "1")` (page references need it);
set `text(lang: L, region: R)` from §4.5's `LangTag`; and pass `#bibliography(.., style:)` the same
style string as writer (§5.2). **Citation locale:** when `[references] locale` is non-empty, the
emitter passes `cite-locale: "<code>"` (§5.2's parsed code) to `writer-doc` (`auto` → the
`writer-config` key `"cite_locale"`; Deviation 9) and the package adds `show cite: set text(lang:
L', region: R')` and `show bibliography: set text(lang: L', region: R')` (verified to switch the
citation locale). That rule also localizes typst's automatic bibliography title (*probe*:
"Bibliografie" under `de`), so with a citation locale every bibliography gets the explicit `title:
[<term(lang, Bibliography)>]` in the document language: the emitter writes it into
`#bibliography(..)`, the package into the one it places itself (the word from its embedded term
dictionary, typst-dialect.md §4.3). Verified: heading and outline entry stay in `lang`, the entries
switch.

```typst
#import "oracle.typ": oracle, oracle-on, figure-kind
#let writer-numbering(headings: false, figures: "document", equations: "labelled", body) = {
  let hpat = if headings == true { "1.1" } else if headings == false { none } else { headings }
  let section = figures == "section" and hpat != none          // §2.3 fallback
  let chapter() = counter(heading).get().first()
  set heading(numbering: hpat)
  set figure(numbering: if section { (..n) => numbering("1.1", chapter(), ..n) } else { "1" })
  set math.equation(numbering: if equations == "none" { none } else if section {
    (..n) => "(" + numbering("1.1", chapter(), ..n) + ")" } else { "(1)" })
  show figure.where(kind: table): set figure.caption(position: top)   // walk order §4.3
  show figure.where(kind: raw): set figure.caption(position: top)     // listings too (HTML, DOCX, LaTeX agree)
  show heading: it => {
    if section and it.level == 1 and it.numbering != none {
      counter(figure.where(kind: image)).update(0)
      counter(figure.where(kind: table)).update(0)
      counter(figure.where(kind: raw)).update(0)
      counter(math.equation).update(0)
    }
    it
    if oracle-on { context oracle("heading", it, if it.numbering == none { none } else {
      numbering(it.numbering, ..counter(heading).at(it.location())) }) }
  }
  show figure: it => { it; if oracle-on { context oracle(figure-kind(it), it,
      if it.numbering == none { none } else { numbering(it.numbering, ..it.counter.at(it.location())) }) } }
  show math.equation.where(block: true): it => {
    if equations == "labelled" and not it.has("label") and it.numbering != none {
      counter(math.equation).update(n => n - 1)                    // typst counted it: undo
      math.equation(block: true, numbering: none, it.body)
    } else { it; if oracle-on { context oracle("equation", it, if it.numbering == none { none } else {
      numbering(it.numbering, ..counter(math.equation).at(it.location())) }) } }
  }
  show footnote: it => { it; if oracle-on { context oracle("footnote", it,
      numbering(it.numbering, ..counter(footnote).at(it.location()))) } }
  // section-mode equation numbering is a function, which typst's ref does not trim:
  // print "Equation 1.2" (not "Equation (1.2)") like writer.
  show ref: it => {
    let el = it.element
    if section and el != none and el.func() == math.equation and it.form == "normal" and el.numbering != none {
      let loc = el.location()
      let n = numbering("1.1", counter(heading).at(loc).first(), ..counter(math.equation).at(loc))
      let sup = if it.supplement == auto { el.supplement } else { it.supplement }
      link(loc, if sup == none or sup == [] { [#n] } else { [#sup#sym.space.nobreak#n] })
    } else { it }
  }
  body
}
```
```typst
// oracle.typ (§7); every call site checks `oracle-on`, so a normal build evaluates no oracle code
#let oracle-on = sys.inputs.at("writer-oracle", default: "0") == "1"
#let oracle(kind, it, number) = if it.has("label") and not str(it.label).starts-with("writer-fn-") {
  [#metadata((kind: kind, label: str(it.label), number: number))#label("writer-oracle-" + str(it.label))]
}
#let figure-kind(it) = if it.kind == table { "table" } else if it.kind == raw { "listing" } else { "figure" }
```
Verified on the §Fixtures `numbering.md` shape: typst prints exactly its expected numbers and
reference texts, and the oracle reports them (also checked: an abstract heading reports `none`, a
`writer-fn-` footnote gets no entry, no oracle code runs without the input). LaTeX equations
emitted as `#mitex(..) <eq-x>` (math.md) are `math.equation(block: true)` and number identically
(mitex 0.2.7).

## 4. The resolve pass — T17 (citation steps T70)

### 4.1 Interface (ir.md §4, unchanged)
```rust
pub struct ResolveOptions<'a> { pub config: &'a Config, pub library: Option<&'a cite::Library>, pub lenient: bool }
pub fn resolve(document: &mut Document, opts: &ResolveOptions) -> Diagnostics;
pub fn anchor(label: &str) -> String;                                   // §1.4
pub fn supplement(kind: LabelKind, lang: &LangTag) -> String;           // §4.5
pub fn term(lang: &LangTag, term: Term) -> String;                      // §4.5
pub fn caption_separator(lang: &LangTag) -> &'static str;               // §4.5
pub const BIBLIOGRAPHY_ANCHOR: &str = "refs";
pub struct LangTag { pub lang: String /* "de" */, pub region: Option<String> /* "AT" */ }
impl LangTag {
    pub fn parse(tag: &str) -> Option<LangTag>;
    pub fn effective(meta: &DocMeta, config: &Config) -> Option<LangTag>; // None → English + `lang-invalid`
    pub fn locale_code(&self) -> String;                            // "de" | "de-AT"
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Term { Figure, Table, Listing, Equation, Section, Footnote, Bibliography, Contents, Page,
                Abstract, Note, Tip, Important, Warning, Caution }
```
`LangTag`, `Term`, `term`, `supplement` and `caption_separator` (signatures above, behaviour §4.5)
live in `src/resolve/l10n.rs`, written complete by T01 with their unit tests and
`assets/l10n/en.toml`, because T13 (wave 2, depending on T01 only) needs them while T17 runs in the
same wave (html-output.md Deviations). T01 also writes the stub `src/resolve/mod.rs` with `pub mod
l10n; pub use l10n::{LangTag, Term, term, supplement, caption_separator};` (stub: T01 → T17, like
`markdown/mod.rs`), so the `resolve::*` paths hold; T17 consumes them and raises `lang-invalid`.
`lenient` is `true` for `serve` and `check`, `false` for `build` and `dump`. T17 compiles
against `cite::Library` before T70 exists: T17 adds (additively) the stub `pub struct Library`
in `src/cite/library.rs` with §5.1's signatures (`load` returns `Error::Other("citations arrive
in T70")`, `keys` an empty set, `is_note_style` false) and `cite::render::render` returning empty
output; T70 replaces the bodies.

### 4.2 Phases (each a full walk in the order of §4.3)
0. **Setup.** `lang = LangTag::effective(meta, config)` (`None` → `lang-invalid`, English);
   parse the heading pattern (`numbering-invalid` → error, use `"1.1"`); apply §2.3's section fallback; `keys =
   library.keys()` or empty; `note = library.is_note_style()`. If `config.document.toc` is
   `Depth(n)` and no `Block::Toc` exists at any depth, insert `Toc { depth: n, span: None }` at
   index 0 (idempotent safety net; the Markdown frontend already does it). Keep the first
   `Block::Bibliography`, remove later ones (`bibliography-duplicate`).
1. **Labels.** Collect explicit labels (§1.1–1.2): kind, span, `label_text`; `duplicate-label`,
   `label-invalid`, `label-reserved`, `anchor-collision`.
2. **Auto ids** (§1.3).
3. **Main walk** (§4.3 order). Number headings, figures and equations as their blocks are
   reached (§2). In every inline sequence, in this order: (a) classify each `Ref`/`Cite` (§4.4),
   replacing nodes; (b) merge adjacent citations (§5.4); (c) left to right: number footnotes and
   note citations (§4.6, §5.5), record each sub-group of each `Cite` (§5.4) as a citation
   request, recurse.
4. **Citations** (T70, only when `library.is_some()` and requests exist): `cite::render::render`
   (§5) → `Cite.resolved` and `Resolved.bibliography`.
5. **Finish.** Fill every `Ref.resolved` (forward references work), `FootnoteRef.number`,
   placeholders (§4.7), internal links and image alt (§4.9).
6. **Assemble.** `Resolved.labels` (§4.5), `Resolved.toc` (§4.8), the Bibliography block,
   `Resolved.footnotes = n`, recompute `Features.citations` (any `Cite` left) and
   `Features.footnotes` (any `Footnote`).

### 4.3 Document order
`DocMeta.abstract_` blocks first, then `Document.blocks`, depth-first, children in IR order:
heading inlines; paragraph inlines; list items; definition terms then definitions; blockquote,
div, callout (title inlines, then blocks); table rows `head`, `body`, `foot`, cells left to
right; `Figure`: number assigned first, then for `kind: Table` or `Listing` caption then body,
otherwise body then caption (typst places table and listing captions on top, §3; a listing's
`CodeBlock` holds no inlines, so its order changes no number); `Inline::Footnote`: its number, then its
blocks, then the following inlines (nested footnotes follow their parent). `Inline::Image.alt` is
never walked (§4.9). `Block::Cell`: `attrs.id` (Other), then per `CellOutput` the counters
(headings, figure kinds, equations, footnotes) are snapshotted, **every** candidate is walked from
the snapshot, and the walk continues from the state after `candidates[0]`; labels are registered
from `candidates[0]` only; citations of all candidates are requested in order (a candidate other
than the one the typst emitter picks can thus add a request typst does not see — accepted).

### 4.4 Classification (markdown-dialect.md §7.3, completed)
`is_label(t)` = `t` in the label table (explicit + auto); `is_key(t)` = `t ∈ keys`.

| parse-time inline | label only | key only | both | neither |
|---|---|---|---|---|
| `Ref{target, supplement, form}` | stays `Ref`; target kind `Footnote` → `FootnoteRef{label: target}` (supplement/`page` ignored with `invalid-reference-mode`) | `Cite{items:[{key, locator: supplement.map(plain_text)}], mode: Normal}`; `form: Page` → `invalid-reference-mode` | `ambiguous-label` | `unresolved-reference` |
| `Cite{items, mode}`, every item | `Ref`s, see below; `mode ≠ Normal` → `invalid-reference-mode` | stays `Cite` | `ambiguous-label` | `unresolved-reference` (markdown-dialect.md §7.3; §8's hint names the case) |

A group whose items classify differently (label and key) is `ambiguous-label`. An all-label
group becomes, per item, `prefix` inlines + `Text(" ")` (if a prefix) + `Ref{target: key,
supplement: None, form: Normal}` + `Text(", " + locator)` (if a locator), items joined by
`Text(", ")`, brackets dropped. Any failing item turns the whole inline into the placeholder
(§4.7) and nothing of it is requested. One diagnostic per failing target occurrence, at the
inline's span.

### 4.5 Reference text, supplements, localisation
`LangTag::parse(tag)`: split on `-`/`_`; first subtag of 2–3 ASCII letters → `lang` (lowercase);
skip a 4-letter script subtag; a following 2-letter subtag → `region` (uppercase); anything else
ignored; no valid first subtag → `None` (`lang-invalid` warning, English). `LangTag::effective`:
parses `meta.lang`, else `config.document.lang`, else `"en"`. `LangTag::locale_code()` = `"de"` /
`"de-AT"`. `Term` = `Figure, Table, Listing, Equation, Section, Footnote, Bibliography, Contents,
Page` (typst keys `figure table raw equation heading footnote bibliography outline page`) and
`Abstract, Note, Tip, Important, Warning, Caution` (writer keys). typst keys come from
`typst::text::localized_str(lang.parse::<Lang>(), region.parse::<Region>(), key)` — the table the
PDF uses, 100 translation files (97 languages plus `fr-CA`, `pt-PT`, `zh-TW`), falling back to
English (`fr-CA` → `Fig.`, `de` → `Abbildung`); writer
keys from `assets/l10n/<lang>-<REGION>.toml`, then `<lang>.toml`, then `en.toml` (flat `key =
"value"`; T01 ships `en.toml`: `abstract = "Abstract"`, `note = "Note"`, `tip = "Tip"`,
`important = "Important"`, `warning = "Warning"`, `caution = "Caution"`).

`supplement(kind)`: Heading `Section` · Figure `Figure` · Table `Table` · Listing `Listing` ·
Equation `Equation` · Footnote `Footnote` · Other `""` (English shown). `caption_separator`:
typst's: `zh` `"\u{2003}"`, `fr` `".\u{a0}– "`, `ru` `". "`, else `": "`. Caption labels are
`supplement + "\u{a0}" + number + caption_separator` (`Figure 1.2: …`).

`label_text(inlines)` (plain text for `LabelInfo.text`, auto ids, unnumbered refs): `Text`, `Code`
text, `Math` source, `Ref` → its target, `Cite` → keys joined by `; `, `SoftBreak`/`LineBreak` →
`" "`, containers recursed, `Footnote`/`FootnoteRef`/`Raw` skipped. `LabelInfo { kind, number,
supplement: supplement(kind), text, span }` with `text` = `label_text` of the heading inlines, the
figure caption or the callout title, else `None`.

`ResolvedRef { kind, number, text, anchor: target }` (the label verbatim), with `E` = the explicit
supplement (`Some` even when empty):
- numbered target, `Normal` or `Page` form: `number` = reference form; `text` = `E ++ [Text("\u{a0}"
  + number)]` if `E` is non-empty, `[Text(number)]` if `E` is empty, else `[Text(S + "\u{a0}" +
  number)]` with `S = LabelInfo.supplement` (adjacent `Text`s merged). U+00A0 exactly as typst.
- unnumbered target: `number = ""`; `text` = `E` if non-empty, else `[Text(LabelInfo.text)]` if
  non-empty, else `[Text(S)]` if non-empty, else `[Text(label)]`. Headings: no diagnostic; every
  other kind (figure `.unnumbered`, equation under `none`/`.unnumbered`, `Other`):
  `unnumbered-target` (warning). `Page` form never warns.
- `Page` form keeps the Normal text (the HTML fallback); paging targets build `page N` (§6 and the emitter specs).

### 4.6 Footnotes
The counter of §2.5 sets `Footnote.number`. `FootnoteRef` matching: Markdown (and notebook) by
`(scope, label)` (§1.2), typst labels globally; a `Footnote` whose `(scope, label)` was
already seen is rewritten to `FootnoteRef` (defensive, e.g. notebook cells sharing a definition).
An unknown `FootnoteRef` is `unresolved-reference` and becomes the placeholder (§4.7). After
resolve every `Footnote.number` is `Some(1..=n)` in order and every `FootnoteRef.number` is `Some`.

### 4.7 Placeholders and lenient mode
A failing `Ref`/`Cite` (unresolved, ambiguous, mixed group) is replaced by
`Inline::Span { attrs: { classes: ["unresolved-reference"] }, inlines: [Text("[?" + targets.join("; ") + "]")] }`
(`[?fig-x]`, `[?a; b]`) in every mode, so `dump`, HTML, DOCX, typst and LaTeX show the same thing
and the typst emitter never meets a dangling `@`. `lenient` only changes the level of
`unresolved-reference` from error to warning; `ambiguous-label`, `duplicate-label`,
`anchor-collision` and `label-invalid` are errors in every mode.

### 4.8 TOC model
`Resolved.toc`: one `TocEntry { level, number: Heading.number, inlines, anchor: attrs.id }` per
`Heading` in document order at any depth (inside callouts, cells' `candidates[0]`), except
abstract headings and headings with class `notoc` (alias `unlisted`); `.unnumbered` headings are
listed with `number: None`. `inlines` = the resolved heading inlines with every `Footnote`/
`FootnoteRef` removed (note citations vanish with them). When `Resolved.bibliography` is `Some`,
an entry `{ level: 1, number: None, inlines: [Text(term Bibliography)], anchor:
BIBLIOGRAPHY_ANCHOR }` sits at the position of the `Block::Bibliography` (typst's bibliography
title is an outlined level-1 heading). Each `Block::Toc { depth }` lists the entries with `level
<= depth` under the title `term(Contents)`; several Toc blocks share the model.

### 4.9 Inserted blocks, internal links, image alt
- With `Resolved.bibliography` `Some` and no `Block::Bibliography`, append `Bibliography { span:
  None }` to `Document.blocks`. A `Bibliography` block without citations stays and renders
  nothing in every output (typst: no `#bibliography(..)` call, since typst prints the title even
  without citations, *probe*; Deviation 9).
- An `Inline::Link` with `url` `#x`: `x` must be a label (explicit or auto) or
  `BIBLIOGRAPHY_ANCHOR`, else `link-target-missing` (warning; typst, DOCX and LaTeX then print the
  link text without a link, HTML keeps the `href`).
- Every `Inline::Image.alt` that contains a `Ref`, `Cite`, `Footnote` or `FootnoteRef` is replaced
  by `[Text(s)]`, `s` = `label_text(alt)` except that each `Ref` becomes the plain text of the
  `ResolvedRef.text` §4.5 computes for its target from the label table (`[?target]` when unknown);
  alt text raises no diagnostics (an implicit figure's alt is a copy of its caption; walking it
  would number footnotes twice).

## 5. Citations — T70 (`src/cite/`)

### 5.1 Library (`cite/library.rs`)
```rust
pub struct Library { /* entries: hayagriva::Library, style: IndependentStyle, style_spec: String, files: Vec<PathBuf> */ }
impl Library {
    /// Loads and merges the files in order: `.bib` → `hayagriva::io::from_biblatex_str`, `.yaml`/`.yml` →
    /// `from_yaml_str`, other → "unknown bibliography format (must be .yaml/.yml or .bib)". A key in two
    /// files → "duplicate bibliography keys: a, b" (typst's message). Style after load: "ieee".
    pub fn load(paths: &[PathBuf]) -> Result<Library>;
    /// `spec` with a file extension → CSL file (absolute or relative to `base_dir`) parsed with
    /// `hayagriva::citationberg::Style::from_xml`; `Style::Dependent` → "dependent CSL styles are not supported".
    /// Otherwise `ArchivedStyle::by_name(spec)` (typst's 80 names, e.g. `ieee`, `apa`, `chicago-notes`)
    /// → `Style::Independent`; unknown → "unknown style: {spec}".
    pub fn set_style(&mut self, spec: &str, base_dir: &Path) -> Result<()>;
    pub fn keys(&self) -> BTreeSet<String>;
    pub fn is_note_style(&self) -> bool;       // style.settings.class == StyleClass::Note
    pub fn style_spec(&self) -> &str;          // as configured
    pub fn files(&self) -> &[PathBuf];         // absolute, for the watch set and LaTeX copies
}
```
Errors are `Error::Resource { what: "bibliography" | "citation style", path, message }`
(`check`/report codes `bibliography-error`, `citation-style-invalid`). The pipeline (T70 wiring)
resolves `[references] bibliography` and `style` paths per config.md §3, calls `load` then
`set_style(config.references.style, ..)`; an empty list means `library: None`. On error `build`
fails; `serve`/`check` report it and resolve with `library: None`. Use `hayagriva::citationberg::…`,
never `citationberg` directly (R5 §1).

### 5.2 Rendering (`cite/render.rs`)
```rust
pub struct Request<'a> { pub items: &'a [CiteItem], pub mode: CiteMode }    // one sub-group (§5.4)
pub struct CiteOutput { pub citations: Vec<Inlines> /* ↔ requests */, pub bibliography: Option<ir::Bibliography> }
pub fn render(library: &Library, locale: &str, requests: &[Request]) -> CiteOutput;
```
One `BibliographyDriver::<Entry>::new()`; per request, in document order,
`driver.citation(CitationRequest::new(items, &style, Some(LocaleCode(locale.to_owned())),
&locales, None))` — `note_number` is `None` because typst 0.15.1 passes `None`; `locales =
hayagriva::archive::locales()` (computed once). Then `driver.finish(BibliographyRequest::new(
&style, Some(LocaleCode(locale.to_owned())), &locales))` (`LocaleCode(pub String)`). `locale` =
`LangTag::parse(&config.references.locale)` → `locale_code()` when the setting is non-empty
(unparsable → `lang-invalid`, then as if empty), else `lang.locale_code()`; the package receives
the same code (§3), as typst rebuilds the CSL locale from `text(lang, region)` as `lang[-REGION]`.
Each item: `CitationItem::new(entry, locator, None, hidden, purpose)`
with `locator = Some(SpecificLocator(Locator::Custom, LocatorPayload::Str(text)))` for a
non-empty `CiteItem.locator` (typst's way; R5 §2.6) and:

| `CiteMode` | typst `form` | `purpose` | `hidden` |
|---|---|---|---|
| `Normal` | `"normal"` (default) | `None` | false |
| `Prose` · `Year` · `Author` · `Full` | `"prose"` · `"year"` · `"author"` · `"full"` | `Some(CitePurpose::Prose / Year / Author / Full)` | false |
| `None` | `none` (the value) | `None` | true (listed, prints nothing) |

`Prose` with a note style prints hayagriva's odd `Doe and Smith (“A Study of Things.”` (identical
in typst): warning `cite-prose-note-style`, output unchanged.

### 5.3 `ElemChildren` → `Inlines`
Flatten to runs `(text, Formatting, href)`: `ElemChild::Text(Formatted{text, formatting})` → one run;
`Elem{children, ..}` → its children (display/meta ignored); `Markup(s)` → run with default
formatting; `Link{text, url}` → run with `href = url`; `Transparent{..}` → nothing (never produced
with `LocatorPayload::Str`). Merge adjacent runs with equal formatting and href. Each run →
`Text(text)` wrapped innermost-first: `Underline` (decoration underline), `SmallCaps` (variant),
`Emph` (style italic), `Strong` (weight bold; light → none), `Superscript`/`Subscript` (vertical
align sup/sub; nature's numbers are superscript), then `Link{url: href, title: None}`. Example
(ieee): `[Text("D. E. Knuth, "), Emph[Text("The TeXbook")], Text(". Reading, MA: Addison-Wesley, 1984.")]`.

### 5.4 Grouping, prefixes and suffixes
typst merges citations separated only by spaces into one group (`typst-realize` `CITES` rule:
`CiteElem` triggers, `SpaceElem` is inner, anything else interrupts; any form). Resolve does the
same after classification: `Cite A`, then only `Text` nodes of spaces or tabs and `SoftBreak`s,
then `Cite B` with `B.mode == A.mode` → one `Cite { items: A ++ B, mode, span: A.span }`,
whitespace removed, repeatedly (`[@a] [@b]` and `@a @b` → one group; apa: `(Doe & Smith, 2020;
Lee et al., 2019)`). hayagriva and typst have no prefixes/suffixes; the typst emitter writes them
as markup next to their item's call (typst-dialect.md §5.6), which splits typst's group there. So
a `Cite` renders as consecutive **sub-groups**: a new one starts at each item with a prefix and
ends after each item with a suffix; each sub-group is one `Request`. `Cite.resolved.inlines` =
per sub-group in order: prefix + `Text(" ")` (if any), the rendered sub-group (or its footnote,
§5.5), `Text(" ")` + suffix (if any); sub-groups joined by `Text(" ")`. Same text in every output
(apa, *probe*): `[see @knuth1984, p. 12]` → `see (Knuth, 1984, p. 12)`; `[see @doe2020; @lee2019]`
→ `see (Doe & Smith, 2020; Lee et al., 2019)`; `[@doe2020; see @lee2019]` → `(Doe & Smith, 2020)
see (Lee et al., 2019)`.

### 5.5 Note styles
With `is_note_style()`, each sub-group (§5.4) of a `Cite` with `mode: Normal` (typst's rule; a
hidden `none` cite makes no footnote, verified) becomes `Footnote { blocks: [Paragraph { inlines:
citation, span: None }], label: None, number: Some(n) }` with the next footnote number at its
position (§2.5), placed in `resolved.inlines` where §5.4 puts the rendered sub-group, and
`note: true` ⇔ `inlines` holds such footnotes. Every emitter prints `resolved.inlines` as it is
and never makes a footnote for a citation (Deviation 3). A note citation inside a footnote is a
nested footnote numbered after its parent (typst, measured: `Note²` with `³` inside note 2). Other modes
render inline (`note: false`). The `Cite` node stays so typst and LaTeX emit real citations.

### 5.6 Bibliography
`Resolved.bibliography = Some(Bibliography { style: library.style_spec(), hanging_indent,
entries })` whenever at least one `Cite` was requested. `render` maps hayagriva's
`Rendered.bibliography` (`hanging_indent`, `items[i].{key, first_field, content}`) to
`CiteOutput.bibliography`: entries in hayagriva's order (numeric styles: first citation;
author-date: sorted), each `BibEntry { key, prefix: first_field → inlines (`[1]` for ieee),
inlines: content }`; `None` (a style without a bibliography section) → `entries: []`,
`hanging_indent: false`. Hidden cites are listed.

### 5.7 Layering (PLAN §1)
Nothing in M1–M6 needs hayagriva: with `library: None`, `keys` is empty, no request is rendered,
and every `@key`/`[@key]` is `unresolved-reference` (error in `build`, `[?key]` in `serve`/`check`)
with the hint "configure a bibliography to cite `key`". T70 only fills §5 and the
`is_note_style`/`keys` branches of §4.

## 6. Per-target rendering (labels, refs, cites, footnotes, bibliography, TOC)

Markup and class names are the emitters' specs (html-output.md, typst-dialect.md, docx-output.md,
latex-output.md). This section fixes what several specs must agree on: the HTML ids and the typst
labels and calls below are binding; for DOCX and LaTeX it fixes names only (after the table).
`T` = `resolved.text`; `E` = the explicit supplement.

| | HTML | typst |
|---|---|---|
| labelled element | `id="{label}"` on the element | `<label>` after it; a block or inline that emits no single element gets `#metadata(none) <label>` before it |
| `Ref`, numbered, Normal | `<a href="#{label}">T</a>` | `@label`; `E` non-empty → `@label[E]`; `E` empty → `@label[]` |
| `Ref`, unnumbered | `<a href="#{label}">T</a>` | `#link(<label>)[T]` |
| `Ref`, `Page` | as Normal (`check --target html`: `unsupported-construct`) | `#ref(<label>, form: "page")` (+ `supplement: [E]`) |
| `Footnote` n | `<sup><a id="fnref-{n}" href="#fn-{n}">n</a></sup>`; list after the body, `<li id="fn-{n}">` in number order, back-link `#fnref-{n}` | `#footnote[…]` + ` <flabel>`: the label verbatim when `Footnote.label` is a label (§1.2, kind `Footnote`); else typst-dialect.md §5.6's generated name (`writer-fn-` + `footnote_prefix` + name, `-k` on the k-th repeat in one emitted unit), never in `resolved.labels` and without oracle entry (§7); `label: None` → no label |
| `FootnoteRef` n (k-th repeat, k ≥ 2) | `<sup><a id="fnref-{n}-{k}" href="#fn-{n}">n</a></sup>` | `#footnote(<flabel of footnote n>)` |
| `Cite` | `<span data-cites="{keys, space-separated}">` + `resolved.inlines` | per sub-group (§5.4), sub-groups separated by one space: prefix markup + space (if any), the items' calls separated by single spaces (one typst group; call forms per typst-dialect.md §5.6: `@key`, `@key[locator]`, `#cite(<key>, form: "prose"[, supplement: [locator]])`, `#cite(<key>, form: none)`), then space + suffix markup (if any): the order of `resolved.inlines`; two adjacent `Cite`s (different modes) separated by `#h(0pt)` (stops typst's grouping, verified) |
| Bibliography | `<section id="refs">`, level-1 unnumbered heading term Bibliography, per entry `<div id="ref-{key}">` (prefix, then inlines) | `#bibliography(("a.bib", …), style: "{style}")` only when `resolved.bibliography` is `Some`, paths per typst-dialect.md; title auto (= term Bibliography), explicit under a citation locale (§3) |
| `Toc{depth}` | `<nav>`, title term Contents, nested lists of `<a href="#{anchor}">` (number + inlines) | `#outline(depth: d)` |

**DOCX and LaTeX.** docx-output.md (§4.8–§4.14) and latex-output.md (§4.3–§4.10, §7) own placement,
fields and cached results, footnote references and nesting, citation commands, the bibliography
commands and their diagnostics. This spec fixes: every bookmark name and `\label` key of a label is
`anchor(label)` (§1.4), and so is every internal link target (`w:hyperlink w:anchor`,
`\hyperref[..]`); DOCX introduces the bibliography with an unnumbered level-1 heading paragraph
(term Bibliography) carrying the bookmark `anchor("refs")`, so `Resolved.toc`'s entry and `#refs`
links have a target (Deviation 10). LaTeX numbers and cites itself (`\autocite` follows note
styles; `Cite.resolved` is unused), so its numbers may differ from writer's, by design.

## 7. The numbering oracle — T31 (metadata), T03 (query), T32 (test)

Contract (this section owns it; Deviation 8): when compiled with `sys.inputs.writer-oracle ==
"1"`, `@writer/doc` emits, right after every **labelled** heading, figure, block equation and
footnote, exactly one `#metadata((kind: K, label: L, number: N)) <writer-oracle-L>` with `K` ∈
`"heading" "figure" "table" "listing" "equation" "footnote"` (`Other` figures report `"figure"`),
`L` the label and `N` the number **as typst displays it at the element** (`"1."`, `"1.2"`,
`"(1.2)"`, `"3"`) or `none` when unnumbered. Footnote names `writer-fn-…` (§6) get no entry.
Without the input no oracle code is evaluated (§3: every call site checks `oracle-on`).

T03's query (`src/typst/compile.rs`; API names verified against typst 0.15.1 with a probe):
```rust
pub struct OracleEntry { pub kind: String, pub number: Option<String> }   // None ↔ typst `none`
/// `None` when the document holds no `writer-oracle-{label}` metadata.
pub fn oracle(document: &PagedDocument, label: &str) -> Option<OracleEntry> {
    let key = Label::new(PicoStr::intern(&format!("writer-oracle-{label}")))?;
    let content = document.introspector().query_label(key).ok()?;   // StrResult<&Content>
    let meta = content.to_packed::<MetadataElem>()?;
    let Value::Dict(d) = &meta.value else { return None };
    let s = |v: &Value| match v { Value::Str(s) => Some(s.as_str().to_owned()), _ => None };
    Some(OracleEntry { kind: s(d.get("kind").ok()?)?, number: s(d.get("number").ok()?) })
}
```
Test (`tests/pdf_oracle.rs`, T32; the one oracle test, over typst-dialect.md §8.4's fixtures plus
§Fixtures' `numbering.md` and its variants): parse + resolve + `typst::emit`, compile with T03's
`WriterWorld` whose inputs (`WorldOptions.inputs` / `set_inputs`) add `writer-oracle = "1"` to the
usual `writer-version` and `writer-config` (never a hand-built `Library`), then:
```rust
let doc = compile_document(&mut world).into_result().unwrap_or_else(|e| panic!("compile: {e}")).0;
for (label, info) in &document.resolved.labels {
    if info.kind == LabelKind::Other || label.starts_with("writer-fn-") { continue; }
    let got = oracle(&doc, label)
        .unwrap_or_else(|| panic!("{label}: writer {:?}, typst: no metadata", info.number));
    // assert kind and number against the expected values below; a mismatch panics naming the
    // label, writer's value and typst's value
}
let n = doc.introspector().query_labelled().iter()          // use typst::introspection::Introspector
    .filter(|c| c.label().is_some_and(|l| l.resolve().as_str().starts_with("writer-oracle-"))).count();
```
Expected per label: kind = the `LabelKind` snake name; number = `Heading.number` /
`Figure.number` / `format!("({})", Math.number)` / `Footnote.number.to_string()`, `None` ↔
`None`; `n` equals the number of labels checked in the loop.

## 8. Diagnostics (raised by resolve unless noted; messages typst-like)

| code | level | message (hint) |
|---|---|---|
| `label-invalid` | error | label `` `x y` `` is not valid (letters, digits, `_ - : .` only) |
| `duplicate-label` | error | label `` `<x>` `` occurs multiple times in the document (note: first defined at file:line) |
| `label-reserved` | warning | label `` `<fn-1>` `` is reserved for generated anchors (rename it) |
| `anchor-collision` | error | labels `` `<a>` `` and `` `<b>` `` map to the same anchor `_…` (rename one) |
| `ambiguous-label` | error | label `` `<x>` `` occurs both in the document and a bibliography (change either the {kind}'s label or the bibliography key); groups: citation group mixes labels and bibliography keys |
| `unresolved-reference` | error; warning when lenient | label `` `<x>` `` does not exist in the document (no library: configure a bibliography to cite `x`; with one: `x` is no bibliography key either) |
| `unknown-citation-key` | — | not raised: markdown-dialect.md §7.3 maps every target that is neither a label nor a key to `unresolved-reference`; the code keeps its meaning (key not in the bibliography) |
| `unnumbered-target` | warning | cannot reference {kind} without numbering; it renders as its text (enable `[numbering]`) |
| `invalid-reference-mode` | warning | `.{mode}` does not apply to {a cross-reference \| a citation \| a footnote reference}; ignored (markdown-dialect.md's code) |
| `numbering-invalid` | error | invalid numbering pattern `` `p` `` (falls back to `"1.1"`) |
| `numbering-unrepresentable` | warning | the numeral system `` `latin` `` cannot represent zero / the number n is too large to be represented with the `` `roman` `` numeral system |
| `numbering-section-without-headings` | warning | figures = "section" needs numbered headings; numbering per document |
| `lang-invalid` | warning | unknown language tag `` `x` ``; using English |
| `bibliography-duplicate` | warning | a document has one bibliography; this one is ignored |
| `link-target-missing` | warning | link target `` `#x` `` does not exist in the document |
| `cite-prose-note-style` | warning | prose citations render oddly in note styles (`{style}`) |
| `bibliography-error` | error | (pipeline, T70) `Library::load` messages of §5.1 |
| `citation-style-invalid` | error | (pipeline, T70) unknown style: `x` / dependent CSL styles are not supported |

Every diagnostic carries the span of the inline (refs, cites) or element (labels) and the file
of that span (`None` → the main source). `check` lists them per markdown-dialect.md §10.

## Fixtures

| fixture | owner | content |
|---|---|---|
| `tests/fixtures/md/references.md` | T01 (input), T1B (resolved snapshot) | markdown-dialect.md §11's cases plus: ref to an unnumbered heading, `@eq-x[]`, `@sec-x[Chapter]`, `{.page}`, an auto-id heading linked with `[x](#auto-id)`, `[@fig-a; @fig-b]`, a duplicate label, a reserved `{#fn-1}` |
| `tests/fixtures/md/footnotes.md` | T01 | repeated `[^a]`, inline `^[…]`, a footnote containing a footnote reference; numbers asserted 1..n |
| `tests/fixtures/md/numbering.md` | T17 (input only; new, additive; adds `images/a.png` if missing) | the text below; T12's `markdown_snapshots.rs` picks it up (T1B updates that snapshot once `@` refs scan), T1B snapshots its resolved IR (`tests/resolve_snapshots.rs`, new), T32 runs the oracle on it |
| `tests/resolve.rs` (+ optional `tests/fixtures/ir/*-unresolved.json`) | T17 (new) | T17 has no Markdown parser (T12 is wave 3, T1B wave 5): it builds the unresolved IR of numbering.md's shape with ir.rs constructors and asserts the expected values below, plus every §4.4/§4.5 ref case, footnotes (repeated, nested, two Markdown scopes), auto ids, `duplicate-label`, `label-reserved`, `anchor-collision` |
| `tests/fixtures/bib/refs.bib` | T01 | exactly R5 §2.8's `BIB` (doe2020, knuth1984, lee2019) |
| `tests/fixtures/ir/sample-document.json` | T01 | resolved form; ref texts use U+00A0 (`"Figure 1"`, Deviation 7) |
| `tests/fixtures/md/citations.md`, `citations-notes.md` | T70 (new; also their `markdown_snapshots__citations*.snap` and diagnostics snapshots, since T12's test snapshots every `*.md`) | apa: every mode, `[see @knuth1984, p. 12]`, `[@doe2020; see @lee2019]`, `[-@doe2020]`, `@doe2020 @lee2019` and `[@a] [@b]` merging, label-vs-key, `ambiguous-label`, unknown key; chicago-notes: note cites, a cite in a footnote (nested numbering), prose warning |
| `tests/fixtures/bib/{refs.yaml, minimal.csl, dup-a.bib, dup-b.bib}` | T70 (new) | R5's YAML entry `k1`; a hand-written independent CSL (MIT, writer's own); one shared key → duplicate error |

Expected strings come from R5 §2.8/§3.4 (ieee `[1]`, `[2, p. 12]`, `[1], [3]`, `J. Doe and J. Smith
[1]`; apa `(Doe & Smith, 2020; Lee et al., 2019)`). Unit tests: §1.4's and §2.1's tables, auto
ids (T17); `LangTag` (`de-AT`, `zh-Hans-CN` → `zh`/`CN`, `x` → None), `term` for en/de/fr-CA (T01,
in `resolve/l10n.rs`, §4.1).

`tests/fixtures/md/numbering.md` (the shape of §3's typst probe): LF line ends, these blocks in
order with one blank line between them (`⏎` = line break inside a block):
`---` ⏎ `title: Numbering` ⏎ `numbering: { headings: "1.", figures: section, equations: labelled }` ⏎ `---` ·
`![before](images/a.png){#fig-zero}` · `# One {#sec-one}` · `![a](images/a.png){#fig-a}` ·
`Table: t {#tbl-a}` · `| x |` ⏎ `|---|` ⏎ `| 1 |` ·
```` ```rust {#lst-a caption="code"} ```` ⏎ `fn main() {}` ⏎ ```` ``` ```` ·
`:::figure kind=other caption="other" {#fig-other}` ⏎ `Some *other* content.` ⏎ `:::` ·
`$$ x = 1 $$` · `$$ y = 2 $$ {#eq-a}` · `## Sub {#sec-sub}` · `### Deep {#sec-deep}` ·
`# Interlude {#sec-interlude .unnumbered}` · `![in interlude](images/a.png){#fig-int}` ·
`## Under interlude {#sec-under}` · `# Two {#sec-two}` · `![c](images/a.png){#fig-c}` ·
`$$ z = 3 $$ {#eq-b}` · `See @sec-one, @fig-zero, @eq-a, @eq-a[Eq.], @eq-a[], @sec-two[Chapter] and @sec-interlude.`

Expected (IR `number` · oracle `number` · reference text): `fig-zero` 0.1 · "0.1" · Figure 0.1;
`sec-one` "1." · "1." · Section 1; `fig-a`, `tbl-a`, `lst-a` 1.1 · "1.1" · Figure/Table/Listing
1.1; `fig-other` 1.2 · "1.2" · Figure 1.2; the unlabelled `x = 1` unnumbered; `eq-a` "1.1" ·
"(1.1)" · Equation 1.1 (`[Eq.]` → Eq. 1.1, `[]` → 1.1); `sec-sub` "1.1."; `sec-deep` "1.1.1.";
`sec-interlude` none · none · Interlude (no diagnostic); `fig-int` 1.3; `sec-under` "1.2.";
`sec-two` "2." · "2." · `[Chapter]` → Chapter 2; `fig-c` 2.1; `eq-b` "2.1" · "(2.1)". T32 also
runs the policies `(true, document, all)`, `("A.1", section, none)` and `(false, section →
fallback, labelled)`: it overwrites `config.numbering` on the effective Config **after**
`Config::merge_front_matter` (the front matter would win otherwise, config.md §1) and passes that
Config to both `resolve` and `typst::emit` (which writes `numbering:` into `writer-doc.with`);
typst is the oracle there, and each variant asserts a number that differs from the base run
(`fig-a` = "2" under `(true, document, all)`).

## Open points (local choices)
- T17: one generic mutable walker with a phase argument or separate walkers; the internal label
  table type.
- T17: message wording beyond the templates of §8.
- T70: caching `locales()` in a `OnceLock`; keeping `hayagriva::Library` or a `BTreeMap` inside `Library`.

## Deviations from PLAN.md and the brief
1. **Anchor mangling.** The brief's formula (`[A-Za-z0-9_]{1,38}` kept, else `hex(sha1)[..39]`) is
   not what pandoc does; pandoc 3.12.1 (measured) keeps labels of ≤ 39 alphanumerics/underscores
   and otherwise uses the sha1 hex **without its first digit** (`sec-intro` →
   `_fdabef51…`, sha1 `3fdabef51…`). §1.4 follows pandoc (the brief's stated intent) but keeps the
   brief's ASCII class (pandoc also keeps Unicode letters; LaTeX labels stay ASCII). Needs the new
   dependency `sha1_smol = "1.0.1"` (BSD-3-Clause; T00, contract change). docx-output.md §4.9's
   parenthetical restates the brief's formula: §1.4 supersedes it (`resolve::anchor` is the only
   implementation; contract change for docx-output.md).
2. **Localisation.** The nine typst terms come from `typst::text::localized_str` instead of
   `assets/l10n/<lang>.toml`, so HTML/DOCX words equal the PDF's by construction for typst's 100
   translation files; `assets/l10n/` (a new path, owned by T01 with `src/resolve/l10n.rs`, §4.1,
   so T13 can use it in wave 2; html-output.md and vendored-assets.md Deviations) keeps the
   mechanism for writer-only terms.
3. **Note citations** keep their `Cite` node; `ResolvedCite.inlines` already holds the generated
   `Footnote`s (§5.5) and the prefixes and suffixes outside the citation sub-groups (§5.4). Contract
   change: this supersedes ir.md's `ResolvedCite.note` comment ("HTML and DOCX put `inlines` in a
   footnote") and docx-output.md §4.9's "with `c.note` a new footnote …": every emitter renders
   `c.inlines` as it is (its `Footnote`s become the notes, nested ones per the emitter's nesting
   rule), `note` is informational, no emitter makes a footnote for a citation (html-output.md
   §1.12 already agrees).
4. **`note_number: None`** (typst 0.15.1's value) instead of R5 §2.4's footnote number.
5. **Adjacent citations merge** (typst's grouping rule) and render as sub-groups split at
   prefixes and suffixes (§5.4) — not stated in markdown-dialect.md §7.3.
6. **Auto identifiers** for unlabelled headings (needed for `TocEntry.anchor`); `.unnumbered` also
   applies to figures and equations; `figures = "section"` without heading numbering falls back to
   `document`; T17 writes the `cite::Library` stub in T70's file (additive).
7. **Ref text spacing.** Contract change for ir.md §6: resolved ref texts are `supplement + U+00A0 +
   number` (typst's, §4.5), so the sample's `"text": "Figure 1"` uses U+00A0; T01 writes
   `sample-document.json` that way.
8. **Oracle.** §7 is the oracle contract and supersedes typst-dialect.md §4.5 (number "as
   `LabelInfo.number` prints it", "unnumbered elements emit nothing"), §8.1 (`OracleEntry.number`
   becomes `Option<String>`), §8.3 (now §7's code) and §8.4's assertion (`Some` for every
   non-`Other` label, `number: None` when unnumbered); one test, `tests/pdf_oracle.rs`.
9. **typst-dialect.md** (contract change for T30/T31/T50 through the coordinator): (a) `figure(..,
   numbering: none)` → class `unnumbered`, and the emitter writes `numbering: none` for
   `.unnumbered` figures; (b) display `Math` with the class `unnumbered` ↔ `#math.equation(block:
   true, numbering: none, $ src $)` / ``#mitex(`src`, numbering: none)``; equations unnumbered only
   by the policy are emitted plainly (§2.4); (c) `heading(outlined: false)` ↔ class
   `notoc`, both directions; (d) new warning `W-TYP-FIGURE-KIND` for a string `kind:`; (e)
   `Bibliography` → `#bibliography(..)` only when `resolved.bibliography` is `Some`, with `title:`
   under a citation locale (§3); (f) footnote labels per §6: a typst label verbatim, the
   `writer-fn-` form only for Markdown/notebook names (the round trip's `normalize` then ignores
   `LabelKind::Footnote` entries of `resolved.labels`, since a stripped `writer-fn-` name becomes a
   typst label); (g) new `writer-doc` argument `cite-locale` (↔ `[references] locale`; `auto` →
   `writer-config` key `"cite_locale"`) in §3, §4.2, §5.3, §6.2; (h) §5.6's cite prefix/suffix
   placement stays (it defines §5.4's sub-groups), with one space before a suffix (§6); (i) listing
   captions sit on top like table captions (§3's `show figure.where(kind: raw)` rule, matching
   html-output.md §1.6, docx-output.md §4.10 and latex-output.md §4.7; T31 implements it and
   typst-dialect.md §4.3 should name it).
10. **DOCX bibliography heading.** docx-output.md §4.14 writes no heading; §6 requires one because
    `Resolved.toc` lists the bibliography and `#refs` links are valid (§4.9). Contract change for
    docx-output.md §4.14 (T41).


> **Contract clarification (coordinator, 2026-10-28):** T50 keeps a labelled
> plain Typst table's id on its inner Table as kind Other. Its unnumbered,
> captionless outer Table figure exists only to satisfy the shared IR invariant;
> it has no id and does not consume a figure counter (typst-dialect.md §2.3).

> When a labelled plain table contains nested figures, T50 carries the same
> Other-kind label on a containing Div instead of introducing forbidden figure
> nesting. The table content and all other labels are retained.
