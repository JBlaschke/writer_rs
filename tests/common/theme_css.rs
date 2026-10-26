//! Theme stylesheet inspection helpers lifted from presenter theme tests.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use regex::Regex;

pub fn theme_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/themes/default")
}

pub fn read(rel: &str) -> String {
    std::fs::read_to_string(theme_dir().join(rel))
        .unwrap_or_else(|error| panic!("{rel}: {error}"))
}

/// Stylesheet entry paths in their manifest cascade order.
pub fn stylesheets() -> Vec<String> {
    let manifest: toml::Table = read("theme.toml").parse().unwrap();
    manifest["styles"]["entries"]
        .as_array()
        .expect("[styles] entries")
        .iter()
        .map(|entry| entry.as_str().expect("a stylesheet path").to_string())
        .collect()
}

/// Every entry's CSS, concatenated with comments removed.
pub fn css() -> String {
    let comments = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    stylesheets()
        .iter()
        .map(|entry| comments.replace_all(&read(entry), "").into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One CSS style rule in source order, with its enclosing at-rule.
#[derive(Debug)]
pub struct Rule {
    pub context: String,
    pub selector: String,
    pub declarations: BTreeMap<String, String>,
}

pub fn rules(css: &str) -> Vec<Rule> {
    let mut result = Vec::new();
    collect_rules(css, "", &mut result);
    result
}

fn collect_rules(css: &str, context: &str, out: &mut Vec<Rule>) {
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let prelude = normalize(&rest[..open]);
        let mut depth = 1;
        let mut close = None;
        for (index, character) in rest[open + 1..].char_indices() {
            depth += match character {
                '{' => 1,
                '}' => -1,
                _ => 0,
            };
            if depth == 0 {
                close = Some(open + 1 + index);
                break;
            }
        }
        let close = close.expect("balanced CSS braces");
        let body = &rest[open + 1..close];
        if prelude.starts_with('@') {
            collect_rules(body, &prelude, out);
        } else {
            let declarations = body
                .split(';')
                .filter_map(|declaration| declaration.split_once(':'))
                .map(|(name, value)| (name.trim().into(), normalize(value)))
                .collect();
            out.push(Rule {
                context: context.into(),
                selector: prelude,
                declarations,
            });
        }
        rest = rest.get(close + 1..).unwrap_or("");
    }
}

/// Split selector lists without splitting the arguments of `:is()` or `:not()`.
pub fn complex_selectors(list: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    for (index, character) in list.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(list[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(list[start..].trim());
    parts
}

/// The last declaration for an exact selector in one at-rule context.
pub fn decl(
    rules: &[Rule], context: &str, selector: &str, property: &str,
) -> Option<String> {
    rules
        .iter()
        .rev()
        .filter(|rule| {
            rule.context == context
                && complex_selectors(&rule.selector).contains(&selector)
        })
        .find_map(|rule| rule.declarations.get(property).cloned())
}

/// Public document tokens declared on `:root` in the given context.
pub fn root_tokens(rules: &[Rule], context: &str) -> BTreeMap<String, String> {
    let mut tokens = BTreeMap::new();
    for rule in rules
        .iter()
        .filter(|rule| rule.context == context && rule.selector == ":root")
    {
        for (name, value) in &rule.declarations {
            if name.starts_with("--w-") {
                tokens.insert(name.clone(), value.clone());
            }
        }
    }
    tokens
}

pub fn used_tokens(css: &str) -> BTreeSet<String> {
    Regex::new(r"var\(\s*(--w-[a-z0-9-]+)")
        .unwrap()
        .captures_iter(css)
        .map(|captures| captures[1].to_string())
        .collect()
}

pub fn has_class(rules: &[Rule], class: &str) -> bool {
    let pattern = format!(r"\.{}(?:[^A-Za-z0-9_-]|$)", regex::escape(class));
    let pattern = Regex::new(&pattern).unwrap();
    rules.iter().any(|rule| pattern.is_match(&rule.selector))
}

pub fn hex_rgb(hex: &str) -> [u8; 3] {
    let digits = hex
        .strip_prefix('#')
        .unwrap_or_else(|| panic!("{hex} is not a hex colour"));
    let wide: String = match digits.len() {
        3 => digits
            .chars()
            .flat_map(|character| [character, character])
            .collect(),
        6 => digits.into(),
        _ => panic!("{hex} is not a hex colour"),
    };
    let channel =
        |index| u8::from_str_radix(&wide[index..index + 2], 16).unwrap();
    [channel(0), channel(2), channel(4)]
}

/// WCAG 2 contrast ratio of two sRGB colours, from 1 through 21.
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let luminance = |rgb: [u8; 3]| {
        let linear = |channel: u8| {
            let channel = f64::from(channel) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(rgb[0])
            + 0.7152 * linear(rgb[1])
            + 0.0722 * linear(rgb[2])
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
