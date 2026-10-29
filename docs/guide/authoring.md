# Authoring documents

Writer accepts CommonMark and GitHub-flavored Markdown, with document
features such as captions, references, citations, footnotes and includes.
The [basic example](../../examples/basic/README.md) shows a complete report.

## Front matter and headings

Front matter starts on the first line and uses YAML:

```yaml
---
title: A small study
authors: [A. Researcher]
date: 2026-10-29
toc: true
numbering:
  headings: "1.1"
  figures: section
  equations: labelled
---
```

The title comes from `title`, not the first heading. `toc: true` lists headings
through level three; an integer from one to six chooses another depth. Use
`.notoc` to omit a heading from the TOC, or `.unnumbered` to omit its number.

```markdown
# Results {#sec-results}
## Supplementary details {.unnumbered .notoc}
```

An unlabelled heading gets an automatic identifier. Prefer an explicit stable
label for references that other sections depend on. Labels are unique across
the whole document, including included files.

A line of `---` immediately after text is a setext heading underline. Leave a
blank line before it when you want a thematic break instead.

## Figures, tables, equations and code

An image alone in a paragraph becomes a figure. Its alt text is the caption;
attributes provide its label and size. Paths are relative to the source file
that contains the image.

```markdown
![Throughput by node count](figures/plot.png){#fig-throughput width=70%}

Table: Measured throughput {#tbl-results}

| Nodes | GB/s |
|------:|-----:|
|     1 | 11.2 |
|     2 | 20.8 |

$$ T(n) = \frac{W}{n} + c $$ {#eq-model}
```

Inline equations use `$...$`. A display equation and its label should occupy
their own paragraph. Equations are numbered when labelled by default;
`numbering.equations` can choose all or none.

Fenced code supports a language, label, caption and line settings:

````markdown
```python {#lst-mean caption="Calculate the mean" .numbered highlight="2"}
samples = [11.2, 20.8]
mean = sum(samples) / len(samples)
print(mean)
```
````

Use `start=10` to change the first displayed line number and `.plain` to
disable highlighting. `highlight="1-3,5"` names displayed line numbers.
These line settings apply to code blocks and notebook cell input, not inline
code. See the [Markdown contract](../spec/markdown-dialect.md) for exact
attribute syntax and directive behavior.

## References and citations

Reference a label with `@fig-throughput`, `@tbl-results`, `@eq-model` or
`@sec-results`. Writer supplies the appropriate name and number. Escape an
ordinary at-sign as `\@`, or disable scanning with `[markdown] references =
false` when a document mainly contains literal handles.

Bibliography keys use the same `@key` syntax. Configure actual source files:

```yaml
---
bibliography: refs.bib
cite_style: ieee
---
```

For example, `refs.bib` can contain:

```bibtex
@book{writer2026,
  author = {Researcher, Alice},
  title = {A Reproducible Study},
  year = {2026},
  publisher = {Example Press}
}
```

```markdown
See @writer2026.
The comparison follows [see @writer2026, p. 12].
```

Use `[@first; @second, p. 12]` for a group. Each item must resolve to a
bibliography key or the group must consist of document labels; mixing keys
and labels in one group is an error. Keep labels and bibliography keys distinct
to avoid ambiguous names. Multiple bibliography files must also have unique
keys; a conflict is reported at the first conflicting file.

Citation modes are `.normal`, `.prose`, `.year`, `.author`, `.full` and `.none`:
write `@writer2026{.prose}` for an author-in-prose form. `[-@writer2026]` is a
year citation; it does not hide the citation. `.none` includes the work in the
bibliography without an inline citation. The chosen style controls formatting;
a note style creates citation notes where appropriate. LaTeX uses its selected
citation command family and can report modes it approximates.

Writer appends a bibliography when there are rendered entries unless the
document places it explicitly:

```markdown
:::bibliography
:::
```

`.page` requests a page reference for a document label. Its useful result
depends on the output: Word and typeset formats can provide page references,
while HTML uses a normal reference and reports that limitation.

## Notes and callouts

Footnotes can be named or inline:

```markdown
Record the environment.[^environment] An inline note also works.^[Details.]

[^environment]: The same compiler and input data were used for each run.
```

Named Markdown footnotes are scoped to their source file, so an included file
can have its own `[^1]`. Repeated references point back to the same note.

```markdown
> [!NOTE] Reproducibility
> Record the compiler version.

:::warning title="Check the quota"
Requests above eight nodes need approval.
:::
```

Directives are containers and need closing fences. Even empty directives use
two lines, such as `:::pagebreak` followed by `:::`. When nesting directives,
give the outer one a longer fence than the inner one.

## Include another source

At the document's top level:

```markdown
:::include parts/method.md
:::

:::include analysis/results.ipynb
:::
```

Includes can be nested in included files. Included headings and figures join
the document's label namespace, and their resources resolve relative to their
own source. Included title and author metadata do not replace the root's
metadata. Include cycles and excessive nesting produce findings. Includes
inside callouts, lists or other nested containers are not expanded.

The [mixed example](../../examples/mixed/README.md) demonstrates nested source
files, a saved notebook and references across file boundaries.

## Target-specific content

Raw fences insert content for one output language:

````markdown
```{=latex}
\clearpage
```
````

Other targets skip that raw content; check reports the skip for the selected
target. Prefer a shared directive such as `:::pagebreak` when every output
should receive the same document operation. Supported raw formats include
HTML, LaTeX, Typst and OpenXML; raw payloads must be valid for their target.

Use [check](troubleshooting.md#check-before-sharing) to find misplaced labels,
invalid directives, missing assets and unsupported target constructs.
