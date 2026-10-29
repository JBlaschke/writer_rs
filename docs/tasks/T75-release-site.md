# T75 — release artifacts and project site

Implemented on `codex/release-site`, based on T74 `63150e9`. Local acceptance
is complete; the hosted six-native run and publication retain their separate
operational boundary. This finishes the core implementation milestones.
The user selected stopping here, leaving T76–T79 stretch work unimplemented.

## Implementation

The release workflow provides six native archive jobs, exact embedded and
linked-crate notices, a 64 MiB executable gate, repeated archive-byte checks,
safe unpacking and native smoke. Only an actual six-archive set can produce
aggregate SHA256SUMS. Metadata identifies an existing full revision and fixed
epoch; an optional existing version tag must name that revision.

Notice assembly uniquely joins the target's runtime tree to locked metadata.
It preserves actual legal bytes, evaluates plain-MIT choices with parsed
operators/grouping, and guards the 21 reviewed source-copy exceptions through
declarations, lock/archive checksums, VCS identity/path and notice hashes.
Missing or changed evidence fails without runtime downloads. Release-only
legal copies stay outside embedded assets and retain their upstream bytes.

The site workflow builds 14 pages from the complete guide and real examples.
Only URL attributes are rewritten; emitted code/help/default/CSS stays intact.
Local links/fragments and mount prefixes are validated. KaTeX's intentional
legacy fallbacks are exempt only inside exact canonical rules with matching
source WOFF2 bytes. Asset and root-license changes trigger site validation.
Both workflows upload artifacts with read-only repository permissions.

The implementation changes no Rust API, dependency/version, release profile
or Windows CRT setting. Read `../dev/release.md` and vendored-assets sections
6 and 8 for the archive and notice contracts.

## Acceptance

- All 62 pure cases pass: 12 archive/metadata/smoke controls, 34 notice cases
  including real checked-in legal payloads, and 16 site controls.
- Formatting, all-target/all-feature denied-warning Clippy and all-feature
  denied-warning rustdoc pass. JavaScript syntax and the pinned Unicode table
  also pass; the latter uses hash-checked local source archives.
- At implementation commit `b4e80c0`, the complete debug run passes 1,953
  cases with one existing cross-thread watcher timeout and three ignores.
  The unchanged 20-case watcher suite then passes. All 1,954 distinct debug
  cases are verified without duplicate counting or a clean-full-run claim.
- The subsequent four-script layout changes have identical Python ASTs;
  the only functional review change expands the site workflow's input paths.
  All 62 helper cases pass again. No Rust source or assertion changes between
  the debug run and final source `5e3062d`.
- At final source, all 1,953 release tests pass in one complete invocation,
  with three intentional ignores, including every original watcher test.
- The actual macOS arm64 release archive passes size, exact-notice assembly,
  complete repeated bytes, architecture and unpacked-binary smoke. All five
  output formats, repeated output bytes and native Typst PDF succeed.
- Actual relative and `/writer_rs/` sites pass production validation. Local
  commit objects verify all 24 repository source URLs and 1,324 tracked file
  bytes. Representative help/default/example code and CSS blocks are exactly
  equal to fresh raw Writer output. Source and binary hashes remain unchanged.

Exact sizes, hashes, identities and timing measurements are recorded in
`../dev/release.md` and `../dev/build-times.md`. These are local committed-source
results, separate from the earlier preliminary debug/site preparation.

## Remaining operational checks

The artifact workflow must still run on all six hosted targets and verify
the aggregate archive/checksum set. The local arm64 run is not a hosted job.
No dispatch, tag, release, deployment, signing, notarization or package-manager
publication occurred. Browser/TeX/manual office QA keeps its documented scope;
changing smoke cwd alone does not prove source paths inaccessible.
