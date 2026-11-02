# T91 — Page background image (`[design.page.background]`)

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T86, T87, T88 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `docs/spec/design.md` (§1.3, §2.2, §2.3, §3.3, §5.3, §6.2, §7.2, §9.6 new, §10), `src/design/{mod,resolve,json,merge}.rs` (`Background*` additions), `src/design/template.toml`, `src/cli/eject.rs` (`rewrite_paths`), `src/html/design.rs` (`background_css`), `assets/typst/writer-doc/lib.typ` (`background-image`, `furniture-background`, `furniture-layout`), `src/docx/furniture.rs` (`background_run`, `with_anchor`, `AnchoredDrawing`), `src/docx/design.rs` (`page_size`, `first_page`), `src/docx/images.rs` (`picture_graphic`), `src/docx/mod.rs` (`wp:anchor` order), `docs/guide/configuration.md`, `docs/guide/customization.md`, `docs/guide/outputs.md`, `docs/dev/docx-validation.md`, tests `tests/{design_config,html_furniture,typst_furniture,docx_furniture}.rs` and their snapshots |

## Goal
A design-only theme can draw a photo cover: `[design.page.background]` names one image that every
backend paints at the page origin at full page size, behind the body and the furniture, on page one
(the cover case, default) or on every page. HTML, Typst PDF and Word read the same resolved value;
the first page of the EIT letter template becomes a full-bleed photograph in all three outputs
without theme CSS, a custom Typst template or a reference document.

## Read first
- `docs/spec/design.md` §2.2, §2.3, §3.3 (shape and JSON), §7.2 (section), §9.6 (the backends;
  "this is your spec"), §1.3 and §12 (paths are build inputs)
- `src/design/resolve.rs` (`slot`, `image`, `keys`), `src/design/merge.rs` (`absolutise_paths`,
  `clear_empty_furniture`), `src/design/json.rs` (`PathMode::Virtual`)
- `assets/typst/writer-doc/lib.typ` (`furniture-background`, `furniture-layout`)
- `src/html/design.rs` (`furniture_css`, `file_url`), `src/docx/furniture.rs` (`generate`,
  `plan_slot`, `image_paragraph`, `image_rel`), `src/docx/images.rs` (`InlineDrawing`)

## API to implement
```rust
// src/design/mod.rs
spec!(BackgroundSpec { image: Option<BackgroundImageSpec>, pages: Option<BackgroundPages> });
spec!(BackgroundImageDetailsSpec { src: String, fit: Option<Fit>, align: Option<String> });
pub enum BackgroundImageSpec { Path(String), Details(BackgroundImageDetailsSpec) }   // untagged
pub struct Background { pub image: BackgroundImage, pub pages: BackgroundPages }
pub struct BackgroundImage { pub path: PathBuf, pub fit: Fit, pub align: BackgroundAlign }
pub enum Fit { Cover, Contain, Stretch }                 // default Cover
pub enum BackgroundPages { First, All }                  // default First
pub struct BackgroundAlign { pub horizontal: Align, pub vertical: VerticalAlign }
pub enum VerticalAlign { Top, Center, Bottom }
// Page gains `pub background: Option<Background>` between `print` and `header`.

// src/html/design.rs
pub fn background_css(design: &Design, targets: &mut Targets<'_>) -> Result<String>;

// src/docx/design.rs
pub(crate) fn page_size(design: &Design, paper: &str) -> (u32, u32);   // twips
pub(crate) fn first_page(design: &Design) -> bool;                      // titlePg
```

## Steps
1. `docs/spec/design.md`: §2.2 block, §2.3 shapes, §3.3 JSON, §5.3 item 5, §6.2 row, §7.2
   `titlePg` rule, new §9.6, §10 wording, §1.3 paths.
2. `src/design/mod.rs`, `resolve.rs` (`design.page.background`, string or table image, `fit`,
   `align` with one or two keywords, `pages`; an empty table resolves to `None`), `merge.rs`
   (absolutise `page.background.image`; an empty table clears an inherited background),
   `json.rs` (virtual path), `template.toml`; `referenced_files`/`register_images` include the
   background so `check`, watch inputs, `eject --design` and the Typst World see it.
3. `lib.typ`: `background-image(spec)`; `furniture-background` places it first; `furniture-layout`
   sets `page(background:)` alone when only a background is declared.
4. `src/html/design.rs`: `background_css` appended by `layout_css`; `body::before` screen band and
   `@media print` absolute/fixed box.
5. `src/docx/furniture.rs`: `background_run` with an anchored `behindDoc` picture (`cover`
   crops with `a:srcRect`, `contain` shrinks the extent and offsets it), placed by `with_anchor`
   at the start of the part's first paragraph (or in a 0.7 pt paragraph of its own), header
   parts for an undeclared or shared slot, `generate` runs for a background alone;
   `design::section` sets `titlePg` for `pages = "first"`; `xml::ORDERS` learns `wp:anchor`;
   `images::picture_graphic` is the picture body shared with `InlineDrawing`.
6. Tests and snapshots; guide pages; validation record.

## Acceptance criteria
- `tests/design_config.rs::page_background_resolves_shorthand_fit_align_and_pages` (JSON shape,
  defaults, errors for bad `fit`, `pages`, `align` and unknown keys); the `design_letter` and
  `typst_design_input` snapshots gain `"background": null`.
- `tests/typst_furniture.rs::page_background_fills_the_first_page_or_every_page`: the image frame
  sits at the page origin with the page width on page 1 only (`first`) or on both pages (`all`),
  and page numbering stays when no slot is declared.
- `tests/html_furniture.rs::page_background_paints_the_first_sheet_and_prints_once_or_fixed`:
  the `body::before` rules, `cover`/`contain`/`stretch` sizes, alignment positions, the asset
  registration, and no CSS without a background.
- `tests/docx_furniture.rs::page_background_is_a_behind_text_picture_in_the_first_page_header`
  (snapshot of the header part; `titlePg`; no picture in the continuation header; `all` puts it
  in every header; an undeclared header slot gets a part of its own) passes the validator and
  the independent reader.
- Manual check in Word for Mac recorded in `docs/dev/docx-validation.md`: the photo fills page
  one behind the title block and no other page.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
Per-page-class backgrounds beyond first/all, colour-only backgrounds (`colors.background` is the
page fill), a bleed beyond the `@page` margins in HTML print (a theme CSS concern, documented),
and the reference-document path (§7.3 stands).

## Hints (verified)
- Word writes behind-text pictures as `wp:anchor behindDoc="1"` with `wp:positionH`/`wp:positionV`
  `relativeFrom="page"` and `wp:wrapNone`; the schema order of `CT_Anchor` is `simplePos,
  positionH, positionV, extent, effectExtent, wrap*, docPr, cNvGraphicFramePr, graphic`
  (ECMA-376 Part 1 §20.4.2.3). `a:srcRect` percentages are thousandths of a percent.
- EMU per twip: 635 (`914400 / 1440`); A4 is `11906 × 16838` twips = `7560310 × 10692130` EMU.
- Typst `image(fit:)` accepts `"cover"`, `"contain"` and `"stretch"` and centres the picture;
  `measure(image(path))` in a `context` gives the natural size, so an off-centre `cover` is an
  image with both dimensions set explicitly, placed at the aligned offset in a `box(clip: true)`
  (an image with only its height forced is capped at the region width and centred).
- Chromium confines absolutely and fixed positioned print boxes to the page area and repeats
  `position: fixed` boxes on every page (the furniture `fixed` mode relies on the same behaviour).
