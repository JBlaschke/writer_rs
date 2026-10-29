# DOCX validation

Writer checks the generated package structurally and reads it with an independent
DOCX library. Platform imports and thumbnails add useful evidence. Rendering and
interaction in Word still require the manual release checks below.

## Automated checks

The mandatory tests run offline on every supported operating system. They need
no Word, Typst executable, installed fonts, browser, TeX or network:

- `docx::validate` checks V01–V20: deterministic ZIP metadata, XML, parts and
  relationships, child order, sections, bookmarks, fields, unique drawing IDs,
  notes, list definitions, style references, table grids, preserved whitespace,
  media signatures and timestamps.
- `docx-rs` independently opens generated files with image previews disabled.
  Snapshots record paragraph, style, abstract-number and image counts. The
  showcase smoke test also compares its counts with separately counted XML
  elements and media parts.
- Reference tests preserve the original style definitions and copied header,
  theme and transitive media relationships. Localized style IDs resolve by
  their style names. Copied drawing/bookmark IDs remain disjoint from body IDs.
- The built-in style showcase contains every vocabulary style and exercises
  every reachable style. Exemptions are the documented eight token styles
  without an emitting scope and pure style base/link targets. Its registered
  grey PNG is fixed at 64×32. Its numbered equation is editable OMML.
- The ejected reference is deterministic and has no created/modified timestamp.
  CLI tests check usable TOML guidance, quiet mode, unusual UTF-8 paths,
  configuration/environment independence, no-clobber races and preservation
  of existing destinations, including symlinks.

Run the focused suites from the repository root:

```sh
cargo test --locked --offline --test docx_showcase --test docx_reference
cargo test --locked --offline --test docx_smoke -- --nocapture
cargo test --locked --offline --test cli_eject_docx
```

The shared DOCX package, snapshots, fields, tables, images, math and pipeline
tests remain part of the complete debug/release suites. XML/read-back success
does not establish page layout, field updates or Word's editing behaviour.

## Optional imports and thumbnails

`tests/docx_smoke.rs` detects each executable and module separately. It prints
`skipping` with the actual reason when the capability is unavailable. It never
installs Python packages and never starts Word or uses osascript.

Each subprocess has a 30-second deadline, drains stdout/stderr while running,
and is killed and reaped on expiry. A missing tool/module is a skip. An actual
DOCX import failure is a test failure. QuickLook failures are skipped only when
the captured message identifies an unavailable desktop, broker or permission
capability; other failures require investigation.

The tests exercise these commands against the generated showcase:

```sh
textutil -convert txt -stdout showcase.docx
python3 -I -c 'import docx,sys; d=docx.Document(sys.argv[1]); print(len(d.paragraphs), len(d.tables), len(d.inline_shapes))' showcase.docx
qlmanage -t -s 1000 -o ARTIFACT_DIRECTORY showcase.docx
```

The Python check first probes the isolated `docx` import. Set
`WRITER_DOCX_PYTHON` to an existing virtual environment's Python executable when
needed. The process still uses `-I`; no `PYTHONPATH` workaround or automatic
installation is used. The expected paragraph/table/inline-drawing triple is
fixed in the test after reviewing the showcase, rather than inferred from a
successful Python result.

Artifacts are retained under
`target/task-prep/crash-recovery/t43/smoke/<tool>-<process-id>/`. Each case saves
`showcase.docx`, separate stdout/stderr/status files, and any QuickLook PNG.
The status records timeouts separately from exit status. A retained thumbnail
confirms thumbnail generation; it does not establish a Word layout pass.

The reference suite may additionally generate Pandoc's default reference at
test time, use it to style the showcase, validate the result and read it back.
Pandoc's GPL reference is never committed or used as Writer's fixture source.

## T43 validation record

Checked locally on 2026-10-28 on macOS 26.7 (25G229), with its system
textutil/QuickLook, Pandoc 3.12.1 and python-docx 1.2.0. These checks do not
replace the manual Word checks in the following section.

| Check | Evidence |
|---|---|
| Style inventory and actual use | All 101 built-ins present; all 82 reachable required styles used, plus the semantic Callout Example style. Only the agreed base/link and eight token exemptions apply. |
| Reference/CLI/showcase | 22 focused tests pass, including no-clobber races, directory/symlink refusal, TOML guidance and deterministic config/environment-independent bytes. |
| Hand-written fixtures | Both reproducible binaries regenerated; red heading/callout/A5/header behavior, theme/header image closure, copied IDs and localized berschrift1 pass. |
| Independent read-back | 81 top-level paragraphs, 102 styles, 2 abstract numbering definitions, 1 shared media part. XML and python-docx agree on 81 paragraphs, 2 tables and 2 inline drawings. |
| textutil | Actual `/usr/bin/textutil -convert txt -stdout` succeeds and retains all 12 showcase headings. |
| Python-docx | Bundled isolated Python with python-docx 1.2.0 successfully reads the actual file; reviewed counts are `(81, 2, 2)`. The default Python still lacks the module. |
| QuickLook | Actual `/usr/bin/qlmanage -t -s 1000` succeeds; retained first-page PNG was viewed and its title, TOC and heading samples are legible. |
| Pandoc reference | Actual Pandoc 3.12.1 default reference ZIP was captured from stdout into a temporary file; the complete showcase merges, validates and reads back. No Pandoc reference is committed. |
| Complete suites | 1,529 debug and 1,528 release tests pass; formatting, all-target/all-feature Clippy and denied-warning default/feature rustdoc pass. |
| Word, LibreOffice, Google Docs | Manual checks below have not been performed. |

The focused smoke run used `WRITER_DOCX_PYTHON` pointing to the bundled runtime
reported by the Codex dependency tool. No Python package was installed. All four
smoke tests passed with actual tool capabilities, including the Python import
and QuickLook preview; none of these local smoke checks was skipped.

Retained local evidence is under:

- `target/task-prep/crash-recovery/t43/smoke/reviewed-jtttk9gy/`: actual ejected
  reference, TOML guidance and independent Python/XML count record.
- `target/task-prep/crash-recovery/t43/smoke/textutil-76250/`: source document,
  command status/stdout/stderr and imported text.
- `target/task-prep/crash-recovery/t43/smoke/python-docx-76250/`: isolated module
  version and read-back output/status.
- `target/task-prep/crash-recovery/t43/smoke/quicklook-76250/showcase.docx.png`:
  inspected first-page thumbnail, alongside command evidence and source file.

Later full-suite runs may create additional per-process directories. These
artifacts are ignored local evidence; commands above reproduce them on another
machine. A QuickLook thumbnail does not establish Word pagination, field-update
interaction, editable-math behavior or the baseline of an equation picture.

## Manual release checklist

These checks are **not performed by the automated tests**. Before each release,
complete them in Word for Mac and Word for Windows. Use the examples, the style
showcase and `tests/fixtures/md/numbering.md`; record the application versions,
operating systems, date, files and outcomes alongside the release evidence.

- [ ] Every document opens without a repair prompt.
- [ ] With `[docx] update_fields = true`, Word gives one update-fields prompt.
      Accepting it populates the TOC and page-reference values. With `false`,
      no prompt appears.
- [ ] The updated TOC excludes abstract headings and every `notoc` heading.
- [ ] Figure, table, listing and equation numbers remain equal to their cached
      values after updates under document numbering and section numbering.
- [ ] Section numbering with headings disabled uses document numbering.
      Sub-figures and figures after unnumbered and numbered-`notoc` level-one
      headings keep the expected numbers.
- [ ] Repeated-note NOTEREF fields remain superscript; hoisted nested notes
      keep their numbers. Footnote links and backlinks behave correctly.
- [ ] Cross-reference links, internal links and TOC links jump to the correct
      target. Number bookmarks exclude surrounding equation parentheses.
- [ ] SVG figures remain sharp. An older viewer displays their PNG fallback.
- [ ] OMML equations can be edited as Word math. With `[docx] math = "image"`,
      inline equation pictures sit on the surrounding text baseline. The
      implemented descent formula still needs this visual verification.
- [ ] Ordered lists restart correctly; task boxes and Symbol/Courier New/
      Wingdings bullets display correctly.
- [ ] Adjacent callout paragraphs form one box. Code token colours and line
      numbers are readable, including in the style showcase.
- [ ] Eject a reference, change Heading 1 colour and Callout Note fill in Word,
      rebuild with that reference, and confirm the changes carry through.
      Check retained header/footer content and page size too.
- [ ] The generated files also open in LibreOffice and Google Docs.

Do not automate Word through osascript: previous research found that path hangs.
Structural checks, text imports, thumbnail generation and independent reading
remain separate evidence from these manual rendering and interaction checks.
