# Choosing outputs

Choose one target or a comma-separated list:

```sh
writer build document.md --to html,pdf,docx,latex,typst
```

The default is HTML for Markdown and notebooks, and PDF for native Typst.
`writer.toml` can change those defaults. A mixed build shares document
preparation, then builds each requested target. A target failure can leave
successful outputs from the other targets; the command still exits with an
error. An invalid source, configuration or configured bibliography can fail
preparation before targets run.

## Files and destinations

By default, outputs go in `dist` next to the source. An `out_dir` configured
in `writer.toml` is relative to that configuration file.

| Target | Output for `document.md` |
|---|---|
| HTML | `dist/document.html` |
| HTML site | `dist/document/index.html` and `dist/document/assets/` |
| PDF | `dist/document.pdf` |
| Word | `dist/document.docx` |
| LaTeX | `dist/document.tex` and resources in `dist/document_assets/` |
| Typst source | `dist/document.typ`, resources in `dist/document_assets/` and materialized include parts beside the main source |

With one target, `-o` names the output file; for an HTML site it names the site
directory. With several targets, it names the output directory:

```sh
writer build document.md --to pdf -o report.pdf
writer build document.md --to html,docx -o publication
writer build document.md --site -o website
```

Source-export resource directories sit beside the named source file. Keep the
source, its resource directory and any include parts together when moving an
export. Writer refuses output paths that would replace input resources.

## HTML

The default standalone HTML keeps the built-in theme, document images and
required math resources in one file. Open it from disk or publish it on a web
server. User-written links, custom styles and raw HTML can still reference
external resources.

Use `--site` for separate assets and `--standalone` to override a site setting.
HTML's theme controls its screen and print appearance. Its browser print view
has its own layout; the PDF target uses Typst for typesetting.

## PDF

Writer embeds the Typst compiler and fonts. Markdown and notebook documents
become generated Typst; a native Typst input compiles its original source.
The native path supports the full Typst language. Other outputs from that same
file use the [structured subset](typst.md).

System font discovery is enabled by default. To make builds independent of
installed fonts and pin the compiler time, use a project configuration such as:

```toml
[pdf]
system_fonts = false
allow_packages = false
timestamp = "2026-10-29T12:00:00Z"
```

The timestamp can otherwise come from the document date or
`SOURCE_DATE_EPOCH`. Embedded packages work offline; external package downloads
require an explicit opt-in. Restart a watch or preview session after changing
font files because the session caches font discovery and loaded fonts.

## Word

Word output preserves editable paragraphs, lists, tables, code and supported
equations. Equations use OMML by default; a conversion that cannot preserve an
equation falls back to an image and produces a finding. Set `[docx] math =
"image"` to request equation images throughout.

Writer supplies named styles and field values for captions and references.
Word may update fields and TOC page numbers when opening the file, depending
on its settings. A reference document supplies styles and page setup; it is
not a body template. See [customization](customization.md) for editing it.

## LaTeX source

```sh
writer build document.md --to latex
```

Writer emits a complete `.tex` source with its template. It copies local
image resources, converts SVG/GIF/WebP images to PDF, and copies supported
`.bib` sources into the sidecar directory. Hayagriva YAML bibliographies can
resolve citations for other formats but cannot be exported as LaTeX BibTeX
resources.

Compile from the output directory so its relative resource paths resolve:

```sh
cd dist
latexmk -pdf -interaction=nonstopmode -halt-on-error document.tex
```

This separate step needs a TeX installation and latexmk. Writer itself never
runs a TeX engine. `[latex] engine` selects source-generation and Unicode
policies; choose a matching engine when compiling. Custom templates own their
citation packages and bibliography placement. The
[template example](../../examples/latex-template/README.md) demonstrates the
required slots and safe custom text.

To inspect source without copying resources, use `writer dump document.md
--latex`. Its stdout is the exact template result, including its final newline
policy; diagnostics go to stderr. A dumped file can reference resources that
have not been materialized, so use `build` for a portable export.

## Typst source

```sh
writer build document.md --to typst
```

This exports generated source, resource files and materialized Markdown or
notebook include parts. It also copies configured bibliography, path-valued
CSL and custom template resources as needed by the export. The main file uses
relative paths into that output bundle.

The exported source uses the bundled document package. To use it in an
external Typst installation, supply the corresponding package files through
a local package root; see [customization](customization.md). Writer's own PDF
build already has those packages embedded.

`writer dump document.md --typst` prints generated source without exporting
the bundle. `--to typst` rejects a source that is already Typst; keep that
original file as its source document.
