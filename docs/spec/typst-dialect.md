# Spec: typst dialect, `@writer/doc`, the embedded compiler

**Status:** contract. Implemented by T03 (§6 World incl. `world_inputs` (§6.2), the
`IncludeProvider` trait and `Synthesized` (§6.4), `SourceMap` complete with `from_lines` (§5.1),
§8.1–8.3 compile, §9 renditions and MathML), T31 (§4 package, `typst::package::eject`), T30 (§5
emitter, building its source map with T03's `SourceMap::from_lines`), T32 (§8.4 PDF pipeline,
including the `[pdf] template` mount and `EmitOptions.template` of §4.6/§6.3, `dump --typst`, the
oracle test, emitter snapshots of the Markdown fixtures), T34 (§10 `--to typst` export, including
the template copy, and the BYO-template end-to-end test of §4.6), T50 (§2–§3 frontend), T52 (`.typ`
inputs end to end, §7's round-trip golden), T53 (§2.7 `.md`/`.ipynb` includes, §6.4's
`MarkdownIncludes::synthesize` body, `include-notebook`), T71 (§7 lints), T72 (§10 eject wiring), T16 (§6.4's provider installation and
its stub). Consumers: T16 (dispatch), T20/T24 (`accessed()`, pages),
T1A/T41/T42/T60 (renditions, MathML), T73 (diagnostic mapping), the guide.

Frozen specs cite sections by number: config.md → §3 (arguments), §4 (template); cli.md → §2
(subset), §7 (lints); ir.md §3 → §6 (`/__assets/`). Types not defined here are ir.md's and
config.md's. Pins: typst crates **0.15.1**, `@writer/doc` **0.1.0**, vendored `@preview/mitex`
**0.2.7** (latest on packages.typst.org, 2026-10-24; vendored-assets.md is authoritative for its
files). Facts come from R1, R2, R4, R5 and probes run for this spec and its review
(`scratchpad/build/typst-dialect-probe`, `td-*-review`, `td-maint-probe`; quoted as *probe*).

## 1. Modules and constants
| file | owner | contents |
|---|---|---|
| `src/typst/mod.rs`, `world.rs` | T03 | `pub const WRITER_DOC_VERSION: &str = "0.1.0"; pub use crate::vendor::MITEX_VERSION;` `WriterWorld` (§6), `world_inputs` (§6.2), the `IncludeProvider` trait and `Synthesized` (§6.4) |
| `src/typst/{compile,render,html,sourcemap}.rs` | T03 | §8.1–8.3, §9; `SourceMap` complete incl. `from_lines` (§5.1) |
| `src/typst/package.rs` | T03 (serving), T31 (`eject`, `install_dir`) | reads `@writer/doc` through `crate::vendor::get("typst/writer-doc/<vpath>")` and `@preview/*` through `vendor::typst_package_dir` (vendored-assets.md §5.1, §5.3); no embed of its own |
| `src/typst/emit.rs`, `frontend.rs`, `includes.rs` | T30, T50, T53 (T16: `MarkdownIncludes` stub) | §5; §2–§3 (`typst::frontend::parse`, ir.md §4); §6.4's `MarkdownIncludes` (the trait is T03's, in `world.rs`) |
| `assets/typst/writer-doc/**` | T31 (+T53 in `cells.typ`) | §4 |

## 2. The structured subset: typst → IR (T50; §2.7 also T53)

> **Contract change (coordinator, 2026-10-28):** T50 may split its implementation
> into private frontend/{walker,literals,metadata,calls,tables,normalize}.rs children,
> declared only in frontend.rs. Target-free check activates with T50; T71's
> per-target guard and T52's target dispatch stay explicit. The pipeline applies
> final CLI strictness before returning Typst Prepared, so metadata cannot
> weaken a CLI strict flag before subset rendering. The shared missing-asset
> parser accepts the optional Typst column suffix for watch recovery.

### 2.1 Entry point and failure policy
`pub fn parse(source: &SourceInfo, text: &str, config: &Config) -> Result<Parsed>` (ir.md §4):
`Source::detached(text)`; every error of `root.errors_and_warnings()` is `E-TYP-SYNTAX` and the result
is `Ok(Parsed)` with `blocks: []` (R2 §3.3: one unclosed `(` swallows the rest of the file); every
warning is `W-TYP-SYNTAX` (same message, hint and column rules; the IR is still built; *probe*:
```` ```c++ ```` warns "no whitespace between language tag and raw text"). Otherwise walk the root
`Markup` with the typed AST (R2 §2.7), positions from `LinkedNode` offsets. Every diagnostic has
`span: SourceSpan { start_line, end_line, file }` (`Lines::byte_to_line` + 1) and a message ending in
` (col N)` (`byte_to_column` + 1). `E-TYP-*` are `Level::Error`, `W-TYP-*` `Level::Warning` (§2.8;
math.md §9 owns `E-TYP-MATH-CODE`, a warning); `E-TYP-*` errors fail the input's `html`, `docx` and
`latex` targets (cli.md: the others are still written). The `pdf` target of a `.typ` input never
consults the IR; when it is the only target the frontend still runs (T52), its diagnostics are
dropped and its `meta.date` feeds §8.4's timestamp policy and `World::today`. `[typst] strict =
true` or `--strict` raise every `W-TYP-*` to `Level::Error` (code unchanged; cli.md names
`W-TYP-*`). The frontend applies `Config::merge_front_matter` (§3) to a clone of `config` before
using it. **Assembly:** `document.source = source.clone()`, `document.assets =
registry.infos(&source.base_dir)`, `includes` per §2.7, and `features` from content (ir.md §2.6):
`math` for any equation, `mi` or `mitex`, `code` for any `CodeBlock` or `Cell`, `notebook` for any
`Cell`, `citations` for any `Cite`, `footnotes` for any `Footnote`, merged across includes
(`Features::merge`).

### 2.2 Markup → IR
A `Markup` is flat (R2 §3.1). **Block-level** children end the current paragraph: `Heading`,
`ListItem`/`EnumItem`/`TermItem`, `Equation` with `block()`, `Raw` with `block()`, `ModuleInclude`,
and calls to `figure image table quote(block: true) list enum terms align block par pagebreak colbreak
line outline bibliography v heading callout mitex listing code-cell include-notebook`,
`raw(block: true)` and `math.equation(block: true)`. A paragraph is a maximal run of other children
between `Parbreak`s and block-level children; leading/trailing spaces are trimmed, adjacent `Text`
merged, empty paragraphs dropped.
Statements (`set`, `let`, `show`, `import`), comments, `Semicolon` and `Hash` leaves are zero-width.

| syntax | IR |
|---|---|
| `Text` / `Escape` (`get()`) | `Text` |
| `Space` | `SoftBreak` if it contains a newline, else `Text(" ")` |
| `Linebreak` (also `linebreak()`) | `LineBreak`; a `Space` directly before or after it is dropped (`a \⏎b` is `Text`, `Space`, `Linebreak`, `Space("\n")`, `Text`, *probe*) |
| `Shorthand` | `Text` of `get()`: `~`→U+00A0, `--`→U+2013, `---`→U+2014, `...`→U+2026, `-?`→U+00AD, `-`→U+2212 |
| `SmartQuote` | `Text` chosen by a port of typst's `SmartQuoter::quote` (typst-library 0.15.1 `text/smartquote.rs`), one quote stack per paragraph, `before` = the previous character (none → space; math or image → U+FFFC): after a digit `′`/`″` unless the innermost open quote has this kind; `'` after a letter or U+FFFC → `’` unless the innermost open quote is single; close (`”`/`’`) when the innermost open quote has this kind and `before` is not whitespace or `([{`; otherwise open (`“`/`‘`) (*probe*: `"well-" and 5' and x"y"` → `“well-” and 5′ and x“y”`) |
| `Strong`, `Emph` | `Strong`, `Emph` (body walked as inlines) |
| `Raw`, `block() == false` | `Code { text: lines joined by "\n", attrs: { classes: [lang] if lang } }` |
| `Raw`, `block() == true` | `CodeBlock { lang, text: lines joined by "\n", attrs: {} }` |
| `Link` (auto-link) | `Link { inlines: [Text(url)], url, title: None }` |
| `Ref` | `Ref { target, supplement, form: Normal }`: `@t` → `None`, `@t[]` → `Some([])`, `@t[x]` → `Some(walked x)`; `FootnoteRef` when `target` (with `writer-fn-` stripped as in §2.3) labels a footnote (post-walk fix-up) |
| `Heading` | `Heading { level: depth, inlines: body }`; depth > 6 → level 6 + `W-TYP-ARG-IGNORED` naming the depth (ir.md: 1–6) |
| `ListItem` / `EnumItem` | consecutive items of one kind separated only by `Space`/`Parbreak` form one `List`; `tight` = no `Parbreak` between items (then forced `false` if an item holds a block other than `Paragraph`/`List`/`CodeBlock`, ir.md §2.1); `Ordered { start: first item's number() or 1 }` |
| `TermItem` | consecutive items → one `DefinitionList`; each item → `DefinitionItem { term, definitions: [description blocks] }` |
| `Equation` | `block()` → `Block::Math { source: body full_text trimmed, dialect: Typst }`; else `Inline::Math` |
| `#ident` | a recorded literal `let` (§2.6): `Str`/`Int`/`Float` → `Text`, content → its walked markup; else `E-TYP-IDENT-UNKNOWN` |
| `CodeBlock`, `Conditional`, loops, `Closure`, `Contextual`, `Binary`, `Unary`, other `FieldAccess`, `DestructAssignment`, `FuncReturn`, `LoopBreak/Continue` | `E-TYP-CODE-UNSUPPORTED`, dropped |

Equations are kept verbatim (`Block::Math`/`Inline::Math`, dialect `Typst`), `#` code included: math.md
owns all math-level diagnostics; code inside math is diagnosed per target by math.md §9
(`E-TYP-MATH-CODE`, LaTeX only) and by the `equation_mathml` fallback (math.md §3.5).

> **Table capacity (coordinator, 2026-10-28):** Structured tables are bounded
> to 65,536 grid slots across their sections. Checked column/span expansion
> beyond that budget emits E-TYP-ARG-INVALID and drops the table call while
> retaining surrounding content. Native PDF compilation remains independent.
> Auto/percent track mixtures and all-fr ratios are supported; fr mixed with
> auto/percent is unsupported. Cell auto/vertical-only alignment inherits the
> column instead of replacing its horizontal alignment.

> **Literal content capacity (coordinator, 2026-10-28):** Captured content
> shares a replay allowance across included files: at most 64 active content
> expansions and 65,536 replayed expressions/content entries per parse. The
> first exceeded limit emits a located E-TYP-ARG-INVALID and drops that replay.
> Direct source markup does not consume this allowance. This bounds recursive
> literal bindings without evaluating code or changing native PDF behavior.
> Each decoded literal value also has a 65,536-node, 64-level and 8 MiB string
> allowance, checked before cloning bound arrays/dictionaries. Excess values
> emit E-TYP-ARG-INVALID. Ordinary literal arrays and aliases remain supported.

### 2.3 Labels
A `Label` attaches to the nearest preceding sibling in the same `Markup` that is not `Space`,
`Parbreak` or a comment (typst-eval 0.15.1 `markup.rs`: `Unlabellable` = space, parbreak, tag;
*probe*: `= H⏎⏎<sec-x>` labels the heading); none (start of the markup) → `W-TYP-LABEL-ORPHAN`.
Targets: `Heading` → `attrs.id`; block `Equation`, `math.equation`, `mitex` → `Block::Math.attrs.id`;
`figure` → `Figure.attrs.id`; `table` → `Table.attrs.id`; `Raw`/`raw`/`listing` → `CodeBlock`/`Code`
`attrs.id`; `link`, `box(image(..))` → `Link`/`Image` `attrs.id`; `metadata(none)` → §2.4's empty
`Div`/`Span`; `footnote[..]` → `Footnote.label` (a label `writer-fn-X` yields `label: Some("X")`,
§5.6); `callout` → `Callout.attrs.id`; `code-cell` → `Cell.attrs.id`; anything else →
`W-TYP-LABEL-IGNORED`. Labels already satisfy references.md's alphabet; `label("x y")` is
`label-invalid`.

> **Labelled plain tables (coordinator, 2026-10-28):** Keep the label on the
> inner Table, with reference kind Other. After figure inference, normalize a
> labelled Table outside an existing Table figure into an unnumbered,
> captionless Table figure with no outer id and the same span. This satisfies
> IR invariants without assigning a table number or changing reference kind.
> If that table contains nested figures, carry its id on a containing Div and
> clear only the table id instead; this is the representable Other-kind anchor
> without forbidden figure nesting. Enclosing non-Other figures downgrade when
> required by the same invariant, without discarding labels or content.

### 2.4 Allow-listed calls (callee `Ident` or `FieldAccess`, optionally through an import alias)
Named arguments not listed are `W-TYP-ARG-IGNORED`, except the *silent* ones (layout-only, PDF
only). A required argument missing or of the wrong shape is `E-TYP-ARG-INVALID`; `..spread` is
`E-TYP-ARG-SPREAD`; any other callee is `E-TYP-CALL-UNKNOWN`.

| call | IR | mapped args · *silent args* |
|---|---|---|
| `figure(body, caption:, kind:, numbering:)` | `Figure { kind, caption, body, attrs }` | kind: explicit `image`→Image, `table`→Table, `raw`→Listing, any string→Other + `W-TYP-FIGURE-KIND`; else from body (a content block holding only one such child counts as that child): `image(..)`→Image, `table(..)`→Table, `Raw`/`raw(..)`/`listing(..)`→Listing, anything else (incl. `Equation`) → Other; a body not matching ir.md §2.2's shape for the kind → Other; `numbering: none` → class `unnumbered` (references.md §2.3) · *placement, scope, gap, outlined* (`supplement`, other `numbering` values warn) |
| `image(path, width:, height:, alt:)` | `Paragraph[Image]` at block level; inside `box(..)` → inline `Image` | path `Str` or `path(Str)`, relative to the file (`/x` = World root, §6.1), registered with `AssetRegistry::register_file`; missing → `missing-asset` (markdown-dialect.md §4.3); width/height `Numeric` in pt/mm/cm/in/em or `%` → `attrs.pairs` (`"70%"`, `"3cm"`), `auto` → none; alt `Str` → `[Text]` · *fit, scaling, format, page* |
| `table(columns:, align:, ..cells)` | `Table` | columns: `Int` n, or an array of tracks (`auto` → `width: None`; `x%` → `Some(x/100)`; all-`fr` arrays → `fr/sum`); align: one alignment or an array per column (`left`/`start`→Left, `center`→Center, `right`/`end`→Right, `auto`→Default; `x + y` takes the horizontal part); cells fill a column×row grid row-major honouring colspan/rowspan, short last row padded with empty cells; cell content must be one paragraph (else paragraphs joined with `LineBreak`, other blocks dropped, `W-TYP-TABLE-CELL`); `Str` cells → `Text` · *rows, stroke, inset, fill, gutter, column-gutter, row-gutter* |
| `table.header(..cells)`, `table.footer(..cells)`, `table.cell(colspan:, rowspan:, align:)[..]` | `head` / `foot` rows, `TableCell` | `header: true` for header cells · *repeat, fill, stroke, inset*; `x:`/`y:` warn |
| `table.hline(..)`, `table.vline(..)`, `v(..)`, `h(..)`, `colbreak(..)` | nothing | silent |
| `quote(block:, attribution:)[..]` | `block: true` → `BlockQuote`; else `Text("“") + inlines + Text("”")` | `attribution` warns |
| `list(tight:, ..items)`, `enum(start:, tight:, ..items)`, `terms(..terms.item(t, d))` | `List` / `DefinitionList` | items are content blocks walked as blocks |
| `align(dir)[..]`, `block[..]`, `par[..]`, `box[..]`, `text(..)[..]` | `Div { classes: [dir] }` for `left`/`center`/`right` else `Div {}`; `par` → its blocks; `box`/`text` → `Span { attrs: {} }` | all other args silent |
| `strong`, `emph`, `underline`, `strike`, `smallcaps`, `super`, `sub` `[..]` | `Strong`, `Emph`, `Underline`, `Strikeout`, `SmallCaps`, `Superscript`, `Subscript` | |
| `overline[..]`, `highlight[..]` | `Span { classes: ["overline"] }` / `["highlight"]` | |
| `raw(text, lang:, block:)` | `Code` / `CodeBlock` | text must be `Str` |
| `heading(level:, numbering:, outlined:)[..]` | `Heading` (level default 1, > 6 as §2.2); `numbering: none` → class `unnumbered`; `outlined: false` → class `notoc` (references.md §2.2) | *bookmarked* |
| `math.equation(block: true, numbering: none, $ … $)` | `Block::Math` (dialect `Typst`, class `unnumbered`) | the only accepted shape (§5.4) |
| `metadata(none)` + label | the only content of its paragraph → `Div { attrs: { id }, blocks: [] }`, else `Span { attrs: { id }, inlines: [] }` (the emitter's anchor, §5.6) | other `metadata(..)` silent |
| `link(dest)[body]` | `Link`; dest `Str` → url, `Label` → `#label`; no body → `[Text(dest)]` | |
| `footnote[body]` / `footnote(<l>)` | `Footnote { blocks, label }` / `FootnoteRef { label }` (`writer-fn-` stripped as in §2.3) | *numbering* |
| `cite(<key>, form:, supplement:)` | `Cite { items: [{ key, locator: plain text of supplement }], mode }`; `form`: `"normal"`/`"prose"`/`"year"`/`"author"`/`"full"`/`none`; `cite` calls separated only by `Space` form one `Cite` group (a call with a different form starts a new group) | `style` warns |
| `ref(<l>, form:, supplement:)` | `Ref { form: Normal \| Page }` | |
| `pagebreak(..)` / `line(..)` / `outline(depth:)` / `linebreak()` | `PageBreak` / `ThematicBreak` / `Toc { depth }` (default: effective `document.toc` depth, else 3) / `LineBreak` (§2.2) | *weak, to, length, title, indent* |
| `bibliography(sources, style:)` | `Bibliography`; `sources` (`Str`, `path(Str)`, array) → `vars.bibliography`, `style` → `vars.cite_style` (§3) | *title, full* |
| `callout(kind:, title:)[body]` | `Callout { kind (default "note"), title, blocks }` | |
| `mi(src)` / `mitex(src, numbering:)` | `Inline::Math` / `Block::Math` with `dialect: Latex`; `src` a `Raw` (its text) or `Str`, minus a leading copy of `macros_as_newcommands(&macros_from_config(&config.math.katex_options))` of the effective config (math.md §6); `numbering: none` → class `unnumbered` | |
| `listing(numbered:, start:, highlight:, plain:)[raw]` | `CodeBlock` with class `numbered`/`plain`, pairs `start`, `highlight` | |
| `code-cell(index:, count:, lang:, input:)[out]..` | `Cell` with `index`, `language` = lang, `source` = the input raw's text, `execution_count` = count, `show_input` = input is not `none`, `show_output` = any outputs; per trailing block one `CellOutput` of kind `Display` with one candidate (mime `text/x-typst`, the walked blocks) | |
| `include-notebook(file, cells:, tags:, hide-input:, hide-output:)` (`file` = `path(Str)` or `Str`) | the blocks of `notebook::parse_with(source, json, &effective_config, ParseOptions { includes: &mut *crate::markdown::include::cell_includes_in(source, &chain), nested: true, selection })` (`source` = the notebook, `chain` as §2.7; notebook-mapping.md §7.3, authoritative; T53) | |

### 2.5 Literal arguments
`Str`, `Int`, `Float`, `Bool`, `none`, `auto`, `Numeric`, `Label`, `ContentBlock`, alignment idents
(`left right center top bottom horizon start end`) and their `+`, `Unary(-)` of a number,
`Array`/`Dict` of literals, `path(Str)`, `label(Str)`, `datetime(year:, month:, day:)`, idents bound
by a literal `let`, and allow-listed calls where §2.4 expects them (`image` in `figure`, `table.*` in
`table`). Trailing `[..]` blocks are positional arguments (R2 §3.1).

### 2.6 `show`, `set`, `let`, `import`
- **`#show: writer-doc.with(..)`** (also `#show: writer-doc`, or `alias.writer-doc.with(..)`) as a
  child of the root `Markup`: the first one is parsed per §3; later ones and any other show rule are
  `W-TYP-SHOW-IGNORED`. The name is matched whatever module exports it (a BYO template, §4.6).
- **Top-level set rules** (children of the root `Markup`) write `DocMeta` or front-matter-style keys
  into `DocMeta.vars` (§3), later in source order wins: `set document(title:, author:, date:,
  keywords:, description:)` → `DocMeta`; `set text(lang:)` → `lang`; `set heading(numbering: p|none)`
  → `numbering.headings = p|false`; `set math.equation(numbering: none|p)` → `numbering.equations =
  "none"|"all"`; `set page(paper:)` → `paper`; `set bibliography(style:)` → `cite_style`. Other args
  of these targets and set rules on `par table raw list enum terms block image figure.caption
  footnote.entry quote outline align` are silent; anything else, and set rules not at the top
  level, is `W-TYP-SET-IGNORED`.
- **`let`**: `#let name = <literal>` binds for the rest of the enclosing markup (a scope per content
  block); closures and destructuring are `W-TYP-LET-IGNORED` (later calls → `E-TYP-CALL-UNKNOWN`).
- **`import`**: `"@writer/doc:<v>"` and `"@preview/mitex:<v>"` (any item list, `*`, `as alias`) are
  recognised; `<v>` ≠ `WRITER_DOC_VERSION` / `MITEX_VERSION` is `W-TYP-PACKAGE-VERSION`. Every
  other import is `W-TYP-IMPORT-IGNORED` and names it imports are unknown callees.

### 2.7 Includes (T50 for `.typ`, T53 for `.md`/`.ipynb`)
`#include "p"` (or `path("p")`) at block level: `p` relative to the including file; kind by
`ir::detect_kind`. `.typ` → this frontend recursively; `.md` → `markdown::parse` with a clone of the
effective config whose `document.toc` is `Toc::Off` (no `Toc` at the include position) and
markdown-dialect.md §8's rules (front matter → `include-meta-ignored`). `markdown::parse` merges the
included file's own front matter over that clone (ir.md §4), so a `toc` key there still makes it
insert its `Toc` (markdown-dialect.md §2.1); the arm therefore removes `Parsed.document.blocks[0]`
when it is a `Block::Toc` with `span: None` (only that inserted `Toc` has no span, ir.md §2.8; an
explicit `:::toc` keeps its span and stays), as markdown-dialect.md §8 requires; `.ipynb` →
`notebook::parse_with(source, json, &effective_config, ParseOptions { includes: &mut
*crate::markdown::include::cell_includes_in(source, &chain), nested: true, selection:
Selection::default() })` (`source` = the notebook; `chain` = the canonical paths of the `.typ` files
from the main file to the including one, outermost first; notebook-mapping.md §7.3; never
`notebook::parse`). T50
writes the `.typ` arm and one `.md`/`.ipynb` arm raising `E-TYP-INCLUDE-MISSING` ("including
.md/.ipynb from typst arrives in T53"); T53 replaces that arm. Blocks are spliced in place; assets,
features and diagnostics merged; every block gets `SourceSpan.file`; `Document.includes` lists every file,
transitively, in first-use order. Missing/unreadable → `E-TYP-INCLUDE-MISSING`; a cycle (message names
the chain) → `E-TYP-INCLUDE-CYCLE`; nesting deeper than 8 → `E-TYP-INCLUDE-DEPTH`; a non-literal path
→ `E-TYP-INCLUDE-DYNAMIC`. The PDF compile serves `.md`/`.ipynb` through §6.4.

> **Included metadata (coordinator, 2026-10-28):** A Typst child uses its own
> effective config while walking, but never changes the outer DocMeta. Explicit
> nonempty show/set metadata emits include-meta-ignored at the include site.
> Remove only a synthetic span-None child TOC; keep explicit outline blocks.
> Invalid known literal argument shapes emit E-TYP-ARG-INVALID and skip the
> argument; nonliteral template arguments emit E-TYP-TEMPLATE-ARG. Remaining
> genuine config validation failures keep the existing Result error contract.

### 2.8 Diagnostic codes
| code | level | meaning |
|---|---|---|
| `E-TYP-SYNTAX` · `E-TYP-CODE-UNSUPPORTED` | error | parser error (message, hints from `SyntaxDiagnostic`) · code outside the subset |
| `E-TYP-CALL-UNKNOWN` · `E-TYP-IDENT-UNKNOWN` | error | callee not allow-listed · `#x` without a literal binding |
| `E-TYP-ARG-INVALID` · `E-TYP-ARG-SPREAD` · `E-TYP-TEMPLATE-ARG` | error | wrong/missing argument shape · `..args` · non-literal `writer-doc.with` argument (skipped, §3) |
| `E-TYP-INCLUDE-MISSING`, `-CYCLE`, `-DEPTH`, `-DYNAMIC` | error | §2.7 |
| `W-TYP-SYNTAX` · `W-TYP-FIGURE-KIND` | warning | parser warning (as `E-TYP-SYNTAX`) · a string `kind:`: its own counter in the PDF, the image counter in HTML/DOCX/LaTeX (references.md §2.3) |
| `W-TYP-ARG-IGNORED`, `-SET-IGNORED`, `-SHOW-IGNORED`, `-LET-IGNORED`, `-IMPORT-IGNORED` | warning | construct kept by the PDF only |
| `W-TYP-LABEL-ORPHAN`, `-LABEL-IGNORED` · `W-TYP-TABLE-CELL` | warning | §2.3 · table cell content is not one paragraph |
| `W-TYP-PACKAGE-VERSION` · `W-TYP-PATH-STRING` | warning | imported `@writer/doc`/`@preview/mitex` version ≠ embedded · a `str` where the package needs `path(..)` (§3) |
| `W-TYP-DUPLICATE-BLOCK` · `W-TYP-TEMPLATE-IGNORED` | warning | `toc`/`bibliography` given to `writer-doc` and also placed explicitly (the PDF shows both) · `[pdf] template` set for a `.typ` input (T52) |

`unresolved-reference`, `duplicate-label`, `ambiguous-label`, `unnumbered-target` and
`unknown-citation-key` are resolve's, raised on this frontend's `Ref`/`Cite` shapes exactly as for
Markdown (markdown-dialect.md §7.3).

## 3. `writer-doc.with(..)` arguments (T50 parses, T31 implements, T30 emits)

The vocabulary is config.md §3's front-matter keys in kebab-case: those arguments, and every key at
every nesting level of their dict values (`numbering`, the table maps), convert to the front-matter
spelling by replacing `-` with `_` (`cite-style` → `cite_style`, `pdf: (allow-packages: true)` →
`allow_packages`). Every other argument keeps its name verbatim, as a front-matter key does, so
pandoc's `toc-title` stays `toc-title`, the name html-output.md and docx-output.md read. The
frontend writes `DocMeta` fields directly and everything else into `DocMeta.vars` (as front matter
would), so
`Config::merge_front_matter` applies it with the same precedence (config.md §1). Values must be §2.5
literals (`E-TYP-TEMPLATE-ARG` otherwise); `auto` means "not given".

| argument | type | default | → | package use |
|---|---|---|---|---|
| `title`, `subtitle` | content \| str \| none | none | `DocMeta.*` (plain text) | title block, `document(title:)` |
| `author` / `authors` | str / array of str or dicts (`name, affiliation, email, id`) | () | `DocMeta.authors` | title block, `document(author:)` |
| `date` | str \| `datetime(..)` \| none | none | `DocMeta.date` (`"YYYY-MM-DD"` for datetime) | title block; PDF date (§8.4) |
| `abstract` | content \| none | none | `DocMeta.abstract_` (walked as blocks) | abstract block |
| `keywords`, `description` | str or array / str | (), none | `DocMeta.*` | `document(..)` |
| `lang` | str | config `document.lang` | `DocMeta.lang` | `set text(lang:)` |
| `toc` | bool \| int 1–6 | false | `[document] toc` | `outline(depth:)` after the title block |
| `numbering` | bool \| `(headings: false\|true\|str, figures: "document"\|"section", equations: "labelled"\|"all"\|"none")` | config | `[numbering]` | numbering policy (§4.3) |
| `bibliography` | `path(..)` \| array of `path(..)` | none | `[references] bibliography` | `bibliography(..)` appended after the body |
| `cite-style` / `csl` | str (style name) \| `path(..)` (.csl) | `"ieee"` | `[references] style` | `bibliography(style:)` |
| `cite-locale` | str | `auto` (→ `writer-config` `"cite_locale"`) | `[references] locale` (as `vars.cite_locale`: the generic `-` → `_` rule of this section, read by `Config::merge_front_matter`, config.md §3) | locale rules (references.md §3) |
| `paper` | str | `"a4"` | `[pdf] paper` | `set page(paper:)` |
| `toc-title`, `abstract-title`, `reference-section-title` | str \| content (plain text) | `auto` | `DocMeta.vars` under that name (pandoc's variables: html-output.md §1.3, §1.10, §1.12; docx-output.md §4.4, §4.13, §4.14) | titles of the outline, the abstract and the bibliography (§4.2) |
| `theme`, `template`, `reference-doc`, `latex-template` | str | — | config.md §3 | ignored |
| `markdown`, `math`, `references`, `html`, `pdf`, `docx`, `latex`, `typst`, `notebook` | dict | (:) | the same tables | uses `pdf.paper` |
| anything else | literal | — | `DocMeta.vars` | ignored (`..extra` sink) |

typst 0.15.1 resolves a **string** path inside the package function that uses it, so
`bibliography: "refs.bib"` fails to compile (typst-library 0.15.1 `PathOrStr::resolve`: a string
resolves relative to the file where the function is called, here inside `@writer/doc`'s root;
*probe*: `bib("refs.bib")` defined in a package fails, `bib(path("refs.bib"))` works). The frontend
accepts a `str` with `W-TYP-PATH-STRING`; the package `panic`s with "writer-doc: pass bibliography:
path(\"refs.bib\")".

## 4. The `@writer/doc` package (T31; `cells.typ` T31 + T53)

### 4.1 Files (`assets/typst/writer-doc/`, embedded by `crate::vendor`, vendored-assets.md §5)
`typst.toml` (`[package] name = "doc"`, `version = "0.1.0"`, `entrypoint = "lib.typ"`,
`compiler = "0.15.1"`, `license = "MIT"`, `description`); `lib.typ` (entrypoint: `writer-doc`,
`listing`, re-exports `callout`, `code-cell`, `include-notebook`); `callout.typ`; `numbering.typ`;
`cells.typ`; `oracle.typ`; `template.typ` (the starter that `eject typst-template` writes, §4.6).
The package uses only typst built-ins (no `@preview` imports); LaTeX math needs the separate
`#import "@preview/mitex:0.2.7": mi, mitex`. Versioning: `WRITER_DOC_VERSION` changes with any change
of the argument vocabulary or output (minor before 1.0); emitted files import exactly it; `check`
warns on others (`W-TYP-PACKAGE-VERSION`).

> **Initial implementation clarification (T53, 2026-10-29):** T53 completes
> the deferred helper in T31's unreleased `0.1.0` package. Keep the initial
> version/import fixtures during that completion; after first publication,
> every package-file change updates `WRITER_DOC_VERSION` as specified above.

### 4.2 `writer-doc(..)` and layout defaults
`#let writer-doc(title: none, subtitle: none, author: none, authors: (), date: none, abstract: none,
abstract-title: auto, keywords: (), description: none, lang: auto, toc: auto, toc-title: auto,
numbering: auto, bibliography: none, reference-section-title: auto, cite-style: auto, csl: auto,
cite-locale: auto, paper: auto, …table dicts…, ..extra, body)`;
positional `extra` is a `panic`. Parameters shadow the built-ins `title`, `numbering`,
`bibliography`, `math` and `pdf` (global modules in typst 0.15.1, `typst-library` `lib.rs`; *probe*:
`show math.equation` with a `math: (:)` parameter fails, "dictionary does not contain key
"equation""), so the body uses `std.title`, `std.numbering`, `std.bibliography`, `std.math.equation`
and `std.pdf`. An `auto` value takes `sys.inputs.writer-config` (§6.2) and then the built-in
default. Under writer (`"writer-version" in sys.inputs`) the package leaves `document(date: auto)`
so writer's PDF timestamp applies; otherwise an ISO `YYYY-MM-DD` `date` becomes `document(date:
datetime(..))`. Defaults, embedded fonts only: `page(paper, margin: (x: 2.5cm, y: 2.5cm), numbering:
"1")`, `text(font: "Libertinus Serif", size: 11pt, lang: L, region: R)` (references.md §4.5's
`LangTag::parse` of `lang`), `par(justify: true)`, `show std.math.equation: set text(font: "New
Computer Modern Math")`, `show raw: set text(font: "DejaVu Sans Mono")`, and with a `cite-locale`
references.md §3's `show cite`/`show bibliography` rules and bibliography `title:`. Order: title
block (title 17pt, subtitle, authors joined by ", " with affiliations below, date), abstract
(heading `abstract-title`, else §4.3's dictionary word `abstract`; indented 2em, under `set heading(numbering:
none, outlined: false)`), `outline` when `toc`, body, `bibliography(..)` when `bibliography` is given;
references.md §3's numbering rules cover all of them. A non-`auto` `toc-title` /
`reference-section-title` becomes `set outline(title: ..)` / `set std.bibliography(title: ..)` over
the whole document (explicit `#outline`/`#bibliography` calls follow it) and replaces the
citation-locale `title:` of the bibliography the package places.

### 4.3 Callouts, numbering, figures
- `callout(kind: "note", title: auto, body)`: `title: auto` → the dictionary word for the kind
  (below; "Note", "Tip", "Important", "Warning", "Caution" in English; other kinds: the kind with
  its first character uppercased, as html-output.md §1.4); the five kinds have distinct accent
  colours, others neutral; a `block(breakable: true, width: 100%)`.
- **Writer-only words.** typst's translations lack `abstract`, `note`, `tip`, `important`,
  `warning` and `caution`, so `callout.typ` embeds a dictionary of them per language holding the
  words of `assets/l10n/*.toml` (references.md §4.5; T01 ships `en.toml` only), looked up like
  `resolve::term`: in a `context`, `text.lang + "-" + upper(text.region)` (when the region is not
  `none`), then `text.lang`, then `"en"`. `lib.typ` imports it for the abstract heading (§4.2).
  A T31 test compares, for every `assets/l10n/*.toml` file and every key, the embedded word with
  `resolve::term`, and fails on an embedded language that has no file, so the PDF prints the
  HTML/DOCX/LaTeX words (as for Bibliography below).
- `numbering.typ` and `oracle.typ` are references.md §3 (its code is normative, incl. the only
  `show ref` rule: section-mode equation references; supplements are typst's own localized words,
  which `resolve::supplement` copies). `numbering.typ` embeds references.md's term Bibliography per
  language for the `title:` under a citation locale (a T31 test compares it with `resolve::term`).
  Figure kinds are typst's `image`, `table`, `raw`; writer's `Other` figures use `kind: image`.
- **Caption-less numbered figures.** typst shows no caption block, and so no "Figure 1", for a
  figure without `caption:`, although it counts it and `@fig-x` reads "Figure 1"; html-output.md
  §1.6, docx-output.md §4.10 and latex-output.md §4.5 print the label for such a figure (ir.md §1
  allows `caption: None` with an id). So for a figure whose `caption` is `none` and whose
  `numbering` is not `none`, the package prints one centred line `[#it.supplement#sym.space.nobreak#n]`,
  `n` = `numbering(it.numbering, ..it.counter.at(it.location()))` in a `context`, with no separator,
  where the caption would go: directly above `it` for `kind: table` and `raw`, directly below
  otherwise (spacing `it.gap`). T31 implements it inside `writer-numbering`'s `show figure` rule of
  references.md §3, keeping the oracle call (CROSS: references.md §3's code gains this branch). The
  emitter (§5.4) still writes no `caption:` argument, so §7's round trip is unchanged. A T31 test
  compiles a labelled caption-less image under both figure policies (with heading numbering on)
  and finds "Figure 1" / "Figure 1.1" in the PDF's text.
- `listing(numbered: false, start: 1, highlight: "", plain: false, body)`: wraps one raw block;
  line numbers from `start`; `highlight` uses the `"1-3,5"` grammar of markdown-dialect.md §4.6;
  `plain` re-creates the raw without `lang`.

### 4.4 `cells.typ`
`code-cell(index: 0, count: none, lang: "", input: none, ..outputs)`: the input raw block, then each
output; no prompts (config.md: `[notebook] show_prompts` is HTML and DOCX; notebook-mapping.md §9).
`include-notebook(file, cells: auto, tags: none, hide-input: auto, hide-output: auto)` (T53,
additive): notebook-mapping.md §7.3 is authoritative (`file` a `path`, imports `writer-cells` from
it, selection semantics, panics).

### 4.5 Oracle (`oracle.typ`)
references.md §3 and §7 own it: with `sys.inputs.writer-oracle == "1"` every labelled heading,
figure, block equation and footnote is followed by `#metadata((kind: K, label: L, number: N))
<writer-oracle-L>`, `N` as typst displays it (`"1."`, `"(1.2)"`) or `none` when unnumbered;
`writer-fn-…` footnotes get none.

### 4.6 BYO template (`[pdf] template`: T30 writes the import, T32 mounts it, T34 tests it)
A `.typ` file exporting `writer-doc`, a function `(..args, body) => content` accepting §3's arguments
(it may ignore any), usually importing `@writer/doc` and delegating to it. For Markdown/notebook
inputs the emitter imports it after the package (§5.3) so it shadows `writer-doc`. T32's
`load_typst_main` (live-reload.md §5.2, used by `build --to pdf` and serve's pages view) mounts the
template's directory at `/__writer/template/` (§6.3) and passes `EmitOptions.template =
Some("/__writer/template/<file name>")`; T34 copies the template for `--to typst` (§10) and tests
this path end to end. `eject
typst-template FILE` writes `template.typ`: `#import "@writer/doc:0.1.0" as base` and `#let
writer-doc(..args, body) = { show heading: set text(fill: rgb("#1a3a8a")); base.writer-doc(..args,
body) }` with comments naming every argument.

## 5. IR → typst emitter (T30)

### 5.1 API
```rust
// src/typst/emit.rs
pub struct EmitOptions<'a> {
    pub config: &'a Config,
    pub assets: AssetPaths,
    pub template: Option<String>,     // import path of a BYO template (§4.6), e.g. "/__writer/template/my.typ"
    pub bibliography: Vec<String>,    // paths as written into `#bibliography(..)`
    pub style: String,                // its `style:` (a style name, or a path string)
    pub footnote_prefix: String,      // "" for documents; "<8 hex of blake3(root-relative path)>-" for includes (§6.4)
    pub fragment: bool,               // an unresolved §6.4 include: no `#show` rule, §5.6's fragment rules
}
pub enum AssetPaths {
    Virtual,                          // `/__assets/<id>.<ext>` (compile, `dump --typst`)
    Export { dir: String },           // `<dir>/<name>` (`--to typst`, §10)
    Source,                           // a File asset's path relative to `source.base_dir` (round trip)
}
pub struct Emitted { pub text: String, pub map: SourceMap, pub diagnostics: Diagnostics,
                     pub assets: BTreeMap<String, String> /* asset id → path written into `text` */ }
pub fn emit(document: &Document, assets: &AssetRegistry, opts: &EmitOptions) -> Result<Emitted>;
/// One part per run of top-level blocks [start, end); the main text gets `#include "<file>"` at `start`.
pub struct IncludeRun { pub start: usize, pub end: usize, pub file: String }
pub fn emit_split(document: &Document, assets: &AssetRegistry, opts: &EmitOptions, runs: &[IncludeRun])
    -> Result<(Emitted, Vec<Emitted>)>;
// src/typst/sourcemap.rs (T03, complete incl. from_lines, PLAN §3.2; T30's emit and T53's .ipynb module
// build their maps with from_lines, notebook-mapping.md §7.3)
pub struct SourceMap { /* one Option<SourceSpan> per emitted line */ }
impl SourceMap { pub fn lookup(&self, line: usize /* 1-based */) -> Option<&SourceSpan>; pub fn len(&self) -> usize;
                 pub fn from_lines(lines: Vec<Option<SourceSpan>>) -> SourceMap; }
```
`emit` fails only on an `asset:` placeholder missing from the registry (ir.md §3: build error); its
warnings (`math-conversion-failed`, `math-tag-ignored`, `unsupported-construct`) are in `diagnostics`.

### 5.2 Escaping
| context | rule |
|---|---|
| markup text (`Text`, captions, titles) | backslash before every ``\ # * _ ` $ < > @ [ ] ~ ' "`` (*probe*: `me@example.com` is a `Ref`, `a<b>` a `Label`, `'`/`"` smart quotes) |
| markup `-`, `.`, `/` | `-` before `-`, `?` or a digit (shorthands); the first `.` of every `...`; `/` before `/` or `*` (comments; also defuses `https://` auto-links, *probe*) |
| markup, first non-blank character of a line | `=`, `+`, `-`, `/`, and the `.` of a leading `\d+\.`; a line-starting `Text` loses its leading whitespace |
| after a `#…` code expression | emit `;` when the next character is `.`, `(` or `;` (*probe*: `#strike[x];.Hello` keeps `.Hello` as text) |
| definition-list term | the markup-text rules plus `\` before every `:` (typst ends the term at the first `:`; *probe*: `/ Note: x: def` → term `Note`) |
| string literal `"…"` | `\\`, `\"`, `\n`, `\r`, `\t`, other control characters `\u{XX}` |
| array / dict literal | `()` empty, `(x,)` one element (`("A")` is a string, *probe*), `(a, b)` otherwise; `(:)` empty dict |
| label | verbatim `<id>` (the alphabets agree) |
| raw block | fence of `max(3, longest backtick run + 1)` backticks at the current indentation, language right after it; lines indented like the fence (empty lines stay empty); closing fence on its own line (*probe*: typst dedents by exactly that indentation). A `lang` failing `typst_syntax::is_ident` (`c++`, `c#`, `html.j2`; typst reads only an identifier: ```` ```c++ ```` → lang `c`, first line `++`, *probe*) → `#raw(block: true, lang: "c++", "…")` with the string rules instead, also inside `#listing(..)[..]` and `#figure(..)` |
| LaTeX math argument | `` `src` `` when `src` has no backtick (single-backtick raws may span lines, *probe*), else a string literal |

### 5.3 File skeleton
```typst
// Generated by writer from document.md; edit the source, not this file.
#import "@writer/doc:0.1.0": *
#import "@preview/mitex:0.2.7": mi, mitex          // only if the document has LaTeX math
#import "/__writer/template/my.typ": writer-doc     // only with [pdf] template (§6.3)
#show: writer-doc.with(
  title: [GPU allocation], authors: ("J. Blaschke", (name: "A. Colleague", affiliation: "EIT")),
  date: "2026-10-24", abstract: [One paragraph.], lang: "en",
  numbering: (headings: "1.1", figures: "section", equations: "labelled"), paper: "a4", toc: false,
)
```
Arguments in this order, each only when set: `title subtitle authors date abstract abstract-title
keywords description` (`abstract-title` = `meta.vars["abstract-title"]` when a string, §3; the
other two title vars go into §5.4's `#outline`/`#bibliography` calls); always `lang` (meta, else
config), `numbering` (effective config), `paper` and `toc: false` (the IR carries the `Toc` blocks; `auto` would add `writer-config`'s outline as well);
`cite-locale: "<code>"` when `[references] locale` is non-empty (references.md §3). `bibliography`
is never passed (the IR carries the `Bibliography` block). Fragments and `emit_split`'s parts have
the import lines but no show rule. Blocks are separated by one blank line; LF line ends, no trailing
spaces, final newline.

### 5.4 Blocks
| IR | typst |
|---|---|
| `Heading` | `=`×level + ` ` + inlines + ` <id>`; class `unnumbered` (while heading numbering is on, references.md §2.2) / `notoc` (alias `unlisted`) → `#heading(level: L, numbering: none, outlined: false)[..] <id>`, each argument only for its class |
| `Paragraph` | inlines; a paragraph whose only inline is an `Image` → `#image(..)` |
| `BlockQuote` | `#quote(block: true)[` ⏎ blocks (indented 2) ⏎ `]` |
| `List` | `- ` / `+ ` items (first ordered item `N. ` when start ≠ 1); continuation and nested blocks indented 2; loose lists: blank line between items; task items: `Text("☐ ")`/`Text("☒ ")` prepended |
| `DefinitionList` | `/ term: ` (§5.2 term escaping, §5.5 breaks) + the definitions' blocks (first paragraph inline, the rest indented 2) |
| `CodeBlock` | raw fence (§5.2); non-default code attributes (markdown-dialect.md §4.6: `plain`, `numbered`, `start`, `highlight`) → `#listing(numbered: true, start: 3, highlight: "1-3,5")[` fence `]` |
| `Figure` Image / Table / Listing | `#figure(image(..) \| table(..) \| fence, caption: [..]) <id>`; class `unnumbered` adds `numbering: none` (references.md §2.3) |
| `Figure` Other | `#figure(kind: image, [` ⏎ blocks ⏎ `], caption: [..]) <id>` (`numbering: none` likewise); inner figures nest |
| `Table` | `#table(columns: (auto, 30%), align: (left, right), table.header([..], ..), cells.., table.footer(..))`; widths `Some(w)` → `w×100%`; `align` omitted when all `Default`; a cell with spans or `align` → `table.cell(colspan:, rowspan:, align:)[..]` |
| `Math` | typst `$ src $`; LaTeX `#mitex(a)`, `a = latex_source::for_typst(source, &macros_from_config(&config.math.katex_options)).0` written per §5.2 (`math-tag-ignored` when it removed something); `mitex::convert_math(&a, None)` erring → `$ #raw("<source>") $` + `math-conversion-failed` (math.md §6); class `unnumbered` → `#math.equation(block: true, numbering: none, $ src $)` / `#mitex(a, numbering: none)` (references.md §2.4); then ` <id>` |
| `Callout` | `#callout(kind: "warning", title: [..])[` ⏎ blocks ⏎ `] <id>` (`title` omitted when `None`) |
| `Div` | `#metadata(none) <id>` when labelled (§5.6), then its blocks (ir.md §1) |
| `Raw` | `typst`: text verbatim; other formats skipped (`check` reports `raw-skipped`) |
| `ThematicBreak` / `PageBreak` / `Toc` | `#line(length: 100%)` / `#pagebreak()` / `#outline(depth: N)`, plus `title: "<meta.vars["toc-title"]>"` (§5.2 string) when that is a string |
| `Bibliography` | `#bibliography(("p1", "p2"), style: "ieee")` from `EmitOptions`, only when `resolved.bibliography` is `Some` (references.md §4.9, §6); `title: "<meta.vars["reference-section-title"]>"` when that is a string, else `title: [<term Bibliography>]` under a citation locale (references.md §3) |
| `Cell` | `#code-cell(index: 3, count: 2, lang: "python", input: [` fence `])[` output blocks `]`…; `input: none` when `!show_input`; no outputs when `!show_output`; each output = `CellOutput::pick("typst")` |

### 5.5 Inlines
`Text` escaped; `Emph`/`Strong` → `_.._`/`*..*`, or `#emph[..]`/`#strong[..]` when a letter or digit
touches either side; `Strikeout` `#strike[..]`, `Subscript` `#sub[..]`, `Superscript` `#super[..]`,
`Underline` `#underline[..]`, `SmallCaps` `#smallcaps[..]`; `Code` → `` `t` `` (no backtick, no
newline, no class) else `#raw("t")` (`lang:` = the first class, if any); `Math` → `$src$` / `#mi(a)`
(`a`, pre-flight and fallback `$#raw("<source>")$` as §5.4); `Link` → `#link("url")[..]` (body
omitted when it is exactly `[Text(url)]`); a `#x` url → `#link(<x>)[..]` when `x` is in
`resolved.labels`, else its inlines only (`link-target-missing`, `#refs` included; references.md
§4.9), and always `#link(<x>)[..]` in fragments; `Image` → `#box(image("..", width: .., height: ..,
alt: ".."))`; `LineBreak` → ` \` + newline; `SoftBreak` → newline + indentation; inside a markup
heading and a definition-list term `SoftBreak` → one space and `LineBreak` → `#linebreak()` (typst
ends both at the first newline; *probe*: `= a⏎b` is a heading `a` and a paragraph `b`, `/ a⏎b: c` a
syntax error); `Span` with class `subcaption` → `#text(size: 0.85em)[..]` (group-panel
sub-captions, notebook-mapping.md §9), any other `Span` → its inlines (labels per §5.6 either way);
`Raw` → `typst` text verbatim, others dropped;
`Footnote` → `#footnote[..]` (block form when not one paragraph) + ` <flabel>`; `FootnoteRef` →
`#footnote(<flabel>)` (§5.6). Image arguments: path per `AssetPaths`; `width`/`height` with `px`
or no unit converted at 0.75 pt/px, other units verbatim;
`alt` = `plain_text(alt)` when non-empty. An image URL that is not `asset:` (remote, `data:`) →
`#link("url")[alt]` plus `unsupported-construct` (typst cannot fetch URLs).

### 5.6 References, citations, footnotes
- **Labels** (references.md §6): an element with `attrs.id` gets ` <id>` after its one typst
  element (inline elements: `<id>` with no space before it, *probe*: `` `t`<x> ``,
  `#link("u")[t]<x>`); `Div` and `Span` emit no single element and get `#metadata(none) <id>`
  (inline `#metadata(none)<id>`) before their children.
- `Ref` (supplement `None` → `@t`, `Some([])` → `@t[]`, else `@t[..]`; references.md §4.5) whose
  target is in `resolved.labels` with `number: None` (unnumbered heading or figure) →
  `#link(<t>)[resolved text]`; `form: Page` → `#ref(<t>, form: "page")` (+ `supplement: [..]`);
  otherwise `@t…`. When the target ends with `.`/`:` or the next character is a label character,
  `#ref(<t>)` / `#ref(<t>, supplement: [..])` is used instead (typst strips trailing `.`/`:`). In
  documents a `Ref` whose target is in no `resolved.labels` and a `Cite` with `resolved: None` print
  the text `[?target]`; fragments are not resolved (§6.4) and write every `Ref` and `Cite`
  syntactically (`@t`, `@key`, `#cite(..)`) for typst to resolve in the whole document.
- `Cite`, per sub-group (references.md §5.4: one starts at each item with a prefix and ends after
  each item with a suffix), sub-groups separated by one space: prefix markup + one space, the items'
  calls separated by single spaces (one typst group, R5 §3.2: `@key` / `@key[locator]` for
  `Normal`, else `#cite(<key>, form: "prose"[, supplement: [locator]])`, `form: none` for mode
  `None`), then one space + suffix markup. A `Cite` separated from the previous `Cite` only by
  whitespace (different modes; resolve merged the others) is preceded by `#h(0pt)`, which stops
  typst's grouping (references.md §6; the frontend drops `h(..)`).
- Footnotes: `flabel` = the label verbatim when `Footnote.label` is a typst label (references.md
  §1.2), else `writer-fn-` + `footnote_prefix` + the Markdown/notebook name (characters outside the
  label alphabet → `_`); the k-th repeat of the same final label in one emitted unit gets `-k`
  (`emit_split`'s main text and parts are one unit); `label: None` (inline `^[..]`) → no label. A
  `FootnoteRef` uses the `flabel` of the `Footnote` with the same number, in fragments of the one
  with the same label in the same scope (references.md §1.2).

> **Contract clarification (coordinator, 2026-10-27):** final footnote labels
> are reserved across the emitted unit. Advance `-k` until the candidate is
> unused, including when a different source name already claimed it: `a`,
> `a-2`, then a scoped repeat of `a` must not define `writer-fn-a-2` twice.
> Image alt text never allocates footnote labels (references.md §4.3).

### 5.7 Determinism, position stability, source map
`emit` is a pure function of its inputs (no clock, environment, machine path or per-build id). The
text emitted for a top-level block depends only on that block, the `resolved.labels` entries of the
targets it references, and the options; an edit changes the edited block's lines and shifts later
ones, so `Source::replace` reparses only that region. `SourceMap` line *i* holds the span of the
innermost block with a span being emitted when line *i* starts; header lines hold `None`.

> **Contract clarification (coordinator, 2026-10-27):** `AssetPaths::Source`
> resolves the explicit source base directory before relativizing File assets,
> matching the registry's canonical file paths. This mode reads directory
> metadata so symlink aliases such as `/tmp` produce valid relative paths.
> Virtual and Export modes remain pure functions of their supplied inputs.

## 6. The World (T03, `src/typst/world.rs`)

### 6.1 Construction, roots, sources
```rust
pub struct WriterWorld { /* LazyHash<Library> ×2 (paged, html), FontStore, slots, mounts, packages, … */ }
pub enum MainFile { Path(PathBuf) /* a .typ under root */, Virtual /* /__writer-main.typ */ }
pub struct WorldOptions { pub root: PathBuf, pub main: MainFile, pub today: Datetime,
    pub system_fonts: bool, pub font_paths: Vec<PathBuf>, pub package_paths: Vec<PathBuf>,
    pub allow_packages: bool, pub inputs: BTreeMap<String, String> }
impl WriterWorld {
    pub fn new(opts: WorldOptions) -> Result<Self>;
    pub fn from_config(config: &Config, root: &Path, main: MainFile, today: Datetime) -> Result<Self>; // inputs: world_inputs(config)
    pub fn set_main_text(&mut self, text: &str, map: SourceMap);      // Virtual main: Source::replace
    pub fn set_assets(&mut self, assets: AssetRegistry);
    pub fn mount(&mut self, prefix: &str, dir: &Path);                 // §6.3
    pub fn set_inputs(&mut self, inputs: BTreeMap<String, String>);    // rebuilds the Libraries only if changed
    pub fn set_include_provider(&mut self, provider: Box<dyn IncludeProvider>);
    pub fn reset(&mut self);                                           // before every rebuild
    pub fn accessed(&self) -> BTreeSet<PathBuf>;
    pub fn include_diagnostics(&self) -> Diagnostics;
    pub fn source_map(&self, id: FileId) -> Option<SourceMap>;
    pub fn real_path(&self, id: FileId) -> Option<PathBuf>;
}
impl typst::World for WriterWorld { /* library, book, main, source, file, font, today */ }
/// §6.2's `writer-version` and `writer-config` entries for `config`; the only producer of that map.
pub fn world_inputs(config: &Config) -> BTreeMap<String, String>;
```
Root = the input's directory (`SourceInfo.base_dir`), typst-cli's default. A `.typ` input reaches
only files below its own directory (typst's sandbox: "path `"../x"` would escape the project root",
typst-library `foundations/path.rs`); documents and fixtures keep bibliographies, images and
includes below the `.typ` file. Main: a `.typ` input is `RootedPath::new(VirtualRoot::Project,
VirtualPath::virtualize(root, input)?).intern()`; Markdown/notebook inputs use `/__writer-main.typ`
(raw typst then resolves relative paths against the root). Sources are retained per `FileId`
(`RootedPath::intern`) across compiles; `reset()` marks disk-backed slots stale, clears
`accessed()`/`include_diagnostics()` and, in debug builds, drops the cached `@writer/doc` files
(live-reload.md §5.3); the next access re-reads and applies `Source::replace`
(unchanged text → no edit); `file()` of a `.typ` id is `Bytes::from_string(source)`. Unchanged inputs
return the same `Source`/`Bytes` and `library()`/`book()` are stable, as typst's process-global comemo
cache needs (R1 §4). `font(i)` = `FontStore::font` (out of range → `None`); `today(_)` =
`Datetime::from_ymd` of the policy timestamp (§8.4), offset ignored. One `WriterWorld` lives for a
whole `build` or `serve` session (render helpers included). `crate::typst::{compile,render,html}`
never call `typst::comemo::evict`; callers evict once per build (live-reload.md §5.3 (c)).

### 6.2 Libraries and inputs
Two Libraries from the same `Dict`: paged (`Library::builder().with_inputs(d).build()`) and HTML
(`.with_features([Feature::Html].into_iter().collect())`), selected by an internal mode flag that only
`html.rs` sets. `sys.inputs`: `writer-version` = the crate version (always); `writer-config` = JSON
of the effective config below the document, `serde_json` compact, keys in this order:
`{"lang": document.lang, "toc": depth (0 = off), "numbering": {"headings": false | "<pattern>"
(`true` written as its pattern "1.1"), "figures": "document" | "section", "equations": "labelled" |
"all" | "none"}, "paper": pdf.paper, "cite_locale": references.locale ("" when unset)}`;
`writer-oracle` = `"1"` only in the oracle test. `world_inputs(config)` (§6.1) is the only producer
of the first two: `from_config` starts with it, `WorldSlot::get` and `load_typst_main`
(live-reload.md §5.2–5.3) pass `world_inputs(&prepared.config)` to `set_inputs`, and the oracle test
(references.md §7) inserts `writer-oracle = "1"` into `world_inputs(&config)`. Nobody serializes the
map by hand, so `set_inputs` sees an equal map for an unchanged config and keeps the Libraries
(and typst's comemo cache) instead of alternating between two spellings.

### 6.3 Virtual files (all under `VirtualRoot::Project`)
| vpath | served from |
|---|---|
| `/__writer-main.typ` | `set_main_text` (Markdown/notebook inputs) |
| `/__assets/<id>.<ext>` | `AssetRegistry::read(id)`; ext from MIME: png, jpg, gif, webp, svg, pdf, else bin |
| `/__writer/template/<name>` | the directory of `[pdf] template` (`mount`) |
| `/__writer/bib/<n>/<name>`, `/__writer/csl/<name>` | the n-th `[references] bibliography` file, a `.csl` style (`mount`) |
| `/__render/main.typ`, `/__render/image.<ext>` | §9 scratch sources and image bytes |
| `*.md`, `*.ipynb` | `source()`: the typst of the `IncludeProvider` (§6.4; `#include` reads `World::source`); `file()`: the bytes on disk (`read`, `image`, data loaders) |
| anything else | `vpath.realize(root)` on disk (`FileError::from_io` on failure) |

`accessed()` holds the real path of every disk file served since the last `reset()` (project files,
mounted files, `File` assets, include sources plus the files their parse read, package files from disk
roots); embedded and virtual files never appear. It feeds the watch set (live-reload.md).

The original `MainFile::Path` is native Typst regardless of its extension,
including an explicit `--format typ` on a `.md` or `.ipynb` filename. Its file
kind remains native while render helpers temporarily switch the main source;
other Markdown/notebook source IDs still use the include provider.

### 6.4 Includes of `.md`/`.ipynb` (trait and `Synthesized`: T03 in `src/typst/world.rs`; `MarkdownIncludes` stub and installation: T16; its body: T53)
```rust
// src/typst/world.rs (T03)
pub trait IncludeProvider: Send + Sync {
    /// Typst for a `.md`/`.ipynb` file requested through `#include`. `path` is the absolute real
    /// path of the included file (`vpath.realize(root)`, also the `SourceInfo.path` the provider
    /// builds); `root` is the World root (§6.1). The World passes both.
    fn synthesize(&self, path: &Path, root: &Path) -> Result<Synthesized>;
}
pub struct Synthesized { pub text: String, pub map: SourceMap, pub assets: AssetRegistry,
                         pub files: Vec<PathBuf>, pub diagnostics: Diagnostics }
```
`IncludeProvider` and `Synthesized` are T03's; T53 writes only `MarkdownIncludes::synthesize`'s
body in `src/typst/includes.rs`.
`typst::includes::MarkdownIncludes { config }`: a `.md` is parsed with `markdown::parse` and a clone
of the effective config whose `document.toc` is `Toc::Off` (a leading `Block::Toc { span: None }`
that `markdown::parse` still returns, §2.7, is removed before `emit`), then emitted with
`fragment: true`, `AssetPaths::Virtual` and `footnote_prefix` = the first 8 hex of blake3 of
`path.strip_prefix(root)` written with `/` separators, + `-` (the same for a `.ipynb`,
notebook-mapping.md §7.3), **without `resolve`**: a fragment alone cannot resolve labels and citations of the whole document
(resolve would turn them into `[?x]` placeholders, references.md §4.7, insert a `Toc` and invent auto
ids that collide across fragments); §5.6's fragment rules apply and typst resolves (a reference to
an unnumbered heading then fails as in hand-written typst). A `.ipynb` gives notebook-mapping.md
§7.3's `writer-cells` module (from `notebook::parse_cells(.., nested: true, ..)`, parts emitted the
same way). The World merges `assets`, records `files` in `accessed()`, keeps `diagnostics` for
`include_diagnostics()`, caches the result until `reset()`, and maps `Err` to
`FileError::Other(Some(message))` (typst reports it at the `#include`). A World without a provider
(T03's own tests) answers `source()` of a `.md`/`.ipynb` id with `FileError::Other(Some("including
.md/.ipynb from typst arrives in T53"))` (PLAN §5's wording, the same message as T16's stub).

**Installation (T16).** `WorldSlot::get` (live-reload.md §5.2–5.3, `src/pipeline/mod.rs`) calls
`world.set_include_provider(Box::new(MarkdownIncludes { config }))` on every call, `config` =
`prepared.config.clone()` for `Some(prepared)`, else the config it creates the World from (a kept
World keeps its provider), so `build` (every target, `--watch`), serve (web and pages) and `check`
share it. T16 adds `pub struct MarkdownIncludes { pub config: Config }` and its `IncludeProvider`
impl to `src/typst/includes.rs` as a stub (additive, like `include::scan_includes` in T1B's file)
whose `synthesize(&self, path: &Path, root: &Path)` returns `Err(Error::Other("including
.md/.ipynb from typst arrives in T53"))`;
T53 replaces only that body, so no integration file changes after T16.

> **T53 implementation clarifications (2026-10-29):** source interception
> follows the input detector's Markdown/notebook extensions, case-insensitively,
> including `.markdown`; `file()` still returns original bytes. The original
> caller-selected native main retains T52's raw-file identity.
>
> Canonical provider paths outside the canonical root, or not representable as
> UTF-8 root-relative paths, fail explicitly. A native fragment uses parent
> emission settings with TOC off, clears ignored metadata before emission, and
> reports explicit ignored document fields once at the included file's line1.
> Parser-local options remain local; ignored abstracts do not reserve imports
> or note identities. Only nonempty runs go to `emit_split`; empty retained
> entries and code variants remain selectable. Exact duplicate conversion
> diagnostics from three passes are kept once, in first-seen order.
>
> Failed provider/structured parses retain attempted paths and best-effort
> declared descendants for missing-file recovery. Successful dependency lists
> also include missing-image attempts. Refused out-of-root sources are not
> scanned. Mapped source-span files take precedence over synthesized owners
> when reporting nested compiler locations.

### 6.5 Packages and the downloader
`file()` for `VirtualRoot::Package(spec)`, first match wins: (1) `writer/doc` at `WRITER_DOC_VERSION`
(`typst/writer-doc/`) and `preview/*` at `vendor::typst_package_dir` (mitex at `MITEX_VERSION`) from
the `vendor` embed (vendored-assets.md §5.3; read from disk in debug builds); (2) each `[typst]
package_paths` root (relative to its `writer.toml`): `{root}/{namespace}/{name}/{version}/{vpath}`;
(3) `SystemPackages::from_parts(
FsPackages::system_data(), FsPackages::system_cache(), UniversePackages::new(d)).obtain(&spec)?
.load(vpath)` (data dir, cache dir, download for `preview` only) with `d` = `WriterDownloader` (`ureq`
+ rustls, user agent `writer/<version>`) under `[pdf] allow_packages`/`--allow-packages`, else
`NoDownloader` (`stream` → `Err(io::ErrorKind::NotFound.into())`). typst-kit features: `embedded-fonts`,
`scan-fonts`, `system-packages`, `universe-packages` (never `system-downloader`: no OpenSSL). Another
`writer/doc` version not on disk → `PackageError::VersionNotFound(spec, WRITER_DOC_VERSION)`.

### 6.6 Fonts
`FontStore::new()`, then `extend(typst_kit::fonts::embedded())` (Libertinus Serif, New Computer
Modern, New Computer Modern Math, DejaVu Sans Mono), then `extend(scan(dir))` for each `[pdf]
font_paths` entry, then `extend(system())` when `[pdf] system_fonts`. Embedded first makes output
independent of the machine for the package's families; `system_fonts = false` gives byte-identical
PDFs across machines (R1 §8).

## 7. Lints and the round trip (T71, T52)

`check` on a `.typ` input runs §2 and reports every §2.8 code (cli.md cites this section).
Per-target lints for documents emitted to typst (`--target pdf|typst`): `raw-skipped` (info) for raw
blocks of other formats; `unsupported-construct` (warning) for a non-`asset:` image, an image MIME
typst cannot decode, and a `Bibliography` block without sources; T33's `mitex` pre-flight
(`math-conversion-failed` when `mitex::convert_math(src, None)` errs).

**Round trip** (T52, `tests/typst_roundtrip.rs`: it needs `markdown::parse` T12, `emit` T30 and
this frontend T50): for every `tests/fixtures/md/*.md` except `presenter-deck.md`, `includes.md` and
`raw.md`, with `R(x) = resolve(x, &ResolveOptions { config: <that side's config after
merge_front_matter>, library: None, lenient: true })`: `normalize(R(md)) ==
normalize(R(typ_frontend(emit(R(md), AssetPaths::Source))))`. `normalize` (one function, applied to
both sides) compares `meta`, `blocks`, `features` and `resolved` only, and: sets every `span` to
`None`; clears `meta.vars` (config keys act through `R`'s config; of the other vars only §3's
three title vars are emitted, §5.3–5.4);
splices `Div` and `Span` children; drops `Raw` of formats other than `typst`, and `Bibliography`
blocks when `resolved.bibliography` is `None`; clears `Attrs.classes`/`pairs` except the classes
`unnumbered` and `notoc` (`unlisted` read as `notoc`) on headings, figures and display math (a
heading's `unnumbered` only while that side's heading numbering is on, §5.4), image
`width`/`height` (px and bare numbers converted to pt) and `CodeBlock` attributes re-read through
`CodeAttrs::from_attrs` (canonical `numbered`/`plain`/`start`/`highlight`; inline
`Code` follows the ordinary class/pair clearing rule, retaining its id and exact
text because line controls apply only to blocks); applies
`latex_source::prepare` to latex math sources (math.md §6); replaces each `Ref` with `form: Normal`
and `resolved.number == ""` by `Link { inlines: resolved.text, url: "#" + target, title: None,
attrs: {} }` (§5.6); drops `LabelKind::Footnote` entries of `resolved.labels` (a stripped
`writer-fn-` name is a typst label; references.md Deviation 9 (f)); turns `SoftBreak` in headings
and terms into `Text(" ")`; sets `Image.title`/`Link.title` to `None` and `Image.alt` to
`[Text(plain_text(alt))]` (empty when blank); turns task items into `checked: None` plus the
`☐ `/`☒ ` text; merges a term's definitions into one; lifts cite prefixes/suffixes out as §5.6
writes them; treats `meta.lang == None` as `config.document.lang`; clears body-cell `header` flags;
merges adjacent `Text`. Any other difference is an emitter or frontend bug.

> **Round-trip refinement (coordinator, 2026-10-29):** canonicalize only
> `features.notebook` to the presence of a surviving `Block::Cell` on each
> normalized side. A Markdown document can include a notebook containing only
> Markdown cells: its merged notebook feature records source provenance, while
> the flattened Typst has no code-cell from which to recover that provenance.
> Preserve every other feature bit. A dropped actual Cell still changes the
> normalized notebook bit and remains a regression. No citation-placeholder
> normalization is added; `R` with `library: None` already classifies unknown
> citation keys to their visible placeholders.

## 8. Compile and the PDF pipeline (T03 §8.1–8.3, T32 §8.4)

### 8.1 API (`src/typst/compile.rs`)
```rust
pub struct Compiled<T> { pub output: Option<T>, pub diagnostics: Diagnostics }
impl<T> Compiled<T> { /// The first error becomes `Error::Compile`; warnings are returned.
    pub fn into_result(self) -> Result<(T, Diagnostics)>; }
pub struct PdfOptions { pub timestamp: Datetime /* UTC */, pub ident: Option<String> }
pub struct Pdf { pub bytes: Vec<u8>, pub pages: usize }
pub struct OracleEntry { pub kind: String, pub number: Option<String> }   // references.md §7
pub fn compile_document(world: &mut WriterWorld) -> Compiled<PagedDocument>;  // pages: live-reload.md §4.1
pub fn compile_pdf(world: &mut WriterWorld, opts: &PdfOptions) -> Compiled<Pdf>;
pub fn oracle(document: &PagedDocument, label: &str) -> Option<OracleEntry>;
pub fn map_diagnostic(world: &WriterWorld, diag: &SourceDiagnostic) -> Diagnostic;
pub fn pdf_timestamp(pdf: &PdfConfig, meta: &DocMeta, source_date_epoch: Option<&str>, now: SystemTime) -> Result<Datetime>;
```
`compile_pdf` calls `typst_pdf::pdf` with `typst_pdf::PdfOptions { ident: opts.ident → Smart::Custom
or Smart::Auto, creator: Smart::Custom(Some(format!("writer {}", env!("CARGO_PKG_VERSION")))),
timestamp: Some(Timestamp::new_utc(opts.timestamp)), page_ranges: None, standards: default, tagged:
true, pretty: false }`.

### 8.2 Diagnostics
Each `SourceDiagnostic` becomes a `Diagnostic { level: Error|Warning, code: "typst-error" |
"typst-warning", message: message + "\n  hint: " per hint, path, span }`. Location: `diag.span` when it
lies in a project file; otherwise the **last** (outermost) tracepoint of `diag.trace` whose span lies
in a project file (the user's call; *probe*: `[mitex-convert@mitex.typ:26, mimath@/main.typ:4]`), with
"in <package>/<file>:<line>" added as a hint; otherwise the package location with path
`@<ns>/<name>:<ver>/<file>`. Span → line: `world.range(span)` then
`source.lines().byte_to_line(start) + 1`. `/__writer-main.typ` and synthesized include lines map
through their `SourceMap` (`SourceSpan` with the Markdown file, line and cell); unmapped lines report
the input path without a line; real files report their own path. `PackageError::NotFound` of a
`preview` package without download adds the hint "writer downloads @preview packages only with
`[pdf] allow_packages = true` or `--allow-packages`". `into_result` builds
`Error::Compile { path, line, message, hints }` from the first error.

### 8.3 Oracle
`oracle` is references.md §7's code (`query_label` of `writer-oracle-{label}`, `MetadataElem`, a
`number` of `none` → `None`).

### 8.4 PDF pipeline (T32, `src/pipeline/pdf.rs`; `.typ` inputs T52, `src/pipeline/typst_source.rs`)
Markdown/notebook (`pdf.rs`): parse → resolve → `emit` (`AssetPaths::Virtual`; bibliography, style
and template mounted, §6.3) → `set_main_text` → `compile_pdf` → `dist/<stem>.pdf` (`N pages`).
`.typ` (`typst_source.rs`, PLAN §3.2): the frontend (for `meta`, §2.1), then `MainFile::Path(input)`
→ T03's `compile_pdf` with the same timestamp policy (`pdf_timestamp`, §8.1). **Timestamp policy**
(config.md `[pdf] timestamp`): non-empty → `time::OffsetDateTime::parse(v,
&time::format_description::well_known::Rfc3339)`, `to_offset(UtcOffset::UTC)`, then
`Datetime::from_ymd_hms` (invalid → `Error::Config`); else `meta.date` matching
`^\d{4}-\d{2}-\d{2}$` → that day 00:00:00 UTC; else `SOURCE_DATE_EPOCH`; else now (whole seconds).
Computed once per session, it drives `PdfOptions.timestamp` and `World::today`; the package keeps
`document(date: auto)` under writer (§4.2), so `/CreationDate` is the policy value (*probe*: a set
document date overrides the timestamp). Fixed timestamp + `system_fonts = false` → byte-identical
PDFs (T32 test). `dump --typst` prints `emit`'s text. **Oracle test** (T32, `tests/pdf_oracle.rs`):
references.md §7 (expected kind and display-form number per label, `None` when unnumbered) over
`numbering.md` and its policy variants, `figures.md`, `tables.md`, `code.md`, `math.md`,
`references.md` and `typ/subset.typ`.

## 9. Renditions and MathML (T03)

```rust
// src/typst/render.rs
pub struct Rendition { pub svg: Vec<u8>, pub png: Vec<u8> /* 2 px/pt */, pub pdf: Option<Vec<u8>>,
                       pub width_pt: f64, pub height_pt: f64, pub baseline_pt: f64 }
pub fn equation(world: &mut WriterWorld, source: &str, dialect: MathDialect, display: bool) -> Result<Rendition>;
pub fn image(world: &mut WriterWorld, bytes: &[u8], mime: &str, width_pt: Option<f64>) -> Result<Rendition>;
// src/typst/html.rs
pub const EQUATION_CSS: &str = "/* Alignment */\n…";   // typst-html 0.15.1 EQUATION_CSS_STYLES, expanded
pub fn equation_mathml(world: &mut WriterWorld, source: &str, display: bool) -> Result<String>;
```
`equation` and `image` compile `/__render/main.typ` (main switched to it for the call) after the
preamble `#set page(width: auto, height: auto, margin: 0pt, fill: none)` + `#set text(top-edge:
"bounds", bottom-edge: "bounds")` (R4 §5.2: without it descenders are clipped). `equation` appends
`#box($src$)` / `#box($ src $)` (typst) or the mitex import and `#box(mi(..))` / `#box(mitex(..))`
(LaTeX, §5.2 literal; `source` is already `for_typst` text, math.md §3.4); `pdf: None`.
`image` stores the bytes as `/__render/image.<ext>` (§6.3 ext; other MIME → `Error::Other`) and
appends `#image("/__render/image.<ext>", width: <w>pt)` (width only when given); `pdf: Some`
(`typst_pdf::pdf`, `timestamp: None`). Page 1 → `typst_svg::svg` → `svg`; `typst_render::render(page,
&RenderOptions { pixel_per_pt: 2.0.into(), render_bleed: false }).encode_png()` → `png`; size from
`page.frame`; for `equation`, `baseline_pt` = `pos.y + g.frame.baseline()` of the first
`FrameItem::Group` whose frame `has_baseline()` (the box: without it many equations yield no group
and a height fallback lifted inline pictures by 3.9 pt, math.md §3.4), and no such group →
`Err(Error::Other("equation has no baseline"))`; for `image` it is the height. Compile errors →
`Error::Compile` (§8.2). `equation_mathml` compiles only `$src$`/`$ src $` with the HTML library,
no preamble (there `#set page(..)` warns "page set rule was ignored during HTML export", R1 §5.3),
runs `typst_html::html(&doc, &HtmlOptions { pretty: false })` and returns the first `<math
…>…</math>`; the detached "html export is under active development" warning is dropped, any other
warning ("… was ignored during MathML export") is `Err` so callers fall back per math.md.
`EQUATION_CSS` is the `<style>` typst emits for a document with an equation (R4 §7); a T03 test
compiles one equation and asserts equality.

## 10. Exports and eject (T34, T72)

`build x.md --to typst` (T34, `src/pipeline/typst.rs`) writes `<out>/<stem>.typ` with
`AssetPaths::Export { dir: "<stem>_assets" }`: each referenced asset is copied to `<stem>_assets/<name>`
(`name` = `file_name_hint` with characters outside `[A-Za-z0-9._-]` → `-`; later duplicates in asset-id
order get `-<id[..6]>` before the extension); bibliography and `.csl` files and a `[pdf] template`
(not the files it imports; the import line names its copy) are copied there too (cli.md's output
table). **Materialised includes**: first-level includes are the files named by the input's own
`:::include` directives, found by a non-recursive scan: comrak-parse the input's text as the walker
does (after `markdown::directives::colon_prepass`, with `markdown::comrak_options(&config.markdown)`;
for an `.ipynb` input, each markdown cell's text as notebook-mapping.md §2.6 does), keep the
`BlockDirective` children of the document whose `markdown::directives::parse_directive_info(info).name`
is `include`, and resolve each path as markdown-dialect.md §8 does (relative to the input's
directory), canonicalised. `include::scan_includes` is not used: it is transitive (every nested
include up to `MAX_DEPTH`) and serves the watch set (live-reload.md §6.1). A run starts at a
top-level block whose canonicalised `span.file` is a first-level include other than the current
run's and extends over following blocks with `span.file != None` (so a nested include stays in its
first-level file's run); each
run becomes `<stem>_<include stem>.typ` (duplicates `-2`, `-3`) via `emit_split` and is `#include`d
by the main file. The tree compiles with `typst compile --package-path <eject typst-package dir>`
(T34's test: a `WriterWorld` rooted at `<out>`, no mounts). `--to typst` on a `.typ` input is an error
(cli.md) suggesting `writer build part.md --to typst` for its `.md` includes. Output line as `latex`.

> **Contract clarification (coordinator, 2026-10-27):** allocate export names
> for referenced assets only, in asset-id order. If `-<id[..6]>` itself
> collides with an allocated name, try `-<id[..6]>-2`, then `-3`, etc., before
> the extension. A generated name must never overwrite another asset.
>
> **Portability clarification (T34, 2026-10-28):** compare allocated basenames
> using ASCII lowercase while preserving their chosen spelling. Trim trailing
> dots, use `asset` for an empty result, and prefix Windows device basenames
> (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, including an
> extension) with `asset-`. This changes only otherwise conflicting or
> unrepresentable Export names; Virtual and Source paths retain their rules.
> Copied resources share this occupied-name set after referenced assets, in
> bibliography order, then path CSL, then template. Repeated canonical resource
> paths reuse one copy; distinct resources use the first six content-hash
> characters and the same collision-counter rule when necessary.
>
> **Shared LaTeX namespace (T60, coordinator, 2026-10-29):** keep the
> existing Typst referenced Image/Link set and its names first. Then reserve
> LaTeX-only picked Image ids, MIME-corrected/rendition names and bibliography
> names using latex-output.md §1.2's fixed priority, even in a standalone export.
> The pure planner performs no reads. Physical Typst resources retain the
> existing canonical reuse/content-hash allocator and may claim an ordinary
> basename already reserved for the same written resource path. Auxiliary
> resource names change only when a new LaTeX reservation blocks their exact
> old destination. A raw asset and its rendition cannot share a byte owner.
>
> For notebook scanning, retained raw cells with MIME `text/markdown` follow
> the same rules as Markdown cells. Span-based runs preserve block order and
> contents: adjacent occurrences of the same file can coalesce, and a nested
> file also named by a first-level include can start a separate run. Export
> does not reconstruct occurrence ancestry from flattened source spans.

`eject typst-package DIR [--install]` (T31: `typst::package::eject(dir: &Path) -> Result<Vec<PathBuf>>`,
`install_dir() -> Result<PathBuf>`; T72 wires both eject commands in `src/cli/eject.rs`) writes
`DIR/writer/doc/0.1.0/` and `DIR/preview/mitex/0.2.7/` (vendored-assets.md §5.3's 11 files);
`--install` uses typst's data package directory (`<data dir>/typst/packages`, as
`FsPackages::system_data()`, R1 §3). An existing version
directory → exit 2, nothing written. `eject typst-template FILE` writes `template.typ` (§4.6).

## 11. Verified API hints

Cargo: vendored-assets.md §7.3 is authoritative (`=0.15.1` pins of `typst`, `typst-syntax`,
`typst-layout` for `PagedDocument`/`Page`, `typst-html`, `typst-pdf`, `typst-svg`, `typst-render`;
typst-kit with §6.5's features; no direct `comemo`, `ecow` or `typst-assets`: use `typst::comemo`,
`typst::ecow`). R1 §5.1's probe manifest is for reference only. World skeleton: R1 §5.2 (verified;
writer adds slots, mounts and packages to `source`/`file`); `Library::builder().with_inputs(d)`,
`id.root()`, `id.vpath().realize(root)?`, `typst::compile::<PagedDocument>(&world)` →
`Warned { output, warnings }`, `Downloader::stream`: R1 §9. Trace (*probe*):
```rust
fn loc(world: &dyn World, span: DiagSpan) -> Option<(FileId, usize)> {
    let id = span.id()?; let range = world.range(span)?;
    Some((id, world.source(id).ok()?.lines().byte_to_line(range.start)? + 1))
}
let outer = diag.trace.iter().rev().find_map(|t| loc(world, t.span.into())
    .filter(|(id, _)| matches!(id.root(), VirtualRoot::Project)));
```
Oracle query: references.md §7. Baseline: R4 §9. Syntax walk: R2 §9 (`Label::get()` has no `<>`,
`Ref::target()` no `@`, `Ref::supplement()` is `Option<ContentBlock>`; `Markup::exprs()` drops the
`Space` after a statement); `typst_syntax::is_ident` (§5.2's raw-language rule).

## Fixtures

| fixture | owner | contents |
|---|---|---|
| `tests/fixtures/typ/minimal.typ` | T50 | `#import "@writer/doc:0.1.0": *`, `writer-doc.with` with every §3 argument kind, one heading, paragraph, footnote |
| `tests/fixtures/typ/subset.typ` + `typ/refs.bib` | T50 | every §2.4 call and §2.2 node, labels on each target kind, refs/cites (`path("refs.bib")`: §6.1's root is `typ/`; `refs.bib` is a byte copy of `bib/refs.bib`, a T50 test asserts equal bytes), `mi`/`mitex`, lists/terms (one with a `:` in the term), tables with spans; oracle test (T32) |
| `tests/fixtures/typ/unsupported.typ` | T50 | one instance of every §2.8 code except `E-TYP-SYNTAX` |
| `tests/fixtures/typ/math.typ` | T50 | R2 §4.2's 15 equations, one with `#` code (kept verbatim, §2.2), `mat(delim: #none, …)` |
| `tests/fixtures/typ/includes.typ` + `typ/include/{part.typ, cycle-a.typ, cycle-b.typ}` | T50 (`.typ` includes only); T53 adds `part.md` (front matter `toc: true`; a T53 test asserts that neither the IR nor the synthesized typst holds a `Toc`/`#outline` for it, §2.7, §6.4), `nb.ipynb` and their `#include` lines (additive) and re-blesses the snapshot | §2.7, §6.4 |
| `tests/fixtures/typ/include-notebook.typ` + `typ/include/cells.ipynb` | T53 | notebook-mapping.md §7.3 / Fixtures |
| `tests/fixtures/typ/include-notebook-missing.typ` | T53 | selects an unknown index of `cells.ipynb`: `notebook-cell-not-found` in the HTML export, a panic naming it in the PDF (notebook-mapping.md Fixtures) |
| `tests/fixtures/typ/syntax-error.typ` | T50 | `E-TYP-SYNTAX` with line/col, empty IR |

Snapshots: `typst_frontend__<name>.snap` and `__<name>-diagnostics.snap` (T50, IR `--no-resolve`,
paths redacted); `typst_emit__<md fixture>.snap` (T32, every `tests/fixtures/md/*.md`: it needs
`markdown::parse`; T30 tests against `ir/sample-document.json` and inline `Document`s, one with
exactly one author and one keyword, §5.2's one-element arrays). T03 uses
inline sources; T31 compiles an inline copy of `minimal.typ` (T50's file lands in the same wave)
and one inline document per numbering policy, plus §4.3's caption-less figure case; T32: oracle, PDF byte identity (§8.4); T34: export
trees of `includes.md`, `figures.md`, and a BYO template's PDF and export (§4.6); T52: §7's round
trip.

## Open points (a task may decide locally)
- T03: retained slots through `typst_kit::files::FileStore<WriterLoader>` (R1 §3) or an own
  `Mutex<BTreeMap<FileId, Slot>>` (observable rules of §6.1 fixed either way).
- T30: line wrapping of long argument lists (one argument per line is fine) within §5.2–5.3.
- T31: colours, title-block spacing and callout styling within §4.2–4.3's fixed values.
- T50: `LinkedNode` offsets vs summing `len()` while walking (R2 §2.3).

## Deviations from PLAN.md and the brief
- **Section order** follows the frozen specs' citations (config.md → §3/§4, cli.md → §2/§7, ir.md →
  §6), not the task's topic order.
- **Bibliography paths** (PLAN §2.2 writes `bibliography: "refs.bib"`): typst 0.15.1 resolves a string
  inside the package that uses it (*probe*), so the package needs `path("refs.bib")`; strings are
  accepted by the frontend with `W-TYP-PATH-STRING` and fail in the PDF with a clear message.
- **Round trip** "identity modulo spans" (brief) is identity modulo spans *and* §7's documented losses
  (classes, titles, `Div`/`Span`, `meta.vars`, cite prefixes, refs to unnumbered targets as links,
  …): typst has no slot for them. An explicit figure `kind:` wins unless the body violates ir.md
  §2.2's shape for it (then `Other`). The golden moves from T50 (PLAN §5) to T52: T50 (wave 3)
  has neither `markdown::parse` (T12) nor `emit` (T30); T52 depends on both through T32. Likewise
  the emitter snapshots of the Markdown fixtures move from T30 to T32. CROSS: PLAN §5's T50 row
  drops "md→typ→IR round-trip golden", T52's row gains it.
- **Eviction**: the brief's "`comemo::evict(10)` after each compile" becomes once per build
  (live-reload.md §5.3 (c)); `compile_pages` and its `PageSvg` are gone (live-reload.md §4.1).
- **Eject wiring** is T72's (`src/cli/eject.rs`, PLAN §3.2), not T34's (PLAN §5's T34 row);
  T31 provides the functions. CROSS: PLAN §5's T34 row drops the eject wiring.
- **`[pdf] template`** (PLAN §5 gives it to T34): T32 mounts it in `load_typst_main` and sets
  `EmitOptions.template` (§4.6, §8.4), which serve's pages view relies on without waiting for T34
  (live-reload.md Fixtures); T34 copies it in the export and tests the BYO path. Dependencies are
  unchanged (T34 already depends on T32). CROSS: PLAN §5's T34 row reads "BYO typst template tests
  (`[pdf] template` mounted by T32), `build --to typst` export with materialised includes …".
- **Include provider installation** (no PLAN row names it): T16's `WorldSlot::get` installs §6.4's
  `MarkdownIncludes`, whose stub T16 adds to T53's `src/typst/includes.rs` (additive); T53 replaces
  its body. Dependencies are unchanged (T16 already depends on T03). CROSS: PLAN §3.2's
  `includes.rs: T53` gains "(`MarkdownIncludes` stub: T16)", PLAN §5's T16 row gains "§6.4 include
  provider installed by `WorldSlot::get` (stub)", and live-reload.md §5.3's World creation names it.
- **`equation_mathml`** takes `display: bool` in addition to the brief's `(world, source)`: typst's
  MathML differs between inline and display (`msubsup` vs `munderover`, R4 §7).
- **`src/typst/html.rs`** is not in PLAN §3.2; it belongs to T03 (T00 adds it to the skeleton).
- **Materialised includes for `.typ` inputs** (PLAN §2.2): cli.md (frozen) makes `--to typst` on a
  `.typ` input an error, so materialisation applies to `.md`/`.ipynb` inputs; a `.typ` author converts
  an included `.md` with `writer build part.md --to typst`.
- **R2 §7.1's `lorem(n)`** is not in the subset (`E-TYP-CALL-UNKNOWN`): its text cannot be
  reproduced without typst's word list. `math.equation(..)` is accepted only in references.md
  §2.4's unnumbered shape; `$ … $` covers the rest.
- **`E-TYP-MATH-CODE`** (R2 §7.3: every `E-*` fails HTML/DOCX/LaTeX) is math.md §9's LaTeX-only
  warning; the frontend keeps such equations (math.md Deviations).


> **T32 coordinator seams (2026-10-28):** Preserve public PDF/pipeline APIs.
> Private detailed build/loading entries retain successful emission findings
> and complete failed compiler diagnostics for target dispatch and later pages.
> The World exposes its full retained timestamp privately and can remove a
> mounted subtree plus cached disk slots without removing reserved resources.
> The PDF loader replaces only its template/bib/csl namespaces, owns no reset
> or cache eviction, and uses the same timestamp as World::today. Mapped
> diagnostic span.file wins, then compiler path, then original input.
> Page counts read the pinned exporter's plain XMP packet and are tested
> against the compiler's exact count; unsupported bytes return None.
