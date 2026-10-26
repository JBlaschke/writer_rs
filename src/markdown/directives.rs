//! Directive info parsing and colon recovery (markdown-dialect.md §5.2–§5.3).
//! Owner: T11. Stub created by T00.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

use crate::error::{Diagnostic, Diagnostics};
use crate::ir::SourceSpan;
use crate::markdown::attrs::{ParsedAttrs, parse_attr_block};
use crate::markdown::recover::split_lines;

/// Diagnostic code for a directive line rewritten before parsing (info).
pub const DIRECTIVE_COLON_REWRITTEN: &str = "directive-colon-rewritten";
/// Diagnostic code for a line that comrak cannot open as a directive (warning).
pub const INVALID_DIRECTIVE: &str = "invalid-directive";

/// A named directive's tokens or a Pandoc attribute block used as its info.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirectiveInfo {
    /// The first token, or the first class of a Pandoc block.
    pub name: String,
    /// The block's identifier, if present.
    pub id: Option<String>,
    /// Other classes, in source order.
    pub classes: Vec<String>,
    /// Arguments in source order, with bare block keys carrying empty values.
    pub args: Vec<(String, String)>,
    /// Malformed quotes or attribute blocks, for the caller's diagnostics.
    pub problems: Vec<String>,
}

/// Reads named info, a whole Pandoc attribute block, or named info followed by
/// an attribute block. Bare named tokens remain classes verbatim.
pub fn parse_directive_info(info: &str) -> DirectiveInfo {
    let trimmed = info.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        let mut out = DirectiveInfo::default();
        merge_block(&mut out, trimmed, info);
        if !out.classes.is_empty() {
            out.name = out.classes.remove(0);
        }
        return out;
    }
    let block_at = trailing_block(trimmed);
    let named = block_at.map_or(trimmed, |at| trimmed[..at].trim_end());
    let (tokens, unbalanced) = split_info(named);
    let mut out = DirectiveInfo::default();
    if let Some(quote) = unbalanced {
        out.problems.push(format!(
            "unbalanced {quote} quote in directive `:::{info}` (read to the end of the line)"
        ));
    }
    let mut tokens = tokens.into_iter();
    out.name = tokens.next().map(|token| token.text).unwrap_or_default();
    for token in tokens {
        match token.eq {
            Some(eq) => {
                let (key, value) = (&token.text[..eq], &token.text[eq + 1..]);
                out.args.push((key.to_string(), value.to_string()));
            }
            None => out.classes.push(token.text),
        }
    }
    if let Some(at) = block_at {
        merge_block(&mut out, &trimmed[at..], info);
    }
    out
}

/// The opening brace of a trailing block, outside quotes and after whitespace.
fn trailing_block(info: &str) -> Option<usize> {
    if !info.ends_with('}') {
        return None;
    }
    let mut quote = None;
    let mut escaped = false;
    let mut previous_space = false;
    for (at, c) in info.char_indices() {
        if let Some(current) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == current {
                quote = None;
            }
        } else if matches!(c, '\'' | '"') {
            quote = Some(c);
        } else if c == '{' && previous_space {
            return Some(at);
        }
        previous_space = c.is_whitespace();
    }
    None
}

fn merge_block(out: &mut DirectiveInfo, block: &str, info: &str) {
    match parse_attr_block(block) {
        Ok(parsed) => merge_attrs(out, parsed),
        Err(error) => {
            out.problems.push(format!("{error} in directive `:::{info}`"))
        }
    }
}

/// Shared class/id pair handling for whole and trailing attribute blocks.
fn merge_attrs(out: &mut DirectiveInfo, parsed: ParsedAttrs) {
    if let Some(id) = parsed.id {
        out.id = (!id.is_empty()).then_some(id);
    }
    out.classes.extend(parsed.classes.into_iter().filter(|c| !c.is_empty()));
    for (key, value) in parsed.pairs {
        match key.to_ascii_lowercase().as_str() {
            "class" => {
                out.classes.extend(value.split_whitespace().map(str::to_string))
            }
            "id" => out.id = (!value.is_empty()).then_some(value),
            _ => out.args.push((key, value)),
        }
    }
}

/// Rewrites colon-bearing directive candidates in document/directive paragraphs.
/// Returns rewritten 1-based lines in ascending order, preserving line endings
/// and the number of lines. Unchanged inputs remain borrowed.
pub fn colon_prepass<'t>(
    text: &'t str, opts: &Options,
) -> (Cow<'t, str>, Vec<usize>) {
    let arena = Arena::new();
    let root = parse_document(&arena, text, opts);
    let mut candidates = BTreeSet::new();
    for node in root.descendants() {
        if !matches!(node.data().value, NodeValue::Paragraph)
            || !node.parent().is_some_and(|parent| {
                matches!(parent.data().value, NodeValue::Document | NodeValue::BlockDirective(_))
            })
            || !node.first_child().is_some_and(|first| {
                matches!(&first.data().value, NodeValue::Text(text) if text.starts_with(":::"))
            })
        {
            continue;
        }
        let pos = node.data().sourcepos;
        let end = pos.end.line.saturating_sub(usize::from(pos.end.column == 0));
        candidates.extend(pos.start.line..=end.max(pos.start.line));
    }
    let (starts, lines) = split_lines(text);
    let mut rewritten = Vec::new();
    let mut edited = String::new();
    let mut copied = 0;
    for line in candidates {
        let Some((&start, body)) =
            starts.get(line - 1).zip(lines.get(line - 1))
        else {
            continue;
        };
        let Some(fence_end) = fence_end(body) else {
            continue;
        };
        let rest = &body[fence_end..];
        let trimmed = rest.trim_end();
        let without_colons = trimmed.trim_end_matches(':');
        let rest = if trimmed.len() - without_colons.len() >= 3 {
            without_colons.trim_end()
        } else {
            rest
        };
        let replacement = rest.replace(':', "&#58;");
        if replacement == body[fence_end..] {
            continue;
        }
        edited.push_str(&text[copied..start + fence_end]);
        edited.push_str(&replacement);
        copied = start + body.len();
        rewritten.push(line);
    }
    if rewritten.is_empty() {
        return (Cow::Borrowed(text), rewritten);
    }
    edited.push_str(&text[copied..]);
    (Cow::Owned(edited), rewritten)
}

/// Byte offset after an opening fence at indentation zero through three.
fn fence_end(line: &str) -> Option<usize> {
    let rest = line.trim_start_matches(char::is_whitespace);
    let indent = line.len() - rest.len();
    if line[..indent].chars().count() > 3 {
        return None;
    }
    let count = rest.bytes().take_while(|&c| c == b':').count();
    (count >= 3).then_some(indent + count)
}

/// Reports each line that starts with an unopened directive fence in text or
/// indented code. Fenced code, HTML, and escaped colons remain uninterpreted.
pub fn check_unopened_directives<'a>(
    root: &'a AstNode<'a>, diagnostics: &mut Diagnostics,
) {
    let mut found = BTreeMap::new();
    for node in root.descendants() {
        let data = node.data();
        match &data.value {
            NodeValue::Text(text)
                if text.starts_with(":::") && starts_line(node) =>
            {
                let line = data.sourcepos.start.line;
                found
                    .entry(line)
                    .or_insert_with(|| unopened_message(text, false));
            }
            NodeValue::CodeBlock(code) if !code.fenced => {
                let (_, lines) = split_lines(&code.literal);
                for (offset, text) in lines.iter().enumerate() {
                    if fence_end(text).is_some() {
                        let line = data.sourcepos.start.line + offset;
                        found
                            .entry(line)
                            .or_insert_with(|| unopened_message(text, true));
                    }
                }
            }
            _ => {}
        }
    }
    for (line, message) in found {
        diagnostics.push(
            Diagnostic::warning(INVALID_DIRECTIVE, message).with_span(
                SourceSpan {
                    start_line: line,
                    end_line: line,
                    cell: None,
                    file: None,
                },
            ),
        );
    }
}

fn starts_line<'a>(node: &'a AstNode<'a>) -> bool {
    match node.previous_sibling() {
        Some(previous) => matches!(
            previous.data().value,
            NodeValue::SoftBreak | NodeValue::LineBreak
        ),
        None => node.parent().is_some_and(|parent| {
            matches!(
                parent.data().value,
                NodeValue::Paragraph | NodeValue::Heading(_)
            )
        }),
    }
}

/// A source line that resembles a directive but was left as text by comrak.
struct Unopened {
    name: String,
    pandoc: bool,
    closing_colons: bool,
}

fn unopened_directive(text: &str) -> Unopened {
    let line = text.lines().next().unwrap_or_default().trim_start();
    let rest = line.trim_start_matches(':').trim_start();
    let word = |text: &str| -> String {
        text.chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
            .collect()
    };
    match rest.strip_prefix('{') {
        Some(block) => Unopened {
            name: block
                .split_whitespace()
                .find_map(|token| token.strip_prefix('.'))
                .map(word)
                .unwrap_or_default(),
            pandoc: true,
            closing_colons: rest.trim_end().ends_with(':'),
        },
        None => {
            Unopened { name: word(rest), pandoc: false, closing_colons: false }
        }
    }
}

fn unopened_message(text: &str, indented: bool) -> String {
    let unopened = unopened_directive(text);
    let name = unopened.name;
    let shown = if unopened.pandoc {
        format!("::: {{.{name} …}}")
    } else {
        format!(":::{name} …")
    };
    let fix = if indented {
        "indent the fence with at most three spaces"
    } else if unopened.closing_colons {
        "remove the `:::` after the attribute block (pandoc allows it, comrak does not)"
    } else {
        "write the colon as `&#58;`"
    };
    format!(
        "`{shown}` is not a directive: comrak did not open the fence; {fix}"
    )
}

/// One whitespace-separated item of a directive info string, quotes removed.
#[derive(Default)]
struct InfoToken {
    text: String,
    /// Byte offset in `text` of the first `=` outside quotes.
    eq: Option<usize>,
}

/// Splits on whitespace outside `"…"` / `'…'` (backslash escapes the next character inside
/// quotes). An unclosed quote runs to the end; its character is returned.
fn split_info(info: &str) -> (Vec<InfoToken>, Option<char>) {
    let mut tokens = Vec::new();
    let mut current: Option<InfoToken> = None;
    let mut unbalanced = None;
    let mut chars = info.chars();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            tokens.extend(current.take());
            continue;
        }
        let token = current.get_or_insert_with(InfoToken::default);
        match c {
            '"' | '\'' => loop {
                match chars.next() {
                    None => {
                        unbalanced = Some(c);
                        break;
                    }
                    Some('\\') => token.text.extend(chars.next()),
                    Some(q) if q == c => break,
                    Some(other) => token.text.push(other),
                }
            },
            '=' if token.eq.is_none() => {
                token.eq = Some(token.text.len());
                token.text.push(c);
            }
            _ => token.text.push(c),
        }
    }
    tokens.extend(current);
    (tokens, unbalanced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MarkdownConfig;
    use crate::markdown::comrak_options;

    fn info(
        name: &str, classes: &[&str], id: Option<&str>, args: &[(&str, &str)],
    ) -> DirectiveInfo {
        DirectiveInfo {
            name: name.into(),
            id: id.map(str::to_string),
            classes: classes.iter().map(|c| c.to_string()).collect(),
            args: args
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            problems: Vec::new(),
        }
    }

    #[test]
    fn pandoc_directive_info() {
        for (text, expected) in [
            ("{.fragment}", info("fragment", &[], None, &[])),
            (
                " {.fragment .fade-up} ",
                info("fragment", &["fade-up"], None, &[]),
            ),
            (
                "{.column width=\"40%\"}",
                info("column", &[], None, &[("width", "40%")]),
            ),
            ("{#intro .note}", info("note", &[], Some("intro"), &[])),
            (
                "{.a class=\"b c\" id=x k='v w'}",
                info("a", &["b", "c"], Some("x"), &[("k", "v w")]),
            ),
            ("{#only}", info("", &[], Some("only"), &[])),
            (
                "columns widths=\"1fr 2fr\" big",
                info("columns", &["big"], None, &[("widths", "1fr 2fr")]),
            ),
            ("", info("", &[], None, &[])),
        ] {
            assert_eq!(parse_directive_info(text), expected, "{text:?}");
        }
        assert_eq!(parse_directive_info("{.a k='v}").problems.len(), 1);
    }

    #[test]
    fn named_info_with_trailing_block() {
        for (text, expected) in [
            (
                "figure {#fig-x .wide width=80%}",
                info("figure", &["wide"], Some("fig-x"), &[("width", "80%")]),
            ),
            (
                "callout {title=\"Fig 1: x\"}",
                info("callout", &[], None, &[("title", "Fig 1: x")]),
            ),
            (
                "figure #c1 .big {#fig-x .wide class='a b' id=over width=80%}",
                info(
                    "figure",
                    &["#c1", ".big", "wide", "a", "b"],
                    Some("over"),
                    &[("width", "80%")],
                ),
            ),
            (
                "callout title='quoted {brace}' {kind=tip}",
                info(
                    "callout",
                    &[],
                    None,
                    &[("title", "quoted {brace}"), ("kind", "tip")],
                ),
            ),
        ] {
            assert_eq!(parse_directive_info(text), expected, "{text:?}");
        }
        assert_eq!(parse_directive_info("figure {title='x}").problems.len(), 1);
    }

    #[test]
    fn split_info_is_quote_aware() {
        let (tokens, unbalanced) =
            split_info(r#"columns widths="1fr 2fr" 'a b' k='v "w"' x=y=z"#);
        let texts: Vec<_> = tokens
            .iter()
            .map(|token| (token.text.as_str(), token.eq))
            .collect();
        assert_eq!(
            texts,
            [
                ("columns", None),
                ("widths=1fr 2fr", Some(6)),
                ("a b", None),
                ("k=v \"w\"", Some(1)),
                ("x=y=z", Some(1))
            ]
        );
        assert_eq!(unbalanced, None);
        assert_eq!(split_info("a 'b").1, Some('\''));
    }

    fn prepass(text: &str) -> (Cow<'_, str>, Vec<usize>) {
        colon_prepass(text, &comrak_options(&MarkdownConfig::default()))
    }

    #[test]
    fn colon_prepass_rewrites_the_colon_trap() {
        let text =
            ":::figure #fig:wrapped\n![A](a.png)\n\nCaption.\n:::\n\nafter\n";
        let (edited, lines) = prepass(text);
        assert_eq!(lines, [1]);
        assert!(edited.starts_with(":::figure #fig&#58;wrapped\n"));
        let arena = Arena::new();
        let opts = comrak_options(&MarkdownConfig::default());
        let root = parse_document(&arena, &edited, &opts);
        let first = root.first_child().unwrap();
        let data = first.data();
        assert!(
            matches!(&data.value, NodeValue::BlockDirective(d) if d.info == "figure #fig:wrapped")
        );
        assert_eq!(
            (data.sourcepos.start.line, data.sourcepos.end.line),
            (1, 5)
        );
        let last = root.last_child().unwrap().first_child().unwrap();
        assert!(
            matches!(&last.data().value, NodeValue::Text(text) if text == "after")
        );
    }

    #[test]
    fn colon_prepass_strips_pandoc_closing_colons() {
        let (edited, lines) =
            prepass("::: {.note title=\"a: b\"} :::\nx\n:::\n");
        assert_eq!(lines, [1]);
        assert_eq!(edited, "::: {.note title=\"a&#58; b\"}\nx\n:::\n");
    }

    #[test]
    fn colon_prepass_reaches_directive_children() {
        let (edited, lines) = prepass("::::outer\n:::inner: x\ny\n:::\n::::\n");
        assert_eq!(lines, [2]);
        assert_eq!(edited, "::::outer\n:::inner&#58; x\ny\n:::\n::::\n");
    }

    #[test]
    fn colon_prepass_leaves_lists_quotes_and_code_alone() {
        for text in [
            "- item\n  :::note: x\n", "> :::note: x\n",
            "```text\n:::note: x\n```\n", "Some text\n:::note: x\n",
        ] {
            let (edited, lines) = prepass(text);
            assert!(matches!(edited, Cow::Borrowed(_)), "{text:?}");
            assert!(lines.is_empty());
        }
    }

    #[test]
    fn colon_prepass_keeps_lines_and_line_endings() {
        for ending in ["\n", "\r\n", "\r"] {
            let text =
                format!(":::note title='a:b'{ending}body{ending}:::{ending}");
            let expected = format!(
                ":::note title='a&#58;b'{ending}body{ending}:::{ending}"
            );
            let (edited, lines) = prepass(&text);
            assert_eq!(edited, expected);
            assert_eq!(lines, [1]);
            assert_eq!(
                split_lines(&edited).1.len(),
                split_lines(&text).1.len()
            );
        }
    }

    #[test]
    fn colon_prepass_is_idempotent() {
        for text in [
            ":::figure #fig:wrapped\nx\n:::\n",
            "::: {.note} :::\nx\n:::\n",
            "::::outer\n:::inner: x\ny\n:::\n::::\n",
            ":::note title='a:b'\r\nx\r\n:::\r\n",
        ] {
            let (edited, _) = prepass(text);
            let (again, lines) = prepass(&edited);
            assert!(matches!(again, Cow::Borrowed(_)));
            assert_eq!(again, edited);
            assert!(lines.is_empty());
        }
    }

    fn unopened(text: &str) -> Vec<Diagnostic> {
        let arena = Arena::new();
        let opts = comrak_options(&MarkdownConfig::default());
        let root = parse_document(&arena, text, &opts);
        let mut diagnostics = Diagnostics::new();
        check_unopened_directives(root, &mut diagnostics);
        diagnostics.iter().cloned().collect()
    }

    #[test]
    fn unopened_directive_lines_are_reported() {
        for (text, line) in [
            ("- item\n  :::note: x", 2),
            ("> :::note: x", 1),
            ("Some text\n:::note: x", 2),
            (":::note: x\n---", 1),
            ("\t:::note", 1),
        ] {
            let found = unopened(text);
            assert_eq!(found.len(), 1, "{text:?}: {found:?}");
            assert_eq!(found[0].code, INVALID_DIRECTIVE);
            let span = found[0].span.as_ref().unwrap();
            assert_eq!((span.start_line, span.end_line), (line, line));
            assert!(span.cell.is_none() && span.file.is_none());
            let fix = if text.starts_with('\t') {
                "at most three spaces"
            } else {
                "&#58;"
            };
            assert!(found[0].message.contains(fix));
        }
        for text in [
            "```text\n:::note: x\n```", "<div>\n:::note: x\n</div>",
            "A :::b line", "::x", "\\:::note: x",
        ] {
            assert!(unopened(text).is_empty(), "{text:?}");
        }
    }

    #[test]
    fn unopened_candidates_in_all_containers_and_names_are_reported() {
        for text in [
            "> [!NOTE]\n> :::odd: x", "A[^x]\n\n[^x]: :::odd: x",
            "::::outer\n- :::odd: x\n::::", "Some text\n:::123: x",
        ] {
            assert_eq!(unopened(text).len(), 1, "{text:?}");
        }
        let found = unopened("::: {.note} :::\nx\n:::");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("remove the `:::`"));
        let found = unopened("first\n:::one: x\n:::two: x");
        assert_eq!(found.len(), 2);
    }
}
