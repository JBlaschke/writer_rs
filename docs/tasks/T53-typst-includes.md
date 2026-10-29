# T53 — Native Typst includes of Markdown and saved notebooks

Implemented from T52 / PR #34 on `codex/typst-includes`, validated on
2026-10-29. Dependencies T18, T1B, T30 and T52 are implemented.

## Result

Native Typst can include Markdown and saved notebooks through the build
session's existing World. Structured exports splice their parsed IR, and the
package's `include-notebook` helper selects saved notebook cells by index, id
and tags. Notebook code is never executed. Public provider, notebook, compiler,
IR and configuration APIs are unchanged.

The provider emits unresolved fragments. The including native document owns
references, citations, numbering, title, bibliography and layout. Each child
uses existing local parsing rules, while its document metadata is ignored with
a located warning. Only automatic child TOCs are suppressed; explicit outline
blocks survive. Ignored abstracts cannot reserve imports, footnotes or assets.

Canonical sources must remain within the canonical project root and have a
UTF-8 relative path. Footnote prefixes use the first eight lowercase BLAKE3 hex
digits of that forward-slash relative path plus `-`, so aliases and relocated
asset-free trees keep their native note identities. Existing file-asset IDs
retain their established canonical absolute-path identity.

## Notebook module and selection

One notebook parse supplies normal bodies, inputs and saved outputs. Three
shared `emit_split` calls build the fixed `writer-cells` content fields, with
imports deduplicated in first-use order. Empty eligible cells remain entries;
removed and Unknown cells do not. Original indices, ids, tags and raw user
imports survive. Typst string escaping handles quotes, control characters and
backslashes without changing source bytes.

Selection follows T18: refs form a union in document order, duplicates do not
repeat cells, tags intersect that selection, and notebook removal wins.
`auto`/`false` visibility keeps notebook policy; `true` hides the requested code
variant. Missing refs remain structural findings and native callsite panics.
Valid negative or usize-overflow indices are invalid arguments. The pinned
lexer parses oversized decimal integers as floats, which are also invalid
selector arguments; overflowing nondecimal literals keep parser syntax errors.

The fixed eager native module has three documented differences from structural
selection:

- Selecting a later footnote reference while excluding its original owner can
  leave a native missing label; structural selection rebinds the definition.
- Failing raw/native expressions in an unselected cell can fail module import.
- For malformed duplicate ids whose first original cell is removed or Unknown,
  the native module can find only the first retained entry. T18 still selects
  the first original entry before checking eligibility.

These cases are covered directly and do not alter T18 or the module shape.
T53 completes the unreleased initial `@writer/doc:0.1.0` helper. Repository tags
and releases were empty when checked on 2026-10-29. Later published package
changes require the existing package-version bump policy.

## Dependencies, errors and live preview

The World still returns original bytes from `file`/`read`/`json`; synthesized
sources and failures are cached until reset. Markdown and notebook extensions
follow `detect_kind`, including `.markdown` and case-insensitive forms. The
T52 native-main exception preserves original source under format overrides.

Successful synthesis inventories transitive includes, file assets and missing
image attempts. Failed parsing retains declared descendants and located missing
paths for watch recovery, including native computed include paths unavailable
to the structural scanner. Refused out-of-root sources are not recursively
scanned. Existing missing-ancestor watch planning is reused.

Generated scaffold lines have no source-map attribution. Body and real user
import lines retain their original file, line and notebook cell. Mapped
`SourceSpan.file` takes precedence over the module owner's path in compiler
findings. Cycle/depth classification is restricted to parser errors with the
exact established prefixes, including recognized notebook location wrappers.

In mixed builds, a missing image can fail an IR export while native PDF emits
its existing visible fallback. Identical `missing-asset` findings merge once at
the strongest severity, preserving first position and exact message/path/full
span identity. Distinct files, lines and cells stay distinct; native-only
warnings retain their severity. Per-target success and last-good preview pages
remain independent.

## Validation

Runtime checks pass:

- 22 integration cases, six provider units and two diagnostic-merge units.
- Existing structured frontend/package, World, native source, round-trip,
  physical export, Markdown include and notebook snapshot suites.
- Two real HTTP/SSE watch cases: computed Markdown includes recover a nested
  missing source and image; selected notebook plot edits update both views,
  and a later native cell error retains the last-good page with its true
  notebook file/cell/line until repair.

The additive include fixture retains its original 11 IR blocks, metadata,
source and asset records exactly, appending six Markdown/notebook blocks and
only the expected feature/include inventory and two metadata warnings. Both
updated frontend goldens and both new provider text/map goldens received an
independent review. Snapshot cleanup changes typed source paths only; literal
code, math and text remain exact.

Full acceptance passed: **1,634 debug / 1,633 release tests**, with three
intentional ignores in each mode. The configured formatter, denied-warning
Clippy across all targets/features, and denied-warning rustdoc also passed.
Measurements are recorded in `docs/dev/build-times.md`.

T60/T61 LaTeX export, T70 citation resolution, T71 checks, T72 eject wiring and
T77 Chrome print stubs remain explicit. The next milestone is T60; its source
is staged independently while this branch's CI runs.
