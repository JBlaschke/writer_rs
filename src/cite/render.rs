//! Stateful CSL citation rendering and literal formatting-tree lowering.

use hayagriva::citationberg::taxonomy::Locator;
use hayagriva::citationberg::{
    FontStyle, FontVariant, FontWeight, LocaleCode, TextDecoration,
    VerticalAlign,
};
use hayagriva::{
    BibliographyDriver, BibliographyRequest, CitationItem, CitationRequest,
    CitePurpose, ElemChild, Entry, Formatting, LocatorPayload, SpecificLocator,
};

use super::library::Library;
use crate::ir::{
    Attrs, BibEntry, Bibliography, CiteItem, CiteMode, Inline, Inlines,
};

/// One citation subgroup, in the document's request order.
pub struct Request<'a> {
    /// Bibliography items in document order.
    pub items: &'a [CiteItem],
    /// Citation rendering form.
    pub mode: CiteMode,
}

/// Formatted subgroups and their shared document bibliography.
pub struct CiteOutput {
    /// One rendered citation subgroup per request, in request order.
    pub citations: Vec<Inlines>,
    /// The formatted bibliography, absent only when no requests were made.
    pub bibliography: Option<Bibliography>,
}

/// Renders every request in one driver, then finishes its bibliography once.
pub fn render(
    library: &Library, locale: &str, requests: &[Request<'_>],
) -> CiteOutput {
    if requests.is_empty() {
        return CiteOutput { citations: Vec::new(), bibliography: None };
    }
    let locales = hayagriva::archive::locales();
    let mut driver = BibliographyDriver::<Entry>::new();
    for request in requests {
        let purpose = match request.mode {
            CiteMode::Normal | CiteMode::None => None,
            CiteMode::Prose => Some(CitePurpose::Prose),
            CiteMode::Year => Some(CitePurpose::Year),
            CiteMode::Author => Some(CitePurpose::Author),
            CiteMode::Full => Some(CitePurpose::Full),
        };
        // Resolution classifies keys before requesting rendering. Direct
        // callers with missing keys contribute no engine item for that key.
        let items = request
            .items
            .iter()
            .filter_map(|item| {
                let entry = library.entries.get(&item.key)?;
                let locator = item
                    .locator
                    .as_deref()
                    .filter(|text| !text.is_empty())
                    .map(|text| {
                        SpecificLocator(
                            Locator::Custom,
                            LocatorPayload::Str(text),
                        )
                    });
                Some(CitationItem::new(
                    entry,
                    locator,
                    None,
                    request.mode == CiteMode::None,
                    purpose,
                ))
            })
            .collect();
        driver.citation(CitationRequest::new(
            items,
            &library.style,
            Some(LocaleCode(locale.to_owned())),
            &locales,
            None,
        ));
    }
    let rendered = driver.finish(BibliographyRequest::new(
        &library.style,
        Some(LocaleCode(locale.to_owned())),
        &locales,
    ));
    let citations = rendered
        .citations
        .iter()
        .map(|citation| lower(&citation.citation.0))
        .collect();
    let (entries, hanging_indent) = rendered.bibliography.map_or_else(
        || (Vec::new(), false),
        |bibliography| {
            let entries = bibliography
                .items
                .into_iter()
                .map(|item| BibEntry {
                    prefix: item
                        .first_field
                        .as_ref()
                        .map(|field| lower(std::slice::from_ref(field))),
                    inlines: lower(&item.content.0),
                    key: item.key,
                })
                .collect();
            (entries, bibliography.hanging_indent)
        },
    );
    CiteOutput {
        citations,
        bibliography: Some(Bibliography {
            style: library.style_spec().into(),
            hanging_indent,
            entries,
        }),
    }
}

struct Run {
    text: String,
    formatting: Formatting,
    href: Option<String>,
}

fn lower(children: &[ElemChild]) -> Inlines {
    let mut runs = Vec::new();
    flatten(children, &mut runs);
    runs.into_iter()
        .map(|run| {
            let mut inline = Inline::Text { text: run.text };
            if run.formatting.text_decoration == TextDecoration::Underline {
                inline = Inline::Underline { inlines: vec![inline] };
            }
            if run.formatting.font_variant == FontVariant::SmallCaps {
                inline = Inline::SmallCaps { inlines: vec![inline] };
            }
            if run.formatting.font_style == FontStyle::Italic {
                inline = Inline::Emph { inlines: vec![inline] };
            }
            if run.formatting.font_weight == FontWeight::Bold {
                inline = Inline::Strong { inlines: vec![inline] };
            }
            inline = match run.formatting.vertical_align {
                VerticalAlign::Sup => {
                    Inline::Superscript { inlines: vec![inline] }
                }
                VerticalAlign::Sub => {
                    Inline::Subscript { inlines: vec![inline] }
                }
                VerticalAlign::None | VerticalAlign::Baseline => inline,
            };
            if let Some(url) = run.href {
                inline = Inline::Link {
                    url,
                    inlines: vec![inline],
                    title: None,
                    attrs: Attrs::default(),
                };
            }
            inline
        })
        .collect()
}

fn flatten(children: &[ElemChild], runs: &mut Vec<Run>) {
    for child in children {
        match child {
            ElemChild::Text(text) => {
                push_run(runs, &text.text, text.formatting, None)
            }
            ElemChild::Elem(element) => flatten(&element.children.0, runs),
            ElemChild::Markup(text) => {
                push_run(runs, text, Formatting::default(), None)
            }
            ElemChild::Link { text, url } => {
                push_run(runs, &text.text, text.formatting, Some(url))
            }
            ElemChild::Transparent { .. } => {}
        }
    }
}

fn push_run(
    runs: &mut Vec<Run>, text: &str, formatting: Formatting, href: Option<&str>,
) {
    if let Some(previous) = runs.last_mut()
        && previous.formatting == formatting
        && previous.href.as_deref() == href
    {
        previous.text.push_str(text);
    } else {
        runs.push(Run {
            text: text.into(),
            formatting,
            href: href.map(str::to_owned),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::plain_text;
    use hayagriva::citationberg::{Display, IndependentStyle};
    use hayagriva::{Elem, ElemChildren, ElemMeta, Formatted};
    use std::path::Path;

    fn sample(style: &str) -> Library {
        let mut library = Library::default();
        // This is the unchanged, independently verified R5 three-entry input.
        library.entries = hayagriva::io::from_biblatex_str(include_str!(
            "../../tests/fixtures/bib/refs.bib"
        ))
        .unwrap();
        library.set_style(style, Path::new("/unused")).unwrap();
        library
    }

    fn item(key: &str, locator: Option<&str>) -> CiteItem {
        CiteItem {
            key: key.into(),
            locator: locator.map(str::to_owned),
            prefix: None,
            suffix: None,
        }
    }

    fn formatted(text: &str, formatting: Formatting) -> ElemChild {
        ElemChild::Text(Formatted { text: text.into(), formatting })
    }

    fn linked(text: &str, formatting: Formatting, url: &str) -> ElemChild {
        ElemChild::Link {
            text: Formatted { text: text.into(), formatting },
            url: url.into(),
        }
    }

    #[test]
    fn all_formatting_flags_nest_in_the_specified_order_inside_the_link() {
        let formatting = Formatting {
            font_style: FontStyle::Italic,
            font_variant: FontVariant::SmallCaps,
            font_weight: FontWeight::Bold,
            text_decoration: TextDecoration::Underline,
            vertical_align: VerticalAlign::Sup,
        };
        let url = "https://example.org/a?x=1&y=2#part";
        assert_eq!(
            lower(&[linked("A & <literal>", formatting, url)]),
            vec![Inline::Link {
                url: url.into(),
                title: None,
                attrs: Attrs::default(),
                inlines: vec![Inline::Superscript {
                    inlines: vec![Inline::Strong {
                        inlines: vec![Inline::Emph {
                            inlines: vec![Inline::SmallCaps {
                                inlines: vec![Inline::Underline {
                                    inlines: vec![Inline::text(
                                        "A & <literal>"
                                    )],
                                }]
                            },]
                        }]
                    },]
                }],
            },]
        );
        assert_eq!(
            lower(&[formatted(
                "sub",
                Formatting {
                    vertical_align: VerticalAlign::Sub,
                    ..Formatting::default()
                }
            )]),
            vec![Inline::Subscript { inlines: vec![Inline::text("sub")] }]
        );
        assert_eq!(
            lower(&[formatted(
                "light",
                Formatting {
                    font_weight: FontWeight::Light,
                    vertical_align: VerticalAlign::Baseline,
                    ..Formatting::default()
                }
            )]),
            vec![Inline::text("light")]
        );
    }

    #[test]
    fn literal_markup_nested_elements_and_transparent_nodes_preserve_text_runs()
    {
        let normal = Formatting::default();
        let children = vec![
            formatted("one ", normal),
            ElemChild::Elem(Elem {
                display: Some(Display::Block),
                meta: Some(ElemMeta::Date),
                children: ElemChildren(vec![
                    ElemChild::Markup("<i>literal</i> & α".into()),
                    ElemChild::Transparent { cite_idx: 3, format: normal },
                    formatted("\n\tend", normal),
                ]),
            }),
        ];
        assert_eq!(
            lower(&children),
            vec![Inline::text("one <i>literal</i> & α\n\tend")]
        );
    }

    #[test]
    fn adjacent_runs_merge_only_when_full_formatting_and_href_are_equal() {
        let italic = Formatting {
            font_style: FontStyle::Italic,
            ..Formatting::default()
        };
        let url = "https://example.org/one";
        let children = vec![
            linked("one", italic, url),
            ElemChild::Elem(Elem {
                children: ElemChildren(vec![linked(" two", italic, url)]),
                display: None,
                meta: Some(ElemMeta::Text),
            }),
            linked("three", italic, "https://example.org/two"),
            formatted(
                "light",
                Formatting {
                    font_weight: FontWeight::Light,
                    ..Formatting::default()
                },
            ),
            formatted("normal", Formatting::default()),
        ];
        assert_eq!(
            lower(&children),
            vec![
                Inline::Link {
                    url: url.into(),
                    title: None,
                    attrs: Attrs::default(),
                    inlines: vec![Inline::Emph {
                        inlines: vec![Inline::text("one two")]
                    }],
                },
                Inline::Link {
                    url: "https://example.org/two".into(),
                    title: None,
                    attrs: Attrs::default(),
                    inlines: vec![Inline::Emph {
                        inlines: vec![Inline::text("three")]
                    }],
                },
                Inline::text("light"),
                Inline::text("normal"),
            ]
        );
    }

    #[test]
    fn ieee_requests_share_numbers_and_keep_document_order_across_modes() {
        let library = sample("ieee");
        let before = library.entries.clone();
        let doe = [item("doe2020", None)];
        let page = [item("knuth1984", Some("p. 12"))];
        let group = [item("doe2020", None), item("lee2019", None)];
        let lee = [item("lee2019", None)];
        let requests = [
            Request { items: &doe, mode: CiteMode::Normal },
            Request { items: &page, mode: CiteMode::Normal },
            Request { items: &group, mode: CiteMode::Normal },
            Request { items: &doe, mode: CiteMode::Prose },
            Request { items: &doe, mode: CiteMode::Year },
            Request { items: &lee, mode: CiteMode::Author },
            Request { items: &doe, mode: CiteMode::Normal },
        ];
        let output = render(&library, "en-US", &requests);
        assert_eq!(
            output
                .citations
                .iter()
                .map(|inlines| plain_text(inlines))
                .collect::<Vec<_>>(),
            [
                "[1]", "[2, p. 12]", "[1], [3]", "J. Doe and J. Smith [1]",
                "2020", "A. Lee, B. Kim, and C. Chen", "[1]",
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
        for (entry, prefix) in
            bibliography.entries.iter().zip(["[1]", "[2]", "[3]"])
        {
            assert_eq!(plain_text(entry.prefix.as_ref().unwrap()), prefix);
        }
        assert_eq!(
            bibliography.entries[1].inlines,
            vec![
                Inline::text("D. E. Knuth, "),
                Inline::Emph { inlines: vec![Inline::text("The TeXbook")] },
                Inline::text(". Reading, MA: Addison-Wesley, 1984."),
            ]
        );
        assert!(bibliography.entries[0].inlines.iter().any(|inline| matches!(inline,
            Inline::Link { url, .. } if url == "https://doi.org/10.1000/xyz123"
        )));
        assert_eq!(library.entries, before);
    }

    #[test]
    fn hidden_items_remain_in_the_bibliography_without_generating_a_note() {
        let library = sample("ieee");
        let hidden = [item("knuth1984", None)];
        let visible = [item("doe2020", None)];
        let output = render(
            &library,
            "en-US",
            &[
                Request { items: &hidden, mode: CiteMode::None },
                Request { items: &visible, mode: CiteMode::Normal },
            ],
        );
        assert_eq!(output.citations.len(), 2);
        assert!(output.citations[0].is_empty());
        assert_eq!(plain_text(&output.citations[1]), "[2]");
        let bibliography = output.bibliography.unwrap();
        assert_eq!(
            bibliography
                .entries
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>(),
            ["knuth1984", "doe2020"]
        );
    }

    #[test]
    fn no_requests_have_no_bibliography_and_missing_direct_keys_stay_empty() {
        let empty = Library::default();
        let output = render(&empty, "en-US", &[]);
        assert!(output.citations.is_empty());
        assert!(output.bibliography.is_none());
        let missing = [item("not-loaded", None)];
        let output = render(
            &empty,
            "en-US",
            &[
                Request { items: &missing, mode: CiteMode::Normal },
                Request { items: &[], mode: CiteMode::None },
            ],
        );
        assert_eq!(output.citations, vec![Inlines::new(), Inlines::new()]);
        let bibliography = output.bibliography.unwrap();
        assert!(bibliography.entries.is_empty());
        assert!(!bibliography.hanging_indent);
        let library = sample("ieee");
        let mixed = [item("not-loaded", None), item("knuth1984", None)];
        let output = render(
            &library,
            "en-US",
            &[Request { items: &mixed, mode: CiteMode::Normal }],
        );
        assert_eq!(plain_text(&output.citations[0]), "[1]");
        assert_eq!(output.bibliography.unwrap().entries[0].key, "knuth1984");
    }

    #[test]
    fn apa_sorts_entries_without_reordering_requests_and_full_keeps_formatting()
    {
        let library = sample("apa");
        let knuth = [item("knuth1984", None)];
        let group = [item("lee2019", None), item("doe2020", None)];
        let output = render(
            &library,
            "en-US",
            &[
                Request { items: &knuth, mode: CiteMode::Normal },
                Request { items: &group, mode: CiteMode::Normal },
                Request { items: &knuth, mode: CiteMode::Full },
            ],
        );
        assert_eq!(plain_text(&output.citations[0]), "(Knuth, 1984)");
        assert_eq!(
            plain_text(&output.citations[1]),
            "(Doe & Smith, 2020; Lee et al., 2019)"
        );
        assert_eq!(
            output.citations[2],
            vec![
                Inline::text("Knuth, D. E. (1984). "),
                Inline::Emph { inlines: vec![Inline::text("The TeXbook")] },
                Inline::text(". Addison-Wesley."),
            ]
        );
        let bibliography = output.bibliography.unwrap();
        assert_eq!(bibliography.style, "apa");
        assert!(bibliography.hanging_indent);
        assert_eq!(
            bibliography
                .entries
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>(),
            ["doe2020", "knuth1984", "lee2019"]
        );
        assert!(
            bibliography.entries.iter().all(|entry| entry.prefix.is_none())
        );
    }

    #[test]
    fn locators_are_literal_custom_strings_and_item_notes_belong_to_the_resolver()
     {
        let library = sample("chicago-author-date");
        let mut located = item("knuth1984", Some("p. 12"));
        located.prefix = Some(vec![Inline::text("DO-NOT-PREFIX")]);
        located.suffix = Some(vec![Inline::text("DO-NOT-SUFFIX")]);
        let empty = [item("knuth1984", Some(""))];
        let output = render(
            &library,
            "en-US",
            &[
                Request {
                    items: std::slice::from_ref(&located),
                    mode: CiteMode::Normal,
                },
                Request { items: &empty, mode: CiteMode::Normal },
            ],
        );
        // R5 contrasts Custom p. 12 with Locator::Page, which drops "p.".
        assert_eq!(plain_text(&output.citations[0]), "(Knuth 1984, p. 12)");
        assert_eq!(plain_text(&output.citations[1]), "(Knuth 1984)");
        assert_eq!(located.prefix, Some(vec![Inline::text("DO-NOT-PREFIX")]));
        assert_eq!(located.suffix, Some(vec![Inline::text("DO-NOT-SUFFIX")]));
    }

    #[test]
    fn note_style_returns_literal_citation_inlines_without_inventing_ir_notes()
    {
        let library = sample("chicago-notes");
        let doe = [item("doe2020", None)];
        let output = render(
            &library,
            "en-US",
            &[Request { items: &doe, mode: CiteMode::Normal }],
        );
        assert_eq!(
            plain_text(&output.citations[0]),
            concat!(
                "Jane Doe and John Smith, “A Study of Things,” Journal of Stuff ",
                "12, no. 3 (2020): 45–67, https://doi.org/10.1000/xyz123.",
            )
        );
        assert!(output.citations[0].iter().all(|inline| !matches!(
            inline,
            Inline::Footnote { .. } | Inline::FootnoteRef { .. }
        )));
        assert!(output.bibliography.unwrap().hanging_indent);
    }

    #[test]
    fn locale_reaches_one_shared_style_and_no_bibliography_still_returns_some()
    {
        let mut library = sample("ieee");
        // A private, hand-written CSL fixture isolates locale forwarding and
        // the supported absence of a bibliography section. It is not an
        // archived style or a replacement for the public loader fixtures.
        library.style = IndependentStyle::from_xml(r#"
<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text">
  <info><title>Writer locale test</title><id>https://example.org/writer-locale</id>
    <updated>2026-10-29T00:00:00Z</updated></info>
  <locale xml:lang="en-US"><terms><term name="and">EN-MARK</term></terms></locale>
  <locale xml:lang="de-DE"><terms><term name="and">DE-MARK</term></terms></locale>
  <citation><layout prefix="[" suffix="]"><group delimiter=": ">
    <text term="and"/><text variable="title"/>
  </group></layout></citation>
</style>
"#).unwrap();
        let knuth = [item("knuth1984", None)];
        for (locale, expected) in [
            ("en-US", "[EN-MARK: The TeXbook]"),
            ("de-DE", "[DE-MARK: The TeXbook]"),
        ] {
            let output = render(
                &library,
                locale,
                &[Request { items: &knuth, mode: CiteMode::Normal }],
            );
            assert_eq!(plain_text(&output.citations[0]), expected);
            let bibliography = output.bibliography.unwrap();
            assert_eq!(bibliography.style, library.style_spec());
            assert!(bibliography.entries.is_empty());
            assert!(!bibliography.hanging_indent);
        }
    }
}
