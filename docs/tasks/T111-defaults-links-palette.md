# T111 — Links and the colour palette

**Status: brief, not ready for hand-off.** Decision D11 and the list of colour literals with
their tokens come first (see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | M |
| Depends on | T110 (and decision D11) |
| Parallel-safe with | nothing: the only task of wave 31 |
| Files you own | the colour additions in `src/design/defaults/colors.toml` and `src/design/defaults/links.toml`, with their bits in `APPLIED`; the colour literals of `assets/docx/styles.xml` outside the syntax token styles; `lib.typ`'s link, reference and footnote-marker rules and its colour literals; `assets/typst/writer-doc/callout.typ:24-29` (the palette only); `assets/themes/default/styles/theme.css:14-24` (the palette tokens, after T108), :87-94 and :292-296; `print.css:7` and :20-30 (the print palette tokens, after T108); the snapshots that change |

## Goal
Every element names a colour token of one palette: text, muted, accent, link, border, code_bg,
highlight, error, background and callout.{note, tip, important, warning, caution, other} (R13
§3.4 M8, §4.2). The colours written as literals in Word's styles, the package and the callout
module become the design's. Links and cross-references are coloured and underlined in all three
formats (`[design.links]`), and printed HTML adds URLs after links only if D11 says so.

## Read first
- `docs/spec/design.md` §0.4
- `docs/research/R13-single-source-defaults.md` §2.7 (links and references), §3.4 M8, §4.2,
  §4.3 (rows colors, Links)

## API to implement
None beyond the keys of R13 §4.2: `colors.border`, `code_bg`, `highlight`, `error`,
`colors.callout.{note, tip, important, warning, caution, other}`, and `[design.links]`
`underline`, `print_url`.

## Steps
1. Add the keys and defaults; switch their bits on.
2. Replace each literal with its token, from the coordinator's list.
3. Links and references in all three; unresolved references in the error colour.
4. Accept the snapshots after reading them.

## Acceptance criteria
1. No colour literal is left in `styles.xml` (outside the syntax token styles, :55-85, which
   T113 replaces), `lib.typ` or `callout.typ`, except the literals on the coordinator's list in
   "Before hand-off".
2. `tests/xfmt/case-b.md`'s muted and accent colours reach all three formats.
3. Links and cross-references are coloured and underlined in all three; unresolved references
   are red in all three.
4. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- `cells.typ`: it holds no colour or other presentational literal at main (it writes the input
  and each output after a `parbreak()`), so it has no colour to move.
- Dark mode (design.md §0.4, exception 2); syntax colours (T113).
- LaTeX, unless the coordinator adds it at hand-off (T120 maps only T108's domains).

## Hints (verified)
- `styles.xml` at main holds `0F4761` 12 times, `59636E` 6, `1F2328` 5, `D0D7DE` and `8C959F` 3
  each, `F6F8FA` and `0969DA` 2 each, besides the token colours of :55-85 and the callout fills
  of :86-92 (`DDF4FF` and others).
- `callout.typ:23` defines `callout`; its palette is :24-28 (`#2867a2`, `#307445`, `#7950a0`,
  `#926500`, `#b33131`) and the fallback `#62676c` is :29. `theme.css:17-22` has another
  callout palette.
- `lib.typ` at main: `listing-grid` (:610) uses `#666666` (:618), `#fff1b8` (:627) and
  `#f7f7f7` (:631); `design-layout` (:433-438) colours links only when the design sets
  `colors.link`.
- `theme.css` at main: links :87-94, unresolved references :292-296. `print.css:7` prints the
  URL after each `http` link.
- The theme's palette tokens are `theme.css:14-24` (border to error background) and the print
  copy `print.css:20-30`. Under D16 the theme's tokens still win over the built-in values, so
  they go once the design carries the palette.

## Before hand-off
- D11 decided (URLs printed after links in HTML print; proposed: no).
- The coordinator lists every colour literal with its token, and the literals that stay (for
  T121's allowlist).
- `tests/xfmt/case-b.md` is committed (T108's "Before hand-off").
