//! The cross-format design schema and its single, backend-independent resolver.

pub mod defaults;
mod elements;
mod json;
mod merge;
mod parse;
mod resolve;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::assets::registry::AssetRegistry;
use crate::ir::Attrs;

pub use defaults::{APPLIED, Domains};
pub use elements::{
    Callouts, Code, Footnotes, Images, Links, Lists, Separator, TableRules,
    TableWidth, Tables,
};
pub use json::{ExportPaths, PathMode};
pub use merge::{absolutise_paths, merge, merged_table};
pub use parse::{parse_align, parse_color, parse_length, parse_weight};

// The unresolved serde schema mirrors the source syntax. Resolution is a
// separate pass so malformed scalars can report their complete key path.
// Keys added for the built-in design (R13 §4.2) follow a `;` and serialize
// only when set: the guide's schema test (tests/guide.rs) asks for a row
// per serialized key, and T122 writes those rows.
macro_rules! spec {
    (
        $name:ident {
            $($field:ident: $ty:ty),* $(,)?
            $(; $($added:ident: $added_ty:ty),* $(,)?)?
        }
    ) => {
        #[doc = concat!("Unresolved `", stringify!($name), "` design table.")]
        #[derive(Debug, Clone, Default, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name {
            $(#[doc = concat!("The optional `", stringify!($field), "` setting.")]
            pub $field: $ty,)*
            $($(#[doc = concat!("The optional `", stringify!($added), "` setting.")]
            #[serde(skip_serializing_if = "Option::is_none")]
            pub $added: $added_ty,)*)?
        }
    };
}

spec!(DesignSpec {
    fonts: FontsSpec, text: TextSpec, colors: ColorsSpec,
    page: PageSpec, styles: BTreeMap<String, StyleSpec>;
    lists: Option<ListsSpec>, table: Option<TablesSpec>,
    code: Option<CodeSpec>, callout: Option<CalloutsSpec>,
    image: Option<ImagesSpec>, footnotes: Option<FootnotesSpec>,
    links: Option<LinksSpec>,
});
spec!(FontsSpec {
    body: Option<FontSpec>, heading: Option<FontSpec>, mono: Option<FontSpec>;
    math: Option<FontSpec>,
});
spec!(FontDetailsSpec { family: String, files: Vec<PathBuf> });
spec!(TextSpec {
    size: Option<String>, line_height: Option<LineHeightSpec>,
    paragraph_spacing: Option<String>, justify: Option<bool>;
    hyphenate: Option<bool>, widows: Option<u8>, orphans: Option<u8>,
    kerning: Option<bool>, ligatures: Option<bool>,
    measure: Option<String>, screen_scale: Option<f64>,
});
spec!(ColorsSpec {
    text: Option<String>, accent: Option<String>, muted: Option<String>,
    link: Option<String>, background: Option<String>;
    border: Option<String>, code_bg: Option<String>,
    highlight: Option<String>, error: Option<String>,
    callout: Option<CalloutColorsSpec>,
});
spec!(CalloutColorsSpec {
    note: Option<String>, tip: Option<String>, important: Option<String>,
    warning: Option<String>, caution: Option<String>, other: Option<String>,
});
spec!(PageSpec {
    paper: Option<String>, margin: Option<MarginSpec>,
    header_offset: Option<String>, footer_offset: Option<String>,
    print: Option<PrintMode>, columns: Option<ColumnsSpec>,
    background: Option<BackgroundSpec>,
    header: Option<FurnitureSpec>, footer: Option<FurnitureSpec>,
});
spec!(ColumnsDetailsSpec {
    count: Option<u8>, gap: Option<String>, start: Option<ColumnStart>,
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
    frame: Option<FrameSpec>;
    case: Option<String>, letter_spacing: Option<String>,
    indent: Option<IndentSpec>, border: Option<BorderSpec>,
    fill: Option<String>, padding: Option<MarginSpec>,
});
spec!(FrameSpec {
    width: Option<String>, align: Option<String>, gap: Option<String>,
    anchor: Option<FrameAnchor>, offset: Option<String>,
});
spec!(IndentSidesSpec { left: Option<String>, right: Option<String> });
spec!(BorderSpec {
    side: Option<String>, width: Option<String>, color: Option<String>,
    gap: Option<String>,
});
spec!(ListsSpec {
    tight_spacing: Option<String>, indent: Option<String>,
    marker_gap: Option<String>, bullets: Option<Vec<String>>,
});
spec!(TableRulesSpec { outer: Option<String>, header: Option<String> });
spec!(TablesSpec {
    rules: Option<TableRulesSpec>, rule_color: Option<String>,
    inset_x: Option<String>, header_weight: Option<WeightSpec>,
    header_fill: Option<String>, width: Option<String>,
});
spec!(CodeSpec {
    background: Option<String>, padding: Option<MarginSpec>,
    radius: Option<String>, line_height: Option<LineHeightSpec>,
    theme: Option<String>,
});
spec!(CalloutsSpec {
    border_width: Option<String>, fill: Option<f64>,
    padding: Option<MarginSpec>, radius: Option<String>,
});
spec!(ImagesSpec { dpi: Option<f64> });
spec!(SeparatorSpec {
    length: Option<String>, width: Option<String>, color: Option<String>,
});
spec!(FootnotesSpec {
    separator: Option<SeparatorSpec>, gap: Option<String>,
    clearance: Option<String>,
});
spec!(LinksSpec { underline: Option<bool>, print_url: Option<bool> });

/// A font role's family string or file-bearing table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FontSpec {
    /// A family name without files.
    Family(String),
    /// A family and its source files.
    Details(FontDetailsSpec),
}

/// A style's indent: one length for both sides, or each side.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IndentSpec {
    /// The same indent on the left and the right.
    Both(String),
    /// Independent sides.
    Sides(IndentSidesSpec),
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

/// A column count, or a table with the count, gap and start.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColumnsSpec {
    /// A column count with the default gap and start.
    Count(u8),
    /// Count, gap and where the columns begin.
    Details(ColumnsDetailsSpec),
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
///
/// What the pipeline resolves is the view of the merged layers that keeps
/// layer 0's keys of the groups in [`APPLIED`] only (design.md §1.1).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Design {
    /// Whether the view's merged table has any keys. The backends,
    /// `src/typst/world.rs`, `src/assets/export.rs` and [`Design::to_json`]
    /// read this flag (R13 §1.4); while [`APPLIED`] is empty it equals
    /// [`Design::user_declared`].
    pub declared: bool,
    /// Whether a layer above layer 0 has a key. `writer check` reads this
    /// flag (`src/check/design.rs`, `src/check/targets.rs`), no backend does
    /// (R13 §1.4). Not part of the JSON.
    #[serde(skip)]
    pub user_declared: bool,
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
    /// List geometry (`[design.lists]`).
    #[serde(skip_serializing_if = "is_unset")]
    pub lists: Lists,
    /// Table rules and insets (`[design.table]`).
    #[serde(skip_serializing_if = "is_unset")]
    pub table: Tables,
    /// Code blocks (`[design.code]`).
    #[serde(skip_serializing_if = "is_unset")]
    pub code: Code,
    /// Callout boxes (`[design.callout]`).
    #[serde(skip_serializing_if = "is_unset")]
    pub callout: Callouts,
    /// Image sizing (`[design.image]`).
    #[serde(skip_serializing_if = "is_unset")]
    pub image: Images,
    /// Footnotes in paged output (`[design.footnotes]`).
    #[serde(skip_serializing_if = "is_unset")]
    pub footnotes: Footnotes,
    /// Links (`[design.links]`).
    #[serde(skip_serializing_if = "is_unset")]
    pub links: Links,
    /// The layer that set each leaf of the merged table, keyed by its key
    /// path below `[design]`. A leaf is a value that is not a table, or an
    /// empty table. Tells built-in values from the user's.
    #[serde(skip)]
    pub origins: BTreeMap<Vec<String>, Origin>,
    /// The layers above layer 0, as given, so that [`Design::view`] can
    /// merge them again under another part of layer 0.
    #[serde(skip)]
    pub sources: Sources,
}

/// The layers above layer 0 that a [`Design`] was merged from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sources(Arc<Vec<Source>>);

#[derive(Debug, Clone, PartialEq)]
struct Source {
    table: Map<String, Value>,
    base_dir: PathBuf,
    origin: Origin,
}

fn is_unset<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
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
    /// Math family.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub math: Option<Font>,
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
    /// Whether words are hyphenated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyphenate: Option<bool>,
    /// Fewest lines of a paragraph left alone at the top of a page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widows: Option<u8>,
    /// Fewest lines of a paragraph left alone at the foot of a page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orphans: Option<u8>,
    /// Whether the font's kerning applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kerning: Option<bool>,
    /// Whether the font's standard ligatures apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ligatures: Option<bool>,
    /// The longest screen line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measure: Option<Pt>,
    /// How much larger HTML sets text on screen than in print.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_scale: Option<f64>,
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
    /// Rules and borders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<Color>,
    /// Code block background.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_bg: Option<Color>,
    /// Highlighted text and code lines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<Color>,
    /// Errors such as unresolved references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Color>,
    /// One colour per callout kind.
    #[serde(skip_serializing_if = "is_unset")]
    pub callout: CalloutColors,
}

/// Callout colours by kind; unset kinds remain unset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct CalloutColors {
    /// Note callouts.
    pub note: Option<Color>,
    /// Tip callouts.
    pub tip: Option<Color>,
    /// Important callouts.
    pub important: Option<Color>,
    /// Warning callouts.
    pub warning: Option<Color>,
    /// Caution callouts.
    pub caution: Option<Color>,
    /// Callouts of any other kind.
    pub other: Option<Color>,
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
    /// Multi-column body; applied by the DOCX backend only.
    pub columns: Option<Columns>,
    /// Full-page image behind the body and furniture.
    pub background: Option<Background>,
    /// Declared header, including an explicitly empty slot.
    pub header: Option<Slot>,
    /// Declared footer, including an explicitly empty slot.
    pub footer: Option<Slot>,
}

/// A body set in columns (`page.columns`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Columns {
    /// Number of columns, 1 to 16.
    pub count: u8,
    /// Space between columns; unset keeps the backend default.
    pub gap: Option<Pt>,
    /// The first block set in columns.
    pub start: ColumnStart,
}

/// Where a multi-column body begins.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum ColumnStart {
    /// The first body block, after the title block.
    #[default]
    Body,
    /// The block after the body's first top-level thematic break, so the
    /// blocks above the rule form a full-width lead section.
    AfterRule,
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
    /// A floating frame for the div; applied by the DOCX backend only.
    pub frame: Option<Frame>,
    /// Letter case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case: Option<Case>,
    /// Extra space between letters; negative tightens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<Pt>,
    /// Left and right indents of the paragraphs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent: Option<Indent>,
    /// A border on one side or all four.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<Border>,
    /// Background colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    /// Space between the border or fill and the text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<Margin>,
}

/// A style's letter case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Case {
    /// All capitals.
    Upper,
    /// Small capitals.
    SmallCaps,
}

/// A style's left and right indents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Indent {
    /// Left indent.
    pub left: Option<Pt>,
    /// Right indent.
    pub right: Option<Pt>,
}

/// A style's border.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Border {
    /// Which side has the border.
    pub side: BorderSide,
    /// Stroke width.
    pub width: Option<Pt>,
    /// Stroke colour.
    pub color: Option<Color>,
    /// Space between the border and the text.
    pub gap: Option<Pt>,
}

/// The side of a style's border.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BorderSide {
    /// Above the paragraphs.
    Top,
    /// Below the paragraphs.
    Bottom,
    /// Left of the paragraphs.
    Left,
    /// Right of the paragraphs.
    Right,
    /// All four sides.
    #[default]
    All,
}

/// A div style's floating frame (`styles.<Name>.frame`, design.md §8.6).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Frame {
    /// Frame width; unset lets Word fit the frame to its contents.
    pub width: Option<Pt>,
    /// Horizontal place between the margins: left, center or right.
    pub align: Align,
    /// Space between the frame and the text beside it.
    pub gap: Option<Pt>,
    /// What the vertical `offset` is measured from.
    pub anchor: FrameAnchor,
    /// Vertical distance from the anchor; negative moves the frame up.
    pub offset: Pt,
}

/// What a frame's vertical position is measured from.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum FrameAnchor {
    /// The div's own place in the body.
    #[default]
    Div,
    /// The foot of the document title: Word writes a top-level div with the
    /// style right after the title paragraph.
    Title,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// writer's built-in design, layer 0 (`src/design/defaults/`).
    Builtin,
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

impl Origin {
    /// A short name for dumps: `builtin`, `theme <file>`, `project <file>`
    /// or `front matter`.
    pub fn label(&self) -> String {
        match self {
            Self::Builtin => "builtin".into(),
            Self::Theme { file } => format!("theme {}", file.display()),
            Self::ProjectFile(file) => format!("project {}", file.display()),
            Self::FrontMatter => "front matter".into(),
        }
    }
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
    pub(crate) fn images(&self) -> impl Iterator<Item = &PathBuf> {
        self.image_slots().into_iter().map(|(_, path)| path)
    }

    /// Declared images with the dotted key of their image object in the §3.3
    /// JSON: `page.header.first.image`, …, `page.background.image`.
    pub fn image_slots(&self) -> Vec<(&'static str, &PathBuf)> {
        let furniture = [
            ("page.header.first.image", &self.page.header, true),
            ("page.header.rest.image", &self.page.header, false),
            ("page.footer.first.image", &self.page.footer, true),
            ("page.footer.rest.image", &self.page.footer, false),
        ];
        let mut slots: Vec<_> = furniture
            .into_iter()
            .filter_map(|(key, slot, first)| {
                let slot = slot.as_ref()?;
                let variant = if first { &slot.first } else { &slot.rest };
                let image = variant.as_ref()?.image.as_ref()?;
                Some((key, &image.path))
            })
            .collect();
        if let Some(background) = &self.page.background {
            slots.push(("page.background.image", &background.image.path));
        }
        slots
    }

    /// Font files in role/declaration order, with duplicates omitted.
    pub fn font_files(&self) -> Vec<PathBuf> {
        let mut seen = BTreeSet::new();
        let fonts = &self.fonts;
        [&fonts.body, &fonts.heading, &fonts.mono, &fonts.math]
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

    /// This design's layers above layer 0, in merge order.
    pub fn layers(&self) -> Vec<Layer<'_>> {
        self.sources
            .0
            .iter()
            .map(|source| Layer {
                table: source.table.clone(),
                base_dir: &source.base_dir,
                origin: source.origin.clone(),
            })
            .collect()
    }

    /// The layers above layer 0 merged again over layer 0's `applied`
    /// groups. `view(APPLIED)` is what the pipeline resolves, and
    /// `view(Domains::NONE)` equals the design without layer 0.
    pub fn view(&self, applied: Domains) -> crate::error::Result<Design> {
        let mut layers = vec![defaults::layer(applied)];
        layers.extend(self.layers());
        merge(&layers)
    }

    /// The layer that set the leaf at `path` below `[design]`.
    pub fn origin(&self, path: &[&str]) -> Option<&Origin> {
        let path: Vec<String> = path.iter().map(|&key| key.into()).collect();
        self.origins.get(&path)
    }

    /// Whether the keys at and below `path` exist and all come from layer 0.
    pub fn is_builtin(&self, path: &[&str]) -> bool {
        let mut found = false;
        for (key, origin) in &self.origins {
            if key.len() >= path.len()
                && key.iter().zip(path).all(|(key, part)| key == part)
            {
                if *origin != Origin::Builtin {
                    return false;
                }
                found = true;
            }
        }
        found
    }

    /// The origin of every leaf, nested like the `[design]` table, each
    /// leaf an [`Origin::label`].
    pub fn origins_json(&self) -> Value {
        let mut root = Map::new();
        for (path, origin) in &self.origins {
            let Some((last, parents)) = path.split_last() else {
                continue;
            };
            let mut node = &mut root;
            for key in parents {
                // A leaf is a value that is not a non-empty table, so no
                // leaf's path is a prefix of another's: each parent is a
                // table this loop inserted.
                node = node
                    .entry(key.clone())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .expect("leaves are never parents of other leaves");
            }
            node.insert(last.clone(), Value::String(origin.label()));
        }
        Value::Object(root)
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
