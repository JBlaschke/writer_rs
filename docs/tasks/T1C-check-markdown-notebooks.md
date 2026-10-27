# T1C — Markdown/notebook `writer check`

Frozen implementation brief, coordinator, 2026-10-27.

| | |
|---|---|
| Milestone / size | M1 / S |
| Depends on | T16, T1B; implementation also consumes completed T18 |
| Primary ownership | `src/check.rs` (replace stub), `src/cli/check.rs` (lift/adapt) |
| Tests | `tests/cli_check.rs` (new, lift/adapt), check unit tests, diagnostic snapshots of existing MD/notebook fixtures |
| Additive integration | Remove only the `check` case from `tests/cli_build.rs::command_stubs_exit_2`; coordinator writes task/status/spec clarifications |

## Goal and binding sources

`writer check INPUT` parses Markdown/notebooks and their includes, resolves
references leniently, collects all frontend/resolver diagnostics, optionally
adds presenter compatibility warnings, and reports text or JSON without
rendering, executing notebooks, or writing document outputs.

Binding: PLAN §3.2, §5 T1C, §6–7; `docs/spec/markdown-dialect.md` §9–11;
`docs/spec/notebook-mapping.md` §8; `docs/spec/cli.md` check/global flags;
`docs/spec/references.md` §4.7, §8; `docs/spec/ir.md` §5. Prefer binding
specs over the older presenter inventory recommendations.

## Existing APIs and recommended implementation seam

The clap tree is already complete and must remain unchanged:

```rust
cli::check::run(&CheckArgs, &GlobalArgs) -> anyhow::Result<ExitCode>
CheckArgs { input, target, deny_warnings, format, compat }
CheckFormat::{Text, Json}; Compatibility::Presenter
pipeline::BuildOptions::new(input)
pipeline::Mode::Check
pipeline::prepare(&BuildOptions) -> Result<Prepared, BuildFailure>
Config::targets_for(SourceKind) -> Vec<Target>
Diagnostics::{has_errors, has_warnings, iter, extend}
Error::{path, span, message}
```

Use `pipeline::prepare` with `mode = Check` and the global config path. It
already applies project/front-matter/CLI precedence and resolves once with
`lenient = true`. Missing assets remain warnings; do not call build/render,
theme resolution, a compiler World, output planning or output writers.

Frozen new check-module API (coordinator, 2026-10-27):

```rust
#[derive(Default)]
pub struct CheckOptions {
    pub compat_presenter: bool,
}
pub fn run(opts: &BuildOptions, check: &CheckOptions)
    -> std::result::Result<Diagnostics, pipeline::BuildFailure>;
pub fn error_diagnostic(error: &Error, input: &Path) -> Diagnostic;
```

These APIs are public. Clone the incoming BuildOptions and force Mode::Check.
Preserve `BuildFailure.diagnostics`
when preparation fails; append the converted terminal error. Use
`Error::span()`/`message()` rather than matching only `Parse.line`: notebook
cell failures encode `cell N, line M:` when `line` is `None`, and YAML spans
are available through the shared error helpers. Prefer the error's own file,
then `span.file`, then the input. Never normalize `..` before filesystem
resolution (T1B directory-symlink regression).

Keep diagnostics deterministic: lifted stable position sort, root first then
`Document.includes` first-use order, by cell/line, with unlocated findings
first and tied frontend findings before added lints. Do not parse includes
again just to repeat their existing frontend/resolver diagnostics.

## Required behavior

- Pass through all Markdown §10 and notebook §8 diagnostics, with their
  existing levels, file/cell positions and messages. Do not duplicate frontend
  lints such as `notebook-not-run`, `invalid-directive`, author-list warnings,
  footnotes, output validation or missing assets.
- Include resolver diagnostics beyond the dialect table: `label-reserved`,
  `anchor-collision`, numbering/lang/link warnings, etc. Unknown targets are
  `unresolved-reference` warnings in check; **never invent
  `unknown-citation-key`** (references §8 supersedes stale CLI list wording).
- Text: existing relative/slashed path display, stderr only, no stdout;
  show Info by default as informational notes. Generic `report_diagnostics`
  currently requires `-v` for Info, so adapt report flags locally or add a
  narrowly approved reporting seam. Keep build/serve visibility unchanged.
- JSON: exactly an array of `{level, code, message, file, line, cell}` on
  stdout, no text reports; lowercase severity, null unknown locations, cell
  indices zero-based, lines one-based. Writer specifies **`file`**, unlike
  presenter's `path`. Use existing display-path semantics, including Windows.
- Exit 0 without errors; exit 1 with errors or (`--deny-warnings` and at
  least one Warning). Info alone never fails. JSON always includes every
  diagnostic, also with `-q`.
- Lift presenter's quiet/deny rule: ordinary `-q` hides text warnings;
  `-q --deny-warnings` still prints warnings explaining the failure.
  Info remains hidden in that special combination.
- `--compat presenter` adds source-based warnings for includes outside their
  own separator-delimited slide, author/authors list front matter, and colon
  pre-pass rewrites. Promote/recompute `directive-colon-rewritten` once;
  never keep an Info duplicate. Ordinary `presenter-separator` remains Info
  unless explicitly changed by the coordinator. Do not flag writer-only
  figures, captions, references, raw fences, leaves or inline notes.
- Compatibility checks cover included source files with their own paths and
  notebook cell spans when relevant. Skip code/output/raw non-Markdown,
  ignored notes/comments and front-matter contents as ordinary source. Do
  not produce duplicate findings when one file is included twice.

## Lift sources and limits

- `../presenter_rs/src/cli/check.rs`: run/output flow, quiet/deny policy,
  JSON DTO, exit-code and JSON unit tests. Adapt one input, `target`, `compat`,
  writer's field names and Info severity. Its `!diagnostics.is_empty()` deny
  condition is incorrect for writer; use `has_warnings()`.
- `../presenter_rs/src/check.rs`: `error_diagnostic`, stable
  `sort_by_position`, appropriate source-location utilities/tests. Replace
  manual Parse spans with shared Error helpers; `FrontMatter` must be
  `parse-error` per writer's notebook/dialect tables, not presenter's
  `front-matter-error`.
- `../presenter_rs/tests/cli_check.rs`: subprocess helpers, text/JSON/quiet/
  deny/error/include-path tests. Drop multi-input, slides, theme/reveal,
  layout/separator pitfalls and `notebook-no-outputs` logic. Writer frontend
  already emits `notebook-not-run` with the correct kept-cell semantics.
- Presenter `src/markdown/include.rs::{include_slide, find_includes}` can
  inform compatibility include-position checks; do not restore their build
  restrictions in writer's parser.
- `tests/{markdown_snapshots,notebook_snapshots}.rs` demonstrate deterministic
  globbing/path redaction; `includes.md` intentionally fails with a cycle.

No dependencies, IR/config fields, public Fragment fields, pipeline shape,
or clap arguments need changing. Do not lift presenter's whole check module:
most of it is slide splitting, layout and theme lints irrelevant to writer.

## Acceptance mapping

1. CLI clean Markdown/notebook → 0; warnings → 0 / deny → 1; errors → 1;
   Info-only → 0 even with deny. Test mixed severities, quiet, and JSON quiet.
   Explicit `--target` exits 2 naming T71; `.typ` exits 2 naming T50/T71.
2. JSON schema/types/nulls and stdout/stderr discipline tested explicitly;
   text includes source paths/lines/cells and no output artifacts are made.
3. Missing assets stay warnings; unresolved refs warn; duplicate/invalid/
   ambiguous labels remain errors. Cross-file references resolve globally.
4. Check every top-level `tests/fixtures/md/*.md` and
   `tests/fixtures/ipynb/*.ipynb`; snapshot stable code/severity/location
   tuples (or normalized JSON), with explicit per-fixture code assertions.
   Include new `include-chain.md` / `includes.ipynb`, not just T18's seven.
5. Fatal fixture include cycle → structured `parse-error`; malformed JSON,
   YAML/front-matter shapes, missing include, unknown extension, config/IO
   failures have deterministic converted findings and exit behavior.
6. Notebook error regression retains cell N/line M for malformed include
   paths and first-cell YAML; included Markdown YAML retains the child's
   file/line. Repeated includes do not duplicate frontend findings.
7. Compatibility matrix covers author scalar/list/authors, valid/invalid
   include positions at file boundaries, nested/included files, colon-trap
   rewrite promotion, excluded code/comments, no false writer-only warnings.
8. Existing stub assertion is removed; `writer check examples/basic/document.md`
   and mixed/notebook examples work. Root runs serial fmt/Clippy/debug+
   release/rustdoc checks and opens one milestone PR against main, then
   continues without waiting for CI or dependency merges.

## Coordinator decisions (2026-10-27)

1. **Frozen scope:** CheckOptions contains only `compat_presenter`, derives
   Default, and public run uses the exact signature above. Explicit
   `--target` remains the milestone stub, exit 2 naming T71. `.typ` remains
   a stub, exit 2 naming T50/T71. Ordinary check runs only frontend + lenient
   resolve. No theme/renderer/World; per-target `raw-skipped`, candidate
   fallback `output-unsupported`, math/unsupported/template checks remain
   T71/T33. CLI signatures and config/IR shapes stay unchanged.
2. **Frozen failure policy:** input/config/unknown
   extension failures become JSON/text error findings and exit 1; only clap
   syntax and the explicit stubs above exit 2. `parse-error` covers
   Parse/FrontMatter/JSON; retain ordinary `config-error`, `io-error`, and
   generic error mappings. Generic CLI helpers otherwise use exit 2
   for usage/config errors, so check must not accidentally fall through them.
3. **Compatibility include boundaries:** file/cell start and end count as
   boundaries and blank lines are ignored. Only top-level real `---`
   thematic breaks count (not YAML/code/setext/list text); the include must
   be the sole significant block between these boundaries. Do not import
   presenter splitter IR or flag already-invalid nested include directives.
4. **Compatibility provenance:** `Prepared`/`Document` retain paths but not
   per-file raw text/effective parse config. Use the minimal approach:
   reread each distinct source only for compatibility checks, promote actual
   frontend colon-rewrite findings (already correctly configured), and scan
   FM/include positions on source. No public IR change.
5. **Notebook compatibility extent:** Markdown/raw-Markdown source
   syntax only; author-list compatibility applies to the actual first Markdown
   front matter, not arbitrary notebook JSON metadata or ignored later headers.
   These optional source checks inspect all such cells as written, including
   removed cells; normal frontend findings retain their kept-cell semantics.
   Do not infer retention from expanded IR, which loses the parent span of an
   include-only cell, or reconstruct child configuration for these checks.
6. **Info text:** reuse the established `info:` severity display, as an
   informational note. Keep JSON `level: "info"` and deny behavior unchanged.

Starts from checked T1B (`fcfa42f`) while PRs #17/#18 run CI. The PR targets
`main`; list those dependencies and merge order if they remain open.
The final checks also include T1B's CI repair (`9ec2dc4`): original include
path bytes and portable notebook asset-origin snapshots.


## Acceptance record (2026-10-27)

All eight acceptance groups are covered by eleven library tests, four CLI
unit tests, sixteen CLI subprocess tests and the 26 reviewed MD/notebook
snapshots. Compatibility regressions include ignored later known-key notebook
headers, literal raw Markdown, real top-level author keys and block scalar
lookalikes. The clean-example test inventories files before and after check.

Formatting, Clippy (all targets/features, warnings denied), warnings-denied
rustdoc, 1,064 debug and 1,063 release tests pass. Manual JSON checks of the
basic/notebook/mixed examples return `[]`; basic HTML builds at 687.6 kB.
Per-target and Typst checking are the explicitly deferred T71/T50 scope.
