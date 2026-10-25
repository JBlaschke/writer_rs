//! Embedded Typst compilation and diagnostics (typst-dialect.md §8.1–§8.3).
//! Owner: T03. Stub created by T00.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};
use typst::diag::{Severity, SourceDiagnostic};
use typst::foundations::{Datetime, Label, Smart, Value};
use typst::introspection::{Introspector, MetadataElem};
use typst::syntax::{DiagSpan, FileId, VirtualRoot};
use typst::utils::PicoStr;
use typst::{World, WorldExt};
use typst_layout::PagedDocument;
use typst_pdf::Timestamp;

use crate::config::PdfConfig;
use crate::error::{Diagnostic, Diagnostics, Error, Level, Result};
use crate::ir::{DocMeta, SourceSpan};

use super::world::WriterWorld;

const MAIN_PATH: &str = "/__writer-main.typ";
const HINT_PREFIX: &str = "\n  hint: ";
const DOWNLOAD_HINT: &str = "writer downloads @preview packages only with \
    `[pdf] allow_packages = true` or `--allow-packages`";

/// A compiler output together with all warnings and errors it produced.
#[derive(Debug)]
pub struct Compiled<T> {
    /// The output when compilation succeeded.
    pub output: Option<T>,
    /// Findings in compiler order, with source locations mapped for the user.
    pub diagnostics: Diagnostics,
}

impl<T> Compiled<T> {
    /// Returns the output and warnings, or converts the first error into a
    /// [`Error::Compile`] with its location and hints.
    pub fn into_result(self) -> Result<(T, Diagnostics)> {
        if let Some(first) =
            self.diagnostics.iter().find(|d| d.level == Level::Error)
        {
            let mut parts = first.message.split(HINT_PREFIX);
            return Err(Error::Compile {
                path: first.path.clone().unwrap_or_else(|| MAIN_PATH.into()),
                line: first.span.as_ref().map(|span| span.start_line),
                message: parts.next().unwrap_or_default().to_owned(),
                hints: parts.map(str::to_owned).collect(),
            });
        }
        let output = self.output.ok_or_else(|| Error::Compile {
            path: MAIN_PATH.into(),
            line: None,
            message: "Typst compilation produced no output".into(),
            hints: Vec::new(),
        })?;
        Ok((output, self.diagnostics))
    }
}

/// Stable metadata used when exporting a PDF.
#[derive(Debug, Clone)]
pub struct PdfOptions {
    /// Creation time in UTC, computed once for the publishing session.
    pub timestamp: Datetime,
    /// A stable document identifier, or `None` for Typst's automatic identifier.
    pub ident: Option<String>,
}

/// A complete PDF document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pdf {
    /// PDF file bytes.
    pub bytes: Vec<u8>,
    /// Number of pages in the document.
    pub pages: usize,
}

/// Numbering metadata emitted by `@writer/doc` for one labelled element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleEntry {
    /// Element kind, such as `heading`, `figure`, or `equation`.
    pub kind: String,
    /// Displayed number, or `None` for an unnumbered element.
    pub number: Option<String>,
}

/// Compiles the world's current main source to a paged document.
///
/// The caller resets the world before a rebuild, so multiple exports can share
/// retained sources and the same set of accessed files.
pub fn compile_document(world: &mut WriterWorld) -> Compiled<PagedDocument> {
    let compiled = typst::compile::<PagedDocument>(&*world);
    let mut diagnostics: Diagnostics = compiled
        .warnings
        .iter()
        .map(|diag| map_diagnostic(world, diag))
        .collect();
    let output = match compiled.output {
        Ok(document) => Some(document),
        Err(errors) => {
            diagnostics
                .extend(errors.iter().map(|diag| map_diagnostic(world, diag)));
            None
        }
    };
    diagnostics.extend(world.include_diagnostics());
    Compiled { output, diagnostics }
}

/// Compiles the world's current main source and exports a tagged PDF.
pub fn compile_pdf(
    world: &mut WriterWorld, opts: &PdfOptions,
) -> Compiled<Pdf> {
    let Compiled { output, mut diagnostics } = compile_document(world);
    let output = output.and_then(|document| {
        let options = typst_pdf::PdfOptions {
            ident: opts.ident.clone().map_or(Smart::Auto, Smart::Custom),
            creator: Smart::Custom(Some(format!(
                "writer {}",
                env!("CARGO_PKG_VERSION")
            ))),
            timestamp: Some(Timestamp::new_utc(opts.timestamp)),
            page_ranges: None,
            standards: Default::default(),
            tagged: true,
            pretty: false,
        };
        match typst_pdf::pdf(&document, &options) {
            Ok(bytes) => Some(Pdf { bytes, pages: document.pages().len() }),
            Err(errors) => {
                diagnostics.extend(
                    errors.iter().map(|diag| map_diagnostic(world, diag)),
                );
                None
            }
        }
    });
    Compiled { output, diagnostics }
}

/// Reads `writer-oracle-{label}` metadata from a compiled document.
///
/// Returns `None` when the label is absent or its metadata has the wrong shape.
pub fn oracle(document: &PagedDocument, label: &str) -> Option<OracleEntry> {
    let key = Label::new(PicoStr::intern(&format!("writer-oracle-{label}")))?;
    let content = document.introspector().query_label(key).ok()?;
    let meta = content.to_packed::<MetadataElem>()?;
    let Value::Dict(dict) = &meta.value else {
        return None;
    };
    let string = |value: &Value| match value {
        Value::Str(value) => Some(value.as_str().to_owned()),
        _ => None,
    };
    Some(OracleEntry {
        kind: string(dict.get("kind").ok()?)?,
        number: string(dict.get("number").ok()?),
    })
}

/// Maps a Typst finding to the user's source, including package call traces.
pub fn map_diagnostic(
    world: &WriterWorld, diag: &SourceDiagnostic,
) -> Diagnostic {
    let (level, code) = match diag.severity {
        Severity::Error => (Level::Error, "typst-error"),
        Severity::Warning => (Level::Warning, "typst-warning"),
    };
    let direct = location(world, diag.span);
    let project = direct.filter(|(id, _)| is_project(*id)).or_else(|| {
        diag.trace.iter().rev().find_map(|point| {
            location(world, point.span.into()).filter(|(id, _)| is_project(*id))
        })
    });
    let selected = project.or(direct);
    let (path, span) = selected
        .map(|(id, line)| mapped_location(world, id, line))
        .unwrap_or((None, None));
    let mut message = diag.message.to_string();
    for hint in &diag.hints {
        append_hint(&mut message, &hint.v);
    }
    if let Some((id, line)) = direct.filter(|(id, _)| !is_project(*id))
        && project.is_some()
    {
        append_hint(
            &mut message,
            &format!("in {}:{line}", package_path(id).display()),
        );
    }
    if !world.allows_packages()
        && diag.message.contains("package not found")
        && diag.message.contains("@preview/")
    {
        append_hint(&mut message, DOWNLOAD_HINT);
    }
    Diagnostic { level, code, message, path, span }
}

fn location(world: &WriterWorld, span: DiagSpan) -> Option<(FileId, usize)> {
    let id = span.id()?;
    let range = world.range(span)?;
    let line = world.source(id).ok()?.lines().byte_to_line(range.start)?;
    Some((id, line + 1))
}

fn is_project(id: FileId) -> bool {
    matches!(id.root(), VirtualRoot::Project)
}

fn mapped_location(
    world: &WriterWorld, id: FileId, line: usize,
) -> (Option<PathBuf>, Option<SourceSpan>) {
    if !is_project(id) {
        return (Some(package_path(id)), Some(line_span(line)));
    }
    let path = world.real_path(id);
    let span = match world.source_map(id) {
        Some(map) => map.lookup(line).cloned(),
        None => Some(line_span(line)),
    };
    (path, span)
}

fn line_span(line: usize) -> SourceSpan {
    SourceSpan { start_line: line, end_line: line, cell: None, file: None }
}

fn package_path(id: FileId) -> PathBuf {
    let VirtualRoot::Package(package) = id.root() else {
        return id.vpath().get_with_slash().into();
    };
    PathBuf::from(format!("{package}{}", id.vpath().get_with_slash()))
}

fn append_hint(message: &mut String, hint: &str) {
    message.push_str(HINT_PREFIX);
    message.push_str(hint);
}

/// Computes the session's PDF creation time, using the publishing policy.
///
/// Precedence is an explicit RFC 3339 `[pdf] timestamp`, a valid `YYYY-MM-DD`
/// document date, `SOURCE_DATE_EPOCH`, then `now`. Invalid explicit timestamps
/// and epoch values are configuration errors; other document dates fall through.
pub fn pdf_timestamp(
    pdf: &PdfConfig, meta: &DocMeta, source_date_epoch: Option<&str>,
    now: SystemTime,
) -> Result<Datetime> {
    if !pdf.timestamp.is_empty() {
        let value = OffsetDateTime::parse(&pdf.timestamp, &Rfc3339).map_err(
            |error| {
                Error::Config(format!(
                    "invalid [pdf] timestamp {:?}: {error}",
                    pdf.timestamp
                ))
            },
        )?;
        let utc = value.checked_to_offset(UtcOffset::UTC).ok_or_else(|| {
            Error::Config(
                "[pdf] timestamp is outside the UTC date range".into(),
            )
        })?;
        return datetime(utc);
    }
    if let Some(date) = meta.date.as_deref().and_then(document_date) {
        return Ok(date);
    }
    if let Some(epoch) = source_date_epoch {
        let seconds = epoch.parse::<i64>().map_err(|error| {
            Error::Config(format!(
                "invalid SOURCE_DATE_EPOCH {epoch:?}: {error}"
            ))
        })?;
        let value =
            OffsetDateTime::from_unix_timestamp(seconds).map_err(|error| {
                Error::Config(format!(
                    "invalid SOURCE_DATE_EPOCH {epoch:?}: {error}"
                ))
            })?;
        return datetime(value);
    }
    let nanos = match now.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos() as i128,
        Err(error) => -(error.duration().as_nanos() as i128),
    };
    let value =
        OffsetDateTime::from_unix_timestamp_nanos(nanos).map_err(|error| {
            Error::Config(format!("invalid current time: {error}"))
        })?;
    datetime(value)
}

// Keep the date and time components grouped in this constructor.
#[rustfmt::skip]
fn document_date(date: &str) -> Option<Datetime> {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    Datetime::from_ymd_hms(
        date[..4].parse().ok()?, date[5..7].parse().ok()?,
        date[8..].parse().ok()?, 0, 0, 0,
    )
}

// Keep date components separate from time components.
#[rustfmt::skip]
fn datetime(value: OffsetDateTime) -> Result<Datetime> {
    Datetime::from_ymd_hms(
        value.year(), u8::from(value.month()), value.day(),
        value.hour(), value.minute(), value.second(),
    )
    .ok_or_else(|| {
        Error::Config("PDF timestamp is outside Typst's date range".into())
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use typst::diag::Tracepoint;
    use typst::syntax::{
        LinkedNode, RootedPath, Side, Span, Spanned, VirtualPath,
    };

    use super::*;
    use crate::typst::sourcemap::SourceMap;
    use crate::typst::world::{MainFile, WorldOptions};

    fn at(
        year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8,
    ) -> Datetime {
        Datetime::from_ymd_hms(year, month, day, hour, minute, second).unwrap()
    }

    #[test]
    fn result_retains_warnings() {
        let diagnostics =
            vec![Diagnostic::warning("typst-warning", "a warning")].into();
        let (output, diagnostics) =
            Compiled { output: Some(42), diagnostics }.into_result().unwrap();
        assert_eq!(output, 42);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics.has_warnings());
    }

    #[test]
    fn result_uses_first_error_and_splits_hints() {
        let first = Diagnostic {
            level: Level::Error,
            code: "typst-error",
            message: "unknown value\n  hint: choose x\n  hint: choose y".into(),
            path: Some("/docs/main.typ".into()),
            span: Some(line_span(12)),
        };
        let diagnostics = vec![
            Diagnostic::warning("typst-warning", "a warning"),
            first,
            Diagnostic::error("typst-error", "a later error"),
        ]
        .into();
        let error = Compiled { output: Some(()), diagnostics }
            .into_result()
            .unwrap_err();
        let Error::Compile { path, line, message, hints } = error else {
            panic!("expected a compile error");
        };
        assert_eq!(path, PathBuf::from("/docs/main.typ"));
        assert_eq!(line, Some(12));
        assert_eq!(message, "unknown value");
        assert_eq!(hints, ["choose x", "choose y"]);
    }

    #[test]
    fn result_falls_back_to_virtual_main_path() {
        let diagnostics =
            vec![Diagnostic::error("typst-error", "bad source")].into();
        let result = Compiled::<()> { output: None, diagnostics }.into_result();
        assert!(matches!(result,
            Err(Error::Compile { path, line: None, .. }) if path == std::path::Path::new(MAIN_PATH)
        ));
        let result =
            Compiled::<()> { output: None, diagnostics: Diagnostics::new() }
                .into_result();
        assert!(matches!(result, Err(Error::Compile { .. })));
    }

    fn diagnostic_world() -> (tempfile::TempDir, WriterWorld, FileId) {
        let dir = tempfile::tempdir().unwrap();
        let packages = dir.path().join("packages");
        let package = packages.join("local/test/0.1.0");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join("lib.typ"), "failure").unwrap();
        let mut world = WriterWorld::new(WorldOptions {
            root: dir.path().to_owned(),
            main: MainFile::Virtual,
            today: at(2026, 10, 24, 0, 0, 0),
            system_fonts: false,
            font_paths: Vec::new(),
            package_paths: vec![packages],
            allow_packages: false,
            inputs: BTreeMap::new(),
        })
        .unwrap();
        world.set_main_text(
            "inner\n\nouter",
            SourceMap::from_lines(vec![
                Some(line_span(11)),
                None,
                Some(line_span(33)),
            ]),
        );
        let id = RootedPath::new(
            VirtualRoot::Package("@local/test:0.1.0".parse().unwrap()),
            VirtualPath::new("/lib.typ").unwrap(),
        )
        .intern();
        (dir, world, id)
    }

    fn main_span(world: &WriterWorld, needle: &str) -> Span {
        let source = world.source(world.main()).unwrap();
        let offset = source.text().find(needle).unwrap();
        LinkedNode::new(source.root())
            .leaf_at(offset, Side::After)
            .unwrap()
            .span()
    }

    #[test]
    fn package_trace_maps_to_outermost_project_call() {
        let (_dir, world, package) = diagnostic_world();
        let mut diagnostic = SourceDiagnostic::error(
            DiagSpan::from_range(package, 0..1),
            "package failed",
        );
        diagnostic.hint("check the argument");
        for needle in ["inner", "outer"] {
            diagnostic.trace.push(Spanned::new(
                Tracepoint::Call(None),
                main_span(&world, needle),
            ));
        }
        let mapped = map_diagnostic(&world, &diagnostic);
        assert_eq!(mapped.path, None);
        assert_eq!(mapped.span, Some(line_span(33)));
        assert_eq!(
            mapped.message,
            concat!(
                "package failed\n  hint: check the argument",
                "\n  hint: in @local/test:0.1.0/lib.typ:1",
            )
        );
    }

    #[test]
    fn project_diagnostic_takes_precedence_over_trace() {
        let (_dir, mut world, _) = diagnostic_world();
        let mut diagnostic = SourceDiagnostic::error(
            main_span(&world, "inner"),
            "project failed",
        );
        diagnostic.trace.push(Spanned::new(
            Tracepoint::Call(None),
            main_span(&world, "outer"),
        ));
        assert_eq!(
            map_diagnostic(&world, &diagnostic).span,
            Some(line_span(11))
        );
        world.set_main_text("inner", SourceMap::from_lines(vec![None]));
        let generated =
            SourceDiagnostic::error(main_span(&world, "inner"), "scaffolding");
        let mapped = map_diagnostic(&world, &generated);
        assert_eq!(mapped.path, None);
        assert_eq!(mapped.span, None);
    }

    #[test]
    fn package_location_is_retained_without_project_call() {
        let (_dir, world, package) = diagnostic_world();
        let diagnostic = SourceDiagnostic::error(
            DiagSpan::from_range(package, 0..1),
            "package failed",
        );
        let mapped = map_diagnostic(&world, &diagnostic);
        assert_eq!(mapped.path, Some("@local/test:0.1.0/lib.typ".into()));
        assert_eq!(mapped.span, Some(line_span(1)));
        assert_eq!(mapped.message, "package failed");
    }

    #[test]
    fn detached_warning_and_download_hint_are_preserved() {
        let (_dir, world, _) = diagnostic_world();
        let mut warning =
            SourceDiagnostic::warning(DiagSpan::detached(), "take care");
        warning.hint("try another value");
        let mapped = map_diagnostic(&world, &warning);
        assert_eq!(mapped.level, Level::Warning);
        assert_eq!(mapped.code, "typst-warning");
        assert_eq!(mapped.message, "take care\n  hint: try another value");
        assert_eq!((mapped.path, mapped.span), (None, None));
        let diagnostic = SourceDiagnostic::error(
            DiagSpan::detached(),
            "package not found (searched for @preview/missing:0.1.0)",
        );
        assert_eq!(
            map_diagnostic(&world, &diagnostic).message,
            format!("{}\n  hint: {DOWNLOAD_HINT}", diagnostic.message,)
        );
    }

    #[test]
    fn explicit_timestamp_is_utc_and_has_precedence() {
        let pdf = PdfConfig {
            timestamp: "2026-10-24T12:30:00.123+02:00".into(),
            ..PdfConfig::default()
        };
        let meta =
            DocMeta { date: Some("2000-01-01".into()), ..DocMeta::default() };
        assert_eq!(
            pdf_timestamp(&pdf, &meta, Some("invalid"), UNIX_EPOCH).unwrap(),
            at(2026, 10, 24, 10, 30, 0),
        );
    }

    #[test]
    fn timestamp_date_fallback_validates_calendar_days() {
        let pdf = PdfConfig::default();
        for (date, expected) in [
            ("2024-02-29", at(2024, 2, 29, 0, 0, 0)),
            ("2023-02-29", at(1970, 1, 1, 0, 0, 0)),
            ("2026-13-01", at(1970, 1, 1, 0, 0, 0)),
            ("2026-1-01", at(1970, 1, 1, 0, 0, 0)),
            ("October 2026", at(1970, 1, 1, 0, 0, 0)),
            ("ééééé", at(1970, 1, 1, 0, 0, 0)),
        ] {
            let meta =
                DocMeta { date: Some(date.into()), ..DocMeta::default() };
            assert_eq!(
                pdf_timestamp(&pdf, &meta, Some("0"), UNIX_EPOCH).unwrap(),
                expected,
                "{date}",
            );
        }
    }

    #[test]
    fn invalid_explicit_timestamp_and_epoch_are_config_errors() {
        let meta = DocMeta::default();
        for timestamp in [
            "2026-10-24", "invalid", "2026-02-30T00:00:00Z",
            "9999-12-31T23:59:59-01:00",
        ] {
            let pdf = PdfConfig {
                timestamp: timestamp.into(),
                ..PdfConfig::default()
            };
            assert!(matches!(
                pdf_timestamp(&pdf, &meta, None, UNIX_EPOCH),
                Err(Error::Config(_))
            ));
        }
        for epoch in ["", "not a number", "1.5", "9223372036854775807"] {
            assert!(matches!(
                pdf_timestamp(
                    &PdfConfig::default(),
                    &meta,
                    Some(epoch),
                    UNIX_EPOCH
                ),
                Err(Error::Config(_))
            ));
        }
    }

    #[test]
    fn timestamp_uses_epoch_then_now_at_whole_seconds() {
        let pdf = PdfConfig::default();
        let meta = DocMeta::default();
        let now = UNIX_EPOCH + Duration::new(42, 123_456_789);
        assert_eq!(
            pdf_timestamp(&pdf, &meta, Some("1700000000"), now).unwrap(),
            at(2023, 11, 14, 22, 13, 20),
        );
        assert_eq!(
            pdf_timestamp(&pdf, &meta, None, now).unwrap(),
            at(1970, 1, 1, 0, 0, 42),
        );
        assert_eq!(
            pdf_timestamp(
                &pdf,
                &meta,
                None,
                UNIX_EPOCH - Duration::from_millis(1)
            )
            .unwrap(),
            at(1969, 12, 31, 23, 59, 59),
        );
        assert_eq!(
            pdf_timestamp(&pdf, &meta, Some("-1"), now).unwrap(),
            at(1969, 12, 31, 23, 59, 59),
        );
    }
}
