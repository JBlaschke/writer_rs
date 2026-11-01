# T85 — Named paragraph styles in HTML, Typst and DOCX

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T81, T83, T84 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `src/html/design.rs` (`styles_css`, from T81), `src/html/emit.rs` (`data-style`), `assets/typst/writer-doc/lib.typ` (`writer-div` body), `src/docx/design.rs` (`style_xml`), `src/docx/styles.rs` (append design styles in `generated`/`from_reference`), `src/docx/body.rs` (`div` route), `src/check/design.rs` (`design-style-unused`), `tests/design_styles.rs` (new), snapshots under `tests/snapshots/{html,typst,docx}/design-styles*` |

## Goal
`[design.styles.<Name>]` formats a div in all three outputs, whether the div carries
`custom-style="<Name>"` or a class that normalises to the same key: an HTML rule on
`[data-style="<Name>"]`, a Typst `writer-div` body applying the style, and a DOCX paragraph style
definition found by the existing name lookup. `check` warns about declared styles nothing uses.

## Read first
- `docs/spec/design.md` §8 (all), §5.3 item 3, §5.5, §6.4, §7.1 last row, §10
- `docs/spec/html-output.md` §1.4 note, `docs/spec/typst-dialect.md` §4.2 note, `docs/spec/docx-output.md` §2.1–2.3 and §4.1 notes
- `src/design/mod.rs` `style_for`, `normalise_style_key` (T80); `src/docx/styles.rs` `custom`;
  `src/docx/body.rs` `div` (~452–461); `src/html/emit.rs` `div` (~789–811)

## API to implement
```rust
// src/html/design.rs
pub fn styles_css(design: &Design) -> String;                 // §8.3, appended by layout_css (item 3)
// src/docx/design.rs
pub fn style_xml(name: &str, style: &Style, design: &Design, ids: &StyleTable) -> String;   // §8.5
// src/docx/body.rs: `Block::Div` → Style(id) when style_for selects (id = custom(name, Paragraph, None))
// src/check/design.rs
pub fn unused_styles(design: &Design, document: &Document) -> Vec<Diagnostic>;   // design-style-unused
```
```typst
#let writer-div(style: none, classes: (), body) = { … design.md §8.4 … }
```

## Steps
1. HTML: `styles_css` (selector escaping, declaration order of §8.3); `emit.rs` adds `data-style`
   after `class` when `style_for` selects; `EmitOptions`/`render` already carry `&Design` (T81).
2. Typst: `writer-div` body per §8.4 (`set text`, `set par`, `align`, `block(above:, below:,
   breakable:, sticky:)`), reading `writer-design.styles`; identity when the style is unknown.
3. DOCX: `style_xml` per §8.5 (`contextualSpacing` when spacing is set; bold iff weight ≥ 600;
   `w:jc` mapping; `basedOn` Body Text's id; a §2.3 vocabulary name replaces that definition's
   `pPr`/`rPr` and keeps its id, design.md §8.1); `generated` appends them after the built-ins,
   `from_reference` appends only names the reference lacks; `body.rs` `div` uses `style_for` first
   and falls back to the existing `custom-style` path.
4. `check`: `design-style-unused` by walking the resolved IR for `Div`s and applying `style_for`.
5. `tests/design_styles.rs`: the letter fixture (`:::recipient`, `:::x {custom-style="Recipient"}`,
   `:::inside-address`, `::::closing` > `:::signoff {custom-style="Closing"}`): HTML snapshot of
   `styles_css` and of the divs' attributes; Typst source snapshot; DOCX `styles.xml` snapshot and
   a `document.xml` read-back showing `w:pStyle w:val="Recipient"` on both the class and the
   attribute route; `check` on a fixture with an unused `Date` style warns once.

## Acceptance criteria
- HTML: `[data-style="Recipient"]{line-height:12.1pt;margin-bottom:17.008pt;break-inside:avoid}` and
  `[data-style="Closing"]{margin-top:17.008pt;margin-bottom:22.677pt;break-after:avoid}` appear in
  the letter's `/* design: layout */` block; both `<div class="recipient" data-style="Recipient">`
  and `<div class="x" data-style="Recipient">` are emitted.
- Typst: the letter's `writer-div(style: "Recipient", …)` renders as a non-breakable block
  followed by 6 mm (oracle: the next paragraph starts ≥ 17 pt below), `Closing` keeps with the
  signature across a forced page break test.
- DOCX: `styles.xml` holds `Recipient` with `keepLines`, `spacing after=340 line=242 lineRule=exact`,
  `contextualSpacing`; `Date` keeps `w:styleId="Date"` with the design's `sz 20`; a reference that
  defines `Recipient` wins (no second definition, `docx-style-kind-mismatch` not raised).
- `design-style-unused` fires for a declared style no div selects and not for the letter.
- Existing snapshots unchanged without a design (`data-style` and style definitions only appear
  when a design declares styles).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
All three outputs space the recipient block 6 mm below, keep it on one page, set the date at
10 pt, open the salutation 6 mm after the address, and keep "Yours sincerely" with the signature,
from the one `[design.styles]` table; `letter.css` drops its `.recipient`/`.closing`/`.signoff`/
`.signature` rules and the reference document's custom styles are no longer needed.

## Out of scope
Furniture (T86–T88), last-paragraph margins (open point 4), inline styles on spans.

## Hints (verified)
- `block(sticky: true)` keeps a block with the next (`typst-library-0.15.1/src/layout/container.rs`
  line 388); `block(breakable: false)` keeps it together; `above`/`below` take lengths or `auto`.
- `StyleTable::custom(name, Paragraph, None)` (`src/docx/styles.rs` 437–475) returns the existing
  id when the name is already in the table — appending the definition first makes the lookup free.
- `w:contextualSpacing` is a `w:pPr` child after `w:spacing` and before `w:jc` in the schema order
  that `docx::xml` enforces (docx-output.md §5).
- `src/html/emit.rs` ~855–880: attribute assembly; `custom-style` is filtered at ~865 — keep that
  filter and add `data-style` to `own`.
- Twips in the acceptance text: 6 mm = 17.008 pt = 340 twips; 8 mm = 22.677 pt = 454 twips;
  12.1 pt = 242 twips.
