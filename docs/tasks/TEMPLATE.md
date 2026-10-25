# Txx — <title>

| | |
|---|---|
| Milestone | M? |
| Size | S / M / L |
| Depends on | T?? (comma-separated; `—` for none; ranges are not allowed) |
| Parallel-safe with | the tasks of the same wave (see the board) |
| Files you own | exact paths (from PLAN.md §3.2); add `(new)`, `(lift from presenter_rs/<path>)` or `(replace stub)` |

## Goal
Two to four sentences: what exists when this task is done and who consumes it.

## Read first
- `docs/spec/<file>.md` §n (the sections this task implements — "this is your spec")
- `docs/research/<file>.md` §n (verified API hints to copy)
- `presenter_rs/<path>` (what to lift) — only when the task lifts code

## API to implement
```rust
// exact signatures from the spec, with module paths
```

## Steps
1. Numbered, concrete, in build order; each step names the file it touches.
2. For lifted code: "copy X → Y; rename …; delete …; keep tests …".
3. …

## Acceptance criteria
- Every criterion is checkable: a named test, a snapshot, a CLI invocation with its expected
  output, or a documented manual check (say which).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
What a tempted agent must leave to other tasks (name them).

## Hints (verified)
- Exact crate versions, function signatures, snippets and measured numbers, each with its
  source (`docs/research/R?-….md §n` or the crate file read).
