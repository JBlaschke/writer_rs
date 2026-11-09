//! Stable JSON shared by dumps, templates, and Typst inputs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use super::{AssetRegistry, Color, Design, FontRef, Pt};

/// Whether image paths name real files or registered Typst virtual assets.
#[derive(Clone, Copy)]
pub enum PathMode<'a> {
    /// Absolute filesystem paths.
    Real,
    /// Typst `/__assets/` paths for registered furniture images.
    Virtual(&'a AssetRegistry),
    /// Paths inside a Typst source export, relative to its main file.
    Export(&'a ExportPaths),
}

/// Where a source export copied a design's images and font files.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExportPaths {
    /// Furniture and background images, keyed as the design holds them.
    pub images: BTreeMap<PathBuf, String>,
    /// Font files, keyed as the design holds them.
    pub fonts: BTreeMap<PathBuf, String>,
}

impl Serialize for Pt {
    fn serialize<S: Serializer>(
        &self, serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64((self.0 * 1000.0).round() / 1000.0)
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "#{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2]
        )
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(
        &self, serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl Serialize for FontRef {
    fn serialize<S: Serializer>(
        &self, serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Body => "body",
            Self::Heading => "heading",
            Self::Mono => "mono",
            Self::Family(name) => name,
        })
    }
}

impl Design {
    /// Stable resolved JSON, or `null` for an undeclared design.
    pub fn to_json(&self, paths: PathMode<'_>) -> Value {
        if !self.declared {
            return Value::Null;
        }
        let mut value = serde_json::to_value(self)
            .expect("resolved design is serializable");
        value
            .as_object_mut()
            .expect("design is an object")
            .shift_remove("declared");
        match paths {
            PathMode::Real => {}
            PathMode::Virtual(registry) => {
                rewrite_images(&mut value, self, &|path| {
                    virtual_path(path, registry)
                });
            }
            PathMode::Export(export) => {
                rewrite_images(&mut value, self, &|path| {
                    export
                        .images
                        .get(path)
                        .cloned()
                        .unwrap_or_else(|| path.to_string_lossy().into_owned())
                });
                rewrite_fonts(&mut value, &export.fonts);
            }
        }
        value
    }
}

/// The object at a dotted key path, when every key exists.
fn object_at<'v>(
    value: &'v mut Value, dotted: &str,
) -> Option<&'v mut Map<String, Value>> {
    let mut node = value;
    for key in dotted.split('.') {
        node = node.get_mut(key)?;
    }
    node.as_object_mut()
}

/// Rewrites the `path` of every declared image with `rewrite(source)`.
fn rewrite_images(
    value: &mut Value, design: &Design, rewrite: &dyn Fn(&Path) -> String,
) {
    for (slot, source) in design.image_slots() {
        if let Some(image) = object_at(value, slot) {
            image.insert("path".into(), Value::String(rewrite(source)));
        }
    }
}

/// Rewrites every font file that `fonts` maps; others keep their real path.
fn rewrite_fonts(value: &mut Value, fonts: &BTreeMap<PathBuf, String>) {
    for role in ["body", "heading", "mono"] {
        let files = object_at(value, &format!("fonts.{role}"))
            .and_then(|font| font.get_mut("files"))
            .and_then(Value::as_array_mut);
        for file in files.into_iter().flatten() {
            if let Some(path) = file.as_str()
                && let Some(exported) = fonts.get(Path::new(path))
            {
                *file = Value::String(exported.clone());
            }
        }
    }
}

fn virtual_path(path: &Path, registry: &AssetRegistry) -> String {
    if let Some(asset) = registry.find_file(path) {
        let extension = match asset.mime.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/svg+xml" => "svg",
            "application/pdf" => "pdf",
            _ => "bin",
        };
        return format!("/__assets/{}.{extension}", asset.id);
    }
    // Consumers register furniture first; retaining the real path makes a
    // missed registration diagnosable rather than inventing an unknown ID.
    crate::assets::registry::resolve_path(path).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::{
        DesignSpec, Layer, Origin, Segment, TEMPLATE_TOML, merge, segments,
    };
    use serde_json::json;

    #[test]
    fn stable_json_has_expected_shapes_and_virtual_images() {
        let table = json!({"text":{"size":"11pt","line_height":1.35},
            "page":{"margin":"22mm","header":{"image":"logo.png"},
                "background":{"image":"cover.jpg","pages":"all"}},
            "styles":{"Recipient":{"font":"body","line_height":"12.1pt"}}});
        let design = merge(&[Layer {
            table: table.as_object().unwrap().clone(),
            base_dir: Path::new(env!("CARGO_MANIFEST_DIR")),
            origin: Origin::FrontMatter,
        }])
        .unwrap();
        let mut registry = AssetRegistry::new();
        design.register_images(&mut registry);
        let value = design.to_json(PathMode::Virtual(&registry));
        assert_eq!(value["text"]["line_height"], json!({"multiple":1.35}));
        assert_eq!(value["styles"]["Recipient"]["font"], "body");
        assert!(value["page"]["footer"].is_null());
        assert_eq!(value["page"]["margin"]["left"], 62.362);
        assert!(
            value["page"]["header"]["first"]["image"]["path"]
                .as_str()
                .unwrap()
                .starts_with("/__assets/")
        );
        let background = &value["page"]["background"];
        assert!(
            background["image"]["path"]
                .as_str()
                .unwrap()
                .starts_with("/__assets/")
        );
        assert_eq!(background["image"]["fit"], "cover");
        assert_eq!(
            background["image"]["align"],
            json!({"horizontal":"center","vertical":"center"})
        );
        assert_eq!(background["pages"], "all");
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["fonts", "text", "colors", "page", "styles"]
        );
        assert_eq!(Design::default().to_json(PathMode::Real), Value::Null);
    }

    #[test]
    fn reference_block_parses_and_placeholders_keep_unknown_braces() {
        let uncommented = TEMPLATE_TOML
            .lines()
            .filter_map(|line| line.strip_prefix("# "))
            .filter(|line| !line.starts_with("Cross-format"))
            .collect::<Vec<_>>()
            .join("\n");
        let root: toml::Table = toml::from_str(&uncommented).unwrap();
        let _: DesignSpec = root["design"].clone().try_into().unwrap();
        assert_eq!(
            segments("Page {page} of {pages}; {other} {pageX}"),
            vec![
                Segment::Text("Page ".into()),
                Segment::Page,
                Segment::Text(" of ".into()),
                Segment::Pages,
                Segment::Text("; {other} {pageX}".into())
            ]
        );
    }
}
