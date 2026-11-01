//! Findings for design resources shared by output targets.

use crate::config::Target;
use crate::design::Design;
use crate::error::Diagnostic;

/// Reports missing fonts and furniture images before backend rendering.
pub fn problems(design: &Design, _targets: &[Target]) -> Vec<Diagnostic> {
    let mut findings = Vec::new();
    let mut fonts = std::collections::BTreeSet::new();
    for font in [&design.fonts.body, &design.fonts.heading, &design.fonts.mono]
        .into_iter()
        .flatten()
    {
        fonts.extend(font.files.iter().cloned());
    }
    for path in design.referenced_files() {
        if path.is_file() {
            continue;
        }
        let (code, kind) = if fonts.contains(&path) {
            ("design-font-file", "font file")
        } else {
            ("design-image-missing", "image")
        };
        findings.push(
            Diagnostic::error(
                code,
                format!("design {kind} not found: {}", path.display()),
            )
            .with_path(path),
        );
    }
    findings
}

/// Checks PDF font availability and templates that replace the design layout.
pub fn pdf_problems(
    prepared: &crate::pipeline::Prepared,
    world: &crate::typst::world::WriterWorld,
) -> Vec<Diagnostic> {
    use typst::World;
    if !prepared.design.declared
        || prepared.document.source.kind == crate::ir::SourceKind::Typst
    {
        return Vec::new();
    }
    let mut findings = Vec::new();
    let fonts = [
        &prepared.design.fonts.body, &prepared.design.fonts.heading,
        &prepared.design.fonts.mono,
    ];
    let mut seen = std::collections::BTreeSet::new();
    for font in fonts.into_iter().flatten() {
        if font.files.is_empty()
            && seen.insert(&font.family)
            && !world.book().contains_family(&font.family.to_lowercase())
        {
            findings.push(Diagnostic::warning(
                "design-font-family",
                format!(
                    "design font family not available for PDF: {}",
                    font.family
                ),
            ));
        }
    }
    let template = &prepared.config.pdf.template;
    if !template.as_os_str().is_empty()
        && let Ok(text) = std::fs::read_to_string(template)
        && !imports_writer_doc(&typst_syntax::parse(&text))
    {
        findings.push(Diagnostic::info(
            "design-shadowed",
            "PDF template does not import @writer/doc; it controls the design layout",
        ).with_path(template));
    }
    findings
}

fn imports_writer_doc(node: &typst_syntax::SyntaxNode) -> bool {
    use typst_syntax::ast;
    if let Some(import) = node.cast::<ast::ModuleImport<'_>>()
        && let ast::Expr::Str(source) = import.source()
        && source.get().starts_with("@writer/doc:")
    {
        return true;
    }
    node.children().any(imports_writer_doc)
}
