#import "@writer/doc:0.1.0": *

#show: writer-doc.with(
    title: [A native Typst document],
    authors: ("Writer examples",),
    toc: true,
    numbering: (headings: "1.1", figures: "document", equations: "labelled"),
)

= Native source and structured content <sec-native>

Writer compiles this file directly for PDF. HTML and Word use the structured
subset, including native Typst math such as $a^2 + b^2 = c^2$.

#figure(
    image("figures/flow.svg", width: 55%),
    caption: [One source can produce a native PDF and structured HTML or Word.],
) <fig-flow>

See @fig-flow for the output paths. A footnote is also shared across outputs.
#footnote[This note is part of the same document.]

= An editable equation <sec-equation>

$ integral_0^1 x^2 dif x = 1/3 $ <eq-integral>

@eq-integral is compiled by Typst for PDF and converted through MathML for
editable Word math.

#callout(kind: "note", title: [The structured subset])[
    Native PDF supports the complete Typst language. Loops, computed content
    and other unsupported source constructs produce diagnostics in the
    structured HTML and Word views.
]
