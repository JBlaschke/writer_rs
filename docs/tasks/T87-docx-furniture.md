# T87 — Page furniture in DOCX (header and footer parts)

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T85 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `src/docx/design.rs` (`header_part`, `footer_part`, `furniture_paragraphs`), `src/docx/reference.rs` (part numbering and references for generated parts), `src/docx/package.rs` (writing the parts, rels and media), `tests/docx_furniture.rs` (new), snapshots `tests/snapshots/docx/design-furniture*` |

## Goal
Declared header and footer slots become `word/header<k>.xml` / `word/footer<k>.xml` parts with
their relationships and media: an inline logo drawing, text lines with right-tab columns, a
border rule, `PAGE`/`NUMPAGES` fields, a first-page part under `w:titlePg`. A declared footer
replaces the built-in page-number footer. A reference document's own parts still win.

## Read first
- `docs/spec/design.md` §9.1–9.2, §9.5, §7.2–7.3
- `docs/spec/docx-output.md` §1.2, §4.11 (fields), §4.16 (images and EMU), §5, §6.2–6.3 and the
  2026-11-01 notes; §7.1 validator rules for drawings and ids
- `src/docx/reference.rs` (footer part numbering, `Ids`), `src/docx/images.rs` (inline drawing
  builder), `src/docx/fields.rs`

## API to implement
```rust
// src/docx/design.rs
pub struct FurniturePart { pub name: String, pub xml: String, pub rels: Vec<Relationship>, pub media: Vec<(String, Vec<u8>)> }
pub fn header_part(furn: Option<&Furniture>, k: usize, ctx: &mut PartContext) -> FurniturePart;   // §9.5; empty part for None
pub fn footer_part(furn: Option<&Furniture>, k: usize, ctx: &mut PartContext) -> FurniturePart;
pub fn plan_parts(design: &Design) -> SectionRefs;    // which parts exist, first/default references, titlePg
```

## Steps
1. `furniture_paragraphs`: rule paragraphs (`w:pBdr` top/bottom, `w:sz` eighths, `w:color`), image
   paragraph (inline drawing from `images.rs` with EMU from width × aspect, `wp:docPr/@id` from
   `Ids.doc_pr`), line paragraphs (`Header`/`Footer` styles via `ensure`, `w:jc`, right tab at the
   text width with `w:tab` runs for `right_lines`, run properties from the resolved line values,
   fields for `{page}`/`{pages}`).
2. `plan_parts`: one part per distinct furniture per slot (`first` ≠ `rest` → two parts and
   `titlePg`); `k` from the smallest-free rule shared with the built-in footer.
3. `package.rs`: write parts, `word/_rels/header<k>.xml.rels`, `word/media/<name>`, content types
   (Override for the parts, Default by extension for media), `document.xml.rels` entries.
4. `reference.rs`: `section()` (T83) receives the planned references; no generation when the
   reference's body `sectPr` exists (design.md §7.3).
5. `tests/docx_furniture.rs`: the letter fixture — parts, rels, media and `sectPr` snapshots;
   validator (§7.1) and read-back (§7.2) pass; `NUMPAGES` present when a line uses `{pages}`; an
   empty footer slot yields an empty `footer<k>.xml` and no `PAGE` field; with a reference document
   nothing is generated and its parts are copied.

## Acceptance criteria
- Letter snapshot: `sectPr` has `headerReference type="first"` → `header1.xml`, `type="default"` →
  `header2.xml`, `footerReference type="default"` → `footer1.xml`, `w:titlePg`, `pgMar header=964 footer=567`;
  `header1.xml` holds a centred paragraph with a `wp:inline` drawing `cx="1800000"` (cy from the
  logo's aspect) and a centred run "Generative Biology Institute" `w:b` `w:color 'A96D4B'` `w:sz 22`;
  `header2.xml` the compact line at `w:sz 20`; `footer1.xml` a `pBdr/top single sz 6 color A96D4B`
  paragraph (0.25 mm = 0.709 pt → `w:sz="6"`) followed by four Footer paragraphs with a right tab at
  `w:pos="9412"`, the first bold at `w:sz 15`, the first two carrying the phone and email after `w:tab`.
- `tests/docx_package.rs` and the validator accept the package; Word read-back shows two headers.
- Without a design: no new parts, the built-in footer unchanged (`tests/docx_snapshots.rs`).
- Manual check recorded in `docs/dev/docx-validation.md`: the letter in Word for Mac shows the
  logo header on page 1, the compact header on page 2, the footer on both.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
`writer build --to docx letter.md` without a reference document produces the full letterhead in
Word: logo and institute line on page 1, compact header afterwards, legal footer with copper rule
and right-aligned phone and email on every page. `theme/reference.docx` can be deleted from the
Letters template.

## Out of scope
HTML and Typst furniture (T86, T88), font embedding (open point 1), background colour.

## Hints (verified)
- Letters `reference.docx` (read 2026-11-01): first header = centred inline drawing extent
  `1800000×288517` EMU + centred bold `A96D4B` sz 22 run; default header = centred bold `A96D4B`
  sz 20; footer = `pBdr/bottom single sz 8 color A96D4B` paragraph with a `sz 2` run, then Footer
  paragraphs with `w:tabs/w:tab w:val="right" w:pos="9411"`. Text width = 11906 − 1247 − 1247 = 9412.
- Inline drawings: `src/docx/images.rs` builds `wp:inline` for figures with `wp:docPr/@id` from
  `Ids.doc_pr` (docx-output.md §4.16); header media go in `word/media/` and the part's own rels.
- Fields: `docx::xml::field(instr, result_runs)` (docx-output.md §4.11); `PAGE` and `NUMPAGES`
  cached results `1`.
- Part numbering: `word/footer{k}.xml` with k the smallest ≥ 1 not taken by a copied part
  (docx-output.md §6.2); extend the same rule to headers.
- EMU: 1 pt = 12700 EMU; 50 mm = 141.732 pt = 1800000 EMU.
