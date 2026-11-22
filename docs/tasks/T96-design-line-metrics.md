# T96 — Font line metrics in the Typst design input and in exports

| | |
|---|---|
| Milestone | M8 |
| Size | S |
| Depends on | T92, T95 (both merged) |
| Parallel-safe with | nothing: T97 to T101 follow this task one after the other |
| Files you own | `src/docx/lines.rs`, `src/typst/world.rs` (`world_inputs` and the new `design_input`), `src/pipeline/typst.rs` (one line of `design_files` and one import), `tests/typst_design.rs` (one changed assertion and one new test), `tests/typst_export.rs` (one added assertion), `tests/snapshots/typst_design__typst_design_input.snap` |

## Goal
The design that Typst receives names, for every font family the design uses, the two numbers
Word's line box is made of: the height of the font's single line and the part of it above the
baseline. They travel as a top-level `lines` table in the `writer-design` input and in an
export's `writer-design.json`. Nothing reads the table yet (T98 does), so no PDF changes in this
task.

The Rust code is written and tested already: `docs/research/R12-spacing-model.md` §5.1 and §5.2
hold it as two diffs. This task installs them, accepts one snapshot and adds two assertions of
its own.

## Read first
- `docs/spec/design.md` §3.3 (the paragraph on `lines`) and §7.4 (`WordLines`)
- `docs/research/R12-spacing-model.md` §2.2 (what the two numbers are), §4.1 (what the code
  does), the introduction of §5 (how a block is extracted)
- `src/docx/lines.rs`, whole, before you change it

## API to implement
```rust
// src/docx/lines.rs (R12 §5.1 is the complete change)
pub struct LineMetrics { pub single: f64, pub ascent: f64 }   // in ems
impl WordLines {
    // the metrics of every family a declared design names, whatever its line heights
    pub fn load_declared(
        design: &Design, font_paths: &[PathBuf], system_fonts: bool,
    ) -> Self;
    // {"<family in lower case>": {"single": s, "ascent": a}}, in family order
    pub fn to_json(&self) -> serde_json::Map<String, serde_json::Value>;
}
pub fn line_metrics(font: &Font) -> Option<LineMetrics>;

// src/typst/world.rs
// the design's JSON plus `lines`; `null` for an undeclared design
pub fn design_input(
    config: &Config, design: &Design, paths: PathMode<'_>,
) -> serde_json::Value;
```

## Steps
1. Apply R12 §5.1's diff. From the repository root:
   ```sh
   awk '/^```diff lines\.diff$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R12-spacing-model.md > lines.diff
   git apply --check lines.diff && git apply lines.diff && rm lines.diff
   ```
   The diff has 332 lines and changes `src/docx/lines.rs`, `src/pipeline/typst.rs` and
   `src/typst/world.rs`. If `git apply --check` fails, stop and make the same edits by hand,
   reading the diff as their exact text; do not rewrite the functions.
2. Apply R12 §5.2's diff (one hunk of `tests/typst_design.rs`) the same way:
   ```sh
   awk '/^```diff lines-tests\.diff$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R12-spacing-model.md > lines-tests.diff
   git apply --check lines-tests.diff && git apply lines-tests.diff && rm lines-tests.diff
   ```
3. Accept the one snapshot that changes and look at the change:
   ```sh
   INSTA_UPDATE=always cargo test --test typst_design
   git diff tests/snapshots/typst_design__typst_design_input.snap
   ```
   The diff must be exactly this, at the end of the file (the closing brace of `styles` gains a
   comma and one line follows it):
   ```diff
   -  }
   +  },
   +  "lines": {}
    }
   ```
   `git status --short tests/snapshots` must list that one file and no `.snap.new` file.
4. Add the test `design_input_carries_the_line_metrics_of_the_designs_fonts` at the end of
   `tests/typst_design.rs` (acceptance criterion 1; the set-up is under "Hints"). Put it at the
   end: T98 applies a diff to the earlier parts of this file.
5. Add the assertion of acceptance criterion 2 to
   `exported_designs_compile_with_their_copied_images_fonts_and_styles` in
   `tests/typst_export.rs`.
6. Run `cargo test --lib docx::lines`, `cargo test --test typst_design --test typst_export`, then
   the whole `cargo test`, then the other three quality gates.

## Acceptance criteria
1. `tests/typst_design.rs` holds `design_input_carries_the_line_metrics_of_the_designs_fonts`
   and it passes. With the design `named` of "Hints", `design_input(&config, &named,
   PathMode::Virtual(&assets))` gives:
   - `input["lines"]` equal to
     ```json
     {
       "dejavu sans mono": {"single": 1.1640625, "ascent": 0.92822265625},
       "dejavu sans mono bold": {"single": 1.1640625, "ascent": 0.92822265625},
       "libertinus serif": {"single": 1.14, "ascent": 0.894},
       "new computer modern": {"single": 1.417, "ascent": 1.127}
     }
     ```
     (compare with `assert_eq!` against `serde_json::json!`; the numbers are exact);
   - the top-level keys in this order: `fonts`, `text`, `colors`, `page`, `styles`, `lines`;
   - no entry for `no such family` (a family no font provides has no metrics).

   With the design `{"text": {"size": "11pt"}}` (no font named) `input["lines"]` is `{}`, and
   with `Design::default()` (undeclared) the whole input is `null`.
2. In `tests/typst_export.rs`, `exported_designs_compile_with_their_copied_images_fonts_and_
   styles` asserts, right after `assert!(design.get("declared").is_none());`:
   ```rust
   // The line metrics of the design's fonts travel with the export
   // (design.md §3.3): the renamed font keeps Libertinus Serif's.
   assert_eq!(
       design["lines"],
       json!({"libertinus fixme": {"single": 1.14, "ascent": 0.894}})
   );
   ```
   and the test passes.
3. `cargo test --lib docx::lines` runs seven unit tests and they pass. Three of them come with
   the diff: `line_metrics_put_the_hhea_descent_below_the_baseline`,
   `load_declared_reads_families_without_a_multiple` and
   `a_family_named_by_a_weight_takes_its_familys_lines`.
4. The snapshot changes as step 3 shows, and no other snapshot changes: in particular no DOCX
   snapshot (`w:line` values come from the same code and stay as they were).
5. `writer dump --design` is unchanged. This prints `False`:
   ```sh
   cargo run -q -- dump --design tests/fixtures/design/letter/letter.md \
       | python3 -c "import json, sys; print('lines' in json.load(sys.stdin))"
   ```
6. The installed code is the reference code: extract `lines.diff` again as in step 1 and run
   `git apply --check --reverse lines.diff` (it must succeed; remove the file afterwards).
7. Outside `docs/` and `PLAN.md` (the plan's own files, which the coordinator changed before this
   task), `git status --short` lists only the six files under "Files you own".
8. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Reading `lines` anywhere: the package (`assets/typst/writer-doc/**`) is T98's.
- Changing the reference code. It was run with all four gates green on 2026-11-21. If a test
  you wrote fails, the test is wrong; if a test that came with a diff fails, stop and report.
- `Design::to_json`, `dump --design` and the HTML template context: they do not carry `lines`
  (design.md §3.3).
- The DOCX line values (`WordLines::line`, `single_line`): their results do not change.
- `docs/**` and `PLAN.md`: the coordinator's.

## Hints (verified)
- **Set-up of the new test** (run against the reference code on 2026-11-21). `design(value,
  root)` exists in `tests/typst_design.rs` already; `PathMode` and `AssetRegistry` are imported
  there; `design_input` needs `use writer::typst::world::design_input` (extend the existing
  import of `MainFile, WriterWorld, world_inputs`):
  ```rust
  let directory = tempfile::tempdir().unwrap();
  let root = directory.path();
  let mut config = Config::default();
  config.pdf.system_fonts = false;
  let (font, _) = writer::typst::fonts::embedded()
      .find(|(_, info)| info.family == "Libertinus Serif")
      .unwrap();
  let path = root.join("libertinus.otf");
  std::fs::write(&path, font.data()).unwrap();
  let named = design(
      serde_json::json!({
          "fonts": {
              "body": "DejaVu Sans Mono",
              "heading": {"family": "Libertinus Serif", "files": [path]},
              "mono": "No Such Family",
          },
          "styles": {
              "Heading 3": {"font": "DejaVu Sans Mono Bold"},
              "Quote": {"font": "New Computer Modern"},
          },
      }),
      root,
  );
  let assets = AssetRegistry::new();
  let input = design_input(&config, &named, PathMode::Virtual(&assets));
  ```
  The key order reads as `input.as_object().unwrap().keys().collect::<Vec<_>>()`.
- **Why these values.** `single` is Word's single line (`hhea` ascender + descender + line gap,
  in ems), `ascent` is `single` minus the descender. DejaVu Sans Mono: 2384/2048 and 1901/2048.
  "DejaVu Sans Mono Bold" names a face Word selects by that name; no family is filed under it,
  so the code looks the family up without the weight word and records the result under both
  names. `No Such Family` is in no font source and gets no entry.
- `system_fonts = false` keeps the test independent of the machine: the sources are then the
  design's font files, `[pdf] font_paths` and writer's embedded fonts (Libertinus Serif, DejaVu
  Sans Mono, New Computer Modern).
- The letter fixture (`tests/fixtures/design/letter`) names Source Sans 3 without files and sets
  `system_fonts = false`, so its `lines` is `{}`: that is the snapshot's new line.
- With `INSTA_UPDATE=always`, insta may also drop an `assertion_line:` header line from a
  snapshot it rewrites. That is harmless.
- Cargo test filters go after `--`: `cargo test --test typst_design -- design_input`.
- The crate's dependencies are vendored (`.cargo/config.toml`), so every command works offline.
  A first `cargo test` in a fresh checkout builds for several minutes.
