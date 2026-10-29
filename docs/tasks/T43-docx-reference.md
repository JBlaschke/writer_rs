# T43 — Ejected Word reference, style showcase and document validation

Implemented on 2026-10-28. The public generator, CLI arm, reproducible fixtures,
style/package coverage and optional platform checks are complete. Focused
validation is recorded below; final full-suite results belong to the milestone
validation record, not an inferred count in this brief.

| | |
|---|---|
| Milestone / size | M4 / M |
| Dependencies | T41 and T42 |
| Primary source | `src/docx/custom_style.rs` |
| CLI integration | `src/cli/eject.rs`, DocxReference only; other eject commands remain T72 |
| Tests | `tests/docx_showcase.rs`, `docx_reference.rs`, `docx_smoke.rs`, `cli_eject_docx.rs` |
| Fixtures | `tests/fixtures/docx/reference.docx`, `reference-localized.docx` |
| Release QA | `docs/dev/docx-validation.md` |
| Binding sources | PLAN T43/§7; docx-output.md §2.3, §6.1–6.5, §7.1–7.4, Fixtures/Deviations; cli.md eject |

## Implemented scope and boundaries

Writer provides a deterministic, editable reference DOCX with a readable style
showcase. Runtime output remains hand-written OOXML through `docx::write`;
`docx-rs` is an independent development-time reader. No dependencies, public
module lists, IR/config fields, World, renderer backend or style vocabulary
were added. Name-based style lookup, reference copying and section/settings
merging use the existing implementation.

T43 handles only `eject docx-reference`. Init, theme/Typst/LaTeX eject and other
T72 infrastructure remain outside this milestone, as do T71 target lints. No
Word automation, external converter or compiler is needed by the generator.

## Public generator contract

The existing `docx::custom_style` module exports:

```rust
pub fn showcase_document() -> (Document, AssetRegistry);
pub fn reference_docx() -> Result<Vec<u8>>;
```

`showcase_document` constructs resolved IR directly, including labels, TOC and
bibliography. It does not parse Markdown, invoke resolution or read a source
file. Its identity is fixed:

- Title: `Writer DOCX style showcase`.
- Source: `/__writer-showcase.md`, Markdown, base directory `/`.
- Numbered headings 1–6: `Heading {N} (numbered)`, labelled
  `showcase-heading-{N}`, with fixed hierarchical number caches.
- Unlisted headings 1–6: `Heading {N} (unlisted)`, labelled
  `showcase-unlisted-{N}`, with `notoc`/`unnumbered` attributes.
- The six TOC entries contain only the numbered heading labels. The abstract
  also has an unlisted heading.
- Equation `showcase-equation`: LaTeX `\frac{a}{b}`, number `1`.
- Exactly one registered 64×32 grayscale PNG, embedded as a private 94-byte
  constant and reused by both image figures. No runtime image generation or
  fixture read occurs.

`reference_docx` writes that pair with default config, no reference,
`all_styles=true`, `source_date_epoch=None` and renderer methods that return
errors. Native PNG and supported editable LaTeX OMML need no renderer call.
Unexpected content diagnostics make generation fail. Repeated calls produce
identical bytes; writing leaves the source document and asset registry intact.

The Date sample is `This paragraph uses Date.`. It exercises Date styling
without parsing as an ISO date and populating core created/modified properties.
The generator consults neither `SOURCE_DATE_EPOCH` nor wall-clock time.

## Real style coverage

The showcase uses semantic IR rather than appended XML style references.
Ordinary prose names its demonstrated style; structural empty paragraphs and
frame cells remain empty where required. It includes:

- Title, subtitle, author, date, abstract, TOC and all numbered/unlisted heading
  levels, including TOC Heading and Section Number runs.
- First Paragraph, Body Text, Compact, bullet/ordered/task lists, restarts,
  empty list items, nested quotes, definitions, footnote text and repeated refs.
- Numbered highlighting samples reaching every emitting token style and Code
  Line Number, plus Verbatim Char and hyperlinks.
- Captioned/uncaptioned PNG figures, a generic figure, a real table and a
  listing, with their figure/table/caption paragraph styles.
- A labelled editable equation in the Equation/Normal Table number frame.
- All five standard callouts, semantic custom `Callout Example`, a base Callout
  paragraph through a custom-style Div, notebook prompt/output, bibliography,
  horizontal rule and footer PAGE field.

`docx::styles::VOCABULARY` remains the authoritative name/kind inventory. Tests
require every one of its 101 entries, unique name/kind definitions, and exactly
one additional style: paragraph `Callout Example`. Case-insensitive lookup is
by style name and kind, independent of a reference's style IDs.

Actual `pStyle`/`rStyle`/`tblStyle` uses are checked across document, footnotes
and footer. All 82 reachable required built-ins are used. Exemptions are exactly:

- Eight token names that `SCOPE_TOKENS` never yields: VerbatimStringTok,
  OtherTok, AlertTok, InformationTok, WarningTok, CommentVarTok, ExtensionTok
  and RegionMarkerTok.
- Pure base/link targets: Normal, Default Paragraph Font, Body Text Char,
  Title Char, Subtitle Char, and Heading 1 Char through Heading 6 Char.

Verbatim Char is required and used; a blanket `* Char` exemption would be
incorrect. Token scopes and exemptions were not expanded to obtain coverage.
Full package and style-use snapshots retain the actual references for review.

## CLI contract

`writer eject docx-reference FILE` generates complete bytes before creating
paths. It creates missing parent directories, then uses `create_new` to prevent
clobbering even if concurrent invocations race. Existing files, directories,
symlinks and dangling symlinks are usage errors (exit 2) and remain unchanged.
Other I/O failures retain their ordinary error category.

Successful ejection reports `Written <path>` on stderr and prints usable
configuration on stdout:

```toml
[docx]
reference_doc = "<FILE>"
```

The path is serialized as a TOML string, preserving UTF-8 quotes, backslashes,
spaces and newlines. A path unrepresentable as UTF-8 is rejected before any
parent/file creation. Quiet suppresses status while retaining the functional
TOML snippet. Project config, explicit `--config` and environment timestamps do
not change built-in bytes. No force flag or new command-line syntax was added.
All other eject arms retain their T72 stub.

## Reproducible reference fixtures

Both binaries are generated from hand-written XML in `tests/docx_reference.rs`,
never copied from Word or Pandoc. Ignored regeneration tests reproduce each
binary exactly; ordinary tests compare committed bytes with the generators.

The upgraded reference retains its red heading 1, Callout fill, A5 body-level
section versus a discarded paragraph section break, rId100 header, settings,
numbering and dropped-content checks. It adds a native header PNG and a theme
picture fill with its own `word/theme/_rels/theme1.xml.rels` targeting
`../media/theme-fill.png`. Header/theme/media relationships are copied through
their transitive closure byte-for-byte, with required content types preserved.
Copied drawing ID 9001 and bookmark ID 12001 require every generated body ID
to start above the copied maxima. The rendered package remains validator-clean.

`reference-localized.docx` names heading 1 `heading 1` while its style ID is
`berschrift1`. Rendering reuses that ID and its red formatting without appending
a duplicate built-in Heading1 definition.

The optional Pandoc oracle invokes its actual
`--print-default-data-file=reference.docx` command. Pandoc 3.12.1 returns the ZIP
on stdout even when `--output` is supplied. The test checks successful status,
ZIP signature and required OPC parts, writes those exact bytes only to a
TempDir, then merges and independently reads the complete showcase. No Pandoc
reference is committed or used as the binary fixture source. Absence of Pandoc
is an explicit skip; a real merge failure is not normalized away.

## Verified validation and smoke evidence

The 2026-10-28 focused run passed 22 tests: 7 ordinary reference tests, 4
showcase tests and 11 CLI tests. Both explicit fixture regenerators remain
ignored during normal runs. The four dedicated smoke tests also passed, with
all local optional capabilities exercised rather than skipped. Configured
rustfmt, denied-warning all-target/all-feature Clippy, and default/feature docs
checks passed. This record does not claim final full debug/release totals.

Mandatory tests are portable and offline: structural V01–V20 validation,
independent read-back with image previews disabled, complete vocabulary/use
coverage, deterministic timestamp-free bytes, nonmutating writes, PNG identity,
editable numbered OMML, fixture reproduction, reference closure/ID preservation
and CLI destination safety. They need no Word, external Typst, installed fonts,
browser, TeX, Pandoc or network.

Actual local evidence was collected on macOS 26.7 (25G229):

| Check | Observed result |
|---|---|
| Independent DOCX reader | 81 top-level paragraphs, 102 styles, 2 abstract numbering definitions, 1 shared media part |
| Package XML and bundled python-docx 1.2.0 | Independently agree on 81 paragraphs, 2 tables and 2 inline drawings |
| System textutil | Actual text import succeeds and retains all 12 showcase heading samples |
| System QuickLook | Actual thumbnail generation succeeds; retained first-page PNG was inspected |
| Pandoc 3.12.1 | Actual default reference merges, validates and reads back |

The default `/usr/local/bin/python3` lacks python-docx. The successful isolated
Python check used the existing bundled interpreter selected through
`WRITER_DOCX_PYTHON`; no package was installed and no PYTHONPATH workaround was
used. Subprocesses have bounded waits and retain status/stdout/stderr. Missing
capabilities produce explicit reasons; a real document import failure fails.

Local artifacts are retained under
`target/task-prep/crash-recovery/t43/smoke/`, including the reviewed ejection,
text import, isolated Python record and QuickLook thumbnail. Exact commands,
versions, directories and results are in `docs/dev/docx-validation.md`.

## Manual release boundary

XML validation, independent reading and a QuickLook thumbnail do not establish
Word pagination, editing or field-update behaviour. No osascript/Word automation
was used. The following release checks remain explicitly unperformed in Word
for Mac/Windows, LibreOffice and Google Docs:

- Open examples, showcase and numbering.md without repair prompts.
- Verify update_fields true/false prompt behaviour and updated TOC/page refs;
  abstract and notoc headings remain excluded from the TOC.
- Preserve figure/table/listing/equation caches under both numbering policies,
  headings-off section fallback, subfigures and unnumbered/numbered-notoc H1;
  NOTEREF remains superscript and nested footnotes retain their numbers.
- Verify internal/TOC links, crisp SVG/older-viewer PNG fallback, editable OMML
  and the visual baseline of forced equation pictures.
- Check list restarts, tasks, Symbol/Courier New/Wingdings bullets, callout boxes
  and syntax colours.
- Restyle ejected heading colour and callout fill, rebuild with that reference,
  and verify formatting, header/footer and page size carry through.

Use the unperformed checklist in `docs/dev/docx-validation.md` as the release
record, with actual application versions and outcomes. These human checks do
not block shipping the implemented generator, tests and documentation, and are
not claimed as completed by automated results.
