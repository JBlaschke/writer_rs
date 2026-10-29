---
title: My report
authors: [Your name]
numbering: {headings: "1.1", figures: document, equations: labelled}
{{bibliography}}---

# Measurements {#sec-measurements}

Start with a small result and explain how it was measured.
The figure @fig-plot and table @tbl-results share the same data.

![Example measurements](figures/plot.png){#fig-plot width=70%}

Table: Example results {#tbl-results}

| Batch | Throughput |
|:------|-----------:|
| Small |       11.2 |
| Medium|       20.8 |
| Large |       30.1 |

## Method {#sec-method}

The average rate in @eq-rate is count divided by elapsed time.

$$ r = \frac{n}{t} $$ {#eq-rate}

```python {#lst-rate caption="Computing the rate"}
def rate(count, seconds):
    return count / seconds
```

{{citation}}
