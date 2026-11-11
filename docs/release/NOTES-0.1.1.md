writer 0.1.1 fixes the PDF's line and paragraph spacing for projects with a
`[design]`, and a Typst export now carries that design with it.

## Changes

- **PDF spacing matches the web page and the Word file.** With a design, the
  PDF set its lines and its paragraphs closer together than the other two
  outputs did. Lines now sit at the design's `text.line_height` and a
  paragraph break adds exactly `text.paragraph_spacing`, in all three
  formats: in the
  [letter example](https://jblaschke.github.io/writer_rs/examples/letter/),
  body lines are 14.85 pt apart (1.35 × 11 pt) and a paragraph break adds
  10.2 pt (3.6 mm). Named styles and header and footer lines follow the same
  rule at their own size. HTML and Word output are unchanged, and a Typst
  template that sets `par(leading:)` itself still wins.
- **A Typst export carries the design.** `writer build document.md --to
  typst` writes the resolved design as `document_assets/writer-design.json`,
  copies the header, footer and background images next to the other assets
  and the declared font files into `document_assets/fonts/`. The exported
  tree compiles with the plain Typst command line and no compiler inputs:

  ```sh
  writer eject typst-package typst-packages
  typst compile --font-path dist/document_assets/fonts --package-path typst-packages dist/document.typ
  ```

  An export without a declared design is unchanged.
- **`@writer/doc` 0.3.0.** The bundled Typst package has a `design:`
  argument and a new version. Typst sources and templates that import
  `@writer/doc:0.2.0` need to import `0.3.0`, and an ejected copy of the
  package needs ejecting again.
- **Vendored dependencies.** The source tree includes its Rust dependencies
  under `vendor/`, and Cargo builds from them.

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
