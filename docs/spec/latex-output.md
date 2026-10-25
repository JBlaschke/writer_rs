# Spec: LaTeX output

**Status:** contract. Implemented by T60 (`src/latex/{emit,escape}.rs`: §4–§7 body, escaping, Unicode, highlighting,
cite commands, the `Usage`/`AssetFile` records) and T61 (`src/latex/{mod,template,assets,preamble}.rs`,
`src/pipeline/latex.rs`, `assets/latex/writer-template.tex`: §1–§3, §7.3, §9; `build --to latex`, `dump --latex`,
`template::SAMPLE` (the content of `eject latex-template`; T72 wires the command in `src/cli/eject.rs`, PLAN §3.2),
latexmk tests). Consumers: T71 (`check --target latex`), T72, T74. Cited, never redefined: ir.md, config.md `[latex]`
(§2.6 explains every key, none added), references.md (`resolve::{anchor, term}`, `LabelInfo.supplement`, the TOC
and link rules), math.md (§7 passthrough and conversion, `typst_to_latex::{convert, char_command}`,
`latex_source::{macros_from_config, macro_arity}`), html-output.md (`CodeAttrs`, `html::highlight::{SYNTAXES,
resolve_syntax}`), notebook-mapping.md (`CellOutput::pick("latex")`, §9's LaTeX column), typst-dialect.md
(`WriterWorld`, §10's export names), T03's `typst::render::image`. Facts: R7 (TeX Live 2026, latexmk 4.87, pandoc
3.12.1) and probes re-run on 2026-10-24 with pdflatex and xelatex (§3, §5, §9).

writer never runs TeX. The body is style-free: standard packages plus writer-named hooks a template may define
(§2.4). LaTeX numbers and cites itself (PLAN goal 4): the emitter decides only *whether* an element is numbered (the
IR's `number` is `Some`), writes `\label`/`\ref`/cite commands, and never prints IR numbers.

## 1. Outputs and layout — T61 (file names: T60)

### 1.1 Files
`<out_dir>/<stem>.tex` is the rendered template (§2), UTF-8 with `\n` line ends. `<out_dir>/<stem>_assets/` (cli.md)
is created only when it receives a file:

| source | file in `<stem>_assets/` |
|---|---|
| asset with MIME `image/png`, `image/jpeg`, `application/pdf` | the bytes (`AssetRegistry::read`) |
| asset with MIME `image/svg+xml`, `image/gif`, `image/webp` | `Rendition.pdf` of `typst::render::image(world, bytes, mime, None)`; `pdf: None` or `Err` → `Error::Resource { what: "image", path: <file_name_hint>, message }` (the latex target fails) |
| each `[references] bibliography` file that is not YAML | the bytes (§7.3) |

`<stem>` is the written `.tex` file's stem (`-o out/paper.tex` → `out/paper.tex` + `out/paper_assets/`; cli.md: the
assets directory sits next to the named file). The `.tex` refers to the files as `<stem>_assets/<name>` (relative,
`/`). writer creates the directory, overwrites same-named files, never deletes; presenter's clash check covers both
paths. Stdout (cli.md, one line per output): `Wrote dist/document.tex (<size>)` and, when the directory received
files, `Wrote dist/document_assets/ (3 files, <size>)` (`<size>` = the bytes written there). `dump --latex` prints
exactly the `.tex` `build` writes and writes nothing; it and `check --target latex` compute no renditions.
Determinism: same inputs → byte-identical `.tex` and files (no timestamps, versions, machine paths or ids;
`BTreeMap`/`BTreeSet`; T03's renditions are deterministic).

### 1.2 File names (T60, `emit.rs`)
`build --to latex,typst` writes both exports into the same `<stem>_assets/` (cli.md), so a name both write must hold
the same bytes. Base name: typst-dialect.md §10's export name, computed over the same set (every asset id an
`Inline::Image` of the abstract or body references, whatever its MIME): `file_name_hint` with each character outside
`[A-Za-z0-9._-]` replaced by `-`, assigned in asset-id order, a later duplicate getting `-<id[..6]>` before the
extension. An asset with a §1.1 MIME is written under its base name when that ends, ASCII case-insensitively, in its
MIME's extension (`.png`; `.jpg` or `.jpeg`; `.pdf`; graphicx reads all of them on both engines), else — always for
the rendered MIMEs — under the base name plus `.png`, `.jpg` or `.pdf` (`diagram.svg` → `diagram.svg.pdf`; graphicx
reads multi-dot names, verified with pdflatex and xelatex). An appended name that equals another asset's base name
gets `-<id[..6]>` before the appended extension. `Emitted.images` lists them in first use order (abstract, then
body). Bibliography copies: §7.3. Other image URLs: §4.5.

### 1.3 Module interface (exact)
```rust
// src/latex/emit.rs (T60)
pub struct Emitted {   // abstract_: meta.abstract_ in the boxed context (§4.2), "" when None; toc: §4.3 (0 = none)
    pub body: String, pub abstract_: String, pub toc: u8, pub usage: Usage /* §3.1 */,
    pub images: Vec<AssetFile> /* first use first, §1.2 */,
    pub bibliography: Vec<(PathBuf, String)>, /* §7.3: non-YAML bibliography paths, config order, with their names */
    pub diagnostics: Diagnostics,
}
pub struct AssetFile { pub id: String, pub name: String, pub render: bool }
pub fn emit(document: &Document, registry: &AssetRegistry, config: &Config, stem: &str) -> Emitted;

// src/latex/mod.rs (T61)
pub enum FileSource { Asset { id: String, render: bool }, Bib(PathBuf) }
pub struct PlannedFile { pub name: String, pub source: FileSource }
pub struct LatexOutput { pub tex: String, pub files: Vec<PlannedFile>, pub diagnostics: Diagnostics }
/// Template lookup (§2.5), slot check (§2.3), emit, preamble, `.bib` planning (§7.3), render;
/// `Err` only for template errors (a missing slot is a diagnostic, §2.3).
pub fn render(document: &Document, registry: &AssetRegistry, config: &Config, stem: &str) -> Result<LatexOutput>;
pub fn needs_world(files: &[PlannedFile]) -> bool;    // src/latex/assets.rs (T61): any Asset { render: true }
pub fn materialise(files: &[PlannedFile], registry: &AssetRegistry, world: Option<&mut WriterWorld>)
    -> Result<BTreeMap<String, Vec<u8>>>;
pub fn preamble(usage: &Usage, config: &Config) -> String;          // src/latex/preamble.rs (T61)
// src/latex/template.rs (T61)
pub const SAMPLE: &str = include_str!("../../assets/latex/writer-template.tex");
pub fn environment(map: bool) -> minijinja::Environment<'static>;   // map: §5.4, for the `tex` filter
pub fn missing_slots(source: &str) -> Result<Vec<&'static str>>;   // ⊆ ["preamble", "body"]
```
`pipeline/latex.rs` calls `render`, refuses to write when `diagnostics.has_errors()` (exit 1), then `materialise`s
and writes. It never creates a World: when `needs_world`, it passes the build session's `WriterWorld` (one per
session, typst-dialect.md §6.1; `build_targets` gets it from `WorldSlot::get`, live-reload.md §5.2–§5.3), else
`None`.

## 2. Template contract — T61 (`template.rs`)

### 2.1 Environment (dedicated; never shared with the HTML themes)
```rust
// minijinja 2.24, features ["custom_syntax", "loop_controls"] (T00 pins them)
let mut env = Environment::new();
env.set_syntax(SyntaxConfig::builder()
    .block_delimiters("\\BLOCK{", "}")
    .variable_delimiters("\\VAR{", "}")
    .comment_delimiters("\\#{", "}")
    .build().expect("static delimiters are valid")); // no line_statement_prefix, no line_comment_prefix
env.set_keep_trailing_newline(true);
env.set_trim_blocks(true);                        // a line holding only a \BLOCK{…} leaves nothing
env.set_lstrip_blocks(true);
env.set_undefined_behavior(UndefinedBehavior::SemiStrict); // `if vars.x` works; printing undefined fails
env.set_auto_escape_callback(|_| AutoEscape::None);
env.add_filter("tex", move |s: String| escape::text(&s, map).into_owned());   // §5.1, for `vars`
```
Rules for authors (verified, minijinja 2.24.0): a comment `\#{…}` ends at its **first** `}` (so comment text may not
contain one: `x \#{ a } b }y` → `x  b }y`); inside `\VAR{…}` and `\BLOCK{…}` the expression is tokenised and the
tag ends at the first `}` outside a string literal and outside an open `(`, `[` or `{` of the expression
(`\VAR{ {'a': 1}['a'] }` → `1`, `\VAR{ "x}y" }` → `x}y`); `% \VAR{x}` still substitutes; text inserted by
`\VAR{body}` is never re-parsed; `\BLOCK{raw}…\BLOCK{endraw}` prints tags literally (the way to write a literal
`\VAR{`).

### 2.2 Variables (always all defined)
| variable | type | value |
|---|---|---|
| `preamble`, `body` | string | §3, §4 |
| `title`, `subtitle`, `date` | string | `DocMeta` field text-escaped (§5.1); `""` when absent |
| `authors` | list of `{name, affiliation, email, id}` | `DocMeta.authors`, fields text-escaped, `""` when absent |
| `abstract`; `keywords` | string; list of strings | `Emitted.abstract_`; text-escaped keywords |
| `lang` | string | `meta.lang` → `config.document.lang` → `"en"`, characters outside `[A-Za-z0-9-]` removed |
| `toc` | integer | `Emitted.toc` (0 = no template-level TOC; the preamble already set `tocdepth`) |
| `bibliography` | list of strings | `<stem>_assets/<name>` per copied file (`<name>` ends in `.bib`, §7.3), config order |
| `has_citations` | bool | `document.features.citations` (any `Inline::Cite`; without one no bibliography is printed, references.md §4.9) |
| `cite` | string | `[latex] cite`: `biblatex`, `natbib` or `plain` |
| `emit_bibliography` | bool | `[latex] emit_bibliography` (the body then holds the bibliography commands, §7.2) |
| `has_math`, `has_code`, `has_highlighted`, `has_cells` | bool | `usage.math` (amsmath/amssymb loaded), `usage.code` (fancyvrb loaded), `usage.highlighted` (`writercode` defined), `usage.cells` |
| `vars` | map | `DocMeta.vars` verbatim, unescaped: write `\VAR{vars.x \| default("") \| tex}` or guard with `\BLOCK{if vars.x}`; printing or filtering an undefined key is a render error (SemiStrict) |

The title block exists only as variables: the body never contains `\maketitle`, `\title` or the abstract. Before
calling `preamble`, `render` sets `usage.math = true` when `map` is effective (§5.4) and `title`, `subtitle`,
`date`, an `authors` field, a keyword or any string anywhere in `meta.vars` (what the `tex` filter may print)
contains a character with `escape::map_char(c).is_some()`: its `\ensuremath{…}` macro (`\mathbb`, `\checkmark`,
`\tfrac`, …) needs amsmath/amssymb even in a document without math.

### 2.3 Slots
`missing_slots` compiles the template with §2.1's environment and returns those of `preamble` and `body` absent from
`Template::undeclared_variables(false)`. `render` checks the slots before emitting: for a template that lacks one it
returns `Ok(LatexOutput { tex: String::new(), files: vec![], diagnostics })` with one error
`latex-template-slot-missing` per missing slot (path = the template path, span `None`, message `LaTeX template
<path> has no \VAR{<slot>}`). `build` then writes nothing for latex (exit 1), `dump --latex` prints nothing (exit
1), and `check` reports the errors.

When `toc` is not among the template's undeclared variables and `Emitted.toc > 0`, `render` prepends
`\tableofcontents` and a blank line to `body`, so a template that ignores `toc` (such as the minimal one of §2.4)
keeps the TOC `[document] toc` asked for.

### 2.4 Hooks and override mechanics
The preamble defines a guarded default for each name the body uses (§3.2): a template may restyle any of them, and
`\documentclass{article}\VAR{preamble}\begin{document}\VAR{body}\end{document}` compiles (verified):

| hook | body form | guard in the preamble | a template overrides it with |
|---|---|---|---|
| `writercallout` env | `\begin{writercallout}{<kind>}{<title>}` … `\end{writercallout}` | `\AtBeginDocument{\@ifundefined{writercallout}{\newenvironment…}{}}` | `\newenvironment`, before or after `\VAR{preamble}` |
| `writercell` env | `\begin{writercell}{In\|Out}{<n or empty>}` … | same | same |
| `\writerpagebreak`, `\writerhrule`, `\tightlist` | macros without arguments | `\providecommand` | `\newcommand` before the slot, `\renewcommand` after |
| `\WrComment` … `\WrError` (12, §6) | `\Wr<Class>{<escaped token>}` inside `writercode` | `\providecommand`, emitted in every preamble (definitions only: `\textcolor` expands only on use, and a use implies `highlighted` and so xcolor) | same |
| `writercode` env | `\begin{writercode}[<opts>]` (fancyvrb, `commandchars=\\\{\}`) | none: defined when `usage.highlighted` (inside `\AtBeginDocument` it breaks `commandchars`, R7 §4.2) | after the slot, `\BLOCK{if has_code}\fvset{…}\BLOCK{endif}` or `\BLOCK{if has_highlighted}\RecustomVerbatimEnvironment{writercode}{Verbatim}{commandchars=\\\{\},…}\BLOCK{endif}` |
| `writerlisting` caption type | `\captionsetup{type=writerlisting}` + `\caption{…}` | `\@ifundefined{c@writerlisting}{\DeclareCaptionType…}{}` | `\DeclareCaptionType{writerlisting}[Programm][…]` before the slot, `\captionsetup[writerlisting]{…}` after |

`<kind>` is the IR callout kind lowercased with every character outside `[a-z0-9]` removed (`""` → `note`);
`<title>` is the callout title; else `resolve::term(&lang, Term::Note|Tip|Important|Warning|Caution)` for the five
kinds (`lang` = `LangTag::effective(meta, config)`, English when `None`; as html-output.md and docx-output.md), any
other kind with its first character uppercased; text-escaped, never empty. A
template that keys colours on `<kind>` must accept unknown kinds (§9.1 uses `\@ifundefinedcolor`).

### 2.5 Template resolution and errors
`config.latex.template` (front matter `latex_template`, CLI `--latex-template`; config.md §3), `SAMPLE` when `""`.
Unreadable → `Error::Resource { what: "LaTeX template", path, message }`; parse/render errors → `Error::Template {
template: <path or "writer-template.tex">, line, message }`. `eject latex-template` writes `SAMPLE` byte for byte.

### 2.6 The `[latex]` keys (config.md, fixed)
| key | effect |
|---|---|
| `template` | §2.5 |
| `top_level` | what a level-1 heading becomes (§4.4); never inferred from the template's class |
| `cite` | citation commands (§7.1) and the `cite` variable |
| `emit_bibliography` | `Block::Bibliography` → bibliography commands in the body (§7.2); `false`: the template prints it |
| `code` | `verbatim` (fancyvrb `Verbatim`), `highlighted` (`writercode` + `\Wr*`), `listings` (`lstlisting`), §4.7 |
| `tables`, `booktabs` | `longtable` (page-breaking) or `tabular` (floating `table`) for outer tables; booktabs rules or `\hline` (§4.6) |
| `cleveref` | `\cref`/`\Cref`/`\cpageref` for references (§4.10); `true` → the preamble always loads cleveref, so a template may use `\cref` itself |
| `figure_placement` | the `[…]` of `figure`/`table` floats; `""` → none; `H` needs the template to load `float` |
| `engine`, `unicode` | `engine` picks the Unicode default and lint level, writer never runs TeX; `unicode`: `auto` (= `map` for pdflatex, else `keep`), `map`, `keep` (§5.4) |
| `cell_prompts` | wrap cell inputs and execute results in `writercell` (§4.12) |
| `wrap` | prose line-wrap column; `0` = none (§4.1) |

R7 §11's `dpi`, `ansi_colors` and `assets_dir` are not keys: 96 px per inch (§4.5); ANSI colours are whatever the
notebook frontend left in the IR (the body never contains `\textcolor`, §6); `<stem>_assets/`.

## 3. Generated preamble — T61 (`preamble.rs`), from T60's `Usage`

### 3.1 Usage record (T60 fills it while emitting the body and the abstract)
```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    pub graphics: bool, pub longtable: bool, pub table: bool, pub caption: bool, pub code: bool,
    pub highlighted: bool, pub listings: bool, pub ulem: bool, pub cells: bool,
    pub listing_caption: bool,  pub uncaptioned_longtable: bool,  // §4.7, §4.6
    pub math: bool,                  // math, a task-item box, a §5.5 replacement, a newunicodechar entry, §2.2
    pub math_packages: Vec<&'static str>,              // §4.8: first-use order, deduplicated
    pub color: bool,                                   // highlighted code or \color/\textcolor in math
    pub newunicodechar: Vec<(char, &'static str)>,     // §5.4: (character, macro), first-use order, one per char
    pub secnumdepth: Option<i8>, pub tocdepth: Option<i8>,       // §4.4, §4.3
}
```
A flag is set when the body or abstract emits what §3.2's condition names: `code` = any `Verbatim`, `writercode` or
`lstlisting`; `table` = any longtable or tabular; `caption` = any `\caption`, `\caption*` or `\captionsetup`.
`[latex] cleveref` is read from the config, not recorded (§3.2).

### 3.2 Exact lines, in this order (`[condition]`; lines without one always appear)
```latex
% generated by writer: the packages and default hooks this body uses (no styling)
\makeatletter
\usepackage{iftex}
\ifPDFTeX
  \@ifpackageloaded{fontenc}{}{\usepackage[T1]{fontenc}}
  \@ifpackageloaded{inputenc}{}{\usepackage[utf8]{inputenc}}
  \usepackage{textcomp}
  \usepackage{newunicodechar}                          [newunicodechar ≠ ∅]
  \newunicodechar{≥}{\ensuremath{\geq}}                [one per newunicodechar entry (c, macro), in order]
\else
  \@ifpackageloaded{fontspec}{}{\usepackage{fontspec}}
\fi
\usepackage{graphicx}                                  [graphics]
\usepackage{longtable}                                 [longtable]
\usepackage{booktabs}                                  [table ∧ booktabs]
\usepackage{array}                                     [table]
\usepackage{caption}                                   [caption]
\usepackage{amsmath,amssymb}                           [math]
\usepackage{<p>}                                       [one per math_packages entry, in order]
\usepackage{xcolor}                                    [color]
\usepackage{fancyvrb}                                  [code]
\usepackage{listings}                                  [listings]
\@ifpackageloaded{ulem}{}{\usepackage[normalem]{ulem}}  [ulem]
\usepackage{hyperref}
\usepackage{cleveref}                                  [config.latex.cleveref]
\DefineVerbatimEnvironment{writercode}{Verbatim}{commandchars=\\\{\}}   [highlighted]
\providecommand{\WrComment}[1]{\textcolor[HTML]{969896}{\textit{#1}}}   [always: the 12 lines of §6]
\@ifundefined{c@writerlisting}{\DeclareCaptionType{writerlisting}[Listing][List of Listings]}{}  [listing_caption]
\crefname{writerlisting}{listing}{listings}            [listing_caption ∧ config.latex.cleveref]
\Crefname{writerlisting}{Listing}{Listings}            [listing_caption ∧ config.latex.cleveref]
\@ifundefined{c@none}{\newcounter{none}}{}             [uncaptioned_longtable]
\providecommand{\tightlist}{\setlength{\itemsep}{0pt}\setlength{\parskip}{0pt}}
\providecommand{\writerpagebreak}{\clearpage}
\providecommand{\writerhrule}{\begin{center}\rule{0.5\linewidth}{0.4pt}\end{center}}
\providecommand{\writernotoc}{\let\writer@acl\addcontentsline\def\addcontentsline##1##2##3{\let\addcontentsline\writer@acl}}
\AtBeginDocument{%
  \@ifundefined{writercallout}{%
    \newenvironment{writercallout}[2]{\par\medskip\noindent\begin{minipage}{\linewidth}\textbf{#2}\par\nobreak}{\end{minipage}\par\medskip}}{}%
  \@ifundefined{writercell}{%
    \newenvironment{writercell}[2]{\par\medskip\noindent\textbf{\texttt{#1[#2]:}}\par\nobreak}{\par\medskip}}{}%
}
\setcounter{secnumdepth}{<secnumdepth>}                [secnumdepth]
\setcounter{tocdepth}{<tocdepth>}                      [tocdepth]
\providecommand{\RR}{\mathbb{R}}                       [math: one per macros_from_config entry, §4.8]
\makeatother
```
Option-bearing packages load only if the template has not (no option clash); option-free lines are no-ops when
loaded already. The font block lets one `.tex` compile on pdflatex (T1, UTF-8) and xelatex/lualatex (fontspec).
This block with every condition true compiled with the sample and the minimal template on pdflatex and xelatex;
`\writernotoc` (§4.4) was verified on both engines with `article` and `report` (it swallows exactly the next
`\addcontentsline`; the other entries and the list of figures stay).
Ordering rule for template authors (the sample follows it): `\documentclass` → your packages, fonts, colours,
`writercallout`/`writercell` → `\VAR{preamble}` → `\fvset` (guarded by `has_code`), `\renewcommand{\Wr…}`,
`\hypersetup`, biblatex/natbib → `\begin{document}`.

## 4. Element mapping — T60 (`emit.rs`)

### 4.1 Layout
Top-level blocks are separated by one blank line; the body has no leading blank line and ends with one `\n`. In an
environment the first child follows the `\begin{…}` line, `\end{…}` follows the last child, children are separated
by a blank line (tight list items: `\n`). No indentation. `SoftBreak` → `\n` in paragraphs, a space in headings,
captions, table cells and `\item[…]` terms; never an empty line inside a paragraph. `wrap = N > 0`: spaces from
`Text` and `SoftBreak`s in text context (never in code, math, URLs, raw) are *breakable*; after each top-level block
a greedy pass turns breakable spaces into `\n` so that no line exceeds N characters (Unicode scalar values) unless
it has no breakable space. A run of adjacent breakable spaces is one break opportunity and becomes a single `\n`;
a break is never placed at a line's first character, so the pass never produces an empty line.

### 4.2 Contexts
- **outer**: the document body, and inside `BlockQuote`, list items, definitions, `Div`s, cells without prompts.
  Figures float; tables follow `[latex] tables`.
- **boxed**: inside `Callout`, `writercell`, the abstract, `Figure { kind: Other }` bodies, footnotes. Nothing
  floats: figures and captioned tables use `\begin{center}` + `\captionsetup{type=figure|table}` + `\caption`
  (verified inside tcolorbox, framed and minipage); every table is `tabular`.
- **footnote** (inside `\footnote{…}`; also boxed): verbatim cannot sit in a macro argument, so code blocks are
  `{\ttfamily <line>\newline <line>…}` with §5.2 escaping.

### 4.3 Blocks
| IR block | LaTeX |
|---|---|
| `Paragraph` | its inlines |
| `Heading`, `CodeBlock`, `Figure`, bare `Table`, `Math`, `Bibliography`, `Cell` | §4.4, §4.7, §4.5–4.7, §4.6, §4.8, §7.2, §4.12 |
| `BlockQuote` | `\begin{quote}` … `\end{quote}` |
| `List { Bullet, tight }` | `\begin{itemize}` + `\tightlist` (tight) + items + `\end{itemize}` |
| `List { Ordered { start } }` | `\begin{enumerate}` + `\setcounter{enum<i\|ii\|iii\|iv>}{<start−1>}` when `start ≠ 1` (roman = depth among enumerates) + `\tightlist` (tight) + items |
| list item | `\item ` + the first block on the same line when it is a `Paragraph`, else `\item` alone; task items `\item[$\square$] `/`\item[$\boxtimes$] ` (`usage.math`) |
| nesting beyond LaTeX (5th itemize or enumerate, 7th list level) | emitted anyway + `unsupported-construct` |
| `DefinitionList` | `\begin{description}` + `\tightlist` when every definition is one `Paragraph` + `\item[{<term>}] <definitions>` (several: blank line) |
| `Callout` | label + `\begin{writercallout}{<kind>}{<title>}` + blocks (boxed) + `\end{writercallout}` (§2.4) |
| `Div` | label, then each class `env-<name>` with `<name>` ∈ `^[A-Za-z]+\*?$` wraps the content in `\begin{<name>}`…`\end{<name>}` (class order, first outermost); first class `center` → `\begin{center}`; `small`/`large` → `\begingroup\small`/`\large` … `\par\endgroup`; other classes (`columns`, `column`, `fragment`, …) → children only |
| `Raw { format: "latex" }` | `text` verbatim, trailing newlines trimmed; other formats: nothing, silently (`check --target latex` reports `raw-skipped`, §8) |
| `ThematicBreak`, `PageBreak` | `\writerhrule`, `\writerpagebreak` |
| `Toc { depth }` | `blocks[0]` with `span: None` (frontend-inserted) is lifted: not emitted, `Emitted.toc = depth`, `usage.tocdepth = depth + off`; otherwise `\setcounter{tocdepth}{<depth + off>}` + `\tableofcontents` |

`off` = 0 for `top_level = "section"`, −1 for `chapter`, −2 for `part`. `<a>` is always `resolve::anchor(id)`
(references.md; `fig-plot` → `_` + 39 hex digits). "Label" for elements without a counter of their own (callouts,
divs, spans, cells, code blocks outside a `Figure`, inline `Code`/`Link`/`Image` with an id, unnumbered elements;
references.md §1.2's `Other` labels) means `\phantomsection\label{<a>}` when there is an id, placed before the
element (on its own line before a block).

### 4.4 Headings
LaTeX depth of level L = `min(L + off, 5)`: −1 `part`, 0 `chapter`, 1 `section`, 2 `subsection`, 3 `subsubsection`,
4 `paragraph`, 5 `subparagraph` (deeper levels clamp silently).
- `number: Some(_)` → `\<cmd>{<inlines>}\label{<a>}` (label only with an id); `usage.secnumdepth` = the deepest
  numbered heading. The IR's number pattern ("1.1", "A.1") is not reproduced.
- `number: None` → `\<cmd>*{<inlines>}\label{<a>}` + `\n\addcontentsline{toc}{<cmd>}{<inlines>}`.
- Not listed in the TOC (references.md §4.8): a heading with class `notoc` (alias `unlisted`) or inside
  `meta.abstract_`. Its unnumbered form has no `\addcontentsline`; its numbered form is preceded by a line
  `\writernotoc` (§3.2), which swallows the one `\addcontentsline` the heading command issues.
- Moving argument: `\footnote`, `\footref`, `\sout`, `\uline` get `\protect`; inline math becomes
  `\texorpdfstring{\(<src>\)}{<src text-escaped>}`; `\url{u}` (§4.11) becomes `\texorpdfstring{\url{u}}{<the link
  text, text-escaped>}` (a bare or `\protect`ed `\url` stops hyperref's bookmarks: "File ended while scanning use of
  \@@BOOKMARK", measured on both engines); `\href` needs nothing; with a footnote inside, the numbered form is
  `\<cmd>[<inlines without footnotes>]{<inlines>}` and `\addcontentsline` gets the inlines without footnotes.

### 4.5 Figures (`Image`, `Other`) and images
Outer: `\begin{figure}[<figure_placement>]` (no `[…]` for `""`; a `fig-pos` pair on the figure wins), `\centering`,
body, caption line, `\end{figure}`. Boxed: `\begin{center}`, `\captionsetup{type=figure}`, body, caption line,
`\end{center}`. Caption line (tables and listings use it too, §4.6–4.7):

| `number` | `caption` | caption line |
|---|---|---|
| `Some` | `Some(c)` / `None` | `\caption{<c>}` / `\caption{}` (prints "Figure 2", no colon) + `\label{<a>}` with an id |
| `None` | `Some(c)` | `\caption*{<c>}` + `\phantomsection\label{<a>}` with an id |
| `None` | `None` | `\phantomsection\label{<a>}` with an id, else nothing |

Captions are moving arguments (§4.4's `\protect` rule; with footnotes `\caption[<without them>]{…}`). In a
`Figure { kind: Other }` body, nested figures emit their body plus, when captioned, `\par{\small <caption>}\par` (no
numbering, brief); their labels follow the container's caption line (`\label{<a>}` when the container is numbered,
so `\ref` gives its number; `\phantomsection\label` otherwise).

Images: `\includegraphics[<opts>]{<stem>_assets/<name>}` for an `asset:` URL with a §1.1 MIME. Options from the
`width`/`height` pairs (markdown-dialect.md §4.3 grammar): `N%` → `N/100` × `\linewidth` (width) or `\textheight`
(height); `px` or a bare number → `N/96` `in`; `pt cm mm in em` as written. Numbers have at most 4 decimals,
trailing zeros and point removed (`0.5\linewidth`, `3.125in`, `1\linewidth`).

Width only → `width=W,height=\textheight,keepaspectratio`; height only → `height=H,width=\linewidth,keepaspectratio`;
both → `width=W,height=H`; neither → `width=\linewidth,height=\textheight,keepaspectratio` for a figure image and
no options (natural size) for an inline image. No `alt=` (graphicx ≥ 2021 only). Any other URL (remote, `data:`,
root-absolute, a kept `missing-asset` URL) or MIME → the alt text, text-escaped (`\href{<url>}{<alt>}` for
`http(s)`), + `unsupported-construct`.

### 4.6 Tables
Column spec `@{}<cols>@{}`. Without widths: `l`/`c`/`r` from `ColumnSpec.align` (`Default` → `l`). When any column
has `width: Some`, every column is `>{<A>\arraybackslash}p{\dimexpr <w>\linewidth-2\tabcolsep\relax}` (`<A>`:
`\raggedright` for Default/Left, `\centering`, `\raggedleft`); a missing width is `max(0, 1 − Σ given) / n_missing`,
or `1/n` when that is 0. Rows: cells joined by ` & `, ending ` \\`; a row whose first cell's output starts with `*`
or `[` gets `{}` in front (after `\\` or `\tabularnewline`, LaTeX would read `\\*` or `\\[…]`). A cell with
`colspan n > 1` or an `align` override is `\multicolumn{n}{<spec>}{…}` (`l`/`c`/`r`, or the p-column form with the
summed width). Rows follow the grid model (html-output.md §1.7, docx-output.md §4.12, notebook-mapping.md's
amendment of ir.md §2 invariant 1): a cell with `rowspan r > 1` keeps its content in its first row and covers its
grid columns in the next r − 1 rows of the same section (head, body, foot); a later row lists only its uncovered
cells. While placing a row, each covered grid slot gets an empty cell (nothing between the `&`s) and the row's own
cells fill the free slots left to right; a row still shorter than `columns.len()` is then padded at the end.
`latex-rowspan` (the span is not drawn) once per table. `header` is not rendered. `LineBreak` in a cell:
`\newline` with p-columns, else a space. Rules: `booktabs` → `\toprule`/`\midrule`/`\bottomrule`, otherwise
`\hline` for each.
- **longtable** (outer and `tables = "longtable"`): `\begin{longtable}[]{<spec>}`, caption line + `\tabularnewline`
  (when any), then with head rows `\toprule` head `\midrule` `\endfirsthead` `\toprule` head `\midrule` `\endhead`,
  without `\toprule` `\endhead`; then (`\midrule` + foot rows) `\bottomrule` `\endlastfoot`, body rows,
  `\end{longtable}`. Without a numbered caption the longtable is wrapped `{\def\LTcaptype{none}` … `}`
  (`usage.uncaptioned_longtable`): longtable steps the counter otherwise (measured: the next captioned table became
  "Table 2").
- **tabular** (boxed, or `tables = "tabular"`): `\begin{tabular}{<spec>}`, `\toprule`, (head rows + `\midrule`) only
  when there are head rows, body rows, (`\midrule` + foot rows) only when there are foot rows, `\bottomrule`,
  `\end{tabular}`; an outer `Figure{Table}` puts it in `\begin{table}[<placement>]` `\centering`
  + caption line, anything else in `\begin{center}` (+ `\captionsetup{type=table}` and caption line for a `Figure`).
  Captions go above tables.

### 4.7 Code blocks and listings
The language `lang` is `CodeBlock.lang` (for a cell's input, `cell.language`), passed as written to
`resolve_syntax` (§6) and lowercased for the listings table below. `CodeAttrs::from_attrs(&attrs)`
(html-output.md's `CodeAttrs`, markdown-dialect.md §4.6; for a cell `&cell.attrs`) gives `plain`, `numbered` and
`start`; options `numbers=left` (numbered) and `firstnumber=<start>` (≠ 1) go in `[…]` after `\begin{<env>}`;
`highlight` is ignored. A code block with an id outside a `Figure` is preceded by its label (§4.3).

Notebook outputs — a `CodeBlock` whose classes include `output-text`, `output-stream-stdout`,
`output-stream-stderr`, `output-error` or `output-json` (notebook-mapping.md §9) — are `\begin{Verbatim}`, `text`,
`\end{Verbatim}` in every code mode: never highlighted, never `lstlisting`, the `ansi` pair ignored (footnote
context: §4.2). Every other code block:

| `[latex] code` | block |
|---|---|
| `verbatim` | `\begin{Verbatim}[opts]`, the text untouched, `\end{Verbatim}` |
| `highlighted` | `resolve_syntax(lang, plain)` (§6) is `SyntaxChoice::Syntax`: `\begin{writercode}[opts]`, §6 runs, `\end{writercode}`; `Plain`/`Unknown`: the `verbatim` form |
| `listings` | `\begin{lstlisting}[language=<L>,opts]`, the text untouched, `\end{lstlisting}`; `language=` only from the table below, else `latex-listings-language` (once per language) |

Fence language (lowercased) → `language=`; a value that starts with `[` (a listings dialect) is written in braces,
`language={[Sharp]C}`, since a bare `]` ends the optional argument ("File ended while scanning use of
\lstKV@OptArg@@", fatal). Lua is defined only as dialects and listings.cfg sets no default one for it, so it is
named with `[5.3]`. All 34 values compiled in one document (listings 1.11b, TeX Live 2026, which still lacks Rust,
JavaScript, TypeScript, JSON, YAML, TOML, Julia):
```
python py→Python  c→C  cpp c++ cxx→C++  csharp cs→{[Sharp]C}  java→Java  go→Go  r→R  bash sh shell zsh→bash  haskell→Haskell
lua→{[5.3]Lua}  ruby→Ruby  php→PHP  perl→Perl  sql→SQL  xml→XML  html→HTML  matlab→Matlab  octave→Octave  fortran→Fortran
pascal→Pascal  delphi→Delphi  ada→Ada  lisp→Lisp  prolog→Prolog  erlang→erlang  ocaml→{[Objective]Caml}  scala→Scala
swift→Swift  tex latex→TeX  make makefile→make  awk→Awk  tcl→tcl  vhdl→VHDL  verilog→Verilog
```

Code containing the substring `\end{Verbatim}` (verbatim forms) or `\end{lstlisting}` (listings) is the error
`latex-verbatim-end` (measured: fancyvrb fails on any line containing it); the block is then emitted as `writercode`
so `dump`/`check` output stays compilable. Captioned listings (`Figure { kind: Listing }`): listings mode puts the
caption in the options (`caption={<c>}` numbered, `title={<c>}` unnumbered, `label=<a>`); the other modes write
`\begingroup`, `\captionsetup{type=writerlisting}`, the caption line, the code environment, `\endgroup` (not
floating, caption above, `usage.listing_caption`). Footnotes: §4.2.

### 4.8 Math
math.md §7 owns these forms; this section restates them for T60 and adds only the preamble bookkeeping.
`dialect: Latex` → the source verbatim (math.md §7.1: passthrough, `\label`/`\tag` kept); `dialect: Typst` → the
string of `math::typst_to_latex::convert(source)` (math.md §7.2), whose diagnostics (`E-TYP-MATH-CODE`,
`W-TYP-MATH-FN-UNKNOWN`, `W-TYP-MATH-SYM-UNKNOWN`, `W-TYP-ARG-IGNORED`) are passed on with the block's span.
- Inline `\(<src>\)` (headings: §4.4). Display with `number: Some` →
  `\begin{equation}\n<src>\n\label{<a>}\n\end{equation}` (label with an id); `number: None` → label (§4.3) +
  `\[\n<src>\n\]`.
- A display source whose first non-blank token is `\begin{<env>}` with `<env>` ∈ `align`, `gather`, `multline`,
  `equation`, `eqnarray`, starred or not, is emitted bare as math.md §7.1 specifies: only the `\begin`/`\end` names
  change, to the unstarred form for `number: Some` and the starred form for `number: None` (so LaTeX numbers exactly
  the equations the IR numbers); with an id, `number: Some` inserts `\label{<a>}` directly after the `\begin{<env>}`
  token (it labels the first row), `number: None` puts the label (§4.3) on a line before the environment. `check`
  raises `math-env-label` (warning) for a labelled environment whose source contains `\\`. Other environments
  (`flalign`, `alignat`, …) are not recognised and get the `equation`/`\[` wrapper like any source.
- `usage.math` for any math; `\color`/`\textcolor` in a math string set `usage.color`. `usage.math_packages`
  (first-use order, deduplicated) gets, per control word `\name` (`\` + the maximal run of ASCII letters) of an
  emitted math string (latex source or converted), its package from `math::typst_to_latex::command_package(name)`
  (math.md §3.3, T51; the one package lookup of the LaTeX target, kept in step with
  `scripts/unicode-latex-overrides.toml`'s `[packages]` by T51's unit tests). It covers at least: `\mathscr`→`mathrsfs`;
  `\cancel \bcancel \xcancel`→`cancel`; `\coloneqq \Coloneqq \eqqcolon \mathclap \underbracket \overbracket
  \xmapsto`→`mathtools`; `\llbracket \rrbracket \mapsfrom \Mapsfrom \longmapsfrom \Mapsto \bigsqcap`→`stmaryrd`;
  `\oiint`→`esint`; `\iddots`→`mathdots`; `\degree`→`gensymb`; `\euro`→`eurosym`; `\bm`→`bm`. Each
  `char_command(c).package` of a newunicodechar math entry (§5.4) joins too. `convert` returns no package list, so
  this scan is the only source for converted math. Each package that a typst-dialect equation adds to the list raises
  one info `math-latex-package` (`added \usepackage{<p>} for \<name>`, the equation's span; math.md §7.2, §9).
- When `usage.math`, the entries of `math::latex_source::macros_from_config(&config.math.katex_options)` (math.md
  §3.6: string entries, keys with their backslash, key order) become `\providecommand{<name>}[<n>]{<body>}` lines,
  `n` = `latex_source::macro_arity(body)` and `[n]` omitted for 0 (math.md §2 rule 1); a name not matching
  `^\\[A-Za-z]+$` is skipped with `math-macro-invalid` (math.md §9's code and message shape
  `[math] katex_options.macros["<name>"]: <message>`, once per name and document; never escalated, like the
  HTML `mathml` route's and DOCX's).

### 4.9 Footnotes
`Footnote` → `\footnote{<blocks>}` in place (paragraphs separated by a blank line, footnote context). If a
`FootnoteRef` with the same `number` exists, the text starts with `\label{writer-fn-<n>}` and each `FootnoteRef` is
`\footref{writer-fn-<n>}` (kernel `\footref`; `writer-…` never collides with `resolve::anchor`). A footnote inside a
footnote stays a nested `\footnote` (compiles) + `latex-nested-footnote`.

### 4.10 References (`Inline::Ref`, after resolve)
`<a>` = `resolve::anchor(target)`, `info` = `document.resolved.labels[target]`, `E` = the `supplement` inlines when
`Some` (references.md §4.5: `Some` even when empty, as in `@eq-x[]`). `\ref` below is `\eqref` for
`LabelKind::Equation` (prints "(1)").
- `resolved: None` (serve/check) → the text `[?target]` escaped; unnumbered target (`info.number == None`) →
  `\hyperref[<a>]{<resolved.text>}`.
- `form: Page`: `E` non-empty → `<E>~\pageref{<a>}`; `E` empty → `\pageref{<a>}`; no supplement →
  `\cpageref{<a>}` with `cleveref`, else `<term(&lang, Term::Page), text-escaped>~\pageref{<a>}` ("page 3", as
  docx-output.md; `lang` as in §2.4).
- `form: Normal`: `E` non-empty → `<E>~\ref{<a>}`; `E` empty → `\ref{<a>}` alone (no tie, also with `cleveref`); no
  supplement and `cleveref = false` → `<info.supplement, text-escaped>~\ref{<a>}` ("Equation~\eqref" prints
  "Equation (1)").
- `form: Normal`, `cleveref = true`, no supplement → `\cref{<a>}`, or `\Cref{<a>}` when the Ref is the first inline
  of its paragraph, heading, caption or item, or the preceding `Text` ends with `.`, `?` or `!` and whitespace.

### 4.11 Inlines
| IR | LaTeX |
|---|---|
| `Text` | §5.1 |
| `Emph`, `Strong`, `SmallCaps`, `Superscript`, `Subscript` | `\emph{…}`, `\textbf{…}`, `\textsc{…}`, `\textsuperscript{…}`, `\textsubscript{…}` |
| `Strikeout`, `Underline` | `\sout{…}`, `\uline{…}` (ulem `[normalem]`, `usage.ulem`) |
| `Code` | `\texttt{<§5.2>}` in every code mode and context |
| `Link` to `#x` / plain text equal to `url` (or to `url` minus `mailto:`) / other | `x` in `document.resolved.labels` → `\hyperref[<resolve::anchor(x)>]{…}`, any other `#x` (`refs` included: the template places the bibliography) → the inlines only (references.md §4.9, `link-target-missing` is resolve's) / `\url{<§5.3>}` (moving arguments: §4.4) / `\href{<§5.3>}{…}`; `title` ignored |
| `Image`, `Math`, `Footnote`/`FootnoteRef`, `Ref`, `Cite` | §4.5, §4.8, §4.9, §4.10, §7.1 |
| `LineBreak` | `\\` + `\n` (a following `*` gets `{}` in front); headings, captions, terms: a space; cells: §4.6 |
| `SoftBreak`; `Span` | §4.1; label + inlines, class `subcaption` (notebook panel captions, notebook-mapping.md §9) → `{\small <inlines>}`, other classes ignored |
| `Code`, `Link`, `Image` with an id | the label (§4.3) directly before the element |
| `Raw { format: "latex" }` | text verbatim; other formats: nothing, silently (as §4.3) |

### 4.12 Notebook cells
Label first. `show_input` → the source as a code block (§4.7, language `cell.language`, attributes
`CodeAttrs::from_attrs(&cell.attrs)`, e.g. `numbered` from `code-line-numbers`, notebook-mapping.md §2.7);
`show_output` → each output's `CellOutput::pick("latex")` blocks. `cell_prompts = true` wraps the input in
`\begin{writercell}{In}{<execution_count or empty>}` … `\end{writercell}`, each `OutputKind::Result` output in
`\begin{writercell}{Out}{<n>}` … (other outputs unwrapped, R7 §8.2; wrapped content is boxed); `usage.cells`.

## 5. Escaping and Unicode — T60 (`escape.rs`)
```rust
pub fn text(s: &str, map: bool) -> Cow<'_, str>;   // §5.1 (+ §5.5 when map)
pub fn code(s: &str, map: bool) -> Cow<'_, str>;   // §5.2, \texttt argument
pub fn url(s: &str) -> Cow<'_, str>;               // §5.3
pub fn writercode(s: &str) -> Cow<'_, str>;        // §6: only \ { }
pub fn map_char(c: char) -> Option<&'static str>;  // §5.5 macro, without \ensuremath
pub fn pdflatex_safe(c: char) -> bool;             // ASCII or §5.6
```
Control words are always followed by `{}` (`\textbackslash{}`), never by a space.

### 5.1 Text (pandoc's `stringToLaTeX` TextString table, R7 §2.2, with R7 §10's variants)
R7 §10's variants: `^`→`\textasciicircum{}` where pandoc 3.12.1 writes `\^{}` (measured), and `{}` after every
control word.
`\`→`\textbackslash{}` · `{ }`→`\{ \}` · `$ % & _ #`→`\$ \% \& \_ \#` · `~`→`\textasciitilde{}` ·
`^`→`\textasciicircum{}` · `|`→`\textbar{}` · `<`→`\textless{}` · `>`→`\textgreater{}` · `[`→`{[}` ·
`]`→`{]}` · `-` followed by `-`→`-\/` · `?`/`!` followed by `` ` ``→`?{\kern0pt}`/`!{\kern0pt}` ·
U+00A0→`~` · U+00AD→`\-` · U+200B→`\hspace{0pt}` · U+202F→`\,` · U+2026→`\ldots{}` · U+2018→`` ` `` ·
U+2019→`'` · U+201C→` `` ` · U+201D→`''` (`\,` between two adjacent mapped quotes) · U+2014→`---` ·
U+2013→`--` · `"`, `'`, `` ` `` unchanged · everything else: §5.4.

### 5.2 Code (`\texttt`)
The §5.1 specials `\ { } $ % & _ # ~ ^ | < > [ ]` and `-\/`, plus `'`→`\textquotesingle{}`,
`` ` ``→`\textasciigrave{}`, each space → `\ ` (runs survive); no quote, dash or ellipsis mapping. Also used
for each line of a footnote-context code block (§4.2).

### 5.3 URLs, labels, keys, verbatim
- URLs (`\href`/`\url` argument; pandoc's URLString table extended by writer: pandoc 3.12.1 turns `\` into `/` and
  keeps non-ASCII, measured): non-ASCII characters → their UTF-8 bytes as `%XX` (uppercase), then
  `%`→`\%`, `#`→`\#`, `\`→`\%5C`, `{`→`\%7B`, `}`→`\%7D`, `^`→`\%5E`, `|`→`\%7C`, `[`→`\%5B`, `]`→`\%5D`,
  `` ` ``→`\%60`, space→`\%20`, `"`→`\%22`, `<`→`\%3C`, `>`→`\%3E`; all else (`_ & ~ ? = : / . -`) unchanged.
- Labels are `resolve::anchor` output (`[A-Za-z0-9_]`) and never escaped; cite keys are unescaped (§7.1 lint).
- `Verbatim`/`lstlisting` content, math sources and raw latex: untouched (§4.7's `latex-verbatim-end`).

### 5.4 Unicode policy
`map` is effective for `unicode = "map"`, or `unicode = "auto"` with `engine = "pdflatex"`.
- Text and `\texttt` contexts (and the template's escaped variables, §2.2): with `map`, a §5.5 character becomes
  `\ensuremath{<macro>}` (`usage.math`).
- Verbatim environments (`Verbatim`, `writercode`, `lstlisting`) keep UTF-8; with `map`, each §5.5 character found
  there joins `usage.newunicodechar` with its §5.5 macro.
- Math sources keep UTF-8 (math.md §7.1). With `map`, every distinct non-ASCII character `c` of a latex-dialect
  math source joins `usage.newunicodechar` with `typst_to_latex::char_command(c).tex`, whether or not `c` is in
  §5.5 or §5.6 (a §5.6 character in math is no error but prints wrong: `\(a → b\)` gave "aßb" with T1); its
  `package` joins `usage.math_packages` (§4.8). A character without a `char_command` entry raises
  `math-conversion-failed` (math.md §9 message, target `latex`, once per character and document) naming it and is
  not added. Converted typst math is ASCII except graphemes without a mapping (already `W-TYP-MATH-SYM-UNKNOWN`,
  math.md §3.3); those are linted like text below.
- `usage.newunicodechar` holds one entry per character in first-use order (abstract, then body); when a character
  occurs both in math and in a verbatim environment, the math macro is kept (at the first use's position). The
  preamble declares the entries under `\ifPDFTeX` (verified in `Verbatim` and `\[…\]`); with `keep` nothing is
  added.
- Lint `latex-unicode`: one diagnostic per document listing the distinct offending code points in first-occurrence
  order as `U+2265 (≥)` (at most 10, then `and N more`), span of the first; raw latex is not scanned. Offending =
  a character of text, `\texttt`, verbatim or converted-math context that is not `pdflatex_safe` and not handled by
  `map`, and under `keep` also a non-`pdflatex_safe` character of a latex-dialect math source (under `map` those are
  math.md's, above); under pdflatex also every non-ASCII character in a `lstlisting` (listings with inputenc cannot
  read them, even with `\newunicodechar`: "Invalid UTF-8 byte sequence", fatal). Level: **warning** for
  `engine = "pdflatex"` (pdflatex stops at the first one); **info** for xelatex/lualatex (they drop glyphs the
  template's fonts lack; the user greps `Missing character` in the log).

### 5.5 Mapping table (134 entries; each macro verified in text, `\texttt`, `Verbatim` and math, pdflatex)
```
0391 Α \mathrm{A} · 0392 Β \mathrm{B} · 0393 Γ \Gamma · 0394 Δ \Delta · 0395 Ε \mathrm{E} · 0396 Ζ \mathrm{Z} · 0397 Η \mathrm{H} · 0398 Θ \Theta
0399 Ι \mathrm{I} · 039A Κ \mathrm{K} · 039B Λ \Lambda · 039C Μ \mathrm{M} · 039D Ν \mathrm{N} · 039E Ξ \Xi · 039F Ο \mathrm{O} · 03A0 Π \Pi · 03A1 Ρ \mathrm{P}
03A3 Σ \Sigma · 03A4 Τ \mathrm{T} · 03A5 Υ \Upsilon · 03A6 Φ \Phi · 03A7 Χ \mathrm{X} · 03A8 Ψ \Psi · 03A9 Ω \Omega · 03B1 α \alpha · 03B2 β \beta · 03B3 γ \gamma
03B4 δ \delta · 03B5 ε \varepsilon · 03B6 ζ \zeta · 03B7 η \eta · 03B8 θ \theta · 03B9 ι \iota · 03BA κ \kappa · 03BB λ \lambda · 03BC μ \mu · 03BD ν \nu
03BE ξ \xi · 03BF ο o · 03C0 π \pi · 03C1 ρ \rho · 03C2 ς \varsigma · 03C3 σ \sigma · 03C4 τ \tau · 03C5 υ \upsilon · 03C6 φ \varphi · 03C7 χ \chi · 03C8 ψ \psi
03C9 ω \omega · 03D1 ϑ \vartheta · 03D5 ϕ \phi · 03D6 ϖ \varpi · 03F0 ϰ \varkappa · 03F1 ϱ \varrho · 03F5 ϵ \epsilon · 2032 ′ {}^{\prime}
2033 ″ {}^{\prime\prime} · 2034 ‴ {}^{\prime\prime\prime} · 2102 ℂ \mathbb{C} · 210F ℏ \hbar · 2113 ℓ \ell · 2115 ℕ \mathbb{N} · 2118 ℘ \wp · 211A ℚ \mathbb{Q}
211D ℝ \mathbb{R} · 2124 ℤ \mathbb{Z} · 2135 ℵ \aleph · 2153 ⅓ \tfrac{1}{3} · 2154 ⅔ \tfrac{2}{3} · 215B ⅛ \tfrac{1}{8} · 2194 ↔ \leftrightarrow
2195 ↕ \updownarrow · 21A6 ↦ \mapsto · 21D0 ⇐ \Leftarrow · 21D1 ⇑ \Uparrow · 21D2 ⇒ \Rightarrow · 21D3 ⇓ \Downarrow · 21D4 ⇔ \Leftrightarrow · 2200 ∀ \forall
2202 ∂ \partial · 2203 ∃ \exists · 2204 ∄ \nexists · 2205 ∅ \emptyset · 2207 ∇ \nabla · 2208 ∈ \in · 2209 ∉ \notin · 220B ∋ \ni · 220F ∏ \prod · 2211 ∑ \sum
2212 − - · 2213 ∓ \mp · 2217 ∗ \ast · 2218 ∘ \circ · 2219 ∙ \bullet · 221A √ \surd · 221D ∝ \propto · 221E ∞ \infty · 2220 ∠ \angle · 2223 ∣ \mid
2225 ∥ \parallel · 2227 ∧ \wedge · 2228 ∨ \vee · 2229 ∩ \cap · 222A ∪ \cup · 222B ∫ \int · 222E ∮ \oint · 2234 ∴ \therefore · 223C ∼ \sim · 2243 ≃ \simeq
2245 ≅ \cong · 2248 ≈ \approx · 2260 ≠ \neq · 2261 ≡ \equiv · 2264 ≤ \leq · 2265 ≥ \geq · 226A ≪ \ll · 226B ≫ \gg · 2282 ⊂ \subset · 2283 ⊃ \supset
2286 ⊆ \subseteq · 2287 ⊇ \supseteq · 2295 ⊕ \oplus · 2297 ⊗ \otimes · 22A5 ⊥ \perp · 22C5 ⋅ \cdot · 22EF ⋯ \cdots · 2308 ⌈ \lceil · 2309 ⌉ \rceil
230A ⌊ \lfloor · 230B ⌋ \rfloor · 25A1 □ \square · 2713 ✓ \checkmark · 27F6 ⟶ \longrightarrow · 27F9 ⟹ \Longrightarrow
```
No entry is in §5.6; emoji and CJK are never mapped.

### 5.6 pdflatex-safe set (`PDFLATEX_SAFE`)
ASCII plus the 349 code points that T1 + `[utf8]{inputenc}` + textcomp declare in TeX Live 2026 (found by testing
`\u8:<char>` over U+00A0–U+33FF, U+FB00–U+FFFF and U+1F600; all 349 then typeset without error):
```
00A0-0125 0128-0137 0139-013E 0141-0148 014A-0165 0168-017E 0192 01C4-01D4 01E2-01E3 01E6-01EB 01F0 01F4-01F5 0218-021B 0232-0233 0237 02C6-02C7
02D8-02D9 02DB-02DD 0E3F 1E02-1E03 1E0D 1E1E-1E21 1E25 1E30-1E31 1E37 1E43 1E45 1E47 1E5B 1E63 1E6D 1E8E-1E91 1E9E 1EF2-1EF3 200C 2010-2016 2018-201A
201C-201E 2020-2022 2026 2030-2031 2039-203B 203D 2044 204E 2052 20A1 20A4 20A6 20A9 20AB-20AC 20B1 2103 2116-2117 211E 2120 2122 2126-2127 212E
2190-2193 2329-232A 2422-2423 25E6 25EF 266A 27E8-27E9 3008-3009 FB00-FB06 FEFF
```
(`→ × ± ½ € ° µ ⟨ ⟩` are safe; `− ≥ α √` are not.)

## 6. Highlighting — T60
One highlighter for every output (html-output.md §1.5 and its Deviations): the syntax is
`crate::html::highlight::resolve_syntax(lang.as_deref(), attrs.plain)` (§4.7's `lang` and `CodeAttrs`) over
`html::highlight::SYNTAXES` (T13; aliases and plain languages included); `SyntaxChoice::Plain` and `Unknown` → the
`verbatim` form (no `code-language-unknown` in LaTeX). The block's lines (html-output.md §1.5: one trailing `\n`
dropped, `\r\n` read as `\n`) are tokenized as `line + "\n"` (`syntect::util::LinesWithEndings`; the `_newlines`
syntaxes need the `\n`) with one `ParseState` and one `ScopeStack` carried across lines; a `parse_line` error
anywhere → the whole block in the `verbatim` form. A token's class is found by walking its scope stack from the innermost scope outward: the first
scope that starts with an entry below (dot-boundary prefix), entries tried in table order, decides; no match →
plain. Adjacent tokens of one class merge into one `\Wr<Class>{…}`; plain and whitespace-only tokens are written
bare; text goes through `escape::writercode`. Before classing, the line's trailing `\n` is cut from its last token
and written bare after the last `}`, so no `\Wr…{…}` argument contains a line break (measured: a python `# c`
comment's last token is `" c\n"`; `\WrComment{# c⏎}y = 2` compiles, but fancyvrb then prints `# cy = 2` on one
line).

| order | scope prefixes | macro | preamble default `\providecommand{\WrX}[1]{…}` |
|---|---|---|---|
| 1 | `comment` | `\WrComment` | `\textcolor[HTML]{969896}{\textit{#1}}` |
| 2 | `string` | `\WrString` | `\textcolor[HTML]{183691}{#1}` |
| 3 | `constant.numeric` | `\WrNumber` | `\textcolor[HTML]{0086B3}{#1}` |
| 4 | `constant` | `\WrConstant` | `\textcolor[HTML]{0086B3}{#1}` |
| 5 | `keyword.operator` | `\WrOperator` | `\textcolor[HTML]{A71D5D}{\textbf{#1}}` |
| 6 | `meta.preprocessor`, `keyword.control.import`, `keyword.control.directive` | `\WrPreproc` | `\textcolor[HTML]{323232}{#1}` |
| 7 | `entity.other.attribute-name`, `variable.annotation`, `meta.annotation` | `\WrAttribute` | `\textcolor[HTML]{795DA3}{#1}` |
| 8 | `keyword`, `storage.modifier` | `\WrKeyword` | `\textcolor[HTML]{A71D5D}{\textbf{#1}}` |
| 9 | `storage.type`, `entity.name.type`, `entity.name.class`, `entity.name.struct`, `entity.name.enum`, `entity.other.inherited-class`, `support.type`, `support.class` | `\WrType` | `\textcolor[HTML]{A71D5D}{\textbf{#1}}` |
| 10 | `entity.name.function`, `support.function`, `support.macro` | `\WrFunction` | `\textcolor[HTML]{795DA3}{\textbf{#1}}` |
| 11 | `variable` | `\WrVariable` | `\textcolor[HTML]{323232}{#1}` |
| 12 | `invalid` | `\WrError` | `\textcolor[HTML]{B52A1D}{\textbf{#1}}` |

Colours = the code theme's: two-face `EmbeddedThemeName::InspiredGithub`, foreground and bold/italic of
`Highlighter::new(theme).style_for_stack(&[Scope::new(rep).expect("static scope")])` (after `let themes =
two_face::theme::extra(); let theme = themes.get(EmbeddedThemeName::InspiredGithub);`) for `rep` in `comment
string constant.numeric constant.language keyword.operator meta.preprocessor entity.other.attribute-name keyword
storage.type entity.name.function variable invalid` (a T60 unit test recomputes them). Fixed, because config.md has no LaTeX code-theme key and LaTeX must not
depend on the HTML theme chain. `[HTML]`, because `\textcolor[rgb]{…}` typed into a `commandchars` Verbatim fails
("Illegal unit of measure", R7 §6.1); the body never contains `\textcolor`.

## 7. Citations and bibliography — T60 (§7.1–7.2, §7.3's names), T61 (§7.3)

### 7.1 Commands (`Inline::Cite`; R5 §7 adapted)
Per item: `pre` = the prefix inlines (LaTeX); `post` = the locator text-escaped with ASCII spaces → `~`, followed by
`, ` + the suffix inlines when there is a suffix. Optional arguments of a biblatex/natbib command: none → nothing;
`post` only → `[post]`; with `pre` → `[pre][post]` (`post` may be empty). A group "has notes" when some item has
`pre` or `post`; without notes the keys are joined by `,` in one command.

| `mode` | biblatex | natbib | plain |
|---|---|---|---|
| `normal` | `\autocite[..]{k}`; group with notes `\autocites[..]{a}[..]{b}` | `\citep[..]{k}`; group with notes `\citetext{\citealp[..]{a}; \citealp[..]{b}}` | `\cite[post]{k}` with `pre` as text before it; group with notes: these joined by `; ` |
| `prose` | `\textcite`; group with notes `\textcites` | `\citet`; group with notes `\citet[..]{a}; \citet[..]{b}` | as `normal` + `unsupported-construct` |
| `year` | `\autocite*`; group with notes: one per item joined by `; ` | `\citeyearpar` | as `normal` + `unsupported-construct` |
| `author` | `\citeauthor[..]{…}` | `\citeauthor{…}` (notes dropped + `unsupported-construct`) | as `normal` + `unsupported-construct` |
| `full` | `\fullcite[..]{…}` | `\citep` form + `unsupported-construct` | as `normal` + `unsupported-construct` |
| `none` | `\nocite{a,b}` | `\nocite{a,b}` | `\nocite{a,b}` |

A group with notes whose cell names no group form gets one command per item, joined by `; `. Verified: `\textcite`,
`\autocites[p.~12]{a}{b}`, `\autocite*`, `\citeauthor`, `\fullcite`, `\nocite`, `\citet`, `\citeyearpar`,
`\citetext{\citealp[see][p.~12]{a}; \citealp{b}}`, `\cite[p.~12]{k}`. `unsupported-construct`: once per document and
mode. `Cite.resolved` and `ResolvedCite.note` are ignored (`\autocite` follows note styles). Keys are unescaped; a key
with whitespace or one of `, { } % # \ ~ "` is the error `latex-cite-key` (typst frontend only).

### 7.2 `Block::Bibliography`
`emit_bibliography = false`: nothing (the template prints it, §9.1). `true`: biblatex `\printbibliography`; natbib
`\bibliographystyle{plainnat}`, plain `\bibliographystyle{plain}`, each followed by a line `\bibliography{<names>}`
(`<names>` = `<stem>_assets/<name>` without its `.bib` for each `Emitted.bibliography` entry, joined by `,`). No
bibliography file, or no `Inline::Cite` in the document (`!document.features.citations`; references.md §4.9: such
a block renders nothing in every output, while natbib and plain would print an empty "References") → nothing.

### 7.3 Files (`render` plans, `materialise` copies)
Each `[references] bibliography` path (absolute after config loading): `.yaml`/`.yml` (any case) → error
`latex-bib-yaml` (T61, "hayagriva YAML cannot feed BibTeX or biber; convert it to .bib"), not copied; any other file
is BibTeX. T60 names those in config order into `Emitted.bibliography`: the file name with each character outside
`[A-Za-z0-9._-]` replaced by `-` (typst-dialect.md §10's rule), plus `.bib` unless it already ends in `.bib`
(BibTeX appends `.bib` to `\bibliography` names), a name already taken by an image (§1.2) or an earlier
bibliography getting `-2`, `-3`, … before `.bib` (`refs.bib`, `refs-2.bib`). T61 turns each entry into
`PlannedFile { name, source: FileSource::Bib(path) }`, copied byte for byte as `<stem>_assets/<name>`, and into the
`bibliography` variable (§2.2). The template loads them (`\addbibresource`, `\bibliography`).

## 8. Diagnostics (T60 and T61 raise them; T71 lists them under `check --target latex`)
| code | level | raised when |
|---|---|---|
| `latex-unicode` | warning (pdflatex), info (xelatex, lualatex) | §5.4 |
| `latex-template-slot-missing`, `latex-verbatim-end`, `latex-cite-key`, `latex-bib-yaml` | error | §2.3, §4.7, §7.1, §7.3 |
| `latex-rowspan` (once per table), `latex-nested-footnote`, `latex-listings-language` (once per language) | warning | §4.6, §4.9, §4.7 |
| `raw-skipped` | info, `check --target latex` only (T71, markdown-dialect.md §10) | a `Raw` block or inline of another format (§4.3, §4.11); `build` drops it silently |
| `unsupported-construct` | warning (error with `--strict`) | deep lists §4.3, images §4.5, cite modes §7.1 |
| `math-macro-invalid` | warning (owner math.md §9) | a `[math] katex_options.macros` name not matching `^\\[A-Za-z]+$` (§4.8) |
| `math-conversion-failed` | warning (error with `--strict`) | under `map`, a non-ASCII character of a latex-dialect math source without `char_command` (§5.4, math.md §7.1) |
| `math-env-label` | warning (`check` only) | a labelled bare display environment containing `\\` (§4.8, math.md §7.1) |
| `E-TYP-MATH-CODE` | warning; error with `[typst] strict = true` or `--strict` | passed on from `typst_to_latex::convert` (§4.8) |
| `W-TYP-MATH-FN-UNKNOWN`, `W-TYP-MATH-SYM-UNKNOWN`, `W-TYP-ARG-IGNORED` | warning; error with `[typst] strict = true` or `--strict` (the pipeline's escalation step, config.md §4) | passed on from `typst_to_latex::convert` (§4.8) |
| `math-latex-package` | info, never escalated | a package added for a typst-dialect equation (§4.8, math.md §7.2, §9) |

Each diagnostic carries the block's `span` and file (`SourceSpan.file`, else the source path). Errors make `build`
skip the latex target (exit 1); `dump --latex` still prints the `.tex`, except after `latex-template-slot-missing`,
where there is none (§2.3).

## 9. Sample template and the latexmk protocol — T61

### 9.1 `assets/latex/writer-template.tex` (exact; `writer eject latex-template` writes it)
```latex
\#{ writer sample template. backslash-VAR prints, backslash-BLOCK runs a statement, backslash-hash comments; a comment ends at its first closing brace. }
\#{ Order: class and packages, the writer hooks you restyle, the preamble slot, then whatever needs the slot's packages. }
\documentclass[11pt,a4paper]{article}
\usepackage[margin=2.5cm]{geometry}
\usepackage{parskip}
\usepackage{iftex}
\ifPDFTeX
  \usepackage[T1]{fontenc}
  \usepackage[utf8]{inputenc}
  \usepackage{lmodern}
\else
  \usepackage{fontspec}
\fi
\usepackage{xcolor}
\usepackage{listings}
\lstset{basicstyle=\ttfamily\small,breaklines=true,showstringspaces=false,frame=single}
\colorlet{writercalloutnote}{blue!60!black}\colorlet{writercallouttip}{green!50!black}\colorlet{writercalloutimportant}{violet}
\colorlet{writercalloutwarning}{orange!80!black}\colorlet{writercalloutcaution}{red!70!black}
\makeatletter
\newcommand{\writercalloutcolor}[1]{%
  \@ifundefinedcolor{writercallout#1}{\colorlet{writercalloutcurrent}{writercalloutnote}}{\colorlet{writercalloutcurrent}{writercallout#1}}}
\newif\ifwritertcolorbox
\IfFileExists{tcolorbox.sty}{\writertcolorboxtrue}{\writertcolorboxfalse}
\ifwritertcolorbox
  \usepackage{tcolorbox}
  \tcbuselibrary{skins,breakable}
  \newenvironment{writercallout}[2]{\writercalloutcolor{#1}%
    \begin{tcolorbox}[enhanced,breakable,colframe=writercalloutcurrent,colback=writercalloutcurrent!5!white,
      coltitle=white,fonttitle=\bfseries,title={#2}]}{\end{tcolorbox}}
\else
  \usepackage{framed}
  \newenvironment{writercallout}[2]{\writercalloutcolor{#1}%
    \def\FrameCommand{{\color{writercalloutcurrent}\vrule width 3pt}\hspace{6pt}}%
    \MakeFramed{\advance\hsize-\width\FrameRestore}\noindent\textbf{#2}\par\nobreak}{\endMakeFramed}
\fi
\makeatother
\newenvironment{writercell}[2]{\par\noindent{\ttfamily\color{gray}#1\,[#2]:}\par\nobreak}{\par}
\VAR{preamble}
\BLOCK{if has_code}\fvset{frame=lines,framesep=2mm,fontsize=\small}\BLOCK{endif}
\hypersetup{colorlinks=true,linkcolor=blue!60!black,citecolor=green!40!black,urlcolor=blue!60!black,pdftitle={\VAR{title}},pdflang={\VAR{lang}}}
\BLOCK{if cite == "biblatex"}
\usepackage[backend=\VAR{vars.biblatex_backend | default("biber")},style=authoryear]{biblatex}
\BLOCK{for b in bibliography}
\addbibresource{\VAR{b}}
\BLOCK{endfor}
\BLOCK{elif cite == "natbib"}
\usepackage[round]{natbib}
\BLOCK{endif}

\title{\VAR{title}\BLOCK{if subtitle}\\[0.5em]\large \VAR{subtitle}\BLOCK{endif}}
\author{\BLOCK{for a in authors}\VAR{a.name}\BLOCK{if a.affiliation}\\\small \VAR{a.affiliation}\BLOCK{endif}\BLOCK{if not loop.last} \and \BLOCK{endif}\BLOCK{endfor}}
\date{\VAR{date}}

\begin{document}
\BLOCK{if title}\maketitle\BLOCK{endif}
\BLOCK{if abstract}
\begin{abstract}
\VAR{abstract}
\end{abstract}
\BLOCK{endif}
\BLOCK{if keywords}\noindent\textbf{Keywords:} \VAR{keywords | join(", ")}\par\BLOCK{endif}
\BLOCK{if toc}\tableofcontents\BLOCK{endif}

\VAR{body}

\BLOCK{if bibliography and has_citations and not emit_bibliography}
\BLOCK{if cite == "biblatex"}
\printbibliography
\BLOCK{else}
\bibliographystyle{\BLOCK{if cite == "natbib"}plainnat\BLOCK{else}plain\BLOCK{endif}}
\bibliography{\BLOCK{for b in bibliography}\VAR{b[:-4]}\BLOCK{if not loop.last},\BLOCK{endif}\BLOCK{endfor}}
\BLOCK{endif}
\BLOCK{endif}
\end{document}
```
biblatex's standard backend is biber; front matter `biblatex_backend: bibtex` (a `vars` entry) selects bibtex. An
empty `\date{}` stops LaTeX printing today's date. The framed branch was forced once and compiled.

### 9.2 latexmk protocol (`tests/latex_compile.rs`)
Engines: `pdflatex` (latexmk flag `-pdf`) and `xelatex` (flag `-xelatex`). Without `latexmk` or the engine on
`PATH` a case prints `skipping: latexmk/<engine> not found` and passes. Per engine and case, the case's `[latex]`
keys plus `engine = "<engine>"` are written to `<tmp>/<case>-<engine>.toml`; then `writer --config
<tmp>/<case>-<engine>.toml build <input> --to latex -o <tmp>/<case>-<engine>.tex` (plus `--latex-template
<fixture>` where the table names one), and in `<tmp>` `latexmk <flag> -interaction=nonstopmode -halt-on-error
-outdir=out-<case>-<engine> <case>-<engine>.tex`. Every case asserts: exit 0; no log line starting with `!`;
pdflatex logs without `Missing character`; in xelatex logs every code point a `Missing character` line names is
listed by the build's `latex-unicode` info (with `engine = "xelatex"` the policy is `keep`, so the info lists every
non-`pdflatex_safe` character); the text from `gs -q -sDEVICE=txtwrite -o - <pdf>` (no pdftotext on the reference
machine) contains no `??`. The `tour.md` cases (`sample-*`, `minimal`, `byo`) also assert `Table 1` and `Listing 1`
in that text.

| case | input and config |
|---|---|
| `sample-biblatex` | `tests/fixtures/latex/tour.md`, built-in template, `cite = "biblatex"` |
| `sample-natbib` | same, `cite = "natbib"`, `code = "listings"`, `cleveref = true`; xelatex only (pdflatex cannot read tour.md's non-ASCII code inside `lstlisting`, §5.4) |
| `sample-highlighted` | same, `code = "highlighted"`, `tables = "tabular"`, `booktabs = false`, `unicode = "keep"`; xelatex only |
| `minimal` | `tour.md`, `--latex-template tests/fixtures/latex/template.tex` (the generated preamble alone, fallback hooks) |
| `byo` | `tour.md`, `--latex-template tests/fixtures/latex/byo-template.tex`, default `code` (its `\renewcommand{\WrKeyword}` meets the always-defined `\Wr*` macros without highlighted code); also asserts `Test venue` in the text |
| `empty` | `<tmp>/empty.md` holding one paragraph, `--latex-template tests/fixtures/latex/template.tex` (the preamble without conditional lines) |
| `notebook` | `tests/fixtures/ipynb/outputs.ipynb`, `cell_prompts = true` |

biber is absent on the reference machine (R7 §1), so `tour.md` sets `biblatex_backend: bibtex` (expect `Package
biblatex Warning: Using fall-back bibtex backend`); with `biber` on `PATH` `sample-biblatex` also runs without that
key. ≈ 5–10 s per run. Also: `missing-slot.tex` → `latex-template-slot-missing`, nothing written, exit 1;
`missing_slots(SAMPLE)` is empty; two builds are byte-identical. T72's CLI test (it wires `eject`) asserts that
`writer eject latex-template <file>` writes `SAMPLE` byte for byte.

## Fixtures
| fixture | owner | use |
|---|---|---|
| `tests/fixtures/latex/tour.md`, `tests/fixtures/latex/verbatim-end.md`, `tests/fixtures/latex/images/{plot.png,diagram.svg,anim.gif}` | T60 | every §4 row reachable from Markdown: numbered and `.unnumbered` headings (one with a footnote, one with math), implicit and `:::figure` figures (sized, unsized, SVG, GIF), a captioned and a bare table, the five callout kinds and a custom one holding a figure and a table, `:::toc`, code for each mode (python, rust, `.numbered start=3`, `caption=`), inline, display, labelled and `align` math, `≥ α` in text, inline code, a fenced python block and display math, repeated and nested footnotes, refs of every kind and form, every cite mode with `tests/fixtures/bib/refs.bib` keys, raw latex and html, divs `center`, `small`, `env-quote`; front matter `biblatex_backend: bibtex`, `toc: 2`, `numbering: {headings: true}`, `venue: Test venue`. `verbatim-end.md`: a fence containing `\end{Verbatim}` |
| `tests/fixtures/latex/template.tex` | T61 | exactly `\documentclass{article}\VAR{preamble}\begin{document}\VAR{body}\end{document}` + `\n` (cases `minimal`, `empty`) |
| `tests/fixtures/latex/byo-template.tex` | T61 | BYO template: its own `writercallout` before the slot, `\renewcommand{\WrKeyword}[1]{\textbf{#1}}` after it, `\VAR{vars.venue \| default("") \| tex}` in the body (case `byo`) |
| `tests/fixtures/latex/missing-slot.tex` | T61 | no `\VAR{preamble}` |
| `examples/latex-template/` | T61 | guide example: a document, a template derived from the sample, the `latexmk` line |
| reused | T01, T18, T50 | `md/*.md`, `bib/refs.bib`, `ipynb/outputs.ipynb`, `typ/math.typ` (typst math through the converter) |

Snapshots (`insta`): T60's `tests/latex_snapshots.rs` covers `Emitted.body` + diagnostics of every `md/*.md`,
`latex/tour.md` per code and cite mode, `outputs.ipynb` with and without prompts, `typ/math.typ`; unit tests build IR
for widths, spans, sub-figures, footnote code, `\Cref`, every §5 row, §5.5 and `PDFLATEX_SAFE` bounds. T61 snapshots
`preamble()` (empty, all-true and tour `Usage`) and the tour's full `.tex` with the sample template.

## Verified API hints
- minijinja 2.24.0: the §2.1 calls exist as written (end markers may be shared, start markers must differ);
  `Template::undeclared_variables(false) -> HashSet<String>` reports for §9.1
  `abstract authors bibliography body cite date emit_bibliography has_citations has_code keywords lang preamble
  subtitle title toc vars` (re-run on the sample above). `SyntaxConfigBuilder::build()` returns
  `Result<SyntaxConfig, Error>` (hence §2.1's `expect`).
- syntect 5.3 + two-face 0.5.2: `two_face::theme::extra()` returns an `EmbeddedLazyThemeSet` by value and its
  `get(EmbeddedThemeName::InspiredGithub) -> &Theme` borrows it, so bind it first (`let themes = …; let theme =
  themes.get(…);`); `Highlighter::new(theme).style_for_stack(&[Scope])`; `Scope::new(&str) -> Result<Scope,
  ParseScopeError>`; `ParseState::new(syntax).parse_line(line, &SYNTAXES) -> Result<Vec<(usize, ScopeStackOp)>,
  ParsingError>`, `ScopeStack::apply(&op)`, `Scope::build_string()`, `syntect::util::LinesWithEndings`. Measured
  scopes: python
  `import`→`keyword.control.import`, `def`→`keyword.declaration.function`, `@x`→`variable.annotation`, `# c` → `"#"`
  (`punctuation.definition.comment` inside `comment.line.number-sign`) and `" c\n"`; rust `fn`→`storage.type`,
  `println!`→`support.macro`.
- TeX Live 2026: `\footref` is in the kernel; `\DeclareCaptionType` is caption's (via newfloat);
  `\captionsetup{type=…}` + `\caption` avoids the hypcap warning of `\captionof`;
  `\AtBeginDocument{\@ifundefined{…}{\newenvironment{…}[2]{…#2…}}{}}` works with single `#`. Text checks:
  `gs -q -sDEVICE=txtwrite -o - main.pdf`.

## Open points (local choices)
- T60: the representation of breakable spaces (§4.1); filling `Usage` during emission or by a scan; message wording.
- T61: whether `render` builds the `Environment` per call or caches it (no observable difference).

## Deviations from PLAN.md and the brief
- PLAN §5 puts "the generated preamble" and the asset directory under T60, §3.2 gives `preamble.rs`/`assets.rs` to
  T61: this spec follows §3.2 (T60 records `Usage` and `AssetFile`s and names files; T61 renders and writes them).
- PLAN §5 lists `eject latex-template` under T61, while PLAN §3.2 gives `src/cli/eject.rs` to T72 ("wiring of the
  others"): T61 provides `template::SAMPLE` and tests it (§9.2), T72 wires the command and tests its output.
- Template variables beyond the brief's list: `emit_bibliography` (so a template does not print the bibliography
  twice), `has_citations` (references.md §4.9), `has_highlighted` (guards a `writercode` override, §2.4); `toc` is
  the lifted depth (integer, 0 = none) rather than a bool.
- The brief calls URLs "unescaped; linted": they get pandoc's URLString escaping extended by writer (§5.3: `\` →
  `\%5C` instead of pandoc's `/`, non-ASCII percent-encoded instead of kept) because hyperref cannot read a raw `#`
  or `%` inside footnotes and captions. Labels are safe by construction (`resolve::anchor`); cite keys stay unescaped
  and are linted by `latex-cite-key`, a code the brief's list lacks.
- The preamble always loads `iftex`, the encoding/fontspec block and hyperref (one `.tex` for every engine;
  templates `\hypersetup` after the slot), loads `cleveref` whenever `[latex] cleveref = true` and `listings` when
  the body uses it (R7 left both to templates) so `[latex] cleveref` and `code` work with any template.
- New fixtures beyond PLAN §7 (which lists only T61's `latex/template.tex`): T60 owns `latex/{tour,verbatim-end}.md`
  and `latex/images/*`; T61 owns `latex/{missing-slot,byo-template}.tex`.
- Contract notes against math.md (coordinator): (1) math.md §8 drops the labels of unnumbered equations; this spec
  keeps `\phantomsection\label{<a>}` (§4.8) because references.md §4.5 lets `@eq-x` reach an unnumbered target
  (`unnumbered-target`) and §4.10 links it with `\hyperref`; math.md §8's "labels dropped" should read "labels kept
  as `\phantomsection\label`" (done in math.md). (2) `typst_to_latex::convert` returns no package list, so §4.8 scans
  control words through math.md §3.3's `command_package` (T51 owns the table and its two consistency tests; the
  earlier `MATH_PACKAGES` copy in `emit.rs` is gone — contract change, coordinator, 2026-10-25). (3) math.md
  §7.1's bare-environment list lacks `flalign(*)` and `alignat(*)`, which this spec therefore wraps like any source
  (LaTeX then fails on the nesting); adding them there (the `\label` after `alignat`'s `{n}` argument) would let
  §4.8 recognise them.
