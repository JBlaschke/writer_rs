# T80 — Design layer: schema, merge, resolution, `dump --design`, `eject theme --design`

| | |
|---|---|
| Milestone | M8 |
| Size | L |
| Depends on | T16, T71, T72 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `src/design/{mod,parse,merge,resolve,json}.rs` (new), `src/check/design.rs` (new), `tests/design_config.rs` (new), `tests/fixtures/design/**` (new); additively: `src/config.rs` (`Config.design`, `merge_front_matter`, `DEFAULT_TOML`), `src/theme/manifest.rs` (`ThemeManifest.design`), `src/pipeline/mod.rs` (`Prepared.design`, `resolve_design`), `src/cli/mod.rs` (`--design` flags), `src/cli/dump.rs`, `src/cli/eject.rs`, `src/check/targets.rs` (one call), `docs/guide/configuration.md` and `docs/guide/cli.md` (regenerated fences only) |

## Goal
`[design]` can be declared in `theme.toml`, `writer.toml` and front matter, is deep-merged along
the theme chain and the config layers, validated once with key-path errors, and resolved into
`Design` (points, hex colours, normalised style names) available as `Prepared.design` to every
later task. `writer dump --design` prints it, `writer eject theme --design` writes it into an
ejected theme, and `check` reports missing font files and images. No backend applies it yet.

## Read first
- `docs/spec/design.md` §0–§4, §10 (your spec), §3.3 (the JSON every consumer reads), §12
- `docs/spec/config.md` §1–§4 and its 2026-11-01 notes; `docs/spec/html-output.md` §3.2–3.3
- `docs/spec/cli.md` `dump`, `eject` and their 2026-11-01 notes
- `src/theme/mod.rs` `Theme::vars` / `deep_merge` (the merge to reuse), `src/theme/manifest.rs`
  (`line_of_key`), `src/config.rs` (`merge_front_matter`, `front_table`, `with_unknown_key_hint`)
- `src/pipeline/mod.rs` `prepare`, `resolve_theme`, `Prepared`

## API to implement
```rust
// src/design/mod.rs — design.md §2.3, verbatim
pub struct DesignSpec { … }            // serde, deny_unknown_fields, every sub-table too
pub struct Design { pub declared: bool, pub fonts: Fonts, pub text: Text, pub colors: Colors,
                    pub page: Page, pub styles: BTreeMap<String, Style> }
pub struct Pt(pub f64);  pub enum LineHeight { Multiple(f64), Exact(Pt) }
pub struct Color(pub [u8; 3]);  pub struct Weight(pub u16);  pub enum Align { Left, Center, Right, Justify }
pub struct Layer<'a> { pub table: serde_json::Map<String, serde_json::Value>, pub base_dir: &'a Path, pub origin: Origin }
pub fn merge(layers: &[Layer<'_>]) -> Result<Design>;
pub fn parse_length(text: &str) -> Result<Pt>;
pub fn parse_color(text: &str, colors: &Colors) -> Result<Color>;
pub fn normalise_style_key(name: &str) -> String;
pub fn style_for<'a>(design: &'a Design, attrs: &crate::ir::Attrs) -> Option<&'a str>;
pub fn segments(text: &str) -> Vec<Segment>;
impl Design {
    pub fn to_json(&self, paths: PathMode) -> serde_json::Value;
    pub fn referenced_files(&self) -> Vec<PathBuf>;
    pub fn font_for(&self, role: &FontRef) -> Option<&Font>;
}
pub const TEMPLATE_TOML: &str;          // design.md §4.2, the commented reference block

// src/pipeline/mod.rs
pub struct Prepared { …, pub design: crate::design::Design }
pub fn resolve_design(prepared: &Prepared, opts: &BuildOptions) -> (Design, Diagnostics);   // design.md §1.4

// src/check/design.rs
pub fn problems(design: &Design, targets: &[Target]) -> Vec<Diagnostic>;  // design-font-file, design-image-missing
```

## Steps
1. `src/design/parse.rs`: `parse_length`, `parse_color`, `parse_weight`, `parse_align`,
   `LineHeight` parsing; unit tests for every §2.1 row including the error messages.
2. `src/design/mod.rs`: `DesignSpec` (serde) and `Design` (hand-built) per §2.3; `Serialize` for
   `Design` with `Color` as `"#rrggbb"` and `Pt` rounded to three decimals (§3.3).
3. `src/design/merge.rs`: absolutise path keys per `Layer` (§1.3; a theme path outside its
   directory → `Error::Theme` `theme-link-outside`), deep-merge with `theme::deep_merge` semantics
   (make that function `pub(crate)` and reuse it), remember the last layer per leaf key, then
   `serde_json::from_value::<DesignSpec>` with the unknown-key hint rewritten to the key path and
   the origin's error type (§4.4).
4. `src/design/resolve.rs`: `DesignSpec` → `Design` (§3.1–3.2: colour tokens after scalars,
   furniture defaults, `first`/`rest` resolution, style-key collisions, nested `first`/`rest`).
5. `src/design/json.rs`: `to_json` with `PathMode::{Real, Virtual(&AssetRegistry)}`; a snapshot
   test of the §11 letter design's JSON (`tests/snapshots/design/letter.json`).
6. `src/config.rs`: `Config.design` raw map; `merge_front_matter` deep-merges `design:`
   (`front_table`); `DEFAULT_TOML` gains the commented `[design]` pointer (config.md §2 note) and
   so does config.md §2's schema block (`tests/config.rs` compares them); regenerate the guide
   fence with `WRITER_UPDATE_GUIDE=1 cargo test --test guide`.
7. `src/theme/manifest.rs`: `ThemeManifest.design` raw map (serde default); `Theme::design_layers()`
   returning the chain's `(table, dir, file)` parent first.
8. `src/pipeline/mod.rs`: `resolve_design` per §1.4 (theme manifests only; `design-theme-unavailable`
   for non-HTML targets); `Prepared.design` set in `prepare` after `merge_front_matter`.
9. `src/cli/mod.rs`, `dump.rs`: `--design` (conflicts like `--context`); `eject.rs`: `theme`'s
   `--design INPUT` (§4.2: merged table to TOML, copies under `assets/design/`, rewritten paths)
   and the commented block appended to the ejected `theme.toml`; regenerate the help fences.
10. `src/check/design.rs` and the call in `src/check/targets.rs`: `design-font-file`,
    `design-image-missing` (and the build-time `Error::Config` for both, §4.3).
11. `tests/design_config.rs`: fixtures under `tests/fixtures/design/` — `letter/` (the §11 theme
    with a 1×1 `logo.png`, no font files), `chain/` (parent and child themes merging lists and
    tables), `errors/` (bad unit, unknown key, colliding style names, nested `first`).

## Acceptance criteria
- `cargo test --test design_config`: §11's theme resolves to the snapshot JSON; a child theme's
  `fonts.body.files` replaces the parent's; `writer.toml` `[design.text] size` overrides the theme;
  front matter `design: {text: {size: "12pt"}}` overrides both; a theme path outside the theme is
  `theme-link-outside`; each `errors/` fixture fails with the documented message (exact text).
- `writer dump --design tests/fixtures/design/letter/letter.md` prints the snapshot; a document
  without any `[design]` prints `null`; `dump --design --ir` is a usage error (exit 2).
- `writer eject theme DIR` writes a `theme.toml` whose commented block, uncommented, parses as
  `DesignSpec` (unit test on `TEMPLATE_TOML`); `writer eject theme DIR --design letter.md` writes
  the merged `[design]` and `DIR/assets/design/logo.png`, and `writer dump --design` of a document
  using that theme equals the original's JSON except for paths.
- `writer check` on a fixture naming a missing font file reports `design-font-file` (error) and
  `writer build` of it fails with the same message; the `design-theme-unavailable` warning appears
  for `--to pdf` with a theme that cannot be resolved and does not for `--to html`.
- Every existing snapshot (HTML, Typst, DOCX, LaTeX) is unchanged: no backend reads the design yet.
- `tests/guide.rs` passes (regenerated `--default-config` and help fences).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
`writer dump --design letter.md` prints the §11 design resolved (Source Sans 3, 11 pt, 1.35,
`#242424`/`#a96d4b`, A4 with 50/22/38 mm margins, two header variants, the footer, six styles).
Every output still looks as it does today: the HTML theme's own CSS, Typst's Libertinus defaults,
the Word reference document.

## Out of scope
Emitting anything from the design (T81 HTML, T82 Typst, T83 DOCX), `style_for` callers and
`data-style`/`writer-div` (T84, T85), furniture rendering (T86–T88), guide prose and the example
(T89). `design-style-unused`, `design-font-family` and `design-shadowed` belong to T85, T82, T83.

## Hints (verified)
- `Theme::vars()` / `deep_merge` live in `src/theme/mod.rs` (lines 500–512 and 1027–1038 at the
  time of writing): child wins for scalars and lists, tables recurse. Reuse, do not re-implement.
- `ThemeManifest::line_of_key(path: &[&str]) -> Option<usize>` (`src/theme/manifest.rs`) locates
  a TOML key for `Error::Theme`; `Config::with_unknown_key_hint` (`src/config.rs` ~957) builds the
  "unknown key (did you mean …)" wording from serde's message.
- `front_table(meta, "design")` (`src/config.rs` ~853) returns a front-matter table as a
  `serde_json::Map`; `merge_front_matter` is at ~734.
- `tests/guide.rs` regenerates its fences with `WRITER_UPDATE_GUIDE=1`; `GUIDE_PAGES` lists the
  pages it checks (`configuration.md`, `cli.md` among them).
- `serde_json::Value` deserializes from `toml` tables directly (`toml::Table` → `Value` via
  `serde_json::to_value`), which is how `[vars]` already reaches templates.
- Points: 1 in = 72 pt, 1 mm = 2.834645669 pt; §11's table gives the letter's values to check
  against (50 mm = 141.732 pt).
