//! Document asset registration and HTML emission (html-output.md §6).
//! Owner: T14. Module skeleton created by T00.
//!
//! The registry records files and bytes; the emitter resolves their placeholders.
//! The HTML tokenizer is shared with the Markdown frontend (T12).

use std::convert::Infallible;
use std::ops::Range;

pub mod emit;
pub(crate) mod export;
pub mod registry;

/// Every start tag outside comments and raw-text elements, in order: its
/// ASCII-lowercased name and byte offset just after the tag. The Markdown
/// frontend (T12) shares this tokenizer when registering raw HTML assets.
pub(crate) fn html_start_tags(html: &str) -> Vec<(String, usize)> {
    let mut tags = Vec::new();
    let Ok(()) = HtmlScan::default().start_tags(html, |tag| {
        tags.push((tag.name.clone(), tag.end));
        Ok::<(), Infallible>(())
    });
    tags
}

/// Every attribute value of the start tags [`html_start_tags`] finds, in order: the
/// attribute's ASCII-lowercased name and the byte range of its value (quotes excluded,
/// entities not decoded; bare attributes are left out). The Markdown frontend (T12)
/// resolves local URLs with it.
pub(crate) fn html_attr_values(html: &str) -> Vec<(String, Range<usize>)> {
    let mut values = Vec::new();
    let Ok(()) = HtmlScan::default().start_tags(html, |tag| {
        let valued = tag
            .attrs
            .iter()
            .filter_map(|attr| Some((attr.name.clone(), attr.value.clone()?)));
        values.extend(valued);
        Ok::<(), Infallible>(())
    });
    values
}

/// Number of start tags with `attr`, outside comments and raw-text elements.
/// Repeated attributes on one tag count once; names are ASCII case-insensitive.
#[allow(
    dead_code,
    reason = "html::document checks the morph root with it (T15)"
)]
pub(crate) fn html_count_attr(html: &str, attr: &str) -> usize {
    let attr = attr.to_ascii_lowercase();
    let mut count = 0;
    let Ok(()) = HtmlScan::default().start_tags(html, |tag| {
        count += usize::from(tag.attrs.iter().any(|a| a.name == attr));
        Ok::<(), Infallible>(())
    });
    count
}

/// Elements whose content is raw text (or escapable raw text) for the HTML tokenizer: tags
/// inside them are not tags.
const RAW_TEXT_ELEMENTS: [&str; 8] = [
    "script", "style", "textarea", "title", "xmp", "iframe", "noembed",
    "noframes",
];

/// Where an HTML scan stands between snippets. The raw HTML nodes of a document are scanned
/// one after the other, and a comment or `<script>` opened in one can close in a later one
/// (`x <script>…'<img src=a.png>'…</script>` is three inline HTML nodes).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HtmlScan {
    /// Ordinary content: tags are recognized.
    #[default]
    Data,
    /// Inside `<!-- … -->`.
    Comment,
    /// Inside a raw-text element (one of [`RAW_TEXT_ELEMENTS`]) until its end tag.
    RawText(&'static str),
}

/// A start tag found by [`HtmlScan::start_tags`].
#[derive(Debug)]
pub(crate) struct StartTag {
    /// The tag name, ASCII-lowercased.
    pub(crate) name: String,
    /// Its attributes in order.
    pub(crate) attrs: Vec<TagAttr>,
    /// Byte offset just after the tag's `>` (or the end of the snippet).
    pub(crate) end: usize,
}

/// One attribute of a [`StartTag`].
#[derive(Debug)]
pub(crate) struct TagAttr {
    /// The name, ASCII-lowercased.
    pub(crate) name: String,
    /// Byte range of the value in the snippet, quotes excluded; `None` for a bare attribute.
    pub(crate) value: Option<Range<usize>>,
}

impl HtmlScan {
    /// Calls `on_tag` for every start tag of `html` that is outside comments and raw-text
    /// elements, then leaves `self` in the state the snippet ends in. End tags, `<!…>`
    /// declarations and `<?…>` are skipped; a `<` that starts none of these is text.
    pub(crate) fn start_tags<E>(
        &mut self, html: &str,
        mut on_tag: impl FnMut(&StartTag) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        let bytes = html.as_bytes();
        let mut at = 0;
        while at < html.len() {
            match *self {
                HtmlScan::Comment => match html[at..].find("-->") {
                    Some(end) => {
                        at += end + "-->".len();
                        *self = HtmlScan::Data;
                    }
                    None => break,
                },
                HtmlScan::RawText(name) => match raw_text_end(html, at, name) {
                    Some(end) => {
                        at = end;
                        *self = HtmlScan::Data;
                    }
                    None => break,
                },
                HtmlScan::Data => {
                    let Some(lt) = html[at..].find('<') else {
                        break;
                    };
                    at += lt;
                    let rest = &html[at..];
                    if let Some(empty) = ["<!-->", "<!--->"]
                        .iter()
                        .find(|c| rest.starts_with(**c))
                    {
                        at += empty.len();
                    } else if rest.starts_with("<!--") {
                        at += "<!--".len();
                        *self = HtmlScan::Comment;
                    } else if bytes
                        .get(at + 1)
                        .is_some_and(u8::is_ascii_alphabetic)
                    {
                        let tag = start_tag(html, at);
                        on_tag(&tag)?;
                        if let Some(raw) = RAW_TEXT_ELEMENTS
                            .iter()
                            .find(|raw| **raw == tag.name)
                        {
                            *self = HtmlScan::RawText(raw);
                        }
                        at = tag.end;
                    } else if matches!(
                        bytes.get(at + 1),
                        Some(b'/' | b'!' | b'?')
                    ) {
                        at = html[at..]
                            .find('>')
                            .map_or(html.len(), |gt| at + gt + 1);
                    } else {
                        at += 1;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Byte offset of the `</name` that ends a raw-text element (searching from `from`).
fn raw_text_end(html: &str, from: usize, name: &str) -> Option<usize> {
    let bytes = html.as_bytes();
    html[from..].match_indices("</").map(|(offset, _)| from + offset).find(
        |&lt| {
            let after = &bytes[lt + 2..];
            after.len() >= name.len()
                && after[..name.len()].eq_ignore_ascii_case(name.as_bytes())
                && after.get(name.len()).is_none_or(|b| {
                    b.is_ascii_whitespace() || matches!(b, b'/' | b'>')
                })
        },
    )
}

/// Tokenizes the start tag whose `<` is at byte `lt` (HTML tokenizer rules: attribute names
/// end at whitespace, `/`, `>` or `=`; a value is quoted or runs to whitespace or `>`).
fn start_tag(html: &str, lt: usize) -> StartTag {
    let bytes = html.as_bytes();
    let ends_name = |b: u8| b.is_ascii_whitespace() || matches!(b, b'/' | b'>');
    let mut at = lt + 1;
    while at < bytes.len() && !ends_name(bytes[at]) {
        at += 1;
    }
    let name = html[lt + 1..at].to_ascii_lowercase();
    let mut attrs = Vec::new();
    loop {
        while at < bytes.len()
            && (bytes[at].is_ascii_whitespace() || bytes[at] == b'/')
        {
            at += 1;
        }
        match bytes.get(at) {
            None => {
                return StartTag { name, attrs, end: at };
            }
            Some(b'>') => {
                return StartTag { name, attrs, end: at + 1 };
            }
            Some(_) => {}
        }
        let name_start = at;
        at += 1; // the first character of a name may be `=`
        while at < bytes.len() && !ends_name(bytes[at]) && bytes[at] != b'=' {
            at += 1;
        }
        let attr_name = html[name_start..at].to_ascii_lowercase();
        let (value, next) = attr_value(bytes, at);
        at = next;
        attrs.push(TagAttr { name: attr_name, value });
    }
}

/// The value after an attribute name that ends at byte `at`: its byte range (quotes
/// excluded) when `=` follows, and where tokenizing continues.
fn attr_value(bytes: &[u8], at: usize) -> (Option<Range<usize>>, usize) {
    let skip_space = |mut i: usize| {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    };
    let eq = skip_space(at);
    if bytes.get(eq) != Some(&b'=') {
        return (None, at);
    }
    let start = skip_space(eq + 1);
    match bytes.get(start) {
        Some(&quote @ (b'"' | b'\'')) => {
            let end = bytes[start + 1..]
                .iter()
                .position(|&b| b == quote)
                .map_or(bytes.len(), |p| start + 1 + p);
            (Some(start + 1..end), (end + 1).min(bytes.len()))
        }
        _ => {
            let end = bytes[start..]
                .iter()
                .position(|&b| b.is_ascii_whitespace() || b == b'>')
                .map_or(bytes.len(), |p| start + p);
            (Some(start..end), end)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_tags_follow_the_html_tokenizer() {
        let tags = |snippets: &[&str]| {
            let mut scan = HtmlScan::default();
            let mut found: Vec<String> = Vec::new();
            for html in snippets {
                let Ok(()) = scan.start_tags(html, |tag| {
                    let attrs: Vec<String> = tag
                        .attrs
                        .iter()
                        .map(|a| match &a.value {
                            Some(range) => {
                                format!("{}={}", a.name, &html[range.clone()])
                            }
                            None => a.name.clone(),
                        })
                        .collect();
                    found.push(format!("{}[{}]", tag.name, attrs.join(" ")));
                    Ok::<(), Infallible>(())
                });
            }
            (found, scan)
        };
        let (found, scan) = tags(&[
            "<A HREF = 'x y' Data-K=v/ bare =odd>",
            "<!--> <b x=1> <!---> <i y=2> <!-- <c> -->",
            "</p x=1> <!DOCTYPE html> <?pi <d>?> a < b <e f=\"1>2\" g=é>",
            "<script type=module>if (a<b) { '<f src=1>' }</SCRIPT ><g>",
            "<p>unclosed <!-- <h>",
            "--> <i src=\"unterminated",
        ]);
        assert_eq!(
            found,
            [
                "a[href=x y data-k=v/ bare=odd]", "b[x=1]", "i[y=2]",
                "e[f=1>2 g=é]", "script[type=module]", "g[]", "p[]",
                "i[src=unterminated]",
            ]
        );
        assert_eq!(scan, HtmlScan::Data);
        assert_eq!(tags(&["<style>a<b>"]).1, HtmlScan::RawText("style"));
        assert_eq!(tags(&["x <!-- y"]).1, HtmlScan::Comment);
    }

    #[test]
    fn html_count_attr_counts_start_tags_outside_text() {
        for html in [
            "<main data-writer-document>\n<p>x</p>\n</main>\n",
            "<main data-writer-document=\"\">",
            "<MAIN DATA-WRITER-DOCUMENT>",
            "<main data-writer-document data-writer-document>",
        ] {
            assert_eq!(
                html_count_attr(html, "data-writer-document"),
                1,
                "{html}"
            );
            assert_eq!(
                html_count_attr(html, "DATA-WRITER-DOCUMENT"),
                1,
                "{html}"
            );
        }
        let two =
            "<main data-writer-document></main><main data-writer-document>";
        assert_eq!(html_count_attr(two, "data-writer-document"), 2);
        for html in [
            "<!-- <main data-writer-document> -->",
            "<script>'<main data-writer-document>'</script>",
            "<p title=\" data-writer-document=x\">t</p>",
            "<code>&lt;main data-writer-document&gt;</code>",
        ] {
            assert_eq!(
                html_count_attr(html, "data-writer-document"),
                0,
                "{html}"
            );
        }
    }

    #[test]
    fn html_attr_values_skip_bare_attributes() {
        assert_eq!(
            html_attr_values("<p hidden id=a>"),
            [("id".into(), 13..14)]
        );
    }
}
