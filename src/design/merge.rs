//! Per-source paths, deterministic deep merge, and error provenance.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::{Design, DesignSpec, Layer, Origin};
use crate::assets::registry::normalize_lexically;
use crate::error::{Error, Result};

/// Absolutises each path before this source is merged with another source.
/// Theme paths are confined to the theme, including through symbolic links.
pub fn absolutise_paths(
    table: &mut Map<String, Value>, base_dir: &Path, origin: &Origin,
) -> Result<()> {
    if let Some(fonts) = table.get_mut("fonts").and_then(Value::as_object_mut) {
        for role in ["body", "heading", "mono"] {
            if let Some(files) = fonts
                .get_mut(role)
                .and_then(Value::as_object_mut)
                .and_then(|font| font.get_mut("files"))
                .and_then(Value::as_array_mut)
            {
                for file in files {
                    absolutise(file, base_dir, origin)?;
                }
            }
        }
    }
    if let Some(page) = table.get_mut("page").and_then(Value::as_object_mut) {
        for slot in ["header", "footer"] {
            if let Some(value) = page.get_mut(slot) {
                furniture_paths(value, base_dir, origin)?;
            }
        }
    }
    Ok(())
}

fn furniture_paths(
    value: &mut Value, base: &Path, origin: &Origin,
) -> Result<()> {
    if let Some(map) = value.as_object_mut() {
        if let Some(image) = map.get_mut("image") {
            if let Some(image) = image.as_object_mut() {
                if let Some(source) = image.get_mut("src") {
                    absolutise(source, base, origin)?;
                }
            } else {
                absolutise(image, base, origin)?;
            }
        }
        for key in ["first", "rest"] {
            if let Some(value) = map.get_mut(key) {
                furniture_paths(value, base, origin)?;
            }
        }
    }
    Ok(())
}

fn absolutise(value: &mut Value, base: &Path, origin: &Origin) -> Result<()> {
    let Some(source) = value.as_str() else {
        return Ok(());
    };
    let base = absolute(base)?;
    let path = absolute(&base.join(source))?;
    if let Origin::Theme { file } = origin
        && !path.starts_with(&base)
    {
        return Err(Error::Theme {
            name: file.display().to_string(),
            message: format!(
                "{}: theme-link-outside: design path {source:?} leaves the theme directory",
                file.display()
            ),
        });
    }
    *value = Value::String(path.to_string_lossy().into_owned());
    Ok(())
}

// Resolve existing ancestors as well as existing files: a missing file beneath
// a symlink still must not bypass the theme boundary.
fn absolute(path: &Path) -> Result<PathBuf> {
    use std::path::Component;
    let path = std::path::absolute(path)?;
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::CurDir => {}
            component => {
                resolved.push(component.as_os_str());
                if let Ok(canonical) = resolved.canonicalize() {
                    resolved = canonical;
                }
            }
        }
    }
    Ok(normalize_lexically(&resolved))
}

/// The merged raw table, with source-relative paths already made absolute.
/// An explicitly empty furniture slot replaces an inherited slot.
pub fn merged_table(layers: &[Layer<'_>]) -> Result<Map<String, Value>> {
    collect(layers).map(|(table, _)| table)
}

/// Deep-merges, validates, and resolves ordered design sources.
pub fn merge(layers: &[Layer<'_>]) -> Result<Design> {
    let (table, origins) = collect(layers)?;
    let result = super::resolve::resolve(&table).and_then(|design| {
        // Keep the public serde schema authoritative too. The contextual pass
        // runs first so untagged forms never erase a useful key-path error.
        serde_json::from_value::<DesignSpec>(Value::Object(table))
            .map_err(|error| Error::Config(format!("design: {error}")))?;
        Ok(design)
    });
    result.map_err(|error| contextualize(error, &origins))
}

type Origins = BTreeMap<String, Origin>;

fn collect(layers: &[Layer<'_>]) -> Result<(Map<String, Value>, Origins)> {
    let mut table = Map::new();
    let mut origins = Origins::new();
    for layer in layers {
        let mut over = layer.table.clone();
        absolutise_paths(&mut over, layer.base_dir, &layer.origin)?;
        clear_empty_furniture(&mut table, &over);
        remember(
            &Value::Object(over.clone()),
            "design",
            &layer.origin,
            &mut origins,
        );
        crate::theme::deep_merge(&mut table, &over);
    }
    Ok((table, origins))
}

fn clear_empty_furniture(
    base: &mut Map<String, Value>, over: &Map<String, Value>,
) {
    let Some(over_page) = over.get("page").and_then(Value::as_object) else {
        return;
    };
    let Some(base_page) = base.get_mut("page").and_then(Value::as_object_mut)
    else {
        return;
    };
    for slot in ["header", "footer"] {
        let Some(over_slot) = over_page.get(slot).and_then(Value::as_object)
        else {
            continue;
        };
        if over_slot.is_empty() {
            base_page.remove(slot);
        } else if let Some(base_slot) =
            base_page.get_mut(slot).and_then(Value::as_object_mut)
        {
            for variant in ["first", "rest"] {
                if over_slot
                    .get(variant)
                    .and_then(Value::as_object)
                    .is_some_and(Map::is_empty)
                {
                    base_slot.remove(variant);
                }
            }
        }
    }
}

fn remember(value: &Value, path: &str, origin: &Origin, origins: &mut Origins) {
    origins.insert(path.into(), origin.clone());
    if let Some(map) = value.as_object() {
        for (key, value) in map {
            remember(value, &format!("{path}.{key}"), origin, origins);
        }
    } else {
        let prefix = format!("{path}.");
        origins.retain(|key, _| !key.starts_with(&prefix));
    }
}

fn contextualize(error: Error, origins: &Origins) -> Error {
    let Error::Config(message) = error else {
        return error;
    };
    let path = message.split_once(": ").map_or("design", |(path, _)| path);
    let mut candidate = path.split('[').next().unwrap_or(path);
    let origin = loop {
        if let Some(origin) = origins.get(candidate) {
            break Some(origin);
        }
        let Some((parent, _)) = candidate.rsplit_once('.') else {
            break None;
        };
        candidate = parent;
    };
    if let Some(Origin::Theme { file }) = origin {
        let line = std::fs::read_to_string(file).ok().and_then(|text| {
            let (table, key) = path.rsplit_once('.')?;
            crate::theme::manifest::line_of_key(&text, table, key)
        });
        let location = line.map_or_else(
            || file.display().to_string(),
            |line| format!("{}:{line}", file.display()),
        );
        Error::Theme {
            name: file.display().to_string(),
            message: format!("{location}: {message}"),
        }
    } else {
        Error::Config(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::{Color, LineHeight, PathMode, Pt};
    use serde_json::json;

    fn layer(value: Value) -> Layer<'static> {
        Layer {
            table: value.as_object().unwrap().clone(),
            base_dir: Path::new("/tmp"),
            origin: Origin::FrontMatter,
        }
    }

    #[test]
    fn merge_lists_defaults_tokens_and_explicit_empty_slots() {
        let parent = layer(json!({
            "fonts": {"body": {"family": "Example", "files": ["a.ttf", "b.ttf"]}},
            "text": {"size": "12pt"}, "colors": {"link": "accent", "accent": "#AbC"},
            "page": {"footer": {"lines": ["parent"], "rule": true}},
            "styles": {"Recipient": {"line_height": "12.1pt"}}
        }));
        let child = layer(json!({"fonts": {"body": {"files": ["c.ttf"]}}}));
        let design = merge(&[parent.clone(), child]).unwrap();
        assert_eq!(
            design.fonts.body.as_ref().unwrap().files,
            vec![std::fs::canonicalize("/tmp").unwrap().join("c.ttf")]
        );
        assert_eq!(design.colors.link, Some(Color([170, 187, 204])));
        assert_eq!(
            design.page.footer.as_ref().unwrap().first.as_ref().unwrap().size,
            Some(Pt(9.0))
        );
        assert_eq!(
            design.styles["Recipient"].line_height,
            Some(LineHeight::Exact(Pt(12.1)))
        );
        let empty =
            merge(&[parent, layer(json!({"page": {"footer": {}}}))]).unwrap();
        assert_eq!(empty.page.footer, Some(Default::default()));
        assert!(
            empty.to_json(PathMode::Real)["page"]["footer"]["first"].is_null()
        );
    }

    #[test]
    fn contextual_errors_preserve_keys_and_theme_origins() {
        for (value, expected) in [
            (
                json!({"text":{"sise":"12pt"}}),
                "design.text.sise: unknown key (did you mean size?)",
            ),
            (
                json!({"styles":{"Recipient":{"space_after":6}}}),
                "design.styles.Recipient.space_after: expected a length with a unit such as \"11pt\" or \"22mm\", got 6",
            ),
            (
                json!({"page":{"header":{"first":{"rest":{}}}}}),
                "design.page.header.first.rest: unknown key",
            ),
            (
                json!({"styles":{"Inside Address":{},"inside-address":{}}}),
                "design.styles: \"Inside Address\" and \"inside-address\" are the same style name",
            ),
        ] {
            let error = merge(&[layer(value)]).unwrap_err();
            assert_eq!(error.to_string(), format!("config: {expected}"));
        }
        let layer = Layer {
            origin: Origin::Theme { file: "/tmp/theme.toml".into() },
            ..layer(json!({"text":{"size":"bad"}}))
        };
        assert!(matches!(merge(&[layer]), Err(Error::Theme { .. })));
    }

    #[test]
    fn theme_paths_cannot_escape_lexically_or_through_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let mut table = json!({"page":{"header":{"image":"../outside.png"}}})
            .as_object()
            .unwrap()
            .clone();
        let origin = Origin::Theme { file: dir.path().join("theme.toml") };
        assert!(
            absolutise_paths(&mut table, dir.path(), &origin)
                .unwrap_err()
                .to_string()
                .contains("theme-link-outside")
        );
        let mut table = serde_json::json!({"page":{"header":{
            "image":"missing/../../outside.png"
        }}})
        .as_object()
        .unwrap()
        .clone();
        assert!(absolutise_paths(&mut table, dir.path(), &origin).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/", dir.path().join("escape")).unwrap();
            let mut table = json!({"fonts":{"body":{"family":"X","files":["escape/missing.ttf"]}}})
                .as_object().unwrap().clone();
            assert!(absolutise_paths(&mut table, dir.path(), &origin).is_err());
        }
    }
}
