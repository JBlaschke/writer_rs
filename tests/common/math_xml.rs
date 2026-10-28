#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};

/// A small test-only tree. QNames are preserved; namespace validation belongs
/// to the production parser/validator, so standalone m:/w: fragments work too.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Node {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    pub fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|node| node.name == name)
    }

    /// Pre-order, including self when its QName matches.
    pub fn descendants(&self, name: &str) -> Vec<&Self> {
        let mut found = Vec::new();
        let mut pending = vec![self];
        while let Some(node) = pending.pop() {
            if node.name == name {
                found.push(node);
            }
            pending.extend(node.children.iter().rev());
        }
        found
    }

    pub fn text_content(&self) -> String {
        let mut text = self.text.clone();
        for child in &self.children {
            text.push_str(&child.text_content());
        }
        text
    }
}

fn element(tag: &BytesStart<'_>) -> Node {
    Node {
        name: tag.name().as_ref().to_string(),
        attrs: tag
            .attributes()
            .map(|attribute| {
                let attribute = attribute.unwrap();
                (
                    attribute.key.as_ref().to_string(),
                    attribute
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .unwrap()
                        .into_owned(),
                )
            })
            .collect(),
        children: Vec::new(),
        text: String::new(),
    }
}

fn attach(node: Node, stack: &mut [Node], root: &mut Option<Node>) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        assert!(root.replace(node).is_none(), "multiple XML roots");
    }
}

pub fn parse(source: &str) -> Node {
    let mut reader = quick_xml::Reader::from_str(source);
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    loop {
        match reader.read_event().unwrap() {
            Event::Start(tag) => stack.push(element(&tag)),
            Event::Empty(tag) => attach(element(&tag), &mut stack, &mut root),
            Event::End(tag) => {
                let node = stack.pop().expect("opening tag");
                assert_eq!(node.name, tag.name().as_ref());
                attach(node, &mut stack, &mut root);
            }
            Event::Text(text) => {
                if let Some(parent) = stack.last_mut() {
                    parent.text.push_str(&text.xml10_content());
                }
            }
            Event::CData(text) => {
                stack.last_mut().unwrap().text.push_str(&text.xml10_content());
            }
            Event::GeneralRef(reference) => {
                let character = match reference.as_ref() {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    _ => reference.resolve_char_ref().unwrap().unwrap(),
                };
                stack.last_mut().unwrap().text.push(character);
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) => {}
            other => panic!("unexpected valid-output XML event: {other:?}"),
        }
    }
    assert!(stack.is_empty(), "unclosed XML element");
    root.expect("XML root")
}

pub fn xml_pretty(source: &str) -> String {
    let mut reader = quick_xml::Reader::from_str(source);
    let mut writer = quick_xml::Writer::new_with_indent(Vec::new(), b' ', 4);
    loop {
        let event = reader.read_event().unwrap();
        if matches!(event, Event::Eof) {
            break;
        }
        match event {
            Event::GeneralRef(reference) => {
                // Indented Writer otherwise treats a reference as a new node,
                // adding whitespace inside the equation's text content.
                let escaped = format!("&{};", reference.as_ref());
                writer
                    .write_event(Event::Text(
                        quick_xml::events::BytesText::from_escaped(escaped),
                    ))
                    .unwrap();
            }
            event => writer.write_event(event).unwrap(),
        }
    }
    String::from_utf8(writer.into_inner()).unwrap()
}

pub fn zip_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut part = archive.by_index(index).unwrap();
            let name = part.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            part.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}

pub fn part(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8(parts[name].clone()).unwrap()
}
