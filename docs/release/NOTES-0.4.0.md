writer 0.4.0 brings the PDF and the web page much closer to the Word file.
One `[design]` now styles headings, the title block, the contents list,
captions and table cells in all three outputs. With a design, the PDF
builds its lines as Word does, and the spaces around paragraphs and
styled divs, and those that a heading style sets, follow Word's rule.
Markdown and notebook projects with no design (in `writer.toml`, the
theme or front matter) and no Typst of their own look as they did with
0.3.1. In a project with a design, text moves in the PDF and can move on
the web page; in Word it moves only where a div style sets `space_before`
or `space_after`, or where writer computes a multiple `line_height` for a
font whose name ends in a weight. To build a PDF with writer 0.4.0, every
Typst file that imports `@writer/doc` must name version 0.6.0.

## Upgrading from 0.3.1

- **Import `@writer/doc` 0.6.0.** writer embeds one version of its Typst
  package, numbered apart from writer itself. It moves from 0.3.0 to
  0.6.0; versions 0.4.0 and 0.5.0 existed only in development builds.
  Change the `#import "@writer/doc:0.3.0"` line of every `[pdf]
  template`, native `.typ` document and Typst file they include to
  `0.6.0`. An old import stops the PDF build with "package found, but
  version 0.3.0 does not exist (latest is 0.6.0)", unless an ejected copy
  of the old package is still around (next point). `writer check` warns
  about a native document's import but not about a template's, so look
  for the line yourself.
- **Remove ejected 0.3.0 packages.** A 0.3.0 package that `writer eject
  typst-package` wrote into a `[typst] package_paths` directory, or
  installed with `--install`, still satisfies an old import, and writer
  then builds without an error: a native document with the old package
  throughout, and a `[pdf] template` with the old `writer-doc` around the
  new package's body elements, a mix of the two releases. Remove those
  copies, or at least their `writer/doc/0.3.0/` directories. writer
  always uses its embedded 0.6.0 for a 0.6.0 import. Eject 0.6.0 only to
  compile exported Typst sources outside writer, and into a new
  directory: an eject where 0.3.x's `preview/mitex/0.2.7` still lies
  fails with "package version destination already exists", and so does
  `--install` until that `preview/mitex/0.2.7` is removed from Typst's
  data directory.
  Sources that 0.3.1 exported need the 0.3.0 package outside writer:
  export them again, or keep a 0.3.0 copy outside `package_paths` and
  Typst's package directories and pass it with `typst --package-path`.
- **Put template rules after `writer-doc`.** In a `[pdf] template`, a rule
  written after `show: base.writer-doc.with(..args)` wins over the design
  for the properties it sets. A rule written before that line loses to
  every key that a named style applies in the PDF and, under any design,
  to the package's block spacing and the vertical inset of table cells.
  As in 0.3.1, it also loses to the package's leading and paragraph
  spacing and to the font, size and colour it sets for the whole text,
  while a `show heading: set text(..)` rule there still applies where no
  named style sets the key. The template that `writer eject
  typst-template` wrote for 0.3.x calls `base.writer-doc(..args, body)`
  after its rules; rewrite it in this shape:

  ```typst
  #import "@writer/doc:0.6.0" as base

  #let writer-doc(..args, body) = {
      show: base.writer-doc.with(..args)
      show heading: set text(fill: rgb("#1a3a8a"))
      body
  }
  ```

  The new starter template that `writer eject typst-template` writes has
  this shape and the same example rule, which now overrides, in the PDF
  only, the heading colours a design sets: delete it if your design sets
  them.
- **Templates under a design.** With a design, a template that sets
  `par(leading: ..)` or `par(spacing: ..)`, indents first lines, adds a
  `v(..)` for the first block's `space_before` or draws its own cover page
  needs the changes listed under
  [the PDF template](https://jblaschke.github.io/writer_rs/guide/customization.html#generated-typst-pdf-template).
  A `block(above: ..)` smaller than the paragraph spacing now brings a
  block no closer to a paragraph above it than the paragraph spacing.
  The package no longer has `line-box`, `design-leading` and
  `design-spacing`, with or without a design, so a template, native
  document or raw Typst block that calls them fails to build; read the
  design with `design-tokens` and `style-tokens` instead (see below).
- **Check every output of a project with a design.** A design in
  `writer.toml`, the theme or front matter switches the PDF to Word's line
  boxes, even when it sets only colours, a single div style, only keys
  new in 0.4.0 or only an empty table such as `[design.colors]`. A
  `[design]` with nothing under it does not. A non-empty `design:` that a
  template passes to `writer-doc` switches it too, while `design: none`
  there keeps Typst's lines. Named styles that a theme declared for
  Word's headings, contents, captions, table cells and title block now
  reach the PDF and HTML. Text can move to another page. The changes
  below say what moves and, for headings, styled divs and the contents
  list, which keys pin it.

## Changes

- **Named styles in the PDF and HTML.** A `[design.styles.<Name>]` table
  named after one of writer's Word styles now styles the matching element
  in the PDF and HTML as well: `Heading 1` to `Heading 6` and
  `Heading N Unlisted` for headings, `TOC Heading` and `toc 1` to `toc 6`
  for the contents, `Caption`, `Table Caption` and `Image Caption` for the
  text of captions, `Compact` for table cells, and `Title`, `Subtitle`,
  `Author` and `Date` for the title block. A theme tuned in Word therefore
  restyles these elements in the PDF and on the web page. In HTML the
  design's rules come after the theme's CSS and use the selectors of
  writer's markup: a `Heading 1` rule beats a theme's `h1 { ... }` and
  loses to `.report h1`, while a `Heading N Unlisted` rule, written for
  `hN.unlisted`, still beats `.report hN`. In the PDF, `Header` and
  `footer` set the font family and line height of the lines of the
  `page.header` and `page.footer` slots, which keep setting their size,
  colour, weight, italics and alignment. See
  [named paragraph styles](https://jblaschke.github.io/writer_rs/guide/configuration.html#named-paragraph-styles).
- **The title block.** When the design declares `Title`, `Subtitle`,
  `Author` or `Date`, the PDF draws the title block from those styles.
  `Subtitle`, `Author` and `Date` take `Title`'s left, centre or right
  alignment unless they set their own, in the PDF and HTML alike. A
  `space_after` set on the last line of the title block replaces the
  built-in gap below it in the PDF, so a letter's body can move up. A
  `Date` style meant for a letter's `:::date` div now also styles the
  front-matter date in the PDF and HTML, as it already did in Word.
- **Word's lines in the PDF.** With a design, the PDF sets each line in a
  box as tall as the line pitch and places the text in it as Word does.
  Where two blocks meet, the larger of the upper block's `space_after` and
  the lower block's `space_before` lies between them, never their sum; a
  body paragraph counts as no space above and the paragraph spacing
  below. The line pitch stays as it was, and so does the space between
  body paragraphs up to a line height of about 1.86 times the text size
  (20.4 pt at 11 pt Libertinus Serif), given as a multiple or a length;
  above that, without `text.paragraph_spacing`, paragraphs follow one
  another at the line pitch with no space between them, which still puts
  them further apart than before. Text sits a few points lower on the
  page: 2.6 pt for 11 pt Libertinus Serif, writer's default PDF font, at a
  line height of 1.35, and for a line height given as a length, four
  fifths of it less the cap height (2.4 pt at 12 pt, 12 pt at 24 pt). The
  first block of the document keeps its `space_before` at the top of page
  1, unless a `title-block` function or a page break comes before it; a
  block that starts a later page drops it. See
  [body text](https://jblaschke.github.io/writer_rs/guide/configuration.html#body-text).
- **What moves in the PDF.** The values here are for 11 pt Libertinus
  Serif with a line height of 1.35 and a paragraph spacing of 3.6 mm. A
  styled div or declared heading that follows a body paragraph now stands
  at least the paragraph spacing below it, even with a smaller
  `space_before`, while a div's `space_after` alone, however small,
  separates it from the paragraph that follows. A heading the design does
  not declare keeps Typst's own spaces, and so does a declared heading on
  a side where it sets no space; at writer's default heading sizes these
  now stand about 8 to 10 pt further from the text, and more for a
  declared heading with a larger `size`. At the default sizes, such a
  heading that opens page 1 also sits about 19 to 24 pt lower. Contents
  entries stand a paragraph spacing apart unless the `toc N` styles set
  their spaces. A block quote stands about 8 pt further from the text.
  Display equations, code blocks and uncaptioned figures and tables stand
  1 to 5 pt closer; a caption moves away from its image or table instead,
  so a captioned figure or table keeps its distance on the caption's
  side. Code and footnote lines take the design's line height, so the
  footnote area can grow. A list nested in an item of a tight list sits
  right under the item. Header and footer lines are line boxes too: check
  letterheads and footers tuned to an exact height, and
  `page.header_offset` and `page.footer_offset`. Declaring `Heading N`,
  `TOC Heading` and `toc N` with both space keys, together with
  `text.paragraph_spacing`, sets those distances in all three outputs.
- **Paragraphs inside styled divs.** A div style's `space_before` and
  `space_after` surround the whole div in Word, the PDF and HTML, as
  before, and the paragraphs inside keep the body's paragraph spacing, as
  they already did in the PDF and HTML. In Word they used to run
  together: writer now gives them the paragraph spacing as direct
  formatting. Two divs of the same style in a row now stand the larger of
  their spaces apart in Word and HTML, as in the PDF; Word had no space
  there, and HTML at least the paragraph spacing. Write lines that must
  stay tight, such as an address, as one paragraph with line breaks. Styles
  that a reference document defines, and divs styled with one of writer's
  own Word style names, such as `Date`, keep 0.3.1's behaviour in Word:
  each paragraph of the div takes the style and its spaces, while the PDF
  and HTML treat the div as one block. In HTML, where the style sets a
  space, the first or last block inside the div no longer adds its own
  margin, so a `space_after` smaller than the paragraph spacing holds; as
  in the PDF, the paragraph spacing above a div still wins over a smaller
  `space_before`.
- **Paragraphs in HTML.** With `text.paragraph_spacing` or `text.justify`,
  the design's paragraph rule now weighs as much as a plain `p`. The
  theme's class rules now win over it, so the title block, the contents
  title, callout titles and the last paragraph of a quote take the
  theme's spacing instead of the paragraph spacing, and a theme rule more
  specific than `p`, such as `main p`, now beats the design.
  `text.justify` now sets the alignment of the whole document, so the
  title block and `::: center` and `::: right` divs keep their own
  alignment, where 0.3.1 justified their paragraphs or, with
  `justify = false`, set them flush left. With `justify = true`,
  headings, list items, definition lists, bibliography entries and
  contents entries are now justified as in Word, and code blocks are
  aligned to the start, also inside `::: center` and `::: right`.
  Paragraphs in the abstract now take the theme's left alignment, as
  table cells already did.
- **Empty table cells.** With a design, an empty table cell holds one line
  in HTML and the PDF, as in Word, so a row of empty cells no longer
  collapses.
- **Font names with a weight.** A named style's `font` whose last word
  names a weight, such as `Source Sans 3 SemiBold`, now selects the family
  `Source Sans 3` at weight 600 in the PDF, or at the style's own
  `weight` if that is heavier; `Header` and `footer` take only the
  family. 0.3.1 fell back to another font for a div style. The words are
  Thin, Hairline, ExtraLight, UltraLight, Light, Regular, Medium,
  SemiBold, DemiBold, Bold, ExtraBold, UltraBold, Black and Heavy, in any
  case and with or without a hyphen. Word still selects the face by its
  full name, and when writer can read the font it now computes a multiple
  `line_height` from that family's metrics instead of Calibri's, so such
  lines get taller or shorter. In `[design.fonts]`, write the family
  name: the PDF does not split a weight off there and falls back to
  another font, though Word's line height does use the family without the
  weight.
- **Lengths in em.** Any length in `[design]` can be written in `em`. In a
  named style that sets its own `size`, 1em is that size; elsewhere it is
  `text.size`, or 11 pt when the design sets no size. writer turns em into
  points when it reads the design. `text.size` itself needs an absolute
  unit.
- **Typst templates and documents.** `writer-doc` takes a `title-block`
  argument: `none` draws no title block, and a function of the metadata
  and the design draws its own, such as a cover page. It changes the PDF
  only. `design-tokens(..args)` returns the main design keys as Typst
  values (the three font roles, the text size, line height, paragraph
  spacing and justification, the five colours, the page size, margins and
  offsets, and the named styles without `frame` or the new style keys),
  and `style-tokens(theme, name)` picks one named style out of that
  result, so a template no longer decodes `sys.inputs.writer-design` for
  these keys. In a native document, the HTML and Word outputs keep the
  text but not the styling these values give it, as for any styling
  outside the structured subset, and a `#let theme = design-tokens()` line
  draws the warning "only simple literal let bindings are part of the
  structured subset" in those builds and in `writer check`. The raw design
  in `sys.inputs.writer-design`, and a Typst export's `writer-design.json`,
  has a new `lines` table with the line metrics of each text font family
  that the design names, in `[design.fonts]` (except `math`) or in a named
  style, where writer can read it on the building machine;
  `writer dump --design` does not print it. See
  [shared design in Typst](https://jblaschke.github.io/writer_rs/guide/typst.html#shared-design-in-typst).
- **New keys, not applied yet.** writer now accepts and checks new
  `[design]` keys: the style keys `case`, `letter_spacing`, `indent`,
  `border`, `fill` and `padding`, the tables `[design.lists]`,
  `[design.table]`, `[design.code]`, `[design.callout]`, `[design.image]`,
  `[design.footnotes]` and `[design.links]`, `fonts.math`, and more `text`
  and `colors` keys. `writer dump --design --defaults` prints the design
  with writer's built-in values filled in, and where each key comes from.
  Both prepare a built-in design that later releases are meant to apply,
  and its values may still change. In 0.4.0 they change no visible
  output, with three exceptions: the new colours `border`, `code_bg`,
  `highlight` and `error` work as colour names in other keys; every file
  listed under `fonts.math` must exist, is loaded for the PDF and is
  copied into a Typst export, while equations keep New Computer Modern
  Math; and a design that sets only new keys is still a design, as above.
  writer 0.3.1 rejects the new keys and `em` lengths, so a project that
  uses them needs 0.4.0.
- **Unchanged.** Without a design, and once every Typst import names 0.6.0
  and calls nothing the package removed, the HTML, PDF and Word files look
  as they did with 0.3.1, and an exported Typst source differs only in its
  import line. The LaTeX export does not change, with or without a
  design.

## Where the outputs still differ

- **Spacing.** Lists, block quotes, code, figures, tables, equations,
  footnotes, callouts and the abstract keep each output's own spacing,
  and so do headings, title lines and contents entries on a side where
  their style sets no space. A caption's spaces apply in Word only.
  `Author`'s spaces surround each author in Word, but the whole author
  line in the PDF and HTML.
- **Justification.** With `justify = true`, HTML sets the abstract and
  table cells flush left, where Word and the PDF justify them, and the PDF
  can leave the lines of a wrapped heading short of the margin, where Word
  and HTML justify them.
- **Abstract and contents titles.** In the PDF, the abstract's title takes
  `Heading 1` with `Heading 1 Unlisted` over it, and the contents title
  takes the keys of `Heading 1 Unlisted` that `Heading 1` and
  `TOC Heading` leave unset. Word styles the abstract's title with
  `Abstract Title`, and HTML with the theme.
- **Title block.** In Word, `Subtitle`, `Author` and `Date` also inherit
  some of `Title`'s other keys, such as its weight and italics. Set the
  keys on each style.
- **Compact.** `Compact` styles tight list items in Word, but only table
  cells in the PDF and HTML.
- **Header and footer.** In Word, the lines of a `page.header` or
  `page.footer` slot take the slot's size, weight, italics and alignment,
  the slot's `color` or else `colors.text` (`colors.muted` for the
  footer) where one is set, and `fonts.body` where it is set, so only the
  line height and spaces of `Header` and `footer` always apply there.
  Without a `page.footer` slot, the page number that Word writes takes
  every key of `footer`, and the PDF's page number none. HTML ignores
  both styles.
- **Captions.** In the PDF, a caption's `align` places it within the width
  of its figure or table, and in HTML and Word within the column. In
  Word, writer's own `Image Caption` style centres image captions
  whatever `Caption` sets, so set `align` on `Image Caption` as well.
- **Centred divs.** Only HTML centres or right-aligns `::: center` and
  `::: right` divs, with or without `justify`: they are classes of the
  default theme, and the PDF and Word set them as body text unless the
  design declares `center` and `right` styles with an `align`, which then
  apply in all three outputs.
- **Font names in HTML.** HTML writes a named style's `font` that ends in
  a weight as given, with no fallback, so a browser that has no face of
  that full name shows its default font.

## Downloads

One archive per platform: macOS (Apple silicon and Intel), Linux (x86_64 and
arm64, glibc) and Windows (x64 and arm64). Each holds the `writer` binary,
the MIT license and the notices of every embedded asset and linked crate.
Check a download against `SHA256SUMS`, unpack it and put `writer` on your
`PATH`.

The binaries are not signed or notarized. On macOS, remove the quarantine
mark of a browser download with `xattr -d com.apple.quarantine writer`.

The web page, PDF and Word outputs need nothing else at run time. Compiling
the LaTeX export needs a separate TeX installation. Chrome-based PDF is not
supported yet; the embedded Typst path needs no browser.

Read the [user guide](https://jblaschke.github.io/writer_rs/guide/) or start
with the [examples](https://jblaschke.github.io/writer_rs/#examples).
