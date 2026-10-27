# One report from several files

Build the report from the repository root:

```sh
cargo run -- build examples/mixed/document.md
```

`document.md` includes `parts/context.md`, which includes
`parts/details/method.md`, and then includes `analysis/results.ipynb`.
The diagram path is relative to the method file. The notebook embeds its
saved plot and DataFrame output, so the example needs no Python runtime.

The headings, plot and table use references across file boundaries. Each
source also has its own `[^1]` footnote; their names stay scoped to that file.
Included metadata never replaces the root document's title or authors.

The notebook's Python cells and stored outputs were copied from the executed
[notebook example](../notebook/README.md). Only Markdown, cell identifiers and
`#|` caption/label comments were adapted; the computation was not rerun.
