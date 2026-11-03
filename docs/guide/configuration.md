# Configuration

Writer reads `writer.toml` by walking up from the input file's directory and
using the first one found. `--config PATH` selects a specific file instead.
An existing Markdown document, notebook or Typst source needs no project
configuration to build with defaults.

## Precedence and paths

Settings combine in this order, from lowest to highest:

1. Built-in defaults.
2. The project `writer.toml`.
3. Document front matter, notebook metadata/front matter, or recognized native
   `writer-doc.with(...)` arguments.
4. Command-line overrides.

Tables merge key by key; lists replace the earlier list. HTML theme variables
are template defaults, not build settings. The theme chain also supplies
the lowest-precedence `[design]` settings, before the project and front
matter; other theme variables do not enter build configuration.

Paths in `writer.toml` are relative to that file. Front-matter paths are
relative to the source document. CLI template/reference paths are relative
to the working directory. Those origins are retained when another layer
overrides a different setting. A configured `build.out_dir` is relative to
the config file; the built-in `dist` is relative to the source.

For example:

```toml
[build]
targets = ["html", "pdf", "docx"]
out_dir = "publication"

[pdf]
system_fonts = false

[references]
bibliography = ["refs.bib"]
style = "apa"
```

Front matter can choose `bibliography: refs.bib`, `cite_style: apa`, `theme`,
`template`, `reference_doc` or `latex_template`, and can override matching
configuration tables. Unknown keys in a configuration table are errors;
other document front-matter keys become custom template variables.

## Built-in configuration

Print these defaults with `writer dump --default-config`. It takes no input
and ignores `--config`. This reference block is generated from that command.

<!-- generated: writer dump --default-config -->
```toml
[document]
lang = "en"                # BCP-47; front matter `lang` overrides
toc = false                # false | true (depth 3) | 1..6 (max heading level listed)
                           # the TOC goes after the title block unless the document places :::toc

[build]
out_dir = "dist"           # set here: relative to this file; default: relative to the input's directory
targets = ["html"]         # for .md/.ipynb inputs; subset of html, pdf, docx, latex, typst (order = output order)
typst_targets = ["pdf"]    # for .typ inputs

[markdown]
smart_punctuation = false  # comrak parse.smart
sourcepos = false          # emit position attributes in `build` HTML too (serve always does)
references = true          # parse @target references/citations; false for presenter decks with literal @handles
superscript = false        # comrak extension.superscript (x^2^)
subscript = false          # comrak extension.subscript (H~2~O); off because ~/paths in prose would subscript

[numbering]
headings = false           # false | true (≡ "1.1") | a typst numbering pattern string ("1.", "1.1", "A.1", "1.1.a")
figures = "document"       # "document" (1, 2, …) | "section" (1.1, 1.2, …; counters reset at level-1 headings)
equations = "labelled"     # "labelled" (only equations with a label) | "all" | "none"

[references]
bibliography = []          # .bib / .yaml files, relative to this file (front matter: relative to the document)
style = "ieee"             # a hayagriva style name (typst's names) or a path to a .csl file
locale = ""                # CSL locale; "" → document.lang

[math]
html = "katex"             # "katex" (client-side KaTeX, vendored) | "mathml" (math-core, no JS) | "image" (compiler-rendered SVG)
mathml_font = "embed"      # for MathML output (html = "mathml", and typst math under any html but "image"):
                           # "embed" (Latin Modern Math woff2 inlined, +≈518 KB as base64) | "system" (font-family rule only)
katex_options = {}         # passed to katex.render (e.g. { macros = { "\\R" = "\\mathbb{R}" } })

[html]
theme = "default"          # built-in name or a directory (relative to this file)
standalone = true          # false → <stem>/index.html + assets/ (site mode)

[pdf]
template = ""              # a .typ file exporting `writer-doc` (typst-dialect.md §4); "" → built-in
paper = "a4"               # any typst paper name ("us-letter", …)
engine = "typst"           # use the embedded Typst compiler
allow_packages = false     # let the compiler download @preview packages (network)
system_fonts = true        # scan the system's fonts after the embedded ones (false for byte-identical PDFs)
font_paths = []            # extra font directories, searched after the embedded fonts and before the system's
timestamp = ""             # "" → front matter `date`, else SOURCE_DATE_EPOCH, else now; an RFC 3339 value pins it

[docx]
reference_doc = ""         # a .docx whose styles/page setup/headers/theme are used; "" → built-in
math = "omml"              # "omml" (editable Word equations, image fallback per equation) | "image" (always SVG+PNG)
update_fields = true       # ask Word to update fields (TOC page numbers, SEQ) on open; false keeps the cached values

[latex]
template = ""              # minijinja .tex template with \VAR{preamble} and \VAR{body}; "" → the built-in sample
top_level = "section"      # "part" | "chapter" | "section": what a level-1 heading becomes
cite = "biblatex"          # "biblatex" | "natbib" | "plain"
emit_bibliography = false  # true → writer appends \printbibliography / \bibliography{...} at the end of the body
code = "verbatim"          # "verbatim" (fancyvrb Verbatim) | "highlighted" (syntect tokens as \Wr* macros) | "listings"
tables = "longtable"       # "longtable" | "tabular" (floating table environment)
booktabs = true            # \toprule/\midrule/\bottomrule; false → \hline
cleveref = false           # \cref instead of "Figure~\ref"
figure_placement = "htbp"  # the [..] option of figure/table environments; "H" needs the float package; "" for none
engine = "pdflatex"        # "pdflatex" | "xelatex" | "lualatex": selects the Unicode policy and the lints; writer never runs TeX
unicode = "auto"           # "auto" (map for pdflatex, keep otherwise) | "map" (maths symbols → macros) | "keep"
cell_prompts = false       # notebook cells wrapped in writercell{In|Out}{n}
wrap = 0                   # prose line-wrap column; 0 = none

[typst]
strict = false             # W-TYP-* and E-TYP-MATH-CODE become errors for HTML/DOCX/LaTeX;
                           # --strict additionally escalates unsupported-construct and math-conversion-failed
package_paths = []         # extra local package roots ({root}/{namespace}/{name}/{version})

[notebook]
mime_priority = ["text/html", "image/svg+xml", "image/png", "image/jpeg", "image/gif", "image/webp",
                 "text/markdown", "text/latex", "application/json", "text/plain"]
hide_input_tags  = ["hide-input", "remove-input"]
hide_output_tags = ["hide-output", "remove-output"]
remove_cell_tags = ["remove-cell", "hide-cell", "skip"]
cell_options = true        # read quarto-style `#| key: value` lines
show_prompts = true        # In [n] / Out [n] in HTML and DOCX

[serve]
host = "127.0.0.1"
port = 3000
open = false
view = ""                  # "" → "pages" for .typ inputs, "web" otherwise | "web" | "pages"

# [design]                # optional cross-format typography, styles and furniture
# See docs/spec/design.md for all keys; unset values keep backend defaults.
```

## `[document]`

| Key | Default | Purpose |
|---|---|---|
| `lang` | `"en"` | Language and localized document terms |
| `toc` | `false` | TOC off, on through level three, or depth one to six |

## `[build]`

| Key | Default | Purpose |
|---|---|---|
| `out_dir` | `"dist"` | Default output directory |
| `targets` | `["html"]` | Markdown/notebook outputs |
| `typst_targets` | `["pdf"]` | Native Typst outputs |

## `[markdown]`

| Key | Default | Purpose |
|---|---|---|
| `smart_punctuation` | `false` | Typographic punctuation substitutions |
| `sourcepos` | `false` | Source positions in built HTML; preview always includes them |
| `references` | `true` | Scan document references and citations |
| `superscript` | `false` | Enable `x^2^` syntax |
| `subscript` | `false` | Enable `H~2~O` syntax |

## `[numbering]`

| Key | Default | Purpose |
|---|---|---|
| `headings` | `false` | Off, true, or a numbering pattern such as `"1.1"` |
| `figures` | `"document"` | Whole-document or section-scoped counters |
| `equations` | `"labelled"` | Number labelled, all, or no display equations |

## `[references]`

| Key | Default | Purpose |
|---|---|---|
| `bibliography` | `[]` | Bibliography files in configured order |
| `style` | `"ieee"` | Bundled style name or independent CSL file path |
| `locale` | `""` | Citation locale; empty uses the document language |

Bibliographies accept `.bib`, `.yaml` and `.yml`, case-insensitively. Keys
must be unique across files, including repeated inputs. CSL files must be
independent styles; a filename such as `journal.xml` works without a `.csl`
suffix. Named bundled styles need no file.

Unreadable/malformed bibliographies and invalid styles fail build and dump
preparation, including native PDF. Preview and check retain a named resource
finding and continue without that library so you can repair the configuration.
LaTeX export requires `.bib` resources even when YAML resolves in other formats.

## `[math]`

| Key | Default | Purpose |
|---|---|---|
| `html` | `"katex"` | KaTeX, MathML or equation images for HTML |
| `mathml_font` | `"embed"` | Embedded or system font when MathML is emitted |
| `katex_options` | `{}` | KaTeX options, including math macros |

## `[html]`

| Key | Default | Purpose |
|---|---|---|
| `theme` | `"default"` | Built-in theme or directory |
| `standalone` | `true` | One HTML file instead of a site directory |

## `[pdf]`

| Key | Default | Purpose |
|---|---|---|
| `template` | `""` | Generated-PDF template; empty uses the built-in one |
| `paper` | `"a4"` | Typst paper name |
| `engine` | `"typst"` | Embedded Typst PDF compiler |
| `allow_packages` | `false` | Permit external package downloads |
| `system_fonts` | `true` | Discover system fonts after embedded/configured fonts |
| `font_paths` | `[]` | Additional local font directories |
| `timestamp` | `""` | Pin an RFC 3339 compiler timestamp |

For repeatable PDF output, pin a timestamp and set `system_fonts = false`.
With an empty timestamp, writer uses a document date when provided, then
`SOURCE_DATE_EPOCH`, then the current time. Preview/watch retains one session
timestamp and caches fonts; restart after changing font files.

## `[docx]`

| Key | Default | Purpose |
|---|---|---|
| `reference_doc` | `""` | Styles/page setup reference; empty uses built-in |
| `math` | `"omml"` | Editable equations with fallback, or images throughout |
| `update_fields` | `true` | Ask Word to refresh fields on opening |

## `[latex]`

| Key | Default | Purpose |
|---|---|---|
| `template` | `""` | LaTeX template; empty uses the sample |
| `top_level` | `"section"` | Command for level-one headings |
| `cite` | `"biblatex"` | Citation command family: biblatex, natbib or plain |
| `emit_bibliography` | `false` | Append the selected bibliography command to the body |
| `code` | `"verbatim"` | Verbatim, highlighted macros or listings |
| `tables` | `"longtable"` | Longtable or floating tabular output |
| `booktabs` | `true` | Use booktabs rules |
| `cleveref` | `false` | Use cleveref reference commands |
| `figure_placement` | `"htbp"` | Figure/table float placement |
| `engine` | `"pdflatex"` | Source/Unicode policy for the chosen external engine |
| `unicode` | `"auto"` | Engine-dependent mapping, map or keep |
| `cell_prompts` | `false` | Prompt environments around notebook input/result output |
| `wrap` | `0` | Prose wrapping column; zero disables wrapping |

Custom templates own their citation packages and bibliography placement.
Writer exports source and resources; it never invokes the selected engine.
See [customization](customization.md#latex-template).

## `[typst]`

| Key | Default | Purpose |
|---|---|---|
| `strict` | `false` | Escalate specified Typst portability findings |
| `package_paths` | `[]` | Local package roots containing namespace/name/version |

## `[notebook]`

| Key | Default | Purpose |
|---|---|---|
| `mime_priority` | `["text/html", "image/svg+xml", "image/png", "image/jpeg", "image/gif", "image/webp", "text/markdown", "text/latex", "application/json", "text/plain"]` | Saved representation preference |
| `hide_input_tags` | `["hide-input", "remove-input"]` | Hide cell source |
| `hide_output_tags` | `["hide-output", "remove-output"]` | Hide saved outputs |
| `remove_cell_tags` | `["remove-cell", "hide-cell", "skip"]` | Remove entire cells |
| `cell_options` | `true` | Read initial `#|` option comments |
| `show_prompts` | `true` | Show saved input/output execution counts |

Disabling option comments does not disable `metadata.writer`. No notebook
setting executes code. See [notebooks](notebooks.md) for precedence and tags.

## `[serve]`

| Key | Default | Purpose |
|---|---|---|
| `host` | `"127.0.0.1"` | Listening interface |
| `port` | `3000` | Port; zero chooses a free one |
| `open` | `false` | Open the preview in a browser |
| `view` | `""` | Default by input kind, or explicitly web/pages |

## `[design]`

Design settings describe typography, named paragraph styles and page furniture
for HTML, Typst PDF and Word. They can live in the theme's `theme.toml`, the
project's `writer.toml` or document front matter. Theme inheritance merges
parent first; project settings then front matter override individual keys.
Lists replace earlier lists. An absent or empty design adds nothing.

```yaml
---
design:
  text:
    size: 12pt
  page:
    margin:
      top: 30mm
---
```

Lengths are non-negative strings with `pt`, `mm`, `cm` or `in`; a bare number
is invalid. Line height accepts a positive number for a multiple of the font
size or a length for an exact baseline pitch. Colors accept `#rgb`, `#rrggbb`
or a declared color token: `text`, `accent`, `muted`, `link`, `background`.
Color tokens can refer to each other but cannot form a cycle. Weights accept
`normal`, `medium`, `semibold`, `bold` or integers 100–900 in steps of 100.

An unset key keeps the backend's current value. The defaults below describe
that behavior; selecting one font role does not select any other role.
`<role>` means `body`, `heading` or `mono`; `<slot>` means `header` or `footer`;
`<Name>` is an arbitrary declared paragraph-style name.

### Font families and files

| Key | Type / unit | Default and effect |
|---|---|---|
| `fonts.body` | Family string or font table | Unset: HTML theme, PDF Libertinus Serif, Word Calibri |
| `fonts.heading` | Family string or font table | Unset: HTML theme, PDF body font, Word built-in heading fonts |
| `fonts.mono` | Family string or font table | Unset: HTML theme, PDF DejaVu Sans Mono, Word Consolas |
| `fonts.<role>.family` | String | Required when using a font table |
| `fonts.<role>.files` | List of file paths | `[]`; HTML embeds/copies files, PDF loads them, Word uses family names only |

Paths are relative to the file that declares them, before merging. Theme
paths must remain inside the theme directory. A child font file list replaces
the parent's list. Files are optional, but each declared file must exist.
Word's built-in headings use Calibri Light at levels one and two and Calibri
at later levels; setting `fonts.heading` selects one family for heading and
title styles.

### Body text

| Key | Type / unit | Default and effect |
|---|---|---|
| `text.size` | Length | Unset: HTML theme, PDF and Word 11 pt |
| `text.line_height` | Positive multiple or Length | Unset: HTML theme, PDF leading 0.65 em, Word multiple 1.1 |
| `text.paragraph_spacing` | Length | Unset: HTML theme, PDF 1.2 em, Word 0 |
| `text.justify` | Boolean | Unset: HTML theme, PDF true, Word false |

### Colors

| Key | Type / unit | Default and effect |
|---|---|---|
| `colors.text` | Color | Unset: HTML theme, PDF and Word black |
| `colors.accent` | Color | Unset; used for rules and the HTML accent token |
| `colors.muted` | Color | Unset; optional footer text color and HTML muted token |
| `colors.link` | Color | Unset; theme/backend link styling |
| `colors.background` | Color | Unset; HTML background and PDF page fill; Word does not apply it |

### Page geometry and furniture slots

| Key | Type / unit | Default and effect |
|---|---|---|
| `page.paper` | Typst paper-name string | Unset: `[pdf] paper` for PDF/Word, theme print size for HTML |
| `page.margin` | Length or sides table | Unset: HTML theme, PDF 2.5 cm, Word 1 in; a length sets all four sides |
| `page.margin.top` | Length | Unset: backend default |
| `page.margin.right` | Length | Unset: backend default |
| `page.margin.bottom` | Length | Unset: backend default |
| `page.margin.left` | Length | Unset: backend default |
| `page.header_offset` | Length from page top | Unset: PDF 30% of top margin, Word 0.5 in; HTML uses its print layout |
| `page.footer_offset` | Length from page bottom | Unset: PDF 30% of bottom margin, Word 0.5 in; HTML uses its print layout |
| `page.print` | `"margin-boxes"` or `"fixed"` | `"margin-boxes"`; HTML print behavior |
| `page.header` | Furniture table | Unset: retain backend header |
| `page.footer` | Furniture table | Unset: retain backend footer/page numbering |

HTML recognizes common paper names and writes other names as CSS page sizes.
Word recognizes A4, A5, US Letter and US Legal and falls back to A4 for other
names. PDF accepts the Typst paper names. Declaring either Typst furniture
slot disables the package's built-in page numbering; include `{page}` in your
footer when you want it.

### Furniture contents and variants

The same keys apply to `[design.page.header]` and `[design.page.footer]` and
to their optional `.first` and `.rest` variants. A variant replaces the base
slot for its page class; it does not inherit its contents. An empty table
explicitly clears the slot or variant. Variants cannot contain more variants.

| Key | Type / unit | Default and effect |
|---|---|---|
| `page.<slot>.image` | Path string or image table | None |
| `page.<slot>.lines` | List of strings or line tables | `[]`; left/main column |
| `page.<slot>.right_lines` | List of strings or line tables | `[]`; flush-right companion column |
| `page.<slot>.rule` | Boolean or rule table | False; true uses a 0.5 pt accent/text/black rule, below headers and above footers |
| `page.<slot>.align` | `left`, `center` or `right` | Header center; footer left; `justify` is invalid |
| `page.<slot>.size` | Length | Header body size; footer 0.75 × body size |
| `page.<slot>.color` | Color | Header text color; footer muted color, else text color |
| `page.<slot>.weight` | Weight | Normal (400) |
| `page.<slot>.italic` | Boolean | False |
| `page.<slot>.first` | Furniture table | Base slot on page one |
| `page.<slot>.rest` | Furniture table | Base slot on subsequent pages |

Text is plain text. `{page}` and `{pages}` are counters in PDF, Word and
printed HTML; screen HTML uses `1`. Unknown braces stay literal. HTML margin
boxes use the slot's styling for all lines; individual line styling remains
available on screen, in fixed HTML furniture, in PDF and in Word.

### Image, line and rule table forms

A string image path is shorthand for `{ src = "path" }`. The line table shape
below works in both `lines` and `right_lines`.

| Key | Type / unit | Default and effect |
|---|---|---|
| `page.<slot>.image.src` | File path | Required in an image table |
| `page.<slot>.image.width` | Length | Unset: natural size; raster images use 96 dpi |
| `page.<slot>.image.align` | `left`, `center` or `right` | Slot alignment |
| `page.<slot>.image.gap` | Length | `text.paragraph_spacing`, else the backend's paragraph spacing; space between the image and the lines |
| `page.<slot>.lines[].text` | String | Required in a line table |
| `page.<slot>.lines[].weight` | Weight | Slot weight |
| `page.<slot>.lines[].italic` | Boolean | Slot italic |
| `page.<slot>.lines[].size` | Length | Slot size |
| `page.<slot>.lines[].color` | Color | Slot color |
| `page.<slot>.rule.position` | `"above"` or `"below"` | Below a header; above a footer |
| `page.<slot>.rule.color` | Color | Accent, else text, else black |
| `page.<slot>.rule.width` | Length | 0.5 pt |

### Named paragraph styles

A style name matches a div's `custom-style` first and then its classes, after
ASCII letters are lowercased and every character other than an ASCII letter
or digit is removed. For example,
`Inside Address` matches `:::inside-address`. Names that normalize identically
cannot both be declared. Styles apply to divs, not inline spans.

| Key | Type / unit | Default and effect |
|---|---|---|
| `styles.<Name>.font` | `body`, `heading`, `mono` or family string | Unset: inherit; an unset role also inherits |
| `styles.<Name>.size` | Length | Unset: inherit |
| `styles.<Name>.line_height` | Positive multiple or Length | Unset: inherit |
| `styles.<Name>.color` | Color | Unset: inherit |
| `styles.<Name>.weight` | Weight | Unset: inherit; Word uses bold at 600 or above |
| `styles.<Name>.italic` | Boolean | Unset: inherit; false explicitly selects normal |
| `styles.<Name>.align` | `left`, `center`, `right` or `justify` | Unset: inherit |
| `styles.<Name>.space_before` | Length | Unset: backend default |
| `styles.<Name>.space_after` | Length | Unset: backend default |
| `styles.<Name>.keep_together` | Boolean | False; Word keeps each paragraph's lines together, not the whole multi-paragraph div |
| `styles.<Name>.keep_with_next` | Boolean | False |

The [customization guide](customization.md#cross-format-design) shows complete
examples, precedence, font handling and the inspection/ejection workflow.

## Strictness

`build --strict` escalates specified unsupported-construct and conversion
warnings, plus Typst portability codes. `[typst] strict = true` escalates the
Typst codes, rather than every backend warning. Other warnings keep their
normal severity. `check --deny-warnings` changes check's exit status when
warnings remain; it does not relabel those findings as errors.

The [CLI reference](cli.md) lists available overrides. The
[configuration contract](../spec/config.md) gives the exact schema and
escalation code sets.
