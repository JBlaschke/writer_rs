# One letterhead for HTML, PDF and Word

This fictional letter uses one [theme design](theme/theme.toml) for typography,
page geometry, six paragraph styles and page furniture. Its
[geometric logo](theme/assets/logo.png) is a neutral placeholder, not an
institution's mark. The [stylesheet](theme/styles/letter.css) adds only a
screen sheet; there is no custom HTML template, PDF template or Word reference.

From the repository root:

```sh
writer build examples/letter/letter.md --to html
writer build examples/letter/letter.md --to pdf
writer build examples/letter/letter.md --to docx
```

Or run `writer build examples/letter/letter.md` to build all three configured
outputs. They appear as `letter.html`, `letter.pdf` and `letter.docx` under
`examples/letter/dist/`.

Open the outputs and compare:

- The recipient, inside address, date and salutation use named div styles.
- The first printed page has the logo and institute name; page two has a
  compact continuation header. An explicit page break makes both visible.
- Both printed pages have the legal/contact footer and page counters. The
  website's continuous screen view shows the first-page furniture and `1`
  for each page placeholder.
- Word keeps the paragraphs and styles editable. The closing stays with the
  signature when the output can honor that pagination constraint.

The design names Source Sans 3 but does not ship font files. With the example's
`system_fonts = false`, PDF uses embedded fallback fonts until you add the
fonts below. HTML uses its CSS fallback when the family is unavailable; Word
uses the reader's installed fonts. Page layout and furniture still work.

To use Source Sans 3 consistently, place its regular, italic, semibold and bold
files in `theme/assets/fonts/`, then replace the existing `[design.fonts]`
table with:

```toml
[design.fonts]
body = { family = "Source Sans 3", files = [
    "assets/fonts/SourceSans3-Regular.ttf", "assets/fonts/SourceSans3-Italic.ttf",
    "assets/fonts/SourceSans3-SemiBold.ttf", "assets/fonts/SourceSans3-Bold.ttf",
] }
heading = "Source Sans 3"
```

Add these paths only after copying the files: a missing declared resource is
a build error. HTML embeds or copies the files, PDF loads them, and Word still
requires the font installed on the reader's machine.

Inspect or package the design:

```sh
writer dump --design examples/letter/letter.md
writer check examples/letter/letter.md --target html,pdf,docx
writer eject theme portable-letter --design examples/letter/letter.md
```

`check` can report `design-font-family` for Source Sans 3 in this font-free
example. That warning explains the PDF fallback. With all styles selected by
the document, there should be no `design-style-unused` warning.

The `text` portion of the resolved `dump --design` output is:

<!-- generated: letter design text -->
```json
{
  "size": 11.0,
  "line_height": {
    "multiple": 1.35
  },
  "paragraph_spacing": 10.205,
  "justify": false
}
```

Lengths are resolved to points. The three outputs share the geometry these
values describe: body lines 1.35 × 11 pt = 14.85 pt apart, and 10.2 pt more
(3.6 mm) across a paragraph break. In the PDF, `writer-doc` derives Typst's
leading and paragraph spacing from them and from the body font's measured cap
height, so the template needs no hand-tuned `par(leading:)`. Changing
`paragraph_spacing` in the theme changes all three outputs. A document can
override individual keys through front matter:

```yaml
---
design:
  text:
    size: 12pt
---
```

Theme CSS remains available for HTML-specific rules; a Typst template can
still delegate to `writer-doc`, and a Word reference document keeps its own
styles and section settings. See the
[cross-format design guide](../../docs/guide/customization.md#cross-format-design)
and [full key reference](../../docs/guide/configuration.md#design).
