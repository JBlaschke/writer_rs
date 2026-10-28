# T50 — Structured Typst syntax frontend

Frozen implementation brief, coordinator, 2026-10-28.

| | |
|---|---|
| Milestone / size | M5 / L |
| Depends on | T01, T17, T30, all implemented |
| Primary ownership | `src/typst/frontend.rs` (replace T50 stub and its stub test) |
| Optional private split | private children under `src/typst/frontend/`, only after coordinator ownership note; no public module-list change |
| Tests / fixtures | `tests/typst_frontend.rs`, `tests/fixtures/typ/**`, `tests/snapshots/typst_frontend__*.snap`; private walker tests |
| Proposed additive hooks | remove T50-only Typst-check guard in `src/cli/check.rs` and update its stub test; make T23 private Typst-prepare-failure test inject failure instead of relying on T50 stub |
| Binding sources | PLAN §3.2/M5 T50/§6–7; typst-dialect.md §2–3, §7, §11, Fixtures/Deviations; ir.md §1–4; references.md §1/§4; config.md §1/§3; math.md §6/§9 |
| Reference | R2-typst-syntax-subset.md typed AST/API/tree probes §2–3, §9; its §7 proposal yields to frozen spec |

## Goal and scope

Parse the explicitly supported Typst syntax into the existing structured IR,
without evaluating user code, invoking an external Typst executable, compiling a
PDF, fetching packages or executing imported modules. Preserve full Typst math
source for downstream emitters. Unsupported syntax is located diagnostic data,
not a panic or a partially evaluated approximation.

T50 unblocks T32's PDF-pipeline work, then T24 actual pages. The fixture numbering
oracle is consumed by T32. Full `.typ` build/serve/math end-to-end and the normalized
Markdown→Typst→IR golden remain T52; per-target check passes remain T71; `.md`/
`.ipynb` include implementation and compiler provider body remain T53. Existing
pipeline dispatch already invokes this frontend; do not gate that useful result
behind another milestone stub. No IR/config/dependency/public API redesign.

## Public API and assembly

Keep exactly:

```rust
pub fn parse(source: &SourceInfo, text: &str, config: &Config) -> Result<Parsed>;
```

Use typst-syntax0.15.1 Source::detached(text), root Markup and typed ast nodes.
No evaluator/World dependency in the frontend. AstNode offsets from LinkedNode
or summed child len are local choices. Source/Lines handle byte→line/column,
including Unicode; avoid treating byte count as display/source column.

Assemble Document with source.clone, default unresolved fields, derived Features,
registry.infos(source.base_dir), first-use transitive includes and structured
blocks. Every produced block has a span; root file follows normal frontend
convention, included blocks explicitly carry their included absolute file.
Ref spans and every diagnostic preserve owning source. SourceSpan line bounds
are1-based, cellNone for Typst itself. Reuse ir::plain_text, Features::merge,
AssetRegistry and Document::invariant_violations in tests.

Frontend clones config and applies Config::merge_front_matter(meta), then uses
that effective config for TOC defaults, macro stripping, strictness and includes.
Pipeline will merge metadata again idempotently. Metadata extraction must respect
source order without allowing a later literal let to become visible backwards.
No changes to caller's Config; only file stat and included-source reads allowed.

## Failure/diagnostic policy

- Every actual parser error -> E-TYP-SYNTAX; return Ok(Parsed) with blocks empty.
  Emit parser warnings W-TYP-SYNTAX too. Parser warnings alone still build IR.
  Strictly escalated syntax warnings do not masquerade as parser errors and
  should not force an otherwise parseable tree's blocks empty.
- Every diagnostic has file/span and message ending ` (col N)` using source
  range start, while span end covers the diagnosed construct. Keep original
  SyntaxDiagnostic message/hints; choose consistent hint separators before the
  final column suffix. Paths are redacted only in snapshots, not production.
- Errors E-TYP-* are Level::Error, W-TYP-* Warning; effective typst.strict or
  CLI --strict raises W-TYP-* to Error without changing code. Do this before
  downstream subset_error checks renderability; pipeline's final escalation
  happens after target execution and cannot be its only enforcement point.
- Syntax/subset/argument/include failures return collected Parsed diagnostics
  and keep supported surrounding content except the parser-error empty-tree
  rule. Frontend should not return Error::Other for unsupported user syntax.
  Genuine failure of Config::merge_front_matter follows its established Result
  contract unless coordinator freezes finer malformed-argument handling below.
- Pipeline::subset_error already finds the first Error E-TYP-/W-TYP- and fails
  HTML/DOCX/LaTeX; PDF `.typ` compilation ignores subset IR in T52. Do not change
  pipeline target error policies here. Resolver owns duplicate/unresolved/
  ambiguous labels, citation failures and numbering after frontend assembly.
- Math `#` expressions remain verbatim. E-TYP-MATH-CODE is math.md's LaTeX-only
  warning (T51/T60/T71), not a new fatal frontend diagnostic.

## Walker semantics — §2.2/2.3 are binding

- Markup is flat. Maximal inline runs between paragraph breaks and block nodes
  form paragraphs; trim edge spaces, merge adjacent Text, drop empty paragraphs.
  Handle Expr::Space through Markup::exprs (ordinary node.cast<Expr> loses it);
  Markup::exprs also drops Space after statements, as R2's probe documents.
- Flush paragraphs at Heading, list/enum/term items, display Equation/Raw,
  ModuleInclude, and the exact block-call allow-list in §2.2. Block-vs-inline
  quote/raw/math.equation obey arguments. Statements/comments/semicolon/hash
  contribute no text or paragraph themselves.
- Text/Escape/Shorthand map decoded values; newline Space -> SoftBreak, other
  Space -> one space. Linebreak/linebreak() -> LineBreak and drops adjacent Space.
- SmartQuote uses the frozen port of SmartQuoter::quote, one stack per paragraph,
  including prime-after-digit, apostrophe-after-letter/object, punctuation and
  closing-quote rules. Refer directly to installed typst-library0.15.1 source;
  preserve license attribution if porting code. Do not use a naive alternating
  quote substitution. Strong/Emph recursively walk inline content.
- Raw.text joins dedented Raw.lines with LF; preserve lang. Inline Raw -> Code
  with lang class, block Raw -> CodeBlock with lang. Display/inline equations
  use trimmed math body full_text, dialectTypst; do not convert/evaluate math.
- Auto Link, Ref optional supplement None vsSome(empty), heading level clamp6
  with W-TYP-ARG-IGNORED, adjacency grouping/tightness/start for lists and terms
  follow §2.2. Tight list must be loosened when block contents violate IR rules.
- Labels attach to nearest preceding nonspace/noncomment sibling in the same
  Markup even across Parbreak; track the logical source node, not just the last
  emitted paragraph. Start orphan warns. Unsupported target label warnsignored.
  Every listed attrs/footnote target in §2.3 works. metadata(none) labelled alone
  becomes empty Div, mixed inline becomes empty Span. Validate label(Str) through
  typst_syntax::is_valid_label_literal_id; invalid -> label-invalid, not E-TYP.
- Footnotes are definitions once plus FootnoteRef for repeats, writer-fn- prefix
  stripped exactly once. Post-walk fixup handles forward @footnote references,
  including included Typst files, without turning ordinary @labels into cites.
  Ref/Cite.resolved and numbering fields remainNone until resolve.

## Calls, arguments and scope — full §2.4 table required

Implement every row, including package helpers. Do not simplify ownership of a
feature by omitting calls already emitted by T30. Callee Ident/FieldAccess may
resolve through a recognized import alias. Argument collection preserves trailing
ContentBlocks as positional args. Missing/wrong required shape E-TYP-ARG-INVALID,
spread E-TYP-ARG-SPREAD, unknown callee E-TYP-CALL-UNKNOWN. Unknown named args
warn W-TYP-ARG-IGNORED; only the table's explicit silent layout args are silent.

- Literals: Str/Int/Float/Bool/none/auto/Numeric/Label/ContentBlock, alignment
  idents/+ combinations, unary negative numbers, arrays/dicts recursively,
  path(Str), label(Str), datetime(year:,month:,day:), prior literal-let idents,
  and nested allow-listed calls exactly where expected. No binary evaluator.
- `let` bindings scope to enclosing Markup/content block; parent visible to
  children, child/shadow bindings do not leak. Closures/destructuring warnignored,
  later calls unknown. A plain #ident substitutes scalar Str/Int/Float or walked
  content; unknown -> E-TYP-IDENT-UNKNOWN. Other unsupported code dropped with
  E-TYP-CODE-UNSUPPORTED, including field access outside recognized shapes.
- Import only recognizes @writer/doc and @preview/mitex paths (items/*/aliases),
  warns mismatched versions against exported constants. Other imports warnignored
  and do not load a file/network module; imported callable names stay unknown.
  writer-doc show rule name matching also accepts BYO-template module names as §2.6
  explicitly requires. Do not restore R2 proposal's lorem or figure-grid support.
- Figure explicitkind image/table/raw, stringOther+W-TYP-FIGURE-KIND; inferfrom
  direct body or single child content. Equation body ->Other. Enforce IR exact
  body shape or downgradeOther; only Other figures can contain nested figures.
  numbering:none adds unnumbered; caption optional, refs/numbers left resolver.
- Image Str/path literal resolves relative including file; `/x` means initial
  World/project root. Stat/register local file, `asset:id`, dimensions Numeric
  pt/mm/cm/in/em/% preserved as attrs pairs, auto omitted, altplainStr. Missing
  stays original URL plus missing-asset warning; never a frontend terminalerror.
- Tables: columns Int or tracks auto/%/all-fr ratios; horizontal align scalar/
  per-column/+vertical extraction; header/body/footer sections; row-major span
  occupancy, pad last row, cells>=1 spans and exact IR grid persection. Flatten
  multi-paragraph cells with LineBreak, drop other blocks with W-TYP-TABLE-CELL.
  A table.cell x/y warns. hline/vline and spacing calls silent. Section rowspan
  does not leak into nextsection. Validate integer counts without truncation.
- Wrappers, inline formatting, quotes, links, heading attrs, metadata anchors,
  lists/terms, raw(), unnumbered math.equation shape, callout, line/page/TOC,
  ref Page/Normal, six Cite forms and same-form Space-only grouping match §2.4.
- `mi`/`mitex` -> Latex math. Remove only an exact leading configured macroblock
  from macros_as_newcommands(macros_from_config(effective math.katex_options));
  preserve the remaining user's bytes. T30 already implements these helpers.
- listing -> CodeBlock attrs numbered/plain/start/highlight; code-cell ->Cell
  index/lang/source/count/show flags, each trailing output content becomes one
  Display CellOutput with text/x-typst candidate containing walked IR blocks.
  Features notebook/code/citations/math/footnotes come from resulting content.

## Metadata and top-level statements — §3 vocabulary exact

First root `show: writer-doc.with(...)` (or bare writer-doc, module alias form)
extracts DocMeta/config; later or other show rules W-TYP-SHOW-IGNORED. Root set
rules map only specified targets/args, with later sourceorder values winning;
other fixed layout targets silent, unrecognized/nested set W-TYP-SET-IGNORED.
Conditional/dynamic settings are not evaluated.

- Recursively convert only known config/frontmatter vocabulary kebab keys to
  underscore form, including nested table keys. Preserve arbitrary vars verbatim
  (toc-title, abstract-title, reference-section-title remain hyphenated).
- Set DocMeta title/subtitle plain text; authors str/array/author dictionaries;
  date str/datetime ISO; abstract walked blocks; keywords/description/lang fields.
  Configurationkeys stay meta.vars, including citation/table maps, numbering,
  TOC, paper, theme/template resource flags. auto means absent, not null override.
- bibliography paths/path arrays become vars.bibliography; cite-style/csl becomes
  vars.cite_style; cite-locale becomes vars.cite_locale. A raw string where package
  expects path is accepted with W-TYP-PATH-STRING. Explicit bibliography(style:)
  updates samevars; outline defaultdepth effectiveToc else3.
- TOC requested by metadata inserts spanNone Toc at index0 if no explicit one;
  bibliography metadata/explicit placement follows §2.4/IR/resolve behavior.
  Both a show-rule TOC/bibliography and explicit block warn W-TYP-DUPLICATE-BLOCK,
  since PDF shows both. Avoid duplicating synthetic blocks in structured IR.

## Typst includes and source identity

Implement recursive `.typ` includes now; `.md`/`.ipynb` share one deliberate
T53 stub arm emitting E-TYP-INCLUDE-MISSING with exact message
`including .md/.ipynb from typst arrives in T53`. include-notebook similarly
retains its T53 boundary, using that diagnostic rather than unknowncall.

Literal Str/path only, relative including source, `/x` initial root. Keep one
canonical recursion chain for cycles and depth (more than8); first-use included
files list is absolute/transitive (including a known missing target for watcher
recovery). Errors are E-TYP-INCLUDE-MISSING/-CYCLE/-DEPTH/-DYNAMIC, with source
span at include call and useful chain. Splice successful blocks; merge assets,
features and diagnostics. Every nested block/ref span names actual includedfile.
Do not reuse Markdown include State blindly: its depthlimit/acceptedkinds and
Error-vs-diagnostic contract differ. T53 later plugs its Markdown/notebook arms
into this frontend, and World IncludeProvider already exists separately.

## Corpus and acceptance

Create the exact owned fixture tree from typst-dialect.md Fixtures:
minimal.typ (every metadata argumentkind, heading/paragraph/footnote), subset.typ
(every call/node/labeltarget/reference/cite/list/term/spannedtable), math.typ
(R2 fifteenequations plus kept #code and mat(delim:#none)), includes.typ+
include/{part.typ,cycle-a.typ,cycle-b.typ}, syntax-error.typ, unsupported.typ,
and refs.bib bytecopy of fixtures/bib/refs.bib. Add valid local imagefixture(s)
inside typ/ for imagecalls; all resources under initial source root. Unsupported
include-depth/missing paths may use generated temporary trees in focusedtests.

- IR and diagnostic snapshots `typst_frontend__<name>.snap` and
  `__<name>-diagnostics.snap`, unresolved, stable redacted source/assetorigins.
  Review every snapshot and run Document::invariant_violations on successful IR.
- Every §2.8 code T50 owns has a fixture/focused assertion. W-TYP-TEMPLATE-IGNORED
  explicitly belongs T52; E-TYP-MATH-CODE belongs downstream math and is not
  required frontend error output. Syntax-error treeempty; warningonly c++ raw
  stillwalks; strictsamecode/errorseverity (config and CLI), Unicodecolumn spans.
- Isolated node/arg tests cover statementwhitespace/paragraph grouping, quotes,
  labelacrossparbreak/forwardfootnote, scope/importaliases/renames, literalwrong
  shape/spread, all figureinference cases incl Equation and explicitkind mismatch,
  everytablegrid/span/alignment/flattening case, macro-prefix matching vs similar
  usertext, code-cell outputs, metadata precedence and unchanged callerConfig.
- Includes: first-use dedup/repeatedinclude/canonicalcycles/depth8vs9, missingfile
  recoverypath, included assetrelativity/ownspans/nesteddiagnostics, outerroot
  absolute paths, T53 stubarms. Missingassets warn and remain unregistered.
- refs.bib equality assertion; valid subsetfixture may compile with existing T03
  embedded World to catch invalid fixture syntax/package calls. Numbering oracle
  comparisons and actual PDFs stayT32; normalized full corpus roundtrip staysT52.
- Existing pipeline dispatch/HtmlDocx smoke verify valid simple .typ prepares and
  subseterror refuses nonPDF target with location. Do not implement .typ PDF target
  (T52), exporttypst (T34), or blanket pertargetcheck (T71). If checkhook approved,
  target-free check .typ reports full syntax/subset JSON and proper0/1 exits.
- Update the one T23 private test relying on frontendstub to inject Action::Fail;
  keep its stage isolation intent. Preserve all other milestone stubs.
- Root serial fmt/Clippyallfeaturesdeniedwarnings/debug+release/rustdoc/basicmixed
  buildsmokes and all new snapshotreviews. PR records provider boundaries, additive
  hooks and inherited PR merge order; continue next feature without CI wait.

## Concrete coordinator decisions before dispatch

1. **Labelled bare table conflict.** Typst §2.3 says label a table's attrs.id;
   references.md §1.2 calls bare-table attrs.id kindOther. IR §2.2/invariantchecker
   requires any labelled Table inside FigureKind::Table. Decide normalization:
   a possible reconciliation is an unnumbered, captionless Table figure with no
   outerid and label retained on innerTable (Other); simply promoting label to
   numbered outerFigure would change reference semantics. Freeze a test/example
   and clarify specs rather than weakening IRchecker or silently violating it.
2. **Typst check integration.** CLI currently returns check-typst T50stub before
   common check::run (which already calls pipeline::prepare). Approve removal of
   that one guard and update tests/cli_check.rs; retain --target T71stub. This
   exposes frontend diagnostics now without implementing T71's pertargetchecks.
3. **Literal argument type errors.** §2.4 E-TYP-ARG-INVALID vs §3 literal accepted
   then Config::merge_front_matter failure is ambiguous for e.g toc:"bad" or an
   invalid nested table value. Recommend validate known shapes as ArgInvalid and
   skip offending args; leave true config-merge validation failures as ordinary
   existing Config/FrontMatter Result errors. Freeze exact policy for snapshot
   expectations, not an invented new error or evaluator.
4. **Included metadata.** §2.7 explicitly merges blocks/assets/features/diagnostics
   but does not state whether included .typ's rootshow/set metadata modifies outer
   DocMeta or only its own effective config. Recommend rootDocMeta remainsroot,
   included metadata affects only that child's walking and emits include-meta-ignored
   when nonempty, as Markdown includes; alternatively discard silently but sayso.
   This is observable for TOC, bibliography/macros/strict precedence and needs a
   focused test. `.md`/`.ipynb` remainstub eitherway.
5. **Private implementation children.** A full L walker is too large for one file.
   Approve narrow private children in owned frontend/ (e.g literals/metadata/calls/
   tables) with `mod` only in frontend.rs and PLAN ownership note, as T23's split.
   Freeze internal walker/argument seams before agent dispatch; no new publicAPI,
   Cargo, IR, config or frozen module-list edits needed.

## Frozen coordinator decisions (2026-10-28)

1. Preserve a logical bare table's label on its inner Table as kind Other.
   During final IR normalization, wrap a labelled Table outside an existing
   Table figure in a captionless FigureKind::Table with class unnumbered,
   no outer id, and the same span. Do this after figure inference so a table
   already inside a Table figure remains its direct child. Assert invariants,
   Other reference kind and unchanged figure counters; clarify the Typst and
   references specs on the T50 branch. No IR/checker changes.
2. Remove only the CLI check's T50 guard and update its test. Keep --target
   T71 and all T52 build-target dispatch stubs. Valid/simple Typst web checks
   use prepare/web_page directly, rather than claiming CLI build is T50.
3. Validate the known argument shapes in §§2.4/3; an incorrect literal shape
   emits E-TYP-ARG-INVALID and skips that argument. Nonliteral template values
   emit E-TYP-TEMPLATE-ARG and skip it. Genuine remaining config validation
   failures propagate the existing Config/FrontMatter Result error; do not
   invent an evaluator or silently replace the whole effective config.
4. An included Typst file retains its own effective config for walking but
   never changes the outer DocMeta. Explicit show/set document metadata emits
   include-meta-ignored at the include site. Remove only a synthetic span-None
   TOC generated from child metadata; retain explicit outline blocks/spans.
   Assets/features/diagnostics/includes merge as specified. Warn only for
   explicit nonempty metadata settings, not inherited config defaults.
5. Approve private frontend children for walker/literals/metadata/calls/tables,
   declared only in owned frontend.rs. Record ownership additions in PLAN and
   the spec before repository edits. Freeze shared internal value/context
   interfaces before parallel implementation. No public module list changes.
6. Apply pipeline::escalate_strict to Typst diagnostics in parse_only after
   metadata merge and final CLI overrides, before returning Prepared. This
   narrow hook ensures CLI strictness cannot be defeated by metadata that
   sets typst.strict=false, and web/subset checks see final severity before
   execution. Frontend also applies its effective-config strictness locally.
   Preserve codes and public API; add a CLI-override regression through the
   prepare/web_page seam. Record additive ownership in brief/spec/PR.

Root has read the binding frontend sections and approved these decisions.
T23 provider work has passed32 units,16 real CLI tests and9 actual Chrome
cases; its full debug suite passes1,283 tests while release validation runs.
Temporary T50 preparation may proceed, with repository writes only after
T23's checked PR and branch handoff.

### Table capacity and alignment decision (coordinator, 2026-10-28)

A structured table grid has a 65,536-slot budget. Check integer conversions and
row/column/span expansion before allocating. If the grid exceeds the budget,
emit E-TYP-ARG-INVALID and drop that table call, preserving surrounding content;
never truncate counts silently. Native PDF compilation remains independent.
Table-cell auto/vertical-only alignment inherits the column (None override).
Column tracks support auto/percent mixtures and all-fr ratios; fr mixed with
auto/percent is unsupported and diagnosed E-TYP-ARG-INVALID. Document this
bounded structured-output policy in the Typst spec on the T50 branch.


### Missing-asset discovery seam (coordinator, 2026-10-28)

Approve the narrow markdown::walker::missing_asset_path extension accepting
an optional trailing Typst `(col N)` suffix. Keep all frontend diagnostics
located/column-suffixed and preserve the existing looked-for path schema. A
regression covers names containing parentheses/column-like text. This prevents
T50's missing-resource diagnostics from breaking the shared watch recovery.


### Nested labelled-table normalization (coordinator, 2026-10-28)

If a labelled plain Table itself contains nested figures, moving its label to
a containing Div is the representable Other-kind anchor: clear only the inner
table id and preserve all content/attrs/spans. Otherwise retain the ordinary
unnumbered Table-figure/inner-id normalization. Downgrade enclosing non-Other
figures when required by the existing nested-figure invariant. This exception
preserves labels and avoids recursive rewrapping without changing shared IR.
The private normalize child owns this post-walk phase and forward-note fixup.

### Literal content capacity (coordinator, 2026-10-28)

Captured literal content shares an allowance across included files: 64 active
content replays and 65,536 replayed expressions/content entries per parse.
Emit one located E-TYP-ARG-INVALID on exhaustion, drop further oversized
replays, and retain surrounding direct source markup. Direct markup does not
consume this allowance. Tests cover exponential duplication, deep chains and
ordinary reuse; native PDF compilation remains independent.
Each decoded literal value is separately limited to 65,536 nodes, 64 levels
and 8 MiB of strings, including dictionary keys. Check references before
cloning their arrays/dictionaries; report E-TYP-ARG-INVALID on excess. Tests
cover array duplication, deep nesting, string composition and ordinary aliases.
