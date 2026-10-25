---
title: Tables and captions
# tables.md: markdown-dialect.md §4.4
---

Table: A caption before the table {#tbl-before}

| left | center | right |
|:-----|:------:|------:|
| one  | two    | three |

| item | count |
|------|------:|
| alpha | 1 |

Table: A caption after the table {#tbl-after}

Table: An orphan caption {#tbl-orphan}

A paragraph separates the orphan from any table.

| pandoc | caption |
|--------|---------|
| a      | b       |
: A pandoc caption is retained with a compatibility warning.

:::figure {#tbl-wrapped caption="A labelled table"}
| column | value |
|--------|------:|
| x      | 2 |
:::
