# Customizing appearance

A shared design controls typography, paragraph styles and page furniture in
HTML, Typst PDF and Word. HTML themes, Typst templates, Word reference
documents and LaTeX templates provide further control over their outputs.

## Cross-format design

Put `[design]` in an HTML theme's `theme.toml`, in `writer.toml`, or in
front matter under `design:`. One definition supplies typography, paragraph
styles and page furniture to HTML, Typst PDF and Word. LaTeX keeps its own
template settings. With no design, every backend keeps its existing defaults.

Typography and geometry use lengths with `pt`, `mm`, `cm` or `in`. Colors are
hex values or names from the design's color table:

```toml
[design.fonts]
body = "Source Sans 3"
heading = "Source Sans 3"

[design.text]
size = "11pt"
line_height = 1.35
paragraph_spacing = "3.6mm"
justify = false

[design.colors]
text = "#242424"
accent = "#a96d4b"
link = "accent"

[design.page]
paper = "a4"
margin = { top = "50mm", right = "22mm", bottom = "38mm", left = "22mm" }
```

Named paragraph styles select document divs by `custom-style` first, then by
class. Matching ignores case and punctuation: `Inside Address` also matches
`:::inside-address`. When `custom-style` does not name a design style, the
first matching class can select one. Inline spans do not receive these paragraph styles.

```toml
[design.styles.Recipient]
line_height = "12.1pt"
space_after = "6mm"
keep_together = true
```

```markdown
:::recipient
Dr Example
Example Institute
:::
```

HTML writes `data-style="Recipient"`, generated Typst writes
`writer-div(style: "Recipient", ...)`, and Word selects the corresponding
paragraph style. A surrounding div's style applies until an inner div selects
another style. `keep_together` keeps a block together in HTML and PDF; Word
uses paragraph keep-lines, which prevents splitting each paragraph but can
still split a div containing several paragraphs. `keep_with_next` keeps the
block with the following content where the output supports that behavior.

Furniture slots can contain an image, lines, a right column and a rule. Text
is plain text. `{page}` and `{pages}` become page counters in printed output;
screen HTML displays `1`. A `first` or `rest` table replaces the base slot for
that page class; it does not inherit the base slot's contents.

```toml
[design.page]
header_offset = "17mm"
footer_offset = "10mm"

[design.page.header.first]
image = { src = "assets/logo.png", width = "50mm" }
lines = ["Example Institute"]
color = "accent"

[design.page.header.rest]
lines = ["Example Institute | Correspondence"]
size = "10pt"

[design.page.footer]
rule = true
lines = ["Page {page} of {pages}"]
```

An empty furniture table explicitly clears an inherited slot. An undeclared
slot keeps the backend's own furniture, except that Typst disables its built-in
page numbering whenever either furniture slot is declared. HTML prints through
page margin boxes by default; `design.page.print = "fixed"` selects repeated fixed furniture.
Margin boxes flatten per-line styling to the slot's styling; screen HTML,
Typst and Word support per-line weight, italic, size and color.

Design sources merge parent theme first, then child theme, project settings
and document front matter. Tables merge by key; lists replace. Resource paths
are relative to the file declaring them, before merging. Theme resources must
stay inside their theme. Font and image changes rebuild every affected output.

| Existing customization | How it interacts with design |
|---|---|
| HTML theme CSS | Explicit theme rules and token overrides win; dark-mode theme rules still apply. |
| Typst PDF template | A template delegating to `writer-doc` receives design defaults; explicit arguments and later rules win. |
| Word reference document | Existing styles and the body section, including referenced headers and footers, win. Missing styles can use generated definitions. |

A font family can be a string or a table with `family` and `files`. HTML emits
font-face rules and copies or embeds the files. PDF loads those files into its
font book. Word selects fonts by family name and does not embed the files, so
readers need the fonts installed. An unset role keeps that backend's default.

Inspect, validate and package a design with:

```sh
writer dump --design letter.md
writer check letter.md --target html,pdf,docx
writer eject theme portable-theme --design letter.md
```

`dump --design` prints resolved points, colors and absolute paths, or `null`
when no design is declared. Ejection copies referenced resources under
`assets/design/` and rewrites paths. `check` reports missing font files and
images. For Markdown and notebooks, it also reports unavailable PDF font
families, unused styles and reference documents or templates that shadow the
design. An unavailable theme produces a warning when HTML is not requested.
Native Typst keeps its own styling authority and receives only design resource
checks.
Missing resources and malformed design values fail a build.

See the [configuration reference](configuration.md#design) for all keys and
the [letter example](../../examples/letter/README.md) for a complete project.

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
