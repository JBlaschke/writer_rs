//! HTML markup contract on resolved, hand-built IR (T13).

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::{Value, json};
use writer::assets::registry::AssetRegistry;
use writer::config::{Config, Toc};
use writer::error::{Diagnostics, Level};
use writer::html::emit::{
    EmitOptions, Emitted, KatexOnly, MARKUP_CLASSES, MathHook, MathRequest,
    MathUsage, emit, escape,
};
use writer::ir::*;

fn fixture() -> Document {
    serde_json::from_str(include_str!("fixtures/ir/sample-document.json"))
        .unwrap()
}

fn document(blocks: Vec<Block>) -> Document {
    Document {
        meta: DocMeta::default(),
        blocks,
        features: Features::default(),
        source: SourceInfo {
            path: PathBuf::from("/document.md"),
            kind: SourceKind::Markdown,
            base_dir: PathBuf::from("/"),
        },
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Resolved::default(),
    }
}

fn inline(value: Value) -> Inline {
    serde_json::from_value(value).unwrap()
}

fn text(value: &str) -> Inlines {
    vec![Inline::text(value)]
}

fn paragraph(value: &str) -> Block {
    Block::paragraph(text(value))
}

fn attrs(id: Option<&str>, classes: &[&str], pairs: &[(&str, &str)]) -> Attrs {
    Attrs {
        id: id.map(str::to_owned),
        classes: classes.iter().map(|class| (*class).into()).collect(),
        pairs: pairs
            .iter()
            .map(|&(key, value)| (key.into(), value.into()))
            .collect(),
    }
}

fn render(
    document: &Document, config: &Config, positions: bool,
) -> (Emitted, Diagnostics) {
    let mut math = KatexOnly::default();
    let mut assets = AssetRegistry::new();
    let mut diagnostics = Diagnostics::new();
    let emitted = emit(
        document,
        &mut EmitOptions {
            config,
            positions,
            includes: &document.includes,
            math: &mut math,
            assets: &mut assets,
            diagnostics: &mut diagnostics,
        },
    );
    (emitted, diagnostics)
}

fn body(blocks: Vec<Block>) -> String {
    render(&document(blocks), &Config::default(), false).0.body
}

fn note(number: u32, blocks: Vec<Block>) -> Inline {
    Inline::Footnote {
        blocks,
        label: Some(format!("note-{number}")),
        number: Some(number),
    }
}

fn note_ref(number: u32) -> Inline {
    Inline::FootnoteRef {
        label: format!("note-{number}"),
        number: Some(number),
    }
}

fn reference(target: &str, anchor: &str, inlines: Inlines) -> Inline {
    Inline::Ref {
        target: target.into(),
        supplement: None,
        form: RefForm::Normal,
        span: None,
        resolved: Some(ResolvedRef {
            kind: LabelKind::Figure,
            number: "1".into(),
            text: inlines,
            anchor: anchor.into(),
        }),
    }
}

#[test]
fn sample_build() {
    let doc = fixture();
    let original = serde_json::to_value(&doc).unwrap();
    let (emitted, _) = render(&doc, &Config::default(), false);
    assert_eq!(
        emitted.main,
        format!(
            "<main data-writer-document>\n{}{}{}</main>\n",
            emitted.title_block, emitted.body, emitted.footnotes
        )
    );
    assert!(emitted.math.katex);
    assert!(emitted.main.contains(
        "<a class=\"ref\" href=\"#fig-throughput\">Figure\u{a0}1</a>"
    ));
    assert!(emitted.main.contains(
        "<span class=\"citation\" data-cites=\"knuth1984\">[1]</span>"
    ));
    assert!(
        emitted.main.contains("<span class=\"equation-number\">(1)</span>")
    );
    assert!(!emitted.main.contains("data-sourcepos="));
    assert_eq!(render(&doc, &Config::default(), false).0, emitted);
    assert_eq!(serde_json::to_value(&doc).unwrap(), original);
    insta::assert_snapshot!(emitted.main);
}

#[test]
fn sample_positions() {
    let (emitted, _) = render(&fixture(), &Config::default(), true);
    assert_eq!(
        emitted.main,
        format!(
            "<main data-writer-document>\n{}{}{}</main>\n",
            emitted.title_block, emitted.body, emitted.footnotes
        )
    );
    assert!(emitted.math.katex);
    assert!(!emitted.title_block.contains("data-sourcepos="));
    assert!(emitted.body.contains("data-sourcepos=\"3-3\" data-source=\"0\""));
    insta::assert_snapshot!(emitted.main);
}

#[test]
fn heading_with_number() {
    assert_eq!(
        body(vec![Block::Heading {
            level: 2,
            inlines: text("A & B"),
            attrs: attrs(
                Some("sec-x"),
                &["wide"],
                &[("role", "heading"), ("x", "y")]
            ),
            number: Some("1.2".into()),
            span: None,
        }]),
        "<h2 id=\"sec-x\" class=\"wide\" role=\"heading\" data-x=\"y\"><span class=\"number\">1.2</span> A &amp; B</h2>\n"
    );
}

#[test]
fn paragraph_and_tight_list() {
    assert_eq!(
        body(vec![
            paragraph("Paragraph"),
            Block::List {
                kind: ListKind::Bullet,
                tight: true,
                items: vec![
                    ListItem { checked: None, blocks: vec![paragraph("One")] },
                    ListItem {
                        checked: None,
                        blocks: vec![
                            paragraph("Two"),
                            Block::List {
                                kind: ListKind::Bullet,
                                tight: true,
                                items: vec![ListItem {
                                    checked: None,
                                    blocks: vec![paragraph("Nested")]
                                }],
                                span: None,
                            }
                        ]
                    }
                ],
                span: None,
            }
        ]),
        "<p>Paragraph</p>\n<ul>\n<li>One</li>\n<li>\nTwo\n<ul>\n<li>Nested</li>\n</ul>\n</li>\n</ul>\n"
    );
}

#[test]
fn task_lists() {
    let items = vec![
        ListItem { checked: Some(true), blocks: vec![paragraph("Done")] },
        ListItem { checked: Some(false), blocks: vec![paragraph("Todo")] },
        ListItem::default(),
    ];
    assert_eq!(
        body(vec![Block::List {
            kind: ListKind::Bullet,
            tight: true,
            items: items.clone(),
            span: None
        }]),
        "<ul class=\"task-list\">\n<li class=\"task-list-item\"><input type=\"checkbox\" disabled checked> Done</li>\n<li class=\"task-list-item\"><input type=\"checkbox\" disabled> Todo</li>\n<li></li>\n</ul>\n"
    );
    assert_eq!(
        body(vec![Block::List {
            kind: ListKind::Bullet,
            tight: false,
            items: vec![items[0].clone()],
            span: None
        }]),
        "<ul class=\"task-list\">\n<li class=\"task-list-item\">\n<p><input type=\"checkbox\" disabled checked> Done</p>\n</li>\n</ul>\n"
    );
    assert_eq!(
        body(vec![Block::List {
            kind: ListKind::Bullet,
            tight: false,
            items: vec![ListItem {
                checked: Some(false),
                blocks: vec![Block::ThematicBreak { span: None }]
            }],
            span: None
        }]),
        "<ul class=\"task-list\">\n<li class=\"task-list-item\"><input type=\"checkbox\" disabled>\n<hr>\n</li>\n</ul>\n"
    );
}

#[test]
fn ordered_list_start() {
    for (start, opening) in [(1, "<ol>"), (5, "<ol start=\"5\">")] {
        assert_eq!(
            body(vec![Block::List {
                kind: ListKind::Ordered { start },
                tight: false,
                items: vec![ListItem {
                    checked: None,
                    blocks: vec![paragraph("Item")]
                }],
                span: None
            }]),
            format!("{opening}\n<li>\n<p>Item</p>\n</li>\n</ol>\n")
        );
    }
}

#[test]
fn definition_list() {
    assert_eq!(
        body(vec![Block::DefinitionList {
            items: vec![DefinitionItem {
                term: text("Term"),
                definitions: vec![
                    vec![paragraph("First")],
                    vec![paragraph("Second")]
                ],
            }],
            span: None
        }]),
        "<dl>\n<dt>Term</dt>\n<dd>\n<p>First</p>\n</dd>\n<dd>\n<p>Second</p>\n</dd>\n</dl>\n"
    );
}

#[test]
fn blockquote() {
    assert_eq!(
        body(vec![Block::BlockQuote {
            blocks: vec![paragraph("Quoted")],
            span: None
        }]),
        "<blockquote>\n<p>Quoted</p>\n</blockquote>\n"
    );
}

#[test]
fn callout_titles() {
    for (kind, title) in [
        ("note", "Note"),
        ("tip", "Tip"),
        ("important", "Important"),
        ("warning", "Warning"),
        ("caution", "Caution"),
        ("remark", "Remark"),
    ] {
        assert_eq!(
            body(vec![Block::Callout {
                kind: kind.into(),
                title: None,
                blocks: vec![paragraph("Body")],
                attrs: Attrs::default(),
                span: None
            }]),
            format!(
                "<aside class=\"callout callout-{kind}\" role=\"note\">\n<p class=\"callout-title\">{title}</p>\n<p>Body</p>\n</aside>\n"
            )
        );
    }
    assert_eq!(
        body(vec![Block::Callout {
            kind: "note".into(),
            title: Some(vec![Inline::Emph { inlines: text("Own") }]),
            blocks: vec![],
            attrs: attrs(
                Some("box"),
                &["wide"],
                &[
                    ("kind", "unused"),
                    ("title", "unused"),
                    ("role", "override")
                ]
            ),
            span: None
        }]),
        "<aside id=\"box\" class=\"callout callout-note wide\" role=\"note\">\n<p class=\"callout-title\"><em>Own</em></p>\n</aside>\n"
    );
}

#[test]
fn div_columns() {
    assert_eq!(
        body(vec![Block::Div {
            attrs: attrs(
                Some("cols"),
                &["columns"],
                &[("widths", "1fr 2fr"), ("style", "gap:1em")]
            ),
            blocks: vec![Block::Div {
                attrs: attrs(
                    None,
                    &["column"],
                    &[("width", "50%"), ("x", "keep")]
                ),
                blocks: vec![paragraph("Body")],
                span: None
            }],
            span: None
        }]),
        "<div id=\"cols\" class=\"columns\" style=\"grid-template-columns:1fr 2fr;gap:1em\">\n<div class=\"column\" data-x=\"keep\">\n<p>Body</p>\n</div>\n</div>\n"
    );
}

#[test]
fn raw_blocks() {
    assert_eq!(
        body(vec![
            Block::Raw {
                format: "html".into(),
                text: "<div>raw</div>".into(),
                span: Some(SourceSpan {
                    start_line: 1,
                    end_line: 1,
                    cell: None,
                    file: None
                })
            },
            Block::Raw {
                format: "latex".into(),
                text: "ignored".into(),
                span: None
            }
        ]),
        "<div>raw</div>\n"
    );
    assert_eq!(
        body(vec![Block::Raw {
            format: "html".into(),
            text: "<div>raw</div>\n".into(),
            span: None
        }]),
        "<div>raw</div>\n"
    );
}

#[test]
fn breaks() {
    assert_eq!(
        body(vec![
            Block::ThematicBreak { span: None },
            Block::PageBreak { span: None }
        ]),
        "<hr>\n<div class=\"page-break\"></div>\n"
    );
}

#[test]
fn figure_captions() {
    for (kind, class, supplement, caption_first) in [
        (FigureKind::Image, "image", "Figure", false),
        (FigureKind::Table, "table", "Table", true),
        (FigureKind::Listing, "listing", "Listing", true),
        (FigureKind::Other, "other", "Figure", false),
    ] {
        let image = Block::paragraph(vec![Inline::Image {
            alt: text("Alt"),
            url: "asset:000000000001".into(),
            title: None,
            attrs: Attrs::default(),
        }]);
        let content = match kind {
            FigureKind::Image => image,
            FigureKind::Table => Block::Table {
                columns: vec![],
                head: vec![],
                body: vec![],
                foot: vec![],
                attrs: Attrs::default(),
                span: None,
            },
            FigureKind::Listing => Block::CodeBlock {
                lang: None,
                text: "Body".into(),
                attrs: Attrs::default(),
                span: None,
            },
            FigureKind::Other => paragraph("Body"),
        };
        let html = body(vec![Block::Figure {
            kind,
            caption: Some(text("Caption")),
            body: vec![content],
            attrs: attrs(
                Some("fig-x"),
                &["wide"],
                &[("caption", "unused"), ("kind", "unused")],
            ),
            number: Some("2".into()),
            span: None,
        }]);
        let content = match kind {
            FigureKind::Image => {
                "<img src=\"asset:000000000001\" alt=\"Alt\">\n"
            }
            FigureKind::Table => "<table>\n</table>\n",
            FigureKind::Listing => {
                "<pre class=\"hl-code\"><code><span class=\"line\">Body</span></code></pre>\n"
            }
            FigureKind::Other => "<p>Body</p>\n",
        };
        let caption = format!(
            "<figcaption><span class=\"caption-label\">{supplement}\u{a0}2:</span> Caption</figcaption>\n"
        );
        assert_eq!(
            html,
            format!(
                "<figure id=\"fig-x\" class=\"figure kind-{class} wide\">\n{}{}</figure>\n",
                if caption_first { &caption } else { content },
                if caption_first { content } else { &caption }
            )
        );
    }
    for (number, caption, expected) in [
        (None, None, ""),
        (
            Some("2"),
            None,
            "<figcaption><span class=\"caption-label\">Figure\u{a0}2</span></figcaption>\n",
        ),
        (None, Some("Caption"), "<figcaption>Caption</figcaption>\n"),
    ] {
        assert_eq!(
            body(vec![Block::Figure {
                kind: FigureKind::Other,
                caption: caption.map(text),
                body: vec![],
                attrs: Attrs::default(),
                number: number.map(str::to_owned),
                span: None
            }]),
            format!(
                "<figure class=\"figure kind-other\">\n{expected}</figure>\n"
            )
        );
    }
}

#[test]
fn table_colgroup_spans_align() {
    let cell = |value: &str, header, colspan, rowspan, align| TableCell {
        inlines: text(value),
        header,
        colspan,
        rowspan,
        align,
    };
    let table = Block::Table {
        columns: vec![
            ColumnSpec { align: Align::Left, width: Some(0.333333) },
            ColumnSpec { align: Align::Center, width: None },
            ColumnSpec { align: Align::Right, width: Some(0.5) },
        ],
        head: vec![Row { cells: vec![cell("H", true, 3, 1, None)] }],
        body: vec![
            Row {
                cells: vec![
                    cell("Row", true, 1, 2, None),
                    cell("Wide", false, 2, 1, Some(Align::Default)),
                ],
            },
            Row {
                cells: vec![
                    cell("Center", false, 1, 1, None),
                    cell("Right", false, 1, 1, None),
                ],
            },
        ],
        foot: vec![Row { cells: vec![cell("Footer", true, 3, 1, None)] }],
        attrs: attrs(Some("grid"), &["wide"], &[("data-extra", "x")]),
        span: None,
    };
    assert_eq!(
        body(vec![table]),
        "<table id=\"grid\" class=\"wide\" data-extra=\"x\">\n<colgroup>\n<col style=\"width:33.33%\">\n<col>\n<col style=\"width:50%\">\n</colgroup>\n<thead>\n<tr>\n<th class=\"align-left\" scope=\"col\" colspan=\"3\">H</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<th class=\"align-left\" scope=\"row\" rowspan=\"2\">Row</th>\n<td colspan=\"2\">Wide</td>\n</tr>\n<tr>\n<td class=\"align-center\">Center</td>\n<td class=\"align-right\">Right</td>\n</tr>\n</tbody>\n<tfoot>\n<tr>\n<th class=\"align-left\" scope=\"row\" colspan=\"3\">Footer</th>\n</tr>\n</tfoot>\n</table>\n"
    );
}

#[test]
fn display_and_inline_math() {
    assert_eq!(
        body(vec![
            Block::paragraph(vec![Inline::Math {
                source: "x < y".into(),
                dialect: MathDialect::Latex
            }]),
            Block::Math {
                source: "x^2".into(),
                dialect: MathDialect::Latex,
                attrs: Attrs::with_id("eq-x"),
                number: Some("1".into()),
                span: None
            }
        ]),
        "<p><span class=\"math inline\" data-math-style=\"inline\" data-tex=\"x &lt; y\">x &lt; y</span></p>\n<div id=\"eq-x\" class=\"equation numbered\"><span class=\"math display\" data-math-style=\"display\" data-tex=\"x^2\">x^2</span><span class=\"equation-number\">(1)</span></div>\n"
    );
}

#[derive(Default)]
struct MathRecorder(Vec<(String, MathDialect, bool, Option<SourceSpan>)>);

impl MathHook for MathRecorder {
    fn equation(
        &mut self, eq: &MathRequest<'_>, _: &mut AssetRegistry,
        _: &mut Diagnostics,
    ) -> String {
        self.0.push((
            eq.source.into(),
            eq.dialect,
            eq.display,
            eq.span.cloned(),
        ));
        format!(
            "<span class=\"math {}\">{}</span>",
            if eq.display { "display" } else { "inline" },
            escape(eq.source)
        )
    }
    fn usage(&self) -> MathUsage {
        MathUsage { mathml: true, ..MathUsage::default() }
    }
}

#[test]
fn math_hook_receives_enclosing_spans() {
    let span = SourceSpan {
        start_line: 4,
        end_line: 8,
        file: Some("/included.md".into()),
        cell: Some(2),
    };
    let equation = |source: &str| Inline::Math {
        source: source.into(),
        dialect: MathDialect::Latex,
    };
    let mut doc = document(vec![
        Block::Div {
            attrs: Attrs::default(),
            blocks: vec![Block::paragraph(vec![equation("nested")])],
            span: Some(span.clone()),
        },
        Block::Math {
            source: "display".into(),
            dialect: MathDialect::Typst,
            attrs: Attrs::default(),
            number: None,
            span: Some(span.clone()),
        },
    ]);
    doc.meta.abstract_ = Some(vec![Block::Paragraph {
        inlines: vec![equation("abstract")],
        span: Some(span.clone()),
    }]);
    let mut hook = MathRecorder::default();
    let mut assets = AssetRegistry::new();
    let mut diagnostics = Diagnostics::new();
    let config = Config::default();
    let emitted = emit(
        &doc,
        &mut EmitOptions {
            config: &config,
            positions: true,
            includes: &[],
            math: &mut hook,
            assets: &mut assets,
            diagnostics: &mut diagnostics,
        },
    );
    assert_eq!(
        hook.0,
        [
            ("abstract".into(), MathDialect::Latex, false, None),
            ("nested".into(), MathDialect::Latex, false, Some(span.clone())),
            ("display".into(), MathDialect::Typst, true, Some(span))
        ]
    );
    assert_eq!(
        emitted.math,
        MathUsage { mathml: true, ..MathUsage::default() }
    );
    assert!(!emitted.title_block.contains("data-sourcepos="));
}

#[test]
fn code_block_numbered_start() {
    assert_eq!(
        body(vec![Block::CodeBlock {
            lang: Some("text".into()),
            text: "a\nb\nc\n".into(),
            attrs: attrs(
                Some("code"),
                &["numbered", "plain", "wide"],
                &[
                    ("start", "5"),
                    ("highlight", "6-7"),
                    ("lines", "7"),
                    ("caption", "skip"),
                    ("include", "skip"),
                    ("trim", "true"),
                    ("escape", "true"),
                    ("ansi", "skip"),
                    ("x", "keep")
                ]
            ),
            span: None
        }]),
        "<pre id=\"code\" class=\"hl-code numbered wide\" style=\"counter-reset:line 4\" data-x=\"keep\"><code class=\"language-text\"><span class=\"line\">a</span>\n<span class=\"line highlight\">b</span>\n<span class=\"line highlight\">c</span></code></pre>\n"
    );
    assert_eq!(
        body(vec![Block::CodeBlock {
            lang: None,
            text: "".into(),
            attrs: Attrs::default(),
            span: None
        }]),
        "<pre class=\"hl-code\"><code></code></pre>\n"
    );
}

fn output(
    kind: OutputKind, count: Option<i64>, candidates: Vec<(&str, Vec<Block>)>,
) -> CellOutput {
    CellOutput {
        kind,
        execution_count: count,
        mimes: vec![],
        candidates: candidates
            .into_iter()
            .map(|(mime, blocks)| OutputCandidate { mime: mime.into(), blocks })
            .collect(),
    }
}

#[test]
fn cell_prompts_outputs() {
    let cell = Cell {
        index: 3,
        language: "text".into(),
        source: "input".into(),
        execution_count: Some(2),
        show_input: true,
        show_output: true,
        attrs: attrs(
            Some("cell-x"),
            &["wide", "plain"],
            &[("start", "5"), ("custom", "x")],
        ),
        outputs: vec![output(
            OutputKind::Result,
            Some(2),
            vec![
                (
                    "text/latex",
                    vec![Block::Raw {
                        format: "latex".into(),
                        text: "skipped".into(),
                        span: None,
                    }],
                ),
                ("text/html", vec![paragraph("Result")]),
                ("text/plain", vec![paragraph("Fallback")]),
            ],
        )],
        ..Cell::default()
    };
    let expected = "<div id=\"cell-x\" class=\"cell wide\" data-cell=\"3\" data-custom=\"x\">\n<div class=\"cell-input\">\n<div class=\"cell-prompt\">In [2]:</div>\n<pre class=\"hl-code\"><code class=\"language-text\"><span class=\"line\">input</span></code></pre>\n</div>\n<div class=\"cell-outputs\">\n<div class=\"output output-result\" data-mime=\"text/html\">\n<div class=\"cell-prompt\">Out [2]:</div>\n<p>Result</p>\n</div>\n</div>\n</div>\n";
    assert_eq!(body(vec![Block::Cell(cell.clone())]), expected);
    let mut config = Config::default();
    config.notebook.show_prompts = false;
    let emitted =
        render(&document(vec![Block::Cell(cell.clone())]), &config, false)
            .0
            .body;
    assert!(!emitted.contains("cell-prompt"));
    let mut input = cell.clone();
    input.execution_count = None;
    input.language.clear();
    input.outputs.clear();
    assert!(body(vec![Block::Cell(input)]).contains("In [ ]:"));
    let mut hidden = cell;
    hidden.show_input = false;
    hidden.show_output = false;
    assert_eq!(body(vec![Block::Cell(hidden)]), "");
}

#[test]
fn cell_output_kinds_and_empty_fallbacks() {
    let candidate = || vec![("text/plain", vec![paragraph("Shown")])];
    let cell = Cell {
        index: 0,
        show_output: true,
        outputs: vec![
            output(
                OutputKind::Stream { name: "stderr".into() },
                None,
                candidate(),
            ),
            output(
                OutputKind::Error { ename: "Error".into(), evalue: "x".into() },
                None,
                candidate(),
            ),
            output(OutputKind::Display, None, candidate()),
            output(OutputKind::Result, None, candidate()),
            output(
                OutputKind::Display,
                None,
                vec![
                    (
                        "text/latex",
                        vec![Block::Raw {
                            format: "latex".into(),
                            text: "skip".into(),
                            span: None,
                        }],
                    ),
                    ("", vec![]),
                ],
            ),
        ],
        ..Cell::default()
    };
    assert_eq!(
        body(vec![Block::Cell(cell)]),
        "<div class=\"cell\" data-cell=\"0\">\n<div class=\"cell-outputs\">\n<div class=\"output output-stream stderr\" data-mime=\"text/plain\">\n<p>Shown</p>\n</div>\n<div class=\"output output-error\" data-mime=\"text/plain\">\n<p>Shown</p>\n</div>\n<div class=\"output output-display\" data-mime=\"text/plain\">\n<p>Shown</p>\n</div>\n<div class=\"output output-result\" data-mime=\"text/plain\">\n<p>Shown</p>\n</div>\n</div>\n</div>\n"
    );
    let cell = Cell {
        show_output: true,
        outputs: vec![output(OutputKind::Display, None, vec![("", vec![])])],
        ..Cell::default()
    };
    assert_eq!(body(vec![Block::Cell(cell)]), "");
}

#[test]
fn toc_nested_and_fragment() {
    let mut doc = document(vec![Block::Toc { depth: 2, span: None }]);
    doc.resolved.toc = vec![
        TocEntry {
            level: 1,
            number: Some("1".into()),
            inlines: text("Introduction"),
            anchor: "sec-intro".into(),
        },
        TocEntry {
            level: 2,
            number: Some("1.1".into()),
            inlines: text("Detail"),
            anchor: "sec-x".into(),
        },
        TocEntry {
            level: 1,
            number: None,
            inlines: text("Method"),
            anchor: "sec-method".into(),
        },
        TocEntry {
            level: 4,
            number: None,
            inlines: text("Too deep"),
            anchor: "deep".into(),
        },
    ];
    let expected = "<nav class=\"toc\" role=\"doc-toc\">\n<p class=\"toc-title\">Contents</p>\n<ol>\n<li><a href=\"#sec-intro\"><span class=\"number\">1</span> Introduction</a>\n<ol>\n<li><a href=\"#sec-x\"><span class=\"number\">1.1</span> Detail</a></li>\n</ol>\n</li>\n<li><a href=\"#sec-method\">Method</a></li>\n</ol>\n</nav>\n";
    let mut config = Config::default();
    config.document.toc = Toc::Depth(2);
    let (emitted, _) = render(&doc, &config, true);
    assert_eq!(emitted.body, expected);
    assert_eq!(emitted.toc, expected);
    assert!(!emitted.toc.contains("data-sourcepos"));
    assert_eq!(
        render(
            &document(vec![Block::Toc { depth: 3, span: None }]),
            &config,
            false
        )
        .0
        .body,
        ""
    );
}

#[test]
fn bibliography_entries() {
    let mut doc = document(vec![Block::Bibliography { span: None }]);
    doc.resolved.bibliography = Some(Bibliography {
        style: "ieee".into(),
        hanging_indent: true,
        entries: vec![
            BibEntry {
                key: "knuth1984".into(),
                prefix: Some(text("[1]")),
                inlines: text("The TeXbook"),
            },
            BibEntry {
                key: "plain".into(),
                prefix: None,
                inlines: text("Entry"),
            },
        ],
    });
    assert_eq!(
        render(&doc, &Config::default(), false).0.body,
        "<section id=\"refs\" class=\"references csl-bib-body hanging-indent\" role=\"doc-bibliography\">\n<h1 class=\"references-title\">Bibliography</h1>\n<div id=\"ref-knuth1984\" class=\"csl-entry\"><div class=\"csl-left-margin\">[1]</div><div class=\"csl-right-inline\">The TeXbook</div></div>\n<div id=\"ref-plain\" class=\"csl-entry\">Entry</div>\n</section>\n"
    );
    assert_eq!(body(vec![Block::Bibliography { span: None }]), "");
}

#[test]
fn title_block_omissions() {
    assert_eq!(
        render(&document(vec![]), &Config::default(), false).0.title_block,
        ""
    );
    let mut doc = document(vec![]);
    doc.meta.title = Some("Title <plain>".into());
    assert_eq!(
        render(&doc, &Config::default(), false).0.title_block,
        "<header class=\"title-block\">\n<h1 class=\"title\">Title &lt;plain&gt;</h1>\n</header>\n"
    );
    doc.meta.title = None;
    doc.meta.authors = vec![Author { name: "A".into(), ..Author::default() }];
    assert_eq!(
        render(&doc, &Config::default(), false).0.title_block,
        "<header class=\"title-block\">\n<div class=\"authors\">\n<p class=\"author\"><span class=\"author-name\">A</span></p>\n</div>\n</header>\n"
    );
}

#[test]
fn inline_formatting() {
    let inlines = [
        "emph", "strong", "strikeout", "subscript", "superscript", "underline",
        "small_caps",
    ]
    .into_iter()
    .map(|kind| {
        inline(
            json!({"type": kind, "inlines": [{"type": "text", "text": "<&"}]}),
        )
    })
    .collect();
    assert_eq!(
        body(vec![Block::paragraph(inlines)]),
        "<p><em>&lt;&amp;</em><strong>&lt;&amp;</strong><del>&lt;&amp;</del><sub>&lt;&amp;</sub><sup>&lt;&amp;</sup><u>&lt;&amp;</u><span class=\"smallcaps\">&lt;&amp;</span></p>\n"
    );
}

#[test]
fn code_link_image_inlines() {
    let inlines = vec![
        Inline::Code {
            text: "<&".into(),
            attrs: attrs(Some("code"), &["wide"], &[("x", "y")]),
        },
        Inline::Link {
            inlines: text("Link"),
            url: "https://x.test/?a=1&b=2".into(),
            title: Some("A \"title\"".into()),
            attrs: attrs(Some("link"), &[], &[("title", "ignored")]),
        },
        Inline::Image {
            alt: vec![Inline::Emph { inlines: text("Alt") }],
            url: "asset:000000000001?x=1&y=2".into(),
            title: Some("Plot".into()),
            attrs: attrs(
                Some("image"),
                &["wide"],
                &[
                    ("width", "20"),
                    ("height", "10%"),
                    ("style", "border:0"),
                    ("title", "ignored"),
                ],
            ),
        },
    ];
    assert_eq!(
        body(vec![Block::paragraph(inlines)]),
        "<p><code id=\"code\" class=\"wide\" data-x=\"y\">&lt;&amp;</code><a id=\"link\" href=\"https://x.test/?a=1&amp;b=2\" title=\"A &quot;title&quot;\">Link</a><img id=\"image\" class=\"wide\" src=\"asset:000000000001?x=1&amp;y=2\" alt=\"Alt\" title=\"Plot\" style=\"width:20px;height:10%;border:0\"></p>\n"
    );
}

#[test]
fn refs_cites_unresolved() {
    let keys = || {
        vec![
            CiteItem { key: "a".into(), ..CiteItem::default() },
            CiteItem { key: "b".into(), ..CiteItem::default() },
        ]
    };
    let nodes = vec![
        reference("fig-x", "fig-x", text("Figure\u{a0}1")),
        reference("text", "", text("Text")),
        Inline::Ref {
            target: "missing".into(),
            supplement: None,
            form: RefForm::Page,
            resolved: None,
            span: None,
        },
        Inline::Cite {
            items: keys(),
            mode: CiteMode::Normal,
            resolved: None,
            span: None,
        },
        Inline::Cite {
            items: keys(),
            mode: CiteMode::Normal,
            resolved: Some(ResolvedCite {
                inlines: text("[1,2]"),
                note: false,
            }),
            span: None,
        },
    ];
    assert_eq!(
        body(vec![Block::paragraph(nodes)]),
        "<p><a class=\"ref\" href=\"#fig-x\">Figure\u{a0}1</a><span class=\"ref\">Text</span><span class=\"ref unresolved\">[?missing]</span><span class=\"citation unresolved\" data-cites=\"a b\">[?a; b]</span><span class=\"citation\" data-cites=\"a b\">[1,2]</span></p>\n"
    );
    let mut page = reference("fig-x", "fig-x", text("Figure 1"));
    if let Inline::Ref { form, .. } = &mut page {
        *form = RefForm::Page;
    }
    let (_, diagnostics) = render(
        &document(vec![Block::paragraph(vec![page])]),
        &Config::default(),
        false,
    );
    assert!(
        diagnostics.iter().next().is_none(),
        "target compatibility is a check concern"
    );
}

#[test]
fn breaks_spans_raw_inline() {
    assert_eq!(
        body(vec![Block::paragraph(vec![
            Inline::text("A"),
            Inline::SoftBreak,
            Inline::text("B"),
            Inline::LineBreak,
            Inline::Span {
                attrs: attrs(Some("span"), &["wide"], &[("dir", "rtl")]),
                inlines: text("C")
            },
            Inline::Raw { format: "html".into(), text: "<b>raw</b>".into() },
            Inline::Raw { format: "latex".into(), text: "ignored".into() }
        ])]),
        "<p>A B<br>\n<span id=\"span\" class=\"wide\" dir=\"rtl\">C</span><b>raw</b></p>\n"
    );
}

#[test]
fn label_fn_1_both_ids() {
    let doc = document(vec![
        Block::Heading {
            level: 1,
            inlines: text("Label"),
            attrs: Attrs::with_id("fn-1"),
            number: None,
            span: None,
        },
        Block::paragraph(vec![note(1, vec![paragraph("Note")])]),
    ]);
    let (emitted, diagnostics) = render(&doc, &Config::default(), false);
    assert_eq!(emitted.main.matches("id=\"fn-1\"").count(), 2);
    assert!(diagnostics.iter().next().is_none());
}

#[test]
fn footnote_inside_link() {
    let doc = document(vec![Block::paragraph(vec![Inline::Link {
        inlines: vec![Inline::text("Link"), note(1, vec![paragraph("Note")])],
        url: "https://example.org".into(),
        title: None,
        attrs: Attrs::default(),
    }])]);
    let (emitted, _) = render(&doc, &Config::default(), false);
    assert_eq!(
        emitted.body,
        "<p><a href=\"https://example.org\">Link</a><sup class=\"footnote-ref\"><a id=\"fnref-1\" href=\"#fn-1\" role=\"doc-noteref\">1</a></sup></p>\n"
    );
    assert_eq!(
        emitted.footnotes,
        "<section class=\"footnotes\" role=\"doc-endnotes\">\n<hr>\n<ol>\n<li id=\"fn-1\">\n<p>Note <a class=\"footnote-back\" href=\"#fnref-1\" role=\"doc-backlink\">↩︎</a></p>\n</li>\n</ol>\n</section>\n"
    );
}

#[test]
fn footnote_body_rereferences_earlier_note() {
    let doc = document(vec![Block::paragraph(vec![
        note(1, vec![paragraph("One")]),
        note(2, vec![Block::paragraph(vec![Inline::text("Two"), note_ref(1)])]),
        note_ref(1),
    ])]);
    let (emitted, _) = render(&doc, &Config::default(), false);
    assert_eq!(
        emitted.body,
        "<p><sup class=\"footnote-ref\"><a id=\"fnref-1\" href=\"#fn-1\" role=\"doc-noteref\">1</a></sup><sup class=\"footnote-ref\"><a id=\"fnref-2\" href=\"#fn-2\" role=\"doc-noteref\">2</a></sup><sup class=\"footnote-ref\"><a id=\"fnref-1-3\" href=\"#fn-1\" role=\"doc-noteref\">1</a></sup></p>\n"
    );
    assert!(emitted.footnotes.contains("id=\"fnref-1-2\" href=\"#fn-1\""));
    for suffix in ["", "-2", "-3"] {
        assert!(emitted.footnotes.contains(&format!(
            "class=\"footnote-back\" href=\"#fnref-1{suffix}\""
        )));
    }
    assert!(emitted.footnotes.contains("↩︎<sup>3</sup>"));
}

#[test]
fn footnote_backlinks_for_nonparagraph_end_and_unnumbered_notes() {
    let doc = document(vec![Block::paragraph(vec![
        note(1, vec![Block::ThematicBreak { span: None }]),
        Inline::Footnote {
            blocks: vec![paragraph("Hidden")],
            label: None,
            number: None,
        },
        Inline::FootnoteRef { label: "unknown".into(), number: None },
    ])]);
    let (emitted, _) = render(&doc, &Config::default(), false);
    assert!(!emitted.main.contains("Hidden"));
    assert!(emitted.footnotes.contains("<hr>\n<p><a class=\"footnote-back\" href=\"#fnref-1\" role=\"doc-backlink\">↩︎</a></p>\n"));
}

#[test]
fn nested_anchors_and_toc_ids() {
    let nested = Inline::Link {
        inlines: vec![
            Inline::text("Outer "),
            Inline::Link {
                inlines: text("inner"),
                url: "inner".into(),
                title: None,
                attrs: Attrs::with_id("inner-id"),
            },
            reference("x", "x", text(" ref")),
        ],
        url: "outer".into(),
        title: None,
        attrs: Attrs::default(),
    };
    assert_eq!(
        body(vec![Block::paragraph(vec![nested])]),
        "<p><a href=\"outer\">Outer inner ref</a></p>\n"
    );
    let mut doc = document(vec![Block::Toc { depth: 3, span: None }]);
    doc.resolved.toc.push(TocEntry {
        level: 1,
        number: None,
        anchor: "heading".into(),
        inlines: vec![
            Inline::Code {
                text: "Code".into(),
                attrs: Attrs::with_id("code-id"),
            },
            Inline::Span {
                attrs: Attrs::with_id("span-id"),
                inlines: text("Span"),
            },
            Inline::Link {
                inlines: text("Link"),
                url: "inner".into(),
                title: None,
                attrs: Attrs::default(),
            },
        ],
    });
    let emitted = render(&doc, &Config::default(), false).0;
    assert!(emitted.body.contains(
        "<a href=\"#heading\"><code>Code</code><span>Span</span>Link</a>"
    ));
    assert!(!emitted.toc.contains("id="));
}

#[test]
fn duplicate_passthrough_keys() {
    assert_eq!(
        body(vec![Block::Div {
            attrs: attrs(
                Some("own"),
                &["wide"],
                &[
                    ("id", "ignored"),
                    ("custom-style", "skip"),
                    ("ansi", "skip"),
                    ("x", "later"),
                    ("data-x", "first"),
                    ("title", "T"),
                    ("style", "color:red"),
                    ("lang", "de"),
                    ("role", "group"),
                    ("aria-label", "A")
                ]
            ),
            blocks: vec![],
            span: None
        }]),
        "<div id=\"own\" class=\"wide\" aria-label=\"A\" data-x=\"first\" data-id=\"ignored\" lang=\"de\" role=\"group\" style=\"color:red\" title=\"T\">\n</div>\n"
    );
}

#[test]
fn output_code_ansi_pair_not_printed() {
    for class in [
        "output-text", "output-stream-stdout", "output-stream-stderr",
        "output-error", "output-json",
    ] {
        let doc = document(vec![Block::CodeBlock {
            lang: Some("nosuchlang".into()),
            text: "<&".into(),
            attrs: attrs(None, &[class], &[("ansi", "canonical ansi")]),
            span: None,
        }]);
        let (emitted, diagnostics) = render(&doc, &Config::default(), false);
        assert_eq!(
            emitted.body,
            format!(
                "<pre class=\"hl-code {class}\"><code class=\"language-nosuchlang\"><span class=\"line\">&lt;&amp;</span></code></pre>\n"
            )
        );
        assert!(!emitted.body.contains("data-ansi"));
        assert!(
            diagnostics.iter().next().is_none(),
            "output language is not resolved"
        );
    }
}

#[test]
fn escape_five_characters() {
    assert_eq!(
        escape("&<>\"'\r\n café / = `"),
        "&amp;&lt;&gt;&quot;&#39;\r\n café / = `"
    );
}

#[test]
fn typst_math_katex_only_fallback() {
    let span = SourceSpan {
        start_line: 2,
        end_line: 3,
        file: Some("/included.md".into()),
        cell: Some(4),
    };
    let doc = document(vec![Block::Math {
        source: "x < y".into(),
        dialect: MathDialect::Typst,
        attrs: Attrs::default(),
        number: None,
        span: Some(span.clone()),
    }]);
    let (emitted, diagnostics) = render(&doc, &Config::default(), false);
    assert_eq!(
        emitted.body,
        "<div class=\"equation\"><span class=\"math display math-source\" data-tex=\"x &lt; y\"><code>x &lt; y</code></span></div>\n"
    );
    assert_eq!(emitted.math, MathUsage::default());
    let found: Vec<_> = diagnostics.iter().collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code, "math-conversion-failed");
    assert_eq!(found[0].level, Level::Warning);
    assert_eq!(found[0].span.as_ref(), Some(&span));
    assert!(
        found[0].message.starts_with("html: cannot convert equation (")
            && found[0].message.ends_with("); used source")
    );
}

#[test]
fn unknown_language_info() {
    for file in [None, Some(PathBuf::from("/included.md"))] {
        let span = SourceSpan {
            start_line: 4,
            end_line: 8,
            cell: None,
            file: file.clone(),
        };
        let doc = document(vec![Block::CodeBlock {
            lang: Some("nosuchlang".into()),
            text: "<&".into(),
            attrs: Attrs::default(),
            span: Some(span.clone()),
        }]);
        let (emitted, diagnostics) = render(&doc, &Config::default(), false);
        assert!(emitted.body.contains("<span class=\"line\">&lt;&amp;</span>"));
        let found: Vec<_> = diagnostics.iter().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "code-language-unknown");
        assert_eq!(found[0].level, Level::Info);
        assert_eq!(found[0].span.as_ref(), Some(&span));
        assert_eq!(
            found[0].path.as_ref(),
            Some(file.as_ref().unwrap_or(&doc.source.path))
        );
    }
}

#[test]
fn markup_classes_cover_sample() {
    fn classes(value: &Value, out: &mut BTreeSet<String>) {
        match value {
            Value::Array(items) => {
                items.iter().for_each(|value| classes(value, out))
            }
            Value::Object(object) => {
                if let Some(Value::Array(items)) = object.get("classes") {
                    out.extend(
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned),
                    );
                }
                object.values().for_each(|value| classes(value, out));
            }
            _ => {}
        }
    }
    let doc = fixture();
    let mut allowed: BTreeSet<String> =
        MARKUP_CLASSES.iter().map(|class| (*class).into()).collect();
    classes(&serde_json::to_value(&doc).unwrap(), &mut allowed);
    for positions in [false, true] {
        let main = render(&doc, &Config::default(), positions).0.main;
        for rest in main.split("class=\"").skip(1) {
            let class = rest.split('"').next().unwrap();
            for token in class.split_whitespace() {
                assert!(
                    allowed.contains(token)
                        || token.starts_with("hl-")
                        || token.starts_with("language-"),
                    "unlisted emitter class {token}"
                );
            }
        }
    }
}

#[test]
fn positions_include_cell_identity_and_omit_inline_raw_abstract() {
    let included = PathBuf::from("/part.md");
    let span = SourceSpan {
        start_line: 7,
        end_line: 9,
        file: Some(included.clone()),
        cell: Some(3),
    };
    let mut doc = document(vec![
        Block::Heading {
            level: 2,
            inlines: text("Heading"),
            attrs: attrs(
                Some("h"),
                &["wide"],
                &[
                    ("data-sourcepos", "wrong"),
                    ("data-source", "wrong"),
                    ("data-cell", "wrong"),
                    ("x", "y"),
                ],
            ),
            number: None,
            span: Some(span.clone()),
        },
        Block::Raw {
            format: "html".into(),
            text: "<div>raw</div>".into(),
            span: Some(span.clone()),
        },
        Block::Cell(Cell {
            index: 3,
            show_input: true,
            source: "input".into(),
            attrs: attrs(Some("cell"), &[], &[("data-cell", "wrong")]),
            span: Some(span.clone()),
            ..Cell::default()
        }),
    ]);
    doc.includes = vec!["/other.md".into(), included];
    doc.meta.abstract_ = Some(vec![Block::Paragraph {
        inlines: vec![Inline::Code {
            text: "abstract".into(),
            attrs: Attrs::default(),
        }],
        span: Some(span),
    }]);
    let emitted = render(&doc, &Config::default(), true).0;
    assert_eq!(
        emitted.body,
        "<h2 id=\"h\" class=\"wide\" data-x=\"y\" data-sourcepos=\"7-9\" data-source=\"2\" data-cell=\"3\">Heading</h2>\n<div>raw</div>\n<div id=\"cell\" class=\"cell\" data-cell=\"3\" data-sourcepos=\"7-9\" data-source=\"2\">\n<div class=\"cell-input\">\n<div class=\"cell-prompt\">In [ ]:</div>\n<pre class=\"hl-code\"><code><span class=\"line\">input</span></code></pre>\n</div>\n</div>\n"
    );
    assert!(!emitted.title_block.contains("data-sourcepos"));
    let unlisted = document(vec![Block::Paragraph {
        inlines: text("Unknown include"),
        span: Some(SourceSpan {
            start_line: 2,
            end_line: 2,
            file: Some("/unknown.md".into()),
            cell: None,
        }),
    }]);
    assert_eq!(
        render(&unlisted, &Config::default(), true).0.body,
        "<p data-sourcepos=\"2-2\" data-source=\"0\">Unknown include</p>\n"
    );
}

#[test]
fn localization_and_title_overrides() {
    for (lang, expected) in [
        ("de", "Abbildung\u{a0}2:"),
        ("fr", "Fig.\u{a0}2.\u{a0}–"),
        ("ru", "Рис.\u{a0}2."),
        ("zh", "图\u{a0}2\u{2003}"),
    ] {
        let mut doc = document(vec![Block::Figure {
            kind: FigureKind::Other,
            caption: Some(text("Caption")),
            body: vec![],
            attrs: Attrs::default(),
            number: Some("2".into()),
            span: None,
        }]);
        doc.meta.lang = Some(lang.into());
        let emitted = render(&doc, &Config::default(), false).0;
        let tail = if lang == "zh" { "" } else { " " };
        assert!(
            emitted.body.contains(&format!(
                "<span class=\"caption-label\">{expected}</span>{tail}Caption"
            )),
            "{lang}: {}",
            emitted.body
        );
    }
    let mut doc = document(vec![
        Block::Toc { depth: 2, span: None },
        Block::Bibliography { span: None },
        Block::Figure {
            kind: FigureKind::Other,
            caption: Some(text("Caption")),
            body: vec![],
            attrs: Attrs::with_id("fig-custom"),
            number: Some("1".into()),
            span: None,
        },
    ]);
    doc.meta.lang = Some("de".into());
    doc.meta.abstract_ = Some(vec![paragraph("Abstract")]);
    doc.meta.vars = json!({"toc-title": "Index", "abstract-title": "Summary", "reference-section-title": "References"}).as_object().unwrap().clone();
    doc.resolved.toc.push(TocEntry {
        level: 1,
        number: None,
        inlines: text("Original heading"),
        anchor: "refs".into(),
    });
    doc.resolved.bibliography = Some(Bibliography {
        style: "ieee".into(),
        hanging_indent: false,
        entries: vec![],
    });
    doc.resolved.labels.insert(
        "fig-custom".into(),
        LabelInfo {
            kind: LabelKind::Figure,
            number: Some("1".into()),
            supplement: "Plot".into(),
            text: None,
            span: None,
        },
    );
    let emitted = render(&doc, &Config::default(), false).0;
    assert!(
        emitted.title_block.contains("<p class=\"abstract-title\">Summary</p>")
    );
    assert!(emitted.body.contains("<p class=\"toc-title\">Index</p>"));
    assert!(emitted.body.contains("<a href=\"#refs\">References</a>"));
    assert!(
        emitted.body.contains("<h1 class=\"references-title\">References</h1>")
    );
    assert!(
        emitted.body.contains(
            "<span class=\"caption-label\">Plot\u{a0}1:</span> Caption"
        )
    );
    assert!(!emitted.body.contains("hanging-indent"));
}

#[test]
fn cell_footnote_plan_visits_only_the_selected_output() {
    let selected =
        Block::paragraph(vec![note(2, vec![paragraph("Chosen note")])]);
    let skipped = vec![
        Block::Raw { format: "latex".into(), text: "skip".into(), span: None },
        Block::paragraph(vec![note(1, vec![paragraph("Skipped note")])]),
    ];
    let doc = document(vec![
        Block::Cell(Cell {
            index: 0,
            show_output: true,
            outputs: vec![output(
                OutputKind::Display,
                None,
                vec![("text/latex", skipped), ("text/html", vec![selected])],
            )],
            ..Cell::default()
        }),
        Block::paragraph(vec![note_ref(2)]),
    ]);
    let emitted = render(&doc, &Config::default(), false).0;
    assert!(
        !emitted.main.contains("Skipped note")
            && !emitted.main.contains("id=\"fn-1\"")
    );
    assert!(emitted.footnotes.contains("id=\"fn-2\""));
    assert!(emitted.body.contains("id=\"fnref-2-2\""));
    assert_eq!(emitted.footnotes.matches("class=\"footnote-back\"").count(), 2);
}

#[test]
fn attrs_are_forwarded_by_each_attributed_element() {
    let pair = attrs(
        None,
        &[],
        &[
            ("custom-style", "skip"),
            ("ansi", "skip"),
            ("lang", "de"),
            ("x", "keep"),
        ],
    );
    let blocks = vec![
        Block::Math {
            source: "x".into(),
            dialect: MathDialect::Latex,
            attrs: pair.clone(),
            number: None,
            span: None,
        },
        Block::Table {
            columns: vec![],
            head: vec![],
            body: vec![],
            foot: vec![],
            attrs: pair.clone(),
            span: None,
        },
        Block::Figure {
            kind: FigureKind::Other,
            caption: None,
            body: vec![],
            attrs: pair.clone(),
            number: None,
            span: None,
        },
        Block::Callout {
            kind: "note".into(),
            title: None,
            blocks: vec![],
            attrs: pair.clone(),
            span: None,
        },
        Block::Cell(Cell {
            show_input: true,
            source: "x".into(),
            attrs: pair.clone(),
            ..Cell::default()
        }),
        Block::paragraph(vec![
            Inline::Code { text: "x".into(), attrs: pair.clone() },
            Inline::Link {
                inlines: text("x"),
                url: "u".into(),
                title: None,
                attrs: pair.clone(),
            },
            Inline::Image {
                alt: text("x"),
                url: "u".into(),
                title: None,
                attrs: pair.clone(),
            },
            Inline::Span { inlines: text("x"), attrs: pair },
        ]),
    ];
    let html = body(blocks);
    assert_eq!(html.matches("lang=\"de\" data-x=\"keep\"").count(), 9);
    assert!(!html.contains("custom-style") && !html.contains("data-ansi"));
}

#[test]
fn abstract_notes_precede_body_notes_and_keep_abstract_positions_off() {
    let span =
        SourceSpan { start_line: 3, end_line: 3, cell: None, file: None };
    let mut doc = document(vec![Block::Paragraph {
        inlines: vec![
            reference(
                "x",
                "x",
                vec![
                    Inline::text("Ref"),
                    note(
                        2,
                        vec![Block::Paragraph {
                            inlines: text("Body note"),
                            span: Some(span.clone()),
                        }],
                    ),
                ],
            ),
            note_ref(1),
        ],
        span: Some(span.clone()),
    }]);
    doc.meta.abstract_ = Some(vec![Block::Paragraph {
        inlines: vec![note(
            1,
            vec![Block::Paragraph {
                inlines: text("Abstract note"),
                span: Some(span),
            }],
        )],
        span: None,
    }]);
    let emitted = render(&doc, &Config::default(), true).0;
    assert!(emitted.title_block.contains("id=\"fnref-1\""));
    assert!(emitted.body.contains("<a class=\"ref\" href=\"#x\">Ref</a><sup class=\"footnote-ref\"><a id=\"fnref-2\""));
    assert!(emitted.body.contains("id=\"fnref-1-2\""));
    assert!(
        emitted.footnotes.find("id=\"fn-1\"").unwrap()
            < emitted.footnotes.find("id=\"fn-2\"").unwrap()
    );
    assert!(
        emitted
            .footnotes
            .contains("<p>Abstract note <a class=\"footnote-back\"")
    );
    assert!(
        emitted
            .footnotes
            .contains("<p data-sourcepos=\"3-3\" data-source=\"0\">Body note")
    );
    assert!(!emitted.title_block.contains("data-sourcepos="));
}

#[test]
fn spanless_children_do_not_inherit_position_attributes() {
    let doc = document(vec![Block::Div {
        attrs: Attrs::default(),
        blocks: vec![paragraph("No own span")],
        span: Some(SourceSpan {
            start_line: 1,
            end_line: 3,
            cell: None,
            file: None,
        }),
    }]);
    let emitted = render(&doc, &Config::default(), true).0;
    assert_eq!(
        emitted.body,
        "<div data-sourcepos=\"1-3\" data-source=\"0\">\n<p>No own span</p>\n</div>\n"
    );
    assert_eq!(
        render(&doc, &Config::default(), false).0.body,
        "<div>\n<p>No own span</p>\n</div>\n"
    );
}
