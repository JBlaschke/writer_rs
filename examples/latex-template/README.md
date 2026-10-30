# A report with a custom LaTeX template

From the repository root:

```sh
cargo run -- build examples/latex-template/document.md
```

The example config selects LaTeX output and the local `writer-template.tex`.
Writer writes `examples/latex-template/dist/document.tex` and converts the SVG
figure to a PDF inside `dist/document_assets/`. It exports source and resources;
it does not run a TeX engine.

With latexmk and a PDFLaTeX installation available, compile from the output
directory so the generated relative image paths resolve:

```sh
cd examples/latex-template/dist
latexmk -pdf -interaction=nonstopmode -halt-on-error document.tex
```

Open `document.pdf`. The report contains a vector figure, an editable table,
numbered cross-references, an equation and highlighted Python code.

The custom template is derived from writer's built-in sample. It keeps the
required `\VAR{preamble}` and `\VAR{body}` slots, adds an accent colour, restyles
`\WrKeyword` after the preamble defines it, and prints an optional venue line
from `\VAR{vars.venue | tex}`. Change `venue` in the Markdown front matter to
customize that line; `tex` escapes characters such as its ampersand. Ordinary
title/author fields are already escaped by writer.

The document has no citations or external bibliography. The figure has no
external resources or fonts. The config fixes the embedded compiler's timestamp
and disables system fonts and package downloads for reproducible sidecar output.
