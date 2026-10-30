# Prepared initial 0.1.0 notes

These are prepared notes for an initial Writer artifact. They are not a record
of a published release, historical versions or completed six-platform results.
The artifact's `writer --version` identifies its exact revision.

Writer accepts Markdown reports, saved Jupyter notebooks and native Typst
sources. It builds standalone/site HTML, embedded-compiler PDF, editable Word
packages, LaTeX source/resources and portable Typst source/resources. Native
Typst PDF compiles the original file; other native-Typst outputs use the
structured subset.

The browser preview has web and typeset-page views, source-position updates,
last-good output retention and located errors/warnings. Checks reuse frontend,
reference and selected-target diagnostics without writing target output.

References/citations use the configured bibliography and CSL style. Notebook
builds preserve saved output selection and never execute a kernel. Themes,
Typst/LaTeX templates and Word references can be ejected for customization.

Archives include the executable, MIT license and complete third-party notices,
including embedded fonts, web assets, packages and linked crate license files.
The artifact workflow builds six native targets and records actual smoke/size
results; only successful workflow reports establish those particular results.
LaTeX compilation, optional browser QA, manual Word/LibreOffice/Google Docs
checks and platform signing/notarization are separate validation/publication
activities and are not implied by these prepared notes. Chrome-based PDF is
currently unsupported.
