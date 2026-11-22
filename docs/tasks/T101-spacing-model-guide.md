# T101 — Guide: one spacing model in Word, the PDF and HTML

| | |
|---|---|
| Milestone | M8 |
| Size | S |
| Depends on | T98, T99, T100 |
| Parallel-safe with | nothing: it is the last task of the milestone |
| Files you own | `docs/guide/configuration.md`, `docs/guide/customization.md`, `docs/guide/outputs.md`, `docs/guide/troubleshooting.md`, `docs/guide/typst.md`, `examples/letter/README.md` |

## Goal
The user guide says what T96 to T100 made true: lines and the spaces between blocks are built
the same way in Word, the PDF and HTML; the named styles of the shared vocabulary style HTML
too; a styled div is one block in all three; and a Typst template meets a new layout under
package `0.5.0`. Six files change, by the fourteen edits below. Each edit replaces one passage
with text that is given in full, so nothing has to be worded by you.

## Read first
- `docs/spec/design.md` §0.3 (the model in four items)
- The section "The edits" below, whole, before you change a file

## Steps
1. Record what the tree holds before this task. From the repository root:
   ```sh
   git status --short > target/t101-before.txt
   ```
2. Make the fourteen edits of "The edits", in order. For each one, find the old text in the named
   file (it occurs exactly once) and replace it with the new text. Keep every line break as
   given: the guide is wrapped by hand at about 78 columns.
3. Run `cargo test --test guide`.
4. Run the checks of acceptance criteria 2 to 5, then the whole `cargo test` and the other three
   quality gates.

## Acceptance criteria
1. `cargo test --test guide` runs 11 tests and they pass. That test checks every link and anchor
   of the guide, and it rejects a task identifier (a capital T followed by two digits) in a guide
   page or an example README: do not add one.
2. Each of these commands prints nothing (the old statements are gone):
   ```sh
   git grep -n "divs only" -- docs/guide
   git grep -n "Word adds them" -- docs/guide
   git grep -n "Word drops them" -- docs/guide
   git grep -n "leading to the pitch" -- docs/guide
   git grep -n "leaves the PDF as it was" -- docs/guide
   git grep -n "Explicit theme rules" -- docs/guide
   git grep -n "hand-tuned" -- examples/letter/README.md
   ```
3. Each of these commands prints exactly one line (the new statements are there):
   ```sh
   git grep -n "builds it as Word does" -- docs/guide/configuration.md
   git grep -n "A div is one block in all three outputs" -- docs/guide/configuration.md
   git grep -n "never their sum" -- docs/guide/configuration.md
   git grep -n "weigh as much as a plain selector" -- docs/guide/customization.md
   git grep -n "Typst's leading is zero" -- docs/guide/customization.md
   git grep -n "rules of their own" -- docs/guide/customization.md
   git grep -n "built as Word builds them" -- docs/guide/outputs.md
   git grep -n "few points lower on the page" -- docs/guide/troubleshooting.md
   git grep -n "move with the lines too" -- docs/guide/troubleshooting.md
   git grep -n "leading of zero, so that lines" -- docs/guide/typst.md
   git grep -n "replaces the 3 mm after the date" -- examples/letter/README.md
   ```
4. `wc -l docs/guide/configuration.md docs/guide/customization.md docs/guide/outputs.md
   docs/guide/troubleshooting.md docs/guide/typst.md examples/letter/README.md` prints these
   line counts, in this order: 661, 431, 194, 144, 210 and 105.
5. In the output of `git status --short | diff target/t101-before.txt -` no line starts with `<`,
   and every line that starts with `>` names a file under "Files you own" (remove the file in
   `target/` afterwards). `docs/guide/customization.md` and `docs/guide/typst.md` may be in the
   list already, from T97; then they add no line.
6. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Rewording. The new texts were checked against the reference code on 2026-11-22: the numbers in
  them are measured, and the template notes of edit 10 were each compiled. If an old text cannot
  be found, stop and report which edit; do not write a replacement of your own.
- The specification (`docs/spec/**`), the research notes, `PLAN.md` and `docs/tasks/**`: the
  coordinator's.
- The code blocks of the guide, the letter example's theme and the starter template
  (`assets/typst/writer-doc/template.typ`): none of them changes.
- Source code and tests.

## Hints (verified)
- An edit's old text is quoted exactly, line for line. If your editor reports that it does not
  occur, look for a difference in line breaks first: search for the first eight words alone.
- `tests/guide.rs` reads every table row between the headings "`[design]`" and "Strictness" of
  `configuration.md` whose first cell is one name in backticks as a design key. Edits 3 and 4
  touch a table whose first cells are plain words; keep them so.
- Where the numbers come from. Libertinus Serif's ascent is 0.894 em and its cap height
  0.658 em, so the two differ by 0.236 em: 2.6 pt at 11 pt (edit 10). The letter example sets
  `text.size = "11pt"`, `text.line_height = 1.35` and `text.paragraph_spacing = "3.6mm"`:
  14.85 pt from line to line and 10.2 pt more across a paragraph break (edit 14). The 8 pt of
  edit 12 is what Typst's own heading spaces gained when they began to count between line boxes:
  above a heading, the part of the body's line box below its baseline (5.0 pt at 11 pt and 1.35)
  and the heading's ascent less its cap height (3.3 pt at 14 pt); below it, the same two parts
  with the roles exchanged (6.4 pt and 2.6 pt). R12 §7 has the measured gaps (7.6 to 9.0 pt).
  Edit 12's third paragraph is R12 §7's table "Blocks the model does not cover": a block quote
  gains the same two parts on each side (5.0 pt and 2.6 pt, 7.6 pt together); a figure, a table
  or a display equation loses 2.6 pt above and 5.0 pt below; a note's lines are 1.35 times
  9.35 pt apart and a code block's 1.35 times 8.8 pt.
- The four template rules of edit 10's footnote and equation item were each compiled after the
  `writer-doc` line of a delegating template (11 pt at 1.35): a note's lines went from 12.62 pt
  to 14.62 pt apart, two notes from 12.62 pt to 23.62 pt, an equation's rows from 17.38 pt to
  20.77 pt, and `set par(leading: 4pt)` moved the body's lines to 18.85 pt and neither of the
  other two.
- Cargo test filters go after `--`. The crate's dependencies are vendored, so every command works
  offline.

## The edits
Each edit names its file, then gives the old text and the new text in two blocks. The fences are
not part of either text.

### Edit 1: `docs/guide/configuration.md`, section "Body text"
Old:
```text
The PDF keeps the same geometry. Typst's own leading is the gap between one
line's bottom edge and the next line's top edge, which sits at the cap
height, so `writer-doc` measures the body font's cap height and sets the
leading to the pitch less that box, and the paragraph spacing to the leading
plus `text.paragraph_spacing`. Lines are then the multiple of the size apart
and a paragraph break adds exactly the spacing, in all three formats; named
styles and furniture lines follow the same rule at their own size. A Typst
template that delegates to `writer-doc` inherits this, and one that sets
`par(leading:)` itself overrides it.
```
New:
```text
The PDF keeps the same geometry and builds it as Word does. A line is a box
as tall as the line pitch, and the boxes of one paragraph stack with nothing
between them. The box holds the font's ascent above the baseline (less when
the line height is smaller than the font's own), or four fifths of the pitch
when the line height is a length. For that, `writer-doc` sets the top and
bottom edge of the text from the font's metrics and Typst's leading to zero.
Lines are then the multiple of the size apart and a paragraph break adds
exactly the spacing, in all three formats; named styles and furniture lines
follow the same rule at their own size. A Typst template that delegates to
`writer-doc` inherits this; [the PDF
template](customization.md#generated-typst-pdf-template) lists what it means
for a template's own rules.

HTML has the same pitch and the same spaces. CSS centres the text in its line
box, where Word and the PDF hang it from the font's ascent. A line whose size
or line height differs from the body's can therefore sit a little higher or
lower against the body text than in the other two outputs: under 1 pt for
headings and title lines at usual line heights, and more only for large text
on a line height far from the font's own.

Each output keeps its own default for a key the design leaves unset, as the
table shows. Set `text.size`, `text.line_height` and `text.paragraph_spacing`
when the three outputs should agree.
```

### Edit 2: `docs/guide/configuration.md`, section "Named paragraph styles"
Old:
```text
for `Header`, the style of the header lines. The PDF applies the same names to
the elements listed below. HTML applies named styles to divs only.
```
New:
```text
for `Header`, the style of the header lines. The PDF and HTML apply the same
names to the elements listed below.
```

### Edit 3: `docs/guide/configuration.md`, the table that follows
Old:
```text
| Element in the PDF | Style names |
```
New:
```text
| Element | Style names |
```

### Edit 4: `docs/guide/configuration.md`, the last row of that table
Old:
```text
| Header and footer lines (font and line height) | `Header`, `footer` |
```
New:
```text
| Header and footer lines in the PDF (font and line height) | `Header`, `footer` |
```

### Edit 5: `docs/guide/configuration.md`, the paragraph after that table
Old:
```text
In the PDF, `space_before` and `space_after` of these styles are added to a
full line, as in Word: a heading with `space_before = "20pt"` starts 20 pt
plus one line below the text above it. Where two styled blocks meet, the
larger of the two spaces applies; Word adds them. The first line of the title
block keeps its `space_before` at the top of the page. `Subtitle`, `Author`
and `Date` follow `Title`'s `align`; set their other keys on each style.
A caption uses the style's text keys, `align` and `line_height` only, and a
table cell takes `space_before` and `space_after` as padding above and below
its text.
```
New:
```text
`space_before` and `space_after` are spaces between line boxes ([body
text](#body-text)), in all three outputs: a heading with `space_before =
"20pt"` has 20 pt between the last line box above it and its own first one.
Where two blocks meet, the larger of the upper block's space after and the
lower block's space before applies, never their sum. A body paragraph counts
with `text.paragraph_spacing` after it and no space before, so a heading with
`space_before = "4pt"` still stands the paragraph spacing below a paragraph.
The first block of the document keeps its `space_before` at the top of the
first page; a block that starts a later page of the PDF or of Word loses it.
For a heading, a title line or a contents entry, a key that the style leaves
unset keeps each output's built-in space, and the three differ: set both keys
where the outputs should agree. `Subtitle`, `Author` and `Date` follow
`Title`'s `align` unless they set their own; set their other keys on each
style, because only Word hands `Title`'s down to them. A caption uses the
style's text keys, `align` and `line_height` only, and a table cell takes
`space_before` and `space_after` as padding above and below its text.
```

### Edit 6: `docs/guide/configuration.md`, two paragraphs further down
Old:
```text
Term`, `Definition`, `Source Code`, `Figure` and `Captioned Figure`. A design
that declares none of the names in the table leaves the PDF as it was.
```
New:
```text
Term`, `Definition`, `Source Code`, `Figure` and `Captioned Figure`. A design
that declares none of the names in the table adds none of these rules to the
PDF or to HTML.
```

### Edit 7: `docs/guide/configuration.md`, the next paragraph
Old:
```text
In Word, `space_before` and `space_after` of a div's style, of `Header` and of
`footer` surround the whole block: Word drops them between consecutive
paragraphs of the same style, as HTML and PDF space a div rather than the
paragraphs inside it. Every other built-in name keeps them between
consecutive paragraphs, because each body paragraph, heading or contents entry
is a block of its own: `space_after` on `Body Text` separates body paragraphs.
```
New:
```text
A div is one block in all three outputs: its style's `space_before` and
`space_after` surround the whole div, and the paragraphs inside it keep the
body's paragraph spacing between themselves. Two divs of the same style in a
row are two blocks, with the larger of their two spaces between them. For
lines that belong together without a gap, such as the lines of an address,
use line breaks inside one paragraph. The lines of `Header` and of `footer`
form one block with no space between them. Every other built-in name is a
block per body paragraph, heading or contents entry: `space_after` on `Body
Text` separates body paragraphs in Word.
```

### Edit 8: `docs/guide/customization.md`, section "Cross-format design"
Old:
```text
which you can declare the same way. The PDF applies the same styles to its
headings, title block, contents list, captions, table cells and furniture
lines, so one table styles both outputs; see [named paragraph
styles](configuration.md#named-paragraph-styles) for the list. HTML applies
named styles to divs only; its heading look comes from the theme's CSS.
`writer check` does not report any of these styles as unused.
```
New:
```text
which you can declare the same way. The PDF applies the same styles to its
headings, title block, contents list, captions, table cells and furniture
lines, and HTML to all of these except the furniture lines, so one table
styles all three outputs; see [named paragraph
styles](configuration.md#named-paragraph-styles) for the list.
`writer check` does not report any of these styles as unused.

In HTML these styles become CSS rules that follow the theme's stylesheets
and weigh as much as a plain selector for the element. A theme rule such as
`h1 { margin-top: 2em }` therefore loses to `Heading 1`'s `space_before`,
while a more specific one such as `.report h1 { margin-top: 2em }` still
wins, and a property the style does not set stays the theme's. A theme that
declares one of these styles can delete the CSS that restated it.
```

### Edit 9: `docs/guide/customization.md`, the table row in the same section
Old:
```text
| HTML theme CSS | Explicit theme rules and token overrides win; dark-mode theme rules still apply. |
```
New:
```text
| HTML theme CSS | Token overrides win, and so does a theme rule more specific than the design's rule for the same element; dark-mode theme rules still apply. |
```

### Edit 10: `docs/guide/customization.md`, section "Generated Typst PDF template"
The new text keeps the old lines and adds a paragraph and a list after them.

Old:
```text
value for the PDF alone, write the rule after the `writer-doc` line, for
example `show heading.where(level: 1): set block(above: 2em, below: 1em)`.
```
New:
```text
value for the PDF alone, write the rule after the `writer-doc` line, for
example `show heading.where(level: 1): set block(above: 2em, below: 1em)`.

With a `[design]`, `writer-doc` builds lines and spaces as Word does ([body
text](configuration.md#body-text)), and a template's own rules meet that
layout:

- A line's top edge is the font's ascent instead of its cap height. Text that
  a template places against the top of a line or of the page sits lower by
  the difference, 2.6 pt at 11 pt Libertinus Serif. A cover or another page
  that the template draws itself gets Typst's edges back by starting with
  `set text(top-edge: "cap-height", bottom-edge: "baseline")` and `set
  par(leading: 0.65em)`.
- Typst's leading is zero. `set par(leading: 4pt)` after the `writer-doc`
  line adds 4 pt between the line boxes; to change the line pitch, set
  `text.line_height` in the design or pass `leading:` to `writer-doc`.
- Footnotes and display equations have rules of their own, which that `set
  par(leading: ..)` does not reach. A footnote's lines are line boxes at the
  note's size: `show footnote.entry: set par(leading: 2pt)` after the
  `writer-doc` line adds 2 pt between them, and `set footnote.entry(gap:
  1em)` sets the space between two notes. The rows of a display equation
  keep the leading they had before, the line pitch less the cap height:
  `show math.equation.where(block: true): set par(leading: 1em)` sets
  another.
- `set par(spacing: 8pt)` after the `writer-doc` line sets the paragraph
  spacing, measured between line boxes.
- A first-line indent needs `all: true`, as in `set par(first-line-indent:
  (amount: 1em, all: true))`. Typst indents only a paragraph that directly
  follows another one, and the package ends every paragraph with an invisible
  block that carries its spacing.
- The first block of the document keeps its `space_before` at the top of the
  first page. A template that added a `v(..)` there for that removes it.
```

### Edit 11: `docs/guide/outputs.md`, section "PDF"
Old:
```text
compiler input. Named styles that carry a Word style name, such as
`Heading 1`, `Title` or `toc 2`, style the same elements here as in Word. A
custom template receives all of this when it delegates to `writer-doc`;
explicit arguments win, and so do Typst rules placed after the `writer-doc`
call ([PDF template](customization.md#generated-typst-pdf-template)).
```
New:
```text
compiler input. Named styles that carry a Word style name, such as
`Heading 1`, `Title` or `toc 2`, style the same elements here as in Word,
and lines and the spaces between blocks are built as Word builds them
([body text](configuration.md#body-text)). A custom template receives all of
this when it delegates to `writer-doc`; explicit arguments win, and so do
Typst rules placed after the `writer-doc` call ([PDF
template](customization.md#generated-typst-pdf-template)).
```

### Edit 12: `docs/guide/troubleshooting.md`
The new text keeps the old lines and adds three paragraphs after them.

Old:
```text
version ... does not exist (latest is ...)" names an older `@writer/doc` in
its `#import` line. Change the version to the latest one the message gives.
```
New:
```text
version ... does not exist (latest is ...)" names an older `@writer/doc` in
its `#import` line. Change the version to the latest one the message gives.

After that change, text in the PDF of a document with a `[design]` can sit a
few points lower on the page, and a template's own `set par(leading: ..)` can
open the lines up. The package builds its lines as Word does; [the PDF
template](customization.md#generated-typst-pdf-template) lists what a
template changes. A heading whose style is not declared, or sets no
`space_before` or `space_after`, also stands further from the text around
it, by about 8 pt at a body size of 11 pt and a line height of 1.35: Typst's
own heading spaces now count between line boxes. Set both keys on the
heading styles to choose the distances, in all three outputs.

Elements that take no named style in the PDF move with the lines too, here
at the same size and line height. A block quote stands about 8 pt further
from the text around it, as such a heading does. A figure, a table or a
display equation stands closer to it, by about 3 pt above and up to 5 pt
below: the paragraph spacing alone now lies between it and the line boxes
around it. The lines of a footnote and of a code block take the design's
line height at their own size.

A heading, a title line, a caption or a table cell in HTML that stopped
following the theme's CSS takes its look from a named style of the design,
which styles HTML as well as Word and the PDF. Remove the CSS rule, or make
it more specific than the plain element, for example `.report h1` instead of
`h1`; see [cross-format design](customization.md#cross-format-design).
```

### Edit 13: `docs/guide/typst.md`, section "Shared design in Typst"
The new text keeps the old lines and adds one paragraph after them.

Old:
```text
`title-block: none` to leave the title block out and a function `(meta, theme)
=> content` to draw another one.
```
New:
```text
`title-block: none` to leave the title block out and a function `(meta, theme)
=> content` to draw another one.

With a design, `writer-doc` sets the top and bottom edge of the text and a
leading of zero, so that lines and the spaces between blocks match Word and
HTML. A document that sets `par(leading:)`, `par(spacing:)` or a first-line
indent of its own after the `writer-doc` line should read the notes on [the
PDF template](customization.md#generated-typst-pdf-template) first.
```

### Edit 14: `examples/letter/README.md`
Old:
```text
(3.6 mm) across a paragraph break. In the PDF, `writer-doc` derives Typst's
leading and paragraph spacing from them and from the body font's measured cap
height, so the template needs no hand-tuned `par(leading:)`. Changing
`paragraph_spacing` in the theme changes all three outputs. A document can
override individual keys through front matter:
```
New:
```text
(3.6 mm) across a paragraph break. The named styles' spaces follow one rule
in all three: where two blocks meet, the larger of the upper block's
`space_after` and the lower block's `space_before` applies, so the 6 mm
before the salutation replaces the 3 mm after the date. In the PDF,
`writer-doc` builds its lines as Word does, so the letter needs no Typst
template for its spacing. Changing `paragraph_spacing` in the theme changes
all three outputs. A document can override individual keys through front
matter:
```
