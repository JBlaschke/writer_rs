//! Explicit syntax-only call lowering for the structured Typst subset.

use typst_syntax::ast::AstNode;
use typst_syntax::{SyntaxNode, ast};

use crate::assets::registry::AssetRegistry;
use crate::config::Toc;
use crate::ir::{
    Attrs, Block, Cell, CellOutput, CiteItem, CiteMode, DefinitionItem,
    FigureKind, Inline, Inlines, ListItem, ListKind, MathDialect,
    OutputCandidate, OutputKind, RefForm, plain_text,
};
use crate::math::latex_source::{macros_as_newcommands, macros_from_config};

use super::Context;
use super::literals::{Args, Literal};
use super::{metadata, tables};

const INVALID: &str = "E-TYP-ARG-INVALID";

pub(super) enum Nodes {
    Blocks(Vec<Block>),
    Inlines(Inlines),
    Anchor,
    Ignored,
}

pub(super) fn is_block<'a>(ctx: &Context<'a>, call: ast::FuncCall<'a>) -> bool {
    let Some(name) = ctx.callee(call.callee()) else {
        return false;
    };
    match name.as_str() {
        "figure" | "image" | "table" | "list" | "enum" | "terms" | "align"
        | "block" | "par" | "pagebreak" | "colbreak" | "line" | "outline"
        | "bibliography" | "v" | "heading" | "callout" | "mitex"
        | "listing" | "code-cell" | "include-notebook" => true,
        "quote" | "raw" | "math.equation" => call.args().items().any(|arg| {
            matches!(arg, ast::Arg::Named(named)
                if named.name().as_str() == "block" &&
                   matches!(ctx.literal(named.expr()), Ok(Literal::Bool(true))))
        }),
        _ => false,
    }
}

pub(super) fn call<'a>(
    ctx: &mut Context<'a>, call: ast::FuncCall<'a>,
) -> Nodes {
    let args = Args::new(ctx, call);
    let Some(name) = ctx.callee(call.callee()) else {
        ctx.error(
            "E-TYP-CALL-UNKNOWN",
            "call is not in the structured subset",
            args.node,
        );
        return Nodes::Ignored;
    };
    let limit = match name.as_str() {
        "figure" | "image" | "quote" | "raw" | "heading" | "math.equation"
        | "footnote" | "cite" | "ref" | "bibliography" | "callout" | "mi"
        | "mitex" | "listing" | "include-notebook" | "strong" | "emph"
        | "underline" | "strike" | "smallcaps" | "super" | "sub"
        | "overline" | "highlight" => Some(1),
        "link" => Some(2),
        _ => None,
    };
    if let Some(limit) = limit
        && args.positional.len() > limit
    {
        ctx.error(
            INVALID,
            format!("`{name}` accepts at most {limit} positional argument(s)"),
            args.node,
        );
    }
    match name.as_str() {
        "figure" => figure(ctx, &args),
        "image" => image(ctx, &args).map_or(Nodes::Ignored, |image| {
            Nodes::Blocks(vec![Block::Paragraph {
                inlines: vec![image],
                span: Some(ctx.span(args.node)),
            }])
        }),
        "table" => tables::table(ctx, &args)
            .map_or(Nodes::Ignored, |table| Nodes::Blocks(vec![table])),
        "table.header" | "table.footer" | "table.cell" | "terms.item" => {
            ctx.error(
                INVALID,
                format!("{name} needs its enclosing table or terms call"),
                args.node,
            );
            Nodes::Ignored
        }
        "table.hline" | "table.vline" | "v" | "h" | "colbreak" => {
            Nodes::Ignored
        }
        "quote" => quote(ctx, &args),
        "list" | "enum" => list(ctx, &args, name == "enum"),
        "terms" => terms(ctx, &args),
        "align" | "block" | "par" | "box" | "text" => {
            wrapper(ctx, &args, &name)
        }
        "strong" | "emph" | "underline" | "strike" | "smallcaps" | "super"
        | "sub" | "overline" | "highlight" => formatting(ctx, &args, &name),
        "raw" => raw(ctx, &args),
        "heading" => heading(ctx, &args),
        "math.equation" => equation(ctx, &args),
        "metadata" => {
            if args.positional.len() == 1
                && args.pos(0).is_some_and(|expr| {
                    matches!(ctx.literal(expr), Ok(Literal::None))
                })
            {
                Nodes::Anchor
            } else {
                Nodes::Ignored
            }
        }
        "link" => link(ctx, &args),
        "footnote" => footnote(ctx, &args),
        "cite" | "ref" => reference(ctx, &args, name == "cite"),
        "linebreak" => {
            args.check_named(ctx, &[], &[]);
            Nodes::Inlines(vec![Inline::LineBreak])
        }
        "pagebreak" | "line" | "outline" => separator(ctx, &args, &name),
        "bibliography" => {
            args.check_named(ctx, &["style"], &["title", "full"]);
            if metadata::bibliography(ctx, &args) {
                Nodes::Blocks(vec![Block::Bibliography {
                    span: Some(ctx.span(args.node)),
                }])
            } else {
                Nodes::Ignored
            }
        }
        "callout" => callout(ctx, &args),
        "mi" | "mitex" => latex(ctx, &args, name == "mitex"),
        "listing" => listing(ctx, &args),
        "code-cell" => code_cell(ctx, &args),
        "include-notebook" => pending_notebook(ctx, &args),
        _ => {
            ctx.error(
                "E-TYP-CALL-UNKNOWN",
                format!("`{name}` is not in the structured subset"),
                args.node,
            );
            Nodes::Ignored
        }
    }
}

pub(super) fn literal<'a>(
    ctx: &mut Context<'a>, expr: ast::Expr<'a>,
) -> Option<Literal<'a>> {
    match ctx.literal(expr) {
        Ok(value) => Some(value),
        Err(error) => {
            error.report(ctx, expr.to_untyped(), INVALID);
            None
        }
    }
}

fn required<'a>(
    ctx: &mut Context<'a>, args: &Args<'a>, index: usize, name: &str,
) -> Option<ast::Expr<'a>> {
    let expr = args.pos(index);
    if expr.is_none() {
        ctx.error(
            INVALID,
            format!("missing required `{name}` argument"),
            args.node,
        );
    }
    expr
}

fn wrong(ctx: &mut Context<'_>, node: &SyntaxNode, expected: &str) {
    ctx.error(INVALID, format!("expected {expected}"), node);
}

pub(super) fn blocks<'a>(
    ctx: &mut Context<'a>, expr: ast::Expr<'a>,
) -> Option<Vec<Block>> {
    match expr {
        ast::Expr::Raw(_) | ast::Expr::Equation(_) | ast::Expr::FuncCall(_) => {
            Some(ctx.expr_blocks(expr))
        }
        _ => match literal(ctx, expr)? {
            Literal::Content(content) => Some(ctx.content_blocks(&content)),
            Literal::Str(text) => Some(vec![Block::Paragraph {
                inlines: vec![Inline::Text { text }],
                span: Some(ctx.span(expr.to_untyped())),
            }]),
            _ => {
                wrong(ctx, expr.to_untyped(), "content or a string");
                None
            }
        },
    }
}

fn inlines<'a>(ctx: &mut Context<'a>, expr: ast::Expr<'a>) -> Option<Inlines> {
    match expr {
        ast::Expr::Raw(_) | ast::Expr::Equation(_) | ast::Expr::FuncCall(_) => {
            Some(ctx.expr_inlines(expr))
        }
        _ => match literal(ctx, expr)? {
            Literal::Content(content) => Some(ctx.content_inlines(&content)),
            Literal::Str(text) => Some(vec![Inline::Text { text }]),
            _ => {
                wrong(ctx, expr.to_untyped(), "inline content or a string");
                None
            }
        },
    }
}

fn optional_inlines<'a>(
    ctx: &mut Context<'a>, expr: Option<ast::Expr<'a>>,
) -> Option<Inlines> {
    let expr = expr?;
    if matches!(ctx.literal(expr), Ok(Literal::None | Literal::Auto)) {
        None
    } else {
        inlines(ctx, expr)
    }
}

fn string<'a>(ctx: &mut Context<'a>, expr: ast::Expr<'a>) -> Option<String> {
    match literal(ctx, expr)? {
        Literal::Str(text) => Some(text),
        _ => {
            wrong(ctx, expr.to_untyped(), "a string");
            None
        }
    }
}

fn boolean<'a>(
    ctx: &mut Context<'a>, expr: Option<ast::Expr<'a>>, default: bool,
) -> bool {
    let Some(expr) = expr else { return default };
    match literal(ctx, expr) {
        Some(Literal::Bool(value)) => value,
        Some(_) => {
            wrong(ctx, expr.to_untyped(), "a boolean");
            default
        }
        None => default,
    }
}

fn unsigned<'a>(
    ctx: &mut Context<'a>, expr: Option<ast::Expr<'a>>, default: u64,
    positive: bool,
) -> u64 {
    let Some(expr) = expr else { return default };
    match literal(ctx, expr) {
        Some(Literal::Int(value)) if value >= 0 && (!positive || value > 0) => {
            value as u64
        }
        Some(_) => {
            wrong(
                ctx,
                expr.to_untyped(),
                if positive {
                    "a positive integer"
                } else {
                    "a nonnegative integer"
                },
            );
            default
        }
        None => default,
    }
}

fn unnumbered<'a>(
    ctx: &mut Context<'a>, expr: Option<ast::Expr<'a>>, attrs: &mut Attrs,
) {
    let Some(expr) = expr else { return };
    match ctx.literal(expr) {
        Ok(Literal::None) => attrs.classes.push("unnumbered".into()),
        Ok(Literal::Auto) => {}
        _ => ctx.warning(
            "W-TYP-ARG-IGNORED",
            "custom numbering is kept only in native Typst",
            expr.to_untyped(),
        ),
    }
}

fn figure<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(
        ctx,
        &["caption", "kind", "numbering"],
        &["placement", "scope", "gap", "outlined"],
    );
    let Some(expr) = required(ctx, args, 0, "body") else {
        return Nodes::Ignored;
    };
    let Some(body) = blocks(ctx, expr) else {
        return Nodes::Ignored;
    };
    let mut kind = infer_figure(&body);
    if let Some(expr) = args.named("kind") {
        match literal(ctx, expr) {
            Some(Literal::Symbol(name)) => match name.as_str() {
                "image" => kind = FigureKind::Image,
                "table" => kind = FigureKind::Table,
                "raw" => kind = FigureKind::Listing,
                _ => wrong(
                    ctx,
                    expr.to_untyped(),
                    "image, table, raw or a string kind",
                ),
            },
            Some(Literal::Auto) => {}
            Some(Literal::Str(name)) => {
                kind = FigureKind::Other;
                ctx.warning("W-TYP-FIGURE-KIND", format!("figure kind `{name}` uses the image counter outside native Typst"), expr.to_untyped());
            }
            Some(_) => wrong(
                ctx,
                expr.to_untyped(),
                "image, table, raw or a string kind",
            ),
            None => {}
        }
    }
    if !figure_shape(kind, &body) {
        kind = FigureKind::Other;
    }
    let caption = optional_inlines(ctx, args.named("caption"));
    let mut attrs = Attrs::default();
    unnumbered(ctx, args.named("numbering"), &mut attrs);
    Nodes::Blocks(vec![Block::Figure {
        kind,
        caption,
        body,
        attrs,
        number: None,
        span: Some(ctx.span(args.node)),
    }])
}

fn infer_figure(body: &[Block]) -> FigureKind {
    match body {
        [Block::Paragraph { inlines, .. }]
            if matches!(inlines.as_slice(), [Inline::Image { .. }]) =>
        {
            FigureKind::Image
        }
        [Block::Table { .. }] => FigureKind::Table,
        [Block::CodeBlock { .. }] => FigureKind::Listing,
        _ => FigureKind::Other,
    }
}

fn figure_shape(kind: FigureKind, body: &[Block]) -> bool {
    kind == FigureKind::Other || infer_figure(body) == kind
}

fn image<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Option<Inline> {
    args.check_named(
        ctx,
        &["width", "height", "alt"],
        &["fit", "scaling", "format", "page"],
    );
    let expr = required(ctx, args, 0, "path")?;
    let written = match literal(ctx, expr)? {
        Literal::Str(path) | Literal::Path(path) => path,
        _ => {
            wrong(ctx, expr.to_untyped(), "a path string or path(string)");
            return None;
        }
    };
    let path = match ctx.resolve_path(&written) {
        Ok(path) => path,
        Err(message) => {
            ctx.error(INVALID, message, expr.to_untyped());
            return None;
        }
    };
    let url = if path.is_file() {
        let id = ctx.assets.register_file(&path);
        AssetRegistry::placeholder(&id)
    } else {
        ctx.warning(
            "missing-asset",
            format!(
                "image `{written}` not found (looked for {})",
                path.display()
            ),
            args.node,
        );
        written
    };
    let mut attrs = Attrs::default();
    for name in ["width", "height"] {
        let Some(expr) = args.named(name) else {
            continue;
        };
        match literal(ctx, expr) {
            Some(Literal::Auto) => {}
            Some(Literal::Numeric { value, unit, .. }) => {
                let suffix = match unit {
                    ast::Unit::Pt => "pt",
                    ast::Unit::Mm => "mm",
                    ast::Unit::Cm => "cm",
                    ast::Unit::In => "in",
                    ast::Unit::Em => "em",
                    ast::Unit::Percent => "%",
                    _ => {
                        wrong(
                            ctx,
                            expr.to_untyped(),
                            "a pt/mm/cm/in/em/% dimension",
                        );
                        continue;
                    }
                };
                if value.is_finite() {
                    attrs.pairs.insert(name.into(), format!("{value}{suffix}"));
                } else {
                    wrong(ctx, expr.to_untyped(), "a finite dimension");
                }
            }
            Some(_) => wrong(ctx, expr.to_untyped(), "a dimension or auto"),
            None => {}
        }
    }
    let alt = args
        .named("alt")
        .and_then(|expr| string(ctx, expr))
        .map_or_else(Vec::new, |text| vec![Inline::Text { text }]);
    Some(Inline::Image { alt, url, title: None, attrs })
}

fn quote<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &["block"], &[]);
    let Some(expr) = required(ctx, args, 0, "body") else {
        return Nodes::Ignored;
    };
    if boolean(ctx, args.named("block"), false) {
        blocks(ctx, expr).map_or(Nodes::Ignored, |blocks| {
            Nodes::Blocks(vec![Block::BlockQuote {
                blocks,
                span: Some(ctx.span(args.node)),
            }])
        })
    } else {
        let Some(mut body) = inlines(ctx, expr) else {
            return Nodes::Ignored;
        };
        body.insert(0, Inline::Text { text: "“".into() });
        body.push(Inline::Text { text: "”".into() });
        Nodes::Inlines(body)
    }
}

fn list<'a>(ctx: &mut Context<'a>, args: &Args<'a>, ordered: bool) -> Nodes {
    args.check_named(
        ctx,
        if ordered {
            &["start", "tight"]
        } else {
            &["tight"]
        },
        &[],
    );
    let mut tight = boolean(ctx, args.named("tight"), true);
    let start = unsigned(ctx, args.named("start"), 1, false);
    let items: Vec<_> = args
        .positional
        .iter()
        .filter_map(|expr| {
            blocks(ctx, *expr).map(|blocks| ListItem { checked: None, blocks })
        })
        .collect();
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
    Nodes::Blocks(vec![Block::List {
        kind: if ordered {
            ListKind::Ordered { start }
        } else {
            ListKind::Bullet
        },
        tight,
        items,
        span: Some(ctx.span(args.node)),
    }])
}

fn terms<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &[], &[]);
    let mut items = Vec::new();
    for expr in &args.positional {
        let ast::Expr::FuncCall(call) = expr else {
            wrong(ctx, expr.to_untyped(), "terms.item(term, description)");
            continue;
        };
        if ctx.callee(call.callee()).as_deref() != Some("terms.item") {
            wrong(ctx, expr.to_untyped(), "terms.item(term, description)");
            continue;
        }
        let item = Args::new(ctx, *call);
        item.check_named(ctx, &[], &[]);
        if item.positional.len() > 2 {
            ctx.error(
                INVALID,
                "terms.item accepts a term and a description",
                item.node,
            );
        }
        let Some(term) =
            required(ctx, &item, 0, "term").and_then(|expr| inlines(ctx, expr))
        else {
            continue;
        };
        let Some(definition) = required(ctx, &item, 1, "description")
            .and_then(|expr| blocks(ctx, expr))
        else {
            continue;
        };
        items.push(DefinitionItem { term, definitions: vec![definition] });
    }
    Nodes::Blocks(vec![Block::DefinitionList {
        items,
        span: Some(ctx.span(args.node)),
    }])
}

fn wrapper<'a>(ctx: &mut Context<'a>, args: &Args<'a>, name: &str) -> Nodes {
    if name == "box"
        && let Some(ast::Expr::FuncCall(call)) = args.pos(0)
        && ctx.callee(call.callee()).as_deref() == Some("image")
    {
        let image_args = Args::new(ctx, call);
        return image(ctx, &image_args)
            .map_or(Nodes::Ignored, |image| Nodes::Inlines(vec![image]));
    }
    let index = usize::from(name == "align");
    let Some(expr) = required(ctx, args, index, "body") else {
        return Nodes::Ignored;
    };
    let mut attrs = Attrs::default();
    if name == "align"
        && let Some(dir) = args.pos(0)
        && let Some(value) = literal(ctx, dir)
    {
        match tables::alignment(&value) {
            Some(Some(align)) => {
                let class = match align {
                    crate::ir::Align::Left => "left",
                    crate::ir::Align::Center => "center",
                    crate::ir::Align::Right => "right",
                    crate::ir::Align::Default => "",
                };
                if !class.is_empty() {
                    attrs.classes.push(class.into());
                }
            }
            Some(None) => {}
            None => wrong(ctx, dir.to_untyped(), "an alignment"),
        }
    }
    if matches!(name, "box" | "text") {
        inlines(ctx, expr).map_or(Nodes::Ignored, |inlines| {
            Nodes::Inlines(vec![Inline::Span { attrs, inlines }])
        })
    } else {
        blocks(ctx, expr).map_or(Nodes::Ignored, |blocks| {
            if name == "par" {
                Nodes::Blocks(blocks)
            } else {
                Nodes::Blocks(vec![Block::Div {
                    attrs,
                    blocks,
                    span: Some(ctx.span(args.node)),
                }])
            }
        })
    }
}

fn formatting<'a>(ctx: &mut Context<'a>, args: &Args<'a>, name: &str) -> Nodes {
    args.check_named(ctx, &[], &[]);
    let Some(expr) = required(ctx, args, 0, "body") else {
        return Nodes::Ignored;
    };
    let Some(inlines) = inlines(ctx, expr) else {
        return Nodes::Ignored;
    };
    let inline = match name {
        "strong" => Inline::Strong { inlines },
        "emph" => Inline::Emph { inlines },
        "underline" => Inline::Underline { inlines },
        "strike" => Inline::Strikeout { inlines },
        "smallcaps" => Inline::SmallCaps { inlines },
        "super" => Inline::Superscript { inlines },
        "sub" => Inline::Subscript { inlines },
        _ => Inline::Span {
            attrs: Attrs { classes: vec![name.into()], ..Attrs::default() },
            inlines,
        },
    };
    Nodes::Inlines(vec![inline])
}

fn raw<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &["lang", "block"], &[]);
    let Some(expr) = required(ctx, args, 0, "text") else {
        return Nodes::Ignored;
    };
    let Some(text) = string(ctx, expr) else {
        return Nodes::Ignored;
    };
    let lang = args.named("lang").and_then(|expr| {
        if matches!(ctx.literal(expr), Ok(Literal::None | Literal::Auto)) {
            None
        } else {
            string(ctx, expr)
        }
    });
    if boolean(ctx, args.named("block"), false) {
        Nodes::Blocks(vec![Block::CodeBlock {
            lang,
            text,
            attrs: Attrs::default(),
            span: Some(ctx.span(args.node)),
        }])
    } else {
        Nodes::Inlines(vec![Inline::Code {
            text,
            attrs: Attrs {
                classes: lang.into_iter().collect(),
                ..Attrs::default()
            },
        }])
    }
}

fn heading<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &["level", "numbering", "outlined"], &["bookmarked"]);
    let Some(expr) = required(ctx, args, 0, "body") else {
        return Nodes::Ignored;
    };
    let Some(inlines) = inlines(ctx, expr) else {
        return Nodes::Ignored;
    };
    let depth = unsigned(ctx, args.named("level"), 1, true);
    if depth > 6 {
        ctx.warning(
            "W-TYP-ARG-IGNORED",
            format!("heading depth {depth} is clamped to 6"),
            args.node,
        );
    }
    let mut attrs = Attrs::default();
    unnumbered(ctx, args.named("numbering"), &mut attrs);
    if !boolean(ctx, args.named("outlined"), true) {
        attrs.classes.push("notoc".into());
    }
    Nodes::Blocks(vec![Block::Heading {
        level: depth.min(6) as u8,
        inlines,
        attrs,
        number: None,
        span: Some(ctx.span(args.node)),
    }])
}

fn equation<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &["block", "numbering"], &[]);
    let valid = args.named("block").is_some_and(|expr| {
        matches!(ctx.literal(expr), Ok(Literal::Bool(true)))
    }) && args
        .named("numbering")
        .is_some_and(|expr| matches!(ctx.literal(expr), Ok(Literal::None)));
    let Some(ast::Expr::Equation(equation)) = args.pos(0) else {
        wrong(
            ctx,
            args.node,
            "math.equation(block: true, numbering: none, $...$)",
        );
        return Nodes::Ignored;
    };
    if !valid {
        wrong(
            ctx,
            args.node,
            "math.equation(block: true, numbering: none, $...$)",
        );
        return Nodes::Ignored;
    }
    Nodes::Blocks(vec![Block::Math {
        source: equation.body().to_untyped().full_text().trim().into(),
        dialect: MathDialect::Typst,
        attrs: Attrs { classes: vec!["unnumbered".into()], ..Attrs::default() },
        number: None,
        span: Some(ctx.span(args.node)),
    }])
}

fn link<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &[], &[]);
    let Some(expr) = required(ctx, args, 0, "destination") else {
        return Nodes::Ignored;
    };
    let (destination, default_text) = match literal(ctx, expr) {
        Some(Literal::Str(url)) => (url.clone(), url),
        Some(Literal::Label(label)) => (format!("#{label}"), label),
        Some(_) => {
            wrong(ctx, expr.to_untyped(), "a string or label destination");
            return Nodes::Ignored;
        }
        None => return Nodes::Ignored,
    };
    let inlines = match args.pos(1) {
        Some(expr) => inlines(ctx, expr).unwrap_or_default(),
        None => vec![Inline::Text { text: default_text }],
    };
    Nodes::Inlines(vec![Inline::Link {
        inlines,
        url: destination,
        title: None,
        attrs: Attrs::default(),
    }])
}

fn footnote<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &[], &["numbering"]);
    let Some(expr) = required(ctx, args, 0, "body or label") else {
        return Nodes::Ignored;
    };
    if let Ok(Literal::Label(label)) = ctx.literal(expr) {
        return Nodes::Inlines(vec![Inline::FootnoteRef {
            label: note_label(&label),
            number: None,
        }]);
    }
    blocks(ctx, expr).map_or(Nodes::Ignored, |blocks| {
        Nodes::Inlines(vec![Inline::Footnote {
            blocks,
            label: None,
            number: None,
        }])
    })
}

fn note_label(label: &str) -> String {
    label.strip_prefix("writer-fn-").unwrap_or(label).into()
}

fn reference<'a>(ctx: &mut Context<'a>, args: &Args<'a>, cite: bool) -> Nodes {
    args.check_named(ctx, &["form", "supplement"], &[]);
    let Some(expr) = required(ctx, args, 0, "label") else {
        return Nodes::Ignored;
    };
    let target = match literal(ctx, expr) {
        Some(Literal::Label(target)) => target,
        Some(_) => {
            wrong(ctx, expr.to_untyped(), "a label");
            return Nodes::Ignored;
        }
        None => return Nodes::Ignored,
    };
    let mut form = String::from("normal");
    if let Some(expr) = args.named("form") {
        match literal(ctx, expr) {
            Some(Literal::None) if cite => form = "none".into(),
            Some(Literal::Str(value)) => form = value,
            Some(_) => wrong(
                ctx,
                expr.to_untyped(),
                "a citation/reference form string",
            ),
            None => {}
        }
    }
    let supplement =
        args.named("supplement").and_then(|expr| match ctx.literal(expr) {
            Ok(Literal::None) => {
                if cite {
                    None
                } else {
                    Some(Vec::new())
                }
            }
            Ok(Literal::Auto) => None,
            _ => inlines(ctx, expr),
        });
    let span = Some(ctx.span(args.node));
    if cite {
        let mode = match form.as_str() {
            "normal" => CiteMode::Normal,
            "prose" => CiteMode::Prose,
            "year" => CiteMode::Year,
            "author" => CiteMode::Author,
            "full" => CiteMode::Full,
            "none" => CiteMode::None,
            _ => {
                wrong(
                    ctx,
                    args.node,
                    "normal/prose/year/author/full/none citation form",
                );
                CiteMode::Normal
            }
        };
        Nodes::Inlines(vec![Inline::Cite {
            items: vec![CiteItem {
                key: target,
                locator: supplement.map(|value| plain_text(&value)),
                ..CiteItem::default()
            }],
            mode,
            resolved: None,
            span,
        }])
    } else {
        let form = match form.as_str() {
            "normal" => RefForm::Normal,
            "page" => RefForm::Page,
            _ => {
                wrong(ctx, args.node, "normal or page reference form");
                RefForm::Normal
            }
        };
        Nodes::Inlines(vec![Inline::Ref {
            target,
            supplement,
            form,
            resolved: None,
            span,
        }])
    }
}

fn separator<'a>(ctx: &mut Context<'a>, args: &Args<'a>, name: &str) -> Nodes {
    args.check_named(
        ctx,
        if name == "outline" { &["depth"] } else { &[] },
        &["weak", "to", "length", "title", "indent"],
    );
    let span = Some(ctx.span(args.node));
    let block = match name {
        "pagebreak" => Block::PageBreak { span },
        "line" => Block::ThematicBreak { span },
        _ => {
            let default = match ctx.config.document.toc {
                Toc::Depth(depth) => u64::from(depth),
                Toc::Off => 3,
            };
            let depth = unsigned(ctx, args.named("depth"), default, true);
            let depth = if depth <= 6 {
                depth
            } else {
                wrong(ctx, args.node, "outline depth in 1..=6");
                default
            };
            Block::Toc { depth: depth as u8, span }
        }
    };
    Nodes::Blocks(vec![block])
}

fn callout<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &["kind", "title"], &[]);
    let Some(expr) = required(ctx, args, 0, "body") else {
        return Nodes::Ignored;
    };
    let Some(blocks) = blocks(ctx, expr) else {
        return Nodes::Ignored;
    };
    let kind = args
        .named("kind")
        .and_then(|expr| string(ctx, expr))
        .unwrap_or_else(|| "note".into());
    let title = optional_inlines(ctx, args.named("title"));
    Nodes::Blocks(vec![Block::Callout {
        kind,
        title,
        blocks,
        attrs: Attrs::default(),
        span: Some(ctx.span(args.node)),
    }])
}

fn latex<'a>(ctx: &mut Context<'a>, args: &Args<'a>, display: bool) -> Nodes {
    args.check_named(ctx, if display { &["numbering"] } else { &[] }, &[]);
    let Some(expr) = required(ctx, args, 0, "source") else {
        return Nodes::Ignored;
    };
    let source = match expr {
        ast::Expr::Raw(raw) => raw
            .lines()
            .map(|line| line.get().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => match string(ctx, expr) {
            Some(source) => source,
            None => return Nodes::Ignored,
        },
    };
    let prefix = macros_as_newcommands(&macros_from_config(
        &ctx.config.math.katex_options,
    ));
    let source = source.strip_prefix(&prefix).unwrap_or(&source).to_string();
    if display {
        let mut attrs = Attrs::default();
        unnumbered(ctx, args.named("numbering"), &mut attrs);
        Nodes::Blocks(vec![Block::Math {
            source,
            dialect: MathDialect::Latex,
            attrs,
            number: None,
            span: Some(ctx.span(args.node)),
        }])
    } else {
        Nodes::Inlines(vec![Inline::Math {
            source,
            dialect: MathDialect::Latex,
        }])
    }
}

fn listing<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &["numbered", "start", "highlight", "plain"], &[]);
    let Some(expr) = required(ctx, args, 0, "raw body") else {
        return Nodes::Ignored;
    };
    let Some(mut body) = blocks(ctx, expr) else {
        return Nodes::Ignored;
    };
    let (lang, text) = match body.as_mut_slice() {
        [Block::CodeBlock { lang, text, .. }] => (lang.clone(), text.clone()),
        [Block::Paragraph { inlines, .. }] => match inlines.as_slice() {
            [Inline::Code { text, attrs }] => {
                (attrs.classes.first().cloned(), text.clone())
            }
            _ => {
                wrong(ctx, expr.to_untyped(), "one raw block");
                return Nodes::Ignored;
            }
        },
        _ => {
            wrong(ctx, expr.to_untyped(), "one raw block");
            return Nodes::Ignored;
        }
    };
    let mut attrs = Attrs::default();
    if boolean(ctx, args.named("numbered"), false) {
        attrs.classes.push("numbered".into());
    }
    if boolean(ctx, args.named("plain"), false) {
        attrs.classes.push("plain".into());
    }
    if args.named("start").is_some() {
        attrs.pairs.insert(
            "start".into(),
            unsigned(ctx, args.named("start"), 1, true).to_string(),
        );
    }
    if let Some(expr) = args.named("highlight")
        && let Some(value) = string(ctx, expr)
    {
        attrs.pairs.insert("highlight".into(), value);
    }
    Nodes::Blocks(vec![Block::CodeBlock {
        lang,
        text,
        attrs,
        span: Some(ctx.span(args.node)),
    }])
}

fn code_cell<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &["index", "count", "lang", "input"], &[]);
    let index = unsigned(ctx, args.named("index"), 0, false);
    let index = match usize::try_from(index) {
        Ok(index) => index,
        Err(_) => {
            wrong(ctx, args.node, "a representable cell index");
            0
        }
    };
    let language = args
        .named("lang")
        .and_then(|expr| string(ctx, expr))
        .unwrap_or_default();
    let execution_count =
        args.named("count").and_then(|expr| match literal(ctx, expr) {
            Some(Literal::None) => None,
            Some(Literal::Int(value)) => Some(value),
            Some(_) => {
                wrong(ctx, expr.to_untyped(), "an integer count or none");
                None
            }
            None => None,
        });
    let mut source = String::new();
    let mut show_input = false;
    if let Some(expr) = args.named("input")
        && !matches!(ctx.literal(expr), Ok(Literal::None))
        && let Some(body) = blocks(ctx, expr)
    {
        match body.as_slice() {
            [Block::CodeBlock { text, .. }] => {
                source = text.clone();
                show_input = true;
            }
            [Block::Paragraph { inlines, .. }]
                if matches!(inlines.as_slice(), [Inline::Code { .. }]) =>
            {
                if let Inline::Code { text, .. } = &inlines[0] {
                    source = text.clone();
                    show_input = true;
                }
            }
            _ => wrong(ctx, expr.to_untyped(), "a raw input or none"),
        }
    }
    let outputs = args
        .positional
        .iter()
        .filter_map(|expr| blocks(ctx, *expr))
        .map(|blocks| CellOutput {
            kind: OutputKind::Display,
            execution_count: None,
            candidates: vec![OutputCandidate {
                mime: "text/x-typst".into(),
                blocks,
            }],
            mimes: vec!["text/x-typst".into()],
        })
        .collect::<Vec<_>>();
    Nodes::Blocks(vec![Block::Cell(Cell {
        index,
        language,
        source,
        execution_count,
        show_input,
        show_output: !outputs.is_empty(),
        outputs,
        span: Some(ctx.span(args.node)),
        ..Cell::default()
    })])
}

fn pending_notebook<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> Nodes {
    args.check_named(ctx, &["cells", "tags", "hide-input", "hide-output"], &[]);
    let Some(expr) = required(ctx, args, 0, "file") else {
        return Nodes::Ignored;
    };
    let path = match ctx.literal(expr) {
        Ok(Literal::Path(path)) => path,
        Ok(Literal::Str(path)) => {
            ctx.warning(
                "W-TYP-PATH-STRING",
                "include-notebook requires path(string) in native Typst",
                expr.to_untyped(),
            );
            path
        }
        Ok(_) => {
            wrong(ctx, expr.to_untyped(), "a path string or path(string)");
            return Nodes::Ignored;
        }
        Err(error) => {
            error.report(ctx, expr.to_untyped(), "E-TYP-INCLUDE-DYNAMIC");
            return Nodes::Ignored;
        }
    };
    for name in ["cells", "tags", "hide-input", "hide-output"] {
        let Some(expr) = args.named(name) else {
            continue;
        };
        let Some(value) = literal(ctx, expr) else {
            continue;
        };
        let valid = match name {
            "cells" => match &value {
                Literal::Auto | Literal::Int(_) | Literal::Str(_) => true,
                Literal::Array(items) => items.iter().all(|item| {
                    matches!(item, Literal::Int(_) | Literal::Str(_))
                }),
                _ => false,
            },
            "tags" => match &value {
                Literal::None | Literal::Str(_) => true,
                Literal::Array(items) => {
                    items.iter().all(|item| matches!(item, Literal::Str(_)))
                }
                _ => false,
            },
            _ => matches!(value, Literal::Auto | Literal::Bool(_)),
        };
        if !valid {
            wrong(ctx, expr.to_untyped(), "a literal notebook selection value");
        }
    }
    ctx.pending_notebook(&path, args.node);
    Nodes::Ignored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_body() -> Vec<Block> {
        vec![Block::Paragraph {
            inlines: vec![Inline::Image {
                alt: Vec::new(),
                url: "asset:000000000001".into(),
                title: None,
                attrs: Attrs::default(),
            }],
            span: None,
        }]
    }

    #[test]
    fn figure_inference_requires_the_exact_ir_body_shape() {
        assert_eq!(infer_figure(&image_body()), FigureKind::Image);
        let table = vec![Block::Table {
            columns: Vec::new(),
            head: Vec::new(),
            body: Vec::new(),
            foot: Vec::new(),
            attrs: Attrs::default(),
            span: None,
        }];
        assert_eq!(infer_figure(&table), FigureKind::Table);
        let raw = vec![Block::CodeBlock {
            lang: None,
            text: "source".into(),
            attrs: Attrs::default(),
            span: None,
        }];
        assert_eq!(infer_figure(&raw), FigureKind::Listing);
        let math = vec![Block::Math {
            source: "x".into(),
            dialect: MathDialect::Typst,
            attrs: Attrs::default(),
            number: None,
            span: None,
        }];
        assert_eq!(infer_figure(&math), FigureKind::Other);
        assert!(!figure_shape(FigureKind::Image, &math));
        assert!(!figure_shape(FigureKind::Image, &table));
        let nested = vec![Block::Figure {
            kind: FigureKind::Image,
            caption: None,
            body: image_body(),
            attrs: Attrs::default(),
            number: None,
            span: None,
        }];
        assert!(!figure_shape(FigureKind::Image, &nested));
        assert!(figure_shape(FigureKind::Other, &nested));
    }

    #[test]
    fn footnote_prefix_is_stripped_exactly_once() {
        assert_eq!(note_label("note"), "note");
        assert_eq!(note_label("writer-fn-note"), "note");
        assert_eq!(note_label("writer-fn-writer-fn-note"), "writer-fn-note");
    }
}
