# T25 — Opt-in headless-browser client and standalone smoke tests

Frozen implementation brief, coordinator, 2026-10-28.

| | |
|---|---|
| Milestone / size | M2 / M |
| Depends on | T16, already present |
| Primary ownership | `src/pdf/devtools.rs` (T00 stub) |
| New tests | `tests/browser.rs`, private transport/client units in `devtools.rs` |
| Binding sources | PLAN §3.2, §4 T25, §5 T25, §6–7; `docs/spec/live-reload.md` §9–10 and Deviations; `docs/spec/math.md` browser fixtures/Deviations |
| Lift sources | `../presenter_rs/src/pdf/devtools.rs`; the client dependency closure in `../presenter_rs/src/pdf.rs`; selected non-serve tests in `../presenter_rs/tests/browser.rs` |

## Goal and boundary

Expose the presenter's bounded Chrome DevTools page client behind the existing
`browser-tests` feature, then use it to open Writer's built standalone HTML
and exercise the client itself. The ordinary build and test suite never
launches Chrome. Real serve/live-reload browser tests belong to T23;
compiler-backed pages/browser/network assertions belong to T24.

Current code already has `browser-tests = []` in Cargo.toml and
`#[cfg(feature = "browser-tests")] pub mod devtools;` in `src/pdf/mod.rs`.
Do not edit either. No new dependency is required: the lift uses the standard
library, serde_json, tracing and (only if retained generic Fetch transport
needs it) the already-pinned base64. `src/pdf/chrome.rs` is the explicit T77
print-target stub; leave its body and public API unchanged.

## Frozen public API (PLAN §4; lifted)

```rust
pub struct Page { /* browser, private profile, flat CDP session, timeout */ }
impl Page {
    pub fn launch(chrome: &Path, timeout: Duration) -> Result<Page>;
    pub fn set_timeout(&mut self, timeout: Duration);
    pub fn navigate(&mut self, url: &str) -> Result<()>;
    pub fn evaluate(&mut self, expression: &str) -> Result<serde_json::Value>;
    pub fn errors(&self) -> &[String];
}
pub fn readable_temp_dir(chrome: &Path) -> Option<PathBuf>;
```

`Page` implements Debug for diagnostics and Drop for bounded Chrome shutdown
and profile removal. Public docs describe its Unix platform limitation,
errors, per-call timeout and error-event collection behavior. No other public
transport types are required.

### Coordinator choice to approve before implementation

The presenter Page imports private machinery from its monolithic `pdf.rs`;
Writer's `pdf/mod.rs` intentionally contains only children and its Chrome
print implementation is T77. Put the required Browser/CDP/temp-profile/
janitor/executable/snap dependency closure privately inside the owned
`devtools.rs` rather than implementing T77 or adding another source module.
Remove only print/deck-specific state that this client cannot use; keep
transport, cleanup and lifecycle fixes faithful. In particular, Page never
enables Fetch interception and never serves a deck: the always-empty Deck
adapter and print-only Cdp.loaded/timeout fields need not be carried over.
T77 can later move/share transport with a separate coordinator ownership note.

For browser discovery, approve one additive owner-local helper:
`pub fn find_chrome(explicit: Option<&Path>) -> Option<PathBuf>` in
`pdf::devtools`, lifted from the presenter. This avoids duplicating discovery
in T25/T23 browser harnesses or prematurely adding a public Chrome-print
function in T77's module. Make the discovery constants and all supporting
helpers private. A future T77 move/re-export is separate from this milestone.
If the coordinator prefers no extra public helper, keep discovery test-local
and explicitly name that choice in the final brief before dispatch.

## Client and transport behavior to preserve

- Resolve supplied executable as a direct executable, macOS `.app` bundle or
  bare PATH name. Discovery: explicit path wins and must be executable;
  CHROME_PATH next (invalid value warns and falls back), macOS app bundles
  Chrome/Chromium/Edge/Brave/Arc under `/Applications` and `~/Applications`,
  then the presenter's ordered PATH_NAMES.
- Unix Chrome starts through the exact shell fd3/fd4 handoff with
  `--remote-debugging-pipe`, private mode0700 throwaway profile, quiet flags,
  own scratch TMPDIR where readable, root-only Linux `--no-sandbox`, and
  cleanup watchdog. No unsafe code, WebSocket dependency or debugging port.
- Snap paths and wrappers are detected as in the lift. Profile/file pages use
  `~/snap/<name>/common` when it exists; readable_temp_dir returns None when
  a never-started snap can read no scratch directory. Explain this in file
  navigation failures. TMPDIR empty means `/tmp`; paths become canonical
  absolute paths so symlink/.. and relative bases keep OS semantics.
- Browser launch enables Page, Runtime, Inspector and Log and lifecycle
  events on one flat Target session. Request IDs persist across calls.
- navigate waits for its own loaderId, ignores earlier loaders/other frames,
  and follows a replacing loader that starts after its own init. Fragment
  navigation returns when accepted, without waiting for nonexistent load.
- evaluate uses awaitPromise and returnByValue; undefined becomes null;
  NaN/Infinity/-Infinity/-0/BigInt retain Chrome's unserializable spelling.
  Runtime exception/rejected promise becomes an immediate Error::Other.
- errors records uncaught exceptions, console.error, error Log entries with
  URL, and dismissed JS dialogs. Events are drained while subsequent calls
  run; evaluate("0") is the documented collection barrier.
- Crash, exit, closed pipe, write failure and timeout fail promptly and
  include browser path and the last30 stderr lines. Protocol refusal fails
  immediately without treating a healthy browser as dead or waiting for
  stderr. Preserve bounded message size and NUL-terminated wire format.
- Drop asks Browser.close, waits CLOSE_GRACE (5s), then kills/reaps if needed,
  and removes the private profile. The watchdog handles parent-process death
  and must not kill a reused/unrelated process ID. Keep poisoned lock recovery.
- `Page::launch` on non-Unix returns the presenter's explicit platform error;
  the feature still compiles on Windows. Pure tests are portable; tests that
  require /bin/sh, Unix permissions/processes or a browser use cfg(unix).

## Tests and acceptance

Port all five presenter's Page units, adapting non-Unix missing-executable
assertions to the explicit unsupported-platform contract. Preserve its pure
lifecycle matrices and JSON value spelling tests. Add/port meaningful
transport/discovery tests for selected retained internals: executable order,
app bundles/PATH/snap/root parsing, private profile canonicalization/cleanup,
NUL framing, malformed/closed/write-failed messages, bounded timeout and
stderr-tail reporting. Fake executable tests use temp shell scripts and
bounded wait/cleanup; do not inherit old unbounded child.wait assumptions.

`tests/browser.rs` is `#![cfg(all(feature = "browser-tests", unix))]` and uses
one serial mutex with poison recovery to avoid concurrent Chromium launches.
Resolve a browser through the approved helper. Missing browser prints
`skipping: no chrome found`; a nonempty `WRITER_REQUIRE_CHROME` instead fails.
Do not edit process environment unsafely to test the switch; use a small pure
helper or a child process when coverage is needed. Browser files live under
a tempfile created in readable_temp_dir; file URLs percent-encode bytes.

Browser assertions, all against trusted generated local fixtures:

1. Build a copied `examples/basic/document.md` with Writer in build mode,
   open its standalone HTML, wait with a deadline for document fonts and
   KaTeX render completion plus two animation frames, then verify exactly one
   data-writer-document root, nonzero content, server-side `pre.hl-code`
   highlighting (`hl-*` spans, not presenter's highlight.js), complete images,
   rendered math where present, and no collected page errors. This fixture's
   resources are embedded; require no external network/service.
2. Port the Page errors fixture: console.error, uncaught Error/string and
   missing local image are collected; same-document fragment navigation
   preserves a window marker; JSON special values/promise completion, thrown
   errors/rejected promises, missing file navigation and readable_temp_dir
   hint behave as documented. Expected fixture errors are asserted, not
   mistaken for production smoke failures.
3. Port navigation loader regressions: stale self-navigation does not satisfy
   the next navigate; a page that replaces itself before load is followed;
   target replacement during evaluate yields immediate refusal. Bound waits
   by per-call timeout; use generous startup timeout and shorter operation
   timeout, preserving the lift's assertions against wrong-loader success.
4. Add/port a bounded dead/quiet fake-browser failure and cleanup check where
   needed; meaningful transport units can cover this without Chrome.

Do not copy reveal.js slide walking, slide config reloads, serve child-process
harness or theme-image live edits. Those are either irrelevant to Writer or
owned by T23's `tests/browser_serve.rs`. No `writer serve` dependency or future
T24 pages fixture is introduced.

## Validation and delivery

Root serializes Cargo, formatter and git. Run repository fmt, Clippy with
alltargets/allfeatures and denied warnings, ordinary debug+release suites,
denied-warning rustdoc (also with browser-tests so feature-only docs resolve),
and basic/mixed build smoke. Run feature units, then actual browser tests with
WRITER_REQUIRE_CHROME=1 when installed. Report a missing browser/platform as
an explicit skip, never as executed browser acceptance. Local process/socket
sandbox escalation may be needed; retain native production behavior.

Record measured runs and browser path/version in task acceptance/build-times.
Publish a separate `codex/browser-smoke` PR targeting main and list inherited
open milestone dependencies/merge order. Start the next ready milestone
without waiting for CI or merge, per the user's standing instruction.

## Frozen coordinator decisions (2026-10-28)

The coordinator approves the private transport dependency closure inside the
existing devtools.rs and additive public devtools::find_chrome helper. Keep
Cargo.toml, module lists and T77's Chrome print stub unchanged. Page's unused
Deck/Fetch/print fields are omitted; native pipe, lifecycle, timeout, stderr,
janitor and snap mechanics stay faithful.

## Narrow lift corrections (coordinator, 2026-10-28)

- Resolve a supplied bare executable name before snap placement, for both
  Page::launch and readable_temp_dir. Return an absolute readable base and
  canonicalize existing directories so TMPDIR relative/symlink/.. semantics
  match file URLs; the private profile already canonicalizes its own path.
- The watchdog cannot prove PID ownership without ps. Fail closed in that
  case, retaining directory cleanup but leaving an unverified process alone.
  The previous kill -0 fallback could kill an unrelated reused process ID.
- Startup protocol error replies use CallError::Refused, matching runtime
  request refusals: report immediately without draining browser stderr or
  marking a healthy browser as dead. Actual process/pipe/timeout failures
  retain the stderr-enriched browser-failure path.
- Focused units cover injected relative/symlink scratch paths, a bare PATH
  snap wrapper, startup refusal cleanup, and a no-ps watchdog with an owned
  unrelated process that must remain alive. No environment mutation occurs.


## Acceptance result (2026-10-28)

All 28 native client/transport units and six browser/helper tests pass against
Google Chrome 156.0.8078.48 with WRITER_REQUIRE_CHROME=1. Formatting,
all-target/all-feature Clippy, default/feature denied-warning rustdoc,
1,235 ordinary debug and 1,234 ordinary release tests pass. Basic/mixed HTML
smoke sizes are unchanged. The standalone example renders math, highlighted
code, images and fonts without browser errors or external resources.
Detailed cases, review corrections and timings are in docs/dev/build-times.md.
No browser acceptance was skipped locally; Windows retains the explicit
unsupported native-driving contract. T23 real-server integration follows.
