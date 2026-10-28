//! Structured Typst syntax to document IR (typst-dialect.md §§2–3).
//!
//! This frontend walks syntax and literal bindings without evaluating user code.

mod calls;
mod literals;
mod metadata;
mod normalize;
mod tables;
mod walker;

use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use typst_syntax::ast::{self, AstNode};
use typst_syntax::{DiagSpanKind, Source, SyntaxDiagnostic, SyntaxNode};

use crate::assets::registry::AssetRegistry;
use crate::config::{Config, Toc};
use crate::error::{Diagnostic, Diagnostics, Level, Result};
use crate::ir::{
    self, Block, DocMeta, Document, Features, Inlines, Parsed, Resolved,
    SourceInfo, SourceSpan,
};

use literals::{ContentValue, Env, Literal, LiteralError};

/// Parses the supported Typst syntax subset without evaluating or compiling it.
pub fn parse(
    source: &SourceInfo, text: &str, config: &Config,
) -> Result<Parsed> {
    let root = canonical(&source.base_dir);
    let chain = vec![canonical(&source.path)];
    let expansion = Rc::new(RefCell::new(Expansion::default()));
    Ok(parse_source(source, text, config, &root, chain, expansion)?.parsed)
}

/// Shared across included files and captured literal environments. Count only
/// replayed content so ordinary source length does not consume this allowance.
#[derive(Default)]
struct Expansion {
    depth: usize,
    visited: usize,
    reported: bool,
}

struct ParsedSource {
    parsed: Parsed,
    explicit_meta: bool,
}

fn parse_source(
    source: &SourceInfo, text: &str, config: &Config, root: &Path,
    chain: Vec<PathBuf>, expansion: Rc<RefCell<Expansion>>,
) -> Result<ParsedSource> {
    let syntax = Source::detached(text);
    let mut ctx = Context {
        source: source.clone(),
        syntax: &syntax,
        root: root.into(),
        config: config.clone(),
        meta: DocMeta::default(),
        env: Env::default(),
        assets: AssetRegistry::new(),
        diagnostics: Diagnostics::new(),
        includes: Vec::new(),
        chain,
        quotes: walker::Quotes::default(),
        expansion,
    };
    let (errors, warnings) = syntax.root().errors_and_warnings();
    let has_errors = !errors.is_empty();
    for finding in errors.into_iter().chain(warnings) {
        ctx.syntax_diagnostic(finding);
    }
    if has_errors {
        return Ok(ctx.finish(Vec::new(), false));
    }
    // Source::detached always has a Markup root, including empty input.
    let markup = syntax.root().cast::<ast::Markup>().expect("markup root");
    let meta = metadata::prepare(&mut ctx, markup)?;
    let mut blocks = ctx.walk_markup(markup, true);
    ctx.config.merge_front_matter(&ctx.meta)?;
    let mut toc = false;
    let mut bibliography = false;
    walker::visit_blocks(&blocks, &mut |block| {
        toc |= matches!(block, Block::Toc { .. });
        bibliography |= matches!(block, Block::Bibliography { .. });
    });
    if let Some(node) = meta.show_node {
        for kind in [
            (meta.show_toc && toc).then_some("TOC"),
            (meta.show_bibliography && bibliography).then_some("bibliography"),
        ]
        .into_iter()
        .flatten()
        {
            ctx.warning(
                "W-TYP-DUPLICATE-BLOCK",
                format!("writer-doc and the body both request a {kind}"),
                node,
            );
        }
    }
    if let Toc::Depth(depth) = ctx.config.document.toc
        && !toc
    {
        blocks.insert(0, Block::Toc { depth, span: None });
    }
    Ok(ctx.finish(blocks, meta.explicit))
}

/// Per-file syntax state; private children share only this literal/walker seam.
struct Context<'a> {
    source: SourceInfo,
    syntax: &'a Source,
    root: PathBuf,
    config: Config,
    meta: DocMeta,
    env: Env<'a>,
    assets: AssetRegistry,
    diagnostics: Diagnostics,
    includes: Vec<PathBuf>,
    chain: Vec<PathBuf>,
    quotes: walker::Quotes,
    expansion: Rc<RefCell<Expansion>>,
}

impl<'a> Context<'a> {
    fn range(&self, node: &SyntaxNode) -> Range<usize> {
        self.syntax.find(node.span()).map_or(0..0, |node| node.range())
    }

    fn span(&self, node: &SyntaxNode) -> SourceSpan {
        self.span_range(self.range(node))
    }

    fn span_range(&self, range: Range<usize>) -> SourceSpan {
        let lines = self.syntax.lines();
        let end = range.end.saturating_sub(1).max(range.start);
        SourceSpan {
            start_line: lines.byte_to_line(range.start).unwrap_or(0) + 1,
            end_line: lines.byte_to_line(end).unwrap_or(0) + 1,
            cell: None,
            file: Some(self.source.path.clone()),
        }
    }

    fn diagnostic(
        &mut self, level: Level, code: &'static str,
        message: impl Into<String>, node: &SyntaxNode,
    ) {
        self.diagnostic_range(level, code, message.into(), self.range(node));
    }

    fn diagnostic_range(
        &mut self, level: Level, code: &'static str, message: String,
        range: Range<usize>,
    ) {
        let column =
            self.syntax.lines().byte_to_column(range.start).unwrap_or(0) + 1;
        let mut diagnostic =
            Diagnostic::warning(code, format!("{message} (col {column})"));
        diagnostic.level = level;
        diagnostic.path = Some(self.source.path.clone());
        diagnostic.span = Some(self.span_range(range));
        self.diagnostics.push(diagnostic);
    }

    fn error(
        &mut self, code: &'static str, message: impl Into<String>,
        node: &SyntaxNode,
    ) {
        self.diagnostic(Level::Error, code, message, node);
    }

    fn warning(
        &mut self, code: &'static str, message: impl Into<String>,
        node: &SyntaxNode,
    ) {
        self.diagnostic(Level::Warning, code, message, node);
    }

    fn syntax_diagnostic(&mut self, finding: SyntaxDiagnostic) {
        let range = match finding.span.get() {
            DiagSpanKind::Number { num, sub_range, .. } => {
                self.syntax.range(num, sub_range).unwrap_or(0..0)
            }
            DiagSpanKind::Range { range, .. } => range,
            DiagSpanKind::Detached => 0..0,
        };
        let mut message = finding.message.to_string();
        for hint in finding.hints {
            message.push_str(&format!("; hint: {}", hint.v));
        }
        let (level, code) = if finding.is_error {
            (Level::Error, "E-TYP-SYNTAX")
        } else {
            (Level::Warning, "W-TYP-SYNTAX")
        };
        self.diagnostic_range(level, code, message, range);
    }

    fn literal(
        &self, expr: ast::Expr<'a>,
    ) -> std::result::Result<Literal<'a>, LiteralError> {
        literals::argument(expr, &self.env)
    }

    fn callee(&self, expr: ast::Expr<'a>) -> Option<String> {
        self.env.resolve_callee(expr)
    }

    fn content_blocks(&mut self, content: &ContentValue<'a>) -> Vec<Block> {
        if !self.enter_content(content) {
            return Vec::new();
        }
        let previous = std::mem::replace(&mut self.env, (*content.env).clone());
        self.env.push();
        let blocks = self.walk_markup(content.block.body(), false);
        self.env = previous;
        self.expansion.borrow_mut().depth -= 1;
        blocks
    }

    fn metadata_inlines(&mut self, content: &ContentValue<'a>) -> Inlines {
        let previous = std::mem::take(&mut self.quotes);
        let inlines = self.content_inlines(content);
        self.quotes = previous;
        inlines
    }

    fn content_inlines(&mut self, content: &ContentValue<'a>) -> Inlines {
        if !self.enter_content(content) {
            return Vec::new();
        }
        let previous = std::mem::replace(&mut self.env, (*content.env).clone());
        self.env.push();
        let inlines = self.walk_inlines(content.block.body());
        self.env = previous;
        self.expansion.borrow_mut().depth -= 1;
        inlines
    }

    fn enter_content(&mut self, content: &ContentValue<'a>) -> bool {
        let node = content.block.to_untyped();
        if self.expansion.borrow().depth >= 64 {
            self.expansion_error(node);
            return false;
        }
        self.expansion.borrow_mut().depth += 1;
        if !self.expansion_expr(node) {
            self.expansion.borrow_mut().depth -= 1;
            return false;
        }
        true
    }

    fn expansion_expr(&mut self, node: &SyntaxNode) -> bool {
        let mut expansion = self.expansion.borrow_mut();
        if expansion.depth == 0 {
            return true;
        }
        if expansion.visited < 65_536 {
            expansion.visited += 1;
            return true;
        }
        drop(expansion);
        self.expansion_error(node);
        false
    }

    fn expansion_error(&mut self, node: &SyntaxNode) {
        let reported =
            std::mem::replace(&mut self.expansion.borrow_mut().reported, true);
        if !reported {
            self.error(
                "E-TYP-ARG-INVALID",
                "literal content expansion limit exceeded",
                node,
            );
        }
    }

    fn resolve_path(
        &self, written: &str,
    ) -> std::result::Result<PathBuf, String> {
        if written.is_empty() || written.contains('\0') {
            return Err("expected a nonempty file path".into());
        }
        let path = if written.starts_with('/') {
            self.root.join(written.trim_start_matches('/'))
        } else {
            self.source.base_dir.join(written)
        };
        Ok(canonical(&path))
    }

    fn remember_include(&mut self, path: PathBuf) {
        if !self.includes.contains(&path) {
            self.includes.push(path);
        }
    }

    fn pending_notebook(&mut self, written: &str, node: &SyntaxNode) {
        if let Ok(path) = self.resolve_path(written) {
            self.remember_include(path);
        }
        self.error(
            "E-TYP-INCLUDE-MISSING",
            "including .md/.ipynb from typst arrives in T53",
            node,
        );
    }

    fn finish(self, blocks: Vec<Block>, explicit_meta: bool) -> ParsedSource {
        let mut document = Document {
            meta: self.meta,
            blocks,
            features: Features::default(),
            source: self.source,
            assets: Vec::new(),
            includes: self.includes,
            resolved: Resolved::default(),
        };
        normalize::finish_document(&mut document);
        document.features = crate::markdown::collect_features(&document);
        document.assets = self.assets.infos(&document.source.base_dir);
        let diagnostics = crate::pipeline::escalate_strict(
            self.diagnostics, None, &self.config,
        );
        ParsedSource {
            parsed: Parsed { document, assets: self.assets, diagnostics },
            explicit_meta,
        }
    }
}

fn canonical(path: &Path) -> PathBuf {
    crate::pipeline::path_after_creating_dirs(path).unwrap_or_else(|| {
        std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
    })
}
