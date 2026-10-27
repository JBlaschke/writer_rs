// Keep these writer-only words in sync with assets/l10n/*.toml.
#let writer-terms = (
    en: (
        abstract: "Abstract", note: "Note", tip: "Tip",
        important: "Important", warning: "Warning", caution: "Caution",
    ),
)

#let localized-term(words, key, lang, region: none) = {
    let codes = if region == none { (lang, "en") }
        else { (lang + "-" + upper(region), lang, "en") }
    for code in codes {
        let terms = words.at(code, default: (:))
        if key in terms { return terms.at(key) }
    }
    upper(key.slice(0, count: 1)) + key.slice(1)
}

#let writer-term(key) = context localized-term(
    writer-terms, key, text.lang, region: text.region
)

#let callout(kind: "note", title: auto, body) = context {
    let colors = (
        note: rgb("#2867a2"), tip: rgb("#307445"),
        important: rgb("#7950a0"), warning: rgb("#926500"),
        caution: rgb("#b33131"),
    )
    let accent = colors.at(kind, default: rgb("#62676c"))
    let caption = if title == auto { writer-term(kind) } else { title }
    block(
        breakable: true, width: 100%, inset: 0.8em,
        fill: accent.lighten(94%),
        stroke: (left: 2pt + accent), radius: 2pt,
    )[
        #if caption != none and caption != [] {
            strong(text(fill: accent, caption))
            parbreak()
        }
        #body
    ]
}
