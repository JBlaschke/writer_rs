//! Structured Typst frontend corpus and contract tests (typst-dialect.md §§2–3).
//! No frontend internals or external Typst process are used.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use writer::config::Config;
use writer::error::{Diagnostics, Level};
use writer::ir::{
    self, Block, Document, LabelKind, Parsed, SourceInfo, SourceKind,
};
use writer::pipeline::{self, BuildOptions, WorldSlot};
use writer::resolve::{ResolveOptions, resolve};
use writer::typst::frontend;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/typ")
}

fn source(path: &Path) -> SourceInfo {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    SourceInfo {
        base_dir: path.parent().unwrap().to_path_buf(),
        path,
        kind: SourceKind::Typst,
    }
}

fn parse_at(path: &Path, text: &str, config: &Config) -> Parsed {
    let parsed = frontend::parse(&source(path), text, config).unwrap();
    assert!(
        parsed.document.invariant_violations().is_empty(),
        "{:?}",
        parsed.document.invariant_violations()
    );
    parsed
}

fn parse(text: &str) -> Parsed {
    parse_at(&fixtures().join("inline.typ"), text, &Config::default())
}

fn clean(text: &str) -> Parsed {
    let parsed = parse(text);
    assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
    parsed
}

fn parse_fixture(name: &str) -> Parsed {
    let path = fixtures().join(format!("{name}.typ"));
    parse_at(&path, &fs::read_to_string(&path).unwrap(), &Config::default())
}

fn codes(diagnostics: &Diagnostics) -> Vec<&str> {
    diagnostics.iter().map(|diagnostic| diagnostic.code).collect()
}

fn json_doc(parsed: &Parsed) -> Value {
    serde_json::to_value(&parsed.document).unwrap()
}

fn nodes<'a>(value: &'a Value, kind: &str) -> Vec<&'a Value> {
    fn visit<'a>(value: &'a Value, kind: &str, found: &mut Vec<&'a Value>) {
        match value {
            Value::Object(map) => {
                if map.get("type").and_then(Value::as_str) == Some(kind) {
                    found.push(value);
                }
                for child in map.values() {
                    visit(child, kind, found);
                }
            }
            Value::Array(items) => {
                for child in items {
                    visit(child, kind, found);
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    visit(value, kind, &mut result);
    result
}

fn paragraphs(document: &Document) -> Vec<String> {
    let mut result = Vec::new();
    ir::walk_blocks(document, &mut |block| {
        if let Block::Paragraph { inlines, .. } = block {
            result.push(ir::plain_text(inlines));
        }
    });
    result
}

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in files {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

fn parse_file(path: &Path) -> Parsed {
    parse_at(path, &fs::read_to_string(path).unwrap(), &Config::default())
}

/// Asset hashes include absolute file paths. Assign stable IDs from first use,
/// then inventory; redact source paths without rewriting verbatim math/code.
fn stable_document(document: &Document) -> Value {
    let ids = document
        .assets
        .iter()
        .map(|asset| asset.id.clone())
        .collect::<Vec<_>>();
    let mut labels = BTreeMap::new();
    let mut content =
        serde_json::to_value((&document.meta.abstract_, &document.blocks))
            .unwrap();
    relabel(&mut content, &ids, &mut labels);
    let mut value = serde_json::to_value(document).unwrap();
    relabel(&mut value, &ids, &mut labels);
    redact(&mut value, "");
    if let Some(assets) = value.get_mut("assets").and_then(Value::as_array_mut)
    {
        assets.sort_by_key(|asset| asset["id"].as_str().map(str::to_string));
    }
    value
}

fn relabel(
    value: &mut Value, ids: &[String], labels: &mut BTreeMap<String, String>,
) {
    match value {
        Value::String(text) => {
            for id in ids {
                if text.contains(id) {
                    let next = format!("{:012x}", labels.len() + 1);
                    let label = labels.entry(id.clone()).or_insert(next);
                    *text = text.replace(id, label);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                relabel(item, ids, labels);
            }
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                relabel(value, ids, labels);
            }
        }
        _ => {}
    }
}

fn redact(value: &mut Value, key: &str) {
    let roots = redaction_roots(&[
        fs::canonicalize(fixtures()).unwrap().to_string_lossy().into_owned(),
        fixtures().to_string_lossy().into_owned(),
    ]);
    redact_with_roots(value, key, &roots);
}

fn redaction_roots(paths: &[String]) -> Vec<String> {
    let mut roots = paths
        .iter()
        .flat_map(|path| [path.clone(), path.replace('\\', "/")])
        .collect::<Vec<_>>();
    // Canonical Windows prefixes contain the ordinary path as a suffix.
    // Replace longer roots first so the extended prefix cannot be left behind.
    roots.sort_by(|left, right| {
        right.len().cmp(&left.len()).then(left.cmp(right))
    });
    roots.dedup();
    roots
}

fn redact_with_roots(value: &mut Value, key: &str, roots: &[String]) {
    match value {
        Value::String(text) => {
            for root in roots {
                *text = text.replace(root, "[typ]");
            }
            if matches!(
                key,
                "path" | "file" | "base_dir" | "includes" | "origin"
            ) || text.starts_with("[typ]")
            {
                *text = text.replace('\\', "/");
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_with_roots(item, key, roots);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                redact_with_roots(value, key, roots);
            }
        }
        _ => {}
    }
}

#[test]
fn snapshot_redaction_handles_extended_windows_paths_before_their_suffixes() {
    let ordinary = r"D:\a\writer\tests\fixtures\typ";
    let extended = r"\\?\D:\a\writer\tests\fixtures\typ";
    let roots = redaction_roots(&[ordinary.into(), extended.into()]);
    let mut value = json!({
        "path": format!(r"{extended}\subset.typ"),
        "file": format!(r"{extended}\include\part.typ"),
        "base_dir": ordinary,
        "includes": [format!(r"{extended}\include\part.typ")],
        "origin": format!(r"file:{extended}\image.png"),
        "forward": format!("{}/subset.typ", extended.replace('\\', "/")),
        "source": r"a & = b \ c & = d",
    });
    redact_with_roots(&mut value, "", &roots);
    assert_eq!(
        value,
        json!({
            "path": "[typ]/subset.typ", "file": "[typ]/include/part.typ",
            "base_dir": "[typ]", "includes": ["[typ]/include/part.typ"],
            "origin": "file:[typ]/image.png", "forward": "[typ]/subset.typ",
            "source": r"a & = b \ c & = d",
        })
    );
}

#[test]
fn corpus_has_reviewed_unresolved_ir_and_diagnostic_snapshots() {
    for name in
        ["minimal", "subset", "math", "includes", "syntax-error", "unsupported"]
    {
        let parsed = parse_fixture(name);
        assert!(parsed.document.resolved.is_empty());
        insta::assert_json_snapshot!(name, stable_document(&parsed.document));
        let mut diagnostics =
            serde_json::to_value(&parsed.diagnostics).unwrap();
        redact(&mut diagnostics, "");
        insta::assert_json_snapshot!(
            format!("{name}-diagnostics"),
            diagnostics
        );
        for diagnostic in parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.contains("-TYP-"))
        {
            assert!(diagnostic.path.is_some(), "{diagnostic:?}");
            assert!(diagnostic.span.is_some(), "{diagnostic:?}");
            assert!(
                diagnostic.message.contains(" (col ")
                    && diagnostic.message.ends_with(')'),
                "{diagnostic:?}"
            );
        }
    }
}

#[test]
fn syntax_errors_empty_the_tree_but_parser_warnings_keep_supported_content() {
    let error = parse_fixture("syntax-error");
    assert!(codes(&error.diagnostics).contains(&"E-TYP-SYNTAX"));
    assert!(error.document.blocks.is_empty());
    let text = "```c++\nint main() {}\n```\n\nAfter the warning.\n";
    let warning = parse(text);
    assert!(codes(&warning.diagnostics).contains(&"W-TYP-SYNTAX"));
    assert!(!warning.document.blocks.is_empty());
    assert!(
        paragraphs(&warning.document)
            .iter()
            .any(|text| text == "After the warning.")
    );
    let mut config = Config::default();
    config.typst.strict = true;
    let strict = parse_at(&fixtures().join("warning.typ"), text, &config);
    assert!(
        strict
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "W-TYP-SYNTAX"
                && diagnostic.level == Level::Error)
    );
    assert!(!strict.document.blocks.is_empty());
}

#[test]
fn diagnostic_columns_count_unicode_characters_instead_of_bytes() {
    let parsed = parse("éé #mystery()");
    let diagnostic = parsed
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E-TYP-CALL-UNKNOWN")
        .unwrap();
    assert!(diagnostic.message.ends_with(" (col 5)"), "{diagnostic:?}");
    let span = diagnostic.span.as_ref().unwrap();
    assert_eq!((span.start_line, span.end_line, span.cell), (1, 1, None));
    let multiline = parse("Before.\n#mystery(\n [body]\n)\n");
    let diagnostic = multiline
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E-TYP-CALL-UNKNOWN")
        .unwrap();
    let span = diagnostic.span.as_ref().unwrap();
    assert_eq!((span.start_line, span.end_line), (2, 4));
}

#[test]
fn unsupported_constructs_have_the_frozen_codes_and_keep_surrounding_content() {
    let parsed = parse_fixture("unsupported");
    let got = codes(&parsed.diagnostics).into_iter().collect::<BTreeSet<_>>();
    for code in [
        "E-TYP-CODE-UNSUPPORTED", "E-TYP-CALL-UNKNOWN", "E-TYP-IDENT-UNKNOWN",
        "E-TYP-ARG-INVALID", "E-TYP-ARG-SPREAD", "E-TYP-TEMPLATE-ARG",
        "E-TYP-INCLUDE-DYNAMIC", "W-TYP-FIGURE-KIND", "W-TYP-ARG-IGNORED",
        "W-TYP-SET-IGNORED", "W-TYP-SHOW-IGNORED", "W-TYP-LET-IGNORED",
        "W-TYP-IMPORT-IGNORED", "W-TYP-LABEL-ORPHAN", "W-TYP-LABEL-IGNORED",
        "W-TYP-TABLE-CELL", "W-TYP-PACKAGE-VERSION", "W-TYP-PATH-STRING",
    ] {
        assert!(got.contains(code), "missing {code}: {got:?}");
    }
    assert!(
        paragraphs(&parsed.document)
            .iter()
            .any(|text| text == "A supported paragraph survives.")
    );
}

#[test]
fn paragraph_runs_trim_edges_preserve_breaks_and_ignore_statement_whitespace() {
    let parsed = clean(
        "  First *strong* _emphasis_\nnext \\\nline.  \n\n#let name = \"bound\";\n#name\n",
    );
    assert_eq!(parsed.document.blocks.len(), 2);
    let value = json_doc(&parsed);
    let first = &value["blocks"][0]["inlines"];
    assert_eq!(nodes(first, "strong").len(), 1);
    assert_eq!(nodes(first, "emph").len(), 1);
    assert_eq!(nodes(first, "soft_break").len(), 1);
    assert_eq!(nodes(first, "line_break").len(), 1);
    assert_eq!(paragraphs(&parsed.document)[1], "bound");
    assert!(!paragraphs(&parsed.document)[0].starts_with(' '));
    assert!(!paragraphs(&parsed.document)[0].ends_with(' '));
}

#[test]
fn smart_quotes_use_context_for_closing_apostrophes_and_primes() {
    let parsed = clean(
        "\"well-\" and 5' and x\"y\"\n\nwriter's and 6\" and \"outer 'inner'\"\n",
    );
    let text = paragraphs(&parsed.document);
    assert_eq!(text[0], "“well-” and 5′ and x“y”");
    assert_eq!(text[1], "writer’s and 6″ and “outer ‘inner’”");
}

#[test]
fn shorthands_and_raw_text_are_decoded_without_evaluation() {
    let parsed = clean(
        "A~B -- C --- D ... E -? F \\*literal\\*.\n\n```python\n  x = 1\n  y = 2\n```\n",
    );
    let text = paragraphs(&parsed.document).join("\n");
    assert!(
        text.contains("A\u{a0}B – C — D … E \u{ad} F *literal*."),
        "{text}"
    );
    let value = json_doc(&parsed);
    let blocks = nodes(&value, "code_block");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["lang"], "python");
    // A closing delimiter at column zero keeps the two-space body indent.
    assert_eq!(blocks[0]["text"], "  x = 1\n  y = 2");
    let indented = clean("```python\n  x = 1\n  y = 2\n  ```\n");
    let value = json_doc(&indented);
    assert_eq!(nodes(&value, "code_block")[0]["text"], "x = 1\ny = 2");
}

#[test]
fn lists_group_by_kind_keep_starts_and_loosen_for_nonparagraph_blocks() {
    let parsed = clean(
        "- one\n- two\n\n+ three\n+ four\n\n/ Term: Meaning.\n/ Other: Definition.\n\n#enum(start: 7, tight: true, [A], [B])\n\n#list(tight: true, [#heading[Inside]])\n",
    );
    let value = json_doc(&parsed);
    let lists = nodes(&value, "list");
    assert_eq!(lists.len(), 4);
    assert_eq!(lists[0]["kind"]["kind"], "bullet");
    assert_eq!(lists[1]["kind"]["start"], 1);
    assert_eq!(lists[2]["kind"]["start"], 7);
    assert_eq!(lists[3]["tight"], false);
    assert_eq!(
        nodes(&value, "definition_list")[0]["items"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn labels_cross_paragraph_breaks_and_metadata_anchors_choose_div_or_span() {
    let parsed = clean(
        "= Heading\n\n<sec-heading>\n\n#metadata(none) <anchor-block>\n\nText #metadata(none) <anchor-inline> tail.\n",
    );
    let value = json_doc(&parsed);
    assert_eq!(nodes(&value, "heading")[0]["attrs"]["id"], "sec-heading");
    let divs = nodes(&value, "div");
    assert!(divs.iter().any(|node| node["attrs"]["id"] == "anchor-block"
        && node["blocks"] == json!([])));
    assert!(
        nodes(&value, "span")
            .iter()
            .any(|node| node["attrs"]["id"] == "anchor-inline")
    );
    let bad = parse("= Heading\n#label(\"bad label\")\n");
    assert!(codes(&bad.diagnostics).contains(&"label-invalid"));
}

#[test]
fn forward_notes_and_repeats_strip_the_writer_prefix_exactly_once() {
    let parsed = clean(
        "Before @writer-fn-note.\n\nNote#footnote[Body.] <writer-fn-note>; again #footnote(<writer-fn-note>).\n\n#footnote[Double prefix.] <writer-fn-writer-fn-two>\n\n@writer-fn-writer-fn-two\n",
    );
    let value = json_doc(&parsed);
    let definitions = nodes(&value, "footnote");
    assert_eq!(definitions.len(), 2);
    assert_eq!(definitions[0]["label"], "note");
    assert_eq!(definitions[1]["label"], "writer-fn-two");
    let refs = nodes(&value, "footnote_ref");
    assert_eq!(refs.len(), 3);
    assert_eq!(refs[0]["label"], "note");
    assert_eq!(refs[1]["label"], "note");
    assert_eq!(refs[2]["label"], "writer-fn-two");
    assert!(parsed.document.features.footnotes);
}

#[test]
fn ref_supplements_keep_none_empty_content_and_page_form_distinct() {
    let parsed = clean(
        "@sec-x @sec-x[] @sec-x[Section]\n\n#ref(<sec-x>, form: \"page\")\n",
    );
    let value = json_doc(&parsed);
    let refs = nodes(&value, "ref");
    assert_eq!(refs.len(), 4);
    assert_eq!(refs[0]["supplement"], Value::Null);
    assert_eq!(refs[1]["supplement"], json!([]));
    assert_eq!(refs[2]["supplement"][0]["text"], "Section");
    assert_eq!(refs[3]["form"], "page");
    assert!(refs.iter().all(|node| node.get("resolved").is_none()));
}

#[test]
fn all_citation_modes_and_space_only_grouping_are_preserved() {
    let parsed = clean(
        "#cite(<doe2020>) #cite(<knuth1984>, supplement: [p. 3])\n\n#cite(<doe2020>, form: \"prose\") #cite(<doe2020>, form: \"year\") #cite(<doe2020>, form: \"author\") #cite(<doe2020>, form: \"full\") #cite(<doe2020>, form: none)\n",
    );
    let value = json_doc(&parsed);
    let citations = nodes(&value, "cite");
    assert_eq!(citations.len(), 6);
    assert_eq!(citations[0]["items"].as_array().unwrap().len(), 2);
    assert_eq!(citations[0]["items"][1]["locator"], "p. 3");
    assert_eq!(
        citations
            .iter()
            .map(|cite| cite["mode"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["normal", "prose", "year", "author", "full", "none"]
    );
    assert!(parsed.document.features.citations);
}

#[test]
fn citation_groups_respect_source_statements_comments_and_content_boundaries() {
    for text in [
        "#cite(<a>) #let x = 1; #cite(<b>)",
        "#cite(<a>) /* separate */ #cite(<b>)",
        "#cite(<a>) #metadata(none) #cite(<b>)",
        "#let saved = [#cite(<a>)]\n#saved #cite(<b>)",
        "#cite(<a>) #[#cite(<b>)]",
    ] {
        let parsed = clean(text);
        let value = json_doc(&parsed);
        let citations = nodes(&value, "cite");
        assert_eq!(citations.len(), 2, "{text}");
        assert!(
            citations
                .iter()
                .all(|cite| { cite["items"].as_array().unwrap().len() == 1 }),
            "{text}"
        );
    }
}

#[test]
fn literal_bindings_are_ordered_scoped_and_content_is_walked() {
    let parsed = clean(
        "#let name = \"outer\"\n#name\n\n#block[#let name = \"inner\"; #name]\n\n#name\n\n#let amount = -2.5\n#amount\n",
    );
    assert_eq!(
        paragraphs(&parsed.document),
        ["outer", "inner", "outer", "-2.5"]
    );
    let forward = parse("#later\n#let later = \"now\"\n#later\n");
    assert!(codes(&forward.diagnostics).contains(&"E-TYP-IDENT-UNKNOWN"));
    assert!(
        paragraphs(&forward.document).iter().any(|text| text.contains("now"))
    );
    let content = clean("#let body = [A *bold* value.]\n#body\n");
    assert_eq!(nodes(&json_doc(&content), "strong").len(), 1);
}

#[test]
fn recognized_import_aliases_and_item_renames_resolve_without_loading_modules()
{
    let parsed = clean(
        "#import \"@writer/doc:0.1.0\" as writer\n#import \"@writer/doc:0.1.0\": callout as note\n#writer.callout[One.]\n#note[Two.]\n",
    );
    assert_eq!(nodes(&json_doc(&parsed), "callout").len(), 2);
    let unknown =
        parse("#import \"not-a-real-module.typ\": arbitrary\n#arbitrary()\n");
    assert_eq!(
        codes(&unknown.diagnostics),
        ["W-TYP-IMPORT-IGNORED", "E-TYP-CALL-UNKNOWN"]
    );
}

#[test]
fn minimal_metadata_covers_literal_kinds_and_preserves_unmapped_variable_names()
{
    let parsed = parse_fixture("minimal");
    assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
    let meta = &parsed.document.meta;
    assert_eq!(meta.title.as_deref(), Some("A structured document"));
    assert_eq!(meta.subtitle.as_deref(), Some("Literal metadata"));
    assert_eq!(meta.authors.len(), 2);
    assert_eq!(meta.authors[1].affiliation.as_deref(), Some("Institute"));
    assert_eq!(meta.date.as_deref(), Some("2026-10-28"));
    assert_eq!(meta.lang.as_deref(), Some("en-GB"));
    assert_eq!(meta.keywords, ["Typst", "IR"]);
    assert!(meta.abstract_.is_some());
    assert_eq!(meta.vars["toc-title"], "Contents");
    assert_eq!(meta.vars["cite_locale"], "en-GB");
    assert_eq!(meta.vars["pdf"]["system_fonts"], false);
    assert!(meta.vars.get("custom-value").is_some());
    assert!(meta.vars.get("custom_value").is_none());
    assert!(matches!(
        parsed.document.blocks.first(),
        Some(Block::Toc { depth: 2, span: None })
    ));
}

#[test]
fn metadata_and_sets_apply_in_order_without_mutating_the_callers_config() {
    let config = Config::default();
    let before = config.clone();
    let parsed = parse_at(
        &fixtures().join("order.typ"),
        "#set document(title: \"Before\")\n#show: writer-doc.with(title: \"Show\", toc: 2, pdf: (allow-packages: false), custom: (toc-title: \"verbatim\"))\n#set document(title: \"After\")\n#set text(lang: \"de\")\n#set heading(numbering: none)\n#set math.equation(numbering: \"(1)\")\n#set page(paper: \"letter\")\n#set bibliography(style: \"apa\")\n= Heading\n",
        &config,
    );
    assert_eq!(config, before);
    assert_eq!(parsed.document.meta.title.as_deref(), Some("After"));
    assert_eq!(parsed.document.meta.lang.as_deref(), Some("de"));
    let vars = &parsed.document.meta.vars;
    assert_eq!(vars["pdf"]["allow_packages"], false);
    assert_eq!(vars["custom"]["toc-title"], "verbatim");
    assert_eq!(vars["numbering"]["headings"], false);
    assert_eq!(vars["numbering"]["equations"], "all");
    assert_eq!(vars["paper"], "letter");
    assert_eq!(vars["cite_style"], "apa");
}

#[test]
fn invalid_shapes_nonliteral_template_values_and_unknown_named_arguments_are_diagnostics()
 {
    for text in [
        "#raw(42)",
        "#image(2)",
        "#heading(level: -1)[Bad]",
        "#list(tight: 2, [A])",
        "#table(columns: 1.5, [A])",
        "#table(columns: 2, table.cell(colspan: 0)[A])",
        "#code-cell(index: 1.5)[out]",
    ] {
        let parsed = parse(text);
        assert!(
            codes(&parsed.diagnostics).contains(&"E-TYP-ARG-INVALID"),
            "{text}: {:?}",
            parsed.diagnostics
        );
    }
    let template = parse(
        "#show: writer-doc.with(title: upper(\"dynamic\"), toc: \"bad\")\nText.",
    );
    assert!(codes(&template.diagnostics).contains(&"E-TYP-TEMPLATE-ARG"));
    assert!(codes(&template.diagnostics).contains(&"E-TYP-ARG-INVALID"));
    let unknown = parse("#heading(bogus: true)[Kept]\n");
    assert!(codes(&unknown.diagnostics).contains(&"W-TYP-ARG-IGNORED"));
    assert_eq!(nodes(&json_doc(&unknown), "heading").len(), 1);
}

#[test]
fn duplicate_toc_and_bibliography_warn_but_do_not_duplicate_synthetic_blocks() {
    let parsed = parse(
        "#show: writer-doc.with(toc: true, bibliography: path(\"refs.bib\"))\n#outline(depth: 4)\n#bibliography(path(\"refs.bib\"), style: \"apa\")\n",
    );
    assert_eq!(
        codes(&parsed.diagnostics)
            .iter()
            .filter(|code| **code == "W-TYP-DUPLICATE-BLOCK")
            .count(),
        2
    );
    let value = json_doc(&parsed);
    assert_eq!(nodes(&value, "toc").len(), 1);
    assert_eq!(nodes(&value, "bibliography").len(), 1);
    assert_eq!(nodes(&value, "toc")[0]["depth"], 4);
    assert_eq!(parsed.document.meta.vars["cite_style"], "apa");
}

#[test]
fn image_paths_dimensions_and_missing_resources_keep_the_asset_contract() {
    let parsed = clean(
        "#image(path(\"image.png\"), width: 70%, height: 2cm, alt: \"Plot\")\n\n#box(image(\"/image.png\", width: auto))\n",
    );
    assert_eq!(parsed.assets.len(), 1);
    let value = json_doc(&parsed);
    let images = nodes(&value, "image");
    assert_eq!(images.len(), 2);
    assert!(images[0]["url"].as_str().unwrap().starts_with("asset:"));
    assert_eq!(images[0]["url"], images[1]["url"]);
    assert_eq!(images[0]["attrs"]["pairs"]["width"], "70%");
    assert_eq!(images[0]["attrs"]["pairs"]["height"], "2cm");
    assert!(images[1]["attrs"]["pairs"].get("width").is_none());
    let missing = parse("#image(\"missing.png\", alt: \"Missing\")");
    assert!(missing.assets.is_empty());
    assert!(codes(&missing.diagnostics).contains(&"missing-asset"));
    assert_eq!(nodes(&json_doc(&missing), "image")[0]["url"], "missing.png");
}

#[test]
fn figure_kinds_are_inferred_then_downgraded_to_preserve_exact_body_shapes() {
    let parsed = clean(
        "#figure(image(\"image.png\"))\n#figure([#image(\"image.png\")])\n#figure(table(columns: 1, [A]))\n#figure(raw(\"x\", block: true))\n#figure([$ x $])\n#figure([A paragraph], kind: image)\n#figure([#figure(image(\"image.png\"))], kind: image)\n",
    );
    let value = json_doc(&parsed);
    let figures = nodes(&value, "figure");
    assert_eq!(
        figures
            .iter()
            .map(|figure| figure["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "image", "image", "table", "listing", "other", "other", "other",
            "image"
        ]
    );
}

#[test]
fn a_labelled_bare_table_is_an_unnumbered_wrapper_with_an_other_label() {
    let mut parsed = clean(
        "#table(columns: 1, [A]) <tbl-bare>\n\n#figure(table(columns: 1, [B]), caption: [Counted]) <tbl-counted>\n",
    );
    let value = json_doc(&parsed);
    assert_eq!(value["blocks"][0]["type"], "figure");
    assert_eq!(value["blocks"][0]["kind"], "table");
    assert_eq!(value["blocks"][0]["attrs"]["id"], Value::Null);
    assert!(
        value["blocks"][0]["attrs"]["classes"]
            .as_array()
            .unwrap()
            .contains(&json!("unnumbered"))
    );
    assert_eq!(value["blocks"][0]["body"][0]["attrs"]["id"], "tbl-bare");
    let mut config = Config::default();
    config.merge_front_matter(&parsed.document.meta).unwrap();
    resolve(
        &mut parsed.document,
        &ResolveOptions { config: &config, library: None, lenient: true },
    );
    assert_eq!(
        parsed.document.resolved.labels["tbl-bare"].kind,
        LabelKind::Other
    );
    assert_eq!(
        parsed.document.resolved.labels["tbl-counted"].number.as_deref(),
        Some("1")
    );
}

#[test]
fn table_tracks_alignments_sections_and_spans_form_a_valid_grid() {
    let parsed = clean(
        "#table(columns: (1fr, 2fr, 1fr), align: (left + top, center, right), table.header([H1], [H2], [H3]), table.cell(colspan: 2)[Wide], [End], table.cell(rowspan: 2, align: right + bottom)[Tall], [B], [C], [D], [E], table.footer([F1], [F2], [F3]))",
    );
    let value = json_doc(&parsed);
    let table = nodes(&value, "table")[0];
    let columns = table["columns"].as_array().unwrap();
    assert_eq!(
        columns
            .iter()
            .map(|column| column["width"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        [0.25, 0.5, 0.25]
    );
    assert_eq!(
        columns
            .iter()
            .map(|column| column["align"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["left", "center", "right"]
    );
    assert_eq!(table["head"][0]["cells"].as_array().unwrap().len(), 3);
    assert!(
        table["head"][0]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .all(|cell| cell["header"] == true)
    );
    assert_eq!(table["body"][0]["cells"][0]["colspan"], 2);
    assert_eq!(table["body"][1]["cells"][0]["rowspan"], 2);
    assert_eq!(table["body"][1]["cells"][0]["align"], "right");
    assert_eq!(table["body"][2]["cells"].as_array().unwrap().len(), 2);
    assert_eq!(table["foot"][0]["cells"].as_array().unwrap().len(), 3);
}

#[test]
fn table_cells_flatten_paragraphs_drop_blocks_warn_and_pad_the_last_row() {
    let parsed = parse(
        "#table(columns: (auto, 40%), table.cell(x: 0, y: 0)[First.\n\nSecond.\n\n= Dropped heading], [B], [Last])",
    );
    assert!(codes(&parsed.diagnostics).contains(&"W-TYP-TABLE-CELL"));
    assert!(codes(&parsed.diagnostics).contains(&"W-TYP-ARG-IGNORED"));
    let value = json_doc(&parsed);
    let table = nodes(&value, "table")[0];
    assert_eq!(table["columns"][0]["width"], Value::Null);
    assert!(
        (table["columns"][1]["width"].as_f64().unwrap() - 0.4).abs() < 1e-6
    );
    assert_eq!(nodes(&table["body"][0]["cells"][0], "line_break").len(), 1);
    assert_eq!(table["body"][1]["cells"].as_array().unwrap().len(), 2);
    assert_eq!(table["body"][1]["cells"][1]["inlines"], json!([]));
}

#[test]
fn equation_source_is_verbatim_including_code_and_matrix_delimiter_values() {
    let parsed = parse_fixture("math");
    assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
    let value = json_doc(&parsed);
    let equations = nodes(&value, "math");
    assert_eq!(equations.len(), 17);
    assert!(equations.iter().all(|equation| equation["dialect"] == "typst"));
    assert_eq!(equations[15]["source"], "mat(delim: #none, 1, 2; 3, 4)");
    assert_eq!(equations[16]["source"], "#text[kept code] + x");
    assert!(!codes(&parsed.diagnostics).contains(&"E-TYP-MATH-CODE"));
    assert!(parsed.document.features.math);
}

#[test]
fn mitex_removes_only_the_exact_leading_configured_macro_block() {
    use writer::math::latex_source::{
        macros_as_newcommands, macros_from_config,
    };
    let mut config = Config::default();
    config
        .math
        .katex_options
        .insert("macros".into(), json!({"\\R":"\\mathbb{R}"}));
    let prefix =
        macros_as_newcommands(&macros_from_config(&config.math.katex_options));
    let remaining = "\\R + 1";
    let src = format!(
        "#import \"@preview/mitex:0.2.7\": mi, mitex\n#mi({})\n\n#mitex({}, numbering: none)\n",
        serde_json::to_string(&format!("{prefix}{remaining}")).unwrap(),
        serde_json::to_string(&format!(" {prefix}{remaining}")).unwrap()
    );
    let parsed = parse_at(&fixtures().join("macros.typ"), &src, &config);
    let value = json_doc(&parsed);
    let math = nodes(&value, "math");
    assert_eq!(math[0]["source"], remaining);
    assert_eq!(math[1]["source"], format!(" {prefix}{remaining}"));
    assert_eq!(math[0]["dialect"], "latex");
    assert_eq!(math[1]["dialect"], "latex");
}

#[test]
fn listing_and_code_cells_preserve_input_options_and_each_output_candidate() {
    let parsed = clean(
        "#import \"@writer/doc:0.1.0\": *\n#listing(numbered: true, start: 4, highlight: \"4-5\", plain: true)[```rust\nlet x = 1;\nlet y = 2;\n```]\n\n#code-cell(index: 2, count: 7, lang: \"python\", input: ```python\nprint(42)\n```)[First output.][Second output.]\n",
    );
    let value = json_doc(&parsed);
    let code = nodes(&value, "code_block")[0];
    assert_eq!(code["attrs"]["pairs"]["start"], "4");
    assert_eq!(code["attrs"]["pairs"]["highlight"], "4-5");
    let cell = nodes(&value, "cell")[0];
    assert_eq!(cell["index"], 2);
    assert_eq!(cell["execution_count"], 7);
    assert_eq!(cell["language"], "python");
    assert_eq!(cell["source"], "print(42)");
    assert_eq!(cell["show_input"], true);
    assert_eq!(cell["show_output"], true);
    assert_eq!(cell["outputs"].as_array().unwrap().len(), 2);
    assert_eq!(cell["outputs"][0]["candidates"][0]["mime"], "text/x-typst");
    assert!(parsed.document.features.code && parsed.document.features.notebook);
}

#[test]
fn includes_merge_assets_features_source_identity_and_first_use_order() {
    let parsed = parse_fixture("includes");
    assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.document.includes,
        ["include/part.typ", "include/part.md", "include/nb.ipynb"]
            .map(|path| fs::canonicalize(fixtures().join(path)).unwrap())
    );
    assert_eq!(parsed.assets.len(), 1);
    assert!(
        parsed.document.features.math && parsed.document.features.footnotes
    );
    let included =
        fs::canonicalize(fixtures().join("include/part.typ")).unwrap();
    let mut count = 0;
    ir::walk_blocks(&parsed.document, &mut |block| {
        if block
            .span()
            .is_some_and(|span| span.file.as_ref() == Some(&included))
        {
            count += 1;
        }
    });
    assert!(count >= 4);
    let value = json_doc(&parsed);
    let owners = nodes(&value, "ref")
        .into_iter()
        .filter(|node| node["target"] == "sec-outer")
        .map(|node| node["span"]["file"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        owners,
        ["include/part.typ", "include/part.typ", "include/part.md"].map(
            |path| fs::canonicalize(fixtures().join(path))
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )
    );
    assert!(
        nodes(&value, "footnote_ref")
            .iter()
            .any(|node| node["label"] == "shared")
    );
}

#[test]
fn included_metadata_stays_local_and_removes_only_the_synthetic_toc() {
    let dir = project(&[
        (
            "main.typ",
            "#show: writer-doc.with(title: \"Outer\")\n#include \"child.typ\"\n",
        ),
        (
            "child.typ",
            "#show: writer-doc.with(title: \"Child\", toc: 2)\n#outline(depth: 1)\n= Child body\n",
        ),
    ]);
    let parsed = parse_file(&dir.path().join("main.typ"));
    assert_eq!(parsed.document.meta.title.as_deref(), Some("Outer"));
    assert!(codes(&parsed.diagnostics).contains(&"include-meta-ignored"));
    let value = json_doc(&parsed);
    let tocs = nodes(&value, "toc");
    assert_eq!(tocs.len(), 1);
    assert_eq!(tocs[0]["depth"], 1);
    assert!(tocs[0].get("span").is_some());
}

#[test]
fn included_absolute_paths_use_the_initial_root_and_diagnostics_keep_the_owner()
{
    let dir = project(&[
        ("main.typ", "#include \"parts/child.typ\"\n"),
        ("parts/child.typ", "#include \"/root.typ\"\n#unknown()\n"),
        ("root.typ", "Root-relative text.\n"),
    ]);
    let parsed = parse_file(&dir.path().join("main.typ"));
    assert_eq!(
        parsed.document.includes,
        [
            fs::canonicalize(dir.path().join("parts/child.typ")).unwrap(),
            fs::canonicalize(dir.path().join("root.typ")).unwrap()
        ]
    );
    let diagnostic = parsed
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E-TYP-CALL-UNKNOWN")
        .unwrap();
    assert_eq!(
        diagnostic.path.as_ref(),
        Some(&fs::canonicalize(dir.path().join("parts/child.typ")).unwrap())
    );
    assert_eq!(diagnostic.span.as_ref().unwrap().start_line, 2);
}

#[test]
fn include_cycle_depth_dynamic_missing_and_mixed_kinds_have_specific_codes() {
    let cycle = parse("#include \"include/cycle-a.typ\"\n");
    assert!(codes(&cycle.diagnostics).contains(&"E-TYP-INCLUDE-CYCLE"));
    let missing = parse("#include \"absent.typ\"\n");
    assert!(codes(&missing.diagnostics).contains(&"E-TYP-INCLUDE-MISSING"));
    assert!(
        missing
            .document
            .includes
            .iter()
            .any(|path| path.ends_with("absent.typ"))
    );
    for text in [
        "#include \"notebook.ipynb\"",
        "#include \"document.md\"",
        "#include-notebook(path(\"notebook.ipynb\"))",
    ] {
        let dir = project(&[
            ("main.typ", text),
            ("document.md", "# Markdown input\n"),
            (
                "notebook.ipynb",
                r#"{"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":[]}"#,
            ),
        ]);
        let parsed = parse_file(&dir.path().join("main.typ"));
        assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
        let child = if text.contains("document.md") {
            "document.md"
        } else {
            "notebook.ipynb"
        };
        assert_eq!(
            parsed.document.includes,
            [fs::canonicalize(dir.path().join(child)).unwrap()]
        );
        if child == "document.md" {
            assert!(paragraphs(&parsed.document).is_empty());
            assert_eq!(nodes(&json_doc(&parsed), "heading").len(), 1);
        } else {
            assert!(parsed.document.blocks.is_empty());
        }
    }
    let dir = project(&[("main.typ", "#include \"1.typ\"\n")]);
    for index in 1..=8 {
        let text = if index == 8 {
            "At depth eight.\n".into()
        } else {
            format!("#include \"{}.typ\"\n", index + 1)
        };
        fs::write(dir.path().join(format!("{index}.typ")), text).unwrap();
    }
    let eight = parse_file(&dir.path().join("main.typ"));
    assert!(!codes(&eight.diagnostics).contains(&"E-TYP-INCLUDE-DEPTH"));
    fs::write(dir.path().join("8.typ"), "#include \"9.typ\"\n").unwrap();
    fs::write(dir.path().join("9.typ"), "Too deep.\n").unwrap();
    let nine = parse_file(&dir.path().join("main.typ"));
    assert!(codes(&nine.diagnostics).contains(&"E-TYP-INCLUDE-DEPTH"));
}

#[cfg(unix)]
#[test]
fn include_cycle_detection_uses_canonical_identity_for_symlinks() {
    let dir = project(&[("main.typ", "#include \"alias.typ\"\n")]);
    std::os::unix::fs::symlink(
        dir.path().join("main.typ"),
        dir.path().join("alias.typ"),
    )
    .unwrap();
    assert!(
        codes(&parse_file(&dir.path().join("main.typ")).diagnostics)
            .contains(&"E-TYP-INCLUDE-CYCLE")
    );
}

#[test]
fn strict_config_and_cli_override_keep_warning_codes_before_web_rendering() {
    let text = "#show: writer-doc.with(typst: (strict: false))\n#heading(level: 7)[Heading]\n";
    let dir = project(&[
        ("main.typ", text),
        ("writer.toml", "[pdf]\nsystem_fonts=false\n"),
    ]);
    let mut options = BuildOptions::new(dir.path().join("main.typ"));
    options.cli.strict = Some(true);
    let prepared = pipeline::prepare(&options).unwrap();
    assert!(
        prepared
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "W-TYP-ARG-IGNORED"
                && diagnostic.level == Level::Error)
    );
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &options).unwrap();
    let failure = pipeline::web_page(&prepared, &options, world)
        .err()
        .expect("strict subset refuses web rendering");
    assert!(
        failure.error.to_string().contains("depth")
            || failure.error.to_string().contains("level")
    );
}

#[test]
fn simple_typst_prepares_and_feeds_the_existing_html_and_docx_library_paths() {
    let dir = project(&[
        ("main.typ", "= A heading <sec-main>\n\nA *strong* paragraph.\n"),
        ("writer.toml", "[pdf]\nsystem_fonts=false\n"),
    ]);
    let options = BuildOptions::new(dir.path().join("main.typ"));
    let prepared = pipeline::prepare(&options).unwrap();
    assert!(!prepared.diagnostics.has_errors());
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &options).unwrap();
    let page = pipeline::web_page(&prepared, &options, world).unwrap();
    assert!(page.html.contains("A heading"));
    assert!(page.html.contains("<strong>strong</strong>"));
    let output = dir.path().join("preview.docx");
    pipeline::docx::build(
        &prepared.document, &prepared.assets, &prepared.config, &output, world,
    )
    .unwrap();
    assert!(
        writer::docx::validate(
            &fs::read(output).unwrap(),
            &writer::docx::ValidateOptions::default()
        )
        .is_empty()
    );
}

#[test]
fn default_and_explicit_typst_checks_expose_real_subset_json_errors() {
    let dir = project(&[("main.typ", "= Good\n\n#unknown()\n")]);
    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"))
        .current_dir(dir.path())
        .args(["check", "main.typ", "--format", "json"])
        .timeout(std::time::Duration::from_secs(10))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let diagnostics: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        diagnostics
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["code"] == "E-TYP-CALL-UNKNOWN")
    );
    let target = assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"))
        .current_dir(dir.path())
        .args(["check", "main.typ", "--target", "html", "--format", "json"])
        .timeout(std::time::Duration::from_secs(10))
        .output()
        .unwrap();
    assert_eq!(target.status.code(), Some(1));
    assert!(target.stderr.is_empty());
    let diagnostics: Value = serde_json::from_slice(&target.stdout).unwrap();
    assert!(diagnostics.as_array().unwrap().iter().any(|diagnostic| {
        diagnostic["code"] == "E-TYP-CALL-UNKNOWN"
            && diagnostic["level"] == "error"
    }));
}

#[test]
fn bibliography_fixture_is_a_byte_copy_and_subset_compiles_offline() {
    use typst::foundations::Datetime;
    use writer::typst::compile::compile_document;
    use writer::typst::world::{MainFile, WriterWorld};
    assert_eq!(
        fs::read(fixtures().join("refs.bib")).unwrap(),
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/bib/refs.bib")
        )
        .unwrap()
    );
    let mut config = Config::default();
    config.pdf.system_fonts = false;
    config.pdf.allow_packages = false;
    let path = fixtures().join("subset.typ");
    let mut world = WriterWorld::from_config(
        &config,
        &fixtures(),
        MainFile::Path(path),
        Datetime::from_ymd(2026, 10, 28).unwrap(),
    )
    .unwrap();
    let compiled = compile_document(&mut world);
    assert!(compiled.output.is_some(), "{:?}", compiled.diagnostics);
}

#[test]
fn valid_subset_corpus_exercises_every_ir_family_and_label_target() {
    let parsed = parse_fixture("subset");
    assert!(!parsed.diagnostics.has_errors(), "{:?}", parsed.diagnostics);
    let value = json_doc(&parsed);
    for kind in [
        "heading", "paragraph", "strong", "emph", "underline", "strikeout",
        "small_caps", "superscript", "subscript", "span", "code", "code_block",
        "image", "link", "figure", "table", "block_quote", "list",
        "definition_list", "div", "math", "footnote", "footnote_ref", "ref",
        "cite", "callout", "cell", "toc", "thematic_break", "page_break",
    ] {
        assert!(!nodes(&value, kind).is_empty(), "missing {kind}");
    }
    let serialized = serde_json::to_string(&value).unwrap();
    for id in [
        "sec-typed", "link-docs", "inline-image", "code-inline", "code-block",
        "fig-image", "tbl-figure", "lst-figure", "fig-other", "sec-plain",
        "eq-typed", "eq-plain", "eq-latex", "anchor-empty", "anchor-inline",
        "callout-tip", "listing-helper", "cell-demo",
    ] {
        assert!(
            serialized.contains(&format!("\"id\":\"{id}\"")),
            "missing label {id}"
        );
    }
    let features = &parsed.document.features;
    assert!(
        features.math
            && features.code
            && features.notebook
            && features.citations
            && features.footnotes
    );
}

#[test]
fn oversized_table_grids_fail_locally_and_keep_surrounding_paragraphs() {
    for call in [
        "#table(columns: 1000000000000, [A])",
        "#table(columns: 1, table.cell(rowspan: 65537)[A])",
        "#table(columns: 32769, table.header([H]), [Body])",
    ] {
        let parsed = parse(&format!("Before.\n\n{call}\n\nAfter.\n"));
        assert!(
            codes(&parsed.diagnostics).contains(&"E-TYP-ARG-INVALID"),
            "{call}: {:?}",
            parsed.diagnostics
        );
        assert!(nodes(&json_doc(&parsed), "table").is_empty());
        assert_eq!(paragraphs(&parsed.document), ["Before.", "After."]);
    }
}

#[test]
fn explicit_ref_supplement_none_and_auto_and_link_string_fragments_stay_distinct()
 {
    let parsed = clean(
        "#ref(<sec-x>, supplement: none) #ref(<sec-x>, supplement: auto) #link(\"#target\")\n",
    );
    let value = json_doc(&parsed);
    let refs = nodes(&value, "ref");
    assert_eq!(refs[0]["supplement"], json!([]));
    assert_eq!(refs[1]["supplement"], Value::Null);
    let link = nodes(&value, "link")[0];
    assert_eq!(link["url"], "#target");
    assert_eq!(link["inlines"][0]["text"], "#target");
}

#[test]
fn notebook_include_strings_warn_and_dynamic_paths_use_the_include_diagnostic()
{
    let literal = parse("#include-notebook(\"notebook.ipynb\")");
    assert!(codes(&literal.diagnostics).contains(&"W-TYP-PATH-STRING"));
    assert!(codes(&literal.diagnostics).contains(&"E-TYP-INCLUDE-MISSING"));
    let dynamic = parse("#include-notebook(upper(\"notebook.ipynb\"))");
    assert!(codes(&dynamic.diagnostics).contains(&"E-TYP-INCLUDE-DYNAMIC"));
}

#[test]
fn smart_quotes_share_paragraph_context_across_inline_wrappers() {
    let parsed = clean("x*'s* and \"#strong[well-]\" and 5#emph[']\n");
    assert_eq!(paragraphs(&parsed.document), ["x’s and “well-” and 5′"]);
}

#[test]
fn a_label_does_not_jump_over_an_unsupported_inline_call() {
    let parsed = parse("`code` #unknown() <lost>\n");
    let got = codes(&parsed.diagnostics);
    assert!(got.contains(&"E-TYP-CALL-UNKNOWN"));
    assert!(got.contains(&"W-TYP-LABEL-IGNORED"));
    let value = json_doc(&parsed);
    assert!(nodes(&value, "code")[0]["attrs"]["id"].is_null());
}

#[test]
fn an_explicit_false_strict_override_wins_over_typst_metadata() {
    let dir = project(&[
        (
            "main.typ",
            "#show: writer-doc.with(typst: (strict: true))\n#heading(level: 7)[Heading]\n",
        ),
        ("writer.toml", "[pdf]\nsystem_fonts=false\n"),
    ]);
    let mut options = BuildOptions::new(dir.path().join("main.typ"));
    options.cli.strict = Some(false);
    let prepared = pipeline::prepare(&options).unwrap();
    assert!(!prepared.config.typst.strict);
    assert!(prepared.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "W-TYP-ARG-IGNORED"
            && diagnostic.level == Level::Warning
    }));
    assert!(pipeline::subset_error(&prepared.diagnostics).is_none());
}

#[test]
fn labels_follow_source_targets_instead_of_incidental_empty_wrapper_shapes() {
    for text in [
        "#text[] <lost>\n", "#block[] <lost>\n", "#par[#raw(\"x\")] <lost>\n",
        "#box[] <lost>\n",
    ] {
        let parsed = parse(text);
        assert!(codes(&parsed.diagnostics).contains(&"W-TYP-LABEL-IGNORED"));
        let json = serde_json::to_string(&parsed.document).unwrap();
        assert!(!json.contains("\"id\":\"lost\""));
    }
}

#[test]
fn exponentially_replayed_literal_content_has_one_bounded_located_failure() {
    let mut source = String::from("#let a0 = [x]\n");
    for index in 1..=24 {
        source.push_str(&format!(
            "#let a{index} = [#a{}#a{}]\n",
            index - 1,
            index - 1
        ));
    }
    source.push_str("\nBefore.\n\n#a24\n\nAfter.\n");
    let parsed = parse(&source);
    let failures = parsed
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == "E-TYP-ARG-INVALID"
                && diagnostic
                    .message
                    .contains("literal content expansion limit exceeded")
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1, "{:?}", parsed.diagnostics);
    assert!(failures[0].path.is_some() && failures[0].span.is_some());
    let paragraphs = paragraphs(&parsed.document);
    assert_eq!(paragraphs.first().map(String::as_str), Some("Before."));
    assert_eq!(paragraphs.last().map(String::as_str), Some("After."));
    assert!(paragraphs.iter().map(String::len).sum::<usize>() < 70_000);
}

#[test]
fn a_deep_captured_content_chain_stops_without_losing_surrounding_markup() {
    let mut source = String::from("#let a0 = [x]\n");
    for index in 1..=80 {
        source.push_str(&format!("#let a{index} = [#a{}]\n", index - 1));
    }
    source.push_str("\nBefore.\n\n#a80\n\nAfter.\n");
    let parsed = parse(&source);
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == "E-TYP-ARG-INVALID"
                    && diagnostic
                        .message
                        .contains("literal content expansion limit exceeded")
            })
            .count(),
        1
    );
    let paragraphs = paragraphs(&parsed.document);
    assert_eq!(paragraphs.first().map(String::as_str), Some("Before."));
    assert_eq!(paragraphs.last().map(String::as_str), Some("After."));
}

#[test]
fn small_repeated_captured_content_still_expands_normally() {
    let parsed =
        clean("#let a = [x]\n#let b = [#a#a]\n\nBefore.\n\n#b #b\n\nAfter.\n");
    assert_eq!(paragraphs(&parsed.document), ["Before.", "xx xx", "After."]);
}

#[test]
fn exponentially_composed_literal_arrays_are_rejected_before_cloning() {
    let mut source = String::from("#let a0 = (1,)\n");
    for index in 1..=24 {
        source.push_str(&format!(
            "#let a{index} = (a{}, a{})\n",
            index - 1,
            index - 1
        ));
    }
    source.push_str("\nBefore.\n\nAfter.\n");
    let parsed = parse(&source);
    let failures = parsed
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == "E-TYP-ARG-INVALID"
                && diagnostic.message.contains("literal value limit exceeded")
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1, "{:?}", parsed.diagnostics);
    assert!(failures[0].path.is_some() && failures[0].span.is_some());
    assert_eq!(paragraphs(&parsed.document), ["Before.", "After."]);
}

#[test]
fn deeply_nested_literal_arrays_have_a_located_failure_and_keep_markup() {
    let source = format!(
        "#let value = {}1{}\n\nBefore.\n\nAfter.\n",
        "(".repeat(80),
        ",)".repeat(80)
    );
    let parsed = parse(&source);
    assert!(!codes(&parsed.diagnostics).contains(&"E-TYP-SYNTAX"));
    let failures = parsed
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == "E-TYP-ARG-INVALID"
                && diagnostic.message.contains("literal value limit exceeded")
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1, "{:?}", parsed.diagnostics);
    assert!(failures[0].path.is_some() && failures[0].span.is_some());
    assert_eq!(paragraphs(&parsed.document), ["Before.", "After."]);
}

#[test]
fn composed_literal_arrays_count_shared_string_bytes_before_cloning() {
    let mut source = format!("#let a0 = \"{}\"\n", "x".repeat(1024 * 1024));
    for index in 1..=4 {
        source.push_str(&format!(
            "#let a{index} = (a{}, a{})\n",
            index - 1,
            index - 1
        ));
    }
    source.push_str("\nBefore.\n\nAfter.\n");
    let parsed = parse(&source);
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == "E-TYP-ARG-INVALID"
                    && diagnostic
                        .message
                        .contains("literal value limit exceeded")
            })
            .count(),
        1,
        "{:?}",
        parsed.diagnostics
    );
    assert_eq!(paragraphs(&parsed.document), ["Before.", "After."]);
}

#[test]
fn normal_literal_arrays_and_dictionary_aliases_remain_usable() {
    let parsed = clean(
        "#import \"@writer/doc:0.1.0\": *\n#let first = (name: \"Ada\")\n#let authors = (first, (name: \"Grace\"))\n#show: writer-doc.with(authors: authors)\n#let widths = (1fr, 2fr)\n#table(columns: widths, [A], [B])\n",
    );
    assert_eq!(
        parsed
            .document
            .meta
            .authors
            .iter()
            .map(|author| author.name.as_str())
            .collect::<Vec<_>>(),
        ["Ada", "Grace"]
    );
    assert_eq!(nodes(&json_doc(&parsed), "table").len(), 1);
}
