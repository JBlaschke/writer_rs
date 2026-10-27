# T22 — Live-reload client

| | |
|---|---|
| Milestone | M2 |
| Size | M |
| Depends on | T00 |
| Parallel-safe with | T01 |
| Files you own | `assets/writer/live-reload.js`, `tests/fixtures/live-reload/{mock-server.py,qa.mjs}`, `docs/dev/live-reload-qa.md` (new; lift/adapt presenter equivalents) |

## Goal

Update a served document in place while preserving the reader's position,
unchanged equations, images and disclosure state. Show errors and warnings
for both web and pages views and provide persistent view navigation.

## Read first

- `PLAN.md` §3–§4, §5 T22, §6–§7; `AGENTS.md`.
- `docs/spec/live-reload.md` §1.2–§1.4, §2, §3, §8–§9 and Fixtures.
- `docs/spec/html-output.md` serve-mode markup and positions;
  `docs/spec/math.md` browser render guard.
- Presenter `assets/presenter/live-reload.js`,
  `tests/fixtures/live-reload/{mock-server.py,qa.mjs}` and
  `docs/dev/live-reload-qa.md`.

## API to implement

`window.__writerLiveReload = { connect, disconnect, state, setView }`.
Use one strict ES2020 IIFE and vendored `window.morphdom`; retain the named
presenter connection, signature, stylesheet and update helpers (§3).

## Steps

1. Lift the connection/retry queue, storage guards, fetched document
   signature and stylesheet swapping. Replace Reveal behavior with the
   writer document root and the two-view protocol.
2. Add morph guards for details, math and images; track visible source
   anchors and scroll off-screen edits by source/line or source/cell.
3. Render per-view errors, dismissed-build state, warnings and view badges;
   persist view/warnings choices and support the guarded `p` shortcut.
4. Adapt the Python mock and Node/Chrome DevTools runner to writer markup,
   events and routes; retain the lifted browser/process transport.
5. Record automated mock results and real-server follow-up checks in the
   QA guide. Add JavaScript syntax checks to CI.

## Acceptance criteria

- Mock/browser checks cover live-reload.md §8 items 1–16, including ±1 px
  anchor keeping, exact unchanged equation/image identity, stylesheet load
  ordering, reconnect/restart, stale IDs, per-view errors, warnings,
  persistent choices and fallback recovery. Pages rendering uses a spy
  implementing `__writerPages.apply`; real pages belong to T24.
- No uncaught client exceptions; client logs use `[writer]`.
- JavaScript/Python syntax checks and the complete browser runner pass.
- Formatting, all-target/all-feature Clippy, debug/release tests, rustdoc
  with warnings denied and the basic HTML example pass, locked/offline.

## Out of scope

Rust server/routes/events (T21), session and CLI wiring (T23), real pages
rendering and its JavaScript (T24), watch classification (T20), and the
opt-in Rust browser client (T25). Keep existing math.js unchanged.

## Coordinator notes

- This brief records the frozen contract missing from the original task
  directory. T00 is merged; T22 can proceed independently while T40 CI runs.
- Notebook positions use `[data-source="s"][data-cell="c"]` on one
  positioned block, matching the existing HTML emitter's attributes.
- No dependency, IR/config or Rust module-list changes. CI syntax checks
  and embedded-asset coverage are additive outside the owned paths.
