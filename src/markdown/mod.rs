//! Markdown frontend and IR conversion (markdown-dialect.md §1–§2; ir.md §4).
//! Owner: T01 → T12. Module skeleton created by T00.

pub mod attrs;
pub mod code;
pub mod directives;
pub mod frontmatter;
pub mod include;
pub mod options;
pub mod recover;
pub mod refs;
pub mod walker;

#[allow(unused_imports)] // T11
pub use options::*;

use std::path::Path;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

use crate::assets::registry::AssetRegistry;
use crate::config::{Config, Toc};
use crate::error::{Diagnostic, Diagnostics, Result};
use crate::ir::{
    self, Block, DocMeta, Document, Features, Inline, Inlines, Parsed,
    SourceInfo, SourceKind, SourceSpan,
};
use directives::{
    DIRECTIVE_COLON_REWRITTEN, check_unopened_directives, colon_prepass,
};
use frontmatter::{parse_document_front_matter, strip_delimiters};
use recover::recover_attr_blocks;

/// Shared resources and source context for a Markdown fragment.
///
/// Notebook cells, titles, captions and abstracts use the document dialect
/// without front matter or include expansion. Lines are relative to the
/// fragment; callers parsing a title or caption remap its diagnostics.
pub struct Fragment<'a> {
    /// Local and generated assets registered by this fragment.
    pub assets: &'a mut AssetRegistry,
    /// Directory against which relative asset URLs resolve.
    pub base_dir: &'a Path,
    /// Findings accumulated while parsing.
    pub diagnostics: &'a mut Diagnostics,
    /// Included-file identity, if the fragment belongs to another file.
    pub file: Option<&'a Path>,
    /// Notebook cell index, if applicable.
    pub cell: Option<usize>,
    /// Return `MissingAsset` instead of a warning for a missing local image.
    pub missing_asset_is_error: bool,
}

/// Parses a Markdown document into unresolved IR and registered assets.
///
/// Front matter is applied to a clone of `config`. Missing local assets are
/// warnings; malformed front matter or unexpected parser nodes return errors.
/// Labels, numbering, references and includes are resolved by later passes.
pub fn parse(
    source: &SourceInfo, text: &str, config: &Config,
) -> Result<Parsed> {
    let _listings = walker::DirListings::enter();
    let arena = Arena::new();
    let empty_front = starts_with_empty_front_matter(text);
    let mut options = document_options(config, empty_front);
    let first = parse_document(&arena, text, &options);
    let mut diagnostics = Diagnostics::new();
    let mut meta = document_meta(first, &mut diagnostics)?;
    let abstract_text = meta.vars.remove("abstract").and_then(|value| {
        if let serde_json::Value::String(text) = value {
            Some(text)
        } else {
            None
        }
    });
    let mut effective = config.clone();
    effective.merge_front_matter(&meta)?;
    options = document_options(&effective, empty_front);
    let mut assets = AssetRegistry::new();
    let mut ctx = Fragment {
        assets: &mut assets,
        base_dir: &source.base_dir,
        diagnostics: &mut diagnostics,
        file: None,
        cell: None,
        missing_asset_is_error: false,
    };
    if let Some(abstract_text) = abstract_text.filter(|text| !text.is_empty()) {
        meta.abstract_ =
            Some(parse_blocks(&abstract_text, &effective, &mut ctx)?);
    }
    let root = prepared_ast(&arena, text, &options, &mut ctx);
    if empty_front {
        mark_empty_front_matter(root);
    }
    let blocks = walker::walk(root, text, &effective, &mut ctx, true)?;
    // T1B: reference scanner (§7.2) runs here.
    let mut document = Document {
        meta,
        blocks,
        features: Features::default(),
        source: source.clone(),
        assets: assets.infos(&source.base_dir),
        includes: Vec::new(),
        resolved: Default::default(),
    };
    insert_toc(&mut document, &effective);
    document.features = collect_features(&document);
    Ok(Parsed {
        document,
        assets,
        diagnostics: finish_diagnostics(diagnostics, &source.path),
    })
}

/// Parses blocks with the same walker as [`parse`], without front matter.
/// The caller supplies the effective configuration and source context.
pub fn parse_blocks(
    text: &str, config: &Config, ctx: &mut Fragment,
) -> Result<Vec<Block>> {
    let _listings = walker::DirListings::enter();
    let mut options = comrak_options(&config.markdown);
    options.extension.front_matter_delimiter = None;
    let arena = Arena::new();
    let root = prepared_ast(&arena, text, &options, ctx);
    let blocks = walker::walk(root, text, config, ctx, false)?;
    // T1B: reference scanner (§7.2) runs here.
    Ok(blocks)
}

/// Parses one paragraph's inlines. For other block shapes, returns the first
/// paragraph's inlines and reports `fragment-not-inline`. Empty text is silent.
pub fn parse_inlines(
    text: &str, config: &Config, ctx: &mut Fragment,
) -> Result<Inlines> {
    let blocks = parse_blocks(text, config, ctx)?;
    let single = matches!(blocks.as_slice(), [Block::Paragraph { .. }]);
    if !single && !text.trim().is_empty() {
        let span =
            blocks.first().and_then(Block::span).cloned().unwrap_or_else(
                || SourceSpan {
                    start_line: 1,
                    end_line: source_lines(text).len().max(1),
                    cell: ctx.cell,
                    file: ctx.file.map(Path::to_path_buf),
                },
            );
        let mut finding = Diagnostic::warning(
            "fragment-not-inline",
            "expected one paragraph of inline content",
        )
        .with_span(span);
        finding.path = ctx.file.map(Path::to_path_buf);
        ctx.diagnostics.push(finding);
    }
    Ok(blocks
        .into_iter()
        .find_map(|block| {
            if let Block::Paragraph { inlines, .. } = block {
                Some(inlines)
            } else {
                None
            }
        })
        .unwrap_or_default())
}

fn document_options(config: &Config, empty_front: bool) -> Options<'static> {
    let mut options = comrak_options(&config.markdown);
    if empty_front {
        options.extension.front_matter_delimiter = None;
    }
    options
}

fn document_meta<'a>(
    root: &'a AstNode<'a>, diagnostics: &mut Diagnostics,
) -> Result<DocMeta> {
    let Some(first) = root.first_child() else {
        return Ok(DocMeta::default());
    };
    let data = first.data();
    match &data.value {
        NodeValue::FrontMatter(literal) => parse_document_front_matter(
            strip_delimiters(literal),
            data.sourcepos.end.line,
            diagnostics,
        ),
        _ => Ok(DocMeta::default()),
    }
}

fn prepared_ast<'a>(
    arena: &'a Arena<'a>, text: &str, options: &Options, ctx: &mut Fragment,
) -> &'a AstNode<'a> {
    let (rewritten, lines) = colon_prepass(text, options);
    let mut diagnostics = Diagnostics::new();
    for line in lines {
        diagnostics.push(Diagnostic::info(
            DIRECTIVE_COLON_REWRITTEN,
            "rewrote colons or closing colons on a directive line for comrak",
        ).with_span(SourceSpan {
            start_line: line, end_line: line, cell: None, file: None,
        }));
    }
    let root = parse_document(arena, &rewritten, options);
    let (root, _) =
        recover_attr_blocks(arena, root, &rewritten, options, &mut diagnostics);
    check_unopened_directives(root, &mut diagnostics);
    for mut finding in diagnostics.into_vec() {
        finding.path = ctx.file.map(Path::to_path_buf);
        if let Some(span) = &mut finding.span {
            span.file = ctx.file.map(Path::to_path_buf);
            span.cell = ctx.cell;
        }
        ctx.diagnostics.push(finding);
    }
    root
}

fn insert_toc(document: &mut Document, config: &Config) {
    let Toc::Depth(depth) = config.document.toc else {
        return;
    };
    let mut exists = false;
    ir::walk_blocks(document, &mut |block| {
        exists |= matches!(block, Block::Toc { .. });
    });
    if !exists {
        document.blocks.insert(0, Block::Toc { depth, span: None });
    }
}

pub(crate) fn collect_features(document: &Document) -> Features {
    let mut features = Features {
        notebook: document.source.kind == SourceKind::Notebook,
        ..Features::default()
    };
    ir::walk_blocks(document, &mut |block| match block {
        Block::Math { .. } => features.math = true,
        Block::CodeBlock { .. } => features.code = true,
        Block::Cell(_) => {
            features.code = true;
            features.notebook = true;
        }
        _ => {}
    });
    ir::walk_inlines(document, &mut |inline| match inline {
        Inline::Math { .. } => features.math = true,
        Inline::Footnote { .. } | Inline::FootnoteRef { .. } => {
            features.footnotes = true;
        }
        Inline::Cite { .. } => features.citations = true,
        _ => {}
    });
    features
}

/// Lines numbered as comrak does: LF, CRLF or CR, ignoring a leading BOM.
fn source_lines(source: &str) -> Vec<&str> {
    let mut rest = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut lines = Vec::new();
    while let Some(end) = rest.find(['\n', '\r']) {
        lines.push(&rest[..end]);
        let ending = if rest[end..].starts_with("\r\n") {
            2
        } else {
            1
        };
        rest = &rest[end + ending..];
    }
    lines.push(rest);
    lines
}

/// Empty front matter must not pair its first rule with a later thematic break.
fn starts_with_empty_front_matter(text: &str) -> bool {
    let lines = source_lines(text);
    lines.len() >= 2 && lines[0] == "---" && lines[1] == "---"
}

/// Replace the first two rules with comrak's empty front-matter representation.
fn mark_empty_front_matter<'a>(root: &'a AstNode<'a>) {
    let breaks: Vec<_> = root
        .children()
        .take(2)
        .filter(|node| {
            let data = node.data();
            matches!(data.value, NodeValue::ThematicBreak)
                && data.sourcepos.start.line == data.sourcepos.end.line
                && data.sourcepos.start.line <= 2
        })
        .collect();
    if let [first, second] = breaks.as_slice() {
        let mut data = first.data_mut();
        data.value = NodeValue::FrontMatter("---\n---\n".to_string());
        data.sourcepos.end = second.data().sourcepos.end;
        second.detach();
    }
}

/// Missing paths use the document path; stable ordering preserves same-line order.
fn finish_diagnostics(diagnostics: Diagnostics, path: &Path) -> Diagnostics {
    let mut diagnostics = diagnostics.into_vec();
    for diagnostic in &mut diagnostics {
        diagnostic.path.get_or_insert_with(|| path.to_path_buf());
    }
    diagnostics.sort_by_key(|d| d.span.as_ref().map(|span| span.start_line));
    diagnostics.into()
}
