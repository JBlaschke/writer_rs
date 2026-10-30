# Releases

Pushing a version tag publishes a GitHub Release with six native archives and
`SHA256SUMS`, then redeploys the [website](site.md) so its download table
names the new release. The workflow does not sign or notarize binaries, and
Cargo remains `publish = false`.

## Cutting a release

1. Set `version` in `Cargo.toml` (and refresh `Cargo.lock` with a build).
   Write the release notes in `docs/release/NOTES-<version>.md`; they become
   the release's description. Merge both to `main`.
2. Optionally dry-run: Actions → Release → Run workflow on `main`. It builds,
   smokes and checksums all six archives as run artifacts and publishes
   nothing.
3. Tag the merged commit on `main` and push the tag:

   ```sh
   git tag -a v0.1.0 -m "writer 0.1.0"
   git push origin v0.1.0
   ```

4. The Release workflow verifies that the tag is `v<Cargo.toml version>` and
   names the checked-out commit, runs the release tests and builds on all six
   runners, verifies the archives and `SHA256SUMS`, and publishes the release.
   A tag with a pre-release suffix (`v0.2.0-rc.1`) makes a pre-release, which
   the website's download table skips: it shows GitHub's latest release.
5. Its last step starts the Site workflow on `main`. If that step only warns,
   run the Site workflow by hand.

If a build fails, nothing is published: fix the problem on `main`, delete the
tag (`git push --delete origin v0.1.0` and `git tag -d v0.1.0`) and tag the
fixed commit. If publishing itself fails, re-run the failed job.

## Native build matrix

| Rust target | GitHub-hosted label | Archive |
|---|---|---|
| aarch64-apple-darwin | macos-15 | tar.gz |
| x86_64-apple-darwin | macos-15-intel | tar.gz |
| x86_64-unknown-linux-gnu | ubuntu-24.04 | tar.gz |
| aarch64-unknown-linux-gnu | ubuntu-24.04-arm | tar.gz |
| x86_64-pc-windows-msvc | windows-2025 | zip |
| aarch64-pc-windows-msvc | windows-11-arm | zip |

The labels were checked on 2026-10-29 against the official
[runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
and [runner-images inventory](https://github.com/actions/runner-images/blob/main/README.md).
This confirms listed runner availability, not successful Writer artifacts.
Ubuntu 24.04 is intentional: GitHub's
[Ubuntu 22 retirement announcement](https://github.com/actions/runner-images/issues/14254)
starts deprecation on 2026-09-17 and retires it on 2027-04-17.

Linux artifacts are built/tested on Ubuntu 24.04. Each native smoke report
records actual required GLIBC symbols from `readelf --version-info` and runtime
libraries from `ldd`; no older-system compatibility floor is inferred from
those labels. Other OS minimum versions are not established by this matrix.
There is no musl, 32-bit, universal macOS or package-manager distribution here.

## Revision and tools

`.github/workflows/release.yml` runs pure helper tests on relevant PRs/main
changes. A pushed `v*` tag builds the tagged commit and publishes it. The
manual artifact mode builds the selected existing revision and publishes
nothing. Both use the exact checked-out SHA as `WRITER_GIT_SHA`. The tag
(pushed, or the optional dispatch input) must be `v<Cargo.toml
package.version>` and resolve to that same SHA; validation never creates a
tag. An empty revision input uses the workflow revision. The source revision
must contain these helper scripts. Release notes are selected from
`docs/release/NOTES-<version>.md`; missing notes fail rather than borrowing a
different version's history.

The workflow uses the existing stable Rust toolchain and locked Cargo inputs,
plus stdlib Python 3.13 for packaging (helpers require Python >=3.11). Native
release tests run before the executable is archived. The profile stays opt3,
thin LTO, codegen-units1 and strip=true. Both Windows targets keep the static
CRT configuration in `.cargo/config.toml`; do not replace its flags through
`RUSTFLAGS`/`CARGO_ENCODED_RUSTFLAGS` overrides.

Official action usage was checked on 2026-10-29:
[setup-python@v7](https://github.com/actions/setup-python),
[upload-artifact@v7](https://github.com/actions/upload-artifact), and
[download-artifact@v8](https://github.com/actions/download-artifact).
Checkout@v7 and rust-cache@v2 follow existing repository CI. The workflow
token is `contents: read` everywhere except the publish job, which needs
`contents: write` for the release and `actions: write` to start the Site
workflow.
No extra Python packages, cargo-about, cargo-dist, archive CLI, Chrome, TeX or
Word installation is required for release smoke. Linux inspection requires
the runner's `readelf` and `ldd`; a missing inspection tool fails explicitly.

## Archive contract

Each `writer-v<version>-<target>.tar.gz` or `.zip` has one matching root:

```
writer-v<version>-<target>/
  writer                         # writer.exe on Windows
  LICENSE
  README.md
  THIRD-PARTY-LICENSES/
    EMBEDDED.txt
    assets/<embedded notice path>
    crates.txt
    crates/<name>-<version>/<notice file>
```

The uncompressed executable must be at most 67,108,864 bytes (64 MiB). This is
a hard per-target build gate, not a compressed-size threshold.

`EMBEDDED.txt` is exact `--licenses` output from the executable being archived.
All embedded notice paths are copied byte for byte under `assets/`, including
the full font/data NOTICE, OFL/GUST files, both two-face listings, CSL rights,
Typst packages, the derived table's license and source pins. CRLF/final spaces
must survive. The collector reconstructs the binary's exact header/separator
encoding and fails when source copies differ from its stdout.

The linked runtime crate set comes from the same target/configuration:

```sh
cargo --color never metadata --locked --format-version 1 --filter-platform "$TARGET"
cargo --color never tree --locked --target "$TARGET" -e normal,no-proc-macro \
    --prefix none --charset ascii --no-dedupe --format '{p}'
```

Every tree identity must map uniquely into metadata; missing or ambiguous
entries fail. `crates.txt` is sorted with stable registry URLs/workspace-relative
paths. Matched LICENSE/COPYING/NOTICE/UNLICENSE files and explicit Cargo
`license_file` are preserved. Registry source trees are searched recursively;
workspace/path crates use their own root notices plus explicit license_file,
without sweeping build/VCS/recovery trees or duplicating embedded assets.
A missing notice receives generic LICENSE-MIT only if the parsed Cargo
expression admits plain MIT alone. Cargo's legacy slash means OR; AND keeps
every obligation, WITH-qualified terms do not qualify, and SPDX precedence and
parentheses apply. Canonical MIT identity is case-insensitive, operators are
case-sensitive, and the declared expression remains unchanged in crates.txt.
This is syntax/choice evaluation rather than a complete SPDX registry validator.
See [Cargo's convention](https://doc.rust-lang.org/stable/style-guide/cargo.html#metadata-conventions),
[Cargo license operators](https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields)
and [SPDX expressions](https://spdx.github.io/spdx-spec/v2.3.1/SPDX-license-expressions/).

Only the exact 21 reviewed source-copy records in
[release-notice-sources.json](release-notice-sources.json) may fill other missing
published notices: six mitex 0.2.4 crates reuse the verified existing embedded
license; Roman 3.1.0 retains its full upstream LICENCE.rst; 13 Typst 0.15.1 crates
require the verified existing LICENSE plus the separate full Typst NOTICE;
selectors 0.41.0 requires exact official MPL text, actual source header and
source provenance. This is a source-copy policy, not non-MIT license generation.
The selectors row keeps its normal name/version/MPL expression and sorted
one-row format, using the pinned full source archive URL in its existing URL
field; a missing or landing-only URL fails.

The shared guard preserves exact package declarations, Cargo.lock and actual
cached archive checksums, exact archive/source VCS bytes and clean revision/path.
Every notice's runtime hash is checked before copying. Release-only sources are
canonically confined beneath the helper checkout's docs/release/notice-sources,
while metadata's workspace root supplies lock/cache evidence. Sources are never
fetched at runtime. Each selected crate keeps its own notice subtree, and
unselected pins add nothing. Other missing source/notice evidence fails.
The release-only legal files use -text -whitespace attributes so upstream CRLF,
trailing spaces and final bytes survive checkout without entering --licenses.

Packaging sorts member paths, normalizes uid/gid/names/modes/timestamps, omits
host metadata and fixes gzip/ZIP header inputs. The native job packs the same
payload twice and compares complete archive bytes. This is same-input archive
determinism, not a claim that Rust/linker output is bit-identical across host
images or toolchain versions. Executables/directories are0755, other files0644.

The bundle job requires exactly the six named archives, writes sorted ASCII
SHA256SUMS, verifies those hashes and includes prepared notes. Checksums are
unsigned. Per-target reports are separate workflow artifacts with actual binary
size/hash, archive hash, Python/runner details and Linux inspection output.
They are not silently converted into a support or release-history claim.

## Native smoke

The smoke helper unpacks the archive after rejecting path escapes, duplicates,
links and non-file members, checks its exact top-level layout and native
ELF/Mach-O/PE architecture, and executes that unpacked binary in a fresh disposable working directory.
It never moves, renames or removes checkout data. The native release tests
include the existing embedded-asset assertions. This smoke does not separately
prove absolute source paths were inaccessible: changing cwd alone would not
rule out a debug binary finding checkout assets.

The helper checks exact version/build id, help and embedded notices. It creates
a real bibliography scaffold, checks all five outputs with denied warnings,
builds HTML/PDF/DOCX/LaTeX/Typst with fixed timestamp and embedded fonts/packages,
validates file signatures/sidecars, then compares complete repeated output bytes.
A second native Typst scaffold builds its original source to PDF. No notebook
kernel or external renderer/compiler is invoked.

Before accepting artifact results, inspect ordinary repository CI as well.
Manual Word/LibreOffice/Google Docs QA, optional browser/TeX tests, signing and
notarization retain their separate documented boundaries. Chrome-based PDF is
currently unsupported (T77). Do not label those
activities complete because the default smoke passed.

## Validation status

Local acceptance passed on 2026-10-29, using source revision
`5e3062d913d514c87621d4c2f2d391382b8d4b69`, macOS 26.7 arm64, Rust 1.99.1
and Python 3.15.2. The binary identifies that full revision; packaging uses its
commit epoch `1793294143`. Later acceptance-record edits do not change the
artifact's source identity. Timings are in [build-times.md](build-times.md).

| Local native result | Measured value |
|---|---|
| Target | `aarch64-apple-darwin` |
| Uncompressed executable | 57,153,472 bytes; below 67,108,864 |
| Exact notices | 420 linked-crate rows/directories; 749 notice files |
| Archive | `writer-v0.1.0-aarch64-apple-darwin.tar.gz`; 28,068,371 bytes |
| Same-input repeated archive | Complete bytes identical |
| Unpacked native smoke | Passed; all five outputs, repeated bytes and native Typst PDF |

Binary SHA-256:
`c5e86a4dc47d411db0e97964ab78b1c22dcfe35baddcef2b132548555664ae98`.
Archive SHA-256:
`22a5b74e8e4e4aa121ec3cead971e24b29411bdafbdeed2e230339c66b10d3c9`.

The matching release binary builds both relative and `/writer_rs/` site
artifacts. Each has 14 HTML pages and 17 files; the relative artifact is
3,042,863 bytes. Production validation passes 149 local references and 20
fragments, with 114 IDs and 44 data URLs. Exact canonical rules and source
WOFF2 bytes prove 80 inactive legacy font fallback occurrences; unrelated
missing resources remain errors. Local Git object/byte checks verify all 24
repository source URLs against the recorded commit and all 1,324 tracked
source files. No remote GitHub URL or external anchor is fetched.

Actual help, default-config and basic-example code/style blocks match raw
Writer output byte for byte. The native archive/site run preserves every
tracked source hash and the binary hash. Local detailed reports/inventories
are retained under ignored
`target/task-prep/crash-recovery/t75/native-final-v2/`; the workflows produce
their own independently measured artifacts/reports.

All 62 pure helper cases pass, as do formatting, denied-warning Clippy and
rustdoc, JavaScript syntax and the pinned Unicode-table check. All 1,954
distinct debug cases are verified across the complete run and an unchanged
20-case watcher rerun. The initial existing cross-thread watcher timeout
remains unexplained; no deadline/assertion changed. The complete release
suite passes 1,953 cases in one run, with three intentional ignores per mode.
The task brief records the exact debug/final-source boundary.

The six-native manual workflow is still pending on every hosted runner;
this local arm64 result does not establish a hosted matrix result or an
aggregate six-archive SHA256SUMS. No workflow was dispatched, tag/release
created, site deployed, binary signed or package published. The site is a
portable artifact with no assumed hostname or release-download URL.
