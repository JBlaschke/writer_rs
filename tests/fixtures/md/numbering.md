---
title: Numbering
numbering: { headings: "1.", figures: section, equations: labelled }
---

![before](images/a.png){#fig-zero}

# One {#sec-one}

![a](images/a.png){#fig-a}

Table: t {#tbl-a}

| x |
|---|
| 1 |

```rust {#lst-a caption="code"}
fn main() {}
```

:::figure kind=other caption="other" {#fig-other}
Some *other* content.
:::

$$ x = 1 $$

$$ y = 2 $$ {#eq-a}

## Sub {#sec-sub}

### Deep {#sec-deep}

# Interlude {#sec-interlude .unnumbered}

![in interlude](images/a.png){#fig-int}

## Under interlude {#sec-under}

# Two {#sec-two}

![c](images/a.png){#fig-c}

$$ z = 3 $$ {#eq-b}

See @sec-one, @fig-zero, @eq-a, @eq-a[Eq.], @eq-a[], @sec-two[Chapter] and @sec-interlude.
