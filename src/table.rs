#![expect(dead_code)]

use pulldown_cmark::Alignment;
use unicode_width::UnicodeWidthStr;

use crate::renderer::{coalesce, to_chars, wrap_widths, Span};

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
        self.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0)
    }
}

pub fn wrap_spans(spans: &[Span], width: usize) -> Vec<Vec<Span>> {
    wrap_widths(&to_chars(spans), width)
        .into_iter()
        .map(|row| coalesce(&row))
        .collect()
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| UnicodeWidthStr::width(s.text.as_str())).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::Style;

    fn sp(text: &str) -> Span {
        Span { text: text.to_string(), style: Style::default() }
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
}
