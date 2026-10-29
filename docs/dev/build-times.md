# Build measurements

T00 baseline, measured on 2026-10-25 with Rust 1.99.1 (Homebrew), macOS arm64.
The revision is `26255f6` plus the T00 working tree before its first implementation
commit. All builds use the committed lockfile, the default Cargo job
count, and cached registry sources. Network access is unnecessary for these
measurements and tests.

Each cold measurement starts with an unused target directory (or, for the
initial release build, the previously absent default `target/`). Commands are
wrapped in `/usr/bin/time -p`; CPU time is user plus system time. Crate count
is the number of `Compiling` lines. Separate target directories keep the
measurements independent of incremental builds.

| date | commit | machine | rustc | measurement | value |
|---|---|---|---|---|---|
| 2026-10-25 | 26255f6 + T00 | macOS arm64 | 1.99.1 | Cold `cargo build --release --locked --offline` | 60.62 s wall; 377.97 s CPU; 478 crates |
| 2026-10-25 | 26255f6 + T00 | macOS arm64 | 1.99.1 | Cold `cargo build --locked --offline` | 85.04 s wall; 961.26 s CPU; 478 crates |
| 2026-10-25 | 26255f6 + T00 | macOS arm64 | 1.99.1 | Cold `cargo test --no-run --locked --offline` | 91.08 s wall; 1,026.17 s CPU; 505 crates |
| 2026-10-25 | 26255f6 + T00 | macOS arm64 | 1.99.1 | Release rebuild after touching `src/lib.rs` | 1.80 s wall; 2.20 s CPU; 1 crate |
| 2026-10-25 | 26255f6 + T00 | macOS arm64 | 1.99.1 | Scaffold release binary | 339,376 bytes (0.324 MiB) |
| 2026-10-25 | 26255f6 + T00 | macOS arm64 | 1.99.1 | Byte total of `vendor::list("")` after notice review | 1,808,514 bytes across 50 paths |

The scaffold does not yet call the compiler or emitters, so the linker removes
their code from the binary. Its size is not the size of the final PDF-capable
program. T32 and T75 append measurements after those paths are connected.

R1's reference measurement for Typst embedding alone was 58 s wall, 295 crates,
and 47,355,536 bytes (45.2 MiB), including embedded fonts. Those figures use a
different crate and are context, not a regression threshold.

## Local acceptance checks

All local checks passed:

- `cargo build --locked` in debug and release (with `--offline`).
- `cargo fmt --check`.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps`.
- `cargo test --locked --no-fail-fast`: 33 passed (3 normalization, 14 build
  metadata, 16 vendoring tests). The same 33 passed with `--release`.
- The binary writes exactly `writer: not implemented yet (T16)` plus a newline
  to stderr, writes nothing to stdout, and exits 2.
- OS/editor junk and a source map were planted under the asset directories
  during both test modes. None was listed, served, or embedded. Temporary probes
  and measurement-only target directories were removed afterward.
- A temporary example calling `SyntaxSet::load_defaults_newlines()` failed
  Clippy with `use of a disallowed method`; the example was then removed.
- Three successful `scripts/vendor.sh` generations produced identical byte and
  mode manifests: 43 downloaded payload files totaling 1,425,604 bytes, plus
  `VERSIONS.toml`, all mode 0644. A temporary script with one incorrect KaTeX
  checksum exited 1, printed the expected URL/hash diagnostic, and left the
  existing vendor tree and original script unchanged.
- Generated notices match both two-face versions and all 80 hayagriva styles;
  the two-face files preserve their 21 CRLF sequences with `-text` attributes.
- `bash -n scripts/vendor.sh` and `git diff --check`.

The build script, its tests, asset normalization helpers/tests, Rust formatting
settings, and Windows runtime configuration were lifted from `presenter_rs` as
specified by T00. The vendoring script and CI preserve the same structure with
writer's asset set and checks. Coordinator changes outside T00's paths are the
README, task-board status/notes, and the documented newline-preservation
amendment to the vendored-assets contract.

The user-supplied `AGENTS.md` style rules and the agreed Rust refinements are
included. Rustfmt uses an 80-column target and compact argument groups; wrapped
declarations use its normal layout. URLs, hashes, and exact-output strings may
exceed the width target. Formatting changes preserve configuration values and
all 50 asset files byte for byte.

The PR review clarified that the hayagriva overview records the locale count
and that the Unicode-to-LaTeX table arrives in T51. The asset total above was
remeasured after those notice-only wording corrections.

The workflow in `.github/workflows/ci.yml` repeats the Rust checks on Linux,
macOS, and Windows and checks vendoring drift separately. Remote CI results
require a push; they have not been run in this local implementation session.

The plan's wave audit currently reports missing T30 and T40 task definitions;
the task board records this pre-existing planning gap. T00 itself has no
dependencies. T01 is the next implementation unit; parsing and output pipelines
remain in their explicitly documented stubs.

## T03 compiler-backed acceptance (2026-10-25)

Measured at `2c9ad24` plus the T03 working tree on the same macOS arm64 host
with Rust 1.99.1. These are incremental suite runs with cached dependencies,
not cold-build benchmarks. Both commands used `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 2.74 s | 9.02 s | 16.42 s | 197 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 47.47 s | 53.20 s | 532.66 s | 197 passed |

The debug rendition suite executed its ten tests in 0.21 s; the sixteen World
integration tests took 0.07 s. Worlds use embedded fonts, disable system font
search, and require no network. The `writer_doc_embedded` test conditionally
skips until T31 supplies `@writer/doc`, as T03 specifies. The embedded mitex
package compiles and renders successfully in both build modes.

Formatting, Clippy across all targets/features with warnings denied, and
rustdoc with warnings denied also pass. Dependency files and the module graph
are unchanged. The CLI still exits with its T16 placeholder; these checks
exercise the compiler library APIs directly.

## T04 DOCX acceptance (2026-10-26)

Measured at `bdcd3f0` plus the T04 working tree on macOS arm64 with Rust
1.99.1. These are incremental suite runs with cached dependencies, wrapped in
`/usr/bin/time -p`; the complete suite includes T03's compiler tests.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 5.12 s | 13.82 s | 24.64 s | 293 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 58.97 s | 65.56 s | 636.27 s | 292 passed |

The debug-only property-builder assertion test explains the count difference.
The reference fixture generator is intentionally ignored in ordinary runs;
it was run explicitly and passed, and the regular byte-for-byte fixture check
passes in both build modes. The eleven DOCX package tests took 0.03 s in debug,
and the four reference integration tests took 0.01 s. Generated packages pass
V01–V20 and the independent docx-rs reader. Formatting, Clippy for every target
and feature, and rustdoc all pass with warnings denied.

T04 adds plain-text placeholder emitters with owner diagnostics. Full DOCX
content mapping remains with T40–T42; the CLI remains the T16 placeholder.

## T10/T11/T14 Markdown and asset acceptance (2026-10-26)

Measured at `4711b50` plus this batch's working tree on macOS arm64 with Rust
1.99.1. These are incremental suite runs with cached dependencies, wrapped in
`/usr/bin/time -p`; the suite also includes the existing compiler and DOCX tests.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 5.09 s | 15.99 s | 29.22 s | 438 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 69 s | 78.91 s | 729.42 s | 437 passed |

The existing debug-only assertion accounts for the count difference; the DOCX
fixture generator remains intentionally ignored. The front-matter parser's
47 tests include 5,456 generated documents and config-merge checks; its two
doctests run normally. The five probe tests compare the original R9 dumps
exactly, exercise 36 reviewed snapshots, and account for every directive-like
fence across 51 Markdown fixtures after pre-pass and attribute recovery.

Asset tests cover all three output modes, content versions, file-stamp cache
reuse, missing files, CSS fonts and HTML tokenizer boundaries. Cross-review
found an inherited prefix match for raw-text closing tags: `</stylex>` and
`</scripture>` must not stop placeholder replacement. The emitter now shares
the scanner's exact end-tag matcher, with regression coverage.

Formatting, Clippy for all targets/features, and rustdoc pass with warnings
denied for Clippy/rustdoc. The four canonical R9 fixture/dump extractions are
byte-exact, including the main fixture's intentional two-space Markdown hard
break. This fixture is the sole expected trailing-whitespace finding when
checking the staged diff. Dependency files and module declarations are unchanged.
The Markdown walker remains T12's stub; the CLI remains T16's placeholder.

## T12 Markdown frontend acceptance (2026-10-26)

Measured at `cf331a0` plus the T12 working tree on macOS arm64 with Rust
1.99.1. These incremental suite runs use cached dependencies and
`/usr/bin/time -p`; all existing compiler, DOCX and asset tests are included.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 6.01 s | 17.71 s | 34.34 s | 478 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 71 s | 81.79 s | 771.93 s | 477 passed |

The count difference remains the existing debug-only assertion, and the DOCX
fixture generator remains intentionally ignored. Formatting, Clippy for all
targets/features, and rustdoc pass, with warnings denied for Clippy/rustdoc.

The 34 walker tests cover the mapping table and diagnostics, including the six
lifted asset-helper tests. Six integration tests cover 16 pairs of reviewed
fixture IR/diagnostic snapshots, determinism, path/span completeness, invariants,
asset URL suffixes and all 35 R9 probe inputs through the new frontend.

Review and regression tests resolved caption footnote duplication, named note
spans inside anonymous notes, quoted braces in caption attributes, misplaced
equation labels and comrak's termless definition continuations. The existing
shared IR checker now accepts query/fragment suffixes after a valid asset id;
malformed ids still fail. The spec and affected briefs record these coordinator
refinements. Dependency files, IR shapes and module declarations are unchanged.

Markdown now produces unresolved IR through `parse`, `parse_blocks` and
`parse_inlines`. References and include expansion remain the T1B hooks; the CLI
remains the T16 placeholder, so no CLI build check applies yet.

The six empty diagnostic snapshots retain Insta's canonical final blank line.
Staged whitespace checks report only those generated EOF lines and pass with
`blank-at-eof` disabled; code and non-empty snapshots have no whitespace findings.

## T13/T17 HTML and resolve acceptance (2026-10-26)

Measured at `f6852ff` plus this batch's working tree on macOS arm64 with Rust
1.99.1. These incremental runs use cached dependencies and `/usr/bin/time -p`;
all existing compiler, DOCX, Markdown and asset tests are included.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 6.01 s | 18.55 s | 33.15 s | 550 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 81 s | 93.00 s | 869.60 s | 549 passed |

The existing debug-only assertion explains the count difference; the DOCX
fixture generator remains intentionally ignored. Formatting, Clippy across all
targets/features, and rustdoc pass, with warnings denied for Clippy/rustdoc.

HTML has 43 integration tests, two reviewed fragment snapshots and seven shared
highlighter tests. Resolver tests exercise the numbering tables, global labels,
auto ids, localized reference text, file-scoped and global notes, TOC assembly,
and counters/notes across alternative notebook outputs. An integration test
runs the new numbering fixture through Markdown parsing, resolve and HTML
emission. Its SHA-256 is exactly
`d15719d560e44e199a5d06c281fa4fcdb297a27e745851d248b554d59ae4031f`.
The two new Markdown snapshots capture the unresolved parser output; existing
snapshots are unchanged.

Cross-review and regressions cover label numbers after citation expansion,
global Typst note references alongside same-spelling Markdown names, finished
nested reference supplements, and recursive note removal in TOC copies. The
spec and task amendments also record reserved-name regex/auto-id termination
corrections, unknown-target diagnostic ownership and fixture-supplied classes
in HTML class coverage. Dependency files, public IR shapes and module
declarations are unchanged, apart from T17's documented citation re-export.

Citation rendering remains the T70 stub, reference/include scanning the T1B
hooks, and the CLI the T16 placeholder. No CLI build check applies yet. The
new empty diagnostic snapshot retains Insta's canonical final blank line;
staged whitespace checks pass with only `blank-at-eof` disabled.

## T15 theme and HTML page acceptance (2026-10-26)

Measured at `9b315d0` plus the T15 working tree on macOS arm64 with Rust
1.99.1. Cached-dependency runs use `/usr/bin/time -p` and include all existing
compiler, DOCX, Markdown, asset, resolver and fragment tests.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 8.78 s | 26.03 s | 52.61 s | 645 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 94 s | 106.92 s | 952.97 s | 644 passed |

The existing debug-only assertion and ignored DOCX fixture generator remain
unchanged. Formatting, Clippy for all targets/features and rustdoc pass, with
warnings denied for Clippy/rustdoc. After the debug suite, a test-only regex
construction was moved outside its loop for Clippy; the full release suite
covers that final form.

The lifted theme/render modules and tests now cover document templates and
code themes. New integration coverage comprises 25 theme and 23 HTML page
tests plus one reviewed context snapshot. All three output modes cover helper
assets, CSS, scripts, math, source positions and deterministic files. Review
regressions guard manifest diagnostic reads, decode escaped template names
for reachability, and record CSS asset cache hits in the serve watch set.

The default manifest/templates and child-theme fixture are present. Theme
styling remains T19, the math client remains T1A's placeholder, and CLI/pipeline
integration remains T16. Dependency files and public IR shapes are unchanged.

Windows CI subsequently found that the `problem_summary` integration-test
helper was compiled without its Unix-only caller. It now has the same
`#[cfg(unix)]` guard as that symbolic-link test. Focused Clippy with warnings
denied and all 25 theme integration tests pass locally after the fix; Windows
CI will verify the target-specific compilation.
This batch follows the user's stacked-PR workflow without waiting for CI or
merging of the completed dependency branch.

## T16 pipeline and CLI acceptance (2026-10-26)

Measured at `647fce5` plus the T16 working tree on macOS arm64 with Rust
1.99.1. Cached-dependency suite runs use `/usr/bin/time -p` and include the
existing compiler, DOCX, Markdown, asset, resolver and HTML/theme coverage.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 7.84 s | 30.28 s | 61.28 s | 733 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 127 s | 144.01 s | 1315.52 s | 732 passed |

The existing debug-only assertion explains the count difference; the DOCX
fixture generator remains intentionally ignored. Formatting, Clippy for all
targets/features and rustdoc pass, with warnings denied for Clippy/rustdoc.

The pipeline's 25 unit tests exercise WorldSlot lifecycle and input refresh,
subset/strict diagnostics, missing assets, effective config/resource paths,
output naming and source/resource/theme clash protection. CLI coverage includes
30 unit tests, 16 build tests and eight dump tests. Five scanner integration
tests plus a path-parser unit test cover best-effort include discovery; three
HTML integration tests cover 35 reviewed fixture/serve snapshots and actual
standalone output budgets. The example's main fragment has a reviewed CLI
snapshot too. Existing snapshots are unchanged.

`cargo run --locked --offline -- build examples/basic/document.md` succeeds,
printing `Wrote examples/basic/dist/document.html (672.6 kB)`. Its real PNG plot
is generated locally (836 bytes). The HTML is self-contained; site mode,
resolved/raw IR, template context and default-config dumps pass integration
checks. Future command/target bodies remain explicit milestone stubs.

Coordinator refinements normalize resource paths at the config/front-matter/
CLI source layer and validate output directories after front matter chooses
standalone/site mode. No dependency, Config/IR-shape or module-list changes.
Empty diagnostic snapshots preserve Insta's canonical final blank line;
staged whitespace checks disable only `blank-at-eof` for those generated files.

Windows CI subsequently measured `BuildFailure` at 128 bytes and rejected
pipeline results under `clippy::result_large_err`. Its `error` field is now
boxed; result signatures and diagnostic behavior stay the same. A size
regression runs on every platform. After this repair and the inherited T15
Unix-helper guard, formatting, all-target/all-feature Clippy, rustdoc with
warnings denied and the full debug suite (734 passed) pass locally. The
updated Windows CI run remains the check of the Windows target itself.

## T19 default theme acceptance (2026-10-26)

Measured at `afe0fa9` plus the T19 working tree on macOS arm64 with Rust
1.99.1. Cached-dependency suite runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 4.30 s | 28.45 s | 47.35 s | 768 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 128 s | 145.97 s | 1372.33 s | 767 passed |

The existing debug-only assertion explains the count difference; the DOCX
fixture generator remains intentionally ignored. Formatting, all-target/
all-feature Clippy and rustdoc pass with warnings denied. The basic example
builds a self-contained HTML page; HTML fixture and size budgets pass.

The three ordered CSS entries total 13,402 bytes against the 24,576-byte
budget. New coverage comprises 21 default-theme layout/build tests and 13
token/README/contrast tests. All 70 emitter classes and the directive, Typst,
notebook and ANSI extensions have selectors. Theme templates, dependency
files, production Rust sources and snapshots are unchanged in this feature.
Three older integration suites now account for inherited stylesheet entries,
fallback code styles and selectors inside inline CSS.

Browser QA used the in-app browser at 1200 and 375 px widths on the basic
example and the Markdown showcase. Temporary generated-page variants force
light or dark/print media rules without changing OS preferences. Notebook
and prefixed/unprefixed bibliography markup were added to the scratch page
for CSS checks; the notebook frontend is still T18's task. Checks cover
title/TOC/figures/tables/callouts, code line spacing, responsive columns,
checkboxes, gutter prompts, bibliography indentation and no page overflow.

Forced dark/print checks found pale metadata and syntax on light highlighted
lines. Supplemental print rules restore the light tokens, use monochrome
syntax, retain ANSI and error tints, and move prompts into normal flow. An
unbroken 240-character code token now wraps without horizontal clipping.
The required print block remains verbatim. Chrome is not connected, so
paginated print preview and margin-box page numbers remain unverified.
The math client remains T1A's placeholder; this task styles its markup.

## T31 bundled Typst document package acceptance (2026-10-26)

Measured at `ed9d4d2` plus the T31 working tree on macOS arm64 with Rust
1.99.1. Cached-dependency suite runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 4.64 s | 29.32 s | 55.37 s | 759 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 133 s | 150.75 s | 1412.39 s | 758 passed |

The existing debug-only assertion explains the count difference; the DOCX
fixture generator remains intentionally ignored. Formatting, all-target/
all-feature Clippy and rustdoc pass with warnings denied. The basic HTML
example still builds (`672.6 kB` on this branch, independently of T19).

Seven package files implement the frozen `@writer/doc:0.1.0` vocabulary with
only Typst built-ins. Eighteen compiler tests use embedded fonts, disabled
downloads and fixed dates to check rich layout and metadata, configuration
precedence, locale dictionaries (including 100 bibliography locale entries),
custom titles and CSL paths, numbering/oracle metadata, caption-less figure
placement, native multiline syntax highlighting, line ranges and cell output.
Negative cases check the bibliography path hint and positional-argument error.

Seven new Rust unit tests cover byte-exact ejection, existing destinations,
parent failures, unrelated files and Typst's platform-specific installation
path. The existing T03 embedded-package import test now runs against real
assets instead of skipping. Review fixes preserve regional language fallback,
native whole-block syntax highlighting and atomic `u64` highlight bounds
without passing them through Typst's narrower integer parser.

No dependency, public IR/config or module-declaration changes. The package
APIs are ready for T30/T32/T34/T72; PDF/export/eject CLI wiring and notebook
inclusion remain their own milestones. No snapshots changed.

## T30 Typst emitter acceptance (2026-10-27)

Measured at `b533020` plus the T30 working tree on macOS arm64 with Rust
1.99.1. Cached-dependency suite runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 5.98 s | 34.25 s | 60.79 s | 835 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 148 s | 163.92 s | 1597.00 s | 834 passed |

Formatting, all-target/all-feature Clippy and rustdoc pass with warnings
denied. The basic HTML example builds at 686.1 kB. The existing debug-only
assertion explains the count difference; the DOCX generator stays ignored.

Ten shared LaTeX unit tests and 32 emitter integration tests on Unix
(31 on Windows) cover preparation/macros, complete IR emission, literal
escaping, metadata, compiler-backed references and footnotes, notebook
output selection, fragments and split files, line maps and all asset modes.
The reviewed sample snapshot compiles with embedded fonts, offline packages
and a fixed date. Final focused debug/release runs also check code-only
list items and multi-paragraph list/definition bodies.

Review fixes preserve innermost spans, code-expression punctuation and
separate cell outputs. Final footnote/export names cannot collide with
source names that already contain a generated suffix. Source asset mode
resolves the explicit base directory to match canonical registry paths;
a Unix symlink regression compiles the result. Virtual and Export modes
perform no filesystem lookup. These small naming/path clarifications are
recorded in the spec and task brief. No dependency, IR/config shape,
module-list or pipeline changes. PDF/export CLI wiring remains T32/T34.

## T1A HTML math acceptance (2026-10-27)

Measured at checked T30 (`1ea66e3`) plus the T1A working tree on macOS
arm64 with Rust 1.99.1. Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 2.47 s | 16.85 s | 26.37 s | 868 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 167 s | 188.01 s | 1792.36 s | 867 passed |

Formatting, all-target/all-feature Clippy and rustdoc pass with warnings
denied. The basic example builds at 687.6 kB. Standalone KaTeX math is
661.4 kB, below its 700,000-byte budget. The existing debug-only assertion
and ignored DOCX generator keep their previous behavior.

The 37-equation corpus has the expected three math-core syntax failures.
Ten converter unit tests cover macro validation/retries, cached map changes,
concurrent callers, local equation state and XML-compatible output. The
23 integration tests cover KaTeX, embedded/system-font MathML, SVG geometry,
Typst MathML and fallback chains, diagnostic counts/spans and CLI builds.
Five new snapshots are reviewed; existing snapshots are unchanged.

Manual QA used the in-app browser on local standalone pages. All 12
equations in the current Markdown fixture render in each route, with no
console warnings/errors, and all SVG images load. MathML's glyph overhang
initially caused vertical scrollbars; vertical padding now contains it.
The compiler's black SVG ink was invisible in dark mode; the scoped image
filter restores contrast, with unfiltered ink under forced light/print
styles. This checks the print rules, not paginated print preview.

The 1,329-byte synchronous math client is lifted from presenter's script
with writer globals and a rendered-element guard. The HTML pipeline now
uses `MathRenderer`. Additive theme/README fixes and existing helper-test
adaptations are recorded in the task brief. The three theme stylesheets
total 13,580 bytes, within budget. No dependency, public IR/config shape or
module-list changes. T30's PR must merge before T1A.

## T40 DOCX body acceptance (2026-10-27)

Measured at merged PR #14 (`223fd42`) plus the T40 working tree on macOS
arm64 with Rust 1.99.1. Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 12.09 s | 47.26 s | 85.99 s | 891 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 175 s | 196.40 s | 1895.33 s | 890 passed |

Formatting, all-target/all-feature Clippy and warnings-denied rustdoc pass.
The basic HTML example still builds at 687.6 kB. The existing debug-only
assertion and ignored DOCX fixture generator explain the suite difference.

Nineteen integration tests, four helper tests and 99 reviewed snapshots
cover every generated package part and independent docx-rs reader counts
for seven IR fixtures and the sample document. All emitted packages pass
the structural validator, including byte-deterministic reference reuse.
Checks include accumulated styles, run merging, metadata, list numbering
and tasks, syntax scopes across lines, displayed code highlights, callouts,
notebook selection/prompts, raw XML/table boundaries, forward/page links,
scoped and nested notes, repeated-note fields and bookmark collisions.

The prepass handles cloned Page-reference supplements once and preserves
first logical label ownership when footnote bodies render in another part.
Crate-private emitter state and shared paragraph helpers are the additive
integration changes. No dependency, IR/config or module-list changes.
T41/T42 retain their rich figure/table/image/field/math stubs, and T43 owns
the later Word/LibreOffice smoke checks. T22 follows while this PR's CI runs.

## T22 live-reload client acceptance (2026-10-27)

Measured at checked T40 (`d757100`) plus the T22 working tree on macOS arm64
with Rust 1.99.1. Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 7.47 s | 45.00 s | 67.52 s | 891 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 172 s | 194.22 s | 1899.55 s | 890 passed |

Formatting, all-target/all-feature Clippy, warnings-denied rustdoc and the
basic HTML example pass. Final JavaScript/Python syntax checks pass; release
embedded-asset coverage is repeated after the final client refinements.

The package-free Python/Node/Chrome mock runner passes 19 browser checks
across eight isolated sessions with Chrome 156.0.8078.48, Node 26.12.0 and
Python 3.15.0. It covers loaded stylesheet handoff, visible CSS changes,
±1 px anchors, included-file/cell edits, math/image identity, details state,
title/template/raw-script behavior, per-view errors and warnings, dismissal,
storage, view navigation, stale fetches, fallback recovery, reconnect/restart
and measured retry intervals. A head-loaded script regression covers both
views. Console output has no uncaught exceptions or unexpected client logs.
The full command and limitations are in `live-reload-qa.md`.

The client and mock/browser transport are lifted and adapted from presenter;
Reveal behavior is replaced by the writer protocol. CI gains JavaScript
syntax checks and the existing asset smoke test includes the new client.
The coordinator clarifies same-block notebook position selectors and adds
the missing T22 brief. No dependencies, public IR/config, module lists or
Rust server code change. T23/T24 retain real-server and actual-pages QA.
Both feature PRs target `main`; merge T40 before T22.

## T18 notebook frontend acceptance (2026-10-27)

Measured at checked T22 (`b2e7905`) plus the T18 working tree on macOS arm64
with Rust 1.99.1. The merged T40/T22 tree (`43a1ed8`) is identical to that
base. Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 0.82 s | 28.01 s | 27.72 s | 981 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 191 s | 215.88 s | 1981.85 s | 980 passed |

Formatting, all-target/all-feature Clippy and warnings-denied rustdoc pass.
The basic HTML example builds at 687.6 kB, and the saved notebook example at
774.5 kB, both without diagnostics. Debug-only assertions and the ignored
DOCX generator retain their existing behavior.

Sixteen lenient-parser tests and 26 notebook integration tests cover metadata
layers and alias replay, options/tags, captions, candidate fallback, tables,
attachments, local-file boundaries, selection, cross-cell notes, includes
hooks, diagnostics and spans. Notebook unit tests cover ANSI, options, table
grids, streams and open-block detection. Twenty-one new snapshots and the
updated DOCX ANSI snapshot are reviewed. Asset labels are seeded by logical
first use so absolute-path hashes cannot reorder Windows snapshots.

Isolated Chrome 156.0.8078.48 with Node 26.12.0 passes 15 browser checks on
the saved example: six cells, five visible inputs, seven outputs, a loaded
plot, portable DataFrame and captions, ANSI palette/effects, and no console
errors or private ANSI leaks. Light/dark desktop and 390 px layouts are
checked visually. A wide-table overflow found at 390 px is fixed by scrolling
outputs below 64 rem, where prompts are already static. The page width is
375 px after the fix; desktop gutter prompts and native table columns remain
intact. After this stylesheet correction, all 37 theme/embedded-asset tests
pass again in debug and release and the rebuilt example passes browser QA.

No Python cells were re-executed: the example retains presenter's stored
outputs and Python bodies, with Markdown, metadata and option-comment edits.
Include expansion/reference scanning remains T1B; rich DOCX output remains
T41/T42. No dependency, public IR/config or module-list changes.

## T1B reference/include acceptance (2026-10-27)

Measured at checked T18 (`b523e05`) plus the T1B working tree on macOS arm64
with Rust 1.99.1. Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 10.03 s | 51.56 s | 76.97 s | 1,028 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 185 s | 209.50 s | 2064.16 s | 1,027 passed |

Formatting, all-target/all-feature Clippy, warnings-denied rustdoc and the
task-brief dependency audit pass. The basic, notebook and mixed HTML examples
build without diagnostics; the mixed report is 111.6 kB. Notebook computation
is read from stored outputs, never executed.

Fourteen scanner unit tests and four frontend integration tests cover syntax,
Unicode/boundaries, modes, groups, supplements, escaping, Link/code/math/raw
exclusions, fragment contexts and disabled references. Twenty-six include
tests on Unix (24 on Windows) cover first/last/nested includes, inherited and
child configuration, explicit empty metadata, file-scoped notes, repeated
files, assets/spans, mixed cycles, depth 8/9, notebook path validation, leaf
bodies, precise YAML/cell locations, and watcher traversal. Unix regressions
cover symlink cycles and directory-symlink/`..` semantics. The watcher retains
its existing depth boundary and ordinary lexical path output.

Twenty-one snapshots are added/updated and reviewed, activating references
in Markdown, numbering/math and notebook footnote fixtures and covering a
Markdown → notebook → Markdown chain. The fatal cycle replaces its earlier
placeholder output; two unused diagnostic snapshots are removed. Snapshot
paths handle Windows canonical prefixes and preserve source text, with
asset labels seeded from content order.

Isolated Chrome 156.0.8078.48 with Node 26.12.0 passes 19 checks on the mixed
example: nested headings, PNG/SVG loads, table/figures, one root TOC, 18 valid
internal targets and three independent file-local notes. A cross-file ref,
footnote and backlink are clicked successfully. Desktop and 390 px light/dark
layouts have no page overflow or clipped prompts; console logs are empty.
Screenshots are visually reviewed. The saved notebook cells are adapted from
the existing executed example without recomputation.

Private Markdown context and the default no-op notebook metadata hook retain
existing Fragment literals and custom include hooks. No dependency, public
IR/config or module-list changes. Bibliography lookup/rendering remains T70,
`check` T1C, Typst includes T53. Both PRs target `main`; merge T18 before T1B.


## T1C Markdown/notebook check acceptance (2026-10-27)

Measured at checked T1B plus its CI repair (`9ec2dc4`) and the T1C working
tree on macOS arm64 with Rust 1.99.1. Cached-dependency runs use
`/usr/bin/time -p`; CPU is user plus system time.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 15.70 s | 62.88 s | 104.64 s | 1,064 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 208 s | 233.33 s | 2176.28 s | 1,063 passed |

Formatting, all-target/all-feature Clippy and warnings-denied rustdoc pass.
Eleven library tests and four CLI unit tests cover structured failures,
source-order diagnostics, presenter include boundaries/front matter and
severity/reporting policy. Sixteen subprocess tests cover text/JSON schema,
exit statuses, quiet/deny, source/cell locations, errors, repeated includes,
compatibility promotion and absence of output writes. Eighteen Markdown and
eight notebook diagnostic snapshots are reviewed, with portable file paths
and explicit per-fixture code assertions.

All three basic, notebook and mixed examples produce `[]` with JSON check;
the basic HTML regression builds at 687.6 kB. Check uses frontend + lenient
resolution without creating a compiler World, rendering or executing cells.
Typst checks and explicit target checks remain the named T50/T71 stubs.

The final suite includes PR #18's original-byte include parsing and notebook
origin-redaction repair. Its focused 604-test run, Clippy and formatting also
passed independently. Remote CI is separate from these local results.
No dependency, IR/config, module-list or clap-shape changes.


## T41 rich DOCX acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T1C plus the T41 working
tree. Main (`91ed669`) subsequently merged all preceding PRs with an
identical base tree; rebase changed no tested implementation bytes.
Cached-dependency runs use `/usr/bin/time -p`; CPU is user plus system time.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 1.39 s | 18.43 s | 31.25 s | 1,115 passed |
| `cargo test --locked --offline --release --no-fail-fast` | 225 s | 254.06 s | 2553.84 s | 1,114 passed |

The debug run is warm after focused builds; the final narrowed legacy stub
assertion also passes its eleven-test package suite independently. Formatting,
all-target/all-feature Clippy and rustdoc with warnings denied pass.

Five image unit tests, 45 new feature integration tests and one cross-feature
prepass regression cover figures/caption fields, section/subfigure SEQ resets,
TOC/bibliography, native PNG/JPEG DPI, vector/fallback SVG, GIF/WebP/PDF
conversions, dimensions/clamping, table grids/merges/styles/recovery and rich
notes/bookmarks across parts. Every successful new test package validates;
whole-fixture snapshots also use docx-rs read-back. All 63 new/updated snapshots
are reviewed (57 new, six updated). Fixture IDs/media hashes and diagnostic
paths are portable; original-byte fixtures are generated locally.

The three basic/notebook/mixed examples build repeated byte-identical packages.
Manual basic/mixed DOCX outputs are 9.5/47.6 kB; their HTML regression builds
remain 687.6/111.6 kB. Canonical references reports its intentional duplicate
labels while preserving valid package structure. The canonical saved-output
fixture's missing selected image prevents output; a staged copy supplies that
file and redirects its outside-image probe to exercise its remaining outputs.

The target borrows the existing compiler World, preserves reference documents
and output-clash rules, and reads SOURCE_DATE_EPOCH once. Corrupt/non-SVG image
bytes fall back to located alt-text diagnostics; registry I/O errors propagate
before ZIP/write. T42 math/OMML and T70 bibliography loading remain separate;
T43 owns visual Word smoke tests. No dependencies, public IR/config/clap shapes
or module lists change. Watcher work follows without waiting for remote CI.


## T20 build watch acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T41 plus T20.
Cached-dependency runs use `/usr/bin/time -p`; CPU is user plus system time.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 10.75 s | 99.21 s | 92.53 s | 1,182 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 284 s | 329.95 s | 2795.66 s | 1,181 passed |

The debug run completed before the desktop crash. Sources and tests survived;
temporary logs did not. The release suite was rerun after recovery and passed,
including the native watcher, library-loop and CLI subprocess tests. Two
helper tests are deliberately ignored. Local native-service access is needed
for macOS FSEvents; production and CI contain no sandbox workaround.
Formatting, all-target/all-feature Clippy and rustdoc with denied warnings
pass. Basic/mixed HTML builds remain 687.6/111.6 kB; basic DOCX is 9.5 kB
with its existing T42 math warnings.

Twenty-one private watcher tests, twenty backend tests, seventeen loop tests,
nine CLI subprocess tests and one status unit test cover canonicalization,
role/package filtering, bounded batching, save/rename/recreate behavior,
initial and later build/config/include/asset failures, independent targets,
quiet/reporting/usage behavior, catch-up and suppression of successful outputs.
The two Unix symlink regressions preserve OS traversal semantics. Windows
parent guards and portable bounded child cleanup are retained from presenter.

The CLI watch stub is replaced. Narrow crate-private pipeline/report helpers
preserve original multi-target output naming during partial rebuilds and
borrow reports without losing source excerpts. Pure access events are ignored
to prevent inotify self-rebuilds; all mutation events still count. Server/SSE,
session runtime and actual PDF generation remain their named later milestones.
No dependencies, public IR/config/clap shapes or module lists change.


## T21 dev server acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T20 plus T21.
Cached-dependency runs use `/usr/bin/time -p`; CPU is user plus system time.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 0.76 s | 88.79 s | 29.36 s | 1,235 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 305 s | 362.68 s | 3016.97 s | 1,234 passed |

Formatting, all-target/all-feature Clippy and warnings-denied rustdoc pass.
Nineteen real HTTP/SSE tests and 34 new module tests cover all routes and
content/cache headers, file streaming/single ranges, generated URLs/fonts,
last-good registries, theme stylesheet rewriting/fallback, view selection,
current/previous SVG hashes and security headers, SSE hello/build/reconnect/
lag/drop behavior, Host checks on every route and bounded stalled-request
shutdown. State tests exercise all nine view-result combinations, retained
errors and warnings, exact diagnostic deduplication and title/page rotation.
Event JSON golden assertions cover null/omission rules, every error kind,
notebook/include excerpts, TOML provenance and first-error compiler locations.
No snapshot files are added; explicit golden JSON is reviewed in source.

The initial full run reproduced startup timeouts for all fourteen concurrent
watch-loop child processes; the isolated case passed. Serializing their
lifetimes passes all seventeen loop tests in 29.73 s and the final full debug
suite, keeping ten-second operation deadlines and production code unchanged.
This narrow inherited harness fix is recorded in the task brief. Native
FSEvents and loopback tests use approved local service access.

Basic/mixed HTML remains 687.6/111.6 kB; basic DOCX remains 9.5 kB with its
existing T42 math warnings. The pages types, shell and SVG routing are real;
PagesBuilder and pages.js remain the explicit empty/placeholder T24 contract.
The serve CLI/session remains T23. No dependency, public IR/config/pipeline/
clap shape or module list changes.


## T25 browser smoke acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T21 plus T25.
Cached-dependency runs use `/usr/bin/time -p`; CPU is user plus system time.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 11.55 s | 170.20 s | 99.36 s | 1,235 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 307 s | 363.21 s | 3052.82 s | 1,234 passed |

With `browser-tests`, all 28 client units pass (3.46 s build, 5.26 s tests).
All six browser/helper tests pass with `WRITER_REQUIRE_CHROME=1` (11.16 s
build, 8.32 s tests). The actual browser is Google Chrome 156.0.8078.48 at
`/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`; CHROME_PATH
is unset, so discovery selects that first installed macOS bundle.

The copied standalone basic document renders its single document root,
highlighted code, KaTeX, complete images and fonts without page errors or
external network resources. Client fixtures verify error/log collection,
fragments, promises/special values, refused evaluations, stale/replacing
loaders and navigation failures. Pure helpers cover required-browser policy
and reserved/non-UTF-8 file URL bytes. Browser calls and cleanup are bounded
and serial; ordinary suites launch no browser. Non-Unix driving returns the
explicit platform error while the feature remains portable to compile.

Transport units cover executable/discovery order, app bundles and snap
placement, private profiles, NUL framing, closed/broken/malformed pipes,
quiet/failed browsers, timeout/stderr tails, lifecycle matching, cleanup and
watchdog ownership. Review fixes canonicalize relative/symlink scratch bases,
classify a bare snap wrapper through its resolved executable, fail promptly
on startup protocol refusals and leave unverified/reused PIDs alive. The
initial fake-browser failures were fixture replies sent to stdout (the real
launcher redirects it); corrected fixtures use DevTools fd4 and pass.

The public Page API and a browser discovery helper are feature-gated. Required
transport internals stay in the owned devtools module; the T77 print stub and
all dependency/IR/config/pipeline/CLI/module lists remain untouched. Real
serve/browser integration follows in T23, actual pages in T24.

Formatting, all-target/all-feature Clippy and default/feature rustdoc with
warnings denied pass. Basic/mixed HTML remains 687.6/111.6 kB. Browser tests
were executed, not skipped; native process/service access was used locally.

## T23 serve CLI acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T25 plus T23.
Cached-dependency runs use `/usr/bin/time -p`; CPU is user plus system time.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 9.98 s | 153.87 s | 113.48 s | 1,283 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 320 s | 383.61 s | 3071.46 s | 1,282 passed |

Formatting, all-target/all-feature Clippy and default/feature rustdoc with
warnings denied pass. All 32 launcher/session units pass (13.31 s build,
11.98 s tests), all 16 real CLI tests pass (11.97 s build, 7.47 s tests), and
all nine real-server Chrome tests pass in 36.34 s with required Chrome
156.0.8078.48. The browser QA measurements and checklist mapping are recorded
in docs/dev/live-reload-qa.md; no browser acceptance was skipped. Basic/mixed
HTML remains 687.6/111.6 kB.

The CLI starts watching before build1, announces its actual bound URL, handles
Ctrl-C before/during/after the first build and during binding, then rebuilds
only affected views while keeping successful output. Tests cover config/theme/
include repair, nested missing config ancestors, source/include/notebook
positions, content-versioned images, equation-image registry routes, Host and
port errors, quiet reporting and live warning/error state. The compiler World
is retained/reset per session, discarded after caught panics and reacquired
for independent stages. A focused regression keeps package filtering aligned
with the World's retained config across disk config changes and discard.

Browser checks verify pixel-level reading/source anchors, selective DOM/math/
image updates, actual reload counts, merged/dismissible errors, persistent
warnings, view shortcuts/storage, reconnect/backoff and stale-tab behavior.
The first run's only failure was a test confusing a skipped event with retained
client view status; the corrected complete run passes without a client change.

Coordinator-approved ownership additions: private cli/serve/session.rs child,
two crate-private watcher helpers, removal of the serve stub assertion and
additive T22 QA evidence. Public IR/config/pipeline/clap/module lists and all
dependencies remain unchanged. Actual pages and .typ end-to-end support retain
their T24/T50/T52 boundaries; this session uses the explicit empty PagesBuilder.


## T50 structured Typst frontend acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T23 plus its watcher repair
and T50. Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 20.32 s | 212.70 s | 135.70 s | 1,359 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 313 s | 378.60 s | 3193.00 s | 1,358 passed |

Formatting, all-target/all-feature Clippy and default/feature rustdoc with
warnings denied pass. All 52 public frontend tests pass in 2.24 s and all 22
private units pass. Twelve unresolved-IR/diagnostic corpus snapshots are
reviewed, including portable path/asset redaction and untouched math bytes.
Basic/mixed HTML remains 687.6/111.6 kB; compiler-backed subset validation and
HTML/DOCX library composition pass offline. No external Typst process or
frontend code evaluation is used.

Coverage includes metadata precedence, scoped captured literals/import aliases,
markup/call/table families, quotes, source-based labels/citation groups, notes,
verbatim math, recursive includes and assets, syntax/unsupported diagnostics,
strict CLI overrides, normalized IR invariants and target-free CLI check. Table
capacity, captured-content replay and literal-value limits have adversarial
regressions. Native .typ target dispatch and cross-format includes retain their
T52/T53 boundaries.

PR24 CI exposed a missing-config ancestor synchronization assumption on Linux
and Windows. Its repair `d8e91e7` waits for post-watch-update SSE events and catches
intermediate directories appearing during watch installation. Twenty-three
watcher units and 53 CLI/backend/loop integration tests, formatting and Clippy
pass before push; the same repair is included in these final full-suite runs.
The three-OS CI reruns independently.


## T32 embedded PDF pipeline acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T50 plus the T23 missing-theme
CI repair and T32. Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 23.69 s | 225.93 s | 148.28 s | 1,387 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 379 s | 443.70 s | 3591.66 s | 1,386 passed |

Formatting, all-target/all-feature Clippy and default/feature rustdoc with
warnings denied pass. All 80 focused CLI/PDF/oracle/emitter tests and 10 World
units pass. Thirty-five new Markdown source/diagnostic snapshots are reviewed;
portable root/suffix redaction preserves exact Typst escapes and asset identity.
The oracle covers the complete required corpus plus three numbering policies,
with the documented unique-label variant of the duplicate-label fixture.

Basic/mixed builds produce 3/2-page PDFs at 50.8/91.5 kB, with unchanged HTML at
687.6/111.6 kB. Fixed-time output is byte-identical across fresh/retained Worlds.
Coverage includes virtual file/byte assets, same-basename bibliographies, CSL
and named styles, relative template imports and mount removal, source/include/
notebook diagnostics, package hints, independent target failure, output/input
refusals, no writes on compile failure and source-only dump. A fake XMP packet
inside an actual PDF title cannot override its page count. Normal/quiet mixed
CLI failures print one mapped compiler error while keeping successful HTML.

The initial release run found one inherited unit still expecting the T32 stub;
it now verifies the generated dotted-stem PDF path. Both final full suites pass.
The remaining missing-theme CI test receives the same post-watch-update SSE
barrier as config recovery (PR24 repair `fe896de`, propagated to T50/T32).

Coordinator-approved private seams retain all failed target diagnostics, expose
the World's full session timestamp and remove managed resource mount subtrees.
The target owns no reset/eviction. Native .typ dispatch, export, full citations
and actual pages retain their T52/T34/T70/T24 boundaries; dependencies and public
API/config/IR schemas are unchanged.


## T24 compiler-backed pages acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T32 plus the independent
main-CI watcher repair and T24. Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 25.94 s | 189.05 s | 162.17 s | 1,401 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 386 s | 458.23 s | 3720.23 s | 1,400 passed |

Formatting, all-target/all-feature Clippy and denied-warning default/feature
rustdoc pass. All 12 pages units, 26 watcher units and 47 real CLI/watch/server
integration tests pass. All 13 real-server Chrome cases pass in 44.91 s with
required Chrome 156.0.8078.48; all 26 A–K mock/Chrome checks pass. JavaScript and
Python syntax checks pass. Detailed measured page anchors, selective requests,
lazy/decode races, zoom, startup/restart state and last-good error behavior are
recorded in docs/dev/live-reload-qa.md. No browser acceptance was skipped.

PagesBuilder loads generated source once through the retained World, combines
emission/compiler findings once and keeps all failure locations/cells/hints.
It exports separate content-addressed SVGs, rounds point sizes and reuses exact
Bytes storage for matching compiler page keys anywhere in the prior good set.
Only successful complete sets replace its cache; reset/eviction stay in T23.
The client reserves final geometry before native lazy loading, preserves page
fractions and keeps old decoded pixels until replacements load/decode.

The first checks exposed two test expectations tied to the prior stub: the
server script body and img.width/height IDL values. The route test now compares
the actual embedded script; geometry checks use intrinsic attributes and
separate rendered dimensions. The final complete browser/full suites pass.

Independent main CI repair `fd233f1` is included through equivalent `dbb25e5`:
macOS historical directory creation is filtered by identity; the already
present PR22 child-lifetime guard is retained. Its main-only validation is in
docs/dev/watch-ci-recovery.md and its standalone PR27 has no feature dependency.
Full .typ build/serve acceptance and compiler-side includes retain T52/T53.


## T51 Typst math conversion acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T24 plus T51.
Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 19.35 s | 258.85 s | 149.88 s | 1,428 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 372 s | 444.06 s | 3709.87 s | 1,427 passed |

Formatting, all-target/all-feature Clippy and denied-warning default/feature
rustdoc pass. All 14 public converter tests and 13 private symbol/call units
pass. All 72 source/diagnostic snapshots were reviewed against the 36 equation
bodies. Actual Pandoc 3.12.1 reproduces 33 recorded successes and three reader
failures; comparisons use only whitespace/braces normalization and consumed,
exact per-form exceptions. Eight Tylax records remain historical oracle data;
Tylax was not executed. All 36 Writer bodies compile with pdflatex.

Default generation and `--check` reproduce 527 names and 1,296 character rows
from the three hash-verified archives. All 38 shorthands are represented. All
11 Python tests pass, including pdflatex for all 1,058 distinct expressions
under amsmath/amssymb plus only the declared extra package. TLS verification
stayed enabled; this host's Python used its installed Homebrew CA bundle.
No generator, compiler, filesystem or network call occurs in runtime lookup.

Initial integration tests found styled Unicode digits classified as Number,
missing scalable double brackets, and malformed call arguments omitted by a
typed syntax iterator. These now use the character table, mapped delimiters
and source-preserving recovery respectively. Pinned parser evidence corrected
the negative named-literal whitelist to native `#(-3)` / `#(-1pt)` syntax;
compound expressions and malformed direct forms retain exclusive code fallback.
Parser-depth recovery, UTF-8 equation-relative ranges and TeX control-word
boundaries pass. The complete final suites include these repairs.

The converter returns a body and warnings only; package discovery remains a
lookup consumed by T60. DOCX compiler MathML/OMML, native `.typ` dispatch and
LaTeX target composition retain T42/T52/T60 ownership. Dependencies, public
module lists and config/IR schemas are unchanged.


## T42 editable DOCX math acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T51 plus T42.
Cached-dependency runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 18.31 s | 280.22 s | 152.48 s | 1,475 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 410 s | 487.84 s | 4088.59 s | 1,474 passed |

Formatting, all-target/all-feature Clippy and denied-warning default/feature
rustdoc pass. All 14 private units, 44 focused public/package tests and 20
package snapshot tests pass. The complete 37 LaTeX and 36 native Typst corpus
classifications agree with the specification. All 73 OMML goldens and 19 new
or changed DOCX package goldens were independently reviewed. Four foreign
MathML fixtures and bounded parser/mapping tests cover recovery, entities,
namespaces, styles, fences, matrices, accents and malformed markup.

All 21 actual Pandoc 3.12.1 DOCX probes pass the shared-shape comparison.
The two styling decisions and additional observed representation aliases are
recorded in math.md §5.2, restricted to named exact forms with negative guards.
No fixture is exempted. An inner plain `\mid` now remains a relation; only
explicit stretchy inner bars split delimiter parts. Specific accents/bars/
group characters precede n-ary limit classification. Both fixes have regressions.
The snapshot formatter also preserves entity references inside text leaves.

DOCX integration verifies both editable routes, every recovery transition,
once-only located macro diagnostics, forced SVG/PNG media and footnote
relationships, reference Normal-size scaling and inline descent, three-column
frames, number-only bookmarks, forward links/PAGEREF and section/SEQ corrections.
Ordinary container styles retain Equation; explicit custom Div styles override
it. Frame cells avoid list indentation and clamp pictures to their own width.
Real retained-World pipeline builds validate, reproduce bytes and keep successful
HTML/DOCX when the independent PDF template fails.

Two initial integration failures were fixture setup errors: a pre-hashed
ResolvedRef anchor and a Normal style assumed to contain an inherited size.
Tests now supply the logical label and an explicit reference Normal override.
The old stub expectation now checks a real editable numbered equation without
requiring a renderer. All final suites include these corrected checks.

The converter uses private parse/fold/render/text children with existing XML
and codex dependencies. Runtime creates no World and calls no external tool.
DOCX borrows the session renderer and reuses picture/field/bookmark builders.
Native `.typ` CLI dispatch and reference/manual Word validation remain T52/T43;
no dependency, public config/IR schema or public module list changes.


## T33 math PDF corpus and check preflight acceptance (2026-10-28)

Measured on macOS arm64 with Rust 1.99.1 at checked T42 plus T33. The final
complete debug run follows a reviewed diagnostic snapshot refresh; its build
cache is warm. Runs use `/usr/bin/time -p`.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 0.46 s | 141.81 s | 41.17 s | 1,507 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 417 s | 497.54 s | 4285.13 s | 1,506 passed |

Formatting, all-target/all-feature Clippy and denied-warning default/feature
rustdoc pass. All 19 check/CLI private units and 78 focused library, CLI,
corpus and existing emitter tests pass. Every one of the 37 LaTeX rows retains
its DOCX classification and gains the specified PDF outcome: 34 compile;
18/32/37 report preflight errors, while unbalanced-brace 19 remains accepted
by mitex and diagnosed by math-core.

The 34 supported equations compile into one actual PDF with embedded fonts,
packages disabled and a fixed timestamp. Fresh and retained Worlds reproduce
identical PDF bytes; compiled page count and XMP page_count agree. Each parser
rejection emits the original source as native raw math in inline/numbered
forms and still compiles, preserving labels/numbering. Bypassing preflight
produces mapped plugin compiler errors. Prepared LaTeX, macro isolation for
17/31, tag/environment preparation and both raw/string literals round-trip.
Ordinary/strict mixed builds retain successful outputs and only conversion
findings escalate; tag warnings keep their base severity.

Check uses the exact shared mitex parser/error normalization without a World,
fonts, package access or output files. Baseline math-core syntax covers visible
source equations across HTML routes. Target-union notebook selection, hidden
outputs, distinct-node identity, nested contexts, included files and notebook
cells preserve diagnostics and avoid duplicate visits. Native Typst equations
are skipped by both cheap LaTeX parsers. Macro warnings are once-only and
prepared-source diagnostics agree with the emitter. PDF-only CLI lists,
configured defaults, JSON/text/quiet/deny behavior and existing T71 stubs pass.

The first full debug run exposed one expected existing golden change:
math.md's line19 tag/label now reports math-tag-ignored. The seven added JSON
lines were reviewed against the source; the corpus and both final full suites
pass. An earlier library assertion assumed a canonical path spelling on macOS;
it now compares file identities while preserving production source provenance.

Only the private preflight helper and additive check/CLI seams change runtime
code; emitter source and fallback bytes stay unchanged. Other explicit target
lints, native `.typ` compilation/exports and references retain their existing
milestone boundaries. No dependencies or public API/config/IR/module-list
changes are introduced.


## T43 Word reference and showcase acceptance (2026-10-28–29)

Measured on the same macOS arm64/Rust 1.99.1 checkout, using the lockfile,
offline registry and warm dependency cache. The release command completed
across the overnight session; its recorded wall time differs substantially
from Cargo's own build duration and the individual test durations. Retain the
raw measurement, but do not use that wall time as a clean compile benchmark.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 24.88 s | 313.74 s | 172.82 s | 1,529 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 615 s | 5,115.55 s | 6,779.32 s | 1,528 passed |

Formatting, all-target/all-feature Clippy and denied-warning default/feature
rustdoc pass. All 22 focused CLI/reference/showcase tests and both ignored
fixture generators pass. The reviewed package goldens cover all 101 built-in
styles, every one of the 82 reachable styles and one semantic custom callout.
Independent docx-rs reading records 81 top-level paragraphs, 102 styles, two
abstract numbering definitions and one shared media part. XML and python-docx
agree on 81 paragraphs, two tables and two inline drawings.

Actual system textutil import, isolated python-docx 1.2.0, QuickLook thumbnail
creation and portable read-back all pass in the dedicated four-test smoke run.
The first-page PNG was inspected. Actual Pandoc 3.12.1 reference merging also
passes without committing its reference. The default Python lacks python-docx;
the dedicated run used the bundled interpreter rather than installing a package.
Manual Word, LibreOffice and Google Docs checks remain unperformed release QA,
with commands, exact capabilities and retained evidence documented separately
in `docx-validation.md`.

The CLI generates complete bytes before atomic no-clobber creation, preserves
existing files/directories/symlinks, prints correctly escaped TOML guidance,
and keeps bytes independent of config/environment timestamps. Both hand-written
reference fixtures reproduce exactly; copied header/theme image closure, high
IDs and localized style-name lookup pass. No dependency, IR/config or module-list
changes were needed; the narrow DocxReference eject arm is the sole T72-owned
integration seam, and other eject resources remain T72.


## T34 portable Typst export acceptance (2026-10-29)

Measured on the same macOS arm64/Rust 1.99.1 checkout with cached dependencies,
the lockfile and offline registry. The long release wall measurement differs
from Cargo's build duration plus individual test timings; preserve the observed
values without treating it as a clean compile benchmark.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 17.37 s | 299.22 s | 174.58 s | 1,561 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 1,108 s | 2,964.83 s | 11,294.61 s | 1,560 passed |

Formatting, all-target/all-feature Clippy and denied-warning default/feature
rustdoc pass. Eight planning units, 23 new integration/CLI cases and 77 inherited
emitter/CLI/watch regressions pass. The actual basic CLI source export succeeds.
No external Typst executable, network, host font or renderer is needed to export.

Fresh physical Worlds compile moved include/figure/notebook/resource trees after
the original input project is removed. Shared references, nested scoped notes,
figure numbers, referenced file/byte assets and custom template shadowing remain
correct. Actual bibliography/CSL copies compile through a hand-resolved fixture
without pretending T70 citation resolution exists. Package ejection uses its
library API, and embedded-package priority remains unchanged.

All sidecars are preflighted through actual build calls: Unix symlink/hardlink
aliases for include/asset/bib/CSL/template, same-path resource clashes on every
OS, and new/../ aliases leave previous main bytes, resources, empty directories
and symlink targets unchanged. Missing reads write nothing. Byte-identical
repeated exports preserve unrelated files. Quiet, strict math fallback, output
naming, format override, mixed-target independence and native-input refusal pass.

The valid private includes.md derivative keeps its actual A→B/note/image chain;
the unchanged cycle/nested-include fixture remains negative. Notebook markers
and exact occurrence counts independently protect Markdown and raw-Markdown
content. Number oracles are explicitly enabled in test Worlds. The reviewed
BOM/empty-front-matter scanner correction matches the frontend for BOM/no-BOM
with LF/CRLF/CR. Existing output/source/virtual emitter behavior remains covered.

The narrow §10 contract clarification shares case-folded, Windows-safe names
between referenced assets and copied resources. Only two crate-private emitter
helpers and existing stub-test migrations cross task ownership; no dependencies,
IR/config/public shapes or module lists change. The template's own imports are
not copied, and Typst CLI package/template ejection remains T72.


## T52 native Typst document acceptance (2026-10-29)

Measured on the same macOS arm64/Rust 1.99.1 checkout with warm dependencies,
the lockfile and offline registry. Both default and browser-feature code paths
were validated with embedded fonts and packages disabled for mandatory tests.

| command | Cargo build phase | total wall time | CPU time | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 41.18 s | 394.95 s | 183.34 s | 1,601 passed |
| `cargo test --release --locked --offline --no-fail-fast` | 473 s | 568.24 s | 5,010.27 s | 1,600 passed |

Formatting, all-target/all-feature Clippy and denied-warning default/feature
rustdoc pass. All 14 real Chrome preview tests pass without skips in 43.80 s.
The new native preview case follows default pages, web recovery, native control
flow and included-file edits through actual HTTP/SSE and Chrome. All 20 real
HTTP/SSE serve cases pass in the full debug suite.

Twenty-two native pipeline/CLI cases and five math integration cases pass. They
exercise original-file native compilation, fixed fresh/retained PDF identity,
source edits, both target orders, independent subset failures, native errors/
hints, last-good outputs, default/format/quiet/engine behavior, template warning,
consumed dynamic source/image/font guards and once-only error reporting.
All 36 native math classifications remain exact: 32 editable OMML forms and four
no-MathML routes, with display picture/final-source recovery. Native HTML's two
MathML knobs, image mode, font/CSS/asset/baseline rules, real LaTeX macros and
retained-main restoration pass. No external compiler, TeX, Pandoc, fonts or
network is needed for these mandatory tests.

The 15 required Markdown fixtures compare equal before snapshot redaction,
with four focused normalizer controls. All code fields stay byte-exact; math
changes only through documented preparation. Both sides use the same effective
emission options before independent metadata merge/resolve. The two narrow §7
refinements cover notebook provenance and block-only code line controls; no
citation, whitespace or fixture-level exemptions were added. All goldens were
independently reviewed.

Two new emitter regressions and all 37 emitter tests preserve nested/repeated
note punctuation and intentional spaces. The actual runtime fix removes one
invented pre-label space; three inherited goldens change by exactly four spaces.
The initial preview assertions were corrected to the existing web-document
marker and shared error-overlay contract; no client/protocol change was needed.

Four new private units preserve all native diagnostics and selected/attempted
font-read inventory. FontStore's existing cached font-content policy remains:
restart after disk font edits. The consumed-font integration uses explicit
public World::font selection; actual compiler-selected custom-font clash
coverage is not claimed. Local font conversion tools were unavailable, and no
font fixture, package install or dependency was added. No public API/config/IR
shape changes; T53 includes and positive LaTeX export retain their next-milestone
boundaries.

The final explicit-format follow-up pins the original native main's file kind,
so `--format typ` on `.md` or `.ipynb` bypasses synthesis while actual includes
still use it. Its two CLI/library regressions and a private scratch/reset unit
pass. The measurements above are from the complete rerun after that correction,
not the earlier 1,598/1,597 pass. All 14 browser tests ran again without skips.

## T53 native Markdown/notebook include acceptance (2026-10-29)

Measured from `573f480` plus the T53 working tree on macOS arm64, Rust 1.99.1,
using cached dependencies and incremental targets. These are suite timings,
not cold-build benchmarks. `/usr/bin/time -p` wrapped both final commands.

| command | Cargo build phase | total wall | CPU (user + system) | result |
|---|---|---|---|---|
| `cargo test --locked --offline --no-fail-fast` | 26.37 s | 380.82 s | 179.28 s | 1,634 passed, 3 ignored |
| `cargo test --release --locked --offline --no-fail-fast` | 8m 58s | 625.73 s | 5,382.40 s | 1,633 passed, 3 ignored |

The debug/release difference is the existing debug-only adversarial frontend
case. Includes pass through the actual pinned compiler and package: 22 new
integration cases, six provider and two diagnostic-merge units, plus two live
HTTP/SSE recovery tests within the 22-case server suite. Generated-source maps,
original read/json bytes, selection, assets, parent references/citations,
cycle/depth, failed dependencies and independent target success are covered.
Both changed frontend snapshots and both new provider snapshots received
independent review. Existing export/round-trip/native-source regressions pass.
Configured formatting, denied-warning Clippy (all targets/features), rustdoc
with denied warnings and `git diff --check` pass. No external TeX or browser
installation is required by this milestone's acceptance tests.
