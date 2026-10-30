# T61 — LaTeX templates, resources and source export

Implemented on `codex/latex-export` after T60 / PR #36. Focused runtime
validation passed on 2026-10-29. Final Clippy, denied-warning rustdoc and
formatting pass. Full acceptance passed on 2026-10-29: **1,800 debug /
1,799 release tests**, with three intentional ignores in each mode.

## Result

`latex::render` produces complete deterministic TeX, an ordered resource plan
and located diagnostics. It preserves the supplied Document, asset registry
and configuration. Rendering reads the selected custom template, or uses the
built-in sample; it does not read image/bibliography bytes, construct a World,
render an image, write output files or invoke a TeX engine. Public
`FileSource`, `PlannedFile`, `LatexOutput`, `render`, `materialise` and
`needs_world` follow the binding output contract.

The dedicated MiniJinja environment uses the TeX delimiters, disables HTML
autoescaping and preserves final newline bytes. Syntactic inspection requires
the actual `preamble` and `body` slots; a name appearing only in a comment, raw
literal, attribute or local binding does not satisfy the requirement. Missing
slots return empty TeX/resources and `latex-template-slot-missing` findings.
Unreadable, malformed or failing custom templates retain their own path and
error; they never silently fall back to the sample.

The rendering context keeps body/abstract/preamble raw, escapes ordinary
metadata and author fields, preserves typed raw `vars`, and uses planned
sidecar paths for bibliographies. The preamble follows actual usage, engine
and Unicode policy in deterministic order. Metadata/recursive vars, including
object keys, conservatively enable math support when their mapped characters
could require it. A TOC command is inserted into the body only when needed and
the template has no syntactic `toc` slot.

The sample and its exact specification remain synchronized. Citation-family
packages and biblatex resource registration require a configured bibliography;
an ordinary citation-free document does not load an unused bibliography
package. Bibliography printing retains its separate citation/placement gate.

`examples/latex-template/` supplies a compact citation-free document, a local
config, a font-free SVG, a sample-derived template and the latexmk command.
The customization adds a venue line via `vars.venue | tex`, a shared accent and
a highlighted-keyword hook after the generated preamble. The documented writer
export and PDFLaTeX latexmk commands both pass. Its PDF
retains the venue, figure/table/listing references, equation and code, with no
TeX errors, missing glyphs or unresolved-reference markers.

## Materialization and output safety

Raw PNG/JPEG/PDF assets and `.bib` files are copied byte-for-byte. Planned
SVG/GIF/WebP renditions use the caller's existing publishing-session World and
the actual image-to-PDF renderer. Missing World, renderer failure or an absent
PDF is an image Resource error attributed to the original filename hint;
there is no alternate-format fallback. Bibliography read errors retain the
original path and `bibliography` Resource category. Materialization creates no
World, writes no files and restores the supplied main after renditions.

The shared T60 namespace remains the planner's authority. For malformed public
plans, an exact duplicate basename coalesces only when its materialized bytes
are identical; conflicting bytes fail. Differently spelled ASCII-case aliases
fail even when their bytes match, since coalescing them would remove a promised
case-sensitive path. The ordinary shared planner already prevents these names.

The pipeline renders once, rejects target emission/render Error findings,
materializes every sidecar completely, then preflights the main file, every
sidecar and the populated asset directory before creating any parent directory
or writing bytes. Checks cover prepared inputs and actual World reads,
including consumed fonts, input/output aliases, output/output aliases,
non-regular destinations, dangling links and invalid parent ancestors. The
private shared export preflight preserves T34 behavior and messages.
The established platform path-identity limits are unchanged; Unix symlink and
hard-link aliases have direct controls.

Expected render, resource-read and preflight failures preserve all last-good
outputs and create no new parents. After those checks, sidecars are written
before the main file. This is not a filesystem transaction: an unexpected
write failure or a concurrent filesystem change can leave already written
sidecars or affect the file being written. No rollback guarantee is claimed.
Stale unrelated sidecars are retained. Successful builds return the main path
and a sidecar directory only when it is populated.

Pure source/copy builds do not invoke or reconfigure the supplied World.
`dump --latex` performs rendering only, so it can show useful TeX without
decoding image data or reading bibliography bytes. It writes the exact template
result to stdout, including empty output, a missing final LF or multiple final
LFs; only broken-pipe errors are ignored. Other dump formats retain their
existing output behavior.

## Findings, status and target independence

Target emission/render Error findings stop that target before materialization
or writes. Preparation findings remain on the aggregate report: a resolved-IR
Error can make the CLI exit nonzero while a target still writes useful output.
This distinction is deliberate and is exercised by the current T70 citation
placeholders. Successful sibling targets survive an independent LaTeX target
failure, and watch rebuilds continue after template failures. Existing initial
overwrite-clash/usage exit policy remains intact.

Private detailed render/build/dump paths retain findings collected before a
later template, rendition, copy or write error, without changing public result
types. The CLI reports an exact already-reported target Parse error once while
retaining its actual failure status. LaTeX dump likewise reports prior emission
findings before the original template error, with empty stdout on evaluation
failure.

Successful LaTeX directory status comes from that build's unique materialized
map, recorded after every sidecar and the main file succeed. Counts and bytes
describe files actually written, rather than a recursive inventory that would
include stale files. Repeated uses of the same image count once. A private
observer carries these records through one-shot and watch reporting without
changing BuildReport/WatchStatus or retaining history between builds. Other
targets keep their existing status behavior.

## Unicode diagnostic boundary

One `latex-unicode` summary merges the complete emitted-IR inventory first,
then unsupported surviving glyphs from executed `tex` filter evaluations, in
first-use order. Repeated code points are deduplicated before the display
limit; the original first IR location is preserved, otherwise the document
path is used with no invented span. The renderer uses structured inventory,
not a parsed or truncated previous diagnostic message.

Filter evaluation is the observable boundary, not eventual printing. A
captured or discarded expression that executes `tex` counts. Unused vars,
ordinary preescaped metadata, raw vars, template literals and raw TeX are not
blanket-scanned; they contribute only if actually passed through the filter.
Characters replaced wholly by supported ASCII do not become surviving-glyph
findings. Earlier filter observations and IR findings survive a later template
evaluation error. PDFLaTeX uses the warning level and other engines the
specified informational level.

## Measured validation

Focused acceptance complements the complete debug and release suites:

- Focused `latex::` filter: **123 passed**; loader-free rendering, slots,
  preamble/template behavior, resource plans/renditions, retained state,
  Unicode/macro order and detailed errors are covered.
- Public pipeline/status/dump suites: **18 passed** (16 pipeline cases,
  written statistics and detailed dump diagnostics). Coverage includes exact
  dump bytes, immutable input/plan state, actual image PDFs, copied bytes,
  failure preservation, output/input aliases, mixed target orders and native
  PDF authority beside the structured LaTeX gate.
- Cross regressions: **85 passed** (9 CLI Typst export, 25 LaTeX emission,
  37 Typst emission and 14 Typst export cases).
- Reconciled CLI/watch suites: **52 passed, 1 intentional private-driver
  ignore**: cli_build 15, cli_dump 10, cli_watch 10 and watch_loop 17. Former
  T61 stub checks now require positive export or permanent real template
  failures, preserving other-target output, later rebuilds and no partial TeX.
- Actual TeX protocols: **12 cases passed**, comprising **5 PDFLaTeX** and
  **7 XeLaTeX** cases. The two optional Biber variants were explicitly skipped
  because Biber was unavailable.

The five common engine cases are the sample biblatex tour, exact minimal
template, BYO template, plain paragraph and resource-complete saved notebook.
XeLaTeX also covers natbib/listings/cleveref and highlighted/tabular/hline/keep.
Each executed case builds twice for exact TeX identity, invokes real latexmk,
reads a real engine log and PDF, rejects TeX error lines and checks glyph
behavior. Ghostscript extraction must succeed with no `??`; tours retain
Table 1/Listing 1 and BYO retains Test venue. The notebook JSON stays original;
its positive scratch project supplies the existing Markdown plot and the
deliberate missing image with existing pixel bytes. Negative fixtures and
missing-asset policy remain unchanged.

Root-run evidence is retained in `/private/tmp/writer-t61-units-fixed.log`,
`writer-t61-public.log`, `writer-t61-cross-regressions.log`,
`writer-t61-cli-watch.log` and the successful `writer-t61-tex-gated.log`.
Per-case writer/repeat/latexmk/Ghostscript stdout, stderr and status, exact TeX,
engine log and PDF are under
`target/task-prep/crash-recovery/t61/tex/<case>-<engine>-<pid>/` (successful
12-case run: pid 37106). Earlier failed setup runs are not acceptance evidence.

## Final gates and milestone boundaries

| Gate | Final result |
|---|---|
| Full debug suite | **1,800 passed, 3 intentional ignores** |
| Full release suite | **1,799 passed, 3 intentional ignores** |
| All-target/all-feature denied-warning Clippy | **PASSED** |
| All-feature rustdoc with warnings denied | **PASSED** |
| Repository-configured formatting check | **PASSED** |
| New custom-template example writer/latexmk probe | **PASSED** |
| Build-time measurements / final acceptance statement | **PASSED — docs/dev/build-times.md** |

T70 citation loading/resolution remains pending. Original tour builds require
exactly the eight known source-derived unresolved citation errors (Doe once,
Lee once, Knuth six times), exit 1 and their escaped placeholders, while the
LaTeX target still writes useful TeX. Citation-free protocol cases require
exit 0; no generic 0-or-1 gate is used. These actual engine runs validate the
current templates, code, tables, figures, labels and assets, not positive
end-to-end citation resolution or a live bibliography backend.

T70 must refresh the tour goldens and strict positive CLI/engine citation gates.
The exact minimal template owns neither citation-family packages nor automatic
bibliography printing; its positive-citation case needs an explicit compatible
configuration without changing template bytes (or a separately frozen template
ownership clarification). The placeholder run is not proof of that future
case. The two unavailable Biber variants remain unverified.

Production source export never launches a TeX engine. Optional test protocols
own those external executions; T77's Chrome PDF work remains separate. Full
acceptance measurements are recorded in `docs/dev/build-times.md`.
