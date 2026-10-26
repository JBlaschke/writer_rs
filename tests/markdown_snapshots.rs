//! T12 frontend snapshots of every top-level `tests/fixtures/md/*.md`.
//!
//! Asset relabelling is lifted from presenter's Markdown snapshot harness.
//! Source paths and diagnostic fixture paths are redacted; the probe corpus
//! stays separate, as it tests comrak rather than the document frontend.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::Value;
use writer::config::Config;
use writer::ir::{self, Block, Document, Parsed, SourceInfo, SourceKind};
use writer::markdown::parse;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md")
}

fn fixture_names() -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(names.len() >= 16, "the T01 source corpus must stay complete");
    names
}

fn parse_fixture(name: &str) -> writer::Result<Parsed> {
    let dir = fixture_dir();
    let source = SourceInfo {
        path: dir.join(format!("{name}.md")),
        kind: SourceKind::Markdown,
        base_dir: dir,
    };
    let text = std::fs::read_to_string(&source.path).unwrap();
    parse(&source, &text, &Config::default())
}

fn stable_json(document: &Document) -> Value {
    let ids: Vec<_> =
        document.assets.iter().map(|asset| asset.id.as_str()).collect();
    let mut labels = BTreeMap::new();
    let mut value = serde_json::to_value(document).unwrap();
    relabel(&mut value, &ids, &mut labels);
    if let Some(assets) = value.get_mut("assets").and_then(Value::as_array_mut)
    {
        for asset in assets.iter_mut() {
            if let Some(Value::String(origin)) = asset.get_mut("origin") {
                *origin = origin.replace('\\', "/");
            }
        }
        assets.sort_by_key(|asset| asset["id"].as_str().map(str::to_string));
    }
    value
}

fn relabel(
    value: &mut Value, ids: &[&str], labels: &mut BTreeMap<String, String>,
) {
    match value {
        Value::String(text) => {
            let hex = Regex::new("[0-9a-f]{12}").unwrap();
            *text = hex
                .replace_all(text, |caps: &regex::Captures<'_>| {
                    let id = &caps[0];
                    if !ids.contains(&id) {
                        return id.to_string();
                    }
                    let next = format!("{:012x}", labels.len() + 1);
                    labels.entry(id.to_string()).or_insert(next).clone()
                })
                .into_owned();
        }
        Value::Array(items) => {
            items.iter_mut().for_each(|item| relabel(item, ids, labels));
        }
        Value::Object(map) => {
            map.values_mut().for_each(|item| relabel(item, ids, labels));
        }
        _ => {}
    }
}

fn redact_paths(text: &str) -> String {
    text.replace(&*fixture_dir().to_string_lossy(), "[fixtures]")
        .replace('\\', "/")
}

#[test]
fn every_fixture_is_snapshotted() {
    for name in fixture_names() {
        match parse_fixture(&name) {
            Ok(parsed) => {
                insta::assert_json_snapshot!(name.clone(), stable_json(&parsed.document), {
                    ".source.path" => "[path]",
                    ".source.base_dir" => "[path]",
                });
                let diagnostics = parsed
                    .diagnostics
                    .iter()
                    .map(|finding| redact_paths(&format!("{finding:#}")))
                    .collect::<Vec<_>>()
                    .join("\n");
                insta::assert_snapshot!(
                    format!("{name}_diagnostics"),
                    diagnostics
                );
            }
            Err(error) => {
                insta::assert_snapshot!(name, redact_paths(&error.to_string()))
            }
        }
    }
}

#[test]
fn fixtures_satisfy_ir_invariants() {
    for name in fixture_names() {
        let parsed = parse_fixture(&name).unwrap();
        assert_eq!(
            parsed.document.invariant_violations(),
            Vec::<String>::new(),
            "{name}"
        );
        let relabelled: Document =
            serde_json::from_value(stable_json(&parsed.document)).unwrap();
        assert_eq!(
            relabelled.invariant_violations(),
            Vec::<String>::new(),
            "{name}: relabelled"
        );
        ir::walk_blocks(&parsed.document, &mut |block| {
            if block.span().is_none() {
                assert!(
                    matches!(block, Block::Toc { .. }),
                    "{name}: {block:?}"
                );
                assert!(
                    parsed
                        .document
                        .blocks
                        .first()
                        .is_some_and(|first| { std::ptr::eq(first, block) }),
                    "{name}: only the inserted TOC may omit a span"
                );
            }
        });
    }
}

#[test]
fn every_diagnostic_has_path_and_span() {
    for name in fixture_names() {
        let parsed = parse_fixture(&name).unwrap();
        let mut previous = 0;
        for finding in parsed.diagnostics.iter() {
            assert_eq!(
                finding.path.as_deref(),
                Some(parsed.document.source.path.as_path())
            );
            let span = finding
                .span
                .as_ref()
                .unwrap_or_else(|| panic!("{name}: {finding:#}"));
            assert!(
                span.start_line >= 1 && span.end_line >= span.start_line,
                "{name}: {finding:#}"
            );
            assert!(
                span.start_line >= previous,
                "{name}: diagnostics must be sorted"
            );
            previous = span.start_line;
        }
    }
}

#[test]
fn parse_is_deterministic() {
    for name in fixture_names() {
        let first = parse_fixture(&name).unwrap();
        let second = parse_fixture(&name).unwrap();
        assert_eq!(first.document, second.document, "{name}");
        assert_eq!(first.diagnostics, second.diagnostics, "{name}");
        assert_eq!(first.assets.len(), second.assets.len(), "{name}");
    }
}

#[test]
fn asset_suffixes_satisfy_ir_invariants() {
    let dir = fixture_dir();
    let source = SourceInfo {
        path: dir.join("asset-suffixes.md"),
        kind: SourceKind::Markdown,
        base_dir: dir,
    };
    let parsed = parse(
        &source,
        "![Caption](images/plot.png?v=2#panel)\n\n<img src=\"images/plot.png#panel\">\n",
        &Config::default(),
    ).unwrap();
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.assets.len(), 1);
    assert_eq!(parsed.document.invariant_violations(), Vec::<String>::new());
}

#[test]
fn probe_corpus_produces_valid_ir() {
    let probe = fixture_dir().join("probe");
    for dir in [&probe, &probe.join("cases")] {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if !path.extension().is_some_and(|ext| ext == "md") {
                continue;
            }
            let source = SourceInfo {
                path: path.clone(),
                kind: SourceKind::Markdown,
                base_dir: dir.to_path_buf(),
            };
            let text = std::fs::read_to_string(&path).unwrap();
            let parsed = parse(&source, &text, &Config::default())
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(
                parsed.document.invariant_violations(),
                Vec::<String>::new(),
                "{}",
                path.display()
            );
        }
    }
}
