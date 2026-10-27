//! Typst source contracts on hand-built IR, including real package compilation.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use typst::foundations::Datetime;
use typst::layout::{Frame, FrameItem};
use typst::model::Document as TypstDocument;
use typst_layout::PagedDocument;
use writer::assets::registry::AssetRegistry;
use writer::config::{Config, HeadingNumbering};
use writer::error::{Level, Result};
use writer::ir::*;
use writer::typst::compile::{compile_document, oracle};
use writer::typst::emit::{
    AssetPaths, EmitOptions, Emitted, IncludeRun, emit, emit_split,
};
use writer::typst::sourcemap::SourceMap;
use writer::typst::world::{
    IncludeProvider, MainFile, Synthesized, WorldOptions, WriterWorld,
    world_inputs,
};

const BIB: &str = include_str!("fixtures/bib/refs.bib");
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#537da5"/></svg>"##;

fn document(blocks: Vec<Block>) -> Document {
    Document {
        meta: DocMeta::default(),
        blocks,
        features: Features::default(),
        source: SourceInfo {
            path: PathBuf::from("/docs/document.md"),
            kind: SourceKind::Markdown,
            base_dir: PathBuf::from("/docs"),
        },
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Resolved::default(),
    }
}

fn text(value: &str) -> Inlines {
    vec![Inline::text(value)]
}

fn paragraph(value: &str) -> Block {
    Block::paragraph(text(value))
}

fn block(value: Value) -> Block {
    serde_json::from_value(value).unwrap()
}

fn inline(value: Value) -> Inline {
    serde_json::from_value(value).unwrap()
}

fn options(config: &Config) -> EmitOptions<'_> {
    EmitOptions {
        config,
        assets: AssetPaths::Virtual,
        template: None,
        bibliography: vec!["refs.bib".into()],
        style: "ieee".into(),
        footnote_prefix: String::new(),
        fragment: false,
    }
}

fn render(document: &Document) -> Emitted {
    emit(document, &AssetRegistry::new(), &options(&Config::default())).unwrap()
}

fn fixture() -> (Document, AssetRegistry) {
    let mut assets = AssetRegistry::new();
    let id = assets.register_bytes(
        include_bytes!("fixtures/md/images/a.png").to_vec(),
        "image/png",
        "plot.png",
    );
    let json = include_str!("fixtures/ir/sample-document.json")
        .replace("32debefb8003", &id);
    (serde_json::from_str(&json).unwrap(), assets)
}

fn world(root: &Path, config: &Config) -> WriterWorld {
    let mut inputs = world_inputs(config);
    inputs.insert("writer-oracle".into(), "1".into());
    WriterWorld::new(WorldOptions {
        root: root.into(),
        main: MainFile::Virtual,
        today: Datetime::from_ymd(2026, 10, 24).unwrap(),
        system_fonts: false,
        font_paths: Vec::new(),
        package_paths: Vec::new(),
        allow_packages: false,
        inputs,
    })
    .unwrap()
}

fn compile(
    emitted: &Emitted, assets: &AssetRegistry, config: &Config,
    parts: &[(&str, &Emitted)],
) -> PagedDocument {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("refs.bib"), BIB).unwrap();
    for (name, part) in parts {
        fs::write(root.path().join(name), &part.text).unwrap();
    }
    let mut world = world(root.path(), config);
    world.set_assets(assets.clone());
    world.set_main_text(&emitted.text, emitted.map.clone());
    compiled(&mut world, &emitted.text)
}

fn compiled(world: &mut WriterWorld, source: &str) -> PagedDocument {
    let compiled = compile_document(world);
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
            _ => {}
        }
    }
}

fn visible(document: &PagedDocument) -> String {
    let mut text = String::new();
    for page in document.pages() {
        frame_text(&page.frame, &mut text);
    }
    compact(&text)
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn compile_text(document: &Document) -> String {
    visible(&compile(
        &render(document),
        &AssetRegistry::new(),
        &Config::default(),
        &[],
    ))
}

fn span(line: usize) -> SourceSpan {
    SourceSpan { start_line: line, end_line: line + 1, cell: None, file: None }
}

fn expect_map(emitted: &Emitted, marker: &str, expected: &SourceSpan) {
    let line = emitted
        .text
        .lines()
        .position(|line| line.contains(marker))
        .unwrap_or_else(|| panic!("missing {marker:?}: {}", emitted.text))
        + 1;
    assert_eq!(emitted.map.lookup(line), Some(expected), "{marker}");
}

#[test]
fn sample_build_is_deterministic_and_does_not_mutate_the_ir() {
    let (document, assets) = fixture();
    let original = document.clone();
    let config: Config =
        toml::from_str("[numbering]\nheadings = '1.1'").unwrap();
    let emitted = emit(&document, &assets, &options(&config)).unwrap();
    let repeated = emit(&document, &assets, &options(&config)).unwrap();
    assert_eq!(emitted.text, repeated.text);
    assert_eq!(emitted.map, repeated.map);
    assert_eq!(emitted.assets, repeated.assets);
    assert_eq!(emitted.diagnostics, repeated.diagnostics);
    assert_eq!(document, original);
    assert!(emitted.diagnostics.is_empty(), "{:?}", emitted.diagnostics);
    assert_eq!(emitted.map.len(), emitted.text.lines().count());
    assert!(emitted.text.ends_with('\n'));
    assert!(!emitted.text.contains('\r'));
    let trailing: Vec<_> = emitted
        .text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim_end() != *line)
        .map(|(index, line)| (index + 1, format!("{line:?}")))
        .collect();
    assert!(trailing.is_empty(), "trailing spaces: {trailing:?}");
    assert!(emitted.text.contains("#import \"@writer/doc:0.1.0\": *"));
    assert!(
        emitted.text.contains("#import \"@preview/mitex:0.2.7\": mi, mitex")
    );
    assert!(emitted.text.contains("toc: false"));
    assert!(!emitted.text.contains("/abs/document"));
    insta::assert_snapshot!(emitted.text);
}

#[test]
fn sample_source_compiles_with_the_embedded_packages_and_fonts() {
    let (document, assets) = fixture();
    let config: Config =
        toml::from_str("[numbering]\nheadings = '1.1'").unwrap();
    let emitted = emit(&document, &assets, &options(&config)).unwrap();
    let compiled = compile(&emitted, &assets, &config, &[]);
    let text = visible(&compiled);
    for expected in [
        "GPUallocation", "Introduction", "Demanddoubled", "Throughput",
        "Reproducibility", "df.describe()", "Knuth", "Everyrunused",
    ] {
        assert!(text.contains(expected), "missing {expected:?}: {text}");
    }
    assert_eq!(compiled.info().title.as_deref(), Some("GPU allocation"));
    assert_eq!(compiled.info().author, ["J. Blaschke"]);
    assert_eq!(
        oracle(&compiled, "sec-intro").unwrap().number.as_deref(),
        Some("1")
    );
    assert_eq!(oracle(&compiled, "fig-throughput").unwrap().kind, "figure");
    assert_eq!(
        oracle(&compiled, "eq-amdahl").unwrap().number.as_deref(),
        Some("(1)")
    );
}

#[test]
fn markup_special_characters_and_shorthands_survive_compilation() {
    let literal =
        r#"all \ # * _ ` $ < > @ [ ] ~ ' " -- -? -2 ... // /* https://x."#;
    let emitted = render(&document(vec![paragraph(literal)]));
    for escaped in [
        r"\\", r"\#", r"\*", r"\_", r"\`", r"\$", r"\<", r"\>", r"\@", r"\[",
        r"\]", r"\~", r"\'", r#"\""#,
    ] {
        assert!(
            emitted.text.contains(escaped),
            "{escaped:?}: {}",
            emitted.text
        );
    }
    let visible = visible(&compile(
        &emitted,
        &AssetRegistry::new(),
        &Config::default(),
        &[],
    ));
    assert!(visible.contains(&compact(literal)), "{visible}");
}

#[test]
fn line_start_syntax_is_defused_across_text_nodes() {
    let cases = ["= heading", "+ enum", "- item", "/ term: body", "12. item"];
    let blocks = cases
        .iter()
        .map(|case| {
            Block::paragraph(vec![Inline::text("  "), Inline::text(case)])
        })
        .collect();
    let source = render(&document(blocks));
    for (case, escaped) in cases.iter().zip([
        r"\= heading", r"\+ enum", r"\- item", r"\/ term: body", r"12\. item",
    ]) {
        assert!(source.text.contains(escaped), "{case}: {}", source.text);
    }
    let text = visible(&compile(
        &source,
        &AssetRegistry::new(),
        &Config::default(),
        &[],
    ));
    for case in cases {
        assert!(text.contains(&compact(case)), "{text}");
    }
}

#[test]
fn code_expressions_keep_following_punctuation_and_word_boundaries() {
    let inlines = vec![
        Inline::Strikeout { inlines: text("A") },
        Inline::text(".Hello "),
        Inline::Underline { inlines: text("B") },
        Inline::text("(group) "),
        Inline::SmallCaps { inlines: text("C") },
        Inline::text(";tail "),
        Inline::text("x"),
        Inline::Emph { inlines: text("middle") },
        Inline::text("y "),
        Inline::Strong { inlines: text("bold") },
        Inline::text("z"),
    ];
    let doc = document(vec![Block::paragraph(inlines)]);
    let emitted = render(&doc);
    assert!(emitted.text.contains("#strike[A];.Hello"), "{}", emitted.text);
    assert!(emitted.text.contains("#underline[B];(group)"), "{}", emitted.text);
    assert!(emitted.text.contains("#smallcaps[C];;tail"), "{}", emitted.text);
    assert!(emitted.text.contains("#emph[middle]"), "{}", emitted.text);
    assert!(emitted.text.contains("#strong[bold]"), "{}", emitted.text);
    let text = compile_text(&doc);
    assert!(text.contains("A.HelloB(group)C;tailxmiddleyboldz"), "{text}");
}

#[test]
fn flattened_spans_preserve_a_nested_code_expression_terminator() {
    let doc = document(vec![Block::paragraph(vec![
        Inline::Span {
            attrs: Attrs::default(),
            inlines: vec![Inline::Span {
                attrs: Attrs::default(),
                inlines: vec![Inline::Strikeout { inlines: text("Nested") }],
            }],
        },
        Inline::text(".after"),
    ])]);
    let emitted = render(&doc);
    assert!(
        emitted.text.contains("#strike[Nested];.after"),
        "{}",
        emitted.text
    );
    assert!(compile_text(&doc).contains("Nested.after"));
}

#[test]
fn shorthand_references_do_not_gain_visible_expression_terminators() {
    let mut doc = document(vec![
        Block::Heading {
            level: 1,
            inlines: text("Main"),
            attrs: Attrs::with_id("sec-main"),
            number: Some("1".into()),
            span: None,
        },
        Block::paragraph(vec![
            inline(
                json!({"type": "ref", "target": "sec-main", "supplement": null, "form": "normal"}),
            ),
            Inline::text("(tail) "),
            inline(
                json!({"type": "ref", "target": "sec-main", "supplement": null, "form": "normal"}),
            ),
            Inline::text(";end"),
        ]),
    ]);
    doc.resolved.labels.insert(
        "sec-main".into(),
        LabelInfo {
            kind: LabelKind::Heading,
            number: Some("1".into()),
            supplement: "Section".into(),
            text: Some("Main".into()),
            span: None,
        },
    );
    let config: Config =
        toml::from_str("[numbering]\nheadings = true").unwrap();
    let emitted = emit(&doc, &AssetRegistry::new(), &options(&config)).unwrap();
    assert!(emitted.text.contains("@sec-main(tail)"), "{}", emitted.text);
    assert!(emitted.text.contains("@sec-main;end"), "{}", emitted.text);
    let text = visible(&compile(&emitted, &AssetRegistry::new(), &config, &[]));
    assert!(text.contains("Section1(tail)Section1;end"), "{text}");
}

#[test]
fn failed_latex_inline_math_keeps_following_punctuation_literal() {
    let mut inlines = Vec::new();
    for suffix in [".after ", "(tail) ", ";end"] {
        inlines.push(Inline::Math {
            source: r"\foo{x}".into(),
            dialect: MathDialect::Latex,
        });
        inlines.push(Inline::text(suffix));
    }
    let doc = document(vec![Block::paragraph(inlines)]);
    let emitted = render(&doc);
    assert_eq!(emitted.diagnostics.len(), 3);
    let text = visible(&compile(
        &emitted,
        &AssetRegistry::new(),
        &Config::default(),
        &[],
    ));
    assert!(text.contains(r"\foo{x}.after\foo{x}(tail)\foo{x};end"), "{text}");
}

#[test]
fn definition_colons_and_heading_breaks_do_not_change_their_structure() {
    let term = vec![
        Inline::text("Key:Type"),
        Inline::SoftBreak,
        Inline::text("next"),
        Inline::LineBreak,
        Inline::text("last"),
    ];
    let doc = document(vec![
        Block::Heading {
            level: 1,
            inlines: vec![
                Inline::text("First"),
                Inline::SoftBreak,
                Inline::text("second"),
                Inline::LineBreak,
                Inline::text("third"),
            ],
            attrs: Attrs::with_id("sec-breaks"),
            number: None,
            span: None,
        },
        Block::DefinitionList {
            items: vec![DefinitionItem {
                term,
                definitions: vec![vec![
                    paragraph("Definition body."),
                    paragraph("Further definition."),
                ]],
            }],
            span: None,
        },
    ]);
    let emitted = render(&doc);
    assert!(emitted.text.contains(r"Key\:Type next#linebreak()last:"));
    assert!(emitted.text.contains("= First second#linebreak()third"));
    assert!(
        emitted.text.contains("Definition body.\n\n  Further definition."),
        "{}",
        emitted.text
    );
    let text = compile_text(&doc);
    assert!(
        text.contains("FirstsecondthirdKey:TypenextlastDefinitionbody."),
        "{text}"
    );
    assert!(text.contains("Definitionbody.Furtherdefinition."), "{text}");
}

#[test]
fn raw_fences_and_invalid_language_tags_preserve_the_code() {
    let code = "before\n```\n\n````\nafter";
    let doc = document(vec![
        Block::BlockQuote {
            blocks: vec![Block::CodeBlock {
                lang: Some("python".into()),
                text: code.into(),
                attrs: Attrs::default(),
                span: None,
            }],
            span: None,
        },
        Block::CodeBlock {
            lang: Some("c++".into()),
            text: "int main() { return 0; }".into(),
            attrs: Attrs {
                classes: vec!["numbered".into()],
                ..Attrs::default()
            },
            span: None,
        },
        Block::List {
            kind: ListKind::Bullet,
            tight: false,
            items: vec![
                ListItem {
                    checked: None,
                    blocks: vec![Block::CodeBlock {
                        lang: Some("python".into()),
                        text: "non_task_code".into(),
                        attrs: Attrs::default(),
                        span: None,
                    }],
                },
                ListItem {
                    checked: None,
                    blocks: vec![
                        paragraph("First list paragraph."),
                        paragraph("Second list paragraph."),
                    ],
                },
            ],
            span: None,
        },
    ]);
    let emitted = render(&doc);
    assert!(emitted.text.contains("  `````python\n"), "{}", emitted.text);
    assert!(emitted.text.contains("\n  `````\n"), "{}", emitted.text);
    assert!(emitted.text.contains("#raw("), "{}", emitted.text);
    assert!(emitted.text.contains("lang: \"c++\""), "{}", emitted.text);
    assert!(!emitted.text.contains("```c++"));
    assert!(!emitted.text.contains("\n  \n"));
    assert!(
        emitted
            .text
            .contains("First list paragraph.\n\n  Second list paragraph."),
        "{}",
        emitted.text
    );
    let syntax = typst_syntax::parse(&emitted.text);
    let list_items = syntax
        .children()
        .filter(|node| node.kind() == typst_syntax::SyntaxKind::ListItem)
        .count();
    assert_eq!(
        list_items, 2,
        "code-only item must stay a list: {}",
        emitted.text
    );
    let text = compile_text(&doc);
    assert!(text.contains("before```````after"), "{text}");
    assert!(text.contains("intmain(){return0;}"), "{text}");
    assert!(text.contains("non_task_code"), "{text}");
    assert!(text.contains("Firstlistparagraph.Secondlistparagraph."), "{text}");
}

#[test]
fn string_literals_escape_quotes_backslashes_and_controls() {
    let code = "quote\" slash\\ newline\nreturn\rtab\tcontrol\u{1}";
    let doc = document(vec![Block::paragraph(vec![Inline::Code {
        text: code.into(),
        attrs: Attrs::default(),
    }])]);
    let emitted = render(&doc);
    for escaped in [r#"\""#, r"\\", r"\n", r"\r", r"\t"] {
        assert!(emitted.text.contains(escaped), "{escaped}: {}", emitted.text);
    }
    assert!(
        emitted.text.contains(r"\u{1}") || emitted.text.contains(r"\u{01}")
    );
    assert!(!emitted.text.contains('\u{1}'));
}

#[test]
fn source_map_uses_the_innermost_spanned_block() {
    let outer = span(10);
    let inner = SourceSpan {
        cell: Some(3),
        file: Some(PathBuf::from("/docs/part.ipynb")),
        ..span(20)
    };
    let code = span(30);
    let doc = document(vec![Block::BlockQuote {
        blocks: vec![
            Block::Paragraph {
                inlines: vec![
                    Inline::text("inner one"),
                    Inline::SoftBreak,
                    Inline::text("inner two"),
                ],
                span: Some(inner.clone()),
            },
            Block::CodeBlock {
                lang: None,
                text: "code one\ncode two".into(),
                attrs: Attrs::default(),
                span: Some(code.clone()),
            },
            paragraph("inherited outer"),
        ],
        span: Some(outer.clone()),
    }]);
    let emitted = render(&doc);
    assert_eq!(emitted.map.len(), emitted.text.lines().count());
    expect_map(&emitted, "#quote(block: true)", &outer);
    expect_map(&emitted, "inner one", &inner);
    expect_map(&emitted, "inner two", &inner);
    expect_map(&emitted, "code one", &code);
    expect_map(&emitted, "code two", &code);
    expect_map(&emitted, "inherited outer", &outer);
    for (line, text) in emitted.text.lines().enumerate() {
        if text.starts_with("//")
            || text.starts_with("#import")
            || text.starts_with("#show")
        {
            assert_eq!(emitted.map.lookup(line + 1), None);
        }
    }
}

#[test]
fn editing_one_block_keeps_unrelated_source_bytes_and_maps_stable() {
    let mut doc = document(vec![
        paragraph("Stable beginning."),
        paragraph("Editable."),
        Block::Paragraph {
            inlines: text("Stable tail."),
            span: Some(span(50)),
        },
    ]);
    let before = render(&doc);
    doc.blocks[1] = Block::paragraph(vec![
        Inline::text("Changed."),
        Inline::SoftBreak,
        Inline::text("More lines."),
    ]);
    let after = render(&doc);
    assert_eq!(
        before.text.split("Stable beginning.\n").next(),
        after.text.split("Stable beginning.\n").next(),
    );
    assert_eq!(
        before.text.split("Stable tail.").nth(1),
        after.text.split("Stable tail.").nth(1),
    );
    expect_map(&before, "Stable tail.", &span(50));
    expect_map(&after, "Stable tail.", &span(50));
}

fn image(url: &str, attrs: Attrs) -> Inline {
    Inline::Image { alt: text("Plot"), url: url.into(), title: None, attrs }
}

#[test]
fn asset_modes_are_stable_and_export_names_resolve_collisions() {
    let root = tempfile::tempdir().unwrap();
    let mut assets = AssetRegistry::new();
    let first = assets.register_file(&root.path().join("first/plot.svg"));
    let second = assets.register_file(&root.path().join("second/plot.svg"));
    let weird =
        assets.register_bytes(SVG.to_vec(), "image/svg+xml", "a b@.svg");
    let mut doc = document(vec![Block::paragraph(vec![
        image(&AssetRegistry::placeholder(&second), Attrs::default()),
        image(&AssetRegistry::placeholder(&first), Attrs::default()),
        image(&AssetRegistry::placeholder(&weird), Attrs::default()),
    ])]);
    doc.source.base_dir = root.path().join("docs");
    doc.source.path = doc.source.base_dir.join("document.md");
    let config = Config::default();
    let emitted = emit(&doc, &assets, &options(&config)).unwrap();
    for id in [&first, &second, &weird] {
        assert_eq!(emitted.assets[id], format!("/__assets/{id}.svg"));
    }
    let mut opts = options(&config);
    opts.assets = AssetPaths::Export { dir: "document_assets".into() };
    let exported = emit(&doc, &assets, &opts).unwrap();
    let mut ids = [&first, &second];
    ids.sort();
    assert_eq!(exported.assets[ids[0]], "document_assets/plot.svg");
    assert_eq!(
        exported.assets[ids[1]],
        format!("document_assets/plot-{}.svg", &ids[1][..6]),
    );
    assert_eq!(exported.assets[&weird], "document_assets/a-b-.svg");
    let paths: BTreeSet<_> = exported.assets.values().collect();
    assert_eq!(paths.len(), 3);
    doc.blocks[0] = Block::paragraph(vec![
        image(&AssetRegistry::placeholder(&first), Attrs::default()),
        image(&AssetRegistry::placeholder(&second), Attrs::default()),
    ]);
    opts.assets = AssetPaths::Source;
    let source = emit(&doc, &assets, &opts).unwrap();
    assert_eq!(source.assets[&first], "../first/plot.svg");
    assert_eq!(source.assets[&second], "../second/plot.svg");
}

#[cfg(unix)]
#[test]
fn source_asset_paths_use_the_same_identity_for_symlinked_directories() {
    let root = tempfile::tempdir().unwrap();
    let real = root.path().join("real");
    let alias = root.path().join("alias");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("document.md"), "Source asset.").unwrap();
    fs::write(real.join("image.svg"), SVG).unwrap();
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let mut assets = AssetRegistry::new();
    let id = assets.register_file(&alias.join("image.svg"));
    let mut doc = document(vec![Block::paragraph(vec![image(
        &AssetRegistry::placeholder(&id),
        Attrs::default(),
    )])]);
    doc.source.path = alias.join("document.md");
    doc.source.base_dir = alias.clone();
    let config = Config::default();
    let mut opts = options(&config);
    opts.assets = AssetPaths::Source;
    let emitted = emit(&doc, &assets, &opts).unwrap();
    assert_eq!(emitted.assets[&id], "image.svg");
    assert_eq!(fs::read(alias.join(&emitted.assets[&id])).unwrap(), SVG);
    let mut world = world(&alias, &config);
    world.set_main_text(&emitted.text, emitted.map.clone());
    compiled(&mut world, &emitted.text);
}

fn ordered_asset_bytes() -> Vec<(String, Vec<u8>)> {
    let mut probe = AssetRegistry::new();
    let mut data: Vec<_> = (0..3)
        .map(|index| {
            let mut bytes = SVG.to_vec();
            bytes.extend(std::iter::repeat_n(b' ', index));
            let id = probe.register_bytes(
                bytes.clone(),
                "image/svg+xml",
                "probe.svg",
            );
            (id, bytes)
        })
        .collect();
    data.sort_by(|a, b| a.0.cmp(&b.0));
    data
}

#[test]
fn generated_export_suffixes_cannot_collide_with_an_original_hint() {
    let data = ordered_asset_bytes();
    let mut assets = AssetRegistry::new();
    let occupied = format!("plot-{}.svg", &data[2].0[..6]);
    for ((_, bytes), hint) in
        data.iter().zip([occupied.as_str(), "plot.svg", "plot.svg"])
    {
        assets.register_bytes(bytes.clone(), "image/svg+xml", hint);
    }
    let doc = document(vec![Block::paragraph(
        data.iter()
            .rev()
            .map(|(id, _)| {
                image(&AssetRegistry::placeholder(id), Attrs::default())
            })
            .collect(),
    )]);
    let config = Config::default();
    let mut opts = options(&config);
    opts.assets = AssetPaths::Export { dir: "document_assets".into() };
    let emitted = emit(&doc, &assets, &opts).unwrap();
    let paths: BTreeSet<_> = emitted.assets.values().collect();
    assert_eq!(paths.len(), 3, "{:?}", emitted.assets);
    assert_eq!(
        emitted.assets[&data[0].0],
        format!("document_assets/{occupied}")
    );
    assert_eq!(emitted.assets[&data[1].0], "document_assets/plot.svg");
}

#[test]
fn unreferenced_assets_do_not_consume_export_names() {
    let data = ordered_asset_bytes();
    let mut assets = AssetRegistry::new();
    for (_, bytes) in &data {
        assets.register_bytes(bytes.clone(), "image/svg+xml", "plot.svg");
    }
    let doc = document(vec![Block::paragraph(vec![image(
        &AssetRegistry::placeholder(&data[2].0),
        Attrs::default(),
    )])]);
    let config = Config::default();
    let mut opts = options(&config);
    opts.assets = AssetPaths::Export { dir: "document_assets".into() };
    let emitted = emit(&doc, &assets, &opts).unwrap();
    assert_eq!(emitted.assets.len(), 1);
    assert_eq!(emitted.assets[&data[2].0], "document_assets/plot.svg");
}

#[test]
fn image_sizes_convert_pixels_and_remote_images_remain_readable() {
    let mut assets = AssetRegistry::new();
    let id = assets.register_bytes(SVG.to_vec(), "image/svg+xml", "plot.svg");
    let doc = document(vec![Block::paragraph(vec![
        image(
            &AssetRegistry::placeholder(&id),
            Attrs {
                pairs: [
                    ("width".into(), "96px".into()),
                    ("height".into(), "32".into()),
                ]
                .into(),
                ..Attrs::default()
            },
        ),
        image("https://example.com/plot.svg", Attrs::default()),
    ])]);
    let emitted = emit(&doc, &assets, &options(&Config::default())).unwrap();
    assert!(emitted.text.contains("width: 72pt"), "{}", emitted.text);
    assert!(emitted.text.contains("height: 24pt"), "{}", emitted.text);
    assert!(
        emitted.text.contains("#link(\"https://example.com/plot.svg\")[Plot]")
    );
    assert_eq!(emitted.diagnostics.len(), 1);
    assert_eq!(
        emitted.diagnostics.iter().next().unwrap().code,
        "unsupported-construct"
    );
    assert_eq!(
        emitted.diagnostics.iter().next().unwrap().level,
        Level::Warning
    );
    compile(&emitted, &assets, &Config::default(), &[]);
}

#[test]
fn virtual_asset_extensions_follow_mime_instead_of_filename_hints() {
    let mut assets = AssetRegistry::new();
    let id =
        assets.register_bytes(SVG.to_vec(), "image/svg+xml", "misleading.png");
    let doc = document(vec![Block::paragraph(vec![image(
        &AssetRegistry::placeholder(&id),
        Attrs::default(),
    )])]);
    let emitted = emit(&doc, &assets, &options(&Config::default())).unwrap();
    assert_eq!(emitted.assets[&id], format!("/__assets/{id}.svg"));
    compile(&emitted, &assets, &Config::default(), &[]);
}

#[test]
fn missing_asset_placeholders_fail_instead_of_emitting_broken_paths() {
    let doc = document(vec![Block::paragraph(vec![image(
        "asset:0123456789ab",
        Attrs::default(),
    )])]);
    let error = emit(&doc, &AssetRegistry::new(), &options(&Config::default()))
        .expect_err("unknown placeholder must fail");
    assert!(error.to_string().contains("0123456789ab"));
}

#[test]
fn unresolved_documents_use_placeholders_but_fragments_keep_syntax() {
    let doc = document(vec![Block::paragraph(vec![
        inline(
            json!({"type": "ref", "target": "sec-whole", "supplement": null, "form": "normal"}),
        ),
        Inline::text(" "),
        inline(
            json!({"type": "cite", "items": [{"key": "knuth1984"}], "mode": "normal"}),
        ),
        Inline::text(" "),
        Inline::Link {
            inlines: text("Whole"),
            url: "#sec-whole".into(),
            title: None,
            attrs: Attrs::default(),
        },
    ])]);
    let emitted = render(&doc);
    assert!(emitted.text.contains(r"\[?sec-whole\]"));
    assert!(emitted.text.contains(r"\[?knuth1984\]"));
    assert!(!emitted.text.contains("#link(<sec-whole>)"));
    let config = Config::default();
    let mut opts = options(&config);
    opts.fragment = true;
    let fragment = emit(&doc, &AssetRegistry::new(), &opts).unwrap();
    assert!(!fragment.text.contains("#show"));
    assert!(fragment.text.contains("#import \"@writer/doc:0.1.0\": *"));
    assert!(fragment.text.contains("@sec-whole"));
    assert!(fragment.text.contains("@knuth1984"));
    assert!(fragment.text.contains("#link(<sec-whole>)[Whole]"));
    assert!(!fragment.text.contains("[?"));
}

struct Fragment {
    document: Document,
    config: Config,
}

impl IncludeProvider for Fragment {
    fn synthesize(&self, path: &Path, _root: &Path) -> Result<Synthesized> {
        let mut document = self.document.clone();
        document.source.path = path.into();
        document.source.base_dir = path.parent().unwrap().into();
        let mut opts = options(&self.config);
        opts.fragment = true;
        opts.footnote_prefix = "abcdef12-".into();
        let assets = AssetRegistry::new();
        let emitted = emit(&document, &assets, &opts)?;
        Ok(Synthesized {
            text: emitted.text,
            map: emitted.map,
            assets,
            files: vec![path.into()],
            diagnostics: emitted.diagnostics,
        })
    }
}

#[test]
fn unresolved_fragment_compiles_in_the_whole_document_scope() {
    let config: Config =
        toml::from_str("[numbering]\nheadings = true").unwrap();
    let fragment = Fragment {
        document: document(vec![Block::paragraph(vec![
            Inline::text("Included reference "),
            inline(
                json!({"type": "ref", "target": "sec-whole", "supplement": null, "form": "normal"}),
            ),
            Inline::text(" and cite "),
            inline(
                json!({"type": "cite", "items": [{"key": "knuth1984"}], "mode": "normal"}),
            ),
            Inline::Footnote {
                blocks: vec![paragraph("Fragment note.")],
                label: Some("odd name!".into()),
                number: None,
            },
            Inline::FootnoteRef { label: "odd name!".into(), number: None },
        ])]),
        config: config.clone(),
    };
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("part.md"), "provider-owned source").unwrap();
    fs::write(root.path().join("refs.bib"), BIB).unwrap();
    let synthesized =
        fragment.synthesize(&root.path().join("part.md"), root.path()).unwrap();
    assert!(synthesized.text.contains("<writer-fn-abcdef12-odd_name_>"));
    assert!(
        synthesized.text.contains("#footnote(<writer-fn-abcdef12-odd_name_>)")
    );
    let source = "#import \"@writer/doc:0.1.0\": *\n\
        #show: writer-doc.with(numbering: true)\n\
        = Whole <sec-whole>\n\
        #include \"part.md\"\n\
        #bibliography(\"refs.bib\", style: \"ieee\")\n";
    let mut world = world(root.path(), &config);
    world.set_include_provider(Box::new(fragment));
    world.set_main_text(source, SourceMap::default());
    let compiled = compiled(&mut world, source);
    let text = visible(&compiled);
    assert!(text.contains("IncludedreferenceSection1andcite[1]"), "{text}");
    assert_eq!(text.matches("Fragmentnote.").count(), 1);
    assert!(world.include_diagnostics().is_empty());
    let part_path = fs::canonicalize(root.path().join("part.md")).unwrap();
    assert!(world.accessed().contains(&part_path));
}

fn note(label: &str, number: u32, content: &str) -> Inline {
    Inline::Footnote {
        blocks: vec![paragraph(content)],
        label: Some(label.into()),
        number: Some(number),
    }
}

#[test]
fn split_parts_share_footnote_labels_and_references_across_the_unit() {
    let doc = document(vec![
        Block::paragraph(vec![
            Inline::text("Before"),
            note("shared label", 1, "First note."),
        ]),
        Block::paragraph(vec![
            Inline::text("Inside"),
            note("shared label", 2, "Second note."),
        ]),
        Block::paragraph(vec![
            Inline::text("After"),
            Inline::FootnoteRef {
                label: "shared label".into(),
                number: Some(1),
            },
            Inline::Text { text: " and ".into() },
            Inline::FootnoteRef {
                label: "shared label".into(),
                number: Some(2),
            },
        ]),
    ]);
    let config = Config::default();
    let (main, parts) = emit_split(
        &doc,
        &AssetRegistry::new(),
        &options(&config),
        &[IncludeRun { start: 1, end: 2, file: "part.typ".into() }],
    )
    .unwrap();
    assert_eq!(parts.len(), 1);
    assert!(main.text.contains("#include \"part.typ\""));
    assert!(!main.text.contains("Inside"));
    assert!(!parts[0].text.contains("#show"));
    assert!(parts[0].text.contains("Inside"));
    assert!(main.text.contains("<writer-fn-shared_label>"), "{}", main.text);
    assert!(
        parts[0].text.contains("<writer-fn-shared_label-2>"),
        "{}",
        parts[0].text
    );
    assert!(main.text.contains("#footnote(<writer-fn-shared_label>)"));
    assert!(main.text.contains("#footnote(<writer-fn-shared_label-2>)"));
    let text = visible(&compile(
        &main,
        &AssetRegistry::new(),
        &config,
        &[("part.typ", &parts[0])],
    ));
    assert_eq!(text.matches("Firstnote.").count(), 1);
    assert_eq!(text.matches("Secondnote.").count(), 1);
    assert!(text.find("Before").unwrap() < text.find("Inside").unwrap());
    assert!(text.find("Inside").unwrap() < text.find("After").unwrap());
}

#[test]
fn generated_note_labels_cannot_collide_with_existing_suffixes() {
    let doc = document(vec![
        Block::paragraph(vec![
            Inline::text("First"),
            note("a", 1, "First note."),
        ]),
        Block::paragraph(vec![
            Inline::text("Second"),
            note("a-2", 2, "Second note."),
        ]),
        Block::Paragraph {
            inlines: vec![
                Inline::text("Included"),
                note("a", 3, "Included note."),
            ],
            span: Some(SourceSpan {
                file: Some(PathBuf::from("/docs/included.md")),
                ..span(10)
            }),
        },
        Block::paragraph(
            (1..=3)
                .map(|number| Inline::FootnoteRef {
                    label: if number == 2 { "a-2" } else { "a" }.into(),
                    number: Some(number),
                })
                .collect(),
        ),
    ]);
    let emitted = render(&doc);
    for label in ["writer-fn-a", "writer-fn-a-2", "writer-fn-a-3"] {
        assert!(
            emitted.text.contains(&format!("<{label}>")),
            "{}",
            emitted.text
        );
        assert!(
            emitted.text.contains(&format!("#footnote(<{label}>)")),
            "{}",
            emitted.text
        );
    }
    let text = compile_text(&doc);
    for content in ["Firstnote.", "Secondnote.", "Includednote."] {
        assert_eq!(text.matches(content).count(), 1, "{text}");
    }
}

#[test]
fn image_alt_text_does_not_consume_footnote_labels() {
    let mut assets = AssetRegistry::new();
    let id = assets.register_bytes(SVG.to_vec(), "image/svg+xml", "plot.svg");
    let doc = document(vec![Block::paragraph(vec![
        Inline::Image {
            alt: vec![Inline::text("Plot"), note("a", 1, "Alt note.")],
            url: AssetRegistry::placeholder(&id),
            title: None,
            attrs: Attrs::default(),
        },
        note("a", 1, "Real note."),
        Inline::FootnoteRef { label: "a".into(), number: Some(1) },
    ])]);
    let emitted = emit(&doc, &assets, &options(&Config::default())).unwrap();
    assert!(emitted.text.contains("<writer-fn-a>"), "{}", emitted.text);
    assert!(!emitted.text.contains("<writer-fn-a-2>"), "{}", emitted.text);
    let text = visible(&compile(&emitted, &assets, &Config::default(), &[]));
    assert_eq!(text.matches("Realnote.").count(), 1);
    assert!(!text.contains("Altnote."));
}

#[test]
fn split_ranges_preserve_top_level_boundaries_and_part_source_maps() {
    let mut doc = document(
        (0..5)
            .map(|index| Block::Paragraph {
                inlines: text(&format!("Block{index}.")),
                span: Some(span(10 + index)),
            })
            .collect(),
    );
    doc.blocks.push(paragraph("Tail."));
    let (main, parts) = emit_split(
        &doc,
        &AssetRegistry::new(),
        &options(&Config::default()),
        &[
            IncludeRun { start: 0, end: 2, file: "first part.typ".into() },
            IncludeRun { start: 3, end: 5, file: "second.typ".into() },
        ],
    )
    .unwrap();
    assert_eq!(parts.len(), 2);
    assert!(main.text.contains("#include \"first part.typ\""));
    assert!(main.text.contains("#include \"second.typ\""));
    assert!(main.text.contains("Block2."));
    assert!(main.text.contains("Tail."));
    for index in [0, 1, 3, 4] {
        assert!(!main.text.contains(&format!("Block{index}.")));
    }
    assert!(parts[0].text.contains("Block0.\n\nBlock1."));
    assert!(parts[1].text.contains("Block3.\n\nBlock4."));
    expect_map(&parts[0], "Block0.", &span(10));
    expect_map(&parts[0], "Block1.", &span(11));
    expect_map(&parts[1], "Block3.", &span(13));
    expect_map(&parts[1], "Block4.", &span(14));
    for emitted in [&main, &parts[0], &parts[1]] {
        assert_eq!(emitted.map.len(), emitted.text.lines().count());
    }
    let text = visible(&compile(
        &main,
        &AssetRegistry::new(),
        &Config::default(),
        &[("first part.typ", &parts[0]), ("second.typ", &parts[1])],
    ));
    assert!(
        text.contains("Block0.Block1.Block2.Block3.Block4.Tail."),
        "{text}"
    );
}

#[test]
fn unnumbered_references_link_text_and_ambiguous_labels_use_ref_calls() {
    let mut doc = document(vec![
        Block::Heading {
            level: 1,
            inlines: text("Unnumbered"),
            attrs: Attrs {
                classes: vec!["unnumbered".into()],
                ..Attrs::with_id("sec-off")
            },
            number: None,
            span: None,
        },
        Block::Heading {
            level: 1,
            inlines: text("Numbered"),
            attrs: Attrs::with_id("sec-end."),
            number: Some("1".into()),
            span: None,
        },
        Block::paragraph(vec![
            inline(
                json!({"type": "ref", "target": "sec-off", "form": "normal", "supplement": null,
                "resolved": {"kind": "heading", "number": "", "text": [{"type": "text", "text": "Unnumbered"}], "anchor": "sec-off"}}),
            ),
            Inline::text("; "),
            inline(
                json!({"type": "ref", "target": "sec-end.", "form": "normal", "supplement": []}),
            ),
            Inline::text("; "),
            inline(
                json!({"type": "ref", "target": "sec-end.", "form": "page", "supplement": [{"type": "text", "text": "page"}]}),
            ),
        ]),
    ]);
    for (label, number) in [("sec-off", None), ("sec-end.", Some("1".into()))] {
        doc.resolved.labels.insert(
            label.into(),
            LabelInfo {
                kind: LabelKind::Heading,
                number,
                supplement: "Section".into(),
                text: Some("Unnumbered".into()),
                span: None,
            },
        );
    }
    let config = Config {
        numbering: writer::config::NumberingConfig {
            headings: HeadingNumbering::Pattern("1.1".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let emitted = emit(&doc, &AssetRegistry::new(), &options(&config)).unwrap();
    assert!(emitted.text.contains("#link(<sec-off>)[Unnumbered]"));
    assert!(emitted.text.contains("#ref(<sec-end.>, supplement: [])"));
    assert!(
        emitted
            .text
            .contains("#ref(<sec-end.>, form: \"page\", supplement: [page])")
    );
    let text = visible(&compile(&emitted, &AssetRegistry::new(), &config, &[]));
    assert!(text.contains("Unnumbered;1;page"), "{text}");
}

#[test]
fn native_anchors_and_target_passthrough_compile_without_foreign_raw() {
    let doc = document(vec![
        Block::Div {
            attrs: Attrs::with_id("div-anchor"),
            blocks: vec![Block::paragraph(vec![Inline::Span {
                attrs: Attrs::with_id("span-anchor"),
                inlines: text("Anchored"),
            }])],
            span: None,
        },
        block(
            json!({"type": "raw", "format": "typst", "text": "#strong[Native]"}),
        ),
        block(
            json!({"type": "raw", "format": "html", "text": "<b>Foreign</b>"}),
        ),
    ]);
    let emitted = render(&doc);
    assert!(emitted.text.contains("#metadata(none) <div-anchor>"));
    assert!(emitted.text.contains("#metadata(none)<span-anchor>Anchored"));
    assert!(emitted.text.contains("#strong[Native]"));
    assert!(!emitted.text.contains("Foreign"));
    assert!(compile_text(&doc).contains("AnchoredNative"));
}

#[test]
fn math_preparation_reports_tags_and_failed_conversion_without_failing_emit() {
    let doc = document(vec![
        Block::Math {
            source: r"\begin{equation}x\tag{3}\end{equation}".into(),
            dialect: MathDialect::Latex,
            attrs: Attrs::with_id("eq-tag"),
            number: Some("1".into()),
            span: Some(span(14)),
        },
        Block::Math {
            source: r"\foo{x}".into(),
            dialect: MathDialect::Latex,
            attrs: Attrs::default(),
            number: None,
            span: Some(span(20)),
        },
    ]);
    let emitted = render(&doc);
    let codes: Vec<_> = emitted.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&"math-tag-ignored"), "{codes:?}");
    assert!(codes.contains(&"math-conversion-failed"), "{codes:?}");
    assert!(emitted.diagnostics.iter().all(|d| d.level == Level::Warning));
    assert!(emitted.text.contains("#mitex(`x`)"), "{}", emitted.text);
    assert!(!emitted.text.contains(r"\tag{3}"));
    assert!(emitted.text.contains(r"#raw("), "{}", emitted.text);
    let text = visible(&compile(
        &emitted,
        &AssetRegistry::new(),
        &Config::default(),
        &[],
    ));
    assert!(text.contains(r"\foo{x}"), "{text}");
}

#[test]
fn backticks_in_latex_use_a_string_without_changing_the_source_bytes() {
    fn strings(node: &typst_syntax::SyntaxNode, values: &mut Vec<String>) {
        if let Some(value) = node.cast::<typst_syntax::ast::Str>() {
            values.push(value.get().to_string());
        }
        for child in node.children() {
            strings(child, values);
        }
    }
    let latex = r"\text{a`b}";
    let doc = document(vec![Block::paragraph(vec![Inline::Math {
        source: latex.into(),
        dialect: MathDialect::Latex,
    }])]);
    let emitted = render(&doc);
    assert!(emitted.diagnostics.is_empty(), "{:?}", emitted.diagnostics);
    assert!(emitted.text.contains("#mi(\""), "{}", emitted.text);
    let syntax = typst_syntax::parse(&emitted.text);
    let (errors, _) = syntax.errors_and_warnings();
    assert!(errors.is_empty(), "{errors:?}");
    let mut values = Vec::new();
    strings(&syntax, &mut values);
    assert!(values.iter().any(|value| value == latex), "{values:?}");
}

#[test]
fn notebook_outputs_choose_typst_candidates_and_respect_visibility() {
    let outputs = vec![CellOutput {
        candidates: vec![
            OutputCandidate {
                mime: "text/html".into(),
                blocks: vec![Block::Raw {
                    format: "html".into(),
                    text: "Foreign output".into(),
                    span: None,
                }],
            },
            OutputCandidate {
                mime: "text/plain".into(),
                blocks: vec![paragraph("Chosen output")],
            },
        ],
        ..Default::default()
    }];
    let doc = document(vec![
        Block::Cell(Cell {
            index: 7,
            language: "python".into(),
            source: "visible_input".into(),
            execution_count: Some(42),
            show_input: true,
            show_output: true,
            outputs: outputs.clone(),
            ..Default::default()
        }),
        Block::Cell(Cell {
            index: 8,
            language: "python".into(),
            source: "hidden_input".into(),
            execution_count: None,
            show_input: false,
            show_output: true,
            outputs: outputs.clone(),
            ..Default::default()
        }),
        Block::Cell(Cell {
            index: 9,
            language: "python".into(),
            source: "second_input".into(),
            execution_count: None,
            show_input: true,
            show_output: false,
            outputs,
            ..Default::default()
        }),
    ]);
    let emitted = render(&doc);
    assert!(emitted.text.contains("index: 7, count: 42"), "{}", emitted.text);
    assert!(emitted.text.contains("input: none"), "{}", emitted.text);
    assert_eq!(emitted.text.matches("Chosen output").count(), 2);
    assert!(!emitted.text.contains("Foreign output"));
    assert!(!emitted.text.contains("hidden_input"));
    let text = compile_text(&doc);
    assert!(text.contains("visible_input"), "{text}");
    assert!(text.contains("second_input"), "{text}");
    assert_eq!(text.matches("Chosenoutput").count(), 2);
    assert!(!text.contains("In["));
    assert!(!text.contains("Out["));
}

#[test]
fn metadata_and_bibliography_arguments_use_the_frozen_order_and_shapes() {
    let mut doc = document(vec![
        Block::Toc { depth: 2, span: None },
        Block::Bibliography { span: None },
    ]);
    doc.meta = DocMeta {
        title: Some("Title #1".into()),
        subtitle: Some("Subtitle".into()),
        authors: vec![Author { name: "Ada".into(), ..Default::default() }],
        date: Some("2026-10-24".into()),
        abstract_: Some(vec![paragraph("A summary.")]),
        keywords: vec!["one".into()],
        description: Some("Description".into()),
        lang: Some("de-AT".into()),
        vars: serde_json::from_value(json!({
            "abstract-title": "Overview", "toc-title": "Map",
            "reference-section-title": "Sources"
        }))
        .unwrap(),
    };
    doc.resolved.bibliography = Some(Bibliography {
        style: "ieee".into(),
        hanging_indent: true,
        entries: Vec::new(),
    });
    let config: Config =
        toml::from_str("[references]\nlocale = 'fr-CA'").unwrap();
    let mut opts = options(&config);
    opts.template = Some("/__writer/template/custom.typ".into());
    let emitted = emit(&doc, &AssetRegistry::new(), &opts).unwrap();
    assert!(
        emitted
            .text
            .contains("#import \"/__writer/template/custom.typ\": writer-doc")
    );
    let mut previous = 0;
    for name in [
        "title:", "subtitle:", "authors:", "date:", "abstract:",
        "abstract-title:", "keywords:", "description:", "lang:", "numbering:",
        "paper:", "toc:", "cite-locale:",
    ] {
        let offset = emitted
            .text
            .find(name)
            .unwrap_or_else(|| panic!("missing {name:?}: {}", emitted.text));
        assert!(offset >= previous, "out of order {name:?}: {}", emitted.text);
        previous = offset;
    }
    assert!(emitted.text.contains("authors: (\"Ada\",)"), "{}", emitted.text);
    assert!(emitted.text.contains("keywords: (\"one\",)"), "{}", emitted.text);
    assert!(emitted.text.contains(r"title: [Title \#1]"));
    assert!(emitted.text.contains("#outline(depth: 2, title: \"Map\")"));
    assert!(emitted.text.contains(
        "#bibliography((\"refs.bib\",), style: \"ieee\", title: \"Sources\")"
    ));
    assert!(!emitted.text.contains("@preview/mitex"));
    doc.resolved.bibliography = None;
    let empty = emit(&doc, &AssetRegistry::new(), &opts).unwrap();
    assert!(!empty.text.contains("#bibliography("));
}
