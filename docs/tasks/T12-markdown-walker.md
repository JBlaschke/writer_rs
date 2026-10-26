# T12 — comrak AST → IR walker

## Amendments
- 2026-10-26 (coordinator): use T10's `strip_delimiters`/`parse_document_front_matter`
  for warnings; T11's sequential helpers now include `check_unopened_directives`.
  The steps and notes below reflect markdown-dialect.md §2.2/§5.3's recorded decisions.

| | |
|---|---|
| Milestone | M1 |
| Size | L |
| Depends on | T10, T11, T14 |
| Parallel-safe with | T15, T30, T31, T40 (wave 3; see the board) |
| Files you own | `src/markdown/mod.rs` (replace stub: T01's `parse` stub body; keep T00's `mod` and `pub use` lines), `src/markdown/walker.rs` (replace stub; partly lifted from presenter_rs/src/markdown/{render,mod,split}.rs, see Steps), `tests/markdown_snapshots.rs` (new), `tests/snapshots/markdown_snapshots__<stem>.snap` and `tests/snapshots/markdown_snapshots__<stem>_diagnostics.snap` (new, one pair per `tests/fixtures/md/*.md`) |

## Goal
The Markdown frontend. `markdown::parse` walks comrak's AST into an unresolved `Document` (blocks, inlines, implicit figures,
`Table:` captions, labelled display math, callouts, directives, raw passthrough, footnotes, assets, diagnostics);
`parse_blocks`/`parse_inlines` run the same walker on the abstract, captions, titles and T18's notebook cells. Consumers: T16,
T17, the emitters' snapshots, and T1B (adds `@` refs and `:::include` at your seams).

## Read first
- `docs/spec/markdown-dialect.md`, **your spec**: §2.1, §2.2 (empty front matter, the `abstract` var), §2.3, §4 (all), §5.1,
  §5.4, §5.5, §6, §9 (rows that drop things), §10, §11, "Open points" and its contract change. Also §1, §3.2–3.3, §5.2–5.3
  (what T11 hands you), §7.2 (why `Text` boundaries matter), §8 (not yet).
- `docs/spec/ir.md` §1–§5 (invariants 1–5, 8–10); `docs/spec/config.md` §2–§4 (`toc`, `[markdown]`, `merge_front_matter`);
  `docs/spec/references.md` §1.1, §1.3 (auto ids are T17's), §4.6; `docs/spec/notebook-mapping.md` §2.3–2.4 (T18's calls);
  `docs/spec/html-output.md` §6.1 (last paragraph).
- `docs/research/R9-comrak-document-dialect.md` §3, §6–§12, §15, §19; `R8-presenter-inventory.md` §3.2, §8, §10 rows 12, 15;
  presenter_rs `src/markdown/{render,mod}.rs` (what to lift).

## API to implement
```rust
// src/markdown/mod.rs (T12) — ir.md §4
pub fn parse(source: &SourceInfo, text: &str, config: &Config) -> crate::error::Result<Parsed>;

// src/markdown/mod.rs (T12) — markdown-dialect.md §2.3. Used for the abstract, notebook markdown cells (T18),
// alert and directive titles, code captions and `Table:` captions. No front matter, no :::include expansion.
pub struct Fragment<'a> { pub assets: &'a mut AssetRegistry, pub base_dir: &'a Path, pub diagnostics: &'a mut Diagnostics,
                          pub file: Option<&'a Path>, pub cell: Option<usize>, pub missing_asset_is_error: bool }
pub fn parse_blocks(text: &str, config: &Config, ctx: &mut Fragment) -> Result<Vec<Block>>;
pub fn parse_inlines(text: &str, config: &Config, ctx: &mut Fragment) -> Result<Inlines>;   // one paragraph's inlines
```
`Fragment` stays a struct with public fields (T18 builds it by struct literal). The walker is `pub(crate)`.

## Steps
1. **Inventory what landed** (use it, never re-implement it): T11's `src/markdown/{options,attrs,recover,directives,code}.rs`,
   T10's `frontmatter.rs`, T14's `src/assets/mod.rs`, T01's `src/{ir,error}.rs` (helpers, invariant checker,
   `Diagnostics::{info, warn, error}`). Shapes: Hints; on a name mismatch `main` wins.
2. **Front matter** (`mod.rs`; lift presenter `mod.rs` 226–275, 415–470): `starts_with_empty_front_matter` and
   `mark_empty_front_matter` verbatim (`---`/`---` on lines 1–2: no `front_matter_delimiter`, §2.2); parse with
   `comrak_options(&config.markdown)`; `FrontMatter` → T10's function that also raises `front-matter-empty`/`-unclosed`
   (`strip_delimiters` then `parse_document_front_matter`); `meta.vars.remove("abstract")`; `config.clone()` +
   `merge_front_matter(&meta)?`; options again from the merged `config.markdown`.
3. **Pre-pass and recovery** (`parse` and `parse_blocks`; fragments set `front_matter_delimiter = None`, notebook-mapping.md
   §2.4 step 1): `colon_prepass(text, &opts)` and one `Level::Info` `directive-colon-rewritten` per returned line;
   `comrak::parse_document` into a `comrak::Arena`; `recover_attr_blocks(..)`, continuing with its root;
   `check_unopened_directives(root, diagnostics)`. Keep presenter's `source_lines` (split.rs 560–575)
   of the **original** text: line numbers never change (§3.2, §5.3); `:::raw` bodies and §9's
   separator test read raw lines.
4. **`walker.rs` skeleton.** A `Walker` with `&mut Fragment`, `&Config`, source lines, the footnote map and walk state;
   methods under 80 lines (PLAN §6). Spans (§4.9): adapt `node_span` (render.rs 1967) to
   `SourceSpan { start_line, end_line, cell: ctx.cell, file: ctx.file.map(Path::to_path_buf) }`, `end_line = end.line - 1`
   when `end.column == 0`, never below `start_line`. Diagnostics carry a span and `path: ctx.file`; those of a nested
   `parse_inlines` get the enclosing node's span. §4.1's last row and `Raw`, `CommentInline`, `MultilineComment` →
   `Error::Other`; `HeexBlock`/`HeexInline`/`ShortCode` do not exist without their features.
5. **Footnotes** (§4.1, §6). First collect every `FootnoteDefinition` at any depth into `name → node` (the last of a name
   wins, as in comrak); never emit them; `total_references == 0` → `footnote-unreferenced`. A `FootnoteReference` whose name
   is not yet emitted → `Footnote { blocks: <walked definition>, label: Some(name), number: None }`, marking the name *before*
   walking (self references end); later → `FootnoteRef { label }`. `__inline_N` → `Footnote { label: None, .. }`, its blocks
   spanned like the block holding the reference (comrak puts the synthetic paragraph on line 1). `Features.footnotes`.
6. **Blocks** (§4.1, §4.2, §4.7). `Heading` minus §9's slide shorthands (ASCII case-insensitive; `data-`/`aria-` kept;
   `presenter-attr-dropped`); `BlockQuote`; `Alert` →
   `Callout { kind: alert_type.default_title().to_ascii_lowercase(), title: title.map(parse_inlines), blocks, attrs: {} }`;
   `List` (`start`, `tight` from the `List` node, never `Item`; `TaskItem{symbol}` → `checked: Some(symbol.is_some())`; §4.2's
   tight fix); `DescriptionList` (one `Vec<Block>` per `DescriptionDetails`); `ThematicBreak`. `Paragraph`, in order: §9
   separator (one-line paragraph whose raw line is `--` or matches `^(---|--)\s*\{.*\}\s*$` → dropped, info
   `presenter-separator`), display math (step 10), implicit figure (step 8), else `Paragraph`.
7. **Attributes and labels** (§3.3). One helper turns comrak `Attributes` and `ParsedAttrs` into `ir::Attrs` (pairs into the
   `BTreeMap`, last wins). Every id must pass `typst_syntax::is_valid_label_literal_id`, else `label-invalid`
   (`Level::Error`), id dropped. Links/images: `title` = the Markdown title, else a `title=` pair (removed from `pairs` either
   way), else `None`.
8. **Images and assets** (§4.3). Lift render.rs 116–128, 262–273 and 790–1105 with their tests into `walker.rs` (`local_asset`
   … `percent_decode` `pub(crate)`; `missing_asset_path` `pub`, its ``{what} `{url}` not found (looked for {path})`` message
   frozen), `RenderCtx` → `Fragment` (same field names), one `DirListings::enter()` guard per parse. Existing file →
   `asset:<id>` (query/fragment kept); missing → `missing-asset`, URL kept, or `Error::MissingAsset` when
   `ctx.missing_asset_is_error`. `width`/`height` not matching `^\d+(\.\d+)?(%|px|pt|cm|mm|in|em)?$` → dropped,
   `invalid-dimension`. Implicit figure: a paragraph whose only child is an `Image` with non-empty alt or an `#id` (in a
   `:::figure` body: `#id` only) →
   `Figure { kind: Image, caption: alt or None, attrs: { id, classes }, body: [Paragraph [Image { alt, url, title, attrs: { pairs } }]] }`.
9. **Tables** (§4.4). `None` alignment → `Default`; `TableRow(true)` → `head` (`header: true`); cells
   `colspan: 1, rowspan: 1, align: None`. Captions, per children list: a `Paragraph` whose first inline is a `Text` starting
   with `Table:` is a candidate; for each table in order bind the candidate right before it, else the one right after it
   unless a table follows that one. Strip `Table:` + whitespace from the first `Text` and a trailing `{…}` from the last
   (`parse_attr_block`; failure → `unparsed-attr-block`, text kept). Unbound candidates: paragraphs + `table-caption-orphan`;
   a last body row's first cell starting `: ` → `table-caption-pandoc-colon`. Caption or id →
   `Figure { kind: Table, caption (None if empty), attrs, body: [Table] }` spanning both blocks.
10. **Math** (§4.5). Children exactly `Math{display_math: true}` [+ `SoftBreak`] [+ `Text` matching `^\s*\{[^}]*\}\s*$`] →
    `Block::Math { source: literal.trim(), dialect: Latex, attrs }` (in lists and quotes too); a fence whose first info word
    is `math` → `Block::Math` with its attrs; other `Math` → `Inline::Math { source: literal }`; a display `Math` with a `{#`
    text in any other shape → `equation-label-misplaced`. `Features.math`.
11. **Fences** (§4.6, §4.8). `info` matching `^\{=([A-Za-z0-9_-]+)\}$` → `Raw { format: lowercase, text: literal }`
    (`raw-unknown-format` unless html, latex, typst, openxml, markdown). Else
    `CodeBlock { lang: first info word, text: literal minus one trailing "\n", attrs }`; attrs = `ast.attrs`, else
    `fence_attrs(info)` (adapt `fence_info`, render.rs 1744; `Invalid` → `unparsed-attr-block`); push T11's
    `invalid-code-attr` findings. `caption=` (removed, `parse_inlines`) or `#id` →
    `Figure { kind: Listing, caption, attrs: { id }, body: [CodeBlock] }`. Indented code: no lang, no attrs. `Features.code`.
12. **Raw HTML** (§4.8, §6). `HtmlBlock{block_type: 2}` and `HtmlInline` starting `<!--` are dropped; others →
    `Raw { format: "html" }`, relative `src`/`poster` rewritten by presenter's `rewrite_html_snippet` (render.rs 374) with one
    `crate::assets::HtmlScan` carried through the document (or `html_attr_values` per snippet if `StartTag` is private).
13. **Directives** (§5.1, §5.4, §5.5). `parse_directive_info(&info)`; `problems` and an empty name → `invalid-directive`
    (empty name: children unwrapped). §5.5's rows as written: `figure` (caption: `caption=`, else the walked body's last
    `Paragraph` when it has ≥ 2 blocks; kind: `kind=`, else inferred; unknown `kind=` or a body breaking ir.md §2 invariant 2
    → `invalid-directive-arg`, `Other`), `callout` (first class or `kind=`, default `note`), the five alert names, leaves
    `pagebreak`, `toc` (`depth` 1–6, else `invalid-directive-arg`; default `config.document.toc`'s depth, else 3),
    `bibliography` (second → `invalid-directive`, dropped), `raw` (`format=` or first class, lowercased; text = original lines
    strictly between the fences + `"\n"`; none → `invalid-directive-arg`, body as blocks), `notes`/`comment` (dropped
    silently), others → `Div { attrs: { id, classes: [name, classes…], pairs: args } }`; unconsumed args stay in `pairs`. Leaf
    with a body: leaf, body, `invalid-directive`. `unclosed-directive` when the span's last line does not match
    `^\s{0,3}:{3,}\s*$`. Adapt `check_unopened_directives` (render.rs 678–763) for paragraphs still starting like one.
14. **Include seam** (§8 is T1B's): every `:::include` →
    `fn include_directive(&mut self, info: &DirectiveInfo, span: SourceSpan, top_level: bool) -> Result<Vec<Block>>`, body
    `tracing::warn!` + `Ok(vec![])`, doc "T1B replaces this body". No diagnostic, no `Err` (PLAN §7: `includes.md` parses
    until T1B).
15. **Inlines** (§6). `Text` one-to-one: never merge neighbours; `Escaped > Text` stays its own `Text` (comrak coalesces all
    but escaped spans; T1B's §7.2 reads these boundaries). `SoftBreak`, `LineBreak`, `Emph`, `Strong`, `Strikethrough` →
    `Strikeout`, `Superscript`, `Subscript`, `Code { text, attrs }` (a following `Text` starting with `{=fmt}` makes it
    `Inline::Raw { format, text }`, `{=fmt}` stripped), `Link` (url as written), inline `Image` (step 8), `FootnoteReference`
    (step 5), `Math` (step 10), `HtmlInline` (step 12).
16. **Finish.** `parse`: `Toc { depth: n, span: None }` at index 0 when the merged `document.toc` is `Depth(n)` and no `Toc`
    exists at any depth; non-empty abstract → `meta.abstract_ = Some(parse_blocks(..))` with the merged config and
    `Fragment { base_dir: &source.base_dir, file: None, cell: None, missing_asset_is_error: false, .. }`; `Features` from the
    final IR (abstract, captions, titles, footnotes included); `assets = registry.infos(&source.base_dir)`, empty `includes`
    and `resolved`; missing diagnostic paths → `source.path`, stable sort by line (`finish_diagnostics`, mod.rs 536).
    `parse_inlines`: the first `Paragraph`'s inlines, `fragment-not-inline` unless exactly one paragraph (empty text → `[]`,
    silent). Write `// T1B: reference scanner (§7.2) runs here` at the end of `parse_blocks` and before the Toc step.

## Acceptance criteria
- `tests/markdown_snapshots.rs::every_fixture_is_snapshotted` (harness adapted from presenter's: `stable_json`, `relabel`,
  lines 68–100): each `tests/fixtures/md/*.md` (T01's 16, T17's `numbering.md`) with `Config::default()`: `Ok` →
  `markdown_snapshots__<stem>.snap` (JSON; `.source.path`, `.source.base_dir` → `[path]`; asset ids relabelled `000000000001`,
  …) and `markdown_snapshots__<stem>_diagnostics.snap` (one `{:#}` line each, fixture dir → `[fixtures]`); `Err` → its Display
  text, same redaction (PLAN §7; none today). Same file: `fixtures_satisfy_ir_invariants` (T01's checker silent; every block
  but an inserted `Toc` has a span), `every_diagnostic_has_path_and_span`, `parse_is_deterministic`.
- Unit tests in `src/markdown/walker.rs` on inline strings (base dir `tests/fixtures/md`), each covering every case its step
  names: `implicit_figures`, `image_assets_and_dimensions` (incl. `Error::MissingAsset` in error mode), `table_captions`
  (before, after, between two tables, orphan, pandoc colon), `display_math`, `code_fences` (every §4.6 row), `raw_blocks`,
  `alerts`, `lists`, `definition_lists`, `directive_table` (each §5.5 row), `directive_diagnostics` (bare `:::`, leaf with
  body, unclosed, unopened in a list item, `toc depth=9`, `kind=bogus`, second bibliography), `colon_prepass_info`
  (`:::callout {title="Fig 1: x"}` opens; one `Level::Info`), `footnotes` (incl. self-reference), `inline_nodes` (`\@key`
  keeps `Text("@")` apart), `labels`, `presenter_compat`, `front_matter_and_toc` (empty front matter; `toc: 2` → spanless
  `Toc` at 0; explicit `:::toc`; abstract), `spans`, `fragments` (`parse_inlines("a\n\nb")` → `fragment-not-inline`),
  `include_is_deferred`, `disabled_extensions_never_reach_the_walker` (`__u__`, `==h==`, `++i++`, `||s||`, `>>>`, `-# x`,
  `%%c%%`, `[[w]]`), `unexpected_nodes_are_errors` (`NodeValue::Raw` → `Error::Other`), and the six lifted tests of step 8
  (`case_listings_are_cached_while_a_guard_is_open`, `exact_lookups_need_no_listing`,
  `missing_asset_path_reads_the_frozen_message`, `image_urls_keep_query_and_fragment_and_are_percent_decoded`,
  `relative_url_classification`, `percent_decoding`).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass; no `unwrap()`/`expect()` outside tests.

## Out of scope
`@` scanning, `\@`, `:::include` expansion, `include-not-top-level`, `include-meta-ignored`, `Document.includes` (T1B;
`scan_includes`: T16). Auto ids, numbering, footnote numbers, `duplicate-label`, TOC model (T17); citations (T70). comrak
options, attribute recovery, directive info, colon pre-pass (T11); YAML (T10); `CodeAttrs` (T01); HTML scanner (T14); HTML
(T13); notebooks (T18); `check` (T1C). No edits to `Cargo.toml`, `src/ir.rs` or specs.

## Hints (verified)
- comrak `=0.57.0` (df, `attributes`):
  `pub fn parse_document<'a>(arena: &'a Arena<'a>, md: &str, options: &Options) -> Node<'a>`;
  `pub type Arena<'a> = typed_arena::Arena<nodes::AstNode<'a>>` (lib.rs 97, 102);
  `pub struct Ast { pub value: NodeValue, pub sourcepos: Sourcepos, pub attrs: Option<Box<Attributes>>, .. }` (nodes.rs 840);
  `NodeValue` is not `#[non_exhaustive]`; `Text(Cow<'static, str>)`; payloads in R9 §3.2; `AlertType::default_title()` →
  `"Note"`, `"Tip"`, `"Important"`, `"Warning"`, `"Caution"` (nodes.rs 612).
- comrak 0.57 (parser/mod.rs 2415–2490): with `leave_footnote_definitions` definitions stay in place, references get
  `ref_num`/`ix` in depth-first tree order and `name` = the definition's spelling, an undefined `[^x]` becomes plain `Text`;
  inline footnotes: parser/inlines.rs 1241–1270. `postprocess_text_nodes` (mod.rs 2536–2625) coalesces adjacent `Text`s but
  keeps `Escaped` apart under `parse.escaped_char_spans`. Sourcepos quirks: R9 §12. Borrow pattern (R9 §15, compiled on
  1.99.1): `for child in node.children() { let ast = child.data(); match &ast.value { .. } }`, holding the `Ref` only to read.
- typst-syntax `=0.15.1`: `pub fn is_valid_label_literal_id(id: &str) -> bool` (lexer.rs 1275, crate root; `fig:x.y-z_1` and
  `größe` pass, `a b` fails). html-escape `0.2.15`: `decode_html_entities(&str) -> Cow<str>`,
  `encode_quoted_attribute(&str) -> Cow<str>`. regex `1.13.1`.
- From presenter, lifted by T11 (attrs.rs 26–65, recover.rs 130, 538):
  `ParsedAttrs { id: Option<String>, classes: Vec<String>, pairs: Vec<(String, String)> }`,
  `parse_attr_block(s: &str) -> Result<ParsedAttrs>`, `fence_attrs(info: &str) -> Option<(&str, &str, ElementAttrs)>`,
  `recover_attr_blocks<'a, 't>(arena: &'a Arena<'a>, root: &'a AstNode<'a>, text: &'t str, opts: &Options, diagnostics: &mut Diagnostics) -> (&'a AstNode<'a>, Cow<'t, str>)`.
  Spec (§5.2–5.3): `colon_prepass(text, opts) -> (Cow<str>, Vec<usize>)`,
  `parse_directive_info(info) -> DirectiveInfo { name, id, classes, args: Vec<(String, String)>, problems }`.
- insta `1.49.0` (`json`, `redactions`): `assert_json_snapshot!(name, value, { ".source.path" => "[path]" })`,
  `assert_snapshot!(name, text)`. Build times: `[profile.dev.package."*"] opt-level = 2` (T00); the first `cargo test` also
  compiles typst (minutes, `docs/dev/build-times.md`); no test here needs the compiler, a browser or the network.

## Notes for the coordinator
- **T1B seams.** T1B owns only `refs.rs`/`include.rs` (PLAN §3.2), yet its scanner and `:::include` must be called from this
  task's files, and no spec allows those edits. This task leaves `include_directive` (drop + `tracing::warn!`; §10 has no "not
  yet" code) and a scanner comment; please allow T1B's two edits there.
- **Front-matter warnings.** T10 exports `strip_delimiters` and `parse_document_front_matter`; the latter raises
  `front-matter-empty`, `front-matter-unclosed`, and `front-matter-author-list`.
- **§5.3 diagnostics.** This task raises `directive-colon-rewritten` from the returned line list and calls T11's
  `check_unopened_directives` for unopened-line warnings; do not duplicate that lint in the walker.
- **First footnote reference.** §6 says `ref_num == 1`, but comrak counts in tree order, so a reference inside a definition
  can be walked first; "first in walk order" keeps ir.md §2 invariant 4.
- **Decided here, spec silent:** `:::figure kind=` breaking invariant 2 → `invalid-directive-arg`, `Other`; every unbound
  `Table:` paragraph → orphan; `parse_inlines("")` silent; `title=` leaves pairs; the `…_diagnostics.snap` name; abstract
  spans are relative to the abstract text (§2.3). Step 8's helpers have no spec home; T16, T18, T20 import them from
  `walker.rs`.
