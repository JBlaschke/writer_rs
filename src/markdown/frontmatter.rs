//! YAML front matter → [`DocMeta`] (markdown-dialect.md §2.2, config.md §3). Implemented by T10.
//!
//! There are two ways to get at the YAML text, and one parser for it:
//! - Markdown files: comrak extracts the block as `NodeValue::FrontMatter(literal)`;
//!   [`strip_delimiters`] turns that literal into the YAML between the `---` lines.
//! - Sources comrak does not see (the first markdown cell of a notebook):
//!   [`split_front_matter`] finds the block in plain text, by the same rules as comrak.
//! - [`parse_front_matter`] turns the YAML into a [`DocMeta`]; [`meta_from_json`] applies
//!   the same key rules to a JSON object (a notebook's `metadata.writer`,
//!   notebook-mapping.md §2.2).
//!
//! Configuration keys and table maps stay in [`DocMeta::vars`], as specified by
//! markdown-dialect.md §2.2 and config.md §3. Merge into a clone of the config:
//!
//! ```
//! # use writer::{config::Config, markdown::frontmatter::parse_front_matter};
//! # let config = Config::default();
//! let meta = parse_front_matter("numbering: {figures: section}")?;
//! let mut document_config = config.clone();
//! document_config.merge_front_matter(&meta)?;
//! assert!(matches!(document_config.numbering.figures,
//!     writer::config::FigureNumbering::Section));
//! # Ok::<(), writer::error::Error>(())
//! ```
//!
//! # YAML flavour (`serde_norway` 0.9)
//! `serde_norway` resolves plain scalars with the **YAML 1.2 core schema**, not YAML 1.1:
//! - booleans are only `true`/`false` (also `True`, `TRUE`, `False`, `FALSE`); the YAML 1.1
//!   spellings `yes`, `no`, `on`, `off`, `y`, `n` are **strings** (unlike PyYAML), so
//!   `smart_punctuation: yes` under `markdown:` is rejected as "expected a boolean";
//! - null is `null`, `Null`, `NULL`, `~` or an empty value;
//! - integers are decimal, `0x…`, `0o…` or `0b…`; a leading zero (`010`) makes a string;
//!   an integer outside the 64-bit range has no JSON number form and becomes the string of
//!   its decimal digits in `vars` (`n: 99999999999999999999` →
//!   `"99999999999999999999"`), up to 128 bits; serde_norway reads even longer digit
//!   strings as floats (`1e42`);
//! - floats include `.inf`/`.nan`, which have no JSON form and become `null` in `vars`;
//! - there is no timestamp type: `date: 2026-10-08` is the string `"2026-10-08"`.
//!
//! The string fields of [`DocMeta`] take any scalar as its text exactly as written
//! (`title: 42` → `"42"`, `subtitle: 1.50` → `"1.50"`, `author: yes` → `"yes"`).
//! Keys must be unique in every mapping (a repeated key is an error, as YAML requires).
//! Custom tags (`!expr x`) are rejected, except on the string fields, where the scalar's
//! text is used and the tag ignored.

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;
use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, IgnoredAny, MapAccess,
    SeqAccess, Visitor,
};
use serde_json::{Map, Value};

use crate::error::{Diagnostic, Diagnostics, Error, Result};
use crate::ir::{Author, DocMeta, SourceSpan};

/// The front matter delimiter line (dialect §1: `front_matter_delimiter = "---"`).
const DELIMITER: &str = "---";

/// Byte-order mark that may precede the opening delimiter (comrak skips it too).
const BOM: char = '\u{feff}';

/// The front matter error for a root that is not a mapping (dialect §2.2).
const NOT_A_MAPPING: &str = "expected a mapping";

/// The keys that map to the [`DocMeta`] string fields.
const STRING_FIELDS: [&str; 5] =
    ["title", "subtitle", "date", "description", "lang"];

/// Config tables and examples used in shape errors.
const TABLES: [(&str, &str); 10] = [
    ("markdown", "markdown: {smart_punctuation: true}"),
    ("math", "math: {html: mathml}"),
    ("numbering", "numbering: {headings: \"1.1\", figures: section}"),
    ("references", "references: {style: apa}"),
    ("html", "html: {theme: ./theme}"),
    ("pdf", "pdf: {paper: us-letter}"),
    ("docx", "docx: {math: image}"),
    ("latex", "latex: {top_level: chapter}"),
    ("typst", "typst: {strict: true}"),
    ("notebook", "notebook: {show_prompts: false}"),
];

/// `std`'s two-parameter `Result`, for serde trait methods (the crate's `Result` has one).
type DeResult<T, E> = std::result::Result<T, E>;

/// Text-based extraction for sources comrak does not see (notebook cells).
/// Returns (yaml_without_delimiters, rest_of_text, number_of_lines_consumed) when `text`
/// starts with a line `---` and a closing `---` line exists.
///
/// Both delimiter lines must be exactly `---` (no trailing spaces), terminated by `\n` or
/// `\r\n`; the closing one may also end the text. As in comrak, the line right after the
/// opening delimiter never closes the block, so `---\n---\n` is not front matter (an empty
/// block needs a blank line: `---\n\n---\n`). The YAML excludes the line break before the
/// closing delimiter, and `rest_of_text` starts right after the closing line's break (a
/// following blank line stays in it). `number_of_lines_consumed` counts the opening line,
/// the YAML lines and the closing line, so `rest_of_text` begins on line `consumed + 1`. A
/// leading byte-order mark is skipped.
///
/// ```
/// use writer::markdown::frontmatter::split_front_matter;
/// assert_eq!(split_front_matter("---\na: 1\n---\nbody"), Some(("a: 1", "body", 3)));
/// assert_eq!(split_front_matter("# no front matter"), None);
/// ```
pub fn split_front_matter(text: &str) -> Option<(&str, &str, usize)> {
    let text = text.strip_prefix(BOM).unwrap_or(text);
    let mut lines = lines_with_ends(text);
    let (first, yaml_start) = lines.next()?;
    // `yaml_start == first.len()` means the opening line has no line break.
    if first != DELIMITER || yaml_start == first.len() {
        return None;
    }
    let mut line_start = yaml_start;
    for (consumed, (line, end)) in (2..).zip(lines) {
        // comrak searches for `\n---` only after the opening line's break, so the closing
        // delimiter is line 3 at the earliest.
        if consumed > 2 && line == DELIMITER {
            let yaml = without_line_break(&text[yaml_start..line_start]);
            return Some((yaml, &text[end..], consumed));
        }
        line_start = end;
    }
    None
}

/// Strip the `---` delimiter lines from comrak's `NodeValue::FrontMatter` literal.
///
/// comrak's literal is the raw block: the opening `---` line, the YAML, the closing `---`
/// line and, when present, one following blank line (`\n` or `\r\n` line breaks, possibly
/// no break at all when the block ends the document). The result is the YAML without the
/// line break before the closing delimiter — the same text [`split_front_matter`] returns
/// for the same block. A delimiter line that is missing is simply not stripped.
pub fn strip_delimiters(literal: &str) -> &str {
    let mut body = literal.strip_prefix(BOM).unwrap_or(literal);
    if let Some(rest) =
        body.strip_prefix(DELIMITER).and_then(strip_leading_line_break)
    {
        body = rest;
    }
    // Drops the blank line comrak keeps after the block and the closing line's break.
    let body = body.trim_end_matches(['\r', '\n']);
    if body == DELIMITER {
        return "";
    }
    match body.strip_suffix(DELIMITER) {
        Some(rest) if rest.ends_with('\n') => without_line_break(rest),
        _ => body,
    }
}

/// Parse YAML into document metadata. Unknown and config-like keys stay in
/// [`DocMeta::vars`] in source order. Scalars for content fields keep their YAML
/// text; authors and keywords also accept lists. The abstract remains a string
/// in `vars["abstract"]` for the frontend to parse into blocks.
///
/// Empty YAML or `null` gives default metadata. A non-mapping root, repeated
/// keys, invalid content values and non-mapping config tables are errors.
/// `numbering` additionally accepts a boolean. Both `author` and `authors`
/// together are an error, including when one is null.
///
/// Every failure is an [`Error::FrontMatter`]. Source line numbers include the
/// opening `---` on line 1. Colons in plain values and tab indentation receive
/// hints after the error's position.
pub fn parse_front_matter(yaml: &str) -> Result<DocMeta> {
    Ok(parse_yaml(yaml)?.meta)
}

/// Parse metadata and retain the author syntax for document warnings and the
/// scalar-text replay. `deserialize_any` validates shapes with positioned errors;
/// a narrow second pass asks for strings to retain numeric YAML spelling.
fn parse_yaml(yaml: &str) -> Result<ParsedRoot> {
    let source = format!("\n{yaml}");
    let parse = || -> DeResult<ParsedRoot, serde_norway::Error> {
        let mut root = serde_norway::Deserializer::from_str(&source)
            .deserialize_any(RootVisitor)?
            .ok_or_else(|| {
                <serde_norway::Error as de::Error>::custom(NOT_A_MAPPING)
            })?;
        root.check_author_keys()
            .map_err(<serde_norway::Error as de::Error>::custom)?;
        serde_norway::Deserializer::from_str(&source)
            .deserialize_any(TextReplay(&mut root))?;
        Ok(root)
    };
    let root = parse().map_err(|err| {
        let mut message = err.to_string();
        if let Some(hint) =
            err.location().and_then(|at| yaml_hint(yaml, &message, at.line()))
        {
            message.push_str(&format!(" ({hint})"));
        }
        Error::FrontMatter(message)
    })?;
    tracing::debug!(vars = root.meta.vars.len(), "parsed front matter");
    Ok(root)
}

/// A hint for the YAML error `message` located on source `line` (the YAML's first line is
/// line 2), for the two mistakes YAML's own messages do not explain: a line indented with a
/// tab (`indent with spaces, not tabs`), and a `mapping values are not allowed` error on a
/// `key: value` line (also a `- key: value` item) whose plain value contains `: ` or ends
/// with `:`, as talk titles do (`put the value in quotes: `title: "Rust: the good parts"``,
/// built from the line with `\` and `"` escaped).
fn yaml_hint(yaml: &str, message: &str, line: usize) -> Option<String> {
    let text = yaml.lines().nth(line.checked_sub(2)?)?;
    let entry = text.trim_start();
    if text[..text.len() - entry.len()].contains('\t') {
        return Some("indent with spaces, not tabs".to_string());
    }
    if !message.starts_with("mapping values are not allowed") {
        return None;
    }
    let entry = entry.strip_prefix("- ").unwrap_or(entry);
    let (key, value) = entry.split_once(": ")?;
    let value = value.trim();
    let plain =
        !value.starts_with(['"', '\'', '[', '{', '|', '>', '&', '*', '!', '#']);
    if key.is_empty()
        || !plain
        || !(value.contains(": ") || value.ends_with(':'))
    {
        return None;
    }
    let quoted = value.replace('\\', "\\\\").replace('"', "\\\"");
    Some(format!("put the value in quotes: `{key}: \"{quoted}\"`"))
}

/// [`parse_front_matter`] for the front matter of a Markdown document, whose closing `---` is on
/// line `closing`, with what it means for a document (A3): front matter that swallowed heading
/// content because a document started with a `---` rule or the closing `---` is missing.
/// Markdown headings are YAML comments, so such content vanishes without an error.
///
/// - YAML without any key (only comments and blank lines) with a comment that is a Markdown
///   heading (`# Title`; not a commented-out key such as `# title: Draft`, nor a label
///   ending with `:`): a [`FRONT_MATTER_EMPTY`] warning (lines 1 to `closing`), as for a
///   notebook's first cell. YAML with keys whose values are all commented out
///   (`html:` over `#   theme: x`) is fine;
/// - otherwise a Markdown heading after a blank line, with only blank lines and
///   comments after it, including a blank line before the closing `---`, as
///   [`swallowed_heading`] finds it: a [`FRONT_MATTER_UNCLOSED`] warning on its line;
/// - a root that is not a mapping: `expected a mapping at line L column C (…)` with the
///   position of the root value and the front matter's extent, asking whether line 1 was
///   meant as a thematic break or the closing `---` is missing; any other error gets that
///   extent and question too when the YAML has a heading after a blank line.
///
/// Errors: as [`parse_front_matter`].
pub fn parse_document_front_matter(
    yaml: &str, closing: usize, diagnostics: &mut Diagnostics,
) -> Result<DocMeta> {
    let extent = format!(
        "the front matter runs from line 1 to the `---` on line {closing}"
    );
    let root = parse_yaml(yaml).map_err(|err| match err {
        Error::FrontMatter(message) if message == NOT_A_MAPPING => {
            let (line, column) = root_position(yaml);
            Error::FrontMatter(format!(
                "{NOT_A_MAPPING} at line {line} column {column} ({extent}; is its closing `---` \
                 missing, or was line 1 meant as a thematic break?)"
            ))
        }
        Error::FrontMatter(message) if !headings_after_blank(yaml).is_empty() => {
            Error::FrontMatter(format!(
                "{message} ({extent}; is its closing `---` missing?)"
            ))
        }
        other => other,
    })?;
    let span = |start_line| SourceSpan {
        start_line,
        end_line: closing,
        cell: None,
        file: None,
    };
    let keyless = yaml.lines().all(|line| {
        let line = line.trim();
        line.is_empty() || line.starts_with('#')
    });
    if keyless {
        if yaml.lines().any(is_markdown_heading) {
            let message = format!(
                "{extent} and sets nothing: its `# …` lines are YAML comments and are not shown. \
                 Was line 1 meant as a thematic break? A document needs no opening thematic break"
            );
            diagnostics.push(
                Diagnostic::warning(FRONT_MATTER_EMPTY, message)
                    .with_span(span(1)),
            );
        }
    } else if let Some((line, text)) = swallowed_heading(yaml) {
        let message = format!(
            "`{text}` on line {line} is a YAML comment, not a heading: {extent}. Is its closing \
             `---` missing (before line {line})?"
        );
        diagnostics.push(
            Diagnostic::warning(FRONT_MATTER_UNCLOSED, message)
                .with_span(span(line)),
        );
    }
    if root.author_list {
        let line = yaml
            .lines()
            .position(|line| {
                let Some((key, _)) = line.split_once(':') else {
                    return false;
                };
                matches!(key.trim_end(), "author" | "'author'" | "\"author\"")
            })
            .map(|index| index + 2);
        let span = SourceSpan {
            start_line: line.unwrap_or(1),
            end_line: line.unwrap_or(closing),
            cell: None,
            file: None,
        };
        diagnostics.push(
            Diagnostic::warning(
                FRONT_MATTER_AUTHOR_LIST,
                "`author` contains a list; use `authors` for a list of authors",
            )
            .with_span(span),
        );
    }
    Ok(root.meta)
}

/// Diagnostic code for a list supplied under the singular `author` key.
pub const FRONT_MATTER_AUTHOR_LIST: &str = "front-matter-author-list";

/// Diagnostic code of front matter that sets nothing but has YAML comments (warning; the same
/// code as the notebook frontend's).
pub const FRONT_MATTER_EMPTY: &str = "front-matter-empty";

/// Diagnostic code of front matter that ends with a Markdown heading, so its closing `---` is
/// probably missing (warning, A3).
pub const FRONT_MATTER_UNCLOSED: &str = "front-matter-unclosed";

/// The source line and column (1-based; the YAML starts on line 2) of the first character
/// that is not whitespace or part of a comment: where a root value starts.
fn root_position(yaml: &str) -> (usize, usize) {
    (2..)
        .zip(yaml.lines())
        .find_map(|(number, line)| {
            let trimmed = line.trim_start();
            (!trimmed.is_empty() && !trimmed.starts_with('#')).then(|| {
                (number, line[..line.len() - trimmed.len()].chars().count() + 1)
            })
        })
        .unwrap_or((2, 1))
}

/// The YAML comment lines that look like Markdown headings ([`is_markdown_heading`]) and
/// follow a blank line, as `(source line, text)` (the YAML's first line is line 2).
fn headings_after_blank(yaml: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = yaml.lines().collect();
    (1..lines.len())
        .filter(|&i| {
            lines[i - 1].trim().is_empty() && is_markdown_heading(lines[i])
        })
        .map(|i| (i + 2, lines[i]))
        .collect()
}

/// A Markdown heading as a YAML comment: `#` to `######` at the start of the line, a space or
/// tab and text, not ending with `:` (a comment that labels what follows) and not a
/// commented-out key ([`is_commented_key`]).
fn is_markdown_heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    let text = &line[hashes..];
    if !(1..=6).contains(&hashes) || !text.starts_with([' ', '\t']) {
        return false;
    }
    let text = text.trim();
    !text.is_empty() && !text.ends_with(':') && !is_commented_key(line)
}

/// A comment holding a `key: value` line (`# date: 2026-01-31`, `#theme: dark`): the key is
/// letters, digits, `_` and `-`, and the colon ends the line or is followed by whitespace.
fn is_commented_key(line: &str) -> bool {
    let Some(comment) = line.trim_start().strip_prefix('#') else {
        return false;
    };
    let comment = comment.trim_start_matches('#').trim();
    comment.split_once(':').is_some_and(|(key, value)| {
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
            && (value.is_empty() || value.starts_with([' ', '\t']))
    })
}

/// A Markdown heading that front matter swallowed (`# Title`, after a blank line; not a
/// label ending with `:` or a commented-out key such as `# date: …`), after the last YAML line
/// (not blank, not a comment), with only blank lines and comments after it, none of them a
/// commented-out key and one of them blank (counting the blank line before the closing
/// `---`, which [`strip_delimiters`] leaves as a final line break); `(source line, text)`,
/// the YAML's first line being line 2. See [`parse_document_front_matter`].
pub fn swallowed_heading(yaml: &str) -> Option<(usize, &str)> {
    let lines: Vec<&str> = yaml.lines().collect();
    let is_yaml = |line: &&str| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    };
    let last_yaml = lines.iter().rposition(is_yaml).map_or(1, |i| i + 2);
    let (line, text) = headings_after_blank(yaml)
        .into_iter()
        .find(|&(line, _)| line > last_yaml)?;
    let after = &lines[line - 1..];
    let keys_after = after.iter().any(|later| is_commented_key(later));
    let blank_after = after.iter().any(|later| later.trim().is_empty())
        || yaml.ends_with('\n');
    (!keys_after && blank_after).then_some((line, text))
}

/// Document metadata from a JSON value, with the key rules of [`parse_front_matter`]: for a
/// notebook's `metadata.writer` object ("same keys as Markdown front matter",
/// notebook-mapping.md §2.2).
///
/// `null` → [`DocMeta::default()`]; any other non-object → `"expected a mapping"`. A
/// number or boolean for a string field becomes its JSON text (`42` → `"42"`, `1.50` →
/// `"1.5"`). Messages start with the offending key and carry no line numbers.
pub fn meta_from_json(value: &Value) -> Result<DocMeta> {
    let object = match value {
        Value::Null => return Ok(DocMeta::default()),
        Value::Object(object) => object,
        _ => return Err(Error::FrontMatter(NOT_A_MAPPING.to_string())),
    };
    let mut root = ParsedRoot::default();
    for (key, value) in object {
        let value = json_content_text(key, value.clone());
        EntrySeed { root: &mut root, key: key.clone() }
            .deserialize(value)
            .map_err(|err| Error::FrontMatter(format!("{key}: {err}")))?;
    }
    root.check_author_keys().map_err(Error::FrontMatter)?;
    Ok(root.meta)
}

/// Convert JSON content scalars to their JSON text before applying the same
/// visitors as YAML. Config and unknown values remain verbatim.
fn json_content_text(key: &str, mut value: Value) -> Value {
    fn scalar(value: &mut Value) {
        if matches!(value, Value::Bool(_) | Value::Number(_)) {
            *value = Value::String(value.to_string());
        }
    }
    if STRING_FIELDS.contains(&key) || key == "abstract" {
        scalar(&mut value);
    } else if key == "keywords" {
        if let Value::Array(items) = &mut value {
            items.iter_mut().for_each(scalar);
        } else {
            scalar(&mut value);
        }
    } else if matches!(key, "author" | "authors") {
        if let Value::Array(items) = &mut value {
            for item in items {
                if let Value::Object(fields) = item {
                    for name in ["name", "affiliation", "email", "id"] {
                        if let Some(value) = fields.get_mut(name) {
                            scalar(value);
                        }
                    }
                } else {
                    scalar(item);
                }
            }
        } else {
            scalar(&mut value);
        }
    }
    value
}

/// Parsing-only syntax flags; these do not belong in the document IR.
#[derive(Default)]
struct ParsedRoot {
    meta: DocMeta,
    author: bool,
    authors: bool,
    author_list: bool,
    author_maps: Vec<bool>,
    keywords_list: bool,
}

impl ParsedRoot {
    fn check_author_keys(&self) -> DeResult<(), String> {
        if self.author && self.authors {
            Err("use either author or authors".into())
        } else {
            Ok(())
        }
    }
}

/// The front matter root: `Some(meta)` for a mapping or an empty document, `None` for any
/// other value (reported as [`NOT_A_MAPPING`]).
struct RootVisitor;

impl<'de> Visitor<'de> for RootVisitor {
    type Value = Option<ParsedRoot>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a mapping")
    }

    fn visit_unit<E: de::Error>(self) -> DeResult<Self::Value, E> {
        Ok(Some(ParsedRoot::default()))
    }

    fn visit_none<E: de::Error>(self) -> DeResult<Self::Value, E> {
        Ok(Some(ParsedRoot::default()))
    }

    fn visit_map<A: MapAccess<'de>>(
        self, mut map: A,
    ) -> DeResult<Self::Value, A::Error> {
        let mut root = ParsedRoot::default();
        let mut seen = BTreeSet::new();
        while let Some(key) = map.next_key_seed(KeySeed(&mut seen))? {
            map.next_value_seed(EntrySeed { root: &mut root, key })?;
        }
        Ok(Some(root))
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> DeResult<Self::Value, E> {
        Ok(None)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> DeResult<Self::Value, E> {
        Ok(None)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> DeResult<Self::Value, E> {
        Ok(None)
    }

    fn visit_i128<E: de::Error>(self, _: i128) -> DeResult<Self::Value, E> {
        Ok(None)
    }

    fn visit_u128<E: de::Error>(self, _: u128) -> DeResult<Self::Value, E> {
        Ok(None)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> DeResult<Self::Value, E> {
        Ok(None)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> DeResult<Self::Value, E> {
        Ok(None)
    }

    fn visit_seq<A: SeqAccess<'de>>(
        self, mut seq: A,
    ) -> DeResult<Self::Value, A::Error> {
        // serde_norway checks that the whole sequence was consumed.
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }

    fn visit_enum<A: EnumAccess<'de>>(
        self, data: A,
    ) -> DeResult<Self::Value, A::Error> {
        unsupported_tag(data)
    }
}

/// One top-level entry: routes the value for `key` into `meta` (see [`parse_front_matter`]).
struct EntrySeed<'m> {
    root: &'m mut ParsedRoot,
    key: String,
}

impl<'de> DeserializeSeed<'de> for EntrySeed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(
        self, value: D,
    ) -> DeResult<(), D::Error> {
        let EntrySeed { root, key } = self;
        if let Some(field) = string_field(&mut root.meta, &key) {
            *field = Option::<String>::deserialize(value)?;
        } else if matches!(key.as_str(), "author" | "authors") {
            root.author |= key == "author";
            root.authors |= key == "authors";
            let parsed = value.deserialize_any(AuthorsVisitor {
                singular: key == "author",
            })?;
            root.author_list = key == "author" && parsed.list;
            root.author_maps = parsed.maps;
            root.meta.authors = parsed.authors;
        } else if key == "keywords" {
            let parsed = value.deserialize_any(KeywordsVisitor)?;
            root.keywords_list = parsed.1;
            root.meta.keywords = parsed.0;
        } else if key == "abstract" {
            if let Some(text) =
                Option::<String>::deserialize(value)?.filter(|s| !s.is_empty())
            {
                root.meta.vars.insert(key, Value::String(text));
            }
        } else if let Some((_, example)) =
            TABLES.iter().find(|(name, _)| *name == key)
        {
            let table = value.deserialize_any(TableVisitor {
                example,
                boolean: key == "numbering",
            })?;
            root.meta.vars.insert(key, table);
        } else {
            root.meta.vars.insert(key, JsonVisitor.deserialize(value)?);
        }
        Ok(())
    }
}

/// The [`DocMeta`] string field for `key`, if it is one of [`STRING_FIELDS`].
fn string_field<'m>(
    meta: &'m mut DocMeta, key: &str,
) -> Option<&'m mut Option<String>> {
    Some(match key {
        "title" => &mut meta.title,
        "subtitle" => &mut meta.subtitle,
        "date" => &mut meta.date,
        "description" => &mut meta.description,
        "lang" => &mut meta.lang,
        _ => return None,
    })
}

/// Numeric callbacks validate scalar shapes. YAML replays these scalar values
/// as strings after validation so their original spelling is not lost.
macro_rules! scalar_visits {
    () => {
        fn visit_bool<E: de::Error>(
            self, value: bool,
        ) -> DeResult<Self::Value, E> {
            self.visit_str(&value.to_string())
        }
        fn visit_i64<E: de::Error>(
            self, value: i64,
        ) -> DeResult<Self::Value, E> {
            self.visit_str(&value.to_string())
        }
        fn visit_u64<E: de::Error>(
            self, value: u64,
        ) -> DeResult<Self::Value, E> {
            self.visit_str(&value.to_string())
        }
        fn visit_i128<E: de::Error>(
            self, value: i128,
        ) -> DeResult<Self::Value, E> {
            self.visit_str(&value.to_string())
        }
        fn visit_u128<E: de::Error>(
            self, value: u128,
        ) -> DeResult<Self::Value, E> {
            self.visit_str(&value.to_string())
        }
        fn visit_f64<E: de::Error>(
            self, value: f64,
        ) -> DeResult<Self::Value, E> {
            self.visit_str(&value.to_string())
        }
    };
}

#[derive(Default)]
struct ParsedAuthors {
    authors: Vec<Author>,
    maps: Vec<bool>,
    list: bool,
}

struct AuthorsVisitor {
    singular: bool,
}

impl<'de> Visitor<'de> for AuthorsVisitor {
    type Value = ParsedAuthors;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(if self.singular {
            "an author scalar or a list of authors"
        } else {
            "a list of authors"
        })
    }

    fn visit_str<E: de::Error>(self, name: &str) -> DeResult<Self::Value, E> {
        if !self.singular {
            return Err(E::custom("authors must be a list"));
        }
        Ok(ParsedAuthors {
            authors: vec![Author { name: name.into(), ..Author::default() }],
            ..ParsedAuthors::default()
        })
    }

    fn visit_unit<E: de::Error>(self) -> DeResult<Self::Value, E> {
        if self.singular {
            Ok(ParsedAuthors::default())
        } else {
            Err(E::custom("authors must be a list"))
        }
    }

    fn visit_seq<A: SeqAccess<'de>>(
        self, mut seq: A,
    ) -> DeResult<Self::Value, A::Error> {
        let mut parsed =
            ParsedAuthors { list: true, ..ParsedAuthors::default() };
        while let Some((author, mapping)) =
            seq.next_element_seed(AuthorVisitor)?
        {
            parsed.authors.push(author);
            parsed.maps.push(mapping);
        }
        Ok(parsed)
    }

    scalar_visits!();
}

/// A list entry is a scalar name or a strict author mapping.
struct AuthorVisitor;

impl<'de> DeserializeSeed<'de> for AuthorVisitor {
    type Value = (Author, bool);

    fn deserialize<D: Deserializer<'de>>(
        self, value: D,
    ) -> DeResult<Self::Value, D::Error> {
        value.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for AuthorVisitor {
    type Value = (Author, bool);

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an author scalar or a mapping with a name")
    }

    fn visit_str<E: de::Error>(self, name: &str) -> DeResult<Self::Value, E> {
        Ok((Author { name: name.into(), ..Author::default() }, false))
    }

    fn visit_map<A: MapAccess<'de>>(
        self, mut map: A,
    ) -> DeResult<Self::Value, A::Error> {
        let mut author = Author::default();
        let mut name = None;
        let mut seen = BTreeSet::new();
        while let Some(key) = map.next_key_seed(KeySeed(&mut seen))? {
            let field = match key.as_str() {
                "name" => &mut name,
                "affiliation" => &mut author.affiliation,
                "email" => &mut author.email,
                "id" => &mut author.id,
                _ => {
                    return Err(de::Error::custom(format_args!(
                        "unknown author key `{key}`; expected name, affiliation, email or id"
                    )));
                }
            };
            *field = map.next_value::<Option<String>>()?;
        }
        author.name =
            name.ok_or_else(|| de::Error::custom("author requires a name"))?;
        Ok((author, true))
    }

    scalar_visits!();
}

struct KeywordsVisitor;

impl<'de> Visitor<'de> for KeywordsVisitor {
    type Value = (Vec<String>, bool);

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a keyword scalar or a list of scalars")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> DeResult<Self::Value, E> {
        Ok((vec![text.into()], false))
    }

    fn visit_unit<E: de::Error>(self) -> DeResult<Self::Value, E> {
        Ok((Vec::new(), false))
    }

    fn visit_seq<A: SeqAccess<'de>>(
        self, mut seq: A,
    ) -> DeResult<Self::Value, A::Error> {
        let mut keywords = Vec::new();
        while let Some(text) = seq.next_element::<String>()? {
            keywords.push(text);
        }
        Ok((keywords, true))
    }

    scalar_visits!();
}

/// Replay only author names and scalar keywords with `deserialize_str`, whose
/// YAML implementation preserves lexical spelling (including aliases).
struct TextReplay<'r>(&'r mut ParsedRoot);

impl<'de> Visitor<'de> for TextReplay<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a mapping")
    }

    fn visit_unit<E: de::Error>(self) -> DeResult<(), E> {
        Ok(())
    }

    fn visit_none<E: de::Error>(self) -> DeResult<(), E> {
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(
        self, mut map: A,
    ) -> DeResult<(), A::Error> {
        let root = self.0;
        while let Some(key) = map.next_key::<String>()? {
            if matches!(key.as_str(), "author" | "authors") {
                if root.author_list || key == "authors" {
                    map.next_value_seed(AuthorReplay {
                        authors: &mut root.meta.authors,
                        maps: &root.author_maps,
                    })?;
                } else if let Some(name) = map.next_value::<Option<String>>()? {
                    root.meta.authors[0].name = name;
                }
            } else if key == "keywords" && !root.keywords_list {
                root.meta.keywords =
                    map.next_value::<Option<String>>()?.into_iter().collect();
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }
}

struct AuthorReplay<'r> {
    authors: &'r mut [Author],
    maps: &'r [bool],
}

impl<'de> DeserializeSeed<'de> for AuthorReplay<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(
        self, value: D,
    ) -> DeResult<(), D::Error> {
        value.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for AuthorReplay<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a list of authors")
    }

    fn visit_seq<A: SeqAccess<'de>>(
        self, mut seq: A,
    ) -> DeResult<(), A::Error> {
        for (author, mapping) in self.authors.iter_mut().zip(self.maps) {
            if *mapping {
                seq.next_element::<IgnoredAny>()?;
            } else if let Some(name) = seq.next_element::<String>()? {
                author.name = name;
            }
        }
        Ok(())
    }
}

/// A mapping key as text (a non-string scalar key becomes its YAML text). A key already in
/// the set is an error; raising it while the key is read lets serde_norway report the
/// key's own line and column.
struct KeySeed<'s>(&'s mut BTreeSet<String>);

impl<'de> DeserializeSeed<'de> for KeySeed<'_> {
    type Value = String;

    fn deserialize<D: Deserializer<'de>>(
        self, key: D,
    ) -> DeResult<String, D::Error> {
        key.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for KeySeed<'_> {
    type Value = String;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a scalar key")
    }

    fn visit_str<E: de::Error>(self, key: &str) -> DeResult<String, E> {
        if self.0.insert(key.to_owned()) {
            Ok(key.to_owned())
        } else {
            Err(E::custom(format_args!("duplicate key `{key}`")))
        }
    }
}

/// A config mapping, preserved as JSON, or null when empty. Only the numbering
/// table also accepts a boolean; other shapes receive an example in the error.
struct TableVisitor {
    example: &'static str,
    boolean: bool,
}

impl<'de> Visitor<'de> for TableVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.boolean {
            f.write_str("a boolean or ")?;
        }
        write!(f, "a mapping, e.g. `{}`", self.example)
    }

    fn visit_unit<E: de::Error>(self) -> DeResult<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> DeResult<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> DeResult<Self::Value, E> {
        if self.boolean {
            Ok(Value::Bool(value))
        } else {
            Err(E::invalid_type(de::Unexpected::Bool(value), &self))
        }
    }

    fn visit_map<A: MapAccess<'de>>(
        self, map: A,
    ) -> DeResult<Self::Value, A::Error> {
        json_map(map).map(Value::Object)
    }

    fn visit_enum<A: EnumAccess<'de>>(
        self, data: A,
    ) -> DeResult<Self::Value, A::Error> {
        unsupported_tag(data)
    }
}

/// Any YAML value as JSON, for `vars`: integers outside the 64-bit range
/// become their decimal text, `.inf`/`.nan` become `null`, keys must be unique and custom
/// tags are errors.
#[derive(Clone, Copy)]
struct JsonVisitor;

impl<'de> DeserializeSeed<'de> for JsonVisitor {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(
        self, value: D,
    ) -> DeResult<Value, D::Error> {
        value.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any YAML value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> DeResult<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> DeResult<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> DeResult<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_i128<E: de::Error>(self, v: i128) -> DeResult<Value, E> {
        // serde_norway only calls this for integers that do not fit an i64.
        Ok(Value::String(v.to_string()))
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> DeResult<Value, E> {
        // serde_norway only calls this for integers that do not fit a u64.
        Ok(Value::String(v.to_string()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> DeResult<Value, E> {
        // `null` for infinities and NaN, which JSON cannot represent.
        Ok(Value::from(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> DeResult<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> DeResult<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E: de::Error>(self) -> DeResult<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> DeResult<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(
        self, mut seq: A,
    ) -> DeResult<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(self)? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> DeResult<Value, A::Error> {
        json_map(map).map(Value::Object)
    }

    fn visit_enum<A: EnumAccess<'de>>(
        self, data: A,
    ) -> DeResult<Value, A::Error> {
        unsupported_tag(data)
    }
}

/// A mapping's entries as JSON, in the order written; a repeated key is an error.
fn json_map<'de, A: MapAccess<'de>>(
    mut map: A,
) -> DeResult<Map<String, Value>, A::Error> {
    let mut seen = BTreeSet::new();
    let mut entries = Map::new();
    while let Some(key) = map.next_key_seed(KeySeed(&mut seen))? {
        let value = map.next_value_seed(JsonVisitor)?;
        entries.insert(key, value);
    }
    Ok(entries)
}

/// The error for a value with a custom YAML tag (serde_norway presents those as enums).
fn unsupported_tag<'de, A: EnumAccess<'de>, T>(
    data: A,
) -> DeResult<T, A::Error> {
    let (tag, _value) = data.variant::<String>()?;
    Err(de::Error::custom(format_args!(
        "YAML tags such as `!{tag}` are not supported"
    )))
}

/// Lines of `text` as `(content, end)`: `content` without its `\n` / `\r\n`, `end` the byte
/// offset just past the line break (`text.len()` for an unterminated last line).
fn lines_with_ends(text: &str) -> impl Iterator<Item = (&str, usize)> {
    text.split_inclusive('\n').scan(0, |offset, raw| {
        *offset += raw.len();
        Some((without_line_break(raw), *offset))
    })
}

/// `s` without one trailing `\n` or `\r\n`.
fn without_line_break(s: &str) -> &str {
    match s.strip_suffix('\n') {
        Some(rest) => rest.strip_suffix('\r').unwrap_or(rest),
        None => s,
    }
}

/// `s` without one leading `\n` or `\r\n`; `None` when it does not start with a line break.
fn strip_leading_line_break(s: &str) -> Option<&str> {
    s.strip_prefix('\n').or_else(|| s.strip_prefix("\r\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, FigureNumbering, HeadingNumbering, Toc};
    use comrak::nodes::NodeValue;

    fn meta(yaml: &str) -> DocMeta {
        parse_front_matter(yaml).unwrap()
    }

    fn front_matter_error(yaml: &str) -> String {
        match parse_front_matter(yaml) {
            Err(Error::FrontMatter(message)) => message,
            other => panic!(
                "expected Error::FrontMatter for {yaml:?}, got {other:?}"
            ),
        }
    }

    fn var_keys(meta: &DocMeta) -> Vec<&str> {
        meta.vars.keys().map(String::as_str).collect()
    }

    // ---- parse_front_matter ------------------------------------------------------------

    #[test]
    fn empty_yaml_is_default() {
        for yaml in ["", "\n", "   \n\n", "# only a comment\n", "~", "null\n"] {
            assert_eq!(meta(yaml), DocMeta::default(), "{yaml:?}");
        }
    }

    #[test]
    fn known_keys_go_to_fields_unknown_keys_to_vars_in_order() {
        let parsed = meta(
            "zeta: last letter\ntitle: Rust\nsubtitle: A start\n\
            date: 2026-10-24\ndescription: Talk\nlang: de\nauthors: [A, B]\n\
            keywords: [rust, science]\ntheme: ./theme\nhtml: {theme: x}\nalpha: [1, 2]\n",
        );
        assert_eq!(parsed.title.as_deref(), Some("Rust"));
        assert_eq!(parsed.subtitle.as_deref(), Some("A start"));
        assert_eq!(parsed.date.as_deref(), Some("2026-10-24"));
        assert_eq!(parsed.description.as_deref(), Some("Talk"));
        assert_eq!(parsed.lang.as_deref(), Some("de"));
        assert_eq!(
            parsed.authors.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(parsed.keywords, ["rust", "science"]);
        assert_eq!(var_keys(&parsed), ["zeta", "theme", "html", "alpha"]);
        assert_eq!(parsed.vars["html"], serde_json::json!({"theme": "x"}));
    }

    #[test]
    fn vars_preserve_author_order() {
        let meta = meta("c: 1\na: 2\nb: 3\ntitle: T\nd: 4\n");
        assert_eq!(var_keys(&meta), ["c", "a", "b", "d"]);
        assert_eq!(
            serde_json::to_string(&meta.vars).unwrap(),
            r#"{"c":1,"a":2,"b":3,"d":4}"#
        );
    }

    #[test]
    fn scalars_for_string_fields_keep_their_yaml_text() {
        let meta = meta(
            "date: 2026-10-08\ntitle: 42\nsubtitle: 1.50\nauthor: true\n\
             description: yes\nlang: 0x1F\ntheme: 2026-10-08T09:30:00Z\n",
        );
        assert_eq!(meta.date.as_deref(), Some("2026-10-08"));
        assert_eq!(meta.title.as_deref(), Some("42"));
        assert_eq!(meta.subtitle.as_deref(), Some("1.50"));
        assert_eq!(
            meta.authors.first().map(|author| author.name.as_str()),
            Some("true")
        );
        assert_eq!(meta.description.as_deref(), Some("yes"));
        assert_eq!(meta.lang.as_deref(), Some("0x1F"));
        assert_eq!(meta.vars["theme"].as_str(), Some("2026-10-08T09:30:00Z"));
    }

    #[test]
    fn quoted_block_and_null_string_fields() {
        let meta = meta(
            "title: \"42\"\nsubtitle: |\n  two\n  lines\nauthor: ~\ndate:\nlang: ''\n",
        );
        assert_eq!(meta.title.as_deref(), Some("42"));
        assert_eq!(meta.subtitle.as_deref(), Some("two\nlines\n"));
        assert_eq!(meta.authors, []);
        assert_eq!(meta.date, None);
        assert_eq!(meta.lang.as_deref(), Some(""));
        assert!(
            meta.vars.is_empty(),
            "null known keys must not leak into vars"
        );
    }

    #[test]
    fn non_scalar_string_field_is_an_error() {
        let message = front_matter_error("title: [a, b]\n");
        assert!(message.contains("title"), "{message}");
        let message = front_matter_error("author:\n  name: X\n");
        assert!(message.contains("author"), "{message}");
    }

    #[test]
    fn config_tables_must_be_mappings() {
        assert_eq!(
            front_matter_error("title: T\nmath: never\n"),
            "math: invalid type: string \"never\", expected a mapping, e.g. `math: {html: mathml}` at line 3 column 7"
        );
        for (key, _) in TABLES {
            for value in ["[x]", "3", "text"] {
                let message = front_matter_error(&format!("{key}: {value}\n"));
                assert!(
                    message.starts_with(&format!("{key}: invalid type")),
                    "{message}"
                );
                assert!(message.contains("mapping, e.g. `"), "{message}");
            }
            if key != "numbering" {
                assert!(
                    front_matter_error(&format!("{key}: true\n"))
                        .contains("invalid type")
                );
            }
        }
        let parsed = meta("html:\nmath: ~\n");
        assert_eq!(var_keys(&parsed), ["html", "math"]);
        assert_eq!(parsed.vars["html"], Value::Null);
        assert_eq!(parsed.vars["math"], Value::Null);
    }

    #[test]
    fn non_mapping_root_is_an_error() {
        for yaml in [
            "- a\n- b\n", "just a sentence\n", "42\n", "\"quoted\"\n", "true\n",
        ] {
            assert_eq!(
                front_matter_error(yaml),
                "expected a mapping",
                "{yaml:?}"
            );
        }
        assert_eq!(
            parse_front_matter("- a\n").unwrap_err().to_string(),
            "front matter: expected a mapping"
        );
    }

    #[test]
    fn yaml_syntax_errors_are_front_matter_errors() {
        // Line numbers are source lines: the YAML starts on line 2, after the opening `---`.
        assert_eq!(
            front_matter_error("title: Rust: the good parts\nauthor: Me\n"),
            "mapping values are not allowed in this context at line 2 column 12 (put the value \
             in quotes: `title: \"Rust: the good parts\"`)"
        );
        assert_eq!(
            front_matter_error("a: 1\n  b: 2\n"),
            "mapping values are not allowed in this context at line 3 column 4"
        );
        assert_eq!(
            front_matter_error("key: *undefined_alias\n"),
            "unknown anchor at line 2 column 6"
        );
        let message =
            front_matter_error("title: T\nauthor: Me\nlist:\n\t- a\n");
        assert!(message.contains("at line 5 column 1"), "{message}");
        let message = front_matter_error("title: T\nlist: [unclosed\n");
        assert!(message.contains("line 3"), "{message}");
        // CRLF line breaks count once.
        assert_eq!(
            front_matter_error("title: T\r\nkey: a: b\r\n"),
            "mapping values are not allowed in this context at line 3 column 7 (put the value \
             in quotes: `key: \"a: b\"`)"
        );
        // A type error is reported at the value's own line too.
        assert_eq!(
            front_matter_error("lang: de\ntitle: [a, b]\n"),
            "title: invalid type: sequence, expected a string at line 3 column 8"
        );
    }

    /// A3 (F29): hints for a colon in a plain value and for tab indentation, after the
    /// position, which still locates the error.
    #[test]
    fn yaml_errors_get_hints_for_colons_and_tabs() {
        let quote = |key: &str, value: &str| {
            format!(" (put the value in quotes: `{key}: \"{value}\"`)")
        };
        for (yaml, key, value) in [
            ("subtitle: Part 2: more\n", "subtitle", "Part 2: more"),
            ("title: Part 1:\n", "title", "Part 1:"),
            ("authors:\n  - name: Ada: the first\n", "name", "Ada: the first"),
            ("title: C:\\dir \"x\": y\n", "title", "C:\\\\dir \\\"x\\\": y"),
        ] {
            let message = front_matter_error(yaml);
            assert!(
                message.ends_with(&quote(key, value)),
                "{yaml:?}: {message}"
            );
        }
        let err =
            parse_front_matter("title: Rust: the good parts\n").unwrap_err();
        assert_eq!(err.span().map(|span| span.start_line), Some(2));
        assert_eq!(err.column(), Some(12));
        // no hint where quoting is not the fix
        for yaml in ["a: 1\n  b: 2\n", "title: \"a\": b\n", "title: [a: b\n"] {
            let message = front_matter_error(yaml);
            assert!(
                !message.contains("put the value in quotes"),
                "{yaml:?}: {message}"
            );
        }
        for yaml in [
            "title: X\n\tsubtitle: y\n", "title: T\nlist:\n\t- a\n",
            "a:\n  b: 1\n\tc: 2\n",
        ] {
            let message = front_matter_error(yaml);
            assert!(
                message.ends_with(" (indent with spaces, not tabs)"),
                "{yaml:?}: {message}"
            );
            let err = Error::FrontMatter(message);
            assert!(err.span().is_some() && err.column().is_some(), "{err}");
        }
    }

    fn document_front_matter(
        yaml: &str, closing: usize,
    ) -> (Result<DocMeta>, Vec<(&'static str, usize, usize)>) {
        let mut diagnostics = Diagnostics::new();
        let meta = parse_document_front_matter(yaml, closing, &mut diagnostics);
        let found = diagnostics
            .iter()
            .map(|d| {
                let span = d.span.as_ref().unwrap();
                (d.code, span.start_line, span.end_line)
            })
            .collect();
        (meta, found)
    }

    /// A3 (F05): front matter that swallowed heading content.
    #[test]
    fn document_front_matter_reports_swallowed_headings() {
        // comments only: the document started with a `---` rule
        let (meta, found) = document_front_matter("\n# Real\n", 4);
        assert_eq!(meta.unwrap(), DocMeta::default());
        assert_eq!(found, [(FRONT_MATTER_EMPTY, 1, 4)]);
        for yaml in ["", "\n", "~\n", "null\n"] {
            assert_eq!(document_front_matter(yaml, 3).1, [], "{yaml:?}");
        }
        // a heading after the last key and a blank line: the closing `---` is missing
        let (meta, found) =
            document_front_matter("title: T\n\n# Slide {layout=title}\n\n", 7);
        assert_eq!(meta.unwrap().title.as_deref(), Some("T"));
        assert_eq!(found, [(FRONT_MATTER_UNCLOSED, 4, 7)]);
        // comments that are not headings: labels, commented-out keys, no blank line before,
        // or YAML after them
        for yaml in [
            "# Front matter: settings for this document.\ntitle: T\n# date: 2026-01-31\n",
            "title: T\n\n# Theme options:\n# theme: dark\n",
            "title: T\n\n# Theme options\n# theme: dark\n",
            "title: T\n# Comment right under a key\n",
            "title: T\n\n# Section\nauthor: A\n",
            "title: T\n\n#hashtag\n",
            "title: T\n\n  # indented comment\n",
            "title: T\n\n# date: 2026\n",
        ] {
            assert_eq!(document_front_matter(yaml, 9).1, [], "{yaml:?}");
        }
        assert_eq!(
            swallowed_heading("a: 1\n\n## Agenda\n\n"),
            Some((4, "## Agenda"))
        );
        // A3 fix round: valid front matter is no false alarm: options all commented out
        // (keys or values), and a closing remark right on the closing `---` (as
        // `strip_delimiters` returns it: no final line break)
        for yaml in [
            "html:\n  # theme: x\n  # controls: false",
            "# title: My talk\n# author: Me",
            "# title: Draft\n\n# author: Me\n",
            "title: X\nauthor: Y\n\n# More options live in writer.toml",
            "title: Talk\n\n# Remember to update the abstract\n# before the conference",
        ] {
            assert_eq!(document_front_matter(yaml, 9).1, [], "{yaml:?}");
        }
        // the same remark followed by a blank line before the `---` is a swallowed heading
        let (_, found) = document_front_matter("title: X\n\n# Slide two\n", 6);
        assert_eq!(found, [(FRONT_MATTER_UNCLOSED, 4, 6)]);
        let (_, found) = document_front_matter(
            "title: X\n\n# Slide two\n\n# (more to say here)",
            8,
        );
        assert_eq!(found, [(FRONT_MATTER_UNCLOSED, 4, 8)]);
        // a heading among commented-out keys still says the document lost its first heading
        let (_, found) =
            document_front_matter("# date: 2026\n\n# Welcome\n", 6);
        assert_eq!(found, [(FRONT_MATTER_EMPTY, 1, 6)]);
        // not a mapping: located at the root value, with the question
        let (meta, _) =
            document_front_matter("\n# Real\n\nSome paragraph\n", 6);
        let Err(Error::FrontMatter(message)) = meta else {
            panic!("expected an error");
        };
        assert_eq!(
            message,
            "expected a mapping at line 5 column 1 (the front matter runs from line 1 to the \
             `---` on line 6; is its closing `---` missing, or was line 1 meant as a thematic break?)"
        );
        // a YAML error in front matter with a heading after a blank line gets the question
        let (meta, _) =
            document_front_matter("title: T\n\n# Title\n\nSome text\n", 7);
        let Err(Error::FrontMatter(message)) = meta else {
            panic!("expected an error");
        };
        assert!(
            message.ends_with(
                "(the front matter runs from line 1 to the `---` on line 7; is its closing `---` \
                 missing?)"
            ),
            "{message}"
        );
        // other errors are unchanged
        let (meta, _) = document_front_matter("html: fade\n", 3);
        assert!(
            matches!(meta, Err(Error::FrontMatter(m)) if m.ends_with("at line 2 column 7"))
        );
    }

    #[test]
    fn error_lines_match_the_markdown_file() {
        let document =
            "---\ntitle: Rust: the good parts\nauthor: Me\n---\n\n# Slide\n";
        let literal = comrak_literal(document).unwrap();
        let message = match parse_front_matter(strip_delimiters(&literal)) {
            Err(Error::FrontMatter(message)) => message,
            other => panic!("expected a front matter error, got {other:?}"),
        };
        let line = document
            .lines()
            .position(|line| line.starts_with("title:"))
            .unwrap()
            + 1;
        assert_eq!(line, 2);
        assert!(
            message.contains(&format!("at line {line} column 12")),
            "{message}"
        );
        // The same through the text-based split (notebook cells).
        let (yaml, _, _) = split_front_matter(document).unwrap();
        assert_eq!(front_matter_error(yaml), message);
    }

    #[test]
    fn vars_keep_yaml_types_with_yaml_1_2_scalars() {
        let meta = meta(
            "count: 3\nratio: 0.5\nflag: true\nyes_word: yes\non_word: on\nno_word: no\n\
             octal_like: 010\nhex: 0x10\npublished: 2026-10-08\nnothing: ~\nbig: .inf\n\
             nested:\n  list: [a, 1]\n  map: {k: v}\n42: numeric key\n",
        );
        let expected = serde_json::json!({
            "count": 3, "ratio": 0.5, "flag": true,
            "yes_word": "yes", "on_word": "on", "no_word": "no",
            "octal_like": "010", "hex": 16, "published": "2026-10-08",
            "nothing": null, "big": null,
            "nested": { "list": ["a", 1], "map": { "k": "v" } },
            "42": "numeric key",
        });
        assert_eq!(Value::Object(meta.vars), expected);
    }

    #[test]
    fn config_tables_stay_in_vars() {
        let parsed = meta(
            "numbering: {figures: section}\nmarkdown: {smart_punctuation: true}\n\
            math: {html: mathml}\nnotebook: {show_prompts: false}\nvenue: PyData\n",
        );
        assert_eq!(
            var_keys(&parsed),
            ["numbering", "markdown", "math", "notebook", "venue"]
        );
        assert_eq!(
            parsed.vars["numbering"],
            serde_json::json!({"figures": "section"})
        );
        let empty = TABLES
            .iter()
            .map(|(key, _)| format!("{key}:\n"))
            .collect::<String>();
        let mut config = Config::default();
        let before = serde_json::to_value(&config).unwrap();
        config.merge_front_matter(&meta(&empty)).unwrap();
        assert_eq!(serde_json::to_value(&config).unwrap(), before);
    }

    #[test]
    fn duplicate_keys_are_errors_at_the_repeated_key() {
        assert_eq!(
            front_matter_error("title: A\ntitle: B\n"),
            "duplicate key `title` at line 3 column 1"
        );
        assert_eq!(
            front_matter_error("a: 1\nb: 2\na: 3\n"),
            "duplicate key `a` at line 4 column 1"
        );
        // A second `extra:` block would otherwise silently drop the first one.
        assert_eq!(
            front_matter_error(
                "extra: {transition: fade}\nextra: {width: 10}\n"
            ),
            "duplicate key `extra` at line 3 column 1"
        );
        assert_eq!(
            front_matter_error("html:\n  theme: a\nhtml:\n  theme: b\n"),
            "duplicate key `html` at line 4 column 1"
        );
        // Nested mappings, inside extra and inside vars.
        assert_eq!(
            front_matter_error(
                "extra:\n  transition: fade\n  transition: zoom\n"
            ),
            "extra: duplicate key `transition` at line 4 column 3"
        );
        assert_eq!(
            front_matter_error("list: [{a: 1, a: 2}]\n"),
            "list[0]: duplicate key `a` at line 2 column 15"
        );
        // A non-string key is compared by its text.
        assert!(
            front_matter_error("1: a\n1: b\n").starts_with("duplicate key `1`")
        );
        // The same key in different mappings is fine.
        let meta = meta("a: {x: 1}\nb: {x: 2}\n");
        assert_eq!(meta.vars["b"]["x"], 2);
    }

    #[test]
    fn integers_outside_64_bits_keep_their_digits() {
        let parsed = meta(
            "title: 99999999999999999999\nlonger: 18446744073709551616\n\
             neg: -99999999999999999999\nmax: 18446744073709551615\nmin: -9223372036854775808\n\
             extra: {big: 340282366920938463463374607431768211455}\n",
        );
        assert_eq!(parsed.title.as_deref(), Some("99999999999999999999"));
        assert_eq!(parsed.vars["longer"], "18446744073709551616");
        assert_eq!(parsed.vars["neg"], "-99999999999999999999");
        // In range: still numbers.
        assert_eq!(parsed.vars["max"], u64::MAX);
        assert_eq!(parsed.vars["min"], i64::MIN);
        assert_eq!(parsed.vars["extra"]["big"], u128::MAX.to_string());
        // Beyond 128 bits serde_norway itself reads the scalar as a float.
        let huge = meta("huge: 999999999999999999999999999999999999999999\n");
        assert_eq!(huge.vars["huge"], Value::from(1e42));
    }

    #[test]
    fn custom_tags_are_rejected_with_a_clear_message() {
        assert_eq!(
            front_matter_error("x: !expr foo\n"),
            "x: YAML tags such as `!expr` are not supported at line 2 column 4"
        );
        assert_eq!(
            front_matter_error("extra: {width: !px 10}\n"),
            "extra.width: YAML tags such as `!px` are not supported at line 2 column 16"
        );
        assert!(front_matter_error("html: !t {theme: x}\n").contains("`!t`"));
        assert!(front_matter_error("!root\na: 1\n").contains("`!root`"));
        // Core-schema tags are plain YAML and work.
        let core = meta("title: !!str 42\nn: !!int 7\n");
        assert_eq!(core.title.as_deref(), Some("42"));
        assert_eq!(core.vars["n"], 7);
        // On a string field the scalar text is used and the tag ignored (documented).
        assert_eq!(meta("title: !t x\n").title.as_deref(), Some("x"));
    }

    #[test]
    fn crlf_yaml_parses() {
        let meta = meta("title: Hello\r\nauthor: Me\r\nvenue: PyData\r\n");
        assert_eq!(meta.title.as_deref(), Some("Hello"));
        assert_eq!(
            meta.authors.first().map(|author| author.name.as_str()),
            Some("Me")
        );
        assert_eq!(meta.vars["venue"], "PyData");
    }

    // ---- split_front_matter ------------------------------------------------------------

    #[test]
    fn split_basic() {
        assert_eq!(
            split_front_matter("---\na: 1\n---\nbody"),
            Some(("a: 1", "body", 3))
        );
        assert_eq!(
            split_front_matter("---\na: 1\nb: 2\n---\n\n# Slide\n"),
            Some(("a: 1\nb: 2", "\n# Slide\n", 4))
        );
    }

    #[test]
    fn split_requires_leading_delimiter_line() {
        assert_eq!(split_front_matter("body\n---\na: 1\n---\n"), None);
        assert_eq!(split_front_matter("\n---\na: 1\n---\n"), None);
        assert_eq!(split_front_matter(" ---\na: 1\n---\n"), None);
        assert_eq!(split_front_matter("--- \na: 1\n---\n"), None);
        assert_eq!(split_front_matter("----\na: 1\n---\n"), None);
        assert_eq!(split_front_matter("---"), None);
        assert_eq!(split_front_matter(""), None);
    }

    #[test]
    fn split_unterminated_is_none() {
        assert_eq!(split_front_matter("---\na: 1\n"), None);
        assert_eq!(split_front_matter("---\na: 1\nbody"), None);
        assert_eq!(split_front_matter("---\na: 1\n--- \nbody"), None);
        assert_eq!(split_front_matter("---\na: 1\n----\nbody"), None);
        assert_eq!(split_front_matter("---\n"), None);
    }

    #[test]
    fn split_crlf() {
        assert_eq!(
            split_front_matter("---\r\na: 1\r\n---\r\nbody"),
            Some(("a: 1", "body", 3))
        );
        assert_eq!(
            split_front_matter("---\r\na: 1\r\nb: 2\r\n---\r\n\r\nbody\r\n"),
            Some(("a: 1\r\nb: 2", "\r\nbody\r\n", 4))
        );
    }

    #[test]
    fn split_edge_cases() {
        // Closing delimiter at the end of the text.
        assert_eq!(split_front_matter("---\na: 1\n---"), Some(("a: 1", "", 3)));
        assert_eq!(
            split_front_matter("---\na: 1\n---\n"),
            Some(("a: 1", "", 3))
        );
        // As in comrak, the line after the opening delimiter never closes the block: an
        // empty block needs a blank line.
        assert_eq!(split_front_matter("---\n---\nbody"), None);
        assert_eq!(split_front_matter("---\n---"), None);
        assert_eq!(split_front_matter("---\r\n---\r\nbody"), None);
        assert_eq!(
            split_front_matter("---\n---\n---\nbody"),
            Some(("---", "body", 3))
        );
        assert_eq!(
            split_front_matter("---\n\n---\nbody"),
            Some(("", "body", 3))
        );
        // Byte-order mark.
        assert_eq!(
            split_front_matter("\u{feff}---\na: 1\n---\nbody"),
            Some(("a: 1", "body", 3))
        );
        // Only the first closing line counts.
        assert_eq!(
            split_front_matter("---\na: 1\n---\nb\n---\nc"),
            Some(("a: 1", "b\n---\nc", 3))
        );
        // Blank lines inside the YAML are kept, minus the final line break.
        assert_eq!(
            split_front_matter("---\na: 1\n\n---\nbody"),
            Some(("a: 1\n", "body", 4))
        );
    }

    #[test]
    fn split_then_parse() {
        let (yaml, rest, consumed) = split_front_matter(
            "---\r\ntitle: Hi\r\ndate: 2026-10-08\r\n---\r\n# Slide\r\n",
        )
        .unwrap();
        assert_eq!((rest, consumed), ("# Slide\r\n", 4));
        let meta = meta(yaml);
        assert_eq!(meta.title.as_deref(), Some("Hi"));
        assert_eq!(meta.date.as_deref(), Some("2026-10-08"));
    }

    // ---- strip_delimiters (against real comrak output) -----------------------------------

    /// comrak options per markdown-dialect.md §1 (with config defaults).
    fn comrak_options() -> comrak::Options<'static> {
        let mut o = comrak::Options::default();
        o.extension.front_matter_delimiter = Some("---".into());
        o.extension.table = true;
        o.extension.strikethrough = true;
        o.extension.tasklist = true;
        o.extension.autolink = true;
        o.extension.footnotes = true;
        o.extension.description_lists = true;
        o.extension.alerts = true;
        o.extension.math_dollars = true;
        o.extension.math_code = true;
        o.extension.block_directive = true;
        o.extension.header_attributes = true;
        o.extension.fenced_code_attributes = true;
        o.extension.link_attributes = true;
        o.extension.inline_code_attributes = true;
        o.parse.smart = false;
        o.parse.relaxed_autolinks = false;
        o.render.r#unsafe = true;
        o.render.escape = false;
        o.render.hardbreaks = false;
        o.render.github_pre_lang = false;
        o.render.full_info_string = false;
        o.render.sourcepos = false;
        o
    }

    /// The literal of the document's `FrontMatter` node, if comrak produced one.
    fn comrak_literal(document: &str) -> Option<String> {
        let arena = comrak::Arena::new();
        let root = comrak::parse_document(&arena, document, &comrak_options());
        root.children().find_map(|node| match &node.data().value {
            NodeValue::FrontMatter(literal) => Some(literal.clone()),
            _ => None,
        })
    }

    #[test]
    fn comrak_literal_shape_is_as_documented() {
        // Both delimiter lines plus the trailing blank line, verbatim.
        assert_eq!(
            comrak_literal("---\na: 1\n---\n\n# Slide\n").as_deref(),
            Some("---\na: 1\n---\n\n")
        );
        assert_eq!(
            comrak_literal("---\r\na: 1\r\n---\r\n\r\n# Slide\r\n").as_deref(),
            Some("---\r\na: 1\r\n---\r\n\r\n")
        );
        assert_eq!(comrak_literal("# Slide\n"), None);
    }

    #[test]
    fn strip_delimiters_on_comrak_literals() {
        let documents = [
            ("---\na: 1\n---\n# Slide\n", "a: 1"),
            ("---\na: 1\nb: 2\n---\n\n# Slide\n", "a: 1\nb: 2"),
            ("---\r\na: 1\r\nb: 2\r\n---\r\n# Slide\r\n", "a: 1\r\nb: 2"),
            ("---\r\na: 1\r\n---\r\n\r\n# Slide\r\n", "a: 1"),
            ("---\na: 1\n---", "a: 1"),
            ("---\na: 1\n---\n", "a: 1"),
            ("\u{feff}---\na: 1\n---\n\nbody", "a: 1"),
            ("---\na: 1\n\n---\n\nbody", "a: 1\n"),
            (
                "---\nlist:\n  - x\nkey: --- not a delimiter\n---\nbody",
                "list:\n  - x\nkey: --- not a delimiter",
            ),
        ];
        for (document, yaml) in documents {
            let literal = comrak_literal(document).unwrap_or_else(|| {
                panic!("comrak found no front matter in {document:?}")
            });
            assert_eq!(strip_delimiters(&literal), yaml, "literal {literal:?}");
            // Same YAML as the text-based extraction.
            let (split_yaml, _, _) = split_front_matter(document).unwrap();
            assert_eq!(split_yaml, yaml, "document {document:?}");
        }
    }

    #[test]
    fn strip_delimiters_without_delimiters() {
        assert_eq!(strip_delimiters("---\n---\n"), "");
        assert_eq!(strip_delimiters("---\r\n---\r\n\r\n"), "");
        assert_eq!(strip_delimiters("a: 1\n"), "a: 1");
        assert_eq!(strip_delimiters("---\na: ---\n"), "a: ---");
    }

    /// Every document of up to five lines built from `---`, `a: 1`, an empty line and `x`
    /// (LF or CRLF, with or without a final line break): comrak and [`split_front_matter`]
    /// agree on whether there is front matter, on its YAML, and on where the block ends
    /// (comrak's literal also takes one following empty line). Lines like `----` or `--- `
    /// and mixed line breaks are left out: there comrak's substring search has quirks that
    /// the line-based split deliberately does not copy.
    #[test]
    fn split_agrees_with_comrak_on_generated_documents() {
        const ATOMS: [&str; 4] = ["---", "a: 1", "", "x"];
        let mut documents = Vec::new();
        let mut sequences: Vec<Vec<&str>> = vec![Vec::new()];
        for _ in 0..5 {
            sequences = sequences
                .iter()
                .flat_map(|seq| {
                    ATOMS.iter().map(move |atom| {
                        let mut longer = seq.clone();
                        longer.push(atom);
                        longer
                    })
                })
                .collect();
            for seq in &sequences {
                for eol in ["\n", "\r\n"] {
                    documents.push(seq.join(eol));
                    documents.push(seq.join(eol) + eol);
                }
            }
        }
        assert_eq!(documents.len(), 4 * (4 + 16 + 64 + 256 + 1024));
        let mut with_front_matter = 0;
        for document in &documents {
            let literal = comrak_literal(document);
            let split = split_front_matter(document);
            let (Some(literal), Some((yaml, rest, consumed))) =
                (&literal, split)
            else {
                assert_eq!(
                    literal.is_some(),
                    split.is_some(),
                    "{document:?}: comrak {literal:?}, split {split:?}"
                );
                continue;
            };
            with_front_matter += 1;
            assert_eq!(strip_delimiters(literal), yaml, "{document:?}");
            let block = &document[..document.len() - rest.len()];
            assert_eq!(block.lines().count(), consumed, "{document:?}");
            let extra = literal.strip_prefix(block).unwrap_or_else(|| {
                panic!("{document:?}: literal {literal:?}, block {block:?}")
            });
            assert!(
                ["", "\n", "\r\n"].contains(&extra) && rest.starts_with(extra),
                "{document:?}: literal {literal:?}, block {block:?}"
            );
        }
        assert!(with_front_matter > 100, "{with_front_matter}");
    }

    #[test]
    fn fixture_front_matter_agrees_between_comrak_and_text_split() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/md");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|ext| ext != "md") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let Some(literal) = comrak_literal(&text) else {
                assert_eq!(
                    split_front_matter(&text),
                    None,
                    "{}",
                    path.display()
                );
                continue;
            };
            let (yaml, _, _) = split_front_matter(&text).unwrap();
            assert_eq!(strip_delimiters(&literal), yaml, "{}", path.display());
            parse_front_matter(yaml)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            checked += 1;
        }
        assert!(
            checked > 0,
            "no fixture with front matter found in {}",
            dir.display()
        );
    }

    // ---- meta_from_json -------------------------------------------------------------------

    #[test]
    fn json_object_follows_the_front_matter_key_rules() {
        let value = serde_json::json!({
            "zeta": 1, "title": 42, "subtitle": 1.50, "author": true,
            "date": "2026-10-24", "lang": null,
            "extra": {"width": 1.50}, "html": {"theme": "x"},
            "alpha": [1, {"k": "v"}],
        });
        let parsed = meta_from_json(&value).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("42"));
        assert_eq!(parsed.subtitle.as_deref(), Some("1.5"));
        assert_eq!(parsed.authors[0].name, "true");
        assert_eq!(parsed.date.as_deref(), Some("2026-10-24"));
        assert_eq!(parsed.lang, None);
        assert_eq!(var_keys(&parsed), ["zeta", "extra", "html", "alpha"]);
        assert_eq!(parsed.vars["extra"], serde_json::json!({"width": 1.5}));
        assert_eq!(
            meta_from_json(
                &serde_json::json!({"title": "T", "venue": "PyData"})
            )
            .unwrap(),
            meta("title: T\nvenue: PyData\n")
        );
        let parsed = meta_from_json(&serde_json::json!({
            "authors": [true, {"name": 42, "id": 1.5, "email": false}],
            "keywords": [true, 3], "abstract": false,
        }))
        .unwrap();
        assert_eq!(parsed.authors[0].name, "true");
        assert_eq!(parsed.authors[1].name, "42");
        assert_eq!(parsed.authors[1].id.as_deref(), Some("1.5"));
        assert_eq!(parsed.authors[1].email.as_deref(), Some("false"));
        assert_eq!(parsed.keywords, ["true", "3"]);
        assert_eq!(parsed.vars["abstract"], "false");
        assert_eq!(
            meta_from_json(&serde_json::json!({"author": ["A", "B"]}))
                .unwrap()
                .authors
                .len(),
            2
        );
    }

    #[test]
    fn json_errors_match_the_yaml_rules() {
        assert_eq!(meta_from_json(&Value::Null).unwrap(), DocMeta::default());
        for value in [
            serde_json::json!([1]),
            serde_json::json!("x"),
            serde_json::json!(3),
        ] {
            assert_eq!(
                meta_from_json(&value).unwrap_err().to_string(),
                "front matter: expected a mapping"
            );
        }
        for (value, expected) in [
            (
                serde_json::json!({"html": "fade"}),
                "html: invalid type: string \"fade\", expected a mapping, e.g. `html: {theme: ./theme}`",
            ),
            (
                serde_json::json!({"title": ["a"]}),
                "title: invalid type: sequence, expected a string",
            ),
        ] {
            let Error::FrontMatter(message) =
                meta_from_json(&value).unwrap_err()
            else {
                panic!("wrong error")
            };
            assert_eq!(message, expected);
        }
        let parsed =
            meta_from_json(&serde_json::json!({"html": null, "math": null}))
                .unwrap();
        assert_eq!(parsed.vars["html"], Value::Null);
        assert_eq!(parsed.vars["math"], Value::Null);
        for value in [
            serde_json::json!({"author": "A", "authors": []}),
            serde_json::json!({"authors": [], "author": null}),
        ] {
            let Error::FrontMatter(message) =
                meta_from_json(&value).unwrap_err()
            else {
                panic!("wrong error")
            };
            assert_eq!(message, "use either author or authors");
        }
        for value in [
            serde_json::json!({"authors": "A"}),
            serde_json::json!({"authors": [{"name": "A", "orcid": "x"}]}),
        ] {
            assert!(
                meta_from_json(&value)
                    .unwrap_err()
                    .to_string()
                    .starts_with("front matter: authors:")
            );
        }
    }

    #[test]
    fn author_string_becomes_one_author() {
        assert_eq!(
            meta("author: A\n").authors,
            [Author { name: "A".into(), ..Author::default() }]
        );
        assert!(meta("author: null\n").authors.is_empty());
    }

    #[test]
    fn authors_strings_and_maps() {
        let parsed = meta(
            "authors: [A, {name: B, affiliation: EIT, email: b@x.org, id: 0000-0001}]\n",
        );
        assert_eq!(
            parsed.authors,
            [
                Author { name: "A".into(), ..Author::default() },
                Author {
                    name: "B".into(),
                    affiliation: Some("EIT".into()),
                    email: Some("b@x.org".into()),
                    id: Some("0000-0001".into())
                }
            ]
        );
        let parsed = meta(
            "authors: [1.50, 0x1F, {name: 1.50, affiliation: true, email: null, id: 0x1F}]\n",
        );
        assert_eq!(parsed.authors[0].name, "1.50");
        assert_eq!(parsed.authors[1].name, "0x1F");
        assert_eq!(parsed.authors[2].name, "1.50");
        assert_eq!(parsed.authors[2].id.as_deref(), Some("0x1F"));
        assert_eq!(parsed.authors[2].affiliation.as_deref(), Some("true"));
        assert_eq!(parsed.authors[2].email, None);
        assert_eq!(meta("author: 1.50\n").authors[0].name, "1.50");
    }

    #[test]
    fn content_scalar_spelling_survives_yaml_aliases() {
        let parsed =
            meta("value: &name 1.50\nauthor: *name\nkeywords: *name\n");
        assert_eq!(parsed.authors[0].name, "1.50");
        assert_eq!(parsed.keywords, ["1.50"]);
        let parsed = meta(
            "value: &name 0x1F\nauthors: [*name, {name: *name, id: *name}]\n",
        );
        assert_eq!(parsed.authors[0].name, "0x1F");
        assert_eq!(parsed.authors[1].name, "0x1F");
        assert_eq!(parsed.authors[1].id.as_deref(), Some("0x1F"));
    }

    #[test]
    fn author_list_is_read_with_a_warning() {
        let (parsed, diagnostics) =
            document_front_matter("title: T\nauthor: [A, B]\n", 4);
        assert_eq!(parsed.unwrap().authors.len(), 2);
        assert_eq!(diagnostics, [(FRONT_MATTER_AUTHOR_LIST, 3, 3)]);
        assert_eq!(
            document_front_matter("author : [A]\n", 3).1,
            [(FRONT_MATTER_AUTHOR_LIST, 2, 2)]
        );
        assert_eq!(
            document_front_matter("'author': [A]\n", 3).1,
            [(FRONT_MATTER_AUTHOR_LIST, 2, 2)]
        );
        assert_eq!(
            document_front_matter("{author: [A]}\n", 3).1,
            [(FRONT_MATTER_AUTHOR_LIST, 1, 3)]
        );
    }

    #[test]
    fn author_and_authors_is_an_error() {
        for yaml in [
            "author: A\nauthors: [B]\n", "authors: [B]\nauthor: A\n",
            "author: null\nauthors: []\n",
        ] {
            assert_eq!(
                front_matter_error(yaml),
                "use either author or authors"
            );
        }
    }

    #[test]
    fn author_map_errors() {
        for (yaml, piece) in [
            ("authors: [{name: A, orcid: x}]\n", "orcid"),
            ("authors: [{email: a@b}]\n", "requires a name"),
            ("authors: A\n", "must be a list"),
        ] {
            let message = front_matter_error(yaml);
            assert!(
                message.contains("authors") && message.contains(piece),
                "{message}"
            );
            assert!(message.contains("line 2 column"), "{message}");
        }
    }

    #[test]
    fn keywords_string_or_list() {
        assert_eq!(meta("keywords: a\n").keywords, ["a"]);
        assert_eq!(meta("keywords: [a, b]\n").keywords, ["a", "b"]);
        assert_eq!(meta("keywords: 1.50\n").keywords, ["1.50"]);
        assert_eq!(meta("keywords: [1.50, 0x1F]\n").keywords, ["1.50", "0x1F"]);
        assert_eq!(meta("keywords: null\n").keywords, Vec::<String>::new());
        assert!(parse_front_matter("keywords: [a, {b: c}]\n").is_err());
    }

    #[test]
    fn abstract_stays_in_vars() {
        let parsed = meta("abstract: |\n  One paragraph.\n");
        assert_eq!(parsed.vars["abstract"], "One paragraph.\n");
        assert_eq!(parsed.abstract_, None);
    }

    #[test]
    fn empty_abstract_is_dropped() {
        for yaml in ["abstract: \"\"\n", "abstract: null\n"] {
            assert!(!meta(yaml).vars.contains_key("abstract"));
        }
    }

    #[test]
    fn config_like_keys_stay_in_vars_verbatim() {
        let parsed = meta(
            "toc: 2\nbibliography: refs.bib\ncite_style: ieee\ncsl: apa\ncite_locale: de\n\
            theme: ./theme\ntemplate: a.html\nreference_doc: a.docx\nlatex_template: a.tex\npaper: a4\n",
        );
        assert_eq!(
            var_keys(&parsed),
            [
                "toc", "bibliography", "cite_style", "csl", "cite_locale",
                "theme", "template", "reference_doc", "latex_template",
                "paper"
            ]
        );
        assert_eq!(parsed.vars["bibliography"], "refs.bib");
        assert_eq!(
            meta("bibliography: [a.bib, b.bib]\n").vars["bibliography"],
            serde_json::json!(["a.bib", "b.bib"])
        );
    }

    #[test]
    fn numbering_bool_or_mapping() {
        assert_eq!(meta("numbering: true\n").vars["numbering"], true);
        assert_eq!(
            meta("numbering: {figures: section}\n").vars["numbering"],
            serde_json::json!({"figures": "section"})
        );
        assert!(
            front_matter_error("numbering: \"1.1\"\n")
                .contains("a boolean or a mapping")
        );
    }

    #[test]
    fn merge_config_like_keys() {
        let mut config = Config::default();
        config.merge_front_matter(&meta("toc: 2\nnumbering: true\ntheme: x\npaper: us-letter\ncite_style: apa\n")).unwrap();
        assert!(matches!(config.document.toc, Toc::Depth(2)));
        assert!(
            matches!(&config.numbering.headings, HeadingNumbering::Pattern(p) if p == "1.1")
        );
        assert_eq!(config.html.theme, "x");
        assert_eq!(config.pdf.paper, "us-letter");
        assert_eq!(config.references.style, "apa");
        config.merge_front_matter(&meta("toc: true\n")).unwrap();
        assert!(matches!(config.document.toc, Toc::Depth(3)));
    }

    #[test]
    fn merge_tables_key_by_key() {
        let mut config = Config::default();
        config.numbering.headings = HeadingNumbering::Pattern("I.A".into());
        config
            .merge_front_matter(&meta("numbering: {figures: section}\n"))
            .unwrap();
        assert!(
            matches!(&config.numbering.headings, HeadingNumbering::Pattern(p) if p == "I.A")
        );
        assert!(matches!(config.numbering.figures, FigureNumbering::Section));
    }

    #[test]
    fn merge_is_idempotent() {
        let mut config = Config::default();
        let parsed = meta(
            "markdown: {smart_punctuation: true}\nmath: {html: mathml}\n\
            notebook: {show_prompts: false}\nhtml: {theme: x}\n",
        );
        config.merge_front_matter(&parsed).unwrap();
        let once = serde_json::to_value(&config).unwrap();
        config.merge_front_matter(&parsed).unwrap();
        assert_eq!(serde_json::to_value(&config).unwrap(), once);
        assert!(config.markdown.smart_punctuation);
        assert!(!config.notebook.show_prompts);
    }

    #[test]
    fn merge_unknown_table_key_is_an_error_with_hint() {
        let mut config = Config::default();
        let before = serde_json::to_value(&config).unwrap();
        let err =
            config.merge_front_matter(&meta("html: {thme: x}\n")).unwrap_err();
        assert!(matches!(err, Error::FrontMatter(_)), "{err}");
        assert!(err.to_string().contains("did you mean 'theme'?"), "{err}");
        assert_eq!(serde_json::to_value(&config).unwrap(), before);
    }

    #[test]
    fn merge_rejects_yaml_1_1_booleans_in_config_tables() {
        let mut config = Config::default();
        let err = config
            .merge_front_matter(&meta("markdown: {smart_punctuation: yes}\n"))
            .unwrap_err();
        assert!(matches!(err, Error::FrontMatter(_)), "{err}");
        assert!(err.to_string().contains("markdown"), "{err}");
    }
}
