//! A bounded, non-evaluating MathML tree with ordered mixed content.

use std::collections::{BTreeMap, BTreeSet};

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::math::latex_mathml::MathError;

const MATHML: &str = "http://www.w3.org/1998/Math/MathML";
const XML: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_NODES: usize = 100_000;
const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone)]
pub(super) enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone)]
pub(super) struct Element {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<Node>,
    pub source: String,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|child| match child {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    pub fn text(&self) -> String {
        let mut output = String::new();
        let mut pending = self.children.iter().rev().collect::<Vec<_>>();
        while let Some(node) = pending.pop() {
            match node {
                Node::Element(element) => {
                    pending.extend(element.children.iter().rev());
                }
                Node::Text(text) => output.push_str(text),
            }
        }
        output
    }
}

struct Frame {
    element: Element,
    qualified: String,
    namespaces: BTreeMap<String, String>,
    start: usize,
}

pub(super) fn parse(source: &str) -> Result<Element, MathError> {
    if source.len() > MAX_BYTES {
        return Err(unsupported("input limit", source));
    }
    let mut reader = Reader::from_str(source);
    reader.config_mut().check_comments = true;
    let mut stack = Vec::new();
    let mut root = None;
    let mut nodes = 0;
    let mut declared = false;
    loop {
        let start = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|_| unsupported("XML", &source[start..]))?;
        let end = reader.buffer_position() as usize;
        let excerpt = &source[start..end];
        match event {
            Event::Start(tag) | Event::Empty(tag) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(unsupported("depth limit", excerpt));
                }
                count_node(&mut nodes, excerpt)?;
                if stack.is_empty() && root.is_some() {
                    return Err(unsupported("multiple roots", excerpt));
                }
                let frame = element(&tag, start, &stack, excerpt)?;
                if stack.is_empty() && frame.element.name != "math" {
                    return Err(unsupported(&frame.element.name, excerpt));
                }
                if excerpt.ends_with("/>") {
                    let mut element = frame.element;
                    element.source = detail(excerpt);
                    attach(element, &mut stack, &mut root)?;
                } else {
                    stack.push(frame);
                }
            }
            Event::End(tag) => {
                let Some(mut frame) = stack.pop() else {
                    return Err(unsupported("XML", excerpt));
                };
                if frame.qualified != tag.name().as_ref() {
                    return Err(unsupported("XML", excerpt));
                }
                frame.element.source = detail(&source[frame.start..end]);
                attach(frame.element, &mut stack, &mut root)?;
            }
            Event::Text(text) => {
                if text.as_ref().contains("]]>") {
                    return Err(unsupported("XML", excerpt));
                }
                append_text(text.as_ref(), &mut stack, &mut nodes, excerpt)?;
            }
            Event::CData(text) => {
                if stack.is_empty() {
                    return Err(unsupported("XML", excerpt));
                }
                append_text(text.as_ref(), &mut stack, &mut nodes, excerpt)?;
            }
            Event::GeneralRef(reference) => {
                let name = reference.as_ref();
                let value = entity(name).ok_or_else(|| {
                    unsupported(&format!("entity &{name};"), excerpt)
                })?;
                if stack.is_empty() {
                    return Err(unsupported("XML", excerpt));
                }
                let mut bytes = [0; 4];
                append_text(
                    value.encode_utf8(&mut bytes),
                    &mut stack,
                    &mut nodes,
                    excerpt,
                )?;
            }
            Event::Decl(declaration) => {
                if declared
                    || !stack.is_empty()
                    || root.is_some()
                    || !source[..start]
                        .trim_start_matches('\u{feff}')
                        .is_empty()
                    || declaration.xml_version().ok()
                        != Some(XmlVersion::Explicit1_0)
                {
                    return Err(unsupported("XML declaration", excerpt));
                }
                declaration_attributes(declaration.as_ref(), excerpt)?;
                declared = true;
            }
            Event::DocType(_) => return Err(unsupported("DOCTYPE", excerpt)),
            Event::Comment(comment) => {
                if !comment.as_ref().chars().all(xml_char) {
                    return Err(unsupported("XML", excerpt));
                }
            }
            Event::PI(instruction) => {
                if instruction.target().eq_ignore_ascii_case("xml")
                    || qname(instruction.target()).is_none()
                    || !instruction.as_ref().chars().all(xml_char)
                {
                    return Err(unsupported("XML", excerpt));
                }
            }
            Event::Eof => break,
        }
    }
    if !stack.is_empty() {
        return Err(unsupported("XML", source));
    }
    root.ok_or_else(|| unsupported("math", source))
}

fn element(
    tag: &BytesStart<'_>, start: usize, stack: &[Frame], source: &str,
) -> Result<Frame, MathError> {
    let mut attrs: BTreeMap<String, String> = BTreeMap::new();
    let mut namespaces = BTreeMap::new();
    for attr in tag.attributes().with_checks(true) {
        let attr = attr.map_err(|_| unsupported("XML attribute", source))?;
        let name = attr.key.as_ref();
        if attr.value.contains('<') {
            return Err(unsupported("XML attribute", source));
        }
        check_entities(attr.value.as_ref(), source)?;
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|_| unsupported("XML attribute", source))?
            .into_owned();
        if !value.chars().all(xml_char) {
            return Err(unsupported("XML attribute", source));
        }
        if name == "xmlns" {
            if matches!(value.as_str(), XML | XMLNS) {
                return Err(unsupported("namespace", source));
            }
            namespaces.insert(String::new(), value);
        } else if let Some(prefix) = name.strip_prefix("xmlns:") {
            if !ncname(prefix)
                || prefix == "xmlns"
                || value.is_empty()
                || value == XMLNS
                || (prefix == "xml") != (value == XML)
            {
                return Err(unsupported("namespace", source));
            }
            namespaces.insert(prefix.into(), value);
        } else {
            attrs.insert(name.into(), value);
        }
    }
    let tag_name = tag.name();
    let qualified = tag_name.as_ref();
    let (prefix, name) =
        qname(qualified).ok_or_else(|| unsupported("XML name", source))?;
    let uri = namespace(prefix, &namespaces, stack)
        .ok_or_else(|| unsupported("namespace", source))?;
    if !matches!(uri, "" | MATHML) || prefix == "xmlns" {
        return Err(unsupported(&format!("namespace {uri}"), source));
    }
    let mut expanded = BTreeSet::new();
    for key in attrs.keys() {
        let (prefix, local) =
            qname(key).ok_or_else(|| unsupported("XML attribute", source))?;
        let uri = if prefix.is_empty() {
            ""
        } else {
            namespace(prefix, &namespaces, stack)
                .ok_or_else(|| unsupported("namespace", source))?
        };
        if prefix == "xmlns" || !expanded.insert((uri, local)) {
            return Err(unsupported("XML attribute", source));
        }
    }
    Ok(Frame {
        element: Element {
            name: name.into(),
            attrs,
            children: Vec::new(),
            source: String::new(),
        },
        qualified: qualified.into(),
        namespaces,
        start,
    })
}

fn namespace<'a>(
    prefix: &str, local: &'a BTreeMap<String, String>, stack: &'a [Frame],
) -> Option<&'a str> {
    if prefix == "xml" {
        return Some(XML);
    }
    local
        .get(prefix)
        .or_else(|| {
            stack.iter().rev().find_map(|frame| frame.namespaces.get(prefix))
        })
        .map(String::as_str)
        .or_else(|| prefix.is_empty().then_some(""))
}

fn declaration_attributes(
    content: &str, source: &str,
) -> Result<(), MathError> {
    let tag = BytesStart::from_content(content, 3);
    let mut previous = None;
    for attribute in tag.attributes().with_checks(true) {
        let attribute =
            attribute.map_err(|_| unsupported("XML declaration", source))?;
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|_| unsupported("XML declaration", source))?;
        let rank = match attribute.key.as_ref() {
            "version" if value == "1.0" => 0,
            "encoding"
                if value.eq_ignore_ascii_case("utf-8")
                    || value.eq_ignore_ascii_case("utf8") =>
            {
                1
            }
            "standalone" if matches!(value.as_ref(), "yes" | "no") => 2,
            _ => return Err(unsupported("XML declaration", source)),
        };
        if previous.is_some_and(|previous| rank <= previous)
            || (previous.is_none() && rank != 0)
            || attribute.value.contains(['&', '<'])
        {
            return Err(unsupported("XML declaration", source));
        }
        previous = Some(rank);
    }
    if previous.is_none() {
        return Err(unsupported("XML declaration", source));
    }
    Ok(())
}

fn attach(
    element: Element, stack: &mut [Frame], root: &mut Option<Element>,
) -> Result<(), MathError> {
    if let Some(parent) = stack.last_mut() {
        parent.element.children.push(Node::Element(element));
    } else if root.is_none() {
        *root = Some(element);
    } else {
        return Err(unsupported("multiple roots", &element.source));
    }
    Ok(())
}

fn append_text(
    text: &str, stack: &mut [Frame], nodes: &mut usize, source: &str,
) -> Result<(), MathError> {
    if !text.chars().all(xml_char) {
        return Err(unsupported("XML character", source));
    }
    if text.is_empty() {
        return Ok(());
    }
    let Some(parent) = stack.last_mut() else {
        return if text.chars().all(xml_space) {
            Ok(())
        } else {
            Err(unsupported("XML", source))
        };
    };
    if let Some(Node::Text(previous)) = parent.element.children.last_mut() {
        previous.push_str(text);
    } else {
        count_node(nodes, source)?;
        parent.element.children.push(Node::Text(text.into()));
    }
    Ok(())
}

fn count_node(nodes: &mut usize, source: &str) -> Result<(), MathError> {
    *nodes += 1;
    if *nodes > MAX_NODES {
        Err(unsupported("node limit", source))
    } else {
        Ok(())
    }
}

fn check_entities(value: &str, source: &str) -> Result<(), MathError> {
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find(';') else {
            return Err(unsupported("XML entity", source));
        };
        let name = &rest[..end];
        if entity(name).is_none() {
            return Err(unsupported(&format!("entity &{name};"), source));
        }
        rest = &rest[end + 1..];
    }
    Ok(())
}

fn entity(name: &str) -> Option<char> {
    let value = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "apos" => '\'',
        "quot" => '"',
        _ => {
            let digits = name.strip_prefix('#')?;
            let code = if let Some(hex) = digits.strip_prefix('x') {
                if hex.is_empty()
                    || !hex.bytes().all(|ch| ch.is_ascii_hexdigit())
                {
                    return None;
                }
                u32::from_str_radix(hex, 16).ok()?
            } else {
                if digits.is_empty()
                    || !digits.bytes().all(|ch| ch.is_ascii_digit())
                {
                    return None;
                }
                digits.parse::<u32>().ok()?
            };
            char::from_u32(code)?
        }
    };
    xml_char(value).then_some(value)
}

fn qname(value: &str) -> Option<(&str, &str)> {
    match value.split_once(':') {
        Some((prefix, local)) => {
            (ncname(prefix) && ncname(local)).then_some((prefix, local))
        }
        None => ncname(value).then_some(("", value)),
    }
}

fn ncname(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(name_start)
        && chars.all(|ch| {
            name_start(ch)
                || matches!(ch, '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}')
        })
}

fn name_start(value: char) -> bool {
    matches!(value,
        'A'..='Z' | '_' | 'a'..='z' | '\u{c0}'..='\u{d6}'
        | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}'
        | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}'
        | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}'
        | '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}'
        | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}'
        | '\u{10000}'..='\u{effff}'
    )
}

fn xml_space(value: char) -> bool {
    matches!(value, ' ' | '\t' | '\r' | '\n')
}

fn xml_char(value: char) -> bool {
    matches!(value, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}'
        | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn detail(source: &str) -> String {
    source.chars().take(80).collect()
}

fn unsupported(element: &str, source: &str) -> MathError {
    MathError::Unsupported { element: element.into(), detail: detail(source) }
}

/// Recover visible content even when the XML tree cannot be accepted.
pub(super) fn loose_text(source: &str) -> String {
    let mut output = String::new();
    let mut offset = 0;
    while offset < source.len() {
        let rest = &source[offset..];
        if let Some(content) = rest.strip_prefix("<![CDATA[") {
            let end = content.find("]]>").unwrap_or(content.len());
            output.push_str(&content[..end]);
            offset += 9 + end + usize::from(end < content.len()) * 3;
        } else if let Some(content) = rest.strip_prefix("<!--") {
            offset += content.find("-->").map_or(rest.len(), |end| 4 + end + 3);
        } else if let Some(content) = rest.strip_prefix("<?") {
            offset += content.find("?>").map_or(rest.len(), |end| 2 + end + 2);
        } else if rest.starts_with("<!") {
            offset += markup_end(rest, true).unwrap_or(rest.len());
        } else if rest.starts_with('<')
            && rest[1..]
                .trim_start_matches('/')
                .chars()
                .next()
                .is_some_and(name_start)
        {
            offset += markup_end(rest, false).unwrap_or(rest.len());
        } else if let Some(content) = rest.strip_prefix('&') {
            let end = content.find(|ch: char| {
                ch == ';' || ch == '<' || ch == '&' || ch.is_whitespace()
            });
            if let Some(end) =
                end.filter(|end| content.as_bytes()[*end] == b';')
            {
                let name = &content[..end];
                match entity(name) {
                    Some(value) => output.push(value),
                    None => output.push_str(&rest[..end + 2]),
                }
                offset += end + 2;
            } else {
                output.push('&');
                offset += 1;
            }
        } else {
            let value =
                rest.chars().next().expect("offset precedes a character");
            output.push(value);
            offset += value.len_utf8();
        }
    }
    output
}

fn markup_end(source: &str, declaration: bool) -> Option<usize> {
    let mut quote = None;
    let mut brackets: usize = 0;
    let mut offset = 0;
    while offset < source.len() {
        if declaration
            && quote.is_none()
            && let Some(content) = source[offset..].strip_prefix("<!--")
        {
            offset += 4 + content.find("-->")? + 3;
            continue;
        }
        let value = source[offset..].chars().next()?;
        if let Some(current) = quote {
            if value == current {
                quote = None;
            }
        } else {
            match value {
                '\'' | '"' => quote = Some(value),
                '[' if declaration => brackets += 1,
                ']' if declaration => brackets = brackets.saturating_sub(1),
                '>' if brackets == 0 => return Some(offset + 1),
                _ => {}
            }
        }
        offset += value.len_utf8();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_entities_cdata_and_mixed_content_preserve_source_order() {
        let source = concat!(
            "<q:math xmlns:q=\"http://www.w3.org/1998/Math/MathML\">",
            "before<q:mtext xml:space=\"preserve\"> x&amp;<![CDATA[y<z]]>",
            "&#945;&#x3B2; </q:mtext>after</q:math>",
        );
        let root = parse(source).unwrap();
        assert_eq!(root.name, "math");
        assert_eq!(root.text(), "before x&y<zαβ after");
        assert_eq!(
            root.elements().next().unwrap().attr("xml:space"),
            Some("preserve")
        );
        assert_eq!(root.source.chars().count(), 80);
        assert!(
            matches!(&root.children[0], Node::Text(text) if text == "before")
        );
        assert!(
            matches!(&root.children[2], Node::Text(text) if text == "after")
        );
    }

    #[test]
    fn foreign_namespaces_entities_dtd_and_malformed_xml_are_rejected() {
        for source in [
            "<math xmlns=\"urn:foreign\"/>",
            "<math><x:mi>x</x:mi></math>",
            "<math><mi xmlns=\"urn:foreign\">x</mi></math>",
            "<math><mi>&ApplyFunction;</mi></math>",
            "<math><mi a=\"&unknown;\">x</mi></math>",
            "<!DOCTYPE math [<!ENTITY x 'expanded'>]><math>&x;</math>",
            "<math><mi>x</math>",
            "<math/><math/>",
            "text<math/>",
            "<math><mi a=\"1\" a=\"2\"/></math>",
            "<math xmlns:a=\"urn:a\" xmlns:b=\"urn:a\"><mi a:x=\"1\" b:x=\"2\"/></math>",
            "<:math/>",
            "<math><mi>&#0;</mi></math>",
        ] {
            let MathError::Unsupported { detail, .. } =
                parse(source).unwrap_err()
            else {
                panic!("wrong error for {source}");
            };
            assert!(detail.chars().count() <= 80);
        }
        assert!(parse("<math/>").is_ok());
        assert!(parse("<?xml version=\"1.0\"?><math/>").is_ok());
    }

    #[test]
    fn recovery_keeps_unknown_entities_and_ignores_non_content_markup() {
        let source = concat!(
            "<!DOCTYPE math [<!ENTITY x 'not expanded'>]>",
            "<math> a&lt;<!--hidden--><mi>𝑥</mi>",
            "<![CDATA[<b>]]>&#x3B1;&unknown;<?p hidden?></wrong>",
        );
        assert_eq!(loose_text(source), " a<𝑥<b>α&unknown;");
        assert_eq!(loose_text("<math>x & y<mi>z"), "x & yz");
        assert_eq!(
            loose_text("<!DOCTYPE math [<!-- ] > -->]><math>x</math>"),
            "x",
        );
    }

    #[test]
    fn input_node_and_depth_limits_are_checked_before_recursive_work() {
        assert!(parse(&" ".repeat(MAX_BYTES + 1)).is_err());
        let deep = format!(
            "{}{}",
            "<math>".repeat(MAX_DEPTH + 1),
            "</math>".repeat(MAX_DEPTH + 1)
        );
        assert!(
            matches!(parse(&deep), Err(MathError::Unsupported { element, .. }) if element == "depth limit")
        );
        let many = format!("<math>{}</math>", "<mi/>".repeat(MAX_NODES));
        assert!(
            matches!(parse(&many), Err(MathError::Unsupported { element, .. }) if element == "node limit")
        );
    }
}
