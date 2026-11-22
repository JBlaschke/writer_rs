# T102 — Cross-format measurement harness

**Status: brief, not ready for hand-off.** The reference code, the expected tables and the
runner's language are the coordinator's to supply first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T101 |
| Parallel-safe with | T103, T104, T105, T107 (wave 26; no shared file) |
| Files you own | `scripts/xfmt/` (new: the DevTools-protocol layout and print readers, the Ghostscript baseline reader, a runner), `tests/xfmt/` (new: reference documents and their expected tables), `docs/dev/xfmt-harness.md` (new) |

## Goal
One command builds each reference document as DOCX, PDF and HTML with the release binary and
prints one table per document: for every pair of consecutive lines, the distance between their
baselines in the PDF, in Chrome's screen layout of the HTML (print media emulated) and in Chrome's
printed PDF of the HTML, Word's value from the model where the document gives one, and whether
each difference is within the tolerances of design.md §0.4 (R13 §0.2). It automates the commands
of R12 §9, so that every later M9 task states its acceptance as a harness result. It reads no
Word rendering: Word's figures come from the XML and from T103's record.

## Read first
- `docs/research/R13-single-source-defaults.md` §0.2 (measurement and tolerances) and §3.2
  (Chrome's rounding, the residual c)
- `docs/research/R12-spacing-model.md` §6 (the agreement table the harness reproduces), §8 and
  §9 (the commands it automates)
- `docs/spec/design.md` §0.3 and §0.4
- `src/pdf/devtools.rs`: `Page::launch` (:223), `navigate` (:279), `evaluate` (:332),
  `print_pdf` (:351) and `find_chrome` (:599), the crate's own DevTools client (feature
  `browser-tests`), for the protocol calls it makes

## API to implement
Set at hand-off. The contract is the printed table: one row per baseline gap, with the columns
Word (model), PDF, HTML screen, HTML screen + c, HTML print and verdict, and a summary line per
document with the number of gaps checked and the number outside tolerance.

## Steps
1. Start from the reference code the coordinator commits under `scripts/xfmt/` (Before
   hand-off) and run it once on R12's p3 build to see the expected tables.
2. PDF: read every text baseline with Ghostscript `txtwrite -r7200` (0.01 pt), as R12 §9 does;
   at `-r720` (0.1 pt) criteria 1 and 2 cannot be checked, because odd multiples of 0.75 pt fall
   on half units.
3. HTML screen layout: start one Chrome with a remote-debugging port and its own profile,
   `--user-data-dir="$(mktemp -d "${TMPDIR}xfmt-chrome-XXXX")"`, emulate print media
   (`Emulation.setEmulatedMedia`), set the viewport to the text width, load the page and read
   each line's client rectangles in fractional CSS px (R13 §0.2).
4. Printed HTML: `Page.printToPDF` in the same Chrome, then the PDF reader of step 2.
5. Close Chrome and remove its profile directory on every exit path, a failed run included.
6. Write `docs/dev/xfmt-harness.md`: what it needs installed, how to run it, the tolerances,
   how to add a reference document and its expected table.

## Acceptance criteria
1. On R12's p3 build it reproduces R12 §6's and §8's PDF and HTML columns within 0.05 pt, and
   R13 §3.2's "Plain CSS" row.
2. Every printed-HTML baseline it reports, measured from the top of its page, lies on the
   0.75 pt grid within 0.01 pt.
3. `git grep -n -e '--dump-dom' -e '--print-to-pdf' -e '--screenshot' -- scripts/xfmt` prints
   nothing, and after a run, a failed one included, `pgrep -fl xfmt-chrome` prints nothing.
   (`pgrep -fl remote-debugging-port` would also find Chrome processes the harness did not
   start.)
4. A full run over `tests/xfmt/` takes under two minutes.
5. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass (no Rust file changes).

## Out of scope
- Rendering in Word (T103), and fixing any difference the harness finds (the domain tasks).
- Running the harness in CI: a coordinator decision.
- Rust code: `src/pdf/devtools.rs` is read, not changed.

## Hints (verified)
- Chrome's one-shot `--dump-dom`, `--print-to-pdf` and `--screenshot` hang on the owner's
  machine (R13 §6.2, T102); a long-lived Chrome driven over the DevTools protocol does not.
- Printed HTML snaps every baseline to a whole CSS px, 0.75 pt: a 17.6 pt pitch prints as
  alternating 17.25 and 18.0 pt gaps, so print checks compare absolute positions within 0.5 pt
  or means over at least eight gaps within 0.1 pt, never a single gap (R13 §0.2).
- Chrome's screen layout puts a baseline round(ascent) + floor(half-leading) CSS px below the
  top of its line box; the floor was seen with a negative half-leading only (R13 §3.2).
- Tolerances (R13 §0.2): Word ↔ PDF 0.3 pt (0.5 pt where Word is read on a coarser grid); HTML
  screen ↔ PDF 0.3 pt after adding the two styles' c; printed HTML ↔ PDF as above; Word XML
  exact, to the twip.
- `scripts/` holds Python today (`scripts/waves.py` and the release scripts); R13's DevTools
  readers are Node scripts.

## Before hand-off
- The coordinator commits R13's probe tools as reference code under `scripts/xfmt/`: the
  DevTools readers (`cdp-eval.mjs`, `cdp-print.mjs`), the baseline reader (`baselines.py`), the
  font-metrics reader (`metrics.py`) and the probes *residual* and *center* (R13 §0.1).
  Before committing `baselines.py`, the coordinator switches it from `-r720` to `-r7200`.
- After T101 merges, the coordinator runs them on R12's p3 build, records the expected tables
  in `tests/xfmt/` and counts the result lines.
- Decide the runner's language and whether CI runs it.
- The ‡ rows of R13 were not re-measured on R12's build (R13 §6.4); this task's first run does
  it, and the coordinator corrects R13 from it.
