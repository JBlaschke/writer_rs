//! LaTeX source export with complete resource reads and destination preflight.

use std::path::{Path, PathBuf};

use super::{BuildFailure, BuildOptions, Prepared};
use crate::config::Target;
use crate::error::{Diagnostic, Diagnostics, Error, Level, Result};
use crate::latex::{self, LatexOutput};
use crate::typst::world::WriterWorld;

type Detailed<T> = std::result::Result<T, BuildFailure>;

/// Builds a TeX document and sidecars through the caller's publishing session.
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)> {
    build_detailed(prepared, opts, out, world).map_err(|failure| *failure.error)
}

/// Retains every emission finding when later materialization or writing fails.
pub(crate) fn build_detailed(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Detailed<(Vec<PathBuf>, Diagnostics)> {
    build_observed(prepared, opts, out, world, &mut |_| {})
}

pub(crate) fn build_observed(
    prepared: &Prepared, _opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld, record: &mut dyn FnMut(super::WrittenDirectory),
) -> Detailed<(Vec<PathBuf>, Diagnostics)> {
    let stem = output_stem(out)?;
    let LatexOutput { tex, files, diagnostics } = latex::render_detailed(
        &prepared.document, &prepared.assets, &prepared.config, stem,
    )
    .map_err(|failure| BuildFailure {
        error: failure.error,
        diagnostics: failure.diagnostics,
    })?;
    if let Some(finding) =
        diagnostics.iter().find(|finding| finding.level == Level::Error)
    {
        return Err(failure(diagnostic_error(finding, prepared), &diagnostics));
    }
    // Pure source/copy builds never invoke or reconfigure the retained World.
    let resources = if latex::needs_world(&files) {
        latex::materialise(&files, &prepared.assets, Some(&mut *world))
    } else {
        latex::materialise(&files, &prepared.assets, None)
    }
    .map_err(|error| failure(error, &diagnostics))?;
    let parent = out.parent().unwrap_or(Path::new("."));
    let directory =
        (!resources.is_empty()).then(|| parent.join(format!("{stem}_assets")));
    let statistics = directory.as_ref().map(|path| super::WrittenDirectory {
        path: path.clone(),
        files: resources.len(),
        bytes: resources.values().map(Vec::len).sum(),
    });
    let sidecars = resources
        .into_iter()
        .map(|(name, bytes)| {
            (parent.join(format!("{stem}_assets")).join(name), bytes)
        })
        .collect::<Vec<_>>();
    let paths = std::iter::once(out)
        .chain(sidecars.iter().map(|(path, _)| path.as_path()))
        .collect::<Vec<_>>();
    // Renditions may have read actual custom fonts. The same native-PDF
    // collector protects those reads in addition to every prepared input.
    let inputs =
        super::typst_source::consumed_inputs(prepared, world.accessed());
    super::export::preflight(&paths, directory.as_deref(), &inputs, "LaTeX")
        .map_err(|error| failure(error, &diagnostics))?;
    // Sidecars are complete before the main file is replaced. Expected render,
    // read and preflight failures have already left every old output intact.
    for (path, bytes) in sidecars {
        super::write_file(&path, &bytes)
            .map_err(|error| failure(error, &diagnostics))?;
    }
    super::write_file(out, tex.as_bytes())
        .map_err(|error| failure(error, &diagnostics))?;
    if let Some(statistics) = statistics {
        record(statistics);
    }
    let mut written = vec![out.to_path_buf()];
    written.extend(directory);
    Ok((written, diagnostics))
}

/// Generates a default build's exact TeX without resource reads or renditions.
pub fn dump(
    prepared: &Prepared, opts: &BuildOptions,
) -> Result<(String, Diagnostics)> {
    dump_detailed(prepared, opts).map_err(|failure| *failure.error)
}

/// Lets the CLI report completed-emission findings before a template failure.
pub(crate) fn dump_detailed(
    prepared: &Prepared, opts: &BuildOptions,
) -> Detailed<(String, Diagnostics)> {
    let out = super::output_path(prepared, opts, Target::Latex, 1);
    let output = latex::render_detailed(
        &prepared.document,
        &prepared.assets,
        &prepared.config,
        output_stem(&out)?,
    )
    .map_err(|failure| BuildFailure {
        error: failure.error,
        diagnostics: failure.diagnostics,
    })?;
    Ok((output.tex, output.diagnostics))
}

fn output_stem(out: &Path) -> Result<&str> {
    out.file_stem().and_then(|name| name.to_str()).ok_or_else(|| {
        Error::Config(format!(
            "LaTeX export needs an output file with a UTF-8 stem: {}",
            out.display(),
        ))
    })
}

fn diagnostic_error(finding: &Diagnostic, prepared: &Prepared) -> Error {
    Error::Parse {
        path: finding
            .path
            .clone()
            .unwrap_or_else(|| prepared.document.source.path.clone()),
        line: finding.span.as_ref().map(|span| span.start_line),
        message: finding.message.clone(),
    }
}

fn failure(error: Error, diagnostics: &Diagnostics) -> BuildFailure {
    BuildFailure { error: Box::new(error), diagnostics: diagnostics.clone() }
}
