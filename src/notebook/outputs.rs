//! Notebook output candidates and sandboxed local assets (§4 and §6).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_PAD_INDIFFERENT;
use comrak::nodes::NodeValue;
use comrak::{Arena, parse_document};
use serde_json::Value;

use crate::assets::registry::{AssetData, AssetRegistry, resolve_path};
use crate::config::Config;
use crate::error::{Diagnostic, Diagnostics, Result};
use crate::ir::{
    self, Attrs, Block, CellOutput, Document, Inline, MathDialect,
    OutputCandidate, OutputKind, SourceInfo, SourceSpan,
};
use crate::markdown::{self, Fragment};

use super::nbformat::{Cell, MimeBundle, Output, mime_text};
use super::options::CellOptions;
use super::{ansi, tables};

#[derive(Default)]
pub(crate) struct RenderedOutputs {
    pub(crate) outputs: Vec<CellOutput>,
    pub(crate) html_captions: BTreeMap<usize, String>,
}

type MimeResult =
    Result<std::result::Result<(Vec<Block>, Option<String>), String>>;

#[derive(Clone)]
struct WrittenUrl {
    url: String,
    line: usize,
}

type WrittenUrls = BTreeMap<String, Vec<WrittenUrl>>;

/// Converts visible outputs, preserving priority order within each bundle.
pub(crate) fn render(
    cell: &Cell, index: usize, source: &SourceInfo, config: &Config,
    options: &CellOptions, assets: &mut AssetRegistry,
    diagnostics: &mut Diagnostics,
) -> Result<RenderedOutputs> {
    if !options.show_output {
        return Ok(RenderedOutputs::default());
    }
    let mut renderer = Renderer {
        index,
        source,
        config,
        options,
        assets,
        diagnostics,
        span: cell.span(index),
    };
    let mut rendered = RenderedOutputs::default();
    let mut stream: Option<(usize, &str, String)> = None;
    for (j, output) in cell.outputs.iter().enumerate() {
        match output {
            Output::Stream { name, .. }
                if name == "stderr" && !options.warning =>
            {
                continue;
            }
            Output::Error { .. } if !options.error => continue,
            Output::Stream { name, text } => {
                match &mut stream {
                    Some((_, open, buffer)) if *open == name.as_str() => {
                        buffer.push_str(&text.0);
                    }
                    _ => {
                        renderer.flush_stream(&mut stream, &mut rendered)?;
                        stream = Some((j, name, text.0.clone()));
                    }
                }
                continue;
            }
            _ => renderer.flush_stream(&mut stream, &mut rendered)?,
        }
        match output {
            Output::Error { ename, evalue, traceback } => {
                let text = if traceback.is_empty() {
                    format!("{ename}: {evalue}")
                } else {
                    traceback.join("\n")
                };
                let block = ansi::code_block(&text, "output-error");
                rendered.outputs.push(CellOutput {
                    kind: OutputKind::Error {
                        ename: ename.clone(),
                        evalue: evalue.clone(),
                    },
                    candidates: vec![
                        renderer.candidate("text/x-ansi", vec![block]),
                    ],
                    ..CellOutput::default()
                });
            }
            Output::DisplayData { data, metadata } => renderer.bundle(
                j,
                data,
                metadata,
                OutputKind::Display,
                None,
                &mut rendered,
            )?,
            Output::ExecuteResult { data, metadata, execution_count } => {
                renderer.bundle(
                    j,
                    data,
                    metadata,
                    OutputKind::Result,
                    *execution_count,
                    &mut rendered,
                )?
            }
            Output::Stream { .. } => unreachable!("streams handled above"),
        }
    }
    renderer.flush_stream(&mut stream, &mut rendered)?;
    Ok(rendered)
}

struct Renderer<'a> {
    index: usize,
    source: &'a SourceInfo,
    config: &'a Config,
    options: &'a CellOptions,
    assets: &'a mut AssetRegistry,
    diagnostics: &'a mut Diagnostics,
    span: SourceSpan,
}

impl Renderer<'_> {
    fn warn(&mut self, j: usize, code: &'static str, text: impl AsRef<str>) {
        self.diagnostics.push(
            Diagnostic::warning(code, format!("output {j}: {}", text.as_ref()))
                .with_path(&self.source.path)
                .with_span(self.span.clone()),
        );
    }

    fn candidate(&self, mime: &str, blocks: Vec<Block>) -> OutputCandidate {
        let mut document = fragment_document(blocks, self.source);
        ir::walk_blocks_mut(&mut document, &mut |block| {
            set_span(block, &self.span)
        });
        OutputCandidate { mime: mime.into(), blocks: document.blocks }
    }

    fn flush_stream(
        &mut self, stream: &mut Option<(usize, &str, String)>,
        rendered: &mut RenderedOutputs,
    ) -> Result<()> {
        let Some((j, name, text)) = stream.take() else {
            return Ok(());
        };
        let text = apply_carriage_returns(&text);
        let text = text.strip_suffix('\n').unwrap_or(&text);
        if text.is_empty() {
            return Ok(());
        }
        let blocks = if name == "stdout" && self.options.output_asis {
            self.markdown(j, &ansi::strip(text))?
        } else {
            let class = if name == "stderr" {
                "output-stream-stderr"
            } else {
                "output-stream-stdout"
            };
            vec![ansi::code_block(text, class)]
        };
        let mut output = CellOutput {
            kind: OutputKind::Stream { name: name.into() },
            candidates: vec![self.candidate("text/x-ansi", blocks)],
            ..CellOutput::default()
        };
        ensure_last_candidate(&mut output, self.source);
        enforce_numbered_with_source(&mut output, self.source);
        rendered.outputs.push(output);
        Ok(())
    }

    fn bundle(
        &mut self, j: usize, data: &MimeBundle, metadata: &Value,
        kind: OutputKind, execution_count: Option<i64>,
        rendered: &mut RenderedOutputs,
    ) -> Result<()> {
        let mut output = CellOutput {
            kind,
            execution_count,
            mimes: data.keys().cloned().collect(),
            candidates: Vec::new(),
        };
        let mut caption = None;
        let config = self.config;
        for mime in &config.notebook.mime_priority {
            let Some(value) = data.get(mime) else {
                continue;
            };
            match self.mime(j, mime, value, metadata)? {
                Ok((blocks, table_caption)) => {
                    caption = caption.or(table_caption);
                    output.candidates.push(self.candidate(mime, blocks));
                }
                Err(reason) => {
                    self.warn(j, "output-invalid", format!("{mime}: {reason}"))
                }
            }
        }
        ensure_last_candidate(&mut output, self.source);
        if output.candidates.is_empty() {
            self.warn(
                j,
                "output-unsupported",
                format!("no renderer for {}", output.mimes.join(", ")),
            );
        } else if let Some(mime) = data.keys().find(|mime| interactive(mime)) {
            self.warn(
                j,
                "output-unsupported",
                format!(
                    "interactive {mime} rendered as {}",
                    output.candidates[0].mime,
                ),
            );
        }
        enforce_numbered_with_source(&mut output, self.source);
        if let Some(caption) = caption
            && output.candidates.iter().any(|candidate| {
                let doc =
                    fragment_document(candidate.blocks.clone(), self.source);
                let mut table = false;
                ir::walk_blocks(&doc, &mut |block| {
                    table |= matches!(block, Block::Table { .. })
                });
                table
            })
        {
            rendered.html_captions.insert(rendered.outputs.len(), caption);
        }
        rendered.outputs.push(output);
        Ok(())
    }

    fn mime(
        &mut self, j: usize, mime: &str, value: &Value, metadata: &Value,
    ) -> MimeResult {
        if mime == "application/json"
            || mime.starts_with("application/") && mime.ends_with("+json")
        {
            return Ok(Ok((
                vec![Block::CodeBlock {
                    lang: Some("json".into()),
                    text: serde_json::to_string_pretty(value)?,
                    attrs: Attrs {
                        classes: vec!["output-json".into()],
                        ..Attrs::default()
                    },
                    span: None,
                }],
                None,
            )));
        }
        if !matches!(
            mime,
            "image/png"
                | "image/jpeg"
                | "image/gif"
                | "image/webp"
                | "image/svg+xml"
                | "text/html"
                | "text/markdown"
                | "text/latex"
                | "text/plain"
        ) {
            return Ok(Err("no renderer".into()));
        }
        let Some(text) = mime_text(value) else {
            return Ok(Err("payload is not text".into()));
        };
        let blocks = match mime {
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
            | "image/svg+xml" => {
                return Ok(self
                    .image(j, mime, &text, metadata)
                    .map(|block| (vec![block], None)));
            }
            "text/html" => {
                if text.to_ascii_lowercase().contains("<script") {
                    self.warn(j, "output-script", "HTML contains <script");
                }
                if self.options.html_table_processing
                    && let Some(table) = tables::parse_html_table(&text)
                {
                    let mut blocks = vec![table.table];
                    if let Some(note) = table.note {
                        blocks
                            .push(Block::paragraph(vec![Inline::text(&note)]));
                    }
                    return Ok(Ok((blocks, table.caption)));
                }
                vec![self.html(j, &text)?]
            }
            "text/markdown" => self.markdown(j, &text)?,
            "text/latex" => {
                let stripped = strip_math_delimiters(&text);
                if stripped != text.trim() {
                    vec![Block::Math {
                        source: stripped.into(),
                        dialect: MathDialect::Latex,
                        attrs: Attrs::default(),
                        number: None,
                        span: None,
                    }]
                } else {
                    vec![Block::Raw {
                        format: "latex".into(),
                        text,
                        span: None,
                    }]
                }
            }
            "text/plain" if self.options.output_asis => {
                self.markdown(j, &ansi::strip(&text))?
            }
            "text/plain" => vec![ansi::code_block(&text, "output-text")],
            _ => return Ok(Err("no renderer".into())),
        };
        Ok(Ok((blocks, None)))
    }

    fn image(
        &mut self, j: usize, mime: &str, text: &str, metadata: &Value,
    ) -> std::result::Result<Block, String> {
        let (bytes, extension) = if mime == "image/svg+xml" {
            if !text.contains("<svg") {
                return Err("SVG payload has no <svg".into());
            }
            (text.as_bytes().to_vec(), "svg")
        } else {
            let encoded: Vec<u8> = text
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect();
            let bytes = STANDARD_PAD_INDIFFERENT
                .decode(encoded)
                .map_err(|error| format!("invalid base64: {error}"))?;
            if bytes.is_empty() {
                return Err("base64 payload is empty".into());
            }
            let extension = match mime {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/gif" => "gif",
                _ => "webp",
            };
            (bytes, extension)
        };
        let stem = self
            .source
            .path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "._-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let hint = format!("{stem}_cell{}_out{j}.{extension}", self.index);
        let id = self.assets.register_bytes(bytes, mime, &hint);
        let pairs = ["width", "height"]
            .into_iter()
            .filter_map(|key| {
                dimension(metadata, mime, key).map(|value| (key.into(), value))
            })
            .collect();
        Ok(Block::paragraph(vec![Inline::Image {
            alt: Vec::new(),
            url: AssetRegistry::placeholder(&id),
            title: None,
            attrs: Attrs { pairs, ..Attrs::default() },
        }]))
    }

    fn html(&mut self, j: usize, text: &str) -> Result<Block> {
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let mut html = text.to_owned();
        let mut fragment = Fragment {
            assets: &mut assets,
            base_dir: &self.source.base_dir,
            diagnostics: &mut diagnostics,
            file: None,
            cell: Some(self.index),
            missing_asset_is_error: false,
        };
        markdown::walker::rewrite_html_snippet(
            &mut html,
            Some(&self.span),
            &mut fragment,
            &mut crate::assets::HtmlScan::default(),
        )?;
        let written = html_urls(text, &self.source.base_dir, 1);
        let mut blocks =
            vec![Block::Raw { format: "html".into(), text: html, span: None }];
        self.finish_files(j, &mut blocks, assets, written);
        self.output_diagnostics(j, diagnostics);
        Ok(blocks.remove(0))
    }

    fn markdown(&mut self, j: usize, text: &str) -> Result<Vec<Block>> {
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let (text, written, includes) =
            prepare_markdown(text, self.config, self.source);
        for _ in 0..includes {
            self.warn(
                j,
                "output-include",
                ":::include is not expanded in outputs",
            );
        }
        let mut blocks = markdown::parse_blocks(
            &text,
            self.config,
            &mut Fragment {
                assets: &mut assets,
                base_dir: &self.source.base_dir,
                diagnostics: &mut diagnostics,
                file: None,
                cell: Some(self.index),
                missing_asset_is_error: false,
            },
        )?;
        self.finish_files(j, &mut blocks, assets, written);
        let mut document = fragment_document(blocks, self.source);
        let prefix = format!("cell{}-out{j}-", self.index);
        ir::walk_blocks_mut(&mut document, &mut |block| {
            own_inlines_mut(block, &mut |inline| match inline {
                Inline::Footnote { label: Some(label), .. }
                | Inline::FootnoteRef { label, .. } => {
                    label.insert_str(0, &prefix)
                }
                _ => {}
            });
        });
        self.output_diagnostics(j, diagnostics);
        Ok(document.blocks)
    }

    fn output_diagnostics(&mut self, j: usize, diagnostics: Diagnostics) {
        for mut diagnostic in diagnostics.into_vec() {
            diagnostic.message = format!("output {j}: {}", diagnostic.message);
            diagnostic.path = Some(self.source.path.clone());
            diagnostic.span = Some(self.span.clone());
            self.diagnostics.push(diagnostic);
        }
    }

    fn finish_files(
        &mut self, j: usize, blocks: &mut Vec<Block>,
        mut scratch: AssetRegistry, written: WrittenUrls,
    ) {
        let outside = split_outside(&mut scratch, &self.source.base_dir);
        let mut replacements: BTreeMap<_, _> = outside
            .iter()
            .map(|(id, path)| {
                let originals = written.get(id).cloned().unwrap_or_else(|| {
                    vec![WrittenUrl {
                        url: relative_path(path, &self.source.base_dir),
                        line: 0,
                    }]
                });
                let original = &originals[0].url;
                self.warn(
                    j,
                    "output-file-outside",
                    format!(
                        "file `{original}` is outside the notebook directory"
                    ),
                );
                (id.clone(), originals)
            })
            .collect();
        if !replacements.is_empty() {
            let mut document =
                fragment_document(std::mem::take(blocks), self.source);
            ir::walk_blocks_mut(&mut document, &mut |block| {
                let span = block.span().cloned();
                if let Block::Raw { format, text, .. } = block
                    && format == "html"
                {
                    restore_html(text, &mut replacements, span.as_ref());
                }
                own_inlines_mut(block, &mut |inline| match inline {
                    Inline::Image { url, .. } | Inline::Link { url, .. } => {
                        restore_url(url, &mut replacements, span.as_ref())
                    }
                    Inline::Raw { format, text } if format == "html" => {
                        restore_html(text, &mut replacements, span.as_ref())
                    }
                    _ => {}
                });
            });
            *blocks = document.blocks;
        }
        self.assets.merge(scratch);
    }
}

fn interactive(mime: &str) -> bool {
    matches!(
        mime,
        "application/vnd.jupyter.widget-view+json"
            | "application/vnd.plotly.v1+json"
            | "application/javascript"
    ) || mime.starts_with("application/vnd.vega")
        || mime.starts_with("application/vnd.bokehjs")
}

fn fragment_document(blocks: Vec<Block>, source: &SourceInfo) -> Document {
    Document {
        meta: Default::default(),
        blocks,
        features: Default::default(),
        source: source.clone(),
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Default::default(),
    }
}

fn contains_raw(blocks: &[Block], source: &SourceInfo) -> bool {
    let document = fragment_document(blocks.to_vec(), source);
    let mut raw = false;
    ir::walk_blocks(&document, &mut |block| {
        raw |= matches!(block, Block::Raw { .. })
    });
    raw
}

fn ensure_last_candidate(output: &mut CellOutput, source: &SourceInfo) {
    if output
        .candidates
        .last()
        .is_some_and(|candidate| contains_raw(&candidate.blocks, source))
    {
        output
            .candidates
            .push(OutputCandidate { mime: String::new(), blocks: Vec::new() });
    }
}

/// Keeps numbering and references identical in every target rendering.
pub(crate) fn enforce_numbered(output: &mut CellOutput) {
    let source = SourceInfo {
        path: PathBuf::new(),
        base_dir: PathBuf::new(),
        kind: ir::SourceKind::Notebook,
    };
    enforce_numbered_with_source(output, &source);
}

fn enforce_numbered_with_source(output: &mut CellOutput, source: &SourceInfo) {
    let numbered = output.candidates.iter().any(|candidate| {
        let document = fragment_document(candidate.blocks.clone(), source);
        let mut numbered = false;
        ir::walk_blocks(&document, &mut |block| {
            numbered |= matches!(
                block,
                Block::Heading { .. }
                    | Block::Figure { .. }
                    | Block::Math { .. }
            );
            numbered |= block_attrs(block).is_some_and(has_id);
        });
        ir::walk_inlines(&document, &mut |inline| {
            numbered |= matches!(
                inline,
                Inline::Footnote { .. }
                    | Inline::FootnoteRef { .. }
                    | Inline::Ref { .. }
                    | Inline::Cite { .. }
            );
            numbered |= match inline {
                Inline::Code { attrs, .. }
                | Inline::Link { attrs, .. }
                | Inline::Image { attrs, .. }
                | Inline::Span { attrs, .. } => has_id(attrs),
                _ => false,
            };
        });
        numbered
    });
    if numbered {
        let first = output
            .candidates
            .iter()
            .position(|candidate| !contains_raw(&candidate.blocks, source));
        output.candidates = first
            .map(|i| vec![output.candidates.remove(i)])
            .unwrap_or_default();
    }
}

fn has_id(attrs: &Attrs) -> bool {
    attrs.id.as_ref().is_some_and(|id| !id.is_empty())
}

fn block_attrs(block: &Block) -> Option<&Attrs> {
    match block {
        Block::Heading { attrs, .. }
        | Block::CodeBlock { attrs, .. }
        | Block::Figure { attrs, .. }
        | Block::Table { attrs, .. }
        | Block::Math { attrs, .. }
        | Block::Callout { attrs, .. }
        | Block::Div { attrs, .. } => Some(attrs),
        Block::Cell(cell) => Some(&cell.attrs),
        _ => None,
    }
}

fn set_span(block: &mut Block, value: &SourceSpan) {
    match block {
        Block::Heading { span, .. }
        | Block::Paragraph { span, .. }
        | Block::BlockQuote { span, .. }
        | Block::List { span, .. }
        | Block::DefinitionList { span, .. }
        | Block::CodeBlock { span, .. }
        | Block::Figure { span, .. }
        | Block::Table { span, .. }
        | Block::Math { span, .. }
        | Block::Callout { span, .. }
        | Block::Div { span, .. }
        | Block::Raw { span, .. }
        | Block::ThematicBreak { span }
        | Block::PageBreak { span }
        | Block::Toc { span, .. }
        | Block::Bibliography { span } => *span = Some(value.clone()),
        Block::Cell(cell) => cell.span = Some(value.clone()),
    }
    own_inlines_mut(block, &mut |inline| {
        if let Inline::Ref { span, .. } | Inline::Cite { span, .. } = inline {
            *span = Some(value.clone());
        }
    });
}

fn own_inlines_mut(block: &mut Block, f: &mut dyn FnMut(&mut Inline)) {
    match block {
        Block::Heading { inlines, .. }
        | Block::Paragraph { inlines, .. }
        | Block::Figure { caption: Some(inlines), .. }
        | Block::Callout { title: Some(inlines), .. } => {
            inlines_mut(inlines, f)
        }
        Block::DefinitionList { items, .. } => {
            for item in items {
                inlines_mut(&mut item.term, f);
            }
        }
        Block::Table { head, body, foot, .. } => {
            for row in head.iter_mut().chain(body).chain(foot) {
                for cell in &mut row.cells {
                    inlines_mut(&mut cell.inlines, f);
                }
            }
        }
        _ => {}
    }
}

fn inlines_mut(inlines: &mut [Inline], f: &mut dyn FnMut(&mut Inline)) {
    for inline in inlines {
        f(inline);
        match inline {
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Link { inlines, .. }
            | Inline::Span { inlines, .. } => inlines_mut(inlines, f),
            Inline::Image { alt, .. } => inlines_mut(alt, f),
            Inline::Ref { supplement: Some(inlines), .. } => {
                inlines_mut(inlines, f)
            }
            Inline::Cite { items, .. } => {
                for item in items {
                    if let Some(prefix) = &mut item.prefix {
                        inlines_mut(prefix, f);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        inlines_mut(suffix, f);
                    }
                }
            }
            _ => {}
        }
    }
}

fn prepare_markdown(
    text: &str, config: &Config, source: &SourceInfo,
) -> (String, WrittenUrls, usize) {
    let mut options = markdown::comrak_options(&config.markdown);
    options.extension.front_matter_delimiter = None;
    let (prepared, _) = markdown::directives::colon_prepass(text, &options);
    let arena = Arena::new();
    let root = parse_document(&arena, &prepared, &options);
    let mut blank = Vec::new();
    let mut written = BTreeMap::new();
    for node in root.descendants() {
        let data = node.data();
        match &data.value {
            NodeValue::BlockDirective(info)
                if markdown::directives::parse_directive_info(&info.info)
                    .name
                    == "include" =>
            {
                blank.push(data.sourcepos.start.line..=data.sourcepos.end.line);
            }
            NodeValue::Image(link) => remember_url(
                &link.url, &source.base_dir, data.sourcepos.start.line,
                &mut written,
            ),
            NodeValue::HtmlInline(html) => {
                merge_urls(
                    &mut written,
                    html_urls(
                        html, &source.base_dir, data.sourcepos.start.line,
                    ),
                );
            }
            NodeValue::HtmlBlock(html) => {
                merge_urls(
                    &mut written,
                    html_urls(
                        &html.literal, &source.base_dir,
                        data.sourcepos.start.line,
                    ),
                );
            }
            _ => {}
        }
    }
    let mut cleaned = String::new();
    for (i, line) in text.split_inclusive('\n').enumerate() {
        if blank.iter().any(|range| range.contains(&(i + 1))) {
            if line.ends_with('\n') {
                cleaned.push('\n');
            }
        } else {
            cleaned.push_str(line);
        }
    }
    (cleaned, written, blank.len())
}

fn merge_urls(written: &mut WrittenUrls, more: WrittenUrls) {
    for (id, mut urls) in more {
        written.entry(id).or_default().append(&mut urls);
    }
}

fn html_urls(text: &str, base_dir: &Path, first_line: usize) -> WrittenUrls {
    let mut written = BTreeMap::new();
    for (name, range) in crate::assets::html_attr_values(text) {
        if matches!(name.as_str(), "src" | "poster") {
            let url = html_escape::decode_html_entities(&text[range.clone()]);
            let line = first_line
                + text[..range.start]
                    .bytes()
                    .filter(|&byte| byte == b'\n')
                    .count();
            remember_url(&url, base_dir, line, &mut written);
        }
    }
    written
}

fn remember_url(
    url: &str, base_dir: &Path, line: usize, written: &mut WrittenUrls,
) {
    if !markdown::walker::is_relative_url(url) {
        return;
    }
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let path = base_dir.join(markdown::walker::percent_decode(path));
    if path.is_file() {
        let mut registry = AssetRegistry::new();
        let id = registry.register_file(&path);
        written
            .entry(id)
            .or_default()
            .push(WrittenUrl { url: url.into(), line });
    }
}

fn split_outside(
    registry: &mut AssetRegistry, base_dir: &Path,
) -> BTreeMap<String, PathBuf> {
    let root = std::fs::canonicalize(base_dir).ok();
    let mut outside = BTreeMap::new();
    let mut inside = AssetRegistry::new();
    for asset in std::mem::take(registry).iter() {
        match &asset.data {
            AssetData::File(path) => {
                let within = root.as_ref().is_some_and(|root| {
                    std::fs::canonicalize(path)
                        .is_ok_and(|path| path.starts_with(root))
                });
                if within {
                    inside.register_file(path);
                } else {
                    outside.insert(asset.id.clone(), path.clone());
                }
            }
            AssetData::Bytes(bytes) => {
                inside.register_bytes(
                    bytes.clone(),
                    &asset.mime,
                    &asset.file_name_hint,
                );
            }
        }
    }
    *registry = inside;
    outside
}

fn restore_url(
    url: &mut String, replacements: &mut WrittenUrls, span: Option<&SourceSpan>,
) {
    let Some(rest) = url.strip_prefix("asset:") else {
        return;
    };
    let id = rest.get(..12).unwrap_or(rest);
    if let Some(originals) = replacements.get_mut(id) {
        let suffix = &rest[id.len()..];
        let same_suffix = |original: &WrittenUrl| {
            let written = &original.url;
            &written[written.find(['?', '#']).unwrap_or(written.len())..]
                == suffix
        };
        let index = originals
            .iter()
            .position(|original| {
                same_suffix(original)
                    && span.is_none_or(|span| {
                        (span.start_line..=span.end_line)
                            .contains(&original.line)
                    })
            })
            .or_else(|| originals.iter().position(same_suffix))
            .unwrap_or(0);
        let written = if originals.len() > 1 {
            originals.remove(index).url
        } else {
            originals[index].url.clone()
        };
        *url = if written.contains(['?', '#']) {
            written
        } else {
            format!("{written}{suffix}")
        };
    }
}

fn restore_html(
    text: &mut String, replacements: &mut WrittenUrls,
    span: Option<&SourceSpan>,
) {
    let mut edits = Vec::new();
    for (_, range) in crate::assets::html_attr_values(text) {
        let mut url = html_escape::decode_html_entities(&text[range.clone()])
            .into_owned();
        if !url.starts_with("asset:") {
            continue;
        }
        restore_url(&mut url, replacements, span);
        edits.push((
            range,
            html_escape::encode_quoted_attribute(&url).into_owned(),
        ));
    }
    for (range, url) in edits.into_iter().rev() {
        text.replace_range(range, &url);
    }
}

fn relative_path(path: &Path, base_dir: &Path) -> String {
    let base = resolve_path(base_dir);
    let mut target = path.components().peekable();
    let mut source = base.components().peekable();
    while source.peek() == target.peek() && source.peek().is_some() {
        source.next();
        target.next();
    }
    if source.peek().is_some_and(|part| {
        matches!(part, Component::Prefix(_) | Component::RootDir)
    }) || target.peek().is_some_and(|part| {
        matches!(part, Component::Prefix(_) | Component::RootDir)
    }) {
        return path.to_string_lossy().replace('\\', "/");
    }
    let mut relative = PathBuf::new();
    for _ in source {
        relative.push("..");
    }
    for part in target {
        relative.push(part);
    }
    relative.to_string_lossy().replace('\\', "/")
}

fn dimension(metadata: &Value, mime: &str, key: &str) -> Option<String> {
    let value = metadata
        .get(mime)
        .and_then(|per_mime| per_mime.get(key))
        .filter(|value| !value.is_null())
        .or_else(|| metadata.get(key))?;
    if let Some(integer) = value.as_u64() {
        return Some(integer.to_string());
    }
    let number = value.as_f64()?;
    (number.is_finite() && number >= 0.0).then(|| number.to_string())
}

fn strip_math_delimiters(tex: &str) -> &str {
    let tex = tex.trim();
    for (open, close) in
        [("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")]
    {
        let Some(inner) = tex
            .strip_prefix(open)
            .and_then(|rest| rest.strip_suffix(close))
            .map(str::trim)
        else {
            continue;
        };
        let dollars = unescaped_dollars(inner);
        let ambiguous = match open {
            "$$" => dollars.windows(2).any(|pair| pair[1] == pair[0] + 1),
            "$" => !dollars.is_empty(),
            _ => false,
        };
        if !inner.is_empty() && !ambiguous {
            return inner;
        }
    }
    tex
}

fn unescaped_dollars(text: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut escaped = false;
    for (at, c) in text.char_indices() {
        match c {
            '$' if !escaped => positions.push(at),
            '\\' => {
                escaped = !escaped;
                continue;
            }
            _ => {}
        }
        escaped = false;
    }
    positions
}

fn apply_carriage_returns(text: &str) -> Cow<'_, str> {
    if !text.contains('\r') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let keep_from = content.as_bytes()[..content.len().saturating_sub(1)]
            .iter()
            .rposition(|&byte| byte == b'\r')
            .map_or(0, |at| at + 1);
        out.push_str(&line[keep_from..]);
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn source(base_dir: &Path) -> SourceInfo {
        SourceInfo {
            path: base_dir.join("a notebook.ipynb"),
            base_dir: base_dir.into(),
            kind: ir::SourceKind::Notebook,
        }
    }

    fn convert(
        outputs: Value, options: CellOptions, base_dir: &Path,
    ) -> (RenderedOutputs, AssetRegistry, Diagnostics) {
        let cell: Cell = serde_json::from_value(json!({
            "cell_type": "code", "source": ["first\n", "second\n"],
            "outputs": outputs,
        }))
        .unwrap();
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let rendered = render(
            &cell,
            3,
            &source(base_dir),
            &Config::default(),
            &options,
            &mut assets,
            &mut diagnostics,
        )
        .unwrap();
        (rendered, assets, diagnostics)
    }

    fn code(candidate: &OutputCandidate) -> &str {
        let Block::CodeBlock { text, .. } = &candidate.blocks[0] else {
            panic!("expected code output")
        };
        text
    }

    #[test]
    fn carriage_returns() {
        for (text, expected) in [
            ("plain", "plain"),
            ("a\rb\rc", "c"),
            ("10%\r20%\nA\rB\n", "20%\nB\n"),
            ("line\r\nnext\r", "line\r\nnext\r"),
            ("\r", "\r"),
            ("\n\r\n", "\n\r\n"),
        ] {
            assert_eq!(apply_carriage_returns(text), expected, "{text:?}");
        }
    }

    #[test]
    fn streams_merge() {
        let outputs = json!([
            {"output_type":"stream","name":"stdout","text":"10%\r"},
            {"output_type":"stream","name":"stderr","text":"hidden"},
            {"output_type":"error","ename":"hidden","evalue":"","traceback":[]},
            {"output_type":"stream","name":"stdout","text":"100%\n\n"},
            {"output_type":"stream","name":"stderr","text":"hidden"},
        ]);
        let options = CellOptions {
            warning: false,
            error: false,
            ..CellOptions::default()
        };
        let (rendered, _, diagnostics) =
            convert(outputs, options, Path::new("."));
        assert!(diagnostics.is_empty());
        assert_eq!(rendered.outputs.len(), 1);
        assert_eq!(code(&rendered.outputs[0].candidates[0]), "100%\n");
        assert!(rendered.outputs[0].mimes.is_empty());

        let (rendered, _, _) = convert(
            json!([
                {"output_type":"stream","name":"stdout","text":"\n"},
                {"output_type":"stream","name":"stderr","text":"oops\n"},
                {"output_type":"stream","name":"stdout","text":"done\n"},
            ]),
            CellOptions::default(),
            Path::new("."),
        );
        assert_eq!(rendered.outputs.len(), 2);
        assert_eq!(code(&rendered.outputs[0].candidates[0]), "oops");
        assert_eq!(code(&rendered.outputs[1].candidates[0]), "done");
    }

    #[test]
    fn strip_math_delimiters_cases() {
        for (input, expected) in [
            ("  $$ x^2 $$  ", "x^2"),
            (r"\[ x \]", "x"),
            (r"\( x \)", "x"),
            ("$x$", "x"),
            ("$$a$$ and $$b$$", "$$a$$ and $$b$$"),
            ("$a$ and $b$", "$a$ and $b$"),
            (r"$$\text{for all $x$}$$", r"\text{for all $x$}"),
            (r"$a\$b$", r"a\$b"),
            ("$$ $$", "$$ $$"),
            ("$ $", "$ $"),
            ("plain", "plain"),
        ] {
            assert_eq!(strip_math_delimiters(input), expected, "{input:?}");
        }
    }

    #[test]
    fn last_candidate_guarantee() {
        let (rendered, _, diagnostics) = convert(
            json!([
                {"output_type":"display_data","data":{"text/html":"<b>hello</b>"}},
                {"output_type":"display_data","data":{"text/html":"<i>hi</i>","text/plain":"hi"}},
                {"output_type":"display_data","data":{"application/javascript":"alert(1)"}},
            ]),
            CellOptions::default(),
            Path::new("."),
        );
        let outputs = rendered.outputs;
        assert_eq!(outputs[0].candidates.len(), 2);
        assert!(outputs[0].candidates[1].mime.is_empty());
        assert!(outputs[0].pick("docx").is_empty());
        assert_eq!(outputs[1].candidates.len(), 2);
        assert_eq!(outputs[1].pick("docx"), outputs[1].candidates[1].blocks);
        assert!(outputs[2].candidates.is_empty());
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "output-unsupported")
                .count(),
            1
        );
    }

    #[test]
    fn numbered_output_single_candidate() {
        let (rendered, _, _) = convert(
            json!([
                {"output_type":"display_data","data":{
                    "text/html":"<h1>unselected</h1>", "text/markdown":"# Heading", "text/plain":"Heading"
                }},
                {"output_type":"display_data","data":{
                    "text/latex":"$$ x $$", "text/plain":"x"
                }},
                {"output_type":"display_data","data":{
                    "text/markdown":"one[^1] twice[^1]\n\n[^1]: note", "text/plain":"fallback"
                }},
            ]),
            CellOptions::default(),
            Path::new("."),
        );
        assert_eq!(rendered.outputs.len(), 3);
        for output in &rendered.outputs {
            assert_eq!(output.candidates.len(), 1);
            assert_eq!(output.pick("html"), output.pick("docx"));
        }
        assert!(matches!(
            rendered.outputs[0].candidates[0].blocks[0],
            Block::Heading { .. }
        ));
        assert!(matches!(
            rendered.outputs[1].candidates[0].blocks[0],
            Block::Math { .. }
        ));
        let document = fragment_document(
            rendered.outputs[2].candidates[0].blocks.clone(),
            &source(Path::new(".")),
        );
        let mut labels = Vec::new();
        ir::walk_inlines(&document, &mut |inline| match inline {
            Inline::Footnote { label: Some(label), .. }
            | Inline::FootnoteRef { label, .. } => labels.push(label.clone()),
            _ => {}
        });
        assert_eq!(labels, ["cell3-out2-1", "cell3-out2-1"]);
        ir::walk_blocks(&document, &mut |block| {
            assert_eq!(
                block.span(),
                Some(&SourceSpan {
                    start_line: 1,
                    end_line: 2,
                    cell: Some(3),
                    file: None,
                })
            )
        });
    }

    #[test]
    fn image_assets_and_invalid_fallback() {
        let (rendered, assets, diagnostics) = convert(
            json!([
                {"output_type":"display_data","data":{"image/png":"%%%", "text/plain":"fallback"}},
                {"output_type":"execute_result","data":{"image/png":["YW", "Jj\n"]},"execution_count":7,
                    "metadata":{"image/png":{"width":64.0},"height":-1}},
                {"output_type":"display_data","data":{"image/png":"YWJj"},"metadata":{"width":32,"height":20}},
                {"output_type":"display_data","data":{"image/png":""}},
            ]),
            CellOptions::default(),
            Path::new("."),
        );
        assert_eq!(assets.len(), 1);
        let asset = assets.iter().next().unwrap();
        assert_eq!(asset.file_name_hint, "a_notebook_cell3_out1.png");
        assert_eq!(asset.data, AssetData::Bytes(b"abc".to_vec()));
        assert_eq!(code(&rendered.outputs[0].candidates[0]), "fallback");
        assert_eq!(rendered.outputs[1].execution_count, Some(7));
        let Block::Paragraph { inlines, .. } =
            &rendered.outputs[1].candidates[0].blocks[0]
        else {
            panic!()
        };
        let Inline::Image { attrs, .. } = &inlines[0] else {
            panic!()
        };
        assert_eq!(attrs.pairs.get("width").map(String::as_str), Some("64"));
        assert!(!attrs.pairs.contains_key("height"));
        assert_eq!(
            diagnostics.iter().filter(|d| d.code == "output-invalid").count(),
            2
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "output-unsupported")
                .count(),
            1
        );
    }

    #[test]
    fn output_includes_are_blanked_and_diagnostics_respanned() {
        let options =
            CellOptions { output_asis: true, ..CellOptions::default() };
        let (rendered, _, diagnostics) = convert(
            json!([
                {"output_type":"stream","name":"stdout","text":":::include path=secret.md\n::: \n\n![missing](missing.png)"},
            ]),
            options,
            Path::new("."),
        );
        assert!(diagnostics.iter().any(|d| d.code == "output-include"));
        assert!(diagnostics.iter().any(|d| d.code == "missing-asset"));
        for diagnostic in diagnostics.iter() {
            assert!(diagnostic.message.starts_with("output 0: "));
            assert_eq!(
                diagnostic.path.as_ref(),
                Some(&source(Path::new(".")).path)
            );
            assert_eq!(diagnostic.span.as_ref().unwrap().cell, Some(3));
        }
        assert!(!rendered.outputs[0].candidates[0].blocks.is_empty());
    }

    #[test]
    fn sandboxed_output_files() {
        let root = tempfile::tempdir().unwrap();
        let notebook_dir = root.path().join("notebook");
        std::fs::create_dir(&notebook_dir).unwrap();
        std::fs::write(root.path().join("outside.png"), b"outside").unwrap();
        std::fs::write(notebook_dir.join("inside.png"), b"inside").unwrap();
        let (rendered, assets, diagnostics) = convert(
            json!([
                {"output_type":"display_data","data":{"text/markdown":"![inside](inside.png)\n\n![outside](../outside.png)"}},
                {"output_type":"display_data","data":{"text/html":"<img src='../outside.png?q=1&amp;x=2'><video poster='inside.png'></video>"}},
            ]),
            CellOptions::default(),
            &notebook_dir,
        );
        assert_eq!(assets.len(), 1);
        assert!(
            matches!(&assets.iter().next().unwrap().data, AssetData::File(path) if path.ends_with("inside.png"))
        );
        let document = fragment_document(
            rendered.outputs[0].candidates[0].blocks.clone(),
            &source(&notebook_dir),
        );
        let mut urls = Vec::new();
        ir::walk_inlines(&document, &mut |inline| {
            if let Inline::Image { url, .. } = inline {
                urls.push(url.clone());
            }
        });
        assert!(urls.iter().any(|url| url == "../outside.png"));
        assert!(urls.iter().any(|url| url.starts_with("asset:")));
        let Block::Raw { text, .. } =
            &rendered.outputs[1].candidates[0].blocks[0]
        else {
            panic!()
        };
        assert!(text.contains("src='../outside.png?q=1&amp;x=2'"));
        assert!(text.contains("poster='asset:"));
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "output-file-outside")
                .count(),
            2
        );
    }

    #[test]
    fn outside_aliases_preserve_each_spelling_and_suffix() {
        let root = tempfile::tempdir().unwrap();
        let notebook_dir = root.path().join("notebook");
        std::fs::create_dir(&notebook_dir).unwrap();
        std::fs::write(root.path().join("outside.png"), b"outside").unwrap();
        let (rendered, assets, _) = convert(
            json!([
                {"output_type":"display_data","data":{"text/markdown":
                    "first ![one](../outside.png?v=1) second ![two](.././outside.png?v=2)\n\nthird ![three](../outside.png) fourth ![four](.././outside.png)"
                }},
                {"output_type":"display_data","data":{"text/html":
                    "<img src='../outside.png?v=1'><img src='.././outside.png?v=2'><img src='../outside.png'><img src='.././outside.png'>"
                }},
            ]),
            CellOptions::default(),
            &notebook_dir,
        );
        assert!(assets.is_empty());
        let expected = [
            "../outside.png?v=1", ".././outside.png?v=2", "../outside.png",
            ".././outside.png",
        ];
        let document = fragment_document(
            rendered.outputs[0].candidates[0].blocks.clone(),
            &source(&notebook_dir),
        );
        let mut urls = Vec::new();
        ir::walk_inlines(&document, &mut |inline| {
            if let Inline::Image { url, .. } = inline {
                urls.push(url.clone());
            }
        });
        assert_eq!(urls, expected);
        let Block::Raw { text, .. } =
            &rendered.outputs[1].candidates[0].blocks[0]
        else {
            panic!()
        };
        let urls: Vec<_> = crate::assets::html_attr_values(text)
            .into_iter()
            .filter(|(name, _)| name == "src")
            .map(|(_, range)| text[range].to_owned())
            .collect();
        assert_eq!(urls, expected);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_outside_keeps_lexical_url() {
        let root = tempfile::tempdir().unwrap();
        let notebook_dir = root.path().join("notebook");
        std::fs::create_dir(&notebook_dir).unwrap();
        std::fs::write(root.path().join("outside.png"), b"outside").unwrap();
        std::os::unix::fs::symlink(
            root.path().join("outside.png"),
            notebook_dir.join("link.png"),
        )
        .unwrap();
        let (rendered, assets, diagnostics) = convert(
            json!([
                {"output_type":"display_data","data":{"text/markdown":"![outside](link.png)"}},
            ]),
            CellOptions::default(),
            &notebook_dir,
        );
        assert!(assets.is_empty());
        let document = fragment_document(
            rendered.outputs[0].candidates[0].blocks.clone(),
            &source(&notebook_dir),
        );
        ir::walk_inlines(&document, &mut |inline| {
            if let Inline::Image { url, .. } = inline {
                assert_eq!(url, "link.png");
            }
        });
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "output-file-outside")
                .count(),
            1
        );
    }

    #[test]
    fn table_metadata_and_processing_switch() {
        let outputs = json!([{"output_type":"display_data","data":{"text/html":
            "<table><caption>Caption</caption><tr><td>value</td></tr></table><p>100 rows × 1 columns</p>"
        }}]);
        let (rendered, _, _) =
            convert(outputs.clone(), CellOptions::default(), Path::new("."));
        assert_eq!(
            rendered.html_captions.get(&0).map(String::as_str),
            Some("Caption")
        );
        assert_eq!(rendered.outputs[0].candidates.len(), 1);
        assert_eq!(rendered.outputs[0].candidates[0].blocks.len(), 2);
        let options = CellOptions {
            html_table_processing: false,
            ..CellOptions::default()
        };
        let (rendered, _, _) = convert(outputs, options, Path::new("."));
        assert!(rendered.html_captions.is_empty());
        assert!(matches!(
            rendered.outputs[0].candidates[0].blocks[0],
            Block::Raw { .. }
        ));
    }

    #[test]
    fn interactive_fallback_and_error_text() {
        let (rendered, _, diagnostics) = convert(
            json!([
                {"output_type":"display_data","data":{"application/vnd.plotly.v1+json":{},"text/html":"<script>x</script>","text/plain":"plot"}},
                {"output_type":"error","ename":"Oops","evalue":"bad","traceback":[]},
            ]),
            CellOptions::default(),
            Path::new("."),
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "output-unsupported")
                .count(),
            1
        );
        assert_eq!(
            diagnostics.iter().filter(|d| d.code == "output-script").count(),
            1
        );
        assert_eq!(code(&rendered.outputs[1].candidates[0]), "Oops: bad");
        assert_eq!(
            rendered.outputs[0].pick("docx"),
            rendered.outputs[0].candidates[1].blocks
        );
    }

    #[test]
    fn hidden_outputs_do_not_register_or_diagnose() {
        let options =
            CellOptions { show_output: false, ..CellOptions::default() };
        let (rendered, assets, diagnostics) = convert(
            json!([
                {"output_type":"display_data","data":{"image/png":"%%%","text/html":"<script>x</script>"}},
            ]),
            options,
            Path::new("."),
        );
        assert!(rendered.outputs.is_empty());
        assert!(assets.is_empty());
        assert!(diagnostics.is_empty());
    }
}
