#import "@writer/doc:0.1.0": *
#let first-author = "Ada Lovelace"
#show: writer-doc.with(
  title: [A *structured* document], subtitle: "Literal metadata",
  authors: (first-author, (name: "Grace Hopper", affiliation: "Institute", email: "grace@example.org", id: "0000-0001")),
  date: datetime(year: 2026, month: 10, day: 28),
  abstract: [An abstract with _emphasis_.],
  keywords: ("Typst", "IR"), description: "An offline frontend fixture",
  lang: "en-GB", toc: 2,
  numbering: (headings: "1.1", figures: "section", equations: "labelled"),
  bibliography: path("refs.bib"), cite-style: "ieee", cite-locale: "en-GB", paper: "a4",
  toc-title: [Contents], abstract-title: "Summary", reference-section-title: [Sources],
  theme: "default", template: "template.typ", reference-doc: "reference.docx", latex-template: "template.tex",
  markdown: (:), math: (:), references: (:), html: (:), pdf: (system-fonts: false),
  docx: (:), latex: (:), typst: (strict: false), notebook: (:),
  custom-value: (enabled: true, number: -2.5, items: ("a", 3, none)),
)

= Introduction <sec-intro>

A paragraph with *strong*, _emphasis_, `inline` and $a^2$.

A forward note reference @writer-fn-later appears before its definition.

A note#footnote[Footnote body with *strong* text.] <writer-fn-later> and its repeat @writer-fn-later.
