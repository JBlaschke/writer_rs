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
