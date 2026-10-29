//! Markdown include scanning and expansion (markdown-dialect.md §8).
//! Owner: T1B; the best-effort watcher scan is lifted by T16.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use comrak::nodes::NodeValue;
use comrak::{Arena, Options, parse_document};
use serde_json::Value;

use crate::assets::registry::normalize_lexically;
use crate::config::Config;
use crate::error::{Diagnostic, Diagnostics, Error, Result};
use crate::ir::{
    self, Block, DocMeta, Features, Inline, Parsed, SourceInfo, SourceKind,
    SourceSpan, detect_kind,
};

/// Maximum nesting depth for Markdown includes.
pub const MAX_DEPTH: usize = 8;

/// The opening directive info as written, before comrak unescapes punctuation.
/// `column` is comrak's 1-based byte column of the opening colon fence.
pub(crate) fn raw_directive_info(line: &str, column: usize) -> Option<&str> {
    let tail = line.get(column.checked_sub(1)?..)?;
    let fence = tail.bytes().take_while(|byte| *byte == b':').count();
    (fence >= 3).then(|| tail[fence..].trim())
}

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

/// A canonical ancestor chain and the current file's own metadata.
#[derive(Clone)]
pub(crate) struct Context {
    files: Vec<PathBuf>,
    root_dir: PathBuf,
    meta: DocMeta,
}

impl Context {
    pub(crate) fn root(source: &SourceInfo, meta: &DocMeta) -> Self {
        Self {
            files: vec![canonical(&source.path)],
            root_dir: canonical(&source.base_dir),
            meta: meta.clone(),
        }
    }

    /// Carries canonical ancestors; `chain` excludes `source` itself.
    pub(crate) fn in_chain(source: &SourceInfo, chain: &[PathBuf]) -> Self {
        let mut context = Self::root(source, &DocMeta::default());
        context.files = chain.iter().map(|path| canonical(path)).collect();
        context.files.push(canonical(&source.path));
        context.root_dir = context
            .files
            .first()
            .and_then(|path| path.parent())
            .unwrap_or(&source.base_dir)
            .to_path_buf();
        context
    }

    pub(crate) fn with_meta(&self, meta: &DocMeta) -> Self {
        Self { meta: meta.clone(), ..self.clone() }
    }

    fn enter(&self, path: PathBuf) -> Self {
        let mut next = self.clone();
        next.files.push(path);
        next
    }

    fn chain_to(&self, last: &Path) -> String {
        self.files
            .iter()
            .map(|path| shown(path, &self.root_dir))
            .chain(std::iter::once(shown(last, &self.root_dir)))
            .collect::<Vec<_>>()
            .join(" → ")
    }
}

/// Per-document assembly state, separate from the public fragment context.
pub(crate) struct State {
    source: SourceInfo,
    context: Context,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) features: Features,
    reported: Vec<Diagnostic>,
}

impl State {
    pub(crate) fn new(source: &SourceInfo, context: Context) -> Self {
        Self {
            source: source.clone(),
            context,
            files: Vec::new(),
            features: Features::default(),
            reported: Vec::new(),
        }
    }

    pub(crate) fn error(&self, span: &SourceSpan, message: String) -> Error {
        at_directive(&self.source, span, message)
    }

    pub(crate) fn expand(
        &mut self, path: &str, span: &SourceSpan, config: &Config,
    ) -> Result<Parsed> {
        let mut parsed =
            expand(path, span, &self.source, config, &self.context)?;
        self.features.merge(&parsed.document.features);
        for path in &parsed.document.includes {
            if !self.files.contains(path) {
                self.files.push(path.clone());
            }
        }
        let mut diagnostics = Diagnostics::new();
        for diagnostic in std::mem::take(&mut parsed.diagnostics) {
            if !self.reported.contains(&diagnostic) {
                self.reported.push(diagnostic.clone());
                diagnostics.push(diagnostic);
            }
        }
        parsed.diagnostics = diagnostics;
        Ok(parsed)
    }
}

/// Expansion/error/assembly helpers adapted from presenter_rs's include module.
fn expand(
    written: &str, span: &SourceSpan, source: &SourceInfo, config: &Config,
    context: &Context,
) -> Result<Parsed> {
    let at = |message| at_directive(source, span, message);
    let path = source.base_dir.join(written);
    let kind = detect_kind(&path).ok().filter(|kind| {
        matches!(kind, SourceKind::Markdown | SourceKind::Notebook)
    }).ok_or_else(|| at(format!(
        "cannot include `{written}`: only Markdown (.md, .markdown) and notebook (.ipynb) files can be included"
    )))?;
    let text = read_included(&path).map_err(|reason| {
        at(format!("cannot include `{written}`: {reason}"))
    })?;
    let path = canonical(&path);
    if context.files.contains(&path) {
        return Err(at(format!("include cycle: {}", context.chain_to(&path))));
    }
    if context.files.len() > MAX_DEPTH {
        return Err(at(format!(
            "includes nest more than {MAX_DEPTH} levels deep: {}",
            context.chain_to(&path),
        )));
    }
    let included = SourceInfo {
        path: path.clone(),
        base_dir: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
        kind,
    };
    let inner = context.enter(path.clone());
    let mut parsed = match kind {
        SourceKind::Markdown => {
            super::parse_included(&included, &text, config, &inner)
        }
        SourceKind::Notebook => {
            let mut includes = cell_includes_in(&included, &context.files);
            crate::notebook::parse_with(
                &included,
                &text,
                config,
                crate::notebook::ParseOptions {
                    includes: &mut *includes,
                    nested: true,
                    selection: crate::notebook::Selection::default(),
                },
            )
        }
        SourceKind::Typst => unreachable!("included kind checked above"),
    }
    .map_err(|error| in_included_file(error, &path))?;
    let keys = differing_meta(
        &parsed.document.meta,
        &context.meta,
        &explicit_keys(&text, kind),
    );
    tag_document(&mut parsed, &path);
    if !keys.is_empty() {
        parsed.diagnostics.push(Diagnostic::warning(
            "include-meta-ignored",
            format!("`{written}` sets {}; an included file's document metadata is ignored", and_list(&keys)),
        ).with_path(&source.path).with_span(span.clone()));
    }
    Ok(parsed)
}

fn at_directive(
    source: &SourceInfo, span: &SourceSpan, message: String,
) -> Error {
    let (line, message) = span.cell.map_or_else(
        || (Some(span.start_line), message.clone()),
        |cell| {
            (None, format!("cell {cell}, line {}: {message}", span.start_line))
        },
    );
    Error::Parse { path: source.path.clone(), line, message }
}

fn read_included(path: &Path) -> std::result::Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => "file not found (a relative path starts at the directory of the file with the directive)".to_owned(),
        _ => format!("cannot read it: {error}"),
    })?;
    String::from_utf8(bytes)
        .map_err(|error| format!("not valid UTF-8 ({error})"))
}

fn in_included_file(error: Error, path: &Path) -> Error {
    let span = error.span();
    let message = error.message().into_owned();
    match error {
        Error::FrontMatter(_) | Error::Config(_) => {
            let (line, message) = match span {
                Some(SourceSpan { cell: Some(cell), start_line, .. }) => {
                    (None, format!("cell {cell}, line {start_line}: {message}"))
                }
                Some(span) => (Some(span.start_line), message),
                None => (None, message),
            };
            Error::Parse { path: path.to_path_buf(), line, message }
        }
        Error::Parse { path: empty, line, message }
            if empty.as_os_str().is_empty() =>
        {
            Error::Parse { path: path.to_path_buf(), line, message }
        }
        other => other,
    }
}

pub(crate) fn tag_document(parsed: &mut Parsed, path: &Path) {
    ir::walk_blocks_mut(&mut parsed.document, &mut |block| {
        if let Some(span) = block_span_mut(block) {
            span.file.get_or_insert_with(|| path.to_path_buf());
        }
        tag_own_inlines(block, path);
    });
    let diagnostics = std::mem::take(&mut parsed.diagnostics);
    parsed.diagnostics = diagnostics
        .into_iter()
        .map(|mut diagnostic| {
            if diagnostic.path.as_deref().is_none_or(|own| own == path) {
                diagnostic.path = Some(path.to_path_buf());
                if let Some(span) = &mut diagnostic.span {
                    span.file.get_or_insert_with(|| path.to_path_buf());
                }
            }
            diagnostic
        })
        .collect();
    let mut includes = vec![path.to_path_buf()];
    for included in std::mem::take(&mut parsed.document.includes) {
        if !includes.contains(&included) {
            includes.push(included);
        }
    }
    parsed.document.includes = includes;
}

fn block_span_mut(block: &mut Block) -> &mut Option<SourceSpan> {
    match block {
        Block::Cell(cell) => &mut cell.span,
        Block::Heading { span, .. }
        | Block::Paragraph { span, .. }
        | Block::BlockQuote { span, .. }
        | Block::List { span, .. }
        | Block::DefinitionList { span, .. }
        | Block::CodeBlock { span, .. }
        | Block::Figure { span, .. }
        | Block::Table { span, .. }
        | Block::Math { span, .. }
        | Block::Callout { span, .. }
        | Block::Div { span, .. }
        | Block::Raw { span, .. }
        | Block::ThematicBreak { span }
        | Block::PageBreak { span }
        | Block::Toc { span, .. }
        | Block::Bibliography { span } => span,
    }
}

fn tag_own_inlines(block: &mut Block, path: &Path) {
    let tag = |inlines: &mut Vec<Inline>| tag_inlines(inlines, path);
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph { inlines, .. } => {
            tag(inlines)
        }
        Block::Figure { caption: Some(inlines), .. }
        | Block::Callout { title: Some(inlines), .. } => tag(inlines),
        Block::DefinitionList { items, .. } => {
            for item in items {
                tag(&mut item.term);
            }
        }
        Block::Table { head, body, foot, .. } => {
            for row in head.iter_mut().chain(body).chain(foot) {
                for cell in &mut row.cells {
                    tag(&mut cell.inlines);
                }
            }
        }
        _ => {}
    }
}

fn tag_inlines(inlines: &mut [Inline], path: &Path) {
    for inline in inlines {
        match inline {
            Inline::Ref { span: Some(span), supplement, .. } => {
                span.file.get_or_insert_with(|| path.to_path_buf());
                if let Some(inlines) = supplement {
                    tag_inlines(inlines, path);
                }
            }
            Inline::Cite { span, items, .. } => {
                if let Some(span) = span {
                    span.file.get_or_insert_with(|| path.to_path_buf());
                }
                for item in items {
                    if let Some(inlines) = &mut item.prefix {
                        tag_inlines(inlines, path);
                    }
                    if let Some(inlines) = &mut item.suffix {
                        tag_inlines(inlines, path);
                    }
                }
            }
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Link { inlines, .. }
            | Inline::Span { inlines, .. } => tag_inlines(inlines, path),
            Inline::Image { alt, .. } => tag_inlines(alt, path),
            _ => {}
        }
    }
}

const CONFIG_KEYS: &[&str] = &[
    "markdown", "math", "numbering", "references", "html", "pdf", "docx",
    "latex", "typst", "notebook", "toc", "bibliography", "cite_style", "csl",
    "cite_locale", "theme", "template", "reference_doc", "latex_template",
    "paper",
];

/// Explicit fields, including empty lists, from source already loaded for parsing.
pub(crate) fn explicit_keys(text: &str, kind: SourceKind) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    if kind == SourceKind::Markdown {
        front_keys(text, &mut keys);
        return keys;
    }
    let Ok(value) =
        serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}'))
    else {
        return keys;
    };
    let metadata = &value["metadata"];
    if metadata["title"].is_string() {
        keys.insert("title".into());
    }
    if metadata["authors"].is_array() {
        keys.insert("authors".into());
    }
    for (layer, shared_only) in [("presenter", true), ("writer", false)] {
        if let Some(map) = metadata[layer].as_object() {
            for (key, value) in map {
                if !shared_only
                    || [
                        "title", "subtitle", "author", "authors", "date",
                        "lang", "description", "keywords",
                    ]
                    .contains(&key.as_str())
                {
                    record_key(key, value, &mut keys);
                }
            }
        }
    }
    if let Some(cells) = value["cells"].as_array()
        && let Some(cell) =
            cells.iter().find(|cell| cell["cell_type"] == "markdown")
        && let Some(text) =
            crate::notebook::nbformat::mime_text(&cell["source"])
    {
        front_keys(&text, &mut keys);
    }
    keys
}

fn front_keys(text: &str, keys: &mut BTreeSet<String>) {
    if let Some((yaml, _, _)) = super::frontmatter::split_front_matter(text)
        && let Ok(Value::Object(map)) = serde_norway::from_str(yaml)
    {
        for (key, value) in &map {
            record_key(key, value, keys);
        }
    }
}

fn record_key(key: &str, value: &Value, keys: &mut BTreeSet<String>) {
    if !value.is_null() && !CONFIG_KEYS.contains(&key) {
        keys.insert(if key == "author" { "authors" } else { key }.into());
    }
}

pub(crate) fn differing_meta(
    included: &DocMeta, parent: &DocMeta, keys: &BTreeSet<String>,
) -> Vec<String> {
    keys.iter()
        .filter(|key| {
            metadata_value(included, key) != metadata_value(parent, key)
        })
        .map(|key| format!("`{key}`"))
        .collect()
}

fn metadata_value(meta: &DocMeta, key: &str) -> Value {
    let value = match key {
        "title" => serde_json::to_value(&meta.title),
        "subtitle" => serde_json::to_value(&meta.subtitle),
        "authors" => serde_json::to_value(&meta.authors),
        "date" => serde_json::to_value(&meta.date),
        "description" => serde_json::to_value(&meta.description),
        "lang" => serde_json::to_value(&meta.lang),
        "keywords" => serde_json::to_value(&meta.keywords),
        "abstract" => serde_json::to_value(&meta.abstract_),
        _ => return meta.vars.get(key).cloned().unwrap_or(Value::Null),
    };
    let mut value = value.unwrap_or(Value::Null);
    if key == "abstract" {
        strip_spans(&mut value);
    }
    value
}

fn strip_spans(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("span");
            for value in map.values_mut() {
                strip_spans(value);
            }
        }
        Value::Array(items) => {
            for value in items {
                strip_spans(value);
            }
        }
        _ => {}
    }
}

fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn shown(path: &Path, dir: &Path) -> String {
    path.strip_prefix(dir).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        std::path::absolute(path)
            .map(|path| normalize_lexically(&path))
            .unwrap_or_else(|_| normalize_lexically(path))
    })
}

/// Finds transitive includes for the watcher, even when a document fails to parse.
/// Notebook Markdown/raw-Markdown cells are scanned; code and output are ignored.
/// Markdown roots are scanned whatever their extension (`--format md`).
pub fn scan_includes(path: &Path) -> Vec<PathBuf> {
    let Ok(path) = std::path::absolute(path) else {
        return Vec::new();
    };
    let (path, identity) = watcher_path(&path);
    let mut found = Vec::new();
    scan_file(&path, 0, &mut vec![identity], &mut found);
    found
}

fn watcher_path(path: &Path) -> (PathBuf, PathBuf) {
    let identity = canonical(path);
    let normalized = normalize_lexically(path);
    // Removing `..` before following directory symlinks can select another file.
    let target = if canonical(&normalized) == identity {
        normalized
    } else {
        identity.clone()
    };
    (target, identity)
}

fn scan_file(
    path: &Path, depth: usize, seen: &mut Vec<PathBuf>,
    found: &mut Vec<PathBuf>,
) {
    // The dependency at the limit is already listed; do not read it further.
    if depth >= MAX_DEPTH {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let kind = detect_kind(path).ok();
    let fragments = if kind == Some(SourceKind::Notebook) {
        match crate::notebook::nbformat::parse_notebook(path, &text) {
            Ok((notebook, _)) => notebook
                .cells
                .iter()
                .filter(|cell| {
                    cell.cell_type
                        == crate::notebook::nbformat::CellType::Markdown
                        || cell.cell_type
                            == crate::notebook::nbformat::CellType::Raw
                            && matches!(
                                cell.raw_mimetype()
                                    .unwrap_or("")
                                    .split(';')
                                    .next()
                                    .unwrap_or("")
                                    .trim()
                                    .to_ascii_lowercase()
                                    .as_str(),
                                "markdown"
                                    | "text/markdown"
                                    | "text/x-markdown"
                            )
                })
                .map(|cell| (cell.source.0.clone(), true))
                .collect::<Vec<_>>(),
            Err(_) if depth == 0 => vec![(text, false)],
            Err(_) => return,
        }
    } else if depth == 0 || kind == Some(SourceKind::Markdown) {
        vec![(text, false)]
    } else {
        return;
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    for (text, cell) in fragments {
        for written in scan_targets(&text, cell) {
            let (target, identity) = watcher_path(&dir.join(written));
            if seen.contains(&identity) {
                continue;
            }
            seen.push(identity);
            found.push(target.clone());
            scan_file(&target, depth + 1, seen, found);
        }
    }
}

fn scan_targets(text: &str, cell: bool) -> Vec<String> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.block_directive = true;
    options.extension.front_matter_delimiter = (!cell).then(|| "---".into());
    let (_, lines) = super::recover::split_lines(text);
    let (prepared, _) = super::directives::colon_prepass(text, &options);
    let root = parse_document(&arena, &prepared, &options);
    root.descendants()
        .filter_map(|node| {
            let data = node.data();
            let NodeValue::BlockDirective(directive) = &data.value else {
                return None;
            };
            if super::directives::parse_directive_info(&directive.info).name
                != "include"
            {
                return None;
            }
            let start = data.sourcepos.start;
            let info = lines
                .get(start.line - 1)
                .and_then(|line| raw_directive_info(line, start.column))
                .unwrap_or(&directive.info);
            include_path(info).ok()
        })
        .collect()
}

/// Creates the notebook include hook with no enclosing files.
pub fn cell_includes(
    notebook: &SourceInfo,
) -> Box<dyn crate::notebook::CellIncludes> {
    cell_includes_in(notebook, &[])
}

/// Creates a notebook include hook with canonical ancestors, excluding this notebook.
pub fn cell_includes_in(
    notebook: &SourceInfo, chain: &[PathBuf],
) -> Box<dyn crate::notebook::CellIncludes> {
    let context = Context::in_chain(notebook, chain);
    Box::new(NotebookIncludes { state: State::new(notebook, context) })
}

struct NotebookIncludes {
    state: State,
}

impl crate::notebook::CellIncludes for NotebookIncludes {
    fn set_parent_meta(&mut self, meta: &DocMeta) {
        self.state.context.meta = meta.clone();
    }

    fn expand(
        &mut self, path: &str, span: &SourceSpan, _: &SourceInfo,
        config: &Config,
    ) -> Result<Parsed> {
        self.state.expand(path, span, config)
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

    #[test]
    fn raw_directive_info_keeps_windows_path_bytes() {
        let path =
            r"\\?\C:\Users\runneradmin\AppData\Local\Temp\.tmp123\part.md";
        let info = format!("include \"{path}\"");
        for (prefix, column) in
            [("", 1), ("   ", 4), ("> ", 3), ("- ", 3), ("\u{feff}", 4)]
        {
            let line = format!("{prefix}::::{info}");
            assert_eq!(raw_directive_info(&line, column), Some(info.as_str()));
            assert_eq!(
                include_path(raw_directive_info(&line, column).unwrap())
                    .unwrap(),
                path
            );
        }
        assert!(raw_directive_info("::include file.md", 1).is_none());
        assert!(raw_directive_info(":::include file.md", 0).is_none());
    }

    #[test]
    fn markdown_and_watcher_keep_raw_include_paths() {
        for (bom, path) in [
            ("", r"\\?\C:\writer-ci-path-byte-regression\.tmp123\part.md"),
            ("\u{feff}", r"folder\.tmp123\part.md"),
        ] {
            let text = format!("{bom}:::include \"{path}\"\n:::\n");
            for cell in [false, true] {
                assert_eq!(
                    scan_targets(&text, cell),
                    [path],
                    "BOM {bom:?}, cell {cell}"
                );
            }
            let source = SourceInfo {
                path: "main.md".into(),
                base_dir: ".".into(),
                kind: SourceKind::Markdown,
            };
            let Error::Parse { message, .. } =
                super::super::parse(&source, &text, &Config::default())
                    .err()
                    .unwrap()
            else {
                panic!("the deliberately absent file must fail")
            };
            assert!(message.contains(&format!("`{path}`")), "{message}");
        }
    }
}
