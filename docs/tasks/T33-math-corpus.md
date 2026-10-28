# T33 — LaTeX math PDF corpus and check pre-flight

Preparation only, 2026-10-28. No implementation, tracked edits, Cargo, Git,
formatter, Typst compiler, or corpus regeneration has run for this brief.
Binding contracts below come from PLAN T33 and math.md; proposed narrow seams
and the two check-policy choices at the end need coordinator freeze before
fan-out. T42 finishes first; T43 follows this small milestone.

| | |
|---|---|
| Milestone / size | M3 / S |
| Dependencies | T1A, T1C, T32, T42; all available after the current T42 branch |
| Primary additive files | src/check.rs, src/cli/check.rs, math corpus tests; optional small shared preflight helper in src/math/latex_source.rs |
| Existing emitter | src/typst/emit.rs already performs the required mitex preflight and fallback; preserve it |
| Shared fixture ownership | Add only latex.*.pdf to tests/fixtures/math/expectations.toml; preserve T42 docx and Typst rows |
| Binding sources | PLAN §3.2/T33/§7; math.md §2, §3.6, §6, §9–11, Fixtures/Deviations; cli.md check/strictness; typst-dialect.md §2.4/§5.2 |

## Goal and boundaries

Exercise the complete existing LaTeX corpus through actual emitter, embedded
mitex package and PDF compilation. Add the same cheap mitex parser preflight to
check, with real source locations, without output or a compiler session. Verify
prepared-source round trips through T50 using existing library APIs.

Do not implement T34 Typst export, T52 native .typ PDF dispatch, T53 includes,
T60 LaTeX output/package scans, T70 bibliography, T71's full target lints or
T77 Chrome printing. Existing unsupported explicit non-PDF check targets remain
T71 unless root expressly expands the boundary. No new dependencies, flags,
public modules, IR/config schema, converter/evaluator or external runtime tool.
DOCX math continues through T42's native MathML→OMML route, not T51 conversion.

## Existing public APIs to retain exactly

```rust
// src/check.rs
pub struct CheckOptions { pub compat_presenter: bool }
pub fn run(
    opts: &BuildOptions, check: &CheckOptions,
) -> std::result::Result<Diagnostics, BuildFailure>;
pub fn error_diagnostic(error: &Error, input: &Path) -> Diagnostic;

// src/math/latex_source.rs
pub fn prepare(src: &str) -> (String, bool);
pub fn macros_from_config(
    opts: &serde_json::Map<String, serde_json::Value>,
) -> BTreeMap<String, String>;
pub fn macros_as_newcommands(macros: &BTreeMap<String, String>) -> String;
pub fn for_typst(
    src: &str, macros: &BTreeMap<String, String>,
) -> (String, bool);

// src/math/latex_mathml.rs; reuse these, never a second math-core setup.
pub fn to_mathml(
    source: &str, display: bool, macros: &BTreeMap<String, String>,
) -> Result<String, MathError>;
pub fn macro_diagnostics(
    macros: &BTreeMap<String, String>,
) -> Vec<Diagnostic>;
```

The module comments identify the existing definitions; the normal Result in
latex_mathml is the standard two-parameter result. CheckOptions is
Copy today; do not add a Vec/target field and break callers' struct literals.
BuildOptions already has cli.targets: Option<Vec<Target>>. Config::apply_cli
sets both input-kind target lists from that override. OutputTarget already has
From<OutputTarget> for Target. Explicit check PDF selection can therefore flow
through opts.cli.targets with no public API change. Effective default targets
come from prepared.config.build.targets / typst_targets by input kind.

Existing pipeline interfaces remain the T32 contracts: prepare, WorldSlot,
pdf::{dump,build,page_count}, public load_typst_main, emit::{emit,EmitOptions},
compile::{compile_document,compile_pdf,PdfOptions,Compiled,Pdf}. Reuse them for
real PDF and round-trip tests; never expose a test-only production entry point.
A World/session belongs only to build/compile tests, not to check preflight.

## Current implementation and exact missing work

src/typst/emit.rs::Emitter::math already:

1. Calls latex_source::for_typst(source, configured_macros).
2. Emits math-tag-ignored once for that equation when preparation strips tags.
3. Calls mitex::convert_math(&prepared, None); ignores its successful output.
4. On Ok writes #mi or #mitex using math_literal, numbering and label policy.
5. On Err trims outer whitespace and the leading `error: `, emits one
   math-conversion-failed, then writes native Typst math containing raw(original
   IR source). One failed equation remains printable and cannot stop the PDF.

The cheap parser is already in the normal dependency graph. Do not replace
its output with emitted Typst; that text requires package scope. T33 primarily
adds corpus evidence and a check integration, rather than recreating T30/T32.

src/check.rs::run currently prepares in Mode::Check, keeps frontend/resolve
findings, optionally adds presenter compatibility, sorts by source position and
returns. It has no math validation at all. src/cli/check.rs currently rejects
EVERY args.target with T71. CheckArgs already parses --target LIST and JSON;
no parser/flag changes are needed. Existing CLI tests assert html and html,docx
remain T71 stubs; replace/add only the PDF expectations for T33.

## Narrow private integration seams

Recommended implementation, for root freeze:

- Keep check::run and CheckOptions unchanged. CLI allows an explicit list only
  when every requested target is Pdf, forwarding its deduplicated conversion
  through opts.cli.targets. Any other explicit target/list keeps the T71 stub;
  do not silently ignore html in an explicit pdf,html list.
- Add private math traversal/reporting helpers in check.rs. Carry the enclosing
  SourceSpan and effective path at every equation. Block::Math owns a span;
  Inline::Math inherits its containing block's span. SourceSpan.file overrides
  the root path; cell stays intact. Visit abstract/body, captions/table cells,
  lists/divs/callouts, nested inline formatting, footnotes and notebook output.
- Existing ir::walk_blocks/walk_inlines are available but globally visit EVERY
  saved-output candidate and walk_inlines does not supply its enclosing block
  span. Do not attach all inline errors to line1, duplicate footnote visits, or
  use the global walker blindly for PDF-specific output selection. Emitter
  cells honor show_output and output.pick("typst"); preserve that selection in
  the PDF preflight traversal. A private context-carrying collector is enough;
  no new public IR walker or renderer dependency is required.
- Baseline check math syntax uses latex_source::prepare(source).0 and configured
  macros separately through latex_mathml::to_mathml, for every LaTeX equation
  regardless of HTML math route. It must still catch fixture19, which mitex
  accepts. Raise macro_diagnostics once per checked document using math-core,
  at the first LaTeX equation like existing emitters; do not repeat per equation.
- Only selected Pdf runs mitex on for_typst(source, macros).0. Typst-dialect
  equations are not sent to mitex, math-core, or T51. Native Typst per-target
  lint/compile coverage stays T71/T52; no compiler runs inside check.
- To prevent message/parser drift, root may add one crate-private helper in the
  existing latex_source module:
  `preflight_typst(prepared: &str) -> std::result::Result<(), String>`.
  It delegates to mitex::convert_math(..., None), discards successful text and
  normalizes the error exactly as the current emitter. Both caller paths still
  use for_typst themselves once. Update emitter to call that helper only if
  this seam is chosen; otherwise keep a small matching call in check and test
  diagnostic equality. No new public function, MathError variant or module.
- Preserve check's existing stable sort, parser failure diagnostics, compatibility
  promotion and no-write guarantee. Apply existing centralized strict escalation
  to newly collected findings when relevant; do not alter diagnostic codes.
  Preparation remains lenient for unresolved references.

## Diagnostics and exits

- mitex Err: Warning math-conversion-failed, exactly
  `pdf: cannot convert equation (<normalized reason>); used the source text`.
  Include the equation's enclosing file/span/cell, once per rejected equation.
  It is a warning in ordinary builds/check; CLI build --strict turns it into
  an error using existing pipeline policy. Check has no --strict flag today.
- math-core Syntax: Warning math-syntax, `invalid LaTeX math: <message> at bytes
  a..b of the equation`, using MathError::Syntax.range when available. Add the
  existing KaTeX caveat where required by math.md §9/HTML route. Byte positions
  are relative to prepared equation text, not invented source-file columns.
- Tag preparation: math-tag-ignored stays Warning, never strict escalation;
  its existing target-qualified message names pdf when checking that route.
- Macro findings use existing math-macro-invalid text and once-only behavior.
- A bad equation may legitimately yield both math-syntax and the PDF-specific
  conversion warning; do not deduplicate distinct codes or fail to diagnose19.
  No loss/cascade into parse-error or generic compile-error for cheap preflight.
- --deny-warnings changes check exit0→1. Text findings stay on stderr; --format
  json remains one complete stdout array with flat level/code/message/file/line/
  cell, including failures. Quiet+deny still prints warnings that fail the run.
- check never writes dist/out/assets or initializes fonts/packages/World. PDF
  hard compiler errors preserve T32 no-write/last-good rules. Strict escalation
  occurs after target work today; do not incorrectly assert that a warning
  escalated after a successful PDF build prevented that output from being written.

## Fixture changes and complete acceptance

Add `pdf` to every one of the 37 existing LaTeX rows in expectations.toml:

| Prefixes | pdf |
|---|---|
| 18-unknown-command, 32-ce, 37-sideset | preflight-error |
| Every other LaTeX fixture, including 19-unbalanced-brace | compile |

There are 34 compile entries and 3 preflight-error entries. Preserve all existing
latex/typst docx keys and classifications, fixture bodies/modes, macros.toml,
Typst oracles and T42 snapshots. No pdf keys are required on Typst rows by this
milestone. Never change an expected failure merely to obtain a green corpus.

1. **Actual corpus PDF.** Read all 37 fixtures with the existing mode convention.
   Run per-fixture preparation/preflight with macros.toml only for31, assert the
   classification above, then emit the 34 compile entries into ONE document and
   compile ONE real PDF with embedded fonts, allow_packages=false and a fixed
   timestamp. Verify header, nonzero exact page count against Pdf.pages,
   pipeline page_count agreement and byte identity across repeated fresh and
   retained-World builds. Do not require one equation per page or guess a count.
2. **Macro isolation.** Fixture 17 defines `\half` locally while 31 uses configured
   `\half`. Do not accidentally apply31's map to every unrelated corpus fixture
   or change fixture17 to hide a macro collision. A combined-document helper may
   materialize31's own for_typst prefix in that equation's IR source while using
   empty global corpus macros; separately exercise actual configured macros31
   through the real emitter/pipeline. Explain this test-only composition.
3. **Every parser rejection.** Emit18/32/37 individually in inline/display and
   labelled/numbered forms. Assert native `$#raw(...)$` / `$ #raw(...) $` source,
   the original unprepared source in the escaped literal, retained label/number
   policy, exactly one PDF conversion warning, and successful real PDF output.
   Compile the same bad text directly through #mi/#mitex WITHOUT preflight and
   require Error::Compile from the vendored plugin. Keep paths/hints through
   existing compiler/source-map helpers; do not implement another plugin wrapper.
4. **Round trips.** Use existing parse/resolve→emit→typst::frontend::parse APIs;
   T34 CLI export stays pending. Compile-positive LaTeX equations keep dialect
   Latex and source==latex_source::prepare(original).0. Fixture27 strips tags,
   28 normalizes top-level align;31 strips exactly the effective leading macro
   block. Backticks choose escaped strings instead of multi-backtick raw, and
   both literal forms read back byte-identically. Preflight-error #raw fallback
   intentionally reads as native Typst math; assert that documented distinction.
5. **Check smoke and equivalence.** Default/config-selected PDF, explicit
   --target pdf, duplicate pdf and unsupported mixed/non-PDF target stubs.
   Prepared macros/tags/env text must match emitter preflight exactly.19 gives
   math-syntax even when PDF preflight succeeds;18/32/37 give conversion findings;
   valid31 resolves configured macros with no false unknown-command warning.
   Check good docs give exit0; deny warnings gives1; JSON/text/quiet and real
   include/notebook source provenance are verified, with no output directories.
6. **Traversal and stability.** Nested inline math, caption/table/list/callout,
   abstract, footnote and selected notebook candidate coverage; no duplicate
   visits or warnings. Native Typst equations are not misdiagnosed as LaTeX.
   Invalid macro diagnostics are once-only; no macro definition leaks between
   successive independent equations or retained-world rebuilds. Removed/hidden
   output candidates do not create PDF-only failures that its emitter never sees.
7. **Build policy.** Ordinary PDF prints source fallback and warning, strict
   build records the same code as Error and exits1; math-syntax/tag warnings
   retain their base severity. Real mapped plugin error still fails only PDF
   while HTML succeeds, preserving existing T32 target independence. Labels,
   numbering and references remain the resolver/@writer/doc policies.
8. Run focused corpus/roundtrip/check suites and existing emitter, PDF, frontend,
   CLI check and DOCX regression tests. Root serializes formatter, denied-warning
   Clippy/docs, required debug/release suites and opens a milestone PR. Existing
   three-OS CI must exercise ordinary generated PDFs, with no external Typst,
   Chrome, TeX, pandoc, network or host-font dependency for mandatory tests.

## Pinned mitex API facts (local primary source)

Cargo.toml pins `mitex = "=0.2.4"`; Cargo.lock has that crate plus its existing
parser/spec family. The embedded @preview package is independently pinned0.2.7.
The local primary implementation is
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/mitex-0.2.4/src/lib.rs`:

```rust
pub fn convert_math(input: &str, spec: Option<CommandSpec>)
    -> Result<String, String>;
```

It delegates to converter::convert_inner in LaTeXMode::Math using the normal
macro-expanding parser. None selects the bundled DEFAULT_SPEC. Errors are
strings prefixed with `error: ` from the conversion path. Successful output
(e.g. `frac(a ,b )`) is package-scoped Typst, not standalone source and must not
replace a #mi/#mitex argument. convert_math_no_macro exists only as an internal
comparison helper; it is not the selected production parser. No runtime CLI,
IO or new dependency is needed to preflight. The package/compile is the final
truth, because versions differ and the Rust parser tolerates unbalanced braces.

## Two small coordinator choices before fan-out

1. Adopt the existing cli.targets/default-config target selection path, allowing
   only explicit Pdf lists in T33 while retaining T71 stubs for all other explicit
   lists. This avoids a new public CheckOptions field/API and preserves current
   html/html,docx stub tests. Decide before parallel implementation, not by
   silently shipping partial checking for explicit mixed requests.
2. Freeze check's generic syntax traversal versus target-visible traversal.
   Recommended: PDF preflight must follow exactly what the PDF emitter renders
   (show_output and pick("typst")); baseline math-syntax should cover each source
   LaTeX equation once, independent of HTML mode. If target union selection is
   needed for baseline saved MIME candidates, keep it a private collector policy
   with explicit no-duplicates tests; do not expand public IR visitor APIs.


## Coordinator freeze and installed seams

Public signatures, configuration/IR and module lists stay unchanged. Explicit
CLI lists containing only PDF are deduplicated and forwarded through existing
BuildOptions.cli.targets; other explicit lists retain T71's stub. Configured
default targets continue to select PDF preflight without an explicit flag.

The crate-private latex_source::preflight_typst helper shares the existing
mitex parser and exact error normalization with the emitter. Successful text
is discarded. Source preparation and fallback/label/number bytes are unchanged.

A private identity-based collector checks source-visible LaTeX syntax once.
Notebook saved output is limited to show_output and the union of candidates
picked by effective targets; PDF uses pick("typst"). Equal source strings at
distinct nodes remain distinct. Math diagnostics preserve enclosing file/span/
cell, and inherit no notebook cell when crossing into another explicit file.
Macro diagnostics occur once, with the first source equation's location.

PDF traversal follows actual nonfragment emission: image alt is flattened;
missing references and unresolved citations use placeholders; numbered/page
references render their supplements, while unnumbered links render resolved
text. Resolved citation summaries are not PDF input. Original supplements and
citation prefixes/suffixes retain baseline syntax checks; only actually emitted
math gets PDF preflight. Generated unnumbered-reference text is PDF-only so it
does not duplicate baseline source diagnosis. Tag warnings name the equation's
selected non-LaTeX route; a sole LaTeX target retains tags.

Initial all-target/all-feature compilation and all 19 check/CLI private units
pass. The first public run passed all seven CLI tests and all nine real PDF/
roundtrip/policy tests. Its sole library failure was a test path-shape
assumption about macOS /var and /private/var aliases; the test now compares
canonical file identity while production preserves source provenance.


## Final acceptance

Formatting, all-target/all-feature Clippy and default/feature rustdoc pass.
Complete suites pass 1,507 debug and 1,506 release tests. All 19 check/CLI
private units and 78 focused public/emitter/CLI tests pass. The actual corpus
PDF, all recovery/plugin-failure/roundtrip/policy cases and no-write checking
pass. The existing math check golden now records the reviewed ignored-tag
warning. See build-times.md for timings and complete validation details.
