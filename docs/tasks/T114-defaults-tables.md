# T114 — Tables

**Status: brief, not ready for hand-off.** Decision D9, T103's readings and a measured prototype
come first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T103, T113 (and decision D9) |
| Parallel-safe with | nothing: the only task of wave 34 |
| Files you own | `src/design/defaults/table.toml` and its bit in `APPLIED`; `assets/themes/default/styles/theme.css:154-169`; `assets/typst/writer-doc/design.typ`'s `cell-inset` (R12's y inset from `Compact`; the x inset and the rule's stroke join it); the table rules this task adds to `lib.typ`'s `paragraph-layout`, beside R12's `empty-cell` rule; `assets/docx/styles.xml` (`Normal Table`'s `tblCellMar`, `Table`); `src/docx/tables.rs`; `src/docx/body.rs` (`separate_table`, `finish_block_sequence` and the direct spacing around tables); `src/typst/emit.rs` (tables); `src/html/design.rs` (table rules); the snapshots that change |

## Goal
Tables follow R13 §3.4 M10 in all three formats: three rules of 1.5/0.75 pt in the text colour,
a bold header, an x inset in em and the y inset from `Compact`'s before and after (R12), a row
pitch of line box + 2y + the rule's stroke, and space around a table under the larger-of rule,
written in Word as direct spacing on the neighbouring paragraphs.

## Read first
- `docs/spec/design.md` §0.4, §6.2 (`cell-inset`, which R12's `vocabulary-layout` applies)
- `docs/research/R13-single-source-defaults.md` §2.6 (inventory), §3.3 (row Table), §3.4 M10,
  §4.3 (row Table)

## API to implement
Set at hand-off: `[design.table]` `rule`, `header_rule`, `inset_x`, `header_bold`,
`header_fill` and `width` (R13 §4.2; D9 decides `width`'s default).

## Steps
1. Add the defaults and switch the bit on.
2. Typst: `stroke: none`, `table.hline` at the top, under the header and at the bottom, the x
   inset and the stroke in `cell-inset`, a bold header row.
3. Word: `tblBorders`, the header rule, `tblCellMar` left and right only, and the separator
   paragraph with an exact 0.7 pt line.
4. HTML: `border-collapse` and `padding: y x`; remove the duplicated values from `theme.css`.
5. Accept the snapshots after reading them.

## Acceptance criteria
1. Three rules of 1.5/0.75 pt in all three.
2. R12 §8's table rows: Word (u) ↔ PDF within 0.3 pt with the stroke counted, HTML within 0.3 pt
   after c (T102's harness).
3. `tblCellMar` holds x only; the space between two tables is exact in the XML.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Table captions (T110); a table ending a cell, which occurs only in raw OpenXML (R13 M10).
- LaTeX, unless the coordinator adds it at hand-off (T120 maps only T108's domains).

## Hints (verified)
- `styles.xml` at main: `Normal Table` (:53) has `tblCellMar` left and right 108 twips, top and
  bottom 0; `Table` (:54) has top and bottom borders `w:sz="8"` and a `firstRow` bottom border
  `w:sz="4"`, all `1F2328`.
- `src/docx/tables.rs` at main: `grid` (:135), `table` (:178) ensures the `Table` style (:195)
  and `Compact` for the cells (:239).
- `src/docx/body.rs:772-785`: `separate_table` and `finish_block_sequence` write `<w:p/>` after
  a table that another table or the end follows.
- `docx/furniture.rs:578-580` documents Word's minimum exact line, 0.7 pt.
- `src/typst/emit.rs` at main: `table` (:803), `table_cell` (:864).
- `theme.css` at main: tables :154-169, cell padding `.5em .65em` (:164).
- `src/html/emit.rs` writes `colgroup` widths (:1195-1210).

## Wave 26's results (coordinator, 2026-11-22)

- T105 named the table keys after R13 §4.2, not after this brief: `rules = {outer, header}`,
  `rule_color`, `inset_x`, `header_weight`, `header_fill` and `width` (`full` or `content`),
  in `src/design/defaults/table.toml`. Steps 2 to 4 render those names.
- T102 re-measured R13 rows 4.5 to 4.7 on R12's build: the row pitch is 25.84 pt in HTML and
  16.12 pt with a declared Compact, which reaches `td` (R13 §2.6).

## Before hand-off
- D9 decided (full text width or content width).
- T103's table readings in.
- The coordinator prototypes the stroke in Typst rows and measures.
