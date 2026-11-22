# T121 — Guard test: no presentational literals

**Status: brief, not ready for hand-off.** The list of literals left after T119 and T120 comes
first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | S |
| Depends on | T119, T120 |
| Parallel-safe with | T122 (wave 40; no shared file) |
| Files you own | `tests/no_presentational_literals.rs` (new) |

## Goal
A test that keeps the single source single: it fails when a colour or length literal appears in
a backend's presentational code or assets outside the built-in defaults and an allowlist.

## Read first
- `docs/spec/design.md` §0.4 (the exceptions)
- `docs/research/R13-single-source-defaults.md` §5, §6.1, §6.2 (T121)

## API to implement
None: an integration test.

## Steps
1. List the files the test scans (the coordinator's list).
2. Match colour and length literals; compare with the allowlist.
3. Check that the test fails on an added literal, then remove it.

## Acceptance criteria
1. The test passes on main after T119 and T120.
2. It fails when a colour or length literal is added outside the defaults and the allowlist.
3. The allowlist includes R12's constants: the 0.8/0.2 exact-pitch split, the Calibri fallback
   for a family no table or file resolves, and the Typst measures that R12's `spacing-model` and
   `cell-inset` (design.typ) keep for an unset key: `0.65em`, `1.2em`, `0.5em`, `5pt`, and the
   0.85 note size if T115 keeps it as the unset fallback.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Removing the literals it finds: those belong to the domain tasks, reopened by the coordinator.

## Hints (verified)
- The exact-pitch split is stated in design.md §0.3 and in `design.typ` (spacing-model): an
  exact pitch `h` puts `0.8 h` above the baseline and `0.2 h` below it.
- The Calibri fallback is `DEFAULT_SINGLE_LINE` (2500/2048 em) in `src/docx/lines.rs:22`.

## Before hand-off
- The coordinator lists the literals left after T119 and T120, and the files to scan.
