# T71 — target-aware document checks

Installed implementation and focused validation, 2026-10-29. The initial
compile/no-run gate passed in **1m34s**. Focused checks pass: **39 check-related
units**, **18 new public controls**, **12 inherited math cases**, and the
**25-case CLI/corpus rerun**. Four goldens were reviewed independently.
Final Clippy, denied-warning rustdoc and configured formatting pass. All
1,904 distinct debug cases and the complete 1,903-test release suite pass,
with three intentional ignores in each mode. Debug rerun details are below.

## Result and public boundary

`writer check` accepts every existing output target: HTML, PDF, DOCX, LaTeX
and Typst source, individually or in a comma-separated list. An explicit
`--target` overrides configured defaults; without it, the source kind selects
`build.targets` or `build.typst_targets`. Duplicate target names run once in
first-selected order. An empty target list still receives baseline source,
reference, math and requested presenter-compatibility checks.

The checker clones options, forces `Mode::Check`, and prepares once. It keeps
lenient reference resolution and the existing frontend/resolver diagnostics.
Source, IR, Config, public CheckOptions/run signatures, error/diagnostic
records and the CLI/clap surface remain unchanged. The former target stub is
removed; no new check-only input language or public visitor is introduced.

Check produces diagnostics without writing target artifacts. Errors exit 1;
warnings fail only with `--deny-warnings`, and informational findings never
make that flag fail. Argument grammar remains exit 2. Text, quiet behavior and
the complete flat JSON array retain the six existing fields: level, code,
message, file, line and cell. Quiet JSON does not discard findings.

T70's configured bibliography/style policy is preserved. Build and dump keep
their global fatal resource failures, including native-only PDF builds.
Check and serve keep the named bibliography/style Error finding and continue
without that library for repair. Selecting native PDF under check does not
apply build's suppression of structured-subset findings.

## Selected branches and actual target adapters

The private `src/check/visible.rs` walker visits what each output actually
interprets. It respects notebook `show_output` and `pick(target)`, rendered
table cells, figure/delegation context, flattened image alt/TOC/bare-link
content, repeated HTML notes, actual reference payloads and citation caches.
HTML/DOCX inspect the resolved citation text they render; Typst/LaTeX inspect
their selected source affixes and command branches. Hidden citation modes and
unrendered affixes do not request equation work or raw-loss findings.

DOCX Page-reference inspection uses its existing two-pass visible-bookmark
ownership, rather than treating every resolved label as a real bookmark.
DOCX/LaTeX table inspection reuses the emitters' selected-cell planners, so
clipped malformed cells are not independently reconstructed by check. Narrow
crate-private adapters expose existing table/bookmark/MIME decisions without
changing their public outputs or introducing another layout implementation.

`src/check/targets.rs` runs the selected adapters and continues after a target
failure. It collects findings already returned before a failure and adds the
located target error. It does not fabricate findings for later work that an
aborted writer never performed.

| Target | Actual check work and boundary |
|---|---|
| HTML | Resolve the winning theme and reuse its static problems. For selected native Typst equations or the configured image math route, use the real per-equation MathRenderer on a cloned registry, then discard representations/assets. Ordinary LaTeX KaTeX/MathML routes retain the existing baseline validation. No full HTML page, style output or site asset tree is built. |
| DOCX | Call the actual `docx::write` with WorldRenderer/reference options, collecting real raw-XML, styles, table/list, image and OMML/fallback findings. Discard the package bytes; no `.docx` is written. |
| PDF / Typst source | Reuse source visibility and a cheap shared mitex preflight for selected LaTeX equations, plus static image/bibliography findings. No full PDF/native document compilation, PDF template loading, image decoding for MIME classification or source-export resource copying is performed by these target passes. |
| LaTeX | Use `latex::render_detailed`, including the selected template and actual emitter/metadata/Unicode/macro/package findings. Discard TeX and resource plans. No materialization, image rendition, resource copy, World construction or TeX process is requested by this adapter. |

One check-local WorldSlot is initialized/reset lazily, only when actual HTML
native/image conversion or the required real DOCX adapter needs it. Both
adapters share that session. Failed initialization is attempted once and
reported once; later targets do not repeat compiler setup. Baseline, cheap
PDF/Typst and LaTeX-only checking do not initialize it merely for shared
plumbing.

This boundary does **not** mean that no compiler runs inside check. Real
HTML/DOCX equation or image conversion can use the embedded compiler. It
excludes a full document PDF build and external browser/TeX/notebook execution,
while retaining the actual conversion diagnostics required by those targets.
Preparation can still read source, configured bibliography and other real
input resources through its existing policy.

## Baseline math ownership and selected-target union

T33's baseline syntax, macro and tag checks retain source ownership. Borrowed
IR node identity distinguishes equations even when their text and spans are
equal. Notebook output candidates form the selected-target union by their
actual borrowed slices, not source-string equality. Target visibility then
assigns the actual PDF/Typst branch to each equation; those two outputs share
one cheap preflight, using the first selected backend's name in its message.

HTML and DOCX suppress only their repeated common math-core/source-preparation
codes: math-syntax, math-macro-invalid and math-tag-ignored. Their actual
conversion failures remain visible. LaTeX retains its distinct macro grammar,
metadata-only/evaluated-tex and native-Typst conversion findings instead of
discarding a same-named code indiscriminately.

There is no global diagnostic tuple/text/span deduplication. Equal source
nodes remain distinct, and source checks still run when target selection is
empty. Newly collected math/target findings undergo the existing centralized
strict escalation once, followed by stable root/include/cell/line ordering.
`--deny-warnings` changes exit status, not stored severities.

## Structural losses and recovery details

Selected foreign raw blocks and inlines produce raw-skipped Info per target.
The accepted raw language is html, openxml, latex or typst as appropriate.
An unknown raw format keeps its frontend Warning as well as the target Info.
Existing directive parsing/checking is reused; a second target regex scan does
not duplicate frontend findings or flag fenced literal examples.

HTML Page references report unsupported-construct while keeping the normal
reference output. PDF/Typst report unsupported or non-asset images and a
bibliography with no sources. Labelled bare multi-line LaTeX environments
produce math-env-label. A configured PDF template on native Typst produces
W-TYP-TEMPLATE-IGNORED without loading that ignored template or compiling the
native document.

Missing assets retain check's Warning policy. If DOCX encounters the exact
already-reported MissingAsset path/full span, the existing frontend Warning
stays once. If a registered file disappears after preparation, check adds a
new located missing-asset Warning. Other reference/template/Resource/I/O
errors retain Error severity. This narrow identity policy does not erase
distinct source/cell findings or soften unrelated target failures.

Two reviewed asset-classification corrections align check with real emission:

- Strip a registered asset URI's query/fragment before registry lookup, just
  as Typst emission does. Suffixed supported PNG stays quiet; suffixed
  unsupported MIME produces the same located warning as its unsuffixed form.
- A literal unknown `asset:<id>` now receives a located unsupported-construct
  Warning per selected backend, naming the exact written URI. It invents no
  filesystem path, missing-file event or successful fallback. Repeated target
  spelling is suppressed and central strict escalation changes only severity.

LaTeX template errors keep the actual known template filename/resource path
and its own line, instead of assigning a template line to the document. The
local adapter preserves generic code/message/span/severity and fallback for
unknown template names. Paths are absolutized lexically, without canonical
path rewriting or parsing the message. Earlier body findings survive a later
template error, other targets continue, and missing required slots stop that
target's content inspection before emission. Public Error/JSON shapes do not
change.

## Focused validation and golden review

Root-run evidence:

| Group | Recorded result |
|---|---|
| Installed compile/no-run gate | PASSED, 1m34s; writer-t71-typecheck.log |
| Check-related library/CLI units | 39 passed; writer-t71-check-units.log |
| New public target controls | 13 passed in check_targets; writer-t71-public.log |
| Template provenance controls | 3 passed in check_template_locations; writer-t71-public.log |
| Asset suffix/unknown placeholder controls | 2 passed in check_asset_suffix; writer-t71-public.log |
| Inherited source/math controls | 12 passed in check_math; writer-t71-public.log |
| Corrected CLI/corpus rerun | 25 passed: check_corpus 1, cli_check 17, cli_check_math 7; writer-t71-cli-corpus.log |

The public log also contains the initial obsolete CLI-stub/basic-raw expected
failures. Those sections are superseded by the corrected 25-case rerun, not
silently treated as successful evidence. Stub expectations became real target
checks; the basic example now expects its genuine raw-skipped Info rather
than an empty array. Info remains Info with quiet JSON and denied warnings.

New controls exercise every backend and union without project mutations,
configured/default/duplicate selection, empty-target baseline, hidden and
selected notebook branches, real HTML native conversion, real DOCX raw
validation, distinct LaTeX macro/metadata findings, target failure continuation,
missing-asset warning identity and one lazy World attempt. Invalid compiler
resources in cheap-target controls prove those paths do not need World setup.

Four diagnostic goldens add exactly **eight raw-skipped Info findings and one
HTML Page-reference Warning**. Source review confirms the raw directive at
line 27, foreign raw blocks at lines 10/14/18/22, notebook raw cells 6/7/8 at
cell line 1, and Page reference at line 10. Unknown-format Warning remains
before same-position Info. No source fixture was rewritten.

Root's parsed JSON subsequence audit proves every earlier finding remains
equal in value and order, including path/cell/line/severity. There is no
unrelated schema change or removed prior finding. The independent review is
recorded in `target/task-prep/crash-recovery/t71/reviews/root-goldens.md`;
snapshot generation by itself is not the acceptance criterion.

## Final gates — root fills after actual completion

| Gate | Result at this checkpoint |
|---|---|
| Final all-target/all-feature denied-warning Clippy | **PASSED** |
| All-feature rustdoc with warnings denied | **PASSED** |
| Debug acceptance | **1,904 cases passed, 3 intentional ignores; full run plus focused rerun** |
| Full release suite | **1,903 passed, 3 intentional ignores** |
| Repository-configured formatting check | **PASSED** |
| Final measurements/acceptance statement | **PASSED — docs/dev/build-times.md** |

This milestone does not scaffold/eject resources, change diagnostic UI/source
excerpt formatting, publish user docs/site/release artifacts, add external
rendering engines or execute notebooks. Those adjacent tasks retain their
separate ownership. Source/IR/config/public shapes and T70's real citation
resource boundary stay intact.


Debug acceptance uses the complete suite followed by a 52-case Typst frontend
rerun. The full run passed 1,903 cases and found one obsolete T50 expectation
that explicit HTML checking still exited with the T71 stub. That test now
requires exit 1, complete JSON and the real E-TYP-CALL-UNKNOWN Error, preserving
the default-target error check. All 52 frontend cases then pass. Production
code was unchanged after the complete debug run; no duplicate case is counted
in the 1,904-case acceptance total.
