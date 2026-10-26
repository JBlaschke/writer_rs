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
| T18 | Notebook frontend | M1 | L | T12, T13, T30, T40 | todo |
| T19 | Default HTML theme v1 | M1 | M | T15, T16 | ready for review |
| T1A | Math in HTML (KaTeX, MathML, image routes) | M1 | M | T03, T15, T16, T30 | todo |
| T1B | References/citations syntax and `:::include` | M1 | M | T12, T16, T18 | todo |
| T1C | `writer check` with Markdown and notebook lints | M1 | S | T16, T1B | todo |
| T20 | File watcher and the document watch set | M2 | M | T16 | todo |
| T21 | Dev server: routes, events, per-view state | M2 | M | T16 | todo |
| T22 | Live-reload client | M2 | M | T00 | todo |
| T23 | `serve` CLI, session runtime, integration tests | M2 | S | T1A, T20, T21, T22, T25 | todo |
| T24 | Pages view | M2 | M | T21, T23, T32 | todo |
| T25 | Headless-browser smoke tests (opt-in) | M2 | M | T16 | todo |
| T30 | IR → typst emitter with source map | M3 | L | T01, T03, T17 | todo |
| T31 | `@writer/doc` typst package | M3 | M | T03 | todo |
| T32 | PDF pipeline, diagnostics, `dump --typst`, reproducibility, numbering oracle | M3 | L | T03, T16, T30, T31, T50 | todo |
| T33 | Math corpus through mitex, `check` pre-flight | M3 | S | T1A, T1C, T32, T42 | todo |
| T34 | BYO typst template and `--to typst` export | M3 | M | T1B, T32 | todo |
| T40 | DOCX body, bookmarks and internal links | M4 | L | T04, T13, T17 | todo |
| T41 | DOCX tables, figures, captions, TOC, bibliography; `--to docx` | M4 | L | T16, T40 | todo |
| T42 | DOCX math (OMML, numbered equations, image fallback) | M4 | M | T03, T1A, T41, T51 | todo |
| T43 | Reference documents and the DOCX validation suite | M4 | M | T41, T42 | todo |
| T50 | typst-syntax → IR walker (structured subset) | M5 | L | T01, T17, T30 | todo |
| T51 | typst math → LaTeX converter and the symbol table | M5 | L | T01 | todo |
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
- T16 is [PR #9](https://github.com/JBlaschke/writer_rs/pull/9), based on PR #8.
  T19 starts from completed T16 (`22723f2`) on `codex/default-html-theme`.
- Windows CI fixes are pushed to PR #8 (`dae7357`, Unix-only test helper) and
  PR #9 (`afe0fa9`, compact boxed build errors). The updated full local debug
  suite passes 734 tests. T19 includes both repairs; PR #8 now targets `main`.
- T19 passes formatting, Clippy, rustdoc, 768 debug and 767 release tests.
  Its 34 new tests cover stylesheet/token/README contracts and the CLI showcase.
  Entries total 13,402 bytes; templates and snapshots are unchanged. In-app
  browser checks cover light/dark, narrow layouts and forced print rules;
  Chrome's paginated print preview remains unverified.
- PR #9 passed CI and merged into `main` (`ed9d4d2`). PR #10 passed CI and
  merged into its base `codex/build-and-dump` (`4289bb6`), so T19 is not yet on
  `main`. The follow-up branch `codex/default-theme-main` brings the same theme
  changes to `main`; later work can continue from this checked implementation.
- Local implementation started on 2026-10-25 with T00. Local acceptance results and timings
  are in [`../dev/build-times.md`](../dev/build-times.md); the local status does not mean
  the changes are merged or that the three-OS CI matrix has run.
- The current `python3 scripts/waves.py` audit stops at missing task definitions `T30` and
  `T40`. Several task briefs are still being written; finish the missing definitions before
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
