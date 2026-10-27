# Spec: Jupyter notebooks → document IR

**Status:** contract. Implemented by T18 (`src/notebook/{mod,nbformat,convert,options,outputs,tables,ansi}.rs`:
§1–§6, §8; plus the ANSI arms of the HTML and DOCX emitters, §5), T01 (`CellOutput::pick` in
`src/ir.rs`, §4.2), T1B (`:::include` inside markdown cells and `.ipynb` includes from Markdown, §2.6,
§7.1–7.2), T53 (notebooks included from typst, §7.3), T1C/T71
(`check`, §8). Consumers: the emitters T13, T30/T31, T40/T41, T60 (§4.2, §5, §9), resolve T17 (§3.4,
§4.6), the guide (T74) and the fixtures below. Lifted from presenter_rs (`src/notebook/*`, its
`notebook-mapping.md` §1/§3/§4/§5): the input model, the lenient parser, tags, the MIME helpers,
stream merging, the carriage-return rule and the local-file rule. Slides are gone: a notebook is one
document whose markdown cells are ordinary Markdown of `markdown-dialect.md` and whose code cells are
`Block::Cell` (ir.md §1). Saved outputs are rendered; nothing is executed (PLAN §1).

## 1. Input model (`src/notebook/nbformat.rs`, T18, lifted verbatim minus slides)

nbformat 4.x JSON, deserialized leniently with `serde_json`:

```rust
#[derive(Deserialize)] pub struct Notebook {
    pub cells: Vec<Cell>,
    #[serde(default)] pub metadata: serde_json::Value,
    pub nbformat: u32, #[serde(default)] pub nbformat_minor: u32,
}
#[derive(Deserialize)] pub struct Cell {
    pub cell_type: CellType,                    // markdown | code | raw | Unknown (#[serde(other)])
    #[serde(default)] pub id: Option<String>,
    #[serde(default)] pub metadata: serde_json::Value,
    #[serde(default)] pub source: MultilineString,
    #[serde(default)] pub outputs: Vec<Output>,            // code cells only
    #[serde(default)] pub execution_count: Option<i64>,
    #[serde(default)] pub attachments: BTreeMap<String, BTreeMap<String, MultilineString>>, // name → MIME → base64
}
#[derive(Deserialize)] #[serde(tag = "output_type", rename_all = "snake_case")]
pub enum Output {
    Stream { name: String, text: MultilineString },
    DisplayData { data: MimeBundle, #[serde(default)] metadata: Value },
    ExecuteResult { data: MimeBundle, #[serde(default)] metadata: Value, execution_count: Option<i64> },
    Error { ename: String, evalue: String, traceback: Vec<String> },
}
/// `"text"` or `["li","ne","s"]` → one String (segments concatenated, nothing inserted); `null` → "".
pub struct MultilineString(pub String);
/// MIME type → payload: a string or list of strings (text), or arbitrary JSON (`*json`). Converted lazily.
pub type MimeBundle = BTreeMap<String, serde_json::Value>;

pub fn parse_notebook(path: &Path, json: &str) -> crate::error::Result<(Notebook, Diagnostics)>;  // the lenient entry point
pub fn parse_outputs(raw: Vec<Value>) -> (Vec<Output>, Vec<String>);
pub fn mime_text(value: &Value) -> Option<String>;   // string or list of strings → String; anything else → None
```

- `parse_notebook` is the only entry point (deserializing `Notebook` directly is strict). One leading
  U+FEFF is ignored. Invalid JSON, a non-object document, `nbformat` missing or not an integer,
  `nbformat < 4` (message exactly `"nbformat 4 required"`), `cells` missing or not a list, and a cell
  that does not deserialize (message `cell <index>: …`) are `Error::Parse { path, line, .. }`.
- `nbformat > 4` → warning `nbformat-newer`, read as 4. An output that does not deserialize (unknown
  `output_type`, missing field) is dropped with `notebook-output-skipped` (`"output <i> (<type>)
  skipped: …"`, `i` = position in the file); a bad attachment → `notebook-attachment-skipped`; an
  unknown `cell_type` → `CellType::Unknown` + `unknown-cell-type` (kept so indices align; converters skip
  it). Unknown keys anywhere are ignored; JSON `null` counts as absent everywhere.
- Accessors (lifted; `presenter` renamed): `Cell::tags()` (`metadata.tags`, strings only, in order),
  `is_hidden_source()` / `is_hidden_outputs()` (`metadata.jupyter.source_hidden|outputs_hidden == true`),
  `raw_mimetype()` (`metadata.raw_mimetype`, else `metadata.format`), `writer_options()` (cell
  `metadata.writer` when an object), `span(index)` (`SourceSpan { start_line: 1, end_line: max(1, source
  lines), cell: Some(index), file: None }`); `Notebook::language()` (`language_info.name`, else
  `kernelspec.language`, else `"text"`; empty = absent), `title()`, `authors()` (`metadata.authors[].name`,
  strings only), `writer_meta()` / `presenter_meta()` (objects only). A notebook or cell
  `metadata.writer` that is neither an object nor `null` → `writer-not-object`, ignored.
- Cell indices are 0-based over **every** cell of the file (`Cell.index`, `SourceSpan.cell`, messages
  `cell 3: …`, file-name hints §6).

## 2. Cells → blocks (`src/notebook/convert.rs`, `mod.rs`, T18)

### 2.1 Entry points and order of work
```rust
// src/notebook/mod.rs (T18). `parse` is ir.md §4's frontend entry; T18 replaces T01's stub body.
pub fn parse(source: &SourceInfo, json: &str, config: &Config) -> Result<Parsed>;
/// = parse_with(source, json, config, ParseOptions { includes: &mut *crate::markdown::include::cell_includes(source),
///                                                   nested: false, selection: Selection::default() })
pub fn parse_with(source: &SourceInfo, json: &str, config: &Config, opts: ParseOptions<'_>) -> Result<Parsed>;
/// The same parse, plus each kept cell's blocks separately (for the typst module of §7.3);
/// `Parsed.document.blocks` is the concatenation of the parts' `body` (plus the Toc unless nested).
pub fn parse_cells(source: &SourceInfo, json: &str, config: &Config, opts: ParseOptions<'_>) -> Result<(Parsed, Vec<CellParts>)>;

pub struct ParseOptions<'a> {
    pub includes: &'a mut dyn CellIncludes,   // §2.6
    pub nested: bool,                         // true when included by another file (§7): no Toc inserted
    pub selection: Selection,                 // §7.3; Default = every cell, nothing forced hidden
}
pub trait CellIncludes {
    /// Expands `:::include <path>` (path as written, quotes removed) found at `span` of a markdown cell.
    /// Returns the included file's parse with spans/diagnostics already tagged with its path and
    /// `document.includes = [file, files it includes…]`; errors are `Error::Parse` at the directive's line.
    fn expand(&mut self, path: &str, span: &SourceSpan, notebook: &SourceInfo, config: &Config) -> Result<Parsed>;
}
#[derive(Debug, Clone, Default)]
pub struct Selection { pub cells: Option<Vec<CellRef>>, pub tags: Option<Vec<String>>, pub hide_input: bool, pub hide_output: bool }
#[derive(Debug, Clone, PartialEq, Eq)] pub enum CellRef { Index(usize), Id(String) }
#[derive(Debug, Clone, PartialEq)] pub enum CellKind { Markdown, Code, Raw }
#[derive(Debug, Clone, PartialEq)]
pub struct CellParts { pub index: usize, pub id: Option<String>, pub tags: Vec<String>, pub kind: CellKind,
                       pub body: Vec<Block>, pub input: Vec<Block>, pub outputs: Vec<Block> } // input/outputs: §7.3
```
`parse_with` does, in this order: (1) `parse_notebook`; (2) document metadata (§2.2) merged into a
clone of `config` (`Config::merge_front_matter`, presenter's layer-by-layer merge so config tables merge
key by key); (3) drop cells that are `Unknown` or removed (§3.3); (4) collect footnote definitions
(§2.4) from every remaining markdown cell; (4b) drop cells not selected (§7.3); (5) convert every kept
cell in order (§2.3, §2.7, §2.8); (6) the cross-cell footnote pass (§2.4); (7) unless `nested`,
insert `Block::Toc { depth: n, span: None }` at index 0 when the effective `document.toc` is
`Depth(n)` and no `Block::Toc` came from a markdown cell (markdown-dialect.md §2.1); (8) `Features`
(`notebook: true` always; `code` when any `Block::Cell` or `CodeBlock` exists; `math` when any `Math`
exists at any depth of any candidate or any kept output's `mimes` contains `text/latex` (ir.md §1: a
`Raw { latex }` payload needs amsmath too); `citations`, `footnotes` from content over **all** candidates);
(9) `notebook-not-run` (§8). `Document.source.kind = Notebook`, `base_dir` = the notebook's directory.

### 2.2 Document metadata (later wins; presenter §5 with writer's keys)
| layer | source | read |
|---|---|---|
| 1 | `metadata.title`, `metadata.authors[].name` (nbformat ≥ 4.2 schema keys; read regardless of `nbformat_minor`) | `DocMeta.title`, `authors` |
| 2 | `metadata.presenter` (object) | only the keys both tools share: `title`, `subtitle`, `author`, `authors`, `date`, `lang`, `description`, `keywords`; one info `notebook-presenter-metadata` names the keys read and those ignored (presenter's deck/reveal/slides tables mean something else) |
| 3 | `metadata.writer` (object) | every front-matter key of config.md §3 (`abstract`, `toc`, `numbering`, `bibliography`, the config tables `notebook:`, `markdown:`, …; unknown → `vars`) |
| 4 | a `---` YAML block at the start of the **first markdown cell** (not necessarily cell 0) | as Markdown front matter (markdown-dialect.md §2.2's rules) through T10's lifted `markdown::frontmatter::split_front_matter(text: &str) -> Option<(&str /*yaml*/, &str /*rest*/, usize /*lines consumed*/)>` and `parse_front_matter(yaml: &str) -> Result<DocMeta>` (presenter's names and shapes, R8 §3.2; T10 exports both) |

- Errors: invalid layer 3 → `Error::FrontMatter("metadata.writer: …")`; invalid layer 4 →
  `Error::FrontMatter("cell <i>: …")`. A layer-4 block that markdown-dialect.md §2.2 flags (no key at
  all and a comment that reads as a Markdown heading) → `front-matter-empty`.
  `abstract` is parsed with `parse_blocks` (`Fragment.cell` = the front-matter cell; layer 3: `None`).
- The front-matter lines of the first markdown cell are replaced by empty lines before parsing, so
  block spans stay lines of the cell as stored. A cell holding only front matter yields no blocks.
- A `---` block at the start of a **later** markdown cell whose YAML is a mapping containing at
  least one key of config.md §3's table other than its "anything else" row (a `DocMeta` field, a
  config-like key such as `toc` or `bibliography`, or a config table) is blanked the same way, with
  the warning `notebook-front-matter-ignored`. A mapping of only other keys (`Note: this matters`
  between two `---` rules is prose, presenter's `sets_deck_metadata`), and any other `---` block
  there, is Markdown (cells are parsed without the front-matter extension).
- Heading text never becomes `meta.title`. Per-cell `metadata.presenter` (slide layout) is ignored silently.

> **Coordinator clarification (2026-10-27):** the merged `DocMeta` must
> replay the metadata layers' effective configuration when the pipeline
> calls `Config::merge_front_matter` again. Remove superseded shorthand
> aliases when later table fields replace them, reconcile `cite_style`/`csl`,
> and preserve omitted numbering keys across boolean/map overlays. This
> normalization adds no configuration defaults to the document metadata.

### 2.3 Markdown cells (and raw cells with a Markdown format, §2.8)
`markdown::parse_blocks(text, &config, &mut Fragment { assets, base_dir: notebook dir, diagnostics, file:
None, cell: Some(index), missing_asset_is_error: false })` (markdown-dialect.md §2.3) on the cell
source after: front-matter blanking (§2.2), `:::include` splitting (§2.6) and appending the footnote
definitions of other cells (§2.4). Every rule of the dialect applies (attributes, directives, figures,
math, callouts, `@refs`, raw fences); relative image paths resolve against the notebook's directory
with `..` allowed (author content, like a `.md` file). One document: heading numbering, figure counters,
labels and references run across cells and across Markdown and code cells; a label defined in cell 3
can be referenced in cell 9. `attachment:` URLs are §2.5. Empty or whitespace-only cells yield nothing.

### 2.4 Footnotes across cells (binding)
Raw cells with a Markdown format (§2.8) count as markdown cells for steps 1–5. Output fragments
(`text/markdown`, `output: asis`, §4.3) do not: they get no appended definitions, every `Footnote`/
`FootnoteRef` label `n` in them becomes `cell<index>-out<j>-n` after parsing (resolve matches
Markdown footnotes by file and label, references.md §4.6, so an output's own `[^1]` must not meet a
cell's), and steps 4–5 skip blocks inside `Block::Cell.outputs`.
1. Before any cell is parsed, scan each markdown cell that is not `Unknown` or removed (§3.3;
   selection, §7.3, does not apply). This scan and §2.6's parse the text exactly as `parse_blocks`
   will: after §2.2's blanking, through T11's `colon_prepass` (lines unchanged), with
   `comrak::parse_document` and `markdown::comrak_options(&config.markdown)` but
   `extension.front_matter_delimiter = None` (else a cell opening with a `---` rule and holding a
   later `---` line is one `FrontMatter` node, *probe*). Every `FootnoteDefinition` that is a direct
   child of the document **and whose name does not start with `__inline_`** is recorded as `(name,
   cell index, start line, source lines)`. Names compare as comrak compares them (trimmed, inner
   whitespace runs collapsed to one space, then `caseless::default_case_fold_str`: comrak's private
   `normalize_label(.., Case::Fold)`), here and in steps 2, 4 and 5. The first definition of a name in
   document order is its *notebook definition*; its spelling is the IR label everywhere (comrak
   rewrites each reference to its definition's spelling). Every later definition of the same name, in
   the same or another cell, has its lines replaced by empty lines before parsing (as §2.2) with the
   warning `footnote-duplicate` at its lines (comrak would bind to the **last** one), so every text
   holds at most one definition per name. comrak 0.57 appends one synthetic definition `__inline_N`
   per `^[…]` to the root (N restarts at 1 in every parse): inline footnotes are local to their cell
   (`Footnote { label: None }`, markdown-dialect.md §6) and are never appended to other cells or
   touched by step 4.
2. Each text given to `parse_blocks` (a cell, or a fragment of a cell split by §2.6) is
   `own text + "\n\n" + the notebook definitions of every name it does not define itself`, each
   separated by a blank line. The appended text comes after its own lines, so its spans are unchanged,
   unless the own text ends inside an open block; such a fragment gets no appended definitions. Test,
   on a scan of the own text as in step 1: follow last children from the document through `List`, `Item`,
   `TaskItem`, `DescriptionList`, `DescriptionItem`, `DescriptionDetails` and `FootnoteDefinition`
   (they run on across blank lines; quotes and alerts end at the separator); the block is open when
   the walk meets a `BlockDirective` that is unclosed (its span is one line, or its last line does not
   match `^\s{0,3}:{3,}\s*$`, markdown-dialect.md §5.1; a closed one ends the walk), a fenced
   `CodeBlock` with `closed == false`, or an `HtmlBlock` of `block_type` 1–5 whose literal lacks that
   type's end condition (*probe*: an unclosed `:::note` takes the definitions as children; an open
   fence in a directive or list item takes them, or the blank lines, as code text). Its references to
   other cells' definitions stay text and raise `footnote-undefined` (step 5).

> **Coordinator clarification (2026-10-27):** synthetic `__inline_N`
> definitions appended by comrak do not replace the last real block in the
> open-block check. A directive closer follows comrak's container-relative
> indentation (at most three spaces) and has at least the opening fence's
> colon count. This also handles footnotes containing directives correctly.

3. Blocks inside footnotes that came from appended text get the span of the definition in its own
   cell (`cell`, lines from step 1). A diagnostic whose span lies within a definition's lines (appended,
   or the definition's own lines in its own cell) is kept only from the fragment that holds the
   `Footnote` step 4 keeps for that name, with the definition's span from step 1; the copies raised by
   other fragments are discarded, so each is reported once. Every `footnote-unreferenced` that
   `parse_blocks` raises is discarded (step 5 replaces it).
4. Cross-cell pass over the kept blocks in document order (outputs skipped, above), skipping every block
   whose `span.file` is `Some` (blocks spliced from included files keep their own per-file footnotes,
   markdown-dialect.md §8): the first `Inline::Footnote { label: Some(n) }` stays; every later one
   becomes `FootnoteRef { label: n }` (ir.md §2 invariant 4; the notebook's own cells are one file for
   footnote names, markdown-dialect.md §8).
5. A notebook definition referenced by no cell → one `footnote-unreferenced` at its definition. A
   `[^name]` left as text (adjacent `Text` inlines concatenated, regex `\[\^([^\]\s]+)\]`; blocks from
   included files skipped) whose name has no notebook definition, or whose fragment got none appended
   (step 2) → warning `footnote-undefined` at that block's span; the text stays.

### 2.5 Attachments (markdown and raw cells; lifted, simplified to IR URLs)
After parsing and §2.4 step 3's re-spanning, every `Inline::Image.url` and every `src`/`poster`
attribute value of a `Raw { format: "html" }` block or an `Inline::Raw { format: "html" }` of the cell
that is exactly `attachment:NAME` (NAME matched as written, then percent-decoded) is replaced by
`asset:<id>` (`Link.url` and `href` values stay as written: links are never registered,
markdown-dialect.md §4.3). Blocks re-spanned to another cell (footnotes from appended definitions)
resolve NAME against that cell (`span.cell`), whose index the hint uses:
`register_bytes(decoded, mime, "<stem>_cell<index>_<NAME sanitized>")` with the first of the
attachment bundle's MIME types in `[notebook] mime_priority` order that is an image type (base64
decoded as in §6). A name the cell does not have → `missing-attachment` warning, URL as written. When
no MIME type of the bundle is both `image/*` and in `mime_priority` (e.g. only `image/bmp`), the URL
stays as written and `notebook-attachment-skipped` names the types offered.

### 2.6 `:::include` in markdown cells (T18 hook, T1B expansion)
- Before parsing, T18 parses the cell text with comrak (§2.4 step 1's text and options) and finds `BlockDirective`
  nodes that are direct children of the document whose name is `include`
  (`markdown::directives::parse_directive_info`, T11). The cell text is split at each such directive's
  lines (opening to closing fence): text before and after is parsed with `parse_blocks` as separate
  fragments, each prefixed with as many `\n` as lines precede it (so spans keep the cell's line
  numbers); the directive becomes the blocks of `opts.includes.expand(path, &span, source,
  &effective_config)`, spliced in place (a non-empty body follows them, with `invalid-directive`,
  markdown-dialect.md §5.4). Each fragment gets §2.4's appended definitions like a cell. Assets, features,
  diagnostics and `includes` of the result are merged (`AssetRegistry::merge`, `Features::merge`,
  `Document.includes` deduplicated, first use first). `:::include` anywhere else in a cell (list, quote,
  directive) is markdown-dialect.md §8's `include-not-top-level`.
- Defined by this spec, in `src/markdown/include.rs` (T1B's file; T18 adds both as stubs, additively,
  whose `expand` returns `Err(Error::Parse { path, line, message: "cell <i>: :::include in notebooks
  arrives with T1B" })`; T1B replaces the bodies with the real expansion, markdown-dialect.md §8: path
  relative to the notebook's directory, `.md`/`.ipynb` only, `MAX_DEPTH = 8`, cycles by canonical path,
  `include-meta-ignored`):
  ```rust
  pub fn cell_includes(notebook: &SourceInfo) -> Box<dyn crate::notebook::CellIncludes>; // ≡ cell_includes_in(notebook, &[])
  /// `chain` = canonical paths of the files that include the notebook, outermost first (not the
  /// notebook itself); the resolver's include stack is `chain + [notebook]` for MAX_DEPTH and cycles.
  pub fn cell_includes_in(notebook: &SourceInfo, chain: &[PathBuf]) -> Box<dyn crate::notebook::CellIncludes>;
  ```
  > **Contract change (coordinator, 2026-10-27):** `CellIncludes` gains the
  > default no-op method `set_parent_meta(&DocMeta)`. The converter calls it
  > after effective metadata construction so the include hook compares
  > ignored child metadata with the actual parent, including in-memory input.
  > Existing custom hooks need no implementation change.

  When T1B includes a `.ipynb` from anywhere it calls `notebook::parse_with(source, json,
  &effective_config, ParseOptions { includes: &mut *cell_includes_in(source, &its_chain), nested:
  true, selection: Selection::default() })`, never `notebook::parse` (which inserts a `Toc` and starts
  a fresh chain), so depth and cycles count across `.md` and `.ipynb` (see Deviations).
- `:::include` inside a `text/markdown` **output** is never expanded: its lines are blanked before
  parsing and the warning `output-include` is raised (outputs are written by code, §4.5).

### 2.7 Code cells → `Block::Cell`
| `ir::Cell` field | value |
|---|---|
| `index`, `id` | cell index (§1), nbformat `id` |
| `language` | `Notebook::language()` |
| `source` | the source without the option lines of §3.1, otherwise verbatim (no trailing-newline trim) |
| `execution_count` | as stored |
| `show_input` / `show_output` | §3.3's resolved `echo` / `output` (`asis` counts as shown) |
| `outputs` | §4, in stored order after stream merging and filtering; **`[]` when `show_output` is false** (hidden outputs are not converted: no assets, no output diagnostics, no captions) |
| `attrs` | `classes`: the `classes` option, then every tag that matches `^[A-Za-z][A-Za-z0-9_-]*$`, deduplicated; `numbered` + pair `highlight` from `code-line-numbers` (§3.2), read by emitters through `CodeAttrs::from_attrs(&cell.attrs)`; `id`: §3.4 |
| `tags` | `metadata.tags` then the `tags` option, deduplicated; `metadata`: verbatim (`Value::Null` if absent) |
| `span` | `cell.span(index)` |

Spans (ir.md §2 invariant 8): every block T18 creates for a code or raw cell gets `span:
Some(cell.span(index))`: the listing `Figure` and its `CodeBlock`, the group `Figure` and both its
`Cell`s, the sub-caption `Paragraph`, every block at any depth of every `OutputCandidate`, and the
raw-cell blocks of §2.8 (except the Markdown row, whose blocks carry cell lines). Blocks parsed from
`text/markdown` outputs and `output: asis` streams are re-spanned recursively (footnote blocks
included) to `cell.span(index)`, and so are their diagnostics, because their line numbers are lines of
the output payload, not of the cell source.

A code cell yields, in order: (a) a listing figure when §3.4 makes one; (b) the `Block::Cell`, or,
for a group (§3.4), an input-only `Block::Cell` (`outputs: []`, `show_output: false`) followed by the group `Figure { kind:
Other }` wrapping an outputs-only `Block::Cell` (`show_input: false`; both cells have the same
`index` and span); (c) nothing for a cell whose input and outputs are both hidden, a cell removed by
§3.3, or a cell whose remaining source is whitespace-only and which has no outputs. A `Block::Cell`
left with `show_input == false` and `outputs == []` is not emitted.

### 2.8 Raw cells → `Block::Raw` (format from `raw_mimetype()`, lowercased, parameters after `;` dropped)
| value | IR |
|---|---|
| `text/html`, `html` | `Raw { format: "html" }`; relative `src`/`poster` registered like Markdown raw HTML (`..` allowed), attachments §2.5 |
| `text/latex`, `text/x-latex`, `text/tex`, `latex` | `Raw { format: "latex" }` |
| `text/x-typst`, `text/typst`, `application/x-typst`, `typst` | `Raw { format: "typst" }` |
| `openxml` | `Raw { format: "openxml" }` |
| `text/markdown`, `text/x-markdown`, `markdown` | parsed exactly like a markdown cell (§2.3–2.6; never front matter) |
| `text/plain` | `CodeBlock { lang: None, text, attrs: { classes: ["raw-cell"] } }` |
| absent, `text/restructuredtext`, `text/x-rst`, `rst`, `text/asciidoc`, `text/x-python`, `application/pdf`, anything else | nothing; warning `raw-cell-skipped` naming the value (absent: "set raw_mimetype to text/html, text/latex, text/markdown or text/x-typst") |

`Raw.text` is the cell source verbatim; span `cell.span(index)`. Emitters skip foreign formats silently
(ir.md §1); `check` reports them per target as info `raw-skipped`.

## 3. Cell options (`src/notebook/options.rs`, T18)

### 3.1 `#|` lines (quarto's grammar, R10 §5.1; only when `[notebook] cell_options = true`)
```rust
pub struct OptionLines { pub yaml: String, pub lines: usize, pub rest: String }
pub fn split_options(source: &str, language: &str) -> OptionLines;
pub fn comment_markers(language: &str) -> (&'static str, Option<&'static str>);   // (open, close)
```
- Scanning starts at line 1 and is contiguous: a line is an option line when it starts at column 0
  with the open marker, then optional whitespace, then `|`; the prefix up to and including the `|` and
  at most one following space (quarto's `\| ?`) is removed. For block-comment languages the trimmed line must also end with the
  close marker, which is removed. The first other line (code, blank, shebang, magic) ends the block.
  `yaml` = the remainders joined with `\n`; `lines` = number of option lines; `rest` = the source after
  them. `Cell.source = rest`.
- Markers by `Cell.language` lowercased (unknown → `#`): `#` for r, python, julia, bash, sh, powershell,
  stan, octave, awk, gawk, sed, perl, prql, ruby, coffee; `//` for scala, csharp, fsharp, cpp, cc, java,
  groovy, kotlin, js, javascript, typescript, d3, node, sass, scss, go, asy, dot, ojs, rust; `%` for
  matlab, tikz; `--` for sql, mysql, psql, lua, haskell; `!` for fortran, fortran95; `*` for stata;
  `/` for q; `⍝` for apl; `/*`…`*/` for c, css; `*`…`;` for sas; `(*`…`*)` for ocaml. This is
  quarto's `kLangCommentChars` plus three writer additions, `sh` (`#`), `javascript` and `typescript`
  (`//`): Jupyter kernels report these `language_info.name` values, which quarto maps to `#`.
- `yaml` that is empty or whitespace → no options. Otherwise it is parsed with `serde_norway` (YAML 1.2
  core); a parse error or a non-mapping → warning `cell-options-invalid` (span lines `1..=lines`), all
  options of the cell ignored, the lines still removed from `source`.

### 3.2 Options (keys as quarto spells them; per-cell `metadata.writer` takes the same keys as JSON)
| option | type | effect |
|---|---|---|
| `label` | string (label alphabet, else `label-invalid`, dropped) | §3.4 |
| `fig-cap`, `tbl-cap` | string or list of strings | captions, §3.4; each parsed with `markdown::parse_inlines` (`Fragment.cell` set) |
| `fig-subcap`, `tbl-subcap` | string, list of strings, or bool | panel sub-captions of a group (§3.4), each entry parsed like `fig-cap`; a string is a one-item list; `true`/`false` → none |
| `fig-alt` | string or list | `Image.alt` = `[Text]` of the k-th entry (a string applies to every image) |
| `fig-align`, `fig-pos` | string | copied into the figure's `attrs.pairs` under the same key; emitters may honour them (latex-output.md: `fig-pos` = placement) |
| `tbl-colwidths` | list of numbers | `ColumnSpec.width = w / sum(w)` for each parsed table of the cell; length ≠ column count → `cell-options-invalid`, ignored; `auto`/bool → ignored |
| `lst-label`, `lst-cap` | string | listing figure, §3.4; `lst-cap` parsed like `fig-cap` |
| `echo` | bool or `"fenced"` (= true) | input shown |
| `output` | bool or `"asis"` | outputs shown; `asis`: stdout streams and `text/plain` candidates are parsed with `parse_blocks` instead of becoming `CodeBlock`s (ANSI stripped first) |
| `include` | bool | `false` → cell omitted |
| `warning` / `error` | bool | `false` → `stderr` streams / `error` outputs dropped before conversion, before §4.1's stream merging |
| `classes` | string (space-separated) or list | `Cell.attrs.classes` |
| `tags` | string or list | merged into `Cell.tags` and tag effects (§3.3) |
| `code-line-numbers` | bool or string | `true` → class `numbered`; a string `"1-3\|5"` → class `numbered` + pair `highlight="1-3,5"` |
| `html-table-processing` | `"none"` | the table parser (§4.4) is skipped for this cell's outputs |
| `eval`, `cache`, `freeze`, `execute`, `kernel` | any | ignored silently (nothing is executed) |
| anything else (`code-fold`, `code-summary`, `code-overflow`, `layout*`, `column*`, `*-cap-location`, `panel`, `output-location`, `fig-link`, `fig-env`, `fig-scap`, `renderings`, `id`, unknown keys) | — | ignored; ONE warning `cell-option-unsupported` per cell naming them in source order |

A value of the wrong type → `cell-options-invalid` naming the key; that key is ignored, the others apply.
For every key, a value from `#|` lines wins over the same key in `metadata.writer`, key by key (a
wrong-typed `#|` value is ignored, so the `metadata.writer` value applies). `[notebook] cell_options =
false` disables only the `#|` lines (they stay in `source`); `metadata.writer` still applies.

### 3.3 Tags, `jupyter.*_hidden` and precedence
| effect | sources, highest first |
|---|---|
| cell omitted | `include` (`#|`, then `metadata.writer`); else a tag in `[notebook] remove_cell_tags` |
| input hidden (`echo: false`) | `echo` (`#|`, then `metadata.writer`); else a tag in `hide_input_tags` or `jupyter.source_hidden` |
| outputs hidden (`output: false`) | `output` (`#|`, then `metadata.writer`); else a tag in `hide_output_tags` or `jupyter.outputs_hidden` |

The first source that sets an effect decides it (`#| echo: true` shows the input of a cell tagged
`hide-input`). Tags match configured names after replacing `_` by `-` on both sides (`remove_cell` ≡
`remove-cell`). `hide-*` and `remove-*` are the same in a static document (no reveal buttons). Raw and
markdown cells honour `remove_cell_tags` only. Defaults: config.md `[notebook]`.

### 3.4 Captions and labels (binding)
Definitions: an output is **figure-captionable** when it is `display_data`/`execute_result` and has an
image candidate (§4.3); **table-captionable** when it has a parsed-table candidate; if both, the kind of
the earlier candidate counts. The **target kind** of `label` is figure for a `fig-` or `fig:` prefix,
table for `tbl-` or `tbl:`, otherwise figure if the cell has a figure-captionable output, else table.
If the target kind has no captionable output but the other kind does, the label goes to the other
kind with the warning `label-kind-mismatch`. Let N be the number of captionable outputs of a kind, `cap` its `*-cap` option.

| case (per kind) | IR |
|---|---|
| N = 1, and `cap` is a string or the `label` targets the kind | that output's candidates become ONE candidate (its first captionable one): `Figure { kind: Image\|Table, caption: cap or None, attrs: { id: the label if it targets the kind, pairs: fig-align/fig-pos }, body: [Paragraph [Image]] \| [Table] }`; a truncation note paragraph (§4.4) stays after the figure |
| N ≥ 2, and `cap` is a string or the `label` targets the kind | ONE container `Figure { kind: Other, caption: cap or None, attrs: { id: the label if it targets the kind, pairs: fig-align/fig-pos }, body: [outputs-only Block::Cell] }` (§2.7 (b)); each captionable output becomes one candidate holding its panel: `Paragraph [Image]` (or the `Table`), then `Paragraph [Span { attrs.classes: ["subcaption"], inlines: subcap_k }]` when `*-subcap` has a k-th entry, then a table's truncation note paragraph. Panels are not figures: no number, no (a)/(b) |
| `cap` a list | N independent captioned outputs as in row 1: output k gets `cap[k-1]` (none when the list is shorter) and, with a `label`, id `<label>-<k>` (k 1-based among captionable outputs of the kind); more captions than outputs → `cell-options-invalid` |
| neither `cap` nor a `label` for the kind | no wrapper: images are `Paragraph [Image]`, tables bare `Table` (unnumbered) |
| a `label` but no captionable output at all, outputs shown | `Cell.attrs.id = label` (an anchor; resolve treats it per references.md); with hidden outputs the label is dropped (§7.3) |

- A cell has at most one group container. When both kinds qualify for row 2, the kind the `label`
  targets forms the group, else the kind of the cell's first captionable output; each output of the
  other kind is treated as in row 4 (no wrapper), with `cell-options-invalid` naming that kind's
  `*-cap`. The group's outputs-only `Block::Cell` holds every shown output of the cell in stored
  order, non-captionable ones (streams, `text/plain` results) included.
- `lst-label`/`lst-cap` with a shown input: `Figure { kind: Listing, caption: lst-cap, attrs.id:
  lst-label, body: [CodeBlock { lang: language, text: source, attrs: Cell.attrs minus id }] }` is emitted
  before the cell, and the cell gets `show_input = false`. With a hidden input: `cell-options-invalid`,
  ignored.
- `fig-subcap`/`tbl-subcap` outside the container case → `cell-option-unsupported`. A table's own HTML
  `<caption>` (§4.4) captions it only when no `tbl-cap`/`label` rule applies to it (then `Figure {
  kind: Table, caption: [Text], id: None }`); otherwise it is dropped.
- `Image.alt`: `fig-alt` if given, else the caption inlines (a panel: its sub-caption), else empty.

## 4. Outputs (`src/notebook/outputs.rs`, `tables.rs`, T18)

### 4.1 Streams and errors (lifted)
Consecutive `stream` outputs with the same `name` are merged; then nbconvert's carriage-return rule
(`apply_carriage_returns`: per line, everything up to and including the last `\r` that is followed by
more text of that line is dropped); then one trailing `\n` is removed; an empty result is dropped.
`stream` → `OutputKind::Stream { name }`, one candidate `{ mime: "text/x-ansi", blocks: [CodeBlock {
lang: None, text, attrs.classes: ["output-stream-stderr"] for stderr, else ["output-stream-stdout"] }] }`.
`error` → `OutputKind::Error { ename, evalue }`, text = `traceback.join("\n")` (empty traceback →
`"<ename>: <evalue>"`), class `output-error`, mime `text/x-ansi`; both have `CellOutput.mimes: []`. ANSI is §5 for both (and for
`text/plain`): `CodeBlock.text` is always plain. With `output: asis` (§3.2) a stdout stream's blocks
are `parse_blocks` of its stripped text instead.

### 4.2 Candidates, `CellOutput::pick` and the last-candidate guarantee
- `display_data` → `OutputKind::Display`, `execute_result` → `OutputKind::Result` with
  `CellOutput.execution_count`. `mimes` = every bundle key (sorted). Candidates are built for the bundle
  keys that appear in `[notebook] mime_priority`, **in that list's order**; a key whose candidate cannot
  be built (§4.3) raises `output-invalid` (`output <j>: <mime>: <reason>`) and is skipped. Keys not in
  the list are never candidates.
- **Guarantee:** if the last candidate contains a `Block::Raw` anywhere in its block tree, the frontend
  appends `OutputCandidate { mime: "", blocks: vec![] }` (renders nothing). So the last candidate of
  every output is renderable by every emitter. No candidate at all → `candidates: []` and
  `output-unsupported`.
- `pick` (src/ir.rs, T01): target formats `html → {html}`, `typst → {typst}`, `docx → {openxml}`,
  `latex → {latex}`; returns the blocks of the first candidate in which every `Block::Raw` (at any depth;
  `Inline::Raw` does not count) has a format of the target; `&[]` when none qualifies or `candidates` is
  empty. Emitters render `cell.outputs[j].pick(target)` and nothing else of an output.

### 4.3 Per-MIME candidates (`j` = the output's index in `Cell.outputs`, §6 for images)
| MIME | candidate blocks | fails (→ `output-invalid`) when |
|---|---|---|
| `image/png`, `image/jpeg`, `image/gif`, `image/webp` | `Paragraph [Image { alt: [], url: "asset:<id>", title: None, attrs.pairs: width/height (§6) }]` | payload not text, base64 empty or invalid |
| `image/svg+xml` | the same, bytes = the UTF-8 SVG text, MIME `image/svg+xml` | payload not text or has no `<svg` |
| `text/html` | `html-table-processing` ≠ `none` and §4.4 accepts it → `[Table]` (+ `Paragraph [Text note]` when there is a truncation note); else `[Raw { format: "html", text }]` with local files per §4.5; `<script` (ASCII case-insensitive) → warning `output-script` | payload not text |
| `text/markdown` | `parse_blocks` (§2.3's fragment, files per §4.5, `:::include` per §2.6) | payload not text |
| `text/latex` | `strip_math_delimiters` (lifted: one `$$…$$`, `\[…\]`, `\(…\)` or `$…$` pair around non-blank text with no inner unescaped `$$`/`$`) stripped a pair → `[Math { source, dialect: Latex, attrs: {} }]`; otherwise `[Raw { format: "latex", text: payload }]` | payload not text |
| `application/json`, `^application/(.*\+)?json$` listed by the user | `[CodeBlock { lang: Some("json"), text: serde_json::to_string_pretty, attrs.classes: ["output-json"] }]` | never |
| `text/plain` | `[CodeBlock { lang: None, text, attrs.classes: ["output-text"] }]` (§5; `output: asis` → markdown blocks) | payload not text |
| any other MIME listed in `mime_priority` | — | always ("no renderer") |

Interactive bundles: when a bundle has a key `application/vnd.jupyter.widget-view+json`,
`application/vnd.plotly.v1+json`, `application/javascript`, or starting with `application/vnd.vega` or
`application/vnd.bokehjs`, the output gets one warning `output-unsupported` ("output <j>: interactive
<mime> rendered as <mime of the first candidate>"; with no candidate only §4.2's
`output-unsupported` is raised); the other candidates render as usual. plotly: a `text/html`
candidate is `Raw` (HTML, with `output-script`); other targets render `text/plain` when the bundle has
it, otherwise the empty fallback, which `check --target` reports as `output-unsupported` (§8). `fig.show()` and a bare figure
as a cell's last value (`_ipython_display_`, so no `execute_result`) write one `display_data`: in a
Jupyter kernel with plotly ≥ 6.1 (renderer `plotly_mimetype`) only `application/vnd.plotly.v1+json`,
so no candidate at all; with plotly ≤ 6.0 (`plotly_mimetype+notebook`) also `text/html` (read from
plotly's `_renderers.py` at v6.0.0/v6.1.0/v7.1.0 and IPython's `DisplayFormatter.format`, not run).

### 4.4 `text/html` table parser (`tables.rs`)
```rust
pub struct HtmlTable { pub table: Block /* Block::Table */, pub caption: Option<String>, pub note: Option<String> }
pub fn parse_html_table(html: &str) -> Option<HtmlTable>;   // None = not a plain table
```
`scraper::Html::parse_fragment(html)`. Accept only when **all** hold:
1. The significant children of `root_element()` (elements; comments and whitespace-only text skipped;
   any other text rejects) are, after removing every `<style>`, exactly `table`, or `table` + `p`, or one
   `div` whose significant children satisfy the same rule (pandas' `_repr_html_`). The `p` must read
   `^\d+ rows × \d+ columns$` (trimmed) → `note`.
2. The table contains no descendant `<table>`; its children are only `caption`, `colgroup`, `thead`,
   `tbody`, `tfoot` (html5ever inserts `tbody` for bare `tr`); rows hold only `th`/`td`.
3. Cell content is text plus the elements `br p div b strong i em code a span sub sup`; anything else
   (`img`, `svg`, `script`, `style`, `input`, an unknown element from unescaped `<`) rejects.
4. At least one row.

Mapping: `thead` rows → `head`, `tbody` rows → `body`, `tfoot` rows → `foot`; without `thead`, a first
body row made only of `th` cells moves to `head`. Cell → `TableCell { header: tag == "th", colspan,
rowspan, align: None, inlines }`; spans parse as `u32 ≥ 1`, else 1. Inlines: text with runs of ASCII
whitespace collapsed to one space and the cell trimmed with `char::is_whitespace` (so a Styler
`&nbsp;` cell is empty; inner U+00A0 stays); `b`/`strong` → `Strong`, `i`/`em` → `Emph`, `code` →
`Code`, `sub`/`sup` → `Subscript`/`Superscript`, `a` → `Link { url: href as written, title }`, `span` →
its children, `br` → `LineBreak`, `p`/`div` → their content followed by `LineBreak` unless last.
Entities arrive decoded. Attributes (`style`, `class`, `id`, `halign`, `valign`, `border`) are dropped.
Columns: lay rows on a grid (a cell takes the first free slot of its row; a rowspan occupies the slots
below); `columns.len()` = the widest row's slot count; rows with fewer slots get empty cells appended
(`header` as the row's last cell) so every row covers every column counting cells that span into it
from above (ir.md §2 invariant 1 as amended, see Deviations); `ColumnSpec { align: Default, width:
None }` unless `tbl-colwidths`.
`caption` = trimmed text of `<caption>`. Verified shapes (probe, pandas 3.1.0): simple, `_repr_html_`
with `<style scoped>`, MultiIndex (3 head rows, `colspan=2`, `rowspan=2`, 5 columns), truncated (note
"100 rows × 2 columns"), Styler (caption "Styled caption"); an unescaped `b<c` Styler cell rejects.

> **Contract change (coordinator, 2026-10-27):** a table whose expanded grid
> exceeds 1,000,000 slots, overflows checked arithmetic, or has overlapping
> spans falls back to raw HTML. This bounds allocation for malformed spans
> while preserving the existing candidate fallback mechanism and public API.

### 4.5 Local files of outputs (lifted, presenter audit F08)
Relative `src`/`poster` values of `text/html` outputs and the images and raw HTML of `text/markdown`
outputs resolve against the notebook's directory, but only files **inside** it (canonical paths,
symlinks resolved) become assets. T18 parses such content into a scratch `AssetRegistry`; file assets
outside → not merged, and their `asset:<id>` placeholder (in `Image`/`Link` URLs and inside `Raw` html
attribute values) is replaced by the file's lexical path relative to the notebook's directory (what was
written for relative URLs), with the warning `output-file-outside` naming it; missing files →
`missing-asset` (escalated by `build`, ir.md §4). Every output warning has the span of the whole cell
and a message starting `output <j>: `.

### 4.6 Numbered content in outputs (binding, keeps numbers equal across targets)
A candidate is *numbered* when its block tree contains a `Heading`, `Figure` or `Math` block, any
block or inline with a non-empty `attrs.id` (references.md registers labels from `candidates[0]`
only), or an `Inline::Footnote`, `FootnoteRef`, `Ref` or `Cite`. When any candidate of an output is
numbered, the frontend replaces the output's candidates by the single **first candidate without `Block::Raw`** (it
exists by §4.2; it may be the empty fallback). Hence every numbered or referencing block in outputs
lives in an output with exactly one candidate that every emitter picks, and resolve may walk all
candidates of all outputs. Captioned outputs (§3.4) are already single-candidate.

## 5. ANSI escapes (`src/notebook/ansi.rs`, T18; crate `anstyle-parse` 1.0.0 + types from `anstyle` 1.0.14)
```rust
pub struct AnsiSpan { pub text: String, pub style: anstyle::Style }
pub fn parse(text: &str) -> Vec<AnsiSpan>;            // adjacent equal styles merged
pub fn strip(text: &str) -> String;                   // concatenated span texts
pub fn canonical(spans: &[AnsiSpan]) -> String;       // plain text, or `style.render()` + text + `style.render_reset()` per styled span
pub fn code_block(text: &str, class: &str) -> Block;  // CodeBlock { lang: None, text: strip(text), attrs: classes [class], pairs {"ansi": canonical} only if any span is styled }
/// (fg, bg) as rendered: the bold→intense rule, then the invert swap; `None` = default colour.
pub fn colors(style: &anstyle::Style) -> (Option<anstyle::Color>, Option<anstyle::Color>);
pub fn to_html(spans: &[AnsiSpan]) -> Vec<String>;    // one HTML string per line (html-output.md §1.5 LINES); the HTML arm
pub fn rgb(color: anstyle::Color) -> (u8, u8, u8);    // the DOCX arm
pub const PALETTE: [(u8, u8, u8); 16];
```
- `parse` feeds every byte to `anstyle_parse::Parser::<anstyle_parse::DefaultCharAccumulator>::new()`
  with a `Perform` whose `print` appends the char, `execute` keeps only `\n` and `\t`, and
  `csi_dispatch(params, intermediates, ignore, action)` applies SGR only when `action == b'm'`,
  `intermediates` is empty (private `ESC[>…m` dropped) and `!ignore`. Every other sequence (cursor
  movement, erase, OSC 8 hyperlinks — their text stays —, DCS, charset escapes) and every other control
  byte is dropped. The result never contains U+001B.
- SGR (each `&[u16]` group of `Params`; the parser pushes the pending parameter before dispatch, so
  `ESC[m` arrives as one group `[0]` and `ESC[;1m` as `[0], [1]`): `0` reset; `1`, `5`, `6` bold; `2` dim;
  `3` italic; `4` underline (colon form `4:0` = off); `7` invert; `8` hidden; `9` strikethrough; `21`,
  `22` bold and dim off; `23` italic off; `24` underline off; `27` invert off; `28` hidden off; `29`
  strikethrough off; `30–37`/`90–97` fg palette 0–7/8–15; `40–47`/`100–107` bg palette; `39`/`49`
  default; `38`/`48` extended: `;5;n` or colon `:5:n` → `Ansi256(n)`, `;2;r;g;b` or `:2:r:g:b` or
  `:2:cs:r:g:b` → `Rgb`; a malformed extended colour or any other code is ignored.
- The **IR form**: `CodeBlock.text` is plain; the private pair `ansi` (present only when styled) holds
  `canonical(parse(raw))`, for which `strip(pair) == text` and `parse(canonical(s)) == s` (tests).
  Emitters never print the `ansi` pair as an attribute. typst and LaTeX use `text` only.
- Colours: `PALETTE` = 0 `3E424D`, 1 `E75C58`, 2 `00A250`, 3 `DDB62B`, 4 `208FFB`, 5 `D160C4`,
  6 `60C6C8`, 7 `C5C1B4`, 8 `282C36`, 9 `B22B31`, 10 `007427`, 11 `B27D12`, 12 `0065CA`, 13 `A03196`,
  14 `258F8F`, 15 `A1A6B2`. `Ansi256(n)`: n < 16 → palette; 16–231 → m = n − 16, components (m / 36,
  m / 6 % 6, m % 6), each k → 0 if k = 0 else 55 + 40k; 232–255 → grey (n − 232)·10 + 8. *Palette
  entries* are `Ansi(c)` (0–15) and `Ansi256(n)` with n < 16 (nbconvert's `_get_extended_color`).
- `colors` (nbconvert `_ansi2anything`/`_htmlconverter`): a bold span whose fg is palette 0–7 renders
  with palette entry fg + 8 (bg unchanged; the `ansi` pair keeps the parsed colour); then, with
  `INVERT`, fg and bg swap.
- `to_html`: the spans are split into lines exactly as html-output.md §1.5's `LINES` (one trailing
  `\n` dropped, `\r\n` read as `\n`); per line each segment's text is escaped with html-output.md
  §2.1's `escape`; a plain segment is bare text; a styled one is `<span class="…" style="…">…</span>`.
  fg/bg from `colors`: palette entries → classes `ansi-<name>-fg`/`-bg` (0–7) and
  `ansi-<name>-intense-fg`/`-bg` (8–15) with names black, red, green, yellow, blue, magenta, cyan,
  white; other colours → `color: rgb(R,G,B)` / `background-color: rgb(R,G,B)` (decimal components, no
  spaces inside `rgb(…)`), in that order joined by `; ` (nbconvert's exact form); with `INVERT`, a
  default fg after the swap becomes the class `ansi-default-inverse-fg` and a default bg
  `ansi-default-inverse-bg` (theme CSS: `.ansi-default-inverse-fg { color: <page background> }`,
  `.ansi-default-inverse-bg { background-color: <text colour> }`); effects → `ansi-bold`, `ansi-dim`,
  `ansi-italic`, `ansi-underline`, `ansi-strike`, `ansi-hidden`. Class order: fg, bg, effects in that
  order. Empty `class`/`style` attributes are omitted. Examples: `ESC[1;31mE` → `<span
  class="ansi-red-intense-fg ansi-bold">E</span>`; `ESC[31;7mX` → `<span class="ansi-default-inverse-fg
  ansi-red-bg">X</span>`; `ESC[38;5;208;48;2;1;2;3mY` → `<span style="color: rgb(255,135,0);
  background-color: rgb(1,2,3)">Y</span>`.
- DOCX (spelled out here; docx-output.md §4.6's paragraph and line rules apply): each segment of a
  line is one run whose `w:rPr` holds, in docx-output.md's `RPR` order, `<w:rStyle w:val="{Verbatim
  Char}"/>`, `<w:b/><w:bCs/>` (bold), `<w:i/><w:iCs/>` (italic), `<w:strike/>`, `<w:color
  w:val="RRGGBB"/>` (fg), `<w:u w:val="single"/>` (underline), `<w:shd w:val="clear" w:color="auto"
  w:fill="RRGGBB"/>` (bg); colours from `colors` then `rgb`, uppercase hex; a default colour writes
  nothing, except with `INVERT`, where a default fg after the swap is `FFFFFF` and a default bg
  `000000`; dim and hidden are ignored.
- **Ownership** (`ansi.rs` lands with T18 in wave 4, after T13 and T40): T13 and T40 render
  `CodeBlock.text` only and treat `ansi` as a consumed pair (never printed). T18 adds the `ansi`
  branch additively, one match arm each, named in its PR: in `src/html/emit.rs` the lines of
  `to_html(&parse(pair))` replace the escaped lines inside html-output.md §1.5's `<span
  class="line">` (output code blocks have no `lang`, so nothing is highlighted); in
  `src/docx/body.rs` the runs above replace the Verbatim Char runs. T18 owns the U+001B test for
  HTML, DOCX and typst in `tests/notebook_snapshots.rs`; T60 adds LaTeX's (§9). Contract change: see
  Deviations.

## 6. Images from outputs (`outputs.rs`, T18)
- Base64 payloads (string or list): ASCII whitespace removed, `base64::engine::general_purpose::
  STANDARD_PAD_INDIFFERENT.decode` (lifted). Bytes go to `assets.register_bytes(bytes, mime, hint)`;
  the id is content-addressed (the same PNG twice registers once; the first hint stays).
- `hint = "<stem>_cell<index>_out<j>.<ext>"`: `stem` = the notebook file stem with every char outside
  `[A-Za-z0-9._-]` replaced by `_`; `index` = the cell index (§1); `j` = the output's index in
  `Cell.outputs`; `ext` = `png`, `jpg`, `gif`, `webp`, `svg`. Attachments: §2.5.
- Size: `dimension(metadata, mime, key)` (lifted: `metadata[mime][key]`, else `metadata[key]`;
  non-negative finite numbers only, printed without a fraction when integral) for `width` and `height`
  → `Image.attrs.pairs` as bare numbers (= px, markdown-dialect.md §4.3 grammar). Intrinsic sizes,
  DPI and renditions (SVG/GIF/WebP → PNG for DOCX, → PDF for LaTeX via `typst::render::image`) are
  the emitters' (docx-output.md, latex-output.md).

## 7. Notebooks as includes

### 7.1 From Markdown (`:::include analysis.ipynb`, T1B)
markdown-dialect.md §8 applies: the whole notebook (its own `remove-cell` etc. honoured) is parsed by
`notebook::parse_with` with `nested: true`, the including document's effective config (the notebook's
own `metadata.writer`/front-matter config tables apply to its parsing), and `cell_includes_in(nb,
&chain)` (§2.6) continuing the include chain. Its document metadata is not applied (`include-meta-ignored` for keys that differ);
its blocks replace the directive; every span gets `file`; `Document.includes` lists it. There is no
cell selection in Markdown (the directive's info is only the path); select with tags or §7.3.

### 7.2 From markdown cells
§2.6. A notebook may include `.md` and `.ipynb` files; depth and cycles count across both kinds.

### 7.3 From typst (`#include "x.ipynb"`, `#include-notebook(…)`, T53)
This section is authoritative for `include-notebook` and for what the World serves for a `.ipynb`;
typst-dialect.md §2.4, §2.7, §4.4 and §6.4 are aligned to it (see Deviations).
- **Selection** (`Selection`, shared by both paths below): `cells` — `None` = all; `CellRef::Index(i)` =
  the cell with index i; `CellRef::Id(s)` = the cell whose nbformat `id` is s; several refs = their union
  in document order. `tags` — cells having at least one of the tags. Both given → intersection. A ref
  naming no cell, or a cell of `Unknown` type or removed by the notebook itself (§3.3), matches nothing:
  error diagnostic `notebook-cell-not-found`, the ref is ignored (the PDF panics on it, as on an
  absent cell). Cells removed by the notebook itself stay removed. `hide_input`/`hide_output` = true
  act on every selected code cell as if set by `#|` options (a hidden input also drops its listing
  figure; hidden outputs drop their
  captions, labels and group container); `false` keeps the notebook's own settings. The structured
  path selects after §2.4's definitions are collected and before its cross-cell pass (§2.1 steps
  4–6), so a selected cell still gets a definition that lives in an unselected cell.
- **Structured exports** (HTML/DOCX/LaTeX from a `.typ`; T50's walker, T53's arm): `#include "x.ipynb"`
  and `#include-notebook(path("x.ipynb"), cells: …, tags: …, hide-input: …, hide-output: …)` with
  literal arguments splice the blocks of `notebook::parse_with(source, json, &effective_config,
  ParseOptions { includes: &mut *cell_includes_in(source, &chain), nested: true, selection })` like
  `:::include`, `chain` = the canonical paths of the `.typ` files from the main file to the including
  one; `#include` uses `Selection::default()`. Never `notebook::parse`: it inserts a `Toc` and
  restarts the include chain, so typst-dialect.md §2.7's depth limit would not count the levels above
  the notebook. Typst values: `cells: auto | int | str | array of int/str`, `tags: none | str | array
  of str`, `hide-input`/`hide-output: auto | bool` (`auto`/`false` → `false`). A string `file` is
  accepted with `W-TYP-PATH-STRING` (the PDF panics on it); a non-literal `file` →
  `E-TYP-INCLUDE-DYNAMIC`; any other non-literal or wrong-shaped argument → `E-TYP-ARG-INVALID`
  (typst-dialect.md §2.8).
- **PDF** (the compiler): the World answers `source(id)` for a project file whose path ends in `.ipynb`
  with a generated module (`World::file(id)` still returns the raw bytes) and records the access (watch
  set). This module is the `text` that typst-dialect.md §6.4's `IncludeProvider::synthesize` returns
  for a `.ipynb` (`.md` keeps §6.4's fragment). T53's `src/typst/includes.rs` builds it from
  `notebook::parse_cells(…, nested: true, Selection::default())` with `includes: &mut
  *cell_includes_in(nb, &[])` (`synthesize` sees no chain; typst rejects cyclic includes itself).
  `Parsed.document` stays unresolved (no resolve, typst-dialect.md §6.4: resolve would turn every
  `Ref`/`Cite` aimed at the including `.typ` into a `[?x]` placeholder, references.md §4.7, and invent
  auto ids that collide with the main document's labels). With `nested: true` no `Toc` is inserted, so
  `document.blocks` is exactly the parts' `body` concatenated (§2.1) and each part's run is the next
  `body.len()` blocks. The bodies come from one `typst::emit_split` over that unresolved document with
  one `IncludeRun` per part (main text discarded); `input` and `outputs` from two more calls on copies
  whose code-cell runs hold those variants. `EmitOptions`: `fragment: true`, `AssetPaths::Virtual`, the
  include's `footnote_prefix`, so every `Ref` and `Cite` is written syntactically for typst to resolve
  in the whole document (typst-dialect.md §5.6), and every `Footnote`/`FootnoteRef` keeps its label: a
  `FootnoteRef` takes the `flabel` of the `Footnote` with the same label in the same scope
  (typst-dialect.md §5.6's fragment rule), also when that `Footnote` is in another part, and the
  `input`/`outputs` variants need no footnote numbers. The parts' import lines are collected, deduplicated in first-seen order and written once after the header comment
  (`@writer/doc` always; mitex when any part has LaTeX math, since `#mi`/`#mitex` resolve only
  through it, typst-dialect.md §4.1); the parts follow without imports. The module's `SourceMap` holds
  `None` for header, import and wrapper lines and the parts' maps for their lines, built with
  T03's `SourceMap::from_lines(Vec<Option<SourceSpan>>)` (`src/typst/sourcemap.rs`,
  typst-dialect.md §5.1, PLAN §3.2; T53 only calls it). The notebook's assets are added to the World's `/__assets/` store.
  Generated text (deterministic; only what the World serves: `--to typst` materialises a notebook
  include as typst-dialect.md §10's `emit_split` part of the resolved document, not as this module):
  ```typst
  // Generated by writer from analysis.ipynb; edits are lost.
  #import "@writer/doc:<version>": *
  #import "@preview/mitex:<MITEX_VERSION>": mi, mitex   // only if a part has LaTeX math
  #let writer-cells = (
    (index: 0, id: "intro", tags: (), kind: "markdown", body: [ …blocks… ]),
    (index: 2, id: none, tags: ("paper",), kind: "code", body: [ … ], input: [ … ], outputs: [ … ]),
  )
  #for c in writer-cells { c.body }
  ```
  `body` = the cell's normal blocks; for code cells `input` = its blocks with outputs hidden and
  `outputs` = its blocks with the input hidden (`none` when empty); `kind` is `"markdown"`, `"code"` or
  `"raw"`. Parts follow the whole-notebook footnote pass, so a selection that keeps a later reference
  to a shared footnote but drops the cell of its first reference does not compile (typst names the
  missing `<writer-fn-<prefix>…>` label, typst-dialect.md §5.6) while the structured path renders it:
  the one case where the paths differ.
  `include-notebook(file, cells: auto, tags: none, hide-input: auto, hide-output: auto)` (T53, additive
  to `cells.typ`): `file` must be a typst `path` value (a string would resolve inside the package; it
  panics with "pass path(\"x.ipynb\")"); it imports `writer-cells` from `file`, applies the selection
  with the same semantics (unknown index/id → panic naming it), and shows per code cell `outputs` when
  hide-input, `input` when hide-output, nothing when both, else `body`; other cells show `body`.
  Verified (probe, typst 0.15.1, a World whose `source()` returns such a module for `x.ipynb` while
  `file()` returns the JSON): `import file: writer-cells` inside a package function resolves
  `path("x.ipynb")` against the calling file; `cells: (1, "plot"), hide-input: true` shows only the
  selected outputs and `@fig-plot` resolves to a label inside them; `tags: "paper"` filters; a string
  `file` and an unknown index panic, reported at the user's call.

## 8. Diagnostics and `check` (cli.md's notebook lint list)
All carry the notebook path; cell-level ones the span of the whole cell unless stated. Levels per ir.md §5.

| code | level | raised when (§) |
|---|---|---|
| `nbformat-newer`, `notebook-output-skipped`, `notebook-attachment-skipped`, `unknown-cell-type`, `writer-not-object` | warning | §1 (`notebook-attachment-skipped` also §2.5) |
| `notebook-presenter-metadata` | info | §2.2 |
| `notebook-front-matter-ignored` | warning | §2.2; `front-matter-empty` (markdown-dialect.md) reused |
| `footnote-undefined`, `footnote-duplicate` | warning | §2.4; `footnote-unreferenced` reused, raised once per notebook definition |
| `missing-attachment` | warning | §2.5 |
| `raw-cell-skipped` | warning | §2.8 |
| `cell-options-invalid` | warning | §3.1, §3.2, §3.4 (span: the option lines, or the cell) |
| `cell-option-unsupported` | warning | §3.2, one per cell |
| `label-kind-mismatch` | warning | §3.4; `label-invalid` (error) reused |
| `output-invalid`, `output-unsupported`, `output-script`, `output-file-outside`, `output-include` | warning | §4.2–§4.5, §2.6; `missing-asset` reused |
| `notebook-not-run` | warning, no span | §2.1: at least one kept code cell has non-whitespace source and no code cell has outputs or an `execution_count` |
| `notebook-cell-not-found` | error | §7.3 |
| `parse-error` (`Error::Parse`/`Error::FrontMatter`) | error | §1, §2.2, §2.6 |

`check` (T1C) runs the frontend and reports all of the above. Per target (T71, `--target`): an output
whose `pick(target)` is the empty fallback or `[]` while the output has candidates → `output-unsupported`
("output <j>: no rendering for docx (text/html only)"); `Raw` candidates and raw cells skipped for a
target → info `raw-skipped`. T1C snapshots `writer check` of every `tests/fixtures/ipynb/*.ipynb`
(codes per fixture).

## 9. Rendering per target (markup owned by html-output.md, docx-output.md, typst-dialect.md, latex-output.md)
| IR | HTML (T13) | DOCX (T40/T41) | typst (T30/T31) | LaTeX (T60) |
|---|---|---|---|---|
| `Block::Cell` frame | `div.cell` (+ `Cell.attrs` classes, `id`; `data-cell` = `Cell.index` always, plus the position attributes, per html-output.md), `div.cell-input`, `div.cell-outputs`, `div.cell-prompt` (presenter's theme vocabulary; html-output.md is authoritative) | input then outputs as consecutive blocks | the `code-cell` helper of `@writer/doc` (`cells.typ`, signature in typst-dialect.md) | blocks in order; with `[latex] cell_prompts` the input in `\begin{writercell}{In}{n}` and each `Result` output in `{Out}{n}` (empty n when none); streams, display and errors unwrapped |
| prompts | `In [n]:` / `Out [n]:` (Result only) when `[notebook] show_prompts` | same, own paragraph | never (config.md: `show_prompts` is HTML and DOCX; `code-cell` prints none, see Deviations) | `writercell` only |
| input | `pre.hl-code > code.language-<lang>`, highlighted, `CodeAttrs::from_attrs(&cell.attrs)` | code paragraphs with highlighted runs | `raw(block: true, lang: "<lang>")` | per `[latex] code` |
| `show_input`/`show_output` false | not emitted (no CSS hiding) | not emitted | not emitted | not emitted |
| `CodeBlock` `output-text`/`output-stream-*`/`output-error`/`output-json` | `pre` with that class; lines from `to_html` of the `ansi` pair (T18's arm, §5), else escaped `text`; `ansi` never printed | code style; ANSI runs (T18's arm, §5), else Verbatim Char runs of `text` | `raw(block: true)` of `text` | `Verbatim` of `text` |
| `Span.subcaption` (group panels) | small (theme CSS) | plain | `text(size: 0.85em)` | `{\small …}` |

Outputs render `cell.outputs[j].pick(target)` (§4.2); figures (the group container is any `Figure {
kind: Other }`), tables, math and Markdown blocks in cells render as anywhere in the document.
Emitters never print U+001B (a test per emitter on `outputs.ipynb`: T18 for HTML, DOCX and typst in
`tests/notebook_snapshots.rs`, T60 for LaTeX).

## Fixtures
T18 owns `tests/fixtures/ipynb/` (PLAN §7), hand-written JSON kept small (real pandas/IPython payloads
from R10's samples), `tests/notebook_snapshots.rs` (IR JSON `--no-resolve` with ids and paths redacted,
plus a diagnostics snapshot per fixture):
- `minimal.ipynb` — one markdown and one code cell, never run (`notebook-not-run`), Python metadata.
- `outputs.ipynb` — a tiny PNG with `metadata.image/png.width`, an SVG with top-level width, the same
  PNG twice (one asset), invalid base64 + `text/plain`, `text/latex` (`$\displaystyle …$`, and two
  formulas → raw), `text/markdown` with an image inside the directory and one naming
  `../md/images/` (`output-file-outside`) and a missing one, `application/json`, a non-table
  `text/html` with `<script>` + `text/plain`, three plotly shapes (`display_data` with only
  `application/vnd.plotly.v1+json` as plotly ≥ 6.1 writes, with `text/html` too as plotly ≤ 6.0 writes,
  and one that also has `text/plain`), a widget bundle, merged streams with a `\r` progress bar, stderr, the IPython
  9.17.1 traceback (16/256-colour ANSI), a stream with `ESC[1;31m` (bold → intense), `ESC[31;7m` and
  `ESC[7m` (inverse), a bundle with only `text/html` `<div>` (empty fallback), `display_data` with only
  an unknown MIME, a `text/markdown` output holding `:::include x.md` (`output-include`), and a
  `text/markdown` `# Heading` + `text/plain` bundle (§4.6: one candidate).
- `options.ipynb` — `#|` in Python, R and C (`/*| … */`), a shebang before options, invalid YAML,
  wrong types, unsupported options, every row of §3.4 (N = 1, N = 2 + `fig-subcap`, list captions with
  `label-1/-2`, `tbl-cap` on a DataFrame, `tbl-colwidths`, a group of two tables, `lst-label` +
  `lst-cap`, `label-kind-mismatch`, label without outputs, a cell where both kinds qualify for a
  group, `label: fig:x`), `echo`, `output: false|asis`, `include: false`, `warning: false`,
  `error: false`, `code-line-numbers`, `#|` and `metadata.writer` both setting `fig-cap`,
  `html-table-processing: none` on a DataFrame.
- `tags.ipynb` — every default tag, underscore spellings, `jupyter.source_hidden`/`outputs_hidden`, a
  `#|` option overriding a tag, per-cell `metadata.writer`, a both-hidden cell, an `Unknown` cell type.
- `frontmatter.ipynb` — front matter in the first markdown cell (cell 1), `metadata.title`/`authors`,
  `metadata.presenter` (shared and ignored keys), `metadata.writer` with `toc: true`, `numbering` and an
  `abstract`, a later `---` block (`notebook-front-matter-ignored`), a later `---` holding only
  `Note: this matters` (Markdown) and a prose `---`, raw cells of every row of §2.8, a markdown
  attachment, an inline `<img src="attachment:…">`, an attachment offered only as `image/bmp`
  (`notebook-attachment-skipped`) and a missing one.
- `dataframe.ipynb` — pandas 3.1.0 `to_html`/`_repr_html_` (simple, MultiIndex, truncated rows and
  columns, NaN/NaT, Styler with caption, unescaped Styler → raw), a table without `thead`.
- `footnotes.ipynb` — reference in cell 1, definition in cell 3, one footnote referenced from two
  cells, an unreferenced and an undefined one, `^[inline]` in two cells (each its own `__inline_1`),
  a footnote with a missing image referenced from two cells (one `missing-asset`) and one with an
  `attachment:` image of its own cell, cells ending in an unclosed fence, an unclosed `:::note`, and a
  `:::note` holding an unclosed fence, each referencing another cell's footnote (`footnote-undefined`,
  code text unchanged), `[^Note]`/`[^note]` in two cells (one footnote), a duplicate definition
  (`footnote-duplicate`), a later cell opening with a `---` rule and a definition before a second
  `---`, a `text/markdown` output with its own `[^1]` beside a cell's `[^1]` (two footnotes), a `@fig-`
  reference across cells.

Added by other tasks (additive): T1B `ipynb/includes.ipynb` (`:::include` of `../md/include/part-a.md`
in a markdown cell, a cycle back to itself, an include in a list, a `[^1]` in the notebook while the
included file has its own `[^1]`: two footnotes, §2.4 step 4); T53 `typ/include-notebook.typ` +
`typ/include/cells.ipynb` (T53's own notebook: markdown cell 0 with `$x^2$`, code cell 1 with an
output, a code cell with nbformat id `plot` whose figure has `label: fig-plot`, a cell tagged
`paper`). The `.typ` holds `#include-notebook(path("include/cells.ipynb"), cells: (1, "plot"),
hide-input: true)` and `@fig-plot`; it is compiled to PDF and exported to HTML, and both must show the
selected outputs only. `typ/include-notebook-missing.typ` (T53) selects an unknown index:
`notebook-cell-not-found` in the HTML export, a panic naming it in the PDF. `typ/includes.typ`'s plain `#include "include/nb.ipynb"` is typst-dialect.md's.
`examples/notebook/analysis.ipynb` (T18) is the executed example, regenerated by a documented command.

## Verified API hints (2026-10-24/25; scratch probes against R10's samples, not kept)
Full signatures of scraper 0.29.0, anstyle-parse 1.0.0 and anstyle 1.0.14 are in R10 §13.1; the
`Cargo.toml` rows are vendored-assets.md §7.3 (see Deviations). What this spec relies on beyond them:
- scraper (`default-features = false`): `Html::parse_fragment(&str)`, `root_element()` (the `<html>`
  wrapper), `child_elements()`, `ElementRef::wrap`, `value().name()`/`attr(&str)`, `text()`; the probe
  implements §4.4 and reproduces the shapes listed there.
- anstyle-parse: `Parser::<DefaultCharAccumulator>::new()`, `advance(&mut performer, byte)`; `Perform`
  methods default to no-ops; `Params::iter()` yields `&[u16]` groups, colon sub-parameters in one group
  (`38:5:81` → `[38, 5, 81]`); `csi_dispatch` always gets at least one group (`self.params.push(self.param)`
  before it; `ESC[m` → `[[0]]`, so `Params::is_empty()` is never true there). Verified: IPython 9
  traceback → 68 spans, plain text without ESC, `parse(canonical(s)) == s`; `22`/`24`/`27` turn effects
  off; OSC 8, `ESC[2K`, `ESC[1A` dropped; `ESC[>4;2m` ignored.
- anstyle: `Style::new()`, `fg_color(Option<Color>)`, `bg_color`, `effects(Effects)`, `get_*()`,
  `is_plain()`, `render()` (`"\x1b[1m"`, `"\x1b[38;5;81m"`, `"\x1b[38;2;10;20;30m"`), `render_reset()`
  (`"\x1b[0m"`); `Color::{Ansi(AnsiColor), Ansi256(Ansi256Color(u8)), Rgb(RgbColor(u8,u8,u8))}`;
  `Effects::{BOLD, DIMMED, ITALIC, UNDERLINE, INVERT, HIDDEN, STRIKETHROUGH}`, `contains(self, Effects)
  -> bool`, `insert(self, Effects) -> Effects` and `remove(self, Effects) -> Effects` (by value,
  `#[must_use]`: `style = style.effects(style.get_effects().insert(Effects::BOLD))`; a bare
  `effects.insert(..);` changes nothing and fails `-D warnings`).
- base64 0.23.1: `general_purpose::STANDARD_PAD_INDIFFERENT.decode(&[u8])` (presenter).
- typst 0.15.1: `include`/`import` accept a `path` value, resolved relative to the file where it is
  written (`typst-eval-0.15.1/src/import.rs`, `foundations/path.rs`); a cycle is "cyclic import";
  `if type(file) != path { panic(..) }` catches a string (`nbinclude-probe`; rest in §7.3).
- comrak 0.57.0 (`parser/mod.rs`): `inline_footnotes` appends `__inline_N` definitions to the root (N
  from 1 per parse); with `parse.leave_footnote_definitions = true` (set by `comrak_options`)
  definitions stay where written and unreferenced ones keep `total_references == 0`, which §2.4 step 1
  relies on (only with the default `false` does `process_footnotes` move them to the end or detach
  them); `find_footnote_definitions` keys a map by `normalize_label(.., Case::Fold)`, so the last of
  repeated definitions wins and `find_footnote_references` renames each reference to its definition's
  spelling; `NodeCodeBlock.closed`; `NodeBlockDirective` has only `fence_length`, `fence_offset`, `info`.
- nbconvert 7 (`nbconvert/filters/ansi.py`, JupyterLab 4.6.4's environment): `_htmlconverter` swaps
  before choosing classes, writes `ansi-default-inverse-fg` for a default fg and `-bg` for a default
  bg, joins styles with `"; "`; `_ansi2anything` passes `fg + 8 if bold and fg in range(8)`; classic
  `style.css`: `.ansi-default-inverse-fg { color: #FFFFFF }`, `.ansi-default-inverse-bg {
  background-color: #000000 }`. nbformat's v4.2–v4.5 schemas (not v4.0/v4.1) define
  `metadata.title`/`authors`.
- presenter sources to lift: `src/notebook/nbformat.rs` (whole), `outputs.rs` (`apply_carriage_returns`,
  `strip_math_delimiters`, `unescaped_dollars`, `dimension`, `mime_text` use, stream merging,
  `split_outside`), `convert.rs` (`find_front_matter`, layered `deck_meta`/`merge_config`/`overlay`,
  `empty_front_matter` with §2.2's trigger, `sets_deck_metadata`, `has_any`, `decode_attachment`); not `ansi_to_html`,
  `svg_html` or the slide code.

## Open points (local choices)
- T18: the internal `CellOptions` struct and how options are validated (serde vs. hand-written);
  message wording; whether `tables.rs` builds IR inlines directly from scraper nodes or via an
  intermediate tree; whether the comrak scans of §2.4 and §2.6 share one parse per cell; how §2.4
  step 3 attributes a diagnostic to a definition (the appended text's line ranges suffice).
- T53: the exact typst code of `include-notebook` and of `writer-show-cell`-style helpers, given the
  module shape and semantics of §7.3.

## Deviations from PLAN.md and the brief
- Section order: cli.md (frozen) sends readers to "`notebook-mapping.md` §8" for the lint list, so §8
  is Diagnostics and the rendering table is §9.
- ir.md's `Cell.attrs` comment says the cell carries `#| label` when it is the labelled container of
  several captioned outputs; the brief's binding rule (one `Figure { kind: Other, label }` container)
  is used instead, and `Cell.attrs.id` is set only for a label with no captionable output and shown
  outputs (§3.4).
  The coordinator should update that comment. A group of N ≥ 2 tables is also `Figure { kind: Other }`
  (ir.md §2 invariant 2 allows exactly one `Table` in `Figure { kind: Table }`); its supplement follows
  references.md's rule for `Other`.
- The brief says "`#include`/`:::include` allowed in markdown cells": only `:::include` is Markdown;
  `#include "x"` in a markdown cell is text. `#include` of notebooks from typst is §7.3.
- Includes inside markdown cells need T1B's machinery, which lands after T18 (waves 5 vs 4): T18 adds
  the hook (`CellIncludes`, the `markdown::include::{cell_includes, cell_includes_in}` stubs, additive
  in T1B's file) and T1B fills it (§2.6).
- T18 adds match arms to T13's `src/html/emit.rs` and T40's `src/docx/body.rs` and tests T30's typst
  emitter (§5). Contract change: PLAN §5's T18 row depends on `T12, T13, T30, T40` (wave unchanged: 4).
- R10's per-target MIME priority lists are not used: config.md has one `[notebook] mime_priority`;
  per-target choice is `CellOutput::pick` over candidates in that order (§4.2), plus §4.6.
- ANSI styling travels in a private `ansi` pair so `CodeBlock.text` is always plain (ir.md lists no such
  pair; additive). The crate is `anstyle-parse` (R10 recommended its own SGR machine on presenter's
  tokenizer; the parser crate replaces the tokenizer, the SGR table of §5 is writer's).
- `include-notebook` takes a typst `path` value, not the string R2/the brief show, because typst 0.15
  resolves strings inside the package; the structured walker accepts both.
- **typst-dialect.md is aligned to §7.3** (both are T53's contracts; §7.3 keeps R2's `cells:`
  selection and is the only design whose PDF and structured exports hide listing figures and group
  containers alike). Contract change for the coordinator: typst-dialect.md §2.4's row becomes
  `include-notebook(path("x.ipynb"), cells:, tags:, hide-input:, hide-output:)` → the blocks of
  `notebook::parse_with(…, nested: true, selection)` (§7.3); §4.4's signature becomes
  `include-notebook(file, cells: auto, tags: none, hide-input: auto, hide-output: auto)` with §7.3's
  semantics, and its state mechanism is dropped (a state around `#include` hides only `code-cell`
  content, so listing figures and group containers would still be shown and numbered in the PDF);
  §2.7's `.ipynb` goes through `notebook::parse_with` with `nested: true` and `cell_includes_in(nb,
  &chain)`; §6.4's provider returns §7.3's `writer-cells` module for a `.ipynb` (built from
  `parse_cells`, unresolved, parts emitted by one `emit_split` with `fragment: true`, whose
  `FootnoteRef` → `flabel` lookup by label and scope spans all parts) instead of a single
  `fragment: true` emit and builds that module's map with §5.1's `SourceMap::from_lines` (T03; §5.1's
  comment calling it "T53, additive" is stale, PLAN §3.2/§5 give `sourcemap.rs` to T03); its fixture
  table gains `typ/include-notebook.typ`,
  `typ/include-notebook-missing.typ` + `typ/include/cells.ipynb` (T53).
- markdown-dialect.md §8 (frozen) says an included `.ipynb` goes through `notebook::parse`. T1B (and
  typst includes, §7.3) must call `notebook::parse_with(source, json, &effective_config, ParseOptions
  { includes: &mut *cell_includes_in(source, &chain), nested: true, selection: Selection::default()
  })` instead (§2.6, §7.1): `parse` inserts a `Toc` (§2.1 step 7) and restarts the include chain, so `a.md → b.ipynb →
  a.md` would recurse past the depth and cycle checks. Contract change: the coordinator updates that
  sentence of §8, and PLAN §5's T1B row depends on `T12, T16, T18` (T1B needs `ParseOptions` and
  `CellIncludes`; its wave stays 5).
- ir.md §2 invariant 1 counts only `colspan`; HTML tables with `rowspan` (every pandas MultiIndex
  frame) leave covered slots out of later rows, the model html-output.md §1.7 and docx-output.md §4.12
  already use. Contract change: invariant 1 becomes "every `Row` covers exactly `columns.len()` grid
  slots, counting its cells' `colspan` plus the slots covered by `rowspan`s of earlier rows of the same
  section (head/body/foot)", T01's invariant checker uses this grid rule, and latex-output.md §4.6
  (which ignores `rowspan` and relies on complete rows) writes an empty cell at each covered slot.
- ANSI rendering crosses task boundaries (§5 Ownership). Contract change: html-output.md §1.5 and
  docx-output.md §4.6 list `ansi` among the consumed pairs and cite §5 for the ANSI lines and runs;
  html-output.md's `MARKUP_CLASSES` and the default theme CSS gain §5's `ansi-*` classes (`PALETTE`
  colours, `ansi-default-inverse-fg`/`-bg`, the six effect classes).
- config.md (frozen) limits `[notebook] show_prompts` to HTML and DOCX, so typst never shows prompts
  (§9). Contract change: typst-dialect.md §4.4's `code-cell` prints no prompts; §5.3 no longer passes
  `notebook: (show-prompts: ..)`, §3's table no longer uses `notebook.show-prompts`, and §6.2's
  `writer-config` drops `show_prompts`.
- Contract change for vendored-assets.md §7.3 (T00's `Cargo.toml` table; no other task edits it): the
  scraper row becomes `=0.29.0`, features `df` (drops `main`/getopts and `errors`); the anstyle row
  becomes `=1.0.14`, for "notebook ANSI spans (T18; anstyle-parse VTE parser + writer's SGR table,
  notebook-mapping.md §5)"; new rows `| anstyle-parse | `=1.0.0` | | notebook ANSI parsing (T18) |` and
  `| caseless | `0.2.2` | | footnote names folded as comrak folds them (T18, §2.4) |`. clap_builder
  4.6.7 requires anstyle ^1.0.14, anstream 1.0.0 (clap's default `color`) anstyle-parse ^1.0.0 and
  comrak 0.57.0 caseless ^0.2, so the pins resolve to the crates already in the tree.
- markdown-dialect.md §2.2 names no front-matter functions; T10 must export presenter's
  `markdown::frontmatter::{split_front_matter, parse_front_matter}` with the signatures of §2.2's layer 4.
- No `[notebook] ansi`, `echo`/`output` defaults or `svg_converter` keys (R10 suggestions): config.md is
  frozen; renditions come from `typst::render` (brief).
