# T73 — diagnostic excerpts and compiler hints

Implementation installed on `codex/diagnostics`, based on T72 `772dc5e`.
The full compile/no-run gate passes (92.68 seconds wall). Focused formatter,
payload, actual compiler, CLI and actual browser checks pass. All 1,944
debug tests and 1,943 release tests pass in complete runs. The separate T72
Windows parent-error repair passes focused integration in both modes.

Diagnostics retain the existing `path:line: severity: message` headline and
append the established source gutter and underline. Included `SourceSpan.file`
takes precedence over a generic diagnostic path for both display and source
lookup. Notebook excerpts reuse the existing cell-source reader, including
BOM/CRLF handling. Missing-asset diagnostics retain their reference underline.
Unavailable, missing or blank source falls back to the normal headline.

Only the compiler's frozen `\n  hint: ` continuation encoding is split: hints
follow the excerpt once. Other multiline diagnostic messages remain literal.
The existing Error report uses the extracted private gutter helper, preserving
its layout. Visibility, severity order, color rules and quiet behavior stay
unchanged. Primary compiler failures remain reported once; the richer output
does not reintroduce their suppressed wrapper error.

The implementation changes only `src/cli/report.rs`. Public Error/Diagnostic,
compiler mapping, package traces, source maps, pipeline failure records and
JSON/SSE shapes remain unchanged. Existing overlay warning retention and stage
replacement behavior already meet the contract; this milestone validates those
paths through the existing public/compiler/browser seams.

## Focused runtime evidence

- 20 formatter/report units pass, including six new source/precedence/cell/
  hint/fallback/underline controls and existing Error-report assertions.
- 14 event-payload units pass with the exact existing schemas.
- Four new public payload/compiler cases pass. The actual package assertion
  maps to the outer `#call()` at user line 3 and retains the package-origin
  hint. The generated included notebook error quotes its original cell 1,
  line 3; unmapped generated scaffolding has no invented source excerpt.
- The native CLI case passes in quiet/nonquiet modes, with one primary
  headline and one actual source quotation. Its two marker occurrences are
  the message and source, rather than duplicate primary reports.
- All 20 existing PDF pipeline cases pass. The generated Markdown raw panic
  retains its original total marker count of one, plus one mapped line-3 fence
  excerpt; independent successful HTML output is preserved.
- All 16 original World/compiler cases pass, including package trace location.
- Both actual Chrome overlay/repair checks pass with Chrome required: shared
  parse errors, warning display/clearing, pages-only failure, retained good
  page images and repair. The existing 10-second deadlines stay unchanged.
- Configured formatting, all-target/all-feature denied-warning Clippy and
  all-feature denied-warning rustdoc pass.
- Complete debug: 1,944 passed, no failures, three intentional ignores. All
  watcher cases passed without rerun. Build 0.51s, wall 535.25s, CPU 103.14s.
- Complete release baseline: 1,943 passed, no failures, three intentional
  ignores. Build 11m 04s, wall 805.82s, CPU 6,952.27s. These baseline full
  suites precede the separate T72 Windows parent-error repair. Its subsequent focused
  integration checks also pass: 88 CLI units and 53 public cases in each mode. Measurements are in `../dev/build-times.md`.

No parser snapshot or diagnostic schema expectation was loosened. The private
source adapter does not infer a column from arbitrary diagnostic text. Existing
notebook cell provenance and compiler hint encoding supply the known location;
normal failures to quote source do not create a second diagnostic.

## Parent repair and final source provenance

The complete debug/release suites above ran on T72 feature `772dc5e` plus
this diagnostic implementation. While release ran, native Windows CI exposed
T72's parent-file exit classification and platform-specific package refusal
expectation. Those were repaired separately in `6eaa3c8` on PR #40, which is
this branch's parent. The original public DOCX exit-1 test stays unchanged.

After that repair, all 88 CLI units and 53 public cases pass in both debug
and release. These include the actual formatter, file/tree error boundary,
all scaffold variants, ejected packages/templates/themes and original DOCX
controls, plus the new compiler/payload cases. Final formatting and denied-
warning Clippy/rustdoc pass on the combined source. Earlier full-run counts
are not presented as new full runs after the repair, and repeated cases are
not counted twice. Native Windows CI remains the platform runtime proof.
