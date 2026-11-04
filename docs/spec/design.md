# Spec: Design layer (`[design]`)

**Status:** contract. Implemented by T80 (§1–§4 schema, merge, resolution, `Prepared.design`,
`dump --design`, `eject theme`), T81 (§5 HTML tokens and fonts), T82 (§6.1–6.3, §6.5–6.7 Typst
input, `writer-doc` defaults, fonts), T83 (§7 DOCX generated styles and section), T84 (§6.4 the
emitter's `writer-div`), T85 (§8 named styles in every backend), T86 (§9.3 HTML furniture), T87
(§9.5 DOCX furniture), T88 (§9.4 Typst furniture), T89 (guide and the letter example). Consumers:
`check` (§10, extends T71's passes), live reload (§12), T74's guide pages (T89 edits them).

The design layer is one table, `[design]`, that describes a document's typography, colours, page
geometry, named paragraph styles and page furniture **once**, and that every backend applies: the
HTML theme stylesheet, the Typst PDF package and the DOCX styles and section properties. It is a
**baseline under** each backend's existing escape hatch: a theme's own CSS entries, a BYO `[pdf]
template` and a `[docx] reference_doc` keep working unchanged and keep winning (§5.4, §6.3, §7.3).
When no layer declares `[design]`, every backend produces byte-identical output to today (§0.2).

## 0. Scope and principles

### 0.1 Three parts, each landable on its own
1. **Tokens** (§2.2 `fonts`, `text`, `colors`, `page` geometry): font families with optional font
   files, body size, line height, paragraph spacing, text and accent colours, paper size, margins.
2. **Named paragraph styles** (§2.2 `styles`, §8): spacing, line height, weight, italic, alignment,
   keep-together, keep-with-next, looked up from a div's `custom-style` attribute **or** its class.
3. **Page furniture** (§2.2 `page.header`/`page.footer`, §9): per slot an image, text lines, a rule
   and page-number placeholders, with a first-page variant.

### 0.2 Invariants
- **Absent means unchanged.** A design is *declared* when the merged `[design]` map (§1.2) has at
  least one key. With no declared design nothing in §5–§9 is emitted: the HTML stylesheet, the
  `sys.inputs` map, the Typst source, `styles.xml`, the section properties and the parts list are
  exactly today's. Snapshot tests of every backend prove it (each task's acceptance).
- **Unset means the backend's default.** Every key is optional. An unset key is not emitted: HTML
  keeps the theme's CSS value, Typst keeps `writer-doc`'s built-in default (typst-dialect.md §4.2)
  and DOCX keeps the built-in `styles.xml` value (docx-output.md §2.2). Keys are independent
  (setting `fonts.body` does not change `fonts.heading`); the example in §11 sets what it needs.
- **Escape hatches win.** Theme CSS entries override design tokens (§5.4); explicit `writer-doc`
  arguments and template code override the design input (§6.3); a reference document's styles,
  section and header/footer parts override generated ones (§7.3). The design fills what the
  escape hatch leaves unsaid.
- **One resolver.** Lengths, colours, weights and style names are parsed and normalised once
  (§3) into `Design`; backends convert from points and never re-parse strings.
- **Deterministic.** The resolved JSON (§3.3) has a fixed key order; generated CSS, Typst input,
  `styles.xml` and header/footer parts are pure functions of `Design` (plus the asset registry).

## 1. Where `[design]` lives and how layers combine

### 1.1 Sources (lowest first)
1. The HTML theme chain's `theme.toml` files, parent first (`extends`, html-output.md §3.3): a
   `[design]` table in each manifest. The theme is the natural home of a design, and the only
   source that ships with a theme.
2. The project `writer.toml` `[design]` table (config.md §2).
3. Document front matter `design:` (a map; config.md §3's "override the same `writer.toml` tables"
   row gains `design:`). Notebook metadata follows the same route (`metadata.writer.design`).
   **Decision:** front matter may override the design, key by key, like every other table; the
   §11 letter does not need it, but a per-document margin or style tweak must not require a
   second theme.

CLI flags do not set design keys (no new flags). `#show: writer-doc.with(..)` arguments of a
native Typst document are not a design source either (§6.6).

### 1.2 Merge
Each source is read as a JSON object (`toml::Table` → `serde_json::Map`, the way `[vars]` is read).
Path-valued keys (§1.3) are absolutised **per source before merging**, exactly as config.md §3's
2026-10-26 note does for templates. Sources are then deep-merged in order with the `[vars]` rule
(html-output.md §3.3, `Theme::vars()`): tables merge key by key, the later source wins for a scalar
or a list (lists **replace**: a child theme that sets `fonts.body.files` replaces the parent's file
list, and `lines` of a furniture table replaces the parent's lines). A key set to TOML's absent is
not expressible; to *unset* a parent's furniture slot set it to an empty table (`[design.page.footer]`
alone), which declares an empty slot (§9.1). The merged map is validated and resolved once (§3);
an error names the key path (`design.text.size`) and, for theme sources, the file and line
(`ThemeManifest::line_of_key`).

### 1.3 Paths
`fonts.<role>.files[]`, `page.{header,footer}[.first|.rest].image` (or `.image.src`) and
`page.background.image` (or `.image.src`) are paths.
They resolve against the directory of the file that declares them: the theme directory for a
`theme.toml`, the directory of `writer.toml`, the document's directory for front matter. A theme
path must stay inside the theme directory (`theme-link-outside`, html-output.md §8); the built-in
theme never declares paths. Every resolved path is a build input (§12).

### 1.4 The theme is read for every target
Today only the HTML target resolves the theme. With `[design]` the theme chain's **manifests** are
read whenever a design may exist, for every target: `pipeline::resolve_design(prepared, opts)` (new
in `src/pipeline/mod.rs`, T80) resolves `[html] theme` with `resolve_theme`'s roots, reads the
chain's manifests and merges §1.1. When the theme cannot be resolved and `html` is **not** among the
requested targets, the failure is the warning `design-theme-unavailable` and the design is merged
from `writer.toml` and front matter alone; with `html` among the targets the existing theme error
stands. Templates, styles and scripts of the theme are not loaded for non-HTML targets. The result
is `Prepared.design: Design` (`Design::default()` when nothing is declared), computed in `prepare`
after `merge_front_matter` and before `resolve`, so emitters, the World, `check` and `dump` share it.

## 2. Schema

### 2.1 Value types
| type | TOML | parsed to | rules |
|---|---|---|---|
| Length | string `"<number><unit>"`, units `pt`, `mm`, `cm`, `in` | `Pt(f64)` points: 1 in = 72 pt, 1 mm = 72/25.4 pt, 1 cm = 10 mm | non-negative; a bare number or an unknown unit is an error: `design.text.size: expected a length with a unit such as "11pt" or "22mm"` |
| LineHeight | number (multiple of the font size) or Length (exact) | `LineHeight::Multiple(f64)` / `Exact(Pt)` | a multiple is > 0 |
| Color | string `#rgb` or `#rrggbb` (any case), or a colour token name `text`, `accent`, `muted`, `link`, `background` | `Color([u8; 3])`, printed `#rrggbb` lower-case | a token name refers to `[design.colors]`; naming an unset token is an error. CSS colour names are not accepted (DOCX needs hex) |
| Weight | `"normal"` (400), `"medium"` (500), `"semibold"` (600), `"bold"` (700), or an integer 100–900 in steps of 100 | `Weight(u16)` | DOCX: bold iff ≥ 600 (§8.3) |
| Align | `"left"`, `"center"`, `"right"`, `"justify"` | `Align` | `justify` is rejected inside furniture |
| Paper | a Typst paper name (`"a4"`, `"us-letter"`, …, config.md `[pdf] paper`) | `String` | passed through to Typst; HTML and DOCX use §5.3's table |
| FontRef | `"body"`, `"heading"`, `"mono"` or any other string (a family name) | `FontRef` | a role name takes that role's family when set, else the backend default |

Length conversions used by the backends (rounded half away from zero): CSS/Typst `<n>pt` with at
most three decimals and trailing zeros trimmed; DOCX twips = `pt × 20`, half-points = `pt × 2`,
eighths of a point (`w:pBdr` `w:sz`) = `pt × 8`, EMU = `pt × 12700`.

### 2.2 `[design]` with every key, type, unit and default
```toml
[design]                       # absent or empty: nothing is generated, every backend is unchanged

[design.fonts]                 # each role: a family string, or { family = "…", files = ["…"] }
body    = { family = "Source Sans 3",       # string; default unset (HTML: theme CSS; PDF: Libertinus Serif; DOCX: Calibri)
            files  = ["assets/fonts/SourceSans3-Regular.ttf"] }   # paths (§1.3); default []; see §4.3 for what each backend does with them
heading = "Source Sans 3"      # same shape; default unset (HTML: theme; PDF: the body font; DOCX: built-in heading fonts)
mono    = "Source Code Pro"    # same shape; default unset (HTML: theme; PDF: DejaVu Sans Mono; DOCX: Consolas)

[design.text]
size              = "11pt"     # Length; default unset (HTML: theme; PDF 11pt; DOCX 11pt)
line_height       = 1.35       # LineHeight; default unset (HTML: theme; PDF: leading 0.65em; DOCX: 1.1 = line 264)
paragraph_spacing = "3.6mm"    # Length between paragraphs; default unset (HTML: theme; PDF: par spacing 1.2em; DOCX: 0)
justify           = false      # bool; default unset (HTML: theme, i.e. off; PDF: true; DOCX: off)

[design.colors]                # Color each; default unset (HTML: theme tokens; PDF: black text; DOCX: black text)
text       = "#242424"         # body text
accent     = "#a96d4b"         # rules in furniture, HTML --w-accent
muted      = "#575757"         # HTML --w-muted; furniture text defaults to it in footers (§9.1)
link       = "#a96d4b"         # HTML --w-link; PDF `show link`; DOCX Hyperlink style colour
background = "#ffffff"         # HTML --w-bg; PDF page fill; DOCX: not applied (open point 3)

[design.page]
paper         = "a4"           # Paper; default unset → `[pdf] paper` (default "a4") for PDF and DOCX, the theme's @page for HTML
margin        = { top = "50mm", right = "22mm", bottom = "38mm", left = "22mm" }
                               # one Length (all sides) or a table of optional sides; an unset side keeps the backend default
                               # (HTML: theme @page; PDF 2.5cm; DOCX 1in)
header_offset = "17mm"         # Length from the top edge to the top of the header furniture; default unset (DOCX 0.5in; PDF: header-ascent)
footer_offset = "10mm"         # Length from the bottom edge to the bottom of the footer furniture; default unset (DOCX 0.5in)
print         = "margin-boxes" # HTML print furniture mode: "margin-boxes" (default) | "fixed" (§9.3)

[design.page.background]       # one image behind everything on the page (§9.6); default none; an empty table removes an inherited one
image = { src = "assets/cover.jpg", fit = "cover", align = "center" }
                               # path, or { src = "…", fit = "cover" | "contain" | "stretch", align = "<keywords>" }; `src` required
                               # fit default "cover"; align default "center": one or two of top, bottom, left, right, center
pages = "first"                # "first" (default): page 1 only, the cover case | "all": every page

[design.page.header]           # a furniture table (§9.1); declaring it replaces the backend's built-in header, if any
image       = "assets/logo.png"            # path, or { src = "…", width = Length, align = Align, gap = Length }; default none
                               # gap: space between the image and the lines; default text.paragraph_spacing, else the
                               # backend's paragraph spacing (HTML 1em, PDF writer-doc's spacing, DOCX document defaults) — §9.1
lines       = ["Generative Biology Institute"]   # strings, or { text = "…", weight, italic, size, color }; default []
right_lines = []               # lines set flush right beside `lines` (§9.1); default []
rule        = false            # bool, or { position = "above" | "below", color = Color, width = Length }
                               # true = accent colour, "0.5pt", below for headers, above for footers
align       = "center"         # Align without "justify"; default: header "center", footer "left"
size        = "11pt"           # Length; default: the body size (header), 0.75 × the body size (footer)
color       = "accent"         # Color; default: text (header), muted when set else text (footer)
weight      = "semibold"       # Weight; default normal
italic      = false            # default false
[design.page.header.first]     # the same keys: page 1 only; absent → page 1 uses the table above
[design.page.header.rest]      # the same keys: pages 2…; absent → they use the table above
[design.page.footer]           # same shape as header, with .first / .rest
[design.page.footer.first]
[design.page.footer.rest]

[design.styles.Recipient]      # one table per named style; the key is the style name (§8.1)
font           = "body"        # FontRef; default unset (inherits)
size           = "11pt"        # Length; default unset
line_height    = 1.2           # LineHeight; default unset
color          = "text"        # Color; default unset
weight         = "normal"      # Weight; default unset
italic         = false         # default unset
align          = "left"        # Align; default unset
space_before   = "0mm"         # Length; default unset
space_after    = "6mm"         # Length; default unset
keep_together  = true          # bool; default unset (false)
keep_with_next = false         # bool; default unset (false)
```
`first`/`rest` tables must not contain `first`/`rest` themselves (`design.page.header.first.rest:
unknown key`). Unknown keys anywhere are errors with the usual hint (§4.4). Furniture `size`,
`color`, `weight`, `italic` are the defaults for that table's lines; a line table overrides them.

### 2.3 Rust shape (`src/design/mod.rs`, T80; new module, owned by T80 → T85 adds `style_for` callers)
```rust
/// The TOML as written (after §1.2 merging), validated with serde.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DesignSpec {
    pub fonts: FontsSpec, pub text: TextSpec, pub colors: ColorsSpec,
    pub page: PageSpec, pub styles: BTreeMap<String, StyleSpec>,
}
// FontsSpec { body, heading, mono: Option<FontSpec> }, FontSpec = untagged String | { family, files }
// TextSpec, ColorsSpec, PageSpec { paper, margin: Option<MarginSpec>, header_offset, footer_offset,
// print: Option<PrintMode>, background: Option<BackgroundSpec>, header, footer: Option<FurnitureSpec> },
// MarginSpec = untagged String | sides table,
// BackgroundSpec { image: Option<BackgroundImageSpec>, pages: Option<BackgroundPages> },
// BackgroundImageSpec = untagged String | { src, fit: Option<Fit>, align: Option<String> },
// FurnitureSpec { image: Option<ImageSpec>, lines, right_lines: Vec<LineSpec>, rule: Option<RuleSpec>,
// align, size, color, weight, italic, first, rest: Option<Box<FurnitureSpec>> }, StyleSpec { … §2.2 }.
// Every struct `#[serde(default, deny_unknown_fields)]`; scalar strings stay strings here.

/// Resolved, in points; what every backend reads. `Default` = nothing declared.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Design {
    pub declared: bool,
    pub fonts: Fonts, pub text: Text, pub colors: Colors, pub page: Page,
    pub styles: BTreeMap<String, Style>,          // key = the declared name, §8.1
}
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)] pub struct Pt(pub f64);
#[derive(Debug, Clone, Copy, PartialEq, Serialize)] pub enum LineHeight { Multiple(f64), Exact(Pt) }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Color(pub [u8; 3]);   // Serialize as "#rrggbb"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] pub struct Weight(pub u16);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "lowercase")]
pub enum Align { Left, Center, Right, Justify }
#[derive(Debug, Clone, PartialEq, Serialize)] pub struct Font { pub family: String, pub files: Vec<PathBuf> }
#[derive(Debug, Clone, Default, PartialEq, Serialize)] pub struct Fonts { pub body: Option<Font>, pub heading: Option<Font>, pub mono: Option<Font> }
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Text { pub size: Option<Pt>, pub line_height: Option<LineHeight>, pub paragraph_spacing: Option<Pt>, pub justify: Option<bool> }
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Colors { pub text: Option<Color>, pub accent: Option<Color>, pub muted: Option<Color>, pub link: Option<Color>, pub background: Option<Color> }
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Page {
    pub paper: Option<String>, pub margin: Margin,
    pub header_offset: Option<Pt>, pub footer_offset: Option<Pt>, pub print: PrintMode,
    pub background: Option<Background>,                          // None = no page background (§9.6)
    pub header: Option<Slot>, pub footer: Option<Slot>,          // None = slot not declared (built-in furniture stays)
}
#[derive(Debug, Clone, PartialEq, Serialize)] pub struct Background { pub image: BackgroundImage, pub pages: BackgroundPages }
#[derive(Debug, Clone, PartialEq, Serialize)] pub struct BackgroundImage { pub path: PathBuf, pub fit: Fit, pub align: BackgroundAlign }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")] pub enum Fit { #[default] Cover, Contain, Stretch }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")] pub enum BackgroundPages { #[default] First, All }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] pub struct BackgroundAlign { pub horizontal: Align, pub vertical: VerticalAlign }   // default center/center; Justify is rejected
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)] #[serde(rename_all = "lowercase")] pub enum VerticalAlign { Top, #[default] Center, Bottom }
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)] pub struct Margin { pub top: Option<Pt>, pub right: Option<Pt>, pub bottom: Option<Pt>, pub left: Option<Pt> }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)] #[serde(rename_all = "kebab-case")] pub enum PrintMode { #[default] MarginBoxes, Fixed }
/// Resolved per page class: `first` = the `first` table else the base table; `rest` likewise.
/// `None` = that page class shows nothing (an empty slot).
#[derive(Debug, Clone, Default, PartialEq, Serialize)] pub struct Slot { pub first: Option<Furniture>, pub rest: Option<Furniture> }
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Furniture {
    pub image: Option<FurnitureImage>, pub lines: Vec<Line>, pub right_lines: Vec<Line>, pub rule: Option<Rule>,
    pub align: Align, pub size: Option<Pt>, pub color: Option<Color>, pub weight: Weight, pub italic: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize)] pub struct FurnitureImage { pub path: PathBuf, pub width: Option<Pt>, pub align: Option<Align>, pub gap: Option<Pt> }
#[derive(Debug, Clone, PartialEq, Serialize)] pub struct Line { pub text: String, pub weight: Option<Weight>, pub italic: Option<bool>, pub size: Option<Pt>, pub color: Option<Color> }
#[derive(Debug, Clone, Copy, PartialEq, Serialize)] pub struct Rule { pub position: RulePosition, pub color: Color, pub width: Pt }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "lowercase")] pub enum RulePosition { Above, Below }
#[derive(Debug, Clone, PartialEq, Serialize)] pub enum FontRef { Body, Heading, Mono, Family(String) }
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Style {
    pub font: Option<FontRef>, pub size: Option<Pt>, pub line_height: Option<LineHeight>, pub color: Option<Color>,
    pub weight: Option<Weight>, pub italic: Option<bool>, pub align: Option<Align>,
    pub space_before: Option<Pt>, pub space_after: Option<Pt>, pub keep_together: bool, pub keep_with_next: bool,
}

/// One source of §1.1: its `[design]` map and the directory its paths resolve against.
pub struct Layer<'a> { pub table: serde_json::Map<String, serde_json::Value>, pub base_dir: &'a Path, pub origin: Origin }
pub enum Origin { Theme { file: PathBuf }, ProjectFile(PathBuf), FrontMatter }
/// §1.2–1.3: absolutise paths per layer, deep-merge (lists replace), deserialize `DesignSpec`, resolve §3.
pub fn merge(layers: &[Layer<'_>]) -> Result<Design>;
pub fn parse_length(text: &str) -> Result<Pt>;           // pub for tests and `check`
pub fn parse_color(text: &str, colors: &Colors) -> Result<Color>;
/// §8.1: lower-case, keep only ASCII letters and digits.
pub fn normalise_style_key(name: &str) -> String;
/// §8.2: the declared style name a div's attributes select, if any.
pub fn style_for<'a>(design: &'a Design, attrs: &Attrs) -> Option<&'a str>;
/// §9.2: split a furniture line at `{page}` / `{pages}`.
pub fn segments(text: &str) -> Vec<Segment>;             // enum Segment { Text(String), Page, Pages }
impl Design {
    pub fn to_json(&self, paths: PathMode) -> serde_json::Value;   // §3.3; PathMode::{Real, Virtual(&AssetRegistry)}
    pub fn referenced_files(&self) -> Vec<PathBuf>;                 // font files and furniture images, sorted, deduplicated
    pub fn font_for(&self, role: &FontRef) -> Option<&Font>;       // Family(_) → None
}
```
Serde lives on `DesignSpec`; `Design` is built by hand so error messages name key paths. `Config`
gains `pub design: serde_json::Map<String, serde_json::Value>` (config.md §4; `#[serde(default)]`,
the raw `[design]` table of `writer.toml`, with front matter `design:` deep-merged into it by
`merge_front_matter`); `ThemeManifest` gains `pub design: serde_json::Map<String, serde_json::Value>`
(html-output.md §3.2). Both stay raw so one validation (`merge`) reports unknown keys and bad
values with the key path; `with_unknown_key_hint`'s wording is reused for the hint.

## 3. Resolution

### 3.1 Values
`merge` deserializes the merged map into `DesignSpec` (unknown keys → §4.4) and converts each
value with §2.1. Colour token names are resolved **after** all `[design.colors]` scalars, so
`link = "accent"` works whatever the key order; `colors.<k> = "<k>"` (self-reference) is an error.
`margin = "22mm"` sets all four sides; a table sets the sides it names. A furniture table whose
image, lines, right_lines and rule are all absent is an **empty** furniture (`Slot.first/rest =
None` for the page classes it covers), which still *declares* the slot (§9.1).

### 3.2 Defaults that depend on other keys (applied in `merge`, visible in `to_json`)
- Furniture `size`: header → `text.size` when set (else unset: the backend's body size); footer →
  `0.75 × text.size` when set, else unset (backends use 0.75 × their body size).
- Furniture `color`: header → `colors.text`; footer → `colors.muted`, else `colors.text`; unset
  when neither is set (backend text colour).
- `rule = true` → `Rule { position: Below (header) | Above (footer), color: colors.accent else
  colors.text else #000000, width: 0.5pt }`.
- `image = "path"` → `FurnitureImage { path, width: None, align: None, gap }`; a `None` width means
  the image's natural size at 96 dpi for raster images (HTML: intrinsic size; Typst: `image(..)`
  default; DOCX: pixel size × 9525 EMU), `align: None` → the furniture's `align`.
- Image `gap` (T90): `text.paragraph_spacing` when it is set and the image table has no `gap`;
  unset otherwise, and each backend then uses its own paragraph spacing (HTML `1em` of the
  furniture text, Typst `writer-doc`'s `spacing`, `1.2em` of the furniture text, DOCX the
  document defaults' `w:after`). The
  image and the lines are two paragraphs of the furniture, so they are spaced like body
  paragraphs: the §11 letter gets 3.6 mm, the distance its hand-made letterheads use.
- Style `font = "body" | "heading" | "mono"` → `FontRef::Body | Heading | Mono`; any other string →
  `FontRef::Family`.
Nothing else is derived: an unset key stays `None` (§0.2).

### 3.3 Resolved JSON (`Design::to_json`; shared by `dump --design`, the HTML context and the Typst input)
```jsonc
{
  "fonts": { "body": { "family": "Source Sans 3", "files": ["/abs/theme/assets/fonts/SourceSans3-Regular.ttf"] },
             "heading": null, "mono": null },
  "text": { "size": 11.0, "line_height": { "multiple": 1.35 }, "paragraph_spacing": 10.205, "justify": false },
  "colors": { "text": "#242424", "accent": "#a96d4b", "muted": "#575757", "link": null, "background": null },
  "page": { "paper": "a4", "margin": { "top": 141.732, "right": 62.362, "bottom": 107.717, "left": 62.362 },
            "header_offset": 48.189, "footer_offset": 28.346, "print": "margin-boxes",
            "background": { "image": { "path": "…", "fit": "cover", "align": { "horizontal": "center", "vertical": "center" } }, "pages": "first" },
            "header": { "first": { "image": { "path": "…", "width": 141.732, "align": null, "gap": 10.205 }, "lines": [ { "text": "…", "weight": null, "italic": null, "size": null, "color": null } ],
                                   "right_lines": [], "rule": null, "align": "center", "size": 11.0, "color": "#242424", "weight": 400, "italic": false },
                        "rest": { /* same shape */ } },
            "footer": { "first": { /* … */ }, "rest": { /* … */ } } },
  "styles": { "Recipient": { "font": null, "size": null, "line_height": { "exact": 12.1 }, "color": null, "weight": null, "italic": null,
                             "align": null, "space_before": null, "space_after": 11.339, "keep_together": true, "keep_with_next": false } }
}
```
Keys in struct order (above); lengths are points as JSON numbers with at most three decimals
(`(pt × 1000).round() / 1000`, so the same design always serializes to the same bytes); unset keys
are `null`, unset `Slot` members are `null`, an undeclared slot is `null`; `styles` is a
`BTreeMap` (sorted by declared name); `print` is always present; an undeclared background is `null`.
`PathMode::Real` writes absolute real paths; `PathMode::Virtual` writes the Typst virtual path
`/__assets/<id>.<ext>` of each furniture and background image registered in the `AssetRegistry`
(typst-dialect.md §6.3) and omits nothing else. `declared` is not
serialized: an undeclared design is `null` wherever the JSON appears.

## 4. CLI, errors and ejection

### 4.1 `writer dump --design <INPUT>` (cli.md `dump`, T80)
Prints §3.3 with `PathMode::Real` as pretty JSON (`serde_json::to_string_pretty`) and a trailing
newline, or `null` when no design is declared. Conflicts with the other dump flags like `--context`
does; respects `--config` and front matter; never writes outputs. `writer dump --context` shows the
same object under `theme.design` (§5.5).

### 4.2 `writer eject theme <DIR> [--design <INPUT>]` (cli.md `eject`, T80)
Without `--design`: unchanged behaviour (the built-in theme is copied), plus `theme.toml` gains the
commented reference block `design::TEMPLATE_TOML` (every key of §2.2 as a comment with its type and
default; the block, uncommented, must parse into `DesignSpec`: a unit test). With `--design INPUT`
the ejected `theme.toml` gets INPUT's **merged** `[design]` (§1.2, serialized back to TOML with
`toml::to_string_pretty` of the merged map), and every referenced file (§1.3) is copied to
`DIR/assets/design/<basename>` (a basename clash → `<stem>-<n>.<ext>`) with the paths rewritten to
those copies, so the ejected theme is self-contained. The built-in theme's own `theme.toml` does
not gain a `[design]` table (its tokens stay in `styles/theme.css`, html-output.md §7: the default
HTML output must not change). `eject typst-template`, `eject typst-package` and `eject
docx-reference` are unchanged (the template delegates to `writer-doc`, which reads the design,
§6.2; the reference document is a reference, §7.3).

### 4.3 Font files per backend
| backend | what happens with `fonts.<role>.files` |
|---|---|
| HTML | one `@font-face` per file, `src: url(..)` through the asset pipeline (§5.2); weight and style read from the file (`typst::text::FontInfo`) |
| PDF | each file is loaded into the World's font book (§6.5); the family is selected by name |
| DOCX | by name only (`w:rFonts`); the file is not embedded (open point 1) |
| LaTeX | ignored (latex-output.md is unchanged by this spec) |
A declared file that does not exist is a build error (`Error::Config`/`Error::Theme`, "design font
file not found: <path>"), because the HTML asset copy and the Typst font load would fail later with
a worse message. `check` reports it as `design-font-file` (§10).

### 4.4 Errors under `deny_unknown_fields`
Unknown keys, wrong types and bad values are reported from the merged map once, as `Error::Config`
when the offending key was contributed by `writer.toml` or front matter and as `Error::Theme` with
file and line when a theme contributed it (`Layer.origin` is kept per leaf key during the merge for
this purpose: the merge records, for every leaf path, the last layer that set it). Messages follow
`with_unknown_key_hint`: `design.text.sise: unknown key (did you mean size?)`; `design.styles.Recipient.space_after:
expected a length with a unit such as "11pt" or "22mm", got 6`. Presenter-era spellings have no
special hints. Two style names that normalise equal (§8.1) → `design.styles: "Inside Address" and
"inside-address" are the same style name`.

## 5. HTML (`src/html/design.rs` new, `src/html/styles.rs`, `src/html/context.rs`, `src/html/emit.rs`; T81, T85, T86)

### 5.1 Where the generated CSS goes
`compile_theme_css` (html-output.md §5) gains the design: the compiled theme stylesheet becomes
`hoisted @imports` + **`design::tokens_css`** + the entries (parent first, as today; the untouched
default `styles/print.css` is followed by **`design::print_colors_css`**, §5.2) +
**`design::layout_css`** + the code-theme blocks. The generated blocks are empty strings for an undeclared design (the
output is byte-identical to today). `tokens_css` starts with `/* design: tokens */\n`, `layout_css`
with `/* design: layout */\n`. The split is deliberate: tokens are *defaults* a theme's own CSS may
refine (they come first, so an entry's `:root { --w-accent: … }` still wins), while layout rules
(`@page`, named styles, furniture, paragraph spacing and justification) are explicit design
decisions that must beat the theme's generic rules of the same specificity (the default theme's
`print.css` sets `@page { size: A4; margin: 20mm 18mm }`, which would otherwise override a declared
margin). Both blocks are served and inlined like the rest of the stylesheet (`theme_style()`,
`id="writer-theme"`) and count toward nothing in §6.3's size budget (the budget is the default
theme's, which declares no design).

### 5.2 `tokens_css`
In this order:
1. `@font-face` per `fonts.<role>.files` entry (body, heading, mono order; files in declared order):
   ```css
   @font-face {
       font-family: "Source Sans 3";
       src: url(asset:…) format("truetype");
       font-weight: 400;
       font-style: normal;
       font-display: swap;
   }
   ```
   `format()` by extension: `ttf` → `truetype`, `otf` → `opentype`, `woff` → `woff`, `woff2` →
   `woff2`, else omitted. Weight and style come from the file's OS/2 table through
   `typst::text::FontInfo::iter(Bytes)` (`variant.weight.to_number()`, `variant.style`;
   Verified API hints); a file with several faces emits one rule per face, and a file from which no
   face can be read emits one rule without `font-weight`/`font-style`. The `url()` is the same
   `asset:<id>` placeholder / serve URL the entry rewriter produces (`Targets`, html-output.md §5):
   the generator registers the file with `AssetRegistry::register_file` and formats the URL through
   the existing target abstraction, so standalone, site and serve modes behave as for an entry.
2. `:root { … }` with only the declared tokens, in this order:

| key | property | value |
|---|---|---|
| `fonts.body` | `--w-font-body` | `"<family>", system-ui, sans-serif` |
| `fonts.heading` | `--w-font-heading` | `"<family>", system-ui, sans-serif` |
| `fonts.mono` | `--w-font-mono` | `"<family>", ui-monospace, monospace` |
| `text.size` | `--w-font-size` | `<pt>pt` |
| `text.line_height` | `--w-line-height` | the multiple as a number, or `<pt>pt` |
| `colors.text` | `--w-text` | `#rrggbb` |
| `colors.background` | `--w-bg` | `#rrggbb` |
| `colors.muted` | `--w-muted` | `#rrggbb` |
| `colors.accent` | `--w-accent` | `#rrggbb` |
| `colors.link` | `--w-link` | `#rrggbb` |

   Family names are quoted with `"` (an embedded `"` is escaped `\"`). The default theme's dark
   block (`@media (prefers-color-scheme: dark)`, html-output.md §7) comes later in the stylesheet
   and still wins in dark mode; a theme that wants design colours in both schemes overrides that
   block in its own CSS (open point 2). An empty `:root {}` is not emitted.

**Print colours** (`print_colors_css`). The default theme's `print.css` restores its own light
palette for paper (`@media print { :root { --w-text: #23262b; --w-accent: #366b76; … } }`) and
sets `body { background: #fff; color: #000 }`, so a printed page would lose the declared colours
that the PDF and Word keep. Directly after that sheet, when it is the embedded default or an
ejected copy with the same bytes (the rule §5.1's caller applies to `styles/theme.css`), the
stylesheet repeats the declared colour tokens for print, in the table's order, and points `body`
back at them:
```css
/* design: print colors */
@media print{:root{--w-text:#242424;--w-bg:#ffffff;--w-accent:#a96d4b}body{background:var(--w-bg);color:var(--w-text)}}
```
`background` only when `colors.background` is set (the PDF's page fill), `color` only when
`colors.text` is set; nothing at all when no colour is declared. It follows the default sheet
rather than every entry, so §5.4 still holds: a later entry (a child theme's, or an edited print
sheet, which is no longer the default) refines the colours on paper as it does on screen. The
default dark block comes earlier, so printing from a dark-mode browser keeps the declared
colours too.

### 5.3 `layout_css`
In this order, each part only when its keys are set:
1. `text.paragraph_spacing` → `[data-writer-document] p { margin-block: 0 <pt>pt; }`;
   `text.justify` → `[data-writer-document] p { text-align: justify; }` (`false` → `start`).
2. `page.paper`/`page.margin` → `@page { size: <css size>; margin: <top> <right> <bottom> <left>; }`
   with only the set descriptors; a partly set margin uses `margin-top:` etc. CSS size names:
   `a3` → `A3`, `a4` → `A4`, `a5` → `A5`, `iso-b4` → `B4`, `iso-b5` → `B5`, `jis-b4` → `JIS-B4`,
   `jis-b5` → `JIS-B5`, `us-letter` → `letter`, `us-legal` → `legal`, `us-tabloid` → `ledger`; any
   other paper name is written as its width and height in mm from Typst's paper table (Verified API hints
   `typst::layout::Paper`), e.g. `size: 210mm 297mm`.
3. Named styles (§8.3), one rule per style in `BTreeMap` order.
4. Furniture (§9.3): screen rules, then the `@page` margin-box or `fixed` rules.
5. Page background (§9.6): the `body::before` screen band, then its `@media print` box.

### 5.4 Escape hatch
Theme CSS entries are unchanged and keep `url()` rewriting only (no templating). They can override
every token (they come after `tokens_css`) and, with higher specificity or a later `@page` rule of
their own, every layout rule. A theme that extends `default` and declares `[design]` therefore gets
the default theme's components (callouts, code, tables) in the design's fonts and colours for free,
and may still restyle anything in `styles/`.

### 5.5 Context and emitter
`theme` in the template context (html-output.md §4.1) gains `"design": <§3.3 JSON> | null` (after
`scripts`). Helper `design_furniture(slot, variant)` (§9.3). The emitter's `div` (html-output.md
§1.4) adds `data-style="<declared name>"` when `design::style_for` selects a style (§8.2); the
attribute is written after `class` and before the other `data-*` pairs; `custom-style` itself stays
dropped. `EmitOptions`/`render` receive `&Design` (`Design::default()` from callers that have none,
such as the serve pages shell).

## 6. Typst (`src/typst/world.rs`, `src/pipeline/pdf.rs`, `assets/typst/writer-doc/lib.typ`, `src/typst/emit.rs`; T82, T84, T85, T88)

### 6.1 `sys.inputs.writer-design`
`world_inputs(config, design)` (typst-dialect.md §6.2 gains the parameter) inserts `writer-design`
= §3.3 JSON (`PathMode::Virtual`, compact `serde_json::to_string`) **only when a design is
declared**; the key is absent otherwise, so `"writer-design" in sys.inputs` is the test and an
undeclared design leaves the inputs map, and therefore the Libraries and the comemo cache,
unchanged. `world_inputs` stays the only producer. Furniture images are registered in the
`AssetRegistry` before the inputs are built (they are served as `/__assets/<id>.<ext>`, typst-dialect.md
§6.3). `set_inputs` sees a new map only when the design changes (§12).

### 6.2 `writer-doc` defaults from the design (`lib.typ`, T82; furniture T88; styles T85)
`from-design(key, default)` (next to `from-config`) decodes `sys.inputs.at("writer-design")` the way
`from-config` decodes `writer-config` and returns `default` for `null`/absent. An explicit
`writer-doc` argument wins over the design, which wins over the built-in default (typst-dialect.md
§4.2 gains the middle rung). Mapping:

| design key | `writer-doc` applies |
|---|---|
| `fonts.body.family` | `set text(font: F)` (replaces `"Libertinus Serif"`) |
| `fonts.heading.family` | `show heading: set text(font: F)` |
| `fonts.mono.family` | `show raw: set text(font: F)` (replaces `"DejaVu Sans Mono"`) |
| `text.size` | `set text(size: S * 1pt)` |
| `text.line_height` | `set par(leading: L)` with `L = (n − 0.8) × size` for a multiple `n`, `L = exact − 0.8 × size` for an exact length (floored at 0pt). The constant is calibrated on the §11 letter (1.35 ↔ the hand-tuned `0.55em`) and documented as an approximation: Typst's leading is a gap, not a pitch |
| `text.paragraph_spacing` | `set par(spacing: P * 1pt)` |
| `text.justify` | `set par(justify: J)` |
| `colors.text` | `set text(fill: rgb(C))` |
| `colors.link` | `show link: set text(fill: rgb(C))` |
| `colors.background` | `set page(fill: rgb(C))` |
| `page.paper` | `paper` default (`auto` → design → `writer-config.paper`) |
| `page.margin.*` | `set page(margin: (top:, right:, bottom:, left:))`, unset sides keep `2.5cm` |
| `page.header`, `page.footer` | §9.4; a declared slot sets `page(header: none, footer: none, numbering: none)` and places the furniture through `page(background:)` |
| `page.background` | §9.6; `page(background:)` places the image at the page origin before the furniture, on page 1 or on every page; a background without a declared slot leaves `numbering: "1"` and the built-in header and footer alone |
| `styles.*` | §8.4 (`writer-div`) |
| `colors.accent`, `colors.muted`, `fonts.*.files`, `header_offset`, `footer_offset`, `print` | read by furniture (§9.4) or not used by Typst (`files`: §6.5; `print`: HTML only) |

The package exposes `writer-design` (the decoded dictionary or `(:)`) for templates and documents.

### 6.3 BYO template and explicit arguments
A `[pdf] template` that delegates (`base.writer-doc(..args, body)`, typst-dialect.md §4.6) inherits
every default above. A template that builds its own layout reads `sys.inputs` itself:
```typst
#let design = if "writer-design" in sys.inputs {
    json(bytes(sys.inputs.at("writer-design")))
} else { (:) }
```
Template code and explicit arguments always win: the design is an input, never an injected `set`
rule. `check` notes which design parts a non-delegating template shadows only when it can tell
(`design-shadowed`, §10: the template does not import `@writer/doc`).

### 6.4 The emitter keeps div classes (`src/typst/emit.rs` §5.4, T84; `writer-div` in `lib.typ`)
The `Div` row of typst-dialect.md §5.4 becomes:
- no id, no classes, no `custom-style`: its blocks (unchanged);
- labelled: `#metadata(none) <id>` first (unchanged), then:
- with classes or `custom-style`: `#writer-div(style: "Recipient", classes: ("recipient",))[` … its
  blocks … `]`. `style` is the declared name `design::style_for` selects (§8.2), else the raw
  `custom-style` value when the attribute is present (DOCX parity: a reference-document style
  name still travels to Typst for a template's own `show` rules), else `none`; `classes` is a Typst array of the IR classes in order (`()` when none; one class →
  `("recipient",)`); strings escaped per §5.2. `EmitOptions` gains `design: &Design`.
Presenter layout divs (`columns`, `column`, `fragment`) are wrapped too; `writer-div` without a
matching style is the identity (`body`), so their PDF output is unchanged. `writer-div(style: none,
classes: (), body)` is exported by `@writer/doc` and allow-listed for the typst frontend
(typst-dialect.md §2.4): `writer-div(style: S, classes: C)[..]` maps back to `Div { attrs: { classes:
C, pairs: { custom-style: S } } }` (no pair when `S` is `none`), so the round trip of §7 holds. The
package version becomes `0.2.0` (a new export; `WRITER_DOC_VERSION`, the import line and the
`eject typst-package` directory follow; snapshots are regenerated once in T84).

### 6.5 Fonts
`WorldOptions` gains `font_files: Vec<PathBuf>` (= `design.referenced_files()` filtered to fonts, i.e.
`fonts.*.files`), loaded after `[pdf] font_paths` and before the system fonts (typst-dialect.md
§6.6). Individual files are read and parsed with `typst::text::Font::iter(Bytes)`; each face's
metadata and actual collection index register a lazy `typst_kit::fonts::FontPath` in the font
store. `typst_kit::fonts::scan` accepts directories, not individual files. Order matters:
embedded → `font_paths` → design files → system, so a design file never shadows the package's
embedded families. `system_fonts = false` stays byte-identical across machines when the design
ships its fonts. A
family that the book does not contain is `design-font-family` in `check` (§10),
including web-only or undecodable files. Unsupported faces keep the backend's
fallback; missing or unreadable files still produce resource errors.

### 6.6 Native `.typ` input
Nothing is injected: a native document gets `sys.inputs.writer-design` (when the project declares a
design) and `@writer/doc`'s `writer-design` dictionary, and only a `#show: writer-doc` picks the
defaults up. `writer check` applies no design lint to native input except the file checks of §10.

### 6.7 Exports (`--format typ`, `dump --typst`, typst-dialect.md §10)
The exported source is unchanged (it reads `sys.inputs`). A standalone `typst compile` without the
input compiles with the package's built-in defaults, as it does for `writer-config` today; `typst
compile --input writer-design="$(writer dump --design DOC)"` restores the design when the fonts are
on the compiler's font path. The exported tree does not copy design fonts or images (open point 5).

## 7. DOCX (`src/docx/{styles,reference,mod}.rs`, `src/docx/design.rs` new; T83, T85, T87)

### 7.1 Generated `styles.xml` (no reference document)
`StyleTable::builtin()` becomes `StyleTable::generated(design: &Design)`; `builtin()` stays as
`generated(&Design::default())` and **is byte-identical to `assets/docx/styles.xml`** (a test
compares them: the asset remains the source and the generator edits it). With a declared design
the generator rewrites these elements of the asset's XML (docx-output.md §2.2–2.3):

| design key | edit |
|---|---|
| `fonts.body` | `rPrDefault/w:rFonts` `ascii`/`hAnsi`/`cs` = family (replaces `Calibri`); Author and Date, whose `w:rFonts` name `Calibri` only to undo Title's heading font, follow it (T92) |
| `fonts.heading` | Heading 1–9, Title, Subtitle and their linked character styles = family (the asset uses both `Calibri Light` and `Calibri`) |
| `fonts.mono` | every `w:rFonts` naming `Consolas` (Source Code, Verbatim Char, …) = family |
| `text.size` | `rPrDefault` `w:sz`/`w:szCs` = half-points; styles whose `w:sz` is relative to the body in §2.3 (headings, Title, captions, footnotes, code) are scaled by `new/22` and rounded to even half-points |
| `text.line_height` | `pPrDefault/w:spacing` `w:line` = `round(n × 240)` `w:lineRule="auto"`, or twips with `w:lineRule="exact"` |
| `text.paragraph_spacing` | `pPrDefault/w:spacing` `w:after` = twips, and Body Text's `w:spacing` becomes `w:before="0"` `w:after` = twips (its §2.3 `sp 120/120` would otherwise override the default); First Paragraph, based on Body Text, inherits it. Body paragraphs are therefore the spacing apart, with none before them, as HTML's `margin-block: 0 <spacing>` and Typst's `par(spacing:)` set them. Every other §2.3 style keeps its own `sp b/a` values (Compact, Block Text, captions, headings, …); one without them inherits the default `w:after` |
| `text.justify` | `pPrDefault/w:jc` `both` when true (`left` when false) |
| `colors.text` | `rPrDefault/w:color` (inserted) = hex without `#` |
| `colors.link` | Hyperlink style `w:color` = hex |
| `colors.accent`, `muted`, `background`, `page.*` | not applied to styles (accent and muted are used by furniture, §9.5; page: §7.2) |
| `styles.*` | appended as `w:style` definitions, §8.5; a §2.3 paragraph name restyles that built-in definition in place (§8.1, T92) |

Edits are made on the parsed asset with `docx::xml` helpers, not with string replacement, and keep
§5's child order. `set_lang` is unchanged. Appended named styles go through the §2.1 id rules.

### 7.2 Section properties and parts (no reference document)
The built-in `sectPr` (docx-output.md §6.3) is generated from the design: `pgSz` from `page.paper`
(else `[pdf] paper`; §6.3's table, anything else A4); `pgMar` `top/right/bottom/left` from
`page.margin` in twips (unset sides 1440), `header` = `header_offset` twips (unset 720), `footer` =
`footer_offset` twips (unset 720), `gutter` 0; `w:titlePg` when a declared slot's `first` and `rest`
differ or `page.background.pages` is `first` (§9.6); `headerReference`/`footerReference` for the
generated parts of §9.5 and §9.6 (`type="first"` for the first-page part when `titlePg`,
`type="default"` for the rest). The built-in page-number footer
(`word/footer{k}.xml` with the `PAGE` field) is written only when **no** footer slot is declared;
a declared footer slot replaces it (an empty one removes it). `DocxOptions` gains `design: &Design`.

### 7.3 With a reference document (`[docx] reference_doc`): what wins
| part | reference present | reference absent |
|---|---|---|
| `docDefaults`, Normal, Body Text, headings, code, every §2.3 name the reference defines | the reference, verbatim (docx-output.md §6.3); tokens of §7.1 are **not** applied to them | generated §7.1 |
| §2.3 names the reference lacks | writer's built-in definition with §7.1 edits applied | generated §7.1 |
| `[design.styles]` names | a reference style of the same name wins entirely; the design definition is appended only for names the reference lacks (§8.5) | appended |
| body-level `sectPr` | the reference's, raw, with its header/footer references (page size, margins, offsets, `titlePg`); `page.*` and furniture are not applied | generated §7.2 |
| header/footer parts and media | copied from the reference when its `sectPr` references them; none generated | generated §9.5 |
| fonts | by name in both cases (§4.3) | by name |
A reference with styles but **no** body-level `sectPr` gets the generated section and furniture
(today it gets the built-in section). `check --target docx` lists what the reference shadows as
`design-shadowed` (§10), once per document, so the author knows why a token did not land.

### 7.4 Fonts
Names only: `w:rFonts` with the family in `ascii`, `hAnsi` and `cs`; no `fontTable.xml` entry and
no embedding (open point 1). Word substitutes a missing font on the reader's machine as it does for
any document.

## 8. Named paragraph styles (T85; `writer-div` in T84's package)

### 8.1 Names and keys
The TOML key of `[design.styles.<Name>]` is the style's declared name, used verbatim as the DOCX
`w:name`, the HTML `data-style` value, the Typst `style:` argument and the JSON key. Two names are
the same style when `normalise_style_key` (lower-case ASCII letters and digits only: `Inside
Address`, `inside-address`, `InsideAddress` → `insideaddress`) agrees; equal keys are an error
(§4.4). A design style whose name is one of docx-output.md §2.3's paragraph vocabulary names
(`Body Text`, `Heading 1`, `TOC Heading`, `toc 2`, `Date`, …; compared by normalised key, so
`Heading-1` is `heading 1`) restyles writer's built-in definition for DOCX instead of adding one
(T92): the §2.3 id, `basedOn`, `next`, `link` and `uiPriority` stay, and the design's keys are laid
over the built-in `w:pPr`/`w:rPr` as §8.5 describes, so every property the design leaves unset
keeps its built-in value. `TOC Heading` and `Heading N Unlisted` therefore keep `outlineLvl 9` and
stay out of the TOC field (docx-output.md §4.13), `toc N` keeps its indent, and `Title` stays
centred until `align` says otherwise. Writer's own emitters select such a style by name (headings,
the title block, the contents list, captions, footer lines), so `design-style-unused` does not
report it (§10); a div may select it as well. Otherwise it behaves like any style; this is how a
letter restyles `Date`. The header furniture's lines select `Header` (`docx::styles::HEADER`,
§9.5), which §2.3 does not list: a design style of that name (compared case-insensitively) is a
custom definition those lines use, and `design-style-unused` does not report it either.

### 8.2 Resolution order (one function, `design::style_for`, every backend)
For a `Div` (ir.md: `attrs.classes`, `attrs.pairs`):
1. `attrs.pairs["custom-style"]`, when its normalised key names a design style → that style.
2. Else the first class in source order whose normalised key names a design style → that style.
3. Else none: HTML emits the div as today; DOCX applies `custom-style` through the existing
   `custom(name, Paragraph, None)` lookup when the attribute is present (a reference style or an
   empty definition, docx-output.md §4.1), otherwise nothing; Typst's `writer-div` receives the raw
   `custom-style` as `style` (§6.4) and is the identity for a name that is not a design style.
A `custom-style` that is not a design style therefore keeps today's meaning everywhere, and a
`:::recipient` div and a `:::x {custom-style="Recipient"}` div format identically in all three
outputs. The resolver applies to `Div` only (callouts, figures and cells keep their own styles).

### 8.3 HTML (`layout_css`, §5.3 item 3)
One rule per style, selector `[data-style="<name>"]` (the name escaped with `CSS.escape` rules:
`"` and `\` backslash-escaped), declarations in this order, each only when set:
`font-family` (`var(--w-font-body|heading|mono)` for a role, `"<family>"` otherwise), `font-size`,
`line-height`, `color`, `font-weight`, `font-style: italic|normal`, `text-align`, `margin-top`
(`space_before`), `margin-bottom` (`space_after`), `break-inside: avoid` (`keep_together`),
`break-after: avoid` (`keep_with_next`). Spacing applies to the div, so a multi-paragraph div keeps
the theme's paragraph gaps inside; the last paragraph's bottom margin is not removed (open point 4).

### 8.4 Typst (`writer-div` in `@writer/doc`)
```typst
#let writer-div(style: none, classes: (), body) = {
    let spec = if style != none { writer-design.at("styles", default: (:)).at(style, default: none) } else { none }
    if spec == none { return body }
    let inner = body
    // set text(font:, size:, fill:, weight:, style:), set par(leading:, justify:), align(..)
    // per spec key, then
    block(
        above: space_before, below: space_after,   // default auto
        breakable: not keep_together,
        sticky: keep_with_next,
        width: 100%,
        inner
    )
}
```
`font: "body"|"heading"|"mono"` → that role's family when the design sets it, else no `set`;
`align` → `align(left|center|right)(..)` or `set par(justify: true)`; `line_height` uses §6.2's
leading formula with the style's size (else the design's, else 11pt). `block(sticky: true)` is
Typst's keep-with-next (Verified API hints). The function is also what §6.4's emitter calls for styleless
divs (`spec == none` → `body`).

### 8.5 DOCX
Each design style is appended to `styles.xml` (§7.1) as
```xml
<w:style w:type="paragraph" w:customStyle="1" w:styleId="Recipient">
    <w:name w:val="Recipient"/>
    <w:basedOn w:val="BodyText"/>
    <w:qFormat/>
    <w:pPr>
        <w:keepNext/>                 <!-- keep_with_next -->
        <w:keepLines/>                <!-- keep_together -->
        <w:spacing w:before="0" w:after="340" w:line="242" w:lineRule="exact"/>  <!-- set keys only -->
        <w:contextualSpacing/>        <!-- space_before or space_after set, on a block style (below) -->
        <w:jc w:val="left|center|right|both"/>
    </w:pPr>
    <w:rPr>
        <w:rFonts w:ascii="…" w:hAnsi="…" w:cs="…"/>  <!-- font: a role's family, or the family; omitted for an unset role -->
        <w:b/><w:bCs/>                <!-- weight ≥ 600; weight < 600 set explicitly → <w:b w:val="0"/> -->
        <w:i/><w:iCs/>                <!-- italic true; false set explicitly → w:val="0" -->
        <w:color w:val="242424"/>
        <w:sz w:val="22"/><w:szCs w:val="22"/>
    </w:rPr>
</w:style>
```
in §5's child order, id from the §2.1 rules, `basedOn` the table's id for `Body Text`. The XML is
built over the table's registered definition for the name (T92): for a §2.3 paragraph name that is
writer's built-in definition with §7.1's edits applied, for any other name the empty `custom`
definition, so a custom style carries the design's keys alone. A set key replaces the child of the
same name in that definition's `w:pPr`/`w:rPr` (`keepNext`, `keepLines`, `contextualSpacing`,
`jc`, `rFonts`, `b`/`bCs`, `i`/`iCs`, `color`, `sz`/`szCs`); `w:spacing` merges attribute by
attribute (`before`, `after`, `line` with `lineRule`), so `toc 2` with `space_after` alone keeps
its `before="0"`; every other child (`outlineLvl`, `ind`, `tabs`, `pBdr`, `shd`, …) is kept.
`keep_with_next` and `keep_together` are booleans: true adds the element, false (the default)
leaves the built-in's alone. `w:contextualSpacing` makes consecutive paragraphs of the same
style omit the gap between them. It is added only to a **block style**, one whose paragraphs
together form one block: a custom name (a div's paragraphs; `Header`, the header furniture's
lines) and `footer` (the footer furniture's lines). There `space_before`/`space_after` behave as
the block's outer margins, like a div's in HTML and Typst. Writer selects every other §2.3 name
per block (each body paragraph, heading, contents entry, caption, table-cell or tight-list
paragraph is a block of its own), so those keep the built-in definition's setting (none in
writer's asset) and their space separates consecutive paragraphs of the same style:
`[design.styles."Body Text"]` with `space_after` keeps body paragraphs apart, and `toc 1` with
`space_before` spaces consecutive top-level contents entries. The body emitter (docx-output.md §4.1) pushes `Style(id)` for a `Div` when
`style_for` selects a style, with `id = custom(name, Paragraph, None)` (the definition already
exists, so `custom` returns its id). With a reference document, §7.3.

## 9. Page furniture (T86 HTML, T87 DOCX, T88 Typst; T90 image gap, T91 page background)

### 9.1 Model
A **slot** is `page.header` or `page.footer`; a slot has a **first-page** and a **rest** furniture
(§2.2, §3.1). A furniture lays out, top to bottom: the image (when any), the lines (`lines` as a
left column and `right_lines` as a right column when the latter is non-empty; otherwise one column
aligned by `align`), and the rule at its `position` (above everything for `above`, below for
`below`). The image's `gap` (§2.2; the paragraph spacing by default, §3.2) is the vertical space
between its bottom edge and the top of the first line's box (T90). That box starts at the cap
height in Typst, at the ascender plus half the leading in HTML, and in Word below the image line's
descent and the line spacing Word adds above the text. The same value therefore looks larger in
HTML and larger again in Word: in the §11 letter the line's capitals sit 0, 1.1 and 1.5 mm below
the gap's end (PDF, HTML screen, Word's own PDF export; measured 4 Nov 2026, after the image
paragraph took single line spacing, §9.5; Word's was 3.4 mm before). Line text is plain
text with §9.2 placeholders; no inline markup (open point 6). Lines
inherit the furniture's `size`, `color`, `weight`, `italic` unless they set their own. The
furniture's width is the text width (page width − left − right margin); the header's top edge is at
`header_offset` from the page top, the footer's bottom edge at `footer_offset` from the page bottom
(backends that cannot honour an offset exactly say so below). Declaring a slot replaces the
backend's built-in furniture for that slot (Typst's `numbering: "1"` footer, DOCX's `PAGE` footer,
the default theme's `@bottom-center { content: counter(page) }`), including with an empty furniture.
The page background of §9.6 is not a slot: it is painted behind both slots and declares nothing.

### 9.2 Placeholders
`{page}` and `{pages}` inside a line's text become the current page number and the page count.
`segments` splits a line into `Text`/`Page`/`Pages`; any other `{…}` is literal text. Backends:
HTML print → `counter(page)` / `counter(pages)` in the `content:` string list; HTML screen → `1` /
`1` (one continuous page); DOCX → `PAGE` / `NUMPAGES` fields through `docx::xml::formatted_field`
(cached result `1`; the begin, instruction, separator and end runs carry the line's run properties,
because Word formats a recomputed result like the field's runs, not like its cached result); Typst → `context counter(page).display()` / `context counter(page).final().first()`.

### 9.3 HTML (`design_furniture` helper, `layout_css` item 4; T86)
**Screen.** The context helper `design_furniture(slot, variant)` (`slot` = `"header" | "footer"`,
`variant` = `"first" | "rest"`) returns the furniture's HTML or `""`:
```html
<div class="w-furniture w-header w-header-first" data-align="center">
    <img class="w-furniture-image" src="asset:…" alt="" style="width:141.732pt;margin-bottom:10.205pt">
    <div class="w-furniture-lines">
        <p class="w-furniture-line" style="font-weight:600;color:#a96d4b">Generative Biology Institute</p>
    </div>
    <div class="w-furniture-lines w-furniture-right">…</div>
    <hr class="w-furniture-rule" style="border-color:#a96d4b;border-width:0.5pt">
</div>
```
(the rule precedes the image for `above`; `style` attributes carry only set values, except the
image's `margin-bottom`, its `gap` or `1em` when unset; the image is registered as an asset).
The default theme's `document.html` (T86's one asset change) calls
`{{ design_furniture("header", "first") }}` before `html.main` and `{{ design_furniture("footer",
"first") }}` after it; an undeclared design renders nothing, so the template's output is unchanged.
Screen CSS in `layout_css`: `.w-furniture { display: grid; grid-template-columns: 1fr auto; gap: 0 1em; … }`,
`.w-furniture[data-align=…]`, `.w-furniture-line { margin: 0; }`, and `@media print { .w-furniture
{ display: none; } }` so the print path below owns printing. Themes may style or hide the classes.

**Print, `print = "margin-boxes"` (default).** For each slot and page class, a margin-box rule:
`@page :first { @top-center { … } }` for `first` and `@page { @top-center { … } }` for `rest`; an
empty furniture writes `content: none` so the theme's own margin-box rule does not show through.
The box is `@top-left` / `@top-center` / `@top-right` by `align` (`@bottom-*` for the footer); with
`right_lines` the left lines go to `-left` and the right lines to `-right` regardless of `align`.
Each box: `content: "line 1\A line 2" counter(page) "…"` (segments concatenated; `\A` between
lines; `white-space: pre-line`), `font: <weight> <italic> <size>/<line-height> <family>`, `color`,
`text-align`, `vertical-align: top` plus `margin-top: <header_offset>` for top boxes and
`vertical-align: bottom` plus `margin-bottom: <footer_offset>` for bottom boxes; no `width` (margin
boxes size themselves from their content). The image becomes `background: url(…)
no-repeat top center / <width> auto` with `padding-top: <image height + gap>` (height from the image's
pixel size and the width, read with `imagesize::blob_size` as `src/docx/images.rs` does, Verified API hints;
`calc(<height> + 1em)` when the gap is unset), the rule a
`border-top`/`border-bottom: <width> solid <color>` with `padding` to separate it. Per-line
weight/colour cannot be expressed in one `content:` string: lines whose own `weight`, `italic`,
`size` or `color` differ from the furniture's are rendered with the furniture's values in margin
boxes (documented limitation; `fixed` mode honours them). The margins of `@page` come from §5.3
item 2, so the boxes have room; the reference engine is Chromium's print (presenter's).

**Print, `print = "fixed"`.** No margin boxes; the screen furniture of the **`rest`** variant is
also printed with `position: fixed; left: 0; right: 0; top: <header_offset>` (`bottom:
<footer_offset>`), which Chromium repeats on every printed page, and the `first` variant prints
once in flow. Placeholders print as `1`. Choose it for furniture with images per line or mixed
weights; choose margin boxes (default) for page numbers and distinct first pages.

### 9.4 Typst (`writer-doc`, `writer-furniture` in `lib.typ`; T88)
A declared slot sets `page(header: none, footer: none, numbering: none)` and
`page(background: context { … })` places both slots (the way the §11 letter's hand-written
`letterhead.typ` does, which keeps offsets exact):
```typst
#let writer-furniture(spec, text-width, spacing: 1.2em) = {
    // spec: the §3.3 furniture dictionary; returns content: optional rule above,
    // block(below: gap, image(spec.image.path, width:)) with gap = spec.image.gap * 1pt or
    // `spacing` (the document's paragraph spacing) when null, grid(columns: (1fr, auto)) of
    // lines / right_lines joined by linebreak(), optional rule below; text(size:, fill:,
    // weight:, style:) per line.
}
#set page(background: context {
    let first = counter(page).get().first() == 1
    let header = if first { design.page.header.first } else { design.page.header.rest }
    if header != none {
        place(top + left, dx: margin.left, dy: header-offset,
              block(width: text-width, writer-furniture(header, text-width, spacing: settings.spacing)))
    }
    let footer = …
    if footer != none {
        place(bottom + left, dx: margin.left, dy: -footer-offset,
              block(width: text-width, writer-furniture(footer, text-width, spacing: settings.spacing)))
    }
})
```
An unset `header_offset` is 30 % of the top margin (Typst's own `header-ascent` default, expressed
as an offset from the edge); an unset `footer_offset` is 30 % of the bottom margin.
Rules: `line(length: 100%, stroke: width * 1pt + rgb(color))`; `{page}`/`{pages}` per §9.2; `align`
→ `align(..)` inside the block. Rules occupy a separate block with a 0.3em
clearance from adjacent text. Image paths are the virtual `/__assets/…` paths
of §3.3. The World mounts those reserved paths for both project code and
`@writer/doc`, whose image calls otherwise retain the package root.

### 9.5 DOCX (`src/docx/design.rs`, `reference.rs`; T87)
Generated parts: `word/header<k>.xml` and `word/footer<k>.xml` with k the smallest free index
(docx-output.md §6.2's rule), one part per **distinct** furniture: the `first` part referenced
`w:type="first"` and the `rest` part `w:type="default"` when they differ (then `w:titlePg`), else one
`default` part; an empty furniture is an empty part (`<w:hdr><w:p/></w:hdr>`), which is how a
declared empty footer removes the page number. Since `titlePg` applies to both
slots, a shared slot also references its single part as `first` when the other
slot differs. Retained `evenAndOddHeaders` settings similarly alias continuation
parts as `even`. The rule and image paragraphs take the slot's style (`w:pStyle` Header / Footer,
as the lines do) and set their own spacing explicitly, so neither that style nor the document
defaults (`text.paragraph_spacing` after, `text.line_height`, a body-size paragraph mark) push the
lines away from them: `w:spacing w:before="0"`, `w:contextualSpacing w:val="0"` (a style with
space keys sets it, §8.5, which would drop the image's gap before a line of the same style) and a
one-point paragraph mark, `w:rPr/w:sz w:val="2"`. Part body, in order:
- rule `above`: a paragraph with `w:pPr/w:pBdr/w:top w:val="single" w:sz="<eighths>" w:space="1" w:color="rrggbb"`,
  `w:spacing w:after="0" w:line="20" w:lineRule="exact"` and a `w:sz w:val="2"` empty run (the
  reference document's own construction: a one-point paragraph, so the rule sits `w:space` from
  the adjacent line as it does in the PDF and HTML);
- image: a paragraph (`w:jc` from the image's or furniture's align; `w:spacing w:after` from the
  image's resolved `gap`, else the document defaults' `w:after`, and `w:line="240"
  w:lineRule="auto"`, so the line is as high as the picture) with an inline drawing in a
  one-point run
  (`wp:inline`, extent EMU from `width` and the image's aspect ratio; `wp:docPr/@id` from
  `Ids.doc_pr`; `r:embed` → `word/_rels/header<k>.xml.rels` → `word/media/<name>`; content type
  by extension as docx-output.md §4.16 does for figures);
- lines: one `w:p` per line of `lines`, `w:pStyle` Header / Footer (`ensure`d; `footer` is a §2.3
  name, `Header` is a custom style based on Body Text unless the design, §8.1, or a reference defines it),
  `w:jc` from `align`; with `right_lines`, line *i* of `lines` is followed by `<w:r><w:tab/></w:r>`
  and the runs of line *i* of `right_lines`, with `w:pPr/w:tabs/w:tab w:val="right" w:pos="<text
  width twips>"` (the reference document's layout); extra right lines get paragraphs of their own
  with the tab only. Runs carry `w:rFonts` (furniture font = body font when set), `w:sz`, `w:color`,
  `w:b`, `w:i` from the line's resolved values; `{page}`/`{pages}` are `PAGE`/`NUMPAGES` fields
  whose every run carries those properties (§9.2);
- rule `below`: as above with `w:pBdr/w:bottom`.
`sectPr` references and offsets: §7.2. With a reference document whose `sectPr` references parts,
nothing is generated (§7.3).

### 9.6 Page background (`page.background`; T91)
**Model.** `page.background.image` is one image drawn at the page origin at the full page size,
behind the body, the furniture and whatever built-in furniture remains. `fit` is `cover` (scale to
fill the page and crop the overflow; default), `contain` (scale to fit inside the page; the page
colour shows elsewhere) or `stretch` (fill the page, ignoring the aspect ratio). `align` is one or
two of `top`, `bottom`, `left`, `right`, `center` in any order (`"center"` by default): the part of
the picture that `cover` keeps, and where `contain` places the picture; `stretch` ignores it.
`pages = "first"` (default) paints page 1 only, the cover case; `"all"` paints every page. The path
follows §1.3 and is a build input (§12); `referenced_files` and `register_images` include it, so
`design-image-missing`, `eject theme --design` and the Typst World treat it like a furniture image.
An empty `[design.page.background]` table removes an inherited background (as an empty slot does)
and resolves to `None`; a non-empty table must name an image. Nothing else changes: a background
declares no slot, so Typst keeps `numbering: "1"` and DOCX keeps the `PAGE` footer unless a slot
says otherwise.

**HTML** (`background_css`, `layout_css` item 5). Pure CSS on `body::before`, so every template
gets it without a helper call. Screen: `content:""; display:block; height:<paper height>;
margin-bottom:-<paper height>; background:url(<asset>) no-repeat <horizontal> <vertical> /
<size>; print-color-adjust:exact; -webkit-print-color-adjust:exact`, with `<size>` `cover`,
`contain` or `100% 100%` by `fit`, paints the first paper height of the document (`page.paper`,
A4 when unset) behind the content that follows it. Print: `@media print { body::before {
position:absolute; top:0; left:0; width:100%; height:100%; margin:0 } }` for `first` (one box, on
the first page) and `position:fixed` for `all` (Chromium repeats fixed boxes on every page, as the
furniture `fixed` mode relies on); both assume a theme leaves `html` and `body` unpositioned, so
the box's containing block is the page area. Limitation: browsers ignore `background` on
`@page :first`, and absolute and fixed boxes are confined to the page area, so the printed image
stops at the `@page` margins; a theme that wants a bleed to the paper edge sets
`@page { margin: 0 }` (or `@page :first { margin: 0 }`) in its print CSS and pads the content
itself.

**Typst** (`background-image` in `lib.typ`; §9.4's `furniture-background`). Before the header and
footer, `place(top + left, box(width: page.width, height: page.height, background-image(spec)))`:
`image(path, width: 100%, height: 100%, fit:)` for `stretch` and for a centred `cover` or
`contain`; an off-centre alignment measures the image (`measure(image(path))`), scales it by the
larger (`cover`) or smaller (`contain`) of the page-to-image ratios and places it at `(page −
image) × factor` (0, 0.5 or 1 per axis) inside a `box(clip: true)`; Typst would otherwise cap a
picture with one forced dimension at the region and centre it. `pages` is tested with
`counter(page).get().first() == 1`.
When no slot is declared, `furniture-layout` sets `page(background:)` alone, so the built-in header,
footer and numbering stay.

**DOCX** (`background_run` and `with_anchor` in `src/docx/furniture.rs`). The picture is an
anchored drawing in the header part, the way Word stores a "behind text" picture: one run with
`<w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0"
behindDoc="1" locked="0" layoutInCell="1" allowOverlap="1">`, `wp:simplePos 0 0`, `wp:positionH`
and `wp:positionV` `relativeFrom="page"` with `wp:posOffset` 0 (`contain`: the aligned offset),
`wp:extent` the page size in EMU (`pgSz` twips × 635; `contain`: the fitted size), `wp:effectExtent`
0, `wp:wrapNone`, `wp:docPr` (`Ids.doc_pr`) and the §9.5 picture body, with `a:srcRect` cropping
(thousandths of a percent) for `cover`. The run opens the part's first paragraph, right after its
`w:pPr`, so the furniture keeps its place; a part without furniture holds one paragraph of Word's
minimum exact line (`w:spacing w:before="0" w:after="0" w:line="14" w:lineRule="exact"`, 0.7 pt)
with the run. The media part and relationship are registered like furniture images (§9.5).
`pages = "first"` forces `w:titlePg` (§7.2) and a first-page header part even when no header slot is
declared: an undeclared slot gets a part holding the picture alone, referenced `first`; a declared
slot whose `first` and `rest` agree is split into a first part with the picture and a default part
without it. `pages = "all"` puts the picture into every header part (a part of its own, referenced
`default` and, under `titlePg`, `first`, when no slot is declared). `furniture::generate` runs
whenever a background is declared, even without slots. With a reference document whose `sectPr`
references parts, nothing is generated (§7.3).

## 10. Diagnostics (`src/check/design.rs` new; codes in the `check` report, T80 for the first four, T85 for `design-style-unused`, T83/T82 for `design-shadowed`)
| code | level | when |
|---|---|---|
| `design-font-file` | error | a `fonts.<role>.files` path does not exist (also a build error, §4.3) |
| `design-image-missing` | error | a furniture or page background image path does not exist (also a build error) |
| `design-theme-unavailable` | warning | §1.4: the theme could not be resolved for a non-HTML target; the design was merged without it |
| `design-font-family` | warning | `check --target pdf`: a declared family (with no files) is not in the World's font book after §6.5 (`FontBook::contains_family` queried with a lower-case family name, matching Typst font selection) |
| `design-style-unused` | warning | a `[design.styles]` name that no `Div` of the document selects (§8.2) and that writer's own paragraphs do not select (`docx::styles::writer_selects`: a docx-output.md §2.3 paragraph name or `Header`, §8.1); evaluated on the resolved IR |
| `design-shadowed` | note | per target: a `[docx] reference_doc` whose `sectPr` or style names override design keys (names the parts: "page setup", "styles: Recipient, Date", "header/footer"); a `[pdf] template` that does not import `@writer/doc` while a design is declared |
Config errors of §4.4 fail `check` and `build` alike. `check` without `--target` runs the
target-specific rows for every configured target.

## 11. Worked example: the Letters letterhead
The Letters template (an HTML theme with a hand-written `document.html`, `letter.css`, a Word
`reference.docx` and a hand-written `letterhead.typ`) expressed as one design. Geometry:

| quantity | design | pt | twips (DOCX) | source today |
|---|---|---|---|---|
| body size | `"11pt"` | 11 | sz 22 | `letter.css` `html { font-size: 11pt }`, Normal sz 22, `text(size: 11pt)` |
| line height | `1.35` | leading 0.55 em (§6.2) | line 324 auto | `line-height: 1.35`; `par(leading: 0.55em)` |
| paragraph spacing | `"3.6mm"` | 10.205 | after 204 | `main p { margin: 0 0 3.6mm }` (reference: after 160) |
| margins | top `"50mm"`, sides `"22mm"`, bottom `"38mm"` | 141.732 / 62.362 / 107.717 | 2835 / 1247 / 2154 | `@page { margin: 50mm 22mm 38mm }`; `pgMar` of `reference.docx` |
| header offset | `"17mm"` | 48.189 | header 964 | `.letterhead { top: 17mm }`; `pgMar/@header="964"` |
| footer offset | `"10mm"` | 28.346 | footer 567 | `.legal-footer { bottom: 10mm }` (reference: 737) |
| logo width | `"50mm"` | 141.732 | 1800000 EMU | `.letterhead img { width: 50mm }`; `wp:extent cx="1800000"` |
| logo to institute line | unset (`gap`): the paragraph spacing | 10.205 | after 204 | `.institute-name { margin-top: 3mm }`; header `w:spacing w:after="120"`; `letterhead.typ` places the line 4 mm below the logo |
| ink / copper / footer grey | `#242424` / `#a96d4b` / `#575757` | | | `--ink`, `--copper`, `.legal-footer { color: #575757 }` |

`theme/theme.toml` of the letter theme (paths relative to the theme directory):
```toml
name        = "letterhead"
description = "Institute letterhead: first-page logo, compact continuation header, legal footer"

[styles]
entries    = ["styles/letter.css"]   # screen sheet, link colours, anything the design does not express
code_theme = "none"

[design.fonts]
body    = { family = "Source Sans 3", files = [
    "assets/fonts/SourceSans3-Regular.ttf", "assets/fonts/SourceSans3-Italic.ttf",
    "assets/fonts/SourceSans3-SemiBold.ttf", "assets/fonts/SourceSans3-Bold.ttf",
] }
heading = "Source Sans 3"

[design.text]
size              = "11pt"
line_height       = 1.35
paragraph_spacing = "3.6mm"
justify           = false

[design.colors]
text   = "#242424"
accent = "#a96d4b"
muted  = "#575757"
link   = "accent"

[design.page]
paper         = "a4"
margin        = { top = "50mm", right = "22mm", bottom = "38mm", left = "22mm" }
header_offset = "17mm"
footer_offset = "10mm"

[design.page.header.first]
image  = { src = "assets/logo.png", width = "50mm" }
lines  = ["Generative Biology Institute"]
color  = "accent"
weight = "semibold"

[design.page.header.rest]
lines  = ["EIT Oxford | Generative Biology Institute"]
size   = "10pt"
color  = "accent"
weight = "semibold"

[design.page.footer]
rule  = { position = "above", color = "accent", width = "0.25mm" }
size  = "7.5pt"
lines = [
    { text = "Ellison Institute of Technology, Oxford Limited", weight = "bold" },
    "Registered Office: Winchester House, The Oxford Science Park,",
    "Heatley Road, Littlemore, Oxford, OX4 4GE, United Kingdom.",
    "Company No. 13773507 | Registered in England and Wales",
]
right_lines = ["+44 1865 649525", "oxford@eit.org"]

[design.styles.Recipient]
space_after   = "6mm"
line_height   = "12.1pt"
keep_together = true

[design.styles."Inside Address"]
keep_together = true

[design.styles.Salutation]
space_before = "6mm"
space_after  = "3mm"

[design.styles.Date]
size        = "10pt"
space_after = "3mm"

[design.styles.Closing]
space_before   = "6mm"
space_after    = "8mm"
keep_with_next = true

[design.styles.Signature]
space_before  = "0mm"
space_after   = "0mm"
line_height   = "12.1pt"
keep_together = true
```
(`"12.1pt"` is the reference document's `w:line="242" w:lineRule="exact"`. The footer's
`line-height: 1.3` of `letter.css` has no key: furniture lines take the body line height; open
point 7.) `writer.toml`:
```toml
[document]
lang = "en-GB"

[build]
targets = ["html", "pdf", "docx"]

[html]
theme = "theme"

[pdf]
system_fonts = false        # the design ships its fonts: byte-identical PDFs
```
and `letter.md` keeps its directives: `:::recipient {custom-style="Recipient"}` (or just
`:::recipient`), `:::salutation`, `::::closing` with `:::signoff {custom-style="Closing"}` and
`:::signature`. What each backend produces once every task of M8 is done:
- **HTML** (T81, T85, T86): `tokens_css` with four `@font-face` rules and `:root { --w-font-body:
  "Source Sans 3", …; --w-font-heading: …; --w-font-size: 11pt; --w-line-height: 1.35; --w-text:
  #242424; --w-muted: #575757; --w-accent: #a96d4b; --w-link: #a96d4b; }`; `layout_css` with
  `[data-writer-document] p { margin-block: 0 10.205pt; text-align: start; }`, `@page { size: A4;
  margin: 141.732pt 62.362pt 107.717pt 62.362pt; }`, six `[data-style="…"]` rules, the screen
  furniture classes, `@page :first { @top-center { content: "Generative Biology Institute";
  background: url(asset:…) no-repeat top center / 141.732pt auto; … } }`, `@page { @top-center {
  content: "EIT Oxford | Generative Biology Institute"; … } }`, `@page { @bottom-left { content:
  "Ellison Institute …\A Registered Office: …\A …"; border-top: 0.709pt solid #a96d4b; … }
  @bottom-right { content: "+44 1865 649525\A oxford@eit.org"; … } }`. The theme's `document.html`
  shrinks to the sheet and `html.main` plus the two helper calls; `letter.css` keeps the sheet
  dimensions and screen padding only.
- **PDF** (T82, T84, T85, T88): no `[pdf] template`; `writer-doc` sets Source Sans 3 11pt, leading
  0.55em, spacing 10.205pt, fill `#242424`, A4 with the margins, no page numbering, a `background`
  that places the logo and institute line at 17mm on page 1 and the compact line on later pages,
  and the footer block (rule, four left lines, two right lines, 7.5pt, `#575757`) 10mm above the
  bottom edge; `writer-div(style: "Recipient", classes: ("recipient",))[..]` renders a
  non-breakable block with 6mm below, `Closing` a sticky block. The four `.ttf` files are in the
  World, so `system_fonts = false` is byte-identical across machines. `letterhead.typ` becomes
  unnecessary (or stays as a BYO template that delegates).
- **DOCX** (T83, T85, T87): no `reference_doc`; generated `styles.xml` with Source Sans 3 in
  `rPrDefault`, sz 22, color 242424, `pPrDefault` spacing line 324 auto after 204, Body Text
  spacing before 0 after 204 (§7.1); headings in Source Sans 3; six appended paragraph styles (`Recipient`: keepLines, after 340, line 242 exact,
  contextualSpacing; `Closing`: keepNext, before 340, after 454; `Date`: the built-in definition
  with sz 20 and after 170 laid over it; …);
  `sectPr` with `pgSz 11906×16838`, `pgMar top=2835 right=1247 bottom=2154 left=1247 header=964
  footer=567`, `titlePg`, `header1.xml` (first: centred logo drawing 1800000 EMU wide + centred
  semibold copper "Generative Biology Institute"), `header2.xml` (default: the compact line, sz 20),
  `footer1.xml` (both page classes: `pBdr/top` copper rule, four Footer paragraphs with right tab
  at 9412 twips carrying the phone and email). The reference document is no longer needed; keeping
  `reference_doc = "theme/reference.docx"` makes §7.3 apply: its styles and section win, and
  `check` says so.

## 12. Live reload and watch
`theme.toml` and `writer.toml` stay `Content` (live-reload.md §6.2): a design change rebuilds every
target. Files referenced by the design (`Design::referenced_files()`) join the watch set as content
inputs (`InputRole` content; pipeline `accessed` inputs) even when they lie under a theme's
`assets/` (which §6.2 would otherwise classify as `Css`), because a font or logo change must rebuild
PDF and DOCX too. `set_inputs` receives a new map only when the serialized design changes (§6.1),
so an edit elsewhere keeps the Libraries.

## Open points (each with the recommended answer)
1. **Embedding fonts in DOCX.** Recommended: a later task (`w:embedRegular` in `fontTable.xml`
   with ODTTF obfuscation, `[docx] embed_fonts = true`); names only for now (§7.4).
2. **Dark-mode colours.** Recommended: later, as `[design.colors.dark]` emitted inside
   `@media (prefers-color-scheme: dark)` in `tokens_css`; until then the theme's dark block wins
   (§5.2) and a light-only design sets `color-scheme: light` in its CSS.
3. **`colors.background` in DOCX.** Recommended: not applied (`w:background` needs
   `w:displayBackgroundShape` in settings and prints oddly); HTML and PDF apply it.
4. **Last-paragraph margin inside a styled div (HTML).** Recommended: `[data-style] > p:last-child
   { margin-bottom: 0 }` is *not* generated; themes add it where wanted.
5. **Exports and design files.** Recommended: `--format typ` copies design fonts and images into the
   export tree and writes `writer-design.json` beside `main.typ` in a later task; until then §6.7.
6. **Inline markup in furniture lines.** Recommended: plain text now; a later task may parse lines
   as Markdown inlines when a `markdown = true` key is set on the furniture.
7. **Furniture `line_height`.** Recommended: add `line_height` (LineHeight) to the furniture table
   in T86–T88 if the letter's footer needs it; §2.2 omits it so the first cut stays small.
8. **Page numbers only on multi-page documents** (the Letters Typst file). Recommended: no key;
   `{pages}` is available and a BYO template may add the condition.
9. **`writer init`.** Recommended: unchanged; the scaffolded child theme does not declare a design
   (the ejected theme's commented block, §4.2, is the discovery path).
10. **Package version bump** (§6.4, `0.1.0` → `0.2.0`). Recommended: bump, because ejected and
    installed packages must expose `writer-div` for exported sources to compile.

## Verified API hints (for the task files; each says where it was read)
- `typst::text::FontInfo::iter(data: &[u8]) -> impl Iterator<Item = FontInfo>` and
  `Font::new(data: Bytes, index: u32) -> Option<Font>` (`typst-library-0.15.1/src/text/font/mod.rs`
  lines 63, 80); `FontInfo { family: String, variant: FontVariant { style, weight, stretch }, flags,
  coverage }`; `FontWeight::to_number() -> u16`; `FontBook::contains_family(&self, &str) -> bool`
  and `select_family` (`text/font/book.rs` lines 54, 81).
- `typst_kit::fonts::{embedded(), system(), scan(path: &Path)}` (`typst-kit-0.15.1/src/fonts.rs`
  lines 135, 148, 163); the World already calls `scan` per `[pdf] font_paths` entry
  (`src/typst/world.rs` 140–144). `scan` delegates to `fontdb::Database::load_fonts_dir` and accepts
  directories only. Individual files use `Font::iter(Bytes)` and retain each face's collection
  index in a lazy `FontPath` entry.
- `block(sticky: bool)` keeps a block with the next one (`typst-library-0.15.1/src/layout/container.rs`
  line 388); `block(breakable: false)` is keep-together.
- `Theme::vars()` / `deep_merge` (`src/theme/mod.rs` 500–512, 1027–1038) is the merge to reuse;
  `ThemeManifest::line_of_key` / `line_of_value` (`src/theme/manifest.rs`) locate theme diagnostics.
- `Config::with_unknown_key_hint` (`src/config.rs` 957) is the hint wording; `merge_front_matter`
  (734) and `front_table` are where `design:` joins.
- `world_inputs(config)` (`src/typst/world.rs` 548–568) is the single producer of `sys.inputs`;
  `WorldOptions { system_fonts, font_paths, … }` (46–48).
- `StyleTable::custom` (`src/docx/styles.rs` 437–475), `Block::Div` handling (`src/docx/body.rs`
  452–461), `builtin_section` (`src/docx/reference.rs` 412), kept rel types and the k-smallest-free
  part index (`reference.rs` 1–125, 340–440); `DocxOptions` (`src/docx/mod.rs` 67–76) takes
  `config.pdf.paper` for the page (`mod.rs` 305).
- HTML: `compile_theme_css`/`compile`/`Targets` (`src/html/styles.rs`), `register_helpers` adds
  template functions (`src/html/document.rs`), `theme_json` builds `theme` (`src/html/context.rs`),
  the `div` emitter and attribute filter (`src/html/emit.rs` 789–811, 855–880: `custom-style` is
  dropped today).
- `imagesize 0.15.0` (`Cargo.toml` line 61): `imagesize::blob_size(bytes)` and `image_type(bytes)`
  give a raster image's pixel size and kind (`src/docx/images.rs` `pixel_size`, 336–348); the DOCX
  drawing EMU size for figures is derived there and is what §9.3 and §9.5 reuse for furniture images.
- Reference geometry of the Letters `reference.docx` (read 2026-11-01): `pgSz 11906×16838`, `pgMar
  top=2835 right=1247 bottom=2154 left=1247 header=964 footer=737`, `titlePg`; `Recipient`:
  `keepLines`, `after=227`, `line=242 exact`; `Closing`: basedOn BodyText, `keepNext`, `before=340
  after=510`; `Date`: `after=240 sz=20`; first-page header: inline drawing extent `1800000×288517`
  EMU then a centred bold `A96D4B` sz 22 run; default header: centred bold `A96D4B` sz 20; footer:
  `pBdr/bottom single sz 8 color A96D4B` paragraph, then Footer paragraphs with a right tab at 9411.

## Deviations from the brief
- `text.paragraph_spacing` and `text.justify` are tokens the brief did not list; the letter needs
  the first (3.6 mm between paragraphs in every output) and `writer-doc` justifies by default, so a
  letter must be able to switch it off.
- `right_lines` (two-column furniture) is added because the letter's footer (and its
  `reference.docx`) set the phone and email flush right; a single `lines` list could not reproduce
  the template.
- Furniture page numbers are placeholders inside lines rather than a separate `page_number` key:
  one mechanism, and the position follows from the line.
- Generated HTML CSS is split into a tokens block before the theme entries and a layout block
  after them (§5.1); the brief placed everything before the entries, which would let the default
  theme's `print.css` override every declared margin.
