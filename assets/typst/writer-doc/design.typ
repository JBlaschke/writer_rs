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

// A face's cap height: the line box Typst gives its lines by default
// (`top-edge: "cap-height"`, `bottom-edge: "baseline"`), which the design's
// line box replaces. `face` holds the `text` arguments of another face than
// the one in effect. Measured, so the font actually in use decides, a
// fallback font's included. Needs context.
#let cap-height(face) = measure(
    text(..face, top-edge: "cap-height", bottom-edge: "baseline")[H]
).height

// A face's line metrics in ems as Word reads them (design.md §7.4): `single`
// is the height of a single line, `ascent` the part of it above the
// baseline. Writer measures both and sends them in the design's `lines`
// table under the family in lower case; a family it did not send falls back
// to Typst's own ascender and descender. Needs context.
#let font-lines(design, face) = {
    let family = face.at("font", default: text.font)
    if type(family) == array { family = family.first() }
    if type(family) == dictionary { family = family.name }
    let sent = design-get(design, "lines", (:)).at(lower(family), default: none)
    if sent != none { return sent }
    let size = face.at("size", default: text.size)
    let reach(top, bottom) = measure(
        text(..face, top-edge: top, bottom-edge: bottom)[H]
    ).height / size
    let ascent = reach("ascender", "baseline")
    (single: ascent + reach("baseline", "descender"), ascent: ascent)
}

// The line box of a face's lines as `text`'s edges (design.md §6.2): Word's.
// An exact pitch `h` puts `0.8 h` above the baseline and `0.2 h` below it,
// whatever the font. A multiple `m` of the size keeps the font's ascent
// while the line is at least a single line and puts the rest below the
// baseline; a tighter line shrinks ascent and descent alike. The edges are
// in ems for a multiple, so the box follows the size of the text, as HTML's
// unitless `line-height` and Word's `auto` lines do. `pitch` is `none` for a
// design that sets no line height: the lines then keep Typst's own pitch,
// the cap height plus `0.65em`. Needs context.
#let line-edges(design, pitch, face) = {
    if pitch != none and "exact" in pitch {
        return (
            top-edge: pitch.exact * 0.8pt, bottom-edge: pitch.exact * -0.2pt
        )
    }
    let multiple = if pitch == none {
        cap-height(face) / face.at("size", default: text.size) + 0.65
    } else { pitch.multiple }
    let lines = font-lines(design, face)
    let ascent = calc.min(1, multiple / lines.single) * lines.ascent
    (top-edge: ascent * 1em, bottom-edge: (ascent - multiple) * 1em)
}

// The pitch of the line box in effect, as a design writes one: what a style
// that sets no `line_height` inherits. `none` outside the line-box model
// (Typst's named edges). Needs context.
#let inherited-pitch() = {
    let top = text.top-edge
    let bottom = text.bottom-edge
    if type(top) != length or type(bottom) != length { return none }
    let pitch = top - bottom
    if pitch.abs == 0pt { (multiple: pitch.em) }
    else { (exact: pitch.abs.pt()) }
}

// How far a face's line box reaches beyond its capitals: `top` above the cap
// height and `bottom` below the baseline. An inset that should look as it
// did around Typst's own line box gives these up. Needs context.
#let line-trim(design, pitch, face) = {
    let edges = line-edges(design, pitch, face)
    let size = face.at("size", default: text.size)
    let resolve(edge) = edge.abs + edge.em * size
    (
        top: resolve(edges.top-edge) - cap-height(face),
        bottom: -resolve(edges.bottom-edge),
    )
}

// The document's line and paragraph model (design.md §6.2) from
// `writer-doc`'s settings: the pitch of its lines, the body's line box and
// the gap between two paragraphs' line boxes. An explicit `leading` or
// `spacing` argument is Typst's own measure, from one line's baseline to the
// next line's cap height, and is turned into the model's. An unset paragraph
// spacing keeps Typst's `1.2em`, measured the same way. Two values stay in
// Typst's measure for what `paragraph-layout` leaves outside the line boxes:
// `leading`, Typst's leading for this pitch (in ems unless the pitch is
// exact), which a display equation puts between its rows, and `note-gap`,
// what is left between two footnotes of the `0.5em` that Typst keeps from
// one note's last baseline to the next note's cap height once the note's
// line box, at `0.85em`, has taken its part. Needs context.
#let spacing-model(settings) = {
    let cap = cap-height((:))
    let pitch = if settings.leading == auto { settings.line-height } else {
        (multiple: (cap + settings.leading.to-absolute()) / text.size)
    }
    let exact = pitch != none and "exact" in pitch
    let height = if pitch == none { cap + 0.65em.to-absolute() }
        else if exact { pitch.exact * 1pt }
        else { pitch.multiple * text.size }
    let gap = if settings.spacing != auto {
        calc.max(0pt, settings.spacing.to-absolute() - (height - cap))
    } else if settings.paragraph-spacing != none {
        settings.paragraph-spacing * 1pt
    } else { calc.max(0pt, 1.2em.to-absolute() - (height - cap)) }
    let leading = if height <= cap { 0pt }
        else if exact { height - cap / text.size * 1em }
        else { (height - cap) / text.size * 1em }
    let note = if exact { height - 0.85 * cap } else { 0.85 * (height - cap) }
    (
        pitch: pitch, gap: gap, leading: leading,
        note-gap: calc.max(0pt, 0.5em.to-absolute() - note),
        edges: line-edges(settings.design, pitch, (:)),
    )
}

// A block without height that carries `below` as its space below. Typst
// keeps a space above a block only when a frame precedes it on the page, and
// lets two blocks' spaces meet on equal terms; this block is that frame and
// that space.
#let flow-mark(below) = block(
    above: 0pt, below: below, height: 0pt, inset: 0pt, outset: 0pt,
    stroke: none, fill: none, sticky: false,
)

// Ends a paragraph in the line-box model: the paragraph, then its spacing as
// a block's space below. Typst gives a paragraph the same weak spacing above
// and below, and any block's own spacing replaces it outright; as a block's
// space the paragraph spacing meets the next block's space above on equal
// terms, and the larger of the two holds, as in Word and HTML. A paragraph
// without spacing needs no mark: every space next to it already holds.
#let par-end(it) = {
    if it.spacing == 0pt { it } else { it + flow-mark(it.spacing) }
}

// The elements content is made of where it holds set or show rules, several
// children or only white space. Typst has no names for them.
#let styled-content = {
    set par(justify: false)
    [x]
}.func()
#let content-sequence = [].func()
#let blank-content = ([ ].func(), parbreak)

// `body` with `mark` in front of its first element. Typst drops the space
// above the first block of a page; Word and HTML keep it on the first page,
// and a block without height in front of the document keeps it there
// (design.md §6.2). The mark goes inside the set and show rules `body`
// starts with: a page rule among them would otherwise find the mark before
// it and start a new page after it. A body that opens with a page or a page
// break of its own gets no mark.
#let open-flow(mark, body) = {
    if type(body) != content { return mark + body }
    let kind = body.func()
    if kind == styled-content {
        styled-content(open-flow(mark, body.child), body.styles)
    } else if kind == content-sequence {
        let children = body.children
        let at = children.position(child => child.func() not in blank-content)
        if at == none { return mark + body }
        (
            children.slice(0, at).join()
                + open-flow(mark, children.at(at))
                + children.slice(at + 1).join()
        )
    } else if kind in (page, pagebreak) { body } else { mark + body }
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

// What a style sets on the element it styles, as the arguments of
// `apply-rules`' show-set rules (design.md §8.7). A style that sets its own
// `line_height` or font gets its own line box; `pitch` is the pitch it
// inherits otherwise. The spaces are plain block spacing between line boxes,
// so the larger of two adjacent spaces holds. Needs context.
#let style-rules(design, spec, pitch) = {
    let face = style-text(design, spec)
    let own = style-value(spec, "line_height")
    let edges = if own != none { line-edges(design, own, face) }
        else if "font" in face { line-edges(design, pitch, face) }
        else { (:) }
    let align = style-value(spec, "align")
    let par = (:)
    if align != none { par.insert("justify", align == "justify") }
    let block = (:)
    let before = style-value(spec, "space_before")
    let after = style-value(spec, "space_after")
    if before != none { block.insert("above", before * 1pt) }
    if after != none { block.insert("below", after * 1pt) }
    if style-value(spec, "keep_with_next") == true {
        block.insert("sticky", true)
    }
    if style-value(spec, "keep_together") == true {
        block.insert("breakable", false)
    }
    (
        text: (: ..face, ..edges), par: par, block: block,
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
// `Heading 1 Unlisted` is based on `heading 1`. `pitch` is the
// document's. Needs context.
#let vocabulary(design, pitch) = {
    let style(name) = design-style(design, name)
    let entry(target, spec) = (
        target: target, ..style-rules(design, spec, pitch)
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

// Table cells take `Compact`'s spaces as their inset above and below the
// cell's line box, as Word's rows do. A space the style leaves unset keeps
// the look of Typst's `5pt` around the capitals. Needs context.
#let cell-inset(design, pitch) = {
    let spec = design-style(design, "Compact")
    let own = style-value(spec, "line_height")
    let trim = line-trim(
        design, if own == none { pitch } else { own }, style-text(design, spec)
    )
    let before = style-value(spec, "space_before")
    let after = style-value(spec, "space_after")
    (inset: (
        top: if before != none { before * 1pt }
            else { calc.max(0pt, 5pt - trim.top) },
        bottom: if after != none { after * 1pt }
            else { calc.max(0pt, 5pt - trim.bottom) },
    ))
}

// An empty table cell holds one line box inside its inset, as in Word, where
// every cell holds a paragraph: a row of empty cells is as tall as a row of
// text. `it` is the cell, whose inset is one length or one per side.
#let empty-cell(it) = {
    let inset = it.inset
    if type(inset) == dictionary { pad(..inset, sym.zws) }
    else if inset == auto { sym.zws }
    else { pad(rest: inset, sym.zws) }
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
// by key. `pitch` is the document's. Needs context.
#let vocabulary-layout(design, pitch, body) = {
    let entries = vocabulary(design, pitch)
    set table(..cell-inset(design, pitch))
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
