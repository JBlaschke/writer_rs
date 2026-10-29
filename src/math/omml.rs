//! Editable Word equations from the supported MathML Core subset (math.md §3.2).

mod fold;
mod parse;
mod render;
mod text;

use crate::math::latex_mathml::MathError;
use parse::{Element, Node};

const EMPTY_FENCE: &str = "\0writer-empty-fence";

/// Converts MathML to one `m:oMath` fragment without namespace declarations.
pub fn from_mathml(mathml: &str) -> Result<String, MathError> {
    let root = parse::parse(mathml)?;
    let root = match normalize(Node::Element(root))? {
        Some(Node::Element(root)) => root,
        _ => return Ok(format!("<m:oMath>{}</m:oMath>", render::empty())),
    };
    let display = root.attr("display") == Some("block");
    Ok(format!("<m:oMath>{}</m:oMath>", render::element(&root, display)?))
}

/// Flattens supported or foreign MathML into a deterministic readable string.
pub fn fallback_text(mathml: &str) -> String {
    match parse::parse(mathml) {
        Ok(root) => text::element(&root),
        Err(_) => text::folded(&parse::loose_text(mathml)),
    }
}

fn unsupported(element: &Element) -> MathError {
    MathError::Unsupported {
        element: element.name.clone(),
        detail: element.source.chars().take(80).collect(),
    }
}

fn attr_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn normalize(node: Node) -> Result<Option<Node>, MathError> {
    let Node::Element(mut element) = node else {
        return Ok(Some(match node {
            Node::Text(value) => Node::Text(
                value
                    .chars()
                    .filter(|c| !matches!(c, '\u{fe00}' | '\u{fe01}'))
                    .collect(),
            ),
            other => other,
        }));
    };
    if element.name == "semantics" {
        let mut first = None;
        for child in element.children {
            match child {
                Node::Text(value) if value.trim().is_empty() => {}
                Node::Element(child) if first.is_none() => first = Some(child),
                Node::Text(_) => {
                    return Err(unsupported(&Element {
                        name: "semantics".into(),
                        children: Vec::new(),
                        attrs: element.attrs,
                        source: element.source,
                    }));
                }
                _ => {}
            }
        }
        return first.map_or(Ok(None), |child| normalize(Node::Element(child)));
    }
    if matches!(element.name.as_str(), "mstyle" | "mpadded" | "maction") {
        element.name = "mrow".into();
    }
    if element.name == "mrow" {
        mark_empty_fences(&mut element);
    }
    if element.name == "mspace" {
        match space_width(&element)? {
            Some(width) if width > 0.0 => {}
            _ => return Ok(None),
        }
    }
    if element.name == "mo" {
        let value = element.text();
        if !value.is_empty()
            && value
                .chars()
                .all(|c| matches!(c, '\u{2061}' | '\u{2062}' | '\u{2063}'))
            && !element.attrs.contains_key(EMPTY_FENCE)
        {
            return Ok(None);
        }
    }
    let leaf =
        matches!(element.name.as_str(), "mi" | "mn" | "mo" | "mtext" | "ms");
    let mut children = Vec::new();
    for child in std::mem::take(&mut element.children) {
        match normalize(child)? {
            Some(Node::Text(value)) if !leaf && value.trim().is_empty() => {}
            Some(child) => children.push(child),
            None => {}
        }
    }
    element.children = children;
    if element.name == "mrow"
        && element.children.len() == 1
        && matches!(element.children[0], Node::Element(_))
    {
        return Ok(element.children.pop());
    }
    Ok(Some(Node::Element(element)))
}

fn mark_empty_fences(element: &mut Element) {
    let indices: Vec<_> = element
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, node)| match node {
            Node::Text(value) if value.trim().is_empty() => None,
            _ => Some(i),
        })
        .collect();
    let (Some(&first), Some(&last)) = (indices.first(), indices.last()) else {
        return;
    };
    if first == last {
        return;
    }
    let edge = |node: &Node| match node {
        Node::Element(element) if element.name == "mo" => {
            let value = element.text();
            value.is_empty() || value == "\u{2063}" || fence(&value)
        }
        _ => false,
    };
    if !edge(&element.children[first]) || !edge(&element.children[last]) {
        return;
    }
    for index in [first, last] {
        if let Node::Element(edge) = &mut element.children[index]
            && (edge.text().is_empty() || edge.text() == "\u{2063}")
        {
            edge.attrs.insert(EMPTY_FENCE.into(), "true".into());
            edge.children.clear();
        }
    }
}

fn fence(value: &str) -> bool {
    matches!(
        value,
        "(" | ")"
            | "["
            | "]"
            | "{"
            | "}"
            | "⟨"
            | "⟩"
            | "⌊"
            | "⌋"
            | "⌈"
            | "⌉"
            | "⟦"
            | "⟧"
            | "|"
            | "‖"
    )
}

fn space_width(element: &Element) -> Result<Option<f64>, MathError> {
    let Some(width) = element.attr("width") else {
        return Ok(None);
    };
    let width = width.trim();
    let em = width.strip_suffix("em");
    let number = if let Some(number) = em {
        number.parse::<f64>()
    } else {
        let split = width
            .find(|c: char| {
                !c.is_ascii_digit() && !matches!(c, '.' | '-' | '+')
            })
            .unwrap_or(width.len());
        width[..split].parse::<f64>()
    }
    .map_err(|_| unsupported(element))?;
    if !number.is_finite() {
        return Err(unsupported(element));
    }
    if number <= 0.0 {
        return Ok(None);
    }
    if em.is_none() {
        return Err(unsupported(element));
    }
    Ok(Some(number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nary_consumes_exactly_one_following_element() {
        let xml =
            from_mathml("<math><mo>∫</mo><mi>x</mi><mi>d</mi></math>").unwrap();
        let end = xml.find("</m:nary>").unwrap();
        assert!(xml[..end].contains("<m:t>x</m:t>"));
        assert!(xml[end..].contains("<m:t>d</m:t>"));
        assert_eq!(xml.matches("<m:t>x</m:t>").count(), 1);
        assert!(xml.contains("<m:subHide m:val=\"on\"/>"));
        assert!(xml.contains("<m:supHide m:val=\"on\"/>"));
    }

    #[test]
    fn empty_fences_and_scripts_survive_normalization() {
        let xml = from_mathml(
            "<math><mrow><mo>⁣</mo><mi>x</mi><mo>|</mo></mrow></math>",
        )
        .unwrap();
        assert!(xml.contains("<m:begChr m:val=\"\"/>"));
        assert!(xml.contains("<m:endChr m:val=\"|\"/>"));
        let xml =
            from_mathml("<math><msub><mi>x</mi><mrow/></msub></math>").unwrap();
        assert!(xml.contains("<m:sub><m:r><m:t>&#x200B;</m:t>"));
        assert!(
            from_mathml("<math><msub><mi>x</mi><mo/></msub></math>").is_ok()
        );
    }

    #[test]
    fn normalization_ignores_annotations_and_folds_script_selectors() {
        let xml = from_mathml("<math><semantics><mstyle mathcolor='red'><mi>ℱ︀</mi></mstyle><annotation-xml><foreign/></annotation-xml></semantics><mspace/><mo>⁡</mo></math>").unwrap();
        assert!(xml.contains("<m:scr m:val=\"script\"/>"));
        assert!(xml.contains("<m:t>F</m:t>"));
        assert!(!xml.contains("color"));
        assert!(!xml.contains('︀'));
    }

    #[test]
    fn foreign_shapes_return_bounded_unsupported_details() {
        for input in [
            "<math><mfrac><mi>x</mi></mfrac></math>",
            "<math><mspace width='2px'/></math>",
            "<math><mtable><mtr><mtd rowspan='1'><mi>x</mi></mtd></mtr></mtable></math>",
            "<math><mo>∑</mo>intervening<mi>x</mi></math>",
        ] {
            let MathError::Unsupported { detail, .. } =
                from_mathml(input).unwrap_err()
            else {
                panic!("unexpected error kind");
            };
            assert!(detail.chars().count() <= 80);
        }
    }

    #[test]
    fn only_explicit_stretchy_inner_bars_become_delimiter_separators() {
        for (attribute, expected) in
            [("", ""), (" stretchy='false'", ""), (" stretchy='true'", "∣")]
        {
            let source = format!(
                "<math><mrow><mo>(</mo><mi>a</mi><mo{attribute}>∣</mo><mi>b</mi><mo>)</mo></mrow></math>"
            );
            let xml = from_mathml(&source).unwrap();
            assert!(xml.contains(&format!("<m:sepChr m:val=\"{expected}\"/>")));
            assert_eq!(xml.contains("<m:t>∣</m:t>"), expected.is_empty());
            assert_eq!(
                xml.matches("<m:e>").count(),
                if expected.is_empty() { 1 } else { 2 }
            );
        }
    }

    #[test]
    fn specific_decorations_precede_nary_limit_classification() {
        for (source, decoration) in [
            ("<mover accent='true'><mo>∑</mo><mo>^</mo></mover>", "acc"),
            ("<mover><mo>∫</mo><mo stretchy='true'>‾</mo></mover>", "bar"),
            ("<munder><mo>∑</mo><mo>⏟</mo></munder>", "groupChr"),
        ] {
            let xml = from_mathml(&format!("<math>{source}<mi>x</mi></math>"))
                .unwrap();
            assert!(xml.contains(&format!("<m:{decoration}>")), "{xml}");
            let end = xml.find(&format!("</m:{decoration}>")).unwrap();
            assert!(xml[end..].contains("<m:t>x</m:t>"), "{xml}");
            assert!(!xml[..end].contains("<m:t>x</m:t>"), "{xml}");
        }
        let xml = from_mathml(
            "<math><mover><mo>∑</mo><mi>n</mi></mover><mi>x</mi></math>",
        )
        .unwrap();
        let end = xml.find("</m:nary>").unwrap();
        assert!(xml[..end].contains("<m:t>x</m:t>"), "{xml}");
        assert!(xml[..end].contains("<m:supHide m:val=\"off\"/>"), "{xml}");
    }

    #[test]
    fn fallback_uses_source_spacing_and_structural_script_tokens() {
        assert_eq!(
            fallback_text(
                "<math><mi>𝑥</mi><mspace/><msup><mi>ℝ</mi><mrow><mi>i</mi><mo>+</mo><mn>1</mn></mrow></msup><mfrac><mn>1</mn><mn>2</mn></mfrac></math>"
            ),
            "x R^(i+1)(1)/(2)"
        );
    }
}
