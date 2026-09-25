# mdout 设计文档：管道 Markdown 实时渲染器

- 日期：2026-09-25
- 状态：已评审待实现
- 目标平台：Linux（x86_64、aarch64）

## 1. 目标与范围

`mdout` 从标准输入（管道）接收 Markdown 字符串流，**边到达边渲染**为终端友好的文档，
以滚动输出方式呈现，可继续被管道/重定向。

### 1.1 目标

- 流式增量渲染：数据一到即显示，未完成的块也能逐字刷新。
- 常用全量 Markdown 语法：标题、粗体/斜体/删除线、行内代码、链接、列表、任务列表、
  引用、围栏代码块（语法高亮）、表格、水平线。
- 表格支持简体中文与英文混排的正确对齐。
- 运行时依赖最小化，产出零共享库依赖的静态单文件，支持 x86_64 与 aarch64。
- 非 TTY 输出自动降级为无颜色纯文本。

### 1.2 非目标（YAGNI）

- 不支持全屏 TUI、滚动回看、快捷键交互。
- 不接收文件路径参数，仅从 stdin 读取。
- 不支持图片渲染、脚注、数学公式。
- 不生成 HTML。

## 2. 关键设计决策

| 决策点 | 选择 | 理由 |
| --- | --- | --- |
| 实时模型 | 全缓冲重解析 + 稳定区/实时区重绘 | 简单正确，兼顾逐字实时 |
| 输出方式 | 滚动式 ANSI 追加 | 可管道、可重定向、无全屏框架 |
| 解析器 | `pulldown-cmark` | 成熟、纯 Rust、支持 GFM 扩展 |
| 高亮 | `syntect`（纯 Rust `regex-fancy`） | 无 C 依赖，静态友好 |
| 宽度 | `unicode-width` | CJK/emoji 双宽正确 |
| 宽度探测 | `terminal_size` 每 tick 轮询 | 免 signal 依赖 |
| 并发 | std 线程 + `mpsc::recv_timeout` | 免异步运行时 |
| CLI | 手写解析 | 免 `clap` 依赖 |

## 3. 架构与数据流

```
stdin 读取线程
   │  Chunk(String) / Eof / ReadError
   ▼
主循环 (去抖 ~40ms，mpsc::recv_timeout)
   │  整个已接收缓冲
   ▼
parser: Markdown -> 渲染事件流 (pulldown-cmark)
   │
   ▼
renderer: 事件 -> Vec<Line>{text, live}   (纯函数)
   │
   ▼
terminal 控制器:
   - 提交新 stable 行 (永久追加、滚入回滚缓冲)
   - 重绘 live 区 (光标上移 -> 清除到屏幕尾 -> 重绘)
```

### 3.1 稳定区 / 实时区重绘模型

- 每次 tick 重解析**整个缓冲**，渲染器返回 `Vec<Line>`，每行带 `live: bool`。
- 最后一个尚未闭合的块 → `live`；其之前已确定的块 → `stable`。
- 维护 `committed_stable: usize`：只增量追加尚未提交的 stable 行。
- live 区重绘步骤（全程耗时极短）：
  1. 隐藏光标；
  2. 光标上移上次 live 行数；
  3. 清除到屏幕末尾（`ESC[J`）；
  4. 输出本次 live 行；
  5. 立即显示光标。
- 光标只在单次重绘期间瞬时隐藏，不跨 tick 保持隐藏状态，因此 SIGINT 无需特殊清理。
- live 区仅包含最后一个未完成块，行数有限，始终位于可视范围内，避免触及回滚缓冲。
- EOF：最终一次渲染时把所有行视为 stable，提交全部内容并收尾。

### 3.2 组件边界

| 组件 | 职责 | 依赖 | 可测性 |
| --- | --- | --- | --- |
| `input` | stdin 增量读取线程，跨块 UTF-8 解码 | std | 单测：喂入任意字节切分 |
| `parser` | Markdown -> 事件序列 | pulldown-cmark | 间接由 renderer 覆盖 |
| `renderer` | 事件 -> `Vec<Line>`，纯函数 | syntect/unicode-width | 纯文本快照单测 |
| `terminal` | ANSI 生成、宽度、stable 提交、live 重绘 | terminal_size | 逻辑单测（宽度/转义） |
| `app` | 去抖事件循环、EOF 收尾、退出码 | std | 集成测试 |
| `cli` | 参数解析与校验 | std | 单测 |

## 4. 依赖

生产依赖（4 个）：

```toml
pulldown-cmark = { version = "0.12", default-features = false }
syntect = { version = "5", default-features = false,
            features = ["parsing", "default-syntaxes", "default-themes", "regex-fancy"] }
unicode-width = "0.2"
terminal_size = "0.4"
```

运行时启用的解析扩展：`ENABLE_TABLES`、`ENABLE_TASKLISTS`、`ENABLE_STRIKETHROUGH`。
不启用 `html`。

有意不引入：`clap`、`crossterm`/`ratatui`、`tokio`、`signal-hook`。
均为纯 Rust 依赖，交叉编译无需 C 工具链（仅需 musl 链接器）。

## 5. 渲染映射

| 语法 | 渲染 |
| --- | --- |
| H1 | 粗体 + 按宽度 `═` 下划线 |
| H2 | 粗体 + 按宽度 `─` 下划线 |
| H3–H6 | `###`… 前缀 + 着色 + 粗体 |
| 粗体 | ANSI bold |
| 斜体 | ANSI italic |
| 删除线 | ANSI 9 |
| 行内代码 | 反差色（反显或独立前景色） |
| 链接 | `文本 (url)`，URL 暗色 |
| 引用 | 前缀 `│ `，按嵌套深度缩进着色 |
| 无序列表 | `•` 前缀；有序 `1.` |
| 任务列表 | `☑` / `☐` |
| 代码块 | syntect 高亮，整体缩进 4 空格，不用跨行背景色 |
| 表格 | 游标 `┌─┬─┐` 边框，按列宽对齐 |
| 水平线 | `─` 铺满宽度 |
| 段落 | 按终端宽度 word-wrap |

### 5.1 表格与中英文混排

- 列宽 = 该列所有单元格**无样式文本**的显示宽度最大值（加左右各 1 空格内边距）。
- 显示宽度用 `unicode-width` 的 `UnicodeWidthStr::width`：简体中文等 East Asian Wide 字符计 2，
  ASCII 计 1，组合字符/零宽连接符计 0，emoji 计 2。
- **先按纯文本计算宽度并填充空格，再套用 ANSI 样式**；转义序列不计入宽度。
- 中英文混排单元格（如 `名称 name`）与纯中文、纯英文列均可对齐。
- 表格在空行（块结束）后才确定并作为 stable 块输出。

### 5.2 行模型

```rust
struct Line {
    text: String,   // 已按 color.opts 决定是否含 ANSI
    live: bool,     // 是否属于未闭合块
}
```

渲染器签名：`render(markdown: &str, opts: &RenderOpts) -> Vec<Line>`。
`opts.color == false` 时不产生任何转义字符，便于精确快照测试。

## 6. 错误处理

| 场景 | 处理 |
| --- | --- |
| stdin 读取错误 | stderr 提示，用已读内容完成渲染，最终退出码 1 |
| 跨块 UTF-8 断字 | 增量解码器缓存不完整尾字节 |
| 非法 UTF-8 | lossy 替换 U+FFFD，不中断 |
| stdout BrokenPipe | 静默退出，退出码 0 |
| SIGINT | 默认终止即可；光标仅瞬时隐藏，无残留状态 |
| 非 TTY stdout | 关闭颜色、禁用重绘，只提交 stable，EOF 提交 live |
| 宽度探测失败 / 0 | 回退 80 |
| 无效 CLI 参数 | stderr 用法提示，退出码 2 |

退出码：`0` 成功，`1` 输入错误，`2` 用法错误。

## 7. CLI

```
mdout [--color auto|always|never]   # 默认 auto
      [--width N]                   # 默认自动探测，回退 80
      [--theme NAME]                # syntect 主题，默认 base16-ocean.dark
      [--no-highlight]              # 关闭代码高亮
      [-h|--help] [-V|--version]
```

`--color auto`：stdout 为 TTY 时启用颜色；`always`/`never` 强制覆盖。
优先级：显式 `--color always|never` 高于 `NO_COLOR`；仅当为 `auto`（默认）且未显式指定时，
`NO_COLOR` 环境变量存在等同 `never`。

## 8. 测试策略

- **渲染器单测**：关闭颜色，逐一断言标题/强调/列表/任务/表格/代码块/引用/链接/水平线/换行的精确纯文本。
- **表格对齐单测**：中文、英文、中英混排、emoji 单元格，断言边框列对齐与显示宽度。
- **流式不变量测试（核心）**：对同一输入的任意字节切分序列，EOF 后的最终渲染结果恒等于一次性整输入渲染结果。
- **UTF-8 跨块解码单测**：在多字节字符中间切分。
- **stable/live 分类单测**。
- **集成测试**：子进程 + 管道喂入（非 TTY 路径），断言输出无 `ESC`、内容正确；`| head` 断管退出码 0。
- **CLI 单测**：参数解析与非法输入。

## 9. 构建与双架构静态发布

- 目标 triple：`x86_64-unknown-linux-musl`、`aarch64-unknown-linux-musl`。
- release 配置：

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
panic = "abort"
opt-level = "z"
```

- `.cargo/config.toml` 为 aarch64 交叉编译配置 `linker = "aarch64-linux-gnu-gcc"`（需 `gcc-aarch64-linux-gnu`）。
- 因依赖均为纯 Rust，交叉编译无需额外 C 依赖；aarch64 原生机器可直接构建。
- 交付：单文件静态二进制，零共享库依赖，体积约 2–5MB。

## 10. 项目结构

```
MDOut/
├── Cargo.toml
├── .cargo/config.toml
├── src/
│   ├── main.rs        # 入口、退出码
│   ├── cli.rs         # 参数解析
│   ├── app.rs         # 去抖事件循环、EOF 收尾
│   ├── input.rs       # stdin 线程、增量 UTF-8 解码
│   ├── parser.rs      # pulldown-cmark 封装
│   ├── renderer.rs    # 事件 -> Vec<Line>
│   └── terminal.rs    # 宽度、ANSI、stable/live 重绘
└── tests/
    └── integration.rs
```

## 11. 待定项

无。所有关键决策已在评审中确认。
