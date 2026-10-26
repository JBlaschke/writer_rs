//! T17 reference and numbering contracts on frontend-independent, unresolved IR.

use std::collections::BTreeMap;
use std::path::PathBuf;

use writer::config::{
    Config, EquationNumbering, FigureNumbering, HeadingNumbering, Toc,
};
use writer::error::{Diagnostics, Level};
use writer::ir::*;
use writer::resolve::{ResolveOptions, resolve};

fn span(line: usize) -> Option<SourceSpan> {
    Some(SourceSpan {
        start_line: line,
        end_line: line,
        cell: None,
        file: None,
    })
}

fn attrs(id: &str, classes: &[&str]) -> Attrs {
    Attrs {
        id: (!id.is_empty()).then(|| id.into()),
        classes: classes.iter().map(|class| (*class).into()).collect(),
        ..Attrs::default()
    }
}

fn text(value: &str) -> Inlines {
    vec![Inline::text(value)]
}

fn paragraph(inlines: Inlines) -> Block {
    Block::Paragraph { inlines, span: span(1) }
}

fn heading(level: u8, title: &str, id: &str, classes: &[&str]) -> Block {
    Block::Heading {
        level,
        inlines: text(title),
        attrs: attrs(id, classes),
        number: None,
        span: span(1),
    }
}

fn image(alt: Inlines) -> Inline {
    Inline::Image {
        alt,
        url: "image.png".into(),
        title: None,
        attrs: Attrs::default(),
    }
}

fn table() -> Block {
    Block::Table {
        columns: vec![ColumnSpec::default()],
        head: vec![Row {
            cells: vec![TableCell {
                inlines: text("x"),
                header: true,
                ..TableCell::default()
            }],
        }],
        body: vec![Row {
            cells: vec![TableCell {
                inlines: text("1"),
                ..TableCell::default()
            }],
        }],
        foot: Vec::new(),
        attrs: Attrs::default(),
        span: span(1),
    }
}

fn figure(kind: FigureKind, id: &str, caption: &str) -> Block {
    let body = match kind {
        FigureKind::Image => vec![paragraph(vec![image(text(caption))])],
        FigureKind::Table => vec![table()],
        FigureKind::Listing => vec![Block::CodeBlock {
            lang: Some("rust".into()),
            text: "fn main() {}".into(),
            attrs: Attrs::default(),
            span: span(1),
        }],
        FigureKind::Other => vec![paragraph(text("Some other content."))],
    };
    Block::Figure {
        kind,
        caption: Some(text(caption)),
        body,
        attrs: attrs(id, &[]),
        number: None,
        span: span(1),
    }
}

fn equation(source: &str, id: &str) -> Block {
    Block::Math {
        source: source.into(),
        dialect: MathDialect::Latex,
        attrs: attrs(id, &[]),
        number: None,
        span: span(1),
    }
}

fn reference(target: &str, supplement: Option<&str>, form: RefForm) -> Inline {
    Inline::Ref {
        target: target.into(),
        supplement: supplement
            .map(|s| if s.is_empty() { Vec::new() } else { text(s) }),
        form,
        resolved: None,
        span: span(1),
    }
}

fn item(key: &str) -> CiteItem {
    CiteItem { key: key.into(), prefix: None, locator: None, suffix: None }
}

fn cite(items: Vec<CiteItem>, mode: CiteMode) -> Inline {
    Inline::Cite { items, mode, resolved: None, span: span(1) }
}

fn note(label: Option<&str>, inlines: Inlines) -> Inline {
    Inline::Footnote {
        blocks: vec![paragraph(inlines)],
        label: label.map(str::to_string),
        number: None,
    }
}

fn note_ref(label: &str) -> Inline {
    Inline::FootnoteRef { label: label.into(), number: None }
}

fn doc(blocks: Vec<Block>) -> Document {
    Document {
        meta: DocMeta::default(),
        blocks,
        features: Features::default(),
        source: SourceInfo {
            path: "/docs/main.md".into(),
            kind: SourceKind::Markdown,
            base_dir: "/docs".into(),
        },
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Resolved::default(),
    }
}

fn policy(
    pattern: Option<&str>, figures: FigureNumbering,
    equations: EquationNumbering,
) -> Config {
    let mut config = Config::default();
    config.numbering.headings = pattern
        .map_or(HeadingNumbering::Off, |p| HeadingNumbering::Pattern(p.into()));
    config.numbering.figures = figures;
    config.numbering.equations = equations;
    config
}

fn run(document: &mut Document, config: &Config) -> Diagnostics {
    resolve(document, &ResolveOptions { config, library: None, lenient: false })
}

fn codes(diagnostics: &Diagnostics) -> Vec<&'static str> {
    diagnostics.iter().map(|d| d.code).collect()
}

fn numbers(document: &Document) -> BTreeMap<String, Option<String>> {
    let mut result = BTreeMap::new();
    walk_blocks(document, &mut |block| {
        if let Block::Heading { attrs, number, .. }
        | Block::Figure { attrs, number, .. }
        | Block::Math { attrs, number, .. } = block
            && let Some(id) = &attrs.id
        {
            result.insert(id.clone(), number.clone());
        }
    });
    result
}

fn assert_numbers(document: &Document, expected: &[(&str, Option<&str>)]) {
    let actual = numbers(document);
    for (id, expected) in expected {
        assert_eq!(
            actual.get(*id).and_then(Option::as_deref),
            *expected,
            "{id}: {actual:?}"
        );
        assert!(actual.contains_key(*id), "missing {id}");
    }
}

fn reference_texts(document: &Document) -> Vec<String> {
    let mut result = Vec::new();
    walk_inlines(document, &mut |inline| {
        if let Inline::Ref { resolved, .. } = inline {
            let resolved =
                resolved.as_ref().expect("every remaining ref resolves");
            result.push(plain_text(&resolved.text));
        }
    });
    result
}

fn numbering_document() -> Document {
    doc(vec![
        figure(FigureKind::Image, "fig-zero", "before"),
        heading(1, "One", "sec-one", &[]),
        figure(FigureKind::Image, "fig-a", "a"),
        figure(FigureKind::Table, "tbl-a", "t"),
        figure(FigureKind::Listing, "lst-a", "code"),
        figure(FigureKind::Other, "fig-other", "other"),
        equation("x = 1", ""),
        equation("y = 2", "eq-a"),
        heading(2, "Sub", "sec-sub", &[]),
        heading(3, "Deep", "sec-deep", &[]),
        heading(1, "Interlude", "sec-interlude", &["unnumbered"]),
        figure(FigureKind::Image, "fig-int", "in interlude"),
        heading(2, "Under interlude", "sec-under", &[]),
        heading(1, "Two", "sec-two", &[]),
        figure(FigureKind::Image, "fig-c", "c"),
        equation("z = 3", "eq-b"),
        paragraph(vec![
            reference("sec-one", None, RefForm::Normal),
            reference("fig-zero", None, RefForm::Normal),
            reference("eq-a", None, RefForm::Normal),
            reference("eq-a", Some("Eq."), RefForm::Normal),
            reference("eq-a", Some(""), RefForm::Normal),
            reference("sec-two", Some("Chapter"), RefForm::Normal),
            reference("sec-interlude", None, RefForm::Normal),
        ]),
    ])
}

#[test]
fn numbering_md_base() {
    let mut document = numbering_document();
    let config = policy(
        Some("1."),
        FigureNumbering::Section,
        EquationNumbering::Labelled,
    );
    assert!(run(&mut document, &config).is_empty());
    assert_numbers(
        &document,
        &[
            ("fig-zero", Some("0.1")),
            ("sec-one", Some("1.")),
            ("fig-a", Some("1.1")),
            ("tbl-a", Some("1.1")),
            ("lst-a", Some("1.1")),
            ("fig-other", Some("1.2")),
            ("eq-a", Some("1.1")),
            ("sec-sub", Some("1.1.")),
            ("sec-deep", Some("1.1.1.")),
            ("sec-interlude", None),
            ("fig-int", Some("1.3")),
            ("sec-under", Some("1.2.")),
            ("sec-two", Some("2.")),
            ("fig-c", Some("2.1")),
            ("eq-b", Some("2.1")),
        ],
    );
    assert_eq!(
        document.resolved.labels["sec-one"].number.as_deref(),
        Some("1")
    );
    assert_eq!(
        reference_texts(&document),
        [
            "Section\u{a0}1", "Figure\u{a0}0.1", "Equation\u{a0}1.1",
            "Eq.\u{a0}1.1", "1.1", "Chapter\u{a0}2", "Interlude"
        ]
    );
    let Block::Paragraph { inlines, .. } = document.blocks.last().unwrap()
    else {
        panic!()
    };
    for inline in inlines {
        let Inline::Ref { resolved: Some(resolved), .. } = inline else {
            panic!("expected a resolved reference");
        };
        assert!(matches!(resolved.text.as_slice(), [Inline::Text { .. }]));
    }
    let display: Vec<_> = document
        .blocks
        .iter()
        .filter_map(|block| {
            if let Block::Math { number, .. } = block {
                Some(number.as_deref())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(display, [None, Some("1.1"), Some("2.1")]);
    assert!(document.invariant_violations().is_empty());
}

#[test]
fn numbering_md_variants() {
    let mut document = numbering_document();
    assert!(
        run(
            &mut document,
            &policy(
                Some("1.1"),
                FigureNumbering::Document,
                EquationNumbering::All
            )
        )
        .is_empty()
    );
    assert_numbers(
        &document,
        &[
            ("sec-one", Some("1")),
            ("sec-sub", Some("1.1")),
            ("sec-deep", Some("1.1.1")),
            ("sec-under", Some("1.2")),
            ("sec-two", Some("2")),
            ("fig-zero", Some("1")),
            ("fig-a", Some("2")),
            ("fig-other", Some("3")),
            ("fig-int", Some("4")),
            ("fig-c", Some("5")),
            ("tbl-a", Some("1")),
            ("lst-a", Some("1")),
            ("eq-a", Some("2")),
            ("eq-b", Some("3")),
        ],
    );
    let mut document = numbering_document();
    let diagnostics = run(
        &mut document,
        &policy(Some("A.1"), FigureNumbering::Section, EquationNumbering::None),
    );
    assert_eq!(codes(&diagnostics), ["unnumbered-target"; 3]);
    assert_numbers(
        &document,
        &[
            ("sec-one", Some("A")),
            ("sec-sub", Some("A.1")),
            ("sec-deep", Some("A.1.1")),
            ("sec-under", Some("A.2")),
            ("sec-two", Some("B")),
            ("fig-zero", Some("0.1")),
            ("fig-a", Some("1.1")),
            ("fig-other", Some("1.2")),
            ("fig-int", Some("1.3")),
            ("fig-c", Some("2.1")),
            ("eq-a", None),
            ("eq-b", None),
        ],
    );
    assert_eq!(reference_texts(&document)[5], "Chapter\u{a0}B");
    let mut document = numbering_document();
    let diagnostics = run(
        &mut document,
        &policy(None, FigureNumbering::Section, EquationNumbering::Labelled),
    );
    assert_eq!(codes(&diagnostics), ["numbering-section-without-headings"]);
    assert_numbers(
        &document,
        &[
            ("fig-zero", Some("1")),
            ("fig-a", Some("2")),
            ("fig-other", Some("3")),
            ("fig-int", Some("4")),
            ("fig-c", Some("5")),
            ("eq-a", Some("1")),
            ("eq-b", Some("2")),
        ],
    );
    assert_eq!(reference_texts(&document)[0], "One");
}

#[test]
fn refs_and_cites() {
    let mut unnumbered = figure(FigureKind::Image, "fig-u", "A caption");
    if let Block::Figure { attrs, .. } = &mut unnumbered {
        attrs.classes.push("unnumbered".into());
    }
    let mut prefixed = item("x");
    prefixed.prefix = Some(text("see"));
    prefixed.locator = Some("p. 2".into());
    let mut document = doc(vec![
        heading(1, "Heading", "x", &[]),
        unnumbered,
        Block::Div {
            attrs: attrs("div", &[]),
            blocks: Vec::new(),
            span: span(2),
        },
        Block::Callout {
            kind: "note".into(),
            title: Some(text("A title")),
            blocks: Vec::new(),
            attrs: attrs("callout", &[]),
            span: span(3),
        },
        paragraph(vec![
            reference("x", Some(""), RefForm::Normal),
            reference("x", Some("E"), RefForm::Normal),
            reference("fig-u", None, RefForm::Normal),
            reference("div", None, RefForm::Normal),
            reference("callout", None, RefForm::Normal),
            reference("fig-u", None, RefForm::Page),
            cite(vec![prefixed, item("x")], CiteMode::Normal),
            cite(vec![item("x")], CiteMode::Prose),
        ]),
    ]);
    let diagnostics = run(
        &mut document,
        &policy(
            Some("1"),
            FigureNumbering::Document,
            EquationNumbering::Labelled,
        ),
    );
    assert_eq!(
        codes(&diagnostics)
            .iter()
            .filter(|&&c| c == "unnumbered-target")
            .count(),
        3
    );
    assert_eq!(
        codes(&diagnostics)
            .iter()
            .filter(|&&c| c == "invalid-reference-mode")
            .count(),
        1
    );
    assert_eq!(
        reference_texts(&document),
        [
            "1", "E\u{a0}1", "A caption", "div", "A title", "A caption",
            "Section\u{a0}1", "Section\u{a0}1", "Section\u{a0}1"
        ]
    );
    let Block::Paragraph { inlines, .. } = document.blocks.last().unwrap()
    else {
        panic!()
    };
    assert!(inlines.windows(3).any(|window| {
        matches!(window, [
            Inline::Text { text: prefix }, Inline::Text { text: gap },
            Inline::Ref { .. },
        ] if prefix == "see" && gap == " ")
    }));
    assert!(
        inlines
            .iter()
            .any(|i| matches!(i, Inline::Text { text } if text == ", p. 2"))
    );

    for lenient in [false, true] {
        let mut unknown = document_with_failures();
        let diagnostics = resolve(
            &mut unknown,
            &ResolveOptions {
                config: &Config::default(),
                library: None,
                lenient,
            },
        );
        assert_eq!(diagnostics.len(), 4);
        assert!(diagnostics.iter().all(|d| d.code == "unresolved-reference"
            && d.level
                == if lenient {
                    Level::Warning
                } else {
                    Level::Error
                }));
        assert!(
            diagnostics
                .iter()
                .all(|d| d.message.contains("configure a bibliography"))
        );
        let Block::Paragraph { inlines, .. } = unknown.blocks.last().unwrap()
        else {
            panic!()
        };
        let placeholders: Vec<_> = inlines
            .iter()
            .filter_map(|inline| {
                if let Inline::Span { attrs, inlines } = inline {
                    assert_eq!(attrs.classes, ["unresolved-reference"]);
                    Some(plain_text(inlines))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(placeholders, ["[?nope]", "[?a; b]", "[?x; missing]"]);
    }
    let mut unnumbered_heading = doc(vec![
        heading(1, "Unnumbered", "x", &[]),
        paragraph(vec![reference("x", None, RefForm::Normal)]),
    ]);
    assert!(run(&mut unnumbered_heading, &Config::default()).is_empty());
    assert_eq!(reference_texts(&unnumbered_heading), ["Unnumbered"]);
}

fn document_with_failures() -> Document {
    doc(vec![
        heading(1, "Known", "x", &[]),
        paragraph(vec![
            reference("nope", None, RefForm::Normal),
            cite(vec![item("a"), item("b")], CiteMode::Normal),
            cite(vec![item("x"), item("missing")], CiteMode::Normal),
        ]),
    ])
}

#[test]
fn footnotes_scopes() {
    let mut included =
        paragraph(vec![note(Some("n"), text("Included")), note_ref("n")]);
    if let Block::Paragraph { span: Some(span), .. } = &mut included {
        span.file = Some(PathBuf::from("/docs/included.md"));
    }
    let mut document = doc(vec![
        paragraph(vec![
            note(
                Some("n"),
                vec![Inline::text("Parent"), note(None, text("Nested"))],
            ),
            note_ref("n"),
            note(Some("n"), text("Duplicate")),
            note(None, text("Inline")),
        ]),
        included,
        paragraph(vec![note_ref("missing")]),
    ]);
    let diagnostics = run(&mut document, &Config::default());
    assert_eq!(codes(&diagnostics), ["unresolved-reference"]);
    assert_eq!(document.resolved.footnotes, 4);
    assert!(document.resolved.labels.is_empty());
    let mut notes = Vec::new();
    walk_inlines(&document, &mut |inline| match inline {
        Inline::Footnote { number, .. } => notes.push((true, *number)),
        Inline::FootnoteRef { number, .. } => notes.push((false, *number)),
        _ => {}
    });
    assert_eq!(
        notes,
        [
            (true, Some(1)),
            (true, Some(2)),
            (false, Some(1)),
            (false, Some(1)),
            (true, Some(3)),
            (true, Some(4)),
            (false, Some(4)),
        ]
    );
    let mut typst = doc(vec![paragraph(vec![
        note(Some("fn-x"), text("Global")),
        reference("fn-x", None, RefForm::Normal),
        reference("fn-x", Some("s"), RefForm::Normal),
    ])]);
    typst.source.path = "/docs/main.typ".into();
    typst.source.kind = SourceKind::Typst;
    assert_eq!(
        codes(&run(&mut typst, &Config::default())),
        ["invalid-reference-mode"]
    );
    assert_eq!(typst.resolved.labels["fn-x"].kind, LabelKind::Footnote);
    assert_eq!(typst.resolved.labels["fn-x"].number.as_deref(), Some("1"));
    let Block::Paragraph { inlines, .. } = &typst.blocks[0] else {
        panic!()
    };
    assert!(
        inlines[1..]
            .iter()
            .all(|i| matches!(i, Inline::FootnoteRef { number: Some(1), .. }))
    );
}

#[test]
fn labels_and_auto_ids() {
    for lenient in [false, true] {
        let mut document = doc(vec![
            heading(1, "First", "same", &[]),
            heading(1, "Second", "same", &[]),
            heading(1, "Reserved", "refs", &[]),
            heading(1, "Collision", "sec-intro", &[]),
            heading(
                1,
                "Collision two",
                "fdabef51864cece78e14c461e4ab265d0b74de7",
                &[],
            ),
            Block::Div {
                attrs: attrs("invalid id", &[]),
                blocks: Vec::new(),
                span: span(2),
            },
        ]);
        let diagnostics = resolve(
            &mut document,
            &ResolveOptions {
                config: &Config::default(),
                library: None,
                lenient,
            },
        );
        for code in ["duplicate-label", "anchor-collision", "label-invalid"] {
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.code == code && d.level == Level::Error)
            );
        }
        assert!(codes(&diagnostics).contains(&"label-reserved"));
        assert_eq!(
            document.resolved.labels["same"].text.as_deref(),
            Some("First")
        );
        assert!(document.resolved.labels.contains_key("refs"));
        assert!(!document.resolved.labels.contains_key("invalid id"));
        let Block::Div { attrs, .. } = &document.blocks[5] else {
            panic!()
        };
        assert!(attrs.id.is_none());
    }
    let mut document = doc(vec![
        heading(1, "Explicit", "intro", &[]),
        heading(1, "1.2 GPU Allocation: Results!", "", &[]),
        heading(1, "Intro", "", &[]),
        heading(1, "Refs", "", &[]),
        heading(1, "42", "", &[]),
        heading(1, "42", "", &[]),
        heading(1, "Writer", "", &[]),
        heading(1, "Writer", "", &[]),
    ]);
    document.meta.abstract_ =
        Some(vec![heading(1, "Abstract heading", "", &[])]);
    let config = policy(
        Some("1"),
        FigureNumbering::Document,
        EquationNumbering::Labelled,
    );
    assert!(run(&mut document, &config).is_empty());
    let ids: Vec<_> = document
        .blocks
        .iter()
        .filter_map(|block| {
            if let Block::Heading { attrs, .. } = block {
                attrs.id.as_deref()
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        ids,
        [
            "intro", "gpu-allocation-results", "intro-1", "refs-1", "section",
            "section-1", "writer", "writer-1"
        ]
    );
    assert!(document.resolved.labels.contains_key("abstract-heading"));
    assert!(
        !document
            .resolved
            .toc
            .iter()
            .any(|entry| entry.anchor == "abstract-heading")
    );
    let Block::Heading { number, .. } =
        &document.meta.abstract_.as_ref().unwrap()[0]
    else {
        panic!()
    };
    assert!(number.is_none());
}

#[test]
fn toc_and_blocks() {
    let mut first = heading(1, "GPU allocation results", "", &[]);
    if let Block::Heading { inlines, .. } = &mut first {
        inlines.push(Inline::Emph {
            inlines: vec![note(None, text("Body note"))],
        });
    }
    let mut document = doc(vec![
        first,
        heading(2, "Hidden", "hidden", &["notoc"]),
        heading(2, "Unlisted", "unlisted", &["unlisted"]),
        heading(2, "Interlude", "interlude", &["unnumbered"]),
        Block::Bibliography { span: span(5) },
        Block::Bibliography { span: span(6) },
        figure(FigureKind::Image, "fig-a", "Caption"),
        paragraph(vec![
            Inline::Link {
                inlines: text("known"),
                url: "#gpu-allocation-results".into(),
                title: None,
                attrs: Attrs::default(),
            },
            Inline::Link {
                inlines: text("bibliography"),
                url: "#refs".into(),
                title: None,
                attrs: Attrs::default(),
            },
            Inline::Link {
                inlines: text("missing"),
                url: "#nope".into(),
                title: None,
                attrs: Attrs::default(),
            },
            image(vec![
                Inline::text("see "),
                reference("fig-a", None, RefForm::Normal),
                note(None, text("Alt note")),
            ]),
            cite(vec![item("unknown")], CiteMode::Normal),
        ]),
    ]);
    let mut config = policy(
        Some("1"),
        FigureNumbering::Document,
        EquationNumbering::Labelled,
    );
    config.document.toc = Toc::Depth(2);
    let diagnostics = run(&mut document, &config);
    assert_eq!(diagnostics.len(), 3);
    for code in [
        "bibliography-duplicate", "link-target-missing", "unresolved-reference",
    ] {
        assert!(codes(&diagnostics).contains(&code));
    }
    assert!(matches!(document.blocks[0], Block::Toc { depth: 2, span: None }));
    assert_eq!(
        document
            .blocks
            .iter()
            .filter(|b| matches!(b, Block::Bibliography { .. }))
            .count(),
        1
    );
    let entries = &document.resolved.toc;
    assert_eq!(
        entries.iter().map(|e| e.anchor.as_str()).collect::<Vec<_>>(),
        ["gpu-allocation-results", "interlude"]
    );
    assert!(entries[1].number.is_none());
    assert_eq!(plain_text(&entries[0].inlines), "GPU allocation results");
    let mut toc_document = doc(vec![paragraph(entries[0].inlines.clone())]);
    walk_inlines(&toc_document, &mut |inline| {
        assert!(!matches!(
            inline,
            Inline::Footnote { .. } | Inline::FootnoteRef { .. }
        ))
    });
    assert_eq!(document.resolved.footnotes, 1);
    assert!(document.features.footnotes);
    assert!(!document.features.citations);
    let mut alts = Vec::new();
    walk_inlines(&document, &mut |inline| {
        if let Inline::Image { alt, .. } = inline {
            alts.push(alt.clone());
        }
    });
    assert_eq!(alts.last().unwrap(), &text("see Figure\u{a0}1"));
    toc_document.blocks = vec![Block::Div {
        attrs: Attrs::default(),
        blocks: vec![Block::Toc { depth: 1, span: span(1) }],
        span: span(1),
    }];
    assert!(run(&mut toc_document, &config).is_empty());
    assert_eq!(toc_document.blocks.len(), 1);
    assert!(matches!(toc_document.blocks[0], Block::Div { .. }));
}

#[test]
fn cells_snapshot() {
    let candidate = |id| OutputCandidate {
        mime: "text/html".into(),
        blocks: vec![
            figure(FigureKind::Image, id, "Output"),
            paragraph(vec![note(None, text("Output note"))]),
        ],
    };
    let mut document = doc(vec![
        Block::Cell(Cell {
            show_output: true,
            outputs: vec![CellOutput {
                candidates: vec![candidate("primary"), candidate("alternate")],
                ..CellOutput::default()
            }],
            ..Cell::default()
        }),
        figure(FigureKind::Image, "after", "After"),
        paragraph(vec![note(None, text("After note"))]),
    ]);
    assert!(run(&mut document, &Config::default()).is_empty());
    assert_numbers(
        &document,
        &[
            ("primary", Some("1")),
            ("alternate", Some("1")),
            ("after", Some("2")),
        ],
    );
    assert!(document.resolved.labels.contains_key("primary"));
    assert!(!document.resolved.labels.contains_key("alternate"));
    assert_eq!(document.resolved.footnotes, 2);
    let mut notes = Vec::new();
    walk_inlines(&document, &mut |inline| {
        if let Inline::Footnote { number, .. } = inline {
            notes.push(*number);
        }
    });
    assert_eq!(notes, [Some(1), Some(1), Some(2)]);
}

#[test]
fn languages() {
    for (language, expected, diagnostic) in [
        ("de", "Abbildung\u{a0}1", None),
        ("x", "Figure\u{a0}1", Some("lang-invalid")),
    ] {
        let mut document = doc(vec![
            figure(FigureKind::Image, "fig-a", "Caption"),
            paragraph(vec![reference("fig-a", None, RefForm::Normal)]),
        ]);
        document.meta.lang = Some(language.into());
        let diagnostics = run(&mut document, &Config::default());
        assert_eq!(
            codes(&diagnostics),
            diagnostic.into_iter().collect::<Vec<_>>()
        );
        assert_eq!(reference_texts(&document), [expected]);
    }
}

#[test]
fn numbering_diagnostics() {
    let mut document = doc(vec![heading(1, "One", "one", &[])]);
    let mut config =
        policy(Some("x"), FigureNumbering::Document, EquationNumbering::None);
    assert_eq!(codes(&run(&mut document, &config)), ["numbering-invalid"]);
    assert_numbers(&document, &[("one", Some("1"))]);
    let mut document = doc(vec![
        heading(1, "One", "one", &[]),
        heading(3, "Deep", "deep", &[]),
    ]);
    config.numbering.headings = HeadingNumbering::Pattern("1.a".into());
    assert_eq!(
        codes(&run(&mut document, &config)),
        ["numbering-unrepresentable"]
    );
    assert_numbers(&document, &[("one", Some("1")), ("deep", Some("1.0.a"))]);
}

#[test]
fn empty_document() {
    let mut document = doc(Vec::new());
    assert!(run(&mut document, &Config::default()).is_empty());
    assert!(document.resolved.is_empty());
}

#[test]
fn markdown_numbering_to_html() {
    use writer::html::emit::{EmitOptions, KatexOnly, emit};

    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md");
    let source = SourceInfo {
        path: directory.join("numbering.md"),
        kind: SourceKind::Markdown,
        base_dir: directory,
    };
    let mut config = Config::default();
    let mut parsed = writer::markdown::parse(
        &source,
        include_str!("fixtures/md/numbering.md"),
        &config,
    )
    .unwrap();
    assert!(parsed.diagnostics.is_empty());
    config.merge_front_matter(&parsed.document.meta).unwrap();
    assert!(run(&mut parsed.document, &config).is_empty());
    assert_numbers(
        &parsed.document,
        &[
            ("sec-one", Some("1.")),
            ("fig-zero", Some("0.1")),
            ("fig-a", Some("1.1")),
            ("eq-a", Some("1.1")),
            ("sec-two", Some("2.")),
            ("fig-c", Some("2.1")),
        ],
    );
    let mut math = KatexOnly::default();
    let mut diagnostics = Diagnostics::default();
    let emitted = emit(
        &parsed.document,
        &mut EmitOptions {
            config: &config,
            positions: false,
            includes: &parsed.document.includes,
            math: &mut math,
            assets: &mut parsed.assets,
            diagnostics: &mut diagnostics,
        },
    );
    assert!(diagnostics.is_empty());
    assert!(emitted.body.contains("Figure\u{a0}0.1"));
    assert!(emitted.body.contains("id=\"sec-one\""));
    assert!(emitted.math.katex);
    assert_eq!(parsed.document.invariant_violations(), Vec::<String>::new());
}
