//! Public MathML Core mapping, recovery and parser-boundary contracts.

use codex::styling::{MathStyle, to_style};
use writer::math::latex_mathml::MathError;
use writer::math::omml::{fallback_text, from_mathml};

#[path = "common/math_xml.rs"]
mod math_xml;

use math_xml::{Node, parse};

fn source(body: &str, display: bool) -> String {
    format!(
        "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"{}>{body}</math>",
        if display { " display=\"block\"" } else { "" },
    )
}

fn convert(body: &str, display: bool) -> Node {
    let input = source(body, display);
    let result = from_mathml(&input).unwrap_or_else(|error| {
        panic!("{input}: {error}");
    });
    assert!(result.starts_with("<m:oMath>"), "{result}");
    assert!(result.ends_with("</m:oMath>"), "{result}");
    assert!(!result.contains("xmlns"), "{result}");
    assert!(!result.contains("m:oMathPara"), "{result}");
    let root = parse(&result);
    assert_eq!(root.name, "m:oMath");
    root
}

fn all<'a>(root: &'a Node, name: &str) -> Vec<&'a Node> {
    let mut result = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.name == name {
            result.push(node);
        }
        pending.extend(node.children.iter().rev());
    }
    result
}

fn first<'a>(root: &'a Node, name: &str) -> &'a Node {
    all(root, name).first().copied().unwrap_or_else(|| {
        panic!("missing {name} in {root:?}");
    })
}

fn child<'a>(root: &'a Node, name: &str) -> &'a Node {
    root.children.iter().find(|node| node.name == name).unwrap_or_else(|| {
        panic!("missing direct {name} in {root:?}");
    })
}

fn attribute<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    node.attrs.get(key).map(String::as_str)
}

fn property<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    all(node, name).first().and_then(|node| attribute(node, "m:val"))
}

fn text(root: &Node) -> String {
    all(root, "m:t").iter().map(|node| node.text.as_str()).collect()
}

fn unsupported(input: &str) -> (String, String) {
    let error = from_mathml(input)
        .expect_err("foreign or unsupported MathML must be rejected");
    let MathError::Unsupported { element, detail } = error else {
        panic!("expected Unsupported for {input}: {error}");
    };
    assert!(detail.chars().count() <= 80, "{detail}");
    (element, detail)
}

#[test]
fn one_fragment_and_identifier_number_operator_text_run_properties() {
    for (body, value, style) in [
        ("<mi>x</mi>", "x", None),
        ("<mi>α</mi>", "α", None),
        ("<mi>Γ</mi>", "Γ", Some("p")),
        ("<mi>argmax</mi>", "argmax", Some("p")),
        ("<mi mathvariant=\"normal\">d</mi>", "d", Some("p")),
        ("<mn>123</mn>", "123", Some("p")),
        ("<mo>≤</mo>", "≤", Some("p")),
    ] {
        let inline = convert(body, false);
        let display = convert(body, true);
        assert_eq!(text(&inline), value, "{body}");
        assert_eq!(property(&inline, "m:sty"), style, "{body}");
        assert_eq!(text(&display), value, "{body}");
        assert!(all(&inline, "m:func").is_empty());
        assert!(all(&inline, "w:rFonts").is_empty());
    }
    let operator =
        convert("<mo minsize=\"4em\" largeop=\"true\">mod</mo>", false);
    assert_eq!(property(&operator, "m:opEmu"), Some("on"));
    assert_eq!(text(first(&operator, "m:box")), "mod");
    let normal = convert("<mtext> if </mtext><ms>word</ms>", false);
    let texts = all(&normal, "m:t");
    assert_eq!(texts[0].text, " if ");
    assert_eq!(attribute(texts[0], "xml:space"), Some("preserve"));
    assert_eq!(texts[1].text, "\"word\"");
    assert_eq!(all(&normal, "m:nor").len(), 2);
    let quoted = convert("<ms lquote=\"‘\" rquote=\"’\">word</ms>", false);
    assert_eq!(text(&quoted), "‘word’");
}

#[test]
fn every_codex_style_and_base_family_folds_to_its_word_properties() {
    let styles = [
        (MathStyle::Bold, Some("b"), None),
        (MathStyle::Italic, None, None),
        (MathStyle::BoldItalic, Some("bi"), None),
        (MathStyle::Script, Some("p"), Some("script")),
        (MathStyle::BoldScript, Some("b"), Some("script")),
        (MathStyle::Fraktur, Some("p"), Some("fraktur")),
        (MathStyle::BoldFraktur, Some("b"), Some("fraktur")),
        (MathStyle::DoubleStruck, Some("p"), Some("double-struck")),
        (MathStyle::SansSerif, Some("p"), Some("sans-serif")),
        (MathStyle::SansSerifBold, Some("b"), Some("sans-serif")),
        (MathStyle::SansSerifItalic, Some("i"), Some("sans-serif")),
        (MathStyle::SansSerifBoldItalic, Some("bi"), Some("sans-serif")),
        (MathStyle::Monospace, Some("p"), Some("monospace")),
    ];
    let mut checked = 0;
    for base in ('A'..='Z')
        .chain('a'..='z')
        .chain('Α'..='Ω')
        .chain('α'..='ω')
        .chain('0'..='9')
    {
        for (style, word_style, script) in styles {
            let styled = to_style(base, style).next().unwrap();
            if styled == base {
                continue;
            }
            let root = convert(&format!("<mi>{styled}</mi>"), false);
            assert_eq!(text(&root), base.to_string(), "{style:?}: {styled}");
            assert_eq!(
                property(&root, "m:sty"),
                word_style,
                "{style:?}: {styled}"
            );
            assert_eq!(property(&root, "m:scr"), script, "{style:?}: {styled}");
            assert_eq!(
                fallback_text(&source(&format!("<mi>{styled}</mi>"), false)),
                base.to_string()
            );
            checked += 1;
        }
    }
    assert!(checked > 900, "incomplete style family coverage: {checked}");
}

#[test]
fn letterlike_holes_variation_selectors_and_mixed_style_runs_stay_distinct() {
    for (value, base, script, style) in [
        ("ℝ", "R", Some("double-struck"), Some("p")),
        ("ℱ", "F", Some("script"), Some("p")),
        ("ℌ", "H", Some("fraktur"), Some("p")),
        ("ℎ", "h", None, None),
        ("𝒜\u{fe00}", "A", Some("script"), Some("p")),
        ("𝒜\u{fe01}", "A", Some("script"), Some("p")),
    ] {
        let root = convert(&format!("<mi>{value}</mi>"), false);
        assert_eq!(text(&root), base);
        assert_eq!(property(&root, "m:scr"), script);
        assert_eq!(property(&root, "m:sty"), style);
    }
    let root = convert("<mi>x𝐀𝒜</mi>", false);
    let runs = all(&root, "m:r");
    assert_eq!(runs.len(), 3);
    assert_eq!(text(runs[0]), "x");
    assert_eq!(text(runs[1]), "A");
    assert_eq!(property(runs[1], "m:sty"), Some("b"));
    assert_eq!(property(runs[2], "m:scr"), Some("script"));
}

#[test]
fn semantics_style_wrappers_and_invisible_operators_normalize_without_text_loss()
 {
    let root = convert(
        concat!(
            "<semantics><mstyle mathcolor=\"red\" mathvariant=\"bold\">",
            "<mpadded><maction><mi>x</mi></maction></mpadded></mstyle>",
            "<annotation encoding=\"application/x-tex\">ignored</annotation></semantics>",
            "<mo>&#x2061;</mo><mo>&#x2062;</mo><mo>&#x2063;</mo>",
            "<mrow>before<mi>y</mi>after</mrow>",
        ),
        false,
    );
    assert_eq!(text(&root), "xbeforeyafter");
    assert!(all(&root, "w:color").is_empty());
    assert!(all(&root, "m:d").is_empty());
    assert_eq!(
        fallback_text(&source("<mrow>before<mi>𝑦</mi>after</mrow>", false,)),
        "beforeyafter"
    );
}

#[test]
fn positive_spaces_use_exact_thresholds_and_nonpositive_or_widthless_disappear()
{
    for (width, expected) in [
        ("0.17em", "\u{2009}"),
        ("0.1701em", "\u{2005}"),
        ("0.23em", "\u{2005}"),
        ("0.2301em", "\u{2004}"),
        ("0.5em", "\u{2004}"),
        ("0.5001em", "\u{2001}"),
        ("1.8em", "\u{2001}"),
        ("1.8001em", "\u{2001}\u{2001}"),
    ] {
        let root = convert(
            &format!("<mi>a</mi><mspace width=\"{width}\"/><mi>b</mi>"),
            false,
        );
        assert_eq!(text(&root), format!("a{expected}b"), "{width}");
    }
    let body = concat!(
        "<mi>a</mi><mspace/><mspace style=\"margin-left:-1em\"/>",
        "<mspace width=\"0em\"/><mspace width=\"-1pt\"/><mi>b</mi>",
    );
    assert_eq!(text(&convert(body, false)), "ab");
    assert_eq!(fallback_text(&source(body, false)), "a    b");
    unsupported(&source("<mspace width=\"oops\"/>", false));
}

#[test]
fn fractions_roots_scripts_and_empty_slots_have_editable_shapes() {
    let fraction = convert("<mfrac><mi>a</mi><mi>b</mi></mfrac>", false);
    assert_eq!(property(&fraction, "m:type"), Some("bar"));
    assert_eq!(text(first(&fraction, "m:num")), "a");
    assert_eq!(text(first(&fraction, "m:den")), "b");
    let no_bar = convert(
        "<mfrac linethickness=\"0\"><mi>a</mi><mi>b</mi></mfrac>",
        false,
    );
    assert_eq!(property(&no_bar, "m:type"), Some("noBar"));
    let square =
        convert("<msqrt><mi>x</mi><mo>+</mo><mn>1</mn></msqrt>", false);
    assert_eq!(property(&square, "m:degHide"), Some("on"));
    assert_eq!(text(child(first(&square, "m:rad"), "m:e")), "x+1");
    let root = convert("<mroot><mi>x</mi><mn>3</mn></mroot>", false);
    assert_eq!(text(child(first(&root, "m:rad"), "m:deg")), "3");
    assert_eq!(text(child(first(&root, "m:rad"), "m:e")), "x");
    for (body, shape) in [
        ("<msub><mi>x</mi><mn>1</mn></msub>", "m:sSub"),
        ("<msup><mi>x</mi><mn>2</mn></msup>", "m:sSup"),
        ("<msubsup><mi>x</mi><mn>1</mn><mn>2</mn></msubsup>", "m:sSubSup"),
    ] {
        let root = convert(body, false);
        assert_eq!(text(child(first(&root, shape), "m:e")), "x");
        assert!(all(&root, "m:nary").is_empty());
    }
    for body in ["", "<mrow/>", "<msup><mrow/><mrow/></msup>"] {
        assert!(text(&convert(body, false)).contains('\u{200b}'), "{body}");
    }
}

#[test]
fn nary_operators_consume_exactly_one_following_sibling_and_keep_hidden_limits()
{
    for operator in ["∫", "∬", "∭", "∮", "∯", "∰", "∏", "∐", "∑"]
    {
        let body = format!(
            "<msubsup><mo>{operator}</mo><mn>1</mn><mn>3</mn></msubsup><mi>x</mi><mi>y</mi>"
        );
        let root = convert(&body, true);
        let nary = first(&root, "m:nary");
        assert_eq!(property(nary, "m:chr"), Some(operator));
        assert_eq!(property(nary, "m:limLoc"), Some("subSup"));
        assert_eq!(property(nary, "m:subHide"), Some("off"));
        assert_eq!(property(nary, "m:supHide"), Some("off"));
        assert_eq!(text(child(nary, "m:e")), "x");
        assert_eq!(root.children.len(), 2, "{body}: {root:?}");
        assert_eq!(text(&root.children[1]), "y");
    }
    let bare = convert("<mo>∫</mo>", false);
    assert_eq!(property(&bare, "m:subHide"), Some("on"));
    assert_eq!(property(&bare, "m:supHide"), Some("on"));
    assert_eq!(text(child(first(&bare, "m:nary"), "m:e")), "\u{200b}");
    let nested =
        convert("<mo>∫</mo><mrow><mi>x</mi><mi>z</mi></mrow><mi>y</mi>", false);
    assert_eq!(text(child(first(&nested, "m:nary"), "m:e")), "xz");
    assert_eq!(text(&nested.children[1]), "y");
}

#[test]
fn movable_limits_use_scripts_inline_and_under_over_in_display() {
    let body = concat!(
        "<munderover><mo movablelimits=\"true\">lim</mo>",
        "<mi>a</mi><mi>b</mi></munderover>",
    );
    let inline = convert(body, false);
    let scripts = first(&inline, "m:sSubSup");
    assert_eq!(text(child(scripts, "m:sub")), "a");
    assert_eq!(text(child(scripts, "m:sup")), "b");
    let display = convert(body, true);
    let lower = first(&display, "m:limLow");
    let upper = first(child(lower, "m:e"), "m:limUpp");
    assert_eq!(text(child(lower, "m:lim")), "a");
    assert_eq!(text(child(upper, "m:lim")), "b");
    let sum = concat!(
        "<munderover><mo movablelimits=\"true\">∑</mo>",
        "<mi>a</mi><mi>b</mi></munderover><mi>x</mi>",
    );
    assert_eq!(property(&convert(sum, false), "m:limLoc"), Some("subSup"));
    assert_eq!(property(&convert(sum, true), "m:limLoc"), Some("undOvr"));
}

#[test]
fn accents_bars_and_group_characters_use_the_specified_special_cases_first() {
    for (spacing, combining) in [
        ("^", "\u{302}"),
        ("ˆ", "\u{302}"),
        (".", "\u{307}"),
        ("˙", "\u{307}"),
        ("~", "\u{303}"),
        ("˜", "\u{303}"),
        ("¯", "\u{304}"),
        ("˘", "\u{306}"),
        ("ˇ", "\u{30c}"),
        ("´", "\u{301}"),
        ("`", "\u{300}"),
        ("¨", "\u{308}"),
        ("→", "\u{20d7}"),
        ("\u{20d7}", "\u{20d7}"),
        ("\u{302}", "\u{302}"),
    ] {
        let body = format!(
            "<mover accent=\"true\"><mi>x</mi><mo stretchy=\"false\">{spacing}</mo></mover>"
        );
        let root = convert(&body, false);
        assert_eq!(property(first(&root, "m:acc"), "m:chr"), Some(combining));
        assert_eq!(text(&root), "x");
    }
    for (tag, position) in [("mover", "top"), ("munder", "bot")] {
        for mark in ["‾", "¯", "\u{332}", "\u{305}", "_"] {
            let body = format!(
                "<{tag} accent=\"true\"><mi>x</mi><mo stretchy=\"true\">{mark}</mo></{tag}>"
            );
            assert_eq!(
                property(first(&convert(&body, false), "m:bar"), "m:pos"),
                Some(position)
            );
        }
    }
    for (tag, position, vertical) in
        [("mover", "top", "bot"), ("munder", "bot", "top")]
    {
        for mark in ["⏞", "⏟", "⎴", "⎵"] {
            let body = format!("<{tag}><mi>x</mi><mo>{mark}</mo></{tag}>");
            let root = convert(&body, false);
            let group = first(&root, "m:groupChr");
            assert_eq!(property(group, "m:chr"), Some(mark));
            assert_eq!(property(group, "m:pos"), Some(position));
            assert_eq!(property(group, "m:vertJc"), Some(vertical));
        }
    }
    let labelled = convert(
        concat!(
            "<mover><mover accent=\"true\"><mi>x</mi><mo>⏞</mo></mover>",
            "<mi>n</mi></mover>",
        ),
        true,
    );
    let upper = first(&labelled, "m:limUpp");
    assert_eq!(text(child(upper, "m:lim")), "n");
    assert_eq!(all(child(upper, "m:e"), "m:groupChr").len(), 1);
}

#[test]
fn fences_empty_edges_and_inner_separators_keep_delimiter_structure() {
    let root = convert(
        concat!(
            "<mrow><mo>(</mo><mi>a</mi><mo stretchy=\"true\">|</mo>",
            "<mi>b</mi><mo>)</mo></mrow>",
        ),
        false,
    );
    let delimiter = first(&root, "m:d");
    assert_eq!(property(delimiter, "m:begChr"), Some("("));
    assert_eq!(property(delimiter, "m:endChr"), Some(")"));
    assert_eq!(property(delimiter, "m:sepChr"), Some("|"));
    assert_eq!(
        delimiter
            .children
            .iter()
            .filter(|node| node.name == "m:e")
            .map(text)
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(all(delimiter, "m:grow").len(), 1);
    for (begin, end) in [
        ("[", "]"),
        ("{", "}"),
        ("⟨", "⟩"),
        ("⌊", "⌋"),
        ("⌈", "⌉"),
        ("⟦", "⟧"),
        ("|", "|"),
        ("‖", "‖"),
    ] {
        let root = convert(
            &format!("<mrow><mo>{begin}</mo><mi>x</mi><mo>{end}</mo></mrow>"),
            false,
        );
        assert_eq!(property(&root, "m:begChr"), Some(begin));
        assert_eq!(property(&root, "m:endChr"), Some(end));
        assert_eq!(text(&root), "x");
    }
    for separator in ["‖", "∣"] {
        let root = convert(
            &format!(
                "<mrow><mo>(</mo><mi>a</mi><mo stretchy=\"true\">{separator}</mo><mi>b</mi><mo>)</mo></mrow>"
            ),
            false,
        );
        assert_eq!(property(&root, "m:sepChr"), Some(separator));
        assert_eq!(text(&root), "ab");
    }
    for (left, right, begin, end) in [
        ("<mo/>", "<mo>|</mo>", "", "|"),
        ("<mo>{</mo>", "<mo>&#x2063;</mo>", "{", ""),
        ("<mo>&#x2063;</mo>", "<mo/>", "", ""),
    ] {
        let root =
            convert(&format!("<mrow>{left}<mi>x</mi>{right}</mrow>"), false);
        assert_eq!(property(&root, "m:begChr"), Some(begin));
        assert_eq!(property(&root, "m:endChr"), Some(end));
        assert_eq!(text(&root), "x");
    }
    for body in [
        "<mrow><mo stretchy=\"false\">(</mo><mi>x</mi><mo>)</mo></mrow>",
        "<mrow><mo>(</mo><mi>x</mi><mo stretchy=\"false\">)</mo></mrow>",
    ] {
        let root = convert(body, false);
        assert!(all(&root, "m:d").is_empty());
        assert_eq!(text(&root), "(x)");
    }
}

#[test]
fn aligned_tables_and_matrix_column_groups_keep_rows_cells_and_empty_slots() {
    let aligned = concat!(
        "<mtable><mtr><mtd style=\"text-align:right\"><mi>a</mi></mtd>",
        "<mtd style=\"text-align:left\"><mi>b</mi></mtd></mtr>",
        "<mtr><mtd style=\"text-align:right\"><mi>c</mi></mtd>",
        "<mtd style=\"text-align:left\"><mi>d</mi></mtd></mtr></mtable>",
    );
    let root = convert(aligned, true);
    let array = first(&root, "m:eqArr");
    assert_eq!(
        array.children.iter().map(text).collect::<Vec<_>>(),
        ["a&b", "c&d"]
    );
    assert!(all(&root, "m:m").is_empty());
    for class in ["aligned", "multiline-equation", "menv-alignlike"] {
        let body = format!(
            "<mtable class=\"other {class}\"><mtr><mtd><mi>x</mi></mtd></mtr></mtable>"
        );
        assert_eq!(all(&convert(&body, true), "m:eqArr").len(), 1);
    }
    let matrix = convert(
        concat!(
            "<mtable><mtr><mtd class=\"left-align\"><mi>a</mi></mtd>",
            "<mtd style=\"text-align:left\"/><mtd class=\"right-align\"><mi>b</mi></mtd>",
            "</mtr><mtr><mtd><mi>c</mi></mtd><mtd><mi>d</mi></mtd>",
            "<mtd><mi>e</mi></mtd></mtr></mtable>",
        ),
        true,
    );
    let grid = first(&matrix, "m:m");
    assert_eq!(property(grid, "m:baseJc"), Some("center"));
    assert_eq!(property(grid, "m:plcHide"), Some("on"));
    assert_eq!(
        all(grid, "m:count")
            .iter()
            .map(|node| attribute(node, "m:val").unwrap())
            .collect::<Vec<_>>(),
        ["2", "1"]
    );
    assert_eq!(
        all(grid, "m:mcJc")
            .iter()
            .map(|node| attribute(node, "m:val").unwrap())
            .collect::<Vec<_>>(),
        ["left", "right"]
    );
    assert_eq!(all(grid, "m:mr").len(), 2);
    assert_eq!(all(grid, "m:e").len(), 6);
    assert!(text(grid).contains('\u{200b}'));
    let cases = convert(
        "<mtable class=\"cases\"><mtr><mtd><mi>x</mi></mtd></mtr></mtable>",
        true,
    );
    assert_eq!(property(&cases, "m:begChr"), Some("{"));
    assert_eq!(property(&cases, "m:endChr"), Some(""));
    assert_eq!(all(&cases, "m:m").len(), 1);
}

#[test]
fn phantom_enclosures_and_prescripts_have_the_frozen_word_nodes() {
    let phantom = convert("<mphantom><mi>x</mi></mphantom>", false);
    assert_eq!(property(&phantom, "m:show"), Some("off"));
    assert_eq!(text(first(&phantom, "m:phant")), "x");
    let strike = convert(
        "<menclose notation=\"updiagonalstrike\"><mi>x</mi></menclose>",
        false,
    );
    assert_eq!(property(&strike, "m:strikeBLTR"), Some("on"));
    for side in ["m:hideTop", "m:hideBot", "m:hideLeft", "m:hideRight"] {
        assert_eq!(property(&strike, side), Some("on"));
    }
    let sides = convert(
        "<menclose notation=\"top right\"><mi>x</mi></menclose>",
        false,
    );
    assert_eq!(property(&sides, "m:hideTop"), Some("off"));
    assert_eq!(property(&sides, "m:hideRight"), Some("off"));
    assert_eq!(property(&sides, "m:hideBot"), Some("on"));
    let pre = convert(
        concat!(
            "<mmultiscripts><mi>𝑋</mi><mrow/><mrow/><mprescripts/>",
            "<mn>2</mn><mn>1</mn></mmultiscripts>",
        ),
        false,
    );
    let pre = first(&pre, "m:sPre");
    assert_eq!(
        pre.children.iter().map(|node| node.name.as_str()).collect::<Vec<_>>(),
        ["m:sub", "m:sup", "m:e"]
    );
    assert_eq!(text(child(pre, "m:sub")), "2");
    assert_eq!(text(child(pre, "m:sup")), "1");
    assert_eq!(text(child(pre, "m:e")), "X");
}

#[test]
fn unsupported_elements_arities_tables_and_postscript_shapes_are_specific() {
    for (body, offending) in [
        ("<mfenced><mi>x</mi></mfenced>", "mfenced"),
        ("<mglyph/>", "mglyph"),
        ("<merror/>", "merror"),
        ("<mfrac><mi>x</mi></mfrac>", "mfrac"),
        ("<mroot><mi>x</mi></mroot>", "mroot"),
        ("<msup><mi>x</mi><mi>y</mi><mi>z</mi></msup>", "msup"),
        ("<mi><mi>x</mi></mi>", "mi"),
        ("<mtable><mlabeledtr><mtd/></mlabeledtr></mtable>", "mlabeledtr"),
        ("<mtable><mtr><mtd rowspan=\"2\"/></mtr></mtable>", "mtd"),
        ("<mtable><mtr><mtd columnspan=\"2\"/></mtr></mtable>", "mtd"),
        ("<mtable><mtr><mtd/></mtr><mtr><mtd/><mtd/></mtr></mtable>", "mtr"),
        (
            "<mmultiscripts><mi>X</mi><mn>3</mn><mn>4</mn><mprescripts/><mn>2</mn><mn>1</mn></mmultiscripts>",
            "mmultiscripts",
        ),
        (
            "<mmultiscripts><mi>X</mi><mprescripts/><mn>2</mn></mmultiscripts>",
            "mmultiscripts",
        ),
        (
            "<mmultiscripts><mprescripts/><mn>2</mn><mn>1</mn></mmultiscripts>",
            "mmultiscripts",
        ),
        (
            "<mmultiscripts><mi>X</mi><mprescripts/><mprescripts/><mn>1</mn></mmultiscripts>",
            "mmultiscripts",
        ),
    ] {
        assert_eq!(unsupported(&source(body, false)).0, offending, "{body}");
    }
}

#[test]
fn namespace_entity_and_xml_boundaries_fail_without_expanding_foreign_content()
{
    for input in [
        "<math xmlns=\"urn:foreign\"><mi>x</mi></math>",
        "<math><x:mi>x</x:mi></math>",
        "<math><mi xmlns=\"urn:foreign\">x</mi></math>",
        "<:math/>",
        "<math/><math/>",
        "<math><mi>x</math>",
        "<math><mi>&#0;</mi></math>",
        "<math><mi a=\"1\" a=\"2\">x</mi></math>",
        "<math xmlns:a=\"urn:a\" xmlns:b=\"urn:a\"><mi a:x=\"1\" b:x=\"2\">x</mi></math>",
        "<!DOCTYPE math [<!ENTITY x 'expanded'>]><math>&x;</math>",
    ] {
        unsupported(input);
        assert_eq!(fallback_text(input), fallback_text(input), "{input}");
    }
    assert_eq!(
        unsupported("<math><mi>&ApplyFunction;</mi></math>").0,
        "entity &ApplyFunction;"
    );
    assert_eq!(
        fallback_text(
            "<!DOCTYPE math [<!ENTITY x 'expanded'>]><math>&x;</math>"
        ),
        "&x;"
    );
    let valid = concat!(
        "<?xml version=\"1.0\"?><q:math xmlns:q=\"http://www.w3.org/1998/Math/MathML\">",
        "<q:mtext><![CDATA[a<b]]>&amp;&#945;&#x3B2;</q:mtext></q:math>",
    );
    assert_eq!(text(&parse(&from_mathml(valid).unwrap())), "a<b&αβ");
    assert!(from_mathml("<math><mi>x</mi></math>").is_ok());
}

#[test]
fn resource_bounds_fail_before_recursive_work_and_recovery_remains_total() {
    assert_eq!(unsupported(&" ".repeat(8 * 1024 * 1024 + 1)).0, "input limit");
    let nodes = format!("<math>{}</math>", "<mi/>".repeat(100_000));
    assert_eq!(unsupported(&nodes).0, "node limit");
    let deep = format!(
        "<math>{}<mi>x</mi>{}</math>",
        "<mrow>".repeat(5_000),
        "</mrow>".repeat(5_000)
    );
    assert_eq!(unsupported(&deep).0, "depth limit");
    assert_eq!(fallback_text(&deep), "x");
    for input in [
        "",
        "<",
        "<math>",
        "<math><mi>𝑥",
        "<math>x & y<mi>z",
        "<math><mtext>&unknown;<![CDATA[<visible>]]></wrong>",
    ] {
        let first = fallback_text(input);
        assert_eq!(fallback_text(input), first);
        assert!(!first.contains("<math>"));
        assert!(!first.contains("\\math"));
    }
    assert_eq!(fallback_text("<math><mi>𝑥"), "x");
    assert_eq!(fallback_text("<math>x & y<mi>z"), "x & yz");
}

#[test]
fn fallback_flattening_keeps_script_token_boundaries_roots_tables_and_fences() {
    for (body, expected) in [
        ("<msup><mi>x</mi><mn>12</mn></msup>", "x^12"),
        ("<msup><mi>x</mi><mrow><mi>i</mi><mi>j</mi></mrow></msup>", "x^(ij)"),
        ("<msubsup><mi>x</mi><mn>1</mn><mn>2</mn></msubsup>", "x_1^2"),
        ("<mfrac><mi>a</mi><mi>b</mi></mfrac>", "(a)/(b)"),
        ("<msqrt><mi>x</mi></msqrt>", "√(x)"),
        ("<mroot><mi>x</mi><mn>3</mn></mroot>", "(3)√(x)"),
        ("<munder><mi>x</mi><mi>a</mi></munder>", "x_(a)"),
        ("<mover><mi>x</mi><mi>b</mi></mover>", "x^(b)"),
        (
            "<munderover><mi>x</mi><mi>a</mi><mi>b</mi></munderover>",
            "x_(a)^(b)",
        ),
        ("<mrow><mo>(</mo><mi>ℝ</mi><mo>)</mo></mrow>", "(R)"),
        (
            "<mtable><mtr><mtd><mi>a</mi></mtd><mtd><mi>b</mi></mtd></mtr><mtr><mtd><mi>c</mi></mtd><mtd><mi>d</mi></mtd></mtr></mtable>",
            "a, b; c, d",
        ),
        ("<mtext> if </mtext><ms>yes</ms>", " if \"yes\""),
    ] {
        assert_eq!(fallback_text(&source(body, false)), expected, "{body}");
    }
}
