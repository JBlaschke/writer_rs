//! Block layout and restoration of nested rendering contexts.

use crate::config::LatexCite;
use crate::ir::{Cell, DefinitionItem, ListItem, ListKind, OutputKind};

use super::*;

impl Emitter<'_> {
    pub(super) fn block(&mut self, block: &Block, context: Context) -> Output {
        let previous = self.span.clone();
        self.span = block.span().cloned().or(previous.clone());
        let output = match block {
            Block::Paragraph { inlines, .. } => self.inlines(inlines, context),
            Block::Heading { level, inlines, attrs, number, .. } => {
                self.heading(*level, inlines, attrs, number.is_some(), context)
            }
            Block::CodeBlock { lang, text, attrs, .. } => {
                self.code(lang.as_deref(), text, attrs, context, None)
            }
            Block::Math { source, dialect, attrs, number, .. } => {
                self.display_math(source, *dialect, attrs, number.is_some())
            }
            Block::Figure { kind, caption, body, attrs, number, .. } => self
                .figure(
                    *kind,
                    caption.as_deref(),
                    body,
                    attrs,
                    number.is_some(),
                    context,
                ),
            Block::Table { columns, head, body, foot, attrs, .. } => {
                self.table(columns, head, body, foot, attrs, context, None)
            }
            Block::BlockQuote { blocks, .. } => {
                let mut output: Output = "\\begin{quote}\n".to_owned().into();
                output.append(self.blocks(blocks, context, false));
                output.literal("\n\\end{quote}");
                output
            }
            Block::List { kind, tight, items, .. } => {
                self.list(kind, *tight, items, context)
            }
            Block::DefinitionList { items, .. } => {
                self.definitions(items, context)
            }
            Block::Callout { kind, title, blocks, attrs, .. } => {
                let mut output = self.label(attrs.id.as_deref(), false);
                if !output.is_empty() {
                    output.push('\n');
                }
                let safe_kind: String = kind
                    .to_ascii_lowercase()
                    .chars()
                    .filter(char::is_ascii_alphanumeric)
                    .collect();
                let safe_kind = if safe_kind.is_empty() {
                    "note"
                } else {
                    &safe_kind
                };
                output.push_str(&format!(
                    "\\begin{{writercallout}}{{{}}}{{",
                    escape::text(safe_kind, self.map)
                ));
                let mut output: Output = output.into();
                let compact = Context { compact: true, ..context };
                let title =
                    title.as_ref().map(|title| self.inlines(title, compact));
                if let Some(title) = title.filter(|title| !title.is_empty()) {
                    output.append(title);
                } else {
                    let title = match kind.to_ascii_lowercase().as_str() {
                        "note" => term(&self.lang, Term::Note),
                        "tip" => term(&self.lang, Term::Tip),
                        "important" => term(&self.lang, Term::Important),
                        "warning" => term(&self.lang, Term::Warning),
                        "caution" => term(&self.lang, Term::Caution),
                        _ if kind.is_empty() => term(&self.lang, Term::Note),
                        _ => {
                            let mut chars = kind.chars();
                            chars
                                .next()
                                .into_iter()
                                .flat_map(char::to_uppercase)
                                .chain(chars)
                                .collect()
                        }
                    };
                    self.unicode(&title, super::math::UnicodeContext::Text);
                    output.prose(&title, self.map);
                }
                output.literal("}\n");
                output.append(self.blocks(
                    blocks,
                    Context { boxed: true, ..context },
                    false,
                ));
                output.literal("\n\\end{writercallout}");
                output
            }
            Block::Div { attrs, blocks, .. } => {
                self.div(attrs, blocks, context)
            }
            Block::Raw { format, text, .. } if format == "latex" => {
                text.trim_end_matches('\n').to_owned().into()
            }
            Block::Raw { .. } => Output::default(),
            Block::ThematicBreak { .. } => "\\writerhrule".to_owned().into(),
            Block::PageBreak { .. } => "\\writerpagebreak".to_owned().into(),
            Block::Toc { depth, .. } => format!(
                "\\setcounter{{tocdepth}}{{{}}}\n\\tableofcontents",
                *depth as i16 + self.offset() as i16,
            )
            .into(),
            Block::Bibliography { .. } => self.bibliography_block(),
            Block::Cell(cell) => self.cell(cell, context),
        };
        self.span = previous;
        output
    }

    fn heading(
        &mut self, level: u8, inlines: &[Inline], attrs: &Attrs,
        numbered: bool, context: Context,
    ) -> Output {
        let depth = (level as i16 + self.offset() as i16).clamp(-1, 5) as i8;
        let command = match depth {
            -1 => "part",
            0 => "chapter",
            1 => "section",
            2 => "subsection",
            3 => "subsubsection",
            4 => "paragraph",
            _ => "subparagraph",
        };
        let listed = !context.abstract_
            && !attrs
                .classes
                .iter()
                .any(|class| matches!(class.as_str(), "notoc" | "unlisted"));
        let (full, short) = self.moving_inlines(inlines, context);
        let toc = (!numbered && listed)
            .then(|| short.clone().unwrap_or_else(|| full.clone()));
        let mut output = Output::default();
        if numbered {
            self.usage.secnumdepth = Some(
                self.usage
                    .secnumdepth
                    .map_or(depth, |previous| previous.max(depth)),
            );
            if !listed {
                output.literal("\\writernotoc\n");
            }
        }
        output.literal(&format!(
            "\\{command}{}",
            if numbered { "" } else { "*" }
        ));
        if numbered && let Some(short) = short {
            output.literal("[");
            output.append(short);
            output.literal("]");
        }
        output.literal("{");
        output.append(full);
        output.literal("}");
        // Section commands establish their own hyperref anchors, including
        // their starred forms (§4.4's explicit exception to §4.3).
        output.literal(&self.label(attrs.id.as_deref(), true));
        if let Some(toc) = toc {
            output
                .literal(&format!("\n\\addcontentsline{{toc}}{{{command}}}{{"));
            output.append(toc);
            output.literal("}");
        }
        output
    }

    fn list(
        &mut self, kind: &ListKind, tight: bool, items: &[ListItem],
        mut context: Context,
    ) -> Output {
        context.list_depth += 1;
        let env = match kind {
            ListKind::Bullet => {
                context.bullet_depth += 1;
                "itemize"
            }
            ListKind::Ordered { .. } => {
                context.ordered_depth += 1;
                "enumerate"
            }
        };
        if context.bullet_depth > 4
            || context.ordered_depth > 4
            || context.list_depth > 6
        {
            self.finding(
                Level::Warning,
                "unsupported-construct",
                "latex: list nesting exceeds LaTeX's supported depth".into(),
            );
        }
        let mut output: Output = format!("\\begin{{{env}}}\n").into();
        if let ListKind::Ordered { start } = kind
            && *start != 1
        {
            let roman = ["i", "ii", "iii", "iv"]
                .get(context.ordered_depth.saturating_sub(1))
                .copied()
                .unwrap_or("iv");
            output.literal(&format!(
                "\\setcounter{{enum{roman}}}{{{}}}\n",
                *start as i128 - 1
            ));
        }
        if tight {
            output.literal("\\tightlist\n");
        }
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                output.literal(if tight { "\n" } else { "\n\n" });
            }
            output.literal("\\item");
            if let Some(checked) = item.checked {
                self.math_used();
                output.literal(if checked {
                    "[$\\boxtimes$]"
                } else {
                    "[$\\square$]"
                });
            }
            output.literal(
                if matches!(item.blocks.first(), Some(Block::Paragraph { .. }))
                {
                    " "
                } else {
                    "\n"
                },
            );
            output.append(self.blocks(&item.blocks, context, tight));
        }
        output.literal(&format!("\n\\end{{{env}}}"));
        output
    }

    fn definitions(
        &mut self, items: &[DefinitionItem], context: Context,
    ) -> Output {
        let tight =
            items.iter().flat_map(|item| &item.definitions).all(|blocks| {
                matches!(blocks.as_slice(), [Block::Paragraph { .. }])
            });
        let mut output: Output = "\\begin{description}\n".to_owned().into();
        if tight {
            output.literal("\\tightlist\n");
        }
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                output.literal(if tight { "\n" } else { "\n\n" });
            }
            output.literal("\\item[{");
            output.append(
                self.inlines(&item.term, Context { compact: true, ..context }),
            );
            output.literal("}] ");
            for (index, blocks) in item.definitions.iter().enumerate() {
                if index > 0 {
                    output.literal("\n\n");
                }
                output.append(self.blocks(blocks, context, false));
            }
        }
        output.literal("\n\\end{description}");
        output
    }

    fn div(
        &mut self, attrs: &Attrs, blocks: &[Block], context: Context,
    ) -> Output {
        let mut output: Output = self.label(attrs.id.as_deref(), false).into();
        if !output.is_empty() {
            output.literal("\n");
        }
        let mut endings = Vec::new();
        let mut centered = false;
        for class in &attrs.classes {
            if let Some(name) = class.strip_prefix("env-") {
                let letters = name.strip_suffix('*').unwrap_or(name);
                if !letters.is_empty()
                    && letters.bytes().all(|byte| byte.is_ascii_alphabetic())
                {
                    output.literal(&format!("\\begin{{{name}}}\n"));
                    endings.push(format!("\\end{{{name}}}"));
                }
            } else if class == "center" && !centered {
                centered = true;
                output.literal("\\begin{center}\n");
                endings.push("\\end{center}".into());
            } else if matches!(class.as_str(), "small" | "large") {
                output.literal(&format!("\\begingroup\\{class}\n"));
                endings.push("\\par\\endgroup".into());
            }
        }
        output.append(self.blocks(blocks, context, false));
        for ending in endings.iter().rev() {
            output.literal("\n");
            output.literal(ending);
        }
        output
    }

    fn cell(&mut self, cell: &Cell, context: Context) -> Output {
        let mut output: Output =
            self.label(cell.attrs.id.as_deref(), false).into();
        if cell.show_input {
            let mut attrs = cell.attrs.clone();
            attrs.id = None;
            let inner = Context {
                boxed: context.boxed || self.config.latex.cell_prompts,
                ..context
            };
            let input = self.code(
                Some(&cell.language),
                &cell.source,
                &attrs,
                inner,
                None,
            );
            append_cell(
                &mut output, input, self.config.latex.cell_prompts, "In",
                cell.execution_count,
            );
            self.usage.cells |= self.config.latex.cell_prompts;
        }
        if cell.show_output {
            for saved in &cell.outputs {
                let prompt = self.config.latex.cell_prompts
                    && matches!(saved.kind, OutputKind::Result);
                let inner =
                    Context { boxed: context.boxed || prompt, ..context };
                let rendered = self.blocks(saved.pick("latex"), inner, false);
                if !rendered.is_empty() {
                    append_cell(
                        &mut output, rendered, prompt, "Out",
                        saved.execution_count,
                    );
                    self.usage.cells |= prompt;
                }
            }
        }
        output
    }

    fn bibliography_block(&self) -> Output {
        if !self.config.latex.emit_bibliography
            || self.bibliography.is_empty()
            || !self.document.features.citations
        {
            return Output::default();
        }
        let text = match self.config.latex.cite {
            LatexCite::Biblatex => "\\printbibliography".to_owned(),
            cite => {
                let style = if cite == LatexCite::Natbib {
                    "plainnat"
                } else {
                    "plain"
                };
                let names = self
                    .bibliography
                    .iter()
                    .map(|(_, name)| {
                        format!(
                            "{}_assets/{}",
                            self.stem,
                            name.strip_suffix(".bib").unwrap_or(name)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "\\bibliographystyle{{{style}}}\n\\bibliography{{{names}}}"
                )
            }
        };
        text.into()
    }
}

fn append_cell(
    output: &mut Output, content: Output, prompt: bool, kind: &str,
    count: Option<i64>,
) {
    if !output.is_empty() {
        output.literal("\n\n");
    }
    if prompt {
        output.literal(&format!(
            "\\begin{{writercell}}{{{kind}}}{{{}}}\n",
            count.map_or_else(String::new, |count| count.to_string())
        ));
    }
    output.append(content);
    if prompt {
        output.literal("\n\\end{writercell}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LatexCode;
    use crate::ir::{
        CiteItem, CiteMode, DocMeta, Features, FigureKind, LabelInfo,
        LabelKind, MathDialect, RefForm, Resolved, ResolvedRef, SourceInfo,
        SourceKind,
    };

    fn span(line: usize, file: &str, cell: Option<usize>) -> SourceSpan {
        SourceSpan {
            start_line: line,
            end_line: line,
            cell,
            file: Some(file.into()),
        }
    }

    fn paragraph(inlines: Vec<Inline>) -> Block {
        Block::Paragraph { inlines, span: Some(span(3, "/book/main.md", None)) }
    }

    fn fixture(blocks: Vec<Block>) -> Document {
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

    fn render(document: &Document, config: &Config) -> Emitted {
        super::super::emit(document, &AssetRegistry::new(), config, "main")
    }

    fn note(blocks: Vec<Block>) -> Inline {
        Inline::Footnote { blocks, label: None, number: None }
    }

    fn math(source: &str) -> Inline {
        Inline::Math { source: source.into(), dialect: MathDialect::Latex }
    }

    fn remote_image() -> Inline {
        Inline::Image {
            alt: vec![Inline::text("external")],
            url: "https://example.test/image.png".into(),
            title: None,
            attrs: Attrs::default(),
        }
    }

    fn cite(items: Vec<CiteItem>, mode: CiteMode) -> Inline {
        Inline::Cite { items, mode, resolved: None, span: None }
    }

    #[test]
    fn heading_short_forms_preserve_full_first_use_without_repeating_findings()
    {
        for numbered in [false, true] {
            let document = fixture(vec![Block::Heading {
                level: 1,
                attrs: Attrs {
                    id: Some("sec-title".into()),
                    ..Attrs::default()
                },
                number: numbered.then(|| "1".into()),
                span: Some(span(7, "/book/title.ipynb", Some(2))),
                inlines: vec![
                    Inline::text("Title"),
                    note(vec![paragraph(vec![math(r"\mathscr{A}")])]),
                    math(r"\cancel{x}"),
                    remote_image(),
                ],
            }]);
            let emitted = render(&document, &Config::default());
            assert_eq!(emitted.usage.math_packages, ["mathrsfs", "cancel"]);
            assert_eq!(emitted.body.matches(r"\protect\footnote").count(), 1);
            assert!(!emitted.body.contains(r"\phantomsection"));
            assert!(emitted.body.contains(&format!(
                "}}\\label{{{}}}",
                crate::resolve::anchor("sec-title"),
            )));
            if numbered {
                assert!(emitted.body.starts_with(r"\section[Title"));
                assert!(!emitted.body.contains(r"\addcontentsline"));
            } else {
                assert!(emitted.body.starts_with(r"\section*{Title"));
                let (_, toc) =
                    emitted.body.split_once(r"\addcontentsline").unwrap();
                assert!(!toc.contains(r"\footnote"));
            }
            let findings = emitted.diagnostics.iter().collect::<Vec<_>>();
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].code, "unsupported-construct");
            assert_eq!(
                findings[0].span,
                Some(span(7, "/book/title.ipynb", Some(2)))
            );
        }
    }

    #[test]
    fn listing_figures_inside_notes_keep_captions_and_escaped_code() {
        let listing = Block::Figure {
            kind: FigureKind::Listing,
            caption: Some(vec![Inline::text("Stored")]),
            attrs: Attrs { id: Some("lst-note".into()), ..Attrs::default() },
            number: Some("1".into()),
            span: None,
            body: vec![Block::CodeBlock {
                lang: Some("python".into()),
                text: "a_b\nx < y".into(),
                attrs: Attrs::default(),
                span: None,
            }],
        };
        for mode in
            [LatexCode::Verbatim, LatexCode::Highlighted, LatexCode::Listings]
        {
            let mut config = Config::default();
            config.latex.code = mode;
            let document =
                fixture(vec![paragraph(vec![note(vec![listing.clone()])])]);
            let emitted = render(&document, &config);
            assert!(emitted.body.contains(
                "\\captionsetup{type=writerlisting}\n\\caption{Stored}"
            ));
            assert!(emitted.body.contains(&format!(
                "\\label{{{}}}",
                crate::resolve::anchor("lst-note"),
            )));
            assert!(
                emitted
                    .body
                    .contains(r"{\ttfamily a\_b\newline x\ \textless{}\ y}")
            );
            assert!(!emitted.body.contains(r"\begin{Verbatim}"));
            assert!(!emitted.body.contains(r"\begin{writercode}"));
            assert!(!emitted.body.contains(r"\begin{lstlisting}"));
            assert!(emitted.usage.listing_caption);
            assert!(emitted.usage.caption);
            assert!(!emitted.usage.code);
            assert!(emitted.diagnostics.is_empty());
        }
    }

    #[test]
    fn listings_caption_notes_have_a_short_option_but_unnumbered_title_does_not()
     {
        let mut config = Config::default();
        config.latex.code = LatexCode::Listings;
        for numbered in [false, true] {
            let document = fixture(vec![Block::Figure {
                kind: FigureKind::Listing,
                caption: Some(vec![
                    Inline::text("Code"),
                    note(vec![paragraph(vec![Inline::text("note")])]),
                ]),
                attrs: Attrs::default(),
                number: numbered.then(|| "1".into()),
                span: None,
                body: vec![Block::CodeBlock {
                    lang: Some("python".into()),
                    text: "pass".into(),
                    attrs: Attrs::default(),
                    span: None,
                }],
            }]);
            let emitted = render(&document, &config);
            let expected = if numbered {
                r"caption={[Code]Code\protect\footnote{note}}"
            } else {
                r"title={Code\protect\footnote{note}}"
            };
            assert!(emitted.body.contains(expected), "{}", emitted.body);
            assert_eq!(emitted.body.matches(r"\footnote").count(), 1);
            assert!(emitted.diagnostics.is_empty());
        }
    }

    #[test]
    fn delegated_listing_caption_and_body_keep_their_own_notebook_locations() {
        let mut config = Config::default();
        config.latex.code = LatexCode::Listings;
        let outer = span(4, "/book/caption.md", None);
        let inner = span(21, "/book/code.ipynb", Some(6));
        let document = fixture(vec![Block::Figure {
            kind: FigureKind::Listing,
            caption: Some(vec![remote_image()]),
            attrs: Attrs::default(),
            number: Some("1".into()),
            span: Some(outer.clone()),
            body: vec![Block::CodeBlock {
                lang: Some("python".into()),
                text: "😀".into(),
                attrs: Attrs::default(),
                span: Some(inner.clone()),
            }],
        }]);
        let emitted = render(&document, &config);
        let caption = emitted
            .diagnostics
            .iter()
            .find(|finding| finding.code == "unsupported-construct")
            .unwrap();
        assert_eq!(caption.span, Some(outer));
        assert_eq!(
            caption.path.as_deref(),
            Some(std::path::Path::new("/book/caption.md"))
        );
        let code = emitted
            .diagnostics
            .iter()
            .find(|finding| finding.code == "latex-unicode")
            .unwrap();
        assert_eq!(code.span, Some(inner));
        assert_eq!(
            code.path.as_deref(),
            Some(std::path::Path::new("/book/code.ipynb"))
        );
    }

    #[test]
    fn hidden_citation_notes_do_not_register_resources_or_diagnostics() {
        let mut config = Config::default();
        config.latex.cite = LatexCite::Natbib;
        config
            .math
            .katex_options
            .insert("macros".into(), serde_json::json!({"bad": "x"}));
        for mode in [CiteMode::Author, CiteMode::None] {
            let item = CiteItem {
                key: "a".into(),
                locator: Some("α😀".into()),
                prefix: Some(vec![math(r"\mathscr{A}"), remote_image()]),
                suffix: Some(vec![Inline::Strikeout {
                    inlines: vec![Inline::text("😀")],
                }]),
            };
            let document = fixture(vec![paragraph(vec![
                cite(vec![item.clone()], mode),
                cite(vec![item], mode),
            ])]);
            let emitted = render(&document, &config);
            assert_eq!(emitted.usage, Usage::default());
            assert!(emitted.images.is_empty());
            let expected = if mode == CiteMode::Author {
                r"\citeauthor{a}"
            } else {
                r"\nocite{a}"
            };
            assert_eq!(emitted.body, format!("{expected}{expected}\n"));
            let findings = emitted.diagnostics.iter().collect::<Vec<_>>();
            if mode == CiteMode::Author {
                assert_eq!(findings.len(), 1);
                assert_eq!(findings[0].code, "unsupported-construct");
            } else {
                assert!(findings.is_empty());
            }
        }
        let document = fixture(vec![paragraph(vec![cite(
            vec![CiteItem {
                key: "a".into(),
                prefix: Some(vec![
                    Inline::text(""),
                    Inline::Raw {
                        format: "html".into(),
                        text: "<b>hidden</b>".into(),
                    },
                ]),
                ..CiteItem::default()
            }],
            CiteMode::Author,
        )])]);
        assert!(render(&document, &config).diagnostics.is_empty());
    }

    #[test]
    fn visible_citation_notes_scan_unicode_in_pre_locator_suffix_order() {
        let document = fixture(vec![paragraph(vec![cite(
            vec![CiteItem {
                key: "a".into(),
                prefix: Some(vec![Inline::text("一")]),
                locator: Some("二 p. 2".into()),
                suffix: Some(vec![Inline::text("三")]),
            }],
            CiteMode::Normal,
        )])]);
        let emitted = render(&document, &Config::default());
        assert_eq!(emitted.body, "\\autocite[一][二~p.~2, 三]{a}\n");
        let warning = emitted
            .diagnostics
            .iter()
            .find(|finding| finding.code == "latex-unicode")
            .unwrap();
        assert!(
            warning.message.contains("U+4E00 (一), U+4E8C (二), U+4E09 (三)")
        );
        let document = fixture(vec![paragraph(vec![cite(
            vec![CiteItem {
                key: "a".into(),
                suffix: Some(vec![Inline::text("tail")]),
                ..CiteItem::default()
            }],
            CiteMode::Normal,
        )])]);
        assert_eq!(
            render(&document, &Config::default()).body,
            "\\autocite[, tail]{a}\n"
        );
    }

    #[test]
    fn synthesized_reference_text_is_located_and_maps_unicode() {
        let mut document = fixture(vec![paragraph(vec![
            Inline::Ref {
                target: "α".into(),
                supplement: None,
                form: RefForm::Normal,
                resolved: None,
                span: Some(span(8, "/book/refs.ipynb", Some(1))),
            },
            Inline::FootnoteRef { label: "😀".into(), number: None },
        ])]);
        let mut config = Config::default();
        config
            .math
            .katex_options
            .insert("macros".into(), serde_json::json!({"bad": "x"}));
        let emitted = render(&document, &config);
        assert!(emitted.usage.math);
        let macro_warning = emitted
            .diagnostics
            .iter()
            .find(|finding| finding.code == "math-macro-invalid")
            .unwrap();
        assert_eq!(
            macro_warning.span,
            Some(span(8, "/book/refs.ipynb", Some(1)))
        );
        assert!(
            emitted
                .diagnostics
                .iter()
                .any(|finding| finding.code == "latex-unicode"
                    && finding.message.contains("U+1F600"))
        );

        document.blocks = vec![paragraph(vec![Inline::Ref {
            target: "fig".into(),
            supplement: None,
            form: RefForm::Normal,
            resolved: Some(ResolvedRef {
                kind: LabelKind::Figure,
                number: "1".into(),
                text: Vec::new(),
                anchor: "fig".into(),
            }),
            span: None,
        }])];
        document.resolved.labels.insert(
            "fig".into(),
            LabelInfo {
                kind: LabelKind::Figure,
                number: Some("1".into()),
                supplement: "α".into(),
                text: None,
                span: None,
            },
        );
        config.latex.cleveref = false;
        assert!(render(&document, &config).usage.math);
    }

    #[test]
    fn custom_callout_titles_keep_the_original_kind_and_empty_kind_is_visible()
    {
        for (kind, hook, title) in [
            ("custom note", "customnote", "Custom note"),
            ("éX", "x", "ÉX"),
            ("", "note", "Note"),
        ] {
            let document = fixture(vec![Block::Callout {
                kind: kind.into(),
                title: None,
                blocks: Vec::new(),
                attrs: Attrs::default(),
                span: None,
            }]);
            let emitted = render(&document, &Config::default());
            assert!(
                emitted.body.contains(&format!(
                    "\\begin{{writercallout}}{{{hook}}}{{{title}}}",
                )),
                "{}",
                emitted.body
            );
        }
    }

    #[test]
    fn references_keep_sentence_case_page_precedence_and_explicit_empty_supplements()
     {
        let reference = |target: &str, form, supplement| Inline::Ref {
            target: target.into(),
            form,
            supplement,
            resolved: Some(ResolvedRef {
                kind: if target == "eq" {
                    LabelKind::Equation
                } else {
                    LabelKind::Heading
                },
                number: "1".into(),
                anchor: target.into(),
                text: vec![Inline::text("Cached heading")],
            }),
            span: None,
        };
        let mut document = fixture(vec![
            paragraph(vec![
                reference("section", RefForm::Normal, None),
                Inline::text(" then "),
                reference("section", RefForm::Normal, None),
                Inline::text(". "),
                reference("section", RefForm::Normal, None),
            ]),
            paragraph(vec![reference("unnumbered", RefForm::Page, None)]),
            paragraph(vec![reference("eq", RefForm::Normal, Some(Vec::new()))]),
        ]);
        for (target, kind, number) in [
            ("section", LabelKind::Heading, Some("1".into())),
            ("unnumbered", LabelKind::Heading, None),
            ("eq", LabelKind::Equation, Some("1".into())),
        ] {
            document.resolved.labels.insert(
                target.into(),
                LabelInfo {
                    kind,
                    number,
                    supplement: "Section".into(),
                    text: None,
                    span: None,
                },
            );
        }
        let mut config = Config::default();
        config.latex.cleveref = true;
        let emitted = render(&document, &config);
        let section = crate::resolve::anchor("section");
        assert!(emitted.body.starts_with(&format!(
            "\\Cref{{{section}}} then \\cref{{{section}}}. \\Cref{{{section}}}",
        )));
        assert!(emitted.body.contains(&format!(
            "\\cpageref{{{}}}",
            crate::resolve::anchor("unnumbered"),
        )));
        assert!(!emitted.body.contains(r"\hyperref"));
        assert!(
            emitted.body.contains(&format!(
                "\\eqref{{{}}}",
                crate::resolve::anchor("eq"),
            ))
        );
        assert!(emitted.diagnostics.is_empty());
    }

    #[test]
    fn missing_note_placeholders_survive_short_titles_without_becoming_moving_notes()
     {
        for with_note in [false, true] {
            let mut inlines = vec![Inline::text("Title")];
            if with_note {
                inlines.push(note(vec![paragraph(vec![Inline::text("body")])]));
            }
            inlines.push(Inline::FootnoteRef {
                label: "missing".into(),
                number: None,
            });
            let document = fixture(vec![Block::Heading {
                level: 1,
                inlines,
                attrs: Attrs::default(),
                number: Some("1".into()),
                span: None,
            }]);
            let emitted = render(&document, &Config::default());
            if with_note {
                assert!(
                    emitted.body.starts_with(r"\section[Title{[}?missing{]}]")
                );
                assert_eq!(emitted.body.matches("?missing").count(), 2);
            } else {
                assert!(
                    emitted.body.starts_with(r"\section{Title{[}?missing{]}}")
                );
                assert_eq!(emitted.body.matches("?missing").count(), 1);
            }
        }
    }

    #[test]
    fn bare_url_bookmark_text_is_scanned_but_percent_encoded_url_bytes_are_not()
    {
        let url = "https://example.test/α";
        let link = Inline::Link {
            inlines: vec![Inline::text(url)],
            url: url.into(),
            title: None,
            attrs: Attrs::default(),
        };
        let config = Config::default();
        let plain = fixture(vec![paragraph(vec![link.clone()])]);
        let emitted = render(&plain, &config);
        assert!(!emitted.usage.math);
        assert!(emitted.diagnostics.is_empty());
        assert!(emitted.body.contains(r"\%CE\%B1"));
        let title = fixture(vec![Block::Heading {
            level: 1,
            inlines: vec![link],
            attrs: Attrs { classes: vec!["notoc".into()], ..Attrs::default() },
            number: None,
            span: None,
        }]);
        let emitted = render(&title, &config);
        assert!(emitted.usage.math);
        assert!(emitted.body.contains(r"\texorpdfstring{\url{"));
        assert!(emitted.body.contains(r"\ensuremath{\alpha}"));
        assert!(emitted.diagnostics.is_empty());
    }

    #[test]
    fn listings_unknown_language_warning_uses_the_lowercased_table_identity() {
        let document = fixture(
            ["rUst", "RUST"]
                .into_iter()
                .map(|lang| Block::CodeBlock {
                    lang: Some(lang.into()),
                    text: "fn main() {}".into(),
                    attrs: Attrs::default(),
                    span: None,
                })
                .collect(),
        );
        let mut config = Config::default();
        config.latex.code = LatexCode::Listings;
        let emitted = render(&document, &config);
        assert_eq!(emitted.body.matches(r"\begin{lstlisting}").count(), 2);
        assert!(!emitted.body.contains("language="));
        let findings = emitted.diagnostics.iter().collect::<Vec<_>>();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "latex-listings-language");
        assert!(findings[0].message.contains("`rUst`"));
    }

    #[test]
    fn empty_or_foreign_only_explicit_callout_titles_use_the_default() {
        for title in [
            Vec::new(),
            vec![Inline::text("")],
            vec![Inline::Raw {
                format: "html".into(),
                text: "<b>hidden</b>".into(),
            }],
        ] {
            let document = fixture(vec![Block::Callout {
                kind: "tip".into(),
                title: Some(title),
                blocks: Vec::new(),
                attrs: Attrs::default(),
                span: None,
            }]);
            let emitted = render(&document, &Config::default());
            assert!(emitted.body.contains(r"\begin{writercallout}{tip}{Tip}"));
            assert!(emitted.diagnostics.is_empty());
        }
    }
}
