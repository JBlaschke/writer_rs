# T92 — Design styles restyle writer's Word definitions in place

| | |
|---|---|
| Milestone | M8 |
| Size | S |
| Depends on | T85, T89 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `src/docx/design.rs` (`style_xml`, `run_props`), `src/docx/styles.rs` (`vocabulary_paragraph`, `HEADER`, `writer_selects` (new), `install_design_styles`, `design_tests`), `src/docx/furniture.rs` (the header lines' style name), `src/check/design.rs` (`unused_styles`), `tests/design_styles.rs`, `tests/snapshots/design_styles__named_styles_word.snap`, `tests/snapshots/docx_design__design_letter_styles.snap`, `docs/spec/design.md` §7.1, §8.1, §8.5, §9.5, §10, §11, `docs/spec/docx-output.md` §2.1–2.2 notes, `docs/guide/{configuration,customization,outputs}.md` |

## Goal
A `[design.styles.<Name>]` table whose name is one of writer's built-in Word paragraph styles
(docx-output.md §2.3: `Title`, `Heading 1`, `TOC Heading`, `Heading 1 Unlisted`, `toc 2`,
`Compact`, `footer`, …) restyles that definition instead of replacing its `w:pPr`/`w:rPr`
wholesale: the keys the design sets replace the matching built-in properties and everything else
(`outlineLvl`, `ind`, `tabs`, `jc`, fonts, colours) keeps its built-in value. `writer check` no
longer reports such a name as unused, because writer's own paragraphs carry it; the same holds for
`Header`, the custom style of the header furniture's lines. A design-only
theme can then carry a document's heading, title-page and contents-list styling into Word without
a reference document.

## Read first
- `docs/spec/design.md` §7.1 (table), §8.1, §8.5, §10 (`design-style-unused`) — this is your spec
- `docs/spec/docx-output.md` §2.1–2.3 (vocabulary and ids), §4.13 (the TOC field lists outline
  levels 1–3, which is why `TOC Heading` and `Heading N Unlisted` carry `outlineLvl 9`), §5 (child
  order)
- `src/docx/design.rs` `style_xml`, `apply_tokens`; `src/docx/styles.rs` `install_design_styles`,
  `definition_xml`, `replace_definition`; `src/check/design.rs` `unused_styles`

## API to implement
```rust
// src/docx/styles.rs
/// The §2.3 paragraph name whose normalised key equals `name`'s, if any.
pub fn vocabulary_paragraph(name: &str) -> Option<&'static str>;
/// The paragraph style of the page furniture's header lines.
pub const HEADER: &str = "Header";
/// Whether writer's own paragraphs select a design style of this name.
pub fn writer_selects(name: &str) -> bool;
// src/docx/design.rs: unchanged signature, new contract (design.md §8.5)
pub fn style_xml(name: &str, style: &Style, design: &Design, ids: &StyleTable) -> String;
```

## Steps
1. `src/docx/design.rs`: `style_xml` seeds its `w:pPr`/`w:rPr` builders from the registered
   definition's children (a vocabulary name's built-in definition after `apply_tokens`; a custom
   definition has none), then applies the design's keys with `Props::set`; `w:spacing` merges the
   built-in attributes with the set ones. A small `seeded` helper replaces the three copies of the
   seeding loop; `spacing_attrs` keeps the asset's attribute order.
2. `src/docx/design.rs` `run_props`: a non-heading `w:rFonts` naming `Calibri` (Author, Date,
   which only undo Title's heading font) follows `fonts.body`, so a merged `Date` does not fall
   back to Calibri next to a Source Sans 3 body.
3. `src/docx/styles.rs`: `vocabulary_paragraph` (used by `install_design_styles`) and
   `writer_selects` (a vocabulary paragraph name, or `HEADER` compared case-insensitively, as a
   custom name resolves); `src/docx/furniture.rs` names the header lines' style through `HEADER`.
4. `src/check/design.rs`: `unused_styles` skips names for which `writer_selects` is true.
5. Specs and guides as listed under "Files you own"; the task board and PLAN.md gain T92.
6. Tests: `design::tests::vocabulary_styles_keep_unset_built_in_properties` (TOC Heading keeps
   `outlineLvl 9` under a design rPr, `toc 2` keeps `ind` and merges spacing, `Title` keeps
   `jc center`, `Heading 3` gets `b val=0`, a custom name is unchanged);
   `design_tests::vocabulary_names_restyle_writer_definitions_in_place` (`Heading 1 Unlisted`);
   `tests/design_styles.rs` (the merged `Date`; vocabulary names and `Header` are not reported
   unused);
   the two snapshots regenerated.

## Acceptance criteria
- `cargo test --lib docx::` and `cargo test --test design_styles --test docx_design` pass with the
  tests above; `tests/snapshots/design_styles__named_styles_word.snap` and
  `docx_design__design_letter_styles.snap` differ from before only in the `Date` definition
  (`before="0"`, the body font and `color 59636E` kept under `sz 20`, `after="170"`).
- Manual check (EIT Oxford design-only theme, 2 November 2026): with `[design.styles."TOC
  Heading"]` and `[design.styles."Heading 1 Unlisted"]` added to the theme, `writer check` prints
  no `design-style-unused` warning, the generated `styles.xml` keeps `<w:outlineLvl w:val="9"/>` in
  both and `<w:ind w:left="240"/>` in `TOC2`, and Word's contents list (file opened in Word for
  Mac, fields updated) lists neither "Table of Contents" nor "Document Control".
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Restyling character and table vocabulary styles (`Hyperlink`, `Verbatim Char`, `Table`): a
  `[design.styles]` table is a paragraph style; such a name still becomes a `<name> Paragraph`
  custom style with `docx-style-kind-mismatch`, as before.
- Removing a built-in property from a vocabulary style (an unset key keeps the built-in value;
  `keep_with_next = false` cannot drop a built-in `keepNext`): a reference document remains the
  escape hatch.
- Applying named styles to HTML and Typst headings or contents lists (design.md open points).

## Hints (verified)
- `StyleTable::definition_xml(id)` returns the appended raw definition, which `append_builtin`
  has already passed through `apply_tokens`; `replace_definition` swaps it by id
  (`src/docx/styles.rs`). `install_design_styles` registers every id before formatting, so a
  restyled `Heading 1` is in place when `TOC Heading` (basedOn `Heading1`) is formatted.
- `docx::xml::Props::set` replaces every child of a name and serialises in §5 order, so seeding
  the built-in children first and setting the design's afterwards needs no ordering code.
- `XmlNode.attrs` is a `BTreeMap`, so `element()` writes attributes alphabetically
  (`w:ascii`, `w:cs`, `w:hAnsi`); Word does not care, and `spacing_attrs` restores the asset's
  `before`, `after`, `line`, `lineRule` order for the merged spacing only.
- The TOC field `TOC \o "1-3" \h \z \u` (`src/docx/toc.rs`) lists paragraphs by outline level; a
  `TOC Heading` or `Heading 1 Unlisted` without `outlineLvl 9` inherits level 0 from `Heading1`
  and enters the list, which is what the EIT Oxford document showed on 2 November 2026.
