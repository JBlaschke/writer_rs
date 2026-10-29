//! `writer build`: select targets, report results and print written paths.
//! Owner: T16. Formatting and argument helpers lifted from presenter_rs.

use std::io::Write;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;

use super::{
    BuildArgs, EXIT_BUILD_ERROR, GlobalArgs, UsageError, clash_error,
    display_path, exit_code_for, input_format, print_stdout, report,
    report_diagnostics,
};
use crate::config::{CliOverrides, Target};
use crate::error::{Diagnostics, Error};
use crate::ir::SourceKind;
use crate::pipeline::{self, BuildOptions, BuildReport, WorldSlot};
use crate::serve::watch::{
    self as watcher, ChangeBatch, ChangeKind, WatchStatus, Watcher,
};

const WATCHING: &str = "Watching for changes (Ctrl-C to stop)";

/// Runs `writer build`; independent targets keep their successful outputs.
pub fn run(args: &BuildArgs, global: &GlobalArgs) -> anyhow::Result<ExitCode> {
    if args.watch {
        return watch(args, global);
    }
    let (opts, targets) = build_options(args, global)?;
    let report =
        pipeline::build_targets(&opts, &targets, &mut WorldSlot::new());
    build_result(report, &opts, args.out.is_some(), global)
}

/// Watches inputs and reports rebuilds until interrupted.
pub fn watch(
    args: &BuildArgs, global: &GlobalArgs,
) -> anyhow::Result<ExitCode> {
    let (opts, targets, announced) = match build_options(args, global) {
        Ok((opts, targets)) => (opts, targets, false),
        Err(error) if is_config_error(&error) => {
            let (opts, targets) = wait_for_config(args, global, &error)?;
            (opts, targets, true)
        }
        Err(error) => return Err(error),
    };
    let mut first = true;
    let mut refused = false;
    let mut stopped = None;
    watcher::run_build_watch_with(&opts, &targets, |status| {
        let stop_for_clash = match &status {
            WatchStatus::Built(report) => {
                std::mem::take(&mut first) && initial_clash(report).is_some()
            }
            _ => false,
        };
        if !(announced && matches!(status, WatchStatus::Watching(_)))
            && let Err(error) =
                print_watch_status(status, &opts, args.out.is_some(), global)
        {
            stopped = Some(error);
            return ControlFlow::Break(());
        }
        if stop_for_clash {
            refused = true;
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    })?;
    match stopped {
        Some(error) => Err(error),
        None if refused => Ok(ExitCode::from(super::EXIT_USAGE)),
        None => Ok(ExitCode::SUCCESS),
    }
}

fn initial_clash(report: &BuildReport) -> Option<&pipeline::Clash> {
    report
        .prepared
        .as_ref()
        .err()
        .and_then(|failure| pipeline::refused_clash(&failure.error))
        .or_else(|| {
            report.targets.iter().find_map(|target| {
                target.written.as_ref().err().and_then(pipeline::refused_clash)
            })
        })
}

fn is_config_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(cause.downcast_ref::<Error>(), Some(Error::Config(_)))
    })
}

fn wait_for_config(
    args: &BuildArgs, global: &GlobalArgs, error: &anyhow::Error,
) -> anyhow::Result<(BuildOptions, Vec<Target>)> {
    report::report_error(error);
    let (tx, rx) = mpsc::channel();
    let (opts, _) = base_options(args, global)?;
    let mut watcher =
        Watcher::start(&watcher::watch_set_before_build(&opts), tx)?;
    status_line(WATCHING, global);
    for batch in rx {
        status_line(&rebuilding_line(&batch), global);
        match build_options(args, global) {
            Ok(found) => return Ok(found),
            Err(error) if is_config_error(&error) => {
                if let Err(error) =
                    watcher.update(&watcher::watch_set_before_build(&opts))
                {
                    tracing::warn!("{error}");
                }
                report::report_error(&error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(anyhow::anyhow!("file watcher stopped"))
}

fn print_watch_status(
    status: WatchStatus<'_>, opts: &BuildOptions, explicit: bool,
    global: &GlobalArgs,
) -> anyhow::Result<()> {
    match status {
        WatchStatus::Built(report) => {
            if let Err(failure) = &report.prepared {
                if let Some(clash) = pipeline::refused_clash(&failure.error) {
                    report_diagnostics(&failure.diagnostics, global);
                    report::report_error(&clash_error(clash, explicit));
                } else {
                    report::print_build_failure(failure, &opts.input, global);
                }
                return Ok(());
            }
            report_diagnostics(&report.diagnostics, global);
            for target in &report.targets {
                match &target.written {
                    Ok(paths) if !global.quiet => {
                        for path in paths {
                            print_stdout(&wrote_line(path)?)?;
                        }
                    }
                    Ok(_) => {}
                    Err(error)
                        if target_error_reported(
                            error, &report.diagnostics,
                        ) => {}
                    Err(error) => match pipeline::refused_clash(error) {
                        Some(clash) => {
                            report::report_error(&clash_error(clash, explicit));
                        }
                        None => report::print_error_in(
                            error,
                            Some(&opts.input),
                            &report::read_source,
                        ),
                    },
                }
            }
        }
        WatchStatus::Watching(_) => status_line(WATCHING, global),
        WatchStatus::Changed(batch) => {
            status_line(&rebuilding_line(batch), global);
        }
    }
    Ok(())
}

fn rebuilding_line(batch: &ChangeBatch) -> String {
    const SHOWN: usize = 3;
    let kind = match batch.kind {
        ChangeKind::Css => "css",
        ChangeKind::Pages => "pages",
        ChangeKind::Content => "content",
    };
    let mut paths: Vec<_> = batch
        .paths
        .iter()
        .take(SHOWN)
        .map(|path| display_path(path).display().to_string())
        .collect();
    if batch.paths.len() > SHOWN {
        paths.push(format!("and {} more", batch.paths.len() - SHOWN));
    }
    format!("Rebuilding ({kind}): {}", paths.join(", "))
}

fn status_line(text: &str, global: &GlobalArgs) {
    if !global.quiet {
        let _ = writeln!(std::io::stderr().lock(), "{text}");
    }
}

/// Builds the effective options and target list from CLI and configuration.
pub fn build_options(
    args: &BuildArgs, global: &GlobalArgs,
) -> anyhow::Result<(BuildOptions, Vec<Target>)> {
    let (opts, kind) = base_options(args, global)?;
    let (config, _) = pipeline::load_config(&opts)?;
    let mut targets = config.targets_for(kind);
    let mut seen = std::collections::HashSet::new();
    targets.retain(|target| seen.insert(*target));
    if kind == SourceKind::Typst && targets.contains(&Target::Typst) {
        return Err(UsageError(
            "typst source is already a .typ file; use writer build part.md --to typst".into(),
        ).into());
    }
    Ok((opts, targets))
}

fn base_options(
    args: &BuildArgs, global: &GlobalArgs,
) -> anyhow::Result<(BuildOptions, SourceKind)> {
    let mut opts = BuildOptions::new(&args.input);
    let kind = input_format(&args.input, args.format)?;
    opts.format = Some(kind);
    opts.config_path = global.config.clone();
    opts.cli = CliOverrides {
        targets: args
            .to
            .as_ref()
            .map(|targets| targets.iter().copied().map(Target::from).collect()),
        out: args.out.clone(),
        theme: args.theme.clone(),
        template: args.template.clone(),
        reference_doc: args.reference_doc.clone(),
        latex_template: args.latex_template.clone(),
        standalone: args.standalone_override(),
        strict: args.strict.then_some(true),
        engine: args.engine.map(Into::into),
        allow_packages: args.allow_packages.then_some(true),
        ..CliOverrides::default()
    };
    Ok((opts, kind))
}

fn build_result(
    report: BuildReport, opts: &BuildOptions, explicit: bool,
    global: &GlobalArgs,
) -> anyhow::Result<ExitCode> {
    let BuildReport { prepared, targets, diagnostics } = report;
    if let Err(failure) = prepared {
        if let Some(clash) = pipeline::refused_clash(&failure.error) {
            report_diagnostics(&failure.diagnostics, global);
            return Err(clash_error(clash, explicit));
        }
        return Err(report::report_build_failure(failure, &opts.input, global));
    }
    report_diagnostics(&diagnostics, global);
    let mut exit = if diagnostics.has_errors() {
        EXIT_BUILD_ERROR
    } else {
        0
    };
    for target in targets {
        match target.written {
            Ok(paths) if !global.quiet => {
                for path in paths {
                    print_stdout(&wrote_line(&path)?)?;
                }
            }
            Ok(_) => {}
            Err(error) => {
                let reported = target_error_reported(&error, &diagnostics);
                let error = match pipeline::refused_clash(&error) {
                    Some(clash) => clash_error(clash, explicit),
                    None => super::locate(error, &opts.input),
                };
                exit = exit.max(exit_code_for(&error));
                if !reported {
                    report::report_error(&error);
                }
            }
        }
    }
    Ok(ExitCode::from(exit))
}

/// Detailed target failures can also carry their error as a reported finding.
fn target_error_reported(error: &Error, diagnostics: &Diagnostics) -> bool {
    if let Error::MissingAsset { path, span } = error {
        return diagnostics.iter().any(|finding| {
            finding.level == crate::error::Level::Error
                && matches!(
                    pipeline::missing_asset_as_error(finding),
                    Some(Error::MissingAsset { path: found, span: located })
                        if &found == path && &located == span
                )
        });
    }
    let Error::Compile { path, line, message, .. } = error else {
        return false;
    };
    diagnostics.iter().any(|diagnostic| {
        diagnostic.level == crate::error::Level::Error
            && diagnostic.code == "typst-error"
            && diagnostic.path.as_deref() == Some(path.as_path())
            && diagnostic.span.as_ref().map(|span| span.start_line) == *line
            && diagnostic.message.split("\n  hint: ").next()
                == Some(message.as_str())
    })
}

fn wrote_line(path: &Path) -> anyhow::Result<String> {
    let (bytes, detail) = if path.is_dir() {
        let files = directory_files(path)?;
        let bytes = files.iter().try_fold(0usize, |bytes, file| {
            Ok::<_, std::io::Error>(bytes + file.metadata()?.len() as usize)
        })?;
        (bytes, Some(format!("{} files", files.len())))
    } else {
        let bytes = path.metadata()?.len() as usize;
        let detail = if path.extension().is_some_and(|ext| ext == "pdf") {
            pipeline::pdf::page_count(&std::fs::read(path)?)
                .map(|pages| format!("{pages} pages"))
        } else {
            None
        };
        (bytes, detail)
    };
    let size = format_size(bytes);
    let detail =
        detail.map_or(size.clone(), |detail| format!("{detail}, {size}"));
    Ok(format!("Wrote {} ({detail})", display_path(path).display()))
}

fn directory_files(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(path) {
        let entry = entry.map_err(std::io::Error::other)?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

/// Decimal units with one decimal place, switching units after rounding.
pub fn format_size(bytes: usize) -> String {
    if bytes < 1_000 {
        return format!("{bytes} B");
    }
    // Output sizes are far below 2^52; this conversion is sufficiently exact.
    let kilo_tenths = (bytes as f64 / 100.0).round();
    if kilo_tenths < 10_000.0 {
        format!("{:.1} kB", kilo_tenths / 10.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1e6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::EXIT_USAGE;
    use crate::error::Error;

    #[test]
    fn sizes_use_decimal_units() {
        for (bytes, expected) in [
            (0, "0 B"),
            (999, "999 B"),
            (1_000, "1.0 kB"),
            (1_049, "1.0 kB"),
            (1_050, "1.1 kB"),
            (12_345, "12.3 kB"),
            (999_949, "999.9 kB"),
            (999_950, "1.0 MB"),
            (999_999, "1.0 MB"),
            (1_000_000, "1.0 MB"),
            (1_849_000, "1.8 MB"),
            (1_250_000_000, "1250.0 MB"),
        ] {
            assert_eq!(format_size(bytes), expected);
        }
    }

    #[test]
    fn written_paths_report_file_and_directory_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("index.html");
        std::fs::write(&file, "hello").unwrap();
        assert!(wrote_line(&file).unwrap().ends_with("index.html (5 B)"));
        assert!(wrote_line(dir.path()).unwrap().ends_with("(1 files, 5 B)"));
    }

    #[test]
    fn stub_errors_are_usage_errors() {
        let error = anyhow::Error::new(Error::Other(
            "pdf: not implemented yet (T32)".into(),
        ));
        assert_eq!(exit_code_for(&error), EXIT_USAGE);
    }

    #[test]
    fn rebuild_status_names_kind_and_limits_paths() {
        for (kind, label) in [
            (ChangeKind::Css, "css"),
            (ChangeKind::Pages, "pages"),
            (ChangeKind::Content, "content"),
        ] {
            let batch = ChangeBatch {
                paths: ["a", "b", "c", "d", "e"].map(PathBuf::from).to_vec(),
                kind,
            };
            assert_eq!(
                rebuilding_line(&batch),
                format!("Rebuilding ({label}): a, b, c, and 2 more"),
            );
        }
        let batch = ChangeBatch {
            paths: vec!["only.md".into()],
            kind: ChangeKind::Content,
        };
        assert_eq!(rebuilding_line(&batch), "Rebuilding (content): only.md");
    }
}
