# T113 — Code, math, callouts and notebook cells

**Status: brief, not ready for hand-off.** T103's readings and a measured prototype of the code
paragraphs and the shared theme come first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | L |
| Depends on | T103, T112 |
| Parallel-safe with | nothing: the only task of wave 33 |
| Files you own | `src/design/defaults/code.toml`, `src/design/defaults/callout.toml` and `fonts.math`, with their bits in `APPLIED`; `assets/themes/default/styles/theme.css:178-250`; the default theme's `notebook.css`; `lib.typ`'s raw and listing rules; `assets/typst/writer-doc/callout.typ`; `assets/typst/writer-doc/cells.typ` (spacing only); `assets/docx/styles.xml` (`Source Code`, `Verbatim Char`, the token styles, `Callout`, the callout kinds and title, `Equation`, the cell styles, `Code Line Number`); `src/docx/body.rs` (code paragraphs and token styles); `assets/typst/writer-doc/writer.tmTheme` (new); `src/html/highlight.rs`; `src/typst/emit.rs` (`raw_block`'s language names: a new alias map); `src/html/math.rs`, `src/typst/render.rs` and `src/html/document.rs` (math fonts and sizes); `assets/docx/settings.xml` (`m:mathFont`); the snapshots that change |

## Goal
Code blocks, inline code, math, callouts and notebook cells take sizes, fonts, colours and
spacing from the built-in design in all three formats: one syntax theme for HTML, the PDF and
Word; one paragraph per code line in Word, so a highlighted line is a full-width bar; line
numbers that do not shrink a listing; one callout palette (T111's tokens); and a math font that
follows the new key `fonts.math` (R13 §3.4 M7) in the PDF, Word and HTML's `mathml` and `image`
modes.

## Read first
- `docs/spec/design.md` §0.4 (exceptions 5, KaTeX, and 16, syntax tokenisation)
- `docs/research/R13-single-source-defaults.md` §2.5 (inventory), §3.4 M7, §4.3 (rows Code,
  Callout, Math), §5
- T109's style keys `indent`, `border`, `fill` and `padding`, which code blocks and callouts use

## API to implement
Set at hand-off: the `[design.code]` and `[design.callout]` keys of R13 §4.2, and `fonts.math`.

## Steps
1. Add the defaults and switch their bits on.
2. Word: one paragraph per code line; line numbers and the highlight bar per paragraph.
3. One `.tmTheme` for the three formats at `assets/typst/writer-doc/writer.tmTheme`: the PDF
   reads it from the package (served by `embedded_dir`, `src/typst/package.rs:187-202`), HTML and
   Word through `vendor::get_str` (`src/vendor.rs` embeds `typst/**/*`). Add a new alias map in
   `raw_block` with the aliases of `src/html/highlight.rs:46-52`.
4. Math font and size per mode.
5. Remove the duplicated values from `theme.css`, `notebook.css`, the package and `styles.xml`.
6. Accept the snapshots after reading them.

## Acceptance criteria
1. R12 §8's code rows: Word (u) ↔ PDF within 0.3 pt, HTML within 0.3 pt after c (T102's
   harness).
2. Callout colours are equal in all three.
3. Line numbers no longer shrink a listing in the PDF; a highlighted line is a full-width bar in
   Word.
4. The math font follows `fonts.math` in the PDF, Word and HTML's `mathml` and `image` modes.
5. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- KaTeX's own fonts and the two syntax sets' grammars (design.md §0.4, exceptions 5 and 16).
- Tables (T114).
- LaTeX, unless the coordinator adds it at hand-off (T120 maps only T108's domains).

## Hints (verified)
- `src/docx/body.rs` at main: `SCOPE_TOKENS` (:25-63, entries :26-62) maps scopes to pandoc's
  token styles;
  `code_block` (:590) writes one `Source Code` or `Cell Output` paragraph for the whole block
  (:613-623); `code_lines` (:683) separates lines with `<w:br/>` (:689) and shades a highlighted
  line with `FFF3BF` (:693).
- `lib.typ` at main: `listing-raw` (:574), `listing-grid` (:610), `listing` (:645); the raw
  font in `layout` (:457).
- `src/html/highlight.rs:17-18`: `SYNTAXES` is two-face's `extra_newlines`; `resolve_syntax`
  (:36-57) holds the aliases at :46-52 (`console`/`shell` → `bash` and others). Typst's `RAW_SYNTAXES` is `extra_no_newlines`
  (typst-library 0.15.1 `src/text/raw.rs:1063-1064`). `src/typst/emit.rs` `raw_block`
  (:723-747) passes the language name through unmapped.
- `src/html/math.rs:143-146` scales an equation image by 12/11; `src/typst/render.rs:13-16`
  is the equation prelude and `equation` is :55-73; `src/html/document.rs:415-433` writes the
  MathML font stack.
- `assets/docx/settings.xml` names `Cambria Math` in `m:mathFont`.
- `styles.xml` at main: `Source Code` :45, `Verbatim Char` :49, tokens :55-85, `Callout` and
  kinds :86-91, `Callout Title` :92, `Equation` :93, `Cell Prompt` :95, `Cell Output` :96,
  `Code Line Number` :97.
- `cells.typ` has no colour literal at main.

## Before hand-off
- T103's code and equation readings in.
- The coordinator prototypes the per-line code paragraphs and the shared `.tmTheme` and
  measures. Only files under the folders `src/vendor.rs` includes are shipped, so the theme's
  path stays under `assets/typst/writer-doc/` unless the coordinator adds an include.
