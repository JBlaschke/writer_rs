# T122 — Guide: one set of defaults

**Status: brief, not ready for hand-off.** The list of guide pages to change comes first (see
"Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | S |
| Depends on | T119, T120 |
| Parallel-safe with | T121 (wave 40; no shared file) |
| Files you own | the `docs/guide/` pages that describe per-backend defaults (the coordinator's list), and a new guide page listing the built-in design and design.md §0.4's exceptions |

## Goal
The user guide says what writer now does: one built-in design, exported to every format, with a
short list of exceptions. No page says that a backend keeps its own default for a key the design
governs.

## Read first
- `docs/spec/design.md` §0.4
- `docs/research/R13-single-source-defaults.md` §4.3, §5

## API to implement
None: documentation.

## Steps
1. Rewrite each listed page's per-backend defaults.
2. Write the new page: the built-in values by domain and the exceptions, in user terms.
3. Link it from `docs/guide/README.md`.

## Acceptance criteria
1. No guide page says a backend keeps its own default for a key the design governs.
2. The new page lists the built-in design and every exception of design.md §0.4.
3. `cargo test` passes, `tests/guide.rs` included.
4. `git grep -n -E '\bT1[0-9]{2}\b' -- docs/guide` prints nothing.

## Out of scope
- The spec (`docs/spec/`), which the domain tasks update.

## Hints (verified)
- `docs/guide/` at main: `README.md`, `authoring.md`, `cli.md`, `configuration.md`,
  `customization.md`, `getting-started.md`, `notebooks.md`, `outputs.md`,
  `troubleshooting.md`, `typst.md`.
- `tests/guide.rs` rejects task IDs in guide pages, but its pattern
  (`\bT(?:[0-9]{2}|[0-9][A-Z])\b`, :486) catches only T00–T99 and T1A-style IDs, so T102–T122
  pass it: write no T-number there (criterion 4 checks).

## Before hand-off
- The coordinator lists the pages.
