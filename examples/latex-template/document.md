---
title: A small methods report
subtitle: One source, a custom LaTeX presentation
authors: [Writer contributors]
date: "2026-10-29"
venue: Methods & Results
numbering: {headings: "1.", figures: document, equations: labelled}
abstract: |
  A compact report with a figure, table, equation and highlighted code.
  Its template adds a venue line and a shared accent colour.
---

# Measurements {#sec-measurements}

We measured three batches over the same four-second interval.
@fig-throughput shows their relative throughput; @tbl-batches keeps the
underlying counts editable.

![Throughput rises from two to four to six jobs per second.](figures/throughput.svg){#fig-throughput width=65%}

Table: Completed jobs in each batch {#tbl-batches}

| Batch | Jobs | Seconds |
|:------|-----:|--------:|
| Small |    8 |       4 |
| Medium|   16 |       4 |
| Large |   24 |       4 |

:::tip title="Keep the source reusable"
The Markdown holds the results. The template controls the venue line,
link colours and highlighted keywords.
:::

# Reproduce the calculation {#sec-method}

The rate in @eq-rate is the completed job count divided by elapsed time.

$$ r = \frac{n}{t} $$ {#eq-rate}

```python {#lst-rate caption="Computing the rate" .numbered}
def rate(jobs, seconds):
    return jobs / seconds

print(rate(24, 4))
```

The template reads `venue` from front matter through `vars.venue | tex`.
The ampersand in **Methods & Results** is escaped for TeX automatically.
