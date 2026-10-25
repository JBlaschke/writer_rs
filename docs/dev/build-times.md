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
