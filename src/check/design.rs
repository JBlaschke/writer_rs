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
