# Native Typst sources

For a `.typ` input, PDF compiles the original Typst source. HTML, Word and
LaTeX read a structured subset from that same file. This keeps ordinary
document content portable while allowing full Typst programs in the native
PDF path.

```sh
writer build document.typ
writer build document.typ --to html,docx,pdf,latex
writer serve document.typ
```

The [Typst example](../../examples/typst/README.md) is a complete source within
the structured subset. Source export `--to typst` expects a Markdown or
notebook input and rejects a native Typst source.

## Start a document

```typst
#import "@writer/doc:0.1.0": *

#show: writer-doc.with(
    title: [My report],
    authors: ("Your name",),
    toc: true,
    numbering: (headings: "1.1", figures: "document", equations: "labelled"),
)

= Results <sec-results>

The model is $T(n) = W/n + c$.
```

The bundled package supplies document styling and portable helpers such as
callouts, code cells and notebook selection. The import version must match
the package you use. Writer provides its bundled version without downloading
it. Local package roots can be configured under `[typst] package_paths`.

Native Typst applies its template in its own source. `[pdf] template` is for
generated Markdown/notebook PDF and is ignored with a warning on native input.

## What structured outputs can read

The subset recognizes document headings, paragraphs, formatting, figures,
tables, raw code, math, references, citations, notes and supported literal
helper calls. Literal local bindings and known imports have defined behavior.
Loops, computed content and arbitrary function evaluation are not a second
Typst interpreter; they produce findings or preserve visible fallback content
where documented.

A native PDF may therefore succeed while a structured output reports an
unsupported construct. A PDF compiler failure can also leave a successful
structured output when its own conversion supports the content. Inspect the
individual findings rather than assuming every target has the same authority.
`--strict` escalates specified portability warnings; see
[configuration](configuration.md#strictness).

Use native Typst math such as `$integral_0^1 x^2 dif x$`. Word's default math
route converts supported equations to editable OMML, with an image fallback.
LaTeX export converts native math to LaTeX and reports unsupported syntax.
The [subset contract](../spec/typst-dialect.md) lists exact accepted calls.

## Including Markdown and notebooks

Native includes accept supported Markdown and notebook extensions:

```typst
#include "parts/context.md"
#include "analysis.ipynb"
```

The included body joins the parent document. Included titles, authors and
other document metadata do not replace the parent's document metadata.
Local assets resolve against their own included source. Native synthesis
requires includes to stay within the canonical document root.

For notebook selection, pass a path value so the package resolves it against
the calling source:

```typst
#include-notebook(
    path("analysis.ipynb"),
    cells: (0, "plot"),
    tags: "report",
    hide-input: true,
)
```

Replace the index, id and tag with ones that exist in your notebook. Indices
are zero-based original cell indices. A tuple of indices/ids selects their
union in notebook order, and `tags` keeps cells having any requested tag.
When both are present, their intersection is selected. A requested unknown,
removed or unsupported cell produces an error; selection does not restore
cells removed by notebook policy.

`hide-input: true` or `hide-output: true` hides that part of every selected
code cell. `auto` or `false` keeps the notebook's own options; it does not
force content hidden by the notebook to reappear. A plain string file argument
is rejected by the native helper: use `path("analysis.ipynb")`.

The native and structured selection paths differ in a few cases. Native
notebook module fields are evaluated eagerly, so failing raw/native expressions
in an unselected cell can still fail import. A selected repeated footnote
reference can fail natively if the selection omits the cell containing its
first rendered definition; structured selection can retain that definition.
Malformed duplicate cell ids can also select differently when the first
original cell was removed. Keep ids unique and test the actual requested
outputs.

## Reproducibility and preview

Use embedded fonts and a fixed `[pdf] timestamp` for reproducible builds.
Package downloads require an explicit opt-in; local and embedded packages do
not. The [output guide](outputs.md#pdf) shows a small configuration.

Serve starts in pages for native Typst, with an independent web view available
through the view button or `p`. Each view retains its previous good result
when the other fails. Retained preview/watch sessions cache font discovery
and decoded fonts, including failed font loads. Restart after changing font
files; a one-shot build creates a fresh session.
