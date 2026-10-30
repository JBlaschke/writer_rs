# Troubleshooting

Start with the reported source path, cell and line. A finding from an included
file points to that file; a notebook finding can identify its original cell.
Compiler diagnostics include available source context and hints. If a source
line is unavailable, its absence does not prevent the error from being shown.

## Check before sharing

```sh
writer check document.md
writer check document.md --target html,pdf,docx,latex,typst
writer check document.md --deny-warnings
writer check document.md --format json
```

Check combines source findings with those for the selected outputs and writes
no output artifacts. Without `--target`, configured defaults select output
checks. Informational notes do not fail a check; errors do, and
`--deny-warnings` also fails on warnings. Quiet text still explains warnings
that caused a denied-warning failure. Quiet JSON stays complete.

JSON is an array whose entries have `level`, `code`, `message`, `file`, `line`
and `cell`. Unavailable locations are null. Redirect stdout when a program
needs that array; text diagnostics use stderr.

Check is not a full native PDF build or a replacement for opening the final
document. It can run conversion needed for a selected output, such as Word
math, without writing that output. A successful PDF compilation still needs
the actual build and its source resources.

## Missing files and includes

Resolve a relative asset against the file that contains it, which can differ
from the root document. A path in `writer.toml` instead starts at that config
file; a CLI template path starts at the working directory. Check spelling and
case, especially when moving from a case-insensitive filesystem.

An include must use its supported top-level form and close its directive.
Cycles and excessive nesting are errors. Native synthesized Markdown/notebook
includes must remain within the document root. See
[authoring](authoring.md#include-another-source) and
[Typst includes](typst.md#including-markdown-and-notebooks).

Writer refuses to overwrite an input through an output path. Choose a new
output directory; do not work around the refusal by moving an input into its
sidecar directory. A source export checks its expected resource and output-plan
failures before that target writes. An unexpected filesystem error during
writing is not a transaction that rolls back every earlier file.

## References and bibliography

An unresolved label/key appears as a visible placeholder. Ensure the label
exists, or configure the bibliography that contains the key. Labels are unique
across includes. Do not reuse a document label as a bibliography key or mix
the two kinds in a citation group.

Bibliography parse errors and duplicate keys name the offending resource.
The first file that conflicts with earlier keys is rejected, and later files
are not read to mask that conflict. Keep keys unique, including when the same
file is listed twice.

Unknown bundled styles, malformed independent CSL and dependent CSL styles
produce a citation-style finding. A custom style file may use an extension
such as `.xml`; its contents must still be an independent CSL style. Locale
settings use CSL locale codes. These configured resource failures stop build
and dump preparation, including native PDF; preview and check retain a named
finding while allowing repair.

`[-@key]` means a year citation. To suppress inline citation text while keeping
the bibliography entry, use `@key{.none}`. Note styles can introduce citation
footnotes; they do not turn every citation mode into a note.

## Templates and math

LaTeX templates need both `\VAR{preamble}` and `\VAR{body}`. Preserve the
sample's citation setup, or supply your own compatible packages and printing
command. Dump can show useful generated TeX alongside error findings; build
does not commit a target that failed its rendering/materialization checks.
An earlier preparation finding may still leave output from a target that
could render. Always check the exit status and diagnostics.

If native Typst math is unsupported in another format, inspect the conversion
finding and try a supported expression or an image route appropriate to that
output. Word's default editable math route falls back to an image per equation
when needed. Arbitrary native code can compile to PDF without having a
structured representation for HTML, Word or LaTeX.

Native `.typ` input chooses its own template through its source show rule.
An ignored `[pdf] template` warning means that setting applies to generated
Markdown/notebook PDF instead. Font changes in retained preview/watch sessions
require a restart, including when an earlier font load failed.

## Packages and preview recovery

Embedded document and math packages need no download. An unavailable external
Typst package is refused by default. If the document requires it, explicitly
allow package downloads with `--allow-packages` or `[pdf] allow_packages = true`,
or configure a local package root. The error identifies the relevant import
and available package trace/hint.

In preview, web and typeset pages have independent results. Correcting one
view's failure does not require the other to fail too. The previous good view
stays visible under an error; repair the source or resource and save again.
Use the warning panel for nonfatal findings. If the server cannot bind its
port, choose another or use `--port 0` and open the printed address.

If a build with several targets exits with an error, inspect which outputs
were written. A target failure does not cancel every other target, while a
fatal source/configuration preparation failure prevents target execution.
The [output guide](outputs.md) explains those boundaries and destinations.
