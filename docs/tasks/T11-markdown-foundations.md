# T11 — Markdown foundations: comrak options, attributes, recovery, directives, colon pre-pass

## Amendments
- 2026-10-26 (coordinator): markdown-dialect.md §5.3 records sequential pre-pass/recovery,
  helper ownership and line-only diagnostics. T12 calls `check_unopened_directives`;
  it does not duplicate that check. Code-attribute lint shares T01's range parser,
  including its pipe alias for either range attribute (the §4.6 `CodeAttrs` contract).

| | |
|---|---|
| Milestone | M1 |
| Size | M |
| Depends on | T01 |
| Parallel-safe with | T03, T04, T10, T13, T14, T17, T51 (wave 2; T13 only reads `code.rs`) |
| Files you own | `src/markdown/options.rs` (replace stub), `src/markdown/attrs.rs` (replace stub; lift from presenter_rs/src/markdown/attrs.rs), `src/markdown/recover.rs` (replace stub; lift from presenter_rs/src/markdown/recover.rs), `src/markdown/directives.rs` (replace stub; lift from presenter_rs/src/markdown/render.rs), `src/markdown/code.rs` (additive: the `invalid-code-attr` check; T01 owns the file), `tests/probe_corpus.rs` (new), `tests/fixtures/md/probe/{main.md,main.r9-ast.txt,footnotes.md,footnotes.r9-ast.txt}` (new), `tests/fixtures/md/probe/cases/<slug>.md` (new, the 33 files of step 10), `tests/snapshots/probe_corpus__*.snap` (new) |

## Goal
The comrak layer every Markdown consumer builds on: the exact `comrak_options`, presenter's attribute grammar and its
recovery, directive info parsing, the `:::` colon pre-pass and its lint, the `invalid-code-attr` check, and a
differential snapshot of R9's probe corpus. T12, T18, T1B, T1C and T34 call these; T11 builds no IR but `ir_attrs`.

## Read first
- `docs/spec/markdown-dialect.md` §1, §3.1–3.3, §5.1–5.3, "Open points" (T11), "Deviations" (**your spec**);
  §4.6 (only its `invalid-code-attr` sentence: `CodeAttrs` is T01's), §9, §10 (codes), §11 (last paragraph)
- `docs/spec/ir.md` §1 (`Attrs`, `SourceSpan`), §5 (`Level::Info`, `Diagnostics`); `references.md` §1.1;
  `config.md` §2 `[markdown]`, §4; `vendored-assets.md` §7.3; your callers: `notebook-mapping.md` §2.4 step 1, §2.6
  (T18: `colon_prepass` with `front_matter_delimiter = None`, `parse_directive_info`), `typst-dialect.md` §10 (T34)
- `docs/research/R9-comrak-document-dialect.md` §2–§6, §12, §14 (the corpus), §15 (Verified API hints), §19;
  `docs/research/R8-presenter-inventory.md` §3.2 (`markdown/*` rows), §4.1–4.2, §10 rows 10–11, §11
- presenter_rs `src/markdown/`: `attrs.rs`, `recover.rs`, `render.rs` lines 674–762, 1471–1543, 1651–1694 (+ tests
  `pandoc_directive_info`, `split_info_is_quote_aware`, `colons_in_a_directive_line_are_reported`), `mod.rs` 381–410
  and its test at 576, `split.rs` 562–576 (`source_lines`)

## API to implement
```rust
// src/markdown/options.rs — §1, body copied exactly (mod.rs re-exports it; that `pub use` line is T00's)
pub fn comrak_options(md: &MarkdownConfig) -> comrak::Options<'static>;

// src/markdown/attrs.rs — §3.1 (lifted, R8 §3.2/§11)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedAttrs { pub id: Option<String>, pub classes: Vec<String>, pub pairs: Vec<(String, String)> }
impl From<&comrak::nodes::Attributes> for ParsedAttrs { /* lifted */ }
pub fn parse_attr_block(text: &str) -> Result<ParsedAttrs>;        // braces optional; unbalanced quote → Error::Other
pub enum ElementAttrs { Attrs(ParsedAttrs), Text, Invalid(String) }
pub fn read_element_attrs(block: &str) -> ElementAttrs;
pub fn is_valid_attr_name(key: &str) -> bool;
// §3.3 (unsigned in the spec): pairs → map, last value wins; a non-label `id` → dropped + `label-invalid` at `span`
pub const LABEL_INVALID: &str = "label-invalid";
pub fn ir_attrs(parsed: ParsedAttrs, span: Option<&SourceSpan>, diagnostics: &mut Diagnostics) -> Attrs;

// src/markdown/recover.rs — §3.2 (lifted)
pub const UNPARSED_ATTR_BLOCK: &str = "unparsed-attr-block";
pub fn recover_attr_blocks<'a, 't>(arena: &'a Arena<'a>, root: &'a AstNode<'a>, text: &'t str, opts: &Options,
                                   diagnostics: &mut Diagnostics) -> (&'a AstNode<'a>, Cow<'t, str>);
pub fn fence_attrs(info: &str) -> Option<(&str, &str, ElementAttrs)>; // lifted; T12 reads §4.6 fences with it

// src/markdown/directives.rs — §5.2, §5.3
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirectiveInfo { pub name: String, pub id: Option<String>, pub classes: Vec<String>,
                           pub args: Vec<(String, String)>, pub problems: Vec<String> }
pub fn parse_directive_info(info: &str) -> DirectiveInfo;
/// (rewritten text, its rewritten 1-based lines ascending); line count and every line ending unchanged.
pub fn colon_prepass<'t>(text: &'t str, opts: &Options) -> (Cow<'t, str>, Vec<usize>);
pub const DIRECTIVE_COLON_REWRITTEN: &str = "directive-colon-rewritten";
pub const INVALID_DIRECTIVE: &str = "invalid-directive";
/// §5.3's last sentence (step 8); spans hold lines only (`cell`/`file` `None`), like `recover_attr_blocks`'.
pub fn check_unopened_directives<'a>(root: &'a AstNode<'a>, diagnostics: &mut Diagnostics);

// src/markdown/code.rs — §4.6, additive: a warning per start/highlight/lines value `from_attrs` ignores
pub const INVALID_CODE_ATTR: &str = "invalid-code-attr";
pub fn check_code_attrs(attrs: &Attrs, span: Option<&SourceSpan>, diagnostics: &mut Diagnostics);
```
Open point decided: **in sequence**. T12 calls `colon_prepass` → a `Level::Info` `directive-colon-rewritten` per
returned line → `parse_document` → `recover_attr_blocks` → `check_unopened_directives`.

## Steps
1. Read T01's `src/error.rs` (`Diagnostic::{warning, error, new}`, `.with_span`, `Diagnostics::{new, push, info, warn}`),
   `ir.rs`, `config.rs`, `markdown/code.rs`, T00's stubs. Never edit `mod.rs`, `lib.rs`, `Cargo.toml`.
2. `options.rs`: copy §1's function verbatim. Lift presenter's `comrak_options_follow_the_dialect_exactly` and extend
   it: writer's additions set; `superscript`, `subscript`, `parse.smart` follow the config; `render.sourcepos` false
   even with `sourcepos: true` (presenter copied it). Add `extensions_writer_never_enables` (§4.1's "a test guards the
   options"): `underline`, `math_latex`, `multiline_block_quotes`, `highlight`, `insert`, `spoiler`, `greentext`,
   `subtext`, both `wikilinks_*`, `ofm_comments`, `cjk_friendly_emphasis`, `parse.ignore_setext` false; the URL
   rewriters and `header_id_prefix` `None`. Never name `shortcodes`/`phoenix_heex` (feature-gated fields).
3. `attrs.rs`: copy presenter's file; delete `SlideAttrs` + impl, `BACKGROUND_KEYS`, `remove_background`,
   `push_unique`, `normalize_key`, `is_css_color`, `CSS_COLOR_KEYWORDS`, `use crate::ir::Slide` and every test or
   helper using them (`separator_and_heading_forms_agree`, `slide_attrs`, `map`, the `slide_attrs` half of
   `invalid_attribute_names_are_dropped`, …). Keep `DATA_SHORTHANDS` as private `SLIDE_KEYS` (§9's presenter keys); in
   `Token::is_known_bare_key` replace `normalize_key(&key, "").1` by `key == "background" ||
   SLIDE_KEYS.binary_search(&key.as_str()).is_ok() || key.starts_with("data-") || key.starts_with("aria-")`
   (presenter's exact "known" set: `{auto-animate}` on a heading stays an `unparsed-attr-block`); add
   `slide_keys_are_sorted`. End the bare-key message after ``write `key=""` `` (drop the parenthetical naming `--- {…}`
   and `<!-- .slide: -->`); update its assertions here and in recover.rs `blocks_meant_as_attributes_are_reported`.
   Module docs without slides. Tests: `dialect_options()` → `comrak_options(&MarkdownConfig::default())`.
4. `attrs.rs` §3.3: `ir_attrs` with `typst_syntax::is_valid_label_literal_id`; the error names the id and the fix
   (`#fig-x`). Tests `ir_attrs_collects_pairs_last_wins` (`{k=1 k=2}` → `k: "2"`), `ir_attrs_rejects_invalid_labels`
   (`# H {#"with space"}` read by comrak → id dropped, one error at the span; `fig:x`, `größe`, `sec-intro` kept).
5. `recover.rs`: copy presenter's file; docs without slides or `--- {…}`; the doc link to `split::source_lines` becomes
   plain words; `split_lines` becomes `pub(crate)` (step 7). Tests: `dialect_options()` as in step 3; copy presenter's
   `source_lines` (split.rs:562–576) into the test module. Keep all 13 tests.
6. `directives.rs`, info: lift `DirectiveInfo` + `directive_info` (render.rs:1471–1543; renamed `parse_directive_info`;
   struct and fields `pub`; `Clone` added) and `InfoToken` + `split_info` (1651–1694). Writer addition (§5.2): when a
   named info ends with `}` and has a `{` outside quotes preceded by whitespace, split the part before it as now, read
   the block from that `{` to the end with `parse_attr_block` and merge it (its `#id` wins, classes appended, pairs →
   `args`, `class=`/`id=` handled like the pandoc branch: factor that branch into one helper); an `Err` goes to
   `problems`. Bare tokens stay classes verbatim (`#c1`, `.big`). Do not lift `is_notes_info`/`is_include_info`.
   Lift the tests `pandoc_directive_info` (build the expected struct with a helper) and `split_info_is_quote_aware`.
7. `directives.rs`, `colon_prepass` (§5.3): parse `text` with `opts` as given (local arena); select every `Paragraph`
   whose parent is the `Document` or a `BlockDirective` and whose first child is a `Text` starting with `:::`; on each
   line of its span (`start.line..=end.line`; `end.line - 1` when `end.column == 0`) matching `^\s{0,3}:{3,}`, keep
   the indent and colon run, strip a trailing `\s*:{3,}\s*$` from the rest, replace every `:` left in it by `&#58;`,
   and record the line if it changed. Split and reassemble with `recover::split_lines` so `\n`, `\r\n` and `\r` endings
   survive. Nothing changed → `(Cow::Borrowed(text), vec![])`. Optional fast path: no line has a `:` after its fence.
8. `directives.rs`, `check_unopened_directives`: one `invalid-directive` warning per line where (a) a `Text` starting
   with `:::` begins the line (first child of a `Paragraph`/`Heading`, or right after a `SoftBreak`/`LineBreak`, in any
   container: lists, quotes, alerts, footnotes, directives), or (b) the line matches `^\s{0,3}:{3,}` inside an indented
   `CodeBlock` (`fenced == false`: a tab before `:::` makes indented code). Never inspect fenced code or HTML blocks.
   Lift `Unopened`/`unopened_directive` (render.rs:710–762) for the message: a colon after the fence → write it as
   `&#58;` (the pre-pass skips lists, quotes, footnotes and a paragraph's later lines); pandoc closing colons → remove
   them; (b) → indent the fence with at most three spaces. Unlike presenter, report a line whatever its name.
9. `code.rs` (additive): `check_code_attrs` per §4.6 (start: integer ≥ 1; highlight: comma-separated `n` or `a-b`;
   lines: the same, `|` too), tested in a new `#[cfg(test)] mod check_tests`. T01's `from_attrs` is the reference: if
   `check_code_attrs_agrees_with_from_attrs` fails, change your check and say so in the PR.
10. Corpus in `tests/fixtures/md/probe/` (a subdirectory, so the `md/*.md` globs of T12/T16/T32/T60 never see it):
    `main.md` = R9 lines 685–785, `main.r9-ast.txt` = R9 791–956, `footnotes.md` = R9 965–975,
    `footnotes.r9-ast.txt` = R9 980–1001 (exact lines, each ending in `\n`). `cases/`: one input per bold case of
    R9 §14.4, in order: `directive-` + `colon-id`, `pandoc-attrs`, `entity-colon`, `colon-title`, `include-paths`,
    `empty-between`, `empty-tight`, `empty-forms`, `nested-longer`, `nested-same`, `unclosed`; then `leaf-candidates`,
    `table-caption`, `table-pandoc-caption`, `image-attrs`, `autolink-citations`, `bracketed-spans`, `heading-attrs`,
    `inline-attrs`, `fence-info`, `raw-html`, `loose-list`, `description-lists`, `math`, `smart`, `inline-footnotes`,
    `multiline`, `alerts`, `setext-breaks`, `escapes`, `links`, `front-matter`, `front-matter-late` (`.md`). Copy
    fenced inputs verbatim; write R9's abbreviations out (`/` = new line, `+` = next line, a blank line between
    variants, every fence with a body line and a closer); keep `[smart]`-style inputs but toggle no option for them.
11. `tests/probe_corpus.rs` (`//!` provenance: R9 §14): copy R9's `options()` (lines 616–644) as `r9_probe_options`
    and `dump` (646–664). `main_matches_r9_dump`: `main.md` dumped with `r9_probe_options()` == `main.r9-ast.txt`;
    `footnotes_match_r9_dump`: the same plus `leave_footnote_definitions = true`. `corpus_snapshots`: every
    `probe/*.md`, `probe/cases/*.md` dumped with `writer::markdown::comrak_options(&MarkdownConfig::default())` →
    `insta::assert_snapshot!` named `main`, `footnotes`, `case-<slug>`. `config_toggles_change_the_ast`: `main.md` with
    `superscript`, `subscript`, `smart_punctuation` on → snapshot `main-toggles` with both node kinds. Bless once
    (`INSTA_UPDATE=always`), then check each snapshot against R9 §14.4: only writer's options may explain a difference.

## Acceptance criteria
- `options.rs`: `comrak_options_follow_the_dialect_exactly`, `extensions_writer_never_enables` pass.
- `attrs.rs`: the 9 lifted tests left after step 3 (incl. the `…_agrees_with_comrak_on_random_blocks` pair, > 1,000
  accepted blocks each) and `slide_keys_are_sorted`, `ir_attrs_collects_pairs_last_wins`,
  `ir_attrs_rejects_invalid_labels` pass; `recover.rs`: all 13 lifted tests pass.
- `directives.rs`: `pandoc_directive_info`, `split_info_is_quote_aware` pass; `named_info_with_trailing_block`:
  `figure {#fig-x .wide width=80%}` → name `figure`, id `fig-x`, classes `[wide]`, args `[(width, 80%)]`, and
  `callout {title="Fig 1: x"}` → args `[(title, Fig 1: x)]`.
- `colon_prepass_rewrites_the_colon_trap`: R9 §14.4's first case → lines `[1]`, line 1 `:::figure #fig&#58;wrapped`,
  the re-parse has `BlockDirective { info: "figure #fig:wrapped" }` on lines 1–5, then paragraph `after`;
  `colon_prepass_strips_pandoc_closing_colons`: `::: {.note title="a: b"} :::` → `::: {.note title="a&#58; b"}`;
  `colon_prepass_reaches_directive_children`: `::::outer` / `:::inner: x` / `y` / `:::` / `::::` → lines `[2]`;
  `colon_prepass_leaves_lists_quotes_and_code_alone`: list, quote, fenced-code and second-line inputs → `(Borrowed,
  [])`; `colon_prepass_keeps_lines_and_line_endings` (CRLF input); `colon_prepass_is_idempotent` (these inputs).
- `unopened_directive_lines_are_reported`: one `invalid-directive` each for `- item\n  :::note: x`, `> :::note: x`,
  `Some text\n:::note: x`, `:::note: x\n---`, `\t:::note`; none for a fence holding `:::note: x`, an HTML block,
  `A :::b line`, `::x`, `\:::note: x`.
- `fence_lines_are_accounted_for`: for every `tests/fixtures/md/*.md` (T01's) and `md/probe/**/*.md`, after pre-pass →
  parse → recovery, each line matching `^\s{0,3}:{3,}` outside front matter, fenced code and HTML is a directive's
  first or last line or reported; line counts are kept; no panic (a math/code span over such a line: named exception).
- `check_code_attrs_agrees_with_from_attrs`: `start=2`, `highlight="1-3,5"`, `highlight=7`, `lines="1-3|5"` change the
  field and warn nothing; `start=0`, `start=-1`, `start=x`, `start=1.5`, `highlight=x`, `highlight="1-"`, `lines="a|b"`
  leave the default and give exactly one `invalid-code-attr`.
- `tests/probe_corpus.rs`: `main_matches_r9_dump`, `footnotes_match_r9_dump`, `corpus_snapshots`,
  `config_toggles_change_the_ast` pass; `tests/snapshots/probe_corpus__{main,footnotes,main-toggles}.snap` and the 33
  `probe_corpus__case-<slug>.snap` are committed.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
T12: the walker, `parse`/`parse_blocks`, raising `directive-colon-rewritten`, `invalid-code-attr`, `invalid-directive`
for a bare `:::` or a leaf with a body, `unclosed-directive`, `invalid-directive-arg`, §5.5, `Table:`/math labels,
`presenter-attr-dropped`. T01: `CodeAttrs`. T10: front matter. T1B: `@` refs, `:::include`. T1C/T71: `check`.

## Hints (verified)
- comrak 0.57.0 (`lib.rs:97,102`, `parser/mod.rs:48`): `pub fn parse_document<'a>(arena: &'a Arena<'a>, md: &str,
  options: &Options) -> Node<'a>`; `pub type Arena<'a> = typed_arena::Arena<nodes::AstNode<'a>>`; `Options<'c>`
  derives `Default, Debug, Clone`; a probe crate set every field §1 names (2026-10-25). Node API: R9 §15.
- Probe, comrak 0.57.0, §1's options (2026-10-25): `:::callout {title="Fig 1&#58; x"}` → `info: "callout {title=\"Fig
  1: x\"}"`; `::: {.note} :::` → paragraph text, the later `:::` opens `info: ""`; `\:::note: x` →
  `Paragraph[Escaped[Text(":")], …]`; in `- item\n  :::note: x`, `Some text\n:::note: x` and `> :::note: x` the line
  is a `Text(":::note: x")`; `:::note: x\n---` → setext `Heading`; `\t:::note` → `CodeBlock { fenced: false }`;
  `   :::note` → `fence_offset: 3`; an info `title="a\"b" k=&quot;q&quot;` reads `title="a"b" k="q"`.
- Lift check (scratch crate, 2026-10-25): attrs.rs + recover.rs after steps 3 and 5 pass all 22 lifted tests with
  writer's options (0.3 s, debug); heading attributes match presenter's options on 40,000 random blocks.
- R9 §14.2/§14.3 inputs and AST blocks at step 10's line ranges equal the probe's files and output byte for byte
  (checked 2026-10-25); `dump()` ends every node line with `\n`, so compare strings directly.
- typst-syntax 0.15.1: `pub fn is_valid_label_literal_id(id: &str) -> bool` (lexer.rs:1275, crate root): non-empty,
  every char `is_xid_continue`, `_`, `-`, `:` or `.`.
- insta 1.49.0: `insta::assert_snapshot!(name, value)` in `tests/probe_corpus.rs` writes
  `tests/snapshots/probe_corpus__<name>.snap`; `cargo-insta` is not installed here: use `INSTA_UPDATE=always`.
- T00's `.gitattributes` checks every file out with LF (the R9 comparison holds on Windows CI); test CRLF with inline
  strings. No compiler-backed tests here; `[profile.dev.package."*"] opt-level = 2` (T00) speeds the comrak loops.

## Notes for the coordinator
- The spec names no signatures for helpers §3.3, §4.6 and §5.3 require; T12's task file should use these:
  `attrs::{ir_attrs, LABEL_INVALID}`, `code::{check_code_attrs, INVALID_CODE_ATTR}`,
  `directives::{check_unopened_directives, DIRECTIVE_COLON_REWRITTEN, INVALID_DIRECTIVE}`, the lifted
  `recover::fence_attrs`, and the call order above. Their diagnostics carry line-only spans; T12 adds `cell`/`file`.
- §5.3's "a line matching `^\s{0,3}:{3,}`" is applied to the AST (step 8): the regex on raw lines misses `> :::x`,
  though the spec's parenthetical names quotes; fenced code and HTML blocks are never reported.
- §5.2 keeps bare `#c1`/`.big` tokens of a named info as classes verbatim (the literal rule, as in presenter); mapping
  them to `id`/classes would be a contract change. PLAN §5's "code attributes" for T11 means only the
  `invalid-code-attr` check (contract change 2026-10-24). The `md/*.md` globs of T12/T16/T32/T60 must stay
  non-recursive, or they read `md/probe/`.
