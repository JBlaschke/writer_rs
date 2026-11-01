# T81 — HTML design tokens and fonts

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T80 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `src/html/design.rs` (new), `tests/html_design.rs` (new), `tests/snapshots/html/design-*.html` (new); additively: `src/html/styles.rs` (`compile_theme_css` parameter and assembly), `src/html/context.rs` (`theme.design`), `src/html/document.rs` / `src/pipeline/mod.rs` (passing `&Design`) |

## Goal
A declared design reaches the HTML output as generated CSS: `@font-face` rules for declared font
files (copied through the asset pipeline), a `:root` block of the existing `--w-*` tokens before
the theme's entries, and a layout block (`@page` size and margins, paragraph spacing and
justification) after them. `theme.design` is in the template context. Without a declared design
the stylesheet is byte-identical to today's.

## Read first
- `docs/spec/design.md` §5.1–5.4, §0.2, §3.3, §4.3
- `docs/spec/html-output.md` §5 and §4.1 with their 2026-11-01 notes, §6.1 (asset URLs), §7
- `src/html/styles.rs` (`compile_theme_css`, `compile`, `rewrite_urls`, `Targets`,
  `standard_font_mime`), `src/html/context.rs` (`theme_json`)

## API to implement
```rust
// src/html/design.rs
pub fn tokens_css(design: &Design, registry: &mut AssetRegistry, targets: &Targets) -> Result<String>;  // §5.2
pub fn layout_css(design: &Design, registry: &mut AssetRegistry, targets: &Targets) -> Result<String>;  // §5.3 items 1–2 (T85, T86 add 3–4)
pub fn css_size(paper: &str) -> String;                                                                  // §5.3 item 2 table
pub(crate) fn css_pt(pt: Pt) -> String;                                                                  // "11pt", "141.732pt"
// src/html/styles.rs
pub fn compile_theme_css(theme: &Theme, design: &Design, features: &Features, registry: &mut AssetRegistry) -> Result<String>;
```

## Steps
1. `css_pt`, `css_size`, family quoting, colour formatting; unit tests.
2. `tokens_css`: `@font-face` per file in body/heading/mono order (weight and style from
   `typst::text::FontInfo::iter`; `format()` by extension; one rule without descriptors when no
   face is readable), then `:root` with the §5.2 table order; register each file with
   `AssetRegistry::register_file` and format its URL exactly as `rewrite_urls` formats an entry's
   (placeholder in build, `/__writer/asset/...` in serve).
3. `layout_css` items 1–2 (paragraph spacing / justify rule, `@page`), leaving a documented
   insertion point (a `push_str` of two further parts) for T85 and T86.
4. `compile_theme_css`: assemble hoisted imports + tokens + entries + layout + code themes (§5.1);
   both variants (`Targets::Placeholders`, `ServeUrls`); the `/* design: … */` headers.
5. `context.rs`: `theme.design` = `design.to_json(PathMode::Real)` or `null`.
6. Thread `&Design` from `Prepared.design` through `pipeline::html` into `render`.
7. `tests/html_design.rs`: build `tests/fixtures/design/letter/letter.md` (from T80) to standalone
   HTML and snapshot the `<style id="writer-theme">` content; a fixture with one font file (write
   an embedded Typst font's bytes to a temp `.ttf` and reference it) proves the `@font-face`
   weight/style and the asset copy in site mode; `dump --context` shows `theme.design`.

## Acceptance criteria
- `tests/html_design.rs`: the letter's stylesheet contains, in order, `/* design: tokens */`,
  `:root{--w-font-body:"Source Sans 3",system-ui,sans-serif;…--w-link:#a96d4b}` (exact text in the
  snapshot), the theme entry, `/* design: layout */`, `[data-writer-document] p{margin-block:0 10.205pt;text-align:start}`
  and `@page{size:A4;margin:141.732pt 62.362pt 107.717pt 62.362pt}`; the font fixture yields
  `@font-face{font-family:"…";src:url(asset:…) format("truetype");font-weight:400;font-style:normal;font-display:swap}`
  and the file appears under `assets/` in site mode with its bytes intact.
- `tests/html_snapshots.rs`, `tests/default_theme.rs` and the §6.3 budget test are unchanged
  (no design declared → identical stylesheet).
- `css_size` unit test covers every named row and one Typst-only name (`a6` → `105mm 148mm`).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
The HTML letter renders in Source Sans 3 at 11 pt / 1.35 in `#242424` with copper links and A4
print margins from the design; its `letter.css` can drop the `@font-face` rules, the root tokens
and the `@page` size/margin line. Headers, footers and the named styles still come from the
theme's CSS and template (T85, T86).

## Out of scope
Named-style rules and `data-style` (T85), furniture and the `design_furniture` helper (T86), dark
mode (open point 2), Typst and DOCX (T82, T83).

## Hints (verified)
- `typst::text::FontInfo::iter(data: Bytes) -> impl Iterator<Item = FontInfo>` and
  `FontInfo { family, variant: FontVariant { style: FontStyle, weight: FontWeight, stretch } }`
  (`typst-library-0.15.1/src/text/font/mod.rs` line 80, `book.rs`); `FontWeight::to_number()`;
  `FontStyle::{Normal, Italic, Oblique}` (map `Oblique` to `oblique`). `typst::foundations::Bytes::new(vec)`.
- `rewrite_urls` in `src/html/styles.rs` registers font MIME types via `standard_font_mime`; use
  the same `Targets` enum so serve URLs carry `?v=<content_version>`.
- `typst::layout::Paper` has the named sizes (verify the constructor name in
  `typst-library-0.15.1/src/layout/page.rs` — the `papers!` macro defines them); fall back to a
  local table for the ten CSS names if the API is private.
- The default theme's `print.css` sets `@page { size: A4; margin: 20mm 18mm }`: that is why the
  layout block comes after the entries (design.md §5.1).
