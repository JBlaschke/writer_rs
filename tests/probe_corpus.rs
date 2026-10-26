//! Differential comrak corpus from R9 §14 (T11).
//!
//! The main/footnote inputs and dumps are exact copies of R9's fenced blocks.
//! Isolated cases expand its `/`, `+`, and `\n` abbreviations, separating
//! variants with blank lines and giving abbreviated code fences a body/closer.
//! Link definitions follow the destinations R9 records. Option annotations
//! such as `[math_latex]` remain input; writer's options apply to every case.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};
use writer::config::MarkdownConfig;
use writer::error::Diagnostics;
use writer::markdown::comrak_options;
use writer::markdown::directives::{
    INVALID_DIRECTIVE, check_unopened_directives, colon_prepass,
};
use writer::markdown::recover::recover_attr_blocks;

const MAIN: &str = include_str!("fixtures/md/probe/main.md");
const FOOTNOTES: &str = include_str!("fixtures/md/probe/footnotes.md");

// R9 §14.1's original probe options intentionally differ from writer's.
fn r9_probe_options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.front_matter_delimiter = Some("---".into());
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.tasklist = true;
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.extension.description_lists = true;
    o.extension.alerts = true;
    o.extension.math_dollars = true;
    o.extension.math_code = true;
    o.extension.block_directive = true;
    o.extension.header_attributes = true;
    o.extension.fenced_code_attributes = true;
    o.extension.link_attributes = true;
    o.extension.inline_code_attributes = true;
    o.extension.superscript = true;
    o.extension.subscript = true;
    o.parse.smart = false;
    o.parse.relaxed_autolinks = false;
    o.render.r#unsafe = true;
    o.render.escape = false;
    o.render.hardbreaks = false;
    o.render.github_pre_lang = false;
    o.render.full_info_string = false;
    o.render.sourcepos = true;
    o
}

// R9 §14.1's dumper: Debug payloads, attributes, byte columns and source lines.
fn dump<'a>(node: &'a AstNode<'a>, depth: usize, out: &mut String) {
    let ast = node.data();
    let indent = "  ".repeat(depth);
    let kind = ast.value.xml_node_name();
    let payload = match &ast.value {
        NodeValue::Document
        | NodeValue::BlockQuote
        | NodeValue::DescriptionList
        | NodeValue::DescriptionTerm
        | NodeValue::DescriptionDetails
        | NodeValue::Paragraph
        | NodeValue::ThematicBreak
        | NodeValue::TableCell
        | NodeValue::SoftBreak
        | NodeValue::LineBreak
        | NodeValue::Emph
        | NodeValue::Strong
        | NodeValue::Strikethrough
        | NodeValue::Superscript
        | NodeValue::Subscript
        | NodeValue::Underline
        | NodeValue::Highlight
        | NodeValue::Insert
        | NodeValue::SpoileredText
        | NodeValue::Escaped
        | NodeValue::Subtext => String::new(),
        v => format!(" {v:?}"),
    };
    let attrs = match &ast.attrs {
        Some(a) => format!(" attrs={a:?}"),
        None => String::new(),
    };
    out.push_str(&format!(
        "{indent}{kind}{payload}{attrs} @{}\n",
        ast.sourcepos
    ));
    drop(ast);
    for child in node.children() {
        dump(child, depth + 1, out);
    }
}

fn dump_document(text: &str, options: &Options<'_>) -> String {
    let arena = Arena::new();
    let root = parse_document(&arena, text, options);
    let mut out = String::new();
    dump(root, 0, &mut out);
    out
}

fn markdown_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md")
}

fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    files.sort();
    files
}

fn probe_files() -> Vec<PathBuf> {
    let probe = markdown_dir().join("probe");
    let mut files = markdown_files(&probe);
    let cases = markdown_files(&probe.join("cases"));
    assert_eq!(files.len(), 2);
    assert_eq!(cases.len(), 33);
    files.extend(cases);
    files
}

#[test]
fn main_matches_r9_dump() {
    assert_eq!(
        dump_document(MAIN, &r9_probe_options()),
        include_str!("fixtures/md/probe/main.r9-ast.txt")
    );
}

#[test]
fn footnotes_match_r9_dump() {
    let mut options = r9_probe_options();
    options.parse.leave_footnote_definitions = true;
    assert_eq!(
        dump_document(FOOTNOTES, &options),
        include_str!("fixtures/md/probe/footnotes.r9-ast.txt")
    );
}

#[test]
fn corpus_snapshots() {
    let options = comrak_options(&MarkdownConfig::default());
    for path in probe_files() {
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let name = if path.parent().unwrap().ends_with("cases") {
            format!("case-{stem}")
        } else {
            stem.to_owned()
        };
        let text = std::fs::read_to_string(&path).unwrap();
        insta::assert_snapshot!(name, dump_document(&text, &options));
    }
}

#[test]
fn config_toggles_change_the_ast() {
    let config = MarkdownConfig {
        superscript: true,
        subscript: true,
        smart_punctuation: true,
        ..MarkdownConfig::default()
    };
    let ast = dump_document(MAIN, &comrak_options(&config));
    assert!(ast.contains("    superscript @"));
    assert!(ast.contains("    subscript @"));
    assert_ne!(
        ast,
        dump_document(MAIN, &comrak_options(&MarkdownConfig::default()))
    );
    insta::assert_snapshot!("main-toggles", ast);
}

#[derive(Default)]
struct FenceLines {
    boundaries: BTreeSet<usize>,
    excluded: BTreeSet<usize>,
}

fn fence_lines<'a>(root: &'a AstNode<'a>) -> FenceLines {
    let mut lines = FenceLines::default();
    for node in root.descendants() {
        let ast = node.data();
        let start = ast.sourcepos.start.line;
        let end = ast
            .sourcepos
            .end
            .line
            .saturating_sub(usize::from(ast.sourcepos.end.column == 0))
            .max(start);
        match &ast.value {
            NodeValue::BlockDirective(_) => {
                lines.boundaries.insert(start);
                lines.boundaries.insert(end);
            }
            NodeValue::FrontMatter(_) | NodeValue::HtmlBlock(_) => {
                lines.excluded.extend(start..=end);
            }
            NodeValue::CodeBlock(code) if code.fenced => {
                lines.excluded.extend(start..=end);
            }
            // Named exception: an inline math/code span can contain an entire
            // fence-like source line. It is literal content, not a directive.
            NodeValue::Math(_) | NodeValue::Code(_) => {
                lines.excluded.extend(start..=end);
            }
            _ => {}
        }
    }
    lines
}

fn assert_fences_accounted_for(path: &Path) {
    let text = std::fs::read_to_string(path).unwrap();
    let options = comrak_options(&MarkdownConfig::default());
    let (rewritten, _) = colon_prepass(&text, &options);
    assert_eq!(text.lines().count(), rewritten.lines().count());
    let arena = Arena::new();
    let root = parse_document(&arena, &rewritten, &options);
    let mut diagnostics = Diagnostics::new();
    let (root, recovered) = recover_attr_blocks(
        &arena, root, &rewritten, &options, &mut diagnostics,
    );
    assert_eq!(text.lines().count(), recovered.lines().count());
    check_unopened_directives(root, &mut diagnostics);
    let lines = fence_lines(root);
    let fence = regex::Regex::new(r"^\s{0,3}:{3,}").unwrap();
    for (index, line) in recovered.lines().enumerate() {
        let number = index + 1;
        if !fence.is_match(line) || lines.excluded.contains(&number) {
            continue;
        }
        let reported = diagnostics.iter().any(|diagnostic| {
            diagnostic.code == INVALID_DIRECTIVE
                && diagnostic.span.as_ref().is_some_and(|span| {
                    (span.start_line..=span.end_line).contains(&number)
                })
        });
        assert!(
            lines.boundaries.contains(&number) || reported,
            "{}:{number}: unaccounted fence {line:?}; {diagnostics:?}",
            path.display()
        );
    }
}

#[test]
fn fence_lines_are_accounted_for() {
    let mut files = markdown_files(&markdown_dir());
    files.extend(probe_files());
    for path in files {
        assert_fences_accounted_for(&path);
    }
}
