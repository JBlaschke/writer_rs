# T72 — project scaffolding and resource ejection

Implementation installed on `codex/scaffold-eject`, based on T71 `d56ff85`.
The full compile/no-run gate, focused runtime checks and complete release
suite pass. All 1,933 distinct debug cases have been
verified across the complete run and an unchanged watcher-suite rerun.

`writer init` creates a deterministic Markdown or native Typst project, its
figure, offline/embedded-font configuration and a child HTML theme. `--bib`
adds a real bibliography and citation using T70's Library. The native template
uses the current embedded package version and `path("refs.bib")`. Documents
have a fixed safe title, independent of the destination's spelling.

A new or empty project directory works without force. `--force` permits
unrelated files but never replaces selected scaffold paths. Expected file and
parent collisions are checked before any outputs are created; actual files
use `create_new`. Normal existing parent traversal, Unicode names and ordinary
linked ancestors work. Selected directory links must remain inside the project.
Missing-component/`..`, broken links and parent type failures refuse before
creating intermediate directories. Unexpected I/O or concurrent filesystem
changes do not have transactional rollback guarantees.

Theme ejection copies every built-in default-theme byte into a new directory.
Typst and LaTeX template ejection preserves the exact embedded template/sample.
Package ejection uses the existing public API; the required DIR positional
remains required with `--install`, which selects the platform package root.
DOCX reference output and its existing behavior are preserved. Functional TOML
guidance is stdout, including quiet mode; human ejection status is stderr unless
quiet. Built-in resources ignore project config and SOURCE_DATE_EPOCH.

A narrow public-package preflight fix also prevents `new/../packages` from
hiding an existing version until directory creation. Tests preserve both owned
package versions and complete directory/file inventories, plus ordinary `..`
and symlink traversal semantics. This is the only package API implementation
change; signatures, return paths and package lookup stay unchanged.

## Validation

- Full compile/no-run: passed, 88.19 seconds total wall.
- Focused CLI units: 81 passed. Public resource ejection: 9 passed; original
  DOCX: 11; original package: 18; new public package preflight: 3; CLI build: 15.
  An obsolete T72 stub assertion was migrated to real resource ejection with
  unchanged DOCX reference bytes, then the entire DOCX suite passed.
- Scaffold controls: 7 passed, including all four variants, actual default
  builds and all supported targets (five for Markdown, four for native Typst).
  Both Markdown variants produce a physical Typst export that compiles to PDF.
  All configured citations resolve through the real Library.
- Configured formatting, all-target/all-feature denied-warning Clippy and
  all-feature denied-warning rustdoc: passed.
- Complete debug: 1,932 passed, one existing watcher timeout, three intentional
  ignores. The unchanged complete 20-case watcher suite then passed; all 1,933
  distinct debug cases are verified across the full run and focused rerun,
  with no repeated case counted twice. No source or test was changed for the
  watcher rerun. This does not claim a single clean complete debug invocation.
- Complete release: 1,932 passed, no failures, three intentional ignores.
  All 20 watcher tests also passed in this fresh run. Build 13m 56s;
  wall 965.01s; CPU (user + system) 7,320.56s. Measurements are recorded
  in `../dev/build-times.md`.

The scaffold's source `gitignore` is embedded under that non-hidden name and
written as `.gitignore`. This preserves embedding in release mode. No external
compiler, kernel, browser or real system-package write is required by these
commands or acceptance tests. Linux's install subprocess uses a disposable
XDG_DATA_HOME; other platforms test selection without writing system data.

## Watcher timeout evidence

The one debug failure was `a_watcher_can_be_sent_to_another_thread`, at the
outer `finished.recv_timeout(BATCH_TIMEOUT)` (10 seconds). The child first
receives an event with its own 10-second deadline, drops the watcher and only
then signals completion. The outer deadline therefore also includes notify's
synchronous macOS backend teardown, whose runloop wait/join has no crate-level
deadline. That makes cleanup delay a possible contributor; the logs do not
establish whether event delivery or cleanup caused this occurrence. The full
unchanged watcher suite passes on rerun. No deadline was relaxed, test ignored
or speculative production fix introduced. A separate audit retains optional
phase instrumentation outside the tracked source tree.
