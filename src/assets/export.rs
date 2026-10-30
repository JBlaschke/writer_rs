//! Pure filename reservations shared by Typst and LaTeX source exports.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::registry::{AssetRegistry, normalize_lexically};
use crate::config::Config;
use crate::ir::{Block, Document, Inline};

/// Basenames only; each caller supplies its own `<stem>_assets/` prefix.
pub(crate) struct ExportAssets {
    pub typst: BTreeMap<String, String>,
    pub latex: BTreeMap<String, LatexAssetName>,
    pub namespace: ExportNamespace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LatexAssetName {
    pub name: String,
    pub render: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Owner {
    Asset(String),
    Rendition(String),
    Resource(PathBuf),
}

/// Ownership lets both targets copy the same resource under its ordinary name.
/// Different sources/renditions always occupy different ASCII-folded names.
#[derive(Debug, Default, Clone)]
pub(crate) struct ExportNamespace {
    occupied: BTreeSet<String>,
    owners: BTreeMap<String, Owner>,
}

impl ExportNamespace {
    fn allocate(&mut self, hint: &str, suffix: &str, owner: Owner) -> String {
        let name = export_name(hint, suffix, &mut self.occupied);
        self.owners.insert(name.to_ascii_lowercase(), owner);
        name
    }

    /// Called only after the existing I/O planner has read and hashed a file.
    /// Canonical aliases are still deduplicated by that caller, not here.
    pub(crate) fn resource_name(
        &mut self, source: &Path, hint: &str, suffix: &str,
    ) -> String {
        let owner = Owner::Resource(normalize_lexically(source));
        let preferred = portable_name(hint);
        if self.owners.get(&preferred.to_ascii_lowercase()) == Some(&owner) {
            return preferred;
        }
        self.allocate(hint, suffix, owner)
    }
}

/// Preserve T34's complete referenced set first; add the other target later.
/// No registry bytes or resource files are read by this planner.
pub(crate) fn assets(
    document: &Document, registry: &AssetRegistry,
) -> ExportAssets {
    let typst_ids = referenced(document, "typst", true);
    let latex_ids = referenced(document, "latex", false);
    let mut namespace = ExportNamespace::default();
    let mut bases = BTreeMap::new();
    for ids in [&typst_ids, &latex_ids] {
        for asset in registry.iter().filter(|asset| ids.contains(&asset.id)) {
            if !bases.contains_key(&asset.id) {
                let name = namespace.allocate(
                    &asset.file_name_hint,
                    &asset.id[..6],
                    Owner::Asset(asset.id.clone()),
                );
                bases.insert(asset.id.clone(), name);
            }
        }
    }
    let typst = bases
        .iter()
        .filter(|(id, _)| typst_ids.contains(*id))
        .map(|(id, name)| (id.clone(), name.clone()))
        .collect();
    let mut latex = BTreeMap::new();
    for asset in registry.iter().filter(|asset| latex_ids.contains(&asset.id)) {
        let Some((extension, render)) = latex_extension(&asset.mime) else {
            continue;
        };
        let base = &bases[&asset.id];
        let matches = base.rsplit_once('.').is_some_and(|(_, current)| {
            current.eq_ignore_ascii_case(extension)
                || asset.mime == "image/jpeg"
                    && current.eq_ignore_ascii_case("jpeg")
        });
        let name = if !render && matches {
            base.clone()
        } else {
            namespace.allocate(
                &format!("{base}.{extension}"),
                &asset.id[..6],
                if render {
                    Owner::Rendition(asset.id.clone())
                } else {
                    Owner::Asset(asset.id.clone())
                },
            )
        };
        latex.insert(asset.id.clone(), LatexAssetName { name, render });
    }
    ExportAssets { typst, latex, namespace }
}

fn latex_extension(mime: &str) -> Option<(&'static str, bool)> {
    match mime {
        "image/png" => Some(("png", false)),
        "image/jpeg" => Some(("jpg", false)),
        "application/pdf" => Some(("pdf", false)),
        "image/svg+xml" | "image/gif" | "image/webp" => Some(("pdf", true)),
        _ => None,
    }
}

/// BibTeX filenames use their own ordinal fallback, after all image names.
/// Paths are the supplied resource identities; canonicalization belongs to I/O.
pub(crate) fn bibliography(
    namespace: &mut ExportNamespace, config: &Config,
) -> Vec<(PathBuf, String)> {
    let mut result = Vec::new();
    for source in &config.references.bibliography {
        if source.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("yaml")
                || extension.eq_ignore_ascii_case("yml")
        }) {
            continue;
        }
        let hint = source.file_name().unwrap_or_default().to_string_lossy();
        let mut original = portable_name(&hint);
        if !original.ends_with(".bib") {
            original.push_str(".bib");
        }
        let stem = original.strip_suffix(".bib").expect("bib extension");
        let mut name = original.clone();
        let mut ordinal = 2;
        while !namespace.occupied.insert(name.to_ascii_lowercase()) {
            name = format!("{stem}-{ordinal}.bib");
            ordinal += 1;
        }
        namespace.owners.insert(
            name.to_ascii_lowercase(),
            Owner::Resource(normalize_lexically(source)),
        );
        result.push((source.clone(), name));
    }
    result
}

/// Exact T34 normalization, shared by hashed and ordinal collision allocation.
fn portable_name(hint: &str) -> String {
    let mut name: String = hint
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("asset")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    name.truncate(name.trim_end_matches('.').len());
    if name.is_empty() {
        name = "asset".into();
    }
    let base = name.split('.').next().unwrap_or_default().to_ascii_uppercase();
    let numbered_device = base
        .strip_prefix("COM")
        .or_else(|| base.strip_prefix("LPT"))
        .is_some_and(|digit| matches!(digit.as_bytes(), [b'1'..=b'9']));
    if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered_device
    {
        name.insert_str(0, "asset-");
    }
    name
}

/// Allocates exactly as T34 did before introducing target-specific candidates.
pub(crate) fn export_name(
    hint: &str, suffix_key: &str, occupied: &mut BTreeSet<String>,
) -> String {
    let original = portable_name(hint);
    let mut name = original.clone();
    let mut collision = 1;
    while !occupied.insert(name.to_ascii_lowercase()) {
        let (stem, extension) =
            original.rsplit_once('.').unwrap_or((&original, ""));
        let suffix = if collision == 1 {
            suffix_key.to_owned()
        } else {
            format!("{suffix_key}-{collision}")
        };
        name = if extension.is_empty() {
            format!("{stem}-{suffix}")
        } else {
            format!("{stem}-{suffix}.{extension}")
        };
        collision += 1;
    }
    name
}

fn referenced(
    document: &Document, target: &str, links: bool,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut collect = |inline: &Inline| {
        let url = match inline {
            Inline::Image { url, .. } => url,
            Inline::Link { url, .. } if links => url,
            _ => return,
        };
        if let Some(id) = url.strip_prefix("asset:") {
            ids.insert(id.split(['?', '#']).next().unwrap_or(id).to_owned());
        }
    };
    if let Some(abstract_) = &document.meta.abstract_ {
        visit_blocks(abstract_, target, &mut collect);
    }
    visit_blocks(&document.blocks, target, &mut collect);
    ids
}

fn visit_blocks(
    blocks: &[Block], target: &str, visit: &mut dyn FnMut(&Inline),
) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. }
            | Block::Paragraph { inlines, .. } => {
                visit_inlines(inlines, target, visit);
            }
            Block::BlockQuote { blocks, .. } | Block::Div { blocks, .. } => {
                visit_blocks(blocks, target, visit);
            }
            Block::Figure { body, caption, .. } => {
                visit_blocks(body, target, visit);
                if let Some(caption) = caption {
                    visit_inlines(caption, target, visit);
                }
            }
            Block::Callout { title, blocks, .. } => {
                if let Some(title) = title {
                    visit_inlines(title, target, visit);
                }
                visit_blocks(blocks, target, visit);
            }
            Block::List { items, .. } => {
                for item in items {
                    visit_blocks(&item.blocks, target, visit);
                }
            }
            Block::DefinitionList { items, .. } => {
                for item in items {
                    visit_inlines(&item.term, target, visit);
                    for blocks in &item.definitions {
                        visit_blocks(blocks, target, visit);
                    }
                }
            }
            Block::Table { head, body, foot, .. } => {
                for row in head.iter().chain(body).chain(foot) {
                    for cell in &row.cells {
                        visit_inlines(&cell.inlines, target, visit);
                    }
                }
            }
            Block::Cell(cell) if cell.show_output => {
                for output in &cell.outputs {
                    visit_blocks(output.pick(target), target, visit);
                }
            }
            _ => {}
        }
    }
}

fn visit_inlines(
    inlines: &[Inline], target: &str, visit: &mut dyn FnMut(&Inline),
) {
    for inline in inlines {
        visit(inline);
        match inline {
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Link { inlines, .. }
            | Inline::Span { inlines, .. } => {
                visit_inlines(inlines, target, visit)
            }
            Inline::Footnote { blocks, .. } => {
                visit_blocks(blocks, target, visit)
            }
            Inline::Ref { supplement, resolved, .. } => {
                if let Some(inlines) = supplement {
                    visit_inlines(inlines, target, visit);
                }
                if let Some(resolved) = resolved {
                    visit_inlines(&resolved.text, target, visit);
                }
            }
            Inline::Cite { items, .. } => {
                for item in items {
                    if let Some(prefix) = &item.prefix {
                        visit_inlines(prefix, target, visit);
                    }
                    if let Some(suffix) = &item.suffix {
                        visit_inlines(suffix, target, visit);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Attrs, Cell, CellOutput, DocMeta, Features, OutputCandidate, RefForm,
        Resolved, SourceInfo, SourceKind,
    };

    fn document(blocks: Vec<Block>) -> Document {
        Document {
            meta: DocMeta::default(),
            blocks,
            features: Features::default(),
            source: SourceInfo {
                path: "/project/main.md".into(),
                base_dir: "/project".into(),
                kind: SourceKind::Markdown,
            },
            assets: Vec::new(),
            includes: Vec::new(),
            resolved: Resolved::default(),
        }
    }

    fn image(id: &str) -> Inline {
        Inline::Image {
            alt: Vec::new(),
            url: format!("asset:{id}"),
            title: None,
            attrs: Attrs::default(),
        }
    }

    fn link(id: &str) -> Inline {
        Inline::Link {
            inlines: Vec::new(),
            url: format!("asset:{id}"),
            title: None,
            attrs: Attrs::default(),
        }
    }

    fn output(typst: &str, latex: &str, unused: &str) -> Block {
        Block::Cell(Cell {
            show_output: true,
            outputs: vec![CellOutput {
                candidates: vec![
                    OutputCandidate {
                        mime: "application/pdf".into(),
                        blocks: vec![
                            Block::Raw {
                                format: "latex".into(),
                                text: String::new(),
                                span: None,
                            },
                            Block::paragraph(vec![image(latex)]),
                        ],
                    },
                    OutputCandidate {
                        mime: "image/svg+xml".into(),
                        blocks: vec![Block::paragraph(vec![image(typst)])],
                    },
                    OutputCandidate {
                        mime: "image/png".into(),
                        blocks: vec![Block::paragraph(vec![image(unused)])],
                    },
                ],
                ..CellOutput::default()
            }],
            ..Cell::default()
        })
    }

    #[test]
    fn typst_reservations_win_even_when_a_latex_only_id_sorts_first() {
        let mut registry = AssetRegistry::new();
        let one = registry.register_bytes(vec![1], "application/pdf", "plot");
        let two = registry.register_bytes(vec![2], "image/svg+xml", "plot");
        let unused = registry.register_bytes(vec![3], "image/png", "plot");
        for (typst, latex) in [(&one, &two), (&two, &one)] {
            let mut document = document(vec![output(typst, latex, &unused)]);
            document.blocks.push(Block::Cell(Cell {
                show_output: false,
                outputs: vec![CellOutput {
                    candidates: vec![OutputCandidate {
                        mime: "image/png".into(),
                        blocks: vec![Block::paragraph(vec![image(&unused)])],
                    }],
                    ..CellOutput::default()
                }],
                ..Cell::default()
            }));
            let plan = assets(&document, &registry);
            assert_eq!(plan.typst.len(), 1);
            assert_eq!(plan.typst[typst], "plot");
            let extra = format!("plot-{}", &latex[..6]);
            assert!(plan.latex[latex].name.starts_with(&format!("{extra}.")));
            assert!(!plan.typst.contains_key(&unused));
            assert!(!plan.latex.contains_key(&unused));
            assert_eq!(
                assets(&document, &registry).latex,
                plan.latex,
                "planning has no target-order or session input",
            );
        }
    }

    #[test]
    fn typst_links_and_nested_reference_images_keep_the_legacy_set() {
        let mut registry = AssetRegistry::new();
        let link_id = registry.register_bytes(vec![1], "image/png", "link.png");
        let note_id = registry.register_bytes(vec![2], "image/png", "note.png");
        let ref_id = registry.register_bytes(vec![3], "image/png", "ref.png");
        let mut document = document(vec![Block::paragraph(vec![
            link(&link_id),
            Inline::Footnote {
                blocks: vec![Block::paragraph(vec![image(&note_id)])],
                label: None,
                number: None,
            },
            Inline::Ref {
                target: "somewhere".into(),
                supplement: Some(vec![image(&ref_id)]),
                form: RefForm::Normal,
                resolved: None,
                span: None,
            },
        ])]);
        document.meta.abstract_ =
            Some(vec![Block::paragraph(vec![image(&note_id)])]);
        let plan = assets(&document, &registry);
        assert_eq!(plan.typst.len(), 3);
        assert_eq!(plan.typst[&link_id], "link.png");
        assert!(!plan.latex.contains_key(&link_id));
        assert_eq!(plan.latex[&note_id].name, "note.png");
        assert_eq!(plan.latex[&ref_id].name, "ref.png");
    }

    #[test]
    fn derived_names_respect_every_base_and_keep_correct_direct_extensions() {
        let mut registry = AssetRegistry::new();
        let svg =
            registry.register_bytes(vec![1], "image/svg+xml", "diagram.svg");
        let pdf = registry.register_bytes(
            vec![2],
            "application/pdf",
            "diagram.svg.pdf",
        );
        let png = registry.register_bytes(vec![3], "image/png", "wrong.svg");
        let jpg = registry.register_bytes(vec![4], "image/jpeg", "UPPER.JPEG");
        let gif = registry.register_bytes(vec![5], "image/gif", "anim.gif");
        let webp = registry.register_bytes(vec![6], "image/webp", "anim.webp");
        let other =
            registry.register_bytes(vec![7], "application/zip", "unused.pdf");
        let ids = [&svg, &pdf, &png, &jpg, &gif, &webp, &other];
        let document = document(vec![Block::paragraph(
            ids.into_iter().map(|id| image(id)).collect(),
        )]);
        let mut plan = assets(&document, &registry);
        assert_eq!(plan.typst[&svg], "diagram.svg");
        assert_eq!(plan.typst[&pdf], "diagram.svg.pdf");
        assert_eq!(
            plan.latex[&svg].name,
            format!("diagram.svg-{}.pdf", &svg[..6])
        );
        assert!(plan.latex[&svg].render);
        assert_eq!(plan.latex[&pdf].name, "diagram.svg.pdf");
        assert!(!plan.latex[&pdf].render);
        assert_eq!(plan.latex[&png].name, "wrong.svg.png");
        assert!(!plan.latex[&png].render);
        assert_eq!(plan.latex[&jpg].name, "UPPER.JPEG");
        for (id, name) in [(&gif, "anim.gif.pdf"), (&webp, "anim.webp.pdf")] {
            assert_eq!(plan.latex[id].name, name);
            assert!(plan.latex[id].render);
        }
        assert!(!plan.latex.contains_key(&other));
        assert_eq!(plan.typst[&other], "unused.pdf");
        assert_eq!(
            plan.namespace.resource_name(
                Path::new("/res/anim.gif.pdf"),
                "anim.gif.pdf",
                "abc123"
            ),
            "anim.gif-abc123.pdf",
        );
    }

    #[test]
    fn bibliography_claims_keep_ordinary_typst_names_and_hash_collisions() {
        let mut config = Config::default();
        config.references.bibliography = [
            "/first/refs.bib", "/second/refs.bib", "/notes", "/skip.YAML",
            "/skip.YmL",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        let mut namespace = ExportNamespace::default();
        let bibliography = bibliography(&mut namespace, &config);
        assert_eq!(
            bibliography
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>(),
            ["refs.bib", "refs-2.bib", "notes.bib"]
        );
        assert_eq!(
            namespace.resource_name(
                Path::new("/first/./refs.bib"),
                "refs.bib",
                "abc123"
            ),
            "refs.bib",
        );
        assert_eq!(
            namespace.resource_name(
                Path::new("/second/refs.bib"),
                "refs.bib",
                "abc123"
            ),
            "refs-abc123.bib",
        );
        assert_eq!(
            namespace.resource_name(Path::new("/notes"), "notes", "abc123"),
            "notes"
        );
        assert_eq!(
            namespace.resource_name(
                Path::new("/different/refs.bib"),
                "refs.bib",
                "abc123"
            ),
            "refs-abc123-2.bib",
        );
    }

    #[test]
    fn a_new_latex_bib_collision_alone_advances_the_old_resource_suffix() {
        let mut config = Config::default();
        config.references.bibliography =
            ["/first/refs.bib", "/second/refs.bib", "/third/refs-abc123.bib"]
                .into_iter()
                .map(PathBuf::from)
                .collect();
        let mut namespace = ExportNamespace::default();
        bibliography(&mut namespace, &config);
        assert_eq!(
            namespace.resource_name(
                Path::new("/second/refs.bib"),
                "refs.bib",
                "abc123"
            ),
            "refs-abc123-2.bib",
        );
        assert_eq!(
            namespace.resource_name(
                Path::new("/third/refs-abc123.bib"),
                "refs-abc123.bib",
                "fedcba"
            ),
            "refs-abc123.bib",
        );
    }

    #[test]
    fn portable_names_and_ordinal_bib_collisions_share_ascii_folded_keys() {
        let mut occupied = BTreeSet::new();
        for (hint, expected) in [
            ("CON.svg", "asset-CON.svg"),
            ("lpt9", "asset-lpt9"),
            ("...", "asset"),
            ("space name.png.", "space-name.png"),
            ("COM10.svg", "COM10.svg"),
        ] {
            assert_eq!(export_name(hint, "abc123", &mut occupied), expected);
        }
        assert_eq!(
            export_name("CON.svg", "abc123", &mut occupied),
            "asset-CON-abc123.svg"
        );
        assert_eq!(
            export_name("con.SVG", "abc123", &mut occupied),
            "asset-con-abc123-2.SVG"
        );
        let mut config = Config::default();
        config.references.bibliography =
            ["/A/refs.bib", "/B/REFS.bib", "/C/refs.bib"]
                .into_iter()
                .map(PathBuf::from)
                .collect();
        let names = bibliography(&mut ExportNamespace::default(), &config);
        assert_eq!(
            names.into_iter().map(|(_, name)| name).collect::<Vec<_>>(),
            ["refs.bib", "REFS-2.bib", "refs-3.bib"]
        );
    }
}
