# T51 — Typst math to LaTeX and the Unicode symbol table

Preparation for coordinator freeze, 2026-10-28. No implementation, tracked edits,
Cargo, Git, formatting, generator or oracle regeneration has run for this brief.
The public contracts and required behavior below are already frozen by the specs;
the private split and final small recovery choices are proposed for coordination.

| | |
|---|---|
| Milestone / size | M5 / L; execute after T24, before T42 |
| Formal dependency | T01; pinned codex/typst-syntax already exist |
| Ownership | `src/math/typst_to_latex.rs`, `assets/math/unicode-latex.toml`, `scripts/gen-unicode-latex.py`, `scripts/unicode-latex-overrides.toml` |
| New tests/corpus | `tests/typst_to_latex.rs`, `tests/fixtures/math/typst/{01..36}*`, corpus README and LaTeX/diagnostic snapshots |
| Binding sources | PLAN T51, §3.2, §6–7; math.md §3.3, §7.2, §9–11 and Fixtures; vendored-assets.md §3.7; latex-output.md §4.8; R2 §4–7, §9 |

## Goal and dependency boundaries

Implement a pure converter over typst-syntax's math tree and an embedded,
deterministically generated Unicode/name→TeX table. Return an equation **body**,
not `\(…\)`, `\[…\]`, `equation`, a document or a package list. `aligned` wrapping
is the one required body-level environment. Runtime never invokes pandoc, tylax,
TeX, a browser, the Typst compiler, network or a filesystem lookup.

The converter stub is empty. `src/latex/escape.rs` is also a T60 stub; there is no
existing runtime Unicode→TeX/TeX-escape helper to import. `math/mod.rs` already
exports typst_to_latex. codex =0.3.0, typst-syntax =0.15.1, serde, toml, regex and
insta are present. Keep Cargo.toml/Cargo.lock/build.rs and public module lists
unchanged; especially no tylax dependency or second typst-syntax version.

T42 consumes this milestone's Typst fixture corpus for MathML→OMML/folding tests.
DOCX's Typst route remains compiler MathML → OMML; it does **not** call this
converter. Leave `math/omml.rs`, `docx/math.rs`, all DOCX fallback/numbering work,
T52 `.typ` dispatch/serve, T53 includes, T60 LaTeX emitter/Unicode policy/package
scan and T71 target checks alone. `math-latex-package` is T60's info diagnostic,
not something convert raises. No changes to T50's math preservation rule.

## Frozen public API

All live in the existing `math::typst_to_latex` module:

```rust
pub fn convert(source: &str) -> (String, Vec<Diagnostic>);
pub fn convert_ast(math: typst_syntax::ast::Math<'_>) -> (String, Vec<Diagnostic>);
pub fn symbol(path: &str) -> Option<&'static Sym>;
pub fn char_command(c: char) -> Option<&'static Sym>;
pub struct Sym {
    pub tex: &'static str,
    pub package: Option<&'static str>,
}
pub fn command_package(name: &str) -> Option<&'static str>;
```

`source` is one Math.source body, without dollar delimiters. Both conversion
entries use the same walker/rules and never return Err. `convert_ast` accepts an
Equation.body() or parse_math(src).cast::<ast::Math>(). `command_package` accepts
one control word **with** its leading backslash, e.g. `"\\mathscr"`; it is the
one package lookup consumed by T60 for converted and original LaTeX alike.

Converter diagnostics have level Warning and initially no path/span. Their
message includes `at bytes a..b of the equation`; a caller supplies the enclosing
block's path/span and pipeline escalation applies strictness. UTF-8 byte ranges
are relative to the supplied equation body, including for convert_ast of a
subtree, not relative to the containing file. No new diagnostic code or severity.

## Recommended private ownership seams

Keep the public facade and generic math walker in typst_to_latex.rs. If parallel
implementation benefits, root may approve **private** children under
`src/math/typst_to_latex/`; no new public module. Freeze these narrow seams before
fan-out (names are concrete suggestions, not additional public API):

- Symbol/generator owner: generated table, overrides and generator; private
  `symbols::{symbol, char_command, resolve_char, command_package}`. Facade
  re-exports only the frozen public functions and Sym; `resolve_char(path:
  &str) -> Option<char>` stays private and strips a trailing text/emoji VS before
  accepting exactly one Unicode scalar. `symbol` checks final names table first.
- Root walker owner: `convert`, `convert_ast`, an equation-relative range index,
  preflight for Hash/code, AST sequencing/attachments/delimiters, token-boundary
  writing and diagnostics. Use LinkedNode rooted at the supplied Math for range
  indexing: root offset is zero even when its actual Source span is numbered.
- Optional calls owner: private `render_call(ctx: &mut Converter<'_>, call:
  ast::MathCall<'_>) -> String`, with `Converter::math(ast::Math) -> String`,
  `Converter::expr(ast::Expr) -> String`, `Converter::warning(code, message,
  node: &SyntaxNode)`, `Converter::text(&str) -> String` and private symbol
  resolution supplied by root. No direct writes to another owner's buffers.
  It owns argument classification/row boundaries/function table and small call
  units; root owns code preflight so children never introduce extra diagnostics
  in the one-code-fallback case. Do not invent a new math IR/evaluator.
- Independent tests owner: all 36 source fixtures, recorded oracles, corpus
  snapshot/normalization and optional external-tool checks; generator owner
  owns its parser/count/drift tests and table integrity/package consistency.

For static Sym references, a process-wide LazyLock with one parsed, embedded
TOML table is sufficient. Owned parsed source may be retained once for process
lifetime to back static string slices; no per-call parse/leak, external file
reads, build-time generator or self-referential unsafe storage is needed. The
loader ignores `char`/`from` provenance fields. A committed invalid table is a
programmer error checked by tests, not a recoverable user configuration error.

## Converter rules to implement completely

The §3.3 walk and function table are binding. In particular:

1. `MathText`: digits unchanged; each grapheme via char_command or ASCII literal.
   `MathIdent`/field path: final symbol table first, codex+char table second, then
   the actual Typst operator table (`liminf`, `limsup` included), dif/Dif and
   spacing names; unknown name → `\mathit{…}` + W-TYP-MATH-SYM-UNKNOWN. Unmapped
   non-ASCII graphemes pass through with that warning. Do not warn for ordinary
   single ASCII variables. Symbol paths with empty modifier segments return
   None instead of reaching codex ModifierSet's debug assertion.
2. MathShorthand uses get(), Escape uses its decoded character, Space emits one
   space, Str emits escaped `\text{…}`. Escape TeX literal punctuation as needed;
   an escaped ampersand is not an alignment point. Preserve text contents and
   case. The text escape set is `\ { } $ & # % _ ^ ~`.
3. Attachments always use `{…}` for sub/superscripts and retain primes; bare
   MathPrimes emits the exact prime count. MathFrac uses num/denom, MathRoot its
   optional index (3/4), and parser-deparenthesized Math nodes do not regain
   grouping parentheses.
4. MathDelimited renders `\left<open> … \right<close>`; mismatched pairs are
   supported, curly delimiters escaped, U+2016/`‖` becomes `\|`. Its body() is the
   **inner Math only** in the pinned parser; do not emit the delimiters twice.
   `lr` unwraps/scales actual delimiters once and leaves an undelimited body
   unchanged. Missing delimiter sides use `.` where the spec calls for it.
5. MathAlignPoint → `&`, Linebreak → `\\`; wrap the equation once in aligned
   when genuine AST alignment/linebreak constructs occur. String/escaped `&` or
   a backslash in text cannot trigger wrapping. Matrix/vector/cases row
   separators are handled by their environment, not a raw source-text search.
6. Implement every §3.3 mapped family: frac/binom/sqrt/root; vec/mat/cases;
   abs/norm/floor/ceil/round/lr; all listed accents including field aliases;
   over/underline, over/underbrace and over/underbracket; bold/upright/italic/
   bb/cal/frak/sans/mono/scr; op with literal limits bool; limits/scripts;
   display/inline/script/sscript; cancel. Mat/vec delimiter choices are default
   pmatrix, `[`: bmatrix, `{`: Bmatrix, `|`: vmatrix, `‖`: Vmatrix, #none: matrix.
   Matrix rows split on MathArg.ends_in_semicolon, never naive text splitting.
7. limits/scripts wrap a non-operator base in `\mathop{…}`; an actual TeX math
   operator command gets limits/nolimits directly. `op(..., limits: #true)` is
   `\operatorname*{…}`. Preserve explicit false behavior.
8. Callees resolving to Typst callable accents use the accent form. Other
   resolved symbols get the symbol plus scalable argument parentheses. Unknown
   functions → W-TYP-MATH-FN-UNKNOWN + `\operatorname{name}(…)`; the documented
   approximate wrappers text/class/stretch/attach/primes retain the body with
   that warning; mid emits `\middle` with its documented warning. Unsupported
   named args emit W-TYP-ARG-IGNORED per construct, including their names.
9. Preflight **before any emission/other warnings**: any embedded Hash/code
   causes the whole escaped `\text{[typst math: <source>]}` fallback with exactly
   one E-TYP-MATH-CODE and no other diagnostics. The only exception is literal
   values in named args (the specified #true/#false/#none/numbers). No general
   evaluation, bound variables, arithmetic, imports, file IO or compile call.
   Quoted `"#"`, escaped hash and comments are not code. A hash-literal positional
   expression remains code. Caller strictness alone escalates this warning.
10. Source Space tokens are the only added visible spaces. Between an emitted
    TeX control word and an adjacent ASCII letter, use a neutral `{}` boundary
    rather than accidentally forming `\alphax` or adding source whitespace.
    Output is deterministic; never depend on iteration over an unordered table.

## Pinned primary-source facts and donors

Local primary files, all read during preparation:

- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/typst-syntax-0.15.1/src/ast.rs`
  and parser.rs: parse_math returns a Math SyntaxNode; Math.exprs retains Space;
  MathCall.callee returns MathAccess; MathArgs.arg_items yields MathArg { arg:
  Arg::Pos/Named/Spread, ends_in_semicolon }; content_items distinguishes code
  semicolons. MathText.get is MathTextKind::Grapheme/Number. Attach has
  base/bottom/top/primes, primes.count, frac num/denom, root index/radicand.
  Parser max recursion depth is 256. Hash is an untyped syntax token and is
  filtered from ordinary expr iteration: preflight must inspect untyped nodes.
- `typst-library-0.15.1/src/math/{mod,op,accent,lr,style}.rs`: current native
  operator spellings and private ACCENTS table. ACCENTS is not a public API;
  make a small audited local mapping, not an import of its private static.
- `codex-0.3.0/src/{lib,shared}.rs`, build.rs and modules/sym.txt: module-aware
  symbol lookup and modifier-set matching. Symbol::get chooses fewest extra
  modifiers, first source variant on ties. Deprecated aliases still resolve;
  trailing U+FE0E/U+FE0F are presentation selectors. Guard empty modifiers.
- `tylax-0.3.8/src/data/maps.rs` and `src/core/typst2latex/utils.rs`: **data donors
  only**. Its converter/math_ir/math_emit may inform reading but do not copy a
  second parser/evaluator or add its dependency. Prefer pinned registry/archive
  source over the mutable sibling `../tylax` checkout.
- `codex::styling` is enabled by codex defaults and styles **to** Unicode; it
  does not supply the frozen reverse LaTeX table. Python unicodedata drives the
  specified alnum generation. T42's reverse folding remains T42 work.

## Generator, overrides and reproducibility

Implement vendored-assets.md §3.7 steps 1–10 exactly with Python >=3.11 stdlib
only. The currently cached crate archives were read and their full hashes
verified; they match the frozen inputs:

| Archive | Bytes | SHA-256 |
|---|---:|---|
| tylax-0.3.8.crate | 487995 | `145d65de1d49c044ae64c664edf034f4956c17dbddaa0cf2fad3afea85400672` |
| codex-0.3.0.crate | 58671 | `0732ab1a27b4ea05e6f9f60a5122c9924dd5123defde0d8e907f58cf643d40e6` |
| typst-syntax-0.15.1.crate | 104197 | `92a20d79df5d10c7fb0b4f7c18e4b2671edfc21e5ed9ad9894e18cccefd1c70f` |

URLs are `https://static.crates.io/crates/<name>/<name>-<version>.crate`.
Read only the exact members listed in §3.7 through tarfile; never extractall.
Downloads and any optional local archive inputs must pass the same full SHA
check before parsing. Abort before output mutation on any verification/count/
parse/package error. Only final UTF-8 LF output is committed; --check must not
rewrite it. Paths resolve from the script's location, not the current directory.

Mandatory generator commands (already consumed by existing vendor-drift CI):

```sh
python3 -I scripts/gen-unicode-latex.py
python3 -I scripts/gen-unicode-latex.py --check
```

The CI step already conditionally invokes --check once this script exists;
**no workflow edit is necessary**. Default operation downloads into a temporary
directory. Proposed small developer-only CLI additions, if root approves, are
`--archive-dir DIR`, `--overrides FILE`, `--output FILE` so local pinned archives
can exercise the empty-overrides baseline without editing tracked inputs.
They are not Writer CLI flags; the default/--check contract stays unchanged.

Required parse counts are N=532, U=159, S=38, codex value declarations=1206
(288 plain +918 variants), symbols=334 (46 variants-only). With empty overrides
expect names=532, chars=1292 (405 N/U +887 alnum), conflicts=17, unresolved=87
(54 undotted +33 dotted). Every conflict and unresolved key is printed to stderr
in deterministic order; do not hide a baseline mismatch by changing expected
counts. The final table's larger/different counts are reported and reviewed,
not confused with those baseline numbers.

Overrides are strings in [names]/[chars], where empty deletes; [packages] maps
backslash-prefixed commands to one package. Include all required initial rules
of §3.7: paragraph/section/thin/med/thick/AA/EE; U+2605/U+27E6/U+27E7/U+03BF;
mathtools/stmaryrd/esint/mathdots/gensymb/eurosym commands. Then repair/delete the
remaining TeX-invalid values according to the all-values pdflatex gate; recorded
bad examples are `\eq`, `\lt`, `\argmax`, `\napprox`. Consult overrides during
generation only; runtime reads the committed final table.

Generation keeps source-order first-wins names→char entries, source-order U
entry overwrites, then explicit char overrides, then assigned unset alnums in
code-point order. Codex resolution uses longest symbol prefix and minimum-extra
modifier matching, strips text/emoji VS, and rejects a remaining multi-scalar
value. Apply exact style/NFKC/ASCII-capital restrictions from §3.7; do not fill
unsupported script/double-struck lowercase with invalid amssymb commands.
No copied GPL pandoc/texmath tables or LPPL unicode-math-table.tex data.

Output keeps the fixed [source] pins, sorted [names]/[chars], escaped TOML basic
strings and per-char `char, tex, package?, from` field order. Preserve the frozen
attribution/license header; `assets/vendor/crates/tylax/LICENSE` already exists.

`COMMAND_PACKAGES` is a private constant of command/package pairs. Its union is
all generated-table [packages], mapped function packages (mathrsfs, mathtools,
cancel), plus the original-LaTeX list in latex-output.md §4.8 (`\bm`, `\bcancel`,
`\xcancel`, `\mathclap`, `\xmapsto`, `\Mapsfrom`, `\longmapsfrom`, etc.). A value
that needs two distinct extra packages fails generation; command matching is
backslash + maximal ASCII letters, so `\foo` must not match `\foobar`.

## Corpus, oracles and required tests

Create all **36** named Typst fixtures exactly as math.md §10 specifies, with
line 1 `// mode: display|inline` and the remaining text the equation body. Keep
that body directly usable by convert and compiler MathML later. T51 does not
create/change T42's `math/expectations.toml` or `.mml` files. Existing `.tex`
corpus/macros fixtures remain untouched. No ancestor writer.toml.

Check in `<stem>.pandoc.tex` for every fixture pandoc parses, excluding known
18 mismatched-lr parse failure. Generation wraps the body in native Typst math
according to its mode and invokes `pandoc -f typst -t latex`; README must give
the exact wrapper/command and pandoc version. Current local executable is
**pandoc 3.12.1**, verified by --version. Do not feed a delimiter-free math body
as ordinary Typst markup. Keep committed oracle bytes independently reviewed.

Only17–24 get `.tylax.tex`; transcribe the recorded pinned R2 §5.2 outputs rather
than running tylax again. Mapping to R2 cases is17→7,18→8,19→9,20→11,21→12,
22→13,23→14,24→15. Its known wrong matrix/vector/accents/alignment output is
comparison evidence, never a reason to weaken Writer's contract.

Acceptance tests:

- convert/convert_ast output+diagnostic equivalence, empty input, every AST form,
  all function families/named options, callable accents and generic symbols,
  implicit `f'(x)`, nested/deparenthesized attachments/fractions, roots3/4,
  mismatched/literal delimiters and control-word token boundaries.
- All36 LaTeX-body and diagnostic goldens, including whole-code fallback exactly
  once, unknown function/symbol, ignored named arg, every38 shorthand, spacings,
  all packaged functions, Unicode Greeks/alnums and unmapped grapheme recovery.
  Escaped/quoted hash and allowed named literal Hash arguments need focused
  negative tests against false whole-equation fallback. Ranges must be UTF-8
  bytes and relative to the equation, even inside a larger parsed Source.
- Compare01–17 and19–27 with pandoc after deleting whitespace, `{` and `}`.
  Explicit per-form tuples `(fixture, pandoc_form, writer_form, reason)` are the
  **only** exceptions: known ones are lim limits, pandoc's invisible left/right
  wrappers, rightarrow/to, d/mathrm-d, and overset-overbrace vs overbrace^label.
  Additional concrete differences (e.g. scalable implicit-call delimiters) need
  individually reviewed tuples; never exempt a whole fixture, strip arbitrary
  commands, or change the converter to match an oracle bug. Optional pandoc-on-
  PATH test regenerates and compares; absence prints an explicit skipping note.
- Table integrity: every U+ key equals its char, source pins match Cargo.lock's
  actual codex/typst-syntax versions, deterministic sorted output and provenance,
  known required values (§3.7 acceptance), all shorthand scalars mapped, no
  accidental duplicate keys or code-point substitution. Pure runtime lookups
  never depend on the crate archive/cache existing.
- Two mandatory package consistency tests: all overrides [packages] pairs occur
  in COMMAND_PACKAGES; every packaged Sym from **every name and char row** has a
  control word in its tex mapping to that same package via command_package.
  Cover function-only/original-LaTeX commands and maximal-word matching too.
- If pdflatex exists, every distinct generated tex must compile as `$x <tex> y$`
  under amsmath/amssymb plus exactly its one declared extra package; absent tool
  prints skipping. Batching by declared package avoids thousands of processes
  while preserving the package-isolation assertion and per-row error context.
- Generator tests cover source counts/baseline, override replace/delete,
  modifier source-order ties/VS handling, Rust/TOML escaping, package conflicts,
  bad SHA/member refusal, final output atomicity and --check drift detection.
  Python unittest with stdlib is sufficient; do not install test dependencies.
- Never-fails regressions include malformed syntax, invalid public symbol paths,
  non-ASCII/combining graphemes, nested calls and parser-depth recovery; no panic,
  evaluator or unbounded Cartesian expansion. Do not add a new resource-policy
  API or arbitrary new diagnostic without coordinator agreement.

After installation root runs generator/default/--check, focused Rust converter
and table tests, available pandoc/pdflatex checks, required fmt/Clippy/docs/debug/
release suites, then opens a milestone PR and continues without waiting on CI.
No such command was run during this preparatory task.

## Narrow decisions still needing coordinator confirmation before fan-out

The specs intentionally leave the final override list to T51. Three behavior
edges are underdescribed rather than permission blockers for preparation:

1. Never-fails recovery for malformed math/arity: recommend best-effort visible
   escaped source for an Error node with W-TYP-MATH-SYM-UNKNOWN at its range;
   known calls with bad arity retain all positional content in generic function
   form with W-TYP-MATH-FN-UNKNOWN. Reserve the whole-equation code fallback and
   its exclusive diagnostic for actual Hash/code, not ordinary syntax recovery.
2. Literal Hash whitelist: recommend direct named Bool/None/Int/Float/Numeric
   and unary-minus numeric literals only; no identifiers/compound expressions.
   Ordinary named Str needs no Hash. Confirm whether a redundant `#"["` should
   count as a permitted literal string or follow the spec's explicit narrower
   bool/none/numbers list. Test the chosen boundary once; no evaluator.
3. Less common native callable accents (dot.triple/quad, acute.double, harpoons)
   must have an audited TeX form under the declared package rules. Direct
   amsmath forms/compositions are preferable to silently emitting an undefined
   macro. Confirm any required per-accent approximation/extra package in the
   same small function/override table, not a new configuration knob.


## Approved coordinator decisions (2026-10-28)

- Private symbols/calls children are approved under math/typst_to_latex/;
  the public facade re-exports only the frozen API. The symbols child owns
  the package constant/lookup and generator; no public module-list change.
- Malformed non-code syntax keeps escaped visible source and emits located
  W-TYP-MATH-SYM-UNKNOWN. A known call with invalid arity uses its generic
  visible function form with W-TYP-MATH-FN-UNKNOWN, retaining positional
  content. Whole-equation exclusive code fallback remains reserved for code.
- Named Hash literal allowance is exactly Bool/None/Int/Float/Numeric and
  unary-negative numeric literals. Redundant Hash string literals remain
  code; ordinary named strings need no Hash. Do not evaluate expressions.
- Listed accent mappings are exact. Additional native callable accents need
  audited valid amsmath compositions; triple/quad dots use dddot/ddddot.
  Report any rare form requiring approximation/package changes to root for
  a concrete choice before adding it; no silent undefined macro.
- Generator archive-dir, overrides and output developer flags are approved;
  every archive still verifies the pinned full hash, and defaults/--check
  remain unchanged. Draft generation/oracle commands may run only against
  ignored preparation outputs until the T51 branch is installed.


## Installed-parser corrections and reviewed acceptance

The pinned parser represents `#-3` and `#-1pt` as a Hash followed by an
Error, with a hint requiring parentheses. The named negative-literal whitelist
therefore accepts native `#(-3)` / `#(-1pt)` only when the parenthesized child
is exactly a unary minus applied to an Int/Float/Numeric literal. Arithmetic,
identifiers, nested grouping and malformed direct forms remain code fallback.
This corrects the preparatory syntax assumption without introducing evaluation.

MathText Number also includes mathematical Unicode digits in typst-syntax
0.15.1. Preserve ordinary ASCII numbers and map non-ASCII digits through the
same generated character table as other graphemes. Unsupported styles retain
the source and warn; in particular the generator intentionally limits
amssymb double-struck/script families to ASCII capitals.


Final acceptance: formatting, all-feature Clippy, default/feature rustdoc,
1,428 debug and 1,427 release tests pass. All 14 public tests, 13 private units,
72 reviewed snapshots and 11 Python tests pass; actual Pandoc reproduction,
36 corpus bodies and all 1,058 distinct generated expressions pass their gates.
See docs/dev/build-times.md for timings and the installed-parser repairs.
