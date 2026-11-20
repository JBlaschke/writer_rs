# T94 — Vocabulary styles in the PDF (`design.typ`, `title-block`, design tokens)

| | |
|---|---|
| Milestone | M8 |
| Size | M |
| Depends on | T92, T93 |
| Parallel-safe with | nothing: T95 starts after this task |
| Files you own | `assets/typst/writer-doc/design.typ` (new), `assets/typst/writer-doc/lib.typ`, `assets/typst/writer-doc/template.typ` (comments and the body of `writer-doc`; the import line is T93's), `src/typst/frontend/metadata.rs` (the skipped argument names in `prepare_inner`), `tests/typst_vocabulary.rs` (new) |

## Goal
A `[design.styles.<Name>]` table named after one of writer's Word styles (`Heading 1`,
`Heading 1 Unlisted`, `TOC Heading`, `toc 2`, `Caption`, `Table Caption`, `Compact`, `Title`,
`Date`, `Header`, `footer`, …) now styles the same element in the PDF, so a theme describes
headings, the title block, the contents list, captions and table text once for Word and PDF. The
package applies those styles as show-set rules, which a `[pdf] template` written in the new
override order can override key by key; the template reads the design as Typst values through
`design-tokens` and can replace the title block through `title-block:`. A document whose design
declares none of those names compiles exactly as before.

The Typst code is written and measured already: `docs/research/R11-typst-vocabulary-styles.md`
§5 and §6 hold it. This task installs that code, replaces the starter template, teaches the
frontend one argument name and adds the tests.

## Read first
- `docs/spec/design.md` §8.7 (this is your spec), §6.3 (the 2026-11-19 note), §9.4 (last
  paragraph), §8.1 (last paragraph)
- `docs/spec/typst-dialect.md` §3 (row `title-block`), §4.2 (the 2026-11-19 note), §4.6 (the
  2026-11-19 note: the starter template)
- `docs/research/R11-typst-vocabulary-styles.md` §2 (the Typst rules the code relies on), §5 and
  §6 (the code and the commands that extract it)
- `tests/typst_leading.rs` (the test pattern this task copies)

## API to implement
```typst
// assets/typst/writer-doc/design.typ (new; R11 §5 is the whole file)
#let design-tokens(design: auto, ..rest) = { … }   // the design as Typst values
#let style-tokens(tokens, name) = { … }            // one named style out of design-tokens' result
#let design-leading(pitch, size, box: auto) = { … }
#let vocabulary-layout(design, leading, body) = { … }   // the show-set rules
// plus the helpers typst-dialect.md §4.2 names: style-key, design-style, style-value,
// style-over, style-face, style-text, style-leading, style-rules, vocabulary, cell-inset,
// apply-rules, typed-style, contents-title, and the helpers moved out of lib.typ

// assets/typst/writer-doc/lib.typ (R11 §6 is the complete change)
#import "design.typ": *
#let styled-title-block(meta, design, leading) = { … }
#let title-styles = ("Title", "Subtitle", "Author", "Date")
#let writer-furniture(spec, text-width, spacing: 1.2em, line-height: none, font: none) = { … }
#let writer-doc(.., design: auto, title-block: auto, .., body) = { … }
// title-block: auto | none | function (meta, theme) => content
// assertion: "writer-doc: title-block must be auto, none or a function"
```
```rust
// src/typst/frontend/metadata.rs, prepare_inner: the names `continue`d before
// metadata_literal gain "title-block"
"font" | "heading-font" | … | "background" | "design" | "title-block"
```

## Steps
1. Create `assets/typst/writer-doc/design.typ` from R11 §5. From the repository root:
   ```sh
   awk '/^```typst design\.typ$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R11-typst-vocabulary-styles.md \
       > assets/typst/writer-doc/design.typ
   ```
   The file has 372 lines and ends with the `style-tokens` function. Do not edit it.
2. Apply R11 §6's diff to `assets/typst/writer-doc/lib.typ`:
   ```sh
   awk '/^```diff lib\.typ$/ {on = 1; next} /^```$/ {on = 0} on' \
       docs/research/R11-typst-vocabulary-styles.md > lib-typ.diff
   git apply --check lib-typ.diff && git apply lib-typ.diff && rm lib-typ.diff
   ```
   `lib.typ` then has 707 lines. T93 does not touch `lib.typ`, so the diff applies as it is. If
   `git apply --check` fails, stop and make the seven edits that R11 §6 lists by hand, reading
   the diff as their exact text; do not rewrite the functions.
3. Replace `assets/typst/writer-doc/template.typ` with the text under "The starter template"
   below, character for character.
4. `src/typst/frontend/metadata.rs`, `prepare_inner`: add `| "title-block"` after `| "design"` in
   the `matches!` list of names that `continue`. Extend the comment above the list by one
   sentence saying that `title-block` takes a function, which is no literal.
5. Leave `tests/typst_design.rs` as it is. Its test
   `native_typography_arguments_stay_with_the_compiler` passes `design: (text: (size:
   "99pt",))`, a string where a design holds points, and must keep passing unchanged:
   `writer-doc` never reads that value, and the tokens are built for a `title-block` function
   alone (design.md §8.7). Step 4 gets a test of its own, the twelfth of step 6.
6. Create `tests/typst_vocabulary.rs` with the harness under "Hints" and the twelve tests under
   "Acceptance criteria". Each test builds its design with `styled(root, extra)` or `design(..)`,
   compiles a document and compares probe values with the numbers given. Compare numbers with
   a tolerance of 0.01 (`assert_close`), strings exactly.
7. Run `cargo test --test typst_vocabulary --test typst_design --test typst_package
   --test typst_furniture --test typst_leading --test cli_eject --test cli_serve --test cli_watch`,
   then the whole `cargo test`, then the other three quality gates.
8. Check the package outside writer once (manual check under "Acceptance criteria").

### The starter template
`assets/typst/writer-doc/template.typ` becomes exactly this (38 lines):

```typst
// Customize this delegating template and set [pdf] template to its path.
//
// The look comes from the project's [design]: writer-doc applies its tokens
// and its named styles (headings, title block, contents list, captions,
// table cells). Write here only what the design does not say. A rule placed
// after `show: base.writer-doc.with(..args)` overrides the package and the
// design key by key; a rule placed before it loses to them.
//
// Arguments of writer-doc. Metadata: title, subtitle, author/authors, date,
// abstract, keywords, description, lang. Layout: toc, toc-title,
// abstract-title, reference-section-title, numbering, paper, title-block
// (auto draws the built-in title block, none draws nothing, a function
// (meta, theme) => content draws its result, for example a cover page).
// Typography: font, heading-font, mono-font, size, leading, spacing, justify,
// margin, text-fill, link-fill, background. Design: design (auto reads the
// writer-design input, none keeps the package defaults, a dictionary replaces
// the input; exported sources pass their copied design here). References:
// bibliography, cite-style/csl, cite-locale. Config maps: markdown, math,
// references, html, pdf, docx, latex, typst, notebook. Ignored routing
// options: theme, template, reference-doc, latex-template. Other named
// arguments are accepted as extras. To set an argument for every document,
// pass it after ..args: base.writer-doc.with(..args, title-block: cover).
//
// `theme` is the design as Typst values, `none` for an unset key:
// theme.colors.accent, theme.fonts.heading, theme.text.size,
// theme.page.margin.top, theme.styles. base.style-tokens(theme, "Title") is
// one named style; its `text` field holds the arguments of text(..) for its
// face.
#import "@writer/doc:0.4.0" as base

#let writer-doc(..args, body) = {
    let theme = base.design-tokens(..args)
    show: base.writer-doc.with(..args)

    // Rules written here override the package and the design key by key.
    show heading: set text(fill: rgb("#1a3a8a"))
    body
}
```

## Acceptance criteria
All numbers below were measured on the reference code with the harness under "Hints"
(2026-11-20). `styled` is the design of that harness, `MAIN` its document. Lengths are in pt.

`tests/typst_vocabulary.rs` holds these tests and they pass:

1. `vocabulary_styles_set_writers_elements`: `styled`, `MAIN` with `toc: 3` and the title
   arguments. Probes:

   | probe | size | weight | fill | justify | above | below | other |
   |---|---|---|---|---|---|---|---|
   | `heading:[First]` | 14 | `bold` | `#0a2f41` | false | 29.1 | 15.1 | sticky true, align `left + top` |
   | `heading:[Second]` | 13.2 | `bold` | `#000000` | true | 15.84 | 8.25 | (not declared: unchanged) |
   | `heading:[Third]` | 10 | `bold` | `#000000` | true | 14.4 | 7.5 | font `dejavu sans mono` |
   | `heading:[Unlisted]` | 18 | `bold` | `#a96d4b` | false | 31.7 | 17.7 | |
   | `heading:[Contents]` | 16 | `regular` | `#0a2f41` | false | 30.4 | 16.4 | |
   | `entry:1` | 11 | `bold` | | | 13.15 | | |
   | `entry:2` | 11 | `regular` | | | 7.15 | | (not declared: unchanged) |
   | `table-caption` | 9 | `bold` | | | | | style `italic` |
   | `figure-caption` | 9 | `regular` | | | | | style `italic` |
   | `cell` | 9 | `regular` | | false | | | align `left + top` |
   | `body` | 11 | `regular` | `#000000` | true | | | |

   and the `table` probe is `top` 5.575, `bottom` 6.575, `left` 5.
2. `title_parts_take_their_styles`: the same document. `title` is size 30, weight `regular`,
   align `left + top`; `subtitle` is size 11 and align `left + top` (`Title`'s alignment holds
   for the block); `date` is size 9.
3. `first_title_part_keeps_its_space_before`: `styled` against `styled` with `Title`'s
   `space_before` set to `"10pt"`, document `TITLED`. The `y` of `title` is 132.119 and 102.119:
   assert the difference is 30 and that `subtitle`, `date` and `body` move by 30 as well.
4. `last_title_part_space_after_replaces_the_closing_gap`: `styled` with `Date` set to
   `{ "size": "9pt", "space_after": "10pt" }` against the same with `"40pt"`, document `TITLED`.
   The `y` of `body` differs by 30 (250.507 and 280.507) and the `y` of `date` is the same
   (228.735). With `styled` itself (no `space_after`) `body` is at 264.357.
5. `designs_without_vocabulary_names_change_nothing`: `plain` (a design whose only style is
   `Recipient`), `MAIN` with `toc: 3` and the title arguments. `heading:[First]`,
   `heading:[Unlisted]` and `heading:[Contents]` are size 15.4, weight `bold`, fill `#000000`,
   above 19.8, below 8.25; `heading:[Second]` 13.2 / 15.84 / 8.25; `heading:[Third]` 11 / 15.84
   / 8.25; every `entry:N` has weight `regular` and above 7.15; `cell`, `table-caption` and
   `figure-caption` are size 11, weight `regular`, style `normal`; `table` is 5, 5, 5; `title`
   is size 17, weight `bold`, align `center + top`. The same values hold for
   `Design::default()` (no design at all).
6. `a_title_style_without_keys_keeps_the_plain_title_block`: `plain` against `plain` plus
   `"Date": { "keep_together": true }`, document `TITLED`. The `y` of `title`, `subtitle`, `date`
   and `body` are equal in both (85.039, 116.404, 157.28, 194.218), and so are their sizes and
   weights.
7. `template_rules_after_writer_doc_override_the_design`: `styled` with the document `AFTER`.
   `heading:[First]` and `heading:[Unlisted]` have fill `#ff4136` (Typst's `red`), and keep size
   14 and 18, weight `bold`, above 29.1 and 31.7. `heading:[Contents]` keeps fill `#0a2f41`
   (`TOC Heading` is laid over `Heading 1`'s keys again).
8. `template_rules_before_writer_doc_lose_to_declared_keys`: `styled` with the document `BEFORE`.
   `heading:[First]` has fill `#0a2f41` and size 14 (declared keys win) and style `oblique` (a key
   the design leaves unset); `heading:[Second]` is size 13.2, fill `#000000`, style `normal`.
9. `title_block_argument_replaces_or_removes_the_title_block`: with `title-block: none` and the
   title arguments, the document has no `title`, `subtitle` or `date` probe; with the function
   of `COVER`, the `cover` probe is `title` `"[The title]"`, `authors` 1, `size` 14, `accent`
   `#a96d4b`, `top` 85.039. `#show: writer-doc.with(title: [T], title-block: 3)` fails to compile
   and the error text contains `writer-doc: title-block must be auto, none or a function`.
10. `design_tokens_are_typst_values`: `styled` with the document `TOKENS`. The `tokens` probe is
    `declared` true, `size` 14, `weight` 700, `color` `#0a2f41`, `before` 20, `keep` true, `text`
    `(weight: 700, size: 14pt, fill: rgb("#0a2f41"))`, `h3font` `DejaVu Sans Mono`, `h3weight`
    700, `missing` true, `plain` 0, `body` 11, `accent` `#a96d4b`, `top` 85.039, `right` true,
    `undeclared` false.
11. `starter_template_overrides_the_design`: a temporary directory with `writer.toml` (`[pdf]`
    `system_fonts = false`, `template = "template.typ"`, and a `[design.styles."Heading 1"]` table
    with `size = "14pt"` and `color = "#0a2f41"`), `template.typ` holding
    `writer::vendor::get("typst/writer-doc/template.typ")`, and a Markdown file `# First`. The
    PDF compiles through the pipeline (copy the set-up of
    `delegating_template_receives_design_defaults_and_explicit_arguments_win` in
    `tests/typst_design.rs`), and the heading's text run is 14pt and filled `#1a3a8a`: the
    template's rule wins for the colour and the design's size stays.
12. `native_title_block_stays_with_the_compiler`: through `pipeline::prepare`, as
    `native_typography_arguments_stay_with_the_compiler` in `tests/typst_design.rs` does it, a
    native `.typ` file holding `#show: writer-doc.with(title: [The title], title-block: X)` and
    the line `Native body.`, once with `X` = `none` and once with `X` = `(meta, theme) => [Cover:
    #meta.title]`. Both prepare without an error diagnostic, `meta.title` is `The title`,
    `meta.vars` has no `title-block` key, and the compiled page holds a text run with `Native
    body`. With `none` no run holds `The title`; with the function a run holds `Cover: The
    title`. Without step 4 the test fails: the function is the error `expected a literal value`
    and `none` lands in `meta.vars`.

Elsewhere:

- `tests/typst_design.rs` is unchanged (`git diff --stat tests/typst_design.rs` prints nothing)
  and `native_typography_arguments_stay_with_the_compiler` passes: its fixture's `design:` holds
  a string that the package must never read.
- No other test changes. In particular `tests/cli_serve.rs` (it replaces `#1a3a8a` in the starter
  template and expects the served pages to follow), `tests/cli_eject.rs`, `tests/cli_watch.rs`,
  `tests/typst_package.rs`, `tests/typst_furniture.rs`, `tests/typst_export.rs` and every
  snapshot pass unmodified. A test outside this task's files that fails is a finding to report,
  not a file to edit.
- `cmp` reports no difference between `assets/typst/writer-doc/design.typ` and the output of step
  1's `awk` command, and `git diff assets/typst/writer-doc/lib.typ` equals R11 §6's diff (compare
  with `git diff assets/typst/writer-doc/lib.typ | diff - lib-typ.diff` after extracting the diff
  again; the `index` line differs).
- Manual check, outside writer: `cargo run -- eject typst-package /tmp/t94-packages` writes
  `writer/doc/0.4.0/design.typ`; a file `probe.typ` holding `#import "@writer/doc:0.4.0": *`,
  `#show: writer-doc.with(title: [T], design: (styles: ("Heading 1": (size: 14.0, color:
  "#0a2f41"))))` and `= First` compiles with `typst compile --package-path /tmp/t94-packages
  probe.typ` when a `typst` 0.15 binary is at hand (say in the report if none was).
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` pass.

## Out of scope
- Changing the reference code: selectors, the `L + space` rule, the title block's geometry and
  the token names are the contract (design.md §8.7). A disagreement between R11 and the spec is
  a finding for the coordinator.
- `writer-div`'s block spacing (design.md open point 11), Word's `basedOn` chains for the title
  parts (12), named styles on HTML elements (13), `Compact` on tight lists (14), a weight Word
  cannot select (15).
- The styles design.md §8.7 lists under "Not applied in the PDF".
- The guide (`docs/guide/**`): T95. The version number: T93.
- `src/docx/**`, `src/html/**`, `src/design/**`, `src/check/**`: the resolver, the JSON and the
  checks do not change. `design-style-unused` already exempts the vocabulary names (T92).
- A native `.typ` document that calls `design-tokens` or `style-tokens` in its body: Typst
  compiles it for the PDF; the structured frontend (HTML, DOCX) is not taught these calls here.
- Editing a real theme or template outside this repository.

## Hints (verified)
- **Harness** (compiled and run against the reference code on 2026-11-20; `Introspector` must
  be imported, it is the trait that provides `query`):
  ```rust
  use std::collections::BTreeMap;
  use std::path::Path;

  use typst::foundations::{Datetime, Label, Selector, Value};
  use typst::introspection::{Introspector, MetadataElem};
  use typst::utils::PicoStr;
  use typst_layout::PagedDocument;
  use writer::assets::registry::AssetRegistry;
  use writer::config::Config;
  use writer::design::{Design, Layer, Origin, merge};
  use writer::typst::compile::compile_document;
  use writer::typst::sourcemap::SourceMap;
  use writer::typst::world::{MainFile, WriterWorld};

  /// Reports the styles in effect where it stands, as one JSON object.
  const PROBE: &str = r##"
  #let probe-space(value) = {
      if type(value) == length { value.to-absolute().pt() } else { repr(value) }
  }
  #let probe(tag) = context [#metadata(json.encode((
      tag: tag, size: text.size.pt(), weight: text.weight,
      fill: text.fill.to-hex(), style: text.style, font: text.font,
      justify: par.justify, align: repr(align.alignment),
      above: probe-space(block.above), below: probe-space(block.below),
      sticky: block.sticky, y: here().position().y.pt(),
  ))) <probe>]
  #show heading: it => { it; probe("heading:" + repr(it.body)) }
  #show outline.entry: it => { it; probe("entry:" + str(it.level)) }
  "##;

  fn design(table: serde_json::Value, root: &Path) -> Design {
      merge(&[Layer {
          table: table.as_object().unwrap().clone(),
          base_dir: root,
          origin: Origin::FrontMatter,
      }])
      .unwrap()
  }

  fn try_compile(
      root: &Path,
      design: &Design,
      body: &str,
  ) -> Result<PagedDocument, String> {
      let mut config = Config::default();
      config.pdf.system_fonts = false;
      let mut world = WriterWorld::from_config_design(
          &config,
          design,
          &AssetRegistry::new(),
          root,
          MainFile::Virtual,
          Datetime::from_ymd(2026, 11, 20).unwrap(),
      )
      .unwrap();
      let source = format!(
          "#import \"@writer/doc:{}\": *\n{PROBE}\n{body}",
          writer::typst::WRITER_DOC_VERSION,
      );
      world.set_main_text(&source, SourceMap::default());
      compile_document(&mut world)
          .into_result()
          .map(|(document, _)| document)
          .map_err(|error| error.to_string())
  }

  /// Every probe of `document` by its tag.
  fn probes(document: &PagedDocument) -> BTreeMap<String, serde_json::Value> {
      let label = Label::new(PicoStr::intern("probe")).unwrap();
      document
          .introspector()
          .query(&Selector::Label(label))
          .iter()
          .map(|content| {
              let metadata = content.to_packed::<MetadataElem>().unwrap();
              let Value::Str(text) = &metadata.value else {
                  panic!("expected a JSON string, got {:?}", metadata.value)
              };
              let value: serde_json::Value =
                  serde_json::from_str(text.as_str()).unwrap();
              (value["tag"].as_str().unwrap().to_owned(), value)
          })
          .collect()
  }
  ```
  A probe value reads as `probes["heading:[First]"]["size"].as_f64().unwrap()` or
  `probes["cell"]["weight"] == "regular"`. `text.weight` is a name where Typst has one
  (`regular`, `bold`) and `text.font` is the family in lower case.
- **The design `styled(root, extra)`**: the table below, with each entry of `extra` replacing
  the style of the same name (`table["styles"][name] = style.clone()`):
  ```rust
  serde_json::json!({
      "text": { "size": "11pt" },
      "colors": { "accent": "#a96d4b" },
      "page": { "margin": { "top": "30mm" } },
      "styles": {
          "Heading 1": {
              "size": "14pt", "weight": 700, "color": "#0a2f41",
              "align": "left", "space_before": "20pt",
              "space_after": "6pt", "keep_with_next": true,
          },
          "Heading 1 Unlisted": { "size": "18pt", "color": "#a96d4b" },
          "TOC Heading": { "size": "16pt", "weight": 400 },
          "Heading-3": { "font": "DejaVu Sans Mono Bold", "size": "10pt" },
          "toc 1": { "weight": 700, "space_before": "6pt" },
          "Caption": { "size": "9pt", "italic": true },
          "Table Caption": { "weight": 700 },
          "Compact": {
              "size": "9pt", "align": "left", "space_before": "2pt",
              "space_after": "3pt",
          },
          "Title": {
              "size": "30pt", "weight": 400, "align": "left",
              "space_before": "40pt",
          },
          "Date": { "size": "9pt" },
      },
  })
  ```
  `plain` is `{ "text": { "size": "11pt" }, "page": { "margin": { "top": "30mm" } }, "styles":
  { "Recipient": { "size": "9pt" } } }`. `"Heading-3"` is written with a hyphen on purpose: names
  compare by normalised key, so it is `Heading 3`.
- **The documents.** The title arguments put each probe first in its part, because a probe at
  the end of a paragraph is placed after the paragraph's spacing in the plain title block:
  ```rust
  const TITLE_ARGS: &str = concat!(
      "title: [#probe(\"title\")The title], ",
      "subtitle: [#probe(\"subtitle\")Sub], authors: (\"Ada\",), ",
      "date: [#probe(\"date\")2026]",
  );
  // TITLED: format!("#show: writer-doc.with({TITLE_ARGS})\n#probe(\"body\")Body\n")
  // MAIN:   format!("#show: writer-doc.with({TITLE_ARGS}, toc: 3)\n{BODY}")
  const BODY: &str = r##"
  = First
  #probe("body")Body
  == Second
  === Third
  #heading(level: 1, numbering: none, outlined: false)[Unlisted]
  #figure(
      table(columns: 2, [A #probe("cell")], [B]),
      caption: [Table #probe("table-caption")],
  )
  #figure(
      rect(width: 2cm, height: 1cm),
      caption: [Figure #probe("figure-caption")],
  )
  #let inset-side(inset, side) = {
      let value = if type(inset) == dictionary { inset.at(side) } else { inset }
      value.length.to-absolute().pt()
  }
  #context [#metadata(json.encode((
      tag: "table", top: inset-side(table.inset, "top"),
      bottom: inset-side(table.inset, "bottom"),
      left: inset-side(table.inset, "left"),
  ))) <probe>]
  "##;
  const AFTER: &str = concat!(
      "#let template(..args, body) = {\n",
      "    show: writer-doc.with(..args)\n",
      "    show heading.where(level: 1): set text(fill: red)\n",
      "    body\n}\n",
      "#show: template.with(toc: 3)\n= First\n",
      "#heading(level: 1, numbering: none, outlined: false)[Unlisted]\n",
  );
  const BEFORE: &str = concat!(
      "#let template(..args, body) = {\n",
      "    show heading.where(level: 1): set text(\n",
      "        fill: red, size: 30pt, style: \"oblique\"\n    )\n",
      "    writer-doc(..args, body)\n}\n",
      "#show: template\n= First\n== Second\n",
  );
  const COVER: &str = concat!(
      "#let cover(meta, theme) = [#metadata(json.encode((\n",
      "    tag: \"cover\", title: repr(meta.title),\n",
      "    authors: meta.authors.len(),\n",
      "    size: theme.styles.at(\"Heading 1\").size.pt(),\n",
      "    accent: theme.colors.accent.to-hex(),\n",
      "    top: theme.page.margin.top.pt(),\n",
      "))) <probe>]\n",
      "#show: writer-doc.with(title: [The title], authors: (\"Ada\",),\n",
      "    title-block: cover)\nBody\n",
  );
  const TOKENS: &str = concat!(
      "#let theme = design-tokens()\n",
      "#let h1 = style-tokens(theme, \"heading1\")\n",
      "#let h3 = style-tokens(theme, \"Heading 3\")\n",
      "#let nope = style-tokens(theme, \"Nope\")\n",
      "#metadata(json.encode((\n",
      "    tag: \"tokens\", declared: theme.declared, size: h1.size.pt(),\n",
      "    weight: h1.weight, color: h1.color.to-hex(),\n",
      "    before: h1.space_before.pt(), keep: h1.keep_with_next,\n",
      "    text: repr(h1.text), h3font: h3.font, h3weight: h3.weight,\n",
      "    missing: nope.size == none, plain: nope.text.len(),\n",
      "    body: theme.text.size.pt(), accent: theme.colors.accent.to-hex(),\n",
      "    top: theme.page.margin.top.pt(),\n",
      "    right: theme.page.margin.right == none,\n",
      "    undeclared: design-tokens(design: none).declared,\n",
      "))) <probe>\n",
  );
  ```
  `tests/typst_vocabulary.rs` is a Rust file: lay the long literals out as above (rustfmt leaves
  a statement with an over-long string literal as it is; AGENTS.md, "Rust refinements").
- **Where the numbers come from.** The design sets no `text.line_height`, so the document's
  leading is Typst's `0.65em` and a style's block spacing is `0.65 × size + space`: `Heading 1`
  above is 0.65 × 14 + 20 = 29.1, below 0.65 × 14 + 6 = 15.1; the contents title 0.65 × 16 + 20 =
  30.4; `Heading 1 Unlisted` 0.65 × 18 + 20 = 31.7; `toc 1` 0.65 × 11 + 6 = 13.15. An undeclared
  heading keeps Typst's own spacing (1.8em or 1.44em of the body size above, 0.75em below, taken
  at the heading's size when a style changes it: 1.44 × 10 = 14.4 for `Heading 3`). A table
  cell's inset is half the leading plus the space: 0.325 × 11 + 2 = 5.575 and + 3 = 6.575. The
  `y` values depend on Libertinus Serif, the embedded default family; the tests that use them
  compare two compilations, except for the absolute values quoted for orientation.
- **For test 11**, a text run's size and fill: walk the page frame as `runs` does in
  `tests/typst_leading.rs` and read `FrameItem::Text(run)`: `run.size.to_pt()` and `run.fill`
  (a `Paint`; compare with `Paint::Solid(Color::from_str("#1a3a8a").unwrap())` from
  `typst::visualize::{Color, Paint}`, `std::str::FromStr`). `listing_fonts_use_design_defaults_
  and_explicit_mono_font_overrides` in `tests/typst_design.rs` shows the same walk for a font.
- **Typst rules the code relies on** (R11 §2; each one probed): a later show-set rule wins over
  an earlier one property by property, which is why the package's rules are show-set rules and
  why a template's rules must come after `show: base.writer-doc.with(..args)`; `set text(..(:))`
  and `set align(..())` change nothing; a dictionary spread without a named pair is written
  `(: ..a, ..b)`; `f.with(..args, title-block: cover)` lets the later named argument replace one
  of the same name in `args`; `dict.field(args)` is a method call, which is why `style-tokens` is
  a function of the package and not a field of the tokens.
- **The tokens are built late on purpose.** `design-tokens` converts every scalar key of the
  design (`number * 1pt`, `rgb(hex)`) and fails on a value of another type. `writer-doc` calls it
  in `numbered-layout`, for a `title-block` function alone (R11 §6, corrected 2026-11-20). A first
  version kept the tokens in `settings` for every document and broke
  `native_typography_arguments_stay_with_the_compiler` with "cannot multiply string with
  length". Do not move the call.
- **Embedding.** `src/vendor.rs` embeds every file under `assets/typst/` (`rust-embed`, `#[folder
  = "assets/"]`, `#[include = "typst/**/*"]`) and `src/typst/package.rs` writes whatever
  `vendor::list("typst/writer-doc/")` returns, so `design.typ` needs no Rust change to be compiled
  in and ejected.
- **The frontend today** (`writer dump native.typ` with writer 0.3.0, 2026-11-20): `title-block:
  none` lands in `meta.vars` as `"title-block": null`, and `title-block: (meta, theme) => []` is
  the error `expected a literal value, found (meta, theme) => []`. Both are what step 4 removes.
- **Existing designs in the repository**: `examples/letter/theme/theme.toml` and
  `tests/fixtures/design/letter/theme/theme.toml` declare `[design.styles.Date]` for a div. Those
  letters have no title metadata, so their title block stays empty and their PDFs do not change.
  No other example, fixture or guide declares a vocabulary name.
