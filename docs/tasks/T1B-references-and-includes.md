# T1B — References, citations and document includes

| | |
|---|---|
| Milestone | M1 |
| Size | M |
| Depends on | T12, T16, T18 |
| Files owned | `src/markdown/refs.rs` (replace stub), `src/markdown/include.rs` (complete expansion; preserve T16 watcher scan and T18 hook APIs), `examples/mixed/**` (new); `tests/include.rs` (new, adapted from presenter), reference scanner unit tests, `tests/fixtures/ipynb/includes.ipynb` (new), corresponding snapshots |
| Additive integration | `src/markdown/{mod,walker}.rs` at the existing scanner/include seams; existing Markdown/notebook snapshot harnesses and affected snapshots; notebook context plumbing only if the coordinator chooses it below |

## Goal and binding sources

Markdown and notebook fragments produce unresolved `Ref`/`Cite` IR with the
syntax of `markdown-dialect.md` §7. Top-level `:::include` expands Markdown and
notebooks in place, continuing one canonical cycle/depth chain and preserving
per-file spans, assets, diagnostics and footnote scope.

Binding: `docs/spec/markdown-dialect.md` §2.3, §4.9, §5.4, §7–8, §10–11;
`docs/spec/notebook-mapping.md` §2.4–2.6 and §7.1–7.2;
`docs/spec/references.md` §4.5–4.7; `docs/spec/ir.md` invariants;
`PLAN.md` §3.2, §6–7. Resolve/classification remains T17; bibliography rendering
is T70, `check` is T1C, Typst includes are T53.

## Frozen API and integration constraints

Keep these existing public APIs and `Fragment`'s public fields unchanged:

```rust
markdown::parse(source: &SourceInfo, text: &str, config: &Config) -> Result<Parsed>
markdown::parse_blocks(text: &str, config: &Config, ctx: &mut Fragment<'_>) -> Result<Vec<Block>>
markdown::parse_inlines(text: &str, config: &Config, ctx: &mut Fragment<'_>) -> Result<Inlines>
markdown::include::scan_includes(path: &Path) -> Vec<PathBuf>
markdown::include::MAX_DEPTH: usize = 8
markdown::include::cell_includes(notebook: &SourceInfo) -> Box<dyn notebook::CellIncludes>
markdown::include::cell_includes_in(notebook: &SourceInfo, chain: &[PathBuf]) -> Box<dyn notebook::CellIncludes>
```

`CellIncludes::expand(path, span, notebook, effective_config) -> Result<Parsed>`
is T18's existing hook. `chain` contains canonical ancestors, outermost first,
excluding the notebook itself. The hook's result must include its immediate
file in `Document.includes`, because T18 merges the returned list directly.

Scanner/context helper names are private implementation choices; add no IR,
config or dependency changes. Comrak `Text` scanning is an expressly permitted
choice (§7 Open points), and is recommended because the walker currently loses
`Escaped` provenance when creating IR. Preserve escape and sibling boundaries;
never add hidden sentinel characters to author text.

## Implementation

1. Implement the scanner in `refs.rs`, call it while the walker still knows
   `Text`, `Escaped`, previous siblings and Link ancestry. Apply it to all
   paragraph/heading/table/caption/title/footnote inlines through the shared
   fragment APIs. Skip Link descendants and code/math/raw nodes. Respect
   `[markdown] references = false`. Avoid scanning constructed IR twice.
2. Scan only valid starts (§7.2); use the maximal Unicode XID-continue plus
   `_ - : .` target, retaining trailing punctuation as text. Preserve the three
   supplement cases: absent → `None`, `[]` → `Some([])`, text → one Text inline.
   `.page` yields `RefForm::Page`; cite-only modes yield one-item `Cite` with
   the bracket text as locator. Parse bracket groups, prefixes and locators;
   `[-@key]` selects Year. Exactly one permitted mode class is valid; invalid
   mode attrs remain text and warn once at the enclosing span.
3. Lift presenter's include path, context, read/error conversion, span tagging,
   differing-metadata and assembly helpers, adapting Deck/Slide to Document/
   Block. Discard slide-position restrictions and slide-only ignored attrs.
   `include_path` and its tests already exist; do not duplicate them.
4. Expand an include wherever it is a direct Document child. Resolve local
   quoted/absolute paths against the including file's directory; accept
   `detect_kind`'s Markdown (`.md`, `.markdown`) and Notebook only. Missing,
   unreadable, invalid UTF-8, unsupported type, URL, cycle and depth failures
   are `Error::Parse` at the directive. Preserve errors originating inside the
   included file at that file. Nested includes are dropped with an **error**
   diagnostic `include-not-top-level` (including notebook cells and abstract).
   Leaf body behavior follows §5.4: include content, warn `invalid-directive`,
   then emit its non-empty body. Validate closure like existing directives.
5. Use one private include context for `.md → .ipynb → .md` recursion.
   Root is depth 0; include depth 8 is allowed, depth 9 fails. Compare canonical
   paths for cycles; format readable chains without platform-specific roots.
   Preserve private context through nested Markdown parse and `cell_includes_in`.
   `parse_blocks` remains include-free except for reporting nested misuse.
6. Included Markdown inherits the including effective config, applies its own
   front-matter config to parsing, and suppresses **automatic** TOC insertion
   even if its own front matter enables TOC. Explicit `:::toc` stays. Included
   notebooks use `parse_with(... nested: true, Selection::default())`, never
   `parse`. Included metadata does not replace parent metadata; warn about
   differing explicitly set metadata keys.
7. Merge assets/features/diagnostics. Tag every included block's span and inline
   Ref/Cite spans with its absolute file path only when no deeper file is
   already set. Keep diagnostic paths; preserve file-scoped named footnotes.
   Deduplicate `Document.includes` in first-use order, including transitive
   files. Preserve repeated inclusion of content; lift presenter's diagnostic
   deduplication for a file included twice.
8. Retain the watcher scan's best-effort behavior: missing targets are listed,
   unreadable files do not fail, root forced-Markdown input works with any
   extension, first-use order is deterministic. Extend notebook traversal as
   decided below so failed mixed trees remain watchable.
9. Update reference/include snapshots and any notebook snapshots changed by
   newly parsed `@` syntax. Add mixed example with root Markdown, nested part,
   notebook, images and cross-file references; saved notebook output is read,
   not executed. README documents building it.

## Acceptance

- Scanner unit tests cover start/whitespace/break/punctuation boundaries,
  Unicode, trailing dot/colon, empty/malformed targets, `\@`, `\@@key`, text
  following escaped siblings, email/autolinks, excluded Links/code/math/raw,
  absent/empty/nonempty supplements, every mode, invalid attrs, bracket groups
  (`[@a; @b, p. 12]`, `[see @a]`, `[-@a]`) and disabled references. Assert exact
  syntactic IR, spans and diagnostics; bibliography lookup is not performed.
- `tests/include.rs`: includes first/last/between paragraphs and headings;
  Markdown↔notebook nesting; quoted spaces and absolute paths; included asset
  bases; parent effective config and child parsing overrides; child front
  matter cannot insert an automatic TOC; child metadata ignored only when
  different; per-file note scope and global label collisions; repeated content,
  one diagnostic set and ordered watch list; every failure kind and its
  path/line; depth 8/9; symlink cycle test gated on Unix.
- Nested include misuse is an error diagnostic in list, quote, directive,
  footnote, abstract and notebook cell. Non-empty leaf bodies survive after
  expansion with `invalid-directive`.
- `includes.ipynb`: include `../md/include/part-a.md`, notebook/file `[^1]`
  coexist as two footnotes, nested misuse and a cycle are independently tested
  (do not hide all assertions behind the fixture's first fatal cycle).
- Every produced Document satisfies IR invariants. Included spans/file paths,
  diagnostics and first-use include order are checked structurally. Snapshot
  redaction normalizes Windows separators, canonical prefixes and asset IDs
  by logical content order before asset inventory order.
- Existing Markdown and notebook fixture snapshots are reviewed, not blindly
  blessed. `examples/mixed` builds through the CLI; notebook example remains
  clean after resolve now that its reference is active.
- Serial repository checks: fmt, Clippy all targets/features with warnings
  denied, debug/release tests, rustdoc warnings denied and basic/mixed CLI
  builds. Root creates a `codex/` milestone branch and PR against `main`, lists
  unmerged dependencies/merge order, then continues while CI runs.

## Lift sources and existing seams

- `../presenter_rs/src/markdown/include.rs`: Context/expand, parse-included,
  assemble/tag-span, read-included/in-included-file, differing-meta/shown/
  canonical, scan-includes; retain/adapt relevant `tests/include.rs` tests.
- There is **no** presenter `src/markdown/refs.rs`; this scanner is new.
- Writer `markdown/mod.rs` has explicit scanner comments; `walker.rs` has
  `include_directive` stub and currently flattens `NodeValue::Escaped`.
- Writer include module already owns path parsing/MAX_DEPTH/watcher scan.
  T18 notebook conversion already splits top-level include directives, merges
  hook results, preserves cell lines and handles leaf bodies. Promote its
  existing nested-misuse placeholder from warning to error when T1B lands.

## Coordinator decisions (2026-10-27)

1. **Private include context transport:** use internal parse/walker context
   arguments, keeping public `Fragment` literals unchanged. Private helper
   signatures are implementation choices in the owned seam files.
2. **Notebook metadata comparison:** frozen `CellIncludes::expand` receives
   SourceInfo/config but no including DocMeta, while §8 requires comparing
   included metadata with it. Add `set_parent_meta(&DocMeta)` with a default
   no-op body; T18 calls it after effective metadata construction. Custom
   hooks remain source compatible. Do not reconstruct metadata by rereading
   SourceInfo.path: parse JSON may be supplied in memory.
3. **Ignored config-key warnings:** exclude configuration tables from
   `include-meta-ignored`, following presenter. They affect child parsing;
   compare explicitly set document metadata, including explicit empty lists.
4. **Watcher notebooks:** descend into Markdown/raw-Markdown notebook cells
   when `detect_kind` identifies a notebook. Keep forced-Markdown roots with
   arbitrary extensions working. No new public API is needed.
5. **Page mode on bracket groups:** §7.2 says a mode after a group applies to
   the group, but §7.3 requires bracket groups to become `Cite`, and `CiteMode`
   has no Page variant. Bare `@x{.page}` is unambiguous. Keep
   `.page` specific to bare references and treating `[@a; @b]{.page}` as an
   invalid group mode (warning, attrs kept as text).
6. **Negative group items:** any `-@` item selects the group's Year mode
   unless a valid explicit group mode overrides it. The IR has one mode for
   the whole group, so this rule is deterministic without a shape change.

Starts on checked T18 (`b523e05`) while PR #17 runs CI. The PR targets `main`
and records that dependency and merge order if it is still open.

## Implementation amendments (coordinator, 2026-10-27)

- The scanner runs on comrak Text nodes in the existing inline walker;
  escaped and Link ancestry remains available, with no sentinel characters
  or second scan over constructed IR. Shared fragment parsers cover notebook
  cells/outputs, captions, titles and footnotes.
- Markdown's private parse/context seam and T18's default metadata setter
  preserve the public Fragment fields and existing custom include hooks.
  Notebook directive paths now use the same raw-info validation as Markdown;
  errors retain the original cell/line, and short/unclosed fences warn once
  while preserving their non-empty leaf body.
- Include access follows the OS path before canonicalization, including
  directory symlinks followed by `..`. Watcher paths preserve their earlier
  lexical spelling when equivalent, and use the actual target otherwise.
  Its existing depth boundary and best-effort behavior remain covered.
- The snapshot harnesses now support fatal include errors and per-file
  spans/diagnostics. Paths include Windows canonical-prefix normalization;
  assets are labeled by content order. The two obsolete diagnostic snapshots
  for the now-fatal cycle fixture are removed.
- Reviewed snapshots change references, numbering/math references, notebook
  footnote references and includes. New mixed-chain snapshots exercise a
  Markdown → notebook → Markdown include. The mixed example retains saved
  computation output; no notebook was executed.
- No dependency, public IR/config or module-list changes. Cite library lookup
  and rendering remain T70; `check` is T1C and Typst includes remain T53.
