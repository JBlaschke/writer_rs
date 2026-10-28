//! Resolved IR numbering agrees with the compiled `@writer/doc` metadata.
//! The corpus and the single oracle test are references.md §7's contract.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use typst::foundations::Datetime;
use typst::introspection::Introspector;
use writer::config::{
    EquationNumbering, FigureNumbering, HeadingNumbering, NumberingConfig,
};
use writer::ir::{self, Block, Document, FigureKind, Inline, LabelKind};
use writer::pipeline::{self, BuildOptions, Prepared};
use writer::resolve::{ResolveOptions, resolve};
use writer::typst::compile::{OracleEntry, compile_document, oracle};
use writer::typst::emit::{AssetPaths, EmitOptions, emit};
use writer::typst::world::{MainFile, WriterWorld, world_inputs};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn prepare(fixture: &str, policy: Option<NumberingConfig>) -> Prepared {
    let opts = BuildOptions::new(fixtures().join(fixture));
    let mut prepared = pipeline::parse_only(&opts)
        .unwrap_or_else(|failure| panic!("{fixture}: {}", failure.error));

    // The references corpus deliberately tests invalid duplicate labels. The
    // coordinator-approved oracle variant clears only the second occurrence
    // before resolve; both headings and their content stay in the document.
    if fixture == "md/references.md" {
        let mut occurrences = 0;
        ir::walk_blocks_mut(&mut prepared.document, &mut |block| {
            if let Block::Heading { attrs, .. } = block
                && attrs.id.as_deref() == Some("sec-duplicate")
            {
                occurrences += 1;
                if occurrences == 2 {
                    attrs.id = None;
                }
            }
        });
        assert_eq!(occurrences, 2, "{fixture}: keep the negative fixture case");
    }

    // Deliberately override the effective config after front-matter merging:
    // putting this policy into the initial config would let the fixture win.
    if let Some(policy) = policy {
        prepared.config.numbering = policy;
    }
    prepared.config.pdf.system_fonts = false;
    prepared.config.pdf.allow_packages = false;
    prepared.diagnostics.extend(resolve(
        &mut prepared.document,
        &ResolveOptions {
            config: &prepared.config,
            library: None,
            lenient: true,
        },
    ));
    assert!(
        prepared.document.invariant_violations().is_empty(),
        "{fixture}: {:?}",
        prepared.document.invariant_violations()
    );
    prepared
}

fn mounted_file(world: &mut WriterWorld, path: &Path, prefix: &str) -> String {
    world.mount(prefix, path.parent().expect("resource parent"));
    let name = path.file_name().and_then(|name| name.to_str()).unwrap();
    format!("{prefix}/{name}")
}

fn expected(document: &Document) -> BTreeMap<String, OracleEntry> {
    let mut entries = BTreeMap::new();
    ir::walk_blocks(document, &mut |block| {
        let (attrs, kind, number) = match block {
            Block::Heading { attrs, number, .. } => {
                (attrs, "heading", number.clone())
            }
            Block::Figure { attrs, kind, number, .. } => {
                let kind = match kind {
                    FigureKind::Image | FigureKind::Other => "figure",
                    FigureKind::Table => "table",
                    FigureKind::Listing => "listing",
                };
                (attrs, kind, number.clone())
            }
            Block::Math { attrs, number, .. } => {
                (attrs, "equation", number.as_ref().map(|n| format!("({n})")))
            }
            _ => return,
        };
        if let Some(label) = &attrs.id {
            let previous = entries.insert(
                label.clone(),
                OracleEntry { kind: kind.into(), number },
            );
            assert!(previous.is_none(), "duplicate oracle target {label}");
        }
    });
    ir::walk_inlines(document, &mut |inline| {
        if let Inline::Footnote { label: Some(label), number, .. } = inline {
            let previous = entries.insert(
                label.clone(),
                OracleEntry {
                    kind: "footnote".into(),
                    number: number.map(|n| n.to_string()),
                },
            );
            assert!(previous.is_none(), "duplicate oracle target {label}");
        }
    });
    entries
}

fn check(
    fixture: &str, name: &str, policy: Option<NumberingConfig>,
) -> BTreeMap<String, OracleEntry> {
    let prepared = prepare(fixture, policy);
    let config = &prepared.config;
    let mut world = WriterWorld::from_config(
        config,
        &prepared.document.source.base_dir,
        MainFile::Virtual,
        Datetime::from_ymd(2026, 10, 24).unwrap(),
    )
    .unwrap_or_else(|error| panic!("{name}: world: {error}"));
    let mut inputs = world_inputs(config);
    inputs.insert("writer-oracle".into(), "1".into());
    world.set_inputs(inputs);

    let bibliography = config
        .references
        .bibliography
        .iter()
        .enumerate()
        .map(|(index, path)| {
            mounted_file(&mut world, path, &format!("/__writer/bib/{index}"))
        })
        .collect();
    let style = if Path::new(&config.references.style)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csl"))
    {
        mounted_file(
            &mut world,
            Path::new(&config.references.style),
            "/__writer/csl",
        )
    } else {
        config.references.style.clone()
    };
    let template = (!config.pdf.template.as_os_str().is_empty()).then(|| {
        mounted_file(&mut world, &config.pdf.template, "/__writer/template")
    });
    let emitted = emit(
        &prepared.document,
        &prepared.assets,
        &EmitOptions {
            config,
            assets: AssetPaths::Virtual,
            template,
            bibliography,
            style,
            footnote_prefix: String::new(),
            fragment: false,
        },
    )
    .unwrap_or_else(|error| panic!("{name}: emit: {error}"));
    world.set_assets(prepared.assets.clone());
    world.set_main_text(&emitted.text, emitted.map.clone());
    let compiled = compile_document(&mut world);
    assert!(
        compiled.output.is_some() && !compiled.diagnostics.has_errors(),
        "{name}: compile: {:#?}\n\nSource:\n{}",
        compiled.diagnostics,
        emitted.text
    );
    let (compiled, _) = compiled
        .into_result()
        .unwrap_or_else(|error| panic!("{name}: compile: {error}"));

    let expected = expected(&prepared.document);
    let mut checked = 0;
    for (label, info) in &prepared.document.resolved.labels {
        if info.kind == LabelKind::Other || label.starts_with("writer-fn-") {
            continue;
        }
        let expected = expected
            .get(label)
            .unwrap_or_else(|| panic!("{name}: missing Writer target {label}"));
        let actual = oracle(&compiled, label);
        assert_eq!(
            actual.as_ref(),
            Some(expected),
            "{name}: {label}: Writer {expected:?}, Typst {actual:?}"
        );
        checked += 1;
    }
    assert!(checked > 0, "{name}: oracle corpus must contain real targets");
    let actual_count = compiled
        .introspector()
        .query_labelled()
        .iter()
        .filter(|content| {
            content.label().is_some_and(|label| {
                label.resolve().as_str().starts_with("writer-oracle-")
            })
        })
        .count();
    assert_eq!(
        actual_count, checked,
        "{name}: Writer checked {checked} labels, Typst emitted {actual_count}"
    );
    expected
}

#[test]
fn resolved_numbering_matches_the_compiled_oracle_for_the_complete_corpus() {
    let base = check("md/numbering.md", "numbering/base", None);
    let mut kinds: BTreeSet<_> =
        base.values().map(|entry| entry.kind.clone()).collect();
    for fixture in [
        "md/figures.md", "md/tables.md", "md/code.md", "md/math.md",
        "md/references.md", "typ/subset.typ",
    ] {
        kinds.extend(
            check(fixture, fixture, None).into_values().map(|entry| entry.kind),
        );
    }
    assert_eq!(
        kinds,
        ["equation", "figure", "footnote", "heading", "listing", "table"]
            .map(str::to_string)
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "the corpus must exercise every oracle kind"
    );
    for (name, pattern, figures, equations, label, number) in [
        (
            "document-all",
            Some("1.1"),
            FigureNumbering::Document,
            EquationNumbering::All,
            "fig-a",
            Some("2"),
        ),
        (
            "alpha-none",
            Some("A.1"),
            FigureNumbering::Section,
            EquationNumbering::None,
            "sec-one",
            Some("A"),
        ),
        (
            "headings-off",
            None,
            FigureNumbering::Section,
            EquationNumbering::Labelled,
            "fig-a",
            Some("2"),
        ),
    ] {
        let policy = NumberingConfig {
            headings: pattern.map_or(HeadingNumbering::Off, |pattern| {
                HeadingNumbering::Pattern(pattern.into())
            }),
            figures,
            equations,
        };
        let actual = check("md/numbering.md", name, Some(policy));
        assert_eq!(actual[label].number.as_deref(), number, "{name}: {label}");
        assert_ne!(
            actual[label].number, base[label].number,
            "{name}: {label}: policy must differ from the base fixture"
        );
    }
}
