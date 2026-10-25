# T18 — Notebook frontend

| | |
|---|---|
| Milestone | M1 |
| Size | L |
| Depends on | T12, T13, T30, T40 |
| Parallel-safe with | T16, T50 (wave 4) |
| Files you own | `src/notebook/mod.rs` (replace T01's stub body; adapt presenter_rs/src/notebook/mod.rs); `src/notebook/nbformat.rs` (replace stub; lift presenter_rs/src/notebook/nbformat.rs minus slides); `src/notebook/convert.rs` (replace stub; adapt presenter_rs/src/notebook/convert.rs: metadata layers, attachments); `src/notebook/outputs.rs` (replace stub; lift helpers from presenter_rs/src/notebook/outputs.rs); `src/notebook/{options,tables,ansi}.rs` (replace stubs; new); `tests/notebook_snapshots.rs` (new; harness from presenter_rs/tests/notebook_snapshots.rs), `tests/nbformat.rs` (new; lift presenter_rs/tests/nbformat.rs, inline-JSON tests only), `tests/snapshots/notebook_snapshots__*.snap` (new); `tests/fixtures/ipynb/{minimal,outputs,options,tags,frontmatter,dataframe,footnotes}.ipynb` and `tests/fixtures/ipynb/images/` (new); `examples/notebook/{analysis.ipynb,README.md}` (new; adapt presenter_rs/examples/notebook/). Additive: `src/markdown/include.rs` (T1B's: the `cell_includes`/`cell_includes_in` stubs), `src/html/emit.rs` (T13's: the `ansi` line arm), `src/docx/body.rs` (T40's: the ANSI run arm) |

## Goal
`notebook::parse` turns an `.ipynb` into `Parsed`: markdown cells become dialect blocks of one document, code cells
`Block::Cell` with outputs as IR candidates (assets, pandas `Table`s, `Math`, ANSI text plus a private `ansi` pair), raw
cells `Raw`; `#|` options and tags drive captions, labels and visibility. Consumers: T16, every emitter, T1B (`parse_with`,
the include hook), T53 (`parse_cells`), T60 and T1C (fixtures). T18 also lands the HTML and DOCX ANSI arms.

## Read first
- `docs/spec/notebook-mapping.md` **§1–§6, §7.3 first bullet (Selection), §8, §9 (ANSI cells), Fixtures, Verified API
  hints, Open points, Deviations: this is your spec**; §7.1–§7.2 for context (T1B)
- `docs/spec/ir.md` §1 (`Cell`, `CellOutput`, `Features`; 2026-10-25 box: config keys go to `vars`), §2 (1, 2, 4, 8), §3–§5
- `docs/spec/markdown-dialect.md` §2.1 (Toc), §2.2 (abstract, T10's exports), §2.3 (`Fragment`, `parse_blocks`,
  `parse_inlines`; struct literal per the box after Open points), §4.3/§4.8 (image and raw-HTML assets), §5.2–§5.3, §8
- `docs/spec/config.md` §2 `[notebook]`, §3; `docs/spec/html-output.md` §1.5, §1.8 (`KatexOnly`), §1.15, §9 (T18 row);
  `docs/spec/docx-output.md` §1.1, §4.6, §5 (`RunProps`); `docs/spec/typst-dialect.md` §5.1; `docs/spec/vendored-assets.md` §7.3
- `docs/research/R10-notebooks-for-documents.md` §4.1–§4.3, §5, §7, §13.1, §13.3; `docs/research/R8-presenter-inventory.md`
  §3.3, §8, §10 steps 16–17; `docs/tasks/T10-front-matter.md` (API block: `meta_from_json`, `swallowed_heading`);
  presenter_rs `src/notebook/{nbformat,outputs,convert,mod}.rs`, `tests/{notebook_snapshots,nbformat}.rs`, `examples/notebook/`

## API to implement
```rust
// src/notebook/nbformat.rs: copy notebook-mapping.md §1's type block verbatim (Notebook, Cell, CellType with
// #[serde(other)] Unknown, Output tagged "output_type", MultilineString(pub String), MimeBundle), and:
pub fn parse_notebook(path: &Path, json: &str) -> crate::error::Result<(Notebook, Diagnostics)>;  // the lenient entry point
pub fn parse_outputs(raw: Vec<Value>) -> (Vec<Output>, Vec<String>);
pub fn mime_text(value: &Value) -> Option<String>;   // string or list of strings → String; anything else → None
// accessors (presenter's shapes, R8 §3.3): Cell::{tags() -> Vec<String>, is_hidden_source() -> bool, is_hidden_outputs() -> bool,
// raw_mimetype() -> Option<&str>, writer_options() -> Option<&Value>, span(index: usize) -> SourceSpan};
// Notebook::{language() -> &str, title() -> Option<&str>, authors() -> Vec<String>, writer_meta() / presenter_meta() -> Option<&Value>}

// src/notebook/mod.rs (§2.1)
pub fn parse(source: &SourceInfo, json: &str, config: &Config) -> Result<Parsed>;
pub fn parse_with(source: &SourceInfo, json: &str, config: &Config, opts: ParseOptions<'_>) -> Result<Parsed>;
pub fn parse_cells(source: &SourceInfo, json: &str, config: &Config, opts: ParseOptions<'_>) -> Result<(Parsed, Vec<CellParts>)>;
pub struct ParseOptions<'a> { pub includes: &'a mut dyn CellIncludes, pub nested: bool, pub selection: Selection }
pub trait CellIncludes {
    fn expand(&mut self, path: &str, span: &SourceSpan, notebook: &SourceInfo, config: &Config) -> Result<Parsed>;
}
#[derive(Debug, Clone, Default)]
pub struct Selection { pub cells: Option<Vec<CellRef>>, pub tags: Option<Vec<String>>, pub hide_input: bool, pub hide_output: bool }
#[derive(Debug, Clone, PartialEq, Eq)] pub enum CellRef { Index(usize), Id(String) }
#[derive(Debug, Clone, PartialEq)] pub enum CellKind { Markdown, Code, Raw }
#[derive(Debug, Clone, PartialEq)]
pub struct CellParts { pub index: usize, pub id: Option<String>, pub tags: Vec<String>, pub kind: CellKind,
                       pub body: Vec<Block>, pub input: Vec<Block>, pub outputs: Vec<Block> }
// src/markdown/include.rs (additive stubs, §2.6; T1B replaces the bodies)
pub fn cell_includes(notebook: &SourceInfo) -> Box<dyn crate::notebook::CellIncludes>; // ≡ cell_includes_in(notebook, &[])
pub fn cell_includes_in(notebook: &SourceInfo, chain: &[PathBuf]) -> Box<dyn crate::notebook::CellIncludes>;
// src/notebook/options.rs (§3.1)
pub struct OptionLines { pub yaml: String, pub lines: usize, pub rest: String }
pub fn split_options(source: &str, language: &str) -> OptionLines;
pub fn comment_markers(language: &str) -> (&'static str, Option<&'static str>);   // (open, close)
// src/notebook/tables.rs (§4.4)
pub struct HtmlTable { pub table: Block /* Block::Table */, pub caption: Option<String>, pub note: Option<String> }
pub fn parse_html_table(html: &str) -> Option<HtmlTable>;   // None = not a plain table
// src/notebook/ansi.rs (§5)
pub struct AnsiSpan { pub text: String, pub style: anstyle::Style }
pub fn parse(text: &str) -> Vec<AnsiSpan>;            // adjacent equal styles merged
pub fn strip(text: &str) -> String;                   // concatenated span texts
pub fn canonical(spans: &[AnsiSpan]) -> String;       // plain text, or `style.render()` + text + `style.render_reset()` per styled span
pub fn code_block(text: &str, class: &str) -> Block;  // CodeBlock { lang: None, text: strip(text), attrs: classes [class], pairs {"ansi": canonical} only if any span is styled }
pub fn colors(style: &anstyle::Style) -> (Option<anstyle::Color>, Option<anstyle::Color>);
pub fn to_html(spans: &[AnsiSpan]) -> Vec<String>;    // one HTML string per line (html-output.md §1.5 LINES); the HTML arm
pub fn rgb(color: anstyle::Color) -> (u8, u8, u8);    // the DOCX arm
pub const PALETTE: [(u8, u8, u8); 16];
```
`convert.rs` and `outputs.rs` items stay `pub(crate)` or private (Open points: `CellOptions` and validation are yours).

## Steps
1. Orient, no edits: `src/ir.rs` (`walk_blocks_mut`, `plain_text`, invariant checker), `src/markdown/` (T10–T12 APIs
   above, `colon_prepass`, `directives::parse_directive_info`), `src/html/emit.rs`, `src/docx/body.rs`, `src/typst/emit.rs`.
2. `nbformat.rs`: copy presenter's file with its inline tests; delete `SlideType`, `Cell::slide_type`,
   `UNKNOWN_SLIDE_TYPE` and slide tests; `presenter_overrides` → `writer_options` (reads `metadata.writer`);
   `PRESENTER_NOT_OBJECT` → `"writer-not-object"` on notebook and cell `metadata.writer`; add `writer_meta`, `authors`.
3. `ansi.rs` (pure; do it early): §5's `Perform` and SGR table, `canonical`, `colors` (bold→intense, then invert),
   `to_html` (nbconvert classes, `rgb(R,G,B)` styles, `crate::html::emit::escape`), `rgb` (palette, 6×6×6 cube, greys).
4. `tables.rs`: §4.4's four acceptance rules on `Html::parse_fragment`, then the grid (rowspan slots, padded rows, `th`
   rows, inline marks); R10 §13.3's `span()` helper for `colspan`/`rowspan`.
5. `options.rs`: §3.1 (quarto's markers plus `sh`, `javascript`, `typescript`); YAML via
   `serde_norway::from_str::<serde_json::Value>`; a private `CellOptions` resolving §3.2 key by key (`#|` over
   `metadata.writer`; wrong type → `cell-options-invalid` for that key; one `cell-option-unsupported` per cell); §3.3's
   precedence with `_`≡`-` tags. `[notebook] cell_options = false` keeps the lines in `source`.
6. `outputs.rs` §4.1: lift `apply_carriage_returns` and the stream-merging loop (`render_with`/`flush_stream`), emitting
   `ansi::code_block`s (set `span` after); drop stderr/errors for `warning`/`error: false` before merging.
7. `outputs.rs` §4.2–§4.3, §6: candidates in `mime_priority` order; lift `strip_math_delimiters`, `unescaped_dollars`,
   `dimension`, base64 decoding; hints `<stem>_cell<index>_out<j>.<ext>`; interactive bundles; empty fallback; §4.6;
   `text/markdown` via `parse_blocks`, footnote labels → `cell<index>-out<j>-n`, `:::include` lines blanked
   (`output-include`); re-span to `cell.span(index)` with diagnostics.
8. `outputs.rs` §4.5: lift `split_outside`; replace `restore_urls` by an IR walk restoring the lexical relative path in
   `Image`/`Link` URLs and `Raw` html attribute values (`output-file-outside`). Raw-HTML `src`/`poster` registration (§4.3
   `text/html`, §2.8): T12's helper if `pub(crate)`, else a private one over `crate::assets::html_attr_values` (Notes).
9. `convert.rs` §2.2: lift `find_front_matter`, `overlay`, `merge_config`, `in_context`, `sets_deck_metadata` (now: a
   `DocMeta` field or a config.md §3 key other than "anything else"), `empty_front_matter` (trigger: T10's
   `swallowed_heading`). Layers 2–3: T10's `meta_from_json` (layer 2 filtered to its eight keys + info
   `notebook-presenter-metadata`); layer 4: `split_front_matter` + `parse_front_matter`. Parse `vars["abstract"]` with
   `parse_blocks`, remove it, then `Config::merge_front_matter` on a clone.
10. `convert.rs` §2.4 steps 1–3 and §2.6: per markdown(-like) cell, one comrak scan of the `colon_prepass`ed text with
    `front_matter_delimiter = None`; record definitions by folded name, blank later duplicates (`footnote-duplicate`);
    split at top-level `include` directives (`\n` prefixes keep lines); append other cells' definitions unless the own
    text ends in an open block; keep each definition's diagnostics once; discard `footnote-unreferenced`.
11. `convert.rs` §2.3, §2.5: `Fragment { assets, base_dir, diagnostics, file: None, cell: Some(index),
    missing_asset_is_error: false }`; lift `decode_attachment`; hints `<stem>_cell<index>_<NAME sanitized>`.
12. `convert.rs` §2.7, §3.4: `Block::Cell` fields, listing figure, the five caption rows, group container after an
    input-only cell, sub-captions, `fig-alt`, `tbl-colwidths`, `label-kind-mismatch`, spans on every created block.
13. `convert.rs` §2.8: the raw-format table; Markdown formats reuse steps 10–11; else `raw-cell-skipped`.
14. `mod.rs`: §2.1 steps (1)–(9), §7.3's selection at (4b) (`notebook-cell-not-found`; `hide_*` act as `#|`), cross-cell
    pass, `footnote-undefined`, `Toc`, `Features`, `notebook-not-run`, `document.assets = assets.infos(&base_dir)`,
    presenter's `finish_diagnostics`. `parse` passes `cell_includes(source)`, `nested: false`, `Selection::default()`.
    `CellParts.input`/`outputs`: convert the code cell again with outputs, resp. input, hidden.
15. `src/markdown/include.rs` (additive, at the end; T16 adds `scan_includes` there in this wave: keep both): the stubs
    return a private struct whose `expand` is `Err(Error::Parse { path: notebook.path.clone(), line: Some(span.start_line),
    message: format!("cell {i}: :::include in notebooks arrives with T1B") })`, `i` = `span.cell`.
16. One arm each: `src/html/emit.rs`, where T13 writes an unhighlighted output block's escaped LINES, use
    `ansi::to_html(&ansi::parse(pair))` when `attrs.pairs["ansi"]` exists; `src/docx/body.rs`, one run per segment with
    `RunProps { style: Verbatim Char, bold, italic, strike, color, underline, shading }` per §5's DOCX bullet.
17. Fixtures: the seven notebooks of notebook-mapping.md "Fixtures", hand-written (nbformat 4.5, Python metadata,
    payloads shaped like R10 §4.1/§7.1); `images/` holds the image `outputs.ipynb`'s `text/markdown` embeds; take the
    outside file from `ls tests/fixtures/md/images/` (T01's).
18. Tests (criteria below). Harness: lift presenter's `source_in`, `stable_json`/`relabel` (asset ids → `000000000001…`),
    redact `.source.path`/`.source.base_dir`; HTML = `resolve` (`library: None, lenient: true`) then `html::emit::emit`
    with `KatexOnly`, `positions: false`, filter `asset:[0-9a-f]{12}` → `asset:[id]`.
19. Example: presenter's `analysis.ipynb` without `slideshow`/`metadata.presenter`, with `title`/`authors` front matter,
    `#| label: fig-rolling` + `fig-cap`, `tbl-cap` on the DataFrame, an `@fig-rolling` reference; README with the
    nbconvert command. Without a usable venv edit only markdown cells, metadata and `#|` lines, and say so in the PR.

## Acceptance criteria
- `cargo test --lib notebook`: lifted `nbformat` tests; `ansi::tests::{sgr_table, spec_html_examples (§5's three examples
  byte for byte), canonical_round_trip (IPython 9 traceback: `strip(pair) == text`, `parse(canonical(s)) == s`),
  bold_is_intense_then_invert, ansi256_and_rgb, non_sgr_sequences_dropped}`; `options::tests::{markers_table,
  python_r_c_options, shebang_or_blank_ends_block, invalid_yaml_ignores_all, wrong_type_ignores_key,
  hash_pipe_beats_metadata_writer, cell_options_off_keeps_lines, code_line_numbers}`; `tables::tests::{pandas_simple,
  repr_html_div_with_style, multiindex_grid (3 head rows, 5 columns), truncated_note ("100 rows × 2 columns"),
  styler_caption_and_nbsp, unescaped_styler_rejects, nested_table_rejects, bare_tr_without_thead}`;
  `outputs::tests::{carriage_returns, streams_merge, strip_math_delimiters_cases, last_candidate_guarantee,
  numbered_output_single_candidate}`.
- `cargo test --test nbformat`: presenter's inline-JSON tests pass (`writer-not-object` for `presenter-not-object`).
- `cargo test --test notebook_snapshots`, with `tests/snapshots/notebook_snapshots__<fixture>.snap` (unresolved IR),
  `…__<fixture>_diagnostics.snap` and `…__<fixture>_html.snap` committed for each fixture (`every_fixture_is_snapshotted`
  globs `tests/fixtures/ipynb/*.ipynb`), and: `invariants_and_spans` (T01's checker finds nothing; every diagnostic has a
  path; every block but the inserted `Toc` has a span); `minimal_is_not_run` (one `notebook-not-run`, no span);
  `outputs_fixture` (repeated PNG = one asset; codes `output-invalid`, `output-script`, `output-unsupported`,
  `output-file-outside`, `missing-asset`, `output-include`; two-formula LaTeX → `Raw`; `# Heading` + `text/plain` → one
  candidate); `no_escape_byte_in_emitters` (`outputs.ipynb`: HTML `main` has no `\u{1b}`, no `data-ansi`, and contains
  `ansi-red-intense-fg ansi-bold` and `ansi-default-inverse-fg ansi-red-bg`; DOCX `word/document.xml` has no `\u{1b}` and
  has `<w:color w:val="B22B31"/>`; `typst::emit` text has no `\u{1b}`); `options_fixture_rows` (every §3.4 row, `label-1`/`-2`,
  `Figure { kind: Other }` group, listing figure before its cell); `tags_fixture_precedence`; `frontmatter_fixture_layers`
  (layer order, `notebook-front-matter-ignored`, `Toc` at 0, parsed abstract, every §2.8 row, attachments,
  `notebook-attachment-skipped`); `dataframe_fixture_tables`; `footnotes_fixture` (one `Footnote` per name, cross-cell
  `FootnoteRef`, `[^Note]`≡`[^note]`, two distinct `[^1]`, `footnote-undefined` for the undefined name and each open-block
  cell with code text unchanged); `selection_and_parts` (index, id, tags, intersection, unknown ref →
  `notebook-cell-not-found`, `hide_input` drops the listing figure, with `nested: true` the parts' `body` concatenated ==
  `document.blocks`); `cell_include_hook_splices` (a fake `CellIncludes`: blocks spliced, cell lines kept, `includes`
  deduplicated); `example_notebook_is_clean` (`examples/notebook/analysis.ipynb`: no warnings after resolve).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
`CellOutput::pick` (T01); `:::include` expansion, `includes.ipynb` (T1B); cell markup and prompts (T13/T40/T30); `ansi-*`
CSS (T19); `check` (T1C, T71); typst includes, `writer-cells` (T53); LaTeX's U+001B test (T60); `build` wiring (T16).

## Hints (verified)
- **scraper =0.29.0**, `default-features = false` (its `default = ["main", "errors"]`, so no `Html::errors`):
  `Html::parse_fragment`, `root_element()` (synthetic `<html>`), `ElementRef::{wrap, value, attr, text, child_elements}`,
  `Element::name()`, `Node::{Text, Element, Comment, ..}` through `Deref` to the ego-tree node (R10 §13.1).
- **anstyle-parse =1.0.0**: `Parser::<DefaultCharAccumulator>::new()`, `advance(&mut performer, byte)`; `Perform` (no-op
  defaults): `print(&mut self, c: char)`, `execute(&mut self, byte: u8)`, `csi_dispatch(&mut self, params: &Params, intermediates:
  &[u8], ignore: bool, action: u8)`; `Params::iter()` → `&[u16]`. Default feature `utf8` = `Utf8Parser` (`src/lib.rs:330`); keep it.
- **anstyle =1.0.14**: `Style::new().fg_color(Some(c)).bg_color(..).effects(e)` (by value), `get_fg_color()`,
  `get_bg_color()`, `get_effects()`, `is_plain()`, `render()`, `render_reset()`; `Effects::{BOLD, DIMMED, ITALIC, UNDERLINE,
  INVERT, HIDDEN, STRIKETHROUGH}`, `insert`/`remove` return the new value (`#[must_use]`); `Color::{Ansi(AnsiColor),
  Ansi256(Ansi256Color), Rgb(RgbColor)}`, `Ansi256Color(pub u8)::{index, into_ansi, from_ansi}`, `RgbColor(pub u8, pub u8, pub u8)`.
- **caseless 0.2.2** `default_case_fold_str(&str) -> String`. comrak's private `normalize_label` (`comrak-0.57.0/src/
  strings.rs:341`): trim `isspace_char` (space `\t \n \x0b \x0c \r`), collapse `char::is_whitespace` runs to one space, fold.
- **comrak =0.57.0**: `parse_document(&arena, text, &options)`; `node.data()` is a `Ref<Ast>` (`.value`,
  `.sourcepos.start.line`); `first_child`/`last_child`/`children`; `NodeValue::{FootnoteDefinition(NodeFootnoteDefinition
  { name, total_references }), BlockDirective(Box<NodeBlockDirective { fence_length, fence_offset, info }>),
  CodeBlock(Box<NodeCodeBlock>) (`fenced`, `closed`), HtmlBlock(NodeHtmlBlock { block_type, literal })}`.
- **base64 0.23.1** `use base64::Engine as _; general_purpose::STANDARD_PAD_INDIFFERENT.decode(&bytes)`; **serde_norway
  0.9.42** `serde_norway::from_str::<T>(&str)` (YAML 1.2 core: `yes` is a string). **zip =9.0.5** (tests):
  `zip::ZipArchive::new(std::io::Cursor::new(bytes))?.by_name("word/document.xml")?` + `std::io::Read::read_to_string`;
  `docx::write` needs a fake `Renderer` returning `Err(Error::Other(..))`. **insta 1.49.0**: `assert_json_snapshot!(name,
  value, { ".source.path" => "[path]" })`; `with_settings!({ filters => vec![(r"asset:[0-9a-f]{12}", "asset:[id]")] }, {..})`.
- Dev profile `[profile.dev.package."*"] opt-level = 2` (T00); nothing here compiles typst, so the suite runs in seconds.

## Notes for the coordinator
- §2.2 names only `split_front_matter`/`parse_front_matter`; layers 2–3 use T10's `meta_from_json`. Layer 4 follows the
  spec, so notebooks never raise `front-matter-author-list` (only T10's `parse_document_front_matter` does).
- No spec exports the raw-HTML `src`/`poster` registration that §2.8 and §4.5 reuse; suggest T12 make it `pub(crate)`.
- Deviations say `MARKUP_CLASSES` gains `ansi-*`; html-output.md §1.15 lists none (§7 lists them for T19). Left alone.
- `@` references stay text until T1B's scanner (wave 5); PLAN §7's re-blessing rule names only md fixtures (add ipynb).
- The Status line gives T18 §1–§6 and §8, but `Selection` (§7.3) and `CellParts.input/outputs` are T18's `mod.rs`; this
  task implements them, T53 the typst side. `tests/nbformat.rs` is not named in PLAN §3.2.
