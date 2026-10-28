# T23 — Serve CLI, retained session and selective rebuild loop

Frozen implementation brief, coordinator, 2026-10-28.

| | |
|---|---|
| Milestone / size | M2 / S in PLAN; the two-view session makes implementation larger than presenter's original single-view wiring |
| Depends on | T1A, T20, T21, T22, T25; complete/check T25 before dispatching T23 |
| Primary ownership | `src/cli/serve.rs` (replace T23 stub) |
| Proposed private split | `src/cli/serve/session.rs`, only with the coordinator decision below; otherwise retain private Session and helpers in existing serve.rs |
| Tests / fixtures | `tests/cli_serve.rs`, `tests/browser_serve.rs`, `tests/fixtures/serve/**`; private session/source-diff tests |
| Required additive edits | Remove only the serve case from `tests/cli_build.rs::command_stubs_exit_2`; record real-server QA in `docs/dev/live-reload-qa.md`; optional narrow shared watcher-helper visibility seam below |
| Binding sources | PLAN §3.2, M2 T23, §6–7; live-reload.md §2.3, §2.5–2.6, §5.2–5.5, §6, §8–9, Fixtures/Deviations; cli.md serve/global flags; html-output.md serve hooks |
| Lift sources | `../presenter_rs/src/cli/serve.rs`, `../presenter_rs/tests/cli_serve.rs`, corresponding browser harness conventions; strip Deck/reveal/site-preview/Sass-worker behavior not present in Writer |

## Goal and boundaries

`writer serve` starts an HTTP/SSE session that rebuilds web and pages selectively,
keeps each view's last good result, watches every known/currently accessed input,
and survives source/config/view errors and caught pipeline panics. It retains one
WorldSlot, one PagesBuilder, source baselines, stage warnings and live watch set.
No new dependency, public config/IR/pipeline API, CLI flag or frontend/body change.

T21's HTTP/state/event APIs and T20's backend are implemented; T22's client already
works with this wire protocol. T25 supplies the opt-in browser-test client and
must be locally checked before T23 starts. T21's PagesBuilder deliberately returns
an empty successful PageSet. T23 wires both views now; actual pages compilation,
page counts/hashes, pages-only compile failures, and build-watch HTML/PDF mtime
checks remain T24 (after T32). `.typ` branch handling is implemented structurally;
the actual subset frontend/fully working `.typ` serve fixture remains T50/T52.

Do not copy presenter's `--site-preview`, `/assets/` route, Build/Deck types, tagged
BuildEvent, source-version postprocessing or Sass isolation code. Writer's T14/T15
renderer already emits versioned document/theme asset URLs. Appending another
`?v=` as presenter's old version_file_assets does would corrupt existing URLs.

## Existing interfaces to reuse

- Public CLI remains exactly
  `pub fn run(args: &ServeArgs, global: &GlobalArgs) -> anyhow::Result<ExitCode>`.
  `ServeArgs` already includes input, port, host, open, view, theme, template,
  allow_packages. `View -> ServeView` conversion exists. Unknown input extension
  is usage exit2 via `input_kind(input)` (no new --format flag for serve).
- BuildOptions { input, format, config_path, cli, mode }; load_config, prepare,
  WorldSlot::{new,get,current,discard}, web_page, watch_inputs, subset_error and
  BuildFailure are complete. Use `Mode::Serve { build_id }` per attempt.
- `pipeline::web_page` returns WebPage { html,theme_css,theme,diagnostics,assets }.
  Move these resources into T21 WebBuild. Warnings stay separately owned stages.
- T21: ServeState::new(default_view,title), Shared, Outcome/ViewOutcome,
  next_build_id, apply_outcome, event_channel, publish, router,
  serve_until_allowing, AllowedHosts. Do not mutate served slot state by hand.
- T21: PagesBuilder::new/build, PagesInput::{Generated,TypstFile}, ErrorPayload
  ::for_failure/for_input, DiagnosticJson::new and the exact event JSON fields.
- T20: Watcher::start/update, WatchSet::merge, watch_set, watch_set_before_build,
  classify, canonical; std mpsc ChangeBatch sorted/deduplicated by backend.
- report::print_error_in/read_source/print_build_failure support borrowed failures;
  report_diagnostics preserves quiet/verbose severity rules. Errors always print.
  Do not stringify Error just to reconstruct one or lose notebook/include excerpts.
- No `src/serve/session.rs` exists in the frozen module skeleton. Session is a
  private CLI implementation detail; no new public session/stop/test-injection API.

## Startup and lifecycle (faithful presenter lift)

1. Validate input extension/build raw options before starting; unknown format exits2.
   Set config_path from global --config; CliOverrides theme/template/port/host,
   open.then_some(true), view.map(Into::into), allow_packages.then_some(true).
   No writes to document/output paths in serve mode.
2. Create multithread Tokio runtime, named writer-serve, thread_stack_size 8 MiB
   including blocking workers. Install interrupt handler immediately on entering
   async start, before config/watch/build work. Unix signal(SignalKind::interrupt)
   and Windows ctrl_c receivers must be installed synchronously, not lazily by an
   unpolled future. User Ctrl-C after this point exits0, even in first build.
3. Read [serve] once from load_config + CLI. On invalid/unreadable config, warn
   code serve-config, use Config::default().apply_cli, and keep first prepare's
   real error. Start watching before build1 with watch_set_before_build; input
   or config missing can recover without restarting. Watcher-start failure exits1.
4. Default view = explicit/config serve.view, else Pages for SourceKind::Typst,
   Web for Markdown/Notebook. Initial title fallback is input file name; each
   successful prepare replaces it with DocMeta.title or input file name.
5. Spawn first content build on blocking pool, moving Session into job and back.
   Biased select Ctrl-C before completed-job handling; abandon first build on
   interrupt and bind nothing. Failed first build still binds/shows fallback.
6. Resolve/bind configured host/port, trying resolved addresses. For nonzero port,
   probe each resolved address (unspecified at loopback of its IP family) with
   300ms timeout to catch wildcard-listener shadowing on macOS. Port0 skips probe.
   In-use => exit1 with `use --port 0 to pick a free port`; unusable/unresolvable
   host => exit1 with --host hint. Preserve IPv6 brackets in addresses/messages.
7. First stdout line is exactly `Serving http://<bound-address>:<port>/`, also
   with quiet; no build output/Wrote lines precede or follow it. Bind port0 uses
   actual selected port. --open/config open launches on blocking pool via open
   crate; wildcard address changes to loopback for browser URL. Open failure warns.
8. Bridge std mpsc watcher batches to tokio::sync::mpsc on writer-batches thread.
   Async loop receives first + drains queued batches, sorts/dedups/reclassifies
   against current WatchSet, then spawn_blocking one rebuild at a time with full
   Session ownership moved in and returned. Saves during build1 and later rebuilds
   stay queued. Do not hold state locks during parse/render/compile/IO/await.
9. HTTP uses T21 router + AllowedHosts::new(bound).with_name(config.serve.host),
   serve_until_allowing shutdown future. Ctrl-C ends SSE, closes listener within
   SHUTDOWN_GRACE, aborts async rebuild task; runtime.shutdown_background abandons
   any remaining blocking build since serve mode writes nothing. Exit0.

Host/port/open/view are fixed for session lifetime. Changed host/port yields
serve-config warning once per distinct desired address, with config-file path,
following presenter restart_note. Switching back clears remembered note so a
later different request is warned again. Compare configured/effective address
with initial requested config (port0 remains0 for comparison, not bound port).
open/view are still fixed even if no address warning is warranted for their edit.

## Session state and one rebuild

Session privately owns opts/global/listening settings, Shared state, event sender,
watcher/current set/last-good-prepare set, WorldSlot, PagesBuilder, accessed_web,
accessed_pages, sources BTreeMap<PathBuf,String>, text_sources Vec<PathBuf>,
last-good HTML versioned-theme-URL set, and restart-note de-dup state.
Method names/layout are local choices; Session::rebuild denotes the spec operation.

- Merge queued paths via canonical sorted set, classify against full current
  WatchSet (including pages_paths). First attempt is Content. Peek next_build_id,
  set opts.mode with it, start elapsed timer; no separate manual build-id bump.
- Content only: compute source diff before parsing (next section); CSS/Pages
  changed=None. Initial attempt has no old baselines, so changed=None.
- Prepare each attempt except Pages + `.typ`. Call worlds.get(prepared.as_ref(),
  opts) immediately afterward, then world.reset before first render/compile.
  Failed get returns real BuildFailure and fails every scheduled view needing
  World; no fallback config disguises an invalid timestamp/font setup.
- Markdown/Notebook prepare failure -> same ErrorPayload::for_failure cloned to
  each scheduled view; retain stage lists of stages that did not actually run.
  `.typ` prepare failure -> web fails; Pages stage still receives TypstFile path
  and can run with worlds.get(None,opts) as §5.3 defines. Pages-only `.typ` skips
  frontend preparation and web entirely; current/default config World semantics
  are WorldSlot's, not reimplemented in Session.
- Web runs on CSS/Content via web_page; Built moves fields into WebBuild;
  Failed records for_failure. Pages runs on Pages/Content via builder.build
  with TypstFile for `.typ`, Generated{prepared,opts} otherwise. Other view is
  ViewOutcome::Skipped, retaining its old error/last-good resources/warnings.
- Prepare, World acquisition/reset, web stage and pages stage are panic boundaries
  using catch_unwind(AssertUnwindSafe). Panic -> Error::Other exact text
  `the build panicked (a bug in writer): <message>` and worlds.discard(). Preserve
  independent view progress; release World borrows before discard, reacquire/reset
  if a subsequent scheduled stage still needs a World. A failed render panic must
  not terminate HTTP or poison future attempts. Test both &str/String panic data
  and an unknown-payload fallback without adding public production test hooks.
- Exactly once after each actual attempt call typst::comemo::evict(10). Do not
  add eviction in per-render functions, stage helpers or asset handlers.
- CSS→Content event upgrade only if a successful web result's set of versioned
  `/__writer/asset/...?...v=...` URLs in document HTML differs from previous good
  web HTML. Do not include `/__writer/theme.css?v=<build_id>` in that comparison.
  Pages remains Skipped and changed staysNone on this upgrade. Use T14's existing
  bytes-based versions; unchanged file contents keep same version. A failed web
  stage retains old comparison baseline. Ordinary CSS-only rule stays Css.
- Compute per-stage warnings, successful-prepare title and changed mapping, then
  apply_outcome atomically exactly once. Build_id includes failures/skips.
- Retain per-stage accessed sets: replace accessed_web after web ran (even failure
  after reads), accessed_pages after pages ran; skipped keeps its prior set.
  world.accessed is cumulative since reset; union both after filtering paths under
  configured package roots (resolved from document base, matching WorldSlot) and
  FsPackages system data/cache roots. Preserve debug writer-doc/theme watches.
- Prepare success: watch_set(watch_inputs(prepared),accessed_union,None), save as
  last_good set. Failed prepare: watch_set_before_build ∪ last_good ∪ accessed.
  Also preserve T20's approved known diagnostic/missing-resource paths so initial
  failures can recover when a missing resource/include/theme is created. Skipped
  prepare: current set ∪ accessed. watcher.update errors warn and do not stop.
- Record latest successful prepared source/include order and baselines as below.
  Print timing/error/diagnostic reports, then publish completed event. Startup
  first build need not publish before subscribers exist (harmless if it does).

## Changed source position — exact §2.3

- text_sources = canonical main input then Document.includes of latest successful
  prepare, preserving include order; sources holds last-read text by canonical path.
  Only document extensions md,markdown,ipynb,typ count, not images/config/theme.
- At start of Content attempt, visit old text_sources in order if path in batch.
  Read each, skip unreadable while retaining old text. Store all readable new text
  even after first changed candidate found. New path without baseline stores text
  and supplies no candidate. First source yielding a position wins.
- Text: compare `old.lines()` vs `new.lines()` (Rust semantics); equal sequences,
  CRLF↔LF and final-newline-only changes have no position. First differing index
  i, or shorter length for prefix, gives line=min(i+1,max(1,new line count)).
- Notebook: parse both JSON Values, get cells arrays. Whole-object cell equality
  (including cell metadata/outputs) determines first differing i; ignore top-level
  notebook metadata if cells equal. Prefix uses shorter length; clamp i to new
  cells.len()-1. Invalid JSON/missing arrays or new empty cells => no position.
- After build, candidate becomes Changed only if prepare succeeded and canonical
  path equals new main/source0 or new includes[k-1]/sourcek. Removed source/no
  prepare => null. Source index uses new include order, including symlink aliases.
  Text uses lineSome/cellNone; notebook cellSome/lineNone. Serialize exactly one.
- file is `/`-separated, relative strictly below cwd (including canonical/symlink
  comparison fallback), otherwise absolute; use §2.3 rule, not cli::display_path's
  outside-cwd `../` shortening. ErrorPayload/DiagnosticJson already follow this
  browser rule. Baseline newly discovered sources after each successful prepare;
  do not overwrite existing baselines outside the content diff step.

## Stage warnings and terminal reporting

- Browser warning lists contain only Level::Warning/Error converted by
  DiagnosticJson::new(base=cwd). Info never sent, still stderr with -v.
- Stage ran -> Some(list), successful or failed, including Some(empty) clearing it.
  Stage skipped/not entered after failed shared prepare -> None preserving it.
  Prepare list updates on prepare success/failure; skipped `.typ` pages -> None.
- Remove exactly the diagnostic whose location became an ErrorPayload: first
  Level::Error compile diagnostic used by for_failure; subset_error diagnostic
  removed from prepare list when it fails web; preserve unrelated same-code or
  same-message findings. A true terminal parse/config failure with no diagnostic
  has no invented warning. Error-level duplicate-label etc remain nonfatal.
- apply_outcome stores lists, stable exact six-field duplicate suppression across
  prepare++web++pages is T21's responsibility. Old errors on skipped view continue
  global Error status. Partial successful stage output remains served.
- Rebuild stderr (unless quiet):
  `rebuilt in <n> ms (<css|pages|content>)`
  or `(<kind>; web failed)`, `(<kind>; pages failed)`,
  `(<kind>; web and pages failed)`. Fail suffix reflects this attempt's failed
  outcomes, not old error held by skipped view. Error reports always print once
  per distinct failure/payload, using report.rs with real excerpts; do not print
  a shared prepare error twice. Print stage diagnostics with ordinary visibility
  and avoid reprinting same diagnostic across stages. No Wrote/stdout progress.

## Coordinator decisions to freeze before implementation

1. **Private session organization.** Recommended: approve new private child file
   `src/cli/serve/session.rs` plus `mod session;` only in owned src/cli/serve.rs.
   Runtime/interrupt/binding remains parent file, Session/source-diff/warning/watch
   state lives in child; this enables independent implementation/review and keeps
   a large two-view state machine away from launcher code. It adds no public API
   and does not touch frozen cli/mod.rs or serve/mod.rs. Record ownership addition
   in PLAN §3.2 and affected brief/spec coordinator note per §6.4. A private inline
   module/Session in existing serve.rs is a valid fallback requiring no module
   seam, but single-file ownership must then serialize concurrent edits.
2. **Shared watcher package/path helper.** T20's accessed_files, failure_paths and
   package_roots are private. Prefer a narrow coordinator-approved crate-private
   helper/re-export in watch.rs to reuse tested filtering/failure-path recovery,
   rather than quietly making them public or copying divergent package roots.
   If no seam approved, local pure equivalents are permitted with tests and no
   public contract changes; still resolve configured roots from document base.
3. **Panic continuation.** Freeze that each caught stage panic discards World,
   fails only scheduled views depending on that failed stage, and independent
   remaining stages may reacquire a fresh World. A Markdown prepare panic blocks
   both scheduled views; `.typ` prepare panic need not block TypstFile pages.
   This is the practical interpretation of §5.4's per-stage panic boundaries.
4. **QA ownership.** Approve additive real-server results in existing T22-owned
   docs/dev/live-reload-qa.md as live-reload.md §8 explicitly requires. Browser
   fixture/harness uses T25's final checked APIs; no dependency or CLI edit.

## Tests and acceptance

- Private units: exact diff matrix first/middle/last/append/delete/empty/equal,
  CRLF/final newline; notebook cell edit/insert/delete/metadata/invalid JSON/
  empty new cells; first changed source by include order, all baselines updated,
  new include no candidate, removed include null, reordered/new indices, symlink.
- Batch merge/classification includes Pages and CSS/Pages mixtures; timing strings
  every failure combination; config options/default view and restart-note de-dup;
  versioned theme-HTML URL comparison excludes theme.css and retains on error.
- Unit/in-process session coverage for per-view selective calls, exactly one
  eviction, retained/recreated WorldSlot semantics (reuse T16 tests as applicable),
  caught prepare/web/pages panics and next-attempt recovery; stage warning
  Some/None behavior, Info filtering, payload diagnostic removal and stable dedup;
  package-root/accessed retention, first failed resource/config recovery watch set.
  Use private test closures/seams where genuine failures cannot yet be triggered
  with T21's empty pages stub, not pretend real compiler pages run in T23.
- New `tests/fixtures/serve/`: document.md title/include/chapter≥5lines,≥60 real
  paragraphs, figures/plot.png + plot-v2.png valid distinct bytes, child theme+
  styles/extra.css, writer.toml `[html] theme='theme'`; notebook.ipynb with markdown
  cell0/codecells1–3 saved outputs; math-image.md inline/display image math. T24 adds
  template.typ/PDF template settings and true multi-page checks later.
- CLI child harness lifts presenter: isolated temp copies, freeport0 URL from first
  stdout line, ureq3 proxyNone, all startup/HTTP/rebuild/event waits≤10seconds,
  kill/reap and reader/SSE-thread joins on Drop. No60second waits from oldlift.
- Integration all unmarked live-reload §9 steps: initial hooks/state1/hello equality,
  main lastparagraph lineChanged, include line3/source1, CSSskip pages/nochanged,
  transitive include failure retains goodHTML and sharedpayload thenfix clears,
  edited image stableid/newv/newbytes, title/config changes, notebookcell2changed,
  math-image every generated SVG URL200, viewflags/default/query, quiet URLonly,
  missing/invalid startupconfig recovery including new ancestors, missingtheme
  creation, warningpanel @nope clears, nonfatal error-level diagnostics stay served,
  used localhost/wildcard port exit1hint, invalidhost exit1hint, unknownext exit2.
- Unix SIGINT tests: active SSE shutdown exits0 and closes stream; Ctrl-C during
  first build exits0 with no URL (synchronize on first-build tracing log, not an
  arbitrary sleep). Windows compile path uses native signal receiver; do not add
  Unix-only command assumptions to all-platform tests.
- Browser-tests feature: actual CLI+realserver (T25client), no uncaught console
  exceptions, visible edit/TOC/deletedanchor offsets±1px, offscreen source scroll,
  unchanged KaTeX DOM identity and editedrerender, edited image/requestselectivity.
  Complete manual/scripted realserver checklist items1–10,12–16 and record evidence
  paths/results in QA document. Items11,17–19 remain T24, explicitly marked.
- Preserve all other milestone stubs. Root serial fmt, alltarget/allfeatureClippy
  denywarnings, debug/release fullsuite, deniedrustdoc, basic/mixedbuildsmoke and
  browser featuretests. Local macOS native watcher/sockets may need approved
  serviceaccess; no sandboxfallback productionchanges. PR lists narrow seams,
  deferred T24/T52 behavior and inherited PR merge order; continue next milestone.

## Frozen coordinator decisions (2026-10-28)

All four choices above are approved for the T23 feature branch, after T25 is
locally checked. Add private `src/cli/serve/session.rs` with `mod session;` only
in the owned parent file; record that ownership addition in PLAN and the spec
when the branch starts. No frozen public module lists change.

Reuse `watch::accessed_files(&WriterWorld, &Config, &Path) -> BTreeSet<PathBuf>`
and `watch::failure_paths(&BuildFailure) -> Vec<PathBuf>` through crate-private
visibility changes. `package_roots` and `outside_packages` stay private; the
accessed-files helper already performs their filtering. Preserve their tested
semantics and note the narrow additive ownership seam.

Keep independent stage panic boundaries and reacquire/reset a discarded World
when the next scheduled stage can proceed. Add private test seams only where
needed to exercise real recovery behavior; no public injection/stop API.
Additive real-server QA notes in docs/dev/live-reload-qa.md are approved.

## Temporary implementation handoff (coordinator, 2026-10-28)

T25's provider APIs have passed all 28 client units, six actual Chrome tests,
all-feature compilation/Clippy and feature rustdoc. Its ordinary regression
suites finish before its PR/branch handoff. Independent T23 implementation may
be prepared under /private/tmp now; repository writes wait for codex/serve-cli.

The parent/child private interface is frozen for parallel implementation:
`pub(super) struct Session`, `Session::new(opts: BuildOptions,
 global: GlobalArgs, watcher: Watcher, listening: &ServeConfig) -> Self`,
`Session::router(&self) -> axum::Router`, and
`Session::rebuild(&mut self, queued: Vec<ChangeBatch>)` (returns unit, reports
and publishes; empty queue denotes the first Content build). Session owns all
shared state/events/current WatchSet; parent needs no direct field access.
Parent traces the input path before moving options into Session. Queue merge
and classification live in Session. No new public API is introduced.


## Acceptance result (2026-10-28)

Formatting, all-target/all-feature Clippy, default/feature denied-warning
rustdoc, 1,283 debug and 1,282 release tests pass. All 32 launcher/session
units, 16 real CLI tests and nine actual Chrome server cases pass. Shared
fixtures include 64 paragraphs, valid distinct images and a saved notebook.
Reading offsets remain within one pixel; math/image identity, error/warning
recovery, view/reload/reconnect behavior and SIGINT lifecycle are verified.
Measured results are in docs/dev/build-times.md and real-server checklist
evidence in docs/dev/live-reload-qa.md. Basic/mixed build sizes are unchanged.
T24 pages compilation and T50/T52 Typst input behavior remain explicit future
provider boundaries; T23's private stub-dependent test is isolated for T50.
Remote CI runs independently while T50 follows.
