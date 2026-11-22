# R11: design styles for writer's own elements in the PDF (`@writer/doc` 0.4.0)

**Status:** research note with a verified prototype (2026-11-19; §6 corrected
2026-11-20). It backs design.md §8.7 and typst-dialect.md §4.2/§4.6 (the
contract) and tasks T93–T95.
Everything below was run with the Typst CLI 0.15.1 against an ejected copy of the
package (`writer eject typst-package`) patched with §5's code, on Source Sans 3
and on two real themes (a report with a cover page and a letterhead). Nothing
here is a contract: where this note and the spec disagree, the spec wins.

## 1. The question

A theme describes headings, the title block, the contents list, captions and
table text once, as `[design.styles.<Name>]` tables named after writer's Word
style vocabulary (docx-output.md §2.3; T92). Word applies them. The PDF did not:
`@writer/doc` applied named styles to divs only, so a theme repeated every one
of those values in a `[pdf] template`, together with the colours and margins the
template's own rules need, because the design reached Typst as raw JSON only.

The answer is yes, the package can apply them, and a template can then hold
only what the design does not say. Three pieces make that work:

1. `writer-doc` turns each vocabulary style into **show-set rules** on the
   element writer uses the style for (§3).
2. A template written as `show: base.writer-doc.with(..args)` followed by its
   own rules **overrides those rules key by key** (§2).
3. `design-tokens(..args)` gives the template the design as Typst values, so
   the rules it still needs (a table's header fill, a cover's offset) name
   `theme.colors.accent` or `theme.page.margin.top` instead of repeating them.

## 2. Typst semantics the design relies on (each one probed)

- **Inner rules win, property by property.** With
  `show: base.writer-doc.with(..args)` first and the template's rules after it,
  the template's rules are the inner ones. Probe: the design declares
  `Heading 1` (14pt, bold, `#0a2f41`) and the template adds
  `show heading.where(level: 1): set text(fill: red)`: the heading is red,
  14pt and bold. A template rule on `heading.where(level: 1)` also covers the
  unlisted level-1 headings and the contents title, so it overrides
  `Heading 1 Unlisted` and `TOC Heading` for the keys it sets.
- **The old starter's order loses.** With the template's rules first and
  `base.writer-doc(..args, body)` after them (the 0.3.0 starter), the
  package's rules are the inner ones. Probe: the same design, the template sets
  `show heading.where(level: 1): set text(fill: red, size: 30pt, style:
  "oblique")`: the heading is `#0a2f41` and 14pt (the design's keys) and
  oblique (a key the design leaves unset). Such a template already loses for
  everything `writer-doc` sets (page, fonts); it now also loses for the keys a
  vocabulary style sets.
- **Show-set, never a transformational rule.** `show heading: it => { set
  text(..); it }` would put the package's `set` innermost, where no template
  rule could override it. Every design rule is `show <selector>: set
  <element>(..)`.
- **Empty rules are no-ops.** `set text(..(:))`, `set par(..(:))`,
  `set block(..(:))` and `set align(..())` compile and change nothing, so one
  code path serves every style whatever keys it sets.
- **A dictionary spread without a named pair needs a colon**:
  `(: ..a, ..b)`; `(..a, ..b)` is an array and fails with "cannot spread
  dictionary into array".
- **The contents title is a level-1 heading.** `outline` builds its title as a
  heading of depth 1 with `outlined: false` and `numbering: none`, so
  `heading.where(level: 1, outlined: false)` matches it as well as a document's
  unlisted headings. A marker isolates it: `show outline: set heading(supplement:
  [writer-contents-title])` and then `heading.where(supplement:
  [writer-contents-title])` selects the title alone (the supplement is never
  shown: nothing references the contents title).
- **Family names carry no weight.** Typst files a face under its typographic
  family and trims a weight word from the name, so `font: "Source Sans 3
  SemiBold"` is an unknown family. Word reaches that face by exactly this name
  (it has no flag for 600). The package therefore reads a trailing weight word
  as family plus weight (`style-face`, §5).
- **Spacing.** Typst's spacing between blocks is weak: of two adjacent ones the
  stronger holds, and of two equally strong ones the larger. A paragraph's
  spacing and a block's `auto` spacing have weakness 4, an explicit block
  `above`/`below` has 3 and therefore beats a paragraph's spacing whatever its
  size. Weak spacing at the top of a page is dropped, also after a strong `v`.
  A heading's defaults are 1.4em/1.2em/1em sizes, bold, `above` 1.8em (level 1)
  or 1.44em, `below` 0.75em, sticky. An outline entry is a block with `above`
  = the paragraph leading.
- **Em lengths resolve where the element is styled.** `par.leading` and a
  block's `above`/`below` given in ems take the text size of the element they
  are set on. A paragraph that consists of one `text(size: 17pt, ..)[..]` takes
  17pt as its own size (Typst uses the styles its children share), so the old
  title block's gap under the title is the paragraph spacing at 17pt.

## 3. Mapping (what was built and measured)

| style name | PDF element | selector or place |
|---|---|---|
| `Heading N` (1–6) | headings of level N | `heading.where(level: N)` |
| `Heading N Unlisted` | unlisted headings of level N | `heading.where(level: N, outlined: false)`, after `Heading N` |
| `TOC Heading` | the contents title | `heading.where(supplement: [writer-contents-title])`, `Heading 1` laid over again first |
| `toc N` | contents entries of level N | `outline.entry.where(level: N)` |
| `Caption` | figure and table captions | `figure.caption` (no spacing keys) |
| `Table Caption`, `Image Caption` | captions of tables, of images | `figure.caption.where(kind: table)`, `.where(kind: image)`, after `Caption` |
| `Compact` | table cells | `table.cell` (no block keys) and `set table(inset:)` |
| `Title`, `Subtitle`, `Author`, `Date` | the parts of the built-in title block | `styled-title-block` |
| `Header`, `footer` | the furniture lines | font and `line_height` in `writer-furniture` |

Keys on a block target: `font` (a role or a family, a trailing weight word
split off), `size`, `color`, `weight`, `italic` → `set text`; `align` → `set
align` for left, center and right and `set par(justify:)`; `line_height` →
`set par(leading:)` by design.md §6.2's rule on the style's own face;
`space_before`/`space_after` → `set block(above:, below:)` as `L + space`,
where `L` is the style's own leading (the document's em-based leading when the
style sets no `line_height`); `keep_with_next` → `sticky`; `keep_together` →
`breakable: false`. An unset key and a `false` keep flag add nothing.

Why `L + space`: design.md §6.2 already sets the paragraph spacing to `leading
+ paragraph_spacing`, so that a break adds the space to a full line pitch as
HTML and Word do. The same rule on blocks reproduces Word's geometry. Measured
on the report theme: `toc 1` is 10pt with `line_height = 1.424`, `space_before
= "6pt"`; Word sets its entries 14.24 + 6 = 20.24pt (7.1 mm) apart; the PDF
gives `above` = (1.424 − 0.66) × 10 + 6 = 13.64pt plus the 6.6pt line box =
20.24pt. Heading-to-body distances came out within 0.7pt of Word's.

Measurements of Source Sans 3 (at 30pt: cap height 19.8pt, ascender 30.72pt,
descender 12pt): cap height 0.66em, ascender 1.024em, one Word single line
1.424em. Word starts a paragraph's first line at the font's ascender, Typst at
its cap height (`top-edge: "cap-height"`), which is why the first title part's
space is `space_before + (ascender − cap height)`, written as a strong `v`
because Typst would drop a block's own space above at the top of a page while
Word keeps it.

Results on the two themes (60 and 80 ppi page images compared with ImageMagick
`compare -metric AE`):

| case | result |
|---|---|
| document without a design, old and new package | byte-identical PDF (`--creation-timestamp 0`, same sha256) |
| design with only a div style, with title metadata | pixel-identical: the plain title block still draws it |
| design with a `Date` style that sets no key | the styled title block draws the same pixels as the plain one (1 antialiased pixel in 827 × 1170) |
| report, its 142-line template unchanged | cover identical; later pages differ only where the theme now supplies a value the template never set (table text leading from `Compact`, header and footer pitch) |
| report, no template | the theme alone sets the cover lines (title baseline at the Word-tuned 160 mm), headings, unlisted headings, contents title and entries, table text |
| report, 84-line overrides-only template (§7) | cover pixel-identical to the 142-line template's; the other pages have the same look with the theme's Word-tuned spacing |
| letter, its 17-line template unchanged | body identical; footer lines follow the `footer` style's 1.424 pitch (3.7 mm, as in Word) |
| letter, no template | the subject line (`Heading 1`) is styled by the theme alone; the template is no longer needed |

What changes for an existing theme, visibly: the PDF's heading gaps and
contents-entry spacing follow the theme's numbers (tuned in Word) instead of a
template's or Typst's own. On the report that moved the first section onto the
contents page, as in the Word file.

## 4. Known differences, left open on purpose

- **Italic leak into the contents title.** `Heading 1 Unlisted` also matches
  the contents title. `TOC Heading` is laid over `Heading 1` again, which
  restores every key either of them sets; a key that only `Heading 1 Unlisted`
  sets (the probe: `italic = true`) still reaches the title. Word shows no
  such leak. Workaround: set that key on `TOC Heading`.
- **Title parts and Word's `basedOn`.** In Word, `Subtitle`, `Author` and
  `Date` are based on `Title` and inherit what neither they nor their built-in
  definitions set (line height, weight, italic, keep flags; `Subtitle` the
  font, `Author` the colour). The PDF takes only `Title`'s `align` for the
  whole block. A theme that wants identical output sets the keys on each
  style.
- **Sum or larger.** Between two styled blocks Typst and CSS use the larger of
  the first's space after and the second's space before; Word adds them.
  *Correction (2026-11-21, R12 §2):* this was assumed, not measured. Word for
  Mac 16 also takes the larger of the two, so the three outputs follow one
  rule.
- **Divs.** `writer-div` still passes `space_before`/`space_after` to its
  block as they are, one leading less than HTML and Word show (design.md §8.4).
  Not changed here; see design.md's open points. *Resolved by package `0.5.0`
  (R12).*
- **Not applied in the PDF**: `Body Text`, `First Paragraph`, `Block Text`,
  `Footnote Text`, `Abstract Title`, `Abstract`, `Bibliography`, `Definition
  Term`, `Definition`, `Source Code`, `Figure`, `Captioned Figure`; `Compact`
  on tight lists; a caption's `space_before`/`space_after`.

## 5. Reference code: `assets/typst/writer-doc/design.typ` (new file)

The first 100 lines are `lib.typ`'s design helpers moved verbatim, with one
change: `design-leading` gains `box: auto`. Extract the file with

```sh
awk '/^```typst design\.typ$/ {on = 1; next} /^```$/ {on = 0} on' \
    docs/research/R11-typst-vocabulary-styles.md \
    > assets/typst/writer-doc/design.typ
```

```typst design.typ
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
```

## 6. Reference code: the changes to `assets/typst/writer-doc/lib.typ`

A unified diff against `lib.typ` at commit `1729e4db` (crate 0.3.0). Extract
and apply it from the repository root with

```sh
awk '/^```diff lib\.typ$/ {on = 1; next} /^```$/ {on = 0} on' \
    docs/research/R11-typst-vocabulary-styles.md > lib-typ.diff
git apply --check lib-typ.diff && git apply lib-typ.diff && rm lib-typ.diff
```

If `lib.typ` has changed since and the diff no longer applies, make the same
seven edits by hand; they are: (1) the design helpers leave for `design.typ`
and `#import "design.typ": *` joins the imports; (2) `styled-title-block` and
`title-styles` are added after `title-block`; (3) `numbered-layout` chooses
the title block and builds the tokens for a `title-block` function; (4) `writer-furniture` gains `font:`; (5) `furniture-background`
passes the `Header` and `footer` styles' font and pitch; (6) `paragraph-layout`
applies `vocabulary-layout` and hands the resolved leading down; (7)
`writer-doc` gains `title-block: auto`, its assertion and one `settings`
entry, and `writer-styled` uses `style-text`.

Corrected on 2026-11-20, when the tasks ran. The first version of this diff
kept `tokens: design-tokens(design: design)` in `settings`, for every
document. `design-tokens` converts every scalar key of the design, so a
native document whose `design:` dictionary held a value the package never
read stopped compiling: `text.size` as the string `"99pt"` beside an explicit
`size:`, a fixture of `tests/typst_design.rs`, failed with "cannot multiply
string with length". The tokens are now built in `numbered-layout`, for a
`title-block` function alone, and a document without one reads only the keys
`writer-doc` applies, as with `0.3.0`. No measurement of §3 depends on this;
T94's tests and both real themes were run again on the corrected code.

```diff lib.typ
diff --git a/assets/typst/writer-doc/lib.typ b/assets/typst/writer-doc/lib.typ
--- a/assets/typst/writer-doc/lib.typ
+++ b/assets/typst/writer-doc/lib.typ
@@ -1,5 +1,6 @@
 #import "callout.typ": callout, writer-term
 #import "cells.typ": code-cell, include-notebook
+#import "design.typ": *
 #import "numbering.typ": writer-numbering, bibliography-term
 
 #let language(tag) = {
@@ -27,78 +28,6 @@
     if value == auto { config.at(key, default: fallback) } else { value }
 }
 
-// The resolved design is shared with HTML and DOCX. Missing inputs leave every
-// built-in default intact, including standalone exported Typst documents.
-#let writer-design = if "writer-design" in sys.inputs {
-    json(bytes(sys.inputs.at("writer-design")))
-} else { (:) }
-
-// A dotted lookup in one design dictionary; `default` stands in for a missing
-// or `none` value.
-#let design-get(design, key, default) = {
-    let value = design
-    for part in key.split(".") {
-        if type(value) != dictionary { return default }
-        value = value.at(part, default: none)
-    }
-    if value == none { default } else { value }
-}
-
-#let from-design(key, default) = design-get(writer-design, key, default)
-
-// The design `writer-doc` applies: its explicit `design:` argument wins over
-// the `writer-design` input, which wins over the package defaults. An exported
-// source passes the JSON copied beside its main file here, so a plain compile
-// needs no input; `none` ignores the input and keeps the defaults.
-#let design-value(design) = {
-    if design == auto { writer-design }
-    else if design == none { (:) }
-    else {
-        assert(
-            type(design) == dictionary,
-            message: "writer-doc: design must be auto, none or a dictionary"
-        )
-        design
-    }
-}
-
-// Shares the applied design with `writer-div`, which the body calls outside
-// `writer-doc`'s arguments; `none` until `writer-doc` records it.
-#let design-state = state("writer-design", none)
-
-// The line box Typst lays text of `size` out with: with the default
-// `top-edge: "cap-height"` and `bottom-edge: "baseline"`, the cap height of
-// the font in effect (its OS/2 `sCapHeight`, else its ascender; a fallback
-// font's when the family is missing). Measured rather than assumed, so the
-// fonts and edges actually in use decide. Needs context.
-#let line-box(size) = measure(text(size: size)[H]).height
-
-// `par.leading` for a design pitch (design.md §6.2). Typst's leading is the
-// gap between one line's bottom edge and the next line's top edge, so a
-// baseline pitch needs `pitch − line box`; HTML's `line-height` and Word's
-// lines then agree with the PDF. The result is in ems of `size`, so that a
-// multiple follows a paragraph's own size as HTML's unitless `line-height`
-// and Word's `auto` lines do, and an exact pitch holds at any size. Needs
-// context, except for the `0.65em` default of an unset pitch.
-#let design-leading(pitch, size) = {
-    if pitch == none { return 0.65em }
-    let box = line-box(size)
-    let target = if "multiple" in pitch { pitch.multiple * size }
-        else { pitch.exact * 1pt }
-    if target <= box { return 0pt }
-    if "multiple" in pitch { (pitch.multiple - box / size) * 1em }
-    else { target - box / size * 1em }
-}
-
-// `par.spacing` for a design paragraph spacing: Typst measures it like the
-// leading, from the last baseline to the next line's cap height, so a
-// paragraph break adds `spacing` to a full line pitch, as HTML's
-// `margin-block` and Word's `w:after` do, only as `leading + spacing`.
-// `none` keeps Typst's `1.2em`.
-#let design-spacing(leading, spacing) = {
-    if spacing == none { 1.2em } else { leading + spacing * 1pt }
-}
-
 #let document-date(date) = {
     if "writer-version" in sys.inputs { return auto }
     if type(date) == datetime { return date }
@@ -155,6 +84,73 @@
     }
 }
 
+// The title block when the design styles one of its parts (design.md §8.7):
+// `Title`, `Subtitle`, `Author` and `Date` over each part's built-in look.
+// Word bases the last three on `Title`, so they follow its alignment. Needs
+// context.
+#let styled-title-block(meta, design, leading) = {
+    let parts = ()
+    if meta.title != none {
+        parts.push(("Title", (size: 17pt, weight: "bold"), meta.title))
+    }
+    if meta.subtitle != none { parts.push(("Subtitle", (:), meta.subtitle)) }
+    if meta.authors.len() > 0 {
+        parts.push(("Author", (:), {
+            meta.authors.map(
+                author => author.at("name", default: "")
+            ).join(", ")
+            for author in meta.authors {
+                let affiliation = author.at("affiliation", default: none)
+                if affiliation != none {
+                    parbreak()
+                    text(size: 9pt, affiliation)
+                }
+            }
+        }))
+    }
+    if meta.date != none {
+        parts.push(("Date", (:), if type(meta.date) == datetime {
+            meta.date.display("[year]-[month]-[day]")
+        } else { meta.date }))
+    }
+    if parts.len() == 0 { return }
+    let rules(name) = style-rules(design, design-style(design, name), leading)
+    let base = rules("Title").align.at(0, default: center)
+    for (index, (name, defaults, body)) in parts.enumerate() {
+        let part = rules(name)
+        let face = (: ..defaults, ..part.text)
+        let spacing = part.block
+        if index == 0 and "above" in spacing {
+            // Typst drops a block's space above at the top of a page, where
+            // Word keeps the first paragraph's. Word's line starts at the
+            // font's ascender, Typst's at its cap height.
+            let before = style-value(design-style(design, name), "space_before")
+            let rise = (
+                measure(text(..face, top-edge: "ascender")[H]).height
+                    - measure(text(..face)[H]).height
+            )
+            v(before * 1pt + rise, weak: false)
+            let _ = spacing.remove("above")
+        }
+        // The face is set around the block, so that the block's spacing is in
+        // ems of the part's own size: an unset space is then the paragraph
+        // spacing at that size, as it is for the plain title block's lines.
+        {
+            set text(..face)
+            block(width: 100%, ..spacing, {
+                set par(..part.par)
+                set align(part.align.at(0, default: base))
+                body
+            })
+        }
+    }
+    // A set `space_after` on the last part replaces the closing gap.
+    let last = design-style(design, parts.last().first())
+    if style-value(last, "space_after") == none { v(1.5em) }
+}
+
+#let title-styles = ("Title", "Subtitle", "Author", "Date")
+
 #let abstract-block(body, title: auto) = {
     if body != none {
         set heading(numbering: none, outlined: false)
@@ -169,7 +165,17 @@
 
 #let numbered-layout(meta, settings, body) = {
     show: writer-numbering.with(..settings.numbering)
-    title-block(meta)
+    if type(settings.title-block) == function {
+        // The typed tokens are built here, for the function that asked for
+        // them: a document without one never converts its design.
+        (settings.title-block)(meta, design-tokens(design: settings.design))
+    } else if settings.title-block == auto {
+        if title-styles.any(name => (
+            design-style(settings.design, name) != none
+        )) {
+            styled-title-block(meta, settings.design, settings.leading)
+        } else { title-block(meta) }
+    }
     abstract-block(meta.abstract, title: settings.abstract-title)
     if settings.toc != false and settings.toc != 0 {
         outline(depth: if settings.toc == true { 3 } else { settings.toc })
@@ -247,11 +253,12 @@
 // image and the lines below it (design.md §9.1); `line-height` is the
 // design's `text.line_height`, which sets the lines' leading at their size.
 #let writer-furniture(
-            spec, text-width, spacing: 1.2em, line-height: none
+            spec, text-width, spacing: 1.2em, line-height: none, font: none
         ) = context {
     if spec == none { return none }
     let size = if spec.size == none { text.size } else { spec.size * 1pt }
     let fill = if spec.color == none { text.fill } else { rgb(spec.color) }
+    set text(..if font == none { (:) } else { (font: font) })
     set text(size: size, fill: fill, weight: spec.weight,
         style: if spec.italic { "italic" } else { "normal" })
     set par(
@@ -349,10 +356,18 @@
     let margin = (top: side("top", "y"), right: side("right", "x"),
         bottom: side("bottom", "y"), left: side("left", "x"))
     let text-width = page.width - margin.left - margin.right
-    let furniture(chosen) = writer-furniture(
-        chosen, text-width,
-        spacing: furniture-gap(settings), line-height: settings.line-height,
-    )
+    // The lines take the font and pitch of the `Header` and `footer` styles
+    // (design.md §8.7), as Word's do; else the document's.
+    let furniture(chosen, name) = {
+        let style = design-style(design, name)
+        let pitch = style-value(style, "line_height")
+        writer-furniture(
+            chosen, text-width, spacing: furniture-gap(settings),
+            line-height: if pitch == none { settings.line-height }
+                else { pitch },
+            font: style-face(design, style).font,
+        )
+    }
     let header = design-get(design, "page.header", none)
     if header != none {
         let chosen = if first { header.first } else { header.rest }
@@ -360,7 +375,8 @@
             let offset = design-get(design, "page.header_offset", none)
             let offset = if offset == none { margin.top * 0.3 }
                 else { offset * 1pt }
-            place(top + left, dx: margin.left, dy: offset, furniture(chosen))
+            place(top + left, dx: margin.left, dy: offset,
+                furniture(chosen, "Header"))
         }
     }
     let footer = design-get(design, "page.footer", none)
@@ -371,8 +387,8 @@
             let offset = if offset == none { margin.bottom * 0.3 }
                 else { offset * 1pt }
             let content = if chosen.size == none {
-                text(size: 0.75em, furniture(chosen))
-            } else { furniture(chosen) }
+                text(size: 0.75em, furniture(chosen, "footer"))
+            } else { furniture(chosen, "footer") }
             place(bottom + left, dx: margin.left, dy: -offset, content)
         }
     }
@@ -392,7 +408,8 @@
             design-spacing(leading, settings.paragraph-spacing)
         } else { settings.spacing },
     )
-    reference-layout(meta, settings, body)
+    show: vocabulary-layout.with(settings.design, leading)
+    reference-layout(meta, (..settings, leading: leading), body)
 }
 
 #let furniture-layout(meta, settings, body) = {
@@ -454,6 +471,7 @@
             font: auto, heading-font: auto, mono-font: auto, size: auto,
             leading: auto, spacing: auto, justify: auto, margin: auto,
             text-fill: auto, link-fill: auto, background: auto, design: auto,
+            title-block: auto,
             markdown: (:), math: (:), references: (:), html: (:), pdf: (:),
             docx: (:), latex: (:), typst: (:), notebook: (:), ..extra, body
         ) = {
@@ -479,6 +497,10 @@
         )
     }
     let locale = from-config(cite-locale, config, "cite_locale", "")
+    assert(
+        title-block in (auto, none) or type(title-block) == function,
+        message: "writer-doc: title-block must be auto, none or a function"
+    )
     let design = design-value(design)
     let from-design(key, default) = design-get(design, key, default)
     let meta = (
@@ -531,7 +553,7 @@
         background: if background == auto {
             colour("colors.background", auto)
         } else { background },
-        design: design,
+        design: design, title-block: title-block,
         lang: language(from-config(lang, config, "lang", "en")),
         toc: from-config(toc, config, "toc", false), toc-title: toc-title,
         numbering: numbers, bibliography: bibliography,
@@ -637,20 +659,8 @@
 #let writer-styled(design, style, body) = {
     let spec = design-get(design, "styles", (:)).at(style, default: none)
     if spec == none { return body }
-    let text-opts = (:)
+    let text-opts = style-text(design, spec)
     let par-opts = (:)
-    if spec.font != none {
-        let family = if spec.font in ("body", "heading", "mono") {
-            design-get(design, "fonts." + spec.font + ".family", none)
-        } else { spec.font }
-        if family != none { text-opts.insert("font", family) }
-    }
-    if spec.size != none { text-opts.insert("size", spec.size * 1pt) }
-    if spec.color != none { text-opts.insert("fill", rgb(spec.color)) }
-    if spec.weight != none { text-opts.insert("weight", spec.weight) }
-    if spec.italic != none {
-        text-opts.insert("style", if spec.italic { "italic" } else { "normal" })
-    }
     if spec.align != none {
         par-opts.insert("justify", spec.align == "justify")
     }
```

## 7. Worked example: an overrides-only template

The report theme's template after the change (84 lines, was 142). It repeats no
colour, size or margin: the cover reads the `Title`, `Subtitle`, `Author` and
`Date` styles and the page margins from `theme`, and the rules after `show:
base.writer-doc.with(..)` are the parts the design has no key for (leaders,
capitals, the table grid, list indents).

```typst
// writer PDF template for a design-only report theme with a cover picture.
//
// The look comes from theme.toml: writer-doc applies the [design] tokens and
// the named styles (headings, title, contents, table cells, captions). This
// file adds only what the theme has no keys for: where the title lines sit
// on the cover picture, the table grid and header row, the case of the
// contents entries and the list indents. A rule placed after
// `show: base.writer-doc.with(..)` overrides the theme for the PDF alone.
#import "@writer/doc:0.4.0" as base

// Cover: the title lines sit on a panel of the cover picture, their
// capitals 153 mm from the top and 21.6 mm from the left edge of the page,
// 124 mm wide. Sizes and colours are the theme's Title, Subtitle, Author and
// Date styles.
#let cover(meta, theme) = {
    let face(name) = base.style-tokens(theme, name).text
    let names = meta.authors.map(
        author => author.at("name", default: "")
    ).filter(name => name != "")
    v(153mm - theme.page.margin.top, weak: false)
    move(dx: 21.6mm - theme.page.margin.left, block(above: 0pt, width: 124mm)[
        #set par(leading: 0.3em, spacing: 0pt, justify: false)
        #if meta.title != none { text(..face("Title"), meta.title) }
        #if meta.subtitle != none {
            v(7mm)
            text(..face("Subtitle"), meta.subtitle)
        }
        #if names.len() > 0 {
            v(6mm)
            text(..face("Author"), names.join(", "))
        }
        #if meta.date != none {
            v(if names.len() > 0 { 1.5mm } else { 6mm })
            text(..face("Date"), if type(meta.date) == datetime {
                meta.date.display("[day]/[month]/[year]")
            } else { meta.date })
        }
    ])
    pagebreak(weak: true)
}

#let writer-doc(..args, body) = {
    let theme = base.design-tokens(..args)
    show: base.writer-doc.with(..args, title-block: cover)

    set heading(hanging-indent: 0pt)

    // Contents list: dotted leaders, capitals and small capitals.
    set outline(indent: 1.2em)
    set outline.entry(fill: repeat([.], gap: 0.3em))
    show outline.entry.where(level: 1): it => upper(it)
    show outline.entry.where(level: 2): it => smallcaps(it)

    // Tables: a ruled grid, the first row an accent-coloured header row.
    set table(
        stroke: 0.5pt + black, inset: (x: 5pt), align: left + top,
        fill: (x, y) => if y == 0 { theme.colors.accent } else { none }
    )
    show table.cell.where(y: 0): set text(weight: "bold", fill: white)

    // writer sizes Markdown table columns automatically; stretch them to the
    // text width: a two-column table keeps a narrow label column.
    show table: it => {
        let columns = it.columns
        if (
            type(columns) == array and columns.len() >= 2
            and columns.all(column => column == auto)
        ) {
            let fields = it.fields()
            let children = fields.remove("children")
            let _ = fields.remove("columns")
            table(
                columns: if columns.len() == 2 { (auto, 1fr) } else {
                    (1fr,) * columns.len()
                },
                ..fields, ..children
            )
        } else { it }
    }

    set list(indent: 1.2em, body-indent: 0.6em)
    set enum(indent: 1.2em, body-indent: 0.6em)
    body
}
```

## 8. Commands used (for anyone repeating the checks)

```sh
writer eject typst-package P                      # then patch P/writer/doc/<v>/
writer --config writer.toml build doc.md --to typst
typst compile --ignore-system-fonts --font-path dist/doc_assets/fonts \
    --package-path P --ppi 60 dist/doc.typ 'out/p{p}.png'
compare -metric AE base/p1.png out/p1.png null:   # ImageMagick; 0 = identical
typst eval 'query(<probe>).map(it => it.value)' --in probe.typ \
    --package-path P                              # read styles back
```

A probe is a show rule placed before `#show: writer-doc.with(..)`; the element's
show-set styles reach what the rule returns:

```typst
#show heading: it => {
    it
    context [#metadata((
        size: text.size, weight: text.weight, fill: text.fill.to-hex(),
    )) <probe>]
}
```
