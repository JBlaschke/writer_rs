# Spec: Markdown dialect

**Status:** contract. Implemented by T01 (`src/markdown/code.rs`: `CodeAttrs`, complete, §4.6),
T10 (front matter and its exported functions, §2.2), T11 (comrak options, attribute grammar and
recovery, the `invalid-code-attr` check added to `code.rs`, directive info, the colon pre-pass),
T12 (AST → IR walker, directives, presenter-compat drops), T1B (references, citations, includes),
T1C and T71 (`check`). Consumers: T18 (notebook markdown cells reuse §2.3 and §2.2's front-matter
functions), T17/T70 (resolve reads the shapes of §7), every emitter (`CodeAttrs`, §4.6), the
guide, and the fixtures of §11.

> **Contract change (coordinator, 2026-10-24):** `src/markdown/code.rs` moves from T11 to T01
> (wave 1, before T11 and T13; no wave changes), which writes `CodeAttrs` complete (§4.6); T11
> only adds the `invalid-code-attr` check there (additive). PLAN §3.2's
> `{options,attrs,recover,code,directives}.rs: T11` therefore reads `code.rs: T01` (html-output.md
> Deviations, "`CodeAttrs` before T11", now cites §4.6 for the struct).

The dialect is CommonMark + GitHub-flavored extensions as implemented by `comrak` 0.57, plus
pandoc conventions for documents (implicit figures, attributes, `Table:` captions, raw fences,
definition lists, footnotes, citations) and presenter_rs's `:::` directives. Design rules:
anything comrak parses natively is used as is (R9 §16 is the starting point); writer adds only
walker-level rules that are unambiguous on comrak's AST; the overlap with presenter_rs
(front matter, attributes, directives, code attributes, math, includes, alerts) is kept
source-compatible (§9). Every example here is a fixture under `tests/fixtures/md/`.

## 1. comrak configuration (exact) — T11

```rust
// src/markdown/options.rs (T11; re-exported as markdown::comrak_options by mod.rs, whose `pub use` line T00 writes)
// Cargo: comrak = { version = "=0.57.0", default-features = false, features = ["attributes"] }  (vendored-assets.md §7.3 is authoritative)
pub fn comrak_options(md: &MarkdownConfig) -> comrak::Options<'static> {
    let mut o = comrak::Options::default();
    // presenter's set (R8 §4.1), unchanged
    o.extension.front_matter_delimiter = Some("---".into());
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.tasklist = true;
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.extension.description_lists = true;
    o.extension.alerts = true;                 // > [!NOTE] Title            → Callout
    o.extension.math_dollars = true;           // $x$, $$x$$
    o.extension.math_code = true;              // $`x`$ and ```math fences
    o.extension.block_directive = true;        // :::name … :::  (container-only, §5)
    o.extension.header_attributes = true;      // # H {#id .c k=v}  (ATX and setext)
    o.extension.fenced_code_attributes = true; // ```lang {#id .c k=v}
    o.extension.link_attributes = true;        // [t](u){…}  ![a](u){…}
    o.extension.inline_code_attributes = true; // `c`{…}
    // writer additions
    o.extension.inline_footnotes = true;       // ^[text]  → FootnoteReference{name:"__inline_N"} + synthetic definition
    o.extension.superscript = md.superscript;  // x^2^   default false ([markdown] superscript)
    o.extension.subscript = md.subscript;      // H~2~O  default false ([markdown] subscript; ~~x~~ stays Strikethrough)
    o.parse.leave_footnote_definitions = true; // definitions stay in place; unreferenced ones keep total_references == 0 (lint)
    o.parse.escaped_char_spans = true;         // \@ → Escaped > Text("@"); the reference scanner skips it (§7.2)
    // parse
    o.parse.smart = md.smart_punctuation;      // default false (keeps -- and "quotes" literal)
    o.parse.relaxed_autolinks = false;
    o.parse.sourcepos_chars = false;           // columns in bytes; writer only uses lines (§4.9)
    // render: writer never formats with comrak; these keep `format_xml` usable as a debugging dump
    o.render.r#unsafe = true;
    o.render.escape = false;
    o.render.hardbreaks = false;
    o.render.github_pre_lang = false;
    o.render.full_info_string = false;
    o.render.sourcepos = false;                // the AST carries sourcepos regardless (R9 §12)
    o
}
```

> **Contract change (coordinator, 2026-10-24):** the Cargo comment pins `=0.57.0` (was the caret
> `"0.57"`), matching vendored-assets.md §7.3, whose table is authoritative: comrak's AST feeds
> the snapshots of §11.

Not enabled, deliberately: `underline` (takes over `__strong__`), `shortcodes` (needs a cargo
feature and uses `:`), `math_latex` (indistinguishable from dollar math in the AST),
`multiline_block_quotes` (`>>>` is a nested quote in presenter; long callouts use `:::callout`),
`highlight`, `insert`, `spoiler`, `greentext`, `subtext`, `wikilinks_*`, `ofm_comments`,
`cjk_friendly_emphasis`, `header_id_prefix`, `image_url_rewriter`/`link_url_rewriter` (HTML
formatter only; writer rewrites URLs in its IR), `parse.ignore_setext` (CommonMark semantics:
`Para` + `---` is a level-2 heading, as in pandoc; the guide documents the pitfall). An exhaustive
`match` on `NodeValue` must include 0.57's `CommentInline(_)` and `MultilineComment(_)` arms;
they are never produced with these options.

## 2. Document structure and front matter — T10 (§2.2), T12 (§2.1, §2.2's abstract parsing, §2.3)

### 2.1 Structure
A document is `[front matter]` followed by blocks; there are no separators. `---` anywhere but
line 1 is a `ThematicBreak` (or a setext underline). The title block (`meta.title`, `subtitle`,
`authors`, `date`), the abstract and the bibliography are not blocks: emitters render them from
`DocMeta` and `Resolved`. When the effective `config.document.toc` is `Depth(n)` and the document
has no `:::toc` (§5.5), the frontend inserts `Block::Toc { depth: n, span: None }` at index 0
(ir.md §1's `Toc` comment; ir.md §2.8's span exception applies to it). Heading text is never
promoted to `meta.title`: the title comes from front matter only. The `.unnumbered` class on a
heading is kept in `attrs.classes` for resolve (references.md).

### 2.2 Front matter
Lifted from presenter (`markdown/frontmatter.rs`, R8 §3.2) with the key table of config.md §3:
- Must start on line 1 with `---` and end with a line `---`; comrak yields
  `NodeValue::FrontMatter(String)` holding the delimiters and the blank line(s) after them
  (R9 §17): strip the first line, the last `---` line and surrounding whitespace, then parse
  as YAML 1.2 core schema with `serde_norway` (`yes/no/on/off` are strings; duplicate keys are
  errors; non-map YAML is `Error::FrontMatter("expected a mapping")`).
- `---` on line 1 directly followed by `---` on line 2 is empty front matter (as in Jekyll and
  pandoc); the document is then parsed without the front matter extension so a later `---` is a
  thematic break. The warnings `front-matter-empty` (no key at all and a comment that reads as a
  Markdown heading) and `front-matter-unclosed` (a heading after a blank line after the last YAML
  line, then a blank line before the closing `---`) and the YAML error hints (`: ` in a plain
  value → "put the value in quotes"; a tab → "indent with spaces") are presenter's, unchanged.
- Known keys go to `DocMeta` (`title`, `subtitle`, `date`, `description`, `lang`, `keywords`
  string or list); non-string scalars for string fields keep their YAML scalar text
  (`date: 2026-10-24` → `"2026-10-24"`, `title: 42` → `"42"`).
- `author` (string) → `authors: [Author { name }]`; `authors` (list of strings or of
  `{name, affiliation, email, id}` maps) → `DocMeta.authors`; `author:` given as a list is
  accepted like `authors` with the warning `front-matter-author-list` (presenter rejects it).
  Both keys present → `Error::FrontMatter("use either author or authors")`.
- `abstract` (string): `parse_front_matter` (T10) keeps the text as `DocMeta.vars["abstract"]`
  (an empty string is dropped) and leaves `abstract_ = None`, because `parse_blocks` (§2.3)
  belongs to T12, which depends on T10. `markdown::parse` (T12) then removes that var before
  `Config::merge_front_matter` and sets `DocMeta.abstract_ = Some(parse_blocks(text, &config, …))`
  with the merged config clone (ir.md §4) and the document's own `Fragment` (§2.3).
  `notebook::parse_with` (T18) does the same for layers 3–4 (notebook-mapping.md §2.2). The
  abstract may contain references, citations, math and footnotes (numbered before the body's);
  `:::include` inside it is the error `include-not-top-level` (§8).
- Config-like keys (`toc`, `numbering`, `bibliography`, `cite_style`/`csl`, `theme`, `template`,
  `reference_doc`, `latex_template`, `paper`) and the table maps (`markdown:`, `math:`,
  `numbering:`, `references:`, `html:`, `pdf:`, `docx:`, `latex:`, `typst:`, `notebook:`) are
  stored in `DocMeta.vars` and consumed by `Config::merge_front_matter` (config.md §3); an empty
  table value (`html:`) is stored as `null` and counts as an empty mapping. `numbering` also accepts a boolean
  (config.md §3); other non-mapping table values are `Error::FrontMatter`. Everything else
  goes to `vars` verbatim.
- T10 exports the following functions (R8 §3.2). T18 uses the split/parser pair for notebook
  metadata layer 4 and `meta_from_json` for layers 2–3 (notebook-mapping.md §2.2):
  ```rust
  // src/markdown/frontmatter.rs (T10)
  /// A `---` block starting on line 1 of `text`: (yaml without the delimiters, the rest, lines consumed).
  pub fn split_front_matter(text: &str) -> Option<(&str /*yaml*/, &str /*rest*/, usize /*lines consumed*/)>;
  /// The YAML of a front-matter block → `DocMeta` by the rules of this section
  /// (`abstract_` stays `None`; the abstract text is `vars["abstract"]`).
  pub fn parse_front_matter(yaml: &str) -> Result<DocMeta>;
  pub fn strip_delimiters(literal: &str) -> &str;
  pub fn parse_document_front_matter(yaml: &str, closing: usize, diagnostics: &mut Diagnostics) -> Result<DocMeta>;
  pub fn swallowed_heading(yaml: &str) -> Option<(usize, &str)>;
  pub fn meta_from_json(value: &serde_json::Value) -> Result<DocMeta>;
  ```

> **Contract change (coordinator, 2026-10-24):** §2.2 now names T10's exported front-matter
> functions (notebook-mapping.md §2.2 and its Deviations).

> **Contract change (coordinator, 2026-10-25):** the abstract is no longer parsed by T10:
> `parse_front_matter` leaves it in `vars["abstract"]`, and T12's `markdown::parse` (T18's
> `notebook::parse_with` for notebooks) parses it with `parse_blocks` and removes the var. T10
> cannot call `parse_blocks` (T12 depends on T10). No PLAN §5 dependency changes.

> **Contract change (coordinator, 2026-10-26):** T10's key rules are explicit: a scalar
> `keywords` value becomes one keyword; `authors` must be a list, and author maps require
> `name` and reject unknown keys. Empty table values are stored as `null`; `numbering` accepts
> a boolean as well as a map. The additional exported helpers above retain presenter's
> shapes; `parse_document_front_matter` raises the three front-matter warnings, whereas
> `parse_front_matter` and `meta_from_json` have no diagnostics sink. No IR changes.

### 2.3 Fragments (shared entry points)
```rust
// src/markdown/mod.rs (T12). Used for the abstract, notebook markdown cells (T18), alert and
// directive titles, code captions and `Table:` captions. No front matter, no :::include expansion.
pub struct Fragment<'a> { pub assets: &'a mut AssetRegistry, pub base_dir: &'a Path, pub diagnostics: &'a mut Diagnostics,
                          pub file: Option<&'a Path>, pub cell: Option<usize>, pub missing_asset_is_error: bool }
pub fn parse_blocks(text: &str, config: &Config, ctx: &mut Fragment) -> Result<Vec<Block>>;
pub fn parse_inlines(text: &str, config: &Config, ctx: &mut Fragment) -> Result<Inlines>;   // one paragraph's inlines
```
`parse_inlines` of text that yields anything but a single `Paragraph` returns the inlines of
the first paragraph and pushes the warning `fragment-not-inline`.
Spans of fragment blocks carry `ctx.file`/`ctx.cell`.

## 3. Attributes — T11

### 3.1 Syntax and comrak's limits (lifted from presenter §3.1, unchanged)
Pandoc-style attribute blocks `{#id .class1 .class2 key=value key2="quoted value"}`. comrak
parses them natively on exactly five node kinds — `Heading`, fenced `CodeBlock`, `Code`, `Link`,
`Image` (R9 §5) — into `Ast.attrs: Option<Box<Attributes { id, classes, pairs }>>`. comrak 0.57's
grammar is byte-identical to 0.56's: it rejects a whole block when a value is single-quoted, an
unquoted value has characters other than letters, digits and `- _ . : %` (`background=#222`,
`src=figures/bg.png`, `lines=1|3`), an item is a bare key, an item ends with an unquoted comma,
or a `{`/`}` appears outside quotes; the rejected block then stays in the heading text / the
fence info string, or becomes a trailing `Text("{…}")` sibling of the inline. Of two blocks at
the end of a heading only the last is read; a block in the middle of a line is text; `{}` is an
empty block; the last `#id` wins. Writer's own `markdown::attrs::parse_attr_block(&str)` has the
same grammar with presenter's extensions (single quotes, bare keys, braces optional) and the two
must agree (shared differential unit tests, lifted). Every item must be `#id`, `.class` or
`key=value` with `is_valid_attr_name(key)` (an ASCII letter, then `[A-Za-z0-9-_:.]`).

### 3.2 Recovery fallback (`src/markdown/recover.rs`, lifted)
`recover_attr_blocks(arena, root, text, opts, diags) -> (root, Cow<str>)` re-reads every block
comrak rejected with `parse_attr_block`'s tokenizer, rewrites it with every value double-quoted
(the form comrak reads itself), re-parses (≤ `MAX_ROUNDS = 3`) and verifies comrak now reads
exactly that; a block that is still invalid stays text and is an `unparsed-attr-block` warning.
Line numbers never change (rewrites stay within their line). Writer additionally applies
`parse_attr_block` itself to three places comrak never reads: the trailing `{…}` of a `Table:`
caption paragraph (§4.4), the label text after display math (§4.5), and the pandoc form of a
directive info string (§5.2).

### 3.3 Where attributes go in the IR
comrak `Attributes` → `ir::Attrs` with `pairs` collected into the `BTreeMap` (a repeated key:
the last value wins). Headings, code blocks, inline code, links and images carry their attrs
on the corresponding IR node; an image alone in a paragraph gives its `#id` and classes to the
`Figure` and keeps its pairs (§4.3). Any `attrs.id` is a label: it must match references.md's
alphabet `[\p{XID_Continue}_\-:.]+` (so comrak's `{#"with space"}` is rejected); the frontend
raises `label-invalid` (error) and drops the id. `kind-name` (`fig-x`, `tbl-x`, `eq-x`, `sec-x`,
`lst-x`) is the convention, `fig:x` is accepted everywhere but on directive lines (§5.3). On
links and images `title=` is used only when there is no Markdown title.

## 4. Blocks → IR — T12

### 4.1 Mapping table (every comrak node; payload names from R9 §3)
| comrak node | IR | rule |
|---|---|---|
| `Document` | `Document.blocks` | children in order; `FootnoteDefinition`s are collected, not emitted |
| `FrontMatter(s)` | `DocMeta` + config | §2.2; never a block |
| `Heading{level}` + attrs | `Heading { level, inlines, attrs }` | §3.3; attrs fallback on trailing `{…}` text (§3.2) |
| `Paragraph` [sole `Image`] | `Figure { kind: Image }` | §4.3 |
| `Paragraph` [`Math{display_math:true}` (+ label text)] | `Math { attrs }` | §4.5 |
| `Paragraph` first `Text` starts with `Table:` | caption of the adjacent `Table` | §4.4; otherwise an ordinary paragraph |
| `Paragraph` whose source line is `--` or matches `^(---\|--)\s*\{.*\}\s*$` | dropped | §9, info `presenter-separator` |
| `Paragraph` | `Paragraph { inlines }` | also inside tight lists (the IR has no `Plain`; `List.tight` tells emitters) |
| `BlockQuote` | `BlockQuote { blocks }` | |
| `Alert{alert_type, title}` | `Callout { kind, title, blocks, attrs: {} }` | `kind` = lowercase type name; `title` = `parse_inlines(title)` or `None` (§4.7) |
| `List` > `Item` / `TaskItem{symbol}` | `List { kind, tight, items }` | §4.2 |
| `DescriptionList` > `DescriptionItem` > `DescriptionTerm` / `DescriptionDetails` | `DefinitionList { items }` | one `DefinitionItem { term: inlines of the term's Paragraph, definitions: [details blocks] }` per term; termless continuation items extend the preceding term's definitions |
| `CodeBlock{info:"math"}` | `Math` | §4.5 (`` ```math {#eq-x} `` carries attrs) |
| `CodeBlock{info:"{=fmt}"}` | `Raw { format }` | §4.8 |
| `CodeBlock{info, literal}` + attrs | `CodeBlock` or `Figure { kind: Listing }` | §4.6 |
| `Table{alignments}` > `TableRow(hdr)` > `TableCell` | `Table` or `Figure { kind: Table }` | §4.4 |
| `ThematicBreak` | `ThematicBreak` | |
| `HtmlBlock{block_type:2}` | dropped | HTML comments (§4.8) |
| `HtmlBlock{literal}` | `Raw { format: "html", text }` | §4.8 |
| `BlockDirective{info}` | §5.5 | containers, leaves, includes |
| `FootnoteDefinition{name}` > blocks | footnote map `name → blocks` | §6; unreferenced (`total_references == 0`) → `footnote-unreferenced` |
| `MultilineBlockQuote`, `Subtext`, `Highlight`, `Insert`, `Underline`, `SpoileredText`, `EscapedTag`, `WikiLink`, `CommentInline`, `MultilineComment`, `Raw`, `ShortCode`, `HeexBlock/Inline` | never produced | the walker returns `Error::Other` if it meets one (a test guards the options) |

### 4.2 Lists

> **Contract change (coordinator, 2026-10-26):** comrak emits an additional termless
> `DescriptionItem` for another `: definition` under the same term. The walker appends its
> details to the preceding `DefinitionItem`, preserving the IR's multiple definitions per
> term instead of creating an empty term. Explicit terms remain separate.

`ListKind::Bullet` for `ListType::Bullet`, `Ordered { start: list.start }` for `Ordered` (the
delimiter `.`/`)` is not kept). `tight` is read from the `List` node, never from `Item` (always
`false` there, R9 §19). A `TaskItem{symbol}` gives `checked: Some(symbol.is_some())`; other
items `None`. ir.md §2.1 requires tight items to hold only `Paragraph`, `List` and `CodeBlock`:
when any item of a tight list holds another block (`- > quote`), the frontend sets
`tight: false` for the whole list.

### 4.3 Implicit figures (R9 §7)
After attribute recovery, a `Paragraph` whose only child is an `Image` is a figure when the
image has a non-empty alt or an `#id` (pandoc's rule: `![](x.png)` alone stays an inline image):

`![Caption with *emph*](plot.png "T"){#fig-x .wide width=50%}` →
`Figure { kind: Image, caption: Some([Text "Caption with ", Emph …]), attrs: { id: "fig-x", classes: ["wide"] },
body: [Paragraph [Image { alt: same inlines, url: "asset:…", title: Some("T"), attrs: { pairs: { width: "50%" } } }]] }`.
Empty alt with an id → `caption: None` (numbered, caption-less). `![One](a.png) ![Two](b.png)`,
`![…](a.png) trailing text` and `[![…](a.png)](u)` are not figures (inline images). Inside a
`:::figure` body the rule applies only to images with an `#id` (sub-figures, ir.md §2.2).

Image URLs follow presenter §6 verbatim: relative paths (no scheme, not `/`-rooted) resolve
against `SourceInfo.base_dir` (the including file's directory for includes); existing files are
`assets.register_file(abs)` and the url becomes `asset:<id>`; missing files keep the url and
raise `missing-asset` (error in `build`, warning in `serve`/`check`); a file found only by
letter case is used with `asset-case-mismatch`; remote URLs are never fetched; links are never
registered. `width`/`height` values must match `^[0-9]+(\.[0-9]+)?(%|px|pt|cm|mm|in|em)?$` (bare
number = px); anything else is dropped with the warning `invalid-dimension`.

> **Contract change (coordinator, 2026-10-26):** when implicit-figure alt text is copied to
> the caption, footnotes belong to that caption alone. Remove `Footnote`/`FootnoteRef`
> occurrences recursively from the body image's alt inlines to preserve ir.md §2 invariant
> 4. Its plain-text alt is unchanged (`ir::plain_text` already omits footnotes). Dimension
> numbers use ASCII `[0-9]`, as CSS requires, rather than Unicode-aware `\d` matching.

### 4.4 Tables and `Table:` captions
`Table` → `columns: alignments.map(None→Default, Left, Center, Right)` with `width: None`;
`head` = the rows with `TableRow(true)` (cells `header: true`), `body` = the rest, `foot = []`;
every cell `colspan: 1, rowspan: 1, align: None`, inlines per §6. A caption is a `Paragraph`
whose first inline is a `Text` starting with `Table:` (exactly that spelling); its caption
inlines are the paragraph's inlines with the `Table:` prefix and following whitespace removed
from the first `Text` and a trailing `{…}` removed from the last `Text` and read with
`parse_attr_block` (R5 §4.4's `: caption` row is superseded: that line becomes a table row in
comrak). Binding (brief): a `Table:` paragraph captions the table immediately after it (next
sibling); a table without one takes an immediately following `Table:` paragraph unless that
paragraph is itself followed by a table; a `Table:` paragraph adjacent to no table is an
ordinary paragraph plus the warning `table-caption-orphan`; a table whose last body row's first
cell starts with `: ` is the warning `table-caption-pandoc-colon` (the row stays). A table with
a caption or an id is wrapped: `Figure { kind: Table, caption, attrs, body: [Table] }`
(ir.md §2.2); a bare table stays `Block::Table` and is unnumbered.

### 4.5 Display math and labels
`$$…$$` is an inline `Math{display_math:true}` node in comrak (R9 §10). A `Paragraph` whose
children are exactly `Math{display_math:true}`, an optional `SoftBreak`, and one `Text` matching
`^\s*\{[^}]*\}\s*$` → `Block::Math { source: literal.trim(), dialect: Latex, attrs:
parse_attr_block(text) }`; a paragraph whose sole child is `Math{display_math:true}` → the same
with empty attrs; both also inside list items and quotes. A fenced `` ```math {#eq-x} `` block
→ `Block::Math` with the fence's attrs. Any other `Math` is `Inline::Math` (a `$$…$$` in the
middle of a paragraph renders inline). A paragraph that contains a display `Math` and a `Text`
containing `{#` but does not match the block form is the warning `equation-label-misplaced`.
`Features.math` becomes true for any math.

### 4.6 Code fences (ir.md §1 cites this table as "markdown-dialect.md §5")
`CodeBlock { lang: first whitespace-separated word of info (None if empty), text: literal without
its trailing newline, attrs }` where `attrs` are the fence's attributes after recovery. The
attribute vocabulary, shared by every emitter through `markdown::code::CodeAttrs::from_attrs(&Attrs)`:

| attribute | meaning |
|---|---|
| first word of info | `lang` (`python`, `rust`, …); `.plain` disables highlighting |
| `.numbered` | line numbers |
| `start=N` | first line number (default 1; `N` ≥ 1) |
| `highlight="1-3,5"` | emphasised lines: comma-separated numbers or `a-b` ranges |
| `lines="1-3\|5"` | presenter alias: `.numbered` + `highlight` with `\|` read as `,` |
| `caption="…"` | `parse_inlines` → the fence becomes `Figure { kind: Listing, caption, body: [CodeBlock] }` |
| `#lst-x` | label; also wraps in `Figure { kind: Listing }` (caption `None` allowed); the id moves to the Figure |
| `include=…` | reserved for T76 (stretch); until then an ordinary pair |
| other `.class`, `k=v` | kept in `attrs`; emitters ignore pairs they do not know (`trim`, `escape`) |

Invalid `start`/`highlight`/`lines` values are kept in `attrs` but ignored, with the warning
`invalid-code-attr`. `Features.code` becomes true. Indented code blocks are `CodeBlock` with
`lang: None` and empty attrs.

Every emitter reads the table above through one type:
```rust
// src/markdown/code.rs (T01, complete; T11 adds the `invalid-code-attr` check here, additively, and T12's walker raises it)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAttrs {
    pub numbered: bool,                                 // `.numbered`, or a valid `lines=`
    pub plain: bool,                                    // `.plain`
    pub start: u64,                                     // `start=N`, N ≥ 1; default 1
    pub highlight: Vec<std::ops::RangeInclusive<u64>>,  // `highlight=` / `lines=` (`|` read as `,`); `n` → `n..=n`; line numbers as displayed
}
impl CodeAttrs {
    /// Reads `attrs.classes` and `attrs.pairs`; invalid values are ignored (defaults kept), never an error.
    pub fn from_attrs(attrs: &Attrs) -> CodeAttrs;
    /// `line` as displayed: the i-th line (0-based) of the block is `start + i`, numbered or not.
    pub fn is_highlighted(&self, line: u64) -> bool;
}
```
`CodeAttrs` carries no language: that is `CodeBlock.lang` (for a notebook cell, `Cell.language`),
which emitters read directly; for a cell they call `CodeAttrs::from_attrs(&cell.attrs)`
(notebook-mapping.md §2.7).

> **Contract change (coordinator, 2026-10-24):** the `CodeAttrs` definition above (first given in
> html-output.md's Deviations, "`CodeAttrs` before T11") is now owned by this section; T01 writes
> `src/markdown/code.rs` and T11 adds only the `invalid-code-attr` check (header).

### 4.7 Alerts, quotes, breaks
`> [!NOTE] Reproducibility` + `> body` → `Callout { kind: "note", title: Some(parse_inlines("Reproducibility")), blocks }`;
the tag is case-insensitive, five kinds only (`note`, `tip`, `important`, `warning`, `caution`);
`title: None` when the tag line is empty (emitters use the kind's default title); an unknown tag
is a plain block quote with its literal text (comrak's behaviour). Custom kinds and attributes
need `:::callout` (§5.5). `***`, `___`, `- - -` and `---` (not on line 1) → `ThematicBreak`.

### 4.8 Raw blocks, `{=fmt}` fences, HTML comments
A fence whose info matches `^\{=([A-Za-z0-9_-]+)\}$` (comrak keeps it in `info`: `=` is not an
attribute item) → `Block::Raw { format: lowercase, text: literal }`; known formats are `html`,
`latex`, `typst`, `openxml`, `markdown`; another format is kept with the warning
`raw-unknown-format`. `HtmlBlock` with `block_type == 2` (a comment) is dropped; any other
`HtmlBlock` → `Raw { format: "html" }` with relative `src="…"`/`poster="…"` values registered
and rewritten to `asset:<id>` like images (presenter §6, `rewrite_html_assets`). Each emitter
prints only its own format(s) and skips others silently; `check` reports every skipped block as
info `raw-skipped` per target (§10). `:::raw` is the directive form (§5.5).

### 4.9 Source spans
Every block gets `span: Some(SourceSpan { start_line: sourcepos.start.line, end_line, cell:
ctx.cell, file: ctx.file })`, 1-based. When comrak reports an end column of `0` (`@26:1-27:0`,
R9 §12) `end_line = end.line - 1` (never below `start_line`). Columns are never used; the
description-list term bug (R9 §15) is harmless under this rule. `Ref`/`Cite` spans are the
paragraph's (lines only).

## 5. Directives — T11 (§5.2–5.3), T12 (§5.4–5.5)

### 5.1 Container syntax and nesting
`:::name args` … `:::` is a comrak `BlockDirective{fence_length, fence_offset, info}` (R9 §6):
≥ 3 colons at indent ≤ 3 open it, a line of ≥ `fence_length` colons closes the **outermost**
open directive whose fence is not longer. Nested directives therefore need a longer outer
fence (`::::columns` … `:::column` … `:::` … `::::`); same-length inner fences close the outer
one early. A directive line interrupts a paragraph. An unclosed directive runs to the end of
the file: the walker detects it when the last line of the directive's span is not a closing
fence (`^\s{0,3}:{3,}\s*$`) and warns `unclosed-directive`.

### 5.2 Info parsing (lifted from presenter §4)
`parse_directive_info(info) -> DirectiveInfo { name, id, classes, args: Vec<(String, String)>, problems }`:
the info is split on whitespace outside quotes; the first token is the name; remaining tokens
are classes unless they contain `=`, in which case they are `key=value` args (values `"…"` or
`'…'` quoted; comrak has already entity- and backslash-unescaped the string, so a value with
`"` must be single-quoted). Pandoc's attribute block is accepted as the whole info:
`::: {.note title="X"}` is `:::note title="X"` (the first class is the name), `::: {#intro .note}`
adds `id="intro"`; a `{…}` at the end of a named info (`:::figure {#fig-x .wide width=80%}`) is
read with `parse_attr_block` and merged. A bare `:::` with empty info is `invalid-directive`
(warning); its children are emitted in place, unwrapped.

### 5.3 The colon pre-pass (binding, PLAN §8)
comrak refuses to open a directive whose line contains a `:` after the fence (R9 §6.2): the
line becomes a paragraph and the later bare `:::` opens an empty directive that swallows the
rest of the file. `colon_prepass(text, opts) -> (Cow<str>, Vec<usize>)` (T11) runs before
attribute recovery: (1) parse once; (2) collect every `Paragraph` that is a direct child of
`Document` or of a `BlockDirective` whose first child is a `Text` starting with `:::`; (3) for
every source line within those paragraphs' spans that matches `^\s{0,3}:{3,}`, strip pandoc's
trailing closing colons (`\s*:{3,}\s*$` after the opening fence) and rewrite every remaining
`:` after the fence to `&#58;` (comrak unescapes entities in the info, so `#fig&#58;x` reads
`#fig:x`); (4) re-parse once. Line numbers are unchanged; columns on those lines shift, which is
why diagnostics use start lines. Each rewritten line is the info-level `directive-colon-rewritten`.
A line matching `^\s{0,3}:{3,}` that still opens or closes nothing after the pre-pass (indented
code, a list item, a quote) is the warning `invalid-directive` naming the fix.

> **Contract change (coordinator, 2026-10-26):** T11 runs the pre-pass and attribute recovery
> in sequence. T12/T18 call `colon_prepass` → emit one info diagnostic per returned line →
> `parse_document` → `recover_attr_blocks` → `check_unopened_directives`. The last helper
> emits `invalid-directive` for unopened lines in paragraphs/headings in any container or
> indented code, skipping fenced code and HTML. Its spans contain lines only; callers add
> file/cell context. T11 also exports `attrs::ir_attrs(parsed, span, diagnostics)` for label
> validation and `code::check_code_attrs(attrs, span, diagnostics)` for ignored code values.
> Their stable codes are `LABEL_INVALID`, `INVALID_CODE_ATTR`, `INVALID_DIRECTIVE`, and
> `DIRECTIVE_COLON_REWRITTEN`. `recover::fence_attrs` remains public for T12's code fences.

### 5.4 Leaf form
There are no leaf directives in comrak; the leaf form is an empty container: `:::pagebreak` +
`:::`, `:::toc depth=2` + `:::`, `:::include analysis.ipynb` + `:::`, `:::bibliography` + `:::`
(`::: {.pagebreak}` works too). A leaf name with a non-empty body emits the leaf, then the body
blocks, with the warning `invalid-directive`.

### 5.5 Directive table
| directive | IR | rules |
|---|---|---|
| `:::figure {#fig-x .cls} [caption="…"] [kind=image\|table\|listing\|other]` | `Figure { kind, caption, body, attrs }` | caption: the `caption=` arg (`parse_inlines`), else the body's last block when it is a `Paragraph` and the body has ≥ 2 blocks, else `None`. kind: the arg; else `Image` when the remaining body is one `Paragraph` holding only an `Image` (normalised to ir.md §2.2's shape), `Table` for exactly one `Table`, `Listing` for exactly one `CodeBlock`, else `Other`. Images with an `#id` inside become sub-figures; a bad `kind=` is `invalid-directive-arg` (→ `Other`) |
| `:::callout [kind] [title="…"]` | `Callout { kind, title, blocks, attrs }` | kind = the first class or `kind=`; default `note`; any word is allowed (ir.md: custom kinds) |
| `:::note`, `:::tip`, `:::important`, `:::warning`, `:::caution` `[title="…"]` | `Callout { kind: name, … }` | `title` → `parse_inlines`; `#id`/other classes → `attrs` |
| `:::pagebreak` (leaf) | `PageBreak` | args ignored |
| `:::toc [depth=N]` (leaf) | `Toc { depth }` | `depth` 1–6, else `invalid-directive-arg`; default: `config.document.toc`'s depth, else 3. Several are allowed |
| `:::bibliography` (leaf) | `Bibliography` | a second one is `invalid-directive` and dropped |
| `:::include PATH` (leaf) | the included file's blocks | §8 |
| `:::raw format=FMT` / `:::raw FMT` | `Raw { format, text }` | `text` = the source lines strictly between the fences, verbatim (sourcepos slice), with a trailing newline; no `FMT` → `invalid-directive-arg` and the body is emitted as blocks |
| `:::notes`, `:::comment` | nothing | dropped from every output, silently (PLAN §1; §9) |
| `:::columns [widths="1fr 2fr"]`, `:::column [width=…]`, `:::fragment [effects…] [index=N]` | `Div { attrs: { classes: [name, …], pairs: args } }` | presenter layouts; the HTML theme styles `columns`/`column`; other emitters print the children |
| any other name | `Div { attrs: { id, classes: [name, classes…], pairs: args } }` | the default theme styles a documented subset (`center`, `small`, `large`) |

Args not consumed by a known directive are kept in `attrs.pairs` without a diagnostic. `Div`
and `Callout` bodies are walked like the document (nested directives, figures, math, includes
excluded: §8).

## 6. Inline syntax — T12 (scanner: T1B)
| comrak | IR | notes |
|---|---|---|
| `Text` | `Text` | smart punctuation already applied in the literal when `parse.smart`; `@…` scanned per §7 |
| `SoftBreak` / `LineBreak` | `SoftBreak` / `LineBreak` | `\` or two trailing spaces give hard breaks |
| `Emph` (`*x*`, `_x_`), `Strong`, `Strikethrough` (`~~x~~`) | `Emph`, `Strong`, `Strikeout` | |
| `Superscript` (`x^2^`), `Subscript` (`H~2~O`) | `Superscript`, `Subscript` | only with `[markdown] superscript`/`subscript` |
| `Code{literal}` + attrs | `Code { text, attrs }` | `` `<b>x</b>`{=html} `` → `Inline::Raw { format: "html" }` (the `{=fmt}` is read from the trailing text) |
| `Link{url,title}` + attrs | `Link { inlines, url, title, attrs }` | inline, reference, collapsed, shortcut, `<autolink>`, `www.`/`https://`/email autolinks; `#label` urls stay as written (emitters mangle anchors per references.md); `title` `None` when empty |
| `Image` (not alone in its paragraph) | `Image { alt, url, title, attrs }` | asset rules of §4.3; `width`/`height` pairs |
| `FootnoteReference{name, ref_num, ix}` | `Footnote { blocks, label: Some(name), number: None }` for `ref_num == 1`, `FootnoteRef { label: name }` after | blocks = the definition's blocks (walked); inline `^[text]` → `Footnote { label: None }` with the synthetic definition's blocks; names are per file (§8); `Features.footnotes` |
| `Math{display_math, literal}` (not the §4.5 shape) | `Math { source, dialect: Latex }` | `` $`x`$ `` too; a literal `$` is `\$` |
| `HtmlInline(s)` | `Raw { format: "html", text }` | `<!-- … -->` inline comments are dropped |
| `Escaped` > `Text` | the child `Text` | `\@`, `\*`, `\$`, `\#` …; see §7.2 |
| `Underline`, `SmallCaps`, `Span` | — | no Markdown syntax (raw HTML or the typst frontend) |

## 7. References and citations — T1B (syntax), T17/T70 (classification, references.md)

### 7.1 Syntax
`@target`, `@target[supplement]`, `@target{.mode}`, `@target[supplement]{.mode}`; pandoc groups
`[@a; @b, p. 12]`, `[see @a]`, `[-@key]` (≡ `@key{.year}`); `\@` is a literal `@`. Modes:
`.normal` `.prose` `.year` `.author` `.full` `.none` (typst's `cite(form:)`, → `CiteMode`) and
`.page` (→ `RefForm::Page`). Labels and keys share one syntax; `kind-name` is the convention
(§3.3). `[markdown] references = false` disables the scanner entirely (`@handle` stays text).

### 7.2 Scanner (`src/markdown/refs.rs`, runs on the IR inlines of every paragraph, heading,
table cell, caption and footnote, but not inside `Link` inlines)
- A reference starts at `@` in a `Text` node only when the `@` is at the start of the node and
  the node is first in its parent or follows a `SoftBreak`/`LineBreak`, or is preceded by
  whitespace, `[`, `(`, `;` or (inside a bracket group) `-`. It never starts inside an
  `Escaped` node, and a `Text` node that directly follows an `Escaped` sibling does not count as
  "start of node" for its first character (so `\@@key` is literal). The target is the maximal
  run of `[\p{XID_Continue}_\-:.]` after `@`, with trailing `.`/`:` stripped (typst's rule:
  `see @fig-x.` → target `fig-x`); an empty target leaves the `@` as text.
- An immediately following `[…]` (no space; within the same `Text` node, so plain text) is the
  supplement; an immediately following `{…}` read by `parse_attr_block` with exactly one class
  from §7.1's set is the mode; any other `{…}` stays text with the warning
  `invalid-reference-mode`.
- A bracket group is a `[` … `]` within one `Text` node whose content is `;`-separated items
  each holding exactly one `-?@target`; per item the text before `@` is `prefix`
  (`parse_inlines`, `None` if empty), the text after the first `,` is `locator` (trimmed free
  text, passed on as `Locator::Custom`), `suffix` is always `None`. A `{.mode}` after `]`
  applies to the group. `[-@key]` sets `mode: Year`.
- Autolinks never interfere: comrak needs `[alnum.+-_]` before `@` to link an email (R9 §8), so
  `me@example.com` is a `Link` and `@fig-x`, `(@tbl-y)`, `[@k; @j]` are text.

### 7.3 Parse-time shape and classification outcomes
> **Coordinator clarification (2026-10-27):** `.page` is valid only on a
> bare reference; a citation group's `.page` is an invalid mode whose attrs
> stay literal. Any negative `-@` item selects the group's Year mode unless
> an explicit valid group mode overrides it. `CiteMode` remains unchanged.

The frontend does not know the bibliography, so it emits syntactic shapes: a bare `@target…` →
`Inline::Ref { target, supplement, form: Page|Normal, resolved: None }`, where `supplement` is
`None` when no `[…]` follows, `Some(vec![])` for `[]`, and otherwise
`Some(vec![Inline::Text { text: <the bracket text> }])`, as in typst-dialect.md §2.2
(references.md §4.5: `@eq-x` → "Equation 1", `@eq-x[]` → "1"). A cite-only mode class is kept as
`Cite { items: [{ key: target, locator }], mode }` instead, with `locator` `None` when no `[…]`
follows, `None` for `[]`, and otherwise `Some(<the bracket text>)`; a bracket group →
`Inline::Cite { items, mode, resolved: None }`. `Features.citations` is set
from `Cite` nodes and recomputed by resolve. The resolve pass classifies each target
(references.md), with these binding outcomes:

| target is … | `Ref` becomes | `Cite` item becomes |
|---|---|---|
| a label, not a key | `Ref` (resolved; a cite-only mode → `invalid-reference-mode`, ignored) | `Ref`; a group of only labels → the refs joined by `Text(", ")`, brackets dropped |
| a key, not a label | `Cite { items: [{ key, locator: plain supplement }], mode }` | `Cite` |
| both | error `ambiguous-label` | error `ambiguous-label` (also for a group whose items classify differently) |
| neither | error `unresolved-reference` in `build`; warning + `[?target]` placeholder in `serve`/`check` | same |

> **Contract change (coordinator, 2026-10-25):** §7.3's parse-time shape now spells out the
> three supplement cases (`@x` → `None`, `@x[]` → `Some([])`, `@x[s]` → `Some([Text(s)])`) and
> the matching `locator` of a cite-only-mode `Cite` (`None`, `None`, `Some(s)`), as resolve
> (references.md §4.5) and the typst frontend (typst-dialect.md §2.2) already assume.

## 8. Includes — T1B (`src/markdown/include.rs`, lifted and adapted; T18 for `.ipynb`)
```markdown
:::include analysis.ipynb
:::
```
- `:::include PATH` with an empty body, as a direct child of the `Document` (any top-level
  position: between paragraphs, after a heading, first or last). `PATH` is the rest of the info,
  quoted when it contains spaces (`:::include "my dir/part 2.md"`, presenter's `include_path`),
  relative to the directory of the file containing the directive; absolute paths work; no URLs.
  The kind comes from `ir::detect_kind` (`.md`, `.ipynb`); any other extension, a missing or
  unreadable file, a depth over `MAX_DEPTH = 8` and a cycle (the message names the chain) are
  `Error::Parse` at the directive's line in every mode (`check`: `parse-error`). Inside a list,
  quote, directive, footnote or the abstract: error `include-not-top-level` (the directive is
  dropped).
- An included `.md` is parsed as a document on its own with a clone of the **including
  document's effective config** whose `document.toc` is `Toc::Off`, so no `Toc` (§2.1) is
  inserted at the include position, not even when the included file's front matter sets `toc`
  (typst-dialect.md §2.7 does the same). Its own front-matter config tables still apply to its
  own parsing; its front matter never sets `meta`; keys it sets that differ from the including
  document's metadata are the warning `include-meta-ignored`. Its blocks replace the directive
  in place; headings keep their levels.
- An included `.ipynb` goes through `notebook::parse_with(source, json, &effective_config,
  ParseOptions { includes: &mut *include::cell_includes_in(source, &chain), nested: true,
  selection: Selection::default() })` (notebook-mapping.md §2.6, §7.1; `chain` = the canonical
  paths of the files that include the notebook, outermost first), never `notebook::parse`, which
  inserts a `Toc` and starts a fresh include chain. Depth and cycles therefore count across `.md`
  and `.ipynb` (`a.md → b.ipynb → a.md` is a cycle). Before T18's body lands, T01's stub error
  `Error::Other("notebook support arrives in T18")` is reported like a missing file.
  `:::include` is also allowed inside notebook markdown cells (notebook-mapping.md §2.6, through
  this file's `cell_includes`/`cell_includes_in`).

> **Contract change (coordinator, 2026-10-24):** an included `.md` is parsed with `Toc::Off`
> (no `Toc` at the include position), and an included `.ipynb` goes through `notebook::parse_with`
> with `nested: true` and the continued include chain instead of `notebook::parse`
> (notebook-mapping.md §2.6 and its Deviations; typst-dialect.md §2.7).

- Includes nest (an included `.md` may include, and so may the markdown cells of an included
  `.ipynb`) to depth 8. Footnote names bind per file (`[^1]` in two files are two footnotes);
  labels are global (`duplicate-label` across files).
  Assets, features and diagnostics are merged (asset ids are collision-free); diagnostics keep
  the included file's path; every block from an included file has `SourceSpan.file` set to its
  absolute path (`None` for the main file); `Document.includes` lists every included file,
  transitively, absolute, in first-use order (the watch set, live-reload.md).
  `include::scan_includes(path) -> Vec<PathBuf>` (lifted) lets the watcher find includes of a
  file that failed to parse.

> **Coordinator clarification (2026-10-27):** include expansion uses private
> parse/walker context; public `Fragment` fields stay unchanged. Configuration
> tables affect included parsing and are excluded from `include-meta-ignored`;
> explicitly set document metadata, including empty lists, is compared.
> The best-effort watcher scanner descends into notebook Markdown and
> raw-Markdown cells while retaining arbitrary-extension Markdown roots.

## 9. Presenter compatibility — T12 (drops), T1C (`check --compat presenter`)
Identical in both tools: front matter grammar and the config-table rule (§2.2), the attribute
grammar and recovery (§3), `:::` container syntax, nesting rule and info parsing (§5.1–5.2),
the code-fence attribute grammar (§4.6; `lines=` is honoured), `$`/`` ```math `` math, alerts,
`:::include PATH` syntax and path rules, image/asset rules (`missing-asset`,
`asset-case-mismatch`), and comrak's option set (presenter's ⊂ writer's, §1). Writer reads these
differently:

| presenter construct | writer | diagnostic |
|---|---|---|
| `---` separator | `ThematicBreak` | none |
| `--` paragraph, `--- {…}` / `-- {…}` paragraph (raw source line, so `smart` does not matter) | dropped | info `presenter-separator` |
| slide shorthands on a heading (`layout`, `background`, `background-*`, `transition`, `transition-speed`, `auto-animate*`, `visibility`, `state`, `timing`, `autoslide`, `menu-title`) | removed from `attrs` | warning `presenter-attr-dropped` (`data-`/`aria-` keys are kept) |
| `<!-- .slide: … -->`, `<!-- .element: … -->`, `Note:` paragraphs | comments are dropped; `Note:` is an ordinary paragraph | none |
| `:::notes`, `:::comment` | dropped from every output | none |
| `:::fragment`, `:::columns`, `:::column`, `:::center`, … | `Div` (§5.5) | none |
| `:::include` only between `---` rules | allowed at any top-level position | `--compat`: `presenter-include-position` |
| `author:` string only | `authors:` lists, `author:` list | `--compat`: `presenter-front-matter` |
| `:::name` lines with a colon (never directives in presenter) | rewritten (§5.3) | `--compat`: `directive-colon-rewritten` as a warning |

`writer check --compat presenter` recomputes the three `--compat` rows from the source text
(the pre-pass line list, the include positions relative to `---` lines, the front matter keys)
and reports them as warnings. Writer-only constructs (implicit figures, `Table:` captions,
`@refs`, raw fences, leaf directives, `^[…]`) are not reported: presenter renders them as
images, paragraphs, text, code blocks, empty divs and text respectively.

> **Coordinator clarification (2026-10-27):** compatibility include
> boundaries are file/cell edges or top-level `---` thematic breaks, with
> no other significant block between the include and either boundary.
> Front matter, code, setext underlines and nested rules do not count.
> Compatibility source checks cover each distinct included file and
> Markdown/raw-Markdown notebook cell; author-list checks apply only to
> the first Markdown cell's front matter. Promote actual colon-rewrite
> findings once, preserving frontend configuration and source locations.
> Optional compatibility source checks include removed Markdown/raw-Markdown
> cells as written; normal frontend diagnostics retain their kept-cell rules.

## 10. What `check` reports — T1C (Markdown lints), T71 (per-target lints)
Levels: `ir.md` §5's `Level` is `Info | Warning | Error`. Info-level codes are pushed as
`Level::Info` diagnostics: `build` and `serve` print them only with `-v`, `check` lists them as
notes, `--compat presenter` promotes the compatibility ones to warnings, and `--deny-warnings`
ignores them. Codes raised by this dialect:

| code | level | section |
|---|---|---|
| `front-matter-empty`, `front-matter-unclosed`, `front-matter-author-list` | warning | §2.2 |
| `fragment-not-inline` | warning | §2.3 |
| `unparsed-attr-block` | warning | §3.2 |
| `label-invalid` | error | §3.3 |
| `missing-asset` | warning (`build`: error) | §4.3 |
| `asset-case-mismatch`, `invalid-dimension` | warning | §4.3 |
| `table-caption-orphan`, `table-caption-pandoc-colon` | warning | §4.4 |
| `equation-label-misplaced` | warning | §4.5 |
| `invalid-code-attr` | warning | §4.6 |
| `raw-unknown-format` | warning | §4.8 |
| `raw-skipped` (per target in `--target`) | info | §4.8 |
| `footnote-unreferenced` | warning | §4.1 |
| `unclosed-directive`, `invalid-directive`, `invalid-directive-arg` | warning | §5 |
| `directive-colon-rewritten` | info (warning with `--compat presenter`) | §5.3 |
| `invalid-reference-mode` | warning | §7.2 |
| `include-meta-ignored` | warning | §8 |
| `include-not-top-level` | error | §8 |
| `parse-error` (include failures, `Error::Parse`) | error | §8 |
| `presenter-separator` | info | §9 |
| `presenter-attr-dropped` | warning | §9 |
| `presenter-include-position`, `presenter-front-matter` | warning, only with `--compat presenter` | §9 |

Raised by resolve on this dialect's shapes (owned by references.md): `unresolved-reference`
(error in `build`, warning in `serve`/`check`), `duplicate-label`, `ambiguous-label` (errors),
`unnumbered-target` (warning). Per-target lints (`unsupported-construct`,
`math-conversion-failed`) are the emitters' (T71). `check` runs every lint above for every
included file too (§8). `--deny-warnings` fails on warnings, never on info events.

> **Contract change (coordinator, 2026-10-26):** the diagnostic list follows
> references.md §8: unknown targets raise `unresolved-reference`, including when
> no bibliography is configured. Resolve does not raise `unknown-citation-key`.

## 11. Fixtures (`tests/fixtures/md/`, inputs by T01 per PLAN §7; snapshots by T12/T1B)
| fixture | exercises |
|---|---|
| `minimal.md` | front matter with `title`/`authors`/`abstract`, one heading, one paragraph |
| `attributes.md` | heading/link/image/inline-code attributes, `.unnumbered`, `title=`, `{#"with space"}` → `label-invalid` |
| `attribute-recovery.md` | single quotes, `/` and `\|` in bare values, bare keys, commas, `{.a}{k='v'}` → recovery and `unparsed-attr-block` |
| `directives.md` | containers, longer outer fence nesting, leaf forms, `:::raw`, unknown names, `:::notes` dropped, empty `:::`, unclosed directive |
| `pandoc-directives.md` | `::: {.note title="…"}`, `::: {.pagebreak}`, closing colons, a colon after the fence (pre-pass), `:::callout {title="Fig 1: x"}` |
| `figures.md` | implicit figures with every attribute variant of R9 §14.4, `![](x.png)` stays inline, `:::figure` wrapper with sub-figures, `invalid-dimension` |
| `tables.md` | alignments, `Table:` before/after, orphan caption, pandoc `: caption` row, labelled table in a `Figure` |
| `callouts.md` | the five alert kinds, mixed-case tags, titles, `> [!UNKNOWN]`, `:::callout kind title=` |
| `code.md` | every row of §4.6, `~~~` fences, indented code, `caption=` + `#lst-x` listing figures, `invalid-code-attr` |
| `math.md` | inline/display dollars, `$$ … $$ {#eq-x}`, `` ```math {#eq-y} ``, misplaced label, `\$5`, math in headings and lists |
| `references.md` | `@sec-x`, `@fig-x[Fig.]`, `@eq-x{.page}`, `[@a; @b, p. 12]{.prose}`, `[-@a]`, `\@literal`, `@x.` trailing dot, `[see @a]` |
| `footnotes.md` | named, repeated, inline `^[…]`, multi-paragraph definitions, an unreferenced definition |
| `includes.md` | includes `include/part-a.md` (which includes `include/part-b.md`), `include/cycle-a.md` ↔ `cycle-b.md`, an include inside a list, `include-meta-ignored` |
| `raw.md` | `{=html}`, `{=latex}`, `{=typst}`, `{=openxml}`, `{=weird}`, raw HTML blocks with `<img src>`, comments, inline `{=html}` |
| `pagebreaks.md` | `:::pagebreak`, `:::toc depth=2`, `:::bibliography`, `***`/`---` breaks |
| `presenter-deck.md` | a presenter deck: `---`/`--` separators, `--- {layout=title}`, heading shorthands, `:::notes`, `:::fragment`, `<!-- .slide: -->` |

`images/` holds the files `figures.md`, `raw.md` and `includes.md` find. `tests/fixtures/md/probe/`
(T11) keeps R9 §14's probe corpus as a differential snapshot of comrak's AST dump. Each fixture's
IR (`Document` as JSON, paths and asset ids redacted, `--no-resolve` form) is an `insta`
snapshot `tests/snapshots/markdown_snapshots__<name>.snap` written by
`tests/markdown_snapshots.rs`, which snapshots every `*.md` in the directory (T12; T1B updates
`references.md` and `includes.md`); diagnostics are a second snapshot per fixture. `check`'s
fixtures are the same files (T1C asserts the codes of §10 per fixture).

## Open points (a task may decide locally)
- T11 (decided): `colon_prepass` and `recover_attr_blocks` run in sequence (§5.3).
- T12: the exact wording of messages.
- T1B: whether the scanner runs on comrak `Text` nodes or on IR `Inline::Text` (the rules of
  §7.2 are stated so that both give the same result).
- T1C: how `--compat presenter` recomputes include positions (a `---` line scan of the source).

> **Contract change (coordinator, 2026-10-25):** the T12 open point no longer lets T12 choose
> between a struct and a trait object for `Fragment`. §2.3's struct with public fields is binding,
> because T18 builds it by struct literal for every notebook markdown cell and the notebook
> abstract (notebook-mapping.md §2.2–2.3). No PLAN §5 dependency changes.

## Deviations from PLAN.md and the brief
- The brief says the colon pre-pass looks at **top-level** paragraphs; §5.3 also includes
  paragraphs that are direct children of a `BlockDirective`, so an inner line of a nested
  directive (`::::columns` / `:::column title="a: b"`) gets the same fix. PLAN §8 ("paragraphs
  whose first text starts with `:::`") does not restrict the depth; lists and quotes stay
  excluded as the brief intends.
- Info-level diagnostics (`directive-colon-rewritten`, `presenter-separator`, `raw-skipped`)
  use `Level::Info`, added to ir.md §5 by a coordinator contract change (2026-10-24) after this
  spec first routed them through `tracing`. `check --compat presenter` still recomputes the
  include-position and front-matter rows from the source (they are not raised during parsing).
