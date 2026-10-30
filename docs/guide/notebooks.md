# Saved notebooks

Writer reads the cells and outputs saved in a Jupyter notebook. It never runs
Python or starts a kernel. Run the notebook in Jupyter and save it before
building when the results need updating.

```sh
writer build analysis.ipynb
writer build analysis.ipynb --to html,pdf,docx
writer serve analysis.ipynb
```

The [notebook example](../../examples/notebook/README.md) contains a saved plot,
DataFrame, Markdown display and deliberate traceback. Its README explains the
optional regeneration environment separately from writer's requirements.

## Metadata and document structure

Markdown cells support the [document dialect](authoring.md), including math,
references, footnotes and top-level includes. Put document front matter in a
Markdown cell, or use the notebook's `metadata.writer` object. Later document
metadata overrides earlier values key by key. Configuration paths resolve
relative to the notebook.

A Markdown heading is a section, not a slide boundary. Writer ignores slide
metadata. Code cells become input followed by their saved outputs, with prompts
when available. Raw cells use their declared MIME type; raw Markdown is parsed
as Markdown, while other raw content is target-specific.

## Cell options

Write Quarto-style option comments at the beginning of a code cell:

```python
#| label: fig-results
#| fig-cap: "Results from the saved run"
#| fig-alt: "The measured values rise with the sample count"
#| echo: false

display(plot)
```

Writer reads the comments as options; it does not execute `display(plot)`.
The cell must already contain a suitable saved output. The same option keys
can appear in the cell's `metadata.writer` object.

| Option | Purpose |
|---|---|
| `echo: false` | Hide code input |
| `output: false` | Hide saved outputs |
| `include: false` | Omit the entire cell |
| `warning: false` | Omit saved stderr streams |
| `error: false` | Omit saved error outputs |
| `label`, `fig-cap`, `tbl-cap` | Label and caption figure or table outputs |
| `lst-label`, `lst-cap` | Label and caption the input listing |
| `fig-subcap`, `tbl-subcap` | Provide panel subcaptions for grouped outputs |
| `code-line-numbers` | Show or highlight input lines |
| `output: asis` | Parse text output as Markdown |

Unsupported options produce a finding rather than silently acquiring execution
semantics. Execution keys such as `eval`, `cache` and `kernel` do not run or
cache code. Use `output: asis` only when the saved text is intended to become
document markup.

## Tags and hiding

Default tags include `hide-input`/`remove-input`,
`hide-output`/`remove-output`, and `remove-cell`/`hide-cell`/`skip`. Configure
their lists under `[notebook]`. An underscore and hyphen are equivalent in
these tag names.

For each effect, an explicit option comment wins over `metadata.writer`, which
wins over the matching tag or Jupyter hidden flag. For example, `#| echo: true`
keeps input visible even on a cell tagged `hide-input`. A wrong-typed comment
does not replace a valid metadata option. Markdown and raw cells honor cell
removal tags, rather than code input/output options.

These are static outputs: hidden content has no reveal button. Hiding an
output also removes its caption and label from that output.

## Saved output choices

A notebook output can offer several representations of the same result.
Writer keeps supported candidates and picks one appropriate to each output
format. Configure preference with `[notebook] mime_priority`. HTML tables can
become editable document tables, and static image forms can work across
formats. A labelled or numbered result uses a stable shared representation
where needed to keep references consistent.

Streams and tracebacks stay saved text. ANSI colors are preserved where the
output supports them. Attachments and local output images are collected as
document resources; relative files resolve beside the notebook.

Interactive widget state does not become a running kernel. Plotly/Bokeh JSON
is not evaluated as a plot. Save a static image or provide another supported
representation when the result must appear in PDF, Word or source exports.

## Includes and selections

Markdown can include a whole notebook using `:::include analysis.ipynb` and
its closing fence. [Native Typst](typst.md#including-markdown-and-notebooks)
also provides cell and tag selection with `include-notebook`.

Notebook footnote definitions can live in another Markdown cell. Structured
selection collects definitions before selecting cells, so a chosen cell can
still use such a definition. Native selection uses generated Typst content
and has additional footnote/evaluation limitations explained in the Typst
guide. Removed or unknown cells are not made selectable by a parent include.
