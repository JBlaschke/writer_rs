# T100 — Word: a styled div is one block

| | |
|---|---|
| Milestone | M8 |
| Size | S |
| Depends on | T99 |
| Parallel-safe with | nothing: T101 starts after this task |
| Files you own | `src/docx/body.rs`, `src/docx/columns.rs`, `src/docx/design.rs`, `src/docx/styles.rs`, `tests/design_styles.rs` (one assertion), `tests/docx_div_spacing.rs` (new), `tests/snapshots/design_styles__named_styles_word.snap`, `tests/snapshots/docx_design__design_letter_styles.snap` |

## Goal
In Word, the paragraphs of a div with a named style form one block, as they do in HTML and the
PDF (`docs/spec/design.md` §0.3 and §8.5):

- the style's `space_before` and `space_after` are the block's outer spaces: the first paragraph
  keeps the space before, the last one the space after;
- between themselves the paragraphs keep the spacing they have outside a div (`Body Text`'s, or
  `Compact`'s for an item of a tight list);
- two consecutive divs of one style are two blocks, with the larger of their two spaces between
  them.

Until now the style carried `w:contextualSpacing`, which made Word drop every space between two
paragraphs of the same style: none inside a div, and none between two divs of one style either.
After this task only the `Header` and `footer` styles carry it, and the emitter writes the inner
spacing on the paragraphs themselves when a div ends.

This is decision D1 of `docs/research/R12-spacing-model.md` §7. It changes how existing documents
look in Word, and the maintainer signs it off when the pull request is reviewed. The task does
not depend on that: do the work as written.

The Rust code is written and tested already: R12 §5.6, §5.7 and §5.8 hold it as two diffs and one
whole test file. This task installs them, accepts two snapshots after checking the change, and
runs one manual check.

## Read first
- `docs/spec/design.md` §8.5, the paragraph "A div is one block"
- `docs/research/R12-spacing-model.md` §2.5 (what Word does with a div), §4.4 (what the code
  does), §7 (decision D1), the introduction of §5 (how a block is extracted)
- `src/docx/body.rs`: the function that writes a `Div` and `start_paragraph`, before you change
  the file

## API to implement
```rust
// src/docx/styles.rs (R12 §5.6 is the complete change)
impl StyleTable {
    // for a design div style writer wrote with a space key: the space before and after
    // (twips) the paragraphs of one div keep between themselves; `None` for any other id
    pub(crate) fn block_spacing(&self, id: &str) -> Option<(u32, u32)>;
    // the space before and after (twips) a paragraph of style `id` gets from its style
    pub(crate) fn spacing_of(&self, id: &str) -> (u32, u32);
}

// src/docx/body.rs
impl Emitter<'_> {
    fn record_div_paragraph(&mut self, props: &ParaProps);  // called by `start_paragraph`
    fn space_div(&mut self, block: DivBlock);               // called when a div ends
}

// src/docx/columns.rs
impl Emitter<'_> {
    // keeps the recorded paragraph offsets valid after `added` bytes were written at `at`
    pub(crate) fn body_grew(&mut self, at: usize, added: usize);
}
```

## Steps
1. Record what the tree holds before this task. From the repository root:
   ```sh
   git status --short > target/t100-before.txt
   ```
2. Apply R12 §5.6's diff (430 lines; it changes `src/docx/body.rs`, `src/docx/columns.rs`,
   `src/docx/design.rs` and `src/docx/styles.rs`):
   ```sh
   awk '/^```diff word\.diff$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R12-spacing-model.md > word.diff
   git apply --check word.diff && git apply word.diff && rm word.diff
   ```
   If `git apply --check` fails, stop and report; do not rewrite the functions.
3. Apply R12 §5.7's diff (15 lines; one assertion of `tests/design_styles.rs`) the same way:
   ```sh
   awk '/^```diff word-tests\.diff$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R12-spacing-model.md > word-tests.diff
   git apply --check word-tests.diff && git apply word-tests.diff && rm word-tests.diff
   ```
4. Create `tests/docx_div_spacing.rs` from R12 §5.8's block (the whole file, 213 lines):
   ```sh
   awk '/^```rust docx_div_spacing\.rs$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R12-spacing-model.md > tests/docx_div_spacing.rs
   wc -l tests/docx_div_spacing.rs
   ```
5. Accept the two snapshots that change:
   ```sh
   INSTA_UPDATE=always cargo test --test design_styles --test docx_design
   git status --short tests/snapshots | diff target/t100-before.txt - | grep snapshots
   ```
   The second command lists the snapshots this task changed. It must print exactly two lines,
   one `>  M` line for each snapshot file under "Files you own", and no `.snap.new` file.
6. Check the two snapshots against acceptance criterion 5.
7. Run `cargo test --test docx_div_spacing`, the unit tests of criterion 2, then the whole
   `cargo test` (without `INSTA_UPDATE`), then the other three quality gates.
8. Do the manual check of criterion 9.

## Acceptance criteria
1. `cargo test --test docx_div_spacing` runs four tests and they pass:
   `paragraphs_of_one_div_keep_body_spacing_between_themselves`,
   `tight_list_items_in_a_div_keep_compact_spacing`, `nested_divs_are_blocks_of_their_own` and
   `a_section_break_after_a_div_stays_in_its_last_paragraph`.
2. The three unit tests that come with the diff pass. This runs exactly these three:
   ```sh
   cargo test --lib -- only_header_and_footer_lines_get_contextual_spacing \
       spacing_follows_the_base_chain_to_the_document_defaults \
       div_styles_with_a_space_key_report_the_spacing_inside_a_div
   ```
3. `cargo test --test design_styles --test docx_design` passes (four tests each).
   `tests/design_styles.rs` now asserts that the generated styles hold no `w:contextualSpacing`.
4. With the design of "Hints", the paragraphs of a div that holds three paragraphs have these
   properties in `word/document.xml` (the content of each `w:pPr`):

   | paragraph | properties |
   |---|---|
   | first | `<w:pStyle w:val="Closing"/><w:spacing w:after="200"/>` |
   | middle | `<w:pStyle w:val="Closing"/><w:spacing w:before="0" w:after="200"/>` |
   | last | `<w:pStyle w:val="Closing"/><w:spacing w:before="0"/>` |

   A div of one paragraph has `<w:pStyle w:val="Closing"/>` alone, and the paragraphs of a div
   whose style sets no space key have their `w:pStyle` alone. `tests/docx_div_spacing.rs` asserts
   all of this; it is listed here so you can read a failure.
5. The two snapshots change in one way only: `<w:contextualSpacing/>` is gone. Each of these two
   commands prints either nothing or the one removed header line `< assertion_line: NN` (with
   its `NdN` line above it):
   ```sh
   git show HEAD:tests/snapshots/design_styles__named_styles_word.snap | sed 's|<w:contextualSpacing/>||g' | diff - tests/snapshots/design_styles__named_styles_word.snap
   git show HEAD:tests/snapshots/docx_design__design_letter_styles.snap | sed 's|<w:contextualSpacing/>||g' | diff - tests/snapshots/docx_design__design_letter_styles.snap
   ```
   and `grep -c contextualSpacing` prints `0` for both snapshot files.
6. No other snapshot changes: after the whole `cargo test`, step 5's second command still prints
   the two files of criterion 5 and nothing else.
7. The installed code is the reference code: extract `word.diff` and `word-tests.diff` again as
   in steps 2 and 3 and run `git apply --check --reverse` on each (both must succeed; remove the
   files afterwards). Extract the test file again to `target/docx_div_spacing.rs` and
   `cmp target/docx_div_spacing.rs tests/docx_div_spacing.rs` prints nothing.
8. In the output of `git status --short | diff target/t100-before.txt -` no line starts with `<`,
   and the lines that start with `>` are exactly these eight (remove the files in `target/`
   afterwards):
   ```
   >  M src/docx/body.rs
   >  M src/docx/columns.rs
   >  M src/docx/design.rs
   >  M src/docx/styles.rs
   >  M tests/design_styles.rs
   >  M tests/snapshots/design_styles__named_styles_word.snap
   >  M tests/snapshots/docx_design__design_letter_styles.snap
   > ?? tests/docx_div_spacing.rs
   ```
9. Manual check with the command line. Write the two files of "Hints" into `target/t100-manual/`
   and run:
   ```sh
   cargo run -q -- build target/t100-manual/doc.md --to docx -o target/t100-manual/doc.docx
   unzip -p target/t100-manual/doc.docx word/document.xml | grep -o '<w:pStyle w:val="Closing"/><w:spacing[^>]*>'
   unzip -p target/t100-manual/doc.docx word/styles.xml | grep -c contextualSpacing
   ```
   The second command prints the three lines of criterion 4's table, in that order; the third
   prints `0`. Remove `target/t100-manual/` afterwards.
10. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
    `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Changing the reference code or the test file. They ran with all four gates green on
  2026-11-21. If a test fails, stop and report what failed and its output.
- The header and footer furniture: their styles keep `w:contextualSpacing`, because their lines
  are paragraphs of one block with nothing between them.
- A reference document's styles: when `reference.docx` defines the style name, its definition
  wins and writer writes no direct spacing (design.md §8.5, last list item).
- Lists, block quotes, code blocks and tables outside a styled div: they keep their built-in
  spacing (the follow-up of R12 §8).
- HTML (T99), the PDF (T98), the guide (T101), `docs/**` and `PLAN.md`.

## Hints (verified)
Everything below ran against the reference code on 2026-11-21.

- **The design and the document** of criteria 4 and 9. `target/t100-manual/writer.toml`:
  ```toml
  [design.text]
  paragraph_spacing = "10pt"

  [design.styles.Closing]
  space_before = "17pt"
  space_after = "23pt"
  ```
  `target/t100-manual/doc.md`:
  ```markdown
  Before

  :::closing
  First

  Middle

  Last
  :::

  After
  ```
  Create the directory with `mkdir -p target/t100-manual` and write the files with your editor
  (`target/` is ignored by git).
- **Reading the numbers.** Word counts spacing in twentieths of a point: 10 pt is `200`, 17 pt is
  `340`, 23 pt is `460`. `text.paragraph_spacing = "10pt"` makes `Body Text` a style with no
  space before and 10 pt after, and `Closing` is based on `Body Text`. So inside the div every
  paragraph but the first gets `w:before="0"` and every paragraph but the last `w:after="200"`,
  as direct formatting, while the style itself says `<w:spacing w:before="340" w:after="460"/>`.
  The first paragraph therefore has 17 pt before and 10 pt after, the last one none before and
  23 pt after.
- An item of a tight list inside the div takes `Compact`'s spacing instead of `Body Text`'s:
  1.8 pt before and after (`w:before="36" w:after="36"`) with writer's built-in styles.
- **Why the spacing is written when the div ends.** Only then is the last paragraph known. The
  emitter notes where each paragraph's `w:pPr` begins in the buffer, and `space_div` rewrites
  them last first so that the earlier offsets stay valid. Writing into the middle of the buffer
  moves what follows, so `body_grew` shifts the section state that points behind the edit; the
  fourth test covers that (a section break that follows a div stays in its last paragraph).
- With `INSTA_UPDATE=always`, insta drops the `assertion_line:` header line from a snapshot it
  rewrites. That is the one extra line criterion 5 allows.
- Cargo test filters go after `--`, and several filters select the tests that match any of them.
- The crate's dependencies are vendored (`.cargo/config.toml`), so every command works offline.
