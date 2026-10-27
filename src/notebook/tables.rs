//! Plain notebook HTML tables and pandas grid conversion (notebook-mapping.md §4.4).

use scraper::{ElementRef, Html, Node};

use crate::ir::{Attrs, Block, ColumnSpec, Inline, Row, TableCell, plain_text};

const MAX_GRID_SLOTS: usize = 1_000_000;

/// A portable table extracted from a plain HTML notebook representation.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlTable {
    /// The table, without HTML presentation attributes.
    pub table: Block,
    /// Trimmed HTML caption, when present.
    pub caption: Option<String>,
    /// A pandas truncation note following the table, when present.
    pub note: Option<String>,
}

/// Extracts a plain table or pandas wrapper, retaining safe inline formatting.
pub fn parse_html_table(html: &str) -> Option<HtmlTable> {
    let fragment = Html::parse_fragment(html);
    let mut outer = significant_children(fragment.root_element())?;
    outer.retain(|element| element.value().name() != "style");
    if let [wrapper] = outer.as_slice()
        && wrapper.value().name() == "div"
    {
        outer = significant_children(*wrapper)?;
        outer.retain(|element| element.value().name() != "style");
    }
    let (table, note) = match outer.as_slice() {
        [table] if table.value().name() == "table" => (*table, None),
        [table, note]
            if table.value().name() == "table"
                && note.value().name() == "p" =>
        {
            let text = note.text().collect::<String>();
            let text = text.trim();
            if !truncation_note(text) {
                return None;
            }
            (*table, Some(text.to_owned()))
        }
        _ => return None,
    };
    if table
        .descendants()
        .filter_map(ElementRef::wrap)
        .any(|element| element != table && element.value().name() == "table")
    {
        return None;
    }
    let mut head = Vec::new();
    let mut body = Vec::new();
    let mut foot = Vec::new();
    let mut caption = None;
    let mut has_head = false;
    for element in significant_children(table)? {
        match element.value().name() {
            "caption" => {
                caption =
                    Some(element.text().collect::<String>().trim().into());
            }
            "colgroup" => {}
            "thead" => {
                has_head = true;
                head.extend(parse_rows(element)?);
            }
            "tbody" => body.extend(parse_rows(element)?),
            "tfoot" => foot.extend(parse_rows(element)?),
            _ => return None,
        }
    }
    if head.is_empty() && body.is_empty() && foot.is_empty() {
        return None;
    }
    if !has_head
        && body.first().is_some_and(|row| {
            !row.cells.is_empty() && row.cells.iter().all(|cell| cell.header)
        })
    {
        head.push(body.remove(0));
    }
    let rows = head.len().checked_add(body.len())?.checked_add(foot.len())?;
    let max_width = MAX_GRID_SLOTS / rows;
    let width = grid_width(&head, max_width)?
        .max(grid_width(&body, max_width)?)
        .max(grid_width(&foot, max_width)?);
    if width.checked_mul(rows)? > MAX_GRID_SLOTS {
        return None;
    }
    pad_rows(&mut head, width)?;
    pad_rows(&mut body, width)?;
    pad_rows(&mut foot, width)?;
    Some(HtmlTable {
        table: Block::Table {
            columns: vec![ColumnSpec::default(); width],
            head,
            body,
            foot,
            attrs: Attrs::default(),
            span: None,
        },
        caption,
        note,
    })
}

fn significant_children(
    element: ElementRef<'_>,
) -> Option<Vec<ElementRef<'_>>> {
    element
        .children()
        .filter_map(|node| match node.value() {
            Node::Comment(_) => None,
            Node::Text(text) if text.trim().is_empty() => None,
            Node::Element(_) => Some(ElementRef::wrap(node)),
            _ => Some(None),
        })
        .collect()
}

fn truncation_note(text: &str) -> bool {
    let Some((rows, columns)) = text.split_once(" rows × ") else {
        return false;
    };
    let Some(columns) = columns.strip_suffix(" columns") else {
        return false;
    };
    [rows, columns].iter().all(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn parse_rows(section: ElementRef<'_>) -> Option<Vec<Row>> {
    significant_children(section)?
        .into_iter()
        .map(|row| {
            if row.value().name() != "tr" {
                return None;
            }
            let cells = significant_children(row)?
                .into_iter()
                .map(|cell| {
                    if !matches!(cell.value().name(), "th" | "td") {
                        return None;
                    }
                    let mut whitespace = false;
                    let mut inlines = inline_children(cell, &mut whitespace)?;
                    trim_inlines(&mut inlines, true);
                    trim_inlines(&mut inlines, false);
                    Some(TableCell {
                        header: cell.value().name() == "th",
                        colspan: span(cell, "colspan"),
                        rowspan: span(cell, "rowspan"),
                        align: None,
                        inlines,
                    })
                })
                .collect::<Option<_>>()?;
            Some(Row { cells })
        })
        .collect()
}

fn span(cell: ElementRef<'_>, name: &str) -> u32 {
    cell.value()
        .attr(name)
        .and_then(|text| text.trim().parse().ok())
        .filter(|span| *span > 0)
        .unwrap_or(1)
}

fn inline_children(
    parent: ElementRef<'_>, whitespace: &mut bool,
) -> Option<Vec<Inline>> {
    let children: Vec<_> = parent.children().collect();
    let last = children.iter().rposition(|node| match node.value() {
        Node::Comment(_) => false,
        Node::Text(text) => !text.trim().is_empty(),
        _ => true,
    });
    let mut inlines = Vec::new();
    for (index, node) in children.into_iter().enumerate() {
        match node.value() {
            Node::Comment(_) => {}
            Node::Text(text) => {
                let mut collapsed = String::new();
                for character in text.chars() {
                    let is_space = character.is_ascii_whitespace();
                    if !is_space || !*whitespace {
                        collapsed.push(if is_space { ' ' } else { character });
                    }
                    *whitespace = is_space;
                }
                push_text(&mut inlines, collapsed);
            }
            Node::Element(_) => {
                let element = ElementRef::wrap(node)?;
                let name = element.value().name();
                if !matches!(
                    name,
                    "br" | "p"
                        | "div"
                        | "b"
                        | "strong"
                        | "i"
                        | "em"
                        | "code"
                        | "a"
                        | "span"
                        | "sub"
                        | "sup"
                ) {
                    return None;
                }
                let content = inline_children(element, whitespace)?;
                match name {
                    "br" => {
                        inlines.push(Inline::LineBreak);
                        *whitespace = false;
                    }
                    "p" | "div" => {
                        inlines.extend(content);
                        if Some(index) != last {
                            inlines.push(Inline::LineBreak);
                            *whitespace = false;
                        }
                    }
                    "b" | "strong" => {
                        inlines.push(Inline::Strong { inlines: content })
                    }
                    "i" | "em" => {
                        inlines.push(Inline::Emph { inlines: content })
                    }
                    "sub" => {
                        inlines.push(Inline::Subscript { inlines: content })
                    }
                    "sup" => {
                        inlines.push(Inline::Superscript { inlines: content })
                    }
                    "code" => inlines.push(Inline::Code {
                        text: plain_text(&content),
                        attrs: Attrs::default(),
                    }),
                    "a" => inlines.push(Inline::Link {
                        inlines: content,
                        url: element.value().attr("href").unwrap_or("").into(),
                        title: element.value().attr("title").map(str::to_owned),
                        attrs: Attrs::default(),
                    }),
                    "span" => inlines.extend(content),
                    _ => unreachable!("validated cell element"),
                }
            }
            _ => return None,
        }
    }
    Some(inlines)
}

fn push_text(inlines: &mut Vec<Inline>, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some(Inline::Text { text: previous }) = inlines.last_mut() {
        previous.push_str(&text);
    } else {
        inlines.push(Inline::Text { text });
    }
}

fn trim_inlines(inlines: &mut Vec<Inline>, start: bool) {
    loop {
        let index = if start {
            0
        } else {
            inlines.len().saturating_sub(1)
        };
        let Some(inline) = inlines.get_mut(index) else {
            return;
        };
        let empty = match inline {
            Inline::Text { text } | Inline::Code { text, .. } => {
                *text = if start {
                    text.trim_start()
                } else {
                    text.trim_end()
                }
                .into();
                text.is_empty()
            }
            Inline::Strong { inlines }
            | Inline::Emph { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Link { inlines, .. } => {
                trim_inlines(inlines, start);
                inlines.is_empty()
            }
            _ => false,
        };
        if empty {
            inlines.remove(index);
        } else {
            return;
        }
    }
}

// Each section starts its own rowspan grid, as HTML row groups and IR require.
fn grid_width(rows: &[Row], max_width: usize) -> Option<usize> {
    let mut remaining = Vec::new();
    let mut width = 0;
    for row in rows {
        place_row(row, &mut remaining, max_width)?;
        width = width.max(remaining.len());
        for height in &mut remaining {
            *height = height.saturating_sub(1);
        }
    }
    Some(width)
}

fn place_row(
    row: &Row, remaining: &mut Vec<u32>, max_width: usize,
) -> Option<()> {
    let mut cursor = 0;
    for cell in &row.cells {
        while remaining.get(cursor).is_some_and(|height| *height > 0) {
            cursor += 1;
        }
        let start = cursor;
        let end = start.checked_add(usize::try_from(cell.colspan).ok()?)?;
        if end > max_width {
            return None;
        }
        if end > remaining.len() {
            remaining.try_reserve(end - remaining.len()).ok()?;
            remaining.resize(end, 0);
        }
        if remaining[start..end].iter().any(|height| *height > 0) {
            return None;
        }
        remaining[start..end].fill(cell.rowspan);
        cursor = end;
    }
    Some(())
}

fn pad_rows(rows: &mut [Row], width: usize) -> Option<()> {
    let mut remaining = vec![0; width];
    for row in rows {
        place_row(row, &mut remaining, width)?;
        let header = row.cells.last().is_some_and(|cell| cell.header);
        for height in &mut remaining {
            if *height == 0 {
                row.cells.push(TableCell { header, ..TableCell::default() });
            }
            *height = height.saturating_sub(1);
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(html: &str) -> (Vec<ColumnSpec>, Vec<Row>, Vec<Row>, Vec<Row>) {
        let Block::Table { columns, head, body, foot, .. } =
            parse_html_table(html).unwrap().table
        else {
            panic!("HTML table maps to a table block");
        };
        (columns, head, body, foot)
    }

    #[test]
    fn pandas_simple() {
        let (columns, head, body, foot) = table(
            "<table border=\"1\" class=\"dataframe\"><thead><tr><th></th><th>A</th><th>B</th></tr></thead><tbody><tr><th>0</th><td>1</td><td>2</td></tr><tr><th>1</th><td>3</td><td>4</td></tr></tbody></table>",
        );
        assert_eq!(columns, vec![ColumnSpec::default(); 3]);
        assert_eq!(head.len(), 1);
        assert_eq!(body.len(), 2);
        assert!(foot.is_empty());
        assert!(head[0].cells.iter().all(|cell| cell.header));
        assert_eq!(plain_text(&body[0].cells[1].inlines), "1");
    }

    #[test]
    fn repr_html_div_with_style() {
        assert!(parse_html_table("<!--pandas--><style scoped>.x{}</style><div><style scoped>.dataframe{}</style><table><tr><td>1</td></tr></table></div>").is_some());
        for html in [
            "text<table><tr><td>1</td></tr></table>",
            "<div><div><table><tr><td>1</td></tr></table></div></div>",
            "<table><tr><td>1</td></tr></table><p>unexpected note</p>",
            "<table></table>",
        ] {
            assert!(parse_html_table(html).is_none(), "{html}");
        }
    }

    #[test]
    fn multiindex_grid() {
        let (columns, head, body, _) = table(
            "<table><thead><tr><th></th><th></th><th colspan=\"2\">a</th><th>b</th></tr><tr><th></th><th></th><th>x</th><th>y</th><th>x</th></tr><tr><th>outer</th><th>inner</th><th></th><th></th><th></th></tr></thead><tbody><tr><th rowspan=\"2\">one</th><th>a</th><td>1</td><td>2</td><td>3</td></tr><tr><th>b</th><td>4</td><td>5</td><td>6</td></tr><tr><th>two</th><th>a</th><td>7</td></tr></tbody></table>",
        );
        assert_eq!(columns.len(), 5);
        assert_eq!(head.len(), 3);
        assert_eq!(head[0].cells[2].colspan, 2);
        assert_eq!(body[0].cells[0].rowspan, 2);
        assert_eq!(body[1].cells.len(), 4);
        assert_eq!(body[2].cells.len(), 5);
        assert!(body[2].cells[4].inlines.is_empty());
        assert!(!body[2].cells[4].header);
    }

    #[test]
    fn truncated_note() {
        let parsed = parse_html_table("<div><table><tr><td>…</td></tr></table><p>100 rows × 2 columns</p></div>").unwrap();
        assert_eq!(parsed.note.as_deref(), Some("100 rows × 2 columns"));
        assert!(!truncation_note("100 rows x 2 columns"));
        assert!(!truncation_note("-1 rows × 2 columns"));
    }

    #[test]
    fn styler_caption_and_nbsp() {
        let parsed = parse_html_table("<style>.row{}</style><table id=\"T_x\"><caption> Styled caption </caption><thead><tr><th>&nbsp;</th><th>A&nbsp;B</th></tr></thead><tbody><tr><th><b>&nbsp;</b></th><td> \t A  <strong> bold </strong><em> italic</em><code>x&lt;y</code><sub>2</sub><sup>3</sup><a href=\"../x?a=1&amp;b=2\" title=\"t\"> link </a><br><p>one</p><div>two</div> </td></tr></tbody></table>").unwrap();
        assert_eq!(parsed.caption.as_deref(), Some("Styled caption"));
        let Block::Table { head, body, attrs, .. } = parsed.table else {
            panic!()
        };
        assert_eq!(attrs, Attrs::default());
        assert!(head[0].cells[0].inlines.is_empty());
        assert!(body[0].cells[0].inlines.is_empty());
        assert_eq!(plain_text(&head[0].cells[1].inlines), "A\u{a0}B");
        let inlines = &body[0].cells[1].inlines;
        assert!(
            inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Strong { .. }))
        );
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Link { url, title: Some(title), .. } if url == "../x?a=1&b=2" && title == "t")));
        assert_eq!(
            inlines
                .iter()
                .filter(|inline| matches!(inline, Inline::LineBreak))
                .count(),
            2
        );
        assert!(plain_text(inlines).starts_with("A bold italic"));
    }

    #[test]
    fn unescaped_styler_rejects() {
        for content in [
            "b<c", "<img src=\"x\">", "<svg/>", "<script>x</script>",
            "<style>x</style>", "<input>",
        ] {
            assert!(
                parse_html_table(&format!(
                    "<table><tr><td>{content}</td></tr></table>"
                ))
                .is_none(),
                "{content}"
            );
        }
    }

    #[test]
    fn nested_table_rejects() {
        assert!(parse_html_table("<table><tr><td><table><tr><td>x</td></tr></table></td></tr></table>").is_none());
    }

    #[test]
    fn bare_tr_without_thead() {
        let (columns, head, body, foot) = table(
            "<table><tr><th colspan=\"0\">A</th><th>B</th></tr><tr><td colspan=\"bogus\" rowspan=\"-1\">1</td></tr><tfoot><tr><td colspan=\"2\">total</td></tr></tfoot></table>",
        );
        assert_eq!(columns.len(), 2);
        assert_eq!(head.len(), 1);
        assert_eq!(body[0].cells.len(), 2);
        assert_eq!(body[0].cells[0].rowspan, 1);
        assert_eq!(foot[0].cells[0].colspan, 2);
    }

    #[test]
    fn excessive_or_overlapping_grids_fall_back() {
        for html in [
            "<table><tr><td colspan=\"4294967295\">x</td></tr></table>",
            "<table><tr><td colspan=\"500001\">x</td></tr><tr><td>y</td></tr></table>",
            "<table><tr><td>x</td><td rowspan=\"2\">y</td></tr><tr><td colspan=\"2\">z</td></tr></table>",
        ] {
            assert!(parse_html_table(html).is_none(), "{html}");
        }
        let (columns, _, body, _) = table(
            "<table><tr><td rowspan=\"4294967295\">x</td><td>y</td></tr><tr><td>z</td></tr></table>",
        );
        assert_eq!(columns.len(), 2);
        assert_eq!(body[1].cells.len(), 1);
    }
}
