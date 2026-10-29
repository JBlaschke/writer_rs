# T34 — Portable Typst source export and BYO template coverage

Implemented and validated on 2026-10-29 from the checked T43 milestone.
Coordinator choices below are frozen; existing public APIs are preserved.
The validation record follows the implementation contract.

| | |
|---|---|
| Milestone / size | M3 / M |
| Formal dependencies | T1B, T32; both implemented |
| Scheduling | After T43, before T52; no dependency requires reversing this |
| Main owner | `src/pipeline/typst.rs`, currently T34 stub |
| Existing shared implementation | `src/typst/emit.rs`, T30 emit/emit_split and Export names; `src/typst/package.rs`, T31 eject/install_dir; T32 PDF/template mounting |
| Binding sources | PLAN T34/§3.2/§6; typst-dialect.md §§4.6,5.1,5.7,8.4,10/Fixtures; cli.md build/dump/eject; markdown-dialect.md §8; notebook-mapping.md §§2.2–2.6/2.8 |

## Outcome and boundaries

`writer build document.md --to typst` writes a portable `.typ` tree: one main
source, materialised first-level include parts, referenced binary resources,
configured bibliography/CSL, and the chosen PDF template. The author can move
the output tree and compile it with the bundled packages ejected through the
existing library API. Export itself does not compile or require an external
Typst executable, renderer, browser, TeX installation or network.

Keep existing parse/resolve/target dispatch, output naming, strictness and
watch semantics. No new dependencies, modules, config/IR fields, flags or
public helper APIs are required for the ordinary path. Do not implement
`.typ` source builds (T52), Typst-to-Markdown/notebook synthesis (T53), citation
resolution (T70), all-target lints (T71), other eject commands (T72), or Chrome
(T77). Runtime export retains the existing unused `&mut WriterWorld` argument
without creating another World or calling its compiler.

## Existing public APIs — preserve exactly

`src/pipeline/typst.rs` already declares the target entry point:

```rust
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)>;
```

Replace its stub body; the public signature, imports' parent visibility and
per-target dispatch already exist. `Target::Typst` has output-path selection
and `.typ`-input refusal in pipeline/mod.rs. `build_targets` prepares/resolves
once, shares the session, continues independent targets after errors and
applies strict diagnostic escalation after emission.

The emitter already supplies the whole source-export seam:

```rust
pub struct EmitOptions<'a> {
    pub config: &'a Config,
    pub assets: AssetPaths,
    pub template: Option<String>,
    pub bibliography: Vec<String>,
    pub style: String,
    pub footnote_prefix: String,
    pub fragment: bool,
}
pub enum AssetPaths { Virtual, Export { dir: String }, Source }
pub struct Emitted {
    pub text: String,
    pub map: SourceMap,
    pub diagnostics: Diagnostics,
    pub assets: BTreeMap<String, String>,
}
pub struct IncludeRun {
    pub start: usize, pub end: usize, pub file: String,
}
pub fn emit(
    document: &Document, assets: &AssetRegistry, opts: &EmitOptions<'_>,
) -> Result<Emitted>;
pub fn emit_split(
    document: &Document, assets: &AssetRegistry, opts: &EmitOptions<'_>,
    runs: &[IncludeRun],
) -> Result<(Emitted, Vec<Emitted>)>;
```

`emit_split` validates ordered, nonempty, disjoint, in-bounds half-open runs;
allocates asset paths and footnote identities once for the whole document;
returns parts in run order; main inserts `#include` at each run. Parts omit
only the show rule; they still have package/template imports. All part files
must therefore remain siblings of the main file so their shared relative
asset/template paths remain correct. Preserve `fragment = false`: these are
resolved parts of a complete document, not T53's unresolved include fragments.

Library package support already exists:

```rust
pub fn typst::package::eject(dir: &Path) -> Result<Vec<PathBuf>>;
pub fn typst::package::install_dir() -> Result<PathBuf>;
```

Eject writes writer/doc/0.1.0 and preview/mitex/0.2.7 package trees from embedded
bytes, with version-destination no-overwrite checks. CLI Typst template/package
eject arms remain T72 stubs; T34 tests call the library. `vendor::get` supplies
the starter template for tests without introducing a production template API.

`WriterWorld::new(WorldOptions { root, main: MainFile::Path(...), today,
system_fonts, font_paths, package_paths, allow_packages, inputs })` and
`compile::compile_document`/`compile_pdf` already test a physical exported tree.
Use a fixed date, system_fonts=false, allow_packages=false, no virtual main,
assets, include provider or mounts. Keep package paths pointed at the ejected
root. Embedded packages take priority in WriterWorld; report this honestly:
this proves physical resource/include portability and package-tree correctness,
not that the test disabled embedded package resolution. No production package
loader change is justified merely to strengthen that oracle.

## Existing narrow private integration seams

- `pipeline::{write_file,input_files,is_csl}` are visible to the child target
  module; reuse them. `find_clash`, `same_file`, and
  `path_after_creating_dirs` are existing public safety helpers. `input_files`
  protects sources, included files, config, assets, bibliography/CSL, PDF
  template, reference document, LaTeX template and known theme files.
- `AssetRegistry::read(id)` supports in-memory and file assets and preserves
  missing-asset/I/O errors. `Emitted.assets` is the authoritative id→export path
  map, not every registry entry. Union the main and parts without duplicate
  writes; all use the same preallocated paths.
- `markdown::{comrak_options,directives::colon_prepass,
  directives::parse_directive_info}` are public. Existing
  `markdown::include::{include_path,raw_directive_info}` are crate-visible and
  preserve quoted/raw directive paths; `recover::split_lines` is available.
  A private scanner in the export target can call them without new public API.
- `notebook::nbformat::parse_notebook`, CellType, cell.source.0,
  cell.raw_mimetype() are public. `notebook::convert::later_metadata_header`
  is crate-visible; frontmatter::split_front_matter is public. Reuse existing
  notebook front-matter/tag semantics rather than a second JSON dialect.
- `pdf::generate/resource` are private to that sibling and need not be exposed.
  PDF resources are mounted at virtual namespaces; export must plan physical
  copies and relative import paths instead. Keep PDF loader/reset ownership
  intact.

The coordinator freezes these two additive crate-private emitter helpers,
owned by root; the export owner consumes them without changing public APIs:

```rust
pub(crate) fn export_name(
    hint: &str, suffix_key: &str, occupied: &mut BTreeSet<String>,
) -> String;
pub(crate) fn export_asset_paths(
    document: &Document, registry: &AssetRegistry, dir: &str,
) -> BTreeMap<String, String>;
```

The first applies the shared portable allocation rules below; occupied holds
ASCII-folded keys. The second returns precisely the emitter's assigned Export
paths without a redundant whole-document emit. Export owns its full resource
read/copy plan. There is no preflight_clash API: use find_clash and return
Error::Io(std::io::Error::new(InvalidInput, clash)), matching existing targets.
Private helpers may scan direct includes, assign part names and preflight every
destination. Virtual and Source naming must remain unchanged.

## Output and asset plan

The existing pipeline chooses `<out>/<input-stem>.typ`. One-target `-o` names
the exact file; several targets interpret it as a directory. Derive the assets
directory from the actual chosen output file's stem, preserving dotted stems:
`report.final.typ` → `report.final_assets/`. Generated include names likewise
use that output stem, not the original input stem. The `.typ` input error
already suggests `writer build part.md --to typst`.

Use `AssetPaths::Export { dir: "<stem>_assets" }`. The existing emitter allocates
names for referenced assets only, in asset-id order. It takes the final hint
component, replaces characters outside `[A-Za-z0-9._-]` with `-`, maps empty/`.`/
`..` to `asset`, and resolves duplicates before the extension using `-<id6>`,
then `-<id6>-2`, `-<id6>-3`, etc. The returned main/part asset maps determine
exactly which bytes are copied. Do not allocate names based on unreferenced
registered assets, reverse source order, rehash image bytes to rename assets,
or borrow HTML's content-addressed naming scheme. Narrow portable-name
clarification: compare occupied names by ASCII lowercase while preserving the
chosen spelling; trim trailing dots; prefix reserved DOS basenames (CON, PRN,
AUX, NUL, COM1–9, LPT1–9, regardless of extension/case) with `asset-`; empty/dot
names become `asset`. Keep all ordinary noncolliding names unchanged. Root
owns the shared helper so emitter and copied resources use one definition.

Copy configured bibliography sources, a path CSL style (case-insensitive `.csl`
extension, using existing is_csl), and the PDF template into the same assets
directory. Named styles such as `ieee` remain strings and are not files. Keep
bibliography order and pass copied relative paths to EmitOptions; pass the
copied template path as EmitOptions.template so its import names the copy.
All copied bytes must be exact. The template's own imported files are **not**
copied, by §10's explicit limitation. Test PDF mounting of a template importing
a helper separately from portable export of a self-contained template. Do not
claim that a template relying on an uncopied sibling is standalone.

Before writing anything: emit/collect diagnostics, read every needed asset and
resource, finish the full filename plan, check all output files against every
protected input and each other, and verify required directory/file shapes.
Then create parent directories and write through existing helpers. Build
normally replaces its previous output; this is distinct from eject's no-clobber
contract. Preserve unrelated files and do not delete stale assets as unsolicited
cleanup. No filesystem transaction/rollback promise is added for failures that
occur during actual writes, but read/plan/clash failures must write nothing.

Return written paths once: main, include siblings in run order, assets directory
when it is actually created. CLI already prints files and directory counts/bytes;
avoid one status line for each asset plus a duplicate assets-directory line.
The returned order is frozen below.

## First-level includes and resolved split semantics

The spec deliberately rejects `include::scan_includes` for export: it recursively
walks every include and is a lenient watcher tool. The export scanner must read
only the original input's direct document-child `:::include` directives after
colon_prepass and effective Markdown options. Recover the raw directive info
from original lines before include_path; raw colon/quotes must not be confused
with comrak's entity unescaping. Canonicalise paths relative to the input's
directory, matching SourceSpan.file identities, including symlink roots.

For notebook inputs, scan content-bearing Markdown cell source with the same
front-matter blanking/removal rules that T18 used. Ignore code-cell source,
outputs, JSON text that merely resembles a directive, fenced code and nested
list/quote/directive includes. Raw-Markdown is included, as T18 already treats it as Markdown for includes.

Spec run rule: start at a top-level block whose canonical span.file is a
first-level include other than the current run's owner. Continue through
following `span.file != None` blocks so nested includes remain in their
first-level part; a root-source block ends the run. Run outputs are
`<stem>_<include-stem>.typ`, then `-2`, `-3` for duplicate names. Preserve source
order, global numbers, shared footnote identity, forward references, and
cross-part image/bibliography paths. Main owns title/show rule exactly once;
parts are resolved source, not Markdown re-parses and not T53 synthesis.

**Frozen span-only limitation:** flattened IR has only canonical span.file,
with no parent include occurrence. Follow §10 literally: a first-level file
identity differing from the current run starts a new run, and a root-source
block ends the current run. Thus adjacent same-file occurrences may coalesce;
a nested B that is also a direct first-level include may begin a B run. This
changes only file partition/naming, preserving all blocks, order, note
identities, paths and compile validity. Do not add provenance fields, replay
frontends, or infer occurrence identity from line-number resets. Add explicit
content/order/compile tests for this case and document the limitation. An empty
first-level include whose only blocks come from nested files may produce no
matching span and no standalone part; those blocks remain in the main source.

## BYO-template tests and T70-independent bibliography coverage

T32 already mounts the entire PDF template directory, passes its virtual import
after @writer/doc, and replaces old template/bib/CSL mount namespaces in a
retained World. Existing tests/pdf_pipeline.rs covers helper import reads,
replacement, removal, missing/invalid template locations and same-basename bibs.
T34 adds a template whose visible output proves its writer-doc actually
shadows the package (e.g. unmistakable fixed marker/delegating layout change),
with all §3 arguments accepted/forwarded. Compile a Markdown and a notebook
case; check actual layout text/metadata rather than mere PDF magic bytes.

Export the same self-contained template byte-for-byte; verify its copied import
line; compile from a fresh output-root World after the original source/template/
asset tree has been removed or made inaccessible. No mounts or IncludeProvider
can satisfy missing resources. Confirm no virtual `/__writer` or `/__assets`
paths or `.md`/`.ipynb` includes remain in the source tree.

T70 still leaves Resolved.bibliography=None, so real emitted bibliography calls
must be exercised by a hand-built library Prepared fixture supplying
Resolved.bibliography plus Block::Bibliography and ordinary citation data,
following tests/pdf_pipeline.rs's existing fixture seam. Configured .bib/CSL
copy assertions can still use CLI inputs without pretending T70 resolution ran.
Do not implement bibliography resolution or make positive source-compile tests
fail on a deliberately unresolved citation key.

## Acceptance — portable, offline, no optional external compiler

1. Library and real CLI export for Markdown and notebook; exact LF source,
   repeated byte-identical tree, one-target/mixed-target/dotted-stem output
   naming, explicit format override, quiet/status stdout, independent HTML+PDF
   successes when export alone fails, and appropriate exit1/2 mapping.
2. Positive export trees exercising figures.md's valid content and a valid
   private includes.md derivative; same-basename images and file/byte assets,
   sanitization, adversarial generated-suffix collisions, unused registered
   assets, chosen notebook output candidates, nested footnote/image content,
   and main/part asset union. Registry/IR remain unchanged.
3. Direct includes from Markdown and notebook, raw-colon/space/quote paths,
   canonical symlink roots, nested includes retained in their owner's part,
   root-source boundaries, repeated part stems and global heading/figure/note
   labels across parts. Tests cover the frozen ambiguous-adjacent policy.
4. Bibliography/CSL/template exact copies, named style vs path, resource-resource
   and resource-asset basename collisions, all paths relative and portable;
   no template import recursion. Real library fixture exercises copied bib/CSL
   with actual Typst compilation independent of T70.
5. Move/delete-original-tree compile through WriterWorld rooted only at exported
   files; eject packages using library API; no provider/mounts, no network or
   system fonts. Compare meaningful compiled text, image/figure references,
   equation and note identities, not only source string occurrence.
6. BYO PDF template proves delegated output and forwarded metadata; exported
   template shadowing proves the same result from copied files. Existing T32
   helper-import/replacement/error-location tests continue passing.
7. Every output file (including part, copied resource, assets child) is protected
   against source/include/config/bib/CSL/template/asset/theme aliases. Cover
   exact paths, symlinks, Unix hard links and `new/../` aliases; all known clash
   and missing-read failures preserve existing bytes and create no output tree.
   Never overwrite one planned output with another, including existing aliased
   destinations; report portable case-only collisions according to the freeze.
8. Strict warning behavior follows existing policy: math conversion fallback
   source remains visible; strict exits1 after writing supported outputs;
   math-tag-ignored/math-syntax retain warning level. Unknown asset id/read
   errors stop only this target; no output on preflight/read failure.
9. Replace only T34 stub expectations in cli_build.rs, cli_watch.rs and
   watch_loop.rs. Preserve unavailable-target watch continuation coverage by
   using the still-pending Latex target (T61), rather than deleting the test.
   T52/T53/T70/T71/T72/T77 stubs remain asserted where already covered.
10. Existing emitter Export/Source/Virtual exact-output and source-map units,
    PDF/template tests, debug/release suites, rustfmt, denied-warning Clippy/docs
    and guide help continue passing. Root owns compiler/Cargo/Git/fmt execution,
    actual snapshot review, milestone taskboard/measurement and PR publication.

Canonical tests/fixtures/md/includes.md currently contains a cycle include and
an include nested in a list; parsing it correctly fails. Preserve it as negative
coverage. A private valid copy keeps include/part-a.md→part-b.md and its scoped
footnote/image, removes only those intentional errors, and supplies positive
export-tree coverage. figures.md includes a width warning; retain that warning
or use an explicit valid subset for positive compilation, with original fixture
still covered. Do not silently bless invalid dimensions as an export feature.

## Coordinator implementation freeze

- Resource namespace: seed export_name's ASCII-folded occupied set from
  export_asset_paths, then process bibliography config order → path CSL →
  template. Read full bytes first and use registry::content_hash(bytes) for
  suffix_key, passing its first six characters to the helper. Repeated canonical
  same-file resources reuse one copy; distinct resources with identical bytes
  still get separate deterministic names if needed through the counter suffix.
- Shared helper normalizes only otherwise-unwritable Windows names and folded
  collision keys; ordinary existing paths/suffix strategy remain unchanged.
  Main output explicit paths retain the pipeline contract; no repository-wide
  filename policy or Virtual/Source rewrite.
- Include partition follows the literal span-based algorithm and documented
  limitation above. No provenance/API/IR expansion or repeated parse/resolve.
- Scan retained raw-Markdown cells as Markdown, consistent with T18. Ignore
  removed cells, code and outputs; apply existing metadata header blanking.
- Return `[main, parts in run order, assets directory]`, omitting the assets
  directory if there are no copied resources. Emit each path once.
- Reject a non-UTF8 chosen output stem as Config error before writes; never
  produce lossy, nonmatching include/asset paths. Resource/part filename hints
  are allocated through the shared sanitized name vocabulary.
- Copy every configured bib/path-CSL/template even before T70 populates
  Bibliography. Use hand-resolved fixtures for actual bibliography compilation.
- The original includes.md remains negative; use an explicit positive private
  derivative preserving part-a→part-b, footnote and image. T72 eject commands
  remain stubs; the portable compile test calls package::eject directly.

No formal dependency blocks implementing these choices after T43. T52 will
later consume the already-complete structured frontend and native .typ PDF
path; T53 adds the distinct runtime source-synthesis path. T34's physical export
must deliberately work while both remain stubs.


## Local validation record

Formatting, all-target/all-feature Clippy and denied-warning default and
browser-feature rustdoc pass. Complete suites pass with 1,561 debug and 1,560
release tests. Eight private planning units include six BOM/line-ending parity
variants. All 23 new library/CLI tests and 77 inherited emitter/CLI/watch tests
pass, and the actual basic CLI command writes the expected source/assets.

Fresh physical Worlds compile moved trees after original sources are removed,
with no mounts or provider. Existing embedded-package priority is unchanged;
ejected package trees are verified, not falsely claimed to disable embedded
resolution. The actual includes fixture's valid private derivative preserves
the A→B chain, note and image; its unchanged negative form still fails safely.
Real sidecar aliases, direct all-OS resource clashes, missing reads, empty
folders and new/../ aliases preserve the entire existing output tree.

The notebook assertion uses independent markers and exact compiled counts.
Numbering assertions explicitly enable the package's test-only oracle input.
The only runtime correction found during review was matching frontend empty
front matter when its source has a BOM; LF, CRLF and CR cases all pass.
Measurements are in `docs/dev/build-times.md`; raw logs remain in the ignored
T34 recovery directory. No new dependencies, public shapes or modules were
needed. Template helper imports and CLI package ejection retain their declared
boundaries.
