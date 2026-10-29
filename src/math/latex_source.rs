//! Shared LaTeX equation preparation for every non-LaTeX output target.

use std::collections::BTreeMap;

/// Remove complete equation tags and labels outside brace groups.
///
/// Handles `\label`, `\tag`, `\tag*`, `\nonumber` and `\notag`. Escaped
/// commands, comments, nested groups and incomplete tag arguments stay intact.
/// The flag reports whether any command was removed; surrounding whitespace
/// and all other source bytes are preserved.
pub fn strip_tags(src: &str) -> (String, bool) {
    let mut output = String::with_capacity(src.len());
    let mut copied = 0;
    let mut at = 0;
    let mut depth: usize = 0;
    let bytes = src.as_bytes();
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => {
                let end = command_end(src, at);
                let name = &src[at + 1..end];
                let removed_end = if depth == 0 {
                    tag_end(src, name, end)
                } else {
                    None
                };
                if let Some(removed_end) = removed_end {
                    output.push_str(&src[copied..at]);
                    at = removed_end;
                    copied = at;
                } else {
                    at = end;
                }
            }
            b'%' => at = comment_end(src, at),
            b'{' => {
                depth += 1;
                at += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                at += 1;
            }
            _ => at += 1,
        }
    }
    output.push_str(&src[copied..]);
    (output, copied != 0)
}

fn tag_end(src: &str, name: &str, end: usize) -> Option<usize> {
    match name {
        "nonumber" | "notag" => Some(end),
        "label" => group_argument(src, end).map(|(_, end)| end),
        "tag" => {
            let end = end + usize::from(src.as_bytes().get(end) == Some(&b'*'));
            group_argument(src, end).map(|(_, end)| end)
        }
        _ => None,
    }
}

/// Normalise matched, outermost LaTeX display environments.
///
/// `align` and `eqnarray` become `aligned`; `gather` and `multline` become
/// `gathered`; `equation` loses its wrapper. Their starred forms behave the
/// same way. Nested environments, brace groups and unmatched wrappers retain
/// their original bytes.
pub fn normalise_env(src: &str) -> String {
    let mut environments: Vec<Environment<'_>> = Vec::new();
    let mut edits = Vec::new();
    let mut at = 0;
    let mut depth: usize = 0;
    let bytes = src.as_bytes();
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => {
                let end = command_end(src, at);
                let name = &src[at + 1..end];
                if depth == 0
                    && matches!(name, "begin" | "end")
                    && let Some((start, group_end)) = group_argument(src, end)
                {
                    let environment = &src[start + 1..group_end - 1];
                    if name == "begin" {
                        let replacement = if environments.is_empty() {
                            normalised_environment(environment)
                        } else {
                            None
                        };
                        environments.push(Environment {
                            name: environment,
                            start: at,
                            end: group_end,
                            replacement,
                        });
                    } else if environments
                        .last()
                        .is_some_and(|open| open.name == environment)
                    {
                        let open = environments.pop();
                        if let Some(Environment {
                            start,
                            end,
                            replacement: Some((begin, finish)),
                            ..
                        }) = open
                        {
                            edits.push((start..end, begin));
                            edits.push((at..group_end, finish));
                        }
                    }
                    at = group_end;
                } else {
                    at = end;
                }
            }
            b'%' => at = comment_end(src, at),
            b'{' => {
                depth += 1;
                at += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                at += 1;
            }
            _ => at += 1,
        }
    }
    edits.sort_unstable_by_key(|(range, _)| range.start);
    let mut output = String::with_capacity(src.len());
    let mut copied = 0;
    for (range, replacement) in edits {
        output.push_str(&src[copied..range.start]);
        output.push_str(replacement);
        copied = range.end;
    }
    output.push_str(&src[copied..]);
    output
}

struct Environment<'a> {
    name: &'a str,
    start: usize,
    end: usize,
    replacement: Option<(&'static str, &'static str)>,
}

fn normalised_environment(name: &str) -> Option<(&'static str, &'static str)> {
    match name.strip_suffix('*').unwrap_or(name) {
        "align" | "eqnarray" => Some((r"\begin{aligned}", r"\end{aligned}")),
        "gather" | "multline" => Some((r"\begin{gathered}", r"\end{gathered}")),
        "equation" => Some(("", "")),
        _ => None,
    }
}

/// Strip tags, then normalise environments for non-LaTeX conversion.
///
/// The flag reports whether equation tags or labels were removed.
pub fn prepare(src: &str) -> (String, bool) {
    let (source, removed) = strip_tags(src);
    (normalise_env(&source), removed)
}

/// Extract string entries from the `macros` member of KaTeX options.
///
/// Names, including their leading backslashes, and replacements are preserved
/// verbatim. Other value types are ignored; the result uses lexical key order.
pub fn macros_from_config(
    opts: &serde_json::Map<String, serde_json::Value>,
) -> BTreeMap<String, String> {
    opts.get("macros")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(name, value)| {
            value.as_str().map(|body| (name.clone(), body.to_owned()))
        })
        .collect()
}

/// Return the highest literal `#1` through `#9` in a macro replacement.
///
/// A replacement with no argument markers has arity zero.
pub fn macro_arity(replacement: &str) -> u8 {
    replacement
        .as_bytes()
        .windows(2)
        .filter(|pair| pair[0] == b'#' && matches!(pair[1], b'1'..=b'9'))
        .map(|pair| pair[1] - b'0')
        .max()
        .unwrap_or(0)
}

/// Emit one `\newcommand` line per macro, in lexical key order.
///
/// The optional argument count is the replacement's highest `#1` through `#9`
/// and is omitted for arity zero. Each line ends with LF; names and bodies are
/// copied verbatim. Validation belongs to the consuming conversion route.
pub fn macros_as_newcommands(macros: &BTreeMap<String, String>) -> String {
    let mut output = String::new();
    for (name, replacement) in macros {
        output.push_str(r"\newcommand{");
        output.push_str(name);
        output.push('}');
        let arity = macro_arity(replacement);
        if arity != 0 {
            output.push('[');
            output.push(char::from(b'0' + arity));
            output.push(']');
        }
        output.push('{');
        output.push_str(replacement);
        output.push_str("}\n");
    }
    output
}

/// Prefix prepared source with macro definitions for mitex and Typst.
///
/// The flag reports whether equation tags or labels were removed.
pub fn for_typst(
    src: &str, macros: &BTreeMap<String, String>,
) -> (String, bool) {
    let (source, removed) = prepare(src);
    let mut output = macros_as_newcommands(macros);
    output.push_str(&source);
    (output, removed)
}

/// Checks package-scoped LaTeX syntax without creating a compiler session.
pub(crate) fn preflight_typst(prepared: &str) -> Result<(), String> {
    mitex::convert_math(prepared, None).map(|_| ()).map_err(|message| {
        message.trim().trim_start_matches("error: ").to_owned()
    })
}

fn command_end(src: &str, start: usize) -> usize {
    let bytes = src.as_bytes();
    let mut end = start + 1;
    if bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
        while bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
            end += 1;
        }
    } else if let Some(character) = src[end..].chars().next() {
        end += character.len_utf8();
    }
    end
}

fn comment_end(src: &str, start: usize) -> usize {
    src[start..].find(['\r', '\n']).map_or(src.len(), |end| start + end)
}

fn group_argument(src: &str, mut at: usize) -> Option<(usize, usize)> {
    let bytes = src.as_bytes();
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'%') {
            break;
        }
        at = comment_end(src, at);
    }
    if bytes.get(at) != Some(&b'{') {
        return None;
    }
    let start = at;
    let mut depth = 1;
    at += 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at = command_end(src, at),
            b'%' => at = comment_end(src, at),
            b'{' => {
                depth += 1;
                at += 1;
            }
            b'}' => {
                depth -= 1;
                at += 1;
                if depth == 0 {
                    return Some((start, at));
                }
            }
            _ => at += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_every_tag_kind_without_touching_surrounding_source() {
        let source =
            "x = α \\label{eq:{a}} \\tag* {A} \\tag{2} \\nonumber \\notag\n";
        assert_eq!(strip_tags(source), ("x = α     \n".into(), true));
        assert_eq!(strip_tags(r"\label{x}"), (String::new(), true));
    }

    #[test]
    fn preserves_nested_escaped_and_commented_commands() {
        let source = concat!(
            "{\\label{nested} + \\tag{x}} + \\\\label{literal} ",
            "\\labelled{x} \\notagged + \\% ",
            "% \\label{comment} {\n",
            "x\\{y\\}\\label{removed}"
        );
        let expected = source.replace(r"\label{removed}", "");
        assert_eq!(strip_tags(source), (expected, true));
    }

    #[test]
    fn handles_balanced_arguments_and_preserves_incomplete_tags() {
        assert_eq!(
            strip_tags("x\\tag % explanation\n{a{b}\\}c}y"),
            ("xy".into(), true)
        );
        for source in [r"x\tag", r"x\tag* 2", r"x\label{open", "x\\"] {
            assert_eq!(strip_tags(source), (source.into(), false));
        }
        assert_eq!(strip_tags(r"x\α\label{x}"), (r"x\α".into(), true));
    }

    #[test]
    fn normalises_each_display_environment_and_starred_form() {
        for (name, target) in [
            ("align", "aligned"),
            ("eqnarray", "aligned"),
            ("gather", "gathered"),
            ("multline", "gathered"),
        ] {
            for star in ["", "*"] {
                let source = format!(
                    " \\begin{{{name}{star}}} a &= b \\\\ c &= d \\end{{{name}{star}}}\n"
                );
                let expected = format!(
                    " \\begin{{{target}}} a &= b \\\\ c &= d \\end{{{target}}}\n"
                );
                assert_eq!(normalise_env(&source), expected);
            }
        }
        for name in ["equation", "equation*"] {
            let source = format!("\\begin{{{name}}}\n x = 2\n\\end{{{name}}}");
            assert_eq!(normalise_env(&source), "\n x = 2\n");
        }
    }

    #[test]
    fn leaves_nested_environments_and_brace_groups_alone() {
        let body = r"\begin{aligned}x&=\text{\begin{gather}a\end{gather}}\end{aligned}";
        let source = format!("\\begin{{equation}}{body}\\end{{equation}}");
        assert_eq!(normalise_env(&source), body);
        let source = r"\begin{cases}\begin{align}x\end{align}\end{cases}";
        assert_eq!(normalise_env(source), source);
        let source = r"{\begin{align}x\end{align}}";
        assert_eq!(normalise_env(source), source);
    }

    #[test]
    fn preserves_unknown_mismatched_escaped_and_commented_wrappers() {
        for source in [
            r"\begin{aligned}x\end{aligned}",
            r"\begin{align}x\end{gather}",
            r"\begin{align}x",
            r"x\end{align}",
            r"\\begin{align}x\\end{align}",
            "% \\begin{align}\nx\n% \\end{align}",
        ] {
            assert_eq!(normalise_env(source), source);
        }
    }

    #[test]
    fn prepares_tags_before_environment_normalisation() {
        let source = r"\begin{equation*}x\label{eq-x}\end{equation*}";
        assert_eq!(prepare(source), ("x".into(), true));
        let source = r"\begin{equation}x\end{equation}";
        assert_eq!(prepare(source), ("x".into(), false));
        assert_eq!(prepare("α + β"), ("α + β".into(), false));
    }

    #[test]
    fn extracts_only_string_macros_and_orders_names() {
        let value = json!({"macros": {
            "\\zz": "#1 + #3", "\\RR": "\\mathbb{R}",
            "\\number": 2, "\\object": {"definition": "x"},
            "\\nothing": null, "name": "unchanged"
        }, "throwOnError": false});
        let options = value.as_object().unwrap();
        assert_eq!(
            macros_from_config(options),
            BTreeMap::from([
                (r"\RR".into(), r"\mathbb{R}".into()),
                (r"\zz".into(), "#1 + #3".into()),
                ("name".into(), "unchanged".into()),
            ])
        );
        for value in
            [json!({}), json!({"macros": "wrong"}), json!({"macros": []})]
        {
            assert!(macros_from_config(value.as_object().unwrap()).is_empty());
        }
    }

    #[test]
    fn macro_arity_uses_highest_single_digit_argument() {
        for (source, expected) in [
            ("", 0),
            (r"\mathbb{R}", 0),
            ("#0 #x #", 0),
            ("#2 + #1 + #2", 2),
            ("#9 #3", 9),
            ("α#4β", 4),
        ] {
            assert_eq!(macro_arity(source), expected);
        }
    }

    #[test]
    fn emits_macros_in_key_order_with_optional_arity() {
        let macros = BTreeMap::from([
            (r"\half".into(), r"\frac{#1}{2}".into()),
            (r"\RR".into(), r"\mathbb{R}".into()),
            (r"\select".into(), "#3+#1".into()),
        ]);
        assert_eq!(
            macros_as_newcommands(&macros),
            concat!(
                "\\newcommand{\\RR}{\\mathbb{R}}\n",
                "\\newcommand{\\half}[1]{\\frac{#1}{2}}\n",
                "\\newcommand{\\select}[3]{#3+#1}\n"
            )
        );
        assert_eq!(macros_as_newcommands(&BTreeMap::new()), "");
        assert_eq!(
            for_typst(
                r"\begin{equation}\half{x}\tag{3}\end{equation}",
                &macros
            ),
            (format!("{}\\half{{x}}", macros_as_newcommands(&macros)), true)
        );
    }
}
