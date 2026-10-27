//! Compiler-backed contracts for the offline `@writer/doc` package.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use typst::foundations::{Datetime, Label, Smart, Value};
use typst::introspection::{Introspector, MetadataElem};
use typst::layout::{Frame, FrameItem};
use typst::model::Document;
use typst::text::TextItem;
use typst::utils::PicoStr;
use typst::visualize::{Color, Paint};
use typst_layout::PagedDocument;
use writer::config::Config;
use writer::resolve::l10n::{LangTag, Term, term};
use writer::typst::compile::{OracleEntry, compile_document, oracle};
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WorldOptions, WriterWorld, world_inputs};

const IMPORT: &str = "#import \"@writer/doc:0.1.0\": *\n";
const BIB: &str = r#"@book{alpha,
  author = {Lovelace, Ada},
  title = {Analytical Engines},
  year = {1843},
  publisher = {Institute}
}"#;
const OTHER_BIB: &str = r#"@book{beta,
  author = {Hopper, Grace},
  title = {Compiler Design},
  year = {1952},
  publisher = {Laboratory}
}"#;
const CSL: &str = r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text">
  <info><title>Package test</title><id>https://example.org/style</id><updated>2026-10-24T00:00:00+00:00</updated></info>
  <citation><layout prefix="(" suffix=")"><text variable="title"/></layout></citation>
  <bibliography><layout><text variable="title"/></layout></bibliography>
</style>"#;

fn compile(source: &str, config: &Config, enabled: bool) -> PagedDocument {
    compile_with(source, config, enabled, false)
}

fn compile_failure(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let mut world = WriterWorld::new(WorldOptions {
        root: dir.path().into(),
        main: MainFile::Virtual,
        today: Datetime::from_ymd(2026, 10, 24).unwrap(),
        system_fonts: false,
        font_paths: Vec::new(),
        package_paths: Vec::new(),
        allow_packages: false,
        inputs: world_inputs(&Config::default()),
    })
    .unwrap();
    world.set_main_text(source, SourceMap::default());
    compile_document(&mut world).into_result().unwrap_err().to_string()
}

fn compile_with(
    source: &str, config: &Config, enabled: bool, standalone: bool,
) -> PagedDocument {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("refs.bib"), BIB).unwrap();
    fs::write(dir.path().join("other.bib"), OTHER_BIB).unwrap();
    fs::write(dir.path().join("custom.csl"), CSL).unwrap();
    fs::create_dir(dir.path().join("package")).unwrap();
    for path in writer::vendor::list("typst/writer-doc/") {
        let name = path.strip_prefix("typst/writer-doc/").unwrap();
        fs::write(
            dir.path().join("package").join(name),
            writer::vendor::get(&path).unwrap(),
        )
        .unwrap();
    }
    fs::write(
        dir.path().join("figure.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#537da5"/></svg>"##,
    )
    .unwrap();
    fs::write(
        dir.path().join("template.typ"),
        writer::vendor::get("typst/writer-doc/template.typ").unwrap(),
    )
    .unwrap();
    let mut inputs = world_inputs(config);
    if enabled {
        inputs.insert("writer-oracle".into(), "1".into());
    }
    if standalone {
        inputs.remove("writer-version");
        inputs.remove("writer-config");
    }
    let mut world = WriterWorld::new(WorldOptions {
        root: dir.path().into(),
        main: MainFile::Virtual,
        today: Datetime::from_ymd(2026, 10, 24).unwrap(),
        system_fonts: false,
        font_paths: Vec::new(),
        package_paths: Vec::new(),
        allow_packages: false,
        inputs,
    })
    .unwrap();
    world.set_main_text(source, SourceMap::default());
    let compiled = compile_document(&mut world);
    assert!(
        compiled.output.is_some(),
        "{:#?}\n\nSource:\n{source}",
        compiled.diagnostics
    );
    let (document, diagnostics) = compiled.into_result().unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    document
}

fn frame_text(frame: &Frame, text: &mut String) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => frame_text(&group.frame, text),
            FrameItem::Text(run) => text.push_str(&run.text),
            FrameItem::Image(..) => text.push('\u{fffc}'),
            _ => {}
        }
    }
}

fn visible(document: &PagedDocument) -> String {
    let mut text = String::new();
    for page in document.pages() {
        frame_text(&page.frame, &mut text);
    }
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn text_runs<'a>(frame: &'a Frame, runs: &mut Vec<&'a TextItem>) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => text_runs(&group.frame, runs),
            FrameItem::Text(run) => runs.push(run),
            _ => {}
        }
    }
}

fn filled_shapes(frame: &Frame) -> usize {
    frame
        .items()
        .map(|(_, item)| match item {
            FrameItem::Group(group) => filled_shapes(&group.frame),
            FrameItem::Shape(shape, _) if shape.fill.is_some() => 1,
            _ => 0,
        })
        .sum()
}

fn json_metadata(document: &PagedDocument, label: &str) -> serde_json::Value {
    let label = Label::new(PicoStr::intern(label)).unwrap();
    let content = document.introspector().query_label(label).unwrap();
    let metadata = content.to_packed::<MetadataElem>().unwrap();
    let Value::Str(value) = &metadata.value else {
        panic!("expected JSON string metadata")
    };
    serde_json::from_str(value).unwrap()
}

fn expect_number(
    document: &PagedDocument, label: &str, kind: &str, number: Option<&str>,
) {
    assert_eq!(
        oracle(document, label),
        Some(OracleEntry {
            kind: kind.into(),
            number: number.map(str::to_owned),
        }),
        "{label}",
    );
}

#[test]
fn rich_document_accepts_the_declared_argument_vocabulary() {
    let source = format!(
        r#"{IMPORT}
#show: writer-doc.with(
  title: [Offline *Publishing*], subtitle: "Rich package contract",
  authors: ("Ada", (name: "Grace", affiliation: "Institute", email: "g@example.org", id: "g")),
  date: datetime(year: 2026, month: 10, day: 24),
  abstract: [A short summary.], abstract-title: [Overview],
  keywords: ("offline", "publishing"), description: "Package integration",
  lang: "en-US", toc: 2, toc-title: [Navigation],
  numbering: (headings: "1.1", figures: "section", equations: "labelled"),
  bibliography: (path("refs.bib"), path("other.bib")),
  reference-section-title: [Literature], cite-style: "ieee", cite-locale: "de-AT",
  paper: "a4", markdown: (:), math: (:), references: (:), html: (:),
  pdf: (paper: "us-letter"), docx: (:), latex: (:), typst: (:), notebook: (:),
  theme: "custom", template: "custom", reference-doc: "custom",
  latex-template: "custom", custom-variable: "accepted",
)
= Introduction <sec-intro>
Body cites @alpha and @beta, with a footnote#footnote[Supporting detail.].

#callout[First callout.]
#callout(kind: "tip")[Second callout.]
#callout(kind: "important")[Third callout.]
#callout(kind: "warning")[Fourth callout.]
#callout(kind: "caution")[Fifth callout.]
#callout(kind: "aside", title: [Custom title])[Sixth callout.]

#figure(image("figure.svg"), caption: [Illustration]) <fig-image>
#figure(table(columns: 1, [Cell]), caption: [Data]) <tbl-data>
#figure(raw("let x = 1", lang: "rust", block: true), caption: [Program]) <lst-code>
$ x = 1 $ <eq-result>

#listing(numbered: true, start: 3, highlight: "1-2,4", plain: true)[
```rust
let x = 1;
let y = 2;
```
]
#code-cell(index: 2, count: 7, lang: "python", input: [```python
print(42)
```])[Cell output.][More output.]
"#
    );
    let document = compile(&source, &Config::default(), true);
    let text = visible(&document);
    for expected in [
        "OfflinePublishing", "Richpackagecontract", "Ada,Grace", "Institute",
        "Overview", "Navigation", "Note", "Tip", "Important", "Warning",
        "Caution", "Customtitle", "Celloutput.", "Moreoutput.", "Literature",
        "AnalyticalEngines", "CompilerDesign",
    ] {
        assert!(text.contains(expected), "missing {expected:?}: {text}");
    }
    assert_eq!(document.info().title.as_deref(), Some("Offline Publishing"));
    assert_eq!(document.info().author, ["Ada", "Grace"]);
    assert_eq!(
        document.info().description.as_deref(),
        Some("Package integration")
    );
    assert_eq!(document.info().keywords, ["offline", "publishing"]);
    assert_eq!(document.info().date, Smart::Auto);
    expect_number(&document, "fig-image", "figure", Some("1.1"));
    expect_number(&document, "tbl-data", "table", Some("1.1"));
    expect_number(&document, "lst-code", "listing", Some("1.1"));
}

#[test]
fn starter_template_delegates_to_the_package() {
    let source = r#"#import "template.typ": writer-doc
#show: writer-doc.with(title: "Template document", author: "Ada", toc: false)
= A blue heading
Delegated body.
"#;
    let document = compile(source, &Config::default(), false);
    assert!(visible(&document).contains("Templatedocument"));
    assert!(visible(&document).contains("Delegatedbody."));
    assert_eq!(document.info().author, ["Ada"]);
}

#[test]
fn automatic_values_follow_writer_config_and_explicit_values_win() {
    let config: Config = toml::from_str(
        "[document]\nlang = 'de-AT'\ntoc = 2\n\
         [numbering]\nheadings = true\nfigures = 'section'\nequations = 'all'\n\
         [pdf]\npaper = 'us-letter'\n[references]\nlocale = 'fr-CA'",
    )
    .unwrap();
    let source = format!(
        "{IMPORT}#show: writer-doc\n= Einleitung <sec-one>\n\
         #figure([Figure body]) <fig-one>\n$ x $\n$ y $ <eq-two>"
    );
    let document = compile(&source, &config, true);
    assert!(visible(&document).contains("Inhalt"));
    expect_number(&document, "sec-one", "heading", Some("1"));
    expect_number(&document, "fig-one", "figure", Some("1.1"));
    expect_number(&document, "eq-two", "equation", Some("(1.2)"));
    let size = document.pages()[0].frame.size();
    assert!((size.x.to_pt() - 612.0).abs() < 0.01);
    assert!((size.y.to_pt() - 792.0).abs() < 0.01);

    let source = format!(
        "{IMPORT}#show: writer-doc.with(lang: \"en\", toc: false, \
         numbering: false, paper: \"a4\")\n= Introduction <sec-one>\n\
         #figure([Figure body]) <fig-one>\n$ x $ <eq-one>"
    );
    let document = compile(&source, &config, true);
    assert!(!visible(&document).contains("Inhalt"));
    expect_number(&document, "sec-one", "heading", None);
    expect_number(&document, "fig-one", "figure", Some("1"));
    let size = document.pages()[0].frame.size();
    assert!((size.x.to_mm() - 210.0).abs() < 0.01);
    assert!((size.y.to_mm() - 297.0).abs() < 0.01);
}

#[test]
fn standalone_defaults_and_dates_are_stable() {
    for (date, expected) in [
        ("\"2024-02-29\"", Some(Datetime::from_ymd(2024, 2, 29).unwrap())),
        (
            "datetime(year: 2025, month: 3, day: 4)",
            Some(Datetime::from_ymd(2025, 3, 4).unwrap()),
        ),
        ("\"Not a calendar date\"", None),
        ("none", None),
    ] {
        let source = format!(
            "{IMPORT}#show: writer-doc.with(date: {date}, keywords: \"one\")\n\
             = Unnumbered <sec-one>\nDefault body."
        );
        let document = compile_with(&source, &Config::default(), true, true);
        let actual = document.info().date;
        if let Some(expected) = expected {
            assert_eq!(actual, Smart::Custom(Some(expected)), "{date}");
        } else {
            assert_eq!(actual, Smart::Auto, "{date}");
        }
        expect_number(&document, "sec-one", "heading", None);
        assert_eq!(document.info().keywords, ["one"]);
    }
}

#[test]
fn package_defaults_keep_writer_timestamp_and_use_pdf_table_paper() {
    let source = format!(
        "{IMPORT}#show: writer-doc.with(date: \"2000-01-02\", \
         pdf: (paper: \"us-letter\"))\nDate policy."
    );
    let document = compile(&source, &Config::default(), false);
    assert_eq!(document.info().date, Smart::Auto);
    assert!(visible(&document).contains("2000-01-02"));
    assert!((document.pages()[0].frame.size().x.to_pt() - 612.0).abs() < 0.01);
}

#[test]
fn dictionaries_match_every_shipped_writer_locale_exactly() {
    let source = r#"#import "package/callout.typ": writer-terms
#metadata(json.encode(writer-terms)) <terms>
"#;
    let document = compile(source, &Config::default(), false);
    let embedded = json_metadata(&document, "terms");
    let terms = embedded.as_object().unwrap();
    let files = writer::vendor::list("l10n/");
    let locales: BTreeSet<_> = files
        .iter()
        .map(|path| {
            path.strip_prefix("l10n/").unwrap().trim_end_matches(".toml")
        })
        .collect();
    assert_eq!(
        terms.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        locales
    );
    let kinds = BTreeMap::from([
        ("abstract", Term::Abstract),
        ("note", Term::Note),
        ("tip", Term::Tip),
        ("important", Term::Important),
        ("warning", Term::Warning),
        ("caution", Term::Caution),
    ]);
    for path in files {
        let locale =
            path.strip_prefix("l10n/").unwrap().trim_end_matches(".toml");
        let lang = LangTag::parse(locale).unwrap();
        let data: BTreeMap<String, String> =
            toml::from_str(&writer::vendor::get_str(&path).unwrap()).unwrap();
        let expected: BTreeMap<_, _> = data
            .keys()
            .map(|key| (key.clone(), term(&lang, kinds[key.as_str()])))
            .collect();
        assert_eq!(terms[locale], serde_json::to_value(expected).unwrap());
    }
}

#[test]
fn bibliography_dictionary_matches_resolver_terms() {
    let source = r#"#import "package/numbering.typ": bibliography-terms
#metadata(json.encode(bibliography-terms)) <terms>
"#;
    let document = compile(source, &Config::default(), false);
    let embedded = json_metadata(&document, "terms");
    let terms = embedded.as_object().unwrap();
    for required in ["en", "de", "fr", "zh"] {
        assert!(terms.contains_key(required), "missing locale {required}");
    }
    for (locale, word) in terms {
        assert_eq!(
            word["bibliography"].as_str().unwrap(),
            term(&LangTag::parse(locale).unwrap(), Term::Bibliography),
            "{locale}",
        );
    }
}

#[test]
fn custom_titles_apply_to_explicit_outline_and_bibliography_calls() {
    let source = format!(
        r#"{IMPORT}#show: writer-doc.with(
  lang: "en", toc: false, toc-title: "Map", abstract: [Summary body.],
  abstract-title: "Summary", reference-section-title: "Sources", cite-locale: "de",
)
#outline()
= Main heading
Cite @alpha.
#bibliography(path("refs.bib"))
"#
    );
    let document = compile(&source, &Config::default(), false);
    let text = visible(&document);
    for expected in ["Map", "Summary", "Sources", "AnalyticalEngines"] {
        assert!(text.contains(expected), "{text}");
    }
    for absent in ["Contents", "Abstract", "Bibliography", "Bibliografie"] {
        assert!(!text.contains(absent), "unexpected {absent}: {text}");
    }
}

#[test]
fn citation_locale_changes_entries_without_changing_document_title_terms() {
    for document_lang in ["en", "de-AT", "fr-CA"] {
        let title =
            term(&LangTag::parse(document_lang).unwrap(), Term::Bibliography);
        let source = format!(
            "{IMPORT}#show: writer-doc.with(lang: \"{document_lang}\", \
             cite-locale: \"fr-CA\", bibliography: path(\"refs.bib\"), \
             csl: \"apa\")\nBody cites @alpha."
        );
        let document = compile(&source, &Config::default(), false);
        let text = visible(&document);
        assert!(text.contains(&title), "{text}");
        let mut runs = Vec::new();
        for page in document.pages() {
            text_runs(&page.frame, &mut runs);
        }
        assert!(runs.iter().any(|run| {
            run.text.contains("Analytical")
                && run.lang.as_str() == "fr"
                && run.region.is_some_and(|region| region.as_str() == "CA")
        }));
    }
}

#[test]
fn bibliography_and_style_paths_keep_the_callers_project_root() {
    for argument in ["cite-style", "csl"] {
        let source = format!(
            "{IMPORT}#show: writer-doc.with(bibliography: path(\"refs.bib\"), \
             {argument}: path(\"custom.csl\"))\nCite @alpha."
        );
        let document = compile(&source, &Config::default(), false);
        let text = visible(&document);
        assert!(text.contains("Cite(AnalyticalEngines)."), "{text}");
        assert!(text.contains("BibliographyAnalyticalEngines"), "{text}");
    }
}

#[test]
fn invalid_package_arguments_report_the_required_call_shape() {
    for (arguments, expected) in [
        (
            "bibliography: \"refs.bib\"",
            "writer-doc: pass bibliography: path(\"refs.bib\")",
        ),
        ("123", "writer-doc: unexpected positional arguments"),
    ] {
        let source =
            format!("{IMPORT}#show: writer-doc.with({arguments})\nBody.");
        let error = compile_failure(&source);
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn writer_words_fall_back_and_unknown_callouts_get_a_title() {
    for language in ["de-AT", "zz-ZZ"] {
        let source = format!(
            "{IMPORT}#show: writer-doc.with(lang: \"{language}\", \
             abstract: [A summary.], toc: true)\n= Heading\n\
             #callout[Body.]\n#callout(kind: \"aside\")[Other.]"
        );
        let document = compile(&source, &Config::default(), false);
        let text = visible(&document);
        for expected in ["Abstract", "Note", "Aside"] {
            assert!(text.contains(expected), "{text}");
        }
    }
}

#[test]
fn section_numbering_handles_abstracts_interludes_and_references() {
    let source = format!(
        r#"{IMPORT}#show: writer-doc.with(
  numbering: (headings: "1.", figures: "section", equations: "labelled"),
  abstract: [#heading[Abstract heading] <sec-abstract>],
)
#figure([Before]) <fig-zero>
= One <sec-one>
#figure([Image]) <fig-a>
#figure(table(columns: 1, [Table])) <tbl-a>
#figure(raw("Code", block: true)) <lst-a>
#figure([Other]) <fig-other>
$ x = 1 $
$ y = 2 $ <eq-a>
== Sub <sec-sub>
=== Deep <sec-deep>
#heading(numbering: none)[Interlude] <sec-interlude>
#figure([Interlude body]) <fig-int>
== Under interlude <sec-under>
= Two <sec-two>
#figure([Last]) <fig-c>
$ z = 3 $ <eq-b>
Reference @eq-a; @eq-a[Eq.]; @eq-a[].
Footnote#footnote[Real note] <note-real>.
Generated#footnote[Generated note] <writer-fn-generated>.
"#
    );
    let document = compile(&source, &Config::default(), true);
    for (label, kind, number) in [
        ("sec-abstract", "heading", None),
        ("fig-zero", "figure", Some("0.1")),
        ("sec-one", "heading", Some("1.")),
        ("fig-a", "figure", Some("1.1")),
        ("tbl-a", "table", Some("1.1")),
        ("lst-a", "listing", Some("1.1")),
        ("fig-other", "figure", Some("1.2")),
        ("eq-a", "equation", Some("(1.1)")),
        ("sec-sub", "heading", Some("1.1.")),
        ("sec-deep", "heading", Some("1.1.1.")),
        ("sec-interlude", "heading", None),
        ("fig-int", "figure", Some("1.3")),
        ("sec-under", "heading", Some("1.2.")),
        ("sec-two", "heading", Some("2.")),
        ("fig-c", "figure", Some("2.1")),
        ("eq-b", "equation", Some("(2.1)")),
        ("note-real", "footnote", Some("1")),
    ] {
        expect_number(&document, label, kind, number);
    }
    assert!(oracle(&document, "writer-fn-generated").is_none());
    let text = visible(&document);
    assert!(text.contains("ReferenceEquation1.1;Eq.1.1;1.1."), "{text}");
    assert!(!text.contains("Equation(1.1)"), "{text}");
    let entries = document.introspector().query_labelled();
    let count = entries
        .iter()
        .filter(|content| {
            content.label().is_some_and(|label| {
                label.resolve().as_str().starts_with("writer-oracle-")
            })
        })
        .count();
    assert_eq!(count, 17);
}

#[test]
fn document_and_equation_policies_and_heading_fallback() {
    for (headings, figures, equations, second, figure) in [
        ("true", "document", "all", Some("(2)"), "2"),
        ("\"A.1\"", "section", "none", None, "1.1"),
        ("false", "section", "labelled", Some("(1)"), "2"),
    ] {
        let source = format!(
            "{IMPORT}#show: writer-doc.with(numbering: (headings: {headings}, \
             figures: \"{figures}\", equations: \"{equations}\"))\n\
             #figure([Before]) <fig-before>\n= One <sec-one>\n\
             #figure([After]) <fig-after>\n$ x $\n$ y $ <eq-y>\n\
             #math.equation(block: true, numbering: none, $ z $) <eq-off>"
        );
        let document = compile(&source, &Config::default(), true);
        expect_number(&document, "eq-y", "equation", second);
        expect_number(&document, "fig-after", "figure", Some(figure));
        expect_number(&document, "eq-off", "equation", None);
    }
}

#[test]
fn oracle_is_disabled_unless_requested() {
    let source = format!(
        "{IMPORT}#show: writer-doc.with(numbering: true)\n\
         = Heading <sec-one>\n#figure([Figure]) <fig-one>\n\
         $ x $ <eq-one>\nNote#footnote[Note] <note-one>."
    );
    let document = compile(&source, &Config::default(), false);
    for label in ["sec-one", "fig-one", "eq-one", "note-one"] {
        assert!(oracle(&document, label).is_none());
    }
    assert!(!document.introspector().query_labelled().iter().any(|content| {
        content.label().is_some_and(|label| {
            label.resolve().as_str().starts_with("writer-oracle-")
        })
    }));
}

#[test]
fn captionless_labels_are_visible_and_follow_kind_placement() {
    check_captionless("en");
    check_captionless("de-AT");
}

fn check_captionless(language: &str) {
    let language_tag = LangTag::parse(language).unwrap();
    for (figures, number) in [("document", "1"), ("section", "1.1")] {
        for (kind, body, marker, supplement, before) in [
            ("image", "image(\"figure.svg\")", "\u{fffc}", Term::Figure, false),
            (
                "table",
                "table(columns: 1, [TableBody])",
                "TableBody",
                Term::Table,
                true,
            ),
            (
                "raw",
                "raw(\"ListingBody\", block: true)",
                "ListingBody",
                Term::Listing,
                true,
            ),
        ] {
            let source = format!(
                "{IMPORT}#show: writer-doc.with(lang: \"{language}\", \
                 numbering: (headings: true, \
                 figures: \"{figures}\"))\n= Heading\n\
                 #figure({body}, kind: {kind}) <fig-one>"
            );
            let document = compile(&source, &Config::default(), true);
            let text = visible(&document);
            let caption =
                format!("{}{number}", term(&language_tag, supplement));
            let caption_position = text
                .find(&caption)
                .unwrap_or_else(|| panic!("missing {caption}: {text}"));
            let body_position = text.find(marker).unwrap();
            assert_eq!(caption_position < body_position, before, "{text}");
            assert_eq!(text.matches(&caption).count(), 1, "{text}");
        }
    }
}

#[test]
fn listing_highlights_are_atomic_and_plain_removes_syntax_colors() {
    let source = |highlight: &str, plain: bool| {
        format!(
            "{IMPORT}#show: writer-doc\n\
         #listing(numbered: true, start: 7, highlight: \"{highlight}\", \
         plain: {plain})[\n```python\n\
         value = \"\"\"opening\ninside_string\nclosing\"\"\"\n```\n]"
        )
    };
    let base = compile(&source("", false), &Config::default(), false);
    let shapes = |doc: &PagedDocument| {
        doc.pages().iter().map(|page| filled_shapes(&page.frame)).sum::<usize>()
    };
    let highlighted =
        compile(&source("7 - 8,10", false), &Config::default(), false);
    assert_eq!(shapes(&highlighted), shapes(&base) + 2);
    for highlight in ["7,x", "7,18446744073709551616", "18446744073709551615"] {
        let document =
            compile(&source(highlight, false), &Config::default(), false);
        assert_eq!(shapes(&document), shapes(&base), "{highlight}");
    }
    let full_range = compile(
        &source("7-18446744073709551615", false),
        &Config::default(),
        false,
    );
    assert_eq!(shapes(&full_range), shapes(&base) + 3);

    let plain = compile(&source("", true), &Config::default(), false);
    let interior_fill = |doc: &PagedDocument| {
        let mut runs = Vec::new();
        for page in doc.pages() {
            text_runs(&page.frame, &mut runs);
        }
        runs.into_iter()
            .find(|run| run.text.contains("inside_string"))
            .map(|run| run.fill.clone())
            .expect("interior string line")
    };
    assert_ne!(interior_fill(&base), Paint::Solid(Color::BLACK));
    assert_eq!(interior_fill(&plain), Paint::Solid(Color::BLACK));
}

#[test]
fn listings_keep_start_numbers_and_cells_emit_only_content() {
    let source = format!(
        r#"{IMPORT}#show: writer-doc
#listing(numbered: true, start: 7, highlight: "7-8,10", plain: true)[
```rust
first_line
second_line
```
]
#code-cell(index: 123, count: 456, lang: "python", input: [```python
cell_input
```])[cell_output][second_output]
#code-cell(index: 5, input: none)[hidden_input_output]
"#
    );
    let document = compile(&source, &Config::default(), false);
    let text = visible(&document);
    for expected in [
        "7first_line", "8second_line", "cell_input", "cell_output",
        "second_output", "hidden_input_output",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    for absent in ["123", "456", "In[", "Out["] {
        assert!(!text.contains(absent), "unexpected {absent}: {text}");
    }
}
