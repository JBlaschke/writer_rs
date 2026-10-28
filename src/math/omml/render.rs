//! Structural MathML Core to editable OMML mapping.

use super::fold;
use super::parse::{Element, Node};
use super::{EMPTY_FENCE, attr_escape, fence, space_width, unsupported};
use crate::math::latex_mathml::MathError;

pub(super) fn empty() -> &'static str {
    "<m:r><m:t>&#x200B;</m:t></m:r>"
}

fn xml(value: &str) -> String {
    attr_escape(value)
}

fn wrap(name: &str, body: String) -> String {
    format!("<m:{name}>{body}</m:{name}>")
}

fn slot(name: &str, body: String) -> String {
    wrap(
        name,
        if body.is_empty() {
            empty().into()
        } else {
            body
        },
    )
}

fn property(name: &str, value: &str) -> String {
    format!("<m:{name} m:val=\"{}\"/>", xml(value))
}

fn fixed(element: &Element, count: usize) -> Result<Vec<&Element>, MathError> {
    let children: Vec<_> = element.elements().collect();
    if children.len() != count || element.children.iter().any(|node| {
        matches!(node, Node::Text(value) if !value.trim().is_empty())
    }) { return Err(unsupported(element)); }
    Ok(children)
}

fn leaf(element: &Element) -> Result<String, MathError> {
    if element.children.iter().any(|node| matches!(node, Node::Element(_))) {
        return Err(unsupported(element));
    }
    Ok(element.text())
}

fn plain_run(value: &str) -> String {
    runs(value, Some("p"), false, false)
}

fn runs(
    value: &str, default_style: Option<&str>, normal: bool, identifier: bool,
) -> String {
    let source: String = value
        .chars()
        .filter(|c| !matches!(c, '\u{fe00}' | '\u{fe01}'))
        .collect();
    if source.is_empty() {
        return empty().into();
    }
    let multi = identifier && source.chars().count() > 1;
    let mut groups: Vec<(Option<String>, Option<String>, String)> = Vec::new();
    for character in source.chars() {
        let folded = fold::character(character);
        let styled = folded.text != character.to_string();
        let greek = matches!(character, '\u{0391}'..='\u{03a9}')
            && character != '\u{03a2}';
        let default = if identifier && !styled && (greek || multi) {
            Some("p")
        } else {
            default_style
        };
        let style = folded.style.or(default).map(str::to_owned);
        let script = folded.script.map(str::to_owned);
        if let Some((last_style, last_script, text)) = groups.last_mut()
            && *last_style == style
            && *last_script == script
        {
            text.push_str(&folded.text);
        } else {
            groups.push((style, script, folded.text));
        }
    }
    let mut output = String::new();
    for (style, script, text) in groups {
        let mut props = String::new();
        if normal {
            props.push_str("<m:nor/>");
        }
        if let Some(script) = script {
            props.push_str(&property("scr", &script));
        }
        if let Some(style) = style {
            props.push_str(&property("sty", &style));
        }
        let props = if props.is_empty() {
            props
        } else {
            wrap("rPr", props)
        };
        let preserve = text.trim() != text;
        output.push_str(&format!(
            "<m:r>{props}<m:t{}>{}</m:t></m:r>",
            if preserve {
                " xml:space=\"preserve\""
            } else {
                ""
            },
            xml(&text)
        ));
    }
    output
}

fn sequence(
    owner: &Element, nodes: &[Node], display: bool,
) -> Result<String, MathError> {
    let mut output = String::new();
    let mut index = 0;
    while index < nodes.len() {
        match &nodes[index] {
            Node::Text(text) => {
                if !text.trim().is_empty() {
                    output.push_str(&plain_run(text));
                }
                index += 1;
            }
            Node::Element(child) => {
                if let Some(nary) = nary(child, display)? {
                    let body = match nodes.get(index + 1) {
                        Some(Node::Element(body)) => {
                            index += 1;
                            element(body, display)?
                        }
                        Some(Node::Text(text)) if !text.trim().is_empty() => {
                            return Err(unsupported(owner));
                        }
                        _ => empty().into(),
                    };
                    output.push_str(&render_nary(nary, body, display)?);
                } else {
                    output.push_str(&element(child, display)?);
                }
                index += 1;
            }
        }
    }
    Ok(if output.is_empty() {
        empty().into()
    } else {
        output
    })
}

pub(super) fn element(
    node: &Element, display: bool,
) -> Result<String, MathError> {
    if let Some(parts) = nary(node, display)? {
        return render_nary(parts, empty().into(), display);
    }
    match node.name.as_str() {
        "math" => sequence(node, &node.children, display),
        "mrow" => {
            if let Some(output) = delimited(node, display)? {
                return Ok(output);
            }
            sequence(node, &node.children, display)
        }
        "mi" => {
            let value = leaf(node)?;
            Ok(runs(
                &value,
                if node.attr("mathvariant") == Some("normal") {
                    Some("p")
                } else {
                    None
                },
                false,
                true,
            ))
        }
        "mn" => Ok(plain_run(&leaf(node)?)),
        "mo" => {
            let value = leaf(node)?;
            let run = plain_run(&value);
            if value
                .chars()
                .filter(|c| !matches!(c, '\u{fe00}' | '\u{fe01}'))
                .count()
                > 1
            {
                Ok(format!(
                    "<m:box><m:boxPr><m:opEmu m:val=\"on\"/></m:boxPr><m:e>{run}</m:e></m:box>"
                ))
            } else {
                Ok(run)
            }
        }
        "mtext" | "ms" => {
            let mut value = leaf(node)?;
            if node.name == "ms" {
                value = format!(
                    "{}{}{}",
                    node.attr("lquote").unwrap_or("\""),
                    value,
                    node.attr("rquote").unwrap_or("\"")
                );
            }
            Ok(runs(&value, None, true, false))
        }
        "mspace" => {
            let Some(width) = space_width(node)? else {
                return Ok(String::new());
            };
            let space = if width <= 0.17 {
                "\u{2009}"
            } else if width <= 0.23 {
                "\u{2005}"
            } else if width <= 0.5 {
                "\u{2004}"
            } else if width <= 1.8 {
                "\u{2001}"
            } else {
                "\u{2001}\u{2001}"
            };
            Ok(plain_run(space))
        }
        "mfrac" => {
            let parts = fixed(node, 2)?;
            let no_bar = node.attr("linethickness").is_some_and(|value| {
                value.trim().parse::<f64>().is_ok_and(|number| number == 0.0)
            });
            Ok(format!(
                "<m:f><m:fPr>{}</m:fPr>{}{}</m:f>",
                property("type", if no_bar { "noBar" } else { "bar" }),
                slot("num", element(parts[0], display)?),
                slot("den", element(parts[1], display)?)
            ))
        }
        "msqrt" => Ok(format!(
            "<m:rad><m:radPr><m:degHide m:val=\"on\"/></m:radPr><m:deg/>{}</m:rad>",
            slot("e", sequence(node, &node.children, display)?)
        )),
        "mroot" => {
            let parts = fixed(node, 2)?;
            Ok(format!(
                "<m:rad>{}{}</m:rad>",
                slot("deg", element(parts[1], display)?),
                slot("e", element(parts[0], display)?)
            ))
        }
        "msub" | "msup" | "msubsup" => scripts(node, display),
        "munder" | "mover" | "munderover" => under_over(node, display),
        "mtable" => table(node, display, false),
        "mphantom" => Ok(format!(
            "<m:phant><m:phantPr><m:show m:val=\"off\"/></m:phantPr>{}</m:phant>",
            slot("e", sequence(node, &node.children, display)?)
        )),
        "menclose" => enclosure(node, display),
        "mmultiscripts" => prescripts(node, display),
        _ => Err(unsupported(node)),
    }
}

fn scripts(node: &Element, display: bool) -> Result<String, MathError> {
    let both = node.name == "msubsup";
    let parts = fixed(node, if both { 3 } else { 2 })?;
    let name = if both {
        "sSubSup"
    } else if node.name == "msub" {
        "sSub"
    } else {
        "sSup"
    };
    let mut body = slot("e", element(parts[0], display)?);
    body.push_str(&slot(
        if node.name == "msup" { "sup" } else { "sub" },
        element(parts[1], display)?,
    ));
    if both {
        body.push_str(&slot("sup", element(parts[2], display)?));
    }
    Ok(wrap(name, body))
}

fn under_over(node: &Element, display: bool) -> Result<String, MathError> {
    let both = node.name == "munderover";
    let parts = fixed(node, if both { 3 } else { 2 })?;
    let base = parts[0];
    let below = node.name == "munder";
    if let Some(decoration) = decoration(node)? {
        let base = slot("e", element(base, display)?);
        return Ok(match decoration {
            Decoration::Bar => format!(
                "<m:bar><m:barPr>{}</m:barPr>{base}</m:bar>",
                property("pos", if below { "bot" } else { "top" }),
            ),
            Decoration::Group(character) => format!(
                "<m:groupChr><m:groupChrPr>{}{}{}</m:groupChrPr>{base}</m:groupChr>",
                property("chr", &character),
                property("pos", if below { "bot" } else { "top" }),
                property("vertJc", if below { "top" } else { "bot" }),
            ),
            Decoration::Accent(character) => format!(
                "<m:acc><m:accPr>{}</m:accPr>{base}</m:acc>",
                property("chr", &character),
            ),
        });
    }
    if !display && movable(base) {
        let base = slot("e", element(base, display)?);
        let mut body = base;
        if below || both {
            body.push_str(&slot("sub", element(parts[1], display)?));
        }
        if !below {
            body.push_str(&slot(
                "sup",
                element(parts[if both { 2 } else { 1 }], display)?,
            ));
        }
        return Ok(wrap(
            if both {
                "sSubSup"
            } else if below {
                "sSub"
            } else {
                "sSup"
            },
            body,
        ));
    }
    let mut body = element(base, display)?;
    if both {
        body = wrap(
            "limUpp",
            slot("e", body) + &slot("lim", element(parts[2], display)?),
        );
        body = wrap(
            "limLow",
            slot("e", body) + &slot("lim", element(parts[1], display)?),
        );
    } else {
        body = wrap(
            if below { "limLow" } else { "limUpp" },
            slot("e", body) + &slot("lim", element(parts[1], display)?),
        );
    }
    Ok(body)
}

enum Decoration {
    Bar,
    Group(String),
    Accent(String),
}

// These specific table rows precede generic under/over n-ary limits. Share
// recognition so a decorated operator cannot consume the following sibling.
fn decoration(node: &Element) -> Result<Option<Decoration>, MathError> {
    if !matches!(node.name.as_str(), "munder" | "mover") {
        return Ok(None);
    }
    let parts = fixed(node, 2)?;
    if parts[1].name != "mo" {
        return Ok(None);
    }
    let accent = leaf(parts[1])?;
    if parts[1].attr("stretchy") == Some("true")
        && matches!(accent.as_str(), "‾" | "¯" | "\u{0332}" | "\u{0305}" | "_")
    {
        return Ok(Some(Decoration::Bar));
    }
    if matches!(accent.as_str(), "⏞" | "⏟" | "⎴" | "⎵") {
        return Ok(Some(Decoration::Group(accent)));
    }
    if node.name == "mover" && node.attr("accent") == Some("true") {
        let accent = match accent.as_str() {
            "^" | "ˆ" => "\u{0302}",
            "." | "˙" => "\u{0307}",
            "~" | "˜" => "\u{0303}",
            "¯" => "\u{0304}",
            "˘" => "\u{0306}",
            "ˇ" => "\u{030c}",
            "´" => "\u{0301}",
            "`" => "\u{0300}",
            "¨" => "\u{0308}",
            "→" | "\u{20d7}" => "\u{20d7}",
            _ => accent.as_str(),
        };
        if accent.chars().count() != 1 {
            return Err(unsupported(node));
        }
        return Ok(Some(Decoration::Accent(accent.into())));
    }
    Ok(None)
}

fn movable(node: &Element) -> bool {
    node.name == "mo" && node.attr("movablelimits") == Some("true")
}

struct Nary<'a> {
    character: String,
    sub: Option<&'a Element>,
    sup: Option<&'a Element>,
    location: &'static str,
}

fn nary(node: &Element, display: bool) -> Result<Option<Nary<'_>>, MathError> {
    if decoration(node)?.is_some() {
        return Ok(None);
    }
    let (base, sub, sup, location) = match node.name.as_str() {
        "mo" => (node, None, None, "subSup"),
        "msub" | "msup" | "munder" | "mover" | "msubsup" | "munderover" => {
            let both = matches!(node.name.as_str(), "msubsup" | "munderover");
            let parts = fixed(node, if both { 3 } else { 2 })?;
            let below = matches!(node.name.as_str(), "msub" | "munder");
            let under_over =
                matches!(node.name.as_str(), "munder" | "mover" | "munderover");
            let location = if under_over && (display || !movable(parts[0])) {
                "undOvr"
            } else {
                "subSup"
            };
            (
                parts[0],
                if below || both { Some(parts[1]) } else { None },
                if !below {
                    Some(parts[if both { 2 } else { 1 }])
                } else {
                    None
                },
                location,
            )
        }
        _ => return Ok(None),
    };
    if base.name != "mo" {
        return Ok(None);
    }
    let character = leaf(base)?;
    if !matches!(
        character.as_str(),
        "∫" | "∬" | "∭" | "∮" | "∯" | "∰" | "∏" | "∐" | "∑"
    ) {
        return Ok(None);
    }
    Ok(Some(Nary { character, sub, sup, location }))
}

fn render_nary(
    parts: Nary<'_>, body: String, display: bool,
) -> Result<String, MathError> {
    let props = property("chr", &parts.character)
        + &property("limLoc", parts.location)
        + &property("subHide", if parts.sub.is_some() { "off" } else { "on" })
        + &property("supHide", if parts.sup.is_some() { "off" } else { "on" });
    let sub = parts
        .sub
        .map_or_else(|| Ok(empty().into()), |node| element(node, display))?;
    let sup = parts
        .sup
        .map_or_else(|| Ok(empty().into()), |node| element(node, display))?;
    Ok(wrap(
        "nary",
        wrap("naryPr", props)
            + &slot("sub", sub)
            + &slot("sup", sup)
            + &slot("e", body),
    ))
}

fn edge(node: &Node) -> Option<&str> {
    let Node::Element(element) = node else {
        return None;
    };
    if element.name != "mo" || element.attr("stretchy") == Some("false") {
        return None;
    }
    if element.attrs.contains_key(EMPTY_FENCE) {
        return Some("");
    }
    // Normalised edge tokens contain text only; borrow a single text leaf.
    if let [Node::Text(value)] = element.children.as_slice()
        && fence(value)
    {
        return Some(value);
    }
    None
}

fn delimiter(
    begin: &str, end: &str, separator: &str, bodies: Vec<String>,
) -> String {
    let props = property("begChr", begin)
        + &property("sepChr", separator)
        + &property("endChr", end)
        + "<m:grow/>";
    let content: String =
        bodies.into_iter().map(|body| slot("e", body)).collect();
    wrap("d", wrap("dPr", props) + &content)
}

fn delimited(
    node: &Element, display: bool,
) -> Result<Option<String>, MathError> {
    let children = &node.children;
    // Native cases may have a left brace and a table without a closing mo.
    if let [first, Node::Element(table_node)] = children.as_slice()
        && edge(first) == Some("{")
        && table_node.name == "mtable"
    {
        return Ok(Some(delimiter(
            "{",
            "",
            "",
            vec![table(table_node, display, true)?],
        )));
    }
    if children.len() < 2 {
        return Ok(None);
    }
    let (Some(begin), Some(end)) =
        (edge(&children[0]), edge(&children[children.len() - 1]))
    else {
        return Ok(None);
    };
    let inner = &children[1..children.len() - 1];
    if begin == "{"
        && let [Node::Element(table_node)] = inner
        && table_node.name == "mtable"
    {
        return Ok(Some(delimiter(
            begin,
            end,
            "",
            vec![table(table_node, display, true)?],
        )));
    }
    let mut groups = Vec::new();
    let mut start = 0;
    let mut separator = "";
    for (index, child) in inner.iter().enumerate() {
        if let Node::Element(operator) = child
            && operator.name == "mo"
            && operator.attr("stretchy") == Some("true")
        {
            let value = operator.text();
            if matches!(value.as_str(), "|" | "‖" | "∣") {
                if !separator.is_empty() && separator != value {
                    return Err(unsupported(operator));
                }
                separator = if value == "‖" {
                    "‖"
                } else if value == "∣" {
                    "∣"
                } else {
                    "|"
                };
                groups.push(sequence(node, &inner[start..index], display)?);
                start = index + 1;
            }
        }
    }
    groups.push(sequence(node, &inner[start..], display)?);
    Ok(Some(delimiter(begin, end, separator, groups)))
}

fn alignment(cell: &Element) -> &'static str {
    if let Some(style) = cell.attr("style") {
        for item in style.split(';') {
            if let Some((name, value)) = item.split_once(':')
                && name.trim().eq_ignore_ascii_case("text-align")
            {
                match value.trim() {
                    "left" => return "left",
                    "right" => return "right",
                    "center" => return "center",
                    _ => {}
                }
            }
        }
    }
    let class = cell.attr("class").unwrap_or("");
    if class.split_whitespace().any(|item| item == "left-align") {
        "left"
    } else if class.split_whitespace().any(|item| item == "right-align") {
        "right"
    } else {
        "center"
    }
}

fn class(node: &Element, target: &str) -> bool {
    node.attr("class").is_some_and(|value| {
        value.split_whitespace().any(|item| item == target)
    })
}

fn table(
    node: &Element, display: bool, in_cases: bool,
) -> Result<String, MathError> {
    if node.children.iter().any(
        |child| matches!(child, Node::Text(value) if !value.trim().is_empty()),
    ) {
        return Err(unsupported(node));
    }
    let rows: Vec<_> = node.elements().collect();
    if rows.is_empty() {
        return Err(unsupported(node));
    }
    let mut cells = Vec::new();
    for row in &rows {
        if row.name != "mtr" {
            return Err(unsupported(row));
        }
        let values: Vec<_> = row.elements().collect();
        if values.is_empty() || row.children.iter().any(|child| matches!(child, Node::Text(value) if !value.trim().is_empty())) {
            return Err(unsupported(row));
        }
        for cell in &values {
            if cell.name != "mtd"
                || cell.attr("rowspan").is_some()
                || cell.attr("columnspan").is_some()
            {
                return Err(unsupported(cell));
            }
        }
        if cells
            .first()
            .is_some_and(|first: &Vec<&Element>| first.len() != values.len())
        {
            return Err(unsupported(row));
        }
        cells.push(values);
    }
    let count = cells[0].len();
    let cases = in_cases || class(node, "cases");
    let aligned = !cases
        && (["aligned", "multiline-equation", "menv-alignlike"]
            .iter()
            .any(|name| class(node, name))
            || (count >= 2
                && count % 2 == 0
                && cells.iter().all(|row| {
                    row.iter().enumerate().all(|(i, cell)| {
                        alignment(cell)
                            == if i % 2 == 0 { "right" } else { "left" }
                    })
                })));
    if aligned {
        let mut output = String::new();
        for row in &cells {
            let mut body = String::new();
            for (index, cell) in row.iter().enumerate() {
                if index != 0 {
                    body.push_str("<m:r><m:t>&amp;</m:t></m:r>");
                }
                body.push_str(&sequence(cell, &cell.children, display)?);
            }
            output.push_str(&slot("e", body));
        }
        return Ok(wrap("eqArr", output));
    }
    let alignments: Vec<_> =
        (0..count).map(|index| alignment(cells[0][index])).collect();
    let mut groups = String::new();
    let mut start = 0;
    while start < count {
        let mut end = start + 1;
        while end < count && alignments[end] == alignments[start] {
            end += 1;
        }
        groups.push_str(&wrap(
            "mc",
            wrap(
                "mcPr",
                property("count", &(end - start).to_string())
                    + &property("mcJc", alignments[start]),
            ),
        ));
        start = end;
    }
    let props = "<m:baseJc m:val=\"center\"/><m:plcHide m:val=\"on\"/>"
        .to_owned()
        + &wrap("mcs", groups);
    let mut body = wrap("mPr", props);
    for row in cells {
        let mut content = String::new();
        for cell in row {
            content
                .push_str(&slot("e", sequence(cell, &cell.children, display)?));
        }
        body.push_str(&wrap("mr", content));
    }
    let matrix = wrap("m", body);
    Ok(if cases && !in_cases {
        delimiter("{", "", "", vec![matrix])
    } else {
        matrix
    })
}

fn enclosure(node: &Element, display: bool) -> Result<String, MathError> {
    let values: Vec<_> =
        node.attr("notation").unwrap_or("box").split_whitespace().collect();
    let has = |value| values.contains(&value);
    let mut props = String::new();
    let sides = ["top", "bottom", "left", "right"];
    let selected_sides = sides.iter().any(|side| has(side));
    let strike = has("updiagonalstrike")
        || has("downdiagonalstrike")
        || has("horizontalstrike")
        || has("verticalstrike");
    if !has("box") && (selected_sides || strike) {
        for (side, property_name) in [
            ("top", "hideTop"),
            ("bottom", "hideBot"),
            ("left", "hideLeft"),
            ("right", "hideRight"),
        ] {
            props.push_str(&property(
                property_name,
                if has(side) { "off" } else { "on" },
            ));
        }
    }
    for (notation, name) in [
        ("updiagonalstrike", "strikeBLTR"),
        ("downdiagonalstrike", "strikeTLBR"),
        ("horizontalstrike", "strikeH"),
        ("verticalstrike", "strikeV"),
    ] {
        if has(notation) {
            props.push_str(&property(name, "on"));
        }
    }
    // The binding table maps menclose generally to borderBox. Unrecognised
    // notations retain that generic box approximation, without new errors.
    Ok(wrap(
        "borderBox",
        wrap("borderBoxPr", props)
            + &slot("e", sequence(node, &node.children, display)?),
    ))
}

fn prescripts(node: &Element, display: bool) -> Result<String, MathError> {
    if node.children.iter().any(
        |child| matches!(child, Node::Text(value) if !value.trim().is_empty()),
    ) {
        return Err(unsupported(node));
    }
    let parts: Vec<_> = node.elements().collect();
    let markers: Vec<_> = parts
        .iter()
        .enumerate()
        .filter(|(_, node)| node.name == "mprescripts")
        .map(|(index, _)| index)
        .collect();
    let [marker] = markers.as_slice() else {
        return Err(unsupported(node));
    };
    let marker = *marker;
    if marker == 0 || parts.len() != marker + 3 || (marker - 1) % 2 != 0 {
        return Err(unsupported(node));
    }
    if !parts[marker].children.is_empty() {
        return Err(unsupported(parts[marker]));
    }
    if !parts[1..marker]
        .iter()
        .all(|part| part.name == "mrow" && part.children.is_empty())
    {
        return Err(unsupported(node));
    }
    Ok(wrap(
        "sPre",
        slot("sub", element(parts[marker + 1], display)?)
            + &slot("sup", element(parts[marker + 2], display)?)
            + &slot("e", element(parts[0], display)?),
    ))
}
