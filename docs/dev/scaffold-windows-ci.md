# Scaffold Windows CI repair

PR #40's original head `772dc5e` failed two Windows debug tests. The
[PR job](https://github.com/JBlaschke/writer_rs/actions/runs/40895584581/job/123523654135)
and [push job](https://github.com/JBlaschke/writer_rs/actions/runs/40895538963/job/123523492283)
reported the same failures; formatting, Clippy and rustdoc passed.

The existing DOCX test expected exit 1 when an existing regular file was used
as the destination's parent, but Windows returned exit 2. Unix's initial
metadata lookup rejects that path as I/O; Windows reaches the new explicit
parent validator. This branch explanation follows from the source and fixed
test setup; the failed assertion did not print the native metadata errno.

The private directory validator now returns typed `NotADirectory` I/O for a
known non-directory. Single-file ejection retains the resource command's
existing exit-1 behavior. Tree creation alone maps that typed condition to a
usage error, preserving exit 2 for invalid project/theme directory arguments.
The public DOCX assertion is unchanged. Private controls preserve direct and
nested file-parent failures, missing-component traversal and tree refusal,
including unchanged bytes and no unexpected directory creation.

The package test's second failure was its reason string. Windows resolved
`new/../packages` to an already-owned version and refused before writing;
POSIX reached the explicit missing-directory traversal check. The test now
requires the actual reason per platform, retaining `Error::Config`, the
complete directory/file inventory, absence of `new` and original package
implementation bytes. No package implementation or overwrite guard changed.

## Validation

On macOS arm64, 88 CLI units and 53 public cases pass after the repair. The
public run includes the original DOCX suite, all four scaffold variants,
resource ejection, package preflight, original package compilation and the
new diagnostic/compiler controls. Configured formatting, all-target/all-feature denied-warning Clippy and
all-feature denied-warning rustdoc pass. Windows CI must still rerun the two
repaired assertions and their following no-write checks; local macOS evidence
does not claim a Windows runtime result.

The milestone's earlier complete debug/release measurements are recorded in
`build-times.md`. They precede this narrow portability repair; the focused
post-repair runs are separate evidence and do not count repeated cases as
additional full-suite passes.
