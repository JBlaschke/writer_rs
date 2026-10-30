# Customizing appearance

HTML themes, generated Typst templates, Word reference documents and LaTeX
templates control different outputs. Select the resource for the output you
want to change; a CSS theme does not style a Typst PDF or Word document.

## Extend the HTML theme

A small child theme usually needs only two files:

```text
theme/
  theme.toml
  styles/custom.css
```

In `theme/theme.toml`:

```toml
name = "my-report"
extends = "default"

[styles]
entries = ["styles/custom.css"]
```

In `theme/styles/custom.css`:

```css
:root {
    --w-measure: 46rem;
    --w-link: #275b4c;
}

@media (prefers-color-scheme: dark) {
    :root { --w-link: #9bd2bd; }
}
```

Select the directory in `writer.toml`:

```toml
[html]
theme = "theme"
```

Child entries load after inherited entries. Their resource URLs resolve
relative to their own files. Use CSS entries; SCSS is not supported. The
[default theme reference](../../assets/themes/default/README.md) lists its
tested tokens and classes, including print behavior.

Override an inherited template by writing the same relative filename under
`templates/`. To extend the page shell, a child
`theme/templates/document.html` can use:

```jinja
{% extends "@default/document.html" %}
{% block body_end %}<footer>Published with writer.</footer>{% endblock %}
```

A replacement page must render `html.main` exactly once. Preserve its
`data-writer-document` root, math initialization and live-reload helper.
Only that document root is patched during preview; a custom TOC placed
outside it will remain stale until a full reload. Inspect available data with
`writer dump document.md --context`.

To copy the entire built-in theme instead of extending it:

```sh
writer eject theme full-theme
writer build document.md --theme full-theme
```

The copied theme keeps its original root-theme name and files. The destination
must not already exist, even as an empty directory.

## Generated Typst PDF template

```sh
writer eject typst-template report-template.typ
```

The command prints the configuration to select the file:

```toml
[pdf]
template = "report-template.typ"
```

Edit the exported `writer-doc` function while preserving its expected package
interface. This template applies to PDF generated from Markdown or notebooks.
A native Typst document applies its own show rule in source and ignores this
setting with a warning.

## Portable Typst packages

```sh
writer eject typst-package typst-packages
```

The directory contains versioned `writer/doc` and `preview/mitex` packages.
Configure a local root under `[typst] package_paths`, or pass that root to an
external Typst compiler's package-path option. Writer already has its own
copies embedded; ejection is useful for external source compilation.

`writer eject typst-package typst-packages --install` selects Typst's platform
data directory instead of the supplied destination. The directory argument
is still required by the command syntax. Inspect the printed destination and
configuration before using a system-wide workflow.

## Word styles and page setup

```sh
writer eject docx-reference reference.docx
```

Open the result in Word and edit the named styles you want to change. It
demonstrates the styles writer uses. Keep their names so writer can match
them, and save the document. Select it with:

```toml
[docx]
reference_doc = "reference.docx"
```

The reference supplies styles, page setup and supported header/footer/theme
parts. Its sample body is not copied into your report. See the
[Word output](outputs.md#word) notes about equations and fields.

## LaTeX template

```sh
writer eject latex-template writer-template.tex
```

Select it with `[latex] template = "writer-template.tex"`, or use the build
flag `--latex-template`. The template uses a TeX-friendly Jinja syntax:

```tex
\documentclass{article}
\VAR{preamble}
\begin{document}
\VAR{body}
\end{document}
```

Both `\VAR{preamble}` and `\VAR{body}` are required. The preamble slot
contains generated definitions and packages needed by the chosen body. It
does not take over a custom template's citation-family packages or decide
where that template prints its bibliography.

The ejected sample includes compatible citation setup. If you use a minimal
template like the one above with citations, select `cite = "plain"` and
`emit_bibliography = true` under `[latex]`, or add the appropriate citation
packages and bibliography command yourself. Avoid printing the bibliography
twice when combining template and body settings.

Title and author fields are already escaped for LaTeX. Escape custom text
variables with `tex`, such as `\VAR{vars.venue | tex}`; raw template text is
your responsibility. `[latex] engine` chooses source-generation and Unicode
rules, so compile using the matching TeX engine. Writer never invokes it.

The [LaTeX template example](../../examples/latex-template/README.md) changes
colors, a code token macro and a custom venue line while preserving the
sample's template contract.

## Resource destinations

Ejection refuses existing destination files. Package ejection can preserve
unrelated files in a package root, but refuses its selected version paths.
Use new names instead of expecting an overwrite flag. Configuration snippets
go to stdout, including in quiet mode; human status goes to stderr.
