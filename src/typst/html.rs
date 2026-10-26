//! Compiler-backed equation MathML output (typst-dialect.md §9; math.md §3.5).
//! Owner: T03. Stub created by T00.

use typst_html::HtmlDocument;

use crate::error::{Error, Result};
use crate::typst::compile::{Compiled, map_diagnostic};
use crate::typst::world::WriterWorld;

/// Styles required to align Typst's exported MathML as in its paged output.
// Expanded from typst-html 0.15.1, src/mathml.rs:99.
// Copyright the Typst project developers; licensed under Apache-2.0.
pub const EQUATION_CSS: &str = r#"/* Alignment */
mtable.right-align mtd,
mtable mtd.right-align,
mtable.left-align mtd.right-align,
mtable.aligned mtd:nth-child(odd) {
  justify-items: end;
  text-align: right;
}
mtable.cases mtd,
mtable.left-align mtd,
mtable mtd.left-align,
mtable.aligned mtd:nth-child(even),
math:is(:not([display])) > mtable.multiline-equation mtd {
  justify-items: start;
  text-align: left;
}
mtable.cases mtd,
mtable.aligned mtd,
mtable mtd.flushed,
mtable mtd.left-flush {
  padding-left: 0;
}
mtable.cases mtd,
mtable.aligned mtd,
mtable mtd.flushed,
mtable mtd.right-flush {
  padding-right: 0;
}

/* Tables */
mtable {
  math-style: inherit;
}
mtd {
  math-depth: auto-add;
  math-style: compact;
  math-shift: compact;
}

/* Equations */
mtable.multiline-equation mtd {
  math-depth: inherit;
  math-style: inherit;
  math-shift: inherit;
  padding: 0;
}
math > mtable.multiline-equation mtr:not(:last-child) mtd {
  padding-bottom: 0.5em;
}

/* Fractions */
mfrac {
  padding-inline: 0;
  margin-inline: 0.1em;
}

/* Accents */
mover[accent="true" i] > :first-child {
  font-feature-settings: "dtls";
}
mover.dotted[accent="true" i] > :first-child {
  font-feature-settings: "dtls" 0;
}

/* Other rules for scriptlevel, displaystyle and math-shift */
munder > :nth-child(2),
munderover > :nth-child(2) {
  math-shift: compact
}
munder[accentunder="true" i] > :not(:first-child),
mover[accent="true" i] > :not(:first-child) {
  math-depth: inherit;
  math-style: inherit;
  math-shift: inherit;
}"#;

/// Compiles one Typst equation to MathML while preserving the world's main file.
///
/// Returns an error for unsupported constructs reported as warnings, allowing
/// callers to use an image fallback without silently losing part of an equation.
pub fn equation_mathml(
    world: &mut WriterWorld, source: &str, display: bool,
) -> Result<String> {
    let text = if display {
        format!("$ {source} $")
    } else {
        format!("${source}$")
    };
    world.with_render_main(&text, true, |world| {
        let typst::diag::Warned { output, warnings } =
            typst::compile::<HtmlDocument>(&*world);
        let document = export_result(world, output)?;
        for warning in warnings {
            if warning.span.is_detached()
                && warning.message
                    == "html export is under active development and incomplete"
            {
                continue;
            }
            return Err(Error::Other(warning.message.to_string()));
        }
        let html = typst_html::html(
            &document,
            &typst_html::HtmlOptions { pretty: false },
        );
        let html = export_result(world, html)?;
        extract_mathml(&html).map(str::to_owned)
    })
}

fn extract_mathml(html: &str) -> Result<&str> {
    let start = html.find("<math").ok_or_else(|| {
        Error::Other("equation HTML has no MathML element".into())
    })?;
    let end = html[start..].find("</math>").ok_or_else(|| {
        Error::Other("equation HTML has no closing MathML tag".into())
    })?;
    Ok(&html[start..start + end + "</math>".len()])
}

fn export_result<T>(
    world: &WriterWorld, output: typst::diag::SourceResult<T>,
) -> Result<T> {
    match output {
        Ok(output) => Ok(output),
        Err(errors) => {
            let diagnostics = errors
                .iter()
                .map(|error| map_diagnostic(world, error))
                .collect();
            Compiled { output: None, diagnostics }
                .into_result()
                .map(|(output, _)| output)
        }
    }
}
