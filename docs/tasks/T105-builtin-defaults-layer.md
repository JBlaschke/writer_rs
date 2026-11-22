# T105 — Built-in defaults layer: the contract first, behind a switch

**Status: brief, not ready for hand-off.** It needs decision D13 signed, the design.md edits of
R13 §4.1 written and a prototype of the view (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | L |
| Depends on | T101 (and decision D13 for the contract) |
| Parallel-safe with | T102, T103, T104, T107 (wave 26; no shared file) |
| Files you own | `src/design/**` (new `src/design/defaults/`; `merge.rs`, `mod.rs`, `parse.rs`, `resolve.rs`, `json.rs`, `template.toml`), `src/pipeline/mod.rs` (the design functions only: `with_design_layers`, `resolve_design`, `merged_design_table`), `src/check/design.rs`, `src/check/targets.rs`, `src/cli/mod.rs` (one flag of `writer dump`), `src/cli/dump.rs`, `docs/spec/cli.md` (that flag's paragraph), `tests/design_config.rs`, `tests/design_defaults.rs` (new) |

## Goal
writer gets a complete built-in design, merged as layer 0 below the theme chain, without any
output changing yet:
- `src/design/defaults/<domain>.toml` (text, fonts, colors, page, styles, lists, table, code,
  callout, image, footnotes, links) together set every key of the schema plus R13 §4.2's
  additions, with R13 §4.3's proposed values.
- They are compiled into the binary and pushed first by `with_design_layers`; `merge` records a
  new origin, builtin, for their keys.
- Lengths accept em: of the style's size inside a style, else of `text.size`.
- The `declared` flag splits in two. `user_declared` is true when a layer above layer 0 has a
  key, and drives `writer check` and provenance. The backends see a view of the design that
  keeps the builtin keys of the domains listed in a constant `APPLIED` and drops the others
  before resolving; its `declared` is true when anything is left.
- `APPLIED` is empty in this task, so the view equals today's design, `declared` included, and
  no backend changes. T108 and the domain tasks after it switch their bits on.
- The view also tells which of its values are built-in, either as a pair of views (built-in,
  user) or as an origin per key. HTML needs it from T108 on: it writes the built-in rules before
  the theme (decision D16) and the user's after it (R12's D2), and one merged view cannot tell
  them apart (R13 §4.1, "HTML").
- `writer dump --design --defaults` prints the merged design with the origin of each key.

## Read first
- `docs/research/R13-single-source-defaults.md` §1.4 (the places that read `declared`), §4.1,
  §4.2 and §4.3
- `docs/spec/design.md` §0.2, §0.4, §1, §2.3 and §3 (after the coordinator's edits)
- `src/design/mod.rs`, `src/design/merge.rs`, `src/design/resolve.rs`, `src/design/parse.rs`
- `src/pipeline/mod.rs:458-519` (`with_design_layers`, `resolve_design`, `merged_design_table`)

## API to implement
Set at hand-off from the coordinator's prototype. R13 §4.1 proposes these names, none of which
exists yet: a variant `Origin::Builtin`, a set type `Domains` with a constant `APPLIED: Domains`
(empty here), a method `Design::view(APPLIED)`, a way to tell built-in values from the user's
in the view (a pair of views or an origin per key), and a `user_declared` flag. Existing signatures
that must keep working for every caller: `merge(layers: &[Layer<'_>]) -> Result<Design>`
(`src/design/merge.rs:115`), `merged_table(layers: &[Layer<'_>]) -> Result<Map<String, Value>>`
(:110), `resolve(table: &Map<String, Value>) -> Result<Design>` (`src/design/resolve.rs:17`),
`parse_length(text: &str) -> Result<Pt>` (`src/design/parse.rs:12`).

## Steps
1. Write the defaults files from R13 §4.3 and the schema additions of R13 §4.2 into the parser
   and the resolver, with em lengths.
2. Add the builtin origin and layer 0 in `with_design_layers`.
3. Add `user_declared` and the view; give the backends the view, `writer check` the flag.
4. Assign each place of R13 §1.4 to one flag, in a comment or a test.
5. Add the dump flag and its paragraph in `docs/spec/cli.md`.

## Acceptance criteria
1. Every file in `tests/snapshots` is unchanged (628 files at main and in R12's worktree on
   2026-11-22): `git status --short tests/snapshots` prints nothing after `cargo test`.
2. A property test: the view with no domain applied equals the design resolved without layer 0,
   `declared` included, for every design in the test fixtures.
3. `writer dump --design --defaults` lists every key, with origin builtin where no layer sets it;
   `writer dump --design` without the flag prints what it prints today (`docs/spec/cli.md:128-130`:
   `null` when nothing is declared).
4. Parse tests cover em lengths inside styles and at the top level.
5. Each place of R13 §1.4 is assigned to one flag: the backends, `src/typst/world.rs:621`,
   `src/assets/export.rs:137` and `src/design/json.rs:73` read the view's `declared`;
   `src/check/design.rs:43`, :95 and `src/check/targets.rs:42` read `user_declared`.
6. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Switching any domain on (T108 to T119), bundled line metrics (T106), the package (T107).
- The backends' code, except that the view tells HTML which values are built-in: the backends
  keep reading `declared` and need no edit for the split (R13 §4.1). Writing HTML's rules in two
  places is T108's.
- `docs/spec/design.md`: the coordinator's.

## Hints (verified)
- `Design` is at `src/design/mod.rs:192`, its `declared` field at :194; `resolve` sets it as
  `!table.is_empty()` (`src/design/resolve.rs:20`).
- `Layer { table, base_dir, origin }` is at `src/design/mod.rs:596-603`; `Origin` (:607-617) has
  `Theme { file }`, `ProjectFile(PathBuf)` and `FrontMatter`.
- `with_design_layers` (`src/pipeline/mod.rs:458-503`) pushes the theme's layers, the project
  file and the front matter, in that order; `Prepared.design` (:80) is set at :254-256.
- PLAN.md §3.2: `src/pipeline/mod.rs` and `src/cli/mod.rs` are edited body-only.
- The dump flag sits beside `design` in `src/cli/mod.rs:311-313`; `src/cli/dump.rs:30-33`
  prints `prepared.design.to_json(PathMode::Real)`.
- `src/design/template.toml:6-30` documents each backend's own default for an unset key; it
  changes with the built-in layer.
- The 19 places that read `declared` are listed with their lines in R13 §1.4.

## Before hand-off
- D13 (the built-in design applies to every document) signed by the owner.
- The coordinator writes the design.md edits of R13 §4.1 (design.md §0.4 lists them).
- The coordinator prototypes the view, runs the full suite with `APPLIED` empty and counts the
  result lines.
- Decide how fine `APPLIED` is. The named styles of `defaults/styles.toml` belong to six tasks
  (T109 headings and title, T110 captions, T112 list and quote styles, T113 code and callout
  styles, T115 footnotes and bibliography, T117 div classes), so one bit per file is too coarse.
  The same holds for `text.toml` (T108; T118's hyphenation, widows, orphans, kerning and
  ligatures), `fonts.toml` (T108; T113's `fonts.math`), `colors.toml` (T108; T111's palette) and
  `page.toml` (T108; T116's page-number slot): `APPLIED` needs one bit per key group, as listed
  in each task's "Files you own".
- Decide the output of `--defaults` and whether `user_declared` appears in the JSON.
