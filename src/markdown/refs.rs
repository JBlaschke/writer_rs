//! Syntactic references and citation groups (markdown-dialect.md §7).
//!
//! The walker calls this on AST text while escape and link ancestry still
//! exist. No bibliography or label lookup belongs to this phase.

use std::sync::LazyLock;

use regex::Regex;

use crate::config::Config;
use crate::error::{Diagnostic, Diagnostics, Result};
use crate::ir::{CiteItem, CiteMode, Inline, Inlines, RefForm, SourceSpan};

use super::attrs::parse_attr_block;
use super::{Fragment, parse_inlines};

/// Scans one unescaped AST text node; its preceding sibling controls `@` at 0.
pub(crate) fn scan(
    text: &str, start_allowed: bool, span: &SourceSpan, config: &Config,
    ctx: &mut Fragment<'_>,
) -> Result<Inlines> {
    if !config.markdown.references {
        return Ok(vec![Inline::text(text)]);
    }
    let mut scanner = Scanner { config, ctx, span };
    let mut out = Vec::new();
    let mut at = 0;
    let mut copied = 0;
    while at < text.len() {
        let parsed = match text.as_bytes()[at] {
            b'[' => scanner.group(&text[at..])?,
            b'@' if valid_start(text, at, start_allowed, false) => {
                scanner.bare(&text[at..])
            }
            _ => None,
        };
        if let Some(parsed) = parsed {
            push_text(&mut out, &text[copied..at]);
            out.push(parsed.inline);
            push_text(&mut out, parsed.literal);
            at += parsed.consumed;
            copied = at;
        } else {
            // `at` advances by whole characters, including non-ASCII targets.
            at += text[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    push_text(&mut out, &text[copied..]);
    Ok(out)
}

struct Scanner<'a, 'c, 'f> {
    config: &'a Config,
    ctx: &'c mut Fragment<'f>,
    span: &'a SourceSpan,
}

struct Parsed<'a> {
    inline: Inline,
    consumed: usize,
    literal: &'a str,
}

#[derive(Clone, Copy)]
enum Mode {
    Default,
    Page,
    Cite(CiteMode),
}

struct ModeSuffix<'a> {
    mode: Mode,
    consumed: usize,
    literal: &'a str,
}

impl Scanner<'_, '_, '_> {
    fn bare<'t>(&mut self, text: &'t str) -> Option<Parsed<'t>> {
        let target = target(&text[1..])?;
        let mut consumed = 1 + target.len();
        let supplement = bracket(&text[consumed..]).map(|(inner, length)| {
            consumed += length;
            inner
        });
        let mode = self.mode(&text[consumed..], false);
        let inline = match mode.mode {
            Mode::Cite(mode) => Inline::Cite {
                items: vec![CiteItem {
                    key: target.into(),
                    locator: supplement
                        .filter(|text| !text.is_empty())
                        .map(str::to_owned),
                    ..CiteItem::default()
                }],
                mode,
                resolved: None,
                span: Some(self.span.clone()),
            },
            Mode::Default | Mode::Page => Inline::Ref {
                target: target.into(),
                supplement: supplement.map(|text| {
                    if text.is_empty() {
                        Vec::new()
                    } else {
                        vec![Inline::text(text)]
                    }
                }),
                form: if matches!(mode.mode, Mode::Page) {
                    RefForm::Page
                } else {
                    RefForm::Normal
                },
                resolved: None,
                span: Some(self.span.clone()),
            },
        };
        Some(Parsed {
            inline,
            consumed: consumed + mode.consumed,
            literal: mode.literal,
        })
    }

    fn group<'t>(&mut self, text: &'t str) -> Result<Option<Parsed<'t>>> {
        let Some((inner, length)) = bracket(text) else {
            return Ok(None);
        };
        let Some(parsed) =
            inner.split(';').map(group_item).collect::<Option<Vec<_>>>()
        else {
            return Ok(None);
        };
        let mut items = Vec::with_capacity(parsed.len());
        let mut default = CiteMode::Normal;
        for item in parsed {
            if item.negative {
                default = CiteMode::Year;
            }
            let prefix = if item.prefix.is_empty() {
                None
            } else {
                Some(self.prefix(item.prefix)?)
            };
            items.push(CiteItem {
                key: item.key.into(),
                prefix,
                locator: item.locator.map(str::to_owned),
                suffix: None,
            });
        }
        let mode = self.mode(&text[length..], true);
        let mode_value = match mode.mode {
            Mode::Cite(mode) => mode,
            Mode::Default | Mode::Page => default,
        };
        Ok(Some(Parsed {
            inline: Inline::Cite {
                items,
                mode: mode_value,
                resolved: None,
                span: Some(self.span.clone()),
            },
            consumed: length + mode.consumed,
            literal: mode.literal,
        }))
    }

    fn mode<'t>(&mut self, text: &'t str, group: bool) -> ModeSuffix<'t> {
        let Some(length) = attr_length(text) else {
            return ModeSuffix {
                mode: Mode::Default,
                consumed: 0,
                literal: "",
            };
        };
        let attrs = &text[..length];
        let mode = parse_attr_block(attrs).ok().and_then(|parsed| {
            if parsed.id.is_some()
                || !parsed.pairs.is_empty()
                || parsed.classes.len() != 1
            {
                return None;
            }
            match parsed.classes[0].as_str() {
                "page" if !group => Some(Mode::Page),
                "normal" => Some(Mode::Cite(CiteMode::Normal)),
                "prose" => Some(Mode::Cite(CiteMode::Prose)),
                "year" => Some(Mode::Cite(CiteMode::Year)),
                "author" => Some(Mode::Cite(CiteMode::Author)),
                "full" => Some(Mode::Cite(CiteMode::Full)),
                "none" => Some(Mode::Cite(CiteMode::None)),
                _ => None,
            }
        });
        if let Some(mode) = mode {
            ModeSuffix { mode, consumed: length, literal: "" }
        } else {
            let mut diagnostic = Diagnostic::warning(
                "invalid-reference-mode", format!("invalid reference mode {attrs}; use exactly one permitted mode class"),
            ).with_span(self.span.clone());
            diagnostic.path = self.ctx.file.map(std::path::Path::to_path_buf);
            self.ctx.diagnostics.push(diagnostic);
            // Invalid attrs remain one literal run, including any @ inside them.
            ModeSuffix { mode: Mode::Default, consumed: length, literal: attrs }
        }
    }

    fn prefix(&mut self, text: &str) -> Result<Inlines> {
        let mut config = self.config.clone();
        config.markdown.references = false;
        let mut diagnostics = Diagnostics::new();
        let result = parse_inlines(
            text,
            &config,
            &mut Fragment {
                assets: self.ctx.assets,
                base_dir: self.ctx.base_dir,
                diagnostics: &mut diagnostics,
                file: self.ctx.file,
                cell: self.ctx.cell,
                missing_asset_is_error: self.ctx.missing_asset_is_error,
            },
        )?;
        for mut diagnostic in diagnostics.into_vec() {
            diagnostic.span = Some(self.span.clone());
            self.ctx.diagnostics.push(diagnostic);
        }
        Ok(result)
    }
}

struct GroupItem<'a> {
    key: &'a str,
    prefix: &'a str,
    locator: Option<&'a str>,
    negative: bool,
}

fn group_item(text: &str) -> Option<GroupItem<'_>> {
    let mut starts = text.char_indices().filter_map(|(at, c)| {
        (c == '@' && valid_start(text, at, true, true)).then_some(at)
    });
    let at = starts.next()?;
    if starts.next().is_some() {
        return None;
    }
    let key = target(&text[at + 1..])?;
    let tail = text[at + 1 + key.len()..].trim();
    let locator = if tail.is_empty() {
        None
    } else {
        Some(tail.strip_prefix(',')?.trim()).filter(|text| !text.is_empty())
    };
    let prefix = text[..at].trim();
    let negative = prefix.ends_with('-');
    let prefix = if negative {
        prefix[..prefix.len() - 1].trim_end()
    } else {
        prefix
    };
    Some(GroupItem { key, prefix, locator, negative })
}

fn valid_start(text: &str, at: usize, initial: bool, group: bool) -> bool {
    if at == 0 {
        return initial;
    }
    text[..at].chars().next_back().is_some_and(|c| {
        c.is_whitespace() || matches!(c, '[' | '(' | ';') || group && c == '-'
    })
}

fn target(text: &str) -> Option<&str> {
    static TARGET: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[\p{XID_Continue}_\-:.]+")
            .expect("fixed Unicode target regex")
    });
    let matched = TARGET.find(text)?.as_str().trim_end_matches(['.', ':']);
    (!matched.is_empty()).then_some(matched)
}

/// Plain bracket text stays within its one AST text node, with nesting intact.
fn bracket(text: &str) -> Option<(&str, usize)> {
    if !text.starts_with('[') {
        return None;
    }
    let mut depth = 0;
    for (at, c) in text.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&text[1..at], at + 1));
                }
            }
            _ => {}
        }
    }
    None
}

fn attr_length(text: &str) -> Option<usize> {
    if !text.starts_with('{') {
        return None;
    }
    let mut quote = None;
    let mut escaped = false;
    for (at, c) in text.char_indices().skip(1) {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
        } else if matches!(c, '\'' | '"') {
            quote = Some(c);
        } else if c == '}' {
            return Some(at + 1);
        }
    }
    // A malformed quote still makes a complete brace block invalid, not invisible.
    text.find('}').map(|at| at + 1)
}

fn push_text(out: &mut Inlines, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Inline::Text { text: previous }) = out.last_mut() {
        previous.push_str(text);
    } else {
        out.push(Inline::text(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::assets::registry::AssetRegistry;
    use crate::ir::{self, Block, SourceInfo, SourceKind};

    fn span() -> SourceSpan {
        SourceSpan {
            start_line: 4,
            end_line: 6,
            cell: Some(3),
            file: Some("chapter.md".into()),
        }
    }

    fn scanned(text: &str, start: bool) -> (Inlines, Diagnostics) {
        let mut assets = AssetRegistry::new();
        let mut diagnostics = Diagnostics::new();
        let inlines = scan(
            text,
            start,
            &span(),
            &Config::default(),
            &mut Fragment {
                assets: &mut assets,
                base_dir: Path::new("."),
                diagnostics: &mut diagnostics,
                file: Some(Path::new("chapter.md")),
                cell: Some(3),
                missing_asset_is_error: false,
            },
        )
        .unwrap();
        (inlines, diagnostics)
    }

    fn reference(
        target: &str, supplement: Option<Inlines>, form: RefForm,
    ) -> Inline {
        Inline::Ref {
            target: target.into(),
            supplement,
            form,
            resolved: None,
            span: Some(span()),
        }
    }

    fn parse_markdown(text: &str, references: bool) -> ir::Parsed {
        let mut config = Config::default();
        config.markdown.references = references;
        super::super::parse(
            &SourceInfo {
                path: "references.md".into(),
                base_dir: ".".into(),
                kind: SourceKind::Markdown,
            },
            text,
            &config,
        )
        .unwrap()
    }

    fn targets(document: &ir::Document) -> Vec<String> {
        let mut targets = Vec::new();
        ir::walk_inlines(document, &mut |inline| match inline {
            Inline::Ref { target, .. } => targets.push(target.clone()),
            Inline::Cite { items, .. } => {
                targets.extend(items.iter().map(|item| item.key.clone()))
            }
            _ => {}
        });
        targets
    }

    #[test]
    fn start_and_punctuation_boundaries() {
        for text in ["@x", " @x", "\t@x", "\u{2003}@x", "(@x)", "a;@x", "a[@x"]
        {
            let (inlines, diagnostics) = scanned(text, true);
            assert!(diagnostics.is_empty(), "{text:?}");
            assert_eq!(
                inlines
                    .iter()
                    .filter(|i| matches!(i, Inline::Ref { .. }))
                    .count(),
                1,
                "{text:?}"
            );
        }
        for text in
            ["a@x", "-@x", ":@x", ".@x", ",@x", "/@x", "x@domain.example"]
        {
            let (inlines, _) = scanned(text, true);
            assert_eq!(inlines, vec![Inline::text(text)], "{text:?}");
        }
        assert_eq!(scanned("@x", false).0, [Inline::text("@x")]);
        assert_eq!(
            scanned(" @x", false).0,
            [Inline::text(" "), reference("x", None, RefForm::Normal)]
        );
    }

    #[test]
    fn unicode_and_trailing_target_punctuation() {
        let (inlines, _) = scanned("@é́_2-表.part:sub..: next", true);
        assert_eq!(
            inlines,
            [
                reference("é́_2-表.part:sub", None, RefForm::Normal),
                Inline::text("..: next"),
            ]
        );
        for text in ["@", "@.", "@:", "@...::", "@😀"] {
            assert_eq!(scanned(text, true).0, [Inline::text(text)]);
        }
        assert_eq!(
            scanned("@x/@y", true).0,
            [reference("x", None, RefForm::Normal), Inline::text("/@y"),]
        );
    }

    #[test]
    fn three_supplement_shapes_and_page() {
        assert_eq!(
            scanned("@x @x[] @x[Fig.] @x[ ]", true).0,
            [
                reference("x", None, RefForm::Normal),
                Inline::text(" "),
                reference("x", Some(vec![]), RefForm::Normal),
                Inline::text(" "),
                reference(
                    "x",
                    Some(vec![Inline::text("Fig.")]),
                    RefForm::Normal
                ),
                Inline::text(" "),
                reference("x", Some(vec![Inline::text(" ")]), RefForm::Normal),
            ]
        );
        assert_eq!(
            scanned("@x[see [appendix]]{.page}", true).0,
            [reference(
                "x",
                Some(vec![Inline::text("see [appendix]")]),
                RefForm::Page
            ),]
        );
        assert_eq!(
            scanned("@x[unclosed", true).0,
            [reference("x", None, RefForm::Normal), Inline::text("[unclosed"),]
        );
    }

    #[test]
    fn every_cite_only_mode_and_locator() {
        for (name, mode) in [
            ("normal", CiteMode::Normal),
            ("prose", CiteMode::Prose),
            ("year", CiteMode::Year),
            ("author", CiteMode::Author),
            ("full", CiteMode::Full),
            ("none", CiteMode::None),
        ] {
            for (suffix, locator) in [
                ("", None),
                ("[]", None),
                ("[p. 3]", Some("p. 3")),
                ("[ ]", Some(" ")),
            ] {
                let text = format!("@key{suffix}{{.{name}}}");
                let (inlines, diagnostics) = scanned(&text, true);
                assert!(diagnostics.is_empty());
                assert_eq!(
                    inlines,
                    [Inline::Cite {
                        items: vec![CiteItem {
                            key: "key".into(),
                            locator: locator.map(str::to_owned),
                            ..Default::default()
                        }],
                        mode,
                        resolved: None,
                        span: Some(span()),
                    }]
                );
            }
        }
    }

    #[test]
    fn bracket_group_prefixes_and_locators() {
        let (inlines, diagnostics) = scanned("[see @a; @b, p. 12]", true);
        assert!(diagnostics.is_empty());
        assert_eq!(
            inlines,
            [Inline::Cite {
                items: vec![
                    CiteItem {
                        key: "a".into(),
                        prefix: Some(vec![Inline::text("see")]),
                        ..Default::default()
                    },
                    CiteItem {
                        key: "b".into(),
                        locator: Some("p. 12".into()),
                        ..Default::default()
                    },
                ],
                mode: CiteMode::Normal,
                resolved: None,
                span: Some(span()),
            }]
        );
        let (inlines, _) = scanned("[*see* @a]", true);
        let Inline::Cite { items, .. } = &inlines[0] else {
            panic!()
        };
        // Prefixes go through the Markdown fragment parser with scanning off.
        assert_eq!(
            items[0].prefix,
            Some(vec![Inline::Emph { inlines: vec![Inline::text("see")] }])
        );
        // The scanner sees decoded AST text, not the raw entity spellings.
        let parsed = parse_markdown("[&ast;see&ast; @a]", true);
        let Block::Paragraph { inlines, .. } = &parsed.document.blocks[0]
        else {
            panic!()
        };
        let Inline::Cite { items, .. } = &inlines[0] else {
            panic!()
        };
        assert_eq!(
            items[0].prefix,
            Some(vec![Inline::Emph { inlines: vec![Inline::text("see")] }])
        );
    }

    #[test]
    fn negative_items_and_explicit_group_modes() {
        for text in ["[-@a]", "[see -@a; @b]", "[@a; -@b]"] {
            let (inlines, _) = scanned(text, true);
            assert!(
                matches!(
                    &inlines[0],
                    Inline::Cite { mode: CiteMode::Year, .. }
                ),
                "{text}"
            );
        }
        let (inlines, _) = scanned("[-@a; @b]{.prose}", true);
        assert!(matches!(
            &inlines[0],
            Inline::Cite { mode: CiteMode::Prose, .. }
        ));
    }

    #[test]
    fn invalid_attrs_are_literal_and_warn_at_enclosing_span() {
        for attrs in [
            "{}", "{.bogus}", "{.year .prose}", "{.year .year}", "{#id}",
            "{.page x=1}", "{.none title=\" @hidden\"}",
        ] {
            let (inlines, diagnostics) = scanned(&format!("@x{attrs}"), true);
            assert_eq!(
                inlines,
                [reference("x", None, RefForm::Normal), Inline::text(attrs)]
            );
            assert_eq!(diagnostics.len(), 1, "{attrs}");
            let finding = diagnostics.iter().next().unwrap();
            assert_eq!(finding.code, "invalid-reference-mode");
            assert_eq!(finding.span, Some(span()));
            assert_eq!(finding.path.as_deref(), Some(Path::new("chapter.md")));
        }
        let (inlines, diagnostics) = scanned("[-@x]{.page}", true);
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            &inlines[0],
            Inline::Cite { mode: CiteMode::Year, .. }
        ));
        assert_eq!(inlines[1], Inline::text("{.page}"));
    }

    #[test]
    fn malformed_groups_preserve_unmatched_text() {
        assert_eq!(
            scanned("[see @a", true).0,
            [Inline::text("[see "), reference("a", None, RefForm::Normal),]
        );
        assert_eq!(
            scanned("[not a reference]", true).0,
            [Inline::text("[not a reference]")]
        );
        let (inlines, _) = scanned("[@a @b]", true);
        assert!(
            !inlines.iter().any(|inline| matches!(inline, Inline::Cite { .. }))
        );
        assert_eq!(
            inlines
                .iter()
                .filter(|inline| matches!(inline, Inline::Ref { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn escaped_siblings_and_format_boundaries() {
        let parsed = parse_markdown(
            "\\@no \\@@also_no \\@ @yes **word**@no **word** @after\n@soft  \n@hard\n\n*@nested*",
            true,
        );
        assert_eq!(
            targets(&parsed.document),
            ["yes", "after", "soft", "hard", "nested"]
        );
        assert!(parsed.diagnostics.is_empty());
        let Block::Paragraph { inlines, .. } = &parsed.document.blocks[0]
        else {
            panic!()
        };
        assert!(ir::plain_text(inlines).contains("@no @@also_no @"));
    }

    #[test]
    fn groups_respect_escaped_and_formatted_sibling_boundaries() {
        for text in [r"\[@a]", r"\;@a", r"[see \@a]", r"[see \@@a]"] {
            let parsed = parse_markdown(text, true);
            assert!(targets(&parsed.document).is_empty(), "{text}");
            assert!(parsed.diagnostics.is_empty(), "{text}");
        }
        let parsed = parse_markdown(r"\*[@a]", true);
        assert_eq!(targets(&parsed.document), ["a"]);
        let mut groups = 0;
        ir::walk_inlines(&parsed.document, &mut |inline| {
            groups += usize::from(matches!(inline, Inline::Cite { .. }));
        });
        assert_eq!(groups, 1);

        for text in [r"[see \@ @a]", "[*see* @a]"] {
            let parsed = parse_markdown(text, true);
            assert_eq!(targets(&parsed.document), ["a"], "{text}");
            ir::walk_inlines(&parsed.document, &mut |inline| {
                assert!(!matches!(inline, Inline::Cite { .. }), "{text}");
            });
        }
    }

    #[test]
    fn links_code_math_and_raw_are_not_scanned() {
        let parsed = parse_markdown(
            "me@example.com [@inside](https://example.com) `@code` $@math$ <span data-x='@raw'> raw</span> @yes\n\n```text\n@fenced\n```\n\n```{=html}\n@raw_block\n```",
            true,
        );
        assert_eq!(targets(&parsed.document), ["yes"]);
        assert!(parsed.diagnostics.is_empty());
    }

    #[test]
    fn raw_inline_suffix_uses_the_same_scanner() {
        let parsed = parse_markdown("`raw`{=html} @after", true);
        assert_eq!(targets(&parsed.document), ["after"]);
        let parsed = parse_markdown("`raw`{=html}@attached", true);
        assert!(targets(&parsed.document).is_empty());
    }

    #[test]
    fn references_disabled_preserves_syntax_without_diagnostics() {
        let text = "@x[Fig.]{.bogus} [see @a; -@b]{.page}";
        let parsed = parse_markdown(text, false);
        assert!(targets(&parsed.document).is_empty());
        assert!(parsed.diagnostics.is_empty());
        let Block::Paragraph { inlines, .. } = &parsed.document.blocks[0]
        else {
            panic!()
        };
        assert_eq!(ir::plain_text(inlines), text);
    }

    #[test]
    fn scanner_reaches_headings_tables_captions_notes_and_title_fragments() {
        let parsed = parse_markdown(
            "# Heading @h\n\n| cell @t |\n|---|\n\n![caption @f](https://example.com/image.png)\n\nText[^n]\n\n[^n]: note @n",
            true,
        );
        let found = targets(&parsed.document);
        for target in ["h", "t", "f", "n"] {
            assert!(
                found.iter().any(|found| found == target),
                "{target}: {found:?}"
            );
        }
        assert!(parsed.diagnostics.is_empty());
    }
}
