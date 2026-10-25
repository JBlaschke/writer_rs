# R7 — LaTeX source export with a bring-your-own template

Research notes for the `writer` plan. Everything marked **[verified]** was read in crate sources under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`, produced by a program or compiler run on this machine
(2026-10-24, macOS, pandoc 3.12.1, TeX Live 20260301 via Homebrew, latexmk 4.87, Rust 1.99.1), or read in
upstream source fetched from GitHub. Everything else is marked **[design]** (a recommendation) or
**[unverified]**.

Scratch artifacts (session scratchpad, copy what the tasks need into `tests/fixtures/`):

- `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/latex-probe/r7/` — hand-written `body.tex`, generated `writer-preamble.tex`, sample templates, rendered `main-*.tex`, six `out-*` build dirs with logs and PDFs, `pages/*.png` renders, `pandoc/` (pandoc inputs and outputs), `figures/` (PNG, SVG, svg2pdf PDF).
- `.../scratchpad/build/svg2pdf-probe/` (svg2pdf 0.13.0), `.../mj-probe/` (minijinja 2.24.0), `.../mj2-probe/` (minijinja 3.0.3), `.../unicode-probe/r7/`, `.../code-probe/r7/`, `.../nb-probe/r7/` (nbconvert run).
- Compiled binaries: `.../scratchpad/build/target/release/{svg2pdf-probe,mj-probe,mj2-probe}`.

---

## 0. TL;DR (the decisions this note supports)

1. **Template engine: minijinja with LaTeX-safe delimiters** `\VAR{…}`, `\BLOCK{…}`, `\#{…}` (the classic Jinja-LaTeX
   convention). **[verified]** on both minijinja 2.24.0 (`features = ["custom_syntax"]`) and 3.0.3 (custom syntax is
   built in, no feature). Same template renders byte-identically on both. Pandoc's `$var$` syntax has no Rust
   implementation and is a worse fit (`$` is math in LaTeX); reject it.
2. **Writer emits a body and a generated preamble**; the template owns everything else. The body only uses standard
   packages (graphicx, longtable, booktabs, array, caption, amsmath, amssymb, xcolor, fancyvrb, ulem, hyperref) plus
   **four writer-named hooks** a template may define: `writercallout` (env, 2 args), `writercell` (env, 2 args),
   `\writerpagebreak`, `\writerhrule`, and the `\Wr*` token macros of highlighted code. The generated preamble provides
   defaults for all of them (`\providecommand` for macros, `\AtBeginDocument{\@ifundefined{…}{\newenvironment…}{}}` for
   environments) so a template that defines nothing still compiles **[verified: `template-minimal.tex` compiled]**.
3. **Code: `fancyvrb` `Verbatim` by default** (safe on every engine), `writercode` = `Verbatim` with
   `commandchars=\\\{\}` + token-class macros `\WrKeyword{…}` etc. for the opt-in syntect-highlighted form, `listings`
   as a third option (no Rust/JS/TS/JSON/YAML/TOML languages in listings 1.10; pdflatex + non-ASCII in listings needs
   `literate=`), minted rejected (needs `-shell-escape` + Pygments).
   **Measured trap:** `\textcolor[rgb]{0.00,0.44,0.13}{x}` written *directly inside* a commandchars Verbatim fails
   ("Illegal unit of measure"); `\textcolor[HTML]{00702A}{x}`, named colours and macros defined in the preamble work.
   nbconvert works around the same trap with `\def\tcRGB{\textcolor[RGB]}\expandafter\tcRGB\expandafter{\detokenize{98,98,98}}{…}`.
4. **SVG figures → PDF with `svg2pdf 0.13.0`** (`svg2pdf::to_pdf(&usvg::Tree, ConversionOptions, PageOptions) -> Result<Vec<u8>, ConversionError>`),
   text embedded as subset Type0/CIDFontType2 fonts, 170 ms for a small diagram, `\includegraphics` of the result compiles
   on pdflatex/xelatex/lualatex **[verified]**. Caveat: svg2pdf 0.13.0 pins **usvg/resvg 0.45** while typst 0.15.1 uses
   usvg 0.47 and the newest usvg is 0.48.1 → two or three usvg copies in the build (see §5.2).
5. **Unicode**: pdflatex (utf8 inputenc + T1 + textcomp) accepts `→ — – … é § ° × µ ½ € £ © ™ ± •` but **errors** on
   `≥ ≠ α ∞ ℏ ′ ⅓ 日本 😀` ("Unicode character … not set up for use with LaTeX"); `newunicodechar` mappings in the
   template fix the common maths symbols. xelatex/lualatex never error but **silently drop** glyphs missing from Latin
   Modern (`≥ α ≠ ′ ℏ ⅓ 日 本 😀` → "Missing character" in the log); a template must choose fonts with coverage
   (`texgyrepagella-regular.otf` + `DejaVuSansMono.ttf` by file name worked; by family name fontspec could not find
   them on this machine). Writer: emit Unicode as-is (pandoc does too), map only quotes/dashes/ellipsis/nbsp like pandoc,
   and offer `[latex] unicode = "map"` that additionally rewrites a fixed table of maths symbols to macros for pdflatex.
6. **Citations**: biblatex by default (`\autocite`, `\textcite`, `\autocites`, `\printbibliography`), natbib alternative,
   exactly as R5 §7. `biber` is **not installed** on this machine (`which biber` → not found); `backend=bibtex` compiles
   (`Package biblatex Warning: Using fall-back bibtex backend`), `backend=biber` makes latexmk fail. Document that
   biblatex+biber is the standard pairing and that the template owns the `\usepackage[...]{biblatex}` line.
7. **Notebook cells**: emit the input as a code block and each output as `Verbatim`/figure/table like any other block,
   both wrapped in `\begin{writercell}{In|Out}{n}` when the template wants nbconvert-style prompts; the default
   `writercell` just prints `In[n]:`. nbconvert's own framing (tcolorbox + `\prompt{In}{incolor}{n}{\boxspacing}` +
   Pygments `\PY{k}{…}` macros) is reproduced in §7 for reference.

---

## 1. Versions (how checked)

| item | version | how checked |
|---|---|---|
| pandoc | 3.12.1 (`+server +lua`) | `pandoc --version` |
| TeX Live | 20260301 (`/opt/homebrew/Cellar/texlive/20260301_1`), kpathsea 6.4.2 | `kpsewhich --version`, `kpsewhich *.sty` paths |
| latexmk | 4.87 (15 June 2025) | `latexmk --version` |
| engines | pdflatex, xelatex, lualatex present; **biber absent**; bibtex present; inkscape present; rsvg-convert absent; gs present; pygmentize absent | `which` |
| LaTeX packages present | biblatex, booktabs, tcolorbox, fancyvrb, listings, cleveref, fontspec, ulem, soul, caption, subcaption, float, multirow, newunicodechar, minted, enumitem, upquote, microtype, xcolor, framed, svg, longtable, array, graphicx, hyperref | `kpsewhich <pkg>.sty` all resolved |
| listings languages | lstlang1/2/3.sty of listings in TL 2026: Python, Go, Swift, Scala, R, Java, C, C++, `[Sharp]C`, bash/sh/csh/ksh, Haskell, Lua, Ruby, PHP, Perl, SQL, XML, HTML, Matlab, Octave, Fortran, Pascal, Delphi, Ada, Lisp, Prolog, erlang, `[Objective]Caml`, VHDL, Verilog, TeX, make, Awk, tcl, … — **no Rust, JavaScript, TypeScript, JSON, YAML, TOML, Kotlin, Julia, Dart, Markdown, CSS** | grep of `\lst@definelanguage` in the three files |
| minijinja | 2.24.0 (presenter's) and **3.0.3** (latest, `cargo info`) | sources in registry, probes compiled and run |
| svg2pdf | **0.13.0** (latest on crates.io; GitHub `main` is also 0.13.0 but bumped to usvg/resvg 0.46 + pdf-writer 0.14, unreleased) | `cargo info svg2pdf`, `Cargo.toml` in registry, WebFetch of `main/Cargo.toml` |
| usvg / resvg | 0.48.1 latest (rust-version 1.85.0); svg2pdf 0.13.0 requires `usvg = "0.45"` (resolves 0.45.1, rust-version 1.65.0), typst-library 0.15.1 requires `usvg = "0.47"`, krilla-svg 0.8.1 requires usvg/resvg 0.47.0 | `cargo info`, registry `Cargo.toml` files |
| fontdb | 0.23 (svg2pdf, typst-library, krilla-svg all pin `0.23`) | registry `Cargo.toml` files |
| nbconvert | 7.17.1 (inside Homebrew jupyterlab 4.6.4_2, not importable from system python3) | `jupyter nbconvert --version` |

License of new crates: svg2pdf MIT OR Apache-2.0; minijinja Apache-2.0 (R6 table).

---

## 2. pandoc 3.12.1 as the reference

### 2.1 Inputs and outputs on disk **[verified]**

- `…/latex-probe/r7/pandoc/input.md` (every feature: front matter, attrs, unnumbered heading, levels 1–6, figures with
  `%`/`px`/`in` widths and an SVG, tables with alignments and caption, code with attrs, all list kinds, task list,
  definition list, block quote, GitHub alert, pandoc div, footnotes, citations, inline/display math, raw blocks, `\newpage`, HR).
- Outputs: `body-default.tex`, `body-chapter.tex` (`--top-level-division=chapter`), `body-listings.tex` (`--listings`),
  `body-natbib.tex`, `body-biblatex.tex`, `body-citeproc.tex`, `standalone-default.tex` (`-s`), `standalone-listings.tex`,
  `body-nowrap.tex` (`--wrap=none`), `esc.md` + its output (escaping probe), `svgonly.md`.
- pandoc prints `[WARNING] Deprecated: --listings. Use --syntax-highlighting=idiomatic instead.` — the new spelling
  gives the same lstlisting output; `--syntax-highlighting=none` gives `\begin{verbatim}`.

### 2.2 Escaping (from `src/Text/Pandoc/Writers/LaTeX/Util.hs` `stringToLaTeX`, fetched from GitHub `main`, and measured) **[verified]**

Three contexts: `TextString`, `CodeString` (inline code / `\texttt`), `URLString`.

| char | TextString / CodeString | note |
|---|---|---|
| `{` `}` | `\{` `\}` | |
| `$ % & _ #` | `\$ \% \& \_ \#` | |
| `~` | `\textasciitilde` | control word; see spacing rule below |
| `^` | `\^{}` | |
| `\` | `\textbackslash` | |
| `|` `<` `>` | `\textbar` `\textless` `\textgreater` | |
| `[` `]` | `{[}` `{]}` | prevents optional-argument capture |
| `'` | `\textquotesingle` | only straight apostrophes that survive `smart` (in code: `\textquotesingle{}p\textquotesingle{}`) |
| `` ` `` | `\textasciigrave` in CodeString only; in text `` ` `` passes (measured: `` `x` ``) | |
| `-` followed by `-` | `-\/` (ligature break: `1-\/-2`, only visible with `-smart`) | |
| `?`/`!` before `` ` `` | `?{\kern0pt}` / `!{\kern0pt}` | avoids ¿ ¡ ligatures |
| U+00A0 nbsp | `~` | `a\ b` in markdown → `a~b` |
| U+00AD soft hyphen | `\-` | |
| U+200B | `\hspace{0pt}` | |
| U+202F | `\,` | |
| U+2026 … | `\ldots` (TextString + smart) | `ellipsis\ldots{} and` |
| U+2018 U+2019 | `` ` `` `'` (smart) | adjacent quote gets `\,` |
| U+201C U+201D | ``` `` ``` `''` (smart) | |
| U+2014 U+2013 | `---` `--` (smart) | |
| `"` | **unchanged** | measured `"double"` with `-smart` |
| everything else | unchanged (UTF-8 passes through: `→ ≥ α ∞ 😀 café § ° × µ ½ € £ © ™ ≠ ± • ′ ℏ ⅓ 日本`) | only `--ascii` maps ß ø æ œ £ € © and combining accents |

Spacing rule for control words (`emitcseq`): followed by a letter → add a space (`\textasciitilde x`); followed by a
space or end/in code → add `{}` (`\textasciitilde{} `); otherwise nothing. Writer can simply always emit `{}`.

URLString (`\href{…}`/`\url{…}` argument): `\` → `/`, `#` → `\#`, `%` → `\%`, `{`→`\%7B`, `}`→`\%7D`, `|`→`\%7C`,
`^`→`\%5E`, `[`→`\%5B`, `]`→`\%5D`, `` ` ``→`\%60`; **`_ & ~ ?` pass through** (hyperref handles them). Measured:
`\href{https://example.com/a\%20b\#frag?q=x&y=_z~w}{link}`, link text escaped as text: `\href{https://example.com/x}{link with text \% \& \_}`.

Labels (`toLabel`): escape as URLString, then keep only `[A-Za-z0-9_\-+=:;.]`; any other char becomes `ux<hex>`.
Headings get `\label{<id>}` where the id is the attribute id or the auto-identifier (`\label{sec:intro}`, `\label{level-four}`).

### 2.3 Headings **[verified]**

```latex
\section{Introduction}\label{sec:intro}
\subsection*{Unnumbered heading}\label{unnumbered-heading}
\addcontentsline{toc}{subsection}{Unnumbered heading}
\subsubsection{Level three}\label{sec:three}
\paragraph{Level four}\label{level-four}
\subparagraph{Level five}\label{level-five}
Level six                                  % article: level 6 has no command → plain paragraph
```

`--top-level-division=chapter`: chapter, section, subsection, subsubsection, paragraph, subparagraph (level 6 OK);
`=part`: part, chapter, section, subsection, subsubsection, paragraph. Default is section **unless the metadata
`documentclass` is `book`, `report` or `memoir`**, then chapter (measured for all three). Unnumbered via `*` form;
`\setcounter{secnumdepth}{-\maxdimen}` in the preamble unless `--number-sections` (then `secnumdepth` = 5).
Internal link `[Section](#sec:intro)` → `\hyperref[sec:intro]{Section}`. Span with id →
`\protect\phantomsection\label{myid}{with class}` (classes dropped). `-s` with `documentclass: book` adds
`\frontmatter`/`\mainmatter`/`\backmatter` (`has-frontmatter`).

### 2.4 Figures and images **[verified]**

```latex
\begin{figure}
\centering
\includegraphics[width=0.5\linewidth,height=\textheight,keepaspectratio,alt={A plot caption with emphasis and \$ \& \%}]{figures/plot.png}
\caption{A plot caption with \emph{emphasis} and \$ \& \%}\label{fig:plot}
\end{figure}
```

- `width=50%` → `width=0.5\linewidth`; `width=300px height=100px` → `width=3.125in,height=1.04167in` (px ÷ `--dpi`,
  default 96); `width=3in` kept; no size → `\pandocbounded{\includegraphics[keepaspectratio,alt={…}]{…}}` (macro in the
  preamble scales to fit `\linewidth`/`\textheight`); when only width is given pandoc adds `height=\textheight,keepaspectratio`.
- `alt={…}` is the graphicx ≥ 2021 accessibility key (plain-text alt), compiles on TL 2026.
- No placement option (`\def\fps@figure{htbp}` in the preamble instead); inline image (not alone in a paragraph) →
  bare `\includegraphics[width=1cm,…]` in the text; the caption is the alt text/implicit figure rule.
- **SVG: pandoc does not convert for `-t latex`**; it emits `\includesvg[…]{figures/diagram.svg}` and `\usepackage{svg}`
  in `-s` output (the `svg` package shells out to inkscape and needs `-shell-escape`). For `-o x.pdf` pandoc tries
  `rsvg-convert`; here it printed `[WARNING] Could not fetch resource figures/diagram.svg: replacing image with description`.

### 2.5 Tables **[verified]**

```latex
\begin{longtable}[]{@{}lcrl@{}}
\caption{A table caption with 50\%}\label{tbl:t1}\tabularnewline
\toprule\noalign{}
Left & Center & Right & Default \\
\midrule\noalign{}
\endfirsthead
\toprule\noalign{}
Left & Center & Right & Default \\
\midrule\noalign{}
\endhead
\bottomrule\noalign{}
\endlastfoot
a \& b & 1 & 2.5 & x\_y \\
\emph{emph} & \texttt{code} & 100\% & \textasciitilde{} \\
\end{longtable}
```

- Column spec from alignments: `l`/`c`/`r`; default alignment → `l`. Caption goes *first* with `\tabularnewline`;
  no caption → `{\def\LTcaptype{none} % do not increment counter … }` (needs `\newcounter{none}` from the preamble).
- Relative widths (grid tables / long pipe rows) → `>{\raggedright\arraybackslash}p{(\linewidth - 2\tabcolsep) * \real{0.9625}}`
  columns and header cells wrapped in `\begin{minipage}[b]{\linewidth}\raggedright … \end{minipage}` (needs `calc`, `array`).
- Preamble: `\usepackage{longtable,booktabs,array}`, `\usepackage{caption}`, `\captionsetup[table]{skip=6pt}`,
  `\newcounter{none}`, `\usepackage{calc}`, and an etoolbox patch of `\longtable` for `\paragraph` ordering.

### 2.6 Code **[verified]**

Default (skylighting): `\begin{Shaded}\begin{Highlighting}[]` + token macros (`\KeywordTok{def}\NormalTok{ f(x):}` …,
`\{`/`\}`/`\#`/`\%` escaped inside, `\textless{}` for `<`), `\end{Highlighting}\end{Shaded}`; preamble defines
`\DefineVerbatimEnvironment{Highlighting}{Verbatim}{commandchars=\\\{\}}`, `\newenvironment{Shaded}{}{}`,
`\newcommand{\VERB}{\Verb[commandchars=\\\{\}]}` and 30 `\…Tok` macros (`\textcolor[rgb]{…}` + `\textbf`/`\textit`).
Inline highlighted code: `\VERB|\NormalTok{code}|`. Unknown/no language → `\begin{verbatim}…\end{verbatim}`;
inline code → `\texttt{code\ with\ \{\ \}\ \textbackslash{}\ \%\ \_}` (spaces as `\ `). Fenced attrs `{#lst:main caption="…"}`
→ `\protect\phantomsection\label{lst:main}%` before the block.

`--syntax-highlighting=idiomatic` (= deprecated `--listings`): `\begin{lstlisting}[language=Python]…`,
`[caption={Main listing}, label=lst:main]`, inline `\passthrough{\lstinline!code!}` (delimiter switches to `"` when
the code contains `!`), preamble `\usepackage{listings}`, `\newcommand{\passthrough}[1]{#1}`,
`\lstset{defaultdialect=[5.3]Lua}`, `\lstset{defaultdialect=[x86masm]Assembler}`. Language mapping =
`Text.Pandoc.Highlighting.langsList` (lower-cased exact lookup):
`abap→ABAP acm acmscript acsl ada→Ada algol→Algol ant→Ant assembler/gnuassembler→Assembler awk→Awk bash→bash monobasic/purebasic→Basic c/cs/objectivec→C cpp/c++→C++ ocaml→Caml cil clean cobol comal80 command.com comsol csh delphi eiffel elan elisp erlang euphoria fortran gap gcl gnuplot go→Go hansl haskell→Haskell html→HTML idl inform java→Java jvmis ksh lingo lisp/commonlisp→Lisp llvm logo lua→Lua make/makefile→make mathematica matlab mercury metapost miranda mizar ml modula2→Modula-2 mupad nastran oberon2→Oberon-2 ocl octave oorexx oz pascal perl php pli→PL/I plasm postscript pov prolog promela pstricks python→Python r→R reduce rexx rsl ruby s sas scala scilab sh shelxl simula sparql sql swift tcl tex/latex→TeX vbscript verilog vhdl vrml xml xslt`.
Measured: `py`, `rust`, `rs`, `javascript`, `js`, `typescript`, `ts`, `json`, `yaml`, `toml`, `shell`, `csharp`,
`julia`, `kotlin`, `text` → `\begin{lstlisting}` with **no** language.

### 2.7 Lists **[verified]**

```latex
\begin{itemize}
\tightlist
\item
  bullet one
\item
  bullet two
  \begin{itemize}
  \tightlist
  \item
    nested
  \end{itemize}
\end{itemize}

\begin{enumerate}
\def\labelenumi{\arabic{enumi})}   % delimiter "3)" ; "." → \arabic{enumi}. ; roman → \roman{enumi}. ; nested level uses enumii
\setcounter{enumi}{2}              % start=3
\tightlist
\item
  third with paren
\end{enumerate}

\begin{itemize}
\tightlist
\item[$\square$]
  task open
\item[$\boxtimes$]
  task done
\end{itemize}

\begin{description}
\tightlist
\item[Term one]
Definition one
\end{description}
```

Loose lists omit `\tightlist`; `\providecommand{\tightlist}{\setlength{\itemsep}{0pt}\setlength{\parskip}{0pt}}` lives
in the preamble; `$\square$`/`$\boxtimes$` need amssymb.

### 2.8 Other blocks and inlines **[verified]**

| markdown | LaTeX |
|---|---|
| block quote | `\begin{quote}A block quote with two lines.\end{quote}` |
| `> [!NOTE]` | plain quote with literal `{[}!NOTE{]}` (pandoc's markdown reader has no alerts) |
| `::: {.callout-warning}` div | wrapper dropped, content emitted; div with id → `\protect\phantomsection\label{mydiv}` |
| footnote | `Here is a footnote.\footnote{The footnote text with \textbf{strong}.` + blank line + `  A second paragraph in the footnote.}`; inline `^[…]` same |
| `~~strike~~` | `\st{strikeout}` (soul; LuaTeX: `\usepackage{luacolor}` + `\usepackage[soul]{lua-ul}`) |
| `[u]{.underline}` | `\ul{u}` (soul) |
| `==mark==` (`+mark`) | `\hl{marked}` (soul) |
| `H~2~O`, `x^2^` | `H\textsubscript{2}O`, `x\textsuperscript{2}` |
| `[x]{.smallcaps}` | `\textsc{x}` |
| `*e*`, `**s**` | `\emph{e}`, `\textbf{s}` |
| hard line break | `\\` ; line blocks `\| a` → `a\\` lines |
| `$a$` / `$$a$$` | `\(a\)` / `\[a\]` (aligned environments go inside `\[…\]`) |
| raw `{=latex}` block / inline / bare `\begin{center}…` | passed through verbatim |
| `\newpage` raw | passed through (pandoc has no page-break element; `\newpage` is raw tex) |
| `---` HR | `\begin{center}\rule{0.5\linewidth}{0.5pt}\end{center}` |
| `<https://…>` | `\url{https://example.com/auto}` |
| citations (no flag) | left as literal `@doe2020`, `{[}@knuth1984, p. 12…{]}` |
| `--natbib` | `See \citet{doe2020} and \citetext{\citealp[p.~12]{knuth1984}; \citealp[see also][]{lee2019}}. Also \citeyearpar{doe2020} and \citep{doe2020}.` (bare `@fig:plot` also becomes `\citet{fig:plot}`!) |
| `--biblatex` | `See \textcite{doe2020} and \autocites[12]{knuth1984}[see also][]{lee2019}. Also \autocite*{doe2020} and \autocite{doe2020}.` |
| `--citeproc` | pre-rendered text + `\protect\phantomsection\label{refs}\begin{CSLReferences}{1}{1}\bibitem[\citeproctext]{ref-doe2020} …\end{CSLReferences}` |

### 2.9 Template variables of `pandoc --print-default-template latex` **[verified]**

`$body$`, `$title$` (+`$thanks$`), `$subtitle$` (defines `\subtitle` via etoolbox), `$author$` (list joined with `\and`),
`$date$`, `$abstract$` (`\begin{abstract}`), `$toc$`/`$toc-title$`/`$toc-depth$`, `$lof$`, `$lot$`, `$header-includes$`
(list), `$include-before$`/`$include-after$` (lists), `$bibliography$` (list) with `$natbib$`/`$biblatex$`/`$biblio-title$`,
`$nocite-ids$`, `$documentclass$`, `$classoption$`, `$fontsize$`, `$papersize$`, `$geometry$`, `$numbersections$`/`$secnumdepth$`,
`$has-frontmatter$`, `$has-chapters$`, `$babel-lang$`/`$babel-otherlangs$`/`$lang$` (→ `pdflang`, babel), `$keywords$`
(→ `pdfkeywords`), `$colorlinks$`/`$toccolor$`, `$linestretch$`, `$beamerarticle$`, `$cancel$`, `$pdf-trailer-id$`, and
partials `document-metadata.latex`, `passoptions.latex`, `fonts.latex`, `font-settings.latex`, `common.latex`,
`after-header-includes.latex`, `hypersetup.latex`. Standalone preamble essentials (`standalone-default.tex`):
`\PassOptionsToPackage{unicode}{hyperref}`, `\PassOptionsToPackage{hyphens}{url}`, `\usepackage{xcolor}`,
`\usepackage{amsmath,amssymb}`, `\usepackage{iftex}` + `\ifPDFTeX \usepackage[T1]{fontenc}\usepackage[utf8]{inputenc}\usepackage{textcomp} \else \usepackage{unicode-math} … \fi`,
`\usepackage{lmodern}`, upquote, microtype, parskip, `color`, `fancyvrb` + Highlighting defs, longtable/booktabs/array/caption/calc,
graphicx + `\pandocbounded`, `svg` (only when an SVG is used), soul (or lua-ul), `\setlength{\emergencystretch}{3em}`,
`\providecommand{\tightlist}`, `bookmark`, `xurl`, `\urlstyle{same}`, `\hypersetup{pdftitle=…,pdfauthor=…,hidelinks,pdfcreator={LaTeX via pandoc}}`.

Pandoc's own standalone output of `input.md` **fails under pdflatex** on `≥` (and would on `α`) and **passes under
xelatex** with `Missing character` warnings for `α ≥ 😀` **[verified, `pandoc/out-pdf`, `pandoc/out-xe`]** — the
unicode problem in §6 is not writer-specific.

---

## 3. Template mechanism

### 3.1 minijinja API **[verified from sources, both versions]**

minijinja 2.24.0, `src/syntax.rs` (needs `features = ["custom_syntax"]`, which pulls `aho-corasick`):

```rust
pub struct SyntaxConfigBuilder { /* delims: Arc<Delims> */ }
impl SyntaxConfigBuilder {
    pub fn block_delimiters<S, E>(&mut self, s: S, e: E) -> &mut Self where S: Into<Cow<'static, str>>, E: Into<Cow<'static, str>>;
    pub fn variable_delimiters<S, E>(&mut self, s: S, e: E) -> &mut Self /* same bounds */;
    pub fn comment_delimiters<S, E>(&mut self, s: S, e: E) -> &mut Self /* same bounds */;
    pub fn line_statement_prefix<S>(&mut self, s: S) -> &mut Self where S: Into<Cow<'static, str>>;
    pub fn line_comment_prefix<S>(&mut self, s: S) -> &mut Self where S: Into<Cow<'static, str>>;
    pub fn build(&self) -> Result<SyntaxConfig, Error>;   // ErrorKind::InvalidDelimiter if a required start marker is empty or two start markers are equal
}
pub struct SyntaxConfig { /* delims, aho_corasick: Option<AhoCorasick> */ }
impl SyntaxConfig {
    pub fn builder() -> SyntaxConfigBuilder;
    pub fn block_delimiters(&self) -> (&str, &str);
    pub fn variable_delimiters(&self) -> (&str, &str);
    pub fn comment_delimiters(&self) -> (&str, &str);
    pub fn line_statement_prefix(&self) -> Option<&str>;
    pub fn line_comment_prefix(&self) -> Option<&str>;
}
// src/environment.rs
#[cfg(feature = "custom_syntax")] pub fn set_syntax(&mut self, syntax: crate::syntax::SyntaxConfig);
#[cfg(feature = "custom_syntax")] pub fn syntax(&self) -> &crate::syntax::SyntaxConfig;
pub fn set_keep_trailing_newline(&mut self, yes: bool);   // 2.x: on Environment
pub fn set_trim_blocks(&mut self, yes: bool);
pub fn set_lstrip_blocks(&mut self, yes: bool);
pub fn set_undefined_behavior(&mut self, behavior: UndefinedBehavior);  // Lenient | Chainable | Strict | SemiStrict (src/utils.rs)
pub fn set_auto_escape_callback<F>(&mut self, f: F) where F: Fn(&str) -> AutoEscape + 'static + Sync + Send;
pub fn add_template(&mut self, name: &'source str, source: &'source str) -> Result<(), Error>;
pub fn add_template_owned<N, S>(&mut self, name: N, source: S) -> Result<(), Error>;
pub fn get_template(&self, name: &str) -> Result<Template<'_, '_>, Error>;
pub fn render_str<S: Serialize>(&self, source: &str, ctx: S) -> Result<String, Error>;
pub fn add_filter<N, F, Rv, Args>(&mut self, name: N, f: F);
// src/template.rs
pub fn render<S: Serialize>(&self, ctx: S) -> Result<String, Error>;
```

The doc comment on `SyntaxConfig` in 2.24.0 literally shows the LaTeX configuration:
`.block_delimiters("\\BLOCK{", "}").variable_delimiters("\\VAR{", "}").comment_delimiters("\\#{", "}")`. Note the
doc rule: "The end markers can be shared, but the start markers need to be distinct."

minijinja 3.0.3 differences (sources read): there is **no `custom_syntax` feature and no aho-corasick dependency**
(`src/syntax.rs` has its own `StartMarkerMatcher`); `set_syntax`/`syntax()` are unconditional. `SyntaxConfigBuilder`
gains `trim_blocks(&mut self, bool)`, `lstrip_blocks(&mut self, bool)`, `keep_trailing_newline(&mut self, bool)` (the
`Environment::set_*` versions are gone) and `SyntaxConfig::to_builder()`. `Template::render<V: Into<Value>>` and
`Environment::render_str<V: Into<Value>>` take `Into<Value>`; a `serde_json::Value`/any `Serialize` goes in as
`Value::from(minijinja::value::Serde(ctx))` with `features = ["serde"]` (`pub struct Serde<T>(pub T)`,
`impl<T: serde::Serialize> From<Serde<T>> for Value`, `src/value/mod.rs:306` and `:1993`). The default auto-escape
callback (`src/defaults.rs`) returns `AutoEscape::Html` only for `.html/.htm/.xml` and `AutoEscape::Json` for
`.json/.json5/.js/.yaml/.yml` (strips `.j2/.jinja/.jinja2/.txt` first), so `.tex` templates are never auto-escaped;
set the callback to `AutoEscape::None` explicitly anyway.

### 3.2 Probe results **[verified, `mj-probe` (2.24.0) and `mj2-probe` (3.0.3), identical output]**

```
[plain braces survive] OK: "\\textbf{x} {{ not a var }} {% not a block %} {# not a comment #}"
[var] OK: "Title: A & B_c"                      -- no escaping unless a filter is applied
[filter] OK: "A \\& B\\_c"                      -- custom `tex` filter
[block] OK: "\\tableofcontents"
[loop] OK: "\\author{Ada \\and Bob}"
[comment] OK: "AB"
[latex comment line] OK: "% A & B_c still expands inside a TeX comment\n"   -- a TeX % comment does NOT hide \VAR{}
[macro call] OK: "xy"                           -- \VAR{ 'x' ~ 'y' }
[nested braces in expr] OK: "1"                 -- \VAR{ {'a': 1}['a'] } works: the lexer tracks nesting inside expressions
[undefined] ERR: undefined value (in <string>:1)  -- UndefinedBehavior::Strict
[whitespace control] OK: "abc"                  -- \BLOCK{- if true -} … \BLOCK{- endif -}
[raw] OK: "\\VAR{kept}"                         -- \BLOCK{raw}\VAR{kept}\BLOCK{endraw}
[trailing newline kept] OK: "x\n"               -- 3.0.3 with .keep_trailing_newline(true)
```

Gotchas measured while building the sample: (1) a `\#{ … }` comment ends at the **first** `}`; a comment that
mentions `\VAR{x}` breaks the template ("unknown `.`, expected statement" at the next `\BLOCK{...}`). Rule for the
docs: no literal `}` inside a tag's text; use `\BLOCK{raw}…\BLOCK{endraw}` for literal `\VAR{` sequences. (2) Values
inserted by `\VAR{body}` are *not* re-parsed, so a document that contains the string `\VAR{` is safe. (3) Templates
must put anything that relies on writer's packages (`\fvset`, `\hypersetup`, `\RecustomVerbatimEnvironment`, cleveref)
**after** `\VAR{preamble}` — `\fvset` before it gave `! Undefined control sequence`.

### 3.3 Recommendation **[design]**

- `minijinja = { version = "3.0", features = ["serde", "loop_controls"] }` (R6 moves presenter to 3.0 anyway; if
  the workspace stays on 2.24, add `custom_syntax`). Delimiters `\VAR{`/`\BLOCK{`/`\#{` with `}`; `UndefinedBehavior::Strict`
  so a template typo fails loudly; `keep_trailing_newline(true)`; `AutoEscape::None`; register filters `tex` (text
  escaping of §9) and `texpath` (URL/path escaping). Template discovery: `[latex] template = "path.tex"` or
  `--template`; none → writer's bundled sample (§3.4) written next to the output on `writer latex --init-template`.
- Context variables (all strings already LaTeX-escaped where they are text; lists stay lists):
  `body`, `preamble`, `title`, `subtitle`, `author` (list), `date`, `abstract`, `keywords` (list), `lang`, `toc` (bool),
  `toc_depth`, `bibliography` (list of copied `.bib` paths, relative), `bib_style` (CSL name from front matter so the
  template can map it to a biblatex/natbib style), `cite_style` (`biblatex`/`natbib`/`plain`), `has_chapters`,
  `engine` (`pdflatex`/`xelatex`/`lualatex`, informational), `meta` (the raw front-matter map for anything else),
  `writer_version`. Raw (unescaped) title inlines are available as `title_plain` for `pdftitle` (hyperref wants no macros).
- Outputs: `<out>.tex` (template + body inlined), `<out>-preamble.tex` (same text as `preamble`, for templates that
  prefer `\input`), copied assets under `<out-dir>/figures/` (PNG/JPG copied, SVG converted to PDF, notebook images
  written), copied `.bib` files. The user runs `latexmk -pdf <out>.tex` (document this; writer never invokes TeX).

### 3.4 Sample template (compiles with pdflatex, xelatex and lualatex) **[verified]**

File `…/latex-probe/r7/template-biblatex.tex` (biblatex variant; the natbib variant replaces three lines, see diff below):

```latex
\#{ writer sample template (BYO). Delimiters: backslash-VAR prints a variable, backslash-BLOCK is a statement, backslash-hash is a comment. }
\#{ Rule: a VAR, BLOCK or comment tag ends at the first closing brace, so none of them may contain a literal closing brace. }
\documentclass[11pt,a4paper]{article}
\usepackage[margin=2.5cm]{geometry}
\usepackage{parskip}
\usepackage{iftex}
\ifPDFTeX
  \usepackage[T1]{fontenc}
  \usepackage[utf8]{inputenc}
  \usepackage{textcomp}
  \usepackage{lmodern}
  \usepackage{newunicodechar}
  \newunicodechar{→}{\ensuremath{\rightarrow}}
  \newunicodechar{≥}{\ensuremath{\geq}}
  \newunicodechar{α}{\ensuremath{\alpha}}
\else
  \usepackage{fontspec}
  \setmainfont{texgyrepagella-regular.otf}[BoldFont=texgyrepagella-bold.otf,ItalicFont=texgyrepagella-italic.otf,BoldItalicFont=texgyrepagella-bolditalic.otf]
  \setmonofont{DejaVuSansMono.ttf}[Scale=MatchLowercase]
\fi
\usepackage{listings}
\lstset{basicstyle=\ttfamily\small,breaklines=true,showstringspaces=false,frame=single}
\usepackage{tcolorbox}
\tcbuselibrary{skins,breakable}
\colorlet{writercalloutnote}{blue!60!black}
\colorlet{writercallouttip}{green!50!black}
\colorlet{writercalloutimportant}{violet}
\colorlet{writercalloutwarning}{orange!80!black}
\colorlet{writercalloutcaution}{red!70!black}
% the body emits \begin{writercallout}{<kind>}{<title>} ... \end{writercallout}
\newenvironment{writercallout}[2]{%
  \begin{tcolorbox}[enhanced,breakable,colframe=writercallout#1,colback=writercallout#1!5!white,
    coltitle=white,fonttitle=\bfseries,title={#2}]}{\end{tcolorbox}}
% the body emits \begin{writercell}{In|Out}{<n>} ... \end{writercell} for notebook cells
\newenvironment{writercell}[2]{\par\noindent{\ttfamily\color{gray}#1\,[#2]:}\par\nobreak}{\par}
\VAR{preamble}
\fvset{frame=lines,framesep=2mm,fontsize=\small}
\hypersetup{colorlinks=true,linkcolor=blue!60!black,citecolor=green!40!black,urlcolor=blue}
\usepackage[backend=\VAR{biblatex_backend},style=authoryear]{biblatex}
\BLOCK{for b in bibliography}\addbibresource{\VAR{b}}
\BLOCK{endfor}

\title{\VAR{title}\BLOCK{if subtitle}\\[0.5em]{\large \VAR{subtitle}}\BLOCK{endif}}
\author{\BLOCK{for a in author}\VAR{a}\BLOCK{if not loop.last} \and \BLOCK{endif}\BLOCK{endfor}}
\date{\VAR{date}}

\begin{document}
\maketitle
\BLOCK{if abstract}
\begin{abstract}
\VAR{abstract}
\end{abstract}
\BLOCK{endif}
\BLOCK{if toc}
\tableofcontents
\BLOCK{endif}

\VAR{body}

\BLOCK{if bibliography}
\printbibliography
\BLOCK{endif}
\end{document}
```

natbib variant (`template-natbib.tex`), the only differences:

```latex
\usepackage[round]{natbib}
…
\bibliographystyle{plainnat}
\bibliography{\BLOCK{for b in bibliography}\VAR{b | replace('.bib', '')}\BLOCK{if not loop.last},\BLOCK{endif}\BLOCK{endfor}}
```

Minimal template (`template-minimal.tex`, defines none of the writer hooks → the preamble's fallbacks are used):

```latex
\documentclass{article}
\usepackage[T1]{fontenc}
\usepackage[utf8]{inputenc}
\usepackage{listings}
\VAR{preamble}
\usepackage[backend=bibtex,style=numeric]{biblatex}
\addbibresource{refs.bib}
\begin{document}
\VAR{body}
\printbibliography
\end{document}
```

### 3.5 Compile matrix **[verified, `latexmk -<engine> -interaction=nonstopmode -halt-on-error -outdir=out-… main-….tex`]**

Body = `body.tex` (§4.3, exercises headings, escapes, inline markup, PNG figure, svg2pdf PDF figure, px-sized figure,
longtable+booktabs with caption, two callouts, Verbatim, writercode (token macros), lstlisting with caption/label,
two writercell blocks, all list kinds, quote, inline/aligned/numbered math, footnote with a cite, biblatex/natbib
cites, raw block, `\writerhrule`, `\writerpagebreak`, cross-references with `\ref`/`\eqref`).

| template | engine | exit | wall time | pages | warnings left in the log |
|---|---|---|---|---|---|
| biblatex (`backend=bibtex`) | pdflatex | 0 | 9 s | 5 | `Package biblatex Warning: Using fall-back bibtex backend`, `Package newunicodechar Warning: Redefining Unicode character` (→ was already mapped by inputenc) |
| natbib + bibtex | pdflatex | 0 | 5 s | 5 | newunicodechar redefinition only |
| biblatex | xelatex | 0 | 10 s | 5 | fall-back bibtex backend |
| natbib | xelatex | 0 | 6 s | 5 | none |
| biblatex | lualatex | 0 | 11 s | 5 | fall-back bibtex backend |
| natbib | lualatex | 0 | 6 s | 5 | none |
| minimal (fallback hooks) | pdflatex | 0 | — | 4 | fall-back bibtex backend |
| biblatex `backend=biber` | pdflatex | **12** | — | — | latexmk: biber not found (`which biber` → not found) |

No `Missing character`, no undefined references, no overfull boxes after the final run. Rendered pages
(`pages/bib-pdf-0{1..5}.png`, `pages/bib-xe-*.png`, `pages/min-*.png`) were inspected: PNG figure, SVG-derived PDF
figure (with its `→ α ≥` text intact), tcolorbox callouts, framed Verbatim, coloured token macros, listings frame,
`In[3]:`/`Out[3]:` cells, task-list boxes, description list, numbered equation `(1)`, author-year citations, and the
bibliography all render as intended. The sibling probe in `…/latex-probe/` (same ideas, written by the R5 agent at
10:15) compiled too; `r7/` is the version this note describes.

---

## 4. What the emitted body assumes

### 4.1 Packages (keep the list fixed and documented; the generated preamble loads exactly these) **[verified by the compile matrix]**

| package | used for | emitted only when |
|---|---|---|
| `graphicx` | `\includegraphics` | always |
| `longtable`, `booktabs`, `array` | tables (`\toprule`…, `\tabularnewline`, `>{…}p{…}` columns) | always (or only when the document has a table — cheap either way) |
| `caption` | `\caption` outside floats (`\captionof`), `\captionsetup` by templates | always |
| `amsmath`, `amssymb` | `\(…\)`, `\[…\]`, `equation`, `aligned`, `\eqref`, `$\square$`/`$\boxtimes$` | always |
| `xcolor` | `\textcolor` in highlighted code and ANSI output | always |
| `fancyvrb` | `Verbatim`, `writercode` (`\DefineVerbatimEnvironment`) | always |
| `ulem` with `[normalem]` | `\sout`, `\uline` | always (`normalem` keeps `\emph` italic) |
| `hyperref` | `\href`, `\url`, `\ref` links, `\phantomsection` | always, loaded last in the preamble so the template can `\hypersetup` after `\VAR{preamble}` and load `cleveref` after it |
| `listings` | only with `[latex] code = "listings"` | template's job to load (it must `\lstset` anyway); writer warns if the preamble is used without it |
| `float` (`[H]`), `multirow`, `subcaption`, `tcolorbox`, `newunicodechar`, `fontspec`, `biblatex`/`natbib`, `cleveref` | styling / optional features | **template's job** (sample template shows them) |

Not used on purpose: `soul` (pandoc's `\st`/`\ul`/`\hl`: fragile with macros inside the argument and needs `lua-ul`
under LuaTeX), `minted`, `svg`, `grffile`, `adjustbox`, `calc`/`etoolbox` patches (no relative-width columns in v1).

### 4.2 Names a template MAY define (writer provides defaults) **[verified]**

| hook | emitted as | default in the generated preamble |
|---|---|---|
| `writercallout` env, 2 args | `\begin{writercallout}{<kind>}{<title>} … \end{writercallout}`; kind ∈ `note tip important warning caution` (lower-case, GitHub alert names); title already escaped, never empty (default title = capitalised kind) | `\AtBeginDocument{\@ifundefined{writercallout}{\newenvironment{writercallout}[2]{\par\medskip\noindent\begin{minipage}{\linewidth}\textbf{#2}\par\nobreak}{\end{minipage}\par\medskip}}{}}` |
| `writercell` env, 2 args | `\begin{writercell}{In}{3} … \end{writercell}`, `{Out}{3}`; second arg empty string when there is no execution count | `\newenvironment{writercell}[2]{\par\medskip\noindent\textbf{\texttt{#1[#2]:}}\par\nobreak}{\par\medskip}` (same `\AtBeginDocument` guard) |
| `writercode` Verbatim env | highlighted code blocks | `\DefineVerbatimEnvironment{writercode}{Verbatim}{commandchars=\\\{\}}` — **defined by writer, always**; templates restyle it with `\fvset{…}` or `\RecustomVerbatimEnvironment{writercode}{Verbatim}{commandchars=\\\{\},frame=single}` after `\VAR{preamble}` (a `\DefineVerbatimEnvironment` inside `\AtBeginDocument` broke `commandchars`: "Illegal unit of measure", so it is not guarded) |
| `\WrKeyword \WrString \WrComment \WrNumber \WrFunction \WrType \WrVariable \WrOperator \WrPreproc \WrConstant \WrAttribute \WrError \WrText` (1 arg each) | tokens inside `writercode` | `\providecommand{\WrKeyword}[1]{\textcolor[rgb]{0.00,0.44,0.13}{\textbf{#1}}}` … (pandoc's default colours); templates `\renewcommand` after the preamble |
| `\writerpagebreak` | page-break directive | `\providecommand{\writerpagebreak}{\clearpage}` |
| `\writerhrule` | thematic break | `\providecommand{\writerhrule}{\begin{center}\rule{0.5\linewidth}{0.4pt}\end{center}}` |
| `\tightlist` | tight lists (pandoc convention) | `\providecommand{\tightlist}{\setlength{\itemsep}{0pt}\setlength{\parskip}{0pt}}` |

The generated preamble as compiled (`…/latex-probe/r7/writer-preamble.tex`):

```latex
% ---- generated by writer: packages and fallbacks the body relies on (no styling) ----
\usepackage{graphicx}
\usepackage{longtable,booktabs,array}
\usepackage{caption}
\usepackage{amsmath,amssymb}
\usepackage{xcolor}
\usepackage{fancyvrb}
\usepackage[normalem]{ulem}
\usepackage{hyperref}
% highlighted code: writer emits \begin{writercode} with \textcolor runs; templates restyle it with \fvset or \RecustomVerbatimEnvironment
\DefineVerbatimEnvironment{writercode}{Verbatim}{commandchars=\\\{\}}
% token-class macros used inside writercode (pandoc-style; templates may \renewcommand them)
\providecommand{\WrKeyword}[1]{\textcolor[rgb]{0.00,0.44,0.13}{\textbf{#1}}}
\providecommand{\WrString}[1]{\textcolor[rgb]{0.25,0.44,0.63}{#1}}
\providecommand{\WrComment}[1]{\textcolor[rgb]{0.38,0.63,0.69}{\textit{#1}}}
\providecommand{\WrNumber}[1]{\textcolor[rgb]{0.25,0.63,0.44}{#1}}
\providecommand{\WrFunction}[1]{\textcolor[rgb]{0.02,0.16,0.49}{#1}}
\providecommand{\WrType}[1]{\textcolor[rgb]{0.56,0.13,0.00}{#1}}
\providecommand{\WrVariable}[1]{\textcolor[rgb]{0.10,0.09,0.49}{#1}}
\providecommand{\WrOperator}[1]{\textcolor[rgb]{0.40,0.40,0.40}{#1}}
\providecommand{\WrPreproc}[1]{\textcolor[rgb]{0.74,0.48,0.00}{#1}}
\providecommand{\WrConstant}[1]{\textcolor[rgb]{0.53,0.00,0.00}{#1}}
\providecommand{\WrAttribute}[1]{\textcolor[rgb]{0.49,0.56,0.16}{#1}}
\providecommand{\WrError}[1]{\textcolor[rgb]{1.00,0.00,0.00}{\textbf{#1}}}
\providecommand{\WrText}[1]{#1}
\providecommand{\tightlist}{\setlength{\itemsep}{0pt}\setlength{\parskip}{0pt}}
\providecommand{\writerpagebreak}{\clearpage}
\providecommand{\writerhrule}{\begin{center}\rule{0.5\linewidth}{0.4pt}\end{center}}
\makeatletter
\AtBeginDocument{%
  \@ifundefined{writercallout}{%
    \newenvironment{writercallout}[2]{\par\medskip\noindent\begin{minipage}{\linewidth}\textbf{#2}\par\nobreak}{\end{minipage}\par\medskip}}{}%
  \@ifundefined{writercell}{%
    \newenvironment{writercell}[2]{\par\medskip\noindent\textbf{\texttt{#1[#2]:}}\par\nobreak}{\par\medskip}}{}%
}
\makeatother
```

Ordering rule for template authors (document it in the sample): `\documentclass` → your packages and your
`\newenvironment{writercallout}`/`{writercell}` → `\VAR{preamble}` → `\fvset`, `\renewcommand{\WrKeyword}`,
`\hypersetup`, `\usepackage{cleveref}`, `\usepackage{biblatex}` → `\begin{document}`.

### 4.3 The hand-written reference body

`…/latex-probe/r7/body.tex` is the exact text the mapping in §8 should produce for the feature set; it is the
golden file for the first `insta` snapshot of the LaTeX backend. Its natbib twin `body-natbib.tex` differs only in
the five citation commands.

---

## 5. Figures

### 5.1 svg2pdf 0.13.0 **[verified: source + `svg2pdf-probe` run]**

```rust
// svg2pdf-0.13.0/src/lib.rs
pub use usvg;                                   // re-export: svg2pdf::usvg is usvg 0.45.1
#[derive(Copy, Clone)] pub struct PageOptions { pub dpi: f32 }            // Default: 72.0
#[derive(Copy, Clone)] pub struct ConversionOptions { pub compress: bool, pub raster_scale: f32, pub embed_text: bool, pub pdfa: bool }
                                                 // Default: compress=true, raster_scale=1.5, embed_text=true, pdfa=false
#[derive(Copy, Clone, Debug)] pub enum ConversionError { InvalidImage, MissingGlyphs, TooMuchNesting, UnknownError,
    #[cfg(feature = "text")] SubsetError(fontdb::ID), #[cfg(feature = "text")] InvalidFont(fontdb::ID) }   // impl Display, no std::error::Error impl
pub fn to_pdf(tree: &usvg::Tree, conversion_options: ConversionOptions, page_options: PageOptions) -> Result<Vec<u8>, ConversionError>;
pub fn to_chunk(tree: &usvg::Tree, conversion_options: ConversionOptions) -> Result<(pdf_writer::Chunk, pdf_writer::Ref), ConversionError>;  // Form XObject for embedding
// usvg-0.45.1
impl usvg::Tree { pub fn from_str(text: &str, opt: &Options) -> Result<Self, Error>; pub fn from_data(data: &[u8], opt: &Options) -> Result<Self, Error>; /* .svgz ok */ pub fn size(&self) -> Size; }
pub struct usvg::Options<'a> { pub resources_dir: Option<PathBuf>, pub dpi: f32, pub font_family: String, pub font_size: f32, pub languages: Vec<String>,
    pub shape_rendering, pub text_rendering, pub image_rendering, pub default_size: Size, pub image_href_resolver: ImageHrefResolver<'a>,
    pub font_resolver: FontResolver<'a>, pub fontdb: Arc<fontdb::Database>, pub style_sheet: Option<String> }
impl usvg::Options<'_> { pub fn fontdb_mut(&mut self) -> &mut fontdb::Database; }   // Arc::make_mut; feature "text"
// fontdb-0.23.0
impl fontdb::Database { pub fn load_system_fonts(&mut self); pub fn load_fonts_dir<P: AsRef<Path>>(&mut self, dir: P); pub fn len(&self) -> usize; pub fn faces(&self) -> impl Iterator<Item = &FaceInfo>; }
```

Features: `default = ["image", "filters", "text"]`; `text` = `usvg/text`, `resvg/text`, `subsetter`, `ttf-parser`,
`fontdb`; `filters` pulls `tiny-skia` + `resvg/raster-images` (rasterises only filtered groups); `image` pulls
`image 0.25.5` (jpeg/png/gif). Dependencies: `pdf-writer 0.12`, `usvg 0.45` (default-features = false), `resvg 0.45`
(optional), `fontdb 0.23`, `ttf-parser 0.25`, `subsetter 0.2`, `miniz_oxide 0.8`, `once_cell`.

Probe (`…/svg2pdf-probe/src/main.rs`, run on `figures/diagram-text.svg` = 320×120 with Helvetica/Courier text, `→ α ≥`, `&amp;`, curly quotes):

```
fontdb faces: 832                      (load_system_fonts on this Mac)
tree size: 320x120 (user units = CSS px)
ConversionOptions default: compress=true raster_scale=1.5 embed_text=true pdfa=false
PageOptions default dpi=72
wrote 35574 bytes                      → %PDF-1.7, /MediaBox [0 0 320 120], fonts AAVTBK+Helvetica, BKUPKT+CourierNewPSMT, OVEMEM+ArialUnicodeMS (Type0/CIDFontType2 subsets)
time: 0.169 s wall (release build)
```

So: 1 SVG user unit → 1 PDF point at the default `dpi: 72.0` (the page is exactly the SVG size; `\includegraphics`
then scales by `width=`). Text is shaped at conversion time with the fonts in `fontdb` (system fonts here; the `α ≥ →`
glyphs came from the Arial Unicode MS fallback). For reproducible output across machines bundle a font set (typst's
`typst-assets` fonts are already in the binary: `fontdb::Database::load_font_data(&mut self, data: Vec<u8>)` or
`load_fonts_dir`, both **[verified in fontdb-0.23.0/src/lib.rs]**) and call `load_system_fonts()` only as a fallback.
usvg `Options::default()` **[verified]**: `dpi: 96.0`, `font_family: "Times New Roman"`, `font_size: 12.0`,
`languages: ["en"]`, `default_size: 100×100`. Behaviour for an SVG font family that is absent from the database
(substitution with `font_family`, no error) is the documented usvg behaviour but was **[not measured]**; `MissingGlyphs`
is only raised in PDF/A mode (doc comment on `ConversionError`). `ConversionError` implements `Display` and `Debug`
but **not** `std::error::Error` (grep of lib.rs) — wrap it with `map_err(|e| e.to_string())` or `anyhow::anyhow!("{e}")`.

### 5.2 Version matrix and alternatives **[verified crate manifests; recommendation is design]**

- writer already carries typst 0.15.1 → `typst-library` pins `usvg = "0.47"` (features `text`), `krilla-svg 0.8.1`
  pins `usvg`/`resvg` 0.47.0, `fontdb 0.23`. R10's `img-probe` used `resvg = "0.48.1"` + `usvg = "0.48.1"` + `svg2pdf = "0.13.0"`
  and built fine — i.e. **three** usvg versions (0.45.1, 0.47.0, 0.48.1) compiled. Pin writer's own `usvg`/`resvg`
  to **0.47** to match typst and leave svg2pdf's private 0.45 copy (two copies total); re-check when svg2pdf releases
  the usvg 0.46 bump on `main` (still not 0.47).
- Alternative without svg2pdf: compile a one-page typst document `#set page(width: auto, height: auto, margin: 0pt)
  #image("x.svg")` through the embedded typst (R1's World) and `typst_pdf::pdf` → a PDF with exactly the SVG size
  (typst renders SVG via krilla-svg with the same usvg 0.47). Zero new crates, but it runs the whole typst pipeline
  per figure and needs the SVG inside the World's file set **[unverified timing]**. Keep svg2pdf as the primary path
  (direct, 170 ms, standalone API) and note this as the fallback if the duplicate usvg becomes a problem.
- SVG inside a PDF needs no `svg` package and no `-shell-escape` (pandoc's `\includesvg` route needs both).

### 5.3 Sizing rules **[verified against pandoc output; mapping is design]**

| writer attribute | LaTeX |
|---|---|
| `width=50%` | `width=0.5\linewidth` (4 significant digits) |
| `width=300px` | `width=3.125in` (px ÷ 96; config `[latex] dpi`) |
| `width=3in`, `width=5cm`, `width=10em` | passed through (`in cm mm pt em ex` accepted; anything else → warning + dropped) |
| width only | `width=…,height=\textheight,keepaspectratio` (pandoc's form; keeps tall images on one page) |
| height only | `height=…,width=\linewidth,keepaspectratio` |
| both | `width=…,height=…` (no keepaspectratio, exactly as requested) |
| none | `width=\linewidth,height=\textheight,keepaspectratio` **[design: simpler than `\pandocbounded`, no macro needed]** |
| `align=left/center/right` on a figure | `\raggedright`/`\centering`/`\raggedleft` inside the figure |
| `fig-pos="H"` (notebook option) | `\begin{figure}[H]` (template must load `float`; default placement is `[htbp]` from config `figure_placement`) |

Emitted figure form (identical to pandoc's modulo `alt=`): `\begin{figure}[htbp]\n\centering\n\includegraphics[…]{figures/plot.png}\n\caption{…}\label{fig:plot}\n\end{figure}`.
Alt text: emit `alt={…}` only when the IR has an explicit alt different from the caption (graphicx ≥ 2021 required,
TL 2026 fine). Inline images (not alone in a paragraph) → bare `\includegraphics[height=1em]{…}`-style.
File names: copy to `figures/<basename>` with collisions resolved by a numeric suffix; `\includegraphics` paths use
forward slashes and no spaces (writer renames `a b.png` → `a_b.png`; graphicx needs `\includegraphics{"a b.png"}` otherwise).

---

## 6. Code listings

### 6.1 Measurements **[verified, `…/code-probe/r7/`]**

| form | pdflatex | xelatex/lualatex | notes |
|---|---|---|---|
| `verbatim` / fancyvrb `Verbatim`, ASCII | ok | ok | `\begin{Verbatim}` + `def f(x): return {"a": x & 1}  # 100% \sure_ ~^` compiled untouched |
| `Verbatim` with `→ ≥ — α café` | **error** `Unicode character ≥ (U+2265) not set up for use with LaTeX` (also `α`; `→ — é` are fine) | ok, but `Missing character: There is no α (U+03B1) in font [lmmono10-regular]` — glyph dropped silently | mitigation for pdflatex: template `\newunicodechar{≥}{\ensuremath{\geq}}` (works inside verbatim too: `uni-pdflatex-mitigated.tex` compiled with `→ ≥ — α` in a `verbatim`) |
| `listings` `language=Python`/`C`/`[Sharp]C` | ok | ok | `language=python` (lower case) also ok (listings is case-insensitive) |
| `listings` `language=Rust` or `rust` | **error** `Package Listings Error: Couldn't load requested language` / `language rust undefined` (fatal) | same | writer must emit `language=` only for names in listings' table (§1) |
| `listings` with non-ASCII, pdflatex | **error** (same inputenc error) | ok (+ Missing character) | `\lstset{inputencoding=utf8,extendedchars=true,literate={→}{{$\rightarrow$}}1 {≥}{{$\geq$}}1 {—}{{---}}1 {α}{{$\alpha$}}1 {é}{{\'e}}1}` compiled (`lst-pdflatex-literate.tex`) |
| `Verbatim[commandchars=\\\{\}]` + `\textcolor{blue}{def}` (named colour) | ok | ok | |
| … + `\textcolor[rgb]{0.00,0.44,0.13}{x}` **directly in the body** | **error** `Illegal unit of measure (pt inserted)` | (not tested) | also fails with an explicit `[]` after `\begin{writercode}` |
| … + `\textcolor[HTML]{00702A}{x}` directly in the body | ok | — | HTML model parses fine inside Verbatim |
| … + `\def\tcRGB{\textcolor[RGB]}\expandafter\tcRGB\expandafter{\detokenize{98,98,98}}{x}` (nbconvert's ansi2latex form) | ok | — | |
| … + `\KW{x}` with `\newcommand{\KW}[1]{\textcolor[rgb]{0.00,0.44,0.13}{\textbf{#1}}}` in the preamble (pandoc's form) | ok | ok (full body compiled on all three engines) | |
| minted | not tested | — | needs `-shell-escape` and `pygmentize` (absent here); rejected for BYO |

### 6.2 Recommendation **[design]**

`[latex] code = "verbatim" | "highlight" | "listings"`; default **`verbatim`**:

- `verbatim`: `\begin{Verbatim}\n<text>\n\end{Verbatim}` — text untouched (no escaping; fancyvrb reads it verbatim),
  except: a line equal to `\end{Verbatim}` cannot be represented → fall back to `writercode` with escaping for that
  block + warning. Inline code → `\texttt{…}` with text escaping and `\ ` for spaces... simpler: `\texttt{}` with the
  CodeString escaping of §2.2 (space → `\ ` only when leading/double; pandoc escapes every space as `\ `, which is
  what keeps alignment; copy pandoc).
- `highlight`: syntect (R6) → `\begin{writercode}` + lines of `\Wr<Class>{escaped text}` runs; plain runs emitted
  bare; escaping inside = `\` → `\textbackslash{}`, `{` → `\{`, `}` → `\}` (those are the only commandchars; `% & # $ _ ~ ^`
  stay literal inside Verbatim) — matches pandoc's Highlighting content (`\{`, `\}`, `\#` are what pandoc writes; `\%`
  too, harmless). Scope → class map by TextMate scope prefix (first match wins):
  `comment` → Comment; `string` → String; `constant.numeric` → Number; `constant` → Constant; `keyword.operator` → Operator;
  `keyword` | `storage.modifier` → Keyword; `storage.type` | `entity.name.type` | `entity.name.class` | `support.type` | `support.class` → Type;
  `entity.name.function` | `support.function` → Function; `variable` → Variable; `meta.preprocessor` | `keyword.control.import` → Preproc;
  `entity.other.attribute-name` → Attribute; `invalid` → Error; else → bare text. Colours come from the template
  (defaults in the preamble = pandoc's pygments-ish palette), so the body stays style-free. Alternative when exact
  theme colours are wanted (e.g. ANSI colours in notebook outputs): `\textcolor[HTML]{RRGGBB}{…}` inline — verified safe.
- `listings`: `\begin{lstlisting}[language=<L>,caption={…},label=<id>]` with `language=` only when the fence name maps
  through pandoc's `langsList` (§2.6) *and* exists in listings (same thing — that list was derived from listings); no
  escaping inside; inline → `\lstinline` with a delimiter not in the text (`!`, then `"`, `|`, `+`). Warn once per document
  under pdflatex when the code contains non-ASCII ("listings + pdflatex needs `literate=`").
- Captioned code (`caption=` attr or notebook `lst-cap`): `verbatim`/`highlight` → wrap in
  `\begin{figure}[htbp]` … `\caption{…}\label{lst:…}\end{figure}`? No — code should not float: emit
  `\captionof{listing}`? That needs a `listing` float type. Decision: emit `\begin{writerlisting}{<caption>}{<label>}…\end{writerlisting}`?
  Keep it simpler: emit the block and then `\captionof{lstlisting}{…}\label{…}` only in `listings` mode (`\captionof`
  is from `caption`, `lstlisting` counter exists when listings is loaded); in the other modes emit a `\noindent\textbf{Listing: …}`
  paragraph via `\providecommand{\writerlistingcaption}[2]{\par\noindent\textbf{#1}\label{#2}\par}` **[design, not compiled]**.

---

## 7. Unicode and engines

### 7.1 Measurements **[verified, `…/unicode-probe/r7/`]**

| text | pdflatex (`[T1]{fontenc}`, `[utf8]{inputenc}`, textcomp, lmodern) | xelatex / lualatex, Latin Modern (default fontspec or pandoc's `unicode-math` preamble) | xelatex, `texgyrepagella-regular.otf` + `DejaVuSansMono.ttf` |
|---|---|---|---|
| `→ — – … é ï § ° × µ ½ € £ © ™ ± • “ ” ‘ ’` | ok | ok | ok |
| `≥ ≠ α ∞ ℏ ′ ⅓` | **fatal** `! LaTeX Error: Unicode character ≥ (U+2265) not set up for use with LaTeX.` (first offender stops the run) | compiles, **glyph silently missing** (`Missing character: There is no ≥ (U+2265) in font [lmroman10-regular]`; `∞` and `→` exist in LM) | ok (Pagella + DejaVu cover them) |
| `日本` | fatal | missing | missing (needs a CJK font) |
| `😀` | fatal `Unicode character 😀 (U+1F600) not set up` | missing | missing |
| same chars inside `verbatim` | same as text | same | same |
| with `\usepackage{newunicodechar}` + `\newunicodechar{≥}{\ensuremath{\geq}}` … `\newunicodechar{😀}{[emoji]}` | ok (also inside verbatim) | n/a | n/a |
| `\setmainfont{TeX Gyre Pagella}` / `{DejaVu Sans Mono}` by family name | — | `! Package fontspec Error: … not loadable` on this machine (Homebrew TeX Live fonts are not visible to fontconfig); **use file names** | — |

### 7.2 Rules for writer **[design]**

- Emit text as UTF-8 and apply only pandoc's mappings (§2.2): quotes → TeX ligatures, `—`/`–` → `---`/`--`, `…` →
  `\ldots{}`, nbsp → `~`, soft hyphen → `\-`, `--` in literal text → `-\/-`. Smart quotes: comrak with `parse.smart`
  already yields `“ ” ‘ ’ — – …` (R9), so the LaTeX writer maps those code points; a straight `'` that survives →
  `\textquotesingle{}` only in code context (pandoc), plain `'` in text.
- `[latex] unicode = "keep" | "map"` (default `map` when `engine = "pdflatex"`, `keep` otherwise): `map` rewrites a
  fixed table in **text and code** to macros: `→ \textrightarrow{}` (textcomp), `← ↔ ⇒ ⇔` likewise, `≤ \ensuremath{\leq}`,
  `≥ \geq`, `≠ \neq`, `± \pm` (textcomp has `\textpm`), `× \texttimes{}`, `∞ \ensuremath{\infty}`, `≈ \approx`, `∑ ∏ √ ∂ ∇ ∈ ∉ ⊂ ⊆ ∪ ∩ ∧ ∨ ¬ ∀ ∃ ∅ ∝ ∠ ⋅`,
  Greek `α…ω Α…Ω` → `\ensuremath{\alpha}` …, `′ ″` → `\ensuremath{{}^\prime}`, `ℏ \ensuremath{\hbar}`, `ℝ ℕ ℤ ℚ ℂ` →
  `\ensuremath{\mathbb{R}}` (needs amssymb, loaded), fractions `½ ⅓ ¼ ¾` → `\textonehalf{}` (textcomp has ½ ¼ ¾; ⅓ →
  `\ensuremath{\frac13}`), `•` → `\textbullet{}`. Inside `Verbatim` the macro is typeset only via commandchars, so with
  `map` writer must emit such code blocks as `writercode` (commandchars) instead of plain `Verbatim` — or leave code
  untouched and warn. Simplest honest behaviour: **`map` applies to text; code blocks get a warning listing the
  offending characters and the hint "use xelatex or add `\newunicodechar`"**. Emoji/CJK: never mapped; warning with
  the code point and line.
- Documentation for template authors: pdflatex = `[T1]{fontenc}` + `[utf8]{inputenc}` + `textcomp` + `newunicodechar`
  lines; xelatex/lualatex = `fontspec` with fonts that cover the document (file names are the portable form on Homebrew
  TeX Live), and `grep "Missing character" *.log` after the build. Writer's `latex` command can print the set of
  non-Latin-1 code points it emitted (`--report-unicode`) so authors know what the font must cover.

---

## 8. Notebook cells in LaTeX

### 8.1 nbconvert 7.17.1 framing **[verified, `jupyter nbconvert --to latex cells.ipynb`, `…/nb-probe/r7/cells-nbconvert.tex`]**

Templates `latex/base.tex.j2` → `style_jupyter.tex.j2` (default `cell_style`), read in
`/opt/homebrew/Cellar/jupyterlab/4.6.4_2/libexec/share/jupyter/nbconvert/templates/latex/`.

```latex
\section{Title}\label{title}
Some \emph{markdown} with \(x^2\) and a 50\% share \& stuff.            % markdown cells go through pandoc
\begin{tcolorbox}[breakable, size=fbox, boxrule=1pt, pad at break*=1mm,colback=cellbackground, colframe=cellborder]
\prompt{In}{incolor}{3}{\boxspacing}
\begin{Verbatim}[commandchars=\\\{\}]
\PY{k+kn}{import}\PY{+w}{ }\PY{n+nn}{numpy}\PY{+w}{ }\PY{k}{as}\PY{+w}{ }\PY{n+nn}{np}
\PY{n+nb}{print}\PY{p}{(}\PY{l+s+s1}{\PYZsq{}}\PY{l+s+s1}{hi \PYZam{} }\PY{l+s+si}{\PYZob{}world\PYZcb{}}\PY{l+s+s1}{\PYZsq{}}\PY{p}{)}
\end{Verbatim}
\end{tcolorbox}
    \begin{Verbatim}[commandchars=\\\{\}]            % stream output: no box, no prompt
hi \& \{world\}
    \end{Verbatim}
            \begin{tcolorbox}[breakable, size=fbox, boxrule=.5pt, pad at break*=1mm, opacityfill=0]
\prompt{Out}{outcolor}{3}{\boxspacing}
\begin{Verbatim}[commandchars=\\\{\}]
array([0, 1, 2])
\end{Verbatim}
\end{tcolorbox}
    \begin{Verbatim}[commandchars=\\\{\}, frame=single, framerule=2mm, rulecolor=\color{outerrorbackground}]   % error output
\textcolor{ansi-red}{ZeroDivisionError}                         Traceback (most recent call last)
Cell \textcolor{ansi-green}{In[4], line 1}
\textcolor{ansi-green}{----> 1} \def\tcRGB{\textcolor[RGB]}\expandafter\tcRGB\expandafter{\detokenize{98,98,98}}{1}…
\textcolor{ansi-red}{ZeroDivisionError}: division by zero
    \end{Verbatim}
\prompt{In}{incolor}{ }{\boxspacing}                 % unexecuted cell: blank count
```

Preamble pieces: `\usepackage[breakable]{tcolorbox}`, `\definecolor{incolor}{HTML}{303F9F}`, `outcolor D84315`,
`cellborder CFCFCF`, `cellbackground F7F7F7`, 16 `ansi-*` colours, `\newcommand{\boxspacing}{\kern\kvtcb@left@rule\kern\kvtcb@boxsep}`,
`\newcommand{\prompt}[4]{{\ttfamily\llap{{\color{#2}[#3]:\hspace{3pt}#4}}\vspace{-\baselineskip}}}` (the prompt hangs in
the left margin), a `\Verbatim` wrapper that breaks long lines at `\PYGZ..` escapes (`\Wrappedbreaksatspecials`), the
Pygments `\PY`/`\PYZ*` definitions (`\PYZsq` = `'`, `\PYZam` = `&`, `\PYZob`/`\PYZcb` = `{`/`}`, `\PYZsh` = `#`, …),
`\floatplacement{figure}{H}`, `\adjustimage{max size={0.9\linewidth}{0.9\paperheight}}{file}` for image outputs, and
`escape_latex` = exactly the ten-character table `& % $ # _ { } ~ ^ \` (`\textasciitilde{}`, `\^{}`, `\textbackslash{}`)
plus `...` → `{\ldots}` (`nbconvert/filters/latex.py`).

### 8.2 Recommendation **[design]**

Lower a `CodeCell` (R10 §9) to ordinary blocks and wrap the two halves:

```latex
\begin{writercell}{In}{3}
\begin{Verbatim}            % or writercode / lstlisting per [latex] code
import numpy as np
np.arange(3)
\end{Verbatim}
\end{writercell}
\begin{Verbatim}            % stream stdout/stderr: no cell wrapper (nbconvert does the same), ANSI stripped
hi & {world}
\end{Verbatim}
\begin{writercell}{Out}{3}
\begin{Verbatim}
array([0, 1, 2])
\end{Verbatim}
\end{writercell}
```

- Rich outputs: figure → the §5 figure form (label `fig-…` from cell options, caption from `fig-cap`); table → the
  §9 table form; `text/latex` → raw; `text/markdown` → blocks; errors → `Verbatim` with ANSI stripped (opt-in
  `[latex] ansi_colors = true` → `writercode` with `\textcolor[HTML]{…}{…}` runs, verified safe form).
- `writercell` is emitted only when `[latex] cell_prompts = true` (default **false**: plain code block + outputs, which
  is what a paper wants); the default definition prints `In[3]:` as a bold monospace line; the sample template shows a
  grey prompt and the nbconvert tcolorbox look is a documented alternative (`\newenvironment{writercell}[2]{\begin{tcolorbox}[…]\prompt…}{\end{tcolorbox}}`).
- Never emit `\prompt`, `\PY`, `\adjustimage` or tcolorbox from the body — those belong to templates.

---

## 9. IR → LaTeX mapping (for the spec) **[design, each row's target form compiled in §3.5 or measured in pandoc's output]**

Blocks (IR names from R9 §16 / R5 §8 / R10 §9):

| IR block | LaTeX |
|---|---|
| `Header{level, id, classes, inlines}` | `\<cmd>{inlines}\label{<id>}`; cmd by `top_level`: section→ `section subsection subsubsection paragraph subparagraph` (level 6 → `\subparagraph` too, with a warning), chapter → `chapter section … subparagraph`, part → `part chapter … paragraph`; `.unnumbered`/`{-}` → starred form + `\addcontentsline{toc}{<cmd>}{inlines}`; id missing → auto id (pandoc rules, `toLabel` sanitising: keep `[A-Za-z0-9_\-+=:;.]`, else `ux<hex>`) |
| `Para{inlines}` | inlines + blank line |
| `Plain` (tight list item) | inlines, no blank line |
| `Figure{src, caption, id, attrs}` | `\begin{figure}[<placement>]\n\centering\n\includegraphics[<size>]{figures/<file>}\n\caption{<caption>}\label{<id>}\n\end{figure}`; `\caption` omitted when caption empty and id absent; `\label` only with an id; SVG → converted PDF; PDF/PNG/JPG copied; GIF/WEBP → warning + PNG rendition via `image` crate **[unverified]** |
| `DisplayMath{tex}` (unlabelled) | `\[\n<tex>\n\]` (tex emitted verbatim; `aligned`/`cases` etc. live inside) |
| `DisplayMath{tex, id}` (labelled/numbered) | `\begin{equation}\n<tex>\n\label{<id>}\n\end{equation}`; if `tex` starts with `\begin{align` → emit it bare and inject `\label` after the first line **[design]** |
| `Table{aligns, header, rows, caption, id, col_widths}` | `\begin{longtable}[]{@{}<spec>@{}}` + optional `\caption{…}\label{…}\tabularnewline` + `\toprule\noalign{}` header `\midrule\noalign{}` `\endfirsthead` … `\endhead` `\bottomrule\noalign{}` `\endlastfoot` rows `\end{longtable}` (pandoc's layout verbatim); `spec` from aligns `l c r`, default `l`; with `col_widths` → `>{\raggedright\arraybackslash}p{<w>\linewidth}` **[design, pandoc uses calc expressions]**; `[latex] tables = "tabular"` → `\begin{table}[htbp]\centering\caption…\begin{tabular}{…}` (floats, no page breaks); `booktabs = false` → `\hline` instead of `\toprule`/`\midrule`/`\bottomrule`; colspan/rowspan (notebook HTML tables) → `\multicolumn{n}{<align>}{…}` / `\multirow{n}{*}{…}` + warning that `multirow` must be loaded |
| `CodeBlock{lang, text, attrs}` | §6.2 by `code` mode; `#id` → `\label` via the caption macro or `\phantomsection\label{}` before the block (pandoc's form) |
| `BlockQuote{blocks}` | `\begin{quote}\n…\n\end{quote}` |
| `Callout{kind, title, blocks}` | `\begin{writercallout}{<kind>}{<title>}\n…\n\end{writercallout}`; kind lower-cased from the five alert kinds, unknown kinds → `note` + warning |
| `Div{name, attrs, blocks}` | content only; id → `\phantomsection\label{<id>}` before it (pandoc); known names handled elsewhere |
| `PageBreak` | `\writerpagebreak` |
| `Toc` | `\tableofcontents` (templates usually place it themselves; emit only when the directive is in the body) |
| `BulletList{tight, items}` | `\begin{itemize}` + `\tightlist` when tight + `\item\n  …` + `\end{itemize}` (pandoc indentation) |
| `OrderedList{start, delim, style, tight}` | `\begin{enumerate}` + `\def\labelenum<i|ii|iii|iv>{\arabic{enum<i>}.}` (or `)` delim; `\roman`/`\alph` when the IR knows the style) + `\setcounter{enum<i>}{start-1}` when start ≠ 1 + `\tightlist` + items; nesting depth → `enumi/enumii/enumiii/enumiv` (max 4 — deeper → warning, no counter line) |
| task items | `\item[$\square$]` / `\item[$\boxtimes$]` |
| `DefinitionList` | `\begin{description}` + `\tightlist` + `\item[<term>]\n<def blocks>` + `\end{description}` |
| `HorizontalRule` | `\writerhrule` |
| `RawBlock{format: latex|tex}` | verbatim; other formats dropped with a warning (`html` raw blocks are dropped silently like pandoc) |
| `Footnote` definitions | inlined at the reference: `\footnote{<blocks>}` (paragraphs separated by a blank line, exactly pandoc's layout); nested footnote → warning + `\footnote` inside `\footnote` (works in LaTeX but renders oddly) |
| `Bibliography` block / end of document | `\printbibliography` (biblatex), `\bibliographystyle{<style>}\bibliography{<names>}` (natbib), `\bibliography{<names>}` (plain) — **only when `[latex] emit_bibliography = true`** (default false: the template owns it, as in the sample) |
| `CodeCell` | §8.2 |
| `Include` | already resolved before the backend |

Inlines:

| IR inline | LaTeX |
|---|---|
| `Str` | escaped text (§2.2 TextString rules) |
| `Space`, `SoftBreak` | space / newline (wrap at 72 columns like pandoc, never inside a `\verb`-like argument; `[latex] wrap = false` emits one paragraph per line) |
| `LineBreak` | `\\` + newline |
| `Emph`, `Strong` | `\emph{…}`, `\textbf{…}` |
| `Strikethrough` | `\sout{…}` (ulem) |
| `Underline` | `\uline{…}` (ulem) |
| `Highlight` | `\colorbox{yellow}{…}`? No colour choices in the body: `\writerhighlight{…}` with `\providecommand{\writerhighlight}[1]{\colorbox{yellow}{#1}}` in the preamble **[design, not compiled]** |
| `Superscript`, `Subscript` | `\textsuperscript{…}`, `\textsubscript{…}` |
| `SmallCaps` | `\textsc{…}` |
| `Code{text}` | `\texttt{<CodeString-escaped>}` (spaces → `\ `; pandoc); in `listings` mode `\lstinline<d>…<d>` |
| `Link{inlines, url}` external | `\href{<url, URLString-escaped>}{<inlines>}`; autolink (text == url) → `\url{<url>}` |
| `Link` to `#id` | `\hyperref[<id>]{<inlines>}` |
| `Image` inline | `\includegraphics[<size>]{figures/<file>}` |
| `Math{inline, tex}` | `\(<tex>\)` (tex verbatim) |
| `RawInline{latex}` | verbatim; other formats dropped |
| `Note` (footnote ref) | `\footnote{…}` at the reference position |
| `Cite` | R5 §7 table (`\autocite`, `\textcite`, `\autocites[p.~12]{a}{b}`, `\autocite*`, `\citeauthor`, `\nocite`; natbib `\citep`/`\citet`/`\citetext{\citealp…}`/`\citeyearpar`; plain `\cite`) |
| `Ref{target, supplement}` resolved as `{kind, number, supplement}` | `<Supplement>~\ref{<target>}` (`Figure~\ref{fig:plot}`, `Equation~\eqref{eq:e1}` — `\eqref` for equations), `[latex] cleveref = true` → `\cref{<target>}` / `\Cref` at sentence start, and `\cpageref` for `form: Page` (otherwise `page~\pageref{}`) |
| `Attr` spans `[x]{#id}` | `\phantomsection\label{<id>}{<inlines>}` (pandoc) |
| non-breaking space U+00A0 | `~` |

Labels: writer ids may contain `:` (`fig:plot`), which `\label` accepts; `\ref` targets must match exactly. Ids with
characters outside `[A-Za-z0-9_\-+=:;.]` are sanitised with pandoc's `ux<hex>` rule in *both* `\label` and `\ref`.

---

## 10. Escaping rules (normative for the backend) **[verified against pandoc; writer's variant is design]**

Text (`Str` in TextString context), per character, in order:

```
\  → \textbackslash{}     {  → \{        }  → \}        $ % & _ #  → \$ \% \& \_ \#
~  → \textasciitilde{}    ^  → \textasciicircum{}   (pandoc: \^{} — both fine)
|  → \textbar{}           <  → \textless{}          >  → \textgreater{}
[  → {[}                  ]  → {]}
-- (two literal hyphens) → -\/-   (ligature guard; only when they come from the source text, not from dashes)
U+00A0 → ~      U+00AD → \-     U+200B → \hspace{0pt}    U+202F → \,
U+2026 → \ldots{}   U+2018 → `   U+2019 → '   U+201C → ``   U+201D → ''   (insert \, between two adjacent quote marks)
U+2014 → ---    U+2013 → --
?` and !` → ?{\kern0pt}` / !{\kern0pt}`
' (straight) → '   in text (pandoc: \textquotesingle{} — only matters in code);  " → "   (unchanged, pandoc)
everything else → unchanged UTF-8 (then the optional unicode map of §7.2)
```

Always emit `{}` after a control word (`\textbackslash{}`), never the bare `\textbackslash` + space form.

Code (`Code` inline → `\texttt{}`): same table plus `'` → `\textquotesingle{}`, `` ` `` → `\textasciigrave{}`,
space → `\ ` (keeps runs of spaces), and no quote/dash/ellipsis mapping. Code blocks in `Verbatim`: no escaping at
all. `writercode`: only `\ { }` escaped (`\textbackslash{}`, `\{`, `\}`).

URLs (`\href`/`\url` argument): `\`→`/`, `#`→`\#`, `%`→`\%`, `{`→`\%7B`, `}`→`\%7D`, `|`→`\%7C`, `^`→`\%5E`,
`[`→`\%5B`, `]`→`\%5D`, `` ` ``→`\%60`; everything else (including `_ & ~ ? =`) unchanged.

Captions, `\item[…]`, `\section{…}`: text rules; additionally a caption/heading containing `]` inside an optional
argument must be braced (`\item[{Term [x]}]`) — writer wraps `\item[{…}]` whenever the term contains `[` or `]`;
`\section[short]{long}` is never used. Fragile content in moving arguments (footnotes inside headings/captions) →
`\protect\footnote` **[design; pandoc emits `\protect\phantomsection` similarly]**.

File paths for `\includegraphics`, `\addbibresource`: ASCII, no spaces, forward slashes (writer renames on copy);
otherwise quote with `{"…"}` **[unverified]**.

---

## 11. Config knobs **[design]**

```toml
[latex]
template = "templates/paper.tex"   # BYO template path; default: writer's bundled sample (§3.4)
top_level = "section"              # "section" (default) | "chapter" | "part"; auto-promote to "chapter" when the template's \documentclass is book/report/memoir? no — explicit only (pandoc's metadata heuristic is surprising)
cite = "biblatex"                  # "biblatex" (default) | "natbib" | "plain" | "rendered" (hayagriva pre-rendered text + plain \bibitem list, like pandoc --citeproc)
emit_bibliography = false          # true → writer appends \printbibliography / \bibliographystyle+\bibliography at the end of the body
code = "verbatim"                  # "verbatim" (default) | "highlight" (syntect → writercode + \Wr* macros) | "listings"
booktabs = true                    # false → \hline rules
tables = "longtable"               # "longtable" (default) | "tabular" (floating \begin{table})
cleveref = false                   # true → \cref/\Cref/\cpageref instead of "Figure~\ref{}"
engine = "pdflatex"                # "pdflatex" (default) | "xelatex" | "lualatex" — only selects the unicode policy and the warnings; writer never runs TeX
unicode = "map"                    # "map" (default for pdflatex) | "keep" (default otherwise); see §7.2
figure_placement = "htbp"          # "htbp" (default) | "H" (template must load float) | "" (no option)
dpi = 96                           # px → in conversion
wrap = 72                          # line wrap column for prose; 0 = no wrapping
cell_prompts = false               # notebook cells: wrap In/Out in writercell
ansi_colors = false                # notebook outputs: keep ANSI colours as \textcolor[HTML] runs inside writercode
assets_dir = "figures"             # where copied/converted images go, relative to the .tex output
```

CLI: `writer latex doc.md -o out/paper.tex [--template t.tex] [--init-template]`; `--init-template` writes the
bundled sample (biblatex) next to the output and exits. Diagnostics (warnings, not errors): unsupported raw format,
unknown callout kind, deep enumerate nesting, non-ASCII in code under pdflatex, listings language without a
listings definition, `.svg` text fonts not found, GIF/WEBP images, nested footnotes, `Ref` to a page in HTML/DOCX
style, template missing one of the hooks (informational), `\end{Verbatim}` inside a code block.

---

## 12. Verified API hints (copy-paste)

```toml
# Cargo.toml
minijinja = { version = "3.0", features = ["serde", "loop_controls"] }    # 2.24.0: features = ["custom_syntax", "loop_controls"]
svg2pdf   = "0.13.0"                                                       # default features image+filters+text; brings its own usvg 0.45 / resvg 0.45 / fontdb 0.23 / pdf-writer 0.12
# writer's own rasterising (PNG renditions for DOCX/HTML) — match typst 0.15.1 to avoid a third usvg:
usvg  = { version = "0.47", features = ["text"] }
resvg = "0.47"
```

```rust
// ---- template rendering (minijinja 3.0.3; the 2.24.0 differences are marked) ----
use minijinja::{Environment, AutoEscape, UndefinedBehavior, syntax::SyntaxConfig, value::{Serde, Value}};

pub fn latex_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_syntax(
        SyntaxConfig::builder()
            .block_delimiters("\\BLOCK{", "}")
            .variable_delimiters("\\VAR{", "}")
            .comment_delimiters("\\#{", "}")
            .keep_trailing_newline(true)        // 2.24.0: env.set_keep_trailing_newline(true) instead
            .build()
            .expect("valid delimiters"),
    );
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_auto_escape_callback(|_name| AutoEscape::None);
    env.add_filter("tex", |s: String| escape_latex_text(&s));     // §10 text rules
    env.add_filter("texpath", |s: String| escape_latex_url(&s));  // §10 URL rules
    env
}

pub fn render_template(env: &Environment<'_>, template_src: String, ctx: serde_json::Value) -> Result<String, minijinja::Error> {
    let mut env = env.clone();
    env.add_template_owned("template.tex", template_src)?;
    let tmpl = env.get_template("template.tex")?;
    tmpl.render(Value::from(Serde(ctx)))                          // 2.24.0: tmpl.render(&ctx) (ctx: impl Serialize)
}
// verified: a template line "% \VAR{title}" still substitutes (TeX comments do not hide tags);
// "\VAR{ {'a': 1}['a'] }" works (nesting tracked inside expressions); a tag ends at its first "}" otherwise.

// ---- SVG → PDF (svg2pdf 0.13.0 / usvg 0.45.1 / fontdb 0.23.0) ----
pub fn svg_to_pdf(svg: &str, fonts_dir: Option<&std::path::Path>) -> Result<Vec<u8>, String> {
    let mut options = svg2pdf::usvg::Options::default();        // verified defaults: dpi 96.0, font_family "Times New Roman", font_size 12.0, languages ["en"]
    options.fontdb_mut().load_system_fonts();                    // or load_fonts_dir / bundled fonts for reproducibility
    if let Some(d) = fonts_dir { options.fontdb_mut().load_fonts_dir(d); }
    let tree = svg2pdf::usvg::Tree::from_str(svg, &options).map_err(|e| e.to_string())?;   // Tree::from_data(&[u8]) accepts .svgz
    let _size = tree.size();                                     // usvg::Size, width()/height() in CSS px == PDF pt at dpi 72
    svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions::default())
        .map_err(|e| e.to_string())                              // ConversionError implements Display, not std::error::Error
}
// measured: 320x120 SVG with Helvetica/Courier text and "→ α ≥" → 35,574-byte PDF 1.7, MediaBox [0 0 320 120],
// subset fonts Helvetica, CourierNewPSMT, ArialUnicodeMS (fallback), 0.17 s; \includegraphics[width=0.6\linewidth]{diagram-text.pdf} compiled on pdflatex/xelatex/lualatex.
```

```latex
% ---- the body forms that compiled on all three engines (copy from …/latex-probe/r7/body.tex) ----
\section{Introduction}\label{sec:intro}
\begin{figure}[htbp]
\centering
\includegraphics[width=0.5\linewidth]{figures/plot.png}
\caption{A PNG plot with \emph{emphasis} and 50\%.}\label{fig:plot}
\end{figure}
\begin{longtable}[]{@{}lcrl@{}}
\caption{A table caption with 50\%.}\label{tbl:t1}\tabularnewline
\toprule\noalign{}
Left & Center & Right & Default \\
\midrule\noalign{}
\endfirsthead
\toprule\noalign{}
Left & Center & Right & Default \\
\midrule\noalign{}
\endhead
\bottomrule\noalign{}
\endlastfoot
a \& b & 1 & 2.5 & x\_y \\
\end{longtable}
\begin{writercallout}{note}{Note}
A callout body with \emph{inline} markup.
\end{writercallout}
\begin{Verbatim}
def f(x):
    return {"a": x & 1}  # 100% \sure_ ~^ 'q' "d"
\end{Verbatim}
\begin{writercode}
\WrKeyword{def} \WrFunction{f}(x):
    \WrKeyword{return} \{\WrString{"a"}: x \WrOperator{\&} \WrNumber{1}\}  \WrComment{\# 100\% \textbackslash{}sure\_}
\end{writercode}
\begin{lstlisting}[language=Python,caption={A listings block},label=lst:main]
def g(y):
    return y
\end{lstlisting}
\begin{writercell}{In}{3}
\begin{Verbatim}
np.arange(3)
\end{Verbatim}
\end{writercell}
\begin{itemize}
\tightlist
\item[$\square$] task open
\item[$\boxtimes$] task done
\end{itemize}
\begin{enumerate}
\def\labelenumi{\arabic{enumi}.}
\setcounter{enumi}{4}
\tightlist
\item starts at five
\end{enumerate}
\begin{description}
\tightlist
\item[Term one] Definition one
\end{description}
\[
\begin{aligned}
a &= b + c \\
d &= e
\end{aligned}
\]
\begin{equation}
e^{i\pi} + 1 = 0 \label{eq:euler}
\end{equation}
See \textcite{doe2020} and \autocites[p.~12]{knuth1984}{lee2019}. Also \autocite*{doe2020} and \autocite{doe2020,lee2019}.   % natbib: \citet, \citetext{\citealp[p.~12]{knuth1984}; \citealp{lee2019}}, \citeyearpar, \citep
A footnote.\footnote{A footnote with \textbf{strong} and a citation \autocite{knuth1984}.}
See Figure~\ref{fig:plot}, Table~\ref{tbl:t1}, Section~\ref{sec:intro}, Listing~\ref{lst:main} and Equation~\eqref{eq:euler}.
\href{https://example.com/a\%20b\#frag?q=x&y=_z~w}{a link}, \url{https://example.com/auto}
\sout{strikeout}, \uline{underline}, H\textsubscript{2}O, x\textsuperscript{2}, \textsc{small caps}, \texttt{code\_with\{braces\}\textbackslash{}}
\writerhrule
\writerpagebreak
```

```sh
# build (what the docs tell users; all verified here)
latexmk -pdf      -interaction=nonstopmode -halt-on-error -outdir=build paper.tex    # pdflatex (+ bibtex/biber automatically)
latexmk -xelatex  -interaction=nonstopmode -halt-on-error -outdir=build paper.tex
latexmk -lualatex -interaction=nonstopmode -halt-on-error -outdir=build paper.tex
grep "Missing character" build/paper.log      # xelatex/lualatex glyph coverage check
# pandoc reference commands used in this note
pandoc --print-default-template latex
pandoc input.md -t latex [--top-level-division=chapter] [--syntax-highlighting=idiomatic|none] [--natbib|--biblatex|--citeproc] [-s] [--wrap=none]
```

---

## 13. Open questions

1. **biber** is not installed here, so biblatex was only exercised with `backend=bibtex`; the `\autocites`/`\textcite`
   forms are style-independent, but a real `biber` run (and `style=ieee`, which bibtex backend cannot do) is unverified.
2. `\textcolor[rgb]{…}` failing inside a commandchars `Verbatim` while `[HTML]` works: cause not analysed (nbconvert's
   `\detokenize` workaround suggests catcode-sensitive comma/number parsing in xcolor's rgb path). The macro form is
   used instead; if the task wants exact theme colours, use `[HTML]`.
3. Highlighted-code scope → class table (§6.2) is a design proposal; it needs a syntect scope survey on real samples
   (R6 has the syntect API) and a snapshot per language.
4. Captioned listings in `verbatim`/`highlight` mode (`\writerlistingcaption`) and `\writerhighlight` for `==mark==`
   were not compiled.
5. Relative column widths for tables (pandoc's `p{(\linewidth - 2\tabcolsep) * \real{0.96}}` + minipage cells) and
   `\multicolumn`/`\multirow` for notebook HTML tables were not compiled; v1 may emit plain `l/c/r` columns and warn.
6. Whether `top_level` should auto-promote to `chapter` when the template is a `book` class: pandoc does it from
   metadata; writer cannot read the template's `\documentclass` reliably (minijinja template, not LaTeX). Proposal: a
   template declares it via a comment tag `\#{ writer: top_level=chapter }`? Simpler: config only.
7. svg2pdf's duplicate `usvg` (0.45 vs typst's 0.47): acceptable now; revisit when svg2pdf releases a usvg 0.47+ build,
   or switch to the typst-compile route of §5.2 if the extra 0.45 copy bloats compile time (not measured in isolation;
   `cargo build --release` of the probe incl. svg2pdf's tree took under the 600 s tool timeout and most of it was shared).
8. Bundled fonts for SVG text: `typst-assets` font bytes → `fontdb::Database::load_font_data(Vec<u8>)` (signature
   verified) would make SVG→PDF reproducible without system fonts; the end-to-end result (which family usvg picks for
   `Helvetica` when only the typst fonts are loaded) needs a probe.
9. Line wrapping of prose at 72 columns (pandoc default) vs none: wrapping must never break inside `\verb`-like
   arguments, `\href{}` arguments or `writercode` lines; decide `wrap = 0` as the safe default if the wrapper is not
   argument-aware.
10. `Highlight` (`==x==`) and `Insert`/`Underline` inline kinds from comrak (R9) need final macro names; `\uline` and
    `\sout` compiled; `\writerhighlight` not.
