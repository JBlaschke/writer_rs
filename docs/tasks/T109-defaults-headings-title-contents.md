# T109 — Headings, title block and contents

**Status: brief, not ready for hand-off.** Decisions D10 and D12 and a measured prototype come
first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T108 (and decisions D10, D12) |
| Parallel-safe with | T120 (wave 29; no shared file) |
| Files you own | the heading, title, abstract and contents styles in `src/design/defaults/styles.toml` and their bits in `APPLIED`; the style keys `case`, `letter_spacing`, `indent`, `border`, `fill` and `padding` (`src/design/`) and their emission in all three backends; `assets/themes/default/styles/theme.css:69-80`, :112-143 and :266-276; `print.css:8` (D10); `lib.typ`'s title block and heading rules; `assets/typst/writer-doc/design.typ` (`style-text`: case and tracking; the style block's inset, stroke and fill); `assets/docx/styles.xml` (Title to Abstract, Heading 1–6, TOC Heading, toc 1–6, Heading N Unlisted); `src/docx/design.rs` (`style_xml`'s run and paragraph properties: `w:caps`, `w:spacing`, `w:ind`, `w:pBdr`, `w:shd`); `src/html/design.rs` (the six keys, and rules for `.abstract-title` and `.toc-title`); the snapshots that change |

## Goal
Headings, the title block, the abstract and the contents take their look from the built-in
design in all three formats: the named styles `Heading 1`–`6`, `Title`, `Subtitle`, `Author`,
`Date`, `Abstract Title`, `Abstract`, `TOC Heading` and `toc 1`–`3` of R13 §4.2, with R13
§4.3's values. They are vocabulary styles already (design.md §8.7, §8.8), so they join §0.3's
model as soon as the built-in design declares them; this task fills their values, adds the two
style keys the abstract title needs (`case`, `letter_spacing`; Word's `w:caps` and `w:spacing`)
and the four block keys of R13 §4.2 that the abstract's inset and the title rule need, and that
T112 (block quote) and T113 (code, callouts) reuse: `indent`, `border`, `fill` and `padding`
(Word's `w:ind`, `w:pBdr`, `w:shd`; Typst's block inset, stroke and fill; CSS margin, border,
background and padding). It removes from the theme and the package the values the design now
carries.

## Read first
- `docs/spec/design.md` §0.4, §8.7 and §8.8
- `docs/research/R13-single-source-defaults.md` §2.2 (inventory), §3.3 (last row), §4.2, §4.3
  (rows Heading to Contents)

## API to implement
Set at hand-off. The keys follow the schema of design.md §2.2: `case` takes "upper" or
"smallcaps", `letter_spacing` a length (em allowed, T105); `indent` (left, right), `border` (side,
width, colour, gap), `fill` and `padding` as R13 §4.2 lists them.

## Steps
1. Add the styles to the defaults and switch their bits on.
2. Parse, resolve and emit `case`, `letter_spacing`, `indent`, `border`, `fill` and `padding` in
   the three backends: Word in `style_xml`, the PDF in `style-text` and the style's block, HTML
   in the generated style rules.
3. Remove the duplicated values from `theme.css`, `lib.typ` and `styles.xml`.
4. Accept the snapshots after reading them.

## Acceptance criteria
1. H1–H6 sizes, weights and colours are equal in all three formats.
2. Body → H1 and H1 → body: Word (u) ↔ PDF within 0.3 pt, HTML within 0.3 pt after c (T102's
   harness).
3. The title is set in the heading font at 26.5 pt in all three.
4. The contents have dot leaders and page numbers in the PDF and in Word; printed HTML follows
   D10 (screen HTML has no pages: design.md §0.4, exception 10).
5. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- LaTeX's headings: T120 runs in this wave and must not share a file; the coordinator assigns
  them (R13 is silent).
- Captions (T110) and the div classes (T117).

## Hints (verified)
- `lib.typ` at main: `title-block` (:52-85: 17 pt bold title, 9 pt affiliation, `v(1.5em)`),
  `styled-title-block` (:91), `title-styles` (:152), `abstract-block` (:154-164), the heading
  font in `layout` (:459-462), the outline's title (:190).
- `theme.css` at main: headings :69-80 (`h1` 1.85em to `h4`–`h6` 1em), title block and abstract
  :112-143 (`.title` 2.4em at :119), contents :266-276 (`.toc-title` at :276).
- `styles.xml` at main: Title to Abstract :7-12, Heading 1–6 :14-19, TOC Heading :38, toc 1–6
  :39-44, Heading 1–6 Unlisted :98-103.
- Word's contents already have a right tab with a dot leader (`src/docx/toc.rs:51`) and a
  PAGEREF field per entry (:78).
- Run properties of named styles are written in `src/docx/design.rs` `style_xml` (:481; `w:b`
  :597, `w:i` :609, `w:color` :621); `src/docx/styles.rs` holds only `vocabulary_paragraph`
  (:170) and `writer_selects` (:191).
- In the PDF, a style's case and tracking go through `style-text` (`design.typ:142` at main).
- `print.css:8` hides `nav.toc` in print today; D10 decides what replaces it.

## Wave 26's results (coordinator, 2026-11-22)

- T105 added the style keys `case`, `letter_spacing`, `indent`, `border`, `fill` and `padding`
  to the schema and the resolver, so step 1 only switches `HEADINGS` on and step 2 renders them.
- R13 gives no key or value for the title block's closing rule, and none for the Author name in
  bold with its affiliation at 0.9em and muted: layer 0's Author sets only `size` and `align`
  (`src/design/defaults/styles.toml:82-84`). Settle both with the owner before hand-off.
- T102's baselines on R12's build: Word's built-in Title is 21.92 pt off in an ad hoc probe, and
  in `tests/xfmt/expected/p10.txt` the printed title is 36.11 pt off and the contents 77.81 pt
  on screen.

## Before hand-off
- D10 (contents in printed HTML) and D12 (space above a heading: HTML's 2em or Word's 18 pt)
  decided.
- The coordinator measures the heading and title-block gaps with the harness on a prototype.
- The EIT report's contents rules and heading 4–6 workaround become double compensation when
  this task lands (R13 §4.6); the coordinator tells the template's owner.
