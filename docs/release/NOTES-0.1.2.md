writer 0.1.2 ships smaller executables: the embedded fonts are now stored
compressed, which takes about 3.1 MiB off the binary on every platform.
Documents build as they did with 0.1.1, and projects need no changes.

## Changes

- **Smaller executables.** The 17 fonts that writer embeds for typesetting
  (9.7 MB) are stored deflated (6.4 MB) and unpacked in memory the first
  time a run needs them, in a few milliseconds. The executables are
  48.7-60.8 MiB, down from 51.9-63.9 MiB in 0.1.1. The fonts and the order
  in which writer offers them are unchanged, so font selection and
  typesetting stay the same.
- **No project changes.** The bundled Typst package stays at
  `@writer/doc` 0.3.0, so templates and ejected package copies from 0.1.1
  keep working. `writer --licenses` lists the same notices; only its line on
  `typst-assets` changes, to say that writer embeds those fonts itself.
- **Release builds.** The release binaries are built with Rust 1.100.0,
  pinned so that every build of a release uses the same compiler. Building
  from source still needs Rust 1.96 or newer. CI now compares the vendored
  dependencies under `vendor/` with the archives published on crates.io.

## Downloads

One archive per platform: macOS (Apple silicon and Intel), Linux (x86_64 and
arm64, glibc) and Windows (x64 and arm64). Each holds the `writer` binary,
the MIT license and the notices of every embedded asset and linked crate.
Check a download against `SHA256SUMS`, unpack it and put `writer` on your
`PATH`.

The binaries are not signed or notarized. On macOS, remove the quarantine
mark of a browser download with `xattr -d com.apple.quarantine writer`.

The web page, PDF and Word outputs need nothing else at run time. Compiling
the LaTeX export needs a separate TeX installation. Chrome-based PDF is not
supported yet; the embedded Typst path needs no browser.

Read the [user guide](https://jblaschke.github.io/writer_rs/guide/) or start
with the [examples](https://jblaschke.github.io/writer_rs/#examples).
