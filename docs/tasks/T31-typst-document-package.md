# T31 — Bundled Typst document package

| | |
|---|---|
| Milestone | M3 |
| Size | M |
| Depends on | T03 |
| Parallel-safe with | T30, T40, T50 |
| Files you own | `assets/typst/writer-doc/{typst.toml,lib.typ,callout.typ,numbering.typ,cells.typ,oracle.typ,template.typ}` (new), `src/typst/package.rs` (add `eject` and `install_dir`), `tests/typst_package.rs` (new) |

## Goal

The embedded compiler can import `@writer/doc:0.1.0` offline to typeset a
document with metadata, localized headings/callouts, numbering, listings,
cells and bibliography. Package ejection writes the writer and mitex trees
for later CLI wiring and standalone Typst use.

## Read first

- `PLAN.md` §3–§4, §5 T31, §6 and the ownership map.
- `docs/spec/typst-dialect.md` §3, §4 (authoritative), §6.2, §6.5, §10,
  Fixtures, Open points and Deviations.
- `docs/spec/references.md` §2–§3, §4.5, §7 (numbering/oracle/locales).
- `docs/spec/vendored-assets.md` §4–§5.3; `src/vendor.rs` and the existing
  package loader in `src/typst/package.rs`.
- `src/typst/{world,compile}.rs`, `src/resolve/l10n.rs`, `assets/l10n/*.toml`
  and `tests/typst_world.rs` (compiler test setup).

## API to implement

```rust
pub fn eject(dir: &std::path::Path) -> crate::error::Result<Vec<std::path::PathBuf>>;
pub fn install_dir() -> crate::error::Result<std::path::PathBuf>;
```

Typst exports and arguments follow typst-dialect.md §3–§4 verbatim. Preserve
the embedded package version and the existing loader, World and public IR.
`include-notebook` gets its declared signature and a clear T53 placeholder
panic; its implementation belongs to T53.

## Steps

1. Add the seven package files. Use only Typst built-ins. Reproduce the
   normative numbering/oracle rules, including caption-less figure labels.
2. Implement layout/config fallback, title/author/abstract/TOC/bibliography
   order, language/locale handling, explicit custom titles and timestamp rules.
3. Implement callout terms/colours, listings with numbering/highlights, cells,
   and the delegating starter template. Mirror the shipped localization words.
4. Add package ejection through the existing vendor API. Preflight both
   version destinations before writing; existing files/directories or links
   at either version destination fail with `Error::Config` and no writes.
   `install_dir` returns Typst's system data package root without writing it.
5. Compile inline documents with embedded fonts and downloads disabled.
   Check visible text and oracle metadata; never skip when assets are missing.

## Acceptance criteria

- Package manifest/version and all seven files are available through `vendor`.
- `tests/typst_package.rs` compiles an inline rich document and the starter
  template, covering every §3 argument kind, five callouts, custom titles,
  raw/listings/cells, bibliography path values, date and config defaults.
- Compiler-backed numbering cases cover document/section figures, all three
  equation policies, unnumbered and abstract headings, section fallback when
  headings are off, footnotes, and oracle disabled by default.
- Caption-less numbered figures print their localized label and number above
  tables/listings and below images. Section equation references omit brackets.
- Locale dictionary tests compare all shipped writer words and Bibliography
  with `resolve::term`, rejecting extra embedded locale entries.
- Ejection tests compare every output byte with `vendor`, preserve unrelated
  files and reject either existing version destination before writing either
  package. Installation-path lookup follows `FsPackages::system_data()`.
- Formatting, all-target/all-feature Clippy, debug/release tests, rustdoc with
  warnings denied and the basic HTML example build pass, locked and offline.

## Out of scope

IR emission (T30), PDF pipeline/CLI (T32), Typst frontend (T50), notebook
include implementation (T53), export (T34), eject CLI wiring (T72), dependency
changes, and new public IR/config shapes.

## Coordinator notes

- This brief records the existing spec because the original task briefs stop
  before T31. It adds no dependency or production API beyond the frozen plan.
- T31 starts from merged T16 (`ed9d4d2`) and is independent of the T19 follow-up
  PR #11. Keep its PR based on `main`.
- Listing highlights retain native whole-block syntax highlighting and use
  decimal comparisons for the shared `u64` line-range grammar, avoiding
  Typst's narrower integer parser for range bounds.
