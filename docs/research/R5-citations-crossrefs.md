# R5 — Citations, bibliographies, labels and cross-references

**Status:** research notes (2026-10-24). Everything marked **[verified]** was read in crate
sources, official docs, or produced by a program run on this machine; **[unverified]** means
recalled or inferred and must be checked before a task depends on it.

Scratch material (all under the session scratchpad, not in the repo):

- hayagriva probe crate: `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/hayagriva-probe/`
  (`src/main.rs` main probe, `src/bin/locator.rs`, `src/bin/cslfile.rs`, `src/bin/dup.rs`, `src/bin/tree.rs`)
- typst 0.15.1 CLI built from crates.io (`cargo install typst-cli --version 0.15.1 --locked`):
  `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/build/typst-root/bin/typst`
- typst experiments: `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/typst-probe/`
- pandoc experiments (DOCX/LaTeX reference output): `/private/tmp/claude-502/-Users-johannes-blaschke-Developer-writer-rs/ed8f303c-a609-45f2-83a0-7c588ada7b48/scratchpad/pandoc-probe/`
- crate sources read: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/{hayagriva-0.10.1,citationberg-0.7.0,biblatex-0.12.0,comrak-0.56.0}/`

## 0. TL;DR for the plan

1. **Use `hayagriva` 0.10.1** (latest on crates.io, `cargo info` 2026-10-24) for HTML and DOCX
   citations. typst 0.15.1's `Cargo.toml` pins `hayagriva = "0.10.1"` **[verified]**, so the
   PDF backend (typst) and the HTML/DOCX backends format citations with the *same* code. Measured:
   for the same `.bib` and style name (`ieee`, `apa`, `chicago-notes`) the citation strings and
   bibliography entries produced by the probe and by `typst compile` are byte-identical
   (§3.4) **[verified]**.
2. **Style names:** `hayagriva::archive::ArchivedStyle::by_name(name)` accepts exactly the
   names typst's `bibliography(style: "...")` accepts (typst calls that function; unknown name →
   `error: unknown style: not-a-style`). 80 bundled styles, 63 CSL locales. A CSL file
   (`IndependentStyle::from_xml`) works in both too (measured identical output with the
   official `ieee.csl`) **[verified]**.
3. **Locators:** typst passes a cite supplement (`@key[p. 12]`) to hayagriva as
   `SpecificLocator(Locator::Custom, LocatorPayload::Transparent(..))`, i.e. verbatim text.
   writer must therefore store the locator as free text and pass `Locator::Custom` +
   `LocatorPayload::Str(text)` to get identical output (APA/IEEE print `p. 12` either way, but
   `chicago-author-date` prints `(Knuth 1984, 12)` for `Locator::Page` vs `(Knuth 1984, p. 12)`
   for `Custom`; see §2.6) **[verified]**.
4. **Cross-ref syntax recommendation:** mirror typst: `@label` (reference) and `@key` (cite)
   share one syntax; a target is a citation iff the key is in the bibliography and not a
   document label (typst errors when both). Labels get pandoc-crossref-style prefixes
   `fig:`, `tbl:`, `eq:`, `sec:`, `lst:`, `fn:` only as a *convention* for readability — typst
   allows `:` in labels and trims a trailing `.`/`:` from `@x.` **[verified]**. Supplement in
   brackets: `@fig:plot[Fig.]`, `@key[p. 12]`. comrak's autolink extension cannot turn
   `@fig:plot`, ` @doe2020` or `[@doe2020, p. 12]` into links (email rule needs ≥1
   alphanumeric/`.+-_` char *before* `@` and a dot in the domain) **[verified by code + run]**.
5. **Numbering:** typst defaults **[verified by `typst query` + compile]**: figures numbered
   `"1"` with one counter *per kind* (image / table / raw → supplements "Figure", "Table",
   "Listing"); headings `numbering: none`; equations `numbering: none`; footnotes `"1"`.
   Referencing an unnumbered heading/equation is a hard error (`cannot reference heading
   without numbering`). writer must compute numbers itself for HTML/DOCX and emit `set` rules in
   the typst template that reproduce the same scheme (per-chapter snippet verified in §5.3).
   LaTeX numbers on its own (`\label`/`\ref`); DOCX needs `SEQ` fields + `Caption` style if
   numbers should survive edits in Word, else literal numbers.
6. **Footnotes:** comrak emits `<section class="footnotes" data-footnotes>`; DOCX needs a
   `word/footnotes.xml` part with `<w:footnote w:id>` + `<w:footnoteReference w:id>` runs
   (verified from pandoc output); typst `#footnote[..]`; LaTeX `\footnote{..}`. Note-class CSL
   styles (`chicago-notes`, `chicago-shortened-notes`, `gb-7714-2015-note`,
   `modern-humanities-research-association-notes`, `turabian-fullnote-8`) are detected with
   `style.settings.class == StyleClass::Note`; typst then wraps every normal-form cite in a
   footnote automatically (measured) and writer must do the same for HTML/DOCX.
7. **LaTeX citations:** default to **biblatex** (`\autocite{key}`, `\autocite[p. 12]{key}`,
   `\textcite{key}`, `\autocites{a}{b}`, `\printbibliography`), with a config knob
   `latex.cite = "biblatex" | "natbib" | "plain"` (natbib: `\citep`/`\citet`/`\citealp`/
   `\citeyearpar`, `\bibliography{file}`; plain: `\cite{a,b}` + `\bibliography{file}`). All
   command names verified against TeX Live 2026 (`biblatex` v3.21, `natbib`) and pandoc 3.12.1's
   `--biblatex`/`--natbib` writers (§7).

## 1. Versions (how checked)

| crate / tool | version | checked by |
|---|---|---|
| hayagriva | 0.10.1 (latest) | `cargo info hayagriva`; source at `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/hayagriva-0.10.1`; license MIT OR Apache-2.0; edition 2024 (so MSRV ≥ 1.85; `rust-version` not declared) |
| citationberg (CSL model, dep of hayagriva, re-exported as `hayagriva::citationberg`) | 0.7.0 | hayagriva's normalized Cargo.toml: `[dependencies.citationberg] version = "0.7.0"`; source dir present |
| biblatex (BibTeX parser, dep of hayagriva, feature `biblatex` on by default) | 0.12.0 | hayagriva Cargo.toml: `version = "0.12.0"`, `features = ["unic-langid"]`, `optional = true` |
| typst | 0.15.1 | `https://raw.githubusercontent.com/typst/typst/v0.15.1/Cargo.toml`: `hayagriva = "0.10.1"`, `comemo = "0.5.1"`, workspace `version = "0.15.1"`; typst-cli 0.15.1 built locally and used for all typst measurements |
| comrak | 0.56.0 (presenter_rs pin; crates.io latest 0.57.0) | `cargo info comrak`; `rust-version: 1.89`; source dir present |
| pandoc | 3.12.1 (installed) | used only as a reference implementation for DOCX/LaTeX markup |
| biblatex (LaTeX package) | 3.21 | `\def\abx@version{3.21}` in `$(kpsewhich biblatex.sty)` (TeX Live 20260301) |
| natbib (LaTeX package) | installed | `\ProvidesPackage{natbib}` in `$(kpsewhich natbib.sty)` |
| pandoc-crossref | **not installed** | `which pandoc-crossref` |

hayagriva Cargo features **[verified]**: `default = ["biblatex", "archive"]`; `archive = ["ciborium"]`
(bundled CSL styles + locales as CBOR); `csl-json = ["citationberg/json"]`; `cli` (binary, not needed).
Use `hayagriva = "0.10.1"` with defaults. **Do not add `citationberg` as a direct dependency** —
use `hayagriva::citationberg::…` (the first probe build failed with `E0433` until this was changed).

## 2. hayagriva 0.10.1 API (all **[verified]** from source)

### 2.1 Loading a library (`src/io.rs`, `src/lib.rs`)

```rust
// hayagriva::io
pub fn from_yaml_str(s: &str) -> Result<Library, serde_yaml::Error>;          // hayagriva YAML
pub fn to_yaml_str(entries: &Library) -> Result<String, serde_yaml::Error>;
pub enum BibLaTeXError { Parse(biblatex::ParseError), Type(biblatex::TypeError) } // impl Display:
//   "biblatex parse error: {err}" / "biblatex type error: {err}"
pub fn from_biblatex_str(biblatex: &str) -> Result<Library, Vec<BibLaTeXError>>;  // BibTeX/BibLaTeX
pub fn from_biblatex(bibliography: &biblatex::Bibliography) -> Result<Library, Vec<biblatex::TypeError>>;

// hayagriva (lib.rs)
pub struct Library(IndexMap<String, Entry>);          // insertion-ordered, keyed by cite key
impl Library {
    pub fn new() -> Self;
    pub fn push(&mut self, entry: &Entry);            // IndexMap::insert → later entry with same key replaces
    pub fn get(&self, key: &str) -> Option<&Entry>;   // None == "unknown citation key"
    pub fn iter(&self) -> impl Iterator<Item = &Entry>;
    pub fn keys(&self) -> impl Iterator<Item = &str>;
    pub fn remove(&mut self, key: &str) -> Option<Entry>;
    pub fn len(&self) -> usize; pub fn is_empty(&self) -> bool; pub fn nth(&self, n: usize) -> Option<&Entry>;
}
impl FromIterator<Entry> for Library; impl IntoIterator for Library; impl<'a> IntoIterator for &'a Library;
impl Entry { pub fn key(&self) -> &str; pub fn entry_type(&self) -> &EntryType; pub fn title(&self) -> Option<&FormatString>;
             pub fn doi(&self) -> Option<&str>; pub fn url_any(&self) -> Option<&QualifiedUrl>; pub fn date_any(&self) -> Option<&Date>; /* … */ }
```

Measured error behaviour (`src/bin/dup.rs`):

- duplicate key in one `.bib` string → **whole file rejected**: `biblatex parse error: duplicate key "k": 52-103`
  (numbers are a byte span). typst has its own check across files: `duplicate bibliography keys: …`.
- malformed field (`year = {not-a-year}`) → `biblatex type error: wrong number of digits: 50-50` (whole file rejected).
- truncated file → `biblatex parse error: unexpected end of file: 28-28`.
- `Library::get("nope")` → `None` (unknown key is *writer's* diagnostic to raise; the driver never sees it).
- hayagriva YAML: `from_yaml_str("k1:\n  type: Book\n  title: Y\n  author: Kwan, Kevin\n  date: 2014\n")` → keys `["k1"]`.
- `.bib` entries loaded by the probe: `@article` with `doi`, `@book`, `@inproceedings` with `url` → `lib.len() == 3`,
  `entry_type() == Article`, `doi() == Some("10.1000/xyz123")`.

### 2.2 Bundled styles and locales (`src/csl/archive.rs`, feature `archive`)

```rust
pub enum ArchivedStyle { Alphanumeric, AmericanAnthropologicalAssociation, /* … 80 variants … */ TurabianFullnote8 }
impl ArchivedStyle {
    pub fn by_name(name: &str) -> Option<Self>;        // what typst calls for `style: "<name>"`
    pub fn by_id(id: &str) -> Option<Self>;            // by CSL id URL
    pub fn all() -> &'static [Self];                   // 80 entries
    pub fn get(self) -> citationberg::Style;           // deserializes the embedded CBOR; all 80 are Style::Independent
    pub fn names(self) -> &'static [&'static str];     // accepted names incl. aliases
    pub fn display_name(self) -> &'static str;
    pub fn csl_id(self) -> &'static str;
}
pub const LOCALES: &[&[u8]];
pub fn locales() -> Vec<citationberg::Locale>;         // 63 locales (measured)
```

Full list printed by the probe (`class` is `IndependentStyle.settings.class`; **Note** styles put
citations in footnotes):

```
InText    ["alphanumeric"]
InText    ["american-anthropological-association"]
InText    ["american-chemical-society"]
InText    ["american-geophysical-union"]
InText    ["american-institute-of-aeronautics-and-astronautics"]
InText    ["american-institute-of-physics"]
InText    ["american-medical-association"]
InText    ["american-meteorological-society"]
InText    ["american-physics-society"]
InText    ["american-physiological-society"]
InText    ["american-political-science-association"]
InText    ["american-psychological-association", "apa"]
InText    ["american-society-for-microbiology"]
InText    ["american-society-of-civil-engineers"]
InText    ["american-society-of-mechanical-engineers"]
InText    ["american-sociological-association"]
InText    ["angewandte-chemie"]
InText    ["annual-reviews"]
InText    ["annual-reviews-author-date"]
InText    ["associacao-brasileira-de-normas-tecnicas"]
InText    ["association-for-computing-machinery"]
InText    ["biomed-central"]
InText    ["bristol-university-press"]
InText    ["british-medical-journal", "bmj"]
InText    ["cell"]
InText    ["chicago-author-date"]
Note      ["chicago-notes", "chicago-fullnotes"]
Note      ["chicago-shortened-notes"]
InText    ["copernicus"]
InText    ["cse-citation-sequence-brackets-8th-edition", "council-of-science-editors"]
InText    ["cse-name-year", "council-of-science-editors-author-date"]
InText    ["current-opinion"]
InText    ["deutsche-gesellschaft-für-psychologie"]
InText    ["deutsche-sprache"]
InText    ["elsevier-harvard"]
InText    ["elsevier-vancouver"]
InText    ["elsevier-with-titles"]
InText    ["frontiers"]
InText    ["future-medicine"]
InText    ["future-science"]
InText    ["gb-7714-2005-numeric"]
InText    ["gb-7714-2015-author-date"]
Note      ["gb-7714-2015-note"]
InText    ["gb-7714-2015-numeric"]
InText    ["gost-r-705-2008-numeric"]
InText    ["harvard-cite-them-right"]
InText    ["institute-of-electrical-and-electronics-engineers", "ieee"]
InText    ["institute-of-physics-numeric"]
InText    ["iso-690-author-date"]
InText    ["iso-690-numeric"]
InText    ["karger"]
InText    ["mary-ann-liebert-vancouver"]
Note      ["modern-humanities-research-association-notes", "modern-humanities-research-association"]
InText    ["modern-language-association", "mla", "modern-language-association-8", "mla-8"]
InText    ["multidisciplinary-digital-publishing-institute"]
InText    ["nature"]
InText    ["nlm-citation-sequence", "vancouver"]
InText    ["nlm-citation-sequence-superscript", "vancouver-superscript"]
InText    ["pensoft"]
InText    ["public-library-of-science", "plos"]
InText    ["royal-society-of-chemistry"]
InText    ["sage-vancouver"]
InText    ["sist02"]
InText    ["spie"]
InText    ["springer-basic"]
InText    ["springer-basic-author-date"]
InText    ["springer-fachzeitschriften-medizin-psychologie"]
InText    ["springer-humanities-author-date"]
InText    ["springer-lecture-notes-in-computer-science"]
InText    ["springer-mathphys"]
InText    ["springer-socpsych-author-date"]
InText    ["springer-vancouver"]
InText    ["taylor-and-francis-chicago-author-date"]
InText    ["taylor-and-francis-national-library-of-medicine"]
InText    ["the-institution-of-engineering-and-technology"]
InText    ["the-lancet"]
InText    ["thieme"]
InText    ["trends"]
InText    ["turabian-author-date"]
Note      ["turabian-fullnote-8"]
```

Display names of the common ones: `ieee` = "IEEE Reference Guide version 11.29.2023" (csl id
`http://www.zotero.org/styles/ieee`), `apa` = "APA Style 7th edition", `chicago-author-date` =
"Chicago Manual of Style 18th edition (author-date)", `chicago-notes` = "Chicago Manual of Style
18th edition (notes and bibliography)", `mla` = "MLA Handbook 9th edition", `nature` = "Nature",
`vancouver` = "NLM/Vancouver: Citing Medicine 2nd edition (citation-sequence)", `springer-basic` =
"Springer - Basic (numeric, brackets)". There is **no `harvard` alias** (use
`harvard-cite-them-right` or `elsevier-harvard`) and **no `acm` alias** (`association-for-computing-machinery`).

### 2.3 CSL style objects (citationberg 0.7.0, via `hayagriva::citationberg`)

```rust
pub enum Style { Independent(IndependentStyle), Dependent(DependentStyle) }
impl Style { pub fn from_xml(xml: &str) -> XmlDeResult<Self>; }
impl IndependentStyle { pub fn from_xml(xml: &str) -> XmlDeResult<Self>; }   // load a user .csl file
pub struct IndependentStyle { pub settings: IndependentStyleSettings /* .class: StyleClass */, pub info: StyleInfo /* .id: String */, pub citation: Citation, pub bibliography: Option<Bibliography>, /* … */ }
pub enum StyleClass { InText, Note }          // Note == citations belong in footnotes
pub struct LocaleCode(pub String); impl LocaleCode { pub fn en_us() -> Self; }
pub struct Locale { pub lang: Option<LocaleCode>, pub info: Option<LocaleInfo>, pub terms: Option<Terms>, pub date: Vec<Date>, /* … */ }
// citationberg::taxonomy
pub enum Locator { Act, Appendix, ArticleLocator, Book, Canon, Chapter, Column, Elocation, Equation, Figure, Folio, Issue, Line, Note, Opus, Page, Paragraph, Part, Rule, Scene, Section, SubVerbo, Supplement, Table, Timestamp, Title, TitleLocator, Verse, Volume, Custom }
// citationberg formatting enums used in hayagriva::Formatting
pub enum FontStyle { Normal, Italic }        pub enum FontVariant { Normal, SmallCaps }
pub enum FontWeight { Normal, Bold, Light }  pub enum TextDecoration { None, Underline }
pub enum VerticalAlign { None, Baseline, Sup, Sub }
pub enum Display { Block, LeftMargin, RightInline, Indent }   // Elem.display
pub enum SecondFieldAlign { Margin, Flush }                   // RenderedBibliography.second_field_align
```

All 80 archived styles deserialize to `Style::Independent` (the probe matched `Style::Independent(s)`
for every entry of `ArchivedStyle::all()` without hitting the `Dependent` arm). A *user-supplied*
`.csl` may still be dependent (parent link) — report that as unsupported.

### 2.4 Driver, requests, items (`src/csl/mod.rs`)

```rust
pub struct BibliographyDriver<'a, T: EntryLike> { /* citations: Vec<CitationRequest>, citation_number_offset */ }
impl<'a, T: EntryLike> BibliographyDriver<'a, T> {
    pub fn new() -> Self;
    pub fn with_citation_number_offset(self, offset: usize) -> Self;   // for several bibliographies
    pub fn citation(&mut self, req: CitationRequest<'a, T>);            // record one in-text citation (in document order!)
}
impl<T: EntryLike + Hash + PartialEq + Eq + Debug> BibliographyDriver<'_, T> {
    pub fn finish(self, request: BibliographyRequest<'_>) -> Rendered;  // renders ALL citations + bibliography at once
}
pub struct CitationRequest<'a, T: EntryLike> {
    pub items: Vec<CitationItem<'a, T>>,
    style: &'a IndependentStyle,              // private
    pub locale: Option<LocaleCode>,
    pub locale_files: &'a [Locale],
    note_number: Option<usize>,               // private; kept only if style.settings.class == StyleClass::Note
}
impl<'a, T: EntryLike> CitationRequest<'a, T> {
    pub fn new(items: Vec<CitationItem<'a, T>>, style: &'a IndependentStyle, locale: Option<LocaleCode>,
               locale_files: &'a [Locale], note_number: Option<usize>) -> Self;
    pub fn from_items(items: Vec<CitationItem<'a, T>>, style: &'a IndependentStyle, locale_files: &'a [Locale]) -> Self;
}
pub struct BibliographyRequest<'a> { pub style: &'a IndependentStyle, pub locale: Option<LocaleCode>, pub locale_files: &'a [Locale] }
impl<'a> BibliographyRequest<'a> { pub fn new(style: &'a IndependentStyle, locale: Option<LocaleCode>, locale_files: &'a [Locale]) -> Self; }
pub struct CitationItem<'a, T: EntryLike> {
    pub entry: &'a T,
    pub locator: Option<SpecificLocator<'a>>,
    pub locale: Option<LocaleCode>,
    pub hidden: bool,                          // true == typst `form: none` (in bibliography, prints nothing)
    pub purpose: Option<CitePurpose>,
    initial_idx: usize,                        // private, set by driver.citation()
}
impl<'a, T: EntryLike> CitationItem<'a, T> {
    pub fn with_entry(entry: &'a T) -> Self;
    pub fn with_locator(entry: &'a T, locator: Option<SpecificLocator<'a>>) -> Self;
    pub fn new(entry: &'a T, locator: Option<SpecificLocator<'a>>, locale: Option<LocaleCode>, hidden: bool, purpose: Option<CitePurpose>) -> Self;
    pub fn kind(self, purpose: CitePurpose) -> Self;
}
pub struct SpecificLocator<'a>(pub Locator, pub LocatorPayload<'a>);
pub enum LocatorPayload<'a> { Str(&'a str), Transparent(TransparentLocator) }   // Transparent → ElemChild::Transparent in output
pub enum CitePurpose { Author, Year, Full, Prose }      // typst form: "author" | "year" | "full" | "prose"; "normal" == None
pub fn standalone_citation<T: EntryLike>(/* … */);      // exists; not needed
```

`hayagriva::Entry` implements `EntryLike + Hash + Eq + Debug` (`impl EntryLike for Entry` at
`src/csl/taxonomy.rs:138`), so `BibliographyDriver<Entry>` works directly.

Important semantics **[verified]**:
- Citations must be fed **in document order**; numeric styles number by first citation, `ibid`/
  subsequent-note forms depend on order. `finish()` consumes the driver, so writer must collect all
  cites during a pass over the IR, then render, then substitute (two-phase, like typst's introspection loop).
- `CitationRequest::new(.., note_number: Some(n))` only matters for `StyleClass::Note` styles
  (`note_number.filter(|_| style.settings.class == StyleClass::Note)`); `RenderedCitation.note_number`
  echoes it (probe: `Some(7)` for chicago-notes, `None` for ieee/apa). Pass the footnote number that
  writer assigns to the cite's footnote so `near-note`/`ibid` logic works.
- `with_citation_number_offset` is for documents with several bibliographies (rare; skip in v1).

### 2.5 Rendered output types (`src/csl/mod.rs`, `src/csl/elem.rs`)

```rust
pub struct Rendered { pub bibliography: Option<RenderedBibliography>, pub citations: Vec<RenderedCitation> } // citations[i] ↔ i-th driver.citation()
pub struct RenderedCitation { pub note_number: Option<usize>, pub citation: ElemChildren }
pub struct RenderedBibliography {
    pub hanging_indent: bool,                              // apa: true, ieee: false
    pub second_field_align: Option<SecondFieldAlign>,      // ieee: Some(Flush) → items[].first_field is Some("[1]")
    pub line_spacing: NonZeroI16,                          // apa: 2
    pub entry_spacing: i16,                                // chicago-notes: 1
    pub items: Vec<BibliographyItem>,
}
pub struct BibliographyItem { pub key: String, pub first_field: Option<ElemChild>, pub content: ElemChildren }

pub struct ElemChildren(pub Vec<ElemChild>);
impl ElemChildren {
    pub fn is_empty(&self) -> bool;
    pub fn find_meta(&self, meta: &ElemMeta) -> Option<&Elem>;
    pub fn find_names(&self) -> Option<&Elem>;
    pub fn find_elem_by<F: Fn(&Elem) -> bool>(&self, f: &F) -> Option<&Elem>;
    pub fn write_buf(&self, w: &mut impl std::fmt::Write, format: BufWriteFormat) -> std::fmt::Result;
}
impl fmt::Display for ElemChildren   // `{}` = VT100 escapes, `{:#}` = Plain   (same for ElemChild)
pub enum ElemChild {
    Text(Formatted),
    Elem(Elem),
    Markup(String),                                   // raw typst markup (only from typst-flavoured CSL use; just emit text)
    Link { text: Formatted, url: String },            // DOI / URL links
    Transparent { cite_idx: usize, format: Formatting }, // placeholder for a LocatorPayload::Transparent; consumer substitutes
}
pub struct Elem { pub children: ElemChildren, pub display: Option<Display>, pub meta: Option<ElemMeta> }
pub enum ElemMeta { Names(Vec<(Vec<Person>, NameVariable)>), Date, Text, Number, Label, CitationNumber, Name(NameVariable, usize), Entry(usize), CitationLabel }
pub struct Formatted { pub text: String, pub formatting: Formatting }
#[derive(Default, Copy, PartialEq)]
pub struct Formatting { pub font_style: FontStyle, pub font_variant: FontVariant, pub font_weight: FontWeight, pub text_decoration: TextDecoration, pub vertical_align: VerticalAlign }
pub enum BufWriteFormat { #[default] Plain, VT100, Html }
```

`BufWriteFormat::Html` (built-in, `Formatting::write_start/write_css`) wraps any non-default
formatting in `<span style="…">` with these CSS declarations: `font-style: italic;`,
`font-weight: bold;` / `font-weight: lighter;`, `text-decoration: underline;`,
`font-variant: small-caps;`, `vertical-align: sub;` / `vertical-align: super;`; links become
`<a href="URL">text</a>`; `Transparent` writes nothing; text is **not HTML-escaped** (`w.write_str(&t.text)`),
so writer should walk the tree itself and escape (§2.7) rather than use `Html` directly.

Measured tree shapes (`src/bin/tree.rs`): `Elem.display` was `None` for every node in ieee / apa /
nature / alphanumeric / chicago-notes output; `ElemMeta::Entry(i)` wraps each cited item inside a
citation (`i` = index into `CitationRequest.items`), `CitationNumber` wraps the number, `Names` /
`Name(Author, k)` wrap authors, `Date`, `Label` (e.g. "pp.", "no."), `Number`, `Text`.
`nature` renders the in-text number with `vertical_align: Sup` — so the DOCX walker **must** map
`Sup` to `<w:vertAlign w:val="superscript"/>`. `alphanumeric` uses `CitationLabel` ("DS20").
IEEE bibliography items carry `first_field = Some(Elem{meta: CitationNumber, "[1]"})` and
`second_field_align = Some(Flush)`; nature's first field is `"1."`.

### 2.6 Locators: what to pass (`src/bin/locator.rs`, measured)

| style | `Locator::Page`, `Str("12")` | `Locator::Custom`, `Str("p. 12")` (typst's way) | `Locator::Chapter`, `Str("3")` | group `[doe; knuth p. 12]` |
|---|---|---|---|---|
| ieee | `[1, p. 12]` | `[1, p. 12]` | `[1, Ch. 3]` | `[1, p. 12], [2]` |
| apa | `(Knuth, 1984, p. 12)` | `(Knuth, 1984, p. 12)` | `(Knuth, 1984, Chapter 3)` | `(Doe & Smith, 2020; Knuth, 1984, p. 12)` |
| chicago-author-date | `(Knuth 1984, 12)` | `(Knuth 1984, p. 12)` | `(Knuth 1984, chap. 3)` | `(Doe and Smith 2020; Knuth 1984, p. 12)` |
| nature | `1` | `1` | `1` | `1,2` |
| mla | `(Knuth 12)` | `(Knuth, p. 12)` | `(Knuth, ch. 3)` | `(Doe and Smith; Knuth, p. 12)` |
| chicago-notes | `Donald E. Knuth, The TeXbook (Addison-Wesley, 1984), 12.` | `Knuth, The TeXbook, p. 12.` (subsequent) | `Knuth, The TeXbook, chap. 3.` | `…; Knuth, The TeXbook, p. 12.` |

typst (`crates/typst-library/src/model/bibliography.rs`, v0.15.1) builds the locator as
`SpecificLocator(citationberg::taxonomy::Locator::Custom, hayagriva::LocatorPayload::Transparent(TransparentLocator::new(c.clone())))`
and substitutes the supplement content back when it meets `ElemChild::Transparent` **[verified via WebFetch of the raw file]**.
Decision: writer stores the supplement as free text and passes `Locator::Custom` + `LocatorPayload::Str(&text)`
(same rendering, no Transparent placeholder to substitute). Only if a future "smart locator"
feature parses `p. 12` → `Locator::Page` should the Page variant be used — it would then diverge from typst's PDF.

### 2.7 Walking `ElemChildren` into HTML and into DOCX runs

```rust
use hayagriva::citationberg::{FontStyle, FontVariant, FontWeight, TextDecoration, VerticalAlign};
use hayagriva::{Elem, ElemChild, ElemChildren, Formatting};

/// Flat run list: the shape DOCX wants (one <w:r> per run) and the shape the HTML writer can
/// serialize with <em>/<strong>/<sup>/<sub>/<span class="smallcaps">/<u>/<a>.
pub struct Run { pub text: String, pub italic: bool, pub bold: bool, pub small_caps: bool, pub underline: bool, pub sup: bool, pub sub: bool, pub href: Option<String> }

fn run_from(text: &str, f: &Formatting, href: Option<String>) -> Run {
    Run { text: text.to_owned(),
          italic: f.font_style == FontStyle::Italic,
          bold: f.font_weight == FontWeight::Bold,          // FontWeight::Light exists; map to normal
          small_caps: f.font_variant == FontVariant::SmallCaps,
          underline: f.text_decoration == TextDecoration::Underline,
          sup: f.vertical_align == VerticalAlign::Sup,
          sub: f.vertical_align == VerticalAlign::Sub,
          href }
}

pub fn flatten(children: &ElemChildren, out: &mut Vec<Run>) {
    for child in &children.0 {
        match child {
            ElemChild::Text(t) => out.push(run_from(&t.text, &t.formatting, None)),
            ElemChild::Elem(Elem { children, .. }) => flatten(children, out),  // display/meta ignored for runs
            ElemChild::Markup(m) => out.push(run_from(m, &Formatting::default(), None)),
            ElemChild::Link { text, url } => out.push(run_from(&text.text, &text.formatting, Some(url.clone()))),
            ElemChild::Transparent { .. } => {} // never produced when we pass LocatorPayload::Str
        }
    }
}
```

HTML conventions worth copying from pandoc/citeproc (verified from pandoc 3.12.1 output):
`<span class="citation" data-cites="knuth1984 lee2019">(Knuth 1984, 12; Lee et al. 2019)</span>` and a
bibliography `<div id="refs" class="references csl-bib-body hanging-indent" role="list">` with
`<div id="ref-KEY" class="csl-entry" role="listitem">…</div>`. typst's own HTML export uses
`<section role="doc-bibliography">` and `<span class="prefix">[1]</span>` for the first field. For
IEEE-like `second_field_align`, render `first_field` in a `<span class="csl-left-margin">` and the
rest in `<span class="csl-right-inline">` (citeproc-js class names, **[unverified]** as a standard but widely styled).

### 2.8 Probe code and output (`src/main.rs`, run with `cargo run -q`)

```rust
//! hayagriva 0.10.1 probe: load BibTeX, cite in IEEE/APA/chicago-notes, walk rendered output.
use hayagriva::citationberg::taxonomy::Locator;
use hayagriva::citationberg::{FontStyle, FontVariant, FontWeight, Style, TextDecoration, VerticalAlign};
use hayagriva::archive::{locales, ArchivedStyle};
use hayagriva::{
    BibliographyDriver, BibliographyRequest, BufWriteFormat, CitationItem, CitationRequest,
    CitePurpose, ElemChild, ElemChildren, Entry, Library, LocatorPayload, SpecificLocator,
};

const BIB: &str = r#"
@article{doe2020,
  author = {Doe, Jane and Smith, John},
  title = {A Study of Things},
  journal = {Journal of Stuff},
  year = {2020},
  volume = {12},
  number = {3},
  pages = {45--67},
  doi = {10.1000/xyz123}
}
@book{knuth1984,
  author = {Knuth, Donald E.},
  title = {The {\TeX}book},
  publisher = {Addison-Wesley},
  address = {Reading, MA},
  year = {1984}
}
@inproceedings{lee2019,
  author = {Lee, Alice and Kim, Bob and Chen, Carol},
  title = {Fast Widgets},
  booktitle = {Proceedings of the Widget Conference},
  year = {2019},
  pages = {1--10},
  url = {https://example.org/widgets},
  publisher = {ACM}
}
"#;

/// Walk rendered children into a marker string: *italic*, **bold**, ^sup^, _sub_, {sc ...}, [text](url).
fn walk(children: &ElemChildren, out: &mut String) {
    for child in &children.0 {
        match child {
            ElemChild::Text(f) => push_fmt(&f.text, &f.formatting, out),
            ElemChild::Elem(e) => walk(&e.children, out),
            ElemChild::Markup(m) => out.push_str(&format!("{{markup:{m}}}")),
            ElemChild::Link { text, url } => {
                out.push('[');
                push_fmt(&text.text, &text.formatting, out);
                out.push_str(&format!("]({url})"));
            }
            ElemChild::Transparent { cite_idx, .. } => out.push_str(&format!("{{transparent:{cite_idx}}}")),
        }
    }
}

fn push_fmt(text: &str, f: &hayagriva::Formatting, out: &mut String) {
    let italic = f.font_style == FontStyle::Italic;
    let bold = f.font_weight == FontWeight::Bold;
    let sc = f.font_variant == FontVariant::SmallCaps;
    let ul = f.text_decoration == TextDecoration::Underline;
    let sup = f.vertical_align == VerticalAlign::Sup;
    let sub = f.vertical_align == VerticalAlign::Sub;
    if sup { out.push('^'); }
    if sub { out.push('_'); }
    if bold { out.push_str("**"); }
    if italic { out.push('*'); }
    if sc { out.push_str("{sc:"); }
    if ul { out.push_str("{ul:"); }
    out.push_str(text);
    if ul { out.push('}'); }
    if sc { out.push('}'); }
    if italic { out.push('*'); }
    if bold { out.push_str("**"); }
    if sub { out.push('_'); }
    if sup { out.push('^'); }
}

fn independent(name: &str) -> hayagriva::citationberg::IndependentStyle {
    match ArchivedStyle::by_name(name).expect("style name").get() {
        Style::Independent(s) => s,
        Style::Dependent(d) => panic!("dependent style {name}: parent {:?}", d.parent_link),
    }
}

fn run_style(name: &str, lib: &Library) {
    let style = independent(name);
    let locales = locales();
    println!("\n===== style {name}: class={:?} (StyleClass), csl id={}", style.settings.class, style.info.id);
    let doe: &Entry = lib.get("doe2020").unwrap();
    let knuth = lib.get("knuth1984").unwrap();
    let lee = lib.get("lee2019").unwrap();

    let mut driver: BibliographyDriver<Entry> = BibliographyDriver::new();
    let labels = [
        "1 plain @doe2020",
        "2 page locator @knuth1984[p. 12]",
        "3 two keys [@doe2020; @lee2019]",
        "4 prose form (textcite) @doe2020",
        "5 year only @doe2020",
        "6 author only @lee2019",
        "7 repeat @doe2020 (ibid/number reuse)",
    ];
    driver.citation(CitationRequest::from_items(vec![CitationItem::with_entry(doe)], &style, &locales));
    driver.citation(CitationRequest::from_items(
        vec![CitationItem::with_locator(knuth, Some(SpecificLocator(Locator::Page, LocatorPayload::Str("12"))))],
        &style, &locales));
    driver.citation(CitationRequest::from_items(
        vec![CitationItem::with_entry(doe), CitationItem::with_entry(lee)], &style, &locales));
    driver.citation(CitationRequest::from_items(
        vec![CitationItem::with_entry(doe).kind(CitePurpose::Prose)], &style, &locales));
    driver.citation(CitationRequest::from_items(
        vec![CitationItem::with_entry(doe).kind(CitePurpose::Year)], &style, &locales));
    driver.citation(CitationRequest::from_items(
        vec![CitationItem::with_entry(lee).kind(CitePurpose::Author)], &style, &locales));
    // note styles: pass the footnote number so near-note/ibid logic works
    driver.citation(CitationRequest::new(vec![CitationItem::with_entry(doe)], &style, None, &locales, Some(7)));

    let rendered = driver.finish(BibliographyRequest::new(&style, None, &locales));
    for (i, c) in rendered.citations.iter().enumerate() {
        let mut html = String::new();
        c.citation.write_buf(&mut html, BufWriteFormat::Html).unwrap();
        let mut markers = String::new();
        walk(&c.citation, &mut markers);
        println!("cite {:<40} note_number={:?}\n   plain  : {:#}\n   html   : {}\n   markers: {}", labels[i], c.note_number, c.citation, html, markers);
    }
    match &rendered.bibliography {
        None => println!("bibliography: None (style has no bibliography section)"),
        Some(b) => {
            println!("bibliography: hanging_indent={} second_field_align={:?} line_spacing={} entry_spacing={}",
                b.hanging_indent, b.second_field_align, b.line_spacing, b.entry_spacing);
            for item in &b.items {
                let mut html = String::new();
                item.content.write_buf(&mut html, BufWriteFormat::Html).unwrap();
                let mut markers = String::new();
                walk(&item.content, &mut markers);
                let ff = item.first_field.as_ref().map(|f| { let mut s = String::new(); f.write_buf(&mut s, BufWriteFormat::Plain).unwrap(); s });
                println!(" [{}] first_field={:?}\n   html   : {}\n   markers: {}", item.key, ff, html, markers);
            }
        }
    }
}

fn main() {
    let lib: Library = hayagriva::io::from_biblatex_str(BIB).unwrap_or_else(|errs| {
        for e in &errs { eprintln!("bib error: {e}"); }
        panic!("bib parse failed");
    });
    println!("loaded {} entries: {:?}", lib.len(), lib.keys().collect::<Vec<_>>());
    println!("unknown key lookup: {:?}", lib.get("nope").map(|e| e.key()));
    let yaml = hayagriva::io::from_yaml_str("k1:\n  type: Book\n  title: Y\n  author: Kwan, Kevin\n  date: 2014\n").unwrap();
    println!("yaml lib keys: {:?}", yaml.keys().collect::<Vec<_>>());
    println!("doe2020 doi={:?} entry_type={:?}", lib.get("doe2020").unwrap().doi(), lib.get("doe2020").unwrap().entry_type());
    println!("locales bundled: {}", locales().len());

    for name in ["ieee", "apa", "chicago-notes"] { run_style(name, &lib); }

    println!("\n===== ArchivedStyle::all(): {} styles", ArchivedStyle::all().len());
    for s in ArchivedStyle::all() {
        let class = match s.get() { Style::Independent(i) => format!("{:?}", i.settings.class), Style::Dependent(_) => "Dependent".into() };
        println!("{:<9} {:?} display={:?} csl_id={}", class, s.names(), s.display_name(), s.csl_id());
    }

    // ---- comrak 0.56 checks: '@' autolink rule and footnotes HTML
    println!("\n===== comrak");
    let mut o = comrak::Options::default();
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.parse.relaxed_autolinks = false;
    for src in ["see @fig:plot and @doe2020 ok", "text [@doe2020, p. 12] end", "mail me a@b.org or see@doe2020 or x@y", "foot[^1]\n\n[^1]: note text"] {
        println!("--- {src:?}\n{}", comrak::markdown_to_html(src, &o));
    }
}
```

Output (abridged to the citation/bibliography lines; `plain` = `{:#}`, `html` = `BufWriteFormat::Html`, `markers` = the walker):

```
loaded 3 entries: ["doe2020", "knuth1984", "lee2019"]
unknown key lookup: None
yaml lib keys: ["k1"]
doe2020 doi=Some("10.1000/xyz123") entry_type=Article
locales bundled: 63

===== style ieee: class=InText (StyleClass), csl id=http://www.zotero.org/styles/ieee
cite 1 plain @doe2020                         note_number=None   plain: [1]
cite 2 page locator @knuth1984[p. 12]         note_number=None   plain: [2, p. 12]
cite 3 two keys [@doe2020; @lee2019]          note_number=None   plain: [1], [3]
cite 4 prose form (textcite) @doe2020         note_number=None   plain: J. Doe and J. Smith [1]
cite 5 year only @doe2020                     note_number=None   plain: 2020
cite 6 author only @lee2019                   note_number=None   plain: A. Lee, B. Kim, and C. Chen
cite 7 repeat @doe2020 (ibid/number reuse)    note_number=None   plain: [1]
bibliography: hanging_indent=false second_field_align=Some(Flush) line_spacing=1 entry_spacing=0
 [doe2020] first_field=Some("[1]")
   html   : J. Doe and J. Smith, “A Study of Things,” <span style="font-style: italic;">Journal of Stuff</span>, vol. 12, no. 3, pp. 45–67, 2020, doi: <a href="https://doi.org/10.1000/xyz123">10.1000/xyz123</a>.
   markers: J. Doe and J. Smith, “A Study of Things,” *Journal of Stuff*, vol. 12, no. 3, pp. 45–67, 2020, doi: [10.1000/xyz123](https://doi.org/10.1000/xyz123).
 [knuth1984] first_field=Some("[2]")
   markers: D. E. Knuth, *The TeXbook*. Reading, MA: Addison-Wesley, 1984.
 [lee2019] first_field=Some("[3]")
   markers: A. Lee, B. Kim, and C. Chen, “Fast Widgets,” in *Proceedings of the Widget Conference*, ACM,  2019, pp. 1–10. [Online].  Available: [https://example.org/widgets](https://example.org/widgets)

===== style apa: class=InText (StyleClass), csl id=http://www.zotero.org/styles/apa
cite 1  (Doe & Smith, 2020)
cite 2  (Knuth, 1984, p. 12)
cite 3  (Doe & Smith, 2020; Lee et al., 2019)
cite 4  Doe & Smith (2020)
cite 5  2020
cite 6  Lee et al.
cite 7  (Doe & Smith, 2020)
bibliography: hanging_indent=true second_field_align=None line_spacing=2 entry_spacing=0
 [doe2020]   Doe, J., & Smith, J. (2020). A Study of Things. *Journal of Stuff*, *12*(3), 45–67. [https://doi.org/10.1000/xyz123](https://doi.org/10.1000/xyz123)
 [knuth1984] Knuth, D. E. (1984). *The TeXbook*. Addison-Wesley.
 [lee2019]   Lee, A., Kim, B., & Chen, C. (2019). Fast Widgets. *Proceedings of the Widget Conference*, 1–10. [https://example.org/widgets](https://example.org/widgets)

===== style chicago-notes: class=Note (StyleClass), csl id=http://www.zotero.org/styles/chicago-notes-bibliography
cite 1  Jane Doe and John Smith, “A Study of Things,” *Journal of Stuff* 12, no. 3 (2020): 45–67, https://doi.org/10.1000/xyz123.
cite 2  Donald E. Knuth, *The TeXbook* (Addison-Wesley, 1984), 12.
cite 3  Doe and Smith, “A Study of Things”; Alice Lee et al., “Fast Widgets,” in “Proceedings of the Widget Conference,” special issue, *Proceedings of the Widget Conference*, 2019, 1–10, https://example.org/widgets.
cite 4  Doe and Smith (“A Study of Things.”          <-- odd output for Prose in a note style; typst prints exactly the same (§3.4)
cite 5  2020
cite 6  Lee et al.
cite 7  note_number=Some(7)  Doe and Smith, “A Study of Things”.
bibliography: hanging_indent=true second_field_align=None line_spacing=1 entry_spacing=1
 [doe2020]   Doe, Jane, and John Smith. “A Study of Things.” *Journal of Stuff* 12, no. 3 (2020): 45–67. https://doi.org/10.1000/xyz123.
 [knuth1984] Knuth, Donald E. *The TeXbook*. Addison-Wesley, 1984.
 [lee2019]   Lee, Alice, Bob Kim, and Carol Chen. “Fast Widgets.” In “Proceedings of the Widget Conference.” Special issue, *Proceedings of the Widget Conference*, 2019, 1–10. https://example.org/widgets.

===== comrak
--- "see @fig:plot and @doe2020 ok"          → <p>see @fig:plot and @doe2020 ok</p>
--- "text [@doe2020, p. 12] end"             → <p>text [@doe2020, p. 12] end</p>
--- "mail me a@b.org or see@doe2020 or x@y"  → <p>mail me <a href="mailto:a@b.org">a@b.org</a> or see@doe2020 or x@y</p>
--- "foot[^1]\n\n[^1]: note text"            →
<p>foot<sup class="footnote-ref"><a href="#fn-1" id="fnref-1" data-footnote-ref>1</a></sup></p>
<section class="footnotes" data-footnotes>
<ol>
<li id="fn-1">
<p>note text <a href="#fnref-1" class="footnote-backref" data-footnote-backref data-footnote-backref-idx="1" aria-label="Back to reference 1">↩</a></p>
</li>
</ol>
</section>
```

Other measured bits (`src/bin/dup.rs`): `CitePurpose::Full` in apa → `Knuth, D. E. (1984). The TeXbook. Addison-Wesley.`;
`CitationItem::new(k, None, None, /*hidden*/ true, None)` → empty citation string, entry still in bibliography (1 item).

## 3. Consistency with typst 0.15.1

### 3.1 Dependency **[verified]**
`https://raw.githubusercontent.com/typst/typst/v0.15.1/Cargo.toml` → `hayagriva = "0.10.1"`.

### 3.2 How typst resolves `style:` and `sources` (`crates/typst-library/src/model/bibliography.rs`, **[verified by reading the raw file]**)
- `CslSource::from_value`: a string without a file extension → `ArchivedStyle::by_name(&string)`; failure →
  `unknown style: {string}` (measured as `error: unknown style: not-a-style`). Some old names are in a
  `replacement` map and produce a deprecation warning. A path (string with extension, or `path`) or
  `bytes` → `IndependentStyle::from_xml`. The `style` argument is documented as `str | path | bytes`.
- `decode_library`: by extension `"yml" | "yaml" => hayagriva::io::from_yaml_str(data)`,
  `"bib" => hayagriva::io::from_biblatex_str(data)`; other → `unknown bibliography format (must be .yaml/.yml or .bib)`.
  Raw bytes without a path: try YAML first, then BibLaTeX.
- Duplicate keys across sources: `duplicate bibliography keys: {keys}`. Cite key not in any
  bibliography: `key `{}` does not exist in the bibliography` / `citation key `{key}` is not present in {} bibliography`
  (in practice an `@nokey` in markup fails earlier in `ref` with `label `<nokey>` does not exist in the document`, measured).
- `CitationForm` → `CitePurpose`: `Normal => None`, `Prose => Prose`, `Full => Full`, `Author => Author`, `Year => Year`,
  `form: none` → `hidden = true`. (`#cite(<k>, form: "none")` is a **syntax error**: must be the value `none`;
  measured error: `expected "normal", "prose", "full", "author", "year", or none`.)
- Note styles: `if group.style.get().settings.class == citationberg::StyleClass::Note` and every cite in the
  group has form `None`/`Normal` → `realized = FootnoteElem::with_content(realized).pack()`.
- Adjacent cites separated only by spaces (`@a @b`, or `#cite(<a>) #cite(<b>)`) form one `CiteGroup` →
  one `CitationRequest` with two items (measured `[1], [3]` / `(Doe & Smith, 2020; Lee et al., 2019)`).

### 3.3 Style names and CSL files in both
- Names: identical by construction (`ArchivedStyle::by_name`). The typst docs list "about 100 options" because aliases are listed separately; `ArchivedStyle::all().len() == 80`.
- CSL file: `style: "ieee-file.csl"` in typst and `IndependentStyle::from_xml(&fs::read_to_string("ieee-file.csl")?)` in hayagriva
  gave identical output for the official `ieee.csl` (`https://raw.githubusercontent.com/citation-style-language/styles/master/ieee.csl`):
  `[1]`, `[2, p. 12]`, `J. Doe and J. Smith [1]` and the same two bibliography entries **[verified]**.
  Locales: typst uses `hayagriva::archive::locales()` too **[unverified — inferred; typst's bibliography.rs was only read in part]**.

### 3.4 Measured equality: typst compile vs probe
`cite-ieee.typ` (same `refs.bib`), compiled with `typst compile --features html --format html` (text extracted):

```
C1 plain: [1]
C2 page locator: [2, p. 12]                        (@knuth1984[p. 12])
C3 two keys adjacent: [1], [3]                     (@doe2020 @lee2019)
C3b two keys cite fn: [1], [3]                     (#cite(<doe2020>) #cite(<lee2019>))
C4 prose: J. Doe and J. Smith [1]                  (#cite(<doe2020>, form: "prose"))
C5 year: 2020
C6 author: A. Lee, B. Kim, and C. Chen
C7 repeat: [1]
C8 ref-fn with supplement: [2, p. 12]              (#ref(<knuth1984>, supplement: [p. 12]))
C9 in footnote1  … 1Note text [3].
[1] J. Doe and J. Smith, “A Study of Things,” Journal of Stuff, vol. 12, no. 3, pp. 45–67, 2020, doi: 10.1000/xyz123.
[2] D. E. Knuth, The TeXbook. Reading, MA: Addison-Wesley, 1984.
[3] A. Lee, B. Kim, and C. Chen, “Fast Widgets,” in Proceedings of the Widget Conference, ACM,  2019, pp. 1–10. [Online].  Available: https://example.org/widgets
```
APA: `(Doe & Smith, 2020)`, `(Knuth, 1984, p. 12)`, `(Doe & Smith, 2020; Lee et al., 2019)`, `Doe & Smith (2020)`, `2020`, `Lee et al.`, bibliography identical to the probe.
chicago-notes: every normal cite became a footnote marker (`C1 plain:1 … C7 repeat:5`), footnote texts
`1 Jane Doe and John Smith, “A Study of Things,” … 2 Donald E. Knuth, The TeXbook (Addison-Wesley, 1984), p. 12. … 5 Doe and Smith, “A Study of Things”. 6 Knuth, The TeXbook, p. 12.`;
a cite inside a user footnote produced a **nested** footnote (`7Note text8.` / `8Lee et al., “Fast Widgets”.`);
prose form printed the same odd `Doe and Smith (“A Study of Things.”` as the probe. So: identical strings
whenever writer feeds hayagriva the same requests in the same order with `Locator::Custom` locators.

## 4. Cross-reference syntax: inputs and recommendation

### 4.1 typst **[verified]**
- Lexer (`crates/typst-syntax/src/lexer.rs` v0.15.1): label/ref characters are `is_id_continue(c) || matches!(c, ':' | '.')`
  with `is_id_continue(c) = is_xid_continue(c) || c == '_' || c == '-'`. `fn ref_marker` then un-eats trailing
  `.`/`:` ("Don't include the trailing characters likely to be part of text"). `<label>` errors: `label cannot be empty`, `unclosed label`.
  Measured: `see @fig:plot.` → `see Figure 1.`; `@fig:plot: done` → `Figure 1: done`.
- `ref` (`crates/typst-library/src/model/reference.rs`): `@label` / `@label[supplement]` / `#ref(<l>, supplement: .., form: "normal" | "page")`.
  Referenceable: headings, figures (any kind), equations, footnotes; anything else → error. Resolution order in `realize`:
  `BibliographyElem::has(engine, target)` first; if the label is *also* an element → error; otherwise `to_citation` (cite).
  Only `Supplement::Content` is forwarded to the citation (`@key[p. 12]`).
- Measured error texts (typst 0.15.1):
  - `error: label `<nope>` does not exist in the document` (also for an unknown bib key)
  - `error: label `<x>` occurs multiple times in the document`
  - `error: label `<doe2020>` occurs both in the document and a bibliography` + `hint: change either the heading's label or the bibliography key to resolve the ambiguity`
  - `error: cannot reference heading without numbering` + `hint: you can enable heading numbering with `#set heading(numbering: "1.")``
  - `error: cannot reference equation without numbering` + `hint: you can enable equation numbering with `#set math.equation(numbering: "1.")``
  - `error: cannot reference image directly, try putting it into a figure`
  - `error: cannot reference without page numbering` + `hint: you can enable page numbering with `#set page(numbering: "1")`` (`form: "page"`; HTML export ignores `page` set rules)
  - `error: unknown style: not-a-style`
- Measured reference text (`refs-ok.typ`, heading numbering `"1.1"`, equation numbering `"(1)"`):
  `@fig:plot`→`Figure 1`, `@tbl:t1`→`Table 1`, `@lst:code`→`Listing 1`, `@eq:e1`→`Equation 1` (**not** `(1)`: the ref uses
  the supplement + plain number, the display equation shows `(1)`), `@sec:intro`→`Section 1`, `@sec:method`→`Section 1.1`,
  `@fig:plot[Fig.]`→`Fig. 1`, `@sec:intro[Chapter]`→`Chapter 1`, `@fn1` and `#footnote(<fn1>)`→ the footnote marker `1`.
  English local names (`crates/typst-library/translations/en.txt`): `figure = Figure`, `table = Table`, `raw = Listing`,
  `heading = Section`, `equation = Equation`, `bibliography = Bibliography`, `footnote = Footnote`, `page = page`, `outline = Contents`.
- `typst query file.typ "figure"` returns JSON with `label`, `kind` (`image`/`table`/`raw`), `numbering` (`"1"` or `null`),
  `supplement` and `caption` — handy for a test oracle. typst's HTML export anchors are `id="loc-N"` (not the label text).

### 4.2 pandoc / pandoc-crossref **[verified from pandoc MANUAL + pandoc-crossref docs/index.md; behaviour measured with pandoc 3.12.1]**
- pandoc citation syntax: key = `@` + letters/digits/`_` with single internal punctuation `:.#$%&-+?<>~/`;
  trailing `.` not part of key; doubled punctuation ends the key; `@{any thing}` braces for odd keys;
  `[@key]` bracketed with `;`-separated items, optional prefix, locator (`p./pp./chap./fig.`…; a bare
  number is a page; `{…}` forces/suppresses), suffix; `[-@key]` suppress author; `@key says` in-text (author-in-text).
- pandoc-crossref: labels `{#fig:x}` on an image alone in a paragraph, `{#tbl:x}` at the end of a table caption,
  `$$ … $$ {#eq:x}`, heading ids `{#sec:x}`, code `{#lst:x}`; refs `@fig:x`, `[@fig:a; @fig:b]` (ranges
  collapse "1-3"), `[@Fig:x]` capitalised prefix, `[Custom @fig:x]` custom prefix, `[-@fig:x]` number only;
  prefixes configurable (`figPrefix` "fig."/"figs.", `tblPrefix`, `eqnPrefix`, `secPrefix`, `lstPrefix`, singular/plural arrays);
  `chapters: true` numbers `chapter.item` resetting at level-1 headings (`chaptersDepth`); must run **before** citeproc because it
  **reuses citation syntax**: pandoc itself parses `@fig:plot` as a citation (measured: `[WARNING] Citeproc: citation fig:plot not found`,
  rendered `(**fig:plot?**)`, LaTeX `\textcite{fig:plot}`), and pandoc-crossref claims the ones with its prefixes.
- Consequence: a prefix-only scheme is fragile (bib keys may legally start with `fig:`); typst's rule ("in the bibliography? then cite, else ref,
  error if both") is unambiguous. Adopt typst's rule, keep the prefixes as a readability convention.

### 4.3 comrak 0.56 autolink `@` rule **[verified: `src/parser/autolink.rs` + run]**
`process_email_autolinks` (only when `options.extension.autolink`) scans for `@`; `email_match` rewinds over
`isalnum(c) || EMAIL_OK_SET[c]` with `EMAIL_OK_SET = b".+-_"`, and `if rewind == 0 { return None; }` — so an `@`
preceded by a space, `[`, `(` or line start can never start a mailto link. Forward it requires alphanumerics with at least
one `.` followed by an alphanumeric (`np == 0 → None`) and `link_end >= 2`, so `see@doe2020` is not a link either; `[`…`]` nesting
is also skipped (`bracket_opening`) unless `relaxed_autolinks` is set. Measured in §2.8. `relaxed_autolinks` stays `false` (as in presenter_rs).
Nothing else in comrak reads `@`; `[@key, p. 12]` stays a literal `[`…`]` text (no link reference definition matches).
`comrak::Options::default()` + `extension.footnotes = true` yields the HTML shown in §2.8 (`<section class="footnotes" data-footnotes>`, `<sup class="footnote-ref">`, `<li id="fn-1">`, `class="footnote-backref"`).

### 4.4 Recommended Markdown syntax (mirrors typst)

| construct | Markdown dialect | typst equivalent |
|---|---|---|
| label a heading | `## Method {#sec:method}` (comrak `header_attributes`) | `== Method <sec:method>` |
| label a figure | `![Caption](plot.png){#fig:plot}` (comrak `link_attributes`; image alone in a paragraph = figure) | `#figure(image(..), caption: [..]) <fig:plot>` |
| label a table | caption paragraph `: Caption {#tbl:t1}` after the table (pandoc-crossref form) **or** `:::figure {#tbl:t1} caption="…"` wrapper directive | `#figure(table(..), caption: [..]) <tbl:t1>` |
| label a code listing | ```` ```python {#lst:code caption="…"} ```` (comrak `fenced_code_attributes`) | `#figure(raw(..), caption: [..]) <lst:code>` |
| label an equation | `$$ … $$ {#eq:e1}` (own post-parse: a `{#id}` paragraph/trailing text after display math; comrak has no attrs on math) | `$ … $ <eq:e1>` |
| label a footnote | `[^fn1]` footnote ids are the label (`fn:` prefix optional) | `#footnote[..] <fn1>` |
| reference | `@sec:method`, `@fig:plot`, `@fig:plot[Fig.]` | `@sec:method`, `@fig:plot[Fig.]` |
| cite | `@doe2020`, `@doe2020[p. 12]`, `[@doe2020; @lee2019]` (also typst-style `@doe2020 @lee2019` → one group) | `@doe2020`, `@doe2020[p. 12]`, `@doe2020 @lee2019` |
| cite modes | `@doe2020{prose}`… **not recommended**; instead `[@doe2020]` normal, `@doe2020` prose (pandoc convention), `[-@doe2020]` year-only, `[@doe2020]{.author}`? → keep v1 to: normal, prose, year, author, none via a small suffix `@doe2020[:prose]`? — see IR §8 for the decision | `#cite(<k>, form: "prose")` |

Parsing: in the inline-text pass, match `@` only when preceded by start/whitespace/`[`/`(`/`;` and followed by a typst label
character; take the maximal run of `[\p{XID_Continue}_\-:.]`, strip trailing `.`/`:`; an optional immediately following `[…]` is the supplement.
Classification: key ∈ bibliography && ∉ labels → `Cite`; ∈ labels && ∉ bibliography → `Ref`; both → error (typst's wording);
neither → error `label `<x>` does not exist in the document` (typst's wording) — in `serve` mode render as `[?x]` with the
diagnostic instead of failing the build (presenter_rs convention of warnings).

## 5. Numbering

### 5.1 typst defaults **[verified by `typst query` on `refs-ok.typ` and by compile]**
- `figure.numbering` default `"1"` (`#[default(Some(NumberingPattern::from_str("1").unwrap().into()))]` in `figure.rs`);
  counter `Counter::new(CounterKey::Selector(select_where!(FigureElem, kind => kind.clone())))` → **one counter per kind**:
  images, tables and raw blocks are numbered independently (`Figure 1`, `Table 1`, `Listing 1`, then `Figure 2`, `Table 2`).
  Kind auto-detected from the first figurable descendant (table → `table`, raw → `raw`, else `image`); `numbering: none` makes an
  unnumbered figure that does not consume a number and cannot be referenced.
- `heading.numbering` default `none`; counter `Counter::of(HeadingElem::ELEM)`; `#set heading(numbering: "1.1")` gives `1`, `1.1`.
- `math.equation.numbering` default `none`; `#set math.equation(numbering: "(1)")` shows `(1)`; refs print `Equation 1`.
- `footnote.numbering` default `"1"`, continuous through the document.
- Numbering patterns (`model/numbering`): counting symbols `1 a A i I α Α *` (and circled variants); the suffix is everything after the
  last symbol, the prefix everything else; with more numbers than symbols, the last symbol *with its prefix* repeats
  (`"1.1"` works for any depth); a function `(..nums) => content` is allowed.

### 5.2 Per-chapter numbering in typst **[verified: `per-chapter.typ` output `Figure 1.1`, `Figure 1.2`, `Table 1.1`, `Equation (1.1)`, `Figure 2.1`, `Equation (2.1)`, `Fig. 1.1`]**
```typst
#set heading(numbering: "1.1")
#set figure(numbering: (..n) => numbering("1.1", counter(heading).get().first(), ..n))
#set math.equation(numbering: (..n) => numbering("(1.1)", counter(heading).get().first(), ..n))
#show heading.where(level: 1): it => {
  counter(figure.where(kind: image)).update(0)
  counter(figure.where(kind: table)).update(0)
  counter(math.equation).update(0)
  it
}
```
(`raw` figures need `counter(figure.where(kind: raw)).update(0)` too.)

### 5.3 DOCX
- pandoc 3.12.1's writer (reference) **[verified by unzipping `doc2.docx`]**: figure = paragraph `w:pStyle CaptionedFigure` with the drawing,
  then caption paragraph `w:pStyle ImageCaption` whose text is the *literal* caption (pandoc writes no number); table caption paragraph
  `w:pStyle TableCaption` before `<w:tbl>` plus `<w:tblCaption w:val="…"/>` in `tblPr`; a `w:bookmarkStart w:name="_<hash>"` /
  `w:bookmarkEnd` pair wraps each figure/table/heading; internal links are `<w:hyperlink w:anchor="_<hash>">`. Styles present in
  pandoc's reference.docx: `Caption`, `TableCaption`, `ImageCaption`, `Figure`, `CaptionedFigure`, `FootnoteText`, `FootnoteReference`, `Bibliography`.
- Word's own auto-numbered captions are **SEQ fields**: `{ SEQ Identifier [Bookmark] [Switches] }` (identifier e.g. `Figure`/`Table`;
  `\n` next number (default), `\c` repeat, `\r n` reset, `\s` reset at heading level, `\h` hide, `\* ARABIC` format) — Microsoft
  "Field codes: Seq (Sequence) field" **[verified]**. Serialization is the OOXML simple field (ISO/IEC 29500-1 §17.16.19, example from the
  Open XML SDK docs **[verified]**):
  ```xml
  <w:fldSimple w:instr="FILENAME"><w:r><w:t>Example Document.docx</w:t></w:r></w:fldSimple>
  ```
  so a caption paragraph is `<w:p><w:pPr><w:pStyle w:val="Caption"/></w:pPr><w:r><w:t xml:space="preserve">Figure </w:t></w:r>
  <w:fldSimple w:instr=" SEQ Figure \* ARABIC "><w:r><w:t>1</w:t></w:r></w:fldSimple><w:r><w:t>: A plot</w:t></w:r></w:p>`
  with the cached result `1` computed by writer (**[unverified]** that Word's own captions use exactly this instr string; the pieces are
  verified; Word recomputes on F9/`w:dirty="true"`/`updateFields`). Chapter-style numbers combine `STYLEREF 1 \s` + `SEQ Figure \* ARABIC \s 1` **[unverified]**.
- Cross-references in Word are `REF` fields: `{ REF Bookmark [\h hyperlink] [\n paragraph number] [\p above/below] [\r] [\w] }`
  (Microsoft "Field codes: Ref field" **[verified]**); the simplest robust form for writer: a bookmark around the caption's number run
  (`<w:bookmarkStart w:id="N" w:name="fig_plot"/>…<w:bookmarkEnd w:id="N"/>`) and `<w:hyperlink w:anchor="fig_plot"><w:r><w:t>Figure 1</w:t></w:r></w:hyperlink>`
  (pandoc's approach, verified) — numbers are literal, computed by writer; use `<w:fldSimple w:instr=" REF fig_plot \h ">` only if live updating matters.
  Footnote references: `<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="10"/></w:r>` (§6.2).
- Bibliography paragraphs: pandoc uses `w:pStyle Bibliography` with one paragraph per entry and `<w:hyperlink r:id="rIdN">` for URLs (needs a
  relationship of type hyperlink with `TargetMode="External"`); hanging indent comes from the style.

### 5.4 LaTeX
Emit `\label{fig:plot}` inside `\begin{figure}…\caption{…}\label{fig:plot}\end{figure}` (pandoc's layout, verified), `\section{Intro}\label{sec:intro}`,
`\begin{equation}…\label{eq:e1}\end{equation}`, tables `\begin{table}\caption{…}\label{tbl:t1}…`, listings `\begin{lstlisting}[caption=…,label=lst:code]` or
`\captionof{lstlisting}` (template's choice); references `\ref{fig:plot}` (number only) or `\cref{fig:plot}` (cleveref gives "Figure 1"; writer should emit
`Figure~\ref{fig:plot}` when the supplement is explicit/known to stay template-agnostic, and offer `latex.cleveref = true` → `\cref`/`\Cref` **[design]**).
LaTeX numbers everything itself, including per-chapter numbering in `book`/`report` classes.

### 5.5 Consequence for writer
`Numbering` must be a frontend pass over the IR that assigns `(kind, counter_value, display string)` to every labelled element in
document order with the policy: per-kind figure counters (image/table/listing) and a per-document equation counter and heading counters
with the chosen pattern (`"1"` default; `"1.1"` per chapter resets figure/equation counters at level-1 headings). HTML and DOCX print these
numbers; the typst template receives the *same policy* as `set` rules (§5.2) so PDF numbers agree; LaTeX ignores them. A `check` command
can diff writer's numbers against `typst query` output (`numbering`, `kind`, `label`) when a typst binary is available.

## 6. Footnotes

### 6.1 typst **[verified]**
`#footnote[body]` (numbering `"1"`, continuous), `#footnote(<fn1>)` or `@fn1` re-references an existing labelled footnote and prints its
marker; `footnote.entry` set rules style the list (`separator`, `clearance`, `gap`, `indent`); a footnote nested in a footnote works (measured `7`/`8`).
`set`/`show` rules in the footnote's call scope may not apply to its content.

### 6.2 DOCX **[verified from pandoc output]**
`word/footnotes.xml` (part type `…/footnotes`) holds `<w:footnote w:type="separator" w:id="-1">` and
`<w:footnote w:type="continuationSeparator" w:id="0">` plus one `<w:footnote w:id="10"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr>
<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> </w:t></w:r><w:r><w:t>Footnote text.</w:t></w:r></w:p></w:footnote>`
per note; the body run is `<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="10"/></w:r>`. Numbers are
assigned by Word in order of `footnoteReference` — writer need not compute them for DOCX (it still must for HTML).
The part needs `[Content_Types].xml` override `application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml` and a relationship
from `document.xml.rels` **[unverified detail, standard OOXML]**.

### 6.3 HTML (comrak) **[verified]** — see §2.8 markup. Each note is `<li id="fn-N">`, reference `<sup class="footnote-ref"><a href="#fn-N" id="fnref-N" data-footnote-ref>N</a></sup>`.
Note: comrak numbers footnotes by first *reference* order and renames them (`[^1]` → `fn-1`); writer-generated note-style citations must be
injected as extra footnotes **after** comrak's, or (cleaner) writer renders footnotes itself from the AST (`NodeValue::FootnoteReference`/`FootnoteDefinition`) so cite-footnotes and user footnotes share one numbering.

### 6.4 LaTeX: `\footnote{…}` (pandoc output verified: `A footnote here.\footnote{Footnote text with a cite \textcite{lee2019}.}`).

### 6.5 Interaction with note-class citation styles
Tell by `style.settings.class == hayagriva::citationberg::StyleClass::Note` (5 of the 80 bundled styles, §2.2). For such styles writer wraps each
`Cite` of mode `normal` in a generated footnote (typst does exactly this), passes the footnote's number via `CitationRequest::new(.., Some(n))`
so hayagriva can shorten subsequent notes / use "ibid."-type logic, and places the rendered citation as the footnote body. A `Cite` that is
already inside a user footnote still gets its own nested footnote in typst (measured); for HTML/DOCX writer may instead inline it into the
containing footnote **[design choice; deviates from typst]**. With biblatex, `\autocite` already does the right thing for note styles
(`\DeclareAutoCiteCommand{footnote}[f]{\smartcite}{\smartcites}`), so LaTeX output is unchanged.

## 7. LaTeX citation commands

Verified in TeX Live 20260301 (`biblatex.def` / `biblatex.sty` v3.21, `natbib.sty`):
- biblatex: `\DeclareCiteCommand{\fullcite}`, `{\citeauthor}`, `{\citeyear}`, `\DeclareMultiCiteCommand{\cites}{\cite}{\multicitedelim}`,
  `{\parencites}[\mkbibparens]{\parencite}`, `{\footcites}[\mkbibfootnote]{\footcite}`, `{\textcites}{\textcite}`;
  `\DeclareAutoCiteCommand{plain}{\cite}{\cites}`, `{inline}{\parencite}{\parencites}`, `{footnote}[f]{\smartcite}{\smartcites}`,
  `{superscript}[l]{\supercite}{\supercites}` (so `\autocite`/`\autocites` dispatch on the style's `autocite=` option);
  `\newrobustcmd*{\printbibliography}`, `\newrobustcmd*{\addbibresource}`. `\cite`/`\parencite`/`\textcite`/`\autocite` are defined in the
  per-style `.cbx` files (standard.cbx) **[unverified location; usage verified through pandoc's `--biblatex` output below]**.
- natbib: `\DeclareRobustCommand\citet` (l.685), `\citep` (691), `\citealp` (705), `\citeyearpar` (737) in `natbib.sty`.
- pandoc 3.12.1 output for `See @doe2020 and [@knuth1984, p. 12; @lee2019]. Also [-@doe2020]` **[verified]**:
  - `--biblatex`: `See \textcite{doe2020} and \autocites[12]{knuth1984}{lee2019}. Also \autocite*{doe2020}`
  - `--natbib`: `See \citet{doe2020} and \citetext{\citealp[p.~12]{knuth1984}; \citealp{lee2019}}. Also \citeyearpar{doe2020}`
  - `--citeproc` (plain LaTeX, pre-rendered): `Doe and Smith (2020) and (Knuth 1984, 12; Lee et al. 2019)` + `\begin{CSLReferences}{1}{1}\bibitem[\citeproctext]{ref-doe2020} …`
  - the manual: `--natbib`/`--biblatex` are "not for use with the --citeproc option or with PDF output", "intended for use in producing a LaTeX file that can be processed with bibtex or biber".

Recommendation (config `[latex] cite = "biblatex"` default; `"natbib"`, `"plain"`, `"rendered"`):

| IR | biblatex (default) | natbib | plain |
|---|---|---|---|
| `Cite{keys:[k], mode: Normal}` | `\autocite{k}` | `\citep{k}` | `\cite{k}` |
| `+ locator "p. 12"` | `\autocite[p.~12]{k}` | `\citep[p.~12]{k}` | `\cite[p.~12]{k}` |
| `+ prefix "see"` | `\autocite[see][p.~12]{k}` | `\citep[see][p.~12]{k}` | `see \cite[p.~12]{k}` |
| `keys:[a,b]`, no locators | `\autocite{a,b}` | `\citep{a,b}` | `\cite{a,b}` |
| `keys:[a,b]`, with per-key locators | `\autocites[p.~12]{a}{b}` | `\citetext{\citealp[p.~12]{a}; \citealp{b}}` | `\cite[p.~12]{a}; \cite{b}` |
| `mode: Prose` | `\textcite{k}` (`\textcites{a}{b}`) | `\citet{k}` | `\citeauthor{k}~\cite{k}` (no natbib → approximate) |
| `mode: Year` | `\autocite*{k}` (author suppressed; numeric styles print the number) | `\citeyearpar{k}` | unsupported → `\cite{k}` + warning |
| `mode: Author` | `\citeauthor{k}` | `\citeauthor{k}` | unsupported → warning |
| `mode: Hidden` | `\nocite{k}` | `\nocite{k}` | `\nocite{k}` |
| bibliography block | `\printbibliography` (preamble supplied by the user's template: `\usepackage[style=…]{biblatex}` + `\addbibresource{refs.bib}`) | `\bibliographystyle{plainnat}` + `\bibliography{refs}` | `\bibliography{refs}` |

`"rendered"` = pre-render with hayagriva (pandoc's `--citeproc` approach) for templates without a bib package — useful but lower priority.
writer copies `.bib` files next to the `.tex` output (`\addbibresource` is the template's job in BYO-template mode; writer exposes the file
list as a template variable, e.g. `{{ bib_files }}`, and `{{ bib_style }}` for the CSL name so the template can map it).

## 8. Recommended IR shapes

```rust
/// A label attached to a block (heading, figure/table/listing, equation, footnote) or an inline anchor.
pub struct Label { pub id: String /* "fig:plot" */, pub kind: LabelKind }
pub enum LabelKind { Heading { level: u8 }, Figure, Table, Listing, Equation, Footnote, Other }

/// Inline::Cite — one citation group (typst CiteGroup / pandoc [@a; @b]).
pub struct Cite {
    pub items: Vec<CiteItem>,
    pub mode: CiteMode,               // whole-group mode (typst: per cite; a group with a non-Normal form is not merged — mirror that)
    pub span: Option<SourceSpan>,
}
pub struct CiteItem {
    pub key: String,                  // must exist in the Library (checked at parse → diagnostic `label <k> does not exist…`)
    pub locator: Option<String>,      // free text, e.g. "p. 12", "chap. 3" — passed as Locator::Custom (§2.6); LaTeX: [p.~12]
    pub prefix: Option<String>,       // pandoc "see" — rendered outside hayagriva's output (HTML/DOCX: prefix + " " + citation; LaTeX: first optional arg)
    pub suffix: Option<String>,       // pandoc suffix — appended after the citation
}
pub enum CiteMode { Normal, Prose, YearOnly, AuthorOnly, Full, Hidden }   // ↔ typst form normal/prose/year/author/full/none ↔ CitePurpose None/Prose/Year/Author/Full/hidden
```
Markdown surface (decided): `@key` → Normal (not pandoc's in-text!) **to match typst**; `[@a; @b]` → group; `@key[p. 12]` → locator;
`@key[see][p. 12]`? — no: keep typst's single bracket; prefix/suffix only via pandoc's bracketed form `[see @key, p. 12, suffix]`.
Modes: `@key{prose}`-style postfix is rejected; use directive-free inline forms `@key:prose`? No — `:` is a label char. Decision:
**mode via a trailing attribute block** `@doe2020{.prose}`, `{.year}`, `{.author}`, `{.full}`, `{.none}` (same attribute grammar as
the rest of the dialect; `[-@key]` is accepted as an alias of `{.year}` for pandoc users). This keeps `@key` identical in meaning in
`.md` and `.typ` files.

```rust
/// Inline::Ref — a cross-reference to a labelled element.
pub struct Ref {
    pub target: String,               // label id, e.g. "fig:plot"
    pub supplement: Option<String>,   // `@fig:plot[Fig.]`; None → kind's default word ("Figure"/"Table"/"Listing"/"Equation"/"Section", localized)
    pub form: RefForm,                // Normal | Page (page only meaningful for PDF/LaTeX; HTML/DOCX fall back to Normal + warning)
    pub span: Option<SourceSpan>,
}
pub enum RefForm { Normal, Page }
/// Resolved during the numbering pass (frontend), stored next to the Ref so every backend prints the same text:
pub struct Resolved { pub kind: LabelKind, pub number: String /* "1.2" */, pub supplement: String /* "Figure" */ }
```

Rendering table:

| backend | `Ref` | `Cite` (in-text style) | `Cite` (note style) | bibliography |
|---|---|---|---|---|
| HTML | `<a href="#fig:plot" class="ref">Figure 1</a>` (text from `Resolved`) | `<span class="citation" data-cites="k1 k2">` + runs from `flatten()` (`<em>`,`<strong>`,`<sup>`,`<sub>`,`<span class="smallcaps">`,`<a href>`) | `<sup class="footnote-ref"><a href="#fn-N">N</a></sup>` + an `<li id="fn-N">` holding the runs | `<section id="refs" class="references csl-bib-body [hanging-indent]">` with `<div id="ref-KEY" class="csl-entry">`; IEEE-style first field in `<span class="csl-left-margin">` |
| DOCX | `<w:hyperlink w:anchor="fig_plot">` + run `Figure 1` (bookmark on the caption) | runs from `flatten()` → `<w:r>` with `<w:i/>`,`<w:b/>`,`<w:smallCaps/>`,`<w:u/>`,`<w:vertAlign w:val="superscript|subscript"/>`, links `<w:hyperlink r:id>` | `<w:footnoteReference w:id>` + `<w:footnote>` in footnotes.xml | `Bibliography` paragraphs; hanging indent via `<w:ind w:left="720" w:hanging="720"/>` when `hanging_indent` |
| typst (PDF) | `@fig:plot` / `@fig:plot[Fig.]` / `#ref(<fig:plot>, form: "page")` | `@k`, `@k[p. 12]`, `#cite(<k>, form: "prose")`, group → `@a @b` (or `#cite(<a>)#cite(<b>)` with a space); prefix/suffix emitted as plain text around it | nothing special (typst does it) | `#bibliography(("refs.bib",), style: "<name or path.csl>", title: …)` with the same `style` string |
| LaTeX | `\ref{fig:plot}` with `Figure~` prefix from `Resolved.supplement` (or `\cref` when `latex.cleveref`) | §7 table | §7 table (`\autocite` handles note styles) | `\printbibliography` / `\bibliography{…}` |

Diagnostics (all at parse/resolve time, before any backend): unknown target, target both label and key, duplicate label, reference to an
unnumbered element (headings without numbering: writer should *not* error like typst — it can always compute a number — but the typst
backend must then enable heading numbering or emit `#ref` with a custom supplement; simplest: the typst template always sets
`heading(numbering: …)` when any `sec:` ref exists), and `RefForm::Page` in HTML/DOCX.

## 9. Verified API hints (copy-paste)

```rust
// Cargo.toml
hayagriva = "0.10.1"            // default features: biblatex + archive

// load
let lib: hayagriva::Library = hayagriva::io::from_biblatex_str(&bib_text)
    .map_err(|errs| errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))?;   // Vec<BibLaTeXError>, Display impl
let lib_yaml: hayagriva::Library = hayagriva::io::from_yaml_str(&yaml_text)?;             // serde_yaml::Error
// merge several files: for e in other.iter() { lib.push(e); }   (later file wins on key clash; emit a warning first)

// style
use hayagriva::citationberg::{IndependentStyle, Style, StyleClass};
let style: IndependentStyle = match hayagriva::archive::ArchivedStyle::by_name("ieee") {
    Some(s) => match s.get() { Style::Independent(s) => s, Style::Dependent(_) => unreachable!("archived styles are independent") },
    None => IndependentStyle::from_xml(&std::fs::read_to_string(path)?)?,   // citationberg::XmlDeError
};
let is_note_style = style.settings.class == StyleClass::Note;
let locales: Vec<hayagriva::citationberg::Locale> = hayagriva::archive::locales();   // 63

// cite
use hayagriva::citationberg::taxonomy::Locator;
use hayagriva::{BibliographyDriver, BibliographyRequest, CitationItem, CitationRequest, CitePurpose, Entry, LocatorPayload, SpecificLocator};
let mut driver: BibliographyDriver<'_, Entry> = BibliographyDriver::new();
let item = CitationItem::with_locator(lib.get("knuth1984").unwrap(), Some(SpecificLocator(Locator::Custom, LocatorPayload::Str("p. 12"))));
let item2 = CitationItem::with_entry(lib.get("doe2020").unwrap()).kind(CitePurpose::Prose);
let hidden = CitationItem::new(lib.get("lee2019").unwrap(), None, None, true, None);
driver.citation(CitationRequest::from_items(vec![item], &style, &locales));                 // in-text styles
driver.citation(CitationRequest::new(vec![item2], &style, None, &locales, Some(footnote_no))); // note styles: pass the footnote number
let rendered: hayagriva::Rendered = driver.finish(BibliographyRequest::new(&style, None, &locales));
for (i, c) in rendered.citations.iter().enumerate() { /* c.citation: ElemChildren; c.note_number */ }
if let Some(b) = &rendered.bibliography { for it in &b.items { /* it.key, it.first_field: Option<ElemChild>, it.content: ElemChildren */ } }

// quick text
let plain = format!("{:#}", c.citation);                      // Display alternate == BufWriteFormat::Plain
let mut html = String::new(); c.citation.write_buf(&mut html, hayagriva::BufWriteFormat::Html)?;  // NOT html-escaped
```

typst template fragments (all compiled with typst 0.15.1):
```typst
#set heading(numbering: "1.1")                 // only when the document has heading refs or numbering requested
#set math.equation(numbering: "(1)")           // only when equations are referenced / numbering requested
#figure(image("plot.png", width: 1cm), caption: [A plot]) <fig:plot>
#figure(table(columns: 2, [a], [b], [1], [2]), caption: [A table]) <tbl:t1>
#figure(```rust fn main() {}```, caption: [Code]) <lst:code>
$ E = m c^2 $ <eq:e1>
See @fig:plot, @fig:plot[Fig.], @tbl:t1, @lst:code, @eq:e1, @sec:intro[Chapter].
Cite @doe2020, @knuth1984[p. 12], @doe2020 @lee2019, #cite(<doe2020>, form: "prose"), #cite(<lee2019>, form: none).
#bibliography("refs.bib", style: "ieee")       // or style: "my.csl"; sources may be an array of .bib/.yml paths
```

## 10. Open questions

1. **Cite mode surface syntax in Markdown** — the `@key{.prose}` attribute-block proposal (§8) is untested against comrak: comrak's
   `inline_code_attributes`/`link_attributes` only attach `{…}` to code/links, so `@key{.prose}` arrives as text `@key{.prose}` and writer's
   own inline scanner must parse it (same tokenizer as `parse_attr_block` in presenter_rs). Alternative: pandoc semantics (`@key` = prose,
   `[@key]` = normal). The plan must pick one; typst-mirroring (`@key` = normal) is recommended but surprises pandoc users.
2. **Equation labels in Markdown**: comrak attaches no attributes to `$$…$$`; a `{#eq:x}` immediately after the display math (same
   paragraph) must be recognised by writer. Untested whether comrak keeps `$$x$$ {#eq:x}` in one paragraph as `Math` + `Text` nodes (expected yes).
3. **Locale**: both backends default to `en-US` (`LocaleCode::en_us()` fallback in hayagriva; typst uses the document `lang`). writer should
   pass `Some(LocaleCode(lang))` from front matter to `CitationRequest::new`/`BibliographyRequest::new` — the exact mapping typst does from
   `text.lang`/`region` to `LocaleCode` was not read.
4. **hayagriva prose form in note styles** prints `Doe and Smith (“A Study of Things.”` (unbalanced) — identical in typst, so "consistent"
   but probably a hayagriva bug; consider rejecting `prose` for note-class styles with a warning.
5. **Nested footnote for a cite inside a footnote** (typst behaviour) vs inlining it (nicer for HTML/DOCX) — decide; either way the
   `note_number` passed to hayagriva must be the number of the footnote that contains the citation text.
6. **DOCX SEQ caption fields**: the exact `w:instr` string Word writes for Insert Caption (` SEQ Figure \* ARABIC `) was not observed on
   this machine (no headless Word); literal numbers + bookmarks (pandoc's approach) are verified and sufficient for v1.
7. **typst locale set for CSL terms** (`locales()` vs document `lang`) and whether typst passes `locale_files` identical to
   `hayagriva::archive::locales()` — not read; only the English output was compared.
8. **comrak 0.57.0** exists (presenter pins 0.56); autolink code may have moved but the `rewind == 0 → None` rule is CommonMark/GFM
   semantics and unlikely to change. Re-check if writer bumps comrak.
