# T99 — HTML: vocabulary styles, a div's spaces as its edges, empty table cells

| | |
|---|---|
| Milestone | M8 |
| Size | S |
| Depends on | T98 |
| Parallel-safe with | nothing: T100 starts after this task |
| Files you own | `src/html/design.rs`, `tests/html_design.rs` (two imports, one helper pair and seven new tests at the end), `tests/snapshots/design_styles__named_styles_css.snap`, `tests/snapshots/html_design__letter_theme.snap`, `tests/snapshots/html_furniture__fixed_furniture_css.snap` |

## Goal
HTML follows the spacing model of `docs/spec/design.md` §0.3 with CSS's own means, in three parts:

1. **Vocabulary styles.** The style names that set headings, the title block, the contents list,
   captions and table cells in Word and the PDF set the same elements on the page. A new
   function, `vocabulary_css`, writes one scoped rule per element (design.md §8.8). In the title
   block the subtitle, the authors and the date take `Title`'s `align` when their own style sets
   none, as they do in Word (whose styles are based on `Title`) and in the PDF.
2. **A div is one block.** A named style's `space_before` and `space_after` are the whole space
   at the div's edges: `styles_css` adds a rule that removes the top margin of the div's first
   child when `space_before` is set, and the bottom margin of its last child when `space_after`
   is set (design.md §8.3).
3. **Empty cells.** An empty table cell holds one line, as in Word (design.md §8.8).

The Rust code is written and tested already: `docs/research/R12-spacing-model.md` §5.5 holds it as
one diff. This task installs it, accepts three snapshots after checking each added line, and
writes seven tests.

## Read first
- `docs/spec/design.md` §5.3 (item 3), §5.4, §8.3 and §8.8
- `docs/research/R12-spacing-model.md` §4.3 (what the code does), §7 (decision D2: where the rules
  stand in the cascade), the introduction of §5 (how a block is extracted)
- `src/html/design.rs`: `layout_css` and `styles_css`, before you change the file

## API to implement
```rust
// src/html/design.rs (R12 §5.5 is the complete change)
/// The rules of design.md §8.8, one per line, each ending with a newline.
pub fn vocabulary_css(design: &Design) -> String;
// `styles_css` keeps its signature and writes up to two more rules per style.
// `layout_css` writes `vocabulary_css` after `styles_css` and before the furniture.
```

## Steps
1. Record what the tree holds before this task. From the repository root:
   ```sh
   git status --short > target/t99-before.txt
   ```
2. Apply R12 §5.5's diff (343 lines; it changes `src/html/design.rs` alone):
   ```sh
   awk '/^```diff html\.diff$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R12-spacing-model.md > html.diff
   git apply --check html.diff && git apply html.diff && rm html.diff
   ```
   If `git apply --check` fails, stop and report; do not rewrite the functions.
3. Accept the three snapshots that change:
   ```sh
   INSTA_UPDATE=always cargo test --test design_styles --test html_design --test html_furniture
   git status --short tests/snapshots | diff target/t99-before.txt - | grep snapshots
   ```
   The second command lists the snapshots this task changed. It must print exactly three lines,
   one `>  M` line for each snapshot file under "Files you own", and no `.snap.new` file.
4. Check each snapshot's change against acceptance criterion 8: every added line is listed
   there, and no line is removed.
5. Add the two imports, the two helpers and the seven tests to `tests/html_design.rs`, the tests
   in the order of acceptance criteria 1 to 7 ("Hints" has the helpers and the design).
6. Run `cargo test --test html_design`, then the whole `cargo test` (without `INSTA_UPDATE`),
   then the other three quality gates.

## Acceptance criteria
Every expected string below is the function's whole output: one rule per line, each line ending
with `\n`, the last one too. Compare with `assert_eq!`.

1. `vocabulary_styles_write_scoped_rules_in_a_fixed_order`: `vocabulary_css(&vocabulary_design())`
   is these 17 lines:
   ```css
   :where([data-writer-document]) h1:where(:not(.title)),:where([data-writer-document]) .references-title{font-size:14pt;color:#0a2f41;font-weight:700;text-align:left;margin-top:20pt;margin-bottom:6pt;break-after:avoid}
   :where([data-writer-document]) h2{font-size:12pt;line-height:1.2}
   :where([data-writer-document]) h1.unlisted,:where([data-writer-document]) h1.notoc,:where([data-writer-document]) .abstract h1{font-size:18pt}
   :where([data-writer-document]) p.toc-title{font-size:16pt;color:#0a2f41;font-weight:400;text-align:left;margin-top:20pt;margin-bottom:6pt;break-after:avoid}
   :where([data-writer-document]) nav.toc>ol>li>a{display:block;font-weight:700;margin-top:6pt}
   :where([data-writer-document]) nav.toc>ol,:where([data-writer-document]) nav.toc>ol>li{margin-top:0}
   :where([data-writer-document]) nav.toc>ol>li>ol>li>a{display:block;font-style:italic}
   :where([data-writer-document]) figcaption{font-size:9pt;font-style:italic}
   :where([data-writer-document]) .kind-table>figcaption{font-weight:700}
   :where([data-writer-document]) .kind-image>figcaption{color:#575757}
   :where([data-writer-document]) th,:where([data-writer-document]) td{font-size:9pt;text-align:left;padding-top:2pt;padding-bottom:3pt}
   :where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:"\200b"}
   :where([data-writer-document]) .title{font-size:30pt;font-weight:400;text-align:left;margin-top:40pt}
   :where([data-writer-document]) p.subtitle{font-size:13pt;text-align:left}
   :where([data-writer-document]) p.author{font-size:10pt;text-align:left;margin-top:0;margin-bottom:0}
   :where([data-writer-document]) .authors{margin-top:4pt;margin-bottom:8pt;justify-content:flex-start}
   :where([data-writer-document]) p.date{font-size:9pt;text-align:left;margin-bottom:12pt}
   ```
   What the lines show: the contents title takes `Heading 1`'s keys with `TOC Heading`'s over
   them (line 4); `toc 3`, which sets no key that HTML writes, has no rule; a caption's
   `space_before` is not written (line 8); `Compact`'s spaces are cell padding (line 11); the
   subtitle and the date, whose styles set no `align`, take `Title`'s (lines 14 and 17); the
   authors' spaces move to their shared row (lines 15 and 16).
2. `the_contents_title_lays_its_keys_over_heading_1s`, two designs:
   - `{"styles": {"TOC Heading": {"size": "16pt"}}}` gives
     ```css
     :where([data-writer-document]) p.toc-title{font-size:16pt}
     :where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:"\200b"}
     ```
   - `{"styles": {"Heading 1": {"size": "14pt", "space_before": "20pt"}}}` gives
     ```css
     :where([data-writer-document]) h1:where(:not(.title)),:where([data-writer-document]) .references-title{font-size:14pt;margin-top:20pt}
     :where([data-writer-document]) p.toc-title{font-size:14pt;margin-top:20pt}
     :where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:"\200b"}
     ```
3. `a_design_without_vocabulary_styles_writes_the_empty_cell_rule_alone`: the design `{"text":
   {"size": "11pt"}}` gives the one line
   ```css
   :where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:"\200b"}
   ```
4. `vocabulary_names_match_by_key_and_justified_authors_keep_their_row`: the design `{"styles":
   {"heading-2": {"size": "12pt"}, "Author": {"align": "justify"}}}` gives
   ```css
   :where([data-writer-document]) h2{font-size:12pt}
   :where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:"\200b"}
   :where([data-writer-document]) p.author{text-align:justify}
   ```
   (`heading-2` is `Heading 2` by key; `justify` has no `justify-content` value and the style
   sets no space, so `.authors` gets no rule.)
5. `a_divs_spaces_replace_its_first_and_last_childs_margins`: with `css =
   styles_css(&vocabulary_design())`,
   - these are lines of `css` (compare whole lines: `css.lines().any(|line| line == ..)`):
     ```css
     [data-style="Closing"]{margin-top:17.008pt;margin-bottom:22.677pt;break-after:avoid}
     [data-style="Closing"]>:first-child{margin-top:0}
     [data-style="Closing"]>:last-child{margin-bottom:0}
     [data-style="Recipient"]>:last-child{margin-bottom:0}
     [data-style="Caption"]>:first-child{margin-top:0}
     [data-style="Note"]{text-align:center}
     [data-style="Note"] p{text-align:inherit}
     ```
   - these are not lines of `css`: `[data-style="Recipient"]>:first-child{margin-top:0}` and
     `[data-style="Caption"]>:last-child{margin-bottom:0}` (a rule is written for a set space
     only);
   - `css` does not contain `[data-style="Note"]>` nor `[data-style="Subtitle"]>` (no space set:
     the paragraphs inside keep their own margins).
6. `vocabulary_rules_stand_between_named_styles_and_furniture`: render the letter fixture as
   `letter_tokens_precede_theme_entries_and_layout_follows_them` does. In `rendered.theme_css`,
   from `/* design: layout */` on, these four strings appear in this order:
   `[data-style="Signature"]>:last-child{margin-bottom:0}`,
   `:where([data-writer-document]) td:empty::before,`,
   `:where([data-writer-document]) p.date{font-size:10pt;margin-bottom:8.504pt}`,
   `.w-furniture{`.
7. `title_block_parts_follow_the_titles_alignment`: the design `{"styles": {"Title": {"align":
   "right"}, "Date": {"align": "center"}}}` gives
   ```css
   :where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:"\200b"}
   :where([data-writer-document]) .title{text-align:right}
   :where([data-writer-document]) p.subtitle{text-align:right}
   :where([data-writer-document]) p.author{text-align:right}
   :where([data-writer-document]) .authors{justify-content:flex-end}
   :where([data-writer-document]) p.date{text-align:center}
   ```
   (A part without a style of its own still follows the title, and a part's own `align` holds.)
8. The three snapshots change by added lines only. `git diff <file> | grep '^[-+][^-+]'` prints:
   - for `tests/snapshots/design_styles__named_styles_css.snap`, these eight lines:
     ```
     +[data-style="Closing"]>:first-child{margin-top:0}
     +[data-style="Closing"]>:last-child{margin-bottom:0}
     +[data-style="Date"]>:last-child{margin-bottom:0}
     +[data-style="Recipient"]>:last-child{margin-bottom:0}
     +[data-style="Salutation"]>:first-child{margin-top:0}
     +[data-style="Salutation"]>:last-child{margin-bottom:0}
     +[data-style="Signature"]>:first-child{margin-top:0}
     +[data-style="Signature"]>:last-child{margin-bottom:0}
     ```
   - for `tests/snapshots/html_design__letter_theme.snap`, the same eight lines and then
     ```
     +:where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:"\200b"}
     +:where([data-writer-document]) p.date{font-size:10pt;margin-bottom:8.504pt}
     ```
   - for `tests/snapshots/html_furniture__fixed_furniture_css.snap`, the one line
     ```
     +:where([data-writer-document]) td:empty::before,:where([data-writer-document]) th:empty::before{content:"\200b"}
     ```
   No other snapshot changes: in particular none of `tests/snapshots/design_styles__named_
   styles_word.snap` and the DOCX snapshots (T100's).
9. The installed code is the reference code: extract `html.diff` again as in step 2 and run
   `git apply --check --reverse html.diff` (it must succeed; remove the file afterwards).
10. In the output of `git status --short | diff target/t99-before.txt -` no line starts with `<`,
    and the lines that start with `>` are exactly these five (remove the file in `target/`
    afterwards):
    ```
    >  M src/html/design.rs
    >  M tests/html_design.rs
    >  M tests/snapshots/design_styles__named_styles_css.snap
    >  M tests/snapshots/html_design__letter_theme.snap
    >  M tests/snapshots/html_furniture__fixed_furniture_css.snap
    ```
11. `tests/html_design.rs` runs 15 tests (eight and the seven new ones). `cargo fmt --check`,
    `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
    `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Changing the reference code. It ran with all four gates green on 2026-11-21, and the strings
  above are its output. If a test you wrote fails, the test is wrong: compare its strings with
  this file character by character. If an existing test fails, stop and report.
- The theme's own CSS (`assets/themes/**`): no file there changes. The new rules stand after the
  theme's entries and win over a theme rule of the same specificity; that is decision D2 of R12
  §7 and design.md §5.4, not something to adjust here.
- The line box: HTML keeps CSS's half-leading (design.md §0.3, "Agreement").
- Word (T100), the guide (T101), `docs/**` and `PLAN.md`.

## Hints (verified)
Everything below ran against the reference code on 2026-11-21.

- **Imports and helpers** for `tests/html_design.rs` (the file has no design helper yet):
  ```rust
  use writer::design::{Design, Layer, Origin, merge};
  use writer::html::design::{styles_css, vocabulary_css};

  fn design(table: serde_json::Value) -> Design {
      merge(&[Layer {
          table: table.as_object().unwrap().clone(),
          base_dir: Path::new(env!("CARGO_MANIFEST_DIR")),
          origin: Origin::FrontMatter,
      }])
      .unwrap()
  }

  /// One style for every element the vocabulary reaches in HTML, and three
  /// (`Recipient`, `Closing`, `Note`) that only divs use.
  fn vocabulary_design() -> Design {
      design(serde_json::json!({
          "text": {"size": "11pt"},
          "styles": {
              "Heading 1": {
                  "size": "14pt", "weight": 700, "color": "#0a2f41",
                  "align": "left", "space_before": "20pt",
                  "space_after": "6pt", "keep_with_next": true,
              },
              "Heading 2": {"size": "12pt", "line_height": 1.2},
              "Heading 1 Unlisted": {"size": "18pt"},
              "TOC Heading": {"size": "16pt", "weight": 400},
              "toc 1": {"weight": 700, "space_before": "6pt"},
              "toc 2": {"italic": true},
              "toc 3": {"keep_together": false},
              "Caption": {
                  "size": "9pt", "italic": true, "space_before": "4pt",
              },
              "Table Caption": {"weight": 700},
              "Image Caption": {"color": "#575757"},
              "Compact": {
                  "size": "9pt", "align": "left", "space_before": "2pt",
                  "space_after": "3pt",
              },
              "Title": {
                  "size": "30pt", "weight": 400, "align": "left",
                  "space_before": "40pt",
              },
              "Subtitle": {"size": "13pt"},
              "Author": {
                  "size": "10pt", "align": "left", "space_before": "4pt",
                  "space_after": "8pt",
              },
              "Date": {"size": "9pt", "space_after": "12pt"},
              "Recipient": {
                  "line_height": "12.1pt", "space_after": "6mm",
                  "keep_together": true,
              },
              "Closing": {
                  "space_before": "6mm", "space_after": "8mm",
                  "keep_with_next": true,
              },
              "Note": {"align": "center"},
          },
      }))
  }
  ```
- **Writing the expected strings.** In Rust source the empty-cell rule's `"\200b"` needs its
  quotes and its backslash escaped: `{content:\"\\200b\"}`. A multi-line expectation reads well
  as `concat!("first line\n", "second line\n")`, one line of CSS per argument. These lines are
  longer than 80 columns; `AGENTS.md` exempts exact-output strings, and rustfmt leaves a
  statement whose string cannot fit as you wrote it. A statement it can still fit by wrapping,
  it wraps. `rustfmt tests/html_design.rs` formats the one file; run it before the gates.
- **Criterion 6** needs no new fixture: the first test of the file shows how to render
  `tests/fixtures/design/letter/letter.md` (`pipeline::prepare`, then `render(.., OutputMode::
  Standalone, BuildInfo::build(true))`). Take the slice from `css.find("/* design: layout */")`
  and compare the four `find` positions.
- **Where the lengths come from.** `6mm` is 17.008 pt and `8mm` 22.677 pt (×72/25.4, three
  decimals). The letter fixture's `Date` style sets `size = "10pt"` and `space_after = "3mm"`,
  which is why its sheet gains a `p.date` rule.
- With `INSTA_UPDATE=always`, insta may also drop an `assertion_line:` header line from a
  snapshot it rewrites. That is harmless; it did not happen for these three files in the
  reference run.
- Cargo test filters go after `--`: `cargo test --test html_design -- vocabulary`.
- The crate's dependencies are vendored (`.cargo/config.toml`), so every command works offline.
