//! The cross-format design schema and its single, backend-independent resolver.

mod json;
mod merge;
mod parse;
mod resolve;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::assets::registry::{AssetData, AssetRegistry};
use crate::ir::Attrs;

pub use json::PathMode;
pub use merge::{absolutise_paths, merge, merged_table};
pub use parse::{parse_align, parse_color, parse_length, parse_weight};

// The unresolved serde schema mirrors the source syntax. Resolution is a
// separate pass so malformed scalars can report their complete key path.
macro_rules! spec {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[doc = concat!("Unresolved `", stringify!($name), "` design table.")]
        #[derive(Debug, Clone, Default, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name {
            $(#[doc = concat!("The optional `", stringify!($field), "` setting.")]
            pub $field: $ty,)*
        }
    };
}

spec!(DesignSpec {
    fonts: FontsSpec, text: TextSpec, colors: ColorsSpec,
    page: PageSpec, styles: BTreeMap<String, StyleSpec>,
});
spec!(FontsSpec {
    body: Option<FontSpec>, heading: Option<FontSpec>, mono: Option<FontSpec>,
});
spec!(FontDetailsSpec { family: String, files: Vec<PathBuf> });
spec!(TextSpec {
    size: Option<String>, line_height: Option<LineHeightSpec>,
    paragraph_spacing: Option<String>, justify: Option<bool>,
});
spec!(ColorsSpec {
    text: Option<String>, accent: Option<String>, muted: Option<String>,
    link: Option<String>, background: Option<String>,
});
spec!(PageSpec {
    paper: Option<String>, margin: Option<MarginSpec>,
    header_offset: Option<String>, footer_offset: Option<String>,
    print: Option<PrintMode>, background: Option<BackgroundSpec>,
    header: Option<FurnitureSpec>, footer: Option<FurnitureSpec>,
});
spec!(BackgroundSpec {
    image: Option<BackgroundImageSpec>, pages: Option<BackgroundPages>,
});
spec!(BackgroundImageDetailsSpec {
    src: String, fit: Option<Fit>, align: Option<String>,
});
spec!(MarginSidesSpec {
    top: Option<String>, right: Option<String>,
    bottom: Option<String>, left: Option<String>,
});
spec!(FurnitureSpec {
    image: Option<ImageSpec>, lines: Vec<LineSpec>, right_lines: Vec<LineSpec>,
    rule: Option<RuleSpec>, align: Option<String>, size: Option<String>,
    color: Option<String>, weight: Option<WeightSpec>, italic: Option<bool>,
    first: Option<Box<FurnitureSpec>>, rest: Option<Box<FurnitureSpec>>,
});
spec!(ImageDetailsSpec {
    src: String, width: Option<String>, align: Option<String>,
    gap: Option<String>,
});
spec!(LineDetailsSpec {
    text: String, weight: Option<WeightSpec>, italic: Option<bool>,
    size: Option<String>, color: Option<String>,
});
spec!(RuleDetailsSpec {
    position: Option<String>, color: Option<String>, width: Option<String>,
});
spec!(StyleSpec {
    font: Option<String>, size: Option<String>, line_height: Option<LineHeightSpec>,
    color: Option<String>, weight: Option<WeightSpec>, italic: Option<bool>,
    align: Option<String>, space_before: Option<String>, space_after: Option<String>,
    keep_together: Option<bool>, keep_with_next: Option<bool>,
});

/// A font role's family string or file-bearing table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FontSpec {
    /// A family name without files.
    Family(String),
    /// A family and its source files.
    Details(FontDetailsSpec),
}

/// A line pitch written as a number or a unit-bearing length.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LineHeightSpec {
    /// A font-size multiple.
    Multiple(f64),
    /// An exact length.
    Exact(String),
}

/// A weight name or integer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WeightSpec {
    /// A documented weight name.
    Name(String),
    /// An integer weight.
    Number(u16),
}

/// All margins together or independent sides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarginSpec {
    /// A shared length.
    All(String),
    /// Per-side lengths.
    Sides(MarginSidesSpec),
}

/// A furniture image's path or table with size and alignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageSpec {
    /// A source path at natural size.
    Path(String),
    /// Source path and optional geometry.
    Details(ImageDetailsSpec),
}

/// A page background image's path or table with fit and alignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackgroundImageSpec {
    /// A source path covering the page.
    Path(String),
    /// Source path and optional fit and alignment.
    Details(BackgroundImageDetailsSpec),
}

/// Plain furniture text or text with typography overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LineSpec {
    /// A plain line.
    Text(String),
    /// A line and optional typography.
    Details(LineDetailsSpec),
}

/// Furniture rule enabled by a boolean or configured in a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuleSpec {
    /// Enable a default rule or disable it.
    Enabled(bool),
    /// Position and optional colour and width.
    Details(RuleDetailsSpec),
}

/// Resolved design, with lengths in points and colours in RGB.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Design {
    /// Whether the merged design table has any keys.
    pub declared: bool,
    /// Independent optional font roles.
    pub fonts: Fonts,
    /// Body typography.
    pub text: Text,
    /// Resolved colour tokens.
    pub colors: Colors,
    /// Page geometry and furniture.
    pub page: Page,
    /// Styles keyed by their original declared names.
    pub styles: BTreeMap<String, Style>,
}

/// A length in points; serialization rounds to three decimal places.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Pt(pub f64);

/// A line pitch, either a font-size multiple or an exact length.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineHeight {
    /// A positive multiple of the current font size.
    Multiple(f64),
    /// An exact pitch in points.
    Exact(Pt),
}

/// A colour serialized as a lowercase hex string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub [u8; 3]);

/// A font weight in the range 100–900.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Weight(pub u16);

/// Paragraph or furniture alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    /// Flush left.
    Left,
    /// Centred.
    Center,
    /// Flush right.
    Right,
    /// Fully justified (paragraphs only).
    Justify,
}

/// A font family with optional source files.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Font {
    /// The family name used by all backends.
    pub family: String,
    /// Absolute font files in declared order.
    pub files: Vec<PathBuf>,
}

/// Independently optional font roles.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Fonts {
    /// Body family.
    pub body: Option<Font>,
    /// Heading family.
    pub heading: Option<Font>,
    /// Monospaced family.
    pub mono: Option<Font>,
}

/// Optional body typography settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Text {
    /// Body font size.
    pub size: Option<Pt>,
    /// Line pitch.
    pub line_height: Option<LineHeight>,
    /// Gap between paragraphs.
    pub paragraph_spacing: Option<Pt>,
    /// Whether body paragraphs are justified.
    pub justify: Option<bool>,
}

/// Resolved colour tokens; unset tokens remain unset.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Colors {
    /// Body text colour.
    pub text: Option<Color>,
    /// Accent colour.
    pub accent: Option<Color>,
    /// Muted colour.
    pub muted: Option<Color>,
    /// Link colour.
    pub link: Option<Color>,
    /// Page background colour.
    pub background: Option<Color>,
}

/// Page geometry and optional header and footer slots.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Page {
    /// A Typst paper name.
    pub paper: Option<String>,
    /// Independent page margins.
    pub margin: Margin,
    /// Distance from page top to header top.
    pub header_offset: Option<Pt>,
    /// Distance from page bottom to footer bottom.
    pub footer_offset: Option<Pt>,
    /// HTML print strategy.
    pub print: PrintMode,
    /// Full-page image behind the body and furniture.
    pub background: Option<Background>,
    /// Declared header, including an explicitly empty slot.
    pub header: Option<Slot>,
    /// Declared footer, including an explicitly empty slot.
    pub footer: Option<Slot>,
}

/// Independent optional page margins.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Margin {
    /// Top margin.
    pub top: Option<Pt>,
    /// Right margin.
    pub right: Option<Pt>,
    /// Bottom margin.
    pub bottom: Option<Pt>,
    /// Left margin.
    pub left: Option<Pt>,
}

/// HTML print furniture strategy.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum PrintMode {
    /// Generated CSS page margin boxes.
    #[default]
    MarginBoxes,
    /// Fixed-position furniture.
    Fixed,
}

/// A full-page image behind the body and the furniture.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Background {
    /// The picture and how it fills the page.
    pub image: BackgroundImage,
    /// Which pages show it.
    pub pages: BackgroundPages,
}

/// A page background image whose source has been resolved against its layer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BackgroundImage {
    /// Absolute image source.
    pub path: PathBuf,
    /// How the picture fills the page.
    pub fit: Fit,
    /// The part `cover` keeps, or where `contain` places the picture.
    pub align: BackgroundAlign,
}

/// How a page background image fills the page.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Scale to fill the page and crop the overflow.
    #[default]
    Cover,
    /// Scale to fit inside the page.
    Contain,
    /// Fill the page, ignoring the aspect ratio.
    Stretch,
}

/// Which pages show the page background.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundPages {
    /// Page one only.
    #[default]
    First,
    /// Every page.
    All,
}

/// Two-dimensional alignment of a page background image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BackgroundAlign {
    /// Left, centre or right.
    pub horizontal: Align,
    /// Top, centre or bottom.
    pub vertical: VerticalAlign,
}

impl Default for BackgroundAlign {
    fn default() -> Self {
        Self { horizontal: Align::Center, vertical: VerticalAlign::Center }
    }
}

/// Vertical alignment of a page background image.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    /// Flush with the top edge.
    Top,
    /// Centred.
    #[default]
    Center,
    /// Flush with the bottom edge.
    Bottom,
}

/// Furniture for the first and subsequent pages; `None` means empty.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Slot {
    /// First-page furniture.
    pub first: Option<Furniture>,
    /// Subsequent-page furniture.
    pub rest: Option<Furniture>,
}

/// A resolved header or footer variant.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Furniture {
    /// Optional logo or other image.
    pub image: Option<FurnitureImage>,
    /// Left/main column lines.
    pub lines: Vec<Line>,
    /// Right column lines.
    pub right_lines: Vec<Line>,
    /// Optional separator rule.
    pub rule: Option<Rule>,
    /// Overall alignment.
    pub align: Align,
    /// Default line size, falling back to backend body size.
    pub size: Option<Pt>,
    /// Default line colour.
    pub color: Option<Color>,
    /// Default line weight.
    pub weight: Weight,
    /// Default line italic setting.
    pub italic: bool,
}

/// A furniture image whose source has been resolved against its own layer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FurnitureImage {
    /// Absolute image source.
    pub path: PathBuf,
    /// Width in points, or natural size.
    pub width: Option<Pt>,
    /// Image alignment, or the furniture alignment.
    pub align: Option<Align>,
    /// Space between the image and the lines below it, in points; `None`
    /// means the backend's own paragraph spacing.
    pub gap: Option<Pt>,
}

/// A plain-text furniture line with optional typography overrides.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    /// Text with optional page placeholders.
    pub text: String,
    /// Override weight.
    pub weight: Option<Weight>,
    /// Override italic setting.
    pub italic: Option<bool>,
    /// Override size.
    pub size: Option<Pt>,
    /// Override colour.
    pub color: Option<Color>,
}

/// A horizontal furniture rule.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rule {
    /// Position relative to the furniture.
    pub position: RulePosition,
    /// Stroke colour.
    pub color: Color,
    /// Stroke width.
    pub width: Pt,
}

/// Rule position relative to furniture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RulePosition {
    /// Before all furniture content.
    Above,
    /// After all furniture content.
    Below,
}

/// A style's font role or explicit family name.
#[derive(Debug, Clone, PartialEq)]
pub enum FontRef {
    /// Design body role.
    Body,
    /// Design heading role.
    Heading,
    /// Design monospace role.
    Mono,
    /// An explicit family name.
    Family(String),
}

/// A named paragraph style shared by all output backends.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Style {
    /// Optional font role or family.
    pub font: Option<FontRef>,
    /// Font size.
    pub size: Option<Pt>,
    /// Line pitch.
    pub line_height: Option<LineHeight>,
    /// Text colour.
    pub color: Option<Color>,
    /// Font weight.
    pub weight: Option<Weight>,
    /// Italic override.
    pub italic: Option<bool>,
    /// Paragraph alignment.
    pub align: Option<Align>,
    /// Space before the containing div.
    pub space_before: Option<Pt>,
    /// Space after the containing div.
    pub space_after: Option<Pt>,
    /// Keep the div on one page.
    pub keep_together: bool,
    /// Keep the div with the following block.
    pub keep_with_next: bool,
}

/// One ordered design source, before merging or resolving values.
#[derive(Debug, Clone)]
pub struct Layer<'a> {
    /// Raw design table.
    pub table: Map<String, Value>,
    /// Directory owning relative paths in this table.
    pub base_dir: &'a Path,
    /// Origin used for contextual error reporting.
    pub origin: Origin,
}

/// Source owning a design setting.
#[derive(Debug, Clone)]
pub enum Origin {
    /// Theme manifest.
    Theme {
        /// Absolute manifest filename.
        file: PathBuf,
    },
    /// Project configuration filename.
    ProjectFile(PathBuf),
    /// Document or notebook metadata.
    FrontMatter,
}

/// A fragment of plain furniture text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Literal text.
    Text(String),
    /// Current page number.
    Page,
    /// Final page count.
    Pages,
}

impl Design {
    /// Referenced font files and furniture and background images, sorted and
    /// deduplicated.
    pub fn referenced_files(&self) -> Vec<PathBuf> {
        let mut files: BTreeSet<PathBuf> =
            self.font_files().into_iter().collect();
        files.extend(self.images().cloned());
        files.into_iter().collect()
    }

    /// Furniture images in slot order, then the page background image.
    fn images(&self) -> impl Iterator<Item = &PathBuf> {
        [&self.page.header, &self.page.footer]
            .into_iter()
            .flatten()
            .flat_map(|slot| [&slot.first, &slot.rest])
            .flatten()
            .filter_map(|furniture| furniture.image.as_ref())
            .map(|image| &image.path)
            .chain(
                self.page
                    .background
                    .as_ref()
                    .map(|background| &background.image.path),
            )
    }

    /// Font files in role/declaration order, with duplicates omitted.
    pub fn font_files(&self) -> Vec<PathBuf> {
        let mut seen = BTreeSet::new();
        [&self.fonts.body, &self.fonts.heading, &self.fonts.mono]
            .into_iter()
            .flatten()
            .flat_map(|font| font.files.iter())
            .filter(|path| seen.insert((*path).clone()))
            .cloned()
            .collect()
    }

    /// Returns a declared font role; explicit family references have no files.
    pub fn font_for(&self, role: &FontRef) -> Option<&Font> {
        match role {
            FontRef::Body => self.fonts.body.as_ref(),
            FontRef::Heading => self.fonts.heading.as_ref(),
            FontRef::Mono => self.fonts.mono.as_ref(),
            FontRef::Family(_) => None,
        }
    }

    /// Registers furniture and background images so virtual JSON and the
    /// World share IDs.
    pub fn register_images(&self, registry: &mut AssetRegistry) {
        for path in self.images() {
            registry.register_file(path);
        }
    }
}

/// Lowercase ASCII letters and digits only, for portable style matching.
pub fn normalise_style_key(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Selects a div style by custom-style first, then source-order classes.
pub fn style_for<'a>(design: &'a Design, attrs: &Attrs) -> Option<&'a str> {
    attrs
        .pairs
        .get("custom-style")
        .into_iter()
        .chain(attrs.classes.iter())
        .find_map(|name| {
            let key = normalise_style_key(name);
            design
                .styles
                .keys()
                .find(|candidate| normalise_style_key(candidate) == key)
                .map(String::as_str)
        })
}

/// Splits only the documented placeholders; unknown braces stay literal.
pub fn segments(text: &str) -> Vec<Segment> {
    let mut result = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{page") {
        let (token, length) = if rest[start..].starts_with("{pages}") {
            (Segment::Pages, 7)
        } else if rest[start..].starts_with("{page}") {
            (Segment::Page, 6)
        } else {
            let end = start + 5;
            append_text(&mut result, &rest[..end]);
            rest = &rest[end..];
            continue;
        };
        append_text(&mut result, &rest[..start]);
        result.push(token);
        rest = &rest[start + length..];
    }
    append_text(&mut result, rest);
    result
}

fn append_text(result: &mut Vec<Segment>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Segment::Text(previous)) = result.last_mut() {
        previous.push_str(text);
    } else {
        result.push(Segment::Text(text.into()));
    }
}

/// A commented reference design appended to an ejected theme.
pub const TEMPLATE_TOML: &str = include_str!("template.toml");
