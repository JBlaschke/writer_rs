//! Deterministic, style-free LaTeX body emission from resolved IR.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::assets::export::{self, LatexAssetName};
use crate::assets::registry::AssetRegistry;
use crate::config::{Config, LatexEngine, LatexTopLevel, LatexUnicode};
use crate::error::{Diagnostic, Diagnostics, Level};
use crate::ir::{Attrs, Block, Document, Inline, SourceSpan};
use crate::resolve::l10n::{LangTag, Term, term};

use super::escape;

mod block;
mod code;
mod figure;
mod highlight;
mod inline;
mod math;
mod output;
mod table;

use output::Output;

/// LaTeX text and the resources used by its abstract and body.
pub struct Emitted {
    /// Body text, with one final line feed when nonempty.
    pub body: String,
    /// Boxed abstract text, independent of the body.
    pub abstract_: String,
    /// The lifted automatic table-of-contents depth, or zero.
    pub toc: u8,
    /// Package and counter requirements for a document template.
    pub usage: Usage,
    /// Actual image uses, unique in abstract-then-body order.
    pub images: Vec<AssetFile>,
    /// Non-YAML bibliography sources and their reserved basenames.
    pub bibliography: Vec<(PathBuf, String)>,
    /// Located findings; recoverable content remains visible.
    pub diagnostics: Diagnostics,
}

/// An asset to copy or render alongside the exported document.
pub struct AssetFile {
    /// Registered asset identity.
    pub id: String,
    /// Portable basename in the shared sidecar directory.
    pub name: String,
    /// Whether the original bytes need a PDF rendition.
    pub render: bool,
}

/// Features used by emitted content, in deterministic first-use order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    /// Images use graphicx.
    pub graphics: bool,
    /// At least one longtable is emitted.
    pub longtable: bool,
    /// At least one table is emitted.
    pub table: bool,
    /// Captions or caption configuration are emitted.
    pub caption: bool,
    /// A verbatim, highlighted or listings environment is emitted.
    pub code: bool,
    /// A writercode environment is emitted.
    pub highlighted: bool,
    /// A lstlisting environment is emitted.
    pub listings: bool,
    /// Underline or strikeout commands are emitted.
    pub ulem: bool,
    /// Notebook prompts use writercell.
    pub cells: bool,
    /// Captions use the writerlisting counter.
    pub listing_caption: bool,
    /// Counterless longtable support is needed.
    pub uncaptioned_longtable: bool,
    /// Math, math Unicode replacements or task boxes are emitted.
    pub math: bool,
    /// Additional math packages, unique in first-use order.
    pub math_packages: Vec<&'static str>,
    /// Highlighted code or math uses color.
    pub color: bool,
    /// Unicode definitions, unique in first-use order; math mappings win.
    pub newunicodechar: Vec<(char, &'static str)>,
    /// Deepest numbered heading depth, when any.
    pub secnumdepth: Option<i8>,
    /// Lifted automatic table-of-contents depth, when any.
    pub tocdepth: Option<i8>,
}

#[derive(Clone, Copy, Default)]
struct Context {
    boxed: bool,
    footnote: bool,
    moving: bool,
    compact: bool,
    omit_notes: bool,
    abstract_: bool,
    table_p: bool,
    group: bool,
    figure_image: bool,
    bullet_depth: usize,
    ordered_depth: usize,
    list_depth: usize,
}

#[derive(Clone, Copy)]
struct Caption<'a> {
    caption: Option<&'a [Inline]>,
    attrs: &'a Attrs,
    numbered: bool,
    span: Option<&'a SourceSpan>,
}

struct Emitter<'a> {
    document: &'a Document,
    registry: &'a AssetRegistry,
    config: &'a Config,
    stem: &'a str,
    names: BTreeMap<String, LatexAssetName>,
    images: Vec<AssetFile>,
    bibliography: Vec<(PathBuf, String)>,
    usage: Usage,
    diagnostics: Diagnostics,
    span: Option<SourceSpan>,
    lang: LangTag,
    map: bool,
    note_refs: BTreeSet<u32>,
    math_span: Option<SourceSpan>,
    unicode: Vec<char>,
    unicode_span: Option<SourceSpan>,
    once: BTreeSet<String>,
    subfigure_labels: Vec<String>,
}

/// Emits content and a pure resource plan without I/O or compiler evaluation.
pub fn emit(
    document: &Document, registry: &AssetRegistry, config: &Config, stem: &str,
) -> Emitted {
    let mut assets = export::assets(document, registry);
    let bibliography = export::bibliography(&mut assets.namespace, config);
    let map = config.latex.unicode == LatexUnicode::Map
        || (config.latex.unicode == LatexUnicode::Auto
            && config.latex.engine == LatexEngine::Pdflatex);
    let mut emitter = Emitter {
        document,
        registry,
        config,
        stem,
        names: assets.latex,
        images: Vec::new(),
        bibliography,
        usage: Usage::default(),
        diagnostics: Diagnostics::new(),
        span: None,
        lang: LangTag::effective(&document.meta, config)
            .unwrap_or_else(|| LangTag::parse("en").unwrap()),
        map,
        note_refs: BTreeSet::new(),
        math_span: None,
        unicode: Vec::new(),
        unicode_span: None,
        once: BTreeSet::new(),
        subfigure_labels: Vec::new(),
    };
    visit(document, config, &mut |inline| {
        if let Inline::FootnoteRef { number: Some(number), .. } = inline {
            emitter.note_refs.insert(*number);
        }
    });
    let abstract_ =
        document.meta.abstract_.as_deref().map_or_else(String::new, |blocks| {
            emitter
                .blocks(
                    blocks,
                    Context {
                        boxed: true,
                        abstract_: true,
                        ..Context::default()
                    },
                    false,
                )
                .wrapped(config.latex.wrap)
        });
    let (toc, body) = match document.blocks.first() {
        Some(Block::Toc { depth, span: None }) => {
            emitter.usage.tocdepth = Some(*depth as i8 + emitter.offset());
            (*depth, &document.blocks[1..])
        }
        _ => (0, document.blocks.as_slice()),
    };
    let mut rendered = Vec::new();
    for block in body {
        let output = emitter.block(block, Context::default());
        if !output.is_empty() {
            rendered.push(output.wrapped(config.latex.wrap));
        }
    }
    let body = final_text(rendered.join("\n\n"));
    emitter.finish();
    Emitted {
        body,
        abstract_: final_text(abstract_),
        toc,
        usage: emitter.usage,
        images: emitter.images,
        bibliography: emitter.bibliography,
        diagnostics: emitter.diagnostics,
    }
}

impl Emitter<'_> {
    fn has_notes(&self, inlines: &[Inline]) -> bool {
        let mut found = false;
        visit_inlines(inlines, self.document, self.config, &mut |inline| {
            found |= matches!(
                inline,
                Inline::Footnote { .. }
                    | Inline::FootnoteRef { number: Some(_), .. }
            );
        });
        found
    }

    fn offset(&self) -> i8 {
        match self.config.latex.top_level {
            LatexTopLevel::Section => 0,
            LatexTopLevel::Chapter => -1,
            LatexTopLevel::Part => -2,
        }
    }

    fn finding(&mut self, level: Level, code: &'static str, message: String) {
        let path = self
            .span
            .as_ref()
            .and_then(|span| span.file.as_ref())
            .unwrap_or(&self.document.source.path);
        let mut finding = Diagnostic::new(level, code, message).with_path(path);
        if let Some(span) = &self.span {
            finding = finding.with_span(span.clone());
        }
        self.diagnostics.push(finding);
    }

    fn once(
        &mut self, key: String, level: Level, code: &'static str,
        message: String,
    ) {
        if self.once.insert(key) {
            self.finding(level, code, message);
        }
    }

    fn label(&self, id: Option<&str>, numbered: bool) -> String {
        id.map_or_else(String::new, |id| {
            format!(
                "{}\\label{{{}}}",
                if numbered { "" } else { "\\phantomsection" },
                crate::resolve::anchor(id),
            )
        })
    }

    fn blocks(
        &mut self, blocks: &[Block], context: Context, tight: bool,
    ) -> Output {
        let mut result = Output::default();
        for block in blocks {
            let output = self.block(block, context);
            if output.is_empty() {
                continue;
            }
            if !result.is_empty() {
                result.literal(if tight { "\n" } else { "\n\n" });
            }
            result.append(output);
        }
        result
    }
}

fn final_text(text: String) -> String {
    if text.is_empty() {
        text
    } else {
        format!("{}\n", text.trim_end_matches('\n'))
    }
}

fn visit(
    document: &Document, config: &Config, on_inline: &mut dyn FnMut(&Inline),
) {
    if let Some(blocks) = &document.meta.abstract_ {
        visit_blocks(blocks, document, config, on_inline);
    }
    visit_blocks(&document.blocks, document, config, on_inline);
}

fn visit_blocks(
    blocks: &[Block], document: &Document, config: &Config,
    visit: &mut dyn FnMut(&Inline),
) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. }
            | Block::Paragraph { inlines, .. } => {
                visit_inlines(inlines, document, config, visit);
            }
            Block::BlockQuote { blocks, .. } | Block::Div { blocks, .. } => {
                visit_blocks(blocks, document, config, visit);
            }
            Block::Figure { body, caption, .. } => {
                visit_blocks(body, document, config, visit);
                if let Some(caption) = caption {
                    visit_inlines(caption, document, config, visit);
                }
            }
            Block::Callout { title, blocks, .. } => {
                if let Some(title) = title {
                    visit_inlines(title, document, config, visit);
                }
                visit_blocks(blocks, document, config, visit);
            }
            Block::List { items, .. } => {
                for item in items {
                    visit_blocks(&item.blocks, document, config, visit);
                }
            }
            Block::DefinitionList { items, .. } => {
                for item in items {
                    visit_inlines(&item.term, document, config, visit);
                    for blocks in &item.definitions {
                        visit_blocks(blocks, document, config, visit);
                    }
                }
            }
            Block::Table { columns, head, body, foot, .. } => {
                for cell in
                    table::rendered_cells(columns.len(), head, body, foot)
                {
                    visit_inlines(&cell.inlines, document, config, visit);
                }
            }
            Block::Cell(cell) if cell.show_output => {
                for output in &cell.outputs {
                    visit_blocks(output.pick("latex"), document, config, visit);
                }
            }
            _ => {}
        }
    }
}

fn visit_inlines(
    inlines: &[Inline], document: &Document, config: &Config,
    visit: &mut dyn FnMut(&Inline),
) {
    for inline in inlines {
        visit(inline);
        match inline {
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Span { inlines, .. } => {
                visit_inlines(inlines, document, config, visit);
            }
            Inline::Link { inlines, url, .. } => {
                let plain = crate::ir::plain_text(inlines);
                let bare = plain == *url
                    || url.strip_prefix("mailto:") == Some(plain.as_str());
                if url.starts_with('#') || !bare {
                    visit_inlines(inlines, document, config, visit);
                }
            }
            Inline::Footnote { blocks, .. } => {
                visit_blocks(blocks, document, config, visit);
            }
            Inline::Ref {
                target,
                supplement,
                resolved: Some(resolved),
                form,
                ..
            } => {
                let inlines = if *form == crate::ir::RefForm::Normal
                    && document
                        .resolved
                        .labels
                        .get(target)
                        .is_some_and(|info| info.number.is_none())
                {
                    Some(resolved.text.as_slice())
                } else {
                    supplement.as_deref()
                };
                if let Some(inlines) = inlines {
                    visit_inlines(inlines, document, config, visit);
                }
            }
            Inline::Cite { items, mode, .. }
                if *mode != crate::ir::CiteMode::None
                    && !(config.latex.cite
                        == crate::config::LatexCite::Natbib
                        && *mode == crate::ir::CiteMode::Author) =>
            {
                for item in items {
                    if let Some(prefix) = &item.prefix {
                        visit_inlines(prefix, document, config, visit);
                    }
                    if let Some(suffix) = &item.suffix {
                        visit_inlines(suffix, document, config, visit);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LatexCite;
    use crate::ir::{
        Cell, CellOutput, CiteItem, CiteMode, DocMeta, Features, LabelInfo,
        LabelKind, OutputCandidate, RefForm, Resolved, ResolvedRef, SourceInfo,
        SourceKind,
    };

    fn document(blocks: Vec<Block>) -> Document {
        Document {
            meta: DocMeta::default(),
            blocks,
            features: Features::default(),
            source: SourceInfo {
                path: "/book/main.md".into(),
                base_dir: "/book".into(),
                kind: SourceKind::Markdown,
            },
            assets: Vec::new(),
            includes: Vec::new(),
            resolved: Resolved::default(),
        }
    }

    fn note_ref(number: u32) -> Inline {
        Inline::FootnoteRef {
            label: format!("note-{number}"),
            number: Some(number),
        }
    }

    fn refs(document: &Document, config: &Config) -> Vec<u32> {
        let mut result = Vec::new();
        visit(document, config, &mut |inline| {
            if let Inline::FootnoteRef { number: Some(number), .. } = inline {
                result.push(*number);
            }
        });
        result
    }

    fn citation(mode: CiteMode, number: u32) -> Inline {
        Inline::Cite {
            mode,
            items: vec![CiteItem {
                key: "a".into(),
                prefix: Some(vec![note_ref(number)]),
                suffix: None,
                locator: None,
            }],
            resolved: None,
            span: None,
        }
    }

    #[test]
    fn note_previsit_is_abstract_first_and_selects_only_visible_latex_outputs()
    {
        let visible = Cell {
            show_output: true,
            outputs: vec![CellOutput {
                candidates: vec![
                    OutputCandidate {
                        mime: "text/html".into(),
                        blocks: vec![
                            Block::Raw {
                                format: "html".into(),
                                text: "html".into(),
                                span: None,
                            },
                            Block::paragraph(vec![note_ref(2)]),
                        ],
                    },
                    OutputCandidate {
                        mime: "text/plain".into(),
                        blocks: vec![Block::paragraph(vec![note_ref(3)])],
                    },
                ],
                ..CellOutput::default()
            }],
            ..Cell::default()
        };
        let hidden = Cell { show_output: false, ..visible.clone() };
        let mut document =
            document(vec![Block::Cell(visible), Block::Cell(hidden)]);
        document.meta.abstract_ =
            Some(vec![Block::paragraph(vec![note_ref(1)])]);
        let before = document.clone();
        assert_eq!(refs(&document, &Config::default()), [1, 3]);
        assert_eq!(document, before);
    }

    #[test]
    fn note_previsit_uses_page_supplements_and_only_normal_unnumbered_cached_text()
     {
        let reference = |target: &str,
                         form: RefForm,
                         supplement: Option<u32>,
                         cached: u32,
                         resolved: bool| Inline::Ref {
            target: target.into(),
            form,
            supplement: supplement.map(|n| vec![note_ref(n)]),
            resolved: resolved.then(|| ResolvedRef {
                kind: LabelKind::Heading,
                number: "ignored".into(),
                text: vec![note_ref(cached)],
                anchor: "unused-cache".into(),
            }),
            span: None,
        };
        let mut document = document(vec![Block::paragraph(vec![
            reference("numbered", RefForm::Normal, Some(4), 5, true),
            reference("numbered", RefForm::Normal, None, 6, true),
            reference("unnumbered", RefForm::Normal, Some(7), 8, true),
            reference("unnumbered", RefForm::Page, Some(9), 10, true),
            reference("unnumbered", RefForm::Page, None, 11, true),
            reference("missing", RefForm::Normal, Some(12), 13, false),
        ])]);
        for (target, number) in
            [("numbered", Some("1".into())), ("unnumbered", None)]
        {
            document.resolved.labels.insert(
                target.into(),
                LabelInfo {
                    kind: LabelKind::Heading,
                    number,
                    supplement: "Section".into(),
                    text: None,
                    span: None,
                },
            );
        }
        assert_eq!(refs(&document, &Config::default()), [4, 8, 9]);
    }

    #[test]
    fn note_previsit_ignores_dropped_citation_notes_and_bare_link_children() {
        let link = |url: &str, title: &str, number| Inline::Link {
            url: url.into(),
            inlines: vec![Inline::text(title), note_ref(number)],
            title: None,
            attrs: Attrs::default(),
        };
        let document = document(vec![Block::paragraph(vec![
            citation(CiteMode::None, 14),
            citation(CiteMode::Author, 15),
            citation(CiteMode::Normal, 16),
            link("https://example.org", "https://example.org", 17),
            link("https://example.org", "Named link", 18),
            link("#target", "#target", 19),
            Inline::Image {
                url: "missing.png".into(),
                alt: vec![note_ref(20)],
                title: None,
                attrs: Attrs::default(),
            },
        ])]);
        let mut config = Config::default();
        config.latex.cite = LatexCite::Natbib;
        assert_eq!(refs(&document, &config), [16, 18, 19]);
        config.latex.cite = LatexCite::Biblatex;
        assert_eq!(refs(&document, &config), [15, 16, 18, 19]);
    }

    #[test]
    fn hidden_reference_notes_do_not_add_labels_to_an_unrepeated_owner() {
        for (style, mode) in [
            (LatexCite::Biblatex, CiteMode::None),
            (LatexCite::Natbib, CiteMode::Author),
        ] {
            let document = document(vec![Block::paragraph(vec![
                Inline::Footnote {
                    blocks: vec![Block::paragraph(vec![Inline::text(
                        "Owner note",
                    )])],
                    label: Some("owner".into()),
                    number: Some(1),
                },
                citation(mode, 1),
            ])]);
            let mut config = Config::default();
            config.latex.cite = style;
            let emitted =
                emit(&document, &AssetRegistry::new(), &config, "paper");
            assert!(emitted.body.contains("\\footnote{Owner note}"));
            assert!(!emitted.body.contains("writer-fn-1"));
            assert!(!emitted.body.contains("\\footref"));
        }
    }

    #[test]
    fn driver_lifts_only_the_first_automatic_toc_and_preserves_inputs_and_final_lf()
     {
        let mut registry = AssetRegistry::new();
        let abstract_asset =
            registry.register_bytes(vec![1], "image/png", "abstract.png");
        let body_asset =
            registry.register_bytes(vec![2], "image/png", "body.png");
        let image = |id: &str| {
            Block::paragraph(vec![Inline::Image {
                url: format!("asset:{id}"),
                alt: vec![Inline::text("Alt")],
                title: None,
                attrs: Attrs::default(),
            }])
        };
        let mut document = document(vec![
            Block::Toc { depth: 2, span: None },
            image(&body_asset),
            Block::Raw {
                format: "html".into(),
                text: "SKIPPED".into(),
                span: None,
            },
            Block::Toc {
                depth: 4,
                span: Some(SourceSpan {
                    start_line: 9,
                    end_line: 10,
                    cell: None,
                    file: None,
                }),
            },
        ]);
        document.meta.abstract_ = Some(vec![
            Block::Heading {
                level: 1,
                inlines: vec![Inline::text("Abstract heading")],
                attrs: Attrs::with_id("abstract-heading"),
                number: None,
                span: None,
            },
            image(&abstract_asset),
        ]);
        let mut config = Config::default();
        config.latex.top_level = LatexTopLevel::Part;
        let before = (document.clone(), registry.clone(), config.clone());
        let emitted = emit(&document, &registry, &config, "paper");
        assert_eq!(emitted.toc, 2);
        assert_eq!(emitted.usage.tocdepth, Some(0));
        assert_eq!(emitted.body.matches("\\tableofcontents").count(), 1);
        assert!(
            emitted
                .body
                .contains("\\setcounter{tocdepth}{2}\n\\tableofcontents")
        );
        assert!(!emitted.abstract_.contains("\\addcontentsline"));
        assert_eq!(
            emitted
                .images
                .iter()
                .map(|image| image.id.as_str())
                .collect::<Vec<_>>(),
            [abstract_asset.as_str(), body_asset.as_str()]
        );
        for text in [&emitted.body, &emitted.abstract_] {
            assert!(!text.starts_with('\n'));
            assert!(text.ends_with('\n'));
            assert!(!text.ends_with("\n\n"));
        }
        assert_eq!((document, registry, config), before);
    }

    #[test]
    fn moving_short_forms_follow_the_rendered_reference_and_citation_branch() {
        let note = || Inline::Footnote {
            blocks: vec![Block::paragraph(vec![Inline::text("Actual note")])],
            label: None,
            number: Some(1),
        };
        let reference = |form: RefForm, supplement| Inline::Ref {
            target: "target".into(),
            form,
            supplement,
            resolved: Some(ResolvedRef {
                kind: LabelKind::Heading,
                number: "unused".into(),
                text: vec![note()],
                anchor: "unused".into(),
            }),
            span: None,
        };
        let cite = |mode| Inline::Cite {
            items: vec![CiteItem {
                key: "a".into(),
                prefix: Some(vec![note()]),
                locator: None,
                suffix: None,
            }],
            mode,
            resolved: None,
            span: None,
        };
        for (style, inline, short) in [
            (LatexCite::Biblatex, reference(RefForm::Normal, None), false),
            (
                LatexCite::Biblatex,
                reference(RefForm::Normal, Some(vec![note()])),
                true,
            ),
            (LatexCite::Biblatex, reference(RefForm::Page, None), false),
            (
                LatexCite::Biblatex,
                reference(RefForm::Page, Some(vec![note()])),
                true,
            ),
            (LatexCite::Biblatex, cite(CiteMode::None), false),
            (LatexCite::Natbib, cite(CiteMode::Author), false),
            (LatexCite::Biblatex, cite(CiteMode::Author), true),
        ] {
            let mut document = document(vec![Block::Heading {
                level: 1,
                inlines: vec![inline],
                attrs: Attrs::default(),
                number: Some("99".into()),
                span: None,
            }]);
            document.resolved.labels.insert(
                "target".into(),
                LabelInfo {
                    kind: LabelKind::Heading,
                    number: Some("1".into()),
                    supplement: "Section".into(),
                    text: None,
                    span: None,
                },
            );
            let mut config = Config::default();
            config.latex.cite = style;
            let emitted =
                emit(&document, &AssetRegistry::new(), &config, "paper");
            assert_eq!(
                emitted.body.starts_with("\\section["),
                short,
                "{}",
                emitted.body
            );
            assert_eq!(
                emitted.body.matches("\\footnote{").count(),
                usize::from(short)
            );
        }
    }

    #[test]
    fn moving_short_form_keeps_an_unresolved_note_reference_placeholder() {
        let document = document(vec![Block::Heading {
            level: 1,
            inlines: vec![
                Inline::Footnote {
                    blocks: vec![Block::paragraph(vec![Inline::text(
                        "Actual note",
                    )])],
                    label: None,
                    number: Some(1),
                },
                Inline::text(" and "),
                Inline::FootnoteRef { label: "missing".into(), number: None },
            ],
            attrs: Attrs::default(),
            number: Some("99".into()),
            span: None,
        }]);
        let emitted =
            emit(&document, &AssetRegistry::new(), &Config::default(), "paper");
        assert!(
            emitted.body.starts_with("\\section[ and {[}?missing{]}]{"),
            "{}",
            emitted.body
        );
        assert_eq!(emitted.body.matches("\\footnote{").count(), 1);
        assert_eq!(emitted.body.matches("{[}?missing{]}").count(), 2);
    }
}
