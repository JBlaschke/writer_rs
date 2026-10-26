//! Resolved Markdown markup, one complete preview page and standalone budgets.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use writer::config::{Config, Target};
use writer::error::Diagnostics;
use writer::html::emit::{EmitOptions, KatexOnly, emit};
use writer::ir::{SourceInfo, SourceKind};
use writer::pipeline::{self, BuildOptions, Mode, WorldSlot};
use writer::resolve::{ResolveOptions, resolve};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/md")
}

fn fixture_paths() -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 17, "keep the complete Markdown fixture corpus");
    paths
}

fn stable_paths(text: &str) -> String {
    text.replace(&*fixture_dir().to_string_lossy(), "[fixtures]")
        .replace('\\', "/")
}

fn stable_asset_ids(html: &str) -> String {
    let mut labels = BTreeMap::new();
    Regex::new(r"asset:([0-9a-f]{12})")
        .unwrap()
        .replace_all(html, |caps: &regex::Captures<'_>| {
            let next = format!("{:012x}", labels.len() + 1);
            let label = labels.entry(caps[1].to_owned()).or_insert(next);
            format!("asset:{label}")
        })
        .into_owned()
}

fn diagnostics_text(diagnostics: &Diagnostics, source: &Path) -> String {
    diagnostics
        .iter()
        .map(|finding| {
            let path = finding.path.as_deref().unwrap_or(source).display();
            let line = finding
                .span
                .as_ref()
                .map(|span| span.start_line.to_string())
                .unwrap_or_else(|| "?".into());
            stable_paths(&format!(
                "{} {} {path}:{line} {}",
                finding.level, finding.code, finding.message,
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn md_fixtures() {
    for path in fixture_paths() {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let source = SourceInfo {
            path: path.clone(),
            kind: SourceKind::Markdown,
            base_dir: fixture_dir(),
        };
        let text = std::fs::read_to_string(&path).unwrap();
        let mut config = Config::default();
        let mut parsed = match writer::markdown::parse(&source, &text, &config)
        {
            Ok(parsed) => parsed,
            Err(error) => {
                insta::assert_snapshot!(name, stable_paths(&error.to_string()));
                continue;
            }
        };
        config.merge_front_matter(&parsed.document.meta).unwrap();
        parsed.diagnostics.extend(resolve(
            &mut parsed.document,
            &ResolveOptions { config: &config, library: None, lenient: true },
        ));
        assert!(
            parsed.document.invariant_violations().is_empty(),
            "{name}: {:?}",
            parsed.document.invariant_violations()
        );
        let mut math = KatexOnly::default();
        let emitted = emit(
            &parsed.document,
            &mut EmitOptions {
                config: &config,
                positions: false,
                includes: &parsed.document.includes,
                math: &mut math,
                assets: &mut parsed.assets,
                diagnostics: &mut parsed.diagnostics,
            },
        );
        insta::assert_snapshot!(name.clone(), stable_asset_ids(&emitted.main));
        insta::assert_snapshot!(
            format!("{name}-diagnostics"),
            diagnostics_text(&parsed.diagnostics, &source.path)
        );
    }
}

#[test]
fn serve_minimal_page() {
    let mut opts = BuildOptions::new(fixture_dir().join("minimal.md"));
    opts.mode = Mode::Serve { build_id: 1 };
    let prepared = pipeline::prepare(&opts).unwrap();
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &opts).unwrap();
    let page = pipeline::web_page(&prepared, &opts, world).unwrap();
    assert!(page.diagnostics.is_empty());
    assert!(page.html.contains("data-writer-document"));
    assert!(page.html.contains("data-sourcepos=\"8-8\""));
    assert!(page.html.contains("/__writer/theme.css"));
    assert!(page.html.contains("/__writer/live-reload.js"));
    let version = Regex::new(r"\?v=\d+").unwrap();
    let stable = version.replace_all(&page.html, "?v=[version]");
    insta::assert_snapshot!("serve_minimal", stable_paths(&stable));
}

#[test]
fn standalone_size_budget() {
    let dir = tempfile::tempdir().unwrap();
    let mut worlds = WorldSlot::new();
    for (name, budget) in [("minimal", 60_000), ("math", 700_000)] {
        let out = dir.path().join(format!("{name}.html"));
        let mut opts =
            BuildOptions::new(fixture_dir().join(format!("{name}.md")));
        opts.cli.out = Some(out.clone());
        let report =
            pipeline::build_targets(&opts, &[Target::Html], &mut worlds);
        assert!(report.prepared.is_ok(), "{name}: prepare failed");
        assert_eq!(report.targets.len(), 1);
        let written = report.targets[0].written.as_ref().unwrap();
        assert_eq!(written, std::slice::from_ref(&out));
        let html = std::fs::read_to_string(out).unwrap();
        assert!(
            html.len() <= budget,
            "{name}: {} bytes exceeds {budget}",
            html.len()
        );
        assert!(html.contains("data-writer-document"));
        assert!(!html.contains("asset:"), "{name}: unresolved asset");
        assert!(!html.contains("/__writer/"), "{name}: serve URL in build");
    }
}
