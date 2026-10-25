---
title: Document directives
# directives.md: markdown-dialect.md §5
---

::::note title="Outer container"
Outer content.

:::tip
Nested content with a longer outer fence.
:::
::::

:::figure {#fig-container caption="A container figure"}
![Figure content](images/plot.png)
:::

:::pagebreak
:::

:::toc depth=2
:::

:::bibliography
:::

:::raw format=latex
\smallskip
:::

:::mystery .custom
An unknown directive is retained as a generic container.
:::

:::notes
Presenter notes are dropped.
:::

:::comment
Comments are dropped.
:::

:::

:::warning title="An unclosed directive"
The document deliberately ends inside this directive.
