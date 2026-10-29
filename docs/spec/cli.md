# Spec: Command-line interface

**Status:** contract. `clap` 4 derive, binary name `writer`. Global flags come first.
Everything presenter_rs's CLI spec says about safety (outputs never overwrite inputs,
file-identity clash checks, exit codes, stdout/stderr discipline, `--help` blocks mirrored in
the guide by `tests/guide.rs`) applies unchanged; this document lists what differs.

T16 implements the complete command tree below. Commands and targets owned by later tasks are
stubs that print `<what>: not implemented yet (Txx)` and exit 2; the owning task replaces the
stub's module body (PLAN.md §6, step 5).

```
writer [-v|-vv] [-q] [--config PATH] [--licenses] <COMMAND>

Commands:
  build    Render a document to HTML, PDF, DOCX, LaTeX or typst source
  serve    Render, watch and live-reload in the browser
  check    Lint a document without building
  dump     Print the IR, the generated typst or LaTeX source, or the template context
  init     Scaffold a new document directory
  eject    Copy a built-in theme, template, package or reference document to edit it
```

## build
```
writer build <INPUT> [--to LIST] [-o/--out PATH] [--watch] [--theme NAME|PATH] [--template PATH]
             [--reference-doc PATH] [--latex-template PATH] [--standalone|--site] [--strict]
             [--engine typst|chrome] [--allow-packages]
```
- `INPUT`: `.md`, `.ipynb` or `.typ` (by extension; `--format md|ipynb|typ` overrides).
- `--to`: comma-separated targets `html`, `pdf`, `docx`, `latex`, `typst`; default
  `build.targets` (`html`) for `.md`/`.ipynb` and `build.typst_targets` (`pdf`) for `.typ`.
  Every target is built from the same parse and resolve; a failure in one target still writes
  the others and exits 1.
- Default outputs, next to each other in `<build.out_dir>/` (an `out_dir` from `writer.toml` is
  relative to that file; the default `dist` is relative to the input's directory):

  | target | output |
  |---|---|
  | html | `<stem>.html` (standalone) or `<stem>/index.html` + `<stem>/assets/` (site) |
  | pdf | `<stem>.pdf` |
  | docx | `<stem>.docx` |
  | latex | `<stem>.tex` + `<stem>_assets/` (images; SVG/GIF/WebP converted; `.bib` copies) |
  | typst | `<stem>.typ` + `<stem>_assets/` (images) + one `<stem>_<include>.typ` per materialised `.md`/`.ipynb` include |

  `-o` with one target names the file (or the site directory); with several targets it names
  the directory. For `latex`/`typst`, the assets directory sits next to the named file.
- For a `.typ` input, `pdf` compiles the user's file directly; the other targets go through
  the structured subset (typst-dialect.md §2). `--to typst` on a `.typ` input is an error.
- `--strict`: warnings about constructs a target cannot render (`unsupported-construct`,
  `math-conversion-failed`, `W-TYP-*`, `E-TYP-MATH-CODE`) become errors.

  > **Contract change (coordinator, 2026-10-24):** `E-TYP-MATH-CODE` added to the `--strict`
  > list; math.md §9, latex-output.md §8 and typst-dialect.md already escalate it under
  > `--strict` (or `[typst] strict = true`). `math-syntax` and `math-tag-ignored` stay warnings.
- `--watch`: rebuild on change without serving.
- Exit codes: 0 ok · 1 build error · 2 usage/config error · 3 a required external program is
  missing (Chrome engine).
- Stdout: one `Wrote <path> (<detail>, <size>)` line per output (`<detail>`: `9 pages` for PDF,
  `3 files` for an assets directory, nothing for HTML/DOCX). Diagnostics on stderr, warnings
  first, errors last, each with path, line and excerpt (presenter's `report.rs`).

## serve
```
writer serve <INPUT> [--port N] [--host H] [--open] [--view web|pages] [--theme …] [--template …] [--allow-packages]
```
- Serves the `web` view (HTML) and the `pages` view (typeset pages) of the same document; the
  client switches with a button or the `p` key. `--view` sets the initial one; the default is
  `pages` for `.typ` inputs and `web` otherwise (`[serve] view`).
- For a `.typ` input the pages view compiles the user's file; the web view renders the subset
  and reports subset errors as its own target's error while the pages keep updating.
- Each view keeps its last good state when only the other fails (live-reload.md §2).
- Port handling, `--open`, Host-header checks, shutdown: as presenter.

## check
```
writer check <INPUT> [--target LIST] [--deny-warnings] [--format text|json] [--compat presenter]
```
- Runs the frontend, the resolve pass and every target's "can I render this?" pass without
  writing output. `--target` limits the per-target lints (default: the input's default targets).
- Lints (full lists in `markdown-dialect.md` §10, `typst-dialect.md` §7,
  `notebook-mapping.md` §8, `latex-output.md` §8): `:::` lines that did not open or close a
  directive, unclosed directives, unparsed attribute blocks, missing assets,
  duplicate/unresolved/ambiguous labels, invalid labels, unknown citation keys, references to
  unnumbered targets, orphan table captions, misplaced equation labels, unsupported constructs
  per target, math a target cannot convert (incl. a `mitex` pre-flight of LaTeX math), raw
  blocks of unknown formats, typst constructs outside the subset, template slots missing,
  notebooks never run. `--compat presenter` adds the constructs presenter would read
  differently (`directive-colon-rewritten`, includes not between rules, `authors:` lists).
- Exit 0 when there are no errors (and, with `--deny-warnings`, no warnings); 1 otherwise.
  `--format json` prints `[{"level","code","message","file","line","cell"}]`.

> **Coordinator clarification (2026-10-29, T71):** Markdown, notebook and
> Typst inputs run frontend/lenient-resolve checks and the selected target
> passes. Target checks discard generated packages and resource plans; they
> create no output files or sidecars. The DOCX pass uses the actual writer and
> shared lazy compiler World, while HTML only renders visible native/image
> equations. PDF and Typst source use cheap conversion checks, without a full
> document compile. Missing assets remain warnings in check: a deferred DOCX
> missing asset repeats no already-collected warning, and a newly missing asset
> gains a located warning. A failed writer contributes no speculative later
> findings. Genuine template/reference/resource/I/O failures remain errors.
> Configuration and unknown extensions are error diagnostics with exit 1.
> Informational findings are shown in text without `-v` and never fail
> `--deny-warnings`. Quiet JSON remains complete; quiet text hides informational
> notes and warnings, except that warnings causing `--deny-warnings` failure
> are shown. JSON uses `file`, not `path`.

## dump
```
writer dump <INPUT> [--ir | --typst | --latex | --context] [--no-resolve]
writer dump --default-config
```
- `--ir` (default): the `Document` as JSON after the resolve pass (`--no-resolve` for the raw
  parse). `--typst`/`--latex`: the generated source on stdout (assets not written; T32/T61).
  `--context`: the JSON the HTML templates receive. `--default-config`: the built-in
  `writer.toml` (used by `tests/guide.rs`). `--default-config` takes no INPUT. INPUT or any
  other dump flag with it is a usage error (exit 2) and `--config` is ignored. It prints
  `Config::default_toml()` (config.md §4).

  > **Contract change (coordinator, 2026-10-25):** `dump --default-config` split onto its own
  > usage line without `<INPUT>`, as in presenter's CLI spec and as config.md §4
  > (`default_toml()`) and the lifted `tests/guide.rs` invoke it (`writer dump
  > --default-config`); T16 builds the clap tree from this usage (e.g. `INPUT` optional with
  > `required_unless_present = "default_config"`, and `--default-config` conflicting with
  > `INPUT` and every other dump flag).

## init
```
writer init [DIR] [--typst] [--bib] [--force]
```
- Writes `writer.toml`, `document.md` (or `document.typ` with `--typst`), `figures/plot.png`,
  `theme/` (a child theme extending `default`), `.gitignore`; `--bib` adds `refs.bib` and a
  citation to the document. Refuses non-empty directories without `--force`.

## eject
```
writer eject theme <DIR>                    # the built-in HTML theme as a directory (extend or edit)
writer eject typst-template <FILE.typ>      # the PDF template: a .typ exporting `writer-doc`
writer eject typst-package <DIR> [--install]   # DIR/writer/doc/<ver>/ and DIR/preview/mitex/<ver>/ for `typst compile --package-path DIR`;
                                            # --install writes into typst's data directory instead (FsPackages::system_data)
writer eject docx-reference <FILE.docx>     # the built-in reference document with every style writer uses (restyle in Word, then [docx] reference_doc)
writer eject latex-template <FILE.tex>      # the sample BYO LaTeX template
```
- Never overwrites an existing path (exit 2). Prints what was written and the config line to
  use it.

## Global flags
`-v/-vv` raise `tracing` verbosity (`-v` also shows info-level diagnostics such as
`directive-colon-rewritten`), `-q` silences warnings, `--config PATH` picks the `writer.toml`.
`--version` prints version and build id. `--licenses` prints the third-party notices for the
files and data embedded in the binary (`vendor::licenses()`, vendored-assets.md §8.2) to stdout
and exits 0; like `--version` it needs no `<COMMAND>`. Colour on stderr when it is a TTY
(`NO_COLOR` honoured).

> **Contract change (coordinator, 2026-10-24):** `--licenses` added (usage line and this
> section), as vendored-assets.md §8.2 and its Deviations require: T00 writes
> `vendor::licenses()`, T16 wires the flag and its `--help` text (mirrored by `tests/guide.rs`).
