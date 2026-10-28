//! DOCX body contracts and complete package snapshots from resolved IR.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use serde_json::json;
use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::docx::{
    DocxOptions, DocxOutput, Picture, Renderer, ValidateOptions, validate,
    write,
};
use writer::error::Result;
use writer::ir::{Block, Document, MathDialect};
use writer::resolve::anchor;

#[path = "common/docx.rs"]
mod common_docx;

const PNG: &[u8] = include_bytes!("fixtures/md/images/plot.png");

/// Fixed bytes and dimensions keep these package tests independent of Typst.
struct FakeRenderer;

impl Renderer for FakeRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Ok(picture())
    }

    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Ok(picture())
    }

    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Ok("<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mi>x</mi></math>"
            .into())
    }
}

fn picture() -> Picture {
    Picture {
        png: PNG.to_vec(),
        svg: None,
        width_pt: 12.0,
        height_pt: 10.0,
        baseline_pt: 8.0,
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_paths() -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(root().join("docx/ir"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.push(root().join("ir/sample-document.json"));
    paths.sort();
    assert!(paths.len() >= 12, "keep T40/T41 fixtures and the sample IR");
    paths
}

fn fixture(name: &str) -> Document {
    serde_json::from_slice(
        &std::fs::read(root().join(format!("docx/ir/{name}.json"))).unwrap(),
    )
    .unwrap()
}

fn assets() -> AssetRegistry {
    let mut assets = AssetRegistry::new();
    assert_eq!(
        assets.register_bytes(PNG.to_vec(), "image/png", "plot.png"),
        "32debefb8003"
    );
    let dir = root().join("docx/images");
    if dir.exists() {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect();
        paths.sort();
        for path in paths {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            assets.register_bytes(
                std::fs::read(&path).unwrap(),
                mime.essence_str(),
                path.file_name().unwrap().to_str().unwrap(),
            );
        }
    }
    assets
}

fn emit(
    doc: &Document, config: &Config, reference: Option<&Path>,
) -> DocxOutput {
    let output = write(
        doc,
        &assets(),
        &mut FakeRenderer,
        &DocxOptions {
            config,
            reference,
            source_date_epoch: Some("0".into()),
            all_styles: false,
        },
    )
    .unwrap();
    let violations = validate(
        &output.bytes,
        &ValidateOptions { builtin_styles: reference.is_none() },
    );
    assert!(violations.is_empty(), "{violations:#?}");
    output
}

fn render(name: &str) -> DocxOutput {
    emit(&fixture(name), &Config::default(), None)
}

fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let mut part = archive.by_index(index).unwrap();
        let name = part.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        part.read_to_end(&mut bytes).unwrap();
        assert!(parts.insert(name, bytes).is_none());
    }
    parts
}

fn xml_parts(output: &DocxOutput) -> BTreeMap<String, String> {
    parts(&output.bytes)
        .into_iter()
        .filter(|(name, _)| name.ends_with(".xml") || name.ends_with(".rels"))
        .map(|(name, bytes)| (name, String::from_utf8(bytes).unwrap()))
        .collect()
}

fn pretty_xml(bytes: &[u8]) -> String {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut writer = quick_xml::Writer::new_with_indent(Vec::new(), b' ', 4);
    loop {
        let event = reader.read_event().unwrap();
        if matches!(event, Event::Eof) {
            break;
        }
        writer.write_event(event).unwrap();
    }
    String::from_utf8(writer.into_inner()).unwrap()
}

fn paragraph<'a>(xml: &'a str, text: &str) -> &'a str {
    xml.split("<w:p>")
        .skip(1)
        .map(|tail| tail.split("</w:p>").next().unwrap())
        .find(|part| part.contains(text))
        .unwrap_or_else(|| panic!("missing paragraph containing {text:?}"))
}

fn run<'a>(xml: &'a str, text: &str) -> &'a str {
    xml.split("<w:r>")
        .skip(1)
        .map(|tail| tail.split("</w:r>").next().unwrap())
        .find(|part| part.contains(text))
        .unwrap_or_else(|| panic!("missing run containing {text:?}"))
}

fn has_style(xml: &str, style: &str) -> bool {
    xml.contains(&format!("<w:pStyle w:val=\"{style}\"/>",))
}

#[test]
fn resolved_ir_packages() {
    for path in fixture_paths() {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let doc: Document =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        if matches!(name, "figures" | "tables" | "toc" | "bibliography") {
            let violations = doc.invariant_violations();
            assert!(violations.is_empty(), "{name}: {violations:#?}");
        }
        let output = emit(&doc, &Config::default(), None);
        for (part, bytes) in parts(&output.bytes) {
            let label: String = part
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let snapshot = if part.ends_with(".xml") || part.ends_with(".rels")
            {
                pretty_xml(&bytes)
            } else {
                format!(
                    "{} bytes\nBLAKE3 {}",
                    bytes.len(),
                    blake3::hash(&bytes).to_hex()
                )
            };
            insta::assert_snapshot!(format!("{name}__{label}"), snapshot);
        }
        insta::assert_json_snapshot!(
            format!("{name}__readback"),
            common_docx::read_back(&output.bytes)
        );
        insta::assert_json_snapshot!(
            format!("{name}__diagnostics"),
            output.diagnostics
        );
    }
}

#[test]
fn paragraph_context_and_title_metadata() {
    let output = render("blocks");
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    for (text, style) in [
        ("Plain *title*", "Title"),
        ("A subtitle", "Subtitle"),
        ("A. Author", "Author"),
        ("B. Author", "Author"),
        ("October 2026", "Date"),
        ("Summary", "AbstractTitle"),
        ("Abstract body", "Abstract"),
        ("First body paragraph", "FirstParagraph"),
        ("Second body paragraph", "BodyText"),
        ("Quoted paragraph", "BlockText"),
        ("Custom paragraph", "LocalParagraph"),
        ("After quote", "FirstParagraph"),
        ("Listed heading", "Heading1"),
        ("Not listed", "Heading2Unlisted"),
        ("Abstract heading", "Heading3Unlisted"),
    ] {
        assert!(has_style(paragraph(xml, text), style), "{text}: {style}");
    }
    let author = paragraph(xml, "A. Author");
    assert!(author.contains("Institute"));
    assert_eq!(author.matches("<w:br/>").count(), 2);
    assert!(author.contains("a@example.com"));
    assert!(
        parts["word/_rels/document.xml.rels"]
            .contains("Target=\"mailto:a@example.com\"")
    );
    let heading = paragraph(xml, "Listed heading");
    assert!(heading.contains("<w:rStyle w:val=\"SectionNumber\"/>"));
    assert!(heading.contains("<w:tab/>"));
    assert!(heading.contains(&format!("w:name=\"{}\"", anchor("listed"))));
    assert!(xml.contains("<w:t xml:space=\"preserve\">\u{200b}</w:t>"));
    assert!(has_style(xml, "HorizontalRule"));
    assert!(xml.contains("<w:br w:type=\"page\"/>"));
}

#[test]
fn inline_flags_styles_and_text_boundaries() {
    let output = render("blocks");
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let both = run(xml, "bold italic");
    for flag in ["<w:b/>", "<w:bCs/>", "<w:i/>", "<w:iCs/>"] {
        assert!(both.contains(flag), "{flag}");
    }
    for (text, flag) in [
        ("strikeout", "<w:strike/>"),
        ("underline", "<w:u w:val=\"single\"/>"),
        ("small_caps", "<w:smallCaps/>"),
        ("subscript", "<w:vertAlign w:val=\"subscript\"/>"),
        ("superscript", "<w:vertAlign w:val=\"superscript\"/>"),
        ("literal &lt;tag&gt;", "<w:rStyle w:val=\"VerbatimChar\"/>"),
        ("custom character", "<w:rStyle w:val=\"AccentText\"/>"),
        ("external link", "<w:rStyle w:val=\"Hyperlink\"/>"),
    ] {
        assert!(run(xml, text).contains(flag), "{text}: {flag}");
    }
    assert!(
        xml.contains("<w:t xml:space=\"preserve\">Adjacent text runs</w:t>")
    );
    assert!(xml.contains("<w:tab/>"));
    assert!(xml.contains("b c "));
    assert!(xml.contains("INLINE XML"));
    assert!(xml.contains("RAW BLOCK"));
    assert!(!xml.contains("FOREIGN INLINE"));
    assert!(!xml.contains("FOREIGN BLOCK"));
    assert!(
        parts["word/_rels/document.xml.rels"]
            .contains("Target=\"https://example.com/?a=1&amp;b=2\"")
    );
}

#[test]
fn list_markers_continuations_and_definitions() {
    let output = render("lists");
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let first = paragraph(xml, "Ordered first");
    assert!(first.contains("<w:ilvl w:val=\"0\"/>"));
    assert!(first.contains("<w:numId w:val=\"1001\"/>"));
    assert!(has_style(first, "FirstParagraph"));
    let continuation = paragraph(xml, "Ordered continuation");
    assert!(continuation.contains("<w:ind w:left=\"720\" w:firstLine=\"0\"/>"));
    assert!(!continuation.contains("<w:numPr>"));
    let nested = paragraph(xml, "Nested bullet");
    assert!(nested.contains("<w:ilvl w:val=\"1\"/>"));
    assert!(has_style(nested, "Compact"));
    for (text, marker) in [("Checked task", '☒'), ("Unchecked task", '☐')] {
        let task = paragraph(xml, text);
        assert!(task.contains(marker));
        assert!(task.contains("w:ascii=\"Segoe UI Symbol\""));
        assert!(task.contains("<w:tab/>"));
        assert!(task.contains("<w:ind w:left=\"720\" w:hanging=\"360\"/>"));
        assert!(!task.contains("<w:numPr>"));
    }
    for (text, style) in [
        ("Tight first", "Compact"),
        ("Tight second", "Compact"),
        ("A term", "DefinitionTerm"),
        ("First definition", "Definition"),
        ("Definition continuation", "Definition"),
        ("Alternative definition", "Definition"),
    ] {
        assert!(has_style(paragraph(xml, text), style), "{text}: {style}");
    }
    let numbering = &parts["word/numbering.xml"];
    assert_eq!(numbering.matches("<w:startOverride w:val=\"7\"/>").count(), 9);
    assert_eq!(numbering.matches("<w:startOverride w:val=\"2\"/>").count(), 9);
}

#[test]
fn code_highlighting_numbers_and_plain_output() {
    let output = render("code");
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let highlighted = paragraph(xml, "a comment");
    assert!(has_style(highlighted, "SourceCode"));
    assert_eq!(highlighted.matches("<w:br/>").count(), 2);
    assert_eq!(highlighted.matches("w:val=\"CodeLineNumber\"").count(), 3);
    assert!(highlighted.contains("<w:t xml:space=\"preserve\"> 8 </w:t>"));
    assert!(highlighted.contains("w:val=\"CommentTok\""));
    assert!(highlighted.contains("w:fill=\"FFF3BF\""));
    for text in
        ["plain &lt;&amp;&gt;", "unrecognised", "never highlight output"]
    {
        let plain = paragraph(xml, text);
        assert!(plain.contains("w:val=\"VerbatimChar\""), "{text}");
        assert!(!plain.contains("Tok\""), "{text}");
        assert!(!plain.contains("ansi="), "{text}");
    }
    assert!(xml.contains("w:val=\"DataTypeTok\""));
    assert!(xml.contains(&format!("w:name=\"{}\"", anchor("code_listing"))));
}

#[test]
fn callout_localization_and_style_precedence() {
    let output = render("callouts");
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    for (title, kind) in [
        ("Note", "Note"),
        ("Tip", "Tip"),
        ("Important", "Important"),
        ("Warning", "Warning"),
        ("Caution", "Caution"),
        ("Aside", "Aside"),
    ] {
        let title = paragraph(xml, title);
        assert!(has_style(title, &format!("Callout{kind}")));
        assert!(title.contains("<w:keepNext/>"));
        assert!(title.contains("w:val=\"CalloutTitle\""));
        let body = paragraph(xml, &format!("{} body", kind.to_lowercase()));
        assert!(has_style(body, &format!("Callout{kind}")));
    }
    let tight = paragraph(xml, "Tight inside callout");
    assert!(has_style(tight, "CalloutNote"));
    assert!(!has_style(tight, "Compact"));
    assert!(has_style(paragraph(xml, "print"), "SourceCode"));
    let styles = &parts["word/styles.xml"];
    assert!(styles.contains("<w:name w:val=\"Callout Aside\"/>"));
    let custom = styles.split("w:styleId=\"CalloutAside\"").nth(1).unwrap();
    assert!(
        custom
            .split("</w:style>")
            .next()
            .unwrap()
            .contains("<w:basedOn w:val=\"Callout\"/>")
    );
}

#[test]
fn nested_notes_are_hoisted_and_repeated_notes_use_fields() {
    let output = render("footnotes");
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let notes = &parts["word/footnotes.xml"];
    let refs: Vec<_> = xml
        .split("<w:footnoteReference w:id=\"")
        .skip(1)
        .map(|part| part.split('"').next().unwrap())
        .collect();
    assert_eq!(refs, ["1", "2", "3", "4"]);
    assert!(!notes.contains("<w:footnoteReference"));
    assert_eq!(notes.matches("<w:footnoteRef/>").count(), 4);
    assert!(notes.contains("<w:footnote w:type=\"separator\" w:id=\"-1\">"));
    assert!(
        notes.contains(
            "<w:footnote w:type=\"continuationSeparator\" w:id=\"0\">"
        )
    );
    // Markdown names bind per file and use an internal bookmark.
    let parent = "_Fn000001";
    assert!(notes.contains(&format!("NOTEREF {parent} \\f \\h")));
    assert!(xml.contains(&format!("NOTEREF {parent} \\f \\h")));
    assert!(xml.contains("NOTEREF _Fn000002 \\f \\h"));
    assert!(has_style(paragraph(notes, "Quoted note"), "FootnoteBlockText"));
    assert!(has_style(paragraph(notes, "Note after code"), "FootnoteText"));
    let third = notes.split("<w:footnote w:id=\"3\">").nth(1).unwrap();
    let first = third.split("</w:p>").next().unwrap();
    assert!(has_style(first, "FootnoteText"));
    assert!(first.contains("<w:footnoteRef/>"));
    assert!(!first.contains("a nonparagraph first"));
    assert!(
        parts["word/_rels/footnotes.xml.rels"]
            .contains("Target=\"https://nested.example/\"")
    );
    assert!(
        !parts["word/_rels/document.xml.rels"]
            .contains("https://nested.example/")
    );
}

#[test]
fn anonymous_note_bookmarks_do_not_collide_with_user_labels() {
    let mut doc = fixture("footnotes");
    doc.blocks.insert(0, serde_json::from_value(json!({
        "type": "paragraph", "inlines": [{
            "type": "span", "attrs": {"id": "Fn000002"},
            "inlines": [{"type": "text", "text": "Reserved-looking user label"}]
        }]
    })).unwrap());
    let output = emit(&doc, &Config::default(), None);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    assert!(xml.contains("w:name=\"_Fn000002\""));
    assert!(xml.contains("w:name=\"_Fn000002x\""));
    assert!(xml.contains("NOTEREF _Fn000002x \\f \\h"));
}

#[test]
fn first_logical_label_owner_wins_across_document_and_footnote_parts() {
    let mut doc = fixture("blocks");
    doc.meta = Default::default();
    doc.blocks = serde_json::from_value(json!([
        {"type": "paragraph", "inlines": [
            {"type": "link", "url": "#shared", "attrs": {},
             "inlines": [{"type": "text", "text": "Forward to note"}]},
            {"type": "footnote", "number": 1, "blocks": [
                {"type": "div", "attrs": {"id": "shared"}, "blocks": [
                    {"type": "paragraph", "inlines": [
                        {"type": "text", "text": "First logical label"}
                    ]}
                ]}
            ]}
        ]},
        {"type": "heading", "level": 1, "attrs": {"id": "shared"},
         "inlines": [{"type": "text", "text": "Later main heading"}]}
    ]))
    .unwrap();
    let output = emit(&doc, &Config::default(), None);
    let parts = xml_parts(&output);
    let name = format!("w:name=\"{}\"", anchor("shared"));
    assert!(!parts["word/document.xml"].contains(&name));
    assert_eq!(parts["word/footnotes.xml"].matches(&name).count(), 1);
    assert!(
        parts["word/document.xml"]
            .contains(&format!("w:anchor=\"{}\"", anchor("shared")))
    );
}

#[test]
fn footnote_labels_follow_the_nearest_source_scope() {
    // An explicit span without `file` resets an inherited include to main.
    for (kind, outer, inner, native) in [
        ("markdown", None, None, false),
        ("typst", None, None, true),
        ("markdown", Some("/included.typ"), None, true),
        ("typst", Some("/included.md"), None, false),
        ("typst", Some("/included.unknown"), None, false),
        ("markdown", Some("/included.typ"), Some(None), false),
        ("typst", Some("/included.md"), Some(None), true),
        ("markdown", Some("/included.md"), Some(Some("/nearest.typ")), true),
    ] {
        let mut doc = fixture("blocks");
        doc.meta = Default::default();
        doc.source.kind = serde_json::from_value(json!(kind)).unwrap();
        let span = |file: Option<&str>| {
            json!({
                "start_line": 1, "end_line": 1, "file": file
            })
        };
        doc.blocks = serde_json::from_value(json!([
            {"type": "div", "attrs": {}, "span": outer.map(|file| span(Some(file))), "blocks": [
                {"type": "paragraph", "span": inner.map(span), "inlines": [
                    {"type": "footnote", "label": "source_note", "number": 1,
                     "blocks": [{"type": "paragraph", "inlines": [
                        {"type": "text", "text": "Scoped note"}
                     ]}]},
                    {"type": "footnote_ref", "label": "source_note", "number": 1}
                ]}
            ]}
        ])).unwrap();
        let output = emit(&doc, &Config::default(), None);
        let parts = xml_parts(&output);
        let xml = &parts["word/document.xml"];
        let expected = if native { "_source_note" } else { "_Fn000001" };
        let absent = if native { "_Fn000001" } else { "_source_note" };
        assert!(
            xml.contains(&format!("w:name=\"{expected}\"")),
            "{kind}, {outer:?}, {inner:?}"
        );
        assert!(!xml.contains(&format!("w:name=\"{absent}\"")));
        assert!(xml.contains(&format!("NOTEREF {expected} \\f \\h")));
    }
}

#[test]
fn later_user_label_reserves_an_anonymous_note_bookmark_name() {
    let mut doc = fixture("blocks");
    doc.meta = Default::default();
    doc.blocks = serde_json::from_value(json!([
        {"type": "paragraph", "inlines": [
            {"type": "footnote", "number": 1, "blocks": [
                {"type": "paragraph", "inlines": [
                    {"type": "text", "text": "Anonymous note"}
                ]}
            ]},
            {"type": "footnote_ref", "label": "", "number": 1}
        ]},
        {"type": "heading", "level": 1, "attrs": {"id": "Fn000001"},
         "inlines": [{"type": "text", "text": "Later reserved-looking heading"}]}
    ])).unwrap();
    let output = emit(&doc, &Config::default(), None);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    assert!(xml.contains("w:name=\"_Fn000001x\""));
    assert!(xml.contains("NOTEREF _Fn000001x \\f \\h"));
    let heading = paragraph(xml, "Later reserved-looking heading");
    assert!(heading.contains("w:name=\"_Fn000001\""));
    assert!(!heading.contains("w:name=\"_Fn000001x\""));
}

#[test]
fn forward_internal_page_and_missing_references() {
    let output = render("refs");
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    let target = anchor("target");
    assert_eq!(xml.matches(&format!("w:anchor=\"{target}\"")).count(), 5);
    assert_eq!(xml.matches(&format!("PAGEREF {target} \\h")).count(), 3);
    assert!(xml.contains("page\u{a0}"));
    assert!(xml.contains("Leaf"));
    assert!(!xml.contains("cached page text"));
    assert!(!xml.contains("cached explicit page"));
    assert!(!xml.contains("cached silent page"));
    assert!(xml.contains("Missing page fallback"));
    assert!(xml.contains("[?absent]"));
    assert!(xml.contains("[?a; b]"));
    assert!(run(xml, "(Author 2026)").contains("<w:b/>"));
    assert!(!run(xml, "Section 1").contains("Hyperlink"));
    assert_eq!(
        xml.matches(&format!("w:name=\"{}\"", anchor("duplicate"))).count(),
        1
    );
    assert!(!xml.contains(&format!("w:name=\"{}\"", anchor("hidden_label"))));
    assert!(!xml.contains(&format!("w:anchor=\"{}\"", anchor("hidden_label"))));
    assert!(xml.contains("Hidden reference"));
    assert!(xml.contains("Hidden link"));
    assert!(xml.contains("Selected candidate"));
    let unknown: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|d| d.code == "docx-unknown-anchor")
        .collect();
    assert_eq!(unknown.len(), 3, "{unknown:#?}");
}

#[test]
fn page_reference_plans_only_the_emitted_supplement() {
    for target_present in [true, false] {
        let mut doc = fixture("blocks");
        doc.meta = Default::default();
        let supplement = json!([
            {"type": "span", "attrs": {"id": "page_supplement"}, "inlines": [
                {"type": "text", "text": "Custom page"}
            ]},
            {"type": "footnote", "number": 1, "blocks": [
                {"type": "paragraph", "inlines": [
                    {"type": "text", "text": "Supplement note"}
                ]}
            ]}
        ]);
        let mut cached = supplement.as_array().unwrap().clone();
        cached.push(json!({"type": "text", "text": "UNUSED CACHED SUFFIX"}));
        doc.blocks = serde_json::from_value(json!([
        {"type": "paragraph", "inlines": [{
            "type": "ref", "target": "page_target", "form": "page",
            "supplement": supplement, "resolved": {
                "kind": "heading", "number": "1", "anchor": "page_target",
                "text": cached
            }
        }]},
        {"type": "heading", "level": 1, "attrs": {"id": if target_present { "page_target" } else { "unrelated" }},
         "inlines": [{"type": "text", "text": "Page target"}]},
        {"type": "paragraph", "inlines": [
            {"type": "footnote", "number": 2, "blocks": [
                {"type": "paragraph", "inlines": [
                    {"type": "text", "text": "Following note"}
                ]}
            ]},
            {"type": "footnote_ref", "label": "", "number": 1},
            {"type": "footnote_ref", "label": "", "number": 2}
        ]}
    ]))
    .unwrap();
        let output = emit(&doc, &Config::default(), None);
        let parts = xml_parts(&output);
        let xml = &parts["word/document.xml"];
        let notes = &parts["word/footnotes.xml"];
        assert_eq!(xml.matches("<w:footnoteReference").count(), 2);
        assert!(xml.contains("<w:footnoteReference w:id=\"1\"/>"));
        assert!(xml.contains("<w:footnoteReference w:id=\"2\"/>"));
        assert_eq!(notes.matches("Supplement note").count(), 1);
        assert_eq!(notes.matches("Following note").count(), 1);
        assert!(!notes.contains("<w:footnote w:id=\"3\">"));
        assert_eq!(xml.contains("UNUSED CACHED SUFFIX"), !target_present);
        assert_eq!(xml.matches("w:name=\"_page_supplement\"").count(), 1);
        assert!(xml.contains("NOTEREF _Fn000001 \\f \\h"));
        assert!(xml.contains("NOTEREF _Fn000002 \\f \\h"));
    }
}

#[test]
fn notebook_visibility_prompts_and_candidate_selection() {
    for prompts in [true, false] {
        let mut config = Config::default();
        config.notebook.show_prompts = prompts;
        let output = emit(&fixture("cells"), &config, None);
        let parts = xml_parts(&output);
        let xml = &parts["word/document.xml"];
        assert_eq!(xml.contains("In [7]:"), prompts);
        assert_eq!(xml.contains("Out [7]:"), prompts);
        assert_eq!(xml.contains("In [ ]:"), prompts);
        assert_eq!(xml.contains("In [9]:"), prompts);
        assert!(!xml.contains("Out [9]:"));
        assert!(xml.contains("Chosen plain output"));
        assert!(xml.contains("Visible output"));
        for hidden in [
            "HIDDEN INPUT", "HIDDEN OUTPUT", "REMOVED INPUT", "REMOVED OUTPUT",
            "HTML CANDIDATE",
        ] {
            assert!(!xml.contains(hidden), "{hidden}");
        }
        assert!(has_style(paragraph(xml, "Chosen plain output"), "CellOutput"));
        assert!(has_style(paragraph(xml, "visible_input"), "SourceCode"));
        assert!(
            xml.contains(&format!("w:name=\"{}\"", anchor("visible_cell")))
        );
    }
}

#[test]
fn raw_tables_get_separating_and_trailing_paragraphs() {
    for name in ["blocks", "footnotes"] {
        let output = render(name);
        let parts = xml_parts(&output);
        let part = if name == "blocks" {
            "word/document.xml"
        } else {
            "word/footnotes.xml"
        };
        let xml = &parts[part];
        assert!(xml.contains("</w:tbl><w:p/><w:tbl>"));
        let ending = if name == "blocks" {
            "<w:sectPr>"
        } else {
            "</w:footnote>"
        };
        assert!(xml.contains(&format!("</w:tbl><w:p/>{ending}")));
    }
}

#[test]
fn adjacent_code_and_equal_nested_flags_merge_without_losing_boundaries() {
    let mut doc = fixture("blocks");
    doc.meta = Default::default();
    doc.blocks = vec![serde_json::from_value(json!({
        "type": "paragraph", "inlines": [
            {"type": "code", "text": "first", "attrs": {}},
            {"type": "code", "text": "second", "attrs": {}},
            {"type": "strong", "inlines": [
                {"type": "text", "text": "A"},
                {"type": "strong", "inlines": [{"type": "text", "text": "B"}]}
            ]},
            {"type": "span", "attrs": {"pairs": {"custom-style": "Outer"}},
             "inlines": [
                {"type": "strong", "inlines": [
                    {"type": "code", "text": "inner code", "attrs": {}},
                    {"type": "link", "url": "https://example.com", "attrs": {},
                     "inlines": [{"type": "text", "text": "inner link"}]}
                ]}
            ]}
        ]
    })).unwrap()];
    let output = emit(&doc, &Config::default(), None);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    assert!(xml.contains("<w:t xml:space=\"preserve\">firstsecond</w:t>"));
    assert!(xml.contains("<w:t xml:space=\"preserve\">AB</w:t>"));
    let code = run(xml, "inner code");
    assert!(code.contains("w:val=\"VerbatimChar\""));
    assert!(code.contains("<w:b/>"));
    assert!(!code.contains("w:val=\"Outer\""));
    let link = run(xml, "inner link");
    assert!(link.contains("w:val=\"Hyperlink\""));
    assert!(link.contains("<w:b/>"));
    assert!(!link.contains("w:val=\"Outer\""));
}

#[test]
fn invalid_raw_xml_is_dropped_without_corrupting_the_package() {
    let mut doc = fixture("blocks");
    let invalid = [
        "<w:p>", "<w:r bad=\"a\" bad=\"b\"/>", "<unknown:p/>",
        "<w:r><w:t>&bogus;</w:t></w:r>",
    ];
    doc.blocks = invalid
        .iter()
        .map(|text| {
            serde_json::from_value(json!({
                "type": "raw", "format": "openxml", "text": text
            }))
            .unwrap()
        })
        .collect();
    doc.blocks.push(
        serde_json::from_value(json!({
            "type": "paragraph", "inlines": invalid.iter().map(|text| json!({
                "type": "raw", "format": "openxml", "text": text
            })).collect::<Vec<_>>()
        }))
        .unwrap(),
    );
    let output = emit(&doc, &Config::default(), None);
    let invalid_count = output
        .diagnostics
        .iter()
        .filter(|d| d.code == "docx-raw-invalid")
        .count();
    assert_eq!(invalid_count, invalid.len() * 2);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    assert!(!xml.contains("&bogus;"));
    assert!(!xml.contains("unknown:p"));
    assert!(!xml.contains("bad="));
}

#[test]
fn list_depth_is_capped_with_a_diagnostic() {
    let mut doc = fixture("lists");
    let mut block = json!({
        "type": "paragraph", "inlines": [{"type": "text", "text": "Deep"}]
    });
    for _ in 0..11 {
        block = json!({
            "type": "list", "kind": {"kind": "bullet"}, "tight": true,
            "items": [{"blocks": [block]}]
        });
    }
    doc.blocks = vec![serde_json::from_value::<Block>(block).unwrap()];
    let output = emit(&doc, &Config::default(), None);
    let parts = xml_parts(&output);
    let xml = &parts["word/document.xml"];
    assert!(xml.contains("<w:ilvl w:val=\"8\"/>"));
    assert!(!xml.contains("<w:ilvl w:val=\"9\"/>"));
    assert!(
        output.diagnostics.iter().any(|d| d.code == "unsupported-construct")
    );
}

#[test]
fn packages_are_deterministic_with_and_without_reference_reuse() {
    let reference = root().join("docx/reference.docx");
    for reference in [None, Some(reference.as_path())] {
        for path in fixture_paths() {
            let doc =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            let config = Config::default();
            let first = emit(&doc, &config, reference);
            let second = emit(&doc, &config, reference);
            assert_eq!(first.bytes, second.bytes);
            assert_eq!(first.diagnostics, second.diagnostics);
            let counts = common_docx::read_back(&first.bytes);
            assert!(counts.paragraphs > 0);
            assert!(counts.styles > 0);
            assert!(counts.abstract_nums >= 2);
        }
    }
}

#[test]
fn rich_content_prepass_matches_emission_across_parts_and_candidates() {
    let mut doc = fixture("blocks");
    doc.meta = Default::default();
    let text = |value: &str| json!({"type": "text", "text": value});
    let note = |number: u32, label: &str| {
        json!({
            "type": "footnote", "number": number, "blocks": [{
                "type": "paragraph", "inlines": [
                    {"type": "span", "attrs": {"id": label}, "inlines": [text(label)]},
                    {"type": "image", "url": "asset:32debefb8003", "attrs": {}, "alt": [text("Note plot")]},
                    {"type": "link", "url": "https://note.example/", "attrs": {}, "inlines": [text("source")]}
                ]
            }]
        })
    };
    let heading = |label: &str| {
        json!({
            "type": "heading", "level": 1, "attrs": {"id": label},
            "inlines": [text(label)]
        })
    };
    doc.blocks = serde_json::from_value(json!([
        {"type": "paragraph", "inlines": [
            {"type": "link", "url": "#caption-note", "attrs": {}, "inlines": [text("Forward caption")]},
            {"type": "link", "url": "#table-note", "attrs": {}, "inlines": [text("Forward table")]},
            {"type": "link", "url": "#bibliography-note", "attrs": {}, "inlines": [text("Forward bibliography")]},
            {"type": "link", "url": "#refs", "attrs": {}, "inlines": [text("References")]}
        ]},
        {"type": "toc", "depth": 2},
        {"type": "figure", "kind": "other", "attrs": {"id": "rich-figure"},
            "body": [{"type": "paragraph", "inlines": [text("Figure body"), note(1, "body-note")]}],
            "caption": [text("Rich caption"), note(2, "caption-note")]},
        {"type": "table", "columns": [{}], "head": [], "foot": [], "attrs": {},
            "body": [{"cells": [{"inlines": [text("Rich cell"), note(3, "table-note")]}]}]},
        {"type": "cell", "index": 0, "show_input": false, "show_output": true,
            "outputs": [{"kind": {"kind": "display"}, "candidates": [
                {"mime": "text/html", "blocks": [
                    {"type": "raw", "format": "html", "text": "<p>Unpicked</p>"},
                    heading("unpicked-heading"),
                    {"type": "paragraph", "inlines": [note(5, "unpicked-note")]}]},
                {"mime": "text/plain", "blocks": [heading("visible-heading")]}
            ], "mimes": ["text/html", "text/plain"]}]},
        {"type": "bibliography"}
    ])).unwrap();
    doc.resolved.toc = serde_json::from_value(json!([
        {"level": 1, "number": null, "anchor": "visible-heading", "inlines": [text("Visible heading")]},
        {"level": 1, "number": null, "anchor": "unpicked-heading", "inlines": [text("Hidden cache")]},
        {"level": 1, "number": null, "anchor": "refs", "inlines": [text("References")]}
    ])).unwrap();
    doc.resolved.bibliography = Some(
        serde_json::from_value(json!({
            "style": "test", "hanging_indent": false,
            "entries": [{"key": "entry", "prefix": [text("[1]")],
                "inlines": [text("Rich entry"), note(4, "bibliography-note")]}]
        }))
        .unwrap(),
    );
    let output = emit(&doc, &Config::default(), None);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let package = parts(&output.bytes);
    let xml = String::from_utf8_lossy(&package["word/document.xml"]);
    let notes = String::from_utf8_lossy(&package["word/footnotes.xml"]);
    for (id, label) in [
        (1, "body-note"),
        (2, "caption-note"),
        (3, "table-note"),
        (4, "bibliography-note"),
    ] {
        assert_eq!(
            xml.matches(&format!("<w:footnoteReference w:id=\"{id}\"/>"))
                .count(),
            1
        );
        assert_eq!(
            notes.matches(&format!("w:name=\"{}\"", anchor(label))).count(),
            1
        );
    }
    for label in ["caption-note", "table-note", "bibliography-note", "refs"] {
        assert!(
            xml.contains(&format!("w:anchor=\"{}\"", anchor(label))),
            "{label}"
        );
    }
    assert!(!xml.contains("unpicked-note"));
    assert!(!notes.contains("unpicked-note"));
    assert!(!xml.contains(&format!("PAGEREF {}", anchor("unpicked-heading"))));
    assert!(xml.contains("Hidden cache"));
    assert!(xml.contains(&format!("PAGEREF {}", anchor("visible-heading"))));
    assert!(
        String::from_utf8_lossy(&package["word/_rels/footnotes.xml.rels"])
            .contains("https://note.example/")
    );
    assert_eq!(
        package.keys().filter(|name| name.starts_with("word/media/")).count(),
        1
    );
    let drawing_ids: Vec<_> = notes
        .split("<wp:docPr id=\"")
        .skip(1)
        .map(|tail| tail.split('"').next().unwrap())
        .collect();
    assert_eq!(drawing_ids, ["1", "2", "3", "4"]);
}
