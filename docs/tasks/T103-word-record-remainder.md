# T103 — Word record, the remainder (run by the owner)

**Status: brief, not ready for hand-off.** The DOCX inputs, the predicted values and the reading
sheet are the coordinator's to prepare first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T101 |
| Parallel-safe with | T102, T104, T105, T107 (wave 26; no shared file) |
| Files you own | `docs/dev/docx-validation.md` (a new dated record after the existing ones), `docs/dev/word-record/` (new: the DOCX inputs and the readings) |

## Goal
Measure in Word for Mac what R12 §2 did not, so that the domain tasks compare against Word
itself and not only against R13's predictions (marked (u) in R13):
- where lines with `w:lineRule="atLeast"` place their extra height;
- the widow-control default of a paragraph without `w:widowControl`;
- s and A (design.md §7.4) of Calibri, Calibri Light, Consolas and Cambria Math;
- lists (tight and loose items, nested lists, the last item), block quotes, code (one paragraph
  per line), tables (the stroke, the 0.7 pt separator paragraph), footnotes, captions and
  equations.

## Read first
- `docs/research/R12-spacing-model.md` §2 (how Word was read, and the values already measured)
- `docs/research/R13-single-source-defaults.md` §0.2, §3.3 and §3.4 (the predictions)
- `docs/dev/docx-validation.md` from :168 (the 2026-11-04 record, the format to follow)

## Steps
1. Open each input of `docs/dev/word-record/` in Word for Mac and note the Word version and
   the zoom.
2. Read each item of the reading sheet with R12 §2's method and write the value next to R13's
   prediction.
3. Add the dated record to `docs/dev/docx-validation.md`: the inputs, the values, and every
   difference over 0.3 pt flagged with the task that uses it.

## Acceptance criteria
1. Every item of the sheet has a measured value, with the Word version and zoom recorded, next
   to R13's prediction.
2. Each difference over 0.3 pt names its task: T112 (lists, quotes, rules), T113 (code,
   equations), T114 (tables), T115 (footnotes), T110 (captions), T118 (widow control), T106 (s
   and A).
3. No file outside "Files you own" changes.

## Out of scope
- Any change to code, styles or the model: the record states what Word does.
- Word on Windows.

## Hints (verified)
- Calibri's A is computed from the font tables (about 0.952, (2500 − 550)/2048), not measured
  (R13 §6.4).
- Word's single line s, as writer computes it: (winAscent + winDescent + max(0, hhea lineGap −
  (winAscent + winDescent − hhea ascender − |hhea descender|))) / unitsPerEm
  (`src/docx/lines.rs:143-191`, the formula at :185; R13 §0.3). Calibri: 2500/2048 = 1.2207.
- The table separator paragraph R13 proposes is an exact 0.7 pt line (`w:line="14"
  w:lineRule="exact"`), the minimum `src/docx/furniture.rs:578-580` already relies on.

## Before hand-off
- After T101 merges, the coordinator builds the DOCX inputs with main's release binary, writes
  the predicted value for each reading, and prepares the reading sheet.
- T110 uses the caption readings but does not depend on this task; the coordinator checks they
  are in before T110's hand-off.
