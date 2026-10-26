//! DOCX list numbering definitions (docx-output.md §3).
//! Owner: T04. Stub created by T00.

use std::sync::LazyLock;

use crate::error::{Error, Result};

use super::validate::parse_xml;
use super::xml::{NS_W, XML_HEAD};

/// Nine-level bullet and decimal definitions with per-list instances.
///
/// Reference definitions below pandoc's reserved ranges are retained. Writer's
/// instances start at 1001 and follow the order in which lists are emitted.
#[derive(Debug, Clone)]
pub struct Numbering {
    root: String,
    pictures: Vec<String>,
    abstracts: Vec<String>,
    kept_nums: Vec<String>,
    cleanup: Vec<String>,
    other: Vec<String>,
    nums: Vec<String>,
    next_id: u32,
}

struct Builtin {
    root: String,
    abstracts: Vec<String>,
}

static BUILTIN: LazyLock<Builtin> = LazyLock::new(|| {
    // This embedded, checked-in XML is validated by builtin_levels below.
    let source = crate::vendor::get_str("docx/numbering.xml")
        .expect("built-in numbering asset is present and UTF-8");
    let tree = parse_xml(&source).expect("built-in numbering is valid XML");
    Builtin {
        root: source[tree.range.start..tree.open_end].into(),
        abstracts: tree
            .children
            .iter()
            .filter(|child| child.name == "w:abstractNum")
            .map(|child| source[child.range.clone()].into())
            .collect(),
    }
});

impl Numbering {
    /// Starts with writer's bullet and decimal definitions and no list instances.
    pub fn builtin() -> Self {
        Self {
            root: BUILTIN.root.clone(),
            pictures: Vec::new(),
            abstracts: Vec::new(),
            kept_nums: Vec::new(),
            cleanup: Vec::new(),
            other: Vec::new(),
            nums: Vec::new(),
            next_id: 1001,
        }
    }

    /// Retains a reference part outside the reserved abstract/instance ranges.
    ///
    /// Abstract definitions with ids ≥990 and instances with ids ≥1000 are
    /// replaced by writer's definitions. A missing part uses the built-in root.
    pub fn from_reference(numbering_xml: Option<&str>) -> Result<Self> {
        let Some(source) = numbering_xml else {
            return Ok(Self::builtin());
        };
        let tree = parse_xml(source).map_err(|error| {
            Error::Other(format!("invalid numbering.xml: {error}"))
        })?;
        if tree.name != "w:numbering" || tree.attr("xmlns:w") != Some(NS_W) {
            return Err(Error::Other(
                "numbering.xml must have a WordprocessingML w:numbering root"
                    .into(),
            ));
        }
        let mut table = Self::builtin();
        table.root = source[tree.range.start..tree.open_end].into();
        if table.root.ends_with("/>") {
            table.root.truncate(table.root.len() - 2);
            table.root.push('>');
        }
        for child in &tree.children {
            let raw = source[child.range.clone()].to_owned();
            match child.name.as_str() {
                "w:numPicBullet" => table.pictures.push(raw),
                "w:abstractNum" => {
                    if numeric_id(child.attr("w:abstractNumId"))? < 990 {
                        table.abstracts.push(raw);
                    }
                }
                "w:num" => {
                    if numeric_id(child.attr("w:numId"))? < 1000 {
                        table.kept_nums.push(raw);
                    }
                }
                "w:numIdMacAtCleanup" => table.cleanup.push(raw),
                _ => table.other.push(raw),
            }
        }
        Ok(table)
    }

    /// Adds a bullet-list instance and returns its `w:numId`.
    pub fn bullet(&mut self) -> u32 {
        self.instance(991, None)
    }

    /// Adds a decimal-list instance, overriding the start on all nine levels.
    pub fn ordered(&mut self, start: u64) -> u32 {
        self.instance(99411, Some(start))
    }

    fn instance(&mut self, abstract_id: u32, start: Option<u64>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let mut body = format!(
            "<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"{abstract_id}\"/>"
        );
        if let Some(start) = start {
            for level in 0..9 {
                body.push_str(&format!(
                    "<w:lvlOverride w:ilvl=\"{level}\"><w:startOverride w:val=\"{start}\"/></w:lvlOverride>"
                ));
            }
        }
        body.push_str("</w:num>");
        self.nums.push(body);
        id
    }

    /// Serializes pictures, abstracts, instances and cleanup in OOXML order.
    pub fn to_xml(&self) -> String {
        let mut result = format!("{XML_HEAD}{}", self.root);
        for group in [
            &self.pictures, &self.abstracts, &BUILTIN.abstracts,
            &self.kept_nums, &self.nums, &self.cleanup, &self.other,
        ] {
            for child in group {
                result.push_str(child);
            }
        }
        result.push_str("</w:numbering>");
        result
    }
}

fn numeric_id(value: Option<&str>) -> Result<u64> {
    value.and_then(|value| value.parse().ok()).ok_or_else(|| {
        Error::Other(
            "numbering.xml has a missing or invalid numbering id".into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child<'a>(
        node: &'a super::super::validate::XmlNode, name: &str,
    ) -> &'a super::super::validate::XmlNode {
        node.children.iter().find(|child| child.name == name).unwrap()
    }

    #[test]
    fn builtin_levels() {
        let source = crate::vendor::get_str("docx/numbering.xml").unwrap();
        let tree = parse_xml(&source).unwrap();
        assert_eq!(tree.children.len(), 2);
        for (index, definition) in tree.children.iter().enumerate() {
            assert_eq!(definition.name, "w:abstractNum");
            assert_eq!(
                definition.attr("w:abstractNumId"),
                Some(if index == 0 { "991" } else { "99411" })
            );
            assert_eq!(
                child(definition, "w:multiLevelType").attr("w:val"),
                Some("multilevel")
            );
            let levels: Vec<_> = definition
                .children
                .iter()
                .filter(|node| node.name == "w:lvl")
                .collect();
            assert_eq!(levels.len(), 9);
            for (level, node) in levels.iter().enumerate() {
                assert_eq!(
                    node.attr("w:ilvl"),
                    Some(level.to_string().as_str())
                );
                let ind = child(child(node, "w:pPr"), "w:ind");
                assert_eq!(
                    ind.attr("w:left"),
                    Some((720 * (level + 1)).to_string().as_str())
                );
                assert_eq!(ind.attr("w:hanging"), Some("360"));
                assert_eq!(child(node, "w:lvlJc").attr("w:val"), Some("left"));
                let text = child(node, "w:lvlText").attr("w:val").unwrap();
                if index == 0 {
                    assert_eq!(
                        child(node, "w:numFmt").attr("w:val"),
                        Some("bullet")
                    );
                    let (glyph, face) = [
                        ("\u{f0b7}", "Symbol"),
                        ("o", "Courier New"),
                        ("\u{f0a7}", "Wingdings"),
                    ][level % 3];
                    assert_eq!(text, glyph);
                    let fonts = child(child(node, "w:rPr"), "w:rFonts");
                    for attribute in ["w:ascii", "w:hAnsi", "w:cs"] {
                        assert_eq!(fonts.attr(attribute), Some(face));
                    }
                    assert_eq!(fonts.attr("w:hint"), Some("default"));
                } else {
                    assert_eq!(child(node, "w:start").attr("w:val"), Some("1"));
                    assert_eq!(
                        child(node, "w:numFmt").attr("w:val"),
                        Some("decimal")
                    );
                    assert_eq!(text, format!("%{}.", level + 1));
                }
            }
        }
    }

    #[test]
    fn num_ids_from_1001() {
        let mut table = Numbering::builtin();
        assert_eq!(table.bullet(), 1001);
        assert_eq!(table.ordered(1), 1002);
        assert_eq!(table.bullet(), 1003);
    }

    #[test]
    fn ordered_overrides_nine_levels() {
        let mut table = Numbering::builtin();
        table.ordered(7);
        let tree = parse_xml(&table.to_xml()).unwrap();
        let num = child(&tree, "w:num");
        assert_eq!(child(num, "w:abstractNumId").attr("w:val"), Some("99411"));
        assert_eq!(num.children.len(), 10);
        for (level, override_) in num.children[1..].iter().enumerate() {
            assert_eq!(override_.name, "w:lvlOverride");
            assert_eq!(
                override_.attr("w:ilvl"),
                Some(level.to_string().as_str())
            );
            assert_eq!(
                child(override_, "w:startOverride").attr("w:val"),
                Some("7")
            );
        }
    }

    #[test]
    fn reference_filter_and_order() {
        let reference = format!(
            concat!(
                "<w:numbering xmlns:w=\"{}\" xmlns:x=\"urn:kept\">",
                "<w:numIdMacAtCleanup w:val=\"9\"/>",
                "<w:num w:numId=\"1000\"><w:abstractNumId w:val=\"991\"/></w:num>",
                "<w:num w:numId=\"5\"><w:abstractNumId w:val=\"5\"/></w:num>",
                "<w:abstractNum w:abstractNumId=\"991\"/>",
                "<w:abstractNum w:abstractNumId=\"990\"/>",
                "<w:abstractNum w:abstractNumId=\"5\"/>",
                "<w:numPicBullet w:numPicBulletId=\"2\"/>",
                "</w:numbering>",
            ),
            NS_W
        );
        let mut table = Numbering::from_reference(Some(&reference)).unwrap();
        table.bullet();
        let output = table.to_xml();
        let tree = parse_xml(&output).unwrap();
        assert_eq!(tree.attr("xmlns:x"), Some("urn:kept"));
        assert_eq!(
            tree.children
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            [
                "w:numPicBullet", "w:abstractNum", "w:abstractNum",
                "w:abstractNum", "w:num", "w:num", "w:numIdMacAtCleanup"
            ]
        );
        assert_eq!(output.matches("w:abstractNumId=\"991\"").count(), 1);
        assert!(!output.contains("w:abstractNumId=\"990\""));
        assert!(!output.contains("w:numId=\"1000\""));
        assert!(output.contains("<w:abstractNum w:abstractNumId=\"5\"/>"));
        assert!(output.contains(
            "<w:num w:numId=\"5\"><w:abstractNumId w:val=\"5\"/></w:num>"
        ));
    }

    #[test]
    fn no_lists_no_num() {
        let output = Numbering::builtin().to_xml();
        let tree = parse_xml(&output).unwrap();
        assert_eq!(tree.children.len(), 2);
        assert!(tree.children.iter().all(|node| node.name == "w:abstractNum"));
        assert_eq!(Numbering::from_reference(None).unwrap().to_xml(), output);
    }

    #[test]
    fn malformed_reference_is_an_error() {
        for reference in [
            "<w:numbering>",
            "<x/>",
            &format!("<w:numbering xmlns:w=\"{NS_W}\"><w:num/></w:numbering>"),
        ] {
            assert!(Numbering::from_reference(Some(reference)).is_err());
        }
        assert!(
            parse_xml(
                &Numbering::from_reference(Some(&format!(
                    "<w:numbering xmlns:w=\"{NS_W}\"/>"
                )))
                .unwrap()
                .to_xml()
            )
            .is_ok()
        );
    }
}
