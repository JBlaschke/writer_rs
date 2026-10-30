---
title: LaTeX element tour
authors: [Jane Doe]
biblatex_backend: bibtex
bibliography: ../bib/refs.bib
toc: 2
numbering: {headings: true}
venue: Test venue
abstract: |
  Abstract text with an image ![Abstract plot](images/plot.png){width=20%}.
---

:::toc depth=2
:::

# Numbered heading {#sec-tour}

Text ≥ α, escaped TeX characters & % # _ { } ~ ^ \, *emphasis*, **strong**,
~~struck text~~ and [a named link](https://example.org/a?q=1&x=2).

Inline code `value ≥ α`, an autolink <https://example.org/α?q=1>, and inline
math $x^2 + α ≥ 0$.

## A heading with a note[^heading] and math $x^2$ {#sec-moving}

## An unnumbered heading {.unnumbered #sec-unnumbered}

A quote follows.

> Quoted prose with a second line.
> Its soft break stays inside the paragraph.

- A tight bullet item.
- Another item with a task list:
  - [ ] Pending task.
  - [x] Completed task.

3. Ordered item starting at three.
4. Another ordered item.

Term with *emphasis*
: First definition.

  Second paragraph in the definition.

![Raster plot](images/plot.png){#fig-plot width=50%}

![SVG diagram](images/diagram.svg){#fig-diagram height=2cm}

![GIF animation](images/anim.gif){#fig-animation}

:::figure {#fig-panels caption="A panel container"}
![Left panel](images/plot.png){#fig-left width=45%}

![Right panel](images/diagram.svg){#fig-right width=45%}
:::

Table: A captioned table {#tbl-tour}

| left | center | right |
|:-----|:------:|------:|
| one | two | three |

| bare | table |
|------|------:|
| alpha | 1 |

```python {.numbered start=3 highlight="3-4"}
value = "α"
print(value, "≥")
```

```rust
fn main() {
    println!("preserved source");
}
```

```python {#lst-tour caption="A captioned listing"}
print("captioned")
```

$$x = 1$$ {#eq-tour}

```math {#eq-align}
\begin{align}
a &= b \\
c &= d
\end{align}
```

$$α ≥ 0$$

:::note
A note callout.
:::

:::tip
A tip callout.
:::

:::important
An important callout.
:::

:::warning
A warning callout.
:::

:::caution
A caution callout.
:::

:::callout custom title="Boxed figures and tables"
![Boxed image](images/plot.png){#fig-boxed width=25%}

Table: A boxed table {#tbl-boxed}

| column | value |
|--------|------:|
| boxed | 2 |
:::

::: {.center .small .env-quote}
Centered, small text wrapped by a quote environment.
:::

See @sec-tour, @fig-plot, @tbl-tour, @lst-tour and @eq-tour. An explicit
supplement @eq-tour[Eq.], an empty supplement @eq-tour[], page @eq-tour{.page},
and an unnumbered heading @sec-unnumbered remain distinct.

A repeated note[^first], its repeat[^first], and an inline note^[Inline note.].

Citations [@doe2020; @lee2019, p. 12], [see @knuth1984],
[@knuth1984]{.prose}, [@knuth1984]{.year}, [@knuth1984]{.author},
[@knuth1984]{.full}, and [-@knuth1984].

:::bibliography
:::

:::raw format=latex
\LaTeX{} raw source retains its \% and {braces}.
:::

:::raw format=html
<b>Foreign raw HTML is omitted by the LaTeX emitter.</b>
:::

:::pagebreak
:::

---

[^heading]: A heading note with ~~moving strikeout~~.

[^first]: A note containing a nested note[^nested].

[^nested]: Nested note body.
