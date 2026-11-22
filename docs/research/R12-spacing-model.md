# R12: one spacing model for Word, the PDF and HTML (`@writer/doc` 0.5.0)

**Status:** research note with verified reference code (2026-11-21). It backs
design.md §0.3, §3.3, §5.3, §6.2, §6.3, §7.4, §8.3 to §8.5, §8.7, §8.8 and
§9.4, typst-dialect.md §4.2 (the contract), and tasks T96–T101.
Word was measured in Word for Mac 16 on DOCX files that `writer build` wrote;
the PDF and HTML numbers come from the same sources built with §5's code in
place (all four gates green, 99 test targets). Nothing here is a contract:
where this note and the spec disagree, the spec wins.

## 1. The question

One theme, three outputs, and the PDF looked much tighter than Word and HTML.
The cause is not one wrong number. Up to `@writer/doc` 0.4.0 the PDF measured
every space the way Typst does, and Typst's units are not Word's:

- A Typst line is as tall as the font's capitals (cap height to baseline). A
  Word or CSS line is a box of the full line pitch.
- A space in Typst runs from the last baseline to the next cap height. In Word
  and CSS it runs between two line boxes.
- A Typst block's own `above`/`below` replaces the paragraph spacing next to
  it outright. A styled div with `space_before = "0mm"` therefore sat one cap
  height under the paragraph before it: 7.26 pt between the two baselines
  where Word shows 25.2 pt (§6, probe p3).

Templates corrected this by hand in their own Typst code, each differently,
which is the duplication the design layer exists to remove.

The answer is one model, Word's, in all three backends (design.md §0.3):

1. **A line is a line box** of the style's pitch, with the baseline at a fixed
   place inside it (§2.2).
2. **Spaces are measured between line boxes**, and of two adjacent spaces the
   larger one holds (§2.3). Word does *not* add them; R11 §4 assumed it did.
3. **A styled div is one block**: its spaces go around it, its paragraphs keep
   the paragraph spacing between themselves (§2.5, decision D1 in §7).
4. **The first block of the document keeps its space before**, a block at the
   top of a later page does not (§2.4).

The PDF needs a new package version for this (`0.5.0`, §4.2), HTML needs
rules for the vocabulary styles and two child-margin rules (§4.3), Word needs
one change in how it writes a div (§4.4), and the Typst input needs the line
metrics of the design's fonts (§4.1). §5 holds all of it as reference code.

Scope: paragraphs, named-style divs, and the vocabulary styles (headings,
title block, contents, captions' text, table cells). Lists, block quotes, code
blocks, the space around tables and figures, footnotes and callouts keep each
backend's own spacing for now; §8 lists what was measured there.

## 2. Word's model, as measured

### 2.1 Method

Every probe is a Markdown file built by `writer build --to html,pdf,docx` with
one theme: Source Sans 3 at 11 pt, `line_height = 1.35`,
`paragraph_spacing = "3.6mm"` (10.205 pt), A4, 10 mm top margin (28.346 pt).
Each text line is `H<tag> EFH`: flat-footed capitals only, starting with an H.

- **Word**: the DOCX opened in Word for Mac 16, print layout, 500 % zoom. A
  script reads captures of the document window and follows the left stem of
  each line's first H, from the cap height down to the baseline, to the
  half-brightness crossing (0.2 pt per pixel; scrolled captures are chained
  through the lines they share). Lines that do not start with an H ("Contents",
  caption labels) cannot be read this way and are left out or marked.
  Page-top positions (§2.4) need three pages and come from Word's own
  `vertical position` of each line through AppleScript (line tops, 0.05 pt).
- **PDF**: baselines from Ghostscript's `txtwrite` device at 7200 dpi
  (0.01 pt), calibrated on a Typst page with text at known baselines.
- **HTML**: Chrome's layout of the standalone page through the DevTools
  protocol: each text node's client rectangle, baseline = top + the rounded
  ascent of the line's font, CSS px × 0.75. Only gaps are comparable (the
  screen page has its own top padding).
- **Font metrics** are read from the font files (`hhea`, `OS/2`):

| family | single line `s` (em) | ascent `A` (em) | cap height (em) |
|---|---|---|---|
| Source Sans 3 | 1.424 | 1.024 | 0.660 |
| Libertinus Serif | 1.140 | 0.894 | 0.658 |
| DejaVu Sans Mono | 2384/2048 = 1.1640625 | 1901/2048 = 0.92822265625 | (not in the file) |
| New Computer Modern | 1.417 | 1.127 | 0.683 |
| Arial | 1.1499 | 0.938 | 0.716 |
| Times New Roman | 1.1499 | 0.934 | 0.662 |

`s` is the height Word gives a line at `w:line="240"` (`WordLines`,
design.md §7.4). `A` is the part of it above the baseline: `s` minus the
descent, so a font's line gap counts as ascent.

### 2.2 The line box

A paragraph's lines are boxes of one pitch, stacked without anything between
them. Where the baseline sits in the box depends on the line rule alone:

- **Exact pitch `h`** (`w:lineRule="exact"`, `line_height = "12.1pt"`):
  `0.8 h` above the baseline, `0.2 h` below, whatever the font and size.
- **Multiple `m`** of the font size (`auto`, `line_height = 1.35`): the box is
  `m × size` tall, `min(1, m / s) × A × size` of it above the baseline. A line
  of at least a single line keeps the font's ascent and grows downwards; a
  tighter one shrinks ascent and descent alike.

Probe p1 stacks eight styles with no space between them, so every gap is one
box's part below the baseline plus the next box's part above it (pt; body
1.35 has 10.68 above and 4.17 below at 11 pt):

| from → to | model | Word | PDF 0.5.0 |
|---|---|---|---|
| page top → `M100` (multiple 1.0), baseline | 36.26 | 36.40 | 36.26 |
| `M100` line → line | 11.00 | 11.00 | 11.00 |
| `M100` → `X12` (exact 12.1 pt) | 12.77 | 12.80 | 12.77 |
| `X12` line → line | 12.10 | 12.00, 12.20 | 12.10 |
| `X12` → `M200` (multiple 2.0) | 13.68 | 13.60 | 13.68 |
| `M200` line → line | 22.00 | 22.00 | 22.00 |
| `M200` → `X20` (exact 20 pt) | 26.74 | 26.80 | 26.74 |
| `X20` line → line | 20.00 | 20.00 | 20.00 |
| `X20` → body (multiple 1.35) | 14.68 | 14.60 | 14.67 |
| body line → line | 14.85 | 15.00 | 14.85 |
| body → `M142` (multiple 1.424, with the paragraph spacing) | 25.64 | 25.40 | 25.65 |
| `M142` line → line | 15.66 | 15.80 | 15.66 |
| `M142` → body | 15.08 | 15.20 | 15.08 |

The rule holds for other fonts. Probe f1 at 24 pt, body `line_height` equal
to the single line (gaps in pt; the model is the PDF column to 0.01):

| from → to | Arial: Word | PDF | Times New Roman: Word | PDF |
|---|---|---|---|---|
| body → `M200` (2.0) | 27.60 | 27.60 | 27.60 | 27.60 |
| `M200` → `X30` (exact 30 pt) | 49.20 | 49.48 | 49.40 | 49.59 |
| `X30` → `M100` (1.0) | 25.80 | 25.58 | 25.60 | 25.49 |
| `M100` → body | 27.00 | 26.94 | 27.00 | 26.92 |

CSS centres the font's content area in the line box instead (half-leading),
so an HTML line sits a constant amount lower or higher in its box than Word's.
HTML cannot follow Word here without giving up `line-height`; §6 says what the
difference comes to.

### 2.3 Spaces between blocks: the larger one holds

Every block has a space before and a space after. A body paragraph has
`(0, P)` with `P` the paragraph spacing; a named style sets either or both.
Between two blocks Word leaves the **larger** of the upper block's space
after and the lower block's space before, measured between the two line
boxes. Probe p3 (single-paragraph divs whose styles set both spaces, written
(before, after): `A6` = (0, 6 mm), `B8` = (8 mm, 0), `B2` = (2 mm, 0),
`Z0` = (0, 0), `BA` = (6 mm, 8 mm); gaps between baselines in pt, 14.85 of
each is the line itself; the `B2` → `B2` row is Word with decision D1, §2.5):

| from → to | spaces (after, before) | larger | sum | Word |
|---|---|---|---|---|
| body → body | 10.21, 0 | 25.06 | 25.06 | 25.00 |
| `A6` → body | 17.01, 0 | 31.86 | 31.86 | 31.80 |
| `A6` → `B8` | 17.01, 22.68 | **37.53** | 54.54 | **37.60** |
| `B8` → body | 0, 0 | 14.85 | 14.85 | 15.00 |
| body → `B2` | 10.21, 5.67 | **25.06** | 30.73 | **24.80** |
| `B2` → `B2` | 0, 5.67 | 20.52 | 20.52 | 20.60 |
| `Z0` → `Z0` | 0, 0 | 14.85 | 14.85 | 14.80 |
| body → `BA` | 10.21, 17.01 | **31.86** | 42.07 | **31.80** |

R11 §4 said Word adds the two spaces. That was an assumption and is wrong;
R11 carries the correction. Typst's block spacing and CSS's margin collapsing
follow the same larger-space rule, which is what makes one model possible.

### 2.4 Page tops

The first block of the document keeps its space before. A block that starts a
later page loses it, after a forced page break and after a natural one alike
(Word's line tops in pt; the page's text area starts at 28.35):

| probe | block | page 1 | page 2 (after a break) | page 3 (natural) |
|---|---|---|---|---|
| p5 | `BA` (before 6 mm = 17.01) | 45.30 | 28.55 | 28.55 |
| p6 | `Heading 1` (before 12 pt) | 40.35 | 28.55 | (mid-page) |

The PDF with §5's code does the same: the baselines of those blocks are 17.01
(p5) and 12.00 (p6) lower on page 1 than on pages 2 and 3. HTML keeps the
space before after a forced break (a CSS margin after a forced break is
kept); that difference remains (§6).

### 2.5 What a div is

A fenced div with a named style can hold several paragraphs. In the PDF and
in HTML the div is one block: its spaces go around it, and the paragraphs
inside keep the paragraph spacing. Up to now Word got the style on every
paragraph, with `w:contextualSpacing` to remove the spaces between them, so
its paragraphs sat with *no* space at all (probe p4, gaps in pt):

| inside the div | PDF, HTML | Word before | Word with §5's code |
|---|---|---|---|
| `A6`: paragraph → paragraph | 25.05 | 14.80, 15.00 | 25.20, 25.00 |
| `BA`: paragraph → paragraph | 25.05 | 15.00 | 25.00 |
| `B2` div → `B2` div (p3) | 20.52 | 15.00 | 20.60 |

The third row is the other face of `w:contextualSpacing`: Word also dropped
the space between two consecutive divs of one style. §4.4 makes Word write
what the other two show (decision D1, §7).

## 3. Typst facts the reference code relies on (each one probed)

- **Length edges make a line box.** With `text(top-edge: a, bottom-edge: -d)`
  as lengths, every line's frame is `a + d` tall with the baseline `a` below
  its top, whatever glyphs it holds. With `par(leading: 0pt)` the lines of a
  paragraph stack as boxes. Edges in ems follow the size of the text they are
  applied to, which is what a multiple needs; absolute edges give an exact
  pitch.
- **Reading the box back.** In a context block `text.top-edge` and
  `text.bottom-edge` are lengths again; their difference is the pitch in
  effect, a multiple when its absolute part is zero. A named edge
  (`"cap-height"`) means the document is outside the model.
- **Block spacing takes the larger.** Between two blocks Typst leaves the
  larger of the first's `below` and the second's `above`, between their
  frames. With line boxes as frames this is §2.3.
- **A paragraph's spacing is weaker than a block's.** `par.spacing` is the
  same weak space above and below a paragraph, and a neighbouring block's own
  `above`/`below` replaces it outright instead of meeting it. A zero-height
  block after each paragraph that carries the paragraph spacing as its
  `below` (`show par: it => it + block(height: 0pt, above: 0pt, below: …)`)
  turns it into a block's space, which then meets the next block's `above` on
  equal terms. A paragraph with `spacing: 0pt` needs no such block.
- **The space above the first block of a page is dropped**, on page 1 as on
  any other. A zero-height block before the first content keeps the first
  block's `above` on page 1; later page tops stay as Typst has them, which is
  §2.4.
- **A page rule after content starts a new page.** `set page(..)` in the body
  begins a new page run when something was placed before it, a zero-height
  block included. The opening block therefore goes *inside* the set and show
  rules the body starts with, and a body that opens with `page(..)[..]` or
  `pagebreak()` gets none.
- **Styled content can be rebuilt.** The element that holds set and show rules
  has no name, but `{ set par(justify: false); [x] }.func()` returns it, with
  fields `child` and `styles`; `[].func()` is the sequence (`children`),
  `[ ].func()` the space. `styled(open(child), styles)` puts a block in front
  of the first real element without leaving the rules.
- **Measuring a face.** `measure(text(..face, top-edge: "cap-height",
  bottom-edge: "baseline")[H]).height` is the face's cap height,
  `"ascender"`/`"descender"` give Typst's own ascent and descent: the fallback
  for a family whose metrics writer did not send.
- **An empty table cell has no line.** `show table.cell.where(body: [])`
  matches it; a zero-width space inside the cell's inset gives it one line
  box, as Word's empty cell paragraph has.
- **Strokes take no height.** A table's rules are painted over the grid, so a
  ruled row is as tall as its inset and line box; Word and HTML add the rule's
  thickness (§8).
- **The leading is read in more places than a paragraph's lines**
  (`typst-library` and `typst-layout` 0.15.1). A footnote entry sets its own,
  `0.5em` at a text size of `0.85em` (`model/footnote.rs`), and leaves
  `footnote.entry.gap`, `0.5em` of the body's size, between two notes. A
  display equation of several rows puts the leading between the rows'
  frames, and a row is as high as its glyphs whatever the text edges are
  (`math/run.rs`). An inline equation takes up to 0.7 of the leading off
  what its frame needs above and below the line's own edges
  (`math/mod.rs`). Tight lists, the outline's entries and a link's box read
  it too. With a leading of zero the first would add its own leading to the
  note's line boxes (17.30 pt from line to line where the body has 14.85),
  the rows of the second would touch (9.77 pt where `0.4.0` has 17.38), and
  the third no longer hides what a fraction needs beyond the line box.

## 4. The model in each backend (what §5's code does)

### 4.1 Line metrics for Typst (T96)

Typst knows a font's ascender and descender, not Word's single line.
`WordLines` (design.md §7.4) already reads `s` for the DOCX line values; the
reference code also keeps `A` and sends both to Typst:

- `LineMetrics { single, ascent }` replaces the bare `f64` in
  `src/docx/lines.rs`; `sfnt_line_metrics` returns both
  (`ascent = (height − descent) / units`).
- `WordLines::load_declared` reads the design's declared font files and the
  system fonts for every family the design names (`fonts.body`, `.heading`,
  `.mono`, each style's `font`). A family that ends in a weight word
  ("Source Sans 3 SemiBold") is also looked up without it and recorded under
  both names.
- `design_input` (`src/typst/world.rs`) adds the table as a top-level `lines`
  key after `styles`: `{"<family in lower case>": {"single": s, "ascent":
  A}}`, sorted, unrounded, `{}` when no font could be read. An undeclared
  design stays `null`. `design_files` (`src/pipeline/typst.rs`) writes the
  same table into an export's `writer-design.json`. `dump --design` and the
  HTML context do not carry it.

### 4.2 The package (`0.5.0`; T97, T98)

Only a declared design uses the model. A document without one takes the
first branch of `paragraph-layout` and is laid out as by `0.4.0`.

- `line-edges(design, pitch, face)` returns the `top-edge`/`bottom-edge` of
  §2.2 for a face, from the `lines` table (`font-lines`) or Typst's own
  metrics. An unset `line_height` keeps Typst's pitch, the cap height plus
  `0.65em`, as a box.
- `spacing-model(settings)` gives the body's edges and the gap between two
  paragraphs' boxes. An explicit `leading:` or `spacing:` argument stays in
  Typst's measure (baseline to cap height) and is converted; an unset
  `paragraph_spacing` keeps the look of Typst's `1.2em`. It also gives the
  two values the next item but one needs: `leading`, Typst's leading for the
  pitch (the pitch less the cap height), and `note-gap`.
- `paragraph-layout` sets `par(leading: 0pt, spacing: gap)`,
  `block(above: 0pt, below: gap)` and `show par: par-end` (§3). Tight lists
  set `par(spacing: 0pt)` inside themselves and loose lists restore the gap;
  empty cells get a line (`empty-cell`); headings in `heading-font` get that
  font's line box.
- Footnotes and display equations (§3's last item) get three rules in
  `paragraph-layout`. `show footnote.entry: set par(leading: 0pt)` makes a
  note's lines line boxes at the note's size, as Word's are (its
  `Footnote Text` sets a size and inherits the line spacing).
  `set footnote.entry(gap: note-gap)` keeps two notes no closer than a plain
  Typst document has them: of Typst's `0.5em` from a note's last baseline to
  the next note's cap height, the note's line box holds `0.85 × (pitch − cap
  height)` (for an exact pitch, the pitch less 0.85 of the cap height), and
  `note-gap` is the rest, never below zero. At a line height of 1.35 nothing
  is left and two notes are a line box apart; at 1.0 it is 2.3 pt.
  `show math.equation.where(block: true): set par(leading: leading)` gives
  the rows of a display equation the distance they had under `0.4.0`, at
  every line height. An inline equation keeps the leading of zero: a line
  grows by what its math needs beyond the line box, as a line in Word or
  CSS does (0.35 pt below the line for the fraction x/y at 11 pt and 1.35).
- `style-rules` gives a vocabulary style or a div its own line box when it
  sets `line_height` or a font, and passes `space_before`/`space_after` to
  `block.above`/`block.below` as they are. `writer-styled` (divs) reads the
  pitch in effect with `inherited-pitch()`; an unset space is the body
  paragraph's (0 above, the gap below), as Word's styles based on
  `Body Text` have it.
- `cell-inset` makes `Compact`'s spaces the inset above and below the cell's
  line box; an unset one keeps the look of Typst's `5pt` around the capitals
  (`line-trim`).
- `flow-mark`, `open-flow` and `numbered-layout` place the opening block of
  §3. A `title-block` function lays its own page out and gets none.
- `writer-furniture` lays header and footer lines out as line boxes too, so
  `header_offset` and `footer_offset` reach the boxes' edges as in Word, and
  an image's `gap` ends at the first line's box (design.md §9.4).
- Gone: `line-box`, `design-leading`, `design-spacing`, `style-leading`. New:
  `cap-height`, `font-lines`, `line-edges`, `inherited-pitch`, `line-trim`,
  `spacing-model`, `flow-mark`, `par-end`, `open-flow`, `empty-cell`.
  `style-rules`, `vocabulary`, `cell-inset`, `vocabulary-layout` and
  `styled-title-block` take the pitch where they took a leading;
  `writer-furniture` gains `design:`.

What a template notices is in design.md §6.3's contract-change note.

### 4.3 HTML (T99)

HTML already measures spaces between line boxes and collapses margins to the
larger. Two things were missing:

- **Vocabulary styles.** HTML applied named styles to divs only, so a theme's
  `Heading 1` or `Title` changed Word and the PDF and left HTML with the
  theme CSS's own numbers. `vocabulary_css` generates one rule per style on
  the element writer uses it for (design.md §8.8 has the selector table),
  each selector behind `:where([data-writer-document])` so that it adds no
  specificity of its own. The rules are layout rules, after the theme's
  entries (decision D2, §7).
- **The title block's alignment.** Word bases `Subtitle`, `Author` and `Date`
  on `Title`, so a part whose style sets no `align` is aligned as the title
  is, and the PDF aligns the whole block as `Title` is aligned. The default
  theme centres the block in CSS, so a left-aligned `Title` alone would leave
  the subtitle, the authors and the date centred beneath it. `vocabulary_css`
  therefore gives each part `Title`'s `align` when its own style sets none,
  declared or not (`p.subtitle`, `p.author` with `.authors`' `justify-
  content`, `p.date`); a part's own `align` holds.
- **A div's first and last child.** A style that sets `space_after` gets
  `[data-style="…"]>:last-child{margin-bottom:0}`, one that sets
  `space_before` gets `>:first-child{margin-top:0}`: the set space is then
  the whole space at that edge, as in Word and the PDF, where it used to add
  to the last paragraph's own margin inside the div.
- An empty table cell gets a zero-width space
  (`td:empty::before,th:empty::before{content:"\200b"}`), always written, so
  a row of empty cells is as tall as a row of text in all three.

### 4.4 Word (T100)

- `style_xml` writes `w:contextualSpacing` for the `Header` and `footer`
  styles only (their lines are paragraphs of one block).
- The emitter records the paragraphs each styled div writes
  (`BodyState.divs`, `record_div_paragraph`) and, when the div closes,
  `space_div` writes direct spacing on them: `w:before` on every paragraph
  but the first, `w:after` on every one but the last, with the values a body
  paragraph has (`StyleTable::block_spacing`; a tight list item's are
  `Compact`'s). The style's own space before stays on the first paragraph and
  its space after on the last. A div of one paragraph gets nothing, and so
  does a style that sets no space key (it inherits the body's through
  `basedOn`).
- Nested divs are blocks of their own; a section break that follows a div
  stays in its last paragraph (`tests/docx_div_spacing.rs`).

## 5. Reference code

Eight blocks, one set per task. Each block is extracted with `awk` and applied
with `git apply` from the repository root; the task files give the exact
commands. The diffs were taken against commit `5890724f` (crate 0.3.1 with
package 0.4.0) and do not touch a line that names the package version, so
they apply before and after T97's bump alike. Test snapshots are not in the
blocks: each task accepts its own with `INSTA_UPDATE=always` and compares the
change with the lines its task file lists.

| block | task | files |
|---|---|---|
| `lines.diff` | T96 | `src/docx/lines.rs`, `src/pipeline/typst.rs`, `src/typst/world.rs` |
| `lines-tests.diff` | T96 | `tests/typst_design.rs` (one hunk) |
| `package.diff` | T98 | `assets/typst/writer-doc/design.typ`, `lib.typ` |
| `model-tests.diff` | T98 | `tests/typst_design.rs`, `typst_leading.rs`, `typst_style_oracle.rs`, `typst_furniture.rs`, `typst_vocabulary.rs` |
| `html.diff` | T99 | `src/html/design.rs` |
| `word.diff` | T100 | `src/docx/body.rs`, `columns.rs`, `design.rs`, `styles.rs` |
| `word-tests.diff` | T100 | `tests/design_styles.rs` |
| `docx_div_spacing.rs` | T100 | `tests/docx_div_spacing.rs` (new file, whole) |

```sh
# One block, here lines.diff:
awk '/^```diff lines\.diff$/ {on = 1; next} /^```$/ {on = 0} on' \
    docs/research/R12-spacing-model.md > lines.diff
git apply --check lines.diff && git apply lines.diff && rm lines.diff
```

If `git apply --check` fails because a file has changed since, make the same
edits by hand, reading the diff as their exact text.

### 5.1 `lines.diff` (T96)

```diff lines.diff
diff --git a/src/docx/lines.rs b/src/docx/lines.rs
index 79c77594..9ac11ded 100644
--- a/src/docx/lines.rs
+++ b/src/docx/lines.rs
@@ -6,6 +6,10 @@
 //! 1.22, Source Sans 3 1.42). Writer divides the multiple by the single line
 //! of the font it applies to, so a Word line is `n × size` as in the other
 //! formats while still scaling with each paragraph's size.
+//!
+//! The PDF lays its lines out as Word's line box (design.md §6.2), so the
+//! Typst package gets the same single line and the part of it above the
+//! baseline for every family the design names ([`WordLines::to_json`]).
 
 use std::collections::{BTreeMap, BTreeSet};
 use std::path::PathBuf;
@@ -21,11 +25,22 @@ use crate::design::{Design, FontRef, LineHeight};
 /// read.
 pub const DEFAULT_SINGLE_LINE: f64 = 2500.0 / 2048.0;
 
-/// Single lines of the design's font families, in ems.
+/// A font's line metrics in ems, as Word reads them.
+#[derive(Debug, Clone, Copy, PartialEq)]
+pub struct LineMetrics {
+    /// The height of a single line.
+    pub single: f64,
+    /// The part of a single line above the baseline.
+    pub ascent: f64,
+}
+
+/// Line metrics of the design's font families, in ems.
 #[derive(Debug, Clone, Default, PartialEq)]
 pub struct WordLines {
-    /// Keyed by lowercase family name.
+    /// Single lines, keyed by lowercase family name.
     single: BTreeMap<String, f64>,
+    /// Ascents of the families read from a font, keyed the same way.
+    ascent: BTreeMap<String, f64>,
 }
 
 impl WordLines {
@@ -39,17 +54,27 @@ impl WordLines {
     pub fn load(
         design: &Design, font_paths: &[PathBuf], system_fonts: bool,
     ) -> Self {
-        let mut lines = Self::default();
         let multiple = |height: Option<LineHeight>| {
             matches!(height, Some(LineHeight::Multiple(_)))
         };
-        if !design.declared
-            || !(multiple(design.text.line_height)
-                || design
-                    .styles
-                    .values()
-                    .any(|style| multiple(style.line_height)))
+        if multiple(design.text.line_height)
+            || design.styles.values().any(|style| multiple(style.line_height))
         {
+            Self::load_declared(design, font_paths, system_fonts)
+        } else {
+            Self::default()
+        }
+    }
+
+    /// Reads the line metrics of the families a declared design names,
+    /// whatever its line heights: the PDF lays every line of a declared
+    /// design out as Word's line box (design.md §6.2). The sources are
+    /// those of [`load`](Self::load).
+    pub fn load_declared(
+        design: &Design, font_paths: &[PathBuf], system_fonts: bool,
+    ) -> Self {
+        let mut lines = Self::default();
+        if !design.declared {
             return lines;
         }
         let mut missing = families(design);
@@ -106,7 +131,26 @@ impl WordLines {
         (n * 240.0 / self.single_line(family)).round() as u32
     }
 
-    /// Picks each missing family's regular face from one source.
+    /// The metrics as the Typst package reads them (design.md §6.2): one
+    /// `{"single": s, "ascent": a}` per family read from a font, under the
+    /// family in lower case and in family order.
+    pub fn to_json(&self) -> serde_json::Map<String, serde_json::Value> {
+        self.single
+            .iter()
+            .filter_map(|(family, single)| {
+                let ascent = self.ascent.get(family)?;
+                let metrics =
+                    serde_json::json!({"single": single, "ascent": ascent});
+                Some((family.clone(), metrics))
+            })
+            .collect()
+    }
+
+    /// Picks each missing family's regular face from one source. A family
+    /// that ends in a weight's name ("Source Sans 3 SemiBold", the name
+    /// Word selects that face by) may be filed under the family alone, so
+    /// it is also looked up without that word and recorded under both
+    /// names.
     fn resolve<'a>(
         &mut self, missing: &mut BTreeSet<String>,
         infos: impl Iterator<Item = &'a FontInfo>,
@@ -116,13 +160,20 @@ impl WordLines {
             return;
         }
         let book = FontBook::from_infos(infos.cloned());
-        missing.retain(|family| {
-            let found = book
-                .select(family, FontVariant::default())
+        let read = |family: &str| {
+            book.select(family, FontVariant::default())
                 .and_then(&load)
-                .and_then(|font| single_line(&font));
-            if let Some(value) = found {
-                self.single.insert(family.clone(), value);
+                .and_then(|font| line_metrics(&font))
+        };
+        missing.retain(|family| {
+            let base = base_family(family);
+            let found = read(family).or_else(|| base.and_then(read));
+            if let Some(metrics) = found {
+                for name in [Some(family.as_str()), base].into_iter().flatten()
+                {
+                    self.single.entry(name.into()).or_insert(metrics.single);
+                    self.ascent.entry(name.into()).or_insert(metrics.ascent);
+                }
             }
             found.is_none()
         });
@@ -144,12 +195,19 @@ impl WordLines {
 /// descent and the external leading, the part of the `hhea` line gap that
 /// exceeds their difference from the `hhea` ascent plus descent.
 pub fn single_line(font: &Font) -> Option<f64> {
-    sfnt_single_line(font.data(), font.index())
+    line_metrics(font).map(|metrics| metrics.single)
 }
 
-/// [`single_line`] read from the `head`, `hhea` and `OS/2` tables of face
+/// Word's line metrics of `font` in ems: its [`single_line`] and the part
+/// of it above the baseline. Word puts the `hhea` descent below the
+/// baseline and the rest of the single line above it.
+pub fn line_metrics(font: &Font) -> Option<LineMetrics> {
+    sfnt_line_metrics(font.data(), font.index())
+}
+
+/// [`line_metrics`] read from the `head`, `hhea` and `OS/2` tables of face
 /// `index` in an sfnt file or collection.
-fn sfnt_single_line(data: &[u8], index: u32) -> Option<f64> {
+fn sfnt_line_metrics(data: &[u8], index: u32) -> Option<LineMetrics> {
     let u16_at = |at: usize| {
         data.get(at..at + 2)
             .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
@@ -186,8 +244,25 @@ fn sfnt_single_line(data: &[u8], index: u32) -> Option<f64> {
         }
         None => ascent + descent + gap,
     };
-    let value = height / units;
-    (value.is_finite() && value > 0.0).then_some(value)
+    let single = height / units;
+    let ascent = (height - descent) / units;
+    (single.is_finite() && single > 0.0)
+        .then_some(LineMetrics { single, ascent })
+}
+
+/// The words that name a face's weight at the end of a family name
+/// (design.md §8.7), in lower case and without hyphens.
+const WEIGHT_WORDS: [&str; 14] = [
+    "thin", "hairline", "extralight", "ultralight", "light", "regular",
+    "medium", "semibold", "demibold", "bold", "extrabold", "ultrabold",
+    "black", "heavy",
+];
+
+/// A lowercase family name without the weight's name it ends in, when one
+/// follows at least one other word.
+fn base_family(family: &str) -> Option<&str> {
+    let (base, last) = family.rsplit_once(' ')?;
+    WEIGHT_WORDS.contains(&last.replace('-', "").as_str()).then_some(base)
 }
 
 /// The design's font families, lowercase as [`FontBook::select`] expects.
@@ -234,6 +309,18 @@ mod tests {
         assert!((value - 2384.0 / 2048.0).abs() < 1e-9, "{value}");
     }
 
+    #[test]
+    fn line_metrics_put_the_hhea_descent_below_the_baseline() {
+        // DejaVu Sans Mono: a single line of 2384 units with an hhea
+        // descent of 483. Libertinus Serif: 1140 with a descent of 246.
+        let mono = line_metrics(&embedded("DejaVu Sans Mono")).unwrap();
+        assert!((mono.single - 2384.0 / 2048.0).abs() < 1e-9, "{mono:?}");
+        assert!((mono.ascent - 1901.0 / 2048.0).abs() < 1e-9, "{mono:?}");
+        let serif = line_metrics(&embedded("Libertinus Serif")).unwrap();
+        assert!((serif.single - 1.14).abs() < 1e-9, "{serif:?}");
+        assert!((serif.ascent - 0.894).abs() < 1e-9, "{serif:?}");
+    }
+
     #[test]
     fn auto_line_divides_the_multiple_by_the_single_line() {
         let lines = WordLines::default().with("Source Sans 3", 1.424);
@@ -264,6 +351,66 @@ mod tests {
         assert_eq!(lines.single_line(Some("Calibri")), DEFAULT_SINGLE_LINE);
     }
 
+    #[test]
+    fn load_declared_reads_families_without_a_multiple() {
+        let mut design = Design { declared: true, ..Design::default() };
+        design.fonts.body = Some(DesignFont {
+            family: "DejaVu Sans Mono".into(),
+            files: Vec::new(),
+        });
+        design.fonts.heading = Some(DesignFont {
+            family: "No Such Family".into(),
+            files: Vec::new(),
+        });
+        let lines = WordLines::load_declared(&design, &[], false);
+        assert_eq!(
+            serde_json::Value::Object(lines.to_json()),
+            serde_json::json!({"dejavu sans mono": {
+                "single": 2384.0 / 2048.0, "ascent": 1901.0 / 2048.0,
+            }})
+        );
+        let undeclared = Design { declared: false, ..design };
+        assert_eq!(
+            WordLines::load_declared(&undeclared, &[], false),
+            WordLines::default()
+        );
+        // A single line recorded by hand has no ascent to send.
+        let by_hand = WordLines::default().with("Calibri", 1.25);
+        assert!(by_hand.to_json().is_empty());
+    }
+
+    #[test]
+    fn a_family_named_by_a_weight_takes_its_familys_lines() {
+        assert_eq!(
+            base_family("source sans 3 semibold"),
+            Some("source sans 3")
+        );
+        assert_eq!(base_family("inter extra-bold"), Some("inter"));
+        assert_eq!(base_family("source sans 3"), None);
+        assert_eq!(base_family("black"), None);
+        let mut design = Design { declared: true, ..Design::default() };
+        design.styles.insert(
+            "Heading 3".into(),
+            crate::design::Style {
+                font: Some(FontRef::Family("DejaVu Sans Mono Bold".into())),
+                line_height: Some(LineHeight::Multiple(1.2)),
+                ..Default::default()
+            },
+        );
+        let lines = WordLines::load(&design, &[], false);
+        let expected = 2384.0 / 2048.0;
+        for family in ["DejaVu Sans Mono Bold", "DejaVu Sans Mono"] {
+            assert!(
+                (lines.single_line(Some(family)) - expected).abs() < 1e-9,
+                "{family}"
+            );
+        }
+        assert_eq!(
+            lines.to_json().keys().collect::<Vec<_>>(),
+            ["dejavu sans mono", "dejavu sans mono bold"]
+        );
+    }
+
     /// A copy of `font` whose OS/2 Windows ascent is `ascent`.
     fn patched(font: &Font, ascent: u16) -> Vec<u8> {
         let mut data = font.data().to_vec();
diff --git a/src/pipeline/typst.rs b/src/pipeline/typst.rs
index 52da22a1..a3b3c4bf 100644
--- a/src/pipeline/typst.rs
+++ b/src/pipeline/typst.rs
@@ -18,7 +18,7 @@ use crate::markdown::frontmatter::split_front_matter;
 use crate::markdown::include::{include_path, raw_directive_info};
 use crate::notebook::nbformat::{self, CellType};
 use crate::typst::emit::{self, AssetPaths, EmitOptions, IncludeRun};
-use crate::typst::world::WriterWorld;
+use crate::typst::world::{WriterWorld, design_input};
 
 struct File {
     path: PathBuf,
@@ -148,7 +148,7 @@ fn design_files(
             paths.images.insert(source.clone(), path.clone());
         }
     }
-    let json = design.to_json(PathMode::Export(&paths));
+    let json = design_input(&prepared.config, design, PathMode::Export(&paths));
     let mut bytes = serde_json::to_vec_pretty(&json)?;
     bytes.push(b'\n');
     let mut files = vec![File {
diff --git a/src/typst/world.rs b/src/typst/world.rs
index f3cdefd6..78ba04db 100644
--- a/src/typst/world.rs
+++ b/src/typst/world.rs
@@ -18,6 +18,7 @@ use typst_kit::fonts::{FontPath, FontStore};
 use crate::assets::registry::{AssetData, AssetRegistry};
 use crate::config::{Config, Toc};
 use crate::design::{Design, PathMode};
+use crate::docx::lines::WordLines;
 use crate::error::{Diagnostics, Error, Result};
 
 use super::package::PackageStore;
@@ -621,12 +622,29 @@ pub fn world_inputs(
     if design.declared {
         inputs.insert(
             "writer-design".into(),
-            design.to_json(PathMode::Virtual(assets)).to_string(),
+            design_input(config, design, PathMode::Virtual(assets)).to_string(),
         );
     }
     inputs
 }
 
+/// The design as the Typst package reads it: the design's JSON (design.md
+/// §3.3) and, under `lines`, the line metrics of the font families it names
+/// (§6.2), read from the PDF's font sources. `null` for an undeclared
+/// design.
+pub fn design_input(
+    config: &Config, design: &Design, paths: PathMode<'_>,
+) -> serde_json::Value {
+    let mut json = design.to_json(paths);
+    if let Some(object) = json.as_object_mut() {
+        let lines = WordLines::load_declared(
+            design, &config.pdf.font_paths, config.pdf.system_fonts,
+        );
+        object.insert("lines".into(), lines.to_json().into());
+    }
+    json
+}
+
 fn libraries(
     inputs: &BTreeMap<String, String>,
 ) -> (LazyHash<Library>, LazyHash<Library>) {
```

### 5.2 `lines-tests.diff` (T96)

`design_input` now adds `lines` on top of `Design::to_json`, so the test that
compares the two adds the empty table (no font file of this fixture holds its
family).

```diff lines-tests.diff
diff --git a/tests/typst_design.rs b/tests/typst_design.rs
--- a/tests/typst_design.rs
+++ b/tests/typst_design.rs
@@ -42,7 +42,11 @@
     );
     let value: serde_json::Value =
         serde_json::from_str(&inputs["writer-design"]).unwrap();
-    assert_eq!(value, prepared.design.to_json(PathMode::Virtual(&assets)));
+    // The input is the design with the line metrics of its fonts on top
+    // (design.md §3.3); no font file here holds the fixture's family.
+    let mut expected = prepared.design.to_json(PathMode::Virtual(&assets));
+    expected["lines"] = serde_json::json!({});
+    assert_eq!(value, expected);
     assert!(
         value["page"]["header"]["first"]["image"]["path"]
             .as_str()
```

### 5.3 `package.diff` (T98)

```diff package.diff
diff --git a/assets/typst/writer-doc/design.typ b/assets/typst/writer-doc/design.typ
index 5746db6c..49000995 100644
--- a/assets/typst/writer-doc/design.typ
+++ b/assets/typst/writer-doc/design.typ
@@ -41,38 +41,169 @@
 // `writer-doc`'s arguments; `none` until `writer-doc` records it.
 #let design-state = state("writer-design", none)
 
-// The line box Typst lays text of `size` out with: with the default
-// `top-edge: "cap-height"` and `bottom-edge: "baseline"`, the cap height of
-// the font in effect (its OS/2 `sCapHeight`, else its ascender; a fallback
-// font's when the family is missing). Measured rather than assumed, so the
-// fonts and edges actually in use decide. Needs context.
-#let line-box(size) = measure(text(size: size)[H]).height
-
-// `par.leading` for a design pitch (design.md §6.2). Typst's leading is the
-// gap between one line's bottom edge and the next line's top edge, so a
-// baseline pitch needs `pitch − line box`; HTML's `line-height` and Word's
-// lines then agree with the PDF. The result is in ems of `size`, so that a
-// multiple follows a paragraph's own size as HTML's unitless `line-height`
-// and Word's `auto` lines do, and an exact pitch holds at any size. Needs
-// context, except for the `0.65em` default of an unset pitch. `box` is the
-// line box of another font than the one in effect (a named style's).
-#let design-leading(pitch, size, box: auto) = {
-    if pitch == none { return 0.65em }
-    let box = if box == auto { line-box(size) } else { box }
-    let target = if "multiple" in pitch { pitch.multiple * size }
-        else { pitch.exact * 1pt }
-    if target <= box { return 0pt }
-    if "multiple" in pitch { (pitch.multiple - box / size) * 1em }
-    else { target - box / size * 1em }
+// A face's cap height: the line box Typst gives its lines by default
+// (`top-edge: "cap-height"`, `bottom-edge: "baseline"`), which the design's
+// line box replaces. `face` holds the `text` arguments of another face than
+// the one in effect. Measured, so the font actually in use decides, a
+// fallback font's included. Needs context.
+#let cap-height(face) = measure(
+    text(..face, top-edge: "cap-height", bottom-edge: "baseline")[H]
+).height
+
+// A face's line metrics in ems as Word reads them (design.md §7.4): `single`
+// is the height of a single line, `ascent` the part of it above the
+// baseline. Writer measures both and sends them in the design's `lines`
+// table under the family in lower case; a family it did not send falls back
+// to Typst's own ascender and descender. Needs context.
+#let font-lines(design, face) = {
+    let family = face.at("font", default: text.font)
+    if type(family) == array { family = family.first() }
+    if type(family) == dictionary { family = family.name }
+    let sent = design-get(design, "lines", (:)).at(lower(family), default: none)
+    if sent != none { return sent }
+    let size = face.at("size", default: text.size)
+    let reach(top, bottom) = measure(
+        text(..face, top-edge: top, bottom-edge: bottom)[H]
+    ).height / size
+    let ascent = reach("ascender", "baseline")
+    (single: ascent + reach("baseline", "descender"), ascent: ascent)
+}
+
+// The line box of a face's lines as `text`'s edges (design.md §6.2): Word's.
+// An exact pitch `h` puts `0.8 h` above the baseline and `0.2 h` below it,
+// whatever the font. A multiple `m` of the size keeps the font's ascent
+// while the line is at least a single line and puts the rest below the
+// baseline; a tighter line shrinks ascent and descent alike. The edges are
+// in ems for a multiple, so the box follows the size of the text, as HTML's
+// unitless `line-height` and Word's `auto` lines do. `pitch` is `none` for a
+// design that sets no line height: the lines then keep Typst's own pitch,
+// the cap height plus `0.65em`. Needs context.
+#let line-edges(design, pitch, face) = {
+    if pitch != none and "exact" in pitch {
+        return (
+            top-edge: pitch.exact * 0.8pt, bottom-edge: pitch.exact * -0.2pt
+        )
+    }
+    let multiple = if pitch == none {
+        cap-height(face) / face.at("size", default: text.size) + 0.65
+    } else { pitch.multiple }
+    let lines = font-lines(design, face)
+    let ascent = calc.min(1, multiple / lines.single) * lines.ascent
+    (top-edge: ascent * 1em, bottom-edge: (ascent - multiple) * 1em)
+}
+
+// The pitch of the line box in effect, as a design writes one: what a style
+// that sets no `line_height` inherits. `none` outside the line-box model
+// (Typst's named edges). Needs context.
+#let inherited-pitch() = {
+    let top = text.top-edge
+    let bottom = text.bottom-edge
+    if type(top) != length or type(bottom) != length { return none }
+    let pitch = top - bottom
+    if pitch.abs == 0pt { (multiple: pitch.em) }
+    else { (exact: pitch.abs.pt()) }
+}
+
+// How far a face's line box reaches beyond its capitals: `top` above the cap
+// height and `bottom` below the baseline. An inset that should look as it
+// did around Typst's own line box gives these up. Needs context.
+#let line-trim(design, pitch, face) = {
+    let edges = line-edges(design, pitch, face)
+    let size = face.at("size", default: text.size)
+    let resolve(edge) = edge.abs + edge.em * size
+    (
+        top: resolve(edges.top-edge) - cap-height(face),
+        bottom: -resolve(edges.bottom-edge),
+    )
+}
+
+// The document's line and paragraph model (design.md §6.2) from
+// `writer-doc`'s settings: the pitch of its lines, the body's line box and
+// the gap between two paragraphs' line boxes. An explicit `leading` or
+// `spacing` argument is Typst's own measure, from one line's baseline to the
+// next line's cap height, and is turned into the model's. An unset paragraph
+// spacing keeps Typst's `1.2em`, measured the same way. Two values stay in
+// Typst's measure for what `paragraph-layout` leaves outside the line boxes:
+// `leading`, Typst's leading for this pitch (in ems unless the pitch is
+// exact), which a display equation puts between its rows, and `note-gap`,
+// what is left between two footnotes of the `0.5em` that Typst keeps from
+// one note's last baseline to the next note's cap height once the note's
+// line box, at `0.85em`, has taken its part. Needs context.
+#let spacing-model(settings) = {
+    let cap = cap-height((:))
+    let pitch = if settings.leading == auto { settings.line-height } else {
+        (multiple: (cap + settings.leading.to-absolute()) / text.size)
+    }
+    let exact = pitch != none and "exact" in pitch
+    let height = if pitch == none { cap + 0.65em.to-absolute() }
+        else if exact { pitch.exact * 1pt }
+        else { pitch.multiple * text.size }
+    let gap = if settings.spacing != auto {
+        calc.max(0pt, settings.spacing.to-absolute() - (height - cap))
+    } else if settings.paragraph-spacing != none {
+        settings.paragraph-spacing * 1pt
+    } else { calc.max(0pt, 1.2em.to-absolute() - (height - cap)) }
+    let leading = if height <= cap { 0pt }
+        else if exact { height - cap / text.size * 1em }
+        else { (height - cap) / text.size * 1em }
+    let note = if exact { height - 0.85 * cap } else { 0.85 * (height - cap) }
+    (
+        pitch: pitch, gap: gap, leading: leading,
+        note-gap: calc.max(0pt, 0.5em.to-absolute() - note),
+        edges: line-edges(settings.design, pitch, (:)),
+    )
+}
+
+// A block without height that carries `below` as its space below. Typst
+// keeps a space above a block only when a frame precedes it on the page, and
+// lets two blocks' spaces meet on equal terms; this block is that frame and
+// that space.
+#let flow-mark(below) = block(
+    above: 0pt, below: below, height: 0pt, inset: 0pt, outset: 0pt,
+    stroke: none, fill: none, sticky: false,
+)
+
+// Ends a paragraph in the line-box model: the paragraph, then its spacing as
+// a block's space below. Typst gives a paragraph the same weak spacing above
+// and below, and any block's own spacing replaces it outright; as a block's
+// space the paragraph spacing meets the next block's space above on equal
+// terms, and the larger of the two holds, as in Word and HTML. A paragraph
+// without spacing needs no mark: every space next to it already holds.
+#let par-end(it) = {
+    if it.spacing == 0pt { it } else { it + flow-mark(it.spacing) }
 }
 
-// `par.spacing` for a design paragraph spacing: Typst measures it like the
-// leading, from the last baseline to the next line's cap height, so a
-// paragraph break adds `spacing` to a full line pitch, as HTML's
-// `margin-block` and Word's `w:after` do, only as `leading + spacing`.
-// `none` keeps Typst's `1.2em`.
-#let design-spacing(leading, spacing) = {
-    if spacing == none { 1.2em } else { leading + spacing * 1pt }
+// The elements content is made of where it holds set or show rules, several
+// children or only white space. Typst has no names for them.
+#let styled-content = {
+    set par(justify: false)
+    [x]
+}.func()
+#let content-sequence = [].func()
+#let blank-content = ([ ].func(), parbreak)
+
+// `body` with `mark` in front of its first element. Typst drops the space
+// above the first block of a page; Word and HTML keep it on the first page,
+// and a block without height in front of the document keeps it there
+// (design.md §6.2). The mark goes inside the set and show rules `body`
+// starts with: a page rule among them would otherwise find the mark before
+// it and start a new page after it. A body that opens with a page or a page
+// break of its own gets no mark.
+#let open-flow(mark, body) = {
+    if type(body) != content { return mark + body }
+    let kind = body.func()
+    if kind == styled-content {
+        styled-content(open-flow(mark, body.child), body.styles)
+    } else if kind == content-sequence {
+        let children = body.children
+        let at = children.position(child => child.func() not in blank-content)
+        if at == none { return mark + body }
+        (
+            children.slice(0, at).join()
+                + open-flow(mark, children.at(at))
+                + children.slice(at + 1).join()
+        )
+    } else if kind in (page, pagebreak) { body } else { mark + body }
 }
 
 // A style name's normalised key (design.md §8.1): its ASCII letters and
@@ -155,34 +286,25 @@
     opts
 }
 
-// A style's leading (§6.2) on its own font and size; `fallback`, the leading
-// around it, when it sets no `line_height`. Needs context.
-#let style-leading(design, spec, fallback) = {
-    let pitch = style-value(spec, "line_height")
-    if pitch == none { return fallback }
-    let opts = style-text(design, spec)
-    design-leading(
-        pitch, opts.at("size", default: text.size),
-        box: measure(text(..opts)[H]).height
-    )
-}
-
 // What a style sets on the element it styles, as the arguments of
-// `apply-rules`' show-set rules (design.md §8.7). Block spacing adds the
-// style's leading, as `design-spacing` does between paragraphs, so the gap
-// to the next baseline is a full line pitch plus the space, as in HTML and
-// Word. Needs context.
-#let style-rules(design, spec, leading) = {
-    let own = style-leading(design, spec, leading)
+// `apply-rules`' show-set rules (design.md §8.7). A style that sets its own
+// `line_height` or font gets its own line box; `pitch` is the pitch it
+// inherits otherwise. The spaces are plain block spacing between line boxes,
+// so the larger of two adjacent spaces holds. Needs context.
+#let style-rules(design, spec, pitch) = {
+    let face = style-text(design, spec)
+    let own = style-value(spec, "line_height")
+    let edges = if own != none { line-edges(design, own, face) }
+        else if "font" in face { line-edges(design, pitch, face) }
+        else { (:) }
     let align = style-value(spec, "align")
     let par = (:)
     if align != none { par.insert("justify", align == "justify") }
-    if style-value(spec, "line_height") != none { par.insert("leading", own) }
     let block = (:)
     let before = style-value(spec, "space_before")
     let after = style-value(spec, "space_after")
-    if before != none { block.insert("above", own + before * 1pt) }
-    if after != none { block.insert("below", own + after * 1pt) }
+    if before != none { block.insert("above", before * 1pt) }
+    if after != none { block.insert("below", after * 1pt) }
     if style-value(spec, "keep_with_next") == true {
         block.insert("sticky", true)
     }
@@ -190,7 +312,7 @@
         block.insert("breakable", false)
     }
     (
-        text: style-text(design, spec), par: par, block: block,
+        text: (: ..face, ..edges), par: par, block: block,
         align: if align in ("left", "center", "right") {
             ((left: left, center: center, right: right).at(align),)
         } else { () },
@@ -203,11 +325,12 @@
 
 // The show-set targets of the vocabulary styles (design.md §8.7), outermost
 // first: a later entry wins over an earlier one key by key, the way Word's
-// `Heading 1 Unlisted` is based on `heading 1`. Needs context.
-#let vocabulary(design, leading) = {
+// `Heading 1 Unlisted` is based on `heading 1`. `pitch` is the
+// document's. Needs context.
+#let vocabulary(design, pitch) = {
     let style(name) = design-style(design, name)
     let entry(target, spec) = (
-        target: target, ..style-rules(design, spec, leading)
+        target: target, ..style-rules(design, spec, pitch)
     )
     let inline(target, spec) = (..entry(target, spec), block: (:))
     let levels = range(1, 7)
@@ -239,18 +362,33 @@
     ))
 }
 
-// Table cells take `Compact`'s spacing as their inset: a line of the pitch
-// with the space above and below it, the text box (cap height to baseline)
-// in the middle of the line. Needs context.
-#let cell-inset(design, leading) = {
+// Table cells take `Compact`'s spaces as their inset above and below the
+// cell's line box, as Word's rows do. A space the style leaves unset keeps
+// the look of Typst's `5pt` around the capitals. Needs context.
+#let cell-inset(design, pitch) = {
     let spec = design-style(design, "Compact")
-    let own = style-leading(design, spec, leading)
-    let inset = (:)
+    let own = style-value(spec, "line_height")
+    let trim = line-trim(
+        design, if own == none { pitch } else { own }, style-text(design, spec)
+    )
     let before = style-value(spec, "space_before")
     let after = style-value(spec, "space_after")
-    if before != none { inset.insert("top", own / 2 + before * 1pt) }
-    if after != none { inset.insert("bottom", own / 2 + after * 1pt) }
-    if inset.len() == 0 { (:) } else { (inset: inset) }
+    (inset: (
+        top: if before != none { before * 1pt }
+            else { calc.max(0pt, 5pt - trim.top) },
+        bottom: if after != none { after * 1pt }
+            else { calc.max(0pt, 5pt - trim.bottom) },
+    ))
+}
+
+// An empty table cell holds one line box inside its inset, as in Word, where
+// every cell holds a paragraph: a row of empty cells is as tall as a row of
+// text. `it` is the cell, whose inset is one length or one per side.
+#let empty-cell(it) = {
+    let inset = it.inset
+    if type(inset) == dictionary { pad(..inset, sym.zws) }
+    else if inset == auto { sym.zws }
+    else { pad(rest: inset, sym.zws) }
 }
 
 #let apply-rules(entries, body) = {
@@ -265,10 +403,10 @@
 
 // Applies the design's vocabulary styles to `body` as show-set rules, so a
 // rule that a template adds after `show: writer-doc.with(..)` still wins key
-// by key. `leading` is the document's leading. Needs context.
-#let vocabulary-layout(design, leading, body) = {
-    let entries = vocabulary(design, leading)
-    set table(..cell-inset(design, leading))
+// by key. `pitch` is the document's. Needs context.
+#let vocabulary-layout(design, pitch, body) = {
+    let entries = vocabulary(design, pitch)
+    set table(..cell-inset(design, pitch))
     if entries.any(rules => (
         rules.target == heading.where(supplement: contents-title)
     )) {
diff --git a/assets/typst/writer-doc/lib.typ b/assets/typst/writer-doc/lib.typ
index 2000b407..fddc01ac 100644
--- a/assets/typst/writer-doc/lib.typ
+++ b/assets/typst/writer-doc/lib.typ
@@ -88,7 +88,7 @@
 // `Title`, `Subtitle`, `Author` and `Date` over each part's built-in look.
 // Word bases the last three on `Title`, so they follow its alignment. Needs
 // context.
-#let styled-title-block(meta, design, leading) = {
+#let styled-title-block(meta, design, pitch) = {
     let parts = ()
     if meta.title != none {
         parts.push(("Title", (size: 17pt, weight: "bold"), meta.title))
@@ -114,7 +114,7 @@
         } else { meta.date }))
     }
     if parts.len() == 0 { return }
-    let rules(name) = style-rules(design, design-style(design, name), leading)
+    let rules(name) = style-rules(design, design-style(design, name), pitch)
     let base = rules("Title").align.at(0, default: center)
     for (index, (name, defaults, body)) in parts.enumerate() {
         let part = rules(name)
@@ -122,19 +122,11 @@
         let spacing = part.block
         if index == 0 and "above" in spacing {
             // Typst drops a block's space above at the top of a page, where
-            // Word keeps the first paragraph's. Word's line starts at the
-            // font's ascender, Typst's at its cap height.
-            let before = style-value(design-style(design, name), "space_before")
-            let rise = (
-                measure(text(..face, top-edge: "ascender")[H]).height
-                    - measure(text(..face)[H]).height
-            )
-            v(before * 1pt + rise, weak: false)
-            let _ = spacing.remove("above")
+            // Word keeps the first paragraph's.
+            v(spacing.remove("above"), weak: false)
         }
-        // The face is set around the block, so that the block's spacing is in
-        // ems of the part's own size: an unset space is then the paragraph
-        // spacing at that size, as it is for the plain title block's lines.
+        // An unset space is the document's: none above, the paragraph
+        // spacing below.
         {
             set text(..face)
             block(width: 100%, ..spacing, {
@@ -165,22 +157,36 @@
 
 #let numbered-layout(meta, settings, body) = {
     show: writer-numbering.with(..settings.numbering)
-    if type(settings.title-block) == function {
-        // The typed tokens are built here, for the function that asked for
-        // them: a document without one never converts its design.
-        (settings.title-block)(meta, design-tokens(design: settings.design))
-    } else if settings.title-block == auto {
-        if title-styles.any(name => (
-            design-style(settings.design, name) != none
-        )) {
-            styled-title-block(meta, settings.design, settings.leading)
-        } else { title-block(meta) }
-    }
-    abstract-block(meta.abstract, title: settings.abstract-title)
-    if settings.toc != false and settings.toc != 0 {
-        outline(depth: if settings.toc == true { 3 } else { settings.toc })
-    }
-    body
+    let custom = type(settings.title-block) == function
+    let front = {
+        if custom {
+            // The typed tokens are built here, for the function that asked
+            // for them: a document without one never converts its design.
+            (settings.title-block)(meta, design-tokens(design: settings.design))
+        } else if settings.title-block == auto {
+            if title-styles.any(name => (
+                design-style(settings.design, name) != none
+            )) {
+                styled-title-block(meta, settings.design, settings.pitch)
+            } else { title-block(meta) }
+        }
+        abstract-block(meta.abstract, title: settings.abstract-title)
+        if settings.toc != false and settings.toc != 0 {
+            outline(depth: if settings.toc == true { 3 } else { settings.toc })
+        }
+    }
+    // The line-box model opens the document with a mark (design.md §6.2):
+    // before what the package draws first, else inside the body. A
+    // `title-block` function lays its own page out and gets none.
+    let mark = settings.at("opening", default: none)
+    if mark == none or custom {
+        front
+        body
+    } else if front == none { open-flow(mark, body) } else {
+        mark
+        front
+        body
+    }
     if settings.bibliography != none {
         std.bibliography(settings.bibliography, style: settings.style)
     }
@@ -250,21 +256,25 @@
 }
 
 // `spacing` is the document's paragraph spacing: the default gap between an
-// image and the lines below it (design.md §9.1); `line-height` is the
-// design's `text.line_height`, which sets the lines' leading at their size.
+// image and the lines below it (design.md §9.1); `line-height` is the pitch
+// of the lines, which take the line box of their own font and size (§6.2),
+// `design` the design whose `lines` table holds that font's metrics.
 #let writer-furniture(
-            spec, text-width, spacing: 1.2em, line-height: none, font: none
+            spec, text-width, spacing: 1.2em, line-height: none, font: none,
+            design: (:)
         ) = context {
     if spec == none { return none }
     let size = if spec.size == none { text.size } else { spec.size * 1pt }
     let fill = if spec.color == none { text.fill } else { rgb(spec.color) }
-    set text(..if font == none { (:) } else { (font: font) })
-    set text(size: size, fill: fill, weight: spec.weight,
+    let face = (size: size, ..if font == none { (:) } else { (font: font) })
+    set text(..face, ..line-edges(design, line-height, face))
+    set text(fill: fill, weight: spec.weight,
         style: if spec.italic { "italic" } else { "normal" })
     set par(
-        first-line-indent: 0pt, spacing: 0pt, justify: false,
-        leading: design-leading(line-height, size),
+        first-line-indent: 0pt, spacing: 0pt, justify: false, leading: 0pt,
     )
+    // The parts below set their own spacing; nothing inherits the body's.
+    set block(above: 0pt, below: 0pt)
     block(width: text-width, above: 0pt, below: 0pt)[
         #if spec.rule != none and spec.rule.position == "above" {
             furniture-rule(spec.rule)
@@ -274,10 +284,9 @@
             let alignment = if img.align == none { spec.align }
                 else { img.align }
             // The lines are a paragraph of their own below the image: its
-            // `gap` (the paragraph spacing unless set) separates them. The
-            // text box starts at the cap height, so the gap runs from the
-            // image's bottom edge to the first line's capitals. The block
-            // spans the text width so that `align` can place the image.
+            // `gap` (the paragraph spacing unless set) separates the image's
+            // bottom edge from the first line's box. The block spans the
+            // text width so that `align` can place the image.
             let gap = img.at("gap", default: none)
             block(width: 100%,
                 below: if gap == none { spacing } else { gap * 1pt },
@@ -365,7 +374,7 @@
             chosen, text-width, spacing: furniture-gap(settings),
             line-height: if pitch == none { settings.line-height }
                 else { pitch },
-            font: style-face(design, style).font,
+            font: style-face(design, style).font, design: design,
         )
     }
     let header = design-get(design, "page.header", none)
@@ -394,22 +403,60 @@
     }
 }
 
-// Leading and paragraph spacing (design.md §6.2): explicit arguments as
-// given, the design's through `design-leading`, which measures the body
-// font's line box and so needs a context block. The page set-up of `layout`
-// and `furniture-layout` stays outside it.
+// Lines and paragraphs (design.md §6.2). Without a design they are Typst's
+// own: its leading and paragraph spacing, or the explicit arguments. With one
+// every line is a line box as Word and HTML lay it out, with no leading
+// between two of them, and every space is a block's, so that the larger of
+// two adjacent spaces holds. The model measures the body font and so needs a
+// context block; the page set-up of `layout` and `furniture-layout` stays
+// outside it.
 #let paragraph-layout(meta, settings, body) = context {
-    let leading = if settings.leading == auto {
-        design-leading(settings.line-height, text.size)
-    } else { settings.leading }
-    set par(
-        leading: leading,
-        spacing: if settings.spacing == auto {
-            design-spacing(leading, settings.paragraph-spacing)
-        } else { settings.spacing },
-    )
-    show: vocabulary-layout.with(settings.design, leading)
-    reference-layout(meta, (..settings, leading: leading), body)
+    if settings.design.len() == 0 {
+        set par(
+            leading: if settings.leading == auto { 0.65em }
+                else { settings.leading },
+            spacing: if settings.spacing == auto { 1.2em }
+                else { settings.spacing },
+        )
+        reference-layout(meta, settings, body)
+    } else {
+        let model = spacing-model(settings)
+        set text(..model.edges)
+        set par(leading: 0pt, spacing: model.gap)
+        set block(above: 0pt, below: model.gap)
+        show par: par-end
+        // A tight list keeps its items together: a nested list or another
+        // block sits right under its item's text, where Typst attaches it.
+        // A loose list inside it has the document's paragraphs again.
+        show list.where(tight: true): set par(spacing: 0pt)
+        show enum.where(tight: true): set par(spacing: 0pt)
+        show terms.where(tight: true): set par(spacing: 0pt)
+        show list.where(tight: false): set par(spacing: model.gap)
+        show enum.where(tight: false): set par(spacing: model.gap)
+        show terms.where(tight: false): set par(spacing: model.gap)
+        show table.cell.where(body: []): empty-cell
+        // Typst reads the leading in two more places. A footnote sets its
+        // own, which would come on top of its lines' boxes, and keeps a gap
+        // to the next note of which the box holds a part. A display equation
+        // puts the leading between its rows: it keeps Typst's for this pitch.
+        show footnote.entry: set par(leading: 0pt)
+        set footnote.entry(gap: model.note-gap)
+        show math.equation.where(block: true): set par(leading: model.leading)
+        // Headings in their own font take that font's line box.
+        show heading: set text(..if settings.heading-font == none { (:) } else {
+            line-edges(
+                settings.design, model.pitch, (font: settings.heading-font)
+            )
+        })
+        show: vocabulary-layout.with(settings.design, model.pitch)
+        // Typst drops the space above the first block of a page. Word and
+        // HTML keep it on the first page, so a block without height opens
+        // the document there (`numbered-layout` places it).
+        reference-layout(
+            meta, (..settings, pitch: model.pitch, opening: flow-mark(0pt)),
+            body
+        )
+    }
 }
 
 #let furniture-layout(meta, settings, body) = {
@@ -659,39 +706,31 @@
 #let writer-styled(design, style, body) = {
     let spec = design-get(design, "styles", (:)).at(style, default: none)
     if spec == none { return body }
-    let text-opts = style-text(design, spec)
-    let par-opts = (:)
-    if spec.align != none {
-        par-opts.insert("justify", spec.align == "justify")
-    }
-    let aligned = if spec.align in ("left", "center", "right") {
-        align((left: left, center: center, right: right).at(spec.align), body)
-    } else { body }
-    let inner = {
-        set text(..text-opts)
-        set par(..par-opts)
-        if spec.line_height == none { aligned } else {
-            // The style's pitch, measured on its own font and size (§6.2),
-            // with the design's paragraph spacing on top of it between the
-            // block's paragraphs; an unset spacing stays inherited.
-            context {
-                let leading = design-leading(spec.line_height, text.size)
-                let spacing = design-get(design, "text.paragraph_spacing", none)
-                set par(leading: leading, ..if spacing == none { (:) }
-                    else { (spacing: design-spacing(leading, spacing)) })
-                aligned
+    context {
+        let rules = style-rules(design, spec, inherited-pitch())
+        // Inside `writer-doc`'s line-box model an unset space is the body
+        // paragraph's, as in Word, whose custom styles are based on the
+        // body's: none above, the paragraph spacing below.
+        let modelled = type(text.top-edge) == length
+        block(
+            above: if spec.space_before != none { spec.space_before * 1pt }
+                else if modelled { 0pt } else { auto },
+            below: if spec.space_after != none { spec.space_after * 1pt }
+                else if modelled { par.spacing } else { auto },
+            breakable: not spec.keep_together,
+            sticky: spec.keep_with_next,
+            width: 100%,
+            {
+                set text(..rules.text)
+                // A line box of its own has no leading between its lines.
+                set par(..rules.par, ..if "top-edge" in rules.text {
+                    (leading: 0pt)
+                })
+                if rules.align.len() == 0 { body }
+                else { align(rules.align.first(), body) }
             }
-        }
+        )
     }
-    block(
-        above: if spec.space_before != none { spec.space_before * 1pt }
-            else { auto },
-        below: if spec.space_after != none { spec.space_after * 1pt }
-            else { auto },
-        breakable: not spec.keep_together,
-        sticky: spec.keep_with_next,
-        width: 100%, inner
-    )
 }
 
 // Preserve div semantics for templates; named design styles are applied in T85.
```

### 5.4 `model-tests.diff` (T98)

The Typst tests that asserted `0.4.0`'s leading and positions, rewritten for
line boxes. Their expected values are computed from §2's rules in the tests
themselves or measured on §5.3's code.

```diff model-tests.diff
diff --git a/tests/typst_design.rs b/tests/typst_design.rs
--- a/tests/typst_design.rs
+++ b/tests/typst_design.rs
@@ -4,10 +4,6 @@
 
 use typst::foundations::{Datetime, Label, Value};
 use typst::introspection::{Introspector, MetadataElem};
-use typst::layout::Abs;
-use typst::text::{
-    FontStretch, FontStyle, FontVariant, FontVariations, FontWeight,
-};
 use typst::utils::PicoStr;
 use writer::assets::registry::AssetRegistry;
 use writer::config::{Config, Target};
@@ -122,6 +118,8 @@
     #context [#metadata((
         size: text.size.pt(), font: text.font,
         leading: par.leading.to-absolute().pt(),
+        top: text.top-edge.to-absolute().pt(),
+        bottom: text.bottom-edge.to-absolute().pt(),
         justify: par.justify,
     )) <design-settings>]
     #body
@@ -164,27 +162,21 @@
     };
     assert_eq!(settings.get("size").unwrap(), &Value::Float(11.0));
     assert_eq!(settings.get("justify").unwrap(), &Value::Bool(false));
-    let Value::Float(leading) = settings.get("leading").unwrap() else {
-        panic!("expected an absolute leading value")
-    };
-    // 1.35 lines of 11pt less the line box Typst lays the body font out
-    // with (design.md §6.2): DejaVu Sans Mono has no OS/2 cap height, so
-    // that box is its ascender, which `cap_height` then reports.
-    let (font, _) = writer::typst::fonts::embedded()
-        .find(|(_, info)| info.family == "DejaVu Sans Mono")
-        .unwrap();
-    let regular = FontVariant::new(
-        FontStyle::Normal,
-        FontWeight::REGULAR,
-        FontStretch::NORMAL,
-    );
-    let instance =
-        font.instantiate(regular, Abs::pt(11.0), &FontVariations::default());
-    let expected = 1.35 * 11.0 - instance.metrics().cap_height.get() * 11.0;
-    assert!(
-        (leading - expected).abs() < 0.01,
-        "leading {leading}, expected {expected}"
-    );
+    // Word's line box (design.md §6.2): no leading, and 1.35 lines of 11pt
+    // split at the baseline by the body font's ascent. DejaVu Sans Mono's
+    // `hhea` ascender is 1901 of 2048 units.
+    let ascent = 1901.0 / 2048.0 * 11.0;
+    for (key, expected) in
+        [("leading", 0.0), ("top", ascent), ("bottom", ascent - 1.35 * 11.0)]
+    {
+        let Value::Float(value) = settings.get(key).unwrap() else {
+            panic!("expected an absolute {key} value")
+        };
+        assert!(
+            (value - expected).abs() < 0.01,
+            "{key} {value}, expected {expected}"
+        );
+    }
     let matches_family = |family: &Value| {
         matches!(family,
             Value::Str(family) if family.as_str().eq_ignore_ascii_case("DejaVu Sans Mono")
diff --git a/tests/typst_furniture.rs b/tests/typst_furniture.rs
index 133dde9e..79ee5676 100644
--- a/tests/typst_furniture.rs
+++ b/tests/typst_furniture.rs
@@ -45,13 +45,16 @@ fn geometry(
                 let transform = transform.pre_concat(group.transform);
                 geometry(&group.frame, transform, images, footer_bottoms);
             }
-            // The fixture's final footer line uses Typst's default baseline
-            // bottom edge, so its baseline is the furniture's bottom edge.
+            // The furniture's bottom edge is the bottom of the fixture's
+            // final footer line (design.md §6.2): its box is 1.35 lines
+            // high, and what the ascent leaves of it is below the baseline.
             FrameItem::Text(run)
                 if run.text.contains("Company No. 13773507") =>
             {
-                footer_bottoms
-                    .push(Point::zero().transform(transform).y.to_pt());
+                let size = run.size.to_pt();
+                let ascent = run.font.metrics().ascender.at(run.size).to_pt();
+                let baseline = Point::zero().transform(transform).y.to_pt();
+                footer_bottoms.push(baseline + 1.35 * size - ascent);
             }
             FrameItem::Image(_, size, _) => images
                 .push((Point::zero().transform(transform), size.x.to_pt())),
@@ -253,7 +256,7 @@ fn unset_offsets_use_thirty_percent_of_the_matching_margin() {
 }
 
 #[test]
-fn image_gap_separates_the_image_from_the_first_line_cap_height() {
+fn image_gap_separates_the_image_from_the_first_line_box() {
     let directory = tempfile::tempdir().unwrap();
     std::fs::write(directory.path().join("logo.svg"),
         "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10'/></svg>",
@@ -297,9 +300,10 @@ fn image_gap_separates_the_image_from_the_first_line_cap_height() {
         let bottom = images[0].0.y.to_pt() + images[0].1;
         let (origin, run) =
             find_run(frame, Transform::identity(), "GAPLINE").unwrap();
-        // Typst's default top edge is the cap height, so the gap ends there.
-        let top = origin.y.to_pt()
-            - run.font.metrics().cap_height.at(run.size).to_pt();
+        // The gap ends at the top of the first line's box, the font's
+        // ascent above the baseline (design.md §6.2).
+        let top =
+            origin.y.to_pt() - run.font.metrics().ascender.at(run.size).to_pt();
         let gap = top - bottom;
         assert!(
             (gap - expected).abs() < 0.3,
diff --git a/tests/typst_leading.rs b/tests/typst_leading.rs
index 54d711ba..a395beb8 100644
--- a/tests/typst_leading.rs
+++ b/tests/typst_leading.rs
@@ -2,7 +2,9 @@
 //! `text.line_height` and `text.paragraph_spacing` with the geometry of HTML
 //! and Word (design.md §6.2): a multiple `n` is a baseline pitch of `n × size`,
 //! and a paragraph break adds the paragraph spacing to a full pitch, as HTML's
-//! `margin-block: 0 <spacing>` and Word's `w:after` do.
+//! `margin-block: 0 <spacing>` and Word's `w:after` do. The lines sit in
+//! Word's line boxes: no leading between them, the font's ascent above the
+//! baseline and the rest of the pitch below it.
 
 use std::path::Path;
 
@@ -27,11 +29,18 @@ const SIZE: f64 = 11.0;
 const LINE_HEIGHT: f64 = 1.35;
 const SPACING: f64 = 3.6 * 72.0 / 25.4;
 const TOLERANCE: f64 = 0.05;
+/// The package's page margin, 2.5 cm in pt.
+const MARGIN: f64 = 25.0 * 72.0 / 25.4;
 
-/// A two-paragraph body whose last paragraph reports the resolved leading.
+/// A two-paragraph body whose last paragraph reports the resolved leading
+/// and the top and bottom edge of its lines, in pt.
 const BODY: &str = concat!(
     "LINEONE \\\nLINETWO\n\n",
-    "PARATWO #context [#metadata(par.leading.to-absolute().pt()) <leading>]\n",
+    "PARATWO #context [#metadata((\n",
+    "    par.leading.to-absolute().pt(),\n",
+    "    text.top-edge.to-absolute().pt(),\n",
+    "    text.bottom-edge.to-absolute().pt(),\n",
+    ")) <line-box>]\n",
 );
 
 /// The embedded `family`'s regular face, instantiated at the body size: the
@@ -109,17 +118,24 @@ fn baseline(document: &PagedDocument, marker: &str) -> f64 {
         .1
 }
 
-/// The `par.leading` the package resolved, reported by `BODY`.
-fn leading(document: &PagedDocument) -> f64 {
+/// The leading and the top and bottom edge the package resolved, reported
+/// by `BODY`.
+fn line_box(document: &PagedDocument) -> Vec<f64> {
     let content = document
         .introspector()
-        .query_label(Label::new(PicoStr::intern("leading")).unwrap())
+        .query_label(Label::new(PicoStr::intern("line-box")).unwrap())
         .unwrap();
     let metadata = content.to_packed::<MetadataElem>().unwrap();
-    let Value::Float(leading) = metadata.value else {
-        panic!("expected a resolved leading, got {:?}", metadata.value)
+    let Value::Array(values) = &metadata.value else {
+        panic!("expected a resolved line box, got {:?}", metadata.value)
     };
-    leading
+    values
+        .iter()
+        .map(|value| match value {
+            Value::Float(value) => *value,
+            other => panic!("expected a length in pt, got {other:?}"),
+        })
+        .collect()
 }
 
 fn assert_close(actual: f64, expected: f64, what: &str) {
@@ -130,7 +146,8 @@ fn assert_close(actual: f64, expected: f64, what: &str) {
 }
 
 /// Compiles `BODY` under `fonts` with the letter's text values and checks the
-/// line pitch and the paragraph gap; returns the resolved leading.
+/// line pitch, the paragraph gap and the line box; returns the ascent the
+/// box puts above the baseline, in pt.
 fn geometry(root: &Path, fonts: serde_json::Value) -> f64 {
     let design = design(
         serde_json::json!({
@@ -148,7 +165,16 @@ fn geometry(root: &Path, fonts: serde_json::Value) -> f64 {
     let next = baseline(&document, "PARATWO");
     assert_close(second - first, LINE_HEIGHT * SIZE, "line pitch");
     assert_close(next - second, LINE_HEIGHT * SIZE + SPACING, "paragraph gap");
-    leading(&document)
+    // Word's line box: no leading, the whole pitch between the two edges,
+    // and the first baseline the ascent below the page margin.
+    let lines = line_box(&document);
+    let [leading, top, bottom] = lines[..] else {
+        panic!("expected three lengths, got {lines:?}")
+    };
+    assert_close(leading, 0.0, "leading");
+    assert_close(top - bottom, LINE_HEIGHT * SIZE, "line box height");
+    assert_close(first, MARGIN + top, "first baseline");
+    top
 }
 
 #[test]
@@ -157,51 +183,46 @@ fn a_declared_font_takes_the_design_pitch_and_paragraph_gap() {
     let font = embedded("Libertinus Serif");
     let path = directory.path().join("libertinus.otf");
     std::fs::write(&path, font.font().data()).unwrap();
-    let leading = geometry(
+    let ascent = geometry(
         directory.path(),
         serde_json::json!({"body": {
             "family": "Libertinus Serif", "files": [path],
         }}),
     );
-    // Typst's line box runs from the cap height to the baseline, so the
-    // leading is the pitch less the font's OS/2 cap height at the size.
-    let metrics = font.metrics();
-    assert!(metrics.cap_height < metrics.ascender, "a cap height metric");
-    assert_close(
-        leading,
-        LINE_HEIGHT * SIZE - metrics.cap_height.get() * SIZE,
-        "leading",
-    );
+    // Word places the baseline the `hhea` ascender below the top of the
+    // line; Libertinus Serif's is the ascender Typst reports too.
+    assert_close(ascent, font.metrics().ascender.get() * SIZE, "ascent");
 }
 
 #[test]
 fn a_font_without_a_cap_height_metric_keeps_the_pitch() {
-    // DejaVu Sans Mono's OS/2 table predates `sCapHeight`; Typst lays it out
-    // with its ascender as the line box, and the measured leading follows.
+    // DejaVu Sans Mono's OS/2 table predates `sCapHeight`, and the ascender
+    // Typst reports for it is below the `hhea` one Word places the baseline
+    // with, 1901 of 2048 units: the box follows the font's line metrics in
+    // the design input, not Typst's metric.
     let directory = tempfile::tempdir().unwrap();
     let font = embedded("DejaVu Sans Mono");
     let metrics = font.metrics();
     assert_eq!(metrics.cap_height, metrics.ascender, "no cap height metric");
-    let leading = geometry(
+    let ascent = geometry(
         directory.path(),
         serde_json::json!({"body": "DejaVu Sans Mono"}),
     );
-    assert_close(
-        leading,
-        LINE_HEIGHT * SIZE - metrics.ascender.get() * SIZE,
-        "leading",
-    );
+    assert_close(ascent, 1901.0 / 2048.0 * SIZE, "ascent");
+    assert!(ascent > metrics.ascender.get() * SIZE + 1.0);
 }
 
 #[test]
 fn a_missing_family_keeps_the_pitch_with_the_fallback_font() {
     let directory = tempfile::tempdir().unwrap();
-    let leading = geometry(
+    let ascent = geometry(
         directory.path(),
         serde_json::json!({"body": "Writer Unknown Font Family"}),
     );
-    let fallback = embedded("Libertinus Serif").metrics().cap_height.get();
-    assert_close(leading, LINE_HEIGHT * SIZE - fallback * SIZE, "leading");
+    // No font file gives the family's line metrics, so the box takes the
+    // ascender of the font Typst sets the text in.
+    let fallback = embedded("Libertinus Serif").metrics().ascender.get();
+    assert_close(ascent, fallback * SIZE, "ascent");
 }
 
 #[test]
diff --git a/tests/typst_style_oracle.rs b/tests/typst_style_oracle.rs
index 508c339d..a0ee01f8 100644
--- a/tests/typst_style_oracle.rs
+++ b/tests/typst_style_oracle.rs
@@ -4,10 +4,7 @@ use std::path::Path;
 
 use typst::foundations::{Datetime, Label, Value};
 use typst::introspection::{Introspector, MetadataElem};
-use typst::layout::{Abs, Frame, FrameItem, Point, Transform};
-use typst::text::{
-    FontStretch, FontStyle, FontVariant, FontVariations, FontWeight,
-};
+use typst::layout::{Frame, FrameItem, Point, Transform};
 use typst::utils::PicoStr;
 use typst_layout::PagedDocument;
 use writer::assets::registry::AssetRegistry;
@@ -96,7 +93,11 @@ fn recipient_uses_exact_leading_and_six_mm_of_following_space() {
     let directory = tempfile::tempdir().unwrap();
     let design = letter_styles(directory.path());
     let body = r#"#writer-div(style: "Recipient")[
-#context [#metadata(par.leading.to-absolute().pt()) <recipient-leading>]
+#context [#metadata((
+    par.leading.to-absolute().pt(),
+    text.top-edge.to-absolute().pt(),
+    text.bottom-edge.to-absolute().pt(),
+)) <recipient-lines>]
 RECIPIENTFIRST\
 RECIPIENTLAST
 ]
@@ -105,29 +106,25 @@ AFTERRECIPIENT
     let with_spacing = compile(directory.path(), &design, body);
     let content = with_spacing
         .introspector()
-        .query_label(Label::new(PicoStr::intern("recipient-leading")).unwrap())
+        .query_label(Label::new(PicoStr::intern("recipient-lines")).unwrap())
         .unwrap();
     let metadata = content.to_packed::<MetadataElem>().unwrap();
-    let Value::Float(leading) = &metadata.value else {
-        panic!("expected resolved leading metadata")
+    let Value::Array(lines) = &metadata.value else {
+        panic!("expected resolved line box metadata")
     };
-    // The exact 12.1pt pitch less the line box of the default body font
-    // (design.md §6.2): Libertinus Serif's cap height at 11pt.
-    let (font, _) = writer::typst::fonts::embedded()
-        .find(|(_, info)| info.family == "Libertinus Serif")
-        .unwrap();
-    let regular = FontVariant::new(
-        FontStyle::Normal,
-        FontWeight::REGULAR,
-        FontStretch::NORMAL,
-    );
-    let instance =
-        font.instantiate(regular, Abs::pt(11.0), &FontVariations::default());
-    let expected = 12.1 - instance.metrics().cap_height.get() * 11.0;
-    assert!(
-        (leading - expected).abs() < 0.001,
-        "leading {leading}, expected {expected}"
-    );
+    // An exact 12.1pt pitch is a line box of that height with no leading,
+    // four fifths of it above the baseline as in Word (design.md §6.2).
+    let expected = [0.0, 0.8 * 12.1, -0.2 * 12.1];
+    assert_eq!(lines.len(), expected.len());
+    for (value, expected) in lines.iter().zip(expected) {
+        let Value::Float(value) = value else {
+            panic!("expected a length in pt, got {value:?}")
+        };
+        assert!(
+            (value - expected).abs() < 0.001,
+            "line box {lines:?}, expected {expected}"
+        );
+    }
     let mut no_spacing = design.clone();
     no_spacing.styles.get_mut("Recipient").unwrap().space_after = Some(Pt(0.0));
     let without_spacing = compile(directory.path(), &no_spacing, body);
diff --git a/tests/typst_vocabulary.rs b/tests/typst_vocabulary.rs
index 38d2ce36..876c37a8 100644
--- a/tests/typst_vocabulary.rs
+++ b/tests/typst_vocabulary.rs
@@ -280,7 +280,7 @@ fn vocabulary_styles_set_writers_elements() {
     expect(
         &p,
         "heading:[First]",
-        &[("size", 14.0), ("above", 29.1), ("below", 15.1)],
+        &[("size", 14.0), ("above", 20.0), ("below", 6.0)],
         &[("weight", "bold"), ("fill", "#0a2f41"), ("align", "left + top")],
     );
     assert!(!p["heading:[First]"]["justify"].as_bool().unwrap());
@@ -306,27 +306,27 @@ fn vocabulary_styles_set_writers_elements() {
     expect(
         &p,
         "heading:[Unlisted]",
-        &[("size", 18.0), ("above", 31.7), ("below", 17.7)],
+        &[("size", 18.0), ("above", 20.0), ("below", 6.0)],
         &[("weight", "bold"), ("fill", "#a96d4b")],
     );
     assert!(!flag(&p, "heading:[Unlisted]", "justify"));
     expect(
         &p,
         "heading:[Contents]",
-        &[("size", 16.0), ("above", 30.4), ("below", 16.4)],
+        &[("size", 16.0), ("above", 20.0), ("below", 6.0)],
         &[("weight", "regular"), ("fill", "#0a2f41")],
     );
     assert!(!flag(&p, "heading:[Contents]", "justify"));
     expect(
         &p,
         "entry:1",
-        &[("size", 11.0), ("above", 13.15)],
+        &[("size", 11.0), ("above", 6.0)],
         &[("weight", "bold")],
     );
     expect(
         &p,
         "entry:2",
-        &[("size", 11.0), ("above", 7.15)],
+        &[("size", 11.0), ("above", 0.0)],
         &[("weight", "regular")],
     );
     expect(
@@ -355,12 +355,7 @@ fn vocabulary_styles_set_writers_elements() {
         &[("weight", "regular"), ("fill", "#000000")],
     );
     assert!(flag(&p, "body", "justify"));
-    expect(
-        &p,
-        "table",
-        &[("top", 5.575), ("bottom", 6.575), ("left", 5.0)],
-        &[],
-    );
+    expect(&p, "table", &[("top", 2.0), ("bottom", 3.0), ("left", 5.0)], &[]);
 }
 
 #[test]
@@ -403,8 +398,8 @@ fn first_title_part_keeps_its_space_before() {
         &titled_document(),
         &tags,
     );
-    assert_close(far[0], 132.119, "title y with 40pt");
-    assert_close(close[0], 102.119, "title y with 10pt");
+    assert_close(far[0], 125.039, "title y with 40pt");
+    assert_close(close[0], 95.039, "title y with 10pt");
     for (tag, (a, b)) in tags.iter().zip(far.iter().zip(&close)) {
         assert_close(a - b, 30.0, &format!("{tag} moves by"));
     }
@@ -428,21 +423,28 @@ fn last_title_part_space_after_replaces_the_closing_gap() {
     };
     let short = with_after("10pt");
     let long = with_after("40pt");
-    assert_close(short[1], 250.507, "body y with 10pt");
-    assert_close(long[1], 280.507, "body y with 40pt");
+    assert_close(short[1], 232.977, "body y with 10pt");
+    assert_close(long[1], 262.977, "body y with 40pt");
     assert_close(long[1] - short[1], 30.0, "body difference");
-    assert_close(short[0], 228.735, "date y with 10pt");
-    assert_close(long[0], 228.735, "date y with 40pt");
+    assert_close(short[0], 211.205, "date y with 10pt");
+    assert_close(long[0], 211.205, "date y with 40pt");
     let default =
         positions(root, &styled(root, &[]), &titled_document(), &tags);
-    assert_close(default[1], 264.357, "body y without space_after");
+    assert_close(default[1], 245.527, "body y without space_after");
 }
 
 #[test]
 fn designs_without_vocabulary_names_change_nothing() {
     let directory = tempfile::tempdir().unwrap();
     let root = directory.path();
-    for design in [plain(root, &[]), Design::default()] {
+    // A declared design lays its lines out in Word's line boxes (design.md
+    // §6.2): no leading comes above a contents entry, and a cell's inset is
+    // Typst's 5pt less what the box adds above the capitals and below the
+    // baseline. A document without a design keeps Typst's own layout.
+    for (design, entry, inset) in [
+        (plain(root, &[]), 0.0, [2.404, 0.446]),
+        (Design::default(), 7.15, [5.0, 5.0]),
+    ] {
         let p = probes(&compile(root, &design, &main_document()));
         for tag in
             ["heading:[First]", "heading:[Unlisted]", "heading:[Contents]"]
@@ -468,7 +470,7 @@ fn designs_without_vocabulary_names_change_nothing() {
         );
         for tag in ["entry:1", "entry:2", "entry:3"] {
             if p.contains_key(tag) {
-                expect(&p, tag, &[("above", 7.15)], &[("weight", "regular")]);
+                expect(&p, tag, &[("above", entry)], &[("weight", "regular")]);
             }
         }
         assert!(p.contains_key("entry:1") && p.contains_key("entry:2"));
@@ -483,7 +485,7 @@ fn designs_without_vocabulary_names_change_nothing() {
         expect(
             &p,
             "table",
-            &[("top", 5.0), ("bottom", 5.0), ("left", 5.0)],
+            &[("top", inset[0]), ("bottom", inset[1]), ("left", 5.0)],
             &[],
         );
         expect(
@@ -507,7 +509,7 @@ fn a_title_style_without_keys_keeps_the_plain_title_block() {
         &plain(root, &[("Date", keep)]),
         &titled_document(),
     ));
-    let expected = [85.039, 116.404, 157.28, 194.218];
+    let expected = [85.039, 113.325, 154.201, 191.139];
     for (tag, y) in tags.iter().zip(expected) {
         assert_close(number(&base, tag, "y"), y, &format!("{tag} y"));
         assert_close(
@@ -536,13 +538,13 @@ fn template_rules_after_writer_doc_override_the_design() {
     expect(
         &p,
         "heading:[First]",
-        &[("size", 14.0), ("above", 29.1)],
+        &[("size", 14.0), ("above", 20.0)],
         &[("weight", "bold"), ("fill", "#ff4136")],
     );
     expect(
         &p,
         "heading:[Unlisted]",
-        &[("size", 18.0), ("above", 31.7)],
+        &[("size", 18.0), ("above", 20.0)],
         &[("weight", "bold"), ("fill", "#ff4136")],
     );
     expect(&p, "heading:[Contents]", &[], &[("fill", "#0a2f41")]);
```

### 5.5 `html.diff` (T99)

```diff html.diff
diff --git a/src/html/design.rs b/src/html/design.rs
index 272c9206..69c6ea9b 100644
--- a/src/html/design.rs
+++ b/src/html/design.rs
@@ -8,7 +8,9 @@ use typst::text::{FontInfo, FontStyle};
 use super::styles::Targets;
 use crate::assets::emit::{Emitter, OutputMode};
 use crate::assets::registry::AssetRegistry;
-use crate::design::{Color, Design, LineHeight, Pt};
+use crate::design::{
+    Align, Color, Design, FontRef, LineHeight, Pt, Style, normalise_style_key,
+};
 use crate::error::Result;
 
 /// Font faces and CSS tokens that form the theme's optional baseline.
@@ -189,6 +191,7 @@ pub fn layout_css(
     }
     // Named styles, furniture and the page background append their CSS here.
     css.push_str(&styles_css(design));
+    css.push_str(&vocabulary_css(design));
     css.push_str(&furniture_css(design, _targets)?);
     css.push_str(&background_css(design, _targets)?);
     Ok(css)
@@ -272,75 +275,263 @@ pub(crate) fn file_url(
     }
 }
 
+/// A style's text keys and alignment as CSS declarations, each only when
+/// set, in design.md §8.3's order.
+fn text_declarations(style: &Style) -> Vec<String> {
+    let mut declarations = Vec::new();
+    if let Some(font) = &style.font {
+        let value = match font {
+            FontRef::Body => "var(--w-font-body)".into(),
+            FontRef::Heading => "var(--w-font-heading)".into(),
+            FontRef::Mono => "var(--w-font-mono)".into(),
+            FontRef::Family(family) => css_string(family),
+        };
+        declarations.push(format!("font-family:{value}"));
+    }
+    if let Some(size) = style.size {
+        declarations.push(format!("font-size:{}", css_pt(size)));
+    }
+    if let Some(height) = style.line_height {
+        declarations.push(format!("line-height:{}", css_line_height(height)));
+    }
+    if let Some(color) = style.color {
+        declarations.push(format!("color:{}", css_color(color)));
+    }
+    if let Some(weight) = style.weight {
+        declarations.push(format!("font-weight:{}", weight.0));
+    }
+    if let Some(italic) = style.italic {
+        declarations.push(format!(
+            "font-style:{}",
+            if italic { "italic" } else { "normal" }
+        ));
+    }
+    if let Some(align) = style.align {
+        let value = match align {
+            Align::Left => "left",
+            Align::Center => "center",
+            Align::Right => "right",
+            Align::Justify => "justify",
+        };
+        declarations.push(format!("text-align:{value}"));
+    }
+    declarations
+}
+
+/// A style's spaces as declarations of the two given properties.
+fn space_declarations(style: &Style, before: &str, after: &str) -> Vec<String> {
+    let mut declarations = Vec::new();
+    if let Some(space) = style.space_before {
+        declarations.push(format!("{before}:{}", css_pt(space)));
+    }
+    if let Some(space) = style.space_after {
+        declarations.push(format!("{after}:{}", css_pt(space)));
+    }
+    declarations
+}
+
+/// Every key a style sets on a block of its own (design.md §8.3): the text
+/// keys, the spaces as margins and the keep flags.
+fn block_declarations(style: &Style) -> Vec<String> {
+    let mut declarations = text_declarations(style);
+    declarations
+        .extend(space_declarations(style, "margin-top", "margin-bottom"));
+    if style.keep_together {
+        declarations.push("break-inside:avoid".into());
+    }
+    if style.keep_with_next {
+        declarations.push("break-after:avoid".into());
+    }
+    declarations
+}
+
 /// Paragraph style rules selected by the resolved design's declared names.
 pub fn styles_css(design: &Design) -> String {
-    use crate::design::{Align, FontRef};
     let mut css = String::new();
     for (name, style) in &design.styles {
-        let mut declarations = Vec::new();
-        if let Some(font) = &style.font {
-            let value = match font {
-                FontRef::Body => "var(--w-font-body)".into(),
-                FontRef::Heading => "var(--w-font-heading)".into(),
-                FontRef::Mono => "var(--w-font-mono)".into(),
-                FontRef::Family(family) => css_string(family),
-            };
-            declarations.push(format!("font-family:{value}"));
-        }
-        if let Some(size) = style.size {
-            declarations.push(format!("font-size:{}", css_pt(size)));
-        }
-        if let Some(height) = style.line_height {
-            declarations
-                .push(format!("line-height:{}", css_line_height(height)));
-        }
-        if let Some(color) = style.color {
-            declarations.push(format!("color:{}", css_color(color)));
-        }
-        if let Some(weight) = style.weight {
-            declarations.push(format!("font-weight:{}", weight.0));
-        }
-        if let Some(italic) = style.italic {
-            declarations.push(format!(
-                "font-style:{}",
-                if italic { "italic" } else { "normal" }
-            ));
-        }
-        if let Some(align) = style.align {
-            let value = match align {
-                Align::Left => "left",
-                Align::Center => "center",
-                Align::Right => "right",
-                Align::Justify => "justify",
-            };
-            declarations.push(format!("text-align:{value}"));
-        }
-        if let Some(space) = style.space_before {
-            declarations.push(format!("margin-top:{}", css_pt(space)));
-        }
-        if let Some(space) = style.space_after {
-            declarations.push(format!("margin-bottom:{}", css_pt(space)));
+        let selector = format!("[data-style={}]", css_string(name));
+        let _ = writeln!(
+            css,
+            "{selector}{{{}}}",
+            block_declarations(style).join(";")
+        );
+        if style.align.is_some() {
+            let _ = writeln!(css, "{selector} p{{text-align:inherit}}");
         }
-        if style.keep_together {
-            declarations.push("break-inside:avoid".into());
+        // A space the style sets is the whole space at that edge of the div:
+        // the first or last block inside must not add its own margin to it.
+        if style.space_before.is_some() {
+            let _ = writeln!(css, "{selector}>:first-child{{margin-top:0}}");
         }
-        if style.keep_with_next {
-            declarations.push("break-after:avoid".into());
+        if style.space_after.is_some() {
+            let _ = writeln!(css, "{selector}>:last-child{{margin-bottom:0}}");
         }
+    }
+    css
+}
+
+/// The selectors, each limited to the document's own elements. `:where` adds
+/// no specificity, so a rule keeps the specificity of the selector alone.
+fn scoped(selectors: &[&str]) -> String {
+    selectors
+        .iter()
+        .map(|selector| format!(":where([data-writer-document]) {selector}"))
+        .collect::<Vec<_>>()
+        .join(",")
+}
+
+/// Writes one rule for the scoped selectors; none without declarations.
+fn push_rule(css: &mut String, selectors: &[&str], declarations: &[String]) {
+    if !declarations.is_empty() {
         let _ = writeln!(
             css,
-            "[data-style={}]{{{}}}",
-            css_string(name),
+            "{}{{{}}}",
+            scoped(selectors),
             declarations.join(";")
         );
-        if style.align.is_some() {
-            let _ = writeln!(
-                css,
-                "[data-style={}] p{{text-align:inherit}}",
-                css_string(name)
-            );
+    }
+}
+
+/// `top`'s keys laid over `base`'s, as Word's `basedOn` layers two styles.
+fn layered(base: Option<&Style>, top: Option<&Style>) -> Option<Style> {
+    let (Some(base), Some(top)) = (base, top) else {
+        return base.or(top).cloned();
+    };
+    Some(Style {
+        font: top.font.clone().or_else(|| base.font.clone()),
+        size: top.size.or(base.size),
+        line_height: top.line_height.or(base.line_height),
+        color: top.color.or(base.color),
+        weight: top.weight.or(base.weight),
+        italic: top.italic.or(base.italic),
+        align: top.align.or(base.align),
+        space_before: top.space_before.or(base.space_before),
+        space_after: top.space_after.or(base.space_after),
+        keep_together: top.keep_together || base.keep_together,
+        keep_with_next: top.keep_with_next || base.keep_with_next,
+        frame: None,
+    })
+}
+
+/// Rules that give writer's own elements the design's vocabulary styles
+/// (design.md §8.8), as Word and the PDF apply them: headings, the contents
+/// title and entries, captions, table cells and the title block.
+pub fn vocabulary_css(design: &Design) -> String {
+    let style = |name: &str| {
+        let key = normalise_style_key(name);
+        design
+            .styles
+            .iter()
+            .find(|(declared, _)| normalise_style_key(declared) == key)
+            .map(|(_, style)| style)
+    };
+    let mut css = String::new();
+    for level in 1..=6 {
+        let Some(style) = style(&format!("Heading {level}")) else {
+            continue;
+        };
+        let heading = format!("h{level}");
+        // The document title is an `h1` of its own style, `Title`; the
+        // bibliography title is a level-1 heading in Word as well.
+        let selectors = if level == 1 {
+            vec!["h1:where(:not(.title))", ".references-title"]
+        } else {
+            vec![heading.as_str()]
+        };
+        push_rule(&mut css, &selectors, &block_declarations(style));
+    }
+    for level in 1..=6 {
+        let Some(style) = style(&format!("Heading {level} Unlisted")) else {
+            continue;
+        };
+        let selectors = [
+            format!("h{level}.unlisted"),
+            format!("h{level}.notoc"),
+            format!(".abstract h{level}"),
+        ];
+        let selectors: Vec<_> = selectors.iter().map(String::as_str).collect();
+        push_rule(&mut css, &selectors, &block_declarations(style));
+    }
+    if let Some(style) = layered(style("Heading 1"), style("TOC Heading")) {
+        push_rule(&mut css, &["p.toc-title"], &block_declarations(&style));
+    }
+    for level in 1..=6 {
+        let Some(style) = style(&format!("toc {level}")) else {
+            continue;
+        };
+        let declarations = block_declarations(style);
+        if declarations.is_empty() {
+            continue;
+        }
+        let list = format!("nav.toc{}>ol", ">ol>li".repeat(level - 1));
+        let item = format!("{list}>li");
+        let mut entry = vec!["display:block".to_string()];
+        entry.extend(declarations);
+        push_rule(&mut css, &[&format!("{item}>a")], &entry);
+        // The entry's own space replaces the theme's list margins above it.
+        if style.space_before.is_some() {
+            push_rule(&mut css, &[&list, &item], &["margin-top:0".into()]);
+        }
+    }
+    for (name, selector) in [
+        ("Caption", "figcaption"),
+        ("Table Caption", ".kind-table>figcaption"),
+        ("Image Caption", ".kind-image>figcaption"),
+    ] {
+        if let Some(style) = style(name) {
+            push_rule(&mut css, &[selector], &text_declarations(style));
         }
     }
+    if let Some(style) = style("Compact") {
+        let mut declarations = text_declarations(style);
+        declarations
+            .extend(space_declarations(style, "padding-top", "padding-bottom"));
+        push_rule(&mut css, &["th", "td"], &declarations);
+    }
+    // An empty cell holds one line, as every cell does in Word.
+    push_rule(
+        &mut css,
+        &["td:empty::before", "th:empty::before"],
+        &["content:\"\\200b\"".into()],
+    );
+    let title = style("Title");
+    if let Some(style) = title {
+        push_rule(&mut css, &[".title"], &block_declarations(style));
+    }
+    // Word bases `Subtitle`, `Author` and `Date` on `Title`, and the PDF
+    // aligns the title block as `Title` is aligned: a part without an
+    // `align` of its own takes `Title`'s here as well.
+    let part = |name: &str| {
+        let mut part = style(name).cloned().unwrap_or_default();
+        part.align = part.align.or(title.and_then(|title| title.align));
+        part
+    };
+    let subtitle = part("Subtitle");
+    push_rule(&mut css, &["p.subtitle"], &block_declarations(&subtitle));
+    // The authors share one block: its edges take the style's spaces, and
+    // each author's own margin gives way to them.
+    let author = part("Author");
+    let mut declarations = text_declarations(&author);
+    if author.space_before.is_some() {
+        declarations.push("margin-top:0".into());
+    }
+    if author.space_after.is_some() {
+        declarations.push("margin-bottom:0".into());
+    }
+    push_rule(&mut css, &["p.author"], &declarations);
+    let mut authors =
+        space_declarations(&author, "margin-top", "margin-bottom");
+    let place = match author.align {
+        Some(Align::Left) => Some("flex-start"),
+        Some(Align::Center) => Some("center"),
+        Some(Align::Right) => Some("flex-end"),
+        Some(Align::Justify) | None => None,
+    };
+    if let Some(place) = place {
+        authors.push(format!("justify-content:{place}"));
+    }
+    push_rule(&mut css, &[".authors"], &authors);
+    push_rule(&mut css, &["p.date"], &block_declarations(&part("Date")));
     css
 }
 
```

### 5.6 `word.diff` (T100)

```diff word.diff
diff --git a/src/docx/body.rs b/src/docx/body.rs
index af256848..3eac0310 100644
--- a/src/docx/body.rs
+++ b/src/docx/body.rs
@@ -80,6 +80,28 @@ pub(crate) struct BodyState {
     in_abstract: bool,
     /// A page break that the next paragraph's properties carry.
     pending_page_break: bool,
+    /// The open divs of design styles with a space key, innermost last.
+    divs: Vec<DivBlock>,
+}
+
+/// An open div of a design style with a space key (design.md §8.5).
+struct DivBlock {
+    /// The div's paragraph style identifier.
+    style: String,
+    /// The space before and after (twips) its paragraphs keep between
+    /// themselves.
+    inner: (u32, u32),
+    /// The paragraphs written with the div's style so far.
+    paragraphs: Vec<DivParagraph>,
+}
+
+/// One paragraph of an open [`DivBlock`], as `start_paragraph` wrote it.
+struct DivParagraph {
+    /// Offset of its `w:pPr` in the part's buffer.
+    at: usize,
+    /// It is an item of a tight list inside the div.
+    tight: bool,
+    props: ParaProps,
 }
 
 struct ListFrame {
@@ -372,12 +394,92 @@ impl Emitter<'_> {
         if custom {
             self.math_state.custom_div_style = Some(style.clone());
         }
+        let inner = custom.then(|| self.styles.block_spacing(&style)).flatten();
+        if let Some(inner) = inner {
+            self.content.body.divs.push(DivBlock {
+                style: style.clone(),
+                inner,
+                paragraphs: Vec::new(),
+            });
+        }
         self.containers.push(Container::Style(style));
         self.blocks(blocks);
         self.containers.pop();
+        if inner.is_some()
+            && let Some(block) = self.content.body.divs.pop()
+        {
+            self.space_div(block);
+        }
         self.math_state.custom_div_style = previous;
     }
 
+    /// Notes a paragraph about to be written with the open div's style, so
+    /// [`Self::space_div`] can space the div's paragraphs when it ends.
+    fn record_div_paragraph(&mut self, props: &ParaProps) {
+        let at = self.buffer().len() + "<w:p>".len();
+        let tight = self
+            .containers
+            .iter()
+            .rev()
+            .take_while(|container| !matches!(container, Container::Style(_)))
+            .any(|container| *container == Container::TightList);
+        if let Some(block) = self.content.body.divs.last_mut()
+            && props.style.as_deref() == Some(block.style.as_str())
+            && props.spacing.is_none()
+        {
+            block.paragraphs.push(DivParagraph {
+                at,
+                tight,
+                props: props.clone(),
+            });
+        }
+    }
+
+    /// Spaces the paragraphs of a design div that has ended (design.md
+    /// §8.5).
+    ///
+    /// The style's space keys are the div's outer margins: its first
+    /// paragraph keeps the style's space before and its last the style's
+    /// space after. Between themselves the paragraphs keep the space they
+    /// have outside a div (Body Text's, or Compact's in a tight list), as
+    /// direct formatting, written here because only now is the last
+    /// paragraph known. A div of one paragraph needs none.
+    fn space_div(&mut self, block: DivBlock) {
+        let last = block.paragraphs.len().saturating_sub(1);
+        if last == 0 {
+            return;
+        }
+        let compact = block.paragraphs.iter().any(|p| p.tight).then(|| {
+            let id = self.styles.ensure("Compact", StyleKind::Paragraph);
+            self.styles.spacing_of(&id)
+        });
+        // Last first, so the offsets of the paragraphs before stay valid.
+        for (index, paragraph) in block.paragraphs.iter().enumerate().rev() {
+            let (before, after) = match compact {
+                Some(compact) if paragraph.tight => compact,
+                _ => block.inner,
+            };
+            let mut attrs = String::new();
+            if index > 0 {
+                attrs.push_str(&format!(" w:before=\"{before}\""));
+            }
+            if index < last {
+                attrs.push_str(&format!(" w:after=\"{after}\""));
+            }
+            let old = paragraph.props.to_xml();
+            let mut props = paragraph.props.clone();
+            props.spacing = Some(format!("<w:spacing{attrs}/>"));
+            let new = props.to_xml();
+            let range = paragraph.at..paragraph.at + old.len();
+            if self.buffer().get(range.clone()) != Some(old.as_str()) {
+                debug_assert!(false, "a div paragraph moved in its buffer");
+                continue;
+            }
+            self.buffer().replace_range(range, &new);
+            self.body_grew(paragraph.at, new.len() - old.len());
+        }
+    }
+
     fn definitions(&mut self, items: &[DefinitionItem]) {
         for item in items {
             let style =
@@ -423,6 +525,7 @@ impl Emitter<'_> {
             None
         };
         self.mark_paragraph_start();
+        self.record_div_paragraph(&props);
         self.buffer().push_str(&format!("<w:p>{}", props.to_xml()));
         let prefix = std::mem::take(&mut self.content.paragraph_prefix);
         self.buffer().push_str(&prefix);
diff --git a/src/docx/columns.rs b/src/docx/columns.rs
index 7001e080..afdccee0 100644
--- a/src/docx/columns.rs
+++ b/src/docx/columns.rs
@@ -185,6 +185,26 @@ impl Emitter<'_> {
         }
     }
 
+    /// Keeps the recorded paragraph offsets in step with `added` bytes
+    /// written at `at`, inside a paragraph that is already in the part's
+    /// buffer (a styled div's paragraph spacing, `Emitter::space_div`).
+    pub(crate) fn body_grew(&mut self, at: usize, added: usize) {
+        if self.part != super::Part::Document {
+            return;
+        }
+        if self.section.paragraph_start > at {
+            self.section.paragraph_start += added;
+        }
+        if let Some((start, end)) = &mut self.section.paragraph {
+            if *start > at {
+                *start += added;
+            }
+            if *end > at {
+                *end += added;
+            }
+        }
+    }
+
     /// An empty open section changes its columns without a break, or
     /// takes back the break that began it when the section before it had
     /// the columns asked for.
diff --git a/src/docx/design.rs b/src/docx/design.rs
index b641119a..cb445947 100644
--- a/src/docx/design.rs
+++ b/src/docx/design.rs
@@ -555,16 +555,16 @@ pub fn style_xml(
         let attrs = spacing_attrs(&spacing);
         paragraph.set("w:spacing", property("w:spacing", &attrs));
     }
-    // The paragraphs of one div, or the lines of one footer, form one block
-    // whose outer margins the space keys are, as in HTML and Typst. Writer
-    // selects the other built-in names per block (body paragraphs, headings,
-    // contents entries, captions), so their space separates consecutive
-    // paragraphs of the same style and they keep the built-in's setting.
-    let block = matches!(
-        super::styles::vocabulary_paragraph(name),
-        None | Some("footer")
-    );
-    if block && (style.space_before.is_some() || style.space_after.is_some()) {
+    // The lines of one header or footer form one block whose outer margins
+    // the space keys are, so no space separates the lines. The paragraphs
+    // of one div form a block too, but keep their own space between
+    // themselves, which the emitter writes on them (`Emitter::space_div`).
+    // Writer selects the other built-in names per block (body paragraphs,
+    // headings, contents entries, captions), so their space separates
+    // consecutive paragraphs of the same style.
+    let lines = name.eq_ignore_ascii_case(super::styles::HEADER)
+        || super::styles::vocabulary_paragraph(name) == Some("footer");
+    if lines && (style.space_before.is_some() || style.space_after.is_some()) {
         paragraph.set("w:contextualSpacing", "<w:contextualSpacing/>");
     }
     if let Some(align) = style.align {
@@ -1002,7 +1002,7 @@ mod tests {
         assert_eq!(spacing.attr("w:before"), Some("60"));
         assert_eq!(spacing.attr("w:after"), Some("204"));
 
-        // The footer, a block of lines like the custom names, does get it.
+        // The footer's lines form one block, so it does get it.
         let footer = descendant(style("Footer"), &["w:pPr"]);
         assert!(names(footer).contains(&"w:contextualSpacing".to_owned()));
 
@@ -1054,13 +1054,34 @@ mod tests {
             descendant(custom, &["w:basedOn"]).attr("w:val"),
             Some("BodyText")
         );
+        // No contextual spacing: the emitter spaces a div's paragraphs.
         assert_eq!(
             names(descendant(custom, &["w:pPr"])),
-            ["w:keepNext", "w:spacing", "w:contextualSpacing"]
+            ["w:keepNext", "w:spacing"]
         );
         assert!(custom.children.iter().all(|n| n.name != "w:rPr"));
     }
 
+    #[test]
+    fn only_header_and_footer_lines_get_contextual_spacing() {
+        use crate::design::Style;
+        use crate::docx::styles::{StyleKind, StyleTable};
+        let mut design = design();
+        let spaced = Style { space_after: Some(Pt(6.0)), ..Style::default() };
+        for name in ["Header", "footer", "Closing", "Caption"] {
+            design.styles.insert(name.into(), spaced.clone());
+        }
+        let table = StyleTable::generated(&design);
+        let contextual = |name: &str| {
+            let id = table.id_for(name, StyleKind::Paragraph).unwrap();
+            table.definition_xml(id).unwrap().contains("w:contextualSpacing")
+        };
+        assert!(contextual("Header"));
+        assert!(contextual("footer"));
+        assert!(!contextual("Closing"));
+        assert!(!contextual("Caption"));
+    }
+
     #[test]
     fn bare_vocabulary_fragments_keep_their_enclosing_namespace_contract() {
         let source = "<w:style w:type=\"character\" w:styleId=\"Hyperlink\"><w:name w:val=\"Hyperlink\"/><w:rPr><w:color w:val=\"0000FF\"/></w:rPr></w:style>";
diff --git a/src/docx/styles.rs b/src/docx/styles.rs
index 2520c659..7c7d622e 100644
--- a/src/docx/styles.rs
+++ b/src/docx/styles.rs
@@ -290,6 +290,10 @@ pub struct StyleTable {
     diagnostics: Diagnostics,
     /// Design styles whose definition writer wrote with a frame.
     framed: BTreeSet<String>,
+    /// Design div styles writer wrote with a space key, by style id, with
+    /// the space before and after (twips) the paragraphs of one div keep
+    /// between themselves (design.md §8.5).
+    blocks: BTreeMap<String, (u32, u32)>,
 }
 
 impl StyleTable {
@@ -484,9 +488,16 @@ impl StyleTable {
             selected.push((name, id));
         }
         for (name, id) in selected {
-            let xml = super::design::style_xml(
-                name, &styles[name], &self.design, self,
-            );
+            let style = &styles[name];
+            if !writer_selects(name)
+                && (style.space_before.is_some() || style.space_after.is_some())
+            {
+                // Until the design's keys replace it, the definition has
+                // its base's spacing: Body Text's, for a custom name.
+                let inner = self.spacing_of(&id);
+                self.blocks.insert(id.clone(), inner);
+            }
+            let xml = super::design::style_xml(name, style, &self.design, self);
             self.replace_definition(&id, xml);
             if styles[name].frame.is_some() {
                 self.framed.insert(name.clone());
@@ -500,6 +511,58 @@ impl StyleTable {
         self.framed.contains(name)
     }
 
+    /// For a design div style writer wrote with a space key, the space
+    /// before and after (twips) the paragraphs of one div keep between
+    /// themselves; `None` for every other style id (design.md §8.5).
+    pub(crate) fn block_spacing(&self, id: &str) -> Option<(u32, u32)> {
+        self.blocks.get(id).copied()
+    }
+
+    /// The space before and after (twips) a paragraph of style `id` gets
+    /// from its style: the nearest `w:spacing` value along the `basedOn`
+    /// chain, else the document defaults', else none.
+    pub(crate) fn spacing_of(&self, id: &str) -> (u32, u32) {
+        let mut before = None;
+        let mut after = None;
+        let mut read = |node: Option<&XmlNode>| {
+            let spacing = node
+                .and_then(|node| child(node, "w:pPr"))
+                .and_then(|node| child(node, "w:spacing"));
+            let value = |name: &str| {
+                spacing
+                    .and_then(|node| node.attr(name))
+                    .and_then(|value| value.parse::<u32>().ok())
+            };
+            before = before.or(value("w:before"));
+            after = after.or(value("w:after"));
+        };
+        let mut seen = BTreeSet::new();
+        let mut next = Some(id.to_owned());
+        while let Some(id) = next.take() {
+            let Some(raw) = self.definition_xml(&id) else {
+                break;
+            };
+            let wrapper = format!("<root xmlns:w=\"{NS_W}\">{raw}</root>");
+            let Ok(root) = parse_xml(&wrapper) else {
+                break;
+            };
+            let tree = root.children.first();
+            read(tree);
+            next = tree
+                .and_then(|tree| child(tree, "w:basedOn"))
+                .and_then(|node| node.attr("w:val"))
+                .map(str::to_owned)
+                .filter(|base| seen.insert(base.clone()));
+        }
+        if let Ok(tree) = style_root(&self.source) {
+            read(
+                child(&tree, "w:docDefaults")
+                    .and_then(|node| child(node, "w:pPrDefault")),
+            );
+        }
+        (before.unwrap_or(0), after.unwrap_or(0))
+    }
+
     fn read(xml: &str) -> Result<Self> {
         let tree = style_root(xml)?;
         let root = &xml[tree.range.clone()];
@@ -521,6 +584,7 @@ impl StyleTable {
             defaults: BTreeSet::new(),
             diagnostics: Diagnostics::new(),
             framed: BTreeSet::new(),
+            blocks: BTreeMap::new(),
         };
         for node in tree.children.iter().filter(|node| node.name == "w:style") {
             let def = Definition::parse(xml, node)?;
@@ -1493,6 +1557,10 @@ Heading 6 Unlisted|Heading6Unlisted|paragraph|Heading6|BodyText||1|0
 mod design_tests {
     use super::*;
 
+    fn reference_xml(children: &str) -> String {
+        format!("<w:styles xmlns:w=\"{NS_W}\">{children}</w:styles>")
+    }
+
     #[test]
     fn undeclared_design_preserves_the_original_lazy_asset_selection() {
         let tree = style_root(&BUILTIN.source).unwrap();
@@ -1554,4 +1622,75 @@ mod design_tests {
         assert!(writer_selects("header"));
         assert!(!writer_selects("Recipient"));
     }
+
+    #[test]
+    fn spacing_follows_the_base_chain_to_the_document_defaults() {
+        let reference = reference_xml(concat!(
+            "<w:docDefaults><w:pPrDefault><w:pPr>",
+            "<w:spacing w:after=\"160\" w:line=\"259\"/>",
+            "</w:pPr></w:pPrDefault></w:docDefaults>",
+            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\">",
+            "<w:name w:val=\"Normal\"/></w:style>",
+            "<w:style w:type=\"paragraph\" w:styleId=\"Base\">",
+            "<w:name w:val=\"Base\"/><w:basedOn w:val=\"Normal\"/>",
+            "<w:pPr><w:spacing w:before=\"80\"/></w:pPr></w:style>",
+            "<w:style w:type=\"paragraph\" w:styleId=\"Leaf\">",
+            "<w:name w:val=\"Leaf\"/><w:basedOn w:val=\"Base\"/>",
+            "<w:pPr><w:spacing w:after=\"40\"/></w:pPr></w:style>",
+            "<w:style w:type=\"paragraph\" w:styleId=\"Loop\">",
+            "<w:name w:val=\"Loop\"/><w:basedOn w:val=\"Loop\"/></w:style>",
+        ));
+        let table = StyleTable::from_reference(&reference).unwrap();
+        // Each side from the nearest definition that sets it.
+        assert_eq!(table.spacing_of("Leaf"), (80, 40));
+        assert_eq!(table.spacing_of("Base"), (80, 160));
+        assert_eq!(table.spacing_of("Normal"), (0, 160));
+        assert_eq!(table.spacing_of("Loop"), (0, 160));
+        assert_eq!(table.spacing_of("Missing"), (0, 160));
+    }
+
+    #[test]
+    fn div_styles_with_a_space_key_report_the_spacing_inside_a_div() {
+        use crate::design::{Pt, Style};
+        let mut design = Design { declared: true, ..Design::default() };
+        let spaced = Style { space_before: Some(Pt(17.0)), ..Style::default() };
+        for name in ["Closing", "Header", "Heading 1", "footer", "Kept"] {
+            design.styles.insert(name.into(), spaced.clone());
+        }
+        design.styles.insert(
+            "Plain".into(),
+            Style { keep_together: true, ..Style::default() },
+        );
+        let spacing = |table: &StyleTable, name: &str| {
+            let id = table.id_for(name, StyleKind::Paragraph).unwrap();
+            table.block_spacing(id)
+        };
+        // Body Text's built-in 6 pt before and after.
+        let table = StyleTable::generated(&design);
+        assert_eq!(spacing(&table, "Closing"), Some((120, 120)));
+        // Writer selects these per block, and Plain sets no space key.
+        for name in ["Header", "Heading 1", "footer", "Plain"] {
+            assert_eq!(spacing(&table, name), None, "{name}");
+        }
+        // `text.paragraph_spacing` is Body Text's space after, with none
+        // before.
+        design.text.paragraph_spacing = Some(Pt(10.0));
+        let table = StyleTable::generated(&design);
+        assert_eq!(spacing(&table, "Closing"), Some((0, 200)));
+        // A reference's definition of the name wins, so writer leaves its
+        // paragraphs alone; its Body Text spaces the design's other divs.
+        let reference = reference_xml(concat!(
+            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\">",
+            "<w:name w:val=\"Normal\"/></w:style>",
+            "<w:style w:type=\"paragraph\" w:styleId=\"BodyText\">",
+            "<w:name w:val=\"Body Text\"/><w:basedOn w:val=\"Normal\"/>",
+            "<w:pPr><w:spacing w:before=\"20\" w:after=\"80\"/></w:pPr></w:style>",
+            "<w:style w:type=\"paragraph\" w:styleId=\"Kept\">",
+            "<w:name w:val=\"Kept\"/><w:basedOn w:val=\"Normal\"/></w:style>",
+        ));
+        let table = StyleTable::from_reference_with_design(&reference, &design)
+            .unwrap();
+        assert_eq!(spacing(&table, "Kept"), None);
+        assert_eq!(spacing(&table, "Closing"), Some((20, 80)));
+    }
 }
```

### 5.7 `word-tests.diff` (T100)

```diff word-tests.diff
diff --git a/tests/design_styles.rs b/tests/design_styles.rs
index 08c84777..7021d322 100644
--- a/tests/design_styles.rs
+++ b/tests/design_styles.rs
@@ -79,7 +79,9 @@ fn class_and_attribute_routes_share_html_typst_and_word_styles() {
     assert_eq!(document.matches("w:pStyle w:val=\"Recipient\"").count(), 2);
     assert!(styles.contains("w:after=\"340\""));
     assert!(styles.contains("w:line=\"242\""));
-    assert!(styles.contains("w:contextualSpacing"));
+    // A div's paragraphs keep their own space between themselves, which the
+    // emitter writes on them (design.md §8.5): no style merges it away.
+    assert!(!styles.contains("w:contextualSpacing"));
     assert_eq!(styles.matches("w:styleId=\"Date\"").count(), 1);
     let date = styles
         .split_once("<w:name w:val=\"Date\"/>")
```

### 5.8 `tests/docx_div_spacing.rs` (T100, new file)

```sh
awk '/^```rust docx_div_spacing\.rs$/ {on = 1; next} /^```$/ {on = 0} on' \
    docs/research/R12-spacing-model.md > tests/docx_div_spacing.rs
```

```rust docx_div_spacing.rs
//! A div of a design style is one block in Word (design.md §8.5): the
//! style's space keys are its outer margins, and its paragraphs keep their
//! own space between themselves.

use std::io::{Cursor, Read};

use writer::docx::{DocxOptions, Picture, Renderer, ValidateOptions};
use writer::error::{Error, Result};
use writer::ir::MathDialect;
use writer::pipeline::{self, BuildOptions};

const PNG: &[u8] = include_bytes!("fixtures/docx/images/dpi144.png");

const DESIGN: &str = concat!(
    "[design.text]\nparagraph_spacing = '10pt'\n",
    "[design.styles.Closing]\nspace_before = '17pt'\nspace_after = '23pt'\n",
    "[design.styles.Recipient]\nspace_after = '17pt'\n",
    "[design.styles.Plain]\nkeep_together = true\n",
);

struct NoRenderer;

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("no equations in this fixture".into()))
    }
    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("PNG images need no renderer".into()))
    }
    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("no equations in this fixture".into()))
    }
}

/// `word/document.xml` and `word/styles.xml` of `body` under `DESIGN` and
/// `extra` configuration.
fn build(extra: &str, body: &str) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("writer.toml"), format!("{DESIGN}{extra}"))
        .unwrap();
    std::fs::write(dir.path().join("pic.png"), PNG).unwrap();
    let input = dir.path().join("doc.md");
    std::fs::write(&input, body).unwrap();
    let prepared = pipeline::prepare(&BuildOptions::new(input)).unwrap();
    let output = writer::docx::write(
        &prepared.document,
        &prepared.assets,
        &mut NoRenderer,
        &DocxOptions {
            design: &prepared.design,
            config: &prepared.config,
            reference: None,
            source_date_epoch: Some("0".into()),
            all_styles: false,
        },
    )
    .unwrap();
    let violations = writer::docx::validate(
        &output.bytes,
        &ValidateOptions { builtin_styles: true },
    );
    assert!(violations.is_empty(), "{violations:#?}");
    let mut archive = zip::ZipArchive::new(Cursor::new(&output.bytes)).unwrap();
    let mut part = |name: &str| {
        let mut xml = String::new();
        archive.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
        xml
    };
    (part("word/document.xml"), part("word/styles.xml"))
}

/// The properties of the paragraph whose text starts with `text`, without
/// the enclosing `w:pPr` tags.
fn props<'a>(document: &'a str, text: &str) -> &'a str {
    let marker = format!("xml:space=\"preserve\">{text}");
    let at = document
        .find(&marker)
        .unwrap_or_else(|| panic!("no paragraph starts with {text:?}"));
    let paragraph = &document[document[..at].rfind("<w:p>").unwrap()..at];
    paragraph
        .split_once("<w:pPr>")
        .and_then(|(_, rest)| rest.split_once("</w:pPr>"))
        .map_or("", |(props, _)| props)
}

#[test]
fn paragraphs_of_one_div_keep_body_spacing_between_themselves() {
    let (document, styles) = build(
        "",
        concat!(
            "Before\n\n",
            ":::closing\nFirst\n\nMiddle\n\nLast\n:::\n\n",
            ":::closing\nAlone\n:::\n\n",
            ":::recipient\nName\n\nStreet\n:::\n\n",
            ":::plain\nOne\n\nTwo\n:::\n\nAfter\n",
        ),
    );
    // Body Text has no space before and 10 pt (200 twips) after under
    // `text.paragraph_spacing`. The div's own 17 pt and 23 pt stay on its
    // first and last paragraph, from the style.
    assert_eq!(
        props(&document, "First"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Middle"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\" w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Last"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\"/>"
    );
    // One paragraph is the whole block: the style says it all.
    assert_eq!(props(&document, "Alone"), "<w:pStyle w:val=\"Closing\"/>");
    // A style with one space key is a block too.
    assert_eq!(
        props(&document, "Name"),
        "<w:pStyle w:val=\"Recipient\"/><w:spacing w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Street"),
        "<w:pStyle w:val=\"Recipient\"/><w:spacing w:before=\"0\"/>"
    );
    // A style without space keys inherits Body Text's spacing, as before.
    assert_eq!(props(&document, "One"), "<w:pStyle w:val=\"Plain\"/>");
    assert_eq!(props(&document, "Two"), "<w:pStyle w:val=\"Plain\"/>");
    // Two divs of one style in a row are two blocks, so nothing may drop
    // the space between paragraphs of the same style.
    assert!(!styles.contains("w:contextualSpacing"));
    assert!(styles.contains(concat!(
        "<w:name w:val=\"Closing\"/><w:basedOn w:val=\"BodyText\"/>",
        "<w:qFormat/><w:pPr><w:spacing w:before=\"340\" w:after=\"460\"/>",
        "</w:pPr>",
    )));
}

#[test]
fn tight_list_items_in_a_div_keep_compact_spacing() {
    let (document, styles) =
        build("", ":::closing\nIntro\n\n- Alpha\n- Beta\n\nOutro\n:::\n");
    assert_eq!(
        props(&document, "Intro"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:after=\"200\"/>"
    );
    // Compact's built-in 1.8 pt (36 twips) before and after.
    for item in ["Alpha", "Beta"] {
        let props = props(&document, item);
        assert!(props.starts_with("<w:pStyle w:val=\"Closing\"/><w:numPr>"));
        assert!(
            props.ends_with("<w:spacing w:before=\"36\" w:after=\"36\"/>"),
            "{props}"
        );
    }
    assert_eq!(
        props(&document, "Outro"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\"/>"
    );
    assert!(styles.contains("w:styleId=\"Compact\""));
}

#[test]
fn nested_divs_are_blocks_of_their_own() {
    let (document, _) = build(
        "",
        concat!(
            "::::closing\nOuter one\n\n",
            ":::recipient\nInner one\n\nInner two\n:::\n\n",
            "Outer two\n::::\n",
        ),
    );
    assert_eq!(
        props(&document, "Outer one"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Inner one"),
        "<w:pStyle w:val=\"Recipient\"/><w:spacing w:after=\"200\"/>"
    );
    assert_eq!(
        props(&document, "Inner two"),
        "<w:pStyle w:val=\"Recipient\"/><w:spacing w:before=\"0\"/>"
    );
    assert_eq!(
        props(&document, "Outer two"),
        "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\"/>"
    );
}

#[test]
fn a_section_break_after_a_div_stays_in_its_last_paragraph() {
    // The spacing is written once the div has ended, which moves the bytes
    // after it. The break before a full-width figure must still land in the
    // div's last paragraph instead of an empty paragraph of its own.
    let (document, _) = build(
        "[design.page]\ncolumns = 2\n",
        concat!(
            ":::closing\nFirst\n\nLast\n:::\n\n",
            "![A wide figure](pic.png){#fig-wide width=100%}\n\n",
            "After\n",
        ),
    );
    let last = props(&document, "Last");
    assert!(
        last.starts_with(concat!(
            "<w:pStyle w:val=\"Closing\"/><w:spacing w:before=\"0\"/>",
            "<w:sectPr>",
        )),
        "{last}"
    );
    assert_eq!(document.matches("<w:sectPr>").count(), 3);
}
```

## 6. Agreement as measured

All numbers are gaps between consecutive baselines in pt, from the three
outputs of one build with §5's code. "P−W" and "H−W" are the PDF's and
HTML's gap minus Word's. Word's readings have a 0.2 pt grid.

**Spaces (p3; Word measured on the DOCX with D1).** The `0.4.0` column is the
PDF before this work:

| line (style) | Word | PDF | HTML | P−W | H−W | PDF 0.4.0 |
|---|---|---|---|---|---|---|
| body | 25.00 | 25.06 | 25.04 | +0.06 | +0.04 | 25.05 |
| `A6` | 25.20 | 25.05 | 25.04 | −0.15 | −0.16 | 7.26 |
| body | 31.80 | 31.86 | 31.85 | +0.06 | +0.05 | 24.27 |
| `A6` | 25.00 | 25.06 | 25.04 | +0.06 | +0.04 | 7.26 |
| `B8` | 37.60 | 37.52 | 37.52 | −0.08 | −0.08 | 29.94 |
| body | 15.00 | 14.85 | 14.85 | −0.15 | −0.15 | 7.26 |
| `B2` | 24.80 | 25.06 | 25.04 | +0.26 | +0.24 | 12.93 |
| `B2` | 20.60 | 20.52 | 20.51 | −0.08 | −0.09 | 12.92 |
| `Z0` | 15.00 | 14.85 | 14.85 | −0.15 | −0.15 | 7.26 |
| `Z0` | 14.80 | 14.85 | 14.85 | +0.05 | +0.05 | 7.26 |
| body | 15.00 | 14.85 | 14.85 | −0.15 | −0.15 | 7.26 |
| `BA` | 31.80 | 31.86 | 31.85 | +0.06 | +0.05 | 24.27 |

**Divs of several paragraphs (p4; D1).** `N` sets an exact 12.1 pt pitch and
no space; `X12` the same pitch with both spaces 0:

| line (style) | Word | PDF | HTML | P−W | H−W |
|---|---|---|---|---|---|
| `A6`, first paragraph | 25.00 | 25.06 | 25.04 | +0.06 | +0.04 |
| `A6`, second | 25.20 | 25.05 | 25.04 | −0.15 | −0.16 |
| `A6`, third | 25.00 | 25.06 | 25.04 | +0.06 | +0.04 |
| body | 31.80 | 31.86 | 31.85 | +0.06 | +0.05 |
| `BA`, first | 32.00 | 31.86 | 31.85 | −0.14 | −0.15 |
| `BA`, second | 25.00 | 25.05 | 25.04 | +0.05 | +0.04 |
| body | 37.60 | 37.53 | 37.52 | −0.07 | −0.08 |
| `N`, first | 24.00 | 24.05 | 23.54 | +0.05 | −0.46 |
| `N`, second | 22.20 | 22.31 | 22.30 | +0.11 | +0.10 |
| body | 23.40 | 23.30 | 23.80 | −0.10 | +0.40 |
| `X12`, first | 24.00 | 24.06 | 23.54 | +0.06 | −0.46 |

**Line boxes and headings (p2).** `S16` is 16 pt at 1.2, `S16X` 16 pt at an
exact 20 pt, `X09` 11 pt at an exact 9 pt; `Heading 1` is 16 pt bold at 1.2
with 12 pt before and 6 pt after, `Heading 2` 13 pt bold at 1.2 with 10 pt
and 4 pt; no style declares `Heading 3`:

| line (style) | Word | PDF | HTML | P−W | H−W |
|---|---|---|---|---|---|
| `S16` line → line | 19.20 | 19.20 | 19.20 | 0.00 | 0.00 |
| `S16X`, first line | 21.20 | 21.40 | 19.20 | +0.20 | −2.00 |
| `S16X` line → line | 20.00 | 20.00 | 20.00 | 0.00 | 0.00 |
| `X09`, first line | 11.20 | 11.20 | 13.25 | 0.00 | +2.05 |
| `X09` line → line | 9.00 | 9.00 | 9.00 | 0.00 | 0.00 |
| body | 12.60 | 12.47 | 12.00 | −0.13 | −0.60 |
| `Heading 1` | 30.00 | 29.98 | 30.60 | −0.02 | +0.60 |
| body | 22.20 | 22.07 | 21.45 | −0.13 | −0.75 |
| `Heading 2` | 25.40 | 25.60 | 25.79 | +0.20 | +0.39 |
| `Heading 3` (undeclared) | 29.00 | 30.90 | 42.73 | +1.90 | +13.73 |
| body after it | 18.80 | 23.10 | 23.57 | +4.30 | +4.77 |
| `Heading 1` | 29.80 | 29.98 | 30.60 | +0.18 | +0.80 |
| `Heading 2` (after a heading) | 26.60 | 26.61 | 26.19 | +0.01 | −0.41 |
| body | 19.20 | 19.06 | 18.83 | −0.14 | −0.37 |

**Title block and contents (p10: `Title` 24 pt bold at 1.1 with 8 pt after,
`Subtitle` 14 pt at 1.2 with 6 pt after, `Author` 4 pt before and 2 pt after,
`Date` 10 pt with 14 pt after, `TOC Heading` 14 pt at 1.2 with 10 pt and
6 pt, `toc 1` bold with 6 pt before, `toc 2` 10 pt with 2 pt before,
`Heading 1 Unlisted` 18 pt with 20 pt before).** Word read at 300 % zoom
(a 0.33 pt grid). The contents title does not start with an H, so its own
baseline cannot be read; the two gaps around it are given as one:

| line | Word | PDF | HTML | P−W | H−W |
|---|---|---|---|---|---|
| subtitle | 27.33 | 27.50 | 26.88 | +0.17 | −0.45 |
| author | 21.33 | 21.39 | 20.55 | +0.06 | −0.78 |
| date | 16.33 | 15.88 | 16.09 | −0.45 | −0.24 |
| date → first `toc 1` entry (across the contents title) | 51.00 | 51.27 | (container) | +0.27 | |
| `toc 2` entry | 16.00 | 15.88 | 16.09 | −0.12 | +0.09 |
| `toc 2` entry | 15.67 | 15.50 | 15.48 | −0.17 | −0.19 |
| `toc 1` entry | 20.00 | 20.47 | 20.24 | +0.47 | +0.24 |
| body after the contents | 15.00 | 14.85 | (container) | −0.15 | |
| `Heading 1` | 30.33 | 29.98 | 30.60 | −0.35 | +0.26 |
| body | 22.00 | 22.07 | 21.45 | +0.07 | −0.55 |
| `Heading 2` | 25.67 | 25.60 | 25.79 | −0.07 | +0.13 |
| body | 19.00 | 19.06 | 18.83 | +0.06 | −0.17 |
| `Heading 1 Unlisted` | 39.67 | 39.71 | 40.09 | +0.04 | +0.42 |
| body | 22.67 | 22.74 | 22.35 | +0.07 | −0.32 |

What the tables say:

- **Word and the PDF** agree within 0.3 pt on every gap between declared
  styles at 500 % (p1 to p4, f1) and within 0.5 pt at 300 % (p10), which is
  the reading error. Page tops agree (§2.4).
- **HTML** has the same spaces and the same pitch, and its lines sit a
  constant amount off inside their boxes (§2.2). Between body text and a
  heading that is 0.4 to 0.8 pt, between the title block's parts up to
  0.8 pt, and it does not add up down the page. Where a style sets an
  *exact* pitch far from its font's natural line, or an extreme multiple,
  the offset is larger: 0.5 pt for body text at an exact 12.1 pt, 2 pt for
  16 pt text at an exact 20 pt, about 10 pt for 24 pt text at a multiple of
  2.0 (f1: 38.09 where Word has 27.60). Those are the cases where CSS and
  Word place the baseline in the box by different rules. The pitch and the
  spaces cannot move them, but a style's lines can be shifted by the
  constant itself: R13 reports a probe that does it per style with
  `position: relative; top: −c` and reads the four heading gaps of p2 in
  Chrome within 0.03 pt of the model (printed HTML still snaps to 0.75 pt).
  Whether writer generates that shift is left to R13 (its decision D18);
  T99 does not.
- **HTML containers.** The default theme's page layout puts its own space
  around the title block and the contents list (79 pt before the contents
  title and 33 pt after the list in p10). That is theme CSS, outside the
  design.
- **Undeclared headings** differ in all three (29.0, 30.9 and 42.7 pt for
  `Heading 3`): each backend keeps its built-in look for a style the design
  does not declare. Declaring the style brings it into the model. The same
  holds for a space key that a declared heading style leaves unset (§7,
  "What changes", and §8).

## 7. Decisions for the maintainer's sign-off

**D1. Word writes a styled div as one block (T100).** Today a div's
paragraphs sit with no space between them in Word and with the paragraph
spacing in the PDF and HTML, and two consecutive divs of one style lose the
space between them in Word only (§2.5). The reference code makes Word match
the other two: `w:contextualSpacing` stays on `Header` and `footer` alone,
and the emitter writes the body's spacing between a div's paragraphs.
Visible change in Word: a multi-paragraph styled block gains the paragraph
spacing between its paragraphs (10.2 pt each in the probe), and consecutive
blocks of one style gain the larger of their two spaces. A theme that wants
the old look sets it explicitly (line breaks inside one paragraph, as the
letter's address block does, are unaffected). The alternative is to change
the PDF and HTML to Word's old behaviour; that needs sibling-aware rules in
both and makes a div's look depend on its neighbours. T100 is the last code
task and can be dropped without touching the others.

**D2. Vocabulary rules beat a theme entry's rule of equal specificity
(T99).** The generated rules come after the theme's entries in the cascade
and add no specificity (`:where(..)`), so `h1 { margin-top: 2em }` in a
theme's CSS loses to `[design.styles."Heading 1"] space_before`, while
`.report h1 { … }` or `main h1 { … }` still wins, and a key the style does
not set stays the theme's. R11's first recommendation (specificity zero, so
that any theme rule wins) would leave HTML disagreeing with Word and the PDF
whenever a theme styles a bare element, which every theme does. With D2 a
theme that declares a vocabulary style can delete the CSS that restated it.

**What changes for existing documents with a declared design.** In the PDF:
the first baseline of a page and the furniture lines sit lower by the font's
ascent minus its cap height (3.4 pt for Source Sans 3 at 11 pt and 1.35),
spaces around styled blocks grow to what Word shows (the `0.4.0` column in
§6), a plain title block is about 3 pt tighter under the title, and what
the model does not cover moves with the line boxes (the two notes that
follow this one). In HTML:
headings, title block, contents and table cells follow the design's styles
where the theme CSS used to decide, and the title block's subtitle, authors
and date follow `Title`'s `align` (§4.3). In Word: D1. Documents without a
design do not change in any backend.

**Headings whose spaces the design does not set move in the PDF.** A heading
without a declared style, and a declared one that leaves `space_before` or
`space_after` unset, keeps Typst's built-in heading space for that edge
(design.md §8.7). The space is now measured between line boxes, as every
other one is, where `0.4.0` measured it between a baseline and a cap height,
so such a heading stands about 8 pt further from the text above and below it
(7.6 to 9.0 pt in the rows below: the lower part of the upper line box plus
the lower line's ascent less its cap height).
Gaps between baselines in pt; the first two rows are §6's p2 probe (Source
Sans 3, 11 pt at 1.35), the others a probe in Libertinus Serif at the same
size and line height with a paragraph spacing of 10 pt:

| gap | Word | PDF `0.4.0` | PDF `0.5.0` |
|---|---|---|---|
| body → undeclared `Heading 3` | 29.00 | 23.10 | 30.90 |
| undeclared `Heading 3` → body | 18.80 | 15.51 | 23.10 |
| body → `Heading 1` that sets `size = "14pt"` alone | | 27.03 | 35.53 |
| that `Heading 1` → body | | 14.74 | 23.72 |
| body → undeclared `Heading 2` | | 24.36 | 32.66 |
| undeclared `Heading 2` → body | | 15.48 | 24.10 |
| `Heading 3` with `space_before = "20pt"` alone → body | | 16.24 | 24.31 |

Above the heading the PDF moves from 5.9 pt under Word to 1.9 pt over it;
below, from 3.3 pt under to 4.3 pt over. Between the boxes the 14 pt
`Heading 1` now has 18.0 pt above and 7.5 pt below (Typst's built-in 1.8 em
and 0.75 em for level 1, each divided by the level's scale of 1.4 and taken
of the heading's own size; `typst-library` `model/heading.rs`), where Word's
built-in `Heading 1` has 18 pt and 6 pt (`w:before="360" w:after="120"` in
the reference document). The change is kept: it follows from the model, and
a theme that sets `space_before` and `space_after` on the heading styles it
declares, as both EIT themes do, chooses the distances in all three outputs.
T101 says so in the guide's troubleshooting entry.

**Blocks the model does not cover move in the PDF too.** Their spaces are
Typst's own or the paragraph spacing, and both now lie between line boxes.
Gaps between baselines in pt, 11 pt Libertinus Serif at 1.35 with a
paragraph spacing of 10 pt; §8 has Word and HTML for most rows:

| gap | PDF `0.4.0` | PDF `0.5.0` | change |
|---|---|---|---|
| paragraph → block quote | 33.63 | 41.25 | +7.62 |
| block quote → paragraph | 27.04 | 34.65 | +7.61 |
| paragraph → display equation of one row | 25.49 | 22.89 | −2.60 |
| that equation → paragraph | 27.11 | 22.09 | −5.02 |
| paragraph → paragraph across an image | 87.81 | 80.21 | −7.60 |
| paragraph → first table row | 29.85 | 27.25 | −2.60 |
| table row → row | 17.24 | 17.25 | |
| last table row → paragraph | 29.85 | 24.85 | −5.00 |
| paragraph → first code line | 24.30 | 22.89 | −1.41 |
| code line → line | 12.78 | 11.88 | −0.90 |
| last code line → paragraph | 24.85 | 23.84 | −1.01 |
| paragraph → tight list item | 24.85 | 24.85 | |
| tight item → item | 14.85 | 14.85 | |
| tight list → paragraph | 24.85 | 24.85 | |
| footnote line → line | 10.83 | 12.62 | +1.79 |
| footnote → footnote | 11.66 | 12.62 | +0.96 |
| display equation row → row | 17.38 | 17.38 | |
| line with the inline fraction x/y → next line | 14.88 | 15.20 | +0.32 |

- **Block quotes** are the headings' case: Typst's `2.4em` above and `1.8em`
  below (`model/quote.rs`) now count between line boxes, 7.6 pt more on each
  side. Word has 25.20 and 19.80 here (§8), so the PDF moves away from it;
  one shared space for a quote belongs to the audit of defaults (§8).
- **A figure, a table or a display equation** is no line box. The paragraph
  spacing lies between it and the boxes around it, where `0.4.0` had the
  leading on both sides as well: it stands 2.6 pt closer to the text above
  (the ascent less the cap height) and 5.0 pt closer to the text below (a
  line box's part below the baseline). For tables that is towards Word
  (27.67 and 18.67 in §8, with `Compact` cells).
- **Code** lines are line boxes at the code's size, 1.35 × 8.8 pt.
- **Footnotes and equation rows** follow §4.2's three rules. `0.4.0` set a
  note's lines 10.83 pt and two notes 11.66 pt apart at every line height;
  `0.5.0` has, for line → line and note → note: 9.35 and 11.66 at a line
  height of 1.0, 10.75 and 11.65 at 1.15, 12.62 and 12.62 at 1.35, 15.00 and
  15.00 at an exact 15 pt, 12.23 and 12.23 without a line height. The rows
  of an equation are the same in both versions at each of the five (13.53,
  15.18, 17.38, 17.53, 16.91).

None of these is converted back to `0.4.0`'s distance: every space in the
PDF is now a space between line boxes, which is the form the follow-up needs
to give each of these elements one value for all three outputs. T101's
troubleshooting entry lists the moves.

## 8. Outside the model: what was measured for the follow-up

These keep each backend's own spacing. Gaps between baselines in pt, same
probes (p7 for lists, quotes and code; p10 for tables):

| from → to | Word | PDF | HTML |
|---|---|---|---|
| paragraph → tight list item | 25.00 | 25.06 | 25.04 |
| tight item → item | 16.80 | 14.85 | 17.59 |
| tight list → paragraph | 16.60 | 25.05 | 27.49 |
| loose item → item | 25.20 | 25.05 | 25.04 |
| paragraph → block quote | 25.20 | 41.25 | 27.79 |
| block quote → paragraph | 19.80 | 34.65 | 40.43 |
| paragraph → code block | 23.00 | 22.92 | 33.63 |
| code line → line | 11.20 | 11.88 | 14.03 |
| code block → paragraph | 19.20 | 24.22 | 34.86 |
| paragraph → first table row | 27.67 | 26.73 | 35.10 |
| table row → row (`Compact` 10 pt at 1.3, 3 pt and 3 pt) | 19.67 | 19.00 | 19.73 |
| table → paragraph | 18.67 | 27.53 | 34.73 |

- **Table rules.** Word and HTML add a rule's thickness to a ruled row (about
  0.67 pt here); Typst paints rules over the grid. After a table the PDF
  leaves the paragraph spacing where Word leaves none.
- **Tight lists** are `Compact` paragraphs in Word; the PDF and HTML apply
  `Compact` to cells only (design.md open point 14).
- **Code in HTML** is set in the theme's monospace stack (Menlo on the Mac
  that measured), whose ascent is not the body font's. *Correction
  (2026-11-22):* the two HTML gaps around the code block first read 34.38
  and 34.11, taken with the body font's ascent for the code lines too. R13's
  harness found it; Chrome's printed PDF of the same page agrees with the
  corrected values on its 0.75 pt grid (33.75 and 35.25, with 13.50 between
  the code lines). The sum of the three gaps is the same either way.
- **Block quotes, code blocks, callouts, footnotes, equations, figures and
  captions' spaces** have built-in spacing per backend, none of it from the
  design. In the PDF those built-in spaces now lie between line boxes (§7's
  last table). Word's side is its styles (docx-output.md §2.3): `Block Text`
  has 5 pt before and after, `Footnote Text` is 9 pt with 3 pt after and
  inherits the line spacing. Typst sets a note at `0.85em`, 9.35 pt here, so
  the lines of a note are 12.15 pt apart in Word and 12.62 pt in the PDF,
  and two notes 3 pt further apart in Word. Word's notes and equations were
  not measured.
- **Body-text styles** (`Body Text`, `First Paragraph`, `Abstract`,
  `Bibliography`, …) are applied by Word only (R11 §4).
- **Keys a declared style leaves unset** keep each backend's built-in value,
  and the three built-ins differ. Headings: Word's reference styles carry
  their own spaces (18 pt and 6 pt for `Heading 1`, 14 pt and 4 pt for
  `Heading 2`), Typst its own (§7), the theme's CSS its own. Title block:
  Word bases `Subtitle`, `Author` and `Date` on `Title`, so they take
  `Title`'s line height, weight, colour and font where their own style is
  silent; the PDF and HTML follow `Title`'s `align` only and take the other
  keys from the document. A theme that sets each key on each style it
  declares, as the EIT report theme does for the title block, agrees in all
  three; one shared default per key belongs to the audit of defaults, not to
  this model.
- **Pagination.** Word lets a footer that is taller than the bottom margin
  push the body up; in p6 (10 mm margins) that moved one paragraph to the
  next page. The PDF places furniture in the margin and never moves the body.
- **HTML after a forced page break** keeps a block's space before (§2.4), and
  HTML furniture does not take the `Header`/`footer` styles.
- **HTML's paragraph rule** is as on `main`:
  `[data-writer-document] p{margin-block:0 P;text-align:…}` (`layout_css`)
  reaches every `p`, so with a design that sets `paragraph_spacing` or
  `justify` it also replaces the theme's margins and alignment on
  paragraphs that are no body text (a title part whose style is not
  declared, the last paragraph of a quote, a cell or a callout). T99's
  vocabulary rules come after it and hold for the styles a design declares;
  narrowing the rule itself is R13's (its T104).
- **A div that starts or ends with something other than a paragraph**, and
  divs nested in divs, were not measured in Word beyond what
  `tests/docx_div_spacing.rs` pins.

A follow-up milestone would bring these into §0.3's model one element at a
time, each with its own Word measurement.

## 9. Commands used (for anyone repeating the checks)

```sh
writer --config writer.toml build p3.md --to html,pdf,docx
gs -q -dNOPAUSE -dBATCH -dSAFER -sDEVICE=txtwrite -r7200 -dTextFormat=0 \
    -sOutputFile=- dist/p3.pdf        # span bbox y = baseline, 0.01 pt
unzip -p dist/p3.docx word/document.xml   # what Word is given
```

Chrome is driven through the DevTools protocol (`Page.navigate`, then
`Runtime.evaluate` over the text nodes' `getClientRects()`); a one-shot
`--dump-dom` or `--print-to-pdf` run is not needed. Word is read from window
captures as §2.1 describes; nothing is saved from Word.

A probe inside the test suite reads the model back from a compiled document
(`tests/typst_vocabulary.rs` has the full version):

```typst
#let probe(tag) = context [#metadata(json.encode((
    tag: tag, y: here().position().y.pt(), page: here().page(),
    top: text.top-edge.to-absolute().pt(),
    bottom: text.bottom-edge.to-absolute().pt(),
    leading: par.leading.to-absolute().pt(),
    gap: par.spacing.to-absolute().pt(),
))) <probe>]
```
