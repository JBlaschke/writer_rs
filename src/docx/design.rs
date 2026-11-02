//! Design tokens in generated Word styles and section properties.

use std::collections::BTreeMap;

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

/// Starts an ordered property element from the children a node already has.
fn seeded(
    tag: &'static str, order: &'static [&'static str], node: Option<&XmlNode>,
) -> Props {
    let mut props = Props::new(tag, order);
    for child in node.into_iter().flat_map(|node| node.children.iter()) {
        if let Some(&tag) = order.iter().find(|&&tag| tag == child.name) {
            props.set(tag, element(child));
        }
    }
    props
}

/// Orders merged `w:spacing` attributes as the asset writes them.
fn spacing_attrs(attrs: &BTreeMap<String, String>) -> Vec<(&str, String)> {
    const FIRST: [&str; 4] = ["w:before", "w:after", "w:line", "w:lineRule"];
    let rest = attrs
        .iter()
        .filter(|(key, _)| !FIRST.contains(&key.as_str()))
        .map(|(key, value)| (key.as_str(), value.clone()));
    FIRST
        .iter()
        .filter_map(|key| attrs.get(*key).map(|value| (*key, value.clone())))
        .chain(rest)
        .collect()
}

fn run_props(
    node: &XmlNode, design: &Design, defaults: bool, scale: bool, heading: bool,
) -> String {
    let mut props = seeded("w:rPr", RPR, Some(node));
    let font = if defaults {
        design.fonts.body.as_ref()
    } else if heading {
        design.fonts.heading.as_ref()
    } else {
        let family = node
            .children
            .iter()
            .find(|n| n.name == "w:rFonts")
            .and_then(|n| n.attr("w:ascii"));
        // Author and Date name Calibri only to undo Title's heading font:
        // they follow the body role like the paragraphs around them.
        match family {
            Some("Calibri Light") => design.fonts.heading.as_ref(),
            Some("Calibri") => design.fonts.body.as_ref(),
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
    let mut props = seeded("w:pPr", PPR, Some(node));
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
    node: &XmlNode, design: &Design, parent: &str, hyperlink: bool,
    scale: bool, heading: bool,
) -> String {
    let hyperlink = hyperlink
        || (node.name == "w:style"
            && node.children.iter().any(|n| {
                n.name == "w:name" && n.attr("w:val") == Some("Hyperlink")
            }));
    let (scale, heading) = if node.name == "w:style" {
        let name = node
            .children
            .iter()
            .find(|n| n.name == "w:name")
            .and_then(|n| n.attr("w:val"))
            .unwrap_or("")
            .to_ascii_lowercase();
        // Heading levels three onward and some linked character styles use
        // Calibri rather than Calibri Light in the asset. Select by role,
        // including Title and Subtitle, instead of relying on the old family.
        let heading = name.starts_with("heading ")
            || matches!(
                name.as_str(),
                "title" | "title char" | "subtitle" | "subtitle char"
            );
        let scale = name.starts_with("heading ")
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
            );
        (scale, heading)
    } else {
        (scale, heading)
    };
    if node.name == "w:rPr" {
        let mut result =
            run_props(node, design, parent == "w:rPrDefault", scale, heading);
        if hyperlink && let Some(color) = design.colors.link {
            let tree = fragment(&result);
            let mut props = seeded("w:rPr", RPR, Some(&tree));
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
            .map(|n| rewrite(n, design, &node.name, hyperlink, scale, heading))
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
    format!("{prefix}{}", rewrite(&tree, design, "", false, false, false))
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

/// Formats a paragraph style over its registered definition.
///
/// A vocabulary name's definition is writer's built-in one with the design's
/// tokens applied, so every key the design leaves unset keeps its built-in
/// value: outline levels, indents and tabs survive. A custom definition has
/// no properties of its own, so the design's keys are all there is.
pub fn style_xml(
    name: &str, style: &crate::design::Style, design: &Design,
    ids: &super::styles::StyleTable,
) -> String {
    use super::styles::StyleKind;
    use super::xml::{NS_W, STYLE};
    use crate::design::{Align, FontRef};
    let id = ids
        .id_for(name, StyleKind::Paragraph)
        .expect("design style registered before formatting");
    let fragment =
        ids.definition_xml(id).expect("registered design definition");
    let source = format!("<root xmlns:w=\"{NS_W}\">{fragment}</root>");
    let root = parse_xml(&source).expect("registered style XML");
    let tree = &root.children[0];
    let attrs = tree
        .attrs
        .iter()
        .map(|(key, value)| format!("{key}=\"{}\"", escape(value)))
        .collect::<Vec<_>>()
        .join(" ");
    let mut definition = Props::new("w:style", STYLE).with_attrs(&attrs);
    for child in &tree.children {
        if let Some(&tag) = STYLE.iter().find(|&&tag| tag == child.name)
            && !["w:pPr", "w:rPr", "w:name"].contains(&tag)
        {
            definition.set(tag, &source[child.range.clone()]);
        }
    }
    definition.set("w:name", property("w:name", &[("w:val", name.to_owned())]));
    definition.set("w:qFormat", "<w:qFormat/>");
    let child = |tag: &str| tree.children.iter().find(|n| n.name == tag);
    let mut paragraph = seeded("w:pPr", PPR, child("w:pPr"));
    if style.keep_with_next {
        paragraph.set("w:keepNext", "<w:keepNext/>");
    }
    if style.keep_together {
        paragraph.set("w:keepLines", "<w:keepLines/>");
    }
    let mut spacing = child("w:pPr")
        .and_then(|node| node.children.iter().find(|n| n.name == "w:spacing"))
        .map(|node| node.attrs.clone())
        .unwrap_or_default();
    if let Some(space) = style.space_before {
        spacing.insert("w:before".into(), twips(space).to_string());
    }
    if let Some(space) = style.space_after {
        spacing.insert("w:after".into(), twips(space).to_string());
    }
    if let Some(height) = style.line_height {
        let (line, rule) = match height {
            LineHeight::Multiple(n) => ((n * 240.0).round() as u32, "auto"),
            LineHeight::Exact(pt) => (twips(pt), "exact"),
        };
        spacing.insert("w:line".into(), line.to_string());
        spacing.insert("w:lineRule".into(), rule.into());
    }
    if !spacing.is_empty() {
        let attrs = spacing_attrs(&spacing);
        paragraph.set("w:spacing", property("w:spacing", &attrs));
    }
    if style.space_before.is_some() || style.space_after.is_some() {
        paragraph.set("w:contextualSpacing", "<w:contextualSpacing/>");
    }
    if let Some(align) = style.align {
        let align = match align {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "both",
        };
        paragraph.set("w:jc", property("w:jc", &[("w:val", align.into())]));
    }
    definition.set("w:pPr", paragraph.to_xml());
    let mut run = seeded("w:rPr", RPR, child("w:rPr"));
    if let Some(font) = &style.font {
        let family = match font {
            FontRef::Family(family) => Some(family.as_str()),
            role => design.font_for(role).map(|font| font.family.as_str()),
        };
        if let Some(family) = family {
            run.set(
                "w:rFonts",
                property(
                    "w:rFonts",
                    &[
                        ("w:ascii", family.into()),
                        ("w:hAnsi", family.into()),
                        ("w:cs", family.into()),
                    ],
                ),
            );
        }
    }
    if let Some(weight) = style.weight {
        for tag in ["w:b", "w:bCs"] {
            run.set(
                tag,
                if weight.0 >= 600 {
                    format!("<{tag}/>")
                } else {
                    format!("<{tag} w:val=\"0\"/>")
                },
            );
        }
    }
    if let Some(italic) = style.italic {
        for tag in ["w:i", "w:iCs"] {
            run.set(
                tag,
                if italic {
                    format!("<{tag}/>")
                } else {
                    format!("<{tag} w:val=\"0\"/>")
                },
            );
        }
    }
    if let Some(color) = style.color {
        run.set("w:color", property("w:color", &[("w:val", hex(color))]));
    }
    if let Some(size) = style.size {
        for tag in ["w:sz", "w:szCs"] {
            run.set(
                tag,
                property(tag, &[("w:val", half_points(size).to_string())]),
            );
        }
    }
    definition.set("w:rPr", run.to_xml());
    definition.to_xml()
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
        let date = tree
            .children
            .iter()
            .find(|n| n.attr("w:styleId") == Some("Date"))
            .unwrap();
        assert_eq!(
            descendant(date, &["w:rPr", "w:rFonts"]).attr("w:ascii"),
            Some("Body Family")
        );
    }

    #[test]
    fn vocabulary_styles_keep_unset_built_in_properties() {
        use crate::design::{Align, FontRef, Style, Weight};
        use crate::docx::StyleTable;
        let mut design = design();
        let styles = [
            (
                "TOC Heading",
                Style {
                    font: Some(FontRef::Family("Display Face".into())),
                    size: Some(Pt(18.0)),
                    weight: Some(Weight(400)),
                    color: Some(Color([10, 47, 65])),
                    ..Style::default()
                },
            ),
            ("toc 2", Style { space_after: Some(Pt(0.0)), ..Style::default() }),
            ("Title", Style { size: Some(Pt(30.0)), ..Style::default() }),
            (
                "Heading 3",
                Style {
                    weight: Some(Weight(400)),
                    align: Some(Align::Left),
                    ..Style::default()
                },
            ),
            (
                "Postscript",
                Style {
                    space_before: Some(Pt(6.0)),
                    keep_with_next: true,
                    ..Style::default()
                },
            ),
        ];
        design.styles =
            styles.into_iter().map(|(name, s)| (name.to_owned(), s)).collect();
        let xml = StyleTable::generated(&design).to_xml();
        let tree = parse_xml(&xml).unwrap();
        let style = |id: &str| {
            tree.children
                .iter()
                .find(|n| n.attr("w:styleId") == Some(id))
                .unwrap_or_else(|| panic!("missing style {id}"))
        };
        let names = |node: &XmlNode| {
            node.children.iter().map(|n| n.name.clone()).collect::<Vec<_>>()
        };

        // A design rPr over a definition without one, keeping its pPr.
        let toc_heading = style("TOCHeading");
        assert_eq!(
            descendant(toc_heading, &["w:basedOn"]).attr("w:val"),
            Some("Heading1")
        );
        assert_eq!(
            descendant(toc_heading, &["w:pPr", "w:outlineLvl"]).attr("w:val"),
            Some("9")
        );
        let run = descendant(toc_heading, &["w:rPr"]);
        assert_eq!(
            names(run),
            ["w:rFonts", "w:b", "w:bCs", "w:color", "w:sz", "w:szCs"]
        );
        assert_eq!(
            descendant(run, &["w:rFonts"]).attr("w:ascii"),
            Some("Display Face")
        );
        assert_eq!(descendant(run, &["w:b"]).attr("w:val"), Some("0"));
        assert_eq!(descendant(run, &["w:sz"]).attr("w:val"), Some("36"));

        // Spacing merges attribute by attribute; the indent stays.
        let toc2 = descendant(style("TOC2"), &["w:pPr"]);
        assert_eq!(names(toc2), ["w:spacing", "w:ind", "w:contextualSpacing"]);
        let spacing = descendant(toc2, &["w:spacing"]);
        assert_eq!(spacing.attr("w:before"), Some("0"));
        assert_eq!(spacing.attr("w:after"), Some("0"));
        assert_eq!(descendant(toc2, &["w:ind"]).attr("w:left"), Some("240"));

        // An unset align keeps the built-in centring; tokens stay applied.
        let title = style("Title");
        assert_eq!(
            names(descendant(title, &["w:pPr"])),
            ["w:keepNext", "w:keepLines", "w:spacing", "w:jc"]
        );
        assert_eq!(
            descendant(title, &["w:pPr", "w:jc"]).attr("w:val"),
            Some("center")
        );
        assert_eq!(
            descendant(title, &["w:rPr", "w:rFonts"]).attr("w:ascii"),
            Some("Heading Family")
        );
        assert_eq!(
            descendant(title, &["w:rPr", "w:sz"]).attr("w:val"),
            Some("60")
        );

        // A set key replaces the built-in child of the same name.
        let heading = style("Heading3");
        assert_eq!(
            descendant(heading, &["w:pPr", "w:jc"]).attr("w:val"),
            Some("left")
        );
        assert_eq!(
            descendant(heading, &["w:pPr", "w:outlineLvl"]).attr("w:val"),
            Some("2")
        );
        assert_eq!(
            descendant(heading, &["w:rPr", "w:b"]).attr("w:val"),
            Some("0")
        );
        assert_eq!(
            descendant(heading, &["w:rPr", "w:bCs"]).attr("w:val"),
            Some("0")
        );
        assert_eq!(
            descendant(heading, &["w:rPr", "w:color"]).attr("w:val"),
            Some("0F4761")
        );

        // A custom name starts from an empty definition, as before.
        let custom = style("Postscript");
        assert_eq!(
            descendant(custom, &["w:basedOn"]).attr("w:val"),
            Some("BodyText")
        );
        assert_eq!(
            names(descendant(custom, &["w:pPr"])),
            ["w:keepNext", "w:spacing", "w:contextualSpacing"]
        );
        assert!(custom.children.iter().all(|n| n.name != "w:rPr"));
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
