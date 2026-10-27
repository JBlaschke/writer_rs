//! `writer check`: frontend and reference diagnostics without output files.
//!
//! Text findings go to stderr; JSON is one complete stdout array, including
//! input/config/parse failures. Quiet mode retains warnings that fail the run.

use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;

use super::{
    CheckArgs, CheckFormat, Compatibility, EXIT_BUILD_ERROR, EXIT_OK,
    GlobalArgs, display_path, print_stdout, report, report_diagnostics,
};
use crate::check::{self, CheckOptions};
use crate::error::{Diagnostic, Diagnostics, Error, Level};
use crate::ir::{self, SourceKind};
use crate::pipeline::{self, BuildOptions, Mode};

/// Runs `writer check`; future Typst and per-target checks remain explicit stubs.
pub fn run(args: &CheckArgs, global: &GlobalArgs) -> anyhow::Result<ExitCode> {
    if args.target.is_some() {
        return Err(pipeline::not_implemented("check --target", "T71").into());
    }
    let diagnostics = check_input(args, global)?;
    match args.format {
        CheckFormat::Text => {
            report_diagnostics(&diagnostics, &text_report_flags(global, args));
        }
        CheckFormat::Json => print_stdout(&json_report(&diagnostics)?)?,
    }
    Ok(ExitCode::from(exit_code(&diagnostics, args.deny_warnings)))
}

fn check_input(
    args: &CheckArgs, global: &GlobalArgs,
) -> anyhow::Result<Diagnostics> {
    let kind = match kind(&args.input) {
        Ok(kind) => kind,
        Err(error) => {
            return Ok([check::error_diagnostic(&error, &args.input)]
                .into_iter()
                .collect());
        }
    };
    if kind == SourceKind::Typst {
        return Err(pipeline::not_implemented("check typst", "T50").into());
    }
    let mut opts = BuildOptions::new(&args.input);
    opts.format = Some(kind);
    opts.config_path = global.config.clone();
    opts.mode = Mode::Check;
    let options = CheckOptions {
        compat_presenter: args.compat == Some(Compatibility::Presenter),
    };
    Ok(match check::run(&opts, &options) {
        Ok(diagnostics) => diagnostics,
        Err(failure) => {
            let mut diagnostics = failure.diagnostics;
            let error = report::with_display_paths(*failure.error);
            diagnostics.push(check::error_diagnostic(&error, &args.input));
            diagnostics
        }
    })
}

/// `check` reports format failures as diagnostics, unlike the shared usage helper.
fn kind(input: &Path) -> crate::error::Result<SourceKind> {
    ir::detect_kind(input).map_err(|error| {
        let message = error.to_string();
        let prefix = format!("{}: ", input.display());
        Error::Other(message.strip_prefix(&prefix).unwrap_or(&message).into())
    })
}

fn text_report_flags(global: &GlobalArgs, args: &CheckArgs) -> GlobalArgs {
    GlobalArgs {
        verbose: if global.quiet {
            0
        } else {
            global.verbose.max(1)
        },
        quiet: global.quiet && !args.deny_warnings,
        ..global.clone()
    }
}

fn exit_code(diagnostics: &Diagnostics, deny_warnings: bool) -> u8 {
    if diagnostics.has_errors() || deny_warnings && diagnostics.has_warnings() {
        EXIT_BUILD_ERROR
    } else {
        EXIT_OK
    }
}

#[derive(Serialize)]
struct JsonDiagnostic<'a> {
    level: Level,
    code: &'a str,
    message: &'a str,
    file: Option<String>,
    line: Option<usize>,
    cell: Option<usize>,
}

impl<'a> From<&'a Diagnostic> for JsonDiagnostic<'a> {
    fn from(diagnostic: &'a Diagnostic) -> Self {
        let span = diagnostic.span.as_ref();
        Self {
            level: diagnostic.level,
            code: diagnostic.code,
            message: &diagnostic.message,
            file: diagnostic
                .path
                .as_deref()
                .map(|path| display_path(path).display().to_string()),
            line: span.map(|span| span.start_line),
            cell: span.and_then(|span| span.cell),
        }
    }
}

fn json_report(diagnostics: &Diagnostics) -> anyhow::Result<String> {
    let entries: Vec<_> =
        diagnostics.iter().map(JsonDiagnostic::from).collect();
    Ok(serde_json::to_string_pretty(&entries)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::SourceSpan;
    use serde_json::json;

    fn args() -> CheckArgs {
        CheckArgs {
            input: "source.md".into(),
            target: None,
            deny_warnings: false,
            format: CheckFormat::Text,
            compat: None,
        }
    }

    #[test]
    fn only_errors_and_denied_warnings_fail() {
        let empty = Diagnostics::new();
        let info = Diagnostics::from(vec![Diagnostic::info("note", "one")]);
        let warning =
            Diagnostics::from(vec![Diagnostic::warning("warning", "one")]);
        let error = Diagnostics::from(vec![Diagnostic::error("error", "one")]);
        for diagnostics in [&empty, &info] {
            assert_eq!(exit_code(diagnostics, false), EXIT_OK);
            assert_eq!(exit_code(diagnostics, true), EXIT_OK);
        }
        assert_eq!(exit_code(&warning, false), EXIT_OK);
        assert_eq!(exit_code(&warning, true), EXIT_BUILD_ERROR);
        assert_eq!(exit_code(&error, false), EXIT_BUILD_ERROR);
        assert_eq!(exit_code(&error, true), EXIT_BUILD_ERROR);
    }

    #[test]
    fn check_shows_info_without_verbose_and_quiet_keeps_failing_warnings() {
        let mut args = args();
        let ordinary = text_report_flags(&GlobalArgs::default(), &args);
        assert_eq!(ordinary.verbose, 1);
        assert!(!ordinary.quiet);
        let quiet = GlobalArgs { quiet: true, ..Default::default() };
        let hidden = text_report_flags(&quiet, &args);
        assert!(hidden.quiet);
        assert_eq!(hidden.verbose, 0);
        args.deny_warnings = true;
        let denied = text_report_flags(&quiet, &args);
        assert!(!denied.quiet);
        assert_eq!(denied.verbose, 0, "quiet still hides informational notes");
    }

    #[test]
    fn json_has_the_frozen_flat_fields_and_relative_files() {
        let cwd = std::env::current_dir().unwrap();
        let diagnostics = Diagnostics::from(vec![
            Diagnostic::warning("missing-asset", "not found")
                .with_path(cwd.join("source.ipynb"))
                .with_span(SourceSpan {
                    start_line: 4,
                    end_line: 8,
                    cell: Some(2),
                    file: None,
                }),
            Diagnostic::info("note", "explanation"),
        ]);
        let value: serde_json::Value =
            serde_json::from_str(&json_report(&diagnostics).unwrap()).unwrap();
        assert_eq!(
            value,
            json!([
                { "level":"warning", "code":"missing-asset", "message":"not found", "file":"source.ipynb", "line":4, "cell":2 },
                { "level":"info", "code":"note", "message":"explanation", "file":null, "line":null, "cell":null },
            ])
        );
        assert_eq!(json_report(&Diagnostics::new()).unwrap(), "[]");
    }

    #[test]
    fn unknown_extensions_are_diagnostics_without_repeating_the_file() {
        let input = Path::new("notes.txt");
        let error = kind(input).unwrap_err();
        let diagnostic = check::error_diagnostic(&error, input);
        assert_eq!(diagnostic.level, Level::Error);
        assert_eq!(diagnostic.path, Some(std::path::absolute(input).unwrap()));
        assert!(!diagnostic.message.contains("notes.txt:"));
        assert!(diagnostic.message.contains("cannot tell the input format"));
    }
}
