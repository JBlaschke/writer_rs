# T74 — user guide and documentation drift checks

Installed on `codex/user-guide`, based on T73 `ece9db6`. The README, ten guide
pages and two stale example READMEs now describe the implemented workflow.
Original example inputs and notebook provenance remain unchanged. A single
comment in the printed default configuration removes an internal task ID and
obsolete engine advertising; no value, key, schema or API changes.

The guide covers source installation, scaffolding, Markdown/notebook/native
Typst authoring, all outputs, resources/templates/styles, configuration, CLI
and troubleshooting. It distinguishes native PDF from structured exports,
saved notebook outputs from execution, and LaTeX source export from the
external compilation step. Source installation is documented without claiming
that release archives or a hosted site have already been published.

## Generated references and drift coverage

Twelve recursive clap command paths have complete real help fences. The
printed default configuration is captured from the executable, with semantic
coverage for every configuration key/default. Local files, images, decoded
fragments and anchors are checked through parsed Markdown. Pinned comrak's
Anchorizer handles combining marks and literal suffix collisions.

All nine guide tests pass after explicit generation and again in normal
read-only mode. The latter preserves all fifteen source-file hashes. The
checks include negative parser/link/default controls and exclude fenced code
from link scanning. Normal tests never update the guide. User-facing prose
has no internal milestone identifiers.

## Actual command validation

The reviewed finite walkthrough passes **54 fixed command cases** in copied
examples and four new scaffold projects. These include all five existing
examples, supported default/mixed outputs, actual bibliography resolution,
check JSON/denied warnings, dump/context/source output, functional ejection
TOML and actual theme/template/reference use. Fresh artifact checks prevent
stale outputs from satisfying a later command. Dump inventories include files,
directories, content and metadata to detect identical rewrites or creation.
Original example input hashes remain unchanged.

This is argument-vector coverage in explicit representative projects, rather
than a claim to execute every shell fence in every context. Source install,
long-lived preview, external TeX and optional notebook regeneration have
separate categories; no arbitrary tutorial shell strings are executed.

Two separate copied-scaffold startup probes pass: Markdown defaults to web,
native Typst to pages, both explicit views and state succeed, and the first
published-hash SVG has the required MIME and immutable caching. Source/copy
inventories remain unchanged. Both child processes exit 0 on SIGINT and are
reaped without force. Address discovery is bounded; socket timeouts bound
inactivity rather than total HTTP wall time. This is HTTP startup/cleanup
proof, distinct from interactive browser/keyboard or `--open` behavior.

The documented source-install command passes using an isolated ignored root
and cached target directory, without changing the user's installed commands.
The installed executable passes version/notices, bibliography scaffold and
actual HTML/PDF builds. The source install takes 66.04s wall on this host.

The documented latexmk PDFLaTeX command also succeeds on a fresh generated
source/resource tree. The actual PDF, final log and extracted figure/table/
listing text pass. The exact TeX title remains `My report`; Ghostscript joins
that title as `Myreport`, so only that observed extraction projection is
special-cased. Other source/text checks remain exact. This citation-free
walkthrough requires no bibliography backend; it does not claim Biber use.

Optional notebook regeneration, real system package installation and opening
an interactive browser are not part of these automated walkthroughs. Existing
CLI/watch and browser suites provide their separately recorded regression
coverage; T73's two required-Chrome overlay/repair cases remain prior evidence.
No notebook was executed or original example source changed.

## Final gates

Configured formatting, all-target/all-feature denied-warning Clippy and
all-feature denied-warning rustdoc pass. The complete release suite passes
1,953 cases with three intentional ignores. All 1,954 distinct debug cases
are verified across the complete run and a 55-case targeted rerun; repeated
cases are not counted twice. Timings are in `../dev/build-times.md`.

The complete debug run passed 1,952 cases and found two failures: the binding
configuration mirror still had the old default-engine comment, and the existing
cross-thread watcher test hit its outer completion timeout. Only the spec's
single comment was corrected; values and production behavior stay unchanged.
All 26 configuration, nine guide and 20 original watcher cases then passed.

A temporary timestamp probe and two isolated native runs also passed. They
measured successful event reception and variable teardown timing, but did not
reproduce a failing phase. The cause of the earlier timeout remains unproven.
The original watcher source was restored byte-exact before the full release
run and final debug rerun; no deadline, ordering or assertion was relaxed.
The diagnostic instrumentation and copied executable are ignored evidence,
not shipped code. These runs do not claim a single clean full debug invocation.
