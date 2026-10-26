//! One context builder shared by document rendering and `dump --context`.
//!
//! Keys follow html-output.md §4.1. Markup keeps asset placeholders until
//! the completed page passes through the asset emitter.

use serde::Serialize;
use serde_json::{Map, Value};

use super::emit::{self, EmitOptions, Emitted};
use super::{BuildMode, RenderInput};
use crate::error::{Diagnostics, Result};
use crate::theme::Theme;

/// Builds the JSON context exposed to document templates.
///
/// The math hook may register generated images in `input.assets`. Rendering
/// diagnostics are discarded here; `render` returns them with its result.
pub fn context_json(input: &mut RenderInput<'_>) -> Result<Value> {
    let (_, value) = build_context(input, &mut Diagnostics::new())?;
    Ok(value)
}

/// Emits markup and builds the ordered template context in one pass.
pub(crate) fn build_context(
    input: &mut RenderInput<'_>, diagnostics: &mut Diagnostics,
) -> Result<(Emitted, Value)> {
    let document = input.document;
    let config = input.config;
    let emitted = emit::emit(
        document,
        &mut EmitOptions {
            config,
            positions: input.build.mode == BuildMode::Serve
                || config.markdown.sourcepos,
            includes: &document.includes,
            math: input.math,
            assets: input.assets,
            diagnostics,
        },
    );
    let mut context = Map::new();
    context.insert("document".into(), to_json(document)?);
    context.insert("meta".into(), to_json(&document.meta)?);
    context.insert("features".into(), to_json(&document.features)?);
    context.insert("config".into(), to_json(config)?);
    context.insert("html".into(), html_json(&emitted));
    context.insert("math".into(), to_json(&emitted.math)?);
    let title = document.meta.title.as_deref().unwrap_or_else(|| {
        document.source.path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
    });
    context.insert("title".into(), title.into());
    let lang = document.meta.lang.as_deref().unwrap_or(&config.document.lang);
    context.insert(
        "lang".into(),
        if lang.is_empty() { "en" } else { lang }.into(),
    );
    context.insert("theme".into(), theme_json(input.theme)?);
    context.insert("build".into(), to_json(&input.build)?);
    Ok((emitted, Value::Object(context)))
}

fn to_json<T: Serialize + ?Sized>(value: &T) -> Result<Value> {
    Ok(serde_json::to_value(value)?)
}

fn html_json(emitted: &Emitted) -> Value {
    let mut map = Map::new();
    for (key, value) in [
        ("main", &emitted.main),
        ("title_block", &emitted.title_block),
        ("body", &emitted.body),
        ("footnotes", &emitted.footnotes),
        ("toc", &emitted.toc),
    ] {
        map.insert(key.into(), value.as_str().into());
    }
    Value::Object(map)
}

fn theme_json(theme: &Theme) -> Result<Value> {
    let scripts: Vec<String> = theme
        .script_entries()?
        .into_iter()
        .map(|file| format!("@{}/{}", file.theme, file.rel_path))
        .collect();
    let chain: Vec<&str> =
        theme.chain.iter().map(|dir| dir.name.as_str()).collect();
    let (light, dark) = theme.code_themes()?;
    let mut map = Map::new();
    map.insert("name".into(), theme.name().into());
    map.insert("chain".into(), chain.into());
    map.insert(
        "code_theme".into(),
        light.map_or("none".into(), |t| t.label).into(),
    );
    map.insert(
        "code_theme_dark".into(),
        dark.map_or("none".into(), |t| t.label).into(),
    );
    map.insert("vars".into(), Value::Object(theme.vars()));
    map.insert("scripts".into(), scripts.into());
    Ok(Value::Object(map))
}
