# T17 — Resolve pass: labels, numbering, references, footnotes, TOC

| | |
|---|---|
| Milestone | M1 |
| Size | M |
| Depends on | T01 |
| Parallel-safe with | T03, T04, T10, T11, T13, T14, T51 (wave 2; see the board) |
| Files you own | `src/resolve/mod.rs` (replace stub: T01 → T17; keep T00's `mod` lines and T01's `pub mod l10n; pub use l10n::{…};` line), `src/resolve/labels.rs`, `src/resolve/numbering.rs`, `src/resolve/footnotes.rs`, `src/resolve/toc.rs` (replace stubs), `tests/resolve.rs` (new), `tests/fixtures/md/numbering.md` (new, input only), `tests/fixtures/ir/*-unresolved.json` (new, optional); (additive, references.md §4.1: the stub `Library` in `src/cite/library.rs`, the stub `Request`/`CiteOutput`/`render` in `src/cite/render.rs`, and `pub use library::Library;` in `src/cite/mod.rs` only if T00's stub lacks it — all T70's files); (additive: `tests/fixtures/md/images/a.png` if missing — T01's directory) |

## Goal
`resolve::resolve` turns every frontend's syntactic IR into one every emitter prints without thought: numbers per
`[numbering]` (typst parity), resolved `Ref`s with localized texts, `[?x]` placeholders, footnote numbers, auto ids, the
TOC model and every reference diagnostic. Citation branches (keys, merge, sub-groups, notes) are written but inactive
until T70. Consumers: T16 (pipeline, `dump`), T30/T31, T13/T40/T60 (emitters), T32 (oracle), T1B/T70.

## Read first
- `docs/spec/references.md` **§1, §2, §4 (4.1–4.9), §8 — this is your spec**; §Fixtures, Open points, Deviations
  1, 6, 7; §5.1–§5.2 (stub signatures), §5.4 (merge, sub-groups), §5.5 (note branch), §5.7; skim §3, §6, §7.
- `docs/spec/ir.md` §1, §2 (invariants 4, 5, 8, 9), §4, §5; `docs/spec/markdown-dialect.md` §7.3 (parse-time shapes),
  §8 (footnote names per file), §10; `docs/spec/notebook-mapping.md` §2.4, §3.4, §4.6; `docs/spec/config.md` §2
  (`[document]`, `[numbering]`), §4; `docs/spec/html-output.md` Deviations "Localisation before T17".
- `docs/research/R5-citations-crossrefs.md` §4.1 (typst ref texts, wording), §5.1–§5.2; `docs/research/R2-typst-syntax-subset.md`
  §2.1. Nothing is lifted from presenter_rs (R8 §10 has no resolve row).

## API to implement
```rust
// src/resolve/mod.rs (references.md §4.1; ir.md §4)
pub struct ResolveOptions<'a> { pub config: &'a Config, pub library: Option<&'a cite::Library>, pub lenient: bool }
pub fn resolve(document: &mut Document, opts: &ResolveOptions) -> Diagnostics;
pub fn anchor(label: &str) -> String;                       // §1.4 (body may live in labels.rs, re-exported)
pub const BIBLIOGRAPHY_ANCHOR: &str = "refs";
// consumed from T01's l10n.rs (never edit): LangTag::{parse, effective, locale_code}, Term, supplement, term, caption_separator

// src/resolve/numbering.rs (§2.1)
pub struct NumberingPattern { pieces: Vec<(String, codex::numeral_systems::NamedNumeralSystem)>, suffix: String }
impl NumberingPattern {
    pub fn parse(pattern: &str) -> Option<NumberingPattern>;
    pub fn apply(&self, numbers: &[u64], trimmed: bool) -> (String, Vec<u64>);
}

// src/cite/library.rs — additive stub (§5.1 signatures; T70 replaces bodies and private fields)
pub struct Library { /* stub: style_spec: String, files: Vec<PathBuf> */ }
impl Library {
    pub fn load(paths: &[PathBuf]) -> Result<Library>;       // Err(Error::Other("citations arrive in T70".into()))
    pub fn set_style(&mut self, spec: &str, base_dir: &Path) -> Result<()>;   // stores spec, Ok(())
    pub fn keys(&self) -> BTreeSet<String>;   pub fn is_note_style(&self) -> bool;   // empty / false
    pub fn style_spec(&self) -> &str;         pub fn files(&self) -> &[PathBuf];
}
// src/cite/render.rs — additive stub (§5.2)
pub struct Request<'a> { pub items: &'a [CiteItem], pub mode: CiteMode }
pub struct CiteOutput { pub citations: Vec<Inlines>, pub bibliography: Option<ir::Bibliography> }
pub fn render(library: &Library, locale: &str, requests: &[Request]) -> CiteOutput;   // empty output
```

## Steps
1. Orient (no edits): on `main` read `src/resolve/{mod,l10n}.rs`, `src/ir.rs` (`plain_text`, `detect_kind`,
   `Block::span`, constructors), `src/error.rs` (`Diagnostic::new(level, code, msg).with_path(p).with_span(s)`,
   `Diagnostics::{push, warn, error}`), `src/config.rs` (field names of `NumberingConfig`, `DocumentConfig`). Read
   the policy only from `opts.config`, never from `meta.vars` (the pipeline already merged the front matter).
2. Cite stubs (additive, named in the PR): bodies as in the API comments, docs "stub, replaced by T70"; add
   `pub use library::Library;` to `src/cite/mod.rs` only if `crate::cite::Library` does not resolve.
3. `numbering.rs`: port typst's `FromStr`/`apply_with` exactly (Hints); on a `represent` error push `n.to_string()`
   and record `n`; a private variant also returns the system name for §8's `numbering-unrepresentable` message.
   Add a `Clone` counter state: heading `Vec<u64>` (typst's step), image (`Image` + `Other`), table, listing,
   equation and footnote counters, §2.3's reset at numbered level-1 headings in section mode.
4. `labels.rs`: `anchor` (§1.4 verbatim), validity (`typst_syntax::is_valid_label_literal_id`), reserved (§1.3
   regex), `label_text` (§4.5; not `ir::plain_text`: `Ref` → target, `Cite` → keys joined `"; "`, footnotes and
   `Raw` skipped), auto ids (§1.3 + the termination rule in Notes), the label table (local choice).
5. Walker (open point: one walker with a phase argument, or several): recursive over `&mut Vec<Block>` and
   `&mut Vec<Inline>` in §4.3's order exactly (abstract first; figure number before its children, `Table`/`Listing`
   caption then body, others body then caption; a `Footnote`'s blocks right after its number; `Image.alt` never).
   Carry the scope (§1.2: nearest enclosing block with a span → `span.file`, else `source.path`; kind via
   `ir::detect_kind(&p).ok()`, `Some(source.kind)` for the main file), the nearest block span and an "in abstract"
   flag; diagnostics get `path = Some(span.file or source.path)`. Cells: phases 1, 2 and 6 read `candidates[0]` only;
   phase 3 clones the state (counters, footnote table) per `CellOutput`, walks every candidate from that snapshot and
   continues from the state after `candidates[0]`.
6. Phase 0 (§4.2): `lang` (`LangTag::effective`; `None` → `lang-invalid`, English); heading pattern (`Off` → none,
   unparsable → `numbering-invalid` error and `"1.1"`); section without heading numbering → one
   `numbering-section-without-headings`, use `Document`; `keys`/`note` (empty/false without a library); insert `Toc {
   depth: n, span: None }` at index 0 when `config.document.toc` is `Depth(n)` and no `Toc` exists anywhere; keep the
   first `Bibliography`, remove later ones (`bibliography-duplicate` at their span).
7. Phases 1–2 (§1): register labels per §1.2's table (typst-scope `Footnote.label` → `Footnote`, every other
   `attrs.id` → `Other`). Invalid → `label-invalid`, `attrs.id = None`; second definition → `duplicate-label` at the
   second site naming the first's file:line, first wins, the IR id stays; reserved → `label-reserved`, kept; one
   anchor for two labels → `anchor-collision` naming both. Then every `Heading` without an id (abstract,
   `.unnumbered`, `.notoc` too) gets an auto id written into `attrs.id` and registered as `Heading`.
8. Phase 3 blocks (§2.2–§2.4): headings (pattern, no class `unnumbered`, not in the abstract; display form into
   `Heading.number`, trimmed form into the table; `numbering-unrepresentable` once per heading), figures
   (`unnumbered` excluded; `"{n}"` / `"{c}.{n}"`, `c` = `heading_state.first()` or 0, arabic; sub-figures after
   their parent), display equations (policy + `unnumbered`). Always write `number` (`Some` or `None`).
9. Phase 3 inline sequences (§4.2 (a)–(c)): (a) classify per §4.4 — unknown → §4.7 placeholder and
   `unresolved-reference` (error; warning when `lenient`; hint "configure a bibliography to cite `x`" without a
   library); all-label `Cite` → refs with prefix/locator, joined by `Text(", ")`, `mode ≠ Normal` →
   `invalid-reference-mode`; a `Footnote`-kind target → `FootnoteRef`. (b) merge adjacent `Cite`s (§5.4). (c) left to
   right: number each `Footnote` or rewrite it to `FootnoteRef` when its `(scope, label)` was seen (§4.6; typst
   scope: the label alone), recurse into its blocks; split each `Cite` into §5.4 sub-groups, record each as an owned
   request (clone the items; build `Request` slices only in phase 4); when `note && mode == Normal` reserve the next
   footnote number per sub-group (inactive until T70 builds the footnote, §5.5).
10. Phase 4: a private function returning at once when `library` is `None` or no request exists; its body is
    T70's (comment citing references.md §5). Do not assemble `Cite.resolved` yourself.
11. Phase 5 (§4.5–§4.9): every `Ref.resolved = ResolvedRef { kind, number, text, anchor: target }` (U+00A0, adjacent
    `Text`s merged, the unnumbered fallback chain, `unnumbered-target` except headings and `Page`); `FootnoteRef`
    numbers (unknown → placeholder + `unresolved-reference`); `Link` `#x` with `x` neither a label nor `refs` →
    `link-target-missing` (URL kept); `Image.alt` holding `Ref`/`Cite`/`Footnote`/`FootnoteRef` → `[Text(s)]`.
12. Phase 6 (`toc.rs` for §4.8): `Resolved.labels` (`supplement(kind, &lang)`, reference-form number, footnote labels
    `Some(n.to_string())`); append `Bibliography { span: None }` when `resolved.bibliography` is `Some` and none exists;
    then the TOC (skip abstract, `notoc`, `unlisted`; `.unnumbered` → `None`; footnotes stripped recursively; the `refs`
    entry at the `Bibliography` when `resolved.bibliography` is `Some`); `Resolved.footnotes = n`; recompute `features.{citations, footnotes}`.
13. Fixture `tests/fixtures/md/numbering.md` (references.md §Fixtures): the 19 blocks below in order (`  ·  `
    separates blocks sharing a line here), one blank line between blocks, `⏎` = a line break inside a block, LF,
    trailing newline. Check: 46 lines, `shasum -a 256` = `d15719d560e44e199a5d06c281fa4fcdb297a27e745851d248b554d59ae4031f`.
    ````text
    ---⏎title: Numbering⏎numbering: { headings: "1.", figures: section, equations: labelled }⏎---
    ![before](images/a.png){#fig-zero}  ·  # One {#sec-one}  ·  ![a](images/a.png){#fig-a}  ·  Table: t {#tbl-a}
    | x |⏎|---|⏎| 1 |
    ```rust {#lst-a caption="code"}⏎fn main() {}⏎```
    :::figure kind=other caption="other" {#fig-other}⏎Some *other* content.⏎:::
    $$ x = 1 $$  ·  $$ y = 2 $$ {#eq-a}  ·  ## Sub {#sec-sub}  ·  ### Deep {#sec-deep}
    # Interlude {#sec-interlude .unnumbered}  ·  ![in interlude](images/a.png){#fig-int}  ·  ## Under interlude {#sec-under}
    # Two {#sec-two}  ·  ![c](images/a.png){#fig-c}  ·  $$ z = 3 $$ {#eq-b}
    See @sec-one, @fig-zero, @eq-a, @eq-a[Eq.], @eq-a[], @sec-two[Chapter] and @sec-interlude.
    ````
    Add `images/a.png` if missing (Hints). No `md/*.md` snapshot test exists before T12 (PLAN §7: nothing to re-bless).
14. `tests/resolve.rs`: no parser exists yet (T12 is wave 3): build the unresolved IR with `ir.rs` constructors + helpers.

## Acceptance criteria
- Unit tests: `resolve::labels::tests::{anchor_table (every §1.4 row), auto_id_examples (§1.3's examples),
  reserved (refs, ref-x, writer-a, fn-1, fnref-2-3 yes; fn-x, refs-1 no)}`; `resolve::numbering::tests::pattern_table`
  (all seven §2.1 rows, both forms; `1.a` on `[2,0,1]` reports `vec![0]`; `parse("x")`, `parse("")` → `None`);
  `resolve::tests::{merge_adjacent_citations, cite_sub_groups}` (§5.4 on hand-built `Cite`s, its three examples).
- `tests/resolve.rs::numbering_md_base` (numbering.md's IR, `("1.", section, labelled)`): every value of §Fixtures
  (`fig-zero` 0.1, `sec-one` "1." / label "1", `fig-a`/`tbl-a`/`lst-a` 1.1, `fig-other` 1.2, `x = 1` `None`, `eq-a` 1.1,
  `sec-sub` "1.1.", `sec-deep` "1.1.1.", `sec-interlude` `None`, `fig-int` 1.3, `sec-under` "1.2.", `sec-two` "2.",
  `fig-c` 2.1, `eq-b` 2.1); ref texts `Section␣1`, `Figure␣0.1`, `Equation␣1.1`, `Eq.␣1.1`, `1.1`, `Chapter␣2`,
  `Interlude` (␣ = U+00A0, one `Text` each); no diagnostics.
- `numbering_md_variants` (derived from §2): `("1.1", document, all)`: headings 1, 1.1, 1.1.1, 1.2, 2, images 1–5 in
  order (`fig-a` "2"), `tbl-a`/`lst-a` 1, equations 1, 2, 3; `("A.1", section, none)`: headings A, A.1, A.1.1, A.2, B,
  figures as in the base run, `eq-a`/`eq-b` `None`, three `unnumbered-target`, `Chapter␣B`; `(Off, section, labelled)`:
  exactly one diagnostic (`numbering-section-without-headings`), images 1–5, `eq-a` 1, `eq-b` 2, `@sec-one` → `One`.
- `refs_and_cites`: `@x[]`, `@x[E]`, unnumbered heading (text, no warning), `.unnumbered` figure (caption +
  `unnumbered-target`), `Div`/`Callout` targets (label or title + warning), `Page` (Normal text, no warning), an all-label
  group with prefix and locator (`see`, `Text(" ")`, ref, `Text(", p. 2")`), a `.prose` group (`invalid-reference-mode`);
  `@nope` → `Span.unresolved-reference` `[?nope]`, `Level::Error` (`Warning` when lenient); unknown `[@a; @b]` → `[?a; b]`,
  two diagnostics; label + unknown → one placeholder, one diagnostic; every `Ref` left has `resolved: Some`.
- `footnotes_scopes`: repeated, nested (inner after parent), inline, two Markdown scopes (`span.file` set vs `None`) with
  one name → two numbers, a duplicate `Footnote` → `FootnoteRef`, unknown `FootnoteRef` → placeholder; numbers 1..=n,
  `Resolved.footnotes == n`; typst `Footnote <fn-x>` is a label, `@fn-x` → `FootnoteRef` (`[s]` → `invalid-reference-mode`).
- `labels_and_auto_ids`: `duplicate-label`, `label-reserved` (kept), `anchor-collision` (`sec-intro` vs `fdabef51864cece78e14c461e4ab265d0b74de7`),
  `label-invalid` (dropped), errors also when lenient; auto ids `gpu-allocation-results`, `intro-1` (explicit `intro`
  exists), `refs-1`, `section`, `section-1`; abstract headings get ids, no number, no TOC entry.
- `toc_and_blocks`: entries in order, `anchor == attrs.id`, `notoc`/`unlisted` skipped, `.unnumbered` → `None`, footnotes
  stripped; `Toc { depth: 2 }` inserted at index 0 only when none exists (one inside a `Div` counts); second
  `Bibliography` removed (`bibliography-duplicate`); `#gpu-allocation-results`, `#refs` pass, `#nope` → `link-target-missing`;
  image alt `[Text("see "), @fig-a]` → `[Text("see Figure␣1")]`, a footnote in alt unnumbered; `features.citations`
  false after placeholders, `features.footnotes` true.
- `cells_snapshot` (two candidates with an unlabelled image figure each → both "1", the next figure "2"; a label only
  in `candidates[1]` is unregistered); `languages` (`meta.lang = "de"` → `Abbildung␣1`; `"x"` → `lang-invalid`,
  English); `numbering_diagnostics` (`Pattern("x")` → `numbering-invalid`, headings "1"; `Pattern("1.a")`, level 3
  after level 1 → "1.0.a", one `numbering-unrepresentable`); `empty_document` (no diagnostics, `resolved.is_empty()`).
- `shasum -a 256 tests/fixtures/md/numbering.md` prints `d15719d5…4031f` (step 13); `cargo fmt --check`, `cargo clippy
  --all-targets --all-features -- -D warnings`, `cargo test`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
T70: `cite::*` bodies, phase 4, §5.5's footnotes, §5.6, key/note-branch tests, `cite-prose-note-style`, `[references]
locale`. T01: `l10n.rs`, `ir.rs`, `config.rs`, `error.rs`. T12/T1B: parsing, the `@` scanner, `tests/resolve_snapshots.rs`.
T16: calling `resolve`, `dump`. T31/T32: package rules, oracle. Emitters; `check` (T1C, T71). No new dependency.

## Hints (verified)
- typst-syntax `=0.15.1` (R2 §2.1; `src/lexer.rs:1252,1275`): `typst_syntax::is_valid_label_literal_id(id: &str) -> bool`
  (`!id.is_empty()` and every char `is_id_continue(c) || matches!(c, ':' | '.')`); `typst_syntax::is_id_continue(c:
  char) -> bool` is `is_xid_continue(c) || c == '_' || c == '-'`.
- codex `=0.3.0` (default feature `numeral-systems`; `src/numeral_systems.rs`): `NamedNumeralSystem::from_shorthand(shorthand:
  &str) -> Option<Self>` (a `&str`), `.system() -> NumeralSystem<'static>`, `.name() -> &'static str` (`"latin"`, `"roman"`),
  `NumeralSystem::represent(self, number: u64) -> Result<impl Display, RepresentationError>` (`Zero`, `TooLarge`).
  `NamedNumeralSystem` is `#[non_exhaustive]` and derives only `Debug, Copy, Clone, Eq, PartialEq, Hash`.
- The port (typst-library 0.15.1 `src/model/numbering.rs` lines 188–316, in the registry as typst's dependency):
  ```rust
  for (i, c) in pattern.char_indices() {
      let Some(kind) = NamedNumeralSystem::from_shorthand(c.encode_utf8(&mut [0; 4])) else { continue };
      pieces.push((pattern[handled..i].to_owned(), kind)); handled = c.len_utf8() + i;
  }   // suffix = pattern[handled..]; no pieces → None
  // apply: zip(pieces, numbers): prefix unless i == 0 && trimmed, then repr; leftover numbers cycle the LAST piece:
  // its prefix (the suffix when that prefix is empty), then repr; finally the suffix unless trimmed
  ```
- sha1_smol `1.0.1`: `Sha1::from<D: AsRef<[u8]>>(data: D) -> Sha1`; `.digest().to_string()` = 40 lowercase hex (§1.4 re-checked).
- Reserved regex (`regex` 1.13.1, in a `std::sync::LazyLock`): `^(refs|ref-.*|writer-.*|fn(?:ref)?-[0-9]+(-[0-9]+)?)$`.
- `Diagnostic` has no hint field: append §8's hint in parentheses (wording beyond §8's templates is yours).
- `a.png` (70 bytes, 1×1 RGBA, CRCs checked): `printf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==' | base64 -d > tests/fixtures/md/images/a.png`.
- Build time: no test here runs the compiler, but the first `cargo test` builds the typst crates (T01's `l10n.rs`
  calls `typst::text::localized_str`) with `[profile.dev.package."*"] opt-level = 2` (T00): minutes cold, then fast.

## Notes for the coordinator
- **Auto-id termination (§1.3).** "Append `-1`, `-2`, … (first free)" never ends if reserved names count as taken (two
  headings "Writer": `writer-1`, `writer-2`, … all match `writer-.*`; likewise "Ref", "Fn 1"). T17 checks reservedness on
  the base only; `base-k` avoids explicit labels, keys and earlier auto ids (`refs-1`, `intro-1` unchanged). Confirm or amend.
- `label-invalid` "drops" is read as "not registered and `attrs.id = None`" (an invalid id must not reach typst).
- No spec assigns `pub use library::Library;` in `src/cite/mod.rs` although ir.md/references.md write `cite::Library`.
- ir.md §4's `resolve` doc comment and markdown-dialect.md §10 list `unknown-citation-key` as raised by resolve;
  references.md §8 says it never is. T17 follows references.md.

## Amendments
- **2026-10-26 (coordinator):** corrected §1.3's reserved footnote regex and
  confirmed the base-only reservedness rule for auto ids. Suffix candidates
  avoid explicit labels, keys and prior auto ids; this guarantees termination.
- **2026-10-26 (coordinator):** references.md §8 owns diagnostic classification:
  unknown targets raise `unresolved-reference`, never `unknown-citation-key`.
  Aligned the stale lists in ir.md §4 and markdown-dialect.md §10.
- **2026-10-26 (coordinator):** T12 is already merged. Adding `numbering.md`
  therefore adds its two automatic Markdown snapshots; existing snapshots stay
  unchanged. Resolver integration tests still build frontend-independent IR
  because the reference scanner belongs to T1B.
