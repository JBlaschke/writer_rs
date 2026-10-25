# T01 — Core IR, config, errors, asset registry, l10n, CodeAttrs, frontend stubs, fixtures

| | |
|---|---|
| Milestone | M0 |
| Size | L |
| Depends on | T00 |
| Parallel-safe with | T22 (wave 1); every wave-2 task starts only after this merges |
| Files you own | `src/ir.rs` (rewrite of presenter_rs/src/ir.rs), `src/error.rs` (lift from presenter_rs/src/error.rs), `src/config.rs` (lift from presenter_rs/src/config.rs), `src/assets/registry.rs` (lift from presenter_rs/src/assets/registry.rs), `src/markdown/code.rs` (replace stub; new code), `src/resolve/l10n.rs` (replace stub; new), `assets/l10n/en.toml` (new), `src/markdown/mod.rs` (replace stub; → T12), `src/notebook/mod.rs` (replace stub; → T18), `src/typst/frontend.rs` (replace stub; → T50), `src/resolve/mod.rs` (replace stub; → T17), `tests/ir_roundtrip.rs` (new; lift from presenter_rs/tests/ir_roundtrip.rs), `tests/config.rs` (new; lift from presenter_rs/tests/config.rs), `tests/fixture_corpus.rs` (new), `tests/fixtures/md/{minimal,attributes,attribute-recovery,directives,pandoc-directives,figures,tables,callouts,code,math,references,footnotes,includes,raw,pagebreaks,presenter-deck}.md` (new), `tests/fixtures/md/images/{plot.png,diagram.svg}` (new), `tests/fixtures/md/include/{part-a,part-b,cycle-a,cycle-b}.md` (new), `tests/fixtures/ir/sample-document.json` (new), `tests/fixtures/config/writer.toml` (new), `tests/fixtures/bib/refs.bib` (new) |

## Goal
Make the contracts compile so the eight wave-2 tasks can start: the IR of `ir.md` (helpers, invariant checker, `pick`), errors, the config, the asset registry,
`CodeAttrs` and l10n (T13 needs both before T11/T17 exist), the three frontend stubs with final signatures, and the md/ir/config/bib fixtures. No parsing here.

## Read first
- `docs/spec/ir.md` §1–§6, all (your spec; every "Contract change" box changes doc-comment wording you write); `docs/spec/config.md` §1–§5 (of
  "Strict escalation" only the `apply_cli` bullet is yours); `docs/spec/markdown-dialect.md` §4.6, §11 (and §2–§9 for what each fixture exercises).
- `docs/spec/references.md` §4.1, §4.5, §1.3, "Fixtures", Deviations 2 and 7; `docs/spec/notebook-mapping.md` §4.2 and Deviations (grid rule);
  `docs/spec/html-output.md` Deviations "Localisation before T17", "`CodeAttrs` before T11"; `docs/spec/math.md` §10 paragraph "Documents";
  `docs/spec/vendored-assets.md` §4 (`l10n` row), §5.1, §7.3.
- `docs/research/R8-presenter-inventory.md` §3.1, §3.4, §6, §8, §10 steps 4–6, 9, 27, §11; `R9-…` §14.4; `R5-…` §2.8. presenter_rs: `src/{error,
  config,ir}.rs`, `src/assets/registry.rs`, `src/pipeline.rs:485` (`detect_kind`), `tests/{ir_roundtrip,config}.rs`, `tests/fixtures/md/*` (style).

## API to implement
```rust
// src/ir.rs — every type of ir.md §1 verbatim (fields, serde attributes, doc comments as amended), plus:
pub struct Parsed { pub document: Document, pub assets: AssetRegistry, pub diagnostics: Diagnostics }   // ir.md §4
impl Block { pub fn span(&self) -> Option<&SourceSpan>; pub fn paragraph(inlines: Inlines) -> Block; }
impl Inline { pub fn text(text: &str) -> Inline; }      impl Attrs { pub fn with_id(id: &str) -> Attrs; }
pub fn plain_text(inlines: &[Inline]) -> String;        // ir.md "Helpers"; `&Inlines` coerces (`&Vec` trips clippy::ptr_arg)
pub fn walk_blocks(document: &Document, f: &mut dyn FnMut(&Block));      pub fn detect_kind(path: &Path) -> Result<SourceKind>;
pub fn walk_blocks_mut(document: &mut Document, f: &mut dyn FnMut(&mut Block));
pub fn walk_inlines(document: &Document, f: &mut dyn FnMut(&Inline));
impl Features { pub fn merge(&mut self, other: &Features); }      impl Resolved { pub fn is_empty(&self) -> bool; }
impl CellOutput { pub fn pick(&self, target: &str) -> &[Block]; }  impl Document { pub fn invariant_violations(&self) -> Vec<String>; } // §2 checker
// src/error.rs — ir.md §5 verbatim: Error (Parse, FrontMatter, Config, Theme, Template, Style, MissingAsset, Compile { path, line: Option<usize>,
//   message, hints: Vec<String> }, Resource { what: &'static str, path, message }, Io, Json, Other), Result<T>, Level { Info, Warning, Error },
//   Diagnostic, Diagnostics (push, info, warn, error, iter, has_errors, has_warnings, len, is_empty, extend, into_vec, IntoIterator).
// src/assets/registry.rs — ir.md §3 verbatim (`read(&self, id: &str) -> Result<Vec<u8>>`).
// src/config.rs — config.md §4 verbatim: Config, Target, Toc, HeadingNumbering, FigureNumbering, EquationNumbering, HtmlMath, MathmlFont, PdfEngine,
//   ServeView, MathConfig, CliOverrides; load, from_toml_str, discover, merge_front_matter, apply_cli, targets_for, default_toml.
// src/markdown/code.rs — markdown-dialect.md §4.6 verbatim: CodeAttrs { numbered, plain, start, highlight }, from_attrs, is_highlighted.
// src/resolve/l10n.rs — references.md §4.1:
pub struct LangTag { pub lang: String /* "de" */, pub region: Option<String> /* "AT" */ }
impl LangTag { pub fn parse(tag: &str) -> Option<LangTag>; pub fn effective(meta: &DocMeta, config: &Config) -> Option<LangTag>;
               pub fn locale_code(&self) -> String; }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Term { Figure, Table, Listing, Equation, Section, Footnote, Bibliography, Contents, Page, Abstract, Note, Tip, Important, Warning, Caution }
pub fn term(lang: &LangTag, term: Term) -> String;     pub fn supplement(kind: LabelKind, lang: &LangTag) -> String;
pub fn caption_separator(lang: &LangTag) -> &'static str;
// src/resolve/mod.rs gains exactly: pub use l10n::{LangTag, Term, term, supplement, caption_separator};
// Frontend stubs (ir.md §4), final signatures; body Err(Error::Other("markdown support arrives in T12")) / "notebook … T18" / "typst … T50":
// src/markdown/mod.rs, src/typst/frontend.rs: pub fn parse(source: &SourceInfo, text: &str, config: &Config) -> crate::error::Result<Parsed>;
// src/notebook/mod.rs:                        pub fn parse(source: &SourceInfo, json: &str, config: &Config) -> crate::error::Result<Parsed>;
```
**Config tables.** config.md §4 fixes the table names; these field types are yours and bind later tasks: `DocumentConfig { lang: String, toc: Toc }` ·
`BuildConfig { out_dir: PathBuf, targets: Vec<Target>, typst_targets: Vec<Target> }` · `MarkdownConfig { smart_punctuation, sourcepos, references,
superscript, subscript: bool }` · `NumberingConfig { headings: HeadingNumbering, figures: FigureNumbering, equations: EquationNumbering }` ·
`ReferencesConfig { bibliography: Vec<PathBuf>, style: String, locale: String }` · `HtmlConfig { theme: String, standalone: bool }` · `PdfConfig {
template: PathBuf, paper: String, engine: PdfEngine, allow_packages: bool, system_fonts: bool, font_paths: Vec<PathBuf>, timestamp: String }` ·
`DocxConfig { reference_doc: PathBuf, math: DocxMath, update_fields: bool }` · `LatexConfig { template: PathBuf, top_level: LatexTopLevel, cite:
LatexCite, emit_bibliography: bool, code: LatexCode, tables: LatexTables, booktabs: bool, cleveref: bool, figure_placement: String, engine:
LatexEngine, unicode: LatexUnicode, cell_prompts: bool, wrap: u32 }` · `TypstConfig { strict: bool, package_paths: Vec<PathBuf> }` ·
`NotebookConfig { mime_priority, hide_input_tags, hide_output_tags, remove_cell_tags: Vec<String>, cell_options: bool, show_prompts: bool }` ·
`ServeConfig { host: String, port: u16, open: bool, view: Option<ServeView> }`. New unit enums (`#[serde(rename_all = "lowercase")]`, variants
spelled as config.md §2's strings): `DocxMath { Omml, Image }`, `LatexTopLevel { Part, Chapter, Section }`, `LatexCite { Biblatex, Natbib, Plain }`,
`LatexCode { Verbatim, Highlighted, Listings }`, `LatexTables { Longtable, Tabular }`, `LatexEngine { Pdflatex, Xelatex, Lualatex }`, `LatexUnicode
{ Auto, Map, Keep }`. An empty path means "built-in". Keep presenter's `pub const CONFIG_FILE_NAME` (`"writer.toml"`) and its two hint functions.

## Steps
1. Start from `main` with T00 merged. In each stub keep T00's lines (`//!`, `mod`, `pub use`) byte for byte; never edit `lib.rs`, `Cargo.toml`, `mod` lists.
2. `src/error.rs`: copy presenter's file; `presenter`/`deck` → `writer`/`document` in docs. Add `Level::Info` first (serde and `Display`: `info`),
   `Diagnostic::info`, `Diagnostics::info` (returns `&mut Diagnostic` like `warn`), `has_warnings` (any `Warning`; info never counts). Add `Compile`
   and `Resource` with ir.md §5's `#[error]` strings; in the exhaustive matches `path()` returns a non-empty `Compile.path` like `Parse`, `span()`
   maps `Compile { line: Some(l) }` to line `l`, `message()` returns `Compile.message`, `quoted_line()` reads both. Keep everything else and its tests.
3. `src/assets/registry.rs`: copy verbatim; only `read` returns `Result<Vec<u8>>` (ir.md §3), its test renamed `read_returns_bytes_and_reads_files_lazily`.
4. `src/ir.rs` types: transcribe ir.md §1 (no `Deck`/`Slide` code survives; R8 §10 step 5). `#[serde(default)]` on a struct needs `Default`: derive it
   on `ListItem`, `ColumnSpec`, `CiteItem`; write `impl Default for TableCell` with `colspan: 1, rowspan: 1` (a derived 0 breaks invariant 1) and
   ir.md's `CellOutput` default. Document every pub type, field and variant (`#![warn(missing_docs)]` + clippy `-D warnings`).
5. Helpers. `Block::span` covers every variant (`Cell` → `cell.span.as_ref()`); constructors set `span: None`. `plain_text`: `Text`/`Code` text, `Math`
   source, breaks → `" "`, container inlines, `Link`, `Span`, `Image.alt` recursed, `Ref`/`Cite` → `plain_text` of `resolved.text`/`resolved.inlines`
   or `"[?]"`; `Footnote`, `FootnoteRef`, `Raw` give nothing. Walks: pre-order over `meta.abstract_` then `blocks`, children in IR field order; a
   block's own inlines (incl. captions, titles, terms, table cells head → body → foot) are searched for `Inline::Footnote`, whose blocks are walked
   where the note stands, then come child blocks (incl. every candidate of every `Cell` output). `walk_inlines` visits an inline before its children
   (container inlines, `Link.inlines`, `Image.alt`, `Span.inlines`, `Ref.supplement`, `CiteItem.prefix/suffix`, `Footnote.blocks`), never `resolved`.
6. Invariant checker: one message per problem, prefixed with a path (`blocks[3].body[0]`), for ir.md §2 items 1 (levels; tight-list contents; spans
   ≥ 1; the grid rule per section, counting slots covered by earlier rows' `rowspan`), 2 (figure shapes; nesting only inside `Figure { kind: Other }`),
   3 (`asset:` + 12 `[0-9a-f]`), 9 (ids non-empty, no whitespace; duplicates are resolve's `duplicate-label`), 10 (raw formats lowercase).
7. `pick` per notebook-mapping.md §4.2 (`html`→`html`, `typst`→`typst`, `docx`→`openxml`, `latex`→`latex`; every `Block::Raw` anywhere in the
   candidate's block tree must match; `Inline::Raw` never disqualifies; `&[]` when none qualifies). `detect_kind`: presenter's (`md`, `markdown`,
   `ipynb`, ASCII case-insensitive) plus `typ` → Typst; the error names the file and the four extensions.
8. `src/config.rs` types: copy presenter's file; delete `DeckConfig`, `SlidesConfig`, `SlideSplit`, `MathMode`, `PluginId`, the `reveal` field, every
   reveal/plugin function and each helper left unused (`canonical_json`, `scalar_text`; clippy denies dead code). Keep `json_map` (on
   `katex_options`), the TOML-date helpers, `overlay_table`, the hint helpers and their tests. Add the tables and enums above, all `#[serde(default,
   deny_unknown_fields)]`, `impl Default` with config.md §2's values, deriving `Debug, Clone, PartialEq, Serialize, Deserialize` (+ `Copy, Eq` on unit
   enums, `Hash` on `Target`). Hand-written serde for TOML and `serde_json::Value`: `Toc` (false ↔ `Off`, `true` → `Depth(3)`, integer 1..=6 ↔
   `Depth(n)`, else an error naming the forms), `HeadingNumbering` (false ↔ `Off`, `true` → `Pattern("1.1")`, string ↔ `Pattern`), `view:
   Option<ServeView>` (`""` ↔ `None`; an unknown value yields serde's "unknown variant `x`, expected …" so the hint works).
9. Config methods. `from_toml_str`, `load`, `discover`: presenter's minus plugin checks; paths stay as written. `merge_front_matter`: on a clone, one
   overlay map per table from `meta.vars[<t>]` for `markdown math numbering references html pdf docx latex typst notebook` (`null` = empty; another
   non-object → `Error::FrontMatter("<t>: expected a mapping")`, except `numbering: <bool>` = `{headings: <bool>}`); then config.md §3's dedicated
   keys, which win over the maps: `toc` → `document.toc`, `bibliography` (string or list) → `references.bibliography`, `cite_style`/`csl` (both →
   `Error::FrontMatter("use either cite_style or csl")`) → `references.style`, `cite_locale` → `references.locale`, `theme` → `html.theme`, `template`
   → `pdf.template`, `reference_doc` → `docx.reference_doc`, `latex_template` → `latex.template`, `paper` → `pdf.paper`. Apply each with
   `overlay_table` (unknown keys → hint); assign the clone only if all succeed; never modify `vars`; idempotent. `apply_cli`: `targets` → both target
   lists; `theme`, `template`, `reference_doc`, `latex_template` as above; `standalone` → `html.standalone`; `port`/`host`/`open` → `serve`; `view` →
   `Some(v)`; `strict: Some(b)` → `typst.strict = b`; `engine`, `allow_packages` → `pdf`; `out` ignored; `None` keeps. `targets_for`: Typst →
   `typst_targets`, else `targets`. `default_toml()`: a `const` with config.md §2's TOML block byte for byte (comments included, final `\n`).
10. `src/markdown/code.rs` (new; presenter's `CodeAttrs` is HTML-specific): `plain` = class `plain`; `start` = `start=` as `u64` ≥ 1, else 1;
    `highlight` = ranges of `highlight=` then of `lines=` (`|` read as `,`); a value counts only if every comma item is `n` or `a-b` with
    `1 ≤ a ≤ b`; `numbered` = class `numbered` or a valid `lines=`. Keep the range parser a private fn (T11 adds `invalid-code-attr` here).
11. `assets/l10n/en.toml`, one per line: `abstract = "Abstract"`, `note = "Note"`, `tip = "Tip"`, `important = "Important"`, `warning = "Warning"`,
    `caution = "Caution"`. `src/resolve/l10n.rs` per references.md §4.5: `parse` (split on `-`/`_`; 2–3 ASCII letters → `lang` lowercased; skip a
    4-letter script; a following 2-letter subtag → `region` uppercased; no valid first subtag → `None`); `effective` (`meta.lang`, else
    `config.document.lang`, else `"en"`; empty = absent; a present invalid tag → `None`); `locale_code`. `term`: typst keys `figure table raw equation
    heading footnote bibliography outline page` via `typst::text::localized_str`; writer keys from `l10n/<lang>-<REGION>.toml`, `<lang>.toml`,
    `en.toml` via `crate::vendor::get`, parsed as `BTreeMap<String, String>` (skip an unreadable file; never panic). `supplement`: Heading →
    Section, Figure/Table/Listing/Equation/Footnote → their term, Other → `""`. `caption_separator` by `lang`: `zh` `"\u{2003}"`, `fr`
    `".\u{a0}\u{2013} "`, `ru` `". "`, else `": "`. Add the `pub use` line to `resolve/mod.rs` (no `resolve()` stub: T17's).
12. Frontend stubs: each `parse` with its final signature, a doc comment naming the owner, `let _ = (source, text, config);` (or `json`), the error
    above, and `#[cfg(test)] fn stub_names_its_owner()` in the same file (the owner deletes it with the body).
13. Markdown fixtures (inputs only; snapshots are T12's/T16's/T1B's): LF, UTF-8, final newline, front matter first with `title:` and YAML comments
    `# <file>: markdown-dialect.md §n` plus one behaviour per line. Content = §11's row, plus: `minimal.md` front matter `title`, `authors` (list),
    `abstract`, then one heading and one paragraph only · `figures.md` R9 §14.4 "Image attribute variants" with every URL an existing file
    (`images/plot.png`, `images/diagram.svg`; T32/T41 build it), `![](images/plot.png)` alone, `:::figure {#fig-panels caption="Two panels"}` around
    `{#fig-left}`/`{#fig-right}` images, `{width=huge}` · `math.md` math.md §10 "Documents" verbatim (front-matter macros; `$\frac{a}{b}$`;
    `$$\sum_{i=0}^{n} i$$ {#eq-sum}` and a sentence with `@eq-sum`; the `aligned` source in a ```` ```math {#eq-aligned} ```` fence; `$\hat{x}
    \vec{v} \dot{y}$`; `$$x = 1 \tag{3} \label{eq:t}$$`; `$\RR^n + \half{x}$`), then §11's other items · `references.md` also references.md
    "Fixtures" (unnumbered-heading ref, `@eq-x[]`, `@sec-x[Chapter]`, `{.page}`, `## Results and discussion` linked as `[the
    results](#results-and-discussion)`, `[@fig-a; @fig-b]`, one duplicated label, `{#fn-1}`), every referenced label present, cite keys from
    `refs.bib` (`[@doe2020; @lee2019, p. 12]{.prose}`, `[-@knuth1984]`, `[see @doe2020]`) · `footnotes.md` also a repeated `[^a]` and a note
    referencing another note · `includes.md` `:::include include/part-a.md` (part-a: `title: Part A` ≠ includes.md's, body `:::include part-b.md`;
    part-b: a heading and `![Part B](../images/plot.png){#fig-part-b}`), `:::include include/cycle-a.md` (→ `cycle-b.md` → `cycle-a.md`), the same
    directive inside a list item · `raw.md` `<img src="images/plot.png">` · `directives.md` ends with the unclosed directive. `images/plot.png` =
    byte copy of presenter_rs `tests/fixtures/md/images/ok.png` (70 B); `images/diagram.svg` hand-written, ≤ 300 B.
14. `ir/sample-document.json`: the **resolved** form, ir.md §6 enlarged to every `Block` and `Inline` variant, both `ListKind`s, all `FigureKind`s, every
    `OutputKind`, a task item, `RefForm::Page`, a `Footnote` + `FootnoteRef` in body paragraphs only (numbers 1..=n, `resolved.footnotes == n`), ref
    texts with a literal U+00A0, image URL `asset:32debefb8003`, `assets` = `infos` of `register_bytes(plot.png)` (`origin: "inline:70"`); write by
    hand, deserialize, overwrite with `serde_json::to_string_pretty` + `"\n"`. `config/writer.toml`: every config.md §2 key non-default. `bib/refs.bib`:
    R5 §2.8's `BIB` minus its leading newline. Lift `tests/ir_roundtrip.rs` and `tests/config.rs` (drop reveal/plugin tests; invert
    `front_matter_non_object_tables_are_plain_vars` into step 9's rule); write the tests below.

## Acceptance criteria
- `tests/ir_roundtrip.rs`: `sample_is_canonical_pretty_json`, `sample_covers_every_variant`, `sample_satisfies_invariants`, `sample_is_resolved` (every
  `Ref`/`Cite` resolved; footnotes 1..=n in `walk_inlines` order), `sample_asset_id_is_reproducible_from_plot_png` (`32debefb8003`, `infos ==
  document.assets`), `spec_example_deserializes` (ir.md's ```` ```json ```` block, no violations), `json_shapes_match_the_spec` (`"code_block"`,
  `"thematic_break"`, `"soft_break"`, `{"kind":"ordered","start":1}`, `{"kind":"result"}`; `None` spans/numbers, empty `includes`/`resolved` absent),
  `table_cell_defaults_to_span_one`, `features_merge_is_fieldwise_or`, `doc_meta_vars_preserve_order`, `invariant_violations_are_detected` (a broken
  document per step-6 rule), `rowspan_covered_slots_are_valid`, `plain_text_rules`, `walks_are_preorder_and_reach_footnotes_and_candidates`,
  `walk_blocks_mut_edits_in_place`, `pick_takes_the_first_renderable_candidate` (four targets over `[Raw html, image Paragraph, text CodeBlock]`),
  `pick_ignores_inline_raw`, `pick_of_no_candidates_is_empty`, `detect_kind_by_extension` (`a.MD`, `b.markdown`, `c.ipynb`, `d.typ`; `e.txt` → error).
- `tests/config.rs`: `default_equals_empty_toml`, `defaults_match_spec_section_2`, `default_toml_parses_to_default`, `default_toml_matches_config_md`
  (equals config.md's ```` ```toml ```` block), `fixture_sets_every_key_to_a_non_default_value` (every key of `serde_json::to_value`),
  `unknown_table_suggests_build` (`[buidl]` → `did you mean 'build'`), `unknown_variant_gets_a_hint` (`[math] html = "katx"` → `did you mean 'katex'`),
  `toc_and_heading_numbering_forms`, `serve_view_empty_is_none`, `load_error_names_the_file`, `discover_walks_up_and_stops_at_the_first_file`,
  `fixture_inputs_do_not_discover_a_repo_config`, `html_theme_precedence_covers_every_pair` (8 subsets of file / front matter / CLI),
  `front_matter_keys_map_to_tables` (each config.md §3 row), `front_matter_dedicated_key_beats_table_map`, `front_matter_numbering_bool_sets_headings`,
  `front_matter_non_mapping_table_is_an_error`, `front_matter_unknown_key_is_an_error_with_hint_and_no_change`, `front_matter_merge_is_idempotent`,
  `cli_overrides_map_every_field` (`strict: None` keeps `typst.strict`), `targets_for_kinds`, `toml_dates_in_free_form_maps_become_strings`.
- Unit tests: `src/error.rs` every presenter test plus `info_is_neither_warning_nor_error`, `compile_and_resource_display_per_spec` (`doc.typ:3 unknown
  variable`; `reference document 'ref.docx': not a zip`), `compile_errors_are_located`; `src/assets/registry.rs` every presenter test;
  `src/markdown/code.rs` `defaults`, `plain_and_numbered_classes`, `start_valid_and_invalid` (`10` → 10; `0`, `x` → 1), `highlight_ranges` (`"1-3,5"` →
  `[1..=3, 5..=5]`), `lines_alias_numbers_and_highlights` (`"1-3|5"`), `invalid_values_are_ignored` (`"3-1"`, `"a"`, `"1,,2"`; invalid `lines=` leaves
  `numbered` false), `is_highlighted_uses_displayed_numbers` (`start=10 highlight="11"`); `src/resolve/l10n.rs` `lang_tag_parse_cases` (`de-AT`,
  `zh-Hans-CN` → zh/CN, `x` → None, `en_us` → en/US, `EN`, `de-419` → de, `""` → None), `effective_falls_back_meta_config_en`, `term_en_de_fr_ca`
  (Figure → `Figure`/`Abbildung`/`Fig.`; de Section → `Abschnitt`; en Contents → `Contents`, Page → `page`; de Note → `Note`), `supplement_per_kind`,
  `caption_separator_cases` (zh, fr, fr-CA, ru, en), `every_l10n_file_parses_and_en_has_every_writer_key`; each stub's `stub_names_its_owner`.
- `tests/fixture_corpus.rs`: `markdown_fixtures_exist_and_follow_conventions` (step 13's rules), `fixture_cases_are_present` (a substring per §11
  behaviour and step-13 addition), `math_fixture_holds_the_math_md_sources`, `include_tree_is_complete`, `images_exist`, `refs_bib_has_the_r5_entries`
  (`@article{doe2020,`, `@book{knuth1984,`, `@inproceedings{lee2019,`, `{\TeX}book`), `no_writer_toml_above_fixtures`.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
Front matter → `DocMeta` (T10); comrak options, attributes, directives, colon pre-pass, `invalid-code-attr`, `md/probe/` (T11); the walker and Markdown
snapshots (T12, T16, T1B); `assets/{mod,emit}.rs` (T14); `resolve()`, `anchor`, numbering, `lang-invalid`, `cite::Library` stub, `md/numbering.md` (T17);
`ipynb/` (T18); `typ/` (T50); relative config paths, `--strict` escalation, `dump --default-config` (T16); `md/include-chain.md` (T1B); citations (T70).

## Hints (verified)
- typst 0.15.1 (`typst-library-0.15.1/src/text/lang.rs`, read 2026-10-25; re-exported by `typst-0.15.1/src/lib.rs:36` `pub use typst_library::*`):
  `typst::text::localized_str(lang: Lang, region: Option<Region>, key: &str) -> &'static str` (memoized; lang+region → lang → English; panics only
  for a key missing in English, so pass only the nine keys); `impl FromStr for Lang` (2–3 ASCII letters, lowercased), `for Region` (2 ASCII letters,
  uppercased), `Err = &'static str`; `Lang::ENGLISH`. English: `figure = Figure`, `table = Table`, `equation = Equation`, `bibliography =
  Bibliography`, `heading = Section`, `outline = Contents`, `raw = Listing`, `page = page`, `footnote = Footnote`; `de`: Abbildung, Abschnitt; `fr`:
  `figure = Fig.`; `fr-CA.txt` holds only `email`, so `fr-CA` falls back to `fr`. Caption separator (`src/model/figure.rs:631–640`): `CHINESE =>
  "\u{2003}"`, `FRENCH => ".\u{a0}– "` (U+2013), `RUSSIAN => ". "`, else `": "`.
- Asset ids (presenter `src/assets/registry.rs`; blake3 1.8.7, mime_guess 2.0.5): `blake3::hash(bytes).to_hex()[..12]`;
  `mime_guess::from_path(&path).first_or_octet_stream().essence_str()`; presenter's `ok.png` hashes to `32debefb8003` (its `tests/ir_roundtrip.rs`).
- serde 1.0.229, serde_json 1.0.154 (`preserve_order`, vendored-assets.md §7.3): `Map` keeps insertion order; a missing `Option` field is `None`;
  `#[serde(default)]` on a struct needs `Default`; `Block::Cell(Cell)` (internally tagged) is `{"type":"cell", …}`. toml 1.1.8: `de::Error` shows line
  and column; serde's "unknown field `x`, expected one of `a`, `b`" / "unknown variant `x`, expected …" feed `unknown_key_hint` (config.rs:647).
- `crate::vendor::get(path: &str) -> Option<Cow<'static, [u8]>>` (T00, vendored-assets.md §5.1) reads `assets/` from disk in debug builds. This task
  links `typst`: the first build compiles ~300 crates (~1 min); keep T00's `[profile.dev.package."*"] opt-level = 2`. Shell: quote globs; no `timeout`.

## Notes for the coordinator
- config.md §4 names the tables but not their field types or the `[docx] math`/`[latex]` enums; this file fixes them (later task files should cite
  them), plus three local choices: `--to` sets both target lists; a dedicated front-matter key beats a table map; `cite_style` + `csl` is an error.
- No spec assigns making relative config paths absolute (config.md §3; latex-output.md §7.3 "absolute after config loading"; docx-output.md
  `DocxOptions.reference`). T01 stores them as written, like presenter (whose `pipeline::load_config` resolves `out_dir`): T16's `prepare` should.
- `sample-document.json` uses id `32debefb8003`, not ir.md §6's illustrative `3f9a1c2b7e0d`, so T30/T40 can `register_bytes(plot.png)`. ir.md §3's
  `read` returns `Vec<u8>` (presenter: `Cow<[u8]>`); T14's lifted `emit.rs` still compiles (both deref to `&[u8]`).
