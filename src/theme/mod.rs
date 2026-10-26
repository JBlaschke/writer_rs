//! Themes: discovery, manifest, inheritance chain, template environment,
//! styles.
//!
//! Contract: `docs/spec/html-output.md` §3. Implemented by T15.
//!
//! A [`Theme`] is the resolved inheritance chain, child first and root last.
//! [`ThemeDir`] reads embedded themes or fresh disk files through the same
//! exact-case interface. Template and asset lookups prefer the child; style and
//! script entries concatenate parent-first; variables deep-merge, and code
//! themes override per key.
//!
//! A disk lookup never reaches outside its theme: lexical escapes and symbolic
//! links leaving the canonical directory count as missing. [`recording_reads`]
//! collects the disk files read while rendering so callers can watch them and
//! avoid overwriting them.

pub mod manifest;
pub mod resolve;
pub mod templates;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde_json::{Map, Value};

use crate::error::{Diagnostic, Error, Result};
use crate::ir::SourceSpan;

pub use manifest::{ScriptsSection, StylesSection, ThemeManifest};
pub use resolve::{builtin_names, resolve};
use templates::TemplateFile;

/// The codes of the diagnostics [`Theme::problems`] returns.
pub mod codes {
    /// A `[styles] entries` file that is missing or outside its theme (error).
    pub const THEME_STYLE_ENTRY: &str = "theme-style-entry";
    /// A `[scripts] entries` file that is missing or outside its theme (error).
    pub const THEME_SCRIPT_ENTRY: &str = "theme-script-entry";
    /// An invalid built-in code theme or theme-relative `.tmTheme` file
    /// (error).
    pub const THEME_CODE_THEME: &str = "theme-code-theme";
    /// A broken template (warning when the document does not load it).
    pub const THEME_TEMPLATE: &str = "theme-template";
    /// A symbolic link whose target is outside its theme, which lookups ignore
    /// (warning).
    pub const THEME_LINK_OUTSIDE: &str = "theme-link-outside";
}

/// Templates (paths under `templates/`) the root theme of every chain must
/// provide.
pub const REQUIRED_TEMPLATES: [&str; 1] = ["document.html"];

/// A code highlighting theme and its name or qualified file path.
#[derive(Debug, Clone)]
pub struct CodeTheme {
    /// Built-in name or `@<theme>/<normalized path>`.
    pub label: String,
    /// The loaded colours used to generate code CSS.
    pub theme: syntect::highlighting::Theme,
}

/// Where a theme's files live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeSource {
    /// A built-in theme in the embedded asset tree; `prefix` is its directory
    /// relative to `assets/`, with a trailing slash (`"themes/default/"`).
    Embedded {
        /// Path prefix of the theme's files in [`crate::vendor`].
        prefix: String,
    },
    /// A theme directory on disk (canonicalized by [`resolve()`]).
    Disk {
        /// The directory containing `theme.toml`.
        dir: PathBuf,
    },
}

/// One theme of a chain: its name, where its files are and its manifest.
#[derive(Debug, Clone)]
pub struct ThemeDir {
    /// The manifest's `name`.
    pub name: String,
    /// Where the files are read from.
    pub source: ThemeSource,
    /// The parsed `theme.toml`.
    pub manifest: ThemeManifest,
}

/// A file read from one theme of the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeFile {
    /// Name of the theme the file came from.
    pub theme: String,
    /// Path relative to that theme's directory, as written in `theme.toml`
    /// (e.g. `styles/theme.css`).
    pub rel_path: String,
    /// File contents.
    pub bytes: Vec<u8>,
}

/// A template of a disk theme that does not compile
/// ([`Theme::broken_templates`]).
#[derive(Debug)]
pub(crate) struct BrokenTemplate<'t> {
    /// The theme it is in.
    pub(crate) dir: &'t ThemeDir,
    /// Its path below `templates/`.
    pub(crate) rel: String,
    /// Its file.
    pub(crate) path: PathBuf,
    /// Why it does not compile: minijinja's error kind and detail.
    pub(crate) message: String,
    /// The line minijinja names, if any.
    pub(crate) line: Option<usize>,
}

impl BrokenTemplate<'_> {
    /// `diagnostic` with this template's path and line.
    pub(crate) fn locate(&self, diagnostic: Diagnostic) -> Diagnostic {
        let diagnostic = diagnostic.with_path(&self.path);
        match self.line {
            Some(line) => diagnostic.with_span(line_span(line)),
            None => diagnostic,
        }
    }
}

/// A resolved theme: the inheritance chain, child first, root last.
#[derive(Debug, Clone)]
pub struct Theme {
    /// The chain, ordered child → … → root. [`resolve()`] never returns an
    /// empty chain.
    pub chain: Vec<ThemeDir>,
}

impl ThemeDir {
    /// Reads the file at `rel` (relative to the theme directory,
    /// `/`-separated).
    ///
    /// `rel` is normalized lexically (`.` and empty segments dropped, `..`
    /// removes the previous segment); a path that is empty or climbs out of the
    /// theme directory yields `Ok(None)`, as does a missing file, a directory
    /// and a symbolic link (or a path through one) whose target is outside the
    /// theme directory. The file name must match exactly, including case (see
    /// the module docs). Disk themes are read from disk on every call. Fails
    /// with [`Error::Theme`] only when an existing file cannot be read.
    pub fn read(&self, rel: &str) -> Result<Option<Vec<u8>>> {
        let Some(rel) = normalize_rel(rel).filter(|r| !r.is_empty()) else {
            return Ok(None);
        };
        match &self.source {
            ThemeSource::Embedded { prefix } => {
                Ok(crate::vendor::get(&format!("{prefix}{rel}"))
                    .map(|data| data.into_owned()))
            }
            ThemeSource::Disk { dir } => {
                let Some(path) = disk_file(dir, &rel) else {
                    return Ok(None);
                };
                note_read(&path);
                std::fs::read(&path).map(Some).map_err(|e| Error::Theme {
                    name: self.name.clone(),
                    message: format!("cannot read {}: {e}", path.display()),
                })
            }
        }
    }

    /// True if the file at `rel` exists (same path rules as
    /// [`ThemeDir::read`]).
    pub fn exists(&self, rel: &str) -> bool {
        let Some(rel) = normalize_rel(rel).filter(|r| !r.is_empty()) else {
            return false;
        };
        match &self.source {
            ThemeSource::Embedded { prefix } => {
                crate::vendor::get(&format!("{prefix}{rel}")).is_some()
            }
            ThemeSource::Disk { dir } => disk_file(dir, &rel).is_some(),
        }
    }

    /// Every file below the directory `rel_dir` (recursively; `""` for the
    /// whole theme), as `/`-separated paths relative to the theme directory,
    /// sorted. Every listed path is accepted by [`ThemeDir::read`]. A missing
    /// directory lists nothing.
    pub fn list(&self, rel_dir: &str) -> Vec<String> {
        let Some(rel_dir) = normalize_rel(rel_dir) else {
            return Vec::new();
        };
        let mut files = match &self.source {
            ThemeSource::Embedded { prefix } => {
                let start = if rel_dir.is_empty() {
                    prefix.clone()
                } else {
                    format!("{prefix}{rel_dir}/")
                };
                crate::vendor::list(&start)
                    .into_iter()
                    .filter_map(|path| {
                        path.strip_prefix(prefix.as_str()).map(str::to_string)
                    })
                    .collect()
            }
            ThemeSource::Disk { dir } => list_disk(dir, &rel_dir),
        };
        files.sort();
        files
    }

    /// True if `rel` names an existing file of this disk theme that
    /// [`ThemeDir::read`] refuses only because a symbolic link takes it out of
    /// the theme directory (module docs), so that messages can say why the file
    /// counts as missing.
    pub fn linked_outside(&self, rel: &str) -> bool {
        let Some(rel) = normalize_rel(rel).filter(|r| !r.is_empty()) else {
            return false;
        };
        match &self.source {
            ThemeSource::Disk { dir } => linked_outside(dir, &rel),
            ThemeSource::Embedded { .. } => false,
        }
    }

    /// Why `entry`, a path this theme's `theme.toml` names
    /// (`[styles]`/`[scripts]` entries, a `.tmTheme` code theme), is not a file
    /// of this theme, as a clause: `must be a relative path`, `must stay inside
    /// the theme directory`, `does not name a file`, `is not found`, or that a
    /// symbolic link takes it out of the theme. `None` when it is one. The rule
    /// of [`Theme::problems`] (`writer check`) and `writer theme check`.
    pub(crate) fn entry_problem(&self, entry: &str) -> Option<&'static str> {
        match entry_path_problem(entry) {
            Some(problem) => Some(problem),
            None if self.exists(entry) => None,
            None => Some(self.missing_reason(entry, "is not found")),
        }
    }

    /// Why the file at `rel` cannot be read: `missing`, or that a symbolic link
    /// takes it out of the theme ([`ThemeDir::linked_outside`]).
    fn missing_reason(&self, rel: &str, missing: &'static str) -> &'static str {
        if self.linked_outside(rel) {
            LINKED_OUTSIDE
        } else {
            missing
        }
    }

    /// Where the theme lives, for messages: the directory, or `built-in
    /// (themes/<name>/)`.
    pub fn location(&self) -> String {
        match &self.source {
            ThemeSource::Embedded { prefix } => format!("built-in ({prefix})"),
            ThemeSource::Disk { dir } => dir.display().to_string(),
        }
    }
}

/// `dir/rel` (`rel` normalized and non-empty) if it is a file whose name
/// matches exactly and that is inside `dir` once symbolic links are resolved
/// ([`inside`]). A file that is there but, through a symbolic link, outside
/// `dir` is refused with a logged warning ([`warn_linked_outside`]), so that a
/// build that then fails on a missing file says why.
fn disk_file(dir: &Path, rel: &str) -> Option<PathBuf> {
    let path = dir.join(rel);
    if !(path.is_file() && exact_names(dir, rel)) {
        return None;
    }
    if inside(dir, &path) {
        return Some(path);
    }
    warn_linked_outside(dir, rel);
    None
}

/// What a file reached through a symbolic link out of its theme is, after its
/// path in messages (html-output.md §3, audit F08).
const LINKED_OUTSIDE: &str = "is a symbolic link (or below one) to a file outside the theme \
                              directory, which writer ignores: theme files must stay \
                              inside the theme";

thread_local! {
    /// The files of disk themes read on this thread while [`recording_reads`]
    /// runs (`None`: nothing is recording).
    static READS: RefCell<Option<BTreeSet<PathBuf>>> = const { RefCell::new(None) };
}

/// Runs `f` and returns with its result every file of a disk theme that it read
/// on this thread ([`ThemeDir::read`]), as paths in their theme directory. A
/// recording inside another one adds its files to the outer one.
pub fn recording_reads<R>(f: impl FnOnce() -> R) -> (R, BTreeSet<PathBuf>) {
    /// Puts back the outer recording, also when `f` panics.
    struct Restore(Option<Option<BTreeSet<PathBuf>>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(outer) = self.0.take() {
                READS.set(outer);
            }
        }
    }
    let mut restore = Restore(Some(READS.replace(Some(BTreeSet::new()))));
    let result = f();
    let read = READS.take().unwrap_or_default();
    if let Some(Some(outer)) = restore.0.as_mut() {
        outer.extend(read.iter().cloned());
    }
    drop(restore);
    (result, read)
}

/// Notes that `path`, a file of a disk theme, was read ([`recording_reads`]).
/// The caller must already have checked the theme's containment rules.
pub(crate) fn note_read(path: &Path) {
    READS.with_borrow_mut(|reads| {
        if let Some(reads) = reads {
            reads.insert(path.to_path_buf());
        }
    });
}

/// The symbolic links already reported as leading out of their theme, by
/// [`warn_linked_outside`] or as `theme-link-outside` findings
/// ([`link_problems`]).
static REPORTED_LINKS: LazyLock<Mutex<BTreeSet<PathBuf>>> =
    LazyLock::new(Mutex::default);

/// True the first time `link` is passed (in this process): it has not been
/// reported yet.
fn first_report(link: &Path) -> bool {
    // A poisoned lock only costs the deduplication.
    REPORTED_LINKS
        .lock()
        .map_or(true, |mut reported| reported.insert(link.to_path_buf()))
}

/// Logs, once per link and process, the symbolic link that takes the file
/// `dir/rel` out of the theme directory `dir`, which [`disk_file`] therefore
/// refuses: so that a build that fails on the "missing" file says why.
fn warn_linked_outside(dir: &Path, rel: &str) {
    if let Some((link, target)) = outside_link(dir, rel)
        && first_report(&link)
    {
        tracing::warn!(
            "{}",
            link_message(&link.display().to_string(), &target)
        );
    }
}

/// The symbolic link that takes `dir/rel` out of the directory `dir`: the first
/// of its ancestors below `dir`, or itself, that is a link whose target is
/// outside `dir`, with the canonical target.
fn outside_link(dir: &Path, rel: &str) -> Option<(PathBuf, PathBuf)> {
    let root = std::fs::canonicalize(dir).ok()?;
    let mut path = dir.to_path_buf();
    for segment in rel.split('/') {
        path.push(segment);
        if path.symlink_metadata().is_ok_and(|meta| meta.is_symlink()) {
            let target = std::fs::canonicalize(&path).ok()?;
            if !target.starts_with(&root) {
                return Some((path, target));
            }
        }
    }
    None
}

/// `<link> is a symbolic link to <target>, outside the theme directory: …`, for
/// a log line and a `theme-link-outside` finding.
fn link_message(link: &str, target: &Path) -> String {
    let below = if target.is_dir() {
        " and everything below it"
    } else {
        ""
    };
    format!(
        "{link} is a symbolic link to {}, outside the theme directory: writer ignores \
         it{below}, so it counts as missing (theme files must stay inside the theme; copy the \
         file into it, or share it through `extends`)",
        target.display()
    )
}

/// True if the existing file `dir/rel` (exact name) is refused only because a
/// symbolic link takes it out of `dir` ([`disk_file`]).
fn linked_outside(dir: &Path, rel: &str) -> bool {
    let path = dir.join(rel);
    path.is_file() && exact_names(dir, rel) && !inside(dir, &path)
}

/// True if the existing `path` is below the directory `dir` when both are
/// canonicalized (symbolic links resolved). False when either cannot be
/// canonicalized.
fn inside(dir: &Path, path: &Path) -> bool {
    match (std::fs::canonicalize(dir), std::fs::canonicalize(path)) {
        (Ok(dir), Ok(path)) => path.starts_with(&dir),
        _ => false,
    }
}

/// True if every segment of the normalized `rel` is the exact name (case
/// included) of an entry of its parent directory below `dir`. A plain
/// `is_file`/`is_dir` would accept `layouts/TITLE.html` for
/// `layouts/title.html` on a case-insensitive file system, which the embedded
/// tree (exact keys) and a case-sensitive file system (Linux CI) reject.
fn exact_names(dir: &Path, rel: &str) -> bool {
    let mut parent = dir.to_path_buf();
    for segment in rel.split('/').filter(|s| !s.is_empty()) {
        #[cfg(test)]
        LISTINGS.set(LISTINGS.get() + 1);
        let Ok(entries) = std::fs::read_dir(&parent) else {
            return false;
        };
        if !entries
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.file_name() == segment)
        {
            return false;
        }
        parent.push(segment);
    }
    true
}

#[cfg(test)]
thread_local! {
    /// How many directories [`exact_names`] listed on this thread (tests count
    /// them).
    static LISTINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many directories the checks of theme paths listed on this thread so far.
#[cfg(test)]
pub(crate) fn directory_listings() -> usize {
    LISTINGS.get()
}

/// Files below `dir/rel_dir` on disk, relative to `dir`, following symlinks
/// that stay inside `dir` (the files [`disk_file`] accepts). A directory a link
/// takes out of `dir` is not descended into, so a link to `/usr` costs nothing.
fn list_disk(dir: &Path, rel_dir: &str) -> Vec<String> {
    let start = dir.join(rel_dir);
    if !start.is_dir() || !exact_names(dir, rel_dir) {
        return Vec::new();
    }
    let Ok(root) = std::fs::canonicalize(dir) else {
        return Vec::new();
    };
    walkdir::WalkDir::new(&start)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| {
            std::fs::canonicalize(entry.path())
                .is_ok_and(|path| path.starts_with(&root))
        })
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let rel = entry.path().strip_prefix(dir).ok()?;
            let parts: Option<Vec<&str>> =
                rel.components().map(|c| c.as_os_str().to_str()).collect();
            Some(parts?.join("/"))
        })
        .collect()
}

/// Normalizes a theme-relative path: `/` and `\` separate segments, empty and
/// `.` segments are dropped, `..` removes the previous segment. `None` if the
/// path climbs out of the theme directory; `Some("")` for the theme directory
/// itself.
pub(crate) fn normalize_rel(rel: &str) -> Option<String> {
    let mut segments: Vec<&str> = Vec::new();
    for segment in rel.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            name => segments.push(name),
        }
    }
    Some(segments.join("/"))
}

impl Theme {
    /// The child theme's name (the theme that was requested).
    pub fn name(&self) -> &str {
        self.chain.first().map_or("", |dir| dir.name.as_str())
    }

    /// The theme of the chain called `name`, if any.
    pub fn dir(&self, name: &str) -> Option<&ThemeDir> {
        self.chain.iter().find(|dir| dir.name == name)
    }

    /// The chain's `[vars]` tables deep-merged root → child (the child wins).
    pub fn vars(&self) -> Map<String, Value> {
        self.merged(|manifest| &manifest.vars)
    }

    fn merged(
        &self, table: impl Fn(&ThemeManifest) -> &Map<String, Value>,
    ) -> Map<String, Value> {
        let mut merged = Map::new();
        for dir in self.chain.iter().rev() {
            deep_merge(&mut merged, table(&dir.manifest));
        }
        merged
    }

    /// The `[styles] entries` of the whole chain, parent-first (root's entries
    /// first, in manifest order), each read from the theme that lists it. An
    /// entry must be a relative path that stays inside its theme's directory
    /// (so `serve` never exposes files outside a theme); such an entry, or a
    /// missing file, is an [`Error::Theme`].
    pub fn style_entries(&self) -> Result<Vec<ThemeFile>> {
        self.entries("style", |manifest| &manifest.styles.entries)
    }

    /// The `[scripts] entries` of the whole chain, parent-first, like
    /// [`Theme::style_entries`].
    pub fn script_entries(&self) -> Result<Vec<ThemeFile>> {
        self.entries("script", |manifest| &manifest.scripts.entries)
    }

    fn entries(
        &self, kind: &str, list: impl Fn(&ThemeManifest) -> &Vec<String>,
    ) -> Result<Vec<ThemeFile>> {
        let mut files = Vec::new();
        for dir in self.chain.iter().rev() {
            for entry in list(&dir.manifest) {
                let found = match entry_path_problem(entry) {
                    Some(problem) => Err(problem),
                    None => dir
                        .read(entry)?
                        .ok_or_else(|| dir.missing_reason(entry, "not found")),
                };
                let bytes = found.map_err(|problem| Error::Theme {
                    name: dir.name.clone(),
                    message: format!(
                        "{kind} entry '{entry}' {problem} ({}; entries are relative to the theme directory)",
                        dir.location()
                    ),
                })?;
                files.push(ThemeFile {
                    theme: dir.name.clone(),
                    rel_path: entry.clone(),
                    bytes,
                });
            }
        }
        Ok(files)
    }

    /// The first light and dark code themes set while walking child → root.
    /// Missing settings and `"none"` disable that code theme.
    pub fn code_themes(
        &self,
    ) -> Result<(Option<CodeTheme>, Option<CodeTheme>)> {
        Ok((
            self.code_theme("code_theme")?,
            self.code_theme("code_theme_dark")?,
        ))
    }

    fn code_theme_setting(&self, key: &str) -> Option<(&ThemeDir, &str)> {
        self.chain.iter().find_map(|dir| {
            let styles = &dir.manifest.styles;
            let value = match key {
                "code_theme_dark" => styles.code_theme_dark.as_deref(),
                _ => styles.code_theme.as_deref(),
            }?;
            Some((dir, value))
        })
    }

    fn code_theme(&self, key: &str) -> Result<Option<CodeTheme>> {
        let Some((dir, value)) = self.code_theme_setting(key) else {
            return Ok(None);
        };
        load_code_theme(dir, key, value)
    }

    /// Problems a document build would encounter, located in the theme's files.
    /// Broken templates outside the statically reached include chain and
    /// symbolic links leaving a theme are warnings; invalid entries and code
    /// themes are errors.
    pub fn problems(&self) -> Vec<Diagnostic> {
        // First, so that lookups below do not log the links they report.
        let links: Vec<Diagnostic> =
            self.chain.iter().flat_map(link_problems).collect();
        let reached = templates::reached_templates(
            self,
            REQUIRED_TEMPLATES.iter().map(|name| name.to_string()),
        );
        let mut found = Vec::new();
        for dir in self.chain.iter().rev() {
            found.extend(entry_problems(dir));
        }
        for key in ["code_theme", "code_theme_dark"] {
            if let Some((dir, value)) = self.code_theme_setting(key)
                && let Err(err) = load_code_theme(dir, key, value)
            {
                found.push(manifest_problem(
                    dir,
                    codes::THEME_CODE_THEME,
                    err.to_string(),
                    |text| {
                        manifest::line_of_value(text, "styles", key, value)
                            .or_else(|| {
                                manifest::line_of_key(text, "styles", key)
                            })
                    },
                ));
            }
        }
        found.extend(self.template_problems(Some(&reached)));
        found.extend(links);
        found
    }

    /// `theme-template`: every template of the chain's disk themes that does
    /// not compile, an error when `reached` is `None` or has it, else a
    /// warning. Hidden files (`.gitkeep`) are skipped; built-in themes always
    /// compile.
    fn template_problems(
        &self, reached: Option<&BTreeSet<TemplateFile>>,
    ) -> Vec<Diagnostic> {
        let broken = match self.broken_templates() {
            Ok(broken) => broken,
            Err(err) => {
                return vec![Diagnostic::error(
                    codes::THEME_TEMPLATE,
                    err.to_string(),
                )];
            }
        };
        let mut found = Vec::new();
        for template in broken {
            let BrokenTemplate { dir, rel, .. } = &template;
            let message = format!(
                "theme '{}': template {rel}: {}",
                dir.name, template.message
            );
            let used = reached.is_none_or(|reached| {
                reached.contains(&(dir.name.clone(), rel.clone()))
            });
            let diagnostic = if used {
                Diagnostic::error(codes::THEME_TEMPLATE, message)
            } else {
                let message = format!("{message} (the build does not load it)");
                Diagnostic::warning(codes::THEME_TEMPLATE, message)
            };
            found.push(template.locate(diagnostic));
        }
        found
    }

    /// Every template of the chain's disk themes that does not compile, chain
    /// order (child first), each theme's in path order. Hidden files
    /// (`.gitkeep`) are skipped; built-in themes always compile. The check of
    /// [`Theme::problems`] (`writer check`) and `writer theme check`. Errors:
    /// the environment cannot be built.
    pub(crate) fn broken_templates(&self) -> Result<Vec<BrokenTemplate<'_>>> {
        let env = self.environment(false)?;
        let mut found = Vec::new();
        for dir in &self.chain {
            let ThemeSource::Disk { dir: root } = &dir.source else {
                continue;
            };
            for path in dir.list("templates") {
                let Some(rel) = path.strip_prefix("templates/") else {
                    continue;
                };
                if rel.split('/').any(|segment| segment.starts_with('.')) {
                    continue;
                }
                let Err(err) =
                    env.get_template(&format!("@{}/{rel}", dir.name))
                else {
                    continue;
                };
                let message = match err.detail() {
                    Some(detail) => format!("{}: {detail}", err.kind()),
                    None => err.kind().to_string(),
                };
                found.push(BrokenTemplate {
                    dir,
                    rel: rel.to_string(),
                    path: root.join(&path),
                    message,
                    line: err.line(),
                });
            }
        }
        Ok(found)
    }

    /// The template `name` as `(theme name, source)`: `templates/<name>` of the
    /// first theme (child → root) that has it. `@<theme>/<path>` reads
    /// `templates/<path>` of that theme only.
    ///
    /// The path is taken as written, like minijinja's own `path_loader`:
    /// `/`-separated segments, none of them empty or starting with `.`, no `\`.
    /// Any other name (`../theme.toml`, `layouts/../slide.html`, `./deck.html`)
    /// finds nothing, so a template can never be reached under a second name.
    /// File names match exactly, including case. `Ok(None)` if no theme has it;
    /// [`Error::Theme`] for an `@` name that does not name a theme of the
    /// chain, [`Error::Template`] for a file that is not UTF-8.
    pub fn read_template(
        &self, name: &str,
    ) -> Result<Option<(String, String)>> {
        let (dirs, path) = self.template_dirs(name)?;
        let Some(file) = template_file(path) else {
            return Ok(None);
        };
        for dir in dirs {
            if let Some(bytes) = dir.read(&file)? {
                let source = String::from_utf8(bytes).map_err(|e| Error::Template {
                    template: name.to_string(),
                    line: None,
                    message: format!(
                        "template '{name}' is not valid UTF-8 (theme '{}'): {e}",
                        dir.name
                    ),
                })?;
                return Ok(Some((dir.name.clone(), source)));
            }
        }
        Ok(None)
    }

    /// True if [`Theme::read_template`] would find `name` (without reading it).
    pub fn has_template(&self, name: &str) -> bool {
        self.template_origin(name).is_some()
    }

    /// Where [`Theme::read_template`] finds `name` (without reading it): the
    /// theme that has it and its path below `templates/`.
    pub(crate) fn template_origin(
        &self, name: &str,
    ) -> Option<(&ThemeDir, String)> {
        let (dirs, path) = self.template_dirs(name).ok()?;
        let file = template_file(path)?;
        let dir = dirs.into_iter().find(|dir| dir.exists(&file))?;
        Some((dir, path.to_string()))
    }

    /// The themes to search for template `name` and the path below
    /// `templates/`.
    fn template_dirs<'a, 'n>(
        &'a self, name: &'n str,
    ) -> Result<(Vec<&'a ThemeDir>, &'n str)> {
        let Some(qualified) = name.strip_prefix('@') else {
            return Ok((self.chain.iter().collect(), name));
        };
        let chain_names = || {
            let names: Vec<&str> =
                self.chain.iter().map(|d| d.name.as_str()).collect();
            names.join(" → ")
        };
        let Some((theme, path)) = qualified.split_once('/') else {
            return Err(Error::Theme {
                name: self.name().to_string(),
                message: format!(
                    "template name '{name}' must have the form '@<theme>/<path>'"
                ),
            });
        };
        let dir = self.dir(theme).ok_or_else(|| Error::Theme {
            name: self.name().to_string(),
            message: format!(
                "template '{name}': no theme named '{theme}' in the chain ({})",
                chain_names()
            ),
        })?;
        Ok((vec![dir], path))
    }

    /// `assets/<rel>` of the first theme (child → root) that has it. `rel` is
    /// normalized like in [`ThemeDir::read`]; a path that climbs out of
    /// `assets/` yields `Ok(None)`.
    pub fn read_asset(&self, rel: &str) -> Result<Option<Vec<u8>>> {
        let Some(rel) = normalize_rel(rel).filter(|r| !r.is_empty()) else {
            return Ok(None);
        };
        for dir in &self.chain {
            if let Some(bytes) = dir.read(&format!("assets/{rel}"))? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    /// The directories of the chain's disk themes, child first, to watch
    /// recursively. Built-in themes contribute nothing.
    pub fn watch_dirs(&self) -> Vec<PathBuf> {
        self.chain
            .iter()
            .filter_map(|theme_dir| match &theme_dir.source {
                ThemeSource::Disk { dir } => Some(dir.clone()),
                ThemeSource::Embedded { .. } => None,
            })
            .collect()
    }

    /// A minijinja environment whose loader searches this chain; see
    /// [`templates::environment`].
    pub fn environment(
        &self, debug: bool,
    ) -> Result<minijinja::Environment<'static>> {
        templates::environment(self, debug)
    }
}

/// Loads one light or dark code theme from the manifest that sets it.
fn load_code_theme(
    dir: &ThemeDir, key: &str, value: &str,
) -> Result<Option<CodeTheme>> {
    if value == "none" {
        return Ok(None);
    }
    let error = |problem: String| Error::Theme {
        name: dir.name.clone(),
        message: format!(
            "[styles] {key} '{value}': {problem} ({})",
            dir.location(),
        ),
    };
    if value.to_ascii_lowercase().ends_with(".tmtheme") {
        if let Some(problem) = entry_path_problem(value) {
            return Err(error(problem.into()));
        }
        let bytes = dir.read(value)?.ok_or_else(|| {
            error(dir.missing_reason(value, "is not found").into())
        })?;
        let theme = syntect::highlighting::ThemeSet::load_from_reader(
            &mut std::io::Cursor::new(bytes),
        )
        .map_err(|err| error(format!("cannot load code theme: {err}")))?;
        let rel = normalize_rel(value).ok_or_else(|| {
            error("must stay inside the theme directory".into())
        })?;
        return Ok(Some(CodeTheme {
            label: format!("@{}/{rel}", dir.name),
            theme,
        }));
    }
    if let Some(theme) = crate::html::highlight::builtin_code_theme(value) {
        return Ok(Some(CodeTheme {
            label: value.into(),
            theme: theme.clone(),
        }));
    }
    let problem = if matches!(value, "ansi" | "base16" | "base16-256") {
        "terminal-only theme; its colours are ANSI palette indices".into()
    } else {
        let candidates = two_face::theme::EmbeddedLazyThemeSet::theme_names()
            .iter()
            .map(|name| name.as_name())
            .filter(|name| !matches!(*name, "ansi" | "base16" | "base16-256"));
        let hint = closest_name(value, candidates)
            .map(|name| format!("; did you mean '{name}'?"))
            .unwrap_or_default();
        format!("unknown code theme{hint}")
    };
    Err(error(problem))
}

/// True if `segment` may appear in a template name: non-empty, not starting
/// with `.` (which also excludes `.` and `..`) and without `\`; the rule of
/// minijinja's `path_loader`.
fn is_name_segment(segment: &str) -> bool {
    !segment.is_empty() && !segment.starts_with('.') && !segment.contains('\\')
}

/// `templates/<path>` for a template path made of valid segments
/// ([`is_name_segment`]), else `None` (so `{% include "../theme.toml" %}` and
/// `layouts/../slide.html` find nothing).
fn template_file(path: &str) -> Option<String> {
    path.split('/').all(is_name_segment).then(|| format!("templates/{path}"))
}

/// Why a `[styles]`/`[scripts]` entry is not a usable path inside the theme
/// directory, if it is not: absolute, climbing out of the directory, or naming
/// the directory itself.
fn entry_path_problem(entry: &str) -> Option<&'static str> {
    if entry.starts_with(['/', '\\']) || Path::new(entry).has_root() {
        return Some("must be a relative path");
    }
    match normalize_rel(entry) {
        None => Some("must stay inside the theme directory"),
        Some(rel) if rel.is_empty() => Some("does not name a file"),
        Some(_) => None,
    }
}

/// `theme-style-entry` / `theme-script-entry` for every `[styles]`/`[scripts]`
/// entry of `dir` that is not a file inside its directory.
fn entry_problems(dir: &ThemeDir) -> Vec<Diagnostic> {
    let lists = [
        ("styles", codes::THEME_STYLE_ENTRY, &dir.manifest.styles.entries),
        ("scripts", codes::THEME_SCRIPT_ENTRY, &dir.manifest.scripts.entries),
    ];
    let mut found = Vec::new();
    for (table, code, entries) in lists {
        for entry in entries {
            let problem = dir.entry_problem(entry).or_else(|| {
                (table == "styles" && !entry.ends_with(".css")).then_some(
                    "is not a .css file; SCSS themes arrive with T79",
                )
            });
            let Some(problem) = problem else {
                continue;
            };
            let message = format!(
                "theme '{}': [{table}] entry '{entry}' {problem} (paths are relative to the \
                 theme directory)",
                dir.name
            );
            found.push(manifest_problem(dir, code, message, |text| {
                manifest::line_of_value(text, table, "entries", entry)
            }));
        }
    }
    found
}

/// `theme-link-outside` (warning) for every symbolic link in the disk theme
/// `dir` whose target is outside the theme directory: lookups ignore it, and
/// every file below it ([`disk_file`]), so it counts as missing (audit F08).
/// Each is marked as reported, so lookups do not log it again
/// ([`warn_linked_outside`]). Links are not followed, so the walk stays inside
/// the theme; dangling links are not reported.
fn link_problems(dir: &ThemeDir) -> Vec<Diagnostic> {
    let ThemeSource::Disk { dir: root } = &dir.source else {
        return Vec::new();
    };
    let Ok(canonical) = std::fs::canonicalize(root) else {
        return Vec::new();
    };
    walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path_is_symlink())
        .filter_map(|entry| {
            let target = std::fs::canonicalize(entry.path()).ok()?;
            if target.starts_with(&canonical) {
                return None;
            }
            let rel = entry.path().strip_prefix(root).ok()?;
            first_report(entry.path());
            let message = format!(
                "theme '{}': {}",
                dir.name,
                link_message(&rel.display().to_string(), &target)
            );
            Some(
                Diagnostic::warning(codes::THEME_LINK_OUTSIDE, message)
                    .with_path(entry.path()),
            )
        })
        .collect()
}

/// An error about `dir`'s manifest: with the path of a disk theme's
/// `theme.toml` and the line `line_of` finds in its text, when it finds one.
fn manifest_problem(
    dir: &ThemeDir, code: &'static str, message: String,
    line_of: impl FnOnce(&str) -> Option<usize>,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(code, message);
    let ThemeSource::Disk { dir: root } = &dir.source else {
        return diagnostic;
    };
    let path = root.join(resolve::MANIFEST_FILE);
    let line = dir
        .read(resolve::MANIFEST_FILE)
        .ok()
        .flatten()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| line_of(&text));
    let diagnostic = diagnostic.with_path(path);
    match line {
        Some(line) => diagnostic.with_span(line_span(line)),
        None => diagnostic,
    }
}

/// A one-line span (of a `theme.toml` or a template).
fn line_span(line: usize) -> SourceSpan {
    SourceSpan { start_line: line, end_line: line, cell: None, file: None }
}

/// The name among `candidates` closest to `word` (edit distance 1 or 2), for a
/// "did you mean" hint; the same rule as the config's unknown-key hints
/// ([`crate::config::unknown_key_hint`], which this asks). Candidates with a
/// backtick or a line break are skipped.
pub(crate) fn closest_name<'c>(
    word: &str, candidates: impl IntoIterator<Item = &'c str>,
) -> Option<String> {
    if word.contains('`') {
        return None;
    }
    let expected: Vec<String> = candidates
        .into_iter()
        .filter(|candidate| {
            !candidate.contains(['`', '\n']) && *candidate != word
        })
        .map(|candidate| format!("`{candidate}`"))
        .collect();
    if expected.is_empty() {
        return None;
    }
    let message =
        format!("unknown variant `{word}`, expected {}", expected.join(", "));
    crate::config::unknown_key_hint(&message)
}

/// Merges `over` into `base`: objects present on both sides merge recursively,
/// anything else in `over` replaces the value in `base`.
fn deep_merge(base: &mut Map<String, Value>, over: &Map<String, Value>) {
    for (key, value) in over {
        match (base.get_mut(key), value) {
            (Some(Value::Object(base_map)), Value::Object(over_map)) => {
                deep_merge(base_map, over_map)
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn object(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            other => panic!("not an object: {other}"),
        }
    }

    #[test]
    fn normalize_rel_stays_inside_the_theme() {
        assert_eq!(
            normalize_rel("styles/theme.css").as_deref(),
            Some("styles/theme.css")
        );
        assert_eq!(
            normalize_rel("./styles//./a\\b.css").as_deref(),
            Some("styles/a/b.css")
        );
        assert_eq!(
            normalize_rel("styles/../assets/x.png").as_deref(),
            Some("assets/x.png")
        );
        assert_eq!(
            normalize_rel("/templates/deck.html").as_deref(),
            Some("templates/deck.html")
        );
        assert_eq!(normalize_rel("").as_deref(), Some(""));
        assert_eq!(normalize_rel("a/..").as_deref(), Some(""));
        assert_eq!(normalize_rel(".."), None);
        assert_eq!(normalize_rel("assets/../../theme.toml"), None);
    }

    #[test]
    fn recordings_collect_the_disk_files_read_and_nest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        std::fs::create_dir_all(root.join("img")).unwrap();
        std::fs::write(root.join("a.css"), "a").unwrap();
        std::fs::write(root.join("img/x.png"), "x").unwrap();
        let dir = ThemeDir {
            name: "t".into(),
            source: ThemeSource::Disk { dir: root.clone() },
            manifest: ThemeManifest::default(),
        };
        // Not recording: nothing is kept. Exact-case checks list the directories.
        let before = directory_listings();
        dir.read("a.css").unwrap();
        assert!(directory_listings() > before);
        let ((), outer) = recording_reads(|| {
            dir.read("a.css").unwrap();
            assert_eq!(dir.read("missing.css").unwrap(), None);
            let ((), inner) = recording_reads(|| {
                dir.read("img/./x.png").map(|_| ()).unwrap()
            });
            assert_eq!(inner, BTreeSet::from([root.join("img/x.png")]));
            // A path that would leave the theme is never noted.
            assert_eq!(dir.read("../outside.png").unwrap(), None);
            assert_eq!(dir.read("img/..").unwrap(), None);
        });
        assert_eq!(
            outer,
            BTreeSet::from([root.join("a.css"), root.join("img/x.png")])
        );
        // A panic inside a recording ends it.
        let panicked =
            std::panic::catch_unwind(|| recording_reads(|| panic!("boom")));
        assert!(panicked.is_err());
        assert!(READS.with_borrow(Option::is_none));
        // Built-in themes have no files on disk.
        let builtin = resolve("default", &[]).unwrap();
        let ((), none) = recording_reads(|| {
            builtin.chain[0].read("theme.toml").unwrap();
        });
        assert!(none.is_empty());
    }

    #[test]
    fn deep_merge_lets_the_child_win_key_by_key() {
        let mut base =
            object(json!({"a": 1, "nested": {"x": 1, "y": 2}, "list": [1, 2]}));
        deep_merge(
            &mut base,
            &object(json!({"b": 2, "nested": {"y": 3, "z": 4}, "list": [3]})),
        );
        assert_eq!(
            Value::Object(base),
            json!({"a": 1, "b": 2, "nested": {"x": 1, "y": 3, "z": 4}, "list": [3]})
        );
        let mut base = object(json!({"a": {"x": 1}}));
        deep_merge(&mut base, &object(json!({"a": "flat"})));
        assert_eq!(Value::Object(base), json!({"a": "flat"}));
    }

    #[test]
    fn disk_theme_dir_reads_lists_and_refuses_to_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("t");
        std::fs::create_dir_all(root.join("templates/layouts")).unwrap();
        std::fs::write(root.join("theme.toml"), "name = \"t\"").unwrap();
        std::fs::write(root.join("templates/layouts/a.html"), "A").unwrap();
        std::fs::write(tmp.path().join("secret.txt"), "s").unwrap();
        let dir = ThemeDir {
            name: "t".into(),
            source: ThemeSource::Disk { dir: root.clone() },
            manifest: ThemeManifest::default(),
        };
        assert_eq!(
            dir.read("templates/layouts/a.html").unwrap(),
            Some(b"A".to_vec())
        );
        assert_eq!(dir.read("templates/missing.html").unwrap(), None);
        assert_eq!(
            dir.read("templates").unwrap(),
            None,
            "directories are not files"
        );
        assert_eq!(dir.read("../secret.txt").unwrap(), None);
        assert!(!dir.exists("../secret.txt"));
        assert!(dir.exists("./theme.toml"));
        assert_eq!(dir.list("templates"), ["templates/layouts/a.html"]);
        assert_eq!(dir.list(""), ["templates/layouts/a.html", "theme.toml"]);
        assert!(dir.list("nope").is_empty());
        assert!(dir.list("..").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn diagnostic_locations_refuse_a_manifest_linked_outside_after_resolve() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("t");
        std::fs::create_dir_all(root.join("templates")).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("templates/document.html"), "document")
            .unwrap();
        let manifest = "name = 't'\n[styles]\nentries = ['missing.css']\n";
        let path = root.join(resolve::MANIFEST_FILE);
        std::fs::write(&path, manifest).unwrap();
        let theme = resolve(root.to_str().unwrap(), &[]).unwrap();

        let outside = tmp.path().join("outside.toml");
        std::fs::write(&outside, manifest).unwrap();
        std::fs::remove_file(&path).unwrap();
        symlink(&outside, &path).unwrap();

        let (problems, reads) = recording_reads(|| theme.problems());
        let entry = problems
            .iter()
            .find(|problem| problem.code == codes::THEME_STYLE_ENTRY)
            .unwrap();
        assert_eq!(entry.path.as_deref(), Some(path.as_path()));
        assert!(entry.span.is_none(), "outside manifest cannot supply a line");
        assert!(!reads.contains(&outside));
        assert!(!reads.contains(&path));
        assert!(problems.iter().any(|p| p.code == codes::THEME_LINK_OUTSIDE));
    }
}
