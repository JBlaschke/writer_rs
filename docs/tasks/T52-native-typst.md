# T52 — Native Typst builds, math routes and Markdown round trips

Implemented on the T52 feature branch, 2026-10-29. Focused validation is recorded
below; final full-suite results and timings are added after the release gates.

| | |
|---|---|
| Milestone / size | M5 / M |
| Dependencies | T1A, T23, T24, T32, T42, T50, T51; implemented |
| Native builder | `src/pipeline/typst_source.rs` |
| Shared integration | `src/pipeline/mod.rs`, `src/cli/build.rs`, `src/typst/world.rs`, existing serve session |
| Main tests | `tests/typst_source.rs`, `tests/typst_math_pipeline.rs`, `tests/typst_roundtrip.rs` |
| Preview tests | Native additions to `tests/cli_serve.rs` and `tests/browser_serve.rs` |
| Example | `examples/typst/` |
| Binding sources | PLAN T52; typst-dialect.md §§2.1, 7–9; math.md §§2, 3.5, 4.5, 5–6, 9–10; cli.md build/serve; live-reload.md §5 |

## Result and release boundaries

A `.typ` build defaults to PDF compiled from the user's original file. HTML and
DOCX use the structured IR, including native Typst MathML and the specified
per-equation recovery. Subset errors belong to those IR targets and cannot
prevent a valid native PDF. Serve defaults to typeset pages for Typst input;
explicit `--view web` and the view toggle use the structured web view. Each
view rebuilds independently and retains its last successful result on failure.

The Markdown emitter/frontend round trip now compares all 15 required fixtures
under the exact documented losses. There are no new public APIs, CLI flags,
config fields, IR fields or dependencies. The existing World, render helpers,
output safety and diagnostic types remain the shared implementation seams.

Positive whole-document LaTeX output remains T60/T61's work; T52 tests the
existing subset refusal for that target. T53 owns native `.md`/`.ipynb` includes
and `include-notebook`. T71 owns the remaining per-target check lints, and T78
owns arbitrary Typst HTML export. T34 source export remains independent and
rejects an input already in Typst. Native documents apply templates in their
own source: a configured `[pdf] template` is ignored with a located
`W-TYP-TEMPLATE-IGNORED`, including when native compilation fails.

## Native PDF and diagnostics

The existing public builder signature is unchanged:

```rust
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)>;
```

A crate-private `build_detailed` retains every compiler finding on failure.
`pipeline::build_target` dispatches native Typst PDF before the Chrome engine
arm, so selecting Chrome does not change how native source is compiled.
`WorldSlot` supplies a retained `MainFile::Path` session; this builder never
loads regenerated IR, replaces the main with a virtual source, or creates a
second World. The existing HTML/DOCX math helpers restore the original main
before subsequent native compilation, in either requested target order.

The builder uses `compile_pdf` with `world.session_timestamp()` and
`ident: None`. The full timestamp is retained for the session; `World::today`
continues to return its date. Timestamp selection remains configured RFC3339
normalized to UTC, then a literal ISO metadata date at midnight UTC, then
`SOURCE_DATE_EPOCH`, then the current whole second. Fixed timestamps and
embedded-only fonts produce identical fresh and retained PDF bytes. A native
source's own document-date setting retains Typst's existing override behavior.

`compiler_result` preserves all compiler findings, hints and native provenance,
choosing `span.file`, then the compiler path, then the original input as the
location. The first error is still the ordinary `Error::Compile`; its companion
diagnostics keep the remaining errors. Compilation and output protection finish
before output directories or PDF bytes are written. A compiler failure leaves
an existing successful PDF intact.

Target-aware reporting is private to the existing build pipeline:

- A request containing only native PDF still prepares metadata and retains all
  findings on `Prepared`, but excludes IR findings from the final report.
  Native compilation supplies the PDF's actual errors and warnings.
- A mixed native/IR request retains all preparation findings. Every missing
  asset is raised to error in build mode; each IR target applies the existing
  missing-asset and subset gates while PDF reaches the native compiler.
- Actual source, config, I/O and World-construction `Result` failures remain
  failures. Filtering diagnostics does not turn failed preparation into success.
- The non-native early missing-asset failure path stays unchanged. Final strict
  escalation remains centralized and does not retrospectively remove outputs
  already written successfully.

`src/cli/build.rs::target_error_reported` avoids printing a detailed native
compiler error twice. It also suppresses a missing-asset target wrapper only
when an error-level finding already reports the same path and span. Build and
watch therefore retain every distinct finding while mixed HTML/DOCX failures
can share one already-reported missing resource. Successful targets still print
their paths and survive another target's failure.

## Consumed-input protection and fonts

The ordinary pre-target guard still checks `input_files(prepared)`. Native
compilation can read dynamic includes or assets that the subset cannot record,
such as `#include ("part" + ".typ")`. Before writing, the native builder checks
the output against prepared inputs plus `world.accessed()` using the existing
`find_clash` identity rules. Existing prepared roles are preserved; newly
consumed source files are Includes and other files are Assets. Direct paths,
missing-parent/`..` aliases, and Unix symbolic and hard links are covered.
Non-Unix hard-link identity remains the existing documented limitation.

`World::font` records the selected disk `FontPath` before loading, including
failed loads and already cached faces. Embedded and unselected faces do not
become consumed inputs. This closes the selected-font overwrite gap without
changing the font loader. The pinned `FontStore` caches discovery and decoded
fonts, including failed loads, across `World::reset`; retained watch/serve
sessions must restart after font-file changes. T52 does not claim discovery or
font-content refresh.

No new font fixture, font conversion, package installation or dependency was
introduced. The public integration test copies an embedded face and explicitly
selects its disk slot through `World::font` before native compilation. That
proves the consumed-file guard using the public World API. Identical embedded
families win ordinary compiler selection, so this test does not claim a custom
font was selected by layout. Private units separately check selected, cached,
failed and unselected path tracking.

## Native math routes

Native Typst equations stay in their original dialect. HTML/DOCX do not route
them through T51's Typst-to-LaTeX converter; code within math remains verbatim.
The integration corpus uses the unchanged 36 Typst fixtures and their existing
`expectations.toml` DOCX classifications: 32 editable equations and four native
MathML failures (`21`, `29`, `30`, `34`).

| Target/config | Implemented path and evidence |
|---|---|
| HTML katex or mathml | Native `equation_mathml`; one native CSS/font rule, no KaTeX assets for Typst-only content |
| HTML MathML failure | Native picture, then original source with one final located diagnostic if rendition also fails |
| HTML image | Native SVG through the existing standalone, site and serve asset modes; inline baseline scaling retained |
| DOCX omml | Native MathML through T42 OMML, with editable structures and styled-Unicode folding |
| DOCX MathML/OMML failure | Inline Verbatim Char or the specified fallback text; display picture, then Source Code if needed |
| DOCX image | Native SVG plus PNG fallback, with T42 sizes and inline positioning |
| Native PDF | Original source and native compiler diagnostics, with no regenerated equation fallback |

Tests check exact corpus routes, CSS/font insertion, located once-only
warnings, display-picture/source recovery, inline sizes and baselines, SVG/PNG
package parts, and separate configured LaTeX macro behavior. Source-level
`#text[kept code]` stays valid native math. The same retained session restores
its real main across math exports and later PDF builds in both target orders.

## Round-trip contract and refinements

`tests/typst_roundtrip.rs` enumerates only direct `tests/fixtures/md/*.md`
children: 18 fixtures minus exactly `presenter-deck.md`, `includes.md` and
`raw.md`, leaving 15, including `include-chain.md`. Each side merges its
metadata into its own config and resolves with `library: None, lenient: true`.
Emission uses `AssetPaths::Source`, an empty footnote prefix and
`fragment: false`. The generated Typst source uses a synthetic identity in the
same base directory.

The Typst reparse receives the effective config used for emission, then clones
that config before its independent metadata merge and resolution. This is
required by the existing exact configured macro-prefix rule: generated source
does not serialize arbitrary config into `writer-doc.with(...)`. Only the
frontend's exact leading configured block is removed. Neither the normalizer
nor a fixture strips arbitrary macro definitions or changes math source bytes.

One typed normalizer compares `meta`, `blocks`, `features` and `resolved`, with
only typst-dialect.md §7's permitted losses. Retained attribute flag order and
duplicates remain visible. Two narrow coordinator refinements are documented
in that section:

- Canonicalize only `features.notebook` to surviving `Block::Cell` presence.
  An included notebook containing only Markdown cells otherwise leaves a
  provenance bit which flattened Typst cannot express. All other feature bits
  stay strict; losing a real Cell still fails equality.
- Re-read `CodeAttrs` controls only for `CodeBlock`. Inline `Code` uses ordinary
  class/pair clearing while retaining its exact text and id, because line
  numbering and highlighting apply to blocks.

The emitter now places inline footnote labels directly after the footnote,
avoiding generated whitespace before adjacent punctuation. Named and anonymous
notes preserve actual following spaces and punctuation; native author-supplied
whitespace stays content. Focused regressions pin that behavior. No general
whitespace loss, source trimming, citation-placeholder rule or additional
fixture exclusion was added. Snapshot redaction affects only registered asset
URL identity fields, after unredacted equality succeeds; math, code, prose and
unknown URLs stay unchanged.

## Focused validation

The following checks have passed on the implementation branch:

- Round-trip target: all 15 fixture pairs equal, with four focused controls
  (five tests total) and their reviewed snapshots.
- Emitter target: 37 tests, including the two spacing regressions.
- Native build and math targets: 27 tests, including fixed-timestamp bytes,
  target independence/order, original-main restoration, real compiler hints,
  dynamic-file protection, native math corpus/recovery and the offline example.
- Four focused private tests for native diagnostics and World/session behavior.
- Two actual native HTTP/SSE preview tests, covering default pages, explicit
  web, source/include edits, retained pages on compiler error and recovery.
- One actual Chrome native preview test, covering both views, subset errors,
  real include edits and native MathML without KaTeX.

The example documents offline PDF/HTML/DOCX builds and both preview views,
including pending exports/includes and font-cache limits. Complete debug/release
suites, Clippy/docs and formatting results are recorded below.


## Complete local gates

Formatting, all-target/all-feature Clippy and denied-warning default and
browser-feature rustdoc pass. Full debug/release suites pass with 1,601/1,600
tests respectively. All 14 real Chrome preview tests run successfully without
skipping; the full native debug suite includes all 20 real HTTP/SSE serve cases.

Independent review confirmed all 15 required round-trip goldens, exact code
bytes, documented math preparation with its trailing spaces intact, registered
URL-only redaction after equality, and no remaining source spans or local paths.
The three existing emitter goldens change by exactly four removed pre-label
spaces. Measurements are recorded in `docs/dev/build-times.md`.


A final explicit-format probe found that source interception still followed a
`.md`/`.ipynb` suffix for a caller-selected native main. The World now retains
that original native file identity separately from temporary render mains.
Two real CLI/library regressions cover both suffixes and all three targets; a
private scratch/reset check proves the original file kind remains native while
other Markdown IDs still request synthesis. All 22 native integration cases and
12 World units pass. Final formatting, all-target/all-feature Clippy, both denied-warning rustdoc
builds, all 14 real browser tests and complete debug/release suites pass after
this correction. The final counts are 1,601/1,600.
