//! Portable Typst source export for Markdown and saved notebooks.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use comrak::nodes::NodeValue;
use comrak::{Arena, parse_document};

use super::{BuildOptions, Prepared, find_clash, input_files, write_file};
use crate::assets::registry::content_hash;
use crate::error::{Diagnostics, Error, Result};
use crate::ir::{Block, SourceKind};
use crate::markdown::directives::{colon_prepass, parse_directive_info};
use crate::markdown::frontmatter::split_front_matter;
use crate::markdown::include::{include_path, raw_directive_info};
use crate::notebook::nbformat::{self, CellType};
use crate::typst::emit::{self, AssetPaths, EmitOptions, IncludeRun};
use crate::typst::world::WriterWorld;

struct File {
    path: PathBuf,
    bytes: Vec<u8>,
}

/// Writes a resolved document and its physical include/resource tree.
///
/// Every source/resource is read and every destination checked before the first
/// write. The supplied compiler session is retained without being invoked.
pub fn build(
    prepared: &Prepared, _opts: &BuildOptions, out: &Path,
    _world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)> {
    if prepared.document.source.kind == SourceKind::Typst {
        return Err(Error::Other(
            "cannot export a .typ input to typst; use writer build part.md --to typst".into(),
        ));
    }
    let stem =
        out.file_stem().and_then(|name| name.to_str()).ok_or_else(|| {
            Error::Config(format!(
                "Typst export needs an output file with a UTF-8 stem: {}",
                out.display(),
            ))
        })?;
    let parent = out.parent().unwrap_or(Path::new("."));
    let directory = format!("{stem}_assets");
    let asset_paths = emit::export_asset_paths(
        &prepared.document, &prepared.assets, &directory,
    );
    let mut resources = Resources::new(parent, &directory, &asset_paths);
    let bibliography = prepared
        .config
        .references
        .bibliography
        .iter()
        .map(|path| resources.copy(path))
        .collect::<Result<Vec<_>>>()?;
    let style = if super::is_csl(&prepared.config.references.style) {
        resources.copy(Path::new(&prepared.config.references.style))?
    } else {
        prepared.config.references.style.clone()
    };
    let template = if prepared.config.pdf.template.as_os_str().is_empty() {
        None
    } else {
        Some(resources.copy(&prepared.config.pdf.template)?)
    };
    let first_level = first_level_includes(prepared)?;
    let runs = include_runs(&prepared.document.blocks, &first_level, stem, out);
    let options = EmitOptions {
        config: &prepared.config,
        assets: AssetPaths::Export { dir: directory.clone() },
        template,
        bibliography,
        style,
        footnote_prefix: String::new(),
        fragment: false,
    };
    let (main, parts) = emit::emit_split(
        &prepared.document, &prepared.assets, &options, &runs,
    )?;
    let mut diagnostics = main.diagnostics;
    let mut referenced = main.assets;
    let mut files =
        vec![File { path: out.to_path_buf(), bytes: main.text.into_bytes() }];
    for (run, part) in runs.iter().zip(parts) {
        diagnostics.extend(part.diagnostics);
        referenced.extend(part.assets);
        files.push(File {
            path: parent.join(&run.file),
            bytes: part.text.into_bytes(),
        });
    }
    let source_files = files.len();
    for (id, path) in referenced {
        files.push(File {
            path: parent.join(path),
            bytes: prepared.assets.read(&id)?,
        });
    }
    files.extend(resources.files);
    let assets = (files.len() > source_files).then(|| parent.join(directory));
    preflight(&files, assets.as_deref(), prepared)?;
    for file in &files {
        write_file(&file.path, &file.bytes)?;
    }
    let mut written: Vec<_> =
        files.into_iter().take(source_files).map(|file| file.path).collect();
    written.extend(assets);
    Ok((written, diagnostics))
}

struct Resources<'a> {
    parent: &'a Path,
    directory: &'a str,
    occupied: BTreeSet<String>,
    copied: BTreeMap<PathBuf, String>,
    files: Vec<File>,
}

impl<'a> Resources<'a> {
    fn new(
        parent: &'a Path, directory: &'a str,
        asset_paths: &BTreeMap<String, String>,
    ) -> Self {
        let occupied = asset_paths
            .values()
            .filter_map(|path| {
                Path::new(path)
                    .file_name()?
                    .to_str()
                    .map(str::to_ascii_lowercase)
            })
            .collect();
        Self {
            parent,
            directory,
            occupied,
            copied: BTreeMap::new(),
            files: Vec::new(),
        }
    }

    fn copy(&mut self, source: &Path) -> Result<String> {
        let identity = fs::canonicalize(source)
            .map_err(|error| super::io_error(error, "cannot read", source))?;
        if let Some(path) = self.copied.get(&identity) {
            return Ok(path.clone());
        }
        let bytes = fs::read(&identity)
            .map_err(|error| super::io_error(error, "cannot read", source))?;
        let hint = source.file_name().unwrap_or_default().to_string_lossy();
        let hash = content_hash(&bytes);
        let name = emit::export_name(&hint, &hash[..6], &mut self.occupied);
        let path = format!("{}/{name}", self.directory);
        self.files.push(File { path: self.parent.join(&path), bytes });
        self.copied.insert(identity, path.clone());
        Ok(path)
    }
}

fn first_level_includes(prepared: &Prepared) -> Result<BTreeSet<PathBuf>> {
    // Hand-built resolved documents without includes need no source-file read.
    if prepared.document.includes.is_empty() {
        return Ok(BTreeSet::new());
    }
    let source = &prepared.document.source;
    let text = fs::read_to_string(&source.path)
        .map_err(|error| super::io_error(error, "cannot read", &source.path))?;
    let written = match source.kind {
        SourceKind::Markdown => scan_directives(&text, prepared, false)?,
        SourceKind::Notebook => notebook_includes(&text, prepared)?,
        SourceKind::Typst => Vec::new(),
    };
    written
        .into_iter()
        .map(|written| {
            let path = source.base_dir.join(written);
            fs::canonicalize(&path)
                .map_err(|error| super::io_error(error, "cannot read", &path))
        })
        .collect()
}

fn scan_directives(
    text: &str, prepared: &Prepared, cell: bool,
) -> Result<Vec<String>> {
    let arena = Arena::new();
    let mut options =
        crate::markdown::comrak_options(&prepared.config.markdown);
    let (_, lines) = crate::markdown::recover::split_lines(text);
    let empty_front = lines.len() >= 2
        && lines[0].strip_prefix('\u{feff}').unwrap_or(lines[0]) == "---"
        && lines[1] == "---";
    if cell || empty_front {
        options.extension.front_matter_delimiter = None;
    }
    let (preprocessed, _) = colon_prepass(text, &options);
    let root = parse_document(&arena, &preprocessed, &options);
    let mut paths = Vec::new();
    for node in root.children() {
        let data = node.data();
        let NodeValue::BlockDirective(directive) = &data.value else {
            continue;
        };
        if parse_directive_info(&directive.info).name != "include" {
            continue;
        }
        let start = data.sourcepos.start;
        let info = lines
            .get(start.line.saturating_sub(1))
            .and_then(|line| raw_directive_info(line, start.column))
            .unwrap_or(&directive.info);
        let path = include_path(info).map_err(|message| Error::Parse {
            path: prepared.document.source.path.clone(),
            line: Some(start.line),
            message,
        })?;
        paths.push(path);
    }
    Ok(paths)
}

fn notebook_includes(text: &str, prepared: &Prepared) -> Result<Vec<String>> {
    let (notebook, _) =
        nbformat::parse_notebook(&prepared.document.source.path, text)?;
    let first = notebook
        .cells
        .iter()
        .position(|cell| cell.cell_type == CellType::Markdown);
    let mut paths = Vec::new();
    for (index, cell) in notebook.cells.iter().enumerate() {
        let raw_markdown = cell.cell_type == CellType::Raw
            && matches!(
                cell.raw_mimetype()
                    .unwrap_or("")
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str(),
                "markdown" | "text/markdown" | "text/x-markdown"
            );
        if cell.cell_type != CellType::Markdown && !raw_markdown {
            continue;
        }
        let removed = cell.tags().iter().any(|tag| {
            prepared.config.notebook.remove_cell_tags.iter().any(|configured| {
                tag.replace('_', "-") == configured.replace('_', "-")
            })
        });
        if removed {
            continue;
        }
        let original = &cell.source.0;
        let blanked;
        let text = if cell.cell_type == CellType::Markdown
            && let Some((yaml, rest, consumed)) = split_front_matter(original)
            && (Some(index) == first
                || crate::notebook::convert::later_metadata_header(yaml))
        {
            blanked = "\n".repeat(consumed) + rest;
            &blanked
        } else {
            original
        };
        paths.extend(scan_directives(text, prepared, true)?);
    }
    Ok(paths)
}

// §10 deliberately uses file spans, not include occurrence identities. Adjacent
// repeated files can coalesce; a nested file that is also first-level may split.
// Neither changes content order or the shared resolved reference identities.
fn include_runs(
    blocks: &[Block], first_level: &BTreeSet<PathBuf>, stem: &str, out: &Path,
) -> Vec<IncludeRun> {
    let mut ranges = Vec::new();
    let mut current: Option<(PathBuf, usize)> = None;
    for (index, block) in blocks.iter().enumerate() {
        let file =
            block.span().and_then(|span| span.file.as_ref()).map(|path| {
                fs::canonicalize(path).unwrap_or_else(|_| path.clone())
            });
        let new_owner =
            file.as_ref().filter(|path| first_level.contains(*path));
        let changes = new_owner.is_some_and(|path| {
            current.as_ref().is_none_or(|(owner, _)| owner != path)
        });
        if file.is_none() || changes {
            if let Some((owner, start)) = current.take() {
                ranges.push((owner, start, index));
            }
            if let Some(owner) = new_owner {
                current = Some((owner.clone(), index));
            }
        }
    }
    if let Some((owner, start)) = current {
        ranges.push((owner, start, blocks.len()));
    }
    let mut occupied = BTreeSet::new();
    if let Some(name) = out.file_name().and_then(|name| name.to_str()) {
        occupied.insert(name.to_ascii_lowercase());
    }
    ranges
        .into_iter()
        .map(|(path, start, end)| {
            let included =
                path.file_stem().unwrap_or_default().to_string_lossy();
            let original = emit::export_name(
                &format!("{stem}_{included}.typ"),
                "2",
                &mut BTreeSet::new(),
            );
            let base = original.strip_suffix(".typ").unwrap_or(&original);
            let mut name = original.clone();
            let mut suffix = 2;
            while !occupied.insert(name.to_ascii_lowercase()) {
                name = format!("{base}-{suffix}.typ");
                suffix += 1;
            }
            IncludeRun { start, end, file: name }
        })
        .collect()
}

fn preflight(
    files: &[File], assets: Option<&Path>, prepared: &Prepared,
) -> Result<()> {
    let outputs = files.iter().map(|file| file.path.as_path()).chain(assets);
    if let Some(clash) = find_clash(outputs, &input_files(prepared)) {
        return Err(Error::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            clash,
        )));
    }
    for (index, file) in files.iter().enumerate() {
        check_file(&file.path)?;
        for previous in &files[..index] {
            let resolved = super::path_after_creating_dirs(&file.path);
            let other = super::path_after_creating_dirs(&previous.path);
            if super::same_file(&file.path, &previous.path)
                || resolved.is_some() && resolved == other
            {
                return Err(Error::Config(format!(
                    "Typst export outputs {} and {} name the same file",
                    previous.path.display(),
                    file.path.display(),
                )));
            }
        }
        if let Some(parent) = file.path.parent() {
            check_directory(parent)?;
        }
    }
    if let Some(assets) = assets {
        check_directory(assets)?;
    }
    Ok(())
}

fn check_file(path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => Err(Error::Config(format!(
            "Typst export output {} is not a regular file",
            path.display(),
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A dangling link cannot be checked for planned-output aliases.
            if fs::symlink_metadata(path).is_ok() {
                return Err(Error::Config(format!(
                    "Typst export output {} is a dangling symlink",
                    path.display(),
                )));
            }
            Ok(())
        }
        Err(error) => Err(super::io_error(error, "cannot write", path)),
    }
}

fn check_directory(path: &Path) -> Result<()> {
    for directory in
        path.ancestors().filter(|path| !path.as_os_str().is_empty())
    {
        match fs::metadata(directory) {
            Ok(metadata) if metadata.is_dir() => return Ok(()),
            Ok(_) => {
                return Err(Error::Io(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    format!(
                        "cannot create directory {}: not a directory",
                        directory.display()
                    ),
                )));
            }
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    && fs::symlink_metadata(directory).is_err() => {}
            Err(error) => {
                return Err(super::io_error(
                    error, "cannot create directory", directory,
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::ir::{
        DocMeta, Document, Features, Inline, Resolved, SourceInfo, SourceSpan,
    };

    fn paragraph(file: Option<&Path>, line: usize) -> Block {
        Block::Paragraph {
            inlines: vec![Inline::Text { text: format!("line {line}") }],
            span: Some(SourceSpan {
                start_line: line,
                end_line: line,
                cell: None,
                file: file.map(Path::to_path_buf),
            }),
        }
    }

    fn prepared(path: &Path) -> Prepared {
        Prepared {
            document: Document {
                meta: DocMeta::default(),
                blocks: Vec::new(),
                features: Features::default(),
                source: SourceInfo {
                    path: path.into(),
                    kind: SourceKind::Markdown,
                    base_dir: path.parent().unwrap().into(),
                },
                assets: Vec::new(),
                includes: Vec::new(),
                resolved: Resolved::default(),
            },
            assets: Default::default(),
            config: Config::default(),
            config_path: None,
            diagnostics: Diagnostics::default(),
        }
    }

    #[test]
    fn nested_spans_stay_in_the_root_run_and_repeated_stems_are_numbered() {
        let directory = tempfile::tempdir().unwrap();
        let a = directory.path().join("first/part.md");
        let b = directory.path().join("second/part.md");
        let nested = directory.path().join("nested.md");
        let blocks = vec![
            paragraph(None, 1),
            paragraph(Some(&a), 1),
            paragraph(Some(&nested), 2),
            paragraph(None, 4),
            paragraph(Some(&b), 1),
            paragraph(Some(&a), 1),
        ];
        let first_level = [a, b].into_iter().collect();
        let runs = include_runs(
            &blocks,
            &first_level,
            "report.final",
            Path::new("report.final.typ"),
        );
        assert_eq!(
            runs,
            vec![
                IncludeRun {
                    start: 1,
                    end: 3,
                    file: "report.final_part.typ".into()
                },
                IncludeRun {
                    start: 4,
                    end: 5,
                    file: "report.final_part-2.typ".into()
                },
                IncludeRun {
                    start: 5,
                    end: 6,
                    file: "report.final_part-3.typ".into()
                },
            ]
        );
    }

    #[test]
    fn span_only_partition_can_split_nested_first_level_files_without_loss() {
        let directory = tempfile::tempdir().unwrap();
        let a = directory.path().join("a.md");
        let b = directory.path().join("b.md");
        let blocks = vec![
            paragraph(Some(&a), 1),
            paragraph(Some(&b), 1),
            paragraph(Some(&b), 1),
            paragraph(None, 4),
        ];
        let first_level = [a, b].into_iter().collect();
        let runs =
            include_runs(&blocks, &first_level, "out", Path::new("out.typ"));
        assert_eq!(
            runs,
            vec![
                IncludeRun { start: 0, end: 1, file: "out_a.typ".into() },
                IncludeRun { start: 1, end: 3, file: "out_b.typ".into() },
            ]
        );
        assert_eq!(
            runs.iter()
                .flat_map(|run| &blocks[run.start..run.end])
                .chain(&blocks[3..])
                .collect::<Vec<_>>(),
            blocks.iter().collect::<Vec<_>>(),
        );
    }

    #[test]
    fn direct_scan_preserves_raw_colons_and_ignores_fenced_nested_and_metadata()
    {
        let document = prepared(Path::new("/project/main.md"));
        let source = "---\ntitle: ':::include metadata.md'\n---\n\
                      :::include \"part: one.md\"\n:::\n\
                      ```text\n:::include fenced.md\n:::\n```\n\
                      > :::include quoted.md\n> :::\n";
        assert_eq!(
            scan_directives(source, &document, false).unwrap(),
            ["part: one.md"],
        );
    }

    #[test]
    fn empty_front_matter_scan_matches_frontend_with_bom_and_all_line_endings()
    {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("main.md");
        let included = directory.path().join("part.md");
        fs::write(&included, "Included content.\n").unwrap();
        let included = fs::canonicalize(included).unwrap();
        for bom in ["", "\u{feff}"] {
            for ending in ["\n", "\r\n", "\r"] {
                let text = format!(
                    "{bom}---{ending}---{ending}:::include part.md{ending}:::{ending}---{ending}After.{ending}",
                );
                fs::write(&source, &text).unwrap();
                let mut prepared = prepared(&source);
                let parsed = crate::markdown::parse(
                    &prepared.document.source, &text, &prepared.config,
                )
                .unwrap();
                prepared.document = parsed.document;
                assert_eq!(prepared.document.includes, vec![included.clone()]);
                assert_eq!(
                    first_level_includes(&prepared).unwrap(),
                    [included.clone()].into_iter().collect::<BTreeSet<_>>(),
                    "BOM={bom:?}, ending={ending:?}",
                );
            }
        }
    }

    #[test]
    fn notebook_scan_blanks_metadata_and_keeps_only_retained_markdown_sources()
    {
        let mut document = prepared(Path::new("/project/main.ipynb"));
        document.document.source.kind = SourceKind::Notebook;
        let notebook = serde_json::json!({
            "nbformat": 4, "nbformat_minor": 5, "metadata": {},
            "cells": [
                {
                    "cell_type": "markdown",
                    "source": "---\ntitle: |\n  :::include metadata.md\n---\n:::include real.md\n:::\n",
                    "metadata": {},
                },
                {
                    "cell_type": "raw", "source": ":::include raw.md\n:::\n",
                    "metadata": {"raw_mimetype": "Text/Markdown; charset=utf-8"},
                },
                {
                    "cell_type": "markdown", "source": ":::include removed.md\n:::\n",
                    "metadata": {"tags": ["remove_cell"]},
                },
                {
                    "cell_type": "code", "source": ":::include code.md\n:::\n",
                    "metadata": {}, "outputs": [], "execution_count": null,
                },
            ],
        });
        assert_eq!(
            notebook_includes(&notebook.to_string(), &document).unwrap(),
            ["real.md", "raw.md"],
        );
    }

    #[test]
    fn resources_share_portable_names_and_reuse_only_canonical_source_identity()
    {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first/template.typ");
        let second = directory.path().join("second/template.typ");
        for path in [&first, &second] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "same bytes").unwrap();
        }
        let asset_paths = [("id".into(), "report_assets/TEMPLATE.typ".into())]
            .into_iter()
            .collect();
        let mut resources =
            Resources::new(directory.path(), "report_assets", &asset_paths);
        let hash = content_hash(b"same bytes");
        let one = resources.copy(&first).unwrap();
        assert_eq!(one, format!("report_assets/template-{}.typ", &hash[..6]));
        assert_eq!(
            resources
                .copy(&first.parent().unwrap().join("./template.typ"))
                .unwrap(),
            one
        );
        assert_eq!(
            resources.copy(&second).unwrap(),
            format!("report_assets/template-{}-2.typ", &hash[..6]),
        );
        assert_eq!(resources.files.len(), 2);
    }

    #[test]
    fn a_late_sidecar_input_clash_preserves_every_existing_output() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("source.md");
        let main = directory.path().join("out.typ");
        fs::write(&input, "source bytes").unwrap();
        fs::write(&main, "previous good output").unwrap();
        let files = vec![
            File { path: main.clone(), bytes: b"replacement".to_vec() },
            File { path: input.clone(), bytes: b"unsafe sidecar".to_vec() },
        ];
        let error = preflight(&files, None, &prepared(&input)).unwrap_err();
        assert!(super::super::refused_clash(&error).is_some());
        assert_eq!(fs::read(main).unwrap(), b"previous good output");
        assert_eq!(fs::read(input).unwrap(), b"source bytes");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_assets_directory_cannot_alias_an_unwritten_main_output() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("source.md");
        fs::write(&input, "source").unwrap();
        let assets = directory.path().join("out_assets");
        std::os::unix::fs::symlink(directory.path(), &assets).unwrap();
        let main = directory.path().join("out.typ");
        let files = vec![
            File { path: main.clone(), bytes: Vec::new() },
            File { path: assets.join("out.typ"), bytes: Vec::new() },
        ];
        let error =
            preflight(&files, Some(&assets), &prepared(&input)).unwrap_err();
        assert!(error.to_string().contains("name the same file"), "{error}");
        assert!(!main.exists());
    }
}
