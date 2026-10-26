# T10 — Front matter → DocMeta and config overrides

## Amendments
- 2026-10-26 (coordinator): markdown-dialect.md §2.2 records this brief's local decisions
  for keywords, author maps, empty tables and boolean numbering, and all exported helpers.
  config.md §3 clarifies that the frontend parses the abstract after T10 stores its text.

| | |
|---|---|
| Milestone | M1 |
| Size | S |
| Depends on | T01 |
| Parallel-safe with | T03, T04, T11, T13, T14, T17, T51 (wave 2) |
| Files you own | `src/markdown/frontmatter.rs` (replace stub; lift from presenter_rs/src/markdown/frontmatter.rs, tests inline) |

## Goal
`src/markdown/frontmatter.rs` turns a YAML front-matter block into `ir::DocMeta`: content keys go to
`DocMeta` fields, while config-like keys, the table maps and the abstract stay in `DocMeta.vars`.
T01's `Config::merge_front_matter` reads those `vars` entries to apply the per-document config
overrides. T12 (`markdown::parse`) and T18 (notebook metadata layers 3 and 4) call this module.
It is presenter's file with writer's key table: same YAML flavour, error positions, hints and
warnings.

## Read first
- `docs/spec/markdown-dialect.md` §2.2, both contract-change boxes included (**this is your spec**);
  §2.1 (`---` after line 1 is a thematic break), §9 first paragraph (grammar stays presenter's),
  §10 (codes `front-matter-empty`, `-unclosed`, `-author-list`), §11 (`minimal.md`)
- `docs/spec/config.md` §3 (the key table and the `cite_locale` box), §1, §4 (type names for the tests;
  every `impl Config` method is T01's), the Status contract change of 2026-10-25
- `docs/spec/ir.md` §1 (`DocMeta`, `Author`, the 2026-10-25 box), §4 last paragraph, §5 (`Error::FrontMatter`, `Diagnostic`)
- `docs/spec/notebook-mapping.md` §2.2 (how T18 calls you; context only)
- `docs/research/R8-presenter-inventory.md` §3.2 (`frontmatter.rs` row), §4.2 last bullet, §6, §10 step 11, §11
- `docs/research/R9-comrak-document-dialect.md` §15 (`parse_document`), §17 (the `FrontMatter` literal)
- `presenter_rs/src/markdown/frontmatter.rs` (1618 LOC, the file to lift) and
  `presenter_rs/src/markdown/mod.rs` lines 412–430 (`deck_meta`: how T12 will call `strip_delimiters` and
  `parse_document_front_matter` with the closing line)

## API to implement
```rust
// src/markdown/frontmatter.rs (T10). Binding signatures, markdown-dialect.md §2.2:
/// A `---` block starting on line 1 of `text`: (yaml without the delimiters, the rest, lines consumed).
pub fn split_front_matter(text: &str) -> Option<(&str /*yaml*/, &str /*rest*/, usize /*lines consumed*/)>;
/// The YAML of a front-matter block → `DocMeta` by the rules of this section
/// (`abstract_` stays `None`; the abstract text is `vars["abstract"]`).
pub fn parse_front_matter(yaml: &str) -> Result<DocMeta>;

// Kept public from presenter (R8 §3.2; same shapes; `Deck` → `Document`, PLAN §6 step 6). T12 needs
// the first two and T18 needs `meta_from_json` (see Notes for the coordinator):
pub fn strip_delimiters(literal: &str) -> &str;
pub fn parse_document_front_matter(yaml: &str, closing: usize, diagnostics: &mut Diagnostics) -> Result<DocMeta>; // was parse_deck_front_matter
pub fn swallowed_heading(yaml: &str) -> Option<(usize, &str)>;
pub fn meta_from_json(value: &serde_json::Value) -> Result<DocMeta>;
pub const FRONT_MATTER_EMPTY: &str = "front-matter-empty";
pub const FRONT_MATTER_UNCLOSED: &str = "front-matter-unclosed";
pub const FRONT_MATTER_AUTHOR_LIST: &str = "front-matter-author-list";   // new in writer
```
Types come from T01: `crate::error::{Diagnostic, Diagnostics, Error, Result}`, `crate::ir::{Author, DocMeta, SourceSpan}`.

Key rules (markdown-dialect.md §2.2 and config.md §3; "text" means the scalar exactly as written):

| key | goes to | accepted values; anything else is an `Error::FrontMatter` |
|---|---|---|
| `title subtitle date description lang` | `DocMeta.*` | any scalar → its text; `null` → `None` (no `vars` entry) |
| `keywords` | `DocMeta.keywords` | scalar → one keyword (local decision, typst's `keywords: str`); list of scalars → texts; `null` → `[]` |
| `author` | `DocMeta.authors` | scalar → `[Author { name: text, .. }]`; a list → as `authors` plus warning `front-matter-author-list`; `null` → nothing |
| `authors` | `DocMeta.authors` | list of scalars (→ `name`) or of maps with `name` (required) and optional `affiliation email id` (scalars → text, `null` → `None`); unknown map key, missing `name`, or a scalar instead of a list → error |
| `author` and `authors` both | — | `Error::FrontMatter("use either author or authors")` (exactly this, checked after the whole map is read) |
| `abstract` | `vars["abstract"]` (string) | scalar → text, dropped when empty; `null` → nothing; `abstract_` stays `None` |
| `markdown math references html pdf docx latex typst notebook` | `vars` | mapping as JSON, or `Value::Null` when empty (presenter's table rule) |
| `numbering` | `vars` | a boolean, a mapping, or `null`; any other value is an error (config.md §3 allows the bool) |
| everything else (`toc`, `bibliography`, `cite_style`, `csl`, `cite_locale`, `theme`, `template`, `reference_doc`, `latex_template`, `paper`, unknown keys) | `vars` | verbatim, by presenter's `JsonVisitor` rules, in the order written |

## Steps
1. Copy `presenter_rs/src/markdown/frontmatter.rs` over T00's stub of `src/markdown/frontmatter.rs`, tests included.
   Rename `DeckMeta` → `DocMeta`, `parse_deck_front_matter` → `parse_document_front_matter`,
   `deck_front_matter` (test helper) → `document_front_matter`, `presenter` → `writer` in doc examples.
   Delete every `reveal` path: the `reveal` arm of `EntrySeed`, `reveal` in the `tracing::debug!`
   call, `DeckMeta::reveal` in tests and docs. Rewrite the module doc against writer's specs
   (markdown-dialect.md §2.2, config.md §3). Keep its "YAML flavour" section: it is still accurate.
2. Key tables. `STRING_FIELDS` = `title subtitle date description lang` (presenter also had `author` and
   `theme`; `theme` now goes to `vars`). `TABLES` = the ten writer tables, each with an error example:
   `markdown: {smart_punctuation: true}`, `math: {html: mathml}`, `numbering: {headings: "1.1", figures: section}`,
   `references: {style: apa}`, `html: {theme: ./theme}`, `pdf: {paper: us-letter}`, `docx: {math: image}`,
   `latex: {top_level: chapter}`, `typst: {strict: true}`, `notebook: {show_prompts: false}`. `TableVisitor`
   accepts a `bool` only for `numbering` (its `expecting` then says "a boolean or a mapping, e.g. …").
   An empty table is stored as `Value::Null`, as in presenter. T01's lifted `overlay_table` skips values
   that are not objects, so `Null` counts as an empty mapping.
3. New `EntrySeed` arms (step 2's table): `keywords`, `abstract`, `author`, `authors`. Write the author rules as
   `Visitor`s called with `value.deserialize_any(..)` and raise failures with `de::Error::custom` from inside
   `visit_seq`/`visit_map`/`visit_str`, so serde_norway adds the key path and the value's position (probe in
   Hints). Track `author`, `authors` and "author was a list" on a small root struct that `RootVisitor` returns with
   the `DocMeta`. Do not add fields to `DocMeta`: `src/ir.rs` belongs to T01.
4. `parse_front_matter` returns the `DocMeta` alone. After the YAML is read, it returns the
   both-keys error. `parse_document_front_matter` keeps presenter's conditions for `front-matter-empty`,
   `front-matter-unclosed` and the "front matter runs from line 1 to …" error wording, but says "thematic
   break" where presenter said "slide separator", and "heading" where it said "slide". The
   not-a-mapping message becomes exactly ``expected a mapping at line L column C (the front matter runs from line 1
   to the `---` on line N; is its closing `---` missing, or was line 1 meant as a thematic break?)``. It also
   pushes the `front-matter-author-list` warning: its span is the line of the top-level `author:` key (the YAML
   index plus 2), or lines 1 to `closing` when that line is not found. The message wording is yours.
5. `meta_from_json`: the same key rules on a JSON object (T18's `metadata.writer`). Booleans and numbers
   for `STRING_FIELDS`, `abstract` and author fields become their JSON text. Keep presenter's
   `"{key}: {err}"` prefix. The same both-keys error applies. An `author` list is accepted silently
   (there is nowhere to push a warning).
6. Tests (same file, `#[cfg(test)] mod tests`), lifted and adapted:
   - Every `reveal` case becomes the unknown key `extra` or the table `html`. Recompute the column numbers
     in exact messages; presenter's line numbers stay valid when the structure is unchanged.
   - `slides`/`SlideSplit` cases become `numbering`/`html`.
   - In `config_tables_must_be_mappings`, `math: never` with `title` first becomes exactly
     ``math: invalid type: string "never", expected a mapping, e.g. `math: {html: mathml}` at line 3 column 7``.
   - Keep the test-local `comrak_options()` copy (T11's `markdown::comrak_options` may still be a stub in this
     wave). Change only what §1 lists for front matter; `front_matter_delimiter = Some("---")` is what matters.
   Then add the new tests named in the acceptance criteria.
7. Rewrite the merge tests against T01's `Config`. Compare through `matches!` or `serde_json::to_value(&config)`:
   config.md §4 does not promise `PartialEq` on `Config` or its enums. If one fails because of
   `src/config.rs`, do not edit that file; name the failing test in the PR (T01 owns it).
8. Fix the doc examples: `split_front_matter`, and the module example that merges
   `numbering: {figures: section}` into a clone and asserts
   `matches!(c.numbering.figures, writer::config::FigureNumbering::Section)`. If `writer::markdown::frontmatter`
   is not public (T00 writes the module lines), mark the examples `ignore` and say so in the PR.
   Run the checks below.

## Acceptance criteria
All tests are in `src/markdown/frontmatter.rs` `mod tests` (run them with `cargo test --lib markdown::frontmatter`)
and its doc tests (`cargo test --doc frontmatter`):
- Lifted, still passing: `empty_yaml_is_default`, `vars_preserve_author_order`,
  `scalars_for_string_fields_keep_their_yaml_text` (`date: 2026-10-24` → `"2026-10-24"`, `title: 42` → `"42"`),
  `quoted_block_and_null_string_fields`, `non_scalar_string_field_is_an_error`, `non_mapping_root_is_an_error`
  (`"front matter: expected a mapping"`), `yaml_syntax_errors_are_front_matter_errors`,
  `yaml_errors_get_hints_for_colons_and_tabs`, `error_lines_match_the_markdown_file`,
  `vars_keep_yaml_types_with_yaml_1_2_scalars` (`yes/no/on/off` are strings),
  `duplicate_keys_are_errors_at_the_repeated_key`, `integers_outside_64_bits_keep_their_digits`,
  `custom_tags_are_rejected_with_a_clear_message`, `crlf_yaml_parses`, `split_basic`,
  `split_requires_leading_delimiter_line`, `split_unterminated_is_none`, `split_crlf`, `split_edge_cases`,
  `split_then_parse`, `comrak_literal_shape_is_as_documented`, `strip_delimiters_on_comrak_literals`,
  `strip_delimiters_without_delimiters` (`"---\n---\n"` → `""`), `split_agrees_with_comrak_on_generated_documents`,
  `fixture_front_matter_agrees_between_comrak_and_text_split` (T01's `tests/fixtures/md/*.md`),
  `json_object_follows_the_front_matter_key_rules`, `json_errors_match_the_yaml_rules`.
- Adapted: `known_keys_go_to_fields_unknown_keys_to_vars_in_order`. YAML with `zeta`, the five string
  fields, `authors`, `keywords`, `theme`, `html: {theme: x}` and `alpha` sets the fields, and `vars` keys are exactly
  `["zeta", "theme", "html", "alpha"]`. `config_tables_must_be_mappings` covers all ten tables, keeps the exact
  `math` message of step 6, and stores `html:` and `math: ~` as `Null`. In `config_tables_stay_in_vars`, a merge
  of only empty tables leaves `serde_json::to_value(&Config::default())` unchanged.
  `document_front_matter_reports_swallowed_headings` keeps every presenter case and the exact not-a-mapping
  message of step 4.
- New: `author_string_becomes_one_author`. `authors_strings_and_maps` covers
  `authors: [A, {name: B, affiliation: EIT, email: b@x.org, id: 0000-0001}]`.
  `author_list_is_read_with_a_warning`: `parse_document_front_matter("title: T\nauthor: [A, B]\n", 4, ..)` gives two
  authors and one `(FRONT_MATTER_AUTHOR_LIST, 3, 3)`. `author_and_authors_is_an_error`: exactly
  `use either author or authors`, in either key order. `author_map_errors` covers an unknown key `orcid` (the
  message contains `authors` and `orcid`), a missing `name`, and `authors: A` (a scalar).
  `keywords_string_or_list` gives `["a"]` and `["a", "b"]`.
  `abstract_stays_in_vars`: `abstract: |\n  One paragraph.\n` → `vars["abstract"] == "One paragraph.\n"`,
  `abstract_ == None`. `empty_abstract_is_dropped` covers `""` and `null`.
  `config_like_keys_stay_in_vars_verbatim` covers the ten config-like keys and `bibliography` as a string
  and as a list. `numbering_bool_or_mapping`: `true` and a map are kept, `numbering: "1.1"` is an error.
- Merge tests through T01's `Config::merge_front_matter` (step 7): `merge_config_like_keys`:
  - `toc: 2` → `Toc::Depth(2)`, and `toc: true` → `Depth(3)`;
  - `numbering: true` → `HeadingNumbering::Pattern("1.1")`;
  - `theme`, `paper` and `cite_style` reach `html.theme`, `pdf.paper` and `references.style`.

  `merge_tables_key_by_key`: `numbering: {figures: section}` keeps a pre-set `headings`. `merge_is_idempotent`.
  `merge_unknown_table_key_is_an_error_with_hint`: `html: {thme: x}` → `Error::FrontMatter` containing
  `did you mean 'theme'?`, and the config is unchanged. `merge_rejects_yaml_1_1_booleans_in_config_tables`
  covers `markdown: {smart_punctuation: yes}`.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- `Config::merge_front_matter`, mapping config-like keys onto tables, unknown-key hints, path resolution
  against the document's directory: T01 (`src/config.rs`) and T16 (`pipeline::prepare`).
- These belong to T12 (`markdown::parse`):
  - parsing the abstract with `parse_blocks`, then removing `vars["abstract"]`;
  - reading the `FrontMatter` node;
  - the `---`/`---` empty-front-matter rule (presenter's `starts_with_empty_front_matter` and
    `mark_empty_front_matter` in `markdown/mod.rs`);
  - inserting the `Toc`.
- Notebook metadata layers, cell blanking, `notebook-front-matter-ignored`: T18. typst `writer-doc.with`: T50.
  `check --compat presenter` (`presenter-front-matter`): T1C. `comrak_options`: T11.

## Hints (verified)
- **serde_norway 0.9.42** (vendored-assets.md §7.3): presenter's entry point:
  ```rust
  serde_norway::Deserializer::from_str(&format!("\n{yaml}")).deserialize_any(RootVisitor)
  ```
  The leading `\n` makes reported lines source lines: YAML starts on line 2. `serde_norway::Error::location() -> Option<Location>`,
  with `Location::line()` and `column()` returning `usize` (`src/error.rs` lines 50–102). YAML 1.2 core schema; there is
  no timestamp type (`date: 2026-10-24` reads as `Some("2026-10-24")`, probe 2026-10-25).
- **Custom errors** (probe 2026-10-25, serde_norway 0.9.42): a `de::Error::custom("bad authors")` raised in a
  `visit_seq` reached through `next_value_seed(..)` → `deserialize_any(..)` on the YAML
  `"\ntitle: T\nauthors:\n  - a\n  - b\n"` displays as `authors: bad authors at line 4 column 3`
  (`fix_mark` in `src/error.rs`: the path, then the value's start). An error raised after the
  value was read is located at the root map instead.
- **comrak =0.57.0**: `pub fn parse_document<'a>(arena: &'a Arena<'a>, md: &str, options: &Options) -> Node<'a>`
  (`src/parser/mod.rs:48`), `pub type Arena<'a> = typed_arena::Arena<nodes::AstNode<'a>>` (`src/lib.rs:102`),
  `NodeValue::FrontMatter(String)`. The literal includes both delimiters and the following blank line
  (R9 §17). Presenter's four comrak-agreement tests (from `comrak_literal_shape_is_as_documented` to
  `split_agrees_with_comrak_on_generated_documents`, 5,456 generated documents) pass unchanged on 0.57.0
  (probe 2026-10-25).
- **serde_json 1.0.154** with `preserve_order` (vendored-assets.md §7.3), so `Map` keeps keys in the order
  written. Presenter's `vars_preserve_author_order` relies on it.
- **T01's lifted `error.rs`** (R8 §10 step 4, verbatim): `Diagnostic::warning(code, msg).with_span(span)`,
  `Diagnostics::new()`, `push`, `iter`. `Error::span()` and `column()` parse `at line L column C` out of a
  `FrontMatter` message, so keep positions in messages.
- Pure CPU, no compiler: the whole module runs in well under a second. The dev profile's
  `[profile.dev.package."*"] opt-level = 2` only matters for the typst-backed tests of other tasks.

## Notes for the coordinator
- PLAN §5's title says "and config overrides". config.md (Status box, 2026-10-25) gives `merge_front_matter`
  and all key-to-table mapping to T01. T10 only validates the shape of the table maps and stores
  config-like keys in `vars`, and its merge tests exercise T01's code.
- config.md §3's `abstract` row still says `DocMeta.abstract_ (parsed)`. Under markdown-dialect.md §2.2
  (box of 2026-10-25), T10 leaves the abstract in `vars["abstract"]`, and T12 and T18 parse it. Suggest
  "(parsed by the frontend)".
- markdown-dialect.md §2.2 lists `numbering:` among the table maps that must be mappings, while
  config.md §3 also allows a bool there. This task accepts both.
- §2.2 names two functions, but its own warnings and T12 need `strip_delimiters` and
  `parse_document_front_matter`, and T18's `metadata.writer` layer needs `meta_from_json`. This task keeps
  them public under those names, so T12's and T18's task files should cite them. Because
  `parse_front_matter` takes no diagnostics, notebooks get `front-matter-author-list` only if T18 calls
  `parse_document_front_matter`.
- `split_front_matter("---\n---\n…")` is `None`, matching presenter and comrak. The `---`/`---` rule of
  §2.2 is therefore T12's parse-level code. For a notebook cell it is unspecified.
- Local decisions to confirm in §2.2:
  - a `keywords` string becomes one keyword;
  - `authors` given as a scalar is an error;
  - author maps reject unknown keys and need a `name`;
  - empty tables are stored as `null`.
