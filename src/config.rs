//! Configuration types, sources, and precedence (config.md §1–§4).
//! Owner: T01. Stub created by T00.
//!
//! Config tables merge key by key; lists replace. Load the project file, merge
//! document front matter, then apply CLI overrides. Theme variables never
//! change build settings. Paths remain as written until pipeline preparation.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::assets::registry::normalize_lexically;
use crate::error::{Error, Result};
use crate::ir::{DocMeta, SourceKind};

/// File name searched for by [`Config::discover`].
pub const CONFIG_FILE_NAME: &str = "writer.toml";

/// The complete configuration; every table and key is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[document]` settings.
    pub document: DocumentConfig,
    /// `[build]` settings.
    pub build: BuildConfig,
    /// `[markdown]` settings.
    pub markdown: MarkdownConfig,
    /// `[numbering]` settings.
    pub numbering: NumberingConfig,
    /// `[references]` settings.
    pub references: ReferencesConfig,
    /// `[math]` settings.
    pub math: MathConfig,
    /// `[html]` settings.
    pub html: HtmlConfig,
    /// `[pdf]` settings.
    pub pdf: PdfConfig,
    /// `[docx]` settings.
    pub docx: DocxConfig,
    /// `[latex]` settings.
    pub latex: LatexConfig,
    /// `[typst]` settings.
    pub typst: TypstConfig,
    /// `[notebook]` settings.
    pub notebook: NotebookConfig,
    /// `[serve]` settings.
    pub serve: ServeConfig,
}

/// `[document]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DocumentConfig {
    /// BCP-47 document language when metadata does not set one.
    pub lang: String,
    /// Table of contents depth, or off.
    pub toc: Toc,
}

impl Default for DocumentConfig {
    fn default() -> Self {
        Self { lang: "en".into(), toc: Toc::Off }
    }
}

/// `[build]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildConfig {
    /// Output directory; kept relative until pipeline preparation.
    pub out_dir: PathBuf,
    /// Output targets for Markdown and notebook inputs, in order.
    pub targets: Vec<Target>,
    /// Output targets for Typst inputs, in order.
    pub typst_targets: Vec<Target>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            out_dir: "dist".into(),
            targets: vec![Target::Html],
            typst_targets: vec![Target::Pdf],
        }
    }
}

/// `[markdown]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MarkdownConfig {
    /// Enable comrak smart punctuation.
    pub smart_punctuation: bool,
    /// Emit source position attributes in build HTML.
    pub sourcepos: bool,
    /// Parse document references and citations.
    pub references: bool,
    /// Enable the superscript extension.
    pub superscript: bool,
    /// Enable the subscript extension.
    pub subscript: bool,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            smart_punctuation: false,
            sourcepos: false,
            references: true,
            superscript: false,
            subscript: false,
        }
    }
}

/// `[numbering]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NumberingConfig {
    /// Heading numbering pattern, or off.
    pub headings: HeadingNumbering,
    /// Scope of figure, table, and listing counters.
    pub figures: FigureNumbering,
    /// Which equations receive numbers.
    pub equations: EquationNumbering,
}

impl Default for NumberingConfig {
    fn default() -> Self {
        Self {
            headings: HeadingNumbering::Off,
            figures: FigureNumbering::Document,
            equations: EquationNumbering::Labelled,
        }
    }
}

/// `[references]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReferencesConfig {
    /// Bibliography paths; relative paths remain as written.
    pub bibliography: Vec<PathBuf>,
    /// Hayagriva style name or CSL file path.
    pub style: String,
    /// CSL locale; empty means the document language.
    pub locale: String,
}

impl Default for ReferencesConfig {
    fn default() -> Self {
        Self {
            bibliography: Vec::new(),
            style: "ieee".into(),
            locale: String::new(),
        }
    }
}

/// `[math]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MathConfig {
    /// HTML equation rendering route.
    pub html: HtmlMath,
    /// Font policy whenever MathML is emitted.
    pub mathml_font: MathmlFont,
    /// Options passed to KaTeX; TOML dates become strings.
    #[serde(deserialize_with = "json_map")]
    pub katex_options: Map<String, Value>,
}

impl Default for MathConfig {
    fn default() -> Self {
        Self {
            html: HtmlMath::Katex,
            mathml_font: MathmlFont::Embed,
            katex_options: Map::new(),
        }
    }
}

/// `[html]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HtmlConfig {
    /// Built-in theme name or theme directory.
    pub theme: String,
    /// Emit one self-contained HTML file rather than a site.
    pub standalone: bool,
}

impl Default for HtmlConfig {
    fn default() -> Self {
        Self { theme: "default".into(), standalone: true }
    }
}

/// `[pdf]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PdfConfig {
    /// Typst template exporting writer-doc; empty means built-in.
    pub template: PathBuf,
    /// Typst paper name.
    pub paper: String,
    /// PDF rendering engine.
    pub engine: PdfEngine,
    /// Permit downloading preview packages.
    pub allow_packages: bool,
    /// Search system fonts after embedded and configured fonts.
    pub system_fonts: bool,
    /// Additional font directories, before system fonts.
    pub font_paths: Vec<PathBuf>,
    /// Pinned RFC 3339 timestamp; empty uses pipeline fallbacks.
    pub timestamp: String,
}

impl Default for PdfConfig {
    fn default() -> Self {
        Self {
            template: PathBuf::new(),
            paper: "a4".into(),
            engine: PdfEngine::Typst,
            allow_packages: false,
            system_fonts: true,
            font_paths: Vec::new(),
            timestamp: String::new(),
        }
    }
}

/// `[docx]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DocxConfig {
    /// Reference DOCX file; empty means built-in.
    pub reference_doc: PathBuf,
    /// Editable equations or rendered images.
    pub math: DocxMath,
    /// Ask Word to refresh fields when the document opens.
    pub update_fields: bool,
}

impl Default for DocxConfig {
    fn default() -> Self {
        Self {
            reference_doc: PathBuf::new(),
            math: DocxMath::Omml,
            update_fields: true,
        }
    }
}

/// `[latex]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LatexConfig {
    /// Minijinja TeX template; empty means built-in.
    pub template: PathBuf,
    /// LaTeX command used for level-one headings.
    pub top_level: LatexTopLevel,
    /// Citation command family.
    pub cite: LatexCite,
    /// Append a bibliography command to the document body.
    pub emit_bibliography: bool,
    /// Code block rendering policy.
    pub code: LatexCode,
    /// Table environment policy.
    pub tables: LatexTables,
    /// Use booktabs table rules rather than hline.
    pub booktabs: bool,
    /// Use cleveref reference commands.
    pub cleveref: bool,
    /// Figure and table placement options; empty omits them.
    pub figure_placement: String,
    /// Target TeX engine; writer never runs it.
    pub engine: LatexEngine,
    /// Unicode mapping policy.
    pub unicode: LatexUnicode,
    /// Wrap notebook prompts in writercell environments.
    pub cell_prompts: bool,
    /// Prose wrapping column; zero disables wrapping.
    pub wrap: u32,
}

impl Default for LatexConfig {
    fn default() -> Self {
        Self {
            template: PathBuf::new(),
            top_level: LatexTopLevel::Section,
            cite: LatexCite::Biblatex,
            emit_bibliography: false,
            code: LatexCode::Verbatim,
            tables: LatexTables::Longtable,
            booktabs: true,
            cleveref: false,
            figure_placement: "htbp".into(),
            engine: LatexEngine::Pdflatex,
            unicode: LatexUnicode::Auto,
            cell_prompts: false,
            wrap: 0,
        }
    }
}

/// `[typst]` configuration (config.md §2).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypstConfig {
    /// Escalate W-TYP-* and E-TYP-MATH-CODE in the pipeline.
    pub strict: bool,
    /// Additional local Typst package roots.
    pub package_paths: Vec<PathBuf>,
}

/// `[notebook]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotebookConfig {
    /// Output MIME types in descending order of preference.
    pub mime_priority: Vec<String>,
    /// Cell tags that hide input.
    pub hide_input_tags: Vec<String>,
    /// Cell tags that hide output.
    pub hide_output_tags: Vec<String>,
    /// Cell tags that remove the entire cell.
    pub remove_cell_tags: Vec<String>,
    /// Read Quarto-style cell option lines.
    pub cell_options: bool,
    /// Show input and output execution prompts.
    pub show_prompts: bool,
}

impl Default for NotebookConfig {
    fn default() -> Self {
        Self {
            mime_priority: [
                "text/html", "image/svg+xml", "image/png", "image/jpeg",
                "image/gif", "image/webp", "text/markdown", "text/latex",
                "application/json", "text/plain",
            ]
            .map(String::from)
            .to_vec(),
            hide_input_tags: ["hide-input", "remove-input"]
                .map(String::from)
                .to_vec(),
            hide_output_tags: ["hide-output", "remove-output"]
                .map(String::from)
                .to_vec(),
            remove_cell_tags: ["remove-cell", "hide-cell", "skip"]
                .map(String::from)
                .to_vec(),
            cell_options: true,
            show_prompts: true,
        }
    }
}

/// `[serve]` configuration (config.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServeConfig {
    /// Interface on which to listen.
    pub host: String,
    /// Listening port; zero chooses a free port.
    pub port: u16,
    /// Open the browser when serving starts.
    pub open: bool,
    /// Preview view; empty selects the default for the input kind.
    #[serde(with = "serve_view")]
    pub view: Option<ServeView>,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 3000,
            open: false,
            view: None,
        }
    }
}

/// An output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// HTML document.
    Html,
    /// PDF document.
    Pdf,
    /// Word document.
    Docx,
    /// LaTeX source.
    Latex,
    /// Typst source.
    Typst,
}

/// Scope of figure, table, and listing numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FigureNumbering {
    /// Continuous numbering throughout the document.
    Document,
    /// Reset at level-one headings and prefix their number.
    Section,
}

/// Which equations receive numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EquationNumbering {
    /// Only equations carrying labels.
    Labelled,
    /// Every displayed equation.
    All,
    /// No equations.
    None,
}

/// HTML math rendering policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HtmlMath {
    /// Client-side KaTeX using vendored assets.
    Katex,
    /// MathML rendered by math-core.
    Mathml,
    /// Compiler-rendered SVG.
    Image,
}

/// Font policy for MathML output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MathmlFont {
    /// Inline Latin Modern Math WOFF2.
    Embed,
    /// Use a font-family rule without embedding a font.
    System,
}

/// PDF rendering engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PdfEngine {
    /// The embedded Typst compiler.
    Typst,
    /// Chrome browser printing.
    Chrome,
}

/// Development preview mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServeView {
    /// Web page preview.
    Web,
    /// Paginated preview.
    Pages,
}

/// DOCX math rendering policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocxMath {
    /// Editable Word equations, with image fallbacks.
    Omml,
    /// Rendered SVG and PNG for every equation.
    Image,
}

/// LaTeX heading corresponding to a level-one heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LatexTopLevel {
    /// Part.
    Part,
    /// Chapter.
    Chapter,
    /// Section.
    Section,
}

/// LaTeX citation command family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LatexCite {
    /// Biblatex commands.
    Biblatex,
    /// Natbib commands.
    Natbib,
    /// Plain text citations.
    Plain,
}

/// LaTeX code block rendering policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LatexCode {
    /// Fancyvrb Verbatim environment.
    Verbatim,
    /// Syntect tokens rendered with Wr macros.
    Highlighted,
    /// Listings environment.
    Listings,
}

/// LaTeX table environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LatexTables {
    /// Longtable environment.
    Longtable,
    /// Tabular in a floating table environment.
    Tabular,
}

/// Target TeX engine; writer never executes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LatexEngine {
    /// pdfLaTeX.
    Pdflatex,
    /// XeLaTeX.
    Xelatex,
    /// LuaLaTeX.
    Lualatex,
}

/// Unicode mapping policy for LaTeX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LatexUnicode {
    /// Map for pdfLaTeX and keep otherwise.
    Auto,
    /// Map supported Unicode symbols to TeX macros.
    Map,
    /// Keep Unicode characters.
    Keep,
}

/// Table of contents depth; `true` is accepted as depth three.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Toc {
    /// Do not generate a table of contents.
    #[default]
    Off,
    /// Include headings through this level, from one through six.
    Depth(u8),
}

impl Serialize for Toc {
    fn serialize<S: Serializer>(
        &self, serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Off => serializer.serialize_bool(false),
            Self::Depth(depth @ 1..=6) => serializer.serialize_u8(*depth),
            Self::Depth(_) => Err(serde::ser::Error::custom(
                "expected false, true, or an integer in 1..=6",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for Toc {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::Bool(false) => Ok(Self::Off),
            Value::Bool(true) => Ok(Self::Depth(3)),
            Value::Number(number) => match number.as_u64() {
                Some(depth @ 1..=6) => Ok(Self::Depth(depth as u8)),
                _ => Err(serde::de::Error::custom(
                    "expected false, true, or an integer in 1..=6",
                )),
            },
            _ => Err(serde::de::Error::custom(
                "expected false, true, or an integer in 1..=6",
            )),
        }
    }
}

/// Heading numbering; `true` is accepted as the pattern `"1.1"`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum HeadingNumbering {
    /// Do not number headings.
    #[default]
    Off,
    /// A Typst numbering pattern.
    Pattern(String),
}

impl Serialize for HeadingNumbering {
    fn serialize<S: Serializer>(
        &self, serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Off => serializer.serialize_bool(false),
            Self::Pattern(pattern) => serializer.serialize_str(pattern),
        }
    }
}

impl<'de> Deserialize<'de> for HeadingNumbering {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::Bool(false) => Ok(Self::Off),
            Value::Bool(true) => Ok(Self::Pattern("1.1".into())),
            Value::String(pattern) => Ok(Self::Pattern(pattern)),
            _ => Err(serde::de::Error::custom(
                "expected false, true, or a numbering pattern string",
            )),
        }
    }
}

mod serve_view {
    use super::*;

    pub fn serialize<S: Serializer>(
        view: &Option<ServeView>, serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match view {
            Some(view) => view.serialize(serializer),
            None => serializer.serialize_str(""),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Option<ServeView>, D::Error> {
        match Option::<String>::deserialize(deserializer)? {
            None => Ok(None),
            Some(view) if view.is_empty() => Ok(None),
            Some(view) => {
                let view =
                    serde::de::value::StringDeserializer::<D::Error>::new(view);
                ServeView::deserialize(view).map(Some)
            }
        }
    }
}

/// Command-line values; `None` leaves the existing setting unchanged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CliOverrides {
    /// `--to`: replace both input-kind target lists.
    pub targets: Option<Vec<Target>>,
    /// `--out`: handled by the pipeline, not a config table setting.
    pub out: Option<PathBuf>,
    /// `--theme`: HTML theme name or directory.
    pub theme: Option<String>,
    /// `--template`: PDF Typst template.
    pub template: Option<PathBuf>,
    /// `--reference-doc`: reference DOCX file.
    pub reference_doc: Option<PathBuf>,
    /// `--latex-template`: LaTeX template file.
    pub latex_template: Option<PathBuf>,
    /// `--standalone` or `--site`: HTML output mode.
    pub standalone: Option<bool>,
    /// Development server port.
    pub port: Option<u16>,
    /// Development server interface.
    pub host: Option<String>,
    /// Open the development server in a browser.
    pub open: Option<bool>,
    /// Explicit development preview view.
    pub view: Option<ServeView>,
    /// Sets typst.strict; the pipeline performs diagnostic escalation.
    pub strict: Option<bool>,
    /// PDF rendering engine.
    pub engine: Option<PdfEngine>,
    /// Permit preview package downloads.
    pub allow_packages: Option<bool>,
}

impl Config {
    /// Parses TOML, applying defaults and reporting unknown keys with hints.
    pub fn from_toml_str(text: &str) -> Result<Config> {
        toml::from_str(text).map_err(|error| {
            Error::Config(with_unknown_key_hint(error.to_string()))
        })
    }

    /// Reads and parses a project file; errors name the supplied path.
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path).map_err(|error| {
            Error::Config(format!("cannot read {}: {error}", path.display()))
        })?;
        Self::from_toml_str(&text).map_err(|error| match error {
            Error::Config(message) => {
                Error::Config(format!("{}: {message}", path.display()))
            }
            other => other,
        })
    }

    /// Walks from a source directory to the root, loading the nearest writer.toml.
    /// Returns an absolute, lexically normalized config path. Config values stay
    /// relative as written; pipeline preparation resolves their origins.
    pub fn discover(start_dir: &Path) -> Result<Option<(PathBuf, Config)>> {
        let start = if start_dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            start_dir
        };
        let start = normalize_lexically(&std::path::absolute(start)?);
        for dir in start.ancestors() {
            let candidate = dir.join(CONFIG_FILE_NAME);
            if candidate.is_file() {
                return Ok(Some((candidate.clone(), Self::load(&candidate)?)));
            }
        }
        Ok(None)
    }

    /// Atomically overlays front matter, leaving metadata unchanged.
    /// Table maps merge by key and lists replace; dedicated keys win over maps.
    /// Repeating a successful merge has no effect. Any invalid key or value
    /// returns FrontMatter with a hint where possible and preserves self.
    pub fn merge_front_matter(&mut self, meta: &DocMeta) -> Result<()> {
        let mut next = self.clone();
        let mut document = Map::new();
        let markdown = front_table(meta, "markdown")?;
        let math = front_table(meta, "math")?;
        let numbering = front_table(meta, "numbering")?;
        let mut references = front_table(meta, "references")?;
        let mut html = front_table(meta, "html")?;
        let mut pdf = front_table(meta, "pdf")?;
        let mut docx = front_table(meta, "docx")?;
        let mut latex = front_table(meta, "latex")?;
        let typst = front_table(meta, "typst")?;
        let notebook = front_table(meta, "notebook")?;

        dedicated(meta, "toc", &mut document, "toc");
        if let Some(value) = meta.vars.get("bibliography") {
            let files = match value {
                Value::String(path) => {
                    Value::Array(vec![Value::String(path.clone())])
                }
                Value::Array(_) => value.clone(),
                _ => {
                    return Err(Error::FrontMatter(
                        "bibliography: expected a path or list of paths".into(),
                    ));
                }
            };
            references.insert("bibliography".into(), files);
        }
        if meta.vars.contains_key("cite_style") && meta.vars.contains_key("csl")
        {
            return Err(Error::FrontMatter(
                "use either cite_style or csl".into(),
            ));
        }
        dedicated(meta, "cite_style", &mut references, "style");
        dedicated(meta, "csl", &mut references, "style");
        dedicated(meta, "cite_locale", &mut references, "locale");
        dedicated(meta, "theme", &mut html, "theme");
        dedicated(meta, "template", &mut pdf, "template");
        dedicated(meta, "paper", &mut pdf, "paper");
        dedicated(meta, "reference_doc", &mut docx, "reference_doc");
        dedicated(meta, "latex_template", &mut latex, "template");

        overlay_table(&mut next.document, "document", &document)?;
        overlay_table(&mut next.markdown, "markdown", &markdown)?;
        overlay_table(&mut next.math, "math", &math)?;
        overlay_table(&mut next.numbering, "numbering", &numbering)?;
        overlay_table(&mut next.references, "references", &references)?;
        overlay_table(&mut next.html, "html", &html)?;
        overlay_table(&mut next.pdf, "pdf", &pdf)?;
        overlay_table(&mut next.docx, "docx", &docx)?;
        overlay_table(&mut next.latex, "latex", &latex)?;
        overlay_table(&mut next.typst, "typst", &typst)?;
        overlay_table(&mut next.notebook, "notebook", &notebook)?;
        *self = next;
        Ok(())
    }

    /// Applies the highest-precedence CLI settings; out is pipeline-only.
    pub fn apply_cli(&mut self, overrides: &CliOverrides) {
        if let Some(targets) = &overrides.targets {
            self.build.targets = targets.clone();
            self.build.typst_targets = targets.clone();
        }
        if let Some(theme) = &overrides.theme {
            self.html.theme = theme.clone();
        }
        if let Some(template) = &overrides.template {
            self.pdf.template = template.clone();
        }
        if let Some(reference_doc) = &overrides.reference_doc {
            self.docx.reference_doc = reference_doc.clone();
        }
        if let Some(template) = &overrides.latex_template {
            self.latex.template = template.clone();
        }
        if let Some(standalone) = overrides.standalone {
            self.html.standalone = standalone;
        }
        if let Some(port) = overrides.port {
            self.serve.port = port;
        }
        if let Some(host) = &overrides.host {
            self.serve.host = host.clone();
        }
        if let Some(open) = overrides.open {
            self.serve.open = open;
        }
        if let Some(view) = overrides.view {
            self.serve.view = Some(view);
        }
        if let Some(strict) = overrides.strict {
            self.typst.strict = strict;
        }
        if let Some(engine) = overrides.engine {
            self.pdf.engine = engine;
        }
        if let Some(allow_packages) = overrides.allow_packages {
            self.pdf.allow_packages = allow_packages;
        }
    }

    /// Returns the ordered targets for the source kind.
    pub fn targets_for(&self, kind: SourceKind) -> Vec<Target> {
        match kind {
            SourceKind::Typst => self.build.typst_targets.clone(),
            SourceKind::Markdown | SourceKind::Notebook => {
                self.build.targets.clone()
            }
        }
    }

    /// The byte-stable, commented default configuration from config.md §2.
    pub fn default_toml() -> String {
        DEFAULT_TOML.to_owned()
    }
}

fn front_table(meta: &DocMeta, name: &str) -> Result<Map<String, Value>> {
    match meta.vars.get(name) {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(map)) => Ok(map.clone()),
        Some(Value::Bool(flag)) if name == "numbering" => {
            Ok([("headings".into(), Value::Bool(*flag))].into_iter().collect())
        }
        _ => Err(Error::FrontMatter(format!("{name}: expected a mapping"))),
    }
}

fn dedicated(
    meta: &DocMeta, name: &str, table: &mut Map<String, Value>, key: &str,
) {
    if let Some(value) = meta.vars.get(name) {
        table.insert(key.into(), value.clone());
    }
}

/// Overlays one table map key by key, retaining every omitted setting.
fn overlay_table<T: Serialize + DeserializeOwned>(
    table: &mut T, name: &str, overrides: &Map<String, Value>,
) -> Result<()> {
    if overrides.is_empty() {
        return Ok(());
    }
    let mut merged = match serde_json::to_value(&*table)? {
        Value::Object(map) => map,
        other => {
            return Err(Error::Other(format!(
                "config table {name} serialized to a non-object: {other}"
            )));
        }
    };
    for (key, value) in overrides {
        merged.insert(key.clone(), value.clone());
    }
    *table =
        serde_json::from_value(Value::Object(merged)).map_err(|error| {
            Error::FrontMatter(format!(
                "{name}: {}",
                with_unknown_key_hint(error.to_string())
            ))
        })?;
    Ok(())
}

/// Deserializes a JSON-like map, turning TOML dates/times into their text.
fn json_map<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Map<String, Value>, D::Error> {
    let mut map = Map::<String, Value>::deserialize(deserializer)?;
    for value in map.values_mut() {
        stringify_toml_datetimes(value);
    }
    Ok(map)
}

/// Replaces every TOML date/time inside `value` by its text (see [`toml_datetime_text`]).
fn stringify_toml_datetimes(value: &mut Value) {
    match value {
        Value::Object(map) => match toml_datetime_text(map) {
            Some(text) => *value = Value::String(text),
            None => map.values_mut().for_each(stringify_toml_datetimes),
        },
        Value::Array(items) => {
            items.iter_mut().for_each(stringify_toml_datetimes)
        }
        _ => {}
    }
}

/// The `toml` deserializer hands a date/time to a `serde_json::Value` as a one-entry object
/// (a private marker key mapped to the date text). Returns that date's text (`2026-10-08`)
/// when `map` is such an object, recognised by `toml`'s own `Datetime` deserializer.
fn toml_datetime_text(map: &Map<String, Value>) -> Option<String> {
    if map.len() != 1 || !map.values().all(Value::is_string) {
        return None;
    }
    serde_json::from_value::<toml::value::Datetime>(Value::Object(map.clone()))
        .ok()
        .map(|datetime| datetime.to_string())
}

/// For a serde "unknown field `x`, expected …" or "unknown variant `x`, expected …" message
/// (as produced for `deny_unknown_fields` structs and enums), the expected name closest to
/// `x` if it is within Levenshtein distance 2.
pub fn unknown_key_hint(message: &str) -> Option<String> {
    let (_, rest) = message
        .split_once("unknown field `")
        .or_else(|| message.split_once("unknown variant `"))?;
    let (unknown, rest) = rest.split_once('`')?;
    let (_, expected) = rest.split_once("expected ")?;
    let expected = expected.lines().next().unwrap_or_default();
    // Names are the backtick-quoted parts: "`a`, `b` or `c`" → a, b, c.
    let candidates = expected.split('`').skip(1).step_by(2);
    closest(unknown, candidates).map(str::to_string)
}

/// `message` (trailing whitespace trimmed) with ` (did you mean '…'?)` appended when
/// [`unknown_key_hint`] finds a suggestion.
pub fn with_unknown_key_hint(message: String) -> String {
    let trimmed = message.trim_end();
    match unknown_key_hint(trimmed) {
        Some(suggestion) => format!("{trimmed} (did you mean '{suggestion}'?)"),
        None => trimmed.to_string(),
    }
}

/// The candidate with the smallest edit distance to `word` (first one on ties), if that
/// distance is 1 or 2.
fn closest<'c>(
    word: &str, candidates: impl IntoIterator<Item = &'c str>,
) -> Option<&'c str> {
    candidates
        .into_iter()
        .map(|candidate| (levenshtein(word, candidate), candidate))
        .filter(|(distance, _)| (1..=2).contains(distance))
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// Levenshtein edit distance over `char`s (insertions, deletions, substitutions).
fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = Vec::with_capacity(b.len() + 1);
        current.push(i + 1);
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != *cb);
            let deletion = previous[j + 1] + 1;
            let insertion = current[j] + 1;
            current.push(substitution.min(deletion).min(insertion));
        }
        previous = current;
    }
    previous[b.len()]
}

/// Commented defaults, kept verbatim with config.md §2.
const DEFAULT_TOML: &str = r###"[document]
lang = "en"                # BCP-47; front matter `lang` overrides
toc = false                # false | true (depth 3) | 1..6 (max heading level listed)
                           # the TOC goes after the title block unless the document places :::toc

[build]
out_dir = "dist"           # set here: relative to this file; default: relative to the input's directory
targets = ["html"]         # for .md/.ipynb inputs; subset of html, pdf, docx, latex, typst (order = output order)
typst_targets = ["pdf"]    # for .typ inputs

[markdown]
smart_punctuation = false  # comrak parse.smart
sourcepos = false          # emit position attributes in `build` HTML too (serve always does)
references = true          # parse @target references/citations; false for presenter decks with literal @handles
superscript = false        # comrak extension.superscript (x^2^)
subscript = false          # comrak extension.subscript (H~2~O); off because ~/paths in prose would subscript

[numbering]
headings = false           # false | true (≡ "1.1") | a typst numbering pattern string ("1.", "1.1", "A.1", "1.1.a")
figures = "document"       # "document" (1, 2, …) | "section" (1.1, 1.2, …; counters reset at level-1 headings)
equations = "labelled"     # "labelled" (only equations with a label) | "all" | "none"

[references]
bibliography = []          # .bib / .yaml files, relative to this file (front matter: relative to the document)
style = "ieee"             # a hayagriva style name (typst's names) or a path to a .csl file
locale = ""                # CSL locale; "" → document.lang

[math]
html = "katex"             # "katex" (client-side KaTeX, vendored) | "mathml" (math-core, no JS) | "image" (compiler-rendered SVG)
mathml_font = "embed"      # for MathML output (html = "mathml", and typst math under any html but "image"):
                           # "embed" (Latin Modern Math woff2 inlined, +≈518 KB as base64) | "system" (font-family rule only)
katex_options = {}         # passed to katex.render (e.g. { macros = { "\\R" = "\\mathbb{R}" } })

[html]
theme = "default"          # built-in name or a directory (relative to this file)
standalone = true          # false → <stem>/index.html + assets/ (site mode)

[pdf]
template = ""              # a .typ file exporting `writer-doc` (typst-dialect.md §4); "" → built-in
paper = "a4"               # any typst paper name ("us-letter", …)
engine = "typst"           # "typst" | "chrome" (stretch T77; needs Chrome)
allow_packages = false     # let the compiler download @preview packages (network)
system_fonts = true        # scan the system's fonts after the embedded ones (false for byte-identical PDFs)
font_paths = []            # extra font directories, searched after the embedded fonts and before the system's
timestamp = ""             # "" → front matter `date`, else SOURCE_DATE_EPOCH, else now; an RFC 3339 value pins it

[docx]
reference_doc = ""         # a .docx whose styles/page setup/headers/theme are used; "" → built-in
math = "omml"              # "omml" (editable Word equations, image fallback per equation) | "image" (always SVG+PNG)
update_fields = true       # ask Word to update fields (TOC page numbers, SEQ) on open; false keeps the cached values

[latex]
template = ""              # minijinja .tex template with \VAR{preamble} and \VAR{body}; "" → the built-in sample
top_level = "section"      # "part" | "chapter" | "section": what a level-1 heading becomes
cite = "biblatex"          # "biblatex" | "natbib" | "plain"
emit_bibliography = false  # true → writer appends \printbibliography / \bibliography{...} at the end of the body
code = "verbatim"          # "verbatim" (fancyvrb Verbatim) | "highlighted" (syntect tokens as \Wr* macros) | "listings"
tables = "longtable"       # "longtable" | "tabular" (floating table environment)
booktabs = true            # \toprule/\midrule/\bottomrule; false → \hline
cleveref = false           # \cref instead of "Figure~\ref"
figure_placement = "htbp"  # the [..] option of figure/table environments; "H" needs the float package; "" for none
engine = "pdflatex"        # "pdflatex" | "xelatex" | "lualatex": selects the Unicode policy and the lints; writer never runs TeX
unicode = "auto"           # "auto" (map for pdflatex, keep otherwise) | "map" (maths symbols → macros) | "keep"
cell_prompts = false       # notebook cells wrapped in writercell{In|Out}{n}
wrap = 0                   # prose line-wrap column; 0 = none

[typst]
strict = false             # W-TYP-* and E-TYP-MATH-CODE become errors for HTML/DOCX/LaTeX;
                           # --strict additionally escalates unsupported-construct and math-conversion-failed
package_paths = []         # extra local package roots ({root}/{namespace}/{name}/{version})

[notebook]
mime_priority = ["text/html", "image/svg+xml", "image/png", "image/jpeg", "image/gif", "image/webp",
                 "text/markdown", "text/latex", "application/json", "text/plain"]
hide_input_tags  = ["hide-input", "remove-input"]
hide_output_tags = ["hide-output", "remove-output"]
remove_cell_tags = ["remove-cell", "hide-cell", "skip"]
cell_options = true        # read quarto-style `#| key: value` lines
show_prompts = true        # In [n] / Out [n] in HTML and DOCX

[serve]
host = "127.0.0.1"
port = 3000
open = false
view = ""                  # "" → "pages" for .typ inputs, "web" otherwise | "web" | "pages"
"###;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_distances() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("build", "build"), 0);
        assert_eq!(levenshtein("buidl", "build"), 2);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("splt", "split"), 1);
        assert_eq!(levenshtein("sérve", "serve"), 1);
    }

    #[test]
    fn hint_parses_serde_messages() {
        let many = "unknown field `buidl`, expected one of `document`, `build`, `markdown`";
        assert_eq!(unknown_key_hint(many).as_deref(), Some("build"));
        let two = "unknown variant `page`, expected `web` or `pages`";
        assert_eq!(unknown_key_hint(two).as_deref(), Some("pages"));
        let one = "unknown field `opem`, expected `open`";
        assert_eq!(unknown_key_hint(one).as_deref(), Some("open"));
        let far = "unknown field `zzzzzz`, expected one of `document`, `build`";
        assert_eq!(unknown_key_hint(far), None);
        assert_eq!(unknown_key_hint("invalid type: string"), None);
        assert_eq!(
            unknown_key_hint("unknown field `x`, there are no fields"),
            None
        );
    }

    #[test]
    fn hint_ties_pick_the_first_candidate() {
        let message = "unknown field `ab`, expected one of `aa`, `bb`, `ac`";
        assert_eq!(unknown_key_hint(message).as_deref(), Some("aa"));
    }

    #[test]
    fn with_hint_trims_and_appends() {
        assert_eq!(
            with_unknown_key_hint(
                "unknown field `buidl`, expected `build`\n".into()
            ),
            "unknown field `buidl`, expected `build` (did you mean 'build'?)"
        );
        assert_eq!(with_unknown_key_hint("other\n".into()), "other");
    }
}
