#![allow(dead_code)]

use pulldown_cmark::Alignment;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::renderer::{coalesce, to_chars, wrap_widths, Span, Style};

pub const MAX_TABLE_DEPTH: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text(Vec<Span>),
    Table(TableModel),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub header: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableModel {
    pub rows: Vec<Row>,
    pub aligns: Vec<Alignment>,
}

impl TableModel {
    pub fn ncols(&self) -> usize {
        let rows_max = self.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
        self.aligns.len().max(rows_max)
    }
}

pub fn wrap_spans(spans: &[Span], width: usize) -> Vec<Vec<Span>> {
    wrap_widths(&to_chars(spans), width)
        .into_iter()
        .map(|row| coalesce(&row))
        .collect()
}

pub(crate) fn fit_table_widths(natural: &[usize], available: usize) -> Vec<usize> {
    let n = natural.len();
    if n == 0 {
        return Vec::new();
    }
    if natural.iter().sum::<usize>() <= available {
        return natural.to_vec();
    }
    if available <= n {
        return vec![1; n];
    }
    let fits = |cap: usize| -> bool {
        natural.iter().map(|&w| w.min(cap)).sum::<usize>() <= available
    };
    let (mut lo, mut hi) = (1usize, *natural.iter().max().unwrap_or(&1));
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let mut widths: Vec<usize> = natural.iter().map(|&w| w.min(lo)).collect();
    let mut remaining = available - widths.iter().sum::<usize>();
    while remaining > 0 {
        let mut progressed = false;
        for (w, &nat) in widths.iter_mut().zip(natural.iter()) {
            if remaining == 0 {
                break;
            }
            if *w < nat {
                *w += 1;
                remaining -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    widths
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| UnicodeWidthStr::width(s.text.as_str())).sum()
}

pub fn layout_table(model: &TableModel, budget: usize) -> Vec<Vec<Span>> {
    layout_table_depth(model, budget.max(1), 0)
}

fn layout_table_depth(model: &TableModel, budget: usize, depth: usize) -> Vec<Vec<Span>> {
    let ncols = model.ncols();
    if ncols == 0 {
        return Vec::new();
    }
    let overhead = 1 + 3 * ncols;
    if budget < overhead + 2 * ncols {
        return flatten_table(model, budget);
    }
    let available = budget - overhead;

    let mut natural = vec![0usize; ncols];
    for r in &model.rows {
        for (i, cell) in r.cells.iter().enumerate() {
            if i < ncols {
                natural[i] = natural[i].max(cell_natural_width(cell, available, depth));
            }
        }
    }
    let widths = fit_table_widths(&natural, available);

    let rendered: Vec<Vec<Vec<Vec<Span>>>> = model
        .rows
        .iter()
        .map(|r| {
            (0..ncols)
                .map(|i| {
                    r.cells
                        .get(i)
                        .map(|c| render_cell(c, widths[i], depth))
                        .unwrap_or_else(|| vec![Vec::new()])
                })
                .collect()
        })
        .collect();

    let header_count = model.rows.iter().take_while(|r| r.header).count();
    let mut lines = vec![border_line("┌", "┬", "┐", &widths)];
    for (r, _) in model.rows.iter().enumerate() {
        let cell_lines = &rendered[r];
        let height = cell_lines.iter().map(|c| c.len()).max().unwrap_or(1);
        for li in 0..height {
            let cells: Vec<Vec<Span>> = cell_lines
                .iter()
                .map(|c| c.get(li).cloned().unwrap_or_default())
                .collect();
            lines.push(grid_row(&cells, &widths, &model.aligns));
        }
        if header_count > 0 && r + 1 == header_count {
            lines.push(border_line("├", "┼", "┤", &widths));
        }
    }
    lines.push(border_line("└", "┴", "┘", &widths));
    lines
}

fn block_natural_width(block: &Block, cap: usize, depth: usize) -> usize {
    match block {
        Block::Text(spans) => spans_width(spans).min(cap),
        Block::Table(t) => {
            if depth + 1 >= MAX_TABLE_DEPTH {
                flattened_natural_width(block, cap)
            } else {
                table_natural_width(t, cap, depth + 1)
            }
        }
    }
}

fn flattened_natural_width(block: &Block, cap: usize) -> usize {
    let inline = inline_of_blocks(std::slice::from_ref(block));
    let mut best = 0usize;
    let mut cur = 0usize;
    for sp in &inline {
        for c in sp.text.chars() {
            if c.is_whitespace() {
                best = best.max(cur);
                cur = 0;
            } else {
                cur += UnicodeWidthChar::width(c).unwrap_or(0);
            }
        }
    }
    best.max(cur).min(cap)
}

fn cell_natural_width(cell: &Cell, cap: usize, depth: usize) -> usize {
    cell.blocks
        .iter()
        .map(|b| block_natural_width(b, cap, depth))
        .max()
        .unwrap_or(0)
}

fn table_natural_width(model: &TableModel, cap: usize, depth: usize) -> usize {
    let ncols = model.ncols();
    if ncols == 0 {
        return 0;
    }
    let overhead = 1 + 3 * ncols;
    let avail = cap.saturating_sub(overhead);
    if avail < 2 * ncols {
        return cap;
    }
    let mut cols = vec![0usize; ncols];
    for r in &model.rows {
        for (i, cell) in r.cells.iter().enumerate() {
            if i < ncols {
                cols[i] = cols[i].max(cell_natural_width(cell, avail, depth));
            }
        }
    }
    (overhead + cols.iter().map(|&c| c.max(2)).sum::<usize>()).min(cap)
}

fn render_cell(cell: &Cell, width: usize, depth: usize) -> Vec<Vec<Span>> {
    let mut out: Vec<Vec<Span>> = Vec::new();
    for block in &cell.blocks {
        match block {
            Block::Text(spans) => out.extend(wrap_spans(spans, width)),
            Block::Table(inner) => {
                let lines = if depth + 1 >= MAX_TABLE_DEPTH {
                    flatten_table(inner, width)
                } else {
                    layout_table_depth(inner, width, depth + 1)
                };
                out.extend(lines);
            }
        }
    }
    if out.is_empty() {
        out.push(Vec::new());
    }
    out
}

fn border_line(l: &str, m: &str, r: &str, widths: &[usize]) -> Vec<Span> {
    let mut s = String::new();
    s.push_str(l);
    for (i, w) in widths.iter().enumerate() {
        s.push_str(&"─".repeat(w + 2));
        s.push_str(if i + 1 < widths.len() { m } else { r });
    }
    vec![Span { text: s, style: Style::default() }]
}

fn grid_row(cells: &[Vec<Span>], widths: &[usize], aligns: &[Alignment]) -> Vec<Span> {
    let mut line: Vec<Span> = Vec::new();
    line.push(Span { text: "│".to_string(), style: Style::default() });
    for (i, w) in widths.iter().enumerate() {
        let empty: Vec<Span> = Vec::new();
        let content = cells.get(i).unwrap_or(&empty);
        let cw = spans_width(content);
        let pad = w.saturating_sub(cw);
        let (lp, rp) = match aligns.get(i) {
            Some(Alignment::Right) => (pad, 0),
            Some(Alignment::Center) => (pad / 2, pad - pad / 2),
            _ => (0, pad),
        };
        line.push(Span { text: " ".to_string(), style: Style::default() });
        if lp > 0 {
            line.push(Span { text: " ".repeat(lp), style: Style::default() });
        }
        line.extend(content.iter().cloned());
        if rp > 0 {
            line.push(Span { text: " ".repeat(rp), style: Style::default() });
        }
        line.push(Span { text: " ".to_string(), style: Style::default() });
        line.push(Span { text: "│".to_string(), style: Style::default() });
    }
    line
}

fn plain_span(text: &str) -> Span {
    Span { text: text.to_string(), style: Style::default() }
}

fn inline_of_blocks(blocks: &[Block]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for b in blocks {
        match b {
            Block::Text(spans) => out.extend(spans.iter().cloned()),
            Block::Table(t) => {
                for (ri, r) in t.rows.iter().enumerate() {
                    if ri > 0 {
                        out.push(plain_span(" / "));
                    }
                    for (ci, c) in r.cells.iter().enumerate() {
                        if ci > 0 {
                            out.push(plain_span(" | "));
                        }
                        out.extend(inline_of_blocks(&c.blocks));
                    }
                }
            }
        }
    }
    out
}

fn flatten_table(model: &TableModel, width: usize) -> Vec<Vec<Span>> {
    let mut out = Vec::new();
    for r in &model.rows {
        let mut spans: Vec<Span> = Vec::new();
        for (i, cell) in r.cells.iter().enumerate() {
            if i > 0 {
                spans.push(plain_span(" | "));
            }
            spans.extend(inline_of_blocks(&cell.blocks));
        }
        out.extend(wrap_spans(&spans, width.max(1)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::Style;
    use crate::renderer::Style as St;

    fn sp(text: &str) -> Span {
        Span { text: text.to_string(), style: Style::default() }
    }

    fn styled(text: &str, style: Style) -> Span {
        Span { text: text.to_string(), style }
    }

    fn strings(rows: Vec<Vec<Span>>) -> Vec<String> {
        rows.into_iter()
            .map(|r| r.into_iter().map(|s| s.text).collect::<String>())
            .collect()
    }

    #[test]
    fn wrap_spans_word_wraps() {
        let spans = vec![sp("one two three four")];
        assert_eq!(strings(wrap_spans(&spans, 7)), vec!["one two", "three", "four"]);
    }

    #[test]
    fn wrap_spans_empty_yields_one_empty_line() {
        assert_eq!(strings(wrap_spans(&[], 10)), vec![""]);
    }

    #[test]
    fn spans_width_counts_cjk_as_two() {
        assert_eq!(spans_width(&[sp("中a")]), 3);
    }

    fn text_cell(s: &str) -> Cell {
        Cell { blocks: vec![Block::Text(vec![sp(s)])] }
    }

    fn row(cells: &[&str], header: bool) -> Row {
        Row { cells: cells.iter().map(|c| text_cell(c)).collect(), header }
    }

    fn layout_strings(model: &TableModel, budget: usize) -> Vec<String> {
        layout_table(model, budget)
            .into_iter()
            .map(|r| r.into_iter().map(|s| s.text).collect::<String>())
            .collect()
    }

    #[test]
    fn layout_flat_matches_gfm_shape() {
        let model = TableModel {
            rows: vec![row(&["a", "b"], true), row(&["1", "2"], false)],
            aligns: vec![],
        };
        assert_eq!(layout_strings(&model, 80), vec![
            "┌───┬───┐",
            "│ a │ b │",
            "├───┼───┤",
            "│ 1 │ 2 │",
            "└───┴───┘",
        ]);
    }

    #[test]
    fn layout_flat_cjk_aligns() {
        let model = TableModel {
            rows: vec![row(&["名称", "value"], true), row(&["中文", "abc"], false)],
            aligns: vec![],
        };
        assert_eq!(layout_strings(&model, 80), vec![
            "┌──────┬───────┐",
            "│ 名称 │ value │",
            "├──────┼───────┤",
            "│ 中文 │ abc   │",
            "└──────┴───────┘",
        ]);
    }

    use pulldown_cmark::Alignment;

    #[test]
    fn layout_applies_alignment() {
        let model = TableModel {
            rows: vec![row(&["left", "center", "right"], true), row(&["a", "b", "c"], false)],
            aligns: vec![Alignment::None, Alignment::Center, Alignment::Right],
        };
        assert_eq!(layout_strings(&model, 80), vec![
            "┌──────┬────────┬───────┐",
            "│ left │ center │ right │",
            "├──────┼────────┼───────┤",
            "│ a    │   b    │     c │",
            "└──────┴────────┴───────┘",
        ]);
    }

    #[test]
    fn too_narrow_table_degrades_to_text() {
        let model = TableModel {
            rows: vec![row(&["aaa", "bbb"], true), row(&["1", "2"], false)],
            aligns: vec![],
        };
        let out = layout_strings(&model, 4);
        assert!(out.iter().all(|l| !l.contains('│') && !l.contains('┌')));
        assert!(out.iter().any(|l| l.contains("aaa")));
    }

    fn inner_table() -> TableModel {
        TableModel {
            rows: vec![row(&["x", "y"], true), row(&["1", "2"], false)],
            aligns: vec![],
        }
    }

    fn outer_with_inner() -> TableModel {
        let inner = Block::Table(inner_table());
        TableModel {
            rows: vec![
                Row { cells: vec![text_cell("head"), Cell { blocks: vec![inner.clone()] }], header: true },
                Row { cells: vec![text_cell("a"), text_cell("b")], header: false },
            ],
            aligns: vec![],
        }
    }

    #[test]
    fn nested_table_is_embedded_in_cell() {
        let out = layout_strings(&outer_with_inner(), 80);
        let widths: Vec<usize> = out.iter().map(|l| UnicodeWidthStr::width(l.as_str())).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "ragged nested table: {:?}", out);
        assert!(out.iter().filter(|l| l.contains('┌')).count() >= 2, "inner border missing: {:?}", out);
        assert!(out.iter().filter(|l| l.contains('└')).count() >= 2, "inner border missing: {:?}", out);
        assert!(out.len() > 5, "outer height must expand: {:?}", out);
    }

    #[test]
    fn nested_table_degrades_past_depth_limit() {
        let mut model = inner_table();
        for _ in 0..(MAX_TABLE_DEPTH + 2) {
            let inner = Block::Table(model);
            model = TableModel {
                rows: vec![Row { cells: vec![Cell { blocks: vec![inner] }], header: false }],
                aligns: vec![],
            };
        }
        let out = layout_strings(&model, 80);
        assert!(!out.is_empty());
        assert!(out.iter().all(|l| UnicodeWidthStr::width(l.as_str()) <= 80));
        assert!(out.iter().any(|l| l.contains('x')), "degraded content lost: {:?}", out);
        assert!(
            out.iter().filter(|l| l.contains('┌')).count() <= MAX_TABLE_DEPTH,
            "nesting not degraded: {:?}",
            out
        );
    }

    #[test]
    fn nested_table_shrinks_to_narrow_budget() {
        let out = layout_strings(&outer_with_inner(), 12);
        let widths: Vec<usize> = out.iter().map(|l| UnicodeWidthStr::width(l.as_str())).collect();
        assert!(widths.iter().all(|w| *w <= 12), "overflow at narrow budget: {:?}", out);
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "ragged at narrow budget: {:?}", out);
    }

    #[test]
    fn nested_layout_preserves_inline_style() {
        let outer = TableModel {
            rows: vec![Row {
                cells: vec![Cell {
                    blocks: vec![Block::Text(vec![styled("bold", St { bold: true, ..St::default() })])],
                }],
                header: false,
            }],
            aligns: vec![],
        };
        let lines = layout_table(&outer, 40);
        assert!(lines.iter().flatten().any(|s| s.style.bold && s.text.contains("bold")));
    }
}
