//! Inline contexts, references, notes and citation commands.

use crate::config::LatexCite;
use crate::ir::{CiteItem, CiteMode, LabelKind, RefForm, ResolvedRef};

use super::math::UnicodeContext;
use super::*;

impl Emitter<'_> {
    pub(super) fn inlines(
        &mut self, inlines: &[Inline], context: Context,
    ) -> Output {
        self.inline_run(inlines, context, &mut true)
    }

    fn inline_run(
        &mut self, inlines: &[Inline], context: Context, sentence: &mut bool,
    ) -> Output {
        let mut output = Output::default();
        for (index, inline) in inlines.iter().enumerate() {
            match inline {
                Inline::Text { text } => {
                    self.unicode(text, UnicodeContext::Text);
                    output.prose(text, self.map);
                    if !text.is_empty() {
                        let tail = text.trim_end();
                        *sentence = tail.len() < text.len()
                            && tail.ends_with(['.', '?', '!']);
                    }
                }
                Inline::Emph { inlines }
                | Inline::Strong { inlines }
                | Inline::Strikeout { inlines }
                | Inline::Subscript { inlines }
                | Inline::Superscript { inlines }
                | Inline::Underline { inlines }
                | Inline::SmallCaps { inlines } => {
                    let command = match inline {
                        Inline::Emph { .. } => "emph",
                        Inline::Strong { .. } => "textbf",
                        Inline::Strikeout { .. } => "sout",
                        Inline::Subscript { .. } => "textsubscript",
                        Inline::Superscript { .. } => "textsuperscript",
                        Inline::Underline { .. } => "uline",
                        _ => "textsc",
                    };
                    let ulem = matches!(command, "sout" | "uline");
                    self.usage.ulem |= ulem;
                    if context.moving && ulem {
                        output.literal("\\protect");
                    }
                    output.literal(&format!("\\{command}{{"));
                    output.append(self.inline_run(inlines, context, sentence));
                    output.literal("}");
                }
                Inline::Code { text, attrs } => {
                    self.unicode(text, UnicodeContext::Code);
                    output.literal(&self.label(attrs.id.as_deref(), false));
                    output.literal(&format!(
                        "\\texttt{{{}}}",
                        escape::code(text, self.map)
                    ));
                    *sentence = false;
                }
                Inline::Math { source, dialect } => {
                    let source = self.math_source(source, *dialect);
                    if context.moving {
                        output.literal(&format!(
                            "\\texorpdfstring{{\\({source}\\)}}{{{}}}",
                            escape::text(&source, self.map)
                        ));
                    } else {
                        output.literal(&format!("\\({source}\\)"));
                    }
                    *sentence = false;
                }
                Inline::Link { inlines, url, attrs, .. } => {
                    output.literal(&self.label(attrs.id.as_deref(), false));
                    let plain = crate::ir::plain_text(inlines);
                    if let Some(target) = url.strip_prefix('#') {
                        if self.document.resolved.labels.contains_key(target) {
                            output.literal(&format!(
                                "\\hyperref[{}]{{",
                                crate::resolve::anchor(target)
                            ));
                            output.append(
                                self.inline_run(inlines, context, sentence),
                            );
                            output.literal("}");
                        } else {
                            output.append(
                                self.inline_run(inlines, context, sentence),
                            );
                        }
                    } else if plain == *url
                        || url.strip_prefix("mailto:") == Some(plain.as_str())
                    {
                        if context.moving {
                            self.unicode(&plain, UnicodeContext::Text);
                            output.literal(&format!(
                                "\\texorpdfstring{{\\url{{{}}}}}{{{}}}",
                                escape::url(url),
                                escape::text(&plain, self.map)
                            ));
                        } else {
                            output.literal(&format!(
                                "\\url{{{}}}",
                                escape::url(url)
                            ));
                        }
                        *sentence = false;
                    } else {
                        output.literal(&format!(
                            "\\href{{{}}}{{",
                            escape::url(url)
                        ));
                        output.append(
                            self.inline_run(inlines, context, sentence),
                        );
                        output.literal("}");
                    }
                }
                Inline::Image { alt, url, attrs, .. } => {
                    output.literal(&self.label(attrs.id.as_deref(), false));
                    output.append(
                        self.image(alt, url, attrs, context.figure_image),
                    );
                    *sentence = false;
                }
                Inline::Footnote { blocks, number, .. } => {
                    if context.omit_notes {
                        continue;
                    }
                    if context.footnote {
                        self.finding(Level::Warning, "latex-nested-footnote",
                            "latex: a footnote is nested inside another footnote".into());
                    }
                    if context.moving {
                        output.literal("\\protect");
                    }
                    output.literal("\\footnote{");
                    if let Some(number) = number
                        && self.note_refs.contains(number)
                    {
                        output
                            .literal(&format!("\\label{{writer-fn-{number}}}"));
                    }
                    output.append(self.blocks(
                        blocks,
                        Context {
                            boxed: true,
                            footnote: true,
                            moving: false,
                            compact: false,
                            ..context
                        },
                        false,
                    ));
                    output.literal("}");
                }
                Inline::FootnoteRef { number, label } => {
                    if let Some(number) = number {
                        if context.omit_notes {
                            continue;
                        }
                        if context.moving {
                            output.literal("\\protect");
                        }
                        output.literal(&format!(
                            "\\footref{{writer-fn-{number}}}"
                        ));
                    } else {
                        let text = format!("[?{label}]");
                        self.unicode(&text, UnicodeContext::Text);
                        output.prose(&text, self.map);
                    }
                }
                Inline::Ref { target, supplement, form, resolved, span } => {
                    let previous = self.span.clone();
                    self.span = span.clone().or(previous.clone());
                    output.append(self.reference(
                        target,
                        supplement.as_deref(),
                        *form,
                        resolved.as_ref(),
                        context,
                        *sentence,
                    ));
                    self.span = previous;
                    *sentence = false;
                }
                Inline::Cite { items, mode, span, .. } => {
                    let previous = self.span.clone();
                    self.span = span.clone().or(previous.clone());
                    output.literal(&self.citation(items, *mode, context));
                    self.span = previous;
                    *sentence = false;
                }
                Inline::SoftBreak => {
                    if context.compact {
                        output.prose(" ", self.map);
                    } else {
                        output.literal("\n");
                    }
                }
                Inline::LineBreak => {
                    if context.table_p {
                        output.literal("\\newline ");
                    } else if context.compact {
                        output.literal(" ");
                    } else {
                        output.literal("\\\\\n");
                        if inlines.get(index + 1).is_some_and(|inline|
                            matches!(inline, Inline::Text { text } if text.starts_with('*')))
                        { output.literal("{}"); }
                    }
                }
                Inline::Span { attrs, inlines } => {
                    output.literal(&self.label(attrs.id.as_deref(), false));
                    let sub =
                        attrs.classes.iter().any(|class| class == "subcaption");
                    if sub {
                        output.literal("{\\small ");
                    }
                    output.append(self.inline_run(inlines, context, sentence));
                    if sub {
                        output.literal("}");
                    }
                }
                Inline::Raw { format, text } => {
                    if format == "latex" {
                        output.literal(text);
                    }
                }
            }
        }
        output
    }

    fn reference(
        &mut self, target: &str, supplement: Option<&[Inline]>, form: RefForm,
        resolved: Option<&ResolvedRef>, context: Context, sentence: bool,
    ) -> Output {
        let Some(resolved) = resolved else {
            let mut output = Output::default();
            let text = format!("[?{target}]");
            self.unicode(&text, UnicodeContext::Text);
            output.prose(&text, self.map);
            return output;
        };
        let anchor = crate::resolve::anchor(target);
        let mut output = Output::default();
        if form == RefForm::Page {
            if let Some(supplement) = supplement {
                if !supplement.is_empty() {
                    output.append(self.inlines(supplement, context));
                    output.literal("~");
                }
                output.literal(&format!("\\pageref{{{anchor}}}"));
            } else if self.config.latex.cleveref {
                output.literal(&format!("\\cpageref{{{anchor}}}"));
            } else {
                let page = term(&self.lang, Term::Page);
                self.unicode(&page, UnicodeContext::Text);
                output.prose(&page, self.map);
                output.literal(&format!("~\\pageref{{{anchor}}}"));
            }
            return output;
        }
        if self
            .document
            .resolved
            .labels
            .get(target)
            .is_some_and(|info| info.number.is_none())
        {
            output.literal(&format!("\\hyperref[{anchor}]{{"));
            output.append(self.inlines(&resolved.text, context));
            output.literal("}");
            return output;
        }
        let command = if resolved.kind == LabelKind::Equation {
            "eqref"
        } else {
            "ref"
        };
        if let Some(supplement) = supplement {
            if !supplement.is_empty() {
                output.append(self.inlines(supplement, context));
                output.literal("~");
            }
            output.literal(&format!("\\{command}{{{anchor}}}"));
        } else if self.config.latex.cleveref {
            output.literal(&format!(
                "\\{}{{{anchor}}}",
                if sentence { "Cref" } else { "cref" }
            ));
        } else {
            if let Some(info) = self.document.resolved.labels.get(target) {
                let supplement = info.supplement.clone();
                self.unicode(&supplement, UnicodeContext::Text);
                output.prose(&supplement, self.map);
                if !supplement.is_empty() {
                    output.literal("~");
                }
            }
            output.literal(&format!("\\{command}{{{anchor}}}"));
        }
        output
    }

    fn citation(
        &mut self, items: &[CiteItem], mode: CiteMode, context: Context,
    ) -> String {
        for item in items {
            if item
                .key
                .chars()
                .any(|ch| ch.is_whitespace() || ",{}%#\\~\"".contains(ch))
            {
                self.finding(
                    Level::Error,
                    "latex-cite-key",
                    format!("latex: invalid citation key `{}`", item.key),
                );
            }
        }
        if items.is_empty() {
            return String::new();
        }
        let keys = items
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>()
            .join(",");
        if mode == CiteMode::None {
            return format!("\\nocite{{{keys}}}");
        }
        let family = self.config.latex.cite;
        if family == LatexCite::Natbib && mode == CiteMode::Author {
            let has_notes = items.iter().any(|item| {
                item.locator.as_ref().is_some_and(|text| !text.is_empty())
                    || self.note_inlines_emit(
                        item.prefix.as_deref().unwrap_or_default(),
                        context,
                    )
                    || self.note_inlines_emit(
                        item.suffix.as_deref().unwrap_or_default(),
                        context,
                    )
            });
            if has_notes {
                self.once(
                    format!("cite-mode:{mode:?}"),
                    Level::Warning,
                    "unsupported-construct",
                    "latex: natbib author citations drop notes".into(),
                );
                return items
                    .iter()
                    .map(|item| format!("\\citeauthor{{{}}}", item.key))
                    .collect::<Vec<_>>()
                    .join("; ");
            }
            return format!("\\citeauthor{{{keys}}}");
        }
        let unsupported = (family == LatexCite::Plain
            && mode != CiteMode::Normal)
            || (family == LatexCite::Natbib && mode == CiteMode::Full);
        if unsupported {
            self.once(
                format!("cite-mode:{mode:?}"),
                Level::Warning,
                "unsupported-construct",
                format!(
                    "latex: citation mode {mode:?} is unsupported by {family:?}"
                ),
            );
        }
        let command = match (family, mode) {
            (LatexCite::Biblatex, CiteMode::Normal) => "autocite",
            (LatexCite::Biblatex, CiteMode::Prose) => "textcite",
            (LatexCite::Biblatex, CiteMode::Year) => "autocite*",
            (LatexCite::Biblatex, CiteMode::Author)
            | (LatexCite::Natbib, CiteMode::Author) => "citeauthor",
            (LatexCite::Biblatex, CiteMode::Full) => "fullcite",
            (LatexCite::Natbib, CiteMode::Prose) => "citet",
            (LatexCite::Natbib, CiteMode::Year) => "citeyearpar",
            (LatexCite::Natbib, _) => "citep",
            _ => "cite",
        };
        let mut notes = Vec::new();
        for item in items {
            let pre = self
                .inlines(
                    item.prefix.as_deref().unwrap_or(&[]),
                    Context { compact: true, ..context },
                )
                .text;
            let mut post = if let Some(locator) = &item.locator {
                self.unicode(locator, UnicodeContext::Text);
                escape::text(locator, self.map).replace(' ', "~")
            } else {
                String::new()
            };
            let suffix = self
                .inlines(
                    item.suffix.as_deref().unwrap_or(&[]),
                    Context { compact: true, ..context },
                )
                .text;
            if !suffix.is_empty() {
                post.push_str(", ");
                post.push_str(&suffix);
            }
            notes.push((pre, post));
        }
        let has_notes =
            notes.iter().any(|(pre, post)| !pre.is_empty() || !post.is_empty());
        if !has_notes {
            return format!("\\{command}{{{keys}}}");
        }
        if family == LatexCite::Biblatex
            && items.len() > 1
            && matches!(mode, CiteMode::Normal | CiteMode::Prose)
        {
            let grouped = if mode == CiteMode::Prose {
                "textcites"
            } else {
                "autocites"
            };
            let args = items
                .iter()
                .zip(&notes)
                .map(|(item, (pre, post))| {
                    format!("{}{{{}}}", cite_options(pre, post), item.key)
                })
                .collect::<String>();
            return format!("\\{grouped}{args}");
        }
        let natbib_group = family == LatexCite::Natbib
            && items.len() > 1
            && matches!(mode, CiteMode::Normal | CiteMode::Full);
        let commands = items
            .iter()
            .zip(notes)
            .map(|(item, (pre, post))| {
                if family == LatexCite::Plain {
                    format!(
                        "{}\\cite{}{{{}}}",
                        if pre.is_empty() {
                            String::new()
                        } else {
                            format!("{pre} ")
                        },
                        if post.is_empty() {
                            String::new()
                        } else {
                            format!("[{post}]")
                        },
                        item.key
                    )
                } else {
                    format!(
                        "\\{}{}{{{}}}",
                        if natbib_group { "citealp" } else { command },
                        cite_options(&pre, &post),
                        item.key
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        if natbib_group {
            format!("\\citetext{{{commands}}}")
        } else {
            commands
        }
    }

    // Dropped author-citation notes must not register packages, assets,
    // Unicode definitions or diagnostics merely to detect their presence.
    fn note_inlines_emit(&self, inlines: &[Inline], context: Context) -> bool {
        inlines.iter().any(|inline| match inline {
            Inline::Text { text } => !text.is_empty(),
            Inline::Raw { format, text } => {
                format == "latex" && !text.is_empty()
            }
            Inline::Cite { items, .. } => !items.is_empty(),
            Inline::Footnote { .. } | Inline::FootnoteRef { .. } => {
                !context.omit_notes
            }
            Inline::Span { attrs, inlines } => {
                attrs.id.is_some()
                    || attrs.classes.iter().any(|class| class == "subcaption")
                    || self.note_inlines_emit(inlines, context)
            }
            Inline::Link { attrs, url, inlines, .. } => {
                attrs.id.is_some()
                    || url.strip_prefix('#').is_none_or(|target| {
                        self.document.resolved.labels.contains_key(target)
                    })
                    || self.note_inlines_emit(inlines, context)
            }
            Inline::Image { attrs, url, alt, .. } => {
                let planned = url.strip_prefix("asset:").is_some_and(|id| {
                    self.names
                        .contains_key(id.split(['?', '#']).next().unwrap_or(id))
                });
                attrs.id.is_some()
                    || planned
                    || url.starts_with("http://")
                    || url.starts_with("https://")
                    || !crate::ir::plain_text(alt).is_empty()
            }
            _ => true,
        })
    }
}

fn cite_options(pre: &str, post: &str) -> String {
    if !pre.is_empty() {
        format!("[{pre}][{post}]")
    } else if !post.is_empty() {
        format!("[{post}]")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{DocMeta, Features, Resolved, SourceInfo, SourceKind};

    #[test]
    fn author_groups_split_only_when_notes_exist_and_none_mode_stays_grouped() {
        let mut config = Config::default();
        config.latex.cite = LatexCite::Natbib;
        config
            .math
            .katex_options
            .insert("macros".into(), serde_json::json!({"invalid": "x"}));
        for (mode, notes, expected, warnings) in [
            (CiteMode::Author, false, r"\citeauthor{a,b}", 0),
            (CiteMode::Author, true, r"\citeauthor{a}; \citeauthor{b}", 1),
            (CiteMode::None, false, r"\nocite{a,b}", 0),
            (CiteMode::None, true, r"\nocite{a,b}", 0),
        ] {
            let document = Document {
                meta: DocMeta::default(),
                features: Features::default(),
                source: SourceInfo {
                    path: "/book/main.md".into(),
                    base_dir: "/book".into(),
                    kind: SourceKind::Markdown,
                },
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Cite {
                        items: vec![
                            CiteItem {
                                key: "a".into(),
                                locator: notes.then(|| "α😀".into()),
                                prefix: notes.then(|| {
                                    vec![
                                        Inline::Math {
                                            source: r"\mathscr{A}".into(),
                                            dialect:
                                                crate::ir::MathDialect::Latex,
                                        },
                                        Inline::Image {
                                            alt: vec![Inline::text("external")],
                                            url:
                                                "https://example.test/image.png"
                                                    .into(),
                                            title: None,
                                            attrs: Attrs::default(),
                                        },
                                    ]
                                }),
                                suffix: None,
                            },
                            CiteItem { key: "b".into(), ..CiteItem::default() },
                        ],
                        mode,
                        resolved: None,
                        span: None,
                    }],
                    span: None,
                }],
                assets: Vec::new(),
                includes: Vec::new(),
                resolved: Resolved::default(),
            };
            let emitted = super::super::emit(
                &document,
                &AssetRegistry::new(),
                &config,
                "main",
            );
            assert_eq!(emitted.body, format!("{expected}\n"));
            assert_eq!(emitted.usage, Usage::default());
            assert!(emitted.images.is_empty());
            let findings = emitted.diagnostics.iter().collect::<Vec<_>>();
            assert_eq!(findings.len(), warnings);
            assert!(findings.iter().all(|finding| {
                finding.code == "unsupported-construct"
                    && finding.message.contains("drop notes")
            }));
        }
    }
}
