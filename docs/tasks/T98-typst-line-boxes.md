# T98 — `@writer/doc` 0.5.0: Word's line boxes and one rule for the space between blocks

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T96, T97 |
| Parallel-safe with | nothing: T99 starts after this task |
| Files you own | `assets/typst/writer-doc/design.typ`, `assets/typst/writer-doc/lib.typ`, `tests/typst_design.rs` (three hunks; T96's test at the end stays), `tests/typst_furniture.rs`, `tests/typst_leading.rs`, `tests/typst_style_oracle.rs`, `tests/typst_vocabulary.rs`, `tests/typst_spacing.rs` (new) |

## Goal
With a declared design the PDF lays text out as Word does (`docs/spec/design.md` §0.3). A line is a
box as tall as its pitch, with the font's ascent above the baseline and nothing between two boxes.
Between two blocks, the distance from the upper block's last line box to the lower block's first
is the larger of the upper block's space after and the lower block's space before. Without a
design the PDF does not change.

The package code is written and tested already, and so are the changed assertions of the five
existing test files: `docs/research/R12-spacing-model.md` §5.3 and §5.4 hold them as two diffs.
This task installs the two diffs and writes one new test file, `tests/typst_spacing.rs`, whose
ten tests pin the positions the model gives.

## Read first
- `docs/spec/design.md` §0.3 (the model), §6.2 from "Lines and blocks" to the end of the section,
  and §6.3's note "Contract change (2026-11-21, package `0.5.0`; T98)"
- `docs/research/R12-spacing-model.md` §2.2 to §2.4 (what Word does), §3 (the Typst facts the code
  relies on), §4.2 (what the package does), the introduction of §5 (how a block is extracted)
- After step 3: `tests/typst_leading.rs`, whole. Its harness is the model for the new file's.

## API to implement
R12 §5.3 is the complete change. For orientation, the names it adds or changes:
```typst
// design.typ
#let cap-height(face) = ..                  // the face's cap height, measured
#let font-lines(design, face) = ..          // (single:, ascent:) of the face, in em
#let line-edges(design, pitch, face) = ..   // (top-edge:, bottom-edge:) for `set text(..)`
#let inherited-pitch() = ..                 // the pitch of the text in effect (context)
#let line-trim(design, pitch, face) = ..    // the parts of a box outside the capitals
#let spacing-model(settings) = ..           // pitch, paragraph gap, equation leading, note gap
#let flow-mark(below) = ..                  // a block without height that carries a space
#let par-end(it) = ..                       // a paragraph followed by its flow mark
#let open-flow(mark, body) = ..             // the mark in front of the body's first element
#let empty-cell(it) = ..                    // an empty table cell holds one line box
#let style-rules(design, spec, pitch) = ..  // took a leading, takes the document's pitch
#let vocabulary(design, pitch) = ..
#let cell-inset(design, pitch) = ..
#let vocabulary-layout(design, pitch, body) = ..
// lib.typ
#let styled-title-block(meta, design, pitch) = ..
#let paragraph-layout(meta, settings, body) = ..  // sets the line box and the block spaces
// removed: line-box, design-leading, design-spacing, style-leading
```
`pitch` is `none` (no line height: Typst's own pitch), `(multiple: m)` or `(exact: h)`.

## Steps
1. Record what the tree holds before this task (T96's and T97's changes are not committed).
   From the repository root:
   ```sh
   git status --short > target/t98-before.txt
   git status --short tests/snapshots > target/t98-snapshots.txt
   ```
2. Apply R12 §5.3's diff (628 lines; it changes `design.typ` and `lib.typ`):
   ```sh
   awk '/^```diff package\.diff$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R12-spacing-model.md > package.diff
   git apply --check package.diff && git apply package.diff && rm package.diff
   ```
   If `git apply --check` fails, stop and report; do not edit the package by hand.
3. Apply R12 §5.4's diff (500 lines; it changes the five existing test files) the same way:
   ```sh
   awk '/^```diff model-tests\.diff$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R12-spacing-model.md > model-tests.diff
   git apply --check model-tests.diff && git apply model-tests.diff && rm model-tests.diff
   ```
4. Run the five changed targets. All of their tests must pass, and no snapshot may change:
   ```sh
   cargo test --test typst_design --test typst_furniture --test typst_leading \
       --test typst_style_oracle --test typst_vocabulary
   git status --short tests/snapshots | diff target/t98-snapshots.txt -
   ```
   The `diff` must print nothing.
5. Create `tests/typst_spacing.rs` with the ten tests of acceptance criteria 1 to 10. "Hints"
   holds the harness, the two designs and every document; the criteria hold the numbers.
6. Run `cargo test --test typst_spacing`, then the whole `cargo test`, then the other three
   quality gates.
7. Do the manual check of acceptance criterion 15 if a Typst 0.15 command line is installed.

## Acceptance criteria
Every baseline below is the vertical page position of a text run's baseline in pt, read with
`assert_baselines` ("Hints"); the tolerance is 0.01 pt. "Page" counts from 1. The design is
`spaced` unless a criterion names another.

1. `adjacent_blocks_keep_the_larger_of_their_spaces`: the document `ADJACENT` has one page and
   these baselines:

   | Run | Baseline | Why (space kept above the run's block) |
   |---|---|---|
   | `PARAA` | 94.873 | top margin, then the ascent |
   | `WIDEONE` | 126.723 | 17 pt: `Wide`'s space before beats the paragraph spacing |
   | `WIDETWO` | 151.778 | the paragraph spacing, inside the div |
   | `PARAB` | 189.628 | 23 pt: `Wide`'s space after |
   | `NARROWONE` | 214.683 | the paragraph spacing beats `Narrow`'s 5 pt |
   | `PARAC` | 233.533 | 4 pt: `Narrow`'s space after |
   | `FLUSHONE` | 258.588 | the paragraph spacing beats `Flush`'s 0 pt |
   | `PARAD` | 273.438 | 0 pt: `Flush`'s space after |
   | `BAREONE` | 298.493 | the paragraph spacing: `Bare` sets no space |
   | `BARETWO` | 323.548 | the paragraph spacing |
   | `PARAE` | 348.603 | the paragraph spacing |
   | `WIDETHREE` | 380.453 | 17 pt |
   | `WIDEFOUR` | 418.303 | 23 pt: the larger of 23 pt after and 17 pt before |
   | `NARROWTWO` | 456.153 | 23 pt: the larger of 23 pt after and 5 pt before |
   | `PARAF` | 475.003 | 4 pt |
   | `HEADONE` | 512.535 | 20 pt: `Heading 1`'s space before |
   | `PARAG` | 534.753 | 6 pt: `Heading 1`'s space after |
   | `HEADTWO` | 560.702 | the paragraph spacing beats `Heading 2`'s 4 pt |
   | `PARAH` | 578.008 | 2 pt: `Heading 2`'s space after |
   | `HEADTHREE` | 615.540 | 20 pt |
   | `HEADFOUR` | 638.652 | 6 pt: the larger of 6 pt after and 4 pt before |
   | `PARAI` | 655.958 | 2 pt |

2. `the_first_block_keeps_its_space_before_on_page_one_only`, three documents:

   | Document | Pages | Page | Run | Baseline |
   |---|---|---|---|---|
   | `FIRST_HEADING` | 2 | 1 | `HEADFIRST` | 117.555 |
   | | | 1 | `PARAA` | 139.773 |
   | | | 2 | `HEADNEXT` | 97.555 |
   | | | 2 | `PARAB` | 119.773 |
   | `FIRST_DIV` | 1 | 1 | `WIDEFIRST` | 111.873 |
   | | | 1 | `PARAA` | 149.723 |
   | `FIRST_PARAGRAPH` | 1 | 1 | `PARAA` | 94.873 |
   | | | 1 | `PARAB` | 119.928 |

   On page 1 the heading sits 20 pt (its space before) lower than on page 2.
3. `a_page_rule_at_the_start_of_the_body_adds_no_page`: `PAGE_RULE` has exactly one page with
   `HEADFIRST` at 42.516 and `PARAA` at 64.734; `TEMPLATE_RULE` has exactly one page with
   `HEADFIRST` at 72.516 and `PARAA` at 94.734.
4. `a_title_block_function_gets_no_opening_mark`: `COVER` has exactly two pages; page 1 holds
   `COVER` at 14.834; page 2 holds `HEADFIRST` at 97.555 and `PARAA` at 119.773.
5. `lists_and_empty_cells_follow_the_model`: `LISTS` has these baselines on page 1:

   | Run | Baseline | |
   |---|---|---|
   | `PARAA` | 94.873 | |
   | `TIGHTONE` | 119.928 | the paragraph spacing above a tight list |
   | `TIGHTTWO` | 134.778 | no space between its items: one pitch, 14.85 pt |
   | `PARAB` | 159.833 | |
   | `LOOSEONE` | 184.888 | |
   | `LOOSETWO` | 209.943 | the paragraph spacing between the items of a loose list |
   | `PARAC` | 234.998 | |
   | `CELLA` | 262.457 | first row of the table |
   | `PARAD` | 304.766 | after two rows of 17.254 pt: the empty row is as high as the first |

6. `line_boxes_follow_the_line_height`: the document `#show: writer-doc` + `LINES` under the
   design `lines(root, text)`, where `text` is `{"size": "11pt", "paragraph_spacing": "3.6mm"}`
   plus the line height of the row (the last row's `text` is `{"size": "11pt"}` alone). The first
   four columns are what the probe reports ("Hints"), the last three are baselines:

   | `line_height` | `top` | `bottom` | `leading` | `gap` | `LINEONE` | `LINETWO` | `PARATWO` |
   |---|---|---|---|---|---|---|---|
   | `1.0` | 8.626 | −2.374 | 0 | 10.205 | 79.492 | 90.492 | 111.697 |
   | `"15pt"` | 12.0 | −3.0 | 0 | 10.205 | 82.866 | 97.866 | 123.071 |
   | not set | 9.834 | −4.554 | 0 | 10.205 | 80.700 | 95.088 | 119.681 |
   | not set, no `paragraph_spacing` | 9.834 | −4.554 | 0 | 6.05 | 80.700 | 95.088 | 115.526 |

7. `an_undeclared_design_keeps_typsts_layout`: the same document under `Design::default()`. The
   probe reports `top` as the string `"\"cap-height\""`, `bottom` as `"\"baseline\""`, `leading`
   7.15 and `gap` 13.2; the baselines are `LINEONE` 78.104, `LINETWO` 92.492, `PARATWO` 112.930.
8. `explicit_arguments_keep_typsts_meaning`: the design `lines(root, {"size": "11pt",
   "line_height": 1.35, "paragraph_spacing": "3.6mm"})` and the document `#show:
   writer-doc.with(ARGUMENT)` + `LINES`:

   | `ARGUMENT` | probe | `LINEONE` | `LINETWO` | `PARATWO` |
   |---|---|---|---|---|
   | `leading: 0.65em` | `top` 9.834, `bottom` −4.554, `leading` 0, `gap` 10.205 | 80.700 | 95.088 | 119.681 |
   | `spacing: 20pt` | `bottom` −5.016, `gap` 12.388 | 80.700 | 95.550 | 122.788 |
   | `spacing: 0pt` | `bottom` −5.016, `gap` 0 | 80.700 | 95.550 | 110.400 |

9. `footnote_lines_are_line_boxes_at_the_notes_size`: the document `NOTES` under the design
   `lines(root, text)`, where `text` is `{"size": "11pt", "paragraph_spacing": "3.6mm"}` plus the
   line height of the row. The probe stands inside the first note; the baselines are on page 1:

   | `line_height` | `top` | `bottom` | `leading` | `NOTEONE` | `NOTETWO` | `NOTETHREE` |
   |---|---|---|---|---|---|---|
   | `1.35` | 8.359 | −4.264 | 0 | 741.515 | 754.138 | 766.760 |
   | `1.0` | 7.332 | −2.018 | 0 | 748.004 | 757.354 | 769.006 |

10. `a_display_equation_keeps_typsts_leading_between_its_rows`: the document `EQUATION` under the
    same design, with the line height of the row. The probe stands inside the equation and only
    its `leading` is checked; the baselines are on page 1, and the second row checks the two
    equation rows alone:

    | `line_height` | `leading` | `PARAA` | `ROWONE` | `ROWTWO` | `PARAB` |
    |---|---|---|---|---|---|
    | `1.35` | 7.612 | 80.700 | 103.676 | 119.285 | 139.566 |
    | `"15pt"` | 7.762 | | 103.826 | 119.585 | |

11. The five changed targets pass with these numbers of tests: `typst_design` 10 (nine and T96's
    one), `typst_furniture` 11, `typst_leading` 4, `typst_style_oracle` 3, `typst_vocabulary` 12.
    `typst_spacing` has 10.
12. No snapshot changes in this task: step 4's `diff` prints nothing after the whole `cargo test`
    too, and there is no `.snap.new` file.
13. The installed code is the reference code. Extract `package.diff` and `model-tests.diff` again
    as in steps 2 and 3; `git apply --check --reverse package.diff` and `git apply --check
    --reverse model-tests.diff` succeed (remove both files afterwards). `wc -l
    assets/typst/writer-doc/design.typ assets/typst/writer-doc/lib.typ` prints 510 and 746.
14. In the output of `git status --short | diff target/t98-before.txt -` no line starts with `<`,
    and the lines that start with `>` are exactly these seven (remove the two files in `target/`
    afterwards):
    ```
    >  M assets/typst/writer-doc/design.typ
    >  M assets/typst/writer-doc/lib.typ
    >  M tests/typst_furniture.rs
    >  M tests/typst_leading.rs
    >  M tests/typst_style_oracle.rs
    >  M tests/typst_vocabulary.rs
    > ?? tests/typst_spacing.rs
    ```
    (`tests/typst_design.rs` is in the list already, from T96.)
15. Manual check outside writer, with a `typst` 0.15 command line (skip it and say so if none is
    installed): `cargo run -- eject typst-package DIR` into an empty directory, the file
    `probe.typ` of "Hints" next to it, then
    `typst query --package-path DIR probe.typ "<edges>" --field value` prints
    `[["0.89em","-0.46em","0pt","10.21pt"]]` (Typst 0.15.1 adds a deprecation warning for `query`;
    that is fine).
16. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
    `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Changing the reference code or the assertions of the two diffs. Both ran with all four gates
  green on 2026-11-22, together with a test file that asserts every number above. If a test you
  wrote fails, the test is wrong: compare its document and design with "Hints" character by
  character. If a test that came with a diff fails, stop and report.
- HTML and Word: T99 and T100.
- The guide and the letter example's README: T101. The starter template
  (`assets/typst/writer-doc/template.typ`) needs no change.
- Lists, block quotes, code blocks, the space around tables and figures, footnotes, callouts and
  equations beyond what criteria 5, 9 and 10 pin: they keep Typst's own spacing for now
  (design.md §0.3, "Scope"; R12 §8).
- `docs/**` and `PLAN.md`: the coordinator's.

## Hints (verified)
Everything below ran against the reference code on 2026-11-22.

- **The model in numbers.** The baseline of a block's first line is

  ```
  previous baseline + below(previous) + max(after(previous), before(next)) + above(next)
  ```

  and the first block of page 1 starts at `top margin + before + above`; on a later page the
  `before` is dropped. `above` and `below` are the two parts of a line box. In the design
  `spaced` (Libertinus Serif: single line 1.14 em, ascent 0.894 em; line height 1.35):

  | Text | Pitch | `above` | `below` |
  |---|---|---|---|
  | body, 11 pt | 14.85 | 9.834 | 5.016 |
  | `Heading 1`, 14 pt | 18.9 | 12.516 | 6.384 |
  | `Heading 2`, 12 pt | 16.2 | 10.728 | 5.472 |

  The paragraph spacing `P` is 3.6 mm = 10.205 pt, the top margin 30 mm = 85.039 pt. A body
  paragraph has (before, after) = (0, `P`); a div of a style takes the style's `space_before`
  and `space_after` where it sets them and (0, `P`) where it does not. A heading takes its
  style's spaces too, and where the style sets none it keeps Typst's built-in heading space
  (design.md §8.7); the design below sets both spaces on both heading styles, so no number here
  depends on that. Example: `WIDEONE` = 94.873 + 5.016 + max(10.205, 17) + 9.834 = 126.723.
- **The line box tests** use the package's own page margin, 2.5 cm = 70.866 pt, and Libertinus
  Serif's cap height, 0.658 em (7.238 pt at 11 pt). A multiple of 1.0 is below the font's single
  line, so the ascent shrinks to 1.0 / 1.14 of 9.834 pt. An exact pitch `h` puts 0.8 `h` above
  the baseline. Without a line height the pitch is Typst's own, the cap height plus 0.65 em
  (14.388 pt). `spacing: 20pt` keeps Typst's meaning, 20 pt from a baseline to the next line's
  cap height: 20 + 7.238 − 14.85 = 12.388 pt between the boxes.
- **Footnotes** (criterion 9). Typst sets a note at 0.85 em, 9.35 pt here, and the note's lines
  are line boxes at that size: 1.35 × 9.35 = 12.623 pt, of which 0.894 × 9.35 = 8.359 pt above
  the baseline. The last note's box ends at the bottom of the text area, 841.890 − 70.866 =
  771.024 pt down the page, so `NOTETHREE` is at 771.024 − 4.264, and each line before it is one
  box higher: the next note follows without a gap. At a line height of 1.0 the box is 9.35 pt
  (7.332 pt above the baseline, 2.018 pt below). Typst keeps 0.5 em of the body's size, 5.5 pt,
  from a note's last baseline to the next note's cap height; the box holds 0.85 × (11 − 7.238) =
  3.198 pt of that, and the remaining 2.302 pt stay between the two notes: `NOTETHREE` −
  `NOTETWO` = 9.35 + 2.302. At 1.35 the box holds more than 5.5 pt and nothing remains.
- **Equation rows** (criterion 10). A row of a display equation is as high as its letters, here
  7.755 pt above the baseline and 0.242 pt below it, and Typst puts the paragraph leading
  between two rows. The package gives a display equation Typst's own leading for the pitch, the
  pitch less the cap height: 14.85 − 7.238 = 7.612 pt, and 15 − 7.238 = 7.762 pt for the exact
  pitch. So `ROWONE` = `PARAA` + 5.016 + 10.205 + 7.755, `ROWTWO` = `ROWONE` + 0.242 + 7.612 +
  7.755 and `PARAB` = `ROWTWO` + 0.242 + 10.205 + 9.834. With the exact pitch `PARAA` is at
  82.866 and its box has 3 pt below the baseline.
- **Harness** (the whole top of the new file; the imports are exactly the ones the ten tests
  need, and clippy rejects an unused one):
  ```rust
  use std::path::Path;

  use typst::foundations::{Datetime, Label, Value};
  use typst::introspection::{Introspector, MetadataElem};
  use typst::layout::{Frame, FrameItem, Point, Transform};
  use typst::utils::PicoStr;
  use typst_layout::PagedDocument;
  use writer::assets::registry::AssetRegistry;
  use writer::config::Config;
  use writer::design::{Design, Layer, Origin, merge};
  use writer::typst::compile::compile_document;
  use writer::typst::sourcemap::SourceMap;
  use writer::typst::world::{MainFile, WriterWorld};

  const TOLERANCE: f64 = 0.01;

  /// Reports the line box, the leading and the paragraph gap in force where it
  /// stands, as JSON: lengths in pt, any other value as its `repr`.
  const PROBE: &str = r##"
  #let probe-edge(value) = {
      if type(value) == length { value.to-absolute().pt() } else { repr(value) }
  }
  #let probe = context [#metadata(json.encode((
      top: probe-edge(text.top-edge), bottom: probe-edge(text.bottom-edge),
      leading: par.leading.to-absolute().pt(),
      gap: par.spacing.to-absolute().pt(),
  ))) <probe>]
  "##;

  /// Two lines of one paragraph, a second paragraph and the probe.
  const LINES: &str = "LINEONE \\\nLINETWO\n\nPARATWO #probe\n";

  fn design(table: serde_json::Value, root: &Path) -> Design {
      merge(&[Layer {
          table: table.as_object().unwrap().clone(),
          base_dir: root,
          origin: Origin::FrontMatter,
      }])
      .unwrap()
  }

  /// Compiles `body` after the package's import line and the probe.
  fn compile(root: &Path, design: &Design, body: &str) -> PagedDocument {
      let mut config = Config::default();
      config.pdf.system_fonts = false;
      let mut world = WriterWorld::from_config_design(
          &config,
          design,
          &AssetRegistry::new(),
          root,
          MainFile::Virtual,
          Datetime::from_ymd(2026, 11, 21).unwrap(),
      )
      .unwrap();
      let source = format!(
          "#import \"@writer/doc:{}\": *\n{PROBE}\n{body}",
          writer::typst::WRITER_DOC_VERSION,
      );
      world.set_main_text(&source, SourceMap::default());
      compile_document(&mut world).into_result().unwrap().0
  }

  /// Every text run of `frame` with its baseline's page position, in pt.
  fn runs(frame: &Frame, transform: Transform, result: &mut Vec<(String, f64)>) {
      for (position, item) in frame.items() {
          let transform =
              transform.pre_concat(Transform::translate(position.x, position.y));
          match item {
              FrameItem::Group(group) => runs(
                  &group.frame,
                  transform.pre_concat(group.transform),
                  result,
              ),
              FrameItem::Text(run) => result.push((
                  run.text.to_string(),
                  Point::zero().transform(transform).y.to_pt(),
              )),
              _ => {}
          }
      }
  }

  /// Checks the baseline of every `(text, y)` pair on page `page` (from 1).
  fn assert_baselines(
      document: &PagedDocument, page: usize, expected: &[(&str, f64)],
  ) {
      let mut found = Vec::new();
      runs(&document.pages()[page - 1].frame, Transform::identity(), &mut found);
      for (marker, expected) in expected {
          let (_, actual) = found
              .iter()
              .find(|(text, _)| text == marker)
              .unwrap_or_else(|| panic!("no {marker} on page {page}: {found:?}"));
          assert!(
              (actual - expected).abs() < TOLERANCE,
              "{marker}: baseline at {actual} pt, expected {expected} pt"
          );
      }
  }

  /// What the probe reported.
  fn probe(document: &PagedDocument) -> serde_json::Value {
      let content = document
          .introspector()
          .query_label(Label::new(PicoStr::intern("probe")).unwrap())
          .unwrap();
      let metadata = content.to_packed::<MetadataElem>().unwrap();
      let Value::Str(text) = &metadata.value else {
          panic!("expected a JSON string, got {:?}", metadata.value)
      };
      serde_json::from_str(text.as_str()).unwrap()
  }

  /// Checks the numbers the probe reported, each `(key, pt)`.
  fn assert_probe(document: &PagedDocument, expected: &[(&str, f64)]) {
      let probe = probe(document);
      for (key, expected) in expected {
          let actual = probe[*key]
              .as_f64()
              .unwrap_or_else(|| panic!("{key} is no number: {probe}"));
          assert!(
              (actual - expected).abs() < TOLERANCE,
              "{key}: {actual} pt, expected {expected} pt"
          );
      }
  }
  ```
  The number of pages is `document.pages().len()`. In criterion 7 the two strings compare as
  `assert_eq!(probe(&document)["top"], "\"cap-height\"")`.
- **The two designs.**
  ```rust
  fn spaced(root: &Path) -> Design {
      design(
          serde_json::json!({
              "text": {
                  "size": "11pt", "line_height": 1.35,
                  "paragraph_spacing": "3.6mm",
              },
              "page": {"margin": {"top": "30mm"}},
              "styles": {
                  "Wide": {"space_before": "17pt", "space_after": "23pt"},
                  "Narrow": {"space_before": "5pt", "space_after": "4pt"},
                  "Flush": {"space_before": "0pt", "space_after": "0pt"},
                  "Bare": {"keep_together": true},
                  "Heading 1": {
                      "size": "14pt", "space_before": "20pt",
                      "space_after": "6pt",
                  },
                  "Heading 2": {
                      "size": "12pt", "space_before": "4pt",
                      "space_after": "2pt",
                  },
              },
          }),
          root,
      )
  }

  fn lines(root: &Path, text: serde_json::Value) -> Design {
      design(serde_json::json!({"text": text}), root)
  }
  ```
  `root` is a temporary directory: `let directory = tempfile::tempdir().unwrap();` and
  `directory.path()`, as in `tests/typst_leading.rs`.
- **The documents** (copy the strings as they are; a blank line or a `\n` more or less changes
  what Typst parses):
  ```rust
  const ADJACENT: &str = concat!(
      "#show: writer-doc\n",
      "PARAA\n\n",
      "#writer-div(style: \"Wide\")[WIDEONE\n\nWIDETWO]\n",
      "PARAB\n\n",
      "#writer-div(style: \"Narrow\")[NARROWONE]\n",
      "PARAC\n\n",
      "#writer-div(style: \"Flush\")[FLUSHONE]\n",
      "PARAD\n\n",
      "#writer-div(style: \"Bare\")[BAREONE\n\nBARETWO]\n",
      "PARAE\n\n",
      "#writer-div(style: \"Wide\")[WIDETHREE]\n",
      "#writer-div(style: \"Wide\")[WIDEFOUR]\n",
      "#writer-div(style: \"Narrow\")[NARROWTWO]\n",
      "PARAF\n\n",
      "= HEADONE\n\n",
      "PARAG\n\n",
      "== HEADTWO\n\n",
      "PARAH\n\n",
      "= HEADTHREE\n\n",
      "== HEADFOUR\n\n",
      "PARAI\n",
  );
  const FIRST_HEADING: &str = concat!(
      "#show: writer-doc\n", "= HEADFIRST\n\n", "PARAA\n\n",
      "#pagebreak()\n\n", "= HEADNEXT\n\n", "PARAB\n",
  );
  const FIRST_DIV: &str = concat!(
      "#show: writer-doc\n",
      "#writer-div(style: \"Wide\")[WIDEFIRST]\n",
      "PARAA\n",
  );
  const FIRST_PARAGRAPH: &str = "#show: writer-doc\n\nPARAA\n\nPARAB\n";
  const PAGE_RULE: &str = concat!(
      "#show: writer-doc\n\n",
      "#set page(width: 300pt, height: 200pt, margin: 10pt, numbering: none)\n\n",
      "= HEADFIRST\n\n",
      "PARAA\n",
  );
  const TEMPLATE_RULE: &str = concat!(
      "#let template(..args, body) = {\n",
      "    show: writer-doc.with(..args)\n",
      "    set page(margin: 40pt)\n",
      "    show heading: set text(fill: red)\n",
      "    body\n}\n",
      "#show: template\n\n",
      "= HEADFIRST\n\n",
      "PARAA\n",
  );
  const COVER: &str = concat!(
      "#show: writer-doc.with(title: [T], title-block: (meta, theme) => ",
      "page(margin: 5pt)[COVER])\n\n",
      "= HEADFIRST\n\n",
      "PARAA\n",
  );
  const LISTS: &str = concat!(
      "#show: writer-doc\n\n",
      "PARAA\n\n",
      "- TIGHTONE\n- TIGHTTWO\n\n",
      "PARAB\n\n",
      "- LOOSEONE\n\n- LOOSETWO\n\n",
      "PARAC\n\n",
      "#table(columns: 2, [CELLA], [], [], [])\n\n",
      "PARAD\n",
  );
  const NOTES: &str = concat!(
      "#show: writer-doc\n",
      "PARAA#footnote[NOTEONE \\ NOTETWO #probe]\n\n",
      "PARAB#footnote[NOTETHREE]\n",
  );
  const EQUATION: &str = concat!(
      "#show: writer-doc\n",
      "PARAA\n\n",
      "$ \"ROWONE\" #probe \\ \"ROWTWO\" $\n\n",
      "PARAB\n",
  );
  ```
  The line-box documents are built in the test: `format!("#show: writer-doc\n{LINES}")` and
  `format!("#show: writer-doc.with({argument})\n{LINES}")`. A line height that is a string
  (`"15pt"`) and one that is a number go into the design's `text` table as they are:
  `serde_json::json!({"size": "11pt", "line_height": line_height, "paragraph_spacing":
  "3.6mm"})` with `line_height` a `serde_json::Value` (`1.35.into()`, `"15pt".into()`).
- **What each document is for.** `PAGE_RULE` and `TEMPLATE_RULE`: the package puts an invisible
  mark in front of the body so that the first block keeps its space before; a page rule that
  came after that mark would start a second page, so the mark goes inside the body's leading
  rules (`open-flow`). `COVER`: a `title-block` function lays out its own page and gets no mark,
  so no empty page appears in front of it. `LISTS`: tight list items are 0 pt apart, loose ones
  `P`; the table's second row has only empty cells and is as high as the first. `NOTES` and
  `EQUATION`: Typst reads the paragraph leading in two places besides a paragraph's lines. A
  footnote sets a leading of its own, which would come on top of the note's line boxes, so the
  package sets it to zero there; a display equation puts the leading between its rows, which
  would touch at zero, so the package gives it Typst's leading for the pitch. In both documents
  the probe stands inside the element, where those rules are in force.
- **`probe.typ` for the manual check** (the design is passed by hand, so no `lines` table exists
  and the package measures the font itself):
  ```typst
  #import "@writer/doc:0.5.0": *
  #show: writer-doc.with(design: (
      text: (size: 11.0, line_height: (multiple: 1.35), paragraph_spacing: 10.205),
  ))
  First paragraph

  Second paragraph #context [#metadata((
      repr(text.top-edge), repr(text.bottom-edge), repr(par.leading),
      repr(par.spacing),
  )) <edges>]
  ```
  A style dictionary passed by hand must hold every key of design.md §3.3 (`writer-div` reads
  `keep_together` and the others without a default), which is why this probe declares none.
- **Formatting.** `rustfmt tests/typst_spacing.rs` formats the new file alone. rustfmt packs
  short `concat!` arguments onto shared lines and leaves a statement with a string longer than
  the line as it is; both are fine. Do not reformat files you do not own.
- A text run's string is the marker alone (`PARAA`, `HEADONE`); list markers and the page number
  are runs of their own, so `text == marker` finds exactly one run per marker and page.
- Cargo test filters go after `--`: `cargo test --test typst_spacing -- lists`.
- The crate's dependencies are vendored (`.cargo/config.toml`), so every command works offline.
