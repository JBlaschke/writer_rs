//! Figure contexts, moving captions and portable graphic uses.

use crate::ir::FigureKind;

use super::math::UnicodeContext;
use super::*;

impl Emitter<'_> {
    pub(super) fn figure(
        &mut self, kind: FigureKind, caption: Option<&[Inline]>,
        body: &[Block], attrs: &Attrs, numbered: bool, context: Context,
    ) -> Output {
        if context.group {
            // Only figures actually traversed through selected output branches
            // reach this state. Their labels belong to the container counter.
            if let Some(id) = &attrs.id {
                self.subfigure_labels.push(id.clone());
            }
            let mut output = self.subfigure_body(kind, body, attrs, context);
            if let Some(caption) = caption {
                if !output.is_empty() {
                    output.literal("\n");
                }
                output.literal("\\par{\\small ");
                output.append(self.inlines(
                    caption,
                    Context { compact: true, moving: false, ..context },
                ));
                output.literal("}\\par");
            }
            return output;
        }
        let caption_span = self.span.clone();
        let caption =
            Caption { caption, attrs, numbered, span: caption_span.as_ref() };
        match (kind, body) {
            (
                FigureKind::Table,
                [Block::Table { columns, head, body, foot, attrs, span }],
            ) => {
                let previous = self.span.clone();
                self.span = span.clone().or(previous.clone());
                let output = self.table(
                    columns,
                    head,
                    body,
                    foot,
                    attrs,
                    context,
                    Some(caption),
                );
                self.span = previous;
                return output;
            }
            (
                FigureKind::Listing,
                [Block::CodeBlock { lang, text, attrs, span }],
            ) => {
                let previous = self.span.clone();
                self.span = span.clone().or(previous.clone());
                let output = self.code(
                    lang.as_deref(),
                    text,
                    attrs,
                    context,
                    Some(caption),
                );
                self.span = previous;
                return output;
            }
            _ => {}
        }
        let previous_labels = (kind == FigureKind::Other)
            .then(|| std::mem::take(&mut self.subfigure_labels));
        let mut output = Output::default();
        let boxed = context.boxed || context.footnote;
        if boxed {
            self.usage.caption = true;
            output.literal("\\begin{center}\n\\captionsetup{type=figure}\n");
        } else {
            let placement = attrs
                .pairs
                .get("fig-pos")
                .unwrap_or(&self.config.latex.figure_placement);
            output.literal("\\begin{figure}");
            if !placement.is_empty() {
                output.literal(&format!("[{placement}]"));
            }
            output.literal("\n\\centering\n");
        }
        let content = self.blocks(
            body,
            Context {
                boxed: true,
                group: kind == FigureKind::Other,
                figure_image: kind == FigureKind::Image,
                ..context
            },
            false,
        );
        output.append(content);
        let line = self.caption(caption, context);
        if !line.is_empty() {
            if !output.text.ends_with('\n') {
                output.literal("\n");
            }
            output.append(line);
        }
        if let Some(previous) = previous_labels {
            for id in std::mem::replace(&mut self.subfigure_labels, previous) {
                output.literal(&self.label(Some(&id), numbered));
            }
        }
        if !output.text.ends_with('\n') {
            output.literal("\n");
        }
        output.literal(if boxed {
            "\\end{center}"
        } else {
            "\\end{figure}"
        });
        output
    }

    fn subfigure_body(
        &mut self, kind: FigureKind, body: &[Block], attrs: &Attrs,
        context: Context,
    ) -> Output {
        let context = Context {
            boxed: true,
            figure_image: kind == FigureKind::Image,
            ..context
        };
        // A table/listing's matching inner id is the same deferred figure
        // label; do not emit an earlier counterless copy from that child.
        match (kind, body) {
            (
                FigureKind::Table,
                [
                    Block::Table {
                        columns,
                        head,
                        body,
                        foot,
                        attrs: inner,
                        span,
                    },
                ],
            ) => {
                let mut inner = inner.clone();
                if inner.id == attrs.id {
                    inner.id = None;
                }
                let previous = self.span.clone();
                self.span = span.clone().or(previous.clone());
                let output = self
                    .table(columns, head, body, foot, &inner, context, None);
                self.span = previous;
                output
            }
            (
                FigureKind::Listing,
                [Block::CodeBlock { lang, text, attrs: inner, span }],
            ) => {
                let mut inner = inner.clone();
                if inner.id == attrs.id {
                    inner.id = None;
                }
                let previous = self.span.clone();
                self.span = span.clone().or(previous.clone());
                let output =
                    self.code(lang.as_deref(), text, &inner, context, None);
                self.span = previous;
                output
            }
            _ => self.blocks(body, context, false),
        }
    }

    /// The full caption owns diagnostics and actual first-use collection.
    /// The short form is another representation of the same inline content.
    pub(super) fn moving_inlines(
        &mut self, inlines: &[Inline], context: Context,
    ) -> (Output, Option<Output>) {
        let context = Context { moving: true, compact: true, ..context };
        let full = self.inlines(inlines, context);
        let short = self.has_notes(inlines).then(|| {
            let retained = std::mem::take(&mut self.diagnostics);
            let output =
                self.inlines(inlines, Context { omit_notes: true, ..context });
            self.diagnostics = retained;
            output
        });
        (full, short)
    }

    pub(super) fn caption(
        &mut self, caption: Caption<'_>, context: Context,
    ) -> Output {
        let previous = self.span.clone();
        self.span = caption.span.cloned();
        let mut output = Output::default();
        if caption.numbered || caption.caption.is_some() {
            self.usage.caption = true;
            let (full, short) = self
                .moving_inlines(caption.caption.unwrap_or_default(), context);
            output.literal(if caption.numbered {
                "\\caption"
            } else {
                "\\caption*"
            });
            if let Some(short) = short {
                output.literal("[");
                output.append(short);
                output.literal("]");
            }
            output.literal("{");
            output.append(full);
            output.literal("}");
        }
        output.literal(
            &self.label(caption.attrs.id.as_deref(), caption.numbered),
        );
        self.span = previous;
        output
    }

    pub(super) fn image(
        &mut self, alt: &[Inline], url: &str, attrs: &Attrs, figure_image: bool,
    ) -> Output {
        let id = url
            .strip_prefix("asset:")
            .map(|id| id.split(['?', '#']).next().unwrap_or(id));
        let planned = id
            .and_then(|id| self.names.get(id).cloned().map(|name| (id, name)));
        if let Some((id, planned)) = planned {
            self.usage.graphics = true;
            if !self.images.iter().any(|image| image.id == id) {
                self.images.push(AssetFile {
                    id: id.into(),
                    name: planned.name.clone(),
                    render: planned.render,
                });
            }
            let options = image_options(attrs, figure_image);
            return format!(
                "\\includegraphics{}{{{}_assets/{}}}",
                if options.is_empty() {
                    String::new()
                } else {
                    format!("[{options}]")
                },
                self.stem,
                planned.name,
            )
            .into();
        }
        let reason = match id.and_then(|id| self.registry.get(id)) {
            Some(asset) => format!("unsupported image MIME `{}`", asset.mime),
            None if id.is_some() => "unregistered image asset".into(),
            None => "image URL is not a registered local asset".into(),
        };
        self.finding(
            Level::Warning,
            "unsupported-construct",
            format!(
                "latex: {reason} for `{url}`; emitted its alternative text"
            ),
        );
        let alt = crate::ir::plain_text(alt);
        self.unicode(&alt, UnicodeContext::Text);
        let remote = url
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
            || url
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"));
        let mut output = Output::default();
        if remote {
            output.literal(&format!("\\href{{{}}}{{", escape::url(url)));
        }
        output.prose(&alt, self.map);
        if remote {
            output.literal("}");
        }
        output
    }
}

fn image_options(attrs: &Attrs, figure: bool) -> String {
    let width =
        attrs.pairs.get("width").and_then(|value| dimension(value, false));
    let height =
        attrs.pairs.get("height").and_then(|value| dimension(value, true));
    match (width, height) {
        (Some(width), Some(height)) => format!("width={width},height={height}"),
        (Some(width), None) => {
            format!("width={width},height=\\textheight,keepaspectratio")
        }
        (None, Some(height)) => {
            format!("height={height},width=\\linewidth,keepaspectratio")
        }
        (None, None) if figure => {
            "width=\\linewidth,height=\\textheight,keepaspectratio".into()
        }
        (None, None) => String::new(),
    }
}

fn dimension(value: &str, height: bool) -> Option<String> {
    let (number, unit) = ["%", "px", "pt", "cm", "mm", "in", "em"]
        .into_iter()
        .find_map(|unit| value.strip_suffix(unit).map(|number| (number, unit)))
        .unwrap_or((value, "px"));
    let mut parts = number.split('.');
    let digits = |part: &str| {
        !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
    };
    if !parts.next().is_some_and(digits)
        || !parts.next().is_none_or(digits)
        || parts.next().is_some()
    {
        return None;
    }
    let mut number: f64 = number.parse().ok()?;
    if !number.is_finite() {
        return None;
    }
    let unit = match unit {
        "%" => {
            number /= 100.0;
            if height {
                "\\textheight"
            } else {
                "\\linewidth"
            }
        }
        "px" => {
            number /= 96.0;
            "in"
        }
        other => other,
    };
    let number = format!("{number:.4}");
    let number = number.trim_end_matches('0').trim_end_matches('.');
    Some(format!("{number}{unit}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Cell, CellOutput, ColumnSpec, DocMeta, Features, MathDialect,
        OutputCandidate, Resolved, Row, SourceInfo, SourceKind, TableCell,
    };

    fn document(blocks: Vec<Block>) -> Document {
        Document {
            meta: DocMeta::default(),
            blocks,
            features: Features::default(),
            source: SourceInfo {
                path: "/project/main.md".into(),
                base_dir: "/project".into(),
                kind: SourceKind::Markdown,
            },
            assets: Vec::new(),
            includes: Vec::new(),
            resolved: Resolved::default(),
        }
    }

    fn render(blocks: Vec<Block>, registry: &AssetRegistry) -> Emitted {
        super::super::emit(
            &document(blocks),
            registry,
            &Config::default(),
            "report",
        )
    }

    fn image(id: &str) -> Inline {
        Inline::Image {
            alt: vec![Inline::text("Alternative")],
            url: format!("asset:{id}"),
            title: None,
            attrs: Attrs::default(),
        }
    }

    fn figure(
        kind: FigureKind, caption: Option<&str>, body: Vec<Block>, id: &str,
        numbered: bool,
    ) -> Block {
        Block::Figure {
            kind,
            caption: caption.map(|text| vec![Inline::text(text)]),
            body,
            attrs: Attrs { id: Some(id.into()), ..Attrs::default() },
            number: numbered.then(|| "1".into()),
            span: None,
        }
    }

    #[test]
    fn image_dimensions_use_percent_axes_pixel_inches_and_four_decimals() {
        for (source, height, expected) in [
            ("50%", false, "0.5\\linewidth"),
            ("100%", true, "1\\textheight"),
            ("300px", false, "3.125in"),
            ("96", false, "1in"),
            ("1px", false, "0.0104in"),
            ("12.123456pt", false, "12.1235pt"),
            ("2cm", false, "2cm"),
            ("3mm", true, "3mm"),
            ("1.50in", false, "1.5in"),
            ("2em", false, "2em"),
            ("0%", false, "0\\linewidth"),
        ] {
            assert_eq!(dimension(source, height).as_deref(), Some(expected));
        }
        for source in ["", ".5", "1.", "+1", "-1", "NaN", "1e3", "2vw", " 1px"]
        {
            assert!(dimension(source, false).is_none(), "{source}");
        }
        let mut attrs = Attrs::default();
        assert!(image_options(&attrs, false).is_empty());
        assert_eq!(
            image_options(&attrs, true),
            "width=\\linewidth,height=\\textheight,keepaspectratio"
        );
        attrs.pairs.insert("width".into(), "50%".into());
        assert_eq!(
            image_options(&attrs, false),
            "width=0.5\\linewidth,height=\\textheight,keepaspectratio"
        );
        attrs.pairs.insert("height".into(), "96px".into());
        assert_eq!(
            image_options(&attrs, true),
            "width=0.5\\linewidth,height=1in"
        );
        attrs.pairs.remove("width");
        assert_eq!(
            image_options(&attrs, false),
            "height=1in,width=\\linewidth,keepaspectratio"
        );
    }

    #[test]
    fn figure_images_fit_but_repeated_inline_uses_keep_natural_size() {
        let mut registry = AssetRegistry::new();
        let id = registry.register_bytes(vec![1], "image/png", "plot.png");
        let emitted = render(
            vec![
                figure(
                    FigureKind::Image,
                    None,
                    vec![Block::paragraph(vec![image(&id)])],
                    "fig-plot",
                    true,
                ),
                Block::paragraph(vec![image(&id)]),
            ],
            &registry,
        );
        assert!(emitted.diagnostics.is_empty(), "{:?}", emitted.diagnostics);
        assert!(emitted.body.contains(
            "\\includegraphics[width=\\linewidth,height=\\textheight,keepaspectratio]{report_assets/plot.png}",
        ));
        assert!(
            emitted.body.contains("\\includegraphics{report_assets/plot.png}")
        );
        assert!(emitted.body.contains("\\caption{}\\label{"));
        assert_eq!(emitted.images.len(), 1);
        assert_eq!(emitted.images[0].id, id);
        assert!(!emitted.images[0].render);
        assert!(emitted.usage.graphics && emitted.usage.caption);
    }

    #[test]
    fn unsupported_image_sources_keep_flat_escaped_alt_with_one_finding_each() {
        let mut registry = AssetRegistry::new();
        let text = registry.register_bytes(vec![1], "text/plain", "note.txt");
        let urls = [
            "https://example.test/image.png".to_string(),
            "HTTPS://example.test/upper.png".into(),
            "data:image/png;base64,AA==".into(),
            "/absolute.png".into(),
            "missing.png".into(),
            format!("asset:{text}"),
        ];
        let blocks = urls
            .iter()
            .map(|url| {
                Block::paragraph(vec![Inline::Image {
                    alt: vec![Inline::Emph {
                        inlines: vec![Inline::text("Alt_&")],
                    }],
                    url: url.clone(),
                    title: None,
                    attrs: Attrs::default(),
                }])
            })
            .collect();
        let emitted = render(blocks, &registry);
        assert_eq!(emitted.diagnostics.len(), urls.len());
        assert!(
            emitted
                .diagnostics
                .iter()
                .all(|finding| { finding.code == "unsupported-construct" })
        );
        assert!(
            emitted
                .body
                .contains("\\href{https://example.test/image.png}{Alt\\_\\&}")
        );
        assert_eq!(emitted.body.matches("Alt\\_\\&").count(), urls.len());
        assert!(!emitted.body.contains("\\emph"));
        assert!(!emitted.body.contains("\\includegraphics"));
        assert!(emitted.images.is_empty());
        assert!(!emitted.usage.graphics);
    }

    #[test]
    fn moving_captions_keep_protected_notes_and_one_set_of_findings() {
        let caption = vec![
            Inline::text("Title "),
            Inline::Underline { inlines: vec![Inline::text("underlined")] },
            Inline::Footnote {
                blocks: vec![Block::paragraph(vec![Inline::text("NOTE-BODY")])],
                label: Some("note".into()),
                number: Some(1),
            },
            Inline::FootnoteRef { label: "note".into(), number: Some(1) },
            Inline::Math { source: "x^2".into(), dialect: MathDialect::Latex },
            Inline::Image {
                alt: vec![Inline::text("fallback")],
                url: "https://example.test/image.png".into(),
                title: None,
                attrs: Attrs::default(),
            },
        ];
        let emitted = render(
            vec![Block::Figure {
                kind: FigureKind::Other,
                caption: Some(caption),
                body: vec![Block::paragraph(vec![Inline::text("Body")])],
                attrs: Attrs::default(),
                number: Some("1".into()),
                span: None,
            }],
            &AssetRegistry::new(),
        );
        let caption = emitted.body.split("\\caption[").nth(1).unwrap();
        let (short, full) = caption.split_once("]{").unwrap();
        assert!(!short.contains("NOTE-BODY"));
        assert!(!short.contains("\\footnote") && !short.contains("\\footref"));
        assert!(full.contains("\\protect\\footnote{"));
        assert!(full.contains("\\protect\\footref{writer-fn-1}"));
        assert!(full.contains("\\protect\\uline{underlined}"));
        assert!(full.contains("\\texorpdfstring{\\(x^2\\)}"));
        assert_eq!(emitted.body.matches("NOTE-BODY").count(), 1);
        assert_eq!(
            emitted
                .diagnostics
                .iter()
                .filter(|finding| { finding.code == "unsupported-construct" })
                .count(),
            1
        );
    }

    #[test]
    fn table_and_listing_figures_delegate_without_an_outer_figure_wrapper() {
        let table = Block::Table {
            columns: vec![ColumnSpec::default()],
            head: Vec::new(),
            body: vec![Row {
                cells: vec![TableCell {
                    inlines: vec![Inline::text("Cell")],
                    ..TableCell::default()
                }],
            }],
            foot: Vec::new(),
            attrs: Attrs::default(),
            span: None,
        };
        let code = Block::CodeBlock {
            lang: Some("text".into()),
            text: "literal code".into(),
            attrs: Attrs::default(),
            span: None,
        };
        let emitted = render(
            vec![
                figure(
                    FigureKind::Table,
                    Some("Table caption"),
                    vec![table],
                    "tbl-test",
                    true,
                ),
                figure(
                    FigureKind::Listing,
                    Some("Listing caption"),
                    vec![code],
                    "lst-test",
                    true,
                ),
            ],
            &AssetRegistry::new(),
        );
        assert!(!emitted.body.contains("\\begin{figure}"));
        assert_eq!(emitted.body.matches("\\begin{longtable}").count(), 1);
        assert_eq!(emitted.body.matches("\\begin{Verbatim}").count(), 1);
        assert!(
            emitted.body.find("Table caption").unwrap()
                < emitted.body.find("Cell").unwrap()
        );
        assert!(
            emitted.body.find("Listing caption").unwrap()
                < emitted.body.find("literal code").unwrap()
        );
        assert_eq!(emitted.body.matches("\\caption{").count(), 2);
    }

    #[test]
    fn delegated_body_and_caption_findings_keep_their_distinct_files_and_cells()
    {
        let outer = SourceSpan {
            start_line: 2,
            end_line: 4,
            cell: None,
            file: Some("/project/outer.md".into()),
        };
        let inner = SourceSpan {
            start_line: 7,
            end_line: 9,
            cell: Some(3),
            file: Some("/project/child.ipynb".into()),
        };
        let table = Block::Table {
            columns: vec![ColumnSpec::default()],
            head: Vec::new(),
            body: vec![Row {
                cells: vec![TableCell {
                    inlines: vec![Inline::text("Cell")],
                    rowspan: 2,
                    ..TableCell::default()
                }],
            }],
            foot: Vec::new(),
            attrs: Attrs::default(),
            span: Some(inner.clone()),
        };
        let code = Block::CodeBlock {
            lang: Some("text".into()),
            text: "\\end{Verbatim}".into(),
            attrs: Attrs::default(),
            span: Some(inner.clone()),
        };
        for (kind, body, code) in [
            (FigureKind::Table, table, "latex-rowspan"),
            (FigureKind::Listing, code, "latex-verbatim-end"),
        ] {
            let emitted = render(
                vec![Block::Figure {
                    kind,
                    caption: Some(vec![Inline::Image {
                        alt: vec![Inline::text("Caption alt")],
                        url: "https://example.test/caption.png".into(),
                        title: None,
                        attrs: Attrs::default(),
                    }]),
                    body: vec![body],
                    attrs: Attrs::default(),
                    number: Some("1".into()),
                    span: Some(outer.clone()),
                }],
                &AssetRegistry::new(),
            );
            let body = emitted
                .diagnostics
                .iter()
                .find(|finding| finding.code == code)
                .unwrap();
            assert_eq!(body.span.as_ref(), Some(&inner));
            assert_eq!(body.path.as_ref(), inner.file.as_ref());
            let caption = emitted
                .diagnostics
                .iter()
                .find(|finding| finding.code == "unsupported-construct")
                .unwrap();
            assert_eq!(caption.span.as_ref(), Some(&outer));
            assert_eq!(caption.path.as_ref(), outer.file.as_ref());
        }
    }

    #[test]
    fn subfigure_labels_follow_the_container_and_ignore_unrendered_candidates()
    {
        let mut registry = AssetRegistry::new();
        let id = registry.register_bytes(vec![1], "image/png", "plot.png");
        let child = |label: &str| {
            figure(
                FigureKind::Image,
                Some("Small caption"),
                vec![Block::paragraph(vec![image(&id)])],
                label,
                true,
            )
        };
        let selected = Block::Cell(Cell {
            show_output: true,
            outputs: vec![CellOutput {
                candidates: vec![
                    OutputCandidate {
                        mime: "text/html".into(),
                        blocks: vec![
                            Block::Raw {
                                format: "html".into(),
                                text: String::new(),
                                span: None,
                            },
                            child("fig-hidden"),
                        ],
                    },
                    OutputCandidate {
                        mime: "image/png".into(),
                        blocks: vec![child("fig-picked")],
                    },
                ],
                ..CellOutput::default()
            }],
            ..Cell::default()
        });
        let emitted = render(
            vec![
                figure(
                    FigureKind::Other,
                    Some("Container caption"),
                    vec![child("fig-shown"), selected],
                    "fig-container",
                    true,
                ),
                figure(
                    FigureKind::Other,
                    None,
                    vec![child("fig-counterless")],
                    "fig-next",
                    false,
                ),
            ],
            &registry,
        );
        let caption =
            emitted.body.find("\\caption{Container caption}").unwrap();
        for target in ["fig-shown", "fig-picked"] {
            let label =
                format!("\\label{{{}}}", crate::resolve::anchor(target));
            assert_eq!(emitted.body.matches(&label).count(), 1);
            assert!(emitted.body.find(&label).unwrap() > caption);
            assert!(
                !emitted.body.contains(&format!("\\phantomsection{label}"))
            );
        }
        assert!(!emitted.body.contains(&crate::resolve::anchor("fig-hidden")));
        assert_eq!(emitted.body.matches("\\begin{figure}").count(), 2);
        assert_eq!(
            emitted.body.matches("\\par{\\small Small caption}\\par").count(),
            3
        );
        assert!(emitted.body.contains(&format!(
            "\\phantomsection\\label{{{}}}",
            crate::resolve::anchor("fig-counterless"),
        )));
        assert_eq!(emitted.images.len(), 1);
        assert_eq!(emitted.images[0].id, id);
    }
}
