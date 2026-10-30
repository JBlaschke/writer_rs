# Getting started

This walkthrough creates a report, previews it while you edit, and builds
files to share. You can also start with any existing Markdown document,
saved notebook or Typst source.

## Download a release

Each [release](https://github.com/JBlaschke/writer_rs/releases) has one
archive per platform: macOS (Apple silicon and Intel), Linux (x86_64 and
arm64, glibc) and Windows (x64 and arm64). Check the archive against the
release's `SHA256SUMS`, unpack it, and move `writer` (`writer.exe` on
Windows) to a directory on your `PATH`:

```sh
tar -xzf writer-v0.1.0-aarch64-apple-darwin.tar.gz
mv writer-v0.1.0-aarch64-apple-darwin/writer ~/.local/bin/
writer --version
```

The binaries are not signed. On macOS, a browser download carries a
quarantine mark; remove it with `xattr -d com.apple.quarantine writer`.
Keep the archive's `THIRD-PARTY-LICENSES` with copies you pass on.

## Install from source

In a checkout of this repository, with Rust 1.96 or newer:

```sh
cargo install --path . --locked
writer --version
```

Cargo installs the binary in its configured binary directory, normally
`~/.cargo/bin`. That directory must be on your `PATH`. `--version` prints the
installed version and build identifier.

The compiler, document package, default theme and built-in math assets are
embedded. You do not need a Typst CLI, Python or Node.js to build the supplied
examples. Compiling exported LaTeX needs a separate TeX installation.

## Create a report

```sh
writer init report
cd report
```

The project contains `document.md`, `writer.toml`, a local figure and child
theme, plus a README and `.gitignore`. To start from Typst instead, use
`writer init report --typst`. Add `--bib` to either form for a bibliography
and an example citation.

Without `--force`, a nonempty destination is refused. `--force` permits
unrelated existing files; it still refuses to replace any selected scaffold
file. Choose a new directory or move a conflicting file yourself.

You can skip scaffolding. Save this as `document.md`:

```markdown
---
title: My report
authors: [Your name]
toc: true
---

# Results

The model is $T(n) = W/n + c$.
```

## Preview while editing

```sh
writer serve document.md --open
```

Writer prints the address of its local server. Saving the source, an included
file, a figure or the selected theme updates the preview. The web view keeps
your reading position where it can; a stylesheet edit updates styles without
reloading the document. Errors appear over the previous good result.

Use the view button or `p` to switch between the web document and typeset
pages. Markdown and notebooks start in the web view; native Typst starts in
pages. In pages, `+` or `=` zooms in, `-` zooms out, and `0` fits the width.
These shortcuts leave text-entry controls alone.

Stop with Ctrl-C. If the default port is occupied, `--port 0` chooses a free
one. You can also select a starting view with `--view web` or `--view pages`.

## Build files

```sh
writer build document.md
writer build document.md --to html,pdf,docx
```

The first command writes `dist/document.html`. The second also writes
`dist/document.pdf` and `dist/document.docx`. Native Typst defaults to PDF:

```sh
writer build document.typ
```

For separate HTML assets, use `writer build document.md --site`. To rebuild
on saves without a server, use `writer build document.md --watch`.

LaTeX and Typst source exports are also available:

```sh
writer build document.md --to latex,typst
```

Keep exported source together with its sidecar resources. Writer exports
LaTeX source; it does not run TeX. See [outputs](outputs.md) for file locations,
custom destinations and compilation steps.

## Find problems

```sh
writer check document.md
writer check document.md --target html,pdf,docx
writer check document.md --format json
```

Check reports source and selected-output findings without writing outputs.
An error exits unsuccessfully. Add `--deny-warnings` when warnings should also
fail a check. A clean check does not replace opening the final output: layout
and font choices still matter.

Continue with [authoring](authoring.md), [notebooks](notebooks.md) or
[native Typst](typst.md). [Configuration](configuration.md) explains project
settings, front matter and path resolution.
