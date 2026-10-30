#import "@writer/doc:{{writer_version}}": *
#show: writer-doc.with(
    title: "My report", authors: ("Your name",),
    numbering: (headings: "1.1", figures: "document", equations: "labelled"),
{{bibliography}})

= Measurements <sec-measurements>

Start with a small result and explain how it was measured.
The figure @fig-plot and table @tbl-results share the same data.

#figure(
    image("figures/plot.png", width: 70%),
    caption: [Example measurements],
) <fig-plot>

#figure(
    table(
        columns: 2, align: (left, right),
        table.header([Batch], [Throughput]),
        [Small], [11.2], [Medium], [20.8], [Large], [30.1],
    ),
    caption: [Example results],
) <tbl-results>

== Method <sec-method>

The average rate in @eq-rate is count divided by elapsed time.

$ r = frac(n, t) $ <eq-rate>

#figure(
    ```python
    def rate(count, seconds):
        return count / seconds
    ```,
    caption: [Computing the rate],
) <lst-rate>

{{citation}}
