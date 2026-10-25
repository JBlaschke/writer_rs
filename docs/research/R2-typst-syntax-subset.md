# R2 — Parsing a structured subset of typst with `typst-syntax`; typst ⇄ LaTeX math

**Date:** 2026-10-24. **Machine:** macOS, Rust 1.99.1 (Homebrew cargo), pandoc 3.12.1, TeX Live
2026, no `typst` CLI. Everything marked **[verified]** was read in crate sources under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<ver>/`, produced by a scratch
program, or produced by a locally run command. Everything else is marked **[unverified]**.

Scratch crates (keep them; the plan's task files may reuse the probe):

- `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/typst-syntax-probe/`
  — `src/main.rs` (modes `dump <file>`, `math <file>`, `parsemath`, `errors`, `codex`, `mitex`,
  `bench <file> [n]`), `docs/sample.typ`, `docs/math.typ`, `docs/labels.typ`, `docs/raw3.typ`,
  `docs/eq/*.typ` (one equation per file, for pandoc).
- `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/conv-probe/`
  — tylax / docmux-transform-math / tex2typst-rs comparison (modes `tylax-math <file>`,
  `tylax-doc <file>`, `tylax-doc-diag <file>`, `l2t`).
- Shared target dir: `.../scratchpad/build/target` (`CARGO_TARGET_DIR`).

---

## 0. Executive summary

1. **`typst-syntax` 0.15.1 is a complete, error-tolerant, lossless (CST) parser** with a typed
   AST view. `typst_syntax::parse(&str) -> SyntaxNode` never fails; syntax errors become
   `SyntaxKind::Error` leaves and are listed by `root.errors_and_warnings()`. A typst document
   that compiles with the real compiler and uses only the constructs in §7 can be walked
   into writer's document IR with the typed accessors listed in §2.7 — no evaluator needed.
   **[verified]** by the probe program (§3).
2. **API names changed vs. older typst-syntax versions** (the fleet must use these):
   `SyntaxNode::leaf_text()` / `full_text()` (there is **no** `text()`),
   `errors_and_warnings()` (no `errors()`), `Source::detached(text)` /
   `Source::new(FileId, String)` with `FileId::new(RootedPath)` (no
   `FileId::new(Option<PackageSpec>, VirtualPath)`), math function calls are
   `SyntaxKind::MathCall` + `MathArgs` (not `FuncCall`), `Label::get()` returns the id
   without `<>`, `Ref::target()` without `@`. **[verified]** (§2).
3. **Math**: the math sub-tree is small and regular (12 math kinds, §4). Symbols are
   resolved with the **`codex` 0.3.0** crate (public API: `codex::SYM.get(name)` →
   `Binding{def: Def::Symbol(Symbol)}`; `Symbol::get(ModifierSet::from_raw_dotted("r.long"))`
   → `Option<(&'static str, Option<&str>)>`); `alpha`→α, `arrow.r`→→, `RR`→ℝ, `oo`→∞ all
   resolve **[verified]**. `dif`/`Dif` and the text operators (`lim`, `sin`, …) and
   spacings (`thin`, `quad`, …) are *not* in codex; they are defined in
   `typst-library/src/math/op.rs` and `mod.rs` (lists in §4.4).
4. **typst math → LaTeX converters**: pandoc 3.12.1's typst reader gives the best output
   (15/15 test equations correct, §5.1) but is an external process and chokes on
   `lr(( … ])` and on `@fig:x`-style refs. The Rust crate **tylax 0.3.8** (Apache-2.0) has
   a real typst-syntax-based typst→LaTeX converter (14/15 good, §5.2) but pins
   `typst-syntax = "0.14"` (a second parser copy, 0.14.2, would be compiled) and its
   markup conversion drops `#include`/`#import`/`#callout` arguments. **Recommendation:**
   write writer's own ~600-line converter over the `Math` tree (shape is trivial, §4.3),
   with codex for names→Unicode and a curated Unicode→LaTeX table (§6); use pandoc and
   tylax as *test oracles*.
5. **LaTeX → typst**: `mitex::convert_math(&str, None) -> Result<String, String>`
   **[verified]** works offline (prebuilt spec is inside the crate), but its output
   (`mitexsqrt(…)`, `pmatrix(…)`, `#textmath[…]`, `operatorname(…)`) **is not standalone
   typst** — it relies on the `mitex-scope` of the `@preview/mitex` typst package (§5.4).
   `tylax::latex_to_typst` produces standalone typst (`lr((a/b ])`, `mat(delim: "(", …)`,
   `cases(…)`), with a few wrong cases (`\textbf{bold}` → `*b o l d*`). `tex2typst-rs` is
   GPL-3.0 (avoid) and loses matrix delimiters.
6. **Unicode→LaTeX table source**: `unicode-math-table.tex` (TeX Live, LPPL 1.3c) has 2448
   `\UnicodeMathSymbol{"XXXXX}{\cmd}{\mathclass}{desc}%` lines; usable with a curated
   override for Greek (it maps U+03B1 to `\mupalpha`, not `\alpha`) and for
   unicode-math-only names (`\BbbR` → prefer `\mathbb{R}`). Details and alternatives in §6.
7. **Speed**: release parse of a 284 KB / 13 500-line file: 5.5 ms; incremental
   `Source::replace` for a mid-file edit: 0.4 ms **[verified]** (§8) — serve-mode reparse is
   free.

---

## 1. Versions and licences **[verified]** (`cargo info`, registry sources)

| crate | version | licence | MSRV | notes |
|---|---|---|---|---|
| typst-syntax | 0.15.1 | Apache-2.0 | 1.92 (edition 2024) | deps: ecow 0.2.6 (serde), rustc-hash 2.1, serde, toml 0.8 (parse,display), typst-timing 0.15.1, typst-utils 0.15.1, unicode-ident, unicode-math-class 0.1, unicode-script, unicode-segmentation, unscanny |
| codex | 0.3.0 | Apache-2.0 | 1.88 | default features `numeral-systems` (dep chinese-number), `styling`; build.rs generates tables from `src/modules/sym.txt`, `emoji.txt` (no network) |
| mitex | 0.2.4 | Apache-2.0 | unknown | deps mitex-parser 0.2.4, mitex-spec-gen 0.2.4, rowan 0.15.15 |
| mitex-spec-gen | 0.2.4 | Apache-2.0 | — | build.rs copies the prebuilt `assets/artifacts/spec/default.rkyv` (51 788 B, in the crate) unless a `typst` binary is on PATH **and** it is inside the mitex workspace; no network |
| tylax | 0.3.8 | Apache-2.0 | unknown | default feature `cli` pulls clap; **pins `typst-syntax = "0.14"`** (0.14.2 gets compiled in addition to 0.15.1); also deps mitex 0.2, mitex-parser/-spec/-spec-gen 0.2, rowan, regex, phf, chrono, indexmap, lazy_static, fxhash; 448 `#[test]`s |
| docmux-transform-math | 0.6.2 | MIT | 1.80 | deps docmux-ast, docmux-core; regex/token based |
| tex2typst-rs | 0.4.1 | **GPL-3.0-or-later** | unknown | deps phf, regex |
| typort-math | 0.2.0 | Apache-2.0 | unknown | typst→OMML on *evaluated* `Content` (deps `typst` 0.15.1, `typst-library` 0.15.1, codex 0.3, quick-xml 0.37) |
| pandoc | 3.12.1 | GPL | — | `--list-input-formats` and `--list-output-formats` both contain `typst` |
| unicode-math | 0.8r (2023-08-13) | LPPL 1.3c | — | `kpsewhich unicode-math-table.tex` → `/opt/homebrew/Cellar/texlive/20260301_1/share/texmf-dist/tex/latex/unicode-math/unicode-math-table.tex`; repo https://github.com/latex3/unicode-math (CTAN page) |

Also present in the registry (already fetched by someone): typst 0.15.1, typst-library 0.15.1,
typst-eval 0.15.1, typst-pdf 0.15.1, typst-kit 0.15.1, typst-html 0.15.1, typst-svg/render 0.15.1.

---

## 2. `typst-syntax` 0.15.1 API **[verified]** (copied from source)

### 2.1 Crate root (`src/lib.rs`)

```rust
pub mod ast;
pub mod package;                      // PackageSpec, PackageVersion, ...
pub use self::highlight::{Tag, highlight, highlight_html};
pub use self::kind::SyntaxKind;
pub use self::lexer::{is_id_continue, is_id_start, is_ident, is_newline,
                      is_valid_label_literal_id, link_prefix, split_newlines};
pub use self::lines::Lines;
pub use self::node::{Diagnosis, LinkedChildren, LinkedNode, Side, SyntaxDiagnostic, SyntaxNode};
pub use self::parser::{parse, parse_code, parse_math};
pub use self::path::{FileId, PathError, RealizeError, RootedPath, VirtualPath, VirtualRoot, VirtualizeError};
pub use self::source::Source;
pub use self::span::{DiagSpan, DiagSpanKind, RangeMapper, Span, SpanKind, SpanNumber, Spanned, SubRange};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SyntaxMode { Markup, Math, Code }
```

### 2.2 Parsing entry points (`src/parser.rs`)

```rust
/// Parses a source file as top-level markup.   root.kind() == SyntaxKind::Markup
pub fn parse(text: &str) -> SyntaxNode
/// Parses top-level code.                      root.kind() == SyntaxKind::Code
pub fn parse_code(text: &str) -> SyntaxNode
/// Parses top-level math.                      root.kind() == SyntaxKind::Math
pub fn parse_math(text: &str) -> SyntaxNode
```

`parse_math("x^2 + frac(1, 2)")` gives exactly the same `Math` subtree as the body of
`$ x^2 + frac(1, 2) $` (verified, §4.3). This lets the Markdown backend reuse one math
converter for `$…$` content written in typst syntax. Nesting depth is capped at 256
(`MAX_DEPTH`); deeper input yields error nodes, not a panic.

### 2.3 `SyntaxNode` (`src/node.rs`)

```rust
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct SyntaxNode { /* data: Node, span: Span */ }

impl SyntaxNode {
    pub fn leaf(kind: SyntaxKind, text: impl Into<EcoString>) -> Self;
    pub fn inner(kind: SyntaxKind, children: Vec<SyntaxNode>) -> Self;
    pub fn error(message: impl Into<EcoString>, text: impl Into<EcoString>) -> Self;
    pub fn warn(&mut self, message: impl Into<EcoString>);
    pub fn warn_at(&mut self, range: Range<usize>, message: impl Into<EcoString>);
    pub fn hint(&mut self, hint: impl Into<EcoString>);
    pub fn hint_at(&mut self, range: Range<usize>, hint: impl Into<EcoString>);
    pub fn with_hints(self, new_hints: impl IntoIterator<Item = EcoString>) -> Self;
    pub const fn placeholder(kind: SyntaxKind) -> Self;
    pub fn kind(&self) -> SyntaxKind;
    pub fn is_empty(&self) -> bool;
    pub fn len(&self) -> usize;                       // byte length in the source
    pub fn span(&self) -> Span;
    pub fn leaf_text(&self) -> &EcoString;            // "" for inner nodes
    pub fn full_text(&self) -> EcoString;             // concatenates leaves (allocates)
    pub fn children(&self) -> std::slice::Iter<'_, SyntaxNode>;
    pub fn diagnosis(&self) -> Diagnosis;             // { pub errors: bool, pub warnings: bool }
    pub fn errors_and_warnings(&self) -> (Vec<SyntaxDiagnostic>, Vec<SyntaxDiagnostic>);
    pub fn synthesize(&mut self, span: Span);
    pub fn synthesize_mapped(&mut self, id: FileId, mapper: &RangeMapper) -> Result<(), EcoString>;
    pub fn spanless_eq(&self, other: &Self) -> bool;
    // from ast.rs:
    pub fn is<'a, T: AstNode<'a>>(&'a self) -> bool;
    pub fn cast<'a, T: AstNode<'a>>(&'a self) -> Option<T>;
}
// Also: Default, Debug (tree-ish), Display is NOT implemented.

pub struct SyntaxDiagnostic {
    pub is_error: bool,
    pub span: DiagSpan,
    pub message: EcoString,
    pub hints: EcoVec<Spanned<EcoString, DiagSpan>>,
}
```

Important properties:

- The CST is **lossless**: in-order concatenation of `leaf_text()` reproduces the file.
  Whitespace is `Space` (≤1 newline) / `Parbreak` (≥1 blank line); comments are
  `LineComment` / `BlockComment` leaves.
- A node carries **no byte offset**; offsets come from `LinkedNode::offset()/range()` or by
  summing `len()` of preceding siblings while walking. Spans are opaque numbers
  (`Span::get() -> SpanKind { Detached, Number{id, num}, Range{id, range} }`); the byte range
  of a numbered span is recovered with `Source::range(num, sub_range)` or
  `Source::find(span).map(|n| n.range())`.
- `Error` nodes are leaves (kind `SyntaxKind::Error`) whose `leaf_text()` is the offending
  text (may be empty); `cast::<Expr>()` on them returns `None`.

### 2.4 `Source`, `Lines`, `FileId`, `Span` (`src/source.rs`, `lines.rs`, `path.rs`, `span.rs`)

```rust
#[derive(Clone, Hash)] pub struct Source(Arc<LazyHash<SourceInner>>);   // cheap to clone
impl Source {
    pub fn new(id: FileId, text: String) -> Self;          // parses + numberizes spans
    pub fn detached(text: impl Into<String>) -> Self;      // id = project:/main.typ
    pub fn with_root(id: FileId, text: String, root: SyntaxNode) -> Self;
    pub fn root(&self) -> &SyntaxNode;
    pub fn id(&self) -> FileId;
    pub fn text(&self) -> &str;
    pub fn lines(&self) -> &Lines<String>;
    pub fn replace(&mut self, new: &str) -> Range<usize>;  // diff + incremental reparse; returns reparsed range
    pub fn edit(&mut self, replace: Range<usize>, with: &str) -> Range<usize>;
    pub fn find(&self, span: Span) -> Option<LinkedNode<'_>>;
    pub fn range(&self, num: SpanNumber, sub_range: Option<SubRange>) -> Option<Range<usize>>;
}

impl<T: AsRef<str>> Lines<T> {                              // ALL indices 0-based
    pub fn new(text: T) -> Self;
    pub fn text(&self) -> &str;
    pub fn len_bytes(&self) -> usize;  pub fn len_utf16(&self) -> usize;  pub fn len_lines(&self) -> usize;
    pub fn byte_to_utf16(&self, byte_idx: usize) -> Option<usize>;
    pub fn byte_to_line(&self, byte_idx: usize) -> Option<usize>;
    pub fn byte_to_column(&self, byte_idx: usize) -> Option<usize>;   // chars before byte_idx on its line
    pub fn byte_to_line_column(&self, byte_idx: usize) -> Option<(usize, usize)>;
    pub fn utf16_to_byte(&self, utf16_idx: usize) -> Option<usize>;
    pub fn line_to_byte(&self, line_idx: usize) -> Option<usize>;
    pub fn line_to_range(&self, line_idx: usize) -> Option<Range<usize>>;
    pub fn line_column_to_byte(&self, line_idx: usize, column_idx: usize) -> Option<usize>;
}

pub enum VirtualRoot { Project, Package(PackageSpec) }
impl RootedPath { pub fn new(root: VirtualRoot, vpath: VirtualPath) -> Self; pub fn intern(self) -> FileId;
                  pub fn root(&self) -> &VirtualRoot; pub fn vpath(&self) -> &VirtualPath; }
#[derive(Copy, Clone, Eq, PartialEq, Hash)] pub struct FileId(NonZeroU16);   // globally interned, Deref<Target = RootedPath>
impl FileId { pub fn new(path: RootedPath) -> Self; pub fn unique(path: RootedPath) -> Self; pub fn get(&self) -> &'static RootedPath; }
impl VirtualPath {
    pub fn new(path: impl AsRef<str>) -> Result<Self, PathError>;              // "/chapters/a.typ" or "chapters/a.typ"
    pub fn virtualize(root_path: &Path, path: &Path) -> Result<Self, VirtualizeError>;
    pub fn realize(&self, root: &Path) -> Result<PathBuf, RealizeError>;
    pub fn get_with_slash(&self) -> &str;  pub fn get_without_slash(&self) -> &str;
    pub fn file_name(&self) -> Option<&str>; pub fn file_stem(&self) -> Option<&str>; pub fn extension(&self) -> Option<&str>;
    pub fn parent(&self) -> Option<Self>;  pub fn join(&self, path: &str) -> Result<Self, PathError>;
    pub fn relative_from(&self, base: &Self) -> EcoString;
    // deprecated: within_root, resolve, as_rootless_path, as_rooted_path
}

impl Span { pub const fn detached() -> Self; pub const fn is_detached(self) -> bool;
            pub const fn id(self) -> Option<FileId>; pub const fn get(self) -> SpanKind;
            pub const fn into_raw(self) -> NonZeroU64; pub const fn from_raw(v: NonZeroU64) -> Self;
            pub fn or(self, other: Self) -> Self; pub fn find(iter: impl IntoIterator<Item = Self>) -> Self; }
pub enum SpanKind     { Detached, Number { id: FileId, num: SpanNumber }, Range { id: FileId, range: Range<usize> } }
pub enum DiagSpanKind { Detached, Number { id: FileId, num: SpanNumber, sub_range: Option<SubRange> }, Range { id: FileId, range: Range<usize> } }
impl DiagSpan { pub fn from_range(id: FileId, range: Range<usize>) -> Self; pub fn from_span(span: Span, sub_range: Option<SubRange>) -> Self;
                pub fn id(self) -> Option<FileId>; pub fn get(self) -> DiagSpanKind; }
```

Probe result: `Source::detached(sample)`, byte 119 (`<sec:intro>`) → `byte_to_line = Some(6)`,
`byte_to_column = Some(15)`, `len_lines = 46`; heading span id = `/main.typ`,
`source.find(span).range() = 104..118`. Error positions via
`DiagSpanKind::Number{num, sub_range, ..}` → `source.range(num, sub_range)` worked (§3.5).

### 2.5 `LinkedNode` (`src/node.rs`) — parent/sibling/offset navigation

```rust
#[derive(Clone)] pub struct LinkedNode<'a> { /* node, parent: Option<Rc<Self>>, index, offset */ }
impl<'a> LinkedNode<'a> {
    pub fn new(root: &'a SyntaxNode) -> Self;
    pub fn get(&self) -> &'a SyntaxNode;  pub fn index(&self) -> usize;
    pub fn offset(&self) -> usize;        pub fn range(&self) -> Range<usize>;
    pub fn children(&self) -> LinkedChildren<'a>;          // DoubleEndedIterator<Item = LinkedNode<'a>>
    pub fn find(&self, span: Span) -> Option<Self>;
    pub fn mode_after(&self) -> Option<SyntaxMode>;  pub fn parent_mode(&self) -> Option<SyntaxMode>;
    pub fn parent(&self) -> Option<&Self>;
    pub fn prev_sibling(&self) -> Option<Self>;  pub fn next_sibling(&self) -> Option<Self>;   // skip trivia
    pub fn prev_sibling_with_trivia(&self) -> Option<Self>;  pub fn next_sibling_with_trivia(&self) -> Option<Self>;
    pub fn parent_kind(&self) -> Option<SyntaxKind>; pub fn prev_sibling_kind(&self) -> Option<SyntaxKind>; pub fn next_sibling_kind(&self) -> Option<SyntaxKind>;
    pub fn prev_leaf(&self) -> Option<Self>; pub fn leftmost_leaf(&self) -> Option<Self>;
    pub fn leaf_at(&self, cursor: usize, side: Side) -> Option<Self>;   // enum Side { Before, After }
    pub fn rightmost_leaf(&self) -> Option<Self>; pub fn next_leaf(&self) -> Option<Self>;
}
impl Deref for LinkedNode<'_> { type Target = SyntaxNode; }
```

"Trivia" = `Shebang | LineComment | BlockComment | Space | Parbreak` (`SyntaxKind::is_trivia`).

### 2.6 Full `SyntaxKind` list (`src/kind.rs`, `#[repr(u8)]`, in declaration order)

```
End, Error, Shebang, LineComment, BlockComment,
Markup, Text, Space, Linebreak, Parbreak, Escape, Shorthand, SmartQuote, Strong, Emph,
Raw, RawLang, RawDelim, RawTrimmed, Link, Label, Ref, RefMarker, Heading, HeadingMarker,
ListItem, ListMarker, EnumItem, EnumMarker, TermItem, TermMarker,
Equation, Math, MathText, MathIdent, MathFieldAccess, MathShorthand, MathAlignPoint, MathCall,
MathArgs, MathDelimited, MathAttach, MathPrimes, MathFrac, MathRoot,
Hash, LeftBrace, RightBrace, LeftBracket, RightBracket, LeftParen, RightParen, Comma, Semicolon,
Colon, Star, Underscore, Dollar, Plus, Minus, Slash, Hat, Dot, Eq, EqEq, ExclEq, Lt, LtEq, Gt,
GtEq, PlusEq, HyphEq, StarEq, SlashEq, Dots, Arrow, Root, Bang,
Not, And, Or, None, Auto, Let, Set, Show, Context, If, Else, For, In, While, Break, Continue,
Return, Import, Include, As,
Code, Ident, Bool, Int, Float, Numeric, Str, CodeBlock, ContentBlock, Parenthesized, Array,
Dict, Named, Keyed, Unary, Binary, FieldAccess, FuncCall, Args, Spread, Closure, Params,
LetBinding, SetRule, ShowRule, Contextual, Conditional, WhileLoop, ForLoop, ModuleImport,
ImportItems, ImportItemPath, RenamedImportItem, ModuleInclude, LoopBreak, LoopContinue,
FuncReturn, Destructuring, DestructAssignment
```

Helpers: `is_grouping()`, `is_terminator()`, `is_block()`, `is_stmt()` (LetBinding | SetRule |
ShowRule | ModuleImport | ModuleInclude), `is_keyword()`, `is_trivia()`, `const fn is_error()`,
`name() -> &'static str` (human readable, e.g. "heading").

### 2.7 The `ast` module (`src/ast.rs`) — typed view

```rust
pub trait AstNode<'a>: Sized {
    fn from_untyped(node: &'a SyntaxNode) -> Option<Self>;
    fn to_untyped(self) -> &'a SyntaxNode;
    fn span(self) -> Span { self.to_untyped().span() }
    fn placeholder() -> Self;          // never panics on malformed trees; accessors return placeholders
}
// every node type is `#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)] pub struct X<'a>(&'a SyntaxNode);`
```

`Expr<'a>` enum variants (one per expression kind): `Text, Space, Linebreak, Parbreak, Escape,
Shorthand, SmartQuote, Strong, Emph, Raw, Link, Label, Ref, Heading, ListItem, EnumItem,
TermItem, Equation, Math, MathText, MathIdent, MathFieldAccess, MathShorthand, MathAlignPoint,
MathCall, MathDelimited, MathAttach, MathPrimes, MathFrac, MathRoot, Ident, None, Auto, Bool,
Int, Float, Numeric, Str, CodeBlock, ContentBlock, Parenthesized, Array, Dict, Unary, Binary,
FieldAccess, FuncCall, Closure, LetBinding, DestructAssignment, SetRule, ShowRule, Contextual,
Conditional, WhileLoop, ForLoop, ModuleImport, ModuleInclude, LoopBreak, LoopContinue,
FuncReturn`. Methods: `Expr::hash(self) -> bool` (can follow `#` in markup),
`Expr::is_literal(self) -> bool`.

**Gotcha:** `Expr::from_untyped` returns `None` for `SyntaxKind::Space`; only
`Markup::exprs()` and `Math::exprs()` (which use the private `cast_with_space`) yield
`Expr::Space`. `node.cast::<ast::Expr>()` on a Space node is `None`. Also `Markup::exprs()`
**drops the `Space` that directly follows a statement** (`#let/#set/#show/#import/#include`).

Typed accessors (exact signatures; all take `self` by value, nodes are `Copy`):

| node | methods |
|---|---|
| `LineComment`, `BlockComment` | `text(&self) -> &'a str` (without `//`, `/* */`) |
| `Markup` | `exprs(self) -> impl DoubleEndedIterator<Item = Expr<'a>>` |
| `Text` | `get(self) -> &'a EcoString` |
| `Space`, `Linebreak`, `Parbreak`, `RawDelim`, `MathAlignPoint`, `None`, `Auto`, `Underscore`, `LoopBreak`, `LoopContinue` | (no methods) |
| `Escape` | `get(self) -> char` (`\#`→`#`, `\u{1F5FA}`→char) |
| `Shorthand` | `const LIST: &[(&str, char)]` = `("...",'…'),("~",'\u{A0}'),("-",'\u{2212}' /*only before a digit*/),("--",'\u{2013}'),("---",'\u{2014}'),("-?",'\u{AD}')`; `get(self) -> char` |
| `SmartQuote` | `double(self) -> bool` |
| `Strong`, `Emph` | `body(self) -> Markup<'a>` |
| `Raw` | `lines(self) -> impl DoubleEndedIterator<Item = Text<'a>>` (dedented lines), `lang(self) -> Option<RawLang<'a>>` (only for 3+ backticks), `block(self) -> bool` (3+ backticks **and** contains a newline) |
| `RawLang` | `get(self) -> &'a EcoString` |
| `Link` | `get(self) -> &'a EcoString` (the URL; auto-links only: `https://…`, `http://…`) |
| `Label` | `get(self) -> &'a str` (**without** `<` `>`) |
| `Ref` | `target(self) -> &'a str` (**without** `@`; never empty), `supplement(self) -> Option<ContentBlock<'a>>` |
| `Heading` | `body(self) -> Markup<'a>`, `depth(self) -> NonZeroUsize` (number of `=`) |
| `ListItem` | `body(self) -> Markup<'a>` |
| `EnumItem` | `number(self) -> Option<u64>` (`Some(3)` for `3.`, `None` for `+`), `body(self) -> Markup<'a>` |
| `TermItem` | `term(self) -> Markup<'a>`, `description(self) -> Markup<'a>` |
| `Equation` | `body(self) -> Math<'a>`, `block(self) -> bool` (true iff space after opening `$` and before closing `$`) |
| `Math` | `exprs(self) -> impl DoubleEndedIterator<Item = Expr<'a>>`, `was_deparenthesized(self) -> bool` |
| `MathText` | `get(self) -> MathTextKind<'a>`; `pub enum MathTextKind<'a> { Grapheme(&'a EcoString), Number(&'a EcoString) }` |
| `MathIdent` | `get(self) -> &'a EcoString`, `as_str(self) -> &'a str`, `Deref<Target = str>` |
| `MathFieldAccess` | `target(self) -> MathAccess<'a>`, `field(self) -> MathIdent<'a>`; `pub enum MathAccess<'a> { MathIdent(MathIdent<'a>), MathFieldAccess(MathFieldAccess<'a>) }` (implements `AstNode`) |
| `MathShorthand` | `const LIST: &[(&str, char)]` (38 entries, see §4.1), `get(self) -> char` |
| `MathCall` | `callee(self) -> MathAccess<'a>`, `args(self) -> MathArgs<'a>` |
| `MathArgs` | `arg_items(self) -> impl Iterator<Item = MathArg<'a>>`, `content_items(self) -> impl Iterator<Item = MathArgItem<'a>>`; `pub struct MathArg<'a> { pub arg: Arg<'a>, pub ends_in_semicolon: bool }`; `pub enum MathArgItem<'a> { Arg(Arg<'a>), Space(Space<'a>), Comma(char, &'a SyntaxNode), Semicolon(char, &'a SyntaxNode), LeftParen(char, &'a SyntaxNode), RightParen(char, &'a SyntaxNode) }` |
| `MathDelimited` | `open(self) -> Expr<'a>`, `body(self) -> Math<'a>`, `close(self) -> Expr<'a>` |
| `MathAttach` | `base(self) -> Expr<'a>`, `bottom(self) -> Option<Expr<'a>>`, `top(self) -> Option<Expr<'a>>`, `primes(self) -> Option<MathPrimes<'a>>` |
| `MathPrimes` | `count(self) -> usize` |
| `MathFrac` | `num(self) -> Expr<'a>`, `denom(self) -> Expr<'a>` |
| `MathRoot` | `index(self) -> Option<u8>` (`√`→None, `∛`→3, `∜`→4), `radicand(self) -> Expr<'a>` |
| `Ident` | `get(self) -> &'a EcoString`, `as_str(self) -> &'a str`, `Deref<Target = str>` |
| `Bool` | `get(self) -> bool` |
| `Int` | `get(self) -> i64` (handles `0x`, `0o`, `0b`) |
| `Float` | `get(self) -> f64` |
| `Numeric` | `get(self) -> (f64, Unit)`; `pub enum Unit { Pt, Mm, Cm, In, Rad, Deg, Em, Fr, Percent }` |
| `Str` | `get(self) -> EcoString` (unquoted, escapes resolved) |
| `CodeBlock` | `body(self) -> Code<'a>`; `Code::exprs(self) -> impl DoubleEndedIterator<Item = Expr<'a>>` |
| `ContentBlock` | `body(self) -> Markup<'a>` |
| `Parenthesized` | `expr(self) -> Expr<'a>`, `pattern(self) -> Pattern<'a>` |
| `Array` | `items(self) -> impl DoubleEndedIterator<Item = ArrayItem<'a>>`; `pub enum ArrayItem<'a> { Pos(Expr<'a>), Spread(Spread<'a>) }` |
| `Dict` | `items(self) -> impl DoubleEndedIterator<Item = DictItem<'a>>`; `pub enum DictItem<'a> { Named(Named<'a>), Keyed(Keyed<'a>), Spread(Spread<'a>) }` |
| `Named` | `name(self) -> Ident<'a>`, `expr(self) -> Expr<'a>`, `pattern(self) -> Pattern<'a>` |
| `Keyed` | `key(self) -> Expr<'a>`, `expr(self) -> Expr<'a>` |
| `Spread` | `expr(self) -> Expr<'a>`, `sink_ident(self) -> Option<Ident<'a>>`, `sink_expr(self) -> Option<Expr<'a>>` |
| `Unary` | `op(self) -> UnOp` (`Pos, Neg, Not`), `expr(self) -> Expr<'a>` |
| `Binary` | `op(self) -> BinOp` (`Add, Sub, Mul, Div, And, Or, Eq, Neq, Lt, Leq, Gt, Geq, Assign, In, NotIn, AddAssign, SubAssign, MulAssign, DivAssign`), `lhs(self) -> Expr<'a>`, `rhs(self) -> Expr<'a>` |
| `FieldAccess` | `target(self) -> Expr<'a>`, `field(self) -> Ident<'a>` (e.g. `table.header` → target `Ident("table")`, field `header`) |
| `FuncCall` | `callee(self) -> Expr<'a>`, `args(self) -> Args<'a>` |
| `Args` | `items(self) -> impl DoubleEndedIterator<Item = Arg<'a>>`, `trailing_comma(self) -> bool`; `pub enum Arg<'a> { Pos(Expr<'a>), Named(Named<'a>), Spread(Spread<'a>) }` — trailing `[content]` blocks are `Arg::Pos(Expr::ContentBlock)` **inside** `Args` |
| `Closure` | `name(self) -> Option<Ident<'a>>`, `params(self) -> Params<'a>`, `body(self) -> Expr<'a>`; `Params::children(self) -> impl DoubleEndedIterator<Item = Param<'a>>`; `pub enum Param<'a> { Pos(Pattern<'a>), Named(Named<'a>), Spread(Spread<'a>) }` |
| `Pattern` | `pub enum Pattern<'a> { Normal(Expr<'a>), Placeholder(Underscore<'a>), Parenthesized(Parenthesized<'a>), Destructuring(Destructuring<'a>) }`, `bindings(self) -> Vec<Ident<'a>>` |
| `LetBinding` | `kind(self) -> LetBindingKind<'a>`; `pub enum LetBindingKind<'a> { Normal(Pattern<'a>), Closure(Ident<'a>) }` (+ `bindings()`), `init(self) -> Option<Expr<'a>>` |
| `DestructAssignment` | `pattern(self) -> Pattern<'a>`, `value(self) -> Expr<'a>` |
| `SetRule` | `target(self) -> Expr<'a>`, `args(self) -> Args<'a>`, `condition(self) -> Option<Expr<'a>>` |
| `ShowRule` | `selector(self) -> Option<Expr<'a>>` (None for `show: …`), `transform(self) -> Expr<'a>` |
| `Contextual` | `body(self) -> Expr<'a>` |
| `Conditional` | `condition(self)`, `if_body(self) -> Expr<'a>`, `else_body(self) -> Option<Expr<'a>>` |
| `WhileLoop` | `condition(self)`, `body(self)` |
| `ForLoop` | `pattern(self) -> Pattern<'a>`, `iterable(self) -> Expr<'a>`, `body(self) -> Expr<'a>` |
| `ModuleImport` | `source(self) -> Expr<'a>`, `imports(self) -> Option<Imports<'a>>` (`pub enum Imports<'a> { Wildcard, Items(ImportItems<'a>) }`), `bare_name(self) -> Result<EcoString, BareImportError>`, `new_name(self) -> Option<Ident<'a>>` |
| `ImportItems` | `iter(self) -> impl DoubleEndedIterator<Item = ImportItem<'a>>`; `pub enum ImportItem<'a> { Simple(ImportItemPath<'a>), Renamed(RenamedImportItem<'a>) }` with `path()`, `original_name() -> Ident`, `bound_name() -> Ident` |
| `ModuleInclude` | `source(self) -> Expr<'a>` (usually `Expr::Str`) |
| `FuncReturn` | `body(self) -> Option<Expr<'a>>` |

---

## 3. Real tree shapes (probe output) **[verified]**

### 3.1 Sample document `docs/sample.typ`

```typst
#set heading(numbering: "1.")
#show: doc => doc
#let x = 1
#include "x.typ"
#import "lib.typ": callout

= Introduction <sec:intro>

A paragraph with *strong*, _emph_, a link https://example.com and #link("https://typst.app")[typst], a reference @fig:x and @sec:intro[Section], a footnote#footnote[Note text.] and a citation @knuth1984 #cite(<knuth1984>). Inline math $x^2$ and `raw code`.

#figure(
  image("plot.png", width: 80%),
  caption: [A plot of $y = x^2$.],
) <fig:x>

#figure(
  table(
    columns: 3,
    table.header([A], [B], [C]),
    [a], [b], [c],
  ),
  caption: [A table.],
) <tab:x>

```python
print("hi")
```

$ frac(1, 2) + sum_(i=0)^n x_i + a^2 + sqrt(x) + mat(1, 2; 3, 4) + cases(1 "if" x, 0 "else") + "text" + alpha + arrow.r + x' + abs(x) + lr(( a/b ]) $ <eq:main>

#callout(kind: "note")[
  This is a note with *strong*.
]

- bullet one
- bullet two
  - nested

+ first
+ second

/ Term: description

Line one \
line two.
```

CST dump (`kind "leaf text"` / `kind (n children)`; 0 errors, 0 warnings):

```
Markup (93 children)
  Hash "#"
  SetRule (4 children)
    Set "set"
    Space " "
    Ident "heading"
    Args (3 children)
      LeftParen "("
      Named (4 children)
        Ident "numbering"
        Colon ":"
        Space " "
        Str "\"1.\""
      RightParen ")"
  Space "\n"
  Hash "#"
  ShowRule (4 children)
    Show "show"
    Colon ":"
    Space " "
    Closure (5 children)
      Params (1 children)
        Ident "doc"
      Space " "
      Arrow "=>"
      Space " "
      Ident "doc"
  Space "\n"
  Hash "#"
  LetBinding (7 children)
    Let "let"
    Space " "
    Ident "x"
    Space " "
    Eq "="
    Space " "
    Int "1"
  Space "\n"
  Hash "#"
  ModuleInclude (3 children)
    Include "include"
    Space " "
    Str "\"x.typ\""
  Space "\n"
  Hash "#"
  ModuleImport (6 children)
    Import "import"
    Space " "
    Str "\"lib.typ\""
    Colon ":"
    Space " "
    ImportItems (1 children)
      ImportItemPath (1 children)
        Ident "callout"
  Parbreak "\n\n"
  Heading (3 children)
    HeadingMarker "="
    Space " "
    Markup (1 children)
      Text "Introduction"
  Space " "
  Label "<sec:intro>"
  Parbreak "\n\n"
  Text "A paragraph with"
  Space " "
  Strong (3 children)
    Star "*"
    Markup (1 children)
      Text "strong"
    Star "*"
  Text ","
  Space " "
  Emph (3 children)
    Underscore "_"
    Markup (1 children)
      Text "emph"
    Underscore "_"
  Text ", a link "
  Link "https://example.com"
  Space " "
  Text "and"
  Space " "
  Hash "#"
  FuncCall (2 children)
    Ident "link"
    Args (4 children)
      LeftParen "("
      Str "\"https://typst.app\""
      RightParen ")"
      ContentBlock (3 children)
        LeftBracket "["
        Markup (1 children)
          Text "typst"
        RightBracket "]"
  Text ", a reference"
  Space " "
  Ref (1 children)
    RefMarker "@fig:x"
  Space " "
  Text "and"
  Space " "
  Ref (2 children)
    RefMarker "@sec:intro"
    ContentBlock (3 children)
      LeftBracket "["
      Markup (1 children)
        Text "Section"
      RightBracket "]"
  Text ", a footnote"
  Hash "#"
  FuncCall (2 children)
    Ident "footnote"
    Args (1 children)
      ContentBlock (3 children)
        LeftBracket "["
        Markup (1 children)
          Text "Note text."
        RightBracket "]"
  Space " "
  Text "and a citation"
  Space " "
  Ref (1 children)
    RefMarker "@knuth1984"
  Space " "
  Hash "#"
  FuncCall (2 children)
    Ident "cite"
    Args (3 children)
      LeftParen "("
      Label "<knuth1984>"
      RightParen ")"
  Text ". Inline math"
  Space " "
  Equation (3 children)
    Dollar "$"
    Math (1 children)
      MathAttach (3 children)
        MathText "x"
        Hat "^"
        MathText "2"
    Dollar "$"
  Space " "
  Text "and"
  Space " "
  Raw (3 children)
    RawDelim "`"
    Text "raw code"
    RawDelim "`"
  Text "."
  Parbreak "\n\n"
  Hash "#"
  FuncCall (2 children)
    Ident "figure"
    Args (9 children)
      LeftParen "("
      Space "\n  "
      FuncCall (2 children)
        Ident "image"
        Args (6 children)
          LeftParen "("
          Str "\"plot.png\""
          Comma ","
          Space " "
          Named (4 children)
            Ident "width"
            Colon ":"
            Space " "
            Numeric "80%"
          RightParen ")"
      Comma ","
      Space "\n  "
      Named (4 children)
        Ident "caption"
        Colon ":"
        Space " "
        ContentBlock (3 children)
          LeftBracket "["
          Markup (4 children)
            Text "A plot of"
            Space " "
            Equation (3 children)
              Dollar "$"
              Math (5 children)
                MathText "y"
                Space " "
                MathText "="
                Space " "
                MathAttach (3 children)
                  MathText "x"
                  Hat "^"
                  MathText "2"
              Dollar "$"
            Text "."
          RightBracket "]"
      Comma ","
      Space "\n"
      RightParen ")"
  Space " "
  Label "<fig:x>"
  Parbreak "\n\n"
  Hash "#"
  FuncCall (2 children)
    Ident "figure"
    Args (9 children)
      LeftParen "("
      Space "\n  "
      FuncCall (2 children)
        Ident "table"
        Args (18 children)
          LeftParen "("
          Space "\n    "
          Named (4 children)
            Ident "columns"
            Colon ":"
            Space " "
            Int "3"
          Comma ","
          Space "\n    "
          FuncCall (2 children)
            FieldAccess (3 children)
              Ident "table"
              Dot "."
              Ident "header"
            Args (9 children)
              LeftParen "("
              ContentBlock (3 children)
                LeftBracket "["
                Markup (1 children)
                  Text "A"
                RightBracket "]"
              Comma ","
              Space " "
              ContentBlock (3 children)
                LeftBracket "["
                Markup (1 children)
                  Text "B"
                RightBracket "]"
              Comma ","
              Space " "
              ContentBlock (3 children)
                LeftBracket "["
                Markup (1 children)
                  Text "C"
                RightBracket "]"
              RightParen ")"
          Comma ","
          Space "\n    "
          ContentBlock (3 children)
            LeftBracket "["
            Markup (1 children)
              Text "a"
            RightBracket "]"
          Comma ","
          Space " "
          ContentBlock (3 children)
            LeftBracket "["
            Markup (1 children)
              Text "b"
            RightBracket "]"
          Comma ","
          Space " "
          ContentBlock (3 children)
            LeftBracket "["
            Markup (1 children)
              Text "c"
            RightBracket "]"
          Comma ","
          Space "\n  "
          RightParen ")"
      Comma ","
      Space "\n  "
      Named (4 children)
        Ident "caption"
        Colon ":"
        Space " "
        ContentBlock (3 children)
          LeftBracket "["
          Markup (1 children)
            Text "A table."
          RightBracket "]"
      Comma ","
      Space "\n"
      RightParen ")"
  Space " "
  Label "<tab:x>"
  Parbreak "\n\n"
  Raw (6 children)
    RawDelim "```"
    RawLang "python"
    RawTrimmed "\n"
    Text "print(\"hi\")"
    RawTrimmed "\n"
    RawDelim "```"
  Parbreak "\n\n"
  Equation (5 children)
    Dollar "$"
    Space " "
    Math (47 children)
      MathCall (2 children)
        MathIdent "frac"
        MathArgs (6 children)
          LeftParen "("
          MathText "1"
          Comma ","
          Space " "
          MathText "2"
          RightParen ")"
      Space " "
      MathText "+"
      Space " "
      MathAttach (5 children)
        MathIdent "sum"
        Underscore "_"
        Math (3 children)
          LeftParen "("
          Math (3 children)
            MathText "i"
            MathText "="
            MathText "0"
          RightParen ")"
        Hat "^"
        MathText "n"
      Space " "
      MathAttach (3 children)
        MathText "x"
        Underscore "_"
        MathText "i"
      Space " "
      MathText "+"
      Space " "
      MathAttach (3 children)
        MathText "a"
        Hat "^"
        MathText "2"
      Space " "
      MathText "+"
      Space " "
      MathCall (2 children)
        MathIdent "sqrt"
        MathArgs (3 children)
          LeftParen "("
          MathText "x"
          RightParen ")"
      Space " "
      MathText "+"
      Space " "
      MathCall (2 children)
        MathIdent "mat"
        MathArgs (12 children)
          LeftParen "("
          MathText "1"
          Comma ","
          Space " "
          MathText "2"
          Semicolon ";"
          Space " "
          MathText "3"
          Comma ","
          Space " "
          MathText "4"
          RightParen ")"
      Space " "
      MathText "+"
      Space " "
      MathCall (2 children)
        MathIdent "cases"
        MathArgs (6 children)
          LeftParen "("
          Math (5 children)
            MathText "1"
            Space " "
            Str "\"if\""
            Space " "
            MathText "x"
          Comma ","
          Space " "
          Math (3 children)
            MathText "0"
            Space " "
            Str "\"else\""
          RightParen ")"
      Space " "
      MathText "+"
      Space " "
      Str "\"text\""
      Space " "
      MathText "+"
      Space " "
      MathIdent "alpha"
      Space " "
      MathText "+"
      Space " "
      MathFieldAccess (3 children)
        MathIdent "arrow"
        Dot "."
        MathIdent "r"
      Space " "
      MathText "+"
      Space " "
      MathAttach (2 children)
        MathText "x"
        MathPrimes "'"
      Space " "
      MathText "+"
      Space " "
      MathCall (2 children)
        MathIdent "abs"
        MathArgs (3 children)
          LeftParen "("
          MathText "x"
          RightParen ")"
      Space " "
      MathText "+"
      Space " "
      MathCall (2 children)
        MathIdent "lr"
        MathArgs (3 children)
          LeftParen "("
          MathDelimited (5 children)
            MathText "("
            Space " "
            Math (1 children)
              MathFrac (3 children)
                MathText "a"
                Slash "/"
                MathText "b"
            Space " "
            MathText "]"
          RightParen ")"
    Space " "
    Dollar "$"
  Space " "
  Label "<eq:main>"
  Parbreak "\n\n"
  Hash "#"
  FuncCall (2 children)
    Ident "callout"
    Args (4 children)
      LeftParen "("
      Named (4 children)
        Ident "kind"
        Colon ":"
        Space " "
        Str "\"note\""
      RightParen ")"
      ContentBlock (3 children)
        LeftBracket "["
        Markup (6 children)
          Space "\n  "
          Text "This is a note with"
          Space " "
          Strong (3 children)
            Star "*"
            Markup (1 children)
              Text "strong"
            Star "*"
          Text "."
          Space "\n"
        RightBracket "]"
  Parbreak "\n\n"
  ListItem (3 children)
    ListMarker "-"
    Space " "
    Markup (1 children)
      Text "bullet one"
  Space "\n"
  ListItem (3 children)
    ListMarker "-"
    Space " "
    Markup (3 children)
      Text "bullet two"
      Space "\n  "
      ListItem (3 children)
        ListMarker "-"
        Space " "
        Markup (1 children)
          Text "nested"
  Parbreak "\n\n"
  EnumItem (3 children)
    EnumMarker "+"
    Space " "
    Markup (1 children)
      Text "first"
  Space "\n"
  EnumItem (3 children)
    EnumMarker "+"
    Space " "
    Markup (1 children)
      Text "second"
  Parbreak "\n\n"
  TermItem (6 children)
    TermMarker "/"
    Space " "
    Markup (1 children)
      Text "Term"
    Colon ":"
    Space " "
    Markup (1 children)
      Text "description"
  Parbreak "\n\n"
  Text "Line one"
  Space " "
  Linebreak "\\"
  Space "\n"
  Text "line two."
  Space "\n"
```

Typed walk of the same file (what the IR builder sees via `Markup::exprs()` + accessors):

```
SetRule target=heading args="(numbering: \"1.\")"
ShowRule selector=None transform_kind=Closure
LetBinding kind="Normal(\"x\")" init=Some("1")
ModuleInclude source_kind=Str text="\"x.typ\""
ModuleImport source="\"lib.typ\"" imports=Some("callout") new_name=None
Parbreak
Heading depth=1 body="Introduction"
Space
Label get()="sec:intro"
Parbreak
Text "A paragraph with"
Space
Strong
  Text "strong"
Text ","
Space
Emph
  Text "emph"
Text ", a link "
Link get()="https://example.com"
Space
Text "and"
Space
FuncCall callee=link trailing_comma=false
  Pos kind=Str text="\"https://typst.app\""
  Pos kind=ContentBlock text="[typst]"
    Text "typst"
Text ", a reference"
Space
Ref target="fig:x" supplement=None
Space
Text "and"
Space
Ref target="sec:intro" supplement=Some("Section")
Text ", a footnote"
FuncCall callee=footnote trailing_comma=false
  Pos kind=ContentBlock text="[Note text.]"
    Text "Note text."
Space
Text "and a citation"
Space
Ref target="knuth1984" supplement=None
Space
FuncCall callee=cite trailing_comma=false
  Pos kind=Label text="<knuth1984>"
Text ". Inline math"
Space
Equation block=false text="x^2"
Space
Text "and"
Space
Raw block=false lang=None lines=["raw code"]
Text "."
Parbreak
FuncCall callee=figure trailing_comma=true
  Pos kind=FuncCall text="image(\"plot.png\", width: 80%)"
    (nested FuncCall callee=image)
      Pos kind=Str text="\"plot.png\""
      Named name=width expr_kind=Numeric text="80%"
  Named name=caption expr_kind=ContentBlock text="[A plot of $y = x^2$.]"
    Text "A plot of"
    Space
    Equation block=false text="y = x^2"
    Text "."
Space
Label get()="fig:x"
Parbreak
FuncCall callee=figure trailing_comma=true
  Pos kind=FuncCall text="table(\n    columns: 3,\n    table.header([A], [B], [C]),\n    [a], [b], [c],\n  )"
    (nested FuncCall callee=table)
      Named name=columns expr_kind=Int text="3"
      Pos kind=FuncCall text="table.header([A], [B], [C])"
      Pos kind=ContentBlock text="[a]"
      Pos kind=ContentBlock text="[b]"
      Pos kind=ContentBlock text="[c]"
  Named name=caption expr_kind=ContentBlock text="[A table.]"
    Text "A table."
Space
Label get()="tab:x"
Parbreak
Raw block=true lang=Some("python") lines=["print(\"hi\")"]
Parbreak
Equation block=true text="frac(1, 2) + sum_(i=0)^n x_i + a^2 + sqrt(x) + mat(1, 2; 3, 4) + cases(1 \"if\" x, 0 \"else\") + \"text\" + alpha + arrow.r + x' + abs(x) + lr(( a/b ])"
Space
Label get()="eq:main"
Parbreak
FuncCall callee=callout trailing_comma=false
  Named name=kind expr_kind=Str text="\"note\""
  Pos kind=ContentBlock text="[\n  This is a note with *strong*.\n]"
    Space
    Text "This is a note with"
    Space
    Strong
      Text "strong"
    Text "."
    Space
Parbreak
ListItem
  Text "bullet one"
Space
ListItem
  Text "bullet two"
  Space
  ListItem
    Text "nested"
Parbreak
EnumItem number=None
  Text "first"
Space
EnumItem number=None
  Text "second"
Parbreak
TermItem term="Term" desc="description"
Parbreak
Text "Line one"
Space
Linebreak
Space
Text "line two."
Space
```

Structural facts the IR builder must rely on (all verified above):

- **Blocks are not grouped by the parser.** `Markup` is a flat sequence; paragraphs are
  the runs between `Parbreak`s; consecutive `ListItem`/`EnumItem`/`TermItem` separated by a
  `Space` containing one newline form one list; nested items live **inside** the parent
  item's `Markup`. Headings/figures/equations are separate "blocks" only by convention
  (typst itself treats block-level content as splitting paragraphs).
- **Labels are siblings**: `<fig:x>` after `#figure(...)`, `= Heading <h>`, `$ … $ <eq:main>`
  and a label at the end of a paragraph all parse as a top-level `Label` after an optional
  `Space` (with or without a space: `#figure(image("a.png"))<fig>` → `FuncCall`, `Label`).
  Inside `#block[Body <inner>]` the label is inside that content block's `Markup`.
  Rule: a `Label` attaches to the nearest preceding non-`Space` sibling in the same
  `Markup`. (typst's own semantics; the parser does not attach.)
- **`#` is a separate `Hash` leaf** preceding the expression it introduces (`FuncCall`,
  `SetRule`, …); `Markup::exprs()` skips `Hash` nodes (they are not `Expr`s).
- Trailing content blocks are positional arguments: `#link("u")[t]` → `Args[Str, ContentBlock]`;
  `#callout(kind: "note")[…]` → `Args[LeftParen, Named, RightParen, ContentBlock]`.
- `@fig:x` is a `Ref` with child `RefMarker "@fig:x"`; a supplement `@sec:intro[Section]`
  adds a `ContentBlock` child. The same syntax is used for citations (`@knuth1984`) — the
  parser cannot tell; writer must resolve against the label table (unknown → citation if a
  bibliography exists, else `W-TYP-REF-UNRESOLVED`).
- `#cite(<knuth1984>)` has a `Label` **as a positional argument** (`Arg::Pos(Expr::Label)`).
- `Raw`: children are `RawDelim`, optional `RawLang`, `RawTrimmed` (whitespace/indent to
  drop), `Text` per line. Multi-line verified with `docs/raw3.typ`:
  ```
  Raw (10 children): RawDelim "```", RawLang "rust", RawTrimmed "\n", Text "fn main() {",
    RawTrimmed "\n", Text "    println!(\"x\");", RawTrimmed "\n", Text "}", RawTrimmed "\n", RawDelim "```"
  → Raw block=true lang=Some("rust") lines=["fn main() {", "    println!(\"x\");", "}"]
  An indented block (2 spaces) is dedented: lines=["indented", "  more"]
  ```
  A single-line ```` ``` `backticks` ``` ```` gives `block=false, lang=None, lines=["`backticks`"]`.
- `Equation::block()` is purely syntactic (spaces inside `$ … $`).

### 3.2 Label/misc probe (`docs/labels.typ`, typed walk excerpt)

```
FuncCall callee=figure   Pos kind=FuncCall text="image(\"a.png\")"            ← `#figure(image("a.png"))<fig>`
Label get()="fig"
Heading depth=1 body="Heading"                                               ← `= Heading<h>`
Label get()="h"
FuncCall callee=figure   Named name=caption … Pos kind=ContentBlock "[\n  #image(\"b.png\")\n]"   ← `#figure(caption: [Cap])[ #image("b.png") ] <fig:b>`
Space
Label get()="fig:b"
FuncCall callee=block    Pos kind=ContentBlock → Text "Body with label", Space, Label get()="inner"
Text "Text with trailing label."  Space  Label get()="para"
FuncCall callee=figure trailing_comma=true
  Pos kind=FuncCall text="image(\"c.png\")"
  Named name=caption expr_kind=ContentBlock   Named name=placement expr_kind=Ident text="top"
  Named name=kind expr_kind=Str text="\"plot\""  Named name=supplement expr_kind=ContentBlock text="[Plot]"
FuncCall callee=callout  Named kind="note"  Named title=[T]  Pos ContentBlock → Space, ListItem(a), Space, ListItem(b), Space
Raw block=true lang=None lines=["no lang"]
FuncCall callee=v  Pos kind=Numeric text="1em"
FuncCall callee=pagebreak        (no args: Args has only "(" ")")
FuncCall callee=outline
FuncCall callee=bibliography     Pos kind=Str text="\"refs.bib\""
```

### 3.3 Error recovery (`errors` mode)

Input (deliberately broken: unclosed `#figure(image("a.png"`, `$ x^ $`, `*unclosed strong`,
then valid lines):

```
== errors: 15 warnings: 0 ==
  error: "unclosed delimiter" at line=Some(0) col=Some(7)
  error: "unclosed delimiter" at line=Some(0) col=Some(13)
  error: "expected comma" at line=Some(0) col=Some(21)
  error: "expected comma" at line=Some(2) col=Some(9)
  error: "expected identifier" at line=Some(2) col=Some(16)
  error: "expected comma" at line=Some(2) col=Some(16)
  error: "expected expression" at line=Some(2) col=Some(21)
  error: "expected expression" at line=Some(2) col=Some(27)
  error: "expected comma" at line=Some(2) col=Some(37)
  error: "the character `#` is not valid in code" at line=Some(4) col=Some(0) hints=["you are already in code mode", "try removing the `#`"]
  ... (same for every later `#` line)
```

Lesson: one unclosed `(` makes the parser stay in code mode and swallow the **rest of the
file** into the `Args` of that call (the dump shows every later `#figure`, `#table`, … nested
inside `image(`'s `Args`, each preceded by an `Error "#"`). The IR builder must therefore
(a) always call `errors_and_warnings()` first and (b) treat a document with syntax errors as
failed for the structured backends (report all errors with line/col; do **not** try to build
IR from an erroneous tree). `parse_math("a^")` → 1 error "expected expression", tree
`Math[MathAttach[MathText "a", Hat "^", Error ""]]`; `MathAttach::top()` then returns `None`.

---

## 4. Math

### 4.1 Lexing/parsing rules that decide the tree shape **[verified]** (`lexer.rs` `fn math`, `parser.rs` `fn math_expr_prec`)

- A run of identifier characters is **`MathIdent` only if it is ≥ 2 graphemes**
  (`is_math_id_start`/`is_math_id_continue`); a single letter is `MathText` (grapheme).
  Digits (incl. `3.14`) are grouped into one `MathText` and reported as
  `MathTextKind::Number`; every other char/grapheme (`+`, `=`, `>`, `,`, `(` …) is a
  `MathText` grapheme.
- `ident.field.field` is lexed as a nested `MathFieldAccess` (`arrow.l.long` →
  `MathFieldAccess(MathFieldAccess(arrow . l) . long)`).
- `MathIdent`/`MathFieldAccess` **directly** followed by `(` (no whitespace) → `MathCall`
  with `MathArgs`. Single-letter `f(x)` is **not** a call: `f` and `MathDelimited((x))` are
  wrapped together in an implicit `Math` group (the "continuable" rule). With primes,
  `f'(x)` gives `MathAttach[f, ']` followed by a sibling `MathDelimited`.
- Inside `MathArgs`: arguments are separated by `,`; `;` ends a *row* (`MathArg.ends_in_semicolon`).
  An argument consisting of ≥2 expressions (or 0) is wrapped in a `Math` node; a
  single-expression argument is **not** wrapped (so `frac(1, 2)` args are bare `MathText`).
  Named args `(delim: "[", …)` are `Named(Ident, Colon, value)`; spread `..x` is `Spread`.
- Delimiters `( ) [ ] { } |` and any Unicode `Opening`/`Closing` class char form
  `MathDelimited[open, Math body, close]` when matched (mismatched pairs like `( … ]` are
  fine: `lr(( a/b ])` → `MathDelimited open="(" close="]"`); unmatched → plain `Math`.
  `[|`/`|]` are `MathShorthand`s (⟦ ⟧), `||` → ‖.
- Attachments: `_` and `^` chain into one `MathAttach` (`x_1^2` → base x, bottom 1, top 2);
  `'`-runs are `MathPrimes` attached to the base (`x''` → `primes=Some(2)`).
  Parenthesised attachments are "deparenthesised": `sum_(i=0)` → bottom =
  `Math[LeftParen, Math[i = 0], RightParen]` with `was_deparenthesized() == true`;
  its `exprs()` yields just the inner `Math`.
- `/` → `MathFrac` (precedence 1, left assoc; both operands deparenthesised):
  `a/b` → `MathFrac[a, Slash, b]`; `(a+b)/2` → num is a deparenthesised `Math`.
- `√ ∛ ∜` → `MathRoot`; `&` → `MathAlignPoint`; `\` → `Linebreak`; `"…"` → `Str`;
  `\x` → `Escape`; `#expr` → `Hash` + code expression (e.g. `FuncCall`) embedded in math.
- Shorthands (`MathShorthand::LIST`, text → char): `...`→…, `-`→− (U+2212, e.g. in `e^(-x)`),
  `*`→∗, `~`→∼, `!=`→≠, `:=`→≔, `::=`→⩴, `=:`→≕, `<<`→≪, `<<<`→⋘, `>>`→≫, `>>>`→⋙,
  `<=`→≤, `>=`→≥, `->`→→, `-->`→⟶, `|->`→↦, `>->`→↣, `->>`→↠, `<-`→←, `<--`→⟵, `<-<`→↢,
  `<<-`→↞, `<->`→↔, `<-->`→⟷, `~>`→⇝, `~~>`→⟿, `<~`→⇜, `<~~`→⬳, `=>`→⇒, `|=>`→⤇,
  `==>`→⟹, `<==`→⟸, `<=>`→⇔, `<==>`→⟺, `[|`→⟦, `|]`→⟧, `||`→‖.

### 4.2 Typed math walk of `docs/math.typ` (15 equations) **[verified]**

```
---- "$ frac(a, b) $" (block=true)
  Math deparen=false
    MathCall callee="frac"
      MathArg ends_in_semicolon=false Pos kind=MathText   MathText Grapheme("a")
      MathArg ends_in_semicolon=false Pos kind=MathText   MathText Grapheme("b")
---- "$ sum_(i=0)^n x_i $"
  Math
    MathAttach primes=None
      base:   MathIdent "sum"
      bottom: Math deparen=true → Math deparen=false → MathText Grapheme("i"), MathText Grapheme("="), MathText Number("0")
      top:    MathText Grapheme("n")
    Space
    MathAttach primes=None  base: MathText Grapheme("x")  bottom: MathText Grapheme("i")
---- "$ a^2 + b^2 = c^2 $"
  MathAttach(a, top 2) Space MathText "+" Space MathAttach(b, top 2) Space MathText "=" Space MathAttach(c, top 2)
---- "$ sqrt(x) + root(3, x) $"
  MathCall callee="sqrt" [Pos MathText x]  … MathCall callee="root" [Pos Number("3"), Pos Grapheme("x")]
---- "$ mat(1, 2; 3, 4) $"
  MathCall callee="mat"
    MathArg ends_in_semicolon=false Pos Number("1")
    MathArg ends_in_semicolon=true  Pos Number("2")      ← row break after this arg
    MathArg ends_in_semicolon=false Pos Number("3")
    MathArg ends_in_semicolon=false Pos Number("4")
---- "$ cases(1 \"if\" x > 0, 0 \"else\") $"
  MathCall callee="cases"
    MathArg Pos kind=Math → Math[ Number("1"), Space, Str "if", Space, Grapheme("x"), Space, Grapheme(">"), Space, Number("0") ]
    MathArg Pos kind=Math → Math[ Number("0"), Space, Str "else" ]
---- "$ x' + x'' + f'(x) $"
  MathAttach primes=Some(1) base x … MathAttach primes=Some(2) base x … MathAttach primes=Some(1) base f, MathDelimited open="(" close=")" → Math[x]
---- "$ abs(x) + norm(v) + lr(( a/b ]) $"
  MathCall abs [x] … MathCall norm [v] … MathCall lr [ Pos kind=MathDelimited open="(" close="]" → Math[ MathFrac num a denom b ] ]
---- "$ alpha + beta + arrow.r + arrow.l.long + dot + RR + NN $"
  MathIdent "alpha" … MathIdent "beta" … MathFieldAccess target="arrow" field="r" … MathFieldAccess target="arrow.l" field="long" … MathIdent "dot" … MathIdent "RR" … MathIdent "NN"
---- "$ integral_0^oo e^(-x) dif x $"
  MathAttach base MathIdent "integral" bottom Number("0") top MathIdent "oo"
  MathAttach base e top Math deparen=true → Math[ MathShorthand "-" -> '−', Grapheme("x") ]
  MathIdent "dif"  Space  MathText Grapheme("x")
---- "$ vec(1, 2, 3) + binom(n, k) $"
  MathCall vec [1,2,3]  …  MathCall binom [n, k]
---- "$ hat(x) + tilde(y) + overline(z) + bold(v) + bb(R) + cal(F) $"
  MathCall hat [x] … tilde [y] … overline [z] … bold [v] … bb [R] … cal [F]
---- "$ \"text\" + op(\"foo\") + lim_(x -> 0) $"
  Str "text" … MathCall op [ Pos kind=Str "foo" ] … MathAttach base MathIdent "lim" bottom Math deparen=true → Math[ x, Space, MathShorthand "->" -> '→', Space, Number("0") ]
---- "$ x_1^2 + x_(i j) + e^(i pi) $"
  MathAttach base x bottom Number("1") top Number("2") … MathAttach base x bottom Math(deparen) → Math[i, Space, j] … MathAttach base e top Math(deparen) → Math[i, Space, MathIdent "pi"]
---- "$ a & = b \\ c & = d $"
  MathText a, Space, MathAlignPoint, Space, MathText "=", Space, MathText b, Space, Linebreak, Space, MathText c, Space, MathAlignPoint, Space, MathText "=", Space, MathText d
```

CST of two representative equations (for the fleet's snapshot tests):

```
Equation (5 children)                       ← `$ sum_(i=0)^n x_i $`
  Dollar "$"
  Space " "
  Math (3 children)
    MathAttach (5 children)
      MathIdent "sum"
      Underscore "_"
      Math (3 children)
        LeftParen "("
        Math (3 children)
          MathText "i"
          MathText "="
          MathText "0"
        RightParen ")"
      Hat "^"
      MathText "n"
    Space " "
    MathAttach (3 children)
      MathText "x"
      Underscore "_"
      MathText "i"
  Space " "
  Dollar "$"

Equation (5 children)                       ← `$ integral_0^oo e^(-x) dif x $`
  Dollar "$"
  Space " "
  Math (7 children)
    MathAttach (5 children)
      MathIdent "integral"
      Underscore "_"
      MathText "0"
      Hat "^"
      MathIdent "oo"
    Space " "
    MathAttach (3 children)
      MathText "e"
      Hat "^"
      Math (3 children)
        LeftParen "("
        Math (2 children)
          MathShorthand "-"
          MathText "x"
        RightParen ")"
    Space " "
    MathIdent "dif"
    Space " "
    MathText "x"
  Space " "
  Dollar "$"
```

### 4.3 How typst evaluates math names (for a faithful converter) **[verified]** (`typst-eval-0.15.1/src/math.rs`, `call.rs`)

- `MathText` grapheme → `SymbolElem` (a symbol char); `MathText` number → `TextElem`.
- `MathIdent` → `vm.scopes.get_in_math(&ident)` (the **math scope**: `typst-library`'s
  `math` module = functions + symbols from `codex::SYM` + text operators + spacings + `dif`/`Dif`).
- `MathFieldAccess` → field access on the resolved value (symbol modifier: `arrow.r`).
- `MathCall`: if the callee value is a function → call it; **if it is not a function (e.g. a
  symbol or an undefined-but-resolvable value) → render callee followed by the args as
  content** (`unparse_math_args`). This is why `hat(x)` works: `hat` is the *symbol* U+0302;
  calling a symbol value triggers `get_accent_func` (accent symbols are callable, see 4.5).
- `MathShorthand` → runtime symbol; `MathAlignPoint` → `AlignPointElem`;
  `MathDelimited` → `LrElem(open + body + close)`; `MathAttach` → `AttachElem{t, b, tr: primes}`;
  `MathFrac` → `FracElem` (records deparenthesised flags); `MathRoot` → `RootElem{index}`.

### 4.4 Names defined in typst's math scope (`typst-library-0.15.1/src/math/mod.rs`, `op.rs`, `lr.rs`, `style.rs`, `root.rs`) **[verified]**

Element functions (function name = struct name without `Elem`, lower-case):
`equation, text, lr, mid, attach, stretch, scripts, limits, accent, underline, overline,
underbrace, overbrace, underbracket, overbracket, underparen, overparen, undershell,
overshell, cancel, frac, binom, vec, mat, cases, root, class, op, primes`.
Plain functions: `abs, norm, round, sqrt` (plus `floor, ceil` in `lr.rs`), styles
`upright, bold, italic, serif, sans, scr, cal, frak, mono, bb`, sizes
`display, inline, script, sscript`. Spacings: `thin, med, thick, quad, wide`.
Text operators (`ops!` macro in `op.rs`; `(limits)` = limits by default): `arccos, arcsin,
arctan, arg, cos, cosh, cot, coth, csc, csch, ctg, deg, det (limits), dim, exp, gcd (limits),
lcm (limits), hom, id, im, inf (limits), ker, lg, lim (limits), liminf "lim inf" (limits),
limsup "lim sup" (limits), ln, log, max (limits), min (limits), mod, Pr (limits), sec, sech,
sin, sinc, sinh, sup (limits), tan, tanh, tg, tr`; plus `dif` (`d`) and `Dif` (`D`).
Symbols: everything in `codex::SYM` (334 symbols / 1206 variants), including `NN ZZ QQ RR CC`
(ℕ ℤ ℚ ℝ ℂ), `oo`/`infinity`, Greek, arrows, relations.

LaTeX targets for the element functions (suggested, matches pandoc/tylax output in §5):
`frac(a,b)`→`\frac{a}{b}`, `binom`→`\binom`, `sqrt(x)`→`\sqrt{x}`, `root(n,x)`→`\sqrt[n]{x}`,
`vec(…)`→`\begin{pmatrix} … \\ … \end{pmatrix}` (one column), `mat(…;…)`→`pmatrix`
(typst default `delim: "("`; `delim: "["`→`bmatrix`, `"|"`→`vmatrix`, `none`→`matrix`),
`cases(…)`→`\begin{cases}` rows joined by `\\`, `abs`→`\left|x\right|`, `norm`→`\left\|x\right\|`,
`floor`/`ceil`→`\left\lfloor…\right\rfloor`/`\lceil…\rceil`, `lr(…)`→`\left…\right…`
(use `\left.` for a missing side), `hat`→`\hat` (tylax uses `\widehat`), `tilde`→`\tilde`,
`overline`→`\overline`, `underline`→`\underline`, `underbrace(x, l)`→`\underbrace{x}_{l}`,
`overbrace`→`\overbrace{x}^{l}`, `bold`→`\mathbf`, `upright`→`\mathrm`, `italic`→`\mathit`,
`bb`→`\mathbb`, `cal`→`\mathcal`, `frak`→`\mathfrak`, `sans`→`\mathsf`, `mono`→`\mathtt`,
`scr`→`\mathscr` (needs `mathrsfs`), `op("x")`→`\operatorname{x}`, `limits(x)`→`x\limits`,
`scripts(x)`→`x\nolimits`, `display`→`\displaystyle`, `inline`→`\textstyle`,
`"text"`→`\text{text}`, `dif`→`\mathrm{d}`, `thin`→`\,`, `med`→`\:`, `thick`→`\;`,
`quad`→`\quad`, `wide`→`\qquad`, `&`→`&`, `\`→`\\` (wrap in `aligned`/`align*`), primes
`x''`→`x''`, `MathText "-"`/shorthand `−`→`-`.

### 4.5 Accents **[verified]** (`typst-library/src/math/accent.rs`)

`ACCENTS: &[(char, &[&str])]` (combining char, alternative spellings): U+0300 grave (`` ` ``),
U+0301 acute (`´`), U+0302 hat (`^`, `ˆ`), U+0303 tilde (`~`, `∼`, `˜`), U+0304 macron (`¯`),
U+0305 overline (`-`, `–`, `‾`, `−`), U+0306 breve (`˘`), U+0307 dot (`.`, `˙`, `⋅`),
U+0308 diaer (`¨`), U+20DB dot.triple, U+20DC dot.quad, U+030A circle (`∘`, `○`),
U+030B acute.double (`˝`), U+030C caron (`ˇ`), U+20D6 arrow.l (`←`), U+20D7 arrow (`→`, `⟶`),
U+20E1 arrow.l.r (`↔`, `⟷`), U+20D0 harpoon.lt (`↼`), U+20D1 harpoon (`⇀`).
The function names users write (`hat`, `tilde`, `grave`, `acute`, `breve`, `caron`,
`macron`, `diaer`, `dot`, `dot.double`, `arrow`, `harpoon`, …) are **codex symbols** whose
values are these chars (`hat ^`, `tilde`, `macron ¯`, `dot` with variants `.op ⋅` …).
So the converter's rule: `MathCall` whose callee resolves (via codex) to a char in `ACCENTS`
→ LaTeX accent (`\hat`, `\tilde`, `\bar` (macron), `\vec` (arrow), `\dot`, `\ddot`
(dot.double), `\breve`, `\check` (caron), `\grave`, `\acute`, `\mathring` (circle),
`\overleftarrow`, `\overrightarrow`, `\overleftrightarrow`).

### 4.6 `codex` 0.3.0 API **[verified]** (`codex-0.3.0/src/lib.rs`, `shared.rs`)

```rust
pub struct Module(&'static [(&'static str, Binding)]);
impl Module { pub fn get(&self, name: &str) -> Option<Binding>;            // binary search
              pub fn iter(&self) -> impl Iterator<Item = (&'static str, Binding)>; }
pub struct Binding { pub def: Def, pub deprecation: Option<&'static str> }
pub enum Def { Symbol(Symbol), Module(Module) }
pub enum Symbol { Single(&'static str),
                  Multi(&'static [(ModifierSet<&'static str>, &'static str, Option<&'static str>)]) }
impl Symbol {
    pub fn get(&self, modifs: ModifierSet<&str>) -> Option<(&'static str, Option<&str>)>;  // (value, deprecation)
    pub fn variants(&self) -> impl Iterator<Item = (ModifierSet<&'static str>, &'static str, Option<&'static str>)>;
    pub fn modifiers(&self) -> impl Iterator<Item = &str> + '_;
}
pub struct ModifierSet<S>(S);   // S: Deref<Target = str>, dot-separated
impl<S: Deref<Target = str>> ModifierSet<S> {
    pub fn from_raw_dotted(s: S) -> Self;  pub fn is_empty(&self) -> bool;  pub fn as_str(&self) -> &str;
    pub fn as_deref(&self) -> ModifierSet<&str>;  pub fn insert_raw(&mut self, m: &str) where S: AddAssign<&str>;
    pub fn iter(&self) -> impl Iterator<Item = &str>;  pub fn contains(&self, m: &str) -> bool;
    pub fn best_match_in<'a, T>(&self, variants: impl Iterator<Item = (ModifierSet<&'a str>, T)>) -> Option<T>;
    pub fn is_subset(&self, other: ModifierSet<&str>) -> bool;
}
pub const ROOT: Module;   // ("emoji", EMOJI), ("sym", SYM)
pub const SYM: Module;    // generated from src/modules/sym.txt (format: `name value`, `  .mod value`, `  .mod.mod value`)
pub const EMOJI: Module;
pub mod styling { pub enum MathStyle; pub enum MathVariant; pub fn to_style(c: char, style: MathStyle) -> ToStyle;
                  pub fn to_bold(c: char) -> char; to_italic, to_bold_italic, to_script, to_bold_script, to_fraktur,
                  to_bold_fraktur, to_sans_serif, to_sans_serif_bold, …; is_digit/is_latin/is_greek/… }
```

Note `ModifierSet::from_raw_dotted` needs `S: Deref<Target = str>` — pass `&str`
(`joined.as_str()`), not `&String`. Modifier matching is by best superset match: `arrow`
with no modifiers gives `→` (first variant `.r`), `arrow.r.long.double` → ⟹.

Probe results (`codex` mode): `alpha`→"α" U+03B1, `arrow.r`→"→" U+2192, `arrow.l.long`→"⟵"
U+27F5, `dot`→"⋅" U+22C5 (same as `dot.op`), `RR`→"ℝ" U+211D, `oo`/`infinity`→"∞",
`arrow`→"→", `arrow.r.long.double`→"⟹" U+27F9, `lt.eq`→"≤", `eq.not`→"≠", `emptyset`→"∅",
`nabla`→"∇", `partial`→"∂", `sum`→"∑", `integral`→"∫", `times`→"×", `in`→"∈", `subset.eq`→"⊆",
`pi`→"π", `Gamma`→"Γ", `ell`→"ℓ", `colon.eq`→"≔". **Not in codex** (expected): `hbar`
(typst name is `planck.reduce`? — `planck` → ħ exists as `planck ħ`; `planck.reduce` → None),
`dif`, `bb`. `codex::SYM` = 334 symbols / 1206 variants; `EMOJI` = 772 / 1386.
`arrow` modifiers: `b, bar, bl, br, ccw, curve, cw, dashed, dotted, double, dstruck, filled,
half, hook, l, long, loop, not, open, quad, r, squiggly, stop, stroked, struck, t, tail,
tilde, tl, tr, triple, turn, twohead, wave, zigzag`.

---

## 5. Existing converters — measured

### 5.1 pandoc 3.12.1 typst reader → LaTeX **[verified]** (`pandoc -f typst -t latex docs/eq/N.typ`)

| # | typst | pandoc LaTeX |
|---|---|---|
| 1 | `frac(a, b)` | `\frac{a}{b}` |
| 2 | `sum_(i=0)^n x_i` | `\sum_{i = 0}^{n}x_{i}` |
| 3 | `a^2 + b^2 = c^2` | `a^{2} + b^{2} = c^{2}` |
| 4 | `sqrt(x) + root(3, x)` | `\sqrt{x} + \sqrt[3]{x}` |
| 5 | `mat(1, 2; 3, 4)` | `\begin{pmatrix}\n1 & 2 \\\n3 & 4\n\end{pmatrix}` |
| 6 | `cases(1 "if" x > 0, 0 "else")` | `\begin{cases}\n1\text{ if }x > 0 \\\n0\text{ else}\n\end{cases}` |
| 7 | `x' + x'' + f'(x)` | `x' + x'' + f'(x)` |
| 8 | `abs(x) + norm(v) + lr(( a/b ])` | **parse error** `unexpected "$" … expecting …` (mismatched `( … ]` inside `lr`) |
| 9 | `alpha + beta + arrow.r + arrow.l.long + dot + RR + NN` | `\alpha + \beta + \rightarrow + \longleftarrow + \cdot + {\mathbb{R}} + {\mathbb{N}}` |
| 10 | `integral_0^oo e^(-x) dif x` | `\int_{0}^{\infty}e^{- x}dx` |
| 11 | `vec(1, 2, 3) + binom(n, k)` | `\begin{pmatrix}\n1 \\\n2 \\\n3\n\end{pmatrix} + \binom{n}{k}` |
| 12 | `hat(x) + tilde(y) + overline(z) + bold(v) + bb(R) + cal(F)` | `\hat{x} + \tilde{y} + \overline{z} + \mathbf{v} + \mathbb{R} + \mathcal{F}` |
| 13 | `"text" + op("foo") + lim_(x -> 0)` | `\text{ text } + \operatorname{foo} + \lim\limits_{x \rightarrow 0}` |
| 14 | `x_1^2 + x_(i j) + e^(i pi)` | `x_{1}^{2} + x_{ij} + e^{i\pi}` |
| 15 | `a & = b \ c & = d` | `\begin{aligned}\na & = b \\\nc & = d\n\end{aligned}` |
| x1 | `lr(( a/b )) + lr(\| x \|) + floor(x) + ceil(x)` | `\left. \left( \frac{a}{b} \right) \right. + \|x\| + \left\lfloor x \right\rfloor + \left\lceil x \right\rceil` |
| x2 | `limits(sum)_(i = 1)^N + scripts(max)_x + op("argmax", limits: #true)_x + hat(x y) + overbrace(a + b, "sum")` | `\sum\limits_{i = 1}^{N} + \max_{x} + \operatorname{argmax}\limits_{x} + \hat{xy} + \overset{\text{ sum}}{\overbrace{a + b}}` |

Whole-document behaviour (`pandoc -f typst -t native docs/sample_pandoc.typ`; needs the
`#include`d and `#import`ed files to exist): headings → `Header`, `<label>` → a separate
`Para [Span ("label",[],[]) []]` (not attached to the figure/heading), `@fig:x` →
`Cite [Citation{citationId="fig"}] [Str "[fig]"], Str ":x"` (**splits at the colon** — refs
with `:` are mangled), `#figure(image, caption)` → `Figure` with `Image … [("width","80%")]`,
`#figure(table(...))` → `Table` with caption, `#callout(kind: "note")[…]` (unknown function)
→ the body only (`kind` dropped silently), `#set`/`#show`/`#let` ignored, raw block →
`CodeBlock ("",["python"],[])`, `/ Term: description` → `DefinitionList`, `\` → `LineBreak`.
`--mathml` output is also available (`-t html --math-method=mathml`).

**Verdict:** best math quality of all candidates, but an external GPL process; fails hard on
`lr(( … ])` and mis-parses colon labels. Use as a **test oracle** (and as an optional
`writer export --via-pandoc` fallback), not as the primary path.

### 5.2 tylax 0.3.8 (Rust, Apache-2.0) typst → LaTeX **[verified]** (`conv-probe tylax-math`)

API (`tylax::{typst_to_latex, typst_to_latex_with_options, typst_to_latex_with_diagnostics,
typst_document_to_latex, T2LOptions, DocumentWrapperMode}`):

```rust
pub fn typst_to_latex(input: &str) -> String;
pub fn typst_to_latex_with_options(input: &str, options: &T2LOptions) -> String;
pub fn typst_to_latex_with_diagnostics(input: &str, options: &T2LOptions) -> ConversionResult; // { output: String, warnings: Vec<ConversionWarning{kind, message, span}> }
pub fn typst_document_to_latex(input: &str) -> String;                     // full \documentclass wrapper
pub struct T2LOptions { pub full_document: bool, pub document_class: String, pub title: Option<String>,
                        pub author: Option<String>, pub math_only: bool, pub block_math_mode: bool,
                        pub wrapper: DocumentWrapperMode }   // Default: full_document=false, "article"
pub enum DocumentWrapperMode { Default, BodyOnly, Custom { before_body: String, after_body: String } }
```

Internals: `typst_syntax::{parse, parse_math}` (**0.14**) → `MathIr` enum
(`Seq, Linebreak, Symbol, Ident, Number, Operator, Punctuation, Spacing, Delimited, Apply,
Limits, Style, Attachment{base,top,bottom,…}, …`) → emitter; name tables
`data::maps::TYPST_TO_TEX: phf::Map<&str,&str>` (532 entries, e.g. `"RR" => "mathbb{R}"`,
`"alpha" => "alpha"`, `"->" => "rightarrow"`), `DELIMITER_MAP`, `data::typst_compat::
TYPST_MATH_HANDLERS`, `utils::UNICODE_TO_LATEX` (22 chars only); plus a "MiniEval"
evaluator for `#let`/`#for` in markup.

Output on the 15 equations (`\[ … \]` wrapper stripped here):
1 `\frac{a}{b}` · 2 `\sum_{i = 0}^n x_i` · 3 ok · 4 `\sqrt{x} + \sqrt[3]{x}` ·
5 `\begin{matrix} 1 & 2 \\ 3 & 4 \end{matrix}` (**loses the default `(` delimiters**) ·
6 `\begin{cases} 1 \text{if} x > 0 \\ 0 \text{else} \end{cases}` (no spaces around `\text{if}`) ·
7 `x' + x'' + f'\left(x\right)` · 8 `\left|x\right| + \left\|v\right\| + \left(\frac{a}{b}\right]` (**handles the mismatched pair**) ·
9 `\alpha + \beta + \rightarrow + \longleftarrow + \cdot + \mathbb{R} + \mathbb{N}` ·
10 `\int_0^{\infty} e^{- x} \mathrm{d} x` · 11 **`\vec{1}{2}{3}`** (wrong) `+ \binom{n}{k}` ·
12 `\widehat{x} + \widetilde{y} + \overline{z} + \mathbf{v} + \mathbb{R} + \mathcal{F}` ·
13 `\text{text} + \operatorname{foo} + \lim_{x \rightarrow 0}` · 14 `x_1^2 + x_{i j} + e^{i \pi}` ·
15 `\begin{align} a & = b \\ c & = d \end{align}` (numbered `align`, not `aligned`).

Whole document (`typst_document_to_latex(sample.typ)`): `\section{ Introduction}\label{sec:intro}`,
`\textbf`, `\textit`, `\url`, `\href`→**`\url{https://typst.app}` (link text lost)**,
`Figure~\ref{fig:x}`, `Section~\ref{sec:intro}`, `\footnote`, `\cite{knuth1984}`,
`\begin{figure}[htbp]\centering\includegraphics[width=0.80\textwidth]{plot.png}\caption{…}\label{fig:x}\end{figure}`,
`\begin{table}[htbp]…\begin{tabular}{|c|c|c|}…\caption{A table.}\label{tab:x}\end{table}`,
`\begin{lstlisting}[language=python]`, `\[ … \] \label{eq:main}` (label outside the math env),
itemize/enumerate, **term list flattened to `Term description`**, `#include "x.typ"` /
`#import` rendered as the literal text `include x.typ import lib.typ: callout`, `#callout(kind:
"note")[…]` → body only. Warning emitted: `file not found: x.typ`.

**Verdict:** the math converter is good and reusable as an oracle; the markup converter is
not suitable for writer's BYO-template LaTeX export (we need our own IR anyway). Depending on
tylax costs a second `typst-syntax` (0.14.2) + mitex stack in the build. Its `TYPST_TO_TEX`
table (Apache-2.0) is a good *source* for writer's own name table.

### 5.3 docmux-transform-math 0.6.2 typst → LaTeX **[verified]**

`docmux_transform_math::typst_to_latex::typst_to_latex(input: &str) -> String` is a token
rewriter: `frac(a, b)` → `\frac{a, b}` (wrong), `sum_(i=0)^n` → `\sum_(i=0)^n` (wrong),
`mat(1, 2; 3, 4)` → `\mat{1, 2; 3, 4}`, `cases(...)` → `\cases{...}`, `arrow.l.long` passed
through, `oo` and `dif` passed through, `"text"` kept with quotes. **Not usable.**

### 5.4 LaTeX → typst **[verified]** (`typst-syntax-probe mitex`, `conv-probe l2t`)

API: `mitex::convert_math(input: &str, spec: Option<CommandSpec>) -> Result<String, String>`,
`mitex::convert_text(input: &str, spec: Option<CommandSpec>) -> Result<String, String>`
(`None` = built-in default spec); `tylax::latex_to_typst(input: &str) -> String`;
`tex2typst_rs::tex2typst(tex: &str) -> Result<String, String>`;
`docmux_transform_math::latex_to_typst::latex_to_typst(&str) -> String`.

| LaTeX | mitex 0.2.4 | tylax 0.3.8 | tex2typst-rs 0.4.1 (GPL) | docmux 0.6.2 |
|---|---|---|---|---|
| `\frac{a}{b}` | `frac(a ,b )` | `a/b` | `a/b` | `(a)/(b)` |
| `\sum_{i=0}^n x_i` | `sum _(i = 0 )^(n ) x _(i )` | `sum_(i = 0)^(n) x_(i)` | `sum_(i = 0)^n x_i` | `sum_(i = 0)^n x_i` |
| `\sqrt{x} + \sqrt[3]{x}` | **`mitexsqrt(x ) +  mitexsqrt(\[3 \],x )`** | `sqrt(x) + root(3, x)` | `sqrt(x) + root(3, x)` | `sqrt(x) + root(3, x)` |
| `\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}` | **`pmatrix( 1  zws , 2  zws ; 3  zws , 4  )`** | `mat(delim: "(", 1, 2 ; 3, 4)` | **`mat(delim: #none, 1, 2; 3, 4)`** (parens lost) | `mat(delim: "(", 1, 2; 3, 4)` |
| `f(x)=\begin{cases} 1 & \text{if } x>0 \\ 0 & \text{otherwise}\end{cases}` | `f \(x \) =  cases( 1  & #textmath[if ]; x  >  0  , 0  & #textmath[otherwise]; )` | `f (x) = cases(1 & #text[if] x > 0, 0 & #text[otherwise])` | **`mat(delim: #none, 1, "if " x > 0; 0, "otherwise")`** | `f(x) = cases(1 & "if " x > 0, 0 & "otherwise")` |
| `\mathbb{R}^n \to \mathcal{F}` | `bb(R )^(n ) ->  cal(F )` | `RR^(n) -> cal(F)` | `RR^n arrow.r cal(F)` | `RR^n arrow.r cal(F)` |
| `\left( \frac{a}{b} \right] + \left\{ x \right\}` | `lr(\(  frac(a ,b ) \] ) +  lr(\{  x  \} )` | `lr((a/b ]) + { x }` | `lr((a/b]) + {x}` | **`\((a)/(b)\] + {x}`** |
| `\hat{x}+\tilde{y}+\bar{z}+\vec{v}+\dot{a}+\ddot{b}` | `hat(x ) +  tilde(y ) +  macron(z ) +  arrow(v ) +  dot(a ) +  dot.double(b )` | `hat(x) + tilde(y) + overline(z) + arrow(v) + dot(a) + dot.double(b)` | `… macron(z) …` | `… overline(z) …` |
| `\operatorname{foo}(x) + \lim_{x \to 0} f(x) + \int_0^\infty e^{-x} \, dx` | `operatorname(f o o )\(x \) +  lim _(x  ->  0 ) f \(x \) +  integral _(0 )^(oo ) e ^(- x ) thin  d x` | `op("foo") (x) + lim_(x -> 0) f (x) + integral_(0)^(infinity) e^(- x) thin d x` | `op("foo")(x) + lim_(x arrow.r 0) f(x) + integral_0^infinity e^(- x) thin d x` | same as tex2typst |
| `\begin{aligned} a &= b \\ c &= d \end{aligned}` | `aligned( a  &=  b  \  c  &=  d  )` | `a & = b \ c & = d` | `a &= b \ c &= d` | `a &= b \\\nc &= d` |
| `\alpha+\beta+\Gamma+\partial+\nabla+\infty+\cdot+\times` | `alpha + beta + Gamma + diff + nabla + oo + dot.c + times` | `alpha + beta + Gamma + partial + nabla + infinity+ dot + times` | `… diff + gradient + infinity + dot.op + times` | `… partial + nabla + infinity + dot.op + times` |
| `x_{ij} + e^{i\pi} + \mathrm{d}x + \textbf{bold} + \mathbf{v}` | `x _(i j ) +  e ^(i pi ) +  upright(d )x  +  #textbf[bold]; +  mitexmathbf(v )` | `x_(i j) + e^(i pi) + dif x + *b o l d* + upright(bold(v))` | `… textbf b o l d + upright(bold(v))` | `… bold("bold") + bold(v)` |
| `\binom{n}{k}+\overline{z}+\underbrace{a+b}_{c}+\|v\|+|x|` | `binom(n ,k ) + overline(z ) + mitexunderbrace(a + b )_(c ) + \|\| v \|\| + \| x \|` | `binom(n, k) + overline(z) + underbrace(a + b, c) + bar.v.double v bar.v.double + \| x \|` | `… + parallel v parallel +\|x\|` | `… + underbrace(a + b)_c + \|\|v\|\| +\|x\|` |
| `\label{eq:x} \tag{1} \quad \qquad a \, b \; c \! d` | `mitexlabel(1 ) quad wide a thin b thick c negthinspace d` | `<eq-x> #h(1fr) "(1)" quad wide a thin b thick c d` | `label e q : x tag 1 quad wide a thin b; c! d` | `quad wide a thin b thick c d` |
| `\begin{bmatrix}…\end{bmatrix} \begin{vmatrix}…\end{vmatrix}` | `bmatrix(…) vmatrix(…)` | `mat(delim: "[", …) mat(delim: "|", …)` | `mat(delim: #none, …) mat(delim: #none, …)` | `mat(delim: "[", …) mat(delim: "|", …)` |

`mitex::convert_text(r"Hello \textbf{world} $x^2$ \section{Intro}")` →
`Hello #strong[world]; #math.equation(block: false, $x ^(2 )$); #heading(level: 1)[Intro];`.

**mitex output is not standalone typst.** `mitexsqrt`, `pmatrix`, `bmatrix`, `vmatrix`,
`textmath`, `operatorname`, `mitexlabel`, `mitexunderbrace`, `mitexmathbf`, `aligned`,
`negthinspace`, `textbf`, `zws` are names of the `mitex-scope` built in the typst package
(`packages/mitex/specs/latex/standard.typ` on GitHub: `sqrt: define-glob-cmd("{,b}t",
"mitexsqrt", …)`, `pmatrix: matrix-handle(delim: "(")`, `text: define-cmd(1, alias:
"#textmath", …)`, `operatorname: define-cmd(1, handle: it => math.op(math.upright(it)))`,
≈650–700 entries) and evaluated by `mitex-convert`/`mimath` via `eval(code, scope:
mitex-scope)` with the WASM plugin (`#import "@preview/mitex:0.2.x": *`; README names 0.2.7)
**[verified via WebFetch of the package sources; the exact helper implementations were not read]**.

**Verdict:** for the Markdown backend's LaTeX-math → typst path use **tylax
`latex_to_typst`** when standalone typst text is needed (`writer convert` to `.typ`), or the
**mitex typst package at compile time** (`#mi("…")`/`#mitex("…")`) when only the PDF is
needed (most battle-tested: README claims 32.5k OI-Wiki equations). Avoid tex2typst-rs
(GPL-3.0, loses `pmatrix` delimiters, mangles `\textbf`/`\label`).

### 5.5 typst → OMML (for DOCX): typort-math 0.2.0 **[verified source, not run]**

`pub fn equation_to_omml(content: &typst::foundations::Content) -> String` — takes an
**evaluated** `EquationElem` (so it needs the `typst` compiler in-process to evaluate the
equation first), walks `SequenceElem/TextElem/SymbolElem/AttachElem/FracElem/LrElem/
MatElem/CasesElem/VecElem/RootElem/AccentElem/OpElem/Under*/Over*Elem/AlignPointElem/
LinebreakElem` and writes `<m:oMath>`/`<m:oMathPara>` with quick-xml. Relevant to the DOCX
backend research; a writer-owned typst-syntax→OMML emitter would mirror the same cases as
the LaTeX emitter.

### 5.6 Recommendation for writer

- **typst-math → LaTeX: own converter** (`src/typst/math_to_latex.rs`, est. 500–800 LOC):
  walk `ast::Math` (§4.2 shapes), resolve `MathIdent`/`MathFieldAccess` with codex (§4.6),
  then Unicode→LaTeX via a generated table (§6) with a curated override; function table from
  §4.4. Snapshot-test against pandoc and tylax outputs (both installed/buildable here).
- **LaTeX-math → typst** (Markdown backend → typst/PDF): tylax `latex_to_typst` or mitex
  package at compile time; decision depends on whether writer compiles typst in-process
  (then package download is needed once) — see open questions.

---

## 6. Unicode → LaTeX command table source **[verified]**

`unicode-math-table.tex` (TeX Live 2026, Homebrew path above), 2469 lines, 241 443 bytes,
**2448 entries**, one per line:

```
\UnicodeMathSymbol{"00021}{\mathexclam               }{\mathclose}{exclamation mark}%
\UnicodeMathSymbol{"003B1}{\mupalpha                 }{\mathalpha}{small alpha, greek}%
\UnicodeMathSymbol{"02192}{\rightarrow               }{\mathrel}{/rightarrow /to a: rightward arrow}%
\UnicodeMathSymbol{"02211}{\sum                      }{\mathop}{summation operator}%
\UnicodeMathSymbol{"0211D}{\BbbR                     }{\mathalpha}{/bbb r, open face r}%
\UnicodeMathSymbol{"1D6FC}{\mitalpha                 }{\mathalpha}{mathematical italic small alpha}%
\UnicodeMathSymbol{"1D44E}{\mita                     }{\mathalpha}{mathematical italic small a}%
```

Grammar per line: `\UnicodeMathSymbol{"<hex, 5 digits>}{\<command><padding>}{\<class>}{<description>}%`.
Classes (counts): `\mathalpha` 1043, `\mathrel` 594, `\mathord` 455, `\mathbin` 175, `\mathop` 62,
`\mathopen` 33, `\mathclose` 30, `\mathaccent` 25, `\mathaccentwide` 10, `\mathbotaccentwide` 7,
`\mathunder` 3, `\mathpunct` 3, `\mathover` 3, `\mathfence` 3, `\mathbotaccent` 1,
`\mathaccentoverlay` 1. 11 code points have two commands (e.g. U+0302 `\hat`/`\widehat`,
U+0303 `\tilde`/`\widetilde`, U+0305 `\overbar`/`\wideoverbar`, U+0306 `\breve`/`\widebreve`,
U+030C `\check`/`\widecheck`). 996 entries are in the U+1Dxxx math-alphanumeric block
(`\mita`, `\mbfa`, `\BbbA`, …); 1329 in U+2xxx.

Licence: LPPL 1.3c (header of `unicode-math.sty`; CTAN page) — the `.tex` file carries only
`%%^^A%% unicode-math-table.tex -- part of UNICODE-MATH <wspr.io/unicode-math>`. The
Homebrew TeX Live does not ship the package's `doc/` directory (`kpsewhich --format=doc`
finds nothing); the `LICENSE` text is in the upstream repo https://github.com/latex3/unicode-math.

Caveats for a typst→LaTeX table generated from it:

1. **Greek**: typst's `alpha` is U+03B1 (codex); unicode-math maps U+03B1 → `\mupalpha`
   (upright) and U+1D6FC → `\mitalpha`. Standard LaTeX wants `\alpha`. Override U+0391–U+03C9
   (and variants `ϵ ϑ ϰ ϕ ϱ ϖ`) to `\Gamma … \omega`, `\epsilon`, `\varphi`, …; uppercase
   Greek that coincide with Latin shapes (`Α Β Ε Ζ Η Ι Κ Μ Ν Ο Ρ Τ Υ Χ`) → plain letters.
2. **unicode-math-only names** (`\BbbR`, `\mbfA`, `\mscrF`, `\mathexclam`, …) don't exist in
   amsmath/amssymb; emit `\mathbb{R}`, `\mathbf{A}`, `\mathcal{F}`, `!` instead (decompose
   U+1D400–U+1D7FF via the description or via `codex::styling` reverse tables).
3. The BYO-LaTeX-template contract should state that the template must load `amsmath` and
   `amssymb` (and `mathrsfs` for `\mathscr`, `stmaryrd` for `⟦ ⟧` `\llbracket`); for symbols
   whose command needs a package outside that set, emit `\symbol{"XXXX}`/Unicode char with a
   `W-TYP-MATH-SYM` diagnostic, or require `unicode-math` (XeLaTeX/LuaLaTeX only).
4. Licence hygiene: a Rust table derived from an LPPL file is a "derived work"; LPPL 1.3c
   permits distribution if the derived file is renamed and identified as modified (it is
   OSI-approved but not MIT). Alternatives with permissive licences: tylax's
   `TYPST_TO_TEX` (typst names → LaTeX, 532 entries, Apache-2.0), the W3C `unicode.xml`
   (https://www.w3.org/Math/characters/unicode.xml, W3C licence, has `<latex>`/`<AMS>`
   fields per code point) **[unverified — not fetched]**, KaTeX's `symbols.js` (MIT)
   **[unverified]**. pandoc's texmath tables are GPL — do not copy.

Suggested build step: a `build.rs`-free, checked-in generated file
`src/typst/unicode_latex_table.rs` produced once by a small script
(`scripts/gen-unicode-latex.py` reading the regex
`\\UnicodeMathSymbol\{"([0-9A-F]{5})\}\{\\([A-Za-z]+)\s*\}\{\\(math[a-z]+)\}\{([^}]*)\}%`),
plus a hand-written `OVERRIDES` map (~150 entries) consulted first.

---

## 7. Recommended "writer-typst" subset

Goal: a `.typ` file that (a) compiles unchanged with the real typst compiler and (b) maps
1:1 onto writer's document IR via the typed AST, without an evaluator. Everything below is
decided by `SyntaxKind` + callee name + argument shapes only.

### 7.1 Markup level (children of any `Markup`)

Allowed and mapped: `Text`, `Space`, `Parbreak`, `Linebreak`, `Escape`, `Shorthand`,
`SmartQuote`, `Strong`, `Emph`, `Raw` (inline / block, `lang`), `Link` (auto-link), `Label`
(attach to previous non-Space sibling), `Ref` (label ref or citation, with optional
supplement), `Heading` (`depth`, body, following `Label`), `ListItem`/`EnumItem`
(`number`)/`TermItem` (grouped by adjacency), `Equation` (`block`, body → math IR, following
`Label`), `LineComment`/`BlockComment` (dropped).

`Hash` + expression, by `Expr` variant:

| expression | rule |
|---|---|
| `FuncCall` with callee `Ident`/`FieldAccess` in the **allow-list** | map to IR; unknown named args → `W-TYP-ARG-IGNORED` |
| allow-list (document) | `figure(body, caption:, kind:, supplement:, placement:, gap:)`, `image(path, width:, height:, alt:, fit:)`, `table(columns:, rows:, align:, stroke:, inset:, …cells)`, `table.header(…)`, `table.footer(…)`, `table.cell(colspan:, rowspan:, align:)[…]`, `table.hline()`/`table.vline()` (ignored with warning), `link(dest)[body]`, `footnote[body]`, `cite(<key>, form:, supplement:)`, `ref(<label>)`, `label("x")`, `heading(level:, numbering:)[…]`, `strong[…]`, `emph[…]`, `raw(text, lang:, block:)`, `underline`, `strike`, `overline`, `smallcaps`, `super`, `sub`, `highlight`, `quote(attribution:, block:)[…]`, `list(..)`, `enum(..)`, `terms(..)`, `align(dir)[…]`, `block[…]`, `box[…]`, `par[…]`, `text(size:, fill:, weight:, style:, font:)[…]`, `v(len)`, `h(len)`, `pagebreak()`, `colbreak()`, `line(length:)`, `outline(title:, depth:)`, `bibliography(path, style:, title:)`, `math.equation(block:, numbering:)[…]`, `lorem(n)` |
| allow-list (writer prelude) | `callout(kind: "note"\|"tip"\|"warning"\|"danger"\|"info", title: [..])[body]`, `include-notebook("x.ipynb", cells:, hide-input:)`, `code-cell(...)`, `figure-grid(...)` etc. — these must be **defined in a typst prelude file** (`writer.typ` / `@local/writer`) so that the real compiler accepts them; writer recognises them by name (see open question 1) |
| `SetRule` | record known settings into IR metadata: `set heading(numbering:)`, `set document(title:, author:, date:)`, `set text(lang:, size:, font:)`, `set page(paper:, margin:, numbering:, columns:)`, `set par(justify:, leading:, first-line-indent:)`, `set figure(numbering:, supplement:)`, `set math.equation(numbering:)`, `set table(…)`, `set raw(…)`; any other target/arg → `W-TYP-SET-IGNORED` (still valid for PDF) |
| `ShowRule` | `show: template.with(...)` / `show: doc => ...` / `show heading: …` → **ignored** for IR (`W-TYP-SHOW-IGNORED`); the PDF path keeps them |
| `LetBinding` | only `Normal` pattern with a literal init (`Str`/`Int`/`Float`/`Bool`/`Numeric`/`ContentBlock`/`Array`/`Dict` of literals): recorded for `#name` substitution; closures (`let f(x) = …`) → `W-TYP-LET-IGNORED` and any later call of `f` → `E-TYP-CALL-UNKNOWN` |
| `Ident` (`#x`) | substitute a recorded literal; else `E-TYP-IDENT-UNKNOWN` |
| `ModuleInclude` with `Str` source | recursive include (resolve relative to the including file; cycle → `E-TYP-INCLUDE-CYCLE`; missing → `E-TYP-INCLUDE-MISSING`); non-literal source → `E-TYP-INCLUDE-DYNAMIC` |
| `ModuleImport` | the writer prelude import is recognised and skipped; `@preview/...` package imports → `W-TYP-IMPORT-IGNORED` (names imported from them become unknown callees); file imports → `W-TYP-IMPORT-IGNORED` |
| `Str`, `Int`, `Float`, `Bool`, `Numeric`, `ContentBlock` (`#[...]`), `Parenthesized`, `Array`, `Dict` | literals: inline as text/content |
| `CodeBlock`, `Conditional`, `ForLoop`, `WhileLoop`, `Closure`, `Contextual`, `Binary`, `Unary` (except `-<number>`), `FieldAccess` (except `table.*`, `math.*`, `figure.caption`), `DestructAssignment`, `LoopBreak/Continue`, `FuncReturn` | `E-TYP-CODE-UNSUPPORTED` (the structured backends refuse; PDF path unaffected) |

Argument shapes accepted inside allowed calls: `Arg::Pos` of `ContentBlock`, `Str`, `Int`,
`Float`, `Bool`, `Numeric`, `Label`, `Ident` (`auto`, `none`, `top`, `bottom`, `left`,
`right`, `center`, `horizon`, …), nested allowed `FuncCall` (`image(...)` inside `figure`,
`table.header(...)` inside `table`), `Array`/`Dict` of those, `Unary(-)` of a number;
`Arg::Named` likewise; `Arg::Spread` → `E-TYP-ARG-SPREAD`. Trailing `[...]` blocks are
positional (§3.1).

### 7.2 Math level (children of `Math`)

All 12 math kinds are structurally fine. Rules:

- `MathCall` callee ∈ §4.4 function set → mapped; a callee that resolves to a codex symbol
  in `ACCENTS` → accent; a callee resolving to another symbol → "symbol followed by
  delimited args" (typst semantics); unknown callee → `W-TYP-MATH-FN-UNKNOWN`, emitted as
  `\operatorname{name}(args)` for LaTeX and `<mi>name</mi>` for MathML/OMML.
- `MathIdent`/`MathFieldAccess` → codex lookup (§4.6) → Unicode → LaTeX (§6); then the
  operator list (`lim`, `sin`, …) → `\lim`, `\sin`, … (`\operatorname{sinc}` where LaTeX
  lacks it); `dif`/`Dif` → `\mathrm{d}`/`\mathrm{D}`; spacings → `\,` `\:` `\;` `\quad` `\qquad`;
  `NN ZZ QQ RR CC` → `\mathbb{N}`…; otherwise `W-TYP-MATH-SYM-UNKNOWN` + `\mathit{name}`.
- `Hash` + code in math (`#text[…]`, `#box(...)`) → `E-TYP-MATH-CODE` (except `#true/#false/#number` as named-arg values, e.g. `mat(delim: #none)`, `op("x", limits: #true)`).
- `MathArgs` rows: split `arg_items()` on `ends_in_semicolon` for `mat`/`cases`.
- Alignment: `MathAlignPoint` → `&`, `Linebreak` → `\\`; an equation containing either is
  emitted as `\begin{aligned}…\end{aligned}` inside `\[ \]` (or `align*`).

### 7.3 Diagnostic code scheme (proposal)

Prefix `TYP` to separate from the Markdown backend's codes; severity letter first.

```
E-TYP-SYNTAX            parser error (message/hints from SyntaxDiagnostic; line/col via Lines)
E-TYP-CODE-UNSUPPORTED  code construct outside the subset (if/for/while/closure/context/{…})
E-TYP-CALL-UNKNOWN      function not in allow-list and not from the writer prelude
E-TYP-IDENT-UNKNOWN     #ident with no recorded literal let-binding
E-TYP-ARG-SPREAD        ..args in an allowed call
E-TYP-INCLUDE-MISSING / E-TYP-INCLUDE-CYCLE / E-TYP-INCLUDE-DYNAMIC
E-TYP-MATH-CODE         `#` code expression inside math (other than literal named-arg values)
W-TYP-ARG-IGNORED       named argument of an allowed call that the IR has no slot for
W-TYP-SET-IGNORED / W-TYP-SHOW-IGNORED / W-TYP-LET-IGNORED / W-TYP-IMPORT-IGNORED
W-TYP-LABEL-ORPHAN      <label> with no preceding element in its Markup
W-TYP-REF-UNRESOLVED    @target matches no label and no bibliography key
W-TYP-MATH-FN-UNKNOWN / W-TYP-MATH-SYM-UNKNOWN
W-TYP-MATH-LATEX-PKG    symbol emitted that needs a package beyond amsmath/amssymb
```

Policy: `E-*` → the HTML/DOCX/LaTeX exports fail (the PDF export via the real compiler is
unaffected and may still succeed); `W-*` → continue. `writer check` prints all of them with
`file:line:col` computed via `Source::lines().byte_to_line_column(offset)` where the offset
comes from `LinkedNode::offset()` during the walk (or from `Source::range` for parser diagnostics).

---

## 8. Performance **[verified]** (`typst-syntax-probe bench`, release build, Apple Silicon)

| input | `typst_syntax::parse` | nodes | `Source::detached` | `Source::replace` (7-byte mid-file edit) |
|---|---|---|---|---|
| `sample.typ` 948 B / 45 lines | 18.5 µs | 506 | 33 µs | 12 µs (reparsed range 390..485) |
| `big.typ` = sample ×300, 284 400 B / 13 500 lines | 5.52 ms (≈51 MB/s) | 151 501 | 4.9 ms | 0.40 ms (reparsed range 142188..142237) |

Serve mode can simply re-parse on every change; incremental `Source::replace` is available
if the writer keeps a `Source` per file.

---

## 9. Verified API hints (copy-paste snippets)

Cargo: `typst-syntax = "0.15.1"`, `codex = "0.3.0"` (default features are fine; `default-features = false`
drops `chinese-number`), `ecow = "0.2"` (for `EcoString` in signatures). Optional oracles:
`tylax = { version = "0.3.8", default-features = false }`, `mitex = "0.2.4"`.

```rust
use typst_syntax::ast::{self, AstNode};
use typst_syntax::{LinkedNode, Source, SyntaxKind, SyntaxNode};

// 1. Parse with diagnostics and line/col.
let source = Source::detached(text.clone());              // or Source::new(file_id, text)
let (errors, warnings) = source.root().errors_and_warnings();
for d in errors.iter().chain(&warnings) {
    let range = match d.span.get() {
        typst_syntax::DiagSpanKind::Detached => None,
        typst_syntax::DiagSpanKind::Range { range, .. } => Some(range),
        typst_syntax::DiagSpanKind::Number { num, sub_range, .. } => source.range(num, sub_range),
    };
    let (line, col) = range.as_ref()
        .and_then(|r| source.lines().byte_to_line_column(r.start))
        .map(|(l, c)| (l + 1, c + 1)).unwrap_or((0, 0));
    eprintln!("{}:{line}:{col}: {}{}", "doc.typ", d.message,
              d.hints.iter().map(|h| format!(" (hint: {})", h.v)).collect::<String>());
}

// 2. Walk markup with the typed AST; keep byte offsets via LinkedNode.
fn walk(markup: ast::Markup, linked: &LinkedNode) {
    for expr in markup.exprs() {
        let node = expr.to_untyped();
        let offset = linked.find(node.span()).map(|n| n.offset());      // Option<usize>
        match expr {
            ast::Expr::Heading(h) => { let depth = h.depth().get(); walk(h.body(), linked); }
            ast::Expr::Label(l) => { let id: &str = l.get(); }
            ast::Expr::Ref(r) => { let target: &str = r.target(); let supp = r.supplement().map(|c| c.body()); }
            ast::Expr::Equation(e) => { let block = e.block(); let math: ast::Math = e.body(); }
            ast::Expr::Raw(r) => { let lang = r.lang().map(|l| l.get().to_string());
                                   let lines: Vec<String> = r.lines().map(|t| t.get().to_string()).collect();
                                   let block = r.block(); }
            ast::Expr::FuncCall(fc) => {
                let callee = callee_name(fc.callee());          // "figure", "table.header", ...
                for arg in fc.args().items() {
                    match arg {
                        ast::Arg::Pos(e) => { /* e.to_untyped().kind(): ContentBlock | Str | FuncCall | Label | ... */ }
                        ast::Arg::Named(n) => { let key: &str = n.name().as_str(); let value = n.expr(); }
                        ast::Arg::Spread(_) => { /* E-TYP-ARG-SPREAD */ }
                    }
                }
            }
            ast::Expr::SetRule(s) => { let target = callee_name(s.target()); let args = s.args(); }
            ast::Expr::ShowRule(_) => { /* W-TYP-SHOW-IGNORED */ }
            ast::Expr::LetBinding(l) => if let ast::LetBindingKind::Normal(ast::Pattern::Normal(ast::Expr::Ident(id))) = l.kind() {
                let init = l.init(); }
            ast::Expr::ModuleInclude(i) => if let ast::Expr::Str(s) = i.source() { let path = s.get(); }
            ast::Expr::ModuleImport(i) => { let src = i.source(); let items = i.imports(); }
            ast::Expr::ListItem(li) => walk(li.body(), linked),
            ast::Expr::EnumItem(ei) => { let n: Option<u64> = ei.number(); walk(ei.body(), linked); }
            ast::Expr::TermItem(ti) => { walk(ti.term(), linked); walk(ti.description(), linked); }
            ast::Expr::Strong(s) => walk(s.body(), linked),
            ast::Expr::Emph(e) => walk(e.body(), linked),
            ast::Expr::Text(t) => { let s: &str = t.get(); }
            ast::Expr::Space(_) | ast::Expr::Parbreak(_) | ast::Expr::Linebreak(_) => {}
            ast::Expr::Shorthand(s) => { let c: char = s.get(); }
            ast::Expr::Escape(e) => { let c: char = e.get(); }
            ast::Expr::SmartQuote(q) => { let double: bool = q.double(); }
            ast::Expr::Link(l) => { let url: &str = l.get(); }
            other => { /* E-TYP-CODE-UNSUPPORTED: other.to_untyped().kind() */ }
        }
    }
}
fn callee_name(e: ast::Expr) -> String {
    match e {
        ast::Expr::Ident(i) => i.get().to_string(),
        ast::Expr::FieldAccess(fa) => format!("{}.{}", callee_name(fa.target()), fa.field().get()),
        other => format!("<{:?}>", other.to_untyped().kind()),
    }
}
let root = source.root();
walk(root.cast::<ast::Markup>().unwrap(), &LinkedNode::new(root));

// 3. Math walk (same for Equation::body() and typst_syntax::parse_math(s).cast::<ast::Math>()).
fn math(e: ast::Expr, out: &mut String) {
    match e {
        ast::Expr::Math(m) => for x in m.exprs() { math(x, out) },       // deparenthesised Math: exprs() yields the inner Math
        ast::Expr::MathText(t) => match t.get() {
            ast::MathTextKind::Grapheme(s) => { /* symbol char: "x", "=", "+", "(" */ }
            ast::MathTextKind::Number(s) => { /* "0", "3.14" */ }
        },
        ast::Expr::MathIdent(i) => { /* codex / ops / dif lookup on i.as_str() */ }
        ast::Expr::MathFieldAccess(fa) => { let path = fa.to_untyped().full_text(); /* "arrow.l.long" */ }
        ast::Expr::MathShorthand(s) => { let c: char = s.get(); }
        ast::Expr::MathAlignPoint(_) => out.push('&'),
        ast::Expr::Linebreak(_) => out.push_str("\\\\"),
        ast::Expr::Str(s) => { let text = s.get(); /* \text{..} */ }
        ast::Expr::MathCall(c) => {
            let name = c.callee().to_untyped().full_text();            // "frac", "mat", "table.cell" ...
            let mut rows: Vec<Vec<ast::Expr>> = vec![vec![]];
            for a in c.args().arg_items() {
                match a.arg { ast::Arg::Pos(e) => rows.last_mut().unwrap().push(e),
                              ast::Arg::Named(n) => { /* delim: "[", limits: #true */ }
                              ast::Arg::Spread(_) => {} }
                if a.ends_in_semicolon { rows.push(vec![]); }
            }
        }
        ast::Expr::MathDelimited(d) => { let open = d.open().to_untyped().full_text(); let close = d.close().to_untyped().full_text(); math(ast::Expr::Math(d.body()), out); }
        ast::Expr::MathAttach(a) => { math(a.base(), out); if let Some(b) = a.bottom() { math(b, out) }
                                     if let Some(t) = a.top() { math(t, out) } if let Some(p) = a.primes() { let n = p.count(); } }
        ast::Expr::MathFrac(f) => { math(f.num(), out); math(f.denom(), out); }
        ast::Expr::MathRoot(r) => { let idx: Option<u8> = r.index(); math(r.radicand(), out); }
        ast::Expr::Space(_) => out.push(' '),
        ast::Expr::Escape(e) => { let c = e.get(); }
        ast::Expr::FuncCall(_) | ast::Expr::Ident(_) | ast::Expr::Bool(_) | ast::Expr::Int(_) => { /* `#…` embedded code */ }
        other => { /* unexpected */ }
    }
}

// 4. codex lookup: "arrow.r.long" -> Some("⟶")
fn symbol(path: &str) -> Option<&'static str> {
    let (name, mods) = path.split_once('.').unwrap_or((path, ""));
    match codex::SYM.get(name)?.def {
        codex::Def::Symbol(sym) => sym.get(codex::ModifierSet::from_raw_dotted(mods)).map(|(s, _dep)| s),
        codex::Def::Module(_) => None,
    }
}

// 5. mitex / tylax / pandoc
let typ: Result<String, String> = mitex::convert_math(r"\frac{a}{b}", None);   // Ok("frac(a ,b )") — needs mitex-scope at typst time
let typ2: String = tylax::latex_to_typst(r"\frac{a}{b}");                    // "a/b" (standalone typst)
let tex: String = tylax::typst_to_latex("$ frac(a, b) $");                   // "\\[\n\\frac{a}{b}\n\\]"
// shell: pandoc -f typst -t latex file.typ      (oracle; fails on `lr(( … ])`)
```

---

## 10. Open questions (for the plan author)

1. **Prelude mechanics.** `#callout(...)` etc. must exist for the real compiler. Options:
   (a) require `#import "writer.typ": *` (writer ships/installs `writer.typ` next to the
   document or as a local package `@local/writer:0.1.0` under the typst package dir) and
   recognise that import; (b) inject the prelude at compile time only (prepend → offsets
   shift; better: compile a wrapper file that `#include`s the user file). The IR walker
   needs only the names. Decide in the typst-compile research note.
2. **In-process `typst` crate vs CLI.** No `typst` CLI is installed; the `typst 0.15.1`
   crates are in the registry. If compiling in-process, `typst-kit` provides fonts/packages
   (`typst-kit` 0.15.1 is present) — not examined here.
3. **List grouping.** Confirm typst's rule for list items separated by a blank line
   (same list, "loose") vs. writer's IR expectations; the parser gives `Parbreak` between
   them, so the grouping is writer's decision.
4. **`#let` substitution.** Whether to support literal substitution at all for the
   structured backends (recommended: yes for `Str`/numbers/`ContentBlock`, nothing else).
5. **Math converter ownership.** Own converter (recommended) vs. depending on tylax (double
   `typst-syntax` 0.14.2/0.15.1 build, 8.7 kLOC pulled in, markup converter unusable).
6. **LaTeX-math → typst path.** tylax (standalone typst text) vs. mitex package at compile
   time (needs `@preview/mitex` download → network on first compile, or vendoring the
   package into writer).
7. **Licence hygiene** of a table generated from `unicode-math-table.tex` (LPPL 1.3c) in an
   MIT project; alternative permissive sources listed in §6 are unverified.
8. **Citations vs refs** both use `@key`; resolution order and whether `#cite(<key>)` and
   `@key` must both be supported in the IR (both parse fine).
9. **Unmatched pandoc behaviours** (colon labels, `lr(( ])`) mean pandoc can only be an
   oracle on a curated corpus; decide which equations go into the snapshot corpus.
10. The W3C `unicode.xml` and KaTeX symbol tables were **not** fetched; if the plan
    prefers a non-LPPL source, verify them first.
