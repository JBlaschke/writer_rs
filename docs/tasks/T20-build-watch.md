# T20 — File watcher, watch sets, and `build --watch`

Frozen implementation brief, coordinator, 2026-10-27.

| | |
|---|---|
| Milestone / size | M2 / M |
| Depends on | T16, already present |
| Primary ownership | `src/serve/watch.rs` (currently only its module comment) |
| Required additive edits | `src/cli/build.rs`, narrow reporting seam in `src/cli/report.rs`, narrow pipeline seam described below, removal/replacement of `tests/cli_build.rs::watch_is_a_stub` |
| New tests | `tests/watch.rs`; private unit tests in the owned watcher module and narrow seam tests |
| Binding sources | PLAN §3.2/§5 T20/§6–7; `docs/spec/live-reload.md` §5.2–5.3, §6–7, §9, Fixtures/Deviations; `docs/spec/cli.md` build |
| Lift sources | `../presenter_rs/src/serve/watch.rs`, `../presenter_rs/src/cli/build.rs`, `../presenter_rs/tests/watch.rs` |

## Goal

A long-running build watches the document, includes, project resources, themes,
and compiler-accessed files. Saves rebuild only affected requested targets,
without rebuilding on the program's own outputs or on ordinary reads. The
same watcher/watch-set/classification API is ready for T21/T23 serve work.
No server, SSE, pages renderer or new dependency is part of T20.

## Existing seams, checked in the current tree

- `pipeline::InputRole`, `ChangeKind::{Css, Pages, Content}`, `BuildOptions`,
  `Prepared`, `BuildFailure`, `BuildReport`, `build_targets`, `watch_inputs`,
  `WorldSlot::{new,get,current,discard}` are implemented, not placeholders.
- `watch_inputs(&Prepared)` already returns source, transitive includes,
  project config, file assets, missing-asset diagnostic paths, bibliography /
  CSL, PDF template, DOCX reference, LaTeX template, and resolved/fallback
  theme-chain directories. Reuse it; do not duplicate its resource origins.
- `build_targets` prepares once, obtains/resets the shared World, writes each
  target independently and returns all target results. PDF, LaTeX, Typst
  source and `.typ` source target bodies remain later milestones, while
  HTML and T41 DOCX work. Keep future-target stubs explicit.
- `WriterWorld::accessed()` returns a `BTreeSet<PathBuf>` of real paths since
  its last reset, including filesystem packages and mounted file origins.
  It does not filter package roots. The watcher caller must remove files
  below effective `config.typst.package_paths` and
  `typst_kit::packages::FsPackages::{system_data,system_cache}().path()`.
  Debug `assets/typst/writer-doc` is added as an explicit recursive watch.
- `markdown::include::scan_includes` is complete (including PR18's raw
  Windows-path fix), public and tolerant. It scans transitive `.md` syntax
  without a full document parse. Do not revive the old normalized-info bug.
- `cli/build.rs::run` already branches to `watch`; only that function is a
  T20 stub. `build_options` currently loads config before returning options,
  and `build_result` currently consumes `BuildReport`.
- `notify = 8.2`, `typst-kit`, `tokio`, `walkdir` and tempfile test support
  are already dependencies; use the existing `typst::comemo` re-export for
  cache eviction. No debouncer, Ctrl-C or encoder crate
  needs to be added.

## Frozen public API to implement

```rust
pub use crate::pipeline::ChangeKind;
pub const DEBOUNCE: Duration = Duration::from_millis(150);
pub const MAX_BATCH_WAIT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Debounce { pub quiet: Duration, pub max_wait: Duration }
// Default uses the two constants.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchSet {
    pub files: Vec<PathBuf>,
    pub dirs_recursive: Vec<PathBuf>,
    pub dirs_flat: Vec<PathBuf>,
    pub theme_dirs: Vec<PathBuf>,
    pub pages_paths: Vec<PathBuf>,
}
impl WatchSet { pub fn merge(&mut self, other: &WatchSet); }

pub fn watch_set(inputs: &[(PathBuf, InputRole)], accessed: &[PathBuf],
    targets: Option<&[Target]>) -> WatchSet;
pub fn watch_set_before_build(opts: &BuildOptions) -> WatchSet;
pub fn classify(paths: &[PathBuf], set: &WatchSet) -> ChangeKind;
pub fn is_ignored(rel: &Path) -> bool;
pub fn canonical(path: &Path) -> PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeBatch { pub paths: Vec<PathBuf>, pub kind: ChangeKind }

impl Watcher {
    pub fn start(set: &WatchSet, tx: Sender<ChangeBatch>) -> Result<Watcher>;
    pub fn start_with(set: &WatchSet, tx: Sender<ChangeBatch>,
        debounce: Debounce) -> Result<Watcher>;
    pub fn update(&mut self, set: &WatchSet) -> Result<()>;
    pub fn watched_dirs(&self) -> Vec<(PathBuf, bool)>;
}

pub enum WatchStatus<'a> {
    Built(&'a BuildReport),
    Watching(&'a WatchSet),
    Changed(&'a ChangeBatch),
}
pub fn run_build_watch_with(opts: &BuildOptions, targets: &[Target],
    report: impl FnMut(WatchStatus<'_>) -> ControlFlow<()>) -> Result<()>;
```

`WatchSet::new(files, themes)` is an optional additive convenience from the
lift, useful to tests; if retained, initialize `pages_paths` empty. All lists
are canonical, sorted and deduplicated; merge also merges `pages_paths`.
Do not copy presenter's old two-variant `ChangeKind` or Build/Deck APIs.

## Watch-set and classification behavior

- Every ordinary input and every supplied, already package-filtered accessed
  file goes in `files`; watch parents non-recursively via `dirs_flat`.
- `Theme` entries are recursively watched directories and populate
  `theme_dirs`. Debug builds also recursively watch the embedded default
  theme and writer-doc package directories when present. The latter is in
  `pages_paths`, not `theme_dirs`.
- `PdfTemplate` is a watched file and `pages_paths` entry. ReferenceDoc is
  retained only for a `Some(targets)` containing Docx; LatexTemplate only
  for Latex. Both are excluded for serve (`None`). PDF template remains
  watched even for HTML-only build watch, so a pages-only batch can be
  deliberately ignored by empty target intersection.
- Before build: source, explicit/discovered writer.toml even if invalid,
  Markdown scan-includes (honor `opts.format` overrides), debug directories,
  and discoverable project/CLI theme directories. Missing directories are
  kept lexically with canonical existing ancestors. No full duplicate parse.
- Css iff nonempty and every path has case-insensitive css/scss/sass extension
  or lies under a theme's `styles/` or `assets/`. Pages iff nonempty and every
  path equals/is below a `pages_paths` entry. Otherwise Content, including
  empty input and CSS/pages mixtures. Use CSS check first for overlaps,
  matching §6.2's order.
- Only `.git` and the specified editor/OS files are ignored recursively.
  Preserve `dist` and `target` changes. Explicitly listed files count even
  if their names look like editor files, as in presenter's Filter.

## Mechanics to lift faithfully

Lift canonicalization/Filter/watch planning/guard watches/coalescer/catch-up
mechanics from the presenter, renaming the thread `writer-watch` and passing
full WatchSet classification context (including pages paths) through shared
state. Keep the following details, which were platform bug fixes:

- All create/modify/remove/rename/metadata events count. Access-event exception
  needs the coordinator clarification below.
- Debounce 150 ms quiet, no later than 2 s after first relevant event; unrelated
  traffic cannot postpone a due batch. Batches sorted/deduplicated.
- Flat directories filter unrelated files. Recursive directories honor only
  the specified ignore list. Rescan/lost-event reports trigger a conservative
  batch of listed inputs.
- Missing paths watch the nearest existing directory ancestor; directory
  recreation becomes a structural event and `update` re-adds watches.
  Windows parent guard watches are required to see removal/recreation of a
  watched directory. `watched_dirs` omits guard-only parents.
- `update` changes filter before backend watches; catches up files modified
  since the last sent batch, including newly discovered files. Retain poisoned
  mutex recovery and robust notify error handling from the lift.
- macOS alone filters stale FSEvents using subsecond inode ctime before the
  last sent batch/start; whole-second times never qualify. Do not apply this
  filter to Linux or Windows.
- Dropping Watcher stops backend/coalescer; pending changes flush and sender
  closes. Watcher remains Send. Existing-directory watch errors fail start;
  update failures after start are warnings while other watches still update.

## Rebuild loop and CLI behavior

One retained WorldSlot per invocation. Clone options into Mode::Build.
Start watching before first build, then emit Built, Watching, and thereafter
Changed + Built for effective nonempty target subsets. Callback Break and
channel closure return Ok; content/target errors are reported as Built and
do not end the loop. Only watcher start fails the library function.

Use last effective successful config's PDF engine to select targets:
Css -> HTML and Chrome-engine PDF; Pages -> Typst and Typst-engine PDF;
Content -> all requested. Intersect with original deduplicated target order.
Ignore a batch with empty intersection before Changed/report/build. Merge
queued batches and reclassify against the current set.

After each actual build call `comemo::evict(10)` once. On successful prepare,
watch `watch_inputs(prepared)` plus current filtered accessed paths, unioned
with the last PDF build's accessed set when this subset did not run PDF.
Retain the PDF observations until another PDF build replaces them. A failed
prepare uses pre-build set union last good set; known missing-file paths
need the coordinator clarification below. Update even after target errors.
Ignore every successfully written path and descendants of written
asset/site directories; retain per-target last successful output roots so
partial rebuilds cannot turn a previous target's output into an input event.
Preserve presenter's suppression of ancestor-directory notifications caused
by writes inside a recursive theme watch without suppressing theme roots.

CLI uses existing normal-build diagnostic/output formatting. Print no status
lines with quiet; otherwise exact `Watching for changes (Ctrl-C to stop)`
after first build and `Rebuilding (<css|pages|content>): <up to 3 paths>[,
and <n> more]` before each rebuild. Diagnostics stderr; Wrote stdout. A
failed initial build can be fixed without restarting. Preserve initial
output-clash refusal as usage exit 2, and later clash reporting without
ending a live watch, as in the presenter. Invalid startup config should
be watched until repaired (see coordinator choices).

## Required coordinator choices and narrow seams

1. **Original multi-target output semantics (concrete pipeline gap).**
   `build_targets` passes the selected subset's length to `output_path`.
   With original `--to html,docx -o DIR`, a CSS subset `[Html]` would treat
   DIR as the HTML file and fail. Approve a crate-private pipeline helper
   carrying the original deduplicated requested target count, used by watch;
   public `build_targets` delegates with its own count and keeps its API.
   Do not parse once per target or change BuildOptions/public target APIs.
   Test multi-target explicit output on a CSS-only rebuild.
2. **Borrowed build reporting.** `WatchStatus::Built` borrows BuildReport,
   while CLI `build_result`, `report_build_failure` and `locate` currently
   consume reports/errors. Approve a small borrowed reporting helper in
   `cli/report.rs` (including source-ordered missing-asset handling) and
   reuse CLI's existing Wrote/clash formatting. Do not clone Error via
   strings, change frozen WatchStatus to own BuildReport, or lose excerpts.
3. **Access events.** Binding §6.3 says every event kind counts, but required
   presenter code deliberately ignores EventKind::Access: inotify emits
   open/read events from the build itself, causing an infinite rebuild loop.
   Freeze “every mutation event counts; pure access events do not,” matching
   the lift, and add a Linux-relevant no-self-rebuild regression/spec note.
4. **Known paths on first failed build.** A missing asset is escalated in
   `build_targets` into `BuildReport.prepared: Err`, discarding the otherwise
   parsed Prepared. The literal before-build-union-last-good rule cannot
   watch its creation on an initially failing build (there is no last good
   set). Approve also merging Error::MissingAsset's path and paths from
   failure diagnostics, without reparsing; test creating that asset repairs
   the output. More elaborate partial-Prepared retention is unnecessary.
5. **Startup invalid config.** Current CLI build_options fails before any
   watcher can start. Preserve presenter's wait-for-config behavior through
   a small raw-options helper, tolerant pre-build set and retry loop until
   effective target defaults can be selected. Explicit `--to` stays fixed
   for the session; default selected targets likewise freeze once available.
   Existing usage errors (unknown format, .typ-to-typst) still return 2.
6. **Touched ownership.** Approve additive CLI reporting/pipeline helpers and
   replacement of the old watch stub test. WatchSet's helper organization and
   local retained loop state remain implementation choices. No changes to
   IR, config, WorldSlot, include scanner or server modules are required.

## Tests and acceptance

Port presenter's real-backend watch tests into temp directories, adapting
Build/Deck helpers to Writer's Prepared/BuildReport API. Every wait bounded
(typically 5 s per batch, at most 10 s); Child Drop kills/reaps the process;
settle/drain startup FSEvents rather than assuming one backend event.

- Unit: classification CSS/pages/content/mixed/empty and precedence; every
  InputRole/target filter; canonical/symlink/missing paths; sorted merges;
  ignore grammar; debounce timing/rescan/irrelevant-traffic bounds;
  macOS-only ctime decisions, Windows structural guards, Send.
- Backend integration: burst coalescing, rename-replace plus next edit,
  irrelevant files, editor files, dist/target included, update new set,
  watcher drop closes channel, missing directory two levels deep,
  removed/recreated source/theme directory, catch-up during update.
- Build loop: initial Built/Watching and Changed/Built order, callback Break,
  initial parse/config failure recovery, first missing asset creation,
  transitive include fix, changed during first build, output-inside-theme
  does not loop, retained PDF accessed paths, package-path filtering,
  per-target output roots, and correct explicit multi-target output naming.
  Future target bodies can be stubs; do not assert PDF generation now.
- CLI: temp document + child theme/styles/x.css + template.typ + writer.toml;
  `build --watch --to html` writes HTML, CSS and content edits rewrite it,
  template-only pages batch does not rewrite (mtime unchanged). Include
  quiet/status strings and startup invalid-config repair. Remove only
  `watch_is_a_stub`, preserving all other milestone stub tests.
- Add HTML+PDF mtime test only in T24 once PDF is available, as the spec says.
  T20 can use HTML+DOCX or HTML+future Typst stub for output-count regression.
- Existing normal build, HTML, DOCX and include suites continue to pass.
  Root serial checks: fmt, all-target/all-feature Clippy denied warnings,
  debug/release tests, denied rustdoc warnings, basic/mixed build smoke.
  PR describes added ownership seams and any open inherited dependency PRs.


## Frozen coordinator decisions (2026-10-27)

All six narrow seams above are approved. Keep the public API unchanged and
record the access-event clarification in live-reload.md on the T20 branch.

- Add `pipeline::build_targets_with_count(opts, targets, worlds,
  requested_count)` as crate-private, delegating normal `build_targets` with
  its deduplicated count. The watch loop passes the original deduplicated
  count, including subset rebuilds. No public options/config shape changes.
- Add borrowed error/report helpers instead of cloning errors or changing
  WatchStatus ownership. Preserve ordinary diagnostic levels/source excerpts,
  Wrote/status streams and output-clash usage formatting. Existing normal
  build behavior stays covered; reuse helpers where it makes code clearer.
- Ignore pure Access events to prevent inotify reads causing rebuild loops.
  Every create/modify/remove/rename/metadata/rescan event still counts.
- On failed preparation, merge the before-build set, last good set, terminal
  Error::MissingAsset path and all known diagnostic input paths. Do not parse
  again merely to reconstruct Prepared. New directories retain ancestor
  watches, so creation can recover the first build as well as later ones.
- Startup invalid config waits using the pre-build set and raw CLI options,
  following the presenter behavior. Explicit --to is fixed immediately;
  configured defaults freeze when the first usable config is available.
  Existing usage errors and initial output clash remain exit 2; later build
  failures are reported while watching continues.
- Root owns pipeline/count and CLI/report seams, docs/task board and CLI
  watch subprocess tests; a watcher agent owns serve/watch.rs and its unit
  tests; a test agent owns tests/watch.rs backend/build-loop integration.
  Keep helper APIs private where possible. Root serializes Cargo/rustfmt.
  No dependency/module-list/IR/config changes and no server/SSE implementation.

T20 starts from the checked T41 branch after its milestone PR is open.
All PRs target main, listing inherited open PRs and merge order. Do not wait
for their CI or merge before implementation of the next ready milestone.


## Implementation refinements (coordinator, 2026-10-28)

- Tests are split into `tests/watch.rs` (real backend), `tests/watch_loop.rs`
  (bounded child processes calling the library callback API), and
  `tests/cli_watch.rs` (CLI/reporting). No test-only public stop API is added.
- Missing-path canonicalization reuses `pipeline::path_after_creating_dirs`
  so existing symlink components keep OS `..` semantics. Unix unit and
  real-backend regressions distinguish the actual target from a lexical alias.
- Configured package roots resolve against the document base directory,
  matching `WorldSlot`/`WriterWorld`, before accessed-file filtering.
- Startup missing-config retries update the watcher after each failed retry,
  following newly created ancestor directories. An initial multi-target clash
  prints the complete build report once before returning usage exit 2, so
  successful target output and independent target errors remain visible.
- This desktop's macOS sandbox blocks FSEvents startup. Real backend/CLI/loop
  and full test runs use approved native-service access; CI requires no
  privileged test code or sandbox-specific production workaround.


## Acceptance result (2026-10-28)

Implemented and locally checked: formatting, all-target/all-feature Clippy,
rustdoc with denied warnings, 1,182 debug tests and 1,181 release tests pass.
The release suite was rerun after crash recovery. Basic/mixed HTML and basic
DOCX smoke builds pass at unchanged sizes. Backend, loop and CLI tests cover
the frozen contracts, including startup missing nested config recovery and
complete reporting of mixed target success/clash/stub results. Detailed
coverage and measured times are in `docs/dev/build-times.md`. T24 retains the
HTML/PDF mtime regression until PDF exists. Remote CI runs independently.
