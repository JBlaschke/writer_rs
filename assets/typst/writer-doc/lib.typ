#import "callout.typ": callout, writer-term
#import "cells.typ": code-cell, include-notebook
#import "numbering.typ": writer-numbering, bibliography-term

#let language(tag) = {
    let parts = tag.replace("_", "-").split("-")
    if (
        parts.len() == 0
        or not parts.first().contains(regex("^[A-Za-z]{2,3}$"))
    ) {
        return (lang: "en", region: none)
    }
    let region = none
    let offset = if (
        parts.len() > 1 and parts.at(1).contains(regex("^[A-Za-z]{4}$"))
    ) { 2 } else { 1 }
    if (
        parts.len() > offset
        and parts.at(offset).contains(regex("^[A-Za-z]{2}$"))
    ) {
        region = upper(parts.at(offset))
    }
    (lang: lower(parts.first()), region: region)
}

#let from-config(value, config, key, fallback) = {
    if value == auto { config.at(key, default: fallback) } else { value }
}

// The resolved design is shared with HTML and DOCX. Missing inputs leave every
// built-in default intact, including standalone exported Typst documents.
#let writer-design = if "writer-design" in sys.inputs {
    json(bytes(sys.inputs.at("writer-design")))
} else { (:) }

// A dotted lookup in one design dictionary; `default` stands in for a missing
// or `none` value.
#let design-get(design, key, default) = {
    let value = design
    for part in key.split(".") {
        if type(value) != dictionary { return default }
        value = value.at(part, default: none)
    }
    if value == none { default } else { value }
}

#let from-design(key, default) = design-get(writer-design, key, default)

// The design `writer-doc` applies: its explicit `design:` argument wins over
// the `writer-design` input, which wins over the package defaults. An exported
// source passes the JSON copied beside its main file here, so a plain compile
// needs no input; `none` ignores the input and keeps the defaults.
#let design-value(design) = {
    if design == auto { writer-design }
    else if design == none { (:) }
    else {
        assert(
            type(design) == dictionary,
            message: "writer-doc: design must be auto, none or a dictionary"
        )
        design
    }
}

// Shares the applied design with `writer-div`, which the body calls outside
// `writer-doc`'s arguments; `none` until `writer-doc` records it.
#let design-state = state("writer-design", none)

// The line box Typst lays text of `size` out with: with the default
// `top-edge: "cap-height"` and `bottom-edge: "baseline"`, the cap height of
// the font in effect (its OS/2 `sCapHeight`, else its ascender; a fallback
// font's when the family is missing). Measured rather than assumed, so the
// fonts and edges actually in use decide. Needs context.
#let line-box(size) = measure(text(size: size)[H]).height

// `par.leading` for a design pitch (design.md §6.2). Typst's leading is the
// gap between one line's bottom edge and the next line's top edge, so a
// baseline pitch needs `pitch − line box`; HTML's `line-height` and Word's
// lines then agree with the PDF. The result is in ems of `size`, so that a
// multiple follows a paragraph's own size as HTML's unitless `line-height`
// and Word's `auto` lines do, and an exact pitch holds at any size. Needs
// context, except for the `0.65em` default of an unset pitch.
#let design-leading(pitch, size) = {
    if pitch == none { return 0.65em }
    let box = line-box(size)
    let target = if "multiple" in pitch { pitch.multiple * size }
        else { pitch.exact * 1pt }
    if target <= box { return 0pt }
    if "multiple" in pitch { (pitch.multiple - box / size) * 1em }
    else { target - box / size * 1em }
}

// `par.spacing` for a design paragraph spacing: Typst measures it like the
// leading, from the last baseline to the next line's cap height, so a
// paragraph break adds `spacing` to a full line pitch, as HTML's
// `margin-block` and Word's `w:after` do, only as `leading + spacing`.
// `none` keeps Typst's `1.2em`.
#let design-spacing(leading, spacing) = {
    if spacing == none { 1.2em } else { leading + spacing * 1pt }
}

#let document-date(date) = {
    if "writer-version" in sys.inputs { return auto }
    if type(date) == datetime { return date }
    if (
        type(date) == str
        and date.contains(regex("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"))
    ) {
        return datetime(
            year: int(date.slice(0, 4)), month: int(date.slice(5, 7)),
            day: int(date.slice(8, 10))
        )
    }
    auto
}

#let author-list(author, authors) = {
    let values = if type(authors) == str { (authors,) } else { authors }
    if values.len() == 0 and author != none { values = (author,) }
    values.map(value => if type(value) == str { (name: value,) } else { value })
}

#let title-block(meta) = {
    if (
        meta.title != none or meta.subtitle != none
        or meta.authors.len() > 0 or meta.date != none
    ) {
        align(center)[
            #if meta.title != none {
                text(size: 17pt, weight: "bold", meta.title)
                parbreak()
            }
            #if meta.subtitle != none {
                meta.subtitle
                parbreak()
            }
            #meta.authors.map(
                author => author.at("name", default: "")
            ).join(", ")
            #for author in meta.authors {
                let affiliation = author.at("affiliation", default: none)
                if affiliation != none {
                    parbreak()
                    text(size: 9pt, affiliation)
                }
            }
            #if meta.date != none {
                parbreak()
                if type(meta.date) == datetime {
                    meta.date.display("[year]-[month]-[day]")
                } else { meta.date }
            }
        ]
        v(1.5em)
    }
}

#let abstract-block(body, title: auto) = {
    if body != none {
        set heading(numbering: none, outlined: false)
        block(inset: (x: 2em))[
            #heading(level: 1, if title == auto { writer-term("abstract") }
                else { title })
            #body
        ]
        v(1em)
    }
}

#let numbered-layout(meta, settings, body) = {
    show: writer-numbering.with(..settings.numbering)
    title-block(meta)
    abstract-block(meta.abstract, title: settings.abstract-title)
    if settings.toc != false and settings.toc != 0 {
        outline(depth: if settings.toc == true { 3 } else { settings.toc })
    }
    body
    if settings.bibliography != none {
        std.bibliography(settings.bibliography, style: settings.style)
    }
}

#let reference-layout(meta, settings, body) = {
    set outline(title: settings.toc-title)
    // Citation entries may use another locale, but their heading stays here.
    let bib-title = settings.reference-title
    if settings.cite-locale != none and bib-title == auto {
        bib-title = bibliography-term(
            settings.lang.lang, region: settings.lang.region
        )
    }
    set std.bibliography(title: bib-title)
    if settings.cite-locale != none {
        show cite: set text(..settings.cite-locale)
        show std.bibliography: set text(..settings.cite-locale)
        numbered-layout(meta, settings, body)
    } else { numbered-layout(meta, settings, body) }
}

// Furniture lines are plain text. Only page placeholders expand, in context.
#let furniture-text(value) = context {
    let start = 0
    for item in value.matches(regex("\\{pages?\\}")) {
        value.slice(start, item.start)
        if item.text == "{page}" { counter(page).display() }
        else { str(counter(page).final().first()) }
        start = item.end
    }
    value.slice(start)
}

#let furniture-align(value) = {
    if value == "center" { center }
    else if value == "right" { right } else { left }
}

#let furniture-line(item, spec) = {
    let inherited(key, fallback) = {
        let value = item.at(key, default: none)
        if value == none { spec.at(key, default: fallback) } else { value }
    }
    let size = inherited("size", none)
    let fill = inherited("color", none)
    text(
        size: if size == none { 1em } else { size * 1pt },
        fill: if fill == none { text.fill } else { rgb(fill) },
        weight: inherited("weight", 400),
        style: if inherited("italic", false) { "italic" } else { "normal" },
        furniture-text(item.text),
    )
}

#let furniture-rule(spec) = {
    if spec != none {
        // Keep the rule clear of text even when its last baseline is the
        // paragraph's bottom edge.
        block(
            width: 100%,
            above: if spec.position == "below" { 0.3em } else { 0pt },
            below: if spec.position == "above" { 0.3em } else { 0pt },
            line(length: 100%, stroke: spec.width * 1pt + rgb(spec.color)),
        )
    }
}

// `spacing` is the document's paragraph spacing: the default gap between an
// image and the lines below it (design.md §9.1); `line-height` is the
// design's `text.line_height`, which sets the lines' leading at their size.
#let writer-furniture(
            spec, text-width, spacing: 1.2em, line-height: none
        ) = context {
    if spec == none { return none }
    let size = if spec.size == none { text.size } else { spec.size * 1pt }
    let fill = if spec.color == none { text.fill } else { rgb(spec.color) }
    set text(size: size, fill: fill, weight: spec.weight,
        style: if spec.italic { "italic" } else { "normal" })
    set par(
        first-line-indent: 0pt, spacing: 0pt, justify: false,
        leading: design-leading(line-height, size),
    )
    block(width: text-width, above: 0pt, below: 0pt)[
        #if spec.rule != none and spec.rule.position == "above" {
            furniture-rule(spec.rule)
        }
        #if spec.image != none {
            let img = spec.image
            let alignment = if img.align == none { spec.align }
                else { img.align }
            // The lines are a paragraph of their own below the image: its
            // `gap` (the paragraph spacing unless set) separates them. The
            // text box starts at the cap height, so the gap runs from the
            // image's bottom edge to the first line's capitals. The block
            // spans the text width so that `align` can place the image.
            let gap = img.at("gap", default: none)
            block(width: 100%,
                below: if gap == none { spacing } else { gap * 1pt },
                align(furniture-align(alignment), image(
                    img.path,
                    width: if img.width == none { auto }
                        else { img.width * 1pt },
                )))
        }
        #if spec.right_lines.len() > 0 {
            grid(
                columns: (1fr, auto), column-gutter: 1em,
                align: (left + top, right + top),
                spec.lines.map(item => furniture-line(item, spec))
                    .join(linebreak()),
                spec.right_lines.map(item => furniture-line(item, spec))
                    .join(linebreak()),
            )
        } else if spec.lines.len() > 0 {
            align(furniture-align(spec.align),
                spec.lines.map(item => furniture-line(item, spec))
                    .join(linebreak()))
        }
        #if spec.rule != none and spec.rule.position == "below" {
            furniture-rule(spec.rule)
        }
    ]
}

// The page background image at full page size. Typst centres a fitted image;
// an off-centre alignment scales the measured image to cover or fit the page
// and places it at the aligned offset inside a clipping box.
#let background-image(spec) = {
    let centred = (
        spec.align.horizontal == "center" and spec.align.vertical == "center"
    )
    if spec.fit == "stretch" or centred {
        image(spec.path, width: 100%, height: 100%, fit: spec.fit)
    } else {
        let factor = (left: 0, top: 0, center: 0.5, right: 1, bottom: 1)
        let natural = measure(image(spec.path))
        let ratios = (page.width / natural.width, page.height / natural.height)
        let scale = if spec.fit == "cover" { calc.max(..ratios) }
            else { calc.min(..ratios) }
        let width = natural.width * scale
        let height = natural.height * scale
        box(width: 100%, height: 100%, clip: true, place(top + left,
            dx: (page.width - width) * factor.at(spec.align.horizontal),
            dy: (page.height - height) * factor.at(spec.align.vertical),
            image(spec.path, width: width, height: height)))
    }
}

// The gap between a furniture image and its lines when the image sets none
// (design.md §9.1): the design's paragraph spacing, else an explicit
// `spacing` argument, else Typst's `1.2em` of the furniture text.
#let furniture-gap(settings) = {
    if settings.paragraph-spacing != none { settings.paragraph-spacing * 1pt }
    else if settings.spacing != auto { settings.spacing } else { 1.2em }
}

#let furniture-background(settings) = context {
    let first = counter(page).get().first() == 1
    let design = settings.design
    let background = design-get(design, "page.background", none)
    if background != none and (background.pages == "all" or first) {
        place(top + left, box(width: page.width, height: page.height,
            background-image(background.image)))
    }
    let side(key, axis) = {
        let value = settings.margin
        if type(value) == dictionary {
            value.at(key, default: value.at(axis, default: 2.5cm))
        } else { value }
    }
    let margin = (top: side("top", "y"), right: side("right", "x"),
        bottom: side("bottom", "y"), left: side("left", "x"))
    let text-width = page.width - margin.left - margin.right
    let furniture(chosen) = writer-furniture(
        chosen, text-width,
        spacing: furniture-gap(settings), line-height: settings.line-height,
    )
    let header = design-get(design, "page.header", none)
    if header != none {
        let chosen = if first { header.first } else { header.rest }
        if chosen != none {
            let offset = design-get(design, "page.header_offset", none)
            let offset = if offset == none { margin.top * 0.3 }
                else { offset * 1pt }
            place(top + left, dx: margin.left, dy: offset, furniture(chosen))
        }
    }
    let footer = design-get(design, "page.footer", none)
    if footer != none {
        let chosen = if first { footer.first } else { footer.rest }
        if chosen != none {
            let offset = design-get(design, "page.footer_offset", none)
            let offset = if offset == none { margin.bottom * 0.3 }
                else { offset * 1pt }
            let content = if chosen.size == none {
                text(size: 0.75em, furniture(chosen))
            } else { furniture(chosen) }
            place(bottom + left, dx: margin.left, dy: -offset, content)
        }
    }
}

// Leading and paragraph spacing (design.md §6.2): explicit arguments as
// given, the design's through `design-leading`, which measures the body
// font's line box and so needs a context block. The page set-up of `layout`
// and `furniture-layout` stays outside it.
#let paragraph-layout(meta, settings, body) = context {
    let leading = if settings.leading == auto {
        design-leading(settings.line-height, text.size)
    } else { settings.leading }
    set par(
        leading: leading,
        spacing: if settings.spacing == auto {
            design-spacing(leading, settings.paragraph-spacing)
        } else { settings.spacing },
    )
    reference-layout(meta, settings, body)
}

#let furniture-layout(meta, settings, body) = {
    let design = settings.design
    if (
        design-get(design, "page.header", none) != none
        or design-get(design, "page.footer", none) != none
    ) {
        set page(
            header: none, footer: none, numbering: none,
            background: furniture-background(settings),
        )
        paragraph-layout(meta, settings, body)
    } else if design-get(design, "page.background", none) != none {
        // A background alone keeps the built-in header, footer and numbering.
        set page(background: furniture-background(settings))
        paragraph-layout(meta, settings, body)
    } else { paragraph-layout(meta, settings, body) }
}

#let design-layout(meta, settings, body) = {
    if settings.link-fill != none {
        show link: set text(fill: settings.link-fill)
        furniture-layout(meta, settings, body)
    } else { furniture-layout(meta, settings, body) }
}

#let layout(meta, settings, body) = {
    set document(
        title: meta.title,
        author: meta.authors.map(it => it.at("name", default: "")),
        date: document-date(meta.date), keywords: meta.keywords,
        description: meta.description
    )
    set page(
        paper: settings.paper, margin: settings.margin, numbering: "1",
        fill: settings.background,
    )
    set text(
        font: settings.font, size: settings.size, fill: settings.text-fill,
        lang: settings.lang.lang, region: settings.lang.region
    )
    set par(justify: settings.justify)
    show std.math.equation: set text(font: "New Computer Modern Math")
    show raw: set text(font: settings.mono-font)
    design-state.update(settings.design)
    if settings.heading-font != none {
        show heading: set text(font: settings.heading-font)
        design-layout(meta, settings, body)
    } else { design-layout(meta, settings, body) }
}

#let writer-doc(
            title: none, subtitle: none, author: none, authors: (), date: none,
            abstract: none, abstract-title: auto, keywords: (),
            description: none, lang: auto, toc: auto, toc-title: auto,
            numbering: auto, bibliography: none, reference-section-title: auto,
            cite-style: auto, csl: auto, cite-locale: auto, paper: auto,
            font: auto, heading-font: auto, mono-font: auto, size: auto,
            leading: auto, spacing: auto, justify: auto, margin: auto,
            text-fill: auto, link-fill: auto, background: auto, design: auto,
            markdown: (:), math: (:), references: (:), html: (:), pdf: (:),
            docx: (:), latex: (:), typst: (:), notebook: (:), ..extra, body
        ) = {
    assert(
        extra.pos().len() == 0,
        message: "writer-doc: unexpected positional arguments"
    )
    let config = json(bytes(sys.inputs.at("writer-config", default: "{}")))
    let base-numbering = config.at("numbering", default: (:))
    let chosen-numbering = if numbering == auto { (:)}
        else if type(numbering) == bool { (headings: numbering,) }
        else { numbering }
    let numbers = (
        headings: false, figures: "document", equations: "labelled",
        ..base-numbering, ..chosen-numbering
    )
    let paths = if type(bibliography) == array { bibliography }
        else if bibliography == none { () } else { (bibliography,) }
    for item in paths {
        assert(
            type(item) != str,
            message: "writer-doc: pass bibliography: path(\"refs.bib\")"
        )
    }
    let locale = from-config(cite-locale, config, "cite_locale", "")
    let design = design-value(design)
    let from-design(key, default) = design-get(design, key, default)
    let meta = (
        title: title, subtitle: subtitle, authors: author-list(author, authors),
        date: date, abstract: abstract, keywords: keywords,
        description: description,
    )
    let body-size = if size == auto {
        from-design("text.size", 11) * 1pt
    } else { size }
    let chosen-margin = if margin != auto { margin } else {
        let sides = from-design("page.margin", (:))
        let side(key) = {
            let value = sides.at(key, default: none)
            if value == none { 2.5cm } else { value * 1pt }
        }
        (top: side("top"), right: side("right"),
            bottom: side("bottom"), left: side("left"))
    }
    let colour(key, fallback) = {
        let value = from-design(key, none)
        if value == none { fallback } else { rgb(value) }
    }
    let settings = (
        font: if font == auto {
            from-design("fonts.body.family", "Libertinus Serif")
        } else { font },
        heading-font: if heading-font == auto {
            from-design("fonts.heading.family", none)
        } else { heading-font },
        mono-font: if mono-font == auto {
            from-design("fonts.mono.family", "DejaVu Sans Mono")
        } else { mono-font },
        size: body-size,
        // `auto` leaves the leading and spacing to `paragraph-layout`, which
        // derives them from `line-height` and `paragraph-spacing` (§6.2).
        leading: leading, spacing: spacing,
        line-height: from-design("text.line_height", none),
        paragraph-spacing: from-design("text.paragraph_spacing", none),
        justify: if justify == auto {
            from-design("text.justify", true)
        } else { justify },
        margin: chosen-margin,
        text-fill: if text-fill == auto {
            colour("colors.text", black)
        } else { text-fill },
        link-fill: if link-fill == auto {
            colour("colors.link", none)
        } else { link-fill },
        background: if background == auto {
            colour("colors.background", auto)
        } else { background },
        design: design,
        lang: language(from-config(lang, config, "lang", "en")),
        toc: from-config(toc, config, "toc", false), toc-title: toc-title,
        numbering: numbers, bibliography: bibliography,
        paper: from-config(
            paper, pdf, "paper", from-design(
                "page.paper", config.at("paper", default: "a4")
            )
        ),
        style: if cite-style != auto { cite-style }
            else if csl != auto { csl } else { "ieee" },
        cite-locale: if locale == "" { none } else { language(locale) },
        abstract-title: abstract-title,
        reference-title: reference-section-title,
    )
    layout(meta, settings, body)
}

#let listing-raw(body) = {
    if body.func() == raw { return body }
    if body.has("children") {
        for child in body.children {
            let found = listing-raw(child)
            if found != none { return found }
        }
    }
    none
}

#let decimal-lte(a, b) = {
    a.len() < b.len() or (a.len() == b.len() and a <= b)
}

// Match CodeAttrs' u64 bounds without Typst's narrower integer conversion.
// One malformed item invalidates the entire range list.
#let highlight-ranges(spec) = {
    let ranges = ()
    for item in spec.replace("|", ",").split(",") {
        let bounds = item.split("-").map(part => part.trim())
        if (
            bounds.len() == 0 or bounds.len() > 2
            or not bounds.all(part => part.contains(regex("^[0-9]+$")))
        ) { return () }
        let first = bounds.first().replace(regex("^0+"), "")
        let last = bounds.last().replace(regex("^0+"), "")
        if (
            first == "" or last == "" or not decimal-lte(first, last)
            or not decimal-lte(last, "18446744073709551615")
        ) { return () }
        ranges.push((first, last))
    }
    ranges
}

#let listing-grid(code, numbered, start, ranges) = context {
    let rows = ()
    // These lines are highlighted together by Typst, preserving lexer state
    // across multiline strings and comments. Never re-highlight line by line.
    for line in code.lines {
        let number = start + line.number - 1
        if numbered {
            rows.push([
                #text(fill: rgb("#666666"), str(number))#box(height: 0.9em)
            ])
        }
        let display = str(number)
        let highlighted = ranges.any(range => (
            decimal-lte(range.first(), display)
            and decimal-lte(display, range.last())
        ))
        rows.push(grid.cell(
            fill: if highlighted { rgb("#fff1b8") } else { none },
            [#line.body#box(height: 0.9em)]
        ))
    }
    block(width: 100%, inset: 0.6em, fill: rgb("#f7f7f7"))[
        #set text(
            font: text.font,
            size: 0.9em,
        )
        // Put leading inside cells so adjacent highlights remain continuous.
        #context grid(
            columns: if numbered { (auto, 1fr) } else { (1fr,) },
            inset: (y: par.leading / 2),
            column-gutter: 0.8em, align: left, ..rows
        )
    ]
}

#let listing(numbered: false, start: 1, highlight: "", plain: false, body) = {
    let code = listing-raw(body)
    assert(code != none, message: "listing: expected one raw block")
    let content = if plain { raw(code.text, lang: none, block: true) }
        else { code }
    if not numbered and highlight == "" { content } else {
        let ranges = highlight-ranges(highlight)
        show raw: it => listing-grid(it, numbered, start, ranges)
        content
    }
}

// A body block in one of `design`'s named styles (design.md §8), or `body`
// itself when the style is unknown.
#let writer-styled(design, style, body) = {
    let spec = design-get(design, "styles", (:)).at(style, default: none)
    if spec == none { return body }
    let text-opts = (:)
    let par-opts = (:)
    if spec.font != none {
        let family = if spec.font in ("body", "heading", "mono") {
            design-get(design, "fonts." + spec.font + ".family", none)
        } else { spec.font }
        if family != none { text-opts.insert("font", family) }
    }
    if spec.size != none { text-opts.insert("size", spec.size * 1pt) }
    if spec.color != none { text-opts.insert("fill", rgb(spec.color)) }
    if spec.weight != none { text-opts.insert("weight", spec.weight) }
    if spec.italic != none {
        text-opts.insert("style", if spec.italic { "italic" } else { "normal" })
    }
    if spec.align != none {
        par-opts.insert("justify", spec.align == "justify")
    }
    let aligned = if spec.align in ("left", "center", "right") {
        align((left: left, center: center, right: right).at(spec.align), body)
    } else { body }
    let inner = {
        set text(..text-opts)
        set par(..par-opts)
        if spec.line_height == none { aligned } else {
            // The style's pitch, measured on its own font and size (§6.2),
            // with the design's paragraph spacing on top of it between the
            // block's paragraphs; an unset spacing stays inherited.
            context {
                let leading = design-leading(spec.line_height, text.size)
                let spacing = design-get(design, "text.paragraph_spacing", none)
                set par(leading: leading, ..if spacing == none { (:) }
                    else { (spacing: design-spacing(leading, spacing)) })
                aligned
            }
        }
    }
    block(
        above: if spec.space_before != none { spec.space_before * 1pt }
            else { auto },
        below: if spec.space_after != none { spec.space_after * 1pt }
            else { auto },
        breakable: not spec.keep_together,
        sticky: spec.keep_with_next,
        width: 100%, inner
    )
}

// Preserve div semantics for templates; named design styles are applied in T85.
// The applied design comes from `writer-doc` through `design-state`; a body
// shown without `writer-doc` reads the `writer-design` input directly.
#let writer-div(style: none, classes: (), body) = {
    if style == none { return body }
    context {
        let design = design-state.get()
        if design == none { design = writer-design }
        writer-styled(design, style, body)
    }
}
