//! DOCX structural validation (docx-output.md §7.1).
//! Owner: T04. Stub created by T00.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::ops::Range;

use quick_xml::{
    Reader, XmlVersion,
    events::{BytesDecl, BytesRef, BytesStart, Event},
};

use super::xml;

/// A structural error in an OPC package or one of its XML parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Stable validator rule, `V01` through `V20`.
    pub rule: &'static str,
    /// Package-relative part path, or an empty string for the ZIP itself.
    pub part: String,
    /// Description of the failed invariant.
    pub message: String,
}

/// Structural validation settings.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidateOptions {
    /// Reject theme attributes in writer's built-in styles.
    pub builtin_styles: bool,
}

/// Small XML tree shared with reference/style readers; ranges preserve raw XML.
#[derive(Debug, Clone)]
pub(crate) struct XmlNode {
    pub(crate) name: String,
    pub(crate) attrs: BTreeMap<String, String>,
    pub(crate) children: Vec<XmlNode>,
    pub(crate) text: String,
    pub(crate) range: Range<usize>,
    pub(crate) open_end: usize,
    pub(crate) close_start: usize,
}

impl XmlNode {
    pub(crate) fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|node| node.name == name)
    }

    fn descendants(&self) -> Vec<&Self> {
        let mut result = Vec::new();
        let mut pending = vec![self];
        while let Some(node) = pending.pop() {
            result.push(node);
            pending.extend(node.children.iter().rev());
        }
        result
    }
}

/// Parse XML with the well-formedness checks quick-xml leaves to its caller.
pub(crate) fn parse_xml(source: &str) -> Result<XmlNode, String> {
    let mut reader = Reader::from_str(source);
    let mut stack: Vec<(XmlNode, BTreeMap<String, String>)> = Vec::new();
    let mut root = None;
    loop {
        let start = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|error| error.to_string())?;
        let end = reader.buffer_position() as usize;
        match event {
            Event::Start(tag) | Event::Empty(tag) => {
                let start = start + source[start..end].find('<').unwrap_or(0);
                let empty = source[start..end].ends_with("/>");
                let inherited = stack.last().map(|(_, ns)| ns);
                let (node, namespaces) =
                    xml_element(&tag, start..end, inherited)?;
                if stack.is_empty() && root.is_some() {
                    return Err("more than one root element".into());
                }
                if empty {
                    attach_node(node, &mut stack, &mut root)?;
                } else {
                    stack.push((node, namespaces));
                }
            }
            Event::End(tag) => {
                let Some((mut node, _)) = stack.pop() else {
                    return Err("end tag without an opening tag".into());
                };
                if node.name != tag.name().as_ref() {
                    return Err("mismatched opening and closing tags".into());
                }
                node.range.end = end;
                node.close_start = start;
                attach_node(node, &mut stack, &mut root)?;
            }
            Event::Text(text) => {
                if text.as_ref().contains("]]>") {
                    return Err("CDATA terminator in character data".into());
                }
                append_text(text.as_ref(), &mut stack)?;
            }
            Event::CData(text) => {
                if stack.is_empty() {
                    return Err("CDATA outside the root element".into());
                }
                append_text(text.as_ref(), &mut stack)?;
            }
            Event::GeneralRef(reference) => {
                let value = xml_reference(&reference)?;
                if stack.is_empty() {
                    return Err("entity reference outside root element".into());
                }
                let mut utf8 = [0; 4];
                append_text(value.encode_utf8(&mut utf8), &mut stack)?;
            }
            Event::Comment(text) => {
                if !text.as_ref().chars().all(xml::is_xml_char)
                    || text.as_ref().contains("--")
                {
                    return Err("invalid XML comment".into());
                }
            }
            Event::PI(text) => {
                if !text.as_ref().chars().all(xml::is_xml_char)
                    || text.target().eq_ignore_ascii_case("xml")
                    || !valid_xml_name(&text.target().replace(':', "_"))
                {
                    return Err("invalid XML processing instruction".into());
                }
            }
            Event::Decl(decl) if root.is_none() && stack.is_empty() => {
                xml_declaration(source, start, &decl)?;
            }
            Event::Decl(_) | Event::DocType(_) => {
                return Err("misplaced declaration".into());
            }
            Event::Eof => break,
        }
    }
    if !stack.is_empty() {
        return Err("unclosed element".into());
    }
    root.ok_or_else(|| "missing root element".into())
}

fn xml_reference(reference: &BytesRef<'_>) -> Result<char, String> {
    let value = match reference.as_ref() {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => reference
            .resolve_char_ref()
            .map_err(|error| error.to_string())?
            .ok_or("unknown entity reference")?,
    };
    if !xml::is_xml_char(value) {
        return Err("invalid XML 1.0 character reference".into());
    }
    Ok(value)
}

fn xml_declaration(
    source: &str, start: usize, decl: &BytesDecl<'_>,
) -> Result<(), String> {
    if !source[..start].trim_start_matches('\u{feff}').is_empty()
        || decl.xml_version().map_err(|error| error.to_string())?
            != XmlVersion::Explicit1_0
    {
        return Err("invalid or misplaced XML 1.0 declaration".into());
    }
    let tag = BytesStart::from_content(decl.as_ref(), 3);
    let mut previous = 0;
    for attr in tag.attributes().with_checks(true) {
        let attr = attr.map_err(|error| error.to_string())?;
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| error.to_string())?;
        let rank = match attr.key.as_ref() {
            "version" => 0,
            "encoding" => 1,
            "standalone" => 2,
            _ => return Err("unknown XML declaration attribute".into()),
        };
        let valid_encoding =
            value.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                && value.bytes().all(|ch| {
                    ch.is_ascii_alphanumeric()
                        || matches!(ch, b'.' | b'_' | b'-')
                });
        if rank < previous
            || attr.value.contains(['&', '<'])
            || (attr.key.as_ref() == "encoding" && !valid_encoding)
            || (attr.key.as_ref() == "standalone"
                && !matches!(value.as_ref(), "yes" | "no"))
        {
            return Err("invalid XML declaration attribute".into());
        }
        previous = rank;
    }
    Ok(())
}

fn attach_node(
    node: XmlNode, stack: &mut [(XmlNode, BTreeMap<String, String>)],
    root: &mut Option<XmlNode>,
) -> Result<(), String> {
    if let Some((parent, _)) = stack.last_mut() {
        parent.children.push(node);
    } else if root.replace(node).is_some() {
        return Err("more than one root element".into());
    }
    Ok(())
}

fn append_text(
    text: &str, stack: &mut [(XmlNode, BTreeMap<String, String>)],
) -> Result<(), String> {
    if !text.chars().all(xml::is_xml_char) {
        return Err("invalid XML 1.0 character".into());
    }
    if let Some((node, _)) = stack.last_mut() {
        node.text.push_str(text);
    } else if !text.chars().all(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n')) {
        return Err("text outside the root element".into());
    }
    Ok(())
}

fn xml_element(
    tag: &BytesStart<'_>, range: Range<usize>,
    inherited: Option<&BTreeMap<String, String>>,
) -> Result<(XmlNode, BTreeMap<String, String>), String> {
    let mut namespaces = inherited.cloned().unwrap_or_else(|| {
        BTreeMap::from([(
            "xml".into(),
            "http://www.w3.org/XML/1998/namespace".into(),
        )])
    });
    let mut attrs = BTreeMap::new();
    for attr in tag.attributes().with_checks(true) {
        let attr = attr.map_err(|error| error.to_string())?;
        let name = attr.key.as_ref();
        if attr.value.contains('<') {
            return Err("unescaped < in attribute value".into());
        }
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| error.to_string())?
            .into_owned();
        if !value.chars().all(xml::is_xml_char) {
            return Err("invalid character in attribute value".into());
        }
        if name == "xmlns" {
            if matches!(
                value.as_str(),
                "http://www.w3.org/XML/1998/namespace"
                    | "http://www.w3.org/2000/xmlns/"
            ) {
                return Err("reserved default namespace".into());
            }
            namespaces.insert(String::new(), value.clone());
        } else if let Some(prefix) = name.strip_prefix("xmlns:") {
            if !valid_xml_name(prefix)
                || value.is_empty()
                || prefix == "xmlns"
                || (prefix == "xml"
                    && value != "http://www.w3.org/XML/1998/namespace")
                || (prefix != "xml"
                    && value == "http://www.w3.org/XML/1998/namespace")
                || value == "http://www.w3.org/2000/xmlns/"
            {
                return Err("invalid namespace declaration".into());
            }
            namespaces.insert(prefix.into(), value.clone());
        }
        attrs.insert(name.to_owned(), value);
    }
    let name = tag.name().as_ref().to_owned();
    check_qname(&name, &namespaces)?;
    let mut expanded = BTreeSet::new();
    for name in attrs
        .keys()
        .filter(|name| name.as_str() != "xmlns" && !name.starts_with("xmlns:"))
    {
        check_qname(name, &namespaces)?;
        let (uri, local) =
            name.split_once(':').map_or(("", name.as_str()), |(p, n)| {
                (namespaces.get(p).map(String::as_str).unwrap_or(""), n)
            });
        if !expanded.insert((uri, local)) {
            return Err("duplicate expanded attribute name".into());
        }
    }
    Ok((
        XmlNode {
            name,
            attrs,
            children: Vec::new(),
            text: String::new(),
            open_end: range.end,
            close_start: range.end,
            range,
        },
        namespaces,
    ))
}

fn check_qname(
    name: &str, namespaces: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut pieces = name.split(':');
    let first = pieces.next().unwrap_or("");
    let second = pieces.next();
    if !valid_xml_name(first)
        || second.is_some_and(|name| !valid_xml_name(name))
        || pieces.next().is_some()
    {
        return Err(format!("invalid qualified name {name:?}"));
    }
    if second.is_some() && !namespaces.contains_key(first) {
        return Err(format!("unbound namespace prefix {first:?}"));
    }
    Ok(())
}

fn valid_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(name_start)
        && chars.all(|ch| {
            name_start(ch)
                || matches!(ch,
                '0'..='9' | '-' | '.' | '\u{b7}'
                | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}')
        })
}

fn name_start(ch: char) -> bool {
    matches!(ch, 'A'..='Z' | '_' | 'a'..='z'
        | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}'
        | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}'
        | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}'
        | '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}'
        | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}'
        | '\u{10000}'..='\u{effff}')
}

/// Check a DOCX package against writer's twenty structural invariants.
///
/// Invalid ZIP/XML input is reported as violations rather than panicking.
pub fn validate(docx: &[u8], opts: &ValidateOptions) -> Vec<Violation> {
    let mut check = Validation::default();
    check.read_zip(docx);
    check.read_xml();
    check.content_types();
    check.relationships();
    check.child_order();
    check.section();
    check.bookmarks_and_fields();
    check.drawing_ids();
    check.footnotes();
    check.numbering();
    check.styles();
    check.tables();
    check.whitespace_and_themes(opts);
    check.media();
    check.settings_and_dates();
    check.violations
}

#[derive(Default)]
struct Validation {
    parts: BTreeMap<String, Vec<u8>>,
    trees: BTreeMap<String, XmlNode>,
    violations: Vec<Violation>,
}

impl Validation {
    fn fail(
        &mut self, rule: &'static str, part: &str, message: impl Into<String>,
    ) {
        self.violations.push(Violation {
            rule,
            part: part.into(),
            message: message.into(),
        });
    }

    fn read_zip(&mut self, bytes: &[u8]) {
        let mut archive = match zip::ZipArchive::new(Cursor::new(bytes)) {
            Ok(archive) => archive,
            Err(error) => {
                self.fail("V01", "", format!("unreadable ZIP: {error}"));
                return;
            }
        };
        if archive.is_empty() {
            self.fail("V01", "", "empty ZIP package");
        }
        // ZipArchive indexes by name and otherwise hides duplicate entries.
        if let Err(error) = archive.validate_header_consistency() {
            self.fail("V01", "", format!("invalid ZIP headers: {error}"));
        }
        for index in 0..archive.len() {
            let mut entry = match archive.by_index(index) {
                Ok(entry) => entry,
                Err(error) => {
                    self.fail(
                        "V01",
                        "",
                        format!("unreadable ZIP entry: {error}"),
                    );
                    continue;
                }
            };
            let name = match entry.name() {
                Ok(name) => name.into_owned(),
                Err(error) => {
                    self.fail(
                        "V01",
                        "",
                        format!("invalid entry name: {error}"),
                    );
                    continue;
                }
            };
            if index == 0 && name != "[Content_Types].xml" {
                self.fail(
                    "V01",
                    &name,
                    "first entry must be [Content_Types].xml",
                );
            }
            if entry.is_dir() {
                self.fail("V01", &name, "directory entry");
            }
            if entry.last_modified() != Some(zip::DateTime::default()) {
                self.fail("V01", &name, "mtime must be 1980-01-01 00:00:00");
            }
            let mut data = Vec::new();
            if let Err(error) = entry.read_to_end(&mut data) {
                self.fail("V01", &name, format!("unreadable part: {error}"));
            }
            if self.parts.insert(name.clone(), data).is_some() {
                self.fail("V01", &name, "duplicate ZIP entry name");
            }
        }
    }

    fn read_xml(&mut self) {
        for (name, bytes) in &self.parts {
            // Copied font/media parts are binary; OPC XML uses .xml or .rels.
            if !(name.ends_with(".xml") || name.ends_with(".rels")) {
                continue;
            }
            let parsed = std::str::from_utf8(bytes)
                .map_err(|error| error.to_string())
                .and_then(parse_xml);
            match parsed {
                Ok(tree) => {
                    self.trees.insert(name.clone(), tree);
                }
                Err(message) => self.violations.push(Violation {
                    rule: "V02",
                    part: name.clone(),
                    message,
                }),
            }
        }
    }

    fn content_types(&mut self) {
        let Some(tree) = self.trees.get("[Content_Types].xml") else {
            if !self.parts.contains_key("[Content_Types].xml") {
                self.fail(
                    "V03", "[Content_Types].xml", "missing content types",
                );
            }
            return;
        };
        let mut defaults = BTreeSet::new();
        let mut overrides = BTreeSet::new();
        let mut errors = Vec::new();
        for node in &tree.children {
            match node.name.as_str() {
                "Default" => {
                    let ext =
                        node.attr("Extension").unwrap_or("").to_lowercase();
                    if ext.is_empty() || !defaults.insert(ext) {
                        errors.push(
                            "missing or duplicate Default extension".into(),
                        );
                    }
                }
                "Override" => {
                    let part = node.attr("PartName").unwrap_or("");
                    let resolved = resolve_target("", part);
                    if !part.starts_with('/')
                        || resolved
                            .as_ref()
                            .is_none_or(|name| !overrides.insert(name.clone()))
                    {
                        errors.push(format!(
                            "invalid or duplicate Override {part:?}"
                        ));
                    }
                    if resolved
                        .as_ref()
                        .is_none_or(|name| !self.parts.contains_key(name))
                    {
                        errors.push(format!(
                            "Override names missing part {part:?}"
                        ));
                    }
                }
                _ => errors.push(format!(
                    "unknown content type element {}",
                    node.name
                )),
            }
        }
        for name in
            self.parts.keys().filter(|name| *name != "[Content_Types].xml")
        {
            if self.content_type(name).is_none() {
                errors.push(format!("no content type for {name}"));
            }
        }
        let main = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
        if self.content_type("word/document.xml") != Some(main) {
            errors.push(
                "/word/document.xml must have the main document type".into(),
            );
        }
        for error in errors {
            self.fail("V03", "[Content_Types].xml", error);
        }
    }

    fn content_type(&self, name: &str) -> Option<&str> {
        let tree = self.trees.get("[Content_Types].xml")?;
        tree.children
            .iter()
            .find(|node| {
                node.name == "Override"
                    && node
                        .attr("PartName")
                        .and_then(|part| resolve_target("", part))
                        .as_deref()
                        == Some(name)
            })
            .or_else(|| {
                let ext = name.rsplit_once('.')?.1;
                tree.children.iter().find(|node| {
                    node.name == "Default"
                        && node.attr("Extension").is_some_and(|value| {
                            value.eq_ignore_ascii_case(ext)
                        })
                })
            })
            .and_then(|node| node.attr("ContentType"))
    }

    fn relationships(&mut self) {
        let mut by_owner: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut root_main = false;
        let mut errors = Vec::new();
        for (part, tree) in &self.trees {
            let Some(owner) = relationship_owner(part) else {
                continue;
            };
            let mut ids = BTreeSet::new();
            for rel in &tree.children {
                if rel.name != "Relationship" {
                    continue;
                }
                let id = rel.attr("Id").unwrap_or("");
                let target = rel.attr("Target").unwrap_or("");
                let kind = rel.attr("Type").unwrap_or("");
                if id.is_empty() || !ids.insert(id.to_owned()) {
                    errors.push((
                        part.clone(),
                        "missing or duplicate relationship Id".into(),
                    ));
                }
                let external = rel.attr("TargetMode") == Some("External");
                if kind.ends_with("/hyperlink") && !external {
                    errors.push((
                        part.clone(),
                        "hyperlink must be External".into(),
                    ));
                }
                if !external {
                    let resolved = resolve_target(&owner, target);
                    if resolved
                        .as_ref()
                        .is_none_or(|name| !self.parts.contains_key(name))
                    {
                        errors.push((
                            part.clone(),
                            format!("missing internal target {target:?}"),
                        ));
                    }
                    if owner.is_empty()
                        && kind == xml::REL_OFFICE_DOCUMENT
                        && resolved.as_deref() == Some("word/document.xml")
                    {
                        root_main = true;
                    }
                }
            }
            by_owner.insert(owner, ids);
        }
        if !root_main {
            self.fail(
                "V04",
                "_rels/.rels",
                "missing relationship to word/document.xml",
            );
        }
        for (part, tree) in &self.trees {
            for node in tree.descendants() {
                for name in ["r:id", "r:embed", "r:link"] {
                    if let Some(id) = node.attr(name)
                        && !by_owner
                            .get(part)
                            .is_some_and(|ids| ids.contains(id))
                    {
                        errors.push((
                            part.clone(),
                            format!("unresolved {name}={id:?}"),
                        ));
                    }
                }
            }
        }
        for (part, message) in errors {
            self.fail("V05", &part, message);
        }
    }

    fn child_order(&mut self) {
        let mut errors = Vec::new();
        for part in [
            "word/document.xml", "word/footnotes.xml", "word/styles.xml",
            "word/numbering.xml", "word/settings.xml",
        ] {
            if let Some(tree) = self.trees.get(part) {
                for node in tree.descendants() {
                    if let Some(message) = order_error(node) {
                        errors.push((part, message));
                    }
                }
            }
        }
        for (part, message) in errors {
            self.fail("V06", part, message);
        }
    }

    fn section(&mut self) {
        let body = self
            .trees
            .get("word/document.xml")
            .and_then(|tree| tree.child("w:body"));
        if !body.is_some_and(|body| {
            body.children.iter().filter(|node| node.name == "w:sectPr").count()
                == 1
                && body
                    .children
                    .last()
                    .is_some_and(|node| node.name == "w:sectPr")
        }) {
            self.fail(
                "V07",
                "word/document.xml",
                "body must end in exactly one sectPr",
            );
        }
    }

    fn bookmarks_and_fields(&mut self) {
        let mut names = BTreeSet::new();
        let mut global_ids = BTreeSet::new();
        let mut anchors = Vec::new();
        let mut errors = Vec::new();
        for (part, tree) in &self.trees {
            let mut starts = BTreeMap::new();
            let mut ends = BTreeMap::new();
            for node in tree.descendants() {
                match node.name.as_str() {
                    "w:bookmarkStart" => {
                        let id = integer_attr(node, "w:id");
                        let name = node.attr("w:name").unwrap_or("");
                        if id.is_none_or(|id| id < 0)
                            || !global_ids.insert(id)
                            || !names.insert(name.to_owned())
                            || name.is_empty()
                            || name.len() > 40
                            || !name.bytes().all(|ch| {
                                ch.is_ascii_alphanumeric() || ch == b'_'
                            })
                        {
                            errors.push((
                                "V08",
                                part.clone(),
                                "invalid or duplicate bookmark".into(),
                            ));
                        }
                        *starts.entry(id).or_insert(0) += 1;
                    }
                    "w:bookmarkEnd" => {
                        *ends.entry(integer_attr(node, "w:id")).or_insert(0) +=
                            1;
                    }
                    "w:hyperlink" => {
                        if let Some(anchor) = node.attr("w:anchor") {
                            anchors.push((part.clone(), anchor.to_owned()));
                        }
                    }
                    _ => {}
                }
            }
            if starts != ends || starts.values().any(|count| *count != 1) {
                errors.push((
                    "V08",
                    part.clone(),
                    "unpaired bookmark ids in part".into(),
                ));
            }
            let (instructions, balanced) = field_instructions(tree);
            if !balanced {
                errors.push((
                    "V10",
                    part.clone(),
                    "unbalanced field characters".into(),
                ));
            }
            for instruction in instructions {
                if let Some(anchor) = field_anchor(&instruction) {
                    anchors.push((part.clone(), anchor));
                }
            }
        }
        for (part, anchor) in anchors {
            if !names.contains(&anchor) {
                errors.push((
                    "V09",
                    part,
                    format!("unknown bookmark {anchor:?}"),
                ));
            }
        }
        for (rule, part, message) in errors {
            self.fail(rule, &part, message);
        }
    }

    fn drawing_ids(&mut self) {
        let mut ids = BTreeSet::new();
        let mut errors = Vec::new();
        for (part, tree) in &self.trees {
            for node in tree
                .descendants()
                .into_iter()
                .filter(|node| node.name == "wp:docPr")
            {
                let id = integer_attr(node, "id");
                if id.is_none_or(|id| id < 0) || !ids.insert(id) {
                    errors.push(part.clone());
                }
            }
        }
        for part in errors {
            self.fail("V11", &part, "invalid or duplicate docPr id");
        }
    }

    fn footnotes(&mut self) {
        let mut definitions = BTreeMap::new();
        let mut references = BTreeMap::new();
        let mut errors = Vec::new();
        if let Some(tree) = self.trees.get("word/footnotes.xml") {
            for node in
                tree.children.iter().filter(|node| node.name == "w:footnote")
            {
                let id = integer_attr(node, "w:id");
                if id.is_none() || definitions.insert(id, node).is_some() {
                    errors.push("invalid or duplicate footnote id".into());
                }
            }
            for (id, kind, child) in [
                (-1, "separator", "w:separator"),
                (0, "continuationSeparator", "w:continuationSeparator"),
            ] {
                if !definitions.get(&Some(id)).is_some_and(|node| {
                    node.attr("w:type") == Some(kind)
                        && node
                            .descendants()
                            .iter()
                            .any(|node| node.name == child)
                }) {
                    errors.push(format!("footnote {id} must be {kind}"));
                }
            }
        }
        for tree in self.trees.values() {
            for node in tree
                .descendants()
                .into_iter()
                .filter(|node| node.name == "w:footnoteReference")
            {
                let id = integer_attr(node, "w:id");
                *references.entry(id).or_insert(0) += 1;
                if id.is_none_or(|id| id < 1) || !definitions.contains_key(&id)
                {
                    errors.push(
                        "footnoteReference must name an existing positive id"
                            .into(),
                    );
                }
            }
        }
        for (id, count) in &references {
            if *count != 1 {
                errors
                    .push(format!("footnote {id:?} referenced more than once"));
            }
        }
        for id in definitions.keys().flatten().filter(|id| **id >= 1) {
            if references.get(&Some(*id)) != Some(&1) {
                errors.push(format!("footnote {id} must be referenced once"));
            }
        }
        for message in errors {
            self.fail("V12", "word/footnotes.xml", message);
        }
    }

    fn numbering(&mut self) {
        let mut abstract_ids = BTreeSet::new();
        let mut nums = BTreeMap::new();
        let mut errors: Vec<String> = Vec::new();
        if let Some(tree) = self.trees.get("word/numbering.xml") {
            for node in &tree.children {
                if node.name == "w:abstractNum" {
                    let id = integer_attr(node, "w:abstractNumId");
                    if id.is_none_or(|id| id < 0) || !abstract_ids.insert(id) {
                        errors
                            .push("invalid or duplicate abstractNumId".into());
                    }
                } else if node.name == "w:num" {
                    let id = integer_attr(node, "w:numId");
                    let abstract_id = node
                        .child("w:abstractNumId")
                        .and_then(|node| integer_attr(node, "w:val"));
                    if id.is_none_or(|id| id < 0)
                        || nums.insert(id, abstract_id).is_some()
                    {
                        errors.push("invalid or duplicate numId".into());
                    }
                }
            }
        }
        for id in nums.values() {
            if id.is_none() || !abstract_ids.contains(id) {
                errors.push("num references a missing abstractNum".into());
            }
        }
        for tree in self.trees.values() {
            for node in tree.descendants() {
                if node.name == "w:numId"
                    && !nums.contains_key(&integer_attr(node, "w:val"))
                {
                    errors.push("used numId is not defined".into());
                }
                let level = match node.name.as_str() {
                    "w:ilvl" => Some(integer_attr(node, "w:val")),
                    "w:lvl" | "w:lvlOverride" => {
                        Some(integer_attr(node, "w:ilvl"))
                    }
                    _ => None,
                };
                if level.is_some_and(|level| {
                    level.is_none_or(|level| !(0..=8).contains(&level))
                }) {
                    errors.push("ilvl must be in 0..=8".into());
                }
            }
        }
        for message in errors {
            self.fail("V13", "word/numbering.xml", message);
        }
    }

    fn styles(&mut self) {
        let mut styles = BTreeMap::new();
        let mut defaults = BTreeSet::new();
        let mut errors = Vec::new();
        if let Some(tree) = self.trees.get("word/styles.xml") {
            for node in
                tree.children.iter().filter(|node| node.name == "w:style")
            {
                let id = node.attr("w:styleId").unwrap_or("");
                let kind = node.attr("w:type").unwrap_or("");
                if id.is_empty() || styles.insert(id, kind).is_some() {
                    errors.push("missing or duplicate styleId".into());
                }
                if node.attr("w:default").is_some_and(is_on)
                    && !defaults.insert(kind)
                {
                    errors.push(format!("more than one default {kind} style"));
                }
            }
        }
        for tree in self.trees.values() {
            for node in tree.descendants() {
                let kind = match node.name.as_str() {
                    "w:pStyle" => Some("paragraph"),
                    "w:rStyle" => Some("character"),
                    "w:tblStyle" => Some("table"),
                    _ => None,
                };
                if let Some(kind) = kind
                    && node.attr("w:val").and_then(|id| styles.get(id)).copied()
                        != Some(kind)
                {
                    errors.push(format!(
                        "{} must name a {kind} style",
                        node.name
                    ));
                }
            }
        }
        if let Some(tree) = self.trees.get("word/styles.xml") {
            for node in tree.descendants().into_iter().filter(|node| {
                matches!(node.name.as_str(), "w:basedOn" | "w:next" | "w:link")
            }) {
                if !node.attr("w:val").is_some_and(|id| styles.contains_key(id))
                {
                    errors.push(format!("{} names a missing style", node.name));
                }
            }
        }
        for message in errors {
            self.fail("V14", "word/styles.xml", message);
        }
    }

    fn tables(&mut self) {
        let mut errors = Vec::new();
        for (part, tree) in &self.trees {
            for table in tree
                .descendants()
                .into_iter()
                .filter(|node| node.name == "w:tbl")
            {
                for message in table_errors(table) {
                    errors.push((part.clone(), message));
                }
            }
        }
        for (part, message) in errors {
            self.fail("V15", &part, message);
        }
    }

    fn whitespace_and_themes(&mut self, opts: &ValidateOptions) {
        let mut errors = Vec::new();
        for (part, tree) in &self.trees {
            for node in tree.descendants() {
                if matches!(node.name.as_str(), "w:t" | "w:instrText" | "m:t")
                    && (node.text.starts_with(char::is_whitespace)
                        || node.text.ends_with(char::is_whitespace))
                    && node.attr("xml:space") != Some("preserve")
                {
                    errors.push((
                        "V16",
                        part.clone(),
                        format!("{} needs xml:space=preserve", node.name),
                    ));
                }
                if opts.builtin_styles
                    && part == "word/styles.xml"
                    && node
                        .attrs
                        .keys()
                        .any(|name| name.to_ascii_lowercase().contains("theme"))
                {
                    errors.push((
                        "V17",
                        part.clone(),
                        "built-in style uses a theme attribute".into(),
                    ));
                }
            }
        }
        for (rule, part, message) in errors {
            self.fail(rule, &part, message);
        }
    }

    fn media(&mut self) {
        let mut referenced = BTreeSet::new();
        for (part, tree) in &self.trees {
            if let Some(owner) = relationship_owner(part) {
                for rel in &tree.children {
                    if rel.attr("TargetMode") != Some("External")
                        && let Some(target) = rel
                            .attr("Target")
                            .and_then(|target| resolve_target(&owner, target))
                    {
                        referenced.insert(target);
                    }
                }
            }
        }
        let mut errors = Vec::new();
        for (part, bytes) in self
            .parts
            .iter()
            .filter(|(name, _)| name.starts_with("word/media/"))
        {
            if !referenced.contains(part) {
                errors.push((part.clone(), "unreferenced media part"));
            }
            let kind = self.content_type(part).unwrap_or("");
            let matches = match kind {
                "image/png" => bytes.starts_with(b"\x89PNG"),
                "image/jpeg" => bytes.starts_with(b"\xFF\xD8"),
                "image/svg+xml" => svg_first_element(bytes),
                _ => true,
            };
            if !matches {
                errors.push((
                    part.clone(),
                    "media bytes do not match content type",
                ));
            }
        }
        for (part, message) in errors {
            self.fail("V18", &part, message);
        }
    }

    fn settings_and_dates(&mut self) {
        if self.trees.get("word/settings.xml").is_some_and(|tree| {
            tree.descendants().iter().any(|node| {
                matches!(node.name.as_str(), "w:footnotePr" | "w:endnotePr")
            })
        }) {
            self.fail(
                "V19",
                "word/settings.xml",
                "settings must not contain note properties",
            );
        }
        if let Some(tree) = self.trees.get("docProps/core.xml") {
            let dates = ["dcterms:created", "dcterms:modified"].map(|name| {
                tree.children
                    .iter()
                    .filter(|node| node.name == name)
                    .collect::<Vec<_>>()
            });
            if dates.iter().all(Vec::is_empty) {
                return;
            }
            let valid = dates.iter().all(|nodes| {
                nodes.len() == 1 && {
                    let node = nodes[0];
                    node.attr("xsi:type") == Some("dcterms:W3CDTF")
                        && node.text.ends_with('Z')
                        && time::OffsetDateTime::parse(
                            &node.text,
                            &time::format_description::well_known::Rfc3339,
                        )
                        .is_ok()
                }
            });
            if !valid {
                self.fail("V20", "docProps/core.xml", "created and modified must both be UTC W3CDTF dates, or absent");
            }
        }
    }
}

fn integer_attr(node: &XmlNode, name: &str) -> Option<i64> {
    node.attr(name)?.parse().ok()
}

fn is_on(value: &str) -> bool {
    matches!(value, "1" | "true" | "on")
}

fn relationship_owner(part: &str) -> Option<String> {
    if part == "_rels/.rels" {
        return Some(String::new());
    }
    let (directory, file) = part.rsplit_once("/_rels/")?;
    Some(format!("{directory}/{}", file.strip_suffix(".rels")?))
}

pub(crate) fn resolve_target(owner: &str, target: &str) -> Option<String> {
    if target.is_empty() || target.contains('\\') {
        return None;
    }
    let mut segments: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        owner
            .rsplit_once('/')
            .map_or(Vec::new(), |(dir, _)| dir.split('/').collect())
    };
    for segment in target.split('#').next()?.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            _ => segments.push(segment),
        }
    }
    let joined = segments.join("/");
    let bytes = joined.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let digits =
                std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(digits, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok().filter(|name| !name.is_empty())
}

fn order_error(node: &XmlNode) -> Option<String> {
    let property = match node.name.as_str() {
        "w:tbl" => Some("w:tblPr"),
        "w:tr" => Some("w:trPr"),
        "w:tc" => Some("w:tcPr"),
        "w:p" => Some("w:pPr"),
        "w:r" => Some("w:rPr"),
        _ => None,
    };
    let order = match node.name.as_str() {
        "w:pPr" => Some(xml::PPR),
        "w:rPr" => Some(xml::RPR),
        "w:tblPr" => Some(xml::TBLPR),
        "w:trPr" => Some(xml::TRPR),
        "w:tcPr" => Some(xml::TCPR),
        "w:sectPr" => Some(xml::SECTPR),
        "w:style" => Some(xml::STYLE),
        "w:lvl" => Some(xml::LVL),
        "w:settings" => Some(xml::SETTINGS),
        _ => xml::ORDERS
            .iter()
            .find(|(name, _)| *name == node.name)
            .map(|(_, order)| *order),
    };
    if property.is_none() && order.is_none() && node.name != "w:body" {
        return None;
    }
    let mut previous = 0;
    for child in &node.children {
        let known_prefix =
            child.name.split_once(':').is_some_and(|(prefix, _)| {
                matches!(prefix, "w" | "m" | "wp" | "a" | "pic")
            });
        let rank = if let Some(property) = property {
            if !known_prefix {
                continue;
            }
            usize::from(child.name != property)
        } else if node.name == "w:body" {
            if !known_prefix {
                continue;
            }
            usize::from(child.name == "w:sectPr")
        } else {
            let order = order?;
            match order.iter().position(|name| *name == child.name) {
                Some(_)
                    if node.name == "w:sectPr"
                        && matches!(
                            child.name.as_str(),
                            "w:headerReference" | "w:footerReference"
                        ) =>
                {
                    0
                }
                Some(index) => index,
                None if !known_prefix => continue,
                None => {
                    return Some(format!(
                        "unlisted {} child of {}",
                        child.name, node.name
                    ));
                }
            }
        };
        if rank < previous {
            return Some(format!(
                "{} is out of order in {}",
                child.name, node.name
            ));
        }
        previous = rank;
    }
    None
}

fn field_instructions(tree: &XmlNode) -> (Vec<String>, bool) {
    let mut stack: Vec<(String, bool)> = Vec::new();
    let mut instructions = Vec::new();
    let mut balanced = true;
    for node in tree.descendants() {
        if node.name == "w:fldSimple" {
            if let Some(instruction) = node.attr("w:instr") {
                instructions.push(instruction.into());
            }
        } else if node.name == "w:fldChar" {
            match node.attr("w:fldCharType") {
                Some("begin") => stack.push((String::new(), false)),
                Some("separate") => {
                    if let Some((_, separated)) = stack.last_mut() {
                        if *separated {
                            balanced = false;
                        }
                        *separated = true;
                    } else {
                        balanced = false;
                    }
                }
                Some("end") => {
                    if let Some((instruction, _)) = stack.pop() {
                        instructions.push(instruction);
                    } else {
                        balanced = false;
                    }
                }
                _ => balanced = false,
            }
        } else if node.name == "w:instrText" {
            if let Some((instruction, false)) = stack.last_mut() {
                instruction.push_str(&node.text);
            } else if stack.is_empty() {
                instructions.push(node.text.clone());
            }
        }
    }
    (instructions, balanced && stack.is_empty())
}

fn field_anchor(instruction: &str) -> Option<String> {
    let instruction = instruction.trim_start();
    let end =
        instruction.find(char::is_whitespace).unwrap_or(instruction.len());
    if !matches!(
        instruction[..end].to_ascii_uppercase().as_str(),
        "REF" | "PAGEREF" | "NOTEREF"
    ) {
        return None;
    }
    let argument = instruction[end..].trim_start();
    let anchor = if let Some(quoted) = argument.strip_prefix('"') {
        quoted.split('"').next().unwrap_or("")
    } else {
        argument.split_whitespace().next().unwrap_or("")
    };
    Some(anchor.into())
}

fn table_errors(table: &XmlNode) -> Vec<String> {
    let mut errors = Vec::new();
    let columns = table.child("w:tblGrid").map_or(0, |grid| {
        grid.children.iter().filter(|node| node.name == "w:gridCol").count()
    });
    let mut merged = BTreeSet::new();
    for row in table.children.iter().filter(|node| node.name == "w:tr") {
        let mut column = 0usize;
        let mut next_merged = BTreeSet::new();
        for cell in row.children.iter().filter(|node| node.name == "w:tc") {
            if cell.children.last().is_none_or(|node| node.name != "w:p") {
                errors.push("table cell must end with a paragraph".into());
            }
            let props = cell.child("w:tcPr");
            let span = props
                .and_then(|node| node.child("w:gridSpan"))
                .map_or(Some(1), |node| integer_attr(node, "w:val"))
                .and_then(|span| usize::try_from(span).ok())
                .filter(|span| *span > 0);
            let Some(span) = span else {
                errors.push("gridSpan must be positive".into());
                continue;
            };
            if let Some(merge) = props.and_then(|node| node.child("w:vMerge")) {
                match merge.attr("w:val").unwrap_or("continue") {
                    "restart" => {},
                    "continue" if merged.contains(&(column, span)) => {},
                    _ => errors.push("vMerge continue must follow a matching restart/continue".into()),
                }
                next_merged.insert((column, span));
            }
            column = column.saturating_add(span);
        }
        if column != columns {
            errors.push("row gridSpans must sum to the gridCol count".into());
        }
        merged = next_merged;
    }
    errors
}

fn svg_first_element(bytes: &[u8]) -> bool {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return false;
    };
    let mut reader = Reader::from_str(source.trim_start_matches('\u{feff}'));
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag) | Event::Empty(tag)) => {
                return tag.name().as_ref().rsplit(':').next() == Some("svg");
            }
            Ok(
                Event::Decl(_)
                | Event::PI(_)
                | Event::Comment(_)
                | Event::DocType(_),
            ) => {}
            Ok(Event::Text(text)) if text.as_ref().trim().is_empty() => {}
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{
        CompressionMethod, DateTime, System, ZipWriter,
        write::SimpleFileOptions,
    };

    const W: &str =
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const R: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
    const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
    const M: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/math";
    const RELS: &str =
        "http://schemas.openxmlformats.org/package/2006/relationships";
    const CT: &str =
        "http://schemas.openxmlformats.org/package/2006/content-types";

    fn root(name: &str, body: &str) -> String {
        format!(
            r#"<{name} xmlns:w="{W}" xmlns:r="{R}" xmlns:wp="{WP}" xmlns:a="{A}" xmlns:m="{M}">{body}</{name}>"#
        )
    }

    fn rel(id: &str, kind: &str, target: &str, mode: &str) -> String {
        format!(
            r#"<Relationship Id="{id}" Type="{R}/{kind}" Target="{target}"{mode}/>"#
        )
    }

    fn valid_parts() -> BTreeMap<String, Vec<u8>> {
        let document = root(
            "w:document",
            concat!(
                "<w:body><w:p><w:pPr><w:pStyle w:val=\"BodyText\"/>",
                "<w:keepNext/><w:numPr><w:ilvl w:val=\"0\"/>",
                "<w:numId w:val=\"1001\"/></w:numPr></w:pPr>",
                "<w:bookmarkStart w:id=\"1\" w:name=\"mark1\"/>",
                "<w:r><w:rPr><w:rStyle w:val=\"Strong\"/></w:rPr><w:t>Text</w:t></w:r>",
                "<w:bookmarkEnd w:id=\"1\"/>",
                "<w:hyperlink w:anchor=\"mark1\"><w:r><w:t>Anchor</w:t></w:r></w:hyperlink>",
                "<w:hyperlink r:id=\"rId6\"><w:r><w:t>External</w:t></w:r></w:hyperlink>",
                "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
                "<w:r><w:instrText xml:space=\"preserve\"> REF mark1 </w:instrText></w:r>",
                "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
                "<w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
                "<w:r><w:footnoteReference w:id=\"1\"/></w:r>",
                "<w:r><w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/>",
                "<wp:docPr id=\"1\" name=\"Image\"/><a:graphic><a:blip r:embed=\"rId5\"/>",
                "</a:graphic></wp:inline></w:drawing></w:r></w:p>",
                "<w:tbl><w:tblPr><w:tblStyle w:val=\"TableNormal\"/></w:tblPr>",
                "<w:tblGrid><w:gridCol w:w=\"100\"/><w:gridCol w:w=\"100\"/></w:tblGrid>",
                "<w:tr><w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr><w:p/></w:tc>",
                "<w:tc><w:p/></w:tc></w:tr>",
                "<w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>",
                "<w:tc><w:p/></w:tc></w:tr></w:tbl>",
                "<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/></w:sectPr></w:body>"
            ),
        );
        let styles = root(
            "w:styles",
            concat!(
                "<w:style w:type=\"paragraph\" w:styleId=\"Normal\" w:default=\"1\">",
                "<w:name w:val=\"Normal\"/></w:style>",
                "<w:style w:type=\"paragraph\" w:styleId=\"BodyText\">",
                "<w:name w:val=\"Body Text\"/><w:basedOn w:val=\"Normal\"/>",
                "<w:next w:val=\"BodyText\"/><w:link w:val=\"BodyTextChar\"/></w:style>",
                "<w:style w:type=\"character\" w:styleId=\"DefaultParagraphFont\" w:default=\"1\">",
                "<w:name w:val=\"Default Paragraph Font\"/></w:style>",
                "<w:style w:type=\"character\" w:styleId=\"BodyTextChar\">",
                "<w:name w:val=\"Body Text Char\"/><w:basedOn w:val=\"DefaultParagraphFont\"/>",
                "<w:link w:val=\"BodyText\"/></w:style>",
                "<w:style w:type=\"character\" w:styleId=\"Strong\">",
                "<w:name w:val=\"Strong\"/><w:basedOn w:val=\"DefaultParagraphFont\"/></w:style>",
                "<w:style w:type=\"table\" w:styleId=\"TableNormal\" w:default=\"1\">",
                "<w:name w:val=\"Table Normal\"/></w:style>"
            ),
        );
        let numbering = root(
            "w:numbering",
            concat!(
                "<w:abstractNum w:abstractNumId=\"991\"><w:lvl w:ilvl=\"0\">",
                "<w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/>",
                "<w:lvlText w:val=\"%1.\"/></w:lvl></w:abstractNum>",
                "<w:num w:numId=\"1001\"><w:abstractNumId w:val=\"991\"/></w:num>"
            ),
        );
        let notes = root(
            "w:footnotes",
            concat!(
                "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/>",
                "</w:r></w:p></w:footnote>",
                "<w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r>",
                "<w:continuationSeparator/></w:r></w:p></w:footnote>",
                "<w:footnote w:id=\"1\"><w:p><w:r><w:t>Note</w:t></w:r></w:p></w:footnote>"
            ),
        );
        let settings = root("w:settings", "<w:zoom w:percent=\"100\"/>");
        let core = concat!(
            "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" ",
            "xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">",
            "<dcterms:created xsi:type=\"dcterms:W3CDTF\">1970-01-01T00:00:00Z</dcterms:created>",
            "<dcterms:modified xsi:type=\"dcterms:W3CDTF\">1970-01-01T00:00:00Z</dcterms:modified>",
            "</cp:coreProperties>"
        );
        let mut parts = BTreeMap::from([
            ("word/document.xml".into(), document.into_bytes()),
            ("word/styles.xml".into(), styles.into_bytes()),
            ("word/numbering.xml".into(), numbering.into_bytes()),
            ("word/footnotes.xml".into(), notes.into_bytes()),
            ("word/settings.xml".into(), settings.into_bytes()),
            ("docProps/core.xml".into(), core.as_bytes().to_vec()),
            ("word/media/image.png".into(), b"\x89PNGexample".to_vec()),
        ]);
        let document_rels = [
            rel("rId1", "styles", "styles.xml", ""),
            rel("rId2", "numbering", "numbering.xml", ""),
            rel("rId3", "footnotes", "footnotes.xml", ""),
            rel("rId4", "settings", "settings.xml", ""),
            rel("rId5", "image", "media/image.png", ""),
            rel(
                "rId6", "hyperlink", "https://example.org",
                " TargetMode=\"External\"",
            ),
        ]
        .concat();
        parts.insert("word/_rels/document.xml.rels".into(), format!(
            "<Relationships xmlns=\"{RELS}\">{document_rels}</Relationships>"
        ).into_bytes());
        parts.insert(
            "_rels/.rels".into(),
            format!(
                "<Relationships xmlns=\"{RELS}\">{}</Relationships>",
                rel("rId1", "officeDocument", "word/document.xml", "")
            )
            .into_bytes(),
        );
        let mut types = format!(
            "<Types xmlns=\"{CT}\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/>"
        );
        for (part, subtype) in [
            ("word/document.xml", "document.main"),
            ("word/styles.xml", "styles"),
            ("word/numbering.xml", "numbering"),
            ("word/footnotes.xml", "footnotes"),
            ("word/settings.xml", "settings"),
        ] {
            types.push_str(&format!(
                "<Override PartName=\"/{part}\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.{subtype}+xml\"/>"
            ));
        }
        types.push_str("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/></Types>");
        parts.insert("[Content_Types].xml".into(), types.into_bytes());
        parts
    }

    fn zip_parts(parts: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(DateTime::default())
            .system(System::Unix)
            .unix_permissions(0o644);
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let first = "[Content_Types].xml";
        for name in std::iter::once(first).chain(
            parts.keys().map(String::as_str).filter(|name| *name != first),
        ) {
            if let Some(bytes) = parts.get(name) {
                archive.start_file(name, options).unwrap();
                archive.write_all(bytes).unwrap();
            }
        }
        archive.finish().unwrap().into_inner()
    }

    fn replace(
        parts: &mut BTreeMap<String, Vec<u8>>, part: &str, from: &str, to: &str,
    ) {
        let source = String::from_utf8(parts[part].clone()).unwrap();
        assert!(source.contains(from), "missing mutation text {from:?}");
        parts.insert(part.into(), source.replace(from, to).into_bytes());
    }

    fn fails(rule: &str, part: &str, from: &str, to: &str) {
        let mut parts = valid_parts();
        replace(&mut parts, part, from, to);
        let violations = validate(
            &zip_parts(&parts),
            &ValidateOptions { builtin_styles: true },
        );
        assert!(
            violations.iter().any(|violation| violation.rule == rule),
            "{violations:#?}"
        );
    }

    #[test]
    fn valid_package_passes() {
        assert_eq!(
            validate(
                &zip_parts(&valid_parts()),
                &ValidateOptions { builtin_styles: true }
            ),
            []
        );
    }

    #[test]
    fn v01_zip() {
        assert_eq!(
            validate(b"not a zip", &ValidateOptions::default())[0].rule,
            "V01"
        );
        let parts = valid_parts();
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        archive.add_directory("folder/", SimpleFileOptions::default()).unwrap();
        for (name, bytes) in parts {
            archive
                .start_file(
                    name,
                    SimpleFileOptions::default().last_modified_time(
                        DateTime::from_date_and_time(2026, 1, 1, 0, 0, 0)
                            .unwrap(),
                    ),
                )
                .unwrap();
            archive.write_all(&bytes).unwrap();
        }
        let result = validate(
            &archive.finish().unwrap().into_inner(),
            &ValidateOptions::default(),
        );
        for text in ["first entry", "directory entry", "mtime"] {
            assert!(
                result.iter().any(|violation| violation.rule == "V01"
                    && violation.message.contains(text)),
                "{result:#?}"
            );
        }

        // Rewriting both names keeps local/central headers consistent, while
        // giving the archive two entries with the same name.
        let mut parts = valid_parts();
        parts.insert("word/copied.xml".into(), b"<copy/>".to_vec());
        let mut bytes = zip_parts(&parts);
        let old = b"word/copied.xml";
        let offsets = bytes
            .windows(old.len())
            .enumerate()
            .filter_map(|(index, value)| (value == old).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(offsets.len(), 2);
        for index in offsets {
            bytes[index..index + old.len()].copy_from_slice(b"word/styles.xml");
        }
        let result = validate(&bytes, &ValidateOptions::default());
        assert!(
            result.iter().any(|violation| {
                violation.rule == "V01"
                    && violation.message.contains("duplicate")
            }),
            "{result:#?}"
        );
    }

    fn bad_xml(source: &str) {
        let mut parts = valid_parts();
        parts.insert("word/document.xml".into(), source.as_bytes().to_vec());
        let violations =
            validate(&zip_parts(&parts), &ValidateOptions::default());
        assert!(
            violations.iter().any(|violation| violation.rule == "V02"),
            "{violations:#?}"
        );
    }

    #[test]
    fn v02_unclosed() {
        bad_xml("<a><b>");
    }
    #[test]
    fn v02_two_roots() {
        bad_xml("<a/><b/>");
    }
    #[test]
    fn v02_unknown_entity() {
        bad_xml("<a>&foo;</a>");
        bad_xml("<a x=\"&foo;\"/>");
    }
    #[test]
    fn v02_duplicate_attribute() {
        bad_xml("<a x=\"1\" x=\"2\"/>");
        bad_xml("<a xmlns:x=\"same\" xmlns:y=\"same\" x:id=\"1\" y:id=\"2\"/>");
    }
    #[test]
    fn v02_unquoted_attribute() {
        bad_xml("<a x=1/>");
    }
    #[test]
    fn v02_control_char() {
        bad_xml("<a>\u{1}</a>");
        bad_xml("<a x=\"\u{1}\"/>");
        bad_xml("<a x=\"unescaped < here\"/>");
    }
    #[test]
    fn v02_bad_char_ref() {
        for source in [
            "<a>&#0;</a>", "<a>&#x1;</a>", "<a>&#xD800;</a>",
            "<a x=\"&#x1;\"/>",
        ] {
            bad_xml(source);
        }
    }
    #[test]
    fn v02_unbound_prefix() {
        bad_xml("<a:root/>");
        bad_xml("<a b:val=\"1\"/>");
        bad_xml("<a><b xmlns:c=\"urn:c\"/><c:root/></a>");
    }
    #[test]
    fn v02_text_outside_root() {
        bad_xml("outside<a/>");
        bad_xml("<a/>outside");
        bad_xml("<a/><![CDATA[text]]>");
    }

    #[test]
    fn v02_declarations_names_and_markup() {
        for source in [
            "<?xml version=\"1.0\" version=\"1.0\"?><a/>",
            "<?xml version=\"1.7\"?><a/>",
            " <!--before--><?xml version=\"1.0\"?><a/>",
            "<?XML version=\"1.0\"?><a/>",
            "<1bad/>",
            "<a xmlns:bad:prefix=\"urn:bad\"/>",
            "<a xmlns:xml=\"urn:wrong\"/>",
            "<a>]]></a>",
        ] {
            bad_xml(source);
        }
        assert!(parse_xml("<é><\u{200c}name/></é>").is_ok());
    }

    #[test]
    fn xml_tree_preserves_ranges_and_resolves_entities() {
        let source = "<?xml version=\"1.0\"?><!--before--><a xmlns:p=\"urn:p\"><p:b x=\"a&#x9;b\">&amp;&#x20;&lt;&gt;&quot;&apos;</p:b><empty/></a>";
        let tree = parse_xml(source).unwrap();
        assert_eq!(
            &source[tree.range.start..tree.open_end],
            "<a xmlns:p=\"urn:p\">"
        );
        let child = &tree.children[0];
        assert_eq!(child.name, "p:b");
        assert_eq!(child.attr("x"), Some("a\tb"));
        assert_eq!(child.text, "& <>\"'");
        assert_eq!(&source[child.close_start..child.range.end], "</p:b>");
        let empty = &tree.children[1];
        assert_eq!(&source[empty.range.clone()], "<empty/>");
        assert_eq!(empty.close_start, empty.open_end);
        assert_eq!(&source[tree.close_start..tree.range.end], "</a>");
    }

    #[test]
    fn v03_content_types() {
        fails(
            "V03",
            "[Content_Types].xml",
            "wordprocessingml.document.main+xml",
            "wordprocessingml.styles+xml",
        );
        fails(
            "V03",
            "[Content_Types].xml",
            "</Types>",
            "<Default Extension=\"png\" ContentType=\"image/png\"/></Types>",
        );
        fails(
            "V03", "[Content_Types].xml", "/word/styles.xml",
            "/word/missing.xml",
        );
        fails(
            "V03",
            "[Content_Types].xml",
            "</Types>",
            "<Override PartName=\"/word/styles.xml\" ContentType=\"application/xml\"/></Types>",
        );
        fails(
            "V03",
            "[Content_Types].xml",
            "</Types>",
            "<Override PartName=\"/word/%73tyles.xml\" ContentType=\"application/xml\"/></Types>",
        );
    }

    #[test]
    fn encoded_part_names_and_relationship_targets_are_valid() {
        let mut parts = valid_parts();
        parts.insert(
            "word/header 1.xml".into(),
            root("w:hdr", "<w:p/>").into_bytes(),
        );
        replace(
            &mut parts,
            "[Content_Types].xml",
            "</Types>",
            "<Override PartName=\"/word/header%201.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/></Types>",
        );
        let image = parts.remove("word/media/image.png").unwrap();
        parts.insert("word/media/image one.png".into(), image);
        replace(
            &mut parts, "word/_rels/document.xml.rels", "media/image.png",
            "media/image%20one.png",
        );
        assert_eq!(
            validate(&zip_parts(&parts), &ValidateOptions::default()),
            []
        );
    }

    #[test]
    fn v04_package_rels() {
        fails("V04", "_rels/.rels", "/officeDocument", "/styles");
    }

    #[test]
    fn v05_rels() {
        let part = "word/_rels/document.xml.rels";
        fails("V05", part, "Id=\"rId2\"", "Id=\"rId1\"");
        fails("V05", part, "media/image.png", "media/missing.png");
        fails("V05", part, " TargetMode=\"External\"", "");
        fails(
            "V05", "word/document.xml", "r:embed=\"rId5\"",
            "r:embed=\"absent\"",
        );
    }

    #[test]
    fn v06_child_order() {
        fails(
            "V06",
            "word/document.xml",
            "<w:pStyle w:val=\"BodyText\"/><w:keepNext/>",
            "<w:keepNext/><w:pStyle w:val=\"BodyText\"/>",
        );
        fails("V06", "word/document.xml", "<w:keepNext/>", "<w:madeUp/>");
        fails(
            "V06",
            "word/numbering.xml",
            "<w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/>",
            "<w:numFmt w:val=\"decimal\"/><w:start w:val=\"1\"/>",
        );
    }

    #[test]
    fn v07_sect_pr() {
        fails("V07", "word/document.xml", "</w:body>", "<w:p/></w:body>");
        fails("V07", "word/document.xml", "</w:body>", "<w:sectPr/></w:body>");
    }

    #[test]
    fn v08_bookmarks() {
        fails("V08", "word/document.xml", "<w:bookmarkEnd w:id=\"1\"/>", "");
        fails(
            "V08", "word/document.xml", "w:name=\"mark1\"",
            "w:name=\"has-hyphen\"",
        );
        fails(
            "V08",
            "word/document.xml",
            "w:name=\"mark1\"",
            "w:name=\"abcdefghijklmnopqrstuvwxyz1234567890_extra\"",
        );
        fails(
            "V08",
            "word/document.xml",
            "</w:p>",
            "<w:bookmarkStart w:id=\"1\" w:name=\"mark1\"/><w:bookmarkEnd w:id=\"1\"/></w:p>",
        );
        fails(
            "V08",
            "word/footnotes.xml",
            "<w:t>Note</w:t>",
            "<w:bookmarkStart w:id=\"1\" w:name=\"other\"/><w:bookmarkEnd w:id=\"1\"/>",
        );
    }

    #[test]
    fn v09_anchors() {
        fails(
            "V09", "word/document.xml", "w:anchor=\"mark1\"",
            "w:anchor=\"missing\"",
        );
        for name in ["REF", "PAGEREF", "NOTEREF"] {
            fails(
                "V09",
                "word/document.xml",
                " REF mark1 ",
                &format!(" {name} missing "),
            );
        }
    }

    #[test]
    fn v10_fields() {
        fails(
            "V10",
            "word/document.xml",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            "",
        );
        fails(
            "V10", "word/document.xml", "w:fldCharType=\"begin\"",
            "w:fldCharType=\"separate\"",
        );
        fails(
            "V10",
            "word/document.xml",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/><w:fldChar w:fldCharType=\"separate\"/></w:r>",
        );
    }

    #[test]
    fn v11_doc_pr() {
        fails(
            "V11",
            "word/document.xml",
            "<wp:docPr id=\"1\" name=\"Image\"/>",
            "<wp:docPr id=\"1\" name=\"Image\"/><wp:docPr id=\"1\" name=\"Duplicate\"/>",
        );
        fails(
            "V11",
            "word/footnotes.xml",
            "<w:t>Note</w:t>",
            "<wp:docPr id=\"1\" name=\"Copied\"/>",
        );
    }

    #[test]
    fn v12_footnotes() {
        fails(
            "V12",
            "word/footnotes.xml",
            "w:type=\"separator\"",
            "w:type=\"continuationSeparator\"",
        );
        fails(
            "V12",
            "word/document.xml",
            "<w:footnoteReference w:id=\"1\"/>",
            "<w:footnoteReference w:id=\"2\"/>",
        );
        fails(
            "V12",
            "word/document.xml",
            "<w:footnoteReference w:id=\"1\"/>",
            "<w:footnoteReference w:id=\"0\"/>",
        );
        fails(
            "V12",
            "word/document.xml",
            "<w:footnoteReference w:id=\"1\"/>",
            "<w:footnoteReference w:id=\"1\"/><w:footnoteReference w:id=\"1\"/>",
        );
    }

    #[test]
    fn v13_numbering() {
        fails(
            "V13", "word/document.xml", "<w:numId w:val=\"1001\"/>",
            "<w:numId w:val=\"1234\"/>",
        );
        fails(
            "V13",
            "word/numbering.xml",
            "<w:abstractNumId w:val=\"991\"/>",
            "<w:abstractNumId w:val=\"0\"/>",
        );
        fails("V13", "word/numbering.xml", "w:ilvl=\"0\"", "w:ilvl=\"9\"");
        fails(
            "V13",
            "word/numbering.xml",
            "</w:numbering>",
            "<w:num w:numId=\"1001\"><w:abstractNumId w:val=\"991\"/></w:num></w:numbering>",
        );
    }

    #[test]
    fn v14_styles() {
        fails(
            "V14", "word/document.xml", "<w:pStyle w:val=\"BodyText\"/>",
            "<w:pStyle w:val=\"Strong\"/>",
        );
        fails(
            "V14", "word/styles.xml", "w:val=\"DefaultParagraphFont\"",
            "w:val=\"absent\"",
        );
        fails(
            "V14", "word/styles.xml", "w:styleId=\"BodyText\"",
            "w:styleId=\"Normal\"",
        );
        fails(
            "V14",
            "word/styles.xml",
            "w:styleId=\"BodyText\"",
            "w:styleId=\"BodyText\" w:default=\"1\"",
        );
    }

    #[test]
    fn v15_tables() {
        fails(
            "V15", "word/document.xml", "<w:tc><w:p/></w:tc>",
            "<w:tc><w:tbl/></w:tc>",
        );
        fails(
            "V15",
            "word/document.xml",
            "<w:tc><w:p/></w:tc>",
            "<w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr><w:p/></w:tc>",
        );
        fails(
            "V15", "word/document.xml", "w:val=\"restart\"",
            "w:val=\"continue\"",
        );
    }

    #[test]
    fn v16_space() {
        fails(
            "V16", "word/document.xml", "<w:t>Text</w:t>", "<w:t> Text </w:t>",
        );
        fails("V16", "word/document.xml", "xml:space=\"preserve\"", "");
        fails("V16", "word/document.xml", "<w:t>Text</w:t>", "<m:t> x </m:t>");
    }

    #[test]
    fn v17_theme_attributes() {
        fails(
            "V17",
            "word/styles.xml",
            "<w:basedOn w:val=\"Normal\"/>",
            "<w:basedOn w:val=\"Normal\"/><w:rPr><w:rFonts w:asciiTheme=\"minorHAnsi\"/></w:rPr>",
        );
        let mut parts = valid_parts();
        replace(
            &mut parts,
            "word/styles.xml",
            "<w:name w:val=\"Normal\"/>",
            "<w:name w:val=\"Normal\" w:themeColor=\"accent1\"/>",
        );
        assert_eq!(
            validate(
                &zip_parts(&parts),
                &ValidateOptions { builtin_styles: false }
            ),
            []
        );
    }

    #[test]
    fn v18_media() {
        let mut parts = valid_parts();
        parts.insert("word/media/image.png".into(), b"not png".to_vec());
        assert!(
            validate(&zip_parts(&parts), &ValidateOptions::default())
                .iter()
                .any(|v| v.rule == "V18")
        );
        parts = valid_parts();
        parts.insert("word/media/orphan.png".into(), b"\x89PNGorphan".to_vec());
        assert!(
            validate(&zip_parts(&parts), &ValidateOptions::default())
                .iter()
                .any(|v| v.rule == "V18")
        );
        for source in [
            "<svg/>",
            "\u{feff}<?xml version=\"1.0\"?><!-- c --><!DOCTYPE svg><svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        ] {
            assert!(svg_first_element(source.as_bytes()));
        }
        assert!(!svg_first_element(b"<!-- <svg/> --><html/>"));
        assert!(!svg_first_element(b"not xml <svg/>"));
    }

    #[test]
    fn v19_settings_notes() {
        fails(
            "V19", "word/settings.xml", "</w:settings>",
            "<w:footnotePr/></w:settings>",
        );
        fails(
            "V19", "word/settings.xml", "</w:settings>",
            "<w:endnotePr/></w:settings>",
        );
    }

    #[test]
    fn v20_dcterms() {
        fails(
            "V20",
            "docProps/core.xml",
            "<dcterms:modified xsi:type=\"dcterms:W3CDTF\">1970-01-01T00:00:00Z</dcterms:modified>",
            "",
        );
        fails(
            "V20", "docProps/core.xml", "1970-01-01T00:00:00Z",
            "1970-99-99T00:00:00Z",
        );
        fails(
            "V20", "docProps/core.xml", "1970-01-01T00:00:00Z",
            "1970-01-01T00:00:00+00:00",
        );
        let mut parts = valid_parts();
        replace(
            &mut parts,
            "docProps/core.xml",
            "<dcterms:modified xsi:type=\"dcterms:W3CDTF\">1970-01-01T00:00:00Z</dcterms:modified>",
            "",
        );
        replace(
            &mut parts,
            "docProps/core.xml",
            "<dcterms:created xsi:type=\"dcterms:W3CDTF\">1970-01-01T00:00:00Z</dcterms:created>",
            "",
        );
        assert_eq!(
            validate(&zip_parts(&parts), &ValidateOptions::default()),
            []
        );
    }

    #[test]
    fn nested_fields_and_split_instructions_are_valid() {
        let mut parts = valid_parts();
        replace(&mut parts, "word/document.xml", " REF mark1 ", " REF ");
        replace(
            &mut parts,
            "word/document.xml",
            "</w:instrText>",
            "</w:instrText><w:instrText xml:space=\"preserve\">mark1 </w:instrText>",
        );
        replace(
            &mut parts,
            "word/document.xml",
            "<w:t>1</w:t>",
            "<w:fldChar w:fldCharType=\"begin\"/><w:instrText>PAGE</w:instrText><w:fldChar w:fldCharType=\"end\"/>",
        );
        assert_eq!(
            validate(&zip_parts(&parts), &ValidateOptions::default()),
            []
        );
    }

    #[test]
    fn relative_relationship_targets_resolve_from_the_owner() {
        assert_eq!(
            relationship_owner("word/theme/_rels/theme1.xml.rels"),
            Some("word/theme/theme1.xml".into())
        );
        assert_eq!(
            resolve_target("word/theme/theme1.xml", "../media/a%20b.png"),
            Some("word/media/a b.png".into())
        );
        assert_eq!(
            resolve_target("word/document.xml", "/word/media/a.png"),
            Some("word/media/a.png".into())
        );
        assert_eq!(
            resolve_target("word/document.xml", "../../escape.xml"),
            None
        );
    }

    #[test]
    fn header_and_footer_references_share_an_order_rank() {
        let children = concat!(
            "<w:footerReference/><w:headerReference/>",
            "<w:footerReference/><w:pgSz/>"
        );
        let node = parse_xml(&root("w:sectPr", children)).unwrap();
        assert_eq!(order_error(&node), None);
        let node = parse_xml(&root(
            "w:pPr",
            "<w:keepNext/><custom xmlns=\"urn:custom\"/><w:spacing/>",
        ))
        .unwrap();
        assert_eq!(order_error(&node), None);
    }
}
