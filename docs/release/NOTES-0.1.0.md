The first release of writer: write a report in Markdown, a saved Jupyter
notebook or Typst, and build a web page, a typeset PDF, an editable Word file,
LaTeX or Typst source from it.

## Highlights

- **Five outputs from one source.** Standalone or site HTML, PDF from the
  embedded Typst compiler, Word packages with editable equations, LaTeX
  source and resources, and portable Typst source. Native Typst input
  compiles directly for PDF; its other outputs use a documented structured
  subset.
- **Live preview.** `writer serve` rebuilds on every save and shows the web
  page or the typeset pages. A failing edit keeps the last good result and
  reports located errors and warnings.
- **Notebooks as saved.** Plots, DataFrames and tracebacks come from the
  notebook's saved outputs. No kernel runs.
- **References and citations.** Numbered figures, tables and equations,
  cross-references, footnotes, citations and a bibliography with a CSL style.
- **Checks without output.** `writer check` reports frontend, reference and
  target diagnostics without writing files.
- **Customization.** `writer init` scaffolds a document; `writer eject`
  copies the built-in theme, Typst and LaTeX templates or the Word reference
  document for editing.

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
