// The resolved design (design.md §3.3) inside Typst: the input, the rules that
// turn its values into Typst's, typed tokens for templates, and the named
// styles `writer-doc` applies to its own elements (design.md §8.7).

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
// context, except for the `0.65em` default of an unset pitch. `box` is the
// line box of another font than the one in effect (a named style's).
#let design-leading(pitch, size, box: auto) = {
    if pitch == none { return 0.65em }
    let box = if box == auto { line-box(size) } else { box }
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

// A style name's normalised key (design.md §8.1): its ASCII letters and
// digits in lower case, so `Heading-1` names `heading 1`.
#let style-key(name) = lower(name.replace(regex("[^A-Za-z0-9]"), ""))

// The design's style of this name, compared by normalised key, or `none`.
#let design-style(design, name) = {
    let wanted = style-key(name)
    for (declared, spec) in design-get(design, "styles", (:)) {
        if style-key(declared) == wanted { return spec }
    }
    none
}

#let style-value(spec, key) = {
    if spec == none { none } else { spec.at(key, default: none) }
}

// `top` laid over `base`, key by key: what Word's `basedOn` does for a
// style that writer bases on another one (design.md §8.7).
#let style-over(base, top) = {
    if base == none { return top }
    if top == none { return base }
    let merged = base
    for (key, value) in top {
        if value != none and value != false { merged.insert(key, value) }
    }
    merged
}

// Word reaches a weight it has no flag for by the face's own name
// ("Source Sans 3 SemiBold"). Typst files that face under its family, having
// trimmed the weight name when it loaded the font, so a trailing weight name
// becomes the family and that weight.
#let face-weights = (
    thin: 100, hairline: 100, extralight: 200, ultralight: 200, light: 300,
    regular: 400, medium: 500, semibold: 600, demibold: 600, bold: 700,
    extrabold: 800, ultrabold: 800, black: 900, heavy: 900,
)

// The family and weight Typst selects a style's face by: a role names the
// design's family for it, and the heavier of a face name's weight and the
// style's own `weight` holds.
#let style-face(design, spec) = {
    let font = style-value(spec, "font")
    let weight = style-value(spec, "weight")
    if font in ("body", "heading", "mono") {
        font = design-get(design, "fonts." + font + ".family", none)
    }
    if font != none {
        let words = font.split(" ")
        let named = face-weights.at(
            lower(words.last().replace("-", "")), default: none
        )
        if words.len() > 1 and named != none {
            font = words.slice(0, -1).join(" ")
            weight = if type(weight) == int { calc.max(weight, named) }
                else { named }
        }
    }
    (font: font, weight: weight)
}

// The `text` arguments of a style: its set keys only, so that an unset key
// keeps whatever applies around it.
#let style-text(design, spec) = {
    let opts = (:)
    let face = style-face(design, spec)
    if face.font != none { opts.insert("font", face.font) }
    if face.weight != none { opts.insert("weight", face.weight) }
    let size = style-value(spec, "size")
    if size != none { opts.insert("size", size * 1pt) }
    let colour = style-value(spec, "color")
    if colour != none { opts.insert("fill", rgb(colour)) }
    let italic = style-value(spec, "italic")
    if italic != none {
        opts.insert("style", if italic { "italic" } else { "normal" })
    }
    opts
}

// A style's leading (§6.2) on its own font and size; `fallback`, the leading
// around it, when it sets no `line_height`. Needs context.
#let style-leading(design, spec, fallback) = {
    let pitch = style-value(spec, "line_height")
    if pitch == none { return fallback }
    let opts = style-text(design, spec)
    design-leading(
        pitch, opts.at("size", default: text.size),
        box: measure(text(..opts)[H]).height
    )
}

// What a style sets on the element it styles, as the arguments of
// `apply-rules`' show-set rules (design.md §8.7). Block spacing adds the
// style's leading, as `design-spacing` does between paragraphs, so the gap
// to the next baseline is a full line pitch plus the space, as in HTML and
// Word. Needs context.
#let style-rules(design, spec, leading) = {
    let own = style-leading(design, spec, leading)
    let align = style-value(spec, "align")
    let par = (:)
    if align != none { par.insert("justify", align == "justify") }
    if style-value(spec, "line_height") != none { par.insert("leading", own) }
    let block = (:)
    let before = style-value(spec, "space_before")
    let after = style-value(spec, "space_after")
    if before != none { block.insert("above", own + before * 1pt) }
    if after != none { block.insert("below", own + after * 1pt) }
    if style-value(spec, "keep_with_next") == true {
        block.insert("sticky", true)
    }
    if style-value(spec, "keep_together") == true {
        block.insert("breakable", false)
    }
    (
        text: style-text(design, spec), par: par, block: block,
        align: if align in ("left", "center", "right") {
            ((left: left, center: center, right: right).at(align),)
        } else { () },
    )
}

// Marks the contents title: `show outline: set heading(supplement: ..)`
// makes it the one heading `heading.where(supplement: ..)` selects.
#let contents-title = [writer-contents-title]

// The show-set targets of the vocabulary styles (design.md §8.7), outermost
// first: a later entry wins over an earlier one key by key, the way Word's
// `Heading 1 Unlisted` is based on `heading 1`. Needs context.
#let vocabulary(design, leading) = {
    let style(name) = design-style(design, name)
    let entry(target, spec) = (
        target: target, ..style-rules(design, spec, leading)
    )
    let inline(target, spec) = (..entry(target, spec), block: (:))
    let levels = range(1, 7)
    (
        ..levels.map(level => entry(
            heading.where(level: level), style("Heading " + str(level))
        )),
        ..levels.map(level => entry(
            heading.where(level: level, outlined: false),
            style("Heading " + str(level) + " Unlisted")
        )),
        // Word bases `TOC Heading` on `heading 1`. The title is an unlisted
        // level-1 heading as well, so `Heading 1` is laid over that style
        // again before `TOC Heading`.
        entry(
            heading.where(supplement: contents-title),
            style-over(style("Heading 1"), style("TOC Heading"))
        ),
        ..levels.map(level => entry(
            outline.entry.where(level: level), style("toc " + str(level))
        )),
        inline(figure.caption, style("Caption")),
        inline(figure.caption.where(kind: table), style("Table Caption")),
        inline(figure.caption.where(kind: image), style("Image Caption")),
        inline(table.cell, style("Compact")),
    ).filter(rules => (
        rules.text.len() + rules.par.len() + rules.block.len()
            + rules.align.len() > 0
    ))
}

// Table cells take `Compact`'s spacing as their inset: a line of the pitch
// with the space above and below it, the text box (cap height to baseline)
// in the middle of the line. Needs context.
#let cell-inset(design, leading) = {
    let spec = design-style(design, "Compact")
    let own = style-leading(design, spec, leading)
    let inset = (:)
    let before = style-value(spec, "space_before")
    let after = style-value(spec, "space_after")
    if before != none { inset.insert("top", own / 2 + before * 1pt) }
    if after != none { inset.insert("bottom", own / 2 + after * 1pt) }
    if inset.len() == 0 { (:) } else { (inset: inset) }
}

#let apply-rules(entries, body) = {
    if entries.len() == 0 { return body }
    let rules = entries.first()
    show rules.target: set text(..rules.text)
    show rules.target: set par(..rules.par)
    show rules.target: set block(..rules.block)
    show rules.target: set align(..rules.align)
    apply-rules(entries.slice(1), body)
}

// Applies the design's vocabulary styles to `body` as show-set rules, so a
// rule that a template adds after `show: writer-doc.with(..)` still wins key
// by key. `leading` is the document's leading. Needs context.
#let vocabulary-layout(design, leading, body) = {
    let entries = vocabulary(design, leading)
    set table(..cell-inset(design, leading))
    if entries.any(rules => (
        rules.target == heading.where(supplement: contents-title)
    )) {
        show outline: set heading(supplement: contents-title)
        apply-rules(entries, body)
    } else { apply-rules(entries, body) }
}

// A style's keys as Typst values (see `design-tokens`): lengths, a colour and
// the family and weight of its face, `none` for an unset key, and under `text`
// the arguments of `text(..)` that set its face. Every key is unset for `none`.
#let typed-style(design, spec) = {
    let points(key) = {
        let number = style-value(spec, key)
        if number == none { none } else { number * 1pt }
    }
    let pitch = style-value(spec, "line_height")
    let hex = style-value(spec, "color")
    (
        ..style-face(design, spec),
        size: points("size"),
        line_height: if pitch != none and "exact" in pitch {
            (exact: pitch.exact * 1pt)
        } else { pitch },
        color: if hex == none { none } else { rgb(hex) },
        italic: style-value(spec, "italic"),
        align: style-value(spec, "align"),
        space_before: points("space_before"),
        space_after: points("space_after"),
        keep_together: style-value(spec, "keep_together") == true,
        keep_with_next: style-value(spec, "keep_with_next") == true,
        text: style-text(design, spec),
    )
}

// The design as Typst values, for a template's own rules: lengths, colours
// and font families instead of the JSON's numbers and strings, under the
// JSON's key names, `none` for an unset key. Takes `writer-doc`'s arguments,
// so a template passes its own on (`design-tokens(..args)`) and gets the
// design `writer-doc` applies: the `design:` argument, else the input.
#let design-tokens(design: auto, ..rest) = {
    let design = design-value(design)
    let value(key) = design-get(design, key, none)
    let length(key) = {
        let number = value(key)
        if number == none { none } else { number * 1pt }
    }
    let colour(key) = {
        let hex = value(key)
        if hex == none { none } else { rgb(hex) }
    }
    let pitch = value("text.line_height")
    let styles = (:)
    for (name, spec) in design-get(design, "styles", (:)) {
        styles.insert(name, typed-style(design, spec))
    }
    (
        declared: design.len() > 0,
        fonts: (
            body: value("fonts.body.family"),
            heading: value("fonts.heading.family"),
            mono: value("fonts.mono.family"),
        ),
        text: (
            size: length("text.size"),
            line_height: if pitch != none and "exact" in pitch {
                (exact: pitch.exact * 1pt)
            } else { pitch },
            paragraph_spacing: length("text.paragraph_spacing"),
            justify: value("text.justify"),
        ),
        colors: (
            text: colour("colors.text"), accent: colour("colors.accent"),
            muted: colour("colors.muted"), link: colour("colors.link"),
            background: colour("colors.background"),
        ),
        page: (
            paper: value("page.paper"),
            margin: (
                top: length("page.margin.top"),
                right: length("page.margin.right"),
                bottom: length("page.margin.bottom"),
                left: length("page.margin.left"),
            ),
            header_offset: length("page.header_offset"),
            footer_offset: length("page.footer_offset"),
        ),
        styles: styles,
    )
}

// The style of this name out of `design-tokens`' result, compared by
// normalised key. A style the design does not declare has every key unset, so
// `text(..style-tokens(theme, "Title").text, ..)` is then plain text.
#let style-tokens(tokens, name) = {
    let wanted = style-key(name)
    for (declared, style) in tokens.styles {
        if style-key(declared) == wanted { return style }
    }
    typed-style((:), none)
}
