# T112 — Lists, block quotes, rules and definitions

**Status: brief, not ready for hand-off.** Decision D17, T103's readings and a measured prototype
come first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T103, T111 (and decision D17) |
| Parallel-safe with | nothing: the only task of wave 32 |
| Files you own | `src/design/defaults/lists.toml` and the list-item, `Block Text`, `Definition Term`, `Definition` and `Horizontal Rule` styles of `src/design/defaults/styles.toml`, with their bits in `APPLIED`; `assets/themes/default/styles/theme.css:96-110` (after T108 dropped `p` from :96); `lib.typ`'s list, enum, terms, quote and line rules; `src/typst/emit.rs` (`first_blocks`, the blank line before a nested list); `assets/docx/numbering.xml` and `src/docx/numbering.rs` (indents and hanging from the design); `assets/docx/styles.xml` (the new list-item style, `Block Text`, `Definition Term`, `Definition`, `Horizontal Rule`); `src/docx/body.rs` (the last item's direct after, the list-item style in `paragraph_style`, and R12's T100 `space_div`'s tight-item spacing); `tests/docx_div_spacing.rs` (`tight_list_items_in_a_div_keep_compact_spacing`, R12's T100); `src/html/design.rs` (list rules); the snapshots that change |

## Goal
Lists, block quotes, definition lists and horizontal rules follow R13 §3.4's models M4 (tight
lists) and M11 (list geometry) in all three formats: items S apart (D17), a list block with
before 0 and after P like a paragraph, a nested list S from its parent's text and from the next
item, the text I per level from the parent's text and the marker G before it, right-aligned,
with the same bullet glyphs. Tight list items take a list-item style of their own: `Compact`
keeps the table's y inset, which R12's `cell-inset` reads (R13 §3.4 M4 and M10), so it cannot
also carry the items' before 0 and after S. R12's `space_div` takes the new style's spaces for
tight items in a div.

## Read first
- `docs/spec/design.md` §0.4, §8.7 (the vocabulary, `Compact`)
- `docs/research/R13-single-source-defaults.md` §2.4 (inventory), §3.3 (row Tight list item),
  §3.4 M4 and M11, §4.3 (rows Lists, Block quote, Rule, Definition)
- T109's style keys `indent`, `border`, `fill` and `padding`, which the block quote and the rule
  use

## API to implement
Set at hand-off. The keys of `[design.lists]` are R13 §4.2's; D17 decides the list-item style's
name and values.

## Steps
1. Add the defaults and switch their bits on.
2. Typst: `list` and `enum` spacing, indent, body indent and marker box (M11); remove the
   blank line before a nested list. A tight list's P below is R12's `set block(below: P)`;
   Typst also puts a weak, attached `v(S)` before a tight list (see Hints): check the
   paragraph-to-list space against R12's T98 criterion 5.
3. Word: the item style (in `paragraph_style` and in `space_div` for items in a div),
   `numbering.xml`'s indents and right-aligned numbers written from the design by
   `src/docx/numbering.rs`, and a direct after = P on the last item of a top-level list.
   Update `tight_list_items_in_a_div_keep_compact_spacing` to the new style.
4. HTML: the M4 and M11 rules; remove the duplicated values from `theme.css`.
5. Accept the snapshots after reading them.

## Acceptance criteria
1. R12 §8's list and quote rows: Word (u) ↔ PDF within 0.3 pt, HTML within 0.3 pt after c
   (T102's harness).
2. The indent per level is equal in all three within 0.5 pt, and the bullet glyphs are the
   same.
3. Word's XML is as M4 and M11 state (exact): the item's before and after, the last item's
   direct after, `w:ind` left and hanging, `lvlJc` right.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- `Compact`'s table-cell role and the cell inset (T114).
- Code blocks and callouts (T113).
- LaTeX, unless the coordinator adds it at hand-off (T120 maps only T108's domains).

## Hints (verified)
- `theme.css` at main: `p, ul, ol, dl, blockquote, pre { margin: 0 0 1.15em; }` (:96),
  `li > ul, li > ol` .35em (:97), `li + li` .25em (:98), `dt`/`dd` (:99-100), `blockquote`
  (:102-109), `hr` (:110).
- `numbering.xml` at main: bullets in Symbol, Courier New and Wingdings by level, `w:ind` left
  720 twips per level with hanging 360; the ordered levels use `%1.`.
- `src/typst/emit.rs` at main: `first_blocks` (:629-648) writes `self.separator()` between an
  item's first paragraph and the rest; `list` (:650-674) writes `- ` or `+ ` per item.
- `src/docx/body.rs` at main: `paragraph_style` (:280) picks `Compact` in a tight list and in a
  table cell (:286, :296); `definitions` (:381); `list` (:451).
- `styles.xml` at main: `Compact` :6, `Block Text` :28, `Definition Term` :31, `Definition` :32,
  `Horizontal Rule` :94.
- Typst starts a list item's body at indent + marker width + body-indent (typst-layout 0.15.1
  `src/lists.rs:448`).
- The spacing model (R12, `src/html/design.rs` `vocabulary_css`) maps `Compact` to `th, td`
  only.
- Typst 0.15.1 puts `v(spacing)`, weak and attached, before a tight list or enum
  (typst-layout `src/rules.rs:131-138` and :150-157); with `list(spacing: S)` that is S.
- `src/docx/numbering.rs` loads `numbering.xml` verbatim (`BUILTIN`, :32-46) and adds instances
  per list (:108-134); indents from the design need code there.

## Before hand-off
- D17 decided: the list-item style's name and values.
- T103's list and quote readings in.
- The coordinator prototypes Typst's list spacing and the HTML rules and measures R12 §8's rows.
