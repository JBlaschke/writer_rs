//! Class-based HTML syntax highlighting (html-output.md §1.5).
//! Owner: T13. Module skeleton created by T00.

use std::sync::LazyLock;

use syntect::highlighting::Theme;
use syntect::html::{
    ClassStyle, css_for_theme_with_class_style, line_tokens_to_classed_spans,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use two_face::theme::EmbeddedLazyThemeSet;

use crate::markdown::code::CodeAttrs;

/// Shared two-face syntaxes, including the newline-aware grammars.
pub static SYNTAXES: LazyLock<SyntaxSet> =
    LazyLock::new(two_face::syntax::extra_newlines);

const STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" };
static THEMES: LazyLock<EmbeddedLazyThemeSet> =
    LazyLock::new(two_face::theme::extra);

/// Whether a code block is plain, has an unknown language, or has a grammar.
pub enum SyntaxChoice {
    /// Deliberately render escaped source without highlighting.
    Plain,
    /// No grammar matches the requested language.
    Unknown,
    /// A shared newline-aware syntax definition.
    Syntax(&'static SyntaxReference),
}

/// Resolves a language using the aliases and plain-language rules shared by
/// the HTML, DOCX and LaTeX emitters.
pub fn resolve_syntax(lang: Option<&str>, plain: bool) -> SyntaxChoice {
    let Some(lang) = lang.filter(|_| !plain) else {
        return SyntaxChoice::Plain;
    };
    if matches!(
        lang,
        "text" | "plain" | "txt" | "none" | "output" | "ps1" | "powershell"
    ) {
        return SyntaxChoice::Plain;
    }
    let lang = match lang {
        "console" | "shell" => "bash",
        "jsx" => "js",
        "fortran" => "f90",
        "ipynb" => "json",
        other => other,
    };
    match SYNTAXES.find_syntax_by_token(lang) {
        Some(syntax) => SyntaxChoice::Syntax(syntax),
        None => SyntaxChoice::Unknown,
    }
}

/// Highlights newline-terminated source into one HTML string per line.
/// Every span closes on its own line; scopes continuing across a line break
/// reopen on the next line. An empty line is an empty string. Any parser or
/// span-conversion error returns `None` for the entire source.
pub fn highlight_lines(
    syntax: &SyntaxReference, text: &str,
) -> Option<Vec<String>> {
    let (mut ps, mut stack, mut out) =
        (ParseState::new(syntax), ScopeStack::new(), Vec::new());
    for line in LinesWithEndings::from(text) {
        let ops = ps.parse_line(line, &SYNTAXES).ok()?;
        let body = line.strip_suffix('\n').unwrap_or(line);
        let ops: Vec<_> = ops
            .into_iter()
            .map(|(index, operation)| (index.min(body.len()), operation))
            .collect();
        let mut html = String::new();
        open_tags(&stack, &mut html);
        let (spans, _) =
            line_tokens_to_classed_spans(body, &ops, STYLE, &mut stack).ok()?;
        html.push_str(&spans);
        for _ in
            0..(html.matches("<span").count() - html.matches("</span>").count())
        {
            html.push_str("</span>");
        }
        out.push(if body.is_empty() { String::new() } else { html });
    }
    Some(out)
}

fn open_tags(stack: &ScopeStack, out: &mut String) {
    for scope in stack.as_slice() {
        let classes: Vec<String> = scope
            .build_string()
            .split('.')
            .map(|part| format!("hl-{part}"))
            .collect();
        out.push_str("<span class=\"");
        out.push_str(&classes.join(" "));
        out.push_str("\">");
    }
}

/// Looks up a two-face theme by its exact, case-sensitive embedded name.
/// The terminal themes `ansi`, `base16` and `base16-256` are unsuitable for HTML.
pub fn builtin_code_theme(name: &str) -> Option<&'static Theme> {
    if matches!(name, "ansi" | "base16" | "base16-256") {
        return None;
    }
    EmbeddedLazyThemeSet::theme_names()
        .iter()
        .find(|theme| theme.as_name() == name)
        .map(|name| THEMES.get(*name))
}

/// Generates the `.hl-code` root rule and `hl-` scope rules for a theme.
pub fn code_theme_css(theme: &Theme) -> String {
    // Syntect only appends to a String here; its implementation always returns Ok.
    css_for_theme_with_class_style(theme, STYLE)
        .expect("syntect CSS generation only writes to a String")
}

/// Builds the line wrappers shared by ordinary code blocks and cell inputs.
pub(crate) fn code_lines(
    choice: &SyntaxChoice, text: &str, code: &CodeAttrs,
) -> String {
    let normalized = text.replace("\r\n", "\n");
    let normalized = normalized.strip_suffix('\n').unwrap_or(&normalized);
    if normalized.is_empty() {
        return String::new();
    }
    let highlighted = match choice {
        SyntaxChoice::Syntax(syntax) => {
            highlight_lines(syntax, &format!("{normalized}\n"))
        }
        _ => None,
    };
    let lines = highlighted.unwrap_or_else(|| {
        normalized.split('\n').map(super::emit::escape).collect()
    });
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let class = if code
                .is_highlighted(code.start.saturating_add(index as u64))
            {
                "line highlight"
            } else {
                "line"
            };
            format!("<span class=\"{class}\">{line}</span>")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Attrs;

    fn code(pairs: &[(&str, &str)]) -> CodeAttrs {
        CodeAttrs::from_attrs(&Attrs {
            pairs: pairs
                .iter()
                .map(|&(key, value)| (key.into(), value.into()))
                .collect(),
            ..Attrs::default()
        })
    }

    #[test]
    fn python_docstring_reopens_spans() {
        let SyntaxChoice::Syntax(python) =
            resolve_syntax(Some("python"), false)
        else {
            panic!("Python syntax must exist");
        };
        let lines = highlight_lines(
            python,
            "def f():\n    \"\"\"Doc\n\n    string\"\"\"\n",
        )
        .unwrap();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[2], "");
        assert_eq!(
            lines[3],
            "<span class=\"hl-source hl-python\"><span class=\"hl-comment hl-block hl-documentation hl-python\">    string<span class=\"hl-punctuation hl-definition hl-comment hl-end hl-python\">&quot;&quot;&quot;</span></span></span>"
        );
        for line in &lines {
            assert_eq!(
                line.matches("<span").count(),
                line.matches("</span>").count()
            );
        }
        assert_eq!(highlight_lines(python, "\n").unwrap(), [""]);
    }

    #[test]
    fn numbered_start_highlight() {
        let html = code_lines(
            &SyntaxChoice::Plain,
            "a\r\nb\r\nc\r\n",
            &code(&[("start", "5"), ("highlight", "6-7")]),
        );
        assert_eq!(
            html,
            "<span class=\"line\">a</span>\n<span class=\"line highlight\">b</span>\n<span class=\"line highlight\">c</span>"
        );
        assert_eq!(
            code_lines(&SyntaxChoice::Unknown, "<&'\"\n", &code(&[])),
            "<span class=\"line\">&lt;&amp;&#39;&quot;</span>"
        );
    }

    #[test]
    fn unknown_language() {
        assert!(matches!(
            resolve_syntax(Some("nosuchlang"), false),
            SyntaxChoice::Unknown
        ));
    }

    #[test]
    fn plain_class_and_plain_languages() {
        assert!(matches!(
            resolve_syntax(Some("python"), true),
            SyntaxChoice::Plain
        ));
        assert!(matches!(resolve_syntax(None, false), SyntaxChoice::Plain));
        for token in
            ["text", "plain", "txt", "none", "output", "ps1", "powershell"]
        {
            assert!(
                matches!(
                    resolve_syntax(Some(token), false),
                    SyntaxChoice::Plain
                ),
                "{token}"
            );
        }
    }

    #[test]
    fn aliases() {
        for (alias, name) in [
            ("console", "Bourne Again Shell (bash)"),
            ("shell", "Bourne Again Shell (bash)"),
            ("jsx", "JavaScript"),
            ("fortran", "Fortran (Modern)"),
            ("ipynb", "JSON"),
            ("typst", "Typst"),
            ("PYTHON", "Python"),
        ] {
            let SyntaxChoice::Syntax(syntax) =
                resolve_syntax(Some(alias), false)
            else {
                panic!("missing syntax for {alias}");
            };
            assert_eq!(syntax.name, name);
        }
    }

    #[test]
    fn empty_text_no_lines() {
        for choice in [
            SyntaxChoice::Plain,
            SyntaxChoice::Unknown,
            resolve_syntax(Some("python"), false),
        ] {
            for text in ["", "\n", "\r\n"] {
                assert_eq!(code_lines(&choice, text, &code(&[])), "");
            }
        }
        assert_eq!(
            code_lines(&SyntaxChoice::Plain, "a\n\n", &code(&[])),
            "<span class=\"line\">a</span>\n<span class=\"line\"></span>"
        );
    }

    #[test]
    fn builtin_themes() {
        for (name, bytes) in
            [("InspiredGitHub", 15_772), ("OneHalfDark", 3_334)]
        {
            let css = code_theme_css(builtin_code_theme(name).unwrap());
            assert_eq!(css.len(), bytes);
            assert!(css.starts_with("/*") && css.contains(".hl-code {"));
        }
        for name in
            ["ansi", "base16", "base16-256", "inspiredgithub", "missing"]
        {
            assert!(builtin_code_theme(name).is_none(), "{name}");
        }
    }
}
