//! Shared source corpus contracts for later frontend and emitter tasks (T01).

use std::path::{Path, PathBuf};

const MARKDOWN: [&str; 16] = [
    "minimal.md", "attributes.md", "attribute-recovery.md", "directives.md",
    "pandoc-directives.md", "figures.md", "tables.md", "callouts.md",
    "code.md", "math.md", "references.md", "footnotes.md", "includes.md",
    "raw.md", "pagebreaks.md", "presenter-deck.md",
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn markdown(name: &str) -> String {
    std::fs::read_to_string(fixtures().join("md").join(name)).unwrap()
}

fn front_matter(text: &str) -> (&str, &str) {
    let rest = text.strip_prefix("---\n").expect("front matter comes first");
    rest.split_once("\n---\n").expect("front matter is closed")
}

#[test]
fn markdown_fixtures_exist_and_follow_conventions() {
    for name in MARKDOWN {
        let text = markdown(name);
        assert!(text.ends_with('\n'), "{name}: missing final newline");
        assert!(!text.contains('\r'), "{name}: use LF line endings");
        let (header, body) = front_matter(&text);
        let value: serde_norway::Value =
            serde_norway::from_str(header).unwrap();
        assert!(value["title"].as_str().is_some(), "{name}: missing title");
        assert!(
            header.contains(&format!("# {name}: markdown-dialect.md §")),
            "{name}: missing contract reference"
        );
        assert!(!body.trim().is_empty(), "{name}: empty document");
    }
    let (header, body) = front_matter(include_str!("fixtures/md/minimal.md"));
    let value: serde_norway::Value = serde_norway::from_str(header).unwrap();
    assert_eq!(value["authors"].as_sequence().unwrap().len(), 2);
    assert!(value["abstract"].as_str().is_some());
    assert_eq!(body.lines().filter(|line| line.starts_with("# ")).count(), 1);
    assert_eq!(body.trim().split("\n\n").count(), 2);
}

#[test]
fn fixture_cases_are_present() {
    let cases: &[(&str, &[&str])] = &[
        (
            "attributes.md",
            &[".unnumbered", "title=", "`inline code`{", "{#\"with space\"}"],
        ),
        (
            "attribute-recovery.md",
            &[
                "width='50%'", "path=images/plot.png", "highlight=1-3|5",
                "{bare}", ".first,.second", "{.a}{k='v'}",
            ],
        ),
        (
            "directives.md",
            &[
                "::::note",
                ":::tip",
                ":::pagebreak",
                ":::toc depth=2",
                ":::bibliography",
                ":::raw format=latex",
                ":::mystery",
                ":::notes",
                ":::comment",
                "\n:::\n",
                ":::warning title=\"An unclosed directive\"",
            ],
        ),
        (
            "pandoc-directives.md",
            &[
                "::: {.note title=",
                "::: {.pagebreak}",
                "\n:::\n",
                "A title: with a colon",
                ":::callout {title=\"Fig 1: x\"}",
            ],
        ),
        (
            "figures.md",
            &[
                "Caption with *emph*",
                "\"The title\"",
                "#fig:x width=50%",
                "#fig:y width='50%'",
                "{src=images/diagram.svg}",
                "{#fig:z .cls} trailing text",
                ") ![Two]",
                "){.bare}",
                "[![nested img]",
                "){.x width=10%}",
                "![](images/plot.png)\n",
                "#fig-panels caption=\"Two panels\"",
                "#fig-left",
                "#fig-right",
                "{width=huge}",
            ],
        ),
        (
            "tables.md",
            &[
                "|:-----|:------:|------:|", "Table: A caption before",
                "Table: A caption after", "Table: An orphan caption",
                "\n: A pandoc caption", ":::figure {#tbl-wrapped",
            ],
        ),
        (
            "callouts.md",
            &[
                "[!NOTE]", "[!TIP] Useful advice", "[!IMPORTANT]",
                "[!WARNING]", "[!CAUTION]", "[!nOtE]", "[!UNKNOWN]",
                ":::callout warning title=",
            ],
        ),
        (
            "code.md",
            &[
                "```python",
                ".plain",
                ".numbered",
                "start=10",
                "highlight=\"1-3,5\"",
                "lines=\"1-3|5\"",
                "#lst-x",
                "caption=",
                "include=",
                ".custom",
                "trim=true",
                "start=0",
                "start=x",
                "highlight=\"3-1\"",
                "lines=\"a\"",
                "highlight=\"1,,2\"",
                "~~~rust",
                "    plain indented code",
                "\n```\nA fence without a language.",
            ],
        ),
        (
            "references.md",
            &[
                "@sec-x",
                "@fig-x[Fig.]",
                "@eq-x{.page}",
                "@eq-x[]",
                "@sec-x[Chapter]",
                "@sec-unnumbered",
                "\\@literal",
                "@x.",
                "[@doe2020; @lee2019, p. 12]{.prose}",
                "[-@knuth1984]",
                "[see @doe2020]",
                "[the results](#results-and-discussion)",
                "## Results and discussion",
                "[@fig-a; @fig-b]",
                "{#fn-1}",
            ],
        ),
        (
            "footnotes.md",
            &[
                "[^a]", "^[Inline content.]", "[^b]", "[^unused]:",
                "    A second paragraph",
            ],
        ),
        (
            "includes.md",
            &[
                ":::include include/part-a.md",
                ":::include include/cycle-a.md", "  :::include",
            ],
        ),
        (
            "raw.md",
            &[
                "{=html}", "{=latex}", "{=typst}", "{=openxml}", "{=weird}",
                "<img src=\"images/plot.png\">", "<!--", "`{=html}",
            ],
        ),
        (
            "pagebreaks.md",
            &[
                ":::pagebreak", ":::toc depth=2", ":::bibliography", "\n***\n",
                "\n---\n",
            ],
        ),
        (
            "presenter-deck.md",
            &[
                "\n---\n",
                "\n--\n",
                "--- {layout=title}",
                ".r-fit-text",
                "layout=title background=red transition=fade",
                "data-state=keep",
                "aria-label=",
                ":::notes",
                ":::fragment",
                "<!-- .slide:",
            ],
        ),
    ];
    for (name, snippets) in cases {
        let text = markdown(name);
        for snippet in *snippets {
            assert!(text.contains(snippet), "{name}: missing {snippet:?}");
        }
    }
    assert!(!markdown("directives.md").trim_end().ends_with(":::"));
    assert!(
        markdown("tables.md")
            .contains("| a      | b       |\n: A pandoc caption")
    );
    assert_eq!(
        markdown("references.md").matches("{#sec-duplicate}").count(),
        2
    );
    assert!(markdown("footnotes.md").matches("[^a]").count() >= 3);
}

#[test]
fn math_fixture_holds_the_math_md_sources() {
    let text = markdown("math.md");
    let sources = [
        r"$\frac{a}{b}$",
        r"$$\sum_{i=0}^{n} i$$ {#eq-sum}",
        "@eq-sum",
        "```math {#eq-aligned}",
        r"\begin{aligned} a &= b + c \\ &= d \end{aligned}",
        r"$\hat{x} \vec{v} \dot{y}$",
        r"$$x = 1 \tag{3} \label{eq:t}$$",
        r"$\RR^n + \half{x}$",
        r"\$5",
        "# Energy $E = mc^2$",
        "- Math inside a list",
        "trailing prose {#eq-misplaced}",
    ];
    for source in sources {
        assert!(text.contains(source), "math.md: missing {source:?}");
    }
    let (header, _) = front_matter(&text);
    let value: serde_norway::Value = serde_norway::from_str(header).unwrap();
    let macros = &value["math"]["katex_options"]["macros"];
    assert_eq!(macros["\\RR"].as_str(), Some("\\mathbb{R}"));
    assert_eq!(macros["\\half"].as_str(), Some("\\frac{#1}{2}"));
}

#[test]
fn include_tree_is_complete() {
    let root = fixtures().join("md/include");
    let part_a = std::fs::read_to_string(root.join("part-a.md")).unwrap();
    let part_b = std::fs::read_to_string(root.join("part-b.md")).unwrap();
    let cycle_a = std::fs::read_to_string(root.join("cycle-a.md")).unwrap();
    let cycle_b = std::fs::read_to_string(root.join("cycle-b.md")).unwrap();
    assert!(part_a.contains("title: Part A"));
    assert!(part_a.contains(":::include part-b.md"));
    assert!(part_b.starts_with("# Part B\n"));
    assert!(part_b.contains("![Part B](../images/plot.png){#fig-part-b}"));
    assert!(cycle_a.contains(":::include cycle-b.md"));
    assert!(cycle_b.contains(":::include cycle-a.md"));
    let parent_title = front_matter(include_str!("fixtures/md/includes.md")).0;
    assert!(!parent_title.contains("title: Part A"));
    assert!(root.join("../images/plot.png").is_file());
}

#[test]
fn images_exist() {
    let png = std::fs::read(fixtures().join("md/images/plot.png")).unwrap();
    assert_eq!(png.len(), 70);
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(&blake3::hash(&png).to_hex()[..12], "32debefb8003");
    let svg = std::fs::read_to_string(fixtures().join("md/images/diagram.svg"))
        .unwrap();
    assert!(svg.len() <= 300);
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.ends_with("</svg>\n"));
}

#[test]
fn refs_bib_has_the_r5_entries() {
    let bib = std::fs::read_to_string(fixtures().join("bib/refs.bib")).unwrap();
    for entry in [
        "@article{doe2020,", "@book{knuth1984,", "@inproceedings{lee2019,",
        "{\\TeX}book",
    ] {
        assert!(bib.contains(entry), "missing bibliography entry {entry}");
    }
    let research = include_str!("../docs/research/R5-citations-crossrefs.md");
    let expected = research
        .split_once("const BIB: &str = r#\"\n")
        .unwrap()
        .1
        .split_once("\"#;")
        .unwrap()
        .0;
    assert_eq!(bib, expected);
}

#[test]
fn no_writer_toml_above_fixtures() {
    for ancestor in fixtures().ancestors() {
        assert!(
            !ancestor.join("writer.toml").exists(),
            "configuration above fixtures: {}",
            ancestor.display()
        );
    }
}
