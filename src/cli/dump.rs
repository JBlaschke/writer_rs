//! `writer dump`: resolved or parsed IR, template context and source exports.
//! Owner: T16. Error handling follows presenter_rs's dump command.

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
    } else if args.typst || args.latex {
        let (text, extra) = if args.typst {
            pipeline::pdf::dump(&prepared, &opts)
        } else {
            pipeline::latex::dump(&prepared, &opts)
        }
        .map_err(|error| {
            report_diagnostics(&diagnostics, global);
            locate(error, input)
        })?;
        diagnostics.extend(extra);
        text
    } else {
        serde_json::to_string_pretty(&prepared.document)?
    };
    report_diagnostics(&diagnostics, global);
    if !text.is_empty() {
        print_stdout(&text)?;
    }
    Ok(if diagnostics.has_errors() {
        ExitCode::from(EXIT_BUILD_ERROR)
    } else {
        ExitCode::SUCCESS
    })
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
