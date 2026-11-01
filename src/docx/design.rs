//! Design tokens in generated Word styles and section properties.

use crate::design::{Design, LineHeight, Pt};

use super::validate::{XmlNode, parse_xml};
use super::xml::{PPR, Props, RPR, escape};

/// Converts points to twentieths of a point.
pub fn twips(pt: Pt) -> u32 {
    (pt.0 * 20.0).round() as u32
}

/// Converts points to half-points.
pub fn half_points(pt: Pt) -> u32 {
    (pt.0 * 2.0).round() as u32
}

/// Converts points to eighths of a point.
pub fn eighths(pt: Pt) -> u32 {
    (pt.0 * 8.0).round() as u32
}

fn element(node: &XmlNode) -> String {
    let attrs = node
        .attrs
        .iter()
        .map(|(key, value)| format!(" {key}=\"{}\"", escape(value)))
        .collect::<String>();
    let contents = format!(
        "{}{}",
        escape(&node.text),
        node.children.iter().map(element).collect::<String>()
    );
    if contents.is_empty() {
        format!("<{}{attrs}/>", node.name)
    } else {
        format!("<{}{attrs}>{contents}</{}>", node.name, node.name)
    }
}

fn property(name: &str, attrs: &[(&str, String)]) -> String {
    let attrs = attrs
        .iter()
        .map(|(key, value)| format!(" {key}=\"{}\"", escape(value)))
        .collect::<String>();
    format!("<{name}{attrs}/>")
}

fn run_props(
    node: &XmlNode, design: &Design, defaults: bool, scale: bool,
) -> String {
    let mut props = Props::new("w:rPr", RPR);
    for child in &node.children {
        if let Some(&tag) = RPR.iter().find(|&&tag| tag == child.name) {
            props.set(tag, element(child));
        }
    }
    let font = if defaults {
        design.fonts.body.as_ref()
    } else {
        let family = node
            .children
            .iter()
            .find(|n| n.name == "w:rFonts")
            .and_then(|n| n.attr("w:ascii"));
        match family {
            Some("Calibri Light") => design.fonts.heading.as_ref(),
            Some("Consolas") => design.fonts.mono.as_ref(),
            _ => None,
        }
    };
    if let Some(font) = font {
        props.set(
            "w:rFonts",
            property(
                "w:rFonts",
                &[
                    ("w:ascii", font.family.clone()),
                    ("w:hAnsi", font.family.clone()),
                    ("w:cs", font.family.clone()),
                ],
            ),
        );
    }
    if let Some(size) = design.text.size {
        for tag in ["w:sz", "w:szCs"] {
            let value = if defaults {
                Some(half_points(size))
            } else if scale {
                node.children
                    .iter()
                    .find(|n| n.name == tag)
                    .and_then(|n| n.attr("w:val"))
                    .and_then(|n| n.parse::<f64>().ok())
                    .map(|old| {
                        ((old * size.0 / 11.0 / 2.0).round() * 2.0) as u32
                    })
            } else {
                None
            };
            if let Some(value) = value {
                props.set(tag, property(tag, &[("w:val", value.to_string())]));
            }
        }
    }
    if defaults && let Some(color) = design.colors.text {
        props.set("w:color", property("w:color", &[("w:val", hex(color))]));
    }
    props.to_xml()
}

fn paragraph_defaults(node: &XmlNode, design: &Design) -> String {
    let mut props = Props::new("w:pPr", PPR);
    for child in &node.children {
        if let Some(&tag) = PPR.iter().find(|&&tag| tag == child.name) {
            props.set(tag, element(child));
        }
    }
    if design.text.line_height.is_some()
        || design.text.paragraph_spacing.is_some()
    {
        let mut attrs = node
            .children
            .iter()
            .find(|n| n.name == "w:spacing")
            .map(|n| n.attrs.clone())
            .unwrap_or_default();
        if let Some(after) = design.text.paragraph_spacing {
            attrs.insert("w:after".into(), twips(after).to_string());
        }
        if let Some(height) = design.text.line_height {
            let (line, rule) = match height {
                LineHeight::Multiple(n) => ((n * 240.0).round() as u32, "auto"),
                LineHeight::Exact(pt) => (twips(pt), "exact"),
            };
            attrs.insert("w:line".into(), line.to_string());
            attrs.insert("w:lineRule".into(), rule.into());
        }
        props.set(
            "w:spacing",
            property(
                "w:spacing",
                &attrs
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.clone()))
                    .collect::<Vec<_>>(),
            ),
        );
    }
    if let Some(justify) = design.text.justify {
        props.set(
            "w:jc",
            property(
                "w:jc",
                &[(
                    "w:val",
                    if justify {
                        "both".into()
                    } else {
                        "left".into()
                    },
                )],
            ),
        );
    }
    props.to_xml()
}

fn rewrite(
    node: &XmlNode, design: &Design, parent: &str, hyperlink: bool, scale: bool,
) -> String {
    let hyperlink = hyperlink
        || (node.name == "w:style"
            && node.children.iter().any(|n| {
                n.name == "w:name" && n.attr("w:val") == Some("Hyperlink")
            }));
    let scale = if node.name == "w:style" {
        let name = node
            .children
            .iter()
            .find(|n| n.name == "w:name")
            .and_then(|n| n.attr("w:val"))
            .unwrap_or("")
            .to_ascii_lowercase();
        name.starts_with("heading ")
            || matches!(
                name.as_str(),
                "title"
                    | "title char"
                    | "subtitle"
                    | "subtitle char"
                    | "caption"
                    | "table caption"
                    | "image caption"
                    | "footnote text"
                    | "footnote block text"
                    | "footnote reference"
                    | "source code"
                    | "verbatim char"
            )
    } else {
        scale
    };
    if node.name == "w:rPr" {
        let mut result =
            run_props(node, design, parent == "w:rPrDefault", scale);
        if hyperlink && let Some(color) = design.colors.link {
            let tree = fragment(&result);
            let mut props = Props::new("w:rPr", RPR);
            for child in &tree.children {
                if let Some(&tag) = RPR.iter().find(|&&tag| tag == child.name) {
                    props.set(tag, element(child));
                }
            }
            props.set("w:color", property("w:color", &[("w:val", hex(color))]));
            result = props.to_xml();
        }
        return result;
    }
    if node.name == "w:pPr" && parent == "w:pPrDefault" {
        return paragraph_defaults(node, design);
    }
    let attrs = node
        .attrs
        .iter()
        .map(|(key, value)| format!(" {key}=\"{}\"", escape(value)))
        .collect::<String>();
    let contents = format!(
        "{}{}",
        escape(&node.text),
        node.children
            .iter()
            .map(|n| rewrite(n, design, &node.name, hyperlink, scale))
            .collect::<String>()
    );
    if contents.is_empty() {
        format!("<{}{attrs}/>", node.name)
    } else {
        format!("<{}{attrs}>{contents}</{}>", node.name, node.name)
    }
}

fn hex(color: crate::design::Color) -> String {
    format!("{:02X}{:02X}{:02X}", color.0[0], color.0[1], color.0[2])
}

/// Applies declared tokens to parsed writer style definitions, retaining order.
pub fn apply_tokens(styles_xml: &str, design: &Design) -> String {
    if !design.declared {
        return styles_xml.to_owned();
    }
    let (tree, prefix) = match parse_xml(styles_xml) {
        Ok(tree) => {
            let prefix = &styles_xml[..tree.range.start];
            (tree, prefix)
        }
        Err(_) => {
            let tree = fragment(styles_xml);
            let prefix = &styles_xml[..styles_xml.find('<').unwrap_or(0)];
            (tree, prefix)
        }
    };
    format!("{prefix}{}", rewrite(&tree, design, "", false, false))
}

// A style/rPr extracted from the asset relies on its root's w namespace.
// The wrapper supplies that binding without adding xmlns to the fragment.
fn fragment(source: &str) -> XmlNode {
    let wrapper =
        format!("<root xmlns:w=\"{}\">{source}</root>", super::xml::NS_W,);
    let root = parse_xml(&wrapper).expect("writer style fragment is valid");
    root.children.into_iter().next().expect("one style fragment")
}

/// Generates page geometry with references to the planned furniture parts.
pub fn section(
    design: &Design, paper: &str, header_refs: &[(&str, String)],
    footer_refs: &[(&str, String)],
) -> String {
    let paper = design.page.paper.as_deref().unwrap_or(paper);
    let (width, height) = match paper {
        "a5" => (8391, 11906),
        "us-letter" => (12240, 15840),
        "us-legal" => (12240, 20160),
        _ => (11906, 16838),
    };
    let mut refs = String::new();
    for (kind, values) in [("header", header_refs), ("footer", footer_refs)] {
        for (page, id) in values {
            refs.push_str(&format!(
                "<w:{kind}Reference w:type=\"{}\" r:id=\"{}\"/>",
                escape(page),
                escape(id),
            ));
        }
    }
    let margins = design.page.margin;
    let top = margins.top.map(twips).unwrap_or(1440);
    let right = margins.right.map(twips).unwrap_or(1440);
    let bottom = margins.bottom.map(twips).unwrap_or(1440);
    let left = margins.left.map(twips).unwrap_or(1440);
    let header = design.page.header_offset.map(twips).unwrap_or(720);
    let footer = design.page.footer_offset.map(twips).unwrap_or(720);
    let first = [&design.page.header, &design.page.footer]
        .iter()
        .filter_map(|slot| slot.as_ref())
        .any(|slot| slot.first != slot.rest);
    let title = if first { "<w:titlePg/>" } else { "" };
    format!(
        "<w:sectPr>{refs}<w:footnotePr><w:numRestart w:val=\"eachSect\"/></w:footnotePr><w:pgSz w:w=\"{width}\" w:h=\"{height}\"/><w:pgMar w:top=\"{top}\" w:right=\"{right}\" w:bottom=\"{bottom}\" w:left=\"{left}\" w:header=\"{header}\" w:footer=\"{footer}\" w:gutter=\"0\"/><w:cols w:space=\"720\"/>{title}</w:sectPr>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::{Color, Font, Margin};

    fn design() -> Design {
        let mut design = Design { declared: true, ..Design::default() };
        design.fonts.body =
            Some(Font { family: "Body Family".into(), files: Vec::new() });
        design.fonts.heading =
            Some(Font { family: "Heading Family".into(), files: Vec::new() });
        design.fonts.mono =
            Some(Font { family: "Code Family".into(), files: Vec::new() });
        design.text.size = Some(Pt(12.0));
        design.text.line_height = Some(LineHeight::Multiple(1.35));
        design.text.paragraph_spacing = Some(Pt(3.6 * 72.0 / 25.4));
        design.colors.text = Some(Color([36; 3]));
        design.colors.link = Some(Color([169, 109, 75]));
        design
    }

    fn descendant<'a>(node: &'a XmlNode, path: &[&str]) -> &'a XmlNode {
        if let Some((first, rest)) = path.split_first() {
            descendant(
                node.children.iter().find(|n| n.name == *first).unwrap(),
                rest,
            )
        } else {
            node
        }
    }

    #[test]
    fn tokens_edit_defaults_and_relative_vocabulary_styles() {
        let source = crate::vendor::get_str("docx/styles.xml").unwrap();
        let tree = parse_xml(&apply_tokens(&source, &design())).unwrap();
        let defaults = descendant(&tree, &["w:docDefaults"]);
        let font = descendant(defaults, &["w:rPrDefault", "w:rPr", "w:rFonts"]);
        assert_eq!(font.attr("w:ascii"), Some("Body Family"));
        assert_eq!(
            descendant(defaults, &["w:rPrDefault", "w:rPr", "w:sz"])
                .attr("w:val"),
            Some("24")
        );
        let spacing =
            descendant(defaults, &["w:pPrDefault", "w:pPr", "w:spacing"]);
        assert_eq!(spacing.attr("w:line"), Some("324"));
        assert_eq!(spacing.attr("w:after"), Some("204"));
        let heading = tree
            .children
            .iter()
            .find(|n| n.attr("w:styleId") == Some("Heading1"))
            .unwrap();
        assert_eq!(
            descendant(heading, &["w:rPr", "w:rFonts"]).attr("w:ascii"),
            Some("Heading Family")
        );
        let code = tree
            .children
            .iter()
            .find(|n| n.attr("w:styleId") == Some("SourceCode"))
            .unwrap();
        assert_eq!(
            descendant(code, &["w:rPr", "w:rFonts"]).attr("w:ascii"),
            Some("Code Family")
        );
        let link = tree
            .children
            .iter()
            .find(|n| n.attr("w:styleId") == Some("Hyperlink"))
            .unwrap();
        assert_eq!(
            descendant(link, &["w:rPr", "w:color"]).attr("w:val"),
            Some("A96D4B")
        );
    }

    #[test]
    fn bare_vocabulary_fragments_keep_their_enclosing_namespace_contract() {
        let source = "<w:style w:type=\"character\" w:styleId=\"Hyperlink\"><w:name w:val=\"Hyperlink\"/><w:rPr><w:color w:val=\"0000FF\"/></w:rPr></w:style>";
        let xml = apply_tokens(source, &design());
        assert!(!xml.contains("xmlns:"));
        let root = fragment(&xml);
        assert_eq!(root.name, "w:style");
        assert_eq!(
            descendant(&root, &["w:rPr", "w:color"]).attr("w:val"),
            Some("A96D4B")
        );
    }

    #[test]
    fn geometry_uses_unrounded_points_and_preserves_unset_sides() {
        let mut design = design();
        design.page.margin = Margin {
            top: Some(Pt(50.0 * 72.0 / 25.4)),
            left: Some(Pt(22.0 * 72.0 / 25.4)),
            ..Margin::default()
        };
        let xml = section(
            &design,
            "us-letter",
            &[],
            &[("default", "rFooter".into())],
        );
        assert!(xml.contains("w:w=\"12240\" w:h=\"15840\""));
        assert!(xml.contains(
            "w:top=\"2835\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1247\""
        ));
        assert_eq!(twips(Pt(0.025)), 1);
        assert_eq!(half_points(Pt(0.25)), 1);
        assert_eq!(eighths(Pt(0.0625)), 1);
    }
}
