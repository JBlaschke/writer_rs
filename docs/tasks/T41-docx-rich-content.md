# T41 — DOCX tables, figures, captions, TOC and target wiring

Frozen implementation brief, coordinator, 2026-10-27.

| | |
|---|---|
| Milestone / size | M4 / L |
| Depends on | T16, T40 (ready); consumes T04/T03/T17 and completed T18 shapes |
| Owned source | `src/docx/{tables,images,fields,toc}.rs`, `src/pipeline/docx.rs` (replace stubs) |
| Fixtures | `tests/fixtures/docx/ir/{figures,tables,toc,bibliography}.json`; `tests/fixtures/docx/images/{dpi144.png,jfif300.jpg,drawing.svg,anim.gif,pic.webp}` |
| Tests | Extend `tests/docx_snapshots.rs` and its affected snapshots; new target integration tests, timestamp parity tests |
| Additive seams needing authorization | `src/docx/{footnotes,body,mod}.rs` prepass/private shared state/heading reset and table boundaries; remove DOCX target from `tests/cli_build.rs::stub_targets_exit_2` |

## Goal and binding sources

Produce editable Word figures/captions/tables, rendered images, a cached TOC
and bibliography from resolved IR; make `writer build --to docx` work using
the already shared compiler World and existing output-safety pipeline.

Binding: PLAN §3.2, §5 T41, §6–7; `docs/spec/docx-output.md` §1.1,
§1.4–1.5, §4.9–4.14, §4.16, §5, §7.1–7.2, §8, Fixtures and Deviations;
`docs/spec/cli.md` build; `docs/spec/references.md` §2.3, §4.5, §6;
`docs/spec/notebook-mapping.md` output candidates/table shapes. Research
`docs/research/R3-docx.md` §2.11–2.12, §2.16, §6 is reference material;
binding specs supersede its old resvg/DPI/pandoc field recommendations.

## Fixed APIs and existing seams

Keep the existing cross-file signatures:

```rust
impl docx::Emitter<'_> {
    pub(crate) fn image_run(&mut self, url: &str, alt: &Inlines,
        title: Option<&str>, attrs: &Attrs);
    pub(crate) fn figure(&mut self, block: &Block);
    pub(crate) fn table(&mut self, block: &Block, figure: Option<&Block>);
    pub(crate) fn toc(&mut self, depth: u8);
    pub(crate) fn bibliography(&mut self);
}
pub struct pipeline::docx::WorldRenderer<'a> { /* private fields */ }
impl<'a> WorldRenderer<'a> {
    pub fn new(config: &'a Config, world: &'a mut WriterWorld) -> Self;
}
pub fn pipeline::docx::build(doc: &Document, assets: &AssetRegistry,
    config: &Config, out: &Path, world: &mut WriterWorld)
    -> Result<Diagnostics>;
```

`WorldRenderer` implements `docx::Renderer::{equation,image,typst_mathml}`
through `typst::render::{equation,image}` and
`typst::html::equation_mathml`, converting Rendition to Picture. Equation
source is already prepared by the emitter (LaTeX macro expansion belongs
there); do not expand it twice. This adapter is implemented now for T42 to
consume, while `docx/math.rs` and OMML remain T42.

`build` calls `docx::write` with resolved reference path, one read of
SOURCE_DATE_EPOCH, and `all_styles: false`; writes through existing
`pipeline::write_file`; returns emission diagnostics. Never create/reset a
World here. T16 already selects `.docx` output paths, checks input/resource
clashes, shares WorldSlot across targets, calls this exact build signature,
and escalates strict diagnostics. No clap/pipeline-dispatch rewrite needed.

Reuse existing state/APIs:

- Emitter has `content` (crate-private T40 ContentState), `part`, body/notes,
  styles/numbering/layout/package/IDs and current source span.
- `start_paragraph`/`end_paragraph`, `inlines`, `blocks`, `start_bookmark`/
  `end_bookmark`, `content_warning` are available. `buffer()` is visible to
  descendant modules; `body::separate_table` is currently private and may
  need a crate-private seam for figure tables. Keep table-separating/trailing
  paragraphs, list context and document/footnote buffers correct.
- `Package::image_rel(part,key,ext,bytes) -> (rel_id,media_name)` deduplicates
  media and makes part-local relationships; `hyperlink_rel` likewise.
- `xml::{TableProps,RowProps,CellProps,ParaProps,RunProps,Props}` already
  enforce property order; `xml::field(instr,result_runs)` makes the complex
  field form. No ad hoc property ordering and no `w:dirty`.
- T40 ContentPlan currently omits figures, table-cell inlines and bibliography
  content. Extending it is required, not optional: bookmarks and nested notes
  must exist before forward links render. Keep whole-IR unknown-asset
  validation separate from `pick("docx")` emitted-content planning.

## Implementation contracts

1. **Figures/captions:** Image drawing paragraph first (Captioned Figure,
   or Figure without caption/number), then Image Caption; Table Caption
   before table with plain `tblCaption`; Listing Caption before code;
   Other body (including ordinary subfigures) before Caption. Reuse resolved
   supplement/number/localized separator. Label bookmark surrounds supplement
   + number, or unnumbered caption content; no caption/number means no caption
   paragraph and any label uses block-level bookmarks. Render rich caption
   runs, images/notes/links, and table cell runs through existing emitters.
2. **SEQ fields:** separate Figure/Table/Listing counters (Image and Other
   share Figure). Effective section mode requires figure policy Section and
   heading numbering not Off. Chapter is literal text plus `.`, then
   `SEQ Kind \* ARABIC \s 1`; otherwise `SEQ Kind \* ARABIC`.
   Cache the resolved IR number. Track Word's emission-order counter and
   actual `heading 1` style resets (including unnumbered headings; exclude
   Heading 1 Unlisted). Append `\r n` only when Word's next value differs
   from the cached counter. Parent/subfigure order requires these resets:
   two children → child reset 2, parent reset 1, next figure reset 4.
3. **Tables:** all fractional widths → fixed grid using §4.12's normalization
   `round(f_i * min(1,1/s) * available_twips)`, percentage tblW; otherwise
   equal columns and remainder in last, auto tblW. Table custom-style wins
   over enclosing Table Figure style; use Table vocabulary fallback. Preserve
   head/body/foot order, tblHeader, tblLook flags and caption. Occupancy grid
   inserts rowspan continuation cells (same colspan), pads ragged rows, drops
   beyond-grid cells with located unsupported-construct. Every cell ends in
   Compact paragraph, includes tcW, explicit/column alignment, and body
   header cells get b+bCs. No direct-byte HTML or DataFrame re-parsing.
4. **Images:** only registry assets are read; never fetch external URLs.
   Remote/unsupported URLs become alt-or-URL hyperlinks with warning. PNG/
   JPEG bytes stay native; SVG remains vector plus PNG fallback, fallback
   failure gives SVG-only + docx-svg-no-fallback; GIF/WebP/PDF become PNG.
   Read pixel dimensions via imagesize. PNG pHYs and JFIF APP0 determine DPI
   (10–2400 valid, else 96); SVG/PDF natural rendition dimensions or 6×4in
   on error. Support px/pt/cm/mm/in/em/% width/height, preserve ratio when
   one dimension is supplied, clamp both dimensions to text bounds. Table
   available width is spanned grid minus 216 twips. Convert via specified
   EMU factors, round positive integers, share unique docPr IDs across parts,
   dedup `(asset/hash,variant)` media, preserve title and collapsed alt text.
5. **TOC:** sdt with Table of Contents gallery, first-only docPartUnique,
   localized/meta-title heading and resolved entries through depth. Style
   `toc n`, right dotted tab at available width, cached plain text and numbers,
   PAGEREF inside hyperlink only when the bookmark prepass contains the target.
   One TOC field spans entry paragraphs; empty entries still yield balanced
   empty field. TOC title override `toc-title`; no duplicate note emission.
6. **Bibliography:** None produces nothing. Some produces localized or
   `reference-section-title` Heading 1 with `_refs` bookmark, then rich
   Bibliography paragraphs in supplied order. Prefix + tab uses 720-twip
   left/hanging indent and tabstop; otherwise hanging indent only when asked.
   Preformatted BibEntry data is available now; bibliography loading and
   citation formatting remain T70, not a T41 dependency.
7. **Errors/determinism:** content limitations are located diagnostics;
   unknown asset IDs/bad references/invalid timestamps/I/O keep library error
   behavior. New code must preserve deterministic first-use IDs, ZIP entries,
   timestamps and reference-part reuse. Existing update_fields settings logic
   stays T04; T42 owns equation frames, OMML and fallback policy.

## Acceptance and fixtures

- Add four resolved-IR fixtures and five image fixtures named above. Snapshot
  every XML/rels part plus docx-rs read-back via existing harness. It already
  globs all IR fixtures, registers sorted image bytes by content hash, uses
  FakeRenderer, validates every output and supports reference reuse. Review
  existing sample/T40 snapshots changed by replacing the rich-content stubs.
- Structural assertions: native PNG/JPEG exact bytes and DPI extents; GIF/
  WebP/PDF conversions; SVG vector+fallback and failing-fallback case; width/
  height units, ratio/clamping, table-cell widths, repeated media across
  document/footnotes with part-local rels and globally unique docPr IDs.
- Tables cover fractional/equal widths, custom styles, head/foot styling,
  colspan/rowspan continuation, missing slots, beyond-grid cells, alignment,
  body-header bold, notes/images/links in cells and trailing paragraphs.
- Figures cover every kind and labelled/unlabelled/numbered/unnumbered/no-
  caption combinations, caption notes/bookmarks/forward refs; section/document
  counters, unnumbered/unlisted level-1 headings and subfigure reset examples
  from §4.11 (`numbering.md` fig-int reset3, fallback no `\s 1`, group resets).
- TOC depth/multiple/empty/override-title/unpicked candidate cases; bibliography
  title/_refs/TOC/internal links, numeric prefix/hanging/links/empty/None.
- Validate every test-produced DOCX (`validate == []`), notably V05 rels,
  V06 child order, V08–12 bookmarks/fields/drawings/notes, V14 styles,
  V15 table grid/merge/end-paragraph and V18 media. Read-back counts are
  additional smoke, not the structural authority.
- Repeated write bytes identical with/without reference, SOURCE_DATE_EPOCH0,
  timestamp parity with `typst::compile::pdf_timestamp` for equivalent inputs;
  preserve DOCX-specific four-digit UTC-year range exception.
- CLI builds MD fixtures figures/tables/math/references/footnotes/raw/numbering,
  notebook outputs/dataframe and basic/mixed examples to DOCX; ordinary T42
  math placeholders remain explicit until T42. Test default/custom output,
  --reference-doc, html+docx target independence, clash refusal, strict warning
  escalation, no extra Worlds and no HTML-theme dependency.
- Root serial checks: fmt, all-target/all-feature Clippy with denied warnings,
  debug/release suites, rustdoc denied warnings, HTML basic/mixed regression.
  Open milestone PR against main, listing unmerged dependencies/merge order.

## Local lift/reference sources

- Presenter has **no DOCX implementation**. `../presenter_rs/src/pptx.rs`
  image headers/dedup/EMU helpers and deterministic ZIP tests are reference
  patterns; T04 already lifted package/XML primitives. Do not copy the
  stored-only ZIP writer or make docx-rs a runtime writer.
- `docs/research/R3-docx.md` §2.11/2.12/2.16 captures tested figure/SVG,
  table and TOC XML. Existing local pandoc output under
  `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/pandoc-docx/out/{all,native}/word/document.xml`
  and `all{,-native}.docx` can be inspected. Local source reference:
  same scratchpad `/pandoc-src/Docx.hs`. These are behavioral references,
  not code/data to vendor; the binding spec intentionally differs on fields.
- Registry `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/docx-rs-0.4.22/src/documents/elements/{table*,pic,table_of_contents*,instr_text}.rs`
  is a secondary element/read-back reference. Use current T04 builders first.

## Coordinator choices/dependencies to freeze

1. Approve narrow T40/mod edits: emitted ContentPlan traverses exactly the new
   visible figure/caption/table/bibliography shapes; private caption/TOC/image
   state; Heading 1 reset hook; scoped table-width state; block table boundary
   seam. Keep public Emitter/Renderer signatures and IR/config shapes intact.
2. Freeze unavailable/corrupt asset/rendition behavior with the unit-returning
   image_run API. AssetRegistry::read may fail after registration; current
   preflight only validates unknown IDs. Decide private deferred I/O error
   storage or selected-asset preload/cache so true I/O is propagated without
   turning arbitrary corrupt image content into a panic/fatal content error.
   GIF/WebP/PDF conversion failure fallback/message also needs a small rule.
3. Keep T42 math warning stubs explicit in this PR; do not require zero math
   warnings for `math.md`, and do not implement numbered equation frames.
   WorldRenderer's equation/MathML adapter is nevertheless fully implemented.
4. Approve a private reusable Picture embed helper for T42, with normal-font
   scaling/baseline parameter, avoiding a later duplicate DrawingML builder.
   Exact name/private arguments are coordinator/local choices, not new API.
5. Confirm no runtime dependency needed: imagesize/zip/quick-xml/renditions
   already exist. One-time WebP fixture generation uses available local
   image-webp source/tool only; do not add a runtime image encoder dependency.

No hard dependency blocker: T16/T40/T03/T04 are present, and resolved
bibliography fixtures can exercise T41 before T70 exists.

## Concrete implementation split (coordinator discussion)

- **Root:** `docx/{mod,body,footnotes}.rs` additive shared-state/prepass/
  Heading 1/table-boundary seams; `pipeline/docx.rs`; CLI integration and
  timestamp tests; snapshot harness/reblessing; docs and final serialization
  of Cargo/git. Root supplies Emitter `fields: fields::FieldState`,
  `images: images::ImageState`, `deferred_error: Option<Error>` (first error),
  `table_width_twips: Option<u32>`, and `available_width_twips() -> u32`.
- **Fields agent:** `docx/{fields,toc}.rs`, figures/toc/bibliography IR
  fixtures and `tests/docx_fields.rs`. FieldState derives Default and holds
  a per-identifier sequence map plus first-TOC flag. It provides
  `Emitter::heading_one()`; root invokes it centrally for actual Heading 1
  paragraphs, including bibliography and unnumbered headings but excluding
  Heading 1 Unlisted. No edits to root's shared files.
- **Images agent:** `docx/images.rs`, five image fixtures,
  `tests/docx_images.rs`; private ImageState/cache and reusable Picture embed
  helper for T42. Reads available width but does not own table scoping.
- **Tables agent:** `docx/tables.rs`, tables IR fixture,
  `tests/docx_tables.rs`; saves/restores available width around cell inlines.
  Expose `rendered_cells(&Block) -> Vec<&TableCell>` from the same occupancy
  plan used by rendering, for root's exact emitted-content prepass.

Prepass claims the Figure label before descendants, then visits body/caption
in actual layout order (Image/Other body then caption; Table/Listing caption
then body). Table visits only retained cell inlines. Bibliography reserves
synthetic `_refs` at its first logical position, then prefix/entry inlines.
TOC emits plain text and must not allocate notes from entry inlines. All rich
prepass/emission walks use original IR references, preserving T40 identities.

Freeze failure clarifications: every AssetRegistry read error is deferred
fatal (first wins; attach current source span to MissingAsset when absent),
checked before package finish/output writing. Renderer failures are content
fallbacks: GIF/WebP/PDF and corrupt native PNG/JPEG produce alt text plus
located unsupported-construct. SVG natural-size failure uses 6×4in silently;
failed display fallback retains original SVG-only plus docx-svg-no-fallback.
Unsupported registered MIME uses alt text; remote URLs use alt-or-URL
hyperlinks; both warn unsupported-construct and never fetch remote bytes.


## Frozen coordinator decisions (2026-10-27)

- The additive private state, ContentPlan/body seams and Picture helper above
  are approved. Root owns `mod.rs`, `body.rs`, `footnotes.rs`, pipeline wiring,
  target/timestamp integration tests and the snapshot harness. A fields agent
  owns `fields.rs`, `toc.rs`, figures/TOC/bibliography fixtures and
  `tests/docx_fields.rs`; an images agent owns `images.rs`, the five image
  fixtures and `tests/docx_images.rs`; a tables agent owns `tables.rs`, the
  table fixture and `tests/docx_tables.rs`. No agent runs Cargo concurrently;
  the coordinator serializes all builds, formatting and snapshot generation.
- Root adds `fields: fields::FieldState` (Default),
  `table_width_twips: Option<u32>` (None), `deferred_error: Option<Error>`
  (None) and `available_width_twips() -> u32` to Emitter. The table width
  temporarily overrides text width for table-cell content and nested tables.
  The fields module supplies `heading_one()` for actual heading-1 paragraphs.
  Additional image cache state is allowed if justified and coordinated.
- Any `assets.read` error is preserved as the first deferred error; `write`
  returns it after content emission and before assembling the ZIP. Invalid
  PNG/JPEG/GIF/WebP bytes and failed GIF/WebP/PDF conversions emit alt text
  and a located `unsupported-construct`, never a panic or a content error.
  Supported native bytes are retained when their dimensions can be read.
  SVG natural-size failure keeps the specified 6x4-inch fallback; failed
  PNG fallback retains the SVG and raises `docx-svg-no-fallback` exactly once.
  Unsupported MIME emits alt text with its warning. External URLs keep the
  specified hyperlink fallback and are never fetched.
- T42 math stubs remain explicit. WorldRenderer implements all three renderer
  methods now, without re-expanding prepared equation macros. No equation
  frames or OMML are added to this milestone.
- New tests may use local fake renderers and package-inspection helpers.
  Every successful generated package is validated before assertions;
  whole-fixture snapshots retain the independent docx-rs readback.
- No runtime dependencies or IR/config/clap shapes change. The PR starts from
  checked T1C and targets main; list all open inherited milestone PRs and
  their merge order, then continue while CI runs.


## Acceptance record (2026-10-28)

All acceptance groups are covered by five image unit tests, ten fields/TOC/
bibliography tests, ten image integration tests, fourteen table tests, eleven
pipeline/CLI tests and the cross-feature prepass regression. Four valid new IR
fixtures and five byte-stable images feed the existing package snapshot and
independent read-back harness. All 63 new/updated snapshots are reviewed.

Formatting, all-target/all-feature Clippy and rustdoc with denied warnings,
1,115 debug and 1,114 release tests pass. The full suite's previous T04
T41-stub assertion is narrowed to the pending T42 math stub; no remaining
T41 placeholder is expected. The SVG validator's existing first-element
sniffer is exposed crate-privately to share corrupt-content handling.

Basic/notebook/mixed DOCX builds are repeatable and valid. Manual basic/mixed
DOCX outputs are 9.5/47.6 kB; HTML regression outputs are 687.6/111.6 kB.
The references fixture intentionally reports duplicate-label errors. The
canonical outputs notebook intentionally fails for a missing selected image;
a staged copy supplies it and redirects the outside-image probe to exercise
all remaining outputs without changing the source corpus.

The final branch rebases onto merged main `91ed669` (same tree as checked
T1C), so its PR has no pending merge dependency. T42 math, T70 bibliography
loading/formatting and T43 visual Word validation retain their planned scope.
