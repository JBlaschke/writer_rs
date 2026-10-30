# Command-line reference

Global flags work before or after a command. `--config PATH` overrides project
configuration discovery, `-v`/`-vv` increases logging, and `-q` limits human
status and diagnostics. `--version` prints build metadata; `--licenses` prints
notices for embedded third-party assets without requiring a command.

Build status goes to stdout and diagnostics to stderr. Dump writes its selected
representation to stdout; JSON check output also uses stdout. Ejection prints
usable TOML guidance to stdout even in quiet mode.

Exit code zero means success, one means a document/check/build failure, and
two means invalid command usage or configuration usage. Check represents
configuration/input failures as findings and exits one. The full diagnostic
and target result matters: a failing mixed build may still have written its
other successful targets.

`build --format` selects an input language; `check --format` chooses text or
JSON diagnostics. `build --strict` escalates selected portability findings;
`check --deny-warnings` instead fails when any warning remains. They are
different controls.

With one build target, `-o` names a file or HTML site directory. With several
targets it names the output directory. See [outputs](outputs.md) for exact
layouts, and [configuration](configuration.md) for path origins.

The following help blocks are generated from the installed command tree.

## Writer

```text
$ writer --help
Render documents from Markdown, notebooks and typst

Usage: writer [OPTIONS] [COMMAND]

Commands:
  build  Render a document to HTML, PDF, DOCX, LaTeX or typst source
  serve  Render, watch and live-reload in the browser
  check  Lint a document without building
  dump   Print the IR, generated source, or template context
  init   Scaffold a new document directory
  eject  Copy a built-in theme, template, package or reference document
  help   Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...     Log more: -v info, -vv debug (RUST_LOG applies on top)
  -q, --quiet          Print only errors (RUST_LOG is ignored)
      --config <PATH>  Use this writer.toml instead of searching beside the input
      --licenses       Print notices for embedded third-party files and data
  -h, --help           Print help
  -V, --version        Print version
```

## Build

```text
$ writer build --help
Render a document to HTML, PDF, DOCX, LaTeX or typst source

Usage: writer build [OPTIONS] <INPUT>

Arguments:
  <INPUT>
          The document: a .md, .ipynb or .typ file

Options:
      --to <LIST>
          Comma-separated output targets [default: the input's configured targets]

          Possible values:
          - html:  HTML document
          - pdf:   PDF document
          - docx:  Word document
          - latex: LaTeX source
          - typst: Typst source

  -v, --verbose...
          Log more: -v info, -vv debug (RUST_LOG applies on top)

  -o, --out <PATH>
          Output file or site directory; with several targets, the output directory

  -q, --quiet
          Print only errors (RUST_LOG is ignored)

      --config <PATH>
          Use this writer.toml instead of searching beside the input

      --watch
          Rebuild whenever the document's inputs change

      --licenses
          Print notices for embedded third-party files and data

      --theme <NAME|PATH>
          Theme name or directory

      --template <PATH>
          Typst PDF template

      --reference-doc <PATH>
          Word reference document

      --latex-template <PATH>
          LaTeX template

      --standalone
          Write one self-contained HTML file

      --site
          Write index.html plus assets/ in a site directory

      --strict
          Treat unsupported constructs and typst warnings as errors

      --engine <ENGINE>
          PDF rendering engine

          Possible values:
          - typst:  The embedded typst compiler
          - chrome: Headless Chrome

      --allow-packages
          Allow loading external typst packages

      --format <FORMAT>
          Input format [default: from the file extension]

          Possible values:
          - md:    Markdown
          - ipynb: Jupyter notebook
          - typ:   Typst source

  -h, --help
          Print help (see a summary with '-h')
```

## Serve

```text
$ writer serve --help
Render, watch and live-reload in the browser

Usage: writer serve [OPTIONS] <INPUT>

Arguments:
  <INPUT>
          The document to serve

Options:
      --port <N>
          Port to listen on; 0 picks a free port

  -v, --verbose...
          Log more: -v info, -vv debug (RUST_LOG applies on top)

      --host <H>
          Address to bind

  -q, --quiet
          Print only errors (RUST_LOG is ignored)

      --config <PATH>
          Use this writer.toml instead of searching beside the input

      --open
          Open the document in the default browser

      --licenses
          Print notices for embedded third-party files and data

      --view <VIEW>
          Initial document view

          Possible values:
          - web:   Structured HTML
          - pages: Typeset pages

      --theme <NAME|PATH>
          Theme name or directory

      --template <PATH>
          Typst PDF template

      --allow-packages
          Allow external typst packages

  -h, --help
          Print help (see a summary with '-h')
```

## Check

```text
$ writer check --help
Lint a document without building

Usage: writer check [OPTIONS] <INPUT>

Arguments:
  <INPUT>
          The document to lint

Options:
      --target <LIST>
          Comma-separated targets to check

          Possible values:
          - html:  HTML document
          - pdf:   PDF document
          - docx:  Word document
          - latex: LaTeX source
          - typst: Typst source

  -v, --verbose...
          Log more: -v info, -vv debug (RUST_LOG applies on top)

      --deny-warnings
          Exit unsuccessfully when warnings remain

  -q, --quiet
          Print only errors (RUST_LOG is ignored)

      --config <PATH>
          Use this writer.toml instead of searching beside the input

      --format <FORMAT>
          Diagnostic output format

          Possible values:
          - text: Human-readable source reports
          - json: A JSON array of diagnostics

          [default: text]

      --compat <COMPAT>
          Also check compatibility with another dialect

          Possible values:
          - presenter: Report constructs that presenter reads differently

      --licenses
          Print notices for embedded third-party files and data

  -h, --help
          Print help (see a summary with '-h')
```

## Dump

```text
$ writer dump --help
Print the IR, generated source, or template context

Usage: writer dump [OPTIONS] <INPUT>
       writer dump --default-config

Arguments:
  [INPUT]  The document to inspect

Options:
      --ir              Print resolved document IR as JSON (the default)
  -v, --verbose...      Log more: -v info, -vv debug (RUST_LOG applies on top)
  -q, --quiet           Print only errors (RUST_LOG is ignored)
      --typst           Print generated typst source
      --config <PATH>   Use this writer.toml instead of searching beside the input
      --latex           Print generated LaTeX source
      --context         Print the HTML template context as JSON
      --licenses        Print notices for embedded third-party files and data
      --no-resolve      Print the parsed IR before reference resolution
      --default-config  Print the built-in default writer.toml (takes no INPUT)
  -h, --help            Print help
```

`dump --default-config` takes no input and conflicts with the other dump modes.
`dump --no-resolve` applies only to IR output. Source dumps do not materialize
sidecar resources; use build for an export bundle.

## Init

```text
$ writer init --help
Scaffold a new document directory

Usage: writer init [OPTIONS] [DIR]

Arguments:
  [DIR]  Directory to create [default: .]

Options:
      --typst          Start with document.typ instead of document.md
  -v, --verbose...     Log more: -v info, -vv debug (RUST_LOG applies on top)
      --bib            Add a bibliography and an example citation
  -q, --quiet          Print only errors (RUST_LOG is ignored)
      --config <PATH>  Use this writer.toml instead of searching beside the input
      --force          Allow unrelated files; existing scaffold files are never replaced
      --licenses       Print notices for embedded third-party files and data
  -h, --help           Print help
```

`--force` permits unrelated files in a nonempty directory. It still refuses
to replace selected scaffold files.

## Eject

```text
$ writer eject --help
Copy a built-in theme, template, package or reference document

Usage: writer eject [OPTIONS] <COMMAND>

Commands:
  theme           Copy the built-in HTML theme
  typst-template  Copy the PDF template exporting writer-doc
  typst-package   Copy portable typst packages
  docx-reference  Copy the Word reference document
  latex-template  Copy the sample LaTeX template
  help            Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...     Log more: -v info, -vv debug (RUST_LOG applies on top)
  -q, --quiet          Print only errors (RUST_LOG is ignored)
      --config <PATH>  Use this writer.toml instead of searching beside the input
      --licenses       Print notices for embedded third-party files and data
  -h, --help           Print help
```

### HTML theme

```text
$ writer eject theme --help
Copy the built-in HTML theme

Usage: writer eject theme [OPTIONS] <DIR>

Arguments:
  <DIR>  Destination directory

Options:
  -v, --verbose...     Log more: -v info, -vv debug (RUST_LOG applies on top)
  -q, --quiet          Print only errors (RUST_LOG is ignored)
      --config <PATH>  Use this writer.toml instead of searching beside the input
      --licenses       Print notices for embedded third-party files and data
  -h, --help           Print help
```

### Typst template

```text
$ writer eject typst-template --help
Copy the PDF template exporting writer-doc

Usage: writer eject typst-template [OPTIONS] <FILE>

Arguments:
  <FILE>  Destination file

Options:
  -v, --verbose...     Log more: -v info, -vv debug (RUST_LOG applies on top)
  -q, --quiet          Print only errors (RUST_LOG is ignored)
      --config <PATH>  Use this writer.toml instead of searching beside the input
      --licenses       Print notices for embedded third-party files and data
  -h, --help           Print help
```

### Typst packages

```text
$ writer eject typst-package --help
Copy portable typst packages

Usage: writer eject typst-package [OPTIONS] <DIR>

Arguments:
  <DIR>  Destination package root

Options:
      --install        Install into typst's system data directory instead
  -v, --verbose...     Log more: -v info, -vv debug (RUST_LOG applies on top)
  -q, --quiet          Print only errors (RUST_LOG is ignored)
      --config <PATH>  Use this writer.toml instead of searching beside the input
      --licenses       Print notices for embedded third-party files and data
  -h, --help           Print help
```

The destination argument remains required with `--install`; that flag selects
the platform data directory instead of that destination.

### Word reference

```text
$ writer eject docx-reference --help
Copy the Word reference document

Usage: writer eject docx-reference [OPTIONS] <FILE>

Arguments:
  <FILE>  Destination file

Options:
  -v, --verbose...     Log more: -v info, -vv debug (RUST_LOG applies on top)
  -q, --quiet          Print only errors (RUST_LOG is ignored)
      --config <PATH>  Use this writer.toml instead of searching beside the input
      --licenses       Print notices for embedded third-party files and data
  -h, --help           Print help
```

### LaTeX template

```text
$ writer eject latex-template --help
Copy the sample LaTeX template

Usage: writer eject latex-template [OPTIONS] <FILE>

Arguments:
  <FILE>  Destination file

Options:
  -v, --verbose...     Log more: -v info, -vv debug (RUST_LOG applies on top)
  -q, --quiet          Print only errors (RUST_LOG is ignored)
      --config <PATH>  Use this writer.toml instead of searching beside the input
      --licenses       Print notices for embedded third-party files and data
  -h, --help           Print help
```
