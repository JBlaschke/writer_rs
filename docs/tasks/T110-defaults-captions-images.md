# T110 — Captions, figures and images

**Status: brief, not ready for hand-off.** The shared size function and the caption gaps are the
coordinator's to prototype first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T109, T120 |
| Parallel-safe with | nothing: the only task of wave 30 |
| Files you own | `src/design/defaults/image.toml` and the caption styles of `src/design/defaults/styles.toml`, with their bits in `APPLIED`; `assets/themes/default/styles/theme.css:64-67` and :171-176; `lib.typ`'s figure and caption rules; a shared natural-size module moved out of `src/docx/images.rs:336-438`, and `src/docx/images.rs`; `src/html/emit.rs` and `src/typst/emit.rs` (image sizes, block and inline); `src/latex/emit/figure.rs` (the image width); `assets/docx/styles.xml` (Caption, Table Caption, Image Caption, Captioned Figure); the snapshots that change |

## Goal
An image has one natural size in every format: the file's DPI, else 96 dpi, computed once by the
function writer already has for Word and written out explicitly by each backend (R13 §3.4 M6):
HTML in px (inches × 96, `max-width: 100%`), Typst as the smaller of the natural width and the
region's (`layout(region => image(.., width: calc.min(W, region.width)))` for a block image, the
same inside the inline box), Word in EMU as today, LaTeX as the smaller of the natural width and
`\linewidth`. Typst clamps an image to its region only when the width is auto, so an explicit
`image(width: W)` wider than the text would overflow the page; the other formats already clamp. Captions take
size, colour, label weight, alignment and spacing from the built-in caption styles.

## Read first
- `docs/spec/design.md` §0.4, §8.7 (captions)
- `docs/research/R13-single-source-defaults.md` §2.3, §3.4 M6, §4.3 (rows Caption, Figure,
  Image)

## API to implement
Set at hand-off: the module's path and its function's signature come from the coordinator's
prototype of the move.

## Steps
1. Move the size function out of `src/docx` and call it from the four backends.
2. Add the image and caption defaults and switch their bits on.
3. Remove the duplicated values from `theme.css` and the package.
4. Accept the snapshots after reading them.

## Acceptance criteria
1. An untagged 400 × 200 px image is 300 pt wide in all three formats; a 600 × 300 px PNG with
   pHYs at 300 dpi is 144 pt wide.
2. An image wider than the text block fills the text width in all four formats.
3. Caption size, colour and alignment are equal in all three; PDF table captions are
   left-aligned.
4. Body → caption and caption → body: Word ↔ PDF within 0.3 pt (T102's harness).
5. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- EXIF: writer reads none today and Typst does; reading it would be new (R13 §3.4 M6).
- The LaTeX template and the rest of `src/latex/` (T120).

## Hints (verified)
- `src/docx/images.rs` at main: `pixel_size` (:336), `valid_dpi` (:363; x and y separately,
  10 to 2400 dpi), `png_dpi` (:371), `jpeg_dpi` (:398-438).
- Typst takes the DPI from EXIF first, then JPEG or PNG metadata (`determine_dpi`, typst-library
  0.15.1 `src/visualize/image/raster.rs:366-374`), else `Image::DEFAULT_DPI` = 72
  (`src/visualize/image/mod.rs:406`).
- `src/html/emit.rs`: `image` (:607-633) writes a `style` width or height only from the image's
  own attributes. `src/typst/emit.rs`: `image` (:1386-1406) writes `image(..)`, and :1156-1160
  wrap an inline one in `#box(..)`. `src/latex/emit/figure.rs:247` writes `\includegraphics`.
- Word clamps to the text width already (`src/docx/images.rs:51`, :55, :168); HTML through
  `max-width: 100%`.
- `theme.css` at main: `img` :64-67, figures and captions :171-176 (`figcaption` .9em at :175).
- `styles.xml` at main: Caption :33, Table Caption :34, Image Caption :35, Captioned Figure :37.

## Before hand-off
- The coordinator moves the size function in a prototype, chooses the module's path, and
  measures the caption gaps.
- T103's caption readings are in (this task does not depend on T103; the coordinator checks).
