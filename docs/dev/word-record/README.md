# Word record inputs (T103)

R13's T103 measures in Word for Mac what R12 §2 left open: where `atLeast`
lines put their extra height, the widow-control default, s and A of Word's
own fonts, and the spacing of lists, quotes, rules, code, equations, tables,
footnotes and captions. This folder holds what the owner needs to take
those readings: the DOCX inputs, their sources, a predicted value for every
reading with its derivation, and the reading sheet below.

Status, 2026-11-22: the inputs are built and the sheet is ready. No
reading is taken yet; the **Word** column is empty. Once it is filled, T103
adds the dated record to docs/dev/docx-validation.md and flags every
difference over 0.3 pt for the task named in the row (R13 §6.2, T103
"Accept").

The committed DOCX files were built with writer 0.3.1
(v0.3.1-16-gead21e88), the release binary of commit ead21e88.

## Before the readings

Record with the readings:
- the date, Word's version (Word > About Microsoft Word; the installed
  bundle said 16.115.1 on 2026-11-22, the earlier records in
  docx-validation.md used 16.114.3) and macOS's;
- the view and zoom: Print Layout at 500 %, as in R12 §2.1;
- the appearance: R12's captures show a dark page with light text (see C).

Fonts:
- w01 to w13 are set in **Source Sans 3** (Regular, Italic, SemiBold,
  Bold), which must be installed: the TTFs in ~/Library/Fonts are the ones
  R12 used and build.sh reads. If Word substitutes another font, every
  design (b) value moves. Check the font box on a body line.
- Calibri, Calibri Light, Consolas and Cambria with Cambria Math come with
  Word (Microsoft Word.app/Contents/Resources/DFonts).

Scripting Word: docx-validation.md's release checklist says not to automate
Word through osascript, since an earlier attempt hung. R12 §2.1 later took
its readings with the scripts in tools/, which drive Word through osascript
with timeouts. Methods C and P can be done by hand; Q and F need the
scripts.

## Inputs

| File | Source | Design | What it probes | Task |
|---|---|---|---|---|
| w01-atleast.docx | design/w01-atleast.md | (b) | where an `atLeast` line puts its extra (raw OpenXML) | T106 |
| w02-widows.docx | design/w02-widows.md | (b) | widow control: none, on, off (raw OpenXML); 12 pages | T118 |
| w03-calibri.docx | design/w03-calibri.md | (b) | s and A of Calibri at 24 pt | T106 |
| w04-calibri-light.docx | design/w04-calibri-light.md | (b) | s and A of Calibri Light | T106 |
| w05-consolas.docx | design/w05-consolas.md | (b) | s and A of Consolas | T106 |
| w06-cambria-math.docx | design/w06-cambria-math.md | (b) | s and A of Cambria Math (captured at 200 %) | T106 |
| w07-lists.docx | design/w07-lists.md | (b) | tight, loose and nested lists, definitions; R13 M4's item style (raw) | T112 |
| w08-quotes-rules.docx | design/w08-quotes-rules.md | (b) | block quote, nested quote, rule | T112 |
| w09-code.docx | design/w09-code.md | (b) | code today; one paragraph per line, three ways (raw) | T113 |
| w10-equations.docx | design/w10-equations.md | (b) | unnumbered, numbered and fraction equations | T113 |
| w11-tables.docx | design/w11-tables.md | (b) | rows and rules; two tables with today's and M10's separator; 3 pt rules (raw) | T114 |
| w12-footnotes.docx | design/w12-footnotes.md | (b) | note pitch, gap between notes, last note, separator | T115 |
| w13-captions.docx | design/w13-captions.md | (b) | figure, image caption, table caption | T110, T114 |
| w14-defaults.docx | default/w14-defaults.md | (a) | R13's (u) rows: body, heading, lists, quote, rule, code, equation, notes | T108, T109, T112, T113, T115 |
| w15-defaults-blocks.docx | default/w15-defaults-blocks.md | (a) | R13's (u) rows: table, figure | T110, T114 |

- **(b)**, design/writer.toml: R12's probe design (R12 §2.1), so that the
  readings sit next to R12 §2 and §8. Source Sans 3 11 pt at 1.35
  (`w:line` 228), P = 3.6 mm = 10.2 pt (Body Text 0 before, 204 twips
  after), A4, margins 10, 20, 30 and 20 mm (top, right, bottom, left). The
  30 mm bottom margin keeps Word's footer inside the margin. X12 and X20
  are exact 12.1 pt and 20 pt lines with no spaces: fences around the
  lines under test. FCalibri and its siblings set one family at 24 pt on
  `w:line` 240, one single line of the family (design/writer.toml says
  why); FCalibriX and its siblings are exact 30 pt fences in the same
  family.
- **(a)**, default/writer.toml: no design, writer's built-in Word styles
  (assets/docx/styles.xml): Calibri 11 pt at `w:line` 264, Body Text 6 pt
  before and after, Heading 1 Calibri Light 18 pt with 18 pt before and
  6 pt after, A4 with 1 in margins. R13's Word values marked (u) are
  computed for this case.
- **Raw OpenXML**: where R13 proposes XML that writer does not write yet
  (M4's item style, a code block as one paragraph per line, M10's 0.7 pt
  separator, thicker table rules, direct spacing after a table, the
  widow-control switch, `atLeast` lines), the source holds a raw
  `{=openxml}` block. The comments in each source say what it is.
- Every text line reads `H<tag> EFH`: flat-footed capitals starting with
  an H, so that captures can follow the H's left stem (R12 §2.1). The
  sheet names lines by their tag.

## Methods

The **Method** column names one or more. Run the commands below from
this folder (docs/dev/word-record) and keep their output outside the
repository, here in ~/word-record-out.

- **C**, baselines from window captures at 500 % (0.2 pt per pixel),
  R12 §2.1. With the scripts:
  `python3 tools/wordrun.py w07-lists.docx ~/word-record-out/w07 --captures 8`
  opens a copy, reads Q, captures the window (`--captures` captures, each
  `--step` small scrolls further down), reads the baselines of page 1 and
  prints both; it creates the output folder. By hand: open the DOCX in
  Print Layout at 500 %, scroll fully left so that the page's left edge
  shows and up so that its top edge shows, and capture the document
  window without its shadow (`screencapture -i -w -o c0.png`, then click
  the window; or Shift-Command-4, Space, then Option-click). Scroll down
  keeping at least two lines of the previous capture in view, capture
  again, and so on; then
  `python3 tools/wordcap.py c0.png c1.png ... --zoom 500` prints each
  baseline in pt from the page's top edge and the gap to the line above.
  - wordcap.py takes a capture's left edge to be Word's canvas and the
    first column that differs to be the page's left edge: a capture with
    the window's shadow, or without the page's left edge, does not read.
  - wordcap.py looks for ink brighter than the paper: R12's Word showed a
    dark page with light text. With a white page, invert each capture
    first (`magick c0.png -negate c0.png`; not tried here). It needs
    ImageMagick's `magick` on the PATH.
  - Page 1 only; the first capture must show the page's top edge, and
    each further capture must share at least two baselines with the ones
    before it.
  - **w06** (Cambria Math): its lines stand up to 134 pt apart, so the
    capture that adds HB2 must show HA2 to HB2, about 285 pt of page or
    1430 pixels at 500 %, more than a window holds. Capture w06 at 200 %
    and pass `--zoom 200` to wordcap.py. After wordq.py has opened it
    (Q, below): `python3 tools/wordshot.py ~/word-record-out/w06/c0.png
    --zoom 200 --up 60`; for each further capture the same command with
    c1.png, c2.png, ... and `--down 5` in place of `--zoom 200 --up 60`;
    then `python3 tools/wordcap.py ~/word-record-out/w06/c*.png --zoom
    200`. At 0.5 pt per pixel this still tells 134.59 from 28.80 and
    gives A to about 0.02; F11 also comes from Q at 500 %. wordrun.py
    captures at 500 % only.
  - wordrun.py pairs Q's page-1 lines with the captured baselines in
    order. A rule, a border, an image, an equation or a table row (two
    cells, one captured line, two Q paragraphs) breaks that pairing, and
    the note lines at the page's foot, which Q leaves out, add baselines.
    So first compare the number of wordcap.py's baselines with the tags.
    For w08, w10, w11, w13, w14 and w15 read wordcap.py's own list and
    match it to the tags by eye. In w14 the quote's left border, the
    empty rule paragraph and the equation come before D19 to D21; D24 and
    D25, like w12's N rows, are note lines that only wordcap.py lists.
  - Lines that do not start with an H cannot be read: caption labels,
    the equation number. A line with a footnote mark, and a note's first
    line (the mark comes first), may read off; the sheet avoids them
    (w12's HA1 and HB1 and w14's HU2 carry the marks).
- **Q**, Word's own line tops: `mkdir -p ~/word-record-out/w07` (wordq.py
  does not create the folder), then `python3 tools/wordq.py
  w07-lists.docx ~/word-record-out/w07/word.json --zoom 500` copies the
  DOCX to ~/Documents/writer-spacing-scratch (without `w:updateFields`),
  opens it, and prints every line's page and `vertical position relative
  to page` (pt from the page's top edge, on a 0.2 pt grid at 500 %) with
  the gap to the line above. `python3 tools/wordtops.py
  ~/word-record-out/*/word.json` reprints them; `python3
  tools/wordclose.py` closes the copies without saving and empties the
  scratch folder.
  - A line top leaves out the paragraph's space before, so the gap between
    two tops is the upper line's box + the space between (+ any border or
    rule). With equal boxes it equals the baseline gap; where it does not,
    the sheet gives the top gap as "Q x".
  - Q reads the main text only, not the notes. Each table cell is a
    paragraph: read the first cell of a row (the tag ending in 1).
- **P**, the page a line lands on: Q's page column, or Word's page view.
- **F**, footnote tops through AppleScript. This snippet was not run;
  run it (Script Editor or `osascript`) on the document wordq.py opened:

  ```applescript
  tell application "Microsoft Word"
      set d to active document
      set acc to ""
      repeat with i from 1 to count of footnotes of d
          set r to text object of footnote i of d
          set y to get range information r ¬
              information type vertical position relative to page
          set acc to acc & i & ": " & y & linefeed
      end repeat
      return acc
  end tell
  ```

- **V**, look at 500 % and describe.

The scripts in tools/ (wordq.py, wordrun.py, wordshot.py, wordcap.py,
wordclose.py, wordtops.py, winlist.swift) are R12's, copied unchanged from
its probe tools. They were not run for this preparation.

## How the predictions follow

The model is R12's line box and spaces (design.md §0.3; R12 §2.2 to §2.4)
with R13 §3.3's rule that every block element follows it:

- An auto line of `w:line` L at size z is (L/240)·s·z tall, with
  min(1, L/240)·A·z above the baseline and the rest below. An exact line
  of h has 0.8h above and 0.2h below.
- Between two paragraphs the larger of the space after and the space
  before holds. At a table edge nothing collapses: the paragraph's space,
  the rule and the cell paragraph's space add up (R12 §8's 27.67 pt). Rules
  and paragraph borders add their full width, a border also its `w:space`
  (R13 §3.3 rule 3).
- s and A come from the font tables, as src/docx/lines.rs computes them:
  s = (winAscent + winDescent + max(0, hhea lineGap − (winAscent +
  winDescent − hhea ascender + hhea descender))) / unitsPerEm, and
  A = s − |hhea descender| / unitsPerEm.

| Family | File | s | A |
|---|---|---|---|
| Source Sans 3 | SourceSans3-Regular.ttf | 1.424 | 1.024 |
| Calibri | DFonts/Calibri.ttf | 2500/2048 = 1.220703 | 1950/2048 = 0.952148 |
| Calibri Light | DFonts/calibril.ttf | 2500/2048 = 1.220703 | 1950/2048 = 0.952148 |
| Consolas | DFonts/Consola.ttf | 2398/2048 = 1.170898 | 1871/2048 = 0.913574 |
| Cambria Math | DFonts/Cambria.ttc, face 1 | 11428/2048 = 5.580078 | 10973/2048 = 5.357910 |
| Cambria | DFonts/Cambria.ttc, face 0 | 2401/2048 = 1.172363 | 1946/2048 = 0.950195 |

Cambria Math's win metrics are tall (6383 and 5045 units) to hold large
operators; its hhea and typo lines equal Cambria's. F10 to F12 give both:
the formula's value, and the value if Word uses those shorter lines.
`python3 predict.py --fonts` reads the numbers from the files again.

The line boxes the sheet uses (pt):

| Line | Box | Above | Below |
|---|---|---|---|
| (b) body: Source Sans 3 11 pt, `w:line` 228 | 14.881 | 10.701 | 4.180 |
| (b) single line: Source Sans 3 11 pt, 240 | 15.664 | 11.264 | 4.400 |
| (b) code: Consolas 10 pt, 228 | 11.124 | 8.679 | 2.445 |
| (b) code: Consolas 10 pt, 277 | 13.514 | 9.136 | 4.378 |
| (b) note: Source Sans 3 9 pt, 228 | 12.175 | 8.755 | 3.420 |
| (b) caption: Source Sans 3 10 pt, 228 | 13.528 | 9.728 | 3.800 |
| (a) body: Calibri 11 pt, 264 | 14.771 | 10.474 | 4.297 |
| (a) Heading 1: Calibri Light 18 pt, 264 | 24.170 | 17.139 | 7.031 |
| (a) code: Consolas 10 pt, 264 | 12.880 | 9.136 | 3.744 |
| (a) note: Calibri 9 pt, 264 | 12.085 | 8.569 | 3.516 |
| (a) caption: Calibri 10 pt, 264 | 13.428 | 9.521 | 3.906 |

Readings the model does not settle:
- **`atLeast` (A1, A3).** R13 gives no value (§3.1 M3 leaves `atLeast`
  lines to T103). Three hypotheses: H1, the extra goes above the text;
  H2, the text keeps its single-line ascent and the extra goes below;
  H3, the line behaves like an exact line (0.8 above, 0.2 below).
- **Equations (E1 to E6, D22, D23).** An OMML line's box is outside the
  model. "(M)" marks a value under the guess that it takes the body
  line's box; Q − 6 below an equation gives its box either way.
- **Images (C2, D31).** The image line's box is outside the model; Q − 3
  gives it.
- **Table edges (C5, T10, D29).** R12 §8's 27.67 pt shows that nothing
  collapses between a paragraph and the table below it: the larger-of
  rule falls about 3 pt short there. R12 read no table bottom edge with
  space on both sides, and R13 4.20 takes the larger of a caption's 6 pt
  after and the cell's 1.8 pt before. These rows give both values, the
  second marked "larger of"; the reading decides for T110 and T114.
- **Widows (W1, W2).** R13 0.25 says Word's default is likely on, about
  2 lines; the sheet gives the page each outcome predicts.

## Reading sheet

One table per input. Gaps are in pt, between baselines unless the row
says line tops; "(Q x)" is the gap between the two lines' tops where it
differs from the baseline gap, and "larger of: x" the gap if the spaces at
a table edge collapse to the larger one. "From the model" adds up the
terms (below, space, above: the parts of the two line boxes and what lies
between). "R13, R12" quotes R13 §2's row for the case (its legend: (a) no
design, (b) the probe design, (u) a Word value computed, not measured), or
R12 §8's Word measurement of the same transition, "other cells" where
R12's table cells had another style. Fill in **Word**; for a reading that
could not be taken, say why. **Task** names the task whose acceptance
compares against the reading (R13 §6.2): a difference over 0.3 pt is
flagged for it.

<!-- sheet:begin -->
### w01-atleast.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| A1 | HA2 → HB1 baselines (into a 30 pt atLeast line) | C | H1 28.02; H2 13.68; H3 26.42 | 2.42 below X12 + above HB1. H1, the extra above the text: 30 - 4.40 = 25.60; H2, ascent kept as in an auto line: 11.26; H3, as an exact line: 24 | R13: no value (§3.1 M3 leaves atLeast to T103) |  | T106 |
| A2 | HB1 → HB2 (the atLeast line's pitch) | C, Q | 30.00 | 30.00 w:line 600 | – |  | T106 |
| A3 | HB2 → HC1 baselines (out of the atLeast line) | C | H1 14.08; H2 28.42; H3 15.68 | below HB2 + 9.68 above X12. Below: H1 4.40, H2 18.74, H3 6 | R13: no value |  | T106 |
| A4 | HC2 → HD1 (into a 10 pt atLeast line, below a single line) | C | 13.68 | 2.42 below X12 + 11.26 single-line ascent | – |  | T106 |
| A5 | HD1 → HD2 | C, Q | 15.66 | 15.66 single line 1.424 em | – |  | T106 |
| A6 | HD2 → HE1 | C | 14.08 | 4.40 single-line descent + 9.68 above X12 | – |  | T106 |

### w02-widows.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| W1 | Test 1, no w:widowControl, orphan: the first test line on p2 | P | on: HB1; off: HB2 | 36 lines of 20 pt fit a page; widow control moves a lone first line (orphan) to the next page, or takes a second line along with a lone last line (widow) | R13 0.25: likely on, about 2 lines (u) |  | T118 |
| W2 | Test 2, no w:widowControl, widow: the first test line on p4 | P | on: HD4; off: HD5 | 36 lines of 20 pt fit a page; widow control moves a lone first line (orphan) to the next page, or takes a second line along with a lone last line (widow) | R13 0.25: likely on, about 2 lines (u) |  | T118 |
| W3 | Test 3, w:widowControl on, orphan: the first test line on p6 | P | on: HF1 | 36 lines of 20 pt fit a page; widow control moves a lone first line (orphan) to the next page, or takes a second line along with a lone last line (widow) | control |  | T118 |
| W4 | Test 4, w:widowControl on, widow: the first test line on p8 | P | on: HK4 | 36 lines of 20 pt fit a page; widow control moves a lone first line (orphan) to the next page, or takes a second line along with a lone last line (widow) | control |  | T118 |
| W5 | Test 5, w:widowControl off, orphan: the first test line on p10 | P | off: HM2 | 36 lines of 20 pt fit a page; widow control moves a lone first line (orphan) to the next page, or takes a second line along with a lone last line (widow) | control |  | T118 |
| W6 | Test 6, w:widowControl off, widow: the first test line on p12 | P | off: HP5 | 36 lines of 20 pt fit a page; widow control moves a lone first line (orphan) to the next page, or takes a second line along with a lone last line (widow) | control |  | T118 |

### w03-calibri.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| F1 | Calibri: HA2 → HB1 baselines (gives A) | C | 28.85 | 6 below the exact 30 pt fence + 24 A (0.952148) | R13 §0.3: s 1.2207; §6.4: A about 0.952 |  | T106 |
| F2 | Calibri: HB1 → HB2 (gives s) | C, Q | 29.30 | 24 s (1.220703) | R13 §0.3: s 1.2207; §6.4: A about 0.952 |  | T106 |
| F3 | Calibri: HB2 → HC1 baselines (check) | C | 30.45 | 24 (s - A) = 6.45 + 24 above the fence | R13 §0.3: s 1.2207; §6.4: A about 0.952 |  | T106 |

### w04-calibri-light.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| F4 | Calibri Light: HA2 → HB1 baselines (gives A) | C | 28.85 | 6 below the exact 30 pt fence + 24 A (0.952148) | – (R13 §3.1 M3: computed from the tables) |  | T106 |
| F5 | Calibri Light: HB1 → HB2 (gives s) | C, Q | 29.30 | 24 s (1.220703) | – (R13 §3.1 M3: computed from the tables) |  | T106 |
| F6 | Calibri Light: HB2 → HC1 baselines (check) | C | 30.45 | 24 (s - A) = 6.45 + 24 above the fence | – (R13 §3.1 M3: computed from the tables) |  | T106 |

### w05-consolas.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| F7 | Consolas: HA2 → HB1 baselines (gives A) | C | 27.93 | 6 below the exact 30 pt fence + 24 A (0.913574) | – (R13 §3.1 M3: computed from the tables) |  | T106 |
| F8 | Consolas: HB1 → HB2 (gives s) | C, Q | 28.10 | 24 s (1.170898) | – (R13 §3.1 M3: computed from the tables) |  | T106 |
| F9 | Consolas: HB2 → HC1 baselines (check) | C | 30.18 | 24 (s - A) = 6.18 + 24 above the fence | – (R13 §3.1 M3: computed from the tables) |  | T106 |

### w06-cambria-math.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| F10 | Cambria Math: HA2 → HB1 baselines (gives A) | C at 200 % | 134.59 | 6 below the exact 30 pt fence + 24 A (5.357910); with hhea metrics (as Cambria): 28.80 | – (R13 §3.1 M3: computed from the tables) |  | T106 |
| F11 | Cambria Math: HB1 → HB2 (gives s) | C at 200 %, Q | 133.92 | 24 s (5.580078); with hhea metrics (as Cambria): 28.14 | – (R13 §3.1 M3: computed from the tables) |  | T106 |
| F12 | Cambria Math: HB2 → HC1 baselines (check) | C at 200 % | 29.33 | 24 (s - A) = 5.33 + 24 above the fence; with hhea metrics (as Cambria): 29.33 | – (R13 §3.1 M3: computed from the tables) |  | T106 |

### w07-lists.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| L1 | HA1 → HB1, paragraph → tight item | C, Q | 25.08 | 4.18 below + 10.20 P (Compact 1.8 before) + 10.70 above | R12 §8 Word 25.00 |  | T112 |
| L2 | HB1 → HB2, tight item → item | C, Q | 16.68 | 4.18 below + 1.80 Compact + 10.70 above | R12 §8 Word 16.80 |  | T112 |
| L3 | HB2 → HC1, item → nested item | C, Q | 16.68 | 4.18 below + 1.80 Compact + 10.70 above | R13 2.6: 1.8 pt |  | T112 |
| L4 | HC2 → HB3, nested item → next item | C, Q | 16.68 | 4.18 below + 1.80 Compact + 10.70 above | R13 2.7: 1.8 pt |  | T112 |
| L5 | HB3 → HD1, tight list → paragraph | C, Q | 16.68 | 4.18 below + 1.80 Compact after (paragraph before 0) + 10.70 above | R12 §8 Word 16.60; R13 2.3 (b) |  | T112 |
| L6 | HD1 → HE1, paragraph → loose item | C, Q | 25.08 | 4.18 below + 10.20 P + 10.70 above | – |  | T112 |
| L7 | HE1 → HE2, loose item → item | C, Q | 25.08 | 4.18 below + 10.20 P + 10.70 above | R12 §8 Word 25.20; R13 2.4 (b) |  | T112 |
| L8 | HE3 → HF1, loose list → paragraph | C, Q | 25.08 | 4.18 below + 10.20 P + 10.70 above | – |  | T112 |
| L9 | HG1 → HG2, paragraphs in one item | C, Q | 25.08 | 4.18 below + 10.20 P + 10.70 above | R13 2.5 (a) only |  | T112 |
| L10 | HH1 → HK1, paragraph → term | C, Q | 25.08 | 4.18 below + 10.20 max(P, 6) + 10.70 above | R13 2.19 (b): 25.08 (u) |  | T112 |
| L11 | HK1 → HL1, term → definition | C, Q | 14.88 | 4.18 below + 0.00 term after 0, definition before 0 + 10.70 above | – |  | T112 |
| L12 | HL1 → HK2, definition → next term | C, Q | 20.88 | 4.18 below + 6.00 6 + 6 → 6 + 10.70 above | R13 2.19 (b): 20.88 (u) |  | T112 |
| L13 | HL2 → HM1, definition → paragraph | C, Q | 20.88 | 4.18 below + 6.00 definition after 6 + 10.70 above | – |  | T112 |
| L14 | HM1 → HN1, paragraph → M4 item | C, Q | 25.08 | 4.18 below + 10.20 P (LI before 0) + 10.70 above | R13 M4 |  | T112 |
| L15 | HN1 → HN2, M4 item → item | C, Q | 17.63 | 4.18 below + 2.75 S + 10.70 above | R13 M4: S = 2.75 |  | T112 |
| L16 | HN2 → HP1, M4 item → nested item | C, Q | 17.63 | 4.18 below + 2.75 S + 10.70 above | R13 M4 |  | T112 |
| L17 | HP2 → HN3, M4 nested → next item | C, Q | 17.63 | 4.18 below + 2.75 S + 10.70 above | R13 M4 |  | T112 |
| L18 | HN3 → HQ1, M4 last item (after P) → paragraph | C, Q | 25.08 | 4.18 below + 10.20 direct after P + 10.70 above | R13 M4 |  | T112 |

### w08-quotes-rules.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| Q1 | HA1 → HB1, paragraph → quote | Q | 25.08 | 4.18 below + 10.20 max(P, 5) + 10.70 above | R12 §8 Word 25.20; R13 2.25 (b) |  | T112 |
| Q2 | HB1 → HB2, inside the quote | Q | 19.88 | 4.18 below + 5.00 5 + 5 → 5 + 10.70 above | R13 2.26 (a) only |  | T112 |
| Q3 | HB2 → HC1, into the nested quote | Q | 19.88 | 4.18 below + 5.00 5 + 10.70 above | R13 2.27: same style |  | T112 |
| Q4 | HC1 → HB3, out of the nested quote | Q | 19.88 | 4.18 below + 5.00 5 + 10.70 above | – |  | T112 |
| Q5 | HB3 → HD1, quote → paragraph | Q | 19.88 | 4.18 below + 5.00 max(5, 0) + 10.70 above | R12 §8 Word 19.80; R13 2.25 (b) |  | T112 |
| Q6 | HD1 → HE1, paragraph, rule, paragraph | Q | 47.71 | 4.18 below + 10.20 P + 14.88 rule's empty line + 1.00 border space + 0.75 border + 6.00 rule after + 10.70 above | R13 2.31 (a) only |  | T112 |

### w09-code.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| K1 | HA1 → HB1, paragraph → code (today) | C, Q | 23.06 (Q 25.08) | 4.18 below + 10.20 P + 8.68 code above; Q: 14.88 body line + 10.20 P | R12 §8 Word 23.00 |  | T113 |
| K2 | HB1 → HB2, code line pitch (today) | C, Q | 11.12 | 11.12 Consolas 10 pt at w:line 228 | R12 §8 Word 11.20 |  | T113 |
| K3 | HB3 → HC1, code → paragraph (today) | C, Q | 19.15 (Q 17.12) | 2.44 code below + 6.00 code after + 10.70 above; Q: 11.12 code line + 6.00 code after | R12 §8 Word 19.20 |  | T113 |
| K4 | HC1 → HD1, paragraph → code (c1) | C, Q | 23.06 (Q 25.08) | 4.18 below + 10.20 P + 8.68 code above; Q: 14.88 body line + 10.20 P | – |  | T113 |
| K5 | HD1 → HD2, code line pitch (c1) | C, Q | 11.12 | 11.12 Consolas 10 pt at w:line 228 | R13 3.7 (a) only |  | T113 |
| K6 | HD3 → HE1, code → paragraph (c1) | C, Q | 19.15 (Q 17.12) | 2.44 code below + 6.00 code after + 10.70 above; Q: 11.12 code line + 6.00 code after | – |  | T113 |
| K7 | HE1 → HF1, paragraph → code (c2) | C, Q | 23.06 (Q 25.08) | 4.18 below + 10.20 P + 8.68 code above; Q: 14.88 body line + 10.20 P | – |  | T113 |
| K8 | HF1 → HF2, code line pitch (c2) | C, Q | 11.12 | 11.12 Consolas 10 pt at w:line 228 | R13 3.7 (a) only |  | T113 |
| K9 | HF3 → HG1, code → paragraph (c2) | C, Q | 19.15 (Q 17.12) | 2.44 code below + 6.00 code after + 10.70 above; Q: 11.12 code line + 6.00 code after | – |  | T113 |
| K10 | HH2 → HK1, two c1 blocks | C, Q | 17.12 | 2.44 code below + 6.00 6 + 6 → 6 + 8.68 code above | – |  | T113 |
| K11 | HM2 → HN1, two c2 blocks | C, Q | 11.12 | 11.12 contextualSpacing drops both | – |  | T113 |
| K12 | HP1 → HQ1, paragraph → code (c3, w:line 277) | C, Q | 23.52 (Q 25.08) | 4.18 below + 10.20 P + 9.14 code above; Q: 14.88 body line + 10.20 P | R13 §3.1 M3: a code line height on the mono face |  | T113 |
| K13 | HQ1 → HQ2, code line pitch (c3) | C, Q | 13.51 | 13.51 Consolas 10 pt at w:line 277 | R13 §3.1 M3 |  | T113 |
| K14 | HQ3 → HR1, code → paragraph (c3) | C, Q | 21.08 (Q 19.51) | 4.38 code below + 6.00 code after + 10.70 above; Q: 13.51 code line + 6.00 code after | R13 §3.1 M3 |  | T113 |
| K15 | Shading of HD, HF, HQ: one grey block per code block, no white seam between lines; HQ2's yellow bar spans the full width | V | yes | per-line paragraphs with equal shading and no space | R13 3.19 |  | T113 |

### w10-equations.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| E1 | HA1 → HE1, paragraph → unnumbered equation | Q, C | Q 25.08; C 25.08 (M) | Q: 14.88 body line + 10.20 max(P, 6); C under M, the math line taking the body's box | R13 3.22 (a) only |  | T113 |
| E2 | HE1 → HB1, unnumbered equation → paragraph | Q, C | 20.88 (M) | Q: the equation's box + 6 (max(6, 0)); under M 14.88 + 6. Record the box: Q - 6 | – |  | T113 |
| E3 | HB1 → HE2, paragraph → numbered equation (a table) | Q, C | 31.08 | 14.88 body line + 10.20 P + 6 the cell's before: no collapse at a table edge (C the same under M) | – |  | T113 |
| E4 | HE2 → HC1, numbered equation → paragraph | Q, C | 20.88 (M) | the row's height (6 + box + 6, the taller of the equation and the (1) cell) less the cell's before, paragraph before 0 | – |  | T113 |
| E5 | HC1 → HE3, paragraph → fraction equation | Q | 25.08 | 14.88 + 10.20, whatever the equation's box | – |  | T113 |
| E6 | HE3 → HD1, fraction equation → paragraph | Q | box + 6 | record the fraction's box (Q - 6); no model value | – |  | T113 |

### w11-tables.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| T1 | HA1 → HB1, paragraph → header row | C, Q | 27.88 | 4.18 below + 10.20 P + 1.00 top rule + 1.80 cell before + 10.70 above | R12 §8 Word 27.67 (other cells); R13 4.21: the paragraph's after |  | T114 |
| T2 | HB1 → HC1, header row → row | C, Q | 18.98 | 4.18 below + 1.80 cell after + 0.50 header rule + 1.80 cell before + 10.70 above | – |  | T114 |
| T3 | HC1 → HD1, row → row | C, Q | 18.48 | 4.18 below + 1.80 cell after + 1.80 cell before + 10.70 above | R13 4.6 (a) only |  | T114 |
| T4 | HD1 → HE1, last row → paragraph | C, Q | 17.68 | 4.18 below + 1.80 cell after + 1.00 bottom rule + 10.70 above | R12 §8 Word 18.67 (other cells); R13 4.22 (b): 0 pt before |  | T114 |
| T5 | HG1 → HH1, two tables, today's separator | C, Q | 45.56 | 4.18 below + 1.80 cell after + 1.00 rule + 14.88 `<w:p/>` line + 10.20 its after (docDefaults) + 1.00 rule + 1.80 cell before + 10.70 above | R13 M10: about 14.8 pt line |  | T114 |
| T6 | HN1 → HP1, two tables, M10's 0.7 pt separator | C, Q | 30.68 | 4.18 below + 1.80 cell after + 1.00 rule + 9.50 before + 0.70 exact line + 1.00 rule + 1.80 cell before + 10.70 above | R13 M10 |  | T114 |
| T7 | HR1 → HS1, paragraph → 3 pt top rule | C, Q | 29.88 | 4.18 below + 10.20 P + 3.00 top rule + 1.80 cell before + 10.70 above | R13 §3.3 rule 3: T1 + 2 |  | T114 |
| T8 | HS1 → HT1, 1.5 pt header rule | C, Q | 19.98 | 4.18 below + 1.80 cell after + 1.50 header rule + 1.80 cell before + 10.70 above | R13 §3.3 rule 3: T2 + 1 |  | T114 |
| T9 | HU1 → HV1, 3 pt bottom rule → paragraph | C, Q | 19.68 | 4.18 below + 1.80 cell after + 3.00 bottom rule + 10.70 above | R13 §3.3 rule 3: T4 + 2 |  | T114 |
| T10 | HX1 → HY1, last row → paragraph with before 10.2 | C, Q | 27.88; larger of: 26.08 | 4.18 below + 1.80 cell after + 1.00 bottom rule + 10.20 direct before + 10.70 above; larger of: 4.18 below + 10.20 max(1.8, P) + 1.00 bottom rule + 10.70 above | R13 M10 |  | T114 |

### w12-footnotes.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| N1 | HN2 → HN3, line pitch in a note | C | 12.18 | 12.18 9 pt at w:line 228 | R12 §8: 12.15 (computed); R13 4.28 (a) only |  | T115 |
| N2 | HN3 → HP2, note → note (one note line between) | C | 27.35 | 3.42 below + 3.00 note after + 12.18 HP1 line + 8.76 above | R13 4.29: 3 pt |  | T115 |
| N3 | HP3's baseline from the page's top edge | C | 753.43 or 750.43 | the text area ends at 756.85; the last note's box ends there, or 3 pt higher if Word keeps its space after | – |  | T115 |
| N4 | Note 1's top → note 2's top (footnote snippet) | F | 39.53 | 3 × 12.18 + 3 | R13 4.29 |  | T115 |
| N5 | Separator line (wordcap's lower edge) → HN2 baseline | C | record | Word's built-in separator in a Normal paragraph (14.88 line, 10.20 after from docDefaults); where Word draws the line in it is not in the model | R13 4.26 (u) |  | T115 |

### w13-captions.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| C1 | HA1 → figure line, line tops | Q | 25.08 | 14.88 + 10.20 max(P, 6) | R13 1.37 (a) only |  | T110 |
| C2 | figure line → caption, line tops | Q | box + 3 | the 72 pt image's line box + 3 (3 after, 3 before): record the box (Q - 3); no model value | R13 1.35 (a): 3 pt |  | T110 |
| C3 | caption → HB1, line tops | Q | 19.53 | 13.53 caption line (10 pt) + 6 max(6, 0) | R13 1.35 (a): 6 pt |  | T110 |
| C4 | HB1 → table caption, line tops | Q | 25.08 | 14.88 + 10.20 max(P, 3) | – |  | T110 |
| C5 | table caption → HS1, line tops | Q | 22.33; larger of: 20.53 | 13.53 + 6 caption after + 1 top rule + 1.8 cell before; larger of: 13.53 + 6 max(6, 1.8) + 1 top rule | R13 4.20: 6 pt, the larger of 6 and 1.8 |  | T110, T114 |
| C6 | HT1 → HC1, table → paragraph (as T4) | C, Q | 17.68 | 4.18 below + 1.80 cell after + 1.00 bottom rule + 10.70 above | R13 4.22 (b): 0 pt before |  | T110 |

### w14-defaults.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| D1 | HA1 → HA2, body line pitch | C, Q | 14.77 | 14.77 Calibri 11 pt at w:line 264 | R13 0.4: 14.77 (u) |  | T108 |
| D2 | HA2 → HB1, paragraph → paragraph | C, Q | 20.77 | 4.30 below + 6.00 6 + 6 → 6 + 10.47 above | R13 0.5: 6 pt (u) |  | T108 |
| D3 | HB1 → HC1, body → Heading 1 | C, Q | 39.44 (Q 32.77) | 4.30 below + 18.00 H1 before + 17.14 H1 above (18 pt); Q: 14.77 body line + 18.00 H1 before | R13 1.5: about 39.4 (u) |  | T109 |
| D4 | HC1 → HD1, Heading 1 → body | C, Q | 23.50 (Q 30.17) | 7.03 H1 below + 6.00 6 + 6 → 6 + 10.47 above; Q: 24.17 H1 line + 6.00 6 + 6 → 6 | R13 1.6: 6 pt |  | T109 |
| D5 | HD1 → HE1, paragraph → tight list | C, Q | 20.77 | 4.30 below + 6.00 max(6, 1.8) + 10.47 above | R13 2.2: 20.77 (u) |  | T112 |
| D6 | HE1 → HE2, line pitch in an item | C, Q | 14.77 | 14.77 w:line 264 | R13 2.0: 14.77 (u) |  | T112 |
| D7 | HE2 → HE3, tight item → item | C, Q | 16.57 | 4.30 below + 1.80 Compact + 10.47 above | R13 2.1: 1.8 pt |  | T112 |
| D8 | HE3 → HF1, tight list → paragraph | C, Q | 20.77 | 4.30 below + 6.00 max(1.8, 6) + 10.47 above | R13 2.3 (a): 20.77 (u) |  | T112 |
| D9 | HF1 → HG1, paragraph → loose item | C, Q | 20.77 | 4.30 below + 6.00 6 + 10.47 above | – |  | T112 |
| D10 | HG1 → HG2, loose item → item | C, Q | 20.77 | 4.30 below + 6.00 6 + 10.47 above | R13 2.4 (a): 20.77 (u) |  | T112 |
| D11 | HG2 → HG3, paragraphs in one item | C, Q | 20.77 | 4.30 below + 6.00 6 + 10.47 above | R13 2.5: 20.77 (u) |  | T112 |
| D12 | HK1 → HL1, paragraph → term | C, Q | 20.77 | 4.30 below + 6.00 max(6, 6) + 10.47 above | – |  | T112 |
| D13 | HL1 → HM1, term → definition | C, Q | 14.77 | 14.77 line; 0 after, 0 before | – |  | T112 |
| D14 | HM1 → HN1, definition → paragraph | C, Q | 20.77 | 4.30 below + 6.00 max(6, 6) + 10.47 above | – |  | T112 |
| D15 | HN1 → HP1, paragraph → quote | Q | 20.77 | 4.30 below + 6.00 max(6, 5) + 10.47 above | R13 2.25 (a): 20.77 (u) |  | T112 |
| D16 | HP1 → HP2, inside the quote | Q | 19.77 | 4.30 below + 5.00 5 + 10.47 above | R13 2.26: 19.77 (u) |  | T112 |
| D17 | HP2 → HQ1, quote → paragraph | Q | 20.77 | 4.30 below + 6.00 max(5, 6) + 10.47 above | R13 2.25 (a): 20.77 (u) |  | T112 |
| D18 | HQ1 → HR1, paragraph, rule, paragraph | Q | 43.29 | 4.30 below + 6.00 max(6, 6) + 14.77 rule's line + 1.00 border space + 0.75 border + 6.00 max(6, 6) + 10.47 above | R13 2.31: about 43.3 (u) |  | T112 |
| D19 | HR1 → HS1, paragraph → code | C, Q | 19.43 (Q 20.77) | 4.30 below + 6.00 max(6, 6) + 9.14 code above; Q: 14.77 body line + 6.00 space | R13 3.12: 6 pt |  | T113 |
| D20 | HS1 → HS2, code line pitch | C, Q | 12.88 | 12.88 Consolas 10 pt at w:line 264 | R13 3.7: about 12.9 (u) |  | T113 |
| D21 | HS2 → HT1, code → paragraph | C, Q | 20.22 (Q 18.88) | 3.74 code below + 6.00 max(6, 6) + 10.47 above; Q: 12.88 code line + 6.00 space | R13 3.12: 6 pt |  | T113 |
| D22 | HT1 → HX1, paragraph → equation | Q | 20.77 | 14.77 + 6 max(6, 6) | R13 3.22: 6 pt |  | T113 |
| D23 | HX1 → HU1, equation → paragraph | Q | 20.77 (M) | the equation's box + 6; under M the body's box | R13 3.22: 6 pt |  | T113 |
| D24 | HN2 → HN3, line pitch in a note | C | 12.08 | 12.08 9 pt at w:line 264 | R13 4.28: about 12.1 (u) |  | T115 |
| D25 | HN3 → HO2, note → note (one note line between) | C | 27.17 | 3.52 below + 3.00 note after + 12.08 HO1 line + 8.57 above | R13 4.29: 3 pt |  | T115 |

### w15-defaults-blocks.docx

| # | Read | Method | Predicted (pt) | From the model | R13, R12 | Word | Task |
|---|---|---|---|---|---|---|---|
| D26 | HU1 → HV1, paragraph → header row | C, Q | 23.57 | 4.30 below + 6.00 after + 1.00 top rule + 1.80 cell before + 10.47 above | R13 4.21: the paragraph's 6 pt after |  | T114 |
| D27 | HV1 → HW1, header row → row | C, Q | 18.87 | 4.30 below + 1.80 after + 0.50 header rule + 1.80 before + 10.47 above | – |  | T114 |
| D28 | HW1 → HY1, row → row | C, Q | 18.37 | 4.30 below + 1.80 after + 1.80 before + 10.47 above | R13 4.6: about 18.4 (u) |  | T114 |
| D29 | HY1 → HZ1, last row → paragraph | C, Q | 23.57; larger of: 21.77 | 4.30 below + 1.80 cell after + 1.00 bottom rule + 6.00 paragraph before + 10.47 above; larger of: 4.30 below + 6.00 max(1.8, 6) + 1.00 bottom rule + 10.47 above | R13 4.22 (a): the paragraph's 6 pt before |  | T114 |
| D30 | HZ1 → figure line, line tops | Q | 20.77 | 14.77 + 6 max(6, 6) | R13 1.37: 6 pt |  | T110 |
| D31 | figure line → caption, line tops | Q | box + 3 | record the image line's box (Q - 3) | R13 1.35: 3 pt |  | T110 |
| D32 | caption → HZ2, line tops | Q | 19.43 | 13.43 caption line + 6 | R13 1.35: 6 pt |  | T110 |
<!-- sheet:end -->

## Rebuilding

`sh build.sh [path/to/writer]` rebuilds every DOCX from design/ and
default/ in a temporary folder (default writer: target/release/writer). It
copies Source Sans 3 from `$WORD_RECORD_FONTS` (default ~/Library/Fonts)
into the design project, since writer must read the font to write
`w:line` 228, and stops if it did not. Two builds give identical files.

`python3 predict.py` prints the sheet, `python3 predict.py --write`
rewrites it above, and `python3 predict.py --fonts` checks the metrics
against the font files.
