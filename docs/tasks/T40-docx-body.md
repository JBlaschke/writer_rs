# T40 — DOCX body, bookmarks and internal links

| | |
|---|---|
| Milestone | M4 |
| Size | L |
| Depends on | T04, T13, T17 |
| Parallel-safe with | T30, T31 |
| Files you own | `src/docx/{body,inline,footnotes}.rs` (replace stubs), additive `src/docx/mod.rs`, `tests/docx_snapshots.rs`, `tests/fixtures/docx/ir/{blocks,lists,code,callouts,footnotes,refs,cells}.json`, `tests/snapshots/docx_snapshots__*.snap` |

## Goal

Render body paragraphs and rich runs, metadata, headings, lists, code,
callouts and saved notebook cells to Word XML. Preserve source labels,
forward references, repeated footnotes and nested note numbering.

## Read first

- `PLAN.md` §3–§4, §5 T40 and §6; `AGENTS.md`.
- `docs/spec/docx-output.md` §1.1, §2, §4.1–§4.9, §5, §7 and Fixtures.
- `docs/spec/references.md` §1.2, §1.4, §4 and §5.5.
- `src/docx/{mod,styles,numbering,package,validate}.rs`, `src/ir.rs`,
  `src/html/highlight.rs` and `src/markdown/code.rs`.

## Steps

1. Precompute bookmarks and note IDs from emitted content, choosing only
   visible notebook outputs for DOCX. Preserve first logical label ownership
   even when footnote bodies are emitted after the document.
2. Implement contextual paragraphs, title metadata, heading styles and
   numbers; accumulate inline formatting and merge identical adjacent runs.
3. Implement list instances, continuation indentation, tasks, definitions,
   code scope mapping, line numbers, highlights and plain output blocks.
4. Render callouts, custom styles, cells/prompts and validated raw XML.
5. Emit bookmarks, internal/external links and page/reference fields;
   delegate rich figures, tables, images, TOC and math to their existing stubs.
6. Emit footnote parts, hoist nested references, create collision-safe note
   bookmarks and use part-local hyperlink relationships.
7. Snapshot every package part and docx-rs readback for seven IR fixtures
   and the sample document. Validate every generated package.

## Acceptance criteria

- All §4.1–§4.9 behavior has structural assertions or reviewed snapshots.
- Paragraph and run property order passes the existing validator; repeated
  output is byte-identical, with and without reference-file reuse.
- Forward/missing/page references, selected cell candidates, note scoping,
  nested notes and label collisions are covered.
- Raw tables have boundary/trailing paragraphs; invalid raw XML is dropped
  with a diagnostic carrying the active source location.
- Formatting, all-target/all-feature Clippy, debug/release tests, rustdoc
  with warnings denied and the basic HTML example pass, locked and offline.

## Out of scope

T41 retains figures, captions, tables, images, TOC, bibliography and DOCX
pipeline wiring. T42 retains equations. T18 adds ANSI output runs. T43 owns
reference ejection and Word/LibreOffice smoke testing.

## Coordinator notes

- This brief records the frozen T40 contracts missing from the original
  task directory. Starts from merged PR #14 (`223fd42`); dependencies done.
- Add crate-private `Emitter.content` state for the prepass, paragraph/list
  context and deferred notes. `write` remains the construction entry point.
  No IR, config, dependency or module-list changes.
- Keep T04's whole-IR unknown-asset validation, including unused notebook
  candidates. The content prepass independently selects only emitted notes
  and bookmarks. Future T41/T42 renderers extend that prepass when replacing
  their plain-text stubs.
