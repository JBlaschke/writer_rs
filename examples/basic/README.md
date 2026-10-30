# Basic document

From the repository root, after installing writer:

```sh
writer build examples/basic/document.md
writer build examples/basic/document.md --site
writer build examples/basic/document.md --to html,pdf,docx,latex,typst
writer serve examples/basic/document.md --open
```

The default standalone build writes `examples/basic/dist/document.html`.
Site mode writes `examples/basic/dist/document/index.html` and its `assets/`
directory. The mixed build writes the other formats beside the standalone
file, with resources and include parts where required by source exports.
LaTeX is source output; a separate TeX installation compiles it to PDF.

The preview watches the document and its resources. It starts in the web view;
use its view button or `p` to switch to typeset pages. Stop it with Ctrl-C.

Inspect the resolved document and HTML template context:

```sh
writer dump examples/basic/document.md
writer dump examples/basic/document.md --context
```

The source demonstrates numbered headings, labelled figures and a table,
callouts, highlighted code, equations, page breaks and a footnote. The config
uses embedded PDF fonts. See the [authoring guide](../../docs/guide/authoring.md)
for references and citations, and the [mixed example](../mixed/README.md) for
Markdown and notebook includes.
