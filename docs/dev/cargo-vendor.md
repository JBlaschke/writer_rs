# The Cargo vendor check

The workspace builds from `vendor/`: `.cargo/config.toml` replaces crates.io
with that directory, which holds every crates.io package of `Cargo.lock` as
`cargo vendor` unpacked it. Cargo checks a vendored file only against the
`.cargo-checksum.json` beside it, and only when it compiles the crate anew.
That record is committed with the files, and Cargo itself writes into it that
it guards against accidental changes only. A commit that edits a file together
with its record, or adds a file that no record lists, still builds.

`scripts/check-cargo-vendor.py` compares the tree with the archives that
crates.io published. `.github/workflows/cargo-vendor.yml` runs it for pull
requests and pushes to `main` that change `vendor/**`, `Cargo.lock`,
`.cargo/config.toml`, `.gitattributes`, the script, its tests or the workflow
itself, and on "Run workflow" (Actions → Cargo vendor).

## What a pass proves

For the checkout it ran on:

- **The crates are the locked ones.** The crate directories under `vendor/`
  are exactly the crates.io packages of `Cargo.lock`, and `vendor/` holds
  nothing else. A directory's package is the name and version in its
  `Cargo.toml`, which is how Cargo reads a vendored directory. The directory
  is named `<name>` or `<name>-<version>`.
- **The archive is the locked one.** The file at
  `https://static.crates.io/crates/<name>/<name>-<version>.crate` has the
  SHA-256 that `Cargo.lock` records, and the record's `package` field holds
  the same value. The checksum, not the connection, ties the downloaded bytes
  to the lock file.
- **The files are the archive's.** Every file of a crate directory except the
  record has exactly the bytes of the archive member at the same path. The
  directory holds no file that the archive lacks, and no link.
- **Nothing is missing.** Every archive member is in the directory, unless
  `cargo vendor` leaves it out (next section).
- **The record is complete.** It lists exactly the files that are there, each
  with its SHA-256. Cargo accepts a record that lists fewer.

So what a build reads from `vendor/` is, byte for byte, what crates.io
published under the names, versions and checksums in `Cargo.lock`.

## Members that `cargo vendor` leaves out

An archive member may be absent only where `cargo vendor` itself skips it. In
Cargo 1.99.1 (commit `5f94df47`), `vendor_this` in `src/ops/cargo_vendor.rs`
and `unpack` in `src/sources/registry/mod.rs` skip

- a path with a `.git` component,
- a file named `.gitattributes` or `.gitignore`, at any depth, and
- a file named `.cargo-ok`.

Everything else is unpacked, `Cargo.toml.orig` and `.cargo_vcs_info.json`
included. The rule depends on the Cargo version: Cargo 1.89 began to unpack
the archive directly and to keep `*.orig` and `*.rej` files, and Cargo 1.94
began to skip the two Git files below the top level, where older versions kept
them. The check does not need to know which Cargo made the tree: a member
that may be absent is compared like any other file when it is there.

## What a pass does not prove

- **That a crate can be trusted.** The check compares with crates.io and
  reviews no code. A version that was published with a flaw, or with malice,
  passes.
- **That `Cargo.lock` names the intended versions.** A change that moves a
  package to another published version, with that version's checksum and
  files, passes. The lock file's diff still needs review.
- **Anything about a revision it did not run on.** A change to other paths
  starts no run, a tag push starts none, and the release workflow does not
  repeat the check. "Run workflow" checks any branch or tag.
- **File modes and empty directories.** Names and bytes are compared, nothing
  else.
- **Packages from anywhere but crates.io.** A git dependency or a package of
  another registry fails the check, because there is no crates.io archive to
  compare it with. A path dependency lives outside `vendor/` and is not
  looked at.
- **How a build is configured.** The check finds the directory through the
  checkout's `.cargo/config.toml`. Cargo also reads configuration from parent
  directories, its home directory, the environment and the command line.
- **`assets/vendor/`.** Those web assets, fonts and Typst packages come from
  `scripts/vendor.sh`, and CI's `vendor-drift` job compares them.

## Running it

```bash
python3 -I scripts/check-cargo-vendor.py
```

The script needs Python 3.11 or newer and nothing else, and never runs Cargo.
It fetches every archive with a User-Agent that names only the tool, holds
each in memory and writes no file. The workflow caches nothing, so every run
compares against a fresh download.

After `cargo vendor` the archives are already in Cargo's registry cache.
`--archive-dir` reads `<name>-<version>.crate` files from such a directory,
uses one only if its SHA-256 is the checksum in `Cargo.lock`, and downloads
the rest:

```bash
python3 -I scripts/check-cargo-vendor.py \
    --archive-dir ~/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f
```

The name of that directory can differ between Cargo versions. A Python
without a certificate store cannot download and fails at the first archive;
on macOS that is a python.org build whose `Install Certificates.command` has
not been run.

The script's own tests build their archives in memory and reach no network:

```bash
python3 -I scripts/test-check-cargo-vendor.py
```

## When it fails

The run names each crate directory with what is wrong in it and ends with
status 1.

- `differs from the archive`, `not in the archive`, `missing, but in the
  archive`, or a line about `.cargo-checksum.json`: the committed directory is
  not what crates.io published. Vendor again, and find out how the difference
  got in. Do not edit the record to fit.
- `the archive has SHA-256 …, Cargo.lock has …`: the lock entry does not
  belong to the published archive.
- `is not vendored` or `not a crates.io package of Cargo.lock`: the lock file
  and `vendor/` are out of step. Vendor again.
- `no archive to compare with`: a download failed, which says nothing about
  the tree. The script tries an archive up to three times and, once one has
  failed, asks for no further archive; the summary counts the crates it
  skipped. Run it again.

## Vendoring again

After a dependency change, make `vendor/` again from `Cargo.lock`.

1. Bring `Cargo.lock` to the new state. While `.cargo/config.toml` replaces
   crates.io, Cargo resolves against `vendor/` alone: `cargo update` finds
   nothing newer, and `cargo add` does not find a crate that is not vendored.
   Cargo reads that file from the current directory and its parents, not from
   the manifest's directory, so run the resolving command from a directory
   outside the checkout:

   ```bash
   cargo update --manifest-path /path/to/writer_rs/Cargo.toml --package <crate>
   ```

2. In the root of the checkout, with Cargo 1.94 or newer:

   ```bash
   rm -rf vendor
   ```

   ```bash
   cargo vendor --locked
   ```

   `cargo vendor` ignores the `[source]` tables unless it is given
   `--respect-source-config`, so it reads crates.io and Cargo's registry
   cache, never the old `vendor/`. It prints the two tables that are already
   in `.cargo/config.toml`. Deleting first matters when the old tree may have
   been changed: `cargo vendor` leaves an existing `<name>-<version>`
   directory of a registry package as it is.

3. Stage all of it:

   ```bash
   git add --force vendor
   ```

   `.gitignore` has the patterns `debug` and `target` without a leading
   slash, and they match directories inside crates: 41 vendored files on
   2026-11-11, such as `vendor/cc/src/target/apple.rs`. Without `--force` a
   new file in such a directory stays out of the commit, and Git says
   nothing. `.gitattributes` marks `vendor/**` as `-text`, so Git stores
   every byte as it is.

4. Run the check, then commit `Cargo.lock` and `vendor/` together.

## State on 2026-11-11

On the tree of commit `717e7eff` the check passed with all 563 archives
downloaded from crates.io (96 MB): 22,456 files and 521,037,483 bytes were
compared. The archives hold 239 further members, 226 `.gitignore` and 13
`.gitattributes` files, all of which the rule above leaves out. With Cargo
1.99.1, `cargo vendor --locked` reproduced the committed tree byte for byte,
into an empty directory and over the existing one.
