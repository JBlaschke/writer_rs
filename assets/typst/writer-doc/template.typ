// Customize this delegating template and set [pdf] template to its path.
// Metadata: title, subtitle, author/authors, date, abstract, keywords,
// description, lang. Layout: toc, toc-title, abstract-title,
// reference-section-title, numbering, paper. References: bibliography,
// cite-style/csl, cite-locale. Config maps: markdown, math, references, html,
// pdf, docx, latex, typst, notebook. Ignored routing options: theme, template,
// reference-doc, latex-template. Other named arguments are accepted as extras.
#import "@writer/doc:0.1.0" as base

#let writer-doc(..args, body) = {
    show heading: set text(fill: rgb("#1a3a8a"))
    base.writer-doc(..args, body)
}
