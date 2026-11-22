# T106 — Bundled line metrics and a missing-family warning

**Status: brief, not ready for hand-off.** The metrics table and probe (h) are the coordinator's
to prepare first, and the font families depend on decision D7 (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T103 (Calibri's A), T105 |
| Parallel-safe with | nothing: the only task of wave 27 |
| Files you own | `src/docx/lines.rs` (with its unit tests), `src/design/metrics.toml` (new), `src/design/metrics.rs` (new; one additive `mod metrics;` line in T105's `src/design/mod.rs`), `src/check/design.rs` (the missing-metrics warning, design.md §10), `tests/design_metrics.rs` (new) |

## Goal
Under §0.3's model both Word's `w:line` and the PDF's line box depend on a family's single line
s and its part above the baseline A. writer reads them from font files today, and a family it
cannot read silently takes Calibri's single line (`DEFAULT_SINGLE_LINE`,
`src/docx/lines.rs:19-22`), so the same document gives a different DOCX on a machine without the
fonts. This task ships s and A for the default and common families in `src/design/metrics.toml`,
with the hhea and OS/2 typo and win ascent and descent that Chrome uses, from which T119
computes HTML's c.
`src/docx/lines.rs` and the `lines` input of the PDF (design.md §3.3, R12's T96) take them from
the table before reading any font file, and writer warns once per family that neither the table
nor a file resolves.

## Read first
- `docs/spec/design.md` §0.3, §3.3 (the `lines` key) and §7.4
- `docs/research/R12-spacing-model.md` §2.1 (measured s and A per family)
- `docs/research/R13-single-source-defaults.md` §0.3 (the formula for s), §3.4 M3 and M7, §4.1
  ("Metrics")
- `src/docx/lines.rs` at main and after R12's T96 (`LineMetrics`, `WordLines::load_declared`,
  `to_json`, `line_metrics`)

## API to implement
Set at hand-off. The table maps a lowercase family name to s and A in em, and to the hhea,
typo and win ascent and descent in font units with unitsPerEm; `src/design/metrics.rs` reads it
once. `WordLines::load(design: &Design, font_paths: &[PathBuf], system_fonts: bool) ->
Self` (`src/docx/lines.rs:39-41` at main) and R12's `WordLines::load_declared` keep their
signatures.

## Steps
1. Add the coordinator's table and its reader.
2. Look a family up in the table before reading font files, in `WordLines` and in the `lines`
   input.
3. Replace the silent fallback with one warning per unresolved family, emitted from
   `src/check/design.rs`; the fallback value stays (R12 allowlists it, T121).
4. Tests: the table against R12 §2.1, the warning count, probe (h).

## Acceptance criteria
1. Probe (h) writes a byte-identical DOCX with and without the design's fonts installed (`w:line`
   296, not 275).
2. The table's s and A for the families of R12 §2.1 equal R12's values within 0.001, and every
   row has the six ascent and descent columns.
3. A design naming two unknown families, each used twice, gives exactly two warnings.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Choosing or shipping font files (decision D7; M7 Option A).
- Switching any default on (T108).

## Hints (verified)
- `src/docx/lines.rs` at main: `DEFAULT_SINGLE_LINE` (:22, Calibri's 2500/2048 em, doc comment
  :19-21), `WordLines::load` (:39; it reads font files only for a declared design, :46),
  `single_line` (:146), `sfnt_single_line` (:152; the formula at :185).
- s = (winAscent + winDescent + max(0, hhea lineGap − (winAscent + winDescent − hhea ascender −
  |hhea descender|))) / unitsPerEm (R13 §0.3). Source Sans 3: (1222 + 202)/1000 = 1.424, A =
  1.024; Georgia: 2327/2048 = 1.136; Calibri: 2500/2048 = 1.2207, A about 0.952 ((2500 − 550)/2048,
  computed, R13 §6.4); Libertinus Serif: s 1.140, A 0.894 (R12 §2.1).
- Word's `w:line` for a multiple m is round(240 · m / s) (design.md §7.4; R13 §3.1 M3).

## Before hand-off
- The coordinator computes s and A with R13's `metrics.py` for the default and common families
  (the D7 candidates, Calibri, Calibri Light, Consolas, Cambria Math, Libertinus Serif, Source
  Sans 3, Georgia), checks them against R12 §2.1, and prepares probe (h).
- Choose the warning's code with design.md §10's other design codes.
- T103 measures Calibri's A in Word; if it differs from the computed value by more than 0.3 pt
  at 11 pt, the table takes Word's.
