//! Root metadata prepass and the bibliography call's metadata side effects.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use typst_syntax::SyntaxNode;
use typst_syntax::ast::{self, AstNode};

use super::Context;
use super::literals::{self, Args, Literal};
use crate::error::Result;
use crate::ir::{Author, DocMeta, plain_text};

pub(super) struct MetadataState<'a> {
    pub explicit: bool,
    pub show_toc: bool,
    pub show_bibliography: bool,
    pub show_node: Option<&'a SyntaxNode>,
}

#[derive(Default)]
struct Pending<'a> {
    values: BTreeMap<String, (Literal<'a>, &'a SyntaxNode)>,
    explicit: bool,
}

pub(super) fn prepare<'a>(
    ctx: &mut Context<'a>, markup: ast::Markup<'a>,
) -> Result<MetadataState<'a>> {
    let initial_env = ctx.env.clone();
    let result = prepare_inner(ctx, markup);
    ctx.env = initial_env;
    result
}

fn prepare_inner<'a>(
    ctx: &mut Context<'a>, markup: ast::Markup<'a>,
) -> Result<MetadataState<'a>> {
    let mut pending = Pending::default();
    let mut state = MetadataState {
        explicit: false,
        show_toc: false,
        show_bibliography: false,
        show_node: None,
    };
    for expr in markup.exprs() {
        match expr {
            ast::Expr::LetBinding(node) => {
                literals::record_let(ctx, node, true)
            }
            ast::Expr::ModuleImport(node) => {
                literals::record_import(ctx, node, true)
            }
            ast::Expr::SetRule(rule) => collect_set(ctx, &mut pending, rule),
            ast::Expr::ShowRule(rule) => {
                let transform = rule.transform();
                let call = match transform {
                    ast::Expr::FuncCall(call) => Some(call),
                    _ => None,
                };
                let recognized = rule.selector().is_none()
                    && match call {
                        Some(call) => {
                            matches!(call.callee(), ast::Expr::FieldAccess(access) if access.field().as_str() == "with")
                                && ctx.env.writer_doc(call.callee())
                        }
                        None => ctx.env.writer_doc(transform),
                    };
                if !recognized || state.show_node.is_some() {
                    ctx.warning(
                        "W-TYP-SHOW-IGNORED",
                        "this show rule is kept only by the PDF",
                        rule.to_untyped(),
                    );
                    continue;
                }
                state.show_node = Some(rule.to_untyped());
                if let Some(call) = call {
                    let args = Args::new(ctx, call);
                    if !args.positional.is_empty() {
                        ctx.error(
                            "E-TYP-ARG-INVALID",
                            "writer-doc.with accepts named metadata arguments",
                            args.node,
                        );
                    }
                    for (name, expr) in args.named_items() {
                        let key = canonical_key(name);
                        if let Ok(value) = metadata_literal(ctx, expr)
                            && pending.assign(
                                ctx,
                                key,
                                value.clone(),
                                expr.to_untyped(),
                            )
                        {
                            if name == "toc" {
                                state.show_toc = requested(&value);
                            }
                            if name == "bibliography" {
                                state.show_bibliography = nonempty(&value);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    state.explicit = pending.explicit;
    let base_config = ctx.config.clone();
    let mut meta = DocMeta::default();
    let mut deferred = Vec::new();
    for (name, (value, node)) in &pending.values {
        if contains_content(value) {
            deferred.push((name, value, *node));
        } else {
            apply(ctx, &mut meta, name, value);
        }
    }
    let mut effective = base_config.clone();
    effective.merge_front_matter(&meta)?;
    ctx.config = effective;
    for (name, value, _) in deferred {
        apply(ctx, &mut meta, name, value);
    }
    let mut effective = base_config;
    effective.merge_front_matter(&meta)?;
    ctx.config = effective;
    ctx.meta = meta;
    Ok(state)
}

fn metadata_literal<'a>(
    ctx: &mut Context<'a>, expr: ast::Expr<'a>,
) -> std::result::Result<Literal<'a>, ()> {
    match ctx.literal(expr) {
        Ok(value) => Ok(value),
        Err(error) => {
            error.report(ctx, expr.to_untyped(), "E-TYP-TEMPLATE-ARG");
            Err(())
        }
    }
}

impl<'a> Pending<'a> {
    fn assign(
        &mut self, ctx: &mut Context<'a>, key: String, value: Literal<'a>,
        node: &'a SyntaxNode,
    ) -> bool {
        if matches!(value, Literal::Auto) {
            return false;
        }
        if !valid_field(&key, &value) || !valid_config_shape(ctx, &key, &value)
        {
            ctx.error(
                "E-TYP-ARG-INVALID",
                format!(
                    "metadata argument `{key}` has the wrong literal shape"
                ),
                node,
            );
            return false;
        }
        if key == "bibliography" {
            warn_path_strings(ctx, &value, node);
        }
        if matches!(key.as_str(), "cite_style" | "csl")
            && matches!(&value, Literal::Str(path) if path.to_ascii_lowercase().ends_with(".csl"))
        {
            ctx.warning(
                "W-TYP-PATH-STRING",
                "pass a CSL file with path(\"style.csl\")",
                node,
            );
        }
        self.explicit |= nonempty(&value);
        // Both spellings set one field; a later show/set assignment replaces
        // the earlier spelling instead of manufacturing a config conflict.
        match key.as_str() {
            "cite_style" => {
                self.values.remove("csl");
            }
            "csl" => {
                self.values.remove("cite_style");
            }
            _ => {}
        }
        self.values.insert(key, (value, node));
        true
    }

    fn table_entry(
        &mut self, ctx: &mut Context<'a>, table: &str, key: &str,
        value: Literal<'a>, node: &'a SyntaxNode,
    ) {
        let mut entries = match self.values.get(table) {
            Some((Literal::Dict(entries), _)) => entries.clone(),
            Some((Literal::Bool(flag), _)) if table == "numbering" => {
                BTreeMap::from([("headings".into(), Literal::Bool(*flag))])
            }
            _ => BTreeMap::new(),
        };
        entries.insert(key.into(), value);
        self.assign(ctx, table.into(), Literal::Dict(entries), node);
    }
}

fn collect_set<'a>(
    ctx: &mut Context<'a>, pending: &mut Pending<'a>, rule: ast::SetRule<'a>,
) {
    if rule.condition().is_some() {
        ctx.warning(
            "W-TYP-SET-IGNORED",
            "conditional set rules are kept only by the PDF",
            rule.to_untyped(),
        );
        return;
    }
    let target = ctx.callee(rule.target());
    let mapped: &[&str] = match target.as_deref() {
        Some("document") => {
            &["title", "author", "date", "keywords", "description"]
        }
        Some("text") => &["lang"],
        Some("heading" | "math.equation") => &["numbering"],
        Some("page") => &["paper"],
        Some("bibliography") => &["style"],
        Some(
            "par" | "table" | "raw" | "list" | "enum" | "terms" | "block"
            | "image" | "figure.caption" | "footnote.entry" | "quote"
            | "outline" | "align",
        ) => return,
        _ => {
            ctx.warning(
                "W-TYP-SET-IGNORED",
                "this set rule is kept only by the PDF",
                rule.to_untyped(),
            );
            return;
        }
    };
    let args = Args::from_ast(ctx, rule.to_untyped(), rule.args());
    if !args.positional.is_empty() {
        ctx.error(
            "E-TYP-ARG-INVALID",
            "this set rule accepts named arguments",
            args.node,
        );
    }
    for (name, expr) in args.named_items() {
        if !mapped.contains(&name) {
            continue;
        }
        let Ok(value) = metadata_literal(ctx, expr) else {
            continue;
        };
        if matches!(value, Literal::Auto) {
            continue;
        }
        match (target.as_deref(), name) {
            (Some("heading"), "numbering") => {
                let value = match value {
                    Literal::None => Literal::Bool(false),
                    Literal::Str(_) => value,
                    _ => {
                        ctx.error("E-TYP-ARG-INVALID", "heading numbering must be a literal string or none", expr.to_untyped());
                        continue;
                    }
                };
                pending.table_entry(
                    ctx,
                    "numbering",
                    "headings",
                    value,
                    expr.to_untyped(),
                );
            }
            (Some("math.equation"), "numbering") => {
                let value = match value {
                    Literal::None => Literal::Str("none".into()),
                    Literal::Str(_) => Literal::Str("all".into()),
                    _ => {
                        ctx.error("E-TYP-ARG-INVALID", "equation numbering must be a literal string or none", expr.to_untyped());
                        continue;
                    }
                };
                pending.table_entry(
                    ctx,
                    "numbering",
                    "equations",
                    value,
                    expr.to_untyped(),
                );
            }
            (Some("bibliography"), "style") => {
                pending.assign(
                    ctx,
                    "cite_style".into(),
                    value,
                    expr.to_untyped(),
                );
            }
            (_, "author") => {
                pending.assign(ctx, "authors".into(), value, expr.to_untyped());
            }
            _ => {
                pending.assign(
                    ctx,
                    canonical_key(name),
                    value,
                    expr.to_untyped(),
                );
            }
        }
    }
}

fn canonical_key(name: &str) -> String {
    match name {
        "author" => "authors".into(),
        "cite-style" | "cite-locale" | "reference-doc" | "latex-template" => {
            name.replace('-', "_")
        }
        _ => name.into(),
    }
}

fn config_table(name: &str) -> bool {
    matches!(
        name,
        "markdown"
            | "math"
            | "numbering"
            | "references"
            | "html"
            | "pdf"
            | "docx"
            | "latex"
            | "typst"
            | "notebook"
    )
}

fn valid_field(name: &str, value: &Literal<'_>) -> bool {
    match name {
        "title" | "subtitle" => matches!(
            value,
            Literal::Str(_) | Literal::Content(_) | Literal::None
        ),
        "abstract" => matches!(value, Literal::Content(_) | Literal::None),
        "authors" => match value {
            Literal::None | Literal::Str(_) => true,
            Literal::Array(values) => values.iter().all(valid_author),
            _ => false,
        },
        "date" => {
            matches!(value, Literal::Str(_) | Literal::Date(_) | Literal::None)
        }
        "description" => matches!(value, Literal::Str(_) | Literal::None),
        "lang" | "cite_locale" | "paper" | "theme" | "template"
        | "reference_doc" | "latex_template" => {
            matches!(value, Literal::Str(_))
        }
        "keywords" => {
            matches!(value, Literal::Str(_))
                || matches!(value, Literal::Array(values) if values.iter().all(|value| matches!(value, Literal::Str(_))))
        }
        "toc" => matches!(value, Literal::Bool(_) | Literal::Int(1..=6)),
        "numbering" => matches!(value, Literal::Bool(_) | Literal::Dict(_)),
        "bibliography" => path_values(value).is_some(),
        "cite_style" | "csl" => {
            matches!(value, Literal::Str(_) | Literal::Path(_))
        }
        "toc-title" | "abstract-title" | "reference-section-title" => {
            matches!(value, Literal::Str(_) | Literal::Content(_))
        }
        name if config_table(name) => matches!(value, Literal::Dict(_)),
        _ => true,
    }
}

fn valid_author(value: &Literal<'_>) -> bool {
    match value {
        Literal::Str(_) => true,
        Literal::Dict(fields) => {
            matches!(fields.get("name"), Some(Literal::Str(_)))
                && ["affiliation", "email", "id"].iter().all(|key| {
                    matches!(
                        fields.get(*key),
                        None | Some(
                            Literal::Str(_) | Literal::None | Literal::Auto
                        )
                    )
                })
        }
        _ => false,
    }
}

fn valid_config_shape(
    ctx: &Context<'_>, name: &str, value: &Literal<'_>,
) -> bool {
    if !config_table(name) || matches!(value, Literal::Bool(_)) {
        return true;
    }
    let Ok(config) = serde_json::to_value(&ctx.config) else {
        return true;
    };
    config.get(name).is_none_or(|template| compatible(template, value, name))
}

fn compatible(template: &Value, value: &Literal<'_>, path: &str) -> bool {
    if matches!(value, Literal::Auto) {
        return true;
    }
    if path == "numbering.headings" {
        return matches!(value, Literal::Bool(_) | Literal::Str(_));
    }
    match (template, value) {
        (Value::Null, _) => true,
        (Value::Bool(_), Literal::Bool(_)) => true,
        (
            Value::String(_),
            Literal::Str(_)
            | Literal::Path(_)
            | Literal::Date(_)
            | Literal::Symbol(_),
        ) => true,
        (Value::Number(number), Literal::Int(value)) => {
            !number.is_u64() || *value >= 0
        }
        (Value::Number(number), Literal::Float(_)) => number.is_f64(),
        (Value::Array(template), Literal::Array(values)) => {
            let string = Value::String(String::new());
            let template = template.first().unwrap_or(&string);
            values.iter().all(|value| compatible(template, value, path))
        }
        (Value::Object(template), Literal::Dict(values)) => {
            values.iter().all(|(key, value)| {
                let key = key.replace('-', "_");
                template.get(&key).is_none_or(|template| {
                    compatible(template, value, &format!("{path}.{key}"))
                })
            })
        }
        _ => false,
    }
}

fn nonempty(value: &Literal<'_>) -> bool {
    match value {
        Literal::None | Literal::Auto => false,
        Literal::Array(values) => !values.is_empty(),
        Literal::Dict(values) => !values.is_empty(),
        Literal::Str(value) => !value.is_empty(),
        _ => true,
    }
}

fn requested(value: &Literal<'_>) -> bool {
    matches!(value, Literal::Bool(true) | Literal::Int(1..=6))
}

fn contains_content(value: &Literal<'_>) -> bool {
    match value {
        Literal::Content(_) => true,
        Literal::Array(values) => values.iter().any(contains_content),
        Literal::Dict(values) => values.values().any(contains_content),
        _ => false,
    }
}

fn apply<'a>(
    ctx: &mut Context<'a>, meta: &mut DocMeta, key: &str, value: &Literal<'a>,
) {
    let text = |ctx: &mut Context<'a>, value: &Literal<'a>| match value {
        Literal::Str(value) | Literal::Date(value) => Some(value.clone()),
        Literal::Content(value) => {
            Some(plain_text(&ctx.metadata_inlines(value)))
        }
        _ => None,
    };
    match key {
        "title" => meta.title = text(ctx, value),
        "subtitle" => meta.subtitle = text(ctx, value),
        "date" => meta.date = text(ctx, value),
        "description" => meta.description = text(ctx, value),
        "lang" => meta.lang = text(ctx, value),
        "authors" => meta.authors = authors(value),
        "abstract" => {
            meta.abstract_ = match value {
                Literal::Content(content) => Some(ctx.content_blocks(content)),
                _ => None,
            }
        }
        "keywords" => {
            meta.keywords = match value {
                Literal::Str(value) => vec![value.clone()],
                Literal::Array(values) => {
                    values.iter().filter_map(|value| text(ctx, value)).collect()
                }
                _ => Vec::new(),
            }
        }
        _ => {
            if let Some(value) = json_value(ctx, value, config_table(key)) {
                meta.vars.insert(key.into(), value);
            }
        }
    }
}

fn authors(value: &Literal<'_>) -> Vec<Author> {
    match value {
        Literal::Str(name) => {
            vec![Author { name: name.clone(), ..Default::default() }]
        }
        Literal::Array(values) => values
            .iter()
            .filter_map(|value| match value {
                Literal::Str(name) => {
                    Some(Author { name: name.clone(), ..Default::default() })
                }
                Literal::Dict(values) => {
                    let text = |key: &str| match values.get(key) {
                        Some(Literal::Str(value)) => Some(value.clone()),
                        _ => None,
                    };
                    Some(Author {
                        name: text("name")?,
                        affiliation: text("affiliation"),
                        email: text("email"),
                        id: text("id"),
                    })
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn json_value<'a>(
    ctx: &mut Context<'a>, value: &Literal<'a>, normalize: bool,
) -> Option<Value> {
    Some(match value {
        Literal::Auto => return None,
        Literal::None => Value::Null,
        Literal::Bool(value) => Value::Bool(*value),
        Literal::Int(value) => Value::from(*value),
        Literal::Float(value) => Value::from(*value),
        Literal::Str(value)
        | Literal::Label(value)
        | Literal::Path(value)
        | Literal::Date(value)
        | Literal::Symbol(value) => Value::String(value.clone()),
        Literal::Numeric { text, .. } => Value::String(text.clone()),
        Literal::Content(value) => {
            Value::String(plain_text(&ctx.metadata_inlines(value)))
        }
        Literal::Array(values) => Value::Array(
            values
                .iter()
                .filter_map(|value| json_value(ctx, value, normalize))
                .collect(),
        ),
        Literal::Dict(values) => {
            let map = values
                .iter()
                .filter_map(|(key, value)| {
                    let key = if normalize {
                        key.replace('-', "_")
                    } else {
                        key.clone()
                    };
                    json_value(ctx, value, normalize).map(|value| (key, value))
                })
                .collect::<Map<_, _>>();
            Value::Object(map)
        }
    })
}

fn path_values(value: &Literal<'_>) -> Option<Vec<String>> {
    match value {
        Literal::Str(value) | Literal::Path(value) => Some(vec![value.clone()]),
        Literal::Array(values) => values
            .iter()
            .map(|value| match value {
                Literal::Str(value) | Literal::Path(value) => {
                    Some(value.clone())
                }
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

fn warn_path_strings(
    ctx: &mut Context<'_>, value: &Literal<'_>, node: &SyntaxNode,
) {
    match value {
        Literal::Str(_) => ctx.warning(
            "W-TYP-PATH-STRING",
            "pass bibliography paths with path(\"refs.bib\")",
            node,
        ),
        Literal::Array(values) => {
            for value in values {
                warn_path_strings(ctx, value, node);
            }
        }
        _ => {}
    }
}

pub(super) fn bibliography<'a>(ctx: &mut Context<'a>, args: &Args<'a>) -> bool {
    if args.positional.len() != 1 {
        ctx.error(
            "E-TYP-ARG-INVALID",
            "bibliography requires one literal source or source array",
            args.node,
        );
        return false;
    }
    let Some(expr) = args.pos(0) else {
        ctx.error(
            "E-TYP-ARG-INVALID",
            "bibliography requires literal sources",
            args.node,
        );
        return false;
    };
    let value = match ctx.literal(expr) {
        Ok(value) if path_values(&value).is_some() => value,
        Ok(_) => {
            ctx.error(
                "E-TYP-ARG-INVALID",
                "bibliography sources must be a path or array of paths",
                expr.to_untyped(),
            );
            return false;
        }
        Err(error) => {
            error.report(ctx, expr.to_untyped(), "E-TYP-ARG-INVALID");
            return false;
        }
    };
    // Native bibliography accepts strings relative to its own call site; the
    // package-only W-TYP-PATH-STRING warning does not apply here.
    let sources = path_values(&value).expect("validated above");
    ctx.meta.vars.insert(
        "bibliography".into(),
        Value::Array(sources.iter().cloned().map(Value::String).collect()),
    );
    ctx.config.references.bibliography =
        sources.into_iter().map(std::path::PathBuf::from).collect();
    if let Some(expr) = args.named("style") {
        match ctx.literal(expr) {
            Ok(Literal::Str(value) | Literal::Path(value)) => {
                ctx.meta
                    .vars
                    .insert("cite_style".into(), Value::String(value.clone()));
                ctx.meta.vars.remove("csl");
                ctx.config.references.style = value;
            }
            Ok(Literal::Auto) => {}
            Ok(_) => ctx.error(
                "E-TYP-ARG-INVALID",
                "bibliography style must be a literal string or path",
                expr.to_untyped(),
            ),
            Err(error) => {
                error.report(ctx, expr.to_untyped(), "E-TYP-ARG-INVALID")
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_metadata_shapes_reject_wrong_values_without_losing_valid_custom_data()
     {
        assert!(!valid_field("toc", &Literal::Str("bad".into())));
        assert!(valid_field("toc", &Literal::Int(6)));
        assert!(!valid_field("toc", &Literal::Int(7)));
        assert!(valid_field("custom-key", &Literal::Int(-2)));
        assert!(!valid_field(
            "authors",
            &Literal::Array(vec![Literal::Int(3)])
        ));
        assert!(valid_field(
            "bibliography",
            &Literal::Array(vec![Literal::Path("refs.bib".into())])
        ));
    }

    #[test]
    fn configuration_shape_validation_preserves_unknown_keys_for_normal_validation()
     {
        let template =
            serde_json::json!({"system_fonts": true, "font_paths": []});
        assert!(compatible(
            &template,
            &Literal::Dict(BTreeMap::from([(
                "system-fonts".into(),
                Literal::Bool(false)
            )])),
            "pdf"
        ));
        assert!(!compatible(
            &template,
            &Literal::Dict(BTreeMap::from([(
                "system-fonts".into(),
                Literal::Str("false".into())
            )])),
            "pdf"
        ));
        assert!(compatible(
            &template,
            &Literal::Dict(BTreeMap::from([(
                "unknown".into(),
                Literal::Bool(false)
            )])),
            "pdf"
        ));
    }
}
