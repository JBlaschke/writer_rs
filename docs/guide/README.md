# Writer user guide

Writer turns a Markdown document, a saved Jupyter notebook or a Typst source
file into shareable documents. Its browser preview follows edits to the
document and its resources.

| Page | Covers |
|---|---|
| [Getting started](getting-started.md) | Install, create a document, preview it and build outputs |
| [Authoring](authoring.md) | Document structure, figures, tables, math, references, citations and includes |
| [Notebooks](notebooks.md) | Saved results, cell options, tags and output selection |
| [Typst sources](typst.md) | Native PDF, the structured subset and mixed-source includes |
| [Outputs](outputs.md) | HTML, PDF, Word and portable source exports |
| [Customization](customization.md) | Themes, templates, packages and Word reference documents |
| [Configuration](configuration.md) | Defaults, precedence and resource paths |
| [Command line](cli.md) | Commands, flags, exit codes and exact help text |
| [Troubleshooting](troubleshooting.md) | Checks, source diagnostics and preview recovery |

The repository contains five complete examples:

| Example | Start here for |
|---|---|
| [Basic report](../../examples/basic/README.md) | Markdown features with the built-in theme |
| [Notebook](../../examples/notebook/README.md) | A stored plot, table and traceback |
| [Mixed report](../../examples/mixed/README.md) | Markdown and notebook includes with scoped footnotes |
| [Typst](../../examples/typst/README.md) | One native source with PDF, HTML and Word outputs |
| [LaTeX template](../../examples/latex-template/README.md) | A custom template and source export |

Markdown and notebooks default to HTML, while native Typst defaults to PDF.
Use `--to html,pdf,docx,latex,typst` to choose outputs from Markdown or a
notebook. For Typst input, omit `typst`: source export expects a Markdown or
notebook input.

Notebook builds use the results already saved in the file. Run computations
in Jupyter before building if their results need updating. Interactive widgets
and Plotly JSON do not become running applications; provide a saved static
image or another supported representation.

HTML, Word and LaTeX from native Typst use a structured subset. Arbitrary
Typst code remains available to the native PDF compiler, and unsupported
structured content produces findings. Choose a suitable output and inspect
those findings before sharing the result.
