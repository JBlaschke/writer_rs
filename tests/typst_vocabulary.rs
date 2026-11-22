//! Vocabulary styles in the Typst PDF (design.md §8.7): a design's styles
//! named after Word's styles set the same elements in the package, a
//! template written after `show: base.writer-doc.with(..args)` overrides them
//! key by key, and `title-block` and `design-tokens` expose the design.

use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use typst::foundations::{Datetime, Label, Selector, Value};
use typst::introspection::{Introspector, MetadataElem};
use typst::layout::{Frame, FrameItem};
use typst::utils::PicoStr;
use typst::visualize::{Color, Paint};
use typst_layout::PagedDocument;
use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::design::{Design, Layer, Origin, merge};
use writer::pipeline::{self, BuildOptions, WorldSlot};
use writer::typst::compile::compile_document;
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{MainFile, WriterWorld};

type Probes = BTreeMap<String, serde_json::Value>;

/// Reports the styles in effect where it stands, as one JSON object.
const PROBE: &str = r##"
#let probe-space(value) = {
    if type(value) == length { value.to-absolute().pt() } else { repr(value) }
}
#let probe(tag) = context [#metadata(json.encode((
    tag: tag, size: text.size.pt(), weight: text.weight,
    fill: text.fill.to-hex(), style: text.style, font: text.font,
    justify: par.justify, align: repr(align.alignment),
    above: probe-space(block.above), below: probe-space(block.below),
    sticky: block.sticky, y: here().position().y.pt(),
))) <probe>]
#show heading: it => { it; probe("heading:" + repr(it.body)) }
#show outline.entry: it => { it; probe("entry:" + str(it.level)) }
"##;

const TITLE_ARGS: &str = concat!(
    "title: [#probe(\"title\")The title], ",
    "subtitle: [#probe(\"subtitle\")Sub], authors: (\"Ada\",), ",
    "date: [#probe(\"date\")2026]",
);

const BODY: &str = r##"
= First
#probe("body")Body
== Second
=== Third
#heading(level: 1, numbering: none, outlined: false)[Unlisted]
#figure(
    table(columns: 2, [A #probe("cell")], [B]),
    caption: [Table #probe("table-caption")],
)
#figure(
    rect(width: 2cm, height: 1cm),
    caption: [Figure #probe("figure-caption")],
)
#let inset-side(inset, side) = {
    let value = if type(inset) == dictionary { inset.at(side) } else { inset }
    value.length.to-absolute().pt()
}
#context [#metadata(json.encode((
    tag: "table", top: inset-side(table.inset, "top"),
    bottom: inset-side(table.inset, "bottom"),
    left: inset-side(table.inset, "left"),
))) <probe>]
"##;

const AFTER: &str = concat!(
    "#let template(..args, body) = {\n",
    "    show: writer-doc.with(..args)\n",
    "    show heading.where(level: 1): set text(fill: red)\n",
    "    body\n}\n",
    "#show: template.with(toc: 3)\n= First\n",
    "#heading(level: 1, numbering: none, outlined: false)[Unlisted]\n",
);

const BEFORE: &str = concat!(
    "#let template(..args, body) = {\n",
    "    show heading.where(level: 1): set text(\n",
    "        fill: red, size: 30pt, style: \"oblique\"\n    )\n",
    "    writer-doc(..args, body)\n}\n",
    "#show: template\n= First\n== Second\n",
);

const COVER: &str = concat!(
    "#let cover(meta, theme) = [#metadata(json.encode((\n",
    "    tag: \"cover\", title: repr(meta.title),\n",
    "    authors: meta.authors.len(),\n",
    "    size: theme.styles.at(\"Heading 1\").size.pt(),\n",
    "    accent: theme.colors.accent.to-hex(),\n",
    "    top: theme.page.margin.top.pt(),\n",
    "))) <probe>]\n",
    "#show: writer-doc.with(title: [The title], authors: (\"Ada\",),\n",
    "    title-block: cover)\nBody\n",
);

const TOKENS: &str = concat!(
    "#let theme = design-tokens()\n",
    "#let h1 = style-tokens(theme, \"heading1\")\n",
    "#let h3 = style-tokens(theme, \"Heading 3\")\n",
    "#let nope = style-tokens(theme, \"Nope\")\n",
    "#metadata(json.encode((\n",
    "    tag: \"tokens\", declared: theme.declared, size: h1.size.pt(),\n",
    "    weight: h1.weight, color: h1.color.to-hex(),\n",
    "    before: h1.space_before.pt(), keep: h1.keep_with_next,\n",
    "    text: repr(h1.text), h3font: h3.font, h3weight: h3.weight,\n",
    "    missing: nope.size == none, plain: nope.text.len(),\n",
    "    body: theme.text.size.pt(), accent: theme.colors.accent.to-hex(),\n",
    "    top: theme.page.margin.top.pt(),\n",
    "    right: theme.page.margin.right == none,\n",
    "    undeclared: design-tokens(design: none).declared,\n",
    "))) <probe>\n",
);

fn main_document() -> String {
    format!("#show: writer-doc.with({TITLE_ARGS}, toc: 3)\n{BODY}")
}

fn titled_document() -> String {
    format!("#show: writer-doc.with({TITLE_ARGS})\n#probe(\"body\")Body\n")
}

fn design(table: serde_json::Value, root: &Path) -> Design {
    merge(&[Layer {
        table: table.as_object().unwrap().clone(),
        base_dir: root,
        origin: Origin::FrontMatter,
    }])
    .unwrap()
}

/// The design of the acceptance criteria, each entry of `extra` replacing the
/// style of the same name.
fn styled(root: &Path, extra: &[(&str, serde_json::Value)]) -> Design {
    let mut table = serde_json::json!({
        "text": { "size": "11pt" },
        "colors": { "accent": "#a96d4b" },
        "page": { "margin": { "top": "30mm" } },
        "styles": {
            "Heading 1": {
                "size": "14pt", "weight": 700, "color": "#0a2f41",
                "align": "left", "space_before": "20pt",
                "space_after": "6pt", "keep_with_next": true,
            },
            "Heading 1 Unlisted": { "size": "18pt", "color": "#a96d4b" },
            "TOC Heading": { "size": "16pt", "weight": 400 },
            "Heading-3": { "font": "DejaVu Sans Mono Bold", "size": "10pt" },
            "toc 1": { "weight": 700, "space_before": "6pt" },
            "Caption": { "size": "9pt", "italic": true },
            "Table Caption": { "weight": 700 },
            "Compact": {
                "size": "9pt", "align": "left", "space_before": "2pt",
                "space_after": "3pt",
            },
            "Title": {
                "size": "30pt", "weight": 400, "align": "left",
                "space_before": "40pt",
            },
            "Date": { "size": "9pt" },
        },
    });
    for (name, style) in extra {
        table["styles"][*name] = style.clone();
    }
    design(table, root)
}

/// A design that declares no vocabulary name.
fn plain(root: &Path, extra: &[(&str, serde_json::Value)]) -> Design {
    let mut table = serde_json::json!({
        "text": { "size": "11pt" },
        "page": { "margin": { "top": "30mm" } },
        "styles": { "Recipient": { "size": "9pt" } },
    });
    for (name, style) in extra {
        table["styles"][*name] = style.clone();
    }
    design(table, root)
}

fn try_compile(
    root: &Path, design: &Design, body: &str,
) -> Result<PagedDocument, String> {
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    let mut world = WriterWorld::from_config_design(
        &config,
        design,
        &AssetRegistry::new(),
        root,
        MainFile::Virtual,
        Datetime::from_ymd(2026, 11, 20).unwrap(),
    )
    .unwrap();
    let source = format!(
        "#import \"@writer/doc:{}\": *\n{PROBE}\n{body}",
        writer::typst::WRITER_DOC_VERSION,
    );
    world.set_main_text(&source, SourceMap::default());
    compile_document(&mut world)
        .into_result()
        .map(|(document, _)| document)
        .map_err(|error| error.to_string())
}

fn compile(root: &Path, design: &Design, body: &str) -> PagedDocument {
    try_compile(root, design, body).unwrap()
}

/// Every probe of `document` by its tag.
fn probes(document: &PagedDocument) -> Probes {
    let label = Label::new(PicoStr::intern("probe")).unwrap();
    document
        .introspector()
        .query(&Selector::Label(label))
        .iter()
        .map(|content| {
            let metadata = content.to_packed::<MetadataElem>().unwrap();
            let Value::Str(text) = &metadata.value else {
                panic!("expected a JSON string, got {:?}", metadata.value)
            };
            let value: serde_json::Value =
                serde_json::from_str(text.as_str()).unwrap();
            (value["tag"].as_str().unwrap().to_owned(), value)
        })
        .collect()
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: measured {actual}, expected {expected}"
    );
}

fn number(probes: &Probes, tag: &str, key: &str) -> f64 {
    probes
        .get(tag)
        .unwrap_or_else(|| panic!("no probe {tag}: {:?}", probes.keys()))[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{tag}.{key} is no number"))
}

fn string<'a>(probes: &'a Probes, tag: &str, key: &str) -> &'a str {
    probes
        .get(tag)
        .unwrap_or_else(|| panic!("no probe {tag}: {:?}", probes.keys()))[key]
        .as_str()
        .unwrap_or_else(|| panic!("{tag}.{key} is no string"))
}

fn flag(probes: &Probes, tag: &str, key: &str) -> bool {
    probes[tag][key].as_bool().unwrap()
}

/// Checks the numeric and string keys of one probe.
fn expect(
    probes: &Probes, tag: &str, numbers: &[(&str, f64)],
    strings: &[(&str, &str)],
) {
    for (key, value) in numbers {
        assert_close(number(probes, tag, key), *value, &format!("{tag}.{key}"));
    }
    for (key, value) in strings {
        assert_eq!(string(probes, tag, key), *value, "{tag}.{key}");
    }
}

#[test]
fn vocabulary_styles_set_writers_elements() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let design = styled(root, &[]);
    let p = probes(&compile(root, &design, &main_document()));
    expect(
        &p,
        "heading:[First]",
        &[("size", 14.0), ("above", 20.0), ("below", 6.0)],
        &[("weight", "bold"), ("fill", "#0a2f41"), ("align", "left + top")],
    );
    assert!(!p["heading:[First]"]["justify"].as_bool().unwrap());
    assert!(flag(&p, "heading:[First]", "sticky"));
    expect(
        &p,
        "heading:[Second]",
        &[("size", 13.2), ("above", 15.84), ("below", 8.25)],
        &[("weight", "bold"), ("fill", "#000000")],
    );
    assert!(flag(&p, "heading:[Second]", "justify"));
    expect(
        &p,
        "heading:[Third]",
        &[("size", 10.0), ("above", 14.4), ("below", 7.5)],
        &[
            ("weight", "bold"),
            ("fill", "#000000"),
            ("font", "dejavu sans mono"),
        ],
    );
    assert!(flag(&p, "heading:[Third]", "justify"));
    expect(
        &p,
        "heading:[Unlisted]",
        &[("size", 18.0), ("above", 20.0), ("below", 6.0)],
        &[("weight", "bold"), ("fill", "#a96d4b")],
    );
    assert!(!flag(&p, "heading:[Unlisted]", "justify"));
    expect(
        &p,
        "heading:[Contents]",
        &[("size", 16.0), ("above", 20.0), ("below", 6.0)],
        &[("weight", "regular"), ("fill", "#0a2f41")],
    );
    assert!(!flag(&p, "heading:[Contents]", "justify"));
    expect(
        &p,
        "entry:1",
        &[("size", 11.0), ("above", 6.0)],
        &[("weight", "bold")],
    );
    expect(
        &p,
        "entry:2",
        &[("size", 11.0), ("above", 0.0)],
        &[("weight", "regular")],
    );
    expect(
        &p,
        "table-caption",
        &[("size", 9.0)],
        &[("weight", "bold"), ("style", "italic")],
    );
    expect(
        &p,
        "figure-caption",
        &[("size", 9.0)],
        &[("weight", "regular"), ("style", "italic")],
    );
    expect(
        &p,
        "cell",
        &[("size", 9.0)],
        &[("weight", "regular"), ("align", "left + top")],
    );
    assert!(!flag(&p, "cell", "justify"));
    expect(
        &p,
        "body",
        &[("size", 11.0)],
        &[("weight", "regular"), ("fill", "#000000")],
    );
    assert!(flag(&p, "body", "justify"));
    expect(&p, "table", &[("top", 2.0), ("bottom", 3.0), ("left", 5.0)], &[]);
}

#[test]
fn title_parts_take_their_styles() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let design = styled(root, &[]);
    let p = probes(&compile(root, &design, &main_document()));
    expect(
        &p,
        "title",
        &[("size", 30.0)],
        &[("weight", "regular"), ("align", "left + top")],
    );
    expect(&p, "subtitle", &[("size", 11.0)], &[("align", "left + top")]);
    expect(&p, "date", &[("size", 9.0)], &[]);
}

/// The `y` of the probes `tags` of the `document` over `design`.
fn positions(
    root: &Path, design: &Design, document: &str, tags: &[&str],
) -> Vec<f64> {
    let p = probes(&compile(root, design, document));
    tags.iter().map(|tag| number(&p, tag, "y")).collect()
}

#[test]
fn first_title_part_keeps_its_space_before() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let tags = ["title", "subtitle", "date", "body"];
    let near = serde_json::json!({
        "size": "30pt", "weight": 400, "align": "left",
        "space_before": "10pt",
    });
    let far = positions(root, &styled(root, &[]), &titled_document(), &tags);
    let close = positions(
        root,
        &styled(root, &[("Title", near)]),
        &titled_document(),
        &tags,
    );
    assert_close(far[0], 125.039, "title y with 40pt");
    assert_close(close[0], 95.039, "title y with 10pt");
    for (tag, (a, b)) in tags.iter().zip(far.iter().zip(&close)) {
        assert_close(a - b, 30.0, &format!("{tag} moves by"));
    }
}

#[test]
fn last_title_part_space_after_replaces_the_closing_gap() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let tags = ["date", "body"];
    let with_after = |after: &str| {
        let date = serde_json::json!({
            "size": "9pt", "space_after": after,
        });
        positions(
            root,
            &styled(root, &[("Date", date)]),
            &titled_document(),
            &tags,
        )
    };
    let short = with_after("10pt");
    let long = with_after("40pt");
    assert_close(short[1], 232.977, "body y with 10pt");
    assert_close(long[1], 262.977, "body y with 40pt");
    assert_close(long[1] - short[1], 30.0, "body difference");
    assert_close(short[0], 211.205, "date y with 10pt");
    assert_close(long[0], 211.205, "date y with 40pt");
    let default =
        positions(root, &styled(root, &[]), &titled_document(), &tags);
    assert_close(default[1], 245.527, "body y without space_after");
}

#[test]
fn designs_without_vocabulary_names_change_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    // A declared design lays its lines out in Word's line boxes (design.md
    // §6.2): no leading comes above a contents entry, and a cell's inset is
    // Typst's 5pt less what the box adds above the capitals and below the
    // baseline. A document without a design keeps Typst's own layout.
    for (design, entry, inset) in [
        (plain(root, &[]), 0.0, [2.404, 0.446]),
        (Design::default(), 7.15, [5.0, 5.0]),
    ] {
        let p = probes(&compile(root, &design, &main_document()));
        for tag in
            ["heading:[First]", "heading:[Unlisted]", "heading:[Contents]"]
        {
            expect(
                &p,
                tag,
                &[("size", 15.4), ("above", 19.8), ("below", 8.25)],
                &[("weight", "bold"), ("fill", "#000000")],
            );
        }
        expect(
            &p,
            "heading:[Second]",
            &[("size", 13.2), ("above", 15.84), ("below", 8.25)],
            &[("weight", "bold"), ("fill", "#000000")],
        );
        expect(
            &p,
            "heading:[Third]",
            &[("size", 11.0), ("above", 15.84), ("below", 8.25)],
            &[("weight", "bold"), ("fill", "#000000")],
        );
        for tag in ["entry:1", "entry:2", "entry:3"] {
            if p.contains_key(tag) {
                expect(&p, tag, &[("above", entry)], &[("weight", "regular")]);
            }
        }
        assert!(p.contains_key("entry:1") && p.contains_key("entry:2"));
        for tag in ["cell", "table-caption", "figure-caption"] {
            expect(
                &p,
                tag,
                &[("size", 11.0)],
                &[("weight", "regular"), ("style", "normal")],
            );
        }
        expect(
            &p,
            "table",
            &[("top", inset[0]), ("bottom", inset[1]), ("left", 5.0)],
            &[],
        );
        expect(
            &p,
            "title",
            &[("size", 17.0)],
            &[("weight", "bold"), ("align", "center + top")],
        );
    }
}

#[test]
fn a_title_style_without_keys_keeps_the_plain_title_block() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let tags = ["title", "subtitle", "date", "body"];
    let keep = serde_json::json!({ "keep_together": true });
    let base = probes(&compile(root, &plain(root, &[]), &titled_document()));
    let with_date = probes(&compile(
        root,
        &plain(root, &[("Date", keep)]),
        &titled_document(),
    ));
    let expected = [85.039, 113.325, 154.201, 191.139];
    for (tag, y) in tags.iter().zip(expected) {
        assert_close(number(&base, tag, "y"), y, &format!("{tag} y"));
        assert_close(
            number(&with_date, tag, "y"),
            number(&base, tag, "y"),
            &format!("{tag} y with Date"),
        );
        assert_close(
            number(&with_date, tag, "size"),
            number(&base, tag, "size"),
            &format!("{tag} size with Date"),
        );
        assert_eq!(
            string(&with_date, tag, "weight"),
            string(&base, tag, "weight"),
            "{tag} weight with Date"
        );
    }
}

#[test]
fn template_rules_after_writer_doc_override_the_design() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let p = probes(&compile(root, &styled(root, &[]), AFTER));
    expect(
        &p,
        "heading:[First]",
        &[("size", 14.0), ("above", 20.0)],
        &[("weight", "bold"), ("fill", "#ff4136")],
    );
    expect(
        &p,
        "heading:[Unlisted]",
        &[("size", 18.0), ("above", 20.0)],
        &[("weight", "bold"), ("fill", "#ff4136")],
    );
    expect(&p, "heading:[Contents]", &[], &[("fill", "#0a2f41")]);
}

#[test]
fn template_rules_before_writer_doc_lose_to_declared_keys() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let p = probes(&compile(root, &styled(root, &[]), BEFORE));
    expect(
        &p,
        "heading:[First]",
        &[("size", 14.0)],
        &[("fill", "#0a2f41"), ("style", "oblique")],
    );
    expect(
        &p,
        "heading:[Second]",
        &[("size", 13.2)],
        &[("fill", "#000000"), ("style", "normal")],
    );
}

#[test]
fn title_block_argument_replaces_or_removes_the_title_block() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let design = styled(root, &[]);
    let none = format!(
        "#show: writer-doc.with({TITLE_ARGS}, title-block: none)\n\
         #probe(\"body\")Body\n"
    );
    let p = probes(&compile(root, &design, &none));
    for tag in ["title", "subtitle", "date"] {
        assert!(!p.contains_key(tag), "{tag} is drawn: {:?}", p.keys());
    }
    assert!(p.contains_key("body"));
    let p = probes(&compile(root, &design, COVER));
    assert_eq!(string(&p, "cover", "title"), "[The title]");
    assert_close(number(&p, "cover", "authors"), 1.0, "authors");
    assert_close(number(&p, "cover", "size"), 14.0, "size");
    assert_eq!(string(&p, "cover", "accent"), "#a96d4b");
    assert_close(number(&p, "cover", "top"), 85.039, "top");
    let error = try_compile(
        root,
        &design,
        "#show: writer-doc.with(title: [T], title-block: 3)\nBody\n",
    )
    .expect_err("a number is no title-block");
    assert!(
        error.contains(
            "writer-doc: title-block must be auto, none or a function"
        ),
        "{error}"
    );
}

#[test]
fn design_tokens_are_typst_values() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let p = probes(&compile(root, &styled(root, &[]), TOKENS));
    assert!(flag(&p, "tokens", "declared"));
    assert_close(number(&p, "tokens", "size"), 14.0, "size");
    assert_close(number(&p, "tokens", "weight"), 700.0, "weight");
    assert_eq!(string(&p, "tokens", "color"), "#0a2f41");
    assert_close(number(&p, "tokens", "before"), 20.0, "before");
    assert!(flag(&p, "tokens", "keep"));
    assert_eq!(
        string(&p, "tokens", "text"),
        "(weight: 700, size: 14pt, fill: rgb(\"#0a2f41\"))"
    );
    assert_eq!(string(&p, "tokens", "h3font"), "DejaVu Sans Mono");
    assert_close(number(&p, "tokens", "h3weight"), 700.0, "h3weight");
    assert!(flag(&p, "tokens", "missing"));
    assert_close(number(&p, "tokens", "plain"), 0.0, "plain");
    assert_close(number(&p, "tokens", "body"), 11.0, "body");
    assert_eq!(string(&p, "tokens", "accent"), "#a96d4b");
    assert_close(number(&p, "tokens", "top"), 85.039, "top");
    assert!(flag(&p, "tokens", "right"));
    assert!(!flag(&p, "tokens", "undeclared"));
}

/// Size and fill of the first text run containing `marker`.
fn run_style(frame: &Frame, marker: &str) -> Option<(f64, Paint)> {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                if let Some(found) = run_style(&group.frame, marker) {
                    return Some(found);
                }
            }
            FrameItem::Text(run) if run.text.contains(marker) => {
                return Some((run.size.to_pt(), run.fill.clone()));
            }
            _ => {}
        }
    }
    None
}

#[test]
fn starter_template_overrides_the_design() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::write(
        root.join("writer.toml"),
        concat!(
            "[pdf]\nsystem_fonts = false\ntemplate = \"template.typ\"\n",
            "[design.styles.\"Heading 1\"]\n",
            "size = \"14pt\"\ncolor = \"#0a2f41\"\n",
        ),
    )
    .unwrap();
    std::fs::write(
        root.join("template.typ"),
        writer::vendor::get("typst/writer-doc/template.typ").unwrap(),
    )
    .unwrap();
    let input = root.join("document.md");
    std::fs::write(&input, "# First\n").unwrap();
    let opts = BuildOptions::new(input);
    let prepared = pipeline::prepare(&opts).unwrap();
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    pipeline::pdf::load_typst_main(&prepared, &opts, world).unwrap();
    let (document, _) = compile_document(world).into_result().unwrap();
    let (size, fill) = run_style(&document.pages()[0].frame, "First")
        .expect("the heading's text run");
    assert_close(size, 14.0, "heading size");
    assert_eq!(fill, Paint::Solid(Color::from_str("#1a3a8a").unwrap()));
}

/// The structured frontend leaves `title-block` to the compiler: a function
/// is no literal and `none` is no metadata.
#[test]
fn native_title_block_stays_with_the_compiler() {
    let cases = [
        ("none", None),
        ("(meta, theme) => [Cover: #meta.title]", Some("Cover: The title")),
    ];
    for (argument, cover) in cases {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(
            root.join("writer.toml"),
            "[pdf]\nsystem_fonts = false\n",
        )
        .unwrap();
        let input = root.join("native.typ");
        std::fs::write(
            &input,
            format!(
                concat!(
                    "#import \"@writer/doc:{}\": writer-doc\n",
                    "#show: writer-doc.with(title: [The title],\n",
                    "  title-block: {})\n",
                    "Native body.\n",
                ),
                writer::typst::WRITER_DOC_VERSION,
                argument,
            ),
        )
        .unwrap();
        let opts = BuildOptions::new(input);
        let prepared = pipeline::prepare(&opts).unwrap();
        assert!(
            !prepared.diagnostics.has_errors(),
            "{argument}: {:?}",
            prepared.diagnostics
        );
        let meta = &prepared.document.meta;
        assert_eq!(meta.title.as_deref(), Some("The title"), "{argument}");
        assert!(!meta.vars.contains_key("title-block"), "{argument}");
        let mut worlds = WorldSlot::new();
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        let (document, _) = compile_document(world).into_result().unwrap();
        let frame = &document.pages()[0].frame;
        assert!(run_style(frame, "Native body").is_some(), "{argument}");
        match cover {
            Some(text) => {
                assert!(run_style(frame, text).is_some(), "{argument}");
            }
            None => {
                assert!(run_style(frame, "The title").is_none(), "{argument}");
            }
        }
    }
}
