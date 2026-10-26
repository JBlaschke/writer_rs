# T16 — Pipeline and CLI tree with stubs, `build` + `dump`, example, e2e snapshots

| | |
|---|---|
| Milestone | M1 |
| Size | M |
| Depends on | T03, T10, T12, T13, T14, T15, T17 |
| Parallel-safe with | T18, T50 (wave 4; see the board) |
| Files you own | `src/pipeline/mod.rs`, `src/pipeline/html.rs`, `src/cli/mod.rs`, `src/cli/build.rs`, `src/cli/dump.rs` (replace T00 stubs); `src/cli/report.rs` (lift from presenter_rs/src/cli/report.rs; → T73); `examples/basic/{document.md, writer.toml, figures/plot.png, README.md}` (new); `tests/{cli_build,cli_dump,html_snapshots,scan_includes}.rs` and `tests/snapshots/{cli_build,html_snapshots}__*.snap` (new). Additive: `src/markdown/include.rs` (`scan_includes` lift, T1B's file), `src/typst/includes.rs` (`MarkdownIncludes` stub, T53's file), the stub entry points of PLAN §6 step 5 in `src/cli/{serve,check,init,eject}.rs` (T23, T1C, T72), `src/pipeline/{pdf,typst,docx,latex,typst_source}.rs` (T32, T34, T41, T61, T52) and `src/pdf/chrome.rs` (T77); `src/main.rs` (T00) only if its body is not already `writer::cli::run()` |

## Goal
Everything of M1 meets here. `writer build examples/basic/document.md` writes one self-contained HTML file,
`writer dump` prints the resolved IR or the template context, and every command and target of cli.md exists:
the ones owned by later tasks are stubs that print `<what>: not implemented yet (Txx)` and exit 2, so later
tasks only replace stub bodies and never touch `src/pipeline/mod.rs` or `src/cli/mod.rs`. The serve-facing
pipeline API of live-reload.md §5.2 (`prepare`, `web_page`, `build_targets`, `WorldSlot`, …) is what T20,
T21, T23, T24, T1C, T32, T41 and T61 build on.

## Read first
- `docs/spec/live-reload.md` §5.2, §5.3, §9 ("Unit. T16"), §7 first bullet, Deviations bullet 5 (**your spec**)
- `docs/spec/cli.md` whole (**your spec**: tree, `build`, `dump`, Global flags); `docs/spec/config.md` §1, §2 (`out_dir`), §3, §4 incl. "Strict escalation" (**your spec**)
- `docs/spec/typst-dialect.md` §6.1–6.2 (World API, `world_inputs`), §6.4 (**your spec**: stub + installation), §8.1 (`pdf_timestamp`), §8.4 (timestamp policy), §2.1 (`E-TYP-*` fail html/docx/latex)
- `docs/spec/html-output.md` §1.8 + §1.15 (`MathHook`, `KatexOnly`, `emit`), §3.3 (`pipeline::resolve_theme` roots), §4.4 (`RenderInput`, `render`, `context_json`), §6 (modes, §6.3 size budget), §9 row T16, Deviations "Math hook"
- `docs/spec/ir.md` §4 (frontends, `resolve`, `detect_kind`), §5 (`Error`, `Diagnostics`); `docs/spec/references.md` §4.1 (`lenient`), §4.7, §5.1 last paragraph, §5.7
- `docs/spec/markdown-dialect.md` §8 last bullet, §11; `docs/spec/vendored-assets.md` §8.2, §7.3; `docs/spec/docx-output.md` §1.1 (last paragraph); `docs/spec/latex-output.md` §1.3 (last paragraph), §2.3
- `docs/research/R8-presenter-inventory.md` §3.1, §3.7, §8, §10 rows 21 and 23, §11; `docs/research/R1-typst-embedding.md` §4, §9
- presenter_rs: `src/pipeline.rs`, `src/cli/{mod,build,dump,report}.rs`, `src/markdown/include.rs` L395–L440 and L776–L820, `src/serve/watch.rs` L376–L445, `tests/cli_build.rs` (helpers)

## API to implement
```rust
// src/pipeline/mod.rs — live-reload.md §5.2, verbatim (add Debug/Clone/Copy/PartialEq derives where useful)
pub enum Mode { Build, Serve { build_id: u64 }, Check }
pub struct BuildOptions { pub input: PathBuf, pub format: Option<SourceKind>, pub config_path: Option<PathBuf>,
                          pub cli: CliOverrides, pub mode: Mode }
pub struct Prepared { pub document: Document, pub assets: AssetRegistry, pub config: Config,
                      pub config_path: Option<PathBuf>, pub diagnostics: Diagnostics }
pub struct BuildFailure { pub error: Error, pub diagnostics: Diagnostics }
pub enum InputRole { Source, Include, Config, Asset, MissingAsset, Theme, Bibliography, PdfTemplate, ReferenceDoc, LatexTemplate }
pub fn prepare(opts: &BuildOptions) -> Result<Prepared, BuildFailure>;
pub fn watch_inputs(prepared: &Prepared) -> Vec<(PathBuf, InputRole)>;
pub struct WebPage { pub html: String, pub theme_css: String, pub theme: Theme, pub diagnostics: Diagnostics, pub assets: AssetRegistry }
pub fn web_page(prepared: &Prepared, opts: &BuildOptions, world: &mut WriterWorld) -> Result<WebPage, BuildFailure>;
pub fn subset_error(diagnostics: &Diagnostics) -> Option<&Diagnostic>;
pub fn load_typst_main(prepared: &Prepared, opts: &BuildOptions, world: &mut WriterWorld) -> Result<(), BuildFailure>;
pub struct TargetResult { pub target: Target, pub written: std::result::Result<Vec<PathBuf>, Error> }  // main output first
pub struct BuildReport { pub prepared: Result<Prepared, BuildFailure>, pub targets: Vec<TargetResult>, pub diagnostics: Diagnostics }
pub fn build_targets(opts: &BuildOptions, targets: &[Target], worlds: &mut WorldSlot) -> BuildReport;
pub struct WorldSlot { /* Option<WriterWorld> + the values it was built from */ }
impl WorldSlot { pub fn new() -> Self;
    pub fn get(&mut self, prepared: Option<&Prepared>, opts: &BuildOptions) -> Result<&mut WriterWorld, BuildFailure>;
    pub fn current(&mut self) -> Option<&mut WriterWorld>;  pub fn discard(&mut self); }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")]
pub enum ChangeKind { Css, Pages, Content }
// src/typst/includes.rs — typst-dialect.md §6.4 stub (T53 replaces the body)
pub struct MarkdownIncludes { pub config: Config }
impl IncludeProvider for MarkdownIncludes { fn synthesize(&self, path: &Path, root: &Path) -> Result<Synthesized> }
// src/markdown/include.rs — markdown-dialect.md §8 (lifted)
pub const MAX_DEPTH: usize = 8;  pub fn scan_includes(path: &Path) -> Vec<PathBuf>;
// T16-chosen (no spec fixes them; see "Notes for the coordinator"): pipeline/mod.rs additions
impl BuildOptions { pub fn new(input: impl Into<PathBuf>) -> BuildOptions }   // Mode::Build, no format/config/overrides
pub fn parse_only(opts: &BuildOptions) -> Result<Prepared, BuildFailure>;      // prepare without resolve (dump --no-resolve)
pub fn load_config(opts: &BuildOptions) -> crate::error::Result<(Config, Option<PathBuf>)>;   // lifted
pub fn resolve_theme(prepared: &Prepared, opts: &BuildOptions) -> crate::error::Result<Theme>;  // html-output.md §3.3
pub fn output_path(prepared: &Prepared, opts: &BuildOptions, target: Target, n_targets: usize) -> PathBuf;
pub fn escalate_strict(diagnostics: Diagnostics, cli_strict: Option<bool>, config: &Config) -> Diagnostics;  // config.md §4
pub fn not_implemented(what: &str, task: &str) -> Error;   pub fn is_not_implemented(err: &Error) -> bool;
pub fn missing_asset_as_error(diagnostic: &Diagnostic) -> Option<Error>;   // + Clash, find_clash, same_file, refused_clash (lifted)
// pipeline/html.rs (T16)
pub fn render(prepared: &Prepared, theme: &Theme, output: OutputMode, build: BuildInfo, world: &mut WriterWorld)
    -> crate::error::Result<(RenderedDocument, AssetRegistry)>;
pub fn build(prepared: &Prepared, opts: &BuildOptions, out: &Path, world: &mut WriterWorld) -> crate::error::Result<(Vec<PathBuf>, Diagnostics)>;
pub fn context(prepared: &Prepared, world: &mut WriterWorld) -> crate::error::Result<serde_json::Value>;   // dump --context
// Stubs in the owners' files, body `Err(not_implemented(..))`: pipeline/pdf.rs ("pdf", T32), typst.rs ("typst", T34),
// latex.rs ("latex", T61), typst_source.rs ("pdf", T52), src/pdf/chrome.rs ("pdf --engine chrome", T77):
pub fn build(prepared: &Prepared, opts: &BuildOptions, out: &Path, world: &mut WriterWorld) -> crate::error::Result<(Vec<PathBuf>, Diagnostics)>;
// pipeline/pdf.rs also: load_typst_main (same signature as above; mod.rs's delegates to it), page_count → None,
// dump ("dump --typst", T32); pipeline/latex.rs also dump ("dump --latex", T61):
pub fn page_count(pdf: &[u8]) -> Option<usize>;   pub fn dump(prepared: &Prepared, opts: &BuildOptions) -> crate::error::Result<(String, Diagnostics)>;
// pipeline/docx.rs (T41), signature from docx-output.md §1.1:
pub fn build(doc: &Document, assets: &AssetRegistry, config: &Config, out: &Path, world: &mut WriterWorld) -> crate::error::Result<Diagnostics>;
// src/cli/<cmd>.rs, every command (presenter's convention):  pub fn run(args: &<Cmd>Args, global: &GlobalArgs) -> anyhow::Result<ExitCode>
```

## Steps
1. **Types, config, parse** (`src/pipeline/mod.rs`): the §5.2 types; lift from presenter `pipeline.rs` `load_config` + `sets_out_dir` (L441–L483; also make relative `[pdf] font_paths` and `[typst] package_paths` absolute against the `writer.toml`'s directory, config.md §3), `write_file`, `read_source`, `source_info`, `absolute`, `io_error` (L886–L930). `parse_only`: kind = `opts.format` else `ir::detect_kind`; load config (CLI applied); dispatch `markdown::parse` / `notebook::parse` / `typst::frontend::parse` (ir.md §4); then `config.merge_front_matter(&document.meta)` (`Err` → `BuildFailure` with the parse diagnostics) and `apply_cli` again (config.md §4 precedence). `prepare` = `parse_only` + `resolve(&mut doc, &ResolveOptions { config, library: None /* T70: [references] wiring */, lenient: !matches!(opts.mode, Mode::Build) })` (references.md §4.1: lenient for serve and check), diagnostics appended.
2. **`WorldSlot`** (mod.rs), exactly live-reload.md §5.3: private `world: Option<WriterWorld>`, the values it was built from (`font_paths`, `system_fonts`, `allow_packages`, `package_paths`, `from_prepare: bool`) and a private `created: u64` counter for the unit tests. Create: root = input's directory, `MainFile::Path(abs input)` for `.typ` else `MainFile::Virtual`, `today = pdf_timestamp(&config.pdf, &document.meta, env SOURCE_DATE_EPOCH, SystemTime::now())`; with `None`, `load_config(opts)` (default config on error) and today = now. Recreate/keep rules, `set_inputs(world_inputs(&prepared.config))` on keep, and on every `get` with `Some` `set_include_provider(Box::new(MarkdownIncludes { config: prepared.config.clone() }))`. A failing `pdf_timestamp`/`from_config` leaves the slot empty and returns `Err(BuildFailure { error, diagnostics: default })`.
3. **`MarkdownIncludes` stub** in `src/typst/includes.rs` (additive): `synthesize` returns `Err(Error::Other("including .md/.ipynb from typst arrives in T53".into()))` (typst-dialect.md §6.4). Import the trait and `Synthesized` from T03's `crate::typst::world`.
4. **Helpers** (mod.rs): `resolve_theme` (spec `config.html.theme`; roots: `opts.cli.theme` set → [CWD]; `theme` or `html.theme` in `document.meta.vars` → [`document.source.base_dir`]; the `writer.toml` sets `[html] theme` (read its table like `sets_out_dir`) → [its directory]; else [] → `theme::resolve`). `watch_inputs`: Source, Include (`document.includes`), Config, Asset (`AssetData::File`), MissingAsset (path parsed from each `missing-asset` message, presenter's format `… (looked for <path>)`, `markdown/render.rs` L262–L271), Bibliography (+ a `.csl` style), PdfTemplate, ReferenceDoc, LatexTemplate (relative: document dir when the document set the key, else the `writer.toml` dir); Theme = `watch_dirs()` of the theme resolved against each of [config dir, document dir, CWD], else presenter's `theme_candidates` + `extends` (`serve/watch.rs` L394–L445). `subset_error`, `escalate_strict` (config.md §4 rules, idempotent; `Diagnostics` has no `iter_mut`: rebuild via `into_vec`), `not_implemented`/`is_not_implemented` (`Error::Other("<what>: not implemented yet (<task>)")`), and lifted `Clash`/`find_clash`/`same_file`/`path_after_creating_dirs`/`file_id`/`refused_clash` (L587–L885, `InputRole` `Display` extended), `escalate_missing_assets`/`missing_asset_as_error` (L932–L1013), with their presenter unit tests.
5. **`src/pipeline/html.rs`**: one private `fn math_hook<'a>(config: &'a Config, world: &'a mut WriterWorld) -> Box<dyn MathHook + 'a>` whose body is the single line `Box::new(KatexOnly { usage: MathUsage::default() })` (T1A replaces it with `MathRenderer::new(config, world)`; mark `world` used with `let _ = &world;`). `render`: `RenderInput { document, assets: &mut prepared.assets.clone(), theme, config: &prepared.config, output, build, math }` → `html::render`. `build`: standalone = `config.html.standalone`; lifted `output_mode`/`document_path`/`SITE_INDEX`/`SITE_ASSETS_DIR`; clash check of every file against the inputs (`watch_inputs` with each Theme dir expanded as presenter's `input_files`/`theme_files`, plus `RenderedDocument.theme_files`); write; return `[file]` (standalone) or `[index.html, <out>/assets]` (site). `context`: `html::context::context_json` with `BuildInfo::build(standalone)`.
6. **`web_page`**: `subset_error(&prepared.diagnostics)` → `Err` exactly as §5.2 says; else `resolve_theme`, then `html::render` with `Mode::Serve { build_id }` → `OutputMode::Serve` + `BuildInfo::serve(build_id)`, other modes → `Standalone` + `BuildInfo::build(true)`; `WebPage.diagnostics` = the render's only; `assets` = the registry `render` filled. `load_typst_main` delegates to `pdf::load_typst_main`.
7. **`build_targets`**: (a) `prepare`; in `Mode::Build` the first `missing-asset` becomes `Err(Error::MissingAsset)` (`escalate_missing_assets`; `dump` and `check` keep warnings); (b) `worlds.get(Some(&prepared), opts)` then `world.reset()` (`Err` → `prepared: Err`, no targets); (c) targets deduplicated in order; per target `output_path` (no `-o`: `<out_dir>/<stem>.{html,pdf,docx,tex,typ}`, html site `<out_dir>/<stem>/`; `-o P` names it for one target, is the directory for several), main output clash check, then `html::build` · `pdf` (`.typ` input → `typst_source::build`; `[pdf] engine = "chrome"` → `pdf::chrome::build`; else `pdf::build`) · `docx::build` (→ `vec![out]`) · `latex::build` · `typst::build` (`.typ` input → `Err`: use `writer build part.md --to typst`); `html`/`docx`/`latex` first return `Error::Parse` from `subset_error` (typst-dialect.md §2.1); (d) `diagnostics` = prepare's then each target's, through `escalate_strict(.., opts.cli.strict, &prepared.config)`; on `prepared: Err(f)`, escalate `f.diagnostics` in place (by the `--strict` flag only: no effective config exists) and leave `BuildReport.diagnostics` empty. Write the stub bodies of the owner files listed in "API".
8. **`scan_includes`** into `src/markdown/include.rs` (additive): copy presenter's `MAX_DEPTH`, `include_path` (L399–L440), `scan_includes`, `scan_file` (L780–L820) under the same names (T1B's lift then merges); `is_include_info(&d.info)` → `crate::markdown::directives::parse_directive_info(&d.info).name == "include"` (T11), `included_kind` → `crate::ir::detect_kind(path).ok()`, `normalize_lexically` from `crate::assets::registry`.
9. **`src/cli/mod.rs`**: lift presenter's `cli/mod.rs` (exit codes, `UsageError`, `run`/`run_from`/`dispatch`, `version()` with `WRITER_GIT_SHA`, `exit_code_for`, `input_format` (`md|ipynb|typ`), `locate`, `display_path` family, `clash_error`, `reject_directory`, `print_stdout`, `log_directives` with `LOG_TARGET = "writer"`, `init_logging`) and rewrite the arg structs from cli.md: `build` (`--to` comma list of a local `ValueEnum` → `Target`, `-o/--out`, `--watch`, `--theme`, `--template`, `--reference-doc`, `--latex-template`, `--standalone`/`--site`, `--strict`, `--engine typst|chrome`, `--allow-packages`, `--format`), `serve`, `check` (`--target`, `--deny-warnings`, `--format text|json`, `--compat presenter`), `dump` (`--ir|--typst|--latex|--context` mutually exclusive, `--no-resolve` only with `--ir`; `INPUT` with `required_unless_present = "default_config"`, `--default-config` conflicting with everything else, two `override_usage` lines), `init [DIR] [--typst] [--bib] [--force]`, `eject` (5 subcommands of cli.md). `Cli { global, licenses: bool, command: Option<Command> }`: `--licenses` prints `vendor::licenses()` to stdout and returns 0; no command otherwise → `Cli::command().error(ErrorKind::MissingSubcommand, …)`, printed, exit 2. Stubs: `serve` (T23), `check` (T1C), `init`/`eject` (T72) print `pipeline::not_implemented(..)`'s text on stderr and return exit 2.
10. **`src/cli/report.rs`**: lift verbatim (R8 §3.7), then: `Level::Info` printed only with `-v` (cli.md), warnings before errors (stable), `-q` drops warnings; `Error::Compile` located at `path:line` with one `hint:` line per hint, `Error::Resource` named by `what` and path (T73 refines both).
11. **`src/cli/build.rs` / `dump.rs`**: `build`: `--watch` → `build --watch: not implemented yet (T20)` exit 2 (T20 replaces this branch, live-reload.md §7); targets = `--to` else `load_config` + `targets_for(kind)`; `.typ` + `typst` → usage error; `build_targets`; print diagnostics, then one `Wrote <path> (<detail>, <size>)` per written path (`format_size` lifted with its test; a directory → `<n> files`, a `.pdf` → `pdf::page_count` pages when `Some`); exit = max of 2 (usage, config, refused clash, `is_not_implemented`), 1 (other target or prepare errors, any error diagnostic), 0. `dump`: `--default-config` prints `Config::default_toml()`; `--ir` = `prepare` (`--no-resolve`: `parse_only`), `Mode::Build`, `serde_json::to_string_pretty(&document)`; `--context` = `html::context` over a fresh `WorldSlot`; `--typst`/`--latex` call the `pdf::dump`/`latex::dump` stubs and print only non-empty text; exit 1 when a diagnostic is an error.
12. **Example and tests**: `examples/basic/document.md` per PLAN §2.1 minus what T1B adds (`@` references, citations, `:::include`): front matter (title, authors, date, abstract, `toc: true`, `numbering`), labelled headings, an implicit figure `figures/plot.png {#fig-… width=70%}`, a `Table:` caption, an alert and `:::warning`, a captioned code listing, inline and labelled display math, a footnote, a `{=latex}` fence, `:::pagebreak`; `writer.toml` with a comment per key set; `figures/plot.png` a small real PNG (≤ 20 KB, generated locally, never downloaded); `README.md` with the commands. Then write the tests below.

## Acceptance criteria
- `tests/cli_build.rs` (assert_cmd into `tempfile` dirs, presenter's `project()` helper): `example_builds_standalone` (`build examples/basic/document.md -o <tmp>/d.html` → 0, one stdout line `^Wrote .*d\.html \([0-9.]+ (B|kB|MB)\)$`, one `data-writer-document`, no `asset:`, no `/__writer/`; snapshot `cli_build__example_basic_main.snap` of the `<main data-writer-document`…`</main>` region, `data:[^"']+` filtered), `example_builds_into_dist` (copy, run in the copy → `dist/document.html`), `site_mode_writes_index_and_assets`, `missing_input_exits_1` (stderr names the file), `unknown_theme_exits_1` (`theme 'nope'`), `unknown_extension_exits_2`, `output_over_input_is_refused` (exit 2, `refusing to write`, input unchanged), `stub_targets_exit_2` (`--to pdf|docx|latex|typst` → `pdf: … (T32)`, `(T41)`, `(T61)`, `(T34)`; `--engine chrome --to pdf` → `(T77)`; `--to html,pdf` writes the HTML and exits 2), `typst_target_on_typ_input_exits_2`, `watch_is_a_stub`, `command_stubs_exit_2` (serve T23, check T1C, init T72, eject theme T72), `licenses_flag` (stdout == `writer::vendor::licenses()`, exit 0; bare `writer` → 2), `version_flag` (`^writer 0\.1\.0`).
- `tests/cli_dump.rs`: `dump_ir_is_resolved` (stdout deserializes into `writer::ir::Document`; `resolved.toc` and the example's labels have numbers), `dump_no_resolve` (no `resolved` key), `dump_context_keys` (top-level keys in html-output.md §4.1 order), `dump_default_config` (== `Config::default_toml()`, `--config missing.toml` ignored; with `INPUT` → 2), `dump_stub_flags` (`--typst` → `(T32)`, `--latex` → `(T61)`, exit 2), `dump_flags_conflict` (`--ir --context` → 2).
- `tests/html_snapshots.rs` (html-output.md §9 row T16): `md_fixtures` writes `html_snapshots__<stem>.snap` (`html.main`, asset ids relabelled `000000000001…` in first-use order) and `html_snapshots__<stem>-diagnostics.snap` (one `level code file:line message` line each, fixture dir filtered to `[fixtures]`; an `Err` parse snapshots its Display) for every `tests/fixtures/md/*.md` (T01); `serve_minimal_page` → `html_snapshots__serve_minimal.snap` (`?v=\d+` filtered); `standalone_size_budget`: built `minimal.md` ≤ 60,000 B, `math.md` ≤ 700,000 B.
- `src/pipeline/mod.rs` unit tests (live-reload.md §9; build `Prepared` by hand where a `.typ` input is needed, T50's frontend may still be T01's stub): `world_slot_creates_and_keeps`, `world_slot_recreates_on_font_paths_change`, `world_slot_recreates_after_world_without_prepare`, `world_slot_discard`, `invalid_timestamp_leaves_slot_empty`, `kept_world_sees_new_writer_config` (a `.typ` main with `#assert.eq(json(bytes(sys.inputs.at("writer-config"))).numbering.headings, "1.1")` compiles after `[numbering] headings = "1.1"`, same `created`), `md_include_answers_t53_stub` (`World::source` of an `x.md` id → `FileError::Other` containing `arrives in T53`), `subset_error_fails_web_page` (`E-TYP-SYNTAX` at line 3 → `Error::Parse` line 3), `subset_warning_keeps_web_page`, `strict_by_flag`, `strict_by_config`, `strict_is_idempotent`, plus presenter's lifted clash and missing-asset tests.
- `tests/scan_includes.rs`: `scanned_in_a_markdown_document_of_any_extension` (lifted from presenter `tests/include.rs`), `transitive_and_stops_at_cycles`, `missing_files_are_listed`.
- `cargo run -- build examples/basic/document.md` exits 0 and writes `examples/basic/dist/document.html` (PLAN §6 step 8).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
Serve routes/session (T21, T23), the watcher and `build --watch` (T20), `check` (T1C, T71), `init`/`eject` (T72),
PDF (T32, T52; Chrome T77), DOCX (T41), LaTeX (T61), `--to typst` (T34), KaTeX/MathML/image math (T1A), `@`
references and `:::include` expansion (T1B; it updates the example and snapshots), citations and the `Library`
wiring (T70), `MarkdownIncludes` body (T53), compile-error excerpts (T73), the guide and `--help` mirroring (T74),
theme styling (T19, which re-blesses `html_snapshots__*.snap`). Never edit `Cargo.toml`, `src/ir.rs`, `src/config.rs`, `lib.rs` module lists.

## Hints (verified)
- clap 4.6.7 (`derive`, `wrap_help`): `Command::error(&mut self, kind: ErrorKind, message: impl fmt::Display) -> Error`
  (clap_builder-4.6.7 `builder/command.rs:641`), `ErrorKind::MissingSubcommand`, `Arg::required_unless_present`,
  `Arg::conflicts_with_all`, `Command::override_usage`; presenter's `DumpArgs` is the pattern. `Cli::command()` needs `clap::CommandFactory`.
- assert_cmd 2.2.2: `assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"))` (`cmd.rs:231`), `.env_remove("RUST_LOG")`,
  `.env_remove("NO_COLOR")`, `.env_remove("CLICOLOR_FORCE")` (R8 §8); predicates 3.1.4 `predicates::str::contains`.
- insta 1.49.0 (features `json`, `filters`, `redactions`; **no** `glob`): iterate `read_dir`, sort, call
  `insta::assert_snapshot!(name, text)` inside `Settings::bind`; `Settings::add_filter(regex, replacement)` (`settings.rs:527`).
  The file is `tests/snapshots/<test file stem>__<name>.snap`.
- tracing-subscriber 0.3.23: `EnvFilter::builder().parse_lossy(directives)` (`filter/env/builder.rs:146`).
- typst 0.15.1: `typst::comemo::evict(n)` (`pub extern crate comemo;`, `typst/src/lib.rs:33`); `json` takes
  `Spanned<DataSource>` with `DataSource::Bytes` (typst-library `loading/mod.rs:48`), so `json(bytes(..))` works; file ids:
  `RootedPath::new(VirtualRoot::Project, VirtualPath::new("x.md").unwrap()).intern()` (R1 §9). Embedded `FontStore`:
  1.4–4.8 ms (R1 §4); set `[pdf] system_fonts = false` in WorldSlot tests to skip the system scan.
- comrak 0.57.0: `options.extension.block_directive`, `options.extension.front_matter_delimiter` (`parser/options.rs:643, 239`);
  `parse_document(&arena, text, &options) -> Node<'a>`; `NodeValue::BlockDirective(Box<NodeBlockDirective>)`, field `info` (R9 §15).
- In mod.rs, §5.2's two-parameter `Result<_, BuildFailure>` is `std::result::Result`; `WorldSlot::new` needs `impl Default` (clippy `new_without_default`).
- Inside `src/pipeline/`, `typst` names the submodule: write `crate::typst::…` for writer's module, `::typst::…` for the crate.
- `Diagnostics` (ir.md §5): `push, info, warn, error, iter, has_errors, has_warnings, len, is_empty, extend, into_vec`, `Default`.
- Compiler-backed tests are slow unoptimised; T00's `[profile.dev.package."*"] opt-level = 2` makes them usable. Pins: vendored-assets.md §7.3.

## Notes for the coordinator
- No spec fixes the entry points of `pipeline/{pdf,typst,latex,typst_source}.rs`, `pdf/chrome.rs`, `pdf::page_count`,
  `pdf::dump`/`latex::dump` (`dump --typst`/`--latex`) or `cli/<cmd>.rs::run`; this file fixes them (only
  `pipeline/docx.rs`'s comes from docx-output.md §1.1). T32, T34, T52, T61, T77, T23, T1C, T72 must replace exactly these bodies.
  `load_typst_main` in mod.rs delegates to `pdf::load_typst_main` so that T32 never edits mod.rs (§5.2 says T32 "replaces the body").
- Local decisions where the specs are silent: outputs are written although `prepare` raised error diagnostics
  (e.g. `unresolved-reference` in build), exit 1; a stub target exits 2 even when other targets succeeded; the
  `missing-asset` escalation runs in `build_targets`, so `dump` (lenient false, references.md §4.1) keeps missing assets as warnings.
- `watch_inputs(prepared)` cannot see `--theme`, so its Theme entries over-approximate (every root).
  Path-valued template/reference/bibliography settings are normalized by T16 at
  their source layer (see Amendments), so later watch/output stages need no path provenance guess.
- typst-dialect.md §6.4 gives the no-provider error and T16's stub the same text, so live-reload.md §9's "not `no include
  provider`" cannot be told apart by message; the test checks `arrives in T53` only.
- `src/main.rs` (T00) must call `writer::cli::run()`; `.gitignore` (T00) should cover `examples/**/dist/`; exit 3 for a
  missing Chrome (cli.md) is T77's additive change to `cli/build.rs`; PLAN §2's `58 KB` is printed `58.0 kB` (lifted `format_size`).

## Amendments
- **2026-10-26 (coordinator):** T16 starts from completed T15 (`647fce5`, PR #8)
  under the user-authorized stacked-PR workflow. CI/merge waits do not block
  implementation; feature branches and merge order remain explicit.
- **2026-10-26 (coordinator):** closed the path-origin gap in the notes: T16
  normalizes file-config PDF/DOCX/LaTeX template paths, bibliography and
  path-valued CSL styles against the config directory. Front-matter overrides
  resolve against the document directory, and CLI template/reference overrides
  against the working directory. Preserve absolute values and empty built-in
  paths. This implements config.md §3 without changing Config or Prepared shapes.
- **2026-10-26 (coordinator):** validate output file/directory spelling using
  effective Prepared config after front matter, before writing. A front-matter
  site-mode override can target an existing directory; a standalone file output
  cannot. Invalid output settings retain usage/config exit code 2.
