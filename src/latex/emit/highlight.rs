//! Stateful code tokens for writercode and the frozen listings language map.

use std::sync::LazyLock;

use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference};
use syntect::util::LinesWithEndings;

use crate::html::highlight::{SYNTAXES, SyntaxChoice, resolve_syntax};
use crate::latex::escape;

/// Highlights a normalized body; the environment caller owns its final newline.
/// Plain/unknown languages and any parsing failure request verbatim fallback.
pub(super) fn highlighted(
    text: &str, lang: Option<&str>, plain: bool,
) -> Option<String> {
    let SyntaxChoice::Syntax(syntax) = resolve_syntax(lang, plain) else {
        return None;
    };
    let normalized = text.replace("\r\n", "\n");
    let body = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let lines = token_lines(syntax, body)?;
    Some(
        lines
            .iter()
            .map(|line| render_line(line))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Returns the exact listings option value, including braces around dialects.
pub(super) fn listings_language(lang: &str) -> Option<&'static str> {
    Some(match lang.to_ascii_lowercase().as_str() {
        "python" | "py" => "Python",
        "c" => "C",
        "cpp" | "c++" | "cxx" => "C++",
        "csharp" | "cs" => "{[Sharp]C}",
        "java" => "Java",
        "go" => "Go",
        "r" => "R",
        "bash" | "sh" | "shell" | "zsh" => "bash",
        "haskell" => "Haskell",
        "lua" => "{[5.3]Lua}",
        "ruby" => "Ruby",
        "php" => "PHP",
        "perl" => "Perl",
        "sql" => "SQL",
        "xml" => "XML",
        "html" => "HTML",
        "matlab" => "Matlab",
        "octave" => "Octave",
        "fortran" => "Fortran",
        "pascal" => "Pascal",
        "delphi" => "Delphi",
        "ada" => "Ada",
        "lisp" => "Lisp",
        "prolog" => "Prolog",
        "erlang" => "erlang",
        "ocaml" => "{[Objective]Caml}",
        "scala" => "Scala",
        "swift" => "Swift",
        "tex" | "latex" => "TeX",
        "make" | "makefile" => "make",
        "awk" => "Awk",
        "tcl" => "tcl",
        "vhdl" => "VHDL",
        "verilog" => "Verilog",
        _ => return None,
    })
}

struct Class {
    name: &'static str,
    scopes: Vec<Scope>,
}

// Table order decides among matching prefixes of one scope. Stack order is
// considered first, so a recognized inner scope wins over every outer class.
static CLASSES: LazyLock<Vec<Class>> = LazyLock::new(|| {
    [
        ("Comment", &["comment"][..]),
        ("String", &["string"]),
        ("Number", &["constant.numeric"]),
        ("Constant", &["constant"]),
        ("Operator", &["keyword.operator"]),
        (
            "Preproc",
            &[
                "meta.preprocessor", "keyword.control.import",
                "keyword.control.directive",
            ],
        ),
        (
            "Attribute",
            &[
                "entity.other.attribute-name", "variable.annotation",
                "meta.annotation",
            ],
        ),
        ("Keyword", &["keyword", "storage.modifier"]),
        (
            "Type",
            &[
                "storage.type", "entity.name.type", "entity.name.class",
                "entity.name.struct", "entity.name.enum",
                "entity.other.inherited-class", "support.type",
                "support.class",
            ],
        ),
        (
            "Function",
            &["entity.name.function", "support.function", "support.macro"],
        ),
        ("Variable", &["variable"]),
        ("Error", &["invalid"]),
    ]
    .into_iter()
    .map(|(name, prefixes)| Class {
        name,
        scopes: prefixes
            .iter()
            .map(|prefix| {
                Scope::new(prefix).expect("frozen highlighting scope is valid")
            })
            .collect(),
    })
    .collect()
});

type Token = (Option<&'static str>, String);

fn token_lines(
    syntax: &SyntaxReference, text: &str,
) -> Option<Vec<Vec<Token>>> {
    let mut parser = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(&format!("{text}\n")) {
        let operations = parser.parse_line(line, &SYNTAXES).ok()?;
        let mut tokens = Vec::new();
        for (region, operation) in ScopeRegionIterator::new(&operations, line) {
            stack.apply(operation).ok()?;
            // Newline-aware grammars still observe the LF and its scope ops.
            // The LF is never part of a macro argument or styled token.
            let region = region.strip_suffix('\n').unwrap_or(region);
            if !region.is_empty() {
                push_token(&mut tokens, token_class(&stack), region);
            }
        }
        lines.push(tokens);
    }
    Some(lines)
}

fn token_class(stack: &ScopeStack) -> Option<&'static str> {
    stack.as_slice().iter().rev().find_map(|scope| {
        CLASSES.iter().find_map(|class| {
            class
                .scopes
                .iter()
                .any(|prefix| prefix.is_prefix_of(*scope))
                .then_some(class.name)
        })
    })
}

fn push_token(
    tokens: &mut Vec<Token>, class: Option<&'static str>, text: &str,
) {
    let class = if text.chars().all(char::is_whitespace) {
        None
    } else {
        class
    };
    if let Some((previous, value)) = tokens.last_mut()
        && *previous == class
    {
        value.push_str(text);
    } else {
        tokens.push((class, text.into()));
    }
}

fn render_line(tokens: &[Token]) -> String {
    let mut output = String::new();
    for (class, text) in tokens {
        let escaped = escape::writercode(text);
        if let Some(class) = class {
            output.push_str(&format!("\\Wr{class}{{{escaped}}}"));
        } else {
            output.push_str(&escaped);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntect::highlighting::{FontStyle, Highlighter};
    use two_face::theme::EmbeddedThemeName;

    // Preamble defaults from §6, kept separate from production token classes.
    // Theme recomputation below independently detects drift in the pinned data.
    const STYLES: &[(&str, &str, &str, bool, bool)] = &[
        ("Comment", "comment", "969896", false, true),
        ("String", "string", "183691", false, false),
        ("Number", "constant.numeric", "0086B3", false, false),
        ("Constant", "constant.language", "0086B3", false, false),
        ("Operator", "keyword.operator", "A71D5D", true, false),
        ("Preproc", "meta.preprocessor", "323232", false, false),
        ("Attribute", "entity.other.attribute-name", "795DA3", false, false),
        ("Keyword", "keyword", "A71D5D", true, false),
        ("Type", "storage.type", "A71D5D", true, false),
        ("Function", "entity.name.function", "795DA3", true, false),
        ("Variable", "variable", "323232", false, false),
        ("Error", "invalid", "B52A1D", true, false),
    ];

    fn stack(scopes: &[&str]) -> ScopeStack {
        ScopeStack::from_vec(
            scopes.iter().map(|name| Scope::new(name).unwrap()).collect(),
        )
    }

    fn syntax(lang: &str) -> &'static SyntaxReference {
        let SyntaxChoice::Syntax(syntax) = resolve_syntax(Some(lang), false)
        else {
            panic!("embedded {lang} grammar");
        };
        syntax
    }

    fn source(lines: &[Vec<Token>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.iter().map(|(_, text)| text.as_str()).collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn closed_arguments(line: &str) {
        let mut depth = 0;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\'
                && chars.peek().is_some_and(|&c| matches!(c, '{' | '}'))
            {
                chars.next();
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                assert!(depth > 0, "unmatched closing brace: {line}");
                depth -= 1;
            }
        }
        assert_eq!(depth, 0, "a token argument crossed a line: {line}");
    }

    #[test]
    fn scope_order_prefers_the_inner_scope_then_the_fixed_prefix_order() {
        for (scopes, expected) in [
            (vec!["source.python", "comment.line.python"], Some("Comment")),
            (vec!["comment.block", "constant.numeric.integer"], Some("Number")),
            (
                vec!["string.quoted", "constant.character.escape"],
                Some("Constant"),
            ),
            (vec!["string.quoted", "meta.unrecognized"], Some("String")),
            (vec!["keyword.control.import.python"], Some("Preproc")),
            (vec!["keyword.operator.assignment"], Some("Operator")),
            (vec!["variable.annotation.python"], Some("Attribute")),
            (vec!["storage.modifier"], Some("Keyword")),
            (vec!["storage.type.rust"], Some("Type")),
            (vec!["support.macro.rust"], Some("Function")),
            (vec!["constant.numerical"], Some("Constant")),
            (vec!["keyword.operatorish"], Some("Keyword")),
            (vec!["stringy.quoted"], None),
            (vec!["comments.line"], None),
            (vec!["entity.name.functionality"], None),
            (vec!["source.rust"], None),
            (Vec::new(), None),
        ] {
            assert_eq!(token_class(&stack(&scopes)), expected, "{scopes:?}");
        }
        assert_eq!(CLASSES.len(), 12);
        for class in CLASSES.iter() {
            for prefix in &class.scopes {
                assert_eq!(
                    token_class(&ScopeStack::from_vec(vec![*prefix])),
                    Some(class.name)
                );
            }
        }
    }

    #[test]
    fn adjacent_classes_merge_but_whitespace_and_plain_tokens_stay_bare() {
        let mut tokens = Vec::new();
        push_token(&mut tokens, Some("String"), "a");
        push_token(&mut tokens, Some("String"), "b");
        push_token(&mut tokens, Some("String"), " ");
        push_token(&mut tokens, None, "\t");
        push_token(&mut tokens, Some("String"), "c");
        push_token(&mut tokens, None, " +");
        push_token(&mut tokens, None, " d");
        assert_eq!(
            tokens,
            [
                (Some("String"), "ab".into()),
                (None, " \t".into()),
                (Some("String"), "c".into()),
                (None, " + d".into()),
            ]
        );
        assert_eq!(render_line(&tokens), "\\WrString{ab} \t\\WrString{c} + d");
        assert_eq!(
            render_line(&[(Some("String"), "\\{x}%#_$^~".into())]),
            r"\WrString{\textbackslash{}\{x\}%#_$^~}",
        );
    }

    #[test]
    fn multiline_lexing_preserves_comment_and_string_state_and_source_bytes() {
        for (lang, text, expected) in [
            ("c", "/* first\n\n continued */\nint value;", "Comment"),
            (
                "python",
                "value = \"\"\"first\n\ncontinued\"\"\"\nreturn 1",
                "String",
            ),
        ] {
            let lines = token_lines(syntax(lang), text).unwrap();
            assert_eq!(source(&lines), text);
            assert!(lines[1].is_empty());
            assert!(
                lines[2].iter().any(|(class, text)| {
                    *class == Some(expected) && text.contains("continued")
                }),
                "{lang}: {:?}",
                lines[2]
            );
            for line in &lines {
                assert!(line.iter().all(|(_, text)| !text.contains('\n')));
                closed_arguments(&render_line(line));
            }
        }
    }

    #[test]
    fn newline_terminated_comments_cannot_swallow_the_next_rendered_line() {
        let text = highlighted("# c\ny = 2", Some("python"), false).unwrap();
        assert_eq!(text.lines().next(), Some(r"\WrComment{# c}"));
        assert!(text.starts_with("\\WrComment{# c}\n"));
        assert!(text.contains(r"\WrNumber{2}"));
        assert!(!text.contains("{# c\n"));
        assert!(!text.contains(r"\textcolor"));
        for line in text.lines() {
            closed_arguments(line);
        }
    }

    #[test]
    fn normalization_removes_only_one_final_lf_and_keeps_remaining_blank_lines()
    {
        assert_eq!(highlighted("", Some("python"), false), Some(String::new()));
        assert_eq!(
            highlighted("\n", Some("python"), false),
            Some(String::new())
        );
        assert_eq!(
            highlighted("\n\n", Some("python"), false),
            Some("\n".into())
        );
        let without = highlighted("# c\ny = 2", Some("python"), false).unwrap();
        assert_eq!(
            highlighted("# c\r\ny = 2\r\n", Some("python"), false),
            Some(without.clone())
        );
        assert_eq!(
            highlighted("# c\ny = 2\n\n", Some("python"), false),
            Some(format!("{without}\n"))
        );
        assert!(
            source(&token_lines(syntax("python"), "# c\r").unwrap())
                .ends_with('\r')
        );
    }

    #[test]
    fn shared_aliases_and_plain_languages_preserve_the_fallback_contract() {
        for (alias, canonical) in
            [("shell", "bash"), ("jsx", "js"), ("ipynb", "json")]
        {
            assert_eq!(
                highlighted("value = 1", Some(alias), false),
                highlighted("value = 1", Some(canonical), false)
            );
        }
        for lang in [
            None,
            Some("text"),
            Some("plain"),
            Some("ps1"),
            Some("powershell"),
            Some("unregistered-lang"),
        ] {
            assert_eq!(highlighted("source", lang, false), None);
        }
        assert_eq!(highlighted("source", Some("python"), true), None);
    }

    #[test]
    fn fixed_palette_matches_the_pinned_inspired_github_theme() {
        let themes = two_face::theme::extra();
        let theme = themes.get(EmbeddedThemeName::InspiredGithub);
        let highlighter = Highlighter::new(theme);
        assert_eq!(STYLES.len(), 12);
        for ((name, representative, color, bold, italic), class) in
            STYLES.iter().zip(CLASSES.iter())
        {
            assert_eq!(*name, class.name);
            let style = highlighter
                .style_for_stack(&[Scope::new(representative).unwrap()]);
            assert_eq!(
                format!(
                    "{:02X}{:02X}{:02X}",
                    style.foreground.r, style.foreground.g, style.foreground.b
                ),
                *color,
                "{name}"
            );
            assert_eq!(
                style.font_style.contains(FontStyle::BOLD),
                *bold,
                "{name}"
            );
            assert_eq!(
                style.font_style.contains(FontStyle::ITALIC),
                *italic,
                "{name}"
            );
        }
    }

    #[test]
    fn listings_aliases_match_all_34_frozen_values_and_brace_the_dialects() {
        const SPEC: &str = include_str!("../../../docs/spec/latex-output.md");
        let body = SPEC
            .split_once("### 4.7")
            .unwrap()
            .1
            .split_once("```")
            .unwrap()
            .1
            .split_once("```")
            .unwrap()
            .0;
        let mut values = std::collections::BTreeSet::new();
        let mut aliases = Vec::new();
        for line in body.lines() {
            for entry in line
                .split("  ")
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
            {
                let (names, expected) = entry.split_once('→').unwrap();
                values.insert(expected);
                for alias in names.split_whitespace() {
                    aliases.push(alias);
                    assert_eq!(
                        listings_language(alias),
                        Some(expected),
                        "{alias}"
                    );
                    assert_eq!(
                        listings_language(&alias.to_ascii_uppercase()),
                        Some(expected)
                    );
                }
            }
        }
        assert_eq!(values.len(), 34);
        assert_eq!(
            aliases.len(),
            aliases.iter().collect::<std::collections::BTreeSet<_>>().len()
        );
        assert_eq!(listings_language("CS"), Some("{[Sharp]C}"));
        assert_eq!(listings_language("Lua"), Some("{[5.3]Lua}"));
        assert_eq!(listings_language("ocaml"), Some("{[Objective]Caml}"));
        for language in [
            "", "rust", "rs", "js", "javascript", "typescript", "json", "yaml",
            "toml", "julia", " python", "python ",
        ] {
            assert_eq!(listings_language(language), None, "{language}");
        }
    }
}
