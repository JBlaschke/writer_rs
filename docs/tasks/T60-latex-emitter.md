# T60 — IR to LaTeX body emission

Implemented on `codex/latex-emitter` from T53 / PR #35 (`377e6ec`). Full
acceptance passed on 2026-10-29. Dependencies are implemented.

## Result

`latex::emit::emit` produces deterministic body/abstract text, usage records,
ordered image and bibliography plans, and located diagnostics. Its provided
IR, asset registry and effective configuration remain unchanged. Emission has
no file reads, World, renderer, process execution or citation-engine lookup.
The six escaping functions and exact Emitted/Usage/AssetFile APIs follow the
LaTeX output contract.

Contexts preserve outer floats, boxed tables/figures and footnote-safe code.
Headings and TOCs use the configured section/chapter/part offset; numbering
selects commands without printing IR numbers. Logical labels are anchored once.
Moving arguments preserve full-first resource/Unicode order, protect notes and
use note-free short forms without duplicate findings. Page references precede
the Normal unnumbered fallback, and heading labels use §4.4's specific form.

Tables place each section on an occupied grid, retaining rowspan content in
its first row and later uncovered cells. Widths, alignment, multicolumns,
optional-row-break guards, longtable heads/feet/counters and boxed tabular
contexts are covered. Empty-column tables keep caption, labels and cell text
as paragraphs with a finding instead of emitting an invalid preamble.

Code uses verbatim, listings or stateful shared-syntax highlighting. The fixed
12-class InspiredGithub palette is checked against the pinned theme; whitespace
and newlines stay outside highlight macros. Verbatim/listings terminators
produce errors with an escaped useful fallback. Notebook outputs retain their
verbatim policy, and hidden/unpicked candidates contribute no usage or files.

LaTeX math stays exact, including sources other backends reject. Native Typst
math uses the existing pure converter, retaining its findings. Package and
Unicode records keep first-use order; math mappings override earlier verbatim
mappings without moving their position. Prose uses the exact 134-entry mapping
and 349-code-point PDFLaTeX-safe set, distinct from math's larger table. Unicode
summaries, macro validation and code/prose/URL escaping have direct controls.
Raw LaTeX is preserved and foreign raw content is omitted.

## Shared resource names

A pure crate-private planner reserves existing Typst image/link selections
first, preserving their basenames. It then reserves LaTeX-only selected image
bases, MIME/rendition names and bibliography names. Actual materialization
still follows each target's selected uses. T34 resources claim their own
written-path reservation or retain the established content-hash suffix policy;
new LaTeX reservations change an auxiliary name only on a real collision.
Names remain portable across ASCII-case-insensitive and Windows filesystems.
The fixed priority is documented in both output specs.

## Validation and boundaries

Focused checks pass:

- All 25 public cases: every top-level Markdown fixture, all code/cite tour
  variants, notebook prompt variants, references, notes, contexts, tables,
  assets and the 37 LaTeX / 36 Typst math cases.
- Private contract tests cover escaping/table boundaries, syntax state,
  palette, Unicode and package order, moving arguments, visible selection,
  diagnostics and source attribution.
- All 60 existing Typst emitter/export/CLI tests and six namespace units pass.
- Denied-warning Clippy across all targets/features, rustdoc and configured
  formatting pass.

All 31 goldens received independent semantic review. Only typed path fields
and two exact diagnostic message path formats are stabilized; component-boundary
written/canonical roots include Windows extended paths. Literal body/code/math
content remains exact. The three regenerated messages changed only their known
fixture-root path fields, verified separately.

T70's citation library remains pending: real-resolve tour citations honestly
retain placeholders, while handbuilt Cite IR exercises every positive command
mode. T61 owns full templates, preambles, resource copying/renditions, positive
CLI output and actual TeX-engine validation. No full .tex pipeline or external
engine run is claimed by this milestone.

Full acceptance passed: **1,741 debug / 1,740 release tests**, with three
intentional ignores in each mode. Measurements are recorded in
`docs/dev/build-times.md`. T61 is staged independently for the next branch.
