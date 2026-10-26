# Basic document

From the repository root:

```sh
cargo run -- build examples/basic/document.md
cargo run -- build examples/basic/document.md --site
cargo run -- dump examples/basic/document.md
cargo run -- dump examples/basic/document.md --context
```

The standalone build writes `examples/basic/dist/document.html`. Site mode
writes `examples/basic/dist/document/index.html` and its `assets/` directory.
Open either HTML file in a browser. The source demonstrates numbered headings,
figures, a table, callouts, highlighted code, equations and a footnote.

PDF, DOCX, LaTeX, typst export and live preview have command placeholders until
their planned implementation tasks are complete. References and includes will
be added to this example with the reference scanner.
