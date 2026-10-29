//! Function-call conversion for the frozen Typst math subset (math.md §3.3).

use typst_syntax::ast::{self, AstNode};
use typst_syntax::{SyntaxKind, SyntaxNode};

use super::symbols::resolve_char;
use super::{Converter, char_command, join_tex, symbol};

const UNKNOWN: &str = "W-TYP-MATH-FN-UNKNOWN";
const IGNORED: &str = "W-TYP-ARG-IGNORED";

#[derive(Clone, Copy)]
struct Positional<'a> {
    expr: ast::Expr<'a>,
    row_end: bool,
}

struct Arguments<'a> {
    positional: Vec<Positional<'a>>,
    named: Vec<ast::Named<'a>>,
    spread: bool,
    malformed: bool,
}

impl<'a> Arguments<'a> {
    fn new(call: ast::MathCall<'a>) -> Self {
        let mut positional = Vec::new();
        let mut named = Vec::new();
        let mut spread = false;
        for item in call.args().arg_items() {
            match item.arg {
                ast::Arg::Pos(expr) => positional
                    .push(Positional { expr, row_end: item.ends_in_semicolon }),
                ast::Arg::Named(value) => {
                    if item.ends_in_semicolon
                        && let Some(previous) = positional.last_mut()
                    {
                        previous.row_end = true;
                    }
                    named.push(value);
                }
                ast::Arg::Spread(_) => spread = true,
            }
        }
        let malformed = call.args().to_untyped().children().any(|node| {
            node.kind() == SyntaxKind::Error && !node.full_text().is_empty()
        });
        Self { positional, named, spread, malformed }
    }

    fn count(&self) -> usize {
        self.positional.len()
    }

    fn value(&self, name: &str) -> Option<ast::Named<'a>> {
        self.named.iter().find(|arg| arg.name().as_str() == name).copied()
    }

    fn ignored(
        &self, ctx: &mut Converter<'a>, callee: &str, supported: &[&str],
    ) {
        for named in &self.named {
            if !supported.contains(&named.name().as_str()) {
                ignored(ctx, callee, *named);
            }
        }
    }
}

/// Parent preflight has already rejected code before this function is entered.
pub(super) fn render_call<'a>(
    ctx: &mut Converter<'a>, call: ast::MathCall<'a>,
) -> String {
    let callee = call.callee().to_untyped().full_text().to_string();
    let args = Arguments::new(call);
    let accent = listed_accent(&callee)
        .or_else(|| resolve_char(&callee).and_then(native_accent));
    let mapped = mapped_arity(&callee, args.count())
        .or_else(|| accent.map(|_| args.count() == 1));
    if args.spread || args.malformed || mapped == Some(false) {
        unknown(
            ctx,
            &callee,
            call.to_untyped(),
            "invalid positional arguments",
        );
        args.ignored(ctx, &callee, &[]);
        return generic(ctx, call, &callee, None);
    }
    if let Some(accent) = accent {
        args.ignored(ctx, &callee, &[]);
        let body = ctx.expr(args.positional[0].expr);
        return accent.apply(&body);
    }
    match callee.as_str() {
        "frac" | "binom" => {
            args.ignored(ctx, &callee, &[]);
            let first = ctx.expr(args.positional[0].expr);
            let second = ctx.expr(args.positional[1].expr);
            format!("\\{callee}{{{first}}}{{{second}}}")
        }
        "sqrt" => unary(ctx, &args, &callee, "\\sqrt"),
        "root" => {
            args.ignored(ctx, &callee, &[]);
            let index = ctx.expr(args.positional[0].expr);
            let body = ctx.expr(args.positional[1].expr);
            format!("\\sqrt[{index}]{{{body}}}")
        }
        "vec" | "mat" | "cases" => environment(ctx, &args, &callee),
        "abs" | "norm" | "floor" | "ceil" | "round" => {
            args.ignored(ctx, &callee, &[]);
            let (left, right) = match callee.as_str() {
                "abs" => ("\\lvert", "\\rvert"),
                "norm" => ("\\lVert", "\\rVert"),
                "floor" => ("\\lfloor", "\\rfloor"),
                "ceil" => ("\\lceil", "\\rceil"),
                _ => ("\\lfloor", "\\rceil"),
            };
            let body = ctx.expr(args.positional[0].expr);
            join_tex(["\\left", left, &body, "\\right", right])
        }
        "lr" => {
            args.ignored(ctx, &callee, &[]);
            // MathDelimited already owns its scalable pair. An ordinary body
            // needs neither a new delimiter pair nor another layer of scaling.
            ctx.expr(args.positional[0].expr)
        }
        "overline" => unary(ctx, &args, &callee, "\\overline"),
        "underline" => unary(ctx, &args, &callee, "\\underline"),
        "overbrace" | "underbrace" | "overbracket" | "underbracket" => {
            args.ignored(ctx, &callee, &[]);
            let body = ctx.expr(args.positional[0].expr);
            let mut output = format!("\\{callee}{{{body}}}");
            if let Some(label) = args.positional.get(1) {
                let label = ctx.expr(label.expr);
                let attachment = if callee.starts_with("under") {
                    '_'
                } else {
                    '^'
                };
                output.push(attachment);
                output.push('{');
                output.push_str(&label);
                output.push('}');
            }
            output
        }
        "bold" => unary(ctx, &args, &callee, "\\mathbf"),
        "upright" => unary(ctx, &args, &callee, "\\mathrm"),
        "italic" => unary(ctx, &args, &callee, "\\mathit"),
        "bb" => unary(ctx, &args, &callee, "\\mathbb"),
        "cal" => unary(ctx, &args, &callee, "\\mathcal"),
        "frak" => unary(ctx, &args, &callee, "\\mathfrak"),
        "sans" => unary(ctx, &args, &callee, "\\mathsf"),
        "mono" => unary(ctx, &args, &callee, "\\mathtt"),
        "scr" => unary(ctx, &args, &callee, "\\mathscr"),
        "op" => {
            args.ignored(ctx, &callee, &["limits"]);
            let limits = match args.value("limits") {
                Some(named) => match scalar(named.expr()) {
                    Some(Scalar::Bool(value)) => value,
                    _ => {
                        ignored(ctx, &callee, named);
                        false
                    }
                },
                None => false,
            };
            let body = match scalar(args.positional[0].expr) {
                Some(Scalar::Text(value)) => ctx.text(&value),
                _ => ctx.expr(args.positional[0].expr),
            };
            let star = if limits { "*" } else { "" };
            format!("\\operatorname{star}{{{body}}}")
        }
        "limits" | "scripts" => {
            args.ignored(ctx, &callee, &[]);
            let body = ctx.expr(args.positional[0].expr);
            let modifier = if callee == "limits" {
                "\\limits"
            } else {
                "\\nolimits"
            };
            if is_operator(&body) {
                join_tex([body.as_str(), modifier])
            } else {
                format!("\\mathop{{{body}}}{modifier}")
            }
        }
        "display" | "inline" | "script" | "sscript" => {
            args.ignored(ctx, &callee, &[]);
            let style = match callee.as_str() {
                "display" => "\\displaystyle",
                "inline" => "\\textstyle",
                "script" => "\\scriptstyle",
                _ => "\\scriptscriptstyle",
            };
            let body = ctx.expr(args.positional[0].expr);
            format!("{{{}}}", join_tex([style, body.as_str()]))
        }
        "cancel" => unary(ctx, &args, &callee, "\\cancel"),
        "text" | "class" | "stretch" | "attach" | "primes" => {
            unknown(ctx, &callee, call.to_untyped(), "approximate wrapper");
            args.ignored(ctx, &callee, &[]);
            let body = if matches!(callee.as_str(), "text" | "class") {
                args.positional.last()
            } else {
                args.positional.first()
            };
            body.map_or_else(String::new, |body| ctx.expr(body.expr))
        }
        "mid" => {
            unknown(
                ctx,
                &callee,
                call.to_untyped(),
                "approximate middle delimiter",
            );
            args.ignored(ctx, &callee, &[]);
            let expr = args.positional[0].expr;
            let body = delimiter_expr(expr).unwrap_or_else(|| ctx.expr(expr));
            join_tex(["\\middle", body.as_str()])
        }
        _ => {
            args.ignored(ctx, &callee, &[]);
            let resolved = symbol(&callee)
                .or_else(|| resolve_char(&callee).and_then(char_command));
            if let Some(symbol) = resolved {
                generic(ctx, call, &callee, Some(symbol.tex))
            } else {
                unknown(ctx, &callee, call.to_untyped(), "unknown function");
                generic(ctx, call, &callee, None)
            }
        }
    }
}

fn mapped_arity(name: &str, count: usize) -> Option<bool> {
    Some(match name {
        "frac" | "binom" | "root" => count == 2,
        "vec" | "mat" | "cases" => count > 0,
        "overbrace" | "underbrace" | "overbracket" | "underbracket" => {
            matches!(count, 1 | 2)
        }
        "text" | "class" | "stretch" | "attach" | "primes" => count > 0,
        "sqrt" | "abs" | "norm" | "floor" | "ceil" | "round" | "lr"
        | "overline" | "underline" | "bold" | "upright" | "italic" | "bb"
        | "cal" | "frak" | "sans" | "mono" | "scr" | "op" | "limits"
        | "scripts" | "display" | "inline" | "script" | "sscript"
        | "cancel" | "mid" => count == 1,
        _ => return None,
    })
}

fn unary<'a>(
    ctx: &mut Converter<'a>, args: &Arguments<'a>, callee: &str, command: &str,
) -> String {
    args.ignored(ctx, callee, &[]);
    let body = ctx.expr(args.positional[0].expr);
    format!("{command}{{{body}}}")
}

fn environment<'a>(
    ctx: &mut Converter<'a>, args: &Arguments<'a>, callee: &str,
) -> String {
    let name = if callee == "cases" {
        args.ignored(ctx, callee, &[]);
        "cases"
    } else {
        args.ignored(ctx, callee, &["delim"]);
        match args.value("delim") {
            None => "pmatrix",
            Some(named) => match scalar(named.expr()) {
                Some(Scalar::None) => "matrix",
                Some(Scalar::Text(value)) => match value.as_str() {
                    "(" => "pmatrix",
                    "[" => "bmatrix",
                    "{" => "Bmatrix",
                    "|" => "vmatrix",
                    "‖" => "Vmatrix",
                    _ => {
                        ignored(ctx, callee, named);
                        "pmatrix"
                    }
                },
                _ => {
                    ignored(ctx, callee, named);
                    "pmatrix"
                }
            },
        }
    };
    let mut rows = Vec::new();
    let mut row = Vec::new();
    for positional in &args.positional {
        row.push(ctx.expr(positional.expr));
        if callee != "mat" || positional.row_end {
            rows.push(row.join(" & "));
            row.clear();
        }
    }
    if !row.is_empty() {
        rows.push(row.join(" & "));
    }
    format!("\\begin{{{name}}}{}\\end{{{name}}}", rows.join(" \\\\ "))
}

enum ArgumentContent<'a> {
    Typed(ast::MathArgItem<'a>),
    Error(&'a SyntaxNode),
}

fn content_node(item: ast::MathArgItem<'_>) -> &SyntaxNode {
    match item {
        ast::MathArgItem::Arg(arg) => arg.to_untyped(),
        ast::MathArgItem::Space(space) => space.to_untyped(),
        ast::MathArgItem::Comma(_, node)
        | ast::MathArgItem::Semicolon(_, node)
        | ast::MathArgItem::LeftParen(_, node)
        | ast::MathArgItem::RightParen(_, node) => node,
    }
}

fn argument_content(args: ast::MathArgs<'_>) -> Vec<ArgumentContent<'_>> {
    let mut typed = args.content_items().peekable();
    let mut content = Vec::new();
    for node in args.to_untyped().children() {
        if node.kind() == SyntaxKind::Error && !node.full_text().is_empty() {
            // The typed iterator omits malformed argument source. Keep it in
            // the same order as the punctuation and surviving typed arguments.
            content.push(ArgumentContent::Error(node));
        } else if typed
            .peek()
            .is_some_and(|item| std::ptr::eq(content_node(*item), node))
            && let Some(item) = typed.next()
        {
            content.push(ArgumentContent::Typed(item));
        }
    }
    content
}

fn generic<'a>(
    ctx: &mut Converter<'a>, call: ast::MathCall<'a>, name: &str,
    resolved: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    let mut separator = String::new();
    let mut seen = false;
    for item in argument_content(call.args()) {
        let item = match item {
            ArgumentContent::Typed(item) => item,
            ArgumentContent::Error(node) => {
                if seen {
                    parts.push(std::mem::take(&mut separator));
                }
                parts.push(ctx.recover(node));
                seen = true;
                continue;
            }
        };
        match item {
            ast::MathArgItem::Arg(ast::Arg::Pos(expr)) => {
                if seen {
                    parts.push(std::mem::take(&mut separator));
                }
                parts.push(ctx.expr(expr));
                seen = true;
            }
            ast::MathArgItem::Arg(ast::Arg::Spread(spread)) => {
                if seen {
                    parts.push(std::mem::take(&mut separator));
                }
                parts.push(format!("..{}", ctx.expr(spread.expr())));
                seen = true;
            }
            ast::MathArgItem::Arg(ast::Arg::Named(_)) => separator.clear(),
            ast::MathArgItem::Space(_) if seen => separator.push(' '),
            ast::MathArgItem::Comma(value, _)
            | ast::MathArgItem::Semicolon(value, _)
                if seen =>
            {
                separator.push(value)
            }
            _ => {}
        }
    }
    let body = join_tex(parts.iter().map(String::as_str));
    if let Some(symbol) = resolved {
        join_tex([symbol, "\\left(", &body, "\\right)"])
    } else {
        let name = ctx.text(name);
        format!("\\operatorname{{{name}}}({body})")
    }
}

enum Scalar {
    None,
    Bool(bool),
    Text(String),
}

fn scalar(expr: ast::Expr<'_>) -> Option<Scalar> {
    match expr {
        ast::Expr::None(_) => Some(Scalar::None),
        ast::Expr::Bool(value) => Some(Scalar::Bool(value.get())),
        ast::Expr::Str(value) => Some(Scalar::Text(value.get().to_string())),
        ast::Expr::Math(math) => {
            let mut values = math
                .exprs()
                .filter(|expr| !matches!(expr, ast::Expr::Space(_)));
            let value = values.next()?;
            values.next().is_none().then(|| scalar(value)).flatten()
        }
        _ => None,
    }
}

fn delimiter_expr(expr: ast::Expr<'_>) -> Option<String> {
    let value = match scalar(expr) {
        Some(Scalar::None) => return Some(".".into()),
        Some(Scalar::Text(value)) => value,
        _ => match expr {
            ast::Expr::MathShorthand(value) => value.get().to_string(),
            ast::Expr::MathText(value) => {
                value.to_untyped().full_text().to_string()
            }
            ast::Expr::MathIdent(_) | ast::Expr::MathFieldAccess(_) => {
                resolve_char(&expr.to_untyped().full_text())?.to_string()
            }
            _ => return None,
        },
    };
    Some(match value.as_str() {
        "(" | ")" | "[" | "]" | "|" | "." => value,
        "{" => "\\{".into(),
        "}" => "\\}".into(),
        "‖" => "\\|".into(),
        "<" | "⟨" => "\\langle".into(),
        ">" | "⟩" => "\\rangle".into(),
        "⌊" => "\\lfloor".into(),
        "⌋" => "\\rfloor".into(),
        "⌈" => "\\lceil".into(),
        "⌉" => "\\rceil".into(),
        _ => return None,
    })
}

fn ignored<'a>(ctx: &mut Converter<'a>, callee: &str, arg: ast::Named<'a>) {
    ctx.warning(
        IGNORED,
        format!(
            "{callee}: named argument `{}` is ignored",
            arg.name().as_str()
        ),
        arg.to_untyped(),
    );
}

fn unknown(
    ctx: &mut Converter<'_>, callee: &str, node: &SyntaxNode, reason: &str,
) {
    ctx.warning(
        UNKNOWN,
        format!("{callee}: {reason}; used an approximation"),
        node,
    );
}

#[derive(Clone, Copy)]
enum Accent {
    Command(&'static str),
    DoubleAcute,
    LeftHarpoon,
    RightHarpoon,
}

impl Accent {
    fn apply(self, body: &str) -> String {
        match self {
            Self::Command(command) => format!("{command}{{{body}}}"),
            // Frozen coordinator choices: valid amsmath fixed-width forms.
            // All three pass isolated pdflatex with amsmath/amssymb only.
            Self::DoubleAcute => {
                format!("\\overset{{\\text{{\\H{{}}}}}}{{{body}}}")
            }
            Self::LeftHarpoon => {
                format!("\\overset{{\\leftharpoonup}}{{{body}}}")
            }
            Self::RightHarpoon => {
                format!("\\overset{{\\rightharpoonup}}{{{body}}}")
            }
        }
    }
}

fn listed_accent(name: &str) -> Option<Accent> {
    Some(Accent::Command(match name {
        "hat" => "\\hat",
        "tilde" => "\\tilde",
        "grave" => "\\grave",
        "acute" => "\\acute",
        "breve" => "\\breve",
        "caron" => "\\check",
        "macron" => "\\bar",
        "diaer" | "dot.double" => "\\ddot",
        "dot" => "\\dot",
        "arrow" => "\\vec",
        "arrow.l" => "\\overleftarrow",
        "arrow.l.r" => "\\overleftrightarrow",
        "circle" => "\\mathring",
        _ => return None,
    }))
}

fn native_accent(value: char) -> Option<Accent> {
    Some(match value {
        '\u{0300}' | '`' => Accent::Command("\\grave"),
        '\u{0301}' | '´' => Accent::Command("\\acute"),
        '\u{0302}' | '^' | 'ˆ' => Accent::Command("\\hat"),
        '\u{0303}' | '~' | '∼' | '˜' => Accent::Command("\\tilde"),
        '\u{0304}' | '¯' => Accent::Command("\\bar"),
        '\u{0305}' | '-' | '–' | '‾' | '−' => {
            Accent::Command("\\overline")
        }
        '\u{0306}' | '˘' => Accent::Command("\\breve"),
        '\u{0307}' | '.' | '˙' | '⋅' => Accent::Command("\\dot"),
        '\u{0308}' | '¨' => Accent::Command("\\ddot"),
        '\u{20db}' => Accent::Command("\\dddot"),
        '\u{20dc}' => Accent::Command("\\ddddot"),
        '\u{030a}' | '∘' | '○' => Accent::Command("\\mathring"),
        '\u{030b}' | '˝' => Accent::DoubleAcute,
        '\u{030c}' | 'ˇ' => Accent::Command("\\check"),
        '\u{20d6}' | '←' => Accent::Command("\\overleftarrow"),
        '\u{20d7}' | '→' | '⟶' => Accent::Command("\\vec"),
        '\u{20e1}' | '↔' | '⟷' => Accent::Command("\\overleftrightarrow"),
        '\u{20d0}' | '↼' => Accent::LeftHarpoon,
        '\u{20d1}' | '⇀' => Accent::RightHarpoon,
        _ => return None,
    })
}

fn is_operator(body: &str) -> bool {
    let body = body.trim();
    if matches!(
        body,
        "\\sum"
            | "\\prod"
            | "\\coprod"
            | "\\int"
            | "\\oint"
            | "\\iint"
            | "\\iiint"
            | "\\iiiint"
            | "\\idotsint"
            | "\\smallint"
            | "\\oiint"
            | "\\oiiint"
            | "\\bigcup"
            | "\\bigcap"
            | "\\biguplus"
            | "\\bigvee"
            | "\\bigwedge"
            | "\\bigoplus"
            | "\\bigotimes"
            | "\\bigodot"
            | "\\bigsqcup"
            | "\\lim"
            | "\\liminf"
            | "\\limsup"
            | "\\max"
            | "\\min"
            | "\\sup"
            | "\\inf"
            | "\\det"
            | "\\gcd"
            | "\\Pr"
            | "\\arccos"
            | "\\arcsin"
            | "\\arctan"
            | "\\arg"
            | "\\cos"
            | "\\cosh"
            | "\\cot"
            | "\\coth"
            | "\\csc"
            | "\\deg"
            | "\\dim"
            | "\\exp"
            | "\\hom"
            | "\\ker"
            | "\\lg"
            | "\\ln"
            | "\\log"
            | "\\sec"
            | "\\sin"
            | "\\sinh"
            | "\\tan"
            | "\\tanh"
    ) {
        return true;
    }
    let Some(mut rest) = body.strip_prefix("\\operatorname") else {
        return false;
    };
    if let Some(value) = rest.strip_prefix('*') {
        rest = value;
    }
    if !rest.starts_with('{') {
        return false;
    }
    let mut depth = 0usize;
    let mut escaped = false;
    for (index, character) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index + character.len_utf8() == rest.len();
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::convert;
    use super::*;

    #[test]
    fn matrix_semicolons_and_named_delimiters_use_typed_arguments() {
        for (source, expected) in [
            ("mat(a,b;c,d)", "\\begin{pmatrix}a & b \\\\ c & d\\end{pmatrix}"),
            (
                "mat(delim: \"[\",a,b;c,d)",
                "\\begin{bmatrix}a & b \\\\ c & d\\end{bmatrix}",
            ),
            (
                "mat(a,b,delim: \"[\";c,d)",
                "\\begin{bmatrix}a & b \\\\ c & d\\end{bmatrix}",
            ),
            ("vec(delim: #none,1,2)", "\\begin{matrix}1 \\\\ 2\\end{matrix}"),
        ] {
            let (output, diagnostics) = convert(source);
            assert_eq!(output, expected, "{source}");
            assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn invalid_arity_and_ignored_named_values_keep_visible_arguments() {
        let (output, diagnostics) = convert("frac(x)");
        assert_eq!(output, "\\operatorname{frac}(x)");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, UNKNOWN);
        let (_, diagnostics) = convert("sqrt(size: #12pt,x)");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, IGNORED);
        assert!(diagnostics[0].message.contains("size"));
    }

    #[test]
    fn op_and_limit_modifiers_remain_valid_tex_math_operators() {
        for (source, expected) in [
            ("op(\"argmax\",limits: #true)", "\\operatorname*{argmax}"),
            ("op(\"argmax\",limits: #false)", "\\operatorname{argmax}"),
            ("limits(sum)", "\\sum\\limits"),
            ("limits(arrow.r)", "\\mathop{\\rightarrow}\\limits"),
        ] {
            let (output, diagnostics) = convert(source);
            assert_eq!(output, expected, "{source}");
            assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
        }
        assert!(!is_operator("\\sum+x"));
        assert!(!is_operator("\\operatorname{x}(y)"));
    }

    #[test]
    fn audited_native_accents_have_valid_amsmath_compositions() {
        for (character, expected) in [
            ('\u{20db}', "\\dddot{x}"),
            ('\u{20dc}', "\\ddddot{x}"),
            ('\u{030b}', "\\overset{\\text{\\H{}}}{x}"),
            ('\u{20d0}', "\\overset{\\leftharpoonup}{x}"),
            ('\u{20d1}', "\\overset{\\rightharpoonup}{x}"),
        ] {
            assert_eq!(native_accent(character).unwrap().apply("x"), expected);
        }
    }

    #[test]
    fn native_accent_field_aliases_use_the_audited_forms() {
        for (source, expected) in [
            ("dot.triple(x)", "\\dddot{x}"),
            ("dot.quad(x)", "\\ddddot{x}"),
            ("acute.double(x)", "\\overset{\\text{\\H{}}}{x}"),
            ("harpoon.lt(x)", "\\overset{\\leftharpoonup}{x}"),
            ("harpoon.rt(x)", "\\overset{\\rightharpoonup}{x}"),
        ] {
            let (output, diagnostics) = convert(source);
            assert_eq!(output, expected, "{source}");
            assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn a_literal_math_spread_keeps_its_content_in_generic_recovery() {
        let (output, diagnostics) = convert("frac(..x,y)");
        assert_eq!(output, "\\operatorname{frac}(..x,y)");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, UNKNOWN);
    }

    #[test]
    fn malformed_call_arguments_keep_source_without_duplicate_findings() {
        for source in ["sqrt(\"unterminated)", "sqrt(x,\"unterminated)"] {
            let (output, diagnostics) = convert(source);
            assert!(output.starts_with("\\operatorname{sqrt}("), "{output}");
            assert!(output.contains("unterminated"), "{output}");
            assert!(output.contains("\\text{"), "{output}");
            assert_eq!(
                diagnostics.iter().filter(|d| d.code == UNKNOWN).count(),
                1
            );
            assert!(diagnostics.iter().any(|d| {
                d.code == "W-TYP-MATH-SYM-UNKNOWN"
                    && d.message.contains("malformed math")
            }));
            assert!(
                !diagnostics.iter().any(|d| {
                    d.message
                        .contains("unsupported math syntax retained as text")
                        || d.code == "E-TYP-MATH-CODE"
                }),
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn generic_functions_preserve_source_argument_punctuation() {
        let (output, diagnostics) = convert("unknownfn(x, y; z)");
        assert_eq!(output, "\\operatorname{unknownfn}(x, y; z)");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, UNKNOWN);
    }
}
