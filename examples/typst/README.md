# Native Typst document

From the repository root, after installing writer:

```sh
writer build examples/typst/document.typ
writer build examples/typst/document.typ --to html,docx,pdf,latex
writer serve examples/typst/document.typ
writer serve examples/typst/document.typ --view web
```

The default build writes `examples/typst/dist/document.pdf` by compiling the
original Typst file. The mixed build writes HTML, Word and LaTeX beside it.
LaTeX is source output and needs a separate TeX installation to compile.

The example stays within the structured subset, so its outputs can render
headings, a figure, references, a callout, a footnote and native math. Word
equations use editable OMML where supported, with an image fallback.

The bundled `@writer/doc` package and embedded fonts work offline. The config
pins a timestamp for repeatable output; adjust or remove it for your document.
The SVG has no external resources or font requirements.

Serve starts in the typeset pages view for Typst input. Use the view button or
`p` to switch to the structured web view. Edits rebuild both views; each keeps
its previous good result when the other fails.

Native PDF supports the full Typst language. HTML, Word and LaTeX use the
documented structured subset: loops, computed content and unknown calls
produce findings there, while valid native PDF can still build. A native
compiler error can likewise leave a usable structured output when that
output's conversion and recovery support the content.

Native Typst can include Markdown and notebooks, including selected notebook
cells through `include-notebook(path("analysis.ipynb"), cells: ...)`. See the
[Typst guide](../../docs/guide/typst.md) for selection and native/structured
differences. Source export `--to typst` is for Markdown and notebook inputs;
it rejects an input that is already Typst. A native document applies its
template in its own source, so `[pdf] template` is ignored with a warning.

Font discovery and decoded fonts are cached for a retained watch or preview
session, including failed font loads. Restart the session after changing font
files. Ordinary one-shot builds create a fresh session.
