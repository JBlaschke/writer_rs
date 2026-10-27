//! Default document theme layout, notebook, print and end-to-end contracts.

#[path = "common/theme_css.rs"]
mod theme_css;

use std::collections::BTreeSet;

use scraper::{Html, Selector};
use theme_css::*;
use writer::assets::registry::AssetRegistry;
use writer::html::compile_theme_css;
use writer::html::emit::MARKUP_CLASSES;
use writer::html::highlight::{builtin_code_theme, code_theme_css};
use writer::ir::Features;

const ENTRIES: &[&str] =
    &["styles/theme.css", "styles/notebook.css", "styles/print.css"];
const ANSI_NAMES: &[&str] =
    &["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
const PALETTE: &[&str] = &[
    "#3e424d", "#e75c58", "#00a250", "#ddb62b", "#208ffb", "#d160c4",
    "#60c6c8", "#c5c1b4", "#282c36", "#b22b31", "#007427", "#b27d12",
    "#0065ca", "#a03196", "#258f8f", "#a1a6b2",
];

fn expect(
    rules: &[Rule], context: &str, selector: &str, name: &str, value: &str,
) {
    assert_eq!(
        decl(rules, context, selector, name).as_deref(),
        Some(value),
        "{context} {selector} {{ {name}: {value} }}"
    );
}

fn base(rules: &[Rule], selector: &str, name: &str, value: &str) {
    expect(rules, "", selector, name, value);
}

#[test]
fn every_required_class_has_a_selector() {
    let rules = rules(&css());
    for class in MARKUP_CLASSES.iter().copied().chain([
        "columns", "column", "center", "small", "large", "overline", "left",
        "right", "unresolved-reference", "output-text", "output-stream-stdout",
        "output-stream-stderr", "output-json", "subcaption", "ansi-bold",
        "ansi-dim", "ansi-italic", "ansi-underline", "ansi-strike",
        "ansi-hidden", "ansi-default-inverse-fg", "ansi-default-inverse-bg",
    ]) {
        assert!(has_class(&rules, class), "unstyled .{class}");
    }
    for name in ANSI_NAMES {
        for suffix in ["fg", "bg", "intense-fg", "intense-bg"] {
            let class = format!("ansi-{name}-{suffix}");
            assert!(has_class(&rules, &class), "unstyled .{class}");
        }
    }
    assert!(!has_class(&rules, "math-err"));
}

#[test]
fn layout_rules() {
    let rules = rules(&css());
    base(&rules, "body", "margin", "0");
    base(&rules, "body", "background-color", "var(--w-bg)");
    base(&rules, "body", "color", "var(--w-text)");
    base(&rules, "body", "font-family", "var(--w-font-body)");
    base(&rules, "[data-writer-document]", "max-width", "var(--w-measure)");
    base(&rules, "[data-writer-document]", "margin", "0 auto");
    base(&rules, "img", "max-width", "100%");
    base(&rules, "img", "height", "auto");
    base(&rules, ".columns", "display", "grid");
    base(&rules, ".columns", "grid-auto-flow", "column");
    base(&rules, ".columns", "grid-auto-columns", "1fr");
    base(&rules, ".column", "min-width", "0");
    assert!(
        rules.iter().any(|rule| {
            rule.context.contains("max-width")
                && complex_selectors(&rule.selector).contains(&".columns")
                && rule
                    .declarations
                    .get("grid-auto-flow")
                    .is_some_and(|value| value == "row")
        }),
        "columns must stack on narrow screens"
    );
}

#[test]
fn tables_are_booktabs_like() {
    let rules = rules(&css());
    base(&rules, "table", "border-collapse", "collapse");
    for property in ["border-top", "border-bottom"] {
        assert!(
            decl(&rules, "", "table", property).is_some(),
            "table needs {property}"
        );
    }
    assert!(decl(&rules, "", "thead", "border-bottom").is_some());
    for rule in &rules {
        if !complex_selectors(&rule.selector)
            .iter()
            .any(|selector| ["td", "th"].contains(selector))
        {
            continue;
        }
        for name in rule.declarations.keys() {
            assert!(
                !["border-left", "border-right", "border-inline"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix)),
                "vertical table rule: {rule:?}"
            );
        }
    }
    for (class, value) in
        [("left", "left"), ("center", "center"), ("right", "right")]
    {
        base(&rules, &format!(".align-{class}"), "text-align", value);
    }
}

#[test]
fn code_rules() {
    let rules = rules(&css());
    base(&rules, ".hl-code", "background-color", "var(--w-code-bg)");
    base(&rules, ".hl-code", "color", "var(--w-text)");
    base(&rules, "pre.hl-code", "overflow-x", "auto");
    base(&rules, "pre.hl-code", "font-family", "var(--w-font-mono)");
    assert!(decl(&rules, "", "pre.hl-code", "padding").is_some());
    for property in ["color", "background-color", "background"] {
        assert!(
            decl(&rules, "", "pre.hl-code", property).is_none(),
            "pre.hl-code must not override generated code colours"
        );
    }
    base(&rules, ":not(pre) > code", "background-color", "var(--w-code-bg)");
    base(&rules, "pre.numbered", "counter-reset", "line");
    base(&rules, "pre.numbered .line::before", "counter-increment", "line");
    base(&rules, "pre.numbered .line::before", "content", "counter(line)");
    base(&rules, "pre.hl-code .line", "display", "inline-block");
    base(&rules, "pre.hl-code .line", "min-width", "100%");
    for selector in [".line.highlight", "span.highlight"] {
        base(&rules, selector, "background-color", "var(--w-code-highlight)");
    }
}

#[test]
fn math_rules() {
    let rules = rules(&css());
    for (name, value) in
        [("display", "flex"), ("align-items", "center"), ("gap", "1em")]
    {
        base(&rules, ".equation", name, value);
    }
    for (name, value) in [
        ("flex", "1 1 auto"),
        ("min-width", "0"),
        ("padding-block", ".5em"),
        ("overflow-x", "auto"),
        ("overflow-y", "hidden"),
        ("text-align", "center"),
    ] {
        base(&rules, ".equation > .math.display", name, value);
    }
    base(&rules, ".equation-number", "flex", "none");
    base(&rules, ".equation-number", "margin-left", "auto");
    base(&rules, ".math-error", "color", "var(--w-error)");
    let dark = "@media (prefers-color-scheme: dark)";
    expect(&rules, dark, ".math img", "filter", "invert(1) hue-rotate(180deg)");
    assert_eq!(decl(&rules, "", ".math img", "filter"), None);
    for rule in rules.iter().filter(|rule| {
        rule.declarations
            .get("filter")
            .is_some_and(|value| value.contains("invert("))
    }) {
        assert_eq!(
            rule.context, dark,
            "inversion must apply only in dark mode"
        );
        for selector in complex_selectors(&rule.selector) {
            assert_eq!(
                selector, ".math img",
                "inversion must leave other figures unchanged"
            );
        }
    }
}

#[test]
fn callouts_have_an_edge_per_kind() {
    let rules = rules(&css());
    base(
        &rules,
        ".callout",
        "border-left",
        ".25em solid var(--w-callout-other)",
    );
    assert!(decl(&rules, "", ".callout", "padding").is_some());
    for kind in ["note", "tip", "important", "warning", "caution"] {
        base(
            &rules,
            &format!(".callout-{kind}"),
            "border-left-color",
            &format!("var(--w-callout-{kind})"),
        );
    }
    base(&rules, ".callout-title", "font-weight", "bold");
}

#[test]
fn lists_and_toc_have_no_markers() {
    let rules = rules(&css());
    for selector in [".task-list", ".toc ol"] {
        base(&rules, selector, "list-style", "none");
    }
}

#[test]
fn bibliography_hangs_and_aligns() {
    let rules = rules(&css());
    base(&rules, ".hanging-indent .csl-entry", "padding-left", "2em");
    base(&rules, ".hanging-indent .csl-entry", "text-indent", "-2em");
    base(&rules, ".csl-left-margin", "float", "left");
    base(&rules, ".csl-left-margin", "min-width", "2em");
    base(&rules, ".csl-right-inline", "margin-left", "2em");
}

#[test]
fn bibliography_prefixes_do_not_stack_hanging_indents() {
    let rules = rules(&css());
    base(&rules, ".hanging-indent .csl-left-margin", "margin-left", "-2em");
    base(&rules, ".hanging-indent .csl-left-margin", "text-indent", "0");
    base(&rules, ".hanging-indent .csl-right-inline", "margin-left", "0");
}

#[test]
fn unresolved_is_marked() {
    let rules = rules(&css());
    for selector in [".unresolved", ".unresolved-reference"] {
        base(&rules, selector, "color", "var(--w-error)");
        let decoration = decl(&rules, "", selector, "text-decoration")
            .or_else(|| decl(&rules, "", selector, "text-decoration-style"))
            .unwrap();
        assert!(decoration.contains("wavy"), "{selector}: {decoration}");
    }
}

#[test]
fn typst_frontend_rules() {
    let rules = rules(&css());
    base(&rules, "span.overline", "text-decoration", "overline");
    base(
        &rules, "span.highlight", "background-color", "var(--w-code-highlight)",
    );
    for direction in ["left", "center", "right"] {
        base(&rules, &format!("div.{direction}"), "text-align", direction);
    }
    base(&rules, ".small", "font-size", ".85em");
    base(&rules, ".large", "font-size", "1.25em");
    base(&rules, ".smallcaps", "font-variant", "small-caps");
}

#[test]
fn notebook_rules() {
    let rules = rules(&css());
    base(&rules, ".cell-prompt", "font-family", "var(--w-font-mono)");
    base(&rules, ".cell-prompt", "font-size", ".75em");
    base(&rules, ".cell-prompt", "color", "var(--w-muted)");
    base(&rules, ".cell-prompt", "position", "absolute");
    base(&rules, ".cell-prompt", "text-align", "right");
    assert!(decl(&rules, "", ".cell-prompt", "right").is_some());
    expect(
        &rules, "@media (max-width: 64rem)", ".cell-prompt", "position",
        "static",
    );
    expect(
        &rules, "@media (max-width: 64rem)", ".cell-prompt", "text-align",
        "left",
    );
    for selector in [".output.stderr > pre", "pre.output-error"] {
        base(&rules, selector, "background-color", "var(--w-error-bg)");
        base(&rules, selector, "color", "var(--w-text)");
    }
    base(&rules, ".subcaption", "font-size", ".85em");
}

#[test]
fn ansi_classes_use_the_palette() {
    let rules = rules(&css());
    let mut checked = 0;
    for (index, name) in ANSI_NAMES.iter().enumerate() {
        for (intense, offset) in [("", 0), ("-intense", 8)] {
            for (suffix, property) in
                [("fg", "color"), ("bg", "background-color")]
            {
                let selector = format!(".ansi-{name}{intense}-{suffix}");
                let value = decl(&rules, "", &selector, property).unwrap();
                assert_eq!(
                    value.to_ascii_lowercase(),
                    PALETTE[index + offset],
                    "{selector}"
                );
                assert_eq!(
                    rules
                        .iter()
                        .filter(|rule| rule.selector == selector)
                        .count(),
                    1,
                    "one rule for {selector}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 32);
    base(&rules, ".ansi-default-inverse-fg", "color", "var(--w-bg)");
    base(
        &rules, ".ansi-default-inverse-bg", "background-color", "var(--w-text)",
    );
    for (class, name, value) in [
        ("bold", "font-weight", "bold"),
        ("dim", "opacity", ".7"),
        ("italic", "font-style", "italic"),
        ("underline", "text-decoration", "underline"),
        ("strike", "text-decoration", "line-through"),
        ("hidden", "visibility", "hidden"),
    ] {
        base(&rules, &format!(".ansi-{class}"), name, value);
    }
}

#[test]
fn page_break_is_hidden_on_screen_only() {
    let rules = rules(&css());
    expect(&rules, "@media screen", ".page-break", "display", "none");
    expect(&rules, "@media print", ".page-break", "display", "block");
    expect(&rules, "@media print", ".page-break", "break-after", "page");
    for rule in &rules {
        if complex_selectors(&rule.selector).contains(&".page-break")
            && rule
                .declarations
                .get("display")
                .is_some_and(|value| value == "none")
        {
            assert_eq!(rule.context, "@media screen", "{rule:?}");
        }
    }
}

#[test]
fn print_rules() {
    let css = css();
    let rules = rules(&css);
    let text = normalize(&css);
    assert!(text.contains("@page { size: A4; margin: 20mm 18mm; @bottom-center { content: counter(page); font: 9pt system-ui; } }"));
    assert!(
        text.contains("@page :first { @bottom-center { content: none; } }")
    );
    let print = |selector, name, value| {
        expect(&rules, "@media print", selector, name, value)
    };
    print("html", "font-size", "11pt");
    for selector in ["body", "[data-writer-document]"] {
        print(selector, "max-width", "none");
    }
    print("body", "background", "#fff");
    print("body", "color", "#000");
    for heading in ["h1", "h2", "h3"] {
        print(heading, "break-after", "avoid");
    }
    for selector in
        ["figure", "table", "pre", ".callout", ".equation", ".cell", "li"]
    {
        print(selector, "break-inside", "avoid");
    }
    print("p", "orphans", "3");
    print("p", "widows", "3");
    for selector in ["nav.toc", ".no-print"] {
        print(selector, "display", "none");
    }
    print("pre", "white-space", "pre-wrap");
    print("pre.hl-code .line", "width", "100%");
    print("pre.hl-code .line", "overflow-wrap", "anywhere");
    print("*", "print-color-adjust", "exact");
    print("*", "-webkit-print-color-adjust", "exact");
    assert!(
        decl(&rules, "@media print", "a[href^=\"http\"]::after", "content")
            .unwrap()
            .contains("attr(href)")
    );
    for rule in &rules {
        assert_ne!(
            rule.declarations.get("break-before").map(String::as_str),
            Some("page")
        );
    }
}

#[test]
fn print_resets_dark_scheme_colours_and_gutters() {
    let rules = rules(&css());
    let light = root_tokens(&rules, "");
    let print = root_tokens(&rules, "@media print");
    for (name, value) in
        light.iter().filter(|(_, value)| value.starts_with('#'))
    {
        assert_eq!(
            print.get(name),
            Some(value),
            "print must restore light {name}"
        );
    }
    expect(&rules, "@media print", ":root", "color-scheme", "light");
    expect(&rules, "@media print", "[data-writer-document]", "padding", "0");
    expect(&rules, "@media print", ".cell-prompt", "position", "static");
    expect(&rules, "@media print", ".math img", "filter", "none");
}

#[test]
fn print_syntax_stays_readable_on_highlighted_lines() {
    let rules = rules(&css());
    expect(&rules, "@media print", "pre.hl-code", "color", "var(--w-text)");
    expect(
        &rules, "@media print", "pre.hl-code", "background-color",
        "var(--w-code-bg)",
    );
    let selector = "pre.hl-code [class*=\"hl-\"]";
    expect(&rules, "@media print", selector, "color", "inherit !important");
    expect(
        &rules, "@media print", selector, "background-color",
        "transparent !important",
    );
    assert!(decl(&rules, "", selector, "color").is_none());
    for name in ["InspiredGitHub", "OneHalfDark"] {
        let generated = code_theme_css(builtin_code_theme(name).unwrap());
        assert!(generated.contains(".hl-code {"));
        assert!(
            !generated.contains("!important"),
            "{name}: print overrides must outrank generated syntax colours"
        );
    }
    let example = Html::parse_fragment(
        r#"<pre class="hl-code"><code><span class="line highlight"><span class="hl-keyword">let</span><span class="ansi-red-fg">message</span></span></code></pre>"#,
    );
    let matched: Vec<_> = example
        .select(&Selector::parse(selector).unwrap())
        .map(|element| element.value().attr("class").unwrap())
        .collect();
    assert_eq!(matched, ["hl-keyword"], "keep line and ANSI colours");
    for error in [".output.stderr > pre", "pre.output-error"] {
        expect(
            &rules, "@media print", error, "background-color",
            "var(--w-error-bg)",
        );
    }
}

#[test]
fn entries_fit_the_size_budget() {
    let size: usize = stylesheets().iter().map(|entry| read(entry).len()).sum();
    eprintln!("default stylesheet entries: {size} B / 24576 B");
    assert!(size <= 24_576, "{size} B exceeds 24 KiB");
}

#[test]
fn theme_toml_matches_the_spec() {
    let manifest: toml::Table = read("theme.toml").parse().unwrap();
    assert_eq!(manifest["name"].as_str(), Some("default"));
    assert!(!manifest["description"].as_str().unwrap().is_empty());
    assert!(!manifest.contains_key("vars"));
    assert_eq!(stylesheets(), ENTRIES);
    assert_eq!(
        manifest["styles"]["code_theme"].as_str(),
        Some("InspiredGitHub")
    );
    assert_eq!(
        manifest["styles"]["code_theme_dark"].as_str(),
        Some("OneHalfDark")
    );
    assert!(manifest["scripts"]["entries"].as_array().unwrap().is_empty());
}

#[test]
fn default_theme_compiles() {
    let theme = writer::theme::resolve("default", &[]).unwrap();
    assert!(theme.problems().is_empty(), "{:?}", theme.problems());
    let mut registry = AssetRegistry::default();
    let plain =
        compile_theme_css(&theme, &Features::default(), &mut registry).unwrap();
    let mut previous = 0;
    for entry in ENTRIES {
        let found = plain.find(&format!("/* @default/{entry} */")).unwrap();
        assert!(found >= previous, "{entry} out of cascade order");
        previous = found;
    }
    assert!(!plain.contains("code theme"));
    let code = compile_theme_css(
        &theme,
        &Features { code: true, ..Features::default() },
        &mut registry,
    )
    .unwrap();
    for marker in [
        "/* code theme: InspiredGitHub */",
        "/* code theme (dark): OneHalfDark */",
    ] {
        assert!(code.find(marker).unwrap() > previous);
    }
}

const SHOWCASE: &str = r#"---
title: A document showcase
subtitle: Typography and document features
authors:
  - name: Ada Reader
    affiliation: Example Institute
    email: ada@example.org
date: "2026-10-26"
abstract: A short abstract for the showcase.
toc: true
numbering: { headings: "1.1" }
---

# Features

A paragraph with *emphasis*, **strong text**, ~~struck text~~, `inline code`,
a [link](https://example.com), inline $x^2$ and a note.[^one]

- [x] A completed task
- [ ] An open task

3. First ordered item
4. Second ordered item

Term
: A definition.

> A block quote.

***

![A dot](dot.svg){#fig-dot width=30%}

Table: Aligned values

| left | centre | right |
|:-----|:------:|------:|
| one  | two    | three |

```python {.numbered start=5 highlight="6"}
value = 1
print(value)
```

$$ E = mc^2 $$ {#eq-e}

> [!NOTE]
> A note.

> [!TIP]
> A tip.

> [!IMPORTANT]
> Important information.

> [!WARNING]
> A warning.

> [!CAUTION]
> A caution.

:::callout kind=remark
A custom remark.
:::

::::columns
:::column
The first column.
:::
:::column
The second column.
:::
::::

:::center
Centred content.
:::

:::small
Small content.
:::

:::large
Large content.
:::

:::pagebreak
:::

[^one]: A footnote body.
"#;

#[test]
fn showcase_builds_and_every_class_is_styled() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("dot.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><circle cx="4" cy="4" r="3"/></svg>"#).unwrap();
    std::fs::write(dir.path().join("showcase.md"), SHOWCASE).unwrap();
    assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"))
        .current_dir(dir.path())
        .args(["build", "showcase.md", "-o", "out.html"])
        .env_remove("RUST_LOG")
        .env_remove("SOURCE_DATE_EPOCH")
        .assert()
        .success();
    let text = std::fs::read_to_string(dir.path().join("out.html")).unwrap();
    assert!(text.contains("<style id=\"writer-theme\">"));
    assert!(text.contains("/* @default/styles/theme.css */"));
    let html = Html::parse_document(&text);
    assert_showcase_constructs(&html);
    let rules = rules(&css());
    let mut classes = BTreeSet::new();
    for element in html.select(&Selector::parse("*").unwrap()) {
        if ["style", "script"].contains(&element.value().name()) {
            continue;
        }
        classes.extend(element.value().classes());
    }
    for class in classes {
        if class.starts_with("hl-")
            || class.starts_with("language-")
            || class == "callout-remark"
        {
            continue;
        }
        assert!(
            has_class(&rules, class),
            "showcase class .{class} has no selector"
        );
    }
}

fn assert_showcase_constructs(html: &Html) {
    for selector in [
        "header.title-block",
        ".subtitle",
        ".authors",
        ".date",
        ".abstract",
        "nav.toc",
        "em",
        "strong",
        "del",
        ":not(pre) > code",
        "a[href=\"https://example.com\"]",
        ".footnote-ref",
        ".footnotes",
        ".task-list input[type=\"checkbox\"][disabled]",
        "ol[start=\"3\"]",
        "dl dt",
        "dl dd",
        "blockquote",
        "hr",
        "figure#fig-dot.kind-image",
        "figure.kind-table table",
        "th.align-left",
        "th.align-center",
        "th.align-right",
        "pre.numbered[style=\"counter-reset:line 4\"]",
        ".line.highlight",
        ".math.inline",
        "#eq-e.equation .math.display",
        ".callout-note",
        ".callout-tip",
        ".callout-important",
        ".callout-warning",
        ".callout-caution",
        ".callout-remark",
        ".columns > .column",
        "div.center",
        "div.small",
        "div.large",
        ".page-break",
    ] {
        assert!(
            html.select(&Selector::parse(selector).unwrap()).next().is_some(),
            "showcase lost {selector}"
        );
    }
}
