# T115 — Footnotes and bibliography

**Status: brief, not ready for hand-off.** A prototype of the PDF bibliography as content, with
its links and the export kept, comes first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | L |
| Depends on | T103, T114 |
| Parallel-safe with | nothing: the only task of wave 35 |
| Files you own | `src/design/defaults/footnotes.toml` and the `Footnote Text` and `Bibliography` styles of `src/design/defaults/styles.toml`, with their bits in `APPLIED`; `assets/themes/default/styles/theme.css:277-291`; `lib.typ`'s `footnote.entry` rules; `assets/typst/writer-doc/design.typ` (`spacing-model`'s note size: the literal 0.85 becomes the design's footnote size); `src/typst/emit.rs` (`bibliography` and `citation`, whose `#cite(<key>)` needs a target once the bibliography is content); `src/cite/render.rs` only if the IR needs a label; `assets/docx/styles.xml` (`Bibliography`, `Footnote Text`, `Footnote Block Text`, `Footnote Reference`); `src/docx/mod.rs` (the `footnotes.xml` separator); the snapshots that change |

## Goal
Footnotes and the bibliography look alike in all three formats: note size, line pitch, gap and
separator follow R13 §3.4 M12 (HTML's endnotes stay exception 1), and the bibliography's label
column and hanging indent come from the built-in `Bibliography` style. In the PDF the
bibliography is writer's own formatted list, as in HTML and Word, in place of Typst's
`#bibliography(..)`.

## Read first
- `docs/spec/design.md` §0.4 (exception 1)
- `docs/spec/typst-dialect.md` §10 (exports and eject)
- `docs/research/R13-single-source-defaults.md` §2.7 (inventory), §3.4 M12, §4.3 (rows
  Footnotes, Bibliography)

## API to implement
Set at hand-off: the `[design.footnotes]` keys of R13 §4.2.

## Steps
1. Add the defaults and switch their bits on.
2. Typst: the `footnote.entry` size (with the line-box edges) and separator, and the design's
   gap and clearance in place of R12's `note-gap` (design.typ `spacing-model`, whose 0.85 note
   size becomes the design's); R12's `set par(leading: 0pt)` stays; the bibliography as content
   with its links.
3. Word: the separator paragraph's border in `footnotes.xml`; the styles' values.
4. Remove the duplicated values from `theme.css`.
5. Accept the snapshots after reading them.

## Acceptance criteria
1. Note size, pitch and gap: Word (u) ↔ PDF within 0.3 pt (T102's harness).
2. The bibliography's label column and hanging indent are equal in all three.
3. Citation links still reach their entries in the PDF.
4. `#cite(<key>)` and `writer export --to typst` still compile, or the prototype records the
   change in `docs/spec/typst-dialect.md` §10 at hand-off.
5. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- HTML's endnotes (exception 1); CSL styles other than the default.
- LaTeX, unless the coordinator adds it at hand-off (T120 maps only T108's domains).

## Hints (verified)
- Typst's footnote entry has a built-in show-set of `leading: 0.5em` and `size: 0.85em` that
  document-level set rules cannot reach (typst-library 0.15.1 `src/model/footnote.rs:324-331`).
  R12's 0.5.0 adds `show footnote.entry: set par(leading: 0pt)` and
  `set footnote.entry(gap: ..)` in `lib.typ`'s `paragraph-layout` (spacing-model).
- Typst's bibliography is a grid with a 0.65em column gutter (typst-layout 0.15.1
  `src/rules.rs:453`, layout :465-494).
- R12's `spacing-model` (design.typ, spacing-model) computes the note gap with Typst's 0.85
  note size: `0.85 * (height - cap)`, or `height - 0.85 * cap` for an exact pitch.
- `src/typst/emit.rs` at main: `bibliography` (:952-977) writes `#bibliography(..)`;
  `citation` (:1324) writes `#cite(<key>)` (:1366), which needs it.
- `src/cite/render.rs` has no Typst output at main: `render` (:35) returns the citations and the
  bibliography as IR (`CiteOutput`, :27-32).
- `src/docx/mod.rs:409` writes `footnotes.xml` with a bare separator and continuation separator.
- `styles.xml` at main: `Bibliography` :13, `Footnote Text` :29, `Footnote Block Text` :30,
  `Footnote Reference` :51.
- `theme.css` at main: footnotes :277-280, references and CSL entries :281-291.

## Before hand-off
- The coordinator prototypes the PDF bibliography as content, with links, checks `#cite` and the
  Typst export, and measures the footnote pitch.
- T103's footnote readings in.
