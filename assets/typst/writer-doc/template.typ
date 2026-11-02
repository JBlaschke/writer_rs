// Customize this delegating template and set [pdf] template to its path.
// Metadata: title, subtitle, author/authors, date, abstract, keywords,
// description, lang. Layout: toc, toc-title, abstract-title,
// reference-section-title, numbering, paper. Typography: font, heading-font,
// mono-font, size, leading, spacing, justify, margin, text-fill, link-fill,
// background. References: bibliography,
// cite-style/csl, cite-locale. Config maps: markdown, math, references, html,
// pdf, docx, latex, typst, notebook. Ignored routing options: theme, template,
// reference-doc, latex-template. Other named arguments are accepted as extras.
// A template that defines its own layout can read the same resolved design:
// let design = if "writer-design" in sys.inputs {
//     json(bytes(sys.inputs.at("writer-design")))
// } else { (:) }
#import "@writer/doc:0.2.0" as base

#let writer-doc(..args, body) = {
    show heading: set text(fill: rgb("#1a3a8a"))
    base.writer-doc(..args, body)
}
