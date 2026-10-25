# R10 — Jupyter notebooks as document input (saved outputs, no execution)

**Status:** research notes, 2026-10-24. Everything marked **[verified]** was read in source, docs
or produced by a scratch program on this machine; **[unverified]** is stated from memory and must
be checked by whoever implements it. Scratch programs and their outputs live under
`/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/`
(see §13; that directory is session-scoped and may be gone later — the relevant outputs are
quoted here).

Machine facts used: macOS, Rust 1.99.1 (Homebrew cargo), pandoc 3.12.1, TeX Live
(pdflatex/xelatex/lualatex at /opt/homebrew/bin), Inkscape and ImageMagick (`magick`) happen to be
installed at /opt/homebrew/bin, **no** typst CLI, **no** rsvg-convert. A venv with pandas 3.1.0,
jinja2 3.1.6, nbformat 5.11.1, IPython 9.17.1 was created for the samples.

---

## 1. What presenter_rs already has (reuse map) [verified by reading the sources]

All paths relative to `/Users/johannes.blaschke/Developer/presenter_rs/`.

| File | Reusable as-is for writer | Notes |
|---|---|---|
| `src/notebook/nbformat.rs` (744 lines) | **Whole data model + lenient parser.** `Notebook`, `Cell`, `CellType`, `Output`, `MimeBundle`, `MultilineString`, `mime_text`, `parse_notebook`, `parse_outputs`, `Cell::tags/is_hidden_source/is_hidden_outputs/raw_mimetype/span`, `Notebook::language/title` | Drop `SlideType`/`slide_type()`; rename the `presenter` metadata accessors to `writer`. Diagnostic codes in `nbformat::codes`. |
| `src/notebook/outputs.rs` (1631 lines) | Stream merging + `apply_carriage_returns` (nbconvert CR rule), `candidates`/`choose_mime` (priority list), `image_html`'s base64 decode (whitespace-stripped, `STANDARD_PAD_INDIFFERENT`), `dimension(metadata, mime, key)` (per-MIME then top-level `width`/`height`), `svg_html`/`set_root_svg_attrs`, `strip_math_delimiters`, the **ANSI tokenizer** `sanitize_ansi`/`escape_sequence`/`skip_bytes`/`control_string_end`/`normalize_sgr`/`colon_sgr`, the `OutputFiles` "only files inside the notebook directory" rule, diagnostic codes `output-script`, `output-unsupported`, `output-invalid`, `output-widget`, `output-file-outside` | The renderer itself produces HTML strings (`CellOutput::Display { mime, html, execution_count }`); writer replaces that with IR blocks (§9) but keeps the helpers. |
| `src/notebook/convert.rs` (2520 lines) | Front matter of the **first markdown cell** (`find_front_matter` → `split_front_matter`), metadata layering (`deck_meta`: `metadata.title` → `metadata.presenter` → first-cell `---` block, later wins), `front-matter-ignored` for `---` blocks in later cells, `front-matter-empty`, tag handling (`has_any(tags, cfg.hide_input_tags)` …), `attachment:` URL resolution, raw-cell rule | Slide structure (slide types, fragments, notes, `split_markdown_cells`) is not needed. |
| `src/assets/registry.rs` | `AssetRegistry::register_bytes(bytes: Vec<u8>, mime: &str, file_name_hint: &str) -> String` (id = first 12 hex of blake3(bytes)), `AssetData::{File, Bytes}`, `Asset { id, mime, data, file_name_hint }`, `AssetRegistry::placeholder(id) -> "asset:<id>"`, `merge` | Content-addressed, so the same PNG in two cells registers once. |
| `src/assets/emit.rs` | `hashed_file_name(hint: &str, id: &str) -> String` → `plot-3f9a1c2b7e0d.png` (sanitises the hint, keeps the extension) | writer's notebook image names (§8) can be built with the same sanitiser. |
| `src/markdown/render.rs` | `RenderMode::Notebook`: parses cells **without** the front-matter extension (`parse_opts.extension.front_matter_delimiter = None`), drops `:::include` with an `include-in-notebook` warning | writer may want to *allow* `:::include` inside notebook markdown cells (open question §12). |
| `src/config.rs` `NotebookConfig` | `mime_priority`, `hide_input_tags = ["hide-input","remove-input"]`, `hide_output_tags = ["hide-output","remove-output"]`, `remove_cell_tags = ["remove-cell","hide-cell","skip"]`, `show_prompts` | |
| `docs/spec/notebook-mapping.md` | §1 model, §3 tags table, §4 MIME table, §5 metadata precedence | Contract text to adapt. |

Key presenter signatures (copied):

```rust
// src/notebook/nbformat.rs
pub fn parse_notebook(path: &Path, json: &str) -> crate::error::Result<(Notebook, Diagnostics)>;
pub fn parse_outputs(raw: Vec<Value>) -> (Vec<Output>, Vec<String>);
pub fn mime_text(value: &Value) -> Option<String>;          // string or list-of-strings → String
pub type MimeBundle = BTreeMap<String, Value>;
pub struct MultilineString(pub String);                      // "text" | ["li","nes"] | null
#[serde(tag = "output_type", rename_all = "snake_case")]
pub enum Output {
    Stream { name: String, text: MultilineString },
    DisplayData { data: MimeBundle, #[serde(default)] metadata: Value },
    ExecuteResult { data: MimeBundle, #[serde(default)] metadata: Value, execution_count: Option<i64> },
    Error { ename: String, evalue: String, traceback: Vec<String> },
}
impl Cell {
    pub fn tags(&self) -> Vec<String>;                        // metadata.tags, strings only
    pub fn is_hidden_source(&self) -> bool;                   // metadata.jupyter.source_hidden == true
    pub fn is_hidden_outputs(&self) -> bool;                  // metadata.jupyter.outputs_hidden == true
    pub fn raw_mimetype(&self) -> Option<&str>;               // metadata.raw_mimetype, else metadata.format
    pub fn span(&self, index: usize) -> SourceSpan;           // lines 1..=max(1, source lines), cell: Some(index)
}
impl Notebook {
    pub fn language(&self) -> &str;   // language_info.name, else kernelspec.language, else "text"
    pub fn title(&self) -> Option<&str>;
}
// src/notebook/outputs.rs
fn apply_carriage_returns(text: &str) -> Cow<'_, str>;       // nbconvert regex `.*\r(?=[^\n])` per line
fn strip_math_delimiters(tex: &str) -> &str;                 // one $$…$$ | \[…\] | \(…\) | $…$ pair
fn dimension(metadata: &Value, mime: &str, key: &str) -> Option<String>;
fn escape_sequence(text: &str, start: usize) -> (usize, Option<&str>); // (end, SGR params if `ESC [ … m`)
pub fn ansi_to_html(text: &str) -> String;                   // HTML only (ansi-to-html crate)
// src/assets/registry.rs
pub fn register_bytes(&mut self, bytes: Vec<u8>, mime: &str, file_name_hint: &str) -> String;
pub fn placeholder(id: &str) -> String;                      // "asset:<id>"
// src/assets/emit.rs
pub fn hashed_file_name(hint: &str, id: &str) -> String;
// src/markdown/frontmatter.rs
pub fn split_front_matter(text: &str) -> Option<(&str /*yaml*/, &str /*rest*/, usize /*lines consumed*/)>;
```

Presenter's real executed notebook `examples/notebook/analysis.ipynb` **[verified]** has outputs
with bundles `['image/png','text/plain']` (cell 4, PNG is 27 733 bytes, 807×243 px, metadata `{}`),
`['text/html','text/plain']` (cell 7, a DataFrame), `['text/markdown','text/plain']` (cell 11) and
an `error` output (cell 13) — a useful fixture for writer too.

---

## 2. nbformat facts [verified]

Source: nbformat v4.5 JSON schema (`nbformat/v4/nbformat.v4.5.schema.json`) and the
format description page (https://nbformat.readthedocs.io/en/latest/format_description.html).

- Top-level: `cells`, `metadata`, `nbformat` (4), `nbformat_minor`. Multi-line fields "may be
  written as a list of strings, which should be joined with `''`" (presenter's `MultilineString`).
- **Cell ids** (4.5+): `id` string, length 1–64, `[A-Za-z0-9_-]`, unique per notebook.
- **Cell metadata** (schema): `tags` ("must be unique, and must not contain commas"), `name`
  (non-empty, unique), `collapsed`, `scrolled` (`true|false|"auto"`), `jupyter.source_hidden`,
  `jupyter.outputs_hidden`, `execution` (timing).
- **Markdown/raw cell attachments**: `attachments` = `{ "<filename>": { "<mime>": "<base64>" } }`
  ("Media attachments (e.g. inline images), stored as mimebundle keyed by filename"); referenced
  from the source as `attachment:<filename>`.
- **Raw cell**: the schema defines `metadata.format` — "Raw cell metadata format for nbconvert"
  (the format-description page: "The only logic in a raw cell is the `format` metadata field. If
  defined, it specifies which nbconvert output format is the intended target"). **The schema has no
  `raw_mimetype` key**, but nbconvert reads `raw_mimetype` (see §6). Read `raw_mimetype` first,
  then `format` (presenter does exactly that).
- **Outputs**: `stream { name: "stdout"|"stderr", text }`; `display_data { data, metadata }`;
  `execute_result { data, metadata, execution_count: int }`; `error { ename, evalue, traceback:
  [str] }`. `mimebundle` values are "either an array of strings or a string"; keys matching
  `^application/(.*\+)?json$` "can be any type" (kept as JSON). `output_metadata` is keyed by
  MIME type, e.g. `"image/png": {"width": 640, "height": 480}`; `isolated` ("Whether the output
  should be isolated into an IFrame") exists but is irrelevant to static documents.
- **Notebook metadata**: `kernelspec { name, display_name }`, `language_info { name, … }`
  (name required when present), `authors` ("The author(s) of the notebook document", items with
  `name`), `title` ("The title of the notebook document").

---

## 3. Per-MIME rendering rules for a structured renderer

### 3.1 MIME priority per target (what the reference tools do) [verified]

| Tool | Priority list |
|---|---|
| nbconvert base (`NbConvertBase.display_data_priority`) | `text/html, application/pdf, text/latex, image/svg+xml, image/png, image/jpeg, text/markdown, text/plain` |
| nbconvert LaTeX exporter (`default_config`) | `text/latex, application/pdf, image/png, image/jpeg, image/svg+xml, text/markdown, text/plain` (note: **no text/html**) |
| Quarto `displayDataMimeType` | base `[text/markdown, image/svg+xml, image/png, image/jpeg]`; for HTML targets `widget-state, widget-view, application/javascript, text/html` are put first; for LaTeX `text/latex, application/pdf` are put first; `text/html` is appended when the output "has an HTML table"; `text/plain` always last |
| presenter `[notebook] mime_priority` | `text/html, image/svg+xml, image/png, image/jpeg, image/gif, image/webp, text/markdown, text/latex, application/json, text/plain` |

**Recommendation:** one priority list per target, defaults:

```
html : text/html, text/markdown, image/svg+xml, image/png, image/jpeg, image/gif, image/webp, text/latex, application/json, text/plain
docx : text/markdown, image/png, image/jpeg, image/svg+xml, image/gif, image/webp, text/html(table only), text/latex, application/json, text/plain
typst: text/markdown, image/svg+xml, image/png, image/jpeg, image/gif, image/webp, application/pdf, text/html(table only), text/latex, application/json, text/plain
latex: text/latex, application/pdf, image/png, image/jpeg, image/svg+xml, text/markdown, text/html(table only), application/json, text/plain
```

"`text/html(table only)`" = the entry is taken only when the table parser (§4) accepts the
payload; otherwise the next candidate is tried (this is presenter's `try_mime` → `None` → next
candidate mechanism, reused). Since writer parses outputs **once into one IR**, the simplest
implementation is: parse the bundle with the *union* of candidates into an `Output::Rich`
holding every renderable alternative (table IR, figure, markdown blocks, raw html, verbatim text),
and let each backend pick (§9). That avoids re-parsing the notebook per target in `serve` mode.

### 3.2 Rules per MIME type

| MIME | IR | HTML | DOCX | typst (PDF) | LaTeX |
|---|---|---|---|---|---|
| `image/png`, `image/jpeg`, `image/gif`, `image/webp` | `Figure { image: ImageRef, caption, label }`; bytes registered as an asset; size: `metadata[mime].width/height` → else `metadata.width/height` → else intrinsic px from `imagesize::blob_size` (§8) | `<figure><img src=asset:…>` | picture run; GIF/WebP **[unverified: Word accepts GIF, not WebP]** → convert to PNG (`image` crate 0.25.10) or warn | `image("<file>")` — typst accepts png, jpg, gif, svg, pdf, webp [verified: typst docs] | `\includegraphics{<file>}`; GIF/WebP not loadable by pdf/xe/lualatex **[unverified]** → convert to PNG |
| `image/svg+xml` | `Figure` with `ImageRef { mime: svg }` + lazily derived PNG and PDF renditions | inline `<svg>` (presenter's `svg_html`, root `width`/`height` from metadata) | PNG rendition via **resvg 0.48.1** (verified §8) — native SVG in DOCX is an open question | write the `.svg` file; `image()` handles it natively | PDF rendition via **svg2pdf 0.13.0** (verified §8; this is also what nbconvert's `SVG2PDFPreprocessor` does with Inkscape: `from_format = "image/svg+xml"`, `to_format = "application/pdf"`) |
| `application/pdf` (plotly `PdfRenderer`, matplotlib `set_matplotlib_formats('pdf')`) | `Figure` with `ImageRef { mime: pdf }` | not displayable; fall through to next MIME | fall through (rasterise would need a PDF renderer — not worth it) | `image()` accepts PDF (typst docs: "any PDF image file used must have a version equal to or lower than the export target PDF version") | `\includegraphics` of the written `.pdf` |
| `text/html` | **try the table parser (§4)** → `Table { caption, head, body, foot, … }`; else `RawHtml` | table → `<table>` from IR, or the payload verbatim (warn `output-script` on `<script`) | table → `w:tbl`; else next MIME (usually `text/plain`) + warning `output-html-fallback` | table → `table(...)`; else next MIME + warning | table → `tabular` (BYO template may define a macro); else next MIME + warning |
| `text/markdown` | parse with writer's own dialect in "cell mode" (no front matter, relative paths against the notebook dir, only files inside it) → `Vec<Block>` | normal | normal | normal | normal |
| `text/latex` | `strip_math_delimiters` → `DisplayMath { tex }` (`$\displaystyle …$` is what SymPy emits; presenter's unit tests cover it) ; a payload that is not a single formula (contains `\begin{tabular}`, `\section`, or several `$…$`) → `RawLatex` | KaTeX display math | OMML via the math pipeline (own research topic) | typst math via a TeX→typst converter (open question: `mitex`) or fall back to the `text/plain` candidate with a warning | `\[ … \]` or the raw block verbatim |
| `application/json` (and `*+json` that nothing else claims) | `CodeBlock { lang: "json", text: serde_json::to_string_pretty }` | `<pre class="output-json">` | code style | `raw(block: true, lang: "json")` | `verbatim`/listings |
| `text/plain` | `Verbatim { text: String, spans: Vec<StyledSpan>, kind: PlainOutput }` — ANSI parsed into spans (§7) | `<pre>` with `<span>`s | runs with colour/bold; or stripped text (config) | `raw(block: true)` of the stripped text | `verbatim` of the stripped text (or nbconvert-style `\textcolor` if the template asks) |
| `application/vnd.plotly.v1+json`, `application/vnd.jupyter.widget-view+json`, `application/vnd.vega*.v*+json`, `application/javascript`, `text/vnd.plotly.v1+html` | not rendered; take the next candidate (plotly's `NotebookRenderer` also emits `text/html` — a `<div>` + `<script>` that needs plotly.js at runtime [verified: plotly `_base_renderers.py` returns `{"text/html": html}` for HtmlRenderer/NotebookRenderer and `{"application/vnd.plotly.v1+json": …}` for PlotlyRenderer]) | HTML target may keep the `text/html` (interactive, warn `output-script`) | `text/plain` fallback + warning `output-widget` / `output-plotly` | same | same |
| `stream` | `Verbatim { kind: Stream(stdout|stderr), spans }` after merging consecutive same-name streams and applying the CR rule | `<pre class="output-stream stderr">` + spans | runs; stderr in a distinguishable style | `raw(block: true)` stripped | `verbatim` stripped |
| `error` | `Verbatim { kind: Error { ename, evalue }, spans }` from `traceback.join("\n")` | spans | stripped or coloured runs | stripped | stripped |

Widgets: bundles with `application/vnd.jupyter.widget-view+json` carry a `text/plain` repr in
practice (presenter's `WIDGET_VIEW` handling); keep presenter's `output-widget` warning.

---

## 4. `text/html` tables: what pandas emits, and the parser contract

### 4.1 pandas 3.1.0 output, generated locally [verified]

`DataFrame.to_html()` (what `execute_result["text/html"]` contains is `_repr_html_()`, which is
`to_html()` wrapped in `<div>\n<style scoped>…</style>\n…\n</div>`; the `<style scoped>` block
holds three `.dataframe` rules):

```html
<table border="1" class="dataframe">
  <thead>
    <tr style="text-align: right;">
      <th></th>
      <th>name</th>
      <th>value</th>
      <th>n</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <th>0</th>
      <td>a</td>
      <td>1.50</td>
      <td>3</td>
    </tr>
    <tr>
      <th>1</th>
      <td>b&lt;c</td>
      <td>2.25</td>
      <td>4</td>
    </tr>
  </tbody>
</table>
```

Observed details:
- Cell text is HTML-escaped (`b&lt;c`, `&amp;amp; &lt;b&gt;` for a cell containing `&amp; <b>`).
- Row index cells are `<th>` **inside `<tbody>`**; the header row starts with an empty `<th></th>`
  for the index column. With `index=False` neither exists (header has no leading `<th>`, body rows
  have only `<td>`).
- A named index adds a **second `<thead>` row**: `<tr><th>key</th><th></th></tr>`.
- **MultiIndex columns**: one `<thead>` row per level, spanning header cells written as
  `<th colspan="2" halign="left">A</th>`; then an index-names row (`<th>R</th><th>S</th><th></th>…`).
  **MultiIndex rows**: `<th rowspan="2" valign="top">r1</th>` in `<tbody>`, and the spanned rows
  have one `<th>` fewer. (`halign`/`valign` are non-standard attributes; ignore them.)
- Missing values: `NaN`, `NaT`; `None` in an object column prints `NaN`. Floats are formatted per
  column (`1.50`, `2.25`). A newline inside a cell is written as the two characters `\n`
  (`<td>multi\nline</td>`).
- **Truncation** (`display.max_rows` / `max_columns`): a `<tr><th>...</th><td>...</td>…</tr>` row
  and/or a `<th>...</th>`/`<td>...</td>` column, plus `<p>100 rows × 2 columns</p>` **after** the
  table inside the wrapping `<div>`.
- `Series` has **no** `_repr_html_` (only `text/plain`).
- **Styler** (`df.style…`, `Styler._repr_html_() == Styler.to_html()`):

```html
<style type="text/css">
#T_69ef7 th {
  font-weight: bold;
}
#T_69ef7_row1_col1 {
  background-color: yellow;
}
</style>
<table id="T_69ef7">
  <caption>Styled caption</caption>
  <thead>
    <tr>
      <th class="blank level0" >&nbsp;</th>
      <th id="T_69ef7_level0_col0" class="col_heading level0 col0" >name</th>
      …
  <tbody>
    <tr>
      <th id="T_69ef7_level0_row0" class="row_heading level0 row0" >0</th>
      <td id="T_69ef7_row0_col0" class="data row0 col0" >a</td>
```

  Styler **does not escape cell text** (`<td …>b<c</td>` was emitted verbatim); the html5ever
  parse then reports `"Saw t in state SelfClosingStartTag"`, `"expected to close <td> or <th>
  with cell"` and the cell text comes out as `"b"` (data loss, but that is also what a browser
  shows). Styler ids change on every render (`T_<5 hex>`), so they must not become IR ids.

### 4.2 Parser contract (recommendation)

Accept a `text/html` payload as a table when **all** hold:
1. After html5ever fragment parsing, the significant top-level elements are exactly one `<table>`,
   optionally wrapped in one `<div>`, with any number of `<style>` siblings (pandas) and at most
   one trailing `<p>` whose text matches `^\d+ rows × \d+ columns$` (kept as a `note` on the IR
   table). Any other top-level element (`<h3>`, `<script>`, `<form>`, `<img>`, a second table …) →
   not a table.
2. The table contains no nested `<table>`.
3. Every cell's content is text plus elements from an allowlist: `br` (→ `\n`), `p`/`div` (→
   trailing `\n`), inline `b strong i em code a span sub sup`. Anything else (`img`, `svg`,
   `select`, `button`, `script` …) → not a table.
4. At least one row.

Mapping: `<caption>` → caption; `<thead>` rows → header rows; `<tbody>`/bare `<tr>` → body rows
(html5ever inserts the implied `<tbody>` for bare `<tr>` — verified); `<tfoot>` → foot rows;
cell → `{ text, header: tag == "th", colspan, rowspan }` with spans parsed as `u32 ≥ 1`
(default 1); `&nbsp;` arrives decoded as U+00A0 (map to a space or empty for the Styler
`blank` cell). Keep inline bold/italic/code as inline marks only if the IR table cell type can
hold inlines; otherwise flatten to text (the probe flattens). Attributes `style`, `class`, `id`,
`halign`, `valign` are dropped. A `<thead>` containing `<td>` still counts as header rows (the
probe shows this parses fine).

Fallback: HTML target keeps the payload as `RawHtml`; other targets take the next MIME candidate
(`text/plain`, which for a DataFrame is the monospace repr) and record a warning
`output-html-fallback` ("output 1: text/html is not a plain table (found <script>); rendered as
text/plain").

### 4.3 scraper 0.29.0 probe [verified]

- `scraper` 0.29.0, ISC, edition 2024, no declared `rust-version`; dependencies `html5ever 0.40.0`,
  `ego-tree 0.11.0`, `selectors 0.41.0`, `cssparser 0.38.0` (+ `getopts` only for the `main`
  feature — use `default-features = false, features = ["errors"]`? note: `default = ["main",
  "errors"]`; `main` pulls `getopts` for the example binary). `cargo tree` of the probe: **50
  crates**. html5ever latest is 0.40.1 (MIT OR Apache-2.0, rust-version 1.85).
- Public API used (copied from source):

```rust
pub fn Html::parse_fragment(fragment: &str) -> Html;            // body context; root_element() is a synthetic <html>
pub fn Html::parse_document(document: &str) -> Html;
pub struct Html { pub errors: Vec<Cow<'static, str>>, /* tree */ }
pub fn Html::select<'a,'b>(&'a self, selector: &'b Selector) -> Select<'a,'b>; // Iterator<Item = ElementRef<'a>>
pub fn Html::root_element(&self) -> ElementRef<'_>;
pub fn Selector::parse(selectors: &str) -> Result<Selector, SelectorErrorKind>;
pub fn ElementRef::wrap(node: NodeRef<'a, Node>) -> Option<ElementRef<'a>>;
pub fn ElementRef::value(&self) -> &'a Element;
pub fn ElementRef::attr(&self, attr: &str) -> Option<&'a str>;
pub fn ElementRef::text(&self) -> Text<'a>;                      // Iterator<Item = &'a str> over descendant text nodes
pub fn ElementRef::html(&self) -> String;  pub fn inner_html(&self) -> String;
pub fn ElementRef::child_elements(&self) -> impl Iterator<Item = ElementRef<'a>>;
impl Deref for ElementRef<'a> { type Target = NodeRef<'a, Node>; }  // so .children(), .parent(), .value() of ego_tree
pub fn Element::name(&self) -> &str;  pub fn Element::classes(&self) -> Classes<'_>;  pub fn Element::attr(&self, &str) -> Option<&str>;
pub enum Node { Document, Fragment, Doctype(_), Comment(_), Text(Text), Element(Element), ProcessingInstruction(_) }
// scraper::node::Text derefs to str (`&**text`), as the Text iterator does.
```

- Probe output on the samples (`nb-probe`, source in §13): simple → head `[H:"", H:"name",
  H:"value", H:"n"]`, body `[H:"0", "a", "1.50", "3"]`…; `_repr_html_` variant → top-level
  `["div"]`, same table; MultiIndex → head rows `[H:"", H:"grp", H:"A"(2x1), H:"B"]`,
  `[H:"", H:"sub", H:"x", H:"y", H:"x"]`, `[H:"R", H:"S", H:"", H:"", H:""]`, body
  `[H:"r1"(1x2), H:"i", "0","1","2"]`, `[H:"ii","3","4","5"]`, `[H:"r2",H:"i","6","7","8"]`;
  Styler → `id="T_69ef7"`, caption `"Styled caption"`, header `[H:"\u{a0}", …]`, two parse
  errors (see 4.1); truncated → a `...` row and `<p>: "100 rows × 2 columns"`; messy table
  (`<tr>` without `<tbody>`, `<br>`, `&nbsp;`, `<b>/<i>/<a>/<code>`, `rowspan="2"`,
  `colspan="2"`, `<p>` inside a cell) → `[H:"A\nB", "x\u{a0}y"]`, `["bold and it\nsecond",
  "r"(1x2)]`, `["para1\npara2"]`, `["link & code"(2x1)]`; nested table → detected via
  `table.select(&table_sel).next().is_some()`; `<h3>…<table>…<script>` → top-level
  `["h3","table","script"]` (reject by rule 1); `<thead><tr><td>` → one head row with
  `header=false` cells.

### 4.4 Alternatives [verified]

- **lol_html 3.0.2** (presenter dep, BSD-3-Clause, rust-version 1.85) describes itself as "a
  Low Output Latency streaming HTML rewriter/parser with CSS-selector based API … two main API
  entry points: `HtmlRewriter` — a streaming HTML rewriter; `rewrite_str` — one-off HTML string
  rewriting". It has element/text handlers but **no tree**: you cannot ask a `<td>` for its
  children or walk siblings. Building a table out of it means a hand-written state machine over
  `element!("table td, table th", …)` + `text!` handlers with start/end-tag bookkeeping — doable
  for pandas' flat tables (one text handler per cell, `colspan`/`rowspan` via `el.get_attribute`)
  but nesting detection and the allowlist of rule 3 become awkward. Not recommended as the primary
  parser; fine to keep for presenter-style attribute rewriting.
- **html5ever 0.40.1 + markup5ever_rcdom**: what scraper wraps; using it directly saves the
  `selectors`/`cssparser` dependencies (~10 crates) at the cost of writing the tree walk against
  `rcdom::Handle`/`NodeData`. Only worth it if dependency count matters; the parser is the same.

---

## 5. Cell options: quarto `#|` comments and tags

### 5.1 Quarto syntax [verified from `quarto-cli/src/core/lib/partition-cell-options.ts`]

- Option lines are recognised with `new RegExp("^" + escapeRegExp(comment) + "\\s*\\| ?")`:
  the line must start at column 0 with the language's comment marker, optional whitespace, `|`,
  optional one space. Scanning is contiguous from the **first line** of the cell; the first
  non-matching line (code, blank line, a shebang) ends the block, and a blank line *before* the
  first option line means no options. For block-comment languages the trimmed line must also end
  with the closing marker, which is stripped.
- Comment markers (`kLangCommentChars`, unknown language → `"#"`): r/python/julia/bash/
  powershell/stan/octave/awk/gawk/sed/perl/prql/ruby/coffee `#`; scala/csharp/fsharp/cpp/cc/java/
  groovy/kotlin/js/d3/node/sass/scss/go/asy/dot/ojs/rust `//`; matlab/tikz `%`; sql/mysql/psql/
  lua/haskell `--`; fortran/fortran95 `!`; stata `*`; q `/`; apl `⍝`; c/css `["/*","*/"]`;
  sas `["*",";"]`; ocaml `["(*","*)"]`.
- The option lines' text after the marker is parsed as one YAML mapping; empty/whitespace-only →
  no options; the YAML reader is not wrapped in try/catch in that file (a parse error propagates;
  writer should turn it into a cell-level warning and ignore the options).
- Documented options (https://quarto.org/docs/reference/cells/cells-jupyter.html): Attributes
  `label`, `classes`, `renderings`, `tags`, `id`; Code Output `eval`, `echo` (`true|false|fenced`),
  `code-fold`, `code-summary`, `code-overflow`, `code-line-numbers`, `lst-label`, `lst-cap`; Cell
  Output `output` (`true|false|asis`), `warning`, `error`, `include` ("Catch-all for preventing
  any output (code or results)"), `panel`, `output-location`; Figures `fig-cap`, `fig-subcap`,
  `fig-link`, `fig-align` (`default|left|right|center`), `fig-alt`, `fig-env`, `fig-pos` (LaTeX,
  computational figures default `H`), `fig-scap`; Tables `tbl-cap`, `tbl-subcap`,
  `tbl-colwidths`, `html-table-processing` (`none` leaves raw HTML tables alone); Layout
  `layout`, `layout-ncol`, `layout-nrow`, `layout-align`, `layout-valign`; Page columns
  `column`, `fig-column`, `tbl-column`, `cap-location`, `fig-cap-location`, `tbl-cap-location`.
- Cross-reference labels "must start with their type (e.g. `fig-` or `tbl-`)"; `lst-label` must
  begin with `lst-`; headings use `#sec-`, equations `#eq-`, theorems `#thm-`.
- **Multiple outputs in one cell** (`src/core/jupyter/jupyter.ts` + `labels.ts`):
  `const outputLabel = label && labelCellContainer && isDisplayData(output) ? label + "-" +
  nextOutputSuffix++ : label;` — the counter advances only for display-data outputs, starting at
  1. `shouldLabelCellContainer` is true when the cell has no (included) outputs, no display-data
  outputs, **more than one display-data output and `fig-cap` is not an array**, or a `tbl-`
  label. So: `label: fig-plots` + two figures → container `fig-plots`, figures `fig-plots-1`,
  `fig-plots-2` (the cross-references page confirms `@fig-plots-1`/`@fig-plots-2` with
  `fig-subcap`); one figure → the figure itself gets `fig-plots`. Captions (`resolveCaptions`):
  `fig-cap` as a list → one caption per captionable output (no cell caption); `fig-cap` scalar +
  `fig-subcap` → cell caption + sub-captions (`fig-subcap: true` → `[""]`); `fig-cap` scalar
  alone → `outputCaptions = [fig-cap]`; each captionable display output takes the next caption
  (`outputCaptions.shift()`).

### 5.2 Tags [verified]

- Jupyter Book (https://jupyter-book.readthedocs.io/v1/interactive/hiding.html): `hide-input`
  (source hidden, outputs shown, reveal button), `hide-output`, `hide-cell`, `remove-input`,
  `remove-output`, `remove-cell` ("Hiding content provides a button that lets readers reveal the
  content. Removing content prevents it from making it into your book."). Cells containing only
  whitespace are removed automatically.
- nbconvert `TagRemovePreprocessor` traits: `remove_cell_tags`, `remove_all_outputs_tags`,
  `remove_input_tags` (also `remove_single_output_tags`, not on that page) — **no default tag
  names**; the docs' examples use `remove_cell`, `remove_output`, `remove_input` (underscores).
- nbformat metadata: `jupyter.source_hidden`, `jupyter.outputs_hidden` (what JupyterLab's
  collapse buttons write).
- presenter defaults: `hide_input_tags = ["hide-input","remove-input"]`, `hide_output_tags =
  ["hide-output","remove-output"]`, `remove_cell_tags = ["remove-cell","hide-cell","skip"]`.

### 5.3 Recommendation: support both, with this precedence

`#|` options (closest to the code) > per-cell `metadata.writer` object (same keys, for people who
refuse comments in code) > tags / `jupyter.*_hidden` > notebook-level defaults (`metadata.writer`
/ front matter `notebook:` table) > config file.

Mapping to `CellOptions` (§9):

| Source | `CellOptions` field |
|---|---|
| `label: fig-x` / `tbl-x` / `lst-x` | `label: Some("fig-x")`; the prefix decides which output kind gets it (figure/table/listing) |
| `fig-cap`, `fig-subcap`, `fig-alt`, `fig-align`, `fig-pos`, `fig-scap`, `fig-env` | `fig_cap: Caption::{Single(String) | PerOutput(Vec<String>)}`, `fig_subcap`, `fig_alt`, `fig_align`, `fig_pos` (LaTeX only), `fig_scap`, `fig_env` |
| `tbl-cap`, `tbl-subcap`, `tbl-colwidths` | `tbl_cap`, `tbl_subcap`, `tbl_colwidths` |
| `lst-label`, `lst-cap` | `lst_label`, `lst_cap` (caption of the **input** code listing) |
| `echo: false` / tag `hide-input`, `remove-input` / `jupyter.source_hidden` | `echo = false` (input not emitted; `hide-*` and `remove-*` are the same thing in a static document — no reveal button) |
| `output: false` / tag `hide-output`, `remove-output` / `jupyter.outputs_hidden` | `output = false` |
| `output: asis` | `output = Asis`: `text/plain`/stream text is parsed as Markdown of the dialect (quarto semantics: "raw markdown output … none of Quarto's standard enclosing divs") |
| `include: false` / tags `remove-cell`, `hide-cell`, `skip` | cell omitted entirely |
| `warning: false` | drop `stderr` streams |
| `error: false` | drop `error` outputs (quarto: `error: true` *shows* errors) |
| `eval`, `cache`, `freeze` | ignored (writer never executes); `eval: false` with saved outputs is contradictory → warning |
| `classes`, `layout*`, `column*`, `cap-location`, `panel`, `code-fold`, `code-summary`, `code-line-numbers`, `code-overflow` | `classes` kept as attrs for HTML; the rest ignored with one `cell-option-unsupported` warning naming them |
| `tags` option | merged into the cell's tag list |

Also accept the underscore spellings (`remove_cell`, `hide_input`, …) — cheap and matches
nbconvert's documented examples.

Label assignment to outputs (quarto's rule, simplified): the cell's `label` goes to the single
captionable output (figure or table) when there is exactly one; with N > 1 captionable outputs the
outputs get `<label>-1 … <label>-N` and the cell gets a container with `<label>` (a figure group
in the IR; sub-captions from `fig-subcap`/`tbl-subcap`, main caption from `fig-cap`/`tbl-cap`);
`fig-cap` given as a list → per-output captions, no group caption. A `fig-` label on a cell whose
only captionable output is a table (or vice versa) → warning `label-kind-mismatch`, label still
applied. Numbering of figures/tables continues across markdown and notebook content of the
document.

---

## 6. Raw cells [verified]

- nbconvert's `null.j2` raw-cell block: `{%- if cell.metadata.get('raw_mimetype', '').lower() in
  resources.get('raw_mimetypes', ['']) -%}{{ cell.source }}`. `TemplateExporter.raw_mimetypes`
  defaults to `[self.output_mimetype, ""]`; the LaTeX exporter's `output_mimetype = "text/latex"`.
  So nbconvert includes a raw cell when its `raw_mimetype` equals the exporter's MIME **or is
  absent** (the empty string matches `""`).
- The nbformat schema only knows `metadata.format` for raw cells (§2); JupyterLab's "Raw NBConvert
  Format" selector writes `raw_mimetype` **[unverified: not checked in JupyterLab's source]**;
  presenter reads `raw_mimetype` then `format` (keep that).

Recommendation (`RawCell` → `Block::Raw { format: RawFormat, text }`):

| `raw_mimetype` | Emitted by |
|---|---|
| `text/html` | HTML backend verbatim (relative `src`/`href` resolved like raw HTML in Markdown); DOCX/typst/LaTeX: skipped with warning `raw-cell-skipped` (DOCX has no raw HTML; a BYO template cannot use it) |
| `text/latex` (also `text/x-latex` [unverified alias]) | LaTeX backend verbatim; others skip |
| `text/markdown` | **all** backends: parsed with the dialect (nbconvert itself would only pass it to the markdown exporter, but for a document tool this is the useful behaviour) |
| `text/restructuredtext`, `text/asciidoc`, `application/x-typst` or `text/x-typst` (a writer extension for typst passthrough), `text/plain` | typst: `*typst` only; `text/plain` → verbatim block in every backend; the rest skipped with a warning |
| absent | nbconvert includes it everywhere; writer: treat as `text/plain`? No — recommend **passthrough per target as if it were the target's own raw format** (nbconvert-compatible: an untyped raw cell with `\section{}` ends up in the LaTeX, one with `<div>` in the HTML) **plus** a warning `raw-cell-untyped` once per cell; DOCX skips it. Alternative (stricter): skip with a warning. Decide in the plan. |

Attachments of raw cells (`attachment:` URLs) resolve like markdown cells (presenter resolves
them in raw HTML cells).

---

## 7. ANSI escape sequences

### 7.1 What real tracebacks contain [verified]

IPython 9.17.1 (`AutoFormattedTB(mode="Context", theme_name="linux")`, the kernel default) —
note the `color_scheme` kwarg was renamed to `theme_name` in IPython 9; signature:
`FormattedTB.__init__(self, mode='Plain', theme_name='linux', call_pdb=False, ostream=None,
tb_offset=0, long_header=False, include_vars=False, check_cache=None, debugger_cls=None)`:

```
'\x1b[91m-----…-----\x1b[39m'
'\x1b[91mZeroDivisionError\x1b[39m                         Traceback (most recent call last)'
'\x1b[96mFile \x1b[39m\x1b[32m/path/gen_tb2.py:14\x1b[39m\n\x1b[92m     12\x1b[39m \x1b[38;5;255m    \x1b[39m\x1b[38;5;81mreturn\x1b[39m … \x1b[92m---> \x1b[39m\x1b[92m14\x1b[39m \x1b[38;5;255m    \x1b[39m\x1b[30;43mf\x1b[39;49m\x1b[30;43m(\x1b[39;49m…'
'\x1b[91mZeroDivisionError\x1b[39m: division by zero'
```

i.e. 16-colour codes `31/32/36/91/92/96/94`, resets `39`/`49` (never `0`), 256-colour
`38;5;n` (n = 81, 141, 148, 204, 255) for syntax highlighting, and `30;43` (black on yellow) for
the failing expression. IPython 8-era (presenter's `analysis.ipynb`): `\x1b[31m…\x1b[39m`,
`\x1b[36m`, `\x1b[32m` only. Neither uses bold, but libraries (rich, tqdm, pytest) do
(`1`, `22`, `4`, `24`, `38;2;r;g;b`, `48;5;n`, `7`).

### 7.2 Crate comparison, run on the samples [verified, `ansi-probe` source in §13]

| Crate | Version / licence / MSRV | API | 16-colour | `38;5;n` | `38;2;r;g;b` | bold `1` / off `22` | underline `4` / off `24` | reverse `7`/`27` | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| `cansi` | 2.2.1, MIT, no MSRV | `cansi::v3::categorise_text(text: &str) -> Vec<CategorisedSlice<'_>>` with `{ text, start, end, fg: Option<Color>, bg: Option<Color>, intensity: Option<Intensity>, italic, underline, blink, reversed, hidden, strikethrough: Option<bool> }` (`Color` = 16 named variants) | yes | **ignored** (`adjust_sgr` has arms only for 0–9, 22–29, 30–37, 40–47, 90–97, 100–107; `"38"` and `"5"`/`"241"` fall to `_ => ()`), text shown as default colour | **ignored** (worse: `38;2;10;20;30` is read as `2` → Faint and `30` → Black) | yes / yes | yes / yes | yes / yes | loses IPython 9 syntax colours; mis-parses RGB |
| `anstream::adapter::WinconBytes` | anstream 1.0.0 (anstyle-parse 1.0.0, anstyle 1.0.14), MIT OR Apache-2.0, MSRV 1.66 | `WinconBytes::new().extract_next(bytes: &[u8]) -> impl Iterator<Item = (anstyle::Style, String)>`; `Style::get_fg_color() -> Option<Color>` (`Ansi(AnsiColor)`, `Ansi256(Ansi256Color(u8))`, `Rgb(RgbColor(u8,u8,u8))`), `get_bg_color()`, `get_effects() -> Effects` (BOLD, DIMMED, ITALIC, UNDERLINE, DOUBLE_UNDERLINE, INVERT, HIDDEN, STRIKETHROUGH…) | yes | yes (`Ansi256Color(255)`) | yes | `1` yes / **`22` ignored** (`csi_dispatch` has no arms for 22, 23, 24, 27, 29 — read in `anstream-1.0.0/src/adapter/wincon.rs`), so `" bold  normal "` stayed BOLD and `under ` kept BOLD\|UNDERLINE | `4` yes / `24` ignored | `7` yes / `27` ignored | best colour model, broken "off" codes |
| `ansi-to-html` | 0.2.4, MIT OR Apache-2.0, MSRV 1.85.1 (presenter dep) | `convert(&str) -> Result<String, Error>`, `Converter`/`convert_with_opts`; parser is `mod ansi` (**private**) | yes (`var(--red,#a00)`) | yes (hex `#626262`) | yes | yes/yes (`<b> bold </b> normal`) | yes/yes (`<u>`) | yes | HTML only; correct; presenter wraps it with `sanitize_ansi` to pre-clean |
| `strip-ansi-escapes` | 0.2.1, Apache-2.0/MIT, depends on `vte 0.14` | `strip_str<T: AsRef<str>>(data: T) -> String`, `strip<T: AsRef<[u8]>>(data: T) -> Vec<u8>`, `Writer<W: Write>` | stripped everything correctly in all three samples | | | | | | fine for stripping |
| `anstream::adapter::strip_str` | anstream 1.0.0 | `strip_str(data: &str) -> StrippedStr<'_>`; `.to_string()`; also `StripStr`/`strip_bytes` | identical result to strip-ansi-escapes on all samples | | | | | | fine; one crate for both strip and (partial) parse |
| `anstyle-parse` | 1.0.0 | `Parser::new()`, `parser.advance(&mut performer, byte)`, `trait Perform { print, execute, csi_dispatch(params: &Params, intermediates, ignore, action: u8), … }` | the VTE state machine both of the above build on | | | | | | building block |
| `vte` | 0.15.0 | same shape | | | | | | | building block |

**Recommendation:** do not depend on cansi or anstream for the *styled* path. Write writer's own
SGR state machine (~100 lines) on top of presenter's tokenizer `escape_sequence`/`normalize_sgr`/
`colon_sgr` (which already turn `38:5:n`, `4:3`, empty params and private sequences into clean
`;`-separated SGR or drop them, and skip OSC/DCS/nF/lone-ESC) and emit
`StyledSpan { text: String, style: anstyle::Style }` — reusing `anstyle 1.0.14` (no deps, MSRV
1.66) for the colour/effects types. Handle: `0`, `1`, `2`, `3`, `4`, `7`, `9`, `21`, `22`, `23`,
`24`, `27`, `29`, `30–37`, `39`, `40–47`, `49`, `90–97`, `100–107`, `38;5;n`, `48;5;n`,
`38;2;r;g;b`, `48;2;r;g;b`. Carry nbconvert's semantics where it matters: codes `21` and `22`
both turn bold off, `5` (blink) is mapped to bold, bold + palette colour 0–7 → bright variant
(+8). Stripping = the same tokenizer with every sequence dropped (presenter's
`sanitize_ansi(text, false)`), or `strip_ansi_escapes::strip_str` if a crate is preferred.

256-colour → RGB (nbconvert `_get_extended_color`, verified): n < 16 → palette index; 16–231 →
6×6×6 cube with each component `0` or `55 + k*40`; 232–255 → grey `(n-232)*10 + 8`.

Per-target rendering of spans:
- **HTML**: nbconvert's class scheme is `ansi-<name>-fg`/`-bg` for the 16 palette colours
  (`ansi-black … ansi-white-intense`), `ansi-bold`, `ansi-underline`, `ansi-default-inverse-fg/-bg`,
  and inline `style="color: rgb(r,g,b)"` for 256/true colour — or presenter's `var(--red,#a00)`
  inline-style scheme. Pick one and theme it in CSS.
- **DOCX**: a run per span: `w:color w:val="RRGGBB"` (palette → the theme's 16 hex values),
  `w:shd w:fill="RRGGBB"` for background, `w:b`, `w:i`, `w:u`, `w:strike`; invert → swap
  fg/bg. Default: colours kept, or `notebook.ansi = "strip"` config to drop them.
- **typst/LaTeX**: strip (default). nbconvert's LaTeX converter emits `\textcolor{ansi-red}{…}`,
  `\colorbox{…}{…}` with `\setlength{\fboxsep}{0pt}`, `\textbf{`, `\underline{`, and
  `\def\tcRGB{\textcolor[RGB]}` for extended colours — only usable if the BYO template defines
  the `ansi-*` colours; offer it behind an option, not by default.

---

## 8. Images from notebook outputs

### 8.1 How presenter does it [verified]

`outputs.rs::image_html`: base64 payload → drop ASCII whitespace → `STANDARD_PAD_INDIFFERENT.decode`
→ `assets.register_bytes(bytes, mime, &format!("output.{ext}"))` with `ext = "jpg"` for
`image/jpeg`, else the part after `image/` (`png`, `gif`, `webp`); the HTML gets
`src="asset:<id>"` and `width`/`height` attributes from `dimension(metadata, mime, key)`
(`metadata[mime][key]`, else `metadata[key]`, numeric and non-negative only). Site mode writes
the file as `hashed_file_name(hint, content_hash(bytes))` → `output-<12hex>.png`
(`emit.rs::url_from`). Markdown-cell `attachments` go through the same registry (first MIME of
the priority list in the bundle).

### 8.2 Proposed scheme for writer

- Register as today (`register_bytes`, content-addressed id, `AssetData::Bytes`). Record the
  *preferred file name* in the IR, not in the registry: `ImageRef.file_hint =
  "<stem>_cell<idx>_out<j>.<ext>"` where `<stem>` = notebook file stem (sanitised with
  `hashed_file_name`'s character rule: `[A-Za-z0-9._-]`, others → `_`), `<idx>` = 0-based cell
  index in the `.ipynb` (stable across edits of other cells' *content*, matches the `cell N`
  in diagnostics), `<j>` = 0-based index among the cell's parsed outputs. Derived renditions add a
  suffix: `…_out<j>.svg` → `…_out<j>.svg.png` (DOCX) and `…_out<j>.svg.pdf` (LaTeX), so the
  source file and its conversions sit together and a reader of the `.tex` can tell where a figure
  came from. Markdown attachments: `<stem>_cell<idx>_<attachment name>`.
- Emitters write assets to `<output stem>_files/` next to the output file (quarto's
  `<name>_files/` convention) and reference them relatively: LaTeX `\includegraphics{<out>_files/…}`,
  typst `image("<out>_files/…")` (typst resolves paths relative to the `.typ` file; writer compiles
  the `.typ` from a directory, so the directory is the project root of the compilation).
  HTML standalone keeps `data:` URIs as presenter does; DOCX embeds (`word/media/image<n>.<ext>`
  is the package-internal name, independent of the hint).
- Where the IR records it:

```rust
pub struct ImageRef {
    pub asset: String,                 // registry id ("asset:<id>" placeholder in HTML)
    pub mime: String,                  // image/png | image/jpeg | image/gif | image/webp | image/svg+xml | application/pdf
    pub file_hint: String,             // "<stem>_cell<idx>_out<j>.<ext>"
    pub width: Option<Length>,         // from metadata[mime].width / metadata.width (px), else None
    pub height: Option<Length>,
    pub intrinsic_px: Option<(u32, u32)>, // imagesize::blob_size for raster; usvg::Tree::size() for SVG
    pub alt: Option<String>,           // fig-alt
}
```

- Intrinsic size: `imagesize 0.15.0` (MIT) — `pub fn blob_size(data: &[u8]) -> ImageResult<ImageSize>`
  with `pub struct ImageSize { pub width: usize, pub height: usize }`, `pub fn image_type(header:
  &[u8]) -> ImageResult<ImageType>`; default features include `png`, `jpeg`, `gif`, `webp` (and
  many more). Verified on the 27 733-byte notebook PNG → `807x243 (Ok(Png))`. Sizes given in
  `metadata` are CSS px; DOCX wants EMU (1 px = 9525 EMU at 96 dpi **[well known, unverified
  here]**), typst/LaTeX want physical lengths: use `px / 96 in` unless the figure option gives a
  width.
- SVG → PNG (DOCX): `resvg 0.48.1` (Apache-2.0 OR MIT, rust-version 1.85; re-exports `usvg` and
  `tiny_skia`): verified round trip — `usvg::Tree::from_str(SVG, &opts)` (120×60) rendered at 2×
  into `Pixmap::new(240,120)` → `encode_png()` = 2 259 bytes. Default features pull `text`,
  `system-fonts`, `raster-images` (gif, webp, jpeg decoders) — keep `text` + `system-fonts` so
  matplotlib SVG text renders; `opts.fontdb_mut().load_system_fonts()` is needed.
- SVG → PDF (LaTeX): `svg2pdf 0.13.0` (MIT OR Apache-2.0) — `to_pdf(tree: &Tree,
  ConversionOptions::default(), PageOptions::default()) -> Result<Vec<u8>>` produced an 11 140-byte
  `%PDF-1.7` single-page file. **Caveat:** svg2pdf 0.13 pins `usvg 0.45`, resvg 0.48 uses
  `usvg 0.48`; both compile together (two `usvg` versions in the tree, verified) but the SVG must
  be parsed twice with the respective `svg2pdf::usvg` / `resvg::usvg`. Check for a newer svg2pdf
  when implementing. Alternative without the duplicate: shell out to Inkscape like nbconvert
  (`inkscape --export-filename=<pdf> <svg>` for Inkscape ≥ 1) — not installed on most machines;
  keep as an opt-in `[notebook] svg_converter`.
- GIF/WebP in LaTeX: convert to PNG with the `image` crate (0.25.10, MSRV 1.88) **[not probed]**
  or warn `output-image-unsupported` and fall back to `text/plain`. typst accepts both natively.

---

## 9. IR shape for a notebook cell (recommendation)

```rust
/// One Jupyter code cell as a document block. Markdown cells become ordinary `Block`s
/// (headings continue the document's numbering); raw cells become `Block::Raw`.
pub struct CodeCell {
    pub index: usize,                     // 0-based cell index in the .ipynb (diagnostics, file names)
    pub id: Option<String>,               // nbformat 4.5 cell id
    pub language: String,                 // notebook.language()
    pub source: String,                   // input without the `#|` option lines
    pub execution_count: Option<i64>,     // `In [n]`
    pub outputs: Vec<Output>,             // streams merged, CR rule applied
    pub options: CellOptions,             // §5.3
    pub span: SourceSpan,                 // cell: Some(index)
}

pub enum Output {
    Stream { name: StreamName, text: AnsiText },                           // stdout | stderr
    Error { ename: String, evalue: String, traceback: AnsiText },          // traceback.join("\n")
    Rich {                                                                 // display_data | execute_result
        execution_count: Option<i64>,                                      // Some only for execute_result
        mimes: Vec<String>,                                                // bundle keys, for diagnostics
        candidates: Vec<RichCandidate>,                                    // in notebook-wide priority order
        label: Option<String>,                                             // fig-x, fig-x-2, tbl-x …
        caption: Option<Inlines>,
    },
    Unsupported { mimes: Vec<String> },
}

/// What one MIME type of a bundle parsed into. A backend walks `candidates` and takes the
/// first it can render (per-target priority = order of this list filtered by the target).
pub enum RichCandidate {
    Figure { mime: String, image: ImageRef },                 // raster, SVG, PDF
    Table { mime: String, table: Table },                     // text/html that passed the table parser
    Markdown { blocks: Vec<Block> },                          // text/markdown
    Math { tex: String },                                     // text/latex, one formula
    RawLatex { text: String },                                // text/latex, not a single formula
    RawHtml { html: String, has_script: bool },               // text/html that is not a plain table
    Code { lang: String, text: String },                      // application/json (+json)
    Text(AnsiText),                                           // text/plain
}

pub struct AnsiText { pub stripped: String, pub spans: Vec<StyledSpan> } // spans concatenate to `stripped`
pub struct StyledSpan { pub text: String, pub style: anstyle::Style }

pub struct Table {
    pub caption: Option<Inlines>, pub label: Option<String>,
    pub head: Vec<Row>, pub body: Vec<Row>, pub foot: Vec<Row>,
    pub note: Option<String>,                                 // "100 rows × 2 columns"
    pub col_widths: Option<Vec<f32>>,                         // tbl-colwidths
}
pub struct Row(pub Vec<TableCell>);
pub struct TableCell { pub content: Inlines, pub header: bool, pub colspan: u32, pub rowspan: u32, pub align: Option<Align> }

pub struct CellOptions {
    pub label: Option<String>, pub lst_label: Option<String>, pub lst_cap: Option<Inlines>,
    pub fig_cap: Option<Caption>, pub fig_subcap: Option<Caption>, pub fig_alt: Option<String>,
    pub fig_align: Option<Align>, pub fig_pos: Option<String>, pub fig_scap: Option<String>, pub fig_env: Option<String>,
    pub tbl_cap: Option<Caption>, pub tbl_subcap: Option<Caption>, pub tbl_colwidths: Option<ColWidths>,
    pub echo: bool, pub output: OutputMode /* Show | Hide | Asis */, pub include: bool,
    pub warning: bool, pub error: bool, pub classes: Vec<String>, pub tags: Vec<String>,
    pub unsupported: Vec<String>,                             // option names ignored (one warning)
}
pub enum Caption { Single(Inlines), PerOutput(Vec<Inlines>), Auto /* fig-subcap: true */ }
```

The table/figure types are the *document* IR's `Table`/`Figure` blocks so that a code cell's
outputs can be lowered to plain blocks (`Block::Figure`, `Block::Table`, `Block::CodeBlock`,
`Block::Verbatim`, `Block::Math`, `Block::Raw`) by a small "lower_cell" pass; backends that have
no notion of a cell (LaTeX, typst) then need nothing cell-specific beyond the input listing.
Keep `Block::Cell(CodeCell)` for HTML/DOCX where the In/Out prompt styling is wanted.

---

## 10. Rendering rules per target (summary table)

| Element | HTML | DOCX | typst → PDF | LaTeX (BYO template) |
|---|---|---|---|---|
| input (`echo`) | `<pre><code class="language-<lang>">`, prompt `In [n]` when `show_prompts`; `lst-cap` → `<figcaption>` | code paragraph style (monospace), optional prompt | `raw(block: true, lang: "<lang>")` inside `figure(caption: …, kind: "listing")` when `lst-cap`/`lst-label` | `\begin{lstlisting}`/`minted` is the template's choice: emit `\begin{writerlisting}[lang]…` macro? No — emit `\begin{verbatim}` by default, overridable by a template hook (open question) |
| raster figure | `<figure><img …><figcaption>` | picture + caption paragraph ("Figure N: …") | `figure(image("…", width: …), caption: […]) <label>` | `\begin{figure}[H]\centering\includegraphics[width=…]{…}\caption{…}\label{…}\end{figure}` (`fig-pos` default `H` like quarto; needs `float` package — template's job) |
| SVG figure | inline `<svg>` | PNG rendition | `image("….svg")` | PDF rendition |
| figure group (N outputs, `fig-subcap`) | `<figure id=fig-x>` with nested `<figure>`s | consecutive pictures with (a)/(b) captions + group caption | `figure(grid(…), caption)` with `subfigure`-like labelling via `subpar`-style helper — open question; simplest: N separate figures labelled `fig-x-1…` | `subfigure` env (`subcaption` package) or N figures — template decides; emit N `\begin{figure}` with `\label{fig-x-1}` by default |
| table (parsed) | `<table>` with `<thead>`/`<tbody>`, `colspan`/`rowspan`, `<caption>` | `w:tbl` with `w:gridSpan` / `w:vMerge` | `table(columns: N, …, table.cell(colspan: …, rowspan: …)[…])` in `figure(kind: table)` | `\begin{table}\centering\begin{tabular}{…}` with `\multicolumn{n}{c}{…}` / `\multirow{n}{*}{…}` (`multirow` package — template's job); `\caption`, `\label` |
| `text/html` not a table | verbatim | `text/plain` fallback + warning | same | same |
| `text/markdown` | blocks | blocks | blocks | blocks |
| `text/latex` formula | KaTeX display | OMML | typst math via converter or `text/plain` fallback | `\[…\]` |
| `application/json` | `<pre class="output-json">` | code style | `raw(block: true, lang: "json")` | `verbatim` |
| `text/plain`, streams, errors | `<pre class="output-text|stream-stdout|stream-stderr|error">` with ANSI spans | runs (colour kept or stripped per config) | `raw(block: true)` stripped | `verbatim` stripped (nbconvert-style `\textcolor` opt-in) |
| widgets / plotly / vega | `text/plain` + warning (HTML may keep plotly's `text/html`, warn `output-script`) | `text/plain` + warning | same | same |
| raw cell | `text/html` verbatim; `text/markdown` parsed | `text/markdown` parsed; others skipped + warning | `text/markdown`, typst raw format | `text/latex` verbatim; `text/markdown` parsed |
| prompts `Out [n]` | optional (`show_prompts`) | optional | never | never |

---

## 11. Markdown cells and notebook metadata

- Markdown cells use **the same dialect as `.md` files**: attributes (`{#id .class k=v}`),
  directives (`:::`), math, callouts. Parse each cell as a fragment of the one document (not a
  separate document): heading numbering, footnote numbering and cross-references continue across
  cells, and definition of a label in cell 3 can be referenced in cell 9. Front matter: only the
  **first markdown cell** (not necessarily cell 0) may start with a `---` YAML block; it is removed
  from the cell. A `---` block at the start of a later cell that would set document metadata →
  removed with `front-matter-ignored` (presenter's rule; keeps Jupyter rendering and writer
  rendering consistent). presenter's `RenderMode::Notebook` disables comrak's front-matter
  extension for cells so a lone `---` block in a later cell is Markdown (a thematic break +
  heading, verified in presenter's test: `"---\ntitle: x\n---\n\nBody\n"` → `<hr />\n<h2>title:
  x</h2>\n<p>Body</p>\n`).
- `:::include` inside a notebook markdown cell: presenter drops it (`include-in-notebook`), because
  a deck is self-contained. For writer, including `.md`/`.ipynb` from a notebook cell is useful
  (a notebook as the main document pulling in chapters). Recommend allowing it, resolved against
  the notebook's directory, with the same cycle detection as `.md` includes (open question §12).
- `attachment:<name>` images → inline assets (`register_bytes` with the first MIME in the
  priority list of the attachment's bundle, hint `<stem>_cell<idx>_<name>`); a missing attachment
  → `missing-attachment` warning, URL left as written (presenter's behaviour).
- Relative file URLs in markdown cells resolve against the notebook's directory, like a `.md`
  file; in **outputs** (`text/html`, `text/markdown`) only files **inside** that directory are
  embedded (`output-file-outside`, presenter audit F08) — keep.
- **Notebook metadata → document metadata**, later wins (mirror of presenter §5 with the key
  renamed): (1) `metadata.title` and `metadata.authors[].name` (nbformat 4.5 schema keys,
  verified) → `title`, `authors`; (2) `metadata.writer` object — the same keys as `.md` front
  matter (`title`, `author`/`authors`, `date`, `lang`, `bibliography`, `numbering`, the config
  tables `notebook:`, `markdown:`, `html:`, `docx:`, `typst:`, `latex:` …; unknown keys →
  `vars`); (3) the first markdown cell's `---` front matter. Not an object (`metadata.writer:
  "x"`) → `writer-not-object` warning, ignored (presenter's `PRESENTER_NOT_OBJECT`). Per-cell
  `metadata.writer` = cell options (§5.3). `metadata.language_info.name` → default code language.
  The notebook-level `notebook:` table (in `metadata.writer` or front matter) can set
  `mime_priority`, tag lists, `show_prompts`, `ansi`, `echo`/`output` defaults for the whole
  file — presenter already merges config tables from the notebook's layers before converting
  cells (`merge_config`).

---

## 12. Open questions

1. **Raw cells without `raw_mimetype`**: nbconvert passes them through to every format
   (`"" in [output_mimetype, ""]`). Passthrough-with-warning (nbconvert-compatible) or
   skip-with-warning? (§6)
2. **DOCX and SVG**: Word 2016+ can embed SVG natively (an `asvg:svgBlip` extension element next
   to a PNG fallback blip) **[unverified]**; whether the DOCX writer chosen by the DOCX research
   topic supports it decides if the PNG rendition is the fallback or the only rendition.
3. **`text/latex` → typst**: a TeX→typst math converter (`mitex` is a typst package; a Rust crate
   `mitex` exists **[unverified]**) or fall back to `text/plain`. Affects SymPy-heavy notebooks.
4. **`:::include` from notebook markdown cells** — allow (recommended) or keep presenter's refusal?
5. **Figure groups** (several figures from one cell with `fig-subcap`): emit nested sub-figures
   (needs `subcaption`/typst `subpar`-like support in every backend) or N independent figures
   labelled `fig-x-1…` (simple; recommended for v1)?
6. **svg2pdf's usvg pin** (0.45 vs resvg's 0.48): accept the duplicate crate, wait for an aligned
   release, or render SVG→PDF through typst itself (typst accepts SVG; for the LaTeX target only
   the duplicate matters).
7. **Styler HTML without escaping** (`b<c` in a cell): accept the html5ever recovery (text
   truncated at `<`) silently, or warn `output-html-malformed` when `Html.errors` is non-empty?
   Recommend: warn once per output, still use the parsed table.
8. **Quarto's "pandas table as text/plain with quotes" quirk** (comment in `jupyter.ts`: "pandas
   inexplicably outputs html tables as text/plain with an enclosing single-quote") — only when
   quarto *executes*; not reproduced with saved outputs; ignore unless a fixture shows it.
9. **Prompt numbers in DOCX** (`In [3]:`): style choice for the DOCX template topic.
10. Should `echo: false` + `output: false` cells still contribute their label/caption (quarto:
    `include: false` removes everything; `echo: false` + `output: false` leaves an empty div)?
    Recommend treating it as `include: false` (presenter: "hide_input && hide_output → omitted").
11. GIF/WebP conversion for LaTeX via the `image` crate (adds ~25 crates **[unverified count]**)
    vs. a warning.

---

## 13. Verified API hints (exact signatures/snippets) and scratch artifacts

### 13.1 Signatures copied from crate sources

```rust
// scraper 0.29.0  (~/.cargo/registry/src/*/scraper-0.29.0/src/{html/mod.rs,element_ref/mod.rs,node.rs,selector.rs})
impl Html {
    pub fn parse_document(document: &str) -> Self;
    pub fn parse_fragment(fragment: &str) -> Self;   // context element <body>; root_element() is <html>
    pub fn select<'a, 'b>(&'a self, selector: &'b Selector) -> Select<'a, 'b>;
    pub fn root_element(&self) -> ElementRef<'_>;
    pub errors: Vec<Cow<'static, str>>;             // html5ever parse errors
}
impl Selector { pub fn parse(selectors: &str) -> Result<Self, SelectorErrorKind>; }
impl<'a> ElementRef<'a> {
    pub fn wrap(node: NodeRef<'a, Node>) -> Option<Self>;
    pub fn value(&self) -> &'a Element;
    pub fn select<'b>(&self, selector: &'b Selector) -> Select<'a, 'b>;
    pub fn html(&self) -> String;  pub fn inner_html(&self) -> String;
    pub fn attr(&self, attr: &str) -> Option<&'a str>;
    pub fn text(&self) -> Text<'a>;                                   // impl Iterator<Item = &'a str>
    pub fn child_elements(&self) -> impl Iterator<Item = ElementRef<'a>> + use<'a>;
    pub fn descendent_elements(&self) -> impl Iterator<Item = ElementRef<'a>> + use<'a>;
}
impl<'a> Deref for ElementRef<'a> { type Target = NodeRef<'a, Node>; }  // ego_tree: .children(), .parent(), .value(), .id()
impl Element { pub fn name(&self) -> &str; pub fn id(&self) -> Option<&str>; pub fn classes(&self) -> Classes<'_>; pub fn attr(&self, attr: &str) -> Option<&str>; pub fn attrs(&self) -> Attrs<'_>; }
pub enum Node { Document, Fragment, Doctype(Doctype), Comment(Comment), Text(Text), Element(Element), ProcessingInstruction(ProcessingInstruction) }
impl Node { pub fn is_text(&self) -> bool; pub fn is_element(&self) -> bool; pub fn as_text(&self) -> Option<&Text>; pub fn as_element(&self) -> Option<&Element>; }

// cansi 2.2.1
pub fn cansi::v3::categorise_text(text: &str) -> v3::CategorisedSlices /* = Vec<CategorisedSlice<'_>> */;
pub struct cansi::v3::CategorisedSlice<'text> { pub text: &'text str, pub start: usize, pub end: usize, pub fg: Option<Color>, pub bg: Option<Color>, pub intensity: Option<Intensity>, pub italic: Option<bool>, pub underline: Option<bool>, pub blink: Option<bool>, pub reversed: Option<bool>, pub hidden: Option<bool>, pub strikethrough: Option<bool> }
pub enum cansi::Color { Black, Red, Green, Yellow, Blue, Magenta, Cyan, White, BrightBlack, …, BrightWhite }  // 16 only
pub enum cansi::Intensity { Normal, Bold, Faint }

// anstream 1.0.0
pub struct anstream::adapter::WinconBytes; impl WinconBytes { pub fn new() -> Self; pub fn extract_next<'s>(&'s mut self, bytes: &'s [u8]) -> WinconBytesIter<'s>; }
impl Iterator for WinconBytesIter<'_> { type Item = (anstyle::Style, String); }
pub fn anstream::adapter::strip_str(data: &str) -> StrippedStr<'_>;  impl StrippedStr<'_> { pub fn to_string(&self) -> String; }
pub fn anstream::adapter::strip_bytes(data: &[u8]) -> StrippedBytes<'_>;
// anstyle 1.0.14
impl Style { pub const fn get_fg_color(self) -> Option<Color>; pub const fn get_bg_color(self) -> Option<Color>; pub const fn get_effects(self) -> Effects; }
pub enum Color { Ansi(AnsiColor), Ansi256(Ansi256Color), Rgb(RgbColor) }   // Ansi256Color(pub u8), RgbColor(pub u8, pub u8, pub u8)
// anstyle-parse 1.0.0
pub struct Parser<C = DefaultCharAccumulator>; impl Parser { pub fn new() -> Parser; pub fn advance<P: Perform>(&mut self, performer: &mut P, byte: u8); }
pub trait Perform { fn print(&mut self, c: char); fn execute(&mut self, byte: u8); fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8); /* + hook, put, unhook, osc_dispatch, esc_dispatch */ }

// strip-ansi-escapes 0.2.1 (vte 0.14)
pub fn strip<T: AsRef<[u8]>>(data: T) -> Vec<u8>;
pub fn strip_str<T: AsRef<str>>(data: T) -> String;

// ansi-to-html 0.2.4
pub fn convert(ansi_string: &str) -> Result<String, Error>;
pub fn convert_with_opts(input: &str, converter: &Converter) -> Result<String, Error>;
pub struct Esc<T>(pub T);   // Display = HTML-escaped

// imagesize 0.15.0
pub fn blob_size(data: &[u8]) -> ImageResult<ImageSize>;   pub struct ImageSize { pub width: usize, pub height: usize }
pub fn image_type(header: &[u8]) -> ImageResult<ImageType>;   pub fn size<P: AsRef<Path>>(path: P) -> ImageResult<ImageSize>;

// resvg 0.48.1 (re-exports `usvg`, `tiny_skia`)
pub fn resvg::render(tree: &usvg::Tree, transform: tiny_skia::Transform, pixmap: &mut tiny_skia::PixmapMut);
pub fn usvg::Tree::from_str(text: &str, opt: &usvg::Options) -> Result<usvg::Tree, usvg::Error>;
pub fn usvg::Tree::from_data(data: &[u8], opt: &usvg::Options) -> Result<usvg::Tree, usvg::Error>;
pub fn usvg::Tree::size(&self) -> usvg::Size;             // .width() / .height() as f32
impl Default for usvg::Options<'_>;   pub fn usvg::Options::fontdb_mut(&mut self) -> &mut fontdb::Database;  // .load_system_fonts()
pub fn tiny_skia::Pixmap::new(width: u32, height: u32) -> Option<Pixmap>;  pub fn as_mut(&mut self) -> PixmapMut<'_>;  pub fn encode_png(&self) -> Result<Vec<u8>, png::EncodingError>;
// svg2pdf 0.13.0 (re-exports its own `usvg` 0.45)
pub fn svg2pdf::to_pdf(tree: &svg2pdf::usvg::Tree, conversion_options: ConversionOptions, page_options: PageOptions) -> Result<Vec<u8>>;
pub struct ConversionOptions { pub compress: bool /*true*/, pub raster_scale: f32 /*1.5*/, pub embed_text: bool /*true*/, /* pdfa … */ }
pub struct PageOptions { pub dpi: f32 /*72.0*/ }
```

### 13.2 Snippet: SVG renditions (compiled and run, `img-probe`)

```rust
let mut opts = resvg::usvg::Options::default();
opts.fontdb_mut().load_system_fonts();
let tree = resvg::usvg::Tree::from_str(svg, &opts)?;
let size = tree.size();                                   // 120x60 for the sample
let scale = 2.0f32;
let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).unwrap();
resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
let png: Vec<u8> = pixmap.encode_png()?;                  // 240x120, 2 259 bytes

let mut opts2 = svg2pdf::usvg::Options::default();        // NOTE: a different usvg (0.45)
opts2.fontdb_mut().load_system_fonts();
let tree2 = svg2pdf::usvg::Tree::from_str(svg, &opts2)?;
let pdf: Vec<u8> = svg2pdf::to_pdf(&tree2, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions::default())?; // %PDF-1.7, 11 140 bytes
```

### 13.3 Snippet: cell text extraction with scraper (from `nb-probe`)

```rust
fn cell_text(el: ElementRef<'_>, out: &mut String) {
    for child in el.children() {
        match child.value() {
            Node::Text(text) => out.push_str(text),
            Node::Element(e) => {
                if e.name() == "br" { out.push('\n'); continue; }
                if let Some(child_el) = ElementRef::wrap(child) { cell_text(child_el, out); }
                if matches!(e.name(), "p" | "div" | "li" | "tr") { out.push('\n'); }
            }
            _ => {}
        }
    }
}
fn span(el: ElementRef<'_>, attr: &str) -> u32 {
    el.attr(attr).and_then(|v| v.trim().parse::<u32>().ok()).filter(|&n| n >= 1).unwrap_or(1)
}
```

### 13.4 Scratch artifacts (session scratchpad; copy what is needed into `tests/fixtures/`)

Base: `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/`
- `build/nb-probe/src/main.rs` — scraper table probe (deps: `scraper = "0.29.0"`).
- `build/ansi-probe/src/main.rs` — ANSI probe (deps: `cansi 2.2.1`, `anstream 1.0.0`, `anstyle 1.0.14`, `anstyle-parse 1.0.0`, `vte 0.15.0`, `strip-ansi-escapes 0.2.1`, `ansi-to-html 0.2.4`).
- `build/img-probe/src/main.rs` — imagesize/resvg/svg2pdf probe (deps: `imagesize 0.15.0`, `resvg 0.48.1`, `usvg 0.48.1`, `svg2pdf 0.13.0`).
- `build/target/` — shared `CARGO_TARGET_DIR`.
- `pandas-out/{simple,simple_repr,multiindex,styler,truncated,truncated_cols,nan}.html` — pandas 3.1.0 samples; `traceback.{json,txt}` — IPython 9.17.1 error output (nbformat `error` object in the JSON); `analysis_tb.txt` — the IPython 8-era traceback from presenter's `analysis.ipynb`; `cell4.png` — the 807×243 PNG from that notebook; `svg_render.{png,pdf}`.
- `scripts/gen_tables.py`, `gen_tables2.py`, `gen_tb2.py` — generators (run with the venv at `pyenv/venv/bin/python -I`).

### 13.5 Versions (checked with `cargo info` on 2026-10-24)

| crate | version | licence | rust-version |
|---|---|---|---|
| scraper | 0.29.0 | ISC | — (edition 2024) |
| html5ever | 0.40.1 | MIT OR Apache-2.0 | 1.85 |
| lol_html | 3.0.2 | BSD-3-Clause | 1.85 |
| cansi | 2.2.1 | MIT | — |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 | 1.66.0 |
| anstream | 1.0.0 | MIT OR Apache-2.0 | 1.66.0 |
| anstyle | 1.0.14 | MIT OR Apache-2.0 | 1.66.0 |
| vte | 0.15.0 | Apache-2.0 OR MIT | 1.62.1 |
| strip-ansi-escapes | 0.2.1 | Apache-2.0/MIT | — |
| ansi-to-html | 0.2.4 | MIT OR Apache-2.0 | 1.85.1 |
| ansi-parser | 0.9.1 | MPL-2.0 | — (not evaluated; MPL) |
| imagesize | 0.15.0 | MIT | — |
| resvg / usvg | 0.48.1 | Apache-2.0 OR MIT | 1.85.0 |
| svg2pdf | 0.13.0 | MIT OR Apache-2.0 | — (depends on usvg 0.45) |
| image | 0.25.10 | MIT OR Apache-2.0 | 1.88.0 |
| png | 0.18.1 | MIT OR Apache-2.0 | 1.73 |
