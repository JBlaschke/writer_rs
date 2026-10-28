# Live-reload client QA

Covers the T22 browser client, `assets/writer/live-reload.js`, against
[live-reload.md](../spec/live-reload.md) §1.2, §2 and §3. Repeat after changes to
the client, serve templates, event payloads or change classification.

## Mock setup

The Python standard-library mock serves a long document, an included source,
a notebook cell, actual vendored KaTeX and math.js, two SVG images, theme CSS,
and the same morphdom wrapper as `writer serve`. It sends complete per-view
`hello` and `build` objects. Skipped views keep their previous error state.
Its pages shell has an `__writerPages.apply` spy: T22 checks delegation and
restart IDs; actual page rendering, zoom and resource reuse belong to T24.

```sh
python3 tests/fixtures/live-reload/mock-server.py --port 8765
```

Open `http://127.0.0.1:8765/?view=web`. Enable Verbose console output to see
`console.debug('[writer]', …)`. Set `window.__qaMarker = 1`: an in-place update
preserves it; a full reload clears it. The mock module docstring lists every
control route. Examples, with `S=http://127.0.0.1:8765`:

| Action | Request |
|---|---|
| Edit document-root children | GET `/__mock/document`, edit the HTML, POST it to the same route, then GET `/__mock/emit?kind=content` |
| Edit theme CSS | POST `/__mock/css`, then GET `/__mock/emit?kind=css` |
| Edit a theme-chain file | POST `/__mock/asset?path=extra.css`, then emit a CSS build |
| Scroll to an included edit | GET `/__mock/emit?kind=content&source=1&line=41&file=chapter.md` |
| Scroll to notebook cell 3 | GET `/__mock/emit?kind=content&source=2&cell=3&file=book.ipynb` |
| Both views fail | GET `/__mock/emit?kind=content&web=error&pages=error` |
| Only pages fail | GET `/__mock/emit?kind=content&pages=error&error_kind=compile` |
| Warnings and a nonfatal error | GET `/__mock/emit?kind=content&warning=1`; use `warning=0` to clear |
| No previous good document | GET `/__mock/emit?kind=content&web=error&pages=error&nodoc=1`, then reload |
| Temporary content-fetch failure | GET `/__mock/failroot?on=1`; use `on=0` to recover |
| SSE outage | GET `/__mock/down?ms=16500` |
| Restart at build 1 | GET `/__mock/restart` |
| Read requests and clear log | GET `/__mock/log` |
| Start over | GET `/__mock/reset` |

The mock binds only loopback. `WRITER_ASSETS` can override the repository
assets directory, and `WRITER_CLIENT` can select a client under development.

## Scripted run

The driver lifts presenter's package-free DevTools transport and uses isolated
headless Chrome profiles. It needs Node ≥22 with global `fetch` and `WebSocket`,
Python 3 and Chrome/Chromium. It installs no packages and uses no external
network services. It is separate from `cargo test`.

```sh
node tests/fixtures/live-reload/qa.mjs --out /tmp/writer-live-reload-qa
```

Optional flags: `--only ABCDEFGH`, `--chrome PATH`, `--assets PATH`, and
`--client PATH`. `CHROME_PATH` also selects Chrome. Each selected session
produces a screenshot and detailed log when `--out` is supplied. The runner
prints a PASS/FAIL verdict for each scenario and exits nonzero on failure.

| Session | Coverage |
|---|---|
| A | CSS and theme-chain swaps, loaded-link handoff, visible edit, TOC insertion, deleted anchor, ±1 px screen offsets |
| B | Offscreen edits above/below, source-index and tightest-range selection, cell selection, unchanged KaTeX node and changed rendering, details state, image identity/version requests, title |
| C | Merged parse errors, escaped excerpt/hints, dismissal across reconnect, pages-only error and skipped-view retention, badge, warning/error counts, expansion persistence, recovery |
| D | Outside-root reload, raw-script reload on every content build, stale stored-ID reload exactly once |
| E | Missed builds and restart IDs update in place; retry delays reach 0.5, 1, 2, 4 and capped 5 seconds |
| F | Keyboard exclusions, p/button navigation, remembered view, explicit query override, pages delegation and skipped CSS |
| G | Failed content fetch defers stored ID, next CSS build repairs content, fallback overlay, first good build reload |
| H | Client in head waits for body and discovers the eventual web/pages marker; in-place updates and pages delegation work |

## Real-server setup

T23 repeats items 1–10 and 12–16 against `writer serve`, editing a temporary
copy of the document and theme. For example, after T23 is implemented:

```sh
cargo run -- serve examples/basic/document.md --port 0
```

Use a document long enough to scroll, an include, a notebook, equations and
images. Edit the actual Markdown/notebook and theme files; keep a console
marker and inspect `window.__writerLiveReload.state` plus the network log.
Stop/restart the server for item 13. T24 supplies the real pages-only failure
case in item 11 and adds page rendering checks 17–19.

## Checklist

| # | Action | Expected |
|---|---|---|
| 1 | Edit theme CSS | No reload; `#writer-theme` gets the applied build ID; the loaded old sheet stays until its replacement loads. |
| 2 | Edit visible text | No reload; first visible source block retains its screen offset within ±1 px. |
| 3 | Edit above or below the viewport | Changed block scrolls to the centre; visible edits do not jump. |
| 4 | Insert a TOC above the viewport, or delete the anchor | Reading place stays within ±1 px; deleted anchors use the defined fallback. |
| 5 | Edit an include or notebook cell 3 | Source index selects the right file; tightest covering range or cell is selected. |
| 6 | Edit around an equation, then edit its source | Unchanged rendered node and its attached marker survive; changed source renders new equation content. |
| 7 | Replace one image | Only its versioned URL is requested; the unchanged image retains its DOM node. |
| 8 | Change title, then outside-root template markup | Title changes in place; template edit causes exactly one full reload. |
| 9 | Insert a raw HTML script in the document | Every content build performs a full reload. |
| 10 | Introduce a parse error | One merged `[web, pages]` overlay shows kind, location, safe excerpt and hints; clicking dismisses it. |
| 11 | Fail pages while web succeeds; then edit CSS and fix | Web updates; pages badge and overlay remain through the skipped CSS build; fixing clears them. T22 tests the protocol and delegation; T24 tests actual pages. |
| 12 | Introduce and fix an unresolved reference | Warning count and detailed code/location appear; expansion persists; fixing hides the panel. |
| 13 | Stop/restart server, miss builds, open stale page | Backoff is 0.5/1/2/4 seconds then 5 seconds; no outage reload; reconnect applies missed/restarted builds in place; stale fresh page reloads once. |
| 14 | Use p, button, query and root URL | Views switch; typing/modifiers/prevented keys do not; root remembers the last view and explicit query wins. |
| 15 | Open before any good build and recover | Fallback error is shown; dismissal survives same-build reconnect; first good build reloads into the document. |
| 16 | Inspect console | No uncaught exceptions; client debug messages begin with `[writer]`. |

## Results

### 2026-10-27: mock implementation checks

Environment: macOS 26.7 arm64, Chrome 156.0.8078.48, Node 26.12.0 and
Python 3.15.0. Sessions A–G exercised checklist items 1–16. After correcting
the math fixture to change its raw source as well as `data-tex`, focused B/H
passed 6/6 checks, including the client loaded before body/theme markers.
Focused C/E passed 4/4 checks, including a long error at the overlay width cap.
Observed retry intervals were 511.5, 1009.8, 2010.7, 4010.5, 5009.2 and
5009.7 ms; the JavaScript page marker survived the outage and restart.

The complete frozen A–H run passed **19 checks, zero failures**:

```sh
node tests/fixtures/live-reload/qa.mjs --out /private/tmp/writer-t22-final-qa
```

This run used the repository paths and final client, with isolated browser
profiles and a loopback mock. Console checks found no uncaught exceptions
or unexpected client logs. CSS changes visibly apply, old stylesheets stay
loaded during replacement, and source anchors remain within one pixel.
Screenshots and request/console logs are written per session. The client
also passes JavaScript syntax and release embedded-asset checks.

Real-server checks remain assigned to T23 and actual pages checks to T24.

## T23 real-server acceptance (2026-10-28)

Executed all nine `tests/browser_serve.rs` cases against the real `writer serve`
CLI with Google Chrome 156.0.8078.48 on macOS arm64. This is separate from the
T22 mock-server evidence above. Every case uses an isolated fixture copy,
bounded browser/server waits, real file edits and owned child cleanup.

```sh
env WRITER_REQUIRE_CHROME=1 cargo test --locked --offline \
    --features browser-tests --test browser_serve -- --nocapture
```

Final result: nine passed in 36.34 s. The local transcript is retained at
`target/task-prep/crash-recovery/writer-t23-browser-final.log` (ignored).

| Checklist | Real-server evidence |
|---|---|
| 1–2: CSS and visible text | Reading anchor stays 12.4375 px before/after text and CSS edits; window marker survives; CSS build3 reports pages skipped while persistent client pages status stays ok. |
| 3/5: offscreen main/include/notebook | Main, include source1/line3 and notebook cell3 centers land within 0.454 px of the viewport center. |
| 4: TOC/deleted anchor | TOC growth moves the anchor 11.6875→11.328125 px (−0.3594); deleted-anchor fallback stays 12.328125 px. |
| 6–7: math and images | Unchanged span/KaTeX child identity survives; edited math replaces the ink. Plot width changes 16→24 px with exactly its new versioned request; the control node and URL survive. |
| 8–9: title/template/raw script | Title changes in place; template and raw-script edits produce the expected four top-level loads and one raw-script execution per context. |
| 10/12: errors and warnings | Warning expansion persists; a parse failure gives one merged web/pages overlay with one excerpt, retains the good document and clears after repair. Dismissal survives same-build reconnect. |
| 13: outage/restart/stale tab | Retry intervals 503.7/1004.8/2003.3/4003.0/5004.1/5001.9 ms match the capped backoff. Restart/missed builds apply, and a stale stored id causes exactly one extra reload. |
| 14: views/keys/storage | Typing, modifiers and prevented events suppress shortcuts; key/button navigation and remembered root/explicit view choices work. |
| 15: initial fallback | Repair causes the first-good full reload; the old marker is absent, the document loads once and status becomes ok. |
| 16: console/logs | All cases reject uncaught/runtime/console errors and unexpected debug prefixes. The outage case admits only its expected connection failures. |

The first run passed eight cases; its CSS test incorrectly expected persistent
client status to become skipped. The corrected test observes the actual CSS
SSE event and separately checks the retained client state. Production client
behavior did not change; the final complete nine-case run passes.

Items 11 and 17–19 remain T24: actual pages rendering, page hashes/requests and
pages-only compiler failure/last-good behavior. T23 intentionally uses T21's
empty successful PagesBuilder contract. The CLI suite separately exercises
source/cell event payloads, invalid/missing config/theme/include repair,
versioned assets, math-image registry routes, Host checks, port errors, quiet
output and SIGINT before/after binding.
