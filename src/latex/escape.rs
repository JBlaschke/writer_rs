//! LaTeX text, inline-code, URL and highlighted-code escaping (§5).
//!
//! The prose math map and pdflatex-safe set are the exact frozen tables in
//! latex-output.md. Math-source conversion has its own, larger T51 table.

use std::borrow::Cow;

/// Escapes prose, optionally mapping §5.5 symbols into explicit math macros.
pub fn text(s: &str, map: bool) -> Cow<'_, str> {
    replace(s, |current, previous, next| {
        if let Some(value) = special(current, next) {
            return Replacement::Literal(value);
        }
        let value = match current {
            '?' if next == Some('`') => Some(r"?{\kern0pt}"),
            '!' if next == Some('`') => Some(r"!{\kern0pt}"),
            '\u{a0}' => Some("~"),
            '\u{ad}' => Some(r"\-"),
            '\u{200b}' => Some(r"\hspace{0pt}"),
            '\u{202f}' => Some(r"\,"),
            '…' => Some(r"\ldots{}"),
            '‘' => Some("`"),
            '’' => Some("'"),
            '“' => Some("``"),
            '”' => Some("''"),
            '—' => Some("---"),
            '–' => Some("--"),
            _ => None,
        };
        if let Some(value) = value {
            if smart_quote(current) && previous.is_some_and(smart_quote) {
                Replacement::AdjacentQuote(value)
            } else {
                Replacement::Literal(value)
            }
        } else if map && let Some(value) = map_char(current) {
            Replacement::Math(value)
        } else {
            Replacement::Keep
        }
    })
}

/// Escapes a texttt argument, preserving code spaces and Unicode punctuation.
pub fn code(s: &str, map: bool) -> Cow<'_, str> {
    replace(s, |current, _, next| {
        let value = match current {
            '\'' => Some(r"\textquotesingle{}"),
            '`' => Some(r"\textasciigrave{}"),
            ' ' => Some(r"\ "),
            _ => special(current, next),
        };
        if let Some(value) = value {
            Replacement::Literal(value)
        } else if map && let Some(value) = map_char(current) {
            Replacement::Math(value)
        } else {
            Replacement::Keep
        }
    })
}

/// Escapes a hyperref URL, including uppercase UTF-8 byte percent encoding.
pub fn url(s: &str) -> Cow<'_, str> {
    replace(s, |current, _, _| {
        let value = match current {
            '%' => Some(r"\%"),
            '#' => Some(r"\#"),
            '\\' => Some(r"\%5C"),
            '{' => Some(r"\%7B"),
            '}' => Some(r"\%7D"),
            '^' => Some(r"\%5E"),
            '|' => Some(r"\%7C"),
            '[' => Some(r"\%5B"),
            ']' => Some(r"\%5D"),
            '`' => Some(r"\%60"),
            ' ' => Some(r"\%20"),
            '"' => Some(r"\%22"),
            '<' => Some(r"\%3C"),
            '>' => Some(r"\%3E"),
            _ => None,
        };
        if let Some(value) = value {
            Replacement::Literal(value)
        } else if !current.is_ascii() {
            Replacement::Percent(current)
        } else {
            Replacement::Keep
        }
    })
}

/// Escapes only commandchars in a writercode token, leaving all else verbatim.
pub fn writercode(s: &str) -> Cow<'_, str> {
    replace(s, |current, _, _| match current {
        '\\' => Replacement::Literal(r"\textbackslash{}"),
        '{' => Replacement::Literal(r"\{"),
        '}' => Replacement::Literal(r"\}"),
        _ => Replacement::Keep,
    })
}

/// Looks up a §5.5 prose symbol's math macro, without an ensuremath wrapper.
pub fn map_char(c: char) -> Option<&'static str> {
    TEXT_MATH
        .binary_search_by_key(&c, |&(symbol, _)| symbol)
        .ok()
        .map(|index| TEXT_MATH[index].1)
}

/// Whether ASCII or the exact §5.6 T1/inputenc/textcomp set accepts a character.
pub fn pdflatex_safe(c: char) -> bool {
    if c.is_ascii() {
        return true;
    }
    let code = c as u32;
    let index = PDFLATEX_SAFE.partition_point(|&(_, end)| end < code);
    PDFLATEX_SAFE.get(index).is_some_and(|&(start, _)| start <= code)
}

fn special(current: char, next: Option<char>) -> Option<&'static str> {
    Some(match current {
        '\\' => r"\textbackslash{}",
        '{' => r"\{",
        '}' => r"\}",
        '$' => r"\$",
        '%' => r"\%",
        '&' => r"\&",
        '_' => r"\_",
        '#' => r"\#",
        '~' => r"\textasciitilde{}",
        '^' => r"\textasciicircum{}",
        '|' => r"\textbar{}",
        '<' => r"\textless{}",
        '>' => r"\textgreater{}",
        '[' => "{[}",
        ']' => "{]}",
        '-' if next == Some('-') => r"-\/",
        _ => return None,
    })
}

fn smart_quote(c: char) -> bool {
    matches!(c, '‘' | '’' | '“' | '”')
}

enum Replacement {
    Keep,
    Literal(&'static str),
    AdjacentQuote(&'static str),
    Math(&'static str),
    Percent(char),
}

impl Replacement {
    fn append(self, output: &mut String) {
        match self {
            Self::Keep => {}
            Self::Literal(value) => output.push_str(value),
            Self::AdjacentQuote(value) => {
                output.push_str(r"\,");
                output.push_str(value);
            }
            Self::Math(value) => {
                output.push_str(r"\ensuremath{");
                output.push_str(value);
                output.push('}');
            }
            Self::Percent(c) => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                let mut buffer = [0; 4];
                for byte in c.encode_utf8(&mut buffer).bytes() {
                    output.push_str(r"\%");
                    output.push(HEX[(byte >> 4) as usize] as char);
                    output.push(HEX[(byte & 0xf) as usize] as char);
                }
            }
        }
    }
}

fn replace(
    s: &str,
    mut replacement: impl FnMut(char, Option<char>, Option<char>) -> Replacement,
) -> Cow<'_, str> {
    let mut output: Option<String> = None;
    let mut chars = s.char_indices().peekable();
    let mut previous = None;
    while let Some((index, current)) = chars.next() {
        let next = chars.peek().map(|&(_, next)| next);
        match replacement(current, previous, next) {
            Replacement::Keep => {
                if let Some(output) = &mut output {
                    output.push(current);
                }
            }
            changed => {
                let output = output.get_or_insert_with(|| {
                    let mut output = String::with_capacity(s.len());
                    output.push_str(&s[..index]);
                    output
                });
                changed.append(output);
            }
        }
        previous = Some(current);
    }
    output.map_or(Cow::Borrowed(s), Cow::Owned)
}

// Sorted by scalar value; every row is latex-output.md §5.5, not T51's map.
const TEXT_MATH: &[(char, &str)] = &[
    ('Α', r"\mathrm{A}"),
    ('Β', r"\mathrm{B}"),
    ('Γ', r"\Gamma"),
    ('Δ', r"\Delta"),
    ('Ε', r"\mathrm{E}"),
    ('Ζ', r"\mathrm{Z}"),
    ('Η', r"\mathrm{H}"),
    ('Θ', r"\Theta"),
    ('Ι', r"\mathrm{I}"),
    ('Κ', r"\mathrm{K}"),
    ('Λ', r"\Lambda"),
    ('Μ', r"\mathrm{M}"),
    ('Ν', r"\mathrm{N}"),
    ('Ξ', r"\Xi"),
    ('Ο', r"\mathrm{O}"),
    ('Π', r"\Pi"),
    ('Ρ', r"\mathrm{P}"),
    ('Σ', r"\Sigma"),
    ('Τ', r"\mathrm{T}"),
    ('Υ', r"\Upsilon"),
    ('Φ', r"\Phi"),
    ('Χ', r"\mathrm{X}"),
    ('Ψ', r"\Psi"),
    ('Ω', r"\Omega"),
    ('α', r"\alpha"),
    ('β', r"\beta"),
    ('γ', r"\gamma"),
    ('δ', r"\delta"),
    ('ε', r"\varepsilon"),
    ('ζ', r"\zeta"),
    ('η', r"\eta"),
    ('θ', r"\theta"),
    ('ι', r"\iota"),
    ('κ', r"\kappa"),
    ('λ', r"\lambda"),
    ('μ', r"\mu"),
    ('ν', r"\nu"),
    ('ξ', r"\xi"),
    ('ο', r"o"),
    ('π', r"\pi"),
    ('ρ', r"\rho"),
    ('ς', r"\varsigma"),
    ('σ', r"\sigma"),
    ('τ', r"\tau"),
    ('υ', r"\upsilon"),
    ('φ', r"\varphi"),
    ('χ', r"\chi"),
    ('ψ', r"\psi"),
    ('ω', r"\omega"),
    ('ϑ', r"\vartheta"),
    ('ϕ', r"\phi"),
    ('ϖ', r"\varpi"),
    ('ϰ', r"\varkappa"),
    ('ϱ', r"\varrho"),
    ('ϵ', r"\epsilon"),
    ('′', r"{}^{\prime}"),
    ('″', r"{}^{\prime\prime}"),
    ('‴', r"{}^{\prime\prime\prime}"),
    ('ℂ', r"\mathbb{C}"),
    ('ℏ', r"\hbar"),
    ('ℓ', r"\ell"),
    ('ℕ', r"\mathbb{N}"),
    ('℘', r"\wp"),
    ('ℚ', r"\mathbb{Q}"),
    ('ℝ', r"\mathbb{R}"),
    ('ℤ', r"\mathbb{Z}"),
    ('ℵ', r"\aleph"),
    ('⅓', r"\tfrac{1}{3}"),
    ('⅔', r"\tfrac{2}{3}"),
    ('⅛', r"\tfrac{1}{8}"),
    ('↔', r"\leftrightarrow"),
    ('↕', r"\updownarrow"),
    ('↦', r"\mapsto"),
    ('⇐', r"\Leftarrow"),
    ('⇑', r"\Uparrow"),
    ('⇒', r"\Rightarrow"),
    ('⇓', r"\Downarrow"),
    ('⇔', r"\Leftrightarrow"),
    ('∀', r"\forall"),
    ('∂', r"\partial"),
    ('∃', r"\exists"),
    ('∄', r"\nexists"),
    ('∅', r"\emptyset"),
    ('∇', r"\nabla"),
    ('∈', r"\in"),
    ('∉', r"\notin"),
    ('∋', r"\ni"),
    ('∏', r"\prod"),
    ('∑', r"\sum"),
    ('−', r"-"),
    ('∓', r"\mp"),
    ('∗', r"\ast"),
    ('∘', r"\circ"),
    ('∙', r"\bullet"),
    ('√', r"\surd"),
    ('∝', r"\propto"),
    ('∞', r"\infty"),
    ('∠', r"\angle"),
    ('∣', r"\mid"),
    ('∥', r"\parallel"),
    ('∧', r"\wedge"),
    ('∨', r"\vee"),
    ('∩', r"\cap"),
    ('∪', r"\cup"),
    ('∫', r"\int"),
    ('∮', r"\oint"),
    ('∴', r"\therefore"),
    ('∼', r"\sim"),
    ('≃', r"\simeq"),
    ('≅', r"\cong"),
    ('≈', r"\approx"),
    ('≠', r"\neq"),
    ('≡', r"\equiv"),
    ('≤', r"\leq"),
    ('≥', r"\geq"),
    ('≪', r"\ll"),
    ('≫', r"\gg"),
    ('⊂', r"\subset"),
    ('⊃', r"\supset"),
    ('⊆', r"\subseteq"),
    ('⊇', r"\supseteq"),
    ('⊕', r"\oplus"),
    ('⊗', r"\otimes"),
    ('⊥', r"\perp"),
    ('⋅', r"\cdot"),
    ('⋯', r"\cdots"),
    ('⌈', r"\lceil"),
    ('⌉', r"\rceil"),
    ('⌊', r"\lfloor"),
    ('⌋', r"\rfloor"),
    ('□', r"\square"),
    ('✓', r"\checkmark"),
    ('⟶', r"\longrightarrow"),
    ('⟹', r"\Longrightarrow"),
];

// Inclusive scalar ranges from latex-output.md §5.6 (349 non-ASCII values).
const PDFLATEX_SAFE: &[(u32, u32)] = &[
    (0x00a0, 0x0125),
    (0x0128, 0x0137),
    (0x0139, 0x013e),
    (0x0141, 0x0148),
    (0x014a, 0x0165),
    (0x0168, 0x017e),
    (0x0192, 0x0192),
    (0x01c4, 0x01d4),
    (0x01e2, 0x01e3),
    (0x01e6, 0x01eb),
    (0x01f0, 0x01f0),
    (0x01f4, 0x01f5),
    (0x0218, 0x021b),
    (0x0232, 0x0233),
    (0x0237, 0x0237),
    (0x02c6, 0x02c7),
    (0x02d8, 0x02d9),
    (0x02db, 0x02dd),
    (0x0e3f, 0x0e3f),
    (0x1e02, 0x1e03),
    (0x1e0d, 0x1e0d),
    (0x1e1e, 0x1e21),
    (0x1e25, 0x1e25),
    (0x1e30, 0x1e31),
    (0x1e37, 0x1e37),
    (0x1e43, 0x1e43),
    (0x1e45, 0x1e45),
    (0x1e47, 0x1e47),
    (0x1e5b, 0x1e5b),
    (0x1e63, 0x1e63),
    (0x1e6d, 0x1e6d),
    (0x1e8e, 0x1e91),
    (0x1e9e, 0x1e9e),
    (0x1ef2, 0x1ef3),
    (0x200c, 0x200c),
    (0x2010, 0x2016),
    (0x2018, 0x201a),
    (0x201c, 0x201e),
    (0x2020, 0x2022),
    (0x2026, 0x2026),
    (0x2030, 0x2031),
    (0x2039, 0x203b),
    (0x203d, 0x203d),
    (0x2044, 0x2044),
    (0x204e, 0x204e),
    (0x2052, 0x2052),
    (0x20a1, 0x20a1),
    (0x20a4, 0x20a4),
    (0x20a6, 0x20a6),
    (0x20a9, 0x20a9),
    (0x20ab, 0x20ac),
    (0x20b1, 0x20b1),
    (0x2103, 0x2103),
    (0x2116, 0x2117),
    (0x211e, 0x211e),
    (0x2120, 0x2120),
    (0x2122, 0x2122),
    (0x2126, 0x2127),
    (0x212e, 0x212e),
    (0x2190, 0x2193),
    (0x2329, 0x232a),
    (0x2422, 0x2423),
    (0x25e6, 0x25e6),
    (0x25ef, 0x25ef),
    (0x266a, 0x266a),
    (0x27e8, 0x27e9),
    (0x3008, 0x3009),
    (0xfb00, 0xfb06),
    (0xfeff, 0xfeff),
];

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = include_str!("../../docs/spec/latex-output.md");

    fn table(section: &str) -> &str {
        SPEC.split_once(section)
            .unwrap()
            .1
            .split_once("```")
            .unwrap()
            .1
            .split_once("```")
            .unwrap()
            .0
    }

    fn prose_map() -> Vec<(char, String)> {
        table("### 5.5")
            .lines()
            .flat_map(|line| line.split(" · "))
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(|entry| {
                let mut fields = entry.splitn(3, ' ');
                let code =
                    u32::from_str_radix(fields.next().unwrap(), 16).unwrap();
                let displayed = fields.next().unwrap();
                let character = char::from_u32(code).unwrap();
                assert_eq!(displayed, character.to_string());
                (character, fields.next().unwrap().to_string())
            })
            .collect()
    }

    fn safe_ranges() -> Vec<(u32, u32)> {
        table("### 5.6")
            .split_whitespace()
            .map(|token| {
                let (first, last) =
                    token.split_once('-').unwrap_or((token, token));
                (
                    u32::from_str_radix(first, 16).unwrap(),
                    u32::from_str_radix(last, 16).unwrap(),
                )
            })
            .collect()
    }

    fn borrowed(actual: Cow<'_, str>, original: &str) {
        let Cow::Borrowed(value) = actual else {
            panic!("unchanged input allocated: {original:?}");
        };
        assert!(std::ptr::eq(value, original));
    }

    #[test]
    fn unchanged_inputs_stay_borrowed_in_every_context() {
        for source in ["", "plain café 😀 日本", "line\nnext\tend\r\n\0"] {
            borrowed(text(source, false), source);
            borrowed(text(source, true), source);
        }
        for source in ["", "abcXYZ09", "café😀日本", "\t\r\n", "‘’“”—–…"]
        {
            borrowed(code(source, false), source);
            borrowed(code(source, true), source);
        }
        let source = "https://example.test/a-b?q=_~&x=1:2/3.,;()!@$'";
        borrowed(url(source), source);
        let source = "$%&_#~^|<>[]'`\" café α 😀\n\t";
        borrowed(writercode(source), source);
    }

    #[test]
    fn text_and_code_escape_every_shared_special_without_reescaping_generated_tex()
     {
        for (source, expected) in [
            ("\\", r"\textbackslash{}"),
            ("{", r"\{"),
            ("}", r"\}"),
            ("$", r"\$"),
            ("%", r"\%"),
            ("&", r"\&"),
            ("_", r"\_"),
            ("#", r"\#"),
            ("~", r"\textasciitilde{}"),
            ("^", r"\textasciicircum{}"),
            ("|", r"\textbar{}"),
            ("<", r"\textless{}"),
            (">", r"\textgreater{}"),
            ("[", "{[}"),
            ("]", "{]}"),
            ("--", r"-\/-"),
            ("---", r"-\/-\/-"),
        ] {
            for map in [false, true] {
                assert_eq!(text(source, map), expected, "{source:?}");
                assert_eq!(code(source, map), expected, "{source:?}");
            }
        }
        assert_eq!(text("\\{}alpha", false), r"\textbackslash{}\{\}alpha");
        assert_eq!(code("\\{}alpha", false), r"\textbackslash{}\{\}alpha");
    }

    #[test]
    fn text_maps_only_its_declared_unicode_punctuation() {
        for (source, expected) in [
            ("\u{a0}", "~"),
            ("\u{ad}", r"\-"),
            ("\u{200b}", r"\hspace{0pt}"),
            ("\u{202f}", r"\,"),
            ("…", r"\ldots{}"),
            ("—", "---"),
            ("–", "--"),
            ("‘", "`"),
            ("’", "'"),
            ("“", "``"),
            ("”", "''"),
        ] {
            assert_eq!(text(source, false), expected);
            assert_eq!(text(source, true), expected);
        }
        let source = "\"literal\" 'single' `grave`";
        borrowed(text(source, false), source);
        borrowed(text(source, true), source);
    }

    #[test]
    fn adjacent_mapped_quotes_separate_at_source_boundaries_only() {
        for (first, left) in [('‘', "`"), ('’', "'"), ('“', "``"), ('”', "''")]
        {
            for (second, right) in
                [('‘', "`"), ('’', "'"), ('“', "``"), ('”', "''")]
            {
                let source = format!("{first}{second}");
                assert_eq!(text(&source, false), format!("{left}\\,{right}"));
            }
        }
        assert_eq!(text("‘“”’", false), r"`\,``\,''\,'");
        assert_eq!(text("‘ ’", false), "` '");
        assert_eq!(text("'‘`“", false), "'````");
    }

    #[test]
    fn text_ligature_guards_do_not_reprocess_mapped_output() {
        assert_eq!(text("?` !`", false), r"?{\kern0pt}` !{\kern0pt}`");
        assert_eq!(text("?‘ !‘", false), "?` !`");
        assert_eq!(text("-—–", false), "------");
        assert_eq!(text("—--", false), r"----\/-");
        assert_eq!(text("?`", true), r"?{\kern0pt}`");
    }

    #[test]
    fn code_preserves_unicode_punctuation_and_non_ascii_spaces() {
        let source = "‘’“”—–…\u{a0}\u{ad}\u{200b}\u{202f}";
        borrowed(code(source, false), source);
        borrowed(code(source, true), source);
        assert_eq!(
            code("'` a  b", false),
            r"\textquotesingle{}\textasciigrave{}\ a\ \ b"
        );
        assert_eq!(
            code("?` !`", false),
            r"?\textasciigrave{}\ !\textasciigrave{}"
        );
        let source = "\t\n\r\n";
        borrowed(code(source, false), source);
    }

    #[test]
    fn url_escapes_each_reserved_ascii_character_once() {
        for (source, expected) in [
            ("%", r"\%"),
            ("#", r"\#"),
            ("\\", r"\%5C"),
            ("{", r"\%7B"),
            ("}", r"\%7D"),
            ("^", r"\%5E"),
            ("|", r"\%7C"),
            ("[", r"\%5B"),
            ("]", r"\%5D"),
            ("`", r"\%60"),
            (" ", r"\%20"),
            ("\"", r"\%22"),
            ("<", r"\%3C"),
            (">", r"\%3E"),
        ] {
            assert_eq!(url(source), expected);
        }
        assert_eq!(url("a%20b#part{c}"), r"a\%20b\#part\%7Bc\%7D");
        let source = "_&~?=:/.-'\t\n\r";
        borrowed(url(source), source);
    }

    #[test]
    fn url_percent_encoding_uses_utf8_bytes_and_uppercase_hex() {
        for (source, expected) in [
            ("é", r"\%C3\%A9"),
            ("α", r"\%CE\%B1"),
            ("日", r"\%E6\%97\%A5"),
            ("😀", r"\%F0\%9F\%98\%80"),
            ("\u{a0}", r"\%C2\%A0"),
            ("\u{10ffff}", r"\%F4\%8F\%BF\%BF"),
        ] {
            assert_eq!(url(source), expected);
        }
        assert_eq!(
            url("café/α?q=😀#x"),
            r"caf\%C3\%A9/\%CE\%B1?q=\%F0\%9F\%98\%80\#x"
        );
    }

    #[test]
    fn writercode_has_only_three_escaped_characters() {
        assert_eq!(writercode(r"\{a}"), r"\textbackslash{}\{a\}");
        let untouched = "[]'`\" $%&_#~^|<> α — 😀\n\r\t";
        borrowed(writercode(untouched), untouched);
        assert_eq!(writercode("α\\β{γ}\n"), "α\\textbackslash{}β\\{γ\\}\n",);
    }

    #[test]
    fn prose_mapping_matches_all_and_only_the_frozen_134_rows() {
        let expected = prose_map();
        assert_eq!(expected.len(), 134);
        let actual = TEXT_MATH
            .iter()
            .map(|&(character, value)| (character, value.to_string()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert!(TEXT_MATH.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (character, value) in expected {
            assert_eq!(map_char(character), Some(value.as_str()));
            assert!(!pdflatex_safe(character), "{character}");
            let source = character.to_string();
            let escaped = format!("\\ensuremath{{{value}}}");
            assert_eq!(text(&source, true), escaped);
            assert_eq!(code(&source, true), escaped);
            borrowed(text(&source, false), &source);
            borrowed(code(&source, false), &source);
        }
        for character in
            ['a', 'é', '→', '×', '±', '½', '€', '°', 'µ', '⟨', '⟩', '日', '😀']
        {
            assert_eq!(map_char(character), None, "{character}");
        }
        assert_eq!(map_char('\u{10ffff}'), None);
    }

    #[test]
    fn mapped_symbols_never_consume_adjacent_source_letters_or_specials() {
        assert_eq!(
            text("aαz ≥_x", true),
            r"a\ensuremath{\alpha}z \ensuremath{\geq}\_x"
        );
        assert_eq!(
            code("aαz ≥_x", true),
            r"a\ensuremath{\alpha}z\ \ensuremath{\geq}\_x"
        );
        assert_eq!(text("ο−", true), r"\ensuremath{o}\ensuremath{-}");
        assert_eq!(
            code("′″‴", true),
            r"\ensuremath{{}^{\prime}}\ensuremath{{}^{\prime\prime}}\ensuremath{{}^{\prime\prime\prime}}"
        );
    }

    #[test]
    fn pdflatex_safe_matches_the_exact_349_values_and_all_range_boundaries() {
        let expected = safe_ranges();
        assert_eq!(PDFLATEX_SAFE, expected.as_slice());
        assert!(expected.windows(2).all(|pair| pair[0].1 < pair[1].0));
        let count: u32 =
            expected.iter().map(|&(start, end)| end - start + 1).sum();
        assert_eq!(count, 349);
        for value in 0..=127 {
            assert!(pdflatex_safe(char::from_u32(value).unwrap()));
        }
        for &(start, end) in &expected {
            for value in start..=end {
                assert!(pdflatex_safe(char::from_u32(value).unwrap()));
            }
            for value in [start - 1, end + 1] {
                let permitted = value <= 127
                    || expected
                        .iter()
                        .any(|&(first, last)| (first..=last).contains(&value));
                assert_eq!(
                    pdflatex_safe(char::from_u32(value).unwrap()),
                    permitted,
                    "U+{value:04X}",
                );
            }
        }
        for character in [
            '−', '≥', 'α', '√', '日', '😀', '\u{80}', '\u{10000}', '\u{10ffff}',
        ] {
            assert!(!pdflatex_safe(character), "{character}");
        }
    }
}
