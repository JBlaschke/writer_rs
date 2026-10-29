//! Pure conversion of Typst math syntax to a LaTeX equation body.

mod calls;
mod symbols;

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use typst_syntax::ast::{self, AstNode};
use typst_syntax::{SyntaxKind, SyntaxNode};

use crate::error::Diagnostic;

pub use symbols::{Sym, char_command, command_package, symbol};

const UNKNOWN: &str = "W-TYP-MATH-SYM-UNKNOWN";

/// Converts one equation body without evaluating code or invoking a compiler.
pub fn convert(source: &str) -> (String, Vec<Diagnostic>) {
    let syntax = typst_syntax::parse_math(source);
    convert_ast(syntax.cast().expect("parse_math has a Math root"))
}

/// Converts an existing math subtree with equation-relative byte locations.
pub fn convert_ast(math: ast::Math<'_>) -> (String, Vec<Diagnostic>) {
    let mut converter = Converter::new(math);
    if let Some(range) = converter.code_range() {
        let output = format!(
            "\\text{{[typst math: {}]}}",
            converter.text(&converter.source),
        );
        converter.diagnostics.push(Diagnostic::warning(
            "E-TYP-MATH-CODE",
            format!(
                "embedded code is outside convertible math at bytes {}..{} of the equation",
                range.start, range.end,
            ),
        ));
        return (output, converter.diagnostics);
    }
    converter.syntax_findings();
    let body = converter.math(math);
    let body = if converter.aligned {
        format!("\\begin{{aligned}}{body}\\end{{aligned}}")
    } else {
        body
    };
    (body, converter.diagnostics)
}

struct Converter<'a> {
    source: String,
    nodes: Vec<&'a SyntaxNode>,
    ranges: HashMap<usize, Range<usize>>,
    reported_errors: HashSet<usize>,
    diagnostics: Vec<Diagnostic>,
    aligned: bool,
}

impl<'a> Converter<'a> {
    fn new(math: ast::Math<'a>) -> Self {
        let mut converter = Self {
            source: math.to_untyped().full_text().to_string(),
            nodes: Vec::new(),
            ranges: HashMap::new(),
            reported_errors: HashSet::new(),
            diagnostics: Vec::new(),
            aligned: false,
        };
        index_nodes(
            math.to_untyped(),
            0,
            &mut converter.nodes,
            &mut converter.ranges,
        );
        converter
    }

    fn range(&self, node: &SyntaxNode) -> Range<usize> {
        self.ranges.get(&identity(node)).cloned().unwrap_or(0..0)
    }

    fn warning(
        &mut self, code: &'static str, message: impl Into<String>,
        node: &SyntaxNode,
    ) {
        let range = self.range(node);
        self.diagnostics.push(Diagnostic::warning(
            code,
            format!(
                "{} at bytes {}..{} of the equation",
                message.into(),
                range.start,
                range.end,
            ),
        ));
    }

    fn code_range(&self) -> Option<Range<usize>> {
        let mut allowed = HashSet::new();
        for node in &self.nodes {
            let Some(named) = node.cast::<ast::Named>() else {
                continue;
            };
            if !literal_named_value(named.expr())
                || !self
                    .ranges
                    .contains_key(&identity(named.expr().to_untyped()))
            {
                continue;
            }
            let mut hashes = Vec::new();
            hash_nodes(node, &mut hashes);
            if let [hash] = hashes.as_slice() {
                allowed.insert(identity(hash));
            }
        }
        self.nodes.iter().find_map(|node| {
            (node.kind() == SyntaxKind::Hash
                && !allowed.contains(&identity(node)))
            .then(|| self.range(node))
        })
    }

    fn syntax_findings(&mut self) {
        for node in self.nodes.clone() {
            if node.kind() == SyntaxKind::Error {
                self.reported_errors.insert(identity(node));
                self.warning(
                    UNKNOWN,
                    "malformed math retained where possible",
                    node,
                );
            }
        }
    }

    fn math(&mut self, math: ast::Math<'a>) -> String {
        let mut pieces = Vec::new();
        for node in math.to_untyped().children() {
            if let Some(space) = node.cast::<ast::Space>() {
                pieces.push(self.expr(ast::Expr::Space(space)));
            } else if let Some(expr) = node.cast::<ast::Expr>() {
                pieces.push(self.expr(expr));
            } else if node.kind() == SyntaxKind::Error {
                pieces.push(self.recover(node));
            }
        }
        join_tex(pieces.iter().map(String::as_str))
    }

    fn expr(&mut self, expr: ast::Expr<'a>) -> String {
        let node = expr.to_untyped();
        match expr {
            ast::Expr::Math(math) => self.math(math),
            ast::Expr::MathText(text) => match text.get() {
                ast::MathTextKind::Number(text) => {
                    let pieces: Vec<_> = text
                        .chars()
                        .map(|value| self.character(value, node))
                        .collect();
                    join_tex(pieces.iter().map(String::as_str))
                }
                ast::MathTextKind::Grapheme(text) => self.grapheme(text, node),
            },
            ast::Expr::MathIdent(_) | ast::Expr::MathFieldAccess(_) => {
                self.name(&node.full_text(), node)
            }
            ast::Expr::MathShorthand(shorthand) => {
                self.character(shorthand.get(), node)
            }
            ast::Expr::Escape(escape) => self.character(escape.get(), node),
            ast::Expr::Space(_) => " ".into(),
            ast::Expr::Str(value) => {
                format!("\\text{{{}}}", self.text(&value.get()))
            }
            ast::Expr::Linebreak(_) => {
                self.aligned = true;
                "\\\\".into()
            }
            ast::Expr::MathAlignPoint(_) => {
                self.aligned = true;
                "&".into()
            }
            ast::Expr::MathAttach(attach) => {
                let mut output = self.expr(attach.base());
                if let Some(bottom) = attach.bottom() {
                    output.push_str(&format!("_{{{}}}", self.expr(bottom)));
                }
                let primes =
                    "'".repeat(attach.primes().map_or(0, |p| p.count()));
                if let Some(top) = attach.top() {
                    output
                        .push_str(&format!("^{{{primes}{}}}", self.expr(top)));
                } else {
                    output.push_str(&primes);
                }
                output
            }
            ast::Expr::MathPrimes(primes) => "'".repeat(primes.count()),
            ast::Expr::MathFrac(frac) => format!(
                "\\frac{{{}}}{{{}}}",
                self.expr(frac.num()),
                self.expr(frac.denom()),
            ),
            ast::Expr::MathRoot(root) => {
                let index = root
                    .index()
                    .map(|value| format!("[{value}]"))
                    .unwrap_or_default();
                format!("\\sqrt{index}{{{}}}", self.expr(root.radicand()))
            }
            ast::Expr::MathDelimited(delimited) => {
                let open = self.delimiter(delimited.open(), true);
                let body = self.math(delimited.body());
                let close = self.delimiter(delimited.close(), false);
                let left = format!("\\left{open}");
                let right = format!("\\right{close}");
                join_tex([left.as_str(), body.as_str(), right.as_str()])
            }
            ast::Expr::MathCall(call) => calls::render_call(self, call),
            _ => self.recover(node),
        }
    }

    fn grapheme(&mut self, value: &str, node: &SyntaxNode) -> String {
        let mut chars = value.chars();
        if let Some(character) = chars.next()
            && chars.next().is_none()
        {
            return self.character(character, node);
        }
        self.warning(
            UNKNOWN,
            format!("unmapped math grapheme `{value}`"),
            node,
        );
        value.into()
    }

    fn character(&mut self, value: char, node: &SyntaxNode) -> String {
        if let Some(symbol) = char_command(value) {
            return symbol.tex.into();
        }
        if value.is_ascii() {
            return self.text(&value.to_string());
        }
        self.warning(
            UNKNOWN,
            format!("unmapped math character `{value}`"),
            node,
        );
        value.to_string()
    }

    fn name(&mut self, name: &str, node: &SyntaxNode) -> String {
        if let Some(symbol) = symbol(name) {
            return symbol.tex.into();
        }
        if let Some(value) = symbols::resolve_char(name)
            && let Some(symbol) = char_command(value)
        {
            return symbol.tex.into();
        }
        if let Some(operator) = operator(name) {
            return operator;
        }
        let known = match name {
            "dif" => Some("\\mathrm{d}"),
            "Dif" => Some("\\mathrm{D}"),
            "thin" => Some("\\,"),
            "med" => Some("\\:"),
            "thick" => Some("\\;"),
            "quad" => Some("\\quad"),
            "wide" => Some("\\qquad"),
            _ => None,
        };
        if let Some(value) = known {
            return value.into();
        }
        if name.len() == 1 && name.as_bytes()[0].is_ascii_alphabetic() {
            return name.into();
        }
        self.warning(UNKNOWN, format!("unknown math symbol `{name}`"), node);
        format!("\\mathit{{{}}}", self.text(name))
    }

    fn delimiter(&mut self, expr: ast::Expr<'a>, opening: bool) -> String {
        let node = expr.to_untyped();
        let value = match expr {
            ast::Expr::MathShorthand(value) => value.get().to_string(),
            ast::Expr::Escape(value) => value.get().to_string(),
            ast::Expr::MathIdent(_) | ast::Expr::MathFieldAccess(_) => {
                symbols::resolve_char(&node.full_text())
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| node.full_text().to_string())
            }
            _ => node.full_text().to_string(),
        };
        match value.as_str() {
            "(" | ")" | "[" | "]" | "|" | "." => value,
            "{" => "\\{".into(),
            "}" => "\\}".into(),
            "‖" => "\\|".into(),
            "⟨" => "\\langle".into(),
            "⟩" => "\\rangle".into(),
            "⌊" => "\\lfloor".into(),
            "⌋" => "\\rfloor".into(),
            "⌈" => "\\lceil".into(),
            "⌉" => "\\rceil".into(),
            "⟦" => "\\llbracket".into(),
            "⟧" => "\\rrbracket".into(),
            "" => ".".into(),
            _ => {
                self.warning(
                    UNKNOWN,
                    format!(
                        "unsupported {} delimiter `{value}`",
                        if opening { "opening" } else { "closing" }
                    ),
                    node,
                );
                ".".into()
            }
        }
    }

    fn recover(&mut self, node: &SyntaxNode) -> String {
        if !self.reported_errors.contains(&identity(node)) {
            self.warning(
                UNKNOWN,
                "unsupported math syntax retained as text",
                node,
            );
        }
        format!("\\text{{{}}}", self.text(&node.full_text()))
    }

    fn text(&self, value: &str) -> String {
        let mut output = String::new();
        for character in value.chars() {
            output.push_str(match character {
                '\\' => "\\textbackslash{}",
                '{' => "\\{",
                '}' => "\\}",
                '$' => "\\$",
                '&' => "\\&",
                '#' => "\\#",
                '%' => "\\%",
                '_' => "\\_",
                '^' => "\\textasciicircum{}",
                '~' => "\\textasciitilde{}",
                _ => {
                    output.push(character);
                    continue;
                }
            });
        }
        output
    }
}

fn literal_named_value(expr: ast::Expr<'_>) -> bool {
    match expr {
        ast::Expr::Bool(_)
        | ast::Expr::None(_)
        | ast::Expr::Int(_)
        | ast::Expr::Float(_)
        | ast::Expr::Numeric(_) => true,
        ast::Expr::Unary(value) if value.op() == ast::UnOp::Neg => {
            matches!(
                value.expr(),
                ast::Expr::Int(_) | ast::Expr::Float(_) | ast::Expr::Numeric(_)
            )
        }
        ast::Expr::Parenthesized(value) => {
            matches!(value.expr(), ast::Expr::Unary(_))
                && literal_named_value(value.expr())
        }
        ast::Expr::Math(math) => {
            let mut values = math
                .exprs()
                .filter(|expr| !matches!(expr, ast::Expr::Space(_)));
            values.next().is_some_and(literal_named_value)
                && values.next().is_none()
        }
        _ => false,
    }
}

fn identity(node: &SyntaxNode) -> usize {
    std::ptr::from_ref(node) as usize
}

fn hash_nodes<'a>(node: &'a SyntaxNode, hashes: &mut Vec<&'a SyntaxNode>) {
    if node.kind() == SyntaxKind::Hash {
        hashes.push(node);
    }
    for child in node.children() {
        hash_nodes(child, hashes);
    }
}

fn index_nodes<'a>(
    node: &'a SyntaxNode, start: usize, nodes: &mut Vec<&'a SyntaxNode>,
    ranges: &mut HashMap<usize, Range<usize>>,
) {
    nodes.push(node);
    ranges.insert(identity(node), start..start + node.len());
    let mut offset = start;
    for child in node.children() {
        index_nodes(child, offset, nodes, ranges);
        offset += child.len();
    }
}

/// Prevent adjoining source tokens from extending a TeX control word.
fn join_tex<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut output = String::new();
    for part in parts {
        if part.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && ends_control_word(&output)
        {
            output.push_str("{}");
        }
        output.push_str(part);
    }
    output
}

fn ends_control_word(text: &str) -> bool {
    let bytes = text.as_bytes();
    let letters = bytes
        .iter()
        .rev()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    if letters == 0 {
        return false;
    }
    bytes[..bytes.len() - letters]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count()
        % 2
        == 1
}

fn operator(name: &str) -> Option<String> {
    const STANDARD: &[&str] = &[
        "arccos", "arcsin", "arctan", "arg", "cos", "cosh", "cot", "coth",
        "csc", "deg", "det", "dim", "exp", "gcd", "hom", "inf", "ker", "lg",
        "lim", "liminf", "limsup", "ln", "log", "max", "min", "Pr", "sec",
        "sin", "sinh", "sup", "tan", "tanh",
    ];
    const OTHER: &[&str] =
        &["csch", "ctg", "id", "im", "lcm", "mod", "sech", "sinc", "tg", "tr"];
    if STANDARD.contains(&name) {
        Some(format!("\\{name}"))
    } else if OTHER.contains(&name) {
        Some(format!("\\operatorname{{{name}}}"))
    } else {
        None
    }
}
