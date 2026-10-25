---
title: Include root
# includes.md: markdown-dialect.md §8
---

# Included documents

:::include include/part-a.md
:::

:::include include/cycle-a.md
:::

- A list item with a forbidden nested include:

  :::include include/part-a.md
  :::
