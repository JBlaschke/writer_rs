---
title: Citations and bibliography
bibliography: ../bib/refs.bib
cite_style: apa
numbering: {headings: "1.1"}
---

# A document label {#sec-only}

A label reference @sec-only and a bibliography citation @doe2020.

Normal [@doe2020]{.normal}.

Prose [@doe2020]{.prose}.

Year [@doe2020]{.year} and the negative shorthand [-@doe2020].

Author [@lee2019]{.author}.

Full [@knuth1984]{.full}.

Hidden [@knuth1984]{.none} still lists the entry.

A custom locator [see @knuth1984, p. 12].

A prefix starts a new subgroup [@doe2020; see @lee2019].

A prefix before a shared subgroup [see @doe2020; @lee2019].

Bare citations merge across spaces: @doe2020 @lee2019.

Bracketed citations also merge: [@doe2020] [@lee2019].

Two label references [@sec-only; @sec-only].

A group mixing a label and a bibliography key [@sec-only; @doe2020].

An unknown bibliography key @missing-key.

:::bibliography
:::
