# T13 — HTML emitter and syntect highlighting

| | |
|---|---|
| Milestone | M1 |
| Size | L |
| Depends on | T01 |
| Parallel-safe with | T03, T04, T10, T11, T14, T17, T51 (wave 2) |
| Files you own | `src/html/emit.rs` (replace stub), `src/html/highlight.rs` (replace stub), `tests/html_emit.rs` (new), `tests/snapshots/html_emit__sample_build.snap` (new), `tests/snapshots/html_emit__sample_positions.snap` (new) |

## Goal
`html::emit` turns a resolved `Document` (ir.md) into the HTML fragments of html-output.md §1 — title block, body,
footnotes, TOC, the `<main data-writer-document>` morph root — byte for byte as the markup contract fixes them, and
`html::highlight` is writer's one syntax highlighter (two-face syntaxes, `hl-` classes, per-line spans). Consumers: T15
(`render`, `math_init()` from `Emitted.math`, code theme CSS), T16 (pipeline, `KatexOnly`, snapshots), T18 (the `ansi`
arm), T1A (`MathRenderer` behind `MathHook`), T22 (position attributes), T40 and T60 (`resolve_syntax`, `SYNTAXES`).
Nothing is lifted from presenter_rs (R8 §10 has no row for T13; its `markdown/code.rs` writes reveal markup).

## Read first
- `docs/spec/html-output.md` §1.1–§1.15 (**this is your spec**), §2.1–§2.2, §8 (rows `code-language-unknown`,
  `math-conversion-failed`), §9 (T13's two rows), §10.1–§10.2, "Open points" (T13), Deviations ("Math hook",
  "Localisation before T17", "One highlighter", "`CodeAttrs` before T11", "ANSI outputs")
- `docs/spec/ir.md` §1 (every type you print), §2 invariants 2, 4, 5, 8, §3 (never resolve `asset:` URLs), §5 (`Diagnostics`)
- `docs/spec/references.md` §1.3 (reserved generated ids), §4.1 (`LangTag`, `Term`, `term`, `supplement`,
  `caption_separator`: T01's `src/resolve/l10n.rs`), §4.3 (document order), §4.5, §4.7 (placeholder `Span`), §4.8, §6 (HTML column)
- `docs/spec/markdown-dialect.md` §4.6 (`CodeAttrs`, T01's `src/markdown/code.rs`), §5.5 (`columns`/`column` pairs)
- `docs/spec/notebook-mapping.md` §2.7 (`Cell` fields), §4.1–§4.3 (output `CodeBlock` classes, `CellOutput::pick`), §5
  "Ownership", §9 (HTML column)
- `docs/spec/math.md` §4.1 (markup, `data-tex`), §9 (`math-conversion-failed` message shape); `docs/spec/config.md` §2, §4
  (`Toc`, `[notebook] show_prompts`); `docs/spec/vendored-assets.md` §3.2–§3.3, §7.3 (syntect/two-face pins)
- `docs/research/R6-html-and-crate-versions.md` §A.1 and "Verified API hints"; the highlighter's other users (do not
  break them): `docs/spec/docx-output.md` §2.4, §4.6 and `docs/spec/latex-output.md` §6

## API to implement
```rust
// src/html/highlight.rs (T13) — html-output.md §1.5, exact
pub static SYNTAXES: LazyLock<SyntaxSet>;   // two_face::syntax::extra_newlines()
pub enum SyntaxChoice { Plain, Unknown, Syntax(&'static SyntaxReference) }
pub fn resolve_syntax(lang: Option<&str>, plain: bool) -> SyntaxChoice;                  // aliases + rules above
pub fn highlight_lines(syntax: &SyntaxReference, text: &str) -> Option<Vec<String>>;      // one HTML string per line, spans closed
pub fn builtin_code_theme(name: &str) -> Option<&'static syntect::highlighting::Theme>;  // exact two-face as_name(); None for ansi, base16, base16-256 (§3.3)
pub fn code_theme_css(theme: &syntect::highlighting::Theme) -> String;                   // css_for_theme_with_class_style(…, hl-)

// src/html/emit.rs (T13) — html-output.md §1.8 and §1.15, exact
pub struct MathRequest<'a> { pub source: &'a str, pub dialect: MathDialect, pub display: bool, pub span: Option<&'a SourceSpan> }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]   // what was emitted; drives math_init() (§4.3)
pub struct MathUsage { pub katex: bool, pub mathml: bool, pub typst_mathml: bool, pub image: bool }
pub trait MathHook {   // returns exactly one element, `<span class="math inline|display" …>`, without id or positions
    fn equation(&mut self, eq: &MathRequest<'_>, assets: &mut AssetRegistry, diagnostics: &mut Diagnostics) -> String;
    fn usage(&self) -> MathUsage; }
pub struct KatexOnly { pub usage: MathUsage }   // T13's hook: unit tests, and the pipeline until T1A's MathRenderer
pub struct EmitOptions<'a> { pub config: &'a Config /* effective */, pub positions: bool /* §1.14 */,
    pub includes: &'a [PathBuf] /* = document.includes */, pub math: &'a mut dyn MathHook,
    pub assets: &'a mut AssetRegistry /* equation images (T1A) */, pub diagnostics: &'a mut Diagnostics }
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Emitted { pub title_block: String, pub body: String, pub footnotes: String, pub toc: String, pub main: String, pub math: MathUsage }
pub fn emit(document: &Document, opts: &mut EmitOptions<'_>) -> Emitted;
pub fn escape(text: &str) -> String;
/// Every class the emitter writes on its own (not the user's); §7 requires the default theme to style each.
pub const MARKUP_CLASSES: &[&str] = &[/* the 70 names of html-output.md §1.15, verbatim and in order */];
```
Allowed additions only: `#[derive(Debug, Default)]` + `impl MathHook` on `KatexOnly`; private and `pub(crate)` helpers.

## Steps
1. On `main`, confirm T01's pieces exist: `crate::ir::*` (with `ir::plain_text`, `CellOutput::pick`, `Block::span`),
   `crate::config::{Config, Toc}`, `crate::error::{Diagnostic, Diagnostics, Level}` (ir.md §5: `push, info, warn, error`),
   `crate::assets::registry::AssetRegistry`, `crate::markdown::code::CodeAttrs`, `crate::resolve::{LangTag, Term, term,
   supplement, caption_separator}`, `tests/fixtures/ir/sample-document.json`. If one is missing (or `src/html/mod.rs`
   lacks `pub mod emit;`/`pub mod highlight;`), stop and ask the coordinator; never write substitutes.
2. `highlight.rs`: `SYNTAXES` (`LazyLock::new(two_face::syntax::extra_newlines)`), a private
   `const STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" }` and a private
   `static THEMES: LazyLock<EmbeddedLazyThemeSet> = LazyLock::new(two_face::theme::extra)`. `resolve_syntax`: `None` or
   `plain` → `Plain`; `lang` ∈ `text plain txt none output ps1 powershell` → `Plain` (check this list first: `txt` would
   find "Plain Text"); map `console→bash, shell→bash, jsx→js, fortran→f90, ipynb→json`; `SYNTAXES.find_syntax_by_token`
   → `Syntax(s)`, else `Unknown`. Compare tokens exactly as written (the spec lists lowercase).
3. `highlight.rs`: `highlight_lines` = html-output.md §10.1's code, copied verbatim (its `open_tags` re-opens the scopes
   still open from the previous line; an empty line yields `""`; any `parse_line`/span error → `None`).
4. `highlight.rs`: `builtin_code_theme(name)`: `None` for `ansi`, `base16`, `base16-256`; else the
   `EmbeddedLazyThemeSet::theme_names()` entry whose `as_name() == name` (exact, case-sensitive) → `Some(THEMES.get(n))`.
   `code_theme_css(theme)` = `css_for_theme_with_class_style(theme, STYLE)` (infallible in practice, Hints). T15 calls both.
5. `highlight.rs`: a `pub(crate)` helper that builds §1.5's `LINES` from a `&SyntaxChoice`, the block text and
   `&CodeAttrs`: drop one trailing `\n`, read `\r\n` as `\n`, split on `\n` (empty text → no lines and no
   `highlight_lines` call); `Syntax(s)` → `highlight_lines(s, &(normalized + "\n"))`, `None` → plain; plain/`Unknown`
   line content = `escape(line)`; wrap each in `<span class="line">` or `<span class="line highlight">` when
   `code.is_highlighted(code.start + i)`; join with `\n`. Unit tests (Acceptance) live in this file.
6. `emit.rs` types: copy the API block; `MARKUP_CLASSES` verbatim from §1.15. `escape` (§2.1) maps exactly
   `& < > " '` → `&amp; &lt; &gt; &quot; &#39;` (syntect's `Escape` table) and nothing else.
7. `KatexOnly` (§1.8): LaTeX → `<span class="math inline" data-math-style="inline" data-tex="SRC">SRC</span>` (`display`
   likewise), `SRC = escape(source)`, sets `usage.katex`; typst (no usage flag) →
   `<span class="math inline math-source" data-tex="SRC"><code>SRC</code></span>` + warning `math-conversion-failed`
   (math.md §9 shape `html: cannot convert equation (<reason>); used source`; span = `eq.span`).
8. `emit.rs` attribute writer (§1.1): order `id`, `class` (emitter classes, then `attrs.classes`; none → no `class`),
   the element's own attributes, pass-through pairs in key order, positions last. Skip consumed keys (per element),
   `custom-style`, `ansi`; write `style`, `title` (only without an own title), `lang`, `dir`, `role`, `data-*`, `aria-*`
   as is, any other `k` as `data-k`. A written name equal to one the emitter writes on that element (own or position):
   `style` merges after `;`, anything else is skipped (local choice: also skip a second pair writing the same name, e.g.
   `x` and `data-x`). Positions (§1.14, only when `opts.positions` and the block's `span` is `Some`):
   `data-sourcepos="S-E"`, `data-source` (`0` main; `k+1` when `opts.includes[k] == span.file`), `data-cell` when
   `span.cell` is `Some` (not again on `div.cell`); never on raw HTML, inlines, `li`/`tr`/`td`, abstract blocks.
9. Footnote plan (a pre-pass; T13 decides how, §1.11 fixes the output): walk in references.md §4.3 order (abstract
   first; a `Footnote`'s blocks right after it; a `Cite`'s `resolved.inlines`; for cells `pick("html")` only). Record
   per number `n` its blocks (`BTreeMap<u32, &[Block]>`) and, per `Footnote`/`FootnoteRef` inline, its `k` (the
   `Footnote` is 1, later refs 2, 3, …), e.g. in a `BTreeMap<*const Inline, u32>` keyed by address; count refs per `n`.
10. Inlines (§1.13, §1.11, §1.12): emitter state holds `lang` (`LangTag::effective(&meta, config)`, `None` → `LangTag {
    lang: "en".into(), region: None }`), an anchor depth, a deferred-notes buffer and a stack of enclosing block spans
    (for `MathRequest.span`; `None` in title block and abstract). Inside any `<a>` the emitter writes and inside `Link`
    content: `Link` → its inlines only, `Ref` → its text only, `Footnote`/`FootnoteRef` go to the buffer and are written
    right after the outermost `</a>`; TOC entries write no inline `id`. `number: None` notes write nothing. Unresolved
    `Ref` → `<span class="ref unresolved">[?TARGET]</span>`; unresolved `Cite` → `[?K1; K2]` in `citation unresolved`.
11. Blocks (§1.4) under W1–W4 exactly: headings with the number span, paragraphs (tight-list rule), blockquote, lists
    (`start` ≠ 1, `task-list`, task items with the checkbox), `dl`, `Callout` (default titles via `term`, other kinds
    first character uppercased; consumes `kind`, `title`), `Div` (`columns` `widths` → `style="grid-template-columns:V"`;
    `column` drops `width`), `Raw` html (+`\n` if missing; other formats nothing), `<hr>`, `div.page-break`.
12. Code blocks (§1.5): `CodeAttrs::from_attrs(&attrs)`; notebook outputs (a class `output-text`, `output-stream-*`,
    `output-error`, `output-json`) are `Plain` without calling `resolve_syntax`; else `resolve_syntax(lang, code.plain)`;
    `Unknown` → plain + info `code-language-unknown` (span = block span; local choice: path = `span.file`, else
    `document.source.path`). Classes `hl-code`, `numbered`, then `attrs.classes` minus `numbered`/`plain`;
    `style="counter-reset:line S-1"` only numbered with `start ≠ 1`; consumed pairs `start highlight lines caption
    include trim escape ansi` (`ansi` is never printed: T18 adds the ANSI arm).
13. Figures (§1.6: class order, `figcaption` first for `Table`/`Listing`, caption label with U+00A0 and `TAIL`) and
    tables (§1.7: `colgroup` with `width × 100` at most two decimals, trailing zeros/dot trimmed; alignment from the
    cell or from the grid column the cell starts in, tracking `rowspan`s per section; `scope` per head/body).
14. Math (§1.8): call `opts.math.equation(&MathRequest{…}, opts.assets, opts.diagnostics)` for every `Inline::Math`
    and `Block::Math`; display: `<div id class="equation[ numbered]" …>` + element + `(N)` span + `</div>`, one line.
15. Cells (§1.9): `data-cell` always; input only with `show_input` (§1.5 on a synthetic `CodeBlock`, language from
    `cell.language`, code attributes only, no id, no span); prompts only with `config.notebook.show_prompts`; outputs
    only with `show_output`, `pick("html")`, `data-mime` from the candidate whose `blocks` `std::ptr::eq` the pick; an
    empty pick writes nothing; no rendered output → no `cell-outputs`; nothing rendered at all → no `div.cell`.
16. TOC (§1.10: nesting by level, `refs` entry text, `toc-title`) for `Block::Toc` and the `toc` fragment
    (`level ≤ D`, `D` from `Toc::Depth(n)`, else 3, no positions, `""` when empty); bibliography (§1.12: `id="refs"`,
    `hanging-indent`, `references-title`, entries with/without `prefix`).
17. Footnotes fragment (§1.11, backlinks on the last `<p>` or a new `<p>`), title block (§1.3; `abstract-title`), then
    `main` (§1.2) and `Emitted.math = opts.math.usage()`. Use `BTreeMap`s only; no `HashMap` iteration.
18. Write `tests/html_emit.rs` (Acceptance), bless snapshots with `INSTA_UPDATE=always cargo test --test html_emit`,
    read both `.snap` files line by line against §1 before committing, then run the four checks.

## Acceptance criteria
- `tests/html_emit.rs::sample_build` / `sample_positions`: `sample-document.json` (T01) deserialized, `Config::default()`,
  `KatexOnly::default()`, positions off/on → `insta::assert_snapshot!` of `Emitted.main` into
  `tests/snapshots/html_emit__sample_{build,positions}.snap`; both assert `main` = `"<main data-writer-document>\n" +
  title_block + body + footnotes + "</main>\n"` and `math.katex`; for the blocks of ir.md §6 (the fixture is its larger
  version) the build snapshot holds `<a class="ref" href="#fig-throughput">Figure 1</a>` (U+00A0),
  `<span class="citation" data-cites="knuth1984">[1]</span>`, `<span class="equation-number">(1)</span>`; the positions
  one has `data-sourcepos="S-E" data-source="0"` on every block with a span and none in the title block.
- `tests/html_emit.rs`, one test per row of §1.4 and §1.13 on hand-built IR, asserting exact strings
  (`heading_with_number`, `paragraph_and_tight_list`, `task_lists`, `ordered_list_start`, `definition_list`, `blockquote`,
  `callout_titles`, `div_columns`, `raw_blocks`, `breaks`, `figure_captions`, `table_colgroup_spans_align`,
  `display_and_inline_math`, `code_block_numbered_start` (`style="counter-reset:line 4"`), `cell_prompts_outputs`,
  `toc_nested_and_fragment`, `bibliography_entries`, `title_block_omissions`, `inline_formatting`,
  `code_link_image_inlines`, `refs_cites_unresolved`, `breaks_spans_raw_inline`), plus §9's cases: `label_fn_1_both_ids`
  (no diagnostic), `footnote_inside_link`, `footnote_body_rereferences_earlier_note` (§1.11 `k` order),
  `duplicate_passthrough_keys`, `output_code_ansi_pair_not_printed` (no `data-ansi`), and `escape_five_characters`,
  `typst_math_katex_only_fallback` (warning), `unknown_language_info`, `markup_classes_cover_sample` (every class token of
  both snapshots except `hl-*`/`language-*` is in `MARKUP_CLASSES`).
- `src/html/highlight.rs` `#[cfg(test)]`: `python_docstring_reopens_spans` (`lines[3]` and `lines[2]` exactly as in
  Hints), `numbered_start_highlight` (`start=5 highlight="6-7"` marks lines 2–3
  `line highlight`), `unknown_language` (`nosuchlang` → `Unknown`), `plain_class_and_plain_languages` (`.plain`,
  `None`, the seven plain tokens), `aliases` (Hints table), `empty_text_no_lines`, `builtin_themes` (`InspiredGitHub`
  `Some`, `ansi`/`base16`/`base16-256`/`inspiredgithub` `None`; `code_theme_css` lengths 15,772 and 3,334 B for
  InspiredGitHub/OneHalfDark, starting `/*` and containing `.hl-code {`).
- Running `emit` twice on the sample yields identical `Emitted` (determinism, `sample_build`).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
Resolving `asset:` URLs (T14); theme, `document.html`, `math_init()`, code CSS in the theme stylesheet (T15); pipeline and
Markdown-fixture snapshots (T16); resolve (T17); the `ansi` arm and notebook snapshots (T18); CSS (T19); `MathRenderer`
(T1A); `check --target html` (T71); the client (T22); DOCX/LaTeX highlighting (T40, T60). Never edit `src/html/mod.rs`.

## Hints (verified)
- Pins (vendored-assets.md §7.3; T00 writes them): `syntect = "=5.3.0"` (df, `default-fancy`), `two-face = "=0.5.2"` (df,
  `syntect-fancy`); sources in `~/.cargo/registry/src/index.crates.io-*/{syntect-5.3.0,two-face-0.5.2+bat-0.26.1}/`.
  `SyntaxSet::load_defaults_*`/`ThemeSet::load_defaults` are clippy `disallowed-methods` (vendored-assets.md §3.3).
- syntect 5.3.0 (R6 §A.1, read in the crate): `ParseState::new(&SyntaxReference)`, `ParseState::parse_line(&mut self,
  line: &str, syntax_set: &SyntaxSet) -> Result<Vec<(usize, ScopeStackOp)>, ParsingError>`;
  `syntect::html::line_tokens_to_classed_spans(line: &str, ops: &[(usize, ScopeStackOp)], style: ClassStyle, stack: &mut
  ScopeStack) -> Result<(String, isize), Error>`; `css_for_theme_with_class_style(&Theme, ClassStyle) -> Result<String,
  Error>` (`src/html.rs:146`, only pushes to a `String` and returns `Ok`: an `expect` with that comment is acceptable).
  Escape table `src/escape.rs:34–38`: `> < & ' "` → `&gt; &lt; &amp; &#39; &quot;`.
- two-face 0.5.2 (R6 §A.1): `two_face::theme::extra() -> EmbeddedLazyThemeSet`, `EmbeddedLazyThemeSet::theme_names() ->
  &'static [EmbeddedThemeName]`, `get(&self, EmbeddedThemeName) -> &Theme`, `EmbeddedThemeName::as_name(self) -> &'static str`.
- Probe (scratch crate, exact pins, run 2026-10-25 for this file): `static THEMES: LazyLock<EmbeddedLazyThemeSet>`
  (→ `&'static Theme`), `match SYNTAXES.find_syntax_by_token(t) { Some(s) => SyntaxChoice::Syntax(s), None =>
  SyntaxChoice::Unknown }` and §10.1's `highlight_lines` compile unchanged. `find_syntax_by_token`: `bash`→"Bourne Again
  Shell (bash)", `js`→"JavaScript", `f90`→"Fortran (Modern)", `json`→"JSON", `typst`→"Typst", `PYTHON`→"Python" (names
  case-insensitive, extensions exact), `txt`→"Plain Text"; `text plain none output console jsx fortran ps1 nosuchlang`→
  `None`. `highlight_lines(py, "\n")` → `Some([""])`. For `def f():\n    """Doc\n\n    string"""\n`: `lines[2] == ""`,
  `lines[3]` = `<span class="hl-source hl-python"><span class="hl-comment hl-block hl-documentation hl-python">    string<span
  class="hl-punctuation hl-definition hl-comment hl-end hl-python">&quot;&quot;&quot;</span></span></span>`.
  CSS bytes: InspiredGitHub 15,772 (its `theme.name` is "GitHub"), OneHalfDark 3,334, Nord 15,914.
- The code theme root rule is `.hl-code`, so `<pre class="hl-code">` is contract (R6 §A.1 "Gotcha"; §1.5).
- Unoptimised two-face loads its 213 syntaxes in ~35 ms (R6); T00's `[profile.dev.package."*"] opt-level = 2` keeps
  tests fast (compiler-backed tests are slow in debug; T13 has none).

## Notes for the coordinator
- notebook-mapping.md Deviations says `MARKUP_CLASSES` gains the `ansi-*` classes; html-output.md §1.15 does not list
  them and §7 lists them separately for T19. T13 copies §1.15 verbatim (it never writes `ansi-*`); T18 may add them.
- references.md §4.3 walks every cell output candidate, the HTML emitter prints only `pick("html")`: T13 counts the
  `k` of `fnref-n-k` over emitted references only, so every backlink has a target.
- A `span.file` missing from `includes` is unspecified (frontends never produce it): T13 writes `data-source="0"`.
