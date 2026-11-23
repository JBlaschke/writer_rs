# T117 — Div-class styles, weights and keep rules

**Status: brief, not ready for hand-off.** A prototype of the face lookup comes first (see
"Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T116 |
| Parallel-safe with | nothing: the only task of wave 37 |
| Files you own | the `center`, `left`, `right`, `small` and `large` styles of `src/design/defaults/styles.toml` and their bits in `APPLIED`; `assets/themes/default/styles/theme.css:259-263` (removed); `src/docx/design.rs` (the style-level keep rules; the face chosen from `font` plus `weight`); `src/docx/body.rs` (R12's T100 `record_div_paragraph` and `space_div`: record a div whose style sets `keep_together` or `keep_with_next` even without a space key, and write `w:keepNext` per paragraph when it closes; per-paragraph `keep_next` via `ParaProps`, :338 and :537 at main); `tests/docx_div_spacing.rs` (new tests for the keep rules); the snapshots that change |

## Goal
The theme's div classes become built-in styles, so `::: {.center}` is centred and `.small` is
0.85em in all three formats. Word's keep rules match the other formats: `keepNext` on every
paragraph of a `keep_together` div but the last, and on the last only for `keep_with_next`. A
`weight` such as 600 selects a semibold face in Word when the family's files are readable
(design.md open point 15).

## Read first
- `docs/spec/design.md` §0.4, §8 (named styles), open point 15 (a weight Word cannot select)
- `docs/research/R13-single-source-defaults.md` §2.8 (rows 5.8, 5.9), §4.3 (row Div classes)

## API to implement
None: the styles use existing keys.

## Steps
1. Add the five styles and switch their bits on; remove `theme.css:259-263`.
2. Word's keep rules per paragraph. `style_xml` writes `w:keepNext` on the style, so it reaches
   every paragraph, the last included; per-paragraph keeps can be written only when the div
   closes, as R12's `space_div` does for spacing. Extend `record_div_paragraph` to record a div
   whose style sets `keep_together` or `keep_with_next` even without a space key, and set
   `keep_next` on its paragraphs in `space_div`.
3. The face lookup from `font` and `weight`.
4. Accept the snapshots after reading them.

## Acceptance criteria
1. `::: {.center}` is centred and `.small` is 0.85em in all three.
2. Rows 5.8 and 5.9 behave alike in Word's XML (exact).
3. `weight = 600` selects a semibold face in Word when the family's files are readable.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- `span.overline` (`theme.css:264`; T118).
- LaTeX, unless the coordinator adds it at hand-off (T120 maps only T108's domains).

## Hints (verified)
- `theme.css` at main: `div.left`, `div.center`, `div.right` (:259-261), `.small` .85em (:262),
  `.large` 1.25em (:263).
- `src/docx/design.rs` at main: `style_xml` (:481) sets `w:keepNext` for `keep_with_next` and
  `w:keepLines` for `keep_together` (:514-518), on the style, so for every paragraph.
- `src/docx/body.rs` at main: `ParaProps.keep_next` is set per paragraph (`inline_paragraph`
  :338, a heading's paragraph :537). R12's T100 adds `record_div_paragraph` and `space_div`
  (spacing-model), which see a div's paragraphs when it closes.

## Before hand-off
- The coordinator prototypes the face lookup.
