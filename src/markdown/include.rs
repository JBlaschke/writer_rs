//! Markdown include scanning and expansion (markdown-dialect.md §8).
//! Owner: T1B; the best-effort watcher scan is lifted by T16.

use std::path::{Path, PathBuf};

use comrak::nodes::NodeValue;
use comrak::{Arena, Options, parse_document};

use crate::assets::registry::normalize_lexically;
use crate::ir::{SourceKind, detect_kind};

/// Maximum nesting depth for Markdown includes.
pub const MAX_DEPTH: usize = 8;

/// Reads the local path from `include PATH`, including single/double quotes.
pub(crate) fn include_path(info: &str) -> std::result::Result<String, String> {
    let info = info.trim();
    if info.starts_with('{') {
        return Err(format!(
            "write `:::include PATH`: the attribute form `::: {info}` is not read for includes"
        ));
    }
    let rest = info.strip_prefix("include").unwrap_or(info).trim();
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''));
    let (path, after) = match quote {
        Some(quote) => {
            let inner = &rest[1..];
            let Some(end) = inner.find(quote) else {
                return Err(format!(
                    "unbalanced {quote} quote in `:::include {rest}`"
                ));
            };
            (&inner[..end], inner[end + 1..].trim())
        }
        None => match rest.split_once(char::is_whitespace) {
            Some((path, after)) => (path, after.trim()),
            None => (rest, ""),
        },
    };
    if path.is_empty() {
        return Err(
            "`:::include` needs the path of a Markdown (.md) or notebook (.ipynb) \
             file: `:::include part2.md`".into(),
        );
    }
    if !after.is_empty() {
        return Err(format!(
            "unexpected `{after}` after the path of `:::include`: quote a path that has \
             spaces (`:::include \"my talk/part 2.md\"`)"
        ));
    }
    if path.contains("://") {
        return Err(format!(
            "cannot include `{path}`: only local files can be included, not URLs"
        ));
    }
    Ok(path.to_string())
}

/// Finds includes transitively, in first-use order, without parsing a document.
///
/// The main file is read as Markdown whatever its extension (`--format md`).
/// Included Markdown files are scanned to [`MAX_DEPTH`], each path once; missing
/// files and other input kinds are listed too. Invalid paths and unreadable
/// files are ignored so a watcher can recover when a document does not parse.
pub fn scan_includes(path: &Path) -> Vec<PathBuf> {
    let Ok(path) =
        std::path::absolute(path).map(|path| normalize_lexically(&path))
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    scan_file(&path, 1, &mut vec![path.clone()], &mut found);
    found
}

/// Scans one file; `seen` contains every path already scanned or listed.
fn scan_file(
    path: &Path, depth: usize, seen: &mut Vec<PathBuf>,
    found: &mut Vec<PathBuf>,
) {
    if depth > MAX_DEPTH
        || (depth > 1 && detect_kind(path).ok() != Some(SourceKind::Markdown))
    {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let dir = path.parent().unwrap_or(Path::new("/"));
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.block_directive = true;
    options.extension.front_matter_delimiter = Some("---".into());
    let root = parse_document(&arena, &text, &options);
    let targets: Vec<PathBuf> = root
        .descendants()
        .filter_map(|node| match &node.data().value {
            NodeValue::BlockDirective(d)
                if super::directives::parse_directive_info(&d.info).name
                    == "include" =>
            {
                include_path(&d.info).ok()
            }
            _ => None,
        })
        .map(|written| normalize_lexically(&dir.join(written)))
        .collect();
    for target in targets {
        if seen.contains(&target) {
            continue;
        }
        seen.push(target.clone());
        found.push(target.clone());
        scan_file(&target, depth + 1, seen, found);
    }
}

/// Creates the notebook include hook; expansion arrives with T1B.
pub fn cell_includes(
    notebook: &crate::ir::SourceInfo,
) -> Box<dyn crate::notebook::CellIncludes> {
    cell_includes_in(notebook, &[])
}

/// Creates a notebook include hook with its enclosing canonical include chain.
pub fn cell_includes_in(
    notebook: &crate::ir::SourceInfo, chain: &[PathBuf],
) -> Box<dyn crate::notebook::CellIncludes> {
    let _ = chain;
    Box::new(NotebookIncludes { path: notebook.path.clone() })
}

struct NotebookIncludes {
    path: PathBuf,
}

impl crate::notebook::CellIncludes for NotebookIncludes {
    fn expand(
        &mut self, _: &str, span: &crate::ir::SourceSpan,
        _: &crate::ir::SourceInfo, _: &crate::config::Config,
    ) -> crate::error::Result<crate::ir::Parsed> {
        let cell = span.cell.unwrap_or(0);
        Err(crate::error::Error::Parse {
            path: self.path.clone(),
            line: Some(span.start_line),
            message: format!(
                "cell {cell}: :::include in notebooks arrives with T1B"
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_paths_are_read_like_the_dialect_says() {
        for (info, path) in [
            ("include b.md", "b.md"),
            ("include   ../x/b.ipynb  ", "../x/b.ipynb"),
            ("include \"a b/c d.md\"", "a b/c d.md"),
            ("include \"it's.md\"", "it's.md"),
            ("include 'say \"hi\".md'", "say \"hi\".md"),
            ("include /abs/document.md", "/abs/document.md"),
        ] {
            assert_eq!(include_path(info).as_deref(), Ok(path), "{info}");
        }
        for info in [
            "include",
            "include \"\"",
            "include 'a.md' b",
            "include \"a.md",
            "include https://example.org/a.md",
            "{.include}",
        ] {
            assert!(include_path(info).is_err(), "{info}");
        }
    }
}
