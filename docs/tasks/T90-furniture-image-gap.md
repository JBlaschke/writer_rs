# T90 — Furniture image `gap` (space between a furniture image and its lines)

| | |
|---|---|
| Milestone | M8 |
| Size | S |
| Depends on | T89 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `src/design/mod.rs` (`FurnitureImage.gap`, `ImageDetailsSpec.gap`), `src/design/resolve.rs` (`image`), `src/design/template.toml`, `src/html/design.rs` (`furniture_html`, `margin_box`), `src/docx/furniture.rs` (`image_paragraph`), `assets/typst/writer-doc/lib.typ` (`writer-furniture`), `docs/spec/design.md` §2.2, §2.3, §3.2, §3.3, §9, §11, `docs/guide/configuration.md`, `docs/guide/customization.md`, `tests/{design_config,html_furniture,docx_furniture,typst_furniture}.rs` and the furniture snapshots |

## Goal
A furniture image is followed by a declared amount of space before the lines (or the rule) under
it, the same in HTML, PDF and Word. The Letters letterhead's institute line no longer touches the
logo in the PDF: `writer-furniture` stacked the image and the lines with `par(spacing: 0pt)` and
Typst's cap-height top edge, so the first capitals started on the image's bottom edge, while HTML
had the grid's zero row gap and Word whatever paragraph spacing the image paragraph inherited.

## Read first
- `docs/spec/design.md` §2.2 (`image` table), §3.2 (derived defaults), §9.1 (model), §9.3–9.5
  (per backend), §11 (the letter's geometry row for the gap)
- `assets/typst/writer-doc/lib.typ` `writer-furniture`; `src/html/design.rs` `furniture_html`
  and `margin_box`; `src/docx/furniture.rs` `image_paragraph`

## API to implement
```rust
// src/design/mod.rs
spec!(ImageDetailsSpec { src: String, width: Option<String>, align: Option<String>, gap: Option<String> });
pub struct FurnitureImage { pub path: PathBuf, pub width: Option<Pt>, pub align: Option<Align>, pub gap: Option<Pt> }
// src/design/resolve.rs: `gap` = the table's Length, else `text.paragraph_spacing`, else None
```
```typst
#let writer-furniture(spec, text-width, spacing: 1.2em) = context { … }   // spacing: settings.spacing
```

## Steps
1. `src/design/mod.rs`, `resolve.rs`: the `gap` key (Length) on the image table; the string form
   and a table without `gap` take `text.paragraph_spacing`; `template.toml` documents it.
2. `lib.typ`: `block(below: gap, align(.., image(..)))`, `gap` = the resolved value or the
   `spacing` argument (`settings.spacing`, i.e. the design's or `writer-doc`'s paragraph spacing).
3. `src/html/design.rs`: `margin-bottom` on the screen `<img>` (`1em` when unset); the print margin
   box's `padding-top` grows by the gap (`calc(<height> + 1em)` when unset).
4. `src/docx/furniture.rs`: `w:spacing w:after="<twips>"` on the image paragraph when the gap is
   resolved; otherwise the paragraph keeps the document defaults.
5. Spec §2.2, §2.3, §3.2, §3.3, §9.1, §9.3–9.5, §11; guide `configuration.md` key table and
   `customization.md` prose.
6. Tests: resolution (string form, explicit gap, invalid gap, no paragraph spacing); HTML markup
   and margin boxes; DOCX `w:after`; Typst frame geometry (image bottom to the first line's cap
   height equals the gap); refreshed snapshots.

## Acceptance criteria
- `tests/design_config.rs`: `image = "logo.png"` with `paragraph_spacing = "3.6mm"` resolves to
  `gap: 10.205`; `gap = "3mm"` wins; without `paragraph_spacing` the gap is `null`; `gap = "3"`
  reports `design.page.header.image.gap`.
- `tests/html_furniture.rs`: the letter's screen header carries `margin-bottom:10.205pt` and its
  `@page :first` box `padding-top:151.937pt`; an image without paragraph spacing gets `1em` /
  `calc(… + 1em)`.
- `tests/docx_furniture.rs`: the letter's image paragraph has `<w:spacing w:after="204"/>`; an
  explicit 3 mm gap gives `170`.
- `tests/typst_furniture.rs`: the distance from the image's bottom edge to the first line's cap
  height equals the gap within 0.3 pt for the default (paragraph spacing), an explicit gap and the
  no-design-spacing fallback (1.2em).
- `tests/typst_furniture.rs`: the letter's logo stays centred on the page.
- The Letters design letter (outside the repo) built with the release binary shows the institute
  line's capitals 3.6 mm below the logo in the PDF with no new key; with `gap = "3mm"` they sit
  3 mm below it, the HTML line box starts 3 mm below the logo and the Word image paragraph has
  `w:after="170"`, which moves Word's line up by the same 0.6 mm (manual check: `gs -r100` and
  `-r300` rasters, the in-app browser, a PDF exported by Word).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
`writer build letter.md` on the Letters design template puts "Generative Biology Institute" one
paragraph space below the logo in all three outputs without any new key; `gap = "4mm"` reproduces
the hand-written `letterhead.typ` exactly.

## Out of scope
A furniture `line_height` (open point 7); matching the backends' line-box models exactly (the
visible gap differs by the font's ascender-to-cap-height distance and Word's leading placement,
§9.1); inline markup in lines (open point 6).

## Hints (verified)
- `block(below: g)` followed by a paragraph under `par(spacing: 0pt)` yields exactly `g`
  (`tests/typst_furniture.rs` and the Letters letter, 2 Nov 2026). A `block` without a width
  shrinks to its content, so an `align` inside it has no room to act: the image block needs
  `width: 100%`, or a centred logo moves to the left margin (seen on the Letters letter during
  this task). `top-edge` defaults to `"cap-height"`, so the gap ends at the first
  capitals (`typst-library` `TextElem::top_edge`).
- Word adds the extra "multiple" line spacing above a line and keeps the image paragraph's
  descent below the picture: the letter's Word export puts the line's capitals 7.0 mm below the
  logo for `w:after="204"` (3.6 mm) and 6.4 mm for `170` (3 mm). The HTML screen header puts them
  1.1 mm below the line box's top (Word export, `gs -r300`, in-app browser; 2 Nov 2026).
- The hand-made Letters references use `.institute-name { margin-top: 3mm }` (HTML),
  `w:spacing w:after="120"` on the logo paragraph (`reference.docx`) and a line placed 4 mm below
  the logo (`letterhead.typ`).
