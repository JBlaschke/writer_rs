//! Deterministic Unicode text recovery; no OMML or TeX commands.

use super::fold;
use super::parse::{Element, Node};

pub(super) fn folded(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '\u{fe00}' | '\u{fe01}'))
        .map(|c| fold::character(c).text)
        .collect()
}

fn children(element: &Element) -> String {
    element
        .children
        .iter()
        .map(|child| match child {
            Node::Element(element) => self::element(element),
            Node::Text(text) if text.trim().is_empty() => String::new(),
            Node::Text(text) => folded(text),
        })
        .collect()
}

fn script(element: &Element) -> String {
    let value = self::element(element);
    if single_token(element) {
        value
    } else {
        format!("({value})")
    }
}

fn single_token(element: &Element) -> bool {
    match element.name.as_str() {
        "mi" | "mn" | "mo" => true,
        "mtext" | "ms" => element.text().split_whitespace().count() <= 1,
        "mrow" | "mstyle" | "mpadded" | "maction" => {
            let children: Vec<_> = element.elements().collect();
            children.len() == 1 && single_token(children[0])
        }
        _ => false,
    }
}

pub(super) fn element(element: &Element) -> String {
    let parts: Vec<_> = element.elements().collect();
    let at = |index: usize| {
        parts.get(index).map_or_else(String::new, |e| self::element(e))
    };
    match element.name.as_str() {
        "semantics" => {
            parts.first().map_or_else(String::new, |e| self::element(e))
        }
        "mi" | "mn" | "mtext" => folded(&element.text()),
        "mo" => folded(&element.text())
            .chars()
            .filter(|c| !matches!(c, '\u{2061}' | '\u{2062}' | '\u{2063}'))
            .collect(),
        "ms" => format!(
            "{}{}{}",
            element.attr("lquote").unwrap_or("\""),
            folded(&element.text()),
            element.attr("rquote").unwrap_or("\"")
        ),
        "mspace" => " ".into(),
        "mfrac" if parts.len() == 2 => format!("({})/({})", at(0), at(1)),
        "msqrt" => format!("√({})", children(element)),
        "mroot" if parts.len() == 2 => format!("({})√({})", at(1), at(0)),
        "msub" if parts.len() == 2 => format!("{}_{}", at(0), script(parts[1])),
        "msup" if parts.len() == 2 => format!("{}^{}", at(0), script(parts[1])),
        "msubsup" if parts.len() == 3 => {
            format!("{}_{}^{}", at(0), script(parts[1]), script(parts[2]))
        }
        "munder" if parts.len() == 2 => format!("{}_({})", at(0), at(1)),
        "mover" if parts.len() == 2 => format!("{}^({})", at(0), at(1)),
        "munderover" if parts.len() == 3 => {
            format!("{}_({})^({})", at(0), at(1), at(2))
        }
        "mtable" => parts
            .iter()
            .map(|row| {
                row.elements().map(self::element).collect::<Vec<_>>().join(", ")
            })
            .collect::<Vec<_>>()
            .join("; "),
        "mprescripts" => String::new(),
        _ => children(element),
    }
}
