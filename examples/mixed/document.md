---
title: A report from Markdown and a notebook
authors: [Writer examples]
toc: true
numbering: { headings: "1.1", figures: document }
---

# Overview {#sec-mixed-overview}

This report combines Markdown sections, a nested file and saved notebook
outputs. References resolve across their file boundaries: the method is in
@sec-mixed-method, the plot is @fig-mixed-rolling and the table is
@tbl-mixed-summary.[^1]

:::include parts/context.md
:::

:::include analysis/results.ipynb
:::

# Discussion {#sec-mixed-discussion}

The plot and table come from the notebook's stored outputs. Building this
report reads those results without executing Python. The diagram in
@fig-mixed-flow comes from the nested Markdown section.

[^1]: This note belongs to the root Markdown file.
