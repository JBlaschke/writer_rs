#import "oracle.typ": oracle, oracle-on, figure-kind
#import "callout.typ": localized-term

// bibliography-terms below mirrors typst-library 0.15.1 translations/*.txt.
// Writer-only words live separately in callout.typ, matching assets/l10n/.
#let bibliography-terms = (
    "af": (bibliography: "Bibliografie",),
    "alt": (bibliography: "Bibliography",),
    "am": (bibliography: "ቢዋ መጽሃፍት",),
    "ar": (bibliography: "المراجع",),
    "as": (bibliography: "তথ্যসূত্ৰ",),
    "ast": (bibliography: "Bibliografía",),
    "az": (bibliography: "Ədəbiyyat",),
    "be": (bibliography: "Літаратура",),
    "bg": (bibliography: "Библиография",),
    "bn": (bibliography: "তথ্যবিবরণ",),
    "bo": (bibliography: "དཔེ་ཆའི་ཐོ་གཞུང་།",),
    "br": (bibliography: "Lennadurezh",),
    "bs": (bibliography: "Bibliografija",),
    "bua": (bibliography: "Ном зүй",),
    "ca": (bibliography: "Bibliografia",),
    "ckb": (bibliography: "کتێبنامە",),
    "cs": (bibliography: "Bibliografie",),
    "cu": (bibliography: "Литература",),
    "cy": (bibliography: "Llyfryddiaeth",),
    "da": (bibliography: "Bibliografi",),
    "de": (bibliography: "Bibliografie",),
    "dsb": (bibliography: "Literatura",),
    "el": (bibliography: "Βιβλιογραφία",),
    "en": (bibliography: "Bibliography",),
    "eo": (bibliography: "Bibliografio",),
    "es": (bibliography: "Bibliografía",),
    "et": (bibliography: "Viited",),
    "eu": (bibliography: "Bibliografia",),
    "fa": (bibliography: "کتاب‌نامه",),
    "fi": (bibliography: "Viitteet",),
    "fil": (bibliography: "Talaaklatan",),
    "fr-CA": (bibliography: "Bibliographie",),
    "fr": (bibliography: "Bibliographie",),
    "fur": (bibliography: "Bibliografie",),
    "ga": (bibliography: "Leabharliosta",),
    "gd": (bibliography: "Leabhraichean",),
    "gl": (bibliography: "Bibliografía",),
    "grc": (bibliography: "Βιβλιογραφία",),
    "gu": (bibliography: "Bibliography",),
    "ha": (bibliography: "Fahirinsa",),
    "he": (bibliography: "רשימת מקורות",),
    "hi": (bibliography: "संदर्भ-ग्रन्थ",),
    "hr": (bibliography: "Literatura",),
    "hsb": (bibliography: "Literatura",),
    "hu": (bibliography: "Irodalomjegyzék",),
    "hy": (bibliography: "Գրականություն",),
    "ia": (bibliography: "Bibliographia",),
    "id": (bibliography: "Daftar Pustaka",),
    "is": (bibliography: "Heimildaskrá",),
    "isv": (bibliography: "Bibliografija",),
    "it": (bibliography: "Bibliografia",),
    "ja": (bibliography: "参考文献",),
    "ka": (bibliography: "ლიტერატურა",),
    "km": (bibliography: "គន្ថនិទ្ទេស",),
    "kmr": (bibliography: "Çavkanîya Pirtukan",),
    "kn": (bibliography: "ಗ್ರಂಥಸೂಚಿ",),
    "ko": (bibliography: "참고 문헌",),
    "ku": (bibliography: "Çavkanîya Pirtukan",),
    "la": (bibliography: "Conspectus librorum",),
    "lb": (bibliography: "Bibliographie",),
    "lo": (bibliography: "ເອກະສານອ້າງອີງ",),
    "lt": (bibliography: "Literatūra",),
    "lv": (bibliography: "Literatūra",),
    "mk": (bibliography: "Библиографиjа",),
    "ml": (bibliography: "Bibliography",),
    "mr": (bibliography: "संदर्भसूची",),
    "ms": (bibliography: "Bibliografi",),
    "nb": (bibliography: "Bibliografi",),
    "nl": (bibliography: "Bibliografie",),
    "nn": (bibliography: "Bibliografi",),
    "no": (bibliography: "Bibliografi",),
    "oc": (bibliography: "Bibliografia",),
    "or": (bibliography: "ସୂଚନା",),
    "pa": (bibliography: "ਸੰਬੰਧਤ ਹਵਾਲੇ",),
    "pl": (bibliography: "Piśmiennictwo",),
    "pms": (bibliography: "Bibliografìa",),
    "pt-PT": (bibliography: "Bibliografia",),
    "pt": (bibliography: "Bibliografia",),
    "rm": (bibliography: "Index bibliografic",),
    "ro": (bibliography: "Bibliografie",),
    "ru": (bibliography: "Библиография",),
    "se": (bibliography: "Girjjálašvuohta",),
    "si": (bibliography: "ග්‍රන්ථ නාමාවලිය",),
    "sk": (bibliography: "Bibliografia",),
    "sl": (bibliography: "Literatura",),
    "sq": (bibliography: "Bibliografi",),
    "sr": (bibliography: "Литература",),
    "sv": (bibliography: "Referenser",),
    "ta": (bibliography: "Bibliography",),
    "te": (bibliography: "గ్రంథాల జాబితా",),
    "th": (bibliography: "บรรณานุกรม",),
    "tk": (bibliography: "Çeşmeler",),
    "tl": (bibliography: "Bibliograpiya",),
    "tr": (bibliography: "Kaynakça",),
    "ug": (bibliography: "پايدىلانما",),
    "uk": (bibliography: "Бібліографія",),
    "ur": (bibliography: "کتابیات",),
    "vi": (bibliography: "Tài liệu tham khảo",),
    "zh-TW": (bibliography: "書目",),
    "zh": (bibliography: "参考文献",),
)

#let bibliography-term(lang, region: none) = localized-term(
    bibliography-terms, "bibliography", lang, region: region
)

#let captionless-figure(it) = context {
    if it.caption == none and it.numbering != none {
        let n = std.numbering(it.numbering, ..it.counter.at(it.location()))
        let caption = align(center)[#it.supplement#sym.space.nobreak#n]
        if it.kind == table or it.kind == raw {
            block[#caption#v(it.gap)#it]
        } else {
            block[#it#v(it.gap)#caption]
        }
    } else { it }
}

#let equation-reference(it) = {
    let el = it.element
    if (
        el != none and el.func() == math.equation and it.form == "normal"
        and el.numbering != none
    ) {
        let loc = el.location()
        let n = std.numbering(
            "1.1", counter(heading).at(loc).first(),
            ..counter(math.equation).at(loc)
        )
        let sup = if it.supplement == auto { el.supplement }
            else { it.supplement }
        link(loc, if sup == none or sup == [] { [#n] }
            else { [#sup#sym.space.nobreak#n] })
    } else { it }
}

#let writer-numbering(
            headings: false, figures: "document", equations: "labelled", body
        ) = {
    let hpat = if headings == true { "1.1" }
        else if headings == false { none } else { headings }
    let section = figures == "section" and hpat != none
    let chapter() = counter(heading).get().first()
    set heading(numbering: hpat)
    set figure(numbering: if section {
        (..n) => std.numbering("1.1", chapter(), ..n)
    } else { "1" })
    set math.equation(numbering: if equations == "none" { none }
        else if section {
            (..n) => "(" + std.numbering("1.1", chapter(), ..n) + ")"
        } else { "(1)" })
    show figure.where(kind: table): set figure.caption(position: top)
    show figure.where(kind: raw): set figure.caption(position: top)
    show heading: it => {
        if section and it.level == 1 and it.numbering != none {
            counter(figure.where(kind: image)).update(0)
            counter(figure.where(kind: table)).update(0)
            counter(figure.where(kind: raw)).update(0)
            counter(math.equation).update(0)
        }
        it
        if oracle-on { context oracle("heading", it,
            if it.numbering == none { none } else {
                std.numbering(it.numbering, ..counter(heading).at(it.location()))
            }
        ) }
    }
    show figure: it => {
        captionless-figure(it)
        if oracle-on { context oracle(figure-kind(it), it,
            if it.numbering == none { none } else {
                std.numbering(it.numbering, ..it.counter.at(it.location()))
            }
        ) }
    }
    show math.equation.where(block: true): it => {
        if (
            equations == "labelled" and not it.has("label")
            and it.numbering != none
        ) {
            // Typst already counted this equation; undo the hidden number.
            counter(math.equation).update(n => n - 1)
            math.equation(block: true, numbering: none, it.body)
        } else {
            it
            if oracle-on { context oracle("equation", it,
                if it.numbering == none { none } else {
                    std.numbering(
                        it.numbering, ..counter(math.equation).at(it.location())
                    )
                }
            ) }
        }
    }
    show footnote: it => {
        it
        if oracle-on { context oracle("footnote", it,
            std.numbering(it.numbering, ..counter(footnote).at(it.location()))
        ) }
    }
    if section {
        show ref: equation-reference
        body
    } else { body }
}
