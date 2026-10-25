//! Localized reference supplements (references.md §4.5).
//! Owner: T01. Stub created by T00.

use std::collections::BTreeMap;

use typst::text::{Lang, Region};

use crate::config::Config;
use crate::ir::{DocMeta, LabelKind};

/// A language and optional region used for localized document terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LangTag {
    /// Lowercase two- or three-letter language code, such as `de`.
    pub lang: String,
    /// Uppercase two-letter region code, such as `AT`.
    pub region: Option<String>,
}

impl LangTag {
    /// Parses the language and region, skipping an optional four-letter script.
    pub fn parse(tag: &str) -> Option<LangTag> {
        let mut subtags = tag.split(['-', '_']);
        let lang = subtags.next()?;
        if !(2..=3).contains(&lang.len())
            || !lang.bytes().all(|byte| byte.is_ascii_alphabetic())
        {
            return None;
        }
        let mut region = subtags.next();
        if region.is_some_and(|script| {
            script.len() == 4
                && script.bytes().all(|byte| byte.is_ascii_alphabetic())
        }) {
            region = subtags.next();
        }
        let region = region
            .filter(|region| {
                region.len() == 2
                    && region.bytes().all(|byte| byte.is_ascii_alphabetic())
            })
            .map(str::to_ascii_uppercase);
        Some(LangTag { lang: lang.to_ascii_lowercase(), region })
    }

    /// Uses metadata, then configuration, then English; invalid present tags fail.
    pub fn effective(meta: &DocMeta, config: &Config) -> Option<LangTag> {
        let tag = meta
            .lang
            .as_deref()
            .filter(|tag| !tag.is_empty())
            .or_else(|| {
                (!config.document.lang.is_empty())
                    .then_some(config.document.lang.as_str())
            })
            .unwrap_or("en");
        Self::parse(tag)
    }

    /// The locale code, such as `de` or `de-AT`.
    pub fn locale_code(&self) -> String {
        match &self.region {
            Some(region) => format!("{}-{region}", self.lang),
            None => self.lang.clone(),
        }
    }
}

/// A reference supplement, document heading or callout title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Term {
    /// Figure reference supplement.
    Figure,
    /// Table reference supplement.
    Table,
    /// Code listing reference supplement.
    Listing,
    /// Equation reference supplement.
    Equation,
    /// Section reference supplement.
    Section,
    /// Footnote reference supplement.
    Footnote,
    /// Bibliography heading.
    Bibliography,
    /// Table of contents heading.
    Contents,
    /// Page reference supplement.
    Page,
    /// Abstract heading.
    Abstract,
    /// Note callout title.
    Note,
    /// Tip callout title.
    Tip,
    /// Important callout title.
    Important,
    /// Warning callout title.
    Warning,
    /// Caution callout title.
    Caution,
}

/// Looks up typst's document terms or writer's localized callout titles.
/// Missing, unreadable or malformed writer locale files fall back to English.
pub fn term(lang: &LangTag, term: Term) -> String {
    let key = match term {
        Term::Figure => "figure",
        Term::Table => "table",
        Term::Listing => "raw",
        Term::Equation => "equation",
        Term::Section => "heading",
        Term::Footnote => "footnote",
        Term::Bibliography => "bibliography",
        Term::Contents => "outline",
        Term::Page => "page",
        Term::Abstract => return writer_term(lang, "abstract", "Abstract"),
        Term::Note => return writer_term(lang, "note", "Note"),
        Term::Tip => return writer_term(lang, "tip", "Tip"),
        Term::Important => return writer_term(lang, "important", "Important"),
        Term::Warning => return writer_term(lang, "warning", "Warning"),
        Term::Caution => return writer_term(lang, "caution", "Caution"),
    };
    let language = lang.lang.parse::<Lang>().unwrap_or(Lang::ENGLISH);
    let region =
        lang.region.as_deref().and_then(|region| region.parse::<Region>().ok());
    typst::text::localized_str(language, region, key).to_string()
}

fn writer_term(lang: &LangTag, key: &str, english: &str) -> String {
    let mut paths = Vec::new();
    if lang.region.is_some() {
        paths.push(format!("l10n/{}.toml", lang.locale_code()));
    }
    paths.push(format!("l10n/{}.toml", lang.lang));
    paths.push("l10n/en.toml".to_string());
    for path in paths {
        let Some(bytes) = crate::vendor::get(&path) else {
            continue;
        };
        let Some(terms) = std::str::from_utf8(&bytes).ok().and_then(|text| {
            toml::from_str::<BTreeMap<String, String>>(text).ok()
        }) else {
            continue;
        };
        if let Some(value) = terms.get(key) {
            return value.clone();
        }
    }
    english.to_string()
}

/// The localized reference supplement for a label kind; other labels have none.
pub fn supplement(kind: LabelKind, lang: &LangTag) -> String {
    let term_kind = match kind {
        LabelKind::Heading => Term::Section,
        LabelKind::Figure => Term::Figure,
        LabelKind::Table => Term::Table,
        LabelKind::Listing => Term::Listing,
        LabelKind::Equation => Term::Equation,
        LabelKind::Footnote => Term::Footnote,
        LabelKind::Other => return String::new(),
    };
    term(lang, term_kind)
}

/// The separator between a numbered caption label and its caption, as in typst.
pub fn caption_separator(lang: &LangTag) -> &'static str {
    match lang.lang.as_str() {
        "zh" => "\u{2003}",
        "fr" => ".\u{a0}\u{2013} ",
        "ru" => ". ",
        _ => ": ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang(tag: &str) -> LangTag {
        LangTag::parse(tag).unwrap()
    }

    #[test]
    fn lang_tag_parse_cases() {
        for (tag, language, region) in [
            ("de-AT", "de", Some("AT")),
            ("zh-Hans-CN", "zh", Some("CN")),
            ("en_us", "en", Some("US")),
            ("EN", "en", None),
            ("de-419", "de", None),
            ("eng", "eng", None),
            ("sr-Latn", "sr", None),
            ("fr-CA-extra", "fr", Some("CA")),
        ] {
            let parsed = lang(tag);
            assert_eq!(parsed.lang, language, "{tag}");
            assert_eq!(parsed.region.as_deref(), region, "{tag}");
            let expected = region.map_or_else(
                || language.to_string(),
                |region| format!("{language}-{region}"),
            );
            assert_eq!(parsed.locale_code(), expected);
        }
        for invalid in ["", "x", "123", "english", "éé", " en", "-en"] {
            assert!(LangTag::parse(invalid).is_none(), "{invalid:?}");
        }
    }

    #[test]
    fn effective_falls_back_meta_config_en() {
        let mut meta = DocMeta::default();
        let mut config = Config::default();
        config.document.lang = "de".into();
        assert_eq!(LangTag::effective(&meta, &config), Some(lang("de")));
        meta.lang = Some("fr-CA".into());
        assert_eq!(LangTag::effective(&meta, &config), Some(lang("fr-CA")));
        meta.lang = Some(String::new());
        assert_eq!(LangTag::effective(&meta, &config), Some(lang("de")));
        config.document.lang.clear();
        assert_eq!(LangTag::effective(&meta, &config), Some(lang("en")));
        meta.lang = Some("x".into());
        assert_eq!(LangTag::effective(&meta, &config), None);
        meta.lang = None;
        config.document.lang = "invalid".into();
        assert_eq!(LangTag::effective(&meta, &config), None);
    }

    #[test]
    fn term_en_de_fr_ca() {
        for (tag, expected) in [
            ("en", "Figure"),
            ("de", "Abbildung"),
            ("fr", "Fig."),
            ("fr-CA", "Fig."),
        ] {
            assert_eq!(term(&lang(tag), Term::Figure), expected);
        }
        assert_eq!(term(&lang("de"), Term::Section), "Abschnitt");
        assert_eq!(term(&lang("en"), Term::Contents), "Contents");
        assert_eq!(term(&lang("en"), Term::Page), "page");
        assert_eq!(term(&lang("de"), Term::Note), "Note");
        assert_eq!(term(&lang("zz-ZZ"), Term::Figure), "Figure");
        assert_eq!(term(&lang("zz-ZZ"), Term::Caution), "Caution");
    }

    #[test]
    fn supplement_per_kind() {
        let language = lang("en");
        for (kind, expected) in [
            (LabelKind::Heading, "Section"),
            (LabelKind::Figure, "Figure"),
            (LabelKind::Table, "Table"),
            (LabelKind::Listing, "Listing"),
            (LabelKind::Equation, "Equation"),
            (LabelKind::Footnote, "Footnote"),
            (LabelKind::Other, ""),
        ] {
            assert_eq!(supplement(kind, &language), expected);
        }
    }

    #[test]
    fn caption_separator_cases() {
        for (tag, expected) in [
            ("zh", "\u{2003}"),
            ("fr", ".\u{a0}– "),
            ("fr-CA", ".\u{a0}– "),
            ("ru", ". "),
            ("en", ": "),
        ] {
            assert_eq!(caption_separator(&lang(tag)), expected);
        }
    }

    #[test]
    fn every_l10n_file_parses_and_en_has_every_writer_key() {
        let files = crate::vendor::list("l10n/");
        assert!(files.iter().any(|path| path == "l10n/en.toml"));
        for path in files {
            let text = crate::vendor::get_str(&path).unwrap();
            let terms: BTreeMap<String, String> =
                toml::from_str(&text).unwrap();
            if path == "l10n/en.toml" {
                assert_eq!(terms.len(), 6);
                for (key, expected) in [
                    ("abstract", "Abstract"),
                    ("note", "Note"),
                    ("tip", "Tip"),
                    ("important", "Important"),
                    ("warning", "Warning"),
                    ("caution", "Caution"),
                ] {
                    assert_eq!(
                        terms.get(key).map(String::as_str),
                        Some(expected)
                    );
                }
            }
        }
    }
}
