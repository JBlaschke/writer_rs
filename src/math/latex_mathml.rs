//! Deterministic LaTeX-to-MathML conversion with shared macro configuration.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::{Arc, Mutex, PoisonError};

use math_core::{LatexToMathML, MathCoreConfig, MathDisplay, PrettyPrint};

use crate::error::Diagnostic;

type CachedConverter =
    (BTreeMap<String, String>, Arc<Converter>, Vec<Diagnostic>);

static CACHE: Mutex<Option<CachedConverter>> = Mutex::new(None);

/// Convert prepared LaTeX source using the converter for the supplied macros.
///
/// Macro names retain their leading backslashes. The cached converter is
/// rebuilt when that map changes, and each equation has independent state.
pub fn to_mathml(
    source: &str, display: bool, macros: &BTreeMap<String, String>,
) -> Result<String, MathError> {
    let (converter, _) = cached_converter(macros);
    converter.to_mathml(source, display)
}

/// Return warnings for invalid macro names or replacements in the current map.
///
/// Call once per document when using math-core. Findings have neither source
/// paths nor spans; callers may add the enclosing equation's location.
pub fn macro_diagnostics(macros: &BTreeMap<String, String>) -> Vec<Diagnostic> {
    cached_converter(macros).1
}

fn cached_converter(
    macros: &BTreeMap<String, String>,
) -> (Arc<Converter>, Vec<Diagnostic>) {
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((previous, converter, diagnostics)) = cache.as_ref()
        && previous == macros
    {
        return (Arc::clone(converter), diagnostics.clone());
    }
    let (converter, diagnostics) = Converter::new(macros);
    let converter = Arc::new(converter);
    *cache =
        Some((macros.clone(), Arc::clone(&converter), diagnostics.clone()));
    (converter, diagnostics)
}

/// A reusable converter with fixed macros and no state shared by equations.
#[derive(Debug)]
pub struct Converter {
    inner: LatexToMathML,
}

impl Converter {
    /// Build a converter, skipping invalid macro definitions with warnings.
    ///
    /// Names must start with `\`; after removing it, math-core accepts one
    /// character or ASCII letters. A bad replacement is removed and the
    /// remaining definitions are retried. Warnings follow lexical key order.
    pub fn new(
        macros: &BTreeMap<String, String>,
    ) -> (Converter, Vec<Diagnostic>) {
        let mut definitions = Vec::new();
        let mut diagnostics = BTreeMap::new();
        for (key, body) in macros {
            match key.strip_prefix('\\') {
                Some(name) if valid_macro_name(name) => {
                    definitions.push((name.to_owned(), body.clone()));
                }
                _ => {
                    diagnostics.insert(key.clone(), invalid_macro(
                        key, "expected a backslash, then one character or ASCII letters",
                    ));
                }
            }
        }
        let config = MathCoreConfig {
            pretty_print: PrettyPrint::Never,
            xml_namespace: true,
            ignore_unknown_commands: false,
            annotation: false,
            global_group: false,
            allow_unreliable_rendering: false,
            ..MathCoreConfig::default()
        };
        loop {
            match LatexToMathML::new(config.clone(), definitions.clone()) {
                Ok(inner) => {
                    return (
                        Converter { inner },
                        diagnostics.into_values().collect(),
                    );
                }
                Err((error, index, _)) => {
                    // math-core returns an index into these definitions;
                    // constructing a converter with no definitions succeeds.
                    let (name, _) = definitions.remove(index);
                    let key = format!("\\{name}");
                    diagnostics.insert(
                        key.clone(),
                        invalid_macro(&key, &error.error_message()),
                    );
                }
            }
        }
    }

    /// Convert prepared LaTeX source to compact, namespace-qualified MathML.
    ///
    /// Display equations receive `display="block"`. HTML-only named entities
    /// are replaced with XML-compatible numeric entities. A syntax failure
    /// includes its source byte range and an escaped inline HTML fallback.
    pub fn to_mathml(
        &self, source: &str, display: bool,
    ) -> Result<String, MathError> {
        let display = if display {
            MathDisplay::Block
        } else {
            MathDisplay::Inline
        };
        self.inner
            .convert_with_local_state(source, display)
            .map(|result| xml_safe_mathml(&result.mathml))
            .map_err(|error| MathError::Syntax {
                message: error.error_message(),
                range: Some(error.0.clone()),
                html: error.to_html(
                    source,
                    MathDisplay::Inline,
                    Some("math-error"),
                ),
            })
    }
}

fn valid_macro_name(name: &str) -> bool {
    !name.is_empty()
        && (name.chars().count() == 1
            || name.bytes().all(|byte| byte.is_ascii_alphabetic()))
}

fn invalid_macro(name: &str, message: &str) -> Diagnostic {
    Diagnostic::warning(
        "math-macro-invalid",
        format!("[math] katex_options.macros[\"{name}\"]: {message}"),
    )
}

fn xml_safe_mathml(mathml: &str) -> String {
    mathml
        .replace("&ApplyFunction;", "&#x2061;")
        .replace("&InvisibleTimes;", "&#x2062;")
        .replace("&InvisibleComma;", "&#x2063;")
        .replace("&nbsp;", "&#xA0;")
}

/// Failures while converting equation sources or mapping MathML elements.
#[derive(Debug, thiserror::Error)]
pub enum MathError {
    /// LaTeX could not be parsed by math-core.
    #[error("{message}")]
    Syntax {
        /// Parser message without source location formatting.
        message: String,
        /// Byte range in the prepared equation source, when known.
        range: Option<Range<usize>>,
        /// Escaped `<span class="math-error">` fallback, valid inside a span.
        html: String,
    },
    /// A named macro definition could not be used.
    #[error("macro {name}: {message}")]
    Macro {
        /// Macro name, including its leading backslash.
        name: String,
        /// Why the definition could not be used.
        message: String,
    },
    /// MathML contains an element unsupported by the target converter.
    #[error("unsupported {element}: {detail}")]
    Unsupported {
        /// Element or unsupported entity name.
        element: String,
        /// Offending markup, limited to 80 characters by its producer.
        detail: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Level;

    #[test]
    fn inline_display_and_empty_output_have_fixed_mathml_shape() {
        let (converter, diagnostics) = Converter::new(&BTreeMap::new());
        assert!(diagnostics.is_empty());
        assert_eq!(
            converter.to_mathml(r"\frac{a}{b}", false).unwrap(),
            r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><mfrac><mi>a</mi><mi>b</mi></mfrac></math>"#
        );
        assert_eq!(
            converter.to_mathml(r"\frac{a}{b}", true).unwrap(),
            r#"<math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><mfrac><mi>a</mi><mi>b</mi></mfrac></math>"#
        );
        assert_eq!(
            converter.to_mathml("", false).unwrap(),
            r#"<math xmlns="http://www.w3.org/1998/Math/MathML"></math>"#
        );
    }

    #[test]
    fn syntax_error_carries_message_byte_range_and_inline_fallback() {
        let (converter, _) = Converter::new(&BTreeMap::new());
        let error = converter.to_mathml(r"\foo{x}", true).unwrap_err();
        assert_eq!(error.to_string(), r#"Unknown command "\foo"."#);
        let MathError::Syntax { message, range, html } = error else {
            panic!("expected a syntax error");
        };
        assert_eq!(message, r#"Unknown command "\foo"."#);
        assert_eq!(range, Some(0..4));
        assert_eq!(
            html,
            r#"<span class="math-error" title="0: Unknown command &quot;\foo&quot;."><code>\foo{x}</code></span>"#
        );
        let MathError::Syntax { range, .. } =
            converter.to_mathml(r"\frac{a}{b", false).unwrap_err()
        else {
            panic!("expected an unclosed-brace error");
        };
        assert_eq!(range, Some(10..10));
    }

    #[test]
    fn invalid_macros_are_reported_and_valid_definitions_survive_retries() {
        let macros = BTreeMap::from([
            (r"\RR".into(), r"\mathbb{R}".into()),
            (r"\half".into(), r"\frac{#1}{2}".into()),
            (r"\bad".into(), r"\nosuch{#1}".into()),
            ("nobackslash".into(), "x".into()),
            (r"\R R".into(), "x".into()),
        ]);
        let (converter, diagnostics) = Converter::new(&macros);
        assert_eq!(diagnostics.len(), 3);
        for (diagnostic, name) in
            diagnostics.iter().zip([r"\R R", r"\bad", "nobackslash"])
        {
            assert_eq!(diagnostic.code, "math-macro-invalid");
            assert_eq!(diagnostic.level, Level::Warning);
            assert!(diagnostic.message.starts_with(&format!(
                "[math] katex_options.macros[\"{name}\"]: "
            )));
            assert!(diagnostic.span.is_none());
            assert!(diagnostic.path.is_none());
        }
        assert!(
            diagnostics[1].message.contains(r#"Unknown command "\nosuch"."#)
        );
        let mathml = converter.to_mathml(r"\RR^n + \half{x}", false).unwrap();
        assert!(mathml.contains("<mi>ℝ</mi>"));
        assert!(mathml.contains("<mfrac><mi>x</mi><mn>2</mn></mfrac>"));
    }

    #[test]
    fn rejects_each_bad_replacement_until_an_empty_converter_succeeds() {
        let macros = BTreeMap::from([
            (r"\broken".into(), r"\nosuch{x}".into()),
            (r"\missing".into(), r"\anotherunknown{x}".into()),
        ]);
        let (converter, diagnostics) = Converter::new(&macros);
        assert_eq!(diagnostics.len(), 2);
        assert!(converter.to_mathml("x", false).is_ok());
        assert!(converter.to_mathml(r"\broken", false).is_err());
    }

    #[test]
    fn name_validation_matches_the_pinned_math_core_rule() {
        for valid in ["RR", "a", "!", "α", "♥"] {
            assert!(valid_macro_name(valid), "{valid}");
        }
        for invalid in ["", "a1", "R R", "αβ", r"\RR"] {
            assert!(!valid_macro_name(invalid), "{invalid}");
        }
    }

    #[test]
    fn equation_definitions_do_not_leak_between_conversions() {
        let (converter, _) = Converter::new(&BTreeMap::new());
        assert!(
            converter
                .to_mathml(r"\newcommand{\temporary}{x}\temporary", false)
                .unwrap()
                .contains("<mi>x</mi>")
        );
        assert!(converter.to_mathml(r"\temporary", false).is_err());
        assert!(converter.to_mathml("x", false).is_ok());
    }

    #[test]
    fn replaces_only_html_only_entities() {
        assert_eq!(
            xml_safe_mathml(
                "&ApplyFunction;&InvisibleTimes;&InvisibleComma;&nbsp;&amp;&foo;"
            ),
            "&#x2061;&#x2062;&#x2063;&#xA0;&amp;&foo;"
        );
        let (converter, _) = Converter::new(&BTreeMap::new());
        let mathml = converter
            .to_mathml(r"\operatorname{argmax}_x f(x)", false)
            .unwrap();
        assert!(mathml.contains("&#x2061;"));
        assert!(!mathml.contains("&ApplyFunction;"));
        assert!(!mathml.contains("<semantics>"));
        assert!(!mathml.contains("\n"));
    }

    #[test]
    fn cache_follows_the_macro_map_and_returns_independent_diagnostics() {
        let a = BTreeMap::from([(r"\RR".into(), r"\mathbb{R}".into())]);
        let b = BTreeMap::from([(r"\RR".into(), r"\mathbf{R}".into())]);
        for (macros, symbol) in [(&a, "ℝ"), (&b, "𝐑"), (&a, "ℝ")] {
            assert!(to_mathml(r"\RR", false, macros).unwrap().contains(symbol));
        }
        let bad = BTreeMap::from([("invalid".into(), "x".into())]);
        let mut diagnostics = macro_diagnostics(&bad);
        diagnostics.clear();
        assert_eq!(macro_diagnostics(&bad).len(), 1);
        assert!(macro_diagnostics(&BTreeMap::new()).is_empty());
        assert_eq!(macro_diagnostics(&bad).len(), 1);
    }

    #[test]
    fn simultaneous_macro_maps_use_their_own_converter() {
        let handles: Vec<_> = [(r"\mathbb{R}", "ℝ"), (r"\mathbf{R}", "𝐑")]
            .into_iter()
            .map(|(body, symbol)| {
                std::thread::spawn(move || {
                    let macros = BTreeMap::from([(r"\RR".into(), body.into())]);
                    for _ in 0..20 {
                        assert!(
                            to_mathml(r"\RR", false, &macros)
                                .unwrap()
                                .contains(symbol)
                        );
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn converter_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Converter>();
    }
}
