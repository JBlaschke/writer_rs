# T88 — Page furniture in Typst (`writer-furniture`, `page(background:)`)

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T85 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `assets/typst/writer-doc/lib.typ` (`writer-furniture`, the `page(background:)` block in `writer-doc`), `tests/typst_furniture.rs` (new), `tests/snapshots/typst/design-furniture*`; additively `src/pipeline/pdf.rs` / `src/typst/world.rs` if image registration needs a hook T82 did not add |

## Goal
`writer-doc` places declared header and footer furniture on every page through
`page(background:)`: the first-page or rest variant by page number, image, lines with an optional
right column, rule and `{page}`/`{pages}` counters, at the design's offsets from the page edges.
A declared slot removes the package's default page numbering.

## Read first
- `docs/spec/design.md` §9.1–9.2, §9.4, §6.1 (virtual image paths), §3.2
- `docs/spec/typst-dialect.md` §4.2 and §6.3 with their 2026-11-01 notes; §8.3 (oracle) for tests
- `assets/typst/writer-doc/lib.typ` after T85; the Letters `typst/letterhead.typ` is the model
  (read-only, outside the repo; its structure is summarised in design.md §9.4)

## API to implement
```typst
#let writer-furniture(spec, text-width) = { … }     // design.md §9.4: content for one furniture dictionary
// inside writer-doc: when design.page.header or .footer is not none
//   set page(header: none, footer: none, numbering: none, background: context { … place(...) … })
```

## Steps
1. `writer-furniture`: rule above/below (`line(length: 100%, stroke: w * 1pt + rgb(c))`), image
   (`image(path, width:)`, aligned), lines and `right_lines` (`grid(columns: (1fr, auto))`, each
   column's lines joined by `linebreak()`, `text(size:, fill:, weight:, style:)` per line from the
   resolved values), placeholders → `counter(page).display()` / `counter(page).final().first()`.
2. The `page(background:)` block of §9.4 in `writer-doc`: variant by `counter(page).get().first()`,
   `place(top + left, dx: margin.left, dy: header-offset)` and `place(bottom + left, dx:
   margin.left, dy: -footer-offset)` with `block(width: text-width)`; offsets default to 30 % of
   the respective margin; `numbering: none`, `header: none`, `footer: none` when a slot is declared.
3. Ensure furniture images are registered in the `AssetRegistry` and reach the World as
   `/__assets/<id>.<ext>` (T82 registers them; verify the live-reload path too).
4. `tests/typst_furniture.rs`: the letter fixture's PDF via the oracle/`typst query`: page 1 has
   the logo image and the institute text at 17 mm, page 2 the compact line, both pages the footer
   rule and six lines 10 mm above the bottom; the page count text when a line uses `{pages}`;
   an empty footer slot gives pages without numbers.

## Acceptance criteria
- Oracle queries on the letter: the first page's `image` element is at `y ≈ 48.19pt` (±1 pt) and
  141.73 pt wide; the footer block's bottom edge is at `page height − 28.35pt` (±1 pt); the second
  page has no image and the text "EIT Oxford | Generative Biology Institute"; no page number
  appears.
- `{page} / {pages}` renders "1 / 2" and "2 / 2" on a two-page fixture.
- Documents without a design keep `writer-doc`'s `numbering: "1"` footer (`tests/pdf_pipeline.rs`).
- A BYO template delegating to `writer-doc` shows the furniture; one that does not import
  `@writer/doc` shows none and `check` says `design-shadowed` (T82).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
`writer build --to pdf letter.md` needs no `letterhead.typ`: the PDF carries the logo and institute
line on page 1, the compact header afterwards, the legal footer with the copper rule and the
right-aligned phone and email, Source Sans 3 from the theme's font files, and the recipient,
salutation, date, closing and signature spacing of the named styles. The Letters `typst/`
directory can be retired.

## Out of scope
HTML and DOCX furniture (T86, T87), page numbers only on multi-page documents (open point 8),
inline markup in lines (open point 6).

## Hints (verified)
- `place(alignment, dx:, dy:, body)` inside `page(background:)` positions relative to the page,
  which is how the Letters `letterhead.typ` sets its logo at `top + center, dy: 17mm` and its footer
  at `bottom + left, dx: 22mm, dy: -11mm`.
- `counter(page).get().first()` needs `context`; `counter(page).final().first()` gives the page
  count (both used by the Letters file).
- `grid(columns: (1fr, 39mm))` is the Letters footer layout; the design uses `(1fr, auto)`.
- The oracle test harness (typst-dialect.md §8.3, `tests/pdf_oracle.rs`) queries element positions
  from the compiled document.
