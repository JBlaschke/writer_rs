# T89 — Design layer guide and the letter example

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T86, T87, T88 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `docs/guide/customization.md` (new section "Cross-format design"), `docs/guide/configuration.md` (`## [design]` section; regenerated fence), `docs/guide/outputs.md` (PDF and Word paragraphs), `docs/guide/typst.md` (`writer-design`, `writer-div` notes), `examples/letter/**` (new), `tests/guide.rs` (`EXAMPLES` entry), `README.md` (one line in the feature list) |

## Goal
Users can learn the design layer from the guide alone: what `[design]` is, where to put it, how
tokens, named styles and furniture map to HTML, PDF and Word, how escape hatches interact, the
`check`/`dump`/`eject` workflow. `examples/letter/` is a complete letterhead project that builds to
all three targets in the guide tests.

## Read first
- `docs/spec/design.md` (all; §11 is the example to turn into prose)
- `docs/guide/customization.md`, `configuration.md`, `outputs.md`, `typst.md` as left by T74;
  `tests/guide.rs` (`GUIDE_PAGES`, `EXAMPLES`, fence regeneration); `examples/basic/README.md`
  for the example layout

## API to implement
None (documentation and example files). `examples/letter/`: `writer.toml`, `letter.md`,
`theme/theme.toml` (the §11 design), `theme/styles/letter.css` (screen sheet only),
`theme/assets/logo.png` (a neutral placeholder, not an institution's mark), `README.md`.

## Steps
1. `customization.md`: section "Cross-format design" — the three parts with one short example
   each, the merge and precedence rules in prose, the escape-hatch table (theme CSS, template,
   reference document), the `data-style`/`writer-div`/`custom-style` equivalence, fonts per backend,
   `check` codes, `dump --design`, `eject theme --design`.
2. `configuration.md`: `## [design]` with the key table (type, unit, default) and front-matter
   `design:`; regenerate the default-config fence (`WRITER_UPDATE_GUIDE=1 cargo test --test guide`).
3. `outputs.md`: PDF paragraph on `writer-design` defaults and font files; Word paragraph on
   generated styles, section and header/footer parts and what a reference document still overrides.
4. `typst.md`: reading `sys.inputs.writer-design` in a template, `writer-div` in emitted source.
5. `examples/letter/`: the project (fonts named, not shipped — the README shows the `files` lines
   to add), `README.md` with the three build commands and what to look for; add `"letter"` to
   `EXAMPLES` in `tests/guide.rs`; add the example to any site/example index T75 generates.
6. Cross-check every claim against the implemented behaviour (build the example to all targets;
   `dump --design` output in the README is pasted from a real run).

## Acceptance criteria
- `cargo test --test guide` passes: fences regenerated, `examples/letter` builds to `html`, `pdf`
  and `docx` (with `system_fonts = false` the PDF falls back to the embedded fonts, which the
  README says).
- `docs/guide/configuration.md` documents every key of design.md §2.2 with the same defaults (a
  drift test over the key list, like `documented_defaults`, if cheap; otherwise a reviewer check
  listed in the PR).
- The site build (T75's workflow validation, `tests/release/*`) accepts the new page content and
  example.
- A reviewer can produce the letterhead from the guide alone, without reading `design.md`.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
`examples/letter/` is the Letters template re-expressed: one `theme.toml` replaces `letter.css`'s
tokens and print rules, `document.html`, `reference.docx` and `letterhead.typ`; `writer build`
yields the same letter in HTML, PDF and Word, and the guide explains each line of it.

## Out of scope
New features; open points of design.md (each is a possible follow-up task).

## Hints (verified)
- `tests/guide.rs`: `GUIDE_PAGES` (ten pages) and `EXAMPLES` (`basic`, `notebook`, `mixed`,
  `typst`, `latex-template`) are the lists to extend; `WRITER_UPDATE_GUIDE=1` regenerates fences.
- `docs/guide/customization.md` sections today: "Extend the HTML theme", "Generated Typst PDF
  template", "Portable Typst packages", "Word styles and page setup", "LaTeX template", "Resource
  destinations" — the new section goes first, since the others become escape hatches of it.
- Source Sans 3 is OFL-licensed; the example does not ship it (repository size), it names it.
