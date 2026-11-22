//! Layer 0: writer's built-in design, merged below the theme chain, and the
//! switch that lets its key groups reach the backends one task at a time
//! (design.md §1.1; R13 §4.1).
//!
//! The files in this directory together form one `[design]` table, written
//! as a user writes theirs. Every leaf belongs to exactly one key group
//! ([`group`]). [`APPLIED`] lists the groups whose built-in values the
//! backends see; [`layer`] keeps only those.

use std::path::Path;
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{Map, Value};

use super::{Layer, Origin, Pt};

/// The groups whose built-in values reach the backends. Empty until T108:
/// the view then equals the design without layer 0, `declared` included.
pub const APPLIED: Domains = Domains::NONE;

/// A set of key groups of the built-in design, one bit per group.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Domains(u32);

impl Domains {
    /// No group.
    pub const NONE: Self = Self(0);
    /// `text.size`, `line_height`, `paragraph_spacing`, `justify`,
    /// `measure` and `screen_scale`. Switched on by T108.
    pub const TEXT: Self = Self(1 << 0);
    /// `fonts.body`, `heading` and `mono`. Switched on by T108.
    pub const FONTS: Self = Self(1 << 1);
    /// `colors.text`, `accent`, `muted`, `link` and `background`. Switched
    /// on by T108.
    pub const COLORS: Self = Self(1 << 2);
    /// `page.paper`, `margin`, `header_offset`, `footer_offset` and
    /// `print`. Switched on by T108.
    pub const PAGE: Self = Self(1 << 3);
    /// The heading, title block, abstract and contents styles. Switched on
    /// by T109.
    pub const HEADINGS: Self = Self(1 << 4);
    /// The Caption, Image Caption and Table Caption styles. Switched on by
    /// T110.
    pub const CAPTIONS: Self = Self(1 << 5);
    /// `[design.image]`. Switched on by T110.
    pub const IMAGES: Self = Self(1 << 6);
    /// `colors.border`, `code_bg`, `highlight`, `error` and
    /// `colors.callout`. Switched on by T111.
    pub const PALETTE: Self = Self(1 << 7);
    /// `[design.links]`. Switched on by T111.
    pub const LINKS: Self = Self(1 << 8);
    /// `[design.lists]`. Switched on by T112.
    pub const LISTS: Self = Self(1 << 9);
    /// The Block Text, Horizontal Rule, Definition Term and Definition
    /// styles. Switched on by T112.
    pub const QUOTES: Self = Self(1 << 10);
    /// `[design.code]` and the Source Code style. Switched on by T113.
    pub const CODE: Self = Self(1 << 11);
    /// `[design.callout]`. Switched on by T113.
    pub const CALLOUTS: Self = Self(1 << 12);
    /// `fonts.math`. Switched on by T113.
    pub const MATH_FONT: Self = Self(1 << 13);
    /// `[design.table]` and the Compact style. Switched on by T114.
    pub const TABLES: Self = Self(1 << 14);
    /// `[design.footnotes]` and the Footnote Text and Bibliography styles.
    /// Switched on by T115.
    pub const FOOTNOTES: Self = Self(1 << 15);
    /// The page-number footer slot and the Header and Footer styles.
    /// Switched on by T116.
    pub const PAGE_NUMBERS: Self = Self(1 << 16);
    /// The center, left, right, small and large div styles. Switched on by
    /// T117.
    pub const DIV_CLASSES: Self = Self(1 << 17);
    /// `text.hyphenate`, `widows`, `orphans`, `kerning` and `ligatures`.
    /// Switched on by T118.
    pub const TYPOGRAPHY: Self = Self(1 << 18);
    /// Every group.
    pub const ALL: Self = Self((1 << 19) - 1);

    /// Each group with its name, in the order the tasks switch them on.
    pub const GROUPS: [(&'static str, Self); 19] = [
        ("text", Self::TEXT),
        ("fonts", Self::FONTS),
        ("colors", Self::COLORS),
        ("page", Self::PAGE),
        ("headings", Self::HEADINGS),
        ("captions", Self::CAPTIONS),
        ("images", Self::IMAGES),
        ("palette", Self::PALETTE),
        ("links", Self::LINKS),
        ("lists", Self::LISTS),
        ("quotes", Self::QUOTES),
        ("code", Self::CODE),
        ("callouts", Self::CALLOUTS),
        ("math_font", Self::MATH_FONT),
        ("tables", Self::TABLES),
        ("footnotes", Self::FOOTNOTES),
        ("page_numbers", Self::PAGE_NUMBERS),
        ("div_classes", Self::DIV_CLASSES),
        ("typography", Self::TYPOGRAPHY),
    ];

    /// Both sets' groups.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every group of `other` is in this set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the set has no group.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The groups whose built-in values name a colour token of another
    /// group: switching a group on needs these on too, or the view cannot
    /// resolve.
    pub const fn needs(self) -> Self {
        let mut needs = Self::NONE;
        let colors = Self::HEADINGS
            .union(Self::CAPTIONS)
            .union(Self::TABLES)
            .union(Self::PAGE_NUMBERS);
        if self.0 & colors.0 != 0 {
            needs = needs.union(Self::COLORS);
        }
        let palette = Self::QUOTES.union(Self::CODE).union(Self::FOOTNOTES);
        if self.0 & palette.0 != 0 {
            needs = needs.union(Self::COLORS).union(Self::PALETTE);
        }
        needs
    }
}

impl std::ops::BitOr for Domains {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        self.union(other)
    }
}

/// The group of the layer-0 leaf at `path`, a key path below `[design]`;
/// [`Domains::NONE`] for a path layer 0 does not set.
pub fn group(path: &[&str]) -> Domains {
    match path {
        [
            "text",
            "hyphenate" | "widows" | "orphans" | "kerning" | "ligatures",
        ] => Domains::TYPOGRAPHY,
        ["text", ..] => Domains::TEXT,
        ["fonts", "math", ..] => Domains::MATH_FONT,
        ["fonts", "body" | "heading" | "mono", ..] => Domains::FONTS,
        [
            "colors",
            "border" | "code_bg" | "highlight" | "error" | "callout",
            ..,
        ] => Domains::PALETTE,
        ["colors", ..] => Domains::COLORS,
        ["page", "footer", ..] => Domains::PAGE_NUMBERS,
        ["page", ..] => Domains::PAGE,
        ["styles", name, ..] => style_group(name),
        ["lists", ..] => Domains::LISTS,
        ["table", ..] => Domains::TABLES,
        ["code", ..] => Domains::CODE,
        ["callout", ..] => Domains::CALLOUTS,
        ["image", ..] => Domains::IMAGES,
        ["footnotes", ..] => Domains::FOOTNOTES,
        ["links", ..] => Domains::LINKS,
        _ => Domains::NONE,
    }
}

fn style_group(name: &str) -> Domains {
    match name {
        "Heading 1" | "Heading 2" | "Heading 3" | "Heading 4" | "Heading 5"
        | "Heading 6" | "Title" | "Subtitle" | "Author" | "Date"
        | "Abstract Title" | "Abstract" | "TOC Heading" | "toc 1" | "toc 2"
        | "toc 3" => Domains::HEADINGS,
        "Caption" | "Image Caption" | "Table Caption" => Domains::CAPTIONS,
        "Block Text" | "Horizontal Rule" | "Definition Term" | "Definition" => {
            Domains::QUOTES
        }
        "Source Code" => Domains::CODE,
        "Compact" => Domains::TABLES,
        "Footnote Text" | "Bibliography" => Domains::FOOTNOTES,
        "Header" | "Footer" => Domains::PAGE_NUMBERS,
        "center" | "left" | "right" | "small" | "large" => Domains::DIV_CLASSES,
        _ => Domains::NONE,
    }
}

/// The defaults files in schema order, each a `[design.<domain>]` table.
const FILES: [(&str, &str); 12] = [
    ("fonts.toml", include_str!("fonts.toml")),
    ("text.toml", include_str!("text.toml")),
    ("colors.toml", include_str!("colors.toml")),
    ("page.toml", include_str!("page.toml")),
    ("styles.toml", include_str!("styles.toml")),
    ("lists.toml", include_str!("lists.toml")),
    ("table.toml", include_str!("table.toml")),
    ("code.toml", include_str!("code.toml")),
    ("callout.toml", include_str!("callout.toml")),
    ("image.toml", include_str!("image.toml")),
    ("footnotes.toml", include_str!("footnotes.toml")),
    ("links.toml", include_str!("links.toml")),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    design: Map<String, Value>,
}

/// Each defaults file's own `[design]` table, by file name.
pub fn files() -> Vec<(&'static str, Map<String, Value>)> {
    FILES
        .iter()
        .map(|(name, text)| {
            // The files are compiled in, and a test parses each of them.
            let file: File = toml::from_str(text)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            (*name, file.design)
        })
        .collect()
}

static TABLE: LazyLock<Map<String, Value>> = LazyLock::new(|| {
    let mut table = Map::new();
    for (_, file) in files() {
        crate::theme::deep_merge(&mut table, &file);
    }
    table
});

static TEXT_SIZE: LazyLock<Pt> = LazyLock::new(|| {
    let size = TABLE["text"]["size"].as_str().unwrap_or_default();
    // A test resolves layer 0, whose text.size is an absolute length.
    super::parse_length(size).expect("layer 0 sets text.size")
});

/// The complete layer-0 `[design]` table.
pub fn table() -> &'static Map<String, Value> {
    &TABLE
}

/// Layer 0's `text.size`: what em is of where no layer sets `text.size`.
pub fn text_size() -> Pt {
    *TEXT_SIZE
}

/// Layer 0 with the leaves of the `applied` groups only, for
/// [`merge`](super::merge). Tables left empty are dropped, since an empty
/// furniture table would clear an inherited one.
pub fn layer(applied: Domains) -> Layer<'static> {
    let table = if applied == Domains::ALL {
        TABLE.clone()
    } else {
        filtered(&TABLE, &mut Vec::new(), applied)
    };
    Layer { table, base_dir: Path::new("/"), origin: Origin::Builtin }
}

fn filtered<'a>(
    map: &'a Map<String, Value>, path: &mut Vec<&'a str>, applied: Domains,
) -> Map<String, Value> {
    let mut kept = Map::new();
    for (key, value) in map {
        path.push(key);
        match value.as_object() {
            Some(child) => {
                let child = filtered(child, path, applied);
                if !child.is_empty() {
                    kept.insert(key.clone(), Value::Object(child));
                }
            }
            None => {
                let group = group(path);
                if !group.is_empty() && applied.contains(group) {
                    kept.insert(key.clone(), value.clone());
                }
            }
        }
        path.pop();
    }
    kept
}

/// The key path of every leaf below `map`: a value that is not a table, or
/// an empty table.
pub fn leaves(map: &Map<String, Value>) -> Vec<Vec<String>> {
    fn walk(
        map: &Map<String, Value>, path: &mut Vec<String>,
        into: &mut Vec<Vec<String>>,
    ) {
        for (key, value) in map {
            path.push(key.clone());
            match value.as_object() {
                Some(child) if !child.is_empty() => walk(child, path, into),
                _ => into.push(path.clone()),
            }
            path.pop();
        }
    }
    let mut into = Vec::new();
    walk(map, &mut Vec::new(), &mut into);
    into
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(path: &[String]) -> Vec<&str> {
        path.iter().map(String::as_str).collect()
    }

    #[test]
    fn files_are_disjoint_and_have_no_empty_tables() {
        let mut seen = std::collections::BTreeMap::new();
        for (name, file) in files() {
            for path in leaves(&file) {
                if let Some(other) = seen.insert(path.clone(), name) {
                    panic!("{path:?} is in both {other} and {name}");
                }
            }
        }
        for path in leaves(table()) {
            let mut node = &Value::Object(table().clone());
            for key in &path {
                node = &node[key];
            }
            assert!(
                node.as_object().is_none_or(|map| !map.is_empty()),
                "{path:?} is an empty table"
            );
        }
    }

    #[test]
    fn every_leaf_has_one_group_and_every_group_a_leaf() {
        let mut used = Domains::NONE;
        for path in leaves(table()) {
            let group = group(&strs(&path));
            assert!(!group.is_empty(), "{path:?} has no group");
            assert_eq!(group.0.count_ones(), 1, "{path:?}");
            used = used | group;
        }
        assert_eq!(used, Domains::ALL);
        let all = Domains::GROUPS
            .iter()
            .fold(Domains::NONE, |all, (_, group)| all | *group);
        assert_eq!(all, Domains::ALL);
    }

    #[test]
    fn the_layer_keeps_the_applied_groups_only() {
        assert!(layer(Domains::NONE).table.is_empty());
        assert_eq!(&layer(Domains::ALL).table, table());
        let text = layer(Domains::TEXT).table;
        assert_eq!(text.keys().collect::<Vec<_>>(), ["text"]);
        assert!(text["text"].get("size").is_some());
        assert!(text["text"].get("widows").is_none());
        let footer = layer(Domains::PAGE).table;
        assert!(footer["page"].get("footer").is_none());
        assert_eq!(layer(Domains::ALL).origin, Origin::Builtin);
        assert_eq!(text_size(), Pt(11.0));
    }

    #[test]
    fn applied_switches_on_what_its_groups_need() {
        assert!(APPLIED.contains(APPLIED.needs()));
        assert!(Domains::ALL.contains(Domains::ALL.needs()));
        assert_eq!(Domains::TEXT.needs(), Domains::NONE);
        assert!(Domains::QUOTES.needs().contains(Domains::PALETTE));
    }
}
