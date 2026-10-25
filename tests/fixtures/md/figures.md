---
title: Image attribute variants
# figures.md: markdown-dialect.md §4.3
---

![Caption with *emph*](images/plot.png "The title"){#fig:x width=50%}

![Caption](images/plot.png){#fig:y width='50%'}

![Caption](images/plot.png){src=images/diagram.svg}

![Caption](images/plot.png){#fig:z .cls} trailing text

![One](images/plot.png) ![Two](images/diagram.svg)

![](images/plot.png){.bare}

[![nested img](images/plot.png)](https://example.org){.wrap}

[![alt](images/plot.png){.x width=10%}](https://example.org){.wrap}

![](images/plot.png)

:::figure {#fig-panels caption="Two panels"}
![Left](images/plot.png){#fig-left width=45%}

![Right](images/diagram.svg){#fig-right width=45%}
:::

![Invalid dimension](images/plot.png){width=huge}
