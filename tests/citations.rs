//! Public library, driver and resolver citation contracts.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use writer::cite::Library;
use writer::cite::render::{Request, render};
use writer::config::{Config, HeadingNumbering};
use writer::error::{Diagnostics, Error, Level};
use writer::ir::*;
use writer::pipeline::{self, BuildOptions, Mode};
use writer::resolve::{ResolveOptions, resolve};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn library(style: &str) -> Library {
    let mut library =
        Library::load(&[fixtures().join("bib/refs.bib")]).unwrap();
    library.set_style(style, &fixtures()).unwrap();
    library
}

fn item(key: &str) -> CiteItem {
    CiteItem { key: key.into(), ..CiteItem::default() }
}

fn cite(items: Vec<CiteItem>, mode: CiteMode) -> Inline {
    Inline::Cite {
        items,
        mode,
        resolved: None,
        span: Some(SourceSpan {
            start_line: 7,
            end_line: 7,
            cell: None,
            file: None,
        }),
    }
}

fn note(label: &str, blocks: Vec<Block>) -> Inline {
    Inline::Footnote { blocks, label: Some(label.into()), number: None }
}

fn document(blocks: Vec<Block>) -> Document {
    let base_dir = fixtures().join("md");
    Document {
        meta: DocMeta::default(),
        blocks,
        features: Features::default(),
        source: SourceInfo {
            path: base_dir.join("citations.md"),
            base_dir,
            kind: SourceKind::Markdown,
        },
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Resolved::default(),
    }
}

fn resolve_with(document: &mut Document, library: &Library) -> Diagnostics {
    let config = Config::default();
    resolve(
        document,
        &ResolveOptions {
            config: &config,
            library: Some(library),
            lenient: false,
        },
    )
}

fn citations(document: &Document) -> Vec<Inline> {
    let mut found = Vec::new();
    writer::ir::walk_inlines(document, &mut |inline| {
        if matches!(inline, Inline::Cite { .. }) {
            found.push(inline.clone());
        }
    });
    found
}

fn citation(inline: &Inline) -> &ResolvedCite {
    let Inline::Cite { resolved: Some(resolved), .. } = inline else {
        panic!("expected a resolved Cite: {inline:?}");
    };
    resolved
}

fn paragraph_text(block: &Block) -> String {
    let Block::Paragraph { inlines, .. } = block else {
        panic!("expected Paragraph: {block:?}");
    };
    plain_text(inlines)
}

fn generated_notes(resolved: &ResolvedCite) -> Vec<u32> {
    resolved
        .inlines
        .iter()
        .filter_map(|inline| {
            let Inline::Footnote { blocks, label, number } = inline else {
                return None;
            };
            assert!(label.is_none());
            assert_eq!(blocks.len(), 1);
            assert!(matches!(&blocks[0], Block::Paragraph { span: None, .. }));
            assert!(!paragraph_text(&blocks[0]).is_empty());
            Some(number.expect("generated notes have preallocated numbers"))
        })
        .collect()
}

#[test]
fn library_merges_real_bib_and_yaml_and_reports_cross_file_duplicates() {
    let paths =
        [fixtures().join("bib/refs.bib"), fixtures().join("bib/refs.yaml")];
    let library = Library::load(&paths).unwrap();
    assert_eq!(library.files(), paths);
    assert_eq!(library.style_spec(), "ieee");
    assert_eq!(
        library.keys(),
        BTreeSet::from([
            "doe2020".into(),
            "knuth1984".into(),
            "lee2019".into(),
            "k1".into(),
        ])
    );
    let first = fixtures().join("bib/dup-a.bib");
    let conflict = fixtures().join("bib/dup-b.bib");
    let error = Library::load(&[first, conflict.clone()]).unwrap_err();
    let Error::Resource { what, path, message } = error else {
        panic!("wrong error: {error:?}");
    };
    assert_eq!(what, "bibliography");
    assert_eq!(path, conflict);
    assert_eq!(message, "duplicate bibliography keys: shared");
}

#[test]
fn public_driver_matches_verified_ieee_strings_and_formatted_bibliography() {
    let library = library("ieee");
    let mut located = item("knuth1984");
    located.locator = Some("p. 12".into());
    let items = [
        vec![item("doe2020")],
        vec![located],
        vec![item("doe2020"), item("lee2019")],
        vec![item("doe2020")],
        vec![item("doe2020")],
        vec![item("lee2019")],
        vec![item("knuth1984")],
    ];
    let modes = [
        CiteMode::Normal,
        CiteMode::Normal,
        CiteMode::Normal,
        CiteMode::Prose,
        CiteMode::Year,
        CiteMode::Author,
        CiteMode::None,
    ];
    let requests: Vec<_> = items
        .iter()
        .zip(modes)
        .map(|(items, mode)| Request { items, mode })
        .collect();
    let output = render(&library, "en", &requests);
    let strings: Vec<_> =
        output.citations.iter().map(|value| plain_text(value)).collect();
    assert_eq!(
        strings,
        [
            "[1]", "[2, p. 12]", "[1], [3]", "J. Doe and J. Smith [1]", "2020",
            "A. Lee, B. Kim, and C. Chen", "",
        ]
    );
    let bibliography = output.bibliography.unwrap();
    assert_eq!(bibliography.style, "ieee");
    assert!(!bibliography.hanging_indent);
    assert_eq!(
        bibliography
            .entries
            .iter()
            .map(|entry| entry.key.as_str())
            .collect::<Vec<_>>(),
        ["doe2020", "knuth1984", "lee2019"]
    );
    assert_eq!(
        plain_text(bibliography.entries[1].prefix.as_ref().unwrap()),
        "[2]"
    );
    assert_eq!(
        plain_text(&bibliography.entries[1].inlines),
        "D. E. Knuth, The TeXbook. Reading, MA: Addison-Wesley, 1984.",
    );
    assert!(bibliography.entries[1].inlines.iter().any(|inline| {
        matches!(inline, Inline::Emph { inlines } if plain_text(inlines) == "The TeXbook")
    }));
    assert!(bibliography.entries[0].inlines.iter().any(|inline| {
        matches!(inline, Inline::Link { url, title: None, .. } if url == "https://doi.org/10.1000/xyz123")
    }));
}

#[test]
fn hidden_only_citations_list_entries_without_text_or_generated_notes() {
    for style in ["ieee", "chicago-notes"] {
        let library = library(style);
        let mut document = document(vec![Block::paragraph(vec![cite(
            vec![item("knuth1984")],
            CiteMode::None,
        )])]);
        assert!(resolve_with(&mut document, &library).is_empty());
        let cites = citations(&document);
        assert_eq!(cites.len(), 1);
        assert_eq!(plain_text(&citation(&cites[0]).inlines), "");
        assert!(!citation(&cites[0]).note);
        assert_eq!(document.resolved.footnotes, 0);
        assert!(!document.features.footnotes);
        assert!(document.features.citations);
        let bibliography = document.resolved.bibliography.as_ref().unwrap();
        assert_eq!(bibliography.entries.len(), 1);
        assert_eq!(bibliography.entries[0].key, "knuth1984");
        assert!(matches!(
            document.blocks.last(),
            Some(Block::Bibliography { span: None })
        ));
    }
}

#[test]
fn apa_subgroups_keep_prefixes_suffixes_custom_locators_and_order() {
    let library = library("apa");
    let mut located = item("knuth1984");
    located.prefix = Some(vec![Inline::text("see")]);
    located.locator = Some("p. 12".into());
    let mut second = item("lee2019");
    second.prefix = Some(vec![Inline::text("see")]);
    let mut after = item("doe2020");
    after.suffix = Some(vec![Inline::text("after")]);
    let mut document = document(vec![
        Block::paragraph(vec![cite(vec![located], CiteMode::Normal)]),
        Block::paragraph(vec![cite(
            vec![item("doe2020"), second],
            CiteMode::Normal,
        )]),
        Block::paragraph(vec![cite(
            vec![after, item("knuth1984")],
            CiteMode::Normal,
        )]),
    ]);
    assert!(resolve_with(&mut document, &library).is_empty());
    assert_eq!(paragraph_text(&document.blocks[0]), "see (Knuth, 1984, p. 12)");
    assert_eq!(
        paragraph_text(&document.blocks[1]),
        "(Doe & Smith, 2020) see (Lee et al., 2019)"
    );
    assert_eq!(
        paragraph_text(&document.blocks[2]),
        "(Doe & Smith, 2020) after (Knuth, 1984)"
    );
    let bibliography = document.resolved.bibliography.as_ref().unwrap();
    assert!(bibliography.hanging_indent);
    assert_eq!(
        bibliography
            .entries
            .iter()
            .map(|entry| entry.key.as_str())
            .collect::<Vec<_>>(),
        ["doe2020", "knuth1984", "lee2019"]
    );
}

#[test]
fn nested_cite_and_forward_reference_affixes_finish_before_display_copies() {
    let library = library("ieee");
    let mut outer = item("doe2020");
    outer.prefix = Some(vec![cite(vec![item("knuth1984")], CiteMode::Normal)]);
    outer.suffix = Some(vec![Inline::Ref {
        target: "sec-later".into(),
        supplement: Some(vec![Inline::text("Chapter")]),
        form: RefForm::Normal,
        resolved: None,
        span: None,
    }]);
    let mut document = document(vec![
        Block::paragraph(vec![cite(vec![outer], CiteMode::Normal)]),
        Block::Heading {
            level: 1,
            inlines: vec![Inline::text("Later")],
            attrs: Attrs { id: Some("sec-later".into()), ..Attrs::default() },
            number: None,
            span: None,
        },
    ]);
    let mut config = Config::default();
    config.numbering.headings = HeadingNumbering::Pattern("1".into());
    let diagnostics = resolve(
        &mut document,
        &ResolveOptions {
            config: &config,
            library: Some(&library),
            lenient: false,
        },
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(paragraph_text(&document.blocks[0]), "[2] [1] Chapter\u{a0}1");
    let cites = citations(&document);
    assert_eq!(cites.len(), 2);
    assert_eq!(plain_text(&citation(&cites[1]).inlines), "[2]");
    assert!(document.invariant_violations().is_empty());
}

#[test]
fn classification_keeps_keys_labels_ambiguity_and_missing_targets_distinct() {
    let library = library("ieee");
    let source = SourceInfo {
        path: fixtures().join("md/citations.md"),
        base_dir: fixtures().join("md"),
        kind: SourceKind::Markdown,
    };
    let mut parsed = writer::markdown::parse(
        &source,
        "# Label {#sec-only}\n\n# Collision {#doe2020}\n\n\
         A label @sec-only.\n\nA citation @knuth1984.\n\n\
         A collision @doe2020.\n\nMixed [@sec-only; @lee2019].\n\n\
         An unknown key @not-in-library.\n",
        &Config::default(),
    )
    .unwrap();
    let diagnostics = resolve_with(&mut parsed.document, &library);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|finding| finding.code == "ambiguous-label")
            .count(),
        2
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|finding| finding.code == "unresolved-reference")
            .count(),
        1
    );
    assert!(diagnostics.iter().all(|finding| {
        finding.level == Level::Error
            && finding.path.as_ref() == Some(&source.path)
            && finding.span.is_some()
    }));
    let missing = diagnostics
        .iter()
        .find(|finding| finding.code == "unresolved-reference")
        .unwrap();
    assert!(
        missing.message.contains("no bibliography key either"),
        "{missing:?}"
    );
    let cites = citations(&parsed.document);
    assert_eq!(cites.len(), 1);
    assert_eq!(plain_text(&citation(&cites[0]).inlines), "[1]");
    assert!(parsed.document.invariant_violations().is_empty());
}

#[test]
fn required_apa_fixture_exercises_all_modes_merging_and_negative_groups() {
    let mut options = BuildOptions::new(fixtures().join("md/citations.md"));
    options.mode = Mode::Check;
    let prepared = pipeline::prepare(&options).unwrap();
    let cites = citations(&prepared.document);
    let modes: BTreeSet<_> = cites
        .iter()
        .filter_map(|inline| {
            let Inline::Cite { mode, .. } = inline else {
                return None;
            };
            Some(format!("{mode:?}"))
        })
        .collect();
    assert_eq!(
        modes,
        BTreeSet::from(
            ["Normal", "Prose", "Year", "Author", "Full", "None",]
                .map(str::to_owned)
        )
    );
    for marker in ["Bare citations merge", "Bracketed citations also merge"] {
        let inlines = prepared
            .document
            .blocks
            .iter()
            .find_map(|block| {
                let Block::Paragraph { inlines, .. } = block else {
                    return None;
                };
                plain_text(inlines).starts_with(marker).then_some(inlines)
            })
            .unwrap();
        let groups: Vec<_> = inlines
            .iter()
            .filter_map(|inline| {
                let Inline::Cite { items, mode: CiteMode::Normal, .. } = inline
                else {
                    return None;
                };
                Some(
                    items
                        .iter()
                        .map(|item| item.key.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(groups, [vec!["doe2020", "lee2019"]], "{marker}");
    }
    assert_eq!(
        prepared
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "ambiguous-label")
            .count(),
        1
    );
    assert_eq!(
        prepared
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "unresolved-reference")
            .count(),
        1
    );
    assert!(cites.iter().all(|inline| matches!(
        inline,
        Inline::Cite { resolved: Some(_), .. }
    )));
    assert_eq!(
        prepared.document.resolved.bibliography.as_ref().unwrap().style,
        "apa"
    );
    assert!(prepared.document.invariant_violations().is_empty());
}

#[test]
fn required_note_fixture_interleaves_regular_repeated_and_nested_notes() {
    let prepared = pipeline::prepare(&BuildOptions::new(
        fixtures().join("md/citations-notes.md"),
    ))
    .unwrap();
    assert!(!prepared.diagnostics.has_errors(), "{:?}", prepared.diagnostics);
    assert_eq!(
        prepared
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "cite-prose-note-style")
            .count(),
        1
    );
    assert_eq!(prepared.document.resolved.footnotes, 6);
    assert!(prepared.document.features.footnotes);
    let cites = citations(&prepared.document);
    let generated: Vec<_> = cites
        .iter()
        .flat_map(|inline| generated_notes(citation(inline)))
        .collect();
    assert_eq!(generated, [2, 3, 5, 6]);
    let mut ordinary = Vec::new();
    let mut repeated = Vec::new();
    writer::ir::walk_inlines(&prepared.document, &mut |inline| match inline {
        Inline::Footnote { number, .. } => ordinary.push(*number),
        Inline::FootnoteRef { number, .. } => repeated.push(*number),
        _ => {}
    });
    assert_eq!(ordinary, [Some(1), Some(4)]);
    assert_eq!(repeated, [Some(1)]);
    assert_eq!(cites.iter().filter(|inline| citation(inline).note).count(), 4);
    assert!(prepared.document.invariant_violations().is_empty());
}

#[test]
fn note_subgroups_allocate_one_note_each_with_affixes_outside_the_notes() {
    let library = library("chicago-notes");
    let mut first = item("doe2020");
    first.suffix = Some(vec![Inline::text("after")]);
    let mut second = item("knuth1984");
    second.prefix = Some(vec![Inline::text("see")]);
    let mut document = document(vec![Block::paragraph(vec![cite(
        vec![first, second],
        CiteMode::Normal,
    )])]);
    assert!(resolve_with(&mut document, &library).is_empty());
    let cites = citations(&document);
    let resolved = citation(&cites[0]);
    assert!(resolved.note);
    assert_eq!(generated_notes(resolved), [1, 2]);
    assert_eq!(plain_text(&resolved.inlines), " after see ");
    assert_eq!(document.resolved.footnotes, 2);
    assert!(document.features.footnotes);
}

#[test]
fn non_normal_note_modes_do_not_increment_footnotes() {
    let library = library("chicago-notes");
    let mut document = document(
        [CiteMode::Prose, CiteMode::Year, CiteMode::Author, CiteMode::Full]
            .map(|mode| {
                Block::paragraph(vec![cite(vec![item("doe2020")], mode)])
            })
            .into(),
    );
    let diagnostics = resolve_with(&mut document, &library);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics.iter().next().unwrap().code,
        "cite-prose-note-style"
    );
    assert_eq!(
        diagnostics.iter().next().unwrap().message,
        "prose citations render oddly in note styles (`chicago-notes`)",
    );
    assert_eq!(document.resolved.footnotes, 0);
    assert!(!document.features.footnotes);
    assert!(citations(&document).iter().all(|inline| !citation(inline).note));
}

#[test]
fn abstract_notes_precede_body_and_note_styles_preserve_source_spans() {
    let library = library("chicago-notes");
    let mut body = cite(vec![item("knuth1984")], CiteMode::Normal);
    let expected_span = SourceSpan {
        start_line: 3,
        end_line: 3,
        cell: Some(9),
        file: Some(fixtures().join("ipynb/outputs.ipynb")),
    };
    let Inline::Cite { span, .. } = &mut body else {
        unreachable!()
    };
    *span = Some(expected_span.clone());
    let mut document = document(vec![Block::paragraph(vec![
        note(
            "body",
            vec![Block::paragraph(vec![Inline::text("Regular note")])],
        ),
        body,
    ])]);
    document.meta.abstract_ = Some(vec![Block::paragraph(vec![cite(
        vec![item("doe2020")],
        CiteMode::Normal,
    )])]);
    assert!(resolve_with(&mut document, &library).is_empty());
    let cites = citations(&document);
    assert_eq!(generated_notes(citation(&cites[0])), [1]);
    assert_eq!(generated_notes(citation(&cites[1])), [3]);
    assert!(
        matches!(&cites[1], Inline::Cite { span: Some(span), .. } if span == &expected_span)
    );
    assert_eq!(document.resolved.footnotes, 3);
}

#[test]
fn every_notebook_candidate_is_requested_but_note_counters_resume_primary() {
    for style in ["ieee", "chicago-notes"] {
        let library = library(style);
        let mut document = document(vec![
            Block::Cell(Cell {
                show_output: true,
                outputs: vec![CellOutput {
                    candidates: ["doe2020", "knuth1984"]
                        .map(|key| OutputCandidate {
                            mime: "text/markdown".into(),
                            blocks: vec![Block::paragraph(vec![cite(
                                vec![item(key)],
                                CiteMode::Normal,
                            )])],
                        })
                        .into(),
                    ..CellOutput::default()
                }],
                ..Cell::default()
            }),
            Block::paragraph(vec![cite(
                vec![item("lee2019")],
                CiteMode::Normal,
            )]),
        ]);
        assert!(resolve_with(&mut document, &library).is_empty());
        let cites = citations(&document);
        assert_eq!(cites.len(), 3);
        if style == "ieee" {
            assert_eq!(
                cites
                    .iter()
                    .map(|inline| plain_text(&citation(inline).inlines))
                    .collect::<Vec<_>>(),
                ["[1]", "[2]", "[3]"]
            );
            assert_eq!(document.resolved.footnotes, 0);
        } else {
            assert_eq!(
                cites
                    .iter()
                    .map(|inline| generated_notes(citation(inline)))
                    .collect::<Vec<_>>(),
                [vec![1], vec![1], vec![2]]
            );
            assert_eq!(document.resolved.footnotes, 2);
        }
        assert_eq!(
            document.resolved.bibliography.as_ref().unwrap().entries.len(),
            3
        );
    }
}

#[test]
fn table_and_other_figure_order_matches_the_existing_document_walk() {
    let library = library("ieee");
    let mut document = document(vec![
        Block::Figure {
            kind: FigureKind::Table,
            caption: Some(vec![cite(
                vec![item("knuth1984")],
                CiteMode::Normal,
            )]),
            body: vec![Block::paragraph(vec![cite(
                vec![item("doe2020")],
                CiteMode::Normal,
            )])],
            attrs: Attrs::default(),
            number: None,
            span: None,
        },
        Block::Figure {
            kind: FigureKind::Other,
            caption: Some(vec![cite(
                vec![item("knuth1984")],
                CiteMode::Normal,
            )]),
            body: vec![Block::paragraph(vec![cite(
                vec![item("lee2019")],
                CiteMode::Normal,
            )])],
            attrs: Attrs::default(),
            number: None,
            span: None,
        },
    ]);
    assert!(resolve_with(&mut document, &library).is_empty());
    let Block::Figure { caption, body, .. } = &document.blocks[0] else {
        unreachable!()
    };
    assert_eq!(plain_text(caption.as_ref().unwrap()), "[1]");
    assert_eq!(paragraph_text(&body[0]), "[2]");
    let Block::Figure { caption, body, .. } = &document.blocks[1] else {
        unreachable!()
    };
    assert_eq!(paragraph_text(&body[0]), "[3]");
    assert_eq!(plain_text(caption.as_ref().unwrap()), "[1]");
}

#[test]
fn no_requests_do_not_insert_a_bibliography() {
    let library = library("ieee");
    let mut document =
        document(vec![Block::paragraph(vec![Inline::text("No citation")])]);
    assert!(resolve_with(&mut document, &library).is_empty());
    assert!(document.resolved.bibliography.is_none());
    assert_eq!(document.blocks.len(), 1);
    assert!(render(&library, "en", &[]).bibliography.is_none());
}
