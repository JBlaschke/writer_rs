//! Deterministic, editable Word reference document (docx-output.md §6.5).

use std::path::Path;

use crate::assets::registry::AssetRegistry;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::ir::{
    Align, Attrs, Author, BibEntry, Bibliography, Block, Cell, CellOutput,
    CiteItem, CiteMode, ColumnSpec, DefinitionItem, DocMeta, Document,
    Features, FigureKind, Inline, Inlines, LabelInfo, LabelKind, ListItem,
    ListKind, MathDialect, OutputCandidate, OutputKind, RefForm, Resolved,
    ResolvedCite, ResolvedRef, Row, SourceInfo, SourceKind, TableCell,
    TocEntry,
};

use super::{DocxOptions, Picture, Renderer};

/// Builds a resolved style showcase without reading files or running a frontend.
///
/// The single asset is a built-in grayscale PNG. Numbers, labels, contents and
/// bibliography are fixed here so ejection needs neither resolution nor a World.
pub fn showcase_document() -> (Document, AssetRegistry) {
    let mut assets = AssetRegistry::new();
    let image = assets.register_bytes(
        GREY_PNG.to_vec(),
        "image/png",
        "showcase-grey.png",
    );
    let mut resolved = Resolved {
        bibliography: Some(Bibliography {
            style: "showcase".into(),
            hanging_indent: true,
            entries: vec![BibEntry {
                key: "writer-style-guide".into(),
                prefix: Some(text("[1]")),
                inlines: vec![
                    Inline::Text { text: "Writer. ".into() },
                    Inline::Emph { inlines: text("Bibliography style guide") },
                    Inline::Text {
                        text: ". This entry uses Bibliography.".into(),
                    },
                ],
            }],
        }),
        footnotes: 1,
        ..Resolved::default()
    };
    let mut blocks = vec![Block::Toc { depth: 6, span: None }];
    headings(&mut blocks, &mut resolved);
    blocks.extend([
        paragraph("This paragraph uses First Paragraph after a heading."),
        inline_samples(),
        Block::BlockQuote {
            blocks: vec![
                paragraph("This paragraph uses Block Text inside a quote."),
                Block::BlockQuote {
                    blocks: vec![paragraph(
                        "This nested quote also uses Block Text.",
                    )],
                    span: None,
                },
            ],
            span: None,
        },
    ]);
    blocks.extend(lists());
    blocks.push(Block::DefinitionList {
        items: vec![DefinitionItem {
            term: text("This term uses Definition Term."),
            definitions: vec![vec![paragraph(
                "This paragraph uses Definition.",
            )]],
        }],
        span: None,
    });
    blocks.extend(code_samples());
    blocks.extend(figures(&image, &mut resolved));
    blocks.push(Block::Math {
        source: r"\frac{a}{b}".into(),
        dialect: MathDialect::Latex,
        attrs: identified("showcase-equation"),
        number: Some("1".into()),
        span: None,
    });
    label(
        &mut resolved,
        "showcase-equation",
        LabelKind::Equation,
        Some("1"),
        "Equation",
        "Editable fraction",
    );
    blocks.push(Block::Paragraph {
        inlines: vec![
            Inline::Text {
                text: "The editable Equation and Normal Table frame above are "
                    .into(),
            },
            Inline::Ref {
                target: "showcase-equation".into(),
                supplement: None,
                form: RefForm::Normal,
                resolved: Some(ResolvedRef {
                    kind: LabelKind::Equation,
                    number: "1".into(),
                    text: text("Equation\u{a0}1"),
                    anchor: "showcase-equation".into(),
                }),
                span: None,
            },
            Inline::Text { text: ".".into() },
        ],
        span: None,
    });
    blocks.extend(callouts());
    blocks.push(notebook_cell());
    blocks.push(Block::Paragraph {
        inlines: vec![
            Inline::Text {
                text: "A resolved citation precedes Bibliography ".into(),
            },
            Inline::Cite {
                items: vec![CiteItem {
                    key: "writer-style-guide".into(),
                    ..CiteItem::default()
                }],
                mode: CiteMode::Normal,
                resolved: Some(ResolvedCite {
                    inlines: text("[1]"),
                    note: false,
                }),
                span: None,
            },
            Inline::Text { text: ".".into() },
        ],
        span: None,
    });
    blocks.push(Block::Bibliography { span: None });
    blocks.push(paragraph(
        "An empty Horizontal Rule paragraph follows. The footer uses a PAGE field.",
    ));
    blocks.push(Block::ThematicBreak { span: None });
    label(
        &mut resolved,
        "showcase-footnote",
        LabelKind::Footnote,
        Some("1"),
        "",
        "Footnote Text and Footnote Block Text",
    );
    let document = Document {
        meta: DocMeta {
            title: Some("Writer DOCX style showcase".into()),
            subtitle: Some("This paragraph uses Subtitle.".into()),
            authors: vec![Author {
                name: "This paragraph uses Author.".into(),
                affiliation: Some("Writer style examples".into()),
                email: Some("writer@example.invalid".into()),
                id: None,
            }],
            // An ISO date would also populate core created/modified properties.
            date: Some("This paragraph uses Date.".into()),
            abstract_: Some(vec![
                paragraph("This paragraph uses Abstract."),
                Block::Heading {
                    level: 2,
                    inlines: text("Abstract heading (unlisted)"),
                    attrs: Attrs {
                        classes: vec!["notoc".into(), "unnumbered".into()],
                        ..Attrs::default()
                    },
                    number: None,
                    span: None,
                },
                paragraph(
                    "This Abstract paragraph follows an unlisted heading.",
                ),
            ]),
            lang: Some("en".into()),
            vars: [
                ("abstract-title".into(), "Abstract Title style".into()),
                ("toc-title".into(), "TOC Heading style".into()),
                ("reference-section-title".into(), "Bibliography".into()),
            ]
            .into_iter()
            .collect(),
            ..DocMeta::default()
        },
        blocks,
        features: Features {
            math: true,
            code: true,
            notebook: true,
            citations: true,
            footnotes: true,
        },
        source: SourceInfo {
            path: "/__writer-showcase.md".into(),
            kind: SourceKind::Markdown,
            base_dir: "/".into(),
        },
        assets: assets.infos(Path::new("/")),
        includes: Vec::new(),
        resolved,
    };
    (document, assets)
}

/// Ejects a complete, deterministic DOCX reference using native PNG and OMML.
///
/// No source files, environment timestamps, compiler or renderer are consulted.
pub fn reference_docx() -> Result<Vec<u8>> {
    let (document, assets) = showcase_document();
    let config = Config::default();
    let output = super::write(
        &document,
        &assets,
        &mut NoRenderer,
        &DocxOptions {
            config: &config,
            reference: None,
            source_date_epoch: None,
            all_styles: true,
        },
    )?;
    if !output.diagnostics.is_empty() {
        let findings = output
            .diagnostics
            .iter()
            .map(|finding| format!("{}: {}", finding.code, finding.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(Error::Other(format!(
            "built-in DOCX reference produced diagnostics: {findings}",
        )));
    }
    Ok(output.bytes)
}

fn text(value: &str) -> Inlines {
    vec![Inline::Text { text: value.into() }]
}

fn paragraph(value: &str) -> Block {
    Block::Paragraph { inlines: text(value), span: None }
}

fn identified(id: &str) -> Attrs {
    Attrs { id: Some(id.into()), ..Attrs::default() }
}

fn label(
    resolved: &mut Resolved, id: &str, kind: LabelKind, number: Option<&str>,
    supplement: &str, title: &str,
) {
    resolved.labels.insert(
        id.into(),
        LabelInfo {
            kind,
            number: number.map(str::to_owned),
            supplement: supplement.into(),
            text: Some(title.into()),
            span: None,
        },
    );
}

fn headings(blocks: &mut Vec<Block>, resolved: &mut Resolved) {
    for level in 1..=6 {
        let id = format!("showcase-heading-{level}");
        let title = format!("Heading {level} (numbered)");
        let number = vec!["1"; usize::from(level)].join(".");
        blocks.push(Block::Heading {
            level,
            inlines: text(&title),
            attrs: identified(&id),
            number: Some(number.clone()),
            span: None,
        });
        label(
            resolved,
            &id,
            LabelKind::Heading,
            Some(&number),
            "Section",
            &title,
        );
        resolved.toc.push(TocEntry {
            level,
            number: Some(number),
            inlines: text(&title),
            anchor: id,
        });
        let id = format!("showcase-unlisted-{level}");
        let title = format!("Heading {level} (unlisted)");
        let mut attrs = identified(&id);
        attrs.classes = vec!["notoc".into(), "unnumbered".into()];
        blocks.push(Block::Heading {
            level,
            inlines: text(&title),
            attrs,
            number: None,
            span: None,
        });
        label(resolved, &id, LabelKind::Heading, None, "Section", &title);
    }
}

fn inline_samples() -> Block {
    Block::Paragraph {
        inlines: vec![
            Inline::Text { text: "This paragraph uses Body Text, ".into() },
            Inline::Code {
                text: "Verbatim Char".into(),
                attrs: Attrs::default(),
            },
            Inline::Text { text: ", and ".into() },
            Inline::Link {
                inlines: text("Hyperlink"),
                url: "https://example.invalid/writer".into(),
                title: Some("External Hyperlink example".into()),
                attrs: Attrs::default(),
            },
            Inline::Text { text: ". The internal ".into() },
            Inline::Link {
                inlines: text("heading link"),
                url: "#showcase-heading-1".into(),
                title: None,
                attrs: Attrs::default(),
            },
            Inline::Text { text: " and Footnote Reference also work".into() },
            Inline::Footnote {
                blocks: vec![
                    paragraph("This paragraph uses Footnote Text."),
                    Block::BlockQuote {
                        blocks: vec![paragraph(
                            "This quote uses Footnote Block Text.",
                        )],
                        span: None,
                    },
                ],
                label: Some("showcase-footnote".into()),
                number: Some(1),
            },
            Inline::Text { text: "; a repeated note reference follows".into() },
            Inline::FootnoteRef {
                label: "showcase-footnote".into(),
                number: Some(1),
            },
            Inline::Text { text: ".".into() },
        ],
        span: None,
    }
}

fn item(value: &str) -> ListItem {
    ListItem { blocks: vec![paragraph(value)], checked: None }
}

fn list(kind: ListKind, items: Vec<ListItem>) -> Block {
    Block::List { kind, tight: true, items, span: None }
}

fn lists() -> Vec<Block> {
    vec![
        list(
            ListKind::Bullet,
            vec![
                ListItem {
                    checked: None,
                    blocks: vec![
                        paragraph("This bullet uses Compact."),
                        list(
                            ListKind::Bullet,
                            vec![ListItem {
                                checked: None,
                                blocks: vec![
                                    paragraph(
                                        "This nested bullet uses Compact.",
                                    ),
                                    list(
                                        ListKind::Bullet,
                                        vec![item(
                                            "This third-level bullet uses Compact.",
                                        )],
                                    ),
                                ],
                            }],
                        ),
                    ],
                },
                ListItem::default(),
            ],
        ),
        list(
            ListKind::Ordered { start: 1 },
            vec![
                item("This ordered item uses Compact."),
                item("This Compact item continues the ordered sequence."),
            ],
        ),
        list(
            ListKind::Ordered { start: 7 },
            vec![
                item("This Compact ordered list restarts at seven."),
                ListItem::default(),
            ],
        ),
        list(
            ListKind::Bullet,
            vec![
                ListItem {
                    checked: Some(false),
                    blocks: vec![paragraph(
                        "This unchecked task uses Compact.",
                    )],
                },
                ListItem {
                    checked: Some(true),
                    blocks: vec![paragraph("This checked task uses Compact.")],
                },
            ],
        ),
    ]
}

fn code_samples() -> Vec<Block> {
    let mut blocks = Vec::new();
    for &(language, source) in CODE_SAMPLES {
        blocks.push(paragraph(&format!(
            "Source Code and Code Line Number: {language} syntax token styles.",
        )));
        blocks.push(Block::CodeBlock {
            lang: Some(language.into()),
            text: source.into(),
            attrs: Attrs {
                classes: vec!["numbered".into()],
                pairs: [("highlight".into(), "2".into())].into_iter().collect(),
                ..Attrs::default()
            },
            span: None,
        });
    }
    blocks
}

fn figures(image: &str, resolved: &mut Resolved) -> Vec<Block> {
    let image_body = || Block::Paragraph {
        inlines: vec![Inline::Image {
            alt: text("Built-in 64 by 32 grey PNG"),
            url: AssetRegistry::placeholder(image),
            title: Some("The same PNG demonstrates both figure styles.".into()),
            attrs: Attrs::default(),
        }],
        span: None,
    };
    for (id, kind, number, supplement, title) in [
        (
            "showcase-image",
            LabelKind::Figure,
            "1",
            "Figure",
            "Image Caption sample",
        ),
        (
            "showcase-table",
            LabelKind::Table,
            "1",
            "Table",
            "Table Caption sample",
        ),
        (
            "showcase-listing",
            LabelKind::Listing,
            "1",
            "Listing",
            "Caption sample",
        ),
        (
            "showcase-other",
            LabelKind::Figure,
            "2",
            "Figure",
            "Generic Caption sample",
        ),
    ] {
        label(resolved, id, kind, Some(number), supplement, title);
    }
    vec![
        paragraph("The uncaptioned PNG paragraph uses Figure."),
        Block::Figure {
            kind: FigureKind::Image,
            caption: None,
            body: vec![image_body()],
            attrs: Attrs::default(),
            number: None,
            span: None,
        },
        paragraph("The captioned PNG paragraph uses Captioned Figure."),
        Block::Figure {
            kind: FigureKind::Image,
            caption: Some(text("This caption uses Image Caption.")),
            body: vec![image_body()],
            attrs: identified("showcase-image"),
            number: Some("1".into()),
            span: None,
        },
        Block::Figure {
            kind: FigureKind::Table,
            caption: Some(text("This caption uses Table Caption.")),
            body: vec![table()],
            attrs: identified("showcase-table"),
            number: Some("1".into()),
            span: None,
        },
        Block::Figure {
            kind: FigureKind::Listing,
            caption: Some(text("This listing caption uses Caption.")),
            body: vec![Block::CodeBlock {
                lang: None,
                text: "This plain Source Code run uses Verbatim Char.".into(),
                attrs: Attrs::default(),
                span: None,
            }],
            attrs: identified("showcase-listing"),
            number: Some("1".into()),
            span: None,
        },
        Block::Figure {
            kind: FigureKind::Other,
            caption: Some(text("This generic figure uses Caption.")),
            body: vec![paragraph(
                "This figure contains ordinary First Paragraph text.",
            )],
            attrs: identified("showcase-other"),
            number: Some("2".into()),
            span: None,
        },
    ]
}

fn table() -> Block {
    let row = |left: &str, right: &str, header| Row {
        cells: [left, right]
            .into_iter()
            .map(|value| TableCell {
                inlines: text(value),
                header,
                ..TableCell::default()
            })
            .collect(),
    };
    Block::Table {
        columns: vec![
            ColumnSpec { align: Align::Left, width: Some(0.5) },
            ColumnSpec { align: Align::Right, width: Some(0.5) },
        ],
        head: vec![row("Table style", "Compact cell paragraphs", true)],
        body: vec![row("This editable table uses Table.", "64 × 32", false)],
        foot: vec![row("Footer row", "Compact", false)],
        attrs: Attrs::default(),
        span: None,
    }
}

fn callouts() -> Vec<Block> {
    let mut blocks = vec![Block::Div {
        attrs: Attrs {
            pairs: [("custom-style".into(), "Callout".into())]
                .into_iter()
                .collect(),
            ..Attrs::default()
        },
        blocks: vec![paragraph("This paragraph uses the base Callout style.")],
        span: None,
    }];
    for (kind, name) in [
        ("note", "Note"),
        ("tip", "Tip"),
        ("important", "Important"),
        ("warning", "Warning"),
        ("caution", "Caution"),
        ("example", "Example"),
    ] {
        blocks.push(Block::Callout {
            kind: kind.into(),
            title: Some(text(&format!("Callout Title: {name}"))),
            blocks: vec![paragraph(&format!(
                "This paragraph uses Callout {name}.",
            ))],
            attrs: Attrs::default(),
            span: None,
        });
    }
    blocks
}

fn notebook_cell() -> Block {
    Block::Cell(Cell {
        index: 0,
        id: Some("showcase-cell".into()),
        language: "python".into(),
        source: "print(\"Cell Prompt and Cell Output\")".into(),
        execution_count: Some(1),
        show_input: true,
        show_output: true,
        outputs: vec![CellOutput {
            kind: OutputKind::Result,
            execution_count: Some(1),
            candidates: vec![OutputCandidate {
                mime: "text/plain".into(),
                blocks: vec![Block::CodeBlock {
                    lang: None,
                    text: "This output uses Cell Output and Verbatim Char."
                        .into(),
                    attrs: Attrs::default(),
                    span: None,
                }],
            }],
            mimes: vec!["text/plain".into()],
        }],
        ..Cell::default()
    })
}

struct NoRenderer;

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("the reference needs no equation rendering".into()))
    }

    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("the reference needs no image rendering".into()))
    }

    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("the reference needs no Typst math export".into()))
    }
}

// These are syntax-highlighting samples, not programs executed by the showcase.
// JSON intentionally includes an invalid token to demonstrate ErrorTok.
const CODE_SAMPLES: &[(&str, &str)] = &[
    (
        "rust",
        r#"/// DocumentationTok: a documented type.
#[derive(Debug)]
pub struct Example {
    pub count: u32,
}
fn example(value: &Example) -> char {
    let hexadecimal = 0xff;
    let floating = 1.5;
    let enabled = true;
    let escaped = "StringTok\n";
    // CommentTok: ordinary comment.
    if enabled { 'x' } else { '\n' }
}"#,
    ),
    (
        "python",
        r#"import math
@staticmethod
def example(value: int = 7):
    """DocumentationTok: a documented function."""
    # CommentTok: ordinary comment.
    message = "StringTok\n"
    if value is not None:
        return print(value + len(message))"#,
    ),
    (
        "c",
        r#"#include <stdio.h>
#define SCALE 2
static const double value = 1.5;
char letter = 'x';
const char *message = "StringTok\n";
int example(int input) {
    return input + 0xff;
}"#,
    ),
    (
        "javascript",
        r#"const matcher = /SpecialStringTok\w+/gi;
function example(value) {
    let enabled = true;
    return enabled && matcher.test(value);
}"#,
    ),
    (
        "html",
        r#"<article class="AttributeTok" data-value="1">
    NormalTok &amp; text
</article>"#,
    ),
    (
        "json",
        r#"{"StringTok": true, "DecValTok": 7,
 "ErrorTok": invalid, "FloatTok": 1.5}"#,
    ),
];

// A grayscale PNG (color type 0), 64 × 32, with a darker one-pixel border.
const GREY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x20,
    0x08, 0x00, 0x00, 0x00, 0x00, 0x87, 0xf6, 0x21, 0x58, 0x00, 0x00, 0x00,
    0x25, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x48, 0xa0, 0x10, 0x30,
    0x24, 0x6c, 0xa0, 0x08, 0x8c, 0x1a, 0x30, 0x6a, 0xc0, 0xa8, 0x01, 0xa3,
    0x06, 0x8c, 0x1a, 0x30, 0x6a, 0xc0, 0xa8, 0x01, 0xd4, 0x37, 0x80, 0x42,
    0x00, 0x00, 0x9d, 0x56, 0x45, 0x8c, 0x70, 0xc9, 0x3b, 0x3c, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];
