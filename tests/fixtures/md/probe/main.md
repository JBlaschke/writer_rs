---
title: Probe document
author: J. Blaschke
---

# Introduction {#intro .wide key=value}

Setext heading {#setext .s}
---------------------------

A paragraph with *emph*, **strong**, ~~strike~~, H~2~O, x^2^, `code`{.lang k=v},
a [link](https://example.com "Title"){.btn target=_blank}, an inline ![img](a.png){width=50%},
a footnote[^one], inline math $E=mc^2$ and display inline $$a+b$$ and a literal \$5.
Hard break here\
and two-space break here  
then soft
break.

- tight one
- tight two
  - nested a
  - nested b

- loose one

- loose two

3. third
4. fourth
   1) nested paren

- [ ] todo
- [x] done
- [X] DONE

> quoted paragraph
>
> second quoted paragraph

> [!NOTE] Custom title
> Note body with **strong**.

> [!warning]
> lowercase warning, no title

```python {#code1 .numbered lines="1-3"}
print("hi")
```

```
no info fence
```

$$
\int_0^1 x\,dx
$$

```math
\sum_i i
```

| Left | Center | Right | None |
|:-----|:------:|------:|------|
| a    | b      | c     | d    |

Term one

: Details for term one

Term two

: Details for term two

    More details.

[^one]: The footnote text.

    Second paragraph of the footnote.

***

<div class="raw">
raw html block
</div>

<!-- an html comment -->

:::callout note title="Hi there" .big #c1
Callout body.

- with a list
:::

::: {.callout .warning id=w1 title="Pandoc form"}
Pandoc-style info string.
:::

:::include part2.ipynb
:::

Final paragraph.
