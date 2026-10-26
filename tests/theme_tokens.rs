//! Default theme tokens, documentation, contrast and offline file contracts.

#[path = "common/theme_css.rs"]
mod theme_css;

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use theme_css::*;
use writer::html::emit::MARKUP_CLASSES;

const REQUIRED_TOKENS: &[&str] = &[
    "--w-font-body", "--w-font-heading", "--w-font-mono", "--w-font-size",
    "--w-line-height", "--w-measure", "--w-text", "--w-bg", "--w-muted",
    "--w-accent", "--w-link", "--w-border", "--w-code-bg",
    "--w-code-highlight", "--w-callout-note", "--w-callout-tip",
    "--w-callout-important", "--w-callout-warning", "--w-callout-caution",
    "--w-callout-other", "--w-error", "--w-error-bg",
];

fn light() -> BTreeMap<String, String> {
    root_tokens(&rules(&css()), "")
}

fn dark() -> BTreeMap<String, String> {
    root_tokens(&rules(&css()), "@media (prefers-color-scheme: dark)")
}

fn documented() -> BTreeMap<String, (String, String)> {
    let row =
        Regex::new(r"(?m)^\| `(--w-[a-z0-9-]+)` \| (.+?) \| (.+?) \| .+\|$")
            .unwrap();
    let mut tokens = BTreeMap::new();
    for captures in row.captures_iter(&read("README.md")) {
        let cell =
            |index: usize| captures[index].trim().trim_matches('`').to_string();
        let previous =
            tokens.insert(captures[1].to_string(), (cell(2), cell(3)));
        assert!(previous.is_none(), "{} documented twice", &captures[1]);
    }
    tokens
}

#[test]
fn every_root_token_is_documented_with_its_defaults() {
    let (light, dark, documented) = (light(), dark(), documented());
    for (name, value) in &light {
        let expected_dark = dark.get(name).map(String::as_str).unwrap_or("—");
        let actual = documented
            .get(name)
            .unwrap_or_else(|| panic!("{name} has no README token row"));
        assert_eq!(
            (&actual.0, actual.1.as_str()),
            (value, expected_dark),
            "{name}"
        );
    }
    for name in documented.keys() {
        assert!(light.contains_key(name), "README declares unknown {name}");
    }
}

#[test]
fn the_spec_tokens_exist() {
    let tokens = light();
    for name in REQUIRED_TOKENS {
        assert!(tokens.contains_key(*name), "missing {name}");
    }
    assert_eq!(tokens["--w-font-size"], "1rem");
    assert_eq!(
        decl(&rules(&css()), "", ":root", "color-scheme").as_deref(),
        Some("light dark")
    );
}

#[test]
fn colour_tokens_are_redefined_for_dark() {
    let (light, dark) = (light(), dark());
    for (name, value) in &light {
        if value.starts_with('#') {
            hex_rgb(value);
            hex_rgb(dark.get(name).unwrap_or_else(|| panic!("no dark {name}")));
        }
    }
    for (name, value) in &dark {
        assert!(light.contains_key(name), "dark-only token {name}");
        hex_rgb(value);
    }
    for name in &REQUIRED_TOKENS[6..] {
        hex_rgb(&light[*name]);
        hex_rgb(&dark[*name]);
    }
}

#[test]
fn error_colour_is_katex_red() {
    assert_eq!(light()["--w-error"], "#cc0000");
}

#[test]
fn text_is_readable_in_both_schemes() {
    for (scheme, tokens) in [("light", light()), ("dark", dark())] {
        let check = |foreground: &str, background: &str, minimum: f64| {
            let ratio = contrast(
                hex_rgb(&tokens[foreground]),
                hex_rgb(&tokens[background]),
            );
            assert!(
                ratio >= minimum,
                "{scheme} {foreground} on {background}: {ratio:.2} < {minimum}"
            );
        };
        check("--w-text", "--w-bg", 7.0);
        for name in ["--w-muted", "--w-link", "--w-error"] {
            check(name, "--w-bg", 4.5);
        }
        for name in ["--w-code-bg", "--w-code-highlight"] {
            check("--w-text", name, 4.5);
        }
        check("--w-accent", "--w-bg", 3.0);
        for kind in ["note", "tip", "important", "warning", "caution", "other"]
        {
            check(&format!("--w-callout-{kind}"), "--w-bg", 3.0);
        }
    }
}

#[test]
fn the_readme_names_only_tokens_the_css_knows() {
    let tokens = light();
    let name = Regex::new(r"--w-[a-z0-9-]+").unwrap();
    for found in name.find_iter(&read("README.md")) {
        assert!(
            tokens.contains_key(found.as_str()),
            "unknown README token {}",
            found.as_str()
        );
    }
    for used in used_tokens(&css()) {
        assert!(tokens.contains_key(&used), "undeclared CSS token {used}");
    }
}

#[test]
fn private_properties_outside_root() {
    for rule in rules(&css()) {
        if complex_selectors(&rule.selector).contains(&":root") {
            continue;
        }
        for name in
            rule.declarations.keys().filter(|name| name.starts_with("--"))
        {
            assert!(
                name.starts_with("--_"),
                "public {name} outside :root in {}",
                rule.selector
            );
        }
    }
}

#[test]
fn stylesheet_loads_nothing_from_the_network() {
    let remote = Regex::new(r#"(?i)url\(\s*['"]?\s*(https?:|//)"#).unwrap();
    let address = Regex::new(r"(?i)https?://").unwrap();
    for entry in stylesheets() {
        let source = read(&entry);
        assert!(!source.contains("@import"), "{entry}: no @import");
        assert!(!source.contains("@font-face"), "{entry}: system fonts only");
        assert!(!remote.is_match(&source), "{entry}: no remote url()");
        assert!(!address.is_match(&source), "{entry}: no remote address");
    }
}

#[test]
fn theme_css_contains_no_text_that_document_tests_look_for() {
    for entry in stylesheets() {
        for needle in
            ["asset:", "/__writer/", "In [", "Out [", "data-sourcepos"]
        {
            assert!(
                !read(&entry).contains(needle),
                "{entry} contains {needle:?}"
            );
        }
    }
}

#[test]
fn the_rule_parser_sees_every_rule() {
    let css = css();
    assert_eq!(css.matches('{').count(), css.matches('}').count());
    let mut expected = 0;
    let mut start = 0;
    for (index, character) in css.char_indices() {
        if character == '{' {
            if !css[start..index].trim().starts_with('@') {
                expected += 1;
            }
            start = index + 1;
        } else if character == '}' {
            start = index + 1;
        }
    }
    let parsed = rules(&css);
    assert_eq!(parsed.len(), expected);
    assert!(parsed.len() >= 60, "only {} rules", parsed.len());
    for rule in parsed {
        assert!(!rule.selector.is_empty(), "empty selector: {rule:?}");
        assert!(!rule.declarations.is_empty(), "empty declarations: {rule:?}");
    }
}

#[test]
fn readme_lists_every_markup_class() {
    let readme = read("README.md");
    for class in MARKUP_CLASSES {
        let pattern = format!(r"(?m)^- `\.{}` — .+$", regex::escape(class));
        assert!(
            Regex::new(&pattern).unwrap().is_match(&readme),
            "no README line for .{class}"
        );
    }
}

#[test]
fn readme_explains_extending() {
    let readme = read("README.md");
    for text in [
        "extends = \"default\"",
        "{% extends \"@default/document.html\" %}",
        "writer eject theme",
        "html.toc",
        "data-writer-document",
        "head",
        "styles",
        "body_start",
        "main",
        "scripts",
        "body_end",
    ] {
        assert!(readme.contains(text), "README misses {text:?}");
    }
}

fn assert_names_no_tasks_or_repository_documents(name: &str, text: &str) {
    let task = Regex::new(r"\bT[0-9][0-9A-Z]\b").unwrap();
    assert!(!task.is_match(text), "{name} names a task id");
    for reference in [
        "docs/spec", "docs/tasks", "docs/dev", ".md §", "config.md", "ir.md",
        "](../",
    ] {
        assert!(!text.contains(reference), "{name} points at {reference}");
    }
}

#[test]
fn shipped_theme_files_name_no_tasks_or_repository_documents() {
    let mut checked = BTreeSet::new();
    for entry in walkdir::WalkDir::new(theme_dir()) {
        let entry = entry.unwrap();
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name =
            path.strip_prefix(theme_dir()).unwrap().display().to_string();
        assert_names_no_tasks_or_repository_documents(
            &name,
            &std::fs::read_to_string(path).unwrap(),
        );
        checked.insert(name);
    }
    assert!(checked.len() >= 6, "only {} files", checked.len());
}
