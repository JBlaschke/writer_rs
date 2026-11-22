# R13: writer as the single source of truth for presentational defaults

**Status:** research note (2026-11-22). An audit of main at `5890724f`
(writer 0.3.1, `@writer/doc` 0.4.0, Typst 0.15.1). Read it together with
R12 (`docs/research/R12-spacing-model.md`: the spacing model and tasks
T96–T101), which is committed on the branch `claude/spacing-model`
(`25168b9f`, PR #89) and not yet merged into main; R13 builds on it and
starts where it stops. Nothing here is a contract: where
this note and the spec disagree, the spec wins. Word was not opened for
R13. Word figures that R13 computes from the OOXML are marked (u); Word
figures without the mark come from R12 §2 and §8 or from the 2026-11-04
record in docs/dev/docx-validation.md:168-205. Code is cited as path:line
at `5890724f`. design.md and the spacing-model files are cited by section,
because R12's changes move their lines once merged.

The owner's framing: writer's job is to be "a consistent repository of
defaults", with every output format following that one source. Without it,
a Word template plus a Typst template would do. This note measures how far
writer is from that and proposes how to get there.

R12 settles how a declared design is laid out. Word is the reference, and
the PDF and HTML follow Word's line box, the larger of two adjoining
spaces, a div as one block, and Word's page tops (design.md §0.3). R13 does
not restate that model. Three things remain, and they are this note:

1. **A built-in default design.** R12's model applies to a declared design
   only (design.md §0.3). A document without `[design]`, which is most
   documents, still gets three unrelated looks. R13 proposes a complete
   built-in design as the lowest layer. That changes the contract, so it is
   planned as a change after T101 (decision D13, §4).
2. **The elements R12 §8 leaves outside the model:** lists, block quotes,
   code, tables, figures and captions, footnotes, callouts, equations, and
   headings no style declares (§3.3).
3. **Everything that is not spacing:** fonts, colours, page geometry,
   furniture, links, images, language and LaTeX (§2, §4.3).

This note replaces the 2026-11-21 draft "R12: writer as the single source
of truth for presentational defaults", which collided with R12 in its
number, tasks, decisions and package version, and whose line model R12's
measurements overturned. §6.3 maps every draft task to R13.

## 0. Method and conventions

### 0.1 Probes

The draft's audit, kept:
- (a) The default theme with no `[design]`.
- (b) A design that sets fonts, the text keys and colours. Each domain
  probe used its own values: for example 10.5 pt × 1.4 in the text-page
  probe, and 11 pt × 1.35 with 10.205 pt spacing in the lists probe.
- (c) Named vocabulary styles, for example `Heading 1` at 20 pt #A96D4B
  with 18/6 pt, `Caption` 9 pt, and `Compact` 9 pt #003366.
- (d) A footer slot `"Page {page} of {pages}"`.
- (f) `justify = true` with the spacing keys unset.
- (g) Missing font families.
- (h) DOCX built where the design font's metrics cannot be read.
- (i) A3 paper.
- (j)/(k) A header slot without and with offsets.
- (n) Numbered headings.
- b2–b5: design styles Recipient 6/8 mm, Tight 1 mm, Pitch 2.0, Badge, a
  Header style, and an oversized header image.
- p1–p6 (the draft's critic): inline markup, divs, metadata, page break,
  sub-figures, `lang` de/ar, `page.columns`, Typst-source decorations, pHYs
  300 dpi, and div classes.

R13's own probes (kept outside the repository with the coordinator's
notes, with the audit's per-probe data; T102 commits them as reference
code):
- **residual:** Chrome screen layout of body 11 pt at 1.35, `Heading 1`
  16 pt bold at 1.2 with 12/6 pt, `Heading 2` 13 pt bold at 1.2 with
  10/4 pt, and 16 pt text on an exact 20 pt pitch, all Source Sans 3 with
  P = 10.205 pt; then the same page with a per-style shift (§3.2).
- **center:** writer 0.3.1, `[design.text] justify = false,
  paragraph_spacing = "6pt"`, computed styles read over DevTools (§1.2).
- **fn:** footnote pitch under 0.4.0, under R12's 0.5.0, and with a fix
  (§3.4, M12).
- **edge-sign:** the sign of Typst's length `bottom-edge`.
- **metrics.py:** hhea, typo and win metrics of Source Sans 3, Georgia and
  Calibri.

### 0.2 Measurement and tolerances

- **PDF.** Baselines read with Ghostscript `txtwrite -r7200` (0.01 pt)
  from writer's PDF, or Typst positions through `metadata` probes (R12
  §9). At `-r720` odd multiples of 0.75 pt fall on half units.
- **HTML.** Read from Chrome's screen layout through the DevTools protocol,
  with print media emulated (`Emulation.setEmulatedMedia`) and the viewport
  at the text width. Client rectangles come in fractional CSS px, as R12
  §2.1 reads them.
- **Printed HTML** (`Page.printToPDF`) snaps every baseline to a whole CSS
  px, 0.75 pt: a 17.6 pt pitch prints as alternating 17.25 and 18.0 pt
  gaps. The draft's 12.4 pt paragraph spacing was 12.65 pt read off that
  grid. Checks that need print (page tops, furniture) compare absolute
  positions within 0.5 pt, or means over at least eight gaps within
  0.1 pt. They never compare a single printed gap. (0.4 pt is not enough:
  an H1-to-body baseline printed at 231.75 pt against the PDF's
  231.34 pt.)
- **Word.** Values are read from the XML. Rendering comes from R12's
  measurements or the 2026-11-04 record; anything else is computed (u).

| Check | Tolerance |
|---|---|
| Word ↔ PDF, any gap or position | 0.3 pt (0.5 pt where Word is read on a coarser grid, R12 §6) |
| HTML screen layout ↔ PDF, per gap | 0.3 pt after adding the residual c of the two styles (§3.2) |
| Printed HTML ↔ PDF | absolute positions within 0.5 pt after adding the c of each page's first block (none with D18's shift), or means over ≥ 8 gaps within 0.1 pt |
| Word XML ↔ model | exact, to the twip |

### 0.3 Symbols

R13 uses R12's names (design.md §0.3, §7.4):
- **size:** the font size.
- **m:** a line-height multiple of the font size. **h:** an exact pitch.
- **s:** Word's single line in em, (winAscent + winDescent + max(0, hhea
  lineGap − (winAscent + winDescent − hhea ascender − |hhea descender|))) /
  unitsPerEm, as src/docx/lines.rs:143-191 computes it (the formula is at
  :185). Source Sans 3: (1222 + 202)/1000 = 1.424. Georgia:
  2327/2048 = 1.136. Calibri: 2500/2048 = 1.2207.
- **A:** the part of s above the baseline, in em (R12 §2.1): s less the
  hhea descent, as R12's lines.rs computes it. Calibri: (2500 − 550)/2048
  ≈ 0.952.
- **P:** paragraph spacing. **S:** a block's space.
- **c:** HTML's residual for a style: its baseline's offset in the line box
  in Chrome minus Word's, positive when HTML sits lower (§3.2).

### 0.4 Codes

- **Origin**, in brackets after a value:
  - [d] a design token;
  - [w] writer's default: a literal in writer's code, in the `writer-doc`
    package, or in assets/docx (styles.xml, numbering.xml, settings.xml; a
    reference.docx replaces them);
  - [e] an engine built-in;
  - [t] the theme's CSS or a user template;
  - [na] not applicable; [?] unknown.
- **Reach:** which formats an existing `[design]` key reaches at
  `5890724f`. H = HTML, P = PDF, W = Word. "none" means no key exists.
- **SS:** single-source difficulty. E easy, M moderate, H hard, I
  impractical.
- **†** A Word space taken as the larger of space after and space before.
  The draft inferred that rule; R12 §2.3 measured it.
- **‡** A row R12 changes for declared designs. Recomputed with R12's line
  box where R13 gives a value; otherwise T102 re-measures it.

### 0.5 Files and counts

| Short name | Path |
|---|---|
| theme.css, print.css, notebook.css | `assets/themes/default/styles/` |
| lib.typ, design.typ, callout.typ, cells.typ, numbering.typ | `assets/typst/writer-doc/` |
| styles.xml, numbering.xml, settings.xml | `assets/docx/` |
| html/design.rs, docx/design.rs, docx/lines.rs, typst/emit.rs | under `src/` |
| the worktree | `.claude/worktrees/spacing-model` (R12's work, branch `claude/spacing-model`, PR #89) |

Typst paths and lines are those of the crates writer compiles against,
typst-library and typst-layout 0.15.1 under
`~/.cargo/registry/src/index.crates.io-*/`, not the ~/Developer/typst
checkout, whose lines differ.

Of 256 audited properties (239 in the inventories, 17 added by the draft's
critic), after the corrections in this note:
- 33 agree in HTML, PDF and Word, 68 agree partly, 154 disagree, and 1 is
  unknown (4.15).
- Of the 239, 84 have no `[design]` key at all.
- By difficulty, the 239 split 149 easy, 79 moderate, 8 hard and 3
  impractical. Rows 0.16, 3.19 and 3.20 moved from hard to moderate.

These counts describe documents without `[design]`, which R12 leaves
byte-identical. For a declared design, R12 brings body text, divs and the
vocabulary styles into agreement (‡ rows).

## 1. The problem

### 1.1 What a reader sees

The same Markdown file with no `[design]`. HTML is printed, so its gaps
are on the 0.75 pt grid; Word is computed.

| Property (case a) | HTML | PDF | Word | LaTeX |
|---|---|---|---|---|
| Body font | Charter (theme stack) | Libertinus Serif | Calibri | Latin Modern |
| Body line pitch | 17.6 pt | 14.4 pt | 14.77 pt (u) | 13.6 pt |
| Extra space between paragraphs | 12.65 pt (1.15em; prints as 12.4 pt) | 6.05 pt | 6 pt (u)† | about 0.5 baselineskip |
| Justification / hyphenation | ragged / none | justified / on | ragged / off | justified / on, English patterns |
| Body baseline to H1 baseline | 66.0 pt | 29.8 pt | about 39.4 pt (u): 4.30 below the body baseline, 18 of space†, 17.14 above the H1 baseline | – |
| H1 size, weight, colour | 20.35 pt, 650 (renders bold), black | 15.4 pt, bold, black | 18 pt, regular, #0F4761 | 14.4 pt, bold |
| Title | 26.4 pt, heading font | 17 pt bold, body font | 26 pt Calibri Light | 17.28 pt |
| Margins, top / side | 20 / 18 mm | 25 mm | 25.4 mm | 25 mm |
| Text width | 493.2 pt | 453.5 pt | 451.3 pt | 453.5 pt |
| Code block | 9.35 pt Menlo on #f2f1ec, 1em padding | 8.8 pt DejaVu Sans Mono, no fill | 10 pt Consolas on #F6F8FA | \small, rules |
| Table | three rules 1.5/0.75 pt, header bold, row 28.6 pt | 1 pt grid, header regular, row 17.2 pt | three rules 1/0.5 pt, header bold, row about 18.4 pt (u) | booktabs |
| Links | #235c93, underlined | black, no underline | #0969DA, underlined | #000099 |
| Page number | 9 pt system-ui, from page 2 | 11 pt body font, every page | 9 pt Calibri #59636E, every page | every page |
| 400×200 px image, no DPI | 300 pt | 400 pt | 300 pt | 453.5 pt (line width) |
| Footnotes | endnotes, 9.9 pt | page foot, 9.35 pt | page foot, 9 pt | page foot, \footnotesize |
| Previous baseline to display-math baseline | 51.0 pt | 22.3 pt | 6 pt of space (u)† | – |
| Callout "note" colour | #326b9a | #2867a2 | #0969DA | blue!60!black |

The Word body-to-H1 figure uses R12's line box: Calibri 11 pt on
`w:line` 264 auto is 1.1 single lines, so A·size = 10.47 pt sits above
the baseline and 4.30 pt below; the 18 pt H1 has 17.14 pt above (Calibri's
A, (2500 − 550)/2048 ≈ 0.952, is computed, not measured).

A design that sets the text keys brings the three main formats together on
the values it names. In case (b) at 0.4.0, the pitch is 14.7 pt
everywhere, paragraph spacing is 6 pt, and colours and margins agree; under
R12's 0.5.0 the spaces between body text, divs and declared vocabulary
styles also agree, HTML within its residual. The following still differ in
case (b):
- the page background (whole page / page area only / nothing);
- dark mode on screen, which overrides the design;
- Word's hard-coded colours 59636E, 0F4761 and 1F2328;
- the PDF title font, which ignores `fonts.heading`;
- page numbers;
- monospace sizes, 9.45 / 8.4 / 10 pt;
- hyphenation, link underlines and widow control;
- every element outside R12's model (R12 §8).

### 1.2 Why: five causes and a bug

1. **No complete default design exists.** With no `[design]`, each format
   takes its values from a different place:
   - HTML: theme.css, print.css and notebook.css.
   - PDF: lib.typ literals and Typst built-ins.
   - Word: styles.xml and numbering.xml.
   - LaTeX: writer-template.tex.

   src/design/template.toml:6-30 documents this as the contract. For
   example, the body font is theme / Libertinus Serif / Calibri; the line
   height is theme / leading 0.65em / `w:line` 264; justify is off / true
   / off; margins are theme / 2.5cm / 1in. Paragraph spacing is theme /
   1.2em / "0" there, but :14's "0" is docDefaults (styles.xml:2); body
   paragraphs use Body Text's 6/6 pt (styles.xml:4).
2. **Each emitter reaches a different set of elements.**
   - Named styles reach PDF headings, captions and contents through
     design.md §8.7. At 0.4.0 they reach HTML divs only; R12's T99 adds the
     HTML vocabulary rules for declared styles.
   - `Compact` reaches PDF table cells and Word lists and cells.
   - `colors.muted` reaches the HTML token and the footer furniture text in
     all three formats (src/design/resolve.rs:391-395; Word through
     src/docx/furniture.rs:301-303). `colors.accent` reaches the HTML
     token and the furniture rules in all three (resolve.rs:602; Word
     through furniture.rs:372-375). Either can be named as a colour value in
     a style or a furniture line (src/design/parse.rs:58-73). No key reaches
     styles.xml's hard-coded colours (59636E, 0F4761, 1F2328 and the rest).
   - `fonts.heading` misses the PDF title (lib.typ:52-85).
3. **One value means different things per engine.**
   - Line height: each backend hangs the line differently. Typst runs from
     cap height to baseline and adds leading between lines; CSS centres the
     font in the line box (half-leading); Word hangs it from the ascent
     (R12 §2.2). R12's 0.5.0 makes the PDF follow Word for a declared
     design; HTML keeps a residual (§3.2).
   - Adjoining spaces: CSS and Word keep the larger (R12 §2.3); Typst lets
     the stronger replace the other, whatever its size. R12's 0.5.0 fixes
     this for a declared design.
   - Image size with no DPI: 96 dpi in HTML and Word, 72 dpi in Typst.
4. **Presentational literals are spread over about ten files.** theme.css,
   print.css, notebook.css, lib.typ, design.typ, callout.typ, numbering.typ,
   styles.xml, numbering.xml, settings.xml, src/docx/body.rs (code line
   numbers and colours) and writer-template.tex. (cells.typ holds none.)
5. **LaTeX ignores `[design]`.** b/out.tex is byte-identical to a/out.tex.

**The paragraph-rule bug.** When a declared design sets
`text.paragraph_spacing` or `text.justify`, `layout_css` writes
`[data-writer-document] p{…}` with the margin `margin-block:0 P`, the
alignment, or both (src/html/design.rs:143-158), at specificity 0,1,1 and
after the theme. Once the built-in design sets both keys (T108), the rule
reaches every document. Probe *center* (computed styles in Chrome) confirms
what it does:
- The subtitle, date and author paragraphs go to the start edge. They are
  centred only by inheritance from `.title-block` (theme.css:112). In case
  (b) the subtitle sits at x 17.99 mm against the PDF's 88.55 mm.
- Paragraphs in `::: {.center}` and `.right` divs (theme.css:259-261) lose
  their alignment the same way.
- It replaces the margins of `.subtitle` (:120), `.date` (its 1em top
  margin, :134) and `.toc-title` (:276).
- It restores the last paragraph's margin in block quotes, abstracts,
  table cells and callouts (theme.css:109, :143, :166, :250); the probe
  measured 6 pt under the last quoted paragraph.

The abstract title and the contents title are not centred in the default theme
(`.abstract` is left-aligned, theme.css:135), so for them only the margins
change. `:where()` alone does not fix the alignment: any rule that matches the
`p`, even at specificity 0, beats an inherited value. The fix (T104) writes
`text-align` on `[data-writer-document]`, where it is inherited, and the
paragraph margin under `:where()`. R12's T99 does not fix it. Its vocabulary
rules give the title-block parts `Title`'s `align` when `Title` sets one and
the part's own style does not, and its `[data-style=X]>:last-child` rules fix
divs, but the `.center` divs, `.date`, `.toc-title`, quotes, cells, callouts
and abstracts stay as listed.

### 1.3 What R12 settles

R12 measured Word for Mac 16 and built the model into all three backends
for a declared design (design.md §0.3). R13 relies on these results and
does not restate them:
- **The larger space holds** between two blocks, never the sum (R12 §2.3;
  for example 37.60 pt in Word where the larger gives 37.53 pt and the sum
  54.54 pt).
- **The line box:** Word hangs a line from the ascent; an exact pitch puts
  0.8h above the baseline (R12 §2.2).
- **Page tops:** only the document's first block keeps its space before
  (R12 §2.4).
- **A div is one block** (R12 §2.5, decision D1).
- **HTML vocabulary rules** sit after the theme without adding specificity
  (decision D2).
- **Agreement:** Word and the PDF within 0.3 pt; HTML a constant per style
  off (R12 §6; design.md §0.3 "Agreement").

Its scope is body paragraphs, divs and the §8.7 vocabulary styles, for a
declared design (design.md §0.3 "Scope"). R12 §8 lists what lies outside.

The draft listed four documents that said Word adds the two spaces. Their
state:

| Passage | State |
|---|---|
| design.md §8.7 (main :1057-1058) | corrected in the worktree, with a dated history note |
| R11 §4 (main :156-157) | annotated in the worktree with a dated correction; the old sentence kept |
| docs/guide/configuration.md:547 ("Word adds them") and :564 ("Word drops them") | planned: R12's T101, Edits 5 and 7 |
| docs/tasks/T95-vocabulary-styles-guide.md:82 | not planned; task files are historical, so a dated note at most (coordinator) |
| docs/tasks/T90-furniture-image-gap.md:88 ("Word adds the extra multiple line spacing above a line"), missed by the draft | stale under R12 §2.2; a dated note at most (coordinator) |
| design.md §9.1 in the worktree ("the line spacing Word adds above the text"), missed by the draft | reads as historical; the coordinator rewords it with the §0.4 edit (§4.1) |

### 1.4 The declared-design gate

R12 keeps design.md §0.2's "Absent means unchanged" and "Unset means the
backend's default", and §0.3 says the model applies to a declared design
only. Its acceptance criteria prove absence with unchanged snapshots. All
of this hangs on one flag, `Design.declared = !table.is_empty()`
(src/design/resolve.rs:20), which these places read:

| Place | What it gates |
|---|---|
| src/design/mod.rs:194 | the field |
| src/design/resolve.rs:20 | sets it |
| src/design/json.rs:73 | resolved JSON, `null` when undeclared |
| src/assets/export.rs:137 | names for the design JSON and fonts in an export |
| src/check/design.rs:43 | Typst diagnostics for a declared design |
| src/check/design.rs:95 | `docx_shadowed`: reference.docx styles that hide declared ones |
| src/check/targets.rs:42 | the PDF target's design check |
| src/typst/world.rs:621 | inserting `writer-design` into the Typst inputs |
| src/html/design.rs:18, :113, :138, :406, :539, :661 | tokens, layout, styles, furniture and the rest of the generated CSS |
| src/html/styles.rs:139 | the named-style rules |
| src/docx/design.rs:315 | the generated styles |
| src/docx/furniture.rs:46 | header and footer parts |
| src/docx/lines.rs:46 | reading font files for `w:line` |
| src/docx/reference.rs:366, :370 | writer's own section properties |
| src/docx/styles.rs:657 | the rewritten styles part |

In the package, design.typ's `declared: design.len() > 0` and the
paragraph layout's no-design branch (worktree lib.typ) keep Typst's
0.65em/1.2em when writer passes no design.

A built-in layer 0, added in `with_design_layers`
(src/pipeline/mod.rs:458-503; `resolve_design` at :506-510 passes it
`crate::design::merge`), makes the table non-empty for every document. Every
gate then flips at once:
- `writer-design` is always inserted, so the package's no-design branch is
  never taken by writer builds.
- HTML always emits tokens, layout, the vocabulary rules, the empty-cell
  rule and the paragraph-rule bug.
- DOCX always rewrites styles and reads font files (lines.rs:46), so the
  output depends on the fonts installed on the build machine.
- `writer check` lints fire on values the user never wrote.
- design.md's introduction (its last sentence), §0.2, §0.3's scope
  sentence, §5.1, §6.1, §6.2 ("byte-identical to 0.4.0's"), §6.7, §7.1,
  §8.7 and §9.3 become false, and so do R12's proofs: T98 criterion 12
  (no snapshot changes), T99 criterion 8 (three snapshots change by added
  lines only) and T100 criterion 6 (no other snapshot changes). T98
  criterion 7 keeps passing, but it then tests a path writer never takes.

So the built-in design is a contract change, not a layer added beside R12.
§4.1 splits the flag and stages the change per domain; D13 asks the owner
to accept that documents without `[design]` change look in all three
formats. R12 §6 shows why that is the point: an undeclared Heading 3 sits
29.0 / 30.9 / 42.7 pt below the body baseline in Word / PDF / HTML.

**What users do today.** The EIT templates compensate per format. R12's
0.5.0, not R13, invalidates most of it, as soon as a template imports 0.5.0
with its declared design:
- **Letter, `spaced-doc`** (pdf-template.typ:1-14 and :19 onwards)
  re-implements HTML spacing in Typst. Under 0.5.0 it is double
  compensation.
- **Letter, letter.css:48-53**, `p:has(+ h1) { margin-bottom: 0 }`, makes
  HTML deviate from the larger-of rule that Word and the PDF now follow.
  Remove it with the move to 0.5.0.
- **Letter, theme.toml:40-44 and :70-75** note that HTML sits about 1 mm
  and Word about 1.3 mm lower than the PDF. Under 0.5.0 the PDF agrees
  with Word, and what is left is HTML's residual c (§3.2).
- **Letter, letter.css's `[data-writer-document] h3{}`** (0,1,1) still
  beats D2's rules; check it.
- **Report, document.css.** Its bare h1–h6 rules (0,0,1) lose to R12's
  vocabulary rules (D2) once the report declares those styles. Its cover
  maths (:36-52) converts Typst baseline-to-cap gaps and must be redone
  with R12's box. Its contents, caption, list and table rules (:154-219,
  :268) stay until R13's domain tasks land.
- **Report, pdf-template.typ:54-98:** heading 4–6 spacing and justification
  off (T108, T109), contents leaders and case (T109), a table grid with
  column stretching (T114), list indents (T112).
- **Report, `toc 1`** (theme.toml:209-221) relies on Word's larger-of rule
  and stays correct.

The EIT file references are the draft's, verified by its critic, not
re-read for R13.

## 2. Inventory

Values are case (a) unless stated. Rows marked C were added by the draft's
critic. The full per-probe data, 1482 lines, is kept outside the
repository with R13's probes (§0.1); these tables keep the value that
decides each row. The draft's tags
and citations are corrected here: every assets/docx value is [w] (§0.4),
and line numbers are main's at `5890724f`.

### 2.1 Body text, colour and page

| # | Property | HTML | PDF | Word | LaTeX | Agree | Reach | SS |
|---|---|---|---|---|---|---|---|---|
| 0.0 | Body font | Charter stack [t] theme.css:3 | Libertinus Serif [w] lib.typ:529 (applied at :451-454) | Calibri [w] styles.xml:2 | Latin Modern | no | fonts.body: HPW | M |
| 0.1 | Heading font | system-ui stack, 650 [t] theme.css:4 | body font [w] lib.typ:459-462 (heading-font none by default, :531-533) | Calibri Light for H1–2 and Title, Calibri for H3–6 [w] styles.xml:7, :14-19 | body font | no | fonts.heading: HPW, not the PDF title | M |
| 0.2 | Mono font and size | Menlo stack, .9em = 9.9 pt [t] theme.css:5 | DejaVu Sans Mono [w] lib.typ:535 (applied at :457), 0.8em = 8.8 pt [e] typst-library text/raw.rs:633 | Consolas 10 pt [w] styles.xml:45, :49 | LM Mono | no | fonts.mono: HPW, family only | M |
| 0.3 | Body size | 11 pt print, 12 pt screen [t] print.css:4 | 11 pt [w] lib.typ:511-513 | 11 pt [w] | 11 pt | partly | text.size: HPW | E |
| 0.4 | Line pitch | 1.6 = 17.6 pt [t] theme.css:7 | leading 0.65em: 14.4 pt [e] | `w:line` 264 auto: 14.77 pt (u) [w] | 13.6 pt | no | text.line_height: HPW | M |
| 0.5 | Extra space between paragraphs | 1.15em = 12.65 pt [t] theme.css:96 | 1.2em between line boxes: 6.05 pt [e] | Body Text 6/6 pt: 6 pt (u)† [w] styles.xml:4 | parskip | no | text.paragraph_spacing: HPW | M |
| 0.6 | First paragraph | like the others | like the others | First Paragraph style, same spacing | – | yes | – | E |
| 0.7 | First-line indent | 0 | 0 | 0 | 0 | yes | none | E |
| 0.8 | Justification | ragged [e] | justified [w] lib.typ:543-545 | left [e] | justified | no | text.justify: HPW | E |
| 0.9 | Hyphenation | none (`hyphens: manual`) [e] | on when justified, patterns from `lang` [e] | off [e] | on, English patterns always | no | none | M |
| 0.10 | Kerning, ligatures | on [e] | on [e] | off: no `w:kern`, no `w14:ligatures` [e] | on | partly | none | E |
| 0.11 | Text colour | #23262b screen, #000 print [t] | black [w] | auto (black) [e] | black | partly | colors.text: HPW (Word rPrDefault only) | E |
| 0.12 | Muted, accent and heading colours | muted #5c626d, accent #366b76 [t] theme.css:11-12 | black; muted and accent reach furniture only [w] | hard-coded 59636E, 0F4761, 1F2328 [w] | – | no | colors.muted: H; footer furniture H, P, W. colors.accent: H; furniture rules H, P, W | M |
| 0.13 | Link colour | #235c93 [t] | black [e] | #0969DA [w] styles.xml:52 | #000099 | no | colors.link: HPW | E |
| 0.14 | Link underline; URL printed after the link | underline; print appends the URL [t] print.css:7 | neither [e] | underline [w] | neither | no | none | E |
| 0.15 | Emphasis, strong | italic; 700 [e] | italic; +300 weight [e] | `w:i`, `w:b` [e] | – | yes | none | E |
| 0.16 | Page background | #fffefb screen, white print; (b) fills the page area only [t][d] | none; (b) fills the whole page [e][d] | not applied [na]; the T91 behind-text anchor (docx/furniture.rs:503-645) can carry a full-page colour | – | partly | colors.background: HP | M |
| 0.17 | Dark mode | theme palette, which beats a design on screen [t] theme.css:27-46 | none [na] | none [na] | – | no | none | I |
| 0.18 | Paper | A4 [t] print.css:1; `[pdf] paper` not applied | A4 [w] | A4; only a5, us-letter, us-legal known (A3 gives A4) [w] | a4paper always | yes (a) | page.paper: HPW | E |
| 0.19 | Margins | 20 mm top and bottom, 18 mm sides [t] print.css:1 | 25 mm [w] lib.typ:514-522 | 1in = 25.4 mm [w] docx/design.rs:438-441 | 2.5cm | no | page.margin: HPW | E |
| 0.20 | Header offset (slot, unset) | baseline 4.23 mm [w] | 30 % of the top margin: baseline 10.05 mm [w][e] lib.typ:376-378 | 12.7 mm to the header top [w] docx/design.rs:442 | – | no | header_offset: HP with a slot; W always | M |
| 0.21 | Footer offset, built-in number | baseline 8.6 mm above the edge [t] | 14.95 mm [e] | footer 12.7 mm [w] docx/design.rs:443; baseline (u) | – | no | footer_offset | M |
| 0.22 | Page number present | print only, not page 1 [t] print.css:1-2 | every page [w] | every page [w] | every page | partly | slots replace it | E |
| 0.23 | Page number position | centred | centred | centred | centred | partly | – | M |
| 0.24 | Page number font | 9 pt system-ui, black [t] | 11 pt body font, black [w][e] lib.typ:448 | 9 pt Calibri #59636E [w] styles.xml:46 | body font | no | slot text | E |
| 0.25 | Widows, orphans | 2/2 screen, 3/3 print [e][t] print.css:7 | prevented [e] | no `w:widowControl`; Word's effective default is likely on, about 2 lines (u; T103 measures it) | penalties 150 | no | none | E |
| 0.26 | Keep rules | print: h1–h3 `break-after: avoid`; figures, tables, code, callouts and li not broken [t] | headings sticky [e] | headings keepNext + keepLines [w] | headings | partly | keep_*: HW, P partly | M |
| C | Text width | screen 504 pt, print 493.2 pt [t] theme.css:8 | 453.5 pt [w] | 451.3 pt [w] | 453.5 pt | no | via margins | M |

Row 0.24: lib.typ:447-450 turns on Typst's built-in footer number with
`numbering: "1"`; it is set in the page's text style (size from
lib.typ:511-513, fill from :547-549). lib.typ:382-393 is the declared
footer's furniture and runs only when `page.footer` is set.

### 2.2 Headings, title block and contents

| # | Property | HTML | PDF | Word | LaTeX | Agree | Reach | SS |
|---|---|---|---|---|---|---|---|---|
| 1.0 | Sizes H1–H6 | 1.85/1.45/1.2/1/1/1em = 20.35/15.95/13.2/11/11/11 pt [t] theme.css:77-80 | 1.4/1.2/1em = 15.4/13.2/11/11/11/11 pt [e] | 18/15/13/12/11/11 pt [w] styles.xml:14-19 | 14.4/12/11 pt | no | `Heading N`.size: PW; HTML after R12 T99 | E |
| 1.1 | Weight | 650 at every level (bold face) [t] | bold [e] | H1–2 regular, H3–6 bold [w] | bold | partly | weight: PW | E |
| 1.2 | Family | system-ui [t] | body font [w] | Calibri Light / Calibri [w] | body font | no | fonts.heading: HPW | E |
| 1.3 | Colour | text [t] | black [e] | #0F4761 [w] | black | no | colors.text: HP; style colour: PW | E |
| 1.4 | Line height | 1.25 of the heading size [t] | leading 0.65em [w] | inherits 264 auto (u) [w] | per size | no | line_height: PW | M |
| 1.5 | Space above (body to H1 baseline) | 2em of the heading = 40.7 pt; 66.0 pt (H3 44.3 pt) [t] theme.css:69-76 | 1.8em / 1.44em scaled; 29.8 pt (H3 22.9 pt) [e] | H1 18 pt before against Body Text 6 pt after: 18 pt of space†, about 39.4 pt (u); H2 14, H3 12, H4–6 10 pt [w] | 3.5ex | no | space_before: PW | M |
| 1.6 | Space below (H1 to body baseline) | 0.65em = 13.23 pt; 30.8 pt [t] | 8.25 pt; 15.4 pt [e] | 6 pt after against 6 pt before: 6 pt of space† | 2.3ex | no | space_after: PW | M |
| 1.7 | Keep with next | print, h1–h3 only [t] | every level [e] | every level [w] | built in | partly | keep_with_next: PW | E |
| 1.8 | Number separator (case n) | about 0.25em space [w] | weak 0.3em space, hanging [e] numbering.typ | tab to the 36 pt stop [w] | quad | partly | none | E |
| 1.9 | Number colour | muted [t] theme.css:82-85 | heading colour [e] | heading colour [w] | – | no | colors.muted: H | E |
| 1.10 | Letter case | none | none | none | none | yes | – | E |
| 1.11 | Unlisted heading | same look, left out of nav | `outlined: false` | `Heading N Unlisted`, outlineLvl 9 | `\writernotoc` | yes | styles: PW | E |
| 1.12 | Unnumbered heading | yes | yes | yes | starred | yes | – | E |
| 1.13 | Title size, weight, family | 2.4em = 26.4 pt, 650, heading font, −0.025em [t] theme.css:119 | 17 pt bold, body font [w] lib.typ:52-85 | 26 pt Calibri Light, regular [w] styles.xml:7 | 17.28 pt regular | no | Title: PW; fonts.heading: HW | E |
| 1.14 | Title colour | text [t] | text [e] | #1F2328 [w] | – | partly | colors.text: HP | E |
| 1.15 | Title alignment | centred [t] | centred [w] | centred [w] | centred | partly; in (c) HTML stays centred | align: PW; H after R12 T99 | E |
| 1.16 | Subtitle | 1.2em, muted, body font, centred [t] | 11 pt, text colour, centred [w] | 15 pt heading font #59636E [w] | 12 pt | no | Subtitle: PW | E |
| 1.17 | Authors | flex row, name bold 11 pt, affiliation 9.9 pt muted [t] | comma list 11 pt, affiliation 9 pt [w] | one 12 pt paragraph per author [w] | 12 pt, side by side | no | Author: PW | M |
| 1.18 | Date | 11 pt muted, 1em above [t] | 11 pt text [w] | 11 pt #59636E, 12 pt after [w] | 12 pt | partly | Date: PW | E |
| 1.19 | Abstract title | 0.85em bold, uppercase, +0.08em tracking, left [t] | level-1 heading 15.4 pt in a 2em inset [w] lib.typ:154-164 | 10 pt bold, centred, 12/3 pt [w] | small bold, centred | no | P via Heading 1; W: Abstract Title | E |
| 1.20 | Abstract body | 11 pt, max-width 36rem [t] | 11 pt, 2em inset [w] | 10 pt, 36 pt indent [w] | small | no | Abstract: W | E |
| 1.21 | Title-block gaps | title to subtitle 32.3 pt; abstract title to body 27.7 pt [t] | 27.7 pt; 15.5 pt [w] | Title 12/6, Subtitle 0/6, Date 0/12, Abstract Title 12/3: 6 pt and 3 pt of space† | – | no | styles: PW | M |
| 1.22 | Title-block close | 0.75 pt rule, 2rem + 2.5rem = 49.5 pt [t] | `v(1.5em)` = 16.5 pt, no rule [w] | Date 12 pt after, no rule [w] | maketitle | no | none | E |
| 1.23 | Contents title | 0.9em bold, heading font [t] | level-1 heading, 15.4 pt [e] | TOC Heading 18 pt #0F4761 [w] | 14.4 pt | no | TOC Heading: PW | E |
| 1.24 | Entry size | 0.9em, heading font [t] | 11 pt body font [e] | 11 pt body font [w] | 11 pt | partly | `toc N`: PW | E |
| 1.25 | Entry pitch | about 18.3 pt [t] | 14.4 pt [e] | 0/3 pt: about 17.8 pt (u) [w] | – | no | `toc N` spacing: PW | M |
| 1.26 | Indent per level | 12.4 pt [t] | 13.2 pt [e] | 12 pt [w] | 1.5em, 3.8em | partly | `toc N` indent: PW | E |
| 1.27 | Leaders, page numbers | neither [t] | dots and numbers [e] | dot tab and PAGEREF [w] docx/toc.rs:51, :78 | numbers | partly | none | E in PDF and Word; HTML page numbers are exception 10 (D10 for printed HTML) |
| 1.28 | Entry colour | link colour [t] | text [e] | text [w] | – | no | colors.link: H | E |
| 1.29 | Contents in print | hidden [t] print.css:8 | shown [w] | shown [w] | shown | no | none | E |
| 1.40 | Vocabulary styles ‡ | 0.4.0: divs only, `[data-style]` rules for h1, .title, figcaption match nothing; R12 T99: rules for declared styles on h1–h6, the title block, the contents, figcaption and td (design.md §8.8) [d] | §8.7 vocabulary [d] design.typ | styles rewritten [d] | ignored | 0.4.0 no; 0.5.0 yes for declared styles | PW; H after R12 T99 | M |
| 1.41 | Title block under the paragraph keys | the p rule beats .subtitle and the rest: subtitle and date go left (bug, §1.2) [d] | unaffected | unaffected | – | no | – | E |
| 1.42 | fonts.heading on the title | yes | no | yes | no | no | HW | E |

The 0.4.0 values of 1.5, 1.6 and 1.21 hold for documents without
`[design]`. With a declared body but an undeclared heading, R12 §6
measures Heading 3 at 29.0 / 30.9 / 42.7 pt (Word / PDF / HTML).

### 2.3 Captions, figures and images

| # | Property | HTML | PDF | Word | LaTeX | Agree | Reach | SS |
|---|---|---|---|---|---|---|---|---|
| 1.30 | Caption size | 0.9em = 9.9 pt [t] theme.css:171-176 | 11 pt [e] | 10 pt [w] styles.xml:33 | 11 pt | no | Caption: PW; H after R12 T99 | E |
| 1.31 | Caption colour | muted [t] | text [e] | #59636E [w] | – | no | colors.muted: H; Caption.color: PW | E |
| 1.32 | Italic; label weight | upright; bold label [t] | upright; regular label [e] | italic; regular label [w] | upright | no | Caption.italic: PW | E |
| 1.33 | Alignment | images centred; tables and listings left [t] | all centred [e] | Image Caption centred, others left [w] | centred | partly | align: PW | E |
| 1.34 | Position | tables and listings above, images below | same | same | same | yes | – | E |
| 1.35 | Gaps | body to caption 5.94 pt; caption to next baseline 36.0 pt [t] | 7.15 pt; 20.4 pt [e] | image to caption 3 pt, caption to body 6 pt† [w] | – | partly | Caption spacing: W only (§8.7 leaves it out of the PDF) | M |
| 1.36 | Label text | "Figure 1: " | same | same | always English "Figure 1:" (no babel) | yes (en) | – | E |
| 1.37 | Figure block spacing | 1.75em = 19.25 pt [t] | block 1.2em = 13.2 pt [e] | CaptionedFigure 6/3 pt: 6 pt† [w] | – | no | Figure: W | M |
| 1.38 | Natural size, 400×200 px, no DPI | 300 pt (96 px/in) [e] | 400 pt (72 dpi) [e] | 300 pt [w] src/docx/images.rs:336-361 | 453.5 pt | no | none | E |
| C | Natural size, 600×300 px, pHYs 300 dpi | 450 pt (Chrome ignores pHYs) [e] | 144 pt [e] | 144 pt [w] images.rs:371-396 | – | no | none | E |
| C | Inline image, 20×20 px | 15 pt [e] | 20 pt [e] typst/emit.rs:1156-1160 | 15 pt [w] | – | no | none | E |
| 1.39 | Maximum width | 100 % of the column, centred [t] theme.css:64-67 | shrink to fit, centred [e] | clamped to 451 pt, centred [w] | always the line width | partly | none | E |
| C | Sub-figure numbering | flat: panels Figure 3 and 4 | same | same | panels unnumbered; the reference prints "Figure 2" | yes (LaTeX no) | – | E |

### 2.4 Lists, block quotes and rules

Gaps are baseline to baseline. R12 §8 measured the same elements with a
declared body design (Libertinus 11 pt at 1.35, P = 10.205 pt); its values
follow the table.

| # | Property | HTML | PDF | Word | LaTeX | Agree | Reach | SS |
|---|---|---|---|---|---|---|---|---|
| 2.0 | Pitch inside items | 17.6 pt [t] | 14.35 pt [w] | 14.77 pt (u) [w] | 13.6 pt | partly | line_height: HPW | E |
| 2.1 | Extra space between tight items | `li + li` .25em = 2.75 pt [t] theme.css:98 | 0 [e] | Compact 1.8/1.8 pt: 1.8 pt† [w] styles.xml:6 | 0 | no | Compact: W | M |
| 2.2 | Paragraph to list | 30.0 pt [t] | 20.5 pt [e] | tight: 20.77 pt (u)† | 20.3 pt | partly | paragraph_spacing: H via p, P, W partly | M |
| 2.3 | List to paragraph | `ul` margin 1.15em: 30.0 pt; (b) 27.5 pt [t] | 20.5 pt; (b) 25.0 pt [e] | (a) 20.77 pt (u)†; (b) 16.60 pt (R12 §8: Compact's 1.8 pt after, Body Text's 0 before) | – | no | P only | M |
| 2.4 | Loose items | 30.0 pt [t] | 20.4 pt [e] | 20.77 pt (a) (u)†; 25.20 pt (b, R12 §8) | 20.3 pt | partly | paragraph_spacing: HPW | E |
| 2.5 | Paragraphs in one item | 30.0 pt, continuation at the text start | 20.5 pt, same | 20.77 pt (u)†, `w:ind` 720 | – | partly | HPW | E |
| 2.6 | Start of a nested list | 0 extra [t] | paragraph spacing, from the emitter's blank line: (b) 25.05 pt [w] typst/emit.rs:629-648 | 1.8 pt† | 0 | no | none | E |
| 2.7 | End of a nested list | .35em = 3.85 pt [t] theme.css:97 | 0 [e] | 1.8 pt† | – | no | none | E |
| 2.8 | Text indent per level | 30 pt (40 px UA padding) [e] | marker + 0.5em: 9.3 pt [e] | 36 pt [w] numbering.xml | 27.2 pt | no | none | M |
| 2.9 | Marker to text gap | about 8.5 pt, placed by Chrome [e] | 0.5em = 5.5 pt [e] | hanging 18 pt plus a tab [w] | – | no | none | M |
| 2.10 | Bullets by depth | • ◦ ▪ ▪ [e] | • ‣ – • [e] | • o ▪ [w] | • – ∗ · | partly | none | E |
| 2.11 | Number format | 1. at every depth | same | same | 1. (a) i. A. | yes | none | E |
| 2.12 | Number alignment | right [e] | right [e] | left at the hanging position [w] | – | partly | none | M |
| 2.13 | Start number | yes | yes | yes | yes | yes | – | E |
| 2.14 | Marker font, colour | shapes in the item colour [e] | body font [e] | Symbol / Courier New / Wingdings [w] | – | partly | fonts.body: HP | M |
| 2.15 | Task marker | disabled checkbox, no bullet [w][e] | bullet plus ☐/☒ from a system fallback font [w][e] | ☐/☒ in Segoe UI Symbol, no bullet [w] | $\square$ | no | none | M |
| 2.16 | Task text start | 30 pt | about 17.6 pt | 36 pt | – | no | none | M |
| 2.17 | Definition term | bold | bold | bold, keepNext [w] | bold | partly | Definition Term: W | E |
| 2.18 | Description layout | below, 1.5em indent [t] | inline after the term, 2em hanging [e] | below, 18 pt indent [w] | inline | no | Definition: W | M |
| 2.19 | Definition spacing (b): paragraph to term; item to item | 25.5 pt; 26.3 pt [t] | 25.1 pt; 25.1 pt [e] | 25.08 pt; 20.88 pt (u)† | – | no | paragraph_spacing: HP | M |
| 2.20 | Quote left inset | border 1.5 pt + 1.25em = 15.25 pt [t] theme.css:102-107 | 1em = 11 pt [e] | 24 pt [w] styles.xml:28 | 27.2 pt | no | none | E |
| 2.21 | Quote right inset | 0 | 11 pt | 24 pt | 2.5em | no | none | E |
| 2.22 | Quote border | 1.5 pt #c8c9c3 [t] | none [e] | 2.25 pt #D0D7DE, 8 pt from the text [w] | none | no | none | M |
| 2.23 | Quote colour | muted [t] | text [e] | #59636E [w] | – | no | colors.muted: H; Block Text: W | E |
| 2.24 | Quote size, style | body, upright | same | same | – | yes | Block Text: W | E |
| 2.25 | Paragraph to quote / quote to paragraph | 33.0 pt before [t] | 2.4em above, 1.8em below at weakness 3: 33.6 / 27.0 pt [e] | Block Text 5/5 pt: 20.77 pt both in (a) (u)†; (b) 25.20 / 19.80 pt (R12 §8) | 20.4 pt | no | Block Text: W | M |
| 2.26 | Inside the quote; last child | 30.7 pt; last child 0 in (a), but (b)'s p rule restores 10.2 pt (bug, §1.2) | 20.5 pt | 5 pt: 19.77 pt (u)† | – | partly | – | E |
| 2.27 | Nested quote | +15.25 pt, second border | +11 pt | invisible: same style | +2.5em | no | none | M |
| 2.28 | Rule thickness | 0.75 pt | 1 pt | 0.75 pt | 0.4 pt | no | none | E |
| 2.29 | Rule colour | #c8c9c3 | black | #D0D7DE | black | no | none | E |
| 2.30 | Rule length | 100 % | 100 % | 100 % | 50 %, centred | yes | – | E |
| 2.31 | Paragraph, rule, paragraph | 2em margins: 62.2 pt [t] | paragraph spacing: 33.6 pt [e] | empty bordered paragraph: about 43.3 pt (u)†, the border's 1 pt space and 0.75 pt width included | 40.7 pt | no | P; Horizontal Rule: W | M |
| 2.32 | Keep an item together | print: avoid [t] | breakable | widow control only | – | no | none | M |
| 2.33 | Compact / Block Text / Definition / Horizontal Rule styles ‡ | rules that match nothing (R12 T99 adds none of these) | not applied; R12 0.5.0 applies Compact to cells only | applied | – | no | W only | M |

R12 §8, Word / PDF / HTML in pt, declared body design: paragraph → tight
item 25.00 / 25.06 / 25.04; tight item → item 16.80 / 14.85 / 17.59;
tight list → paragraph 16.60 / 25.05 / 27.49; loose item → item
25.20 / 25.05 / 25.04; paragraph → quote 25.20 / 41.25 / 27.79; quote →
paragraph 19.80 / 34.65 / 40.43.

### 2.5 Code, math, callouts and notebook cells

| # | Property | HTML | PDF | Word | LaTeX | Agree | Reach | SS |
|---|---|---|---|---|---|---|---|---|
| 3.0 | Inline code family | Menlo stack [t] | DejaVu Sans Mono [w] | Consolas [w] | LM Mono | no | fonts.mono: HPW | E |
| 3.1 | Inline code size | .9em = 9.9 pt [t] | 0.8em = 8.8 pt [e] | 10 pt [w] | – | no | text.size scales all three | E |
| 3.2 | Inline code background | #f2f1ec [t] | none | none | none | no | none | E |
| 3.3 | Inline padding, radius | .1em .3em, .2em [t] | 0 | 0 [na] | – | no | none | M |
| 3.4 | Inline code colour | text | black | auto | – | partly | colors.text | E |
| 3.6 | Code block size | .85em = 9.35 pt [t] | 0.8em = 8.8 pt; listings 7.92 pt [e][w] lib.typ:634 | 10 pt [w] | \small | no | text.size | E |
| 3.7 | Code pitch | 1.5 = 14.03 pt [t] | 12.39 pt; (b) 9.2 pt [e] | Normal's 264 auto on Consolas: about 12.9 pt (u) | – | no | line_height: PW, scaled wrongly in the PDF | H |
| 3.8 | Code background | #ffffff screen (InspiredGitHub), #f2f1ec print [t] | none; listings #f7f7f7 [w] lib.typ:631 | #F6F8FA [w] | frame rules | no | none | E |
| 3.9 | Code colour | #323232 screen, text in print | black | auto | – | partly | colors.text, not HTML screen | E |
| 3.10 | Code padding | 1em = 9.35 pt [t] | 0; listings 0.6em, about 5.3 pt [w] | 0 [w] | – | no | none | M |
| 3.11 | Border, radius | radius .25rem | square | square | rules | partly | none | M |
| 3.12 | Body to code; code to body | 37.2 pt; 36.0 pt [t] | 19.9 pt; 20.4 pt [e] | Source Code 6/6 pt: 6 pt of space both ways† [w] | – | no | P only | M |
| 3.13 | Long lines | scroll on screen, wrap in print | wrap | wrap | – | partly | none | E |
| 3.14 | Syntax colours | InspiredGitHub screen, monochrome print, OneHalfDark dark [t] | Typst built-in theme [e] | Pandoc-like token styles [w] styles.xml:55-85, docx/body.rs:25-63 | InspiredGitHub (highlighted mode) | no | none | M |
| 3.15 | Listing variant | one style | numbered or highlighted blocks shrink 10 % and gain a fill [w] | one style | – | no | none | E |
| 3.16 | Line-number colour | #5c626d | #666666 | #8C959F | – | no | colors.muted: H | E |
| 3.17 | Line-number alignment, gap | right, 1.5ch | left, 0.8em | right, one space | – | no | none | E |
| 3.18 | Highlighted-line colour | #fff0b3 | #fff1b8 | #FFF3BF | none | partly | none | E |
| 3.19 | Highlight extent | full-width bar | full-width row | the text only: run shading (docx/body.rs:693) in one Source Code paragraph with `<w:br/>` between lines (:613-623, :689); one paragraph per line gives a full-width bar | – | no | none | M |
| 3.20 | Math font | KaTeX under the default `[math] html = "katex"` [e]; Latin Modern Math under `mathml`; the PDF's font under `image` | New Computer Modern Math [w] lib.typ:456 | Cambria Math [w] settings.xml | LM Math | no | none | M |
| 3.21 | Math size | 1.21em = 13.31 pt [e] | 1em | 11 pt | – | no | text.size | E |
| 3.22 | Display math: previous baseline to equation; equation to next | 51.0 pt; 49.5 pt [t] theme.css:213-221 | 22.3 pt; 20.5 pt [e] | Equation 6/6 pt: 6 pt of space† | – | no | P only | M |
| 3.23 | Display alignment | centred | centred | centred | centred | yes | – | E |
| 3.24 | Equation numbers | (1) at the right | (1) at the right | (1) in a 10 % table cell | (1) | yes | – | E |
| 3.25 | Math colour | text | text | auto | – | yes | colors.text | E |
| 3.26 | Callout colours | note #326b9a, tip #2c7758, important #7954a1, warning #946118, caution #b24438 [t] theme.css:17-21 | #2867a2, #307445, #7950a0, #926500, #b33131 [w] callout.typ:24-28 | GitHub palette #0969DA, #1A7F37, #8250DF, #9A6700, #CF222E [w] styles.xml:87-91 | blue!60!black and others | no | none | E |
| 3.27 | Left border width | .25em: 2.25 pt | 2 pt | 3 pt | tcolorbox | no | none | E |
| 3.28 | Fill | none | 94 % tint | GitHub tints | 5 % tint | no | none | E |
| 3.29 | Radius | 0 | 2 pt | 0 | – | no | none | M |
| 3.30 | Padding | 9.35 pt / 12.65 pt | 8.8 pt | 3 pt top, about 8 pt left | – | no | none | M |
| 3.31 | Title font, colour | heading font bold, text colour | body font bold, kind colour | body font bold, text colour | white on a bar | no | fonts.heading: H | E |
| 3.32 | Title text | English terms, three separate tables | same | same | – | yes | – | E |
| 3.33 | Title to body | 21.0 pt [t] | 20.4 pt [e] | 3 pt of space† | – | partly | P | M |
| 3.34 | Between callouts | 54.0 pt [t] | 37.9 pt [e] | 6 pt of space† | – | no | – | M |
| 3.35 | Icons | none | none | none | – | yes | – | E |
| 3.36 | Custom kind | #5c626d | #62676c | #8C959F | – | no | none | E |
| 3.37 | Source Code, Callout, Equation styles | rules match nothing | ignored | applied | – | no | W | M |
| 3.38 | Cell prompts | shown, left of the cell on wide screens | none | own paragraph | off by default | no | `[notebook] show_prompts`: HW | M |
| 3.39 | Prompt style | mono .75em, muted | – | Consolas 9 pt #8C959F | – | no | – | E |
| 3.40 | Cell input | as code | as code | as code | – | partly | – | E |
| 3.41 | Output box | filled, no border | bare | white with a 1 pt #D0D7DE left rule | – | no | none | E |
| 3.42 | Output size | 9.35 pt | 8.8 pt | 10 pt | – | no | – | E |
| 3.43 | stderr and errors | red rule and tint | none | none | – | no | none | E |
| 3.44 | ANSI colours | palette | dropped | same hex as HTML | – | partly | none | E |
| 3.45 | Spacing between cell parts | stacked paddings and margins | 17.2 pt baseline to baseline | 6 pt of space† | – | no | P | M |

Code-block family (3.5) and code colour follow 3.0 and 3.4. R12 §8, Word /
PDF / HTML in pt: paragraph → code 23.00 / 22.92 / 34.38; code line → line
11.20 / 11.88 / 14.03; code → paragraph 19.20 / 24.22 / 34.11.

One Word size rule is off today. docx/design.rs:126 scales styles.xml
sizes by `text.size` with whole-point rounding (`(old * size / 11 / 2)
.round() * 2` half points), so Source Code stays 10 pt at `text.size =
10.5pt`. `half_points` (docx/design.rs:16-19) would round to half points
instead (M9).

### 2.6 Tables

| # | Property | HTML | PDF | Word | LaTeX | Agree | Reach | SS |
|---|---|---|---|---|---|---|---|---|
| 4.0 | Rules | 1.5 pt top and bottom, 0.75 pt under the header [t] | 1 pt on every cell edge [e] | 1 pt top and bottom, 0.5 pt under the header [w] styles.xml:54 | booktabs | no | none | E |
| 4.1 | Rule colour | #23262b; group dividers #c8c9c3 | black | #1F2328 | black | no | colors.text: H | E |
| 4.2 | Header weight | bold | regular | bold | regular | no | none | E |
| 4.3 | Header vertical alignment | top | top | bottom | – | partly | none | E |
| 4.4 | Row-header cells | bold | regular | bold | – | no | none | E |
| 4.5 | Cell padding, x / y ‡ | 7.15 / 5.5 pt (.65em / .5em) [t] theme.css:164 | 5 / 5 pt [e]; R12 0.5.0: y from Compact's spaces | 5.4 / 0 pt, plus Compact 1.8 pt [w] | 6 pt / – | no | Compact: P (inset), W; H after R12 T99 | M |
| 4.6 | Single-line row pitch ‡ | 28.6 pt [t] | 17.2 pt [e] | about 18.4 pt (u) | – | no | line_height: HW | M |
| 4.7 | Cell paragraph style ‡ | none at 0.4.0; R12 T99 applies a declared Compact to td | Compact only when declared [d] | always Compact [w] | – | partly | Compact: PW; H after R12 T99 | M |
| 4.8 | Cell alignment in a captioned table | left | centred, inherited from the figure [e] | left | from colspec | no | none | E |
| 4.9 | Justified cells | no | yes in (a) [w] | no, unless text.justify | – | partly | text.justify: PW | E |
| 4.10 | Width | full text width, columns by content | content width | full width, equal columns until Word autofits (u) | natural | no (auto widths); yes when the source gives widths | none | H |
| 4.11 | Placement | fills the width | captioned tables centred | left (full width) | – | partly | none | E |
| 4.12 | Row striping | none | none | none | – | yes | – | E |
| 4.13 | Header repeats on new pages | yes | yes | yes | – | yes | – | E |
| 4.14 | Page breaking | print: not broken inside [t] print.css:6 | captioned tables unbreakable [e] typst-library model/figure.rs:423; uncaptioned tables break | rows split | – | partly | none | M |
| 4.15 | Tabular figures | forced | font default | font default | – | unknown | none | E |
| 4.16 | Caption placement | above | above | above | above | yes | – | E |
| 4.17 | Caption style | 9.9 pt, muted, bold label | 11 pt, text colour | 10 pt italic #59636E [w] | 11 pt | no | Caption / Table Caption: PW | E |
| 4.18 | Caption alignment | left | centred | left | centred | no | Table Caption.align: PW | E |
| 4.19 | Caption label | "Table 1: " | same | same | English always | yes (en) | – | E |
| 4.20 | Caption to table | 16.5 pt [t] | 7.15 pt [e] | Caption 6 pt after against Compact 1.8 pt before: 6 pt† | – | no | Table Caption spacing: W | E |
| 4.21 | Paragraph to table | 16.5 pt, collapsed [t] | 13.2 pt [e] | previous paragraph's after: 6 pt [w] | – | no | P, W | M |
| 4.22 | Table to paragraph | 16.5 pt [t] | 13.2 pt [e] | next paragraph's before: 6 pt (a), 0 pt (b) [w] | – | no | P, W | M |

Row 4.10: when the source gives column widths, all three already write the
same fractions: docx/tables.rs:143-155 (a fixed grid), html/emit.rs:1196-1210
(`<col style="width:N%">`) and typst/emit.rs:818 (`columns:`). Only
auto-width tables differ. Row 4.14: writer emits captioned tables as
`#figure(`, whose show-set makes the block unbreakable; HTML print avoids
breaks inside every table, so the two agree for captioned tables.

R12 §8, Word / PDF / HTML in pt: paragraph → first row 27.67 / 26.73 /
35.10; row → row (Compact 10 pt at 1.3, 3/3 pt) 19.67 / 19.00 / 19.73;
table → paragraph 18.67 / 27.53 / 34.73. Word and HTML add a rule's
thickness to a ruled row (about 0.67 pt); Typst paints rules over the
grid. After a table the PDF leaves P where Word leaves none.

### 2.7 Footnotes, citations, links and references

| # | Property | HTML | PDF | Word | LaTeX | Agree | Reach | SS |
|---|---|---|---|---|---|---|---|---|
| 4.23 | Footnote placement | endnotes [t] | page foot | page foot | page foot | no | none | I |
| 4.24 | Marker | sup at 0.75em [t] | typographic sup, else 0.6em [e] | Word superscript (u) | superscript | no | – | M |
| 4.25 | Marker colour | link colour | text | text | linkcolor | no | colors.link: H | E |
| 4.26 | Separator | full width, 0.75 pt, #c8c9c3 | 30 % width, 0.55 pt, black [e] | Word built-in (u) | 0.4 pt × 40 % | no | none | M |
| 4.27 | Note size | 0.9em = 9.9 pt | 0.85em = 9.35 pt [e] typst-library model/footnote.rs:324-331 | 9 pt [w] | \footnotesize | no | Footnote Text: W | E |
| 4.28 | Note pitch ‡ | 15.84 pt | 10.8 pt: the built-in 0.5em leading beats writer's [e]; R12 0.5.0 with a declared design: 17.30 pt before R12 took R13's fix, 12.62 pt with it (§3.4 M12) | about 12.1 pt (u) | – | no | line_height: HW | E |
| 4.29 | Gap between notes | 11.38 pt | 5.5 pt | 3 pt | – | no | P: H | E |
| 4.30 | Number and indent | "1." list, 30 pt hanging | superscript, 1em first-line indent | superscript, flush | – | no | none | M |
| 4.31 | Back-links | ↩︎ | the number links back | none | – | partly | none | E |
| 4.32 | Numbering | continuous | continuous | continuous | continuous | yes | – | E |
| 4.33 | Footnote Text style | rule matches nothing | not applied | applied | – | partly | W | M |
| 4.34 | Citation style | ieee [w] | ieee through Typst's hayagriva [e] | ieee [w] | authoryear | partly | `[references]` | M |
| 4.35 | Citation linked | no | yes | no | yes, green | partly | none | E |
| 4.36 | Bibliography label column | 2em = 20.9 pt | about 20.1 pt, from the widest label (typst-layout src/rules.rs:465-494: auto columns, a 0.65em gutter at :453, no show step) | 36 pt tab | – | no | none | M |
| 4.37 | Hanging indent | 2em = 20.9 pt | 1.5em = 16.5 pt | 36 pt, fixed | – | no | none | M |
| 4.38 | Entry size | 0.95em = 10.45 pt | 11 pt | 11 pt | – | no | Bibliography: W | E |
| 4.39 | Entry gap | 0.75em = 7.84 pt | 13.2 pt (paragraph spacing) | 6 pt | – | no | P: P | E |
| 4.40 | Justification | left | justified in (a) | left | – | partly | text.justify: PW | E |
| 4.41 | Bibliography title | h1 20.35 pt | 15.4 pt | 18 pt | – | partly | heading styles | E |
| 4.42 | Bibliography style | rule matches nothing | not applied | applied | – | no | W | M |
| 4.43 | URL colour | #235c93 | black | #0969DA | #000099 | partly | colors.link: HPW | E |
| 4.44 | URL underline | yes | no | yes | no | no | none | E |
| 4.45 | URL printed after the link | yes, in print | no | no | – | no | none | E |
| 4.46 | Internal link | as URL | as URL | as URL | – | partly | colors.link | E |
| 4.47 | Cross-reference colour | link colour, underlined | text | text | #000099 | no | colors.link: H | E |
| 4.48 | Reference text | "Table 1" | same | same | Table~\ref | yes | – | E |
| C | Unresolved `[?label]` | red #cc0000, wavy underline [t] theme.css:292-296 | plain | plain | plain | no | none | E |

### 2.8 Named styles, divs and page furniture

The probes use a 40 mm top margin, Libertinus Serif 11 pt at 1.35 (pitch
14.85 pt, 9.83 pt above the baseline and 5.02 pt below in Word), P =
10.205 pt (3.6 mm), and a div style on an exact 12.1 pt pitch (9.68 pt
above, 2.42 pt below). The HTML and 0.4.0 PDF columns are the draft's
measurements. ‡ Word values are recomputed with R12's line box; R12's
0.5.0 PDF equals them by construction (R12 §6). Under R12, HTML's value is
Word's gap − c(upper style) + c(lower style) (§3.2), not re-measured here.

| # | Property | HTML | PDF | Word | LaTeX | Agree | Reach | SS |
|---|---|---|---|---|---|---|---|---|
| 5.0 | Div with 6 mm before, after a paragraph ‡ | margins collapse to the larger: 10.58 mm [d] | 0.4.0: weakness 3 replaces P, 8.57 mm [d] | 5.02 + max(10.205, 17.01) + 9.68 pt = 11.18 mm (u) | ignored | 0.5.0: PW | HPW | M |
| 5.1 | Div with 8 mm after ‡ | 12.98 mm | 0.4.0: 10.55 mm | 2.42 + 22.68 + 9.83 pt = 12.32 mm (u) | ignored | 0.5.0: PW | HPW | M |
| 5.2 | Paragraphs inside a styled div ‡ | P: 7.94 mm | 0.4.0: L + P, 7.87 mm | 0.4.0: `contextualSpacing` drops P, 4.27 mm (u) (docx/design.rs:563-569); with D1 (R12 T100): 7.87 mm (u) | – | 0.5.0: yes | indirect | M |
| 5.3 | Div spacing 1 mm (smaller than P), Alpha to Bravo ‡ | the larger, P: 8.71 mm | 0.4.0: replaces P, 3.56 mm | max(P, 1 mm): 8.84 mm (u)† | – | 0.5.0: PW | HPW | M |
| 5.4 | space_before at a page top: document start, after a forced break ‡ | kept, kept: 49.21 mm | 0.4.0: dropped, 42.54 mm; 0.5.0: kept at the start only | kept at the start, dropped after any break (R12 §2.4) | – | 0.5.0: PW; HTML keeps it after a forced break (T119) | none | M |
| 5.5 | Exact line 12.1 pt | 12.1 pt | 12.1 pt | 12.1 pt | – | yes | line_height | E |
| 5.6 | Line height 2.0: gaps at the paragraph's edges ‡ | half the extra on each side: 10.34 / 10.05 mm | 0.4.0: between lines only, 8.82 / 8.82 mm | box 22 pt, 9.83 pt above the baseline and 12.17 pt below: 8.84 / 11.36 mm (u) | – | 0.5.0: PW | – | M |
| 5.7 | First baseline in a 40 mm margin ‡ | 43.64 mm (half leading) | 0.4.0: 42.54 mm (cap height at the edge); 0.5.0: 43.47 mm | 40 mm + 9.83 pt = 43.47 mm (u) | – | 0.5.0: PW | none | H |
| 5.8 | keep_with_next | `break-after: avoid` | sticky | keepNext on every paragraph, which also keeps the div together | – | partly | – | E |
| 5.9 | keep_together | `break-inside: avoid` | unbreakable | keepLines per paragraph only | – | partly | – | E |
| 5.10 | frame | not applied | not applied | framePr | – | no | W by spec | M |
| 5.11 | `font = "heading"` with fonts.heading unset | theme's system-ui | body font | body font | – | partly | – | E |
| 5.12 | size | 9 pt | 9 pt | half points: 8.25 pt becomes 8.5 pt | – | yes | – | E |
| 5.13 | weight 600 | 600 | Semibold face | bold (open point 15; T117) | – | partly | – | M |
| 5.14–16 | colour, italic, alignment | agree | agree | agree | – | yes | – | E |
| 5.17 | Heading 2 given the div's keys ‡ | 0.4.0: h2 keeps the theme's margins, 18.27 mm; R12 T99 applies the keys | 0.4.0: §8.7 L + space, 10.27 mm | 10.27 mm (u)†, old line rule: recompute (T102) | – | 0.4.0: PW | PW; H after R12 T99 | M |
| 5.18 | Named div with no design (a) | 10.58 mm | 7.20 mm | basedOn Body Text 6/6 pt: 7.33 mm (u)† | children only | partly | none | M |
| 5.19 | Built-in page number | 9 pt, from page 2, baseline 288.40 mm | 11 pt, every page, 282.05 mm | 9 pt #59636E, every page | every page | no | slots | E |
| 5.20 | header_offset 12 mm, page-2 baseline ‡ | 15.59 mm | 0.4.0: 14.57 mm | about 16.28 mm (u), old line rule: recompute (T102) | – | partly | – | H |
| 5.21 | footer_offset 10 mm, last baseline ‡ | 285.75 mm | 0.4.0: 287.02 mm | about 286.26 mm (u), old line rule: recompute (T102) | – | partly | – | H |
| 5.22 | Offsets unset, header baseline ‡ | 3.70 mm | 0.4.0: 10.44 mm | 12.7 mm plus a line (u) | – | no | none | E |
| 5.23 | Furniture pitch | with design tokens, within 0.1 mm in all three; without, 6.35 / 5.08 / about 5.21 mm | | | – | partly | line_height | E |
| 5.24 | Furniture font | fonts.body, else Charter | body font | fonts.body, else Calibri | – | partly | fonts.body | E |
| 5.25 | Footer size, 0.75 × body | 8.25 pt | 8.25 pt | 8.5 pt | – | partly | – | E |
| 5.26 | Footer colour, unset | black | black | #59636E [w] | – | no | colors.muted | E |
| 5.27 | Header and Footer named styles | none applied (R12 §8) | font and line height only | only the exact line survives | – | no | – | M |
| 5.28 | Header rule position ‡ | 39.67 mm (margin-box border) | 0.4.0: 28.57 mm | about 31.0 mm (u), old line rule: recompute (T102) | – | no | – | M |
| 5.29 | Rule 0.5 pt | yes | yes | yes | – | partly | – | E |
| 5.30 | Header image 400×100 px, natural size | 105.7 mm | 141.1 mm | 105.8 mm | – | no | image.width | E |
| 5.31 | Image gap 3 mm, header baseline ‡ | 28.57 mm | 0.4.0: 27.52 mm | about 29.37 mm (u), old line rule: recompute (T102) | – | partly | – | H |
| 5.32 | right_lines, vertical | beside the last line (print) | beside line 1 | line 1 | – | partly | – | E |
| 5.33 | First and rest variants | margin boxes correct; fixed mode prints rest on page 1 | correct | titlePg | – | partly | – | M |
| 5.34 | `print = "fixed"` origin | page area | [na] | [na] | – | no | – | M |
| 5.35 | {page} / {pages} | margin boxes correct; screen and fixed mode show "of 1" | correct | fields | – | partly | – | I |
| 5.36 | Furniture on screen | spans the viewport | [na] | [na] | – | no | – | E |
| 5.37 | Per-line overrides | margin boxes cannot | per line | per run | – | partly | – | H |
| 5.38 | Furniture taller than the margin | clamped, overlaps the body | overlaps | the body moves down (R12 §8, p6) | – | no | none | M |
| 5.39 | Margins in furniture-only designs | 20/18 mm | 25 mm | 25.4 mm | 2.5cm | no | page.margin | E |

Row 5.6 corrects the draft: Word puts the extra below the text, not above,
so the larger gap follows the paragraph instead of preceding it. Row
5.4: R12's 0.5.0 PDF follows Word; HTML's forced-break case is a residual
that print CSS can remove (§3.1, T119). Row 5.7: the 0.5.0 PDF moves down
by the ascent minus the cap height, 0.92 mm here.

### 2.9 Inline and document-level rows

| # | Property | HTML | PDF | Word | LaTeX | Agree | Reach | SS |
|---|---|---|---|---|---|---|---|---|
| C | Div `.center` `.left` `.right` | text-align [t] theme.css:259-261 | ignored | ignored | center only as the first class | no | none; fixable as default named styles | E |
| C | Div `.small` `.large` | .85em / 1.25em [t] theme.css:262-263 | ignored | ignored | \small / \large | no | none | E |
| C | `div.columns` | grid, 1.5em gap [t] theme.css:252-258 | stacked | stacked | stacked | no | none | M |
| C | Superscript, subscript | 9.17 pt; a subscript grows the line to 18.9 pt [e] | the font's sups/subs glyphs; pitch unchanged [e] | `w:vertAlign` (u) | 8 pt | no | none | M |
| C | Strike, underline, small caps | browser, from font metrics | from font metrics; smcp | `w:strike`, `w:u`; small caps synthesised (u) | ulem 0.4 pt | partly | none | M |
| C | Overline, highlight (Typst sources) | overline; #fff0b3 | overline; #FFFD11 at 63 % alpha | both dropped | dropped | no | none | M |
| C | Hard line break | `<br>` | linebreak | `w:br` | `\\` | yes | – | E |
| C | Page break | print `break-after` | pagebreak | pageBreakBefore | yes | yes | – | E |
| C | Metadata | title, author, keywords, description, lang | Info and XMP | core.xml | pdftitle and pdflang only; prints keywords | yes (LaTeX no) | – | E |
| C | Author email and id | both shown, muted | neither | email as a Hyperlink run | neither | no | none | E |
| C | `lang` ar / de | LTR; no hyphenation | RTL from `lang`; German patterns | `w:lang` only, LTR (u) | LTR, English patterns | no | none | M |
| C | page.columns = 2 | not applied | not applied | two columns, 36 pt gap | not applied | no | W | M |

The critic's corrections, kept:
- **Word widow control.** Word's effective widow control is likely on,
  although styles.xml has no `w:widowControl`. T103 measures it.
- **Image natural size.** The single image default is "the file's DPI,
  else 96". A blanket 96 dpi would split the PDF from Word for tagged
  files.
- **LaTeX hyphenation and captions.** LaTeX loads no babel or polyglossia.
  It hyphenates every language with English patterns, and its caption
  labels stay English while writer localises the in-text references.

## 3. Semantic mismatches and models

Each model is the HTML/Word behaviour the owner prefers, made exact, with
Word as the reference wherever Word has a rule (design.md §0.3). The draft
had twelve models, M1–M12. Four are now R12's (§3.1), §3.2 states HTML's
residual, §3.3 extends the model to the elements R12 leaves out, and §3.4
revises the rest.

### 3.1 What R12 replaces

| Draft model | Replaced by | Left for R13 |
|---|---|---|
| M1 Spacing between blocks | R12 §2.3, §2.5, §4.2–4.4; design.md §0.3 "Blocks and their spaces"; D1. The draft's emitter rule (`above: max(S, P)` from the previous block) and its `v(.., weak: true)` alternative are superseded by 0.5.0's `par-end` and flow mark, which carry P as a block's `below` | the elements outside the model (§3.3) |
| M2 Line edges | Word's line box (R12 §2.2; design.md §0.3 "Line box"). The CSS box is not canonical and Word has no residual. The draft's Typst formula is moot; its bottom edge also had the wrong sign, since Typst negates a length bottom edge and clamps the line at the baseline | HTML's residual (§3.2) |
| M3 Line-height values | R12 §4.1; design.md §3.3 (`lines`) and §7.4: `w:line = round(240·m/s)`, and s and A sent to Typst | bundled metrics (T106); `atLeast` lines (T103); a code line height on the mono face (T113) |
| M5 Space at a page top | R12 §2.4; design.md §0.3 "Page tops": only the document's first block keeps its space before, after natural and forced breaks alike. No non-weak `v` after `#pagebreak()` | HTML print after a forced break (T119) |

The HTML forced-break residual needs one print rule,
`.page-break + * { margin-top: 0 }`, after the `.page-break` rule of
print.css:5.

### 3.2 HTML line edges

CSS centres the font's content area in the line box; Word hangs it from the
ascent (design.md §0.3 "Agreement"). With the same pitch and the same
spaces, every line of an HTML style sits a constant c off Word's position
in its box, and the gap between two blocks differs from Word's by
c(lower) − c(upper). With Chrome's ascent and descent equal to Word's A
and s − A, theory gives c = (m − s)/2·size for m ≥ s and c = (s − m)·size
·(A/s − ½) for m < s.

Chrome rounds, though. Probe *residual* finds the baseline at
round(ascent in px) + floor(half-leading in px) below the box top, in CSS
px of 0.75 pt (the floor was seen only with a negative half-leading). For
body text at 1.35 the rounding turns a theoretical +0.18 pt into −0.18 pt:

| Style (Source Sans 3) | Above the baseline in Chrome | In Word | c |
|---|---|---|---|
| body 11 pt at 1.35 | 10.50 pt | 10.68 pt | −0.18 pt |
| H1 16 pt bold at 1.2 | 14.25 pt | 13.81 pt | +0.44 pt |
| H2 13 pt bold at 1.2 | 11.25 pt | 11.22 pt | +0.03 pt |
| 16 pt on an exact 20 pt | 14.25 pt | 16.00 pt | −1.75 pt |

Gaps between baselines in Chrome's screen layout, in pt:

| | body → H1 | H1 → body | body → H2 | H2 → body | body → x20 | x20 → body |
|---|---|---|---|---|---|---|
| Plain CSS | 30.60 | 21.45 | 25.79 | 18.83 | 28.79 | 26.45 |
| Word's model | 29.98 | 22.07 | 25.59 | 19.06 | 30.38 | 24.88 |
| CSS with `top: −c` per style | 29.99 | 22.05 | 25.59 | 19.03 | 30.36 | 24.88 |

A paint-time shift, `position: relative; top: −c` on each style, puts
every baseline on Word's within 0.03 pt and leaves layout untouched.
R12 §6 and design.md §0.3 "Agreement" said nothing short of giving up
`line-height` removes the residual; after R13 reported this probe, both
now say a per-style shift can, and leave generating it to R13 (D18).
Caveats:
- c must reproduce Chrome's rounding, so it depends on the CSS px grid and
  on the platform's metrics source (hhea on macOS; on Windows the win
  metrics unless USE_TYPO_METRICS is set). Source Sans 3's hhea and typo
  metrics agree (1024/−400) but its win metrics do not (934/288). On a
  screen with two device pixels per CSS px the rounding is unverified.
- A relative shift moves paint, not boxes. On a bordered or filled block
  (quotes, code, callouts) it must go on an inner element, or the
  decoration moves with the text.
- Printed HTML still snaps every baseline to 0.75 pt, so print gains the
  mean, not each gap.
- CSS `text-box-trim` and `text-box-edge` might set the edges directly;
  that is untested in Chrome.

Decision D18 chooses between stating c per style as exception 11, and
generating the shift (T119).

### 3.3 Elements outside R12's model: one rule

design.md §0.3's scope stops at body paragraphs, divs and the §8.7
vocabulary styles. R13 extends the same model to every block element:
1. **Each element is a block** with a space before and a space after, from
   its default named style in the built-in design (§4.2), and line boxes
   per §0.3 at its own size and line height.
2. **The larger space holds** between two blocks. A container's first and
   last children do not add their own spaces at its edges: D1's rule for
   divs, applied to quotes, callouts, list items and cells.
3. **Decorations count in all three.** A border's width, a padding and a
   rule's stroke add to the distance between line boxes. Word holds them as
   paragraph borders with `w:space`, shading and cell margins; CSS as
   border and padding; Typst as a block's `inset` and `stroke`. Typst
   paints table rules over the grid, so the PDF adds the stroke to the row.
4. **The PDF ignores Typst's built-in spacing.** Quote, raw, figure,
   equation, list and table take the style's spaces as
   `block(above:, below:)` set on each element, replacing Typst's
   built-ins, as the vocabulary styles do (design.md §6.2); `par-end`
   makes the neighbouring paragraph's P a block space, so the larger of
   the two holds (R12 §4.2).
5. **Word writes direct spacing only where no style can carry it:** the
   last item of a list, the paragraphs on either side of a table, and the
   separator between two tables.
6. **Acceptance** per element: Word ↔ PDF within 0.3 pt in the harness;
   HTML within 0.3 pt after c (§0.2).

| Element | HTML | Typst | Word | Today, Word / PDF / HTML (pt) | Task |
|---|---|---|---|---|---|
| Tight list item | `li` of a tight list | `list.item`, `enum.item` | list-item style (D17) | item → item 16.80 / 14.85 / 17.59 | T112 |
| List block | `ul`, `ol` | `list`, `enum` | direct after on the last item | tight list → paragraph 16.60 / 25.05 / 27.49 | T112 |
| Definition list | `dl`, `dt`, `dd` | `terms` | Definition Term, Definition | rows 2.17–2.19 | T112 |
| Block quote | `blockquote` | `quote` | Block Text | 25.20 / 41.25 / 27.79 in; 19.80 / 34.65 / 40.43 out | T112 |
| Rule | `hr` | `line` | Horizontal Rule | row 2.31 | T112 |
| Code block | `pre` | `raw` block | Source Code, one paragraph per line | 23.00 / 22.92 / 34.38 in; line 11.20 / 11.88 / 14.03; 19.20 / 24.22 / 34.11 out | T113 |
| Display math | `.math.display` | `math.equation` block | Equation | row 3.22 | T113 |
| Callout | `.callout` | callout.typ | Callout | rows 3.33, 3.34 | T113 |
| Notebook cell | cell parts | cells.typ | cell styles | row 3.45 | T113 |
| Table | `table`, `td` | `table` | Table, Compact (cells) | 27.67 / 26.73 / 35.10 in; row 19.67 / 19.00 / 19.73; 18.67 / 27.53 / 34.73 out | T114 |
| Figure, caption | `figure`, `figcaption` | `figure`, `figure.caption` | Captioned Figure, Caption | rows 1.35, 1.37 (not in R12) | T110 |
| Footnote | `.footnotes li` | `footnote.entry` | Footnote Text | rows 4.27–4.29 | T115 |
| Bibliography entry | `.csl-entry` | content written by writer (T115) | Bibliography | rows 4.36–4.39 | T115 |
| Heading, title block, contents | `h1`–`h6`, `.title-block`, `nav.toc` | `heading`, title-block, `outline` | Heading N, Title … `toc N` | undeclared Heading 3 29.0 / 30.9 / 42.7 (R12 §6) | T109 |

Headings, the title block and the contents are vocabulary styles, so they
join R12's model as soon as the built-in design declares them; T109 only
fills their values.

R12 §7 now tabulates how 0.5.0 moves these blocks in the PDF against
0.4.0 (11 pt Libertinus Serif at 1.35, P = 10 pt). Block quotes move 7.6 pt
further from the text on each side, away from Word; a figure, a table or a
one-row display equation moves 2.6 pt closer above and 5.0 pt closer
below; code lines go from 12.78 to 11.88 pt. R12 converts none of them
back and leaves one shared value per element to R13: the task in the
table's last column sets it.

### 3.4 The other models, revised

**M4. Tight lists.**
- **Model.** Items are S apart (D17; HTML's 0.25em). A list block has
  before 0 and after P, like a paragraph. A nested list sits S from its
  parent item's text and from the next item.
- **HTML.** `li + li { margin-top: S }`,
  `li > :where(ul, ol) { margin-block: S 0 }` and
  `:where(ul, ol) { margin-block: 0 P }`. The nested list's end then takes
  the next item's S, in place of theme.css:97's .35em.
- **Typst.** 0.5.0 already gives tight lists `par(spacing: 0pt)` (R12
  §4.2). Add `list(spacing: S)` and `enum(spacing: S)`. A tight list's
  paragraphs have spacing 0pt and get no flow mark from `par-end`; the
  list's P below is R12's `set block(below: P)`. Typst also puts a weak,
  attached `v(S)` before a tight list (typst-layout 0.15.1
  rules.rs:131-138 and :150-157); T112 checks the paragraph-to-list
  space against R12's T98 criterion 5. Remove the blank line the emitter
  writes between an item's text and its nested list
  (typst/emit.rs:629-648).
- **Word.** `Compact` has two roles today: Word's tight list paragraph,
  and the cell inset in the PDF and HTML (R12 §8; design.md open point 14).
  M10 gives `Compact` the table's y inset (R12's `cell-inset`), so tight
  list items take a list-item style of their own, and R12's `space_div`
  takes that style's spaces for tight items in a div (T112). D17 decides
  only the style's name and values. The item style has before 0 and
  after S, and writer writes a direct after = P on the last item of a
  top-level list (Word leaves 1.8 pt there today: 16.60 pt in R12 §8). At
  11 pt, S = 2.75 pt is after = 55 twips.

**M6. Image natural size.**
- **Model.** The file's DPI, else 96 dpi. writer reads PNG pHYs and JPEG
  JFIF density today, x and y separately and only between 10 and
  2400 dpi (src/docx/images.rs:336-438: `pixel_size` :336-361, PNG
  :371-396, JPEG :398-438). It reads no EXIF. Typst does, and consults
  EXIF first, taking the larger axis (typst-library
  visualize/image/raster.rs:366-374), so EXIF would be new in writer.
- The size is computed once, by that function moved out of src/docx, and
  written explicitly:
  - HTML: px = inches × 96, with `max-width: 100%`.
  - Typst: `image(width: inches × 72pt)`, block and inline
    (typst/emit.rs:1386-1406 writes `image(..)`; :1156-1160 wraps inline
    ones in `#box(..)`).
  - Word: EMU = inches × 914400, as today.
  - LaTeX: `width=` the smaller of the natural width and `\linewidth`.
  - Header and footer images follow the same rule.

**M7. Fonts.**
- **Model.** Each role (body, heading, mono, math) resolves to a family
  that writer ships or names, with the same fallback everywhere.
- **Option A, the target.** Ship one OFL family with TrueType outlines
  (regular, italic, bold, bold italic), plus a mono and a math font. HTML
  gets `@font-face` from the asset registry, Typst the files, Word an
  embedded copy (design.md open point 1: `w:embedRegular` in fontTable.xml
  with ODTTF obfuscation behind `[docx] embed_fonts`). TrueType outlines
  avoid the unverified CFF case.
- **Option B, interim.** Serve the PDF's Libertinus Serif to HTML through
  `@font-face` and name it in Word, which substitutes it where it is not
  installed. Check first that Chrome's metrics for that face match R12
  §2.1's Libertinus row (s 1.140, A 0.894).
- **Metrics.** Under R12 both the PDF line box and Word's `w:line` depend
  on a family's s and A, so whichever fonts are chosen, their metrics ship
  with writer (T106).
- **New key `fonts.math`.** Typst and Word (`m:mathFont` in settings.xml)
  follow it; HTML follows it outside KaTeX mode (exception 5).

**M8. Colours.**
- **Model.** One palette: text, muted, accent, link, border, code_bg,
  highlight, error, background, and
  callout.{note, tip, important, warning, caution, other}. Every element
  names a token.
- **Mapping.** CSS custom properties `--w-*`, already the model; the Typst
  design dictionary, read by lib.typ and callout.typ; Word's generated
  `w:color` and `w:shd`.
- **Word page background.** `w:background` needs `w:displayBackgroundShape`
  and prints oddly (open point 3). Instead writer draws `colors.background`
  as a page-sized picture behind the text: a 1×1 PNG of the colour with fit
  = stretch, through the background-image path (docx/furniture.rs:608-634,
  an anchored drawing with `behindDoc="1"` relative to the page; :67-129
  gives it a header part when no header slot exists). The colour is drawn
  before an image (relativeHeight 0 at :627). Row 0.16 becomes M.
- This removes styles.xml's 59636E, 0F4761, 1F2328, D0D7DE, 8C959F,
  F6F8FA and 0969DA, callout.typ:24-28's palette, and lib.typ's #f7f7f7
  and #666666.

**M9. Relative sizes.**
- **Model.** Sizes are given in em of `text.size`, or of the style's own
  size for its spaces, and the resolver turns them into points.
- **Rounding (D19).** The resolver rounds every resolved font size to
  0.5 pt for all backends, so Word's half-point grid costs nothing and
  exception 15 disappears. Spaces stay exact: Word holds them in twips.
  docx/design.rs:126 moves to `half_points`.
- Without D19, Word differs by up to 0.25 pt per size, for example a
  0.75em footer at 11 pt: 8.25 pt becomes 8.5 pt.

**M10. Tables.**
- **Model.** Outer rule width, header rule width, a colour token, an x
  inset in em, a bold header, and a row pitch equal to the line box + 2y +
  the rule's stroke. The y inset is `Compact`'s before and after, as R12
  made it (design.md §6.2 `cell-inset`); there is no second key.
- **CSS:** `border-collapse`; `padding: y x`.
- **Typst:** `table(stroke: none, inset: (x: ..))` with `table.hline` at
  the top, under the header and at the bottom, the stroke added to the
  row; `show table.cell.where(y: 0): strong`.
- **Word:** `tblBorders` top and bottom; the header rule through
  `tblStylePr firstRow`; `tblCellMar` left and right = 20·x twips only;
  the y inset stays in `Compact`'s before and after.
- **Space around a table.** The table style has a before and an after, and
  the larger-of rule applies against its neighbours. Word cannot put
  spacing on a table, so writer writes the larger value as direct spacing
  on the paragraphs before and after it. Between two tables, and after a
  table that ends the body or a footnote, writer already writes an empty
  separator paragraph (`separate_table` and `finish_block_sequence`,
  docx/body.rs:772-785). Today it is a Normal paragraph one line tall,
  about 14.8 pt. Give it `w:before` = the space − 0.7 pt and an exact
  0.7 pt line (`w:line="14" w:lineRule="exact"`, the minimum
  docx/furniture.rs:578-580 already uses). Cells hold one paragraph of
  inlines (docx/tables.rs:270-296), so a table at the end of a cell occurs
  only in raw OpenXML. This was the draft's exception 11; it is not one.
- **Width** is D9. Tables with given column widths already agree (§2.6).

**M11. List geometry.**
- **Model.** The text starts I per level from the parent's text; the
  marker ends G before the text, right-aligned.
- **CSS:** `padding-left: I`. For an exact G, `list-style: none` and
  `li::before { position: absolute; width: W; left: calc(-W - G);
  text-align: right }` (or `right: 100%; margin-right: G`). With
  `::marker`, Chrome places the marker itself, about 8.5 pt away.
- **Typst:** `list(indent: I − W − G, body-indent: G,
  marker: box(width: W, align(right, ..)))`, with W at least the widest
  marker and I ≥ W + G. Typst starts the body at indent + marker width +
  body-indent (typst-layout lists.rs:448), so the text starts at I and the
  marker ends G before it. `enum` takes the same box through `numbering:`.
- **Word:** `w:ind left = I·level`, hanging = G, a right-aligned number
  (`lvlJc right`) and a tab.

**M12. Footnote geometry.** HTML's endnotes are exception 1. Size, pitch,
gap and separator are still shared:
- **Typst:** R12's T98 already sets `show footnote.entry: set
  par(leading: 0pt)` and `set footnote.entry(gap: note-gap)`. R13 adds
  `show footnote.entry: set text(size: ..)` with the line-box edges, since
  the entry's built-in show-set sets a 0.5em leading and a 0.85em size
  that document-level set rules cannot reach (typst-library
  model/footnote.rs:324-331), replaces the 0.85 in R12's `spacing-model`
  note gap with the design's note size, and sets
  `set footnote.entry(separator: line(length: .., stroke: ..), gap: ..,
  clearance: ..)` from the design.
- **Word:** Footnote Text, and a separator paragraph with a top border in
  footnotes.xml.
- **A regression in R12's 0.5.0, now fixed in R12.** The entry's
  show-set survived `paragraph-layout`'s `par(leading: 0pt)`. With a
  declared design (Libertinus 11 pt at 1.35), footnote lines sat
  12.62 + 4.675 = 17.30 pt apart, against 10.83 pt under 0.4.0 and wider
  than the body's 14.85 pt (probe *fn*). Adding
  `show footnote.entry: set par(leading: 0pt)` in the model branch gives
  12.62 pt (1.35 × 9.35 pt). After R13 reported it, R12 took the fix into
  T98 (R12 §3, §4.2 and §5.3), with `set footnote.entry(gap: note-gap)`,
  which keeps two notes no closer than plain Typst has them. T115 starts
  from that and sets the note gap from the design.

## 4. Proposed single-source design

### 4.1 The built-in layer

**Layer 0.**
- `src/design/defaults/<domain>.toml` (text, fonts, colors, page, styles,
  lists, table, code, callout, image, footnotes, links) together form a
  complete `[design]` table: every key in the schema plus §4.2's additions.
- They are compiled into the binary and merged as layer 0 below the theme
  chain. `with_design_layers` (src/pipeline/mod.rs:458-503) pushes them
  first, before the theme layers built at :480-481, and `merge`
  (src/design/merge.rs:115) records a new `Origin::Builtin` for their
  keys.
- The one resolver turns em lengths into points per element, and rounds
  font sizes (D19).

**Two flags instead of one.** `declared` (src/design/resolve.rs:20)
splits:
- `user_declared`: a layer above layer 0 has a key. It drives
  `writer check`, its lints and provenance.
- The design the backends see is a view, `Design::view(APPLIED)`. A
  constant `APPLIED: Domains` lists the domains whose built-in values reach
  the backends. The view drops builtin-origin keys of every other domain
  before resolving, and its `declared` is true when anything is left.
- `APPLIED` has one bit per key group, not per defaults file: styles.toml,
  text.toml, fonts.toml, colors.toml and page.toml are each split between
  T108 and later tasks (§6.2).
- While `APPLIED` is empty, the view equals today's design, `declared`
  included, so no backend changes and every snapshot stays. Each domain
  task switches its bits on. Backends keep reading `declared`, except
  HTML, which also needs to know which values are built-in (below).

Proposed assignment of §1.4's places: the backends, src/typst/world.rs:621,
src/assets/export.rs:137 and src/design/json.rs:73 read the view's
`declared`; src/check/design.rs:43, :95 and src/check/targets.rs:42 read
`user_declared`. T105 confirms each.

**PDF.**
- writer passes the viewed design to `writer-doc`; once a domain is on,
  that is always a non-empty design.
- The package carries `writer-defaults.json`, generated from the defaults
  and checked by a test. A hand-written template that calls `writer-doc`
  without writer's input falls back to it, so the package's no-design
  branch (0.65em/1.2em) serves nobody and goes.
- R12's T98 criterion 7 (`an_undeclared_design_keeps_typsts_layout`) is
  re-specified as a test of that package-only path, and a pipeline test
  checks that a document without `[design]` gets the built-in model in Word
  and the PDF. T98 criterion 6's "not set" rows (cap height + 0.65em;
  max(0, 1.2em − (pitch − cap))) become unreachable from writer builds.
- The package has one gate per document, `settings.design.len() == 0` in
  `paragraph-layout`. Once T108 removes it, R12 §7's moves reach every
  element of an undeclared document in the PDF, including the domains not
  yet switched on: undeclared headings, quotes, code, figures, tables,
  equations and footnotes move from T108, not from their own tasks.

**HTML.**
- `tokens_css` emits every token.
- Built-in values come out as rules before the theme entries (D16), so the
  default theme's CSS and existing themes still win where the user
  declared nothing. User-declared values keep D2: after the theme, behind
  `:where([data-writer-document])`.
- One merged view cannot do this: `layout_css` writes R12's `styles_css`
  and `vocabulary_css` after the theme, and the view carries no origin per
  key. T105 therefore exposes which values are built-in, as a pair of
  views (built-in, user) or an origin per key, and HTML writes its
  generated rules twice: the built-in part before the theme, the user part
  after it.
- theme.css keeps what is HTML's own: the screen sheet (width, padding,
  responsive rules), dark mode, the nav, notebook prompt placement.
- The default theme's CSS is 13.3 KiB (13,614 B) today (theme.css
  8,743 B; print.css 1,959 B; notebook.css 2,912 B), against the 24 KiB
  budget in html-output.md:620 (§6.3, :617-625). The generated sheet
  counts toward it.

**Word.**
- styles.xml is generated from the design. assets/docx/styles.xml shrinks
  to structure: ids, names, `basedOn`, outline levels, numbering links and
  latent styles.
- numbering.xml takes its indents and bullet fonts from the list keys;
  settings.xml and docDefaults take kerning, ligatures, hyphenation and
  widow control from the text keys.

**Metrics.** `src/design/metrics.toml` holds s and A for the default and
common families, and the hhea and OS/2 typo and win ascent and descent that
Chrome uses, from which T119 computes HTML's c. lines.rs uses it before
reading font files and warns once per family it cannot resolve, in place of
the silent Calibri fallback (lines.rs:19-22); the table also feeds the `lines`
input (design.md §3.3). Output then no longer depends on the fonts installed
on the build machine.

**LaTeX.** writer-template.tex takes the design through template
variables: fontspec families, sizes, geometry margins, xcolor colours,
hyperref link colours, babel or polyglossia from `lang`, and caption
labels from writer's l10n tables. Spacing follows the same model:
larger-of spaces, `\baselineskip` = pitch.

**Guard test.** A test scans backend sources and assets for colour and
length literals outside the defaults and an allowlist (T121).

**Spec edits, made by the coordinator** before T105's hand-off, as R12's
were: design.md §0.2 (§4.4 below), §0.3's scope sentence and "Scope"
paragraph, §1.1 (layer 0), §3 (the view, em lengths, rounding), §6.1
(`writer-design` always inserted), §6.2 (the no-design branch), §6.7 ("An
undeclared design reserves nothing"), §8.7 ("Absent means unchanged"),
§9.1's stale sentence (§1.3), §9.3 ("an undeclared design renders
nothing"), the byte-identity statements of §5.1 and §7.1 and the
introduction's last sentence, and a new §0.4 that holds §5's list of
exceptions.

### 4.2 Schema additions

- **Lengths** accept em: of the style's size inside a style, else of
  `text.size`.
- **`[design.text]`:** `hyphenate` (bool), `widows` and `orphans` (lines),
  `kerning`, `ligatures`, `measure` (screen line length in em),
  `screen_scale` (D8).
- **`[design.fonts]`:** `math`.
- **`[design.colors]`:** `border`, `code_bg`, `highlight`, `error`, and
  `callout = { note, tip, important, warning, caution, other }`.
- **Style keys:** `case` ("upper" or "smallcaps"), `letter_spacing`,
  `indent` (left, right), `border` (side, width, colour, gap), `fill`,
  `padding`. They apply to block styles; Word maps them to `w:caps`,
  `w:spacing`, `w:ind`, `w:pBdr` and `w:shd`.
- **Element tables** for geometry no Word paragraph style holds:
  - `[design.lists]`: `tight_spacing`, `indent`, `marker_gap`, `bullets`
    (and the list-item style, if D17 says so);
  - `[design.table]`: `rules`, `rule_color`, `inset_x`, `header_weight`,
    `header_fill`, `width` (the y inset is `Compact`'s, M10);
  - `[design.code]`: `background`, `padding`, `radius`, `line_height`,
    `theme`;
  - `[design.callout]`: `border_width`, `fill` (a tint), `padding`,
    `radius`;
  - `[design.image]`: `dpi = 96`;
  - `[design.footnotes]`: `separator`, `gap`, `clearance`;
  - `[design.links]`: `underline`, `print_url`.
- **Default named styles:** `Heading 1`–`6`, `Title`, `Subtitle`,
  `Author`, `Date`, `Abstract Title`, `Abstract`, `Caption`, `Image
  Caption`, `Table Caption`, `TOC Heading`, `toc 1`–`3`, `Block Text`,
  `Source Code`, `Compact`, `Footnote Text`, `Bibliography`,
  `Horizontal Rule`, `Header`, `Footer`, and `center`, `left`, `right`,
  `small` and `large` for the div classes.

### 4.3 Default values

The rule, in order: the three formats agree first; then the HTML/Word
look, the less tight one, over the PDF's. Where HTML and Word disagree,
HTML's designed value is proposed. A value that is a matter of taste is an
owner decision. Points are at `text.size` 11 pt, with font sizes rounded
to 0.5 pt (D19) and spaces taken from the rounded size.

| Key | Proposed default | Resolved at 11 pt | HTML / PDF / Word today |
|---|---|---|---|
| text.size | 11pt | 11 pt | 11 pt print, 12 pt screen / 11 / 11 (D8: screen scale) |
| fonts.body, heading, mono, math | one shipped family, a sans companion, a mono and a math font (D7) | – | Charter, system-ui, Menlo, KaTeX / Libertinus, body, DejaVu, NCM Math / Calibri, Calibri Light, Consolas, Cambria Math |
| text.line_height | 1.6 (D3) | pitch 17.6 pt; `w:line` 337 (Libertinus), 270 (Source Sans 3), 315 (Calibri) | 1.6 / 14.4 pt / 14.77 pt |
| text.paragraph_spacing | 1.15em (D4) | 12.65 pt; Word after 253 twips; paragraph pitch 30.25 pt | 12.65 / 6.05 / 6 pt |
| text.justify, hyphenate | false, false | – | off, off / on, on / off, off |
| kerning, ligatures | on | – | on / on / off |
| widows, orphans | 2, 2, written explicitly | – | 3 / prevented / likely on |
| colors | text #23262b, muted #5c626d, accent #366b76, link #235c93, border #c8c9c3, code_bg #f2f1ec, highlight #fff0b3, error #cc0000; callouts as theme.css:17-21; background #ffffff | – | HTML theme values / black / hard-coded Word values |
| page.paper, margin | a4, 25mm (D5) | 25 mm | 20/18 mm / 25 mm / 25.4 mm |
| header/footer offset | 12.5mm, half the margin, to the line box (design.md §9.1, §9.4) | 12.5 mm | 4.2–12.0 pt / 30 % of the margin / 0.5in |
| page number | footer slot "{page}", centred, 0.75em, muted; page 1 numbered or not (D6) | 8.5 pt | page 2 on / every page / every page |
| Heading 1–6 | 1.85/1.45/1.2/1/1/1em, bold, text colour, heading font; before 2em and after 0.65em of its own size (D12); line height 1.25; keep with next | 20.5/16/13/11/11/11 pt; H1 before 41 pt, after 13.33 pt, pitch 25.63 pt | HTML values / 1.4em, 8.25 pt below / Word values |
| Title, Subtitle | 2.4em bold, heading font, centred; Subtitle 1.2em muted | 26.5 pt; 13 pt | 26.4 / 17 / 26 pt |
| Author, Date | 1em, name bold; affiliation 0.9em muted; date muted | 11 pt; 10 pt | HTML |
| Title-block close | 0.75pt border-colour rule, 2em above, 2.5em below | 22 pt; 27.5 pt | HTML only |
| Abstract | title 0.85em bold, `case = "upper"`, letter spacing 0.08em; body inset 3.5em | 9.5 pt; 38.5 pt | HTML |
| Contents | TOC Heading as an unlisted Heading 1; entries 1em, body font, 0.25em apart, 1.2em indent per level; dot leaders and page numbers in paged output (D10 for printed HTML) | 2.75 pt; 13.2 pt | – |
| Caption | 0.9em, muted, bold label, upright; images centred, tables and listings left; 0.6em from the body | 10 pt; 6 pt | HTML |
| Figure, table blocks | 1.75em / 1.5em before and after | 19.25 pt / 16.5 pt | HTML |
| Code | 0.85em mono, line height 1.5, code_bg, padding 1em, radius 0.25em; after P | 9.5 pt; pitch 14.25 pt; padding 9.5 pt | HTML |
| Inline code | 0.9em on code_bg, padding 0.1em 0.3em | 10 pt | HTML |
| Syntax palette | InspiredGitHub, one .tmTheme for all three formats | – | four schemes |
| Display math | 1.5em before and after | 16.5 pt | 51 pt / 22 pt / 6 pt |
| Callout | border 0.25em in the kind colour, no fill, padding 0.85em 1.15em, title heading font bold, 0.4em to the body, 1.5em around | 2.75 pt; 9.35 / 12.65 pt; 4.4 pt; 16.5 pt | HTML |
| Lists | tight spacing 0.25em (D17); indent 2.75em; marker gap 0.75em; bullets • ◦ ▪; numbers "1." | 2.75 pt; 30.25 pt; 8.25 pt | HTML |
| Definitions | term bold, keep with next; description below, 1.5em indent, 0.25em below the term | 16.5 pt; 2.75 pt | HTML |
| Block quote | left border 1.5pt in border colour, 1.25em gap, muted, no right inset; before 0, after P | 13.75 pt gap | HTML |
| Rule | 0.75pt, border colour, full width, 2em above and below | 22 pt | HTML |
| Table | three rules 1.5/0.75pt in text colour, bold header, inset x 0.65em, y 0.5em (`Compact`'s before and after), left; width (D9) | 7.15 pt; 5.5 pt | HTML |
| Footnotes | page foot in paged output; 0.9em; separator 33 % of the width, 0.5pt, border colour; marker 0.75em in link colour | 10 pt; 8.5 pt marker | – |
| Bibliography | 0.95em; 0.75em between entries; 2em label column and hanging indent | 10.5 pt; 7.88 pt; 21 pt | HTML |
| Links, cross-references | link colour, underlined (0.06em at a 0.15em offset); print_url false (D11) | 0.66 pt at 1.65 pt | HTML |
| Superscript, subscript | 0.75em; raise 0.35em, lower 0.15em; no line growth | 8.5 pt | – |
| Image | 96 dpi default, max width 100 %, centred | – | – |

Owner decisions on values, renumbered from the draft's D1–D10 (R12 holds
D1 and D2):
- **D3. Body line height:** 1.6 (HTML; proposed) or about 1.35 (Word's
  14.77 pt).
- **D4. Paragraph spacing:** 1.15em (HTML; proposed) or 6 pt (Word and
  the PDF).
- **D5. Margins:** 25 mm (PDF, Word, LaTeX; proposed) or HTML's
  20/18 mm.
- **D6. Is page 1 numbered?** HTML print says no; the PDF, Word and LaTeX
  say yes.
- **D7. Font families:** the Option A family, and Option B until it ships
  (M7).
- **D8. HTML on screen at 12 pt, or at the print size.** Screen only; it
  does not affect agreement in print.
- **D9. Table width:** full text width or content width.
- **D10. Contents in printed HTML:** shown without page numbers, or
  hidden.
- **D11. URLs printed after links** in HTML print (proposed: no).
- **D12. Heading space above:** HTML's 2em (41 pt before H1; proposed) or
  Word's tighter 18 pt.

The others are D13–D15 (§4.6), D16 (§4.1), D17 (§3.4, M4), D18
(§3.2) and D19 (§3.4, M9).

### 4.4 design.md §0.2 rewritten (proposed wording)

- **One set of defaults.** writer's built-in design (`src/design/defaults/`)
  is the lowest layer (§1.1) and sets every key. With nothing declared,
  every backend renders the built-in design. No backend falls back to an
  engine default for a property the design governs. The exceptions listed
  in §0.4 are the only presentational values a backend owns.
- **Unset means inherited.** A key a layer leaves unset takes the value
  from the layer below, ending at the built-in design. em lengths follow
  `text.size` or the style's size.
- **One meaning everywhere.** Each key has one model: §0.3's line box, the
  larger of two adjoining spaces, and image size from the file's DPI or 96.
  Each backend converts that model exactly, or with a residual named in
  §0.4.
- **Escape hatches win.** Unchanged, with D16 for built-in values in HTML.
- **One resolver, no literals.** As today, and backends hold no
  presentational literals; a test checks this.
- **Deterministic.** As today, and independent of the fonts installed on
  the build machine.

Of today's invariants, "Absent means unchanged" and "Unset means the
backend's default" are replaced; "Escape hatches win" stays; "One
resolver" and "Deterministic" are strengthened. §0.3 changes with them:
its whole sentence "The model applies to a **declared** design only
(§0.2): without one, the PDF keeps Typst's own leading and paragraph
spacing and HTML keeps the theme's CSS." becomes "The model applies to
every document (§0.2): without `[design]`, the built-in design applies
(§0.4).", and its "Scope" paragraph grows as each element of §3.3 joins
the model, ending with "§0.4 lists the residuals". A design
is then *user-declared* when a layer above the built-in one has a key, and
that flag serves `writer check` and provenance only.

### 4.5 Escape hatches

- **Theme CSS** still wins over built-in values: they come before the
  theme's entries (D16). A user-declared value keeps D2 and beats a theme
  rule of equal specificity. A theme that wants a different value in every
  format sets `[design]`; CSS remains for looks that are HTML's own.
- **Hand-written Typst templates.** Rules after
  `show: base.writer-doc.with(..)` still win (design.md §6.3).
  `design-tokens` and `style-tokens` now always return complete values.
- **reference.docx** still wins verbatim (design.md §7.3). `writer check`
  may note that a reference document overrides the built-in design.

### 4.6 Compatibility

- **Snapshots.** 628 files in tests/snapshots, in main and in the
  worktree. Each domain task regenerates its own share. The "absent means
  unchanged" proofs are replaced by golden snapshots of the default
  output, plus the harness's cross-format check on its reference
  documents. That retires R12's T98 criterion 12 (no snapshot changes),
  T99 criterion 8 (three snapshots change by added lines only) and T100
  criterion 6 (no other snapshot changes) as each domain switches on.
- **D13. The built-in design applies to every document.** A contract
  change after T101: documents without `[design]` change look in all three
  formats, spacing model included. It needs the owner's sign-off and a note
  to EIT, whose templates without a full `[design]` change too.
- **D14. Package version.** `@writer/doc` 0.5.0 is R12's (T97). The
  defaults need 0.6.0, so templates bump twice. The alternative is to fold
  them into 0.5.0 before any writer release ships 0.5.0 (main's 0.4.0 is
  itself unreleased; v0.3.1 embeds 0.3.0): templates bump once, but R12
  waits. If D14 folds, T107 is dropped.
- **D15. No frozen old package.** writer embeds one package version
  (src/typst/package.rs:187-202, `embedded_dir`; :204-206,
  `is_writer_doc`), and R12's T97 rejects a second one. Proposed: keep it
  so, since neither 0.4.0 nor 0.5.0 was in a release and only out-of-tree
  templates such as EIT's are affected. The alternative is a frozen 0.5.0
  for one release that receives only the user-declared design and warns
  on import; it needs a second `embedded_dir` entry.
- **The package API** (`writer-doc`, `design-tokens`, `style-tokens`,
  rules after `show`) stays.
- **Existing themes.**
  - A theme without `[design]` changes look: it gets writer's defaults and
    R12's model in every format. The default theme is the first case.
  - A theme with `[design]` that relied on an engine fallback gets
    writer's default for that key. For example, an unset `justify` stops
    justifying the PDF.
  - Theme CSS keeps winning over built-in values (D16).
- **EIT templates.** R12's 0.5.0 obsoletes `spaced-doc`, the report's
  cover maths and letter.css:48-53 (§1.4); remove them when the templates
  move to 0.5.0. Each remaining workaround becomes double compensation when
  the matching R13 task lands, and goes in the same writer release: the
  report's contents and caption rules (T109, T110), its list and table
  rules and pdf-template items (T112, T114), and heading 4–6 spacing and
  justification (T108, T109; keep them as
  `[design.styles."Heading 4"]` if the report wants another look).

## 5. Exceptions that cannot be single-sourced

This list becomes design.md §0.4. Each item is a value a backend owns, or
a residual the model names.

1. **Footnotes in HTML.** A screen has no pages, and Chrome does not
   support CSS GCPM `float: footnote`, so HTML keeps endnotes. Size, gap
   and marker style are still shared (M12).
2. **Dark mode.** HTML only. The design defines the light palette; a later
   `[design.colors.dark]` (open point 2) could feed the dark block.
3. **Identical line and page breaks.** Chrome, Typst and Word break lines
   and pages with different algorithms. The single source fixes the
   inputs, not the break points, so justified text and page ends differ.
4. **Fonts in Word.** Word renders only installed or embedded fonts.
   Embedding is planned (M7, open point 1), so the lasting exception is
   narrower: fonts whose licence forbids embedding, CFF-only families, and
   builds with embedding off.
5. **KaTeX.** Under the default `[math] html = "katex"`, HTML keeps
   KaTeX's own fonts and its 1.21em size. Only this mode is an exception.
   writer has two other HTML math modes (config.md:66-72):
   - `html = "mathml"` sets math-core's MathML in an OpenType MATH font,
     Latin Modern Math today, inlined as woff2 when `mathml_font = "embed"`
     (src/html/document.rs:415-433). Following `fonts.math` means
     embedding the declared font there instead, at 1em.
   - `html = "image"` compiles each equation to SVG with Typst
     (src/html/math.rs:129-150, src/typst/render.rs:55-73). It follows the
     PDF once two things change: the render prelude (render.rs:13-16),
     which sets no font or size today, takes the design's math font and
     size; and the 12/11 screen scale (math.rs:143-146) does not apply in
     print.
6. **Word runs** have no padding or radius, so inline code and callout
   corners differ. (A highlighted code line is not part of this any more:
   one paragraph per code line gives Word a full-width bar, T113.)
7. **A nested quote's second border in Word.** One paragraph border per
   paragraph: the indent step can match, the second rule cannot.
8. **Overline in Word.** No run property exists.
9. **Per-line overrides in HTML margin boxes.** One string and one font
   per box (row 5.37); per-line weights need `print = "fixed"`.
10. **Screen has no pages.** No page numbers, page margins, furniture
    positions or contents page numbers. Chrome print also lacks
    `target-counter()`.
11. **HTML line edges.** CSS centres the font in the line box; Word and the
    PDF hang it from the ascent (design.md §0.3). HTML's baselines sit a
    constant c per style off in their boxes, larger for exact or extreme
    pitches (§3.2). A paint-time shift removes it in screen layout; D18
    decides whether writer generates it. Printed HTML keeps its 0.75 pt
    grid either way.
12. **Auto-width tables.** CSS automatic table layout, Typst auto columns
    and Word autofit are three algorithms. Tables whose source gives column
    widths already agree (§2.6).
13. **Frames and text wrap.** Word frames (design.md §8.6) have no Typst
    equivalent, because Typst does not wrap text around floats.
14. **Right-to-left text.** Direction can be single-sourced from `lang`.
    Each engine's bidi details, such as mirrored lists and punctuation,
    remain its own.
15. **Word half points**, only if D19 is rejected: up to 0.25 pt per font
    size, once docx/design.rs:126 uses `half_points` (up to 0.5 pt today).
16. **Syntax tokenisation, a grammar-release residual.** HTML and Word
    tokenise with one SyntaxSet, two_face 0.5.2's grammars
    (src/html/highlight.rs:16-17, imported by src/docx/body.rs:11), so
    their tokens are identical. Typst 0.15.1 uses its own copy, two_face
    0.4.5's no-newline grammars (typst-library text/raw.rs:1063-1064), and
    gets the fence's language name without writer's aliases
    (src/typst/emit.rs `raw_block` :723-747, which passes the name
    through, against the aliases at src/html/highlight.rs:46-52). The
    colours differ more than the tokens: Word maps scopes onto about 30
    Pandoc-style token styles (docx/body.rs:25-63, `SCOPE_TOKENS`;
    styles.xml:55-85), and each engine has its own theme. One .tmTheme
    for Typst's `raw(theme:)`, the CSS classes and Word's run colours,
    plus a new alias map in the Typst emitter, close most of the gap
    (T113); one two_face release for writer and Typst closes the rest.

The draft had twenty items. These left the list, because each has a fix:
- **Page background in Word** (draft 3): a picture behind the text (M8,
  T116).
- **The PDF bibliography's label column** (draft 10): Typst's grid is
  hard-wired (typst-layout src/rules.rs:465-494: auto columns, a 0.65em
  gutter at :453, laid out without a grid show step, so no set rule
  reaches it).
  writer already renders bibliographies for HTML and Word
  (src/cite/render.rs); rendering the PDF's as Typst content instead of
  `#bibliography(` (typst/emit.rs:952-977), with citation links kept,
  fixes it (T115). `#cite(<key>)` (emit.rs:1366, in `citation` :1324)
  needs `#bibliography` to resolve, so citations become links to labelled
  entries, and the `--to typst` export (typst-dialect §10) changes with
  them.
- **Spacing around Word tables** (draft 11): the separator paragraph and
  direct spacing (M10, T114).
- **Semibold in Word** (draft 14): design.md open point 15 recommends
  choosing the face from `font` plus `weight` by looking the family up in
  the declared font files (T117). The residual is a family whose files
  writer cannot read.
- **Word line edges** (draft 15) became HTML line edges (item 11): under
  R12 Word is the reference.

## 6. Phased plan

### 6.1 Rules for the tasks

- Every task starts after R12's T96–T101 have merged, and depends on T101
  directly or through another task. The files R12 owns (the "Files you
  own" of docs/tasks/T96–T101: among them src/docx/lines.rs,
  src/typst/world.rs, src/pipeline/typst.rs, the package,
  src/html/design.rs, src/docx/{body,columns,design,styles}.rs, their
  tests and snapshots, and the guide pages) are owned here only by tasks
  that depend on the R12 task that owns them.
- No file is owned by two tasks of the same wave. The domain tasks
  (T108–T118) all edit theme.css, lib.typ, styles.xml and shared
  snapshots, so they run one per wave, and T119 (src/html/design.rs,
  print.css) runs after them.
- Line numbers in "Owns" are main's at `5890724f`. Where R12 changes a
  file, the task names the function or rule instead; the coordinator
  re-cites them when R12 merges.
- design.md belongs to the coordinator, as in R12. Each "Before hand-off"
  line says what the coordinator prototypes, measures and edits first.
- Acceptance uses the harness (T102) and §0.2's tolerances: Word ↔ PDF
  within 0.3 pt; HTML screen layout within 0.3 pt after c; printed HTML
  positions within 0.5 pt; Word XML exact. Word rendering is the owner's
  (T103).
- Sizes: S about half a day, M about a day, L two to three days.
- Milestone: M9 ("One set of defaults"), the next free one; R12's
  T96–T101 stay in M8.
- scripts/waves.py matches only two-character IDs (`\bT[0-9A-F]{2}\b`,
  :26-29), so it cannot parse T100 onward. The table below was computed
  from the task files' "Depends on" rows by the stack's check script;
  widening waves.py's pattern is the coordinator's, before T102.
- §6.2 summarises each task; the task files in docs/tasks are
  authoritative for owned files, steps and acceptance.

| wave | tasks | parallelism |
|---|---|---|
| 26 | T102, T103, T104, T105, T107 | 5 |
| 27 | T106 | 1 |
| 28 | T108 | 1 |
| 29 | T109, T120 | 2 |
| 30 | T110 | 1 |
| 31 | T111 | 1 |
| 32 | T112 | 1 |
| 33 | T113 | 1 |
| 34 | T114 | 1 |
| 35 | T115 | 1 |
| 36 | T116 | 1 |
| 37 | T117 | 1 |
| 38 | T118 | 1 |
| 39 | T119 | 1 |
| 40 | T121, T122 | 2 |

### 6.2 Tasks

**T102. Cross-format measurement harness.** M. Depends on T101.
- **Owns:** scripts/xfmt/ (the DevTools-protocol print and layout readers,
  the Ghostscript baseline reader, a runner), tests/xfmt/ (reference
  documents and expected tables), docs/dev/xfmt-harness.md.
- **Does:** builds each reference document in all three formats, reads
  PDF baselines, HTML screen-layout baselines (print media emulated) and
  printed-HTML positions, and prints one table per document with the
  tolerances of §0.2. It automates R12 §9's commands.
- **Accept:**
  - It reproduces R12 §6's and §8's PDF and HTML columns within 0.05 pt,
    and §3.2's "plain CSS" row.
  - Printed-HTML baselines, measured from the top of their page, all lie
    on the 0.75 pt grid within 0.01 pt (Ghostscript `txtwrite -r7200`).
  - It never uses Chrome's one-shot `--dump-dom`, `--print-to-pdf` or
    `--screenshot`, which hang on the owner's machine, and leaves no Chrome
    process behind: Chrome runs with its own `xfmt-chrome` profile, and
    `pgrep -fl xfmt-chrome` prints nothing after a run.
  - A full run takes under two minutes.
- **Before hand-off:** the coordinator commits R13's probe tools as
  reference code, with `baselines.py` switched from `-r720` to `-r7200`,
  runs them on R12's p3 build, and records the expected tables and the
  number of result lines.

**T103. Word record, the remainder (run by the owner).** M. Depends on
T101.
- **Owns:** docs/dev/docx-validation.md (a new dated record),
  docs/dev/word-record/ (the DOCX inputs and the readings).
- **Measures** in Word for Mac what R12 §2 did not:
  - where `atLeast` lines place their extra;
  - the widow-control default with no `w:widowControl`;
  - s and A for Calibri, Calibri Light, Consolas and Cambria Math;
  - lists (tight and loose items, nested lists, the last item), block
    quotes, code (one paragraph per line), tables (stroke, the 0.7 pt
    separator paragraph), footnotes, captions and equations.
- **Accept:** every item has a measured value, with the Word version and
  zoom recorded, next to R13's prediction (u). Differences over 0.3 pt are
  flagged for the task that uses them.
- **Before hand-off:** the coordinator builds the DOCX inputs with main's
  release binary after T101, writes the predicted value for each reading,
  and prepares the reading sheet.

**T104. HTML paragraph-rule fix.** S. Depends on T101.
- **Owns:** src/html/design.rs (the paragraph part of `layout_css`),
  tests/html_design.rs (the existing assertion of the rule at :57, and new
  tests), and the snapshots whose CSS contains the rule (at main only
  html_design__letter_theme.snap).
- **Does:** `text-align` moves to `[data-writer-document]`; the paragraph
  margin goes under `:where()`, with last-child resets for quotes,
  abstracts, cells and callouts.
- **Accept** (probe *center*): the subtitle, the date and `.center > p`
  are centred; the last children have margin 0; `.date` keeps its 1em top
  margin; `justify = true` still justifies body paragraphs; R12's p3 HTML
  positions are unchanged within 0.05 pt.
- **Keep:** R12's T99 vocabulary rules still come after this rule in
  `layout_css`, and none has lower specificity than its `:where()` margin
  rule. T99 makes `p.subtitle`,
  `p.author` and `p.date` take Title's alignment partly because of this
  rule's `text-align`; that stays unless T99's tests show it redundant.
  The seven T99 tests at the end of tests/html_design.rs stay green.
- **Before hand-off:** the coordinator prototypes the CSS in probe
  *center* and records the computed styles before and after.

**T105. Built-in defaults layer: contract first, behind a switch.** L.
Depends on T101, and D13 for the contract.
- **Owns:** src/design/** (new `defaults/`, merge.rs, mod.rs, parse.rs,
  resolve.rs, json.rs, template.toml), src/pipeline/mod.rs
  (`with_design_layers`), src/check/design.rs, src/check/targets.rs,
  src/cli/mod.rs and src/cli/dump.rs (a `writer dump --design --defaults`
  flag), docs/spec/cli.md (that flag's paragraph), tests/design_config.rs,
  tests/design_defaults.rs (new).
- **Does:** the defaults files with every key and §4.2's additions;
  layer 0 with `Origin::Builtin`; em lengths in the parser; the
  `user_declared` flag and `Design::view(APPLIED)` with `APPLIED` empty,
  one bit per key group; a way for HTML to tell built-in values from the
  user's (a pair of views or an origin per key, §4.1); the assignment of
  §1.4's places to the two flags.
- **Accept:**
  - All 628 snapshots unchanged.
  - A property test: `view(NONE)` equals the design resolved without
    layer 0, `declared` included, for every fixture design.
  - The dump lists every key, with origin `builtin` where no layer sets
    it.
  - Parse tests cover em lengths in styles and at the top level.
  - Each of §1.4's places is assigned to one flag in a comment or test.
- **Before hand-off:** the coordinator writes the design.md edits of
  §4.1, prototypes `view` and runs the full suite with `APPLIED` empty
  (counting result lines), and has D13 signed.

**T106. Bundled line metrics and a missing-family warning.** M. Depends
on T103 (Calibri's A) and T105.
- **Owns:** src/docx/lines.rs, src/design/metrics.toml (new),
  src/design/metrics.rs (new; one `mod metrics;` line in T105's
  src/design/mod.rs), src/check/design.rs (the warning), the lines unit
  tests, tests/design_metrics.rs (new).
- **Does:** lines.rs and the `lines` input take s and A from the table
  before reading font files; one warning per family it cannot resolve.
  The table also holds the hhea and OS/2 typo and win ascent and descent,
  for T119's c.
- **Accept:** probe (h) writes byte-identical DOCX with and without the
  fonts installed (`w:line` 296, not 275); R12 §2.1's values within 0.001;
  exactly one warning per unknown family.
- **Before hand-off:** the coordinator computes s and A with metrics.py
  for the default and common families, checks them against R12 §2.1, and
  prepares probe (h).

**T107. `@writer/doc` 0.6.0, no behaviour change.** S. Depends on T101
and D14 (dropped if D14 folds the defaults into 0.5.0).
- **Owns:** the occurrences of the package version that R12's T97 owned:
  typst.toml, src/typst/mod.rs, src/typst/includes.rs, the template's
  import line, the guide and example imports, the test fixtures, and the
  23 import snapshots.
- **Accept:** only the version changes; every other snapshot is
  byte-identical.
- **Before hand-off:** D14 decided; the coordinator lists every 0.5.0
  occurrence, as for T97.

**T108. Body text, fonts, page and colours: the first switch.** L.
Depends on T102, T104, T105, T106, T107; D3, D4, D5, D7, D8, D13, D16,
D19.
- **Owns:**
  - src/design/defaults/{text,fonts,page,colors}.toml, their `APPLIED`
    bits, and src/design/resolve.rs (D19's rounding);
  - src/html/design.rs (`tokens_css`; built-in rules before the theme,
    D16), src/html/styles.rs;
  - theme.css:1-8, :9-24 and :48-56; print.css:1-4 and :12-31;
  - theme.css:96 (drop `p` from the selector list; T112 keeps the rest);
  - lib.typ's font, size, colour and page setup (`layout`, `writer-doc`)
    and `paragraph-layout`'s no-design branch (R12's T98); design.typ's
    fallback; assets/typst/writer-doc/writer-defaults.json (new);
    src/typst/world.rs;
  - styles.xml docDefaults, Normal, Body Text and First Paragraph;
    src/docx/design.rs (docDefaults, margins :438-441, :126 to
    `half_points`);
  - tests/typst_spacing.rs (`an_undeclared_design_keeps_typsts_layout`,
    re-specified), tests/design_defaults_json.rs and
    tests/pipeline_defaults.rs (new);
  - the snapshots that change, and golden snapshots of
    tests/xfmt/case-a.md.
- **Does:** besides the four domains, removing the package's one gate puts
  every element of an undeclared document under §0.3 in the PDF (§4.1);
  HTML writes the built-in rules before the theme and the user's after it.
- **Accept:**
  - tests/xfmt/case-a.md: pitch, paragraph gap and first baseline agree
    Word (u) ↔ PDF within 0.3 pt (17.6 and 30.25 pt under the proposed D3
    and D4); HTML screen layout agrees within 0.3 pt after c.
  - Margins, text colour and link colour are equal in all three; nothing
    is justified.
  - A test checks that writer-defaults.json equals the built-in design.
  - The default theme stays within 24 KiB.
  - The PDF snapshots of undeclared documents with headings, quotes, code,
    figures, tables, equations or notes change by R12 §7's amounts, and
    the PR lists them.
- **Before hand-off:** the decisions above are made; the coordinator
  prototypes the switch for these four domains, computes the Word (u)
  values, commits probes (a) and (b) as tests/xfmt/case-a.md and
  case-b.md, and measures case (a) with the harness.

**T109. Headings, title block and contents.** M. Depends on T108; D10,
D12.
- **Owns:** the heading, title, abstract and contents styles in
  defaults/styles.toml; the style keys `case`, `letter_spacing`, `indent`,
  `border`, `fill` and `padding` (src/design) and their emission in every
  backend; theme.css:69-80, :112-143 and :266-276; print.css:8 (D10);
  lib.typ's `title-block`, `styled-title-block`, `abstract-block` and
  heading rules; design.typ's `style-text` (case, tracking) and the style
  block (inset, stroke, fill); styles.xml's heading, title and TOC styles;
  src/docx/design.rs (`style_xml`'s run and paragraph properties:
  `w:caps`, `w:spacing`, `w:ind`, `w:pBdr`, `w:shd`); src/html/design.rs
  (the six keys, rules for `.abstract-title` and `.toc-title`);
  snapshots.
- **Accept:** H1–H6 sizes, weights and colours equal in all three; body →
  H1 and H1 → body Word (u) ↔ PDF within 0.3 pt, HTML within 0.3 pt after
  c; the title in the heading font at 26.5 pt everywhere; contents leaders
  and page numbers in the PDF and Word.
- **Before hand-off:** D10 and D12 decided; the coordinator measures the
  heading and title-block gaps with the harness on a prototype.

**T110. Captions, figures and images.** M. Depends on T109, and T120
(both edit src/latex/).
- **Owns:** defaults/image.toml and the caption styles; theme.css:64-67
  and :171-176; lib.typ's figure and caption rules; a shared natural-size
  module (moved from src/docx/images.rs:336-438) and src/docx/images.rs;
  src/html/emit.rs and src/typst/emit.rs image sizes (block and inline,
  :1386-1406 and :1156-1160); src/latex/ image widths; styles.xml Caption,
  Image Caption, Table Caption and Captioned Figure.
- **Does:** Typst writes the smaller of the natural width and the
  region's (`layout` and `calc.min(W, region.width)`), since an explicit
  `image(width:)` does not clamp.
- **Accept:** an untagged 400×200 px image is 300 pt in all three and a
  600×300 px pHYs 300 dpi image is 144 pt; an image wider than the text
  fills the text width in all four formats; caption size, colour and
  alignment equal; PDF table captions left-aligned; body → caption and
  caption → body Word ↔ PDF within 0.3 pt.
- **Before hand-off:** the coordinator moves the size function in a
  prototype and measures caption gaps.

**T111. Links and colour palette.** M. Depends on T110; D11.
- **Owns:** the colour additions in defaults/colors.toml and
  defaults/links.toml; styles.xml's colour literals outside the token
  styles (T113); lib.typ's link, ref and footnote-marker rules;
  callout.typ:24-29; theme.css:14-24 (palette tokens, after T108),
  :87-94 and :292-296; print.css:7 and :20-30.
- **Accept:** no colour literal is left in styles.xml, lib.typ or callout.typ
  outside the coordinator's list; tests/xfmt/case-b.md's muted and accent
  reach all three; links and cross-references are coloured and underlined in
  all three; unresolved references are red in all three.
- **Before hand-off:** D11 decided; the coordinator lists every colour
  literal with its token.

**T112. Lists, quotes, rules and definitions.** M. Depends on T103,
T111; D17.
- **Owns:** defaults/lists.toml and the list-item, Block Text, Definition
  and Horizontal Rule styles; theme.css:96-110; lib.typ's list, enum,
  terms, quote and line rules; src/typst/emit.rs (the nested-list blank
  line, :629-648); numbering.xml and src/docx/numbering.rs (indents from
  the design); styles.xml's new list-item style, Block Text, Definition
  and Horizontal Rule; src/docx/body.rs (the last item's direct after, and
  R12's `space_div` for tight items) and tests/docx_div_spacing.rs
  (`tight_list_items_in_a_div_keep_compact_spacing`); src/html/design.rs
  (list rules); snapshots.
- **Accept:** R12 §8's list and quote rows Word ↔ PDF within 0.3 pt, HTML
  within 0.3 pt after c; indent per level within 0.5 pt; the same bullet
  glyphs; Word XML as M4 and M11.
- **Before hand-off:** D17 decided and T103's list and quote readings in;
  the coordinator prototypes Typst's list spacing and the HTML rules and
  measures R12 §8's rows.

**T113. Code, math, callouts and cells.** L. Depends on T103, T112.
- **Owns:** defaults/code.toml, defaults/callout.toml and `fonts.math`;
  theme.css:178-250; notebook.css; lib.typ's raw and listing rules (one
  variant); callout.typ; cells.typ (spacing only); styles.xml Source Code,
  Verbatim Char, token, Callout, Equation and cell styles;
  src/docx/body.rs (one paragraph per code line, :613-623, :689, :693;
  token styles, `SCOPE_TOKENS` :25-63); assets/typst/writer-doc/
  writer.tmTheme (new, read by HTML and Word through `vendor::get_str`);
  src/html/highlight.rs; src/typst/emit.rs (a new alias map in
  `raw_block`, :723-747); src/html/math.rs, src/typst/render.rs and
  src/html/document.rs (math fonts and sizes per mode).
- **Accept:** R12 §8's code rows Word ↔ PDF within 0.3 pt, HTML within
  0.3 pt after c; callout colours equal; line numbers no longer shrink a
  listing; a highlighted line is a full-width bar in Word; the math font
  follows `fonts.math` in the PDF, Word and the `mathml` and `image`
  modes.
- **Before hand-off:** T103's code and equation readings in; the
  coordinator prototypes the per-line code paragraphs and the shared
  .tmTheme and measures.

**T114. Tables.** M. Depends on T103, T113; D9.
- **Owns:** defaults/table.toml; theme.css:154-169; design.typ's
  `cell-inset` (R12's y inset; the x inset and the stroke join it); the
  table rules it adds to lib.typ's `paragraph-layout` beside R12's
  `empty-cell`; styles.xml's Normal Table (:53, `tblCellMar`) and Table
  (:54) styles; src/docx/tables.rs; src/docx/body.rs
  (`separate_table`, `finish_block_sequence` and direct spacing around
  tables); src/typst/emit.rs (tables); src/html/design.rs (table rules);
  snapshots.
- **Accept:** three rules of 1.5/0.75 pt in all three; R12 §8's table rows
  Word ↔ PDF within 0.3 pt with the stroke counted, HTML within 0.3 pt
  after c; `tblCellMar` holds x only; the space between two tables is
  exact in the XML.
- **Before hand-off:** D9 decided and T103's table readings in; the
  coordinator prototypes the stroke in Typst rows and measures.

**T115. Footnotes and bibliography.** L. Depends on T103, T114.
- **Owns:** defaults/footnotes.toml, Footnote Text and Bibliography;
  theme.css's footnote and `.csl-entry` rules; lib.typ's
  `footnote.entry` rules; design.typ's `spacing-model` (its 0.85 note
  size becomes the design's); src/typst/emit.rs (`bibliography` :952-977
  as content, and `citation` :1324, whose `#cite(<key>)` at :1366 needs a
  target then; also `--to typst`); src/cite/render.rs only if the IR
  needs a label; styles.xml Footnote Text, Footnote Reference and
  Bibliography; the footnotes.xml separator.
- **Accept:** note size, pitch and gap Word ↔ PDF within 0.3 pt; the label
  column and hanging indent equal in all three; citation links kept in the
  PDF.
- **Before hand-off:** the coordinator prototypes the PDF bibliography as
  content, with links, and measures footnote pitch.

**T116. Furniture, page numbers and page background.** M. Depends on
T115; D6.
- **Owns:** the default page-number slot; html/design.rs's furniture
  rules and print.css:1-2; lib.typ's `furniture-background` (its header
  and footer offsets);
  src/docx/furniture.rs; src/docx/design.rs's offsets (:442-443); the Word
  page-background picture.
- **Accept:** page-number baselines Word (u) ↔ PDF within 0.3 pt and
  printed HTML within 0.5 pt; a 12 mm `header_offset` gives the same
  page-2 baseline in Word (u) and the PDF within 0.3 pt;
  `colors.background` fills the page in all three.
- **Before hand-off:** D6 decided; the coordinator prototypes the
  background picture and recomputes rows 5.20–5.22, 5.28 and 5.31.

**T117. Div-class styles, weights and keep rules.** M. Depends on T116.
- **Owns:** the `center`, `left`, `right`, `small` and `large` styles in
  defaults/styles.toml; theme.css:259-263 (removed); src/docx/design.rs
  (the face chosen from `font` plus `weight`, open point 15);
  src/docx/body.rs (R12's `record_div_paragraph` and `space_div`, and
  `ParaProps.keep_next` at :338 and :537: keepNext on every paragraph of
  a `keep_together` div but the last, and on the last only for
  `keep_with_next`, written when the div closes) and
  tests/docx_div_spacing.rs.
- **Accept:** `::: {.center}` is centred and `.small` is 0.85em in all
  three; rows 5.8 and 5.9 behave alike in the XML; `weight = 600` selects
  a semibold face in Word when the family's files are readable.
- **Before hand-off:** the coordinator prototypes the face lookup.

**T118. Language, hyphenation, widows and inline details.** M. Depends
on T103, T117.
- **Owns:** the `hyphenate`, `widows`, `orphans`, `kerning` and
  `ligatures` keys; direction from `lang` (`dir` in HTML, `text(dir:)` in
  Typst, `w:bidi` and `w:rtl` in Word); settings.xml `autoHyphenation`;
  `w:kern`, `w14:ligatures` and `w:widowControl`; the superscript and
  subscript model; highlight and overline; author email and id rendering.
- **Accept:** probe p2's `ar` is right to left in all three; German
  hyphenation is off by default in all three and on with the key
  (`hyphens: auto` in HTML); superscript and subscript sizes are equal.
- **Before hand-off:** T103's widow reading in; the coordinator runs
  probe p2.

**T119. HTML print: forced breaks and line-edge compensation.** S, or M
if D18 compensates. Depends on T118; D18.
- **Owns:** print.css (`.page-break + *`), src/html/design.rs (the
  per-style shift, and the forced-break rule where it depends on c, from
  T106's ascent and descent columns), tests/html_design.rs.
- **Accept:** after a forced break, the first printed baseline equals the
  PDF's plus the first block's c within 0.5 pt (without D18's shift), or
  the PDF's within 0.5 pt (with it). If D18 compensates: the screen-layout
  gaps of the
  harness's reference documents equal Word's model within 0.1 pt.
- **Before hand-off:** D18 decided; the coordinator generates c per style
  from §3.2's probe and checks it on a two-pixel-per-px screen.

**T120. LaTeX consumes the design.** M. Depends on T108.
- **Owns:** assets/latex/writer-template.tex, src/latex/ (except the
  image widths, T110), src/pipeline/latex.rs (`render_detailed` at :34
  and :104 gets the design), src/check/targets.rs (its `render_detailed`
  call, :97), tests/latex_pipeline.rs (the `render` calls) and the
  latex_* snapshots.
- **Accept:** a tests/xfmt case built with and without its `[design]`
  gives two `.tex` files that differ in margins, sizes and colours; a unit
  test on `render_context` shows the template values equal the resolved
  design's (no TeX engine needed); caption labels follow `lang`;
  `hypersetup` carries the author, keywords and subject.
- **Before hand-off:** the coordinator maps each template variable to a
  design key.

**T121. Guard test.** S. Depends on T119, T120.
- **Owns:** tests/no_presentational_literals.rs.
- **Accept:** passes on main, and fails when a colour or length literal is
  added outside the defaults and the allowlist. The allowlist includes
  R12's constants: the 0.8/0.2 exact-pitch split, the Calibri fallback
  for a family no table or file resolves, and the Typst measures R12's
  `spacing-model` and `cell-inset` keep for an unset key (`0.65em`,
  `1.2em`, `0.5em`, `5pt`, and 0.85 if T115 keeps it as a fallback).
- **Before hand-off:** the coordinator lists the literals left after
  T119 and T120.

**T122. Guide: one set of defaults.** S. Depends on T119, T120.
- **Owns:** the docs/guide pages that describe per-backend defaults, and a
  page listing the built-in design and §0.4's exceptions.
- **Accept:** no guide page says a backend keeps its own default for a key
  the design governs; `git grep -n -E '\bT1[0-9]{2}\b' -- docs/guide`
  prints nothing (tests/guide.rs:486 catches T00–T99 and T1A-style IDs
  only).
- **Before hand-off:** the coordinator lists those pages.

### 6.3 The draft's tasks and decisions

The draft planned T96–T114; R12 took T96–T101 meanwhile. This table maps
each draft task to its R13 successor.

| Draft | R13 |
|---|---|
| T96 Cross-format measurement harness | T102 |
| T97 Word behaviour record and doc corrections | T103; the line-height and page-top readings are R12 §2, the doc corrections §1.3 |
| T98 Built-in defaults layer | T105, with the metrics in T106 |
| T99 Body text, page and colours | T108, with the paragraph rule in T104 and the version in T107 |
| T100 Typst edge model and the larger-of rule | dropped: R12's T96–T98 |
| T101 HTML cascade | R12's T99, plus T104 and T119 |
| T102 DOCX spacing | R12's T100, plus parts of T112 and T113 |
| T103 Headings, title block and contents | T109 |
| T104 Captions, figures and images | T110 |
| T105 Links and colour tokens | T111 |
| T106 Tables | T114 |
| T107 Code, math, callouts and cells | T113 |
| T108 Lists, quotes, rules and definitions | T112 |
| T109 Footnotes and bibliography | T115 |
| T110 Furniture and page numbers | T116 |
| T111 Named-style vocabulary and div classes | T117 (the vocabulary itself is R12's T99 and T100) |
| T112 Language, hyphenation, widows and inline details | T118 |
| T113 LaTeX consumes the design | T120 |
| T114 Guard test | T121 |
| – | T122 Guide (new) |

The draft's decisions D1–D10 are D3–D12 here. D13–D19 are new: D13 (the
built-in design applies to every document), D14 (package version), D15
(no frozen package), D16 (built-in CSS before the theme), D17 (a
list-item style), D18 (HTML line edges: stated or compensated) and D19
(font sizes rounded to 0.5 pt).

### 6.4 Open items

- **The coordinator's answer** (2026-11-22, a reply note and edits to
  R12, folded in here): R13, T102 onward, D3 onward and `@writer/doc`
  0.6.0 are free. R12's T98 takes the footnote fix (§3.4, M12) and gives
  display equations the leading between rows. R12 §6 and design.md §0.3
  now allow the per-style shift and leave it to D18. R12 §7 adds the
  table of blocks outside the model (§3.3). The paragraph rule stays with
  R13's T104, which R12 §8 records; T99 does not touch it.
- **R12 is not merged yet.** Its files were uncommitted while R13 was
  written and are now committed on `claude/spacing-model` (`25168b9f`,
  PR #89; T96–T101 done). Where R13 cites R12's sections or values, or
  the files R12's tasks own, the coordinator re-checks them when PR #89
  merges.
- **‡ rows in HTML.** The HTML values of rows changed by R12 were not
  re-measured on R12's build; T102 re-measures them.
- **Calibri's A** is computed from the font tables (about 0.952), not
  measured in Word; T103 measures it with the other families.
- **Chrome's floor rule** (§3.2) was seen only with negative
  half-leading. A style with positive half-leading may round differently;
  T119's hand-off checks it.
- **EIT templates** were not re-read for R13. §4.6's list comes from the
  draft and R12 §1.4; the coordinator checks it before T108.
- **Milestone.** M9, the coordinator's answer of 2026-11-22 (§6.1).
