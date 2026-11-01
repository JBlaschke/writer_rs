# T83 — DOCX generated styles and section properties from the design

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T80 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `src/docx/design.rs` (new), `tests/docx_design.rs` (new), `tests/snapshots/docx/design-*.xml` (new); additively: `src/docx/styles.rs` (`StyleTable::generated`), `src/docx/reference.rs` (`builtin_section` from the design), `src/docx/mod.rs` (`DocxOptions.design`), `src/pipeline/docx.rs`, `src/check/design.rs` (`design-shadowed` for references) |

## Goal
Without a reference document, `styles.xml` and the body `sectPr` are generated from the design:
docDefaults fonts, size, colour, line spacing, paragraph spacing and justification; heading and
code fonts; the Hyperlink colour; page size, margins and header/footer distances. With no design
the generated XML is byte-identical to the static assets. A reference document keeps winning for
everything it defines, and `check` says what it shadows.

## Read first
- `docs/spec/design.md` §7.1–7.4, §0.2, §2.1 (twips, half-points)
- `docs/spec/docx-output.md` §2.1–2.3, §5, §6.2–6.3 and their 2026-11-01 notes
- `src/docx/styles.rs` (`builtin`, `from_reference`, `set_lang`), `src/docx/reference.rs`
  (`builtin_section`, `page_layout`), `src/docx/mod.rs` (`DocxOptions`, the `config.pdf.paper` use at ~305)

## API to implement
```rust
// src/docx/styles.rs
impl StyleTable {
    pub fn builtin() -> StyleTable { Self::generated(&Design::default()) }
    pub fn generated(design: &Design) -> StyleTable;          // design.md §7.1
}
// src/docx/design.rs
pub fn apply_tokens(styles_xml: &str, design: &Design) -> String;   // the §7.1 edit table, via docx::xml, child order kept
pub fn section(design: &Design, paper: &str, header_refs: &[(SectionRefKind, String)], footer_refs: &[...]) -> String;  // design.md §7.2
pub fn twips(pt: Pt) -> u32;  pub fn half_points(pt: Pt) -> u32;  pub fn eighths(pt: Pt) -> u32;
// src/docx/mod.rs
pub struct DocxOptions<'a> { …, pub design: &'a Design }
```

## Steps
1. `src/docx/design.rs`: unit conversions; `apply_tokens` editing the parsed built-in XML with
   `docx::xml` (rPrDefault fonts/sz/color, pPrDefault spacing/jc, every `Calibri Light` and
   `Consolas` `w:rFonts`, relative `w:sz` scaling for the §2.3 styles listed, Hyperlink colour).
2. `StyleTable::generated` calling `apply_tokens`; `builtin()` delegates; `from_reference` applies
   §7.1 edits only to the writer definitions it appends for missing names (design.md §7.3 row 2).
3. `section`: `pgSz` from `design.page.paper` else `[pdf] paper`, `pgMar` sides and `header`/`footer`
   distances, `gutter` 0, `titlePg` and the reference list (T87 supplies real parts; here the
   built-in `PAGE` footer reference when no footer slot is declared, no footer when one is).
4. `reference.rs`: `builtin_section` takes the design; the no-reference and
   reference-without-body-`sectPr` paths use it; text width/height computations follow.
5. `DocxOptions.design` and the pipeline wiring from `Prepared.design`.
6. `src/check/design.rs`: `design-shadowed` for `--target docx` when a reference has a body-level
   `sectPr` (names "page setup", "header/footer") and for each design style name the reference
   defines (names "styles: …").
7. `tests/docx_design.rs`: byte-equality of `generated(&Design::default()).to_xml()` with
   `assets/docx/styles.xml`; the letter fixture's `styles.xml` and `sectPr` snapshots; a reference
   document (reuse `tests/fixtures/docx/*.docx` or `eject docx-reference`) with the letter design:
   its docDefaults and `sectPr` survive, `check` lists what it shadows.

## Acceptance criteria
- `tests/docx_design.rs`: default equality test; the letter's `rPrDefault` is
  `<w:rFonts w:ascii="Source Sans 3" w:hAnsi="Source Sans 3" w:cs="Source Sans 3"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:color w:val="242424"/><w:lang w:val="en-GB"/>`,
  its `pPrDefault` `<w:spacing w:after="204" w:line="324" w:lineRule="auto"/>`, Heading 1 names
  Source Sans 3, and `sectPr` is `pgSz 11906×16838`, `pgMar top=2835 right=1247 bottom=2154 left=1247 header=964 footer=567 gutter=0`
  with a `default` footer reference to the `PAGE` footer (no furniture yet); the validator of §7.1
  passes and Word's read-back helper (§7.2) opens it.
- With a reference document: `sectPr` and docDefaults equal the reference's; `check --target docx`
  reports `design-shadowed` naming "page setup".
- `tests/docx_snapshots.rs`, `docx_reference.rs`, `docx_smoke.rs` unchanged without a design.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
`writer build --to docx letter.md` without `reference_doc` opens in Word with Source Sans 3 11 pt,
1.35 line spacing, 3.6 mm after paragraphs, ink text, A4 with 50/22/38 mm margins and the
header/footer distances; the page still has writer's centred page-number footer and no
letterhead (T87), and `Recipient`, `Closing` … are plain Body Text (T85). Keeping
`reference_doc = "theme/reference.docx"` leaves the document exactly as today and `check` says why.

## Out of scope
Appended `[design.styles]` definitions and the class route (T85), header/footer parts and media
(T87), font embedding (open point 1), `colors.background` (open point 3).

## Hints (verified)
- `assets/docx/styles.xml` docDefaults: `Calibri` sz 22, `pPrDefault` `after=0 line=264 auto`;
  headings use `Calibri Light`, code styles `Consolas` (docx-output.md §2.2).
- `builtin_section(width, height, footer_rel)` is at `src/docx/reference.rs` ~412; the paper table
  (a4 11906×16838, a5 8391×11906, us-letter 12240×15840, us-legal 12240×20160) is in docx-output.md §6.3.
- `DocxOptions` (`src/docx/mod.rs` 67–76) reads `opts.config.pdf.paper` at ~305 for the page.
- Letters `reference.docx` geometry for comparison: `pgMar top=2835 right=1247 bottom=2154 left=1247 header=964 footer=737`
  (the design's footer offset of 10 mm gives 567).
- `docx::xml` ordered builders (docx-output.md §5) keep `w:pPr`/`w:rPr` child order; the validator
  (`src/docx/validate.rs`) rejects misordered children, so edit through them.
