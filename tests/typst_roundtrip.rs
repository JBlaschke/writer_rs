//! Exact Markdown -> resolved IR -> Typst -> resolved IR corpus contract.
//!
//! Normalization is typed and limited to typst-dialect.md §7. The sole extra
//! coordinator amendment is notebook provenance -> surviving Cell presence.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use writer::assets::registry::AssetRegistry;
use writer::config::{Config, HeadingNumbering};
use writer::ir::*;
use writer::markdown::code::CodeAttrs;
use writer::math::latex_source;
use writer::resolve::{ResolveOptions, resolve};
use writer::typst::emit::{AssetPaths, EmitOptions, emit};

const EXCLUDED: &[&str] = &["presenter-deck.md", "includes.md", "raw.md"];

fn fixtures() -> Vec<PathBuf> {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md");
    let mut files: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "md")
        })
        .collect();
    files.sort();
    assert_eq!(files.len(), 18, "review additions to the top-level corpus");
    for excluded in EXCLUDED {
        assert!(
            files.iter().any(|path| path.file_name().unwrap() == *excluded)
        );
    }
    files.retain(|path| {
        !EXCLUDED.contains(&path.file_name().unwrap().to_str().unwrap())
    });
    assert_eq!(files.len(), 15);
    files
}

fn resolve_side(document: &mut Document, config: &mut Config) {
    config.merge_front_matter(&document.meta).unwrap();
    // These findings are not part of the compared IR. The intentionally
    // recoverable fixtures still exercise exactly the resolver's real policy.
    let _ = resolve(
        document,
        &ResolveOptions { config, library: None, lenient: true },
    );
}

#[derive(Serialize)]
struct Compared<'a> {
    meta: &'a DocMeta,
    blocks: &'a [Block],
    features: &'a Features,
    resolved: &'a Resolved,
}

fn compared(document: &Document) -> Value {
    serde_json::to_value(Compared {
        meta: &document.meta,
        blocks: &document.blocks,
        features: &document.features,
        resolved: &document.resolved,
    })
    .unwrap()
}

struct Normalizer<'a> {
    config: &'a Config,
    bibliography: bool,
}

impl Normalizer<'_> {
    fn document(&self, document: &mut Document) {
        document.meta.vars.clear();
        if document.meta.lang.is_none() {
            document.meta.lang = Some(self.config.document.lang.clone());
        }
        if let Some(blocks) = &mut document.meta.abstract_ {
            *blocks = self.blocks(std::mem::take(blocks));
        }
        document.blocks = self.blocks(std::mem::take(&mut document.blocks));
        document
            .resolved
            .labels
            .retain(|_, info| info.kind != LabelKind::Footnote);
        for info in document.resolved.labels.values_mut() {
            info.span = None;
        }
        for entry in &mut document.resolved.toc {
            entry.inlines =
                self.inlines(std::mem::take(&mut entry.inlines), true);
        }
        if let Some(bibliography) = &mut document.resolved.bibliography {
            for entry in &mut bibliography.entries {
                if let Some(prefix) = &mut entry.prefix {
                    *prefix = self.inlines(std::mem::take(prefix), false);
                }
                entry.inlines =
                    self.inlines(std::mem::take(&mut entry.inlines), false);
            }
        }
        // Approved §7 refinement: this one bit can otherwise retain an
        // included notebook's provenance even when it contains no Cell.
        let mut cells = false;
        writer::ir::walk_blocks(document, &mut |block| {
            cells |= matches!(block, Block::Cell(_));
        });
        document.features.notebook = cells;
    }

    fn attrs(&self, attrs: &mut Attrs, numbering: bool) {
        attrs.classes.retain(|class| {
            numbering
                && matches!(class.as_str(), "unnumbered" | "notoc" | "unlisted")
        });
        for class in &mut attrs.classes {
            if class == "unlisted" {
                *class = "notoc".into();
            }
        }
        attrs.pairs.clear();
    }

    fn code_attrs(&self, attrs: &mut Attrs) {
        let code = CodeAttrs::from_attrs(attrs);
        self.attrs(attrs, false);
        if code.numbered {
            attrs.classes.push("numbered".into());
        }
        if code.plain {
            attrs.classes.push("plain".into());
        }
        attrs.pairs.insert("start".into(), code.start.to_string());
        if !code.highlight.is_empty() {
            let ranges = code
                .highlight
                .iter()
                .map(|range| {
                    if range.start() == range.end() {
                        range.start().to_string()
                    } else {
                        format!("{}-{}", range.start(), range.end())
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            attrs.pairs.insert("highlight".into(), ranges);
        }
    }

    fn blocks(&self, blocks: Vec<Block>) -> Vec<Block> {
        let mut output = Vec::new();
        for mut block in blocks {
            match &mut block {
                Block::Div { blocks, .. } => {
                    output.extend(self.blocks(std::mem::take(blocks)));
                    continue;
                }
                Block::Raw { format, .. } if format != "typst" => continue,
                Block::Bibliography { .. } if !self.bibliography => continue,
                Block::Heading { inlines, attrs, span, .. } => {
                    *span = None;
                    *inlines = self.inlines(std::mem::take(inlines), true);
                    self.attrs(attrs, true);
                    if matches!(
                        &self.config.numbering.headings,
                        HeadingNumbering::Off
                    ) {
                        attrs.classes.retain(|class| class != "unnumbered");
                    }
                }
                Block::Paragraph { inlines, span } => {
                    *span = None;
                    *inlines = self.inlines(std::mem::take(inlines), false);
                }
                Block::BlockQuote { blocks, span } => {
                    *span = None;
                    *blocks = self.blocks(std::mem::take(blocks));
                }
                Block::List { items, span, .. } => {
                    *span = None;
                    for item in items {
                        item.blocks =
                            self.blocks(std::mem::take(&mut item.blocks));
                        if let Some(checked) = item.checked.take() {
                            let mark = Inline::text(if checked {
                                "☒ "
                            } else {
                                "☐ "
                            });
                            if let Some(Block::Paragraph { inlines, .. }) =
                                item.blocks.first_mut()
                            {
                                inlines.insert(0, mark);
                                *inlines = merge_text(std::mem::take(inlines));
                            } else {
                                item.blocks
                                    .insert(0, Block::paragraph(vec![mark]));
                            }
                        }
                    }
                }
                Block::DefinitionList { items, span } => {
                    *span = None;
                    for item in items {
                        item.term =
                            self.inlines(std::mem::take(&mut item.term), true);
                        let definitions = std::mem::take(&mut item.definitions)
                            .into_iter()
                            .flatten()
                            .collect();
                        item.definitions = vec![self.blocks(definitions)];
                    }
                }
                Block::CodeBlock { attrs, span, .. } => {
                    *span = None;
                    self.code_attrs(attrs);
                }
                Block::Figure { caption, body, attrs, span, .. } => {
                    *span = None;
                    if let Some(caption) = caption {
                        *caption = self.inlines(std::mem::take(caption), false);
                    }
                    *body = self.blocks(std::mem::take(body));
                    self.attrs(attrs, true);
                }
                Block::Table { attrs, head, body, foot, span, .. } => {
                    *span = None;
                    self.attrs(attrs, false);
                    for (rows, body_cells) in
                        [(head, false), (body, true), (foot, false)]
                    {
                        for row in rows {
                            for cell in &mut row.cells {
                                cell.inlines = self.inlines(
                                    std::mem::take(&mut cell.inlines),
                                    false,
                                );
                                if body_cells {
                                    cell.header = false;
                                }
                            }
                        }
                    }
                }
                Block::Math { source, dialect, attrs, span, .. } => {
                    *span = None;
                    if *dialect == MathDialect::Latex {
                        *source = latex_source::prepare(source).0;
                    }
                    self.attrs(attrs, true);
                }
                Block::Callout { title, blocks, attrs, span, .. } => {
                    *span = None;
                    if let Some(title) = title {
                        *title = self.inlines(std::mem::take(title), false);
                    }
                    *blocks = self.blocks(std::mem::take(blocks));
                    self.attrs(attrs, false);
                }
                Block::Cell(cell) => {
                    cell.span = None;
                    self.attrs(&mut cell.attrs, false);
                    for output in &mut cell.outputs {
                        for candidate in &mut output.candidates {
                            candidate.blocks = self
                                .blocks(std::mem::take(&mut candidate.blocks));
                        }
                    }
                }
                Block::Raw { span, .. }
                | Block::ThematicBreak { span }
                | Block::PageBreak { span }
                | Block::Toc { span, .. }
                | Block::Bibliography { span } => *span = None,
            }
            output.push(block);
        }
        output
    }

    fn inlines(&self, inlines: Inlines, space_breaks: bool) -> Inlines {
        let mut output = Vec::new();
        for mut inline in inlines {
            match &mut inline {
                Inline::Span { inlines, .. } => {
                    output.extend(
                        self.inlines(std::mem::take(inlines), space_breaks),
                    );
                    continue;
                }
                Inline::Raw { format, .. } if format != "typst" => continue,
                Inline::SoftBreak if space_breaks => inline = Inline::text(" "),
                Inline::Emph { inlines }
                | Inline::Strong { inlines }
                | Inline::Strikeout { inlines }
                | Inline::Subscript { inlines }
                | Inline::Superscript { inlines }
                | Inline::Underline { inlines }
                | Inline::SmallCaps { inlines } => {
                    *inlines =
                        self.inlines(std::mem::take(inlines), space_breaks);
                }
                Inline::Code { attrs, .. } => self.attrs(attrs, false),
                Inline::Math { source, dialect } => {
                    if *dialect == MathDialect::Latex {
                        *source = latex_source::prepare(source).0;
                    }
                }
                Inline::Link { inlines, title, attrs, .. } => {
                    *inlines =
                        self.inlines(std::mem::take(inlines), space_breaks);
                    *title = None;
                    self.attrs(attrs, false);
                }
                Inline::Image { alt, title, attrs, .. } => {
                    let text = writer::ir::plain_text(alt);
                    *alt = if text.is_empty() {
                        Vec::new()
                    } else {
                        vec![Inline::text(&text)]
                    };
                    *title = None;
                    let dimensions: Vec<_> = ["width", "height"]
                        .into_iter()
                        .filter_map(|name| {
                            attrs.pairs.get(name).map(|value| {
                                (name.to_owned(), image_dimension(value))
                            })
                        })
                        .collect();
                    self.attrs(attrs, false);
                    attrs.pairs.extend(dimensions);
                }
                Inline::Footnote { blocks, .. } => {
                    *blocks = self.blocks(std::mem::take(blocks));
                }
                Inline::Ref { target, supplement, form, resolved, span } => {
                    *span = None;
                    if let Some(supplement) = supplement {
                        *supplement = self
                            .inlines(std::mem::take(supplement), space_breaks);
                    }
                    if let Some(resolved) = resolved {
                        resolved.text = self.inlines(
                            std::mem::take(&mut resolved.text),
                            space_breaks,
                        );
                        if *form == RefForm::Normal
                            && resolved.number.is_empty()
                        {
                            inline = Inline::Link {
                                url: format!("#{target}"),
                                inlines: resolved.text.clone(),
                                title: None,
                                attrs: Attrs::default(),
                            };
                        }
                    }
                }
                Inline::Cite { items, mode, resolved, .. } => {
                    // This preserves Cite itself. There is no extra rule
                    // replacing unresolved Cite by a placeholder.
                    if let Some(resolved) = resolved {
                        resolved.inlines = self.inlines(
                            std::mem::take(&mut resolved.inlines),
                            space_breaks,
                        );
                    }
                    output.extend(self.citation(
                        std::mem::take(items),
                        *mode,
                        resolved.clone(),
                        space_breaks,
                    ));
                    continue;
                }
                _ => {}
            }
            output.push(inline);
        }
        merge_text(output)
    }

    fn citation(
        &self, mut items: Vec<CiteItem>, mode: CiteMode,
        resolved: Option<ResolvedCite>, space_breaks: bool,
    ) -> Inlines {
        let mut groups = Vec::new();
        let mut start = 0;
        for (index, item) in items.iter().enumerate() {
            if index > start && item.prefix.is_some() {
                groups.push(start..index);
                start = index;
            }
            if item.suffix.is_some() {
                groups.push(start..index + 1);
                start = index + 1;
            }
        }
        if start < items.len() {
            groups.push(start..items.len());
        }
        assert!(
            resolved.is_none() || groups.len() <= 1,
            "library=None must not produce a rendered multi-group citation cache"
        );
        let mut output = Vec::new();
        for (index, group) in groups.iter().enumerate() {
            if index > 0 {
                output.push(Inline::text(" "));
            }
            if let Some(prefix) = items[group.start].prefix.take() {
                output.extend(self.inlines(prefix, space_breaks));
                output.push(Inline::text(" "));
            }
            let suffix = items[group.end - 1].suffix.take();
            let mut members = items[group.clone()].to_vec();
            for item in &mut members {
                item.prefix = None;
                item.suffix = None;
            }
            output.push(Inline::Cite {
                items: members,
                mode,
                span: None,
                resolved: resolved.clone(),
            });
            if let Some(suffix) = suffix {
                output.push(Inline::text(" "));
                output.extend(self.inlines(suffix, space_breaks));
            }
        }
        output
    }
}

fn image_dimension(source: &str) -> String {
    let number = source.strip_suffix("px").unwrap_or(source);
    match number.trim().parse::<f64>() {
        Ok(number) => {
            let value = format!("{:.6}", number * 0.75);
            format!("{}pt", value.trim_end_matches('0').trim_end_matches('.'))
        }
        Err(_) => source.to_owned(),
    }
}

fn merge_text(inlines: Inlines) -> Inlines {
    let mut merged = Vec::new();
    for inline in inlines {
        if let Inline::Text { text } = &inline
            && let Some(Inline::Text { text: previous }) = merged.last_mut()
        {
            previous.push_str(text);
        } else {
            merged.push(inline);
        }
    }
    merged
}

fn normalize(document: &mut Document, config: &Config) {
    Normalizer {
        config,
        bibliography: document.resolved.bibliography.is_some(),
    }
    .document(document);
}

/// Snapshot-only redaction recognizes registered Image/Link URL fields. It
/// never rewrites arbitrary code, math, prose, metadata or unknown URL IDs.
fn stable_snapshot(mut value: Value, assets: &AssetRegistry) -> Value {
    let registered: BTreeSet<_> =
        assets.iter().map(|asset| asset.id.clone()).collect();
    let mut aliases = BTreeMap::new();
    fn visit(
        value: &mut Value, registered: &BTreeSet<String>,
        aliases: &mut BTreeMap<String, String>,
    ) {
        match value {
            Value::Object(object) => {
                let typed_url = object
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| matches!(kind, "image" | "link"));
                if typed_url
                    && let Some(Value::String(url)) = object.get_mut("url")
                    && let Some(suffix) = url.strip_prefix("asset:")
                {
                    let end = suffix.find(['?', '#']).unwrap_or(suffix.len());
                    let id = &suffix[..end];
                    if registered.contains(id) {
                        let next = format!("{:012x}", aliases.len() + 1);
                        let alias =
                            aliases.entry(id.to_owned()).or_insert(next);
                        *url = format!("asset:{alias}{}", &suffix[end..]);
                    }
                }
                for child in object.values_mut() {
                    visit(child, registered, aliases);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, registered, aliases);
                }
            }
            _ => {}
        }
    }
    visit(&mut value, &registered, &mut aliases);
    value
}

fn differences(
    expected: &Value, actual: &Value, path: &str, found: &mut Vec<String>,
) {
    if expected == actual || found.len() >= 24 {
        return;
    }
    match (expected, actual) {
        (Value::Object(left), Value::Object(right)) => {
            let keys: BTreeSet<_> = left.keys().chain(right.keys()).collect();
            for key in keys {
                differences(
                    left.get(key).unwrap_or(&Value::Null),
                    right.get(key).unwrap_or(&Value::Null),
                    &format!("{path}.{key}"),
                    found,
                );
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                found.push(format!(
                    "{path}: array lengths {} != {}",
                    left.len(),
                    right.len()
                ));
            }
            for (index, (left, right)) in left.iter().zip(right).enumerate() {
                differences(left, right, &format!("{path}[{index}]"), found);
            }
        }
        _ => found.push(format!("{path}: {expected} != {actual}")),
    }
}

#[test]
fn fifteen_markdown_fixtures_round_trip_with_only_the_documented_losses() {
    let mut mismatches = Vec::new();
    for path in fixtures() {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let source = SourceInfo {
            path: path.clone(),
            base_dir: path.parent().unwrap().into(),
            kind: SourceKind::Markdown,
        };
        let initial = Config::default();
        let mut left = writer::markdown::parse(
            &source,
            &fs::read_to_string(&path).unwrap(),
            &initial,
        )
        .unwrap();
        let mut left_config = initial.clone();
        resolve_side(&mut left.document, &mut left_config);
        let bibliography: Vec<_> = left_config
            .references
            .bibliography
            .iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect();
        let emitted = emit(
            &left.document,
            &left.assets,
            &EmitOptions {
                config: &left_config,
                assets: AssetPaths::Source,
                template: None,
                bibliography,
                style: left_config.references.style.clone(),
                footnote_prefix: String::new(),
                fragment: false,
            },
        )
        .unwrap();
        let generated = SourceInfo {
            path: source.base_dir.join("__roundtrip.generated.typ"),
            kind: SourceKind::Typst,
            base_dir: source.base_dir.clone(),
        };
        // Reparse with the effective options that supplied the emitted macro
        // prefix; the generated show rule does not serialize arbitrary config.
        let mut right = writer::typst::frontend::parse(
            &generated, &emitted.text, &left_config,
        )
        .unwrap();
        let mut right_config = left_config.clone();
        resolve_side(&mut right.document, &mut right_config);
        normalize(&mut left.document, &left_config);
        normalize(&mut right.document, &right_config);
        let expected = compared(&left.document);
        let actual = compared(&right.document);
        if expected != actual {
            let mut findings = Vec::new();
            differences(&expected, &actual, "$", &mut findings);
            mismatches.push(format!("{name}:\n{}", findings.join("\n")));
            continue;
        }
        insta::assert_json_snapshot!(
            name,
            stable_snapshot(actual, &right.assets)
        );
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n\n"));
}

fn hand_document(blocks: Vec<Block>) -> Document {
    Document {
        meta: DocMeta::default(),
        blocks,
        features: Features::default(),
        source: SourceInfo {
            path: "/roundtrip.md".into(),
            base_dir: "/".into(),
            kind: SourceKind::Markdown,
        },
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Resolved::default(),
    }
}

#[test]
fn snapshot_asset_redaction_does_not_rewrite_source_bytes_or_unregistered_urls()
{
    let mut assets = AssetRegistry::new();
    let id = assets.register_bytes(vec![1, 2, 3], "image/png", "sample.png");
    let source = format!("asset:{id} ?# `quotes` \\n");
    let unknown = "asset:ffffffffffff";
    let value = serde_json::json!({"blocks":[{"type":"paragraph","inlines":[
        {"type":"image","url":format!("asset:{id}?v=2#panel")},
        {"type":"link","url":format!("asset:{id}")},
        {"type":"math","source":source,"dialect":"typst"},
        {"type":"code","text":source},
        {"type":"text","text":source},
        {"type":"raw","format":"typst","text":source},
        {"type":"image","url":unknown}
    ]}]});
    let redacted = stable_snapshot(value.clone(), &assets);
    assert_eq!(
        redacted["blocks"][0]["inlines"][0]["url"],
        "asset:000000000001?v=2#panel"
    );
    assert_eq!(
        redacted["blocks"][0]["inlines"][1]["url"],
        "asset:000000000001"
    );
    for index in 2..=5 {
        assert_eq!(
            redacted["blocks"][0]["inlines"][index],
            value["blocks"][0]["inlines"][index]
        );
    }
    assert_eq!(redacted["blocks"][0]["inlines"][6]["url"], unknown);
}

#[test]
fn only_the_notebook_feature_is_canonicalized_from_surviving_cells() {
    let config = Config::default();
    let mut document = hand_document(Vec::new());
    document.features = Features {
        math: true,
        code: true,
        notebook: true,
        citations: true,
        footnotes: true,
    };
    normalize(&mut document, &config);
    assert_eq!(
        document.features,
        Features {
            math: true,
            code: true,
            notebook: false,
            citations: true,
            footnotes: true,
        }
    );
    document.blocks.push(Block::Cell(Cell::default()));
    normalize(&mut document, &config);
    assert!(document.features.notebook);
}

#[test]
fn normalization_keeps_source_whitespace_native_math_and_inline_code_identity()
{
    let native = "#text[keep `bytes` and  spaces] + x";
    let mut document = hand_document(vec![Block::Paragraph {
        inlines: vec![
            Inline::text("Keep  two spaces and trailing "),
            Inline::Math { source: native.into(), dialect: MathDialect::Typst },
            Inline::Code {
                text: "literal  code".into(),
                attrs: Attrs {
                    id: Some("code-id".into()),
                    pairs: [("highlight".into(), "1-3|5".into())]
                        .into_iter()
                        .collect(),
                    ..Attrs::default()
                },
            },
        ],
        span: Some(SourceSpan {
            start_line: 2,
            end_line: 2,
            cell: None,
            file: None,
        }),
    }]);
    normalize(&mut document, &Config::default());
    let Block::Paragraph { inlines, span } = &document.blocks[0] else {
        panic!()
    };
    assert!(span.is_none());
    assert_eq!(inlines[0], Inline::text("Keep  two spaces and trailing "));
    assert_eq!(
        inlines[1],
        Inline::Math { source: native.into(), dialect: MathDialect::Typst }
    );
    let Inline::Code { text, attrs } = &inlines[2] else {
        panic!()
    };
    assert_eq!(text, "literal  code");
    assert_eq!(attrs.id.as_deref(), Some("code-id"));
    assert!(attrs.pairs.is_empty());
}

#[test]
fn code_block_line_options_remain_canonical_and_inline_controls_do_not_apply() {
    let mut document = hand_document(vec![Block::CodeBlock {
        lang: Some("rust".into()),
        text: "let  value = 1;\n\n".into(),
        attrs: Attrs {
            id: Some("block-code".into()),
            classes: vec!["numbered".into(), "plain".into()],
            pairs: [
                ("start".into(), "7".into()),
                ("highlight".into(), "1-3|5".into()),
            ]
            .into_iter()
            .collect(),
        },
        span: None,
    }]);
    normalize(&mut document, &Config::default());
    let Block::CodeBlock { lang, text, attrs, .. } = &document.blocks[0] else {
        panic!("expected a code block");
    };
    assert_eq!(lang.as_deref(), Some("rust"));
    assert_eq!(text, "let  value = 1;\n\n");
    assert_eq!(attrs.id.as_deref(), Some("block-code"));
    assert_eq!(attrs.classes, ["numbered", "plain"]);
    assert_eq!(attrs.pairs["start"], "7");
    assert_eq!(attrs.pairs["highlight"], "1-3,5");
}
