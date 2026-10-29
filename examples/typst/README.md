# Native Typst document

From the repository root:

```sh
cargo run -- build examples/typst/document.typ
cargo run -- build examples/typst/document.typ --to html,docx,pdf
cargo run -- serve examples/typst/document.typ
cargo run -- serve examples/typst/document.typ --view web
```

The default build writes `examples/typst/dist/document.pdf` by compiling the
original Typst file. HTML and Word outputs appear beside it as `document.html`
and `document.docx`. The example stays within the structured subset, so all
three outputs can render its headings, figure, references, callout, footnote
and native math. Word equations use editable OMML.

The bundled `@writer/doc` package and embedded fonts work offline. The config
pins a timestamp for repeatable output; adjust or remove it for your document.
The SVG has no external resources or font requirements.

Serve starts in the typeset pages view for Typst input. Use the view button or
`p` to switch to the structured web view. Edits rebuild both views; each keeps
its previous good result when the other fails.

Native PDF supports the full Typst language. HTML and Word render the documented
structured subset: loops, computed content and unknown calls are reported there,
while a valid native PDF can still build. A native compiler error can likewise
leave a usable HTML or Word output through the documented math recovery paths.

LaTeX document export is still pending T60/T61. Native `#include` of `.md` or
`.ipynb` files and `include-notebook` are pending T53. Source export `--to typst`
is for Markdown/notebook inputs; it rejects an input that is already Typst.
A native document applies its template in its own source; `[pdf] template` is
ignored with a warning for Typst inputs.

Font discovery and decoded fonts are cached for a retained watch/serve session,
including failed font loads. Restart that session after changing font files;
this milestone does not add font-file reloading. Ordinary one-shot builds
create a fresh session.
