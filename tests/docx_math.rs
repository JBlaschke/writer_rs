//! Editable DOCX math, independent renderer routes and recovery placement.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use regex::Regex;
use serde_json::{Value, json};
use writer::assets::registry::AssetRegistry;
use writer::config::{
    Config, DocxMath, FigureNumbering, HeadingNumbering, Target,
};
use writer::docx::{
    DocxOptions, DocxOutput, Picture, Renderer, ValidateOptions, validate,
    write,
};
use writer::error::{Error, Level, Result};
use writer::ir::{Document, MathDialect};
use writer::math::latex_source::{for_typst, macros_from_config};
use writer::pipeline::{self, BuildOptions, WorldSlot};
use writer::resolve::anchor;

#[path = "common/docx.rs"]
mod common_docx;
#[path = "common/math_xml.rs"]
mod math_xml;
use math_xml::{Node, parse};

const GOOD: &str = "<math><mfrac><mi>x</mi><mn>2</mn></mfrac></math>";
const UNSUPPORTED: &str =
    "<math><merror><mtext>visible fallback</mtext></merror></math>";
const PNG: &[u8] = include_bytes!("fixtures/md/images/plot.png");
const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10"/></svg>"#;

#[derive(Debug, PartialEq)]
enum Call {
    Mathml(String, bool),
    Equation(String, MathDialect, bool),
}

struct Fake {
    calls: Vec<Call>,
    mathml: std::result::Result<&'static str, &'static str>,
    picture_error: Option<&'static str>,
    echo: bool,
}

impl Default for Fake {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            mathml: Ok(GOOD),
            picture_error: None,
            echo: false,
        }
    }
}

impl Renderer for Fake {
    fn equation(
        &mut self, source: &str, dialect: MathDialect, display: bool,
    ) -> Result<Picture> {
        self.calls.push(Call::Equation(source.into(), dialect, display));
        if let Some(error) = self.picture_error {
            return Err(Error::Other(error.into()));
        }
        Ok(Picture {
            png: PNG.to_vec(),
            svg: Some(SVG.to_vec()),
            width_pt: 20.0,
            height_pt: 10.0,
            baseline_pt: 7.25,
        })
    }

    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        panic!("math must use Renderer::equation, not Renderer::image")
    }

    fn typst_mathml(&mut self, source: &str, display: bool) -> Result<String> {
        self.calls.push(Call::Mathml(source.into(), display));
        if self.echo {
            return Ok(format!("<math><mi>{source}</mi></math>"));
        }
        self.mathml
            .map(str::to_string)
            .map_err(|error| Error::Other(error.into()))
    }
}

fn text(value: &str) -> Value {
    json!({"type":"text", "text":value})
}

fn inline(source: &str, dialect: &str) -> Value {
    json!({"type":"math", "source":source, "dialect":dialect})
}

fn paragraph(inlines: Vec<Value>) -> Value {
    json!({"type":"paragraph", "inlines":inlines})
}

fn display(
    source: &str, dialect: &str, label: Option<&str>, number: Option<&str>,
) -> Value {
    json!({"type":"math", "source":source, "dialect":dialect,
        "attrs":{"id":label}, "number":number})
}

fn document(blocks: Vec<Value>) -> Document {
    serde_json::from_value(json!({
        "meta":{}, "blocks":blocks, "features":{"math":true}, "assets":[],
        "source":{"path":"/source/main.md", "base_dir":"/source", "kind":"markdown"}
    })).unwrap()
}

fn emit(
    doc: &Document, config: &Config, renderer: &mut Fake,
    reference: Option<&Path>,
) -> DocxOutput {
    let output = write(
        doc,
        &AssetRegistry::new(),
        renderer,
        &DocxOptions {
            config,
            reference,
            source_date_epoch: Some("0".into()),
            all_styles: false,
        },
    )
    .unwrap();
    assert_eq!(
        validate(
            &output.bytes,
            &ValidateOptions { builtin_styles: reference.is_none() }
        ),
        []
    );
    common_docx::read_back(&output.bytes);
    output
}

fn package(output: &DocxOutput) -> BTreeMap<String, Vec<u8>> {
    math_xml::zip_parts(&output.bytes)
}

fn document_tree(output: &DocxOutput) -> Node {
    parse(&math_xml::part(&package(output), "word/document.xml"))
}

fn style_id(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    let styles = parse(&math_xml::part(parts, "word/styles.xml"));
    styles
        .children
        .iter()
        .find(|node| {
            node.name == "w:style"
                && node.child("w:name").and_then(|n| n.attr("w:val"))
                    == Some(name)
        })
        .unwrap_or_else(|| panic!("missing style {name}"))
        .attr("w:styleId")
        .unwrap()
        .to_string()
}

fn paragraph_style(node: &Node) -> Option<&str> {
    node.child("w:pPr")
        .and_then(|n| n.child("w:pStyle"))
        .and_then(|n| n.attr("w:val"))
}

fn warning<'a>(
    output: &'a DocxOutput, code: &str, count: usize,
) -> Vec<&'a writer::error::Diagnostic> {
    let found: Vec<_> =
        output.diagnostics.iter().filter(|d| d.code == code).collect();
    assert_eq!(found.len(), count, "{:?}", output.diagnostics);
    assert!(found.iter().all(|d| d.level == Level::Warning));
    found
}

#[test]
fn both_dialects_are_editable_and_only_native_typst_calls_the_renderer() {
    let doc = document(vec![
        paragraph(vec![
            text("before "),
            inline(r"\frac{a}{b}", "latex"),
            text(" after"),
        ]),
        paragraph(vec![inline("#text[x] + y", "typst")]),
        display("x^2", "latex", None, None),
        display("sqrt(x)", "typst", None, None),
    ]);
    let mut renderer = Fake::default();
    let output = emit(&doc, &Config::default(), &mut renderer, None);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        renderer.calls,
        [
            Call::Mathml("#text[x] + y".into(), false),
            Call::Mathml("sqrt(x)".into(), true)
        ]
    );
    let tree = document_tree(&output);
    assert_eq!(tree.descendants("m:oMath").len(), 4);
    assert_eq!(tree.descendants("m:oMathPara").len(), 2);
    assert!(tree.descendants("wp:inline").is_empty());
    let first = tree
        .descendants("w:p")
        .into_iter()
        .find(|p| p.text_content().contains("before"))
        .unwrap();
    assert_eq!(
        first.children.iter().filter(|n| n.name == "m:oMath").count(),
        1
    );
    assert_eq!(first.children.iter().filter(|n| n.name == "w:r").count(), 2);
    let alone = tree
        .descendants("w:p")
        .into_iter()
        .find(|p| {
            p.child("m:oMath").is_some() && !p.text_content().contains("before")
        })
        .unwrap();
    let trailing = alone.children.last().unwrap();
    assert_eq!(trailing.name, "w:r");
    assert_eq!(trailing.text_content(), "\u{200b}");
    assert_eq!(
        alone
            .descendants("w:t")
            .iter()
            .filter(|t| t.text == "\u{200b}")
            .count(),
        1
    );
    for equation in tree.descendants("m:oMathPara") {
        assert_eq!(
            equation
                .child("m:oMathParaPr")
                .unwrap()
                .child("m:jc")
                .unwrap()
                .attr("m:val"),
            Some("center")
        );
    }
}

#[test]
fn invalid_latex_recovers_as_source_without_any_rendition_attempt() {
    let source = r"\foo{x}";
    let doc = document(vec![
        paragraph(vec![inline(source, "latex")]),
        display(source, "latex", None, None),
    ]);
    let mut renderer = Fake::default();
    let output = emit(&doc, &Config::default(), &mut renderer, None);
    warning(&output, "math-syntax", 2);
    assert!(renderer.calls.is_empty());
    let parts = package(&output);
    let tree = document_tree(&output);
    assert!(tree.descendants("m:oMath").is_empty());
    assert!(tree.descendants("wp:inline").is_empty());
    let verbatim = style_id(&parts, "Verbatim Char");
    assert!(
        tree.descendants("w:rStyle")
            .iter()
            .any(|n| n.attr("w:val") == Some(verbatim.as_str()))
    );
    let source_code = style_id(&parts, "Source Code");
    let p = tree
        .descendants("w:p")
        .into_iter()
        .find(|p| paragraph_style(p) == Some(source_code.as_str()))
        .unwrap();
    assert_eq!(p.text_content(), source);
    assert_eq!(
        p.child("w:pPr").unwrap().child("w:jc").unwrap().attr("w:val"),
        Some("center")
    );
}

#[test]
fn native_mathml_failure_uses_source_inline_and_one_final_display_recovery() {
    for (displayed, render_fails) in
        [(false, false), (true, false), (true, true)]
    {
        let source = "native_missing(x)";
        let block = if displayed {
            display(source, "typst", None, None)
        } else {
            paragraph(vec![inline(source, "typst")])
        };
        let mut renderer = Fake {
            mathml: Err("native export failed"),
            picture_error: render_fails.then_some("rendition failed"),
            ..Fake::default()
        };
        let output = emit(
            &document(vec![block]),
            &Config::default(),
            &mut renderer,
            None,
        );
        let diagnostics = warning(&output, "math-conversion-failed", 1);
        assert!(diagnostics[0].message.contains("native export failed"));
        let tree = document_tree(&output);
        assert_eq!(
            tree.descendants("wp:inline").len(),
            usize::from(displayed && !render_fails)
        );
        assert_eq!(renderer.calls[0], Call::Mathml(source.into(), displayed));
        if displayed {
            assert_eq!(
                renderer.calls[1],
                Call::Equation(source.into(), MathDialect::Typst, true)
            );
        } else {
            assert_eq!(renderer.calls.len(), 1)
        }
        if render_fails {
            assert!(diagnostics[0].message.contains("rendition failed"));
        }
        if !displayed || render_fails {
            assert!(tree.descendants("w:t").iter().any(|n| n.text == source));
        }
    }
}

#[test]
fn unsupported_mathml_never_creates_default_inline_pictures() {
    for dialect in ["latex", "typst"] {
        for displayed in [false, true] {
            let source = if dialect == "latex" {
                r"\sideset{_1^2}{_3^4}\sum"
            } else {
                "native_unsupported(x)"
            };
            let block = if displayed {
                display(source, dialect, None, None)
            } else {
                paragraph(vec![inline(source, dialect)])
            };
            let mut renderer =
                Fake { mathml: Ok(UNSUPPORTED), ..Fake::default() };
            let output = emit(
                &document(vec![block]),
                &Config::default(),
                &mut renderer,
                None,
            );
            let diagnostics = warning(&output, "math-conversion-failed", 1);
            assert!(diagnostics[0].message.contains(if dialect == "latex" {
                "mmultiscripts"
            } else {
                "merror"
            }));
            let tree = document_tree(&output);
            assert_eq!(
                tree.descendants("wp:inline").len(),
                usize::from(displayed)
            );
            let pictures = renderer
                .calls
                .iter()
                .filter(|c| matches!(c, Call::Equation(..)))
                .count();
            assert_eq!(pictures, usize::from(displayed));
            if !displayed {
                assert!(
                    tree.descendants("w:rStyle").is_empty(),
                    "unsupported MathML is plain text, not source style"
                );
                assert!(!tree.descendants("w:t").is_empty());
            }
            if displayed {
                let expected = if dialect == "latex" {
                    for_typst(source, &BTreeMap::new()).0
                } else {
                    source.into()
                };
                let dialect = if dialect == "latex" {
                    MathDialect::Latex
                } else {
                    MathDialect::Typst
                };
                assert_eq!(
                    renderer.calls.last(),
                    Some(&Call::Equation(expected, dialect, true))
                );
            }
        }
    }
    let mut renderer = Fake {
        mathml: Ok(UNSUPPORTED),
        picture_error: Some("rendition failed"),
        ..Fake::default()
    };
    let source = "native_unsupported(x)";
    let output = emit(
        &document(vec![display(source, "typst", Some("eq-failed"), Some("1"))]),
        &Config::default(),
        &mut renderer,
        None,
    );
    let diagnostics = warning(&output, "math-conversion-failed", 1);
    assert!(
        diagnostics[0].message.contains("merror")
            && diagnostics[0].message.contains("rendition failed")
    );
    let tree = document_tree(&output);
    assert_eq!(tree.descendants("w:tbl").len(), 1);
    let middle = &tree.descendants("w:tr")[0]
        .children
        .iter()
        .filter(|n| n.name == "w:tc")
        .collect::<Vec<_>>()[1];
    let source_style = style_id(&package(&output), "Source Code");
    assert!(
        middle
            .descendants("w:p")
            .iter()
            .any(|p| paragraph_style(p) == Some(source_style.as_str())
                && p.text_content() == source)
    );
}

#[test]
fn forced_images_keep_prepared_sources_baselines_svg_png_and_note_relationships()
 {
    let mut config = Config::default();
    config.docx.math = DocxMath::Image;
    config
        .math
        .katex_options
        .insert("macros".into(), json!({"\\RR":"\\mathbb{R}"}));
    let source = r"\RR^2 \tag{7}";
    let mut doc = document(vec![paragraph(vec![
        inline(source, "latex"),
        json!({"type":"footnote","label":"note","number":1,
        "blocks":[paragraph(vec![inline("x","typst")]),display("y","typst",None,None)]}),
    ])]);
    doc.features.footnotes = true;
    let mut renderer = Fake::default();
    let output = emit(&doc, &config, &mut renderer, None);
    warning(&output, "math-tag-ignored", 1);
    assert_eq!(
        renderer.calls,
        [
            Call::Equation(
                for_typst(
                    source,
                    &macros_from_config(&config.math.katex_options)
                )
                .0,
                MathDialect::Latex,
                false
            ),
            Call::Equation("x".into(), MathDialect::Typst, false),
            Call::Equation("y".into(), MathDialect::Typst, true),
        ]
    );
    let parts = package(&output);
    let mut all_ids = Vec::new();
    for (name, rels) in [
        ("word/document.xml", "word/_rels/document.xml.rels"),
        ("word/footnotes.xml", "word/_rels/footnotes.xml.rels"),
    ] {
        let tree = parse(&math_xml::part(&parts, name));
        let relationships = parse(&math_xml::part(&parts, rels));
        let valid: Vec<_> = relationships
            .children
            .iter()
            .filter_map(|n| n.attr("Id"))
            .collect();
        for blip in tree
            .descendants("a:blip")
            .into_iter()
            .chain(tree.descendants("asvg:svgBlip"))
        {
            assert!(valid.contains(&blip.attr("r:embed").unwrap()));
        }
        for extent in tree.descendants("wp:extent") {
            assert_eq!(extent.attr("cx"), Some("254000"));
            assert_eq!(extent.attr("cy"), Some("127000"));
        }
        all_ids.extend(
            tree.descendants("wp:docPr")
                .iter()
                .map(|n| n.attr("id").unwrap().to_string()),
        );
    }
    let doc = parse(&math_xml::part(&parts, "word/document.xml"));
    assert_eq!(doc.descendants("w:position")[0].attr("w:val"), Some("-6"));
    assert_eq!(doc.descendants("wp:docPr")[0].attr("descr"), Some(source));
    let notes = parse(&math_xml::part(&parts, "word/footnotes.xml"));
    assert_eq!(
        notes.descendants("w:position").len(),
        1,
        "display picture must not be baseline lowered"
    );
    assert_eq!(
        all_ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
        all_ids.len()
    );
    assert!(
        parts
            .keys()
            .any(|name| name.starts_with("word/media/")
                && name.ends_with(".png"))
    );
    assert!(
        parts
            .keys()
            .any(|name| name.starts_with("word/media/")
                && name.ends_with(".svg"))
    );
}

#[test]
fn macro_diagnostics_are_once_only_and_inherit_the_first_equation_location() {
    let mut config = Config::default();
    config
        .math
        .katex_options
        .insert("macros".into(), json!({"invalid":"x","\\RR":"\\mathbb{R}"}));
    let mut first = display(r"\RR^2", "latex", None, None);
    first["span"] = json!({"start_line":7,"end_line":8,"cell":2,"file":"/source/included.ipynb"});
    let doc = document(vec![first, display(r"\RR^3", "latex", None, None)]);
    let output = emit(&doc, &config, &mut Fake::default(), None);
    let diagnostics = warning(&output, "math-macro-invalid", 1);
    assert_eq!(
        diagnostics[0].path.as_deref(),
        Some(Path::new("/source/included.ipynb"))
    );
    let span = diagnostics[0].span.as_ref().unwrap();
    assert_eq!((span.start_line, span.cell), (7, Some(2)));
    assert_eq!(document_tree(&output).descendants("m:oMath").len(), 2);
    config.docx.math = DocxMath::Image;
    let output = emit(&doc, &config, &mut Fake::default(), None);
    warning(&output, "math-macro-invalid", 0);
}

#[test]
fn numbered_frames_keep_forward_refs_number_bookmarks_and_seq_corrections() {
    let label = "eq-forward";
    let reference = json!({"type":"ref","target":label,"supplement":null,"form":"normal", "resolved":{
        "kind":"equation","number":"1","text":[text("Equation 1")],"anchor":label}});
    let mut page_reference = reference.clone();
    page_reference["form"] = json!("page");
    page_reference["supplement"] = json!([]);
    let doc = document(vec![
        paragraph(vec![reference, page_reference]),
        display("a", "typst", Some(label), Some("1")),
        display("b", "typst", Some("eq-two"), Some("2")),
        display("c", "typst", Some("eq-seven"), Some("7")),
        display("u", "typst", Some("eq-unnumbered"), None),
    ]);
    let output = emit(&doc, &Config::default(), &mut Fake::default(), None);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let parts = package(&output);
    let tree = document_tree(&output);
    let fields: Vec<_> = tree
        .descendants("w:instrText")
        .iter()
        .map(|n| n.text.trim().to_string())
        .collect();
    let bookmark_name = anchor(label);
    let links: Vec<_> = tree
        .descendants("w:hyperlink")
        .into_iter()
        .filter(|link| link.attr("w:anchor") == Some(bookmark_name.as_str()))
        .collect();
    assert_eq!(links.len(), 2, "both references target the later equation");
    assert!(links.iter().any(|link| link.text_content() == "Equation 1"
        && link.descendants("w:instrText").is_empty()));
    assert!(
        fields.iter().any(|f| f == &format!("PAGEREF {} \\h", bookmark_name))
    );
    assert_eq!(
        fields.iter().filter(|f| f.starts_with("SEQ Equation")).count(),
        3
    );
    assert!(
        fields
            .iter()
            .any(|f| f.starts_with("SEQ Equation") && f.contains(r"\r 7"))
    );
    let section = tree.descendants("w:sectPr")[0];
    let page = section.child("w:pgSz").unwrap();
    let margins = section.child("w:pgMar").unwrap();
    let width = page.attr("w:w").unwrap().parse::<u32>().unwrap()
        - margins.attr("w:left").unwrap().parse::<u32>().unwrap()
        - margins.attr("w:right").unwrap().parse::<u32>().unwrap();
    for table in tree.descendants("w:tbl") {
        let grid: Vec<_> = table
            .child("w:tblGrid")
            .unwrap()
            .children
            .iter()
            .map(|n| n.attr("w:w").unwrap().parse::<u32>().unwrap())
            .collect();
        assert_eq!(grid, [width / 10, width - 2 * (width / 10), width / 10]);
        assert_eq!(
            table
                .child("w:tblPr")
                .unwrap()
                .child("w:tblLayout")
                .unwrap()
                .attr("w:type"),
            Some("fixed")
        );
        assert_eq!(
            table
                .child("w:tblPr")
                .unwrap()
                .child("w:tblBorders")
                .unwrap()
                .children
                .iter()
                .filter(|n| n.attr("w:val") == Some("nil"))
                .count(),
            6
        );
        let row = table.child("w:tr").unwrap();
        assert!(row.child("w:trPr").unwrap().child("w:cantSplit").is_some());
        let cells: Vec<_> =
            row.children.iter().filter(|n| n.name == "w:tc").collect();
        for (cell, percentage) in cells.iter().zip(["500", "4000", "500"]) {
            let properties = cell.child("w:tcPr").unwrap();
            assert_eq!(
                properties.child("w:tcW").unwrap().attr("w:w"),
                Some(percentage)
            );
            assert_eq!(
                properties.child("w:tcW").unwrap().attr("w:type"),
                Some("pct")
            );
            assert_eq!(
                properties.child("w:vAlign").unwrap().attr("w:val"),
                Some("center")
            );
        }
        assert_eq!(
            paragraph_style(cells[0].child("w:p").unwrap()),
            Some(style_id(&parts, "Compact").as_str())
        );
        assert_eq!(cells[1].descendants("m:oMathPara").len(), 1);
        let number = cells[2].child("w:p").unwrap();
        assert_eq!(
            number.child("w:pPr").unwrap().child("w:jc").unwrap().attr("w:val"),
            Some("right")
        );
        let start = number
            .children
            .iter()
            .position(|n| n.name == "w:bookmarkStart")
            .unwrap();
        let end = number
            .children
            .iter()
            .position(|n| n.name == "w:bookmarkEnd")
            .unwrap();
        assert_eq!(number.children[start - 1].text_content(), "(");
        assert_eq!(number.children[end + 1].text_content(), ")");
    }
    assert!(tree.descendants("w:bookmarkStart").iter().any(|n|n.attr("w:name")==Some(anchor("eq-unnumbered").as_str())));
}

#[test]
fn section_sequences_and_the_headings_off_fallback_keep_effective_policy() {
    for headings in
        [HeadingNumbering::Pattern("1.".into()), HeadingNumbering::Off]
    {
        let mut config = Config::default();
        config.numbering.figures = FigureNumbering::Section;
        config.numbering.headings = headings.clone();
        let section = !matches!(headings, HeadingNumbering::Off);
        let doc = document(vec![
            json!({"type":"heading","level":1,"inlines":[text("Chapter")],"attrs":{},"number":"1."}),
            display(
                "x",
                "typst",
                Some("eq-one"),
                Some(if section { "1.1" } else { "1" }),
            ),
        ]);
        let output = emit(&doc, &config, &mut Fake::default(), None);
        let tree = document_tree(&output);
        let fields: Vec<_> = tree
            .descendants("w:instrText")
            .iter()
            .map(|n| n.text.trim())
            .collect();
        let sequence =
            fields.iter().find(|f| f.starts_with("SEQ Equation")).unwrap();
        assert_eq!(sequence.contains(r"\s 1"), section);
        let number =
            tree.descendants("w:tbl")[0].descendants("w:tc")[2].text_content();
        assert_eq!(number.contains("1."), section);
    }
}

#[test]
fn equation_styles_ignore_ambient_containers_and_number_frames_ignore_list_indent()
 {
    let equation = |name| display(name, "typst", Some(name), None);
    let mut doc = document(vec![
        json!({"type":"block_quote","blocks":[equation("quote")]}),
        json!({"type":"callout","kind":"note","title":null,"attrs":{},"blocks":[equation("callout")]}),
        json!({"type":"definition_list","items":[{"term":[text("Term")],"definitions":[[equation("definition")]]}]}),
        json!({"type":"div","attrs":{"pairs":{"custom-style":"Custom Equation"}},"blocks":[equation("custom")]}),
        json!({"type":"list","kind":{"kind":"ordered","start":1},"tight":false,"items":[{"blocks":[paragraph(vec![text("List parent")]),display("list","typst",Some("eq-list"),Some("1"))]}]}),
    ]);
    doc.meta.abstract_ =
        Some(vec![serde_json::from_value(equation("abstract")).unwrap()]);
    let mut renderer = Fake { echo: true, ..Fake::default() };
    let output = emit(&doc, &Config::default(), &mut renderer, None);
    let parts = package(&output);
    let tree = document_tree(&output);
    let equation_style = style_id(&parts, "Equation");
    for name in ["quote", "callout", "definition", "abstract"] {
        let paragraph = tree
            .descendants("w:p")
            .into_iter()
            .find(|p| p.descendants("m:t").iter().any(|t| t.text == name))
            .unwrap();
        assert_eq!(
            paragraph_style(paragraph),
            Some(equation_style.as_str()),
            "{name}"
        );
    }
    let custom = tree
        .descendants("w:p")
        .into_iter()
        .find(|p| p.descendants("m:t").iter().any(|t| t.text == "custom"))
        .unwrap();
    assert_eq!(
        paragraph_style(custom),
        Some(style_id(&parts, "Custom Equation").as_str())
    );
    assert!(!tree.descendants("w:numPr").is_empty(), "exercise the outer list");
    let frame = tree.descendants("w:tbl")[0];
    assert!(frame.descendants("w:numPr").is_empty());
    assert!(frame.descendants("w:ind").is_empty());
}

#[test]
fn reference_normal_size_scales_both_picture_extent_and_inline_descent() {
    let doc = document(vec![paragraph(vec![inline("x", "typst")])]);
    let initial = emit(&doc, &Config::default(), &mut Fake::default(), None);
    let mut parts = package(&initial);
    let styles = String::from_utf8(parts["word/styles.xml"].clone()).unwrap();
    let regex = Regex::new(
        r#"(?s)<w:style\b[^>]*w:styleId="Normal"[^>]*>.*?</w:style>"#,
    )
    .unwrap();
    let normal = regex.find(&styles).unwrap().as_str();
    // Normal inherits its initial font size from docDefaults. Set an explicit
    // reference size so this test exercises the reference-style override.
    let run_properties =
        Regex::new(r"(?s)<w:rPr\b[^>]*(?:/>|>.*?</w:rPr>)").unwrap();
    let cleared = run_properties.replace(normal, "");
    let enlarged = cleared.replace(
        "</w:style>",
        r#"<w:rPr><w:sz w:val="44"/><w:szCs w:val="44"/></w:rPr></w:style>"#,
    );
    parts.insert(
        "word/styles.xml".into(),
        styles.replacen(normal, &enlarged, 1).into_bytes(),
    );
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    let reference = tempfile::tempdir().unwrap();
    let path = reference.path().join("reference.docx");
    fs::write(&path, zip.finish().unwrap().into_inner()).unwrap();
    let mut config = Config::default();
    config.docx.math = DocxMath::Image;
    let output = emit(&doc, &config, &mut Fake::default(), Some(&path));
    let tree = document_tree(&output);
    let extent = tree.descendants("wp:extent")[0];
    assert_eq!(extent.attr("cx"), Some("508000"));
    assert_eq!(extent.attr("cy"), Some("254000"));
    assert_eq!(tree.descendants("w:position")[0].attr("w:val"), Some("-11"));
}

#[test]
fn real_pipeline_math_builds_are_valid_deterministic_and_independent_of_pdf_failure()
 {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("document.md"),"---\ndate: 2026-10-28\n---\n\n# Math\n\nInline $x^2$.\n\n$$x=1$$ {#eq-one}\n").unwrap();
    fs::write(
        directory.path().join("writer.toml"),
        "[pdf]\nsystem_fonts=false\n",
    )
    .unwrap();
    let mut opts = BuildOptions::new(directory.path().join("document.md"));
    opts.cli.out = Some(directory.path().join("result.docx"));
    let mut worlds = WorldSlot::new();
    let report = pipeline::build_targets(&opts, &[Target::Docx], &mut worlds);
    assert!(report.targets[0].written.is_ok(), "{:?}", report.targets);
    let bytes = fs::read(directory.path().join("result.docx")).unwrap();
    assert_eq!(validate(&bytes, &ValidateOptions { builtin_styles: true }), []);
    let report = pipeline::build_targets(&opts, &[Target::Docx], &mut worlds);
    assert!(report.targets[0].written.is_ok());
    assert_eq!(fs::read(directory.path().join("result.docx")).unwrap(), bytes);
    let tree = parse(&math_xml::part(
        &math_xml::zip_parts(&bytes),
        "word/document.xml",
    ));
    assert_eq!(tree.descendants("m:oMath").len(), 2);
    fs::write(
        directory.path().join("bad.typ"),
        "#let writer-doc(..args, body) = { panic(\"PDF only\") }\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("writer.toml"),
        "[pdf]\nsystem_fonts=false\ntemplate='bad.typ'\n",
    )
    .unwrap();
    opts.cli.out = Some(directory.path().join("out"));
    let report = pipeline::build_targets(
        &opts,
        &[Target::Html, Target::Docx, Target::Pdf],
        &mut worlds,
    );
    assert!(report.targets[0].written.is_ok());
    assert!(report.targets[1].written.is_ok());
    assert!(report.targets[2].written.is_err());
}
