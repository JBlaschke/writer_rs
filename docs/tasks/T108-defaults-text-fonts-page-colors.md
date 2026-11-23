# T108 — Body text, fonts, page and colours: the first switch

**Status: brief, not ready for hand-off.** Eight decisions and a measured prototype come first
(see "Before hand-off").

| | |
|---|---|
| Milestone | M9 |
| Size | L |
| Depends on | T102, T104, T105, T106, T107 (and decisions D3, D4, D5, D7, D8, D13, D16, D19) |
| Parallel-safe with | nothing: the only task of wave 28 |
| Files you own | `src/design/defaults/{text,fonts,page,colors}.toml`, their bits in `APPLIED`, `src/design/resolve.rs` (font sizes rounded, D19); `src/html/design.rs` (`tokens_css`, and the built-in rules before the theme, D16), `src/html/styles.rs`; `assets/themes/default/styles/theme.css:1-8`, :9-24, :48-56 and :96 (drop `p` from the selector list; T112 keeps the rest of :96-110), `print.css:1-4` and :12-31; `assets/typst/writer-doc/lib.typ` (`layout`, `writer-doc`, and `paragraph-layout`'s no-design branch from R12's T98), `assets/typst/writer-doc/design.typ` (the fallback design), `assets/typst/writer-doc/writer-defaults.json` (new), `src/typst/world.rs`; `assets/docx/styles.xml` (`w:docDefaults` and the styles Normal, Body Text, First Paragraph), `src/docx/design.rs` (document defaults, margins, style sizes); `tests/typst_spacing.rs` (`an_undeclared_design_keeps_typsts_layout` only, T98 criterion 7 re-specified), `tests/design_defaults_json.rs` (new), `tests/pipeline_defaults.rs` (new); the snapshots that change and golden snapshots of `tests/xfmt/case-a.md` |

## Goal
From this task on, a document without `[design]` gets writer's body text, fonts, page geometry
and colours in all three formats, laid out in design.md §0.3's model; a document with a partial
`[design]` gets them for every key of these four domains it leaves unset. The task switches the
four domains on in `APPLIED` and makes each backend carry the values: HTML emits the built-in
tokens before the theme's entries (D16), so the default theme and existing themes still win
where the user declared nothing; the package carries `writer-defaults.json`, generated from the
defaults, as the fallback for a template that calls `writer-doc` without writer's input; Word's
document defaults and body styles come from the design. design.md §0.2's first two invariants and
§0.3's restriction to a declared design are retired for these domains (design.md §0.4).

The PDF goes further than the four domains. The package has one gate per document,
`settings.design.len() == 0` in `paragraph-layout` (R12's T98), and this task removes it. From
here on the PDF puts every element of a document without `[design]` under §0.3's line boxes and
block spaces, so R12 §7's moves reach undeclared headings, quotes, code, figures, tables,
equations and footnotes now, not in their own tasks. HTML and Word change for the four domains
only. HTML writes the generated rules in two places: the built-in values before the theme (D16)
and the user's after it (R12's D2), using T105's distinction between the two.

## Read first
- `docs/spec/design.md` §0.2, §0.3, §0.4 and the sections the coordinator edited for T105
- `docs/research/R13-single-source-defaults.md` §2.1 (the inventory of these domains), §4.1
  ("PDF", "HTML", "Word"), §4.3 (the values), §4.6 (compatibility, the EIT templates)
- `docs/tasks/T98-typst-line-boxes.md` criterion 7 (`an_undeclared_design_keeps_typsts_layout`),
  which this task re-specifies

## API to implement
Set at hand-off from the coordinator's prototype. Existing entry points that change behaviour,
not signature: `tokens_css` (`src/html/design.rs:15`), `compile_theme_css`
(`src/html/styles.rs:77`), R12's `world_inputs` and `design_input` (`src/typst/world.rs`).

## Steps
1. Switch the four domains on and set D19's rounding in the resolver.
2. HTML: the built-in tokens and body rules before the theme's entries; remove from `theme.css`
   and `print.css` the values the design now carries.
3. PDF: `writer-defaults.json` and its test; `writer-doc`'s and `layout`'s literal defaults
   replaced by the design; the package's no-design branch goes (R13 §4.1).
4. Word: document defaults, Normal, Body Text, First Paragraph and the section margins from the
   design; style sizes scaled with `half_points`.
5. Re-specify T98's criterion 7 (`tests/typst_spacing.rs`) as a test of the package-only path,
   and add a pipeline test (`tests/pipeline_defaults.rs`) that a document without `[design]`
   gets the built-in model in Word and the PDF.
6. Accept the snapshots after reading them; add golden snapshots of `tests/xfmt/case-a.md`.

## Acceptance criteria
1. `tests/xfmt/case-a.md`, measured with T102's harness: pitch, paragraph gap and first baseline agree Word
   (u) ↔ PDF within 0.3 pt (17.6 and 30.25 pt under the proposed D3 and D4); HTML screen layout
   agrees within 0.3 pt after c.
2. Margins, text colour and link colour are equal in all three; nothing is justified.
3. A test (`tests/design_defaults_json.rs`) checks that `writer-defaults.json` equals the
   built-in design.
4. The default theme stays within 24 KiB (`tests/default_theme.rs:462-463`).
5. The pipeline test of step 5 passes, and T98's re-specified criterion 7 passes.
6. The PDF snapshots of documents without `[design]` that hold headings, quotes, code, figures,
   tables, equations or notes change by R12 §7's amounts, and the PR lists them.
7. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Headings, the title block and the contents (T109), captions and images (T110), the rest of
  the palette and links (T111), and every other domain of design.md §0.4's table.
- LaTeX (T120).

## Hints (verified)
- Main's literal defaults: `writer-doc` (`lib.typ:465`) takes 11 pt (:511-513), a 2.5 cm margin
  (:514-522), Libertinus Serif (:529), DejaVu Sans Mono (:535), `justify` true (:543-545) and
  black text; `layout` (:440-463) sets `numbering: "1"` (:447-449) and the math font "New
  Computer Modern Math" (:456). `design.typ:7-9` falls back to an empty design.
- Word: `w:docDefaults` is `styles.xml:2` (Calibri), Normal :3, Body Text :4, First Paragraph
  :5. `src/docx/design.rs:438-441` default the margins to 1440 twips; :126 scales style sizes
  to whole points, `((old * size.0 / 11.0 / 2.0).round() * 2.0)`; `half_points` is :17-19.
- HTML: `theme.css:1-24` holds the font, size, line-height, measure and colour tokens, :48-56
  the body rule; `print.css:1` sets `@page` (A4, 20 mm 18 mm, page number), :4 `font-size: 11pt`
  under print, :12-31 the print colour tokens.
- Files under `assets/typst/` are embedded by `#[include = "typst/**/*"]` (`src/vendor.rs:30-35`),
  so `writer-defaults.json` needs no registration.
- The default theme's CSS is 13,614 B today (theme.css 8,743 B, print.css 1,959 B,
  notebook.css 2,912 B), against the 24 KiB budget of html-output.md §6.3 (R13 §4.1).

## Wave 26's results (coordinator, 2026-11-22)

- T105 landed (PR #90). The switch is `APPLIED` in `src/design/defaults/mod.rs:20`; step 1
  sets it to `TEXT | FONTS | COLORS | PAGE`, the first row of `TASKS` in
  `tests/design_defaults.rs`, whose tests hold for any `APPLIED`. `PAGE` holds only
  `page.paper` and `page.margin`: `header_offset`, `footer_offset` and `print` are in
  `PAGE_NUMBERS` (T116).
- Layer 0 uses Option B's fonts, 12 pt screen and 11 pt print, and no rounding, until D7, D8
  and D19 are taken; step 1 sets the decided values in `src/design/defaults/`.
- D16: HTML writes the rules whose keys `Design::is_builtin` reports (`src/design/mod.rs:1003`)
  before the theme's entries and the rest after them.
- With no margins in the design, Word's `pgMar` falls back to 1440 twips
  (`src/docx/design.rs:438-441`) while the PDF uses 25 mm, so Word's page top is 1.13 pt lower
  (T102). Layer 0's `page.margin` replaces both fallbacks.
- T104 landed: the alignment rule is on `[data-writer-document]`, the paragraph margin under
  `:where()`. T102's harness is `scripts/xfmt/run.py`; an intended move is recorded with
  `--update` in the task's own commit, as `e404d1b5` did for p7 after T104.

## Before hand-off
- Decisions D3 (body line height), D4 (paragraph spacing), D5 (margins), D7 (fonts), D8 (screen
  size), D13, D16 (built-in CSS before the theme) and D19 (rounding) are taken.
- The coordinator prototypes the switch for these four domains, computes the Word (u) values and
  measures `tests/xfmt/case-a.md` with the harness.
- The coordinator commits R13's probes (a) and (b) as `tests/xfmt/case-a.md` and
  `tests/xfmt/case-b.md`, after T102, which owns `tests/xfmt/`.
- The coordinator checks the EIT templates against R13 §4.6 (not re-read for R13, §6.4) and
  re-cites the lines R12 moved in `lib.typ`, `design.typ`, `src/html/design.rs`,
  `src/docx/design.rs` and `src/typst/world.rs`.
