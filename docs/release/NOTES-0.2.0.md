writer 0.2.0 can set the body of a Word document in two or more columns,
like a journal article, with `columns` in `[design.page]`. Documents that do
not set the new key look as they did with 0.1.2, and projects need no
changes.

## Changes

- **Columns in Word.** `page.columns` takes a column count, or a table with
  the count, the gap between the columns and where they start:

  ```toml
  [design.page]
  columns = { count = 2, gap = "7mm", start = "after-rule" }
  ```

  `count` runs from 1 to 16, and `columns = 2` is short for
  `{ count = 2 }`. `gap` defaults to Word's 0.5 in. With
  `start = "body"`, the default, the columns start after the title block.
  With `"after-rule"` they start after the body's first top-level `---`,
  so the blocks above it form a full-width lead section; without a `---`
  they start at the first body block. See
  [columns in Word](https://jblaschke.github.io/writer_rs/guide/configuration.html#columns-in-word).
- **Full-width title, lead and figures.** The title block, the lead section
  and every top-level figure, captioned tables included, run across the
  full text width. The parts are joined by continuous section breaks, and
  Word balances the columns above each figure, so the text reads down the
  left column and then the right before it. Images, uncaptioned tables
  and equation images in a column are sized to the column width, as are
  those in a footnote referenced from a column.
- **Headers, footers and footnotes run on.** Every section has the same
  headers and footers. Only the first section asks for a first-page header
  and footer (`w:titlePg`), so they appear on page one only, even where a
  figure moves to the top of a later page. Footnote numbers continue
  across the sections instead of restarting.
- **Word only.** The HTML and PDF outputs do not apply `page.columns`.
  Without the key the Word file is byte for byte the one 0.1.2 writes, and
  `columns = 1` keeps one column. A reference document's page setup still
  takes precedence, its own columns included. The resolved design that a
  theme's HTML templates (`theme.design`), a Typst template
  (`sys.inputs.writer-design`) and a Typst export's `writer-design.json`
  read now has a `page.columns` entry, `null` when unset, so a template
  can set its own columns from it. The bundled Typst package stays at
  `@writer/doc` 0.3.0. writer 0.1.2 rejects the key with
  `design.page.columns: unknown key`, so a project that sets it needs
  0.2.0.

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
