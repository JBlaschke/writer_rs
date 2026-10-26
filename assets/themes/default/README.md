# The default theme

The built-in document theme uses system fonts, a comfortable reading measure,
light and dark colours, and print styles. It supports prose, figures, tables,
equations, code, references and notebook cells without downloading fonts or
stylesheets. Dark mode follows the reader's operating-system preference.

The stylesheets load in this order; a child theme's entries follow them:

```text
default/
├── theme.toml                 name, stylesheet order and code themes
├── styles/
│   ├── theme.css              tokens, typography and document elements
│   ├── notebook.css           cell layout, prompts, output and ANSI colours
│   └── print.css              page layout and print rules
├── templates/
│   ├── document.html          page shell and its six overridable blocks
│   └── partials/
│       ├── head.html          charset, viewport and document metadata
│       └── scripts.html       theme scripts and the live-reload helper
└── README.md
```

## Tokens

Override these CSS custom properties on `:root` in your theme. Colour tokens
have separate light and dark values; the other tokens apply to both schemes.
The screen font size assumes the browser's usual 16 px default. Equation images
are sized for 16 px text, so substantial font-size changes can affect their fit.

| token | default | dark | purpose |
|---|---|---|---|
| `--w-font-body` | `Charter, "Bitstream Charter", "Sitka Text", Cambria, serif` | — | Running text and captions. |
| `--w-font-heading` | `system-ui, -apple-system, "Segoe UI", Roboto, sans-serif` | — | Headings, TOC and callout titles. |
| `--w-font-mono` | `ui-monospace, SFMono-Regular, Menlo, Consolas, monospace` | — | Code and cell prompts. |
| `--w-font-size` | `1rem` | — | Body font size; print uses an 11 pt root. |
| `--w-line-height` | `1.6` | — | Running-text line height. |
| `--w-measure` | `42rem` | — | Maximum document content width. |
| `--w-text` | `#23262b` | `#e9e6de` | Main text and fallback code text. |
| `--w-bg` | `#fffefb` | `#181c20` | Page background. |
| `--w-muted` | `#5c626d` | `#acb3ba` | Captions, metadata, numbers and prompts. |
| `--w-accent` | `#366b76` | `#a0c7cf` | Hover, focus and checkbox accent. |
| `--w-link` | `#235c93` | `#8abbe8` | Links and cross-references. |
| `--w-border` | `#c8c9c3` | `#515b63` | Rules, quotes and the TOC frame. |
| `--w-code-bg` | `#f2f1ec` | `#22282d` | Inline code and blocks with code theme `"none"`. |
| `--w-code-highlight` | `#fff0b3` | `#4b422b` | Highlighted code lines and inline highlights. |
| `--w-callout-note` | `#326b9a` | `#8dbce3` | Note edge. |
| `--w-callout-tip` | `#2c7758` | `#91c8a9` | Tip edge. |
| `--w-callout-important` | `#7954a1` | `#c0a8e0` | Important edge. |
| `--w-callout-warning` | `#946118` | `#e1bf80` | Warning edge. |
| `--w-callout-caution` | `#b24438` | `#e5a098` | Caution edge. |
| `--w-callout-other` | `#5c626d` | `#acb3ba` | Edge for custom callout kinds. |
| `--w-error` | `#cc0000` | `#ff8a80` | Unresolved references, math errors and stderr edge. |
| `--w-error-bg` | `#fcedeb` | `#3f2828` | Stderr and error-output background. |

The code themes are `InspiredGitHub` in light mode and `OneHalfDark` in dark
mode. Their generated styles load after the entries above when code is present.
They set their own block text and background colours. `--w-code-bg` controls
inline code and the fallback for code theme `"none"`; it does not recolour a
syntax theme. To change syntax colours, set `code_theme` and `code_theme_dark`
in your theme's `[styles]` table.

## Classes

These classes are emitted by writer. User-supplied classes can add further
styles without changing the document structure.

- `.title-block` — Centred document header and separating rule.
- `.title` — Main title.
- `.subtitle` — Secondary title text.
- `.authors` — Wrapping group of authors.
- `.author` — One author's metadata.
- `.author-name` — Author's name.
- `.author-affiliation` — Affiliation on its own line.
- `.author-email` — Email link on its own line.
- `.author-id` — Author identifier on its own line.
- `.date` — Document date.
- `.abstract` — Left-aligned abstract within the title block.
- `.abstract-title` — Compact abstract heading.
- `.number` — Heading or TOC number.
- `.task-list` — List with hanging checkboxes and no markers.
- `.task-list-item` — Checkbox positioning context.
- `.hl-code` — Fallback block text and background colours.
- `.numbered` — Code line counter.
- `.line` — One code line, preserving horizontal scrolling.
- `.highlight` — Background highlight for code lines or inline text.
- `.figure` — Figure spacing.
- `.kind-image` — Centred image figure.
- `.kind-table` — Table figure with caption above its body.
- `.kind-listing` — Code listing with caption above its body.
- `.kind-other` — Centred general-purpose figure.
- `.caption-label` — Bold figure, table or listing label.
- `.align-left` — Left-aligned table cell.
- `.align-center` — Centred table cell.
- `.align-right` — Right-aligned table cell.
- `.equation` — Flex row containing a display equation and its number.
- `.equation-number` — Equation number at the right edge.
- `.math` — Mathematical content, inline or displayed.
- `.inline` — Inline mathematical expression.
- `.display` — Centred display math, scrolling when too wide.
- `.math-source` — Code-style source when math cannot be converted.
- `.math-error` — Visible mathematical error.
- `.callout` — Inset note with a coloured left edge.
- `.callout-note` — Note edge colour.
- `.callout-tip` — Tip edge colour.
- `.callout-important` — Important edge colour.
- `.callout-warning` — Warning edge colour.
- `.callout-caution` — Caution edge colour.
- `.callout-title` — Bold callout heading.
- `.page-break` — Hidden on screen; forces a page break in print.
- `.toc` — Framed table of contents without list markers.
- `.toc-title` — TOC heading.
- `.references` — Bibliography section spacing.
- `.csl-bib-body` — Bibliography text size.
- `.hanging-indent` — Hanging bibliography entries.
- `.references-title` — Bibliography heading.
- `.csl-entry` — One bibliography entry.
- `.csl-left-margin` — Left-hand bibliography prefix.
- `.csl-right-inline` — Bibliography text beside its prefix.
- `.citation` — Citation with tabular numerals.
- `.unresolved` — Error colour and wavy underline.
- `.ref` — Cross-reference with tabular numerals.
- `.footnotes` — Smaller endnote section.
- `.footnote-ref` — Raised footnote link.
- `.footnote-back` — Compact backlink to a footnote's source.
- `.smallcaps` — Small capitals.
- `.cell` — Notebook cell spacing and positioning context.
- `.cell-input` — Notebook input and its prompt.
- `.cell-prompt` — Muted monospace prompt in the left gutter on wide screens.
- `.cell-outputs` — Notebook output group.
- `.output` — One notebook output and its optional prompt.
- `.output-stream` — Compact stream-output spacing.
- `.stdout` — Standard output text colour.
- `.stderr` — Error-coloured left edge.
- `.output-error` — Rounded, tinted error output.
- `.output-display` — Display-output spacing.
- `.output-result` — Result-output spacing.

The theme also supports these directive, typst and notebook classes:

- `.columns` — Equal grid columns, stacked on narrow screens.
- `.column` — Column that allows code and equations to scroll within it.
- `.left` — Left-aligned container.
- `.center` — Centred container.
- `.right` — Right-aligned container.
- `.small` — Text at 85% of its surrounding size.
- `.large` — Text at 125% of its surrounding size.
- `.overline` — Overlined inline text.
- `.unresolved-reference` — Unresolved-reference placeholder.
- `.output-text` — Plain notebook output.
- `.output-stream-stdout` — Standard-output code block.
- `.output-stream-stderr` — Standard-error code block.
- `.output-json` — JSON output code block.
- `.subcaption` — Smaller group-panel caption.
- `.no-print` — Content omitted from print.

ANSI output retains its palette rather than adapting each colour to the page.
The colour names are `black`, `red`, `green`, `yellow`, `blue`, `magenta`,
`cyan` and `white`:

- `.ansi-<name>-fg`, `.ansi-<name>-bg` — Normal foreground and background.
- `.ansi-<name>-intense-fg`, `.ansi-<name>-intense-bg` — Intense palette pair.
- `.ansi-default-inverse-fg`, `.ansi-default-inverse-bg` — Swapped page colours.
- `.ansi-bold`, `.ansi-dim`, `.ansi-italic` — Weight, reduced opacity and italics.
- `.ansi-underline`, `.ansi-strike`, `.ansi-hidden` — Underline, strike and hiding.

## Print

Print uses A4 paper, 20 mm vertical and 18 mm horizontal margins, and an 11 pt
root font. The document tokens use their light values regardless of the screen
scheme. Page numbers appear at the bottom in Chrome, with the first page
unnumbered. Other browsers may omit margin-box page numbers. The TOC and
`.no-print` content are hidden. `.page-break` forces a new page, and headings
stay with the following content where the browser supports it. Long code lines
wrap; external links gain their destinations after the link text. Syntax colours
print in monochrome, keeping line highlights readable in either screen scheme.
ANSI output retains its palette, and errors keep their light background tint.
Equation images assume 16 px screen text and may appear slightly larger relative
to printed text.

## Extending

Create a child theme directory containing `theme.toml`:

```toml
name = "mytheme"
extends = "default"

[styles]
entries = ["styles/custom.css"]
```

Your entries load after the inherited entries. Override tokens in a child
`:root`, and supply a dark override if the value should change with the scheme:

```css
:root { --w-measure: 46rem; --w-link: #275b4c; }
@media (prefers-color-scheme: dark) {
    :root { --w-link: #9bd2bd; }
}
```

Override an inherited template by putting the same relative path under your
theme's `templates/`. To extend the page shell, use:

```jinja
{% extends "@default/document.html" %}
{% block body_end %}<footer>Published with writer.</footer>{% endblock %}
```

The six blocks are `head`, `styles`, `body_start`, `main`, `scripts` and
`body_end`. Keep `math_init()` in the styles block, `theme_style()` last among
the stylesheets, `html.main` once inside the page, and `live_reload()` in the
scripts when replacing those blocks. `html.main` includes the title, abstract,
TOC, body and footnotes.

The default shell uses only `html.main`. A custom layout can put `html.toc`
outside it, but that sidebar goes stale during `writer serve` until a full
reload: only the element with `data-writer-document` is patched.

To copy the complete built-in theme for direct editing, run
`writer eject theme ./theme`. Select it with `writer build document.md --theme
./theme`, or set `[html] theme = "./theme"` in `writer.toml`.
