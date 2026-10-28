//! File watching, change classification and selective rebuilds (§6–§7).
//!
//! Files are watched through their parents so atomic replacement saves count.
//! Theme directories are recursive, except for editor files and `.git`.
//! Missing directories use their nearest existing ancestor until an update
//! can watch them directly. The notify backend feeds one `writer-watch`
//! coalescing thread; dropping the watcher stops it and flushes pending work.
//!
//! A watch session starts before its first build, retains one compiler World,
//! and ignores its own written outputs. Only actual changes count: inotify
//! access events from reading inputs must never start another rebuild.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _,
};
use typst_kit::packages::FsPackages;

use crate::assets::registry::{AssetRegistry, normalize_lexically};
use crate::config::{Config, PdfEngine, Target};
use crate::error::{Diagnostics, Error, Result};
use crate::ir::{Document, Features, SourceInfo, SourceKind};
use crate::markdown::include::scan_includes;
use crate::pipeline::{
    self, BuildFailure, BuildOptions, BuildReport, InputRole, Mode, Prepared,
    WorldSlot,
};
use crate::typst::world::WriterWorld;

pub use crate::pipeline::ChangeKind;

/// Quiet time after the most recent relevant filesystem event.
pub const DEBOUNCE: Duration = Duration::from_millis(150);
/// Maximum time from the first relevant event until its batch is sent.
pub const MAX_BATCH_WAIT: Duration = Duration::from_secs(2);

/// Coalescing windows; configurable in backend tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Debounce {
    /// Time without another relevant event before sending a batch.
    pub quiet: Duration,
    /// Maximum duration of one continuously changing batch.
    pub max_wait: Duration,
}

impl Default for Debounce {
    fn default() -> Self {
        Self { quiet: DEBOUNCE, max_wait: MAX_BATCH_WAIT }
    }
}

/// Canonical, sorted and deduplicated inputs and watch directories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchSet {
    /// Individual input files; only listed files count under flat watches.
    pub files: Vec<PathBuf>,
    /// Directories whose descendants count, subject to [`is_ignored`].
    pub dirs_recursive: Vec<PathBuf>,
    /// Parents of files, watched without recursive event delivery.
    pub dirs_flat: Vec<PathBuf>,
    /// Theme-chain roots, for stylesheet and theme-asset classification.
    pub theme_dirs: Vec<PathBuf>,
    /// Files or directory roots whose changes only affect typeset output.
    pub pages_paths: Vec<PathBuf>,
}

impl WatchSet {
    /// Watches files through their parents and theme directories recursively.
    pub fn new(
        files: impl IntoIterator<Item = PathBuf>,
        theme_dirs: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        let files = canonical_set(files);
        let themes = canonical_set(theme_dirs);
        let dirs_flat = files
            .iter()
            .filter_map(|file| file.parent().map(Path::to_path_buf))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self {
            files: files.into_iter().collect(),
            dirs_flat,
            dirs_recursive: themes.iter().cloned().collect(),
            theme_dirs: themes.into_iter().collect(),
            pages_paths: Vec::new(),
        }
    }

    /// Unions all lists, keeping paths canonical, sorted and deduplicated.
    pub fn merge(&mut self, other: &Self) {
        let union = |mine: &mut Vec<PathBuf>, theirs: &[PathBuf]| {
            *mine = canonical_set(mine.drain(..).chain(theirs.iter().cloned()))
                .into_iter()
                .collect();
        };
        union(&mut self.files, &other.files);
        union(&mut self.dirs_recursive, &other.dirs_recursive);
        union(&mut self.dirs_flat, &other.dirs_flat);
        union(&mut self.theme_dirs, &other.theme_dirs);
        union(&mut self.pages_paths, &other.pages_paths);
    }
}

/// Converts build input roles and already package-filtered World reads to watches.
/// `None` selects serve inputs; build sessions include target-specific resources.
pub fn watch_set(
    inputs: &[(PathBuf, InputRole)], accessed: &[PathBuf],
    targets: Option<&[Target]>,
) -> WatchSet {
    let mut files = accessed.to_vec();
    let mut themes = Vec::new();
    let mut pages = Vec::new();
    for (path, role) in inputs {
        match role {
            InputRole::Theme => themes.push(path.clone()),
            InputRole::ReferenceDoc
                if !targets.is_some_and(|t| t.contains(&Target::Docx)) => {}
            InputRole::LatexTemplate
                if !targets.is_some_and(|t| t.contains(&Target::Latex)) => {}
            InputRole::PdfTemplate => {
                files.push(path.clone());
                pages.push(path.clone());
            }
            _ => files.push(path.clone()),
        }
    }
    let mut set = WatchSet::new(files, themes);
    set.pages_paths = canonical_set(pages).into_iter().collect();
    if cfg!(debug_assertions) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let theme = root.join("assets/themes/default");
        if theme.is_dir() {
            set.merge(&WatchSet::new([], [theme]));
        }
        let package = root.join("assets/typst/writer-doc");
        if package.is_dir() {
            let package = canonical(&package);
            set.merge(&WatchSet {
                dirs_recursive: vec![package.clone()],
                pages_paths: vec![package],
                ..Default::default()
            });
        }
    }
    set
}

/// Watches known inputs before parsing, including invalid project configuration.
pub fn watch_set_before_build(opts: &BuildOptions) -> WatchSet {
    let path = std::path::absolute(&opts.input)
        .map(|path| normalize_lexically(&path))
        .unwrap_or_else(|_| opts.input.clone());
    let kind = opts
        .format
        .or_else(|| crate::ir::detect_kind(&path).ok())
        .unwrap_or(SourceKind::Markdown);
    let (config, config_path) =
        pipeline::load_config(opts).unwrap_or_else(|_| {
            let mut config = Config::default();
            config.apply_cli(&opts.cli);
            (config, config_file(opts))
        });
    let includes = if kind == SourceKind::Markdown {
        scan_includes(&opts.input)
    } else {
        Vec::new()
    };
    let prepared = Prepared {
        document: Document {
            meta: Default::default(),
            blocks: Vec::new(),
            features: Features::default(),
            source: SourceInfo {
                base_dir: path.parent().unwrap_or(Path::new("/")).to_path_buf(),
                path,
                kind,
            },
            assets: Vec::new(),
            includes,
            resolved: Default::default(),
        },
        assets: AssetRegistry::new(),
        config,
        config_path,
        diagnostics: Diagnostics::new(),
    };
    watch_set(&pipeline::watch_inputs(&prepared), &[], None)
}

fn config_file(opts: &BuildOptions) -> Option<PathBuf> {
    if let Some(path) = &opts.config_path {
        return Some(path.clone());
    }
    let input = std::path::absolute(&opts.input).ok()?;
    input
        .parent()?
        .ancestors()
        .map(|dir| dir.join(crate::config::CONFIG_FILE_NAME))
        .find(|path| path.is_file())
}

/// A coalesced burst of relevant paths and the rebuild it requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeBatch {
    /// Canonical paths in sorted, deduplicated order.
    pub paths: Vec<PathBuf>,
    /// Classification against the watch set in effect when the batch was sent.
    pub kind: ChangeKind,
}

/// Classifies a nonempty uniform batch as CSS or pages; all other batches are content.
pub fn classify(paths: &[PathBuf], set: &WatchSet) -> ChangeKind {
    classify_paths(paths, &set.theme_dirs, &set.pages_paths)
}

fn classify_paths(
    paths: &[PathBuf], themes: &[PathBuf], pages: &[PathBuf],
) -> ChangeKind {
    if !paths.is_empty() && paths.iter().all(|path| is_style_path(path, themes))
    {
        ChangeKind::Css
    } else if !paths.is_empty()
        && paths
            .iter()
            .all(|path| pages.iter().any(|root| path.starts_with(root)))
    {
        ChangeKind::Pages
    } else {
        ChangeKind::Content
    }
}

fn is_style_path(path: &Path, themes: &[PathBuf]) -> bool {
    path.extension().is_some_and(|ext| {
        ["css", "scss", "sass"]
            .iter()
            .any(|style| ext.eq_ignore_ascii_case(style))
    }) || themes.iter().any(|theme| {
        path.starts_with(theme.join("styles"))
            || path.starts_with(theme.join("assets"))
    })
}

/// Directory names whose contents never count below a recursively watched directory.
/// (`dist` and `target` are not among them: a theme may keep inputs there, audit F18.)
const IGNORED_DIRS: [&str; 1] = [".git"];

/// Whether `rel` (a path relative to a recursively watched directory) is on the ignore
/// list: anything inside `.git/`, and editor files: backups (`*~`), Emacs lock and
/// auto-save files (`.#*`, `#*#`), vim swap files (`*.swp`, `*.swx`, and `.*.swa` …
/// `.*.swo` for further sessions) and vim's `4913` probe, and `.DS_Store`.
pub fn is_ignored(rel: &Path) -> bool {
    let in_ignored_dir = rel.components().any(|component| match component {
        Component::Normal(name) => IGNORED_DIRS.iter().any(|dir| name == *dir),
        _ => false,
    });
    let Some(name) = rel.file_name().and_then(|name| name.to_str()) else {
        return in_ignored_dir;
    };
    in_ignored_dir
        || name.ends_with('~')
        || name.starts_with(".#")
        || (name.len() > 1 && name.starts_with('#') && name.ends_with('#'))
        || name.ends_with(".swp")
        || name.ends_with(".swx")
        || is_vim_swap(name)
        || name == "4913"
        || name == ".DS_Store"
}

/// Whether `name` is a vim swap file of a later session: `.<file>.sw<a…p>` (vim counts
/// down from `.swp`).
fn is_vim_swap(name: &str) -> bool {
    let Some(stem) = name.strip_prefix('.') else {
        return false;
    };
    match stem.rsplit_once('.') {
        Some((file, ext)) => {
            !file.is_empty()
                && ext.len() == 3
                && ext.starts_with("sw")
                && ext.as_bytes()[2].is_ascii_lowercase()
                && ext.as_bytes()[2] <= b'p'
        }
        None => false,
    }
}

/// `path` with symlinks resolved, like the paths the OS reports: canonicalized when it
/// exists, else its parent canonicalized (recursively) joined with its name.
pub fn canonical(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    pipeline::path_after_creating_dirs(path)
        .unwrap_or_else(|| path.to_path_buf())
}

/// The canonical paths of `paths`, sorted and without duplicates.
fn canonical_set(
    paths: impl IntoIterator<Item = PathBuf>,
) -> BTreeSet<PathBuf> {
    paths.into_iter().map(|path| canonical(&path)).collect()
}

/// Which paths count, shared by [`Watcher::update`] and the coalescing thread.
#[derive(Debug, Default)]
struct Filter {
    /// [`WatchSet::files`], canonical.
    files: BTreeSet<PathBuf>,
    /// The recursively watched directories (no one inside another).
    recursive: Vec<PathBuf>,
    /// [`WatchSet::theme_dirs`], canonical.
    theme_dirs: Vec<PathBuf>,
    /// Typeset-only inputs, for classifying queued changes.
    pages_paths: Vec<PathBuf>,
    /// Every directory to watch and how.
    targets: BTreeMap<PathBuf, RecursiveMode>,
}

impl Filter {
    /// The filter for `set`: recursive targets are `dirs_recursive` and `theme_dirs`
    /// (without directories inside another one), flat targets are `dirs_flat` and the
    /// parents of `files` that no recursive target already covers.
    fn new(set: &WatchSet) -> Filter {
        let files = canonical_set(set.files.iter().cloned());
        let theme_dirs = canonical_set(set.theme_dirs.iter().cloned());
        let all_recursive = canonical_set(set.dirs_recursive.iter().cloned())
            .into_iter()
            .chain(theme_dirs.iter().cloned())
            .collect::<BTreeSet<_>>();
        let covered = |dir: &Path, roots: &BTreeSet<PathBuf>| {
            roots.iter().any(|root| dir != root && dir.starts_with(root))
        };
        let recursive: Vec<PathBuf> = all_recursive
            .iter()
            .filter(|dir| !covered(dir, &all_recursive))
            .cloned()
            .collect();
        let flat = canonical_set(set.dirs_flat.iter().cloned())
            .into_iter()
            .chain(
                files.iter().filter_map(|f| f.parent().map(Path::to_path_buf)),
            )
            .filter(|dir| !recursive.iter().any(|root| dir.starts_with(root)));
        let mut targets: BTreeMap<PathBuf, RecursiveMode> =
            flat.map(|dir| (dir, RecursiveMode::NonRecursive)).collect();
        for dir in &recursive {
            targets.insert(dir.clone(), RecursiveMode::Recursive);
        }
        Filter {
            files,
            recursive,
            theme_dirs: theme_dirs.into_iter().collect(),
            pages_paths: canonical_set(set.pages_paths.iter().cloned())
                .into_iter()
                .collect(),
            targets,
        }
    }

    /// Whether a change of `path` (canonical) counts: one of the files, or below a
    /// recursively watched directory and not on the ignore list.
    fn matches(&self, path: &Path) -> bool {
        self.files.contains(path)
            || self.recursive.iter().any(|root| {
                path.strip_prefix(root).is_ok_and(|rel| !is_ignored(rel))
            })
    }
}

/// The directories to watch for `targets` and the targets that do not exist: each target
/// that exists, as it asks; for each missing one its nearest existing ancestor,
/// non-recursively, unless a directory already watched reports changes there.
fn plan_watches(
    targets: &BTreeMap<PathBuf, RecursiveMode>,
) -> (BTreeMap<PathBuf, RecursiveMode>, BTreeSet<PathBuf>) {
    let (mut wanted, missing): (BTreeMap<_, _>, BTreeMap<_, _>) = targets
        .iter()
        .map(|(dir, mode)| (dir.clone(), *mode))
        .partition(|(dir, _)| dir.is_dir());
    for dir in missing.keys() {
        if let Some(ancestor) = existing_ancestor(dir)
            && !covers(&wanted, &ancestor)
        {
            wanted.insert(ancestor, RecursiveMode::NonRecursive);
        }
    }
    (wanted, missing.into_keys().collect())
}

/// Whether changes of the entries of `dir` reach a watch in `watched`: `dir` itself is
/// watched, or it lies below a recursively watched directory.
fn covers(watched: &BTreeMap<PathBuf, RecursiveMode>, dir: &Path) -> bool {
    watched.contains_key(dir)
        || watched.iter().any(|(root, mode)| {
            *mode == RecursiveMode::Recursive && dir.starts_with(root)
        })
}

/// The parents to watch next to `wanted` on Windows, non-recursively: there, a watched
/// directory's own removal (and its creation again) is reported only by its parent's
/// watch, the directory's own handle dies with it. Parents that `wanted` covers already
/// are left out. Empty elsewhere: inotify and FSEvents report the removal of a watched
/// directory themselves.
fn guard_watches(
    wanted: &BTreeMap<PathBuf, RecursiveMode>,
) -> BTreeSet<PathBuf> {
    if !cfg!(windows) {
        return BTreeSet::new();
    }
    wanted
        .keys()
        .filter_map(|dir| dir.parent().map(Path::to_path_buf))
        .filter(|parent| !covers(wanted, parent))
        .collect()
}

/// The nearest proper ancestor of `path` that is an existing directory.
fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors().skip(1).find(|dir| dir.is_dir()).map(Path::to_path_buf)
}

/// State shared between the [`Watcher`] and its coalescing thread.
#[derive(Debug)]
struct Shared {
    /// What counts.
    filter: Filter,
    /// Targets of the filter that do not exist (watched through an ancestor).
    missing: BTreeSet<PathBuf>,
    /// Watched directories, and directories on the way to a missing target, that were
    /// created or removed since the last update.
    recreated: BTreeSet<PathBuf>,
    /// When the last batch was sent (at first: when the watcher started).
    since: SystemTime,
}

impl Shared {
    /// Whether a creation, removal or rename of `path` changes what can be watched: it is
    /// a target, a directory above one (removed or renamed, it takes the target away; on
    /// Windows the parent's watch is the one that reliably reports this, the target's own
    /// handle dies with it), or a directory on the way to a missing one.
    fn is_structural(&self, path: &Path) -> bool {
        self.filter.targets.keys().any(|target| target.starts_with(path))
            || self.missing.iter().any(|missing| missing.starts_with(path))
    }
}

/// Locks `shared`; a panic while it was held cannot leave it inconsistent (every update
/// replaces whole fields), so a poisoned lock is used as is.
fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Watches the inputs of a build and sends one [`ChangeBatch`] per burst of changes
/// (module docs: what is watched, debouncing, threads).
pub struct Watcher {
    /// The backend watcher; dropping it stops its thread, and then the coalescing thread.
    backend: RecommendedWatcher,
    /// Filter and recreated directories, shared with the coalescing thread.
    shared: Arc<Mutex<Shared>>,
    /// The directories currently watched, and how.
    watched: BTreeMap<PathBuf, RecursiveMode>,
    /// Of `watched`, the parents watched only to see a watched directory removed or
    /// created again ([`guard_watches`]; Windows); [`Watcher::watched_dirs`] leaves them out.
    guards: BTreeSet<PathBuf>,
    /// Where [`Watcher::update`] sends its catch-up batches.
    tx: Sender<ChangeBatch>,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watcher")
            .field("watched", &self.watched)
            .finish_non_exhaustive()
    }
}

impl Watcher {
    /// Starts watching `set`; batches go to `tx` until the watcher is dropped. Changes are
    /// coalesced with [`Debounce::default`] (the contract: [`DEBOUNCE`],
    /// [`MAX_BATCH_WAIT`]).
    ///
    /// Errors: the backend cannot be started, or a directory that exists cannot be watched
    /// (e.g. the inotify watch limit). A directory that does not exist is watched through
    /// its nearest existing ancestor (module docs).
    pub fn start(set: &WatchSet, tx: Sender<ChangeBatch>) -> Result<Watcher> {
        Watcher::start_with(set, tx, Debounce::default())
    }

    /// [`Watcher::start`] coalescing changes with `debounce` instead of the default.
    pub fn start_with(
        set: &WatchSet, tx: Sender<ChangeBatch>, debounce: Debounce,
    ) -> Result<Watcher> {
        let (raw_tx, raw_rx) = mpsc::channel::<notify::Result<Event>>();
        let backend = notify::recommended_watcher(raw_tx)
            .map_err(|e| watch_error("start", None, e))?;
        let shared = Arc::new(Mutex::new(Shared {
            filter: Filter::default(),
            missing: BTreeSet::new(),
            recreated: BTreeSet::new(),
            since: SystemTime::now(),
        }));
        let thread_shared = Arc::clone(&shared);
        let thread_tx = tx.clone();
        std::thread::Builder::new().name("writer-watch".into()).spawn(
            move || coalesce(&raw_rx, &thread_tx, &thread_shared, debounce),
        )?;
        let mut watcher = Watcher {
            backend,
            shared,
            watched: BTreeMap::new(),
            guards: BTreeSet::new(),
            tx,
        };
        watcher.update(set)?;
        Ok(watcher)
    }

    /// Watches `set` instead of the current set (call it after every build): directories
    /// no longer needed are unwatched, new ones watched, ones that were recreated since
    /// the last update (or could not be watched before) are watched again; a missing one
    /// is watched through its nearest existing ancestor. Then the inputs modified since the
    /// last batch that may have been missed are sent as a batch (module docs).
    ///
    /// Errors: as [`Watcher::start`]; the other directories are still updated.
    pub fn update(&mut self, set: &WatchSet) -> Result<()> {
        let filter = Filter::new(set);
        let (mut wanted, missing) = plan_watches(&filter.targets);
        self.guards = guard_watches(&wanted);
        for parent in &self.guards {
            wanted.insert(parent.clone(), RecursiveMode::NonRecursive);
        }
        let (files, recursive) =
            (filter.files.clone(), filter.recursive.clone());
        // Swap the filter first, so that events of newly watched directories count.
        let (recreated, since, old_files) = {
            let mut shared = lock(&self.shared);
            let old = std::mem::replace(&mut shared.filter, filter);
            shared.missing = missing;
            (std::mem::take(&mut shared.recreated), shared.since, old.files)
        };
        let stale: Vec<PathBuf> = self
            .watched
            .iter()
            .filter(|(dir, mode)| {
                wanted.get(*dir) != Some(*mode) || recreated.contains(*dir)
            })
            .map(|(dir, _)| dir.clone())
            .collect();
        let unwatched = !stale.is_empty();
        for dir in stale {
            self.watched.remove(&dir);
            if let Err(err) = self.backend.unwatch(&dir) {
                tracing::debug!("unwatch {}: {err}", dir.display());
            }
        }
        let (watched, result) = self.watch_all(wanted);
        if unwatched || watched {
            self.catch_up(files.iter(), &recursive, since);
        } else {
            self.catch_up(files.difference(&old_files), &[], since);
        }
        result
    }

    /// Watches every directory of `wanted` that is not watched yet. A directory that has
    /// disappeared in the meantime is watched through its nearest existing ancestor.
    /// Returns whether a directory was newly watched, and the first error.
    fn watch_all(
        &mut self, wanted: BTreeMap<PathBuf, RecursiveMode>,
    ) -> (bool, Result<()>) {
        let mut queue: Vec<(PathBuf, RecursiveMode)> = wanted
            .into_iter()
            .filter(|(dir, _)| !self.watched.contains_key(dir))
            .collect();
        let mut added = false;
        let mut first_error = None;
        while let Some((dir, mode)) = queue.pop() {
            // An ancestor queued for a directory that vanished may be wanted itself: watch
            // it once, recursively when either asks for that.
            let done = self.watched.get(&dir).is_some_and(|watched| {
                *watched == mode || *watched == RecursiveMode::Recursive
            });
            if done {
                continue;
            }
            match self.backend.watch(&dir, mode) {
                Ok(()) => {
                    tracing::debug!("watching {} ({mode:?})", dir.display());
                    self.watched.insert(dir, mode);
                    added = true;
                }
                Err(err) if is_not_found(&err) => {
                    tracing::debug!("not watching {} (missing)", dir.display());
                    lock(&self.shared).missing.insert(dir.clone());
                    if let Some(ancestor) = existing_ancestor(&dir)
                        && !covers(&self.watched, &ancestor)
                    {
                        queue.push((ancestor, RecursiveMode::NonRecursive));
                    }
                }
                Err(err) => {
                    first_error.get_or_insert(watch_error(
                        "watch",
                        Some(&dir),
                        err,
                    ));
                }
            }
        }
        (added, first_error.map_or(Ok(()), Err))
    }

    /// Sends a batch for the inputs modified at or after `since`: of `files`, and the files
    /// below the `recursive` directories that are not on the ignore list. Their changes may
    /// have been missed while the watch set changed (module docs). A batch sent counts as
    /// the last batch ([`Shared::since`]), so later updates do not report the same changes
    /// again.
    fn catch_up<'a>(
        &self, files: impl Iterator<Item = &'a PathBuf>, recursive: &[PathBuf],
        since: SystemTime,
    ) {
        // Whatever changes from now on is reported by the watches (or the next update).
        let scanned = SystemTime::now();
        let mut paths: BTreeSet<PathBuf> =
            files.filter(|file| modified_since(file, since)).cloned().collect();
        for root in recursive {
            modified_below(root, root, since, &mut paths);
        }
        if paths.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = paths.into_iter().collect();
        tracing::debug!("changed while the watches changed: {paths:?}");
        let kind = {
            let mut shared = lock(&self.shared);
            shared.since = shared.since.max(scanned);
            classify_paths(
                &paths, &shared.filter.theme_dirs, &shared.filter.pages_paths,
            )
        };
        // Nobody receiving is not an error of the update.
        let _ = self.tx.send(ChangeBatch { paths, kind });
    }

    /// The directories currently watched (canonical), recursive ones flagged `true`. This
    /// includes the ancestors watched for missing directories, not the parents watched on
    /// Windows for a watched directory's removal (`guard_watches`).
    pub fn watched_dirs(&self) -> Vec<(PathBuf, bool)> {
        self.watched
            .iter()
            .filter(|(dir, _)| !self.guards.contains(*dir))
            .map(|(dir, mode)| (dir.clone(), *mode == RecursiveMode::Recursive))
            .collect()
    }
}

/// Whether `path` is a file modified at or after `since`.
fn modified_since(path: &Path, since: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| modified >= since)
}

/// Whether `path` exists and was last changed before `since`, by its inode change time
/// (set by every write, rename, `touch` and permission change, and not by user code).
/// Only a sub-second change time counts: a file system with whole-second times cannot
/// tell a change in the same second as `since` from an earlier one, so nothing there is
/// ever "before".
///
/// macOS only, the one platform whose backend reports changes from before a watch
/// (FSEvents, module docs), and one that stamps files from the system clock. Linux stamps
/// them from a coarse clock that runs up to a tick (1-10 ms) behind it, so a change made
/// right after a batch can look older than the batch; inotify and Windows report nothing
/// from before a watch anyway, so there nothing is "before".
#[cfg(target_os = "macos")]
fn changed_before(path: &Path, since: SystemTime) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let (Ok(secs), Ok(nanos)) =
        (u64::try_from(meta.ctime()), u32::try_from(meta.ctime_nsec()))
    else {
        return false;
    };
    nanos != 0 && std::time::UNIX_EPOCH + Duration::new(secs, nanos) < since
}

/// The macOS version says why nothing is "before" elsewhere.
#[cfg(not(target_os = "macos"))]
fn changed_before(_path: &Path, _since: SystemTime) -> bool {
    false
}

/// Adds the files below `dir` (inside the recursively watched `root`) that were modified at
/// or after `since` to `found`, skipping the ignore list. Symbolic links are not followed.
fn modified_below(
    root: &Path, dir: &Path, since: SystemTime, found: &mut BTreeSet<PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.strip_prefix(root).is_ok_and(is_ignored) {
            continue;
        }
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {
                modified_below(root, &path, since, found)
            }
            Ok(kind) if kind.is_file() && modified_since(&path, since) => {
                found.insert(path);
            }
            _ => {}
        }
    }
}

/// Whether a notify error means the path does not exist.
fn is_not_found(err: &notify::Error) -> bool {
    match &err.kind {
        notify::ErrorKind::PathNotFound => true,
        notify::ErrorKind::Io(io) => io.kind() == std::io::ErrorKind::NotFound,
        _ => false,
    }
}

/// A notify error as an [`Error::Other`]: `file watcher: cannot <what> [<dir>]: <err>`.
fn watch_error(what: &str, dir: Option<&Path>, err: notify::Error) -> Error {
    let dir = dir.map(|dir| format!(" {}", dir.display())).unwrap_or_default();
    Error::Other(format!("file watcher: cannot {what}{dir}: {err}"))
}

/// The coalescing thread: gathers the backend's events into batches (module docs; the
/// window is `debounce`) and sends them on `tx`. Returns when the backend is gone or
/// nobody receives.
fn coalesce(
    raw: &Receiver<notify::Result<Event>>, tx: &Sender<ChangeBatch>,
    shared: &Mutex<Shared>, debounce: Debounce,
) {
    let mut pending = BTreeSet::new();
    // When the batch being gathered got its first and its latest relevant change.
    let mut window: Option<(Instant, Instant)> = None;
    loop {
        let next = match window {
            None => raw.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some((first, last)) => raw.recv_timeout(
                debounce
                    .quiet
                    .saturating_sub(last.elapsed())
                    .min(debounce.max_wait.saturating_sub(first.elapsed())),
            ),
        };
        match next {
            Ok(result) => {
                if collect(result, shared, &mut pending) {
                    let now = Instant::now();
                    window = Some(
                        window.map_or((now, now), |(first, _)| (first, now)),
                    );
                }
                // Irrelevant events keep coming without a timeout: check the clock too.
                let due = window.is_some_and(|(first, last)| {
                    last.elapsed() >= debounce.quiet
                        || first.elapsed() >= debounce.max_wait
                });
                if !due {
                    continue;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                send_batch(tx, shared, &mut pending);
                return;
            }
        }
        window = None;
        if !send_batch(tx, shared, &mut pending) {
            return;
        }
    }
}

/// Adds the relevant paths of one backend event to `pending` and returns whether there
/// were any; records created or removed directories that change what can be watched
/// ([`Shared::is_structural`]) in [`Shared::recreated`] (they are changes too); logs
/// errors.
fn collect(
    result: notify::Result<Event>, shared: &Mutex<Shared>,
    pending: &mut BTreeSet<PathBuf>,
) -> bool {
    let event = match result {
        Ok(event) => event,
        Err(err) => {
            tracing::warn!("file watcher: {err}");
            return false;
        }
    };
    let mut shared = lock(shared);
    if event.need_rescan() {
        tracing::debug!(
            "file watcher: events were lost; treating every file as changed"
        );
        pending.extend(shared.filter.files.iter().cloned());
        return !shared.filter.files.is_empty();
    }
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    let structural = matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(notify::event::ModifyKind::Name(_))
    );
    let mut relevant = false;
    for path in event.paths.iter().map(|path| canonical(path)) {
        let recreated = structural && shared.is_structural(&path);
        if !recreated
            && ((!structural && path.is_dir()) || !shared.filter.matches(&path))
        {
            // A directory reported as modified (Windows does that when an entry in it is
            // written, renamed or removed) is no change by itself: the entry has its own
            // event. A created, removed or renamed directory still counts.
            continue;
        }
        if changed_before(&path, shared.since) {
            // FSEvents reports changes made shortly before its stream was created, and it
            // creates a stream on every watch (module docs): the build that the last batch
            // started (or the first build) read the path after this change, and a
            // directory that existed before it was not recreated since.
            tracing::debug!(
                "file watcher: {} last changed before the last batch; skipped",
                path.display()
            );
            continue;
        }
        if recreated {
            shared.recreated.insert(path.clone());
        }
        pending.insert(path);
        relevant = true;
    }
    relevant
}

/// Sends `pending` (emptied) as one batch, classified with the current theme
/// directories, and records the time in [`Shared::since`]. Returns false when nobody
/// receives any more. Nothing is sent for no paths.
fn send_batch(
    tx: &Sender<ChangeBatch>, shared: &Mutex<Shared>,
    pending: &mut BTreeSet<PathBuf>,
) -> bool {
    if pending.is_empty() {
        return true;
    }
    let paths: Vec<PathBuf> = std::mem::take(pending).into_iter().collect();
    let kind = {
        let mut shared = lock(shared);
        shared.since = SystemTime::now();
        classify_paths(
            &paths, &shared.filter.theme_dirs, &shared.filter.pages_paths,
        )
    };
    tracing::debug!(?kind, paths = paths.len(), "change batch");
    tx.send(ChangeBatch { paths, kind }).is_ok()
}

/// Status callbacks begin with Built and Watching, then Changed and Built per rebuild.
#[derive(Debug)]
pub enum WatchStatus<'a> {
    /// All preparation, target and diagnostic results of one completed build.
    Built(&'a BuildReport),
    /// The initial build has finished and these inputs are now watched.
    Watching(&'a WatchSet),
    /// A batch that affects at least one requested target is about to rebuild.
    Changed(&'a ChangeBatch),
}

/// Builds once, then selectively rebuilds until the callback breaks or watching stops.
/// Content failures are reported through Built; only starting the watcher returns an error.
pub fn run_build_watch_with(
    opts: &BuildOptions, targets: &[Target],
    mut report: impl FnMut(WatchStatus<'_>) -> ControlFlow<()>,
) -> Result<()> {
    let mut state = BuildLoop::new(opts, targets);
    let (tx, rx) = mpsc::channel();
    let mut watcher = Watcher::start(&watch_set_before_build(&state.opts), tx)?;
    let all_targets = state.targets.clone();
    let ControlFlow::Continue(mut set) =
        state.build(&all_targets, &mut watcher, &mut report)
    else {
        return Ok(());
    };
    if report(WatchStatus::Watching(&set)).is_break() {
        return Ok(());
    }
    while let Some(batch) = next_batch(&rx, &state.outputs, &set) {
        let targets = targets_for_batch(
            batch.kind, state.config.pdf.engine, &state.targets,
        );
        if targets.is_empty() {
            continue;
        }
        if report(WatchStatus::Changed(&batch)).is_break() {
            return Ok(());
        }
        let ControlFlow::Continue(next) =
            state.build(&targets, &mut watcher, &mut report)
        else {
            return Ok(());
        };
        set = next;
    }
    Ok(())
}

struct BuildLoop {
    opts: BuildOptions,
    targets: Vec<Target>,
    worlds: WorldSlot,
    config: Config,
    last_good: Option<WatchSet>,
    accessed: AccessHistory,
    outputs: Outputs,
}

impl BuildLoop {
    fn new(opts: &BuildOptions, requested: &[Target]) -> Self {
        let mut opts = opts.clone();
        opts.mode = Mode::Build;
        let mut targets = Vec::new();
        for target in requested {
            if !targets.contains(target) {
                targets.push(*target);
            }
        }
        let config = pipeline::load_config(&opts)
            .map(|(config, _)| config)
            .unwrap_or_else(|_| {
                let mut config = Config::default();
                config.apply_cli(&opts.cli);
                config
            });
        Self {
            opts,
            targets,
            worlds: WorldSlot::new(),
            config,
            last_good: None,
            accessed: AccessHistory::default(),
            outputs: Outputs::default(),
        }
    }

    fn build(
        &mut self, targets: &[Target], watcher: &mut Watcher,
        report: &mut impl FnMut(WatchStatus<'_>) -> ControlFlow<()>,
    ) -> ControlFlow<(), WatchSet> {
        let build = pipeline::build_targets_with_count(
            &self.opts,
            targets,
            &mut self.worlds,
            self.targets.len(),
        );
        typst::comemo::evict(10);
        self.outputs.remember(&build);
        let set = self.observe(&build, targets);
        if let Err(error) = watcher.update(&set) {
            tracing::warn!("{error}");
        }
        report(WatchStatus::Built(&build))?;
        ControlFlow::Continue(set)
    }

    fn observe(&mut self, build: &BuildReport, targets: &[Target]) -> WatchSet {
        match &build.prepared {
            Ok(prepared) => {
                self.config = prepared.config.clone();
                let current = self
                    .worlds
                    .current()
                    .map(|world| {
                        accessed_files(
                            world,
                            &prepared.config,
                            &prepared.document.source.base_dir,
                        )
                    })
                    .unwrap_or_default();
                let accessed = self
                    .accessed
                    .update(current, targets.contains(&Target::Pdf));
                let set = watch_set(
                    &pipeline::watch_inputs(prepared),
                    &accessed.into_iter().collect::<Vec<_>>(),
                    Some(&self.targets),
                );
                self.last_good = Some(set.clone());
                set
            }
            Err(failure) => {
                let mut set = watch_set_before_build(&self.opts);
                if let Some(good) = &self.last_good {
                    set.merge(good);
                }
                set.merge(&WatchSet::new(failure_paths(failure), []));
                set
            }
        }
    }
}

#[derive(Default)]
struct AccessHistory {
    pdf: BTreeSet<PathBuf>,
}

impl AccessHistory {
    fn update(
        &mut self, mut current: BTreeSet<PathBuf>, ran_pdf: bool,
    ) -> BTreeSet<PathBuf> {
        if ran_pdf {
            self.pdf = current.clone();
        } else {
            current.extend(self.pdf.iter().cloned());
        }
        current
    }
}

fn accessed_files(
    world: &WriterWorld, config: &Config, root: &Path,
) -> BTreeSet<PathBuf> {
    let roots = package_roots(config, root);
    outside_packages(world.accessed(), &roots)
}

fn outside_packages(
    paths: BTreeSet<PathBuf>, roots: &[PathBuf],
) -> BTreeSet<PathBuf> {
    paths
        .into_iter()
        .map(|path| canonical(&path))
        .filter(|path| !roots.iter().any(|root| path.starts_with(root)))
        .collect()
}

fn package_roots(config: &Config, root: &Path) -> Vec<PathBuf> {
    let system = [FsPackages::system_data(), FsPackages::system_cache()]
        .into_iter()
        .flatten()
        .map(|packages| packages.path().to_path_buf());
    canonical_set(
        config
            .typst
            .package_paths
            .iter()
            .map(|path| root.join(path))
            .chain(system),
    )
    .into_iter()
    .collect()
}

fn failure_paths(failure: &BuildFailure) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Error::MissingAsset { path, .. } = failure.error.as_ref() {
        paths.push(path.clone());
    }
    paths.extend(failure.error.path().map(Path::to_path_buf));
    paths.extend(failure.error.span().and_then(|span| span.file));
    for diagnostic in &failure.diagnostics {
        paths.extend(diagnostic.path.iter().cloned());
        paths.extend(
            diagnostic.span.as_ref().and_then(|span| span.file.clone()),
        );
        if let Some(Error::MissingAsset { path, .. }) =
            pipeline::missing_asset_as_error(diagnostic)
        {
            paths.push(path);
        }
    }
    paths.retain(|path| !path.as_os_str().is_empty());
    paths
}

fn targets_for_batch(
    kind: ChangeKind, engine: PdfEngine, targets: &[Target],
) -> Vec<Target> {
    targets
        .iter()
        .copied()
        .filter(|target| match kind {
            ChangeKind::Content => true,
            ChangeKind::Css => {
                *target == Target::Html
                    || (*target == Target::Pdf && engine == PdfEngine::Chrome)
            }
            ChangeKind::Pages => {
                *target == Target::Typst
                    || (*target == Target::Pdf && engine == PdfEngine::Typst)
            }
        })
        .collect()
}

#[derive(Default)]
struct Outputs(Vec<(Target, Vec<PathBuf>)>);

impl Outputs {
    fn remember(&mut self, report: &BuildReport) {
        for target in &report.targets {
            let Ok(paths) = &target.written else { continue };
            let paths =
                canonical_set(paths.iter().cloned()).into_iter().collect();
            if let Some((_, previous)) =
                self.0.iter_mut().find(|(kind, _)| *kind == target.target)
            {
                *previous = paths;
            } else {
                self.0.push((target.target, paths));
            }
        }
    }

    fn contains(&self, path: &Path, roots: &[&PathBuf]) -> bool {
        self.0.iter().flat_map(|(_, paths)| paths).any(|out| {
            path.starts_with(out)
                || (out.starts_with(path)
                    && roots.iter().any(|root| {
                        path != root.as_path() && path.starts_with(root)
                    }))
        })
    }
}

fn next_batch(
    rx: &Receiver<ChangeBatch>, outputs: &Outputs, set: &WatchSet,
) -> Option<ChangeBatch> {
    let roots: Vec<_> =
        set.dirs_recursive.iter().chain(&set.theme_dirs).collect();
    loop {
        let mut paths: BTreeSet<_> =
            rx.recv().ok()?.paths.into_iter().collect();
        while let Ok(more) = rx.try_recv() {
            paths.extend(more.paths);
        }
        paths.retain(|path| !outputs.contains(path, &roots));
        if !paths.is_empty() {
            let paths = paths.into_iter().collect::<Vec<_>>();
            return Some(ChangeBatch { kind: classify(&paths, set), paths });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Diagnostic;
    use crate::ir::SourceSpan;
    use crate::pipeline::TargetResult;
    use notify::event::{AccessKind, DataChange, ModifyKind, RemoveKind};

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect::<Vec<_>>()
    }

    fn shared(set: &WatchSet) -> Mutex<Shared> {
        Mutex::new(Shared {
            filter: Filter::new(set),
            missing: BTreeSet::new(),
            recreated: BTreeSet::new(),
            since: std::time::UNIX_EPOCH,
        })
    }

    fn modified(path: &Path) -> notify::Result<Event> {
        Ok(Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(path.to_path_buf()))
    }

    #[test]
    fn classification_includes_pages_mixtures_and_empty_batches() {
        let set = WatchSet {
            theme_dirs: paths(&["/p/child", "/p/parent"]),
            pages_paths: paths(&["/p/template.typ", "/p/writer-doc"]),
            ..Default::default()
        };
        for name in [
            "/p/x.css", "/p/x.SCSS", "/p/x.Sass", "/p/child/styles/x.toml",
            "/p/parent/assets/logo.png",
        ] {
            assert_eq!(
                classify(&paths(&[name]), &set),
                ChangeKind::Css,
                "{name}"
            );
        }
        for name in
            ["/p/template.typ", "/p/writer-doc", "/p/writer-doc/lib.typ"]
        {
            assert_eq!(
                classify(&paths(&[name]), &set),
                ChangeKind::Pages,
                "{name}"
            );
        }
        for names in [
            vec![],
            vec!["/p/template.typ", "/p/x.css"],
            vec!["/p/source.md"],
            vec!["/p/child/theme.toml"],
            vec!["/p/child/templates/document.html"],
            vec!["/p/assets/plot.png"],
            vec!["/p/writer-doc-other/lib.typ"],
        ] {
            assert_eq!(
                classify(&paths(&names), &set),
                ChangeKind::Content,
                "{names:?}"
            );
        }
        let overlap = WatchSet { pages_paths: paths(&["/p/x.css"]), ..set };
        assert_eq!(classify(&paths(&["/p/x.css"]), &overlap), ChangeKind::Css);
    }

    #[test]
    fn change_kind_serializes_lowercase() {
        for (kind, source) in [
            (ChangeKind::Css, "\"css\""),
            (ChangeKind::Pages, "\"pages\""),
            (ChangeKind::Content, "\"content\""),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), source);
            assert_eq!(
                serde_json::from_str::<ChangeKind>(source).unwrap(),
                kind
            );
        }
    }

    #[test]
    fn ignore_list_keeps_dist_and_target() {
        for name in [
            ".git", ".git/index", "assets/lib/.git/HEAD", "styles/x.css~",
            "styles/.#x.css", "styles/.x.css.swp", "styles/.x.css.swx",
            "styles/.x.css.swo", "styles/.x.css.swa", "styles/#x.css#",
            "#theme.toml#", "templates/4913", ".DS_Store",
        ] {
            assert!(is_ignored(Path::new(name)), "{name}");
        }
        for name in [
            "styles/x.css", "theme.toml", "target/debug/x.css",
            "dist/document.html", "scripts/dist/lib.js", "styles/x.swo",
            "styles/.x.css.swq", "styles/.swo", "assets/#", "",
        ] {
            assert!(!is_ignored(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn canonical_resolves_missing_files_and_sorts_watch_lists() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        assert_eq!(
            canonical(&directory.path().join("a/../missing/x.png")),
            root.join("missing/x.png")
        );
        let set = WatchSet::new(
            [
                root.join("a.md"),
                root.join("fig/b.png"),
                root.join("fig/a.png"),
                root.join("a.md"),
            ],
            [root.join("theme")],
        );
        assert_eq!(
            set.files,
            [root.join("a.md"), root.join("fig/a.png"), root.join("fig/b.png")]
        );
        assert_eq!(set.dirs_flat, [root.clone(), root.join("fig")]);
        assert_eq!(set.dirs_recursive, [root.join("theme")]);
        assert_eq!(set.theme_dirs, set.dirs_recursive);
        assert!(set.pages_paths.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn missing_paths_preserve_existing_symlink_parent_semantics() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        std::fs::create_dir_all(root.join("real/nested")).unwrap();
        std::os::unix::fs::symlink(root.join("real/nested"), root.join("link"))
            .unwrap();
        assert_eq!(
            canonical(&root.join("link/../missing.png")),
            root.join("real/missing.png")
        );
        assert_eq!(
            canonical(&root.join("link/../missing/file.png")),
            root.join("real/missing/file.png")
        );
    }

    #[test]
    fn merge_unions_and_canonicalizes_every_list() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        let mut first =
            WatchSet::new([root.join("a.md")], [root.join("theme")]);
        first.pages_paths.push(root.join("template.typ"));
        let second = WatchSet {
            files: vec![root.join("sub/../a.md"), root.join("fig/a.png")],
            dirs_flat: vec![root.join("fig")],
            dirs_recursive: vec![root.join("writer-doc")],
            theme_dirs: vec![root.join("theme")],
            pages_paths: vec![root.join("writer-doc")],
        };
        first.merge(&second);
        assert_eq!(first.files, [root.join("a.md"), root.join("fig/a.png")]);
        assert_eq!(
            first.dirs_recursive,
            [root.join("theme"), root.join("writer-doc")]
        );
        assert_eq!(
            first.pages_paths,
            [root.join("template.typ"), root.join("writer-doc")]
        );
    }

    #[test]
    fn filter_collapses_recursive_roots_and_keeps_explicit_files() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        let set = WatchSet {
            files: vec![root.join("a.md"), root.join("theme/.git/HEAD")],
            dirs_recursive: vec![root.join("theme"), root.join("theme/parent")],
            dirs_flat: vec![root.join("img")],
            theme_dirs: vec![root.join("theme")],
            pages_paths: vec![root.join("template.typ")],
        };
        let filter = Filter::new(&set);
        assert_eq!(
            filter.targets,
            [
                (root.clone(), RecursiveMode::NonRecursive),
                (root.join("img"), RecursiveMode::NonRecursive),
                (root.join("theme"), RecursiveMode::Recursive),
            ]
            .into_iter()
            .collect::<BTreeMap<_, _>>()
        );
        assert!(filter.matches(&root.join("theme/.git/HEAD")));
        assert!(!filter.matches(&root.join("theme/.git/index")));
        assert!(!filter.matches(&root.join("img/other.png")));
        assert_eq!(filter.pages_paths, [root.join("template.typ")]);
    }

    #[test]
    fn missing_targets_use_existing_ancestors_and_structural_events() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        std::fs::create_dir_all(root.join("recursive")).unwrap();
        std::fs::create_dir_all(root.join("other")).unwrap();
        let targets = [
            (root.clone(), RecursiveMode::NonRecursive),
            (root.join("a/b"), RecursiveMode::NonRecursive),
            (root.join("recursive"), RecursiveMode::Recursive),
            (root.join("recursive/x/y"), RecursiveMode::NonRecursive),
            (root.join("other/c/d"), RecursiveMode::NonRecursive),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        let (wanted, missing) = plan_watches(&targets);
        assert_eq!(
            wanted,
            [
                (root.clone(), RecursiveMode::NonRecursive),
                (root.join("other"), RecursiveMode::NonRecursive),
                (root.join("recursive"), RecursiveMode::Recursive),
            ]
            .into_iter()
            .collect::<BTreeMap<_, _>>()
        );
        let shared = Shared {
            filter: Filter { targets, ..Default::default() },
            missing,
            recreated: BTreeSet::new(),
            since: SystemTime::now(),
        };
        for name in ["a", "a/b", "other", "other/c", "recursive"] {
            assert!(shared.is_structural(&root.join(name)), "{name}");
        }
        for name in ["a/b/c", "other/x", "recursive/z"] {
            assert!(!shared.is_structural(&root.join(name)), "{name}");
        }
    }

    #[test]
    fn guard_watches_are_only_windows_parents_not_already_covered() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        let wanted = [(root.join("theme"), RecursiveMode::Recursive)]
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let guards = guard_watches(&wanted);
        if cfg!(windows) {
            assert_eq!(
                guards,
                [root.clone()].into_iter().collect::<BTreeSet<_>>()
            );
        } else {
            assert!(guards.is_empty());
        }
        let wanted = [(root, RecursiveMode::Recursive)]
            .into_iter()
            .chain(wanted)
            .collect::<BTreeMap<_, _>>();
        assert!(
            guard_watches(&wanted).iter().all(|path| !covers(&wanted, path))
        );
    }

    #[test]
    fn access_events_are_ignored_but_mutations_and_rescans_count() {
        let directory = tempfile::tempdir().unwrap();
        let path = canonical(&directory.path().join("a.md"));
        let set = WatchSet::new([path.clone()], []);
        let shared = shared(&set);
        let mut pending = BTreeSet::new();
        let access = Event::new(EventKind::Access(AccessKind::Any))
            .add_path(path.clone());
        assert!(!collect(Ok(access), &shared, &mut pending));
        assert!(pending.is_empty());
        assert!(collect(modified(&path), &shared, &mut pending));
        assert_eq!(
            pending,
            [path.clone()].into_iter().collect::<BTreeSet<_>>()
        );
        pending.clear();
        let mut event = Event::new(EventKind::Other);
        event.attrs.set_flag(notify::event::Flag::Rescan);
        assert!(collect(Ok(event), &shared, &mut pending));
        assert_eq!(pending, [path].into_iter().collect::<BTreeSet<_>>());
    }

    #[test]
    fn changes_before_the_last_batch_are_skipped_only_where_ctime_is_reliable()
    {
        let directory = tempfile::tempdir().unwrap();
        let path = canonical(&directory.path().join("a.md"));
        std::fs::write(&path, "one").unwrap();
        let shared = shared(&WatchSet::new([path.clone()], []));
        lock(&shared).since = SystemTime::now();
        let mut pending = BTreeSet::new();
        if changed_before(&path, SystemTime::now()) {
            assert!(!collect(modified(&path), &shared, &mut pending));
        }
        std::fs::write(&path, "two").unwrap();
        assert!(collect(modified(&path), &shared, &mut pending));
        std::fs::remove_file(&path).unwrap();
        let removed = Event::new(EventKind::Remove(RemoveKind::File))
            .add_path(path.clone());
        assert!(collect(Ok(removed), &shared, &mut pending));
        assert!(pending.contains(&path));
    }

    #[test]
    fn coalescer_flushes_pending_work_when_the_backend_stops() {
        let directory = tempfile::tempdir().unwrap();
        let path = canonical(&directory.path().join("a.md"));
        let state = Arc::new(shared(&WatchSet::new([path.clone()], [])));
        let (raw_tx, raw_rx) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            coalesce(
                &raw_rx,
                &tx,
                &state,
                Debounce {
                    quiet: Duration::from_secs(30),
                    max_wait: Duration::from_secs(30),
                },
            )
        });
        raw_tx.send(modified(&path)).unwrap();
        drop(raw_tx);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap().paths,
            [path]
        );
        thread.join().unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_err());
    }

    #[test]
    fn max_wait_bounds_continuous_relevant_traffic() {
        let debounce = Debounce {
            quiet: Duration::from_millis(80),
            max_wait: Duration::from_millis(160),
        };
        assert_eq!(batch_during_traffic(true, debounce).paths.len(), 1);
    }

    #[test]
    fn unrelated_traffic_does_not_delay_the_quiet_window() {
        let debounce = Debounce {
            quiet: Duration::from_millis(80),
            max_wait: Duration::from_secs(10),
        };
        assert_eq!(batch_during_traffic(false, debounce).paths.len(), 1);
    }

    fn batch_during_traffic(relevant: bool, debounce: Debounce) -> ChangeBatch {
        use std::sync::atomic::{AtomicBool, Ordering};
        let directory = tempfile::tempdir().unwrap();
        let path = canonical(&directory.path().join("a.md"));
        let state = Arc::new(shared(&WatchSet::new([path.clone()], [])));
        let (raw_tx, raw_rx) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            coalesce(&raw_rx, &tx, &state, debounce)
        });
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        raw_tx.send(modified(&path)).unwrap();
        let changed = if relevant {
            path
        } else {
            directory.path().join("other.md")
        };
        let producer = std::thread::spawn(move || {
            let started = Instant::now();
            while !stopped.load(Ordering::Relaxed)
                && started.elapsed() < Duration::from_secs(4)
            {
                if raw_tx.send(modified(&changed)).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let first = rx.recv_timeout(Duration::from_secs(2));
        stop.store(true, Ordering::Relaxed);
        producer.join().unwrap();
        thread.join().unwrap();
        first.expect("continuous traffic must not postpone the batch")
    }

    #[test]
    fn pdf_access_history_survives_partial_builds_and_replaces_on_the_next_pdf()
    {
        let mut history = AccessHistory::default();
        let pdf =
            paths(&["/p/chapter.typ"]).into_iter().collect::<BTreeSet<_>>();
        assert_eq!(history.update(pdf.clone(), true), pdf);
        let css = paths(&["/p/web.tex"]).into_iter().collect::<BTreeSet<_>>();
        assert_eq!(
            history.update(css, false),
            paths(&["/p/chapter.typ", "/p/web.tex"])
                .into_iter()
                .collect::<BTreeSet<_>>()
        );
        let fresh = paths(&["/p/new.typ"]).into_iter().collect::<BTreeSet<_>>();
        assert_eq!(history.update(fresh.clone(), true), fresh);
        assert_eq!(history.update(BTreeSet::new(), false), fresh);
    }

    #[test]
    fn package_reads_are_removed_with_path_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        let input = [
            root.join("packages/p/lib.typ"),
            root.join("packages-other/a.typ"),
            root.join("a.typ"),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        let filtered = outside_packages(input, &[root.join("packages")]);
        assert_eq!(
            filtered,
            [root.join("a.typ"), root.join("packages-other/a.typ")]
                .into_iter()
                .collect::<BTreeSet<_>>()
        );
        let mut config = Config::default();
        config.typst.package_paths.push(root.join("packages"));
        assert!(package_roots(&config, &root).contains(&root.join("packages")));
        config.typst.package_paths = vec!["relative-packages".into()];
        assert!(
            package_roots(&config, &root)
                .contains(&root.join("relative-packages"))
        );
    }

    #[test]
    fn failure_paths_keep_resource_locations_and_source_context() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        let missing = root.join("missing.png");
        let second = root.join("second.png");
        let source = root.join("source.md");
        let included = root.join("include.md");
        let mut diagnostic = Diagnostic::warning(
            "missing-asset",
            format!(
                "missing local image `second.png` (looked for {})",
                second.display(),
            ),
        );
        diagnostic.path = Some(source.clone());
        diagnostic.span = Some(SourceSpan {
            start_line: 3,
            end_line: 3,
            cell: None,
            file: Some(included.clone()),
        });
        let failure = BuildFailure {
            error: Box::new(Error::MissingAsset {
                path: missing.clone(),
                span: None,
            }),
            diagnostics: vec![diagnostic].into(),
        };
        assert_eq!(
            canonical_set(failure_paths(&failure)),
            [missing, second, source, included]
                .into_iter()
                .collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn selective_targets_follow_the_effective_engine_and_request_order() {
        let all = [
            Target::Docx,
            Target::Pdf,
            Target::Html,
            Target::Typst,
            Target::Latex,
        ];
        assert_eq!(
            targets_for_batch(ChangeKind::Css, PdfEngine::Typst, &all),
            [Target::Html]
        );
        assert_eq!(
            targets_for_batch(ChangeKind::Css, PdfEngine::Chrome, &all),
            [Target::Pdf, Target::Html]
        );
        assert_eq!(
            targets_for_batch(ChangeKind::Pages, PdfEngine::Typst, &all),
            [Target::Pdf, Target::Typst]
        );
        assert_eq!(
            targets_for_batch(ChangeKind::Pages, PdfEngine::Chrome, &all),
            [Target::Typst]
        );
        assert_eq!(
            targets_for_batch(ChangeKind::Content, PdfEngine::Typst, &all),
            all
        );
        assert!(
            targets_for_batch(
                ChangeKind::Pages,
                PdfEngine::Typst,
                &[Target::Html]
            )
            .is_empty()
        );
    }

    fn report(targets: Vec<TargetResult>) -> BuildReport {
        BuildReport {
            prepared: Err(Error::Other("unused test preparation".into()).into()),
            targets,
            diagnostics: Diagnostics::new(),
        }
    }

    #[test]
    fn outputs_remember_other_targets_and_keep_last_success_on_failure() {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        let mut outputs = Outputs::default();
        outputs.remember(&report(vec![
            TargetResult {
                target: Target::Html,
                written: Ok(vec![root.join("a.html")]),
            },
            TargetResult {
                target: Target::Docx,
                written: Ok(vec![root.join("a.docx")]),
            },
        ]));
        outputs.remember(&report(vec![TargetResult {
            target: Target::Html,
            written: Ok(vec![root.join("b.html")]),
        }]));
        outputs.remember(&report(vec![TargetResult {
            target: Target::Docx,
            written: Err(Error::Other("failed".into())),
        }]));
        assert!(outputs.contains(&root.join("a.docx"), &[]));
        assert!(outputs.contains(&root.join("b.html"), &[]));
        assert!(!outputs.contains(&root.join("a.html"), &[]));
    }

    #[test]
    fn next_batch_merges_reclassifies_and_suppresses_outputs_and_created_ancestors()
     {
        let directory = tempfile::tempdir().unwrap();
        let root = canonical(directory.path());
        let theme = root.join("theme");
        let out = theme.join("build/sub/out.html");
        let outputs = Outputs(vec![(Target::Html, vec![out.clone()])]);
        let set = WatchSet::new([root.join("a.md")], [theme.clone()]);
        let (tx, rx) = mpsc::channel();
        for names in [
            vec!["theme/build", "theme/build/sub", "theme/build/sub/out.html"],
            vec!["theme/styles/a.css"],
            vec!["theme/styles/b.css"],
        ] {
            tx.send(ChangeBatch {
                paths: names
                    .iter()
                    .map(|name| root.join(name))
                    .collect::<Vec<_>>(),
                kind: ChangeKind::Content,
            })
            .unwrap();
        }
        let next = next_batch(&rx, &outputs, &set).unwrap();
        assert_eq!(next.kind, ChangeKind::Css);
        assert_eq!(
            next.paths,
            [theme.join("styles/a.css"), theme.join("styles/b.css")]
        );
        assert!(!outputs.contains(&theme, &[&theme]));
        assert!(!outputs.contains(&root, &[&theme]));
        tx.send(ChangeBatch { paths: vec![out], kind: ChangeKind::Content })
            .unwrap();
        drop(tx);
        assert_eq!(next_batch(&rx, &outputs, &set), None);
    }

    #[test]
    fn watcher_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Watcher>();
    }
}
