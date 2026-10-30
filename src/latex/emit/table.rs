//! Table grids and page-breaking/boxed LaTeX environments.

use crate::config::LatexTables;
use crate::ir::{Align, Attrs, ColumnSpec, Row, TableCell};

use super::{Caption, Context, Emitter, Level, Output};

#[derive(Clone, Copy)]
struct Placed<'a> {
    column: usize,
    span: usize,
    source: Option<&'a TableCell>,
}

struct Section<'a> {
    rows: Vec<Vec<Placed<'a>>>,
    dropped: usize,
    rowspan: bool,
}

impl<'a> Section<'a> {
    fn new(rows: &'a [Row], columns: usize) -> Self {
        let mut plan = Self { rows: Vec::new(), dropped: 0, rowspan: false };
        let mut remaining = vec![0u32; columns];
        for row in rows {
            let mut occupied: Vec<_> =
                remaining.iter().map(|&r| r > 0).collect();
            let mut next: Vec<_> =
                remaining.iter().map(|r| r.saturating_sub(1)).collect();
            let mut placed = Vec::new();
            let mut cursor = 0;
            for cell in &row.cells {
                plan.rowspan |= cell.rowspan > 1;
                while cursor < columns && occupied[cursor] {
                    cursor += 1;
                }
                let span =
                    usize::try_from(cell.colspan.max(1)).unwrap_or(usize::MAX);
                let end =
                    cursor.checked_add(span).filter(|&end| end <= columns);
                let Some(end) = end else {
                    plan.dropped += 1;
                    continue;
                };
                if occupied[cursor..end].iter().any(|&slot| slot) {
                    plan.dropped += 1;
                    continue;
                }
                occupied[cursor..end].fill(true);
                next[cursor..end].fill(cell.rowspan.saturating_sub(1));
                placed.push(Placed {
                    column: cursor,
                    span,
                    source: Some(cell),
                });
                cursor = end;
            }
            for column in 0..columns {
                if remaining[column] > 0 || !occupied[column] {
                    placed.push(Placed { column, span: 1, source: None });
                }
            }
            placed.sort_by_key(|cell| cell.column);
            plan.rows.push(placed);
            remaining = next;
        }
        plan
    }
}

/// Actual inline owners for the emitter's note-reference prepass.
/// Zero-column fallback retains every source cell as a plain paragraph.
pub(super) fn rendered_cells<'a>(
    columns: usize, head: &'a [Row], body: &'a [Row], foot: &'a [Row],
) -> Vec<&'a TableCell> {
    let mut cells = Vec::new();
    for rows in [head, body, foot] {
        if columns == 0 {
            cells.extend(rows.iter().flat_map(|row| &row.cells));
        } else {
            for row in Section::new(rows, columns).rows {
                cells.extend(row.into_iter().filter_map(|cell| cell.source));
            }
        }
    }
    cells
}

impl Emitter<'_> {
    #[allow(
        clippy::too_many_arguments,
        reason = "the shared emitter seam carries all three table sections"
    )]
    pub(super) fn table(
        &mut self, columns: &[ColumnSpec], head: &[Row], body: &[Row],
        foot: &[Row], attrs: &Attrs, context: Context,
        caption: Option<Caption<'_>>,
    ) -> Output {
        if columns.is_empty() {
            return self.table_without_columns(
                head, body, foot, attrs, context, caption,
            );
        }
        let caption_output =
            caption.map(|caption| self.caption(caption, context));
        let sections = [
            Section::new(head, columns.len()),
            Section::new(body, columns.len()),
            Section::new(foot, columns.len()),
        ];
        if sections.iter().any(|section| section.rowspan) {
            self.finding(
                Level::Warning, "latex-rowspan",
                "LaTeX keeps rowspanned content in its first row; later covered cells are empty".into(),
            );
        }
        let dropped: usize =
            sections.iter().map(|section| section.dropped).sum();
        if dropped > 0 {
            self.finding(
                Level::Warning, "unsupported-construct",
                format!("latex: {dropped} table cell(s) exceed the column grid or overlap a rowspan; those cells were dropped"),
            );
        }
        let widths = column_widths(columns);
        let cell_context = Context {
            boxed: true,
            compact: true,
            table_p: widths.is_some(),
            ..context
        };
        let [head, body, foot] = sections.map(|section| {
            self.table_rows(
                &section.rows,
                columns,
                widths.as_deref(),
                cell_context,
            )
        });
        let spec = table_spec(columns, widths.as_deref());
        let longtable = !context.boxed
            && !context.footnote
            && self.config.latex.tables == LatexTables::Longtable;
        let numbered = caption.is_some_and(|caption| caption.numbered);
        let caption_row = caption.is_some_and(|caption| {
            caption.numbered || caption.caption.is_some()
        });
        let own_id = attrs.id.as_deref().filter(|id| {
            caption
                .is_none_or(|caption| caption.attrs.id.as_deref() != Some(*id))
        });
        let mut output = Output::default();
        let own_label = self.label(own_id, false);
        if !own_label.is_empty() {
            output.literal(&own_label);
            output.literal("\n");
        }
        self.usage.table = true;
        let (top, mid, bottom) = if self.config.latex.booktabs {
            ("\\toprule\n", "\\midrule\n", "\\bottomrule\n")
        } else {
            ("\\hline\n", "\\hline\n", "\\hline\n")
        };
        if longtable {
            self.usage.longtable = true;
            self.usage.uncaptioned_longtable |= !numbered;
            // A label-only caption belongs outside the alignment, not in an
            // empty row. A real caption line ends with tabularnewline.
            if !caption_row
                && let Some(caption) = caption_output.as_ref()
                && !caption.is_empty()
            {
                output.append(caption.clone());
                output.literal("\n");
            }
            if !numbered {
                output.literal("{\\def\\LTcaptype{none}\n");
            }
            output.literal(&format!("\\begin{{longtable}}[]{{{spec}}}\n"));
            if caption_row {
                output
                    .append(caption_output.expect("caption row has a caption"));
                output.literal("\\tabularnewline\n");
            }
            output.literal(top);
            if !head.is_empty() {
                output.append(head.clone());
                output.literal(mid);
                output.literal("\\endfirsthead\n");
                output.literal(top);
                output.append(head);
                output.literal(mid);
            }
            output.literal("\\endhead\n");
            if !foot.is_empty() {
                output.literal(mid);
                output.append(foot);
            }
            output.literal(bottom);
            output.literal("\\endlastfoot\n");
            output.append(body);
            output.literal("\\end{longtable}");
            if !numbered {
                output.literal("\n}");
            }
        } else {
            let floating =
                caption.is_some() && !context.boxed && !context.footnote;
            if floating {
                let placement = caption
                    .and_then(|caption| caption.attrs.pairs.get("fig-pos"))
                    .unwrap_or(&self.config.latex.figure_placement);
                output.literal("\\begin{table}");
                if !placement.is_empty() {
                    output.literal(&format!("[{placement}]"));
                }
                output.literal("\n\\centering\n");
            } else {
                output.literal("\\begin{center}\n");
                if caption.is_some() {
                    self.usage.caption = true;
                    output.literal("\\captionsetup{type=table}\n");
                }
            }
            if let Some(caption) = caption_output
                && !caption.is_empty()
            {
                output.append(caption);
                output.literal("\n");
            }
            output.literal(&format!("\\begin{{tabular}}{{{spec}}}\n"));
            output.literal(top);
            if !head.is_empty() {
                output.append(head);
                output.literal(mid);
            }
            output.append(body);
            if !foot.is_empty() {
                output.literal(mid);
                output.append(foot);
            }
            output.literal(bottom);
            output.literal("\\end{tabular}\n");
            output.literal(if floating {
                "\\end{table}"
            } else {
                "\\end{center}"
            });
        }
        output
    }

    fn table_without_columns(
        &mut self, head: &[Row], body: &[Row], foot: &[Row], attrs: &Attrs,
        context: Context, caption: Option<Caption<'_>>,
    ) -> Output {
        self.finding(
            Level::Warning, "unsupported-construct",
            "latex: table has no columns; emitted its cells as plain paragraphs".into(),
        );
        let mut output = Output::default();
        let own_id = attrs.id.as_deref().filter(|id| {
            caption
                .is_none_or(|caption| caption.attrs.id.as_deref() != Some(*id))
        });
        let label = self.label(own_id, false);
        if !label.is_empty() {
            output.literal(&label);
        }
        if let Some(caption) = caption {
            let command = self.caption(caption, context);
            if !command.is_empty() {
                if !output.is_empty() {
                    output.literal("\n\n");
                }
                if caption.numbered || caption.caption.is_some() {
                    self.usage.caption = true;
                    output.literal("{\n\\captionsetup{type=table}\n");
                    output.append(command);
                    output.literal("\n}");
                } else {
                    output.append(command);
                }
            }
        }
        for row in head.iter().chain(body).chain(foot) {
            for cell in &row.cells {
                let content = self.inlines(&cell.inlines, context);
                if !content.is_empty() {
                    if !output.is_empty() {
                        output.literal("\n\n");
                    }
                    output.append(content);
                }
            }
        }
        output
    }

    fn table_rows(
        &mut self, rows: &[Vec<Placed<'_>>], columns: &[ColumnSpec],
        widths: Option<&[f64]>, context: Context,
    ) -> Output {
        let mut output = Output::default();
        for row in rows {
            let mut rendered = Output::default();
            for (index, cell) in row.iter().enumerate() {
                if index > 0 {
                    rendered.literal(" & ");
                }
                let Some(source) = cell.source else { continue };
                let content = self.inlines(&source.inlines, context);
                if cell.span > 1 || source.align.is_some() {
                    let align =
                        source.align.unwrap_or(columns[cell.column].align);
                    let width = widths.map(|widths| {
                        widths[cell.column..cell.column + cell.span]
                            .iter()
                            .sum()
                    });
                    rendered.literal(&format!(
                        "\\multicolumn{{{}}}{{{}}}{{",
                        cell.span,
                        column_spec(align, width),
                    ));
                    rendered.append(content);
                    rendered.literal("}");
                } else {
                    rendered.append(content);
                }
            }
            if row_guard(&rendered.text) {
                output.literal("{}");
            }
            output.append(rendered);
            output.literal(" \\\\\n");
        }
        output
    }
}

fn column_widths(columns: &[ColumnSpec]) -> Option<Vec<f64>> {
    if !columns.iter().any(|column| column.width.is_some()) {
        return None;
    }
    let given: f64 =
        columns.iter().filter_map(|column| column.width).map(f64::from).sum();
    let missing =
        columns.iter().filter(|column| column.width.is_none()).count();
    let remaining = (1.0 - given).max(0.0);
    let fallback = if missing > 0 && remaining > 0.0 {
        remaining / missing as f64
    } else {
        1.0 / columns.len() as f64
    };
    Some(
        columns
            .iter()
            .map(|column| column.width.map(f64::from).unwrap_or(fallback))
            .collect(),
    )
}

fn table_spec(columns: &[ColumnSpec], widths: Option<&[f64]>) -> String {
    let mut spec = "@{}".to_string();
    for (index, column) in columns.iter().enumerate() {
        spec.push_str(&column_spec(
            column.align,
            widths.map(|widths| widths[index]),
        ));
    }
    spec.push_str("@{}");
    spec
}

fn column_spec(align: Align, width: Option<f64>) -> String {
    if let Some(width) = width {
        let command = match align {
            Align::Default | Align::Left => "\\raggedright",
            Align::Center => "\\centering",
            Align::Right => "\\raggedleft",
        };
        let width = format!("{width:.6}");
        let width = width.trim_end_matches('0').trim_end_matches('.');
        format!(
            ">{{{command}\\arraybackslash}}p{{\\dimexpr {width}\\linewidth-2\\tabcolsep\\relax}}"
        )
    } else {
        match align {
            Align::Default | Align::Left => "l",
            Align::Center => "c",
            Align::Right => "r",
        }
        .into()
    }
}

fn row_guard(text: &str) -> bool {
    text.trim_start().starts_with(['*', '['])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::registry::AssetRegistry;
    use crate::config::Config;
    use crate::ir::{
        Block, DocMeta, Document, Features, FigureKind, Inline, Resolved,
        SourceInfo, SourceKind, SourceSpan,
    };

    fn document(block: Block) -> Document {
        Document {
            meta: DocMeta::default(),
            blocks: vec![block],
            features: Features::default(),
            source: SourceInfo {
                path: "/project/main.md".into(),
                base_dir: "/project".into(),
                kind: SourceKind::Markdown,
            },
            assets: Vec::new(),
            includes: Vec::new(),
            resolved: Resolved::default(),
        }
    }

    fn cell(text: &str, colspan: u32, rowspan: u32) -> TableCell {
        TableCell {
            inlines: vec![Inline::text(text)],
            colspan,
            rowspan,
            ..TableCell::default()
        }
    }

    #[test]
    fn rowspan_slots_are_empty_and_every_section_starts_a_fresh_grid() {
        let rows = vec![
            Row { cells: vec![cell("span", 2, 2), cell("right", 1, 1)] },
            Row { cells: vec![cell("later", 1, 1)] },
        ];
        let section = Section::new(&rows, 3);
        assert!(section.rowspan);
        assert_eq!(section.dropped, 0);
        assert_eq!(
            section.rows[0]
                .iter()
                .map(|cell| (cell.column, cell.span))
                .collect::<Vec<_>>(),
            [(0, 2), (2, 1)]
        );
        assert_eq!(
            section.rows[1]
                .iter()
                .map(|cell| { (cell.column, cell.source.is_some()) })
                .collect::<Vec<_>>(),
            [(0, false), (1, false), (2, true)]
        );
        let next = vec![Row { cells: vec![cell("new section", 3, 1)] }];
        let fresh = Section::new(&next, 3);
        assert_eq!(fresh.rows[0][0].column, 0);
        assert_eq!(fresh.rows[0][0].span, 3);
    }

    #[test]
    fn malformed_spans_do_not_drop_later_cells_that_fit_free_slots() {
        let rows = vec![
            Row { cells: vec![cell("left", 1, 1), cell("held", 1, 2)] },
            Row {
                cells: vec![cell("invalid", u32::MAX, 1), cell("kept", 1, 1)],
            },
        ];
        let section = Section::new(&rows, 2);
        assert_eq!(section.dropped, 1);
        assert_eq!(section.rows[1][0].column, 0);
        assert_eq!(
            section.rows[1][0].source.unwrap().inlines,
            [Inline::text("kept")]
        );
        assert_eq!(section.rows[1][1].column, 1);
        assert!(section.rows[1][1].source.is_none());
    }

    #[test]
    fn p_columns_fill_missing_widths_and_span_specs_sum_them() {
        let columns = [
            ColumnSpec { align: Align::Left, width: Some(0.5) },
            ColumnSpec { align: Align::Center, width: None },
            ColumnSpec { align: Align::Right, width: None },
        ];
        let widths = column_widths(&columns).unwrap();
        assert_eq!(widths, [0.5, 0.25, 0.25]);
        assert_eq!(
            column_spec(Align::Center, Some(widths[1..].iter().sum())),
            ">{\\centering\\arraybackslash}p{\\dimexpr 0.5\\linewidth-2\\tabcolsep\\relax}"
        );
        assert!(
            table_spec(&columns, Some(&widths))
                .starts_with("@{}>{\\raggedright")
        );
        let saturated = [
            ColumnSpec { align: Align::Default, width: Some(1.0) },
            ColumnSpec::default(),
        ];
        assert_eq!(column_widths(&saturated).unwrap(), [1.0, 0.5]);
        assert_eq!(table_spec(&columns, None), "@{}lcr@{}");
    }

    #[test]
    fn row_guards_cover_optional_break_syntax_and_leading_whitespace() {
        assert!(row_guard("[option]"));
        assert!(row_guard("*star"));
        assert!(row_guard("  [option]"));
        assert!(!row_guard(" & [second cell]"));
        assert!(!row_guard("\\multicolumn{2}{l}{[safe]}"));
    }

    #[test]
    fn zero_column_tables_keep_plain_content_without_table_usage_or_invalid_tex()
     {
        let span = SourceSpan {
            start_line: 5,
            end_line: 6,
            cell: Some(2),
            file: Some("/project/child.ipynb".into()),
        };
        for body in [
            Vec::new(),
            vec![Row {
                cells: vec![
                    cell("FIRST-CONTENT", u32::MAX, 2),
                    cell("SECOND-CONTENT", 0, 0),
                ],
            }],
        ] {
            let has_cells = !body.is_empty();
            let document = document(Block::Table {
                columns: Vec::new(),
                head: Vec::new(),
                body,
                foot: Vec::new(),
                attrs: Attrs::default(),
                span: Some(span.clone()),
            });
            let emitted = super::super::emit(
                &document,
                &AssetRegistry::new(),
                &Config::default(),
                "report",
            );
            assert!(!emitted.usage.table);
            assert!(!emitted.usage.longtable);
            assert!(!emitted.body.contains("\\begin{tabular}"));
            assert!(!emitted.body.contains("\\begin{longtable}"));
            assert_eq!(emitted.diagnostics.len(), 1);
            let finding = emitted.diagnostics.iter().next().unwrap();
            assert_eq!(finding.code, "unsupported-construct");
            assert_eq!(finding.span.as_ref(), Some(&span));
            if has_cells {
                assert_eq!(emitted.body, "FIRST-CONTENT\n\nSECOND-CONTENT\n");
            } else {
                assert!(emitted.body.is_empty());
            }
        }
    }

    #[test]
    fn zero_column_figure_keeps_caption_and_one_shared_label_without_a_float() {
        let attrs = Attrs { id: Some("tbl-empty".into()), ..Attrs::default() };
        let document = document(Block::Figure {
            kind: FigureKind::Table,
            caption: Some(vec![Inline::text("Kept caption")]),
            attrs: attrs.clone(),
            number: Some("1".into()),
            span: None,
            body: vec![Block::Table {
                columns: Vec::new(),
                head: Vec::new(),
                body: Vec::new(),
                foot: Vec::new(),
                attrs,
                span: None,
            }],
        });
        let emitted = super::super::emit(
            &document,
            &AssetRegistry::new(),
            &Config::default(),
            "report",
        );
        assert!(emitted.body.contains("\\captionsetup{type=table}"));
        assert!(emitted.body.contains("\\caption{Kept caption}"));
        let label =
            format!("\\label{{{}}}", crate::resolve::anchor("tbl-empty"));
        assert_eq!(emitted.body.matches(&label).count(), 1);
        assert!(!emitted.body.contains("\\begin{table}"));
        assert!(!emitted.usage.table);
        assert!(!emitted.usage.longtable);
        assert!(emitted.usage.caption);
    }

    #[test]
    fn a_reference_only_in_a_dropped_cell_does_not_label_its_owner_note() {
        let table = Block::Table {
            columns: vec![ColumnSpec::default()],
            head: Vec::new(),
            body: vec![Row {
                cells: vec![
                    TableCell {
                        inlines: vec![Inline::FootnoteRef {
                            label: "note".into(),
                            number: Some(1),
                        }],
                        colspan: u32::MAX,
                        ..TableCell::default()
                    },
                    cell("KEPT-CELL", 1, 1),
                ],
            }],
            foot: Vec::new(),
            attrs: Attrs::default(),
            span: None,
        };
        let mut document = document(table);
        document.blocks.insert(
            0,
            Block::paragraph(vec![Inline::Footnote {
                blocks: vec![Block::paragraph(vec![Inline::text(
                    "OWNER-NOTE",
                )])],
                label: Some("note".into()),
                number: Some(1),
            }]),
        );
        let emitted = super::super::emit(
            &document,
            &AssetRegistry::new(),
            &Config::default(),
            "report",
        );
        assert!(emitted.body.contains("\\footnote{OWNER-NOTE}"));
        assert!(emitted.body.contains("KEPT-CELL"));
        assert!(!emitted.body.contains("writer-fn-1"));
        assert_eq!(
            emitted
                .diagnostics
                .iter()
                .filter(|finding| { finding.code == "unsupported-construct" })
                .count(),
            1
        );
    }
}
