---
title: Recoverable attribute syntax
# attribute-recovery.md: markdown-dialect.md §3.2
---

# Single quotes {#sec-recovery title='Recovered title'}

![Single-quoted width](images/plot.png){width='50%'}

[A bare path](https://example.org){path=images/plot.png}

`A bare pipe`{highlight=1-3|5}

[A bare key](https://example.org){bare}

`Comma-separated classes`{.first,.second}

[Consecutive blocks](https://example.org){.a}{k='v'}

{this is not an attribute block}
