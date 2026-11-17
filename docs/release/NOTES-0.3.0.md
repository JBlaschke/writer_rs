writer 0.3.0 can float a div style's paragraphs in a Word frame, for
example a badge beside the title, with `frame` in `[design.styles.<Name>]`.
Documents that do not set the new key look as they did with 0.2.0, and
projects need no changes.

## Changes

- **Frames in Word.** A named div style's `frame` floats the div's
  paragraphs in a Word frame, and the text beside the frame wraps around
  it:

  ```toml
  [design.styles.Repository]
  size = "8pt"
  align = "center"
  frame = { width = "16mm", align = "right", gap = "6mm", anchor = "title", offset = "-8mm" }
  ```

  `width` sets the frame's width; unset, Word fits the frame to its
  contents. `align` places it between the page margins: `left`, the
  default, `center` or `right`. `gap` is the space between the frame and
  the text around it, and `offset` moves the frame down from where the div
  is, or up with a negative length. See
  [frames in Word](https://jblaschke.github.io/writer_rs/guide/configuration.html#frames-in-word).
- **Beside the title.** With `anchor = "title"`, each top-level div with
  the style moves to just after the title paragraph, ahead of the
  subtitle, authors and date, and a negative `offset` lifts it beside the
  title, which wraps around it. The offset that lines the frame up with
  the title's last line depends on the frame's height, so check it in
  Word. Footnotes in a moved div are numbered first, in document order,
  and with `page.columns` the div stays in the full-width title section.
  Nested divs, and divs in a document without a title, keep their place.
  The default, `anchor = "div"`, leaves the frame where the div is.
- **What Word frames.** Every paragraph of the div that takes the style is
  framed, including list items, display math and the body of a figure
  that is not an image, table or code listing. Tables, image figures,
  headings and code blocks inside the div keep their own styles and stay
  in the text flow.
- **Word only.** The HTML and PDF outputs ignore `frame`; a theme places
  such a div with its own CSS or Typst template. The resolved design that
  templates and a Typst export's `writer-design.json` read now has a
  `frame` entry on every style, `null` when unset. Only div styles can
  have a frame: `Title`, `Body Text`, `Header` and writer's other
  paragraph styles are an error. A reference document that defines a style
  of the same name wins, so writer adds no frame and the div keeps its
  place. The bundled Typst package stays at `@writer/doc` 0.3.0. writer
  0.2.0 rejects the key with `design.styles.<Name>.frame: unknown key`, so
  a project that sets it needs 0.3.0.

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
