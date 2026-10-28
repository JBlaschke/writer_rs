//! The generated symbol table and package lookup for the math converter.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

const DATA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/math/unicode-latex.toml"
));

/// A supported LaTeX expression and its package beyond amsmath/amssymb.
#[derive(Debug, PartialEq, Eq)]
pub struct Sym {
    /// The expression, without surrounding math delimiters.
    pub tex: &'static str,
    /// An extra package required by the expression.
    pub package: Option<&'static str>,
}

#[derive(Deserialize)]
struct OwnedSym {
    tex: String,
    package: Option<String>,
}

#[derive(Deserialize)]
struct OwnedTable {
    names: BTreeMap<String, OwnedSym>,
    chars: BTreeMap<String, OwnedSym>,
}

struct Tables {
    names: BTreeMap<&'static str, Sym>,
    chars: BTreeMap<char, Sym>,
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    // One process-lifetime source allocation backs all public static slices.
    let source: &'static OwnedTable = Box::leak(Box::new(
        toml::from_str(DATA).expect("valid generated Unicode-to-LaTeX table"),
    ));
    let sym = |row: &'static OwnedSym| Sym {
        tex: row.tex.as_str(),
        package: row.package.as_deref(),
    };
    let names = source
        .names
        .iter()
        .map(|(name, row)| (name.as_str(), sym(row)))
        .collect();
    let chars = source
        .chars
        .iter()
        .map(|(key, row)| {
            let value = u32::from_str_radix(
                key.strip_prefix("U+").expect("generated code point key"),
                16,
            )
            .expect("generated hexadecimal code point");
            (char::from_u32(value).expect("generated Unicode scalar"), sym(row))
        })
        .collect();
    Tables { names, chars }
});

/// Looks up a Typst name in the final, curated symbol table.
pub fn symbol(path: &str) -> Option<&'static Sym> {
    TABLES.names.get(path)
}

/// Looks up one Unicode math character in the final, curated symbol table.
pub fn char_command(c: char) -> Option<&'static Sym> {
    TABLES.chars.get(&c)
}

/// Resolves a current codex name/modifier path without evaluating user code.
pub(super) fn resolve_char(path: &str) -> Option<char> {
    let parts = path.split('.').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let mut module = codex::SYM;
    for (index, part) in parts.iter().enumerate() {
        match module.get(part)?.def {
            codex::Def::Module(nested) => module = nested,
            codex::Def::Symbol(symbol) => {
                let modifiers = parts[index + 1..].join(".");
                let (value, _) = symbol.get(
                    codex::ModifierSet::from_raw_dotted(modifiers.as_str()),
                )?;
                let value = value.trim_end_matches(['\u{fe0e}', '\u{fe0f}']);
                let mut chars = value.chars();
                let first = chars.next()?;
                return chars.next().is_none().then_some(first);
            }
        }
    }
    None
}

const COMMAND_PACKAGES: &[(&str, &str)] = &[
    ("\\Coloneqq", "mathtools"),
    ("\\Mapsfrom", "stmaryrd"),
    ("\\Mapsto", "stmaryrd"),
    ("\\bcancel", "cancel"),
    ("\\bigsqcap", "stmaryrd"),
    ("\\bm", "bm"),
    ("\\cancel", "cancel"),
    ("\\coloneqq", "mathtools"),
    ("\\degree", "gensymb"),
    ("\\eqqcolon", "mathtools"),
    ("\\euro", "eurosym"),
    ("\\iddots", "mathdots"),
    ("\\llbracket", "stmaryrd"),
    ("\\longmapsfrom", "stmaryrd"),
    ("\\mapsfrom", "stmaryrd"),
    ("\\mathclap", "mathtools"),
    ("\\mathscr", "mathrsfs"),
    ("\\oiint", "esint"),
    ("\\overbracket", "mathtools"),
    ("\\reflectbox", "graphicx"),
    ("\\rrbracket", "stmaryrd"),
    ("\\underbracket", "mathtools"),
    ("\\xcancel", "cancel"),
    ("\\xmapsto", "mathtools"),
];

/// Returns the extra package defining an exact backslash-prefixed control word.
pub fn command_package(name: &str) -> Option<&'static str> {
    COMMAND_PACKAGES
        .iter()
        .find_map(|(command, package)| (*command == name).then_some(*package))
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn generated_table_has_the_required_symbols_and_static_references() {
        for (char, tex) in [
            ('§', "\\S"),
            ('¶', "\\P"),
            ('α', "\\alpha"),
            ('ε', "\\varepsilon"),
            ('→', "\\rightarrow"),
            ('★', "\\bigstar"),
            ('ℝ', "\\mathbb{R}"),
            ('𝐀', "\\mathbf{A}"),
            ('𝔸', "\\mathbb{A}"),
        ] {
            let row = char_command(char).unwrap();
            assert_eq!(row.tex, tex, "{char}");
            assert!(std::ptr::eq(row, char_command(char).unwrap()));
        }
        assert_eq!(symbol("AA").unwrap().tex, "\\mathbb{A}");
        for row in [symbol(":=").unwrap(), char_command('≔').unwrap()] {
            assert_eq!(row.tex, "\\coloneqq");
            assert_eq!(row.package, Some("mathtools"));
        }
        assert!(symbol("planck.reduce").is_none());
        assert!(char_command('ℏ').is_some());
    }

    #[test]
    fn codex_resolution_handles_modifiers_aliases_and_invalid_public_paths() {
        assert_eq!(resolve_char("arrow.r.long"), Some('⟶'));
        assert_eq!(resolve_char("arrow.long.r"), Some('⟶'));
        assert_eq!(resolve_char("ast.basic"), Some('*'));
        assert_eq!(resolve_char("RR"), Some('ℝ'));
        for path in [
            "", ".", ".arrow", "arrow.", "arrow..r", "planck.reduce",
            "not-a-symbol",
        ] {
            assert_eq!(resolve_char(path), None, "{path}");
        }
    }

    #[test]
    fn every_shorthand_and_character_row_keeps_its_code_point() {
        let raw: toml::Value = toml::from_str(DATA).unwrap();
        for (key, row) in raw["chars"].as_table().unwrap() {
            let code = u32::from_str_radix(&key[2..], 16).unwrap();
            assert_eq!(
                row["char"].as_str().unwrap(),
                char::from_u32(code).unwrap().to_string()
            );
        }
        for (shorthand, char) in typst_syntax::ast::MathShorthand::LIST {
            assert!(char_command(*char).is_some(), "{shorthand}: {char}");
        }
        let lock: toml::Value = toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/Cargo.lock"
        )))
        .unwrap();
        for name in ["codex", "typst-syntax"] {
            let package = lock["package"]
                .as_array()
                .unwrap()
                .iter()
                .find(|package| package["name"].as_str() == Some(name))
                .unwrap();
            assert_eq!(raw["source"][name], package["version"]);
        }
    }

    #[test]
    fn command_packages_cover_every_override_and_exact_control_words() {
        let overrides: toml::Value = toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/unicode-latex-overrides.toml"
        )))
        .unwrap();
        for (command, package) in overrides["packages"].as_table().unwrap() {
            assert_eq!(command_package(command), package.as_str(), "{command}");
        }
        for (command, package) in COMMAND_PACKAGES {
            assert_eq!(command_package(command), Some(*package));
            assert_eq!(command_package(&format!("{command}extra")), None);
            assert_eq!(command_package(command.trim_start_matches('\\')), None);
        }
    }

    #[test]
    fn every_packaged_symbol_can_be_discovered_by_the_output_command_scan() {
        let commands = Regex::new(r"\\[A-Za-z]+").unwrap();
        for row in TABLES.names.values().chain(TABLES.chars.values()) {
            if let Some(package) = row.package {
                assert!(
                    commands.find_iter(row.tex).any(|command| {
                        command_package(command.as_str()) == Some(package)
                    }),
                    "{} requires {package}",
                    row.tex
                );
            }
        }
    }
}
