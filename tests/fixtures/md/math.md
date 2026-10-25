---
title: Math sources and labels
# math.md: markdown-dialect.md §4.5
math: { katex_options: { macros: { "\\RR": "\\mathbb{R}", "\\half": "\\frac{#1}{2}" } } }
---

Inline $\frac{a}{b}$.

$$\sum_{i=0}^{n} i$$ {#eq-sum}

See @eq-sum.

```math {#eq-aligned}
\begin{aligned} a &= b + c \\ &= d \end{aligned}
```

Accents $\hat{x} \vec{v} \dot{y}$.

$$x = 1 \tag{3} \label{eq:t}$$

Macros $\RR^n + \half{x}$.

Inline $x + y$ and display $$x^2$$.

$$x = y$$ {#eq-x}

$$z = 2$$ trailing prose {#eq-misplaced}

The escaped dollar price is \$5.

# Energy $E = mc^2$

- Math inside a list: $x_1$.
