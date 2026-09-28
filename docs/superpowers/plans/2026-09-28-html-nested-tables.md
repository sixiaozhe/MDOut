# HTML 嵌套表格渲染 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 mdout 把 HTML 块里的 `<table>`（含任意层嵌套）渲染成真网格终端表格，并保持流式实时重绘。

**Architecture:** 新增 `src/html.rs`（手写 HTML 分词/树构建 → `Vec<HtmlPiece>`）与 `src/table.rs`（递归 `TableModel` + 纯函数 `layout_table`）。`renderer.rs` 在 `Tag::HtmlBlock` 期间缓冲原始 HTML，`TagEnd::HtmlBlock` 时解析；GFM 与 HTML 表格统一走 `layout_table`。未闭合的表格按 `<table>`/`</table>` 配平标为 live。

**Tech Stack:** Rust 2021、`pulldown-cmark` 0.12、`unicode-width` 0.2，无新增依赖。

**设计文档:** `docs/superpowers/specs/2026-09-28-html-nested-tables-design.md`

**约定:** 每个任务结束都运行 `cargo test --bin mdout` 与 `cargo clippy --all-targets -- -D warnings`，全绿后提交。所有测试在 `src/*.rs` 内的 `#[cfg(test)] mod tests` 中（本项目是 binary crate，无 lib target，故用 `--bin mdout`）。

---

### Task 1: `src/table.rs` 模型与 `wrap_spans`

**Files:**
- Create: `src/table.rs`
- Modify: `src/main.rs:1-6`（新增模块声明）
- Modify: `src/renderer.rs:86-112`（把 `char_width`/`to_chars`/`coalesce` 改为 `pub(crate)`）

- [ ] **Step 1: 新建 `src/table.rs`（模型 + 类型 + wrap_spans）**

```rust
use pulldown_cmark::Alignment;
use unicode_width::UnicodeWidthStr;

use crate::renderer::{coalesce, to_chars, wrap_widths, Span, Style};

pub const MAX_TABLE_DEPTH: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Text(Vec<Span>),
    Table(TableModel),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub header: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
```

- [ ] **Step 2: 在 `src/main.rs` 顶部按字母序插入模块声明**

本任务只加 `mod table;`（`mod html;` 留给 Task 6 再加，避免中间态引用不存在的模块）。开头改为：

```rust
mod app;
mod cli;
mod input;
mod parser;
mod renderer;
mod table;
mod terminal;
```

- [ ] **Step 3: 在 `src/renderer.rs` 暴露三个纯函数**

只改可见性，**函数体不动**：在 `fn char_width`、`fn to_chars`、`fn coalesce`、`fn wrap_widths` 的 `fn` 前加 `pub(crate) `。例如：

```rust
pub(crate) fn to_chars(spans: &[Span]) -> Vec<(char, Style)> {
    // 函数体保持原样
}
```

`char_width`、`coalesce`、`wrap_widths` 同理，只加 `pub(crate) ` 前缀，其余一律保持现有实现。

- [ ] **Step 4: 运行测试**

Run: `cargo test --bin mdout table::tests`
Expected: 3 个测试通过（`wrap_spans_word_wraps`、`wrap_spans_empty_yields_one_empty_line`、`spans_width_counts_cjk_as_two`）。

- [ ] **Step 5: clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 无警告。

- [ ] **Step 6: 提交**

```bash
git add src/table.rs src/main.rs src/renderer.rs
git commit -m "feat: add recursive table model and span wrapping"
```

---

### Task 2: 把 `fit_table_widths` 移到 `table.rs`

**Files:**
- Modify: `src/renderer.rs`（删除 `fn fit_table_widths`，改用 `crate::table::fit_table_widths`）
- Modify: `src/table.rs`（移入该函数）

- [ ] **Step 1: 迁移函数**

把 `src/renderer.rs` 中的 `fn fit_table_widths(...)` 整段剪切到 `src/table.rs`，可见性改为 `pub(crate)`，并把 `div_ceil` 版本保留：

```rust
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
```

- [ ] **Step 2: 在 `renderer.rs` 的 `emit_table` 中改调用**

把 `fit_table_widths(&natural, available)` 改为 `crate::table::fit_table_widths(&natural, available)`。

- [ ] **Step 3: 运行全部测试**

Run: `cargo test --bin mdout`
Expected: 全部通过（表格快照不变）。

- [ ] **Step 4: 提交**

```bash
git add src/renderer.rs src/table.rs
git commit -m "refactor: move fit_table_widths into table module"
```

---

### Task 3: `layout_table`（仅 `Block::Text`，无嵌套）

**Files:**
- Modify: `src/table.rs`

- [ ] **Step 1: 写失败测试**

在 `src/table.rs` 的 `mod tests` 末尾追加：

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --bin mdout table::tests::layout_flat`
Expected: 编译失败 `cannot find function layout_table`。

- [ ] **Step 3: 实现 `layout_table`（本任务只处理 Text）**

在 `src/table.rs` 追加：

```rust
pub fn layout_table(model: &TableModel, budget: usize) -> Vec<Vec<Span>> {
    layout_table_depth(model, budget.max(1), 0)
}

fn layout_table_depth(model: &TableModel, budget: usize, depth: usize) -> Vec<Vec<Span>> {
    let ncols = model.ncols();
    if ncols == 0 {
        return Vec::new();
    }
    let overhead = 1 + 3 * ncols;
    if budget < overhead + ncols {
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

    let rendered: Vec<Vec<Vec<Span>>> = model
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
    for (r, rowmodel) in model.rows.iter().enumerate() {
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

fn cell_natural_width(cell: &Cell, _cap: usize, _depth: usize) -> usize {
    cell.blocks
        .iter()
        .map(|b| match b {
            Block::Text(spans) => spans_width(spans),
            Block::Table(_) => 0,
        })
        .max()
        .unwrap_or(0)
}

fn render_cell(cell: &Cell, width: usize, _depth: usize) -> Vec<Vec<Span>> {
    let mut out: Vec<Vec<Span>> = Vec::new();
    for block in &cell.blocks {
        if let Block::Text(spans) = block {
            out.extend(wrap_spans(spans, width));
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

fn flatten_table(model: &TableModel, width: usize) -> Vec<Vec<Span>> {
    let mut out = Vec::new();
    for r in &model.rows {
        let mut spans: Vec<Span> = Vec::new();
        for (i, cell) in r.cells.iter().enumerate() {
            if i > 0 {
                spans.push(Span { text: " | ".to_string(), style: Style::default() });
            }
            for b in &cell.blocks {
                if let Block::Text(s) = b {
                    spans.extend(s.iter().cloned());
                }
            }
        }
        out.extend(wrap_spans(&spans, width.max(1)));
    }
    out
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --bin mdout table::tests`
Expected: 全部通过。

- [ ] **Step 5: 提交**

```bash
git add src/table.rs
git commit -m "feat: add pure layout_table for flat tables"
```

---

### Task 4: GFM `emit_table` 改走统一布局器

**Files:**
- Modify: `src/renderer.rs:658-737`（`fn emit_table`）

- [ ] **Step 1: 替换 `emit_table` 实现**

```rust
    fn emit_table(&mut self) {
        let t = match self.table.take() {
            Some(t) => t,
            None => return,
        };
        let ncols = t.aligns.len().max(t.rows.iter().map(|r| r.len()).max().unwrap_or(0));
        if ncols == 0 {
            return;
        }
        let rows: Vec<crate::table::Row> = t
            .rows
            .iter()
            .enumerate()
            .map(|(ri, r)| crate::table::Row {
                cells: (0..ncols)
                    .map(|i| crate::table::Cell {
                        blocks: vec![crate::table::Block::Text(vec![Span {
                            text: r.get(i).cloned().unwrap_or_default(),
                            style: Style::default(),
                        }])],
                    })
                    .collect(),
                header: ri == 0,
            })
            .collect();
        let aligns = (0..ncols)
            .map(|i| t.aligns.get(i).copied().unwrap_or(Alignment::None))
            .collect();
        let model = crate::table::TableModel { rows, aligns };

        let (_, cont) = self.prefixes();
        let budget = self.opts.width.saturating_sub(prefix_width(&cont)).max(1);
        let lines = crate::table::layout_table(&model, budget);
        for line in lines {
            let mut spans = cont.clone();
            spans.extend(line);
            self.out.push(Line { text: encode_line(&spans, self.opts.color), live: false });
        }
    }
```

同时删除现在不再使用的 `fn wrap_cell`（`renderer.rs` 中），并删除 `fit_table_widths` 的本地引用（Task 2 已迁移）。

- [ ] **Step 2: 运行全部测试（回归护栏）**

Run: `cargo test --bin mdout`
Expected: 全部通过，尤其是 `renders_table_borders_plain`、`cjk_table_exact_layout`、`applies_column_alignment`、`table_wraps_to_terminal_width`、`table_wraps_cjk_without_exceeding_width`。

- [ ] **Step 3: 若 `wrap_cell` 变 unused 产生警告则删除**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 无警告；如有 `function is never used: wrap_cell`，删除该函数后重跑。

- [ ] **Step 4: 提交**

```bash
git add src/renderer.rs
git commit -m "refactor: render GFM tables through shared layout_table"
```

---

### Task 5: 嵌套表格与降级

**Files:**
- Modify: `src/table.rs`（`cell_natural_width`、`render_cell` 支持递归 + 深度/过窄降级）

- [ ] **Step 1: 写失败测试**

在 `src/table.rs` 的 `mod tests` 追加：

```rust
use crate::renderer::Style as St;
use unicode_width::UnicodeWidthStr;

fn styled(text: &str, style: Style) -> Span {
    Span { text: text.to_string(), style }
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

#[test]
fn nested_layout_preserves_inline_style() {
    let outer = TableModel {
        rows: vec![Row {
            cells: vec![Cell { blocks: vec![Block::Text(vec![styled("bold", St { bold: true, ..St::default() })])] }],
            header: false,
        }],
        aligns: vec![],
    };
    let lines = layout_table(&outer, 40);
    assert!(lines.iter().flatten().any(|s| s.style.bold && s.text.contains("bold")));
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --bin mdout table::tests::nested`
Expected: 失败（嵌套未实现，内层表格被忽略）。

- [ ] **Step 3: 用递归实现替换 `cell_natural_width` 与 `render_cell`**

```rust
fn block_natural_width(block: &Block, cap: usize, depth: usize) -> usize {
    match block {
        Block::Text(spans) => spans_width(spans).min(cap),
        Block::Table(t) => table_natural_width(t, cap, depth),
    }
}

fn cell_natural_width(cell: &Cell, cap: usize, depth: usize) -> usize {
    cell.blocks
        .iter()
        .map(|b| block_natural_width(b, cap, depth))
        .max()
        .unwrap_or(0)
        .min(cap)
}

fn table_natural_width(model: &TableModel, cap: usize, depth: usize) -> usize {
    let ncols = model.ncols();
    if ncols == 0 {
        return 0;
    }
    let overhead = 1 + 3 * ncols;
    let avail = cap.saturating_sub(overhead);
    if avail < ncols {
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
    (overhead + cols.iter().sum::<usize>()).min(cap)
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
```

（删除旧的 `cell_natural_width`/`render_cell` 单一定义，只保留上面这组。）

- [ ] **Step 4: 运行测试**

Run: `cargo test --bin mdout table::tests`
Expected: 全部通过。

- [ ] **Step 5: 提交**

```bash
git add src/table.rs
git commit -m "feat: recursive nested table layout with depth and width degradation"
```

---

### Task 6: `src/html.rs` 分词与实体

**Files:**
- Create: `src/html.rs`
- Modify: `src/main.rs`（新增 `mod html;`）

- [ ] **Step 1: 写 `src/html.rs`（分词 + 实体 + 测试）**

```rust
use crate::renderer::{Span, Style};

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source",
    "track", "wbr",
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Start { name: String, self_closing: bool, start: usize, end: usize },
    End { name: String, start: usize, end: usize },
    Text { text: String, start: usize, end: usize },
}

fn is_void(name: &str) -> bool {
    VOID.contains(&name)
}

fn tag_name(inner: &str) -> String {
    inner
        .trim()
        .trim_start_matches('/')
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn tokenize(src: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut text_start = 0usize;
    let bytes = src.as_bytes();
    let mut i = 0usize;

    let flush = |tokens: &mut Vec<Token>, text: &mut String, start: usize, end: usize| {
        if !text.is_empty() {
            tokens.push(Token::Text { text: std::mem::take(text), start, end });
        }
    };

    while i < bytes.len() {
        if bytes[i] != b'<' {
            let next = src[i..].find('<').map(|o| i + o).unwrap_or(bytes.len());
            if text.is_empty() {
                text_start = i;
            }
            text.push_str(&decode_entities(&src[i..next]));
            i = next;
            continue;
        }
        if src[i..].starts_with("<!--") {
            flush(&mut tokens, &mut text, text_start, i);
            i = match src[i + 4..].find("-->") {
                Some(o) => i + 4 + o + 3,
                None => bytes.len(),
            };
            continue;
        }
        if src[i..].starts_with("<!") {
            flush(&mut tokens, &mut text, text_start, i);
            i = match src[i..].find('>') {
                Some(o) => i + o + 1,
                None => bytes.len(),
            };
            continue;
        }
        match src[i..].find('>') {
            Some(off) => {
                let tag_end = i + off + 1;
                let inner = &src[i + 1..tag_end - 1];
                flush(&mut tokens, &mut text, text_start, i);
                if let Some(name) = closing_name(inner) {
                    tokens.push(Token::End { name, start: i, end: tag_end });
                } else {
                    let name = tag_name(inner);
                    if !name.is_empty() {
                        let self_closing = inner.trim_end().ends_with('/') || is_void(&name);
                        tokens.push(Token::Start { name, self_closing, start: i, end: tag_end });
                    }
                }
                i = tag_end;
            }
            None => {
                if text.is_empty() {
                    text_start = i;
                }
                text.push_str(&src[i..]);
                i = bytes.len();
            }
        }
    }
    flush(&mut tokens, &mut text, text_start, bytes.len());
    tokens
}

fn closing_name(inner: &str) -> Option<String> {
    let t = inner.trim();
    t.strip_prefix('/').map(|r| tag_name(r))
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut i = 0usize;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(rel) = s[i..].find(';') {
                let semi = i + rel;
                if let Some(ch) = decode_entity(&s[i + 1..semi]) {
                    out.push_str(&ch);
                    i = semi + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn decode_entity(ent: &str) -> Option<String> {
    match ent {
        "amp" => Some("&".to_string()),
        "lt" => Some("<".to_string()),
        "gt" => Some(">".to_string()),
        "quot" => Some("\"".to_string()),
        "apos" => Some("'".to_string()),
        "nbsp" => Some(" ".to_string()),
        _ => {
            let num = ent.strip_prefix('#')?;
            let hex = num.strip_prefix('x').or_else(|| num.strip_prefix('X'));
            let code = if let Some(hex) = hex {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num.parse::<u32>().ok()
            };
            code.and_then(char::from_u32).map(|c| c.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_tags_and_text() {
        let toks = tokenize("<td>a</td>");
        assert_eq!(toks.len(), 3);
        assert!(matches!(&toks[0], Token::Start { name, .. } if name == "td"));
        assert!(matches!(&toks[1], Token::Text { text, .. } if text == "a"));
        assert!(matches!(&toks[2], Token::End { name, .. } if name == "td"));
    }

    #[test]
    fn void_elements_are_self_closing() {
        let toks = tokenize("<br>x");
        assert!(matches!(&toks[0], Token::Start { name, self_closing, .. } if name == "br" && *self_closing));
    }

    #[test]
    fn drops_comments_and_doctype() {
        assert!(tokenize("<!DOCTYPE html><!-- c -->hi").iter().all(|t| matches!(t, Token::Text { .. })));
    }

    #[test]
    fn decodes_named_and_numeric_entities() {
        assert_eq!(decode_entities("a &amp; b &#65; &#x42;"), "a & b A B");
        assert_eq!(decode_entities("&unknown;"), "&unknown;");
    }

    #[test]
    fn incomplete_tag_does_not_panic() {
        let toks = tokenize("<table><tr><td>a");
        assert!(!toks.is_empty());
    }
}
```

- [ ] **Step 2: 在 `src/main.rs` 加模块声明**

最终为：

```rust
mod app;
mod cli;
mod html;
mod input;
mod parser;
mod renderer;
mod table;
mod terminal;
```

- [ ] **Step 3: 运行测试**

Run: `cargo test --bin mdout html::tests`
Expected: 全部通过。

- [ ] **Step 4: 提交**

```bash
git add src/html.rs src/main.rs
git commit -m "feat: add HTML tokenizer and entity decoding"
```

---

### Task 7: `parse_block` 产出 `HtmlPiece`

**Files:**
- Modify: `src/html.rs`

- [ ] **Step 1: 写失败测试**

在 `src/html.rs` 的 `mod tests` 追加：

```rust
use crate::table::{Block, Cell, TableModel};

fn tbl(rows: &[(&[&str], bool)]) -> TableModel {
    TableModel {
        rows: rows
            .iter()
            .map(|(cells, header)| crate::table::Row {
                cells: cells
                    .iter()
                    .map(|c| Cell {
                        blocks: vec![Block::Text(vec![Span { text: c.to_string(), style: Style::default() }])],
                    })
                    .collect(),
                header: *header,
            })
            .collect(),
        aligns: vec![],
    }
}

#[test]
fn no_table_is_passthrough_raw() {
    assert_eq!(parse_block("<div>hi</div>", 8), vec![HtmlPiece::Raw("<div>hi</div>".to_string())]);
}

#[test]
fn parses_simple_table() {
    let pieces = parse_block("<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>", 8);
    assert_eq!(pieces, vec![HtmlPiece::Table(tbl(&[(&["a", "b"], true), (&["1", "2"], false)]))]);
}

#[test]
fn wrapper_tags_are_transparent_around_table() {
    let pieces = parse_block("<div>A<table><tr><td>x</td></tr></table>B</div>", 8);
    assert_eq!(pieces.len(), 3);
    assert_eq!(pieces[0], HtmlPiece::Raw("A".to_string()));
    assert!(matches!(pieces[1], HtmlPiece::Table(_)));
    assert_eq!(pieces[2], HtmlPiece::Raw("B".to_string()));
}

#[test]
fn nested_table_is_recursive() {
    let src = "<table><tr><td><table><tr><td>x</td></tr></table></td></tr></table>";
    let pieces = parse_block(src, 8);
    match &pieces[0] {
        HtmlPiece::Table(t) => match &t.rows[0].cells[0].blocks[0] {
            Block::Table(inner) => assert_eq!(inner.rows[0].cells[0].blocks.len(), 1),
            other => panic!("expected inner table, got {:?}", other),
        },
        other => panic!("expected table, got {:?}", other),
    }
}

#[test]
fn br_splits_text_blocks() {
    let pieces = parse_block("<table><tr><td>a<br>b</td></tr></table>", 8);
    match &pieces[0] {
        HtmlPiece::Table(t) => assert_eq!(t.rows[0].cells[0].blocks.len(), 2),
        other => panic!("expected table, got {:?}", other),
    }
}

#[test]
fn inline_tags_become_styles() {
    let pieces = parse_block("<table><tr><td><b>x</b><i>y</i></td></tr></table>", 8);
    match &pieces[0] {
        HtmlPiece::Table(t) => {
            let spans = match &t.rows[0].cells[0].blocks[0] {
                Block::Text(s) => s,
                other => panic!("expected text, got {:?}", other),
            };
            assert!(spans.iter().any(|s| s.style.bold && s.text == "x"));
            assert!(spans.iter().any(|s| s.style.italic && s.text == "y"));
        }
        other => panic!("expected table, got {:?}", other),
    }
}

#[test]
fn thead_marks_header_rows() {
    let pieces = parse_block("<table><thead><tr><td>h</td></tr></thead><tbody><tr><td>b</td></tr></tbody></table>", 8);
    match &pieces[0] {
        HtmlPiece::Table(t) => {
            assert!(t.rows[0].header);
            assert!(!t.rows[1].header);
        }
        other => panic!("expected table, got {:?}", other),
    }
}

#[test]
fn multiple_tables_keep_order() {
    let src = "<table><tr><td>a</td></tr></table>mid<table><tr><td>b</td></tr></table>";
    let pieces = parse_block(src, 8);
    assert!(matches!(pieces[0], HtmlPiece::Table(_)));
    assert_eq!(pieces[1], HtmlPiece::Raw("mid".to_string()));
    assert!(matches!(pieces[2], HtmlPiece::Table(_)));
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --bin mdout html::tests::parses_simple_table`
Expected: 编译失败 `cannot find function parse_block`。

- [ ] **Step 3: 实现树构建与 `parse_block`**

在 `src/html.rs` 追加上方为 `tokenize` 等之后的部分：

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HtmlPiece {
    Table(TableModel),
    Raw(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Element { name: String, children: Vec<Node> },
    Text(String),
}

fn push_child(stack: &mut Vec<(String, Vec<Node>)>, root: &mut Vec<Node>, node: Node) {
    match stack.last_mut() {
        Some((_, children)) => children.push(node),
        None => root.push(node),
    }
}

fn build_tree(tokens: &[Token]) -> Vec<Node> {
    let mut root: Vec<Node> = Vec::new();
    let mut stack: Vec<(String, Vec<Node>)> = Vec::new();
    for tok in tokens {
        match tok {
            Token::Text { text, .. } => push_child(&mut stack, &mut root, Node::Text(text.clone())),
            Token::Start { name, self_closing, .. } => {
                if *self_closing {
                    push_child(&mut stack, &mut root, Node::Element { name: name.clone(), children: Vec::new() });
                } else {
                    stack.push((name.clone(), Vec::new()));
                }
            }
            Token::End { name, .. } => {
                if let Some(pos) = stack.iter().rposition(|(n, _)| n == name) {
                    while stack.len() > pos + 1 {
                        let (n, c) = stack.pop().unwrap();
                        push_child(&mut stack, &mut root, Node::Element { name: n, children: c });
                    }
                    let (n, c) = stack.pop().unwrap();
                    push_child(&mut stack, &mut root, Node::Element { name: n, children: c });
                }
            }
        }
    }
    while let Some((n, c)) = stack.pop() {
        push_child(&mut stack, &mut root, Node::Element { name: n, children: c });
    }
    root
}

fn flush_text(cur: &mut Vec<Span>, out: &mut Vec<Block>) {
    if !cur.is_empty() {
        out.push(Block::Text(std::mem::take(cur)));
    }
}

fn push_span(cur: &mut Vec<Span>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = cur.last_mut() {
        if last.style == style {
            last.text.push_str(text);
            return;
        }
    }
    cur.push(Span { text: text.to_string(), style });
}

fn walk_inline(nodes: &[Node], style: Style, depth: usize, max_depth: usize, cur: &mut Vec<Span>, out: &mut Vec<Block>) {
    for node in nodes {
        match node {
            Node::Text(t) => push_span(cur, t, style),
            Node::Element { name, children } => match name.as_str() {
                "br" => flush_text(cur, out),
                "table" => {
                    flush_text(cur, out);
                    if depth >= max_depth {
                        walk_inline(children, style, depth, max_depth, cur, out);
                    } else {
                        out.push(Block::Table(convert_table(children, depth + 1, max_depth)));
                    }
                }
                "b" | "strong" => walk_inline(children, Style { bold: true, ..style }, depth, max_depth, cur, out),
                "i" | "em" => walk_inline(children, Style { italic: true, ..style }, depth, max_depth, cur, out),
                "code" => walk_inline(children, Style { code: true, ..style }, depth, max_depth, cur, out),
                "s" | "del" | "strike" => walk_inline(children, Style { strike: true, ..style }, depth, max_depth, cur, out),
                _ => walk_inline(children, style, depth, max_depth, cur, out),
            },
        }
    }
}

fn cell_blocks(nodes: &[Node], depth: usize, max_depth: usize) -> Vec<Block> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    walk_inline(nodes, Style::default(), depth, max_depth, &mut cur, &mut out);
    flush_text(&mut cur, &mut out);
    out
}

fn convert_row(children: &[Node], in_thead: bool, cell_depth: usize, max_depth: usize) -> Row {
    let mut cells = Vec::new();
    let mut any_th = false;
    for node in children {
        if let Node::Element { name, children } = node {
            if name == "td" || name == "th" {
                if name == "th" {
                    any_th = true;
                }
                cells.push(Cell { blocks: cell_blocks(children, cell_depth, max_depth) });
            }
        }
    }
    Row { cells, header: in_thead || any_th }
}

fn collect_rows(nodes: &[Node], in_thead: bool, cell_depth: usize, max_depth: usize, rows: &mut Vec<Row>) {
    for node in nodes {
        if let Node::Element { name, children } = node {
            match name.as_str() {
                "tr" => rows.push(convert_row(children, in_thead, cell_depth, max_depth)),
                "thead" => collect_rows(children, true, cell_depth, max_depth, rows),
                "tbody" | "tfoot" => collect_rows(children, false, cell_depth, max_depth, rows),
                _ => {}
            }
        }
    }
}

fn convert_table(nodes: &[Node], depth: usize, max_depth: usize) -> TableModel {
    let mut rows = Vec::new();
    collect_rows(nodes, false, depth, max_depth, &mut rows);
    TableModel { rows, aligns: Vec::new() }
}

fn text_content(segment: &str) -> String {
    let mut out = String::new();
    for tok in tokenize(segment) {
        if let Token::Text { text, .. } = tok {
            out.push_str(&text);
        }
    }
    out
}

fn top_level_tables(tokens: &[Token]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for tok in tokens {
        match tok {
            Token::Start { name, start: s, .. } if name == "table" => {
                if depth == 0 {
                    start = *s;
                }
                depth += 1;
            }
            Token::End { name, end: e, .. } if name == "table" => {
                depth -= 1;
                if depth <= 0 {
                    spans.push((start, *e));
                    depth = 0;
                }
            }
            _ => {}
        }
    }
    spans
}

pub fn parse_block(src: &str, max_depth: usize) -> Vec<HtmlPiece> {
    let tokens = tokenize(src);
    let spans = top_level_tables(&tokens);
    if spans.is_empty() {
        return vec![HtmlPiece::Raw(src.to_string())];
    }
    let mut pieces = Vec::new();
    let mut prev = 0usize;
    for (s, e) in spans {
        if s > prev {
            let raw = text_content(&src[prev..s]);
            if !raw.trim().is_empty() {
                pieces.push(HtmlPiece::Raw(raw));
            }
        }
        let sub: Vec<Token> = tokens
            .iter()
            .filter(|t| token_bounds(t).0 >= s && token_bounds(t).1 <= e)
            .cloned()
            .collect();
        let tree = build_tree(&sub);
        if let Some(Node::Element { name, children }) = tree.first() {
            if name == "table" {
                pieces.push(HtmlPiece::Table(convert_table(children, 1, max_depth)));
            }
        }
        prev = e;
    }
    if prev < src.len() {
        let raw = text_content(&src[prev..]);
        if !raw.trim().is_empty() {
            pieces.push(HtmlPiece::Raw(raw));
        }
    }
    pieces
}

fn token_bounds(t: &Token) -> (usize, usize) {
    match t {
        Token::Start { start, end, .. } | Token::End { start, end, .. } | Token::Text { start, end, .. } => (*start, *end),
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --bin mdout html::tests`
Expected: 全部通过。

- [ ] **Step 5: 提交**

```bash
git add src/html.rs
git commit -m "feat: parse HTML blocks into tables and raw pieces"
```

---

### Task 8: renderer 集成 HTML 块

**Files:**
- Modify: `src/renderer.rs`（`R` 字段、`start`、`event`、`end`、新增 `emit_html_block`）

- [ ] **Step 1: 写失败测试**

在 `src/renderer.rs` 的 `mod tests` 追加：

```rust
    #[test]
    fn renders_html_table() {
        let md = "<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>";
        assert_eq!(plain(md, 80), vec![
            "┌───┬───┐",
            "│ a │ b │",
            "├───┼───┤",
            "│ 1 │ 2 │",
            "└───┴───┘",
        ]);
    }

    #[test]
    fn html_without_table_is_passthrough() {
        assert_eq!(plain("<div>hi there</div>", 80), vec!["<div>hi there</div>"]);
    }

    #[test]
    fn html_table_inside_blockquote_keeps_prefix() {
        let out = plain("> <table><tr><td>x</td></tr></table>", 80);
        assert!(out.iter().all(|l| l.starts_with("│ ")), "missing quote prefix: {:?}", out);
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --bin mdout renders_html_table`
Expected: FAIL（当前把 `<table>` 当纯文本）。

- [ ] **Step 3: 给 `R` 增加字段**

在 `struct R` 中 `table: Option<TableState>,` 后加：

```rust
    html_buf: Option<String>,
```

在 `R::new` 的 `table: None,` 后加：

```rust
            html_buf: None,
```

- [ ] **Step 4: `start` 识别 HtmlBlock**

把 `start()` 顶部的 `matches!` 列表加上 `Tag::HtmlBlock`：

```rust
        if matches!(
            &tag,
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(..)
                | Tag::CodeBlock(_)
                | Tag::List(_)
                | Tag::Item
                | Tag::Table(_)
                | Tag::HtmlBlock
        ) {
            self.emit_current();
        }
```

在 `start()` 的 `match tag` 中新增分支：

```rust
            Tag::HtmlBlock => {
                self.html_buf = Some(String::new());
            }
```

- [ ] **Step 5: `event` 缓冲 `Event::Html`**

把：

```rust
            Event::Html(t) | Event::InlineHtml(t) => self.push_text(&t, self.style),
```

改为：

```rust
            Event::Html(t) => {
                if let Some(buf) = self.html_buf.as_mut() {
                    buf.push_str(&t);
                } else {
                    self.push_text(&t, self.style);
                }
            }
            Event::InlineHtml(t) => self.push_text(&t, self.style),
```

- [ ] **Step 6: `end` 处理 `TagEnd::HtmlBlock` 并实现 `emit_html_block`**

在 `end()` 的 `match tag` 中新增：

```rust
            TagEnd::HtmlBlock => self.emit_html_block(),
```

在 `impl R` 中（`emit_table` 之后）新增方法：

```rust
    fn emit_html_block(&mut self) {
        let raw = match self.html_buf.take() {
            Some(s) => s,
            None => return,
        };
        let pieces = crate::html::parse_block(&raw, crate::table::MAX_TABLE_DEPTH);
        let has_table = pieces
            .iter()
            .any(|p| matches!(p, crate::html::HtmlPiece::Table(_)));
        if !has_table {
            self.push_text(&raw, self.style);
            return;
        }
        self.emit_current();
        let (first, cont) = self.prefixes();
        let pw = prefix_width(&first).max(prefix_width(&cont));
        let budget = self.opts.width.saturating_sub(pw).max(1);
        let mut lines: Vec<Line> = Vec::new();
        for piece in pieces {
            match piece {
                crate::html::HtmlPiece::Raw(s) => {
                    let spans = vec![Span { text: s, style: self.style }];
                    for row in wrap_with_prefix(&spans, &first, &cont, self.opts.width) {
                        lines.push(Line { text: encode_line(&row, self.opts.color), live: false });
                    }
                }
                crate::html::HtmlPiece::Table(t) => {
                    for line in crate::table::layout_table(&t, budget) {
                        let mut spans = cont.clone();
                        spans.extend(line);
                        lines.push(Line { text: encode_line(&spans, self.opts.color), live: false });
                    }
                }
            }
        }
        self.out.extend(lines);
    }
```

- [ ] **Step 7: 运行测试**

Run: `cargo test --bin mdout`
Expected: 全部通过。

- [ ] **Step 8: 提交**

```bash
git add src/renderer.rs
git commit -m "feat: render HTML tables in markdown blocks"
```

---

### Task 9: live / stable 流式行为

**Files:**
- Modify: `src/renderer.rs`（`trailing_block_closed`）

- [ ] **Step 1: 写失败测试**

在 `src/renderer.rs` 的 `mod tests` 追加：

```rust
    #[test]
    fn unclosed_html_table_stays_live() {
        let o = opts(false, 80);
        let partial = "<table><tr><td>a";
        assert!(render(partial, &o, false).iter().any(|l| l.live));
        let done = "<table><tr><td>a</td></tr></table>\n\n";
        assert!(render(done, &o, false).iter().all(|l| !l.live));
    }

    #[test]
    fn html_chunk_boundary_invariance() {
        let md = "<table><tr><td>a</td><td><table><tr><td>x</td></tr></table></td></tr></table>\n\ntail\n";
        let o = opts(false, 40);
        let final_lines = render(md, &o, true);
        for (i, _) in md.char_indices() {
            let inc = render(&md[..i], &o, false);
            let stable: Vec<&Line> = inc.iter().filter(|l| !l.live).collect();
            for (k, l) in stable.iter().enumerate() {
                assert_eq!(
                    Some(&l.text),
                    final_lines.get(k).map(|f| &f.text),
                    "stable prefix diverged at chunk {i}, line {k}"
                );
            }
        }
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --bin mdout unclosed_html_table_stays_live`
Expected: FAIL（`<table>` 未闭合也返回 stable）。

- [ ] **Step 3: 扩展 `trailing_block_closed`**

在 `src/renderer.rs` 的 `trailing_block_closed` 中，`false` 返回之前插入 HTML 块判定：

```rust
    let block_start = md.rfind("\n\n").map(|i| i + 2).unwrap_or(0);
    let tail = md[block_start..].trim_start();
    if tail.starts_with('<') {
        let lower = tail.to_ascii_lowercase();
        let opens = lower.matches("<table").count();
        if opens == 0 {
            return false;
        }
        let closes = lower.matches("</table").count();
        return opens == closes && tail.trim_end().ends_with('>');
    }
    false
```

（即把原函数结尾的 `false` 替换为以上整段。）

- [ ] **Step 4: 运行测试**

Run: `cargo test --bin mdout`
Expected: 全部通过（含新增两条）。

- [ ] **Step 5: 提交**

```bash
git add src/renderer.rs
git commit -m "feat: stream HTML tables through live region until closed"
```

---

### Task 10: 集成测试与文档

**Files:**
- Modify: `tests/integration.rs`
- Modify: `README.md:103-117`（支持的语法表）

- [ ] **Step 1: 写集成测试**

在 `tests/integration.rs` 末尾追加（沿用该文件现有的 `mdout` 调用辅助；若辅助函数名不同，按文件现有模式改写）：

```rust
#[test]
fn html_nested_table_renders_within_width() {
    let md = "<table><tr><td>a</td><td><table><tr><td>x</td></tr></table></td></tr></table>";
    let out = run_mdout_with_width(md, 60);
    assert!(out.contains('┌'));
    assert!(out.contains('x'));
    for line in out.lines() {
        assert!(unicode_width::UnicodeWidthStr::width(line) <= 60, "line too wide: {:?}", line);
    }
}
```

若 `tests/integration.rs` 未暴露 `run_mdout_with_width`/`unicode_width`，改为复用文件内已有的调用辅助并把宽度断言用 `chars().count()` 近似，或新增依赖（不推荐）。**实现时必须先阅读 `tests/integration.rs` 现有辅助函数，按同样的方式调用二进制。**

- [ ] **Step 2: 运行集成测试**

Run: `cargo test --test integration`
Expected: 全部通过。

- [ ] **Step 3: 更新 README 语法表**

在「支持的语法」表中，`| 表格 | ... |` 一行后插入：

```markdown
| HTML `<table>` | 解析块内 `<table>`（含任意层嵌套），真网格嵌套渲染；支持 `<td>`/`<th>`/`<tr>` 与行内标签；不支持 colspan/rowspan |
```

- [ ] **Step 4: 提交**

```bash
git add tests/integration.rs README.md
git commit -m "test: cover HTML nested table rendering end to end"
```

---

### Task 11: 重新打包 `dist/`（发布产物）

**Files:**
- Modify: `dist/mdout-0.1.0-x86_64-unknown-linux-musl.tar.gz`
- Modify: `dist/mdout-0.1.0-x86_64-unknown-linux-musl.tar.gz.sha256`

- [ ] **Step 1: 构建 musl 静态二进制**

Run: `cargo build --release --target x86_64-unknown-linux-musl`
Expected: 编译成功。

- [ ] **Step 2: 冒烟验证**

Run:

```bash
printf '<table><tr><td>a</td><td><table><tr><td>x</td></tr></table></td></tr></table>\n' | target/x86_64-unknown-linux-musl/release/mdout --color=never
```

Expected: 输出含内外两层 `┌`/`└` 框线与 `x`。

- [ ] **Step 3: 重新打包并更新校验和**

```bash
set -e
STAGE=/tmp/opencode/dist-stage
rm -rf "$STAGE"
mkdir -p "$STAGE/mdout-0.1.0-x86_64-unknown-linux-musl"
cp target/x86_64-unknown-linux-musl/release/mdout "$STAGE/mdout-0.1.0-x86_64-unknown-linux-musl/mdout"
cp README.md "$STAGE/mdout-0.1.0-x86_64-unknown-linux-musl/README.md"
chmod 755 "$STAGE/mdout-0.1.0-x86_64-unknown-linux-musl/mdout"
tar -C "$STAGE" -czf dist/mdout-0.1.0-x86_64-unknown-linux-musl.tar.gz mdout-0.1.0-x86_64-unknown-linux-musl
cd dist
sha256sum mdout-0.1.0-x86_64-unknown-linux-musl.tar.gz > mdout-0.1.0-x86_64-unknown-linux-musl.tar.gz.sha256
sha256sum -c mdout-0.1.0-x86_64-unknown-linux-musl.tar.gz.sha256
```

Expected: `OK`。

- [ ] **Step 4: 提交**

```bash
git add dist/mdout-0.1.0-x86_64-unknown-linux-musl.tar.gz dist/mdout-0.1.0-x86_64-unknown-linux-musl.tar.gz.sha256
git commit -m "release: rebuild x86_64 musl package with HTML nested tables"
```

---

## 自检记录

- **Spec 覆盖**：数据模型（Task 1）、解析器/实体（Task 6-7）、布局/嵌套/降级（Task 3、5）、流式 live/stable（Task 8-9）、统一布局器与回归护栏（Task 2、4）、测试策略（各任务 + Task 10）、dist（Task 11）。
- **占位符**：无 TODO/TBD；Task 10 Step 1 要求在实现时先读 `tests/integration.rs` 复用其现有辅助，属明确的阅读指令而非占位。
- **类型一致性**：`TableModel`/`Row`/`Cell`/`Block`、`parse_block`、`HtmlPiece`、`layout_table`、`fit_table_widths`、`wrap_spans` 在各任务中名称与签名一致；`MAX_TABLE_DEPTH` 在 Task 1 定义、Task 5/7 使用。
- **已知取舍**：`emit_table` 现用 `prefixes()` 的 `cont`（引用前缀带 dim 样式）替代旧的无样式 `quote_prefix`，颜色模式下引用内表格前缀轻微变化，属与段落一致的有意统一。
