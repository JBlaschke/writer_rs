# writer — implementation plan

`writer` is a single Rust binary that turns documents written in a Markdown dialect, Jupyter
notebooks and typst sources into **HTML**, **PDF**, **Word (`.docx`)** and **LaTeX source**.
While you write, `writer serve` watches the sources, the theme and the templates and pushes
every save to the browser: as a web page, or as the typeset pages the PDF will have.

It is the document-shaped sibling of [`presenter_rs`](../presenter_rs) (same author, same
conventions, much of the code lifted from it). Where presenter has one engine (reveal.js),
writer has two authoring backends and four outputs, so the centre of the design is a
**structured document IR** that every frontend produces and every emitter consumes.

This document is the master plan. It is written so that a fleet of independent, relatively
simple agents can each pick up one task file under `docs/tasks/` and complete it without the
whole picture. The contracts those tasks depend on live in `docs/spec/` and are frozen before
the fan-out begins. The facts the plan rests on were verified on 2026-10-24 and are recorded,
with sources, under `docs/research/` (§9 lists the ones that drive decisions). The plan was
reviewed adversarially before freezing; the review's findings are folded in.

---

## 1. Goals and non-goals

### Goals
1. **Two authoring backends.**
   - *Markdown* (light): `.md` files and `.ipynb` notebooks in the presenter_rs dialect,
     extended for documents (figures with captions and labels, callouts, cross-references,
     footnotes, includes, page breaks, raw passthrough). Quick to write; limited control over
     typesetting.
   - *typst* (heavy): a `.typ` file that imports writer's `@writer/doc` package. PDF comes
     from the real typst compiler with full fidelity. A documented **structured subset** of
     typst (markup plus an allow-list of calls with literal arguments) is what the HTML, DOCX
     and LaTeX exports understand; anything outside it still compiles to PDF but is reported
     for the other targets.
2. **Four outputs** from one source: single-file HTML (theme, assets inlined, offline), PDF
   (typst compiler embedded in the binary, no browser or TeX needed), `.docx` (native, editable,
   styled through a pandoc-compatible reference document), and LaTeX source (`<stem>.tex` plus
   an assets directory, produced by a **bring-your-own template**; no styling exported; the user
   runs `latexmk`). Plus `.typ` source export, so a Markdown document can graduate to the heavy
   backend.
3. **Live preview.** `writer serve` rebuilds on every save of the document, its includes,
   images, bibliography, theme and PDF template, and updates the browser in place: the reader's
   place on the page is kept, the edited block is scrolled into view when it is off screen, and
   errors show in an overlay with file, line and excerpt. Two views: `web` (the HTML output) and
   `pages` (the PDF's pages as SVG, only changed pages re-fetched). Each view keeps its last good
   state when only the other one fails.
4. **One IR, consistent results.** Figure/table/equation/section numbers, cross-reference text,
   citations and bibliography entries are identical across HTML, DOCX and PDF (the typst template
   reproduces writer's numbering policy; typst and writer share the same citation engine,
   `hayagriva`). LaTeX numbers and cites itself, as its users expect.
5. **Deterministic, testable builds.** Same inputs → byte-identical HTML, DOCX, `.tex` and
   `.typ`; byte-identical PDF with a fixed timestamp and system fonts off. Golden snapshots for
   IR, HTML, typst and LaTeX; structural validation for DOCX.
6. **Fast iteration.** A typical document rebuilds in well under a second in `serve`: the serve
   loop keeps one typst `World` alive and edits its sources in place, so the compiler recompiles
   a 25-page document in ~10 ms after an edit (measured, R1); the Markdown walker and the HTML
   emitter are cheaper still.
7. **Single binary, nothing to install.** Fonts for PDF are embedded (the typst compiler's
   Libertinus Serif, New Computer Modern, New Computer Modern Math and DejaVu Sans Mono; system
   fonts are scanned in addition unless `[pdf] system_fonts = false`). Network is never used
   unless the user opts in (`[pdf] allow_packages` for typst `@preview` packages).

### Decisions already taken
- License MIT (`LICENSE` in the repo root; remote `origin` is github.com/JBlaschke/writer_rs).
  Vendored: KaTeX 0.19.1 (MIT; its fonts OFL-1.1, recorded as `MIT AND OFL-1.1`), morphdom
  2.7.8 (MIT), the `mitex` typst package (Apache-2.0), Latin Modern Math 1.959 (GUST Font
  License; for `[math] mathml_font = "embed"`), the fonts and data the `typst-assets` crate embeds (per-asset licenses enumerated in
  `docs/spec/vendored-assets.md`), two-face syntax and theme definitions (per-file licenses
  listed by the crate), a Unicode→LaTeX symbol table derived from tylax (Apache-2.0). Every
  vendored work ships its license.
- Single crate (lib + bin), module ownership per task, no workspace, no cargo feature for the
  compiler (PDF is a core deliverable; the typst crates compile once and are cached).
- **Lift, don't link:** writer copies code from presenter_rs (MIT, same author) rather than
  depending on it. `docs/research/R8-presenter-inventory.md` §10 is the copy plan; each task
  file names what to copy and what to change.
- The Markdown dialect stays **source-compatible with presenter_rs** where the two overlap
  (front matter, attribute grammar, `:::` directives, code-fence attributes, math, includes,
  alerts), so a notebook or a Markdown file can feed both tools. `---` is a thematic break;
  presenter's `--` separators and slide shorthands are dropped with a diagnostic; `:::notes`
  and `:::comment` are dropped from every output (`docs/spec/markdown-dialect.md` §9).
- **Citations and bibliographies are a deliberate extension** beyond the original brief
  (includes, images, sections, callouts, highlighting, math, four outputs, serve). They are
  layered: nothing in M1–M6 needs `hayagriva`; T70 adds the bibliography lookup, rendering and
  the "label or key?" rule on top of a resolve pass that otherwise reports unknown `@x` targets.
- One rendering engine for everything raster or vector that is not the browser: the embedded
  typst compiler renders equation images (SVG/PNG with baseline), converts SVG/GIF/WebP figures
  to PDF (LaTeX) or PNG (DOCX fallback), and produces the pages view. No `resvg`, no `svg2pdf`,
  no `image` dependency of writer's own.

### Non-goals (for this plan)
- Executing notebooks (saved outputs are rendered; no Python at run time).
- Running LaTeX. writer emits `.tex`; it never calls `latexmk` (tests may, when present).
- Converting arbitrary typst to DOCX/LaTeX. Only the documented subset is structured; the
  escape hatch for the rest is "PDF only" (rasterising unsupported constructs is a stretch).
- Interactive outputs (plotly, widgets) beyond their text fallbacks; multi-document books;
  sub-figure groups with lettered sub-captions; a WYSIWYG editor; a hosted service; EPUB.
- Reading `.docx`/`.tex` as inputs.

---

## 2. User experience (what we are building toward)

```bash
writer init report                 # report/writer.toml, report/document.md, figures/, a theme stub (--bib adds refs.bib)
cd report
writer serve document.md --open    # http://127.0.0.1:3000 — web view; "p" or --view pages shows the typeset pages
writer build document.md           # Wrote dist/document.html (58 KB)        ← ≈650 KB when the document has math (KaTeX inlined)
writer build document.md --to pdf,docx,latex
                                   # Wrote dist/document.pdf (9 pages, 212 KB)
                                   # Wrote dist/document.docx (48 KB)
                                   # Wrote dist/document.tex + dist/document_assets/ (3 files)
writer build analysis.ipynb --to pdf
writer build paper.typ                       # typst backend: PDF (the default target for .typ) from the compiler
writer build paper.typ --to docx,html        # … and DOCX/HTML from the structured subset
writer build document.md --to typst          # dist/document.typ + dist/document_assets/: continue in typst
writer check document.md --deny-warnings     # lints: unresolved @refs, duplicate labels, missing images, …
writer dump document.md                      # the resolved IR as JSON (also --context, --default-config)
writer eject theme ./theme                   # the HTML theme to hack on
writer eject typst-template ./template.typ   # the PDF template (a show function) to customise
writer eject typst-package ./packages        # @writer/doc and the vendored mitex as a typst package path
writer eject docx-reference ./reference.docx # Word styles to restyle in Word
writer eject latex-template ./template.tex   # the sample LaTeX template
```

### 2.1 Markdown in one screen (full spec: `docs/spec/markdown-dialect.md`)

````markdown
---
title: GPU allocation at the institute
authors: [J. Blaschke, A. Colleague]
date: 2026-10-24
abstract: |
  One paragraph that every output renders as the abstract.
bibliography: refs.bib
cite_style: ieee
toc: true
numbering: { headings: "1.1", figures: section }
---

# Introduction {#sec-intro}

Demand doubled last year [@knuth1984; @lamport1994], see @fig-throughput and
@tbl-results. Details are in @sec-method and a footnote[^1]. Write `\@` for a literal at sign.

![Throughput per node, 2025–2026](figures/plot.png){#fig-throughput width=70%}

Table: Benchmark results {#tbl-results}

| nodes | GB/s |
|------:|-----:|
|     1 | 12.3 |
|     8 | 91.0 |

> [!NOTE] Reproducibility
> Every run used the pinned container image.

:::warning title="Check the quota first"
Requests above 8 nodes need sign-off.
:::

```python {#lst-load caption="Loading the data" .numbered}
import polars as pl
df = pl.read_csv("runs.csv")
```

$$ T(n) = \frac{W}{n} + c $$ {#eq-amdahl}

## Method {#sec-method}

:::include analysis.ipynb
:::

```{=latex}
\clearpage
```

:::pagebreak
:::

[^1]: Footnotes work in every output.
````

Labels use `kind-name` by convention (`fig-`, `tbl-`, `eq-`, `sec-`, `lst-`): that is what
quarto uses in notebooks and it keeps labels out of the one place comrak forbids colons (the
`:::` directive line; `fig:name` works everywhere else and is accepted). `@target` is a
reference when the target is a label in the document and a citation when it is a key in the
bibliography — typst's rule; being both is an error, being neither is an error in `build` and a
`[?target]` placeholder with a warning in `serve` and `check`.

### 2.2 typst in one screen (full spec: `docs/spec/typst-dialect.md`)

```typst
#import "@writer/doc:0.1.0": *
#show: writer-doc.with(
  title: [GPU allocation at the institute],
  authors: ("J. Blaschke", "A. Colleague"),
  date: "2026-10-24",
  abstract: [One paragraph.],
  bibliography: path("refs.bib"), cite-style: "ieee",   // a plain string fails: typst resolves it inside the package
  toc: true, numbering: (headings: "1.1", figures: "section"),
)

= Introduction <sec-intro>
Demand doubled last year @knuth1984 @lamport1994, see @fig-throughput and @tbl-results.

#figure(image("figures/plot.png", width: 70%), caption: [Throughput per node]) <fig-throughput>

#figure(table(columns: 2, align: right, table.header[nodes][GB/s], [1], [12.3], [8], [91.0]),
        caption: [Benchmark results]) <tbl-results>

#callout(kind: "warning", title: [Check the quota first])[Requests above 8 nodes need sign-off.]

#figure(```python
import polars as pl
```, caption: [Loading the data]) <lst-load>

$ T(n) = W / n + c $ <eq-amdahl>

== Method <sec-method>
#include "analysis.ipynb"      // a writer convenience: the notebook is served to the compiler as typst
#pagebreak()
```

The structured exports understand exactly this vocabulary (`writer check paper.typ` lists the
constructs that only the PDF will carry). The file compiles with the plain `typst` CLI given
`--package-path` of an ejected package directory, except for `#include` of `.md`/`.ipynb`
files, which only writer's World serves; `writer build --to typst` materialises those
includes as sibling `.typ` files so an exported tree is plain typst.

---

## 3. Architecture

```
            ┌─────────────────────────────────────────────────────────────────────────────┐
 inputs     │  frontends                                                                  │
 .md ──────►│  markdown::{frontmatter, walker, directives, refs, include}  ─┐             │
 .ipynb ───►│  notebook::{nbformat, convert, options, tables, ansi}        ─┼─► Document IR + AssetRegistry
 .typ ─────►│  typst::frontend  (typst-syntax → IR, the structured subset) ─┘             │
            └───────────────────────────────────────────────────────────────┬─────────────┘
                                                                            ▼
            ┌─────────────────────────────────────────────────────────────────────────────┐
            │  resolve::{labels, numbering, footnotes, toc}  (+ cite::* when a bibliography is configured)
            └───────────────────────────────────────────────────────────────┬─────────────┘
              ┌──────────────┬──────────────────┬───────────────────────────┼────────────────┐
              ▼              ▼                  ▼                           ▼                ▼
       html::emit      typst::emit         docx::{body,…}            latex::emit         (dump)
       + theme chain   IR → .typ source    IR → OOXML parts          IR → document.tex   IR JSON
       (minijinja)     ───────────────┐    + reference-doc merge     + minijinja template
       + assets::emit                 ▼    + OMML / equation images  + assets (SVG→PDF by the compiler)
       ▼                    typst::{world, compile, render}  ◄───── .typ (the user's own file, typst backend)
   HTML (single file)       embedded compiler: fonts, @writer/doc, @preview/mitex, virtual assets
                            ▼                 ▼                    ▼
                           PDF           SVG pages (serve)    equation and figure renditions (SVG/PNG/PDF)

 serve: serve::watch (notify) → pipeline rebuild in memory (one long-lived World) → serve::server (axum, SSE)
        GET /  GET /__writer/theme.css  GET /__writer/events  GET /__writer/pages  GET /__writer/pages/{n}.svg?v=<hash>
        client: morphdom patches [data-writer-document], keeps the reader's place, scrolls to the edited block when off screen
```

### 3.1 Pipeline stages
1. **Parse** — a frontend turns one source file (and its includes) into a `Document`
   (`docs/spec/ir.md`) plus an `AssetRegistry` (images and binary outputs referenced through
   `asset:<id>` placeholders, lifted from presenter). Math keeps its source dialect
   (`latex` or `typst`). References and citations are parsed but not resolved.
2. **Resolve** — one pass over the IR (`docs/spec/references.md`): collect labels, apply the
   numbering policy (headings, per-kind figure counters, equations; per-document or
   per-section), resolve every `@ref` (kind, number, localized supplement), number footnotes,
   build the TOC model. With a bibliography configured (T70), `@key` targets become citations,
   rendered to inlines by `hayagriva`, and the bibliography entries are produced. All diagnostics
   about references are raised here, once, for every target.
3. **Emit** — one emitter per target consumes the resolved IR:
   - `html::emit` (markup contract in `docs/spec/html-output.md`) → body HTML; the theme's
     `document.html` (minijinja) wraps it; `assets::emit` inlines everything (single file).
   - `typst::emit` → `.typ` source that imports `@writer/doc`; `typst::compile` turns it into
     a PDF or SVG pages. For the typst backend the user's `.typ` is compiled directly.
   - `docx::*` → OOXML parts, merged with the reference document's styles, zipped.
   - `latex::emit` → the body and a generated preamble, rendered through the template.
4. **Write / Serve** — `pipeline` writes `dist/<stem>.<ext>` (never over an input, presenter's
   clash check) or keeps the outputs in memory for the dev server.

**Math** (`docs/spec/math.md`): LaTeX math reaches the PDF through the vendored `mitex` typst
package (converted at compile time by the package's WASM plugin, which the compiler runs);
LaTeX math reaches HTML through KaTeX (client-side, lifted from presenter; `[math] html =
"mathml"` renders MathML with `math-core` instead) and DOCX through `math-core` MathML → writer's
own OMML writer; typst math reaches HTML and DOCX through the compiler's own MathML export
(`typst-html`) and LaTeX through writer's typst→LaTeX converter; every DOCX equation the OMML
writer cannot map becomes an equation image rendered by the compiler with its baseline.

### 3.2 Source layout and file ownership (single crate, library + binary)

The task in the right column owns the path; a later task listed after `→` takes it over. A
task may touch another task's file only additively (new function, new test, one new match arm)
and must say so in its PR; `src/pipeline/mod.rs` and `src/cli/mod.rs` are the exception: T16
writes them once with every dispatch arm and command present, later tasks replace only the body
of their stub (§6).

```
writer_rs/                                                                    owner
├── Cargo.toml  build.rs  clippy.toml  rustfmt.toml  .cargo/  .gitattributes   T00 (complete dependency list with pins; no other task edits Cargo.toml)
├── PLAN.md  CHANGELOG.md  README.md  LICENSE                                   coordinator (README: T74)
├── docs/spec/*.md  docs/tasks/*.md  docs/research/*.md                        coordinator
├── docs/guide/*.md  docs/dev/*.md                                             T74 (dev/docx-validation.md: T43; dev/release.md: T75; dev/build-times.md: T00; dev/live-reload-qa.md: T22, T24 additive)
├── .github/workflows/{ci,release,site}.yml                                    T00 (release/site: T75)
├── scripts/vendor.sh  assets/vendor/**  assets/vendor/VERSIONS.toml  assets/licenses/**   T00 (KaTeX, morphdom, mitex package, Latin Modern Math, licenses)
├── scripts/gen-unicode-latex.py  scripts/unicode-latex-overrides.toml         T51
├── assets/writer/live-reload.js: T22   pages.js: T21 (placeholder) → T24   math.js: T00 (one-line placeholder) → T1A
├── assets/l10n/**                                                              T01 (en.toml, with src/resolve/l10n.rs)
├── assets/themes/default/{theme.toml, templates/**}                           T15 → T19;  styles/**, README.md: T19
├── assets/typst/writer-doc/**   (@writer/doc: typst.toml, lib.typ, callout.typ, numbering.typ, cells.typ, oracle.typ, template.typ)   T31 (cells.typ include-notebook helper: T53, additive)
├── assets/docx/{styles.xml, numbering.xml, settings.xml}                       T04
├── assets/latex/writer-template.tex                                            T61
├── assets/scaffold/**                                                          T72
├── examples/basic/  examples/notebook/  examples/typst/  examples/mixed/  examples/latex-template/   T16, T18, T52, T1B, T61
├── src/main.rs  src/lib.rs  src/vendor.rs                                     T00
├── src/error.rs  src/ir.rs  src/config.rs                                     T01
├── src/assets/registry.rs                                                      T01;   src/assets/{mod,emit}.rs: T14
├── src/markdown/mod.rs (stub: T01) → T12   frontmatter.rs: T10   code.rs: T01 (T11 adds invalid-code-attr)   {options,attrs,recover,directives}.rs: T11 (options.rs: comrak_options; directives.rs: info parsing + colon pre-pass)   walker.rs: T12   {refs,include}.rs: T1B (include::scan_includes: T16 lifts it first, additive, for T20)
├── src/notebook/mod.rs (stub: T01) → T18   {nbformat,convert,options,outputs,tables,ansi}.rs: T18
├── src/typst/frontend.rs (stub: T01) and private frontend/{walker,literals,metadata,calls,tables,normalize}.rs → T50   {mod,world,compile,package,render,html,sourcemap}.rs: T03 (package.rs: T31 adds eject, install_dir)   emit.rs: T30   includes.rs: T53 (`MarkdownIncludes` stub: T16)
├── src/resolve/mod.rs (stub: T01) → T17   l10n.rs: T01   {labels,numbering,footnotes,toc}.rs: T17
├── src/cite/{mod,library,render}.rs  + the resolve hook that fills Cite.resolved   T70
├── src/math/mod.rs: T00 (skeleton)   latex_source.rs: T30   latex_mathml.rs: T1A   omml.rs and private omml/{parse,fold,render,text}.rs: T42   typst_to_latex.rs and private typst_to_latex/{symbols,calls}.rs + assets/math/unicode-latex.toml: T51
├── src/html/{emit,highlight}.rs: T13   {mod,context,document,paths,styles}.rs: T15   math.rs: T1A
├── src/theme/{mod,manifest,resolve,templates}.rs                               T15
├── src/docx/{mod,package,styles,numbering,reference,validate}.rs: T04   {body,inline,footnotes}.rs: T40   {tables,images,fields,toc}.rs: T41   math.rs: T42   custom_style.rs: T43
├── src/latex/{emit,escape}.rs: T60   {mod,template,assets,preamble}.rs: T61
├── src/pipeline/mod.rs (dispatch, every arm): T16   html.rs: T16   pdf.rs: T32   typst.rs (export): T34   docx.rs: T41   latex.rs: T61   typst_source.rs (.typ inputs): T52
├── src/serve/{mod,server,files}.rs: T21   events.rs: T21 → T73 (ErrorPayload bodies)   watch.rs: T20   pages.rs: T21 (types, shell_html, PagesBuilder::build stub) → T24
├── src/check.rs  src/cli/check.rs: T1C (T33 adds the mitex pre-flight, additive) → T71 (typst and per-target lints, additive)
├── src/pdf/devtools.rs (browser-test client): T25   src/pdf/chrome.rs: T77
├── src/cli/mod.rs (clap tree, stubs for every command): T16   build.rs, dump.rs: T16   serve.rs and private serve/session.rs: T23   init.rs, eject.rs: T72   report.rs: T16 (lift) → T73
└── tests/  fixtures/{md,ir,config,bib}: T01 (other tasks add files, named in §7)   ipynb: T18   typ: T50   math: T1A, T42, T51   themes/child-theme: T15   docx/reference.docx: T04 → T43   latex/template.tex: T61   live-reload: T22   serve: T23
            common/docx.rs (read-back helper): T04
            integration tests belong to the task whose feature they test (named in each task file)
```

### 3.3 Technology decisions (and why)

| Decision | Choice | Why (alternatives considered) |
|---|---|---|
| Document model | A structured block/inline IR (pandoc-shaped), never HTML strings | DOCX, typst and LaTeX need structure. presenter's `Block::Html` cannot drive them (R8). |
| Markdown parser | `comrak` 0.57, `default-features = false, features = ["attributes"]` | AST with source positions, attributes on headings/code/links/images, `:::` directives, alerts, footnotes, tables, `$math$`. Its attribute, autolink, table and scanner sources are byte-identical to presenter's 0.56 and R9's probe corpus parses to identical ASTs on both; T11 keeps that corpus as a differential snapshot. |
| Document dialect specifics | Implicit figures (a paragraph that is only an image); `Table:` caption paragraphs; a display equation with a trailing `{#eq-x}` block; empty `:::name` containers as leaf directives; ```` ```{=latex} ```` raw fences; `@target[supplement]` references with `\@` as the escape; `kind-name` labels | What comrak parses unambiguously (R9 §6–7). A pre-pass rewrites `:` on `:::` lines to `&#58;` (R9 §6.2), recorded as an info diagnostic for presenter compatibility. |
| PDF engine | The typst compiler **in-process** (`typst`, `typst-layout`, `typst-pdf`, `typst-svg`, `typst-render`, `typst-kit` with embedded fonts) | Measured: ≈45 MiB total binary of which fonts 9.3 MiB, 58 s cold release build (without `codegen-units = 1`), ~10 ms incremental recompiles, no runtime dependency, pure-Rust crate set (typst-cli ships Windows builds; T00's 3-OS CI verifies). Chrome printing (presenter's driver) is Unix-only and cannot number a TOC; kept as an optional engine (T77). |
| Markdown → PDF | IR → typst source (`@writer/doc` template) → compiler | One typesetting path for both backends; `--to typst` is the migration path to the heavy backend. |
| typst backend exports | Syntax-level subset via `typst-syntax` (no evaluation) for HTML/DOCX/LaTeX; the compiler for PDF | Deterministic, small, no evaluator; typst's own HTML export is experimental and CSS-less (R1 §6, R2 §7). Revisit when it matures (T78). |
| Custom package | `@writer/doc` served from `World::file` (embedded files), no registry or network | The compiler only asks the World (R1 §7). `writer eject typst-package DIR` writes `DIR/writer/doc/<ver>/` for `typst compile --package-path DIR` (the import stays `@writer/doc`). |
| LaTeX math in PDFs | The `mitex` typst package, vendored whole (pinned) and served as `@preview/mitex` | Converted at compile time by the package's WASM plugin (verified to load and compile from the embedded store); emitted `.typ` stays portable (the CLI downloads the same package). The `mitex` Rust crate's output is not standalone typst (R2 §5.4). |
| LaTeX math in HTML | KaTeX client-side (vendored 0.19.1, presenter's `math.js` lifted), guarded against re-rendering on live reload; `[math] html = "mathml"` renders MathML with `math-core` 0.9.0 plus a math-font rule | Proven path with the best rendering; MathML Core is widely available but needs an explicit math font to render well (R4 §3). Server-side `katex-rs` is a stretch option once its fidelity is measured (R4 §4). |
| LaTeX math in DOCX | `math-core` MathML → writer's own MathML-Core→OMML writer (mapping per texmath's OMML writer); unmappable display equations become compiler-rendered images | No Rust crate converts MathML/LaTeX to Word-grade OMML (R4 §6, R3 §4). |
| typst math in HTML/DOCX | The compiler's MathML export (`typst-html`, one equation at a time) + typst's `EQUATION_CSS`; MathML → OMML as above | Verified 16/16 equations, 62 µs each (R4 §7); no converter needed. |
| typst math → LaTeX | Own converter over the typst-syntax math tree + `codex` symbols + a Unicode→LaTeX table derived from tylax's (Apache-2.0) | No adequate crate (tylax pins an older parser, pandoc is an external process); the tree is small and regular (R2 §4); pandoc and tylax are test oracles. |
| Equation and figure renditions | The embedded compiler (`typst::render`): SVG/PNG of one equation with its baseline (`Frame::baseline()`), PDF/PNG of an SVG, GIF or WebP figure | One deterministic engine with embedded fonts; avoids `resvg`/`svg2pdf`, which pin other `usvg` lines (R4 §5, R7 §5). |
| Citations | `hayagriva` 0.10.1 (BibTeX/BibLaTeX/YAML + 80 CSL styles) for HTML/DOCX; typst's `bibliography()` (same crate, same version) for PDF; `\autocite`/`\citep`/`\cite` for LaTeX (`.bib` copied into `<stem>_assets/`; YAML libraries are an error for LaTeX) | Byte-identical citations between writer and typst were measured (R5 §3). |
| DOCX | Hand-rolled OOXML (`zip` 9 with deflate and a fixed timestamp; XML through ordered property builders), pandoc's style names resolved by name, pandoc's reference-doc part-copy rules, own `styles.xml` with explicit fonts and colours (no theme part unless the reference supplies one) | `docx-rs` has no OMML, no SVG, no raw body XML, no `tblHeader`/`tblCaption`/`updateFields`, and a lossy reader (reference documents cannot be honoured) (R3 §1). presenter's `pptx.rs` XML helpers (escape, clean, rels, content types) are lifted. |
| HTML code highlighting | `syntect` 5.3 (`default-fancy`, pure Rust) + `two-face` syntaxes, class-based output + theme stylesheets generated into the theme CSS | Server-side, works in DOCX (coloured runs) and LaTeX (`\Wr*` token macros) from the same tokens (R6 §A.1, R7 §6). |
| Templates | `minijinja` 2.24 (`loader`, `json`, `loop_controls`, `custom_syntax`); LaTeX templates with `\VAR{}`, `\BLOCK{}`, `\#{}` delimiters, no auto-escaping | Lifted theme code targets 2.x; 3.0 was released this week and removed `loader` (R6). Upgrade is a separate task (T79). |
| Live reload | SSE + morphdom 2.7.8 morphing `[data-writer-document]`; `data-sourcepos`/`data-source` attributes (serve mode) → anchor keeping and scroll-to-edit; per-view status; pages view as cached per-page SVG resources | Protocol and server lifted from presenter; morphdom keeps DOM identity (R6 §A.2). |
| Watcher | `notify` 8.2 with presenter's own debouncer | notify 9 is a release candidate (R6). |
| Front matter | YAML via `serde_norway` | As presenter. |
| Embedding | `rust-embed` 8 (disk in debug builds) | As presenter. |
| Build profiles | `dev`: no LTO, `[profile.dev.package."*"] opt-level = 2`; `release`: presenter's (`opt-level = 3`, thin LTO, `codegen-units = 1`, strip) | Unoptimised typst is 8–60× slower; tests render equations and pages (R4 §5 note). |

---

## 4. Contracts (frozen before fan-out)

| Contract | File | Consumers |
|---|---|---|
| Intermediate representation (`Document`, `Block`, `Inline`, `Cell`, labels, math, assets), frontend stubs and the resolve interface | `docs/spec/ir.md` | every frontend, resolver and emitter |
| Markdown dialect (document extensions, directive table, labels, references, includes, raw blocks, presenter compatibility) | `docs/spec/markdown-dialect.md` | T01 (`code.rs`), T10–T12, T1B, T18, T1C, T71, docs, fixtures |
| typst dialect: the structured subset, the recognised `show: writer-doc.with(...)`, the `@writer/doc` package API, the IR→typst emitter rules, the World (virtual files, packages, fonts, long-lived serve World, timestamps) | `docs/spec/typst-dialect.md` | T03, T30–T34, T50–T53, T71, T72 |
| References: label alphabet, numbering policy, `@ref`/`@cite` resolution, footnotes, TOC, per-target rendering, DOCX bookmark names, the numbering oracle | `docs/spec/references.md` | T01 (`resolve::l10n`), T17, T70, T03 and T31 (oracle), every emitter |
| Math: dialects, conversion matrix, crates, fallbacks, fixtures | `docs/spec/math.md` | T03, T19, T1A, T30, T33, T42, T51, T52, T60 |
| Notebook → IR mapping (cells, outputs, `#|` options, tags, tables, ANSI, raw cells, footnotes across cells, includes) | `docs/spec/notebook-mapping.md` | T18, T1B, T53 |
| HTML output: markup contract, theme format (lifted, trimmed), template context, position attributes, single-file emission and size budget | `docs/spec/html-output.md` | T13, T14, T15, T19, T1A, T22 |
| Live reload protocol (events with per-view status, routes, client behaviour, pages view, watch set, the serve World) | `docs/spec/live-reload.md` | T20–T24, T73 |
| DOCX output: parts, style vocabulary and name-based lookup, element mapping, child-order tables, reference-doc rules, fields, bookmarks, numbered equations, images, validation | `docs/spec/docx-output.md` | T04, T40–T43 |
| LaTeX export: element mapping, escaping and Unicode policy, template contract (`preamble`/`body` slots), generated preamble and hooks, assets, config | `docs/spec/latex-output.md` | T60–T61 |
| Configuration schema and precedence | `docs/spec/config.md` | T01, T10, T16 |
| CLI surface | `docs/spec/cli.md` | T16, T23, T1C, T71–T73 |
| Vendored assets and embedded data: exactly what is copied, licenses, sizes, the ZIP and raster decisions | `docs/spec/vendored-assets.md` | T00, T03, T1A, T51 (§3.7), T75 |

Tasks without a spec of their own:
- **T25** (browser-test client): its contract is presenter's lifted `Page` API in
  `src/pdf/devtools.rs` (R8 §3.8: `Page::{launch(chrome, timeout), set_timeout, navigate(url),
  evaluate(expr) -> Result<Value>, errors()}`), behind the `browser-tests` feature. Its users are
  live-reload.md §9 and math.md's browser test. The "network log" checks of live-reload.md §9 read
  `evaluate("performance.getEntriesByType('resource')")` (after clearing with
  `performance.clearResourceTimings()`), so the lifted API needs no new method.
- **T76–T79** (stretch) are specified by their task files, bound by what the specs already fix:
  T76 by markdown-dialect.md §4.6's reserved `include=` pair; T77 by config.md `[pdf] engine`
  (`PdfEngine::Chrome`), cli.md's `--engine` and exit code 3; T78 by typst-dialect.md (the
  structured subset stays the export path); T79 by html-output.md's theme format (a `.scss` entry
  is `theme-style-entry` until T79) and the exact pins of vendored-assets.md §7.3.

---

## 5. Milestones and task graph

Each task is a file `docs/tasks/Txx-*.md` with goal, dependencies, owned files, steps,
acceptance criteria and verified API hints (pointing into `docs/research/`). Sizes: S ≈ half a
day, M ≈ a day, L ≈ 2–3 days for a careful agent, tests included. "Lift" means copying from
presenter_rs per R8 §10.

### M0 — Contracts and scaffold
| ID | Task | Size | Depends on |
|---|---|---|---|
| T00 | Repository scaffold: complete `Cargo.toml` with every pinned dependency and profile, CI (3 OSes, vendor drift), `scripts/vendor.sh` (KaTeX, morphdom, the mitex package, Latin Modern Math, license files), embedded assets, `build.rs` (lift), the §3.2 module skeleton and placeholders (`assets/writer/math.js`) | M | — |
| T01 | Core IR, config, error types, asset registry (lift), frontend stubs with final signatures (`markdown::parse`, `notebook::parse`, `typst::frontend::parse`, `detect_kind`), `markdown::code` (`CodeAttrs`), `resolve::l10n` + `assets/l10n/en.toml`, md/ir/config/bib fixtures | L | T00 |
| T03 | typst compiler embedding: in-memory `World` (retained `Source`s, `replace`, accessed-file log, `today`; the `IncludeProvider` trait and `Synthesized` in `world.rs`, `SourceMap` complete with `from_lines` in `sourcemap.rs`), embedded + scanned fonts, `@writer/*` and embedded `@preview/*` packages, own `Downloader`, compile to PDF/SVG pages, `render` helpers (equation SVG/PNG with baseline, SVG/GIF/WebP → PDF/PNG), `typst::html` (equation MathML, `EQUATION_CSS`), diagnostics, the numbering oracle query | L | T01 |
| T04 | DOCX package core: deflated deterministic ZIP, parts, name-keyed `StyleTable`, own `styles.xml`/`numbering.xml`/`settings.xml`, ordered property builders with child-order tables, reference-doc merging, structural validator, a minimal `reference.docx` fixture | L | T01 |

### M1 — Markdown and notebooks → HTML (`writer build`)
| ID | Task | Size | Depends on |
|---|---|---|---|
| T10 | Front matter → `DocMeta` and config overrides (lift) | S | T01 |
| T11 | Markdown parser foundations (lift): `comrak_options`, attribute grammar and recovery, code attributes, directive info parsing, the `:::` colon pre-pass, differential snapshot of R9's probe corpus | M | T01 |
| T12 | comrak AST → IR walker: blocks, inlines, implicit figures, table captions, display math with labels, alerts, directives (containers and leaves), raw fences, footnotes, presenter-compat drops | L | T10, T11, T14 |
| T13 | HTML emitter: IR → body HTML per the markup contract, syntect highlighting, citation spans and bibliography section (from the resolved fixture) | L | T01 |
| T14 | Asset emitter (lift) with versioned serve URLs | S | T01 |
| T15 | Theme chain (lift, trimmed), `document.html`, template context, helpers (`paths.rs`, `styles.rs`), `child-theme` fixture | M | T03, T13, T14 |
| T16 | Pipeline with every target arm, complete clap tree with stubs, `build` + `dump`, `report.rs` lift, `include::scan_includes` lift (additive in T1B's file, for T20), `WorldSlot` (live-reload.md §5.2–5.3) installing typst-dialect.md §6.4's `MarkdownIncludes` stub (additive in T53's `src/typst/includes.rs`) on every `get`, example document, end-to-end snapshots | M | T03, T10, T12, T13, T14, T15, T17 |
| T17 | Resolve pass: labels, numbering policy, references, footnotes, TOC, diagnostics (no bibliography yet) | M | T01 |
| T18 | Notebook frontend: nbformat (lift), cells → IR, `#|` options and tags, DataFrame tables, ANSI, attachments, footnotes across cells, replaces the stub | L | T12, T13, T30, T40 |
| T19 | Default HTML theme v1 (typography, figures, tables, callouts, code themes, cells, footnotes, TOC, print CSS, dark mode) | M | T15, T16 |
| T1A | Math in HTML: KaTeX (vendored, `math.js` lifted with the morph guard), `math-core` MathML route, equation-image route for the `image` option | M | T03, T15, T16, T30 |
| T1B | References and citations syntax in text (`@target[sup]{.mode}`, `[@a; @b]`, `\@`), `:::include` of `.md`/`.ipynb` (lift + adapt), example and snapshots updated | M | T12, T16, T18 |
| T1C | `writer check` (lift) with the Markdown and notebook lints, `--format json`, `--deny-warnings` | S | T16, T1B |

### M2 — Live development loop (`writer serve`)
| ID | Task | Size | Depends on |
|---|---|---|---|
| T20 | File watcher (lift), the document watch set (incl. the World's accessed files), change classification (`css`/`pages`/`content`) | M | T16 |
| T21 | Dev server (lift): document, theme CSS, versioned assets, SSE with per-view status, pages routes, `/__writer/state` | M | T16 |
| T22 | Live-reload client: morphdom on `[data-writer-document]`, anchor keeping, scroll-to-edit, KaTeX guard, CSS swap, overlay with per-view badges, view toggle, `docs/dev/live-reload-qa.md` | M | T00 |
| T23 | `serve` CLI wiring, the session and rebuild loop with the one long-lived World, `--view` defaults, integration tests | S | T1A, T20, T21, T22, T25 |
| T24 | Pages view: SVG per page with hashes, `pages.js` (`<img>` grid, lazy, sized), last-good pages on error | M | T21, T23, T32 |
| T25 | Headless-browser smoke test (lift the DevTools client internals; opt-in feature) | M | T16 |

### M3 — PDF through typst
| ID | Task | Size | Depends on |
|---|---|---|---|
| T30 | IR → typst emitter: escaping, every block/inline, figures/tables/callouts/code/cells, refs/cites/footnotes, `#mi`/`#mitex` for LaTeX math, `math::latex_source`, raw typst, deterministic position-stable output, source map (built with T03's `SourceMap::from_lines`) | L | T01, T03, T17 |
| T31 | `@writer/doc` package: document template, numbering policies (incl. per-section), callouts, cells, figure kinds, bibliography, the oracle metadata, `template.typ`; `typst::package::{eject, install_dir}` | M | T03 |
| T32 | PDF pipeline: emit → compile with virtual assets, the `[pdf] template` mount and `EmitOptions.template` in `load_typst_main` (typst-dialect.md §4.6, §8.4), diagnostics mapped to source lines (incl. package traces), `build --to pdf`, `dump --typst`, emitter snapshots of the Markdown fixtures, reproducible PDF (timestamp policy), numbering-oracle test, Windows smoke test | L | T03, T16, T30, T31, T50 |
| T33 | Math corpus: LaTeX equations through mitex in PDFs, `check` pre-flight with the `mitex` crate (additive in T1C's `src/check.rs`), round-trip fixtures | S | T1A, T1C, T32, T42 |
| T34 | BYO typst template tests (`[pdf] template` mounted by T32) and the template copy in the export, `build --to typst` export with materialised includes (T31 provides the eject functions, T72 wires them) | M | T1B, T32 |

### M4 — Word (`.docx`)
| ID | Task | Size | Depends on |
|---|---|---|---|
| T40 | DOCX body: paragraphs/runs, headings, lists, code (coloured runs), quotes, callouts, page breaks, title block, hyperlinks, bookmarks and internal links, footnotes, cells, `custom-style` honoured | L | T04, T13, T17 |
| T41 | DOCX tables, figures (PNG/JPEG native, SVG/GIF/WebP via renditions; sizing and clamping), captions with SEQ fields per policy (caption bookmarks), TOC field with entries, bibliography paragraphs; `build --to docx` wiring | L | T16, T40 |
| T42 | DOCX math: `docx/math.rs` + `math/omml.rs` (MathML Core → OMML), the numbered-equation frame, equation-image fallback | M | T03, T1A, T41, T51 |
| T43 | Reference documents: `eject docx-reference` (every style name writer uses), reference fixtures, validation suite (structural, read-back, `textutil`/python-docx smoke), `docs/dev/docx-validation.md` | M | T41, T42 |

### M5 — typst backend
| ID | Task | Size | Depends on |
|---|---|---|---|
| T50 | typst-syntax → IR walker for the structured subset: markup, the `writer-doc.with` show rule → `DocMeta`/config, allow-listed calls (incl. `mi`/`mitex`, whose leading configured-macro block is stripped with `math::latex_source`; figure kinds from `Raw`/`Equation`/`image`/`table`), `set` rules, literal `let`, labels/refs/cites, `.typ` includes, diagnostic codes | L | T01, T17, T30 |
| T51 | typst math → LaTeX converter (own; `codex` symbols; Unicode→LaTeX table from tylax; pandoc and tylax as oracles) | L | T01 |
| T52 | typst documents end to end: `build` (PDF from the user's file; HTML/DOCX/LaTeX from the IR), typst math in HTML/DOCX end to end (routes implemented by T1A and T42), `serve` (pages default + web), subset diagnostics surfaced, md→typ→IR round-trip golden (`tests/typst_roundtrip.rs`) | M | T1A, T23, T24, T32, T42, T50, T51 |
| T53 | `#include` of `.md`/`.ipynb` from typst through virtual files (`typst::includes::MarkdownIncludes`, implementing T03's `IncludeProvider`); `include-notebook` helper in `cells.typ` | M | T18, T1B, T30, T52 |

### M6 — LaTeX export
| ID | Task | Size | Depends on |
|---|---|---|---|
| T60 | IR → LaTeX emitter: escaping and Unicode policy, every element, math passthrough/conversion, cells, the `Usage` record and asset file naming (`AssetFile`s) | L | T13, T17, T18, T1A, T32, T50, T51 |
| T61 | Template mechanism (`\VAR{preamble}`/`\VAR{body}` contract), sample template and `template::SAMPLE` (wired by T72), the generated preamble, asset materialisation (compiler renditions, `.bib` copies), `build --to latex`, `dump --latex`, config knobs, latexmk-when-available tests (pdflatex and xelatex) | M | T60 |

### M7 — Citations, checks, polish, documentation
| ID | Task | Size | Depends on |
|---|---|---|---|
| T70 | Citations: `cite::*` (hayagriva library and styles, `ElemChildren` → inlines), the resolve hook (`Cite.resolved`, `Resolved.bibliography`, note styles → footnotes, label-or-key rule), `[references]` wiring, `md/citations*.md` fixtures and snapshots | M | T16, T17, T1B |
| T71 | `check`: typst subset lints, per-target unsupported-construct reporting, template slot checks, directive-line lints | S | T1C, T33, T41, T42, T52, T61 |
| T72 | `writer init` scaffold (incl. `--bib`: `refs.bib` and one citation, cli.md `init`) and the `eject` command family (theme lift; wiring of the others: T31's typst template and package, T43's reference document, T61's `template::SAMPLE`) | M | T16, T31, T43, T61, T70 |
| T73 | Diagnostics quality: compiler errors with excerpts, package traces, overlay payloads with warnings (`ErrorPayload` bodies in `src/serve/events.rs`) | S | T16, T21, T22, T32 |
| T74 | User documentation (guide, examples, README) | M | T53, T61, T71, T72, T73 |
| T75 | Release builds (6 targets, THIRD-PARTY-LICENSES incl. fonts and packages), project site | S | T16 |
| T76 | *Stretch:* code and CSV includes (```` ```rust {include="src/x.rs" lines="1-20"} ````, `:::include data.csv`) | S | T1B |
| T77 | *Stretch:* Chrome print engine (`[pdf] engine = "chrome"`, lift `pdf.rs`) | M | T16 |
| T78 | *Stretch:* web view of arbitrary typst through typst's HTML export (`typst-html`) | M | T52 |
| T79 | *Stretch:* SCSS themes (lift the grass stack) and the `minijinja` 3 upgrade | M | T19 |

### Dependency graph (what each task unblocks; derived from the dependency column)

```
critical path:  T00 ─► T01 ─► T03 ─► T15 ─► T16 ─► T25 ─► T23 ─► T24 ─► T52 ─► T53 ─► T74   (11 waves)

w0  T00 ─► T01, T22
w1  T01 ─► T03, T04, T10, T11, T13, T14, T17, T51, T30, T50
    T22 ─► T23, T73
w2  T03 ─► T15, T31, T16, T1A, T30, T32, T42     T13 ─► T15, T40, T16, T18, T60
    T04 ─► T40                                   T14 ─► T12, T15, T16
    T10 ─► T12, T16                              T17 ─► T30, T40, T16, T50, T60, T70
    T11 ─► T12                                   T51 ─► T42, T52, T60
w3  T12 ─► T16, T18, T1B                         T31 ─► T32, T72
    T15 ─► T16, T19, T1A                         T40 ─► T18, T41
    T30 ─► T18, T50, T1A, T32, T53
w4  T16 ─► T19, T1A, T1B, T20, T21, T25, T32, T41, T75, T77, T1C, T70, T73, T72
    T18 ─► T1B, T53, T60                         T50 ─► T52, T32, T60
w5  T19 ─► T79                                   T25 ─► T23
    T1A ─► T42, T23, T33, T52, T60               T32 ─► T33, T34, T60, T73, T24, T52
    T1B ─► T1C, T70, T76, T34, T53               T41 ─► T42, T43, T71
    T20 ─► T23                                   T21 ─► T23, T73, T24
w6  T1C ─► T71, T33                              T42 ─► T33, T43, T52, T71
    T23 ─► T24, T52                              T60 ─► T61
    T70 ─► T72                                   T73 ─► T74
w7  T24 ─► T52                                   T33 ─► T71
    T43 ─► T72                                   T61 ─► T72, T74, T71
w8  T52 ─► T53, T71, T78                         T72 ─► T74
w9  T53 ─► T74        T71 ─► T74
```

### Waves (computed from the dependency column; no task depends on a task in its own wave)

| wave | tasks | parallelism |
|---|---|---|
| 0 | T00 | 1 |
| 1 | T01, T22 | 2 |
| 2 | T03, T04, T10, T11, T13, T14, T17, T51 | 8 |
| 3 | T12, T15, T30, T31, T40 | 5 |
| 4 | T16, T18, T50 | 3 |
| 5 | T19, T1A, T1B, T20, T21, T25, T32, T41, T75, T77 | 10 |
| 6 | T1C, T23, T34, T42, T60, T70, T73, T76, T79 | 9 |
| 7 | T24, T33, T43, T61 | 4 |
| 8 | T52, T72 | 2 |
| 9 | T53, T71, T78 | 3 |
| 10 | T74 | 1 |

Edges added after the first plan (`A → B`: B depends on A). Requested in the specs' Deviations:
T03 → T15, T14 → T12, T13 → T18, T13 → T60, T30 → T1A, T16 → T1A (html-output.md, math.md);
T30, T40 → T18 and T18 → T1B (notebook-mapping.md); T00 → T22, T03 → T16, T25 → T23, T23 → T24,
T21 → T73 (live-reload.md). Added by the coordinator's cross-spec review (2026-10-25; each spec's
maintainer records its edge in that spec's Deviations): T30 → T50 (math.md: `#mi`/`#mitex`
arguments lose `latex_source::macros_as_newcommands(..)`, §3.6); T1B → T70 (references.md: the
`md/citations*.md` snapshots need T1B's `@`/`[@a; @b]` scanner); T1B → T1C (markdown-dialect.md:
`include-meta-ignored`, `include-not-top-level`, include `parse-error` and `invalid-reference-mode`
of §10 are raised by T1B code); T70 → T72 (the `init --bib` scaffold must build; no wave change).
Added by the cross-spec consistency review (round 1, 2026-10-25): T1A, T1C, T42 → T33 (T33 compiles
T1A's `math/latex/*.tex`, adds the `pdf` key to T42's `math/expectations.toml` and its pre-flight to
T1C's `src/check.rs`; T33 moves to wave 7) and T33 → T71 (T71 takes over `src/check.rs`); T42 → T43
(the showcase's numbered equation is the only user of the `Equation` style, docx-output.md §6.5; T43
moves to wave 7); T1A → T23 (`serve math-image.md`, live-reload.md §9 item 11); T50 → T32 (oracle
test over `typ/subset.typ`); T1B → T34 (export tree of `includes.md`); T1A, T42 → T52 (typst math in
HTML/DOCX is math.md §4.5 and docx-output.md §4.15; `expectations.toml`); T1B → T53 (markdown-dialect.md
§8's include rules, the `cell_includes_in` bodies); T1A, T18, T50 → T60 (`math/latex/*.tex`,
`outputs.ipynb` and `typ/math.typ` snapshots); T41, T42, T61 → T71 (`check --target docx|latex`).
Added by the cross-spec consistency review (round 2, 2026-10-25): T03 → T30 (`SourceMap::from_lines`
for `Emitted.map`, `WRITER_DOC_VERSION` for the `@writer/doc` import; T30 stays in wave 3); T51 → T42
(the typst equations of `math/typst/*.typ` and their `docx` rows in `math/expectations.toml`, math.md
§10; T42 stays in wave 6).
No other wave changes; `docs/tasks/README.md` (status rows and wave table) must be regenerated from
this section. The same review moved `src/typst/sourcemap.rs` (`SourceMap` with `from_lines`) and the
`IncludeProvider` trait and `Synthesized` (in `world.rs`) to T03, because T03's World signatures use
them (typst-dialect.md §1, §5.1, §6.1, §6.4; notebook-mapping.md §7.3); without a provider,
`source()` of a `.md`/`.ipynb` returns `FileError::Other(Some("including .md/.ipynb from typst
arrives in T53"))`.
T20 (live-reload.md §6.4) calls `include::scan_includes` without depending on T1B because T16 lifts
that pure function into `src/markdown/include.rs` first (§3.2).

The hard integration points are T16 (everything of M1 meets), T32 (emitter, package and
compiler meet) and T52 (the typst backend meets serve and the exports). The coordinator
regenerates this table from the task files' dependency lines before each wave.

---

## 6. How an agent works a task

1. Read `PLAN.md` §3–§4, your task file, and the spec sections it lists under *Read first*.
   Research notes under `docs/research/` are reference material: their "Verified API hints"
   sections are what to copy; where a note's recommendation differs from a spec, the spec wins.
2. Create a branch `task/Txx-short-name` from `main`. Rebase before opening the PR. (If the
   coordinator runs tasks as local worktrees, "PR description" means the commit message body.)
   **User-authorized workflow (2026-10-26):** continue on completed, locally checked
   dependency branches while CI runs. Open one PR per feature branch/milestone,
   targeting `main` and listing any unmerged dependencies and their merge order.
   While dependencies remain open, review the feature's own commit or compare
   against its dependency branch. CI and merging do not block the next ready
   implementation. This prevents a stacked PR from merging into a feature
   branch after its parent PR has already merged.
3. Only edit the files listed under *Files you own* (§3.2 is the map). If you must touch
   another file, make the change additive (new function, new test, one new match arm) and call
   it out in the PR description. Never change `src/ir.rs`, `Cargo.toml` or `docs/spec/*`
   without a "contract change" note; the coordinator reviews those first. Adding a dependency
   is a contract change.
4. **Module skeleton.** T00 creates every source file listed in §3.2 as a stub (a `//!` doc
   comment naming its owner task and purpose) together with every `mod` declaration in
   `lib.rs` and the `mod.rs` files, so no later task edits a module list; a task that needs a
   file §3.2 does not list asks the coordinator (contract change).
5. **Integration files are written once.** T16 writes `src/pipeline/mod.rs` and `src/cli/mod.rs`
   with every target arm and every command present; commands and targets owned by later tasks
   are stubs that print `<what>: not implemented yet (Txx)` and exit 2. A later task replaces
   only the body of its stub (its own module); editing `cli/mod.rs` after T16 is a contract
   change. The same holds for the frontend stubs T01 creates (`notebook::parse`,
   `typst::frontend::parse`): the owning task replaces the body.
6. **Lifting from presenter_rs:** copy the file the task names, rename identifiers
   (`presenter` → `writer`, `/__presenter/` → `/__writer/`, `Deck` → `Document`), keep its
   tests, then make only the adaptations the task lists. Do not "improve" lifted code; it
   is tested and audited. Note the lifted files in the PR description.
7. Write tests as you go: unit tests next to code, snapshot tests (`insta`) for IR, HTML,
   typst and LaTeX output, structural tests for DOCX, fixtures under `tests/fixtures/`. Every
   acceptance criterion in the task file must map to at least one test or a documented manual
   check.
8. Before the PR: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
   `cargo test`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`, and
   `cargo run -- build examples/basic/document.md` (once T16 exists).
9. The coordinator updates the status table in `docs/tasks/README.md` when it merges your
   branch; task agents never edit that file. Use the PR description template there.
10. Report honestly: if an acceptance criterion is not met, say which one and why.

### Coding conventions
- Edition 2024, `rust-version = "1.96"` (math-core), stable toolchain. `#![deny(unsafe_code)]`,
  `#![warn(missing_docs)]` in `lib.rs`.
- Library code returns `crate::error::Result<T>`; no `unwrap()`/`expect()` outside tests
  unless infallibility is obvious and commented. The binary uses `anyhow` at the top level.
- Deterministic output: `BTreeMap` for anything rendered; no timestamps unless requested;
  ZIP entries with a fixed date; emitted `.typ`/`.tex` have no per-build ids.
- Paths relative to a source file resolve against that file's directory, never the CWD.
- Logging through `tracing`; `-v/-vv` raise the level. No `println!` in the library.
- Public items carry doc comments. Functions stay under ~80 lines.
- Tests need no network. Browser tests sit behind the `browser-tests` feature; tests that need
  `latexmk`, `textutil` or python-docx detect them and print `skipping` otherwise. Tests that
  render with the compiler keep fixtures small (one page, 2 px/pt); the dev profile optimises
  dependencies (`[profile.dev.package."*"] opt-level = 2`).

---

## 7. Testing strategy

| Level | Tool | What |
|---|---|---|
| Unit | `#[test]` | attribute grammar, directive info, label/ref syntax, numbering policy, escaping (typst, LaTeX, XML), math converters, bookmark names, config merge |
| Golden IR | `insta` JSON snapshots | each `tests/fixtures/{md,ipynb,typ}/*` → `Document` JSON (paths and asset ids redacted); md → `.typ` → IR round trip |
| Golden outputs | `insta` text snapshots | HTML body (build mode; one serve-mode fixture for position attributes), `.typ` source, `.tex` output per fixture; DOCX parts per fixture |
| DOCX structure | own validator (T04) + `docx-rs` read-back (dev-dependency) | parts, rels, content types, child order, bookmarks, fields, numbering, styles; optional `textutil`/python-docx smoke on macOS; manual Word checklist for releases |
| PDF | the embedded compiler in tests | page count, byte-identity of two builds, the numbering oracle (`sys.inputs.writer-oracle` metadata queried by label) against `dump` |
| CLI | `assert_cmd` + `tempfile` | `build`, `dump`, `check`, `init`, `eject` exit codes and files written |
| Serve | integration test with `ureq` | start on port 0, GET `/`, edit, read SSE events (`build` with per-view status, `pages` hashes), versioned asset URLs |
| Browser (opt-in) | DevTools client (feature `browser-tests`) | built HTML opens without console errors; KaTeX renders; live reload morphs, keeps the anchor and leaves untouched equations alone; an edited image shows without reload |
| LaTeX (opt-in) | `latexmk` when installed | the sample template compiles the fixture body with pdflatex and xelatex; the generated preamble alone compiles |
| Docs | `tests/guide.rs` (lift) | guide links, `--help` blocks and `dump --default-config` do not drift |

Fixture corpus under `tests/fixtures/` (ownership per task; T01 creates only the md, ir, config
and bib fixtures it lists, and other tasks add files to those directories as named here; the
owning spec's Fixtures section gives each file's content):
- T01 — `md/{minimal,attributes,attribute-recovery,directives,pandoc-directives,figures,tables,callouts,code,math,references,footnotes,includes,raw,pagebreaks,presenter-deck}.md` (inputs, with `md/images/` and `md/include/`), `ir/sample-document.json` (the resolved form), `config/writer.toml`, `bib/refs.bib`;
- T11 — `md/probe/` (R9 §14's probe corpus, differential snapshot); T17 — `md/numbering.md` (input), optional `ir/*-unresolved.json`;
- T1B — `md/include-chain.md`, `ipynb/includes.ipynb`;
- T70 — `md/{citations,citations-notes}.md` (with their Markdown and diagnostics snapshots), `bib/{refs.yaml,minimal.csl,dup-a.bib,dup-b.bib}`;
- T18 — `ipynb/{minimal,outputs,options,tags,frontmatter,dataframe,footnotes}.ipynb`; T15 — `themes/child-theme/`;
- math (math.md §10): T1A — `math/latex/*.tex`, `math/macros.toml`; T42 — `math/mathml/*.mml`, `math/expectations.toml` (T33 adds the `pdf` key); T51 — `math/typst/*.typ` with their `.pandoc.tex`/`.tylax.tex` oracles and `README`;
- T04 — `docx/reference.docx` (produced by writer's own package writer, never pandoc's GPL-2 file; T43 regenerates it); T40 — `docx/ir/{blocks,lists,code,callouts,footnotes,refs,cells}.json`; T41 — `docx/ir/{figures,tables,toc,bibliography}.json`, `docx/images/*`; T42 — `docx/ir/math.json`; T43 — `docx/reference-localized.docx`;
- T50 — `typ/{minimal,subset,unsupported,math,includes,syntax-error}.typ`, `typ/refs.bib`, `typ/include/{part.typ,cycle-a.typ,cycle-b.typ}`; T53 — `typ/{include-notebook,include-notebook-missing}.typ`, `typ/include/cells.ipynb`, and `typ/include/{part.md,nb.ipynb}` added to `includes.typ`;
- T60 — `latex/{tour,verbatim-end}.md`, `latex/images/*`; T61 — `latex/{template,byo-template,missing-slot}.tex`;
- T22 — `live-reload/{mock-server.py,qa.mjs}` (T24 adds the page routes); T23 — `serve/` (T24 adds `template.typ` and `[pdf] template`; T52 adds the `.typ` serve fixtures).

Every snapshot test that globs `tests/fixtures/md/*.md` (T12 `markdown_snapshots`, T16
`html_snapshots`, T32 `typst_emit`, T60 `latex_snapshots`) snapshots a fixture whose parse returns
`Err` as the error's Display text (`includes.md` once T1B adds the cycle check). A task that adds a
file there (T17, T1B, T70) or changes how one parses (T1B: `includes.md` and every fixture with `@`
references or citations, such as `references.md` and `numbering.md`) adds or re-blesses that file's
snapshots in every such test already on `main` when it merges; this counts as an additive change and
is named in its PR. For same-wave pairs (T1B/T32, T70/T60), the task that merges second does it.

No `writer.toml` may live in an ancestor of the fixture directories.

---

## 8. Risks and mitigations

| Risk | Mitigation |
|---|---|
| comrak silently demotes a `:::` line that contains a colon (R9 §6.2) | The frontend parses, finds paragraphs whose first text starts with `:::`, rewrites `:` after the fence to `&#58;` on those source lines and re-parses (info diagnostic `directive-colon-rewritten`); `check` lints `:::` lines that still did not open or close a directive; labels use `kind-name`. |
| Same-length nested `:::` fences close the outer directive early; unclosed directives swallow the document | Documented (longer outer fence); `check` warns on an unclosed directive and on a bare `:::` with empty info. |
| typst crates raise MSRV, add ~45 MiB and ~60 s cold builds | Pinned 0.15.1; `dev` profile without LTO and with optimised dependencies; size budget recorded in `vendored-assets.md`. |
| typst API churn between minor versions | Pinned exact versions; the World and compile code live in `src/typst/{world,compile,render}.rs` with their own tests; upgrades are a deliberate task. |
| The typst subset and the compiled PDF disagree (a show rule the walker ignores) | Documented as the contract; `check` reports ignored constructs; the md→typ→IR round-trip golden keeps the emitter inside the subset. |
| Numbers differ between HTML/DOCX (writer's resolve pass) and PDF (typst's counters) | The `@writer/doc` package implements exactly the policies in `references.md`; T32's oracle test queries the compiled document's `writer-oracle` metadata per label and compares with `dump`. |
| Word rendering cannot be verified automatically (Word hangs under osascript) | Structural validator with child-order rules + read-back + macOS `textutil`/QuickLook smoke; a manual Word checklist in `docs/dev/docx-validation.md` for releases. |
| OMML coverage | The writer handles the MathML Core element set; anything it cannot map falls back to a compiler-rendered equation image (display) or a Unicode text run (inline) with a warning. |
| Word's "update fields?" prompt recomputes SEQ numbers | Field instructions per kind and policy (per-section numbers: the literal chapter number + `SEQ … \s 1`, with `\r n` where Word's own count would differ; docx-output.md §4.11) so Word's recomputation agrees with writer's numbers; `[docx] update_fields = false` keeps cached values. |
| minijinja 3 / comrak 0.57 / notify 9 drift | Exact pins; T79 budgets the minijinja upgrade with the differential tests as gate. |
| `@preview` packages need network | Off by default; `[pdf] allow_packages = true` enables writer's own `Downloader` (ureq + rustls); `mitex` is vendored so LaTeX math never needs it. |
| Fleet agents editing the same files | The ownership map in §3.2; stub-replacement rule for integration files; IR/spec/Cargo changes gated by the coordinator. |
| Large documents in `serve` | Pages view re-fetches only changed pages; morphdom patches only changed nodes; one long-lived World recompiles incrementally. |

---

## 9. Verified facts (2026-10-24; details and sources in `docs/research/`)

- **Local toolchain:** rustc/cargo 1.99.1 (Homebrew), pandoc 3.12.1, TeX Live 2026 (pdflatex,
  xelatex, lualatex, latexmk; no biber), Google Chrome, Microsoft Word.app; no typst CLI, no
  LibreOffice.
- **typst 0.15.1** (Apache-2.0, rust-version 1.92; R1): a complete in-memory `World` is ~60
  lines; `typst::compile::<PagedDocument>` → `typst_pdf::pdf` / `typst_svg::svg` /
  `typst_render::render`; `typst_layout::{PagedDocument, Page}` must be a direct dependency.
  Cold `cargo build --release` of the embedding crate: 58 s wall, 295 crates; binary 45.2 MiB
  with embedded fonts (35.8 MiB without). Compile: 20 ms cold for a tiny document, 69–76 ms
  for 25 pages, 9–14 ms after an edit with a retained `Source`, 0.25 ms unchanged; PDF export
  7–9 ms. typst-kit 0.15.1 was reworked (features `embedded-fonts`, `scan-fonts`,
  `system-packages`, `universe-packages`, `system-downloader` (OpenSSL), …). A custom
  namespace `@writer/doc:0.1.0` resolves purely through `World::file`. HTML export is
  library-usable behind a runtime `Feature::Html`, emits semantic HTML with MathML and no CSS,
  and is officially experimental. The vendored `mitex` package's WASM plugin loads and compiles
  from an in-memory World (review probe).
- **typst-syntax 0.15.1** (R2): `typst_syntax::parse` never fails; errors are `SyntaxKind::Error`
  leaves; `SyntaxNode::leaf_text()`/`full_text()` (no `text()`); math calls are
  `SyntaxKind::MathCall` + `MathArgs`; `Label::get()` without `<>`; `Ref::target()` without `@`.
  Parse of a 13,500-line file: 5.5 ms; `Source::replace` edit: 0.4 ms. `codex` 0.3.0 maps
  symbol names to Unicode. pandoc's typst reader converts 15/15 test equations to LaTeX; tylax
  0.3.8 (Apache-2.0) 14/15 but pins typst-syntax 0.14. `mitex::convert_math` output needs the
  mitex package scope.
- **Math** (R4): `math-core` 0.9.0 (MIT, MSRV 1.96) and `pulldown-latex` 0.8.0 (MIT) both
  convert 13/13 KaTeX-style inputs to MathML; math-core recommended (macro ergonomics,
  unknown-command recovery; emits `&ApplyFunction;` which the OMML writer must tolerate).
  MathML Core is Baseline widely available (Chrome/Edge 109+, Firefox, Safari) but renders
  badly without an explicit math font (`font-family: math` failed in Chromium 152 even with
  STIX Two Math installed; Latin Modern Math woff2 388 KB, GUST FL, works). KaTeX npm 0.19.1
  (2026-10-23): js 273 KB + css 25 KB + 20 woff2 260 KB. `katex-rs` 0.3.0 (MIT) is a pure-Rust
  port of KaTeX 0.18.5 (17–33 µs/render, +1.2 MB). The compiler renders one equation to a
  self-contained SVG (glyphs as paths, 2.6–12.7 KB) or PNG in 15–65 µs warm; `#set text(top-edge:
  "bounds", bottom-edge: "bounds")` is required or descenders are clipped; the first group
  frame's `baseline()` gives the baseline. No Rust crate converts MathML → Word-grade OMML
  (tex2word-math, mt-omml, deckmint-math, easydoc-math all inspected). typst's HTML export
  produced MathML for 16/16 typst equations (62 µs each) and needs its 1.5 KB `EQUATION_CSS`.
- **comrak 0.57.0** (BSD-2-Clause; R9): attribute, autolink, table and scanner sources
  identical to 0.56; two new `NodeValue` variants. Directives are container-only; the opening
  line may contain no colon after the fence; an empty container is the leaf form. `$$…$$` is an
  inline `Math` node (display only as a paragraph's sole child, or with a trailing attribute
  text). Footnote definitions are moved and unreferenced ones dropped unless
  `parse.leave_footnote_definitions = true`. `@fig-x`, `(@tbl-y)`, `[@key]` are never
  autolinked; `\@` needs `parse.escaped_char_spans`. `sourcepos` is always filled in the AST.
- **hayagriva 0.10.1** (R5): the version typst 0.15.1 pins; `ArchivedStyle::by_name` accepts
  typst's style names (80 styles, 63 locales); measured byte-identical citations and
  bibliography entries against `typst compile` for ieee, apa and chicago-notes. Locators must be
  passed as `Locator::Custom`. No BibLaTeX export (YAML libraries cannot feed LaTeX). typst
  defaults: figures numbered `"1"` with one counter per kind, headings and equations unnumbered.
- **DOCX** (R3): docx-rs 0.4.22 has no OMML, no SVG (`svgBlip`), no raw body XML, no
  `tblHeader`/`tblCaption`/`updateFields`, and a lossy reader. pandoc's reference-doc rules and
  its complete style-name table are recorded; its `reference.docx` is GPL-2. `textutil`,
  `qlmanage` and python-docx 1.2.0 work as smoke tests; Word via osascript hangs. Verified:
  zip 9.0.5, quick-xml 0.42.0, imagesize 0.15.0. pandoc hashes bookmark names containing
  anything but `[A-Za-z0-9_]` (review).
- **LaTeX export** (R7): minijinja 2.24 `custom_syntax` renders `\VAR{}`/`\BLOCK{}`/`\#{}`
  templates (a shared `}` end marker works); pandoc 3.12.1's escaping table and default
  template recorded; a body using only graphicx, longtable, booktabs, array, caption, amsmath,
  amssymb, xcolor, fancyvrb, ulem, hyperref plus writer's `providecommand`-guarded hooks
  (`writercallout`, `writercell`, `\writerpagebreak`, `\writerhrule`, `\Wr*` tokens) compiles
  with pdflatex and xelatex; `\textcolor[rgb]` inside a `commandchars` Verbatim fails while
  `[HTML]` works; pdflatex errors on `≥ ≠ α ∞` without `newunicodechar` mappings, xe/lualatex
  silently drop glyphs missing from Latin Modern; svg2pdf 0.13 pins usvg 0.45 (not used).
- **HTML side** (R6): syntect 5.3.0 `default-fancy` + two-face 0.5.2 (213 syntaxes incl. typst,
  julia, toml; +~0.6–1 MiB); class-based generator and `css_for_theme_with_class_style`
  verified; `HighlightLines::highlight_line` gives DOCX-ready runs. morphdom 2.7.8 (MIT, 12 KB).
  Chrome 155 print CSS supports `@page`, `break-*`, margin boxes with `counter(page)` but not
  `target-counter()`. Latest crate versions and license table in R6 §B; majors changed since
  presenter: minijinja 3.0.3 (pin 2.24), html-escape 0.3, notify 9 rc (pin 8.2).
- **Notebooks** (R10): quarto's `#|` option grammar and label suffix rules, nbconvert tags,
  nbformat raw-cell `format`/`raw_mimetype`, pandas `to_html` table shape, `scraper` for
  parsing, ANSI crates compared.
- **presenter_rs** (R8): 54.8k LOC; ~13.5k LOC lift verbatim, ~18k adapt; `pptx.rs` holds the
  XML helpers for DOCX; `pdf.rs` is a generic Chrome DevTools driver (Unix-only).
