//! Literal values, lexical bindings and raw call arguments for the subset walker.

use std::collections::BTreeMap;
use std::rc::Rc;

use typst_syntax::SyntaxNode;
use typst_syntax::ast::{self, AstNode};

use super::Context;

#[derive(Clone)]
pub(super) struct ContentValue<'a> {
    pub block: ast::ContentBlock<'a>,
    pub env: Rc<Env<'a>>,
}

#[derive(Clone)]
pub(super) enum Literal<'a> {
    None,
    Auto,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Numeric {
        value: f64,
        unit: ast::Unit,
        text: String,
    },
    Label(String),
    Path(String),
    Date(String),
    Symbol(String),
    Content(ContentValue<'a>),
    Array(Vec<Literal<'a>>),
    Dict(BTreeMap<String, Literal<'a>>),
}

#[derive(Clone)]
pub(super) enum Binding<'a> {
    Literal(Literal<'a>),
    Callable(String),
    Module(Module),
    ImportedUnknown(String),
    Unknown,
}

#[derive(Clone, Copy)]
pub(super) enum Module {
    Writer,
    Mitex,
    External,
}

#[derive(Clone, Default)]
struct Scope<'a> {
    names: BTreeMap<String, Binding<'a>>,
    unknown_wildcard: bool,
}

#[derive(Clone, Default)]
pub(super) struct Env<'a> {
    scopes: Vec<Scope<'a>>,
}

impl<'a> Env<'a> {
    pub(super) fn push(&mut self) {
        self.scopes.push(Scope::default());
    }

    pub(super) fn pop(&mut self) {
        self.scopes.pop();
    }

    pub(super) fn bind(&mut self, name: String, value: Binding<'a>) {
        if self.scopes.is_empty() {
            self.push();
        }
        self.scopes.last_mut().expect("scope exists").names.insert(name, value);
    }

    pub(super) fn get(&self, name: &str) -> Option<&Binding<'a>> {
        self.lookup(name).0
    }

    fn lookup(&self, name: &str) -> (Option<&Binding<'a>>, bool) {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.names.get(name) {
                return (Some(binding), false);
            }
            if scope.unknown_wildcard {
                return (None, true);
            }
        }
        (None, false)
    }

    fn unknown_wildcard(&mut self) {
        if self.scopes.is_empty() {
            self.push();
        }
        let scope = self.scopes.last_mut().expect("scope exists");
        // Unknown exports may shadow earlier local names as well as builtins.
        scope.names.clear();
        scope.unknown_wildcard = true;
    }

    pub(super) fn resolve_callee(&self, expr: ast::Expr<'a>) -> Option<String> {
        match expr {
            ast::Expr::Ident(ident) => {
                let name = ident.as_str();
                match self.lookup(name) {
                    (Some(Binding::Callable(name)), _) => Some(name.clone()),
                    (None, false) if native(name) => Some(name.into()),
                    _ => None,
                }
            }
            ast::Expr::FieldAccess(access) => {
                let field = access.field().as_str();
                if let ast::Expr::Ident(ident) = access.target() {
                    match self.lookup(ident.as_str()) {
                        (Some(Binding::Module(module)), _)
                            if exported(*module, field) =>
                        {
                            return Some(field.into());
                        }
                        (Some(_), _) | (None, true) => return None,
                        _ => {}
                    }
                }
                let base = self.resolve_callee(access.target())?;
                let name = format!("{base}.{field}");
                native(&name).then_some(name)
            }
            _ => None,
        }
    }

    pub(super) fn writer_doc(&self, expr: ast::Expr<'a>) -> bool {
        if let ast::Expr::FieldAccess(access) = expr
            && access.field().as_str() == "with"
        {
            return self.writer_doc(access.target());
        }
        match expr {
            ast::Expr::Ident(ident) => match self.lookup(ident.as_str()) {
                (
                    Some(
                        Binding::Callable(name)
                        | Binding::ImportedUnknown(name),
                    ),
                    _,
                ) => name == "writer-doc",
                (None, _) => ident.as_str() == "writer-doc",
                _ => false,
            },
            ast::Expr::FieldAccess(access)
                if access.field().as_str() == "writer-doc" =>
            {
                matches!(access.target(), ast::Expr::Ident(ident)
                    if matches!(self.get(ident.as_str()), Some(Binding::Module(Module::Writer | Module::External))))
            }
            _ => false,
        }
    }
}

fn native(name: &str) -> bool {
    matches!(
        name,
        "document"
            | "page"
            | "figure.caption"
            | "footnote.entry"
            | "figure"
            | "image"
            | "table"
            | "table.header"
            | "table.footer"
            | "table.cell"
            | "table.hline"
            | "table.vline"
            | "quote"
            | "list"
            | "enum"
            | "terms"
            | "terms.item"
            | "align"
            | "block"
            | "par"
            | "box"
            | "text"
            | "strong"
            | "emph"
            | "underline"
            | "strike"
            | "smallcaps"
            | "super"
            | "sub"
            | "overline"
            | "highlight"
            | "raw"
            | "heading"
            | "math"
            | "math.equation"
            | "metadata"
            | "link"
            | "footnote"
            | "cite"
            | "ref"
            | "pagebreak"
            | "colbreak"
            | "line"
            | "outline"
            | "linebreak"
            | "bibliography"
            | "callout"
            | "mi"
            | "mitex"
            | "listing"
            | "code-cell"
            | "include-notebook"
            | "v"
            | "h"
            | "path"
            | "label"
            | "datetime"
            | "writer-doc"
            | "writer-doc.with"
    )
}

fn exported(module: Module, name: &str) -> bool {
    match module {
        Module::Writer => matches!(
            name,
            "writer-doc"
                | "callout"
                | "listing"
                | "code-cell"
                | "include-notebook"
        ),
        Module::Mitex => matches!(name, "mi" | "mitex"),
        Module::External => false,
    }
}

pub(super) enum LiteralError {
    NonLiteral(String),
    Invalid(String),
    InvalidLabel(String),
    Spread(String),
}

impl LiteralError {
    pub(super) fn report(
        self, ctx: &mut Context<'_>, node: &SyntaxNode,
        nonliteral_code: &'static str,
    ) {
        let (code, message) = match self {
            Self::NonLiteral(message) => (nonliteral_code, message),
            Self::Invalid(message) => ("E-TYP-ARG-INVALID", message),
            Self::InvalidLabel(message) => ("label-invalid", message),
            Self::Spread(message) => ("E-TYP-ARG-SPREAD", message),
        };
        ctx.error(code, message, node);
    }
}

pub(super) fn argument<'a>(
    expr: ast::Expr<'a>, env: &Env<'a>,
) -> Result<Literal<'a>, LiteralError> {
    decode(expr, env, true)
}

/// Bound each decoded value before cloning captured arrays or dictionaries.
/// Content captures share their environments and have a separate replay limit.
#[derive(Default)]
struct DecoderFuel {
    nodes: usize,
    string_bytes: usize,
}

impl DecoderFuel {
    fn limit() -> LiteralError {
        LiteralError::Invalid("literal value limit exceeded".into())
    }

    fn node(&mut self, depth: usize) -> Result<(), LiteralError> {
        if depth > 64 || self.nodes >= 65_536 {
            return Err(Self::limit());
        }
        self.nodes += 1;
        Ok(())
    }

    fn string(&mut self, bytes: usize) -> Result<(), LiteralError> {
        if bytes > (8 * 1024 * 1024) - self.string_bytes {
            return Err(Self::limit());
        }
        self.string_bytes += bytes;
        Ok(())
    }

    fn reference(
        &mut self, value: &Literal<'_>, depth: usize,
    ) -> Result<(), LiteralError> {
        self.node(depth)?;
        match value {
            Literal::Str(text)
            | Literal::Label(text)
            | Literal::Path(text)
            | Literal::Date(text)
            | Literal::Symbol(text)
            | Literal::Numeric { text, .. } => self.string(text.len())?,
            Literal::Array(items) => {
                for item in items {
                    self.reference(item, depth + 1)?;
                }
            }
            Literal::Dict(items) => {
                for (key, item) in items {
                    self.string(key.len())?;
                    self.reference(item, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn decode<'a>(
    expr: ast::Expr<'a>, env: &Env<'a>, kinds: bool,
) -> Result<Literal<'a>, LiteralError> {
    decode_with(expr, env, kinds, &mut DecoderFuel::default(), 1)
}

fn decode_with<'a>(
    expr: ast::Expr<'a>, env: &Env<'a>, kinds: bool, fuel: &mut DecoderFuel,
    depth: usize,
) -> Result<Literal<'a>, LiteralError> {
    if let ast::Expr::Ident(ident) = expr
        && let (Some(Binding::Literal(value)), _) = env.lookup(ident.as_str())
    {
        // Inspect the existing tree first: cloning could double its size.
        fuel.reference(value, depth)?;
        return Ok(value.clone());
    }
    fuel.node(depth)?;
    let nonliteral = || {
        LiteralError::NonLiteral(format!(
            "expected a literal value, found {}",
            expr.to_untyped().full_text()
        ))
    };
    let value = match expr {
        ast::Expr::None(_) => Literal::None,
        ast::Expr::Auto(_) => Literal::Auto,
        ast::Expr::Bool(value) => Literal::Bool(value.get()),
        ast::Expr::Int(value) => Literal::Int(value.get()),
        ast::Expr::Float(value) => Literal::Float(value.get()),
        ast::Expr::Str(value) => {
            let text = value.get();
            fuel.string(text.len())?;
            Literal::Str(text.to_string())
        }
        ast::Expr::Label(value) => {
            let text = value.get();
            fuel.string(text.len())?;
            Literal::Label(text.into())
        }
        ast::Expr::Numeric(value) => {
            let (number, unit) = value.get();
            let text = value.to_untyped().full_text();
            fuel.string(text.len())?;
            Literal::Numeric { value: number, unit, text: text.to_string() }
        }
        ast::Expr::ContentBlock(block) => {
            Literal::Content(ContentValue { block, env: Rc::new(env.clone()) })
        }
        ast::Expr::Parenthesized(value) => {
            return decode_with(value.expr(), env, kinds, fuel, depth + 1);
        }
        ast::Expr::Ident(ident) => match env.lookup(ident.as_str()) {
            (None, false) if alignment(ident.as_str()) => {
                fuel.string(ident.as_str().len())?;
                Literal::Symbol(ident.as_str().into())
            }
            (None, false)
                if kinds
                    && matches!(ident.as_str(), "image" | "table" | "raw") =>
            {
                fuel.string(ident.as_str().len())?;
                Literal::Symbol(ident.as_str().into())
            }
            _ => return Err(nonliteral()),
        },
        ast::Expr::FieldAccess(_) if kinds => {
            match env.resolve_callee(expr).as_deref() {
                Some(name @ ("image" | "table" | "raw")) => {
                    fuel.string(name.len())?;
                    Literal::Symbol(name.into())
                }
                _ => return Err(nonliteral()),
            }
        }
        ast::Expr::Unary(value) if value.op() == ast::UnOp::Neg => {
            match decode_with(value.expr(), env, kinds, fuel, depth + 1)? {
                Literal::Int(value) => {
                    Literal::Int(value.checked_neg().ok_or_else(|| {
                        LiteralError::Invalid("integer is out of range".into())
                    })?)
                }
                Literal::Float(value) => Literal::Float(-value),
                Literal::Numeric { value: number, unit, text } => {
                    fuel.string(1)?;
                    Literal::Numeric {
                        value: -number,
                        unit,
                        text: format!("-{text}"),
                    }
                }
                _ => return Err(nonliteral()),
            }
        }
        ast::Expr::Binary(value) if value.op() == ast::BinOp::Add => {
            match (
                decode_with(value.lhs(), env, kinds, fuel, depth + 1)?,
                decode_with(value.rhs(), env, kinds, fuel, depth + 1)?,
            ) {
                (Literal::Symbol(left), Literal::Symbol(right))
                    if left.split(" + ").all(alignment)
                        && right.split(" + ").all(alignment) =>
                {
                    fuel.string(3)?;
                    Literal::Symbol(format!("{left} + {right}"))
                }
                _ => return Err(nonliteral()),
            }
        }
        ast::Expr::Array(value) => {
            let mut items = Vec::new();
            for item in value.items() {
                match item {
                    ast::ArrayItem::Pos(expr) => {
                        items.push(decode_with(
                            expr, env, kinds, fuel, depth + 1,
                        )?);
                    }
                    ast::ArrayItem::Spread(_) => return Err(LiteralError::Spread("spread arguments are outside the structured subset".into())),
                }
            }
            Literal::Array(items)
        }
        ast::Expr::Dict(value) => {
            let mut items = BTreeMap::new();
            for item in value.items() {
                let (key, expr) = match item {
                    ast::DictItem::Named(named) => {
                        fuel.string(named.name().as_str().len())?;
                        (named.name().as_str().to_string(), named.expr())
                    }
                    ast::DictItem::Keyed(keyed) => {
                        match decode_with(
                            keyed.key(), env, kinds, fuel, depth + 1,
                        )? {
                            Literal::Str(key) => (key, keyed.expr()),
                            _ => return Err(LiteralError::Invalid("dictionary keys must be literal strings".into())),
                        }
                    }
                    ast::DictItem::Spread(_) => return Err(LiteralError::Spread("spread arguments are outside the structured subset".into())),
                };
                let value = decode_with(expr, env, kinds, fuel, depth + 1)?;
                items.insert(key, value);
            }
            Literal::Dict(items)
        }
        ast::Expr::FuncCall(call) => {
            return constructor(call, env, kinds, fuel, depth);
        }
        _ => return Err(nonliteral()),
    };
    Ok(value)
}

fn alignment(name: &str) -> bool {
    matches!(
        name,
        "left"
            | "right"
            | "center"
            | "start"
            | "end"
            | "top"
            | "bottom"
            | "horizon"
    )
}

fn constructor<'a>(
    call: ast::FuncCall<'a>, env: &Env<'a>, kinds: bool,
    fuel: &mut DecoderFuel, depth: usize,
) -> Result<Literal<'a>, LiteralError> {
    let name = env.resolve_callee(call.callee()).ok_or_else(|| {
        LiteralError::NonLiteral(
            "this call is not a literal constructor".into(),
        )
    })?;
    if !matches!(name.as_str(), "path" | "label" | "datetime") {
        return Err(LiteralError::NonLiteral(format!(
            "{name} is not a literal constructor"
        )));
    }
    let mut positional = Vec::new();
    let mut named = BTreeMap::new();
    for arg in call.args().items() {
        match arg {
            ast::Arg::Pos(expr) => {
                positional.push(decode_with(
                    expr,
                    env,
                    kinds,
                    fuel,
                    depth + 1,
                )?);
            }
            ast::Arg::Named(arg) => {
                let name = arg.name().as_str().to_string();
                fuel.string(name.len())?;
                let value =
                    decode_with(arg.expr(), env, kinds, fuel, depth + 1)?;
                if named.insert(name.clone(), value).is_some() {
                    return Err(LiteralError::Invalid(format!(
                        "argument `{name}` is given more than once"
                    )));
                }
            }
            ast::Arg::Spread(_) => {
                return Err(LiteralError::Spread(
                    "spread arguments are outside the structured subset".into(),
                ));
            }
        }
    }
    if name == "datetime" {
        if !positional.is_empty() || named.len() != 3 {
            return Err(LiteralError::Invalid(
                "datetime requires literal year, month and day".into(),
            ));
        }
        let integer = |name: &str| match named.get(name) {
            Some(Literal::Int(value)) => Some(*value),
            _ => None,
        };
        let year = integer("year").and_then(|value| i32::try_from(value).ok());
        let month = integer("month")
            .and_then(|value| u8::try_from(value).ok())
            .and_then(|value| time::Month::try_from(value).ok());
        let day = integer("day").and_then(|value| u8::try_from(value).ok());
        let date = year
            .zip(month)
            .zip(day)
            .and_then(|((year, month), day)| {
                time::Date::from_calendar_date(year, month, day).ok()
            })
            .ok_or_else(|| {
                LiteralError::Invalid(
                    "datetime requires a valid literal year, month and day"
                        .into(),
                )
            })?;
        let value = format!(
            "{:04}-{:02}-{:02}",
            date.year(),
            u8::from(date.month()),
            date.day()
        );
        fuel.string(value.len())?;
        return Ok(Literal::Date(value));
    }
    if !named.is_empty() || positional.len() != 1 {
        return Err(LiteralError::Invalid(format!(
            "{name} requires one literal string"
        )));
    }
    let Some(Literal::Str(value)) = positional.pop() else {
        return Err(LiteralError::Invalid(format!(
            "{name} requires one literal string"
        )));
    };
    if name == "label" {
        if !typst_syntax::is_valid_label_literal_id(&value) {
            return Err(LiteralError::InvalidLabel(format!(
                "label `{value}` is not valid"
            )));
        }
        Ok(Literal::Label(value))
    } else {
        Ok(Literal::Path(value))
    }
}

pub(super) fn record_let<'a>(
    ctx: &mut Context<'a>, node: ast::LetBinding<'a>, emit: bool,
) {
    let names = node.kind().bindings();
    let normal = matches!(
        node.kind(),
        ast::LetBindingKind::Normal(ast::Pattern::Normal(ast::Expr::Ident(_)))
    );
    let decoded = normal
        .then(|| node.init().map(|expr| decode(expr, &ctx.env, false)))
        .flatten();
    if let Some(Ok(value)) = decoded {
        if let Some(name) = names.first() {
            ctx.env.bind(name.as_str().into(), Binding::Literal(value));
        }
        return;
    }
    for name in names {
        ctx.env.bind(name.as_str().into(), Binding::Unknown);
    }
    if emit {
        match decoded {
            Some(Err(error @ (LiteralError::Invalid(_) | LiteralError::InvalidLabel(_) | LiteralError::Spread(_)))) => error.report(ctx, node.to_untyped(), "E-TYP-ARG-INVALID"),
            _ => ctx.warning("W-TYP-LET-IGNORED", "only simple literal let bindings are part of the structured subset", node.to_untyped()),
        }
    }
}

pub(super) fn record_import<'a>(
    ctx: &mut Context<'a>, node: ast::ModuleImport<'a>, emit: bool,
) {
    let source = match decode(node.source(), &ctx.env, false) {
        Ok(Literal::Str(source)) => Some(source),
        _ => None,
    };
    let module = source.as_deref().and_then(|source| {
        if source.starts_with("@writer/doc:") {
            Some(Module::Writer)
        } else if source.starts_with("@preview/mitex:") {
            Some(Module::Mitex)
        } else {
            None
        }
    });
    if emit {
        match module {
            Some(module) => {
                let version = source
                    .as_deref()
                    .and_then(|source| source.rsplit_once(':'))
                    .map(|(_, version)| version);
                let expected = match module {
                    Module::Writer => crate::typst::WRITER_DOC_VERSION,
                    _ => crate::typst::MITEX_VERSION,
                };
                if version != Some(expected) {
                    ctx.warning("W-TYP-PACKAGE-VERSION", format!("imported {} differs from embedded version {expected}", source.as_deref().unwrap_or_default()), node.to_untyped());
                }
            }
            None => ctx.warning(
                "W-TYP-IMPORT-IGNORED",
                format!(
                    "import {} is outside the structured subset",
                    node.source().to_untyped().full_text()
                ),
                node.to_untyped(),
            ),
        }
    }
    match node.imports() {
        Some(ast::Imports::Wildcard) => match module {
            Some(module) => {
                let names: &[&str] = match module {
                    Module::Writer => &[
                        "writer-doc", "callout", "listing", "code-cell",
                        "include-notebook",
                    ],
                    _ => &["mi", "mitex"],
                };
                for name in names {
                    ctx.env.bind(
                        (*name).into(),
                        Binding::Callable((*name).into()),
                    );
                }
            }
            None => ctx.env.unknown_wildcard(),
        },
        Some(ast::Imports::Items(items)) => {
            for item in items.iter() {
                let original = item
                    .path()
                    .iter()
                    .map(|ident| ident.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let binding = match module {
                    Some(module) if exported(module, &original) => {
                        Binding::Callable(original)
                    }
                    _ => Binding::ImportedUnknown(original),
                };
                ctx.env.bind(item.bound_name().as_str().into(), binding);
            }
        }
        None => {
            let name = node
                .new_name()
                .map(|name| name.as_str().to_string())
                .or_else(|| node.bare_name().ok().map(|name| name.to_string()));
            if let Some(name) = name {
                ctx.env.bind(
                    name,
                    Binding::Module(module.unwrap_or(Module::External)),
                );
            }
        }
    }
}

pub(super) struct Args<'a> {
    pub node: &'a SyntaxNode,
    pub positional: Vec<ast::Expr<'a>>,
    named: Vec<(String, ast::Expr<'a>)>,
}

impl<'a> Args<'a> {
    pub(super) fn new(ctx: &mut Context<'a>, call: ast::FuncCall<'a>) -> Self {
        Self::from_ast(ctx, call.to_untyped(), call.args())
    }

    pub(super) fn from_ast(
        ctx: &mut Context<'a>, node: &'a SyntaxNode, args: ast::Args<'a>,
    ) -> Self {
        let mut out = Self { node, positional: Vec::new(), named: Vec::new() };
        for arg in args.items() {
            match arg {
                ast::Arg::Pos(expr) => out.positional.push(expr),
                ast::Arg::Named(named) => {
                    let name = named.name().as_str();
                    if let Some((_, value)) =
                        out.named.iter_mut().find(|(key, _)| key == name)
                    {
                        ctx.error(
                            "E-TYP-ARG-INVALID",
                            format!(
                                "argument `{name}` is given more than once"
                            ),
                            named.to_untyped(),
                        );
                        *value = named.expr();
                    } else {
                        out.named.push((name.into(), named.expr()));
                    }
                }
                ast::Arg::Spread(spread) => ctx.error(
                    "E-TYP-ARG-SPREAD",
                    "spread arguments are outside the structured subset",
                    spread.to_untyped(),
                ),
            }
        }
        out
    }

    pub(super) fn pos(&self, index: usize) -> Option<ast::Expr<'a>> {
        self.positional.get(index).copied()
    }
    pub(super) fn named(&self, name: &str) -> Option<ast::Expr<'a>> {
        self.named.iter().find(|(key, _)| key == name).map(|(_, value)| *value)
    }
    pub(super) fn named_items(
        &self,
    ) -> impl Iterator<Item = (&str, ast::Expr<'a>)> {
        self.named.iter().map(|(key, value)| (key.as_str(), *value))
    }
    pub(super) fn check_named(
        &self, ctx: &mut Context<'a>, mapped: &[&str], silent: &[&str],
    ) {
        for (name, value) in &self.named {
            if !mapped.contains(&name.as_str())
                && !silent.contains(&name.as_str())
            {
                ctx.warning(
                    "W-TYP-ARG-IGNORED",
                    format!(
                        "argument `{name}` is ignored in the structured subset"
                    ),
                    value.to_untyped(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst_syntax::Source;

    fn expression(source: &Source) -> ast::Expr<'_> {
        source.root().cast::<ast::Markup>().unwrap().exprs().next().unwrap()
    }

    #[test]
    fn native_callees_do_not_escape_explicit_or_wildcard_shadowing() {
        let source = Source::detached("#image(\"x\")");
        let ast::Expr::FuncCall(call) = expression(&source) else {
            panic!("call")
        };
        let mut env = Env::default();
        assert_eq!(env.resolve_callee(call.callee()).as_deref(), Some("image"));
        env.bind("image".into(), Binding::Unknown);
        assert!(env.resolve_callee(call.callee()).is_none());
        env.bind("image".into(), Binding::ImportedUnknown("image".into()));
        assert!(env.resolve_callee(call.callee()).is_none());
        env.unknown_wildcard();
        assert!(env.resolve_callee(call.callee()).is_none());
        env.bind("image".into(), Binding::Callable("mi".into()));
        assert_eq!(env.resolve_callee(call.callee()).as_deref(), Some("mi"));
    }

    #[test]
    fn namespace_shadowing_and_scope_exit_are_respected() {
        let source = Source::detached("#table.cell[Body]");
        let ast::Expr::FuncCall(call) = expression(&source) else {
            panic!("call")
        };
        let mut env = Env::default();
        assert_eq!(
            env.resolve_callee(call.callee()).as_deref(),
            Some("table.cell")
        );
        env.push();
        env.bind("table".into(), Binding::Unknown);
        assert!(env.resolve_callee(call.callee()).is_none());
        env.pop();
        assert_eq!(
            env.resolve_callee(call.callee()).as_deref(),
            Some("table.cell")
        );
    }

    #[test]
    fn content_captures_immutable_bindings_without_recursively_cloning_snapshots()
     {
        let source = Source::detached("#[#name]");
        let mut env = Env::default();
        env.bind(
            "name".into(),
            Binding::Literal(Literal::Str("before".into())),
        );
        let Literal::Content(content) = argument(expression(&source), &env)
            .unwrap_or_else(|_| panic!("content"))
        else {
            panic!("content")
        };
        env.bind("name".into(), Binding::Literal(Literal::Str("after".into())));
        assert!(
            matches!(content.env.get("name"), Some(Binding::Literal(Literal::Str(value))) if value == "before")
        );
        let copied = content.clone();
        assert!(Rc::ptr_eq(&content.env, &copied.env));
    }

    #[test]
    fn literal_constructors_validate_dates_labels_and_nested_spreads() {
        for (text, good) in [
            ("#datetime(year:2024,month:2,day:29)", true),
            ("#datetime(year:2023,month:2,day:29)", false),
            ("#label(\"fig-x\")", true),
            ("#label(\"not valid\")", false),
            ("#path(1)", false),
            ("#(1,..xs)", false),
        ] {
            let source = Source::detached(text);
            assert_eq!(
                argument(expression(&source), &Env::default()).is_ok(),
                good,
                "{text}"
            );
        }
    }
}
