// Customize this delegating template and set [pdf] template to its path.
//
// The look comes from the project's [design]: writer-doc applies its tokens
// and its named styles (headings, title block, contents list, captions,
// table cells). Write here only what the design does not say. A rule placed
// after `show: base.writer-doc.with(..args)` overrides the package and the
// design key by key; a rule placed before it loses to them.
//
// Arguments of writer-doc. Metadata: title, subtitle, author/authors, date,
// abstract, keywords, description, lang. Layout: toc, toc-title,
// abstract-title, reference-section-title, numbering, paper, title-block
// (auto draws the built-in title block, none draws nothing, a function
// (meta, theme) => content draws its result, for example a cover page).
// Typography: font, heading-font, mono-font, size, leading, spacing, justify,
// margin, text-fill, link-fill, background. Design: design (auto reads the
// writer-design input, none keeps the package defaults, a dictionary replaces
// the input; exported sources pass their copied design here). References:
// bibliography, cite-style/csl, cite-locale. Config maps: markdown, math,
// references, html, pdf, docx, latex, typst, notebook. Ignored routing
// options: theme, template, reference-doc, latex-template. Other named
// arguments are accepted as extras. To set an argument for every document,
// pass it after ..args: base.writer-doc.with(..args, title-block: cover).
//
// `theme` is the design as Typst values, `none` for an unset key:
// theme.colors.accent, theme.fonts.heading, theme.text.size,
// theme.page.margin.top, theme.styles. base.style-tokens(theme, "Title") is
// one named style; its `text` field holds the arguments of text(..) for its
// face.
#import "@writer/doc:0.5.0" as base

#let writer-doc(..args, body) = {
    let theme = base.design-tokens(..args)
    show: base.writer-doc.with(..args)

    // Rules written here override the package and the design key by key.
    show heading: set text(fill: rgb("#1a3a8a"))
    body
}
