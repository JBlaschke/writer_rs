//! The ejected reference is a deterministic, readable demonstration of real IR.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use writer::assets::registry::AssetData;
use writer::config::Config;
use writer::docx::custom_style::{reference_docx, showcase_document};
use writer::docx::styles::{StyleKind, VOCABULARY};
use writer::docx::{
    self, DocxOptions, DocxOutput, Picture, Renderer, ValidateOptions,
};
use writer::error::{Error, Result};
use writer::ir::{self, Block, MathDialect, SourceKind};
use writer::resolve::anchor;

#[path = "common/docx.rs"]
mod common_docx;
#[path = "common/math_xml.rs"]
mod math_xml;
use math_xml::{Node, parse};

const UNREACHABLE_TOKENS: &[&str] = &[
    "VerbatimStringTok", "OtherTok", "AlertTok", "InformationTok",
    "WarningTok", "CommentVarTok", "ExtensionTok", "RegionMarkerTok",
];

#[derive(Default)]
struct NoRenderer {
    calls: usize,
}

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        self.calls += 1;
        Err(Error::Other("showcase must not render equations".into()))
    }

    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        self.calls += 1;
        Err(Error::Other("showcase must not convert images".into()))
    }

    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        self.calls += 1;
        Err(Error::Other("showcase must not compile math".into()))
    }
}

fn emit(reference: Option<&Path>) -> DocxOutput {
    let (document, assets) = showcase_document();
    let original_document = document.clone();
    let original_assets = assets.clone();
    let mut renderer = NoRenderer::default();
    let output = docx::write(
        &document,
        &assets,
        &mut renderer,
        &DocxOptions {
            config: &Config::default(),
            reference,
            source_date_epoch: None,
            all_styles: true,
        },
    )
    .unwrap();
    assert_eq!(renderer.calls, 0);
    assert_eq!(
        document, original_document,
        "writing must not resolve or mutate IR"
    );
    assert_eq!(
        assets, original_assets,
        "writing must not alter the built-in assets"
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        docx::validate(
            &output.bytes,
            &ValidateOptions { builtin_styles: reference.is_none() }
        ),
        []
    );
    output
}

fn tree(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> Node {
    parse(&math_xml::part(parts, name))
}

fn style_kind(kind: StyleKind) -> &'static str {
    match kind {
        StyleKind::Paragraph => "paragraph",
        StyleKind::Character => "character",
        StyleKind::Table => "table",
    }
}

fn style_map(styles: &Node) -> BTreeMap<String, (String, String)> {
    let mut result = BTreeMap::new();
    for style in styles.descendants("w:style") {
        let id = style.attr("w:styleId").unwrap().to_owned();
        let name = style.child("w:name").unwrap().attr("w:val").unwrap();
        let kind = style.attr("w:type").unwrap();
        assert!(result.insert(id, (name.into(), kind.into())).is_none());
    }
    result
}

fn pure_base_or_link(name: &str) -> bool {
    matches!(
        name,
        "Normal"
            | "Default Paragraph Font"
            | "Body Text Char"
            | "Title Char"
            | "Subtitle Char"
            | "Heading 1 Char"
            | "Heading 2 Char"
            | "Heading 3 Char"
            | "Heading 4 Char"
            | "Heading 5 Char"
            | "Heading 6 Char"
    )
}

#[test]
fn complete_vocabulary_has_real_uses_with_only_the_frozen_exemptions() {
    let output = emit(None);
    let parts = math_xml::zip_parts(&output.bytes);
    let styles = style_map(&tree(&parts, "word/styles.xml"));
    let inventory: BTreeSet<_> = styles.values().cloned().collect();
    let expected: BTreeSet<_> = VOCABULARY
        .iter()
        .map(|(name, kind)| ((*name).to_owned(), style_kind(*kind).to_owned()))
        .collect();
    assert!(
        expected.is_subset(&inventory),
        "missing vocabulary name/kinds: {:?}",
        expected.difference(&inventory)
    );
    let extras: BTreeSet<_> =
        inventory.difference(&expected).cloned().collect();
    assert_eq!(
        extras,
        BTreeSet::from([("Callout Example".into(), "paragraph".into()),]),
        "only the required semantic custom callout adds a style"
    );
    assert_eq!(
        styles.len(),
        inventory.len(),
        "no duplicate name/kind definitions"
    );
    let mut uses: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, bytes) in &parts {
        if name != "word/document.xml"
            && name != "word/footnotes.xml"
            && !(name.starts_with("word/footer") && name.ends_with(".xml"))
        {
            continue;
        }
        let root = parse(std::str::from_utf8(bytes).unwrap());
        for (tag, kind) in [
            ("w:pStyle", "paragraph"),
            ("w:rStyle", "character"),
            ("w:tblStyle", "table"),
        ] {
            for node in root.descendants(tag) {
                let id = node.attr("w:val").unwrap();
                let (style, actual_kind) = styles
                    .get(id)
                    .unwrap_or_else(|| panic!("{name}: missing style {id}"));
                assert_eq!(actual_kind, kind, "{name}: {style}");
                uses.entry(style.clone()).or_default().insert(name.clone());
            }
        }
    }
    let missing: Vec<_> = VOCABULARY
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| {
            !pure_base_or_link(name) && !UNREACHABLE_TOKENS.contains(name)
        })
        .filter(|name| !uses.contains_key(*name))
        .collect();
    assert!(
        missing.is_empty(),
        "reachable showcase styles not used: {missing:?}"
    );
    assert!(uses.contains_key("Verbatim Char"));
    for token in UNREACHABLE_TOKENS {
        assert!(
            !uses.contains_key(*token),
            "unreachable token was manufactured: {token}"
        );
    }
    insta::assert_json_snapshot!("style_use", uses);
}

#[test]
fn hand_resolved_showcase_keeps_fixed_heading_toc_and_equation_identity() {
    let (document, assets) = showcase_document();
    assert_eq!(
        document.meta.title.as_deref(),
        Some("Writer DOCX style showcase")
    );
    assert!(
        document.meta.date.is_some(),
        "Date style needs its visible sample"
    );
    assert_eq!(document.source.path, Path::new("/__writer-showcase.md"));
    assert_eq!(document.source.base_dir, Path::new("/"));
    assert_eq!(document.source.kind, SourceKind::Markdown);
    assert!(document.includes.is_empty());
    let violations = document.invariant_violations();
    assert!(violations.is_empty(), "{violations:?}");
    let mut headings = BTreeMap::new();
    let mut equations = Vec::new();
    ir::walk_blocks(&document, &mut |block| match block {
        Block::Heading { level, inlines, attrs, number, .. } => {
            if let Some(id) = &attrs.id {
                headings.insert(
                    id.clone(),
                    (
                        *level,
                        ir::plain_text(inlines),
                        attrs.clone(),
                        number.clone(),
                    ),
                );
            }
        }
        Block::Math { source, dialect, attrs, number, .. } => {
            equations.push((
                source.clone(),
                *dialect,
                attrs.id.clone(),
                number.clone(),
            ));
        }
        _ => {}
    });
    for level in 1..=6 {
        for unlisted in [false, true] {
            let id = if unlisted {
                format!("showcase-unlisted-{level}")
            } else {
                format!("showcase-heading-{level}")
            };
            let (actual_level, text, attrs, number) = &headings[&id];
            let suffix = if unlisted { "unlisted" } else { "numbered" };
            assert_eq!(*actual_level, level);
            assert_eq!(text, &format!("Heading {level} ({suffix})"));
            assert_eq!(
                attrs.classes.iter().any(|class| class == "notoc"),
                unlisted
            );
            let label = &document.resolved.labels[&id];
            assert_eq!(&label.number, number);
            if !unlisted {
                assert!(number.is_some());
            }
        }
    }
    assert_eq!(document.resolved.toc.len(), 6);
    for (index, entry) in document.resolved.toc.iter().enumerate() {
        let level = index as u8 + 1;
        assert_eq!(entry.anchor, format!("showcase-heading-{level}"));
        assert_eq!(entry.level, level);
        assert_eq!(entry.number, headings[&entry.anchor].3);
    }
    assert!(document.resolved.bibliography.is_some());
    assert!(equations.iter().any(|(source, dialect, id, number)| source
        == r"\frac{a}{b}"
        && *dialect == MathDialect::Latex
        && id.as_deref() == Some("showcase-equation")
        && number.as_deref() == Some("1")));
    assert!(
        equations
            .iter()
            .all(|(_, dialect, _, _)| *dialect == MathDialect::Latex)
    );
    assert_eq!(
        document.resolved.labels["showcase-equation"].number.as_deref(),
        Some("1")
    );
    assert_eq!(assets.len(), 1);
    let asset = assets.iter().next().unwrap();
    assert_eq!(asset.mime, "image/png");
    let AssetData::Bytes(bytes) = &asset.data else {
        panic!("the built-in PNG must not depend on a runtime file");
    };
    let size = imagesize::blob_size(bytes).unwrap();
    assert_eq!((size.width, size.height), (64, 32));
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(bytes[25], 0, "the fixed sample uses grayscale PNG pixels");
    let output = emit(None);
    let parts = math_xml::zip_parts(&output.bytes);
    let body = tree(&parts, "word/document.xml");
    assert!(!body.descendants("m:f").is_empty(), "editable LaTeX fraction");
    assert!(
        body.descendants("w:bookmarkStart")
            .iter()
            .any(|bookmark| bookmark.attr("w:name")
                == Some(anchor("showcase-equation").as_str()))
    );
    assert!(
        body.descendants("w:instrText")
            .iter()
            .any(|field| field.text.contains("SEQ Equation"))
    );
    let media: Vec<_> = parts
        .iter()
        .filter(|(name, _)| name.starts_with("word/media/"))
        .collect();
    assert_eq!(media.len(), 1);
    assert_eq!(media[0].1, bytes);
}

#[test]
fn reference_bytes_are_deterministic_timestamp_free_and_independently_readable()
{
    let first = reference_docx().unwrap();
    assert_eq!(first, reference_docx().unwrap());
    let output = emit(None);
    assert_eq!(
        first, output.bytes,
        "the public reference uses exactly the native showcase"
    );
    let parts = math_xml::zip_parts(&first);
    let core = math_xml::part(&parts, "docProps/core.xml");
    assert!(
        !core.contains("dcterms:created") && !core.contains("dcterms:modified")
    );
    for (name, bytes) in parts {
        let snapshot = if name.ends_with(".xml") || name.ends_with(".rels") {
            math_xml::xml_pretty(std::str::from_utf8(&bytes).unwrap())
        } else {
            format!(
                "{} bytes\nBLAKE3 {}",
                bytes.len(),
                blake3::hash(&bytes).to_hex()
            )
        };
        let label: String = name
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        insta::assert_snapshot!(format!("package__{label}"), snapshot);
    }
    let counts = common_docx::read_back(&first);
    assert!(counts.paragraphs > 0);
    assert!(counts.styles >= VOCABULARY.len());
    assert!(counts.abstract_nums >= 2);
    assert_eq!(counts.images, 1);
    insta::assert_json_snapshot!("read_back", counts);
}

#[test]
fn actual_pandoc_default_reference_merges_the_complete_native_showcase() {
    let version = match assert_cmd::Command::new("pandoc")
        .arg("--version")
        .timeout(Duration::from_secs(10))
        .output()
    {
        Ok(output) => {
            assert!(output.status.success());
            String::from_utf8(output.stdout).unwrap()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "skipping optional Pandoc default reference: pandoc not on PATH"
            );
            return;
        }
        Err(error) => panic!("pandoc --version: {error}"),
    };
    let directory = tempfile::tempdir().unwrap();
    let reference = directory.path().join("pandoc-reference.docx");
    let output = assert_cmd::Command::new("pandoc")
        .arg("--print-default-data-file=reference.docx")
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.starts_with(b"PK\x03\x04"),
        "Pandoc must return its actual default DOCX bytes"
    );
    let reference_parts = math_xml::zip_parts(&output.stdout);
    assert!(reference_parts.contains_key("word/document.xml"));
    assert!(reference_parts.contains_key("word/styles.xml"));
    // Print-default-data-file returns binary stdout even when --output is set.
    // Keep the actual bytes only in this test's temporary directory.
    std::fs::write(&reference, &output.stdout).unwrap();
    eprintln!("optional reference oracle: {}", version.lines().next().unwrap());
    let output = emit(Some(&reference));
    let counts = common_docx::read_back(&output.bytes);
    assert!(counts.paragraphs > 0 && counts.styles > 0);
    assert!(counts.images >= 1);
}
