---
title: References and citations
# references.md: markdown-dialect.md §7
bibliography: ../bib/refs.bib
numbering: {headings: "1.1"}
---

# A numbered section {#sec-x}

See @sec-x, @sec-x[Chapter], @fig-x[Fig.], @eq-x{.page}, and @eq-x[].

![A referenced figure](images/plot.png){#fig-x}

$$x = 1$$ {#eq-x}

## An unnumbered heading {.unnumbered #sec-unnumbered}

A reference to an unnumbered heading: @sec-unnumbered.

## Results and discussion

See [the results](#results-and-discussion).

![First figure](images/plot.png){#fig-a}

![Second figure](images/diagram.svg){#fig-b}

A label group [@fig-a; @fig-b].

Citations [@doe2020; @lee2019, p. 12]{.prose}, [-@knuth1984], and [see @doe2020].

Write \@literal for an at sign and keep the dot after @x.

### A trailing punctuation target {#x}

### A duplicate label {#sec-duplicate}

### Another duplicate label {#sec-duplicate}

### A reserved label {#fn-1}
