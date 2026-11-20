writer 0.3.1 can print the build date: `{{today}}` in Markdown text
becomes a date such as 19 November 2026. Documents that do not use the
placeholder build as they did with 0.3.0, and projects need no changes.

## Changes

- **Today's date.** `{{today}}` in Markdown text, or `{{ today }}` with
  spaces, prints the build date as day, month name and year. It works in
  the HTML, PDF, Word, LaTeX and Typst outputs and in notebook Markdown
  cells, so a letter's date no longer needs to be written by hand:

  ```markdown
  :::date
  {{today}}
  :::
  ```

  See
  [today's date](https://jblaschke.github.io/writer_rs/guide/authoring.html#todays-date).
- **Which date.** The date is the day of the build, in UTC. For
  repeatable builds, pin it with `[pdf] timestamp` or the
  `SOURCE_DATE_EPOCH` environment variable, which already pin the PDF's
  timestamp. Unlike the PDF timestamp, the placeholder ignores front matter
  `date`. A document that uses the placeholder needs a valid timestamp and
  `SOURCE_DATE_EPOCH` in every output, not only in PDF.
- **What stays literal.** Code, raw HTML, math and link URLs keep
  `{{today}}` as written, and so does an escaped `\{{today}}`; in alert
  titles and `title=` or `caption=` values, use a code span instead. Front
  matter `title`, `subtitle` and `date` are plain text and keep the
  placeholder as written, while the `abstract` expands it. Native Typst
  sources have no placeholder. Text that already says `{{today}}` now
  prints the date; escape it to keep it.
- **Compatibility.** Month names are English. writer 0.3.0 prints the
  placeholder as written rather than failing, so a document that relies
  on it needs 0.3.1. The bundled Typst package stays at `@writer/doc`
  0.3.0.

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
