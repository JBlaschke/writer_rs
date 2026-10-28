//! Notebook cell to IR conversion (notebook-mapping.md §2).
//! Owner: T18. Metadata helpers adapted from presenter_rs.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::LazyLock;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_PAD_INDIFFERENT;
use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, parse_document};
use regex::Regex;
use serde_json::Value;

use crate::assets::registry::AssetRegistry;
use crate::config::{Config, Toc};
use crate::error::{Diagnostic, Diagnostics, Error, Result};
use crate::ir::{
    self, Attrs, Author, Block, Cell, DocMeta, Document, FigureKind, Inline,
    Inlines, SourceInfo, SourceKind, SourceSpan,
};
use crate::markdown::frontmatter::{
    meta_from_json, parse_front_matter, split_front_matter,
};
use crate::markdown::{self, Fragment};

use super::nbformat::{self, CellType, Notebook};
use super::options::{self, CellOptions, TextList};
use super::{CellKind, CellParts, CellRef, ParseOptions, Selection, outputs};

const CONFIG_TABLES: &[&str] = &[
    "markdown", "math", "numbering", "references", "html", "pdf", "docx",
    "latex", "typst", "notebook",
];
const META_KEYS: &[&str] = &[
    "title", "subtitle", "author", "authors", "date", "lang", "description",
    "keywords", "abstract", "toc", "numbering", "bibliography", "cite_style",
    "csl", "cite_locale", "theme", "template", "reference_doc",
    "latex_template", "paper",
];

pub(crate) fn convert(
    notebook: &Notebook, source: &SourceInfo, config: &Config,
    opts: ParseOptions<'_>, assets: &mut AssetRegistry,
    diagnostics: &mut Diagnostics,
) -> Result<(Document, Vec<CellParts>)> {
    let mut texts: Vec<_> =
        notebook.cells.iter().map(|c| c.source.0.clone()).collect();
    let (mut meta, effective) =
        metadata(notebook, source, config, &mut texts, assets, diagnostics)?;
    opts.includes.set_parent_meta(&meta);
    let language = notebook.language();
    let mut options: Vec<_> = notebook
        .cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            if cell.cell_type == CellType::Code {
                options::resolve(
                    cell, index, language, &effective.notebook, &source.path,
                    diagnostics,
                )
            } else {
                // Markdown and raw cells honour only the removal tag list.
                let tags = cell.tags();
                let removed = tags.iter().any(|tag| {
                    effective.notebook.remove_cell_tags.iter().any(
                        |configured| {
                            tag.replace('_', "-")
                                == configured.replace('_', "-")
                        },
                    )
                });
                CellOptions {
                    source: cell.source.0.clone(),
                    tags,
                    include: !removed,
                    ..Default::default()
                }
            }
        })
        .collect();
    let definitions =
        definitions(notebook, &options, &effective, &mut texts, diagnostics);
    let selected =
        selected_cells(notebook, &options, &opts.selection, diagnostics);
    let mut converter = Converter {
        notebook,
        source,
        config: &effective,
        assets,
        diagnostics,
        includes: opts.includes,
        definitions,
        seen_notes: BTreeSet::new(),
        included: Vec::new(),
        included_features: Default::default(),
    };
    let mut parts = Vec::new();
    for (index, cell) in notebook.cells.iter().enumerate() {
        if !selected.contains(&index) {
            continue;
        }
        let option = &mut options[index];
        if opts.selection.hide_input {
            option.show_input = false;
            option.lst_label = None;
            option.lst_cap = None;
        }
        if opts.selection.hide_output {
            option.show_output = false;
        }
        let (kind, body) = match cell.cell_type {
            CellType::Markdown => {
                (CellKind::Markdown, converter.markdown(index, &texts[index])?)
            }
            CellType::Code => (CellKind::Code, converter.code(index, option)?),
            CellType::Raw => {
                (CellKind::Raw, converter.raw(index, cell, &texts[index])?)
            }
            CellType::Unknown => continue,
        };
        let (input, output) = if cell.cell_type == CellType::Code {
            // Partial forms must not repeat the full conversion's diagnostics.
            let saved = std::mem::take(converter.diagnostics);
            let mut input = option.clone();
            input.show_output = false;
            input.label = None;
            let input = converter.code(index, &input)?;
            let mut output = option.clone();
            output.show_input = false;
            output.lst_label = None;
            output.lst_cap = None;
            let output = converter.code(index, &output)?;
            *converter.diagnostics = saved;
            (input, output)
        } else {
            (Vec::new(), Vec::new())
        };
        parts.push(CellParts {
            index,
            id: cell.id.clone(),
            tags: option.tags.clone(),
            kind,
            body,
            input,
            outputs: output,
        });
    }
    for definition in &converter.definitions {
        if !converter.seen_notes.contains(&definition.key) {
            converter.diagnostics.push(
                Diagnostic::warning(
                    "footnote-unreferenced",
                    format!(
                        "footnote {:?} is never referenced",
                        definition.label
                    ),
                )
                .with_span(definition.span.clone()),
            );
        }
    }
    let mut blocks: Vec<_> =
        parts.iter().flat_map(|p| p.body.iter().cloned()).collect();
    if !opts.nested
        && let Toc::Depth(depth) = effective.document.toc
    {
        let mut found = false;
        visit_blocks(&mut blocks, &mut |block| {
            found |= matches!(block, Block::Toc { .. });
        });
        if !found {
            blocks.insert(0, Block::Toc { depth, span: None });
        }
    }
    let mut document = Document {
        meta: std::mem::take(&mut meta),
        blocks,
        features: Default::default(),
        source: SourceInfo { kind: SourceKind::Notebook, ..source.clone() },
        assets: converter.assets.infos(&source.base_dir),
        includes: converter.included,
        resolved: Default::default(),
    };
    document.features = markdown::collect_features(&document);
    let mut latex_output = false;
    ir::walk_blocks(&document, &mut |block| {
        if let Block::Cell(cell) = block {
            latex_output |= cell
                .outputs
                .iter()
                .any(|o| o.mimes.iter().any(|m| m == "text/latex"));
        }
    });
    document.features.math |= latex_output;
    document.features.merge(&converter.included_features);
    let code_cells: Vec<_> = selected
        .iter()
        .filter_map(|&index| {
            let cell = &notebook.cells[index];
            (cell.cell_type == CellType::Code).then_some(cell)
        })
        .collect();
    if selected.iter().any(|&index| {
        notebook.cells[index].cell_type == CellType::Code
            && !options[index].source.trim().is_empty()
    }) && code_cells
        .iter()
        .all(|c| c.execution_count.is_none() && c.outputs.is_empty())
    {
        converter.diagnostics.warn(
            "notebook-not-run",
            "notebook has code but no saved execution counts or outputs",
        );
    }
    Ok((document, parts))
}

fn selected_cells(
    notebook: &Notebook, options: &[CellOptions], selection: &Selection,
    diagnostics: &mut Diagnostics,
) -> BTreeSet<usize> {
    let eligible = |index: usize| {
        options[index].include
            && notebook.cells[index].cell_type != CellType::Unknown
    };
    let mut selected = BTreeSet::new();
    if let Some(refs) = &selection.cells {
        for reference in refs {
            let index = match reference {
                CellRef::Index(index) => {
                    (*index < notebook.cells.len()).then_some(*index)
                }
                CellRef::Id(id) => notebook
                    .cells
                    .iter()
                    .position(|c| c.id.as_ref() == Some(id)),
            }
            .filter(|&index| eligible(index));
            if let Some(index) = index {
                selected.insert(index);
            } else {
                diagnostics.error("notebook-cell-not-found", format!("notebook cell {reference:?} was not found or is removed"));
            }
        }
    } else {
        selected
            .extend((0..notebook.cells.len()).filter(|&index| eligible(index)));
    }
    if let Some(tags) = &selection.tags {
        selected.retain(|&index| {
            options[index].tags.iter().any(|tag| tags.contains(tag))
        });
    }
    selected
}

fn metadata(
    notebook: &Notebook, source: &SourceInfo, config: &Config,
    texts: &mut [String], assets: &mut AssetRegistry,
    diagnostics: &mut Diagnostics,
) -> Result<(DocMeta, Config)> {
    let mut meta = DocMeta {
        title: notebook.title().map(str::to_owned),
        authors: notebook
            .authors()
            .into_iter()
            .map(|name| Author { name, ..Default::default() })
            .collect(),
        ..Default::default()
    };
    let mut effective = config.clone();
    if let Some(presenter) = notebook.presenter_meta() {
        let shared = &META_KEYS[..8];
        let mut read = Vec::new();
        let mut ignored = Vec::new();
        let object: serde_json::Map<_, _> = presenter
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(key, value)| {
                if shared.contains(&key.as_str()) {
                    read.push(key.clone());
                    Some((key.clone(), value.clone()))
                } else {
                    ignored.push(key.clone());
                    None
                }
            })
            .collect();
        diagnostics.info(
            "notebook-presenter-metadata",
            format!(
                "metadata.presenter: read {}; ignored {}",
                read.join(", "),
                ignored.join(", ")
            ),
        );
        apply_layer(
            &mut meta,
            &mut effective,
            &Value::Object(object),
            "metadata.presenter",
        )?;
    }
    if let Some(writer) = notebook.writer_meta() {
        apply_layer(&mut meta, &mut effective, writer, "metadata.writer")?;
    }
    let first =
        notebook.cells.iter().position(|c| c.cell_type == CellType::Markdown);
    let mut abstract_cell = None;
    for (index, cell) in notebook.cells.iter().enumerate() {
        if cell.cell_type != CellType::Markdown {
            continue;
        }
        let Some((yaml, rest, consumed)) = split_front_matter(&texts[index])
        else {
            continue;
        };
        let span = SourceSpan {
            start_line: 1,
            end_line: consumed,
            cell: Some(index),
            file: None,
        };
        if Some(index) == first {
            let layer = parse_front_matter(yaml)
                .map_err(|e| in_context(e, &format!("cell {index}")))?;
            effective
                .merge_front_matter(&layer)
                .map_err(|e| in_context(e, &format!("cell {index}")))?;
            empty_front_matter(yaml, &span, diagnostics);
            let keys =
                serde_norway::from_str::<Value>(yaml).unwrap_or(Value::Null);
            if layer.vars.contains_key("abstract") {
                abstract_cell = Some(index);
            }
            overlay(&mut meta, layer, &keys);
            texts[index] = "\n".repeat(consumed) + rest;
        } else if later_metadata_header(yaml) {
            diagnostics.push(
                Diagnostic::warning(
                    "notebook-front-matter-ignored",
                    "front matter in a later Markdown cell is ignored",
                )
                .with_span(span),
            );
            texts[index] = "\n".repeat(consumed) + rest;
        }
    }
    if let Some(Value::String(text)) = meta.vars.remove("abstract") {
        meta.abstract_ = Some(markdown::parse_blocks(
            &text,
            &effective,
            &mut Fragment {
                assets,
                base_dir: &source.base_dir,
                diagnostics,
                file: None,
                cell: abstract_cell,
                missing_asset_is_error: false,
            },
        )?);
    }
    Ok((meta, effective))
}

/// Whether a later Markdown header is recognized and removed from cell text.
pub(crate) fn later_metadata_header(yaml: &str) -> bool {
    parse_front_matter(yaml).is_ok()
        && serde_norway::from_str::<Value>(yaml)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .is_some_and(|map| {
                map.keys().any(|key| {
                    META_KEYS.contains(&key.as_str())
                        || CONFIG_TABLES.contains(&key.as_str())
                })
            })
}

fn apply_layer(
    meta: &mut DocMeta, config: &mut Config, value: &Value, context: &str,
) -> Result<()> {
    let layer = meta_from_json(value).map_err(|e| in_context(e, context))?;
    config.merge_front_matter(&layer).map_err(|e| in_context(e, context))?;
    overlay(meta, layer, value);
    Ok(())
}

fn empty_front_matter(
    yaml: &str, span: &SourceSpan, diagnostics: &mut Diagnostics,
) {
    if !yaml
        .lines()
        .all(|line| line.trim().is_empty() || line.trim().starts_with('#'))
    {
        return;
    }
    let mut findings = Diagnostics::new();
    // Reuse T10's heading/comment distinction after the same YAML was parsed.
    let _ = markdown::frontmatter::parse_document_front_matter(
        yaml, span.end_line, &mut findings,
    );
    for mut finding in findings
        .into_vec()
        .into_iter()
        .filter(|d| d.code == "front-matter-empty")
    {
        finding.span = Some(span.clone());
        diagnostics.push(finding);
    }
}

fn in_context(error: Error, context: &str) -> Error {
    match error {
        Error::FrontMatter(message) => {
            Error::FrontMatter(format!("{context}: {message}"))
        }
        Error::Config(message) => {
            Error::FrontMatter(format!("{context}: {message}"))
        }
        other => other,
    }
}

fn overlay(meta: &mut DocMeta, later: DocMeta, keys: &Value) {
    reconcile_config_aliases(meta, &later);
    for (slot, value) in [
        (&mut meta.title, later.title),
        (&mut meta.subtitle, later.subtitle),
        (&mut meta.date, later.date),
        (&mut meta.description, later.description),
        (&mut meta.lang, later.lang),
    ] {
        if value.is_some() {
            *slot = value;
        }
    }
    if keys.get("author").is_some_and(|v| !v.is_null())
        || keys.get("authors").is_some_and(|v| !v.is_null())
    {
        meta.authors = later.authors;
    }
    if keys.get("keywords").is_some_and(|v| !v.is_null()) {
        meta.keywords = later.keywords;
    }
    for (key, value) in later.vars {
        match (meta.vars.get_mut(&key), value) {
            (Some(Value::Object(base)), Value::Object(over))
                if CONFIG_TABLES.contains(&key.as_str()) =>
            {
                base.extend(over)
            }
            (Some(Value::Object(base)), Value::Bool(flag))
                if key == "numbering" =>
            {
                base.insert("headings".into(), Value::Bool(flag));
            }
            (Some(base @ Value::Bool(_)), Value::Object(mut over))
                if key == "numbering" =>
            {
                if !over.contains_key("headings") {
                    over.insert("headings".into(), base.clone());
                }
                *base = Value::Object(over);
            }
            (Some(_), Value::Null) if CONFIG_TABLES.contains(&key.as_str()) => {
            }
            (_, value) => {
                meta.vars.insert(key, value);
            }
        }
    }
}

/// The final metadata is replayed through `Config::merge_front_matter` by the
/// pipeline. Remove an earlier shorthand when a later table sets its field;
/// otherwise the shorthand would win again during that single merge.
fn reconcile_config_aliases(meta: &mut DocMeta, later: &DocMeta) {
    for (alias, table, field) in [
        ("bibliography", "references", "bibliography"),
        ("cite_style", "references", "style"),
        ("csl", "references", "style"),
        ("cite_locale", "references", "locale"),
        ("theme", "html", "theme"),
        ("template", "pdf", "template"),
        ("paper", "pdf", "paper"),
        ("reference_doc", "docx", "reference_doc"),
        ("latex_template", "latex", "template"),
    ] {
        let table_sets_field = later
            .vars
            .get(table)
            .and_then(Value::as_object)
            .is_some_and(|table| table.contains_key(field));
        let changes_style_alias = matches!(alias, "cite_style" | "csl")
            && (later.vars.contains_key("cite_style")
                || later.vars.contains_key("csl"));
        if table_sets_field || changes_style_alias {
            meta.vars.remove(alias);
        }
    }
}

struct Definition {
    key: String,
    label: String,
    text: String,
    span: SourceSpan,
}

fn normalize_name(name: &str) -> String {
    caseless::default_case_fold_str(
        &name.split_whitespace().collect::<Vec<_>>().join(" "),
    )
}

fn definitions(
    notebook: &Notebook, options: &[CellOptions], config: &Config,
    texts: &mut [String], diagnostics: &mut Diagnostics,
) -> Vec<Definition> {
    let mut definitions = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, cell) in notebook.cells.iter().enumerate() {
        if !options[index].include || !markdown_like(cell) {
            continue;
        }
        let (found, _) = scan(&texts[index], config);
        for (label, start, end) in found {
            let key = normalize_name(&label);
            let span = SourceSpan {
                start_line: start,
                end_line: end,
                cell: Some(index),
                file: None,
            };
            if seen.insert(key.clone()) {
                let text = texts[index]
                    .lines()
                    .skip(start - 1)
                    .take(end - start + 1)
                    .collect::<Vec<_>>()
                    .join("\n");
                definitions.push(Definition { key, label, text, span });
            } else {
                diagnostics.push(Diagnostic::warning("footnote-duplicate", format!("duplicate footnote {label:?}; the first definition wins")).with_span(span));
                texts[index] = blank_range(&texts[index], start, end);
            }
        }
    }
    definitions
}

type ScannedDefinitions = Vec<(String, usize, usize)>;

fn scan(text: &str, config: &Config) -> (ScannedDefinitions, bool) {
    let mut options = markdown::comrak_options(&config.markdown);
    options.extension.front_matter_delimiter = None;
    let (rewritten, _) = markdown::directives::colon_prepass(text, &options);
    let arena = Arena::new();
    let root = parse_document(&arena, &rewritten, &options);
    let definitions = root
        .children()
        .filter_map(|node| {
            let data = node.data();
            match &data.value {
                NodeValue::FootnoteDefinition(definition)
                    if !definition.name.starts_with("__inline_") =>
                {
                    Some((
                        definition.name.clone(),
                        data.sourcepos.start.line,
                        data.sourcepos.end.line,
                    ))
                }
                _ => None,
            }
        })
        .collect();
    (definitions, open_block(root, text))
}

fn open_block<'a>(root: &'a AstNode<'a>, text: &str) -> bool {
    let mut last = root.children().filter(|node| {
        !matches!(&node.data().value, NodeValue::FootnoteDefinition(definition) if definition.name.starts_with("__inline_"))
    }).last();
    while let Some(node) = last {
        let data = node.data();
        match &data.value {
            NodeValue::List(_)
            | NodeValue::Item(_)
            | NodeValue::TaskItem(_)
            | NodeValue::DescriptionList
            | NodeValue::DescriptionItem(_)
            | NodeValue::DescriptionDetails
            | NodeValue::FootnoteDefinition(_) => {
                last = node.last_child();
            }
            NodeValue::CodeBlock(code) => return code.fenced && !code.closed,
            NodeValue::BlockDirective(directive) => {
                return !closing_directive(text, data.sourcepos, directive);
            }
            NodeValue::HtmlBlock(html) => {
                return match html.block_type {
                    1 => !["</script>", "</pre>", "</style>", "</textarea>"]
                        .iter()
                        .any(|s| html.literal.to_ascii_lowercase().contains(s)),
                    2 => !html.literal.contains("-->"),
                    3 => !html.literal.contains("?>"),
                    4 => !html.literal.contains('>'),
                    5 => !html.literal.contains("]]>"),
                    _ => false,
                };
            }
            _ => return false,
        }
    }
    false
}

fn closing_directive(
    text: &str, position: comrak::nodes::Sourcepos,
    directive: &comrak::nodes::NodeBlockDirective,
) -> bool {
    if position.start.line == position.end.line {
        return false;
    }
    let opening = text.lines().nth(position.start.line.saturating_sub(1));
    let closing = text.lines().nth(position.end.line.saturating_sub(1));
    let (Some(opening), Some(closing)) = (opening, closing) else {
        return false;
    };
    let Some(prefix) = opening.as_bytes().get(..position.start.column - 1)
    else {
        return false;
    };
    let columns = |bytes: &[u8]| {
        bytes.iter().fold(0_usize, |column, byte| {
            column + if *byte == b'\t' { 4 - column % 4 } else { 1 }
        })
    };
    let container = columns(prefix).saturating_sub(directive.fence_offset);
    let rest = closing.trim_start_matches([' ', '\t']);
    let prefix = &closing.as_bytes()[..closing.len() - rest.len()];
    let Some(indent) = columns(prefix).checked_sub(container) else {
        return false;
    };
    let count = rest.bytes().take_while(|byte| *byte == b':').count();
    indent <= 3
        && count >= directive.fence_length
        && rest[count..]
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

fn blank_range(text: &str, start: usize, end: usize) -> String {
    text.split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| {
            if (start..=end).contains(&(index + 1)) {
                if line.ends_with('\n') { "\n" } else { "" }
            } else {
                line
            }
        })
        .collect()
}

fn raw_format(cell: &nbformat::Cell) -> String {
    cell.raw_mimetype()
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn markdown_like(cell: &nbformat::Cell) -> bool {
    cell.cell_type == CellType::Markdown
        || (cell.cell_type == CellType::Raw
            && matches!(
                raw_format(cell).as_str(),
                "text/markdown" | "text/x-markdown" | "markdown"
            ))
}

struct Converter<'a> {
    notebook: &'a Notebook,
    source: &'a SourceInfo,
    config: &'a Config,
    assets: &'a mut AssetRegistry,
    diagnostics: &'a mut Diagnostics,
    includes: &'a mut dyn super::CellIncludes,
    definitions: Vec<Definition>,
    seen_notes: BTreeSet<String>,
    included: Vec<PathBuf>,
    included_features: ir::Features,
}

impl Converter<'_> {
    fn warn(
        &mut self, code: &'static str, index: usize, message: impl Into<String>,
    ) {
        let message = message.into();
        if code == "cell-option-unsupported" {
            let mut findings = std::mem::take(self.diagnostics).into_vec();
            let existing = findings.iter_mut().find(|d| {
                d.code == code
                    && d.span.as_ref().and_then(|s| s.cell) == Some(index)
            });
            if let Some(existing) = existing {
                existing.message.push_str(", ");
                existing.message.push_str(&message);
                self.diagnostics.extend(findings);
                return;
            }
            self.diagnostics.extend(findings);
        }
        self.diagnostics.push(
            Diagnostic::warning(code, message)
                .with_span(self.notebook.cells[index].span(index)),
        );
    }

    fn caption(
        &mut self, text: Option<&str>, index: usize,
    ) -> Result<Option<Inlines>> {
        let Some(text) = text else { return Ok(None) };
        let mut diagnostics = Diagnostics::new();
        let inlines = markdown::parse_inlines(
            text,
            self.config,
            &mut Fragment {
                assets: self.assets,
                base_dir: &self.source.base_dir,
                diagnostics: &mut diagnostics,
                file: None,
                cell: Some(index),
                missing_asset_is_error: false,
            },
        )?;
        for mut diagnostic in diagnostics.into_vec() {
            diagnostic.span = Some(self.notebook.cells[index].span(index));
            self.diagnostics.push(diagnostic);
        }
        Ok(Some(inlines))
    }

    fn code(
        &mut self, index: usize, options: &CellOptions,
    ) -> Result<Vec<Block>> {
        if !options.show_input && !options.show_output {
            return Ok(Vec::new());
        }
        let mut rendered = outputs::render(
            &self.notebook.cells[index], index, self.source, self.config,
            options, self.assets, self.diagnostics,
        )?;
        if options.source.trim().is_empty() && rendered.outputs.is_empty() {
            return Ok(Vec::new());
        }
        let mut cell = self.code_cell(index, options);
        let mut before: Vec<_> =
            self.listing(index, options, &mut cell)?.into_iter().collect();
        if !options.show_output {
            if cell.show_input {
                before.push(Block::Cell(cell));
            }
            return Ok(before);
        }
        let plan = self.apply_captions(index, options, &mut rendered)?;
        if plan.counts.iter().sum::<usize>() == 0 {
            cell.attrs.id = options.label.clone();
        }
        cell.outputs = rendered.outputs;
        self.package_code(index, options, cell, before, &plan)
    }

    fn code_cell(&self, index: usize, options: &CellOptions) -> Cell {
        let cell = &self.notebook.cells[index];
        Cell {
            index,
            id: cell.id.clone(),
            language: self.notebook.language().into(),
            source: options.source.clone(),
            execution_count: cell.execution_count,
            show_input: options.show_input,
            show_output: options.show_output,
            outputs: Vec::new(),
            attrs: code_attrs(options),
            tags: options.tags.clone(),
            metadata: cell.metadata.clone(),
            span: Some(cell.span(index)),
        }
    }

    fn listing(
        &mut self, index: usize, options: &CellOptions, cell: &mut Cell,
    ) -> Result<Option<Block>> {
        if options.lst_label.is_none() && options.lst_cap.is_none() {
            return Ok(None);
        }
        if !cell.show_input {
            self.warn(
                "cell-options-invalid",
                index,
                "lst-label/lst-cap require a shown input",
            );
            return Ok(None);
        }
        let body = Block::CodeBlock {
            lang: (!cell.language.is_empty()).then(|| cell.language.clone()),
            text: cell.source.clone(),
            attrs: cell.attrs.clone(),
            span: cell.span.clone(),
        };
        cell.show_input = false;
        Ok(Some(Block::Figure {
            kind: FigureKind::Listing,
            caption: self.caption(options.lst_cap.as_deref(), index)?,
            body: vec![body],
            attrs: Attrs {
                id: options.lst_label.clone(),
                ..Default::default()
            },
            number: None,
            span: cell.span.clone(),
        }))
    }

    fn caption_plan(
        &mut self, index: usize, options: &CellOptions,
        outputs: &[ir::CellOutput],
    ) -> CaptionPlan {
        let kinds: Vec<_> = outputs.iter().map(captionable).collect();
        let counts = [
            count_kind(&kinds, FigureKind::Image),
            count_kind(&kinds, FigureKind::Table),
        ];
        let target = self.caption_target(index, options, &kinds, counts);
        let groups: Vec<_> = [FigureKind::Image, FigureKind::Table]
            .into_iter()
            .filter(|&kind| {
                count_kind(&kinds, kind) >= 2
                    && !matches!(
                        cap_option(options, kind),
                        Some(TextList::List(_))
                    )
                    && (cap_option(options, kind)
                        .is_some_and(TextList::is_scalar)
                        || target == Some(kind))
            })
            .collect();
        let group = self.caption_group(index, &kinds, target, &groups);
        let plan = CaptionPlan {
            kinds,
            counts,
            target,
            group,
            group_conflict: groups.len() == 2,
        };
        self.validate_captions(index, options, &plan);
        plan
    }

    fn caption_target(
        &mut self, index: usize, options: &CellOptions,
        kinds: &[Option<(FigureKind, usize)>], counts: [usize; 2],
    ) -> Option<FigureKind> {
        let mut target = options.label.as_ref().map(|label| {
            if label.starts_with("fig-") || label.starts_with("fig:") {
                FigureKind::Image
            } else if label.starts_with("tbl-") || label.starts_with("tbl:") {
                FigureKind::Table
            } else if counts[0] > 0 {
                FigureKind::Image
            } else {
                FigureKind::Table
            }
        });
        if let Some(kind) = target
            && count_kind(kinds, kind) == 0
            && counts.iter().sum::<usize>() > 0
        {
            target = Some(other_kind(kind));
            self.warn(
                "label-kind-mismatch",
                index,
                "label kind has no matching output; using the other captionable kind",
            );
        }
        target
    }

    fn caption_group(
        &mut self, index: usize, kinds: &[Option<(FigureKind, usize)>],
        target: Option<FigureKind>, groups: &[FigureKind],
    ) -> Option<FigureKind> {
        if groups.len() == 2 {
            let chosen = target.unwrap_or_else(|| {
                kinds
                    .iter()
                    .flatten()
                    .next()
                    .map(|k| k.0)
                    .unwrap_or(FigureKind::Image)
            });
            self.warn(
                "cell-options-invalid",
                index,
                format!(
                    "{}-cap ignored: a cell can have only one group",
                    kind_prefix(other_kind(chosen)),
                ),
            );
            Some(chosen)
        } else {
            groups.first().copied()
        }
    }

    fn validate_captions(
        &mut self, index: usize, options: &CellOptions, plan: &CaptionPlan,
    ) {
        for kind in [FigureKind::Image, FigureKind::Table] {
            if let Some(TextList::List(captions)) = cap_option(options, kind)
                && captions.len() > count_kind(&plan.kinds, kind)
            {
                self.warn(
                    "cell-options-invalid",
                    index,
                    format!(
                        "more {}-cap entries than outputs",
                        kind_prefix(kind)
                    ),
                );
            }
            if subcaps(options, kind).is_some() && plan.group != Some(kind) {
                self.warn(
                    "cell-option-unsupported",
                    index,
                    format!("{}-subcap outside a group", kind_prefix(kind)),
                );
            }
        }
    }

    fn apply_captions(
        &mut self, index: usize, options: &CellOptions,
        rendered: &mut outputs::RenderedOutputs,
    ) -> Result<CaptionPlan> {
        let plan = self.caption_plan(index, options, &rendered.outputs);
        let mut positions = [0, 0];
        for (output_index, output) in rendered.outputs.iter_mut().enumerate() {
            self.column_widths(output, options, index);
            let Some((kind, candidate)) = plan.kinds[output_index] else {
                continue;
            };
            let slot = usize::from(kind == FigureKind::Table);
            let panel =
                CaptionPanel { kind, candidate, position: positions[slot] };
            positions[slot] += 1;
            self.apply_panel(
                index,
                options,
                &plan,
                panel,
                output,
                rendered.html_captions.get(&output_index).map(String::as_str),
            )?;
            outputs::enforce_numbered(output);
        }
        Ok(plan)
    }

    fn panel_text(
        &mut self, index: usize, options: &CellOptions, plan: &CaptionPlan,
        panel: &CaptionPanel, captioned: bool,
    ) -> Result<PanelText> {
        let grouped = plan.group == Some(panel.kind);
        let subcaption = if grouped {
            self.caption(
                subcaps(options, panel.kind)
                    .and_then(|captions| captions.get(panel.position))
                    .map(String::as_str),
                index,
            )?
        } else {
            None
        };
        let caption = if captioned && !grouped {
            self.caption(
                cap_option(options, panel.kind)
                    .and_then(|captions| captions.at(panel.position)),
                index,
            )?
        } else {
            None
        };
        let alt = options
            .fig_alt
            .as_ref()
            .and_then(|alt| alt.at(panel.position))
            .map(|text| vec![Inline::Text { text: text.into() }])
            .or_else(|| subcaption.clone())
            .or_else(|| caption.clone())
            .unwrap_or_default();
        Ok(PanelText { caption, subcaption, alt })
    }

    fn apply_panel(
        &mut self, index: usize, options: &CellOptions, plan: &CaptionPlan,
        panel: CaptionPanel, output: &mut ir::CellOutput,
        html_caption: Option<&str>,
    ) -> Result<()> {
        let cap = cap_option(options, panel.kind);
        let ignored_group =
            plan.group_conflict && plan.group != Some(panel.kind);
        let captioned = !ignored_group
            && (cap.is_some() || plan.target == Some(panel.kind));
        let text = self.panel_text(index, options, plan, &panel, captioned)?;
        if panel.kind == FigureKind::Image {
            set_output_alt(output, &text.alt);
        }
        let span = Some(self.notebook.cells[index].span(index));
        if plan.group == Some(panel.kind) {
            let mut chosen = output.candidates.remove(panel.candidate);
            if let Some(inlines) = text.subcaption {
                insert_subcaption(&mut chosen.blocks, inlines, span);
            }
            output.candidates = vec![chosen];
        } else if captioned {
            let mut chosen = output.candidates.remove(panel.candidate);
            wrap_figure(
                &mut chosen.blocks,
                panel.kind,
                text.caption,
                figure_attrs(options, panel_label(options, plan, &panel)),
                span,
            );
            output.candidates = vec![chosen];
        } else if !ignored_group
            && panel.kind == FigureKind::Table
            && cap.is_none()
            && plan.target != Some(panel.kind)
            && let Some(caption) = html_caption
        {
            let mut chosen = output.candidates.remove(panel.candidate);
            wrap_figure(
                &mut chosen.blocks,
                panel.kind,
                Some(vec![Inline::Text { text: caption.into() }]),
                figure_attrs(options, None),
                span,
            );
            output.candidates = vec![chosen];
        }
        Ok(())
    }

    fn package_code(
        &mut self, index: usize, options: &CellOptions, mut cell: Cell,
        mut before: Vec<Block>, plan: &CaptionPlan,
    ) -> Result<Vec<Block>> {
        if let Some(kind) = plan.group {
            if cell.show_input {
                let mut input = cell.clone();
                input.outputs.clear();
                input.show_output = false;
                before.push(Block::Cell(input));
            }
            cell.show_input = false;
            let caption = self.caption(
                cap_option(options, kind).and_then(|captions| captions.at(0)),
                index,
            )?;
            let id =
                options.label.clone().filter(|_| plan.target == Some(kind));
            let span = cell.span.clone();
            before.push(Block::Figure {
                kind: FigureKind::Other,
                caption,
                body: vec![Block::Cell(cell)],
                attrs: figure_attrs(options, id),
                number: None,
                span,
            });
        } else if cell.show_input && !cell.source.trim().is_empty()
            || !cell.outputs.is_empty()
        {
            before.push(Block::Cell(cell));
        }
        Ok(before)
    }

    fn column_widths(
        &mut self, output: &mut ir::CellOutput, options: &CellOptions,
        index: usize,
    ) {
        let Some(widths) = &options.tbl_colwidths else {
            return;
        };
        visit_blocks_in_output(output, &mut |block| {
            if let Block::Table { columns, .. } = block {
                if columns.len() == widths.len() {
                    let sum: f64 = widths.iter().sum();
                    for (column, width) in columns.iter_mut().zip(widths) {
                        column.width = Some((width / sum) as f32);
                    }
                } else {
                    self.warn(
                        "cell-options-invalid",
                        index,
                        "tbl-colwidths length does not match the table",
                    );
                }
            }
        });
    }
}

struct CaptionPlan {
    kinds: Vec<Option<(FigureKind, usize)>>,
    counts: [usize; 2],
    target: Option<FigureKind>,
    group: Option<FigureKind>,
    group_conflict: bool,
}

struct CaptionPanel {
    kind: FigureKind,
    candidate: usize,
    position: usize,
}

struct PanelText {
    caption: Option<Inlines>,
    subcaption: Option<Inlines>,
    alt: Inlines,
}

fn code_attrs(options: &CellOptions) -> Attrs {
    let mut attrs =
        Attrs { classes: options.classes.clone(), ..Default::default() };
    for tag in &options.tags {
        if valid_class(tag) && !attrs.classes.contains(tag) {
            attrs.classes.push(tag.clone());
        }
    }
    if options.numbered
        && !attrs.classes.iter().any(|class| class == "numbered")
    {
        attrs.classes.push("numbered".into());
    }
    if let Some(highlight) = &options.highlight {
        attrs.pairs.insert("highlight".into(), highlight.clone());
    }
    attrs
}

fn panel_label(
    options: &CellOptions, plan: &CaptionPlan, panel: &CaptionPanel,
) -> Option<String> {
    options.label.as_ref().filter(|_| plan.target == Some(panel.kind)).map(
        |label| {
            if matches!(
                cap_option(options, panel.kind),
                Some(TextList::List(_))
            ) {
                format!("{label}-{}", panel.position + 1)
            } else {
                label.clone()
            }
        },
    )
}

fn set_output_alt(output: &mut ir::CellOutput, alt: &Inlines) {
    visit_blocks_in_output(output, &mut |block| {
        own_inlines(block, &mut |inlines| {
            inline_nodes(inlines, &mut |inline| {
                if let Inline::Image { alt: image_alt, .. } = inline {
                    *image_alt = alt.clone();
                }
            })
        });
    });
}

fn insert_subcaption(
    blocks: &mut Vec<Block>, inlines: Inlines, span: Option<SourceSpan>,
) {
    let note_at = blocks.len().min(1);
    blocks.insert(
        note_at,
        Block::Paragraph {
            inlines: vec![Inline::Span {
                inlines,
                attrs: Attrs {
                    classes: vec!["subcaption".into()],
                    ..Default::default()
                },
            }],
            span,
        },
    );
}

fn captionable(output: &ir::CellOutput) -> Option<(FigureKind, usize)> {
    if !matches!(output.kind, ir::OutputKind::Display | ir::OutputKind::Result)
    {
        return None;
    }
    output.candidates.iter().enumerate().find_map(|(index, candidate)| {
        if candidate.mime.starts_with("image/") {
            Some((FigureKind::Image, index))
        } else if matches!(candidate.blocks.first(), Some(Block::Table { .. }))
        {
            Some((FigureKind::Table, index))
        } else {
            None
        }
    })
}

fn count_kind(
    kinds: &[Option<(FigureKind, usize)>], kind: FigureKind,
) -> usize {
    kinds.iter().flatten().filter(|(found, _)| *found == kind).count()
}

fn other_kind(kind: FigureKind) -> FigureKind {
    if kind == FigureKind::Image {
        FigureKind::Table
    } else {
        FigureKind::Image
    }
}

fn kind_prefix(kind: FigureKind) -> &'static str {
    if kind == FigureKind::Image {
        "fig"
    } else {
        "tbl"
    }
}

fn cap_option(options: &CellOptions, kind: FigureKind) -> Option<&TextList> {
    if kind == FigureKind::Image {
        options.fig_cap.as_ref()
    } else {
        options.tbl_cap.as_ref()
    }
}

fn subcaps(options: &CellOptions, kind: FigureKind) -> Option<&Vec<String>> {
    if kind == FigureKind::Image {
        options.fig_subcap.as_ref()
    } else {
        options.tbl_subcap.as_ref()
    }
}

fn figure_attrs(options: &CellOptions, id: Option<String>) -> Attrs {
    let mut attrs = Attrs { id, ..Default::default() };
    for (key, value) in
        [("fig-align", &options.fig_align), ("fig-pos", &options.fig_pos)]
    {
        if let Some(value) = value {
            attrs.pairs.insert(key.into(), value.clone());
        }
    }
    attrs
}

fn wrap_figure(
    blocks: &mut Vec<Block>, kind: FigureKind, caption: Option<Inlines>,
    attrs: Attrs, span: Option<SourceSpan>,
) {
    if blocks.is_empty() {
        return;
    }
    let body = blocks.remove(0);
    blocks.insert(
        0,
        Block::Figure {
            kind,
            caption,
            body: vec![body],
            attrs,
            number: None,
            span,
        },
    );
}

fn valid_class(tag: &str) -> bool {
    tag.bytes().next().is_some_and(|c| c.is_ascii_alphabetic())
        && tag
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn visit_blocks_in_output(
    output: &mut ir::CellOutput, f: &mut dyn FnMut(&mut Block),
) {
    for candidate in &mut output.candidates {
        visit_blocks(&mut candidate.blocks, f);
    }
}

fn span_mut(block: &mut Block) -> &mut Option<SourceSpan> {
    match block {
        Block::Cell(cell) => &mut cell.span,
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
        | Block::Bibliography { span } => span,
    }
}

fn own_inlines(block: &mut Block, f: &mut dyn FnMut(&mut Inlines)) {
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph { inlines, .. } => {
            f(inlines)
        }
        Block::Figure { caption: Some(inlines), .. }
        | Block::Callout { title: Some(inlines), .. } => f(inlines),
        Block::DefinitionList { items, .. } => {
            for item in items {
                f(&mut item.term);
            }
        }
        Block::Table { head, body, foot, .. } => {
            for row in head.iter_mut().chain(body).chain(foot) {
                for cell in &mut row.cells {
                    f(&mut cell.inlines);
                }
            }
        }
        _ => {}
    }
}

fn inline_nodes(inlines: &mut [Inline], f: &mut dyn FnMut(&mut Inline)) {
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
            | Inline::Span { inlines, .. } => inline_nodes(inlines, f),
            Inline::Image { alt, .. } => inline_nodes(alt, f),
            Inline::Ref { supplement: Some(inlines), .. } => {
                inline_nodes(inlines, f)
            }
            Inline::Cite { items, .. } => {
                for item in items {
                    if let Some(prefix) = &mut item.prefix {
                        inline_nodes(prefix, f);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        inline_nodes(suffix, f);
                    }
                }
            }
            _ => {}
        }
    }
}

fn visit_blocks(blocks: &mut [Block], f: &mut dyn FnMut(&mut Block)) {
    for block in blocks {
        f(block);
        own_inlines(block, &mut |inlines| {
            inline_nodes(inlines, &mut |inline| {
                if let Inline::Footnote { blocks, .. } = inline {
                    visit_blocks(blocks, f);
                }
            })
        });
        match block {
            Block::BlockQuote { blocks, .. }
            | Block::Callout { blocks, .. }
            | Block::Div { blocks, .. } => visit_blocks(blocks, f),
            Block::Figure { body, .. } => visit_blocks(body, f),
            Block::List { items, .. } => {
                for item in items {
                    visit_blocks(&mut item.blocks, f);
                }
            }
            Block::DefinitionList { items, .. } => {
                for item in items {
                    for blocks in &mut item.definitions {
                        visit_blocks(blocks, f);
                    }
                }
            }
            Block::Cell(cell) => {
                for output in &mut cell.outputs {
                    visit_blocks_in_output(output, f);
                }
            }
            _ => {}
        }
    }
}
// Additional Converter methods; inserted by the coordinator into convert.rs.
impl Converter<'_> {
    fn markdown(&mut self, index: usize, text: &str) -> Result<Vec<Block>> {
        let directives = cell_includes(text, self.config);
        let mut cleaned = text.to_owned();
        for directive in
            directives.iter().filter(|directive| !directive.top_level)
        {
            self.diagnostics.push(
                Diagnostic::error(
                    "include-not-top-level",
                    ":::include must be at the top level of a notebook cell",
                )
                .with_span(SourceSpan {
                    start_line: directive.start,
                    end_line: directive.end,
                    cell: Some(index),
                    file: None,
                }),
            );
            cleaned = blank_range(&cleaned, directive.start, directive.end);
        }
        let text = cleaned.as_str();
        let mut result = Vec::new();
        let mut cursor = 1;
        for directive in
            directives.into_iter().filter(|directive| directive.top_level)
        {
            if cursor < directive.start {
                let fragment = cell_lines(text, cursor, directive.start - 1);
                result.extend(self.markdown_fragment(index, &fragment)?);
            }
            let span = SourceSpan {
                start_line: directive.start,
                end_line: directive.end,
                cell: Some(index),
                file: None,
            };
            let path = directive.path.map_err(|message| Error::Parse {
                path: self.source.path.clone(),
                line: None,
                message: format!(
                    "cell {index}, line {}: {message}",
                    directive.start
                ),
            })?;
            if !directive.closed {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "unclosed-directive",
                        "directive has no closing `:::` fence",
                    )
                    .with_span(span.clone()),
                );
            }
            let parsed =
                self.includes.expand(&path, &span, self.source, self.config)?;
            self.included_features.merge(&parsed.document.features);
            self.assets.merge(parsed.assets);
            self.diagnostics.extend(parsed.diagnostics);
            for included in parsed.document.includes {
                if !self.included.contains(&included) {
                    self.included.push(included);
                }
            }
            result.extend(parsed.document.blocks);
            if !directive.body.trim().is_empty() {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "invalid-directive",
                        ":::include is a leaf directive; its body follows the included content",
                    )
                    .with_span(span),
                );
                let body = "\n".repeat(directive.start) + &directive.body;
                result.extend(self.markdown_fragment(index, &body)?);
            }
            cursor = cursor.max(directive.end + 1);
        }
        if cursor <= text.lines().count() {
            let fragment = cell_lines(text, cursor, text.lines().count());
            result.extend(self.markdown_fragment(index, &fragment)?);
        }
        Ok(result)
    }

    fn markdown_fragment(
        &mut self, index: usize, text: &str,
    ) -> Result<Vec<Block>> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let (own_definitions, open) = scan(text, self.config);
        let own_names: BTreeSet<_> = own_definitions
            .iter()
            .map(|(name, _, _)| normalize_name(name))
            .collect();
        let mut origins = Vec::new();
        for (name, start, end) in own_definitions {
            let key = normalize_name(&name);
            if let Some(definition) =
                self.definitions.iter().find(|d| d.key == key)
            {
                origins.push(NoteOrigin {
                    key,
                    start,
                    end,
                    span: definition.span.clone(),
                    appended: false,
                });
            }
        }
        let mut source = text.to_owned();
        if !open {
            for definition in &self.definitions {
                if own_names.contains(&definition.key) {
                    continue;
                }
                source.push_str("\n\n");
                let start = source.bytes().filter(|&b| b == b'\n').count() + 1;
                source.push_str(&definition.text);
                origins.push(NoteOrigin {
                    key: definition.key.clone(),
                    start,
                    end: start
                        + definition.text.lines().count().saturating_sub(1),
                    span: definition.span.clone(),
                    appended: true,
                });
            }
        }
        let mut diagnostics = Diagnostics::new();
        let mut blocks = markdown::parse_blocks(
            &source,
            self.config,
            &mut Fragment {
                assets: self.assets,
                base_dir: &self.source.base_dir,
                diagnostics: &mut diagnostics,
                file: None,
                cell: Some(index),
                missing_asset_is_error: false,
            },
        )
        .map_err(|error| cell_parse_error(error, index, self.source))?;
        let mut kept = BTreeSet::new();
        let definitions = &self.definitions;
        let seen = &mut self.seen_notes;
        visit_blocks(&mut blocks, &mut |block| {
            if block.span().is_some_and(|span| span.file.is_some()) {
                return;
            }
            own_inlines(block, &mut |inlines| {
                inline_nodes(inlines, &mut |inline| {
                    let (label, is_note) = match inline {
                        Inline::Footnote { label: Some(label), .. } => {
                            (label, true)
                        }
                        Inline::FootnoteRef { label, .. } => (label, false),
                        _ => return,
                    };
                    let key = normalize_name(label);
                    if let Some(definition) =
                        definitions.iter().find(|d| d.key == key)
                    {
                        *label = definition.label.clone();
                    }
                    if is_note {
                        if seen.insert(key.clone()) {
                            kept.insert(key);
                        } else {
                            *inline = Inline::FootnoteRef {
                                label: label.clone(),
                                number: None,
                            };
                        }
                    }
                })
            });
        });
        visit_blocks(&mut blocks, &mut |block| {
            let Some(span) = block.span().cloned() else {
                return;
            };
            if span.file.is_some() {
                return;
            }
            if let Some(origin) = origins.iter().find(|origin| {
                origin.appended
                    && (origin.start..=origin.end).contains(&span.start_line)
            }) {
                *span_mut(block) = Some(origin.span.clone());
                own_inlines(block, &mut |inlines| {
                    inline_nodes(inlines, &mut |inline| {
                        if let Inline::Ref { span, .. }
                        | Inline::Cite { span, .. } = inline
                        {
                            *span = Some(origin.span.clone());
                        }
                    })
                });
            }
        });
        for mut diagnostic in diagnostics.into_vec() {
            if diagnostic.code == "footnote-unreferenced" {
                continue;
            }
            if let Some(origin) = diagnostic.span.as_ref().and_then(|span| {
                origins.iter().find(|origin| {
                    (origin.start..=origin.end).contains(&span.start_line)
                })
            }) {
                if !kept.contains(&origin.key) {
                    continue;
                }
                diagnostic.span = Some(origin.span.clone());
            }
            self.diagnostics.push(diagnostic);
        }
        let mut attachment_diags = Diagnostics::new();
        let notebook = self.notebook;
        let priority = &self.config.notebook.mime_priority;
        let assets = &mut *self.assets;
        let source = self.source;
        let definition_keys: BTreeSet<_> =
            self.definitions.iter().map(|d| d.key.clone()).collect();
        visit_blocks(&mut blocks, &mut |block| {
            let span = block.span().cloned();
            if span.as_ref().is_some_and(|span| span.file.is_some()) {
                return;
            }
            let origin =
                span.as_ref().and_then(|span| span.cell).unwrap_or(index);
            resolve_attachments(
                block, origin, notebook, source, priority, assets,
                &mut attachment_diags,
            );
            undefined_footnotes(
                block, open, &definition_keys, &mut attachment_diags,
            );
        });
        self.diagnostics.extend(attachment_diags);
        Ok(blocks)
    }

    fn raw(
        &mut self, index: usize, cell: &nbformat::Cell, text: &str,
    ) -> Result<Vec<Block>> {
        let format = raw_format(cell);
        if matches!(
            format.as_str(),
            "text/markdown" | "text/x-markdown" | "markdown"
        ) {
            return self.markdown(index, text);
        }
        let span = cell.span(index);
        let mut block = match format.as_str() {
            "text/html" | "html" => Block::Raw {
                format: "html".into(),
                text: text.into(),
                span: Some(span.clone()),
            },
            "text/latex" | "text/x-latex" | "text/tex" | "latex" => {
                Block::Raw {
                    format: "latex".into(),
                    text: text.into(),
                    span: Some(span.clone()),
                }
            }
            "text/x-typst" | "text/typst" | "application/x-typst" | "typst" => {
                Block::Raw {
                    format: "typst".into(),
                    text: text.into(),
                    span: Some(span.clone()),
                }
            }
            "openxml" => Block::Raw {
                format,
                text: text.into(),
                span: Some(span.clone()),
            },
            "text/plain" => Block::CodeBlock {
                lang: None,
                text: text.into(),
                attrs: Attrs {
                    classes: vec!["raw-cell".into()],
                    ..Attrs::default()
                },
                span: Some(span.clone()),
            },
            _ => {
                let message = if format.is_empty() {
                    "set raw_mimetype to text/html, text/latex, text/markdown or text/x-typst"
                        .into()
                } else {
                    format!("raw cell format {format:?} is not supported")
                };
                self.diagnostics.push(
                    Diagnostic::warning("raw-cell-skipped", message)
                        .with_span(span),
                );
                return Ok(Vec::new());
            }
        };
        if let Block::Raw { format, text, .. } = &mut block
            && format == "html"
        {
            markdown::walker::rewrite_html_snippet(
                text,
                Some(&span),
                &mut Fragment {
                    assets: self.assets,
                    base_dir: &self.source.base_dir,
                    diagnostics: self.diagnostics,
                    file: None,
                    cell: Some(index),
                    missing_asset_is_error: false,
                },
                &mut crate::assets::HtmlScan::default(),
            )?;
        }
        resolve_attachments(
            &mut block,
            index,
            self.notebook,
            self.source,
            &self.config.notebook.mime_priority,
            self.assets,
            self.diagnostics,
        );
        Ok(vec![block])
    }
}

struct NoteOrigin {
    key: String,
    start: usize,
    end: usize,
    span: SourceSpan,
    appended: bool,
}

struct CellInclude {
    start: usize,
    end: usize,
    top_level: bool,
    path: std::result::Result<String, String>,
    closed: bool,
    body: String,
}

fn cell_includes(text: &str, config: &Config) -> Vec<CellInclude> {
    let mut options = markdown::comrak_options(&config.markdown);
    options.extension.front_matter_delimiter = None;
    let (rewritten, _) = markdown::directives::colon_prepass(text, &options);
    let arena = Arena::new();
    let root = parse_document(&arena, &rewritten, &options);
    let mut found = Vec::new();
    let (_, lines) = markdown::recover::split_lines(text);
    for node in root.descendants() {
        let data = node.data();
        let NodeValue::BlockDirective(directive) = &data.value else {
            continue;
        };
        let info = markdown::directives::parse_directive_info(&directive.info);
        if info.name != "include" {
            continue;
        }
        if node.ancestors().skip(1).any(|ancestor| {
            let data = ancestor.data();
            matches!(&data.value, NodeValue::BlockDirective(d) if markdown::directives::parse_directive_info(&d.info).name == "include")
        }) { continue; }
        let start = data.sourcepos.start.line;
        let end = data.sourcepos.end.line;
        let closed = closing_directive(text, data.sourcepos, directive);
        let body_end = end.saturating_sub(usize::from(closed));
        let body = text
            .lines()
            .skip(start)
            .take(body_end.saturating_sub(start))
            .collect::<Vec<_>>()
            .join("\n");
        let raw = lines
            .get(start - 1)
            .and_then(|line| {
                markdown::include::raw_directive_info(
                    line, data.sourcepos.start.column,
                )
            })
            .unwrap_or(&directive.info);
        found.push(CellInclude {
            start,
            end,
            top_level: node
                .parent()
                .is_some_and(|parent| parent.same_node(root)),
            path: markdown::include::include_path(raw),
            closed,
            body,
        });
    }
    found
}

fn cell_lines(text: &str, start: usize, end: usize) -> String {
    let mut fragment = "\n".repeat(start.saturating_sub(1));
    for line in text.split_inclusive('\n').skip(start - 1).take(end - start + 1)
    {
        fragment.push_str(line);
    }
    fragment
}

fn cell_parse_error(error: Error, index: usize, source: &SourceInfo) -> Error {
    match error {
        Error::Parse { path, line, message } => Error::Parse {
            path: if path.as_os_str().is_empty() {
                source.path.clone()
            } else {
                path
            },
            line: None,
            message: format!(
                "cell {index}{}: {message}",
                line.map(|line| format!(", line {line}")).unwrap_or_default()
            ),
        },
        Error::FrontMatter(message) => {
            Error::FrontMatter(format!("cell {index}: {message}"))
        }
        other => other,
    }
}

fn undefined_footnotes(
    block: &mut Block, open: bool, definitions: &BTreeSet<String>,
    diagnostics: &mut Diagnostics,
) {
    static REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\[\^([^\]\s]+)\]").expect("fixed footnote regex")
    });
    let span = block.span().cloned();
    let mut names = BTreeSet::new();
    let mut check = |inlines: &mut Inlines| {
        let mut text = String::new();
        for inline in inlines.iter().chain(std::iter::once(&Inline::SoftBreak))
        {
            if let Inline::Text { text: part } = inline {
                text.push_str(part);
            } else {
                for capture in REFERENCE.captures_iter(&text) {
                    if open
                        || !definitions.contains(&normalize_name(&capture[1]))
                    {
                        names.insert(capture[1].to_owned());
                    }
                }
                text.clear();
            }
        }
    };
    own_inlines(block, &mut |inlines| {
        check(inlines);
        inline_nodes(inlines, &mut |inline| match inline {
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Link { inlines, .. }
            | Inline::Span { inlines, .. } => check(inlines),
            Inline::Image { alt, .. } => check(alt),
            _ => {}
        });
    });
    for name in names {
        let mut diagnostic = Diagnostic::warning(
            "footnote-undefined",
            format!("footnote {name:?} is not defined in this fragment"),
        );
        diagnostic.span = span.clone();
        diagnostics.push(diagnostic);
    }
}

fn resolve_attachments(
    block: &mut Block, index: usize, notebook: &Notebook, source: &SourceInfo,
    priority: &[String], assets: &mut AssetRegistry,
    diagnostics: &mut Diagnostics,
) {
    let span = block
        .span()
        .cloned()
        .unwrap_or_else(|| notebook.cells[index].span(index));
    let mut resolve = |url: &mut String| {
        let Some(written) = url.strip_prefix("attachment:") else {
            return;
        };
        let cell = &notebook.cells[index];
        let decoded = markdown::walker::percent_decode(written);
        let (name, bundle) = if let Some(bundle) = cell.attachments.get(written)
        {
            (written, bundle)
        } else if let Some(bundle) = cell.attachments.get(&decoded) {
            (decoded.as_str(), bundle)
        } else {
            diagnostics.push(
                Diagnostic::warning(
                    "missing-attachment",
                    format!(
                        "attachment {written:?} is not present in cell {index}"
                    ),
                )
                .with_span(span.clone()),
            );
            return;
        };
        let mut tried = false;
        for mime in priority {
            if !mime.starts_with("image/") {
                continue;
            }
            let Some(payload) = bundle.get(mime) else {
                continue;
            };
            tried = true;
            match decode_attachment(&payload.0) {
                Ok(bytes) => {
                    let stem = source
                        .path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy();
                    let sanitized = |text: &str| {
                        text.chars()
                            .map(|c| {
                                if c.is_ascii_alphanumeric()
                                    || "._-".contains(c)
                                {
                                    c
                                } else {
                                    '_'
                                }
                            })
                            .collect::<String>()
                    };
                    let hint = format!(
                        "{}_cell{index}_{}",
                        sanitized(&stem),
                        sanitized(name)
                    );
                    let id = assets.register_bytes(bytes, mime, &hint);
                    *url = AssetRegistry::placeholder(&id);
                    return;
                }
                Err(reason) => diagnostics.push(
                    Diagnostic::warning(
                        "notebook-attachment-skipped",
                        format!("attachment {name:?} ({mime}): {reason}"),
                    )
                    .with_span(span.clone()),
                ),
            }
        }
        if !tried {
            diagnostics.push(
                Diagnostic::warning(
                    "notebook-attachment-skipped",
                    format!(
                        "attachment {name:?} has no preferred image MIME type (offered: {})",
                        bundle.keys().cloned().collect::<Vec<_>>().join(", "),
                    ),
                )
                .with_span(span.clone()),
            );
        }
    };
    if let Block::Raw { format, text, .. } = block
        && format == "html"
    {
        attachment_html(text, &mut resolve);
    }
    own_inlines(block, &mut |inlines| {
        inline_nodes(inlines, &mut |inline| match inline {
            Inline::Image { url, .. } => resolve(url),
            Inline::Raw { format, text } if format == "html" => {
                attachment_html(text, &mut resolve)
            }
            _ => {}
        })
    });
}

fn attachment_html(text: &mut String, resolve: &mut dyn FnMut(&mut String)) {
    for (name, range) in crate::assets::html_attr_values(text).into_iter().rev()
    {
        if !matches!(name.as_str(), "src" | "poster") {
            continue;
        }
        let mut url = html_escape::decode_html_entities(&text[range.clone()])
            .into_owned();
        if !url.starts_with("attachment:") {
            continue;
        }
        resolve(&mut url);
        text.replace_range(range, &html_escape::encode_quoted_attribute(&url));
    }
}

fn decode_attachment(payload: &str) -> std::result::Result<Vec<u8>, String> {
    if payload.trim_start().starts_with('<') {
        return Ok(payload.as_bytes().to_vec());
    }
    let compact: Vec<_> =
        payload.bytes().filter(|byte| !byte.is_ascii_whitespace()).collect();
    if compact.is_empty() {
        return Err("empty payload".into());
    }
    STANDARD_PAD_INDIFFERENT
        .decode(compact)
        .map_err(|error| format!("invalid base64: {error}"))
}

#[cfg(test)]
mod open_block_tests {
    use super::*;

    #[test]
    fn definitions_retain_duplicate_and_unused_ranges() {
        let text = "Use[^Note] and [^note].\n\n[^Note]: first\n\n[^note]: second\n\n[^unused]: unused\n\n    continued\n";
        let (definitions, open) = scan(text, &Config::default());
        assert_eq!(
            definitions,
            [
                ("Note".into(), 3, 3),
                ("note".into(), 5, 5),
                ("unused".into(), 7, 9),
            ]
        );
        assert!(!open);
        let blanked = blank_range(text, 5, 5);
        let (definitions, _) = scan(&blanked, &Config::default());
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0].0, "Note");
        assert_eq!(normalize_name("Straße"), normalize_name("STRASSE"));
    }

    #[test]
    fn inline_notes_do_not_hide_open_blocks() {
        for ending in ["```python\nopen\n", ":::note\nopen\n", "<!-- open\n"] {
            let text = format!("Inline ^[local note].\n\n{ending}");
            assert!(scan(&text, &Config::default()).1, "{text}");
        }
        assert!(!scan("Inline ^[local note].", &Config::default()).1);
    }

    #[test]
    fn directive_closer_respects_indentation_and_fence_length() {
        for text in [
            ":::note\nopen\n    :::\n", "::::note\nopen\n:::\n",
            ":::note\nopen\n::: trailing\n",
        ] {
            assert!(scan(text, &Config::default()).1, "{text}");
        }
        for text in [
            ":::note\nbody\n :::\n",
            "::::note\nbody\n :::::\n",
            "- item\n\n  :::note\n  body\n  :::\n",
            "- item\n\n\t:::note\n\tbody\n\t:::\n",
            "[^n]: body\n\n    :::note\n    body\n    :::\n",
        ] {
            assert!(!scan(text, &Config::default()).1, "{text}");
        }
    }

    #[test]
    fn nested_open_blocks_and_closed_outer_directives() {
        for text in [
            "- item\n\n  ```python\n  open\n",
            "[^n]: body\n\n    ```python\n    open\n",
            ":::note\n```python\nopen\n",
        ] {
            assert!(scan(text, &Config::default()).1, "{text}");
        }
        assert!(!scan(":::note\n```python\nopen\n:::\n", &Config::default()).1);
    }

    #[test]
    fn notebook_include_info_preserves_windows_path_bytes() {
        for (bom, path) in [
            (
                "",
                r"\\?\C:\Users\runneradmin\AppData\Local\Temp\.tmp123\part.md",
            ),
            ("\u{feff}", r"folder\.tmp123\part.md"),
        ] {
            let text = format!("{bom}:::include \"{path}\"\n:::\n");
            let includes = cell_includes(&text, &Config::default());
            assert_eq!(includes.len(), 1, "BOM {bom:?}");
            assert_eq!(includes[0].path.as_deref(), Ok(path), "BOM {bom:?}");
        }
    }
}
