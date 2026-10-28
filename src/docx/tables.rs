//! DOCX table rendering (docx-output.md §4.12).

use crate::ir::{Align, Block, ColumnSpec, Row, TableCell, plain_text};

use super::styles::StyleKind;
use super::{Container, Emitter, xml};

#[derive(Clone, Copy)]
struct Merge {
    colspan: usize,
    remaining: u32,
}

#[derive(Clone, Copy)]
enum CellContent<'a> {
    Source(&'a TableCell),
    Continuation,
    Empty,
}

struct Cell<'a> {
    column: usize,
    colspan: usize,
    content: CellContent<'a>,
}

struct PlannedRow<'a> {
    head: bool,
    body: bool,
    cells: Vec<Cell<'a>>,
}

#[derive(Default)]
struct Plan<'a> {
    rows: Vec<PlannedRow<'a>>,
    dropped: usize,
}

impl<'a> Plan<'a> {
    fn section(
        &mut self, rows: &'a [Row], columns: usize, head: bool, body: bool,
    ) {
        // Rowspans belong to their row group, just as in the IR validator.
        let mut active = vec![None::<Merge>; columns];
        for row in rows {
            let mut occupied = vec![false; columns];
            let mut cells = Vec::new();
            let mut next = vec![None; columns];
            for (column, merge) in active.iter().enumerate() {
                let Some(merge) = merge else { continue };
                occupied[column..column + merge.colspan].fill(true);
                cells.push(Cell {
                    column,
                    colspan: merge.colspan,
                    content: CellContent::Continuation,
                });
                if merge.remaining > 1 {
                    next[column] = Some(Merge {
                        remaining: merge.remaining - 1,
                        ..*merge
                    });
                }
            }
            let mut cursor = 0;
            for cell in &row.cells {
                while cursor < columns && occupied[cursor] {
                    cursor += 1;
                }
                let span =
                    usize::try_from(cell.colspan.max(1)).unwrap_or(usize::MAX);
                let end = cursor.checked_add(span);
                let Some(end) = end.filter(|&end| end <= columns) else {
                    self.dropped += 1;
                    continue;
                };
                if occupied[cursor..end].iter().any(|&slot| slot) {
                    self.dropped += 1;
                    continue;
                }
                occupied[cursor..end].fill(true);
                cells.push(Cell {
                    column: cursor,
                    colspan: span,
                    content: CellContent::Source(cell),
                });
                if cell.rowspan > 1 {
                    next[cursor] = Some(Merge {
                        colspan: span,
                        remaining: cell.rowspan - 1,
                    });
                }
                cursor = end;
            }
            for (column, occupied) in occupied.into_iter().enumerate() {
                if !occupied {
                    cells.push(Cell {
                        column,
                        colspan: 1,
                        content: CellContent::Empty,
                    });
                }
            }
            cells.sort_by_key(|cell| cell.column);
            self.rows.push(PlannedRow { head, body, cells });
            active = next;
        }
    }
}

fn plan(block: &Block) -> Plan<'_> {
    let mut plan = Plan::default();
    if let Block::Table { columns, head, body, foot, .. } = block {
        plan.section(head, columns.len(), true, false);
        plan.section(body, columns.len(), false, true);
        plan.section(foot, columns.len(), false, false);
    }
    plan
}

/// Original cells retained by the exact grid plan used for emission.
///
/// The content prepass must not allocate notes or bookmarks from dropped cells.
pub(crate) fn rendered_cells(block: &Block) -> Vec<&TableCell> {
    plan(block)
        .rows
        .into_iter()
        .flat_map(|row| row.cells)
        .filter_map(|cell| match cell.content {
            CellContent::Source(source) => Some(source),
            CellContent::Continuation | CellContent::Empty => None,
        })
        .collect()
}

fn grid(columns: &[ColumnSpec], available: u32) -> (Vec<u32>, String, bool) {
    if columns.is_empty() {
        return (
            Vec::new(),
            "<w:tblW w:w=\"0\" w:type=\"auto\"/>".into(),
            false,
        );
    }
    let fractions: Option<Vec<_>> =
        columns.iter().map(|column| column.width.map(f64::from)).collect();
    if let Some(fractions) = fractions {
        let sum: f64 = fractions.iter().sum();
        let scale = (1.0 / sum).min(1.0);
        let grid = fractions
            .iter()
            .map(|fraction| {
                (fraction * scale * f64::from(available)).round() as u32
            })
            .collect();
        let percent = (sum.min(1.0) * 5000.0).round() as u32;
        (grid, format!("<w:tblW w:w=\"{percent}\" w:type=\"pct\"/>"), true)
    } else {
        let count = u32::try_from(columns.len()).unwrap_or(u32::MAX);
        let width = available / count;
        let mut grid = vec![width; columns.len()];
        if let Some(last) = grid.last_mut() {
            *last += available % count;
        }
        (grid, "<w:tblW w:w=\"0\" w:type=\"auto\"/>".into(), false)
    }
}

fn alignment(align: Align) -> Option<&'static str> {
    match align {
        Align::Default => None,
        Align::Left => Some("left"),
        Align::Center => Some("center"),
        Align::Right => Some("right"),
    }
}

impl Emitter<'_> {
    /// Emits editable rows, including explicit grid-span and merge cells.
    pub(crate) fn table(&mut self, block: &Block, figure: Option<&Block>) {
        let Block::Table { columns, head, foot, attrs, span, .. } = block
        else {
            return;
        };
        let (figure_attrs, caption) = match figure {
            Some(Block::Figure { attrs, caption, .. }) => (
                Some(attrs),
                caption.as_ref().map(|caption| plain_text(caption)),
            ),
            _ => (None, None),
        };
        let custom = attrs.pairs.get("custom-style").or_else(|| {
            figure_attrs.and_then(|attrs| attrs.pairs.get("custom-style"))
        });
        let style = match custom {
            Some(name) => self.styles.custom(name, StyleKind::Table, None),
            None => self.styles.ensure("Table", StyleKind::Table),
        };
        let (widths, width, fixed) =
            grid(columns, self.available_width_twips());
        let look = (u8::from(!head.is_empty()) * 0x20)
            | (u8::from(!foot.is_empty()) * 0x40);
        let properties = xml::TableProps {
            style: Some(style),
            width: Some(width),
            layout_fixed: fixed,
            look: Some(format!(
                "<w:tblLook w:firstRow=\"{}\" w:lastRow=\"{}\" w:firstColumn=\"0\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"0\" w:val=\"{look:04X}\"/>",
                u8::from(!head.is_empty()),
                u8::from(!foot.is_empty()),
            )),
            caption: caption.map(|caption| {
                format!("<w:tblCaption w:val=\"{}\"/>", xml::escape(&caption),)
            }),
            ..Default::default()
        };
        let plan = plan(block);
        for _ in 0..plan.dropped {
            let diagnostic = self.diagnostics.warn(
                "unsupported-construct",
                "table cell exceeds the column grid or overlaps a rowspan; cell dropped",
            );
            diagnostic.span =
                span.clone().or_else(|| self.content.span.clone());
            diagnostic.path = Some(
                diagnostic
                    .span
                    .as_ref()
                    .and_then(|span| span.file.clone())
                    .unwrap_or_else(|| self.doc.source.path.clone()),
            );
        }
        self.separate_table();
        self.buffer().push_str("<w:tbl>");
        self.buffer().push_str(&properties.to_xml());
        self.buffer().push_str("<w:tblGrid>");
        for width in &widths {
            self.buffer().push_str(&format!("<w:gridCol w:w=\"{width}\"/>"));
        }
        self.buffer().push_str("</w:tblGrid>");
        let compact = self.styles.ensure("Compact", StyleKind::Paragraph);
        for row in plan.rows {
            self.buffer().push_str("<w:tr>");
            let properties =
                xml::RowProps { header: row.head, ..Default::default() };
            self.buffer().push_str(&properties.to_xml());
            for cell in row.cells {
                let width = widths[cell.column..cell.column + cell.colspan]
                    .iter()
                    .copied()
                    .fold(0u32, u32::saturating_add);
                let source = match cell.content {
                    CellContent::Source(source) => Some(source),
                    _ => None,
                };
                let merge = match cell.content {
                    CellContent::Continuation => Some("continue"),
                    CellContent::Source(source) if source.rowspan > 1 => {
                        Some("restart")
                    }
                    _ => None,
                };
                let properties = xml::CellProps {
                    width: Some(format!(
                        "<w:tcW w:w=\"{width}\" w:type=\"dxa\"/>"
                    )),
                    grid_span: (cell.colspan > 1)
                        .then_some(cell.colspan as u32),
                    v_merge: merge,
                    ..Default::default()
                };
                self.buffer().push_str("<w:tc>");
                self.buffer().push_str(&properties.to_xml());
                let align = source
                    .and_then(|source| source.align)
                    .unwrap_or(columns[cell.column].align);
                let properties = xml::ParaProps {
                    style: Some(compact.clone()),
                    jc: alignment(align),
                    ..Default::default()
                };
                self.buffer().push_str("<w:p>");
                self.buffer().push_str(&properties.to_xml());
                if let Some(source) = source {
                    let previous_width = self.table_width_twips;
                    self.table_width_twips = Some(width.saturating_sub(216));
                    self.containers.push(Container::TableCell);
                    self.inlines(
                        &source.inlines,
                        &xml::RunProps {
                            bold: row.body && source.header,
                            ..Default::default()
                        },
                    );
                    self.containers.pop();
                    self.table_width_twips = previous_width;
                }
                self.buffer().push_str("</w:p></w:tc>");
            }
            self.buffer().push_str("</w:tr>");
        }
        self.buffer().push_str("</w:tbl>");
    }
}
