# T97 — `@writer/doc` 0.5.0: the version bump

| | |
|---|---|
| Milestone | M8 |
| Size | S |
| Depends on | T96 |
| Parallel-safe with | nothing: T98 starts after this task |
| Files you own | the package-version occurrences of `0.4.0` in: `src/typst/mod.rs`, `src/typst/includes.rs` (two test literals), `assets/typst/writer-doc/typst.toml`, `assets/typst/writer-doc/template.typ` (the `#import` line only), `docs/guide/customization.md` and `docs/guide/typst.md` (two `#import` lines each), `examples/typst/document.typ`, `tests/fixtures/serve/{native,template}.typ`, `tests/fixtures/typ/{include-notebook-missing,include-notebook,minimal,subset,template}.typ`, `tests/{cli_serve,diagnostic_payloads,pdf_pipeline,typst_emit,typst_export,typst_frontend,typst_includes,typst_package,typst_world}.rs`, and the 23 files of `tests/snapshots/` listed under Hints |

## Goal
The embedded Typst package is `@writer/doc` version `0.5.0` everywhere: the constant, the package
manifest, the starter template's import, the example, the guide's import lines, the fixtures,
the test literals and the snapshots. Nothing else changes: the package behaves exactly as `0.4.0`
did. T98 then adds the behaviour that the new version number announces.

## Read first
- `docs/spec/typst-dialect.md` §4.2, the note "Contract change (2026-11-21, package `0.5.0`; T97,
  T98)", §4.6's note of the same date (the starter's import line) and §10's (`eject
  typst-package DIR` writes `DIR/writer/doc/0.5.0/`)
- `src/typst/mod.rs` (the constant), `src/typst/package.rs` (how the version becomes the package
  path) — read only
- `docs/tasks/T93-writer-doc-0-4-0.md`: the same task one version earlier

## API to implement
```rust
// src/typst/mod.rs
pub const WRITER_DOC_VERSION: &str = "0.5.0";
```
```toml
# assets/typst/writer-doc/typst.toml
version = "0.5.0"
```

## Steps
1. Keep a copy of the changes the tree already holds (T96's are not committed), so that you can
   show at the end what this task added. From the repository root:
   ```sh
   git diff HEAD > target/t97-before.diff
   ```
2. Replace the string `@writer/doc:0.4.0` with `@writer/doc:0.5.0` in every file under "Files you
   own" (it is the only form in which most of them name the version):
   ```sh
   git grep -l '@writer/doc:0\.4\.0' -- src tests examples assets/typst docs/guide \
       | xargs sed -i '' 's|@writer/doc:0\.4\.0|@writer/doc:0.5.0|g'
   ```
   (`sed -i ''` is the macOS form; on Linux write `sed -i`.)
3. Change the five occurrences that have another form, by hand:
   - `src/typst/mod.rs`: `WRITER_DOC_VERSION` is `"0.5.0"`.
   - `assets/typst/writer-doc/typst.toml`: `version = "0.5.0"`.
   - `tests/typst_export.rs`: the path `packages/writer/doc/0.4.0/typst.toml` becomes
     `packages/writer/doc/0.5.0/typst.toml`.
   - `tests/typst_world.rs` (`writer_doc_other_version`): the expected text is `latest is 0.5.0`.
   - `tests/snapshots/typst_frontend__unsupported-diagnostics.snap`: `differs from embedded
     version 0.4.0` becomes `differs from embedded version 0.5.0`.
4. Run `git grep -n '0\.4\.0' -- src tests examples assets/typst docs/guide`. The only line left
   must be `tests/vendor_smoke.rs:318` (it names the package `cetz` 0.4.0, not this one).
5. Run `cargo test`. Every test must pass without `INSTA_UPDATE`: step 2 already rewrote the
   snapshots, so a snapshot that still fails shows a change this task must not make. If one
   fails, read the difference; do not accept it blindly.
6. Run the four quality gates of the acceptance criteria.

## Acceptance criteria
- `git grep -n '0\.4\.0' -- src tests examples assets/typst docs/guide` prints exactly one line,
  `tests/vendor_smoke.rs:318`.
- `git grep -c '@writer/doc:0\.5\.0' -- tests/snapshots | wc -l` prints `22`, and
  `tests/snapshots/typst_frontend__unsupported-diagnostics.snap` names `embedded version 0.5.0`.
- Every line this task changed differs from its old version in the version number alone. This
  prints nothing:
  ```sh
  git diff HEAD | diff target/t97-before.diff - | grep '^[<>] [-+][^-+]' | grep -v '0\.[45]\.0'
  ```
  (Remove `target/t97-before.diff` afterwards.)
- `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, `docs/release/**`, `docs/research/**`,
  `docs/spec/**`, `docs/tasks/**`, `PLAN.md` and `vendor/**` are not changed by this task.
- `cargo run -- eject typst-package target/t97-packages` (any empty directory; `target/` is
  ignored by git) writes `target/t97-packages/writer/doc/0.5.0/typst.toml` whose `version` is
  `0.5.0` (manual check; remove the directory afterwards).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Any change to what the package does: `design.typ`, `lib.typ`, the body and the comments of
  `template.typ` and every new test belong to T98.
- The guide's prose (T101). Here only the four import lines of `docs/guide/customization.md` and
  `docs/guide/typst.md` change.
- The crate's own version (`Cargo.toml` is `0.3.1` and stays), the release notes and
  `CHANGELOG.md`.
- Every other `0.4.0` in the repository: `cetz 0.4.0` in `tests/vendor_smoke.rs` and
  `docs/spec/vendored-assets.md`, and the contract notes and research notes in `docs/spec/`,
  `docs/research/`, `docs/tasks/` and `PLAN.md`, which name the earlier version on purpose.
- Keeping `@writer/doc:0.4.0` importable beside `0.5.0`: writer embeds one package version; a
  document or template that imports another gets the existing compile error.

## Hints (verified)
- The complete list of occurrences at commit `5890724f` (`git grep -n '0\.4\.0'`, 2026-11-21).
  Source and assets: `src/typst/mod.rs:5`; `src/typst/includes.rs:566,583`;
  `assets/typst/writer-doc/typst.toml:3`; `assets/typst/writer-doc/template.typ:29`;
  `docs/guide/customization.md:238,270`; `docs/guide/typst.md:21,71`;
  `examples/typst/document.typ:1`. Fixtures: `tests/fixtures/serve/native.typ:1`,
  `tests/fixtures/serve/template.typ:2`,
  `tests/fixtures/typ/{include-notebook-missing,include-notebook,minimal,subset,template}.typ:1`.
  Tests: `tests/cli_serve.rs:1135,1209`; `tests/diagnostic_payloads.rs:49,65`;
  `tests/pdf_pipeline.rs:249`; `tests/typst_emit.rs:213,863,929`; `tests/typst_export.rs:26,156`;
  `tests/typst_frontend.rs:520,795,1374`; `tests/typst_includes.rs:282,665,860,942,1124,1378`;
  `tests/typst_package.rs:20`; `tests/typst_world.rs:326,337`. (T96 added an assertion to
  `tests/typst_export.rs`, so line 156 may have moved by a few lines.)
- The 23 snapshot files: `tests/snapshots/typst_emit__{attribute-recovery,attributes,callouts,
  citations-notes,citations,code,directives,figures,footnotes,include-chain,math,minimal,
  numbering,pagebreaks,pandoc-directives,presenter-deck,raw,references,
  sample_build_is_deterministic_and_does_not_mutate_the_ir,tables}.snap`,
  `tests/snapshots/typst_frontend__unsupported-diagnostics.snap`,
  `tests/snapshots/typst_includes__{markdown_fragment,notebook_module}.snap`. Twenty-two hold the
  import string (one line each); `typst_frontend__unsupported-diagnostics.snap` holds the message
  `imported @writer/doc:9.9.9 differs from embedded version 0.4.0 (col 2)` instead.
- The package's files are embedded from `assets/typst/writer-doc/` by `rust-embed`
  (`src/vendor.rs`) and written under `writer/doc/<WRITER_DOC_VERSION>/` (`src/typst/package.rs`),
  so the constant and `typst.toml` must agree: Typst rejects a package whose manifest version
  differs from the imported one.
- The reference code of T98 to T100 (R12 §5) names the version nowhere, so its diffs apply after
  this task as they are.
- `target/` is ignored by git, which is why step 1's copy goes there.
- The previous bump, `0.3.0` to `0.4.0`, is in the history of PR #88; its diff shows the same
  files.
- The crate's dependencies are vendored (`.cargo/config.toml`), so every command works offline.
