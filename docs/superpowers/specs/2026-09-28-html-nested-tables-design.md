# mdout 设计文档：HTML 嵌套表格渲染

- 日期：2026-09-28
- 状态：已评审待实现
- 前置设计：`docs/superpowers/specs/2026-09-25-md-stream-renderer-design.md`

## 1. 目标与范围

### 1.1 目标

- 在 Markdown 渲染流程中识别 HTML 块里的 `<table>`，渲染为与 GFM 表格一致的终端边框表格。
- 支持**任意层嵌套**（受常量深度上限约束），采用「真网格嵌套」：内层表格的边框画在外层单元格内，外层行高自动撑开，外层列宽容纳内层表格，终端不够宽时内层递归收缩换行。
- 表格单元格支持行内 HTML 标签（`b`/`strong`、`i`/`em`、`code`、`s`/`del`/`strike`、`br`），保留样式。
- 流式增量：未闭合的 HTML 表格处于 live 区实时重绘，`</table>` 到齐后作为 stable 块提交。
- 不新增运行时依赖，保持「纯 Rust、零 C 依赖、musl 静态单文件」的特性。

### 1.2 非目标（YAGNI）

- 不支持 `colspan` / `rowspan`（合并单元格），不支持 `<caption>`（忽略）。
- 不解析表格之外的 HTML 为结构化元素；非表格 HTML 仍原样透传（按宽度换行）。
- 不渲染 HTML 之外的块级标签（`div`/`section`/`ul`/`p` 等仅作透明包裹）。
- 不引入 HTML5 规范级容错；只做表格子集所需的宽容解析。
- 不支持 CSS、内联 `style`、`class`、事件属性等。

## 2. 关键设计决策

| 决策点 | 选择 | 理由 |
| --- | --- | --- |
| 解析实现 | 手写轻量 HTML 解析器 | 保持 4 个依赖、体积不变；只覆盖表格子集 |
| 架构路线 | 递归 Block 模型 + 纯函数布局器 | 嵌套要求「先量后画」两遍布局，必须有中间模型；便于单测 |
| 表格模型 | 单元格是块列表，块可递归为表格 | 结构上支持任意嵌套 |
| 渲染方式 | 真网格嵌套 | 最忠实 HTML 表格语义 |
| 触发条件 | HTML 块内查找顶层 `<table>` 并全部渲染 | 贴近真实 HTML（表格常被 `div` 包裹） |
| 流式策略 | 未闭合实时重绘，闭合后提交 | 与现有稳定区/实时区模型一致 |
| 闭合判定 | 自行配平 `<table>` / `</table>` | pulldown-cmark 每次把当前缓冲当完整文档，必然发出 `End(HtmlBlock)`，不能据此判断闭合 |
| 布局统一 | GFM 与 HTML 表格共用 `layout_table` | 列宽/换行/边框逻辑只有一份，减少重复 |

## 3. 数据模型

新增两个模块：`src/html.rs`（解析）、`src/table.rs`（模型与布局）。

```rust
// html.rs
pub enum HtmlPiece {
    Table(TableModel),   // 解析出的表格（已递归含嵌套）
    Raw(String),         // 表格前后的非表格内容，原样透传
}

pub fn parse_block(src: &str, max_depth: usize) -> Vec<HtmlPiece>;

// table.rs
pub struct TableModel { pub rows: Vec<Row> }

pub struct Row {
    pub cells: Vec<Cell>,
    pub header: bool,          // <th> 行或 <thead> 内的行
}

pub struct Cell { pub blocks: Vec<Block> }

pub enum Block {
    Text(Vec<Span>),           // 一段行内内容，含样式；换行由布局按列宽决定
    Table(TableModel),         // 递归嵌套
}
```

- `Span` / `Style` 复用 `src/renderer.rs` 现有定义，`table.rs` 通过 `use crate::renderer::{Span, Style}` 引用，不新增类型。
- 单元格持有块列表，因此「文字 + 嵌套表格」可共存；嵌套表格的单元格又是块列表，递归受 `max_depth` 约束。
- `header` 标记决定表头下的分隔线；GFM 表格首行恒为 `header`，HTML 由 `<th>` / `<thead>` 决定。
- 模型不存 `colspan` / `rowspan`。

## 4. HTML 解析器（`src/html.rs`）

### 4.1 分词

单遍扫描，输出 token：`StartTag { name, self_closing }`、`EndTag { name }`、`Text`、`Comment`、`Doctype`/`CDATA`。

- 标签名大小写不敏感（统一小写）；属性一律跳过，只取标签名。
- `<!-- … -->`、`<!DOCTYPE …>`、`<![CDATA[ … ]]>` 整体丢弃。
- 末尾不完整的 `<tb`、未闭合引号等按普通文本或丢弃处理，**绝不 panic**，保证流式重解析安全。

### 4.2 块级扫描（产出 `Vec<HtmlPiece>`）

- 用「原始缓冲」累积非表格内容；遇到 `<table>` 时先 flush 成 `HtmlPiece::Raw`，再解析表格，之后继续扫描。
- 包裹标签（`div`、`section`、`body`、`center` 等）透明：标签本身丢弃，内部文字保留进 Raw。
- `<script>` / `<style>` 内容整体丢弃；注释丢弃。
- 一个 HTML 块内按文档顺序渲染所有顶层 `<table>`，表格之间的文字原样保留。

### 4.3 表格子树构建

栈式、容错构建：

- 透明容器：`table`、`thead`、`tbody`、`tfoot`（内容贡献行）；`caption`、`colgroup`、`col` 忽略。
- `tr` → `Row`；直接位于 `tr` 下的非空白文本忽略（浏览器语义）。
- `td` / `th` → `Cell`；`header = true` 当标签是 `th`，或祖先含 `thead`。
- 单元格内的嵌套 `<table>` 递归构建 → `Block::Table`。
- 缺结束标签容错：新 `<td>` 自动关闭上一个；新 `<tr>` 关闭未闭合单元格；`</tr>` / `</table>` 逐级收尾；`<table>` 未闭合时保留已解析行（live 标记由渲染层处理）。

### 4.4 行内内容与样式

- `b` / `strong` → `bold`；`i` / `em` → `italic`；`code` → `code`；`s` / `del` / `strike` → `strike`；`a` → 保留文字（不追加 URL）；其余未知标签透明展开。
- `<br>` → 结束当前 `Block::Text` 并开启新块（强制换行）。
- void 元素（`br`、`hr`、`img`、`input`、`meta`、`link`、`col`）视为自闭合，不吞后续内容。
- 连续文字与行内样式合并为 `Block::Text(Vec<Span>)`。

### 4.5 HTML 实体

实现小型解码器，支持常见命名实体（`amp`、`lt`、`gt`、`quot`、`apos`、`nbsp` 等）与数字实体（`&#123;`、`&#x1F;`）；未知或非法实体原样保留。

## 5. 布局算法（`src/table.rs`，纯函数）

统一入口：GFM 表格与 HTML 表格都先转成 `TableModel`，再走同一个纯函数。

```rust
pub fn layout_table(model: &TableModel, budget: usize) -> Vec<Vec<Span>>; // 每行一组带样式 Span
```

`layout_table` 不含引用前缀；渲染层负责加 `prefixes()` 前缀并相应扣减宽度预算。

### 5.1 第一遍：量宽度（自底向上）

- 单元格自然宽 = 各 block 自然宽的最大值。`Block::Text` 用 `UnicodeWidthStr` 求行内总宽；`Block::Table` 递归求内层表格自然宽。
- 关键优化：`natural_cell_width(cell, cap)` 把结果**截断在 `cap`**（外层可用宽）。等价于「只要 ≥ 预算，具体多少不重要」，避免深层嵌套宽度爆炸。
- 表格自然宽 = `1 + Σ(列自然宽) + 3·ncols`（`1 + 3·ncols` 为边框与内边距开销，与现有公式一致；引用前缀不计入，见上）。
- 列宽 = `fit_table_widths(列自然宽, available)`，`available = budget - (1 + 3·ncols)`，复用现有二分收缩（放得下时返回自然宽，逐字节保持现状）。

### 5.2 第二遍：画（自顶向下）

- 每个单元格按最终列宽 `widths[j]` 渲染：
  - `Block::Text` → 行内换行 `wrap_spans(&[Span], width) -> Vec<Vec<Span>>`（从现有 `wrap_widths` + `coalesce` 提取）。
  - `Block::Table` → 递归 `layout_table(inner, widths[j])`。
- 各 block 的输出按顺序纵向堆叠；`<br>` 已切成多个 `Block::Text`，天然占多行。
- 行高 = 该行各单元格行数的最大值；不足的单元格底部补空行（顶对齐）。
- 逐视觉行拼接边框与内边距，支持列对齐（GFM 的 `:--` / `:-:` / `--:`；HTML 默认左对齐）。
- 内层表格边框直接画在单元格内 → 真网格嵌套。

### 5.3 样式与颜色

所有输出行均为 `Vec<Span>`；边框用默认样式，行内标签样式保留；最终统一交给 `encode_line` 决定是否着色，plain 模式自动无样式。

## 6. 流式集成（`src/renderer.rs`）

### 6.1 状态与事件

- `R` 新增 `html_buf: Option<String>`（`Some` 表示处于 `HtmlBlock`）。
- `Tag::HtmlBlock` 加入 `start()` 的顶层块列表（与 `Paragraph` 一样触发 `emit_current`、空行分隔、记录 `block_starts`），并置 `html_buf = Some(String::new())`。
- `Event::Html(t)`：`html_buf` 为 `Some` 时追加到缓冲；`Event::InlineHtml` 保持原样透传（不解析）。
- `TagEnd::HtmlBlock`：取走缓冲并 `parse_block`：
  - 含表格：先 `emit_current()`；按 `HtmlPiece` 顺序输出，`Raw` 走 `push_text` + `emit_current`，`Table` 走 `layout_table` 并按 `prefixes()` 加前缀、扣宽度预算。
  - 不含表格：`push_text(raw)`，与现有行为一致。

### 6.2 live / stable

- 配平判定：扫描缓冲中 `<table`（大小写不敏感）与 `</table>` 的数量，`open > close` 即未完成，本轮所有表格行标 `live`。
- `trailing_block_closed(md)` 扩展：末尾块若以 `<` 开头（HTML 块），仅当 `<table>`/`</table>` 配平**且最后一行以 `>` 结尾**时算闭合，否则返回 `false`，由现有 `finish_input` 从 `block_starts.last()` 起标 `live`。
- 流是仅追加的：缓冲末尾一旦出现 `</table>`，后续只会追加到表格之后，不会向表格内部插行，故此时提交 stable 安全，满足现有 `chunk_boundary_invariance` 稳定前缀约束。

### 6.3 兼容

`emit_table`（GFM）重构为「构造 `TableModel` → `layout_table`」，放得下时输出与现状逐字节一致；`html_buf` 为空时所有路径与改动前相同。

## 7. 降级与边界

- `MAX_TABLE_DEPTH = 8`：超过深度的嵌套表格降级为文本行（每行单元格行内文本用 ` | ` 连接），再走普通换行。
- 某层 `available < ncols`（每列连 1 字符都放不下）时同样降级为文本行，避免错位边框。
- 空表格（无行）、单元格内连续/末尾空 `Block::Text`、`<br>` 产生的尾随空行：折叠不输出。
- `budget` 为 0/1 时按 1 处理。
- 未闭合/错配标签、缺失 `<td>`：按 4.3 隐式收尾规则处理。
- `colspan`/`rowspan` 属性忽略（按单列渲染，不合并、不崩溃）。
- 复杂度：解析 O(n)，布局 O(行 × 列)，递归受深度上限约束；流式每 tick 重解析整个缓冲，与现状同阶。

## 8. 模块与依赖

| 模块 | 职责 | 依赖 |
| --- | --- | --- |
| `html`（新） | HTML 分词、树构建、实体解码，产出 `Vec<HtmlPiece>` | 仅 `renderer` 的 `Span`/`Style` |
| `table`（新） | `TableModel`/`Block` 模型与 `layout_table` 纯函数 | `renderer` 的 `Span`/`Style`、`unicode-width` |
| `renderer`（改） | 缓冲 HTML 块、调用解析与布局、live/stable、GFM 表格改走统一布局 | `html`、`table` |

生产依赖保持 4 个（`pulldown-cmark`、`syntect`、`unicode-width`、`terminal_size`），无新增。

## 9. 测试策略

- **`html.rs`**：基本表格；`th`/`thead` 表头；嵌套表格；行内标签样式；`<br>` 切块；命名/数字实体；包裹标签透明；表格前后文字保留；多表格顺序；无表格 → 全 Raw；`script`/`style`/注释丢弃；大小写不敏感；void 元素；未闭合/错配标签不 panic。
- **`table.rs`**：放得下时与现有 GFM 输出一致；超宽收缩换行；真网格嵌套（内层边框在单元格内、外层行高撑开）；超过 `MAX_TABLE_DEPTH` 降级；`available < ncols` 降级；CJK 嵌套列宽对齐；颜色模式样式保留（ANSI）；空单元格/空表格折叠。
- **`renderer.rs`**：`plain(md, width)` 快照 HTML 表格；未闭合 `<table>` → 行 `live`，闭合 `</table>` → stable；`chunk_boundary_invariance` 扩展含嵌套 HTML 表格样例；非表格 HTML 输出与改动前一致；引用块内 HTML 表格带 `│ ` 前缀。
- **回归护栏**：现有 GFM 表格测试（`renders_table_borders_plain`、`cjk_table_exact_layout`、`table_wraps_to_terminal_width` 等）必须原样通过。
- **健壮性**：对一段含嵌套表格的 HTML 遍历每个前缀 `render(.., false)`，断言不 panic 且稳定前缀与最终一致。
- **`tests/integration.rs`**：CLI 级用例，`--width` 下 HTML 嵌套表格输出对齐、无超宽。

## 10. 风险与权衡

- 手写 HTML 解析器只覆盖表格子集，遇到复杂真实 HTML 可能行为不如 html5ever；以「表格可用、其余透传」为边界。
- 深层嵌套在窄终端下反复降级，视觉退化但保证不错位、不崩溃。
- 闭合配平是行/文本层面的启发式；极端畸形输入可能提前提交或持续 live，但不会 panic，且稳定前缀约束始终成立。
