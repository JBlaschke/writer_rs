# T84 — Typst emitter keeps div classes: `writer-div` and package 0.2.0

| | |
|---|---|
| Milestone | M8 |
| Size | S |
| Depends on | T80, T82 |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | `src/typst/emit.rs` (`Div` arms, `EmitOptions.design`), `src/typst/package.rs` (`WRITER_DOC_VERSION`), `assets/typst/writer-doc/typst.toml` and `lib.typ` (`writer-div` export, identity body), `src/typst/frontend.rs` (allow-list row), `tests/snapshots/typst/**` (regenerated), `tests/typst_emit.rs`, `tests/typst_roundtrip.rs` (new cases) |

## Goal
The emitter stops flattening classed divs: a `Div` with classes or a `custom-style` becomes
`#writer-div(style: …, classes: (…))[ … ]`, with `style` resolved through `design::style_for`.
`writer-div` is exported by `@writer/doc` 0.2.0 and is the identity until T85 fills it; the typst
frontend maps the call back to a `Div`, so the round trip holds and PDF output is unchanged.

## Read first
- `docs/spec/design.md` §6.4, §8.2 (resolution order), open point 10
- `docs/spec/typst-dialect.md` §1, §2.4, §5.3–5.4, §7 and their 2026-11-01 notes
- `src/typst/emit.rs` (`Block::Div` arms around lines 363–387 and 468–476; the skeleton import at
  262–281), `src/typst/frontend.rs` (allow-listed calls), `assets/typst/writer-doc/lib.typ`

## API to implement
```rust
// src/typst/emit.rs
pub struct EmitOptions<'a> { …, pub design: &'a Design }
// emitted: `#writer-div(style: "Recipient", classes: ("recipient",))[` … `]`
// style = design::style_for(..) → else attrs.pairs["custom-style"] → else none
```
```typst
#let writer-div(style: none, classes: (), body) = body   // T85 replaces the body
```

## Steps
1. `lib.typ`: export `writer-div` (identity); `typst.toml` version `0.2.0`; `WRITER_DOC_VERSION`.
2. `emit.rs`: the three `Div` cases of design.md §6.4 (plain → blocks; `#metadata(none) <id>` when
   labelled; wrapper when classes or `custom-style`); escaping of the style and class strings per
   §5.2; `style` from `design::style_for(opts.design, attrs)`.
3. `frontend.rs`: allow-list `writer-div(style: S, classes: C)[..]` → `Div { classes: C, pairs:
   { custom-style: S } }` (no pair for `none`); `classes` may be a string or an array.
4. Regenerate the Typst snapshots (`tests/snapshots/typst/**`) and the eject-package fixtures that
   embed the version; inspect the diff: only the import version and `writer-div` wrappers change.
5. `tests/typst_emit.rs`: a classed div, a `custom-style` div (with and without a declared design
   style), a labelled classed div, presenter `columns`; `tests/typst_roundtrip.rs`: the wrapper
   survives typst → IR → typst.
6. PDF equivalence: `tests/pdf_pipeline.rs` (or the oracle) shows a document with `:::center` and
   `:::columns` renders the same pages as before (identity wrapper).

## Acceptance criteria
- `writer dump --typst` of `tests/fixtures/md/directives.md` (or the fixture holding classed divs)
  contains `#writer-div(style: none, classes: ("center",))[` and no flattened div; a div with
  `custom-style="Recipient"` under the letter design emits `style: "Recipient"` (the declared name,
  also for `custom-style="recipient"`), and under no design the raw attribute value
  `style: "Recipient"` (design.md §6.4: resolved name, else the raw `custom-style`, else `none`).
- Round trip: `writer-div(style: "X", classes: ("a", "b"))[..]` parses to the `Div` with those
  attrs (`tests/typst_roundtrip.rs`).
- `WRITER_DOC_VERSION == "0.2.0"`; `writer eject typst-package DIR` writes `DIR/writer/doc/0.2.0/`;
  `typst compile` of an exported document against the ejected packages succeeds (`tests/typst_export.rs`).
- PDF pages of the existing fixtures are unchanged (oracle or page-count/text comparison).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## The letter after this task
`writer dump --typst letter.md` shows `#writer-div(style: "Recipient", classes: ("recipient",))[`
around the recipient block and `style: "Closing"` / `"Signature"` around the sign-off; the PDF
looks exactly as after T82 because `writer-div` is still the identity.

## Out of scope
Applying `[design.styles]` inside `writer-div` (T85), furniture (T88), HTML and DOCX (T85).

## Hints (verified)
- `Block::Div` arms in `src/typst/emit.rs`: lines 363–365 (no id → children), 380–387 and 468–476
  (labelled → `#metadata(none) <id>`); the skeleton writes `#import "@writer/doc:{WRITER_DOC_VERSION}": *`
  at ~262–281.
- Typst single-element arrays need the trailing comma: `("recipient",)`; an empty array is `()`.
- `tests/typst_roundtrip.rs` and typst-dialect.md §2.4 list the allow-listed calls; `writer-div`
  joins `writer-doc`, `callout`, … there. `classes:` arrives as `Array` or `Str` literals (§2.5).
- The package files are embedded by `crate::vendor` (vendored-assets.md §5); the version appears
  in `assets/typst/writer-doc/typst.toml`, `src/typst/package.rs` and the eject tests.
