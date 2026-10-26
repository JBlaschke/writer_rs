//! Reference DOCX loading and merging (docx-output.md §6).
//! Owner: T04. Stub created by T00.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::Path;

use zip::ZipArchive;

use crate::error::{Error, Result};
use crate::vendor;

use super::package::Relationship;
use super::validate::{XmlNode, parse_xml, resolve_target};
use super::xml::{
    NS_M, NS_O, NS_R, NS_SL, NS_V, NS_W, NS_W10, REL_FONT_TABLE, REL_FOOTER,
    REL_HEADER, REL_THEME, REL_WEB_SETTINGS, SETTINGS, XML_HEAD, escape, field,
};
use super::{Ids, PageLayout};

/// Validated parts and retained relationships from a reference document.
#[derive(Debug, Clone)]
pub(crate) struct Reference {
    pub(crate) styles_xml: String,
    pub(crate) numbering_xml: Option<String>,
    pub(crate) settings_xml: Option<String>,
    pub(crate) copied_parts: BTreeMap<String, (Vec<u8>, String)>,
    pub(crate) copied_defaults: BTreeMap<String, String>,
    pub(crate) document_namespaces: BTreeMap<String, String>,
    pub(crate) kept_rels: Vec<Relationship>,
    pub(crate) sect_pr: Option<String>,
    pub(crate) ids: Ids,
    section: Option<XmlNode>,
}

impl Reference {
    /// Reads a complete, well-formed reference package, mapping every failure
    /// to the source document rather than leaking ZIP or XML implementation errors.
    pub(crate) fn load(path: &Path) -> Result<Self> {
        Self::read(path).map_err(|message| Error::Resource {
            what: "reference document",
            path: path.into(),
            message,
        })
    }

    fn read(path: &Path) -> std::result::Result<Self, String> {
        let parts = archive_parts(path)?;
        let trees = parse_parts(&parts)?;
        let document_namespaces = trees["word/document.xml"]
            .attrs
            .iter()
            .filter(|(name, _)| name.starts_with("xmlns:"))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let types = ContentTypes::parse(&trees["[Content_Types].xml"])?;
        let relationships =
            relationships(&trees["word/_rels/document.xml.rels"])?;
        let kept_rels: Vec<_> = relationships
            .into_iter()
            .filter(|rel| {
                [
                    REL_THEME, REL_FONT_TABLE, REL_WEB_SETTINGS, REL_HEADER,
                    REL_FOOTER,
                ]
                .contains(&rel.kind.as_str())
            })
            .collect();
        let mut needed = BTreeSet::new();
        for rel in &kept_rels {
            if !rel.external {
                copy_closure(
                    "word/document.xml", &rel.target, &parts, &trees,
                    &mut needed,
                )?;
            }
        }
        let mut copied_parts = BTreeMap::new();
        let mut copied_defaults = BTreeMap::new();
        let mut ids = Ids { bookmark: 1, doc_pr: 1, footnote: 1 };
        for name in needed {
            let (content_type, default) = types.for_part(&name)?;
            if let Some(extension) = default {
                copied_defaults.insert(extension, content_type.clone());
            }
            copied_parts
                .insert(name.clone(), (parts[&name].clone(), content_type));
            if let Some(tree) = trees.get(&name) {
                update_ids(tree, &mut ids);
            }
        }
        let document = text_part(&parts, "word/document.xml")?;
        let section = body_section_node(&trees["word/document.xml"]).cloned();
        let sect_pr =
            section.as_ref().map(|node| document[node.range.clone()].into());
        Ok(Self {
            styles_xml: text_part(&parts, "word/styles.xml")?,
            numbering_xml: parts
                .contains_key("word/numbering.xml")
                .then(|| text_part(&parts, "word/numbering.xml"))
                .transpose()?,
            settings_xml: parts
                .contains_key("word/settings.xml")
                .then(|| text_part(&parts, "word/settings.xml"))
                .transpose()?,
            copied_parts,
            copied_defaults,
            document_namespaces,
            kept_rels,
            sect_pr,
            ids,
            section,
        })
    }
}

fn archive_parts(
    path: &Path,
) -> std::result::Result<BTreeMap<String, Vec<u8>>, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| error.to_string())?;
    archive.validate_header_consistency().map_err(|error| error.to_string())?;
    let mut parts = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().map_err(|e| e.to_string())?.into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if parts.insert(name.clone(), bytes).is_some() {
            return Err(format!("duplicate part {name:?}"));
        }
    }
    for name in [
        "[Content_Types].xml", "word/document.xml", "word/styles.xml",
        "word/_rels/document.xml.rels",
    ] {
        if !parts.contains_key(name) {
            return Err(format!("missing required part {name:?}"));
        }
    }
    Ok(parts)
}

fn parse_parts(
    parts: &BTreeMap<String, Vec<u8>>,
) -> std::result::Result<BTreeMap<String, XmlNode>, String> {
    let mut trees = BTreeMap::new();
    for (name, bytes) in parts {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            let xml = std::str::from_utf8(bytes)
                .map_err(|error| format!("{name}: {error}"))?;
            let tree =
                parse_xml(xml).map_err(|error| format!("{name}: {error}"))?;
            check_namespaces(name, &tree)?;
            trees.insert(name.clone(), tree);
        }
    }
    Ok(trees)
}

fn check_namespaces(
    name: &str, root: &XmlNode,
) -> std::result::Result<(), String> {
    let expected_root = match name {
        "word/document.xml" => Some("w:document"),
        "word/styles.xml" => Some("w:styles"),
        "word/numbering.xml" => Some("w:numbering"),
        "word/settings.xml" => Some("w:settings"),
        "[Content_Types].xml" => Some("Types"),
        name if name.ends_with(".rels") => Some("Relationships"),
        _ => None,
    };
    if expected_root.is_some_and(|expected| root.name != expected) {
        return Err(format!("{name}: unexpected root {}", root.name));
    }
    for (prefix, expected) in [("xmlns:w", NS_W), ("xmlns:r", NS_R)] {
        if let Some(uri) = root.attrs.get(prefix)
            && uri != expected
        {
            return Err(format!(
                "{name}: {prefix} must be bound to {expected}"
            ));
        }
    }
    if ["word/document.xml", "word/styles.xml"].contains(&name)
        && root.attrs.get("xmlns:w").map(String::as_str) != Some(NS_W)
    {
        return Err(format!("{name}: missing WordprocessingML namespace w"));
    }
    if name == "word/document.xml"
        && root.attrs.get("xmlns:r").map(String::as_str) != Some(NS_R)
    {
        return Err(format!(
            "{name}: missing office relationships namespace r"
        ));
    }
    Ok(())
}

fn text_part(
    parts: &BTreeMap<String, Vec<u8>>, name: &str,
) -> std::result::Result<String, String> {
    String::from_utf8(parts[name].clone()).map_err(|e| format!("{name}: {e}"))
}

struct ContentTypes {
    defaults: BTreeMap<String, String>,
    overrides: BTreeMap<String, String>,
}

impl ContentTypes {
    fn parse(root: &XmlNode) -> std::result::Result<Self, String> {
        let mut types =
            Self { defaults: BTreeMap::new(), overrides: BTreeMap::new() };
        for child in &root.children {
            let key = match child.name.as_str() {
                "Default" => "Extension",
                "Override" => "PartName",
                _ => continue,
            };
            let name = required(child, key)?;
            let name = if key == "PartName" {
                let path = resolve_target("", name).ok_or_else(|| {
                    format!("invalid content type part URI {name:?}")
                })?;
                format!("/{path}")
            } else {
                name.into()
            };
            let content_type = required(child, "ContentType")?;
            let map = if key == "Extension" {
                &mut types.defaults
            } else {
                &mut types.overrides
            };
            map.insert(name, content_type.into());
        }
        Ok(types)
    }

    fn for_part(
        &self, name: &str,
    ) -> std::result::Result<(String, Option<String>), String> {
        if let Some(value) = self.overrides.get(&format!("/{name}")) {
            return Ok((value.clone(), None));
        }
        let ext = name.rsplit('.').next().unwrap_or("");
        self.defaults
            .get(ext)
            .map(|value| (value.clone(), Some(ext.into())))
            .ok_or_else(|| format!("no content type for copied part {name:?}"))
    }
}

fn relationships(
    root: &XmlNode,
) -> std::result::Result<Vec<Relationship>, String> {
    root.children
        .iter()
        .filter(|node| node.name == "Relationship")
        .map(|node| {
            Ok(Relationship {
                id: required(node, "Id")?.into(),
                kind: required(node, "Type")?.into(),
                target: required(node, "Target")?.into(),
                external: node
                    .attrs
                    .get("TargetMode")
                    .is_some_and(|v| v == "External"),
            })
        })
        .collect()
}

fn required<'a>(
    node: &'a XmlNode, name: &str,
) -> std::result::Result<&'a str, String> {
    node.attrs
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("{} is missing attribute {name}", node.name))
}

fn copy_closure(
    source: &str, target: &str, parts: &BTreeMap<String, Vec<u8>>,
    trees: &BTreeMap<String, XmlNode>, needed: &mut BTreeSet<String>,
) -> std::result::Result<(), String> {
    let path = target_path(source, target)?;
    if !parts.contains_key(&path) {
        return Err(format!(
            "relationship from {source:?} names missing part {path:?}"
        ));
    }
    if !needed.insert(path.clone()) {
        return Ok(());
    }
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", &path));
    let rels_path = if dir.is_empty() {
        format!("_rels/{name}.rels")
    } else {
        format!("{dir}/_rels/{name}.rels")
    };
    if let Some(tree) = trees.get(&rels_path) {
        needed.insert(rels_path);
        for rel in relationships(tree)? {
            if !rel.external {
                copy_closure(&path, &rel.target, parts, trees, needed)?;
            }
        }
    }
    Ok(())
}

fn target_path(
    source: &str, target: &str,
) -> std::result::Result<String, String> {
    resolve_target(source, target).ok_or_else(|| {
        format!("invalid relationship target {target:?} in {source:?}")
    })
}

fn update_ids(node: &XmlNode, ids: &mut Ids) {
    if node.name == "wp:docPr"
        && let Some(id) =
            node.attrs.get("id").and_then(|v| v.parse::<u32>().ok())
    {
        ids.doc_pr = ids.doc_pr.max(id.saturating_add(1));
    }
    if node.name == "w:bookmarkStart"
        && let Some(id) =
            node.attrs.get("w:id").and_then(|v| v.parse::<u32>().ok())
    {
        ids.bookmark = ids.bookmark.max(id.saturating_add(1));
    }
    for child in &node.children {
        update_ids(child, ids);
    }
}

fn body_section_node(document: &XmlNode) -> Option<&XmlNode> {
    document
        .children
        .iter()
        .find(|node| node.name == "w:body")?
        .children
        .iter()
        .rev()
        .find(|node| node.name == "w:sectPr")
}

/// Selects the reference's body section or writer's footer-bearing page setup.
pub(crate) fn page_layout(
    reference: Option<&Reference>, paper: &str, footer_rel: Option<&str>,
) -> PageLayout {
    let (width, height) = paper_size(paper);
    let sect_pr = reference
        .and_then(|reference| reference.sect_pr.clone())
        .unwrap_or_else(|| {
            builtin_section(width, height, footer_rel.unwrap_or(""))
        });
    let builtin;
    let section =
        if let Some(section) = reference.and_then(|r| r.section.as_ref()) {
            section
        } else {
            builtin = parse_xml(&format!(
                r#"<root xmlns:w="{NS_W}" xmlns:r="{NS_R}">{sect_pr}</root>"#
            ))
            .expect("writer's section XML is valid");
            &builtin.children[0]
        };
    let page = section.children.iter().find(|node| node.name == "w:pgSz");
    let margins = section.children.iter().find(|node| node.name == "w:pgMar");
    let width = dimension(page, "w:w", width);
    let height = dimension(page, "w:h", height);
    let left = dimension(margins, "w:left", 1440);
    let right = dimension(margins, "w:right", 1440);
    let gutter = dimension(margins, "w:gutter", 0);
    let top = dimension(margins, "w:top", 1440);
    let bottom = dimension(margins, "w:bottom", 1440);
    PageLayout {
        sect_pr,
        text_width_twips: width
            .saturating_sub(left)
            .saturating_sub(right)
            .saturating_sub(gutter),
        text_height_twips: height.saturating_sub(top).saturating_sub(bottom),
    }
}

fn dimension(node: Option<&XmlNode>, name: &str, fallback: u32) -> u32 {
    node.and_then(|node| node.attrs.get(name))
        .and_then(|value| value.parse::<i64>().ok())
        .and_then(|value| u32::try_from(value.unsigned_abs()).ok())
        .unwrap_or(fallback)
}

fn paper_size(paper: &str) -> (u32, u32) {
    match paper {
        "a5" => (8391, 11906),
        "us-letter" => (12240, 15840),
        "us-legal" => (12240, 20160),
        _ => (11906, 16838),
    }
}

fn builtin_section(width: u32, height: u32, footer_rel: &str) -> String {
    format!(
        r#"<w:sectPr><w:footerReference w:type="default" r:id="{}"/><w:footnotePr><w:numRestart w:val="eachSect"/></w:footnotePr><w:pgSz w:w="{width}" w:h="{height}"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/><w:cols w:space="720"/></w:sectPr>"#,
        escape(footer_rel)
    )
}

/// Renders the simple PAGE field in writer's built-in footer.
pub(crate) fn footer_xml(style_id: &str) -> String {
    let result = r#"<w:r><w:t xml:space="preserve">1</w:t></w:r>"#;
    format!(
        r#"{XML_HEAD}<w:ftr xmlns:w="{NS_W}" xmlns:r="{NS_R}"><w:p><w:pPr><w:pStyle w:val="{}"/></w:pPr>{}</w:p></w:ftr>"#,
        escape(style_id),
        field("PAGE", result)
    )
}

/// Merges ordered settings, preserving repeatable children and root namespaces.
pub(crate) fn settings_xml(
    reference: Option<&str>, update_fields: bool,
) -> Result<String> {
    let builtin = vendor::get_str("docx/settings.xml")?;
    let builtin_tree = parse_xml(&builtin).map_err(Error::Other)?;
    let reference_tree =
        reference.map(parse_xml).transpose().map_err(Error::Other)?;
    let mut namespaces: BTreeMap<String, String> = reference_tree
        .as_ref()
        .map(|tree| {
            tree.attrs
                .iter()
                .filter(|(name, _)| name.starts_with("xmlns:"))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (prefix, uri) in [
        ("w", NS_W),
        ("m", NS_M),
        ("r", NS_R),
        ("o", NS_O),
        ("v", NS_V),
        ("w10", NS_W10),
        ("sl", NS_SL),
    ] {
        namespaces.insert(format!("xmlns:{prefix}"), uri.into());
    }
    let mut xml = format!("{XML_HEAD}<w:settings");
    for (name, uri) in namespaces {
        xml.push_str(&format!(r#" {name}="{}""#, escape(&uri)));
    }
    xml.push('>');
    for name in SETTINGS {
        if *name == "w:updateFields" {
            if update_fields {
                xml.push_str(r#"<w:updateFields w:val="true"/>"#);
            }
            continue;
        }
        let copyable =
            !["w:footnotePr", "w:endnotePr", "m:mathPr", "sl:schemaLibrary"]
                .contains(name);
        let reference_children: Vec<_> = reference_tree
            .as_ref()
            .filter(|_| copyable)
            .into_iter()
            .flat_map(|tree| &tree.children)
            .filter(|node| node.name == *name)
            .collect();
        if !reference_children.is_empty() {
            for child in reference_children {
                xml.push_str(
                    &reference.expect("has parsed reference")
                        [child.range.clone()],
                );
            }
        } else {
            for child in
                builtin_tree.children.iter().filter(|node| node.name == *name)
            {
                xml.push_str(&builtin[child.range.clone()]);
            }
        }
    }
    xml.push_str("</w:settings>");
    Ok(xml)
}

#[cfg(test)]
mod tests {
    use super::super::Package;
    use super::super::package::zip_parts;
    use super::super::xml::{CT_DOCUMENT, CT_RELS, CT_STYLES, NS_RELS, NS_WP};
    use super::*;

    fn parts(body: &str) -> BTreeMap<String, Vec<u8>> {
        [
            ("[Content_Types].xml", format!(r#"{XML_HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="{CT_RELS}"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="{CT_DOCUMENT}"/><Override PartName="/word/styles.xml" ContentType="{CT_STYLES}"/></Types>"#)),
            ("word/document.xml", format!(r#"{XML_HEAD}<w:document xmlns:w="{NS_W}" xmlns:r="{NS_R}"><w:body>{body}</w:body></w:document>"#)),
            ("word/styles.xml", format!(r#"{XML_HEAD}<w:styles xmlns:w="{NS_W}"/>"#)),
            ("word/_rels/document.xml.rels", format!(r#"{XML_HEAD}<Relationships xmlns="{NS_RELS}"/>"#)),
        ].into_iter().map(|(path, xml)| (path.into(), xml.into_bytes())).collect()
    }

    fn load(parts: BTreeMap<String, Vec<u8>>) -> Result<Reference> {
        let file = tempfile::NamedTempFile::new().unwrap();
        let entries = parts.into_iter().collect::<Vec<_>>();
        std::fs::write(file.path(), zip_parts(&entries).unwrap()).unwrap();
        Reference::load(file.path())
    }

    fn add_relationship(parts: &mut BTreeMap<String, Vec<u8>>, rel: &str) {
        let xml =
            String::from_utf8(parts["word/_rels/document.xml.rels"].clone())
                .unwrap();
        let xml = xml.replace("/>", &format!(">{rel}</Relationships>"));
        parts.insert("word/_rels/document.xml.rels".into(), xml.into_bytes());
    }

    #[test]
    fn pandoc_default_reference_text_width() {
        let reference =
            load(parts("<w:sectPr><w:footnotePr/></w:sectPr>")).unwrap();
        let layout = page_layout(Some(&reference), "a4", None);
        assert_eq!(layout.text_width_twips, 9026);
        assert_eq!(layout.text_height_twips, 13958);
        assert_eq!(layout.sect_pr, "<w:sectPr><w:footnotePr/></w:sectPr>");
    }

    #[test]
    fn builtin_sect_pr_per_paper() {
        for (paper, width, height) in [
            ("a4", 11906, 16838),
            ("a5", 8391, 11906),
            ("us-letter", 12240, 15840),
            ("us-legal", 12240, 20160),
            ("b5", 11906, 16838),
        ] {
            let layout = page_layout(None, paper, Some("rId9"));
            assert_eq!(layout.text_width_twips, width - 2880);
            assert_eq!(layout.text_height_twips, height - 2880);
            assert!(
                layout
                    .sect_pr
                    .contains(&format!(r#"w:w="{width}" w:h="{height}""#))
            );
            assert!(layout.sect_pr.contains(
                r#"<w:footerReference w:type="default" r:id="rId9"/>"#
            ));
        }
        let reference = load(parts(r#"<w:sectPr><w:pgSz w:w="10000"/><w:pgMar w:top="-1000" w:bottom="-2000" w:left="100" w:right="200" w:gutter="300"/></w:sectPr>"#)).unwrap();
        let layout = page_layout(Some(&reference), "us-letter", None);
        assert_eq!(layout.text_width_twips, 9400);
        assert_eq!(layout.text_height_twips, 12840);
    }

    #[test]
    fn paragraph_level_sect_pr_is_ignored() {
        let reference = load(parts(r#"<w:p><w:pPr><w:sectPr><w:pgSz w:w="10" w:h="20"/></w:sectPr></w:pPr></w:p>"#)).unwrap();
        assert!(reference.sect_pr.is_none());
        assert_eq!(
            page_layout(Some(&reference), "a4", Some("rId4")).text_width_twips,
            9026
        );
    }

    #[test]
    fn settings_merge() {
        let reference = format!(
            r#"<w:settings xmlns:w="{NS_W}" xmlns:m="{NS_M}" xmlns:w14="urn:w14" xmlns:mc="urn:mc" mc:Ignorable="w14"><w:activeWritingStyle w:lang="en"/><w:activeWritingStyle w:lang="de"/><w:zoom w:percent="73"/><w:updateFields w:val="false"/><w:footnotePr/><w:endnotePr/><w:compat><w:compatSetting w:name="reference"/></w:compat><w:rsids><w:rsidRoot w:val="1234"/></w:rsids><m:mathPr><m:mathFont m:val="Ignored"/></m:mathPr><w14:docId w14:val="1"/></w:settings>"#
        );
        let xml = settings_xml(Some(&reference), true).unwrap();
        let tree = parse_xml(&xml).unwrap();
        assert_eq!(tree.attrs["xmlns:w14"], "urn:w14");
        assert!(!xml.contains("mc:Ignorable"));
        assert!(!xml.contains("w14:docId"));
        assert!(!xml.contains("w:footnotePr"));
        assert!(!xml.contains("w:endnotePr"));
        assert!(!xml.contains("Ignored"));
        assert!(xml.contains("Cambria Math"));
        assert!(xml.contains("w:name=\"reference\""));
        assert!(xml.contains("w:rsidRoot"));
        assert!(xml.contains("w:percent=\"73\""));
        assert!(xml.contains(r#"<w:updateFields w:val="true"/>"#));
        assert_eq!(
            tree.children
                .iter()
                .filter(|n| n.name == "w:activeWritingStyle")
                .count(),
            2
        );
        assert!(
            !settings_xml(Some(&reference), false)
                .unwrap()
                .contains("w:updateFields")
        );
        assert!(
            xml.find("<w:zoom").unwrap()
                < xml.find("<w:activeWritingStyle").unwrap()
        );
        assert!(
            xml.find("<w:updateFields").unwrap()
                < xml.find("<w:compat").unwrap()
        );
    }

    #[test]
    fn reference_errors() {
        let mut missing = parts("");
        missing.remove("word/styles.xml");
        assert_resource(load(missing));
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"not a ZIP").unwrap();
        assert_resource(Reference::load(file.path()));
        for xml in [
            format!(
                r#"<w:document xmlns:w="{NS_W}" xmlns:r="{NS_R}"><w:body>"#
            ),
            format!(r#"<w:document xmlns:w="urn:wrong" xmlns:r="{NS_R}"/>"#),
            format!(r#"<w:document xmlns:w="{NS_W}" xmlns:r="urn:wrong"/>"#),
        ] {
            let mut invalid = parts("");
            invalid.insert("word/document.xml".into(), xml.into_bytes());
            assert_resource(load(invalid));
        }
    }

    fn assert_resource(result: Result<Reference>) {
        assert!(matches!(
            result,
            Err(Error::Resource { what: "reference document", .. })
        ));
    }

    #[test]
    fn ids_start_above_copied_parts() {
        let mut parts = parts("");
        add_relationship(
            &mut parts,
            &format!(
                r#"<Relationship Id="rId100" Type="{REL_HEADER}" Target="header1.xml"/>"#
            ),
        );
        parts.insert("word/header1.xml".into(), format!(r#"<w:hdr xmlns:w="{NS_W}" xmlns:wp="{NS_WP}"><w:p><w:bookmarkStart w:id="17" w:name="header"/><wp:docPr id="9"/><w:bookmarkEnd w:id="17"/></w:p></w:hdr>"#).into_bytes());
        let reference = load(parts).unwrap();
        assert_eq!(reference.ids.bookmark, 18);
        assert_eq!(reference.ids.doc_pr, 10);
        assert_eq!(reference.ids.footnote, 1);
    }

    #[test]
    fn footer_name_avoids_copied_parts() {
        let mut parts = parts("");
        add_relationship(
            &mut parts,
            &format!(
                r#"<Relationship Id="rId9" Type="{REL_FOOTER}" Target="footer1.xml"/><Relationship Id="rId12" Type="{REL_FOOTER}" Target="footer2.xml"/>"#
            ),
        );
        for number in 1..=2 {
            parts.insert(
                format!("word/footer{number}.xml"),
                format!(r#"<w:ftr xmlns:w="{NS_W}"/>"#).into_bytes(),
            );
        }
        let reference = load(parts).unwrap();
        let package = Package::new(
            reference.kept_rels, reference.copied_parts, false, true,
        );
        assert_eq!(package.footer(), Some(("word/footer3.xml", "rId16")));
    }

    #[test]
    fn copied_relationship_targets_are_transitive_and_relative() {
        let mut parts = parts("");
        add_relationship(
            &mut parts,
            &format!(
                r#"<Relationship Id="rId7" Type="{REL_THEME}" Target="theme/theme1.xml"/>"#
            ),
        );
        parts.insert("word/theme/theme1.xml".into(), b"<theme/>".to_vec());
        parts.insert("word/theme/_rels/theme1.xml.rels".into(),
            format!(r#"<Relationships xmlns="{NS_RELS}"><Relationship Id="rId1" Type="image" Target="../media/x%20space.png#picture"/><Relationship Id="rId2" Type="font" Target="../fonts/x.odttf"/></Relationships>"#).into_bytes());
        parts.insert("word/media/x space.png".into(), b"\x89PNG".to_vec());
        parts.insert("word/fonts/x.odttf".into(), vec![42]);
        let types = String::from_utf8(parts["[Content_Types].xml"].clone()).unwrap()
            .replace("</Types>", r#"<Default Extension="png" ContentType="image/png"/><Default Extension="odttf" ContentType="font/type"/></Types>"#);
        parts.insert("[Content_Types].xml".into(), types.into_bytes());
        let reference = load(parts).unwrap();
        assert_eq!(reference.copied_parts.len(), 4);
        assert_eq!(
            reference.copied_parts["word/media/x space.png"].0,
            b"\x89PNG"
        );
        assert_eq!(reference.copied_defaults["odttf"], "font/type");
        assert_eq!(reference.copied_defaults["png"], "image/png");
    }

    #[test]
    fn content_type_overrides_use_opc_uri_paths() {
        let mut parts = parts("");
        add_relationship(
            &mut parts,
            &format!(
                r#"<Relationship Id="rId8" Type="{REL_HEADER}" Target="header%20one.xml"/>"#
            ),
        );
        parts.insert(
            "word/header one.xml".into(),
            format!(r#"<w:hdr xmlns:w="{NS_W}"/>"#).into_bytes(),
        );
        let types = String::from_utf8(parts["[Content_Types].xml"].clone()).unwrap()
            .replace("</Types>", r#"<Override PartName="/word/header%20one.xml" ContentType="header/reference"/></Types>"#);
        parts.insert("[Content_Types].xml".into(), types.into_bytes());
        let reference = load(parts).unwrap();
        assert_eq!(
            reference.copied_parts["word/header one.xml"].1,
            "header/reference"
        );
    }

    #[test]
    fn section_keeps_its_root_namespace_bindings() {
        let mut parts = parts(r#"<w:sectPr><w14:extra/></w:sectPr>"#);
        let document = String::from_utf8(parts["word/document.xml"].clone())
            .unwrap()
            .replace("<w:document ", r#"<w:document xmlns:w14="urn:w14" "#);
        parts.insert("word/document.xml".into(), document.into_bytes());
        let reference = load(parts).unwrap();
        assert_eq!(reference.document_namespaces["xmlns:w14"], "urn:w14");
        assert!(
            page_layout(Some(&reference), "a4", None)
                .sect_pr
                .contains("w14:extra")
        );
    }
}
