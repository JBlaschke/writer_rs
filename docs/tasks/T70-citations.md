# T70 — citation libraries, rendering and resolver integration

Implemented on `codex/citations` after T61. Focused acceptance and static gates
passed on 2026-10-29, with 1,865 debug cases and 1,864 release tests verified.
Public APIs, dependencies, configuration and IR shapes are unchanged.

## Result

Configured bibliography keys now resolve to real citations and a rich shared
bibliography. `Library` loads BibLaTeX and Hayagriva YAML in caller order, with
case-insensitive extensions. It defaults to empty IEEE, selects archived aliases
or independent CSL files, and preserves the configured style spelling.
Unsupported formats, malformed files, unknown styles and dependent styles keep
their named Resource errors. There is no dependent-style network resolution.

Bibliography files remain absolute, lexically normalized, bibliography-only and
in configured occurrence order; symlink spellings are preserved. Repeated inputs
are parsed rather than silently deduplicated. Within-file duplicates stay parser
errors. Cross-file conflicts stop at the first conflicting file, reporting its
sorted unique duplicate keys at that file before processing later inputs.
Every style extension denotes a CSL path, including `.xml`; the shared path
predicate now gives such files the correct base, watch set and overwrite guard.
Extensionless archive aliases keep their existing behavior.

The pure rendering adapter uses one stateful Hayagriva driver for all requests,
one locale set and one bibliography finish. Custom locators retain their literal
strings; Normal/Prose/Year/Author/Full/None retain their distinct engine purposes.
Hidden cites contribute bibliography membership without visible text or notes.
Literal element-tree lowering preserves formatting and links, merges only equal
adjacent runs, and does not serialize through HTML or trim engine output.
No request means no bibliography; requested styles without a bibliography return
an explicit empty bibliography. Direct missing keys contribute no invented
placeholder while preserving one returned vector per request.

Resolver requests retain the existing abstract/body/depth-first and all-candidate
notebook order. Structural paths are assigned after classification and adjacent
merging, preserving subgroup identity without depending on moved inline addresses.
Finish resolves original descendants before copying prefixes/suffixes around
their cached subgroup. Generated caches never become fresh requests. Note styles
use the already allocated Normal-subgroup note numbers; other modes do not add
notes. Alternative candidates keep their own replay identity while later content
resumes the primary counter state. Label/key ambiguity and missing targets keep
their existing diagnostic and placeholder policy.

Locale overrides use the existing language parser and document fallback, with
one warning for an invalid override. Prose under note styles retains the engine's
literal output and the exact warning containing `Library.style_spec()`.

## Preparation and target boundaries

Preparation loads a local library after front matter, path normalization and CLI
precedence, then passes it through the existing resolver options. `Prepared`
does not retain a Library. Empty bibliography lists do not load an unused style.
The citation adapter creates no World, renderer, process or output artifact.

Configured bibliography/style failures are global: Build and resolved CLI dump
fail before target work, including native-only PDF requests. Serve/Check collect
the located error and resolve without the failed library, preserving native PDF
authority over separate structured-subset findings. Reloading repaired resources
recovers normally. Global resource findings survive native preview filtering.

CLI dumps now prepare and therefore load configured bibliography bytes. A target
dump using an already prepared document remains a render-only operation: it does
not materialize bibliography/image files or reread that library. Tests distinguish
these layers by preparing valid resources before removing them for copy failures.

HTML/DOCX consume actual rendered citation/note trees; Typst and LaTeX retain real
citation calls. Resource tests now use real source citations instead of fabricated
resolved bibliography state, while deliberate no-library/native fragment controls
and isolated copy-planner tests retain their own contracts.

## Goldens and measured validation

Focused acceptance passes:

- **23 loader/driver units**, **25 tests matching `resolve::`**, and **29 public
  citation/resolver/pipeline cases**. These cover formats/styles/duplicates,
  rich formatting and modes, affixes/locales/notes/candidates, target orders,
  real Typst text parity, editable DOCX notes, resource safety and failure recovery.
- **17 strict Markdown–Typst roundtrip pairs and 8 comparison controls**.
  Canonical views preserve subgroup source facts, cache presence and note flags;
  rendered views preserve full existing cache trees, formatting, order and note
  numbers before canonical clearing. No subgroup rerender or new exclusion was
  added. An independent baseline audit proves the complete canonical JSON and
  rendered projection unchanged for all **14 noncitation previous snapshots**.
- Independent Markdown/HTML/Typst/roundtrip and LaTeX golden reviews found no
  unrelated source rewriting or machine-path leakage. The two new parse-only
  fixtures remain unresolved. Existing references retain duplicate/reserved-label
  negatives while replacing only old citation placeholders with real output.
  All nine LaTeX tour bodies match the previous bytes outside their citation
  paragraph; usage/resource plans are unchanged. The full SAMPLE changes only
  citation commands and bibliography printing. These static reviews are
  separate from the actual root-run engine evidence below.
- **12 real positive TeX cases passed**: 5 PDFLaTeX and 7 XeLaTeX, each with
  writer exit 0, repeated identical TeX, real engine logs/PDFs and Ghostscript
  checks. All **8 citation-bearing cases ran actual BibTeX**, retaining `.bbl`
  entries and `.blg` backend evidence. Two optional Biber variants were explicitly
  skipped because Biber was unavailable; they remain unverified.

Exact minimal/BYO templates are unchanged; their positive citation cases select
plain citations plus body bibliography printing. SAMPLE families keep their own
commands. Protocols reject undefined citations and retain glyph, Table 1/Listing 1,
BYO venue and resource-complete original-notebook checks. Production never invokes
TeX. Per-case evidence is under `target/task-prep/crash-recovery/t70/tex/`.

Initial harness failures were corrected as test assumptions: negative shorthand
`-@key` is Year, not hidden None, and resolved reference fixtures no longer have
old unresolved-citation findings. Original fixture/template bytes were preserved;
explicit hidden modes remain covered independently.

## Final gates

| Gate | Result |
|---|---|
| All-target/all-feature typecheck | **PASSED** |
| All-target/all-feature denied-warning Clippy | **PASSED** |
| All-feature rustdoc with warnings denied | **PASSED** |
| Repository-configured formatting | **PASSED** |
| Debug acceptance | **1,865 cases passed, 3 intentional ignores; full run plus focused rerun** |
| Full release suite | **1,864 passed, 3 intentional ignores** |
| Build-time measurements/final acceptance totals | **PASSED — docs/dev/build-times.md** |

T71 target-aware checking, T72 scaffolding/eject and T73 diagnostics work remain
separate milestones. No new frontend, generic visitor, external runtime or public
schema was introduced by T70.


Debug acceptance used the complete suite, followed by a nine-case CLI rerun.
The first run had 1,864 passes and one test expectation failure: a directory
where the Typst main file should go is correctly a usage error (exit 2).
The test now checks that exact status and still requires successful HTML/PDF
siblings and an unchanged sentinel. Its separate missing-bibliography case
requires global exit 1 with no output tree. All nine cases then passed; no
production code changed after the full debug run.

The two new empty Markdown diagnostic snapshots retain Insta's canonical final
blank line. Whitespace verification passes with only `blank-at-eof` disabled;
all other generated output and source whitespace checks remain enabled.
