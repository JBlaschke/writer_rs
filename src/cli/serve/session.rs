//! One retained serve session: independent stages, source baselines and watches.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Instant;

use axum::Router;
use regex::Regex;
use tokio::sync::broadcast;

use crate::cli::{GlobalArgs, report, report_diagnostics};
use crate::config::{Config, ServeConfig, ServeView};
use crate::error::{Diagnostic, Diagnostics, Error, Level};
use crate::ir::{SourceKind, detect_kind};
use crate::pipeline::{
    self, BuildFailure, BuildOptions, Mode, Prepared, WebPage, WorldSlot,
};
use crate::serve::events::{BuildEvent, Changed, DiagnosticJson, ErrorPayload};
use crate::serve::pages::{PageSet, PagesBuilder, PagesInput};
use crate::serve::server::{
    self, Outcome, ServeState, Shared, ViewOutcome, WebBuild,
};
use crate::serve::watch::{
    ChangeBatch, ChangeKind, WatchSet, Watcher, accessed_files, canonical,
    classify, failure_paths, watch_set, watch_set_before_build,
};
use crate::typst::world::WriterWorld;

type BuildResult<T> = std::result::Result<T, BuildFailure>;

/// Moves into one blocking build at a time; request handlers only share served state.
pub(super) struct Session {
    opts: BuildOptions,
    global: GlobalArgs,
    kind: SourceKind,
    listening: ServeConfig,
    restart_note: Option<String>,
    state: Shared,
    events: broadcast::Sender<BuildEvent>,
    watcher: Watcher,
    set: WatchSet,
    last_good: Option<WatchSet>,
    worlds: WorldSlot,
    world_config: Option<(Config, PathBuf)>,
    pages: PagesBuilder,
    accessed_web: BTreeSet<PathBuf>,
    accessed_pages: BTreeSet<PathBuf>,
    sources: Sources,
    versions: BTreeSet<String>,
}

impl Session {
    /// Starts with no built output; the caller has already started the watcher.
    pub(super) fn new(
        opts: BuildOptions, global: GlobalArgs, watcher: Watcher,
        listening: &ServeConfig,
    ) -> Self {
        let kind = opts
            .format
            .or_else(|| detect_kind(&opts.input).ok())
            .unwrap_or(SourceKind::Markdown);
        let default_view = listening.view.unwrap_or(match kind {
            SourceKind::Typst => ServeView::Pages,
            _ => ServeView::Web,
        });
        let state = ServeState::new(default_view, input_title(&opts.input));
        let set = watch_set_before_build(&opts);
        Self {
            opts,
            global,
            kind,
            listening: listening.clone(),
            restart_note: None,
            state: Arc::new(RwLock::new(state)),
            events: server::event_channel(),
            watcher,
            set,
            last_good: None,
            worlds: WorldSlot::new(),
            world_config: None,
            pages: PagesBuilder::new(),
            accessed_web: BTreeSet::new(),
            accessed_pages: BTreeSet::new(),
            sources: Sources::default(),
            versions: BTreeSet::new(),
        }
    }

    /// Request handlers retain only state/events, not the mutable build session.
    pub(super) fn router(&self) -> Router {
        server::router(self.state.clone(), self.events.clone())
    }

    /// An empty queue denotes build1; later queues combine into one selective attempt.
    pub(super) fn rebuild(&mut self, queued: Vec<ChangeBatch>) {
        self.rebuild_with(queued, &mut LiveStages);
    }

    fn rebuild_with(
        &mut self, queued: Vec<ChangeBatch>, stages: &mut impl BuildStages,
    ) {
        let batch = merge_batches(queued, &self.set);
        tracing::info!(kind = kind_name(batch.kind), paths = ?batch.paths, "changed");
        let started = Instant::now();
        let build_id = server::next_build_id(&self.state);
        let mut opts = self.opts.clone();
        opts.mode = Mode::Serve { build_id };
        let position = (batch.kind == ChangeKind::Content)
            .then(|| self.sources.changed(&batch.paths))
            .flatten();
        let attempt = self.render_views(&opts, batch.kind, build_id, stages);
        typst::comemo::evict(10);
        self.finish(&opts, batch.kind, started, position, attempt);
    }

    fn render_views(
        &mut self, opts: &BuildOptions, kind: ChangeKind, build_id: u64,
        stages: &mut impl BuildStages,
    ) -> Attempt {
        let prepare =
            if kind == ChangeKind::Pages && self.kind == SourceKind::Typst {
                None
            } else {
                let captured = capture(|| stages.prepare(opts));
                if captured.panicked {
                    self.discard_world();
                }
                Some(captured.result)
            };
        let prepared = prepare.as_ref().and_then(|result| result.as_ref().ok());
        let failed_prepare =
            prepare.as_ref().and_then(|result| result.as_ref().err());
        let wants_web = kind != ChangeKind::Pages;
        let wants_pages = kind != ChangeKind::Css;
        let can_build = prepared.is_some() || self.kind == SourceKind::Typst;
        let mut web = ViewStage::skipped();
        let mut pages = ViewStage::skipped();
        if let Some(failure) = failed_prepare {
            let payload = ErrorPayload::for_failure(failure, &opts.input);
            if wants_web {
                web = ViewStage::blocked(payload.clone());
            }
            if wants_pages && self.kind != SourceKind::Typst {
                pages = ViewStage::blocked(payload);
            }
        }
        let world_failure = if can_build
            && (wants_pages || (wants_web && prepared.is_some()))
        {
            self.ready_world(prepared, opts).err()
        } else {
            None
        };
        if let Some(failure) = &world_failure {
            let payload = ErrorPayload::for_failure(failure, &opts.input);
            if wants_web && prepared.is_some() {
                web = ViewStage::blocked(payload.clone());
            }
            if wants_pages && can_build {
                pages = ViewStage::blocked(payload);
            }
        } else {
            if wants_web && let Some(prepared) = prepared {
                web = self.render_web(prepared, opts, stages);
            }
            if wants_pages && can_build {
                pages = self.render_pages(prepared, opts, build_id, stages);
            }
        }
        Attempt { prepare, world_failure, web, pages }
    }

    fn render_web(
        &mut self, prepared: &Prepared, opts: &BuildOptions,
        stages: &mut impl BuildStages,
    ) -> ViewStage<WebBuild> {
        let world = self.worlds.current().expect("World was acquired");
        let captured = capture(|| stages.web(prepared, opts, world));
        self.accessed_web = self.accessed();
        if captured.panicked {
            self.discard_world();
        }
        ViewStage::from_result(captured.result, &opts.input, |page| {
            let build = WebBuild {
                html: page.html,
                theme_css: page.theme_css,
                assets: page.assets,
                theme: page.theme,
            };
            (build, page.diagnostics)
        })
    }

    fn render_pages(
        &mut self, prepared: Option<&Prepared>, opts: &BuildOptions,
        build_id: u64, stages: &mut impl BuildStages,
    ) -> ViewStage<Arc<PageSet>> {
        // A preceding web panic may have discarded the World. Pages can still
        // proceed independently, and future attempts retain its fresh World.
        if self.worlds.current().is_none()
            && let Err(failure) = self.ready_world(prepared, opts)
        {
            return ViewStage::failed(failure, &opts.input);
        }
        let world = self.worlds.current().expect("World was acquired");
        let input = match (self.kind, prepared) {
            (SourceKind::Typst, _) => {
                PagesInput::TypstFile { path: &opts.input }
            }
            (_, Some(prepared)) => PagesInput::Generated { prepared, opts },
            _ => unreachable!("a generated pages stage needs preparation"),
        };
        let captured =
            capture(|| stages.pages(&mut self.pages, world, input, build_id));
        self.accessed_pages = self.accessed();
        if captured.panicked {
            self.discard_world();
        }
        ViewStage::from_result(
            captured.result,
            &opts.input,
            std::convert::identity,
        )
    }

    fn finish(
        &mut self, opts: &BuildOptions, mut kind: ChangeKind, started: Instant,
        position: Option<Position>, attempt: Attempt,
    ) {
        let Attempt { prepare, world_failure, web, pages } = attempt;
        let prepared = prepare.as_ref().and_then(|result| result.as_ref().ok());
        if let ViewOutcome::Built(build) = &web.outcome {
            let versions = document_versions(&build.html);
            if kind == ChangeKind::Css && versions != self.versions {
                kind = ChangeKind::Content;
            }
            self.versions = versions;
        }
        let mut messages =
            prepare_messages(prepare.as_ref(), web.failure.as_ref());
        if let Some(prepared) = prepared
            && let Some(notice) = self.restart_diagnostic(prepared)
        {
            messages.push(notice);
        }
        let changed = prepared.and_then(|prepared| {
            position.and_then(|position| position.in_document(prepared))
        });
        let title = prepared.map(|prepared| {
            prepared
                .document
                .meta
                .title
                .clone()
                .unwrap_or_else(|| input_title(&opts.input))
        });
        let duration_ms =
            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let web_failed = matches!(web.outcome, ViewOutcome::Failed(_));
        let pages_failed = matches!(pages.outcome, ViewOutcome::Failed(_));
        let event = server::apply_outcome(
            &self.state,
            Outcome {
                kind,
                duration_ms,
                changed,
                title,
                web: web.outcome,
                pages: pages.outcome,
                prepare_warnings: messages.warnings,
                web_warnings: web.warnings,
                pages_warnings: pages.warnings,
            },
        );
        let failed_prepare =
            prepare.as_ref().and_then(|result| result.as_ref().err());
        let failures = failed_prepare
            .into_iter()
            .chain(world_failure.as_ref())
            .chain(web.failure.as_ref())
            .chain(pages.failure.as_ref())
            .collect::<Vec<_>>();
        self.update_watches(prepared, prepare.is_some(), &failures);
        if let Some(prepared) = prepared {
            self.sources.observe(prepared);
        }
        if !self.global.quiet {
            let line =
                rebuilt_line(duration_ms, kind, web_failed, pages_failed);
            let _ = writeln!(std::io::stderr().lock(), "{line}");
        }
        print_reports(
            messages.diagnostics.as_ref(),
            &web.diagnostics,
            &pages.diagnostics,
            &failures,
            &opts.input,
            &self.global,
        );
        server::publish(&self.events, event);
    }

    fn ready_world(
        &mut self, prepared: Option<&Prepared>, opts: &BuildOptions,
    ) -> BuildResult<()> {
        // get(None) retains an existing World's settings. Only a new World may
        // use freshly loaded fallback settings for accessed-package filtering.
        let captured = capture(|| {
            let context = match prepared {
                Some(prepared) => Some((
                    prepared.config.clone(),
                    prepared.document.source.base_dir.clone(),
                )),
                None if self.worlds.current().is_none() => {
                    Some(self.fallback_config())
                }
                None => None,
            };
            self.worlds.get(prepared, opts)?.reset();
            Ok(context)
        });
        match captured.result {
            Ok(context) => {
                if let Some(context) = context {
                    self.world_config = Some(context);
                }
                Ok(())
            }
            Err(failure) => {
                self.discard_world();
                Err(failure)
            }
        }
    }

    fn discard_world(&mut self) {
        self.worlds.discard();
        self.world_config = None;
    }

    fn accessed(&mut self) -> BTreeSet<PathBuf> {
        match (self.worlds.current(), self.world_config.as_ref()) {
            (Some(world), Some((config, root))) => {
                accessed_files(world, config, root)
            }
            _ => BTreeSet::new(),
        }
    }

    fn fallback_config(&self) -> (Config, PathBuf) {
        // Match WorldSlot::get(None)'s construction fallback exactly.
        let config = pipeline::load_config(&self.opts)
            .map(|(config, _)| config)
            .unwrap_or_default();
        let input = std::path::absolute(&self.opts.input)
            .unwrap_or_else(|_| self.opts.input.clone());
        let root = canonical(input.parent().unwrap_or(Path::new("/")));
        (config, root)
    }

    fn update_watches(
        &mut self, prepared: Option<&Prepared>, ran: bool,
        failures: &[&BuildFailure],
    ) {
        let accessed = self
            .accessed_web
            .union(&self.accessed_pages)
            .cloned()
            .collect::<Vec<_>>();
        let mut set = match prepared {
            Some(prepared) => {
                let set = watch_set(
                    &pipeline::watch_inputs(prepared),
                    &accessed,
                    None,
                );
                self.last_good = Some(set.clone());
                set
            }
            None if ran => {
                let mut set = watch_set_before_build(&self.opts);
                if let Some(good) = &self.last_good {
                    set.merge(good);
                }
                set.merge(&WatchSet::new(accessed, []));
                set
            }
            None => {
                let mut set = self.set.clone();
                set.merge(&WatchSet::new(accessed, []));
                set
            }
        };
        for failure in failures {
            set.merge(&WatchSet::new(failure_paths(failure), []));
        }
        if let Err(error) = self.watcher.update(&set) {
            tracing::warn!("{error}");
        }
        self.set = set;
    }

    fn restart_diagnostic(
        &mut self, prepared: &Prepared,
    ) -> Option<Diagnostic> {
        let note = restart_note(&self.listening, &prepared.config.serve);
        let changed = note
            .as_ref()
            .filter(|note| self.restart_note.as_ref() != Some(*note))
            .cloned();
        self.restart_note = note;
        changed.map(|note| {
            let mut diagnostic = Diagnostic::warning("serve-config", note);
            diagnostic.path = prepared.config_path.clone();
            diagnostic
        })
    }
}

struct Attempt {
    prepare: Option<BuildResult<Prepared>>,
    world_failure: Option<BuildFailure>,
    web: ViewStage<WebBuild>,
    pages: ViewStage<Arc<PageSet>>,
}

struct PrepareMessages {
    diagnostics: Option<Diagnostics>,
    warnings: Option<Vec<DiagnosticJson>>,
}

impl PrepareMessages {
    fn push(&mut self, diagnostic: Diagnostic) {
        self.warnings.as_mut().expect("prepare ran").push(DiagnosticJson::new(
            &diagnostic,
            std::env::current_dir().ok().as_deref(),
        ));
        self.diagnostics.as_mut().expect("prepare ran").push(diagnostic);
    }
}

fn prepare_messages(
    prepare: Option<&BuildResult<Prepared>>, web_failure: Option<&BuildFailure>,
) -> PrepareMessages {
    match prepare {
        Some(Ok(prepared)) => {
            let omitted = web_failure.and_then(|failure| {
                subset_diagnostic_index(&prepared.diagnostics, failure)
            });
            PrepareMessages {
                diagnostics: Some(prepared.diagnostics.clone()),
                warnings: Some(browser_warnings(
                    &prepared.diagnostics, omitted,
                )),
            }
        }
        Some(Err(failure)) => PrepareMessages {
            diagnostics: Some(failure.diagnostics.clone()),
            warnings: Some(failure_warnings(failure)),
        },
        None => PrepareMessages { diagnostics: None, warnings: None },
    }
}

/// Production stage calls stay separate so tests can exercise panic recovery without
/// adding a public injection API or asking a compiler bug to occur.
trait BuildStages {
    fn prepare(&mut self, opts: &BuildOptions) -> BuildResult<Prepared>;
    fn web(
        &mut self, prepared: &Prepared, opts: &BuildOptions,
        world: &mut WriterWorld,
    ) -> BuildResult<WebPage>;
    fn pages(
        &mut self, builder: &mut PagesBuilder, world: &mut WriterWorld,
        input: PagesInput<'_>, id: u64,
    ) -> BuildResult<(Arc<PageSet>, Diagnostics)>;
}

struct LiveStages;

impl BuildStages for LiveStages {
    fn prepare(&mut self, opts: &BuildOptions) -> BuildResult<Prepared> {
        pipeline::prepare(opts)
    }

    fn web(
        &mut self, prepared: &Prepared, opts: &BuildOptions,
        world: &mut WriterWorld,
    ) -> BuildResult<WebPage> {
        pipeline::web_page(prepared, opts, world)
    }

    fn pages(
        &mut self, builder: &mut PagesBuilder, world: &mut WriterWorld,
        input: PagesInput<'_>, id: u64,
    ) -> BuildResult<(Arc<PageSet>, Diagnostics)> {
        builder.build(world, input, id)
    }
}

struct Captured<T> {
    result: BuildResult<T>,
    panicked: bool,
}

fn capture<T>(build: impl FnOnce() -> BuildResult<T>) -> Captured<T> {
    match catch_unwind(AssertUnwindSafe(build)) {
        Ok(result) => Captured { result, panicked: false },
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|message| (*message).to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Captured {
                result: Err(Error::Other(format!(
                    "the build panicked (a bug in writer): {message}"
                ))
                .into()),
                panicked: true,
            }
        }
    }
}

struct ViewStage<T> {
    outcome: ViewOutcome<T>,
    failure: Option<BuildFailure>,
    warnings: Option<Vec<DiagnosticJson>>,
    diagnostics: Diagnostics,
}

impl<T> ViewStage<T> {
    fn skipped() -> Self {
        Self {
            outcome: ViewOutcome::Skipped,
            failure: None,
            warnings: None,
            diagnostics: Diagnostics::new(),
        }
    }

    fn blocked(payload: ErrorPayload) -> Self {
        Self { outcome: ViewOutcome::Failed(payload), ..Self::skipped() }
    }

    fn failed(failure: BuildFailure, input: &Path) -> Self {
        let warnings = failure_warnings(&failure);
        let diagnostics = failure.diagnostics.clone();
        Self {
            outcome: ViewOutcome::Failed(ErrorPayload::for_failure(
                &failure, input,
            )),
            failure: Some(failure),
            warnings: Some(warnings),
            diagnostics,
        }
    }

    fn from_result<U>(
        result: BuildResult<U>, input: &Path,
        convert: impl FnOnce(U) -> (T, Diagnostics),
    ) -> Self {
        match result {
            Ok(value) => {
                let (value, diagnostics) = convert(value);
                let warnings = browser_warnings(&diagnostics, None);
                Self {
                    outcome: ViewOutcome::Built(value),
                    failure: None,
                    warnings: Some(warnings),
                    diagnostics,
                }
            }
            Err(failure) => Self::failed(failure, input),
        }
    }
}

fn browser_warnings(
    diagnostics: &Diagnostics, omitted: Option<usize>,
) -> Vec<DiagnosticJson> {
    let cwd = std::env::current_dir().ok();
    diagnostics
        .iter()
        .enumerate()
        .filter(|(index, diagnostic)| {
            Some(*index) != omitted && diagnostic.level != Level::Info
        })
        .map(|(_, diagnostic)| DiagnosticJson::new(diagnostic, cwd.as_deref()))
        .collect()
}

fn failure_warnings(failure: &BuildFailure) -> Vec<DiagnosticJson> {
    let omitted = matches!(*failure.error, Error::Compile { .. })
        .then(|| {
            failure
                .diagnostics
                .iter()
                .position(|diagnostic| diagnostic.level == Level::Error)
        })
        .flatten();
    browser_warnings(&failure.diagnostics, omitted)
}

fn subset_diagnostic_index(
    diagnostics: &Diagnostics, failure: &BuildFailure,
) -> Option<usize> {
    let subset = pipeline::subset_error(diagnostics)?;
    if !matches!(failure.error.as_ref(), Error::Parse { message, .. } if *message == subset.message)
    {
        return None;
    }
    diagnostics.iter().position(|diagnostic| std::ptr::eq(diagnostic, subset))
}

fn print_reports(
    prepare: Option<&Diagnostics>, web: &Diagnostics, pages: &Diagnostics,
    failures: &[&BuildFailure], input: &Path, global: &GlobalArgs,
) {
    let mut diagnostics = Vec::new();
    for diagnostic in
        prepare.into_iter().flat_map(Diagnostics::iter).chain(web).chain(pages)
    {
        if !diagnostics.contains(diagnostic) {
            diagnostics.push(diagnostic.clone());
        }
    }
    report_diagnostics(&diagnostics.into(), global);
    let mut printed = Vec::new();
    for failure in failures {
        let payload = ErrorPayload::for_failure(failure, input);
        if !printed.contains(&payload) {
            report::print_error_in(
                &failure.error,
                Some(input),
                &report::read_source,
            );
            printed.push(payload);
        }
    }
}

fn merge_batches(queued: Vec<ChangeBatch>, set: &WatchSet) -> ChangeBatch {
    let paths = queued
        .into_iter()
        .flat_map(|batch| batch.paths)
        .map(|path| canonical(&path))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    ChangeBatch { kind: classify(&paths, set), paths }
}

fn kind_name(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Css => "css",
        ChangeKind::Pages => "pages",
        ChangeKind::Content => "content",
    }
}

fn rebuilt_line(ms: u64, kind: ChangeKind, web: bool, pages: bool) -> String {
    let failure = match (web, pages) {
        (false, false) => "",
        (true, false) => "; web failed",
        (false, true) => "; pages failed",
        (true, true) => "; web and pages failed",
    };
    format!("rebuilt in {ms} ms ({}{failure})", kind_name(kind))
}

fn input_title(input: &Path) -> String {
    input
        .file_name()
        .unwrap_or(input.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn restart_note(
    listening: &ServeConfig, configured: &ServeConfig,
) -> Option<String> {
    let address =
        |config: &ServeConfig| match config.host.parse::<std::net::IpAddr>() {
            Ok(ip) => std::net::SocketAddr::new(ip, config.port).to_string(),
            Err(_) => format!("{}:{}", config.host, config.port),
        };
    let now = address(listening);
    let wanted = address(configured);
    (now != wanted).then(|| format!(
        "[serve] now asks for {wanted}, but the server was started with {now}: restart writer serve to apply it"
    ))
}

fn document_versions(html: &str) -> BTreeSet<String> {
    static VERSIONED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"/__writer/asset/[^"'\s?#<>()]*\?v=[0-9A-Za-z]+"#)
            .expect("versioned asset URL regex is valid")
    });
    VERSIONED.find_iter(html).map(|found| found.as_str().to_string()).collect()
}

#[derive(Default)]
struct Sources {
    text: BTreeMap<PathBuf, String>,
    order: Vec<PathBuf>,
}

impl Sources {
    fn changed(&mut self, paths: &[PathBuf]) -> Option<Position> {
        let paths =
            paths.iter().map(|path| canonical(path)).collect::<BTreeSet<_>>();
        let mut first = None;
        for path in &self.order {
            if !paths.contains(path) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let old = self.text.insert(path.clone(), text.clone());
            if let Some(old) = old {
                let place = match detect_kind(path).ok() {
                    Some(SourceKind::Notebook) => {
                        notebook_difference(&old, &text).map(Place::Cell)
                    }
                    Some(SourceKind::Markdown | SourceKind::Typst) => {
                        text_difference(&old, &text).map(Place::Line)
                    }
                    None => None,
                };
                if first.is_none() {
                    first = place
                        .map(|place| Position { path: path.clone(), place });
                }
            }
        }
        first
    }

    fn observe(&mut self, prepared: &Prepared) {
        self.order = std::iter::once(&prepared.document.source.path)
            .chain(&prepared.document.includes)
            .filter(|path| detect_kind(path).is_ok())
            .map(|path| canonical(path))
            .collect();
        for path in &self.order {
            if !self.text.contains_key(path)
                && let Ok(text) = std::fs::read_to_string(path)
            {
                self.text.insert(path.clone(), text);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Line(usize),
    Cell(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Position {
    path: PathBuf,
    place: Place,
}

impl Position {
    fn in_document(self, prepared: &Prepared) -> Option<Changed> {
        let source = std::iter::once(&prepared.document.source.path)
            .chain(&prepared.document.includes)
            .position(|path| canonical(path) == self.path)?;
        let cwd = std::env::current_dir().ok();
        let (line, cell) = match self.place {
            Place::Line(line) => (Some(line), None),
            Place::Cell(cell) => (None, Some(cell)),
        };
        Some(Changed {
            file: browser_path(&self.path, cwd.as_deref()),
            source,
            line,
            cell,
        })
    }
}

fn browser_path(path: &Path, base: Option<&Path>) -> String {
    let relative = base
        .and_then(|base| {
            path.strip_prefix(base).ok().map(Path::to_path_buf).or_else(|| {
                canonical(path)
                    .strip_prefix(canonical(base))
                    .ok()
                    .map(Path::to_path_buf)
            })
        })
        .filter(|relative| !relative.as_os_str().is_empty());
    relative
        .as_deref()
        .unwrap_or(path)
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn text_difference(old: &str, new: &str) -> Option<usize> {
    let old = old.lines().collect::<Vec<_>>();
    let new = new.lines().collect::<Vec<_>>();
    if old == new {
        return None;
    }
    let index = old
        .iter()
        .zip(&new)
        .position(|(old, new)| old != new)
        .unwrap_or(old.len().min(new.len()));
    Some((index + 1).min(new.len().max(1)))
}

fn notebook_difference(old: &str, new: &str) -> Option<usize> {
    let old: serde_json::Value = serde_json::from_str(old).ok()?;
    let new: serde_json::Value = serde_json::from_str(new).ok()?;
    let (old, new) =
        (old.get("cells")?.as_array()?, new.get("cells")?.as_array()?);
    if old == new || new.is_empty() {
        return None;
    }
    let index = old
        .iter()
        .zip(new)
        .position(|(old, new)| old != new)
        .unwrap_or(old.len().min(new.len()));
    Some(index.min(new.len() - 1))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use serde_json::json;
    use typst::World as _;

    use super::*;
    use crate::ir::SourceSpan;
    use crate::serve::events::ViewStatus;
    use crate::typst::sourcemap::SourceMap;

    #[test]
    fn text_diff_covers_line_positions_prefixes_and_empty_files() {
        for (old, new, expected) in [
            ("one\ntwo\nthree\n", "ONE\ntwo\nthree\n", Some(1)),
            ("one\ntwo\nthree\n", "one\nTWO\nthree\n", Some(2)),
            ("one\ntwo\nthree\n", "one\ntwo\nTHREE\n", Some(3)),
            ("one\ntwo", "one\ntwo\nthree", Some(3)),
            ("one\ntwo\nthree", "one\ntwo", Some(2)),
            ("one", "", Some(1)),
            ("", "one", Some(1)),
            ("", "", None),
            ("one\ntwo\n", "one\ntwo", None),
            ("one\r\ntwo\r\n", "one\ntwo\n", None),
        ] {
            assert_eq!(
                text_difference(old, new),
                expected,
                "{old:?} -> {new:?}"
            );
        }
    }

    #[test]
    fn notebook_diff_compares_whole_cells_and_ignores_only_top_level_metadata()
    {
        let notebook = |cells: serde_json::Value, metadata: u32| {
            json!({"cells": cells, "metadata": {"modified": metadata}})
                .to_string()
        };
        let old = notebook(json!([{"source":"a"}, {"source":"b"}]), 1);
        assert_eq!(
            notebook_difference(
                &old,
                &notebook(json!([{"source":"a"}, {"source":"b"}]), 2)
            ),
            None
        );
        for (cells, expected) in [
            (json!([{"source":"A"}, {"source":"b"}]), Some(0)),
            (json!([{"source":"a"}, {"source":"B"}]), Some(1)),
            (json!([{"source":"a"}, {"source":"b", "outputs": [1]}]), Some(1)),
            (
                json!([{"source":"a"}, {"source":"b", "metadata": {"tag":"new"}}]),
                Some(1),
            ),
            (json!([{"source":"a"}, {"source":"b"}, {"source":"c"}]), Some(2)),
            (json!([{"source":"a"}]), Some(0)),
            (json!([]), None),
        ] {
            assert_eq!(
                notebook_difference(&old, &notebook(cells, 1)),
                expected
            );
        }
        assert_eq!(notebook_difference("broken", &old), None);
        assert_eq!(notebook_difference(&old, "broken"), None);
        assert_eq!(notebook_difference("{}", &old), None);
        assert_eq!(notebook_difference(&old, "{\"cells\":null}"), None);
    }

    fn project() -> (tempfile::TempDir, BuildOptions) {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("writer.toml"),
            "[pdf]\nsystem_fonts = false\n",
        )
        .unwrap();
        let input = directory.path().join("document.md");
        std::fs::write(&input, "# Document\n\nParagraph.\n").unwrap();
        let mut opts = BuildOptions::new(input);
        opts.mode = Mode::Serve { build_id: 1 };
        opts.format = Some(SourceKind::Markdown);
        (directory, opts)
    }

    fn session(opts: BuildOptions) -> Session {
        let (tx, _rx) = mpsc::channel();
        let watcher =
            Watcher::start(&watch_set_before_build(&opts), tx).unwrap();
        Session::new(
            opts,
            GlobalArgs { verbose: 0, quiet: true, config: None },
            watcher,
            &ServeConfig::default(),
        )
    }

    fn batch(path: &Path) -> Vec<ChangeBatch> {
        vec![ChangeBatch {
            paths: vec![path.to_path_buf()],
            kind: ChangeKind::Content,
        }]
    }

    #[test]
    fn sources_choose_document_order_and_update_every_changed_baseline() {
        let (directory, opts) = project();
        let a = directory.path().join("a.md");
        let b = directory.path().join("b.md");
        std::fs::write(&a, "a\nold\n").unwrap();
        std::fs::write(&b, "b\nold\n").unwrap();
        let mut prepared = pipeline::prepare(&opts).unwrap();
        prepared.document.includes = vec![a.clone(), b.clone()];
        let mut sources = Sources::default();
        sources.observe(&prepared);
        std::fs::write(&a, "a\nnew\n").unwrap();
        std::fs::write(&b, "b\nnew\n").unwrap();
        let position = sources.changed(&[b.clone(), a.clone()]).unwrap();
        assert_eq!(position.path, canonical(&a));
        assert_eq!(position.place, Place::Line(2));
        assert_eq!(sources.changed(&[a.clone(), b.clone()]), None);
        prepared.document.includes = vec![b, a.clone()];
        let changed = position.clone().in_document(&prepared).unwrap();
        assert_eq!(changed.source, 2);
        assert_eq!(changed.line, Some(2));
        assert_eq!(changed.cell, None);
        prepared.document.includes.clear();
        assert_eq!(position.in_document(&prepared), None);
        std::fs::write(&a, "new include\n").unwrap();
        prepared.document.includes.push(a.clone());
        sources.text.remove(&canonical(&a));
        sources.observe(&prepared);
        assert_eq!(sources.changed(&[a]), None);
    }

    #[test]
    fn source_without_baseline_and_unreadable_source_have_no_position() {
        let (directory, opts) = project();
        let mut prepared = pipeline::prepare(&opts).unwrap();
        let missing = directory.path().join("missing.md");
        prepared.document.includes.push(missing.clone());
        let mut sources = Sources::default();
        sources.observe(&prepared);
        assert_eq!(sources.changed(std::slice::from_ref(&missing)), None);
        std::fs::write(&missing, "new\n").unwrap();
        assert_eq!(sources.changed(std::slice::from_ref(&missing)), None);
        std::fs::remove_file(&missing).unwrap();
        assert_eq!(sources.changed(std::slice::from_ref(&missing)), None);
        assert_eq!(sources.text[&canonical(&missing)], "new\n");
    }

    #[cfg(unix)]
    #[test]
    fn canonical_include_aliases_map_back_to_the_new_source_index() {
        let (directory, opts) = project();
        let target = directory.path().join("chapter.md");
        let alias = directory.path().join("alias.md");
        std::fs::write(&target, "old\n").unwrap();
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        let mut prepared = pipeline::prepare(&opts).unwrap();
        prepared.document.includes.push(alias);
        let mut sources = Sources::default();
        sources.observe(&prepared);
        std::fs::write(&target, "new\n").unwrap();
        let position = sources.changed(&[target]).unwrap();
        assert_eq!(position.in_document(&prepared).unwrap().source, 1);
    }

    #[test]
    fn browser_paths_are_relative_only_below_base() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("project");
        let file = base.join("chapter.md");
        let outside = directory.path().join("elsewhere.md");
        std::fs::create_dir(&base).unwrap();
        assert_eq!(browser_path(&file, Some(&base)), "chapter.md");
        assert_eq!(
            browser_path(&base, Some(&base)),
            base.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")
        );
        assert_eq!(
            browser_path(&outside, Some(&base)),
            outside.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")
        );
    }

    #[test]
    fn merge_batches_sorts_deduplicates_and_reclassifies_pages_and_mixtures() {
        let directory = tempfile::tempdir().unwrap();
        let css = directory.path().join("x.css");
        let template = canonical(&directory.path().join("template.typ"));
        let set = WatchSet {
            pages_paths: vec![template.clone()],
            ..Default::default()
        };
        let page_batch = merge_batches(batch(&template), &set);
        assert_eq!(page_batch.kind, ChangeKind::Pages);
        let css_batch = merge_batches(batch(&css), &set);
        assert_eq!(css_batch.kind, ChangeKind::Css);
        let mut queued = batch(&template);
        queued.extend(batch(&css));
        queued.extend(batch(&template));
        let all = merge_batches(queued, &set);
        assert_eq!(all.kind, ChangeKind::Content);
        assert_eq!(all.paths.len(), 2);
        assert!(all.paths.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(merge_batches(Vec::new(), &set).kind, ChangeKind::Content);
    }

    #[test]
    fn version_detection_excludes_build_id_css_and_document_assets() {
        let versions = document_versions(
            r#"<link href="/__writer/theme.css?v=1"><img src="/__writer/doc-asset/0123456789ab?v=2"><img src="/__writer/asset/@theme/assets/logo.png?v=Ab3">"#,
        );
        assert_eq!(
            versions,
            BTreeSet::from([
                "/__writer/asset/@theme/assets/logo.png?v=Ab3".into()
            ])
        );
    }

    #[test]
    fn timing_lines_cover_each_attempt_failure_combination() {
        for (web, pages, suffix) in [
            (false, false, ""),
            (true, false, "; web failed"),
            (false, true, "; pages failed"),
            (true, true, "; web and pages failed"),
        ] {
            assert_eq!(
                rebuilt_line(42, ChangeKind::Pages, web, pages),
                format!("rebuilt in 42 ms (pages{suffix})")
            );
        }
    }

    #[test]
    fn restart_notes_compare_requested_addresses_including_zero_and_ipv6() {
        let listening = ServeConfig { port: 0, ..Default::default() };
        assert_eq!(restart_note(&listening, &listening), None);
        let configured =
            ServeConfig { host: "::1".into(), port: 0, ..Default::default() };
        assert_eq!(restart_note(&listening, &configured), Some("[serve] now asks for [::1]:0, but the server was started with 127.0.0.1:0: restart writer serve to apply it".into()));
    }

    #[test]
    fn browser_warnings_remove_only_the_payload_error_and_keep_unrelated_findings()
     {
        let first = Diagnostic::error("compile-error", "first");
        let second = Diagnostic::error("compile-error", "second");
        let failure = BuildFailure {
            error: Box::new(Error::Compile {
                path: "document.md".into(),
                line: Some(1),
                message: "first".into(),
                hints: Vec::new(),
            }),
            diagnostics: vec![
                Diagnostic::info("note", "info"),
                first,
                Diagnostic::warning("warn", "warning"),
                second,
            ]
            .into(),
        };
        let warnings = failure_warnings(&failure);
        assert_eq!(
            warnings
                .iter()
                .map(|warning| warning.message.as_str())
                .collect::<Vec<_>>(),
            ["warning", "second"]
        );
        let mut diagnostics = Diagnostics::new();
        diagnostics.push(Diagnostic::error("E-TYP-SYNTAX", "unsupported"));
        diagnostics.push(Diagnostic::warning("W-TYP-OTHER", "keep this"));
        let stopped = BuildFailure::from(Error::Parse {
            path: "document.md".into(),
            line: None,
            message: "unsupported".into(),
        });
        assert_eq!(subset_diagnostic_index(&diagnostics, &stopped), Some(0));
        assert_eq!(
            browser_warnings(&diagnostics, Some(0))[0].message,
            "keep this"
        );
    }

    #[test]
    fn capture_preserves_errors_and_reports_each_panic_payload_kind() {
        for build in [
            (|| -> BuildResult<()> { panic!("string slice") })
                as fn() -> BuildResult<()>,
            || -> BuildResult<()> {
                std::panic::panic_any("owned string".to_string())
            },
            || -> BuildResult<()> { std::panic::panic_any(7u8) },
        ] {
            let captured = capture(build);
            assert!(captured.panicked);
            assert!(
                captured
                    .result
                    .unwrap_err()
                    .error
                    .to_string()
                    .starts_with("the build panicked (a bug in writer): ")
            );
        }
        let ordinary = capture(|| {
            Err::<(), _>(Error::Config("config error".into()).into())
        });
        assert!(!ordinary.panicked);
        assert!(matches!(
            *ordinary.result.unwrap_err().error,
            Error::Config(_)
        ));
    }

    #[derive(Clone, Copy, Default)]
    enum Action {
        #[default]
        Good,
        Fail,
        Panic,
    }

    #[derive(Default)]
    struct Stages {
        prepare: Action,
        web: Action,
        pages: Action,
        calls: Vec<&'static str>,
        fresh_pages_world: bool,
        prepared_diagnostics: Diagnostics,
        web_diagnostics: Diagnostics,
        pages_diagnostics: Diagnostics,
        html_version: Option<&'static str>,
        web_read: Option<&'static str>,
        pages_read: Option<&'static str>,
    }

    impl BuildStages for Stages {
        fn prepare(&mut self, opts: &BuildOptions) -> BuildResult<Prepared> {
            self.calls.push("prepare");
            match self.prepare {
                Action::Panic => panic!("prepare panic"),
                Action::Fail => Err(Error::Parse {
                    path: opts.input.clone(),
                    line: Some(1),
                    message: "prepare failure".into(),
                }
                .into()),
                Action::Good => {
                    let mut prepared = pipeline::prepare(opts)?;
                    prepared
                        .diagnostics
                        .extend(self.prepared_diagnostics.clone());
                    Ok(prepared)
                }
            }
        }

        fn web(
            &mut self, prepared: &Prepared, opts: &BuildOptions,
            world: &mut WriterWorld,
        ) -> BuildResult<WebPage> {
            self.calls.push("web");
            if let Some(path) = self.web_read {
                let id = typst::syntax::RootedPath::new(
                    typst::syntax::VirtualRoot::Project,
                    typst::syntax::VirtualPath::new(path).unwrap(),
                )
                .intern();
                world.file(id).unwrap();
            }
            match self.web {
                Action::Panic => {
                    world.set_main_text(
                        "poisoned old world",
                        SourceMap::default(),
                    );
                    panic!("web panic");
                }
                Action::Fail => Err(Error::Other("web failure".into()).into()),
                Action::Good => {
                    let mut page = pipeline::web_page(prepared, opts, world)?;
                    page.diagnostics.extend(self.web_diagnostics.clone());
                    if let Some(version) = self.html_version {
                        page.html.push_str(&format!(
                            "<img src=\"/__writer/asset/logo.svg?v={version}\">"
                        ));
                    }
                    Ok(page)
                }
            }
        }

        fn pages(
            &mut self, builder: &mut PagesBuilder, world: &mut WriterWorld,
            input: PagesInput<'_>, id: u64,
        ) -> BuildResult<(Arc<PageSet>, Diagnostics)> {
            self.calls.push("pages");
            if let Some(path) = self.pages_read {
                let id = typst::syntax::RootedPath::new(
                    typst::syntax::VirtualRoot::Project,
                    typst::syntax::VirtualPath::new(path).unwrap(),
                )
                .intern();
                world.file(id).unwrap();
            }
            if self.fresh_pages_world {
                assert_eq!(world.source(world.main()).unwrap().text(), "");
            }
            match self.pages {
                Action::Panic => panic!("pages panic"),
                Action::Fail => {
                    Err(Error::Other("pages failure".into()).into())
                }
                Action::Good => {
                    let (set, mut diagnostics) =
                        builder.build(world, input, id)?;
                    diagnostics.extend(self.pages_diagnostics.clone());
                    Ok((set, diagnostics))
                }
            }
        }
    }

    fn state(session: &Session) -> serde_json::Value {
        serde_json::to_value(session.state.read().unwrap().hello()).unwrap()
    }

    #[test]
    fn session_runs_only_affected_stages_and_keeps_skipped_warnings() {
        let (_directory, opts) = project();
        let css = opts.input.with_extension("css");
        let template = opts.input.with_extension("typ");
        let mut session = session(opts);
        session.set.pages_paths.push(canonical(&template));
        let mut stages = Stages {
            web_diagnostics: vec![Diagnostic::warning("web-warning", "web")]
                .into(),
            pages_diagnostics: vec![Diagnostic::warning(
                "pages-warning", "pages",
            )]
            .into(),
            ..Default::default()
        };
        session.rebuild_with(Vec::new(), &mut stages);
        assert_eq!(stages.calls, ["prepare", "web", "pages"]);
        stages.calls.clear();
        stages.web_diagnostics = Diagnostics::new();
        session.rebuild_with(batch(&css), &mut stages);
        assert_eq!(stages.calls, ["prepare", "web"]);
        assert_eq!(state(&session)["warnings"][0]["message"], "pages");
        assert_eq!(state(&session)["warnings"].as_array().unwrap().len(), 1);
        stages.calls.clear();
        // The synthetic template is explicitly added after watch-set refresh.
        session.set.pages_paths.push(canonical(&template));
        session.rebuild_with(batch(&template), &mut stages);
        assert_eq!(stages.calls, ["prepare", "pages"]);
        assert_eq!(state(&session)["build_id"], 3);
    }

    #[test]
    fn session_prepare_failure_shares_payload_and_retains_prior_stage_warnings()
    {
        let (_directory, opts) = project();
        let mut session = session(opts);
        let mut stages = Stages {
            web_diagnostics: vec![Diagnostic::warning("web-warning", "web")]
                .into(),
            ..Default::default()
        };
        session.rebuild_with(Vec::new(), &mut stages);
        stages.calls.clear();
        stages.prepare = Action::Fail;
        session.rebuild_with(Vec::new(), &mut stages);
        assert_eq!(stages.calls, ["prepare"]);
        let failed = state(&session);
        assert_eq!(failed["status"], "error");
        assert_eq!(failed["web"]["error"], failed["pages"]["error"]);
        assert_eq!(failed["warnings"][0]["message"], "web");
        assert!(session.state.read().unwrap().web.good.is_some());
        stages.prepare = Action::Good;
        session.rebuild_with(Vec::new(), &mut stages);
        assert_eq!(state(&session)["status"], "ok");
    }

    #[test]
    fn web_panic_discards_world_and_independent_pages_reacquires_then_recovers()
    {
        let (_directory, opts) = project();
        let mut session = session(opts);
        let mut stages = Stages {
            web: Action::Panic,
            fresh_pages_world: true,
            ..Default::default()
        };
        session.rebuild_with(Vec::new(), &mut stages);
        let failed = state(&session);
        assert_eq!(failed["web"]["status"], "error");
        assert_eq!(failed["pages"]["status"], "ok");
        assert_eq!(stages.calls, ["prepare", "web", "pages"]);
        assert!(session.worlds.current().is_some());
        stages.web = Action::Good;
        stages.fresh_pages_world = false;
        session.rebuild_with(Vec::new(), &mut stages);
        assert_eq!(state(&session)["status"], "ok");
    }

    #[test]
    fn pages_panic_preserves_successful_web_and_next_build_recovers() {
        let (_directory, opts) = project();
        let mut session = session(opts);
        let mut stages = Stages { pages: Action::Panic, ..Default::default() };
        session.rebuild_with(Vec::new(), &mut stages);
        let failed = state(&session);
        assert_eq!(failed["web"]["status"], "ok");
        assert_eq!(failed["pages"]["status"], "error");
        assert!(session.worlds.current().is_none());
        stages.pages = Action::Good;
        session.rebuild_with(Vec::new(), &mut stages);
        assert_eq!(state(&session)["status"], "ok");
    }

    #[test]
    fn prepare_panic_fails_both_without_entering_views_then_next_build_recovers()
     {
        let (_directory, opts) = project();
        let mut session = session(opts);
        let mut stages =
            Stages { prepare: Action::Panic, ..Default::default() };
        session.rebuild_with(Vec::new(), &mut stages);
        assert_eq!(stages.calls, ["prepare"]);
        assert_eq!(state(&session)["status"], "error");
        assert!(session.worlds.current().is_none());
        stages.prepare = Action::Good;
        session.rebuild_with(Vec::new(), &mut stages);
        assert_eq!(state(&session)["status"], "ok");
    }

    #[test]
    fn css_upgrade_uses_theme_asset_versions_but_keeps_pages_skipped() {
        let (_directory, opts) = project();
        let css = opts.input.with_extension("css");
        let mut session = session(opts);
        let mut receiver = session.events.subscribe();
        let mut stages =
            Stages { html_version: Some("1"), ..Default::default() };
        session.rebuild_with(Vec::new(), &mut stages);
        assert_eq!(receiver.try_recv().unwrap().kind, ChangeKind::Content);
        session.rebuild_with(batch(&css), &mut stages);
        assert_eq!(receiver.try_recv().unwrap().kind, ChangeKind::Css);
        stages.html_version = Some("2");
        session.rebuild_with(batch(&css), &mut stages);
        let event = receiver.try_recv().unwrap();
        assert_eq!(event.kind, ChangeKind::Content);
        assert_eq!(event.pages.status, ViewStatus::Skipped);
        assert_eq!(event.changed, None);
    }

    #[test]
    fn failed_stage_browser_lists_filter_info_and_keep_other_stage_diagnostics()
    {
        let (_directory, opts) = project();
        let mut session = session(opts);
        let mut stages = Stages {
            prepared_diagnostics: vec![
                Diagnostic::info("info", "quiet note"),
                Diagnostic::error("duplicate-label", "nonfatal"),
            ]
            .into(),
            web_diagnostics: vec![Diagnostic::warning("shared", "same")].into(),
            pages_diagnostics: vec![Diagnostic::warning("shared", "same")]
                .into(),
            ..Default::default()
        };
        session.rebuild_with(Vec::new(), &mut stages);
        let hello = state(&session);
        assert_eq!(hello["status"], "ok");
        assert_eq!(hello["warnings"].as_array().unwrap().len(), 2);
        assert_eq!(hello["warnings"][0]["message"], "nonfatal");
        assert_eq!(hello["warnings"][1]["message"], "same");
    }

    #[test]
    fn notebook_changed_position_has_cell_and_no_line() {
        let (directory, opts) = project();
        let input = directory.path().join("notebook.ipynb");
        let old = json!({"cells": [{"source":"old"}]}).to_string();
        let new = json!({"cells": [{"source":"new"}]}).to_string();
        std::fs::write(&input, &old).unwrap();
        let mut prepared = pipeline::prepare(&opts).unwrap();
        prepared.document.source.path = input.clone();
        let mut sources = Sources::default();
        sources.observe(&prepared);
        std::fs::write(&input, &new).unwrap();
        let changed =
            sources.changed(&[input]).unwrap().in_document(&prepared).unwrap();
        assert_eq!(changed.cell, Some(0));
        assert_eq!(changed.line, None);
        assert_eq!(changed.source, 0);
    }

    #[test]
    fn compile_diagnostic_removal_preserves_cell_provenance_in_other_entries() {
        let diagnostic =
            Diagnostic::error("compile", "first").with_span(SourceSpan {
                start_line: 3,
                end_line: 3,
                cell: Some(2),
                file: Some("included.ipynb".into()),
            });
        let remaining =
            Diagnostic::warning("warning", "second").with_span(SourceSpan {
                start_line: 5,
                end_line: 5,
                cell: Some(3),
                file: Some("included.ipynb".into()),
            });
        let failure = BuildFailure {
            error: Box::new(Error::Compile {
                path: "generated.typ".into(),
                line: Some(1),
                message: "first".into(),
                hints: vec!["hint".into()],
            }),
            diagnostics: vec![diagnostic, remaining].into(),
        };
        let warnings = failure_warnings(&failure);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].cell, Some(3));
        assert_eq!(warnings[0].line, Some(5));
    }

    #[test]
    fn skipped_stage_keeps_its_accessed_files_across_partial_builds() {
        let (directory, opts) = project();
        let root = directory.path();
        let web = root.join("web.typ");
        let pages = root.join("pages.typ");
        std::fs::write(&web, "web input").unwrap();
        std::fs::write(&pages, "pages input").unwrap();
        let css = root.join("theme.css");
        let template = root.join("template.typ");
        let mut session = session(opts);
        let mut stages = Stages {
            web_read: Some("web.typ"),
            pages_read: Some("pages.typ"),
            ..Default::default()
        };
        session.rebuild_with(Vec::new(), &mut stages);
        session.rebuild_with(batch(&css), &mut stages);
        assert!(session.set.files.contains(&canonical(&web)));
        assert!(session.set.files.contains(&canonical(&pages)));
        session.set.pages_paths.push(canonical(&template));
        session.rebuild_with(batch(&template), &mut stages);
        assert!(session.set.files.contains(&canonical(&web)));
        assert!(session.set.files.contains(&canonical(&pages)));
    }

    #[test]
    fn typst_prepare_failure_does_not_block_pages_and_pages_kind_skips_frontend()
     {
        let (directory, mut opts) = project();
        opts.input = directory.path().join("document.typ");
        opts.format = Some(SourceKind::Typst);
        std::fs::write(&opts.input, "Hello").unwrap();
        let input = opts.input.clone();
        let mut session = session(opts);
        let mut stages = Stages::default();
        session.rebuild_with(Vec::new(), &mut stages);
        assert_eq!(stages.calls, ["prepare", "pages"]);
        assert_eq!(state(&session)["web"]["status"], "error");
        assert_eq!(state(&session)["pages"]["status"], "ok");
        stages.calls.clear();
        session.set.pages_paths.push(canonical(&input));
        session.rebuild_with(batch(&input), &mut stages);
        assert_eq!(stages.calls, ["pages"]);
        assert_eq!(state(&session)["pages"]["status"], "ok");
    }

    #[test]
    fn retained_world_package_filter_keeps_its_config_until_discard() {
        let (directory, opts) = project();
        let root = directory.path();
        for name in ["old-packages", "new-packages"] {
            std::fs::create_dir(root.join(name)).unwrap();
            std::fs::write(root.join(name).join("lib.typ"), "package bytes")
                .unwrap();
        }
        let mut prepared = pipeline::prepare(&opts).unwrap();
        prepared.config.typst.package_paths = vec![root.join("old-packages")];
        let mut session = session(opts.clone());
        session.ready_world(Some(&prepared), &opts).unwrap();
        std::fs::write(root.join("writer.toml"), "[pdf]\nsystem_fonts = false\n[typst]\npackage_paths = ['new-packages']\n").unwrap();
        session.ready_world(None, &opts).unwrap();
        let old = typst::syntax::RootedPath::new(
            typst::syntax::VirtualRoot::Project,
            typst::syntax::VirtualPath::new("old-packages/lib.typ").unwrap(),
        )
        .intern();
        session.worlds.current().unwrap().file(old).unwrap();
        assert!(session.accessed().is_empty());
        assert!(
            session
                .world_config
                .as_ref()
                .unwrap()
                .0
                .typst
                .package_paths
                .contains(&root.join("old-packages"))
        );
        session.discard_world();
        assert!(session.world_config.is_none());
        session.ready_world(None, &opts).unwrap();
        let new = typst::syntax::RootedPath::new(
            typst::syntax::VirtualRoot::Project,
            typst::syntax::VirtualPath::new("new-packages/lib.typ").unwrap(),
        )
        .intern();
        session.worlds.current().unwrap().file(new).unwrap();
        assert!(session.accessed().is_empty());
        assert!(
            session
                .world_config
                .as_ref()
                .unwrap()
                .0
                .typst
                .package_paths
                .contains(&root.join("new-packages"))
        );
    }
}
