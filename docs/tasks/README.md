# Task board

Each file in this directory is one unit of work sized for a single agent. Read
[`../../PLAN.md`](../../PLAN.md) §3, §4 and §6 first, then your task file, then the spec
sections it lists. Specs in `../spec/` are contracts: implement them, do not reinterpret them.
Research notes in `../research/` are reference material for the "Verified API hints".

## How to pick up a task
1. Check the status table below and the dependency column. Dependencies normally
   must be **done** (merged to `main`). The user authorized
   stacked feature PRs on 2026-10-26: a completed, locally checked dependency
   branch may be used while CI and merging proceed independently.
2. Tell the coordinator (or set the status to `in progress (<handle>)` in a tiny PR when the
   coordinator asks for that).
3. Branch `task/Txx-slug`. Edit only the files your task owns (PLAN.md §3.2 is the map);
   additive changes elsewhere must be called out in the PR. Integration files (`src/pipeline/mod.rs`,
   `src/cli/mod.rs`, the frontend stubs) are replaced body-only, never restructured.
4. Done means: every acceptance criterion has a test or a documented manual check,
   `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
   `cargo test`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass, and the PR
   description lists any criterion you could not meet.

## Status

Generated from PLAN.md §5 (titles shortened); PLAN.md is authoritative.

| ID | Task | Milestone | Size | Depends on | Status |
|---|---|---|---|---|---|
| T00 | Repository scaffold, pinned dependencies, CI, vendoring, module skeleton | M0 | M | — | done (PR #1) |
| T01 | Core IR, config, errors, asset registry, l10n, CodeAttrs, frontend stubs, fixtures | M0 | L | T00 | done (PR #2) |
| T03 | typst compiler embedding: World, fonts, packages, compile, renditions, MathML | M0 | L | T01 | done (PR #3) |
| T04 | DOCX package core: ZIP, parts, styles, numbering, builders, reference merge, validator | M0 | L | T01 | done (PR #4) |
| T10 | Front matter → DocMeta and config overrides | M1 | S | T01 | done (PR #5) |
| T11 | Markdown foundations: comrak options, attributes, recovery, directives, colon pre-pass | M1 | M | T01 | done (PR #5) |
| T12 | comrak AST → IR walker | M1 | L | T10, T11, T14 | done (PR #6) |
| T13 | HTML emitter and syntect highlighting | M1 | L | T01 | done (PR #7) |
| T14 | Asset emitter with versioned serve URLs | M1 | S | T01 | done (PR #5) |
| T15 | Theme chain, document template, context, helpers, styles | M1 | M | T03, T13, T14 | done (PR #8) |
| T16 | Pipeline and CLI tree with stubs, `build` + `dump`, example, e2e snapshots | M1 | M | T03, T10, T12, T13, T14, T15, T17 | done (PR #9) |
| T17 | Resolve pass: labels, numbering, references, footnotes, TOC | M1 | M | T01 | done (PR #7) |
| T18 | Notebook frontend | M1 | L | T12, T13, T30, T40 | done (PR #17) |
| T19 | Default HTML theme v1 | M1 | M | T15, T16 | done (PR #11) |
| T1A | Math in HTML (KaTeX, MathML, image routes) | M1 | M | T03, T15, T16, T30 | done (PR #14) |
| T1B | References/citations syntax and `:::include` | M1 | M | T12, T16, T18 | done (PR #18) |
| T1C | `writer check` with Markdown and notebook lints | M1 | S | T16, T1B | done (PR #19) |
| T20 | File watcher and the document watch set | M2 | M | T16 | done (PR #21) |
| T21 | Dev server: routes, events, per-view state | M2 | M | T16 | done (PR #22) |
| T22 | Live-reload client | M2 | M | T00 | done (PR #16) |
| T23 | `serve` CLI, session runtime, integration tests | M2 | S | T1A, T20, T21, T22, T25 | done (PR #24) |
| T24 | Pages view | M2 | M | T21, T23, T32 | ready for review |
| T25 | Headless-browser smoke tests (opt-in) | M2 | M | T16 | done (PR #23) |
| T30 | IR → typst emitter with source map | M3 | L | T01, T03, T17 | done (PR #13) |
| T31 | `@writer/doc` typst package | M3 | M | T03 | done (PR #12) |
| T32 | PDF pipeline, diagnostics, `dump --typst`, reproducibility, numbering oracle | M3 | L | T03, T16, T30, T31, T50 | done (PR #26) |
| T33 | Math corpus through mitex, `check` pre-flight | M3 | S | T1A, T1C, T32, T42 | ready for review |
| T34 | BYO typst template and `--to typst` export | M3 | M | T1B, T32 | todo |
| T40 | DOCX body, bookmarks and internal links | M4 | L | T04, T13, T17 | done (PR #15) |
| T41 | DOCX tables, figures, captions, TOC, bibliography; `--to docx` | M4 | L | T16, T40 | done (PR #20) |
| T42 | DOCX math (OMML, numbered equations, image fallback) | M4 | M | T03, T1A, T41, T51 | ready for review |
| T43 | Reference documents and the DOCX validation suite | M4 | M | T41, T42 | todo |
| T50 | typst-syntax → IR walker (structured subset) | M5 | L | T01, T17, T30 | done (PR #25) |
| T51 | typst math → LaTeX converter and the symbol table | M5 | L | T01 | ready for review |
| T52 | typst documents end to end, round-trip golden | M5 | M | T1A, T23, T24, T32, T42, T50, T51 | todo |
| T53 | `#include` of `.md`/`.ipynb` from typst | M5 | M | T18, T1B, T30, T52 | todo |
| T60 | IR → LaTeX emitter | M6 | L | T13, T17, T18, T1A, T32, T50, T51 | todo |
| T61 | LaTeX template mechanism, preamble, assets, `--to latex` | M6 | M | T60 | todo |
| T70 | Citations (hayagriva) and the resolve hook | M7 | M | T16, T17, T1B | todo |
| T71 | `check`: typst and per-target lints | M7 | S | T1C, T33, T41, T42, T52, T61 | todo |
| T72 | `init` scaffold and the `eject` family | M7 | M | T16, T31, T43, T61, T70 | todo |
| T73 | Diagnostics quality | M7 | S | T16, T21, T22, T32 | todo |
| T74 | User documentation | M7 | M | T53, T61, T71, T72, T73 | todo |
| T75 | Release builds and site | M7 | S | T16 | todo |
| T76 | *Stretch:* code and CSV includes | M7 | S | T1B | todo |
| T77 | *Stretch:* Chrome print engine | M7 | M | T16 | todo |
| T78 | *Stretch:* typst HTML export web view | M7 | M | T52 | todo |
| T79 | *Stretch:* SCSS themes and minijinja 3 | M7 | M | T19 | todo |

## Order of execution (waves, computed from the dependency column)

| wave | tasks | parallelism |
|---|---|---|
| 0 | T00 | 1 |
| 1 | T01, T22 | 2 |
| 2 | T03, T04, T10, T11, T13, T14, T17, T51 | 8 |
| 3 | T12, T15, T30, T31, T40 | 5 |
| 4 | T16, T18, T50 | 3 |
| 5 | T19, T1A, T1B, T20, T21, T25, T32, T41, T75, T77 | 10 |
| 6 | T1C, T23, T34, T42, T60, T70, T73, T76, T79 | 9 |
| 7 | T24, T33, T43, T61 | 4 |
| 8 | T52, T72 | 2 |
| 9 | T53, T71, T78 | 3 |
| 10 | T74 | 1 |

## Coordinator notes (2026-10-24)
- T00 merged in [PR #1](https://github.com/JBlaschke/writer_rs/pull/1); all CI
  passed. T01 starts from that merged main commit (`2de291d`).
- T01 merged in [PR #2](https://github.com/JBlaschke/writer_rs/pull/2); all CI
  passed. T03 starts from that merged main commit (`2c9ad24`).
- T03 merged in [PR #3](https://github.com/JBlaschke/writer_rs/pull/3); all CI
  passed. T04 starts from that merged main commit (`bdcd3f0`).
- T04 merged in [PR #4](https://github.com/JBlaschke/writer_rs/pull/4); all CI
  passed. T10, T11 and T14 start from that merged main commit (`4711b50`).
  Their only dependency is the merged T01. T12 waits until this batch merges.
- T10/T11/T14 merged in [PR #5](https://github.com/JBlaschke/writer_rs/pull/5);
  all CI passed. T12 starts from that merged main commit (`cf331a0`). Its three
  dependencies are done; helper contracts and serve cache choices remain recorded
  in the specs and affected briefs.
- T12 merged in [PR #6](https://github.com/JBlaschke/writer_rs/pull/6); all CI
  passed. T13 and T17 start from that merged main commit (`f6852ff`). Their
  dependency T01 is done; reference/include parsing stays with T1B and the CLI T16.
- T13/T17 pass local formatting, Clippy, rustdoc, 550 debug and 549 release
  tests. [PR #7](https://github.com/JBlaschke/writer_rs/pull/7) merged on
  2026-10-26 (`c2b1cf5`). Reference syntax/includes, citation rendering and CLI
  integration retain their task owners.
- The user authorized continuing without CI/merge waits on 2026-10-26. T15
  starts from completed T13/T17 (`9b315d0`) on `codex/theme-rendering`; open its
  PR against `codex/html-and-resolution` while PR #7 is open, or against `main`
  after that dependency merges. Each feature keeps its own PR and merge order.
- T15 passes local formatting, Clippy, rustdoc, 645 debug and 644 release tests.
  Its 25 theme and 23 page-rendering tests cover the lifted APIs and new context
  snapshot. CI/merge proceeds independently; T16 follows on this implementation.
- T15 merged in [PR #8](https://github.com/JBlaschke/writer_rs/pull/8) after CI
  passed (`7ed1de7`). T16 started from T15 (`647fce5`) on
  `codex/build-and-dump`; its PR #9 now targets `main`.
- T16 passes local formatting, Clippy, rustdoc, 733 debug and 732 release
  tests. The basic example builds a standalone HTML file; site/build/dump, world
  lifecycle and 36 new reviewed snapshots pass. T19 follows on this implementation.
- T16 passed CI and merged in PR #9 (`ed9d4d2`). T19 started from completed
  T16 (`22723f2`) on `codex/default-html-theme`.
- Windows CI repairs in PRs #8/#9 guard the Unix-only test helper, box build
  errors and compare CSL paths by components. The first two repairs passed
  734 local debug tests; the path assertions then passed all 26 pipeline tests.
- T19 passes formatting, Clippy, rustdoc, 768 debug and 767 release tests.
  Its 34 new tests cover stylesheet/token/README contracts and the CLI showcase.
  Entries total 13,402 bytes; templates and snapshots are unchanged. In-app
  browser checks cover light/dark, narrow layouts and forced print rules;
  Chrome's paginated print preview remains unverified.
- T19 passed CI in PR #10 and merged into its feature-branch base. The
  follow-up [PR #11](https://github.com/JBlaschke/writer_rs/pull/11) brought it
  onto `main` (`4c0fe16`). T31 started independently from merged T16, depends
  only on completed T03 and records its existing spec in a new task brief.
- Future feature PRs target `main`, including those developed on checked,
  unmerged dependencies. Their descriptions list the dependencies and merge
  order; feature commits or branch comparisons keep reviews focused while
  dependencies remain open. CI waits still do not block implementation.
- T31 passes local formatting, Clippy, rustdoc, 759 debug and 758 release tests.
  Eighteen compiler-backed tests and seven new package helper tests cover the
  bundled document template, numbering/oracle, locales, listings, cells and
  safe ejection. The basic HTML example still builds; PDF/eject CLI wiring
  remains with T32/T72. Its only task dependency, T03, is merged.
- T31 passed CI and merged in [PR #12](https://github.com/JBlaschke/writer_rs/pull/12)
  (`b533020`). Its integration with T19 passes all 793 local debug tests.
  T30 starts from that merged `main` on `codex/typst-emitter` (2026-10-27),
  with its frozen contracts recorded in a new brief. CI waits still do not
  block subsequent feature branches.
- T30 passes formatting, Clippy, rustdoc, 835 debug and 834 release tests.
  Its 32 integration tests on Unix (31 on Windows), ten shared math unit
  tests and reviewed sample snapshot cover complete source emission,
  fragments, split files, source maps, escaping and collision-safe names.
  Source asset paths handle symlink aliases; Virtual/Export remain pure.
  T1A follows on the checked T30 implementation while CI runs.
- T30 is open in [PR #13](https://github.com/JBlaschke/writer_rs/pull/13)
  (`1ea66e3`). T1A starts from that checked feature on `codex/html-math`;
  merge T30 first. Its other dependencies are merged. Both PRs target `main`.
- T1A passes formatting, Clippy, rustdoc, 868 debug and 867 release tests.
  Ten converter unit tests, 23 integration tests and five reviewed snapshots
  cover the 37-equation corpus and all HTML math routes. In-app browser QA
  confirms all 12 Markdown equations render, SVG images remain readable in
  dark mode and MathML ink no longer causes vertical scrollbars. Light/print
  style checks reset the image filter; the basic example builds at 687.6 kB.
- Local implementation started on 2026-10-25 with T00. Local acceptance results and timings
  are in [`../dev/build-times.md`](../dev/build-times.md); the local status does not mean
  the changes are merged or that the three-OS CI matrix has run.
- T30 and T1A passed CI and merged in PRs #13/#14 (`49341bd`/`223fd42`).
  T40 starts from merged `main` on `codex/docx-body`. Its existing frozen
  contract is recorded in a new brief; subsequent feature PRs continue while
  CI runs and all target `main`.
- T40 passes formatting, Clippy, 891 debug and 890 release tests. Nineteen
  integration tests, four helper tests and 99 reviewed package/readback
  snapshots cover the body contracts; generated DOCX packages validate.
  Its only dependencies are merged. T22 is being prepared independently
  while T40 checks/CI proceed; each feature keeps its own PR against `main`.
- T40 is open in [PR #15](https://github.com/JBlaschke/writer_rs/pull/15)
  (`d757100`). T22 follows on that checked commit on
  `codex/live-reload-client`, preserving merge order T40 then T22. Its client,
  mock server and browser QA work independently of the future Rust server.
- T22 passes formatting, Clippy, rustdoc, 891 debug and 890 release tests,
  JavaScript/Python syntax checks and 19 mock/Chrome browser checks. The
  client preserves reading position, unchanged math/images and per-view
  state; QA covers reload/reconnect paths and scripts loaded in the head.
  Real-server and actual pages checks remain with T23/T24.
- T40/T22 passed CI and merged in PRs #15/#16 (`94ad41b`/`43a1ed8`).
  T18 was developed on checked T22 (`b2e7905`); that tree matches merged
  `main`, so its PR has no pending merge dependency.
- T18 passes formatting, Clippy, rustdoc, 981 debug and 980 release tests.
  Sixteen parser tests, 26 notebook integration tests and 21 reviewed
  snapshots cover saved outputs, options, metadata, tables, attachments,
  selection and cross-cell notes. Fifteen browser checks cover the saved
  example in light/dark and narrow layouts. A small notebook theme fix
  contains wide-table scrolling without clipping desktop output prompts.
  T1B follows while CI runs; each milestone keeps its own PR against `main`.
- T18 is open in [PR #17](https://github.com/JBlaschke/writer_rs/pull/17)
  (`b523e05`). T1B starts from that checked feature on
  `codex/references-and-includes`; merge T18 first while both PRs target
  `main`. Its scanner/include brief records the additive context contracts.
- T1B passes formatting, Clippy, rustdoc, 1,028 debug and 1,027 release
  tests. Fourteen scanner unit tests, four frontend integration tests and
  26 include tests on Unix cover syntax, metadata, scoped notes, mixed-file
  recursion and located failures. Twenty-one new/updated snapshots are
  reviewed; two obsolete cycle diagnostic snapshots are removed. Nineteen
  Chrome checks cover the mixed example's images, links and three scoped
  notes in light/dark and narrow layouts. T18 must merge before T1B.
- T1B is open in [PR #18](https://github.com/JBlaschke/writer_rs/pull/18).
  Its CI repair (`9ec2dc4`) preserves original Windows include-path bytes
  and redacts notebook asset origins. All 604 focused local tests, formatting
  and Clippy pass; the remote matrix is rerunning independently.
- T1C passes formatting, Clippy, rustdoc, 1,064 debug and 1,063 release
  tests. Eleven library tests, four CLI unit tests and 16 subprocess tests
  cover severity/exit/quiet/JSON behavior, located failures, compatibility
  checks and no output writes. Twenty-six reviewed snapshots cover all 18
  Markdown and eight notebook fixtures. The three examples check cleanly;
  basic HTML remains 687.6 kB. It includes the checked T1B CI repair.
  Both inherited PRs remain open: merge T18, then T1B, then T1C.
- T1C is open in [PR #19](https://github.com/JBlaschke/writer_rs/pull/19)
  (`e1d5acb`). T41 starts from that checked feature on
  `codex/docx-rich-content`. Merge #17, then #18, then #19 before T41;
  all milestone PRs target main and CI does not block implementation.
- T18/T1B/T1C passed CI and merged in PRs #17/#18/#19. Main is
  `91ed669`; its tree matches checked T1C, including the PR #18 CI repair.
  T41 rebases onto that merged main and has no pending merge dependency.
- T41 passes formatting, Clippy, rustdoc, 1,115 debug and 1,114 release
  tests. Five image unit tests and 46 new integration tests cover rich
  figures/captions, SEQ resets, editable tables, images/DPI/conversions,
  TOC/bibliography, notes/bookmarks, target wiring and output safety.
  All 63 new/updated snapshots are reviewed; successful packages validate
  and read back independently. Basic/mixed DOCX builds are 9.5/47.6 kB;
  HTML remains 687.6/111.6 kB. T42 math warnings remain explicit.
- T41 is open in [PR #20](https://github.com/JBlaschke/writer_rs/pull/20)
  (`80a97fc`). T20 follows on that checked feature on `codex/build-watch`;
  merge T41 first. Each milestone targets main while CI runs independently.
- The T40 brief closes the previous `scripts/waves.py` missing-definition
  error. The audit now passes for written briefs; several later task briefs
  are still being written. Finish those definitions before
  using this script to dispatch subsequent waves. T00 has no dependencies.
- Tasks may run on local branches `task/Txx-slug` in separate git worktrees merged into `main`
  by the coordinator, or as pull requests against `origin` (github.com/JBlaschke/writer_rs).
  When there is no PR, "PR description" means the commit message body (template below).
- The coordinator updates the status table after merging; task agents do not edit this file.
- The coordinator recomputes the wave table from the task files' *Depends on* lines before each
  wave (`python3 scripts/waves.py` once T00 adds it; the rule is "no task depends on a task in
  its own wave").
- Contract changes (`src/ir.rs`, `Cargo.toml`, `docs/spec/*`) go through the coordinator and are
  recorded as "Contract change (coordinator, <date>)" boxes in the spec and under "Amendments" in
  the affected task files.
- Measured build/test times live in `docs/dev/build-times.md` (T00 writes it) so that slow
  compiler-backed tests are not mistaken for regressions.

## PR description template

```
Task: Txx — <title>
Spec sections implemented: …
Lifted from presenter_rs: <files> (adaptations: …)
Acceptance criteria: [x] … [x] … [ ] <not met: reason>
Files outside my ownership touched (additive): …
Manual checks performed: …
```

- T20 passes formatting, Clippy, rustdoc, 1,182 debug and 1,181 release
  tests. Real backend, library-loop and CLI subprocess tests cover recovery,
  debouncing, target subsets, output suppression and Windows directory guards.
  The release suite was rerun after desktop crash recovery; source and tests
  survived unchanged. T21 follows while CI runs, with one PR per milestone.

- T20 is open in [PR #21](https://github.com/JBlaschke/writer_rs/pull/21)
  (`722cf8e`). T21 starts on `codex/dev-server` from that checked milestone.
  Merge #20, then #21, then T21; all PRs target main and CI proceeds
  independently of implementation.

- T21 passes formatting, Clippy, rustdoc, 1,235 debug and 1,234 release tests.
  Nineteen real HTTP/SSE tests and 34 module tests cover the routes, state,
  ranges/cache, per-view protocol, Host checks and bounded shutdown. The
  pages shell is complete; compilation/client remains T24. Full validation
  also hardens the inherited watch-loop harness against concurrent startup
  contention without changing deadlines or production behavior.

- T21 is open in [PR #22](https://github.com/JBlaschke/writer_rs/pull/22)
  (`bbbd82c`). T25 starts on `codex/browser-smoke` from that checked feature.
  Merge #20, #21, then #22 before T25; all PRs target main while CI runs.

- T41 passed CI and merged in [PR #20](https://github.com/JBlaschke/writer_rs/pull/20)
  (`9a7d9d3`). T25 keeps the checked T20/T21 dependency commits while their
  PRs remain open; pending merge order is #21, then #22, then T25.

- T25 passes formatting, Clippy, default/feature rustdoc, 1,235 debug and
  1,234 release tests. All 28 feature client units and six browser/helper
  tests pass with required Chrome 156.0.8078.48; the standalone document
  renders math, highlighted code, images and fonts without browser errors.
  The native driver stays opt-in; T23 real-server integration follows.

- T25 is open in [PR #23](https://github.com/JBlaschke/writer_rs/pull/23)
  (`800103f`). T23 starts on `codex/serve-cli` from that checked feature.
  Merge #21, #22, then #23 before T23; all PRs target main while CI runs.
  The brief/spec record its private session module and shared watcher seams.

- T23 passes formatting, Clippy, default/feature rustdoc, 1,283 debug and
  1,282 release tests. All 32 launcher/session units, 16 real CLI tests and
  nine real-server Chrome cases pass; pixel-level anchors, selective updates,
  recovery/reconnect, source/cell events and SIGINT are verified. T50 follows
  to unblock T32 and the later T24 pages implementation, without waiting on CI.

- T20 passed CI and merged in PR #21 (`1bc404f`). T23 retains the checked
  T21/T25 dependency commits while #22/#23 remain open. Pending merge order
  is #22, then #23, then T23; main includes the same checked T20 code.

- T23 is open in [PR #24](https://github.com/JBlaschke/writer_rs/pull/24)
  (`b162c27`). T50 starts on `codex/typst-frontend` from that checked feature.
  Merge #22, #23, then #24 before T50; all PRs target main while CI runs.
  T50's brief freezes its private module split and narrow check/strict seams.


- T50 passes formatting, Clippy, default/feature rustdoc, 1,359 debug and
  1,358 release tests. All 52 frontend tests, 22 private units and 12 reviewed
  snapshots cover the structured syntax subset, metadata/literals, calls/tables,
  includes/assets, labels/notes/cites, exact math and located recovery. Library
  HTML/DOCX composition and offline native subset compilation pass; target-free
  CLI check is enabled. T32 follows to enable PDFs, then T24 actual pages.
- PR24's Linux/Windows CI repair `d8e91e7` corrects the ancestor test's completion
  barrier and catches intermediate directories created during watch retargeting.
  Focused 23 watcher units and 53 integration tests pass; T50 includes the
  equivalent cherry-pick `1dfa595` and passes both full suites with the repair.


- T50 is open in [PR #25](https://github.com/JBlaschke/writer_rs/pull/25)
  (`9a46045`). T32 starts on `codex/pdf-pipeline` from that checked feature.
  Merge #22, #23, #24, then #25 before T32; each PR targets main while CI runs.
  Its brief freezes private PDF diagnostic and World timestamp/mount seams.


- T32 passes formatting, Clippy, default/feature rustdoc, 1,387 debug and
  1,386 release tests. All 80 focused tests, 10 World units and 35 reviewed
  source/diagnostic snapshots cover embedded PDF builds and source-only dump,
  timestamps/reproducibility, resources, mapped failure retention, output safety
  and the complete numbering oracle. Basic/mixed PDFs have 3/2 pages at 50.8/91.5 kB;
  HTML is unchanged. T24 actual pages follows while CI runs independently.
- PR24's additional Windows missing-theme test repair `fe896de` uses the same SSE
  completion barrier as config recovery. Its focused real-process test passes;
  T50 includes equivalent `f32603f` and T32 includes `8fd053f`. Both final T32 full
  suites include that repair; remote CI proceeds independently.


- T32 is open in [PR #26](https://github.com/JBlaschke/writer_rs/pull/26)
  (`eb16145`). T24 starts on `codex/pages-view` from that checked feature.
  Merge #22, #23, #24, #25, then #26 before T24. All PRs target main while
  CI runs; its brief records builder/client/test ownership and no duplicate
  include-provider diagnostics.


- T24 passes formatting, Clippy, default/feature rustdoc, 1,401 debug and 1,400
  release tests. Twelve builder units, 26 watcher units, 47 real process/server
  tests, 13 actual Chrome cases and 26 mock/Chrome checks cover separate SVGs,
  key/byte reuse, lazy sizing, source anchors, selective requests, zoom and
  last-good/error/recovery behavior. T51 follows to enable later DOCX math.
- Independent [PR #27](https://github.com/JBlaschke/writer_rs/pull/27)
  (`fd233f1`) fixes main's macOS stale-directory event race and backports the
  existing PR22 harness guard; 1,185 debug/1,184 release tests pass on main.
  T24 includes equivalent `dbb25e5` and retains the already present guard.


- PRs #22–25 merged into main (`38fa4b3`) while the pages milestone was being
  validated. T24 retains their checked code. Pending feature order is #26,
  then T24; #27 repairs the independent watcher race and is included here.


- T24 is open in [PR #28](https://github.com/JBlaschke/writer_rs/pull/28)
  (`c9d2282`). T51 starts on `codex/typst-math` from that checked milestone.
  Its private symbol/call split and narrow recovery/generator choices are frozen
  in the brief. Merge #26, then #28 before T51; #27 is an independent CI repair.


- T51 passes formatting, Clippy, default/feature rustdoc, 1,428 debug and
  1,427 release tests. All 14 public tests, 13 private units, 72 reviewed
  snapshots and 11 Python tests pass. Actual Pandoc records, all 36 corpus
  bodies and all 1,058 distinct generated table expressions pass their gates.
  Pure conversion preserves unsupported source with located warnings and never
  evaluates code. T42 follows for editable DOCX math while CI runs.


- T51 is open in [PR #29](https://github.com/JBlaschke/writer_rs/pull/29)
  (`df800ce`). T42 starts on `codex/docx-math` from that checked milestone.
  Merge #26, #28, then #29 before T42; #27 remains an independent CI repair.
  T42's private XML/folding/renderer split is frozen in its brief; root owns
  DOCX integration, with independent corpus and route tests.


- T42 passes formatting, Clippy, default/feature rustdoc, 1,475 debug and
  1,474 release tests. Fourteen private units, 44 focused public/package tests
  and 20 package snapshot tests pass; all 73 OMML and 19 DOCX package goldens
  were independently reviewed. Native Typst/LaTeX equations remain editable,
  numbered frames preserve references and recovery, and actual Pandoc probes
  pass with exact documented aliases. T33 follows while CI runs.


- T42 is open in [PR #30](https://github.com/JBlaschke/writer_rs/pull/30)
  (`95267f8`). T33 starts on `codex/math-pdf-corpus` from that checked milestone.
  Merge #26, #28, #29, then #30 before T33; #27 remains an independent repair.
  The brief freezes a private shared preflight helper, existing CLI target
  forwarding and source/PDF visibility without new public APIs.


- T33 passes formatting, Clippy, default/feature rustdoc, 1,507 debug and
  1,506 release tests. All 19 check/CLI private units and 78 focused public,
  emitter and CLI tests pass. The 34 supported equations compile into one
  deterministic PDF; all three parser failures retain printable source,
  labels and numbering. Check adds the same parser without a World or output.
- PR #26 merged into main (`c0e35d2`) while T33 was validated. Pending feature
  order is #28, #29, #30, then T33; independent repair #27 proceeds separately.
  T43's reference/style showcase is staged next while CI runs.
