# T120 — LaTeX consumes the design

**Status: brief, not ready for hand-off.** A map from each template variable to a design key
comes first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T108 |
| Parallel-safe with | T109 (wave 29; no shared file) |
| Files you own | `assets/latex/writer-template.tex`; `src/latex/` except `src/latex/emit/figure.rs` (the image width, T110); `src/pipeline/latex.rs` (the `render_detailed` calls at :34 and :104 get the design); `src/check/targets.rs` (the `render_detailed` call, :97 at main; after T105); `tests/latex_pipeline.rs` (the `render` calls); the `latex_*` snapshots that change |

## Goal
The LaTeX output reads the resolved design for T108's domains (text, fonts, page, colours) and
for language and metadata, so margins, sizes and colours match the PDF's and the template no
longer hard-codes them. Today LaTeX never sees the design: the pipeline renders it without one.

## Read first
- `docs/spec/design.md` §0.4
- `docs/spec/latex-output.md`
- `docs/research/R13-single-source-defaults.md` §4.3 (the LaTeX column), §6.2 (T120)

## API to implement
Set at hand-off: how the resolved design reaches `src/latex/` (`Prepared.design` exists, at
`src/pipeline/mod.rs:80`).

## Steps
1. Pass the resolved design from `src/pipeline/latex.rs` into the LaTeX renderer.
2. Map each template variable to a design key: class options, `geometry`, fonts, the callout
   colours, `hypersetup`.
3. Caption labels from `lang`; `hypersetup` with author, keywords and subject.
4. Accept the snapshots after reading them.

## Acceptance criteria
1. For `tests/xfmt/<case>.md` built with and without its `[design]`, the two `.tex` files differ
   in the template values of margins, sizes and colours.
2. The template values for margins, sizes and colours equal the resolved design's (a unit test
   on `render_context`); no TeX engine is needed.
3. Caption labels follow `lang`; `hypersetup` carries the author, keywords and subject.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Headings, captions, lists, code, tables and notes in LaTeX: the domain tasks T109–T118 own
  them in the other formats, and R13 assigns them to no LaTeX task (the coordinator decides).
- The image width (T110).
- Sub-figure references: not part of the design.

## Hints (verified)
- `writer-template.tex` at main: `\documentclass[11pt,a4paper]{article}` (:3),
  `geometry` with `margin=2.5cm` (:4), `parskip` (:5), `lmodern` or `fontspec` (:10, :12), the
  callout `\colorlet`s (:17-18), `\hypersetup` with `linkcolor=blue!60!black`, `pdftitle` and
  `pdflang` only (:40), biblatex `authoryear` (:42).
- `src/latex/mod.rs` at main: `render` (:64), `render_detailed` (:85), `render_context` (:264).
- `src/pipeline/latex.rs` calls `render_detailed` at :34 and :104 without a design.
- `Prepared.design` is set at `src/pipeline/mod.rs:254-256`.
- `src/latex/` at main: `assets.rs`, `emit.rs`, `emit/{block,code,figure,highlight,inline,math,
  output,table}.rs`, `escape.rs`, `mod.rs`, `preamble.rs`, `template.rs`.

## Before hand-off
- The coordinator maps each template variable to a design key.
- The coordinator decides whether LaTeX's headings and the later domains get tasks of their
  own.
