//! DOCX styles and style resolution (docx-output.md §2).
//! Owner: T04. Stub created by T00.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::LazyLock;

use crate::error::{Diagnostics, Error, Result};

use super::validate::{XmlNode, parse_xml};
use super::xml::{NS_W, Props, RPR, STYLE, XML_HEAD, escape};

/// WordprocessingML style category, used together with the style's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StyleKind {
    /// A paragraph style (`w:pStyle`).
    Paragraph,
    /// A character style (`w:rStyle`).
    Character,
    /// A table style (`w:tblStyle`).
    Table,
}

impl StyleKind {
    fn xml_name(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Character => "character",
            Self::Table => "table",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Paragraph => "Paragraph",
            Self::Character => "Char",
            Self::Table => "Table",
        }
    }

    fn base(self) -> &'static str {
        match self {
            Self::Paragraph => "Body Text",
            Self::Character => "Body Text Char",
            Self::Table => "Table",
        }
    }

    fn parse(value: Option<&str>) -> Option<Self> {
        match value? {
            "paragraph" => Some(Self::Paragraph),
            "character" => Some(Self::Character),
            "table" => Some(Self::Table),
            _ => None,
        }
    }
}

/// The 101 built-in names and kinds from `docx-output.md` §2.3.
pub const VOCABULARY: &[(&str, StyleKind)] = &[
    ("Normal", StyleKind::Paragraph),
    ("Body Text", StyleKind::Paragraph),
    ("First Paragraph", StyleKind::Paragraph),
    ("Compact", StyleKind::Paragraph),
    ("Title", StyleKind::Paragraph),
    ("Subtitle", StyleKind::Paragraph),
    ("Author", StyleKind::Paragraph),
    ("Date", StyleKind::Paragraph),
    ("Abstract Title", StyleKind::Paragraph),
    ("Abstract", StyleKind::Paragraph),
    ("Bibliography", StyleKind::Paragraph),
    ("heading 1", StyleKind::Paragraph),
    ("heading 2", StyleKind::Paragraph),
    ("heading 3", StyleKind::Paragraph),
    ("heading 4", StyleKind::Paragraph),
    ("heading 5", StyleKind::Paragraph),
    ("heading 6", StyleKind::Paragraph),
    ("Title Char", StyleKind::Character),
    ("Subtitle Char", StyleKind::Character),
    ("Heading 1 Char", StyleKind::Character),
    ("Heading 2 Char", StyleKind::Character),
    ("Heading 3 Char", StyleKind::Character),
    ("Heading 4 Char", StyleKind::Character),
    ("Heading 5 Char", StyleKind::Character),
    ("Heading 6 Char", StyleKind::Character),
    ("Block Text", StyleKind::Paragraph),
    ("Footnote Text", StyleKind::Paragraph),
    ("Footnote Block Text", StyleKind::Paragraph),
    ("Definition Term", StyleKind::Paragraph),
    ("Definition", StyleKind::Paragraph),
    ("Caption", StyleKind::Paragraph),
    ("Table Caption", StyleKind::Paragraph),
    ("Image Caption", StyleKind::Paragraph),
    ("Figure", StyleKind::Paragraph),
    ("Captioned Figure", StyleKind::Paragraph),
    ("TOC Heading", StyleKind::Paragraph),
    ("toc 1", StyleKind::Paragraph),
    ("toc 2", StyleKind::Paragraph),
    ("toc 3", StyleKind::Paragraph),
    ("toc 4", StyleKind::Paragraph),
    ("toc 5", StyleKind::Paragraph),
    ("toc 6", StyleKind::Paragraph),
    ("Source Code", StyleKind::Paragraph),
    ("footer", StyleKind::Paragraph),
    ("Default Paragraph Font", StyleKind::Character),
    ("Body Text Char", StyleKind::Character),
    ("Verbatim Char", StyleKind::Character),
    ("Section Number", StyleKind::Character),
    ("Footnote Reference", StyleKind::Character),
    ("Hyperlink", StyleKind::Character),
    ("Normal Table", StyleKind::Table),
    ("Table", StyleKind::Table),
    ("KeywordTok", StyleKind::Character),
    ("DataTypeTok", StyleKind::Character),
    ("DecValTok", StyleKind::Character),
    ("BaseNTok", StyleKind::Character),
    ("FloatTok", StyleKind::Character),
    ("ConstantTok", StyleKind::Character),
    ("CharTok", StyleKind::Character),
    ("SpecialCharTok", StyleKind::Character),
    ("StringTok", StyleKind::Character),
    ("VerbatimStringTok", StyleKind::Character),
    ("SpecialStringTok", StyleKind::Character),
    ("ImportTok", StyleKind::Character),
    ("CommentTok", StyleKind::Character),
    ("DocumentationTok", StyleKind::Character),
    ("AnnotationTok", StyleKind::Character),
    ("CommentVarTok", StyleKind::Character),
    ("OtherTok", StyleKind::Character),
    ("FunctionTok", StyleKind::Character),
    ("VariableTok", StyleKind::Character),
    ("ControlFlowTok", StyleKind::Character),
    ("OperatorTok", StyleKind::Character),
    ("BuiltInTok", StyleKind::Character),
    ("ExtensionTok", StyleKind::Character),
    ("PreprocessorTok", StyleKind::Character),
    ("AttributeTok", StyleKind::Character),
    ("RegionMarkerTok", StyleKind::Character),
    ("InformationTok", StyleKind::Character),
    ("WarningTok", StyleKind::Character),
    ("AlertTok", StyleKind::Character),
    ("ErrorTok", StyleKind::Character),
    ("NormalTok", StyleKind::Character),
    ("Callout", StyleKind::Paragraph),
    ("Callout Note", StyleKind::Paragraph),
    ("Callout Tip", StyleKind::Paragraph),
    ("Callout Important", StyleKind::Paragraph),
    ("Callout Warning", StyleKind::Paragraph),
    ("Callout Caution", StyleKind::Paragraph),
    ("Callout Title", StyleKind::Character),
    ("Equation", StyleKind::Paragraph),
    ("Horizontal Rule", StyleKind::Paragraph),
    ("Cell Prompt", StyleKind::Paragraph),
    ("Cell Output", StyleKind::Paragraph),
    ("Code Line Number", StyleKind::Character),
    ("Heading 1 Unlisted", StyleKind::Paragraph),
    ("Heading 2 Unlisted", StyleKind::Paragraph),
    ("Heading 3 Unlisted", StyleKind::Paragraph),
    ("Heading 4 Unlisted", StyleKind::Paragraph),
    ("Heading 5 Unlisted", StyleKind::Paragraph),
    ("Heading 6 Unlisted", StyleKind::Paragraph),
];

#[derive(Debug, Clone)]
struct Definition {
    id: String,
    name: String,
    kind: Option<StyleKind>,
    default: bool,
    references: Vec<(String, String)>,
    raw: String,
}

impl Definition {
    fn parse(source: &str, node: &XmlNode) -> Result<Self> {
        let id = node.attr("w:styleId").ok_or_else(|| {
            Error::Other("styles.xml has a style without w:styleId".into())
        })?;
        let name = child(node, "w:name")
            .and_then(|node| node.attr("w:val"))
            .unwrap_or(id);
        let references = node
            .children
            .iter()
            .filter(|child| {
                matches!(child.name.as_str(), "w:basedOn" | "w:next" | "w:link")
            })
            .filter_map(|child| {
                child
                    .attr("w:val")
                    .map(|value| (child.name.clone(), value.into()))
            })
            .collect();
        Ok(Self {
            id: id.into(),
            name: name.into(),
            kind: StyleKind::parse(node.attr("w:type")),
            default: matches!(
                node.attr("w:default"),
                Some("1" | "true" | "on")
            ),
            references,
            raw: source[node.range.clone()].into(),
        })
    }
}

struct Builtin {
    source: String,
    definitions: Vec<Definition>,
    by_name: BTreeMap<(StyleKind, String), usize>,
    by_id: BTreeMap<String, usize>,
}

static BUILTIN: LazyLock<Builtin> = LazyLock::new(|| {
    // The embedded definitions and their closure are checked by the tests below.
    let source = crate::vendor::get_str("docx/styles.xml")
        .expect("built-in styles asset is present and UTF-8");
    let root = style_root(&source).expect("built-in styles are valid XML");
    let definitions: Vec<_> = root
        .children
        .iter()
        .filter(|child| child.name == "w:style")
        .map(|child| {
            Definition::parse(&source, child).expect("built-in style has an id")
        })
        .collect();
    let by_name = definitions
        .iter()
        .enumerate()
        .filter_map(|(index, def)| {
            def.kind.map(|kind| ((kind, def.name.to_lowercase()), index))
        })
        .collect();
    let by_id = definitions
        .iter()
        .enumerate()
        .map(|(index, def)| (def.id.clone(), index))
        .collect();
    Builtin { source, definitions, by_name, by_id }
});

/// Case-insensitive, name-based style lookup with reference formatting intact.
///
/// Built-in definitions are appended only as needed, including their linked,
/// next and base styles. Ids are reserved before following those references so
/// a paragraph and its character counterpart can link to each other.
#[derive(Debug, Clone)]
pub struct StyleTable {
    source: String,
    append_at: usize,
    appended: Vec<String>,
    names: BTreeMap<(StyleKind, String), String>,
    ids: BTreeSet<String>,
    defaults: BTreeSet<StyleKind>,
    diagnostics: Diagnostics,
}

impl StyleTable {
    /// Starts with `docDefaults` and one default style of each kind.
    pub fn builtin() -> Self {
        // Both the source and its selected definitions are repository assets.
        let tree = style_root(&BUILTIN.source)
            .expect("checked built-in styles have a styles root");
        let mut source = format!(
            "{XML_HEAD}{}",
            &BUILTIN.source[tree.range.start..tree.open_end]
        );
        for node in &tree.children {
            if node.name == "w:docDefaults"
                || (node.name == "w:style"
                    && matches!(
                        node.attr("w:default"),
                        Some("1" | "true" | "on")
                    ))
            {
                source.push_str(&BUILTIN.source[node.range.clone()]);
            }
        }
        source.push_str("</w:styles>");
        Self::read(&source).expect("selected built-in styles are valid")
    }

    /// Keeps the reference's root and children, supplying missing known ids.
    ///
    /// A matching reference name wins entirely, including localized ids and
    /// formatting. Definitions needed by its `basedOn`, `next` and `link`
    /// references are appended when the target is one of writer's known ids.
    pub fn from_reference(styles_xml: &str) -> Result<Self> {
        let mut table = Self::read(styles_xml)?;
        let tree = style_root(styles_xml)?;
        let references: Vec<_> = tree
            .children
            .iter()
            .filter(|node| node.name == "w:style")
            .map(|node| Definition::parse(styles_xml, node))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flat_map(|def| def.references)
            .map(|(_, id)| id)
            .collect();
        for id in references {
            if !table.ids.contains(&id)
                && let Some(&index) = BUILTIN.by_id.get(&id)
            {
                let definition = &BUILTIN.definitions[index];
                let kind = definition.kind.expect("built-in style has a kind");
                if table.id_for(&definition.name, kind).is_some() {
                    // A localized name may already have another id. The raw
                    // reference still names this id, so retain both definitions.
                    table.append_builtin(index, &definition.name, false);
                } else {
                    table.ensure(&definition.name, kind);
                }
            }
        }
        Ok(table)
    }

    fn read(xml: &str) -> Result<Self> {
        let tree = style_root(xml)?;
        let root = &xml[tree.range.clone()];
        let mut source = format!("{XML_HEAD}{root}");
        if root.ends_with("/>") {
            source.truncate(source.len() - 2);
            source.push_str("></w:styles>");
        }
        let mut table = Self {
            append_at: source
                .rfind("</w:styles>")
                .expect("a normalized styles root has a closing tag"),
            source,
            appended: Vec::new(),
            names: BTreeMap::new(),
            ids: BTreeSet::new(),
            defaults: BTreeSet::new(),
            diagnostics: Diagnostics::new(),
        };
        for node in tree.children.iter().filter(|node| node.name == "w:style") {
            let def = Definition::parse(xml, node)?;
            if !table.ids.insert(def.id.clone()) {
                return Err(Error::Other(format!(
                    "styles.xml has duplicate style id {:?}",
                    def.id
                )));
            }
            if let Some(kind) = def.kind {
                table
                    .names
                    .entry((kind, def.name.to_lowercase()))
                    .or_insert(def.id);
                if def.default {
                    table.defaults.insert(kind);
                }
            }
        }
        Ok(table)
    }

    /// Looks up a name and kind without adding definitions.
    pub fn id_for(&self, name: &str, kind: StyleKind) -> Option<&str> {
        self.names.get(&(kind, name.to_lowercase())).map(String::as_str)
    }

    /// Adds the named built-in style and its dependency closure when absent.
    ///
    /// Unknown names use [`Self::custom`] and its default base style.
    pub fn ensure(&mut self, name: &str, kind: StyleKind) -> String {
        if let Some(id) = self.id_for(name, kind) {
            return id.into();
        }
        let key = (kind, name.to_lowercase());
        let Some(&index) = BUILTIN.by_name.get(&key) else {
            return self.custom(name, kind, None);
        };
        let effective =
            self.effective_name(&BUILTIN.definitions[index].name, kind);
        if let Some(id) = self.id_for(&effective, kind) {
            let id = id.to_owned();
            self.names.insert(key, id.clone());
            return id;
        }
        let id = self.append_builtin(index, &effective, true);
        self.names.insert(key, id.clone());
        id
    }

    fn append_builtin(
        &mut self, index: usize, name: &str, register_name: bool,
    ) -> String {
        let def = &BUILTIN.definitions[index];
        let kind = def.kind.expect("built-in style has a kind");
        let id = self.unique_id(&def.id);
        self.ids.insert(id.clone());
        if register_name {
            self.names.insert((kind, def.name.to_lowercase()), id.clone());
            self.names.insert((kind, name.to_lowercase()), id.clone());
        }
        let keep_default = def.default && self.defaults.insert(kind);
        let mut raw = def.raw.replacen(
            &format!("w:styleId=\"{}\"", escape(&def.id)),
            &format!("w:styleId=\"{}\"", escape(&id)),
            1,
        );
        raw = raw.replacen(
            &format!("<w:name w:val=\"{}\"/>", escape(&def.name)),
            &format!("<w:name w:val=\"{}\"/>", escape(name)),
            1,
        );
        if !keep_default {
            raw = raw.replacen(" w:default=\"1\"", "", 1);
        }
        for (tag, old_id) in &def.references {
            if let Some(&target) = BUILTIN.by_id.get(old_id) {
                let target = &BUILTIN.definitions[target];
                let target_kind =
                    target.kind.expect("built-in style has a kind");
                let new_id = self.ensure(&target.name, target_kind);
                raw = raw.replacen(
                    &format!("<{tag} w:val=\"{}\"/>", escape(old_id)),
                    &format!("<{tag} w:val=\"{}\"/>", escape(&new_id)),
                    1,
                );
            }
        }
        self.appended.push(raw);
        id
    }

    /// Adds a user style, using a named base or the category's default base.
    ///
    /// The id contains only ASCII letters and digits and is uniquified within
    /// the part. A name already defined for another kind gets a kind suffix
    /// and a `docx-style-kind-mismatch` warning.
    pub fn custom(
        &mut self, name: &str, kind: StyleKind, based_on: Option<&str>,
    ) -> String {
        if let Some(id) = self.id_for(name, kind) {
            return id.into();
        }
        if BUILTIN.by_name.contains_key(&(kind, name.to_lowercase())) {
            return self.ensure(name, kind);
        }
        let effective = self.effective_name(name, kind);
        if let Some(id) = self.id_for(&effective, kind) {
            let id = id.to_owned();
            self.names.insert((kind, name.to_lowercase()), id.clone());
            return id;
        }
        let stem: String =
            effective.chars().filter(char::is_ascii_alphanumeric).collect();
        let id = self.unique_id(if stem.is_empty() { "Style" } else { &stem });
        self.ids.insert(id.clone());
        self.names.insert((kind, name.to_lowercase()), id.clone());
        self.names.insert((kind, effective.to_lowercase()), id.clone());
        let base = self.ensure(based_on.unwrap_or(kind.base()), kind);
        let mut props = Props::new("w:style", STYLE).with_attrs(&format!(
            "w:type=\"{}\" w:customStyle=\"1\" w:styleId=\"{}\"",
            kind.xml_name(),
            escape(&id),
        ));
        props.set(
            "w:name",
            format!("<w:name w:val=\"{}\"/>", escape(&effective)),
        );
        props.set(
            "w:basedOn",
            format!("<w:basedOn w:val=\"{}\"/>", escape(&base)),
        );
        self.appended.push(props.to_xml());
        id
    }

    fn effective_name(&mut self, name: &str, kind: StyleKind) -> String {
        let lower = name.to_lowercase();
        if self
            .names
            .keys()
            .any(|(other, existing)| *other != kind && *existing == lower)
        {
            let effective = format!("{name} {}", kind.suffix());
            self.diagnostics.warn("docx-style-kind-mismatch", format!(
                "DOCX style {name:?} has another kind; using {effective:?} for a {} style",
                kind.xml_name(),
            ));
            effective
        } else {
            name.into()
        }
    }

    fn unique_id(&self, stem: &str) -> String {
        if !self.ids.contains(stem) {
            return stem.into();
        }
        for suffix in 1_u64.. {
            let id = format!("{stem}{suffix}");
            if !self.ids.contains(&id) {
                return id;
            }
        }
        unreachable!("a finite style table cannot occupy all suffixes")
    }

    /// Edits only the default run language, adding missing parents if needed.
    pub fn set_lang(&mut self, lang: &str) {
        // Constructors validate this XML; each edit below inserts escaped XML.
        let tree =
            style_root(&self.source).expect("style source remains valid");
        let value = format!("<w:lang w:val=\"{}\"/>", escape(lang));
        if let Some(defaults) = child(&tree, "w:docDefaults") {
            if let Some(run_default) = child(defaults, "w:rPrDefault") {
                if let Some(props) = child(run_default, "w:rPr") {
                    if let Some(language) = child(props, "w:lang") {
                        set_attribute(
                            &mut self.source, language, "w:val", lang,
                        );
                    } else {
                        let rank = RPR
                            .iter()
                            .position(|name| *name == "w:lang")
                            .expect("w:lang is in RPR");
                        let before = props
                            .children
                            .iter()
                            .find(|node| {
                                RPR.iter()
                                    .position(|name| *name == node.name)
                                    .is_some_and(|index| index > rank)
                            })
                            .map(|node| node.range.start);
                        insert_child(&mut self.source, props, &value, before);
                    }
                } else {
                    insert_child(
                        &mut self.source,
                        run_default,
                        &format!("<w:rPr>{value}</w:rPr>"),
                        None,
                    );
                }
            } else {
                let value = format!(
                    "<w:rPrDefault><w:rPr>{value}</w:rPr></w:rPrDefault>"
                );
                let before =
                    defaults.children.first().map(|node| node.range.start);
                insert_child(&mut self.source, defaults, &value, before);
            }
        } else {
            let value = format!(
                "<w:docDefaults><w:rPrDefault><w:rPr>{value}</w:rPr></w:rPrDefault></w:docDefaults>"
            );
            let before = tree.children.first().map(|node| node.range.start);
            insert_child(&mut self.source, &tree, &value, before);
        }
        self.append_at = self
            .source
            .rfind("</w:styles>")
            .expect("language insertion preserves the styles root");
    }

    /// Serializes preserved reference XML followed by appended definitions.
    pub fn to_xml(&self) -> String {
        let mut xml = self.source[..self.append_at].to_owned();
        for style in &self.appended {
            xml.push_str(style);
        }
        xml.push_str(&self.source[self.append_at..]);
        xml
    }

    pub(crate) fn take_diagnostics(&mut self) -> Diagnostics {
        std::mem::take(&mut self.diagnostics)
    }
}

fn style_root(source: &str) -> Result<XmlNode> {
    let tree = parse_xml(source).map_err(|error| {
        Error::Other(format!("invalid styles.xml: {error}"))
    })?;
    if tree.name != "w:styles" || tree.attr("xmlns:w") != Some(NS_W) {
        return Err(Error::Other(
            "styles.xml must have a WordprocessingML w:styles root".into(),
        ));
    }
    Ok(tree)
}

fn child<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    node.children.iter().find(|node| node.name == name)
}

fn insert_child(
    source: &mut String, parent: &XmlNode, xml: &str, before: Option<usize>,
) {
    let opening = &source[parent.range.start..parent.open_end];
    if let Some(opening) = opening.strip_suffix("/>") {
        let replacement = format!("{opening}>{xml}</{}>", parent.name);
        source.replace_range(parent.range.clone(), &replacement);
    } else {
        source.insert_str(before.unwrap_or(parent.close_start), xml);
    }
}

fn set_attribute(source: &mut String, node: &XmlNode, name: &str, value: &str) {
    let opening = &source[node.range.start..node.open_end];
    let value = escape(value).replace('\'', "&apos;");
    if let Some(range) = attribute_value(opening, name) {
        source.replace_range(
            node.range.start + range.start..node.range.start + range.end,
            &value,
        );
    } else {
        let end = node.open_end - if opening.ends_with("/>") { 2 } else { 1 };
        source.insert_str(end, &format!(" {name}=\"{value}\""));
    }
}

// Lexical offsets let set_lang change one value while retaining the reference's
// quoting, whitespace, other language attributes and child bytes exactly.
fn attribute_value(opening: &str, name: &str) -> Option<Range<usize>> {
    let bytes = opening.as_bytes();
    let mut cursor = 1;
    while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let start = cursor;
        while cursor < bytes.len()
            && !bytes[cursor].is_ascii_whitespace()
            && !matches!(bytes[cursor], b'=' | b'/' | b'>')
        {
            cursor += 1;
        }
        if start == cursor {
            return None;
        }
        let key = &opening[start..cursor];
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b'=')
        {
            cursor += 1;
        }
        let quote = *bytes.get(cursor)?;
        cursor += 1;
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor] != quote {
            cursor += 1;
        }
        if key == name {
            return Some(start..cursor);
        }
        cursor += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_xml(children: &str) -> String {
        format!("<w:styles xmlns:w=\"{NS_W}\">{children}</w:styles>")
    }

    fn definitions(source: &str) -> Vec<Definition> {
        let root = style_root(source).unwrap();
        root.children
            .iter()
            .filter(|node| node.name == "w:style")
            .map(|node| Definition::parse(source, node).unwrap())
            .collect()
    }

    #[test]
    fn builtin_asset_matches_vocabulary() {
        let source = crate::vendor::get_str("docx/styles.xml").unwrap();
        let definitions = definitions(&source);
        assert_eq!(definitions.len(), 101);
        assert_eq!(VOCABULARY.len(), 101);
        let actual: Vec<_> = definitions
            .iter()
            .map(|def| (def.name.as_str(), def.kind.unwrap()))
            .collect();
        assert_eq!(actual, VOCABULARY);
        assert_eq!(definitions.iter().filter(|def| def.default).count(), 3);
        for (row, definition) in EXPECTED.lines().zip(&definitions) {
            let fields: Vec<_> = row.split('|').collect();
            assert_eq!(fields.len(), 8, "{row}");
            assert_eq!(definition.name, fields[0]);
            assert_eq!(definition.id, fields[1]);
            assert_eq!(definition.kind.unwrap().xml_name(), fields[2]);
            let raw = reference_xml(&definition.raw);
            let root = parse_xml(&raw).unwrap();
            let style = &root.children[0];
            for (tag, expected) in [
                ("w:basedOn", fields[3]),
                ("w:next", fields[4]),
                ("w:link", fields[5]),
            ] {
                let actual =
                    child(style, tag).and_then(|node| node.attr("w:val"));
                assert_eq!(
                    actual,
                    (!expected.is_empty()).then_some(expected),
                    "{} {tag}",
                    definition.name
                );
            }
            assert_eq!(
                style.attr("w:customStyle") == Some("1"),
                fields[6] == "1",
                "{} customStyle",
                definition.name
            );
            assert_eq!(
                definition.default,
                fields[7] == "1",
                "{} default",
                definition.name
            );
        }
        assert_eq!(EXPECTED.lines().count(), definitions.len());
    }

    #[test]
    fn builtin_has_no_theme_attributes() {
        let source = crate::vendor::get_str("docx/styles.xml").unwrap();
        let tree = parse_xml(&source).unwrap();
        let mut pending = vec![&tree];
        while let Some(node) = pending.pop() {
            assert!(
                node.attrs
                    .keys()
                    .all(|key| !key.to_lowercase().contains("theme")),
                "theme attribute in {}",
                node.name
            );
            if node.name == "w:rFonts" {
                for key in ["w:ascii", "w:hAnsi", "w:cs"] {
                    assert!(node.attr(key).is_some());
                }
            }
            pending.extend(&node.children);
        }
        let defaults = child(&tree, "w:docDefaults").unwrap();
        let props =
            child(child(defaults, "w:rPrDefault").unwrap(), "w:rPr").unwrap();
        assert_eq!(child(props, "w:lang").unwrap().attr("w:val"), Some("en"));
        assert_eq!(
            child(props, "w:rFonts").unwrap().attr("w:ascii"),
            Some("Calibri")
        );
        assert_eq!(child(props, "w:sz").unwrap().attr("w:val"), Some("22"));
        assert_eq!(child(props, "w:szCs").unwrap().attr("w:val"), Some("22"));
    }

    #[test]
    fn token_formatting_and_heading_pairs() {
        let definitions = &BUILTIN.definitions;
        for (name, flags, colour) in [
            ("KeywordTok", "b", "007020"),
            ("DataTypeTok", "", "902000"),
            ("DecValTok", "", "40A070"),
            ("BaseNTok", "", "40A070"),
            ("FloatTok", "", "40A070"),
            ("ConstantTok", "", "880000"),
            ("CharTok", "", "4070A0"),
            ("SpecialCharTok", "", "4070A0"),
            ("StringTok", "", "4070A0"),
            ("VerbatimStringTok", "", "4070A0"),
            ("SpecialStringTok", "", "BB6688"),
            ("ImportTok", "b", "008000"),
            ("CommentTok", "i", "60A0B0"),
            ("DocumentationTok", "i", "BA2121"),
            ("AnnotationTok", "bi", "60A0B0"),
            ("CommentVarTok", "bi", "60A0B0"),
            ("OtherTok", "", "007020"),
            ("FunctionTok", "", "06287E"),
            ("VariableTok", "", "19177C"),
            ("ControlFlowTok", "b", "007020"),
            ("OperatorTok", "", "666666"),
            ("BuiltInTok", "", "008000"),
            ("ExtensionTok", "", ""),
            ("PreprocessorTok", "", "BC7A00"),
            ("AttributeTok", "", "7D9029"),
            ("RegionMarkerTok", "", ""),
            ("InformationTok", "bi", "60A0B0"),
            ("WarningTok", "bi", "60A0B0"),
            ("AlertTok", "b", "FF0000"),
            ("ErrorTok", "b", "FF0000"),
            ("NormalTok", "", ""),
        ] {
            let def = definitions.iter().find(|def| def.name == name).unwrap();
            let mut expected = String::new();
            if flags.contains('b') {
                expected.push_str("<w:b/>");
            }
            if flags.contains('i') {
                expected.push_str("<w:i/>");
            }
            if !colour.is_empty() {
                expected.push_str(&format!("<w:color w:val=\"{colour}\"/>"));
            }
            let root = style_root(&reference_xml(&def.raw)).unwrap();
            let actual = child(&root.children[0], "w:rPr");
            if expected.is_empty() {
                assert!(actual.is_none(), "{name}");
            } else {
                let xml = reference_xml(&def.raw);
                let props = actual.unwrap();
                assert_eq!(
                    &xml[props.range.clone()],
                    format!("<w:rPr>{expected}</w:rPr>"),
                    "{name}"
                );
            }
        }
        for id in [
            "Title", "Subtitle", "Heading1", "Heading2", "Heading3",
            "Heading4", "Heading5", "Heading6",
        ] {
            let paragraph = &definitions[BUILTIN.by_id[id]];
            let character = &definitions[BUILTIN.by_id[&format!("{id}Char")]];
            let paragraph_xml = reference_xml(&paragraph.raw);
            let character_xml = reference_xml(&character.raw);
            let p = style_root(&paragraph_xml).unwrap();
            let c = style_root(&character_xml).unwrap();
            let p = child(&p.children[0], "w:rPr").unwrap();
            let c = child(&c.children[0], "w:rPr").unwrap();
            assert_eq!(
                &paragraph_xml[p.range.clone()],
                &character_xml[c.range.clone()],
                "{id}"
            );
        }
    }

    #[test]
    fn ensure_appends_the_closure() {
        let mut table = StyleTable::builtin();
        assert_eq!(definitions(&table.to_xml()).len(), 3);
        assert!(
            table.id_for("First Paragraph", StyleKind::Paragraph).is_none()
        );
        assert_eq!(
            table.ensure("First Paragraph", StyleKind::Paragraph),
            "FirstParagraph"
        );
        for (name, kind, id) in [
            ("Body Text", StyleKind::Paragraph, "BodyText"),
            ("Body Text Char", StyleKind::Character, "BodyTextChar"),
        ] {
            assert_eq!(table.id_for(name, kind), Some(id));
        }
        let xml = table.to_xml();
        assert_eq!(definitions(&xml).len(), 6);
        assert!(xml.contains("<w:link w:val=\"BodyTextChar\"/>"));
        assert!(xml.contains("<w:link w:val=\"BodyText\"/>"));
        table.ensure("First Paragraph", StyleKind::Paragraph);
        assert_eq!(table.to_xml(), xml);
    }

    #[test]
    fn custom_ids() {
        let mut table = StyleTable::builtin();
        assert_eq!(
            table.custom("Callout Danger", StyleKind::Paragraph, None),
            "CalloutDanger"
        );
        assert_eq!(table.custom("", StyleKind::Character, None), "Style");
        assert_eq!(
            table.custom("CalloutDanger", StyleKind::Paragraph, None),
            "CalloutDanger1"
        );
        assert_eq!(table.custom("Tabular", StyleKind::Table, None), "Tabular");
        let definitions = definitions(&table.to_xml());
        for (name, base) in [
            ("Callout Danger", "BodyText"),
            ("", "BodyTextChar"),
            ("Tabular", "Table"),
        ] {
            let def = definitions.iter().find(|def| def.name == name).unwrap();
            assert!(
                def.references.contains(&("w:basedOn".into(), base.into()))
            );
        }
        let id = table.custom(
            "Explicit Base",
            StyleKind::Paragraph,
            Some("Caption"),
        );
        assert!(table.to_xml().contains(&format!(
            "w:styleId=\"{id}\"><w:name w:val=\"Explicit Base\"/><w:basedOn w:val=\"Caption\"/>"
        )));
    }

    #[test]
    fn names_are_case_insensitive() {
        let mut table = StyleTable::builtin();
        let id = table.ensure("BODY text", StyleKind::Paragraph);
        assert_eq!(
            table.id_for("Body Text", StyleKind::Paragraph),
            Some(id.as_str())
        );
        assert_eq!(table.ensure("body TEXT", StyleKind::Paragraph), id);
        assert!(table.to_xml().contains("<w:name w:val=\"Body Text\"/>"));
        let custom = table.custom("Überschrift", StyleKind::Paragraph, None);
        assert_eq!(
            table.custom("ÜBERSCHRIFT", StyleKind::Paragraph, None),
            custom
        );
    }

    #[test]
    fn kind_mismatch_renames_and_warns() {
        let reference = reference_xml(concat!(
            "<w:style w:type=\"character\" w:styleId=\"Existing\">",
            "<w:name w:val=\"Body Text\"/></w:style>",
        ));
        let mut table = StyleTable::from_reference(&reference).unwrap();
        let id = table.ensure("Body Text", StyleKind::Paragraph);
        assert_ne!(id, "Existing");
        let output = table.to_xml();
        assert!(output.contains("<w:name w:val=\"Body Text Paragraph\"/>"));
        assert!(output.contains("w:styleId=\"Existing\""));
        assert_eq!(table.ensure("Body Text", StyleKind::Paragraph), id);
        let diagnostics = table.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        let warning = diagnostics.iter().next().unwrap();
        assert_eq!(warning.code, "docx-style-kind-mismatch");
        assert!(warning.message.contains("Body Text Paragraph"));
        assert!(table.take_diagnostics().is_empty());
        for (kind, suffix) in
            [(StyleKind::Character, "Char"), (StyleKind::Table, "Table")]
        {
            let mut table = StyleTable::builtin();
            let id = table.custom("Normal", kind, None);
            assert!(
                table
                    .to_xml()
                    .contains(&format!("<w:name w:val=\"Normal {suffix}\"/>"))
            );
            assert_eq!(table.id_for("Normal", kind), Some(id.as_str()));
            assert_eq!(table.take_diagnostics().len(), 1);
        }
    }

    #[test]
    fn kind_mismatch_reuses_existing_suffix_once() {
        let mut table = StyleTable::builtin();
        let before = table.to_xml();
        let id = table.custom("Normal", StyleKind::Table, None);
        assert_eq!(id, "TableNormal");
        assert_eq!(table.to_xml(), before);
        assert_eq!(
            table.id_for("Normal", StyleKind::Table),
            Some("TableNormal")
        );
        assert_eq!(table.custom("normal", StyleKind::Table, None), id);
        assert_eq!(table.take_diagnostics().len(), 1);
        assert!(table.take_diagnostics().is_empty());

        let reference = reference_xml(concat!(
            "<w:style w:type=\"character\" w:styleId=\"Existing\">",
            "<w:name w:val=\"Body Text\"/></w:style>",
            "<w:style w:type=\"paragraph\" w:styleId=\"ExistingSuffix\">",
            "<w:name w:val=\"Body Text Paragraph\"/></w:style>",
        ));
        let mut table = StyleTable::from_reference(&reference).unwrap();
        let before = table.to_xml();
        assert_eq!(
            table.ensure("Body Text", StyleKind::Paragraph),
            "ExistingSuffix"
        );
        assert_eq!(
            table.id_for("Body Text", StyleKind::Paragraph),
            Some("ExistingSuffix")
        );
        assert_eq!(
            table.ensure("body text", StyleKind::Paragraph),
            "ExistingSuffix"
        );
        assert_eq!(table.to_xml(), before);
        assert_eq!(table.take_diagnostics().len(), 1);
    }

    #[test]
    fn reference_style_wins() {
        let style = concat!(
            "<w:style w:type=\"paragraph\" w:styleId=\"LocalHeading\">",
            "<w:name w:val=\"heading 1\"/><w:rPr>",
            "<w:color w:val=\"FF0000\"/></w:rPr></w:style>",
        );
        let reference = reference_xml(style);
        let mut table = StyleTable::from_reference(&reference).unwrap();
        assert_eq!(
            table.ensure("Heading 1", StyleKind::Paragraph),
            "LocalHeading"
        );
        assert_eq!(table.to_xml(), format!("{XML_HEAD}{reference}"));
        assert!(table.to_xml().contains(style));
    }

    #[test]
    fn missing_based_on_is_appended() {
        let reference = reference_xml(concat!(
            "<w:style w:type=\"table\" w:styleId=\"Table\">",
            "<w:name w:val=\"Table\"/><w:basedOn w:val=\"TableNormal\"/>",
            "</w:style>",
        ));
        let table = StyleTable::from_reference(&reference).unwrap();
        assert_eq!(
            table.id_for("Normal Table", StyleKind::Table),
            Some("TableNormal")
        );
        assert_eq!(definitions(&table.to_xml()).len(), 2);
        assert!(table.to_xml().contains("w:styleId=\"TableNormal\""));
    }

    #[test]
    fn no_second_default() {
        let reference = reference_xml(concat!(
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"P\">",
            "<w:name w:val=\"Reference P\"/></w:style>",
            "<w:style w:type=\"character\" w:default=\"1\" w:styleId=\"C\">",
            "<w:name w:val=\"Reference C\"/></w:style>",
            "<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"T\">",
            "<w:name w:val=\"Reference T\"/></w:style>",
        ));
        let mut table = StyleTable::from_reference(&reference).unwrap();
        for &(name, kind) in VOCABULARY {
            table.ensure(name, kind);
        }
        let definitions = definitions(&table.to_xml());
        assert_eq!(definitions.iter().filter(|def| def.default).count(), 3);
        for kind in
            [StyleKind::Paragraph, StyleKind::Character, StyleKind::Table]
        {
            assert_eq!(
                definitions
                    .iter()
                    .filter(|def| def.kind == Some(kind) && def.default)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn localized_ids_are_rewritten() {
        let reference = reference_xml(concat!(
            "<w:style w:type=\"paragraph\" w:styleId=\"berschrift1\">",
            "<w:name w:val=\"heading 1\"/></w:style>",
        ));
        let mut table = StyleTable::from_reference(&reference).unwrap();
        table.ensure("Heading 1 Char", StyleKind::Character);
        let definitions = definitions(&table.to_xml());
        let char_style = definitions
            .iter()
            .find(|def| def.name == "Heading 1 Char")
            .unwrap();
        assert!(
            char_style
                .references
                .contains(&("w:link".into(), "berschrift1".into()))
        );
        assert!(definitions.iter().all(|def| def.id != "Heading1"));
    }

    #[test]
    fn set_lang_edits_or_inserts() {
        let unrelated = concat!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Kept\">",
            "<w:name w:val=\"Kept\"/><w:rPr><w:lang w:val=\"fr\"/>",
            "</w:rPr></w:style>",
        );
        let defaults = concat!(
            "<w:docDefaults><w:rPrDefault><w:rPr>",
            "<w:lang w:val='en' w:eastAsia='ja'/>",
            "</w:rPr></w:rPrDefault></w:docDefaults>",
        );
        let reference = reference_xml(&format!("{defaults}{unrelated}"));
        let mut table = StyleTable::from_reference(&reference).unwrap();
        table.set_lang("de-CH");
        assert_eq!(
            table.to_xml(),
            format!(
                "{XML_HEAD}{}",
                reference.replace("w:val='en'", "w:val='de-CH'")
            )
        );
        for parents in [
            "",
            "<w:docDefaults/>",
            "<w:docDefaults><w:rPrDefault/></w:docDefaults>",
            "<w:docDefaults><w:rPrDefault><w:rPr/></w:rPrDefault></w:docDefaults>",
            "<w:docDefaults><w:pPrDefault><w:pPr/></w:pPrDefault></w:docDefaults>",
            concat!(
                "<w:docDefaults><w:rPrDefault><w:rPr>",
                "<w:eastAsianLayout w:combine=\"1\"/>",
                "</w:rPr></w:rPrDefault></w:docDefaults>",
            ),
        ] {
            let mut table =
                StyleTable::from_reference(&reference_xml(parents)).unwrap();
            table.set_lang("pt-BR");
            let xml = table.to_xml();
            let tree = parse_xml(&xml).unwrap();
            let defaults = child(&tree, "w:docDefaults").unwrap();
            let run_default = child(defaults, "w:rPrDefault").unwrap();
            let props = child(run_default, "w:rPr").unwrap();
            assert_eq!(
                child(props, "w:lang").unwrap().attr("w:val"),
                Some("pt-BR")
            );
            assert_eq!(defaults.children[0].name, "w:rPrDefault");
            assert_eq!(props.children[0].name, "w:lang");
            table.set_lang("en");
            assert_eq!(table.to_xml().matches("<w:lang ").count(), 1);
        }
    }

    #[test]
    fn builtin_id_collisions_are_uniquified() {
        let reference = reference_xml(concat!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\">",
            "<w:name w:val=\"Other\"/></w:style>",
        ));
        let mut table = StyleTable::from_reference(&reference).unwrap();
        assert_eq!(
            table.ensure("heading 1", StyleKind::Paragraph),
            "Heading11"
        );
        let definitions = definitions(&table.to_xml());
        let linked = definitions
            .iter()
            .find(|def| def.name == "Heading 1 Char")
            .unwrap();
        assert!(
            linked.references.contains(&("w:link".into(), "Heading11".into()))
        );
    }

    #[test]
    fn language_values_and_custom_names_are_escaped() {
        let reference = reference_xml(concat!(
            "<w:docDefaults><w:rPrDefault><w:rPr>",
            "<w:lang w:val='en'/></w:rPr></w:rPrDefault></w:docDefaults>",
        ));
        let mut table = StyleTable::from_reference(&reference).unwrap();
        table.set_lang("a'\"<&>");
        table.custom("A \"quote\" & <angle>", StyleKind::Paragraph, None);
        let xml = table.to_xml();
        assert!(parse_xml(&xml).is_ok());
        assert!(xml.contains("w:val='a&apos;&quot;&lt;&amp;&gt;'"));
    }

    #[test]
    fn all_styles_have_resolvable_dependencies() {
        let mut table = StyleTable::builtin();
        for &(name, kind) in VOCABULARY {
            table.ensure(name, kind);
        }
        let definitions = definitions(&table.to_xml());
        assert_eq!(definitions.len(), 101);
        let ids: BTreeSet<_> =
            definitions.iter().map(|def| def.id.as_str()).collect();
        assert_eq!(ids.len(), 101);
        for def in &definitions {
            for (_, target) in &def.references {
                assert!(
                    ids.contains(target.as_str()),
                    "{} -> {target}",
                    def.name
                );
            }
        }
    }

    // name | id | type | basedOn | next | link | custom | default
    const EXPECTED: &str = "\
Normal|Normal|paragraph||||0|1
Body Text|BodyText|paragraph|Normal||BodyTextChar|0|0
First Paragraph|FirstParagraph|paragraph|BodyText|BodyText||1|0
Compact|Compact|paragraph|BodyText|||1|0
Title|Title|paragraph|Normal|BodyText|TitleChar|0|0
Subtitle|Subtitle|paragraph|Title|BodyText|SubtitleChar|0|0
Author|Author|paragraph|Title|BodyText||1|0
Date|Date|paragraph|Title|BodyText||0|0
Abstract Title|AbstractTitle|paragraph|Normal|Abstract||1|0
Abstract|Abstract|paragraph|Normal|BodyText||1|0
Bibliography|Bibliography|paragraph|Normal|Bibliography||0|0
heading 1|Heading1|paragraph|Normal|BodyText|Heading1Char|0|0
heading 2|Heading2|paragraph|Normal|BodyText|Heading2Char|0|0
heading 3|Heading3|paragraph|Normal|BodyText|Heading3Char|0|0
heading 4|Heading4|paragraph|Normal|BodyText|Heading4Char|0|0
heading 5|Heading5|paragraph|Normal|BodyText|Heading5Char|0|0
heading 6|Heading6|paragraph|Normal|BodyText|Heading6Char|0|0
Title Char|TitleChar|character|DefaultParagraphFont||Title|0|0
Subtitle Char|SubtitleChar|character|DefaultParagraphFont||Subtitle|0|0
Heading 1 Char|Heading1Char|character|DefaultParagraphFont||Heading1|0|0
Heading 2 Char|Heading2Char|character|DefaultParagraphFont||Heading2|0|0
Heading 3 Char|Heading3Char|character|DefaultParagraphFont||Heading3|0|0
Heading 4 Char|Heading4Char|character|DefaultParagraphFont||Heading4|0|0
Heading 5 Char|Heading5Char|character|DefaultParagraphFont||Heading5|0|0
Heading 6 Char|Heading6Char|character|DefaultParagraphFont||Heading6|0|0
Block Text|BlockText|paragraph|BodyText|BodyText||0|0
Footnote Text|FootnoteText|paragraph|Normal|FootnoteText||0|0
Footnote Block Text|FootnoteBlockText|paragraph|FootnoteText|FootnoteText||0|0
Definition Term|DefinitionTerm|paragraph|Normal|Definition||1|0
Definition|Definition|paragraph|Normal|||1|0
Caption|Caption|paragraph|Normal|||0|0
Table Caption|TableCaption|paragraph|Caption|||1|0
Image Caption|ImageCaption|paragraph|Caption|||1|0
Figure|Figure|paragraph|Normal|||1|0
Captioned Figure|CaptionedFigure|paragraph|Figure|||1|0
TOC Heading|TOCHeading|paragraph|Heading1|BodyText||0|0
toc 1|TOC1|paragraph|Normal|BodyText||0|0
toc 2|TOC2|paragraph|Normal|BodyText||0|0
toc 3|TOC3|paragraph|Normal|BodyText||0|0
toc 4|TOC4|paragraph|Normal|BodyText||0|0
toc 5|TOC5|paragraph|Normal|BodyText||0|0
toc 6|TOC6|paragraph|Normal|BodyText||0|0
Source Code|SourceCode|paragraph|Normal||VerbatimChar|1|0
footer|Footer|paragraph|Normal|||0|0
Default Paragraph Font|DefaultParagraphFont|character||||0|1
Body Text Char|BodyTextChar|character|DefaultParagraphFont||BodyText|0|0
Verbatim Char|VerbatimChar|character|BodyTextChar|||1|0
Section Number|SectionNumber|character|BodyTextChar|||1|0
Footnote Reference|FootnoteReference|character|BodyTextChar|||0|0
Hyperlink|Hyperlink|character|BodyTextChar|||0|0
Normal Table|TableNormal|table||||0|1
Table|Table|table|TableNormal|||0|0
KeywordTok|KeywordTok|character|VerbatimChar|||1|0
DataTypeTok|DataTypeTok|character|VerbatimChar|||1|0
DecValTok|DecValTok|character|VerbatimChar|||1|0
BaseNTok|BaseNTok|character|VerbatimChar|||1|0
FloatTok|FloatTok|character|VerbatimChar|||1|0
ConstantTok|ConstantTok|character|VerbatimChar|||1|0
CharTok|CharTok|character|VerbatimChar|||1|0
SpecialCharTok|SpecialCharTok|character|VerbatimChar|||1|0
StringTok|StringTok|character|VerbatimChar|||1|0
VerbatimStringTok|VerbatimStringTok|character|VerbatimChar|||1|0
SpecialStringTok|SpecialStringTok|character|VerbatimChar|||1|0
ImportTok|ImportTok|character|VerbatimChar|||1|0
CommentTok|CommentTok|character|VerbatimChar|||1|0
DocumentationTok|DocumentationTok|character|VerbatimChar|||1|0
AnnotationTok|AnnotationTok|character|VerbatimChar|||1|0
CommentVarTok|CommentVarTok|character|VerbatimChar|||1|0
OtherTok|OtherTok|character|VerbatimChar|||1|0
FunctionTok|FunctionTok|character|VerbatimChar|||1|0
VariableTok|VariableTok|character|VerbatimChar|||1|0
ControlFlowTok|ControlFlowTok|character|VerbatimChar|||1|0
OperatorTok|OperatorTok|character|VerbatimChar|||1|0
BuiltInTok|BuiltInTok|character|VerbatimChar|||1|0
ExtensionTok|ExtensionTok|character|VerbatimChar|||1|0
PreprocessorTok|PreprocessorTok|character|VerbatimChar|||1|0
AttributeTok|AttributeTok|character|VerbatimChar|||1|0
RegionMarkerTok|RegionMarkerTok|character|VerbatimChar|||1|0
InformationTok|InformationTok|character|VerbatimChar|||1|0
WarningTok|WarningTok|character|VerbatimChar|||1|0
AlertTok|AlertTok|character|VerbatimChar|||1|0
ErrorTok|ErrorTok|character|VerbatimChar|||1|0
NormalTok|NormalTok|character|VerbatimChar|||1|0
Callout|Callout|paragraph|BodyText|||1|0
Callout Note|CalloutNote|paragraph|Callout|||1|0
Callout Tip|CalloutTip|paragraph|Callout|||1|0
Callout Important|CalloutImportant|paragraph|Callout|||1|0
Callout Warning|CalloutWarning|paragraph|Callout|||1|0
Callout Caution|CalloutCaution|paragraph|Callout|||1|0
Callout Title|CalloutTitle|character|DefaultParagraphFont|||1|0
Equation|Equation|paragraph|Normal|||1|0
Horizontal Rule|HorizontalRule|paragraph|Normal|||1|0
Cell Prompt|CellPrompt|paragraph|Normal|||1|0
Cell Output|CellOutput|paragraph|SourceCode|||1|0
Code Line Number|CodeLineNumber|character|VerbatimChar|||1|0
Heading 1 Unlisted|Heading1Unlisted|paragraph|Heading1|BodyText||1|0
Heading 2 Unlisted|Heading2Unlisted|paragraph|Heading2|BodyText||1|0
Heading 3 Unlisted|Heading3Unlisted|paragraph|Heading3|BodyText||1|0
Heading 4 Unlisted|Heading4Unlisted|paragraph|Heading4|BodyText||1|0
Heading 5 Unlisted|Heading5Unlisted|paragraph|Heading5|BodyText||1|0
Heading 6 Unlisted|Heading6Unlisted|paragraph|Heading6|BodyText||1|0
";
}
