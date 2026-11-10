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

Expect a release to take three to four and a half hours (the second dry run
took 4 h 21 min, gated by Intel macOS). Each native job runs the release
tests first, and thin LTO links each of the roughly 90 test binaries: CI's
`cargo test --release` took up to 90 min on Linux, 130 on macOS and 174 on
Windows in November 2026. In the two release dry runs the Intel macOS
runner took 156 and then 232 min, the longest of the six. The job then builds
the archived executable again from scratch with the `dist` profile's fat LTO,
which took 9-26 min on the hosted runners (longest on Intel macOS, whose job
took 260 min in all). The build job allows 350 min and the test step 300,
below GitHub's 360-minute limit for a hosted job.

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
release tests run before the executable is archived. They use the `release`
profile: opt-level 3, thin LTO, codegen-units 1 and strip. The archived
executable uses the `dist` profile, which inherits `release` and switches to
fat LTO. On main `2161bc1` (4 November 2026) fat LTO made the executable 5.6%
smaller on arm64 macOS and 4.7% on x86_64 macOS, and both design templates
rendered byte for byte the same; the x86_64 Windows executable needs it to
get under the size gate below. Keep fat LTO out of `release`, or the test
step applies it to every test binary. Both Windows targets keep the static
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
a hard per-target build gate, not a compressed-size threshold. The first
hosted dry run (4 November 2026, run 41812146321) still built with thin LTO:
x86_64 Windows failed the gate at 68,976,128 bytes, and x86_64 Linux reached
65,711,440. The second dry run (run 41908189568, `cf9dcf5`) built the
archived executable with the `dist` profile's fat LTO and passed on all six
targets. Fat LTO saved 2.9% on x86_64 Windows, which now sits at 66,987,008
bytes, only 121,856 bytes under the gate; the other targets saved 3.4-5.7%
and keep 4.4-12.8 MB of headroom. Anything that grows the x86_64 Windows
executable by more than about 119 KiB fails a release, so check its size
before adding dependencies or embedded assets.

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
`license_file` are preserved. Registry crates' source trees, vendored or
unpacked, are searched recursively;
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

The shared guard preserves exact package declarations and the reviewed
Cargo.lock checksum, then proves the crate source that Cargo built from. The
proof depends on where `cargo metadata` found the package:

- **Vendored source.** This repository builds from the crates committed under
  its root `vendor/` directory. The package root must be a direct child of the
  directory that the workspace's own `.cargo/config.toml` substitutes for
  crates.io, named `<name>` or `<name>-<version>`. Its `.cargo-checksum.json`
  must give the reviewed checksum as `package`, the directory must hold
  exactly the files that record lists, each with its listed SHA-256, and the
  SHA-256 of that file table must equal the pin's `vendored_files`. This
  proves that every file of the copy is the one reviewed when the pin was
  made.
- **Registry source.** In a checkout that does not vendor, the root is
  `<registry>/src/<index>/<name>-<version>`. The cached
  `<name>-<version>.crate` must hash to the reviewed checksum, and its one
  regular VCS record must equal the unpacked one. This proves the published
  archive and the VCS record taken from it; the other unpacked files are not
  compared.

A root in any other place fails. In both layouts the proven VCS record must
name the reviewed clean revision and package path.

A vendored tree keeps no archive, and its checksum record cannot stand in for
one. Cargo (checked with 1.99.1) compares `package` with Cargo.lock and each
listed file with its listed hash, but it accepts unlisted files, an emptied
`files` table, and a file edited together with its listed hash: a copy changed
along with its record still builds. The guard therefore takes nothing in the
committed record on trust. It checks the record against the bytes on disk and
against the pin, so a vendored copy of a pinned crate that differs from the
reviewed copy in any file fails the release until `VENDORED_FILES` in
`scripts/release-notices.py` changes too, as a changed archive fails until its
checksum pin changes. This holds even when the copy gains a notice file of its
own: the published crate has none, so the guard still runs. Unlike the
registry proof, the vendored one hashes no published archive during the
release. It rests on the comparison made when the pin was reviewed.

The 21 `vendored_files` values were reviewed on 2026-11-10. For each crate the
archive in the local Cargo cache had the reviewed checksum, every vendored
file equalled the archive member of the same path, and a fresh `cargo vendor`
of those archives wrote the same checksum records. The only archive member
without a vendored file is `assets/artifacts/.gitattributes` in mitex-spec-gen
0.2.4, because `cargo vendor` leaves out `.gitattributes` and `.gitignore`
files.

`vendored_files` is the SHA-256 of the file table as a `sha256sum` listing:
one `<sha256>  <path>` line per file, sorted by path, without
`.cargo-checksum.json`. To compute it from a vendored directory:

```sh
cd vendor/<name>
find . -type f ! -path ./.cargo-checksum.json | sed 's|^\./||' | LC_ALL=C sort \
    | tr '\n' '\0' | xargs -0 shasum -a 256 | shasum -a 256
```

A failing guard prints the value it computed. The table depends on which files
`cargo vendor` keeps, so vendoring again with another Cargo version can change
it although the crate is the same. Pin a new value only after comparing the
copy with the published archive: download
`https://static.crates.io/crates/<name>/<name>-<version>.crate`, check that
its SHA-256 is the reviewed checksum, unpack it and compare the two trees.
Only `.cargo-checksum.json` and the files `cargo vendor` leaves out may
differ. Then change `VENDORED_FILES` and the row's `vendored_files_sha256` in
release-notice-sources.json; a helper test keeps the two equal.

The helper tests also run the guard on this checkout's own `vendor/`,
`Cargo.lock` and assets for every locked pin. They are the first job of every
release and dry run, and they run on pull requests and pushes to main that
touch the helpers, `vendor/`, `Cargo.lock` or `.cargo/config.toml`. A copy
that drifts from its pin therefore fails before the release tests start, not
after them.

The guard covers the 21 pinned crates only. The notice files of the other
linked crates are copied from the source tree Cargo reports, as before. In a
vendored tree those are the committed bytes: Cargo checks them against the
committed record, and nothing compares them with the published archives.

Every notice's runtime hash is checked before copying. Release-only sources are
canonically confined beneath the helper checkout's docs/release/notice-sources,
while metadata's workspace root supplies the lock, the vendoring configuration
and the crate sources. Sources are never
fetched at runtime. Each selected crate keeps its own notice subtree, and
unselected pins add nothing. Other missing source/notice evidence fails.
The release-only legal files use -text -whitespace attributes so upstream CRLF,
trailing spaces and final bytes survive checkout without entering --licenses.
`vendor/**` has the same attributes, so the bytes behind the pins survive
checkout on every runner.

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

The first six-native hosted dry run (4 November 2026, run 41812146321)
failed the size gate on x86_64 Windows. The second (run 41908189568, `cf9dcf5`,
`dist` profile) built, smoke-tested and verified all six archives and their
SHA256SUMS. No tag/release was created, site deployed, binary signed or
package published. The site is a
portable artifact with no assumed hostname or release-download URL.
