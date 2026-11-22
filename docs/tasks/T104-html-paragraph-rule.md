# T104 — HTML paragraph rule: alignment on the document, margins under `:where()`

**Status: brief, not ready for hand-off.** The CSS is the coordinator's to prototype in probe
*center* first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | S |
| Depends on | T101 |
| Parallel-safe with | T102, T103, T105, T107 (wave 26; no shared file) |
| Files you own | `src/html/design.rs` (the paragraph part of `layout_css` only), `tests/html_design.rs` (the existing assertion of the paragraph rule, :57 at main, and new tests at the end), `tests/snapshots/html_design__letter_theme.snap` (at main the only snapshot that holds the rule; the coordinator recounts after T101) |

## Goal
When a declared design sets `text.paragraph_spacing` or `text.justify`
(`src/html/design.rs:143-152` at main), `layout_css` writes `[data-writer-document] p{…}` with the
margin `margin-block:0 P`, the alignment, or both (:153-157, the format string at :155), at
specificity 0,1,1 after the theme. Because it matches every `p`, it beats what the theme gives
paragraphs by inheritance or by a weaker rule (R13 §1.2, probe *center*):
- the subtitle, the authors and the date go to the start edge (centred only by inheritance from
  `.title-block`, `theme.css:112`);
- paragraphs in `::: {.center}` and `.right` divs lose their alignment (`theme.css:259-261`);
- the margins of `.subtitle` (:120), `.date` (its 1em top margin, :134) and `.toc-title` (:276)
  are replaced;
- the last paragraph of a block quote, an abstract, a table cell and a callout gets its margin
  back (`theme.css:109`, :143, :166, :250).

Today it reaches declared designs that set one of those keys; once T108's built-in design sets
both, it reaches every document, so it is fixed first. `text-align` moves to `[data-writer-document]`, where it is
inherited, and the paragraph margin goes under `:where()`, so that the theme's own rules for those
elements win again.

## Read first
- `docs/research/R13-single-source-defaults.md` §1.2 ("The paragraph-rule bug")
- `docs/spec/design.md` §5.3, §5.4 and §8.8 (where the generated rules stand in the cascade;
  R12's decision D2)
- `src/html/design.rs`: `layout_css` (:135 at main; after T101 also R12's `vocabulary_css`,
  which `layout_css` writes after this rule)
- `assets/themes/default/styles/theme.css:96-143`, :164-166, :245-263

## API to implement
None. `layout_css` keeps its signature (`src/html/design.rs:135`) and its order of rules.

## Steps
1. Install the coordinator's prototype CSS in the paragraph part of `layout_css`.
2. Update the existing assertion
   `"[data-writer-document] p{margin-block:0 10.205pt;text-align:start}"` (`tests/html_design.rs:57`
   at main) to the new rules.
3. Add tests at the end of `tests/html_design.rs` for the acceptance criteria that do not need a
   browser (the generated CSS text, the rule order).
4. Accept the snapshot change after reading every changed line.

## Acceptance criteria
1. In probe *center* (computed styles in Chrome): the subtitle, the date and `.center > p` are
   centred; the last children of a block quote, an abstract, a cell and a callout have margin 0;
   `.date` keeps its 1em top margin; with `justify = true` body paragraphs are still justified.
2. R12's p3 HTML positions are unchanged within 0.05 pt, read with the probe tools the
   coordinator commits for T102.
3. R12's T99 vocabulary rules still come after this rule in `layout_css`, and none has lower
   specificity than its `:where()` margin rule; T99's seven tests at the end of
   `tests/html_design.rs` pass unchanged.
4. Only `html_design__letter_theme.snap` changes among the snapshots, in the paragraph rule
   alone.
5. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- The vocabulary rules (R12's T99) and the built-in values (T108).
- Removing T99's rule that gives `p.subtitle`, `p.author` and `p.date` `Title`'s alignment: it
  stays unless T99's tests show it redundant after this change.

## Hints (verified)
- `:where()` alone does not restore the alignment: any rule that matches the `p`, even at
  specificity 0, beats an inherited value (R13 §1.2).
- `.abstract` is left-aligned in the default theme (`theme.css:135`), so for the abstract title
  and the contents title only the margins change (R13 §1.2).
- R12 §8 records that this rule is T104's and that T99 does not touch it.

## Before hand-off
- The coordinator prototypes the CSS in probe *center* and records the computed styles before
  and after, including a heading and a multi-line paragraph under `justify = true`.
- After T101 merges, the coordinator re-cites `layout_css`'s lines and recounts the snapshots
  that contain `[data-writer-document] p{`.
- Criterion 2 needs the probe tools before T102 lands; the coordinator commits them first or
  measures at review.
