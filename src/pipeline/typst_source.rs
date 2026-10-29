//! Native Typst PDF output through the retained publishing session.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use super::{BuildFailure, BuildOptions, InputRole, Prepared};
use crate::error::{Diagnostic, Diagnostics, Error, Result};
use crate::typst::compile::{self, Compiled, Pdf, PdfOptions};
use crate::typst::world::WriterWorld;

type Detailed<T> = std::result::Result<T, BuildFailure>;

/// Compiles the caller's original native main and writes its PDF.
pub fn build(
    prepared: &Prepared, opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Result<(Vec<PathBuf>, Diagnostics)> {
    build_detailed(prepared, opts, out, world).map_err(|failure| *failure.error)
}

/// Keeps every compiler finding on failure and protects dynamically read files.
pub(crate) fn build_detailed(
    prepared: &Prepared, _opts: &BuildOptions, out: &Path,
    world: &mut WriterWorld,
) -> Detailed<(Vec<PathBuf>, Diagnostics)> {
    let options =
        PdfOptions { timestamp: world.session_timestamp(), ident: None };
    // Native main, inputs, resources and reset belong to the session. In
    // particular, no generated-main loader or per-target World is needed here.
    let mut compiled = compile::compile_pdf(world, &options);
    if !prepared.config.pdf.template.as_os_str().is_empty() {
        let mut findings = Diagnostics::new();
        findings.push(Diagnostic::warning(
            "W-TYP-TEMPLATE-IGNORED",
            "[pdf] template is ignored for native Typst input; apply the template in the source",
        ).with_path(&prepared.document.source.path));
        findings.extend(compiled.diagnostics);
        compiled.diagnostics = findings;
    }
    let (pdf, diagnostics) =
        compiler_result(compiled, &prepared.document.source.path)?;
    let inputs = consumed_inputs(prepared, world.accessed());
    if let Some(clash) = super::find_clash([out], &inputs) {
        return Err(BuildFailure {
            error: Box::new(Error::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                clash,
            ))),
            diagnostics,
        });
    }
    super::write_file(out, &pdf.bytes).map_err(|error| BuildFailure {
        error: Box::new(error),
        diagnostics: diagnostics.clone(),
    })?;
    Ok((vec![out.to_path_buf()], diagnostics))
}

fn compiler_result(
    compiled: Compiled<Pdf>, source: &Path,
) -> Detailed<(Pdf, Diagnostics)> {
    let Compiled { output, diagnostics: found } = compiled;
    let diagnostics: Diagnostics = found
        .into_iter()
        .map(|mut finding| {
            finding.path = finding
                .span
                .as_ref()
                .and_then(|span| span.file.clone())
                .or(finding.path)
                .or_else(|| Some(source.to_path_buf()));
            finding
        })
        .collect();
    Compiled { output, diagnostics: diagnostics.clone() }
        .into_result()
        .map_err(|error| BuildFailure { error: Box::new(error), diagnostics })
}

fn consumed_inputs(
    prepared: &Prepared, accessed: BTreeSet<PathBuf>,
) -> Vec<(PathBuf, InputRole)> {
    let mut inputs = super::input_files(prepared);
    let mut known: BTreeSet<_> =
        inputs.iter().map(|(path, _)| path.clone()).collect();
    for path in accessed {
        if known.insert(path.clone()) {
            let role = if crate::ir::detect_kind(&path).is_ok() {
                InputRole::Include
            } else {
                InputRole::Asset
            };
            inputs.push((path, role));
        }
    }
    inputs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::error::Diagnostic;
    use crate::ir::{
        DocMeta, Document, Features, Resolved, SourceInfo, SourceKind,
        SourceSpan,
    };

    #[test]
    fn failed_native_compilation_retains_every_location_and_hint() {
        let first = Diagnostic::error(
            "typst-error",
            "first native failure\n  hint: keep this hint",
        )
        .with_path("/native/include.typ")
        .with_span(SourceSpan {
            start_line: 4,
            end_line: 4,
            cell: None,
            file: None,
        });
        let second = Diagnostic::error("typst-error", "second native failure")
            .with_path("/native/main.typ")
            .with_span(SourceSpan {
                start_line: 8,
                end_line: 8,
                cell: None,
                file: None,
            });
        let failure = compiler_result(
            Compiled {
                output: None,
                diagnostics: vec![first.clone(), second.clone()].into(),
            },
            Path::new("/native/main.typ"),
        )
        .unwrap_err();
        assert_eq!(failure.diagnostics.into_vec(), [first, second]);
        let Error::Compile { path, line, message, hints } = *failure.error
        else {
            panic!("expected the original native compiler error");
        };
        assert_eq!(path, Path::new("/native/include.typ"));
        assert_eq!(line, Some(4));
        assert_eq!(message, "first native failure");
        assert_eq!(hints, ["keep this hint"]);
    }

    #[test]
    fn consumed_reads_keep_prepared_roles_and_classify_only_new_paths() {
        let root = tempfile::tempdir().unwrap();
        let path = |name: &str| root.path().join(name);
        let mut config = Config::default();
        config.references.bibliography = vec![path("refs.bib")];
        let prepared = Prepared {
            document: Document {
                meta: DocMeta::default(),
                blocks: Vec::new(),
                features: Features::default(),
                source: SourceInfo {
                    path: path("main.typ"),
                    kind: SourceKind::Typst,
                    base_dir: root.path().into(),
                },
                assets: Vec::new(),
                includes: Vec::new(),
                resolved: Resolved::default(),
            },
            assets: Default::default(),
            config,
            config_path: None,
            diagnostics: Diagnostics::default(),
        };
        let inputs = consumed_inputs(
            &prepared,
            [
                path("main.typ"),
                path("refs.bib"),
                path("computed.typ"),
                path("computed.png"),
                path("font.otf"),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(
            inputs.iter().filter(|(p, _)| p == &path("main.typ")).count(),
            1
        );
        assert_eq!(
            inputs.iter().filter(|(p, _)| p == &path("refs.bib")).count(),
            1
        );
        for (name, role) in [
            ("main.typ", InputRole::Source),
            ("refs.bib", InputRole::Bibliography),
            ("computed.typ", InputRole::Include),
            ("computed.png", InputRole::Asset),
            ("font.otf", InputRole::Asset),
        ] {
            assert!(inputs.contains(&(path(name), role)), "missing {name}");
        }
    }
}
