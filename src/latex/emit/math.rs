//! Math passthrough, native conversion and package/Unicode bookkeeping.

use crate::ir::MathDialect;
use crate::math::{latex_source, typst_to_latex};

use super::*;

#[derive(Clone, Copy)]
pub(super) enum UnicodeContext {
    Text,
    Code,
    Verbatim,
    Listings,
    LatexMath,
    ConvertedMath,
}

impl Emitter<'_> {
    pub(super) fn math_used(&mut self) {
        if !self.usage.math {
            self.math_span = self.span.clone();
        }
        self.usage.math = true;
    }

    pub(super) fn math_source(
        &mut self, source: &str, dialect: MathDialect,
    ) -> String {
        self.math_used();
        let text = match dialect {
            MathDialect::Latex => source.to_owned(),
            MathDialect::Typst => {
                let (text, diagnostics) = typst_to_latex::convert(source);
                for diagnostic in diagnostics {
                    self.finding(
                        diagnostic.level, diagnostic.code, diagnostic.message,
                    );
                }
                text
            }
        };
        let context = if dialect == MathDialect::Latex {
            UnicodeContext::LatexMath
        } else {
            UnicodeContext::ConvertedMath
        };
        for token in math_tokens(&text) {
            match token {
                MathToken::Command(name) => {
                    if matches!(name, r"\color" | r"\textcolor") {
                        self.usage.color = true;
                    }
                    if let Some(package) = typst_to_latex::command_package(name)
                    {
                        self.package(
                            package,
                            name,
                            dialect == MathDialect::Typst,
                        );
                    }
                }
                MathToken::Unicode(glyph) => self.unicode(glyph, context),
            }
        }
        text
    }

    fn package(&mut self, package: &'static str, command: &str, native: bool) {
        if !self.usage.math_packages.contains(&package) {
            self.usage.math_packages.push(package);
            if native {
                self.finding(
                    Level::Info,
                    "math-latex-package",
                    format!("added \\usepackage{{{package}}} for {command}",),
                );
            }
        }
    }

    pub(super) fn display_math(
        &mut self, source: &str, dialect: MathDialect, attrs: &Attrs,
        numbered: bool,
    ) -> Output {
        let source = self.math_source(source, dialect);
        let label = self.label(attrs.id.as_deref(), numbered);
        if let Some((start, end, name)) = bare_environment(&source) {
            let desired = format!("{name}{}", if numbered { "" } else { "*" });
            let mut text = String::new();
            if !numbered && !label.is_empty() {
                text.push_str(&label);
                text.push('\n');
            }
            text.push_str(&source[..start]);
            text.push_str(&format!("\\begin{{{desired}}}"));
            if numbered && !label.is_empty() {
                text.push_str(&label);
            }
            text.push_str(&source[end..]);
            let original = &source[start + "\\begin{".len()..end - 1];
            if let Some(close) = text.rfind(&format!("\\end{{{original}}}")) {
                let length = "\\end{}".len() + original.len();
                text.replace_range(
                    close..close + length,
                    &format!("\\end{{{desired}}}"),
                );
            }
            return text.into();
        }
        let text = if numbered {
            format!(
                "\\begin{{equation}}\n{source}\n{}\\end{{equation}}",
                if label.is_empty() {
                    String::new()
                } else {
                    format!("{label}\n")
                },
            )
        } else {
            format!(
                "{}\\[\n{source}\n\\]",
                if label.is_empty() {
                    String::new()
                } else {
                    format!("{label}\n")
                }
            )
        };
        text.into()
    }

    pub(super) fn unicode(&mut self, text: &str, context: UnicodeContext) {
        for ch in text.chars().filter(|ch| !ch.is_ascii()) {
            if matches!(context, UnicodeContext::LatexMath) && self.map {
                if let Some(symbol) = typst_to_latex::char_command(ch) {
                    self.unicode_definition(ch, symbol.tex, true);
                    if let Some(package) = symbol.package {
                        self.package(package, symbol.tex, false);
                    }
                } else {
                    self.once(
                        format!("math-unicode:{ch}"),
                        Level::Warning,
                        "math-conversion-failed",
                        format!(
                            "latex: no LaTeX mapping for U+{:04X} ({ch})",
                            ch as u32,
                        ),
                    );
                }
                continue;
            }
            // Converted math is already emitted TeX; text() does not run on
            // surviving glyphs and cannot supply its replacements here.
            let mapping = if self.map
                && !matches!(context, UnicodeContext::ConvertedMath)
            {
                escape::map_char(ch)
            } else {
                None
            };
            if let Some(command) = mapping {
                self.math_used();
                if matches!(
                    context,
                    UnicodeContext::Verbatim | UnicodeContext::Listings
                ) {
                    self.unicode_definition(ch, command, false);
                }
            }
            // These text characters have unconditional ASCII replacements;
            // code/verbatim contexts intentionally retain their source bytes.
            let text_special = matches!(context, UnicodeContext::Text)
                && matches!(
                    ch,
                    '\u{a0}'
                        | '\u{ad}'
                        | '\u{200b}'
                        | '\u{202f}'
                        | '\u{2026}'
                        | '\u{2018}'
                        | '\u{2019}'
                        | '\u{201c}'
                        | '\u{201d}'
                        | '\u{2013}'
                        | '\u{2014}'
                );
            let listings = matches!(context, UnicodeContext::Listings)
                && self.config.latex.engine == LatexEngine::Pdflatex;
            let offending = listings
                || (!text_special
                    && mapping.is_none()
                    && !escape::pdflatex_safe(ch));
            if offending && !self.unicode.contains(&ch) {
                if self.unicode.is_empty() {
                    self.unicode_span = self.span.clone();
                }
                self.unicode.push(ch);
            }
        }
    }

    fn unicode_definition(
        &mut self, ch: char, command: &'static str, math: bool,
    ) {
        self.math_used();
        if let Some((_, previous)) = self
            .usage
            .newunicodechar
            .iter_mut()
            .find(|(existing, _)| *existing == ch)
        {
            if math {
                *previous = command;
            }
        } else {
            self.usage.newunicodechar.push((ch, command));
        }
    }

    pub(super) fn finish(&mut self) {
        if self.usage.math {
            let previous =
                std::mem::replace(&mut self.span, self.math_span.clone());
            for (name, _) in latex_source::macros_from_config(
                &self.config.math.katex_options,
            ) {
                let valid = name.strip_prefix('\\').is_some_and(|name| {
                    !name.is_empty()
                        && name.bytes().all(|byte| byte.is_ascii_alphabetic())
                });
                if !valid {
                    self.finding(Level::Warning, "math-macro-invalid", format!(
                        "[math] katex_options.macros[\"{name}\"]: expected a backslash followed by ASCII letters",
                    ));
                }
            }
            self.span = previous;
        }
        if !self.unicode.is_empty() {
            let mut message = self
                .unicode
                .iter()
                .take(10)
                .map(|ch| format!("U+{:04X} ({ch})", *ch as u32))
                .collect::<Vec<_>>()
                .join(", ");
            if self.unicode.len() > 10 {
                message.push_str(&format!(
                    " and {} more",
                    self.unicode.len() - 10
                ));
            }
            let previous =
                std::mem::replace(&mut self.span, self.unicode_span.clone());
            self.finding(
                if self.config.latex.engine == LatexEngine::Pdflatex {
                    Level::Warning
                } else {
                    Level::Info
                },
                "latex-unicode",
                format!("characters not supported by pdfLaTeX: {message}"),
            );
            self.span = previous;
        }
    }
}

fn bare_environment(source: &str) -> Option<(usize, usize, &str)> {
    let start = source.len() - source.trim_start().len();
    let body = source[start..].strip_prefix("\\begin{")?;
    let end = body.find('}')?;
    let written = &body[..end];
    let name = written.strip_suffix('*').unwrap_or(written);
    matches!(name, "align" | "gather" | "multline" | "equation" | "eqnarray")
        .then_some((start, start + "\\begin{".len() + end + 1, name))
}

#[derive(Debug, PartialEq)]
enum MathToken<'a> {
    Command(&'a str),
    Unicode(&'a str),
}

/// Scans commands and glyphs in source order. A control symbol consumes its
/// following scalar, so an escaped backslash cannot start another control word.
fn math_tokens(source: &str) -> impl Iterator<Item = MathToken<'_>> {
    let mut chars = source.char_indices().peekable();
    std::iter::from_fn(move || {
        loop {
            let (index, ch) = chars.next()?;
            if ch == '\\' {
                let (start, next) = chars.next()?;
                if next.is_ascii_alphabetic() {
                    while chars
                        .peek()
                        .is_some_and(|&(_, ch)| ch.is_ascii_alphabetic())
                    {
                        chars.next();
                    }
                    let end =
                        chars.peek().map_or(source.len(), |&(index, _)| index);
                    return Some(MathToken::Command(&source[index..end]));
                }
                if !next.is_ascii() {
                    return Some(MathToken::Unicode(
                        &source[start..start + next.len_utf8()],
                    ));
                }
            } else if !ch.is_ascii() {
                return Some(MathToken::Unicode(
                    &source[index..index + ch.len_utf8()],
                ));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{DocMeta, Features, Resolved, SourceInfo, SourceKind};

    fn span(line: usize, file: &str) -> SourceSpan {
        SourceSpan {
            start_line: line,
            end_line: line,
            cell: None,
            file: Some(file.into()),
        }
    }

    fn equation(source: &str, dialect: MathDialect, line: usize) -> Block {
        Block::Math {
            source: source.into(),
            dialect,
            attrs: Attrs::default(),
            number: None,
            span: Some(span(line, "/book/math.md")),
        }
    }

    fn code(source: &str, line: usize) -> Block {
        Block::CodeBlock {
            lang: None,
            text: source.into(),
            attrs: Attrs::default(),
            span: Some(span(line, "/book/code.md")),
        }
    }

    fn prose(source: &str, line: usize) -> Block {
        Block::Paragraph {
            inlines: vec![Inline::text(source)],
            span: Some(span(line, "/book/prose.md")),
        }
    }

    fn render(blocks: Vec<Block>, config: &Config) -> Emitted {
        let document = Document {
            meta: DocMeta::default(),
            blocks,
            features: Features::default(),
            source: SourceInfo {
                path: "/book/main.md".into(),
                base_dir: "/book".into(),
                kind: SourceKind::Markdown,
            },
            assets: Vec::new(),
            includes: Vec::new(),
            resolved: Resolved::default(),
        };
        super::super::emit(&document, &AssetRegistry::new(), config, "main")
    }

    #[test]
    fn bare_environment_accepts_exactly_one_optional_star_and_keeps_byte_offsets()
     {
        for name in ["align", "gather", "multline", "equation", "eqnarray"] {
            for star in ["", "*"] {
                let source = format!(
                    " \t\n\\begin{{{name}{star}}}x\\end{{{name}{star}}}"
                );
                let (start, end, base) = bare_environment(&source).unwrap();
                assert_eq!(&source[..start], " \t\n");
                assert_eq!(
                    &source[start..end],
                    format!("\\begin{{{name}{star}}}")
                );
                assert_eq!(base, name);
                assert_eq!(&source[end..], format!("x\\end{{{name}{star}}}"));
            }
        }
        for name in [
            "align**", "gather***", "aligned", "flalign", "alignat", "Align",
            "align *", "equation*extra",
        ] {
            assert!(
                bare_environment(&format!("\\begin{{{name}}}x\\end{{{name}}}"))
                    .is_none(),
                "{name}"
            );
        }
        assert!(
            bare_environment("prefix \\begin{align}x\\end{align}").is_none()
        );
        assert!(
            bare_environment("% comment\n\\begin{align}x\\end{align}")
                .is_none()
        );
        assert!(bare_environment("\\begin{align").is_none());
        let source = "\u{2003}\\begin{align}x\\end{align}";
        let (start, end, name) = bare_environment(source).unwrap();
        assert_eq!(start, '\u{2003}'.len_utf8());
        assert_eq!(&source[start..end], r"\begin{align}");
        assert_eq!(name, "align");
    }

    #[test]
    fn bare_equations_change_only_the_outer_names_and_required_anchor() {
        let body = r"a &= b \\ c &= d \tag{kept}\label{author}";
        let anchor = crate::resolve::anchor("eq-test");
        for numbered in [false, true] {
            let source = format!(" \t\\begin{{align*}}{body}\\end{{align*}}");
            let mut block = equation(&source, MathDialect::Latex, 2);
            let Block::Math { attrs, number, .. } = &mut block else {
                panic!()
            };
            attrs.id = Some("eq-test".into());
            *number = numbered.then(|| "9999".into());
            let emitted = render(vec![block], &Config::default());
            let expected = if numbered {
                format!(
                    " \t\\begin{{align}}\\label{{{anchor}}}{body}\\end{{align}}\n"
                )
            } else {
                format!(
                    "\\phantomsection\\label{{{anchor}}}\n \t\\begin{{align*}}{body}\\end{{align*}}\n"
                )
            };
            assert_eq!(emitted.body, expected);
        }
        let source = r"\begin{align**}x\end{align**}";
        let emitted = render(
            vec![equation(source, MathDialect::Latex, 1)],
            &Config::default(),
        );
        assert_eq!(emitted.body, format!("\\[\n{source}\n\\]\n"));
    }

    #[test]
    fn source_scanner_consumes_control_symbols_and_retains_full_control_words()
    {
        assert_eq!(math_tokens(r"\\mathscr \\color").collect::<Vec<_>>(), []);
        assert_eq!(
            math_tokens(r"\\\mathscr{x}").collect::<Vec<_>>(),
            [MathToken::Command(r"\mathscr")]
        );
        assert_eq!(
            math_tokens(r"\mathscrx α\é\cancel{x}\%\color{red}")
                .collect::<Vec<_>>(),
            [
                MathToken::Command(r"\mathscrx"),
                MathToken::Unicode("α"),
                MathToken::Unicode("é"),
                MathToken::Command(r"\cancel"),
                MathToken::Command(r"\color"),
            ]
        );
        assert_eq!(math_tokens("\\").collect::<Vec<_>>(), []);
        assert_eq!(
            math_tokens(r"\{mathscr\}\%textcolor").collect::<Vec<_>>(),
            []
        );
    }

    #[test]
    fn package_lookup_is_exact_and_mixed_unicode_commands_keep_source_order() {
        let source = r"\mathscr{A} + ⬳ + \cancel{x} + \textcolor{red}{x}";
        let emitted = render(
            vec![equation(source, MathDialect::Latex, 1)],
            &Config::default(),
        );
        assert_eq!(
            emitted.usage.math_packages,
            ["mathrsfs", "graphicx", "cancel"]
        );
        assert!(emitted.usage.color);
        assert_eq!(emitted.usage.newunicodechar.len(), 1);
        assert_eq!(emitted.usage.newunicodechar[0].0, '⬳');
        assert!(
            !emitted
                .diagnostics
                .iter()
                .any(|finding| finding.code == "math-latex-package")
        );
        assert!(emitted.body.contains(source));
        let escaped = render(
            vec![equation(
                r"\\mathscr{x} \\color{red} \mathscrx",
                MathDialect::Latex,
                2,
            )],
            &Config::default(),
        );
        assert!(escaped.usage.math_packages.is_empty());
        assert!(!escaped.usage.color);
    }

    #[test]
    fn native_package_information_is_new_once_located_and_has_one_command_slash()
     {
        let emitted = render(
            vec![
                equation("scr(A) + cancel(x)", MathDialect::Typst, 3),
                equation("cancel(y)", MathDialect::Typst, 8),
            ],
            &Config::default(),
        );
        assert_eq!(emitted.usage.math_packages, ["mathrsfs", "cancel"]);
        let notes = emitted
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "math-latex-package")
            .collect::<Vec<_>>();
        assert_eq!(notes.len(), 2);
        assert_eq!(
            notes[0].message,
            r"added \usepackage{mathrsfs} for \mathscr"
        );
        assert_eq!(notes[1].message, r"added \usepackage{cancel} for \cancel");
        for note in notes {
            assert_eq!(note.level, Level::Info);
            assert_eq!(note.span, Some(span(3, "/book/math.md")));
        }
        let emitted = render(
            vec![
                equation(r"\cancel{x}", MathDialect::Latex, 1),
                equation("cancel(y)", MathDialect::Typst, 2),
            ],
            &Config::default(),
        );
        assert_eq!(emitted.usage.math_packages, ["cancel"]);
        assert!(
            !emitted
                .diagnostics
                .iter()
                .any(|finding| finding.code == "math-latex-package")
        );
    }

    #[test]
    fn math_definitions_override_verbatim_macros_without_moving_the_first_entry()
     {
        assert_eq!(escape::map_char('′'), Some(r"{}^{\prime}"));
        assert_eq!(typst_to_latex::char_command('′').unwrap().tex, "'");
        assert!(escape::pdflatex_safe('→'));
        let emitted = render(
            vec![
                code("′", 1),
                equation("→′", MathDialect::Latex, 4),
                code("′", 8),
            ],
            &Config::default(),
        );
        assert_eq!(
            emitted.usage.newunicodechar,
            [('′', "'"), ('→', r"\rightarrow")]
        );
        assert!(
            !emitted
                .diagnostics
                .iter()
                .any(|finding| finding.code == "latex-unicode")
        );
        assert!(emitted.body.contains("→′"));
    }

    #[test]
    fn effective_unicode_policy_handles_safe_math_characters_and_keep_levels() {
        for (engine, policy, mapped, level) in [
            (LatexEngine::Pdflatex, LatexUnicode::Auto, true, Level::Warning),
            (LatexEngine::Xelatex, LatexUnicode::Auto, false, Level::Info),
            (LatexEngine::Pdflatex, LatexUnicode::Keep, false, Level::Warning),
            (LatexEngine::Xelatex, LatexUnicode::Map, true, Level::Info),
            (LatexEngine::Lualatex, LatexUnicode::Keep, false, Level::Info),
        ] {
            let mut config = Config::default();
            config.latex.engine = engine;
            config.latex.unicode = policy;
            let emitted =
                render(vec![equation("α→", MathDialect::Latex, 5)], &config);
            assert!(emitted.body.contains("α→"));
            let warning = emitted
                .diagnostics
                .iter()
                .find(|finding| finding.code == "latex-unicode");
            if mapped {
                assert_eq!(
                    emitted.usage.newunicodechar,
                    [('α', r"\alpha"), ('→', r"\rightarrow")]
                );
                assert!(warning.is_none());
            } else {
                assert!(emitted.usage.newunicodechar.is_empty());
                let warning = warning.unwrap();
                assert_eq!(warning.level, level);
                assert!(warning.message.contains("U+03B1 (α)"));
                assert!(!warning.message.contains("U+2192"));
            }
        }
    }

    #[test]
    fn unknown_latex_math_unicode_warns_once_at_the_first_equation() {
        let emitted = render(
            vec![
                equation("😀 😀", MathDialect::Latex, 2),
                equation("😀", MathDialect::Latex, 9),
            ],
            &Config::default(),
        );
        let findings = emitted
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "math-conversion-failed")
            .collect::<Vec<_>>();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("U+1F600 (😀)"));
        assert_eq!(findings[0].span, Some(span(2, "/book/math.md")));
        assert!(emitted.usage.newunicodechar.is_empty());
        assert!(
            !emitted
                .diagnostics
                .iter()
                .any(|finding| finding.code == "latex-unicode")
        );
    }

    #[test]
    fn macro_validation_is_delayed_until_math_and_keeps_the_first_math_location()
     {
        let mut config = Config::default();
        config.typst.strict = true;
        config.math.katex_options.insert(
            "macros".into(),
            serde_json::json!({
                "\\Good": "x", "\\+": "y", "bad": "z", "\\Ignored": 1,
            }),
        );
        let plain = render(vec![prose("plain", 1)], &config);
        assert!(
            !plain
                .diagnostics
                .iter()
                .any(|finding| finding.code == "math-macro-invalid")
        );
        let emitted = render(
            vec![
                prose("α", 2),
                equation("x", MathDialect::Latex, 9),
                prose("last", 20),
            ],
            &config,
        );
        let warnings = emitted
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "math-macro-invalid")
            .collect::<Vec<_>>();
        assert_eq!(warnings.len(), 2);
        for warning in warnings {
            assert_eq!(warning.level, Level::Warning);
            assert_eq!(
                warning.path.as_deref(),
                Some(std::path::Path::new("/book/prose.md"))
            );
            assert_eq!(warning.span, Some(span(2, "/book/prose.md")));
        }
    }

    #[test]
    fn unicode_summary_preserves_first_location_order_and_the_ten_character_limit()
     {
        let first: String = (0x4e00..=0x4e0a)
            .map(|value| char::from_u32(value).unwrap())
            .collect();
        let emitted = render(
            vec![prose(&first, 4), prose("😀一", 9)],
            &Config::default(),
        );
        let warnings = emitted
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "latex-unicode")
            .collect::<Vec<_>>();
        assert_eq!(warnings.len(), 1);
        let warning = warnings[0];
        assert_eq!(warning.level, Level::Warning);
        assert_eq!(warning.span, Some(span(4, "/book/prose.md")));
        assert_eq!(
            warning.path.as_deref(),
            Some(std::path::Path::new("/book/prose.md"))
        );
        assert!(warning.message.contains("U+4E00 (一), U+4E01 (丁)"));
        assert!(warning.message.contains("U+4E09 (三) and 2 more"));
        assert!(!warning.message.contains("U+4E0A"));
        assert!(!warning.message.contains("U+1F600"));
    }

    #[test]
    fn explicit_text_replacements_do_not_hide_surviving_code_characters() {
        let emitted = render(
            vec![prose("\u{200b}\u{202f}\u{a0}\u{ad}", 1)],
            &Config::default(),
        );
        assert!(
            !emitted
                .diagnostics
                .iter()
                .any(|finding| finding.code == "latex-unicode")
        );
        let emitted = render(
            vec![code("\u{200b}\u{202f}\u{a0}\u{ad}", 7)],
            &Config::default(),
        );
        let warning = emitted
            .diagnostics
            .iter()
            .find(|finding| finding.code == "latex-unicode")
            .unwrap();
        assert!(warning.message.contains("U+200B"));
        assert!(warning.message.contains("U+202F"));
        assert!(!warning.message.contains("U+00A0"));
        assert!(!warning.message.contains("U+00AD"));
        assert_eq!(warning.span, Some(span(7, "/book/code.md")));
    }
}
