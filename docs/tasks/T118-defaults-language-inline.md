# T118 — Language, hyphenation, widows and inline details

**Status: brief, not ready for hand-off.** T103's widow reading and probe p2 come first (see
"Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T103, T117 |
| Parallel-safe with | nothing: the only task of wave 38 |
| Files you own | the `hyphenate`, `widows`, `orphans`, `kerning` and `ligatures` keys (`src/design/`), their defaults in `src/design/defaults/text.toml` and their bits; direction from `lang` (`dir` in HTML, `text(dir:)` in Typst, `w:bidi` and `w:rtl` in Word); `assets/docx/settings.xml` (`autoHyphenation`); Word's `w:kern`, `w14:ligatures` and `w:widowControl`; the superscript and subscript model; highlight and overline; author email and id rendering; the snapshots that change |

## Goal
Language-driven and inline typography agree in all three formats: the text direction follows
`lang`, hyphenation is off by default and on with the key, widows and orphans, kerning and
ligatures have one setting each, and superscript, subscript, highlight and overline render at
the same sizes and in the same colours.

## Read first
- `docs/spec/design.md` §0.4
- `docs/research/R13-single-source-defaults.md` §2.9 (inventory), §4.2, §4.3 (row Inline)

## API to implement
Set at hand-off: `hyphenate`, `widows`, `orphans`, `kerning`, `ligatures` (R13 §4.2).

## Steps
1. Add the keys and defaults; switch their bits on.
2. Direction and hyphenation in the three formats; `autoHyphenation` in `settings.xml`.
3. Widows, orphans, kerning and ligatures in Word's styles.
4. Superscript, subscript, highlight, overline, and the author's email and id.
5. Accept the snapshots after reading them.

## Acceptance criteria
1. Probe p2's `ar` is right to left in all three.
2. German hyphenation is off by default in all three and on with the key (`hyphens: auto` in
   HTML).
3. Superscript and subscript sizes are equal in all three.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Each engine's own bidi details (design.md §0.4, exception 14); overline in Word, which has
  no run property for it (exception 8).
- LaTeX, unless the coordinator adds it at hand-off (T120 maps only T108's domains and
  `lang`).

## Hints (verified)
- `assets/docx/settings.xml` at main has no `w:autoHyphenation`; `src/docx/mod.rs:693` already
  lists it in the settings element order, and :581-598 list `w:widowControl`, `w:bidi`,
  `w:kern` and `w:rtl` in the property orders.
- `src/html/emit.rs:876` passes a `dir` attribute through.
- `theme.css:264`: `span.overline { text-decoration: overline; }`; print.css:7 sets
  `orphans: 3; widows: 3` for `p`.

## Wave 26's results (coordinator, 2026-11-22)

- T105 added the typography keys `text.hyphenate`, `widows`, `orphans`, `kerning` and
  `ligatures` to the schema and to layer 0 (`src/design/defaults/text.toml`), so step 1 only
  switches `TYPOGRAPHY` on.
- R13 gives no value for superscripts and subscripts; settle them with the owner before
  hand-off.

## Before hand-off
- T103's widow reading in.
- The coordinator names the files for direction, the inline model and the author's
  email and id: R13 names the keys and Word properties, not the files.
- The coordinator runs probe p2 (R13 §0.1: the draft critic's probes, `lang` de and ar).
