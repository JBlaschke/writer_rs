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

#let from-design(key, default) = {
    let value = writer-design
    for part in key.split(".") {
        if type(value) != dictionary { return default }
        value = value.at(part, default: none)
    }
    if value == none { default } else { value }
}

#let design-leading(pitch, size) = {
    if pitch == none { return 0.65em }
    // Typst's leading is a gap rather than a baseline pitch. The 0.8 factor
    // approximates font ascent/descent and matches the calibrated letter.
    calc.max(0pt, if "multiple" in pitch {
        (pitch.multiple - 0.8) * size
    } else { pitch.exact * 1pt - 0.8 * size })
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

#let design-layout(meta, settings, body) = {
    if settings.link-fill != none {
        show link: set text(fill: settings.link-fill)
        reference-layout(meta, settings, body)
    } else { reference-layout(meta, settings, body) }
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
    set par(
        justify: settings.justify, leading: settings.leading,
        spacing: settings.spacing,
    )
    show std.math.equation: set text(font: "New Computer Modern Math")
    show raw: set text(font: settings.mono-font)
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
            text-fill: auto, link-fill: auto, background: auto,
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
        leading: if leading == auto {
            design-leading(from-design("text.line_height", none), body-size)
        } else { leading },
        spacing: if spacing == auto {
            let value = from-design("text.paragraph_spacing", none)
            if value == none { 1.2em } else { value * 1pt }
        } else { spacing },
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

#let listing-grid(code, numbered, start, ranges) = {
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
            font: from-design("fonts.mono.family", "DejaVu Sans Mono"),
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

// Preserve div semantics for templates; named design styles are applied in T85.
#let writer-div(style: none, classes: (), body) = {
    let spec = if style != none {
        from-design("styles", (:)).at(style, default: none)
    } else { none }
    if spec == none { return body }
    let text-opts = (:)
    let par-opts = (:)
    if spec.font != none {
        let family = if spec.font in ("body", "heading", "mono") {
            from-design("fonts." + spec.font + ".family", none)
        } else { spec.font }
        if family != none { text-opts.insert("font", family) }
    }
    if spec.size != none { text-opts.insert("size", spec.size * 1pt) }
    if spec.color != none { text-opts.insert("fill", rgb(spec.color)) }
    if spec.weight != none { text-opts.insert("weight", spec.weight) }
    if spec.italic != none {
        text-opts.insert("style", if spec.italic { "italic" } else { "normal" })
    }
    if spec.line_height != none {
        let size = if spec.size != none { spec.size }
            else { from-design("text.size", 11) }
        par-opts.insert("leading", design-leading(spec.line_height, size * 1pt))
    }
    if spec.align != none {
        par-opts.insert("justify", spec.align == "justify")
    }
    let inner = {
        set text(..text-opts)
        set par(..par-opts)
        if spec.align in ("left", "center", "right") {
            let position = (left: left, center: center, right: right).at(spec.align)
            align(position, body)
        } else { body }
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
