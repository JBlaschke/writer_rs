# T32 — PDF pipeline preparation

Frozen implementation brief, coordinator, 2026-10-28. The decisions below
approve the narrow cross-owner seams while preserving the public contracts.

| | |
|---|---|
| Milestone / size | M3 / L |
| Dependencies | T03, T16, T30, T31, T50; root completes T50 first |
| Primary ownership | `src/pipeline/pdf.rs` |
| Tests | `tests/typst_emit.rs`, `tests/pdf_oracle.rs`, PDF pipeline tests, additive CLI tests/snapshots |
| Binding sources | PLAN §3.2, T32, §6–7; typst-dialect.md §4.6, §5–6, §8, Fixtures; references.md §7; cli.md build/dump; live-reload.md §4.1, §5.2–5.3 |

## Goal and boundaries

Markdown/notebook prepare+resolve → deterministic Typst with virtual assets →
shared in-process World → compile_pdf → requested PDF output. `dump --typst`
prints the same generated source without writing or compiling a PDF. The CLI
reports PDF page count and size. Mount configured bibliography, CSL and template
resources, preserve mapped diagnostics, test timestamp reproducibility and the
numbering oracle, and exercise generated PDFs on the existing three-OS CI.

Leave `.typ` PDF dispatch in `pipeline/typst_source.rs` for T52; oracle tests may
parse its T50 subset fixture and emit its IR now. Leave `--to typst` in T34,
Chrome printing in T77, real PagesBuilder/SVG rendering in T24, compiler-side
Markdown/notebook includes in T53, full bibliography loading in T70, and expanded
diagnostic presentation in T73. Do not implement these to make this milestone
appear complete. No new dependency, IR/config schema, CLI flag or public module.

## Existing interfaces and call sites

`src/pipeline/pdf.rs` has exactly four pending bodies:

```rust
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)>;
pub fn dump(
    prepared: &Prepared, opts: &BuildOptions,
) -> Result<(String, Diagnostics)>;
pub fn load_typst_main(
    prepared: &Prepared, opts: &BuildOptions, world: &mut WriterWorld,
) -> std::result::Result<(), BuildFailure>;
pub fn page_count(pdf: &[u8]) -> Option<usize>;
```

- `pipeline::load_typst_main` publicly delegates to the frozen unit-result
  loader; future T24 consumes it. Preserve that public signature.
- `Prepared` owns document, assets, effective normalized config and preparation
  diagnostics. Config resource paths have already been made absolute.
- `build_targets_with_count` prepares once, retains one WorldSlot, resets it
  once, and executes targets independently. Its dispatch already selects the
  `.typ` and Chrome stubs before the ordinary PDF arm. Existing output path,
  directory refusal and input identity-clash protections run before target work.
- `super::write_file` creates parents and contextualizes I/O failures; write
  only after successful compile. Main PDF is first/only returned written path.
- `cli/dump.rs` already calls pdf::dump, prints source to stdout and findings to
  stderr. `cli/build.rs::wrote_line` already rereads bytes through page_count.
- Strict escalation is centrally applied after target execution. Keep emitter
  diagnostics at base severity, preserving their codes; no ad hoc exit policy.

T30 exposes `emit(document, registry, &EmitOptions) -> Result<Emitted>`;
Emitted owns text, SourceMap, diagnostics and asset-id→emitted-path map.
Use `AssetPaths::Virtual`, fragment=false and empty footnote_prefix. Configured
template becomes Some(`/__writer/template/<filename>`); bibliography paths and
CSL path use the mounts below, while a named style remains its name. Share this
option/source construction between dump and build, without emitting twice.

T03 exposes WriterWorld::{set_main_text,set_assets,mount,set_inputs,reset,
accessed,include_diagnostics,source_map,real_path}; construction already belongs
to WorldSlot. set_main_text retains syntax trees, set_assets replaces asset
state, mount replaces an equal prefix and invalidates its cached slots. Use
`world_inputs(&prepared.config)` as the only writer-config/version producer.

| Virtual path | Resource |
|---|---|
| `/__writer-main.typ` | generated main source and SourceMap |
| `/__assets/<id>.<ext>` | AssetRegistry file or byte data |
| `/__writer/template/<filename>` | configured template's directory |
| `/__writer/bib/<n>/<filename>` | nth configured bibliography's directory |
| `/__writer/csl/<filename>` | configured CSL's directory |

Do not reset or evict from emit/load/compile helpers: the caller owns each
attempt's reset and one comemo eviction, and shared web/PDF accesses must survive.
No handwritten Library or replacement include provider. File bytes/imports
must go through World so watches observe reads and missing paths.

T03 compile::{compile_document,compile_pdf} return Compiled<T> with optional
output plus all diagnostics. Pdf has bytes and exact pages; PdfOptions has
timestamp and optional ident. compile_pdf already adds include diagnostics,
sets writer creator/tagging, and maps compiler/export failures. It must not be
reimplemented. `Compiled::into_result` converts the first error to Error::Compile
but currently discards the complete diagnostics on Err; see seams below.

## Concrete seams for coordinator decision

1. **Successful loader findings.** The frozen loader returns (), so emission
   warnings cannot reach T24's future pages stage through it. Proposed: private
   emit/load helpers in pdf.rs with an internal load returning Diagnostics; keep
   public load_typst_main as its compatibility wrapper. Root should freeze the
   smallest crate-private entry T24 can use, or explicitly choose storage. Do
   not silently drop findings, emit twice, or mix them into include-provider
   diagnostics, whose lifetime/owner differs.
2. **Failed target findings.** Private build_target and public pdf::build return
   Error-only Err; BuildReport collects render findings only on Ok. Proposed:
   private detailed PDF entry returning BuildFailure plus a narrow private
   dispatch change that appends failure.diagnostics and retains TargetResult's
   existing Error. Keep the public build wrapper and other target APIs. Compile
   errors/warnings must remain available once, including mapped cell context.
3. **Full session timestamp.** WorldSlot already computes pdf_timestamp when
   constructing a World, but only date-only World::today is public; its full
   stored Datetime is private. Recomputing now in pdf::build can diverge from
   that retained World. Proposed: crate-private full policy timestamp accessor,
   used for PdfOptions, with regression that creation date and today agree.
   Decide explicitly whether a metadata/config timestamp edit updates a retained
   session; current WorldSlot validates a new value but retains its original
   stored value unless font/package construction settings change.
4. **Managed mount replacement.** mount replaces equal prefixes, but there is
   no removal API. Removing template/CSL or shrinking bibliography leaves stale
   mounts in a retained World. Proposed: narrow managed-prefix replacement or
   unmount support in world.rs, invalidating associated slots. Preserve render
   namespaces/project root and test A→B/removal in one World.
5. **Diagnostic fallback paths.** mapped_location returns no real path for
   virtual main; mapped SourceSpan.file can hold the original/include path, and
   unmapped scaffolding has neither. into_result currently falls back to
   `/__writer-main.typ`. The PDF boundary should prefer span.file, then mapped
   path, then prepared input (without generated line for scaffolding), preserving
   package path/hints. Test actual pipeline failures rather than assuming T03's
   mapper alone supplies the input filename.

`pdf_timestamp` itself already implements and unit-tests config RFC3339→UTC,
valid metadata YYYY-MM-DD→midnight UTC, SOURCE_DATE_EPOCH, then whole-second now,
in that order. Invalid configured timestamp/epoch is Error::Config. Do not
reimplement it or use date-only today as PDF creation time.

## Suggested parallel ownership

- Root: pdf.rs composition, private pipeline failure-dispatch seam, narrow World
  timestamp/mount changes after freezing decisions; preserve frozen public APIs.
- Agent: emitter corpus snapshots and the single numbering-oracle integration
  test, including helper-only test code under tests/ if useful.
- Agent: real PDF/CLI integration and page_count implementation/tests in a
  coordinator-assigned private child or temporary draft to avoid pdf.rs overlap.
- All compile/fmt/Clippy/debug/release/docs commands remain serialized by root.
  Review generated snapshots before acceptance; do not bless blindly.

## Required coverage and acceptance

- `tests/typst_emit.rs` globs every `tests/fixtures/md/*.md`, parses/resolves and
  snapshots deterministic emitted source (and relevant findings). A parse Err
  snapshots Display text, per PLAN §7; malformed corpus entries need not compile.
  No writer.toml may be placed above fixture directories.
- Real PDFs from basic/mixed Markdown and notebook content using embedded fonts,
  local/byte assets, no external Typst/Chrome/network dependency. PDF header,
  nonzero page count, creation metadata and output summary are checked.
- Fixed `[pdf] timestamp` plus system_fonts=false produces identical bytes on
  repeated builds. Test date/epoch policy through existing pure helper or child
  process env; do not mutate process-global env in parallel Rust tests.
- Test local bibliography mount separation for identical basenames, named style
  vs CSL path, template import and relative imports, file/byte asset mount use,
  and reads retained in accessed(). T34 owns the fuller BYO/export corpus.
- Compiler failures from raw Typst map to original Markdown line; included IR
  locations retain span.file and notebook cell. Template/package failures retain
  their real location or outer project call plus package hint. Header failure
  names input without a synthetic line. No new output on compile failure.
- Independent HTML+PDF success and HTML success despite real PDF compile failure;
  every compiler diagnostic retained once. Output-clash/refusal behavior remains.
- Replace only T32 stub expectations in cli_build.rs (including mixed HTML/PDF)
  and cli_dump.rs; keep T34/T52/T61/T77 stubs. `dump --typst` checks deterministic
  stdout, findings on stderr, no output writes and no compile requirement.
- Existing three-OS CI should include an ordinary generated PDF smoke test with
  portable paths and embedded fonts; no Unix-only browser/tool requirement.
- Root runs appropriate focused suites, then repository-required fmt, Clippy
  all-target/all-feature deny-warnings, debug/release tests and rustdoc checks.

**One oracle test**, `tests/pdf_oracle.rs`, follows references.md §7 exactly:
numbering.md and policy variants, figures.md, tables.md, code.md, math.md,
references.md, and typ/subset.typ. Prepare+resolve+emit IR, set world_inputs(config)
with writer-oracle="1", compile_document and inspect oracle metadata. Skip
LabelKind::Other and writer-fn-* labels; compare each remaining kind and displayed
number (equations parenthesized; unnumbered None) and assert total oracle entries
equals checked labels. Failures name fixture, label, Writer expected and Typst
actual. T50's labelled bare-table wrapper/inner Other label follow these rules;
do not rewrite frontend normalization to simplify the oracle.

## Donors and page-count limits

Main implementation composes Writer's existing providers; presenter's PDF export
is Chrome-oriented and is not the Typst pipeline donor. Reuse CLI/report and
output-safety patterns from existing Writer pipeline and, as needed,
`../presenter_rs/src/cli/export.rs`.

`../presenter_rs/src/pdf.rs:614` has a small token-based page_count and private
lexer; tests near lines 1916/1925 exclude strings/comments/streams. Its contract
explicitly assumes Chrome's uncompressed object dictionaries. Validate it against
actual Writer/Typst PDFs before adapting; do not call it a general PDF parser.
No direct PDF parser dependency is present or required by this brief.

Pinned local primary source offers a second supported-output seam:
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/krilla-0.8.2/src/chunk_container.rs`
lines 186/210 writes exact page count via XMP in a plain metadata stream;
`xmp-writer-0.3.3/src/lib.rs:612` names the property xmpTPg:NPages. XMP defaults
enabled in krilla serialize.rs. Decide after inspecting real generated output;
require count == compile_pdf's Pdf.pages in tests, return None for unsupported
bytes, and bound malformed-input parsing. Avoid an unreviewed new dependency or
counting arbitrary /Page occurrences inside user strings/streams.

## Approved coordinator decisions (2026-10-28)

- Keep public PDF functions unchanged. Add crate-private load_with_diagnostics
  for pages to retain successful emission findings, and build_detailed for the
  private target dispatcher to retain all failed compiler findings. Preserve
  TargetResult's Error result and all other target APIs.
- WriterWorld provides crate-private session_timestamp returning its full
  stored Datetime and unmount(prefix) removing equal/descendant mounts at path
  component boundaries and invalidating their cached project slots. Preserve
  reserved main/render files. PDF owns only its template/bib/csl subtrees.
- Use the retained World's timestamp, including time, for PdfOptions. Existing
  WorldSlot construction/retention policy remains; no timestamp recomputation
  in the PDF target, no resetting or evicting inside loaders/exporters.
- At the PDF boundary, diagnostic paths prefer mapped span.file, then an
  existing compiler path, then original input. Preserve complete warnings,
  errors, source lines/cells, package traces and hints once on target failure.
- Read page counts only from the pinned exporter's plain XMP packet and
  verify exact agreement with Compiled<Pdf>.pages. Unsupported/malformed bytes
  return None; this helper does not claim to parse arbitrary PDF producers.
- The references.md fixture deliberately repeats sec-duplicate. The oracle
  test uses a documented unique-label variant: assert exactly two occurrences,
  remove only the second label before resolve, preserve all content/citations
  and the original fixture. This avoids ambiguous metadata lookup/counts;
  ordinary corpus snapshots still cover its duplicate-label recovery.


### CLI reporting seam (coordinator, 2026-10-28)

Detailed PDF failures retain their first compiler error both as a target Error
and in the complete diagnostics. The CLI build/watch reporting skips that
redundant target print when path, line and message match a typst-error finding;
exit status and all diagnostics, hints and notebook-cell provenance remain.
Normal/quiet mixed-target subprocess regressions verify one mapped error, HTML
success, PDF failure and no PDF output. This narrow additive cli/build.rs hook
prevents a new duplicate-reporting regression without expanding T73 scope.
