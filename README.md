# writer

Write a report in Markdown, a saved Jupyter notebook or Typst, then build HTML,
PDF, Word (`.docx`), LaTeX or Typst source. Preview changes in the browser while
you write. Figures, tables, equations, references and bibliographies travel
with the document.

From a checkout, with Rust 1.96 or newer:

```sh
cargo install --path . --locked
writer build examples/basic/document.md
writer serve examples/basic/document.md --open
```

The build writes `examples/basic/dist/document.html`. The server watches the
source, included files, images and theme; its web and typeset-page views keep
the previous good result when an edit fails. Stop it with Ctrl-C.

Choose several outputs with one command:

```sh
writer build examples/basic/document.md --to html,pdf,docx,latex,typst
```

PDF uses the embedded Typst compiler. Word equations are editable where the
math conversion supports them, with an image fallback. LaTeX export writes
source and resources for a separate TeX installation. Notebook builds read
saved outputs and never execute the notebook.

For native Typst input, PDF compiles the original source. The other formats
use a documented structured subset, so full Typst programs can have different
results in those outputs. Markdown and notebook inputs default to HTML;
Typst inputs default to PDF.

Read the [user guide](docs/guide/README.md) for authoring, output choices,
configuration and customization. Start with an example:

| Example | Demonstrates |
|---|---|
| [Basic report](examples/basic/README.md) | Headings, figures, tables, code, equations and footnotes |
| [Saved notebook](examples/notebook/README.md) | A plot, DataFrame and saved traceback |
| [Several source files](examples/mixed/README.md) | Nested Markdown and notebook includes |
| [Native Typst](examples/typst/README.md) | Native PDF with structured HTML and Word outputs |
| [LaTeX template](examples/latex-template/README.md) | Custom source template and separate latexmk compilation |

The built-in theme, math assets, fonts and document package are embedded.
External Typst package downloads require an explicit opt-in. Custom content
can still refer to external resources; use local resources for a portable
document.

writer is the document sibling of
[presenter_rs](https://github.com/JBlaschke/presenter_rs). Contributors can
start with the [architecture and development plan](PLAN.md),
[format contracts](docs/spec/) and
[build measurements](docs/dev/build-times.md).
