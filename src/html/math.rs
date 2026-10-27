//! HTML equation routes, shared preparation and per-equation fallbacks.

use std::collections::BTreeMap;

use crate::assets::registry::AssetRegistry;
use crate::config::{Config, HtmlMath};
use crate::error::{Diagnostic, Diagnostics, Error};
use crate::html::emit::{MathHook, MathRequest, MathUsage, escape};
use crate::ir::MathDialect;
use crate::math::{latex_mathml, latex_source};
use crate::typst::world::WriterWorld;

/// Renders a document's equations, borrowing its long-lived compiler world.
pub struct MathRenderer<'a> {
    config: &'a Config,
    world: &'a mut WriterWorld,
    macros: BTreeMap<String, String>,
    usage: MathUsage,
    images: usize,
    macro_diags_done: bool,
}

impl<'a> MathRenderer<'a> {
    /// Creates a renderer with the document's effective math configuration.
    pub fn new(config: &'a Config, world: &'a mut WriterWorld) -> Self {
        Self {
            config,
            world,
            macros: latex_source::macros_from_config(
                &config.math.katex_options,
            ),
            usage: MathUsage::default(),
            images: 0,
            macro_diags_done: false,
        }
    }

    fn latex(
        &mut self, eq: &MathRequest<'_>, prepared: &str,
        assets: &mut AssetRegistry, diagnostics: &mut Diagnostics,
    ) -> String {
        match self.config.math.html {
            HtmlMath::Katex => {
                self.usage.katex = true;
                escape(prepared)
            }
            HtmlMath::Mathml => self.latex_mathml(eq, prepared, diagnostics),
            HtmlMath::Image => {
                let text = latex_source::for_typst(eq.source, &self.macros).0;
                self.image(eq, &text, assets, diagnostics, None)
            }
        }
    }

    fn latex_mathml(
        &mut self, eq: &MathRequest<'_>, prepared: &str,
        diagnostics: &mut Diagnostics,
    ) -> String {
        if !self.macro_diags_done {
            for mut finding in latex_mathml::macro_diagnostics(&self.macros) {
                finding.span = eq.span.cloned();
                finding.path = eq.span.and_then(|span| span.file.clone());
                diagnostics.push(finding);
            }
            self.macro_diags_done = true;
        }
        match latex_mathml::to_mathml(prepared, eq.display, &self.macros) {
            Ok(mathml) => {
                self.usage.mathml = true;
                mathml
            }
            Err(latex_mathml::MathError::Syntax { message, range, html }) => {
                let position = range.map_or_else(String::new, |range| {
                    format!(
                        " at bytes {}..{} of the equation",
                        range.start, range.end
                    )
                });
                warning(
                    diagnostics,
                    eq,
                    "math-syntax",
                    format!("invalid LaTeX math: {message}{position}"),
                );
                html
            }
            Err(error) => {
                warning(
                    diagnostics,
                    eq,
                    "math-syntax",
                    format!("invalid LaTeX math: {error}"),
                );
                format!(
                    "<code class=\"math-error\">{}</code>",
                    escape(eq.source)
                )
            }
        }
    }

    fn typst(
        &mut self, eq: &MathRequest<'_>, assets: &mut AssetRegistry,
        diagnostics: &mut Diagnostics,
    ) -> String {
        if self.config.math.html == HtmlMath::Image {
            return self.image(eq, eq.source, assets, diagnostics, None);
        }
        match crate::typst::html::equation_mathml(
            self.world, eq.source, eq.display,
        ) {
            Ok(mathml) => {
                self.usage.typst_mathml = true;
                mathml
            }
            Err(error) => self.image(
                eq,
                eq.source,
                assets,
                diagnostics,
                Some(reason(error)),
            ),
        }
    }

    fn image(
        &mut self, eq: &MathRequest<'_>, source: &str,
        assets: &mut AssetRegistry, diagnostics: &mut Diagnostics,
        previous: Option<String>,
    ) -> String {
        match crate::typst::render::equation(
            self.world, source, eq.dialect, eq.display,
        ) {
            Ok(rendition) => {
                if let Some(reason) = previous {
                    conversion_warning(diagnostics, eq, &reason, "an image");
                }
                self.images += 1;
                self.usage.image = true;
                let name = format!("eq-{}.svg", self.images);
                let id = assets
                    .register_bytes(rendition.svg, "image/svg+xml", &name);
                let height = decimal(rendition.height_pt * 12.0 / 11.0);
                let descent = decimal(
                    (rendition.height_pt - rendition.baseline_pt) * 12.0 / 11.0,
                );
                format!(
                    "<img src=\"asset:{id}\" alt=\"{}\" style=\"height:{height}pt;vertical-align:-{descent}pt\">",
                    escape(eq.source)
                )
            }
            Err(error) => {
                let current = reason(error);
                let reason = previous.map_or_else(
                    || current.clone(),
                    |previous| format!("{previous}; image: {current}"),
                );
                conversion_warning(diagnostics, eq, &reason, "the source text");
                format!(
                    "<code class=\"math-error\">{}</code>",
                    escape(eq.source)
                )
            }
        }
    }
}

impl MathHook for MathRenderer<'_> {
    fn equation(
        &mut self, eq: &MathRequest<'_>, assets: &mut AssetRegistry,
        diagnostics: &mut Diagnostics,
    ) -> String {
        let (prepared, removed) = if eq.dialect == MathDialect::Latex {
            latex_source::prepare(eq.source)
        } else {
            (eq.source.to_owned(), false)
        };
        if removed {
            warning(
                diagnostics,
                eq,
                "math-tag-ignored",
                "\\tag/\\label inside math are ignored for html; label the equation with {#eq-x}",
            );
        }
        let content = match eq.dialect {
            MathDialect::Latex => {
                self.latex(eq, &prepared, assets, diagnostics)
            }
            MathDialect::Typst => self.typst(eq, assets, diagnostics),
        };
        let class = if eq.display { "display" } else { "inline" };
        let style = if eq.dialect == MathDialect::Latex
            && self.config.math.html == HtmlMath::Katex
        {
            format!(" data-math-style=\"{class}\"")
        } else {
            String::new()
        };
        format!(
            "<span class=\"math {class}\"{style} data-tex=\"{}\">{content}</span>",
            escape(&prepared)
        )
    }

    fn usage(&self) -> MathUsage {
        self.usage
    }
}

fn warning(
    diagnostics: &mut Diagnostics, eq: &MathRequest<'_>, code: &'static str,
    message: impl Into<String>,
) {
    let mut finding = Diagnostic::warning(code, message);
    finding.span = eq.span.cloned();
    finding.path = eq.span.and_then(|span| span.file.clone());
    diagnostics.push(finding);
}

fn conversion_warning(
    diagnostics: &mut Diagnostics, eq: &MathRequest<'_>, reason: &str,
    fallback: &str,
) {
    warning(
        diagnostics,
        eq,
        "math-conversion-failed",
        format!("html: cannot convert equation ({reason}); used {fallback}"),
    );
}

fn reason(error: Error) -> String {
    match error {
        Error::Compile { message, .. } => message,
        other => other.to_string(),
    }
}

fn decimal(value: f64) -> String {
    let value = format!("{value:.2}");
    value.trim_end_matches('0').trim_end_matches('.').to_owned()
}
