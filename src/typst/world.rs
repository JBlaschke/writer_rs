//! Embedded Typst World, virtual files, and fonts (typst-dialect.md §6).
//! Owner: T03. Stub created by T00.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Dict, Duration, IntoValue};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Library, LibraryExt, World};
use typst_kit::fonts::FontStore;

use crate::assets::registry::{AssetData, AssetRegistry};
use crate::config::{Config, Toc};
use crate::error::{Diagnostics, Error, Result};

use super::package::PackageStore;
use super::sourcemap::SourceMap;

const VIRTUAL_MAIN: &str = "/__writer-main.typ";
const RENDER_MAIN: &str = "/__render/main.typ";

/// The input source used for document compilation.
#[derive(Debug, Clone)]
pub enum MainFile {
    /// A Typst file within the project root.
    Path(PathBuf),
    /// An in-memory source at `/__writer-main.typ`.
    Virtual,
}

/// Construction options for a long-lived compiler world.
pub struct WorldOptions {
    /// Directory containing the input document and project files.
    pub root: PathBuf,
    /// Main source location.
    pub main: MainFile,
    /// Fixed date and time used by the compilation session.
    pub today: Datetime,
    /// Search system fonts after embedded and configured fonts.
    pub system_fonts: bool,
    /// Additional font directories, searched in order.
    pub font_paths: Vec<PathBuf>,
    /// Additional Typst package roots, searched in order.
    pub package_paths: Vec<PathBuf>,
    /// Permit downloading packages from Typst Universe.
    pub allow_packages: bool,
    /// Strings exposed to Typst as `sys.inputs`.
    pub inputs: BTreeMap<String, String>,
}

/// Produces Typst fragments for Markdown or notebook includes.
pub trait IncludeProvider: Send + Sync {
    /// Synthesizes an absolute included path under the world's project root.
    fn synthesize(&self, path: &Path, root: &Path) -> Result<Synthesized>;
}

/// A synthesized include and the resources consumed while producing it.
pub struct Synthesized {
    /// Typst source for the fragment.
    pub text: String,
    /// Mapping from emitted lines to the original source.
    pub map: SourceMap,
    /// Assets referenced by the fragment.
    pub assets: AssetRegistry,
    /// Additional real files read by the provider.
    pub files: Vec<PathBuf>,
    /// Findings from parsing the included document.
    pub diagnostics: Diagnostics,
}

/// Retained sources, libraries, fonts, and resources for one compiler session.
///
/// Call [`reset`](Self::reset) before rebuilding. Disk sources are refreshed on
/// demand while their existing syntax trees are edited in place.
pub struct WriterWorld {
    root: PathBuf,
    main: FileId,
    html_mode: bool,
    paged_library: LazyHash<Library>,
    html_library: LazyHash<Library>,
    inputs: BTreeMap<String, String>,
    library_builds: usize,
    fonts: FontStore,
    today: Datetime,
    packages: PackageStore,
    allow_packages: bool,
    slots: Mutex<HashMap<FileId, Slot>>,
    assets: AssetRegistry,
    mounts: Vec<(String, PathBuf)>,
    render_files: HashMap<String, Bytes>,
    provider: Option<Box<dyn IncludeProvider>>,
    includes: Mutex<IncludeState>,
    accessed: Mutex<BTreeSet<PathBuf>>,
}

#[derive(Default)]
struct Slot {
    source: Option<Source>,
    source_result: Option<FileResult<Source>>,
    bytes: Option<FileResult<Bytes>>,
    map: Option<SourceMap>,
    real_path: Option<PathBuf>,
    retain: bool,
}

#[derive(Default)]
struct IncludeState {
    assets: AssetRegistry,
    diagnostics: Diagnostics,
}

struct Loaded {
    bytes: Bytes,
    real_path: Option<PathBuf>,
    retain: bool,
}

impl WriterWorld {
    /// Creates a world with embedded fonts and the requested resource roots.
    pub fn new(opts: WorldOptions) -> Result<Self> {
        let root = fs::canonicalize(&opts.root)?;
        if !root.is_dir() {
            return Err(Error::Config(format!(
                "Typst project root is not a directory: {}",
                root.display()
            )));
        }
        let main = main_id(&root, opts.main)?;
        let (paged_library, html_library) = libraries(&opts.inputs);
        let mut fonts = FontStore::new();
        fonts.extend(typst_kit::fonts::embedded());
        for dir in &opts.font_paths {
            fonts.extend(typst_kit::fonts::scan(dir));
        }
        if opts.system_fonts {
            fonts.extend(typst_kit::fonts::system());
        }
        let mut world = Self {
            root,
            main,
            html_mode: false,
            paged_library,
            html_library,
            inputs: opts.inputs,
            library_builds: 1,
            fonts,
            today: opts.today,
            packages: PackageStore::new(
                opts.package_paths, opts.allow_packages,
            ),
            allow_packages: opts.allow_packages,
            slots: Mutex::new(HashMap::new()),
            assets: AssetRegistry::new(),
            mounts: Vec::new(),
            render_files: HashMap::new(),
            provider: None,
            includes: Mutex::new(IncludeState::default()),
            accessed: Mutex::new(BTreeSet::new()),
        };
        world.set_source(project_id(VIRTUAL_MAIN), "", SourceMap::default());
        Ok(world)
    }

    /// Builds options from the effective configuration.
    ///
    /// Relative font and package directories are resolved against `root`.
    pub fn from_config(
        config: &Config, root: &Path, main: MainFile, today: Datetime,
    ) -> Result<Self> {
        let paths = |items: &[PathBuf]| {
            items.iter().map(|path| root.join(path)).collect()
        };
        Self::new(WorldOptions {
            root: root.to_path_buf(),
            main,
            today,
            system_fonts: config.pdf.system_fonts,
            font_paths: paths(&config.pdf.font_paths),
            package_paths: paths(&config.typst.package_paths),
            allow_packages: config.pdf.allow_packages,
            inputs: world_inputs(config),
        })
    }

    /// Replaces the virtual document source while retaining its syntax tree.
    pub fn set_main_text(&mut self, text: &str, map: SourceMap) {
        self.set_source(project_id(VIRTUAL_MAIN), text, map);
    }

    /// Replaces document assets and invalidates cached asset files.
    pub fn set_assets(&mut self, assets: AssetRegistry) {
        self.assets = assets;
        exclusive(&mut self.slots).retain(|id, _| !is_asset(*id));
    }

    /// Mounts a real directory at a virtual prefix, replacing an equal mount.
    pub fn mount(&mut self, prefix: &str, dir: &Path) {
        let Ok(path) = VirtualPath::new(prefix) else {
            return;
        };
        let prefix = path.get_with_slash().trim_end_matches('/').to_string();
        self.mounts.retain(|(existing, _)| *existing != prefix);
        self.mounts.push((prefix.clone(), self.root.join(dir)));
        self.mounts.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
        exclusive(&mut self.slots).retain(|id, _| {
            !matches!(id.root(), VirtualRoot::Project)
                || is_virtual(*id)
                || mounted_rest(id.vpath().get_with_slash(), &prefix).is_none()
        });
    }

    /// Removes mounts and cached disk files under a virtual path prefix.
    /// Reserved main, render and asset resources remain installed.
    pub(crate) fn unmount(&mut self, prefix: &str) {
        let Ok(path) = VirtualPath::new(prefix) else {
            return;
        };
        let prefix = path.get_with_slash().trim_end_matches('/');
        self.mounts
            .retain(|(existing, _)| mounted_rest(existing, prefix).is_none());
        exclusive(&mut self.slots).retain(|id, _| {
            !matches!(id.root(), VirtualRoot::Project)
                || is_virtual(*id)
                || mounted_rest(id.vpath().get_with_slash(), prefix).is_none()
        });
    }

    /// Rebuilds both libraries only when their input strings have changed.
    pub fn set_inputs(&mut self, inputs: BTreeMap<String, String>) {
        if self.inputs == inputs {
            return;
        }
        (self.paged_library, self.html_library) = libraries(&inputs);
        self.inputs = inputs;
        self.library_builds += 1;
    }

    /// Installs the provider used for subsequent Markdown/notebook includes.
    pub fn set_include_provider(&mut self, provider: Box<dyn IncludeProvider>) {
        self.provider = Some(provider);
        *exclusive(&mut self.includes) = IncludeState::default();
        exclusive(&mut self.slots).retain(|id, _| !is_asset(*id));
        for (id, slot) in exclusive(&mut self.slots) {
            if is_include(*id) {
                slot.source_result = None;
                slot.map = None;
            }
        }
    }

    /// Starts a rebuild, clearing observations and invalidating disk caches.
    pub fn reset(&mut self) {
        exclusive(&mut self.accessed).clear();
        *exclusive(&mut self.includes) = IncludeState::default();
        for (id, slot) in exclusive(&mut self.slots) {
            if !slot.retain || is_asset(*id) || debug_writer_package(*id) {
                slot.source_result = None;
                slot.bytes = None;
                slot.map = None;
            }
        }
    }

    /// Real paths consumed since the last reset, in deterministic order.
    pub fn accessed(&self) -> BTreeSet<PathBuf> {
        locked(&self.accessed).clone()
    }

    /// Diagnostics emitted by include providers during this rebuild.
    pub fn include_diagnostics(&self) -> Diagnostics {
        locked(&self.includes).diagnostics.clone()
    }

    /// The retained emitted-line mapping for a virtual or synthesized source.
    pub fn source_map(&self, id: FileId) -> Option<SourceMap> {
        locked(&self.slots).get(&id)?.map.clone()
    }

    /// The real source path, absent for embedded and virtual files.
    pub fn real_path(&self, id: FileId) -> Option<PathBuf> {
        if let Some(path) =
            locked(&self.slots).get(&id).and_then(|slot| slot.real_path.clone())
        {
            return Some(path);
        }
        if !matches!(id.root(), VirtualRoot::Project) || is_virtual(id) {
            return None;
        }
        self.project_path(id.vpath()).ok()
    }

    /// Full fixed timestamp selected when this compiler session was created.
    pub(crate) fn session_timestamp(&self) -> Datetime {
        self.today
    }

    pub(crate) fn allows_packages(&self) -> bool {
        self.allow_packages
    }

    pub(crate) fn with_render_main<T>(
        &mut self, text: &str, html: bool, run: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let id = project_id(RENDER_MAIN);
        self.set_source(id, text, SourceMap::default());
        let previous_main = std::mem::replace(&mut self.main, id);
        let previous_mode = std::mem::replace(&mut self.html_mode, html);
        let result = run(self);
        self.main = previous_main;
        self.html_mode = previous_mode;
        result
    }

    pub(crate) fn set_render_image(&mut self, ext: &str, bytes: &[u8]) {
        let path = format!("/__render/image.{ext}");
        self.render_files.insert(path.clone(), Bytes::new(bytes.to_vec()));
        exclusive(&mut self.slots).remove(&project_id(&path));
    }

    fn set_source(&mut self, id: FileId, text: &str, map: SourceMap) {
        let slot = exclusive(&mut self.slots).entry(id).or_default();
        replace_source(slot, id, text);
        slot.map = Some(map);
        slot.retain = true;
    }

    fn source_in_slot(
        &self, id: FileId, slot: &mut Slot,
    ) -> FileResult<Source> {
        if let Some(source) = &slot.source_result {
            return source.clone();
        }
        let result = if is_include(id) {
            self.synthesize(id, slot)
        } else {
            self.bytes_in_slot(id, slot).and_then(|bytes| {
                let text = std::str::from_utf8(&bytes)?;
                Ok(replace_source(
                    slot,
                    id,
                    text.trim_start_matches('\u{feff}'),
                ))
            })
        };
        slot.source_result = Some(result.clone());
        result
    }

    fn bytes_in_slot(&self, id: FileId, slot: &mut Slot) -> FileResult<Bytes> {
        if let Some(bytes) = &slot.bytes {
            return bytes.clone();
        }
        let result = self.load(id).map(|loaded| {
            slot.real_path = loaded.real_path;
            slot.retain = loaded.retain;
            loaded.bytes
        });
        slot.bytes = Some(result.clone());
        result
    }

    fn synthesize(&self, id: FileId, slot: &mut Slot) -> FileResult<Source> {
        let path = self.project_path(id.vpath())?;
        slot.real_path = Some(path.clone());
        let provider = self.provider.as_ref().ok_or_else(|| {
            FileError::Other(Some(
                "including .md/.ipynb from typst arrives in T53".into(),
            ))
        })?;
        let included =
            provider.synthesize(&path, &self.root).map_err(|error| {
                FileError::Other(Some(error.to_string().into()))
            })?;
        locked(&self.accessed).insert(path);
        locked(&self.accessed).extend(included.files);
        let mut state = locked(&self.includes);
        state.assets.merge(included.assets);
        state.diagnostics.extend(included.diagnostics);
        slot.map = Some(included.map);
        Ok(replace_source(slot, id, &included.text))
    }

    fn load(&self, id: FileId) -> FileResult<Loaded> {
        if let VirtualRoot::Package(spec) = id.root() {
            let (bytes, real_path) = self.packages.load(spec, id.vpath())?;
            if let Some(path) = &real_path {
                locked(&self.accessed).insert(path.clone());
            }
            return Ok(Loaded {
                bytes,
                retain: real_path.is_none(),
                real_path,
            });
        }
        let path = id.vpath().get_with_slash();
        if path == VIRTUAL_MAIN || path.starts_with("/__render/") {
            return self
                .render_files
                .get(path)
                .cloned()
                .map(|bytes| Loaded { bytes, real_path: None, retain: true })
                .ok_or_else(|| FileError::NotFound(path.into()));
        }
        if let Some(name) = path.strip_prefix("/__assets/") {
            return self.load_asset(name);
        }
        let path = self.project_path(id.vpath())?;
        self.load_disk(&path)
    }

    fn load_asset(&self, name: &str) -> FileResult<Loaded> {
        let id = name.split_once('.').map_or(name, |(id, _)| id);
        let state = locked(&self.includes);
        let registry = if self.assets.get(id).is_some() {
            &self.assets
        } else {
            &state.assets
        };
        let asset = registry.get(id).ok_or_else(|| {
            FileError::NotFound(format!("/__assets/{name}").into())
        })?;
        let bytes = registry.read(id).map_err(|error| {
            FileError::Other(Some(error.to_string().into()))
        })?;
        let real_path = match &asset.data {
            AssetData::File(path) => {
                let path = canonical_if_possible(path);
                locked(&self.accessed).insert(path.clone());
                Some(path)
            }
            AssetData::Bytes(_) => None,
        };
        Ok(Loaded {
            bytes: Bytes::new(bytes),
            retain: real_path.is_none(),
            real_path,
        })
    }

    fn load_disk(&self, path: &Path) -> FileResult<Loaded> {
        let bytes =
            fs::read(path).map_err(|error| FileError::from_io(error, path))?;
        let path = canonical_if_possible(path);
        locked(&self.accessed).insert(path.clone());
        Ok(Loaded {
            bytes: Bytes::new(bytes),
            real_path: Some(path),
            retain: false,
        })
    }

    fn project_path(&self, path: &VirtualPath) -> FileResult<PathBuf> {
        let virtual_path = path.get_with_slash();
        for (prefix, dir) in &self.mounts {
            if let Some(rest) = mounted_rest(virtual_path, prefix) {
                let path = VirtualPath::new(rest).map_err(|error| {
                    FileError::Other(Some(error.to_string().into()))
                })?;
                return Ok(canonical_if_possible(&path.realize(dir)?));
            }
        }
        Ok(canonical_if_possible(&path.realize(&self.root)?))
    }
}

impl World for WriterWorld {
    fn library(&self) -> &LazyHash<Library> {
        if self.html_mode {
            &self.html_library
        } else {
            &self.paged_library
        }
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        let mut slots = locked(&self.slots);
        self.source_in_slot(id, slots.entry(id).or_default())
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let mut slots = locked(&self.slots);
        let slot = slots.entry(id).or_default();
        if id.vpath().extension() == Some("typ") {
            self.source_in_slot(id, slot)?;
        }
        self.bytes_in_slot(id, slot)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        Datetime::from_ymd(
            self.today.year()?,
            self.today.month()?,
            self.today.day()?,
        )
    }
}

/// The canonical `writer-version` and compact `writer-config` input strings.
pub fn world_inputs(config: &Config) -> BTreeMap<String, String> {
    let toc = match config.document.toc {
        Toc::Off => 0,
        Toc::Depth(depth) => depth,
    };
    let json = serde_json::json!({
        "lang": config.document.lang,
        "toc": toc,
        "numbering": {
            "headings": config.numbering.headings,
            "figures": config.numbering.figures,
            "equations": config.numbering.equations,
        },
        "paper": config.pdf.paper,
        "cite_locale": config.references.locale,
    });
    BTreeMap::from([
        ("writer-version".into(), env!("CARGO_PKG_VERSION").into()),
        ("writer-config".into(), json.to_string()),
    ])
}

fn libraries(
    inputs: &BTreeMap<String, String>,
) -> (LazyHash<Library>, LazyHash<Library>) {
    let dict: Dict = inputs
        .iter()
        .map(|(key, value)| (key.as_str().into(), value.as_str().into_value()))
        .collect();
    let paged = Library::builder().with_inputs(dict.clone()).build();
    let html = Library::builder()
        .with_inputs(dict)
        .with_features([Feature::Html].into_iter().collect())
        .build();
    (LazyHash::new(paged), LazyHash::new(html))
}

fn main_id(root: &Path, main: MainFile) -> Result<FileId> {
    match main {
        MainFile::Virtual => Ok(project_id(VIRTUAL_MAIN)),
        MainFile::Path(path) => {
            let path = fs::canonicalize(root.join(path))?;
            let path = VirtualPath::virtualize(root, &path)
                .map_err(|error| Error::Config(error.to_string()))?;
            Ok(RootedPath::new(VirtualRoot::Project, path).intern())
        }
    }
}

fn project_id(path: &str) -> FileId {
    // Every caller constructs a known, absolute virtual path.
    let path = VirtualPath::new(path).expect("valid internal virtual path");
    RootedPath::new(VirtualRoot::Project, path).intern()
}

fn replace_source(slot: &mut Slot, id: FileId, text: &str) -> Source {
    let source =
        slot.source.get_or_insert_with(|| Source::new(id, text.into()));
    source.replace(text);
    let source = source.clone();
    slot.source_result = Some(Ok(source.clone()));
    if id.vpath().extension() == Some("typ") {
        slot.bytes = Some(Ok(Bytes::from_string(source.clone())));
    }
    source
}

// A caller panic must not make the session's cache permanently inaccessible.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn exclusive<T>(mutex: &mut Mutex<T>) -> &mut T {
    mutex.get_mut().unwrap_or_else(PoisonError::into_inner)
}

fn mounted_rest<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)
        .filter(|rest| rest.is_empty() || rest.starts_with('/'))
}

fn canonical_if_possible(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn is_include(id: FileId) -> bool {
    matches!(id.root(), VirtualRoot::Project)
        && matches!(id.vpath().extension(), Some("md" | "ipynb"))
}

fn is_asset(id: FileId) -> bool {
    matches!(id.root(), VirtualRoot::Project)
        && id.vpath().get_with_slash().starts_with("/__assets/")
}

fn is_virtual(id: FileId) -> bool {
    let path = id.vpath().get_with_slash();
    path == VIRTUAL_MAIN || path.starts_with("/__render/") || is_asset(id)
}

fn debug_writer_package(id: FileId) -> bool {
    cfg!(debug_assertions)
        && matches!(id.root(),
            VirtualRoot::Package(spec) if spec.namespace == "writer" && spec.name == "doc"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typst::compile::compile_document;

    fn world(root: &Path) -> WriterWorld {
        WriterWorld::new(WorldOptions {
            root: root.into(),
            main: MainFile::Virtual,
            today: Datetime::from_ymd(2026, 10, 24).unwrap(),
            system_fonts: false,
            font_paths: Vec::new(),
            package_paths: Vec::new(),
            allow_packages: false,
            inputs: world_inputs(&Config::default()),
        })
        .unwrap()
    }

    #[test]
    fn world_inputs_default_config() {
        let inputs = world_inputs(&Config::default());
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs["writer-version"], "0.1.0");
        assert_eq!(
            inputs["writer-config"],
            r#"{"lang":"en","toc":0,"numbering":{"headings":false,"figures":"document","equations":"labelled"},"paper":"a4","cite_locale":""}"#
        );
    }

    #[test]
    fn world_inputs_follow_config() {
        let config: Config = toml::from_str(
            "[document]\nlang = 'de'\ntoc = true\n\
             [numbering]\nheadings = true\nfigures = 'section'\n\
             [references]\nlocale = 'de'",
        )
        .unwrap();
        assert_eq!(
            world_inputs(&config)["writer-config"],
            r#"{"lang":"de","toc":3,"numbering":{"headings":"1.1","figures":"section","equations":"labelled"},"paper":"a4","cite_locale":"de"}"#
        );
    }

    #[test]
    fn set_inputs_unchanged_keeps_libraries() {
        let dir = tempfile::tempdir().unwrap();
        let mut world = world(dir.path());
        let paged = &world.paged_library as *const _;
        let html = &world.html_library as *const _;
        world.set_inputs(world.inputs.clone());
        assert_eq!(world.library_builds, 1);
        assert_eq!(&world.paged_library as *const _, paged);
        assert_eq!(&world.html_library as *const _, html);
        let mut inputs = world.inputs.clone();
        inputs.insert("other".into(), "value".into());
        world.set_inputs(inputs.clone());
        assert_eq!(world.library_builds, 2);
        world.set_inputs(inputs);
        assert_eq!(world.library_builds, 2);
    }

    #[test]
    fn mitex_from_embedded_store() {
        let dir = tempfile::tempdir().unwrap();
        let mut world = world(dir.path());
        world.set_main_text(
            "#import \"@preview/mitex:0.2.7\": mi\n#mi(\"\\frac{a}{b}\")",
            SourceMap::default(),
        );
        compile_document(&mut world).into_result().unwrap();
        assert!(world.accessed().is_empty());
    }

    #[test]
    fn embedded_fonts_first() {
        let dir = tempfile::tempdir().unwrap();
        let world = world(dir.path());
        let expected = [
            "Libertinus Serif", "New Computer Modern",
            "New Computer Modern Math", "DejaVu Sans Mono",
        ];
        let embedded: Vec<_> = typst_kit::fonts::embedded().collect();
        for family in expected {
            assert!(world.book().families().any(|(name, _)| name == family));
        }
        for (index, (_, info)) in embedded.iter().enumerate() {
            assert_eq!(world.book().info(index), Some(info));
        }
        assert!(world.font(usize::MAX).is_none());
    }

    #[test]
    fn mounts_preserve_reserved_sources() {
        let dir = tempfile::tempdir().unwrap();
        let mounted = tempfile::tempdir().unwrap();
        let mut world = world(dir.path());
        world.set_main_text("Virtual main", SourceMap::default());
        world.mount("/", mounted.path());
        assert_eq!(world.source(world.main()).unwrap().text(), "Virtual main");
        world.with_render_main("Render source", false, |world| {
            world.mount("/__render", mounted.path());
            assert_eq!(
                world.source(world.main()).unwrap().text(),
                "Render source"
            );
        });
        assert_eq!(world.source(world.main()).unwrap().text(), "Virtual main");
    }

    #[test]
    fn session_timestamp_retains_time_while_world_today_is_date_only() {
        let dir = tempfile::tempdir().unwrap();
        let timestamp =
            Datetime::from_ymd_hms(2026, 10, 28, 23, 45, 6).unwrap();
        let mut world = WriterWorld::new(WorldOptions {
            root: dir.path().into(),
            main: MainFile::Virtual,
            today: timestamp,
            system_fonts: false,
            font_paths: Vec::new(),
            package_paths: Vec::new(),
            allow_packages: false,
            inputs: world_inputs(&Config::default()),
        })
        .unwrap();
        assert_eq!(world.session_timestamp(), timestamp);
        assert_eq!(world.session_timestamp().hour(), Some(23));
        assert_eq!(world.session_timestamp().minute(), Some(45));
        assert_eq!(world.session_timestamp().second(), Some(6));
        assert_eq!(world.today(None), Datetime::from_ymd(2026, 10, 28));
        world.reset();
        assert_eq!(world.session_timestamp(), timestamp);
    }

    #[test]
    fn unmount_invalidates_sources_and_bytes_then_uses_remaining_roots() {
        let root = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let child = tempfile::tempdir().unwrap();
        let sibling = tempfile::tempdir().unwrap();
        for (dir, text) in [(root.path(), "project"), (parent.path(), "parent")]
        {
            let dir = dir.join(if text == "project" {
                "__writer/bib/0"
            } else {
                "bib/0"
            });
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("entry.typ"), text).unwrap();
            fs::write(dir.join("data.bin"), text).unwrap();
        }
        fs::write(child.path().join("entry.typ"), "child").unwrap();
        fs::write(child.path().join("data.bin"), "child").unwrap();
        fs::write(sibling.path().join("data.bin"), "sibling").unwrap();
        let mut world = world(root.path());
        world.mount("/__writer", parent.path());
        world.mount("/__writer/bib/0", child.path());
        world.mount("/__writer/bibliography", sibling.path());
        let source = project_id("/__writer/bib/0/entry.typ");
        let bytes = project_id("/__writer/bib/0/data.bin");
        let neighbor = project_id("/__writer/bibliography/data.bin");
        assert_eq!(world.source(source).unwrap().text(), "child");
        assert_eq!(&*world.file(bytes).unwrap(), b"child");
        assert_eq!(&*world.file(neighbor).unwrap(), b"sibling");
        fs::write(sibling.path().join("data.bin"), "changed").unwrap();
        world.unmount("/__writer/bib/");
        assert_eq!(world.source(source).unwrap().text(), "parent");
        assert_eq!(&*world.file(bytes).unwrap(), b"parent");
        // A similar spelling is outside the removed path-component prefix.
        assert_eq!(&*world.file(neighbor).unwrap(), b"sibling");
        assert!(
            world
                .mounts
                .iter()
                .any(|(prefix, _)| { prefix == "/__writer/bibliography" })
        );
        world.unmount("/__writer");
        assert_eq!(world.source(source).unwrap().text(), "project");
        assert_eq!(&*world.file(bytes).unwrap(), b"project");
        assert!(world.mounts.is_empty());
    }

    #[test]
    fn unmount_preserves_virtual_main_and_render_sources_and_bytes() {
        let root = tempfile::tempdir().unwrap();
        let mounted = tempfile::tempdir().unwrap();
        let mut world = world(root.path());
        world.set_main_text("Virtual main", SourceMap::default());
        world.set_render_image("png", b"render image");
        let image = project_id("/__render/image.png");
        assert_eq!(&*world.file(image).unwrap(), b"render image");
        world.mount("/", mounted.path());
        world.with_render_main("Render source", false, |world| {
            world.mount("/__render", mounted.path());
            world.unmount("/");
            assert_eq!(
                world.source(world.main()).unwrap().text(),
                "Render source"
            );
            assert_eq!(&*world.file(world.main()).unwrap(), b"Render source");
            assert_eq!(&*world.file(image).unwrap(), b"render image");
            assert!(world.mounts.is_empty());
        });
        assert_eq!(world.source(world.main()).unwrap().text(), "Virtual main");
        assert_eq!(&*world.file(world.main()).unwrap(), b"Virtual main");
        assert_eq!(&*world.file(image).unwrap(), b"render image");
    }

    #[test]
    fn md_include_without_provider() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("part.md"), "# Markdown").unwrap();
        let world = world(dir.path());
        let error = world.source(project_id("/part.md")).unwrap_err();
        assert!(error.to_string().contains("arrives in T53"));
        assert_eq!(
            &*world.file(project_id("/part.md")).unwrap(),
            b"# Markdown"
        );
    }
}
