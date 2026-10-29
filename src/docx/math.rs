//! Editable Word equations, numbered frames and specified recovery paths.

use std::collections::BTreeMap;

use crate::config::DocxMath;
use crate::ir::{Block, MathDialect};
use crate::math::latex_mathml::{self, MathError};
use crate::math::{latex_source, omml};

use super::{Emitter, Picture, StyleKind, xml};

pub(crate) struct State {
    macros: BTreeMap<String, String>,
    macro_diags_done: bool,
    pub(crate) custom_div_style: Option<String>,
}

impl State {
    pub(crate) fn new(config: &crate::config::Config) -> Self {
        Self {
            macros: latex_source::macros_from_config(
                &config.math.katex_options,
            ),
            macro_diags_done: false,
            custom_div_style: None,
        }
    }
}

enum Body {
    Omml(String),
    Picture(Picture),
    Source(String),
    Plain(String),
}

impl Emitter<'_> {
    /// Writes one inline equation beside the paragraph's ordinary runs.
    pub(crate) fn math_inline(&mut self, source: &str, dialect: MathDialect) {
        let body = self.equation_body(source, dialect, false);
        self.equation_runs(&body, source, dialect, false);
    }

    /// Writes a display equation, retaining its resolved number on recovery.
    pub(crate) fn math_block(&mut self, block: &Block) {
        let Block::Math { source, dialect, attrs, number, .. } = block else {
            return;
        };
        let body = self.equation_body(source, *dialect, true);
        if let Some(number) = number {
            self.equation_frame(
                &body,
                source,
                *dialect,
                attrs.id.as_deref(),
                number,
            );
        } else {
            let bookmark =
                attrs.id.as_deref().and_then(|id| self.start_bookmark(id));
            self.equation_paragraph(&body, source, *dialect, false);
            self.end_bookmark(bookmark);
        }
    }

    fn equation_body(
        &mut self, source: &str, dialect: MathDialect, display: bool,
    ) -> Body {
        let prepared = match dialect {
            MathDialect::Latex => {
                let (prepared, removed) = latex_source::prepare(source);
                if removed {
                    self.content_warning(
                        "math-tag-ignored",
                        "\\tag/\\label inside math are ignored for docx; label the equation with {#eq-x}",
                    );
                }
                prepared
            }
            MathDialect::Typst => source.into(),
        };
        if self.config.docx.math == DocxMath::Image {
            return self.equation_picture(source, dialect, display, None);
        }
        let mathml = match dialect {
            MathDialect::Latex => {
                if !self.math_state.macro_diags_done {
                    let diagnostics = latex_mathml::macro_diagnostics(
                        &self.math_state.macros,
                    );
                    for diagnostic in diagnostics {
                        self.content_warning(
                            diagnostic.code, diagnostic.message,
                        );
                    }
                    self.math_state.macro_diags_done = true;
                }
                match latex_mathml::to_mathml(
                    &prepared, display, &self.math_state.macros,
                ) {
                    Ok(mathml) => mathml,
                    Err(error) => {
                        let message = match error {
                            MathError::Syntax { message, range, .. } => {
                                let position =
                                    range.map_or_else(String::new, |range| {
                                        format!(
                                            " at bytes {}..{} of the equation",
                                            range.start, range.end
                                        )
                                    });
                                format!(
                                    "invalid LaTeX math: {message}{position}"
                                )
                            }
                            error => format!("invalid LaTeX math: {error}"),
                        };
                        self.content_warning("math-syntax", message);
                        return Body::Source(source.into());
                    }
                }
            }
            MathDialect::Typst => match self
                .renderer
                .typst_mathml(source, display)
            {
                Ok(mathml) => mathml,
                Err(error) => {
                    if display {
                        return self.equation_picture(
                            source,
                            dialect,
                            true,
                            Some(error.to_string()),
                        );
                    }
                    self.equation_failure(&error.to_string(), "source text");
                    return Body::Source(source.into());
                }
            },
        };
        match omml::from_mathml(&mathml) {
            Ok(equation) => Body::Omml(equation),
            Err(error) => {
                if display {
                    self.equation_picture(
                        source,
                        dialect,
                        true,
                        Some(error.to_string()),
                    )
                } else {
                    self.equation_failure(&error.to_string(), "plain text");
                    Body::Plain(omml::fallback_text(&mathml))
                }
            }
        }
    }

    fn equation_picture(
        &mut self, source: &str, dialect: MathDialect, display: bool,
        previous: Option<String>,
    ) -> Body {
        let prepared = match dialect {
            MathDialect::Latex => {
                latex_source::for_typst(source, &self.math_state.macros).0
            }
            MathDialect::Typst => source.into(),
        };
        match self.renderer.equation(&prepared, dialect, display) {
            Ok(picture) => {
                if let Some(reason) = previous {
                    self.equation_failure(&reason, "an equation picture");
                }
                Body::Picture(picture)
            }
            Err(error) => {
                let reason = match previous {
                    Some(reason) => {
                        format!("{reason}; rendition failed: {error}")
                    }
                    None => error.to_string(),
                };
                self.equation_failure(&reason, "source text");
                Body::Source(source.into())
            }
        }
    }

    fn equation_failure(&mut self, reason: &str, fallback: &str) {
        self.content_warning(
            "math-conversion-failed",
            format!(
                "docx: cannot convert equation ({reason}); used {fallback}"
            ),
        );
    }

    fn equation_runs(
        &mut self, body: &Body, source: &str, dialect: MathDialect,
        display: bool,
    ) {
        match body {
            Body::Omml(equation) => {
                if display {
                    self.buffer().push_str("<m:oMathPara><m:oMathParaPr><m:jc m:val=\"center\"/></m:oMathParaPr>");
                }
                self.buffer().push_str(equation);
                if display {
                    self.buffer().push_str("</m:oMathPara>");
                }
            }
            Body::Picture(picture) => {
                let mut hash = blake3::Hasher::new();
                hash.update(&[
                    u8::from(dialect == MathDialect::Typst),
                    u8::from(display),
                ]);
                hash.update(source.as_bytes());
                let key = format!("equation:{}", hash.finalize().to_hex());
                self.picture_run(&key, picture, source, !display);
            }
            Body::Source(text) if !display => {
                let style =
                    self.styles.ensure("Verbatim Char", StyleKind::Character);
                self.inlines(
                    &[crate::ir::Inline::Text { text: text.clone() }],
                    &xml::RunProps { style: Some(style), ..Default::default() },
                );
            }
            Body::Source(text) | Body::Plain(text) => self.text_run(text),
        }
    }

    fn equation_paragraph(
        &mut self, body: &Body, source: &str, dialect: MathDialect,
        framed: bool,
    ) {
        let style = if matches!(body, Body::Source(_)) {
            self.styles.ensure("Source Code", StyleKind::Paragraph)
        } else {
            self.math_state.custom_div_style.clone().unwrap_or_else(|| {
                self.styles.ensure("Equation", StyleKind::Paragraph)
            })
        };
        self.equation_start_paragraph(
            xml::ParaProps {
                style: Some(style),
                jc: Some("center"),
                ..Default::default()
            },
            framed,
        );
        self.equation_runs(body, source, dialect, true);
        self.end_paragraph();
    }

    fn equation_frame(
        &mut self, body: &Body, source: &str, dialect: MathDialect,
        label: Option<&str>, number: &str,
    ) {
        self.separate_table();
        let style = self.styles.ensure("Normal Table", StyleKind::Table);
        let properties = xml::TableProps {
            style: Some(style),
            width: Some("<w:tblW w:w=\"5000\" w:type=\"pct\"/>".into()),
            layout_fixed: true,
            borders: Some(concat!(
                "<w:tblBorders><w:top w:val=\"nil\"/><w:left w:val=\"nil\"/>",
                "<w:bottom w:val=\"nil\"/><w:right w:val=\"nil\"/>",
                "<w:insideH w:val=\"nil\"/><w:insideV w:val=\"nil\"/></w:tblBorders>"
            ).into()),
            look: Some("<w:tblLook w:firstRow=\"0\" w:lastRow=\"0\" w:firstColumn=\"0\" w:lastColumn=\"0\" w:noHBand=\"1\" w:noVBand=\"1\" w:val=\"0600\"/>".into()),
            ..Default::default()
        };
        let width = self.available_width_twips();
        let side = width / 10;
        self.buffer().push_str(&format!(
            "<w:tbl>{}<w:tblGrid><w:gridCol w:w=\"{side}\"/><w:gridCol w:w=\"{}\"/><w:gridCol w:w=\"{side}\"/></w:tblGrid><w:tr><w:trPr><w:cantSplit/></w:trPr>",
            properties.to_xml(), width - 2 * side,
        ));
        self.equation_cell(500);
        let compact = self.styles.ensure("Compact", StyleKind::Paragraph);
        self.equation_start_paragraph(
            xml::ParaProps { style: Some(compact), ..Default::default() },
            true,
        );
        self.end_paragraph();
        self.buffer().push_str("</w:tc>");
        self.equation_cell(4000);
        let previous_width = self.table_width_twips;
        self.table_width_twips = Some(width - 2 * side);
        self.equation_paragraph(body, source, dialect, true);
        self.table_width_twips = previous_width;
        self.buffer().push_str("</w:tc>");
        self.equation_cell(500);
        let equation = self.styles.ensure("Equation", StyleKind::Paragraph);
        self.equation_start_paragraph(
            xml::ParaProps {
                style: Some(equation),
                jc: Some("right"),
                ..Default::default()
            },
            true,
        );
        self.text_run("(");
        let bookmark = label.and_then(|id| self.start_bookmark(id));
        self.sequence_number("Equation", number);
        self.end_bookmark(bookmark);
        self.text_run(")");
        self.end_paragraph();
        self.buffer().push_str("</w:tc></w:tr></w:tbl>");
    }

    fn equation_cell(&mut self, width: u32) {
        let properties = xml::CellProps {
            width: Some(format!("<w:tcW w:w=\"{width}\" w:type=\"pct\"/>")),
            v_align: Some("center"),
            ..Default::default()
        };
        self.buffer().push_str(&format!("<w:tc>{}", properties.to_xml()));
    }

    fn equation_start_paragraph(
        &mut self, props: xml::ParaProps, framed: bool,
    ) {
        if framed {
            // Frame cells have their own paragraph context, like T41 tables.
            self.buffer().push_str(&format!("<w:p>{}", props.to_xml()));
        } else {
            self.start_paragraph(props);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::registry::AssetRegistry;
    use crate::config::Config;
    use crate::docx::{DocxOptions, Renderer};
    use crate::error::Result;
    use crate::ir::Document;

    struct NoRenderer;

    impl Renderer for NoRenderer {
        fn equation(
            &mut self, _: &str, _: MathDialect, _: bool,
        ) -> Result<Picture> {
            unreachable!("the frame consumes an already rendered picture")
        }

        fn image(
            &mut self, _: &[u8], _: &str, _: Option<f64>,
        ) -> Result<Picture> {
            unreachable!("the picture already includes its PNG")
        }

        fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
            unreachable!("the frame consumes an already rendered picture")
        }
    }

    #[test]
    fn equation_frame_clamps_to_its_middle_cell_and_restores_outer_width() {
        let doc: Document = serde_json::from_value(serde_json::json!({
            "meta": {}, "blocks": [], "features": {}, "assets": [],
            "source": {"path": "/math.md", "kind": "markdown", "base_dir": "/"}
        }))
        .unwrap();
        let config = Config::default();
        let assets = AssetRegistry::new();
        let opts = DocxOptions {
            config: &config,
            reference: None,
            source_date_epoch: None,
            all_styles: false,
        };
        let mut renderer = NoRenderer;
        let mut emitter = crate::docx::prepare_emitter(
            &doc, &assets, &mut renderer, &opts, None, "en", false,
        )
        .unwrap();
        emitter.table_width_twips = Some(1000);
        let body = Body::Picture(Picture {
            png: include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/docx/images/dpi144.png"
            ))
            .to_vec(),
            svg: None,
            width_pt: 80.0,
            height_pt: 40.0,
            baseline_pt: 30.0,
        });
        emitter.equation_frame(&body, "x", MathDialect::Latex, None, "1");
        assert!(emitter.body.contains("<w:gridCol w:w=\"100\"/><w:gridCol w:w=\"800\"/><w:gridCol w:w=\"100\"/>"));
        assert!(
            emitter.body.contains("<wp:extent cx=\"508000\" cy=\"254000\"/>")
        );
        assert_eq!(emitter.table_width_twips, Some(1000));
        emitter.equation_paragraph(&body, "y", MathDialect::Latex, false);
        assert!(
            emitter.body.contains("<wp:extent cx=\"635000\" cy=\"317500\"/>")
        );
    }
}
