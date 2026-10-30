//! `writer dump`: resolved or parsed IR, template context and source exports.
//! Owner: T16. Error handling follows presenter_rs's dump command.

use std::io::{self, Write};
use std::process::ExitCode;

use super::{
    DumpArgs, EXIT_BUILD_ERROR, GlobalArgs, UsageError, input_kind, locate,
    print_stdout, report, report_diagnostics,
};
use crate::config::Config;
use crate::pipeline::{self, BuildOptions, Prepared, WorldSlot};

/// Runs `writer dump`; structured results go only to stdout.
pub fn run(args: &DumpArgs, global: &GlobalArgs) -> anyhow::Result<ExitCode> {
    if args.default_config {
        let text = Config::default_toml();
        print_stdout(text.strip_suffix('\n').unwrap_or(&text))?;
        return Ok(ExitCode::SUCCESS);
    }
    let input = args
        .input
        .as_ref()
        .ok_or_else(|| UsageError("missing INPUT".into()))?;
    let mut opts = BuildOptions::new(input);
    opts.format = Some(input_kind(input)?);
    opts.config_path = global.config.clone();
    let prepared = prepared(&opts, args.no_resolve, global)?;
    let mut diagnostics = prepared.diagnostics.clone();
    let text = if args.context {
        let mut worlds = WorldSlot::new();
        let world = worlds.get(Some(&prepared), &opts).map_err(|failure| {
            report_diagnostics(&diagnostics, global);
            report::report_build_failure(failure, input, global)
        })?;
        let context =
            pipeline::html::context(&prepared, world).map_err(|error| {
                report_diagnostics(&diagnostics, global);
                locate(error, input)
            })?;
        serde_json::to_string_pretty(&context)?
    } else if args.latex {
        let (text, extra) = pipeline::latex::dump_detailed(&prepared, &opts)
            .map_err(|mut failure| {
                let mut found = diagnostics.clone();
                found.extend(failure.diagnostics);
                failure.diagnostics = found;
                report::report_build_failure(failure, input, global)
            })?;
        diagnostics.extend(extra);
        text
    } else if args.typst {
        let (text, extra) =
            pipeline::pdf::dump(&prepared, &opts).map_err(|error| {
                report_diagnostics(&diagnostics, global);
                locate(error, input)
            })?;
        diagnostics.extend(extra);
        text
    } else {
        serde_json::to_string_pretty(&prepared.document)?
    };
    report_diagnostics(&diagnostics, global);
    if args.latex {
        print_latex(io::stdout().lock(), &text)?;
    } else if !text.is_empty() {
        print_stdout(&text)?;
    }
    Ok(if diagnostics.has_errors() {
        ExitCode::from(EXIT_BUILD_ERROR)
    } else {
        ExitCode::SUCCESS
    })
}

// LaTeX is a complete template result, not line-oriented command output.
fn print_latex(mut output: impl Write, text: &str) -> io::Result<()> {
    match output.write_all(text.as_bytes()).and_then(|()| output.flush()) {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}

fn prepared(
    opts: &BuildOptions, no_resolve: bool, global: &GlobalArgs,
) -> anyhow::Result<Prepared> {
    let result = if no_resolve {
        pipeline::parse_only(opts)
    } else {
        pipeline::prepare(opts)
    };
    result.map_err(|failure| {
        report::report_build_failure(failure, &opts.input, global)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latex_stdout_preserves_empty_and_every_trailing_newline_byte() {
        for source in ["", "no final line feed", "one\n", "two\n\n"] {
            let mut output = Vec::new();
            print_latex(&mut output, source).unwrap();
            assert_eq!(output, source.as_bytes());
        }
    }

    struct Failing {
        kind: io::ErrorKind,
        flush_only: bool,
    }

    impl Write for Failing {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.flush_only {
                Ok(bytes.len())
            } else {
                Err(io::Error::from(self.kind))
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(self.kind))
        }
    }

    #[test]
    fn latex_stdout_ignores_only_broken_pipes_in_write_or_flush() {
        for flush_only in [false, true] {
            assert!(
                print_latex(
                    Failing { kind: io::ErrorKind::BrokenPipe, flush_only },
                    "complete source"
                )
                .is_ok()
            );
            let error = print_latex(
                Failing { kind: io::ErrorKind::PermissionDenied, flush_only },
                "complete source",
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        }
    }
}
