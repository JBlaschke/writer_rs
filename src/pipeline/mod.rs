//! Frontend, resolver, output and write orchestration.
//!
//! A build prepares one document, shares one compiler world across its targets,
//! and refuses to overwrite any source or resource it consumes.

mod export;

pub mod docx;
pub mod html;
pub mod latex;
pub mod pdf;
pub mod typst;
pub mod typst_source;

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::assets::emit::OutputMode;
use crate::assets::registry::{AssetData, AssetRegistry, normalize_lexically};
use crate::config::{CliOverrides, Config, PdfEngine, Target};
use crate::error::{Diagnostic, Diagnostics, Error, Level, Result};
use crate::html::BuildInfo;
use crate::ir::{Document, SourceInfo, SourceKind};
use crate::resolve::{self, ResolveOptions};
use crate::theme::{self, Theme, ThemeDir, ThemeSource};
use crate::typst::compile::pdf_timestamp;
use crate::typst::includes::MarkdownIncludes;
use crate::typst::world::{MainFile, WriterWorld, world_inputs};

/// How resolution and rendering should treat a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A publication build; unresolved references are errors.
    Build,
    /// A preview rebuild with a monotonically increasing id.
    Serve {
        /// Preview rebuild number.
        build_id: u64,
    },
    /// A lint request; unresolved references remain warnings.
    Check,
}

/// Source selection and command-line overrides for one request.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// Source path, as supplied by the caller.
    pub input: PathBuf,
    /// Input-format override, otherwise detected from the extension.
    pub format: Option<SourceKind>,
    /// Explicit project config, otherwise discovered above the source.
    pub config_path: Option<PathBuf>,
    /// Highest-precedence settings.
    pub cli: CliOverrides,
    /// Build, preview or lint behavior.
    pub mode: Mode,
}

impl BuildOptions {
    /// A build request with format discovery and no overrides.
    pub fn new(input: impl Into<PathBuf>) -> Self {
        Self {
            input: input.into(),
            format: None,
            config_path: None,
            cli: CliOverrides::default(),
            mode: Mode::Build,
        }
    }
}

/// A document with its effective configuration and registered assets.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// Parsed, and normally resolved, document IR.
    pub document: Document,
    /// Registered document resources.
    pub assets: AssetRegistry,
    /// Config after project, front matter and command-line precedence.
    pub config: Config,
    /// Absolute project config path, when one was used.
    pub config_path: Option<PathBuf>,
    /// Findings from parsing and resolution.
    pub diagnostics: Diagnostics,
}

/// A stopped preparation and every diagnostic collected before it failed.
#[derive(Debug)]
pub struct BuildFailure {
    /// The operation that failed, boxed to keep results small on every target.
    pub error: Box<Error>,
    /// Findings collected before the failure.
    pub diagnostics: Diagnostics,
}

impl From<Error> for BuildFailure {
    fn from(error: Error) -> Self {
        Self { error: Box::new(error), diagnostics: Diagnostics::default() }
    }
}

/// How a path contributes to a build or its watch set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRole {
    /// Main document.
    Source,
    /// Included source.
    Include,
    /// Project configuration.
    Config,
    /// Registered local resource.
    Asset,
    /// Referenced resource that does not exist yet.
    MissingAsset,
    /// A theme directory, or one of its files in a clash check.
    Theme,
    /// Bibliography or CSL style.
    Bibliography,
    /// Typst PDF template.
    PdfTemplate,
    /// Word reference document.
    ReferenceDoc,
    /// LaTeX template.
    LatexTemplate,
}

impl fmt::Display for InputRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Source => "the input file",
            Self::Include => "the included file",
            Self::Config => "the config file",
            Self::Asset => "the document's asset",
            Self::MissingAsset => "the missing asset",
            Self::Theme => "the theme file",
            Self::Bibliography => "the bibliography or style file",
            Self::PdfTemplate => "the PDF template",
            Self::ReferenceDoc => "the reference document",
            Self::LatexTemplate => "the LaTeX template",
        })
    }
}

/// A rendered page and the resources a development server must retain.
pub struct WebPage {
    /// Complete HTML document.
    pub html: String,
    /// Stylesheet served separately in preview mode.
    pub theme_css: String,
    /// Resolved theme chain.
    pub theme: Theme,
    /// Render-only findings; prepare findings remain on `Prepared`.
    pub diagnostics: Diagnostics,
    /// Document assets plus generated equation images.
    pub assets: AssetRegistry,
}

/// Result of one target, independent of other target failures.
#[derive(Debug)]
pub struct TargetResult {
    /// Requested output format.
    pub target: Target,
    /// Written paths, with the main output first, or the target error.
    pub written: Result<Vec<PathBuf>>,
}

/// Preparation and all target results of a build.
#[derive(Debug)]
pub struct BuildReport {
    /// Effective document, or preparation failure.
    pub prepared: std::result::Result<Prepared, BuildFailure>,
    /// Target results in deduplicated request order.
    pub targets: Vec<TargetResult>,
    /// Prepare and render findings, after strict escalation.
    pub diagnostics: Diagnostics,
}

/// What a batch of edited files needs to rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    /// Stylesheets or theme assets only.
    Css,
    /// Typeset pages only.
    Pages,
    /// Source, metadata or other document content.
    Content,
}

/// Parses input and applies config precedence, without resolving references.
pub fn parse_only(
    opts: &BuildOptions,
) -> std::result::Result<Prepared, BuildFailure> {
    let kind =
        opts.format.map_or_else(|| crate::ir::detect_kind(&opts.input), Ok)?;
    let text = read_source(&opts.input)?;
    let source = source_info(&opts.input, kind)?;
    let (mut config, config_path) = load_config(opts)?;
    let parsed = match kind {
        SourceKind::Markdown => crate::markdown::parse(&source, &text, &config),
        SourceKind::Notebook => crate::notebook::parse(&source, &text, &config),
        SourceKind::Typst => {
            crate::typst::frontend::parse(&source, &text, &config)
        }
    }?;
    if let Err(error) = config.merge_front_matter(&parsed.document.meta) {
        return Err(BuildFailure {
            error: Box::new(error),
            diagnostics: parsed.diagnostics,
        });
    }
    normalize_resources(&mut config, &source.base_dir);
    apply_cli(&mut config, &opts.cli)?;
    let diagnostics = if kind == SourceKind::Typst {
        // The frontend applies its metadata strictness before CLI precedence.
        // An explicit false override restores the base level of Typst warnings.
        let diagnostics = if opts.cli.strict == Some(false) {
            parsed
                .diagnostics
                .into_iter()
                .map(|mut diagnostic| {
                    if diagnostic.level == Level::Error
                        && diagnostic.code.starts_with("W-TYP-")
                    {
                        diagnostic.level = Level::Warning;
                    }
                    diagnostic
                })
                .collect()
        } else {
            parsed.diagnostics
        };
        escalate_strict(diagnostics, opts.cli.strict, &config)
    } else {
        parsed.diagnostics
    };
    Ok(Prepared {
        document: parsed.document,
        assets: parsed.assets,
        config,
        config_path,
        diagnostics,
    })
}

/// Parses and resolves one source; preview and lint requests are lenient.
pub fn prepare(
    opts: &BuildOptions,
) -> std::result::Result<Prepared, BuildFailure> {
    let mut prepared = parse_only(opts)?;
    let diagnostics = resolve::resolve(
        &mut prepared.document,
        &ResolveOptions {
            config: &prepared.config,
            library: None,
            lenient: !matches!(opts.mode, Mode::Build),
        },
    );
    prepared.diagnostics.extend(diagnostics);
    Ok(prepared)
}

/// Loads the explicit or nearest project config and applies CLI overrides.
/// Project resource paths are absolute; an unset default `dist` stays relative.
pub fn load_config(opts: &BuildOptions) -> Result<(Config, Option<PathBuf>)> {
    let (mut config, path) = match &opts.config_path {
        Some(path) => (Config::load(path)?, Some(absolute(path)?)),
        None => {
            let input = absolute(&opts.input)?;
            let dir = input.parent().unwrap_or(Path::new("/"));
            match Config::discover(dir)? {
                Some((path, config)) => (config, Some(path)),
                None => (Config::default(), None),
            }
        }
    };
    if let Some(file) = path.as_deref() {
        let dir = file.parent().unwrap_or(Path::new("/"));
        if config.build.out_dir.is_relative()
            && sets_key(file, "build", "out_dir")
        {
            config.build.out_dir =
                normalize_lexically(&dir.join(&config.build.out_dir));
        }
        normalize_resources(&mut config, dir);
        absolutize_paths(&mut config.pdf.font_paths, dir);
        absolutize_paths(&mut config.typst.package_paths, dir);
    }
    apply_cli(&mut config, &opts.cli)?;
    Ok((config, path))
}

fn apply_cli(config: &mut Config, cli: &CliOverrides) -> Result<()> {
    config.apply_cli(cli);
    if cli.template.is_some()
        || cli.reference_doc.is_some()
        || cli.latex_template.is_some()
    {
        let cwd = absolute(Path::new("."))?;
        if cli.template.is_some() {
            absolutize(&mut config.pdf.template, &cwd);
        }
        if cli.reference_doc.is_some() {
            absolutize(&mut config.docx.reference_doc, &cwd);
        }
        if cli.latex_template.is_some() {
            absolutize(&mut config.latex.template, &cwd);
        }
    }
    Ok(())
}

fn normalize_resources(config: &mut Config, dir: &Path) {
    absolutize(&mut config.pdf.template, dir);
    absolutize(&mut config.docx.reference_doc, dir);
    absolutize(&mut config.latex.template, dir);
    absolutize_paths(&mut config.references.bibliography, dir);
    if is_csl(&config.references.style) {
        let mut style = PathBuf::from(&config.references.style);
        absolutize(&mut style, dir);
        config.references.style = style.to_string_lossy().into_owned();
    }
}

fn absolutize_paths(paths: &mut [PathBuf], dir: &Path) {
    for path in paths {
        absolutize(path, dir);
    }
}

fn absolutize(path: &mut PathBuf, dir: &Path) {
    if !path.as_os_str().is_empty() && path.is_relative() {
        *path = normalize_lexically(&dir.join(&*path));
    }
}

fn is_csl(style: &str) -> bool {
    Path::new(style)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("csl"))
}

fn sets_key(path: &Path, table: &str, key: &str) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .is_some_and(|value| {
            value.get(table).and_then(|v| v.get(key)).is_some()
        })
}

fn meta_sets(
    prepared: &Prepared, key: &str, table: &str, nested: &str,
) -> bool {
    let vars = &prepared.document.meta.vars;
    vars.contains_key(key)
        || vars.get(table).and_then(|v| v.get(nested)).is_some()
}

/// Resolves the HTML theme against the origin of its winning setting.
pub fn resolve_theme(
    prepared: &Prepared, opts: &BuildOptions,
) -> Result<Theme> {
    let cwd;
    let roots: Vec<&Path> = if opts.cli.theme.is_some() {
        cwd = absolute(Path::new("."))?;
        vec![&cwd]
    } else if meta_sets(prepared, "theme", "html", "theme") {
        vec![&prepared.document.source.base_dir]
    } else if let Some(file) = prepared.config_path.as_deref()
        && sets_key(file, "html", "theme")
    {
        file.parent().into_iter().collect()
    } else {
        Vec::new()
    };
    theme::resolve(&prepared.config.html.theme, &roots)
}

/// Every source, resource and theme directory to watch, in first-use order.
pub fn watch_inputs(prepared: &Prepared) -> Vec<(PathBuf, InputRole)> {
    let mut inputs =
        vec![(prepared.document.source.path.clone(), InputRole::Source)];
    inputs.extend(
        prepared
            .document
            .includes
            .iter()
            .cloned()
            .map(|p| (p, InputRole::Include)),
    );
    inputs.extend(
        prepared.config_path.iter().cloned().map(|p| (p, InputRole::Config)),
    );
    inputs.extend(prepared.assets.iter().filter_map(|a| match &a.data {
        AssetData::File(path) => Some((path.clone(), InputRole::Asset)),
        AssetData::Bytes(_) => None,
    }));
    inputs.extend(
        prepared
            .diagnostics
            .iter()
            .filter_map(missing_asset_path)
            .map(|p| (p, InputRole::MissingAsset)),
    );
    let config = &prepared.config;
    inputs.extend(
        config
            .references
            .bibliography
            .iter()
            .cloned()
            .map(|p| (p, InputRole::Bibliography)),
    );
    if is_csl(&config.references.style) {
        inputs.push((
            config.references.style.clone().into(),
            InputRole::Bibliography,
        ));
    }
    for (path, role) in [
        (&config.pdf.template, InputRole::PdfTemplate),
        (&config.docx.reference_doc, InputRole::ReferenceDoc),
        (&config.latex.template, InputRole::LatexTemplate),
    ] {
        if !path.as_os_str().is_empty() {
            inputs.push((path.clone(), role));
        }
    }
    inputs.extend(
        theme_dirs(prepared).into_iter().map(|p| (p, InputRole::Theme)),
    );
    inputs
}

fn theme_dirs(prepared: &Prepared) -> Vec<PathBuf> {
    let cwd = std::env::current_dir().ok();
    let roots: Vec<&Path> = prepared
        .config_path
        .as_deref()
        .and_then(Path::parent)
        .into_iter()
        .chain([prepared.document.source.base_dir.as_path()])
        .chain(cwd.as_deref())
        .collect();
    let mut dirs = Vec::new();
    for root in &roots {
        if let Ok(theme) = theme::resolve(&prepared.config.html.theme, &[*root])
        {
            dirs.extend(theme.watch_dirs());
        }
    }
    if !dirs.is_empty()
        || theme::resolve(&prepared.config.html.theme, &roots).is_ok()
    {
        return dirs;
    }
    dirs = roots
        .iter()
        .map(|root| root.join(&prepared.config.html.theme))
        .collect();
    let mut next = dirs
        .iter()
        .find(|dir| dir.join(theme::resolve::MANIFEST_FILE).is_file())
        .cloned();
    for _ in 0..theme::resolve::MAX_CHAIN_LEN {
        let Some(parent) =
            next.take().and_then(|dir| Some(dir.join(extends(&dir)?)))
        else {
            break;
        };
        next = parent
            .join(theme::resolve::MANIFEST_FILE)
            .is_file()
            .then(|| parent.clone());
        dirs.push(parent);
    }
    dirs
}

fn extends(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(theme::resolve::MANIFEST_FILE))
        .ok()?;
    let table: toml::Table = text.parse().ok()?;
    table.get("extends")?.as_str().map(str::to_string)
}

/// First error belonging to the structured Typst subset, in diagnostic order.
pub fn subset_error(diagnostics: &Diagnostics) -> Option<&Diagnostic> {
    diagnostics.iter().find(|d| {
        d.level == Level::Error
            && (d.code.starts_with("E-TYP-") || d.code.starts_with("W-TYP-"))
    })
}

fn subset_parse(diagnostic: &Diagnostic, input: &Path) -> Error {
    Error::Parse {
        path: diagnostic
            .span
            .as_ref()
            .and_then(|s| s.file.clone())
            .or_else(|| diagnostic.path.clone())
            .unwrap_or_else(|| input.to_path_buf()),
        line: diagnostic.span.as_ref().map(|s| s.start_line),
        message: diagnostic.message.clone(),
    }
}

/// Renders one web view, preserving prepared diagnostics on their owner.
pub fn web_page(
    prepared: &Prepared, opts: &BuildOptions, world: &mut WriterWorld,
) -> std::result::Result<WebPage, BuildFailure> {
    if let Some(diagnostic) = subset_error(&prepared.diagnostics) {
        return Err(subset_parse(diagnostic, &opts.input).into());
    }
    let theme = resolve_theme(prepared, opts)?;
    let (output, build) = match opts.mode {
        Mode::Serve { build_id } => {
            (OutputMode::Serve, BuildInfo::serve(build_id))
        }
        _ => (OutputMode::Standalone, BuildInfo::build(true)),
    };
    let (rendered, assets) =
        html::render(prepared, &theme, output, build, world)?;
    Ok(WebPage {
        html: rendered.html,
        theme_css: rendered.theme_css,
        theme,
        diagnostics: rendered.diagnostics,
        assets,
    })
}

/// Loads generated Typst into the shared world without resetting the session.
pub fn load_typst_main(
    prepared: &Prepared, opts: &BuildOptions, world: &mut WriterWorld,
) -> std::result::Result<(), BuildFailure> {
    pdf::load_typst_main(prepared, opts, world)
}

/// Chooses a target's file, or its site directory, following `--out` semantics.
pub fn output_path(
    prepared: &Prepared, opts: &BuildOptions, target: Target, n_targets: usize,
) -> PathBuf {
    if let Some(out) = &opts.cli.out
        && n_targets == 1
    {
        return out.clone();
    }
    let dir = opts.cli.out.clone().unwrap_or_else(|| {
        prepared.document.source.base_dir.join(&prepared.config.build.out_dir)
    });
    let stem = prepared
        .document
        .source
        .path
        .file_stem()
        .unwrap_or_else(|| std::ffi::OsStr::new("document"));
    let mut name = stem.to_os_string();
    if target != Target::Html || prepared.config.html.standalone {
        name.push(match target {
            Target::Html => ".html",
            Target::Pdf => ".pdf",
            Target::Docx => ".docx",
            Target::Latex => ".tex",
            Target::Typst => ".typ",
        });
    }
    dir.join(name)
}

/// Prepares once, then builds each requested target independently.
pub fn build_targets(
    opts: &BuildOptions, targets: &[Target], worlds: &mut WorldSlot,
) -> BuildReport {
    let count = targets.iter().collect::<std::collections::HashSet<_>>().len();
    build_targets_with_count(opts, targets, worlds, count)
}

/// Retains original output-directory semantics during a selective watch rebuild.
pub(crate) fn build_targets_with_count(
    opts: &BuildOptions, targets: &[Target], worlds: &mut WorldSlot,
    requested_count: usize,
) -> BuildReport {
    build_targets_observed(opts, targets, worlds, requested_count, &mut |_| {})
}

/// A successfully written LaTeX directory, measured from unique materialized bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WrittenDirectory {
    pub(crate) path: PathBuf,
    pub(crate) files: usize,
    pub(crate) bytes: usize,
}

/// Keeps reporting metadata private and scoped to this exact completed build.
pub(crate) fn build_targets_recording(
    opts: &BuildOptions, targets: &[Target], worlds: &mut WorldSlot,
    requested_count: usize,
) -> (BuildReport, Vec<WrittenDirectory>) {
    let mut directories = Vec::new();
    let report = build_targets_observed(
        opts,
        targets,
        worlds,
        requested_count,
        &mut |directory| directories.push(directory),
    );
    (report, directories)
}

fn build_targets_observed(
    opts: &BuildOptions, targets: &[Target], worlds: &mut WorldSlot,
    requested_count: usize, record: &mut dyn FnMut(WrittenDirectory),
) -> BuildReport {
    let mut prepared = match prepare(opts) {
        Ok(prepared) => prepared,
        Err(failure) => return failed_report(failure, opts.cli.strict),
    };
    let native_pdf = prepared.document.source.kind == SourceKind::Typst
        && targets.contains(&Target::Pdf);
    let native_only =
        native_pdf && targets.iter().all(|target| *target == Target::Pdf);
    if !native_pdf {
        match escalate_missing_assets(
            prepared.diagnostics,
            matches!(opts.mode, Mode::Build),
        ) {
            Ok(diagnostics) => prepared.diagnostics = diagnostics,
            Err(failure) => return failed_report(failure, opts.cli.strict),
        }
    }
    let world = match worlds.get(Some(&prepared), opts) {
        Ok(world) => world,
        Err(failure) => return failed_report(failure, opts.cli.strict),
    };
    world.reset();
    let mut unique = Vec::new();
    for target in targets {
        if !unique.contains(target) {
            unique.push(*target);
        }
    }
    // Native PDF consumes the original source; subset findings apply only to
    // IR targets. Keep them on Prepared for provenance and later web views.
    let mut diagnostics = if native_only {
        Diagnostics::new()
    } else {
        prepared
            .diagnostics
            .clone()
            .into_iter()
            .map(|mut finding| {
                if native_pdf
                    && matches!(opts.mode, Mode::Build)
                    && finding.code == "missing-asset"
                {
                    finding.level = Level::Error;
                }
                finding
            })
            .collect()
    };
    let mut results = Vec::new();
    for target in &unique {
        let out = output_path(&prepared, opts, *target, requested_count);
        let result = if native_pdf
            && matches!(target, Target::Html | Target::Docx | Target::Latex)
        {
            match escalate_missing_assets(
                prepared.diagnostics.clone(),
                matches!(opts.mode, Mode::Build),
            ) {
                Ok(_) => {
                    build_target(&prepared, opts, *target, &out, world, record)
                }
                Err(mut failure) => {
                    // The report already owns these preparation findings.
                    failure.diagnostics = Diagnostics::new();
                    Err(failure)
                }
            }
        } else {
            build_target(&prepared, opts, *target, &out, world, record)
        };
        let written = match result {
            Ok((written, found)) => {
                diagnostics.extend(found);
                Ok(written)
            }
            Err(failure) => {
                diagnostics.extend(failure.diagnostics);
                Err(*failure.error)
            }
        };
        results.push(TargetResult { target: *target, written });
    }
    if native_pdf {
        diagnostics = merge_missing_asset_findings(diagnostics);
    }
    let diagnostics =
        escalate_strict(diagnostics, opts.cli.strict, &prepared.config);
    BuildReport { prepared: Ok(prepared), targets: results, diagnostics }
}

// Native providers can report the same missing image that Prepared already
// owns. Its IR-target Error and native-target Warning describe one finding.
// Preserve first-seen order, exact source identity and the strongest severity.
fn merge_missing_asset_findings(diagnostics: Diagnostics) -> Diagnostics {
    let mut merged: Vec<Diagnostic> = Vec::new();
    for finding in diagnostics {
        if finding.code == "missing-asset"
            && let Some(previous) = merged.iter_mut().find(|previous| {
                previous.code == finding.code
                    && previous.message == finding.message
                    && previous.path == finding.path
                    && previous.span == finding.span
            })
        {
            if matches!(
                (previous.level, finding.level),
                (Level::Info, Level::Warning | Level::Error)
                    | (Level::Warning, Level::Error)
            ) {
                previous.level = finding.level;
            }
        } else {
            merged.push(finding);
        }
    }
    merged.into()
}

fn failed_report(
    mut failure: BuildFailure, strict: Option<bool>,
) -> BuildReport {
    failure.diagnostics =
        escalate_strict(failure.diagnostics, strict, &Config::default());
    BuildReport {
        prepared: Err(failure),
        targets: Vec::new(),
        diagnostics: Diagnostics::default(),
    }
}

fn build_target(
    prepared: &Prepared, opts: &BuildOptions, target: Target, out: &Path,
    world: &mut WriterWorld, record: &mut dyn FnMut(WrittenDirectory),
) -> std::result::Result<(Vec<PathBuf>, Diagnostics), BuildFailure> {
    let file_output = target != Target::Html || prepared.config.html.standalone;
    if file_output && names_directory(out) {
        return Err(Error::Config(format!(
            "output {} names a directory; give an output file or use HTML site mode",
            out.display(),
        )).into());
    }
    let main = if target == Target::Html {
        html::document_path(out, prepared.config.html.standalone)
    } else {
        out.to_path_buf()
    };
    if let Some(clash) = find_clash([main.as_path()], &input_files(prepared)) {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            clash,
        ))
        .into());
    }
    if matches!(target, Target::Html | Target::Docx | Target::Latex)
        && let Some(diagnostic) = subset_error(&prepared.diagnostics)
    {
        return Err(subset_parse(diagnostic, &opts.input).into());
    }
    match target {
        Target::Html => html::build(prepared, opts, out, world),
        Target::Pdf if prepared.document.source.kind == SourceKind::Typst => {
            return typst_source::build_detailed(prepared, opts, out, world);
        }
        Target::Pdf if prepared.config.pdf.engine == PdfEngine::Chrome => {
            crate::pdf::chrome::build(prepared, opts, out, world)
        }
        Target::Pdf => return pdf::build_detailed(prepared, opts, out, world),
        Target::Docx => {
            let diagnostics = docx::build(
                &prepared.document, &prepared.assets, &prepared.config,
                out, world,
            )?;
            Ok((vec![out.to_path_buf()], diagnostics))
        }
        Target::Latex => return latex::build_observed(prepared, opts, out, world, record),
        Target::Typst if prepared.document.source.kind == SourceKind::Typst => Err(Error::Other("cannot export a .typ input to typst; use writer build part.md --to typst".into())),
        Target::Typst => typst::build(prepared, opts, out, world),
    }.map_err(Into::into)
}

fn names_directory(path: &Path) -> bool {
    if path.is_dir() {
        return true;
    }
    let spelling = path.to_string_lossy();
    let trailing = spelling.ends_with(std::path::MAIN_SEPARATOR)
        || spelling.ends_with('/');
    let last = spelling.rsplit([std::path::MAIN_SEPARATOR, '/']).next();
    trailing || matches!(last, Some("." | ".."))
}

/// Applies CLI-wide or Typst-only strictness, without changing diagnostic codes.
pub fn escalate_strict(
    diagnostics: Diagnostics, cli_strict: Option<bool>, config: &Config,
) -> Diagnostics {
    diagnostics
        .into_vec()
        .into_iter()
        .map(|mut diagnostic| {
            let typst = diagnostic.code == "E-TYP-MATH-CODE"
                || diagnostic.code.starts_with("W-TYP-");
            let target = matches!(
                diagnostic.code,
                "unsupported-construct" | "math-conversion-failed"
            );
            if diagnostic.level == Level::Warning
                && ((cli_strict == Some(true) && (typst || target))
                    || (config.typst.strict && typst))
            {
                diagnostic.level = Level::Error;
            }
            diagnostic
        })
        .collect()
}

/// The standard error for a command or target whose owner task is pending.
pub fn not_implemented(what: &str, task: &str) -> Error {
    Error::Other(format!("{what}: not implemented yet ({task})"))
}

/// Whether an error is one of the explicit future-task stubs.
pub fn is_not_implemented(error: &Error) -> bool {
    matches!(error, Error::Other(message) if message.contains(": not implemented yet (") && message.ends_with(')'))
}

/// Retains one compiler world throughout a build or preview session.
#[derive(Default)]
pub struct WorldSlot {
    world: Option<WriterWorld>,
    font_paths: Vec<PathBuf>,
    system_fonts: bool,
    allow_packages: bool,
    package_paths: Vec<PathBuf>,
    from_prepare: bool,
    created: u64,
}

impl WorldSlot {
    /// An empty compiler slot.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the retained world, recreating only for construction settings.
    pub fn get(
        &mut self, prepared: Option<&Prepared>, opts: &BuildOptions,
    ) -> std::result::Result<&mut WriterWorld, BuildFailure> {
        if prepared.is_some() || self.world.is_none() {
            self.refresh(prepared, opts)?;
        }
        // Refresh creates a world or returns an error; the other branch retains one.
        Ok(self.world.as_mut().expect("world created or retained"))
    }

    fn refresh(
        &mut self, prepared: Option<&Prepared>, opts: &BuildOptions,
    ) -> std::result::Result<(), BuildFailure> {
        let fallback;
        let config = if let Some(prepared) = prepared {
            &prepared.config
        } else {
            fallback = load_config(opts).map(|v| v.0).unwrap_or_default();
            &fallback
        };
        let now = SystemTime::now();
        let timestamp = if let Some(prepared) = prepared {
            let epoch = std::env::var("SOURCE_DATE_EPOCH").ok();
            pdf_timestamp(
                &config.pdf,
                &prepared.document.meta,
                epoch.as_deref(),
                now,
            )
        } else {
            pdf_timestamp(&Default::default(), &Default::default(), None, now)
        };
        let timestamp = match timestamp {
            Ok(timestamp) => timestamp,
            Err(error) => {
                self.discard();
                return Err(error.into());
            }
        };
        if self.needs_recreation(config) {
            self.create(config, prepared, opts, timestamp)?;
        } else if prepared.is_some() {
            // All paths reaching this branch retain the previously created world.
            let world = self.world.as_mut().expect("kept world exists");
            world.set_inputs(world_inputs(config));
            world.set_include_provider(Box::new(MarkdownIncludes {
                config: config.clone(),
            }));
        }
        Ok(())
    }

    fn needs_recreation(&self, config: &Config) -> bool {
        self.world.is_none()
            || !self.from_prepare
            || self.font_paths != config.pdf.font_paths
            || self.system_fonts != config.pdf.system_fonts
            || self.allow_packages != config.pdf.allow_packages
            || self.package_paths != config.typst.package_paths
    }

    fn create(
        &mut self, config: &Config, prepared: Option<&Prepared>,
        opts: &BuildOptions, timestamp: ::typst::foundations::Datetime,
    ) -> Result<()> {
        self.discard();
        let input = absolute(&opts.input)?;
        let root = input.parent().unwrap_or(Path::new("/"));
        let kind = prepared
            .map(|p| p.document.source.kind)
            .or(opts.format)
            .or_else(|| crate::ir::detect_kind(&input).ok());
        let main = if kind == Some(SourceKind::Typst) {
            MainFile::Path(input.clone())
        } else {
            MainFile::Virtual
        };
        let mut world =
            WriterWorld::from_config(config, root, main, timestamp)?;
        world.set_include_provider(Box::new(MarkdownIncludes {
            config: config.clone(),
        }));
        self.world = Some(world);
        self.font_paths = config.pdf.font_paths.clone();
        self.system_fonts = config.pdf.system_fonts;
        self.allow_packages = config.pdf.allow_packages;
        self.package_paths = config.typst.package_paths.clone();
        self.from_prepare = prepared.is_some();
        self.created += 1;
        Ok(())
    }

    /// The current world, if creation has succeeded.
    pub fn current(&mut self) -> Option<&mut WriterWorld> {
        self.world.as_mut()
    }

    /// Discards compiler state so the next request creates a fresh world.
    pub fn discard(&mut self) {
        self.world = None;
    }
}

/// An output file that is one of the build's inputs, so that writing it would destroy that
/// input (audit F01). Found by [`find_clash`] and [`html::build`]; [`html::build`]
/// refuses to write when there is one (its error carries the clash: [`refused_clash`]).
/// Displayed as `refusing to write <output>: it is the input file <input>` (paths as
/// stored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clash {
    /// The output file, as it would be written.
    pub output: PathBuf,
    /// The input file it is: the same file, possibly through a symbolic or hard link or
    /// another spelling of its name on a case-insensitive file system.
    pub input: PathBuf,
    /// What `input` is to the build.
    pub role: InputRole,
}

impl fmt::Display for Clash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "refusing to write {}: it is {} {}",
            self.output.display(),
            self.role,
            self.input.display()
        )
    }
}

impl std::error::Error for Clash {}

/// The first of `outputs` that is one of `inputs` (the same file, by [`same_file`]'s
/// identity; the first matching input names it), if any. An output path that does not
/// resolve yet is taken as the file it reaches once [`html::build`] has created its
/// missing directories ([`path_after_creating_dirs`]: `sub/../slides.md` is `slides.md`).
/// Relative paths are taken from the current directory.
pub fn find_clash<'a>(
    outputs: impl IntoIterator<Item = &'a Path>,
    inputs: &[(PathBuf, InputRole)],
) -> Option<Clash> {
    let existing: Vec<(FileId, &PathBuf, InputRole)> = inputs
        .iter()
        .filter_map(|(path, role)| Some((file_id(path)?, path, *role)))
        .collect();
    if existing.is_empty() {
        return None;
    }
    outputs.into_iter().find_map(|output| {
        let id = file_id(output)
            .or_else(|| file_id(&path_after_creating_dirs(output)?))?;
        let (_, input, role) =
            existing.iter().find(|(known, ..)| *known == id)?;
        Some(Clash {
            output: output.to_path_buf(),
            input: (*input).clone(),
            role: *role,
        })
    })
}

/// Whether `a` and `b` name the same existing file. On Unix: the same device and inode,
/// which sees through symbolic links, hard links and other spellings of a name on a
/// case-insensitive file system. Elsewhere: the same canonical path (hard links are not
/// detected there). `false` when either does not exist.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (file_id(a), file_id(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// The path a write to `path` reaches once its missing directories are created (as
/// [`html::build`] does), with every symbolic link that the write follows resolved.
///
/// The components are followed one at a time, as the operating system follows them. While
/// every directory so far exists, the next name is looked up on disk: an existing file,
/// directory or symbolic link is replaced by its canonical path, so a later `..` leaves
/// the directory a link points to, not the link's own directory. A name that does not
/// exist starts the directories the write creates; a `..` after it returns to the
/// directory it was created in (where lookups on disk resume) and `.` is skipped. So
/// `new/../link/../x.md` is `x.md` next to the directory that `link` points to.
///
/// A relative `path` is taken from the current directory; `None` when that cannot be
/// resolved. The result is absolute, and canonical as far as it exists.
pub fn path_after_creating_dirs(path: &Path) -> Option<PathBuf> {
    let mut base = if path.has_root() {
        PathBuf::new()
    } else {
        std::fs::canonicalize(".").ok()?
    };
    // Names below `base` that do not exist yet: the directories the write creates.
    let mut created: Vec<&std::ffi::OsStr> = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => base.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                if created.pop().is_none()
                    && let Some(parent) = base.parent()
                {
                    base = parent.to_path_buf();
                }
            }
            Component::Normal(name) => {
                let existing = created
                    .is_empty()
                    .then(|| std::fs::canonicalize(base.join(name)).ok())
                    .flatten();
                match existing {
                    Some(real) => base = real,
                    None => created.push(name),
                }
            }
        }
    }
    Some(created.iter().fold(base, |dir, name| dir.join(name)))
}

/// The identity of an existing file ([`same_file`]).
#[cfg(unix)]
type FileId = (u64, u64);

/// The identity of an existing file ([`same_file`]).
#[cfg(not(unix))]
type FileId = PathBuf;

/// `path`'s device and inode (symbolic links followed), or `None` when it does not exist.
#[cfg(unix)]
fn file_id(path: &Path) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt as _;
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.dev(), meta.ino()))
}

/// `path`'s canonical form, or `None` when it does not exist.
#[cfg(not(unix))]
fn file_id(path: &Path) -> Option<FileId> {
    std::fs::canonicalize(path).ok()
}

/// The [`Clash`] that made [`html::build`] refuse to write, when `error` is that refusal
/// (for a caller that reports it in its own words, like `build --watch`); `None` for every
/// other error.
pub fn refused_clash(error: &Error) -> Option<&Clash> {
    match error {
        Error::Io(err) => err.get_ref()?.downcast_ref::<Clash>(),
        _ => None,
    }
}

/// Writes `bytes` to `path`, creating its parent directories.
fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| io_error(e, "cannot create", parent))?;
    }
    std::fs::write(path, bytes).map_err(|e| io_error(e, "cannot write", path))
}

/// The input's text. Errors name the file as given.
fn read_source(input: &Path) -> Result<String> {
    let bytes =
        std::fs::read(input).map_err(|e| io_error(e, "cannot read", input))?;
    String::from_utf8(bytes).map_err(|e| Error::Parse {
        path: input.to_path_buf(),
        line: None,
        message: format!("not valid UTF-8: {e}"),
    })
}

/// [`SourceInfo`] for `input`: absolute, lexically normalized path; `base_dir` its parent.
fn source_info(input: &Path, kind: SourceKind) -> Result<SourceInfo> {
    let path = absolute(input)?;
    let base_dir = path.parent().unwrap_or(Path::new("/")).to_path_buf();
    Ok(SourceInfo { path, kind, base_dir })
}

/// `path` made absolute against the current directory and lexically normalized.
fn absolute(path: &Path) -> Result<PathBuf> {
    let abs = std::path::absolute(path)
        .map_err(|e| io_error(e, "cannot resolve", path))?;
    Ok(normalize_lexically(&abs))
}

/// An [`Error::Io`] whose message names `path`.
fn io_error(err: std::io::Error, what: &str, path: &Path) -> Error {
    Error::Io(std::io::Error::new(
        err.kind(),
        format!("{what} {}: {err}", path.display()),
    ))
}

/// With `missing_assets_are_errors`, the first `missing-asset` diagnostic becomes a
/// [`BuildFailure`] whose error is [`Error::MissingAsset`] and whose diagnostics are the
/// others, with every other `missing-asset` raised to level error. Otherwise (or without
/// one) `diagnostics` is returned unchanged.
fn escalate_missing_assets(
    diagnostics: Diagnostics, missing_assets_are_errors: bool,
) -> std::result::Result<Diagnostics, BuildFailure> {
    if !missing_assets_are_errors {
        return Ok(diagnostics);
    }
    let Some(error) = missing_asset_error(&diagnostics) else {
        return Ok(diagnostics);
    };
    let mut first = true;
    let rest = diagnostics
        .into_iter()
        .filter_map(|mut diagnostic| {
            if diagnostic.code == "missing-asset" {
                if std::mem::take(&mut first) {
                    return None;
                }
                diagnostic.level = Level::Error;
            }
            Some(diagnostic)
        })
        .collect();
    Err(BuildFailure { error: Box::new(error), diagnostics: rest })
}

/// The first `missing-asset` diagnostic as [`Error::MissingAsset`]
/// ([`missing_asset_as_error`]), if there is one.
fn missing_asset_error(diagnostics: &Diagnostics) -> Option<Error> {
    diagnostics.iter().find_map(missing_asset_as_error)
}

/// A `missing-asset` diagnostic as the [`Error::MissingAsset`] an escalated build fails
/// with: the missing file from its message (else the diagnostic's path), the span from the
/// diagnostic. `None` for any other diagnostic. The CLI uses it to report every missing
/// asset of a failed build the same way (audit F37).
///
/// For a missing asset of an included file the span's `file` names that file
/// (`SourceSpan.file`, T48): the CLI's report and the serve overlay locate and quote it there.
pub fn missing_asset_as_error(diagnostic: &Diagnostic) -> Option<Error> {
    if diagnostic.code != "missing-asset" {
        return None;
    }
    let path = missing_asset_path(diagnostic)
        .or_else(|| diagnostic.path.clone())
        .unwrap_or_default();
    Some(Error::MissingAsset { path, span: diagnostic.span.clone() })
}

/// Missing path encoded by a frontend's stable diagnostic message.
fn missing_asset_path(diagnostic: &Diagnostic) -> Option<PathBuf> {
    if diagnostic.code != "missing-asset" {
        return None;
    }
    let (_, path) =
        diagnostic.message.strip_suffix(')')?.rsplit_once(" (looked for ")?;
    Some(path.into())
}

/// Files protected from overwrite, expanding watched theme directories.
pub(crate) fn input_files(prepared: &Prepared) -> Vec<(PathBuf, InputRole)> {
    let mut inputs = Vec::new();
    for (path, role) in watch_inputs(prepared) {
        if role == InputRole::Theme {
            if let Ok(theme) = theme::resolve(&path.to_string_lossy(), &[]) {
                for dir in &theme.chain {
                    inputs.extend(
                        theme_files(dir).into_iter().map(|p| (p, role)),
                    );
                }
            } else {
                inputs.push((path.join(theme::resolve::MANIFEST_FILE), role));
            }
        } else {
            inputs.push((path, role));
        }
    }
    inputs
}

fn theme_files(theme_dir: &ThemeDir) -> Vec<PathBuf> {
    let ThemeSource::Disk { dir } = &theme_dir.source else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.is_file() && !has_output_extension(path))
        .collect();
    files.sort();
    for sub in ["templates", "styles", "assets", "scripts"] {
        files.extend(theme_dir.list(sub).into_iter().map(|rel| dir.join(rel)));
    }
    let manifest = &theme_dir.manifest;
    let named = manifest
        .styles
        .entries
        .iter()
        .chain(&manifest.scripts.entries)
        .chain(
            manifest
                .styles
                .code_theme
                .iter()
                .filter(|s| s.ends_with(".tmTheme")),
        )
        .chain(
            manifest
                .styles
                .code_theme_dark
                .iter()
                .filter(|s| s.ends_with(".tmTheme")),
        );
    files.extend(
        named
            .filter_map(|rel| theme::normalize_rel(rel))
            .filter(|rel| !rel.is_empty())
            .map(|rel| dir.join(rel)),
    );
    files
}

fn has_output_extension(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
        ["html", "htm", "pdf", "docx", "tex", "typ"]
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HeadingNumbering;
    use crate::ir::{DocMeta, SourceSpan};

    #[test]
    fn native_missing_asset_merge_keeps_strongest_level_and_first_position() {
        let missing = |level| {
            Diagnostic::new(
                level,
                "missing-asset",
                "image missing (looked for /missing.png)",
            )
            .with_path("part.md")
            .with_span(SourceSpan {
                start_line: 3,
                end_line: 3,
                cell: Some(2),
                file: Some("part.md".into()),
            })
        };
        let other = Diagnostic::warning("typst-warning", "keep both copies");
        for levels in [
            [Level::Error, Level::Warning],
            [Level::Warning, Level::Error],
            [Level::Info, Level::Error],
        ] {
            let found = merge_missing_asset_findings(
                vec![
                    other.clone(),
                    missing(levels[0]),
                    other.clone(),
                    missing(levels[1]),
                ]
                .into(),
            );
            assert_eq!(
                found.into_vec(),
                [other.clone(), missing(Level::Error), other.clone(),]
            );
        }
        let warning = missing(Level::Warning);
        assert_eq!(
            merge_missing_asset_findings(vec![warning.clone()].into())
                .into_vec(),
            [warning],
        );
    }

    #[test]
    fn native_missing_asset_merge_preserves_different_source_findings() {
        let first = Diagnostic::warning("missing-asset", "same missing image")
            .with_path("one.ipynb")
            .with_span(SourceSpan {
                start_line: 1,
                end_line: 1,
                cell: Some(0),
                file: Some("one.ipynb".into()),
            });
        let mut next_cell = first.clone();
        next_cell.span.as_mut().unwrap().cell = Some(1);
        let mut next_line = first.clone();
        next_line.span.as_mut().unwrap().start_line = 2;
        next_line.span.as_mut().unwrap().end_line = 2;
        let mut next_file = first.clone();
        next_file.path = Some("two.ipynb".into());
        next_file.span.as_mut().unwrap().file = Some("two.ipynb".into());
        let mut next_asset = first.clone();
        next_asset.message = "another missing image".into();
        let expected = vec![first, next_cell, next_line, next_file, next_asset];
        assert_eq!(
            merge_missing_asset_findings(expected.clone().into()).into_vec(),
            expected,
        );
    }

    #[test]
    fn build_failure_stays_below_clippy_result_size_threshold() {
        assert!(std::mem::size_of::<BuildFailure>() < 128);
    }

    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, contents) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        dir
    }

    fn prepared(dir: &Path) -> (BuildOptions, Prepared) {
        let opts = BuildOptions::new(dir.join("document.md"));
        let mut config = Config::default();
        config.pdf.system_fonts = false;
        let source = source_info(&opts.input, SourceKind::Markdown).unwrap();
        let parsed =
            crate::markdown::parse(&source, "# Heading\n", &config).unwrap();
        let prepared = Prepared {
            document: parsed.document,
            assets: parsed.assets,
            config,
            config_path: None,
            diagnostics: parsed.diagnostics,
        };
        (opts, prepared)
    }

    #[test]
    fn world_slot_creates_and_keeps() {
        let dir = tempfile::tempdir().unwrap();
        let (opts, prepared) = prepared(dir.path());
        let mut worlds = WorldSlot::new();
        assert!(worlds.current().is_none());
        worlds.get(Some(&prepared), &opts).unwrap();
        assert_eq!(worlds.created, 1);
        worlds.get(Some(&prepared), &opts).unwrap();
        worlds.get(None, &opts).unwrap();
        assert_eq!(worlds.created, 1);
    }

    #[test]
    fn world_slot_recreates_on_font_paths_change() {
        let dir = tempfile::tempdir().unwrap();
        let (opts, mut prepared) = prepared(dir.path());
        let mut worlds = WorldSlot::new();
        worlds.get(Some(&prepared), &opts).unwrap();
        prepared.config.pdf.font_paths.push(dir.path().join("fonts"));
        worlds.get(Some(&prepared), &opts).unwrap();
        assert_eq!(worlds.created, 2);
        prepared.config.typst.package_paths.push(dir.path().join("packages"));
        worlds.get(Some(&prepared), &opts).unwrap();
        assert_eq!(worlds.created, 3);
        prepared.config.pdf.allow_packages = true;
        worlds.get(Some(&prepared), &opts).unwrap();
        assert_eq!(worlds.created, 4);
    }

    #[test]
    fn world_slot_recreates_after_world_without_prepare() {
        let dir = project(&[("writer.toml", "[pdf]\nsystem_fonts = false\n")]);
        let (opts, prepared) = prepared(dir.path());
        let mut worlds = WorldSlot::new();
        worlds.get(None, &opts).unwrap();
        assert_eq!(worlds.created, 1);
        worlds.get(Some(&prepared), &opts).unwrap();
        assert_eq!(worlds.created, 2);
    }

    #[test]
    fn world_slot_discard() {
        let dir = tempfile::tempdir().unwrap();
        let (opts, prepared) = prepared(dir.path());
        let mut worlds = WorldSlot::new();
        worlds.get(Some(&prepared), &opts).unwrap();
        worlds.discard();
        assert!(worlds.current().is_none());
        worlds.get(Some(&prepared), &opts).unwrap();
        assert_eq!(worlds.created, 2);
    }

    #[test]
    fn invalid_timestamp_leaves_slot_empty() {
        let dir = tempfile::tempdir().unwrap();
        let (opts, mut prepared) = prepared(dir.path());
        let mut worlds = WorldSlot::new();
        worlds.get(Some(&prepared), &opts).unwrap();
        prepared.config.pdf.timestamp = "yesterday".into();
        let Err(failure) = worlds.get(Some(&prepared), &opts) else {
            panic!("invalid timestamp accepted");
        };
        assert!(matches!(*failure.error, Error::Config(_)));
        assert!(failure.diagnostics.is_empty());
        assert!(worlds.current().is_none());
    }

    #[test]
    fn kept_world_sees_new_writer_config() {
        let dir = project(&[(
            "document.typ",
            "#assert.eq(json(bytes(sys.inputs.at(\"writer-config\"))).numbering.headings, \"1.1\")\nOK",
        )]);
        let (_, mut prepared) = prepared(dir.path());
        let opts = BuildOptions::new(dir.path().join("document.typ"));
        prepared.document.source =
            source_info(&opts.input, SourceKind::Typst).unwrap();
        prepared.document.meta = DocMeta::default();
        let mut worlds = WorldSlot::new();
        worlds.get(Some(&prepared), &opts).unwrap();
        prepared.config.numbering.headings =
            HeadingNumbering::Pattern("1.1".into());
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        crate::typst::compile::compile_document(world).into_result().unwrap();
        assert_eq!(worlds.created, 1);
    }

    #[test]
    fn md_include_provider_is_installed_across_session_resets() {
        use ::typst::World;
        use ::typst::syntax::{RootedPath, VirtualPath, VirtualRoot};
        let dir = project(&[("x.md", "# Included\n")]);
        let (opts, prepared) = prepared(dir.path());
        let mut worlds = WorldSlot::new();
        let id = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("x.md").unwrap(),
        )
        .intern();
        for _ in 0..2 {
            let world = worlds.get(Some(&prepared), &opts).unwrap();
            let source = world.source(id).unwrap();
            assert!(source.text().contains("= Included"));
            assert!(source.text().contains("#import \"@writer/doc:0.1.0\""));
            assert_eq!(world.file(id).unwrap().as_slice(), b"# Included\n");
        }
    }

    fn subset(level: Level) -> Diagnostic {
        Diagnostic::new(
            level,
            if level == Level::Error {
                "E-TYP-SYNTAX"
            } else {
                "W-TYP-SYNTAX"
            },
            "bad syntax",
        )
        .with_path("outer.typ")
        .with_span(SourceSpan {
            start_line: 3,
            end_line: 3,
            cell: None,
            file: Some("inner.typ".into()),
        })
    }

    #[test]
    fn subset_error_fails_web_page() {
        let dir = tempfile::tempdir().unwrap();
        let (opts, mut prepared) = prepared(dir.path());
        prepared.diagnostics.push(subset(Level::Error));
        let mut worlds = WorldSlot::new();
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        let Err(failure) = web_page(&prepared, &opts, world) else {
            panic!("subset error rendered");
        };
        assert!(
            matches!(*failure.error, Error::Parse { path, line: Some(3), .. } if path == Path::new("inner.typ"))
        );
        assert!(failure.diagnostics.is_empty());
    }

    #[test]
    fn subset_warning_keeps_web_page() {
        let dir = tempfile::tempdir().unwrap();
        let (opts, mut prepared) = prepared(dir.path());
        prepared.diagnostics.push(subset(Level::Warning));
        assert!(subset_error(&prepared.diagnostics).is_none());
        let mut worlds = WorldSlot::new();
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        let page = web_page(&prepared, &opts, world).unwrap();
        assert!(page.html.contains("Heading"));
        assert!(
            page.diagnostics.is_empty(),
            "prepare diagnostics are separate"
        );
    }

    fn strict_diagnostics() -> Diagnostics {
        [
            "unsupported-construct", "math-conversion-failed",
            "E-TYP-MATH-CODE", "W-TYP-MATH-CODE", "math-syntax",
            "math-tag-ignored", "missing-asset",
        ]
        .into_iter()
        .map(|code| Diagnostic::warning(code, "finding"))
        .collect()
    }

    #[test]
    fn strict_by_flag() {
        let found = escalate_strict(
            strict_diagnostics(),
            Some(true),
            &Config::default(),
        );
        let levels: Vec<_> = found.iter().map(|d| d.level).collect();
        assert_eq!(
            levels,
            [
                Level::Error,
                Level::Error,
                Level::Error,
                Level::Error,
                Level::Warning,
                Level::Warning,
                Level::Warning
            ]
        );
    }

    #[test]
    fn strict_by_config() {
        let mut config = Config::default();
        config.typst.strict = true;
        let found = escalate_strict(strict_diagnostics(), None, &config);
        let levels: Vec<_> = found.iter().map(|d| d.level).collect();
        assert_eq!(
            levels,
            [
                Level::Warning,
                Level::Warning,
                Level::Error,
                Level::Error,
                Level::Warning,
                Level::Warning,
                Level::Warning
            ]
        );
    }

    #[test]
    fn strict_is_idempotent() {
        let first = escalate_strict(
            strict_diagnostics(),
            Some(true),
            &Config::default(),
        );
        let twice =
            escalate_strict(first.clone(), Some(true), &Config::default());
        assert_eq!(first, twice);
    }

    #[test]
    fn config_front_matter_and_cli_resource_origins() {
        let dir = project(&[
            (
                "writer.toml",
                "[pdf]\ntemplate = 'project.typ'\nfont_paths = ['fonts']\nsystem_fonts = false\n[docx]\nreference_doc = 'project.docx'\n[latex]\ntemplate = 'project.tex'\n[references]\nbibliography = ['project.bib']\nstyle = 'project.csl'\n[typst]\npackage_paths = ['packages']\n",
            ),
            ("doc/plain.md", "# Plain\n"),
            (
                "doc/front.md",
                "---\ntemplate: front.typ\nreference_doc: front.docx\nlatex_template: front.tex\nbibliography: front.bib\ncite_style: front.csl\n---\n# Front\n",
            ),
        ]);
        let root = absolute(dir.path()).unwrap();
        let plain =
            prepare(&BuildOptions::new(root.join("doc/plain.md"))).unwrap();
        assert_eq!(plain.config.pdf.template, root.join("project.typ"));
        assert_eq!(plain.config.docx.reference_doc, root.join("project.docx"));
        assert_eq!(plain.config.latex.template, root.join("project.tex"));
        assert_eq!(
            plain.config.references.bibliography,
            [root.join("project.bib")]
        );
        assert_eq!(
            Path::new(&plain.config.references.style),
            root.join("project.csl")
        );
        assert_eq!(plain.config.pdf.font_paths, [root.join("fonts")]);
        assert_eq!(plain.config.typst.package_paths, [root.join("packages")]);
        let mut opts = BuildOptions::new(root.join("doc/front.md"));
        let front = prepare(&opts).unwrap();
        assert_eq!(front.config.pdf.template, root.join("doc/front.typ"));
        assert_eq!(
            front.config.docx.reference_doc,
            root.join("doc/front.docx")
        );
        assert_eq!(front.config.latex.template, root.join("doc/front.tex"));
        assert_eq!(
            front.config.references.bibliography,
            [root.join("doc/front.bib")]
        );
        assert_eq!(
            Path::new(&front.config.references.style),
            root.join("doc/front.csl")
        );
        opts.cli.template = Some("cli.typ".into());
        opts.cli.reference_doc = Some("cli.docx".into());
        opts.cli.latex_template = Some("cli.tex".into());
        let cli = prepare(&opts).unwrap();
        let cwd = absolute(Path::new(".")).unwrap();
        assert_eq!(cli.config.pdf.template, cwd.join("cli.typ"));
        assert_eq!(cli.config.docx.reference_doc, cwd.join("cli.docx"));
        assert_eq!(cli.config.latex.template, cwd.join("cli.tex"));
        let watched = watch_inputs(&cli);
        assert!(
            watched.contains(&(cwd.join("cli.typ"), InputRole::PdfTemplate))
        );
        assert!(
            watched.contains(&(
                root.join("doc/front.bib"),
                InputRole::Bibliography
            ))
        );
    }

    #[test]
    fn out_dir_is_relative_to_the_writer_toml_that_sets_it() {
        let dir = project(&[
            ("writer.toml", "[build]\nout_dir = 'out'\n"),
            ("docs/document.md", "# Document\n"),
        ]);
        let opts = BuildOptions::new(dir.path().join("docs/document.md"));
        let prepared = prepare(&opts).unwrap();
        assert_eq!(
            output_path(&prepared, &opts, Target::Html, 1),
            absolute(dir.path()).unwrap().join("out/document.html")
        );
    }

    #[test]
    fn output_names_preserve_dotted_stems_and_deduplicate_targets() {
        let dir = project(&[
            ("my.paper.md", "# Paper\n"),
            ("writer.toml", "[pdf]\nsystem_fonts = false\n"),
        ]);
        let opts = BuildOptions::new(dir.path().join("my.paper.md"));
        let prepared = prepare(&opts).unwrap();
        assert_eq!(
            output_path(&prepared, &opts, Target::Html, 1),
            absolute(dir.path()).unwrap().join("dist/my.paper.html")
        );
        let report = build_targets(
            &opts,
            &[Target::Html, Target::Html, Target::Pdf],
            &mut WorldSlot::new(),
        );
        assert_eq!(report.targets.len(), 2);
        assert!(report.targets[0].written.is_ok());
        assert_eq!(
            report.targets[1].written.as_ref().unwrap(),
            &[absolute(dir.path()).unwrap().join("dist/my.paper.pdf")]
        );
    }

    #[test]
    fn missing_assets_are_errors_only_for_build_targets_in_build_mode() {
        let dir = project(&[(
            "document.md",
            "# One\n\n![x](figures/none.png)\n\n![y](second.png)\n",
        )]);
        let mut opts = BuildOptions::new(dir.path().join("document.md"));
        let parsed = prepare(&opts).unwrap();
        assert_eq!(parsed.diagnostics.len(), 2);
        assert!(parsed.diagnostics.iter().all(|d| d.level == Level::Warning));
        let report =
            build_targets(&opts, &[Target::Html], &mut WorldSlot::new());
        let failure = report.prepared.unwrap_err();
        let Error::MissingAsset { path, span } = *failure.error else {
            panic!("missing asset was not escalated");
        };
        assert!(path.ends_with("figures/none.png"));
        assert_eq!(span.unwrap().start_line, 3);
        assert_eq!(failure.diagnostics.len(), 1);
        assert_eq!(
            failure.diagnostics.iter().next().unwrap().level,
            Level::Error
        );
        assert!(report.targets.is_empty());
        assert!(report.diagnostics.is_empty());
        opts.mode = Mode::Check;
        assert!(prepare(&opts).is_ok());
    }

    #[test]
    fn theme_origin_follows_its_winning_setting() {
        let dir = project(&[
            ("writer.toml", "[html]\ntheme = 'theme'\n"),
            ("theme/theme.toml", "name = 'project'\nextends = 'default'\n"),
            (
                "docs/theme/theme.toml",
                "name = 'document'\nextends = 'default'\n",
            ),
            ("docs/plain.md", "# Plain\n"),
            ("docs/front.md", "---\nhtml:\n  theme: theme\n---\n# Front\n"),
        ]);
        let plain_opts = BuildOptions::new(dir.path().join("docs/plain.md"));
        let plain = prepare(&plain_opts).unwrap();
        assert_eq!(
            resolve_theme(&plain, &plain_opts).unwrap().name(),
            "project"
        );
        let mut front_opts =
            BuildOptions::new(dir.path().join("docs/front.md"));
        let front = prepare(&front_opts).unwrap();
        assert_eq!(
            resolve_theme(&front, &front_opts).unwrap().name(),
            "document"
        );
        front_opts.cli.theme =
            Some(dir.path().join("theme").to_string_lossy().into_owned());
        let cli = prepare(&front_opts).unwrap();
        assert_eq!(resolve_theme(&cli, &front_opts).unwrap().name(), "project");
    }
    #[test]
    fn same_file_sees_through_links_and_spellings() {
        let dir = project(&[("deck.md", "# One\n"), ("other.md", "# Two\n")]);
        let deck = dir.path().join("deck.md");
        assert!(same_file(&deck, &deck));
        assert!(same_file(&deck, &dir.path().join("./deck.md")));
        assert!(!same_file(&deck, &dir.path().join("other.md")));
        assert!(!same_file(&deck, &dir.path().join("missing.md")));
        assert!(!same_file(
            &dir.path().join("missing.md"),
            &dir.path().join("missing.md")
        ));
        #[cfg(unix)]
        {
            let alias = dir.path().join("alias.html");
            std::os::unix::fs::symlink("deck.md", &alias).unwrap();
            assert!(same_file(&deck, &alias), "symbolic link");
            let hard = dir.path().join("hard.html");
            std::fs::hard_link(&deck, &hard).unwrap();
            assert!(same_file(&deck, &hard), "hard link");
        }
        // Another spelling of the name, where the file system ignores case.
        let upper = dir.path().join("DECK.MD");
        if upper.exists() {
            assert!(same_file(&deck, &upper), "case variant");
        }
    }

    #[test]
    fn find_clash_names_the_output_and_the_input() {
        let dir = project(&[("deck.md", "# One\n"), ("writer.toml", "")]);
        let deck = dir.path().join("deck.md");
        let config = dir.path().join("writer.toml");
        let inputs = [
            (deck.clone(), InputRole::Source),
            (config.clone(), InputRole::Config),
            (dir.path().join("gone.png"), InputRole::Asset),
        ];
        let out = dir.path().join("out.html");
        assert_eq!(
            find_clash([out.as_path()], &inputs),
            None,
            "does not exist"
        );
        std::fs::write(&out, "x").unwrap();
        assert_eq!(find_clash([out.as_path()], &inputs), None, "another file");
        let clash =
            find_clash([out.as_path(), config.as_path()], &inputs).unwrap();
        assert_eq!(
            clash,
            Clash {
                output: config.clone(),
                input: config.clone(),
                role: InputRole::Config
            }
        );
        assert_eq!(
            clash.to_string(),
            format!(
                "refusing to write {}: it is the config file {}",
                config.display(),
                config.display()
            )
        );
        let clash = find_clash([deck.as_path()], &inputs).unwrap();
        assert_eq!(clash.role, InputRole::Source);
        assert!(clash.to_string().contains(": it is the input file "));
        assert_eq!(InputRole::Asset.to_string(), "the document's asset");
        assert_eq!(find_clash([deck.as_path()], &[]), None);
        // Through directories that do not exist yet: write_output creates `sub`, and
        // `sub/..` is the deck's directory again.
        let around = dir.path().join("sub/new/../../deck.md");
        let clash = find_clash([around.as_path()], &inputs).unwrap();
        assert_eq!((clash.output, clash.role), (around, InputRole::Source));
        let elsewhere = dir.path().join("sub/../other/deck.md");
        assert_eq!(find_clash([elsewhere.as_path()], &inputs), None);
        assert!(!dir.path().join("sub").exists(), "nothing is created");
    }

    #[test]
    fn paths_are_resolved_as_a_write_after_creating_directories_would() {
        let dir = project(&[("a/x.md", "x")]);
        let base = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(
            path_after_creating_dirs(&dir.path().join("a/new/../x.md")),
            Some(base.join("a/x.md"))
        );
        assert_eq!(
            path_after_creating_dirs(&dir.path().join("n1/n2/../../a/x.md")),
            Some(base.join("a/x.md"))
        );
        assert_eq!(
            path_after_creating_dirs(&dir.path().join("./a/./n/m/../../x.md")),
            Some(base.join("a/x.md"))
        );
        assert_eq!(
            path_after_creating_dirs(&dir.path().join("n/../n/x.md")),
            Some(base.join("n/x.md")),
            "a directory created on the way is new and empty"
        );
        #[cfg(unix)]
        {
            // An existing link in the prefix is followed, as the OS follows it.
            std::os::unix::fs::symlink(base.join("a"), base.join("link"))
                .unwrap();
            assert_eq!(
                path_after_creating_dirs(&dir.path().join("link/new/../x.md")),
                Some(base.join("a/x.md"))
            );
            // A link reached after a `..` out of a new directory is followed too, and a
            // `..` after it leaves the directory it points to (review of A1: the lexical
            // rest went to `<dir>/x.md`, while the OS wrote `a/../x.md`).
            std::fs::create_dir_all(base.join("other/sub")).unwrap();
            std::os::unix::fs::symlink(
                base.join("other/sub"),
                base.join("a/up"),
            )
            .unwrap();
            assert_eq!(
                path_after_creating_dirs(
                    &dir.path().join("a/new/../up/../deck.md")
                ),
                Some(base.join("other/deck.md"))
            );
            assert_eq!(
                path_after_creating_dirs(
                    &dir.path().join("link/up/../../x.md")
                ),
                Some(base.join("x.md")),
                "link/up is other/sub: .. twice is the temporary directory"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn find_clash_follows_a_link_after_a_dot_dot_out_of_a_new_directory() {
        // work/link -> ../other/sub; `nonexist/../link/../deck.md` from work/ is
        // other/deck.md, the input.
        let dir =
            project(&[("other/deck.md", "# Precious\n"), ("work/x.md", "")]);
        std::fs::create_dir(dir.path().join("other/sub")).unwrap();
        std::os::unix::fs::symlink(
            "../other/sub",
            dir.path().join("work/link"),
        )
        .unwrap();
        let deck = dir.path().join("other/deck.md");
        let inputs = [(deck.clone(), InputRole::Source)];
        let out = dir.path().join("work/nonexist/../link/../deck.md");
        let clash = find_clash([out.as_path()], &inputs).unwrap();
        assert_eq!((clash.output, clash.input), (out, deck));
        let elsewhere = dir.path().join("work/nonexist/../deck.md");
        assert_eq!(find_clash([elsewhere.as_path()], &inputs), None);
    }

    #[test]
    fn missing_asset_error_falls_back_to_the_diagnostic_path() {
        let mut diagnostics = Diagnostics::new();
        diagnostics.push(Diagnostic::warning("empty-slide", "x"));
        assert!(missing_asset_error(&diagnostics).is_none());
        diagnostics.push(
            Diagnostic::warning("missing-asset", "no message format")
                .with_path("deck.md")
                .with_span(SourceSpan {
                    start_line: 4,
                    end_line: 4,
                    cell: None,
                    file: None,
                }),
        );
        match missing_asset_error(&diagnostics) {
            Some(Error::MissingAsset { path, span }) => {
                assert_eq!(path, PathBuf::from("deck.md"));
                assert_eq!(span.unwrap().start_line, 4);
            }
            other => panic!("expected MissingAsset, got {other:?}"),
        }
    }

    #[test]
    fn html_never_overwrites_source_assets_or_theme_reads() {
        let dir = project(&[
            ("document.md", "# Heading\n\n![plot](plot.svg)\n"),
            ("plot.svg", "<svg xmlns='http://www.w3.org/2000/svg'/>"),
            (
                "theme/theme.toml",
                "name = 'local'\nextends = 'default'\n[styles]\nentries = ['styles/main.css']\n",
            ),
            (
                "theme/styles/main.css",
                ".plot { background: url('../img/x.svg'); }\n",
            ),
            ("theme/img/x.svg", "<svg xmlns='http://www.w3.org/2000/svg'/>"),
            ("theme/templates/partials/local.html", "local template"),
            ("theme/dist/notes.txt", "earlier output"),
        ]);
        let mut opts = BuildOptions::new(dir.path().join("document.md"));
        opts.cli.theme =
            Some(dir.path().join("theme").to_string_lossy().into_owned());
        let mut prepared = prepare(&opts).unwrap();
        prepared.config.pdf.system_fonts = false;
        let mut worlds = WorldSlot::new();
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        for target in [
            "document.md",
            "plot.svg",
            "theme/theme.toml",
            "theme/styles/main.css",
            "theme/templates/partials/local.html",
            "theme/img/x.svg",
        ] {
            let out = dir.path().join(target);
            let before = std::fs::read(&out).unwrap();
            let error = html::build(&prepared, &opts, &out, world).unwrap_err();
            let clash = refused_clash(&error).unwrap();
            assert_eq!(clash.output, out);
            assert_eq!(std::fs::read(&out).unwrap(), before);
        }
        assert!(refused_clash(&Error::Other("plain".into())).is_none());
        let plain_io = Error::Io(std::io::Error::other("disk full"));
        assert!(refused_clash(&plain_io).is_none());
        let out = dir.path().join("theme/dist/notes.txt");
        assert!(html::build(&prepared, &opts, &out, world).is_ok());
    }

    #[test]
    fn html_site_index_is_protected_even_with_a_format_override() {
        let dir = project(&[("index.html", "# Precious\n")]);
        let mut opts = BuildOptions::new(dir.path().join("index.html"));
        opts.format = Some(SourceKind::Markdown);
        opts.cli.standalone = Some(false);
        let mut prepared = prepare(&opts).unwrap();
        prepared.config.pdf.system_fonts = false;
        let mut worlds = WorldSlot::new();
        let world = worlds.get(Some(&prepared), &opts).unwrap();
        let error =
            html::build(&prepared, &opts, dir.path(), world).unwrap_err();
        assert_eq!(refused_clash(&error).unwrap().role, InputRole::Source);
        assert_eq!(
            std::fs::read_to_string(&opts.input).unwrap(),
            "# Precious\n"
        );
    }

    #[test]
    fn directory_spellings_are_rejected_for_file_outputs() {
        let dir = tempfile::tempdir().unwrap();
        assert!(names_directory(dir.path()));
        assert!(names_directory(&dir.path().join("new/")));
        assert!(names_directory(&dir.path().join("new/.")));
        assert!(names_directory(&dir.path().join("new/..")));
        assert!(!names_directory(&dir.path().join("new.html")));
    }
}
