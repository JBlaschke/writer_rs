//! Unresolved Markdown fragments and saved-notebook modules for native Typst.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::assets::registry::{AssetData, AssetRegistry};
use crate::config::{Config, Toc};
use crate::error::{Diagnostic, Diagnostics, Error, Result};
use crate::ir::{
    self, Block, DocMeta, Document, Parsed, SourceInfo, SourceKind, SourceSpan,
};
use crate::markdown;
use crate::notebook::{self, CellKind, CellParts, ParseOptions, Selection};

use super::emit::{self, AssetPaths, EmitOptions, Emitted, IncludeRun};
use super::sourcemap::SourceMap;
use super::world::{IncludeProvider, Synthesized};

/// Effective config used when synthesizing a Markdown or notebook include.
pub struct MarkdownIncludes {
    /// Parent document's effective settings.
    pub config: Config,
}

impl IncludeProvider for MarkdownIncludes {
    fn synthesize(&self, path: &Path, root: &Path) -> Result<Synthesized> {
        let root =
            fs::canonicalize(root).map_err(|error| read_error(error, root))?;
        let path =
            fs::canonicalize(path).map_err(|error| read_error(error, path))?;
        let relative = path.strip_prefix(&root).map_err(|_| {
            Error::Other(format!(
                "cannot include {}: source is outside Typst project root {}",
                path.display(),
                root.display(),
            ))
        })?;
        let relative = relative.to_str().ok_or_else(|| Error::Other(format!(
            "cannot include {}: root-relative source path must be UTF-8",
            path.display(),
        )))?.replace('\\', "/");
        let prefix =
            format!("{}-", &blake3::hash(relative.as_bytes()).to_hex()[..8]);
        let kind = ir::detect_kind(&path)?;
        let source = SourceInfo {
            path: path.clone(),
            kind,
            base_dir: path.parent().unwrap_or(&root).into(),
        };
        let text = fs::read_to_string(&path)
            .map_err(|error| read_error(error, &path))?;
        let mut config = self.config.clone();
        config.document.toc = Toc::Off;
        let (mut parsed, cells) = match kind {
            SourceKind::Markdown => {
                (markdown::parse(&source, &text, &config)?, None)
            }
            SourceKind::Notebook => {
                let mut includes =
                    markdown::include::cell_includes_in(&source, &[]);
                let (parsed, cells) = notebook::parse_cells(
                    &source,
                    &text,
                    &config,
                    ParseOptions {
                        includes: &mut *includes,
                        nested: true,
                        selection: Selection::default(),
                    },
                )?;
                (parsed, Some(cells))
            }
            SourceKind::Typst => {
                return Err(Error::Other(format!(
                    "include provider expects Markdown or notebook source: {}",
                    path.display(),
                )));
            }
        };
        if matches!(
            parsed.document.blocks.first(),
            Some(Block::Toc { span: None, .. })
        ) {
            parsed.document.blocks.remove(0);
        }
        markdown::include::tag_document(&mut parsed, &path);
        let keys = markdown::include::explicit_keys(&text, kind);
        if !keys.is_empty() {
            parsed.diagnostics.push(
                Diagnostic::warning(
                    "include-meta-ignored",
                    format!(
                        "included document metadata is ignored: {}",
                        keys.iter()
                            .map(|key| format!("`{key}`"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                )
                .with_path(&path)
                .with_span(SourceSpan {
                    start_line: 1,
                    end_line: 1,
                    cell: None,
                    file: Some(path.clone()),
                }),
            );
        }
        // Parsing owns the child's metadata/config, but native fragment
        // emission owns only its body. Ignored abstracts must not reserve
        // imports, footnote labels or asset paths during whole-document scans.
        parsed.document.meta = DocMeta::default();
        let files = dependencies(&parsed, &path);
        let options = EmitOptions {
            config: &config,
            assets: AssetPaths::Virtual,
            template: None,
            bibliography: Vec::new(),
            style: config.references.style.clone(),
            footnote_prefix: prefix,
            fragment: true,
        };
        let mut diagnostics = Diagnostics::new();
        extend_unique(&mut diagnostics, parsed.diagnostics);
        let (text, map) = match cells {
            Some(cells) => notebook_module(
                &parsed.document, &parsed.assets, &cells, &options, &path,
                &mut diagnostics,
            )?,
            None => {
                let emitted =
                    emit::emit(&parsed.document, &parsed.assets, &options)?;
                extend_unique(&mut diagnostics, emitted.diagnostics);
                (emitted.text, emitted.map)
            }
        };
        Ok(Synthesized { text, map, assets: parsed.assets, files, diagnostics })
    }
}

fn read_error(error: std::io::Error, path: &Path) -> Error {
    Error::Io(std::io::Error::new(
        error.kind(),
        format!("cannot read include {}: {error}", path.display(),),
    ))
}

fn dependencies(parsed: &Parsed, source: &Path) -> Vec<PathBuf> {
    let mut found = BTreeSet::from([source.to_path_buf()]);
    let mut files = Vec::new();
    for path in parsed
        .document
        .includes
        .iter()
        .cloned()
        .chain(parsed.assets.iter().filter_map(|asset| match &asset.data {
            AssetData::File(path) => Some(path.clone()),
            AssetData::Bytes(_) => None,
        }))
        .chain(
            parsed
                .diagnostics
                .iter()
                .filter_map(markdown::walker::missing_asset_path),
        )
    {
        if found.insert(path.clone()) {
            files.push(path);
        }
    }
    files
}

fn extend_unique(target: &mut Diagnostics, findings: Diagnostics) {
    for finding in findings {
        if !target.iter().any(|existing| existing == &finding) {
            target.push(finding);
        }
    }
}

#[derive(Clone, Copy)]
enum Variant {
    Body,
    Input,
    Outputs,
}

struct Fragment {
    text: Vec<(String, Option<SourceSpan>)>,
}

struct Assembly<'a> {
    imports: &'a mut Vec<String>,
    diagnostics: &'a mut Diagnostics,
}

#[derive(Default)]
struct Module {
    text: String,
    lines: Vec<Option<SourceSpan>>,
}

impl Module {
    fn wrapper(&mut self, text: &str) {
        self.text.push_str(text);
        self.text.push('\n');
        self.lines.push(None);
    }

    fn body(&mut self, fragment: &Fragment) {
        // Keep literal/raw content at its emitted columns: adding indentation
        // could change code bytes inside a fenced raw block.
        for (text, span) in &fragment.text {
            self.text.push_str(text);
            self.text.push('\n');
            self.lines.push(span.clone());
        }
    }
}

fn notebook_module(
    document: &Document, assets: &AssetRegistry, cells: &[CellParts],
    options: &EmitOptions<'_>, path: &Path, diagnostics: &mut Diagnostics,
) -> Result<(String, SourceMap)> {
    let mut imports = Vec::new();
    let (bodies, inputs, outputs) = {
        let mut assembly = Assembly { imports: &mut imports, diagnostics };
        let bodies = cell_fragments(
            document,
            assets,
            cells,
            Variant::Body,
            options,
            path,
            &mut assembly,
        )?;
        let inputs = cell_fragments(
            document,
            assets,
            cells,
            Variant::Input,
            options,
            path,
            &mut assembly,
        )?;
        let outputs = cell_fragments(
            document,
            assets,
            cells,
            Variant::Outputs,
            options,
            path,
            &mut assembly,
        )?;
        (bodies, inputs, outputs)
    };
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .replace(['\r', '\n'], " ");
    let mut module = Module::default();
    module.wrapper(&format!(
        "// Generated by writer from {filename}; edits are lost.",
    ));
    for import in imports {
        module.wrapper(&import);
    }
    module.wrapper("#let writer-cells = (");
    for (index, cell) in cells.iter().enumerate() {
        module.wrapper("  (");
        module.wrapper(&format!("    index: {},", cell.index));
        let id = cell.id.as_deref().map_or_else(|| "none".into(), emit::string);
        module.wrapper(&format!("    id: {id},"));
        let tags = match cell.tags.as_slice() {
            [] => "()".into(),
            [tag] => format!("({},)", emit::string(tag)),
            tags => format!(
                "({})",
                tags.iter()
                    .map(|tag| emit::string(tag))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        module.wrapper(&format!("    tags: {tags},"));
        let kind = match &cell.kind {
            CellKind::Markdown => "markdown",
            CellKind::Code => "code",
            CellKind::Raw => "raw",
        };
        module.wrapper(&format!("    kind: {},", emit::string(kind)));
        module.wrapper("    body: [");
        module.body(&bodies[index]);
        module.wrapper("    ],");
        if cell.kind == CellKind::Code {
            for (name, fragments) in [("input", &inputs), ("outputs", &outputs)]
            {
                let fragment = &fragments[index];
                if fragment.text.is_empty() {
                    module.wrapper(&format!("    {name}: none,"));
                } else {
                    module.wrapper(&format!("    {name}: ["));
                    module.body(fragment);
                    module.wrapper("    ],");
                }
            }
        }
        module.wrapper("  ),");
    }
    module.wrapper(")");
    module.wrapper("#for c in writer-cells { c.body }");
    Ok((module.text, SourceMap::from_lines(module.lines)))
}

fn cell_fragments(
    document: &Document, assets: &AssetRegistry, cells: &[CellParts],
    variant: Variant, options: &EmitOptions<'_>, path: &Path,
    assembly: &mut Assembly<'_>,
) -> Result<Vec<Fragment>> {
    let mut parsed = Parsed {
        document: document.clone(),
        assets: AssetRegistry::new(),
        diagnostics: Diagnostics::new(),
    };
    parsed.document.blocks.clear();
    let mut runs = Vec::new();
    for cell in cells {
        let blocks = if cell.kind == CellKind::Code {
            match variant {
                Variant::Body => &cell.body,
                Variant::Input => &cell.input,
                Variant::Outputs => &cell.outputs,
            }
        } else {
            &cell.body
        };
        let start = parsed.document.blocks.len();
        parsed.document.blocks.extend(blocks.iter().cloned());
        let end = parsed.document.blocks.len();
        if start < end {
            runs.push(IncludeRun {
                start,
                end,
                file: format!("cell-{}.typ", cell.index),
            });
        }
    }
    markdown::include::tag_document(&mut parsed, path);
    let (main, fragments) =
        emit::emit_split(&parsed.document, assets, options, &runs)?;
    extend_unique(assembly.diagnostics, main.diagnostics);
    let mut fragments = fragments.into_iter();
    let mut result = Vec::with_capacity(cells.len());
    for cell in cells {
        let blocks = if cell.kind == CellKind::Code {
            match variant {
                Variant::Body => &cell.body,
                Variant::Input => &cell.input,
                Variant::Outputs => &cell.outputs,
            }
        } else {
            &cell.body
        };
        result.push(if blocks.is_empty() {
            Fragment { text: Vec::new() }
        } else {
            let emitted =
                fragments.next().expect("one emitted part per nonempty run");
            fragment(emitted, assembly)
        });
    }
    // Even an empty notebook module imports the package, just like emit's
    // main header; no fabricated part is needed to manufacture that import.
    header_imports(&main.text, assembly.imports);
    Ok(result)
}

fn header_imports(text: &str, imports: &mut Vec<String>) -> usize {
    let mut skipped = 0;
    for line in text.lines() {
        if skipped == 0 && line.starts_with("// Generated by writer from ") {
            skipped += 1;
        } else if line.starts_with("#import ") {
            if !imports.iter().any(|existing| existing == line) {
                imports.push(line.into());
            }
            skipped += 1;
        } else if line.is_empty() {
            skipped += 1;
            break;
        } else {
            break;
        }
    }
    skipped
}

fn fragment(emitted: Emitted, assembly: &mut Assembly<'_>) -> Fragment {
    let skipped = header_imports(&emitted.text, assembly.imports);
    extend_unique(assembly.diagnostics, emitted.diagnostics);
    Fragment {
        text: emitted
            .text
            .lines()
            .enumerate()
            .skip(skipped)
            .map(|(index, line)| {
                (line.into(), emitted.map.lookup(index + 1).cloned())
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HeadingNumbering;

    fn provider() -> MarkdownIncludes {
        MarkdownIncludes { config: Config::default() }
    }

    #[test]
    fn markdown_fragment_keeps_native_references_and_parent_numbering_policy() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("part.md");
        fs::write(&path, concat!(
            "---\ntitle: Ignored title\ntoc: true\nnumbering:\n  headings: false\n---\n",
            "# Unnumbered child {.unnumbered}\n\n",
            "See @parent. A scoped note.[^one]\n\n[^one]: Note body.\n",
        )).unwrap();
        let mut includes = provider();
        includes.config.numbering.headings =
            HeadingNumbering::Pattern("1.".into());
        let fragment = includes.synthesize(&path, directory.path()).unwrap();
        let prefix = format!("{}-", &blake3::hash(b"part.md").to_hex()[..8]);
        assert!(!fragment.text.contains("#show:"));
        assert!(!fragment.text.contains("#outline"));
        assert!(!fragment.text.contains("Ignored title"));
        assert!(fragment.text.contains("numbering: none"));
        assert!(fragment.text.contains("#ref(<parent>)"));
        assert!(!fragment.text.contains("[?parent]"));
        assert!(fragment.text.contains(&format!("writer-fn-{prefix}one")));
        assert_eq!(fragment.map.len(), fragment.text.lines().count());
        let metadata = fragment
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "include-meta-ignored")
            .collect::<Vec<_>>();
        assert_eq!(metadata.len(), 1);
        assert!(metadata[0].message.contains("`title`"));
        assert_eq!(metadata[0].span.as_ref().unwrap().start_line, 1);
        assert_eq!(metadata[0].span.as_ref().unwrap().cell, None);
    }

    #[test]
    fn notebook_module_retains_empty_cells_and_maps_body_lines_without_duplicate_findings()
     {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cells.ipynb");
        let notebook = serde_json::json!({
            "nbformat": 4, "nbformat_minor": 5,
            "metadata": {"title": "Ignored notebook title"},
            "cells": [
                {"cell_type": "markdown", "id": "empty", "metadata": {}, "source": ""},
                {"cell_type": "code", "id": "hidden", "metadata": {
                    "writer": {"echo": false, "output": false},
                }, "source": "print(1)", "outputs": [], "execution_count": 1},
                {"cell_type": "markdown", "id": "odd\"\u{1}", "metadata": {
                    "tags": ["paper", "quoted\"tag"],
                }, "source": "Late cell.\n\n$\\unknowncommand{x}$\n"},
            ],
        });
        fs::write(&path, notebook.to_string()).unwrap();
        let fragment = provider().synthesize(&path, directory.path()).unwrap();
        for index in 0..3 {
            assert!(fragment.text.contains(&format!("index: {index},")));
        }
        assert!(fragment.text.contains("id: \"empty\""));
        assert!(fragment.text.contains("input: none"));
        assert!(fragment.text.contains("outputs: none"));
        assert!(fragment.text.contains("\\u{1}"));
        assert_eq!(fragment.text.matches("#import \"@writer/doc:").count(), 1);
        assert_eq!(
            fragment.text.matches("#import \"@preview/mitex:").count(),
            1
        );
        assert_eq!(fragment.map.len(), fragment.text.lines().count());
        assert!((1..=fragment.map.len()).any(|line| {
            fragment.map.lookup(line).is_some_and(|span| {
                span.cell == Some(2)
                    && span.file.as_deref()
                        == Some(fs::canonicalize(&path).unwrap().as_path())
            })
        }));
        assert_eq!(
            fragment
                .diagnostics
                .iter()
                .filter(|finding| { finding.code == "math-conversion-failed" })
                .count(),
            1,
            "{:?}",
            fragment.diagnostics
        );
        let syntax = typst_syntax::Source::detached(&fragment.text);
        assert!(
            syntax.root().errors_and_warnings().0.is_empty(),
            "{}",
            fragment.text
        );
    }

    #[test]
    fn ignored_latex_abstract_cannot_reserve_imports_or_body_footnote_labels() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("part.md");
        fs::write(
            &path,
            concat!(
                "---\nabstract: |\n",
                "  Hidden $\\unknowncommand{x}$ and note.[^shared]\n\n",
                "  [^shared]: Ignored abstract note.\n---\n\n",
                "Plain body with its own note.[^shared]\n\n",
                "[^shared]: Kept body note.\n",
            ),
        )
        .unwrap();
        let fragment = provider().synthesize(&path, directory.path()).unwrap();
        assert!(!fragment.text.contains("@preview/mitex"));
        assert!(!fragment.text.contains("unknowncommand"));
        assert!(!fragment.text.contains("Ignored abstract note"));
        assert!(fragment.text.contains("Kept body note"));
        assert!(
            !fragment
                .diagnostics
                .iter()
                .any(|finding| { finding.code == "math-conversion-failed" })
        );
        let prefix = format!("{}-", &blake3::hash(b"part.md").to_hex()[..8]);
        let label = format!("writer-fn-{prefix}shared");
        assert_eq!(fragment.text.matches(&format!("<{label}>")).count(), 1);
        assert!(!fragment.text.contains(&format!("<{label}-2>")));
    }

    #[test]
    fn source_map_header_removal_keeps_user_imports_and_literal_columns() {
        let span = SourceSpan {
            start_line: 12,
            end_line: 12,
            cell: Some(3),
            file: Some("/project/cells.ipynb".into()),
        };
        let emitted = Emitted {
            text: concat!(
                "// Generated by writer from cells.ipynb; edit the source, not this file.\n",
                "#import \"@writer/doc:0.1.0\": *\n\n",
                "#import \"user.typ\": helper\n  literal columns  \n",
            ).into(),
            map: SourceMap::from_lines(vec![
                None, None, None, Some(span.clone()), Some(span.clone()),
            ]),
            diagnostics: Diagnostics::new(), assets: Default::default(),
        };
        let mut imports = Vec::new();
        let mut diagnostics = Diagnostics::new();
        let fragment = fragment(
            emitted,
            &mut Assembly {
                imports: &mut imports,
                diagnostics: &mut diagnostics,
            },
        );
        assert_eq!(imports, ["#import \"@writer/doc:0.1.0\": *"]);
        assert_eq!(fragment.text[0].0, "#import \"user.typ\": helper");
        assert_eq!(fragment.text[1].0, "  literal columns  ");
        let mut module = Module::default();
        module.wrapper("body: [");
        module.body(&fragment);
        module.wrapper("]");
        let map = SourceMap::from_lines(module.lines);
        assert!(map.lookup(1).is_none());
        assert_eq!(map.lookup(2), Some(&span));
        assert_eq!(map.lookup(3), Some(&span));
        assert!(map.lookup(4).is_none());
    }

    #[test]
    fn synthesis_inventories_transitive_assets_and_missing_asset_attempts_once()
    {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("root.md");
        let nested = directory.path().join("nested.md");
        let image = directory.path().join("image.svg");
        fs::write(&nested, "Nested body.\n").unwrap();
        fs::write(&image, b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"/>").unwrap();
        fs::write(
            &path,
            concat!(
                ":::include nested.md\n:::\n\n",
                "![Present](image.svg)\n\n![Missing](missing.svg)\n",
            ),
        )
        .unwrap();
        let fragment = provider().synthesize(&path, directory.path()).unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        assert_eq!(
            fragment.files,
            [
                fs::canonicalize(nested).unwrap(),
                fs::canonicalize(image).unwrap(),
                root.join("missing.svg"),
            ]
        );
        assert_eq!(fragment.assets.len(), 1);
    }

    #[test]
    fn canonical_sources_outside_the_project_root_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("outside.md");
        fs::write(&path, "Outside.\n").unwrap();
        let Err(error) = provider().synthesize(&path, directory.path()) else {
            panic!("expected outside-root source refusal");
        };
        assert!(error.to_string().contains("outside Typst project root"));
    }
}
