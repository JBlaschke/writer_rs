# T19 — Default HTML theme v1

| | |
|---|---|
| Milestone | M1 |
| Size | M |
| Depends on | T15, T16 |
| Parallel-safe with | T1A, T1B, T20, T21, T25, T32, T41, T75, T77 (wave 5) |
| Files you own | `assets/themes/default/theme.toml`, `assets/themes/default/templates/document.html`, `assets/themes/default/templates/partials/head.html`, `assets/themes/default/templates/partials/scripts.html` (taken over from T15; no change required); `assets/themes/default/styles/theme.css`, `assets/themes/default/styles/notebook.css`, `assets/themes/default/styles/print.css` (new, or replace T00's placeholder; names and split are yours); `assets/themes/default/README.md` (new); `tests/theme_tokens.rs` (new; helpers lifted from presenter_rs/tests/theme_tokens.rs); `tests/default_theme.rs` (new); (additive: T16's `tests/snapshots/html_snapshots__serve_minimal.snap`, only if you edit a template) |

## Goal
The built-in `default` theme gets its stylesheets: system-font typography on a readable measure, title block, figures,
booktabs tables, numbered/highlighted code, equations, callouts, notebook cells with ANSI colours, footnotes, TOC,
bibliography, print rules and a dark scheme, driven by `--w-*` tokens that `README.md` documents and tests check both
ways. Standalone pages inline these entries (≤ 24 KiB); T21 serves them, T72 ejects the theme, T79 moves it to SCSS.

## Read first
- `docs/spec/html-output.md` **§7 — this is your spec**; §6.3 (row "default theme entries"), §9 (T19 row), Open points
  (T19), §1.2 (`html.toc` note for the README), §1.3–§1.12 and §1.15 (markup behind each class, `MARKUP_CLASSES`),
  §3.1–§3.3 (layout, `extends`, code themes), §4.4 (rules a restyled `document.html` keeps), §5 (cascade)
- `docs/spec/math.md` §4.1, §4.3 (`.math-error` `#cc0000`; the `writer-math` font rule is T15's), §4.4 (16 px text)
- `docs/spec/markdown-dialect.md` §5.5 (`columns column center small large`); `docs/spec/typst-dialect.md` §2.4 (rows
  `align(..)`, `overline`/`highlight`); `docs/spec/references.md` §4.7 (`unresolved-reference`);
  `docs/spec/notebook-mapping.md` §4.1, §4.3, §5 (ANSI classes, `PALETTE`), §9 (`subcaption`); `docs/spec/config.md` §2
- `docs/spec/live-reload.md` §1.2, §6.1; `docs/spec/vendored-assets.md` §4, §5.1–§5.2 (debug reads disk), §7.3 (pins)
- `docs/research/R6-html-and-crate-versions.md` §A.3 (stylesheet survey, print CSS facts)
- presenter_rs `tests/theme_tokens.rs` (helpers to lift, Hints), `assets/themes/default/README.md` (shape only); its
  `styles/*.css` are reveal-specific: copy nothing

## API to implement
No Rust API. The theme contract (html-output.md §7, §3.2):
```toml
# assets/themes/default/theme.toml
name = "default"
description = "…"                         # yours; no [vars] table (step 3)
[styles]
entries = ["styles/theme.css", "styles/notebook.css", "styles/print.css"]   # this order is the cascade order
code_theme = "InspiredGitHub"
code_theme_dark = "OneHalfDark"
[scripts]
entries = []
```
```text
:root tokens (one README row each; colour tokens #hex, redefined in @media (prefers-color-scheme: dark) { :root {…} }):
  --w-font-body --w-font-heading --w-font-mono --w-font-size --w-line-height --w-measure --w-text --w-bg --w-muted
  --w-accent --w-link --w-border --w-code-bg --w-code-highlight --w-callout-{note,tip,important,warning,caution,other}
  added here: --w-error (light value #cc0000, math.md §4.3), --w-error-bg (stderr/error tint)
classes that must appear as `.<class>` in a selector, besides MARKUP_CLASSES (§7):
  columns column center small large | span.overline span.highlight div.left div.center div.right
  unresolved-reference | output-text output-stream-stdout output-stream-stderr output-json subcaption
  ansi-{black,red,green,yellow,blue,magenta,cyan,white}-{fg,bg,intense-fg,intense-bg}
  ansi-{bold,dim,italic,underline,strike,hidden} ansi-default-inverse-fg ansi-default-inverse-bg
```
Consumed by the tests (T13, T15; spec signatures, do not implement):
```rust
pub const MARKUP_CLASSES: &[&str] = &[/* … */];                          // writer::html::emit, html-output.md §1.15
pub fn resolve(spec: &str, search_roots: &[&Path]) -> Result<Theme>;     // writer::theme, §3.3
impl Theme { pub fn problems(&self) -> Vec<Diagnostic>; }                // §3.3
pub fn compile_theme_css(theme: &Theme, features: &Features, registry: &mut AssetRegistry) -> Result<String>; // html, §5
```

## Steps
1. **Check inputs on `main`:** `ls assets/themes/default/templates/partials`, `grep -n MARKUP_CLASSES src/html/emit.rs`,
   `grep -n compile_theme_css src/html/mod.rs`, `grep -n "pub fn resolve\|pub fn problems" src/theme/*.rs`; `cargo run --
   build examples/basic/document.md` prints `Wrote …document.html`. Anything missing: stop and ask. Never edit `src/`.
2. **Templates** (T15's, now yours): keep them; they follow §4.4, and the default theme never uses `html.toc` (§1.2).
   Dark mode needs no meta tag (CSS `color-scheme`). Only drop comments naming task ids or `docs/` paths. After a
   template edit run `cargo test --test html_render --test theme_resolve --test html_snapshots` and re-bless
   `html_snapshots__serve_minimal.snap` (`INSTA_UPDATE=always`), named in the PR.
3. **`theme.toml`** = the API block. No `[vars]`: T15's child theme merges them into `html_render__context.snap`.
4. **`styles/theme.css`, base:** `:root` with every token and `color-scheme: light dark`. Values are yours; suggested:
   body `Charter, "Bitstream Charter", "Sitka Text", Cambria, serif`, heading `system-ui, -apple-system, "Segoe UI",
   Roboto, sans-serif`, mono `ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace`, `--w-measure:
   42rem`, `--w-line-height: 1.6`; fixed: `--w-font-size: 1rem` (16 px on screen as math.md §4.4 assumes, 11 pt in
   print). Then the dark `:root`; `body` (margin 0, `--w-bg`, `--w-text`, body font); `[data-writer-document] {
   max-width: var(--w-measure); margin: 0 auto; padding: … }`; `img { max-width: 100%; height: auto; }`; headings and
   `.number`; links; title-block classes, `.abstract`. Custom properties outside `:root` start with `--_` (private).
5. **`theme.css`, blocks** (backticked selectors are what the tests look up; selector lists are fine): `.task-list`
   `list-style: none` (checkbox hanging into the indent); `dl`, `blockquote`, `hr`; booktabs: `table`
   `border-collapse: collapse` + `border-top`/`border-bottom`, a `thead` rule with `border-bottom`, no
   `border-left|right|inline` on `td`/`th`; `.align-left|center|right`; `figure.figure`, `figcaption`, `.caption-label`
   (bold), the `.kind-*`. Code: one-class `.hl-code { background-color: var(--w-code-bg); color: var(--w-text); }` (the
   `"none"` fallback, Hints), `pre.hl-code` (`overflow-x: auto`, padding, mono, no colours), `:not(pre) > code` on
   `--w-code-bg`, `pre.numbered { counter-reset: line }`, `pre.numbered .line::before` (`counter-increment: line;
   content: counter(line)`, muted gutter), `pre.hl-code .line { display: inline-block; min-width: 100%; }`,
   `.line.highlight`, `span.highlight` (`background-color: var(--w-code-highlight)`). Math, exactly: `.equation {
   display: flex; align-items: center; gap: 1em; }`, `.equation > .math.display { flex: 1 1 auto; min-width: 0;
   overflow-x: auto; text-align: center; }`, `.equation-number { flex: none; margin-left: auto; }`; also `.math.inline`,
   `.math-source` (as inline code), `.math-error { color: var(--w-error); }`. Callouts: `.callout` (`border-left: .25em
   solid var(--w-callout-other)`, padding), `.callout-<kind> { border-left-color: var(--w-callout-<kind>); }` for the
   five kinds, `.callout-title` (bold). `.columns` (`display: grid; grid-auto-flow: column; grid-auto-columns: 1fr;`
   gap; one column on narrow screens), `.column { min-width: 0; }`, `div.left|center|right` (`text-align`), `.small`
   (`.85em`), `.large` (`1.25em`), `span.overline { text-decoration: overline; }`; `@media screen { .page-break {
   display: none; } }` (never unconditional).
6. **`theme.css`, references:** `nav.toc`, `.toc ol { list-style: none; }`, `.toc-title`, `.footnotes`, `.footnote-ref`,
   `.footnote-back`, `.citation`, `.ref`, `.references`, `.csl-bib-body`, `.references-title`, `.csl-entry`,
   `.hanging-indent .csl-entry { padding-left: 2em; text-indent: -2em; }`, `.csl-left-margin { float: left; min-width:
   2em; }`, `.csl-right-inline { margin-left: 2em; }`, `.unresolved, .unresolved-reference { color: var(--w-error); }`
   (+ wavy underline), `.smallcaps { font-variant: small-caps; }`.
7. **`styles/notebook.css`:** `.cell`, `.cell-input`, `.cell-outputs`, `.output`, `.output-display`, `.output-result`,
   `.output-stream`, `.stdout`, `.output-text`, `.output-stream-stdout`, `.output-stream-stderr`, `.output-json`;
   `.cell-prompt` in a left gutter (mono, small, muted; beside the measure on wide viewports, above the code on narrow
   ones); `.output.stderr > pre, pre.output-error { background-color: var(--w-error-bg); }`; `.subcaption { font-size:
   .85em; }`. ANSI, one rule per class with exactly that selector: `.ansi-<name>-fg { color: #… }` and `-bg {
   background-color: #… }` with `PALETTE[i]` for black red green yellow blue magenta cyan white (`i` = 0–7), the
   `-intense-` pair with `PALETTE[i + 8]`; `.ansi-default-inverse-fg { color: var(--w-bg); }`,
   `.ansi-default-inverse-bg { background-color: var(--w-text); }`; `.ansi-bold`, `.ansi-dim` (`opacity: .7`),
   `.ansi-italic`, `.ansi-underline`, `.ansi-strike` (`line-through`), `.ansi-hidden` (`visibility: hidden`).
8. **`styles/print.css`:** §7's block verbatim (no `h1 { break-before: page }`); last in `entries`, so it wins.
9. **`README.md`** (ships with the binary and `eject theme`: no task ids, no `docs/` paths): files and entry order;
   `## Tokens` as `| token | default | dark | purpose |`, one row per token, values in backticks exactly as in the CSS,
   `—` as the dark value of non-colour tokens; `## Classes`, one line `` - `.c` — … `` per `MARKUP_CLASSES` entry,
   then the others (ANSI may share a line); code themes (`--w-code-bg` only for inline code and `"none"`); print (A4,
   page numbers in Chrome only, TOC hidden, `.page-break`, `.no-print`); equation images assume 16 px text; extending:
   `extends = "default"`, child entries after these, tokens overridden in a child `:root`, `{% extends
   "@default/document.html" %}` with the six block names, `writer eject theme ./theme`, and that `html.toc` placed
   outside `html.main` goes stale in `writer serve` until a full reload (only `data-writer-document` is patched).
10. **`tests/theme_tokens.rs`:** lift presenter's helpers (Hints), `--p-` → `--w-`; write `light()` (top-level `:root`
    declarations), `dark()` (`:root` in context `@media (prefers-color-scheme: dark)`), `documented()` over
    `` ^\| `(--w-[a-z0-9-]+)` \| (.+?) \| (.+?) \| .+\|$ ``. Tests: Acceptance.
11. **`tests/default_theme.rs`:** `decl(rules, context, selector, property) -> Option<String>` = the value in the last
    rule of `context` whose `complex_selectors` contain `selector` exactly; class coverage = regex
    `\.<regex::escape(c)>(?:[^A-Za-z0-9_-]|$)` over all selectors (`.math` must not count `.math-error`); hard-code
    notebook-mapping.md §5's `PALETTE` (no dependency on T18). Tests: Acceptance.
12. **Regressions and manual check:** `cargo test` (T15's `html_render`/`theme_resolve`, T16's `html_snapshots` incl.
    `minimal.md` ≤ 60,000 B, and `cli_build` pass unchanged). Build the showcase, `examples/basic/document.md` and (if
    T18 merged) `tests/fixtures/ipynb/outputs.ipynb` with `cargo run -- build <file> -o <scratch>/x.html`; open each in
    Chrome: light, dark (DevTools → Rendering → `prefers-color-scheme`), narrow, print preview. List checks in the PR.

## Acceptance criteria
- `tests/theme_tokens.rs`: `every_root_token_is_documented_with_its_defaults` (each `light()` token is a README row with
  that default and its `dark()` value or `—`; every row declared); `the_spec_tokens_exist` (§7's 20);
  `colour_tokens_are_redefined_for_dark` (light value `#…` ⇒ in `dark()`; `dark()` ⊆ `light()`; §7's 14 colour
  tokens and `--w-error*` are hex); `error_colour_is_katex_red` (light `--w-error` = `#cc0000`);
  `text_is_readable_in_both_schemes` (WCAG `contrast` on `--w-bg`: text ≥ 7; muted, link, error ≥ 4.5; text on
  `--w-code-bg` and `--w-code-highlight` ≥ 4.5; accent, callouts ≥ 3); `the_readme_names_only_tokens_the_css_knows`;
  `private_properties_outside_root`; `stylesheet_loads_nothing_from_the_network` (lifted);
  `theme_css_contains_no_text_that_document_tests_look_for` (`asset:`, `/__writer/`, `In [`, `Out [`, `data-sourcepos`);
  `the_rule_parser_sees_every_rule`; `readme_lists_every_markup_class`; `readme_explains_extending` (`extends =
  "default"`, `{% extends "@default/document.html" %}`, `writer eject theme`, `html.toc`);
  `shipped_theme_files_name_no_tasks_or_repository_documents` (lifted).
- `tests/default_theme.rs`: `every_required_class_has_a_selector` (`MARKUP_CLASSES` + the API list); `layout_rules`
  (`[data-writer-document]` measure, `img` `max-width: 100%`, `.columns` grid); `tables_are_booktabs_like`;
  `code_rules` (`pre.hl-code` scrolls, `pre.numbered` reset, `::before` increments and shows `counter(line)`, both
  highlight rules); `math_rules` (step 5's three, `.math-error`); `callouts_have_an_edge_per_kind`;
  `lists_and_toc_have_no_markers`; `bibliography_hangs_and_aligns`; `unresolved_is_marked`; `typst_frontend_rules`;
  `notebook_rules`; `ansi_classes_use_the_palette` (32 rules = `PALETTE` ignoring case, inverse, six effects);
  `page_break_is_hidden_on_screen_only` (`display: none` on `.page-break` only in `@media screen`; `@media print` has
  `display: block` + `break-after: page`); `print_rules` (`size: A4`, `counter(page)`, `@page :first`;
  `[data-writer-document]` `max-width: none`, `nav.toc` hidden, `pre-wrap`, `print-color-adjust: exact`; no
  `break-before: page`); `entries_fit_the_size_budget` (≤ 24,576 B, printed); `theme_toml_matches_the_spec`;
  `default_theme_compiles` (`resolve("default", &[])`, `problems()` empty; `compile_theme_css` with
  `Features::default()` has each `/* @default/<entry> */` in order and no `code theme`; with `code: true` also
  `/* code theme: InspiredGitHub */` and `/* code theme (dark): OneHalfDark */`).
- `tests/default_theme.rs::showcase_builds_and_every_class_is_styled`: in a `tempfile::tempdir()`, `dot.svg` and
  `showcase.md` with front matter (title, subtitle, authors, date, abstract, `toc: true`, `numbering: { headings: "1.1"
  }`), inline formatting, a link, a footnote, task list, ordered list from 3, definition list, quote, `***`, `![A
  dot](dot.svg){#fig-dot width=30%}`, a `Table:`-captioned aligned table, ```` ```python {.numbered start=5
  highlight="6"} ````, inline math, `$$ E = mc^2 $$ {#eq-e}`, the five `> [!KIND]` alerts, `:::callout kind=remark`,
  `::::columns` with two `:::column`s, `:::center`, `:::small`, `:::large`, `:::pagebreak` (no `@` references: T1B is
  in this wave). `writer build showcase.md -o out.html` exits 0; the page has `<style id="writer-theme">` and `/*
  @default/styles/theme.css */`; each class of a `class="…"` (outside `<script>`/`<style>`) except `hl-*`,
  `language-*`, `callout-remark` has a selector. Fix syntax per markdown-dialect.md; never drop a construct.
- No `tests/snapshots/*.snap` changes unless a template changed (then only T16's serve snapshot, named in the PR).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
Markup and `MARKUP_CLASSES` (T13; a missing class is a coordinator question), stylesheet compilation, helpers,
`math_init()` (T15), KaTeX/MathML (T1A), the ANSI converter (T18), snapshots and size asserts (T16), overlay and pages
view (T22, T24), theme lints (T1C), `eject theme` (T72), SCSS (T79), sidenotes, a dark-mode toggle. Never edit `src/`.

## Hints (verified)
- Cascade (html-output.md §5, T15's `compile`): entries in order, each after `/* @default/<path> */`, then, only with
  `features.code`, the code theme CSS with root rule `.hl-code { color; background-color }` (InspiredGitHub 15,772 B,
  OneHalfDark 3,334 B, T13 probe; outside the 24 KiB; light wrapped in `@media not all and (prefers-color-scheme: dark)`
  when both are set). It comes after your entries: your one-class `.hl-code` only serves `"none"`, and a rule that must
  beat the code theme needs more specificity (`pre.output-error`). Notebook outputs are `<pre class="hl-code output-…">`.
- Code markup (§1.5): `<pre class="hl-code numbered" style="counter-reset:line 4">` (inline style beats `pre.numbered`);
  lines `<span class="line">`/`<span class="line highlight">` joined by `\n`, so `span.highlight` (typst's
  `#highlight`) also hits code lines: background only. `display: block` lines with `\n` between them give blank lines
  in a `pre` (known pitfall, not browser-checked here; confirm in step 12). Equations: math.md §4.1's one-line markup.
- Print (R6 §A.3, MDN BCD 2026-10-24): `@bottom-center` margin boxes Chrome ≥ 131 only; `break-after: always`
  unsupported in Chrome (`page` is); no `orphans`/`widows` in Firefox; no browser has `target-counter()` (hence no TOC in
  print). Unverified: whether Chrome's print preview keeps `prefers-color-scheme: dark`; check in step 12 and, if dark
  tokens leak, give the colour tokens light values in `@media print { :root { … } }`.
- presenter_rs `tests/theme_tokens.rs` lines: `theme_dir`/`read` 30–36, `stylesheets` 39–52, `css` 55–62, `normalize`
  69–71, `declared` 77–100 (→ `light()`/`dark()`), `used` 103–109, `documented_tokens` 113–122, `stylesheet_loads…`
  224–242, `Rule`/`rules`/`collect_rules`/`rule` 244–310, `complex_selectors` 397–413, `hex_rgb`/`contrast` (WCAG 2)
  767–806, `theme_css_contains_no_text…` 1085–1093, `rule_in` 1097–1105, `assert_names_no_tasks_or_repository_documents`
  + `shipped_theme_files_…` 1441–1484 (`checked >= 6`). `collect_rules` turns `@page { size: A4; …; @bottom-center {…}
  }` into one odd pseudo-rule: test `@page` on the comment-free text.
- Test crates (vendored-assets.md §7.3): normal deps `regex` 1.13.1 (no look-around; `regex::escape`), `toml` 1.1.8
  (`read("theme.toml").parse::<toml::Table>()`), `walkdir` 2.5.0; dev-deps `tempfile` 3.27.0, `assert_cmd` 2.2.2:
  `assert_cmd::Command::cargo_bin("writer")` (`src/cmd.rs:70`, not deprecated) or `env!("CARGO_BIN_EXE_writer")`.
- Debug builds read `assets/` from disk (vendored-assets.md §5.2): CSS edits need no rebuild. No compiler-backed test
  here; two-face loads fast with T00's `[profile.dev.package."*"] opt-level = 2`. The showcase inlines KaTeX (≈ 650 KB).

## Notes for the coordinator
- math.md §4.3 fixes `.math-error` at `#cc0000` (≈ 3.2:1 on a dark background, below WCAG AA). The task keeps it as
  `--w-error`'s light value and lightens it in the dark scheme (KaTeX's own `errorColor` is unaffected).
- math.md §4.4 sizes equation images for 16 px (12 pt) text; §7's print block sets `html { font-size: 11pt }`, so
  printed equation images are ≈ 9 % too large unless T1A sizes them in `em`.
- The inlined stylesheet contains the selector `[data-writer-document]` (§7): counting that attribute by substring in a
  standalone page fails (T16's `example_builds_standalone`); use T14's `html_count_attr`, which skips `<style>`.
- §9 has T19 re-bless T16 snapshots its "entries change": T16 snapshots `html.main` and a serve page linking the
  stylesheet, so only template edits change them (`html_snapshots__serve_minimal.snap`).
