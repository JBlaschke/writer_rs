# writer archive

This archive contains the `writer` executable (`writer.exe` on Windows), the
Writer MIT license and complete embedded/linked-crate notices under
`THIRD-PARTY-LICENSES/`. Keep those notices with redistributed copies.

Check the executable's version before using it:

```sh
./writer --version
./writer --help
./writer --licenses
```

In Windows PowerShell, use `.\writer.exe --version`. Move the executable to a directory on your PATH
if you want to call it as `writer`. No Typst, Python or notebook runtime is
required to use Writer's embedded HTML/PDF/Word and source-export paths.
Python and Rust are release-building tools, not runtime requirements.

Create a report, preview it and build outputs:

```sh
writer init report --bib
cd report
writer build document.md
writer serve document.md
writer build document.md --to html,pdf,docx,latex,typst
```

PDF uses the embedded Typst compiler and fonts. Notebook builds use saved
outputs without executing cells. LaTeX export produces source/resources for
a separate TeX installation; it never runs a TeX engine. Chrome-based PDF is
currently unsupported; the embedded default path needs no Chrome.

Source and documentation: <https://github.com/JBlaschke/writer_rs>.
This README describes an archive's contents; it does not establish a published
release, signing/notarization status or a platform compatibility floor.
