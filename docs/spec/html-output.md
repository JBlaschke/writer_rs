# Spec: HTML output

**Status:** contract. Implemented by T13 (§1 markup, §2 escaping, `html::highlight`), T14 (§6.1 asset URLs per output
mode), T15 (§3 theme format, §4 template context and helpers, §5 styles, §6.2 page assembly), T19 (§7 the default
theme), T1A (the math renderer behind §1.8's hook). Consumers: T16 (pipeline, `dump --context`, snapshots), T18 (cell
markup), T1B (snapshots, `data-source`), T21 (serve routes, `resolve_helper_path`, the theme stylesheet), T22 (the
client hooks of §1.14), T25 (browser tests), T40 and T60 (`html::highlight` for DOCX runs and LaTeX tokens), T1C/T71
(`check` reports theme problems), T72 (`eject theme`), T74 (the guide).

The HTML output is one page: a theme template (`document.html`, minijinja) wraps markup that `html::emit` produces from
the resolved IR (`docs/spec/ir.md`); `assets::emit` resolves every `asset:<id>` placeholder for the output mode.
Templates never render IR nodes (no presenter `section/block/cell.html`). Theme machinery, helpers and asset emitter are
lifted from presenter_rs (R8 §3.4, §3.6, §5, §10 steps 9, 18–19) with the trims listed here.

## 1. Markup contract — T13 (`src/html/emit.rs`, `src/html/highlight.rs`)

### 1.1 General rules
- **Escaping:** every text and attribute value goes through `html::emit::escape` (§2.1).
- **Attribute order:** `id`, `class`, then the element's own attributes in the order this section writes them, then
  pass-through pairs (below) in key order, then position attributes (§1.14). `class` lists the emitter's classes first,
  then `attrs.classes` in source order; an empty class list writes no `class`. Boolean attributes are bare (`disabled`,
  `checked`, `data-writer-document`). Void elements have no slash (`<img …>`, `<br>`, `<hr>`, `<input …>`, `<col …>`).
- **Pass-through pairs** of `Attrs.pairs` (every IR node with `attrs`: `Heading`, `Div`, `Span`, `Callout`, `Figure`,
  `CodeBlock`, `Table`, `Math`, `Code`, `Link`, `Image`, `Cell`): a key the element consumes (listed per element) is
  skipped; `custom-style` (DOCX) and `ansi` (notebook-mapping.md §5) are skipped everywhere; `style`, `title` (only
  where the element has no title of its own), `lang`, `dir`, `role` and keys starting with `data-` or `aria-` are
  written as `key="value"`; any other key `k` is written `data-k="value"`. A key equal to an attribute the emitter
  writes on that element (its own or a position attribute) is merged: `style` is appended after `;` (as for `Image`);
  any other (`role`, `data-cell`, `data-mime`, …) is skipped.
- **Whitespace** (normative; the snapshots depend on it):
  W1 every block element ends with exactly one `\n` after its closing tag;
  W2 a container of blocks (`blockquote ul ol dl dd figure aside div section nav header table thead tbody tfoot tr
  colgroup main`, and `li` unless W3) writes `\n` after its opening tag, then its children (each ending in `\n`), then
  its closing tag; exception: a TOC item with children writes no `\n` after `<li>` (§1.10);
  W3 elements holding only inlines, or nothing, are one line (the emitter adds no `\n` inside):
  `p hN dt td th figcaption pre`, `li` whose content is a single tight paragraph or nothing, TOC items without children
  (§1.10), `div.equation`, `p.callout-title`, `div.cell-prompt`, `div.page-break`, and a bibliography entry
  (`div.csl-entry` with its `div.csl-left-margin`/`div.csl-right-inline`, §1.12); `pre` holds §1.5's `LINES` (joined
  by `\n`) and nothing else;
  W4 inline content contains `\n` only right after `<br>`, inside raw HTML, between the lines of a `pre` (§1.5) and
  inside text written verbatim (math sources in text and attributes, `Code` text, `DocMeta`).
  `SoftBreak` is one space (ir.md). Line endings are `\n` only.
- **Anchor context:** inside any `<a>` the emitter writes (refs, TOC entries, footnote references) and inside `Link`
  content, `Link` renders as its inlines only, `Ref` renders its text without `<a>`, and a `Footnote`/`FootnoteRef` is
  written right after the outermost `</a>` (so its number and backlink target exist); TOC entries write no inline `id`.
- **Language:** `lang` = `resolve::LangTag::effective(&meta, config)` (`None` → English); `term`, `supplement` and
  `caption_separator` are references.md §4.1's (`resolve::*`, written by T01 in `resolve/l10n.rs`, Deviations).
- **Generated ids:** `refs`, `ref-<key>`, `fn-<n>`, `fnref-<n>`, `fnref-<n>-<k>`. They are reserved by references.md
  §1.3 (`label-reserved`, raised by resolve); the emitter writes both ids and raises nothing.

### 1.2 Fragments and the morph root
`emit` returns the fragments of §1.15; `main` is the morph root, the only element with `data-writer-document`,
containing everything document-derived: `main = "<main data-writer-document>\n" + title_block + body + footnotes +
"</main>\n"`. `title_block` (§1.3), `body` (the blocks in order; `Toc` and `Bibliography` render where they stand) and
`footnotes` (§1.11) each end with `\n` or are `""`. `toc` (context `html.toc`, for sidebars) is §1.10's `<nav>` for
every `resolved.toc` entry with `level ≤ D` (`D` = `config.document.toc` depth, else 3), without position attributes,
`""` when empty; whatever a theme places outside `html.main` is not morphed by the client (live-reload.md §1.2), so a
theme that puts `html.toc` there shows a stale sidebar in serve until a full reload (T19's `README.md` says so); the
default theme never uses `html.toc`.

### 1.3 Title block
Rendered from `DocMeta`; each line is omitted when its value is absent, the `authors` div when there are no authors, the
whole `header` when all are absent. Title, subtitle and date are plain strings (escaped, never parsed).
```html
<header class="title-block">
<h1 class="title">GPU allocation</h1>
<p class="subtitle">Subtitle</p>
<div class="authors">
<p class="author"><span class="author-name">J. Blaschke</span> <span class="author-affiliation">EIT</span> <a class="author-email" href="mailto:j@x.org">j@x.org</a> <span class="author-id">0000-0002-…</span></p>
</div>
<p class="date">2026-10-24</p>
<section class="abstract" role="doc-abstract">
<p class="abstract-title">Abstract</p>
<p>One paragraph.</p>
</section>
</header>
```
`abstract-title` text: `meta.vars["abstract-title"]` when a string (pandoc's variable), else
`term(&lang, Term::Abstract)`. Abstract blocks get no position attributes (spans relative to the YAML string).

### 1.4 Blocks
`C` = container content per W2; `INL` = inlines (§1.13).

| IR | HTML |
|---|---|
| `Heading{level n, inlines, attrs, number}` | `<hn id class …>` + (`number` Some: `<span class="number">N</span>` + one space) + `INL</hn>`; `id` = `attrs.id` (references.md: resolve gives every heading listed in `resolved.toc` an `attrs.id` equal to its `TocEntry.anchor`); flat, no `<section>` wrappers |
| `Paragraph` | `<p>INL</p>`; in a `tight` list item: `INL` alone, followed by `\n` unless the item is a W3 `li` (the paragraph is its only block) |
| `BlockQuote` | `<blockquote>C</blockquote>` |
| `List{Bullet}` / `List{Ordered{start}}` | `<ul>` / `<ol start="S">` (`start` only when ≠ 1) with class `task-list` when any item has `checked: Some` |
| `ListItem` | `<li>`; task item: `<li class="task-list-item">` and `<input type="checkbox" disabled>` (`… disabled checked>` for `Some(true)`) + one space prepended to the first `Paragraph`'s content (inside its `<p>` when loose), or, when the first block is not a paragraph, `<li class="task-list-item"><input …>` then `\n`, then the blocks per W2 |
| `DefinitionList` | `<dl>`, per item `<dt>INL</dt>` then per definition `<dd>C</dd>` |
| `CodeBlock` | §1.5 |
| `Figure` | §1.6 |
| `Table` | §1.7 |
| `Math` (block) | §1.8 |
| `Callout{kind, title, blocks, attrs}` | `<aside id class="callout callout-KIND …" role="note">` + `<p class="callout-title">TITLE</p>` + blocks; `title: None` → `term(&lang, Term::Note|Tip|Important|Warning|Caution)` for the five kinds, any other kind with its first character uppercased (`Remark`); consumes `kind`, `title` |
| `Div{attrs, blocks}` | `<div id class …>C</div>`; a `columns` div's `widths` pair becomes `style="grid-template-columns:<value>"` (consumed); a `column` div's `width` pair is consumed and not written (presenter's rule: the parent's `widths` sizes columns) |
| `Raw{format:"html"}` | the text verbatim, plus `\n` if it does not end with one; other formats: nothing |
| `ThematicBreak` | `<hr>` |
| `PageBreak` | `<div class="page-break"></div>` |
| `Toc{depth}` | §1.10 |
| `Bibliography` | §1.12 |
| `Cell` | §1.9 |

### 1.5 Code blocks (`html::highlight`)
```
<pre id class="hl-code[ numbered][ classes]" style="counter-reset:line S-1" …><code class="language-LANG">LINES</code></pre>
```
- Attributes come from `markdown::code::CodeAttrs::from_attrs(&attrs)` (markdown-dialect.md §4.6; struct in
  Deviations). `numbered` is added when `CodeAttrs.numbered`; `style` only when numbered and `start ≠ 1` (value
  `start − 1`); `class="language-LANG"` only when `lang` is `Some` (verbatim, escaped). Consumed pairs: `start`,
  `highlight`, `lines`, `caption`, `include`, `trim`, `escape`, `ansi`; classes `numbered` and `plain` are not repeated.
- `LINES`: drop one trailing `\n`, read `\r\n` as `\n`, split on `\n` (empty text → no lines); each line is
  `<span class="line">…</span>`, or `<span class="line highlight">…</span>` when `CodeAttrs::is_highlighted(start + i)`
  (i 0-based, whether or not numbered); lines are joined by `\n` with no trailing newline. Empty normalized text is
  never passed to `highlight_lines`; both paths then write `<code …></code>` with no line spans.
- **Notebook outputs** (a class `output-text`, `output-stream-*`, `output-error` or `output-json`; notebook-mapping.md
  §4, §9) are never highlighted. With an `ansi` pair (§5 there) the line contents are
  `notebook::ansi::to_html(&parse(pair))` (one string per line, split as above; a styled span crossing a line break
  ends and restarts), each inside `<span class="line">` as above. `notebook::ansi` is T18's (wave 4): T13 skips the
  pair and writes the escaped `text`; T18 adds this branch to `src/html/emit.rs` (additive, named in its PR).
- Highlighting (`resolve_syntax`): `lang` mapped through the alias table `console→bash, shell→bash, jsx→js,
  fortran→f90, ipynb→json`; `None`, `.plain`, and `text|plain|txt|none|output|ps1|powershell` are `Plain` (not
  highlighted); otherwise `SYNTAXES.find_syntax_by_token` → `Syntax`; not found → `Unknown`, rendered as plain, and the
  HTML emitter raises info `code-language-unknown` (DOCX and LaTeX treat `Unknown` as `Plain`). The normalized text plus
  one `\n` goes to `highlight_lines` (§10.1), which returns one string per line of syntect's classed spans
  (`SpacedPrefixed { prefix: "hl-" }`): scopes still open from the previous line are re-opened at the line start, every
  span closes before the line's `</span>`, an empty line is `<span class="line"></span>`. A parse error anywhere renders
  the whole block unhighlighted. `<pre class="hl-code">` is contract: the code theme's root rule is `.hl-code` (§5).
- Shared API (also for docx-output.md's coloured runs and latex-output.md's `\Wr*` tokens):
  ```rust
  pub static SYNTAXES: LazyLock<SyntaxSet>;   // two_face::syntax::extra_newlines()
  pub enum SyntaxChoice { Plain, Unknown, Syntax(&'static SyntaxReference) }
  pub fn resolve_syntax(lang: Option<&str>, plain: bool) -> SyntaxChoice;                  // aliases + rules above
  pub fn highlight_lines(syntax: &SyntaxReference, text: &str) -> Option<Vec<String>>;      // one HTML string per line, spans closed
  pub fn builtin_code_theme(name: &str) -> Option<&'static syntect::highlighting::Theme>;  // exact two-face as_name(); None for ansi, base16, base16-256 (§3.3)
  pub fn code_theme_css(theme: &syntect::highlighting::Theme) -> String;                   // css_for_theme_with_class_style(…, hl-)
  ```

### 1.6 Figures
```html
<figure id="fig-x" class="figure kind-image wide" …>
<img src="asset:3f9a1c2b7e0d" alt="Throughput per node" style="width:70%">
<figcaption><span class="caption-label">Figure 1:</span> Throughput per node</figcaption>
</figure>
```
- Class `figure kind-image|kind-table|kind-listing|kind-other` + `attrs.classes`; consumes `caption`, `kind`. `Image`
  figures write the image without its paragraph; others their blocks (sub-figures nest). `figcaption` is **first** for
  `Table`/`Listing`, **last** otherwise.
- Caption: `number` and `caption` both → `<span class="caption-label">LABEL</span>` + `TAIL` + caption inlines, where
  `LABEL` + `TAIL` = `SUP` + U+00A0 + `N` + `caption_separator(&lang)` and `TAIL` is its trailing ASCII spaces (English:
  `Figure 1:` + one space; references.md §4.5); number only → `<span class="caption-label">SUP N</span>` (U+00A0);
  caption only → inlines; neither → no `figcaption`. `SUP` = the figure's own `resolved.labels[id].supplement`, else
  `supplement(kind, &lang)` (`Image`/`Other` → `LabelKind::Figure`, `Table` → `Table`, `Listing` → `Listing`).

### 1.7 Tables
```html
<table>
<thead>
<tr>
<th scope="col"></th>
<th class="align-right" scope="col">nodes</th>
<th scope="col">GB/s</th>
</tr>
</thead>
<tbody>
<tr>
<th scope="row">0</th>
<td class="align-center" colspan="2">12.3</td>
</tr>
</tbody>
</table>
```
`<colgroup>` (before `thead`) only when some `ColumnSpec.width` is `Some`: one `<col>` per column,
`<col style="width:P%">` when it has a width (`P` = `width × 100` with at most two decimals, trailing zeros and dot
trimmed). `thead`/`tbody`/`tfoot` only when non-empty. `header: true` cells are `<th>` with `scope="col"` in head rows
and `scope="row"` elsewhere. Alignment class `align-left|align-center|align-right` from `TableCell.align`, else the
column's align at the cell's starting column (the first column not covered by an earlier cell of the row or a `rowspan`
from above); `Default` writes none. `colspan`/`rowspan` only when > 1. `Table.attrs` give `id`/classes/pairs; a
captioned table sits in its `Figure`.

### 1.8 Math (hook implemented by T1A per `docs/spec/math.md`)
The emitter never converts math. For every `Inline::Math` and `Block::Math` it calls the hook:
```rust
// src/html/emit.rs (T13)
pub struct MathRequest<'a> { pub source: &'a str, pub dialect: MathDialect, pub display: bool, pub span: Option<&'a SourceSpan> }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]   // what was emitted; drives math_init() (§4.3)
pub struct MathUsage { pub katex: bool, pub mathml: bool, pub typst_mathml: bool, pub image: bool }
pub trait MathHook {   // returns exactly one element, `<span class="math inline|display" …>`, without id or positions
    fn equation(&mut self, eq: &MathRequest<'_>, assets: &mut AssetRegistry, diagnostics: &mut Diagnostics) -> String;
    fn usage(&self) -> MathUsage; }
pub struct KatexOnly { pub usage: MathUsage }   // T13's hook: unit tests, and the pipeline until T1A's MathRenderer
// src/html/math.rs (T1A, math.md §4): the image and typst routes compile through the borrowed World
pub struct MathRenderer<'a> { /* private */ }
impl<'a> MathRenderer<'a> { pub fn new(config: &'a Config, world: &'a mut WriterWorld) -> MathRenderer<'a>; }   // + impl MathHook
```
`span` = the `Block::Math`'s span; for `Inline::Math` (no span in ir.md) the span of the nearest enclosing block that
has one (`None` in the title block and abstract).
- **KaTeX form** (`[math] html = "katex"`, LaTeX dialect; `KatexOnly` for every LaTeX equation):
  `<span class="math inline" data-math-style="inline" data-tex="SRC">SRC</span>` and the same with `display`; `SRC` =
  the escaped source verbatim in T13's `KatexOnly`; in T1A's `MathRenderer`, the escaped
  `math::latex_source::prepare(source).0` (math.md §4.1). The span content and `data-tex` are always the same text; T1A
  updates the T13/T16 snapshots that change (equations with `\tag`, `\label` or normalised environments; additive,
  named in its PR). `math.js` (math.md §4.2) renders exactly `[data-math-style]:not([data-rendered])` from the text
  content; `data-tex`, on every form (math.md §4.1), is the morph guard's key (§1.14). Sets `usage.katex`.
- **MathML and image forms** and every fallback: math.md §4 (T1A's `html::math::MathRenderer`): the root rule above
  without `data-math-style`, setting `usage.mathml` (math-core), `usage.typst_mathml` (the compiler's MathML, needing
  `EQUATION_CSS`) or `usage.image`. `KatexOnly` renders typst-dialect math (warning `math-conversion-failed`) as
  `<span class="math inline math-source" data-tex="SRC"><code>SRC</code></span>` (`display` likewise).
- **Display equations** (`Block::Math`): `<div id class="equation[ numbered]" …>` + the hook's element + (`number` Some:
  `<span class="equation-number">(N)</span>`) + `</div>`, one line (W3); `id` = `attrs.id`; inline math: the element.

### 1.9 Notebook cells (`Block::Cell`, notebook-mapping.md)
```html
<div id="cell-x" class="cell cls" data-cell="3" …>
<div class="cell-input">
<div class="cell-prompt">In [2]:</div>
<pre class="hl-code"><code class="language-python"><span class="line">…</span></code></pre>
</div>
<div class="cell-outputs">
<div class="output output-result" data-mime="text/html">
<div class="cell-prompt">Out [2]:</div>
…blocks…
</div>
</div>
</div>
```
- `data-cell` = `Cell.index`, always (identity, not a position attribute); `div.cell` writes `Cell.attrs.id`,
  `Cell.attrs.classes` without `numbered`/`plain`, and the other pairs per §1.1 (`start`, `highlight`, `lines`
  consumed). Input only when `show_input`: §1.5 applied to `CodeBlock { lang, text: source, attrs }` with
  `lang = (!cell.language.is_empty()).then(|| cell.language.clone())` (an empty language writes no `language-` class
  and no `code-language-unknown`) and `attrs` holding only the code attributes of `Cell.attrs` (classes
  `numbered`/`plain`, pairs `start`/`highlight`/`lines`; notebook-mapping.md §2), with no id and no span. Prompts only
  with `config.notebook.show_prompts`: `In [N]:` (`In [ ]:` without a count); `Out [N]:` first in a `Result` with a count.
- Outputs only when `show_output`: `CellOutput::pick("html")`'s blocks, ordinary rules, inside
  `<div class="output KIND" data-mime="M">`; `KIND` = `output-stream NAME` (the stream name), `output-error`,
  `output-display` or `output-result`; `M` = the `mime` of the candidate whose `blocks` is the picked slice
  (`std::ptr::eq`). An empty pick writes nothing; no outputs → no `cell-outputs`; neither input nor outputs → nothing.

### 1.10 Table of contents (`Block::Toc{depth}`)
```html
<nav class="toc" role="doc-toc">
<p class="toc-title">Contents</p>
<ol>
<li><a href="#sec-intro"><span class="number">1</span> Introduction</a>
<ol>
<li><a href="#sec-x"><span class="number">1.1</span> Detail</a></li>
</ol>
</li>
<li><a href="#sec-method">Method</a></li>
</ol>
</nav>
```
Entries = `resolved.toc` with `level ≤ depth`; an entry is the child of the nearest earlier entry with a lower level,
else top-level. Inlines in anchor context (§1.1); no number span when `number` is `None`; an item with children writes
`<li><a…>…</a>\n<ol>\n…</ol>\n</li>` as shown. The entry whose `anchor` is `refs` (references.md §4.8) renders
§1.12's heading text (`reference-section-title` when a string, else its inlines) instead of its inlines. Title:
`meta.vars["toc-title"]` when a string, else `term(&lang, Term::Contents)`. No entries → nothing.

### 1.11 Footnotes
- `Inline::Footnote{number n}`: `<sup class="footnote-ref"><a id="fnref-n" href="#fn-n" role="doc-noteref">n</a></sup>`;
  the k-th later `FootnoteRef` to n (k = 2, 3, …) uses `id="fnref-n-k"`, k counting the references to n in references.md
  §4.3 document order (abstract first; a footnote's blocks at its first reference), not in emission order; a
  `Footnote` or `FootnoteRef` with `number: None` (never after resolve) writes nothing.
- `footnotes` fragment, items in number order, the backlinks (`↩︎` = U+21A9 U+FE0E) of every reference appended to the
  last `<p>` of the item (or as a new `<p>` when the last block is not a paragraph), separated by one space:
```html
<section class="footnotes" role="doc-endnotes">
<hr>
<ol>
<li id="fn-1">
<p>Text. <a class="footnote-back" href="#fnref-1" role="doc-backlink">↩︎</a> <a class="footnote-back" href="#fnref-1-2" role="doc-backlink">↩︎<sup>2</sup></a></p>
</li>
</ol>
</section>
```
  The k-th backlink (k ≥ 2) shows `↩︎<sup>k</sup>`. Abstract footnotes come first (markdown-dialect.md §2.2).

### 1.12 Citations, bibliography, references
- `Inline::Cite`: `<span class="citation" data-cites="K1 K2">INL</span>` (item keys, one space); `resolved: None`
  (unresolved IR only; resolve replaces a failing cite by references.md §4.7's `Span.unresolved-reference`) →
  `<span class="citation unresolved" data-cites="K1 K2">[?K1; K2]</span>`, the text of that placeholder. Note-class
  footnotes are made by resolve (references.md owns footnote numbers); the emitter never makes footnotes.
- `Block::Bibliography` with `resolved.bibliography` `Some` (else nothing):
```html
<section id="refs" class="references csl-bib-body hanging-indent" role="doc-bibliography">
<h1 class="references-title">Bibliography</h1>
<div id="ref-knuth1984" class="csl-entry"><div class="csl-left-margin">[1]</div><div class="csl-right-inline">D. E. Knuth, <em>The TeXbook</em>. Addison-Wesley, 1984.</div></div>
</section>
```
  `hanging-indent` only when `hanging_indent`; the level-1 heading is always written (references.md §4.8 lists it in
  `resolved.toc` with anchor `refs`), its text `meta.vars["reference-section-title"]` when a string, else
  `term(&lang, Term::Bibliography)`; without `prefix` an entry is `<div id class="csl-entry">INL</div>`.
- `Inline::Ref`: `<a class="ref" href="#ANCHOR">TEXT</a>` (`resolved.text` in anchor context; `ANCHOR` =
  `resolved.anchor`, the label verbatim); empty anchor → `<span class="ref">TEXT</span>`; unresolved →
  `<span class="ref unresolved">[?TARGET]</span>`. `RefForm` changes nothing; `Page` renders as Normal, which
  `check --target html` reports as `unsupported-construct` (§8).

### 1.13 Inlines
| IR | HTML |
|---|---|
| `Text` | escaped text |
| `Emph`, `Strong`, `Strikeout`, `Subscript`, `Superscript`, `Underline` | `<em>`, `<strong>`, `<del>`, `<sub>`, `<sup>`, `<u>` |
| `SmallCaps` | `<span class="smallcaps">` |
| `Code{text, attrs}` | `<code id class …>text</code>` (never highlighted) |
| `Math` | §1.8 |
| `Link{inlines, url, title, attrs}` | `<a id class href="URL" title="T" …>INL</a>` (`title` only when `Some`); content in anchor context |
| `Image{alt, url, title, attrs}` | `<img id class src="URL" alt="ALT" title="T" style="…" …>`; `ALT` = `ir::plain_text(alt)`; `width`/`height` (consumed) → `style="width:V;height:V"` (bare numbers get `px`), a `style` pair appended after `;` |
| `Footnote`, `FootnoteRef` | §1.11 |
| `Ref`, `Cite` | §1.12 |
| `LineBreak` | `<br>` + `\n` |
| `SoftBreak` | one space |
| `Span{attrs, inlines}` | `<span id class …>INL</span>` |
| `Raw{format:"html"}` | verbatim; other formats nothing |

URLs (`asset:<id>`, remote, `#label`) are written as in the IR, escaped; `assets::emit` resolves placeholders (§6.1).

### 1.14 Position attributes and client hooks (producer T13, consumer T22)
- Only when `EmitOptions::positions` (serve, or `[markdown] sourcepos = true`), last in the attribute list of the
  outermost element of every block whose `span` is `Some` (inside containers and footnotes too):
  `data-sourcepos="START-END"` (the span's 1-based lines), `data-source="I"` (`0` = main file, `k+1` for
  `document.includes[k] == span.file`) and `data-cell="C"` when `span.cell` is `Some` (not repeated on `div.cell`).
  Never on raw HTML, inline elements, `li`/`tr`/`td`, abstract blocks, or blocks without a span.
- Client hooks (live-reload.md): the single `[data-writer-document]`; `#writer-theme` (§4.2); math `data-tex` (an
  already rendered element whose old and new `data-tex` are equal is not morphed); `math.js` marks rendered elements
  `data-rendered="1"` and exposes `window.__writerRenderMath(root)`.

### 1.15 API
```rust
// src/html/emit.rs (T13)
pub struct EmitOptions<'a> { pub config: &'a Config /* effective */, pub positions: bool /* §1.14 */,
    pub includes: &'a [PathBuf] /* = document.includes */, pub math: &'a mut dyn MathHook,
    pub assets: &'a mut AssetRegistry /* equation images (T1A) */, pub diagnostics: &'a mut Diagnostics }
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Emitted { pub title_block: String, pub body: String, pub footnotes: String, pub toc: String, pub main: String, pub math: MathUsage }
pub fn emit(document: &Document, opts: &mut EmitOptions<'_>) -> Emitted;
pub fn escape(text: &str) -> String;
/// Every class the emitter writes on its own (not the user's); §7 requires the default theme to style each.
pub const MARKUP_CLASSES: &[&str] = &["title-block", "title", "subtitle", "authors", "author", "author-name", "author-affiliation",
  "author-email", "author-id", "date", "abstract", "abstract-title", "number", "task-list", "task-list-item", "hl-code", "numbered",
  "line", "highlight", "figure", "kind-image", "kind-table", "kind-listing", "kind-other", "caption-label", "align-left", "align-center",
  "align-right", "equation", "equation-number", "math", "inline", "display", "math-source", "math-error", "callout", "callout-note", "callout-tip",
  "callout-important", "callout-warning", "callout-caution", "callout-title", "page-break", "toc", "toc-title", "references",
  "csl-bib-body", "hanging-indent", "references-title", "csl-entry", "csl-left-margin", "csl-right-inline", "citation", "unresolved",
  "ref", "footnotes", "footnote-ref", "footnote-back", "smallcaps", "cell", "cell-input", "cell-prompt", "cell-outputs", "output",
  "output-stream", "stdout", "stderr", "output-error", "output-display", "output-result"];
```

> **Contract change (coordinator, 2026-10-26):** class-coverage tests distinguish
> emitter-owned classes from classes supplied by fixture `Attrs.classes` (for
> example `mark` and `columns`). The latter need not appear in `MARKUP_CLASSES`;
> the constant remains the exact 70-name vocabulary above.

## 2. Escaping and determinism — T13, T15

### 2.1 Escaping
`escape` maps `&`→`&amp;`, `<`→`&lt;`, `>`→`&gt;`, `"`→`&quot;`, `'`→`&#39;` and nothing else (identical to syntect's
`Escape`, so highlighted and plain code agree); it is used for text, attribute values (always double-quoted) and code.
Non-ASCII stays UTF-8. Raw HTML is written verbatim, never parsed or repaired. Inlined `<style>`/`<script>` text has
`</style`/`</script` written `<\/style`/`<\/script`; JSON in scripts goes through `script_safe_json` (§10.3: every `<`
becomes a JSON unicode escape); a script that would leave the tokenizer in "script data double escaped" is inlined as a
`data:` URI `src` (all lifted, R8 §3.6).

### 2.2 Determinism
Same `Document`, config, theme and output mode → byte-identical HTML and theme CSS: no `HashMap` iteration, no
timestamps, no `now`, no generator/version meta in the default theme, ids only from labels, keys and numbers, floats as
§1.7. Site file names hash their bytes; serve-only values (`build.id`, `?v=`) never appear in build output.

## 3. Theme directory format — T15 (`src/theme/*`, lifted from presenter theme-format.md §1–§3)

### 3.1 Layout
```
mytheme/
├── theme.toml               manifest (required)
├── templates/
│   ├── document.html        the page (required in the root theme; children may omit)
│   └── partials/*.html      optional includes (the default theme has head.html, scripts.html)
├── styles/*.css             listed in [styles] entries
├── assets/                  fonts, images: asset("x"), CSS url(x)
└── scripts/*.js             listed in [scripts] entries
```

### 3.2 `theme.toml`
```toml
name = "mytheme"               # required; valid in @<name>/ paths (presenter's check_theme_name)
extends = "default"            # optional: a built-in name or a directory relative to this file
description = "…"
[styles]
entries = ["styles/theme.css"] # .css only; concatenated into the theme stylesheet (§5)
code_theme = "InspiredGitHub"  # a two-face theme name (exact as_name()), a .tmTheme path relative to this file, or "none"
code_theme_dark = "OneHalfDark"# same; wrapped in @media (prefers-color-scheme: dark)
[scripts]
entries = []                   # e.g. ["scripts/extra.js"], loaded at the end of <body>
[vars]                         # free-form; templates read theme.vars.*
accent = "#2a76dd"
```
`ThemeManifest { name, extends, description, styles: StylesSection { entries, code_theme, code_theme_dark }, scripts:
ScriptsSection { entries }, vars }` (`src/theme/manifest.rs`, every table `#[serde(default, deny_unknown_fields)]`).
Trimmed from presenter: `reveal_theme`, `highlight_style`, `[reveal]`, `[layouts]`, `REVEAL_THEMES`; using one is
`Error::Theme` naming the replacement (`highlight_style` → `code_theme`; the others "has no meaning in writer").

### 3.3 Inheritance, resolution, chain (lifted)
- Child values override parent values key by key; `styles.entries`/`scripts.entries` concatenate **parent-first**; code
  themes are plain overrides (unset in the whole chain = `"none"`); `[vars]` deep-merge, child wins.
- `theme::resolve(spec, search_roots)`: a directory with `theme.toml` under a search root wins over a built-in name
  (`default`); not found → `Error::Theme` listing where it looked. T16's `pipeline::resolve_theme` (shared by build,
  serve and `check`) builds `search_roots` from where the effective `[html] theme` was set (config.md §1, §3): `--theme`
  → [CWD]; front matter, notebook metadata or `writer-doc.with` (`theme`, or `theme` in the `html` map) → [the
  document's directory]; `writer.toml` → [that file's
  directory]; default → [] (built-ins only). `extends` is child-relative; chain child → root, ≤ 8, no cycles or
  duplicates; root has `document.html`.
- A code theme value ending in `.tmTheme` (ASCII case-insensitive) is a path, `"none"` disables, any other value must
  equal a two-face `as_name()` exactly (names may hold spaces: `Solarized (dark)`). A path follows the rules of style
  entries (relative, inside its theme, existing) and is read with `ThemeSet::load_from_reader`; an unknown name or
  unreadable file is `Error::Theme` (`theme-code-theme` in `check`), with the closest two-face name as a hint. The
  terminal themes `ansi`, `base16` and `base16-256` are rejected like an unknown name (hint: "terminal-only theme; its
  colours are ANSI palette indices"; their CSS is near-black on black), so 29 of the 32 two-face themes are usable.
- **Symlink safety** (presenter A4, lifted): every lookup (templates, `assets/`, entries, code theme files, CSS `url()`
  targets, `/__writer/asset/`) opens a file only when its canonical path is inside the canonical theme directory; a link
  leaving it counts as missing, logs a warning, and `check` reports `theme-link-outside`. `check` reports what a build
  fails on as errors in the theme files: missing or escaping entries and code theme files, a stylesheet that does not
  compile, a broken template among `document.html` and what it names as string literals in
  `include`/`extends`/`import`/`from` (decided statically, presenter W8FIX); an unused broken template is a warning.
- Templates: minijinja 2.24, `set_loader` over the chain (child first); `@<theme>/<path>` reads that theme's
  `templates/<path>`; disk themes are re-read on every render; `UndefinedBehavior::Lenient`; auto-escape for `.html`;
  `set_debug(true)` in serve; errors → `Error::Template { template, line, message }` (`template_error`).

```rust
// src/theme/{mod,resolve,templates}.rs (T15); ThemeDir/ThemeFile/ThemeSource as presenter's
pub const REQUIRED_TEMPLATES: [&str; 1] = ["document.html"];   // MAX_CHAIN_LEN = 8, MANIFEST_FILE = "theme.toml"
pub fn resolve(spec: &str, search_roots: &[&Path]) -> Result<Theme>;   pub fn builtin_names() -> Vec<String>;
pub struct Theme { pub chain: Vec<ThemeDir> }   // child → root
impl Theme { pub fn name(&self) -> &str; pub fn vars(&self) -> serde_json::Map<String, serde_json::Value>;
    pub fn style_entries(&self) -> Result<Vec<ThemeFile>>; pub fn script_entries(&self) -> Result<Vec<ThemeFile>>;   // parent-first
    pub fn code_themes(&self) -> Result<(Option<CodeTheme>, Option<CodeTheme>)>;   // (light, dark); None = "none"
    pub fn read_template(&self, name: &str) -> Result<Option<(String, String)>>; pub fn read_asset(&self, rel: &str) -> Result<Option<Vec<u8>>>;
    pub fn watch_dirs(&self) -> Vec<PathBuf>; pub fn problems(&self) -> Vec<Diagnostic>;
    pub fn environment(&self, debug: bool) -> Result<minijinja::Environment<'static>>; }
pub struct CodeTheme { pub label: String /* name or @theme/path */, pub theme: syntect::highlighting::Theme }
pub fn recording_reads<R>(f: impl FnOnce() -> R) -> (R, BTreeSet<PathBuf>);   // theme files read (watch set)
```

## 4. Template context and helpers — T15 (`src/html/{mod,context,document,paths}.rs`)

### 4.1 Context (what `document.html` receives; `writer dump --context` prints it)
```jsonc
{
  "document": { /* the resolved IR, ir.md */ },  "meta": { /* = document.meta */ },
  "features": { "math": true, "code": true, "notebook": false, "citations": true, "footnotes": true },
  "config": { /* the effective Config, config.md */ },
  "html": { "main": "<main data-writer-document>…", "title_block": "…", "body": "…", "footnotes": "…", "toc": "…" },
  "math": { "katex": true, "mathml": false, "typst_mathml": false, "image": false },   // Emitted.math
  "title": "GPU allocation", "lang": "en",   // meta.title, else the source stem; meta.lang, else config.document.lang, else "en"
  "theme": { "name": "mytheme", "chain": ["mytheme", "default"], "code_theme": "InspiredGitHub",
             "code_theme_dark": "OneHalfDark", "vars": { "accent": "#2a76dd" }, "scripts": ["@default/scripts/x.js"] },
  "build": { "mode": "build", "standalone": true, "live_reload": false, "version": "0.1.0", "id": 0 }
}
```
`html.*` keep their `asset:` placeholders (resolved after rendering; `dump --context` shows them). `theme.scripts` are
`@<theme>/<path>` helper paths, chain parent-first. Templates insert `html.*` with `|safe`. Filters: minijinja
built-ins, `attrs` (a map → ` k="v"` pairs, escaped, `none` skipped), `json` (`tojson`); no `markdown` filter or `now`.

### 4.2 Helpers (all return HTML marked safe)
| helper | standalone | site | serve |
|---|---|---|---|
| `style(path)` | `<style>…</style>`, `url()`s as `data:` | `<link rel="stylesheet" href="assets/<name>-<hash>.css">` + file | `<link rel="stylesheet" href="/__writer/asset/<path>">` |
| `script(path)` | `<script>…</script>` | `<script src="assets/<name>-<hash>.js"></script>` + file | `<script src="/__writer/asset/<path>"></script>` |
| `asset(path)` | `data:` URI | `assets/<name>-<hash>.<ext>` + file | `/__writer/asset/<path>`, `?v=<content_version>` for theme-chain files |
| `theme_style()` | `<style id="writer-theme">…</style>` | `<link id="writer-theme" rel="stylesheet" href="assets/theme-<hash>.css">` | `<link id="writer-theme" rel="stylesheet" href="/__writer/theme.css?v=<build id>">` |
| `math_init()` | §4.3 | §4.3 | §4.3 |
| `live_reload()` | `""` | `""` | exactly `<script src="/__writer/live-reload.js"></script>` (live-reload.md §1.2; that script bundles morphdom, §1.3) |

Helper paths (one resolver, `html::paths::resolve_helper_path`, also behind the `/__writer/asset/{*path}` route) are
normalized (`/`, `\`, `.`, `..`; climbing out finds nothing), then resolved by first segment: `vendor/…` → the embedded
vendor tree; `writer/…` → `assets/writer/`; `@<theme>/<path>` → that theme's root directory; else the chain's `assets/`,
child first. Nothing found → `Error::Template` naming the path and where it looked.

### 4.3 `math_init()`
`""` unless `Emitted.math` has `katex`, `mathml` or `typst_mathml`; else these parts joined by `\n`: (1) with `katex`:
`style("vendor/katex/katex.min.css")`, `<script>window.__writerKatexOptions = JSON;</script>`
(`config.math.katex_options` through `script_safe_json`, `{}` when empty), `script("vendor/katex/katex.min.js")`,
`script("writer/math.js")`; (2) with `mathml` or `typst_mathml`: `<style id="writer-math">\n` + (`[math] mathml_font
= "embed"`: `@font-face { font-family: "Latin Modern Math"; src:
url("<asset("vendor/latin-modern-math/latinmodern-math.woff2")>") format("woff2"); }\n`) + `math { font-family: "Latin
Modern Math", "STIX Two Math", "Cambria Math", "Libertinus Math", math; }` + (with `typst_mathml`: `\n` +
`crate::typst::html::EQUATION_CSS`, T03's const of typst-dialect.md §9) + `\n</style>`. Vendor paths are
vendored-assets.md §2's; `math.js` is T1A's lift of presenter's (globals `__writerRenderMath`, `__writerKatexOptions`).
The image form needs nothing here.

### 4.4 `document.html` and rendering
T15 writes the default `templates/document.html` (T19 may restyle it, keeping these rules): `<html lang="{{ lang }}">`;
`<head>`: `partials/head.html` (charset, viewport, `<title>{{ title }}</title>`, description/author/keywords metas),
`{{ math_init() }}`, `{{ theme_style() }}` as the **last** stylesheet; `<body>`: `{{ html.main|safe }}` once, then
`partials/scripts.html` (`script()` per `theme.scripts`, then `{{ live_reload() }}`); blocks `head`, `styles`,
`body_start`, `main`, `scripts`, `body_end`. After rendering, T15 checks `html_count_attr(&html,
"data-writer-document") == 1` (§6.1; else `Error::Template` for `document.html`) and, in serve mode, that
`live_reload()` was called (else warning `theme-template`), then resolves placeholders with one `Emitter` pass.
```rust
// src/html/mod.rs (T15; lifted render/mod.rs): the types below, BuildMode { Build, Serve } (serde lowercase) and
// BuildInfo { mode, standalone, live_reload, id: u64, version: &'static str } with build(standalone)/serve(id) as presenter's,
// plus presenter's re-exports and `pub use document::{render, serve_stylesheet, SERVE_HELPER_PREFIX, SERVE_THEME_CSS};`.
// src/html/document.rs (T15; lifted render/html.rs) defines render, serve_stylesheet and the two consts.
pub struct RenderInput<'a> { pub document: &'a Document, pub assets: &'a mut AssetRegistry, pub theme: &'a Theme,
    pub config: &'a Config /* effective */, pub output: OutputMode, pub build: BuildInfo, pub math: &'a mut dyn MathHook }
pub struct RenderedDocument { pub html: String, pub theme_css: String, pub files: Vec<EmittedFile>,
    pub diagnostics: Diagnostics, pub theme_files: BTreeSet<PathBuf> }
pub fn render(input: RenderInput<'_>) -> Result<RenderedDocument>;   // emit → context → compile CSS → document.html → resolve
pub fn context_json(input: &mut RenderInput<'_>) -> Result<serde_json::Value>;   // dump --context (html/context.rs)
pub const SERVE_HELPER_PREFIX: &str = "/__writer/asset/";   pub const SERVE_THEME_CSS: &str = "/__writer/theme.css";
pub fn serve_stylesheet(theme: &Theme, key: &str, css: &str) -> Result<String>;   // a style() file in serve mode
```
`positions` = `build.mode == Serve || config.markdown.sourcepos`.

## 5. Styles — T15 (`src/html/styles.rs`, lifted `render/styles.rs` without the reveal fallback)
- **Compiled theme stylesheet** = the entries' absolute `@import`s (hoisted, below); per entry of the chain,
  parent-first, `/* @<theme>/<path> */\n` + its text (UTF-8, one BOM removed, `url()`s rewritten) + `\n`; then, only
  with `features.code`, for a light code theme `/* code theme: <label> */\n` + `code_theme_css(light)` + `\n`, and for a
  dark one `/* code theme (dark): <label> */\n@media (prefers-color-scheme: dark) {\n` + `code_theme_css(dark)` + `}\n`.
  When both are set the light CSS is wrapped too, in `@media not all and (prefers-color-scheme: dark) {\n` … `}\n`: 115
  of InspiredGitHub's 131 selectors have no rule in OneHalfDark's CSS, and its two-class rules (`.hl-storage.hl-type`)
  outrank the dark one-class rules, so unwrapped light rules leak into dark mode.
- `theme/styles.rs` (grass, 1,711 LOC) is not lifted. In the lifted `compile`, `compile_entry` becomes `decode_text` of
  a `.css` entry (other extensions, `.scss`/`.sass` included: `Error::Theme`, `theme-style-entry`, "SCSS themes arrive
  with T79"), and `inline_imports` becomes a scan of the entry's top-level `@import`s with presenter's
  `CSS_IMPORT`/`ImportRule` (lifted from that file; comments, strings and blocks skipped): an absolute URL
  (`remote_imports`' pattern) is removed and hoisted as `/* hoisted from <key> */\n<rule>` (presenter's format; in
  standalone output also the warning `remote-import`); any other is
  `Error::Style { file: <key>, message: "… list the file in [styles] entries" }`.
- **`url()` rewriting** (lifted): a relative `url()` in an entry resolves next to the entry, then `assets/<rel>` in the
  chain (child first), then a `vendor/…`/`writer/…` helper path; each must resolve (`Error::Style`) and becomes an
  `asset:<id>` placeholder (standalone, site) or `/__writer/asset/<helper path>?v=<content_version>` (serve; no `?v=`
  for vendor/writer files). `style()` of a vendor stylesheet is lenient (KaTeX's `.woff`/`.ttf` fallbacks stay).
- `id="writer-theme"` (§4.2), last stylesheet in `<head>`. Print rules, the `--w-*` properties and their dark values
  live in the entries (§7 for the default theme); dark mode is `prefers-color-scheme` only, like the dark code theme.
```rust
pub fn compile_theme_css(theme: &Theme, features: &Features, registry: &mut AssetRegistry) -> Result<String>;   // + serve variant (Targets::ServeUrls, lifted)
```

## 6. Emission modes and size budget — T14 (`src/assets/{mod,emit}.rs`), T15, T16

### 6.1 Asset URLs (lifted `assets/emit.rs`; prefix renamed)
```rust
// R6 §A.4 / R8 §11 signatures unchanged: Emitter::{new, with_files, resolve_html, resolve_css, resolve_css_in_assets_dir, url_for},
// EmittedFile { path, bytes }, data_uri (base64 STANDARD, padded), hashed_file_name, register_css_urls. Changed/added:
pub const SERVE_ASSET_PREFIX: &str = "/__writer/doc-asset/";
pub enum OutputMode { Standalone, Site { assets_dir: PathBuf, url_prefix: String }, Serve }
pub fn content_version(bytes: &[u8]) -> u64;   // blake3 first 8 bytes, LE (moved here from presenter's render/paths.rs)
```
| mode | `url_for(id)` | used by |
|---|---|---|
| `Standalone` | `data:<mime>;base64,…` | `build` (default, `[html] standalone = true`) → `<stem>.html` |
| `Site` | `url_prefix` + `hashed_file_name(hint, &content_hash(&bytes))` (the bytes, as presenter's `url_from` and ir.md §3; e.g. `assets/plot-5e0c7b1f9a2d.png`), file emitted | `--site` → `<stem>/index.html` + `<stem>/assets/` |
| `Serve` | `/__writer/doc-asset/<id>?v=<content_version of the asset's bytes>` | `serve` (the route serves `registry.read(id)`, ignoring the query) |

Placeholders are rewritten only in attribute values and `<style>`/`<script>` text; text and the text attributes
(presenter's `is_text_attribute`: `alt`, `title`, `placeholder`, `label`, `value`, `abbr`, `aria-*`) never; an
unresolvable placeholder is a build error (ir.md §3). The HTML scanner (`html_start_tags`, `html_attr_values`,
`HtmlScan`) moves from presenter's `markdown/render.rs` into `src/assets/mod.rs` as `pub(crate)` (T12 uses it for raw
HTML), plus T14's `pub(crate) fn html_count_attr(html: &str, attr: &str) -> usize`: the start tags outside comments and
raw-text elements that have `attr`, bare or valued (built on `HtmlScan::start_tags` like presenter's `has_math_markup`;
`html_attr_values` drops bare attributes).

> **Contract change (coordinator, 2026-10-26):** T14 caches file content versions by absolute
> path, length and modification time, hashing on a cache miss. Equal length and mtime reuse
> the version even if bytes changed; this is the accepted cache caveat. A deleted file raises
> `MissingAsset` in serve mode too. Standalone and serve drop an author's placeholder query
> while preserving its fragment; site keeps both. Serve writes the single `?v=` itself, so
> T23 must not lift presenter's later `version_file_assets` pass.

### 6.2 What each mode contains
- **Standalone:** one file, no request at load time except remote URLs the author wrote and absolute `@import`s
  (warning); KaTeX (JS, CSS, 20 woff2 as `data:`) only when `Emitted.math.katex`, Latin Modern Math (+≈ 518 KB) only
  when `Emitted.math.mathml || Emitted.math.typst_mathml` and `mathml_font = "embed"`; no `asset:` and no `/__writer/`
  substring remains (tested). **Site:** the same page with
  files under `assets/` (theme CSS `assets/theme-<hash>.css`). **Serve:** dev-server URLs (§4.2, §6.1), positions on,
  `live_reload()` scripts; the server keeps `RenderedDocument.theme_css` for `/__writer/theme.css` and, for
  `/__writer/doc-asset/{id}`, the registry `render` was given (`RenderInput.assets`), which also holds what the math
  hook registered (equation images): live-reload.md's `WebPage` gains `assets: AssetRegistry` (a clone of
  `prepared.assets` filled by `render`), which `WebBuild.assets` takes instead of `prepared.assets.clone()`.

### 6.3 Size budget (standalone; checked by T16 and T19)
| part | size |
|---|---|
| default theme entries (T19 limit) | ≤ 24 KiB |
| code themes, only with `features.code` | InspiredGitHub 15,772 B + OneHalfDark 3,334 B (measured; other two-face themes 1.2–22.9 KB, ≈ 16 KB is the planning figure) |
| document without math | ≈ 45–60 KB; T16 asserts `minimal.md` ≤ 60 KB |
| KaTeX inlined (only `math.katex`) | +644,746 B (js 273,081 + css 24,793 + 20 woff2 as data URIs 346,872; vendored-assets.md §6); T16 asserts `math.md` ≤ 700,000 B (KaTeX + theme entries ≤ 24 KiB + head, scripts and markup ≤ 30 KB; the fixture has no code, so no code CSS) |
| MathML (math-core or typst) with `mathml_font = "embed"` | +≈ 518 KB (388 KB × 4/3); `system`: +≈ 2 KB |
| images | bytes × 4/3 |

## 7. Default theme obligations — T19 (`assets/themes/default/`; T15 writes the first `theme.toml` and templates)
- `theme.toml`: `name = "default"`; `[styles] entries = […]` (T19's CSS files; T15 starts with `[]`),
  `code_theme = "InspiredGitHub"`, `code_theme_dark = "OneHalfDark"`; `[scripts] entries = []` (§3.2).
- **Classes:** every class of `html::emit::MARKUP_CLASSES`, the directive classes `columns`, `column`, `center`,
  `small`, `large` (markdown-dialect.md §5.5), the typst-frontend classes of typst-dialect.md §2.4
  (`span.overline { text-decoration: overline }`, `span.highlight { background: var(--w-code-highlight) }`, distinct
  from `.line.highlight`, and `div.left`/`div.center`/`div.right` setting `text-align`), resolve's
  `unresolved-reference` (references.md §4.7), the notebook classes `output-text`, `output-stream-stdout`,
  `output-stream-stderr`, `output-json` and `subcaption` (smaller text; notebook-mapping.md §4, §9), and the ANSI classes of notebook-mapping.md §5 (`ansi-<name>-fg|bg`,
  `ansi-<name>-intense-fg|bg` for its eight names, `ansi-bold|dim|italic|underline|strike|hidden`,
  `ansi-default-inverse-fg|bg`; colours from its `PALETTE`) appear as `.<class>` in a selector. Behaviour: a readable
  measure on `[data-writer-document]`; images `max-width: 100%`; booktabs-like tables (rules at `thead`/`tbody` edges,
  no vertical lines); `pre.hl-code` scrolls horizontally; `pre.numbered { counter-reset: line }` and
  `pre.numbered .line::before` numbering with `counter-increment: line`; `.line.highlight` background; `.equation`
  centres `.math.display` (horizontal scroll when too wide) with `.equation-number` at the right; callouts with an edge
  colour per kind (`--w-callout-<kind>`, other kinds `--w-callout-other`); `.task-list` and TOC lists without markers;
  `.hanging-indent .csl-entry` hanging, `.csl-left-margin`/`.csl-right-inline` side by side; `.unresolved` visibly
  marked; prompts in a left gutter, `.stderr`/`.output-error` tinted; `.page-break` hidden on screen only, by
  `@media screen { .page-break { display: none; } }` (never an unconditional `display: none`: a box-less element
  breaks no page in print); `.columns` a grid.
- **Tokens** on `:root`, one per line in `README.md`, contract-tested both ways by `tests/theme_tokens.rs`:
  `--w-font-body`, `--w-font-heading`, `--w-font-mono`, `--w-font-size`, `--w-line-height`, `--w-measure`, `--w-text`,
  `--w-bg`, `--w-muted`, `--w-accent`, `--w-link`, `--w-border`, `--w-code-bg`, `--w-code-highlight`,
  `--w-callout-{note,tip,important,warning,caution,other}` (T19 may add more); every colour token is redefined in
  `@media (prefers-color-scheme: dark) { :root { … } }`.
- **Self-contained:** system font stacks, no `@font-face`, no `@import`, no remote `url()`.
- **Print** (R6 §A.3, with `h1 { break-before: page }` dropped, page breaks added and the measure reset on the
  morph root, where this theme puts it):
```css
@page { size: A4; margin: 20mm 18mm; @bottom-center { content: counter(page); font: 9pt system-ui; } }
@page :first { @bottom-center { content: none; } }
@media print {
  html { font-size: 11pt; } body, [data-writer-document] { max-width: none; } body { background: #fff; color: #000; }
  h1, h2, h3 { break-after: avoid; } .page-break { display: block; break-after: page; }
  figure, table, pre, .callout, .equation, .cell, li { break-inside: avoid; }
  p { orphans: 3; widows: 3; } a[href^="http"]::after { content: " (" attr(href) ")"; font-size: 85%; }
  nav.toc, .no-print { display: none; } pre { white-space: pre-wrap; }
  * { print-color-adjust: exact; -webkit-print-color-adjust: exact; }
}
```
- `README.md`: layout of the theme, every token, the class list of §1 with one line each, how to extend
  (`extends = "default"`, `{% extends "@default/document.html" %}`), `eject theme`.

## 8. Diagnostics
| code | level | raised by | when |
|---|---|---|---|
| `code-language-unknown` | info | T13 | a code block's language has no syntax after aliases (§1.5) |
| `math-conversion-failed` | warning | T1A (T13's `KatexOnly` for typst math) | math.md |
| `remote-import` | warning | T15 | standalone stylesheet with an absolute `@import` |
| `theme-template` | error; warning for a template the build does not load and for a serve render without `live_reload()` | T15 | template problems (lifted presenter rules) |
| `theme-style-entry`, `theme-script-entry`, `theme-code-theme`, `theme-link-outside` | error (`theme-link-outside`: warning) | T15 via `check` | lifted from presenter (`theme-highlight-style` renamed, `theme-plugins` dropped) |

Hard failures: `Error::Theme`, `Error::Template`, `Error::Style` (ir.md §5), unresolvable placeholders. HTML renders
every IR construct. The one lossy case is `Ref { form: Page }`, which renders as Normal; `check --target html` reports
it as `unsupported-construct` (warning, references.md §6, T71); the emitter raises nothing for it. `Raw` of other
formats is skipped silently (`check`: `raw-skipped`).

## 9. Fixtures
| owner | what | where |
|---|---|---|
| T13 | unit tests per row of §1.4/§1.13 on hand-built IR, plus a label `fn-1` (both ids written, no diagnostic), a footnote inside `Link` content, a footnote body re-referencing an earlier note (§1.11 order), duplicate pass-through keys (§1.1), an output `CodeBlock` with an `ansi` pair (no `data-ansi`); snapshots of `tests/fixtures/ir/sample-document.json` (T01) through `emit` with `KatexOnly`, positions off and on | `tests/html_emit.rs` → `tests/snapshots/html_emit__sample_{build,positions}.snap` |
| T13 | highlighting: Python with a multi-line docstring (spans re-opened per line), `.numbered start=5 highlight="6-7"`, unknown language, `.plain` | unit tests in `html/highlight.rs` |
| T14 | `url_for` in the three modes, `?v=` changes with the bytes, text attributes untouched | `tests/assets.rs` (lifted + additions) |
| T15 | `tests/fixtures/themes/child-theme/`: `theme.toml` (`extends = "default"`, entries `["styles/child.css"]`, `code_theme = "Nord"`, `[scripts] entries = ["scripts/child.js"]`, `[vars] accent = "#aa3300"`), `templates/partials/head.html` (adds `<meta name="x-child" content="{{ theme.vars.accent }}">`), `styles/child.css` (`url(logo.svg)`), `assets/logo.svg`, `scripts/child.js`; tests: chain order, entry concatenation, code CSS only with `features.code`, `url()` in three modes, every helper in three modes, symlink refusal, missing `document.html`, presenter keys rejected, a `context_json` snapshot (version redacted); on temporary copies of the fixture: relative `@import` → `Error::Style`, absolute `@import` hoisted (+ `remote-import` in standalone), `.scss` entry → `theme-style-entry`, `code_theme = "styles/code.tmTheme"` loaded, light+dark code CSS both wrapped (§5), a template without `data-writer-document` or with two → `Error::Template`, a serve render without `live_reload()` → `theme-template` warning, `</script` in a script entry → `<\/script` | `tests/theme_resolve.rs`, `tests/html_render.rs` |
| T16 | build-mode `html.main` snapshot of every `tests/fixtures/md/*.md`: the harness calls the frontend (ir.md §4), then `resolve` with `lenient: true, library: None`, then `html::emit` with `KatexOnly`, positions off, and snapshots `html.main` whatever the diagnostics, with a diagnostics snapshot beside it; a fixture whose parse returns `Err` (`includes.md` once T1B adds the cycle check) is snapshotted as the error's Display text; ONE serve-mode full-page snapshot (`minimal.md`, `BuildInfo::serve(1)`, `OutputMode::Serve`: helper URLs, `live_reload()`, positions); size assertions of §6.3 | `tests/html_snapshots.rs` |
| T18 | `html.main` snapshots of every `tests/fixtures/ipynb/*.ipynb` (cell markup §1.9, ANSI lines §1.5); `outputs.ipynb`'s HTML holds no U+001B and no `data-ansi` | `tests/notebook_snapshots.rs` |
| T1A | `md/math.md` under `[math] html` = `katex`, `mathml`, `image` (math.md fixtures) | `tests/html_math.rs` |
| T1B | updates `references.md`/`includes.md` snapshots; adds `tests/fixtures/md/include-chain.md` (owner T1B: `:::include include/part-a.md`, which includes `include/part-b.md`; its Markdown and HTML snapshots too) and asserts its `data-source` values (0, 1, 2) with positions on | `tests/html_snapshots.rs` |
| T19 | coverage of every class §7 lists (including `overline`, `left`, `right` and a `span.highlight` rule), token test, entries ≤ 24 KiB, no `@import`/remote `url()`, no rule outside `@media screen` that hides `.page-break`; updates T16's `tests/snapshots/html_snapshots__*.snap` that its `document.html`/entries change (additive, named in its PR) | `tests/theme_tokens.rs`, `tests/default_theme.rs` |

## 10. Verified API hints
### 10.1 Per-line classed highlighting (probe `scratchpad/build/html-spec-probe`, syntect 5.3.0 + two-face 0.5.2, run 2026-10-24)
```rust
use std::sync::LazyLock;
use syntect::html::{line_tokens_to_classed_spans, ClassStyle};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
pub static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
const STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" };
fn open_tags(stack: &ScopeStack, out: &mut String) { for scope in stack.as_slice() {   // re-open scopes left open by the previous line
    let classes: Vec<String> = scope.build_string().split('.').map(|a| format!("hl-{a}")).collect();
    out.push_str("<span class=\""); out.push_str(&classes.join(" ")); out.push_str("\">"); } }
pub fn highlight_lines(syntax: &SyntaxReference, text: &str) -> Option<Vec<String>> {
    let (mut ps, mut stack, mut out) = (ParseState::new(syntax), ScopeStack::new(), Vec::new());
    for line in LinesWithEndings::from(text) {                 // text: one trailing '\n' guaranteed by the caller
        let ops = ps.parse_line(line, &SYNTAXES).ok()?;       // newline syntaxes need the '\n'
        let body = line.strip_suffix('\n').unwrap_or(line);
        let ops: Vec<_> = ops.into_iter().map(|(i, op)| (i.min(body.len()), op)).collect();
        let mut html = String::new(); open_tags(&stack, &mut html);
        let (spans, _) = line_tokens_to_classed_spans(body, &ops, STYLE, &mut stack).ok()?;
        html.push_str(&spans);
        for _ in 0..(html.matches("<span").count() - html.matches("</span>").count()) { html.push_str("</span>"); }
        out.push(if body.is_empty() { String::new() } else { html });
    }
    Some(out)
}
```
Probe output (docstring line 3 of 3; line 2 is empty and yields `""`): `<span class="hl-source hl-python"><span class="hl-comment hl-block
hl-documentation hl-python">    string<span class="hl-punctuation …">&quot;&quot;&quot;</span></span></span>`.
`find_syntax_by_token("nosuchlang")` → `None`. syntect escapes `& < > ' "` (`src/escape.rs`). `highlight_lines(py, "\n")`
→ `Some([""])` (hence §1.5's empty-text rule). Root rules (same probe): `ansi` `.hl-code { color: #000000;
background-color: #000000 }`, `base16`/`base16-256` `color: #070000` on `#000000` (§3.3).

### 10.2 Code theme CSS
```rust
let themes = two_face::theme::extra();                            // EmbeddedLazyThemeSet; 32 themes, names in R6 §A.1
let name = two_face::theme::EmbeddedLazyThemeSet::theme_names().iter().find(|n| n.as_name() == "InspiredGitHub");
let css = syntect::html::css_for_theme_with_class_style(themes.get(*name.unwrap()), STYLE)?;   // root rule ".hl-code { color; background-color }"
let custom = syntect::highlighting::ThemeSet::load_from_reader(&mut std::io::Cursor::new(bytes))?;   // .tmTheme (feature plist-load, in default-fancy)
```
Cargo (no `onig`, R6 §A.1; exact pins as vendored-assets.md §7.3):
`syntect = { version = "=5.3.0", default-features = false, features = ["default-fancy"] }`,
`two-face = { version = "=0.5.2", default-features = false, features = ["syntect-fancy"] }`.

### 10.3 Lifted presenter signatures (R8 §11; rename `presenter` → `writer`, `Deck` → `Document`)
`render::paths` → `html::paths`, `render::styles` → `html::styles` (+ `features`), `render::html` → `html::document`,
`render::context` → `html::context`, `render::mod` → `html::mod`; `theme::{resolve, templates, recording_reads}`,
`assets::emit::*` and `error::template_error` keep their paths (`theme::styles` is not lifted, §5).
`script_safe_json(json) = json.replace('<', "\\u003c")`. minijinja stays 2.24 (`loader`, `json`, `loop_controls`,
`custom_syntax`; PLAN §3.3).

## Open points (a task may decide locally)
- T13: `String` vs `fmt::Write`, how footnote bodies are collected (output fixed by §1.11).
- T14 (decided): cache `content_version` per `(path, len, mtime)` between serve rebuilds (§6.1).
- T15: message wording; whether `context_json` and `render` share one builder. T19: entry file names and split, token
  values, colours, font stacks.

## Deviations from PLAN.md and the brief
- **Contract change, files and dependencies:** PLAN §3.2's line becomes
  `src/html/{emit,highlight}.rs: T13   {mod,context,document,paths,styles}.rs: T15   math.rs: T1A`
  (`paths.rs`/`styles.rs` keep presenter's `render/paths.rs`/`render/styles.rs` file-for-file; T00 creates the stubs and
  `mod` lines), and T03 joins T15's dependencies (`crate::typst::html::EQUATION_CSS`; waves unchanged). The HTML scanner
  goes into `src/assets/mod.rs`, `content_version` into `src/assets/emit.rs` (both T14). Dependency lines also gain
  T14 for T12 (the HTML scanner) and T13 for T18 (the `ansi` arm in `html/emit.rs`, `html::emit` snapshots), and T60
  gains T13 (`html::highlight`, below); waves unchanged (T13 and T14 are in wave 2).
- **Math hook:** T13 (wave 2) cannot call T1A's `html/math.rs`, so §1.8 defines `MathHook` with `KatexOnly`;
  `RenderInput.math` carries it. T1A's dependency line becomes T03, T15, T16, T30 (T30 from math.md §3.6); T1A moves to
  wave 5, which supersedes math.md's "its wave stays 4"; T42 stays in wave 6. T16 constructs `KatexOnly` in
  `src/pipeline/html.rs`, and T1A replaces that one line with `html::math::MathRenderer::new(&config, world)` (§1.8;
  `world` is the pipeline's `&mut WriterWorld`, live-reload.md §5.2; additive, named in its PR). `math_init()` is T15's:
  it needs only `Emitted.math`, vendored paths, `EQUATION_CSS`.
- **`math.js` placeholder:** `math_init()` writes `script("writer/math.js")` whenever `math.katex`, and a missing helper
  path is `Error::Template` (§4.2), but `assets/writer/math.js` is T1A's (wave 5) while T15 (wave 3) and T16 (wave 4)
  render KaTeX pages. Contract change: T00 creates `assets/writer/math.js` as a one-line placeholder
  (`/* writer math.js: replaced by T1A (math.md §4.2) */`); T1A replaces its content. T16's `math.md` bound (§6.3)
  already allows the real file (≤ 2 KB).
- **Localisation before T17:** T13 (wave 2, depends on T01 only) cannot call T17's code (same wave). Contract change:
  `LangTag`, `Term`, `term`, `supplement`, `caption_separator` (references.md §4.1/§4.5, signatures unchanged, with
  their unit tests) and `assets/l10n/en.toml` move to T01 in a new file `src/resolve/l10n.rs`; T01 also writes
  `pub mod l10n; pub use l10n::{LangTag, Term, term, supplement, caption_separator};` into `src/resolve/mod.rs` (stub:
  T01 → T17, like `markdown/mod.rs`), so `resolve::*` paths hold. T01 writes them complete; T17 consumes them. Waves
  unchanged.
- **Math assets in standalone files:** KaTeX is inlined iff `Emitted.math.katex`; Latin Modern Math iff
  (`math.mathml` or `math.typst_mathml`) and `mathml_font = "embed"`, which includes typst math under the default
  `html = "katex"` (typst equations always render as MathML, math.md §4.5). This supersedes vendored-assets.md §2.1's
  "only when `features.math` and `html = "katex"`" and "only for `html = "mathml"`".
- **Theme `url()`s are strict:** an unresolvable `url()` in a theme entry or helper stylesheet is `Error::Style`
  (§5, lifted). vendored-assets.md §2.1's "everywhere else an unresolvable URL is `missing-asset`" applies only to
  document assets (ir.md §3); vendor CSS stays lenient.
- **One highlighter:** docx-output.md's coloured runs and latex-output.md §6's `\Wr*` tokens use
  `html::highlight::{SYNTAXES, resolve_syntax}` (aliases and plain rules included; `Unknown` = `Plain`), not
  `find_syntax_by_token(lang)` directly, so `console` or `jsx` highlight alike in every output.
- **`CodeAttrs` before T11:** T13 (wave 2) reads code attributes through `markdown::code` (T11, same wave). Contract
  change: `src/markdown/code.rs` moves to T01 (wave 1, before both; no wave changes), which writes it complete; T11 adds
  the `invalid-code-attr` check there (additive). Every emitter uses:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]   // `from_attrs` ignores invalid values; `lines=` sets `numbered`, `|` read as `,`
  pub struct CodeAttrs { pub numbered: bool, pub plain: bool, pub start: u64 /* ≥ 1, default 1 */,
                         pub highlight: Vec<std::ops::RangeInclusive<u64>> /* line numbers as displayed */ }
  impl CodeAttrs { pub fn from_attrs(attrs: &Attrs) -> CodeAttrs; pub fn is_highlighted(&self, line: u64) -> bool; }
  ```
  `CodeAttrs` carries no language: the brief's `lang` is `CodeBlock.lang` (for a cell, `Cell.language`, §1.9), which
  every emitter reads directly. latex-output.md §4.7's "gives the language" must read "the language from
  `CodeBlock.lang`; `.plain`, `.numbered`, `start` from `CodeAttrs::from_attrs(&attrs)`" (coordinator).
- **ANSI outputs:** `notebook::ansi` is T18's (wave 4, after T13), so T18 adds the HTML branch (§1.5;
  notebook-mapping.md §5 "Ownership").
- **Code theme CSS only when `features.code`** (still "appended to the theme stylesheet", per document), so "+16 KB per
  code theme" is an addition to the base budget.
- **No `markdown` filter:** it needs T12 (same wave as T15); the emitter renders the title block, so templates need no
  Markdown, and titles stay plain text.
- **Relies on references.md** for two shapes: TOC-listed headings carry `attrs.id == TocEntry.anchor`, and note-class
  citations arrive already wrapped in footnotes (ir.md's `ResolvedCite.note` comment states the effect; resolve owns
  footnote numbering).
- **Localized defaults** (references.md §4.1, §4.5: `term`, `supplement`, `caption_separator` for `lang`) for the
  callout, TOC, abstract and bibliography titles and caption labels; pandoc's `toc-title`, `abstract-title`,
  `reference-section-title` in `meta.vars` override them (the `refs` TOC entry follows the heading, §1.10, though
  references.md §4.8 builds it from the term). Unlike pandoc, the bibliography heading is always written (references.md
  lists it in the TOC).
