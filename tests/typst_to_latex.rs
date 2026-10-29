//! Pure Typst-math conversion, recorded corpus and public symbol contracts.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use typst_syntax::ast::{self, AstNode};
use typst_syntax::{Source, SyntaxNode};
use writer::error::{Diagnostic, Level};
use writer::math::typst_to_latex::{
    char_command, command_package, convert, convert_ast, symbol,
};

struct Fixture {
    name: String,
    path: PathBuf,
    display: bool,
    body: String,
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/math/typst")
}

fn fixtures() -> Vec<Fixture> {
    let mut paths: Vec<_> = fs::read_dir(fixture_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "typ"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 36, "keep the exact shared Typst corpus");
    paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
            assert!(name.starts_with(&format!("{:02}-", index + 1)), "{name}");
            let text = fs::read_to_string(&path).unwrap();
            assert!(text.ends_with('\n') && !text.contains('\r'), "{name}: LF");
            let (mode, body) = text.split_once('\n').unwrap();
            let display = match mode {
                "// mode: display" => true,
                "// mode: inline" => false,
                other => panic!("{name}: invalid mode {other:?}"),
            };
            Fixture {
                name,
                path,
                display,
                body: body.strip_suffix('\n').unwrap().to_string(),
            }
        })
        .collect()
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|diagnostic| diagnostic.code).collect()
}

fn compact(tex: &str) -> String {
    tex.chars().filter(|ch| !ch.is_whitespace()).collect()
}

/// The ONLY generic oracle normalization, as required by math.md §10.
fn oracle_normalize(tex: &str) -> String {
    tex.chars()
        .filter(|ch| !ch.is_whitespace() && !matches!(ch, '{' | '}'))
        .collect()
}

fn diagnostic_range(diagnostic: &Diagnostic, body: &str) -> (usize, usize) {
    assert_eq!(diagnostic.level, Level::Warning);
    assert_eq!(diagnostic.path, None);
    assert_eq!(diagnostic.span, None);
    let pattern =
        Regex::new(r"at bytes (\d+)\.\.(\d+) of the equation").unwrap();
    let captures = pattern.captures(&diagnostic.message).unwrap_or_else(|| {
        panic!("missing equation-relative range: {diagnostic:?}")
    });
    let start = captures[1].parse().unwrap();
    let end = captures[2].parse().unwrap();
    assert!(start <= end && end <= body.len(), "{body:?}: {diagnostic:?}");
    assert!(body.is_char_boundary(start) && body.is_char_boundary(end));
    (start, end)
}

fn equation(node: &SyntaxNode) -> Option<ast::Equation<'_>> {
    node.cast::<ast::Equation>().or_else(|| node.children().find_map(equation))
}

#[test]
fn complete_corpus_has_deterministic_body_and_diagnostic_snapshots() {
    for fixture in fixtures() {
        let (tex, diagnostics) = convert(&fixture.body);
        assert_eq!((tex.clone(), diagnostics.clone()), convert(&fixture.body));
        for diagnostic in &diagnostics {
            diagnostic_range(diagnostic, &fixture.body);
        }
        assert!(!tex.contains("\\documentclass"), "{}: {tex}", fixture.name);
        assert!(!tex.contains("\\begin{document}"), "{}: {tex}", fixture.name);
        assert!(!tex.contains("\\usepackage"), "{}: {tex}", fixture.name);
        assert!(!tex.starts_with("\\[") && !tex.starts_with("\\("));
        insta::assert_snapshot!(fixture.name.clone(), tex);
        insta::assert_json_snapshot!(
            format!("{}-diagnostics", fixture.name),
            diagnostics
        );
    }
}

#[test]
fn both_ast_entry_points_agree_even_inside_a_larger_numbered_source() {
    for fixture in fixtures() {
        let math = typst_syntax::parse_math(&fixture.body);
        let ast = math.cast::<ast::Math>().unwrap();
        assert_eq!(
            convert_ast(ast),
            convert(&fixture.body),
            "{}",
            fixture.name
        );
        let source = Source::detached(format!(
            "Prefix α with unrelated text.\n\n${}$\nTrailing content.",
            fixture.body
        ));
        let math = equation(source.root()).unwrap().body();
        assert_eq!(math.to_untyped().full_text().to_string(), fixture.body);
        assert_eq!(
            convert_ast(math),
            convert(&fixture.body),
            "{}",
            fixture.name
        );
    }
}

#[test]
fn mapped_functions_attachments_and_styles_keep_the_frozen_tex_shapes() {
    for (source, expected) in [
        ("", ""),
        ("frac(a,b)", r"\frac{a}{b}"),
        ("binom(n,k)", r"\binom{n}{k}"),
        ("sqrt(x)", r"\sqrt{x}"),
        ("root(3,x)", r"\sqrt[3]{x}"),
        ("√x", r"\sqrt{x}"),
        ("∛x", r"\sqrt[3]{x}"),
        ("∜x", r"\sqrt[4]{x}"),
        ("x_1^2", r"x_{1}^{2}"),
        ("x_(i j)", r"x_{i j}"),
        ("a/(b+c)", r"\frac{a}{b+c}"),
        ("abs(x)", r"\left\lvert{}x\right\rvert"),
        ("norm(x)", r"\left\lVert{}x\right\rVert"),
        ("floor(x)", r"\left\lfloor{}x\right\rfloor"),
        ("ceil(x)", r"\left\lceil{}x\right\rceil"),
        ("round(x)", r"\left\lfloor{}x\right\rceil"),
        ("lr(x)", "x"),
        ("hat(x)", r"\hat{x}"),
        ("tilde(x)", r"\tilde{x}"),
        ("grave(x)", r"\grave{x}"),
        ("acute(x)", r"\acute{x}"),
        ("breve(x)", r"\breve{x}"),
        ("caron(x)", r"\check{x}"),
        ("macron(x)", r"\bar{x}"),
        ("diaer(x)", r"\ddot{x}"),
        ("dot(x)", r"\dot{x}"),
        ("dot.double(x)", r"\ddot{x}"),
        ("dot.triple(x)", r"\dddot{x}"),
        ("dot.quad(x)", r"\ddddot{x}"),
        ("arrow(x)", r"\vec{x}"),
        ("arrow.l(x)", r"\overleftarrow{x}"),
        ("arrow.l.r(x)", r"\overleftrightarrow{x}"),
        ("circle(x)", r"\mathring{x}"),
        ("overline(x)", r"\overline{x}"),
        ("underline(x)", r"\underline{x}"),
        ("overbrace(x,l)", r"\overbrace{x}^{l}"),
        ("underbrace(x,l)", r"\underbrace{x}_{l}"),
        ("overbracket(x,l)", r"\overbracket{x}^{l}"),
        ("underbracket(x,l)", r"\underbracket{x}_{l}"),
        ("bold(x)", r"\mathbf{x}"),
        ("upright(x)", r"\mathrm{x}"),
        ("italic(x)", r"\mathit{x}"),
        ("bb(R)", r"\mathbb{R}"),
        ("cal(F)", r"\mathcal{F}"),
        ("frak(x)", r"\mathfrak{x}"),
        ("sans(x)", r"\mathsf{x}"),
        ("mono(x)", r"\mathtt{x}"),
        ("scr(x)", r"\mathscr{x}"),
        ("cancel(x)", r"\cancel{x}"),
        ("op(\"foo\")", r"\operatorname{foo}"),
        ("op(\"foo\",limits:#true)", r"\operatorname*{foo}"),
        ("op(\"foo\",limits:#false)", r"\operatorname{foo}"),
        ("limits(x)", r"\mathop{x}\limits"),
        ("scripts(x)", r"\mathop{x}\nolimits"),
        ("limits(sum)", r"\sum\limits"),
        ("scripts(max)", r"\max\nolimits"),
        ("display(x)", r"{\displaystyle{}x}"),
        ("inline(x)", r"{\textstyle{}x}"),
        ("script(x)", r"{\scriptstyle{}x}"),
        ("sscript(x)", r"{\scriptscriptstyle{}x}"),
    ] {
        let (actual, diagnostics) = convert(source);
        assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
        assert_eq!(compact(&actual), compact(expected), "{source}");
    }
    assert_eq!(convert("x_1^2 + x_(i j)").0, "x_{1}^{2} + x_{i j}");
}

#[test]
fn matrices_vectors_and_cases_own_their_row_separators_and_delimiters() {
    for (source, expected) in [
        ("vec(1,2,3)", r"\begin{pmatrix}1\\2\\3\end{pmatrix}"),
        ("mat(a,b;c,d)", r"\begin{pmatrix}a&b\\c&d\end{pmatrix}"),
        ("mat(delim:\"[\",a,b;c,d)", r"\begin{bmatrix}a&b\\c&d\end{bmatrix}"),
        ("mat(delim:\"{\",a,b;c,d)", r"\begin{Bmatrix}a&b\\c&d\end{Bmatrix}"),
        ("mat(delim:\"|\",a,b;c,d)", r"\begin{vmatrix}a&b\\c&d\end{vmatrix}"),
        ("mat(delim:\"‖\",a,b;c,d)", r"\begin{Vmatrix}a&b\\c&d\end{Vmatrix}"),
        ("mat(delim:#none,a,b;c,d)", r"\begin{matrix}a&b\\c&d\end{matrix}"),
        (
            "cases(1 \"if\" x > 0,0 \"else\")",
            r"\begin{cases}1\text{if}x>0\\0\text{else}\end{cases}",
        ),
    ] {
        let (tex, diagnostics) = convert(source);
        assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
        assert_eq!(compact(&tex), compact(expected), "{source}");
        assert!(!tex.contains("\\begin{aligned}"), "{source}: {tex}");
    }
    for source in ["a &= b", r"a \ b", r"a & = b \ c & = d"] {
        let (tex, diagnostics) = convert(source);
        assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
        assert_eq!(tex.matches("\\begin{aligned}").count(), 1, "{source}");
        assert_eq!(tex.matches("\\end{aligned}").count(), 1, "{source}");
    }
    for source in [r#""&""#, r"\&", r#""\\""#] {
        assert!(!convert(source).0.contains("\\begin{aligned}"), "{source}");
    }
}

#[test]
fn literal_delimiters_text_and_approximate_calls_remain_visible() {
    let (tex, diagnostics) = convert("lr((a/b])");
    assert_eq!(compact(&tex), r"\left(\frac{a}{b}\right]");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let (tex, diagnostics) = convert("alpha(x)");
    assert_eq!(compact(&tex), r"\alpha\left(x\right)");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let (tex, diagnostics) = convert(r##""\\ { } $ & # % _ ^ ~""##);
    assert!(tex.starts_with(r"\text{"), "{tex}");
    assert!(tex.is_ascii() && diagnostics.is_empty(), "{tex}: {diagnostics:?}");
    for escaped in [r"\{", r"\}", r"\$", r"\&", r"\#", r"\%", r"\_"] {
        assert!(tex.contains(escaped), "missing {escaped}: {tex}");
    }
    assert!(tex.contains(r"\textbackslash") || tex.contains(r"\backslash"));
    assert!(tex.contains(r"\textasciicircum") || tex.contains(r"\^"));
    assert!(tex.contains(r"\textasciitilde") || tex.contains(r"\~"));
    assert!(!tex.contains(r"\begin{aligned}"));
    for (source, body) in [
        ("text(\"words\")", "words"),
        ("stretch(x)", "x"),
        ("class(\"ord\",x)", "x"),
        ("attach(X,tl:1,bl:2)", "X"),
        ("primes(2)", "2"),
    ] {
        let (tex, diagnostics) = convert(source);
        assert!(tex.contains(body), "{source}: {tex}");
        assert!(codes(&diagnostics).contains(&"W-TYP-MATH-FN-UNKNOWN"));
        for diagnostic in &diagnostics {
            diagnostic_range(diagnostic, source);
        }
    }
    let (tex, diagnostics) = convert("mid(bar.v)");
    assert!(tex.contains(r"\middle"), "{tex}");
    assert!(codes(&diagnostics).contains(&"W-TYP-MATH-FN-UNKNOWN"));
}

#[test]
fn code_is_one_exclusive_fallback_but_named_literal_hashes_are_allowed() {
    let (tex, diagnostics) = convert("#text[x] + y");
    assert_eq!(tex, r"\text{[typst math: \#text[x] + y]}");
    assert_eq!(codes(&diagnostics), ["E-TYP-MATH-CODE"]);
    for source in [
        "α + #text[x] + foo(y)",
        "frac(1,#2)",
        "#(1+2)",
        "mat(delim:#\"[\",1,2)",
        "frac(1,2,gap:#-3)",
        "frac(1,2,gap:#-1pt)",
        "frac(1,2,gap:#(1+2))",
        "frac(1,2,gap:#(-x))",
        "frac(1,2,gap:#(-(1)))",
        "op(\"x\",limits:#(true or false))",
    ] {
        let (tex, diagnostics) = convert(source);
        assert!(tex.starts_with(r"\text{[typst math: "), "{source}: {tex}");
        assert_eq!(codes(&diagnostics), ["E-TYP-MATH-CODE"], "{source}");
        let (start, end) = diagnostic_range(&diagnostics[0], source);
        assert!(source[start..end].contains('#'), "{source}: {diagnostics:?}");
    }
    for source in [
        "op(\"x\",limits:#true)", "op(\"x\",limits:#false)",
        "mat(delim:#none,1,2)", "frac(1,2,gap:#3)", "frac(1,2,gap:#3.5)",
        "frac(1,2,gap:#1pt)", "frac(1,2,gap:#(-3))", "frac(1,2,gap:#(-1pt))",
        r"\#", r##""#""##, "alpha /* #not-code */ + beta",
    ] {
        let (_, diagnostics) = convert(source);
        assert!(!codes(&diagnostics).contains(&"E-TYP-MATH-CODE"), "{source}");
        diagnostics.iter().for_each(|diagnostic| {
            diagnostic_range(diagnostic, source);
        });
    }
}

#[test]
fn recovery_diagnostics_keep_names_and_equation_relative_utf8_ranges() {
    let source = "α + foo(x)";
    let (tex, diagnostics) = convert(source);
    assert_eq!(compact(&tex), r"\alpha+\operatorname{foo}(x)");
    assert_eq!(codes(&diagnostics), ["W-TYP-MATH-FN-UNKNOWN"]);
    assert!(diagnostics[0].message.contains("foo"));
    assert_eq!(diagnostic_range(&diagnostics[0], source), (5, 11));
    let source = "α + planck.reduce";
    let (tex, diagnostics) = convert(source);
    assert!(tex.contains(r"\mathit{planck.reduce}"), "{tex}");
    assert_eq!(codes(&diagnostics), ["W-TYP-MATH-SYM-UNKNOWN"]);
    assert_eq!(diagnostic_range(&diagnostics[0], source), (5, 18));
    let (_, diagnostics) = convert("frac(a,b,gap:1pt)");
    assert_eq!(codes(&diagnostics), ["W-TYP-ARG-IGNORED"]);
    assert!(diagnostics[0].message.contains("gap"));
    let (tex, diagnostics) = convert("frac(x)");
    assert_eq!(compact(&tex), r"\operatorname{frac}(x)");
    assert_eq!(codes(&diagnostics), ["W-TYP-MATH-FN-UNKNOWN"]);
    let (tex, diagnostics) = convert("frac(");
    assert!(!tex.is_empty());
    assert!(codes(&diagnostics).contains(&"W-TYP-MATH-SYM-UNKNOWN"));
    assert!(!codes(&diagnostics).contains(&"E-TYP-MATH-CODE"));
    let (tex, diagnostics) = convert("🦀");
    assert!(tex.contains('🦀'));
    assert_eq!(codes(&diagnostics), ["W-TYP-MATH-SYM-UNKNOWN"]);
    diagnostic_range(&diagnostics[0], "🦀");
}

#[test]
fn symbols_control_word_boundaries_and_packages_are_publicly_consistent() {
    assert_eq!(symbol("alpha").unwrap().tex, r"\alpha");
    assert_eq!(char_command('α').unwrap().tex, r"\alpha");
    assert_eq!(symbol("RR").unwrap().tex, r"\mathbb{R}");
    assert!(std::ptr::eq(symbol("RR").unwrap(), symbol("RR").unwrap()));
    for path in ["", ".", "arrow..r", ".alpha", "alpha.", "not-a-symbol"] {
        assert!(symbol(path).is_none(), "{path}");
    }
    assert!(char_command('🦀').is_none());
    for (command, package) in [
        (r"\mathscr", "mathrsfs"),
        (r"\overbracket", "mathtools"),
        (r"\underbracket", "mathtools"),
        (r"\cancel", "cancel"),
        (r"\bcancel", "cancel"),
        (r"\xcancel", "cancel"),
        (r"\bm", "bm"),
        (r"\mathclap", "mathtools"),
        (r"\xmapsto", "mathtools"),
        (r"\Mapsfrom", "stmaryrd"),
        (r"\longmapsfrom", "stmaryrd"),
        (r"\reflectbox", "graphicx"),
    ] {
        assert_eq!(command_package(command), Some(package), "{command}");
        assert_eq!(command_package(&format!("{command}suffix")), None);
    }
    for command in ["mathscr", r"\Mathscr", r"\frac", r"\mathbb", r"\|", ""] {
        assert_eq!(command_package(command), None, "{command}");
    }
    for source in ["→x", "⟶x"] {
        let command = char_command(source.chars().next().unwrap()).unwrap();
        assert_eq!(convert(source).0, format!("{}{{}}x", command.tex));
    }
    assert_eq!(char_command('⟦').unwrap().package, Some("stmaryrd"));
    assert_eq!(char_command('⬳').unwrap().package, Some("graphicx"));
}

#[test]
fn mapped_unicode_shorthands_and_bounded_compositions_stay_deterministic() {
    assert_eq!(ast::MathShorthand::LIST.len(), 38);
    for &(spelling, character) in ast::MathShorthand::LIST {
        let symbol = char_command(character).unwrap_or_else(|| {
            panic!(
                "unmapped shorthand {spelling:?}, U+{:04X}",
                character as u32
            )
        });
        assert!(symbol.tex.is_ascii());
        let first = convert(spelling);
        assert_eq!(first, convert(spelling), "{spelling}");
    }
    assert_eq!(convert("12.30").0, "12.30");
    let styled = "𝟎𝟏𝟐";
    let expected: String = styled
        .chars()
        .map(|character| char_command(character).unwrap().tex)
        .collect();
    let (actual, diagnostics) = convert(styled);
    assert_eq!(actual, expected);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    // amssymb has no double-struck digits: the frozen generator intentionally
    // maps DOUBLE-STRUCK only for ASCII capitals, so retain this with a warning.
    let (unmapped, diagnostics) = convert("𝟘");
    assert_eq!(unmapped, "𝟘");
    assert_eq!(codes(&diagnostics), ["W-TYP-MATH-SYM-UNKNOWN"]);
    diagnostic_range(&diagnostics[0], "𝟘");
    let mut mapped = 0;
    for codepoint in (0x0391..=0x03c9).chain(0x1d400..=0x1d7ff) {
        let Some(character) = char::from_u32(codepoint) else {
            continue;
        };
        let Some(symbol) = char_command(character) else {
            continue;
        };
        let (tex, diagnostics) = convert(&character.to_string());
        assert!(tex.is_ascii(), "U+{codepoint:04X}: {tex}");
        assert!(diagnostics.is_empty(), "U+{codepoint:04X}: {diagnostics:?}");
        assert_eq!(tex, symbol.tex, "U+{codepoint:04X}");
        mapped += 1;
    }
    assert!(mapped > 850, "keep generated Greek/math alphanumeric coverage");
    for atom in ["x", "alpha", "1", "α", "frac(a,b)", "sqrt(x)"] {
        for source in [
            format!("{atom}_1^2"),
            format!("sqrt({atom})"),
            format!("frac({atom},sqrt(x))"),
            format!("lr(({atom}))"),
            format!("\"text\" + {atom}"),
        ] {
            let first = convert(&source);
            assert_eq!(first, convert(&source), "{source}");
            for diagnostic in first.1 {
                diagnostic_range(&diagnostic, &source);
            }
        }
    }
    let nested = format!("{}x{}", "sqrt(".repeat(32), ")".repeat(32));
    assert_eq!(convert(&nested), convert(&nested));
}

#[test]
fn malformed_and_parser_depth_recovery_remains_located_and_deterministic() {
    let mut sources = [
        "sqrt(\"unterminated)", "sqrt(x,\"unterminated)", "x_", "x^",
        "frac(a,)", "mat(a,;b)", "((x", "x))", "α\u{301}", "→🦀",
    ]
    .map(String::from)
    .to_vec();
    sources.push(format!("{}x{}", "sqrt(".repeat(300), ")".repeat(300)));
    for source in sources {
        let converted = convert(&source);
        assert_eq!(converted, convert(&source), "{source}");
        assert!(
            !converted.0.is_empty(),
            "source must remain visible: {source}"
        );
        for diagnostic in &converted.1 {
            diagnostic_range(diagnostic, &source);
            assert_ne!(diagnostic.code, "E-TYP-MATH-CODE", "{source}");
        }
    }
}

/// Exact forms reviewed against actual corpus output and math.md §3.3.
/// Every tuple is consumed by its named fixture; none exempts a whole fixture.
const FORM_DIFFERENCES: &[(&str, &str, &str, &str)] = &[
    ("03-integral-dif", "xdx", r"x\mathrm{d}x", "Typst dif is upright"),
    (
        "07-lr-frac",
        r"\left.\left(\frac{a}{b}\right)\right.",
        r"\left(\frac{a}{b}\right)",
        "Pandoc's invisible lr wrappers",
    ),
    (
        "11-accents",
        r"\overset{\rightarrow}{v}",
        r"\vec{v}",
        "native arrow(v) is the frozen vector accent",
    ),
    (
        "12-op-argmax-limits",
        r"\operatorname{argmax}\limits",
        r"\operatorname*{argmax}",
        "op(limits: true) uses the starred operator name",
    ),
    (
        "12-op-argmax-limits",
        "f(x)",
        r"f\left(x\right)",
        "the native implicit call retains scalable MathDelimited parentheses",
    ),
    (
        "14-lim-oo",
        r"\lim\limits_{x",
        r"\lim_{x",
        "native lim does not force limits",
    ),
    (
        "14-lim-oo",
        "f(x)",
        r"f\left(x\right)",
        "the native implicit call retains scalable MathDelimited parentheses",
    ),
    (
        "17-primes-fcall",
        "f'(x)",
        r"f'\left(x\right)",
        "the primed native call retains scalable MathDelimited parentheses",
    ),
    (
        "22-text-op-lim",
        r"\lim\limits_{x",
        r"\lim_{x",
        "native lim does not force limits",
    ),
    (
        "25-lr-floor-ceil",
        r"\left.\left(\frac{a}{b}\right)\right.",
        r"\left(\frac{a}{b}\right)",
        "Pandoc's invisible lr wrappers",
    ),
    (
        "26-limits-scripts-overbrace",
        r"\max_{x}",
        r"\max\nolimits_{x}",
        "scripts(max) explicitly preserves nolimits",
    ),
    (
        "26-limits-scripts-overbrace",
        r"\operatorname{argmax}\limits",
        r"\operatorname*{argmax}",
        "op(limits: true) uses the starred operator name",
    ),
    (
        "26-limits-scripts-overbrace",
        r"\overset{\text{sum}}{\overbrace{a+b}}",
        r"\overbrace{a+b}^{\text{sum}}",
        "equivalent brace annotation placement",
    ),
];

#[test]
fn oracle_normalization_keeps_commands_delimiters_and_control_word_text() {
    assert_eq!(oracle_normalize(r" \frac{a}{ b } "), r"\fracab");
    assert_ne!(oracle_normalize(r"\lim\limits_x"), oracle_normalize(r"\lim_x"));
    assert_ne!(oracle_normalize(r"\left(x\right)"), oracle_normalize("(x)"));
    assert_ne!(oracle_normalize(r"\mathit{foo}"), oracle_normalize("foo"));
}

#[test]
fn recorded_pandoc_forms_agree_without_broad_or_fixture_level_exemptions() {
    let mut mismatches = Vec::new();
    let mut used_forms = BTreeSet::new();
    for fixture in fixtures().into_iter().filter(|fixture| {
        let number: usize = fixture.name[..2].parse().unwrap();
        number <= 27 && number != 18
    }) {
        let recorded =
            fs::read_to_string(fixture.path.with_extension("pandoc.tex"))
                .unwrap();
        let (tex, _) = convert(&fixture.body);
        let expected = oracle_normalize(&recorded);
        let mut actual = oracle_normalize(&tex);
        for (index, &(name, pandoc, writer, reason)) in
            FORM_DIFFERENCES.iter().enumerate()
        {
            if fixture.name != name {
                continue;
            }
            assert!(!reason.is_empty());
            let pandoc = oracle_normalize(pandoc);
            let writer = oracle_normalize(writer);
            if expected.contains(pandoc.as_str())
                && actual.contains(writer.as_str())
                && !actual.contains(pandoc.as_str())
            {
                assert_eq!(
                    actual.matches(writer.as_str()).count(),
                    1,
                    "{name}: {reason}"
                );
                actual = actual.replacen(writer.as_str(), pandoc.as_str(), 1);
                used_forms.insert(index);
            }
        }
        if actual != expected {
            mismatches.push(format!(
                "{}: Typst {:?}\nWriter: {tex}\nPandoc: {recorded}\nNormalized Writer: {actual}\nNormalized Pandoc: {expected}",
                fixture.name, fixture.body
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n\n"));
    let unused: Vec<_> = FORM_DIFFERENCES
        .iter()
        .enumerate()
        .filter(|(index, _)| !used_forms.contains(index))
        .map(|(_, entry)| entry)
        .collect();
    assert!(
        unused.is_empty(),
        "remove obsolete oracle form exceptions: {unused:?}"
    );
}

fn pandoc_body(output: &str, display: bool) -> String {
    let (open, close) = if display {
        (r"\[", r"\]")
    } else {
        (r"\(", r"\)")
    };
    let trimmed = output.trim();
    let body = trimmed
        .strip_prefix(open)
        .and_then(|body| body.strip_suffix(close))
        .unwrap_or_else(|| panic!("unexpected Pandoc delimiters: {output:?}"));
    format!("{}\n", body.trim_matches('\n'))
}

#[test]
fn installed_pandoc_reproduces_every_recorded_oracle_and_reader_failure() {
    let version = match Command::new("pandoc").arg("--version").output() {
        Ok(output) => {
            assert!(output.status.success(), "pandoc --version failed");
            String::from_utf8(output.stdout).unwrap()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "skipping optional Pandoc regeneration: pandoc not on PATH"
            );
            return;
        }
        Err(error) => panic!("pandoc --version: {error}"),
    };
    for fixture in fixtures() {
        let wrapped = if fixture.display {
            format!("$ {} $\n", fixture.body)
        } else {
            format!("${}$\n", fixture.body)
        };
        let mut command = assert_cmd::Command::new("pandoc");
        let output = command
            .args(["-f", "typst", "-t", "latex", "--wrap=none"])
            .timeout(std::time::Duration::from_secs(10))
            .write_stdin(wrapped)
            .output()
            .unwrap();
        let path = fixture.path.with_extension("pandoc.tex");
        if ["18-abs-norm-lr-mismatch", "29-unknown-fn", "30-unknown-sym"]
            .contains(&fixture.name.as_str())
        {
            assert!(!path.exists(), "{}: no invented oracle", fixture.name);
            assert!(
                !output.status.success(),
                "{}: reader behavior changed",
                fixture.name
            );
            assert!(!output.stderr.is_empty());
        } else {
            assert!(
                output.status.success(),
                "{}: {}",
                fixture.name,
                String::from_utf8_lossy(&output.stderr)
            );
            let body = pandoc_body(
                &String::from_utf8(output.stdout).unwrap(),
                fixture.display,
            );
            assert_eq!(
                body,
                fs::read_to_string(path).unwrap(),
                "{}: {version}",
                fixture.name
            );
        }
    }
}

#[test]
fn recorded_tylax_evidence_keeps_its_known_differences_and_exact_inventory() {
    let actual: BTreeSet<_> = fs::read_dir(fixture_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tylax.tex"))
        .collect();
    let expected: BTreeSet<_> = fixtures()
        .into_iter()
        .filter(|fixture| {
            (17..=24).contains(&fixture.name[..2].parse::<usize>().unwrap())
        })
        .map(|fixture| format!("{}.tylax.tex", fixture.name))
        .collect();
    assert_eq!(actual, expected);
    assert!(
        fs::read_to_string(fixture_dir().join("20-vec-binom.tylax.tex"))
            .unwrap()
            .contains(r"\vec{1}{2}{3}")
    );
    assert!(
        fs::read_to_string(fixture_dir().join("21-styles.tylax.tex"))
            .unwrap()
            .contains(r"\widehat{x}")
    );
    assert!(
        fs::read_to_string(fixture_dir().join("24-alignpoints.tylax.tex"))
            .unwrap()
            .starts_with(r"\begin{align}")
    );
}
