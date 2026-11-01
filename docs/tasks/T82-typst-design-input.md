# T82 — Typst `writer-design` input, `writer-doc` defaults and design fonts

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T80 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `tests/typst_design.rs` (new), `tests/snapshots/typst/design-*.{typ,json}` (new); additively: `src/typst/world.rs` (`world_inputs(config, design)`, `WorldOptions.font_files`), `src/pipeline/pdf.rs`, `src/pipeline/typst_source.rs`, `src/serve/*` call sites of `world_inputs`, `assets/typst/writer-doc/lib.typ` (`from-design`, `writer-design`, the §6.2 defaults), `src/check/design.rs` (`design-font-family`, `design-shadowed` for templates) |

## Goal
The World carries `sys.inputs.writer-design` whenever a design is declared and loads the declared
font files; `writer-doc` takes the design as the middle rung between explicit arguments and its
built-in defaults (fonts, size, leading, spacing, justification, text and link colour, background,
paper, margins). BYO templates and native documents can read the same input.

## Read first
- `docs/spec/design.md` §6.1–6.3, §6.5–6.7, §3.3 (virtual paths), §10 (`design-font-family`, `design-shadowed`)
- `docs/spec/typst-dialect.md` §4.2, §4.6, §6.1–6.3, §6.6 and their 2026-11-01 notes
- `src/typst/world.rs` (`world_inputs`, `set_inputs`, `WorldOptions`, font loading lines ~140),
  `src/pipeline/pdf.rs` (`load_with_diagnostics`), `assets/typst/writer-doc/lib.typ` (`from-config`, `writer-doc`)

## API to implement
```rust
// src/typst/world.rs
pub fn world_inputs(config: &Config, design: &Design) -> BTreeMap<String, String>;   // adds "writer-design" only when design.declared
pub struct WorldOptions { …, pub font_files: Vec<PathBuf> }                            // loaded after font_paths, before system fonts
```
```typst
// assets/typst/writer-doc/lib.typ
#let writer-design = …            // decoded dictionary or (:)
#let from-design(key, default) = …
```

## Steps
1. `world_inputs(config, design)`: insert `writer-design` = `design.to_json(PathMode::Virtual(registry))`
   compact when declared; update every caller (`from_config`, `WorldSlot::get`, `load_typst_main`,
   the oracle test) so nobody serializes by hand; register furniture images in the registry first.
2. `WorldOptions.font_files` and the load order (read each file, parse `Font::iter(Bytes)` and
   register lazy `FontPath` entries with each face's actual collection index); callers pass
   `design.font_files()`. A test verifies that a single file adds usable faces to the World.
3. `lib.typ`: `writer-design`, `from-design`; apply §6.2's table inside `writer-doc` (explicit
   argument → design → built-in), including the leading formula and `set page(margin:)` per side.
4. Template documentation comment in `template.typ` showing how a non-delegating template reads
   `sys.inputs.writer-design` (design.md §6.3 snippet).
5. `src/check/design.rs`: `design-font-family` for `--target pdf` (build a World with the design
   files and ask `book().contains_family`); `design-shadowed` when `[pdf] template` is set, the
   design is declared and the template's text does not import `@writer/doc`.
6. `tests/typst_design.rs`: the letter fixture's `sys.inputs` map (snapshot JSON, keys in §3.3
   order; the key absent without a design); a PDF build with `system_fonts = false` and an embedded
   font's bytes written to a fixture `.ttf` under a different family name proves the file is loaded
   (`typst query` or the oracle: a `text(font:)` that resolves); a template delegating to
   `writer-doc` gets the design's margins (compare page geometry through the PDF oracle of §8.3).

## Acceptance criteria
- `tests/typst_design.rs`: `world_inputs` has exactly `writer-version`, `writer-config`,
  `writer-design` for the letter and no `writer-design` for `tests/fixtures/md/basic.md`; the
  letter's PDF has A4 pages with the text block starting 50 mm from the top (oracle query of the
  first paragraph's position, ±1 pt) and uses the design's font when its files are given;
  `writer check --target pdf` on a fixture naming an unknown family without files warns
  `design-font-family`; with `[pdf] template` not importing `@writer/doc`, `design-shadowed`.
- `tests/typst_snapshots.rs`, `tests/pdf_pipeline.rs`, `tests/typst_world.rs` unchanged (no design
  → same inputs, same Libraries: `library_builds` stays 1 across a rebuild with an unchanged design).
- `set_inputs` after editing an unrelated `writer.toml` key keeps the Libraries (`library_builds`
  unchanged) while a design edit rebuilds them once.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
`writer build --to pdf letter.md` without a template produces Source Sans 3 11 pt with 0.55 em
leading, 3.6 mm paragraph spacing, unjustified, ink `#242424`, A4 with 50/22/38 mm margins. The
page still shows `writer-doc`'s centred page number and no letterhead (T88), and `:::recipient`
has no special spacing (T84, T85). `letterhead.typ` can already be reduced to a delegating template.

## Out of scope
`writer-div` and the emitter (T84), named styles (T85), furniture (T88), the package version
bump (T84), exports copying fonts (open point 5).

## Hints (verified)
- `typst_kit::fonts::scan(path: &Path)` (`typst-kit-0.15.1/src/fonts.rs` line 163) delegates to
  `fontdb::Database::load_fonts_dir` and only accepts directories. For individual files use
  `typst::text::Font::iter(Bytes)` and register `FontPath { path, index: font.index() }` with the
  face's metadata. Preserve the actual index: enumerating filtered faces can lose TTC indices.
- `FontBook::contains_family(&self, family: &str) -> bool` (`typst-library-0.15.1/src/text/font/book.rs`
  line 54) expects a lowercase lookup key; the World exposes `book()`.
- `typst_kit::fonts::embedded()` yields `(Font, FontInfo)`; `Font::data()` gives the bytes to write
  a fixture font file (no binary fixture needs to be committed).
- `from-config` in `lib.typ` shows how `writer-config` is decoded (`json(bytes(..))` in 0.15;
  `json.decode` is deprecated).
- Leading: design.md §6.2 — `leading = (n − 0.8) × size`; the letter's 1.35 gives the hand-tuned
  0.55 em of `letterhead.typ`.
- `set_inputs` rebuilds the Libraries only when the map differs (`src/typst/world.rs`, `library_builds`
  counter is already observable in `tests/typst_world.rs`).
