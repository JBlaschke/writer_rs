# T107 — `@writer/doc` 0.6.0: the package version wherever it is named; no behaviour change

**Status: brief, not ready for hand-off.** It waits for decision D14; if D14 folds M9 into 0.5.0
the task is dropped (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | S |
| Depends on | T101 (and decision D14) |
| Parallel-safe with | T102, T103, T104, T105 (wave 26; no shared file) |
| Files you own | the package-version occurrences of `0.5.0` that R12's T97 wrote: `src/typst/mod.rs`, `src/typst/includes.rs` (two test literals), `assets/typst/writer-doc/typst.toml`, `assets/typst/writer-doc/template.typ` (the `#import` line only), `docs/guide/customization.md` and `docs/guide/typst.md` (the `#import` lines), `examples/typst/document.typ`, `tests/fixtures/serve/{native,template}.typ`, `tests/fixtures/typ/{include-notebook-missing,include-notebook,minimal,subset,template}.typ`, `tests/{cli_serve,diagnostic_payloads,pdf_pipeline,typst_emit,typst_export,typst_frontend,typst_includes,typst_package,typst_world}.rs`, and the 23 snapshot files T97 lists under its Hints |

## Goal
The embedded Typst package is `@writer/doc` version `0.6.0` everywhere, before T108 changes its
behaviour, as T93 and T97 did before T94 and T98. Only the version changes.

## Read first
- `docs/tasks/T97-writer-doc-0-5-0.md`, whole: this task repeats it from 0.5.0 to 0.6.0
- `docs/research/R13-single-source-defaults.md` §4.6 (D14, D15)

## Steps
Follow T97's steps with `0.5.0` → `0.6.0`.

## Acceptance criteria
1. `git grep -c '@writer/doc:0\.6\.0' -- tests/snapshots | wc -l` prints `22`, and
   `tests/snapshots/typst_frontend__unsupported-diagnostics.snap` names `embedded version 0.6.0`.
2. `git grep -n 'writer/doc:0\.5\.0' -- src assets tests examples docs/guide` prints nothing.
3. Every other snapshot is byte-identical.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Any behaviour change (T108 onward); a second embedded package version (decision D15).
- `docs/spec/**`, `docs/tasks/**`: the coordinator's.

## Hints (verified)
- In R12's worktree on 2026-11-22, `@writer/doc:0.5.0` occurs in the files listed above plus
  `docs/spec/design.md`, `docs/spec/typst-dialect.md` and two task files, which are the
  coordinator's; 22 snapshots hold the import string, and
  `typst_frontend__unsupported-diagnostics.snap` holds the version in a message (T97's Hints).
- `WRITER_DOC_VERSION` is `src/typst/mod.rs:5` in the worktree; `typst.toml` has the version at
  :3. `embedded_dir` (`src/typst/package.rs:187-202`) serves only that version.

## Before hand-off
- D14 decided: 0.6.0, or fold the defaults into 0.5.0 before any release ships 0.5.0 (then drop
  this task and its row, and drop it from T108's dependencies).
- After T101 merges, the coordinator lists every 0.5.0 occurrence, as for T97.
