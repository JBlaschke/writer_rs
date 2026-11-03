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
        if seen.insert(&font.family)
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
/// Explains which declared design settings a Word reference document wins over.
/// Invalid reference packages are reported by the DOCX target adapter itself.
pub fn docx_shadowed(
    design: &Design, path: &std::path::Path,
) -> Vec<Diagnostic> {
    if !design.declared || path.as_os_str().is_empty() {
        return Vec::new();
    }
    let Ok(reference) = crate::docx::reference::Reference::load(path) else {
        return Vec::new();
    };
    let mut parts = Vec::new();
    if reference.sect_pr.is_some() && design.page != Default::default() {
        parts.push("page setup".to_string());
        if design.page.header.is_some() || design.page.footer.is_some() {
            parts.push("header/footer".to_string());
        }
    }
    let Ok(tree) = crate::docx::validate::parse_xml(&reference.styles_xml)
    else {
        return Vec::new();
    };
    let tokens = design.fonts != Default::default()
        || design.text != Default::default()
        || design.colors.text.is_some()
        || design.colors.link.is_some();
    if tokens && tree.children.iter().any(|node| node.name == "w:docDefaults") {
        parts.push("document defaults".to_string());
    }
    let mut names = Vec::new();
    for name in design.styles.keys() {
        if tree
            .children
            .iter()
            .filter(|node| node.name == "w:style")
            .flat_map(|node| &node.children)
            .any(|node| {
                node.name == "w:name"
                    && node.attr("w:val").is_some_and(|reference| {
                        reference.eq_ignore_ascii_case(name)
                    })
            })
        {
            names.push(name.as_str());
        }
    }
    if !names.is_empty() {
        parts.push(format!("styles: {}", names.join(", ")));
    }
    if parts.is_empty() {
        return Vec::new();
    }
    vec![
        Diagnostic::info(
            "design-shadowed",
            format!(
                "Word reference document overrides design {}",
                parts.join("; ")
            ),
        )
        .with_path(path),
    ]
}

/// Reports declared styles which no div in the resolved document selects.
///
/// Names writer's own paragraphs select are exempt: the Word vocabulary's
/// paragraph names and the header furniture's `Header`.
pub fn unused_styles(
    design: &Design, document: &crate::ir::Document,
) -> Vec<Diagnostic> {
    if document.source.kind == crate::ir::SourceKind::Typst {
        return Vec::new();
    }
    let mut selected = std::collections::BTreeSet::new();
    crate::ir::walk_blocks(document, &mut |block| {
        if let crate::ir::Block::Div { attrs, .. } = block
            && let Some(name) = crate::design::style_for(design, attrs)
        {
            selected.insert(name.to_owned());
        }
    });
    design
        .styles
        .keys()
        .filter(|name| {
            !selected.contains(*name)
                && !crate::docx::styles::writer_selects(name)
        })
        .map(|name| {
            Diagnostic::warning(
                "design-style-unused",
                format!("design style {name:?} is not selected by any div"),
            )
        })
        .collect()
}
