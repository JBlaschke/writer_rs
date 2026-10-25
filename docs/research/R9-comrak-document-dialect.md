# R9 — comrak as the parser of writer's document dialect

**Status:** research notes, 2026-10-24. Everything marked *(verified)* was read in comrak's
source or produced by the probe described in §14; nothing here is recalled from memory.

**Sources read**
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/comrak-0.56.0/src/{nodes.rs, parser/mod.rs, parser/inlines.rs, parser/attributes.rs, parser/autolink.rs, parser/options.rs, scanners.re, arena_tree.rs, lib.rs, html.rs, xml.rs}`
- same files in `…/comrak-0.57.0/` (downloaded by `cargo info comrak` on 2026-10-24), plus its `CHANGELOG.md`
- presenter's dialect spec: `/Users/johannes.blaschke/Developer/presenter_rs/docs/spec/markdown-dialect.md` §1 (comrak options), §3 (attributes), §4 (directives)
- GitHub issue search `repo:kivikakk/comrak directive` (api.github.com, 2026-10-24)
- probe crate `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/comrak-probe/` (session scratch; the dumps are copied into §14 so this file is self-contained)

---

## 0. Summary (the decisions this supports)

1. **Pin `comrak = { version = "0.57", default-features = false, features = ["attributes"] }`.**
   0.57.0 (released 2026-10-21) is the latest on crates.io; it differs from 0.56.0 only by
   two new `NodeValue` variants (`CommentInline`, `MultilineComment` behind the new
   `extension.ofm_comments`), a new `NodeWikiLink.embed: bool` field and dependency bumps.
   The attribute parser, autolink, table and block scanners are byte-identical, and every
   probe document in §14 produced an identical AST dump on 0.56.0 and 0.57.0 *(verified)*.
   MSRV is 1.89 for both; license BSD-2-Clause *(verified: Cargo.toml, `cargo info`)*.
2. comrak gives writer a complete **block/inline AST with per-node `sourcepos` always filled
   in** (the parser never consults `render.sourcepos`; that flag only affects the HTML/XML
   formatters) *(verified)* and **attributes on exactly five node kinds**: `Heading`,
   `CodeBlock` (fenced), `Code`, `Link`, `Image` *(verified: the only four call sites of
   `attributes::parse`/`parse_off`)*. Nothing else (paragraphs, tables, directives,
   thematic breaks, lists) can carry `{…}` attributes natively.
3. **Directives are container-only (`:::name … :::`).** There is no leaf (`::name`) or
   inline (`:name[…]{…}`) directive in 0.56/0.57 and no issue/PR proposing one *(verified)*.
   An **empty** container `:::pagebreak` + `:::` parses cleanly as a childless
   `BlockDirective`, even directly after a paragraph line, so that is the leaf form to use.
4. **The opening line of a directive may not contain a colon anywhere after the fence**
   (scanner `[:]{3,} / [^:\r\n\xff]*[\r\n\xff]`). `:::figure #fig:x` and
   `:::callout {title="Figure 1: results"}` are silently *not* directives; the paragraph
   that results is followed by a bare `:::` that *opens* an empty-info directive which
   swallows the rest of the document until the next `:::` *(verified in probe)*. Writer's
   directive syntax must therefore use colon-free labels (`{#fig-x}` / `label=fig-x`) or
   the `&#58;` entity (comrak HTML-unescapes the info string, `#fig&#58;x` → `#fig:x`,
   verified), and writer's `check` must lint `:::` lines that did not open a directive.
5. **Implicit figure**: `![Caption *emph*](a.png "T"){#fig:x width=50%}` alone on a line is
   `Paragraph > Image{url,title; attrs{id:"fig:x", pairs:[("width","50%")]}} > [Text, Emph…]`
   *(verified)*. The rule "paragraph whose only child is an `Image` ⇒ figure, caption = the
   image's inline children" is unambiguous. A colon in `#fig:x` is fine *inside* an
   attribute block (the grammar allows `:` in bare values) — only directive info lines
   reject colons.
6. **Attribute grammar limits are unchanged in 0.57** (`parser/attributes.rs` identical).
   Single-quoted values, bare values with `/ # | ,` etc., bare keys and comma-separated
   items make comrak reject the whole block: on blocks the `{…}` stays in the heading
   text / fence info, on inlines it becomes a trailing `Text("{…}")` sibling. Keep
   presenter's `parse_attr_block` fallback (presenter spec §3.1 A3 note).
7. Everything in pandoc's block/inline vocabulary that writer needs has a direct comrak
   node except: bracketed spans `[text]{.cls}` (plain `Text`, no support), pandoc's
   `: caption` line directly under a table (becomes an extra **table row** — do not
   support), definition-list sourcepos (buggy), and leaf directives (use empty containers).

---

## 1. Versions

| | 0.56.0 | 0.57.0 (latest, crates.io 2026-10-24) |
|---|---|---|
| released | 2026-10-06 | 2026-10-21 |
| `rust-version` | 1.89 | 1.89 |
| license | BSD-2-Clause | BSD-2-Clause |
| features (from `cargo info`) | — | `default = [cli, syntect-onig, bon]`; `attributes = []`; `bon`, `clap`, `cli`, `emojis`, `fmt2io`, `phoenix_heex`, `shell-words`, `shortcodes = [emojis]`, `syntect`, `syntect-onig`, `syntect-fancy`, `xdg`, `arbitrary` |
| local copy | `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/comrak-0.56.0/` | `…/comrak-0.57.0/` |

**0.57.0 changelog (verified, `CHANGELOG.md`):**
- "Support wikilink embeds" (#874) → `NodeWikiLink { url, embed: bool }` (new field).
- "Support OFM `%%` comment syntax, with the `ofm_comments` extension" (#876) →
  `NodeValue::CommentInline(NodeComment { literal })`,
  `NodeValue::MultilineComment(NodeCommentBlock { literal, closed })`,
  `Extension.ofm_comments: bool`.
- dependency bumps (toml, time, clap).

**Source diff 0.56.0 → 0.57.0** (`diff` line counts, verified): `parser/attributes.rs` 0,
`parser/autolink.rs` 0, `parser/table.rs` 0, `scanners.re` 0, `nodes.rs` ≈ +45 (the items
above), `parser/options.rs` +18 (`ofm_comments` docs/field), `parser/mod.rs` 104,
`parser/inlines.rs` 103 (comment parsing + a refactor of `last_line_blank` bookkeeping),
`html.rs` 9, `cm.rs` 52, `xml.rs` 19. Probe dumps identical (§14).

**Consequence for the fleet:** an exhaustive `match` on `NodeValue` written against 0.57
must include `CommentInline(_)` and `MultilineComment(_)` (or a `_` arm); with
`ofm_comments = false` (default) they are never produced.

**0.56.0 notes relevant to writer** (from its changelog, verified): MSRV bump to 1.89;
`tagfilter` extension removed; several sourcepos fixes (multiline inline sourcepos, table
row sourcepos, front matter CR counting); "Zero-end columns adopt the widest end position
in the path" (#868) — yet the probes still show a few `…-N:0` end columns, see §12.

---

## 2. Recommended `Options` for writer (exact field names, all verified in `options.rs`)

```rust
// Cargo.toml:  comrak = { version = "0.57", default-features = false, features = ["attributes"] }
use comrak::Options;

pub fn comrak_options(smart: bool) -> Options<'static> {
    let mut o = Options::default();                  // Options: #[derive(Default, Debug, Clone)]
    // --- presenter's set (presenter spec §1) ---
    o.extension.front_matter_delimiter = Some("---".into());
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.tasklist = true;
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.extension.description_lists = true;
    o.extension.alerts = true;                 // > [!NOTE] Title
    o.extension.math_dollars = true;           // $x$, $$x$$
    o.extension.math_code = true;              // $`x`$ and ```math fences
    o.extension.block_directive = true;        // :::name … :::
    o.extension.header_attributes = true;      // # H {#id .c k=v}   (ATX and setext)
    o.extension.fenced_code_attributes = true; // ```lang {#id .c k=v}
    o.extension.link_attributes = true;        // [t](u){…}  ![a](u){…}
    o.extension.inline_code_attributes = true; // `c`{…}
    // --- writer additions ---
    o.extension.superscript = true;            // x^2^        -> NodeValue::Superscript
    o.extension.subscript = true;              // H~2~O       -> NodeValue::Subscript (~~x~~ stays Strikethrough)
    o.extension.multiline_block_quotes = true; // >>> … >>>  and  >>> [!TIP] … >>> multi-paragraph alerts
    o.parse.leave_footnote_definitions = true; // keep FootnoteDefinition nodes in place; keep unreferenced ones (total_references == 0) so `check` can warn
    // optional, decide in the dialect spec:
    // o.extension.inline_footnotes = true;    // ^[text]  -> FootnoteReference{name:"__inline_1"} + synthetic FootnoteDefinition
    // o.extension.math_latex = true;          // \( \) \[ \] -> Math nodes (dollar_math is still reported true, see §11)
    // o.extension.highlight = true;           // ==mark==  -> Highlight ;  o.extension.insert = true; // ++ins++ -> Insert
    // --- parse ---
    o.parse.smart = smart;                     // default false (keeps -- literal); true turns "quotes"/--/... into Unicode inside Text
    o.parse.relaxed_autolinks = false;
    o.parse.sourcepos_chars = false;           // columns in UTF-8 bytes (cmark compatible); true = chars
    // --- render (only matters if format_html/format_xml are ever called; writer walks the AST) ---
    o.render.r#unsafe = true;
    o.render.escape = false;
    o.render.hardbreaks = false;
    o.render.github_pre_lang = false;
    o.render.full_info_string = false;
    o.render.sourcepos = false;                // does NOT influence the AST (see §12)
    o
}
```

**Do not enable** (verified behaviour/doc): `underline` (`__text__` becomes `<u>`, i.e. it
takes over the double-underscore strong form — doc example `__underlined text__` →
`<p><u>underlined text</u></p>`; not probed, so the exact interaction with `**strong**` is
unverified), `spoiler` (`||x||`), `greentext`, `subtext` (`-# x`), `wikilinks_*`,
`cjk_friendly_emphasis` (harmless, but not needed), `phoenix_heex`/`shortcodes` (need cargo
features), `header_id_prefix` (HTML-render only), `image_url_rewriter`/`link_url_rewriter`
(they only run inside the HTML formatter — `URLRewriter::to_html(&self, url) -> String`;
writer rewrites URLs on its own IR instead).

---

## 3. The AST types (copied from `comrak-0.56.0/src/nodes.rs`; 0.57 deltas marked)

### 3.1 `NodeValue` — every variant and payload *(verified)*

```rust
pub enum NodeValue {
    Document,                                   // root; contains blocks
    FrontMatter(String),                        // raw block INCLUDING the delimiters and the blank line(s) that follow (see §17)
    BlockQuote,                                 // contains blocks
    List(NodeList),                             // contains Item / TaskItem
    Item(NodeList),                             // contains blocks; NodeList copied per item (start = this item's ordinal; tight is NOT meaningful here)
    DescriptionList,                            // contains DescriptionItem
    DescriptionItem(NodeDescriptionItem),       // contains DescriptionTerm + DescriptionDetails
    DescriptionTerm,                            // contains blocks (a Paragraph)
    DescriptionDetails,                         // contains blocks
    CodeBlock(Box<NodeCodeBlock>),              // fenced or indented; no children
    HtmlBlock(NodeHtmlBlock),                   // raw; no children
    #[cfg(feature = "phoenix_heex")] HeexBlock(Box<NodeHeexBlock>),
    Paragraph,                                  // contains inlines
    Heading(NodeHeading),                       // contains inlines
    ThematicBreak,                              // no children
    FootnoteDefinition(NodeFootnoteDefinition), // contains blocks
    Table(Box<NodeTable>),                      // contains TableRow
    TableRow(bool),                             // true = header row; contains TableCell
    TableCell,                                  // contains inlines
    Text(Cow<'static, str>),
    TaskItem(NodeTaskItem),                     // REPLACES Item for "- [ ]" / "- [x]" items (block; contains blocks)
    SoftBreak,
    LineBreak,
    Code(NodeCode),                             // inline code span; no children
    HtmlInline(String),
    #[cfg(feature = "phoenix_heex")] HeexInline(String),
    Raw(String),                                // programmatic only, never parsed
    Emph,
    Strong,
    Strikethrough,
    Highlight,                                  // ==x==   (extension.highlight)
    Insert,                                     // ++x++   (extension.insert)
    Superscript,                                // x^2^    (extension.superscript)
    Link(Box<NodeLink>),                        // contains inlines
    Image(Box<NodeLink>),                       // contains inlines = the alt text
    FootnoteReference(Box<NodeFootnoteReference>),
    #[cfg(feature = "shortcodes")] ShortCode(Box<NodeShortCode>),
    Math(NodeMath),                             // inline node, also for $$ display math (see §11)
    MultilineBlockQuote(NodeMultilineBlockQuote), // >>> … >>>
    Escaped,                                    // only with parse.escaped_char_spans; contains one Text
    WikiLink(NodeWikiLink),
    Underline,                                  // __x__   (extension.underline)
    Subscript,                                  // ~x~     (extension.subscript)
    SpoileredText,                              // ||x||   (extension.spoiler)
    EscapedTag(&'static str),                   // spoiler extension internals
    Alert(Box<NodeAlert>),                      // > [!NOTE] …  (extension.alerts); contains blocks
    Subtext,                                    // -# x     (extension.subtext); contains inlines
    BlockDirective(Box<NodeBlockDirective>),    // :::name … :::  (extension.block_directive); contains blocks
    // 0.57.0 additions:
    CommentInline(NodeComment),                 // %%x%%        (extension.ofm_comments)
    MultilineComment(NodeCommentBlock),         // %% … %% block (extension.ofm_comments)
}
```

### 3.2 Payload structs *(verified, copied)*

```rust
pub struct NodeList {
    pub list_type: ListType,       // enum ListType { #[default] Bullet, Ordered }
    pub marker_offset: usize,      // spaces before the marker
    pub padding: usize,            // chars from marker start to item text (incl. marker)
    pub start: usize,              // ordered: ordinal (on List = first; on Item = that item's)
    pub delimiter: ListDelimType,  // enum ListDelimType { #[default] Period, Paren }
    pub bullet_char: u8,           // b'-' (45), b'*', b'+'; 0 for ordered
    pub tight: bool,               // meaningful on List only (Item copies show false, verified)
    pub is_task_list: bool,        // set on List when any item became TaskItem
}
pub struct NodeDescriptionItem { pub marker_offset: usize, pub padding: usize, pub tight: bool }
pub struct NodeCodeBlock {
    pub fenced: bool, pub fence_char: u8 /* b'`'=96 or b'~'=126 */, pub fence_length: usize,
    pub fence_offset: usize,
    pub info: String,     // info string AFTER the trailing `{…}` attribute block is stripped (fenced_code_attributes)
    pub literal: String,  // contents, keeps the trailing "\n"
    pub closed: bool,     // fenced: closed by a fence; indented: always true
}
pub struct NodeHeading { pub level: u8, pub setext: bool, pub closed: bool /* ATX had trailing #s */ }
pub struct NodeHtmlBlock { pub block_type: u8, pub literal: String }
//  block_type (scanners.re html_block_start, CommonMark §4.6): 1 <script|pre|textarea|style>, 2 "<!--" comment,
//  3 "<?", 4 "<!X", 5 "<![CDATA[", 6 known block tag (<div>, <figure>, …), 7 any other complete open/close tag alone on its line
pub struct NodeFootnoteDefinition { pub name: String, pub total_references: u32 }
pub struct NodeFootnoteReference {
    pub name: String,
    pub texts: Vec<(String, usize)>, // original text pieces, e.g. [("^",1),("one",3)]; empty for inline footnotes
    pub ref_num: u32,                // n-th reference to THIS footnote (1-based)
    pub ix: u32,                     // index of the footnote in the document (1-based, order of first reference)
}
pub struct NodeTable { pub alignments: Vec<TableAlignment>, pub num_columns: usize, pub num_rows: usize, pub num_nonempty_cells: usize }
pub enum TableAlignment { None, Left, Center, Right }
pub struct NodeTaskItem { pub symbol: Option<char> /* None = "[ ]" */, pub symbol_sourcepos: Sourcepos }
pub struct NodeCode { pub num_backticks: usize, pub literal: String }
pub struct NodeLink { pub url: String, pub title: String }           // title "" when absent
pub struct NodeWikiLink { pub url: String /* 0.57: */, pub embed: bool }
pub struct NodeMath { pub dollar_math: bool /* false = $`code`$ */, pub display_math: bool /* $$ */, pub literal: String }
pub struct NodeMultilineBlockQuote { pub fence_length: usize, pub fence_offset: usize }
pub struct NodeAlert {
    pub alert_type: AlertType,     // enum AlertType { #[default] Note, Tip, Important, Warning, Caution }
    pub title: Option<String>,     // text after "[!type]" on the same line, entity- and backslash-unescaped, trimmed; None if empty
    pub multiline: bool,           // from ">>> [!type]" (needs extension.multiline_block_quotes)
    pub fence_length: usize, pub fence_offset: usize,
}
impl AlertType { pub fn default_title(&self) -> &'static str /* "Note","Tip","Important","Warning","Caution" */;
                 pub fn css_class(&self, style: AlertStyleType) -> &'static str }
pub struct NodeBlockDirective { pub fence_length: usize, pub fence_offset: usize, pub info: String }
// 0.57:
pub struct NodeComment { pub literal: String }
pub struct NodeCommentBlock { pub literal: String, pub closed: bool }

#[cfg(feature = "attributes")]
pub struct Attributes {              // comrak::nodes::Attributes  (Default, Debug, Clone, PartialEq, Eq)
    pub id: Option<String>,          // "#id" — last one wins
    pub classes: Vec<String>,        // ".a .b"
    pub pairs: Vec<(String, String)>,// "k=v" in source order; duplicates are kept
}
```

### 3.3 `Ast`, `Sourcepos`, `LineColumn`, node/arena types *(verified)*

```rust
pub struct Ast {
    pub value: NodeValue,
    pub sourcepos: Sourcepos,
    #[cfg(feature = "attributes")] pub attrs: Option<Box<Attributes>>,
    // private: open, last_line_blank, table_visited, lines
}
pub struct Sourcepos { pub start: LineColumn, pub end: LineColumn }   // Display: "L:C-L:C"; both inclusive, 1-based
pub struct LineColumn { pub line: usize, pub column: usize }          // column in UTF-8 bytes unless parse.sourcepos_chars
impl LineColumn { pub fn column_add(&self, offset: isize) -> LineColumn }
impl Ast { pub fn new(value: NodeValue, start: LineColumn) -> Self;
           pub fn new_with_sourcepos(value: NodeValue, sourcepos: Sourcepos) -> Self }

pub type AstNode<'a> = arena_tree::Node<'a, RefCell<Ast>>;
pub type Node<'a> = &'a AstNode<'a>;                        // re-exported as comrak::Node
pub type Arena<'a> = typed_arena::Arena<nodes::AstNode<'a>>; // comrak::Arena
```

`arena_tree::Node` API *(verified, `arena_tree.rs`)*: `parent()`, `first_child()`,
`last_child()`, `previous_sibling()`, `next_sibling()` (all `-> Option<&'a Node>`),
`same_node(&other) -> bool`, iterators `ancestors()`, `preceding_siblings()`,
`following_siblings()`, `children()`, `reverse_children()`, `descendants()` (self first,
depth-first), `traverse()`/`reverse_traverse()` (enter/exit events); mutation `detach()`,
`append(child)`, `extend(iter)`, `prepend(child)`, `insert_after(sib)`, `insert_before(sib)`;
data access `data() -> Ref<Ast>`, `try_data()`, `data_mut() -> RefMut<Ast>`. On `AstNode`:
`can_contain_type(&NodeValue) -> bool`, `validate() -> Result<(), ValidationError>`,
`collect_text() -> String` (Text/Code/Math literals, breaks as one space),
`collect_text_append(&mut String)`. `NodeValue` helpers: `block() -> bool`,
`contains_inlines() -> bool` (Paragraph, Heading, TableCell, Subtext), `text() -> Option<&str>`,
`xml_node_name() -> &'static str` (the names used in the dumps below).

### 3.4 Containment rules the walker can rely on (`can_contain_type`, verified)

- `Document`, `BlockQuote`, `FootnoteDefinition`, `DescriptionTerm`, `DescriptionDetails`,
  `Item`, `TaskItem`, `MultilineBlockQuote`, `Alert`, `BlockDirective`: any block except
  `Item`/`TaskItem`.
- `List`: only `Item` / `TaskItem`. `DescriptionList`: only `DescriptionItem`.
  `DescriptionItem`: `DescriptionTerm` | `DescriptionDetails`.
- `Paragraph`, `Heading`, `Emph`, `Strong`, `Link`, `Image`, `Strikethrough`, `Highlight`,
  `Insert`, `Superscript`, `Subscript`, `Underline`, `SpoileredText`, `Subtext`: inlines only.
- `Table` → `TableRow` → `TableCell` → inlines (Text, Code, Emph, Strong, Link, Image,
  Strikethrough, Highlight, Insert, HtmlInline, Math, WikiLink, FootnoteReference,
  Superscript, SpoileredText, Underline, Subscript, TaskItem, Escaped, EscapedTag).
- Leaves (never have children): `FrontMatter`, `CodeBlock`, `HtmlBlock`, `ThematicBreak`,
  `Text`, `SoftBreak`, `LineBreak`, `Code`, `HtmlInline`, `Raw`, `FootnoteReference`,
  `Math`. (`Image` with empty alt also has no children, verified.)

---

## 4. Extensions relevant to documents — option name → AST effect *(all verified)*

| `Options.extension.*` | syntax | AST | probe notes |
|---|---|---|---|
| `front_matter_delimiter: Option<String>` (`Some("---")`) | `---`…`---` at line 1 | `FrontMatter(String)` as first child of Document | literal = `"---\ntitle: …\n---\n\n"` (delimiters + following blank line included); `sourcepos 1:1-4:3`; not at top ⇒ `ThematicBreak` + setext heading (§17) |
| `table` | GFM pipe tables | `Table(NodeTable{alignments,num_columns,num_rows,num_nonempty_cells}) > TableRow(is_header) > TableCell > inlines` | alignments `[Left, Center, Right, None]` for `:--`, `:-:`, `--:`, `---`; a non-blank line right after the table (e.g. pandoc `: caption`) becomes another row |
| `strikethrough` | `~~x~~` | `Strikethrough` | coexists with `subscript` (`~x~`) |
| `subscript` | `~x~` | `Subscript` | verified `H~2~O` → `Text("H") Subscript[Text("2")] Text("O…")` |
| `superscript` | `x^2^` | `Superscript` | |
| `tasklist` | `- [ ] / - [x] / - [X]` | the `Item` is **replaced** by `TaskItem(NodeTaskItem{symbol: None/Some('x')/Some('X'), symbol_sourcepos})`; parent `List.is_task_list = true` | `parse.relaxed_tasklist_matching` admits other symbols; `parse.tasklist_in_table` puts a `TaskItem` inline inside a cell |
| `autolink` | bare `www.`, `http(s)://`, emails | `Link{url:"http://www…"/"mailto:…"} > Text` | `@fig:x` never links (§9) |
| `footnotes` | `[^n]` / `[^n]: …` | `FootnoteReference{name,ref_num,ix}`; `FootnoteDefinition{name,total_references} > blocks` | definitions are **moved to the end** of Document and unreferenced ones **deleted** unless `parse.leave_footnote_definitions = true` (§10) |
| `inline_footnotes` (needs `footnotes`) | `^[text]` | `FootnoteReference{name:"__inline_1", texts:[]}` + synthetic `FootnoteDefinition{name:"__inline_1"}` with bogus sourcepos `1:1-1:0` | |
| `description_lists` | `Term\n\n: details` | `DescriptionList > DescriptionItem(NodeDescriptionItem) > DescriptionTerm > Paragraph`, `DescriptionDetails > blocks` | docs say "Not (yet) compatible with render.sourcepos"; term sourcepos is wrong when there is no blank line before `:` (§15) |
| `alerts` | `> [!NOTE] Title` | `Alert(NodeAlert{alert_type,title,…}) > blocks` | tag is **case-insensitive** (`re2c:case-insensitive = 1` in scanners.re; `[!NOTE]`, `[!Important]` verified); unknown tag ⇒ plain `BlockQuote` with `Text("[!UNKNOWN]")`; `>> [!NOTE]` ⇒ `BlockQuote > Alert`; `>>> [!TIP]` needs `multiline_block_quotes` (else not an alert) |
| `multiline_block_quotes` | `>>>` … `>>>` | `MultilineBlockQuote{fence_length,fence_offset} > blocks`; with alerts: `Alert{multiline:true}` | |
| `math_dollars` | `$x$`, `$$x$$` | `Math{dollar_math:true, display_math:false/true, literal}` — always an **inline** node inside a Paragraph (§11) | `$5 and $6` stays text |
| `math_code` | `` $`x`$ `` and ```` ```math ```` | `Math{dollar_math:false,…}` / `CodeBlock{info:"math"}` | writer maps `CodeBlock{info=="math"}` to display math |
| `math_latex` | `\(x\)`, `\[x\]` | `Math{dollar_math:true, display_math:false/true}` (the flag cannot distinguish the delimiter) | without it `\(a\)` is `Text("(a)")` (backslash escapes) |
| `block_directive` | `:::name info` … `:::` | `BlockDirective{fence_length,fence_offset,info} > blocks` | §7 |
| `header_attributes` | `# H {…}` / setext | `attrs` on `Heading` | block must be last on the line, preceded by whitespace |
| `fenced_code_attributes` | ```` ```lang {…} ```` | `attrs` on `CodeBlock`, `info` stripped of the block | braces required: ```` ```python title="A: B" ```` keeps `info:"python title=\"A: B\""`, no attrs |
| `link_attributes` | `[t](u){…}`, `![a](u){…}`, `[t][ref]{…}` | `attrs` on `Link`/`Image` | `{` must immediately follow `)`/`]`; multi-line blocks allowed |
| `inline_code_attributes` | `` `c`{…} `` | `attrs` on `Code` | |
| `highlight` / `insert` | `==x==` / `++x++` | `Highlight` / `Insert` | not probed; doc-verified HTML `<mark>`/`<ins>` |
| `underline` | `__x__` | `Underline` | doc-verified; takes the double-underscore form |
| `spoiler`, `greentext`, `subtext`, `wikilinks_title_after_pipe`, `wikilinks_title_before_pipe`, `cjk_friendly_emphasis`, `shortcodes` (feature), `phoenix_heex` (feature), `ofm_comments` (0.57) | — | `SpoileredText`/`EscapedTag`, (greentext: parse rule only), `Subtext`, `WikiLink`, —, `ShortCode`, `HeexBlock/HeexInline`, `CommentInline/MultilineComment` | not needed for writer |
| `image_url_rewriter` / `link_url_rewriter: Option<Arc<dyn URLRewriter + 'c>>` | — | **HTML formatter only**; the AST keeps the original url | `trait URLRewriter: RefUnwindSafe + Send + Sync { fn to_html(&self, url: &str) -> String }`, blanket impl for `Fn(&str) -> String` |
| `header_id_prefix`, `header_id_prefix_in_href` | — | HTML formatter only | |

`Options.parse.*` *(verified)*: `smart: bool` (converts quotes, `--`, `---`, `...` to
`“” ‘’ – — …` inside `Text`; verified), `default_info_string: Option<String>`,
`relaxed_tasklist_matching`, `tasklist_in_table`, `relaxed_autolinks`, `ignore_setext`
(`Para\n---` → paragraph + `ThematicBreak` instead of `<h2>`; worth enabling if writer wants
`---` to be safe — see open questions), `broken_link_callback: Option<Arc<dyn BrokenLinkCallback>>`
(`fn resolve(&self, BrokenLinkReference{normalized,original}) -> Option<ResolvedReference{url,title}>`),
`leave_footnote_definitions`, `escaped_char_spans` (wraps escaped chars in `Escaped > Text`),
`sourcepos_chars`.

`Options.render.*` relevant only if comrak's formatters are used: `sourcepos`,
`figure_with_caption` (HTML: `<figure><img…><figcaption>title</figcaption></figure>` from the
*title*, not the alt — not what writer wants), `alert_style: AlertStyleType { Specific, Semantic }`,
`list_style: ListStyleType { Dash=45, Plus=43, Star=42 }`, `tasklist_classes`,
`ignore_empty_links`, `gfm_quirks`, `prefer_fenced`, `ol_width`, `compact_html`, `width`,
`hardbreaks`, `github_pre_lang`, `full_info_string`, `r#unsafe`, `escape`, `escaped_char_spans`,
`experimental_minimize_commonmark`.

---

## 5. Attributes: grammar, where they attach, limits *(verified in `parser/attributes.rs`, identical in 0.57)*

**Grammar of `attributes::parse(input: &str) -> Option<(Attributes, usize)>`** (input starts at `{`):
- Between items: ` `, `\t`, `\r`, `\n` allowed (so multi-line blocks work, verified for inline code).
- Item starts: `#` → id, `.` → class, `[a-zA-Z]` → key, `}` → end. **Anything else ⇒ whole block rejected** (so `,`, `{`, `'`, digits-first keys, `-`, `@` as an item start are fatal).
- Key chars: `[a-zA-Z0-9-_:.]`, then `=` is mandatory (`{k}` ⇒ None, `{k=}` ⇒ None, `{k=""}` ⇒ `("k","")`).
- Bare value chars: `[a-zA-Z0-9-_:.%]` only — `/`, `#`, `|`, `,`, `'`, `(`, `@`, `&`, `=`, `+`, `!`, `?`, `*` or non-ASCII ⇒ **None**. A bare value may not be empty (`{#}`, `{.}` ⇒ None).
- Quoted value: only `"…"`, must start right after `#`/`.`/`=`; `\` escapes the next char; newlines inside quotes not permitted; after the closing quote a whitespace or `}` is required.
- `{}` ⇒ `Some(Attributes::default())` (empty attrs, verified on `## Empty {}`).
- Semantics: id = last `#` wins; classes and pairs in source order.

**Block form `attributes::parse_off(content: &mut String) -> Option<Attributes>`** (headings and fence info strings): scans backwards from the end; the block must be the last thing on the line (trailing whitespace ok), the `{` must be preceded by **whitespace** (`Well!{#yay}` ⇒ None, verified by comrak's own test), the block is removed from the content together with the whitespace before it. For a heading only the **last** block is read: `## Two blocks {.a} {k="v"}` ⇒ `attrs{pairs:[("k","v")]}` and `Text("Two blocks {.a}")` (verified). A block in the middle (`## Middle {.a} text`) stays text.

**Inline form** (`handle_potential_attribute`): called right after a code span (`inline_code_attributes`) and right after a link/image closes (`link_attributes`); the `{` must be the very next byte (no space). On failure nothing is consumed and the `{…}` becomes part of the following `Text` node (verified: `` `y`{k='v'} `` ⇒ `Code{"y"}` + `Text("{k='v'} and ")`).

**Nodes that can carry `Ast.attrs`:** `Heading` (ATX and setext), `CodeBlock` (fenced only), `Code`, `Link` (inline, reference, collapsed, shortcut), `Image` — and nothing else *(verified: the four call sites in `parser/mod.rs:2201,2263` and `parser/inlines.rs:477,2038`)*. Nested works: `[![alt](a.png){.x width=10%}](https://u){.wrap}` gives attrs on both (verified).

**Rejected forms observed in the probe (block stays as text):** `{#fig:y width='50%'}` (single quotes), `{src=figures/bg.png}` (`/`), `{k}` (bare key), `{a=1, b=2}` (comma). Accepted: `{k=a:b data-x=1.5%}`, `{#"with space" .x}`, `{#c} ##` (closed ATX), `{ .multi\n k="1" }` across lines.

**Recommendation:** keep presenter's `markdown::attrs::parse_attr_block` tokenizer as the fallback exactly as presenter spec §3.1 describes (trailing `Text` that is a `{…}` after `Image`/`Link`/`Code`; trailing `{…}` in heading text; trailing `{…}` in `CodeBlock.info`), emitting presenter's `unparsed-attr-block` warning when even the fallback fails. The 0.57 grammar is identical, so nothing changes.

---

## 6. Directives *(verified in `scanners.re`, `parser/mod.rs`, probe)*

### 6.1 What comrak implements
Only **container block directives**:

```
scanners.re:
  open_block_directive_fence:  [:]{3,} / [^:\r\n\xff]*[\r\n\xff]    -> Some(fence_length)
  close_block_directive_fence: [:]{3,} / [ \t]*[\r\n\xff]           -> Some(fence_length)
```

- Opening: ≥3 colons at indent ≤ 3; **the rest of the line may not contain another `:`**
  (that is the `[^:\r\n\xff]*` lookahead). Info = rest of the line, HTML-entity-unescaped,
  trimmed, backslash-unescaped (`strings::unescape`, punctuation only: `my\ dir` keeps the
  backslash, `\"` would not). `NodeBlockDirective { fence_length, fence_offset: indent, info }`.
- A directive line **interrupts a paragraph** (no blank line needed before it, verified).
- Closing: a line of ≥ `fence_length` colons (optionally followed by spaces) at indent ≤ 3.
  `parse_block_directive_prefix` is evaluated outermost-first, so **a closing fence closes
  the outermost open directive whose `fence_length` ≤ the closer's length**. Nested
  directives therefore need a **longer outer fence** (`::::columns` / `:::column` …
  `:::` … `::::`, verified); same-length nesting closes the outer one at the first `:::`
  (verified: `:::columns/:::column/left/:::` ⇒ columns ends at line 4, the second
  `:::column` becomes a sibling, and the final `:::` opens a new empty-info directive).
- An unclosed directive swallows everything to EOF (verified).
- There are **no leaf (`::name`) or inline (`:name[…]{…}`) directives** in 0.56 or 0.57:
  grep for `LeafDirective|InlineDirective|leaf.directive|inline.directive` over 0.57 `src/`
  finds only a phoenix_heex test comment; GitHub issues/PRs mentioning directives are
  #780 "Add fenced container extension", #782 "Add block directive extension", #788, #791,
  #836 — all container-only (verified via api.github.com search on 2026-10-24).

### 6.2 The colon trap (most important finding for the dialect)

| input line | parses as |
|---|---|
| `:::figure #fig:wrapped` | **Paragraph** `Text(":::figure #fig:wrapped")`; the later bare `:::` opens `BlockDirective{info:""}` that swallows everything up to the next `:::`/EOF |
| `:::callout {.note title="Figure 1: results"}` | same failure (colon inside quotes counts too) |
| `:::note: colon after fence` | same failure |
| `:::figure #fig&#58;wrapped` | `BlockDirective{info:"figure #fig:wrapped"}` (entity unescape happens after the scan) |
| `:::figure {#fig-wrapped .wide width=80%}` | `BlockDirective{info:"figure {#fig-wrapped .wide width=80%}"}` |
| `::: {.pagebreak}` | `BlockDirective{info:"{.pagebreak}"}` |
| `:::include "my dir/part 2.md"` | `info:"include \"my dir/part 2.md\""` (quotes kept, writer splits) |

Rules for writer's dialect that follow: (a) ids/labels in directive info lines are
colon-free (`fig-x`, or `label=fig-x`), or users write `&#58;`; (b) `check` must warn on
every source line matching `^\s{0,3}:{3,}` that did not become a `BlockDirective` start or
end (presenter's `invalid-directive` lint, spec §4 A3 note), because the failure mode is
silent swallowing; (c) nested directives use longer outer fences (document it, as presenter does).

### 6.3 Leaf directive options (how a document expresses a page break, TOC, etc.)

| candidate | AST (verified) | verdict |
|---|---|---|
| `:::pagebreak` + `:::` (empty body) | `BlockDirective{info:"pagebreak"}` with **no children**, between paragraphs or even without surrounding blank lines | **use this**: unambiguous, carries args (`:::pagebreak landscape`, `:::toc depth=2`, `::: {.pagebreak}` all verified); writer treats "known leaf name + no children" as a leaf and "known leaf name + children" as an error |
| `::pagebreak` | `Paragraph > Text("::pagebreak")` | would need writer's own regex on paragraph text; indistinguishable from prose by comrak |
| `\newpage` | `Paragraph > Text("\\newpage")` (`n` is not escapable, backslash kept) | detectable by exact-text match; acceptable only as a LaTeX-flavoured alias |
| `<!-- pagebreak -->` | `HtmlBlock{block_type:2, literal:"<!-- pagebreak -->\n"}` | clean and unambiguous (comment blocks are type 2); good for reveal-style compat comments, but opaque to readers |
| `--- {.pagebreak}` | `Paragraph > Text("--- {.pagebreak}")` | thematic breaks cannot take attributes; only via a text regex (presenter does this for slide separators) |
| `***` / `---` | `ThematicBreak` | ambiguous with a horizontal rule; `Para\n--` / `Para\n---` become setext headings (verified) unless `parse.ignore_setext` |

---

## 7. Implicit figures and captions *(verified)*

```
![Caption with *emph*](a.png "The title"){#fig:x width=50%}
→ paragraph @1:1-1:59
    image Image(NodeLink { url: "a.png", title: "The title" }) attrs=Attributes { id: Some("fig:x"), classes: [], pairs: [("width", "50%")] } @1:1-1:59
      text Text("Caption with ") ; emph > text Text("emph")
```

- Figure rule: `Paragraph` whose **only** child is `Image` ⇒ `Figure { id/classes/pairs:
  image.attrs, src: url, title, caption: image's inline children }`. Verified non-figures:
  `![Caption](a.png){#fig:z .cls} trailing text` (Image + Text), `![One](a.png) ![Two](b.png)`
  (Image, Text(" "), Image — a candidate for a "subfigure row" rule), `[![…](a.png)](u)`
  (Link > Image).
- Attribute failure (`{#fig:y width='50%'}`, `{src=figures/bg.png}`) yields
  `Paragraph > [Image(no attrs), Text("{…}")]`; the fallback must (1) try
  `parse_attr_block` on a trailing `Text` matching `^\{.*\}$`, (2) then re-apply the
  sole-child rule.
- `![](noalt.png){.bare}` ⇒ `Image` with attrs and **no children** (empty caption).
- Images inside directives keep working: `:::figure {#fig-wrapped .wide width=80%}` ⇒
  `BlockDirective > [Paragraph > Image, Paragraph > Text("The caption paragraph.")]` — the
  wrapper form for multi-image figures or long captions (id must be colon-free, §6.2).
- **Table captions:** a `Table: …` paragraph after a blank line is an ordinary
  `Paragraph > Text("Table: The table caption {#tbl:t}")`, so writer detects it as "a
  Paragraph adjacent to a Table whose first inline is `Text` starting with `Table:`"
  (pandoc accepts it before or after the table) and parses the trailing `{#tbl:t}` itself
  (`{…}` in paragraph text is never parsed by comrak). The pandoc colon form (`: caption`
  on the line right after the table) is **broken**: it becomes a third `TableRow` whose
  first cell is `Text(": pandoc colon caption")` (verified) — reject/lint it.
- comrak's own `render.figure_with_caption` is HTML-only and uses the *title* as caption; irrelevant.

---

## 8. Autolink and the `@` rule *(verified in `parser/autolink.rs` `email_match`)*

```rust
const EMAIL_OK_SET: [bool; 256] = character_set!(b".+-_");
// rewind: walk LEFT from the '@' over [alnum | . + - _] (and a "mailto:"/"xmpp:" scheme);
if rewind == 0 { return None; }               // nothing link-like before '@' => no link
// forward: alnum, '-', '_' allowed; '.' only if followed by alnum (counts as np); a second '@' => None; anything else stops
if link_end < 2 || np == 0 || (!isalpha(last) && last != b'.') { return None; }   // needs at least one "x.y" dot
```

Consequences (all verified in the probe): `see @fig:x`, `(@tbl:y)`, `[@cite2020; @other]`,
`-@sec:intro`, and `@fig:x` at line start all stay `Text` — a pandoc-crossref/citation
syntax on `Text` nodes is safe. `me@example.com` ⇒ `Link{url:"mailto:me@example.com"}`,
`www.example.com` ⇒ `http://www.example.com`, `https://x.y/z` ⇒ link. Caveat: a reference
glued to a word with a dotted id (`ref@fig.a`) would autolink (`-`, `_`, `.` and alnum
before `@`, a dot after) — keep ids colon-separated (`fig:x`), which never match.
`<https://auto.link>`, `<me@x.y>` (CommonMark autolinks) are `Link` nodes without the
extension. `parse.relaxed_autolinks = false` keeps `[https://foo.com]` as text.

---

## 9. Footnotes *(verified)*

- `FootnoteReference { name, texts, ref_num, ix }`: `ix` numbers footnotes by first
  reference (1-based), `ref_num` counts references to the same footnote (`[^b] … [^b]` ⇒
  `ref_num` 1 and 2; `ix` 1 both times).
- Default (`parse.leave_footnote_definitions = false`): `process_footnotes` detaches every
  `FootnoteDefinition`, re-appends the referenced ones at the **end** of `Document` sorted by
  `ix`, and **drops unreferenced ones silently**; `total_references` is filled in.
- `leave_footnote_definitions = true`: definitions stay in place in source order, unreferenced
  ones stay with `total_references: 0` ⇒ writer can lint them. Recommended for writer (the
  walker collects definitions into a map by `name` anyway).
- Inline footnotes (`inline_footnotes`): synthetic definition with name `__inline_N`,
  `texts: []`, sourcepos `1:1-1:0` — use the reference's sourcepos for diagnostics.

---

## 10. Math *(verified)*

| source | AST |
|---|---|
| `$a$` | `Math{dollar_math:true, display_math:false, literal:"a"}` inline |
| `$$b$$` mid-paragraph | `Math{display_math:true, literal:"b"}` **inline, inside the paragraph** |
| `$$\nc\n$$` on its own lines | `Paragraph > Math{display_math:true, literal:"\nc\n"}` — the paragraph's **sole child**; same inside a list item or block quote |
| `$$d$$` alone on a line | `Paragraph > Math{display_math:true, literal:"d"}` sole child |
| `` $`e`$ `` | `Math{dollar_math:false, display_math:false, literal:"e"}` |
| ```` ```math\nf\n``` ```` | `CodeBlock{info:"math", literal:"f\n"}` |
| `\(a\)`, `\[b\]` with `math_latex` | `Math{dollar_math:true, …}` (same shape as dollars; display for `\[`) |
| `$5 and $6` | `Text` (pandoc heuristics: no space after opening `$`, no digit after closing) |
| `\$5` | `Text("$5")` |

IR rule: `Paragraph` whose only child is `Math{display_math:true}` ⇒ display-math block;
`CodeBlock{info=="math"}` ⇒ display-math block; any other `Math` ⇒ inline math (with
`display_math` kept as a hint). The `literal` keeps the newlines around display math; trim.
Math inside headings works (`## The $E=mc^2$ law {#law .x}` ⇒ Heading[Text, Math, Text] +
attrs, verified).

---

## 11. Alerts (callouts) *(verified)*

- `> [!NOTE] Custom title` + `> body` ⇒ `Alert{alert_type:Note, title:Some("Custom title"),
  multiline:false, fence_length:1} > Paragraph`. Title = rest of the tag line, trimmed and
  unescaped; `None` when empty. Tag is case-insensitive. A title with no body ⇒ `Alert` with no children.
- `>>> [!TIP] Multi title` … `>>>` (needs `multiline_block_quotes`) ⇒
  `Alert{multiline:true, fence_length:3}` containing several blocks without `>` prefixes —
  the nicest syntax for long callouts.
- Alerts work inside list items (`- > [!TIP] in list`). Unknown types fall back to
  `BlockQuote`. Five types only: Note, Tip, Important, Warning, Caution; writer's extra
  callout kinds (e.g. "example", "quote") need `:::callout kind` directives.
- HTML formatter output (if ever used): `<div class="markdown-alert markdown-alert-note"><p class="markdown-alert-title">Note</p>…</div>`.

---

## 12. Source positions *(verified)*

- `Ast.sourcepos` is filled for every node by `parse_document` regardless of options;
  `render.sourcepos` is read only in `html.rs` and `xml.rs`. Columns are 1-based UTF-8 byte
  offsets (`parse.sourcepos_chars = true` converts them to char counts after parsing).
- Observed imperfections (0.56 = 0.57): the last `Item` of a loose list and of a task list
  ends at `N+1:0` (`@26:1-27:0`, `@34:1-35:0`, `@29:1-31:0`); description-list end `@66:1-75:0`;
  a term with no blank line before `:` gets `@5:1-5:1` while its text is `@5:1-5:11`; a
  tight list item's paragraph can end before its text (`paragraph @91:3-91:3`,
  `text @91:3-91:13`); inline-footnote definitions `1:1-1:0`; email-autolink `Link` and its
  `Text` share one span. Heading sourcepos includes the attribute block
  (`heading @6:1-6:39`, `text @6:3-6:14`). For `serve`-mode "scroll to source line" use
  `start.line` of block nodes only — that is always right in the probes.
- `FootnoteDefinition` nodes keep their original sourcepos even after being moved to the end.

---

## 13. comrak's formatters do not emit attributes *(verified)*

`format_html` with presenter's options renders `# Heading {#h1 .cls k=v}` as
`<h1 data-sourcepos="1:1-1:24">Heading</h1>` and `![Caption](a.png){#fig:x width=50%}` as
`<img … src="a.png" alt="Caption" />` — no `id`, `class`, `width`; `format_xml` likewise
has no attribute output (`grep attrs src/html.rs src/xml.rs` is empty). Writer must render
attributes from its own IR (it does anyway). comrak's XML is still a handy debugging dump
(`comrak::format_xml(root, &options, &mut String)`; `sourcepos="…"` attributes appear when
`render.sourcepos = true`).

---

## 14. The probe

### 14.1 Crate
`/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/comrak-probe/` — `Cargo.toml`:

```toml
[package]
name = "comrak-probe"
version = "0.1.0"
edition = "2024"

[dependencies]
comrak = { version = "=0.57.0", default-features = false, features = ["attributes"] }   # first run was =0.56.0; dumps identical
```

Built with `CARGO_TARGET_DIR=/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/target cargo run --release -q -- all` (rustc 1.99.1, macOS). `src/main.rs` (the dumper is the seed of writer's AST→IR walker):

```rust
use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

fn options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.front_matter_delimiter = Some("---".into());
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.tasklist = true;
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.extension.description_lists = true;
    o.extension.alerts = true;
    o.extension.math_dollars = true;
    o.extension.math_code = true;
    o.extension.block_directive = true;
    o.extension.header_attributes = true;
    o.extension.fenced_code_attributes = true;
    o.extension.link_attributes = true;
    o.extension.inline_code_attributes = true;
    o.extension.superscript = true;
    o.extension.subscript = true;
    o.parse.smart = false;
    o.parse.relaxed_autolinks = false;
    o.render.r#unsafe = true;
    o.render.escape = false;
    o.render.hardbreaks = false;
    o.render.github_pre_lang = false;
    o.render.full_info_string = false;
    o.render.sourcepos = true;
    o
}

fn dump<'a>(node: &'a AstNode<'a>, depth: usize, out: &mut String) {
    let ast = node.data();                       // Ref<Ast>; drop before touching children mutably
    let indent = "  ".repeat(depth);
    let kind = ast.value.xml_node_name();
    let payload = match &ast.value {
        NodeValue::Document | NodeValue::BlockQuote | NodeValue::DescriptionList
        | NodeValue::DescriptionTerm | NodeValue::DescriptionDetails | NodeValue::Paragraph
        | NodeValue::ThematicBreak | NodeValue::TableCell | NodeValue::SoftBreak
        | NodeValue::LineBreak | NodeValue::Emph | NodeValue::Strong | NodeValue::Strikethrough
        | NodeValue::Superscript | NodeValue::Subscript | NodeValue::Underline
        | NodeValue::Highlight | NodeValue::Insert | NodeValue::SpoileredText
        | NodeValue::Escaped | NodeValue::Subtext => String::new(),
        v => format!(" {:?}", v),
    };
    let attrs = match &ast.attrs { Some(a) => format!(" attrs={:?}", a), None => String::new() };
    out.push_str(&format!("{indent}{kind}{payload}{attrs} @{}\n", ast.sourcepos));
    drop(ast);
    for child in node.children() { dump(child, depth + 1, out); }
}

fn run(title: &str, md: &str, opts: &Options) {
    let arena = Arena::new();
    let root = parse_document(&arena, md, opts);
    let mut out = String::new();
    dump(root, 0, &mut out);
    println!("=== {title} ===\n--- input ---\n{md}\n--- ast ---\n{out}");
}
// main(): runs main.md, then footnotes.md with/without parse.leave_footnote_definitions,
// then cases.md split on "%%%% " (a "[smart]"/"[inline_footnotes]"/"[multiline]"/"[math_latex]"
// prefix in the title toggles that option), then format_html/format_xml on html.md.
```

Dump line format: `<xml_node_name> <Debug of NodeValue payload> [attrs=…] @start-end`.

### 14.2 MAIN document dump (identical on 0.56.0 and 0.57.0)

Input (`main.md`):

````markdown
---
title: Probe document
author: J. Blaschke
---

# Introduction {#intro .wide key=value}

Setext heading {#setext .s}
---------------------------

A paragraph with *emph*, **strong**, ~~strike~~, H~2~O, x^2^, `code`{.lang k=v},
a [link](https://example.com "Title"){.btn target=_blank}, an inline ![img](a.png){width=50%},
a footnote[^one], inline math $E=mc^2$ and display inline $$a+b$$ and a literal \$5.
Hard break here\
and two-space break here  
then soft
break.

- tight one
- tight two
  - nested a
  - nested b

- loose one

- loose two

3. third
4. fourth
   1) nested paren

- [ ] todo
- [x] done
- [X] DONE

> quoted paragraph
>
> second quoted paragraph

> [!NOTE] Custom title
> Note body with **strong**.

> [!warning]
> lowercase warning, no title

```python {#code1 .numbered lines="1-3"}
print("hi")
```

```
no info fence
```

$$
\int_0^1 x\,dx
$$

```math
\sum_i i
```

| Left | Center | Right | None |
|:-----|:------:|------:|------|
| a    | b      | c     | d    |

Term one

: Details for term one

Term two

: Details for term two

    More details.

[^one]: The footnote text.

    Second paragraph of the footnote.

***

<div class="raw">
raw html block
</div>

<!-- an html comment -->

:::callout note title="Hi there" .big #c1
Callout body.

- with a list
:::

::: {.callout .warning id=w1 title="Pandoc form"}
Pandoc-style info string.
:::

:::include part2.ipynb
:::

Final paragraph.
````

AST:

```
document @1:1-101:16
  frontmatter FrontMatter("---\ntitle: Probe document\nauthor: J. Blaschke\n---\n\n") @1:1-4:3
  heading Heading(NodeHeading { level: 1, setext: false, closed: false }) attrs=Attributes { id: Some("intro"), classes: ["wide"], pairs: [("key", "value")] } @6:1-6:39
    text Text("Introduction") @6:3-6:14
  heading Heading(NodeHeading { level: 2, setext: true, closed: false }) attrs=Attributes { id: Some("setext"), classes: ["s"], pairs: [] } @8:1-9:27
    text Text("Setext heading") @8:1-8:14
  paragraph @11:1-17:6
    text Text("A paragraph with ") @11:1-11:17
    emph @11:18-11:23
      text Text("emph") @11:19-11:22
    text Text(", ") @11:24-11:25
    strong @11:26-11:35
      text Text("strong") @11:28-11:33
    text Text(", ") @11:36-11:37
    strikethrough @11:38-11:47
      text Text("strike") @11:40-11:45
    text Text(", H") @11:48-11:50
    subscript @11:51-11:53
      text Text("2") @11:52-11:52
    text Text("O, x") @11:54-11:57
    superscript @11:58-11:60
      text Text("2") @11:59-11:59
    text Text(", ") @11:61-11:62
    code Code(NodeCode { num_backticks: 1, literal: "code" }) attrs=Attributes { id: None, classes: ["lang"], pairs: [("k", "v")] } @11:63-11:79
    text Text(",") @11:80-11:80
    softbreak @11:81-11:81
    text Text("a ") @12:1-12:2
    link Link(NodeLink { url: "https://example.com", title: "Title" }) attrs=Attributes { id: None, classes: ["btn"], pairs: [("target", "_blank")] } @12:3-12:57
      text Text("link") @12:4-12:7
    text Text(", an inline ") @12:58-12:69
    image Image(NodeLink { url: "a.png", title: "" }) attrs=Attributes { id: None, classes: [], pairs: [("width", "50%")] } @12:70-12:93
      text Text("img") @12:72-12:74
    text Text(",") @12:94-12:94
    softbreak @12:95-12:95
    text Text("a footnote") @13:1-13:10
    footnote_reference FootnoteReference(NodeFootnoteReference { name: "one", texts: [("^", 1), ("one", 3)], ref_num: 1, ix: 1 }) @13:11-13:16
    text Text(", inline math ") @13:17-13:30
    math Math(NodeMath { dollar_math: true, display_math: false, literal: "E=mc^2" }) @13:31-13:38
    text Text(" and display inline ") @13:39-13:58
    math Math(NodeMath { dollar_math: true, display_math: true, literal: "a+b" }) @13:59-13:65
    text Text(" and a literal $5.") @13:66-13:84
    softbreak @13:85-13:85
    text Text("Hard break here") @14:1-14:15
    linebreak @14:16-14:17
    text Text("and two-space break here") @15:1-15:24
    linebreak @15:25-15:27
    text Text("then soft") @16:1-16:9
    softbreak @16:10-16:10
    text Text("break.") @17:1-17:6
  list List(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: false, is_task_list: false }) @19:1-26:11
    item Item(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: false, is_task_list: false }) @19:1-19:11
      paragraph @19:3-19:11
        text Text("tight one") @19:3-19:11
    item Item(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: false, is_task_list: false }) @20:1-22:12
      paragraph @20:3-20:11
        text Text("tight two") @20:3-20:11
      list List(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: true, is_task_list: false }) @21:3-22:12
        item Item(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: false, is_task_list: false }) @21:3-21:12
          paragraph @21:5-21:12
            text Text("nested a") @21:5-21:12
        item Item(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: false, is_task_list: false }) @22:3-22:12
          paragraph @22:5-22:12
            text Text("nested b") @22:5-22:12
    item Item(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: false, is_task_list: false }) @24:1-24:11
      paragraph @24:3-24:11
        text Text("loose one") @24:3-24:11
    item Item(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: false, is_task_list: false }) @26:1-27:0
      paragraph @26:3-26:11
        text Text("loose two") @26:3-26:11
  list List(NodeList { list_type: Ordered, marker_offset: 0, padding: 3, start: 3, delimiter: Period, bullet_char: 0, tight: true, is_task_list: false }) @28:1-30:18
    item Item(NodeList { list_type: Ordered, marker_offset: 0, padding: 3, start: 3, delimiter: Period, bullet_char: 0, tight: false, is_task_list: false }) @28:1-28:8
      paragraph @28:4-28:8
        text Text("third") @28:4-28:8
    item Item(NodeList { list_type: Ordered, marker_offset: 0, padding: 3, start: 4, delimiter: Period, bullet_char: 0, tight: false, is_task_list: false }) @29:1-31:0
      paragraph @29:4-29:9
        text Text("fourth") @29:4-29:9
      list List(NodeList { list_type: Ordered, marker_offset: 0, padding: 3, start: 1, delimiter: Paren, bullet_char: 0, tight: true, is_task_list: false }) @30:4-30:18
        item Item(NodeList { list_type: Ordered, marker_offset: 0, padding: 3, start: 1, delimiter: Paren, bullet_char: 0, tight: false, is_task_list: false }) @30:4-30:18
          paragraph @30:7-30:18
            text Text("nested paren") @30:7-30:18
  list List(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: true, is_task_list: true }) @32:1-34:10
    taskitem TaskItem(NodeTaskItem { symbol: None, symbol_sourcepos: 32:4-32:4 }) @32:1-32:10
      paragraph @32:7-32:10
        text Text("todo") @32:7-32:10
    taskitem TaskItem(NodeTaskItem { symbol: Some('x'), symbol_sourcepos: 33:4-33:4 }) @33:1-33:10
      paragraph @33:7-33:10
        text Text("done") @33:7-33:10
    taskitem TaskItem(NodeTaskItem { symbol: Some('X'), symbol_sourcepos: 34:4-34:4 }) @34:1-35:0
      paragraph @34:7-34:10
        text Text("DONE") @34:7-34:10
  block_quote @36:1-38:25
    paragraph @36:3-36:18
      text Text("quoted paragraph") @36:3-36:18
    paragraph @38:3-38:25
      text Text("second quoted paragraph") @38:3-38:25
  alert Alert(NodeAlert { alert_type: Note, title: Some("Custom title"), multiline: false, fence_length: 1, fence_offset: 0 }) @40:1-41:28
    paragraph @41:3-41:28
      text Text("Note body with ") @41:3-41:17
      strong @41:18-41:27
        text Text("strong") @41:20-41:25
      text Text(".") @41:28-41:28
  alert Alert(NodeAlert { alert_type: Warning, title: None, multiline: false, fence_length: 1, fence_offset: 0 }) @43:1-44:29
    paragraph @44:3-44:29
      text Text("lowercase warning, no title") @44:3-44:29
  code_block CodeBlock(NodeCodeBlock { fenced: true, fence_char: 96, fence_length: 3, fence_offset: 0, info: "python", literal: "print(\"hi\")\n", closed: true }) attrs=Attributes { id: Some("code1"), classes: ["numbered"], pairs: [("lines", "1-3")] } @46:1-48:3
  code_block CodeBlock(NodeCodeBlock { fenced: true, fence_char: 96, fence_length: 3, fence_offset: 0, info: "", literal: "no info fence\n", closed: true }) @50:1-52:3
  paragraph @54:1-56:2
    math Math(NodeMath { dollar_math: true, display_math: true, literal: "\n\\int_0^1 x\\,dx\n" }) @54:1-56:2
  code_block CodeBlock(NodeCodeBlock { fenced: true, fence_char: 96, fence_length: 3, fence_offset: 0, info: "math", literal: "\\sum_i i\n", closed: true }) @58:1-60:3
  table Table(NodeTable { alignments: [Left, Center, Right, None], num_columns: 4, num_rows: 2, num_nonempty_cells: 8 }) @62:1-64:32
    table_row TableRow(true) @62:1-62:32
      table_cell @62:2-62:7
        text Text("Left") @62:3-62:6
      table_cell @62:9-62:16
        text Text("Center") @62:10-62:15
      table_cell @62:18-62:24
        text Text("Right") @62:19-62:23
      table_cell @62:26-62:31
        text Text("None") @62:27-62:30
    table_row TableRow(false) @64:1-64:32
      table_cell @64:2-64:7
        text Text("a") @64:3-64:3
      table_cell @64:9-64:16
        text Text("b") @64:10-64:10
      table_cell @64:18-64:24
        text Text("c") @64:19-64:19
      table_cell @64:26-64:31
        text Text("d") @64:27-64:27
  description_list @66:1-75:0
    description_item DescriptionItem(NodeDescriptionItem { marker_offset: 0, padding: 2, tight: false }) @66:1-68:22
      description_term @66:1-66:8
        paragraph @66:1-66:8
          text Text("Term one") @66:1-66:8
      description_details @68:1-68:22
        paragraph @68:3-68:22
          text Text("Details for term one") @68:3-68:22
    description_item DescriptionItem(NodeDescriptionItem { marker_offset: 0, padding: 2, tight: false }) @70:1-74:17
      description_term @70:1-70:8
        paragraph @70:1-70:8
          text Text("Term two") @70:1-70:8
      description_details @72:1-74:17
        paragraph @72:3-72:22
          text Text("Details for term two") @72:3-72:22
        paragraph @74:5-74:17
          text Text("More details.") @74:5-74:17
  thematic_break @80:1-80:3
  html_block HtmlBlock(NodeHtmlBlock { block_type: 6, literal: "<div class=\"raw\">\nraw html block\n</div>\n" }) @82:1-84:6
  html_block HtmlBlock(NodeHtmlBlock { block_type: 2, literal: "<!-- an html comment -->\n" }) @86:1-86:24
  block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "callout note title=\"Hi there\" .big #c1" }) @88:1-92:3
    paragraph @89:1-89:13
      text Text("Callout body.") @89:1-89:13
    list List(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: true, is_task_list: false }) @91:1-91:13
      item Item(NodeList { list_type: Bullet, marker_offset: 0, padding: 2, start: 1, delimiter: Period, bullet_char: 45, tight: false, is_task_list: false }) @91:1-91:3
        paragraph @91:3-91:3
          text Text("with a list") @91:3-91:13
  block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "{.callout .warning id=w1 title=\"Pandoc form\"}" }) @94:1-96:3
    paragraph @95:1-95:25
      text Text("Pandoc-style info string.") @95:1-95:25
  block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "include part2.ipynb" }) @98:1-99:3
  paragraph @101:1-101:16
    text Text("Final paragraph.") @101:1-101:16
  footnote_definition FootnoteDefinition(NodeFootnoteDefinition { name: "one", total_references: 1 }) @76:1-78:37
    paragraph @76:9-76:26
      text Text("The footnote text.") @76:9-76:26
    paragraph @78:5-78:37
      text Text("Second paragraph of the footnote.") @78:5-78:37
```

(Note the footnote definition moved to the end — default `leave_footnote_definitions = false`.)

### 14.3 Footnote placement (`footnotes.md`)

Input:
```markdown
First[^b] then[^a] and again[^b].

[^a]: Note A.

[^b]: Note B.

    Second paragraph of B.

[^unused]: never referenced

Last paragraph.
```

With `parse.leave_footnote_definitions = true`:
```
document @1:1-11:15
  paragraph @1:1-1:33
    text Text("First") @1:1-1:5
    footnote_reference FootnoteReference(NodeFootnoteReference { name: "b", texts: [("^", 1), ("b", 1)], ref_num: 1, ix: 1 }) @1:6-1:9
    text Text(" then") @1:10-1:14
    footnote_reference FootnoteReference(NodeFootnoteReference { name: "a", texts: [("^", 1), ("a", 1)], ref_num: 1, ix: 2 }) @1:15-1:18
    text Text(" and again") @1:19-1:28
    footnote_reference FootnoteReference(NodeFootnoteReference { name: "b", texts: [("^", 1), ("b", 1)], ref_num: 2, ix: 1 }) @1:29-1:32
    text Text(".") @1:33-1:33
  footnote_definition FootnoteDefinition(NodeFootnoteDefinition { name: "a", total_references: 1 }) @3:1-3:13
    paragraph @3:7-3:13
      text Text("Note A.") @3:7-3:13
  footnote_definition FootnoteDefinition(NodeFootnoteDefinition { name: "b", total_references: 2 }) @5:1-7:26
    paragraph @5:7-5:13
      text Text("Note B.") @5:7-5:13
    paragraph @7:5-7:26
      text Text("Second paragraph of B.") @7:5-7:26
  footnote_definition FootnoteDefinition(NodeFootnoteDefinition { name: "unused", total_references: 0 }) @9:1-9:27
    paragraph @9:12-9:27
      text Text("never referenced") @9:12-9:27
  paragraph @11:1-11:15
    text Text("Last paragraph.") @11:1-11:15
```

Default (`false`): same paragraph, then `paragraph "Last paragraph."`, then
`footnote_definition b (total_references 2)`, then `footnote_definition a` — sorted by `ix`;
`unused` is gone.

### 14.4 Isolated edge cases (`cases.md`; identical on 0.56.0 / 0.57.0)

Each block below is `input` → `ast`.

**Directive: colon in info line (NOT a directive; bare `:::` swallows)**
```
:::figure #fig:wrapped
![](plot.png)

The caption paragraph.
:::

after
→
document @1:1-7:5
  paragraph @1:1-2:13
    text Text(":::figure #fig:wrapped") @1:1-1:22
    softbreak @1:23-1:23
    image Image(NodeLink { url: "plot.png", title: "" }) @2:1-2:13
  paragraph @4:1-4:22
    text Text("The caption paragraph.") @4:1-4:22
  block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "" }) @5:1-7:5
    paragraph @7:1-7:5
      text Text("after") @7:1-7:5
```

**Directive: pandoc attribute form, colon-free**
```
:::figure {#fig-wrapped .wide width=80%}
![](plot.png)

The caption paragraph.
:::

after
→
document @1:1-7:5
  block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "figure {#fig-wrapped .wide width=80%}" }) @1:1-5:3
    paragraph @2:1-2:13
      image Image(NodeLink { url: "plot.png", title: "" }) @2:1-2:13
    paragraph @4:1-4:22
      text Text("The caption paragraph.") @4:1-4:22
  paragraph @7:1-7:5
    text Text("after") @7:1-7:5
```

**Directive: entity-escaped colon** — `:::figure #fig&#58;wrapped` / `body` / `:::` →
`block_directive … info: "figure #fig:wrapped" @1:1-3:3` > `paragraph > text Text("body")`.

**Directive: colon inside a quoted title (NOT a directive)** — `:::callout {.note title="Figure 1: results"}` / `body` / `:::` / blank / `after` →
`paragraph[text Text(":::callout {.note title=\"Figure 1: results\"}"), softbreak, text Text("body")]`, then `block_directive{info:""} @3:1-5:5 > paragraph "after"`.

**Directive: quoted/escaped include paths** — `:::include "my dir/part 2.md"` → `info: "include \"my dir/part 2.md\""`; `:::include my\ dir/x.md` → `info: "include my\\ dir/x.md"` (backslash before a space is kept).

**Empty directive between paragraphs**
```
before

:::pagebreak
:::

after
→
document @1:1-6:5
  paragraph @1:1-1:6 > text Text("before")
  block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "pagebreak" }) @3:1-4:3
  paragraph @6:1-6:5 > text Text("after")
```

**Empty directive with no blank lines around it** — `before` / `:::pagebreak` / `:::` / `after` →
`paragraph "before" @1:1-1:6`, `block_directive{info:"pagebreak"} @2:1-3:3`, `paragraph "after" @4:1-4:5` (the directive interrupts the paragraph).

**Other empty forms** — `::: {.pagebreak}`+`:::` → `info: "{.pagebreak}"`; `:::pagebreak landscape`+`:::` → `info: "pagebreak landscape"`; `:::toc depth=2`+`:::` → `info: "toc depth=2"`; all childless.

**Nested, outer fence longer**
```
::::columns widths="1fr 2fr"
:::column
left
:::
:::column
right
:::
::::

after
→
  block_directive BlockDirective(NodeBlockDirective { fence_length: 4, fence_offset: 0, info: "columns widths=\"1fr 2fr\"" }) @1:1-8:4
    block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "column" }) @2:1-4:3
      paragraph @3:1-3:4 > text Text("left")
    block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "column" }) @5:1-7:3
      paragraph @6:1-6:5 > text Text("right")
  paragraph @10:1-10:5 > text Text("after")
```

**Nested, same-length fences (outer closed by the first `:::`)**
```
:::columns
:::column
left
:::
:::column
right
:::
:::

after
→
  block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "columns" }) @1:1-4:3
    block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "column" }) @2:1-3:4
      paragraph @3:1-3:4 > text Text("left")
  block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "column" }) @5:1-7:3
    paragraph @6:1-6:5 > text Text("right")
  block_directive BlockDirective(NodeBlockDirective { fence_length: 3, fence_offset: 0, info: "" }) @8:1-10:5
    paragraph @10:1-10:5 > text Text("after")
```

**Unclosed directive** — `:::unclosed` / `swallowed paragraph` / blank / `# Heading inside` / blank / `para` →
`block_directive{info:"unclosed"} @1:1-6:4` containing the paragraph, the heading and `para`.

**Leaf candidates**
```
before

::pagebreak

\newpage

<!-- pagebreak -->

--- {.pagebreak}

***

- - -

after
→
document @1:1-15:5
  paragraph @1:1-1:6 > text Text("before")
  paragraph @3:1-3:11 > text Text("::pagebreak")
  paragraph @5:1-5:8 > text Text("\\newpage")
  html_block HtmlBlock(NodeHtmlBlock { block_type: 2, literal: "<!-- pagebreak -->\n" }) @7:1-7:18
  paragraph @9:1-9:16 > text Text("--- {.pagebreak}")
  thematic_break @11:1-11:3
  thematic_break @13:1-13:5
  paragraph @15:1-15:5 > text Text("after")
```

**Table + `Table:` paragraph**
```
| A | B |
|---|---|
| 1 | 2 |

Table: The table caption {#tbl:t}

after
→
  table Table(NodeTable { alignments: [None, None], num_columns: 2, num_rows: 2, num_nonempty_cells: 4 }) @1:1-3:9
    table_row TableRow(true) @1:1-1:9
      table_cell @1:2-1:4 > text Text("A") @1:3-1:3
      table_cell @1:6-1:8 > text Text("B") @1:7-1:7
    table_row TableRow(false) @3:1-3:9
      table_cell @3:2-3:4 > text Text("1")
      table_cell @3:6-3:8 > text Text("2")
  paragraph @5:1-5:33
    text Text("Table: The table caption {#tbl:t}") @5:1-5:33
  paragraph @7:1-7:5 > text Text("after")
```

**Table + pandoc `: caption` line (becomes a row)**
```
| C | D |
|---|---|
| 3 | 4 |
: pandoc colon caption
→
  table Table(NodeTable { alignments: [None, None], num_columns: 2, num_rows: 3, num_nonempty_cells: 5 }) @1:1-4:22
    table_row TableRow(true) …  C, D
    table_row TableRow(false) … 3, 4
    table_row TableRow(false) @4:1-4:22
      table_cell @4:1-4:22 > text Text(": pandoc colon caption") @4:1-4:22
      table_cell @4:23-4:23
```

**Image attribute variants**
```
![Caption with *emph*](a.png "The title"){#fig:x width=50%}

![Caption](a.png){#fig:y width='50%'}

![Caption](a.png){src=figures/bg.png}

![Caption](a.png){#fig:z .cls} trailing text

![One](a.png) ![Two](b.png)

![](noalt.png){.bare}

[![nested img](a.png)](https://x.y){.wrap}
→
document @1:1-13:42
  paragraph @1:1-1:59
    image Image(NodeLink { url: "a.png", title: "The title" }) attrs=Attributes { id: Some("fig:x"), classes: [], pairs: [("width", "50%")] } @1:1-1:59
      text Text("Caption with ") @1:3-1:15
      emph @1:16-1:21
        text Text("emph") @1:17-1:20
  paragraph @3:1-3:37
    image Image(NodeLink { url: "a.png", title: "" }) @3:1-3:17
      text Text("Caption") @3:3-3:9
    text Text("{#fig:y width='50%'}") @3:18-3:37
  paragraph @5:1-5:37
    image Image(NodeLink { url: "a.png", title: "" }) @5:1-5:17
      text Text("Caption") @5:3-5:9
    text Text("{src=figures/bg.png}") @5:18-5:37
  paragraph @7:1-7:44
    image Image(NodeLink { url: "a.png", title: "" }) attrs=Attributes { id: Some("fig:z"), classes: ["cls"], pairs: [] } @7:1-7:30
      text Text("Caption") @7:3-7:9
    text Text(" trailing text") @7:31-7:44
  paragraph @9:1-9:27
    image Image(NodeLink { url: "a.png", title: "" }) @9:1-9:13
      text Text("One") @9:3-9:5
    text Text(" ") @9:14-9:14
    image Image(NodeLink { url: "b.png", title: "" }) @9:15-9:27
      text Text("Two") @9:17-9:19
  paragraph @11:1-11:21
    image Image(NodeLink { url: "noalt.png", title: "" }) attrs=Attributes { id: None, classes: ["bare"], pairs: [] } @11:1-11:21
  paragraph @13:1-13:42
    link Link(NodeLink { url: "https://x.y", title: "" }) attrs=Attributes { id: None, classes: ["wrap"], pairs: [] } @13:1-13:42
      image Image(NodeLink { url: "a.png", title: "" }) @13:2-13:21
        text Text("nested img") @13:4-13:13
```
`[![alt](a.png){.x width=10%}](https://u){.wrap}` → `link{attrs .wrap} > image{attrs .x, width=10%} > text "alt"`.

**Autolink / citation-like text**
```
see @fig:x and (@tbl:y) and [@cite2020; @other] and -@sec:intro.
@fig:x
mail me@example.com or www.example.com or https://x.y/z.
→
  paragraph @1:1-3:56
    text Text("see @fig:x and (@tbl:y) and [@cite2020; @other] and -@sec:intro.") @1:1-1:64
    softbreak @1:65-1:65
    text Text("@fig:x") @2:1-2:6
    softbreak @2:7-2:7
    text Text("mail ") @3:1-3:5
    link Link(NodeLink { url: "mailto:me@example.com", title: "" }) @3:6-3:19
      text Text("me@example.com") @3:6-3:19
    text Text(" or ") @3:20-3:23
    link Link(NodeLink { url: "http://www.example.com", title: "" }) @3:24-3:38
      text Text("www.example.com") @3:24-3:38
    text Text(" or ") @3:39-3:42
    link Link(NodeLink { url: "https://x.y/z", title: "" }) @3:43-3:55
      text Text("https://x.y/z") @3:43-3:55
    text Text(".") @3:56-3:56
```

**Bracketed spans (pandoc `[text]{.cls}`) — no support** — `A [span text]{.cls #s1} and [other]{k=v} and [x]{} here.` → one `text Text("A [span text]{.cls #s1} and [other]{k=v} and [x]{} here.")`.

**Heading attribute edge cases**
```
## Two blocks {.a} {k="v"}      → attrs { pairs: [("k","v")] }, text Text("Two blocks {.a}") @1:4-1:18 ; heading @1:1-1:26
## Middle {.a} text             → no attrs, text Text("Middle {.a} text")
## Empty {}                     → attrs { id: None, classes: [], pairs: [] }, text Text("Empty")
## Bare key {k}                 → no attrs, text Text("Bare key {k}")
## Comma {a=1, b=2}             → no attrs, text Text("Comma {a=1, b=2}")
## Closed hashes {#c} ##        → NodeHeading { closed: true }, attrs { id: Some("c") }, text Text("Closed hashes")
## Quoted id {#"with space" .x} → attrs { id: Some("with space"), classes: ["x"] }
## Colon value {k=a:b data-x=1.5%} → attrs { pairs: [("k","a:b"), ("data-x","1.5%")] }
## The $E=mc^2$ law {#law .x}   → attrs { id: Some("law"), classes: ["x"] }; children text "The ", math{literal:"E=mc^2"}, text " law"
```

**Inline attribute edge cases**
```
Text with `x`{.lang k=v} and `y`{k='v'} and [ref link][r]{.cls} and [a]( u ){.c} and `z`{ .multi
k="1" }.

[r]: https://r.example
→
  paragraph @1:1-2:8
    text Text("Text with ") @1:1-1:10
    code Code(NodeCode { num_backticks: 1, literal: "x" }) attrs=Attributes { id: None, classes: ["lang"], pairs: [("k", "v")] } @1:11-1:24
    text Text(" and ") @1:25-1:29
    code Code(NodeCode { num_backticks: 1, literal: "y" }) @1:30-1:32
    text Text("{k='v'} and ") @1:33-1:44
    link Link(NodeLink { url: "https://r.example", title: "" }) attrs=Attributes { id: None, classes: ["cls"], pairs: [] } @1:45-1:63
      text Text("ref link") @1:46-1:53
    text Text(" and ") @1:64-1:68
    link Link(NodeLink { url: "u", title: "" }) attrs=Attributes { id: None, classes: ["c"], pairs: [] } @1:69-1:80
      text Text("a") @1:70-1:70
    text Text(" and ") @1:81-1:85
    code Code(NodeCode { num_backticks: 1, literal: "z" }) attrs=Attributes { id: None, classes: ["multi"], pairs: [("k", "1")] } @1:86-2:7
    text Text(".") @1:105-1:105
```

**Fence info variants**
```
```python extra words here      → CodeBlock { info: "python extra words here" }, no attrs
``` {.numbered}                 → CodeBlock { info: "" } attrs { classes: ["numbered"] }
~~~rust {#r}                    → CodeBlock { fence_char: 126, info: "rust" } attrs { id: Some("r") }
```python title="A: B"          → CodeBlock { info: "python title=\"A: B\"" }, no attrs (no braces)
```

**Raw HTML**
```
<figure>
<img src="x.png">
</figure>

Inline <span class="x">hi</span> and <!-- c --> here.

<custom-tag>
type 7 block
</custom-tag>
→
  html_block HtmlBlock(NodeHtmlBlock { block_type: 6, literal: "<figure>\n<img src=\"x.png\">\n</figure>\n" }) @1:1-3:9
  paragraph @5:1-5:53
    text Text("Inline ") @5:1-5:7
    html_inline HtmlInline("<span class=\"x\">") @5:8-5:23
    text Text("hi") @5:24-5:25
    html_inline HtmlInline("</span>") @5:26-5:32
    text Text(" and ") @5:33-5:37
    html_inline HtmlInline("<!-- c -->") @5:38-5:47
    text Text(" here.") @5:48-5:53
  html_block HtmlBlock(NodeHtmlBlock { block_type: 7, literal: "<custom-tag>\ntype 7 block\n</custom-tag>\n" }) @7:1-9:13
```

**Loose list with block children** — `- item one` / blank / fenced `py` code / blank / `  > quote in item` / blank / `- item two` →
`list{tight:false} > item @1:1-7:17 > [paragraph "item one", code_block{info:"py", literal:"code\n"} @3:3-5:5, block_quote @7:3-7:17 > paragraph "quote in item"]`, `item @9:1-9:10 > paragraph "item two"`.

**Description list variants**
```
Term

: Details

Second term
: no blank line before details

Third

:   indented details

    second paragraph
→
  description_list @1:1-12:20
    description_item DescriptionItem(NodeDescriptionItem { marker_offset: 0, padding: 2, tight: false }) @1:1-3:9
      description_term @1:1-1:4 > paragraph @1:1-1:4 > text Text("Term")
      description_details @3:1-3:9 > paragraph @3:3-3:9 > text Text("Details")
    description_item DescriptionItem(NodeDescriptionItem { marker_offset: 0, padding: 2, tight: true }) @5:1-6:30
      description_term @5:1-5:1 > paragraph @5:1-5:1 > text Text("Second term") @5:1-5:11     ← bogus term sourcepos
      description_details @6:1-6:30 > paragraph @6:3-6:30 > text Text("no blank line before details")
    description_item DescriptionItem(NodeDescriptionItem { marker_offset: 0, padding: 4, tight: false }) @8:1-12:20
      description_term @8:1-8:5 > paragraph > text Text("Third")
      description_details @10:1-12:20
        paragraph @10:5-10:20 > text Text("indented details")
        paragraph @12:5-12:20 > text Text("second paragraph")
```

**Math variants** — see §10 table; dump:
```
Inline $a$ and $$b$$ mid-paragraph.   → paragraph[text "Inline ", math{display:false,"a"}, text " and ", math{display:true,"b"}, text " mid-paragraph."]
$$\nc\n$$                             → paragraph @3:1-5:2 > math Math(NodeMath { dollar_math: true, display_math: true, literal: "\nc\n" })
$$d$$                                 → paragraph > math{display:true,"d"}
$`e`$                                 → paragraph > math Math(NodeMath { dollar_math: false, display_math: false, literal: "e" })
```math\nf\n```                       → code_block CodeBlock { info: "math", literal: "f\n" }
Not math: $5 and $6.                  → text Text("Not math: $5 and $6.")
- item / blank / "  $$\n  x\n  $$"    → item > [paragraph "item", paragraph > math{display:true, "\nx\n"}]
> $$y$$                               → block_quote > paragraph > math{display:true,"y"}
[math_latex] Inline \(a\) and display \[b\].  → math{dollar_math:true, display:false,"a"} … math{dollar_math:true, display:true,"b"}
[math_latex] \[\nc\n\]                → paragraph > math{display:true, "\nc\n"}
(without math_latex) \(a\) \[b\]     → text Text("Inline (a) and display [b].")
```

**Smart punctuation (`parse.smart = true`)** — `"quotes" and 'single' -- dash --- emdash ... ellipsis` → `text Text("“quotes” and ‘single’ – dash — emdash … ellipsis")`.

**Inline footnotes (`extension.inline_footnotes = true`)**
```
Hi^[An inline note with *emph*] there[^n].

[^n]: Named.
→
  paragraph @1:1-1:42
    text Text("Hi") @1:1-1:2
    footnote_reference FootnoteReference(NodeFootnoteReference { name: "__inline_1", texts: [], ref_num: 1, ix: 1 }) @1:3-1:31
    text Text(" there") @1:32-1:37
    footnote_reference FootnoteReference(NodeFootnoteReference { name: "n", texts: [("^", 1), ("n", 1)], ref_num: 1, ix: 2 }) @1:38-1:41
    text Text(".") @1:42-1:42
  footnote_definition FootnoteDefinition(NodeFootnoteDefinition { name: "__inline_1", total_references: 1 }) @1:1-1:0
    paragraph @1:1-1:0
      text Text("An inline note with ") @1:1-1:20
      emph @1:21-1:26 > text Text("emph")
  footnote_definition FootnoteDefinition(NodeFootnoteDefinition { name: "n", total_references: 1 }) @3:1-3:12
    paragraph @3:7-3:12 > text Text("Named.")
```

**Multiline alert / quote (`extension.multiline_block_quotes = true`)**
```
>>> [!TIP] Multi title
para one

- list
>>>

>>>
plain multiline quote
>>>
→
  alert Alert(NodeAlert { alert_type: Tip, title: Some("Multi title"), multiline: true, fence_length: 3, fence_offset: 0 }) @1:1-4:6
    paragraph @2:1-2:8 > text Text("para one")
    list List(… tight: true …) @4:1-4:6 > item > paragraph > text Text("list")
  multiline_block_quote MultilineBlockQuote(NodeMultilineBlockQuote { fence_length: 3, fence_offset: 0 }) @7:1-9:3
    paragraph @8:1-8:21 > text Text("plain multiline quote")
```

**Alert variants**
```
> [!NOTE]\n> no title            → alert Alert(NodeAlert { alert_type: Note, title: None, multiline: false, fence_length: 1, fence_offset: 0 }) @1:1-2:10 > paragraph "no title"
> [!Important] Mixed case        → alert Alert(NodeAlert { alert_type: Important, title: Some("Mixed case"), … }) @4:1-4:25   (no children)
> [!NOTE] Title only, no body    → alert { Note, title: Some("Title only, no body") } (no children)
>> [!NOTE]\n>> two markers       → block_quote @8:1-9:14 > alert { Note, title: None } @8:2-9:14 > paragraph "two markers"
> [!UNKNOWN]\n> not an alert type → block_quote > paragraph[text Text("[!UNKNOWN]"), softbreak, text Text("not an alert type")]
- > [!TIP] in list\n  > body     → list > item > alert { Tip, title: Some("in list") } > paragraph "body"
```

**Thematic breaks and the setext pitfall** — `Para then` / `--` → `heading Heading(NodeHeading { level: 2, setext: true, closed: false }) @1:1-2:2 > text Text("Para then")`; `***`, `___`, `- - -` → `thematic_break`.

**Escapes** — `\*not emph\*, \$5, \\back, \# not heading, a\` / `hard, 5 \< 6` → `text Text("*not emph*, $5, \\back, # not heading, a") @1:2-1:44`, `linebreak @1:45-1:46`, `text Text("hard, 5 < 6")`.

**Links** — `[inline](u "t") [ref][r] [collapsed][] [shortcut] <https://auto.link> <me@x.y> [empty]()` (+ definitions) → `link{url:"u",title:"t"}`, `link{url:"/r"}`, `link{url:"/c",title:"C"}`, `link{url:"/s"}`, `link{url:"https://auto.link"}`, `link{url:"mailto:me@x.y"}`, `link{url:"",title:""} > text "empty"`.

**Front matter** — `---\ntitle: T\n---\n# H` → `frontmatter FrontMatter("---\ntitle: T\n---\n") @1:1-3:3`, `heading … "H" @4:1-4:3`. Not at the top (`Text first` / blank / `---` / `title: not front matter` / `---`) → `paragraph`, `thematic_break @3:1-3:3`, `heading{level:2, setext:true} > text Text("title: not front matter")`.

### 14.5 Formatter output for reference (`html.md`)

`format_html` (presenter options + `render.sourcepos = true`):
```html
<h1 data-sourcepos="1:1-1:24">Heading</h1>
<p data-sourcepos="3:1-3:35"><img data-sourcepos="3:1-3:35" src="a.png" alt="Caption" /></p>
<p data-sourcepos="5:1-5:33">Code <code data-sourcepos="5:6-5:15">x</code> and <a data-sourcepos="5:21-5:32" href="u">l</a>.</p>
<pre data-sourcepos="7:1-9:3"><code class="language-python">x = 1
</code></pre>
<div class="markdown-alert markdown-alert-note" data-sourcepos="11:1-12:6">
<p class="markdown-alert-title">Title</p>
<p data-sourcepos="12:3-12:6">body</p>
</div>
<div class="callout note" data-sourcepos="14:1-16:3">
<p data-sourcepos="15:1-15:4">body</p>
</div>
<p data-sourcepos="18:1-20:2"><span data-math-style="display" data-sourcepos="18:1-20:2">
x
</span></p>
<p data-sourcepos="22:1-22:10">H<sub data-sourcepos="22:2-22:4">2</sub>O x<sup data-sourcepos="22:8-22:10">2</sup></p>
```
(`{#h1 .cls k=v}`, `{#fig:x width=50%}`, `{.lang}`, `{.btn}`, `{#c .n}` were all in the
source and none appears in the output.) `format_xml` emits `<alert type="note" title="Title">`,
`<block_directive info="callout note">`, `<math math_style="display">`, `<code_block info="python">`,
`<image destination="a.png" title="">` — also without attrs.

---

## 15. Verified API hints (exact signatures / snippets)

```rust
// comrak 0.56.0 / 0.57.0, default-features = false, features = ["attributes"]
pub fn comrak::parse_document<'a>(arena: &'a Arena<'a>, md: &str, options: &Options) -> Node<'a>;
pub type comrak::Arena<'a> = typed_arena::Arena<comrak::nodes::AstNode<'a>>;
pub type comrak::nodes::AstNode<'a> = comrak::arena_tree::Node<'a, RefCell<Ast>>;
pub type comrak::Node<'a> = &'a AstNode<'a>;          // also comrak::nodes::Node
pub fn comrak::format_html<'a>(root: Node<'a>, options: &Options, output: &mut dyn std::fmt::Write) -> std::fmt::Result; // (probe called it with &mut String)
pub fn comrak::format_xml<'a>(root: Node<'a>, options: &Options, output: &mut dyn std::fmt::Write) -> std::fmt::Result;
pub fn comrak::format_commonmark<'a>(root: Node<'a>, options: &Options, output: &mut dyn std::fmt::Write) -> std::fmt::Result;
pub fn comrak::markdown_to_html(md: &str, options: &Options) -> String;
pub fn comrak::version() -> &'static str;
pub struct comrak::Options<'c> { pub extension: Extension<'c>, pub parse: Parse<'c>, pub render: Render }  // Default + Clone
pub use comrak::options::{Extension, Parse, Render, AlertStyleType, ListStyleType, URLRewriter, BrokenLinkCallback, BrokenLinkReference};
pub use comrak::nodes::{Ast, AstNode, Node, NodeValue, Attributes, Sourcepos, LineColumn, NodeList, ListType, ListDelimType,
    NodeCodeBlock, NodeHtmlBlock, NodeHeading, NodeFootnoteDefinition, NodeFootnoteReference, NodeTable, TableAlignment,
    NodeTaskItem, NodeCode, NodeLink, NodeWikiLink, NodeMath, NodeMultilineBlockQuote, NodeAlert, AlertType,
    NodeBlockDirective, NodeDescriptionItem, ValidationError};
// arena_tree::Node<'a, T>
pub fn parent(&self) -> Option<&'a Node<'a, T>>;  pub fn first_child(&self) -> Option<&'a Node<'a, T>>;  pub fn last_child(&self) -> …;
pub fn previous_sibling(&self) -> …;  pub fn next_sibling(&self) -> …;
pub fn children(&'a self) -> Children<'a, T>;  pub fn descendants(&'a self) -> Descendants<'a, T>;  pub fn traverse(&'a self) -> Traverse<'a, T>;
pub fn detach(&self);  pub fn append(&'a self, new_child: &'a Node<'a, T>);  pub fn insert_after(&'a self, new_sibling: &'a Node<'a, T>);
// Node<'_, RefCell<T>>
pub fn data(&self) -> std::cell::Ref<'_, T>;  pub fn data_mut(&self) -> std::cell::RefMut<'_, T>;
// AstNode
pub fn collect_text(&'a self) -> String;  pub fn validate(&'a self) -> Result<(), ValidationError<'a>>;
// NodeValue
pub fn block(&self) -> bool;  pub fn contains_inlines(&self) -> bool;  pub fn text(&self) -> Option<&str>;  pub fn xml_node_name(&self) -> &'static str;
// macro
comrak::node_matches!(node, NodeValue::Paragraph | NodeValue::Heading(..));   // borrows node.data()
```

Walker skeleton that matched the probe's borrow pattern (verified compiles on 1.99.1):

```rust
fn walk_blocks<'a>(node: &'a AstNode<'a>, out: &mut Vec<Block>) {
    for child in node.children() {
        let ast = child.data();                       // Ref<Ast>: hold only while reading
        match &ast.value {
            NodeValue::Paragraph => {
                // implicit figure: sole child is Image
                let only_image = child.first_child().is_some_and(|f| f.next_sibling().is_none()
                    && matches!(f.data().value, NodeValue::Image(_)));
                // display math: sole child is Math{display_math:true}
                // …
            }
            NodeValue::Heading(h) => { let attrs = ast.attrs.as_deref().cloned(); /* h.level, child inlines */ }
            NodeValue::CodeBlock(cb) => { /* cb.info, cb.literal, ast.attrs */ }
            NodeValue::BlockDirective(d) => { /* d.info; children blocks; no children => leaf */ }
            NodeValue::Alert(a) => { /* a.alert_type, a.title, children blocks */ }
            NodeValue::List(l) => { /* l.list_type, l.start, l.tight; children are Item/TaskItem */ }
            NodeValue::Table(t) => { /* t.alignments; TableRow(is_header) > TableCell > inlines */ }
            NodeValue::FootnoteDefinition(f) => { /* f.name; collect into map */ }
            NodeValue::HtmlBlock(h) => { /* h.block_type == 2 => comment */ }
            NodeValue::FrontMatter(s) => { /* strip delimiter lines, parse YAML */ }
            _ => {}
        }
        // ast.sourcepos.start.line / .end.line for diagnostics
    }
}
```

---

## 16. Suggested comrak → IR mapping (pandoc-like)

| comrak | IR block/inline | notes |
|---|---|---|
| `FrontMatter(s)` | document metadata | strip first/last delimiter lines and trailing blank line before YAML |
| `Heading{level}` + attrs | `Header{level, attrs, inlines}` | attrs fallback on trailing `{…}` text |
| `Paragraph` [sole `Image`] | `Figure{attrs, src, title, caption}` | §7 |
| `Paragraph` [sole `Math{display_math:true}`] / `CodeBlock{info:"math"}` | `DisplayMath{tex}` | trim literal |
| `Paragraph` first `Text` starts with `Table:` and adjacent `Table` | caption of that table | own `{…}` parsing |
| `Paragraph` | `Para{inlines}` | in a `List{tight:true}` → `Plain` |
| `CodeBlock{info, literal}` + attrs | `CodeBlock{lang: first word of info, attrs, text}` | presenter §5 conventions reusable |
| `BlockQuote` / `MultilineBlockQuote` | `BlockQuote{blocks}` | |
| `Alert{alert_type,title}` | `Callout{kind, title, blocks}` | `AlertType::default_title()` for the default |
| `BlockDirective{info}` | `Div{name, attrs, blocks}` / leaf (`PageBreak`, `Toc`, `Include{path}`) when childless and name is registered | info parsing per presenter §4 (first token = name, `k=v` args, pandoc `{…}` form) |
| `List{list_type,start,delimiter,tight,is_task_list}` > `Item`/`TaskItem{symbol}` | `BulletList`/`OrderedList{start, delim}` / task items | use `List.tight`, not `Item.tight` |
| `DescriptionList > DescriptionItem > DescriptionTerm/DescriptionDetails` | `DefinitionList{(term inlines, [blocks])}` | term = inlines of the term's Paragraph |
| `Table{alignments}` > `TableRow(hdr)` > `TableCell` | `Table{aligns, header, rows}` | cells are inlines only |
| `ThematicBreak` | `HorizontalRule` | |
| `HtmlBlock{block_type,literal}` | `RawBlock{html}`; `block_type==2` → comment (drop or keep for compat) | |
| `FootnoteDefinition{name}` | footnote map `name → blocks` | with `leave_footnote_definitions = true` |
| `Text`, `SoftBreak`, `LineBreak` | `Str`/`Space`/`SoftBreak`/`LineBreak` | with `parse.smart` the Unicode is already in `Text` |
| `Emph`, `Strong`, `Strikethrough`, `Superscript`, `Subscript`, `Highlight`, `Insert`, `Underline` | the obvious inline containers | |
| `Code{literal}` + attrs | `Code{attrs, text}` | |
| `Link{url,title}` + attrs | `Link{attrs, inlines, url, title}` | `url == ""` possible |
| `Image{url,title}` + attrs (not sole child) | `Image{attrs, alt inlines, url, title}` | |
| `FootnoteReference{name, ix, ref_num}` | `Note{name}` with numbering from `ix` | |
| `Math{display_math,literal}` (not sole child) | `Math{inline/display, tex}` | |
| `HtmlInline(s)` | `RawInline{html}` | |
| `Text` containing `@id`, `[@key]` | citation / cross-reference parsing in writer's own pass | never autolinked (§8) |

---

## 17. Front matter details *(verified)*

- Only recognised at line 1; the `FrontMatter` literal is the whole block **including** both
  delimiter lines **and the blank line(s) that follow** (`"---\ntitle: …\n---\n\n"`), and
  without a following blank line just `"---\ntitle: T\n---\n"`. Strip the first line, the last
  `---` line and surrounding whitespace before YAML parsing; keep presenter's
  `front-matter-empty`/`front-matter-unclosed` heuristics (spec §2.1).
- A `---` block later in the file is a `ThematicBreak` (or a setext underline).

---

## 18. Open questions

1. **Label scheme for directive-wrapped figures/tables.** Inline attribute ids may contain
   colons (`{#fig:x}`), directive info lines may not. Options: (a) writer ids are colon-free
   everywhere (`fig-x`) and the LaTeX backend emits `\label{fig-x}`; (b) keep `fig:x` on
   images and require `label=fig-x`/`&#58;` in directives; (c) writer pre-processes
   directive opening lines (replace `:` after the fence with `&#58;` before calling comrak —
   a 1-line regex on `^\s{0,3}:{3,}[^:\r\n]*:`), which would also fix `title="A: B"`. (c)
   keeps the user syntax clean; it needs a decision because it changes sourcepos columns on
   that line only (lengths differ) — or do the substitution with equal-length placeholders.
2. Whether to enable `parse.ignore_setext` so `Para\n---` never becomes an `<h2>` (presenter
   warns instead). Pandoc keeps setext.
3. Whether to enable `math_latex` (`\(…\)`, `\[…\]`); the AST cannot tell it apart from
   dollar math (`dollar_math: true` for both), which only matters for round-tripping.
4. `underline` is not enabled; the interaction of `__x__` with `**x**`/`__x__` strong is
   doc-only (`<u>`), not probed.
5. Bracketed spans `[text]{.cls}` have no comrak support; if writer wants inline
   attributed spans (e.g. small caps, index terms), it needs its own `Text`-level parser or
   raw HTML `<span class>`.
6. Sourcepos end columns of `0` (`@26:1-27:0`) and the description-list term sourcepos bug:
   decide whether `serve`'s live-reload scroll sync uses only `start.line` (recommended) or
   needs a fix-up pass.
7. The `shortcodes` feature (`:smile:`) — presenter does not enable it; the `:` conflict
   with directive lines makes it even less attractive for writer.

---

## 19. Risks checklist for the task files

- A colon anywhere after `:::` silently turns the line into a paragraph and the closing
  `:::` into a document-swallowing directive (lint required; §6.2).
- Same-length nested fences close the outer directive early; unclosed directives run to EOF.
- `Item.tight` is always `false`; read `List.tight`.
- `FrontMatter` literal includes delimiters and trailing blank lines.
- Default footnote processing drops unreferenced definitions silently (set
  `leave_footnote_definitions = true`).
- Attribute blocks that comrak rejects are *not* errors: they come back as text (fallback needed).
- `$$…$$` is an inline `Math` node; only the sole-child rule makes it a display block.
- pandoc `: caption` under a table becomes a table row.
- 0.57 adds two `NodeValue` variants: exhaustive matches must handle them.
