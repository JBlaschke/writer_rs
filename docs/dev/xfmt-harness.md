# Cross-format measurement harness

Writer's purpose is one set of defaults that every format renders the same
way. The harness checks that claim where it is easiest to lose: the vertical
space between lines. One command builds each reference document as DOCX, PDF
and HTML, reads every text baseline in each format and prints one row per
gap between consecutive lines, with a verdict against the tolerances of R13
§0.2.

The harness is a developer tool. **CI does not run it**: it needs Chrome,
Ghostscript, Node and the reference fonts, and its numbers follow the Chrome
version. Run it by hand before and after a change to spacing, and record the
tables again when a change is meant to move them.

## Requirements

- Python 3.11 or later (`tomllib`), standard library only.
- Node 22 or later (the built-in `WebSocket`).
- Ghostscript (`gs`) with the `txtwrite` device.
- Google Chrome or Chromium. The harness looks in `/Applications`; set
  `CHROME_PATH` to use another browser binary.
- A writer binary: `target/release/writer` by default, else `--writer` or
  `XFMT_WRITER`. The harness does not build it.

## Running

```sh
cargo build --release
python3 scripts/xfmt/run.py                 # every document, tables only
python3 scripts/xfmt/run.py --check         # compare with the expected tables
python3 scripts/xfmt/run.py --update        # record the expected tables
python3 scripts/xfmt/run.py p3 p7 --check   # some documents of the manifest
python3 scripts/xfmt/run.py --doc my.md --config writer.toml
```

`--out` sets where the builds go (default `target/xfmt`). `--strict` also
fails when a gap is outside tolerance. `--verbose` passes on what the
DevTools client writes to stderr. With `--doc`, `--media`, `--root` and
`--width` stand in for the manifest fields below; an `.html` file is measured
as it is, in Chrome only.

Exit status: 0 when every table was made (and, with `--check`, matches);
1 when a table differs from its expected table, or with `--strict` when a gap
is outside tolerance; 2 when a document could not be built or measured. A
full run of the nine reference documents takes 6 to 9 s here.

## What it measures

For each document of `tests/xfmt/xfmt.toml`:

1. `writer build --to html,pdf,docx,typst` builds it.
2. **PDF.** Ghostscript's `txtwrite` at 7200 dpi gives each span's baseline
   in 0.01 pt (`baselines.py`). At 720 dpi, odd multiples of 0.75 pt fall on
   half units. Spans that overlap vertically form one line, so a footnote
   mark or a subscript stays with its line.
3. **HTML on screen.** One headless Chrome loads the HTML with the
   document's media emulated and a viewport as wide as the DOCX's text
   width (`cdp-print.mjs`). `lines.js` reads each line box's baseline. Each
   baseline is the fragment top plus the font's ascent, which is measured
   with a zero-size inline-block marker. It also reads the offset of each
   block's first baseline below its content top.
4. **HTML printed.** The same Chrome prints the page with `Page.printToPDF`.
   Ghostscript reads the baselines as for the PDF.
5. **Word.** `word.py` lays the DOCX out from its XML with Word's model,
   R12 §2. An exact line height h puts 0.8 h above the baseline and 0.2 h
   below. An auto line L = `line`/240 makes a box of L times the font's
   natural height, with min(1, L) times the ascent above the baseline. The
   larger of space after and space before separates paragraphs.
   `contextualSpacing` applies. The first block on a page keeps its space
   before only at the start of the document. Forced and natural page breaks
   apply. Word is not run.

Word's single line s and ascent A per font come from the build's
`writer-design.json` (`lines`), else from the font files (the document's
folder, `vendor/typst-assets/files/fonts`, the system font folders and
Word's own fonts), else from a short list in `metrics.py`.

Lines are matched across the formats by their text. The key drops a list
marker, a contents leader with its page number, and spaces. The first 16
characters of what is left are matched in order with `difflib`.

## The table

```text
== p3
   Word     PDF  screen   scr+c   print  verdict            line
  39.05   39.02       -       -   39.00  ok                 p1 top: HA1 EFH (FirstParagraph)
  25.08   25.06   25.04   25.04   24.75  ok                 HA1 EFH → HA2 EFH (BodyText)
      -  832.73       -       -       -  -                  p1 at: 1
xfmt result: p3: 15 gaps checked, 0 outside tolerance; print: 15 baselines, 0 off the 0.75 pt grid, mean gap after c -0.01 pt against the PDF over 14 gaps ok
```

Rows follow the PDF's lines; a document without a PDF follows the screen
layout. Values are in pt.

- **`pN top:`** is the first line of page N, measured from the page top.
- **`A → B`** is the gap from line A's baseline to line B's. The style in
  brackets is B's Word style, else its HTML `data-style`, class or tag.
- **`pN at:`** is a PDF line that neither Word nor the screen layout has,
  such as a page number in the footer, measured from the page top. It
  takes no part in the gaps around it.

Columns:

- **Word**: the model's gap. It is empty when a table, a drawing, an
  equation or a frame lies between the two lines, because the model does
  not lay those out.
- **PDF**: Typst's gap, read from the PDF.
- **screen**: Chrome's gap on screen.
- **scr+c**: the screen gap corrected for c. c is how far Chrome puts a
  block's first baseline below the block's content top, minus the space
  Word puts above that baseline (R13 §3.2). Two fonts at the same line
  height can give different c, so the gap after correction is screen minus
  (c of B minus c of A). It is empty for a line Word does not have.
- **print**: Chrome's gap in its printed PDF. Chrome prints on a 0.75 pt
  grid, so a gap here is within about 0.75 pt of the screen gap.

Verdict: `ok` when every comparison of the row holds, `-` when nothing was
compared, else each fault:

- `W±d`: Word's gap minus the PDF's, beyond 0.3 pt.
- `H±d`: the scr+c gap minus the PDF's, beyond 0.3 pt.
- `Pr±d`: a printed position minus the PDF's, beyond 0.5 pt. It is checked
  at page tops (after c) and on `at:` rows, where both are positions from the
  page top.
- `Pr page N`: the printed HTML puts this line on another page than the PDF
  does, relative to the lines before.

The `xfmt result` line gives the number of rows compared and of rows with a
fault. It also gives the number of printed baselines off the 0.75 pt grid by
more than 0.01 pt; each one gets an `off grid:` line above it. Last comes
the mean of (print gap minus the change in c minus the PDF gap). It is `ok`
within 0.1 pt over at least 8 gaps, else `outside`.

## Tolerances

From R13 §0.2:

| Comparison | Tolerance |
|---|---|
| Word model ↔ PDF, per gap | 0.3 pt |
| HTML screen + c ↔ PDF, per gap | 0.3 pt |
| HTML printed ↔ PDF, per position after c | 0.5 pt |
| HTML printed ↔ PDF, mean gap after c over ≥ 8 gaps | 0.1 pt |
| HTML printed baselines on the 0.75 pt grid | 0.01 pt |

The DOCX's XML is compared exactly elsewhere (the Rust tests); the harness
only reads it.

## What the Word model leaves out

- **Wrapping.** The model does not break lines. A paragraph whose text is
  the exact join of several PDF lines takes the PDF's line breaks;
  otherwise it is one line.
- **Tables, drawings, equations and frames** are opaque. Lines on either
  side keep their positions relative to their own segment, but no gap is
  computed across them.
- **Widow control and keep rules** are not modelled, so Word's page breaks
  can come earlier than the model's.
- **`atLeast` line heights** are used only when the height is at most the
  natural one; otherwise the line gets no Word value.
- **Notes, headers and footers** are not modelled.
- Word's page top comes from the DOCX's `w:pgMar`. When a design sets no
  margins, writer's DOCX has 1 inch where the PDF has 25 mm. That is a
  difference between the outputs, not a model limit.

These are model limits, not Word measurements. R12 §2 lists what was
measured in Word itself.

## Chrome

The harness drives **one long-lived headless Chrome over the DevTools
protocol** (`cdp.mjs`) for all documents of a run. Chrome's one-shot headless
modes (dumping the DOM, printing to PDF or taking a screenshot from the
command line) hang on this machine and must not be used in `scripts/xfmt`.

Each run uses a fresh profile directory whose name starts with
`xfmt-chrome-`, in `$TMPDIR`. `run.py` starts the DevTools client in its own
process group. On every way out (success, a failed document, a timeout of
300 s, Ctrl-C, SIGTERM or SIGHUP) it stops that group, kills anything still
running with the profile, and removes the profile. After a run,
`pgrep -fl xfmt-chrome` prints nothing. Only SIGKILL of `run.py` itself skips
this. The client then finishes its jobs, closes Chrome and removes the
profile by itself.

`cdp-eval.mjs` uses the same client to evaluate one script in one page, for
probes such as `scripts/xfmt/probes/residual` and `scripts/xfmt/probes/center`:

```sh
node scripts/xfmt/cdp-eval.mjs page.html out.json expr.js --media print --width 600
```

## Reference documents

`tests/xfmt/xfmt.toml` lists them, with comments on each field:

- `r12/p1.md` to `r12/p7.md`: R12's probes (R12 §2, §6 to §8), with R12's
  theme and Source Sans 3 in `r12/theme`.
- `r12-vocab/p10.md`: the title block, the contents and tables. It uses a
  theme that extends R12's with the vocabulary styles. Its screen layout
  uses screen media, because print media hides the contents.
- `r13/residual.html`: R13 §3.2's plain-CSS probe, in Chrome only. Its
  screen column is R13's "Plain CSS" row.

Each document's table is kept in `tests/xfmt/expected/<name>.txt`. The
tables were recorded with writer at ead21e88 and Chrome 157.0.8103.79 on
macOS: 9 tables, 223 lines in all, of which 196 are rows. Of 171 rows
compared, 23 are outside tolerance. These are the differences M9 sets out
to remove, such as lists, quotes and code blocks (p7), the title block
container and contents (p10), and HTML's space before after a forced page
break (p5, p6). Another Chrome version or other fonts may move the screen
and print columns. Record the tables again (`--update`) and say so in the
commit.

To add a document, put it and its `writer.toml` under `tests/xfmt/`, add a
`[[doc]]` entry, run `--update` and check the new table by hand before
committing it.

## Files

| File | Role |
|---|---|
| `scripts/xfmt/run.py` | The runner: builds, measures, matches, prints the tables |
| `scripts/xfmt/word.py` | Word's model from the DOCX's XML |
| `scripts/xfmt/baselines.py` | PDF baselines through Ghostscript; usable on its own |
| `scripts/xfmt/metrics.py` | s and A of font files; usable on its own |
| `scripts/xfmt/cdp.mjs` | The DevTools client and Chrome's lifetime |
| `scripts/xfmt/cdp-print.mjs` | Screen lines and printed PDF for a list of pages |
| `scripts/xfmt/cdp-eval.mjs` | One script evaluated in one page |
| `scripts/xfmt/lines.js` | The baseline reader evaluated in the page |
| `scripts/xfmt/probes/` | R13's probes (§1.2, §3.2), kept as reference |
