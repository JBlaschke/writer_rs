# T95 — Guide: one theme for Word and PDF, the PDF template as overrides

| | |
|---|---|
| Milestone | M8 |
| Size | S |
| Depends on | T94 |
| Parallel-safe with | nothing: it documents what T94 installs |
| Files you own | `docs/guide/configuration.md` (the section "Named paragraph styles"), `docs/guide/customization.md` (the vocabulary-style paragraph of "Cross-format design" and the section "Generated Typst PDF template"), `docs/guide/outputs.md` (the `[design]` paragraph of the PDF section), `docs/guide/typst.md` (the section "Shared design in Typst"), `docs/guide/troubleshooting.md` (two added paragraphs) |

## Goal
The user guide says what T94 made true: a `[design.styles]` table named after a Word style
styles the same element in the PDF, a `[pdf] template` is a short list of overrides written
after the `writer-doc` call, and a template reads the theme through `design-tokens` instead of
repeating its values. A reader with a theme tuned in Word learns what changes in their PDF and
how to keep a PDF-only value.

## Read first
- `docs/spec/design.md` §8.7 (the facts; this is your spec) and §6.3's 2026-11-19 note
- `docs/spec/typst-dialect.md` §4.6's 2026-11-19 note (the starter template)
- `assets/typst/writer-doc/template.typ` after T94 (quote it, do not paraphrase its code)
- The five guide passages named under "Steps", each in full before you edit it

## API to implement
None. The guide's code blocks are given under "Steps"; copy them as they are.

## Steps
1. `docs/guide/configuration.md`, section "### Named paragraph styles", second paragraph (it
   starts "A name that matches one of writer's built-in Word styles"). Replace its last sentence,
   "HTML and PDF apply named styles to divs only.", with: "The PDF applies the same names to the
   elements listed below. HTML applies named styles to divs only." Then add, after that
   paragraph, the table and the three paragraphs of "Text for configuration.md" below.
2. `docs/guide/customization.md`, section "## Cross-format design", the paragraph that starts "A
   style named after one of writer's built-in Word styles". Replace its sentence "HTML and PDF
   apply named styles to divs only; their heading look comes from the theme and the Typst
   template." with: "The PDF applies the same styles to its headings, title block, contents
   list, captions, table cells and furniture lines, so one table styles both outputs; see [named
   paragraph styles](configuration.md#named-paragraph-styles) for the list. HTML applies named
   styles to divs only; its heading look comes from the theme's CSS."
3. `docs/guide/customization.md`, section "## Generated Typst PDF template". Keep the `eject`
   command, the `[pdf] template` fence and the last sentence about native Typst documents.
   Replace the paragraph "Edit the exported `writer-doc` function while preserving its expected
   package interface. This template applies to PDF generated from Markdown or notebooks." with
   the text of "Text for customization.md" below (it keeps the second of those sentences).
4. `docs/guide/outputs.md`, PDF section, the paragraph that starts "A shared `[design]` supplies
   PDF font families". Replace its sentence "A custom template receives them when it delegates
   to `writer-doc`; explicit arguments and later Typst rules take precedence." with: "Named
   styles that carry a Word style name, such as `Heading 1`, `Title` or `toc 2`, style the same
   elements here as in Word. A custom template receives all of this when it delegates to
   `writer-doc`; explicit arguments win, and so do Typst rules placed after the `writer-doc`
   call ([PDF template](customization.md#generated-typst-pdf-template))."
5. `docs/guide/typst.md`, section "## Shared design in Typst". Replace the sentence "A custom
   template can inspect them without re-parsing lengths or colors:", the `typst` fence that
   follows it and the paragraph after the fence (it starts "Lengths in that JSON are points")
   with the text of "Text for typst.md" below.
6. `docs/guide/troubleshooting.md`: after the paragraph that starts "Native `.typ` input chooses
   its own template", add the two paragraphs of "Text for troubleshooting.md" below.
7. Run `cargo test --test guide`. Then build the letter example to PDF once, to see that the
   guide's claims hold where a design is declared: `cargo run -- build examples/letter/letter.md
   --to pdf -o /tmp/t95-letter.pdf` must succeed (remove the file afterwards).
8. Check each code block you added against "Hints": the template blocks must be byte-identical
   to the ones given here.

### Text for configuration.md

```markdown
| Element in the PDF | Style names |
|---|---|
| Headings of a level | `Heading 1` to `Heading 6` |
| Headings left out of the contents list | `Heading 1 Unlisted` to `Heading 6 Unlisted`, laid over `Heading N` |
| The title of the contents list | `TOC Heading`, laid over `Heading 1` |
| Contents entries of a level | `toc 1` to `toc 6` |
| Figure and table captions | `Caption` |
| Captions of tables and of images | `Table Caption` and `Image Caption`, laid over `Caption` |
| Table cells | `Compact` |
| The lines of the title block | `Title`, `Subtitle`, `Author`, `Date` |
| Header and footer lines (font and line height) | `Header`, `footer` |

In the PDF, `space_before` and `space_after` of these styles are added to a
full line, as in Word: a heading with `space_before = "20pt"` starts 20 pt
plus one line below the text above it. Where two styled blocks meet, the
larger of the two spaces applies; Word adds them. The first line of the title
block keeps its `space_before` at the top of the page. `Subtitle`, `Author`
and `Date` follow `Title`'s `align`; set their other keys on each style.
A caption uses the style's text keys, `align` and `line_height` only, and a
table cell takes `space_before` and `space_after` as padding above and below
its text.

A font name that ends in a weight, such as `Source Sans 3 SemiBold`, selects
that face in both outputs: Word by the name, the PDF as the family `Source
Sans 3` at weight 600.

These names restyle Word only: `Body Text`, `First Paragraph`, `Block Text`,
`Footnote Text`, `Abstract Title`, `Abstract`, `Bibliography`, `Definition
Term`, `Definition`, `Source Code`, `Figure` and `Captioned Figure`. A design
that declares none of the names in the table leaves the PDF as it was.
```

### Text for customization.md

````markdown
This template applies to PDF generated from Markdown or notebooks. The look
comes from `[design]`; the template holds what the design has no key for, and
overrides for the PDF alone. The exported file is the smallest such template:

```typst
#import "@writer/doc:0.4.0" as base

#let writer-doc(..args, body) = {
    let theme = base.design-tokens(..args)
    show: base.writer-doc.with(..args)

    // Rules written here override the package and the design key by key.
    show heading: set text(fill: rgb("#1a3a8a"))
    body
}
```

The order matters. A `set` or `show` rule written after `show:
base.writer-doc.with(..args)` wins over the design for the properties it
sets and leaves the others alone: the rule above changes the heading colour
and keeps the size, weight and spacing of the theme's `Heading 1`. A rule
written before that line loses to the design.

`theme` is the design as Typst values, so a rule can name a colour or a
margin instead of repeating it: `theme.colors.accent`, `theme.fonts.heading`,
`theme.text.size`, `theme.page.margin.top`. An unset key is `none`.
`base.style-tokens(theme, "Title")` returns one named style; its `text` field
holds the arguments that give `text(..)` the style's font, size, weight and
colour.

`title-block` replaces the built-in title block: `none` draws nothing, and a
function receives the document's metadata and the theme. This template draws
a cover page from the theme's `Title` and `Subtitle` styles, gives tables a
header row in the accent colour and sets second-level headings in italics in
the PDF only:

```typst
#import "@writer/doc:0.4.0" as base

#let cover(meta, theme) = {
    let face(name) = base.style-tokens(theme, name).text
    v(60mm)
    if meta.title != none { text(..face("Title"), meta.title) }
    if meta.subtitle != none {
        v(6mm)
        text(..face("Subtitle"), meta.subtitle)
    }
    pagebreak(weak: true)
}

#let writer-doc(..args, body) = {
    let theme = base.design-tokens(..args)
    show: base.writer-doc.with(..args, title-block: cover)

    set table(fill: (x, y) => if y == 0 { theme.colors.accent })
    show table.cell.where(y: 0): set text(fill: white, weight: "bold")
    show heading.where(level: 2): set text(style: "italic")
    body
}
```

If your theme was tuned in Word and an older template carried the PDF's look,
three things follow the theme once it declares the styles: the space around
headings and contents entries, the text and padding of table cells (`Compact`)
and the line height of header and footer lines (`Header`, `footer`). To keep a
value for the PDF alone, write the rule after the `writer-doc` line, for
example `show heading.where(level: 1): set block(above: 2em, below: 1em)`.
````

### Text for typst.md

````markdown
A template or a native document reads them as Typst values through the
package:

```typst
#import "@writer/doc:0.4.0": *

#let theme = design-tokens()
#let title = style-tokens(theme, "Title")
```

`theme.colors.accent` is a colour, `theme.text.size` and
`theme.page.margin.top` are lengths, `theme.fonts.heading` is a family and
`theme.styles` holds the named styles; an unset key is `none`.
`style-tokens(theme, name)` returns one named style, with every key unset
when the design does not declare it, and its `text` field holds the arguments
for `text(..)`. A template that passes its arguments on writes
`base.design-tokens(..args)`, which also sees an exported source's `design:`
argument.

A template that delegates to `writer-doc` receives the design defaults and
the named styles of [the shared vocabulary](configuration.md#named-paragraph-styles).
Explicit `writer-doc` arguments take precedence, and so do set and show rules
written after `show: base.writer-doc.with(..args)`; see [the PDF
template](customization.md#generated-typst-pdf-template). `writer-doc` takes
`title-block: none` to leave the title block out and a function `(meta, theme)
=> content` to draw another one.

A template that does not import the package can read the raw values, where
lengths are points, colors are lowercase hex strings and unset keys are
`none`:

```typst
#let design = if "writer-design" in sys.inputs {
    json(bytes(sys.inputs.at("writer-design")))
} else { (:) }
```

Furniture image paths name registered virtual assets. A fully independent
generated-PDF template keeps its own appearance, and `check` reports
`design-shadowed` when it does not import the bundled document package.
Native Typst keeps its original styling authority; design checks on native
inputs are limited to missing resources.
````

### Text for troubleshooting.md

```markdown
A rule in a PDF template that no longer changes a heading, a caption or the
contents list is written before the `writer-doc` call, where the design's
named styles win. Move it after `show: base.writer-doc.with(..args)`; see
[the PDF template](customization.md#generated-typst-pdf-template).

A template that stops compiling after an upgrade with "package found, but
version ... does not exist (latest is ...)" names an older `@writer/doc` in
its `#import` line. Change the version to the latest one the message gives.
```

## Acceptance criteria
- `cargo test --test guide` passes (links and anchors resolve, the generated blocks match, no
  internal identifier appears).
- `git grep -n 'HTML and PDF apply named styles' -- docs/guide` prints nothing, and the two
  passages (`configuration.md` "Named paragraph styles", `customization.md` "Cross-format
  design") say that HTML applies named styles to divs only and that the PDF applies the names of
  the table.
- `git grep -n 'design-tokens' -- docs/guide` finds `customization.md` and `typst.md`;
  `git grep -n 'title-block' -- docs/guide` finds the same two files.
- Both `typst` fences added to `customization.md` and the first one added to `typst.md` contain
  `@writer/doc:0.4.0`, and `git grep -n '@writer/doc:0\.3' -- docs/guide` prints nothing.
- The first template block of "Text for customization.md" equals `assets/typst/writer-doc/
  template.typ` from its `#import` line to its end (`diff` them: the guide block is the file
  without its leading comment).
- Manual check: `cargo run -- eject typst-template /tmp/t95-template.typ` writes a file whose
  code is the block the guide shows (remove it afterwards).
- `git diff --stat` lists only the five guide files.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- `docs/spec/**`, `docs/research/**`, `PLAN.md`, `docs/tasks/**`: the coordinator's.
- Any file under `assets/`, `src/`, `tests/` or `examples/`. If the guide's text and the
  package's behaviour disagree, report it; do not change either to match.
- `docs/guide/cli.md` (its `--help` fences are generated and do not change) and
  `docs/guide/getting-started.md`.
- Release notes and `CHANGELOG.md`.
- New claims: write what is given here and in design.md §8.7. Do not describe HTML as applying
  these styles, and do not promise identical spacing in Word and PDF beyond what the text says.

## Hints (verified)
- `tests/guide.rs` rejects an internal task identifier in any guide page or example README (the
  pattern is a capital T followed by two digits, or by a digit and a capital letter), so the
  guide never names this task, T94 or a milestone. It also checks that every link target and
  anchor exists: `configuration.md#named-paragraph-styles`,
  `customization.md#generated-typst-pdf-template` and `customization.md#cross-format-design` are
  the anchors of existing headings.
- `tests/guide.rs` (`design_reference_covers_the_serialized_schema_shapes`) reads every table
  row between the headings "## `[design]`" and "## Strictness" whose first cell is one name in
  backticks as a design key, and "### Named paragraph styles" lies in that range. That is why the
  style table of step 1 names the PDF element first, in plain words, and the style names second:
  a row that starts with `` | `Caption` | `` fails that test with an unknown key (found when the
  task ran, 2026-11-20). Keep the column order.
- The cover template of "Text for customization.md" was compiled with Typst 0.15.1 against the
  reference package on 2026-11-20, with a design (accent `#a96d4b`, a `Title` style of 30pt) and
  without one: the cover takes the `Title` style, the table's first row is filled with the
  accent, second-level headings are italic and keep the theme's other values, and without a
  design every token is `none` and the document still compiles.
- The starter template's rule order was measured the same day: with `Heading 1` declared as 14pt
  and `#0a2f41`, the ejected template gives a 14pt heading filled `#1a3a8a`.
- The numbers behind "20 pt plus one line" and the list of names are design.md §8.7; the three
  effects on a Word-tuned theme are its paragraph "Absent means unchanged".
- A template that imports an older package fails today with `template.typ:1: error: package
  found, but version 0.2.0 does not exist (latest is 0.3.0)` (writer 0.3.0, 2026-11-20); after
  T93 the message names `0.4.0`.
- The guide wraps prose at about 80 columns; keep the fences' content as given, and use four
  backticks around a block that itself holds a fenced block only in this task file, not in the
  guide (there every block is an ordinary three-backtick fence).
