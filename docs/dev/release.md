# Release artifact preparation

This workflow builds reviewable archives and a static site artifact. It does
not create GitHub releases, tags or deployments, change remote settings, sign
binaries or notarize them. Publication is a separate authorized action after
actual artifacts/results are reviewed. Cargo remains `publish = false`.

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
changes. Its manual artifact mode builds the selected existing revision using
the exact checked-out SHA as `WRITER_GIT_SHA`. An optional existing tag must be
`v<Cargo.toml package.version>` and resolve to that same SHA; validation never
creates a tag. An empty revision input uses the workflow revision. The source
revision must contain these helper scripts. Prepared notes are selected from
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
Checkout@v7 and rust-cache@v2 follow existing repository CI. Workflow token
permissions are `contents: read`; no release/Pages/deployment token is needed.
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

The T75 scripts/workflows initially enter as unexecuted drafts. Add measured
per-target rows to `docs/dev/build-times.md` only after actual native runs;
retain the run/revision/toolchain identity. One earlier local T70 working-tree
macOS arm64 release binary was 57,019,632 bytes on 2026-10-29 (T61 head22adf47
plus uncommitted T70), which is under the budget. That is not a final T75 size
or evidence for another architecture.

The project site is a separate artifact-only build over the complete T74
source tree. Its generator must validate links/prefixes, map repository source
URLs to the supplied immutable revision and keep code/help/default-config bytes
intact. No hostname, release download URL or deployed-site status is assumed.
