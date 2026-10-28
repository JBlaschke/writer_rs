//! Literal table columns, cells and bounded row-major occupancy.

use typst_syntax::ast::AstNode;
use typst_syntax::{SyntaxNode, ast};

use crate::ir::{
    Align, Attrs, Block, ColumnSpec, Inline, Inlines, Row, TableCell,
};

use super::Context;
use super::calls::{blocks, literal};
use super::literals::{Args, Literal};

const INVALID: &str = "E-TYP-ARG-INVALID";
/// Bound the structured output before any grid allocation; native PDF is separate.
const GRID_SLOTS: usize = 65_536;

pub(super) fn table<'a>(
    ctx: &mut Context<'a>, args: &Args<'a>,
) -> Option<Block> {
    args.check_named(
        ctx,
        &["columns", "align"],
        &[
            "rows", "stroke", "inset", "fill", "gutter", "column-gutter",
            "row-gutter",
        ],
    );
    let mut columns = columns(ctx, args.named("columns"), args.node)?;
    if let Some(expr) = args.named("align") {
        apply_alignment(ctx, expr, &mut columns);
    }
    let mut head = Vec::new();
    let mut body = Vec::new();
    let mut foot = Vec::new();
    let mut count = 0usize;
    for &expr in &args.positional {
        if ignored_layout(ctx, expr) {
            continue;
        }
        if let ast::Expr::FuncCall(call) = expr {
            let name = ctx.callee(call.callee());
            if matches!(name.as_deref(), Some("table.header" | "table.footer"))
            {
                let section = Args::new(ctx, call);
                section.check_named(
                    ctx,
                    &[],
                    &["repeat", "fill", "stroke", "inset"],
                );
                let header = name.as_deref() == Some("table.header");
                for &expr in &section.positional {
                    if ignored_layout(ctx, expr) {
                        continue;
                    }
                    if count == GRID_SLOTS {
                        budget(ctx, args.node);
                        return None;
                    }
                    count += 1;
                    let cell = cell(ctx, expr, header)?;
                    if header {
                        head.push(cell);
                    } else {
                        foot.push(cell);
                    }
                }
                continue;
            }
        }
        if count == GRID_SLOTS {
            budget(ctx, args.node);
            return None;
        }
        count += 1;
        body.push(cell(ctx, expr, false)?);
    }
    let mut remaining = GRID_SLOTS;
    let head = section(ctx, head, columns.len(), &mut remaining, args.node)?;
    let body = section(ctx, body, columns.len(), &mut remaining, args.node)?;
    let foot = section(ctx, foot, columns.len(), &mut remaining, args.node)?;
    Some(Block::Table {
        columns,
        head,
        body,
        foot,
        attrs: Attrs::default(),
        span: Some(ctx.span(args.node)),
    })
}

fn ignored_layout<'a>(ctx: &mut Context<'a>, expr: ast::Expr<'a>) -> bool {
    let ast::Expr::FuncCall(call) = expr else {
        return false;
    };
    if !matches!(
        ctx.callee(call.callee()).as_deref(),
        Some("table.hline" | "table.vline" | "v" | "h" | "colbreak")
    ) {
        return false;
    }
    // Even layout-only calls reject spread arguments, collected exactly once.
    let _ = Args::new(ctx, call);
    true
}

fn invalid(
    ctx: &mut Context<'_>, node: &SyntaxNode, message: impl Into<String>,
) {
    ctx.error(INVALID, message, node);
}

fn budget(ctx: &mut Context<'_>, node: &SyntaxNode) {
    invalid(ctx, node, "structured table grid exceeds the 65,536-slot limit");
}

fn columns<'a>(
    ctx: &mut Context<'a>, expr: Option<ast::Expr<'a>>, node: &SyntaxNode,
) -> Option<Vec<ColumnSpec>> {
    let Some(expr) = expr else {
        return Some(vec![ColumnSpec::default()]);
    };
    let Some(value) = literal(ctx, expr) else {
        return Some(vec![ColumnSpec::default()]);
    };
    let widths = match value {
        Literal::Int(value) if value > 0 => {
            let count = match usize::try_from(value) {
                Ok(count) if count <= GRID_SLOTS => count,
                _ => {
                    budget(ctx, expr.to_untyped());
                    return None;
                }
            };
            return Some(vec![ColumnSpec::default(); count]);
        }
        Literal::Array(values) if !values.is_empty() => {
            if values.len() > GRID_SLOTS {
                budget(ctx, node);
                return None;
            }
            match track_widths(&values) {
                Some(widths) => widths,
                None => {
                    invalid(
                        ctx,
                        expr.to_untyped(),
                        "columns require auto/percentage tracks or an all-fr array",
                    );
                    return Some(vec![ColumnSpec::default()]);
                }
            }
        }
        _ => {
            invalid(
                ctx,
                expr.to_untyped(),
                "columns require a positive integer or a nonempty track array",
            );
            return Some(vec![ColumnSpec::default()]);
        }
    };
    Some(
        widths
            .into_iter()
            .map(|width| ColumnSpec { width, ..ColumnSpec::default() })
            .collect(),
    )
}

fn track_widths(values: &[Literal<'_>]) -> Option<Vec<Option<f32>>> {
    let all_fr = values.iter().all(|value| {
        matches!(value, Literal::Numeric { unit: ast::Unit::Fr, .. })
    });
    if all_fr {
        let numbers: Vec<_> = values
            .iter()
            .map(|value| match value {
                Literal::Numeric { value, .. } => *value,
                _ => 0.0,
            })
            .collect();
        let sum: f64 = numbers.iter().sum();
        if !sum.is_finite()
            || sum <= 0.0
            || numbers.iter().any(|value| !value.is_finite() || *value < 0.0)
        {
            return None;
        }
        return Some(
            numbers
                .into_iter()
                .map(|value| Some((value / sum) as f32))
                .collect(),
        );
    }
    values
        .iter()
        .map(|value| match value {
            Literal::Auto => Some(None),
            Literal::Numeric { value, unit: ast::Unit::Percent, .. } => {
                let width = (*value / 100.0) as f32;
                width.is_finite().then_some(Some(width))
            }
            _ => None,
        })
        .collect()
}

/// Horizontal component; vertical-only values inherit the column's alignment.
pub(super) fn horizontal(symbol: &str) -> Option<Align> {
    symbol.split('+').map(str::trim).find_map(|part| match part {
        "left" | "start" => Some(Align::Left),
        "center" => Some(Align::Center),
        "right" | "end" => Some(Align::Right),
        _ => None,
    })
}

pub(super) fn alignment(value: &Literal<'_>) -> Option<Option<Align>> {
    match value {
        Literal::Auto => Some(None),
        Literal::Symbol(symbol) => {
            let mut horizontal_count = 0;
            let mut vertical_count = 0;
            for part in symbol.split('+').map(str::trim) {
                match part {
                    "left" | "start" | "center" | "right" | "end" => {
                        horizontal_count += 1
                    }
                    "top" | "bottom" | "horizon" => vertical_count += 1,
                    _ => return None,
                }
            }
            (horizontal_count <= 1 && vertical_count <= 1)
                .then(|| horizontal(symbol))
        }
        _ => None,
    }
}

fn apply_alignment<'a>(
    ctx: &mut Context<'a>, expr: ast::Expr<'a>, columns: &mut [ColumnSpec],
) {
    let Some(value) = literal(ctx, expr) else {
        return;
    };
    let values = match value {
        Literal::Array(values) => values,
        value => vec![value],
    };
    let Some(alignments) =
        values.iter().map(alignment).collect::<Option<Vec<_>>>()
    else {
        invalid(
            ctx,
            expr.to_untyped(),
            "table alignment requires horizontal/vertical alignment literals",
        );
        return;
    };
    for (index, column) in columns.iter_mut().enumerate() {
        column.align = index
            .checked_rem(alignments.len())
            .and_then(|index| alignments[index])
            .unwrap_or_default();
    }
}

fn cell<'a>(
    ctx: &mut Context<'a>, expr: ast::Expr<'a>, header: bool,
) -> Option<TableCell> {
    let mut cell = TableCell { header, ..TableCell::default() };
    let mut content = expr;
    if let ast::Expr::FuncCall(call) = expr
        && ctx.callee(call.callee()).as_deref() == Some("table.cell")
    {
        let args = Args::new(ctx, call);
        if args.positional.len() > 1 {
            invalid(ctx, args.node, "table.cell accepts one positional body");
        }
        args.check_named(
            ctx,
            &["colspan", "rowspan", "align"],
            &["repeat", "fill", "stroke", "inset"],
        );
        cell.colspan = span(ctx, args.named("colspan"))?;
        cell.rowspan = span(ctx, args.named("rowspan"))?;
        if let Some(expr) = args.named("align")
            && let Some(value) = literal(ctx, expr)
        {
            match alignment(&value) {
                Some(align) => cell.align = align,
                None => invalid(
                    ctx,
                    expr.to_untyped(),
                    "cell alignment requires an alignment or auto",
                ),
            }
        }
        content = match args.pos(0) {
            Some(body) => body,
            None => {
                invalid(ctx, args.node, "table.cell requires content");
                return Some(cell);
            }
        };
    }
    let Some(body) = blocks(ctx, content) else {
        return Some(cell);
    };
    cell.inlines = flatten(ctx, body, content.to_untyped());
    Some(cell)
}

fn span<'a>(ctx: &mut Context<'a>, expr: Option<ast::Expr<'a>>) -> Option<u32> {
    let Some(expr) = expr else { return Some(1) };
    match literal(ctx, expr) {
        Some(Literal::Int(value)) if value > 0 => {
            if value > GRID_SLOTS as i64 {
                budget(ctx, expr.to_untyped());
                return None;
            }
            Some(value as u32)
        }
        Some(_) => {
            invalid(
                ctx,
                expr.to_untyped(),
                "cell spans require a positive integer",
            );
            Some(1)
        }
        None => Some(1),
    }
}

fn flatten(
    ctx: &mut Context<'_>, blocks: Vec<Block>, node: &SyntaxNode,
) -> Inlines {
    let mut inlines = Vec::new();
    let mut paragraphs = 0;
    let mut dropped = false;
    for block in blocks {
        if let Block::Paragraph { inlines: body, .. } = block {
            if paragraphs != 0 {
                inlines.push(Inline::LineBreak);
            }
            inlines.extend(body);
            paragraphs += 1;
        } else {
            dropped = true;
        }
    }
    if dropped || paragraphs > 1 {
        ctx.warning("W-TYP-TABLE-CELL", "table cell keeps paragraphs joined by line breaks; other blocks are dropped", node);
    }
    inlines
}

fn section(
    ctx: &mut Context<'_>, cells: Vec<TableCell>, columns: usize,
    remaining: &mut usize, node: &SyntaxNode,
) -> Option<Vec<Row>> {
    match grid(cells, columns, *remaining) {
        Ok((rows, slots)) => {
            *remaining -= slots;
            Some(rows)
        }
        Err(GridError::Budget) => {
            budget(ctx, node);
            None
        }
        Err(GridError::Overlap) => {
            invalid(
                ctx,
                node,
                "table cell span crosses an occupied slot or the row boundary",
            );
            None
        }
    }
}

#[derive(Clone)]
enum Slot {
    Empty,
    Covered,
    Cell(TableCell),
}
#[derive(Debug, PartialEq, Eq)]
enum GridError {
    Budget,
    Overlap,
}

fn grid(
    cells: Vec<TableCell>, columns: usize, budget: usize,
) -> Result<(Vec<Row>, usize), GridError> {
    if columns == 0 {
        return Err(GridError::Budget);
    }
    if cells.is_empty() {
        return Ok((Vec::new(), 0));
    }
    if columns > budget {
        return Err(GridError::Budget);
    }
    let mut slots = Vec::<Slot>::new();
    let mut cursor = 0usize;
    let header = cells.first().is_some_and(|cell| cell.header);
    for cell in cells {
        if cell.colspan == 0 || cell.rowspan == 0 {
            return Err(GridError::Overlap);
        }
        let index = slots
            .get(cursor..)
            .and_then(|slots| {
                slots.iter().position(|slot| matches!(slot, Slot::Empty))
            })
            .map_or(slots.len().max(cursor), |offset| cursor + offset);
        let column = index % columns;
        let colspan = cell.colspan as usize;
        let rowspan = cell.rowspan as usize;
        if colspan > budget || rowspan > budget {
            return Err(GridError::Budget);
        }
        if colspan > columns - column {
            return Err(GridError::Overlap);
        }
        let rows =
            (index / columns).checked_add(rowspan).ok_or(GridError::Budget)?;
        let length = rows
            .checked_mul(columns)
            .filter(|length| *length <= budget)
            .ok_or(GridError::Budget)?;
        slots.resize_with(length.max(slots.len()), || Slot::Empty);
        for row in index / columns..rows {
            for column in column..column + colspan {
                if !matches!(slots[row * columns + column], Slot::Empty) {
                    return Err(GridError::Overlap);
                }
            }
        }
        for row in index / columns..rows {
            for column in column..column + colspan {
                slots[row * columns + column] = Slot::Covered;
            }
        }
        slots[index] = Slot::Cell(cell);
        cursor = index + colspan;
    }
    let count = slots.len();
    let mut rows = Vec::new();
    for chunk in slots.chunks(columns) {
        let cells = chunk
            .iter()
            .filter_map(|slot| match slot {
                Slot::Cell(cell) => Some(cell.clone()),
                Slot::Covered => None,
                Slot::Empty => {
                    Some(TableCell { header, ..TableCell::default() })
                }
            })
            .collect();
        rows.push(Row { cells });
    }
    Ok((rows, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> crate::ir::Parsed {
        use crate::config::Config;
        use crate::ir::{SourceInfo, SourceKind};

        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let source = SourceInfo {
            path: directory.join("table-call-test.typ"),
            base_dir: directory,
            kind: SourceKind::Typst,
        };
        let parsed =
            crate::typst::frontend::parse(&source, text, &Config::default())
                .unwrap();
        assert!(parsed.document.invariant_violations().is_empty());
        parsed
    }

    #[test]
    fn header_and_footer_layout_calls_do_not_create_empty_cells() {
        let parsed = parse(
            "#table(columns: 2, \
             table.header(table.hline(), [H1], h(1em), [H2]), \
             [B1], [B2], \
             table.footer(v(1em), [F1], table.vline(), [F2], colbreak()))",
        );
        assert!(!parsed.diagnostics.has_errors());
        let Block::Table { head, body, foot, .. } = &parsed.document.blocks[0]
        else {
            panic!("expected a table");
        };
        for (rows, expected) in
            [(head, ["H1", "H2"]), (body, ["B1", "B2"]), (foot, ["F1", "F2"])]
        {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].cells.len(), 2);
            let text: Vec<_> = rows[0]
                .cells
                .iter()
                .map(|cell| crate::ir::plain_text(&cell.inlines))
                .collect();
            assert_eq!(text, expected);
        }
    }

    #[test]
    fn nested_cell_and_term_calls_diagnose_extra_positional_arguments() {
        let parsed = parse(
            "#table(columns: 1, table.cell([Kept], [Ignored]))\n\n\
             #terms(terms.item([Term], [Definition], [Ignored]))",
        );
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .filter(|diagnostic| { diagnostic.code == INVALID })
                .count(),
            2
        );
        let Block::Table { body, .. } = &parsed.document.blocks[0] else {
            panic!("expected a table");
        };
        assert_eq!(crate::ir::plain_text(&body[0].cells[0].inlines), "Kept");
        let Block::DefinitionList { items, .. } = &parsed.document.blocks[1]
        else {
            panic!("expected a definition list");
        };
        assert_eq!(crate::ir::plain_text(&items[0].term), "Term");
        assert_eq!(items[0].definitions[0].len(), 1);
    }

    fn text(value: &str) -> TableCell {
        TableCell {
            inlines: vec![Inline::Text { text: value.into() }],
            ..TableCell::default()
        }
    }

    #[test]
    fn spans_skip_covered_slots_and_pad_the_final_row() {
        let cells = vec![
            TableCell { rowspan: 2, ..text("tall") },
            text("b"),
            text("c"),
            text("d"),
        ];
        let (rows, slots) = grid(cells, 3, GRID_SLOTS).unwrap();
        assert_eq!(slots, 6);
        assert_eq!(rows[0].cells.len(), 3);
        assert_eq!(rows[1].cells, [text("d"), TableCell::default()]);
        let (rows, slots) = grid(
            vec![TableCell { colspan: 2, rowspan: 2, ..text("all") }],
            2,
            GRID_SLOTS,
        )
        .unwrap();
        assert_eq!(slots, 4);
        assert_eq!(rows[1].cells.len(), 0);
    }

    #[test]
    fn impossible_spans_fail_before_relocation_or_large_allocation() {
        assert_eq!(
            grid(vec![TableCell { colspan: 3, ..text("wide") }], 2, GRID_SLOTS)
                .unwrap_err(),
            GridError::Overlap
        );
        assert_eq!(
            grid(
                vec![TableCell { rowspan: u32::MAX, ..text("huge") }],
                1,
                GRID_SLOTS
            )
            .unwrap_err(),
            GridError::Budget
        );
        assert_eq!(
            grid(vec![text("one")], GRID_SLOTS + 1, GRID_SLOTS).unwrap_err(),
            GridError::Budget
        );
        let cells = vec![
            text("a"),
            TableCell { rowspan: 2, ..text("tall") },
            text("c"),
            TableCell { colspan: 2, ..text("overlap") },
        ];
        assert_eq!(grid(cells, 3, GRID_SLOTS).unwrap_err(), GridError::Overlap);
    }

    #[test]
    fn each_section_resets_rowspan_occupancy_and_uses_the_remaining_budget() {
        let (head, used) = grid(
            vec![TableCell { rowspan: 2, header: true, ..text("head") }],
            2,
            8,
        )
        .unwrap();
        assert_eq!(head.len(), 2);
        assert!(head.iter().flat_map(|row| &row.cells).all(|cell| cell.header));
        let (body, used_body) = grid(vec![text("body")], 2, 8 - used).unwrap();
        assert_eq!(body[0].cells[0], text("body"));
        assert_eq!(used_body, 2);
        assert_eq!(
            grid(
                vec![TableCell { rowspan: 2, ..text("foot") }],
                2,
                8 - used - used_body
            )
            .unwrap_err(),
            GridError::Budget
        );
    }

    #[test]
    fn tracks_preserve_percentages_and_normalize_only_all_fr() {
        let numeric =
            |value, unit| Literal::Numeric { value, unit, text: String::new() };
        assert_eq!(
            track_widths(&[Literal::Auto, numeric(25.0, ast::Unit::Percent)]),
            Some(vec![None, Some(0.25)])
        );
        assert_eq!(
            track_widths(&[
                numeric(1.0, ast::Unit::Fr),
                numeric(3.0, ast::Unit::Fr)
            ]),
            Some(vec![Some(0.25), Some(0.75)])
        );
        assert_eq!(
            track_widths(&[Literal::Auto, numeric(1.0, ast::Unit::Fr)]),
            None
        );
        assert_eq!(track_widths(&[numeric(0.0, ast::Unit::Fr)]), None);
        assert_eq!(horizontal("top + right"), Some(Align::Right));
        assert_eq!(alignment(&Literal::Symbol("horizon".into())), Some(None));
        assert_eq!(alignment(&Literal::Auto), Some(None));
    }
}
