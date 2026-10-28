//! Markdown lint codes/locations through the complete check pipeline.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use writer::check::{self, CheckOptions};
use writer::error::Diagnostics;
use writer::pipeline::BuildOptions;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn findings(path: &Path) -> Diagnostics {
    match check::run(&BuildOptions::new(path), &CheckOptions::default()) {
        Ok(diagnostics) => diagnostics,
        Err(failure) => {
            let mut diagnostics = failure.diagnostics;
            diagnostics.push(check::error_diagnostic(&failure.error, path));
            diagnostics
        }
    }
}

fn stable_file(path: &Path) -> String {
    let root = fixtures();
    let canonical = std::fs::canonicalize(&root).unwrap();
    path.strip_prefix(&root)
        .or_else(|_| path.strip_prefix(&canonical))
        .unwrap_or_else(|_| {
            panic!("fixture diagnostic outside corpus: {path:?}")
        })
        .to_string_lossy()
        .replace('\\', "/")
}

fn stable(diagnostics: &Diagnostics) -> Vec<Value> {
    diagnostics
        .iter()
        .map(|finding| {
            let span = finding.span.as_ref();
            json!({
                "level": finding.level,
                "code": finding.code,
                "file": finding.path.as_deref().map(stable_file),
                "line": span.map(|span| span.start_line),
                "cell": span.and_then(|span| span.cell),
            })
        })
        .collect()
}

#[test]
fn every_markdown_fixture_has_reviewed_check_codes() {
    let expected: &[(&str, &[&str])] = &[
        ("minimal", &[]),
        ("attributes", &["label-invalid"]),
        ("attribute-recovery", &[]),
        ("directives", &["unclosed-directive", "invalid-directive"]),
        ("pandoc-directives", &["directive-colon-rewritten"]),
        ("figures", &["invalid-dimension"]),
        ("tables", &["table-caption-orphan", "table-caption-pandoc-colon"]),
        ("callouts", &[]),
        ("code", &["invalid-code-attr"]),
        ("math", &["equation-label-misplaced"]),
        (
            "references",
            &["duplicate-label", "label-reserved", "unresolved-reference"],
        ),
        ("footnotes", &["footnote-unreferenced"]),
        ("includes", &["parse-error"]),
        ("include-chain", &["include-meta-ignored"]),
        ("raw", &["raw-unknown-format"]),
        ("pagebreaks", &[]),
        ("presenter-deck", &["presenter-separator", "presenter-attr-dropped"]),
        ("numbering", &[]),
    ];
    let mut names: Vec<_> = std::fs::read_dir(fixtures().join("md"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "md")
        })
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut covered: Vec<_> = expected.iter().map(|(name, _)| *name).collect();
    covered.sort();
    assert_eq!(
        names, covered,
        "every added Markdown fixture needs check coverage"
    );
    for (name, required) in expected {
        let diagnostics =
            findings(&fixtures().join("md").join(format!("{name}.md")));
        for code in *required {
            assert!(
                diagnostics.iter().any(|finding| finding.code == *code),
                "{name}: missing {code}: {diagnostics:?}"
            );
        }
        for finding in &diagnostics {
            assert!(finding.path.is_some(), "{name}: {finding}");
            if let Some(span) = &finding.span {
                assert!(
                    span.start_line >= 1 && span.end_line >= span.start_line
                );
            }
        }
        insta::assert_json_snapshot!(name.to_string(), stable(&diagnostics));
    }
}
