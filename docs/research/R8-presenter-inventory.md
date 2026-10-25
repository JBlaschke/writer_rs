# R8 — presenter_rs inventory for reuse in writer_rs

Date: 2026-10-24. Source tree inspected: `/Users/johannes.blaschke/Developer/presenter_rs`
(git `main`, package `presenter` 0.1.0, edition 2024, `rust-version = "1.89"`). Every fact
below was read from the files named; LOC are `wc -l` counts. 54 792 lines under `src/` +
`build.rs` + `assets/presenter/*.js`; 28 402 lines of integration tests under `tests/`.

Verdict legend: **LIFT** = copy verbatim (rename `presenter`→`writer` identifiers only);
**ADAPT** = copy, then change what the "adaptation" column says; **SKIP** = reveal.js / slide /
pptx-specific, do not port.

---

## 1. Crate, toolchain and repo scaffolding

### 1.1 Cargo.toml (verified)

- `[package] name = "presenter"`, `version = "0.1.0"`, `edition = "2024"`,
  `rust-version = "1.89"` (comment: "comrak 0.56 requires 1.89 (edition 2024 alone would need 1.85)"),
  `publish = false`. `[lib] name = "presenter"` + `[[bin]] name = "presenter"`.
- `[features] browser-tests = []` (gates `pdf::devtools` and `tests/browser.rs`).
- `[profile.release] opt-level = 3, lto = "thin", codegen-units = 1, strip = true`.
- Dependencies (Cargo.toml spec → exact Cargo.lock version → latest on crates.io on 2026-10-24
  via `cargo search`):

| crate | Cargo.toml | Cargo.lock | crates.io latest | features / notes |
|---|---|---|---|---|
| anyhow | 1 | 1.0.104 | — | CLI error plumbing |
| thiserror | 2 | 2.0.21 | — | `Error` enum |
| clap | 4.6 | 4.6.7 | 4.6.7 | `derive`, `wrap_help` |
| serde | 1 | 1.0.229 | — | `derive` |
| serde_json | 1 | 1.0.151 | — | `preserve_order` |
| toml | 1.1 | 1.1.7+spec-1.1.0 | — | config |
| serde_norway | 0.9 | 0.9.42 | 0.9.42 | YAML front matter (serde_yaml fork) |
| comrak | 0.56 | 0.56.0 | **0.57.0** | `default-features = false, features = ["attributes"]` |
| minijinja | 2.24 | 2.24.0 | **3.0.3** (major bump) | `loader`, `json`, `loop_controls` |
| rust-embed | 8.13 | 8.13.0 | 8.13.0 | `include-exclude` |
| notify | 8.2 | 8.2.0 | 9.0.0-rc.6 (prerelease; 8.2.0 stable) | watcher |
| notify-debouncer-full | — | not in lock | — | presenter does its own debounce |
| tokio | 1 | 1.53.2 | 1.54.1 | `rt-multi-thread, macros, sync, signal, time, fs` |
| axum | 0.8 | 0.8.9 | 0.8.9 | dev server |
| tokio-stream | 0.1 | 0.1.19 | — | `sync` (SSE) |
| tracing / tracing-subscriber | 0.1 / 0.3 | 0.1.44 / 0.3.23 | — | `env-filter` |
| base64 | 0.23 | 0.23.1 | 0.23.1 | data URIs |
| blake3 | 1 | 1.8.7 | 1.8.7 | asset ids, content versions |
| mime_guess | 2 | 2.0.5 | — | |
| html-escape | 0.2 | 0.2.15 | — | |
| ansi-to-html | 0.2 | 0.2.4 | — | notebook streams |
| regex | 1 | 1.13.1 | — | |
| walkdir | 2 | 2.5.0 | — | |
| open | 5 | 5.4.4 | — | `serve --open` |
| lol_html | 3 | 3.0.1 | 3.0.2 | postprocess (reveal `.element:` comments, incremental lists) |
| grass | 0.13 (no default features) | 0.13.4 | 0.13.4 | SCSS themes |
| codemap | 0.1 | 0.1.3 | — | grass error spans |
| insta (dev) | 1.49 | 1.49.0 | 1.49.0 | `json`, `filters`, `redactions` |
| assert_cmd (dev) | 2 | 2.2.2 | 2.2.2 | |
| predicates (dev) | 3 | 3.1.4 | — | |
| tempfile (dev) | 3 | 3.27.0 | — | |
| ureq (dev) | 3 | 3.4.2 | 3.5.0 | server tests |
| (candidates for writer, not in presenter) | | | zip 9.0.5, docx-rs 0.4.22, typst 0.15.1 | see open questions |

Licenses were not re-checked per crate in this pass (release.yml generates
`THIRD-PARTY-LICENSES/crates.txt` from `cargo metadata`, so the mechanism exists).

### 1.2 Lint / format / cargo config (verified)

- `clippy.toml`: `too-many-arguments-threshold = 8`.
- `rustfmt.toml`: `edition = "2024"`.
- `.cargo/config.toml`: `[target.x86_64-pc-windows-msvc] rustflags = ["-C", "target-feature=+crt-static"]`
  and the same for `aarch64-pc-windows-msvc`; `[source.crates-io] replace-with = "vendored-sources"`,
  `[source.vendored-sources] directory = "vendor"` (271 vendored crate dirs; `cargo vendor` output).
- `.gitattributes`: `* text=auto eol=lf`, `vendor/** -text`. `.gitignore`: `/target`, `/dist`,
  `examples/**/dist/`, `.DS_Store`, `*.snap.new`.
- `src/lib.rs` (30): `#![deny(unsafe_code)] #![warn(missing_docs)]`; `pub mod assets, check, cli,
  config, error, ir, markdown, notebook, pdf, pipeline, pptx, render, serve, theme, vendor;
  pub use error::{Error, Result};`.
- `src/main.rs` (21): checks `styles::WORKER_ARG` first (Sass worker re-exec), then
  `styles::isolate_in_worker(exe)` then `presenter::cli::run()`.

### 1.3 build.rs (305 LOC) — LIFT

Sets `cargo::rustc-env=PRESENTER_GIT_SHA=<id>` and `PRESENTER_BUILD_ID_SOURCE=override|git|unknown`.
Id precedence: env `PRESENTER_GIT_SHA` (trimmed; blank ignored) → `git describe --always` plus
`-dirty` when `git --no-optional-locks status --porcelain --untracked-files=no` is non-empty →
`unknown`. Characters outside `[A-Za-z0-9.+-]` become `-`. `rerun-if-changed`: `build.rs`,
`Cargo.toml`, `Cargo.lock`, `src`, plus `assets` only when `CARGO_CFG_DEBUG_ASSERTIONS` is absent
(release embeds assets), plus the git paths from `git rev-parse --git-common-dir --git-path HEAD
--git-path logs/HEAD --git-path packed-refs --git-path refs/tags --git-path reftable` and the
current branch ref (linked worktrees and reftable handled). `run_git` strips
`GIT_DIR/GIT_WORK_TREE/GIT_INDEX_FILE/GIT_COMMON_DIR` and sets `GIT_CEILING_DIRECTORIES` to the
crate's parent so a parent repository is never used. Key API:
`pub(crate) fn build_id(env_override: Option<&str>, git: &str, crate_dir: &Path) -> BuildId`,
`struct BuildId { id, source: Source, watch: Vec<PathBuf>, warnings }`, `enum Source { Override, Git, Unknown }`.
`tests/build_script.rs` includes `build.rs` as a module (`#[path = "../build.rs"]`) and tests it
against scratch repos. Adaptation: rename the two env vars to `WRITER_*`.

### 1.4 scripts/vendor.sh (11 727 B) — ADAPT

Pins (name, version, npm tarball URL, SHA-256) for reveal.js 6.0.2, KaTeX 0.19.0, highlight.js
11.11.1, highlightjs-line-numbers.js 2.8.0, marked 17.0.5, fitty 2.4.2 (the last four are bundled
inside reveal's dist, only their LICENSE files are copied). Downloads each tarball to a temp dir,
verifies the hash, extracts, copies an explicit file list (reveal `dist/**` minus maps, `css/theme`
SCSS sources and fonts; KaTeX `LICENSE`, `katex.min.js`, `katex.min.css`, `fonts/*.woff2` only),
writes `LICENSE-OFL.txt` for League Gothic, writes `assets/vendor/VERSIONS.toml` (`date`, one
`[packages.<name>]` with `version`, `sha256`, `url`), and swaps the result atomically into
`assets/vendor/`. CI's `vendor-drift` job reruns it and `git diff --exit-code`s.
Adaptation for writer: drop reveal/fitty/marked; keep KaTeX; add highlight.js standalone (presenter
gets it from reveal's plugin bundle) and anything the HTML backend needs (e.g. a print stylesheet);
keep the VERSIONS.toml + hash-pin + atomic-swap mechanics verbatim.

### 1.5 assets/ layout (verified by listing)

```
assets/
  presenter/live-reload.js   (234 LOC, 8 986 B; CI checks `node --check` and size < 10240)
  presenter/math.js          (28 LOC; window.__presenterRenderMath(root))
  scaffold/{.gitignore, README.md, presenter.toml, slides.ipynb, slides.md,
            theme/{theme.toml, styles/theme.css, templates/layouts/title.html}}
  themes/default/{README.md, theme.toml, styles/{theme.css, notebook.css},
                  templates/{deck.html, slide.html, block.html, cell.html,
                             layouts/{blank,default,section,title}.html,
                             partials/{head,logo,scripts}.html}}
  vendor/VERSIONS.toml
  vendor/katex/{LICENSE, katex.min.js, katex.min.css, fonts/*.woff2 (20)}
  vendor/reveal/{LICENSE, css/theme/**/*.scss, css/theme/fonts/**, dist/{reveal.js, reveal.css,
                 reset.css, theme/*.css (14), plugin/{highlight,notes,zoom,search}.js,
                 plugin/highlight/{monokai,zenburn}.css}}
  vendor/{highlight.js, highlightjs-line-numbers.js, marked, fitty}/LICENSE*
```

`src/vendor.rs` (152) — LIFT: `#[derive(rust_embed::Embed)] #[folder = "assets/"]
#[include = "vendor/**/*"] #[include = "presenter/**/*"] #[include = "themes/**/*"]
#[include = "scaffold/**/*"] #[exclude = "**/*.map"] #[exclude = "**/.DS_Store"] #[exclude = "**/._*"]
#[exclude = "**/Thumbs.db"] #[exclude = "**/*.swp"] #[exclude = "**/*~"] pub struct Assets;` with
`pub fn get(path: &str) -> Option<Cow<'static, [u8]>>` (lexically normalized, exact case even in
debug where rust-embed reads from disk), `pub fn get_str(path: &str) -> Result<String>`,
`pub fn list(prefix: &str) -> Vec<String>` (sorted). Rename `presenter/` include to `writer/`.

---

## 2. Architecture (as implemented)

```
input (.md | .ipynb)
  └─ frontends: markdown::parse / notebook::parse  ──►  Parsed { deck: Deck, assets: AssetRegistry, diagnostics }
       (includes expanded in markdown::include; config merged from presenter.toml + front matter)
  └─ theme::resolve(spec, search_roots) ──► Theme (chain of ThemeDir, minijinja env, compiled CSS)
  └─ render::render(RenderInput) ──► RenderedDocument { html, theme_css, files, slide_count, diagnostics, theme_files }
       (templates get helpers style()/script()/asset()/theme_style()/reveal_init()/live_reload())
  └─ assets::emit::Emitter resolves `asset:<12hex>` placeholders per OutputMode {Standalone, Site, Serve}
  └─ pipeline::write_output (clash-safe) | serve::server (axum, SSE) + serve::watch (notify) + live-reload.js
```

Error model: `error::Error` (thiserror enum) for failures, `error::Diagnostics` for warnings/errors
collected during a build; `cli::report` prints both rustc-style.

---

## 3. Module inventory

### 3.1 Core: ir / error / config / pipeline / check / vendor

| path | LOC | purpose | public API (short) | deps | verdict |
|---|---|---|---|---|---|
| `src/ir.rs` | 587 | Deck IR (`docs/spec/ir.md`) | `Deck{meta,slides,features,source,assets,includes}` + `invariant_violations()`; `DeckMeta{title,subtitle,author,date,description,lang,theme,reveal:Map,vars:Map}`; `Slide{id,layout,classes,attrs:BTreeMap,title,blocks,notes,children,span}`; `enum Block{Html{html},Cell(NotebookCell)}` (`tag="type"`); `NotebookCell{index,id,language,source,execution_count,outputs,hide_input,hide_output,fragment,tags,metadata}`; `enum CellOutput{Stream{name,html},Error{ename,evalue,html},Display{mime,html,execution_count},Unsupported{mimes}}` (`tag="kind"`); `Features{math,code,notes,notebook}` + `merge`; `SourceInfo{path,kind,base_dir}`; `enum SourceKind{Markdown,Notebook}`; `SourceSpan{start_line,end_line,cell,file}`; `AssetInfo{id,mime,origin}`; `Parsed{deck,assets,diagnostics}` | serde, serde_json | **ADAPT → rewrite**: keep `Features`, `SourceInfo`, `SourceKind`, `SourceSpan`, `AssetInfo`, `Parsed`, `NotebookCell`, `CellOutput`; replace `Deck/Slide` with `Document/Section/Block` (see §9) and `DeckMeta.reveal` with nothing |
| `src/error.rs` | 1344 | `Error`, `Diagnostic(s)`, excerpts, template-error mapping | `enum Error{Parse{path,line,message},FrontMatter(String),Config(String),Theme{name,message},Template{template,line,message},Style{file,message},MissingAsset{path,span},Io(#[from]),Json(#[from]),Other(String)}`; `path()`, `span()`, `column()`, `message()`, `excerpt(&self,file,&dyn Fn(&Path)->Option<String>)->Option<Excerpt>`, `quoted_line()`; `EXCERPT_WIDTH=120`; `Excerpt{line,text,marker}`; `source_line(text,&SourceSpan)`; `From<toml::de::Error>`, `From<minijinja::Error>`, `template_error(err,debug)`; `enum Level{Warning,Error}`; `Diagnostic{level,code:&'static str,message,path,span}` (`warning/error/new/with_path/with_span`; Display `path:line: level: message`, `{:#}` adds `[code]`); `Diagnostics(Vec)` (`push,warn,error,iter,has_errors,len,is_empty,into_vec`, Extend/FromIterator/IntoIterator) | thiserror, toml, minijinja | **LIFT** (keeps notebook `cell N, line M:` span parsing) |
| `src/config.rs` | 762 | `presenter.toml` model, discovery, merge | `CONFIG_FILE_NAME`; `Config{deck,build,slides,markdown,reveal,math,notebook,serve}` all `#[serde(default, deny_unknown_fields)]`; `DeckConfig{theme:"default",lang:"en",title,author,date (scalar_text)}`; `BuildConfig{out_dir:"dist",standalone:true,minify:false}`; `SlidesConfig`; `MarkdownConfig{smart_punctuation,sourcepos}`; `MathConfig{enabled:MathMode{Auto,Always,Never},katex_options:Map}`; `NotebookConfig{mime_priority,hide_input_tags,hide_output_tags,remove_cell_tags,split_markdown_cells,show_prompts}`; `ServeConfig{host:"127.0.0.1",port:3000,open:false}`; `CliOverrides{theme,out,standalone,port,host,open}`; `Config::{from_toml_str,load,discover(start_dir)->Result<Option<(PathBuf,Config)>>,merge_front_matter,apply_cli,...}`; `unknown_key_hint`, `with_unknown_key_hint`, `canonical_json` | serde, toml, serde_json | **ADAPT**: drop `slides`, `reveal`, `PluginId`; rename `deck`→`document`; add `[outputs]`/`[html]`/`[pdf]`/`[docx]`/`[latex]`/`[typst]` tables; keep discovery, hint (levenshtein) helpers, `scalar_text` |
| `src/pipeline.rs` | 2025 | build orchestration, output writing, clash safety | `SITE_INDEX="index.html"`, `SITE_ASSETS_DIR="assets"`; `BuildOptions{input,config_path,cli,output:OutputMode,build:BuildInfo,missing_asset_is_error,format:Option<SourceKind>}` + `new(input)`; `Prepared{deck,assets,theme,config,config_path,diagnostics}` + `render_input`; `Build{deck,assets,theme,config,config_path,document:RenderedDocument,diagnostics}`; `BuildFailure{error,diagnostics}`; `build_deck`, `try_build_deck`, `try_build_deck_inspecting`, `prepare`, `try_prepare`, `parse_only`, `try_parse_only`, `load_config(opts)->Result<(Config,Option<PathBuf>)>`, `detect_kind(path)->Result<SourceKind>`, `default_output_path`, `output_mode(standalone,out)`, `enum InputRole{Source,Config,Asset,Theme,Include}`, `Clash{output,input,role}`, `input_files(&Build)`, `find_clash`, `same_file`, `write_output(&Build,out,&OutputMode)->Result<Vec<PathBuf>>`, `missing_asset_as_error`, `resolve_theme` | everything | **ADAPT**: one `Build` per backend/format; keep `load_config` (out_dir relative to the toml that sets it), `detect_kind`, clash/overwrite refusal logic verbatim |
| `src/check.rs` | 1955 | `presenter check` lints | `run(&BuildOptions)->Result<Diagnostics>`; codes `SEPARATOR_NOT_RECOGNIZED, DUPLICATE_SLIDE_ID, UNKNOWN_LAYOUT, NOTEBOOK_NO_OUTPUTS, THEME_ERROR, IO_ERROR, PARSE_ERROR, FRONT_MATTER_ERROR, CONFIG_ERROR, TEMPLATE_ERROR, STYLE_ERROR, MISSING_ASSET, ERROR`; `error_diagnostic(&Error,input)->Diagnostic`; `duplicate_slide_ids`, `unknown_layouts`, `notebooks_without_outputs`, `style_problem`, `included_lints`, `shell_quoted` | pipeline, theme | **ADAPT**: drop separator lints; keep `error_diagnostic`, duplicate-id, notebook-no-outputs, theme/template/style checks |
| `src/vendor.rs` | 152 | embedded assets | see §1.5 | rust-embed | **LIFT** |

### 3.2 markdown/*

| path | LOC | purpose | public API (short) | deps | verdict |
|---|---|---|---|---|---|
| `markdown/mod.rs` | 885 | frontend entry; comrak options; slide loop | `parse(&SourceInfo,&str,&Config)->Result<Parsed>`; `parse_with_includes`; `parse_included(source,text,config,&Context)->Result<Outcome>`; `comrak_options(&MarkdownConfig)->Options<'static>` (exact §4.1 below); `starts_with_empty_front_matter`, `mark_empty_front_matter`, `build_slide`, `postprocess_slide`, `finish_diagnostics` | comrak | **ADAPT**: keep `comrak_options`, front-matter handling, diagnostics finishing; replace the split/slide loop with a section builder |
| `markdown/frontmatter.rs` | 1618 | YAML front matter → `DeckMeta` | `split_front_matter(text)->Option<(&str,&str,usize)>`; `strip_delimiters`; `parse_front_matter(yaml)->Result<DeckMeta>` (serde_norway visitors; `STRING_FIELDS` = 7 keys; `TABLES` = reveal/slides/markdown/math/notebook must be mappings; `yaml_hint` for `: ` and tabs); `parse_deck_front_matter(yaml,closing,&mut Diagnostics)` (codes `front-matter-empty`, `front-matter-unclosed`); `swallowed_heading`; `meta_from_json(&Value)` | serde_norway, serde_json | **LIFT** core; change `STRING_FIELDS`/`TABLES` lists |
| `markdown/attrs.rs` | 1231 | pandoc attribute blocks | `ParsedAttrs{id,classes,pairs}` + `From<&comrak::nodes::Attributes>`; `parse_attr_block(&str)->Result<ParsedAttrs>` (braces optional; unbalanced quote → `Error::Other`); `enum ElementAttrs{Attrs(ParsedAttrs),Text,Invalid(String)}`; `read_element_attrs(block)->ElementAttrs`; `is_valid_attr_name(key)->bool`; `SlideAttrs{..}` + `normalize_key` + `DATA_SHORTHANDS` (26) + `CSS_COLOR_KEYWORDS` (150) | comrak | **LIFT** `ParsedAttrs`, `parse_attr_block`, `read_element_attrs`, `is_valid_attr_name`, tokenizer + differential tests; **SKIP** `SlideAttrs`, `normalize_key`, shorthand tables |
| `markdown/recover.rs` | 1015 | re-read attribute blocks comrak rejected | `UNPARSED_ATTR_BLOCK`; `recover_attr_blocks<'a,'t>(arena,root,text,opts,diags)->(&'a AstNode<'a>,Cow<'t,str>)` (rewrite to double-quoted form, re-parse ≤ `MAX_ROUNDS=3`, verify with `read_by_comrak`); `fence_attrs(info)->Option<(&str,&str,ElementAttrs)>`; `unparsed(what,block,reason)->Diagnostic`; `split_lines`, `line_starts` | comrak | **LIFT** |
| `markdown/code.rs` | 344 | fenced code → `<pre><code>` | `CodeAttrs{lang,pre_id,pre_classes,code_attrs,escape}`; `code_attrs(info,Option<&ParsedAttrs>)->CodeAttrs` (`lines`→`data-line-numbers`, `.numbered`, `start`→`data-ln-start-from`, `trim=false`, `escape=false`→`data-noescape`, `.plain`→`nohighlight`); `render_code_block(source,&CodeAttrs)->String`; `render_code_block_with(..,sourcepos)`; `push_attr`, `strip_trailing_newline` | html-escape | **ADAPT**: keep structure; replace reveal/highlight-plugin `data-*` conventions with writer's (line numbers, highlight ranges, captions) |
| `markdown/render.rs` | 2908 | AST → HTML with assets, math, directives, footnotes | `enum RenderMode{Deck,Notebook}`; `RenderCtx<'r>{assets:&mut AssetRegistry,base_dir,mode,diagnostics,missing_asset_is_error}`; `Rendered{html,notes,features,title}`; `render_nodes(arena,nodes,opts,ctx)`; `render_fragment(text,opts,ctx)`; `render_fragment_with_lift(..)->Result<(Rendered,Option<ParsedAttrs>)>`; `missing_asset_path(&Diagnostic)`; `has_math_markup(html)`; `html_start_tags(html)->Vec<(String,usize)>`; `html_attr_values(html)->Vec<(String,Range<usize>)>`; `rewrite_html_assets(&mut String,span,ctx)`; `HtmlScan` tokenizer; `local_asset`, `case_mismatch`/`DirListings`, `is_relative_url`, `percent_decode`; footnotes (`prepare_footnotes`, `<div class="footnotes" data-footnotes><ol>`); `format_node` override via `comrak::html::format_document_with_formatter` (BlockDirective→`<div class="name …">`, `directive_info()` pandoc form, Math→`<span data-math-style="inline|display">`, ```math→`<div data-math-style="display">`); `DirectiveInfo{name,classes,id,args,problems}`; `is_notes_info`, `is_include_info`, `check_unopened_directives` | comrak, regex | **ADAPT**: keep HTML scanner, asset rewriting, math markup, element-attr emission, footnotes, directive parsing; replace the reveal directive table (`fragment`, `columns`) with writer's (`figure`, `callout`, `note/warning/tip`, `table`, `include`, `pagebreak`) |
| `markdown/include.rs` | 1106 | `:::include` expansion | `MAX_DEPTH=8`; codes `INCLUDE_ATTRS_IGNORED`, `INCLUDE_META_IGNORED`, `INCLUDE_IN_NOTEBOOK`; `Include{line,path}`; `IncludedSource{path,kind,config}`; `Context{files,root_dir,meta}` (`root/with_meta/enter/chain_to`); `Expanded{slides,assets,features,diagnostics,includes,sources}`; `find_includes(..)`, `include_path(info)->Result<String,String>`, `expand(&Include,&SourceInfo,&Config,&Context,&mut Diagnostics)->Result<Expanded>`, `assemble`, `differing_meta`, `scan_includes(path)->Vec<PathBuf>` (for the watcher) | comrak | **ADAPT**: allow block-level include anywhere (not horizontal-slide-only); keep path parsing/quoting, cycle + depth detection, `scan_includes`, diagnostic file tagging |
| `markdown/split.rs` | 3041 | slide splitting (`---`/`--`, headings mode) | `SplitMode`, `RawSlide`, `RawDeck`, `split`, `split_ast`, `split_with_source`; helpers `lifted_heading`, `lift_heading_attrs(nodes)->Option<ParsedAttrs>`, `first_heading_text(nodes)->Option<String>`, `source_lines(text)->Vec<&str>`, `first_text_line`, `text_lines`/`definition_lines`, `unsmart`, `match_attr_separator`, `take_slide_comments` | comrak | **SKIP** bulk; **LIFT** `source_lines`, `first_heading_text`, `lift_heading_attrs`, `push_plain_text` into a small `markdown/headings.rs` |
| `markdown/compat.rs` | 640 | reveal.js Markdown compatibility (`<!-- .slide: -->`, `Note:`) | `slide_comment*`, `element_comment`, `is_blank`, `is_invisible`, `is_comment_only`, `note_marker`, `image_background`, `render_slide` | comrak | **SKIP** (keep `is_blank`/`is_comment_only` if needed) |
| `markdown/postprocess.rs` | 775 | lol_html passes | `incremental_enabled`, `slide_html`, `element_attributes(html,span,diags)` (`<!-- .element: -->`), `incremental_lists(html)`, `close_implied` | lol_html | **SKIP** (lol_html dependency goes away unless writer wants an HTML post-pass) |

### 3.3 notebook/*

| path | LOC | purpose | public API (short) | deps | verdict |
|---|---|---|---|---|---|
| `notebook/mod.rs` | 156 | entry | `parse(source,json,config)->Result<Parsed>`; `parse_deferring_config(..)->Result<(Parsed,Option<Error>)>` | — | **ADAPT** (calls convert) |
| `notebook/nbformat.rs` | 744 | nbformat 4 model, lenient parse | `Notebook{cells,metadata,nbformat,nbformat_minor}`; `Cell{cell_type,id,metadata,source:MultilineString,outputs,execution_count,attachments}`; `enum CellType{Markdown,Code,Raw,#[serde(other)] Unknown}`; `enum Output{Stream{name,text},DisplayData{data,metadata},ExecuteResult{data,metadata,execution_count},Error{ename,evalue,traceback}}` (`tag="output_type"`); `MimeBundle=BTreeMap<String,Value>`; `MultilineString(pub String)`; `mime_text(&Value)->Option<String>`; `SlideType` + `parse`; `Cell::{slide_type,tags,is_hidden_source,is_hidden_outputs,raw_mimetype,presenter_overrides,span(index)}`; `Notebook::{language,title,presenter_meta}`; `parse_notebook(path,json)->Result<(Notebook,Diagnostics)>` (codes `UNKNOWN_SLIDE_TYPE, UNKNOWN_CELL_TYPE, OUTPUT_SKIPPED, ATTACHMENT_SKIPPED, PRESENTER_NOT_OBJECT, NBFORMAT_NEWER`); `parse_outputs(Vec<Value>)->(Vec<Output>,Vec<String>)` | serde_json | **LIFT** (rename `metadata.presenter` key → `metadata.writer`; `SlideType` optional) |
| `notebook/outputs.rs` | 1631 | MIME bundle → HTML | codes `OUTPUT_SCRIPT/UNSUPPORTED/INVALID/WIDGET/FILE_OUTSIDE`; `OutputFiles{base_dir,markdown:&Options}`; `CellOutputs{outputs,features,notes}`; `render_outputs(outputs,&NotebookConfig,&mut AssetRegistry,&mut Diagnostics,md:&mut dyn FnMut(&str)->Result<String>)->Result<(Vec<CellOutput>,Features)>`; `render_cell_outputs(outputs,cfg,&OutputFiles,assets,diags)->Result<CellOutputs>`; `choose_mime`; `render_display(mime,payload,metadata,assets)->Result<(String,bool)>`; `ansi_to_html(text)->String`; `strip_math_delimiters`; `apply_carriage_returns`; SVG root attrs | ansi-to-html, base64 | **LIFT** |
| `notebook/convert.rs` | 2520 | Notebook → Deck | `convert(nb,source,&mut Config,&mut AssetRegistry,&mut Diagnostics)->Result<Deck>`; `convert_deferring_config`; codes `EMPTY_SLIDE, UNKNOWN/INVALID_SLIDE_ATTR, OVERRIDE_IGNORED/INVALID, RAW_CELL_SKIPPED, MISSING_ATTACHMENT, FRONT_MATTER_IGNORED/EMPTY, SINGLE_SLIDE`; metadata layering (`metadata.title` → `metadata.presenter` → first-cell front matter; `overlay`); `Converter` walk (markdown cells via `render_fragment_with_lift`, code cells → `Block::Cell`, raw cells, notes cells); attachments (`attachment_url_candidates`, `embedded_urls`, `srcset_urls`, `css_urls`, `char_reference`, `decode_attachment`); `html_heading_text`; `relocate`; `cell_error` | comrak, serde_json | **ADAPT**: emit a flat block sequence (headings open sections) instead of slide structure; keep attachments, metadata layering, cell conversion, error relocation |

### 3.4 assets/*

| path | LOC | purpose | public API (short) | deps | verdict |
|---|---|---|---|---|---|
| `assets/registry.rs` | 618 | content-addressed asset registry | `ID_LEN=12`; `enum AssetData{File(PathBuf),Bytes(Vec<u8>)}`; `Asset{id,mime,data,file_name_hint}`; `AssetRegistry` (BTreeMap): `new, register_file(&Path)->String, register_bytes(Vec<u8>,&str,&str)->String, get, iter, infos(&Path)->Vec<AssetInfo>, placeholder(id)->String, merge, len, is_empty, read(id)->Result<Cow<[u8]>>`; `content_hash(&[u8])->String`; `resolve_path`, `normalize_lexically` | blake3, mime_guess | **LIFT** |
| `assets/emit.rs` | 1701 | placeholder resolution per output mode | `SERVE_ASSET_PREFIX="/__presenter/deck-asset/"`; `enum OutputMode{Standalone, Site{assets_dir,url_prefix}, Serve}`; `EmittedFile{path,bytes}`; `Emitter<'a>{registry,mode,files}`: `new, with_files, resolve_html(&str)->Result<String>, resolve_css, resolve_css_in_assets_dir, url_for(id)`; `data_uri(mime,bytes)->String`; `hashed_file_name(hint,id)->String` (`name-<hash>.ext`); `register_css_urls(css,&mut AssetRegistry,resolve_rel:&mut dyn FnMut(&str)->Option<AssetData>,lenient)->Result<String>`; `is_text_attribute(name)` (alt,title,placeholder,label,value,abbr,aria-*) | markdown::render (`html_attr_values`, `html_start_tags`) | **LIFT** (rename prefix to `/__writer/doc-asset/`); add a 4th mode later for docx/typst (`Files{dir}`) that just dumps bytes |

### 3.5 serve/* + live-reload.js

| path | LOC | purpose | public API (short) | deps | verdict |
|---|---|---|---|---|---|
| `serve/watch.rs` | 1748 | notify watcher, debounce, classification, build-watch loop | `DEBOUNCE=150ms`, `MAX_BATCH_WAIT=2s`, `Debounce{quiet,max_wait}`; `enum ChangeKind{Css,Content}` (serde lowercase); `WatchSet{files,dirs_recursive,dirs_flat,theme_dirs}` + `new(files,theme_dirs)`/`merge`; `watch_set_for(&Build)`, `watch_set_for_failed(&BuildOptions)`, `try_build_watched(&BuildOptions)->(Result<Build,BuildFailure>,WatchSet)`, `watch_set_before_build`; `ChangeBatch{paths,kind}`; `classify(paths,theme_dirs)->ChangeKind`; `is_ignored(rel)`; `canonical(path)`; `Watcher::{start(set,tx)->Result<Watcher>, start_with(set,tx,Debounce), update(&mut self,set), watched_dirs()}` (coalescing thread "presenter-watch", macOS FSEvents ctime filter `changed_before`, structural events, Windows guard watches, `catch_up`); `enum WatchStatus<'a>{Built{build,written},Failed(BuildFailure),Watching(&WatchSet),Changed(&ChangeBatch)}`; `run_build_watch_with(opts,out,report: impl FnMut(WatchStatus)->ControlFlow<()>)->Result<()>` | notify, pipeline, theme | **LIFT** (rename thread; `Build`/`BuildOptions` types come from writer's pipeline) |
| `serve/server.rs` | 1160 | axum server, SSE, host check | `EVENT_CAPACITY=64`, `SHUTDOWN_GRACE=2s`; `ServeState{build_id,document,theme_css,assets,theme,last_error}` + `hello()`; `type Shared=Arc<RwLock<ServeState>>`; `event_channel()`, `publish(&Sender,BuildEvent)->usize`, `next_build_id`, `apply_build(&Shared,&Build)->u64`, `apply_error`, `apply_error_payload`; `router(state,events)->Router` (routes `/`, `/__presenter/theme.css`, `/__presenter/asset/{*path}`, `/__presenter/deck-asset/{id}`, `/__presenter/events`, `/__presenter/live-reload.js`, `/__presenter/state`, `/favicon.ico`); `serve(listener,router)`, `serve_until(listener,router,shutdown)`, `serve_until_allowing(..,AllowedHosts)`; `AllowedHosts::{new(SocketAddr),with_name,allows}` (DNS-rebinding guard, 403); `fallback_page` | axum, tokio, tokio-stream | **LIFT** (rename routes to `/__writer/…`; fallback page without Reveal) |
| `serve/events.rs` | 756 | SSE payloads | `enum BuildEvent{Ok{build_id,kind:ChangeKind,duration_ms},Error{build_id,error:ErrorPayload}}` (`tag="status"`); `ErrorPayload{message,file,line,kind,excerpt}` + `From<&Error>`, `for_input(err,input)`, `relative_to`, `new(err,base,input)`; `enum Status{Ok,Error}`; `Hello{build_id,status,error}` | serde | **LIFT** |
| `serve/files.rs` | 475 | byte-range file/bytes responses | `whole_response(bytes,mime,cache)`, `bytes_response(..,&HeaderMap)` (206/416), `file_response(path,mime,cache,&HeaderMap)->io::Result<Response>` (64 KiB streamed chunks) | axum, tokio::fs | **LIFT** |
| `serve/mod.rs` | 18 | module docs + `pub mod events; mod files; pub mod server; pub mod watch;` | | | **LIFT** |
| `assets/presenter/live-reload.js` | 234 | browser client | `EventSource('/__presenter/events')`; sessionStorage key `presenter:build_id:`; `signature(doc)` (DOM outside `.reveal .slides`); `swapCss(id)` swaps `link#presenter-theme` + `/__presenter/asset/` stylesheets via `swapLink`; `swapContent(id)` fetches `/`, DOMParser, compares signature, `replaceChildren`, `Reveal.sync()`, re-runs `__presenterRenderMath`, highlight init, `restore` indices; `refreshOutside` bumps `?v=`; overlay `#presenter-overlay` with `<pre>` excerpt; `onHello`/`onBuild`; exposes `window.__presenterLiveReload = {connect, disconnect, state}` | — | **ADAPT**: keep SSE/overlay/css-swap/fullReload/signature logic; replace the Reveal-specific `swapContent` with `<main>` replacement + scroll position restore + re-run math/highlight |
| `assets/presenter/math.js` | 28 | KaTeX init | `window.__presenterRenderMath(root)` renders `[data-math-style]` with `katex.render({displayMode, throwOnError:false, ...window.__presenterKatexOptions})`, marks `data-rendered="1"` | KaTeX | **LIFT** (rename globals) |

### 3.6 theme/* and render/*

| path | LOC | purpose | public API (short) | deps | verdict |
|---|---|---|---|---|---|
| `theme/mod.rs` | 1281 | theme chain, file access, problems | codes `THEME_STYLE_ENTRY/SCRIPT_ENTRY/HIGHLIGHT_STYLE/TEMPLATE/PLUGINS/LINK_OUTSIDE`; `REQUIRED_TEMPLATES=["deck.html","slide.html","block.html","cell.html","layouts/default.html"]`; `DEFAULT_REVEAL_THEME="white"`, `DEFAULT_HIGHLIGHT_STYLE="monokai"`; `enum ThemeSource{Embedded{prefix},Disk{dir}}`; `ThemeDir{name,source,manifest}` (`read(rel)->Result<Option<Vec<u8>>>` exact-name + inside-dir symlink rule, `exists`, `list`, `linked_outside`, `location`); `ThemeFile{theme,rel_path,bytes}`; `Theme{chain}`: `name, dir, reveal_theme, highlight_style, reveal_defaults, vars, style_entries, script_entries, highlight_style_file, problems, deck_problems, unknown_layout_message, read_template(name)->Result<Option<(String,String)>>, has_template, has_layout, layouts, read_asset, watch_dirs, environment(debug)`; `recording_reads<R>(f)->(R,BTreeSet<PathBuf>)`; `normalize_rel`, `is_layout_name`, `closest_name`, `deep_merge` | manifest, templates, styles | **ADAPT**: drop `reveal_*`/plugins; `REQUIRED_TEMPLATES` → `["document.html","section.html","block.html","cell.html"]`; keep chain, symlink safety, `recording_reads`, problems |
| `theme/manifest.rs` | 451 | `theme.toml` | `REVEAL_THEMES` (14); `ThemeManifest{name,extends,description,styles:StylesSection{entries,reveal_theme,highlight_style},scripts:ScriptsSection{entries},reveal,layouts:BTreeMap,vars}` (`deny_unknown_fields`); `from_toml_str(text,theme,origin)`; `check_theme_name`; `line_of_value`, `line_of_key` | toml, serde | **ADAPT**: drop `reveal_theme`, `[reveal]`, `REVEAL_THEMES`; add `[print]`/`[typst]`/`[latex]` sections for backend templates |
| `theme/resolve.rs` | 414 | chain resolution | `MAX_CHAIN_LEN=8`, `MANIFEST_FILE="theme.toml"`; `resolve(spec,search_roots:&[&Path])->Result<Theme>`; `builtin_names()`; `locate`; `not_found_hint`; cycle / unique-name / root-template checks | vendor | **LIFT** |
| `theme/templates.rs` | 460 | minijinja env | `environment(theme,debug)->Result<Environment<'static>>` (auto-escape `.html`, `UndefinedBehavior::Lenient`, `set_loader` over the chain with `@<theme>/path` names); `type TemplateFile=(String,String)`; `reached_templates(theme,roots)` (regex on include/extends/import) | minijinja | **LIFT** (watch the minijinja 3.x API if upgrading) |
| `theme/styles.rs` | 1711 | CSS/SCSS entries; grass over a virtual FS; `@import` inlining; isolation | `REVEAL_CSS_DIR`, `SASS_TIME_LIMIT=10s`, `SASS_STACK_SIZE=256 MiB`, `MAX_IMPORT_DEPTH=5`; `enum SourceKind` + `of(path)`; `entry_key(&ThemeFile)`; `compile_entry(&ThemeFile,&Theme)->Result<String>`; `compile_scss(theme,path)`; `VirtualFs<'t>` (`new`, `with_file`, `key`, `read_key`); `reveal_fallback(rel)->Option<String>`; `Inlined{css,imports}`; `inline_imports(theme,key,text,finish)` ; re-exports `recursion::{Recursion,RecursionKind,unconditional_recursion}`, `worker::{WORKER_ARG,WORKER_GRACE,isolate_in_worker,run_worker,with_worker}` | grass, codemap, regex | **ADAPT** (optional, milestone 2+): drop `reveal/` alias + `reveal_fallback`; everything else is generic. If writer ships CSS-only themes first, defer the whole SCSS stack (3 175 LOC incl. recursion+worker) |
| `theme/styles/recursion.rs` | 999 | static SCSS recursion check | `enum RecursionKind`, `Recursion` + `message`, `unconditional_recursion(scss)->Option<Recursion>` | — | **LIFT** with styles.rs |
| `theme/styles/worker.rs` | 465 | Sass worker subprocess protocol | `WORKER_ARG="__presenter-sass-worker"`, `WORKER_GRACE=5s`, `isolate_in_worker(exe)`, `with_worker(exe,f)`, `run_worker()->ExitCode` | serde_json | **LIFT** with styles.rs (rename arg) |
| `render/mod.rs` | 148 | render types | `enum BuildMode{Build,Serve}`; `BuildInfo{mode,standalone,live_reload,id,version}` + `build(standalone)`/`serve(id)`; `RenderInput<'a>{deck,assets,theme,config,output,build}`; `RenderedDocument{html,theme_css,files,slide_count,diagnostics,theme_files}`; re-exports `build_context, render, normalize_helper_path, resolve_helper_path, compile_theme_css` | — | **ADAPT** (`slide_count`→`section_count`; one `RenderedDocument` per backend) |
| `render/context.rs` | 201 | template context | `build_context(&RenderInput,&mut Diagnostics)->Result<minijinja::Value>` (keys `deck, meta, slides, features, config, reveal, theme, build`); `effective_config(&Config,&DeckMeta,&Theme)->Result<Config>` | serde_json | **ADAPT** (keys `document, meta, sections, features, config, theme, build`) |
| `render/html.rs` | 886 | HTML renderer + helpers | `SERVE_HELPER_PREFIX="/__presenter/asset/"`, `SERVE_THEME_CSS`, `SERVE_LIVE_RELOAD_JS`; `serve_stylesheet`; `render(RenderInput)->Result<RenderedDocument>`; `HelperState` (Arc<Mutex>) with helpers `style/script/asset/theme_style/reveal_init/live_reload`; filters `attrs`, `markdown` (notebook-mode `render_fragment`, unwrap single `<p>`), `json`; `inline_script` (`raw_text` `</script`→`<\/script`, data-URI fallback); `check_background_videos` | minijinja | **ADAPT**: drop `reveal_init`, add `math_init` (KaTeX options JSON via `script_safe_json`), `toc()`; keep helper table semantics per mode |
| `render/paths.rs` | 411 | helper path forms | `resolve_helper_path(theme,path)->Result<Option<(Vec<u8>,String)>>`; `normalize_helper_path`; `helper_path_exists`; `enum PathForm{Builtin,Theme{name,rel},Asset}`; `path_form`; `join_relative`; `is_builtin`; `unknown_reason`; `content_version(bytes)->u64` (blake3 first 8 bytes LE); `versioned_serve_url`; `serve_url(key)` (percent-encode except `-._~/@`) | blake3 | **LIFT** (rename `presenter/` builtin segment) |
| `render/styles.rs` | 563 | compiled theme stylesheet + `url()` rewriting | `UrlRules` (`THEME_ENTRY`, `for_style`, `leniently`); `enum Targets{Placeholders(&mut AssetRegistry),ServeUrls}`; `compile_theme_css(theme,registry)->Result<String>`; `compile(theme,targets,remote)`; `decode_text` (one BOM); `rewrite_urls(theme,key,css,rules,targets)`; `replace_placeholder`; `standard_font_mime`; `Lookup::candidates` (next to file → `@<theme>/assets/<rel>` child-first → builtin → reveal fallback); `remote_imports(css)` | emit::register_css_urls | **ADAPT** (remove reveal fallback branch; otherwise lift) |
| `render/reveal.rs` | 265 | reveal plugins/init | `PluginInfo`, `plugins(config,features)`, `reveal_config_json`, `math_enabled(config,features)->bool`, `reveal_init_script`, `script_safe_json` (`<`→`<`) | config | **SKIP** except `math_enabled` + `script_safe_json` (move into `render/html.rs`) |

### 3.7 cli/*

| path | LOC | purpose | public API (short) | deps | verdict |
|---|---|---|---|---|---|
| `cli/mod.rs` | 1447 | clap tree, exit codes, path display | `Cli{global:GlobalArgs{verbose,quiet,config},command}`; `enum Command{Build,Serve,Init,Theme,Dump,Check,Export}` + arg structs; `UsageError(String)`; `EXIT_OK=0`, `EXIT_BUILD_ERROR=1`, `EXIT_USAGE=2`; `run()`, `version()` (`CARGO_PKG_VERSION` + `PRESENTER_GIT_SHA`), `run_from(args)`, `exit_code_for(&anyhow::Error)`, `input_format`, `input_kind`, `locate(err,input)->anyhow::Error`, `display_path(path)->PathBuf` (`relative_to`, `slashed`, `resolve_directories`, `canonical_form`), `clash_error`, `reject_directory`, `print_stdout`, `log_directives`, `init_logging` | clap, anyhow, tracing-subscriber | **ADAPT**: commands `build [--format html,pdf,docx,latex,typst]`, `serve`, `init`, `theme`, `dump`, `check`, `export <fmt>`; keep everything else |
| `cli/report.rs` | 963 | rustc-style error/diagnostic printing | `type SourceCache<'a>=&'a dyn Fn(&Path)->Option<String>`; `report_error(&anyhow::Error)`; `print_error`; `print_error_in`; `format_error(err,input,cache,color)->String` (` --> file:line`, gutter, underline); `read_source`; `report_diagnostics(&Diagnostics,&GlobalArgs)`; `install_panic_hook()`; `stderr_color()` (`NO_COLOR`/`CLICOLOR_FORCE`/`CLICOLOR`/`TERM`/`CI`); `with_display_paths(Error)->Error`; `report_build_failure(BuildFailure,input,global)->anyhow::Error`; `source_order` | anyhow | **LIFT** |
| `cli/build.rs` | 346 | `build` / `build --watch` | `run`, `watch` (via `run_build_watch_with`), `build_options`, `build_once`, `wrote_line`, `format_size(bytes)` (`512 B`, `12.3 kB`, `1.8 MB`) | pipeline, watch | **ADAPT** (multi-format) |
| `cli/serve.rs` | 1131 | `serve` | `run` (tokio multi-thread, `BUILD_STACK_SIZE=8<<20`), `start` (SIGINT first, watcher, first build before bind), `serve_options`, `serve_config`, `bind(host,port)` (wildcard probe, SO_REUSEADDR), `bind_error`, `rebuild_loop`, `merge_batches`, `Session{opts,site_preview,global,state,events,site_files,watcher,set,last_good,versions,listening,restart_note}` (`build/store/watch/note_restart`, `try_build` with `catch_unwind`), `version_file_assets`/`versioned_urls` (`?v=` by mtime+size), `document_versions`, `--site-preview` | tokio, server, watch | **LIFT** (mostly; HTML-only backend in serve) |
| `cli/check.rs` | 270 | `check` text/json | `JsonDiagnostic{level,code,message,path,line,cell}`; exit codes; `--deny-warnings` | check | **LIFT** |
| `cli/dump.rs` | 243 | `dump` deck/context JSON, `--default-config` | `default_config_toml()` (every key commented out; checked by `tests/guide.rs`) | pipeline | **LIFT** |
| `cli/export.rs` | 552 | `export pdf|pptx` | `EXIT_NO_CHROME=3`; `PDF_LIMITATIONS`, `PPTX_LIMITATIONS` help text; `Format{Pdf,Pptx{scale}}`; `export()` flow (find_chrome → build standalone → clash check → `pdf::html_to_pdf(&chrome,html,DEFAULT_TIMEOUT)` / `capture::html_to_pages` + `pptx::write`); `default_output` | pdf, pptx | **ADAPT** (keep the PDF-via-Chrome branch as the "light" PDF backend; drop pptx) |
| `cli/init.rs` | 1057 | `init` scaffold | `run(&InitArgs,&GlobalArgs)->anyhow::Result<ExitCode>`; placeholders `{{name}}`, `{{name_yaml}}`, `{{theme_name}}`, `{{deck}}`, `{{child_theme}}` marker; `--notebook`, `--full-theme`; empty/non-existent dir rule; lexical `..` rule; never overwrites | vendor | **LIFT** (rename placeholders `{{deck}}`→`{{document}}`) |
| `cli/theme.rs` | 1439 | `theme list/eject/new/check` | `run`, `BUILTIN_THEME_README`, `eject_builtin(name,dir)->anyhow::Result<Theme>`, `copied_readme(bytes,builtin,name,made_by)`; `new` writes `theme.toml` + token-comment stylesheet; exit-code rule (2 = argument names nothing usable, 1 = what it names is broken) | theme | **ADAPT** (drop reveal token listing in `new`; otherwise lift) |

### 3.8 pdf/* and pptx.rs

| path | LOC | purpose | public API (short) | deps | verdict |
|---|---|---|---|---|---|
| `pdf.rs` | 2496 | headless Chrome over DevTools pipe; find Chrome; print to PDF; temp dir + janitor | `DEFAULT_TIMEOUT=120s`, `CLOSE_GRACE=5s`, `SERVED_DECK_URL`, `PATH_NAMES`, `CHROME_FLAGS` (`--headless=new --disable-gpu … --remote-debugging-pipe`), `LAUNCH_SCRIPT` (`exec "$0" "$@" 3<&0 4>&1 …`), `JANITOR_SCRIPT`, `READY_SCRIPT` (reveal-specific); `find_chrome(explicit:Option<&Path>)->Option<PathBuf>` (`--chrome` → `$CHROME_PATH` → macOS bundles Chrome/Chromium/Edge/Brave/Arc → PATH names); `html_to_pdf(chrome,html,timeout)->Result<Vec<u8>>` (steps: TempDir 0700, `Target.createTarget`, attach flat, `Page.navigate file://…`, wait load, evaluate READY_SCRIPT, `Page.printToPDF{preferCSSPageSize,printBackground,margins 0,ReturnAsStream}`, `IO.read`, `Browser.close`); `page_count(pdf)->usize` (tokenizer skipping strings/streams/comments); internals `Cdp{call,send,next_message,handle_event,serve,dismiss_dialog}`, `Browser{launch,stop,stderr_tail}`, `TempDir`, `Janitor`, snap handling (`placement`, `snap_name`, served-deck fallback via `Fetch.enable`) | base64, serde_json | **ADAPT**: generic Chrome driver is ~1 900 LOC of reusable code; replace `READY_SCRIPT` with "fonts.ready + KaTeX done + 2 rAF" and the `?print-pdf` URL suffix; keep janitor/snap/served fallback. This is the "light" PDF backend (HTML → Chrome). Unix-only (`/bin/sh`). |
| `pdf/devtools.rs` | 580 | test-only `Page` (feature `browser-tests`) | `Page::{launch(chrome,timeout),set_timeout,navigate(url),evaluate(expr)->Result<Value>,errors()}`; `readable_temp_dir(chrome)->Option<PathBuf>`; `LoadWait` (loaderId tracking), `Lifecycle` | pdf internals | **LIFT** (optional; needed for browser smoke tests of live reload) |
| `pdf/capture.rs` | 773 (+ `scene.js` 1418) | pages → PPTX shapes | `html_to_pages(chrome,html,timeout,scale)->Result<Pages>`; `PAGES_SCRIPT`, `SCENE_SCRIPT` | pdf, pptx | **SKIP** |
| `pptx.rs` | 2701 | OOXML `.pptx` writer without a library | model (`Presentation`, `Slide`, `Shape`, …) + `write(&Presentation)->Result<Vec<u8>>`; generic helpers listed in §7 | base64, blake3, serde | **ADAPT → seed `docx.rs`**: lift `Zip`, `crc32`, `escape`, `clean`, `is_xml_char`, `single_line`, `cut`, `Media` dedup, relationship-id scheme, `[Content_Types].xml`/`_rels/.rels`/`docProps/core.xml`/`app.xml` writers, `Image{dimensions}`+`jpeg_dimensions`, `image_from_data_url`, EMU constants; skip DrawingML shapes |

---

## 4. Rules writer must keep identical

### 4.1 comrak options (dialect §1, verified against `markdown/mod.rs::comrak_options`)

```rust
let mut o = comrak::Options::default();
o.extension.front_matter_delimiter = Some("---".into());
o.extension.table = true;
o.extension.strikethrough = true;
o.extension.tasklist = true;
o.extension.autolink = true;
o.extension.footnotes = true;
o.extension.description_lists = true;
o.extension.alerts = true;                 // > [!NOTE] callouts
o.extension.math_dollars = true;           // $x$ and $$x$$
o.extension.math_code = true;              // ```math fences
o.extension.block_directive = true;        // :::name … :::
o.extension.header_attributes = true;      // ## Title {#id .cls k=v}
o.extension.fenced_code_attributes = true; // ```python {lines="1-3"}
o.extension.link_attributes = true;        // ![](x.png){width=50%}
o.extension.inline_code_attributes = true;
o.parse.smart = config.markdown.smart_punctuation;   // default false
o.parse.relaxed_autolinks = false;
o.render.r#unsafe = true;                  // raw HTML passes through
o.render.escape = false;
o.render.hardbreaks = false;
o.render.github_pre_lang = false;          // <code class="language-x"> emitted by code.rs
o.render.full_info_string = false;
o.render.sourcepos = config.markdown.sourcepos;      // default false
// Cargo: comrak = { version = "0.56", default-features = false, features = ["attributes"] }
```

### 4.2 Attribute grammar (dialect §3.1 + A3/W8FIX notes, verified)

- Pandoc form `{#id .class key=value key2="quoted" key3='single'}`; unquoted values run to
  whitespace. Braces are optional for `parse_attr_block`.
- comrak 0.56 rejects a whole block on headings/fences/links/images/inline code when a value is
  single-quoted or an unquoted value has characters outside letters, digits, `- _ . : %`
  (`background=#222`, `lines=1|3`), leaving `{…}` as text; `recover.rs` re-reads it with
  `parse_attr_block`'s tokenizer, rewrites every value double-quoted, re-parses (≤3 rounds) and
  verifies comrak now reads exactly that.
- Every item must be `#id`, `.class` or `key=value` with `is_valid_attr_name(key)` (ASCII letter
  then `[A-Za-z0-9-_:.]`); a bare key needs `=""` outside separator forms; no unquoted comma at the
  end of an item; no `{`/`}` outside quotes (`{.a}{k='v'}` = one invalid block); `{1, 2}` stays
  text; an invalid block stays text with an `unparsed-attr-block` warning; a block crossing a
  table cell / emphasis stays text; of two trailing blocks only the last is read.
- On links/images, `title=` applies only when there is no Markdown title.
- Directive info: split on whitespace; first token = name; remaining tokens are classes unless
  they contain `=`; pandoc `::: {.name #id k=v}` accepted (first class is the name); unconsumed
  args become `data-<key>`; nesting needs a **longer outer fence**; no `:` after the fence (use
  `&#58;`); a bare `:::` opener or an unopened `:::name` line is an `invalid-directive` warning.
- Front matter: `---` on line 1, closed by `---`; `serde_norway` (YAML 1.2 core: `yes/no` are
  strings); duplicate keys are errors; string fields keep scalar text; empty/unclosed front matter
  warns (`front-matter-empty`, `front-matter-unclosed`).

### 4.3 Asset placeholders (ir.md §3, verified in registry/emit)

`asset:<12 hex>` where the id is the first 12 hex chars of blake3 over the absolute path
(`register_file`) or the bytes (`register_bytes`). The emitter rewrites only attribute values and
`<style>`/`<script>` text (never visible text, so a literal placeholder in prose survives). Site
file names are `hashed_file_name(hint, id)` = `<stem>-<id>.<ext>`. Standalone inlines `data:` URIs;
Serve uses `/__presenter/deck-asset/<id>` (+ `?v=` by mtime+size in `cli/serve.rs`).

### 4.4 Live reload protocol (live-reload.md, verified in server/events/live-reload.js)

SSE at `/__presenter/events`: first event `hello` = `Hello{build_id,status,error}`; then one
`build` event per rebuild = `BuildEvent::Ok{build_id,kind:css|content,duration_ms}` or
`Error{build_id,error:ErrorPayload{message,file,line,kind,excerpt}}`. The client stores the last
build id in sessionStorage, swaps stylesheets in place for `css`, refetches `/` and replaces
content for `content` (full reload when the non-content "signature" changed), shows an overlay for
errors. Watcher: 150 ms quiet / 2 s max batch; `ChangeKind::Css` iff every path is a `.css/.scss/
.sass` under a theme dir. `AllowedHosts` refuses requests whose `Host` is not the bound address
(DNS rebinding guard).

### 4.5 CLI conventions (cli.md, verified in cli/*)

Exit codes 0 / 1 (build error) / 2 (usage, config) / 3 (no Chrome). `--version` prints
`presenter <ver> (<git id>)`. Paths in messages are relative to CWD, `/`-separated
(`display_path`). Errors: `error: <message>` then ` --> file:line` + excerpt + caret.
`-v/-vv/-q`, `RUST_LOG` adds to verbosity. `dump --default-config` output is contract-tested
against `docs/guide/configuration.md`.

---

## 5. Theme system facts (theme-format.md, verified in theme/*)

- `theme.toml`: `name`, `extends`, `description`, `[styles] entries = [...]`, `reveal_theme`,
  `highlight_style`, `[scripts] entries`, `[reveal]`, `[layouts] name = "description"`, `[vars]`.
  `deny_unknown_fields`; names must be valid in `@<name>/` template paths; chain ≤ 8; cycles and
  duplicate names rejected; root must have `REQUIRED_TEMPLATES`.
- `Theme::resolve(spec, search_roots)`: built-in name (embedded `themes/<name>/`) or a directory
  relative to each search root (config dir, then CWD); `extends` resolves relative to the child.
- Files are read by exact name; symlinks that leave the theme dir are refused (`LINK_OUTSIDE`).
- Templates: minijinja `Environment` with a chain loader; `@<theme>/<path>` names a specific
  theme's file; helpers `style(path)`, `script(path)`, `asset(path)`, `theme_style()`,
  `live_reload()`; filters `attrs`, `markdown`, `json`. Helper paths: `vendor/…` and `presenter/…`
  (embedded), `@<theme>/…`, or `assets/…` (chain child-first).
- `[styles] entries` are concatenated parent-first into one stylesheet (each after
  `/* @<theme>/<path> */`), `.scss/.sass` compiled by grass in a worker process (10 s / 256 MiB
  stack), local `@import`s inlined, `url()`s resolved and registered as assets.
- Built-in default theme: 4 layouts (`default`, `title`, `section`, `blank`), tokens `--p-*`
  documented in its README and contract-tested (`tests/theme_tokens.rs`).

---

## 6. Diagnostics / Error usage pattern (verified)

Frontends push warnings via `diagnostics.warn(CODE, msg).with_path(p).with_span(span)`; hard
failures return `Err(Error::…)`. `Error::span()` parses `cell N, line M:` and `at line L column C`
prefixes out of messages so notebook/YAML errors locate themselves. `cli::report::format_error`
renders with an excerpt from a `SourceCache` closure. `check` turns an `Error` into a
`Diagnostic` with `error_diagnostic`. `serve` turns both into `ErrorPayload` for the overlay.
Codes are `&'static str` kebab-case constants in the module that emits them.

---

## 7. pptx.rs helpers that seed a docx writer (verified, with line numbers in `src/pptx.rs`)

| helper | lines | signature | generic? |
|---|---|---|---|
| `Zip` | 1901–1983 | `struct Zip{bytes,directory,entries:u16}`; `fn add(&mut self,name:&str,data:&[u8])->Result<()>`; `fn finish(self)->Result<Vec<u8>>` — stored entries, `DOS_DATE=(1<<5)|1` (1980-01-01), `DOS_TIME=0`, no ZIP64, errors at 4 GiB / 65 535 entries | yes (docx is the same container; keep "[Content_Types].xml first") |
| `crc32` / `crc32_table` | 1985–2014 | `fn crc32(data:&[u8])->u32` (const table, poly 0xEDB88320) | yes |
| `escape` | 1860–1872 | `fn escape(text:&str)->String` (`& < > "`; drops non-XML chars via `clean`) | yes |
| `clean` / `is_xml_char` | 1874–1883 | XML 1.0 `Char` production filter | yes |
| `single_line` / `cut` | 1885–1897 | whitespace collapse; char-count truncation with `…` | yes |
| `Media` | 920–949 | `number_of(&Image)->usize` dedup by blake3 → `media/imageN.<ext>` | yes (`word/media/`) |
| `Image`, `ImageKind`, `jpeg_dimensions`, `image_from_data_url` | 523–653 | PNG/GIF/JPEG header size readers; `data:` URL decoder | yes (docx needs pixel sizes for EMU extents) |
| `EMU_PER_INCH=914_400`, `EMU_PER_POINT=12_700` | 42–45 | | yes |
| `content_types`, `package_rels`, `core_properties`, `app_properties` | 1002–1064 | `[Content_Types].xml` with `Default`/`Override`; `_rels/.rels` (officeDocument, core, app); `docProps/core.xml` title+creator, **no dates** (deterministic); `docProps/app.xml` | yes (swap part names to `word/document.xml`, content type `…wordprocessingml.document.main+xml`) |
| relationship scheme | 1185–1233, 1801–1829 | per-part `rels: Vec<(id, type, target)>`, `rId<n>` allocation, `TargetMode="External"` for hyperlinks, dedup of hyperlink targets via `HashMap` | yes |
| `XML_HEAD`, `NS_R`, `NS_RELS`, `REL_IMAGE`, `REL_HYPERLINK`, `CT_CORE`, `CT_APP`, `CT_RELS` | 953–994 | | yes |
| `color_xml`, `gradient_xml`, `blip_fill`, `place_background`, slide/notes XML, `Scale` | rest | DrawingML | no |
| determinism test `two_writes_give_the_same_bytes` + ZIP read-back helper `entries()` | 2029–2063, 2688 | test-side ZIP parser (central directory walk) | yes (reuse in docx tests) |

---

## 8. Test infrastructure (verified)

- Unit tests live inline in each module (`#[cfg(test)] mod tests`).
- 28 integration test files (28 402 LOC); fixtures under `tests/fixtures/{md (13 files + images/),
  ipynb (5), ir/sample-deck.json, include/, config/, themes/, live-reload/{mock-server.py,qa.mjs}}`;
  rule: no `presenter.toml` above `tests/fixtures/md|ipynb` (checked by `config.rs`).
- insta: `insta::assert_json_snapshot!(name, json, { ".source.path" => "[path]",
  ".source.base_dir" => "[path]" })` with asset ids relabelled to `000000000001…` in first-use
  order (`relabel`); snapshots in `tests/snapshots/<testfile>__<name>.snap` (40 files:
  `markdown_snapshots__<fixture>.snap`, `notebook_snapshots__<fixture>[_diagnostics].snap`,
  `render__slides_{standalone,site,serve}.snap`, `diagnostics__*.snap`, `cli_build__example_slides_region.snap`).
  Every fixture test also asserts `invariant_violations() == []` and that every diagnostic has a
  path + span. `*.snap.new` is gitignored.
- assert_cmd pattern: `Command::new(env!("CARGO_BIN_EXE_presenter")).current_dir(root())
  .env_remove("RUST_LOG").env_remove("NO_COLOR").env_remove("CLICOLOR_FORCE")`; helper
  `project(&[(path, contents)]) -> tempfile::TempDir`; `predicates::str::contains`.
- Server tests (`tests/server.rs`, `tests/cli_serve.rs`) bind `127.0.0.1:0`, talk HTTP via `ureq`,
  read SSE streams line by line, run the binary as a child and parse the URL from stdout line 1.
- Watcher tests run against the real OS backend with generous timeouts (`BATCH_TIMEOUT=5s`).
- `tests/browser.rs` is `#![cfg(all(feature = "browser-tests", unix))]`; uses
  `presenter::pdf::devtools::Page`; skips with `skipping: no chrome found` unless
  `PRESENTER_REQUIRE_CHROME` is set (CI sets it).
- `tests/guide.rs` keeps docs honest: every ```` ```text ```` block starting `$ presenter … --help`
  must equal the real help at 100 columns; the ```` ```toml ```` block after
  `<!-- generated: presenter dump --default-config -->` must equal `dump --default-config`; the
  per-table key tables must list every config key; every relative link/anchor in `README.md` and
  `docs/guide/*.md` must resolve. `PRESENTER_UPDATE_GUIDE=1 cargo test --test guide` regenerates.
- `tests/cli_version.rs` has the `VERSION_LINE` regex that release.md says to bump.
- `tests/vendor_smoke.rs`: embedded file list matches `vendored-assets.md`, every package ships a
  LICENSE, `vendor_dir_is_below_8_mb`.
- `tests/theme_tokens.rs`: README token table ⇔ CSS custom properties; stylesheet loads nothing
  from the network; a hand-written CSS rule parser checks print rules.
- Test file → coverage summary:

| file | LOC | #tests | covers |
|---|---|---|---|
| assets.rs | 917 | 18 | registry + emitter in all modes, CSS url() registration, site determinism |
| browser.rs | 808 | 7 | Chrome smoke: example runs, live reload swaps CSS, reloads on config change |
| build_script.rs | 447 | 14 | build.rs id logic on scratch repos |
| cli_build.rs | 1666 | 38 | build/dump end-to-end, exit codes, clash refusals, site mode |
| cli_check.rs | 995 | 24 | check lints, json format, notebook lints, theme problems |
| cli_export.rs | 1683 | 33 | export pdf/pptx with real + fake browsers, cleanup, exit 3 |
| cli_init.rs | 809 | 16 | scaffold builds, placeholders, refusal rules |
| cli_serve.rs | 1040 | 16 | serve binary: rebuilds, SSE, host check, Ctrl-C, SCSS runaway |
| cli_theme.rs | 870 | 20 | theme list/eject/new/check |
| cli_version.rs | 20 | 1 | `--version` |
| config.rs | 781 | 33 | defaults, hints, discovery, precedence |
| diagnostics.rs | 392 | 17 | error report snapshots, NO_COLOR, panic hook |
| guide.rs | 550 | 6 | docs vs binary |
| include.rs | 954 | 21 | `:::include` composition and misuse errors |
| ir_roundtrip.rs | 445 | 14 | IR JSON contract |
| markdown_snapshots.rs | 1117 | 27 | every md fixture + front matter/dialect edge cases |
| nbformat.rs | 852 | 24 | nbformat model leniency |
| notebook_snapshots.rs | 2279 | 45 | notebook → deck |
| notebook_theme.rs | 320 | 5 | executed example notebook builds |
| outputs.rs | 1037 | 26 | MIME outputs, ANSI, SVG, LaTeX |
| render.rs | 1625 | 31 | renderer in 3 modes, helpers, katex inclusion |
| server.rs | 1028 | 14 | axum routes, SSE, byte ranges, shutdown |
| styles.rs | 1315 | 24 | SCSS compile, worker, recursion, imports |
| theme_resolve.rs | 1361 | 33 | chain resolution contract |
| theme_skeleton.rs | 1826 | 25 | default theme templates render the sample deck |
| theme_tokens.rs | 1634 | 36 | theme README ⇔ CSS |
| vendor_smoke.rs | 501 | 17 | embedded assets |
| watch.rs | 1130 | 21 | watcher, watch sets, build --watch |

---

## 9. CI, release and site (verified)

### ci.yml
- `check` job, matrix `ubuntu-latest / macos-latest / windows-latest`: `actions/checkout@v7`,
  `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`; `cargo fmt --check`;
  `cargo clippy --all-targets --all-features -- -D warnings`; `node --check
  assets/presenter/live-reload.js` + size `< 10240`; plants junk files under `assets/` (so the
  embed excludes are exercised); `cargo test --no-fail-fast`; `cargo test --release --no-fail-fast`
  (release = embedded assets).
- `browser` job (ubuntu): `PRESENTER_REQUIRE_CHROME=1`, `cargo test --features browser-tests --lib
  pdf::devtools`, `cargo test --features browser-tests --test browser -- --nocapture`.
- `vendor-drift` job: `scripts/vendor.sh` then `git diff --exit-code`.

### release.yml
- Triggers: tag `v*` and `workflow_dispatch`. `label` job: tag must equal `v<Cargo.toml version>`
  and be annotated. `build` matrix (6): `aarch64-apple-darwin/macos-15`,
  `x86_64-apple-darwin/macos-15-intel`, `x86_64-unknown-linux-gnu/ubuntu-22.04`,
  `aarch64-unknown-linux-gnu/ubuntu-22.04-arm`, `x86_64-pc-windows-msvc/windows-2025` (zip),
  `aarch64-pc-windows-msvc/windows-11-arm` (zip). Steps: `cargo test --release --locked
  --no-fail-fast`, `cargo build --release --locked`, smoke (`--version` == `presenter <v> (<tag>)`,
  build `examples/basic`), third-party licenses (`cargo tree -e normal,no-proc-macro` + `cargo
  metadata` + jq → `THIRD-PARTY-LICENSES/{assets/, crates.txt, crates/<name-ver>/}`; MIT text
  written for crates without a license file), package `presenter-<label>-<target>.tar.gz` (owner
  root:0, no xattrs, `COPYFILE_DISABLE`) or `.zip` (python), `upload-artifact@v7`.
- `release` job: `download-artifact@v8`, expects 6 archives, `SHA256SUMS`, notes = the CHANGELOG
  `## [<version>]` section, `gh release create --verify-tag` (`--prerelease` when the tag has `-`),
  then `gh workflow run site.yml --ref main`.
- Archive contents: binary, `LICENSE`, `README.md`, `THIRD-PARTY-LICENSES/`. No signing or
  notarization.

### site.yml
Build release, `scripts/site.py --presenter target/release/presenter --out dist/site …`
(renders guide pages + example decks), `configure-pages@v6`, `upload-pages-artifact@v5`,
`deploy-pages@v5`.

### docs/dev/release.md checklist
1. bump `Cargo.toml` version + `cargo check` (Cargo.lock); 2. update the `VERSION_LINE` regex in
`tests/cli_version.rs` and the `version_string` unit test in `src/cli/mod.rs`; 3. CHANGELOG:
rename `## [Unreleased]` → `## [x.y.z] - YYYY-MM-DD` (Keep a Changelog 1.1.0, SemVer); 4.
`git tag -a vX.Y.Z -m …`; 5. push the tag. Measurements recorded: binary 10.96 MB with the
release profile, ~4.8 MB embedded assets, offline build verified. Design choices: hand-written
workflow (not cargo-dist), native runners per target, Ubuntu 22.04 glibc floor, static CRT on
Windows.

### docs/guide structure
`README.md` (index) → `getting-started.md` (195: install, create, preview, build, export,
check), `authoring.md` (561: dialect walkthrough), `cli.md` (521: generated `--help` blocks per
command), `configuration.md` (254: generated default config + per-table key tables),
`notebooks.md` (201), `themes.md` (346). Examples: `basic`, `notebook`, `mixed`, `headings-mode`,
`custom-theme-scss` (+ a private `eit`). Specs under `docs/spec/` (8 files, 1 852 LOC) are the
contracts tests cite by section; `docs/tasks/README.md` is the task board (T00–T49 with waves and
a PR template); `docs/dev/live-reload-qa.md` is the manual QA script.

---

## 10. Lift plan — milestone 1 (scaffold, IR/config/error, markdown frontend, assets, serve/watch/live-reload, notebook)

Ordered so each step compiles on its own. "verbatim" = copy + `sed s/presenter/writer/`
identifiers; "adapt" = copy + the listed change. LOC are source LOC to copy (tests included
where inline).

| # | source | writer path | mode | adaptation | LOC |
|---|---|---|---|---|---|
| 1 | `Cargo.toml`, `clippy.toml`, `rustfmt.toml`, `.cargo/config.toml` (targets only), `.gitattributes`, `.gitignore` | same | adapt | name `writer`; drop `lol_html`, `grass`, `codemap` until SCSS is wanted; keep `browser-tests` feature; add `zip`? (see Q1) | ~90 |
| 2 | `build.rs` | `build.rs` | adapt | env vars `WRITER_GIT_SHA`, `WRITER_BUILD_ID_SOURCE` | 305 |
| 3 | `src/lib.rs`, `src/main.rs` | same | adapt | module list; drop Sass worker dispatch from `main.rs` for now | 51 |
| 4 | `src/error.rs` | `src/error.rs` | verbatim | — | 1344 |
| 5 | `src/ir.rs` | `src/ir.rs` | rewrite | keep `Features/SourceInfo/SourceKind/SourceSpan/AssetInfo/Parsed/NotebookCell/CellOutput`; new `Document{meta:DocMeta,blocks:Vec<Block>,features,source,assets,includes}`, `DocMeta` (= `DeckMeta` minus `reveal`, plus `toc`, `numbering`, `bibliography?`), `Block{Heading{level,id,classes,attrs,html,span},Html{html,span},Cell(NotebookCell),Figure{…},Callout{kind,title,blocks},Include?,PageBreak}`; keep `invariant_violations` | ~400 of 587 |
| 6 | `src/config.rs` | `src/config.rs` | adapt | `[document]` (was `[deck]`), drop `[slides]`/`[reveal]`/`PluginId`; add `[outputs] formats = ["html"]`, `[html]`, `[pdf] engine = "chrome"|"typst"`, `[docx]`, `[latex] template`, `[typst] template`; keep discovery + hints + `merge_front_matter` | ~600 of 762 |
| 7 | `src/vendor.rs` + `assets/` skeleton | same | adapt | includes `vendor/**`, `writer/**`, `themes/**`, `scaffold/**`; vendor only KaTeX + highlight.js at first | 152 |
| 8 | `scripts/vendor.sh` | same | adapt | pins for KaTeX 0.19.0 + highlight.js 11.11.1 (standalone tarball) | ~250 |
| 9 | `src/assets/{mod,registry,emit}.rs` | same | verbatim | prefix `/__writer/doc-asset/`; depends on `markdown::render::{html_attr_values,html_start_tags}` (step 12) | 2325 |
| 10 | `src/markdown/attrs.rs` | `src/markdown/attrs.rs` | adapt | delete `SlideAttrs`, `normalize_key`, `DATA_SHORTHANDS`, `CSS_COLOR_KEYWORDS` (~500 LOC); keep tokenizer + comrak differential tests | ~700 of 1231 |
| 11 | `src/markdown/{recover,frontmatter,code}.rs` | same | verbatim / adapt | `recover` verbatim; `frontmatter`: `TABLES = ["markdown","math","notebook","html","pdf","docx","latex","typst"]`, `STRING_FIELDS` per `DocMeta`; `code`: replace reveal `data-*` with writer's attrs | 2977 |
| 12 | `src/markdown/render.rs` | `src/markdown/render.rs` | adapt | drop `RenderMode::Deck` notes/fragment/columns handling and `rewrite_slide_assets`; directive table → `figure/callout/note/tip/warning/caution/table/include/pagebreak`; footnotes stay document-wide; keep HTML scanner + asset rewriting + math + element attrs | ~2300 of 2908 |
| 13 | `src/markdown/split.rs` (helpers only) | `src/markdown/headings.rs` | lift subset | `source_lines`, `first_heading_text`, `lift_heading_attrs`, `push_plain_text`; new: heading → section/toc builder | ~200 of 3041 |
| 14 | `src/markdown/include.rs` | same | adapt | include allowed as any top-level block; `Expanded.slides` → `blocks`; keep depth/cycle/`scan_includes`/diagnostic tagging | ~900 of 1106 |
| 15 | `src/markdown/mod.rs` | same | adapt | `comrak_options` verbatim; replace `build_slide` loop with "render whole AST → blocks" | ~600 of 885 |
| 16 | `src/notebook/{nbformat,outputs}.rs` | same | verbatim | metadata key `writer`; keep `SlideType` parsing (harmless) or drop | 2375 |
| 17 | `src/notebook/{mod,convert}.rs` | same | adapt | emit blocks in cell order, headings open sections; drop `EMPTY_SLIDE/SINGLE_SLIDE/*_SLIDE_ATTR` codes; keep attachments + metadata layering | ~1800 of 2676 |
| 18 | `src/theme/{resolve,templates,manifest,mod}.rs` | same | adapt | manifest without `reveal_theme`/`[reveal]`/`REVEAL_THEMES`; `REQUIRED_TEMPLATES = ["document.html","section.html","block.html","cell.html"]`; drop `reveal_defaults`, plugin checks | ~2300 of 2606 |
| 19 | `src/render/{mod,context,html,paths,styles}.rs` | same | adapt | drop `reveal_init`, add `math_init` (uses `script_safe_json` + `math_enabled` from `render/reveal.rs`); context keys `document/sections/meta/features/config/theme/build`; `styles.rs` minus reveal fallback | ~2000 of 2209 |
| 20 | `assets/themes/default/**` | same | rewrite | document templates (`document.html`, `section.html`, `block.html`, `cell.html`, partials), print CSS (`@page`, page breaks), tokens `--w-*` | new (~600) |
| 21 | `src/pipeline.rs` | same | adapt | `Build` carries `outputs: Vec<(Format, RenderedOutput)>`; `write_output` per format; keep `load_config`, `detect_kind`, clash logic verbatim | ~1700 of 2025 |
| 22 | `src/check.rs` | same | adapt | drop separator lints (`opened_lines`, `unrecognized_separators`, `why_not_a_separator`, `skipped_ranges` ≈ 700 LOC); keep the rest | ~1200 of 1955 |
| 23 | `src/cli/{mod,report,build,check,dump,init}.rs` | same | adapt / verbatim | `report`, `check`, `dump`, `init` verbatim; `mod` command tree; `build --format` | ~4100 of 4326 |
| 24 | `src/serve/{mod,events,files,server,watch}.rs` + `src/cli/serve.rs` | same | verbatim | routes `/__writer/…`; fallback page text | 5288 |
| 25 | `assets/presenter/{live-reload,math}.js` | `assets/writer/` | adapt | `swapContent` replaces `<main>` (or `[data-writer-content]`) and restores `scrollY`; keep CSS swap/overlay/hello; rename globals `__writer*` | 262 |
| 26 | `assets/scaffold/**` | same | adapt | `writer.toml`, `document.md`, `document.ipynb`, child theme | ~150 |
| 27 | tests: `error`-related unit tests come with the modules; copy `tests/{assets,config,diagnostics,ir_roundtrip,markdown_snapshots,nbformat,notebook_snapshots,outputs,include,server,watch,cli_serve,cli_build,cli_check,cli_init,cli_version,guide,vendor_smoke,theme_resolve,build_script}.rs` and fixtures | `tests/` | adapt | snapshots regenerated (`cargo insta`); slide-shape assertions removed; fixture `md/` files rewritten for documents (keep `attributes.md`, `attribute-recovery.md`, `code.md`, `math.md`, `images.md`, `directives.md`, `pandoc-directives.md`, `minimal.md`) | ~15 000 |
| 28 | `.github/workflows/{ci,release,site}.yml`, `docs/dev/release.md` | same | adapt | names/paths; keep job structure and license-collection step | ~700 |

Milestone-1 total to touch: ≈ 32 k source LOC, of which ≈ 13.5 k verbatim (error, registry, emit,
recover, nbformat, outputs, events, files, server, watch, cli/serve, report, check, dump, init,
resolve, templates, paths, vendor, build.rs).

Deferred (milestone 2+): `pdf.rs` + `pdf/devtools.rs` (Chrome PDF backend and browser tests,
≈ 3 000 LOC, adapt READY_SCRIPT), `theme/styles*` (SCSS, 3 175 LOC), `cli/theme.rs` (1 439),
`cli/export.rs` (552), `pptx.rs` helpers → `docx.rs` (≈ 350 LOC generic), `render/reveal.rs`
`script_safe_json`/`math_enabled` (≈ 40 LOC into html.rs).

---

## 11. Verified API hints (exact text seen in source)

```rust
// src/error.rs
pub enum Error { Parse { path: PathBuf, line: usize, message: String }, FrontMatter(String), Config(String),
  Theme { name: String, message: String }, Template { template: String, line: Option<usize>, message: String },
  Style { file: String, message: String }, MissingAsset { path: PathBuf, span: Option<SourceSpan> },
  Io(#[from] std::io::Error), Json(#[from] serde_json::Error), Other(String) }
pub type Result<T> = std::result::Result<T, Error>;
pub const EXCERPT_WIDTH: usize = 120;
pub fn template_error(err: minijinja::Error, debug: bool) -> Error;
pub struct Diagnostic { pub level: Level, pub code: &'static str, pub message: String, pub path: Option<PathBuf>, pub span: Option<SourceSpan> }
pub struct Diagnostics(Vec<Diagnostic>);   // push, warn(code, msg) -> &mut Diagnostic, error, iter, has_errors, len, is_empty, into_vec

// src/ir.rs
pub struct SourceSpan { pub start_line: usize, pub end_line: usize, pub cell: Option<usize>,
  #[serde(default, skip_serializing_if = "Option::is_none")] pub file: Option<PathBuf> }
pub struct Features { pub math: bool, pub code: bool, pub notes: bool, pub notebook: bool }  // + fn merge(&mut self, other: &Features)
pub struct Parsed { pub deck: Deck, pub assets: AssetRegistry, pub diagnostics: Diagnostics }

// src/assets/registry.rs
pub const ID_LEN: usize = 12;
pub fn register_file(&mut self, path: &Path) -> String;
pub fn register_bytes(&mut self, bytes: Vec<u8>, mime: &str, file_name_hint: &str) -> String;
pub fn placeholder(id: &str) -> String;           // "asset:<id>"
pub fn content_hash(bytes: &[u8]) -> String;      // blake3, first ID_LEN hex chars

// src/assets/emit.rs
pub enum OutputMode { Standalone, Site { assets_dir: PathBuf, url_prefix: String }, Serve }
pub fn resolve_html(&mut self, html: &str) -> Result<String>;
pub fn register_css_urls(css: &str, registry: &mut AssetRegistry, resolve_rel: &mut dyn FnMut(&str) -> Option<AssetData>, lenient: bool) -> Result<String>;
pub fn hashed_file_name(hint: &str, id: &str) -> String;

// src/markdown/mod.rs
pub fn parse(source: &SourceInfo, text: &str, config: &Config) -> Result<Parsed>;
pub fn comrak_options(config: &MarkdownConfig) -> comrak::Options<'static>;

// src/markdown/attrs.rs
pub fn parse_attr_block(text: &str) -> Result<ParsedAttrs>;
pub fn read_element_attrs(block: &str) -> ElementAttrs;
pub fn is_valid_attr_name(key: &str) -> bool;

// src/markdown/recover.rs
pub const UNPARSED_ATTR_BLOCK: &str = "unparsed-attr-block";
pub fn recover_attr_blocks<'a, 't>(arena: &'a Arena<AstNode<'a>>, root: &'a AstNode<'a>, text: &'t str, opts: &Options, diags: &mut Diagnostics) -> (&'a AstNode<'a>, Cow<'t, str>);

// src/markdown/render.rs
pub struct RenderCtx<'r> { pub assets: &'r mut AssetRegistry, pub base_dir: &'r Path, pub mode: RenderMode, pub diagnostics: &'r mut Diagnostics, pub missing_asset_is_error: bool }
pub fn render_fragment_with_lift(text: &str, opts: &Options, ctx: &mut RenderCtx, lift: bool) -> Result<(Rendered, Option<ParsedAttrs>)>;
pub(crate) fn html_start_tags(html: &str) -> Vec<(String, usize)>;
pub(crate) fn html_attr_values(html: &str) -> Vec<(String, std::ops::Range<usize>)>;

// src/markdown/include.rs
pub const MAX_DEPTH: usize = 8;
pub fn scan_includes(path: &Path) -> Vec<PathBuf>;

// src/notebook/nbformat.rs
pub fn parse_notebook(path: &Path, json: &str) -> Result<(Notebook, Diagnostics)>;
pub fn parse_outputs(raw: Vec<Value>) -> (Vec<Output>, Vec<String>);
// src/notebook/outputs.rs
pub fn render_outputs(outputs: &[Output], config: &NotebookConfig, assets: &mut AssetRegistry, diagnostics: &mut Diagnostics, markdown: &mut dyn FnMut(&str) -> Result<String>) -> Result<(Vec<CellOutput>, Features)>;
pub fn ansi_to_html(text: &str) -> String;

// src/serve/watch.rs
pub const DEBOUNCE: Duration = Duration::from_millis(150);
pub enum ChangeKind { Css, Content }
pub fn classify(paths: &[PathBuf], theme_dirs: &[PathBuf]) -> ChangeKind;
impl Watcher { pub fn start(set: &WatchSet, tx: Sender<ChangeBatch>) -> Result<Watcher>; pub fn update(&mut self, set: &WatchSet) -> Result<()>; }
pub fn run_build_watch_with(opts: &BuildOptions, out: &Path, report: impl FnMut(WatchStatus<'_>) -> ControlFlow<()>) -> Result<()>;

// src/serve/server.rs
pub type Shared = Arc<RwLock<ServeState>>;
pub fn router(state: Shared, events: broadcast::Sender<BuildEvent>) -> axum::Router;
pub fn serve_until(listener: TcpListener, router: Router, shutdown: oneshot::Receiver<()>) -> Result<()>;  // tokio::net::TcpListener; async
// src/serve/events.rs
#[serde(tag = "status", rename_all = "lowercase")] pub enum BuildEvent { Ok { build_id: u64, kind: ChangeKind, duration_ms: u64 }, Error { build_id: u64, error: ErrorPayload } }
pub struct ErrorPayload { pub message: String, pub file: Option<String>, pub line: Option<usize>, pub kind: String, pub excerpt: Option<String> }

// src/theme/resolve.rs
pub fn resolve(spec: &str, search_roots: &[&Path]) -> Result<Theme>;
pub const MAX_CHAIN_LEN: usize = 8;  pub const MANIFEST_FILE: &str = "theme.toml";
// src/theme/templates.rs
pub fn environment(theme: &Theme, debug: bool) -> Result<minijinja::Environment<'static>>;
// src/theme/mod.rs
pub fn recording_reads<R>(f: impl FnOnce() -> R) -> (R, BTreeSet<PathBuf>);

// src/render/paths.rs
pub fn resolve_helper_path(theme: &Theme, path: &str) -> Result<Option<(Vec<u8>, String)>>;
pub fn content_version(bytes: &[u8]) -> u64;
// src/render/styles.rs
pub fn compile_theme_css(theme: &Theme, registry: &mut AssetRegistry) -> Result<String>;
pub(super) fn remote_imports(css: &str) -> Vec<String>;
// src/render/reveal.rs (keep these two)
pub fn math_enabled(config: &Config, features: &Features) -> bool;
fn script_safe_json(json: &str) -> String { json.replace('<', "\\u003c") }

// src/pipeline.rs
pub fn load_config(opts: &BuildOptions) -> Result<(Config, Option<PathBuf>)>;
pub fn detect_kind(path: &Path) -> Result<SourceKind>;
pub fn write_output(build: &Build, out: &Path, mode: &OutputMode) -> Result<Vec<PathBuf>>;
pub fn find_clash(outputs: &[PathBuf], inputs: &[(PathBuf, InputRole)]) -> Option<Clash>;

// src/cli/report.rs
pub type SourceCache<'a> = &'a dyn Fn(&Path) -> Option<String>;
pub fn format_error(err: &Error, input: Option<&Path>, cache: SourceCache<'_>, color: bool) -> String;
pub fn install_panic_hook();
pub fn stderr_color() -> bool;
// src/cli/mod.rs
pub const EXIT_OK: ExitCode; EXIT_BUILD_ERROR = 1; EXIT_USAGE = 2;   // cli/export.rs: EXIT_NO_CHROME = 3
pub fn display_path(path: &Path) -> PathBuf;

// src/pdf.rs
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
pub fn find_chrome(explicit: Option<&Path>) -> Option<PathBuf>;
pub fn html_to_pdf(chrome: &Path, html: &str, timeout: Duration) -> Result<Vec<u8>>;
pub fn page_count(pdf: &[u8]) -> usize;
const CHROME_FLAGS: &[&str] = &["--headless=new","--disable-gpu","--no-first-run","--no-default-browser-check","--disable-extensions","--disable-background-networking","--disable-component-update","--disable-sync","--disable-default-apps","--disable-domain-reliability","--disable-client-side-phishing-detection","--no-service-autorun","--password-store=basic","--use-mock-keychain","--hide-scrollbars","--mute-audio","--remote-debugging-pipe"];
const LAUNCH_SCRIPT: &str = r#"exec "$0" "$@" 3<&0 4>&1 0</dev/null 1>/dev/null"#;
// Page.printToPDF params used: { preferCSSPageSize: true, printBackground: true, marginTop/Bottom/Left/Right: 0, transferMode: "ReturnAsStream" }
// src/pdf/devtools.rs (feature browser-tests)
pub fn launch(chrome: &Path, timeout: Duration) -> Result<Page>; pub fn navigate(&mut self, url: &str) -> Result<()>; pub fn evaluate(&mut self, expression: &str) -> Result<Value>; pub fn errors(&self) -> &[String];
pub fn readable_temp_dir(chrome: &Path) -> Option<PathBuf>;

// src/pptx.rs (generic part)
pub const EMU_PER_INCH: u64 = 914_400;  pub const EMU_PER_POINT: u64 = 12_700;
struct Zip { bytes: Vec<u8>, directory: Vec<u8>, entries: u16 }   // fn add(&mut self, name: &str, data: &[u8]) -> Result<()>; fn finish(self) -> Result<Vec<u8>>
const DOS_DATE: u16 = (1 << 5) | 1;  const DOS_TIME: u16 = 0;
fn crc32(data: &[u8]) -> u32;   // poly 0xEDB88320, crc32(b"123456789") == 0xCBF4_3926
fn escape(text: &str) -> String; fn clean(text: &str) -> String; fn is_xml_char(c: char) -> bool;
fn single_line(text: &str) -> String; fn cut(text: &str, max: usize) -> String;
pub fn image_from_data_url(url: &str) -> Option<Image>;  impl Image { pub fn dimensions(&self) -> Option<(u32, u32)> }
```

Live-reload client contract (assets/presenter/live-reload.js): `new EventSource('/__presenter/events')`;
events `hello` and `build`; `sessionStorage['presenter:build_id:' + location.pathname]`;
`link#presenter-theme` is the theme stylesheet swapped in place; overlay element id
`presenter-overlay`; debug handle `window.__presenterLiveReload = { connect, disconnect, state }`.

---

## 12. Open questions

1. **ZIP writer for docx**: reuse pptx.rs's 80-line stored-only `Zip` (deterministic, no deflate,
   no dependency) or take `zip` 9.0.5 for deflate? Word opens stored packages fine (presenter
   proves it for pptx); docx files are small, so stored is acceptable and keeps output
   byte-reproducible.
2. **minijinja 3.0.3** is out (presenter pins 2.24.0). Lifting `theme/templates.rs` verbatim means
   pinning 2.x; upgrading needs a check of `set_loader`, `UndefinedBehavior` and `Value` APIs.
3. **comrak 0.57.0** vs the 0.56 contract notes (attribute rejection rules, directive fence
   closing). The dialect spec's "A3 notes" are tied to 0.56 behaviour; verify with the differential
   tests in `attrs.rs` before bumping.
4. **notify 9 rc**: stay on 8.2.0 (stable, what the watcher was tested against on FSEvents/inotify).
5. **Which `render.rs` directive table** writer wants (`figure`, `callout` family, `table` with
   caption, `include`, `pagebreak`, `toc`?) must be fixed before step 12; each extra directive needs
   an HTML form *and* a docx/typst/LaTeX mapping.
6. **IR granularity**: presenter's `Block::Html` (rendered HTML string) is cheap for the HTML
   backend but useless for docx/typst/LaTeX. Writer needs either (a) a structured block/inline IR
   produced from the comrak AST (one walker per backend), or (b) keep comrak's AST alive and run a
   per-backend formatter (`format_document_with_formatter` exists for HTML only). (a) is the
   clean choice; it means `render.rs`'s HTML-specific walker becomes one of several backends and
   step 12 is "adapt into `backend/html.rs`".
7. **PDF backends**: "light" = HTML → headless Chrome (`pdf.rs`, Unix-only, no Windows); "heavy" =
   typst (`typst` crate 0.15.1 or the CLI, not installed on this machine) or LaTeX via pandoc/TeX
   Live (installed here). Decide whether Chrome stays the default PDF path.
8. **Notebook metadata key**: `metadata.presenter` → `metadata.writer`; should writer also read
   `metadata.presenter` for shared notebooks?
9. **SCSS themes**: 3 175 LOC incl. worker process + recursion checker. Worth deferring until a
   CSS-only theme chain exists.
10. **Site mode** (`OutputMode::Site`) maps naturally to a multi-page HTML document (chapters);
    decide whether writer's HTML backend has a one-file and a directory mode from day one.
