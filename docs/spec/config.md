# Spec: Configuration

**Status:** contract. Types and every `impl Config` method of §4 (`load`, `from_toml_str`,
`discover`, `merge_front_matter`, `apply_cli`, `targets_for`, `default_toml`) live in
`src/config.rs`, written by T01 (lifted from presenter's config.rs) together with §4's precedence
tests. T10 stores the front-matter keys in `DocMeta`/`DocMeta.vars` (markdown-dialect.md §2.2).
T16 calls `discover`/`load`/`merge_front_matter`/`apply_cli` in `pipeline::prepare`. Consumed
everywhere.

> **Contract change (coordinator, 2026-10-25):** the Status line used to say "loading and merging
> in T10 and T16", which named no file and let three tasks claim the same methods; PLAN §3.2 gives
> `src/config.rs` to T01 alone and R8 §10 step 6 lifts discovery and `merge_front_matter` into it.
> T50 and T18 call `Config::merge_front_matter` from T01's file, so their dependency lines stand.

presenter's mechanics are kept: discovery by walking up, `#[serde(default, deny_unknown_fields)]`
on every table, "did you mean" hints for unknown keys, idempotent front-matter merging,
precedence tests for every pair.

## 1. Sources and precedence (highest first)

1. CLI flags (`--to`, `--out`, `--theme`, `--template`, `--reference-doc`, `--latex-template`,
   `--standalone/--site`, `--port`, `--view`, `--strict`, `--engine`, `--allow-packages`)
2. Document front matter (Markdown), notebook metadata (`metadata.writer`, first-cell front
   matter), or the typst `#show: writer-doc.with(...)` arguments (typst-dialect.md §3)
3. Project file `writer.toml` — the first one found walking up from the source file's
   directory; `--config PATH` overrides discovery
4. The HTML theme chain's `theme.toml` `[vars]` (templates only; no build settings)
5. Built-in defaults (§2)

Tables merge key by key; lists replace.

## 2. `writer.toml` schema with defaults

```toml
[document]
lang = "en"                # BCP-47; front matter `lang` overrides
toc = false                # false | true (depth 3) | 1..6 (max heading level listed)
                           # the TOC goes after the title block unless the document places :::toc

[build]
out_dir = "dist"           # set here: relative to this file; default: relative to the input's directory
targets = ["html"]         # for .md/.ipynb inputs; subset of html, pdf, docx, latex, typst (order = output order)
typst_targets = ["pdf"]    # for .typ inputs

[markdown]
smart_punctuation = false  # comrak parse.smart
sourcepos = false          # emit position attributes in `build` HTML too (serve always does)
references = true          # parse @target references/citations; false for presenter decks with literal @handles
superscript = false        # comrak extension.superscript (x^2^)
subscript = false          # comrak extension.subscript (H~2~O); off because ~/paths in prose would subscript

[numbering]
headings = false           # false | true (≡ "1.1") | a typst numbering pattern string ("1.", "1.1", "A.1", "1.1.a")
figures = "document"       # "document" (1, 2, …) | "section" (1.1, 1.2, …; counters reset at level-1 headings)
equations = "labelled"     # "labelled" (only equations with a label) | "all" | "none"

[references]
bibliography = []          # .bib / .yaml files, relative to this file (front matter: relative to the document)
style = "ieee"             # a hayagriva style name (typst's names) or a path to a .csl file
locale = ""                # CSL locale; "" → document.lang

[math]
html = "katex"             # "katex" (client-side KaTeX, vendored) | "mathml" (math-core, no JS) | "image" (compiler-rendered SVG)
mathml_font = "embed"      # for MathML output (html = "mathml", and typst math under any html but "image"):
                           # "embed" (Latin Modern Math woff2 inlined, +≈518 KB as base64) | "system" (font-family rule only)
katex_options = {}         # passed to katex.render (e.g. { macros = { "\\R" = "\\mathbb{R}" } })

[html]
theme = "default"          # built-in name or a directory (relative to this file)
standalone = true          # false → <stem>/index.html + assets/ (site mode)

[pdf]
template = ""              # a .typ file exporting `writer-doc` (typst-dialect.md §4); "" → built-in
paper = "a4"               # any typst paper name ("us-letter", …)
engine = "typst"           # "typst" | "chrome" (stretch T77; needs Chrome)
allow_packages = false     # let the compiler download @preview packages (network)
system_fonts = true        # scan the system's fonts after the embedded ones (false for byte-identical PDFs)
font_paths = []            # extra font directories, searched after the embedded fonts and before the system's
timestamp = ""             # "" → front matter `date`, else SOURCE_DATE_EPOCH, else now; an RFC 3339 value pins it

[docx]
reference_doc = ""         # a .docx whose styles/page setup/headers/theme are used; "" → built-in
math = "omml"              # "omml" (editable Word equations, image fallback per equation) | "image" (always SVG+PNG)
update_fields = true       # ask Word to update fields (TOC page numbers, SEQ) on open; false keeps the cached values

[latex]
template = ""              # minijinja .tex template with \VAR{preamble} and \VAR{body}; "" → the built-in sample
top_level = "section"      # "part" | "chapter" | "section": what a level-1 heading becomes
cite = "biblatex"          # "biblatex" | "natbib" | "plain"
emit_bibliography = false  # true → writer appends \printbibliography / \bibliography{...} at the end of the body
code = "verbatim"          # "verbatim" (fancyvrb Verbatim) | "highlighted" (syntect tokens as \Wr* macros) | "listings"
tables = "longtable"       # "longtable" | "tabular" (floating table environment)
booktabs = true            # \toprule/\midrule/\bottomrule; false → \hline
cleveref = false           # \cref instead of "Figure~\ref"
figure_placement = "htbp"  # the [..] option of figure/table environments; "H" needs the float package; "" for none
engine = "pdflatex"        # "pdflatex" | "xelatex" | "lualatex": selects the Unicode policy and the lints; writer never runs TeX
unicode = "auto"           # "auto" (map for pdflatex, keep otherwise) | "map" (maths symbols → macros) | "keep"
cell_prompts = false       # notebook cells wrapped in writercell{In|Out}{n}
wrap = 0                   # prose line-wrap column; 0 = none

[typst]
strict = false             # W-TYP-* and E-TYP-MATH-CODE become errors for HTML/DOCX/LaTeX;
                           # --strict additionally escalates unsupported-construct and math-conversion-failed
package_paths = []         # extra local package roots ({root}/{namespace}/{name}/{version})

[notebook]
mime_priority = ["text/html", "image/svg+xml", "image/png", "image/jpeg", "image/gif", "image/webp",
                 "text/markdown", "text/latex", "application/json", "text/plain"]
hide_input_tags  = ["hide-input", "remove-input"]
hide_output_tags = ["hide-output", "remove-output"]
remove_cell_tags = ["remove-cell", "hide-cell", "skip"]
cell_options = true        # read quarto-style `#| key: value` lines
show_prompts = true        # In [n] / Out [n] in HTML and DOCX

[serve]
host = "127.0.0.1"
port = 3000
open = false
view = ""                  # "" → "pages" for .typ inputs, "web" otherwise | "web" | "pages"
```

> **Contract change (coordinator, 2026-10-24):** `[math] mathml_font` applies whenever MathML is
> emitted, not only under `html = "mathml"`: typst-dialect equations render as MathML under
> `html = "katex"` (the default) and `"mathml"`, so with the default `"embed"` they also inline
> Latin Modern Math (math.md §4.3, §4.5; html-output.md; vendored-assets.md §2.1). The size is the
> base64 data URI's (388,212 B × 4/3 ≈ 518 KB), not the woff2's. `default_toml()` carries the
> same two comment lines.

> **Contract change (coordinator, 2026-10-25):** the `[typst] strict` comment now says exactly what
> the key escalates (`W-TYP-*`, math ones included, and `E-TYP-MATH-CODE`) and that only `--strict`
> also escalates `unsupported-construct` and `math-conversion-failed`; the rule and where it is
> applied are in §4 ("Strict escalation"). `default_toml()` carries the same two comment lines.

The tables above are the schema; `docs/spec/math.md`, `latex-output.md` and `docx-output.md`
explain each of their keys and may not add or rename keys without a contract change here.

## 3. Front matter mapping (Markdown and notebooks)

| front matter key | goes to |
|---|---|
| `title`, `subtitle`, `date`, `description`, `lang`, `keywords` (string or list) | `DocMeta.*` |
| `author` (string) / `authors` (list of strings, or list of `{name, affiliation, email, id}`) | `DocMeta.authors` (`author:` as a list is accepted with a `front-matter-author-list` warning: presenter rejects it) |
| `abstract` (string, Markdown) | `DocMeta.abstract_` (parsed by the frontend; T10 first stores the text in `vars["abstract"]`) |
| `toc` (bool or int) | `[document] toc` |
| `numbering` (bool → headings; or map with `headings`, `figures`, `equations`) | `[numbering]` |
| `bibliography` (path or list), `cite_style` / `csl`, `cite_locale` | `[references] bibliography`, `style`, `locale` |
| `theme` | `[html] theme` |
| `template` | `[pdf] template` |
| `reference_doc` | `[docx] reference_doc` |
| `latex_template` | `[latex] template` |
| `paper` | `[pdf] paper` |
| `markdown:`, `math:`, `numbering:`, `references:`, `html:`, `pdf:`, `docx:`, `latex:`, `typst:`, `notebook:` (maps) | override the same `writer.toml` tables for this document |
| anything else | `DocMeta.vars` |

> **Contract change (coordinator, 2026-10-26):** the `abstract` row now identifies the
> frontend as its parser. T10 stores only its text; T12/T18 parse it after applying the
> document's config, as markdown-dialect.md §2.2 already requires.

The typst backend uses the same vocabulary with kebab-case keys as named arguments of
`writer-doc.with(...)` (`cite-style`, `cite-locale`, `reference-doc`, `numbering: (headings: "1.1",
figures: "section", equations: "labelled")`), typst-dialect.md §3.

> **Contract change (coordinator, 2026-10-24):** new front-matter key `cite_locale` →
> `[references] locale`, the twin of the `writer-doc` argument `cite-locale` that references.md
> (Deviation 9 (g)) added to typst-dialect.md §3. The vocabulary stays symmetric: typst-dialect.md
> §3's `cite-locale` row follows the generic `-` → `_` rule (→ `vars.cite_locale`, read by
> `merge_front_matter`) with no special case, and markdown-dialect.md §2.2's list of config-like
> keys gains `cite_locale` (it already reached `vars` as "everything else").

Paths in front matter resolve against the document's directory; paths in `writer.toml`
against that file. As in presenter, the table maps are stored in `DocMeta.vars` and
`Config::merge_front_matter(&DocMeta)` reads them from there (idempotent, unknown keys → error
with a hint). `yes/no/on/off` are strings (YAML 1.2 core schema via `serde_norway`).

## 4. Rust shape (`src/config.rs`)

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)] #[serde(default, deny_unknown_fields)]
pub struct Config { pub document: DocumentConfig, pub build: BuildConfig, pub markdown: MarkdownConfig,
                    pub numbering: NumberingConfig, pub references: ReferencesConfig, pub math: MathConfig,
                    pub html: HtmlConfig, pub pdf: PdfConfig, pub docx: DocxConfig, pub latex: LatexConfig,
                    pub typst: TypstConfig, pub notebook: NotebookConfig, pub serve: ServeConfig }
// one struct per table, all `#[serde(default, deny_unknown_fields)]`, `impl Default` giving §2 values.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] #[serde(rename_all = "lowercase")]
pub enum Target { Html, Pdf, Docx, Latex, Typst }

pub enum Toc { Off, Depth(u8) }                              // serde: false | true | int
pub enum HeadingNumbering { Off, Pattern(String) }           // false | true (= "1.1") | string
pub enum FigureNumbering { Document, Section }
pub enum EquationNumbering { Labelled, All, None }
pub enum HtmlMath { Katex, Mathml, Image }
pub enum MathmlFont { Embed, System }
pub enum PdfEngine { Typst, Chrome }                         // [pdf] engine, --engine
pub enum ServeView { Web, Pages }                            // "" in TOML = None (decided per input kind)
// HtmlMath, MathmlFont, PdfEngine: #[serde(rename_all = "lowercase")] (the §2 strings)

pub struct MathConfig { pub html: HtmlMath, pub mathml_font: MathmlFont,
    pub katex_options: serde_json::Map<String, serde_json::Value> }  // {} by default; math.md §3.6 macros_from_config

impl Config {
    pub fn load(path: &Path) -> Result<Config>;
    pub fn from_toml_str(text: &str) -> Result<Config>;
    pub fn discover(start_dir: &Path) -> Result<Option<(PathBuf, Config)>>;
    pub fn merge_front_matter(&mut self, meta: &DocMeta) -> Result<()>;
    pub fn apply_cli(&mut self, overrides: &CliOverrides);
    pub fn targets_for(&self, kind: SourceKind) -> Vec<Target>;  // build.targets or build.typst_targets
    pub fn default_toml() -> String;             // `writer dump --default-config`, checked by tests/guide.rs
}
pub struct CliOverrides { pub targets: Option<Vec<Target>>, pub out: Option<PathBuf>, pub theme: Option<String>,
    pub template: Option<PathBuf>, pub reference_doc: Option<PathBuf>, pub latex_template: Option<PathBuf>,
    pub standalone: Option<bool>, pub port: Option<u16>, pub host: Option<String>, pub open: Option<bool>,
    pub view: Option<ServeView>, pub strict: Option<bool>, pub engine: Option<PdfEngine>, pub allow_packages: Option<bool> }
```

> **Contract change (coordinator, 2026-10-24):** the shape now defines `PdfEngine` (used by
> `CliOverrides.engine` and `[pdf] engine`), `MathmlFont`, and `MathConfig` with
> `katex_options: serde_json::Map<String, serde_json::Value>`, the type that
> `math::latex_source::macros_from_config` takes (math.md §3.6) and that typst-dialect.md §5.4,
> docx-output.md §4.15, latex-output.md §4.8 and html-output.md §4.3 (`script_safe_json`) use.

**Strict escalation.**
- `apply_cli` with `strict: Some(b)` sets `typst.strict = b`; `None` leaves it unchanged.
- Escalation happens in one place: the pipeline (T16, `pipeline::build_targets`, live-reload.md
  §5.2), after it has collected prepare's and every target's diagnostics. It reads
  `opts.cli.strict` (`BuildOptions.cli`) and the effective config (`Prepared.config`, after
  `merge_front_matter` and `apply_cli`).
- When `opts.cli.strict == Some(true)`, every `Level::Warning` diagnostic whose code is
  `unsupported-construct`, `math-conversion-failed` or `E-TYP-MATH-CODE`, or starts with `W-TYP-`,
  becomes `Level::Error` (code unchanged).
- Otherwise, when the effective `config.typst.strict` is true, only `E-TYP-MATH-CODE` and
  `W-TYP-*` (the `W-TYP-MATH-*` ones included) are re-levelled that way.
- `[typst] strict` never escalates `unsupported-construct` or `math-conversion-failed`, so setting
  it in `writer.toml` does not fail the DOCX/LaTeX build of a Markdown document over them.
- Re-levelling is idempotent. Producers that already escalate under `[typst] strict` (the typst
  frontend, typst-dialect.md §2.1) therefore stay correct.
- Emitters never read a strict flag (the CLI flag does not reach `DocxOptions.config` or
  `latex::emit`'s `config`). They always raise `unsupported-construct` and
  `math-conversion-failed` as warnings. "Error with `--strict`" in docx-output.md §8,
  latex-output.md §8 and html-output.md means this pipeline step.
- No other code is escalated by either switch (`math-syntax`, `math-tag-ignored` and
  `math-latex-package` stay as they are, math.md §9).

> **Contract change (coordinator, 2026-10-25):** "Strict escalation" is new. Before it, no spec
> said what `--strict` does to the `Config`, and the emitters that "error with `--strict`" only
> receive `&Config`. Coordinator follow-ups (other files):
> - latex-output.md §8's `W-TYP-MATH-*` row also escalates under `[typst] strict`, matching
>   typst-dialect.md §2.1 ("every `W-TYP-*`").
> - cli.md's `--strict` should name this subsection as the mechanism.

Precedence is implemented by calling defaults → `load/discover` → `merge_front_matter` →
`apply_cli`. Write a unit test for every precedence pair.

## 5. Determinism
`default_toml()` is byte-stable and documented in the guide; snapshot tests depend on it.
Config values never leak timestamps or machine paths into outputs (an `out_dir` is never
written into an output).
