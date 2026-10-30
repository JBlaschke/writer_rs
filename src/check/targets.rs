//! Check-time target adapters. Generated strings, packages and assets are dropped.

use std::path::Path;

use crate::config::{HtmlMath, Target};
use crate::error::{Diagnostic, Diagnostics, Error};
use crate::html::emit::{MathHook, MathRequest};
use crate::ir::{Block, Inline, MathDialect, RefForm, SourceKind};
use crate::pipeline::{self, BuildFailure, BuildOptions, Prepared, WorldSlot};
use crate::typst::world::WriterWorld;

use super::{MathLocation, error_diagnostic, visible};

pub(super) fn run(prepared: &Prepared, opts: &BuildOptions) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut selected = Vec::new();
    let mut world = CheckWorld::default();
    for target in prepared.config.targets_for(prepared.document.source.kind) {
        if selected.contains(&target) {
            continue;
        }
        selected.push(target);
        let inspect = match target {
            Target::Latex => latex(prepared, &mut diagnostics),
            Target::Html => {
                html(prepared, opts, &mut world, &mut diagnostics);
                true
            }
            Target::Docx => {
                docx(prepared, opts, &mut world, &mut diagnostics);
                true
            }
            Target::Pdf | Target::Typst => true,
        };
        if inspect {
            structural(prepared, target, &mut diagnostics);
        }
    }
    diagnostics
}

/// One lazy World shared only by adapters that actually render equations/images.
#[derive(Default)]
struct CheckWorld {
    slot: WorldSlot,
    attempted: bool,
}

impl CheckWorld {
    fn get(
        &mut self, prepared: &Prepared, opts: &BuildOptions,
    ) -> std::result::Result<Option<&mut WriterWorld>, BuildFailure> {
        if !self.attempted {
            self.attempted = true;
            self.slot.get(Some(prepared), opts)?.reset();
        }
        Ok(self.slot.current())
    }
}

fn latex(prepared: &Prepared, diagnostics: &mut Vec<Diagnostic>) -> bool {
    let stem = prepared
        .document
        .source
        .path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("document");
    match crate::latex::render_detailed(
        &prepared.document, &prepared.assets, &prepared.config, stem,
    ) {
        Ok(output) => {
            let slots_valid = !output
                .diagnostics
                .iter()
                .any(|finding| finding.code == "latex-template-slot-missing");
            diagnostics.extend(output.diagnostics);
            slots_valid
        }
        Err(failure) => {
            diagnostics.extend(failure.diagnostics);
            diagnostics.push(latex_error(
                &failure.error, &prepared.document.source.path,
            ));
            false
        }
    }
}

fn latex_error(error: &Error, input: &Path) -> Diagnostic {
    let mut finding = error_diagnostic(error, input);
    let path = match error {
        Error::Template { template, .. }
            if !template.is_empty() && template != "<unknown>" =>
        {
            Some(Path::new(template))
        }
        Error::Resource { what: "LaTeX template", path, .. } => {
            Some(path.as_path())
        }
        _ => None,
    };
    if let Some(path) = path {
        finding.path =
            Some(std::path::absolute(path).unwrap_or_else(|_| path.to_owned()));
    }
    finding
}

fn html(
    prepared: &Prepared, opts: &BuildOptions, world: &mut CheckWorld,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match pipeline::resolve_theme(prepared, opts) {
        Ok(theme) => diagnostics.extend(theme.problems()),
        Err(error) => diagnostics
            .push(error_diagnostic(&error, &prepared.document.source.path)),
    }
    let mut equations = Vec::new();
    visible::walk(
        &prepared.document,
        Target::Html,
        &prepared.config,
        |node, location| {
            let (source, dialect, display) = match node {
                visible::Node::Block(Block::Math {
                    source, dialect, ..
                }) => (source, *dialect, true),
                visible::Node::Inline(Inline::Math { source, dialect }) => {
                    (source, *dialect, false)
                }
                _ => return,
            };
            if dialect == MathDialect::Typst
                || prepared.config.math.html == HtmlMath::Image
            {
                equations.push((source, dialect, display, location.clone()));
            }
        },
    );
    if equations.is_empty() {
        return;
    }
    let world = match world.get(prepared, opts) {
        Ok(Some(world)) => world,
        Ok(None) => return,
        Err(failure) => {
            diagnostics.extend(failure.diagnostics);
            diagnostics.push(error_diagnostic(
                &failure.error, &prepared.document.source.path,
            ));
            return;
        }
    };
    let mut renderer =
        crate::html::math::MathRenderer::new(&prepared.config, world);
    let mut assets = prepared.assets.clone();
    for (source, dialect, display, location) in equations {
        let request = MathRequest {
            source,
            dialect,
            display,
            span: location.span.as_ref(),
        };
        let mut findings = Diagnostics::new();
        renderer.equation(&request, &mut assets, &mut findings);
        diagnostics.extend(
            findings
                .into_iter()
                .filter(|finding| !common_math(finding))
                .map(|finding| locate(finding, &location)),
        );
    }
}

fn docx(
    prepared: &Prepared, opts: &BuildOptions, world: &mut CheckWorld,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let world = match world.get(prepared, opts) {
        Ok(Some(world)) => world,
        Ok(None) => return,
        Err(failure) => {
            diagnostics.extend(failure.diagnostics);
            diagnostics.push(error_diagnostic(
                &failure.error, &prepared.document.source.path,
            ));
            return;
        }
    };
    let reference = &prepared.config.docx.reference_doc;
    let options = crate::docx::DocxOptions {
        config: &prepared.config,
        reference: (!reference.as_os_str().is_empty())
            .then_some(reference.as_path()),
        source_date_epoch: std::env::var("SOURCE_DATE_EPOCH").ok(),
        all_styles: false,
    };
    let mut renderer =
        pipeline::docx::WorldRenderer::new(&prepared.config, world);
    match crate::docx::write(
        &prepared.document, &prepared.assets, &mut renderer, &options,
    ) {
        Ok(output) => {
            let root = MathLocation {
                path: prepared.document.source.path.clone(),
                span: None,
            };
            diagnostics.extend(
                output
                    .diagnostics
                    .into_iter()
                    .filter(|finding| !common_math(finding))
                    .map(|finding| locate(finding, &root)),
            );
        }
        Err(error) => {
            if let Some(finding) = docx_failure(prepared, &error) {
                diagnostics.push(finding);
            }
        }
    }
}

// T33 owns these exact math-core/source-preparation findings. The LaTeX
// adapter deliberately does not use this filter: its macro grammar differs,
// and metadata-only math and native-Typst conversion findings are distinct.
fn common_math(finding: &Diagnostic) -> bool {
    matches!(
        finding.code,
        "math-syntax" | "math-macro-invalid" | "math-tag-ignored"
    )
}

fn locate(mut finding: Diagnostic, inherited: &MathLocation) -> Diagnostic {
    let location = inherited.nested(finding.span.as_ref());
    if finding.path.is_none() {
        finding.path = Some(location.path);
    }
    if finding.span.is_none() {
        finding.span = location.span;
    }
    finding
}

fn docx_failure(prepared: &Prepared, error: &Error) -> Option<Diagnostic> {
    if let Error::MissingAsset { path, span } = error {
        let known = prepared.diagnostics.iter().any(|finding| {
            matches!(pipeline::missing_asset_as_error(finding),
                Some(Error::MissingAsset { path: known, span: known_span })
                    if known == *path && known_span == *span)
        });
        if known {
            return None;
        }
        let root = MathLocation {
            path: prepared.document.source.path.clone(),
            span: None,
        };
        let location = root.nested(span.as_ref());
        return Some(
            location.attach(Diagnostic::warning(
                "missing-asset",
                error.to_string(),
            )),
        );
    }
    Some(error_diagnostic(error, &prepared.document.source.path))
}

fn structural(
    prepared: &Prepared, target: Target, diagnostics: &mut Vec<Diagnostic>,
) {
    if target == Target::Pdf
        && prepared.document.source.kind == SourceKind::Typst
        && !prepared.config.pdf.template.as_os_str().is_empty()
    {
        diagnostics.push(Diagnostic::warning(
            "W-TYP-TEMPLATE-IGNORED",
            "[pdf] template is ignored for native Typst input; apply the template in the source",
        ).with_path(prepared.document.source.path.clone()));
    }
    visible::walk(
        &prepared.document,
        target,
        &prepared.config,
        |node, location| {
            let raw = match node {
                visible::Node::Block(Block::Raw { format, .. })
                | visible::Node::Inline(Inline::Raw { format, .. }) => {
                    Some(format.as_str())
                }
                _ => None,
            };
            if let Some(format) = raw
                && format != raw_format(target)
            {
                diagnostics.push(location.attach(Diagnostic::info(
                    "raw-skipped",
                    format!("{}: skipped raw {format} content", name(target)),
                )));
            }
            match node {
                visible::Node::Inline(Inline::Ref {
                    form: RefForm::Page,
                    ..
                }) if target == Target::Html => {
                    diagnostics.push(location.attach(Diagnostic::warning(
                        "unsupported-construct",
                        "html: page references are unsupported; used a normal reference",
                    )));
                }
                visible::Node::Inline(Inline::Image { url, .. })
                    if matches!(target, Target::Pdf | Target::Typst) =>
                {
                    typst_image(prepared, target, url, location, diagnostics);
                }
                visible::Node::Block(Block::Bibliography { .. })
                    if matches!(target, Target::Pdf | Target::Typst)
                        && prepared
                            .config
                            .references
                            .bibliography
                            .is_empty() =>
                {
                    diagnostics.push(location.attach(Diagnostic::warning(
                        "unsupported-construct",
                        format!(
                            "{}: bibliography has no sources",
                            name(target)
                        ),
                    )));
                }
                visible::Node::Block(Block::Math {
                    source,
                    dialect: MathDialect::Latex,
                    attrs,
                    ..
                }) if target == Target::Latex
                    && attrs.id.is_some()
                    && multiline_environment(source) =>
                {
                    diagnostics.push(location.attach(Diagnostic::warning(
                        "math-env-label",
                        "\\label applies to the first line of the align environment",
                    )));
                }
                _ => {}
            }
        },
    );
}

fn typst_image(
    prepared: &Prepared, target: Target, url: &str, location: &MathLocation,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let message = if let Some(id) = url.strip_prefix("asset:") {
        let id = id.split(['?', '#']).next().unwrap_or(id);
        match prepared.assets.get(id) {
            Some(asset) => {
                if crate::typst::render::image_extension(&asset.mime).is_ok() {
                    return;
                }
                format!(
                    "{}: unsupported image MIME type: {}",
                    name(target),
                    asset.mime
                )
            }
            None => format!(
                "{}: unregistered asset placeholder `{url}`",
                name(target),
            ),
        }
    } else {
        format!(
            "{}: cannot fetch image `{url}`; used a link to its source",
            name(target)
        )
    };
    diagnostics.push(
        location.attach(Diagnostic::warning("unsupported-construct", message)),
    );
}

fn multiline_environment(source: &str) -> bool {
    let Some(body) = source.trim_start().strip_prefix("\\begin{") else {
        return false;
    };
    let Some((written, _)) = body.split_once('}') else {
        return false;
    };
    let name = written.strip_suffix('*').unwrap_or(written);
    matches!(name, "align" | "gather" | "multline" | "equation" | "eqnarray")
        && source.contains("\\\\")
}

fn raw_format(target: Target) -> &'static str {
    match target {
        Target::Html => "html",
        Target::Docx => "openxml",
        Target::Latex => "latex",
        Target::Pdf | Target::Typst => "typst",
    }
}

fn name(target: Target) -> &'static str {
    match target {
        Target::Html => "html",
        Target::Pdf => "pdf",
        Target::Docx => "docx",
        Target::Latex => "latex",
        Target::Typst => "typst",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::error::Level;
    use crate::ir::{Attrs, SourceSpan};

    fn prepared() -> (tempfile::TempDir, Prepared, BuildOptions) {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("document.md");
        std::fs::write(&input, "Text.\n").unwrap();
        let mut config = Config::default();
        config.build.targets = Vec::new();
        config.pdf.system_fonts = false;
        let path = directory.path().join("writer.toml");
        std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        let mut opts = BuildOptions::new(input);
        opts.config_path = Some(path);
        opts.mode = pipeline::Mode::Check;
        let prepared = pipeline::prepare(&opts).unwrap();
        (directory, prepared, opts)
    }

    #[test]
    fn raw_losses_are_per_selected_target_and_repeated_targets_run_once() {
        let (_directory, mut prepared, opts) = prepared();
        prepared.config.build.targets = vec![
            Target::Html,
            Target::Pdf,
            Target::Typst,
            Target::Latex,
            Target::Html,
        ];
        prepared.document.blocks = vec![
            Block::Raw {
                format: "html".into(),
                text: "<p>raw</p>".into(),
                span: None,
            },
            Block::Paragraph {
                inlines: vec![Inline::Raw {
                    format: "typst".into(),
                    text: "#linebreak()".into(),
                }],
                span: None,
            },
        ];
        let findings = run(&prepared, &opts);
        let raw: Vec<_> = findings
            .iter()
            .filter(|finding| finding.code == "raw-skipped")
            .collect();
        assert_eq!(raw.len(), 5, "{findings:?}");
        for target in ["html", "pdf", "typst"] {
            assert_eq!(
                raw.iter()
                    .filter(|finding| {
                        finding.message.starts_with(&format!("{target}:"))
                    })
                    .count(),
                1,
                "{findings:?}"
            );
        }
        assert!(raw.iter().all(|finding| finding.level == Level::Info));
    }

    #[test]
    fn missing_slots_stop_latex_content_checks_before_emission() {
        let (directory, mut prepared, opts) = prepared();
        let template = directory.path().join("missing-body.tex");
        std::fs::write(&template, "\\VAR{preamble}").unwrap();
        prepared.config.latex.template = template;
        prepared.config.build.targets = vec![Target::Latex];
        prepared.document.blocks = vec![Block::Raw {
            format: "html".into(),
            text: "<b>ignored</b>".into(),
            span: None,
        }];
        let findings = run(&prepared, &opts);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, "latex-template-slot-missing");
        assert_eq!(findings[0].level, Level::Error);
    }

    #[test]
    fn exact_known_docx_missing_asset_keeps_the_original_warning_once() {
        let (directory, mut prepared, _opts) = prepared();
        let path = directory.path().join("lost.png");
        let span = SourceSpan {
            start_line: 4,
            end_line: 4,
            cell: Some(3),
            file: Some(directory.path().join("included.ipynb")),
        };
        let mut warning = Diagnostic::warning(
            "missing-asset",
            format!(
                "image `lost.png` not found (looked for {})",
                path.display(),
            ),
        );
        warning.path = span.file.clone();
        warning.span = Some(span.clone());
        prepared.diagnostics.push(warning.clone());
        assert!(
            docx_failure(
                &prepared,
                &Error::MissingAsset { path, span: Some(span) }
            )
            .is_none()
        );
        assert_eq!(prepared.diagnostics.into_vec(), vec![warning]);
    }

    #[test]
    fn disappearing_docx_asset_is_a_new_located_warning() {
        let (directory, mut prepared, opts) = prepared();
        let path = directory.path().join("plot.png");
        std::fs::write(
            &path,
            include_bytes!("../../tests/fixtures/md/images/plot.png"),
        )
        .unwrap();
        let id = prepared.assets.register_file(&path);
        let span = SourceSpan {
            start_line: 8,
            end_line: 8,
            cell: Some(2),
            file: Some(directory.path().join("included.ipynb")),
        };
        prepared.document.blocks = vec![Block::Paragraph {
            inlines: vec![Inline::Image {
                alt: vec![],
                url: format!("asset:{id}"),
                title: None,
                attrs: Attrs::default(),
            }],
            span: Some(span.clone()),
        }];
        std::fs::remove_file(path).unwrap();
        let mut findings = Vec::new();
        docx(&prepared, &opts, &mut CheckWorld::default(), &mut findings);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, "missing-asset");
        assert_eq!(findings[0].level, Level::Warning);
        assert_eq!(findings[0].path, span.file);
        assert_eq!(findings[0].span, Some(span));
    }

    #[test]
    fn bare_environment_lint_accepts_one_star_and_real_row_separator() {
        for environment in
            ["align", "gather", "multline", "equation", "eqnarray"]
        {
            for star in ["", "*"] {
                assert!(multiline_environment(&format!(
                    " \n\\begin{{{environment}{star}}}a\\\\b\\end{{{environment}{star}}}",
                )));
            }
        }
        for source in [
            "\\begin{align}x\\end{align}",
            "\\begin{align**}a\\\\b\\end{align**}",
            "prefix \\begin{align}a\\\\b\\end{align}",
            "% comment\n\\begin{align}a\\\\b\\end{align}",
        ] {
            assert!(!multiline_environment(source), "{source}");
        }
    }

    #[test]
    fn cheap_targets_do_not_read_supported_image_bytes() {
        let (directory, mut prepared, opts) = prepared();
        prepared.config.build.targets = vec![Target::Pdf, Target::Typst];
        let missing = directory.path().join("never-read.png");
        let id = prepared.assets.register_file(&missing);
        prepared.document.blocks = vec![Block::Paragraph {
            inlines: vec![Inline::Image {
                alt: vec![],
                url: format!("asset:{id}"),
                title: None,
                attrs: Attrs::default(),
            }],
            span: None,
        }];
        let mut findings = Vec::new();
        for target in &prepared.config.build.targets {
            structural(&prepared, *target, &mut findings);
        }
        assert!(findings.is_empty(), "{findings:?}");
        assert!(run(&prepared, &opts).is_empty());
    }

    #[test]
    fn failed_world_creation_is_reported_once_without_a_second_attempt() {
        let (_directory, mut prepared, opts) = prepared();
        prepared.config.pdf.timestamp = "invalid timestamp".into();
        let mut world = CheckWorld::default();
        assert!(world.get(&prepared, &opts).is_err());
        // Even repaired settings cannot trigger compiler work again during
        // this one check invocation; its first global error is retained.
        prepared.config.pdf.timestamp.clear();
        assert!(world.get(&prepared, &opts).unwrap().is_none());
    }
}
