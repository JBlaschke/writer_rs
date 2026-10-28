# Watch CI recovery (2026-10-28)

The macOS release job for the T20 merge (`1bc404f`) failed
`pages_only_changes_with_no_requested_target_are_ignored_before_reporting`.
Its batch contained the temporary project directory and `template.typ`, so it
was classified as content and rebuilt HTML instead of skipping a pages-only
change. Linux, Windows and the macOS debug suite passed that run.

[Failing workflow](https://github.com/JBlaschke/writer_rs/actions/runs/40719157537)
([macOS job](https://github.com/JBlaschke/writer_rs/actions/runs/40719157537/job/122924982212)).

macOS FSEvents can deliver a directory creation from before watch startup.
The first build creates an output file and changes the parent's ctime, so that
timestamp cannot distinguish the historical event from a new directory. The
CI transcript does not include the raw notify kind; a deterministic regression
reproduces the observed classification using that stale-create sequence.

The watcher now snapshots existing planned directory identities on macOS
(device, inode and birth time). It ignores a creation event only when the path
still names that same directory. New/replaced directories, removals, renames,
root changes and rescans retain their existing handling. Missing identity
metadata falls back conservatively. Other platforms keep their event behavior.

Local validation also reproduced concurrent watch-loop child startup exhausting
the bounded wait. This repair backports the existing PR22 harness guard across
one child's complete lifetime; deadlines and production startup are unchanged.

Validated on macOS arm64, Rust 1.99.1:

| Check | Result |
|---|---|
| Watcher unit tests, including three new regressions | 24 passed |
| Real native-backend integration | 20 passed |
| Serialized watch-loop integration | 17 passed, one private driver ignored |
| Full debug suite | 1,185 passed; 12.17 s build, 179.95 s wall |
| Full release suite | 1,184 passed; 287 s build, 340.60 s wall |

Formatting, all-target/all-feature Clippy and rustdoc with warnings denied pass.
The repair is based directly on main and does not depend on pending feature PRs.
