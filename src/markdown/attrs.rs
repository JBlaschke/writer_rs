//! Markdown attribute parsing (markdown-dialect.md §3).
//! Owner: T11. Stub created by T00.

//! The raw grammar keeps source order and accepts single-quoted values.
//! Element recovery validates keys before applying the parsed attributes.

use crate::error::{Diagnostic, Diagnostics, Error, Result};
use crate::ir::{Attrs, SourceSpan};

/// Diagnostic code for an identifier that cannot be a Typst label (error).
pub const LABEL_INVALID: &str = "label-invalid";

/// Converts parsed attributes to the IR, validating labels and keeping the
/// last value of each pair. Invalid labels are dropped after being reported.
pub fn ir_attrs(
    parsed: ParsedAttrs, span: Option<&SourceSpan>,
    diagnostics: &mut Diagnostics,
) -> Attrs {
    let id = parsed.id.filter(|id| {
        if typst_syntax::is_valid_label_literal_id(id) {
            return true;
        }
        let mut diagnostic = Diagnostic::error(
            LABEL_INVALID,
            format!(
                "label `{id}` is invalid; use letters, digits, `_`, `-`, `:` or `.`, e.g. `#fig-x`"
            ),
        );
        diagnostic.span = span.cloned();
        diagnostics.push(diagnostic);
        false
    });
    Attrs {
        id,
        classes: parsed.classes,
        pairs: parsed.pairs.into_iter().collect(),
    }
}

/// A raw `{#id .class key=value}` block in source order, before IR conversion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedAttrs {
    /// The `#id`, if any (the last one wins).
    pub id: Option<String>,
    /// The `.class` entries, in source order (not deduplicated).
    pub classes: Vec<String>,
    /// `key=value` pairs in source order; a bare `key` is `(key, "")`. Keys keep their case.
    pub pairs: Vec<(String, String)>,
}

impl ParsedAttrs {
    /// True when there is no id, class or pair.
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.classes.is_empty() && self.pairs.is_empty()
    }
}

/// comrak's parsed attributes (headings, fenced code, links, images, inline code).
impl From<&comrak::nodes::Attributes> for ParsedAttrs {
    fn from(attrs: &comrak::nodes::Attributes) -> Self {
        ParsedAttrs {
            id: attrs.id.clone(),
            classes: attrs.classes.clone(),
            pairs: attrs.pairs.clone(),
        }
    }
}

/// Parses `{#id .cls key=value key2="v 2" bare}` (braces optional). Errors on unbalanced quotes.
///
/// Grammar (dialect §3.1): one leading `{` and one trailing `}` are skipped; the rest is split
/// on whitespace outside quotes. Each item is `#id`, `.class`, `key=value` (split at the first
/// `=` outside quotes) or a bare `key` (→ `(key, "")`). Double or single quotes group
/// whitespace anywhere in an item and are removed; inside quotes a backslash escapes the next
/// character. Unquoted values run to the next whitespace, so `background=#111` works here
/// although comrak's heading grammar rejects it. Items with an empty key (`=v`) and a bare `#`
/// or `.` are ignored; a quoted empty id or class (`#""`, `.""`) is kept as `""`, as comrak
/// does. Keys are not validated here (see [`is_valid_attr_name`]). Every block comrak accepts
/// on a heading parses to the same [`ParsedAttrs`] here (shared unit tests, including a
/// differential run over random blocks).
pub fn parse_attr_block(s: &str) -> Result<ParsedAttrs> {
    let trimmed = s.trim();
    let inner = trimmed.strip_prefix('{').unwrap_or(trimmed);
    let inner = inner.strip_suffix('}').unwrap_or(inner);
    let mut out = ParsedAttrs::default();
    for token in tokenize(inner).map_err(|quote| {
        Error::Other(format!(
            "unbalanced {quote} quote in attribute block {{{inner}}}"
        ))
    })? {
        token.classify_into(&mut out);
    }
    Ok(out)
}

/// How an attribute block that comrak left as text reads with the dialect §3.1 grammar
/// ([`read_element_attrs`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementAttrs {
    /// Every item is an `#id`, a `.class` or a `key=value` pair with a valid key name: the
    /// attributes, exactly as [`parse_attr_block`] reads the block.
    Attrs(ParsedAttrs),
    /// Not meant as attributes (`{1, 2}`, `{curly braces}`, R Markdown's `{r}`): the block
    /// stays text and nothing is reported.
    Text,
    /// Meant as attributes (some item is valid, or a known presenter key is bare,
    /// such as `{auto-animate}`), but not valid; the reason, for a warning.
    Invalid(String),
}

/// Reads `block` (`{…}`, braces included) as the attributes of a heading, fenced code,
/// link, image or inline code (dialect §3.1, A3 contract note): with [`parse_attr_block`]'s
/// tokenizer, so single-quoted values and unquoted values with any character but whitespace
/// work, but stricter in what an item may be. Each item must be `#id`, `.class` (both
/// non-empty or quoted) or `key=value` with an unquoted key that [`is_valid_attr_name`]
/// accepts; a bare key (`{auto-animate}`) needs a value there (`auto-animate=""`), as §3.2
/// says, no item may end with an unquoted comma (`{debug=true, verbose=false}` is prose
/// or another tool's syntax: items are separated by spaces), and no item may have a `{` or
/// `}` outside quotes (two blocks written together, `{.dark}{k=v}`, which reach here as one
/// block, or a stray `}`). The strictness keeps ordinary text in braces as text.
///
/// [`ElementAttrs::Invalid`] when the block has an unbalanced quote or an unquoted brace
/// in an item and starts like attributes (`#x`, `.x` or `key=`), or when some item is
/// invalid while another one is valid or a known presenter key has no
/// value (`{.dark auto-animate}`, `{auto-animate}`); [`ElementAttrs::Text`] for anything
/// else that is not attributes.
pub fn read_element_attrs(block: &str) -> ElementAttrs {
    let Some(inner) =
        block.strip_prefix('{').and_then(|rest| rest.strip_suffix('}'))
    else {
        return ElementAttrs::Text;
    };
    let tokens = match tokenize(inner) {
        Ok(tokens) => tokens,
        Err(quote) if starts_like_attrs(inner) => {
            return ElementAttrs::Invalid(format!("unbalanced {quote} quote"));
        }
        Err(_) => return ElementAttrs::Text,
    };
    let mut out = ParsedAttrs::default();
    let (mut valid, mut known_bare, mut problem) = (0, false, None);
    let brace = tokens.iter().any(|token| token.brace);
    for token in tokens {
        match token.element_problem() {
            None => {
                valid += 1;
                token.classify_into(&mut out);
            }
            Some(reason) => {
                known_bare |= token.is_known_bare_key();
                problem.get_or_insert(reason);
            }
        }
    }
    match problem {
        None => ElementAttrs::Attrs(out),
        Some(reason) if valid > 0 || known_bare => {
            ElementAttrs::Invalid(reason)
        }
        Some(reason) if brace && starts_like_attrs(inner) => {
            ElementAttrs::Invalid(reason)
        }
        Some(_) => ElementAttrs::Text,
    }
}

/// Whether the inside of a block starts like an attribute item: `#x`, `.x` or `key=`.
fn starts_like_attrs(inner: &str) -> bool {
    let inner = inner.trim_start();
    if let Some(rest) = inner.strip_prefix(['#', '.']) {
        return rest.starts_with(|c: char| !c.is_whitespace());
    }
    inner.split_once('=').is_some_and(|(key, _)| is_valid_attr_name(key))
}

/// One whitespace-separated item of an attribute block, with its quotes removed.
#[derive(Default)]
struct Token {
    /// The item's text without quotes and escapes.
    text: String,
    /// Byte offset in `text` of the first `=` that was outside quotes.
    eq: Option<usize>,
    /// The item's first character when it was outside quotes (only then is `#`/`.` a marker).
    lead: Option<char>,
    /// Whether any character (quoted or not) has been read.
    started: bool,
    /// Whether the item contained a quoted run (so `#""` is an empty id, not a bare `#`).
    quoted: bool,
    /// Whether the item's last character was inside quotes.
    ends_quoted: bool,
    /// Whether the item has a `{` or `}` outside quotes.
    brace: bool,
}

impl Token {
    /// Why this item is not a valid element attribute ([`read_element_attrs`]); `None` when
    /// it is one.
    fn element_problem(&self) -> Option<String> {
        if self.brace {
            // Two blocks written together (`{.dark}{k=v}`), read as one, or a stray brace.
            return Some(format!(
                "`{}` has a `{{` or `}}` outside quotes: write one block with its items \
                 separated by spaces, or quote the brace",
                self.text
            ));
        }
        let comma = (self.text.ends_with(',') && !self.ends_quoted).then(|| {
            format!(
                "`{}` ends with a comma: items are separated by spaces, not commas",
                self.text
            )
        });
        match (self.lead, self.eq) {
            (Some(marker @ ('#' | '.')), _) => {
                let rest = &self.text[marker.len_utf8()..];
                (rest.is_empty() && !self.quoted)
                    .then(|| format!("`{marker}` without a name after it"))
                    .or(comma)
            }
            (lead, Some(at)) => {
                let key = &self.text[..at];
                (lead.is_none() || !is_valid_attr_name(key))
                    .then(|| format!("`{key}` is not a valid attribute name"))
                    .or(comma)
            }
            (_, None) if is_valid_attr_name(&self.text) => Some(format!(
                "`{key}` has no value: write `{key}=\"\"`",
                key = self.text
            )),
            (_, None) => Some(format!(
                "`{}` is not an attribute (`#id`, `.class` or `key=value`)",
                self.text
            )),
        }
    }

    /// A bare key (no `=`) that names a presenter attribute: a shorthand, a
    /// `data-`/`aria-` key, `layout`, `id` or `class`.
    fn is_known_bare_key(&self) -> bool {
        if self.eq.is_some()
            || matches!(self.lead, Some('#' | '.'))
            || self.text.is_empty()
        {
            return false;
        }
        let key = self.text.to_lowercase();
        matches!(key.as_str(), "layout" | "id" | "class")
            || key == "background"
            || SLIDE_KEYS.binary_search(&key.as_str()).is_ok()
            || key.starts_with("data-")
            || key.starts_with("aria-")
    }

    fn classify_into(self, out: &mut ParsedAttrs) {
        match self.lead {
            Some('#') => {
                let id = &self.text['#'.len_utf8()..];
                if !id.is_empty() || self.quoted {
                    out.id = Some(id.to_string());
                }
            }
            Some('.') => {
                let class = &self.text['.'.len_utf8()..];
                if !class.is_empty() || self.quoted {
                    out.classes.push(class.to_string());
                }
            }
            _ => match self.eq {
                Some(at) => {
                    let (key, value) =
                        (&self.text[..at], &self.text[at + '='.len_utf8()..]);
                    if !key.is_empty() {
                        out.pairs.push((key.to_string(), value.to_string()));
                    }
                }
                None if !self.text.is_empty() => {
                    out.pairs.push((self.text, String::new()))
                }
                None => {}
            },
        }
    }
}

/// Splits on whitespace outside quotes. `Err(quote)` names the quote character left open.
fn tokenize(s: &str) -> std::result::Result<Vec<Token>, char> {
    let mut tokens = Vec::new();
    let mut current: Option<Token> = None;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            tokens.extend(current.take());
            continue;
        }
        let token = current.get_or_insert_with(Token::default);
        match c {
            '"' | '\'' => {
                read_quoted(&mut chars, c, &mut token.text)?;
                token.quoted = true;
            }
            '=' if token.eq.is_none() => {
                token.eq = Some(token.text.len());
                token.text.push(c);
            }
            _ => {
                if !token.started {
                    token.lead = Some(c);
                }
                token.brace |= matches!(c, '{' | '}');
                token.text.push(c);
            }
        }
        token.ends_quoted = matches!(c, '"' | '\'');
        token.started = true;
    }
    tokens.extend(current);
    Ok(tokens)
}

/// Reads a quoted run up to the closing `quote` (already past the opening one), appending its
/// content with backslash escapes resolved. `Err(quote)` when the input ends first.
fn read_quoted(
    chars: &mut std::str::Chars<'_>, quote: char, out: &mut String,
) -> std::result::Result<(), char> {
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next().ok_or(quote)?),
            c if c == quote => return Ok(()),
            c => out.push(c),
        }
    }
    Err(quote)
}

/// Whether a key follows comrak's ASCII attribute-name grammar.
pub fn is_valid_attr_name(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')
        })
}

/// Presenter keys whose bare form signals an intended attribute block.
const SLIDE_KEYS: &[&str] = &[
    "auto-animate", "auto-animate-duration", "auto-animate-easing",
    "auto-animate-id", "auto-animate-restart", "auto-animate-unmatched",
    "autoslide", "background-color", "background-gradient",
    "background-iframe", "background-image", "background-interactive",
    "background-opacity", "background-position", "background-repeat",
    "background-size", "background-transition", "background-video",
    "background-video-loop", "background-video-muted", "menu-title", "state",
    "timing", "transition", "transition-speed", "visibility",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownConfig;
    use crate::markdown::comrak_options;
    use comrak::nodes::NodeValue;

    fn attrs(
        id: Option<&str>, classes: &[&str], pairs: &[(&str, &str)],
    ) -> ParsedAttrs {
        ParsedAttrs {
            id: id.map(str::to_string),
            classes: classes.iter().map(|c| c.to_string()).collect(),
            pairs: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// Attribute blocks comrak accepts on a heading; both parsers must agree on each.
    const SHARED_CASES: &[&str] = &[
        "{}",
        "{#intro}",
        "{.a .b}",
        "{k=v}",
        "{k=\"v 2\"}",
        "{#intro .big k=v}",
        "{layout=section .x}",
        "{auto-animate=\"\"}",
        "{.b .shared #merged transition=zoom auto-animate=\"\"}",
        "{.oken #yip m=26 .sure x=3% #yap}",
        "{#\"has space, will travel\" .\"ok\\\"en\" title=\"是非 \\\"not\\\"\"}",
        "{ #a   .b\tc=d }",
        "{data-background-image=\"img/a b.png\" target=_blank width=50% x:y=1}",
        "{background=\"#111\" class=\"one two\"}",
        // empty quoted id / class are kept as `""`, like comrak
        "{#\"\"}",
        "{.\"\" .a}",
        // escapes and characters that are Markdown syntax outside the block
        "{title=\"a \\\"q\\\" b\"}",
        "{data-path=\"C:\\\\dir\"}",
        "{data-x=\"&lt;b&gt;\" data-y=\"<b>\"}",
        "{background=\"img/_draft_.png\" data-z=\"a*b*c $x$ `c`\"}",
    ];

    fn comrak_heading_attrs(block: &str) -> Option<ParsedAttrs> {
        let arena = comrak::Arena::new();
        let options = comrak_options(&MarkdownConfig::default());
        let root = comrak::parse_document(
            &arena,
            &format!("# Heading {block}\n"),
            &options,
        );
        let heading = root.first_child()?;
        let data = heading.data();
        assert!(matches!(data.value, NodeValue::Heading(_)), "{block}");
        data.attrs.as_deref().map(ParsedAttrs::from)
    }

    #[test]
    fn parse_attr_block_agrees_with_comrak_on_heading_attributes() {
        for block in SHARED_CASES {
            let from_comrak =
                comrak_heading_attrs(block).unwrap_or_else(|| {
                    panic!("comrak did not parse {block} as heading attributes")
                });
            let ours = parse_attr_block(block).unwrap();
            assert_eq!(ours, from_comrak, "{block}");
        }
    }

    /// A small deterministic generator (64-bit LCG), so the differential test needs no
    /// dependency and always checks the same blocks.
    struct Lcg(u64);

    impl Lcg {
        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) % n as u64) as usize
        }
    }

    /// Differential check: random blocks over the grammar's alphabet; whenever comrak accepts
    /// one as heading attributes, `parse_attr_block` must return the same `ParsedAttrs`.
    #[test]
    fn parse_attr_block_agrees_with_comrak_on_random_blocks() {
        let alphabet: Vec<char> = "ab#.=\" \\'-_:%1\tZé".chars().collect();
        let mut rng = Lcg(0x5eed);
        let mut accepted = 0;
        for _ in 0..20_000 {
            let len = rng.below(12);
            let body: String =
                (0..len).map(|_| alphabet[rng.below(alphabet.len())]).collect();
            let block = format!("{{{body}}}");
            let Some(from_comrak) = comrak_heading_attrs(&block) else {
                continue;
            };
            accepted += 1;
            assert_eq!(
                parse_attr_block(&block).ok(),
                Some(from_comrak),
                "{block:?}"
            );
        }
        assert!(accepted > 1_000, "only {accepted} blocks accepted by comrak");
    }

    /// A3: blocks comrak rejects on a heading but §3.1 allows; the heading form reads them
    /// with the shared recovery grammar (`markdown::recover`).
    const RECOVERED_CASES: &[&str] = &[
        "{.dark background=#222}",
        "{background=figures/bg.png}",
        "{background=https://ex.com/a.png?x=1&y=2}",
        "{lines=1|3 .numbered}",
        "{lines=1,3}",
        "{k='v w' #id}",
        "{title='say \"hi\"' data-x='a\\'b'}",
        "{data-x=a(b);c data-y=+!*@~$^}",
    ];

    #[test]
    fn read_element_attrs_reads_what_parse_attr_block_reads() {
        for block in SHARED_CASES.iter().chain(RECOVERED_CASES) {
            assert_eq!(
                read_element_attrs(block),
                ElementAttrs::Attrs(parse_attr_block(block).unwrap()),
                "{block}"
            );
        }
        for block in RECOVERED_CASES {
            assert_eq!(
                comrak_heading_attrs(block),
                None,
                "comrak accepts {block}"
            );
        }
    }

    #[test]
    fn read_element_attrs_keeps_text_and_reports_mistakes() {
        for text in [
            "{1, 2, 3}", "{curly braces}", "{r}", "{python}", "{it's}", "{=v}",
            "no braces",
        ] {
            assert_eq!(read_element_attrs(text), ElementAttrs::Text, "{text}");
        }
        let invalid = |block: &str| match read_element_attrs(block) {
            ElementAttrs::Invalid(reason) => reason,
            other => panic!("{block}: {other:?}"),
        };
        assert_eq!(
            invalid("{.dark auto-animate}"),
            "`auto-animate` has no value: write `auto-animate=\"\"`"
        );
        assert!(
            invalid("{auto-animate}")
                .starts_with("`auto-animate` has no value")
        );
        assert!(invalid("{layout}").starts_with("`layout` has no value"));
        assert_eq!(invalid("{k='v}"), "unbalanced ' quote");
        assert_eq!(invalid("{.a k=\"v}"), "unbalanced \" quote");
        assert_eq!(
            invalid("{r, echo=FALSE}"),
            "`r,` is not an attribute (`#id`, `.class` or `key=value`)"
        );
        assert_eq!(invalid("{.a 1x=2}"), "`1x` is not a valid attribute name");
        assert_eq!(
            invalid("{.a \"k\"=v}"),
            "`k` is not a valid attribute name"
        );
        assert_eq!(invalid("{# .a}"), "`#` without a name after it");
        // items separated by commas are prose or another tool's syntax, not attributes
        assert_eq!(
            invalid("{debug=true, verbose=false}"),
            "`debug=true,` ends with a comma: items are separated by spaces, not commas"
        );
        assert!(invalid("{.a, .b}").starts_with("`.a,` ends with a comma"));
        assert!(invalid("{#a, .b}").starts_with("`#a,` ends with a comma"));
        assert_eq!(read_element_attrs("{a, b}"), ElementAttrs::Text);
        // Wave-8 review: two blocks written together reach here as one block, and a stray
        // `}` ends up in an item; neither may become mangled attributes.
        assert_eq!(
            invalid("{.dark}{background='#222'}"),
            "`.dark}{background=#222` has a `{` or `}` outside quotes: write one block with \
             its items separated by spaces, or quote the brace"
        );
        assert!(
            invalid("{.dark}{background=\"#222\"}")
                .starts_with("`.dark}{background=#222`")
        );
        assert!(
            invalid("{k='v'}}")
                .starts_with("`k=v}` has a `{` or `}` outside quotes")
        );
        assert!(
            invalid("{.a}{lines='1|3'}")
                .starts_with("`.a}{lines=1|3` has a `{`")
        );
        assert!(invalid("{.a k=v}x}").starts_with("`k=v}x` has a `{`"));
        assert_eq!(read_element_attrs("{a}{b}"), ElementAttrs::Text);
        // quoted braces are content
        assert_eq!(
            read_element_attrs("{data-x='a {b}' .c}"),
            ElementAttrs::Attrs(ParsedAttrs {
                id: None,
                classes: vec!["c".into()],
                pairs: vec![("data-x".into(), "a {b}".into())],
            })
        );
        // a comma inside a value or quoted is fine
        for block in ["{lines=1,3}", "{k=\"a,\"}", "{k='a,' j=b}", "{.\"a,\"}"]
        {
            assert!(
                matches!(read_element_attrs(block), ElementAttrs::Attrs(_)),
                "{block}"
            );
        }
    }

    /// Every block comrak accepts on a heading reads the same with `read_element_attrs`.
    #[test]
    fn read_element_attrs_agrees_with_comrak_on_random_blocks() {
        let alphabet: Vec<char> = "ab#.=\" \\'-_:%1\tZé/|".chars().collect();
        let mut rng = Lcg(0xa3);
        let mut accepted = 0;
        for _ in 0..20_000 {
            let len = rng.below(12);
            let body: String =
                (0..len).map(|_| alphabet[rng.below(alphabet.len())]).collect();
            let block = format!("{{{body}}}");
            let Some(from_comrak) = comrak_heading_attrs(&block) else {
                continue;
            };
            accepted += 1;
            assert_eq!(
                read_element_attrs(&block),
                ElementAttrs::Attrs(from_comrak),
                "{block:?}"
            );
        }
        assert!(accepted > 1_000, "only {accepted} blocks accepted by comrak");
    }

    #[test]
    fn parse_attr_block_examples() {
        let cases: &[(&str, ParsedAttrs)] = &[
            (
                "{#id .cls key=value key2=\"v 2\" bare}",
                attrs(
                    Some("id"),
                    &["cls"],
                    &[("key", "value"), ("key2", "v 2"), ("bare", "")],
                ),
            ),
            // braces are optional
            (
                "#id .cls key=value",
                attrs(Some("id"), &["cls"], &[("key", "value")]),
            ),
            ("", ParsedAttrs::default()),
            ("{ }", ParsedAttrs::default()),
            // the raw grammar allows what comrak's heading grammar does not
            ("{auto-animate}", attrs(None, &[], &[("auto-animate", "")])),
            (
                "{.dark background=#111 layout=title}",
                attrs(
                    None,
                    &["dark"],
                    &[("background", "#111"), ("layout", "title")],
                ),
            ),
            (
                "aria-label='Single quoted' k=\"it's\" j='say \"hi\"'",
                attrs(
                    None,
                    &[],
                    &[
                        ("aria-label", "Single quoted"),
                        ("k", "it's"),
                        ("j", "say \"hi\""),
                    ],
                ),
            ),
            (
                "background=\"rgba(0, 0, 0, 0.5)\" background-opacity=0.4",
                attrs(
                    None,
                    &[],
                    &[
                        ("background", "rgba(0, 0, 0, 0.5)"),
                        ("background-opacity", "0.4"),
                    ],
                ),
            ),
            // only the first unquoted `=` splits; quoted `=` and `}` are content
            (
                "a=b=c d=\"x=}\"",
                attrs(None, &[], &[("a", "b=c"), ("d", "x=}")]),
            ),
            // backslash escapes inside quotes, literal outside
            (
                r#"k="a\"b\\c" w=C:\x"#,
                attrs(None, &[], &[("k", r#"a"b\c"#), ("w", r"C:\x")]),
            ),
            // bare `#` / `.` and empty keys are ignored; quoted empty id / class are kept
            ("{# . =v \"\" .ok}", attrs(None, &["ok"], &[])),
            ("{#\"\" .\"\" .''}", attrs(Some(""), &["", ""], &[])),
            // the last id wins, keys keep their case
            ("#a #b Data-X=1", attrs(Some("b"), &[], &[("Data-X", "1")])),
            ("k= x", attrs(None, &[], &[("k", ""), ("x", "")])),
        ];
        for (input, expected) in cases {
            assert_eq!(&parse_attr_block(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn parse_attr_block_rejects_unbalanced_quotes() {
        for input in [
            "{k=\"v}", "k='v", "#\"id", "{.a k=\"x\\\"}", "k=\"a\" j=\"b",
            "k=\"trailing backslash\\",
        ] {
            let err = parse_attr_block(input).unwrap_err();
            assert!(
                matches!(&err, Error::Other(m) if m.contains("unbalanced")),
                "{input}: {err:?}"
            );
        }
    }

    #[test]
    fn invalid_attribute_names_are_dropped() {
        for (key, valid) in [
            ("k", true),
            ("data-x", true),
            ("aria-label", true),
            ("x:y.z_w-1", true),
            ("Data-X", true),
            ("", false),
            ("1x", false),
            ("-x", false),
            ("_x", false),
            ("a b", false),
            (">x", false),
            ("a}", false),
            ("{b", false),
            ("a\"b", false),
            ("é", false),
            ("a=b", false),
        ] {
            assert_eq!(is_valid_attr_name(key), valid, "{key:?}");
        }
    }

    #[test]
    fn from_comrak_attributes_copies_every_field() {
        let comrak_attrs = comrak::nodes::Attributes {
            id: Some("i".into()),
            classes: vec!["c".into()],
            pairs: vec![("k".into(), "v".into())],
        };
        let parsed = ParsedAttrs::from(&comrak_attrs);
        assert_eq!(parsed, attrs(Some("i"), &["c"], &[("k", "v")]));
        assert!(!parsed.is_empty());
        assert!(ParsedAttrs::default().is_empty());
    }

    #[test]
    fn slide_keys_are_sorted() {
        assert!(SLIDE_KEYS.windows(2).all(|keys| keys[0] < keys[1]));
        assert_eq!(SLIDE_KEYS.len(), 26);
    }

    #[test]
    fn ir_attrs_collects_pairs_last_wins() {
        let mut diagnostics = Diagnostics::new();
        let parsed = parse_attr_block("{k=1 k=2}").unwrap();
        let attrs = ir_attrs(parsed, None, &mut diagnostics);
        assert_eq!(attrs.pairs.get("k").map(String::as_str), Some("2"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn ir_attrs_rejects_invalid_labels() {
        let parsed = comrak_heading_attrs("{#\"with space\"}").unwrap();
        let span =
            SourceSpan { start_line: 7, end_line: 7, cell: None, file: None };
        let mut diagnostics = Diagnostics::new();
        let attrs = ir_attrs(parsed, Some(&span), &mut diagnostics);
        assert!(attrs.id.is_none());
        let found: Vec<_> = diagnostics.iter().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, LABEL_INVALID);
        assert_eq!(found[0].level, crate::error::Level::Error);
        assert_eq!(found[0].span.as_ref(), Some(&span));
        assert!(found[0].message.contains("with space"));
        assert!(found[0].message.contains("#fig-x"));
        for id in ["fig:x", "größe", "sec-intro"] {
            let parsed = parse_attr_block(&format!("{{#{id}}}")).unwrap();
            let mut diagnostics = Diagnostics::new();
            assert_eq!(
                ir_attrs(parsed, None, &mut diagnostics).id.as_deref(),
                Some(id)
            );
            assert!(diagnostics.is_empty());
        }
    }
}
