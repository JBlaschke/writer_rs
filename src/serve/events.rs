//! Per-view live-build events and located browser diagnostics (live-reload.md §2).
//!
//! The payloads are shared by SSE and `/__writer/state`. Error conversion,
//! source excerpts and relative paths are lifted from presenter_rs; Writer adds
//! independent web/pages status, notebook cells and compiler hints.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Diagnostic, Error, Level};
use crate::ir::SourceSpan;
use crate::pipeline::{BuildFailure, ChangeKind};

/// Whether either served view currently holds an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Neither view holds an error, including before the first attempt.
    Ok,
    /// At least one view holds an error.
    Error,
}

/// The result of one view's stage in a build attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewStatus {
    /// The stage succeeded and its new output is served.
    Ok,
    /// The stage failed; its last good output remains served.
    Error,
    /// This attempt did not run the stage.
    Skipped,
}

/// Status and optional error of the web view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WebView {
    /// This attempt's stage result, or the current result in `hello`.
    pub status: ViewStatus,
    /// The slot's current error, omitted when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
}

/// Status of the pages view and metadata of the page set still being served.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PagesView {
    /// This attempt's stage result, or the current result in `hello`.
    pub status: ViewStatus,
    /// The slot's current error, omitted when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
    /// Number of pages in the last good set, zero before any success.
    pub count: usize,
    /// Content hashes in page order.
    pub hashes: Vec<String>,
    /// Width and height in points, in page order.
    pub size_pt: Vec<[f64; 2]>,
}

/// The first changed source position, used to keep the reader near an edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Changed {
    /// Source path relative to the current directory when below it.
    pub file: String,
    /// Source index: zero for the main input, then the document's includes.
    pub source: usize,
    /// A changed 1-based text line; absent for a notebook cell change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// A changed 0-based notebook cell; absent for a text line change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<usize>,
}

/// One completed build attempt, broadcast as the `build` SSE event.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuildEvent {
    /// Attempt id, including failed attempts.
    pub build_id: u64,
    /// Whether either slot holds an error after this attempt.
    pub status: Status,
    /// What the changed paths affected.
    pub kind: ChangeKind,
    /// Wall time of the rebuild in milliseconds.
    pub duration_ms: u64,
    /// Changed source position, or null when no source position changed.
    pub changed: Option<Changed>,
    /// Web stage result and current error.
    pub web: WebView,
    /// Pages stage result, current error and served page metadata.
    pub pages: PagesView,
    /// Current diagnostics from preparation, web and pages, deduplicated.
    pub warnings: Vec<DiagnosticJson>,
}

/// Current state sent immediately to an SSE subscriber and by `/__writer/state`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hello {
    /// Latest attempt id, zero before the first build.
    pub build_id: u64,
    /// Whether either slot currently holds an error.
    pub status: Status,
    /// Web slot status, never `skipped`.
    pub web: WebView,
    /// Pages slot status, never `skipped`, with the served page metadata.
    pub pages: PagesView,
    /// Current stage diagnostics, deduplicated in stage order.
    pub warnings: Vec<DiagnosticJson>,
}

/// A located error as displayed by the browser's overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorPayload {
    /// Explanation prefixed with its source location when known.
    pub message: String,
    /// Source, template, stylesheet or resource path, or null when unknown.
    pub file: Option<String>,
    /// 1-based line in the file, or in `cell` when a cell is present.
    pub line: Option<usize>,
    /// 0-based notebook cell, or null for an ordinary file location.
    pub cell: Option<usize>,
    /// The library error variant in kebab-case.
    pub kind: String,
    /// Offending source line, omitted when it cannot be quoted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    /// Compiler suggestions, omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
}

impl From<&Error> for ErrorPayload {
    fn from(err: &Error) -> Self {
        let cwd = std::env::current_dir().ok();
        Self::new(err, cwd.as_deref(), None)
    }
}

impl ErrorPayload {
    /// Converts an error from `input`, showing paths below the current directory
    /// relatively. Input supplies the source of missing assets and front matter.
    pub fn for_input(err: &Error, input: &Path) -> Self {
        let cwd = std::env::current_dir().ok();
        Self::new(err, cwd.as_deref(), Some(input))
    }

    /// Converts a stopped build. A compile error takes its source location from
    /// the first error diagnostic, which can name an included notebook cell.
    pub fn for_failure(failure: &BuildFailure, input: &Path) -> Self {
        let err = failure.error.as_ref();
        let mut payload = Self::for_input(err, input);
        if !matches!(err, Error::Compile { .. }) {
            return payload;
        }
        let Some(diagnostic) = failure
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.level == Level::Error)
        else {
            return payload;
        };
        let span = diagnostic.span.as_ref();
        let path = diagnostic_path(diagnostic);
        let cwd = std::env::current_dir().ok();
        let (file, line, cell, message) =
            located(path, span, cwd.as_deref(), &err.message());
        payload.file = file;
        payload.line = line;
        payload.cell = cell;
        payload.message = message;
        payload.excerpt = source_excerpt(path, span, &err.message())
            .or_else(|| quoted_excerpt(err));
        payload
    }

    /// Converts `err`, shortening paths below `base` and using `input` for a
    /// source error that carries no file of its own. Excerpts are read now.
    pub fn new(err: &Error, base: Option<&Path>, input: Option<&Path>) -> Self {
        let span = err.span();
        let (file, line, cell, message) = match err {
            Error::Parse { .. } | Error::Compile { .. } => {
                located(err.path(), span.as_ref(), base, &err.message())
            }
            Error::MissingAsset { path, .. } => {
                let source = included_file(err).or(input);
                let message = format!(
                    "asset not found: {}",
                    display_relative(path, base),
                );
                located(source, span.as_ref(), base, &message)
            }
            Error::FrontMatter(_) => {
                let span = input.and(span.as_ref());
                located(input, span, base, &err.message())
            }
            Error::Template { template, line, .. } => {
                (Some(template.clone()), *line, None, err.to_string())
            }
            Error::Style { file, .. } => {
                (Some(file.clone()), None, None, err.to_string())
            }
            Error::Config(message) => match config_location(message, base) {
                Some((file, line, message)) => {
                    (Some(file), line, None, message)
                }
                None => (None, None, None, err.to_string()),
            },
            Error::Resource { what, path, message } => {
                let file = display_relative(path, base);
                let message = format!("{what} '{file}': {message}");
                (Some(file), None, None, message)
            }
            _ => (None, None, None, err.to_string()),
        };
        let hints = match err {
            Error::Compile { hints, .. } => hints.clone(),
            _ => Vec::new(),
        };
        Self {
            message,
            file,
            line,
            cell,
            kind: error_kind(err).to_string(),
            excerpt: excerpt(err, input),
            hints,
        }
    }
}

/// A non-fatal finding in the shared JSON diagnostic format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticJson {
    /// Lowercase severity: `info`, `warning` or `error`.
    pub level: &'static str,
    /// Stable diagnostic identifier.
    pub code: String,
    /// Explanation without a location prefix.
    pub message: String,
    /// Source path, preferring the span's included file, or null.
    pub file: Option<String>,
    /// 1-based start line, within the cell when one is present, or null.
    pub line: Option<usize>,
    /// 0-based notebook cell, or null.
    pub cell: Option<usize>,
}

impl DiagnosticJson {
    /// Converts a diagnostic, shortening source paths below `base`. The session
    /// filters informational findings before publishing warnings.
    pub fn new(diagnostic: &Diagnostic, base: Option<&Path>) -> Self {
        Self {
            level: match diagnostic.level {
                Level::Info => "info",
                Level::Warning => "warning",
                Level::Error => "error",
            },
            code: diagnostic.code.to_string(),
            message: diagnostic.message.clone(),
            file: diagnostic_path(diagnostic)
                .map(|path| display_relative(path, base)),
            line: diagnostic.span.as_ref().map(|span| span.start_line),
            cell: diagnostic.span.as_ref().and_then(|span| span.cell),
        }
    }
}

/// The diagnostic's precise included-file origin takes precedence over its path.
fn diagnostic_path(diagnostic: &Diagnostic) -> Option<&Path> {
    diagnostic
        .span
        .as_ref()
        .and_then(|span| span.file.as_deref())
        .or(diagnostic.path.as_deref())
}

/// A source location and its prefixed message, preserving lines within cells.
fn located(
    path: Option<&Path>, span: Option<&SourceSpan>, base: Option<&Path>,
    message: &str,
) -> (Option<String>, Option<usize>, Option<usize>, String) {
    let file = path.map(|path| display_relative(path, base));
    let line = span.map(|span| span.start_line);
    let cell = span.and_then(|span| span.cell);
    let message = match (&file, line, cell) {
        (Some(file), Some(line), Some(cell)) => {
            format!("{file} cell {cell}:{line}: {message}")
        }
        (Some(file), Some(line), None) => format!("{file}:{line}: {message}"),
        (Some(file), None, _) => format!("{file}: {message}"),
        (None, _, _) => message.to_string(),
    };
    (file, line, cell, message)
}

/// The file where a missing asset was referenced, when its span names one.
fn included_file(err: &Error) -> Option<&Path> {
    match err {
        Error::MissingAsset { span: Some(span), .. } => span.file.as_deref(),
        _ => None,
    }
}

/// The offending line, read from the source when possible, else already quoted.
fn excerpt(err: &Error, input: Option<&Path>) -> Option<String> {
    let file = match err {
        Error::MissingAsset { .. } | Error::FrontMatter(_) => {
            included_file(err).or(input)
        }
        _ => err.path(),
    };
    let read = |path: &Path| std::fs::read_to_string(path).ok();
    file.and_then(|file| err.excerpt(file, &read))
        .map(|excerpt| excerpt.text)
        .filter(|text| !text.trim().is_empty())
        .or_else(|| quoted_excerpt(err))
}

/// Applies the ordinary excerpt formatting to a compiler-mapped source span.
fn source_excerpt(
    path: Option<&Path>, span: Option<&SourceSpan>, message: &str,
) -> Option<String> {
    let path = path?;
    let span = span?;
    let message = match span.cell {
        Some(cell) => {
            format!("cell {cell}, line {}: {message}", span.start_line)
        }
        None => message.to_string(),
    };
    let err = Error::Parse {
        path: path.to_path_buf(),
        line: span.cell.is_none().then_some(span.start_line),
        message,
    };
    excerpt(&err, None)
}

/// A nonblank source line quoted in a library-rendered error message.
fn quoted_excerpt(err: &Error) -> Option<String> {
    err.quoted_line().map(|line| line.trim_end().to_string())
}

/// Configuration provenance retained from presenter's error conversion.
fn config_location(
    message: &str, base: Option<&Path>,
) -> Option<(String, Option<usize>, String)> {
    let at = message.find(".toml: ")? + ".toml".len();
    let (path, rest) = (&message[..at], &message[at + ": ".len()..]);
    if !Path::new(path).is_file() {
        return None;
    }
    let file = display_relative(Path::new(path), base);
    Some(match toml_parse_error(rest) {
        Some((line, text)) => {
            (file.clone(), Some(line), format!("{file}:{line}: {text}"))
        }
        None => (file.clone(), None, format!("{file}: {rest}")),
    })
}

/// TOML's explanation without its source gutter, which is shown as the excerpt.
fn toml_parse_error(text: &str) -> Option<(usize, String)> {
    let rest = text.strip_prefix("TOML parse error at line ")?;
    let digits = rest.split(|c: char| !c.is_ascii_digit()).next()?;
    let line = digits.parse().ok()?;
    let quoted = |line: &&str| {
        line.trim_start()
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start()
            .starts_with('|')
    };
    let explanation: Vec<&str> =
        text.lines().skip(1).filter(|line| !quoted(line)).collect();
    Some((line, explanation.join("\n").trim().to_string()))
}

/// Shortens paths strictly below base, resolving symlink spelling as a fallback.
fn display_relative(path: &Path, base: Option<&Path>) -> String {
    let below = |path: &Path, base: &Path| -> Option<PathBuf> {
        let relative = path.strip_prefix(base).ok()?;
        (!relative.as_os_str().is_empty()).then(|| relative.to_path_buf())
    };
    let relative = base.and_then(|base| {
        below(path, base).or_else(|| {
            below(&path.canonicalize().ok()?, &base.canonicalize().ok()?)
        })
    });
    relative
        .as_deref()
        .unwrap_or(path)
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn error_kind(err: &Error) -> &'static str {
    match err {
        Error::Parse { .. } => "parse",
        Error::FrontMatter(_) => "front-matter",
        Error::Config(_) => "config",
        Error::Theme { .. } => "theme",
        Error::Template { .. } => "template",
        Error::Style { .. } => "style",
        Error::MissingAsset { .. } => "missing-asset",
        Error::Compile { .. } => "compile",
        Error::Resource { .. } => "resource",
        Error::Io(_) => "io",
        Error::Json(_) => "json",
        Error::Other(_) => "other",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;
    use crate::error::Diagnostics;

    fn span(line: usize, cell: Option<usize>) -> SourceSpan {
        SourceSpan { start_line: line, end_line: line, cell, file: None }
    }

    fn compile(path: PathBuf, line: Option<usize>) -> Error {
        Error::Compile {
            path,
            line,
            message: "unknown variable: foo".into(),
            hints: vec!["check the name".into()],
        }
    }

    fn pages(status: ViewStatus) -> PagesView {
        PagesView {
            status,
            error: None,
            count: 2,
            hashes: vec!["9c1e0b7a44d2f310".into(), "7f30c2d9e1a05b68".into()],
            size_pt: vec![[595.28, 841.89], [595.28, 841.89]],
        }
    }

    #[test]
    fn successful_build_wire_shape_matches_the_spec() {
        let build = BuildEvent {
            build_id: 18,
            status: Status::Ok,
            kind: ChangeKind::Content,
            duration_ms: 84,
            changed: Some(Changed {
                file: "chapters/method.md".into(),
                source: 2,
                line: Some(41),
                cell: None,
            }),
            web: WebView { status: ViewStatus::Ok, error: None },
            pages: pages(ViewStatus::Ok),
            warnings: vec![DiagnosticJson::new(
                &Diagnostic::warning("unresolved-reference", "missing label")
                    .with_path("document.md")
                    .with_span(span(12, None)),
                None,
            )],
        };
        assert_eq!(
            serde_json::to_value(&build).unwrap(),
            json!({
                "build_id": 18, "status": "ok", "kind": "content", "duration_ms": 84,
                "changed": {"file": "chapters/method.md", "source": 2, "line": 41},
                "web": {"status": "ok"},
                "pages": {"status": "ok", "count": 2,
                    "hashes": ["9c1e0b7a44d2f310", "7f30c2d9e1a05b68"],
                    "size_pt": [[595.28, 841.89], [595.28, 841.89]]},
                "warnings": [{"level": "warning", "code": "unresolved-reference",
                    "message": "missing label", "file": "document.md", "line": 12,
                    "cell": null}]
            })
        );
        assert!(!serde_json::to_string(&build).unwrap().contains('\n'));
    }

    #[test]
    fn hello_and_failed_skipped_views_keep_the_served_page_metadata() {
        let payload = ErrorPayload::new(
            &compile("document.md".into(), Some(7)),
            None,
            None,
        );
        let web = WebView { status: ViewStatus::Ok, error: None };
        let mut pages = pages(ViewStatus::Error);
        pages.error = Some(payload.clone());
        let hello = Hello {
            build_id: 19,
            status: Status::Error,
            web: web.clone(),
            pages: pages.clone(),
            warnings: vec![],
        };
        let hello_json = serde_json::to_value(&hello).unwrap();
        assert_eq!(
            hello_json,
            json!({
                "build_id": 19, "status": "error", "web": {"status": "ok"},
                "pages": {"status": "error", "error": {
                    "message": "document.md:7: unknown variable: foo",
                    "file": "document.md", "line": 7, "cell": null,
                    "kind": "compile", "hints": ["check the name"]},
                    "count": 2, "hashes": ["9c1e0b7a44d2f310", "7f30c2d9e1a05b68"],
                    "size_pt": [[595.28, 841.89], [595.28, 841.89]]},
                "warnings": []
            })
        );
        for key in ["kind", "duration_ms", "changed"] {
            assert!(hello_json.get(key).is_none());
        }
        pages.status = ViewStatus::Skipped;
        let skipped = BuildEvent {
            build_id: 20,
            status: Status::Error,
            kind: ChangeKind::Css,
            duration_ms: 4,
            changed: None,
            web,
            pages,
            warnings: vec![],
        };
        let skipped = serde_json::to_value(skipped).unwrap();
        assert_eq!(skipped["changed"], json!(null));
        assert_eq!(skipped["pages"]["status"], json!("skipped"));
        assert_eq!(
            skipped["pages"]["error"],
            serde_json::to_value(payload).unwrap()
        );
        assert_eq!(skipped["pages"]["count"], json!(2));
    }

    #[test]
    fn changed_omits_the_other_position_and_empty_payload_fields() {
        assert_eq!(
            serde_json::to_value(Changed {
                file: "notebook.ipynb".into(),
                source: 0,
                line: None,
                cell: Some(2),
            })
            .unwrap(),
            json!({"file": "notebook.ipynb", "source": 0, "cell": 2})
        );
        let payload = ErrorPayload::from(&Error::Other("bad".into()));
        assert_eq!(
            serde_json::to_value(payload).unwrap(),
            json!({
                "message": "bad", "file": null, "line": null, "cell": null,
                "kind": "other"
            })
        );
    }

    #[test]
    fn parse_paths_are_relative_only_strictly_below_the_base() {
        let tmp = tempfile::tempdir().unwrap();
        let below = tmp.path().join("chapters/method.md");
        let make = |path, line| Error::Parse {
            path,
            line,
            message: "unbalanced quote".into(),
        };
        let payload =
            ErrorPayload::new(&make(below, Some(3)), Some(tmp.path()), None);
        assert_eq!(payload.file.as_deref(), Some("chapters/method.md"));
        assert_eq!(payload.message, "chapters/method.md:3: unbalanced quote");
        let same = ErrorPayload::new(
            &make(tmp.path().into(), None),
            Some(tmp.path()),
            None,
        );
        assert_eq!(same.file, Some(display_relative(tmp.path(), None)));
        let cwd = std::env::current_dir().unwrap();
        let from = ErrorPayload::from(&make(cwd.join("document.md"), Some(41)));
        assert_eq!(from.file.as_deref(), Some("document.md"));
        let bare = ErrorPayload::new(
            &make("elsewhere.md".into(), None),
            Some(tmp.path()),
            None,
        );
        assert_eq!(bare.message, "elsewhere.md: unbalanced quote");
    }

    #[test]
    fn parse_excerpts_quote_cells_and_ignore_blank_or_missing_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("document.md");
        fs::write(&source, "# A\n\n:::bad x=\"a\n").unwrap();
        let parse = Error::Parse {
            path: source.clone(),
            line: Some(3),
            message: "unbalanced quote".into(),
        };
        assert_eq!(
            ErrorPayload::from(&parse).excerpt.as_deref(),
            Some(":::bad x=\"a")
        );
        fs::write(&source, "# A\n\n   \n").unwrap();
        assert_eq!(ErrorPayload::from(&parse).excerpt, None);
        fs::remove_file(&source).unwrap();
        assert_eq!(ErrorPayload::from(&parse).excerpt, None);

        let notebook = tmp.path().join("notebook.ipynb");
        fs::write(&notebook, r##"{"cells":[{"cell_type":"markdown","source":["# T\n",":::bad x=\"a"]}]}"##).unwrap();
        let in_cell = Error::Parse {
            path: notebook,
            line: None,
            message: "cell 0, line 2: unbalanced quote".into(),
        };
        let payload = ErrorPayload::new(&in_cell, Some(tmp.path()), None);
        assert_eq!((payload.line, payload.cell), (Some(2), Some(0)));
        assert_eq!(
            payload.message,
            "notebook.ipynb cell 0:2: unbalanced quote"
        );
        assert_eq!(payload.excerpt.as_deref(), Some(":::bad x=\"a"));
    }

    #[test]
    fn source_errors_use_input_or_the_included_file() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("document.md");
        fs::write(&input, "---\ntitle: a: b\n---\n\n![x](img/x.png)\n")
            .unwrap();
        let front = Error::FrontMatter(
            "mapping values are not allowed in this context at line 2 column 9"
                .into(),
        );
        let payload = ErrorPayload::new(&front, Some(tmp.path()), Some(&input));
        assert_eq!(payload.file.as_deref(), Some("document.md"));
        assert_eq!((payload.line, payload.cell), (Some(2), None));
        assert_eq!(payload.excerpt.as_deref(), Some("title: a: b"));
        assert!(payload.message.starts_with("document.md:2: front matter:"));
        let bare = ErrorPayload::new(&front, Some(tmp.path()), None);
        assert_eq!(
            (bare.file, bare.line, bare.cell, bare.excerpt),
            (None, None, None, None)
        );

        let missing = Error::MissingAsset {
            path: tmp.path().join("img/x.png"),
            span: Some(span(5, None)),
        };
        let payload =
            ErrorPayload::new(&missing, Some(tmp.path()), Some(&input));
        assert_eq!(
            payload.message,
            "document.md:5: asset not found: img/x.png"
        );
        assert_eq!(payload.excerpt.as_deref(), Some("![x](img/x.png)"));
        let bare = ErrorPayload::new(&missing, Some(tmp.path()), None);
        assert_eq!(bare.message, "asset not found: img/x.png");
        assert_eq!((bare.file, bare.line), (None, Some(5)));

        let part = tmp.path().join("parts/p.md");
        fs::create_dir_all(part.parent().unwrap()).unwrap();
        fs::write(&part, "# P\n\n![y](y.png)\n").unwrap();
        let included = Error::MissingAsset {
            path: tmp.path().join("parts/y.png"),
            span: Some(SourceSpan { file: Some(part), ..span(3, None) }),
        };
        for input in [Some(input.as_path()), None] {
            let payload = ErrorPayload::new(&included, Some(tmp.path()), input);
            assert_eq!(payload.file.as_deref(), Some("parts/p.md"));
            assert_eq!(
                payload.message,
                "parts/p.md:3: asset not found: parts/y.png"
            );
            assert_eq!(payload.excerpt.as_deref(), Some("![y](y.png)"));
        }
        assert_eq!(
            ErrorPayload::for_input(&front, &input),
            ErrorPayload::new(
                &front,
                std::env::current_dir().ok().as_deref(),
                Some(&input),
            )
        );
    }

    #[test]
    fn notebook_front_matter_and_missing_assets_preserve_cell_locations() {
        let tmp = tempfile::tempdir().unwrap();
        let notebook = tmp.path().join("notebook.ipynb");
        fs::write(&notebook, r##"{"cells":[{"cell_type":"markdown","source":["---\n","title: a: b\n","---\n","![x](x.png)\n"]}]}"##).unwrap();
        let front = Error::FrontMatter(
            "cell 0: mapping values are not allowed in this context at line 2 column 9".into(),
        );
        let front =
            ErrorPayload::new(&front, Some(tmp.path()), Some(&notebook));
        assert_eq!((front.line, front.cell), (Some(2), Some(0)));
        assert!(
            front.message.starts_with("notebook.ipynb cell 0:2: front matter:")
        );
        assert!(!front.message.contains("cell 0: mapping"));
        assert_eq!(front.excerpt.as_deref(), Some("title: a: b"));
        let missing = Error::MissingAsset {
            path: tmp.path().join("x.png"),
            span: Some(span(4, Some(0))),
        };
        let missing =
            ErrorPayload::new(&missing, Some(tmp.path()), Some(&notebook));
        assert_eq!((missing.line, missing.cell), (Some(4), Some(0)));
        assert_eq!(missing.excerpt.as_deref(), Some("![x](x.png)"));
        assert_eq!(
            missing.message,
            "notebook.ipynb cell 0:4: asset not found: x.png"
        );
    }

    #[test]
    fn config_errors_use_toml_provenance_and_a_separate_excerpt() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("writer.toml");
        fs::write(&path, "[document]\nunknown = true\n").unwrap();
        let err = Error::Config(format!(
            "{}: TOML parse error at line 2, column 1\n  |\n2 | unknown = true\n  | ^^^^^^^\nunknown field `unknown`\n",
            path.display(),
        ));
        let payload = ErrorPayload::new(&err, Some(tmp.path()), None);
        assert_eq!(payload.file.as_deref(), Some("writer.toml"));
        assert_eq!(payload.line, Some(2));
        assert_eq!(payload.message, "writer.toml:2: unknown field `unknown`");
        assert_eq!(payload.excerpt.as_deref(), Some("unknown = true"));
        let err = Error::Config(format!("{}: invalid setting", path.display()));
        let payload = ErrorPayload::new(&err, Some(tmp.path()), None);
        assert_eq!(payload.message, "writer.toml: invalid setting");
        assert_eq!(payload.line, None);
        let err =
            Error::Config(format!("cannot read {}: denied", path.display()));
        let payload = ErrorPayload::new(&err, Some(tmp.path()), None);
        assert_eq!(payload.message, err.to_string());
        assert_eq!(payload.file, None);
    }

    #[test]
    fn templates_and_styles_keep_their_names_and_library_excerpts() {
        let mut env = minijinja::Environment::new();
        env.set_debug(true);
        let err =
            env.add_template("document.html", "<a>\n{{ x </a>\n").unwrap_err();
        let payload = ErrorPayload::from(&Error::from(err));
        assert_eq!(payload.file.as_deref(), Some("document.html"));
        assert_eq!(payload.line, Some(2));
        assert_eq!(payload.excerpt.as_deref(), Some("{{ x </a>"));
        let style = Error::Style {
            file: "styles/theme.css".into(),
            message: "bad".into(),
        };
        let payload = ErrorPayload::from(&style);
        assert_eq!(payload.file.as_deref(), Some("styles/theme.css"));
        assert_eq!((payload.line, payload.cell), (None, None));
        assert_eq!(payload.message, style.to_string());
    }

    #[test]
    fn compile_failure_uses_the_first_error_diagnostic_and_its_cell() {
        let tmp = tempfile::tempdir().unwrap();
        let notebook = tmp.path().join("included.ipynb");
        fs::write(&notebook, r##"{"cells":[{"cell_type":"markdown","source":["# T\n","#foo()\n"]}]}"##).unwrap();
        let diagnostics: Diagnostics = vec![
            Diagnostic::warning("warning", "ignore location")
                .with_path("warning.typ"),
            Diagnostic::error("compile", "unknown variable: foo")
                .with_path("generated.typ")
                .with_span(SourceSpan {
                    file: Some(notebook.clone()),
                    ..span(2, Some(0))
                }),
            Diagnostic::error("compile", "later error").with_path("later.typ"),
        ]
        .into();
        let failure = BuildFailure {
            error: Box::new(compile("generated.typ".into(), Some(300))),
            diagnostics,
        };
        let payload = ErrorPayload::for_failure(
            &failure,
            &tmp.path().join("document.md"),
        );
        let cwd = std::env::current_dir().unwrap();
        let file = display_relative(&notebook, Some(&cwd));
        assert_eq!(payload.file.as_deref(), Some(file.as_str()));
        assert_eq!((payload.line, payload.cell), (Some(2), Some(0)));
        assert_eq!(
            payload.message,
            format!("{file} cell 0:2: unknown variable: foo")
        );
        assert_eq!(payload.excerpt.as_deref(), Some("#foo()"));
        assert_eq!(payload.hints, ["check the name"]);
    }

    #[test]
    fn failure_fallback_and_noncompile_locations_remain_explicit() {
        let input = Path::new("document.md");
        let failure = BuildFailure {
            error: Box::new(compile("source.typ".into(), Some(8))),
            diagnostics: Diagnostics::default(),
        };
        assert_eq!(
            ErrorPayload::for_failure(&failure, input),
            ErrorPayload::for_input(&failure.error, input)
        );
        let failure = BuildFailure {
            error: Box::new(Error::Parse {
                path: input.into(),
                line: Some(2),
                message: "bad".into(),
            }),
            diagnostics: vec![
                Diagnostic::error("other", "other").with_path("other.md"),
            ]
            .into(),
        };
        assert_eq!(
            ErrorPayload::for_failure(&failure, input),
            ErrorPayload::for_input(&failure.error, input)
        );
        let failure = BuildFailure {
            error: Box::new(compile("source.typ".into(), Some(8))),
            diagnostics: vec![Diagnostic::error("compile", "bad")].into(),
        };
        let payload = ErrorPayload::for_failure(&failure, input);
        assert_eq!(
            (payload.file, payload.line, payload.cell),
            (None, None, None)
        );
        assert_eq!(payload.message, "unknown variable: foo");
    }

    #[test]
    fn diagnostics_serialize_all_six_fields_and_prefer_span_file() {
        let diagnostic = Diagnostic::warning("code", "message")
            .with_path("main.md")
            .with_span(SourceSpan {
                file: Some("parts/p.md".into()),
                ..span(7, Some(2))
            });
        assert_eq!(
            serde_json::to_value(DiagnosticJson::new(&diagnostic, None))
                .unwrap(),
            json!({
                "level": "warning", "code": "code", "message": "message",
                "file": "parts/p.md", "line": 7, "cell": 2
            })
        );
        for (level, name) in [
            (Level::Info, "info"),
            (Level::Warning, "warning"),
            (Level::Error, "error"),
        ] {
            let diagnostic = Diagnostic::new(level, "code", "message");
            assert_eq!(
                serde_json::to_value(DiagnosticJson::new(&diagnostic, None))
                    .unwrap(),
                json!({
                    "level": name, "code": "code", "message": "message",
                    "file": null, "line": null, "cell": null
                })
            );
        }
        let diagnostic = Diagnostic::warning("code", "message")
            .with_path("main.md")
            .with_span(span(3, None));
        let payload = DiagnosticJson::new(&diagnostic, None);
        assert_eq!(payload.file.as_deref(), Some("main.md"));
        assert_eq!(payload.line, Some(3));
    }

    #[cfg(unix)]
    #[test]
    fn canonical_paths_can_shorten_a_different_symlink_spelling() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        let link = tmp.path().join("link");
        fs::create_dir(&real).unwrap();
        fs::write(real.join("document.md"), "# T\n").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = Error::Parse {
            path: link.join("document.md"),
            line: Some(1),
            message: "bad".into(),
        };
        let payload = ErrorPayload::new(&err, Some(&real), None);
        assert_eq!(payload.file.as_deref(), Some("document.md"));
        assert_eq!(payload.excerpt.as_deref(), Some("# T"));
    }

    #[test]
    fn every_error_variant_has_a_kebab_case_kind() {
        let cases = [
            (
                Error::Parse {
                    path: "document.md".into(),
                    line: None,
                    message: "bad".into(),
                },
                "parse",
            ),
            (Error::FrontMatter("bad".into()), "front-matter"),
            (Error::Config("bad".into()), "config"),
            (
                Error::Theme { name: "theme".into(), message: "bad".into() },
                "theme",
            ),
            (
                Error::Template {
                    template: "document.html".into(),
                    line: None,
                    message: "bad".into(),
                },
                "template",
            ),
            (
                Error::Style {
                    file: "theme.css".into(),
                    message: "bad".into(),
                },
                "style",
            ),
            (
                Error::MissingAsset { path: "x.png".into(), span: None },
                "missing-asset",
            ),
            (compile("source.typ".into(), None), "compile"),
            (
                Error::Resource {
                    what: "template",
                    path: "template.typ".into(),
                    message: "bad".into(),
                },
                "resource",
            ),
            (Error::Io(std::io::Error::other("bad")), "io"),
            (
                Error::Json(
                    serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
                ),
                "json",
            ),
            (Error::Other("bad".into()), "other"),
        ];
        for (error, kind) in cases {
            assert_eq!(ErrorPayload::from(&error).kind, kind);
        }
    }
}
