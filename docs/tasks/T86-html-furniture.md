# T86 — Page furniture in HTML (screen shell and print margin boxes)

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T85 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `src/html/design.rs` (`furniture_css`, `furniture_html`), `src/html/document.rs` (`design_furniture` helper registration), `assets/themes/default/templates/document.html` (two helper calls), `tests/html_furniture.rs` (new), snapshots `tests/snapshots/html/design-furniture*` |

## Goal
A declared header or footer slot renders in the HTML page: on screen as furniture blocks the
default theme's `document.html` places around `html.main` through the `design_furniture` helper,
and in print as `@page` margin boxes (first-page and rest variants, images as backgrounds,
`counter(page)`/`counter(pages)` placeholders) or, with `print = "fixed"`, as fixed-position copies.

## Read first
- `docs/spec/design.md` §9.1–9.3, §3.2 (furniture defaults), §5.3 item 4
- `docs/spec/html-output.md` §4.2–4.4, §7 and their 2026-11-01 notes; §6.3 (budget: the default
  template must stay within it)
- `src/html/document.rs` (`register_helpers`, `theme_style`), `assets/themes/default/templates/document.html`

## API to implement
```rust
// src/html/design.rs
pub fn furniture_html(design: &Design, slot: Slot, variant: Variant, registry: &mut AssetRegistry, targets: &Targets) -> String;  // §9.3 screen markup or ""
pub fn furniture_css(design: &Design, registry: &mut AssetRegistry, targets: &Targets) -> Result<String>;  // §9.3 screen rules + @page boxes or fixed rules; appended by layout_css (item 4)
// src/html/document.rs: minijinja function `design_furniture(slot: str, variant: str) -> safe HTML`
```

## Steps
1. `furniture_html` per §9.3 (classes, `data-align`, inline `style` with set values only, image
   registered as an asset, placeholders → `1`).
2. `furniture_css`: screen grid rules and `@media print { .w-furniture { display: none } }`; for
   `margin-boxes`: per slot and variant the `@page`/`@page :first` boxes (`content:` with `\A`
   between lines and `counter()` segments, `white-space: pre-line`, font/colour/alignment,
   `margin-top`/`margin-bottom` offsets, image as `background` with `padding-top` from the pixel
   size and width via `imagesize`, rule as border; `content: none` for empty furniture); for
   `fixed`: the `rest` copy positioned, the `first` copy in flow.
3. `register_helpers`: `design_furniture(slot, variant)` validating its arguments (a bad value is
   a template error naming the helper).
4. `document.html` of the default theme: `{{ design_furniture("header", "first") }}` before and
   `{{ design_furniture("footer", "first") }}` after `html.main`; confirm the rendered default
   output is unchanged and the §6.3 budget holds.
5. `tests/html_furniture.rs`: the letter fixture's screen markup and print CSS snapshots; a Chrome
   print-to-PDF check when Chrome is available (the repo's `browser.rs` harness): page 1 shows the
   logo and institute line, page 2 the compact header, both pages the footer with the rule; a
   `print = "fixed"` fixture; an empty footer slot removes the default theme's page number.

## Acceptance criteria
- Snapshot: the letter's `layout_css` contains `@page :first{@top-center{content:"Generative Biology Institute";background:url(asset:…) no-repeat top center/141.732pt auto;padding-top:<logo height>;font:600 11pt/1.35 "Source Sans 3",system-ui,sans-serif;color:#a96d4b;vertical-align:top;margin-top:48.189pt}}`
  (whitespace aside; the logo height is width × the image's aspect ratio), `@page{@top-center{content:"EIT Oxford | Generative Biology Institute";…}}`,
  `@page{@bottom-left{content:"Ellison Institute of Technology, Oxford Limited\A Registered Office: …";border-top:0.709pt solid #a96d4b;…}@bottom-right{content:"+44 1865 649525\A oxford@eit.org";…}}`.
- Screen: `document.html` output holds `<div class="w-furniture w-header w-header-first" …>` with
  the `<img>` and one `<p class="w-furniture-line">` before `<main`, and the footer block after it;
  a build with no design renders the default theme byte-identically (`tests/default_theme.rs`).
- Print check (when Chrome is present; otherwise documented as a manual check in the PR): a
  two-page letter printed to PDF has different headers on pages 1 and 2 and the footer on both.
- Placeholders: a footer line `Page {page} of {pages}` becomes `content:"Page " counter(page) " of " counter(pages)`.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
The HTML letter shows the logo and "Generative Biology Institute" above the text and the legal
footer with its copper rule below it on screen, and prints with the first-page letterhead, the
compact continuation header and the footer on every page, all from `[design.page]`; the theme's
`document.html` shrinks to the sheet and `html.main`, and `letter.css` keeps only the screen sheet.

## Out of scope
Typst furniture (T88), DOCX furniture (T87), per-line styling inside margin boxes (documented
limitation), inline markup in lines (open point 6).

## Hints (verified)
- Chromium repeats `position: fixed` elements on every printed page and supports `@page :first`
  and the 16 margin boxes with `counter(page)`/`counter(pages)`; margin boxes take `content`, font,
  colour, borders, padding, `vertical-align` and `margin`, and size themselves from their content.
- `imagesize::blob_size(bytes)` (`imagesize 0.15.0`, used in `src/docx/images.rs` `pixel_size`)
  gives the raster size for the `padding-top` computation (height = width × h/w).
- `register_helpers` in `src/html/document.rs` adds `theme_style`; add `design_furniture` the same
  way (it needs the registry and targets: capture them like `theme_style` captures the compiled CSS).
- `tests/browser.rs` drives Chrome through the DevTools protocol; one-shot `--print-to-pdf` hangs
  on this machine (see `docs/dev/live-reload-qa.md` and the memory note), so reuse that harness.
