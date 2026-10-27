# T30 — IR to Typst source with source maps

| | |
|---|---|
| Milestone | M3 |
| Size | L |
| Depends on | T01, T03, T17 |
| Parallel-safe with | T31, T40 |
| Files you own | `src/typst/emit.rs` (replace stub), `src/math/latex_source.rs` (replace stub), `tests/typst_emit.rs` (new) |

## Goal

Emit deterministic Typst source from the shared document IR, preserving
labels, numbering policies, references, assets and source positions. The
emitter supports complete documents, unresolved include fragments and split
export trees. Shared LaTeX preparation serves later math routes too.

## Read first

- `PLAN.md` §3–§4, §5 T30 and §6; `AGENTS.md`.
- `docs/spec/typst-dialect.md` §3, §5 (authoritative), §6.4, §10, Fixtures
  and Deviations.
- `docs/spec/math.md` §2, §3.6, §6 and Fixtures.
- `docs/spec/references.md` §1.2, §3–§6; `docs/spec/ir.md` §1–§3.
- `src/ir.rs`, `src/config.rs`, `src/assets/registry.rs`,
  `src/typst/{sourcemap,world,compile}.rs` and `tests/typst_package.rs`.

## API to implement

The public types and signatures follow typst-dialect.md §5.1 verbatim:
`EmitOptions`, `AssetPaths`, `Emitted`, `IncludeRun`, `emit` and `emit_split`.
Fragments use `EmitOptions.fragment`; they retain imports and omit the show
rule. Source maps use the existing `SourceMap::from_lines` constructor.

Implement all seven shared functions in math.md §3.6: `strip_tags`,
`normalise_env`, `prepare`, `macros_from_config`, `macro_arity`,
`macros_as_newcommands` and `for_typst`. Preserve their frozen signatures.

## Steps

1. Implement shared LaTeX preparation, ordered macro definitions and tests
   using inline strings and maps. Do not wait for T1A's fixture corpus.
2. Implement context-aware markup/string/raw escaping and asset path modes,
   with deterministic export names and collision handling.
3. Emit the file skeleton, metadata and every IR block/inline according to
   §5, including target selection for saved notebook outputs.
4. Preserve resolved and fragment reference/citation semantics, footnote
   labels, definition-term breaks and inline expression boundaries.
5. Produce one span per generated line, using the innermost available block
   span. Implement `emit_split` with shared footnote identity across parts.
6. Test generated source with the embedded compiler, offline packages,
   embedded fonts and fixed dates. Test the sample IR and inline documents.

## Acceptance criteria

- The sample IR produces deterministic, reviewed Typst output without
  changing the input. Singleton author and keyword arrays remain arrays.
- Markup metacharacters, shorthand/comment/list prefixes, strings, raw
  fences and non-identifier code languages preserve literal content.
- All IR block/inline variants emit their specified Typst forms. LaTeX
  equations use prepared macro-prefixed arguments, pre-flight conversion
  and a source-text fallback with diagnostics on conversion errors.
- Virtual, export and source-relative asset paths are deterministic;
  missing placeholders fail. Foreign raw content is skipped and remote
  images retain their linked alt text with a diagnostic.
- Reference punctuation, supplements, page references, citation groups,
  unresolved fragment targets and repeated footnote labels are covered.
- Nested source-map lines retain the innermost span, while scaffolding has
  none. Split parts replace the designated top-level runs with includes.
- Shared math tests cover tags/labels/number suppression, top-level
  environments and ordered macros with argument arity.
- Formatting, all-target/all-feature Clippy, debug/release tests, rustdoc
  with warnings denied and the basic HTML example pass, locked and offline.

## Out of scope

PDF pipeline and `dump --typst` wiring (T32), Markdown corpus emitter
snapshots (T32), Typst frontend (T50), round-trip tests (T52), include
synthesis (T53), export tree writing (T34) and HTML math (T1A). No dependency,
IR/config or module-list changes are needed.

## Coordinator notes

- This brief records the existing frozen T30 contracts; it adds no API or
  dependency. The original task briefs stopped before this task.
- Starts from `main` after T31 merged in PR #12 (`b533020`). T30's three
  dependencies are merged. Keep its PR based on `main` and continue with
  the next ready feature while CI runs.

## Amendments

- Coordinator clarification (2026-10-27), typst-dialect.md §5.6 and §10:
  reserve final footnote and export names globally, advancing deterministic
  suffixes past occupied candidates. Export naming uses only referenced
  assets. Tests cover generated suffixes that collide with source names;
  no public API or dependency changes.
- Source asset mode resolves its explicit base directory before computing
  relative paths (typst-dialect.md §5.7). This metadata lookup handles aliases
  such as `/tmp` without changing pipeline config or diagnostic path spelling.
  Virtual and Export modes remain pure. A symlink regression covers the case.
