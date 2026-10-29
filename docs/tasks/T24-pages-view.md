# T24 — Compiler-backed pages view

Frozen implementation brief, coordinator, 2026-10-28. Scope/API requirements
follow the spec; narrow additive ownership and clarifications are approved below.

| | |
|---|---|
| Milestone / size | M2 / M |
| Dependencies | T21, T23, T32; finish/publish T32 first |
| Primary ownership | `src/serve/pages.rs`, `assets/writer/pages.js` |
| Additive coverage | CLI/server/browser/watch integration, serve fixture template, mock pages routes and QA results |
| Binding sources | PLAN T24, §3.2, §6–7; live-reload.md §1–4, §5.2–5.4, §8–10, Fixtures/Deviations; typst-dialect.md §6, §8; T32 task decisions |

## Goal and boundaries

Replace the successful-empty PagesBuilder stub with real paged Typst compilation
and separate content-addressed SVGs; implement the pages shell client with lazy
images, stable sizing, scroll anchors, zoom, page counter and last-good status.
Use the session's retained World and existing HTTP/SSE state machine. No new
dependency, config/IR/public API, route, browser runtime or external Typst tool.

T21 already owns routes/cache policy and server current/previous sets; T23 owns
WorldSlot, reset/eviction, independent stage errors, warnings, watching, source
positions and panic boundaries. T22 owns live-reload.js, overlays, warnings and
view toggling; T24 should consume those interfaces, not implement a second SSE
client. PDF engine=chrome affects build only: pages always use embedded Typst.

Preserve the existing TypstFile branch using the World's on-disk main; T52 owns
the full `.typ` serve fixtures and end-to-end subset/web acceptance. T24 can unit
test direct TypstFile compilation and existing default-view wiring without
adding a new Typst pipeline. Includes-provider implementation remains T53.

## Existing Rust and pipeline contracts

`src/serve/pages.rs` already provides PageSvg { hash, svg: Bytes, size_pt, key },
PageSet { build_id, pages }, find/hashes/sizes, page_hash and shell_html. Keep
their signatures and wire representations unchanged. The only pending Rust
implementation is the stateful PagesBuilder and its empty-success stub test.

```rust
pub enum PagesInput<'a> {
    Generated { prepared: &'a Prepared, opts: &'a BuildOptions },
    TypstFile { path: &'a Path },
}
pub fn build(
    &mut self, world: &mut WriterWorld, input: PagesInput<'_>, build_id: u64,
) -> Result<(Arc<PageSet>, Diagnostics), BuildFailure>;
```

- Keep one private `previous: Option<Arc<PageSet>>` in PagesBuilder. Compile and
  export into a new local set, replacing the cached successful set only after
  the entire operation succeeds. Error/panic must not replace last-good reuse
  state; server apply_outcome independently retains last-good current/previous.
- Generated loads once through T32's approved crate-private
  `pipeline::pdf::load_with_diagnostics`, preserving successful emitter findings.
  It replaces managed mounts/assets/main/inputs without resetting the World.
  TypstFile uses the current on-disk main and path for error fallback.
- Call compile_document, preserve every mapped compile finding on failure in
  BuildFailure and convert the first fatal finding with into_result semantics.
  Generated error paths prefer span.file, existing compiler path, then original
  input, as T32 does. Keep line/cell/package hints. Do not drop warnings on Err.
- compile_document ALREADY appends world.include_diagnostics(). Despite §4.1's
  older wording, do not append include diagnostics a second time. Combine
  emission and compile findings exactly once; T23 owns prepared findings.
- No world.reset(), recreation, timestamp recomputation or comemo eviction in
  PagesBuilder; T23 already resets once before stages and evicts once per attempt.
  Mounted reads/accesses must remain visible to T23's existing watch refresh.

For every compiled page, compute typst::utils::hash128(page). Reuse a PageSvg
whose key matches ANY page of the previous successful set, independent of its
old index, preserving Bytes storage pointer and hash without re-export. Otherwise
call `typst_svg::svg(page, &SvgOptions::default())`, convert to Bytes and derive
the first 16 lowercase hex characters of BLAKE3. Width/height are frame Abs
to_pt(), rounded to two decimals. Never svg_merged, inline SVG or PDF export.

Pinned 0.15.1 primary sources already confirm Page: Clone+Hash, svg(Page,
SvgOptions)->String and Abs::to_pt. Possible private helpers, all in pages.rs:
compile_input (diagnostics/result boundary), export_page (reuse/export),
rounded_size. No new public Rust module or obsolete compile_pages API.

## Existing shell/routes/session are ready

shell_html already emits the specified toolbar, grey/dark background, centered
single column with gaps, white pages/shadows, escaped title, disclaimer and
pages.js BEFORE live-reload.js. Keep all DOM IDs/data hooks and that script order;
CSS refinements may stay local. No page SVG goes in shell HTML.

T21 page_svg already supports current/previous hash lookup (immutable one-year
cache), unknown/missing version fallback to current index (no-store), malformed
or missing page 404, SVG content type/nosniff/CSP. A hash lookup can match a page
at a different index by design. These semantics and Host checks are not T24 work.

T23 render_pages already passes Generated or TypstFile, catches panics, captures
accessed files and applies independent web/pages outcomes. CSS skips pages;
template edits classify pages and skip web; content runs both. Existing pages
stage/warnings/error transitions should require no session redesign.

## pages.js contract and suggested private split

Expose `window.__writerPages = { apply(pages, buildId), state }`. Startup fetches
`/__writer/state` with cache:no-store. A response is ignored if ANY public apply
ran since that fetch started. Public apply always applies its input, including
18 then 1 after restart; never filter by monotonically increasing build IDs.
live-reload.js already queues/awaits apply for non-skipped pages events and owns
SSE, reconnect IDs, status overlays and view switching. Never throw; logs start
with `[writer]`; storage access may fail and must not break rendering.

Per page, preserve exact §4.3 DOM: div.writer-page, one-based data-page, data-hash;
img alt="Page N", loading=lazy, decoding=async, draggable=false, intrinsic W/H
round(size_pt*4/3), source `/__writer/pages/N.svg?v=HASH`. CSS width/height are
size_pt*4/3*zoom px so every page reserves final space before image loading.

apply records the first page rect.bottom>0 and fraction -top/height, creates or
updates pages in order, changes src/data-hash ONLY when hash changes, updates
sizes, removes excess pages, then restores the anchor index clamped to the new
count with the same fraction. Keep the old decoded image visible until a new
revision loads; avoid eager preloading that defeats lazy loading or stale load
callbacks that replace a newer revision. Empty count shows exactly No pages yet.

On status error retain the last-good pages and show `pages: build error, showing
the last good pages` or `pages: build error, no pages yet`; ok hides the status.
The received error payload already includes last-good count/hashes/sizes. The
client must also behave safely for an initial empty state.

Zoom levels: .5,.67,.75,.9,1,1.1,1.25,1.5,1.75,2,2.5,3 and fit(default).
Fit clamps (clientWidth-32)/max unzoomed page width to [.25,2], recomputes on
resize; choose a finite fallback when no pages exist. Persist choice in
sessionStorage `writer:pages-zoom`. Buttons and +/=, -, 0 obey T22's key rules
(no modifiers/defaultPrevented/input/textarea/select/contenteditable). From fit,
choose the next strictly higher/lower discrete level. Preserve anchor on zoom;
label Fit or rounded percentage. Counter is Page k / count, updated on scroll.

Keep the implementation in a small IIFE with private helpers for capture/restore
anchor, page DOM reconciliation, zoom choice/layout, status/counter and guarded
startup fetch. state layout is private implementation detail beyond exposure.
No new client library, second EventSource or test-only production hook.

## Suggested parallel ownership

- Root: pages.rs builder/cache/diagnostic boundary, private units and fixture
  template/config; freeze any narrow T32 diagnostic helper reuse before edits.
- Agent: pages.js plus mock-server/qa.mjs page route/client cases. Preserve
  existing T22 mock coverage and client hook names.
- Agent: additive tests/cli_serve.rs + tests/browser_serve.rs, real-server QA
  evidence; coordinate watch test ownership separately to avoid shared edits.
- Root runs all Cargo/fmt/browser commands serially. A browser agent may prepare
  assertions first, with actual Chrome verification after Rust/client integration.

## Required tests and browser QA

- Private units: real Generated and TypstFile compile; same key reuses the exact
  Bytes pointer across rebuilds (also moved page), changed page gets new bytes;
  page_hash format; size_pt rounding; shell hooks/escaping; failed compile keeps
  builder last good and complete diagnostics; notebook error maps cell. Replace
  only the stub_returns_empty unit; preserve existing route/type/hash tests.
- Add serve/template.typ exporting writer-doc via the embedded package, plus
  `[pdf] template` in the isolated serve writer.toml. It must remain valid when
  edited for a pages-only build; T34's fuller template export corpus stays later.
- Existing CLI initial pages.count==0 assertion becomes >=2 and lengths equal
  count. Last paragraph edit preserves first page hash; CSS leaves all hashes;
  template edit emits pages kind and web skipped. Raw `{=typst}` unknown call
  fails pages alone with compile location, web succeeds, old hash URL remains
  immutable/200; CSS retains that pages error, fixing clears it. Check actual
  SVG bytes' page_hash, fallback version, invalid indices, notebook cell payload.
- Add html,pdf build-watch regression: CSS rewrites HTML only/PDF mtime stays;
  content rewrites both. Existing process harness is tests/cli_watch.rs, whereas
  spec §9 names tests/watch.rs (now backend-only); prefer extending the existing
  CLI harness, with coordinator ownership note, rather than duplicate it. Preserve
  HTML-only template-ignore regression and portable/bounded waits.
- Browser feature/browser-tests uses T25 Page::{launch,navigate,evaluate,errors},
  existing serial fixtures/owned children, resourceTiming via evaluate and
  WRITER_REQUIRE_CHROME. No DevTools public API extension is needed. Existing
  READY waits for ALL images and will deadlock with intentionally lazy far pages:
  add pages-specific readiness (layout plus visible/nearby loaded images) instead
  of weakening web readiness or eagerly loading the whole document.
- Confirm every page has final intrinsic/CSS dimensions before far-page load;
  some sufficiently distant pages are not requested (allow Chrome's preload
  lookahead, do not assert a rigid visible-only request count). Use enough pages
  and explicit page breaks/equal-height edits for deterministic page 5/page 7 QA.
- On page 7, edit page 5: preserve anchor/fraction (±1 px), re-request only URLs
  for changed hashes, keep unchanged DOM/img identity and src. Count follows
  scrolling. Exercise +/-/0/buttons/fit resize, storage persistence and typing
  exclusions, error last-good status, p/view toggle, 18→1 apply and startup-fetch
  race. ResourceTiming suffices for page URL request evidence; always inspect
  Page::errors and prefixed debug logs.
- Add actual QA results for §8 item11 and 17–19 in docs/dev/live-reload-qa.md,
  distinguishing automated real-server/Chrome evidence from mock-only checks.
  Full `.typ` end-to-end default/pages subset-error case stays assigned to T52;
  record any already-working simple TypstFile smoke explicitly without widening
  that ownership. Keep screenshots/request facts in ignored scratch artifacts.
- Verify existing T23 web/browser regressions still pass with real pages running
  independently, plus required fmt/Clippy/docs/debug/release checks. No timing
  assertion derived from R1's old benchmark; cache pointer/request assertions are
  acceptance, not a promised performance number.


## Approved coordinator clarifications (2026-10-28)

- Keep the existing public types, shell/hooks, routes and session staging.
  Private builder helpers and its last-success cache live in pages.rs only.
- Generated source loads via T32's crate-private load_with_diagnostics. The
  compile_document wrapper already appends include-provider diagnostics: do
  not append them again. Map compiler findings and retain failures as T32 does.
- Add build-watch coverage to the existing process harness tests/cli_watch.rs;
  tests/watch.rs remains the native backend harness. Existing HTML-only tests
  stay; the new case checks HTML/PDF selective output through real processes.
- Add pages-specific browser readiness instead of requiring all lazy images.
  Settled page DOM has one image; a decoded old image may stay temporarily
  beneath a DOM-attached lazy candidate until its new revision decodes.
- Extend existing mock QA sessions additively and run required real Chrome
  checks. Record observed outcomes only after execution. Native .typ full
  build/serve end-to-end remains T52, provider bodies remain T53.
