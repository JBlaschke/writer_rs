# T116 — Furniture, page numbers and page background

**Status: brief, not ready for hand-off.** Decision D6 and a prototype of Word's page background
come first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T115 (and decision D6) |
| Parallel-safe with | nothing: the only task of wave 36 |
| Files you own | the default page-number slot in `src/design/defaults/page.toml` and its bit in `APPLIED`; `src/html/design.rs` (`furniture_html`, `furniture_css`, `background_css`); `assets/themes/default/styles/print.css:1-2`; `lib.typ`'s `furniture-background`; `src/docx/furniture.rs`; `src/docx/design.rs` (header and footer offsets); the snapshots that change |

## Goal
Every format numbers pages the same way by default (D6), puts headers and footers at the same
offsets, and fills the page with `colors.background`. In Word the background is a page-sized
picture behind the text, through the existing background-image path, not `w:background`
(R13 §3.4 M8).

## Read first
- `docs/spec/design.md` §0.4, §9 (page furniture; §9.6 page background)
- `docs/research/R13-single-source-defaults.md` §2.8 (rows 5.20–5.22, 5.28, 5.31), §3.4 M8,
  §4.3 (row Page)

## API to implement
None beyond the built-in slot: the page number uses the slot keys of design.md §4.

## Steps
1. Add the default page-number slot and switch its bit on.
2. One header and footer offset rule in the three formats.
3. Word: the background colour as a 1 × 1 PNG stretched behind the text.
4. Remove the duplicated page-number rules from `print.css`.
5. Accept the snapshots after reading them.

## Acceptance criteria
1. Page-number baselines: Word (u) ↔ PDF within 0.3 pt; printed HTML within 0.5 pt (T102's
   harness).
2. A 12 mm `header_offset` gives the same page-2 baseline in Word (u) and the PDF within 0.3 pt.
3. `colors.background` fills the page in all three.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Dark mode (design.md §0.4, exception 2).
- LaTeX, unless the coordinator adds it at hand-off (T120 maps only T108's domains).

## Hints (verified)
- `print.css` at main: `@page` A4, `20mm 18mm`, a bottom-centre page counter in 9 pt
  `system-ui` (:1); none on the first page (:2).
- `lib.typ` at main: `furniture-background` (:342); without an offset, the header sits at
  `margin.top * 0.3` and the footer at `margin.bottom * 0.3`, in 0.75em when the slot has no
  size (:376-393).
- `src/docx/design.rs:442-443`: header and footer offsets default to 720 twips.
- `src/docx/furniture.rs` at main: `generate` (:42), `background_run` (:506), `with_anchor`
  (:581), `AnchoredDrawing` (:609-634; `relativeHeight="0"` and `behindDoc="1"`, relative to
  the page).
- `src/html/design.rs` at main: `furniture_html` (:400), `furniture_css` (:536),
  `background_css` (:654).

## Before hand-off
- D6 decided (is page 1 numbered).
- The coordinator prototypes the background picture and recomputes rows 5.20–5.22, 5.28 and
  5.31.
