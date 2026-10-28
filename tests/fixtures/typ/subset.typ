#import "@writer/doc:0.1.0": *
#import "@preview/mitex:0.2.7": mi, mitex
#show: writer-doc.with(title: "Subset corpus", bibliography: path("refs.bib"), toc: false, numbering: true, pdf: (system-fonts: false))

= Typed content <sec-typed>

Text with \*escaped stars\*, ~ nonbreaking space -- en dash --- em dash ... ellipsis.
A soft line break and an explicit\
hard break; #linebreak()a call break. https://example.org links automatically.

"Quotes" and 'single quotes', 5' and an apostrophe in writer's prose.

#strong[Strong] #emph[Emphasis] #underline[Underlined] #strike[Struck] #smallcaps[Small capitals] #super[super] #sub[sub] #overline[overline] #highlight[highlight].

#quote(block: false)[Inline quotation.] #link("https://example.org")[A link] <link-docs>.

#box(image("image.png", width: 1cm, alt: "Inline image")) <inline-image>

#raw("inline raw", lang: "rust") <code-inline>

```rust
let answer = 42;
```
<code-block>

#raw("block raw\nsecond line", lang: "text", block: true)

#figure(image(path("image.png"), width: 70%, height: auto, alt: "A local image"), caption: [Image caption]) <fig-image>

#figure(table(columns: 2, [A], [B], [C], [D]), caption: [Table caption]) <tbl-figure>

#figure(raw("let x = 1;", lang: "rust", block: true), caption: [Listing caption]) <lst-figure>

#figure([A generic body.], kind: image, caption: [Generic caption], numbering: none) <fig-other>

#table(
  columns: (1fr, 2fr, 1fr), align: (left + top, center, right),
  table.header([H1], [H2], [H3]), table.hline(), table.vline(),
  table.cell(colspan: 2)[Wide], [R1C3],
  table.cell(rowspan: 2)[Tall], [R2C2], [R2C3], [R3C2], [R3C3],
  table.footer([F1], [F2], [F3]),
)

#quote(block: true)[A block quote with *formatting*.]

- A bullet
- Another bullet

+ An ordered item
+ Another ordered item

/ Term: A definition.
/ Other: Another definition.

#list(tight: false, [First explicit item.], [Second explicit item.])
#enum(start: 3, tight: true, [Third.], [Fourth.])
#terms(terms.item([API], [A structured interface.]))

#align(center)[Centred paragraph.]
#block[Wrapped block.]
#par[Explicit paragraph.]
#box[Boxed inline] and #text(size: 12pt)[styled inline].

#heading(level: 2, numbering: none, outlined: false)[Unnumbered heading] <sec-plain>

$ x_1^2 + frac(1, 2) $ <eq-typed>

#math.equation(block: true, numbering: none, $ x + 1 $) <eq-plain>

A LaTeX inline #mi("x^2") and a display below.

#mitex("x^2 + y^2 = z^2", numbering: none) <eq-latex>

#metadata(none) <anchor-empty>

An inline anchor #metadata(none) <anchor-inline> appears here.

Before the footnote @writer-fn-proof; here is the note#footnote[The proof.] <writer-fn-proof> and #footnote(<writer-fn-proof>).

References @sec-typed, @fig-image[Fig.], @tbl-figure[], #ref(<sec-typed>, form: "page") and #link(<sec-typed>)[section link].

#cite(<doe2020>) #cite(<knuth1984>, supplement: [p. 12])
#cite(<doe2020>, form: "prose") #cite(<knuth1984>, form: "year") #cite(<doe2020>, form: "author") #cite(<knuth1984>, form: "full") #cite(<lee2019>, form: none)

#callout(kind: "tip", title: [Remember])[Callout body.] <callout-tip>

#listing(numbered: true, start: 4, highlight: "4-5", plain: true)[```rust
let x = 1;
let y = 2;
```] <listing-helper>

#code-cell(index: 2, count: 7, lang: "python", input: ```python
print(42)
```)[A saved output.][Another saved output.] <cell-demo>

#outline(depth: 2)
#line(length: 70%)
#v(1em)
#h(1em)
#colbreak()
#pagebreak()
