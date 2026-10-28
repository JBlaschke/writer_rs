//! Flat markup grouping, source labels, and recursive Typst includes.

use typst_syntax::ast::{self, AstNode};

use super::calls::{self, Nodes};
use super::literals::{self, Literal};
use super::*;
use crate::ir::{
    Attrs, DefinitionItem, Inline, ListItem, ListKind, MathDialect, RefForm,
    SourceKind,
};

/// English smart-quote policy ported from Typst 0.15.1 SmartQuoter::quote
/// (typst-library/text/smartquote.rs, Apache-2.0).
#[derive(Default)]
pub(super) struct Quotes {
    stack: Vec<bool>,
    before: Option<char>,
}

impl Quotes {
    fn quote(&mut self, double: bool) -> char {
        let opened = self.stack.last().copied();
        let before = self.before.unwrap_or(' ');
        if before.is_numeric() && opened != Some(double) {
            return if double { '″' } else { '′' };
        }
        if !double
            && opened != Some(false)
            && (before.is_alphabetic() || before == '\u{FFFC}')
        {
            return '’';
        }
        if opened == Some(double)
            && !before.is_whitespace()
            && !matches!(before, '(' | '[' | '{')
        {
            self.stack.pop();
            return if double { '”' } else { '’' };
        }
        if self.stack.len() < 32 {
            self.stack.push(double);
        }
        if double { '“' } else { '‘' }
    }
}

#[derive(Default)]
enum Target {
    #[default]
    Orphan,
    Ignored,
    Block(usize),
    Pending(usize),
    Inline(usize, usize),
}

#[derive(Default)]
struct Flow {
    blocks: Vec<Block>,
    pending: Inlines,
    span: Option<SourceSpan>,
    target: Target,
    cite_end: Option<usize>,
}

impl Flow {
    fn flush(&mut self) {
        self.cite_end = None;
        let mut inlines = std::mem::take(&mut self.pending);
        trim(&mut inlines);
        compact_text(&mut inlines);
        if inlines.is_empty() {
            self.span = None;
            return;
        }
        let index = self.blocks.len();
        let anchor = matches!(&inlines[..], [Inline::Span { inlines, .. }] if inlines.is_empty());
        let target = std::mem::replace(&mut self.target, Target::Ignored);
        self.target = match target {
            Target::Pending(_) if anchor => Target::Block(index),
            Target::Pending(_) => Target::Inline(index, inlines.len() - 1),
            other => other,
        };
        let span = self.span.take();
        self.blocks.push(if anchor {
            let Inline::Span { attrs, .. } = inlines.remove(0) else {
                unreachable!()
            };
            Block::Div { attrs, blocks: Vec::new(), span }
        } else {
            Block::Paragraph { inlines, span }
        });
    }

    fn blocks(&mut self, blocks: Vec<Block>) {
        self.flush();
        let start = self.blocks.len();
        self.blocks.extend(blocks);
        self.target = self
            .blocks
            .last()
            .filter(|_| self.blocks.len() > start)
            .filter(|block| block_labelable(block))
            .map_or(Target::Ignored, |_| Target::Block(self.blocks.len() - 1));
    }

    fn barrier(&mut self) {
        self.target = Target::Ignored;
        self.cite_end = None;
    }

    fn inlines(
        &mut self, inlines: Inlines, span: SourceSpan, space: bool,
        merge_cite: bool,
    ) {
        if inlines.is_empty() {
            if !space {
                self.target = Target::Ignored;
            }
            return;
        }
        if !space {
            self.span = Some(match self.span.take() {
                Some(mut previous) => {
                    previous.end_line = span.end_line;
                    previous
                }
                None => span,
            });
        }
        if space
            && (self.pending.is_empty()
                || matches!(self.pending.last(), Some(Inline::LineBreak)))
        {
            return;
        }
        for inline in inlines {
            if matches!(inline, Inline::LineBreak) {
                trim_end(&mut self.pending);
            }
            if merge_cite
                && let Inline::Cite { items, mode, span, .. } = &inline
            {
                let at =
                    self.pending.iter().rposition(|inline| !is_space(inline));
                if let Some(at) = at
                    && let Inline::Cite {
                        items: previous,
                        mode: previous_mode,
                        span: previous_span,
                        ..
                    } = &mut self.pending[at]
                    && previous_mode == mode
                {
                    previous.extend(items.clone());
                    if let (Some(previous), Some(next)) = (previous_span, span)
                    {
                        previous.end_line = next.end_line;
                    }
                    self.pending.truncate(at + 1);
                    self.target = Target::Ignored;
                    continue;
                }
            }
            if !space
                && let Inline::Text { text } = &inline
                && let Some(Inline::Text { text: previous }) =
                    self.pending.last_mut()
            {
                previous.push_str(text);
            } else {
                self.pending.push(inline);
            }
        }
        if !space {
            self.target = self
                .pending
                .last()
                .filter(|inline| inline_labelable(inline))
                .map_or(Target::Ignored, |_| {
                    Target::Pending(self.pending.len() - 1)
                });
        }
    }

    fn label(&mut self, label: &str) -> bool {
        match self.target {
            Target::Block(index) => label_block(&mut self.blocks[index], label),
            Target::Pending(index) => self
                .pending
                .get_mut(index)
                .is_some_and(|inline| label_inline(inline, label)),
            Target::Inline(block, index) => match &mut self.blocks[block] {
                Block::Paragraph { inlines, .. } => inlines
                    .get_mut(index)
                    .is_some_and(|inline| label_inline(inline, label)),
                _ => false,
            },
            _ => false,
        }
    }

    fn image_target(&self) -> bool {
        let inline = match self.target {
            Target::Pending(index) => self.pending.get(index),
            Target::Inline(block, index) => match &self.blocks[block] {
                Block::Paragraph { inlines, .. } => inlines.get(index),
                _ => None,
            },
            _ => None,
        };
        matches!(inline, Some(Inline::Image { .. }))
    }
}

impl<'a> Context<'a> {
    pub(super) fn walk_markup(
        &mut self, markup: ast::Markup<'a>, root: bool,
    ) -> Vec<Block> {
        let previous_quotes = std::mem::take(&mut self.quotes);
        self.env.push();
        let expressions = markup.exprs().collect::<Vec<_>>();
        let mut flow = Flow::default();
        let mut index = 0;
        while index < expressions.len() {
            let expr = expressions[index];
            if !self.expansion_expr(expr.to_untyped()) {
                break;
            }
            match expr {
                ast::Expr::Parbreak(_) => {
                    flow.flush();
                    self.quotes = Quotes::default();
                }
                ast::Expr::Label(label) => self.attach(&mut flow, label),
                ast::Expr::FuncCall(call)
                    if self.callee(call.callee()).as_deref()
                        == Some("label") =>
                {
                    self.literal_label(&mut flow, expr);
                }
                ast::Expr::LetBinding(node) => {
                    literals::record_let(self, node, !root);
                    flow.barrier();
                }
                ast::Expr::ModuleImport(node) => {
                    literals::record_import(self, node, !root);
                    flow.barrier();
                }
                ast::Expr::SetRule(_) | ast::Expr::ShowRule(_) => {
                    if !root {
                        self.ignored_rule(expr);
                    }
                    flow.barrier();
                }
                ast::Expr::ListItem(_)
                | ast::Expr::EnumItem(_)
                | ast::Expr::TermItem(_) => {
                    flow.flush();
                    self.quotes = Quotes::default();
                    let (block, next) = self.list_group(&expressions, index);
                    flow.blocks(vec![block]);
                    index = next;
                    continue;
                }
                _ if self.block_expr(expr) => {
                    flow.flush();
                    self.quotes = Quotes::default();
                    let blocks = self.expr_blocks(expr);
                    flow.blocks(blocks);
                    self.retain_label_target(&mut flow, expr);
                    self.quotes = Quotes::default();
                }
                _ => {
                    self.push_inline_expr(&mut flow, expr);
                }
            }
            index += 1;
        }
        flow.flush();
        self.env.pop();
        self.quotes = previous_quotes;
        flow.blocks
    }

    pub(super) fn walk_inlines(&mut self, markup: ast::Markup<'a>) -> Inlines {
        self.env.push();
        let mut flow = Flow::default();
        for expr in markup.exprs() {
            if !self.expansion_expr(expr.to_untyped()) {
                break;
            }
            match expr {
                ast::Expr::Label(label) => self.attach(&mut flow, label),
                ast::Expr::FuncCall(call)
                    if self.callee(call.callee()).as_deref()
                        == Some("label") =>
                {
                    self.literal_label(&mut flow, expr);
                }
                ast::Expr::Parbreak(_) => {
                    self.quotes = Quotes::default();
                    flow.inlines(
                        vec![Inline::LineBreak],
                        self.span(expr.to_untyped()),
                        false,
                        false,
                    );
                    flow.cite_end = None;
                }
                ast::Expr::LetBinding(node) => {
                    literals::record_let(self, node, true);
                    flow.barrier();
                }
                ast::Expr::ModuleImport(node) => {
                    literals::record_import(self, node, true);
                    flow.barrier();
                }
                ast::Expr::SetRule(_) | ast::Expr::ShowRule(_) => {
                    self.ignored_rule(expr);
                    flow.barrier();
                }
                _ => {
                    self.push_inline_expr(&mut flow, expr);
                }
            }
        }
        trim(&mut flow.pending);
        compact_text(&mut flow.pending);
        self.env.pop();
        flow.pending
    }

    fn push_inline_expr(&mut self, flow: &mut Flow, expr: ast::Expr<'a>) {
        let space = matches!(expr, ast::Expr::Space(_));
        let direct_cite = matches!(expr, ast::Expr::FuncCall(call)
            if self.callee(call.callee()).as_deref() == Some("cite"));
        let range = self.range(expr.to_untyped());
        let inlines = self.expr_inlines(expr);
        let cite = direct_cite && matches!(&inlines[..], [Inline::Cite { .. }]);
        // Markup's expression iterator omits comments. Inspect the source gap
        // so only adjacent calls separated by Space can form one group. A
        // call's range starts after its markup '#'.
        let merge_cite = cite
            && flow.cite_end.is_some_and(|end| {
                self.syntax.text().get(end..range.start).is_some_and(|gap| {
                    gap.strip_suffix('#')
                        .unwrap_or(gap)
                        .chars()
                        .all(char::is_whitespace)
                })
            });
        flow.inlines(inlines, self.span(expr.to_untyped()), space, merge_cite);
        if !space {
            flow.cite_end = cite.then_some(range.end);
            self.retain_label_target(flow, expr);
        }
    }

    fn block_expr(&self, expr: ast::Expr<'a>) -> bool {
        match expr {
            ast::Expr::Heading(_)
            | ast::Expr::ListItem(_)
            | ast::Expr::EnumItem(_)
            | ast::Expr::TermItem(_)
            | ast::Expr::ModuleInclude(_) => true,
            ast::Expr::Raw(node) => node.block(),
            ast::Expr::Equation(node) => node.block(),
            ast::Expr::FuncCall(call) => calls::is_block(self, call),
            _ => false,
        }
    }

    fn retain_label_target(&self, flow: &mut Flow, expr: ast::Expr<'a>) {
        let allowed = match expr {
            ast::Expr::Heading(_)
            | ast::Expr::Raw(_)
            | ast::Expr::Equation(_)
            | ast::Expr::Link(_) => true,
            ast::Expr::FuncCall(call) => {
                match self.callee(call.callee()).as_deref() {
                    Some("box") => flow.image_target(),
                    Some(
                        "heading" | "figure" | "image" | "table" | "raw"
                        | "listing" | "math.equation" | "mitex" | "callout"
                        | "code-cell" | "link" | "footnote" | "metadata",
                    ) => true,
                    _ => false,
                }
            }
            _ => false,
        };
        if !allowed {
            flow.target = Target::Ignored;
        }
    }

    pub(super) fn expr_blocks(&mut self, expr: ast::Expr<'a>) -> Vec<Block> {
        let node = expr.to_untyped();
        let span = Some(self.span(node));
        match expr {
            ast::Expr::ContentBlock(_) => match self.literal(expr) {
                Ok(Literal::Content(content)) => self.content_blocks(&content),
                Err(error) => {
                    error.report(self, node, "E-TYP-ARG-INVALID");
                    Vec::new()
                }
                _ => Vec::new(),
            },
            ast::Expr::Ident(_) => match self.literal(expr) {
                Ok(Literal::Content(content)) => self.content_blocks(&content),
                _ => self.inline_paragraph(expr),
            },
            ast::Expr::Parenthesized(node) => self.expr_blocks(node.expr()),
            ast::Expr::Heading(heading) => {
                let depth = heading.depth().get();
                if depth > 6 {
                    self.warning(
                        "W-TYP-ARG-IGNORED",
                        format!("heading depth {depth} is clamped to 6"),
                        node,
                    );
                }
                let inlines = self.walk_inlines(heading.body());
                vec![Block::Heading {
                    level: depth.min(6) as u8,
                    inlines,
                    attrs: Attrs::default(),
                    number: None,
                    span,
                }]
            }
            ast::Expr::Raw(raw) if raw.block() => vec![Block::CodeBlock {
                lang: raw.lang().map(|lang| lang.get().to_string()),
                text: raw_text(raw),
                attrs: Attrs::default(),
                span,
            }],
            ast::Expr::Equation(equation) if equation.block() => {
                vec![Block::Math {
                    source: equation
                        .body()
                        .to_untyped()
                        .full_text()
                        .trim()
                        .into(),
                    dialect: MathDialect::Typst,
                    attrs: Attrs::default(),
                    number: None,
                    span,
                }]
            }
            ast::Expr::ModuleInclude(include) => self.include(include),
            ast::Expr::FuncCall(call) => match calls::call(self, call) {
                Nodes::Blocks(blocks) => blocks,
                Nodes::Inlines(inlines) => {
                    vec![Block::Paragraph { inlines, span }]
                }
                Nodes::Anchor => vec![Block::Div {
                    attrs: Attrs::default(),
                    blocks: Vec::new(),
                    span,
                }],
                Nodes::Ignored => Vec::new(),
            },
            _ => self.inline_paragraph(expr),
        }
    }

    fn inline_paragraph(&mut self, expr: ast::Expr<'a>) -> Vec<Block> {
        let inlines = self.expr_inlines(expr);
        if inlines.is_empty() {
            Vec::new()
        } else {
            vec![Block::Paragraph {
                inlines,
                span: Some(self.span(expr.to_untyped())),
            }]
        }
    }

    pub(super) fn expr_inlines(&mut self, expr: ast::Expr<'a>) -> Inlines {
        let node = expr.to_untyped();
        let result = match expr {
            ast::Expr::Text(text) => vec![Inline::text(text.get())],
            ast::Expr::Space(_) => vec![if node.leaf_text().contains('\n')
                || node.leaf_text().contains('\r')
            {
                Inline::SoftBreak
            } else {
                Inline::text(" ")
            }],
            ast::Expr::Escape(escape) => {
                vec![Inline::text(&escape.get().to_string())]
            }
            ast::Expr::Shorthand(shorthand) => {
                vec![Inline::text(&shorthand.get().to_string())]
            }
            ast::Expr::SmartQuote(quote) => {
                let value = self.quotes.quote(quote.double());
                vec![Inline::text(&value.to_string())]
            }
            ast::Expr::Linebreak(_) => vec![Inline::LineBreak],
            ast::Expr::Strong(strong) => vec![Inline::Strong {
                inlines: self.walk_inlines(strong.body()),
            }],
            ast::Expr::Emph(emph) => {
                vec![Inline::Emph { inlines: self.walk_inlines(emph.body()) }]
            }
            ast::Expr::Raw(raw) => {
                let mut attrs = Attrs::default();
                if let Some(lang) = raw.lang() {
                    attrs.classes.push(lang.get().to_string());
                }
                vec![Inline::Code { text: raw_text(raw), attrs }]
            }
            ast::Expr::Equation(equation) => vec![Inline::Math {
                source: equation.body().to_untyped().full_text().trim().into(),
                dialect: MathDialect::Typst,
            }],
            ast::Expr::Link(link) => vec![Inline::Link {
                inlines: vec![Inline::text(link.get())],
                url: link.get().to_string(),
                title: None,
                attrs: Attrs::default(),
            }],
            ast::Expr::Ref(reference) => {
                let supplement = reference
                    .supplement()
                    .map(|body| self.walk_inlines(body.body()));
                vec![Inline::Ref {
                    target: reference.target().into(),
                    supplement,
                    form: RefForm::Normal,
                    resolved: None,
                    span: Some(self.span(node)),
                }]
            }
            ast::Expr::ContentBlock(_)
            | ast::Expr::Ident(_)
            | ast::Expr::Str(_)
            | ast::Expr::Int(_)
            | ast::Expr::Float(_) => {
                match self.literal(expr) {
                    Ok(Literal::Str(text)) => vec![Inline::Text { text }],
                    Ok(Literal::Int(value)) => {
                        vec![Inline::text(&value.to_string())]
                    }
                    Ok(Literal::Float(value)) => {
                        vec![Inline::text(&value.to_string())]
                    }
                    Ok(Literal::Content(content)) => {
                        self.content_inlines(&content)
                    }
                    _ => {
                        self.error("E-TYP-IDENT-UNKNOWN", "identifier has no scalar or content literal binding", node);
                        Vec::new()
                    }
                }
            }
            ast::Expr::Parenthesized(group) => self.expr_inlines(group.expr()),
            ast::Expr::FuncCall(call) => match calls::call(self, call) {
                Nodes::Inlines(inlines) => inlines,
                Nodes::Blocks(blocks) => {
                    blocks_into_inlines(self, blocks, node)
                }
                Nodes::Anchor => vec![Inline::Span {
                    attrs: Attrs::default(),
                    inlines: Vec::new(),
                }],
                Nodes::Ignored => Vec::new(),
            },
            _ => {
                self.error(
                    "E-TYP-CODE-UNSUPPORTED",
                    "code is outside the structured subset",
                    node,
                );
                Vec::new()
            }
        };
        if let Some(inline) = result.last() {
            self.quotes.before = last_character(inline);
        }
        result
    }

    fn ignored_rule(&mut self, expr: ast::Expr<'a>) {
        let (code, message) = if matches!(expr, ast::Expr::SetRule(_)) {
            ("W-TYP-SET-IGNORED", "nested set rule is ignored")
        } else {
            ("W-TYP-SHOW-IGNORED", "nested show rule is ignored")
        };
        self.warning(code, message, expr.to_untyped());
    }

    fn attach(&mut self, flow: &mut Flow, label: ast::Label<'a>) {
        self.attach_name(flow, label.get(), label.to_untyped());
    }

    fn literal_label(&mut self, flow: &mut Flow, expr: ast::Expr<'a>) {
        flow.cite_end = None;
        match self.literal(expr) {
            Ok(Literal::Label(label)) => {
                self.attach_name(flow, &label, expr.to_untyped());
            }
            Err(error) => {
                error.report(self, expr.to_untyped(), "E-TYP-ARG-INVALID");
                flow.target = Target::Ignored;
            }
            _ => flow.target = Target::Ignored,
        }
    }

    fn attach_name(&mut self, flow: &mut Flow, label: &str, node: &SyntaxNode) {
        let orphan = matches!(flow.target, Target::Orphan);
        let attached = flow.label(label);
        flow.barrier();
        if attached {
            return;
        }
        let code = if orphan {
            "W-TYP-LABEL-ORPHAN"
        } else {
            "W-TYP-LABEL-IGNORED"
        };
        self.warning(code, "label has no supported preceding target", node);
    }

    fn list_group(
        &mut self, expressions: &[ast::Expr<'a>], start: usize,
    ) -> (Block, usize) {
        let first = expressions[start];
        let kind = list_kind(first);
        let mut index = start;
        let mut last = start;
        let mut tight = true;
        let mut items = Vec::new();
        let mut definitions = Vec::new();
        loop {
            match expressions[index] {
                ast::Expr::ListItem(item) => items.push(ListItem {
                    checked: None,
                    blocks: self.walk_markup(item.body(), false),
                }),
                ast::Expr::EnumItem(item) => items.push(ListItem {
                    checked: None,
                    blocks: self.walk_markup(item.body(), false),
                }),
                ast::Expr::TermItem(item) => definitions.push(DefinitionItem {
                    term: self.walk_inlines(item.term()),
                    definitions: vec![
                        self.walk_markup(item.description(), false),
                    ],
                }),
                _ => break,
            }
            last = index;
            let mut next = index + 1;
            let mut separated = false;
            while let Some(expr) = expressions.get(next) {
                if matches!(expr, ast::Expr::Parbreak(_)) {
                    separated = true;
                } else if !matches!(expr, ast::Expr::Space(_)) {
                    break;
                }
                next += 1;
            }
            if expressions
                .get(next)
                .is_some_and(|expr| list_kind(*expr) == kind)
            {
                tight &= !separated;
                index = next;
            } else {
                break;
            }
        }
        let mut span = self.span(first.to_untyped());
        span.end_line = self.span(expressions[last].to_untyped()).end_line;
        let block = if kind == 2 {
            Block::DefinitionList { items: definitions, span: Some(span) }
        } else {
            tight &= items.iter().all(|item| {
                item.blocks.iter().all(|block| {
                    matches!(
                        block,
                        Block::Paragraph { .. }
                            | Block::List { .. }
                            | Block::CodeBlock { .. }
                    )
                })
            });
            let kind = match first {
                ast::Expr::EnumItem(item) => {
                    ListKind::Ordered { start: item.number().unwrap_or(1) }
                }
                _ => ListKind::Bullet,
            };
            Block::List { kind, tight, items, span: Some(span) }
        };
        (block, last + 1)
    }

    fn include(&mut self, include: ast::ModuleInclude<'a>) -> Vec<Block> {
        let node = include.to_untyped();
        let written = match self.literal(include.source()) {
            Ok(Literal::Str(path) | Literal::Path(path)) => path,
            _ => {
                self.error(
                    "E-TYP-INCLUDE-DYNAMIC",
                    "include path must be a literal string or path",
                    node,
                );
                return Vec::new();
            }
        };
        let path = match self.resolve_path(&written) {
            Ok(path) => path,
            Err(message) => {
                self.error("E-TYP-INCLUDE-MISSING", message, node);
                return Vec::new();
            }
        };
        self.remember_include(path.clone());
        if self.chain.contains(&path) {
            let mut chain = self
                .chain
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            chain.push(path.display().to_string());
            self.error(
                "E-TYP-INCLUDE-CYCLE",
                format!("include cycle: {}", chain.join(" → ")),
                node,
            );
            return Vec::new();
        }
        if self.chain.len() > 8 {
            self.error(
                "E-TYP-INCLUDE-DEPTH",
                "include nesting exceeds 8 levels",
                node,
            );
            return Vec::new();
        }
        let kind = match ir::detect_kind(&path) {
            Ok(kind) => kind,
            Err(error) => {
                self.error("E-TYP-INCLUDE-MISSING", error.to_string(), node);
                return Vec::new();
            }
        };
        if kind != SourceKind::Typst {
            self.pending_notebook(&written, node);
            return Vec::new();
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.error(
                    "E-TYP-INCLUDE-MISSING",
                    format!("cannot include `{written}`: {error}"),
                    node,
                );
                return Vec::new();
            }
        };
        let source = SourceInfo {
            path: path.clone(),
            kind,
            base_dir: path.parent().unwrap_or(&self.root).into(),
        };
        let mut chain = self.chain.clone();
        chain.push(path);
        let child = match parse_source(
            &source,
            &text,
            &self.config,
            &self.root,
            chain,
            self.expansion.clone(),
        ) {
            Ok(child) => child,
            Err(error) => {
                self.error(
                    "E-TYP-INCLUDE-MISSING",
                    format!("cannot include `{written}`: {error}"),
                    node,
                );
                return Vec::new();
            }
        };
        if child.explicit_meta {
            self.warning("include-meta-ignored", format!("`{written}` sets document metadata; an included file's document metadata is ignored"), node);
        }
        let mut document = child.parsed.document;
        if matches!(
            document.blocks.first(),
            Some(Block::Toc { span: None, .. })
        ) {
            document.blocks.remove(0);
        }
        self.assets.merge(child.parsed.assets);
        self.diagnostics.extend(child.parsed.diagnostics);
        for path in document.includes {
            self.remember_include(path);
        }
        document.blocks
    }
}

fn list_kind(expr: ast::Expr<'_>) -> u8 {
    match expr {
        ast::Expr::ListItem(_) => 0,
        ast::Expr::EnumItem(_) => 1,
        ast::Expr::TermItem(_) => 2,
        _ => 3,
    }
}

fn raw_text(raw: ast::Raw<'_>) -> String {
    raw.lines().map(|line| line.get().as_str()).collect::<Vec<_>>().join("\n")
}

fn blocks_into_inlines(
    ctx: &mut Context<'_>, blocks: Vec<Block>, node: &SyntaxNode,
) -> Inlines {
    let mut inlines = Vec::new();
    for block in blocks {
        match block {
            Block::Paragraph { inlines: next, .. } => {
                if !inlines.is_empty() {
                    inlines.push(Inline::LineBreak);
                }
                inlines.extend(next);
            }
            _ => {
                ctx.error("E-TYP-ARG-INVALID", "expected inline content", node)
            }
        }
    }
    inlines
}

fn last_character(inline: &Inline) -> Option<char> {
    match inline {
        Inline::Math { .. }
        | Inline::Image { .. }
        | Inline::Footnote { .. }
        | Inline::FootnoteRef { .. } => Some('\u{FFFC}'),
        Inline::SoftBreak | Inline::LineBreak => Some(' '),
        _ => ir::plain_text(std::slice::from_ref(inline)).chars().last(),
    }
}

fn is_space(inline: &Inline) -> bool {
    matches!(inline, Inline::SoftBreak)
        || matches!(inline, Inline::Text { text } if text.chars().all(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r')))
}

fn trim(inlines: &mut Inlines) -> usize {
    let leading = inlines.iter().take_while(|inline| is_space(inline)).count();
    inlines.drain(..leading);
    trim_end(inlines);
    leading
}

fn trim_end(inlines: &mut Inlines) {
    while inlines.last().is_some_and(is_space) {
        inlines.pop();
    }
}

fn compact_text(inlines: &mut Inlines) {
    let mut result = Vec::with_capacity(inlines.len());
    for inline in std::mem::take(inlines) {
        if let Inline::Text { text } = &inline
            && let Some(Inline::Text { text: previous }) = result.last_mut()
        {
            previous.push_str(text);
        } else {
            result.push(inline);
        }
    }
    *inlines = result;
}

fn inline_labelable(inline: &Inline) -> bool {
    matches!(
        inline,
        Inline::Code { .. }
            | Inline::Link { .. }
            | Inline::Image { .. }
            | Inline::Footnote { .. }
    ) || matches!(inline, Inline::Span { inlines, .. } if inlines.is_empty())
}

fn block_labelable(block: &Block) -> bool {
    matches!(
        block,
        Block::Heading { .. }
            | Block::Math { .. }
            | Block::Figure { .. }
            | Block::Table { .. }
            | Block::CodeBlock { .. }
            | Block::Callout { .. }
            | Block::Cell(_)
    ) || matches!(block, Block::Div { blocks, .. } if blocks.is_empty())
        || matches!(block, Block::Paragraph { inlines, .. } if inlines.len() == 1 && inline_labelable(&inlines[0]))
}

fn label_inline(inline: &mut Inline, label: &str) -> bool {
    match inline {
        Inline::Code { attrs, .. }
        | Inline::Link { attrs, .. }
        | Inline::Image { attrs, .. }
        | Inline::Span { attrs, .. } => attrs.id = Some(label.into()),
        Inline::Footnote { label: target, .. } => {
            *target =
                Some(label.strip_prefix("writer-fn-").unwrap_or(label).into())
        }
        _ => return false,
    }
    true
}

fn label_block(block: &mut Block, label: &str) -> bool {
    match block {
        Block::Heading { attrs, .. }
        | Block::Math { attrs, .. }
        | Block::Figure { attrs, .. }
        | Block::Table { attrs, .. }
        | Block::CodeBlock { attrs, .. }
        | Block::Callout { attrs, .. }
        | Block::Div { attrs, .. } => attrs.id = Some(label.into()),
        Block::Cell(cell) => cell.attrs.id = Some(label.into()),
        Block::Paragraph { inlines, .. } if inlines.len() == 1 => {
            return label_inline(&mut inlines[0], label);
        }
        _ => return false,
    }
    true
}

pub(super) fn visit_blocks(blocks: &[Block], f: &mut dyn FnMut(&Block)) {
    let document = Document {
        meta: DocMeta::default(),
        blocks: blocks.to_vec(),
        features: Features::default(),
        source: SourceInfo {
            path: PathBuf::new(),
            base_dir: PathBuf::new(),
            kind: SourceKind::Typst,
        },
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Resolved::default(),
    };
    ir::walk_blocks(&document, f);
}
