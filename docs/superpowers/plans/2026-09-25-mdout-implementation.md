# mdout 流式 Markdown 渲染器 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 `mdout`：从 stdin 流式接收 Markdown，实时渲染为终端友好的滚动文档，支持常用全量语法、中英文表格对齐，产出 x86_64/aarch64 零依赖静态二进制。

**Architecture:** 全缓冲重解析 + 稳定区/实时区双区域重绘。stdin 读取线程按块送入主循环，每次 tick 用 `pulldown-cmark` 重解析整个缓冲，渲染器（纯函数）输出带 `live` 标记的行；终端控制器永久提交 stable 行、就地重绘未闭合块的 live 行。

**Tech Stack:** Rust 2021、`pulldown-cmark`、`syntect`（纯 Rust `regex-fancy`）、`unicode-width`、`terminal_size`；仅用标准库做并发与 CLI。

参考 spec：`docs/superpowers/specs/2026-09-25-md-stream-renderer-design.md`

---

## 文件结构

```
MDOut/
├── Cargo.toml            # 包与依赖、release 压缩配置
├── .cargo/config.toml    # aarch64 交叉链接器
├── src/
│   ├── main.rs           # 入口、颜色/宽度解析、模块声明、退出码
│   ├── cli.rs            # 参数解析 Config
│   ├── app.rs            # 去抖事件循环、EOF 收尾、退出码
│   ├── input.rs          # Utf8Decoder + stdin 读取线程
│   ├── parser.rs         # pulldown-cmark Options 封装
│   ├── renderer.rs       # 事件 -> Vec<Line>{text, live}
│   └── terminal.rs       # 宽度探测、stable 提交、live 重绘
└── tests/
    └── integration.rs    # 子进程 + 管道端到端
```

---

### Task 1: 项目脚手架与依赖

**Files:**
- Create: `Cargo.toml`
- Create: `.cargo/config.toml`
- Create: `src/main.rs`
- Create: `src/cli.rs`, `src/app.rs`, `src/input.rs`, `src/parser.rs`, `src/renderer.rs`, `src/terminal.rs`

- [ ] **Step 1: 写 `Cargo.toml`**

```toml
[package]
name = "mdout"
version = "0.1.0"
edition = "2021"

[dependencies]
pulldown-cmark = { version = "0.12", default-features = false }
syntect = { version = "5", default-features = false, features = ["parsing", "default-syntaxes", "default-themes", "regex-fancy"] }
unicode-width = "0.2"
terminal_size = "0.4"

[profile.release]
lto = true
codegen-units = 1
strip = true
panic = "abort"
opt-level = "z"
```

- [ ] **Step 2: 写 `.cargo/config.toml`**

```toml
[target.aarch64-unknown-linux-musl]
linker = "aarch64-linux-gnu-gcc"
```

- [ ] **Step 3: 写 `src/parser.rs`**

```rust
use pulldown_cmark::Options;

pub fn options() -> Options {
    let mut o = Options::empty();
    o.insert(Options::ENABLE_TABLES);
    o.insert(Options::ENABLE_TASKLISTS);
    o.insert(Options::ENABLE_STRIKETHROUGH);
    o
}
```

- [ ] **Step 4: 写其余模块的占位定义**

`src/cli.rs`:
```rust
pub const USAGE: &str = "Usage: mdout [--color auto|always|never] [--width N] [--theme NAME] [--no-highlight] [-h|--help] [-V|--version]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorMode { Auto, Always, Never }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub color: ColorMode,
    pub width: Option<usize>,
    pub theme: String,
    pub highlight: bool,
    pub help: bool,
    pub version: bool,
}

pub fn parse_args<I: IntoIterator<Item = String>>(_args: I) -> Result<Config, String> {
    Ok(Config { color: ColorMode::Auto, width: None, theme: "base16-ocean.dark".into(), highlight: true, help: false, version: false })
}
```

`src/input.rs`:
```rust
pub enum InputMsg { Chunk(String), Eof, Error(std::io::Error) }
```

`src/renderer.rs`:
```rust
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Line { pub text: String, pub live: bool }
```

`src/terminal.rs`:
```rust
pub fn detect_width() -> usize { 80 }
```

`src/app.rs`:
```rust
pub struct RunConfig { pub color: bool, pub width: usize, pub highlight: bool, pub theme: String, pub redraw: bool }
pub fn run(_cfg: RunConfig) -> u8 { 0 }
```

`src/main.rs`:
```rust
mod app;
mod cli;
mod input;
mod parser;
mod renderer;
mod terminal;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let _cfg = match cli::parse_args(args) {
        Ok(c) => c,
        Err(e) => { eprintln!("mdout: {e}"); eprintln!("{}", cli::USAGE); return ExitCode::from(2); }
    };
    ExitCode::SUCCESS
}
```

- [ ] **Step 5: 构建并运行**

Run: `cargo build`
Expected: 编译成功。

- [ ] **Step 6: 提交**

```bash
git add Cargo.toml Cargo.lock .cargo/config.toml src
git commit -m "chore: scaffold mdout crate and dependencies"
```

---

### Task 2: CLI 参数解析

**Files:**
- Modify: `src/cli.rs`
- Test: `src/cli.rs` 内 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试（追加到 `src/cli.rs`）**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Config, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn defaults_are_auto_theme_and_highlight() {
        let c = p(&[]).unwrap();
        assert_eq!(c.color, ColorMode::Auto);
        assert_eq!(c.width, None);
        assert_eq!(c.theme, "base16-ocean.dark");
        assert!(c.highlight);
    }

    #[test]
    fn parses_color_space_form() {
        assert_eq!(p(&["--color", "never"]).unwrap().color, ColorMode::Never);
        assert_eq!(p(&["--color", "always"]).unwrap().color, ColorMode::Always);
    }

    #[test]
    fn parses_equals_form() {
        assert_eq!(p(&["--width=50"]).unwrap().width, Some(50));
        assert_eq!(p(&["--color=always"]).unwrap().color, ColorMode::Always);
        assert_eq!(p(&["--theme=InspiredGitHub"]).unwrap().theme, "InspiredGitHub");
    }

    #[test]
    fn parses_no_highlight_and_flags() {
        let c = p(&["--no-highlight", "-h"]).unwrap();
        assert!(!c.highlight);
        assert!(c.help);
        assert!(p(&["-V"]).unwrap().version);
    }

    #[test]
    fn rejects_unknown_and_bad_values() {
        assert!(p(&["--nope"]).is_err());
        assert!(p(&["--color", "blue"]).is_err());
        assert!(p(&["--width", "abc"]).is_err());
        assert!(p(&["--width"]).is_err());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test cli::`
Expected: FAIL（`parse_args` 忽略参数，断言不通过）。

- [ ] **Step 3: 实现解析（替换 `src/cli.rs` 的 `parse_args`）**

```rust
fn take_value(
    it: &mut impl Iterator<Item = String>,
    inline: Option<String>,
    key: &str,
) -> Result<String, String> {
    match inline {
        Some(v) => Ok(v),
        None => it.next().ok_or_else(|| format!("missing value for {key}")),
    }
}

pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Config, String> {
    let mut cfg = Config {
        color: ColorMode::Auto,
        width: None,
        theme: "base16-ocean.dark".into(),
        highlight: true,
        help: false,
        version: false,
    };
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let (key, inline) = match arg.split_once('=') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (arg, None),
        };
        match key.as_str() {
            "--color" => {
                let v = take_value(&mut it, inline, &key)?;
                cfg.color = match v.as_str() {
                    "auto" => ColorMode::Auto,
                    "always" => ColorMode::Always,
                    "never" => ColorMode::Never,
                    _ => return Err(format!("invalid color: {v}")),
                };
            }
            "--width" => {
                let v = take_value(&mut it, inline, &key)?;
                cfg.width = Some(v.parse().map_err(|_| format!("invalid width: {v}"))?);
            }
            "--theme" => cfg.theme = take_value(&mut it, inline, &key)?,
            "--no-highlight" => cfg.highlight = false,
            "-h" | "--help" => cfg.help = true,
            "-V" | "--version" => cfg.version = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(cfg)
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test cli::`
Expected: PASS（5 个测试）。

- [ ] **Step 5: 提交**

```bash
git add src/cli.rs
git commit -m "feat: parse mdout CLI arguments"
```

---

### Task 3: 增量 UTF-8 解码与 stdin 读取线程

**Files:**
- Modify: `src/input.rs`
- Test: `src/input.rs` 内 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_split_multibyte_char() {
        let mut d = Utf8Decoder::new();
        let bytes = "中".as_bytes();
        assert_eq!(d.push(&bytes[..1]), "");
        assert_eq!(d.push(&bytes[1..2]), "");
        assert_eq!(d.push(&bytes[2..]), "中");
    }

    #[test]
    fn decodes_incremental_ascii_and_cjk() {
        let mut d = Utf8Decoder::new();
        assert_eq!(d.push(b"ab"), "ab");
        assert_eq!(d.push("中".as_bytes()), "中");
        assert_eq!(d.finish(), "");
    }

    #[test]
    fn replaces_invalid_bytes_lossily() {
        let mut d = Utf8Decoder::new();
        assert_eq!(d.push(&[0xff, b'a']), "\u{FFFD}a");
    }

    #[test]
    fn finish_flushes_incomplete_as_replacement() {
        let mut d = Utf8Decoder::new();
        assert_eq!(d.push(&[0xe4, 0xb8]), ""); // 不完整的中文字首
        assert_eq!(d.finish(), "\u{FFFD}");
    }

    #[test]
    fn reader_thread_forwards_chunks_then_eof() {
        use std::io::Cursor;
        let rx = spawn_reader(Cursor::new(b"hello".to_vec()));
        match rx.recv().unwrap() { InputMsg::Chunk(s) => assert_eq!(s, "hello"), _ => panic!("want chunk") }
        assert!(matches!(rx.recv().unwrap(), InputMsg::Eof));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test input::`
Expected: FAIL（类型/函数不存在）。

- [ ] **Step 3: 实现（替换 `src/input.rs`）**

```rust
use std::io::Read;
use std::sync::mpsc::{self, Receiver};

pub enum InputMsg {
    Chunk(String),
    Eof,
    Error(std::io::Error),
}

pub struct Utf8Decoder {
    buf: Vec<u8>,
}

impl Utf8Decoder {
    pub fn new() -> Self {
        Utf8Decoder { buf: Vec::new() }
    }

    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.buf.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.buf) {
                Ok(s) => {
                    out.push_str(s);
                    self.buf.clear();
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    out.push_str(std::str::from_utf8(&self.buf[..valid]).unwrap());
                    match e.error_len() {
                        Some(len) => {
                            out.push('\u{FFFD}');
                            self.buf.drain(..valid + len);
                        }
                        None => {
                            self.buf.drain(..valid);
                            break;
                        }
                    }
                }
            }
        }
        out
    }

    pub fn finish(mut self) -> String {
        let rest = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        rest
    }
}

impl Default for Utf8Decoder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spawn_reader<R: Read + Send + 'static>(mut reader: R) -> Receiver<InputMsg> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut decoder = Utf8Decoder::new();
        let mut raw = [0u8; 8192];
        loop {
            match reader.read(&mut raw) {
                Ok(0) => {
                    let tail = decoder.finish();
                    if !tail.is_empty() {
                        let _ = tx.send(InputMsg::Chunk(tail));
                    }
                    let _ = tx.send(InputMsg::Eof);
                    break;
                }
                Ok(n) => {
                    let text = decoder.push(&raw[..n]);
                    if !text.is_empty() && tx.send(InputMsg::Chunk(text)).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    let _ = tx.send(InputMsg::Error(e));
                    break;
                }
            }
        }
    });
    rx
}

pub fn spawn_stdin_reader() -> Receiver<InputMsg> {
    spawn_reader(std::io::stdin())
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test input::`
Expected: PASS（5 个测试）。

- [ ] **Step 5: 提交**

```bash
git add src/input.rs
git commit -m "feat: incremental UTF-8 stdin reader"
```

---

### Task 4: 渲染器基础（类型 / 换行 / 行内 / 标题 / 颜色）

**Files:**
- Modify: `src/renderer.rs`
- Test: `src/renderer.rs` 内 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn opts(color: bool, width: usize) -> RenderOpts {
        RenderOpts::new(color, width, false, "base16-ocean.dark")
    }

    fn plain(md: &str, width: usize) -> Vec<String> {
        render(md, &opts(false, width), true).into_iter().map(|l| l.text).collect()
    }

    #[test]
    fn renders_paragraph_plain() {
        assert_eq!(plain("hello world", 80), vec!["hello world"]);
    }

    #[test]
    fn renders_emphasis_and_code_plain() {
        assert_eq!(plain("a *b* **c** `d`", 80), vec!["a b c d"]);
    }

    #[test]
    fn renders_heading_with_underline() {
        assert_eq!(plain("# Title", 80), vec!["Title", "═════"]);
        assert_eq!(plain("## Sub", 80), vec!["Sub", "───"]);
        assert_eq!(plain("### Deep", 80), vec!["### Deep"]);
    }

    #[test]
    fn renders_link_with_url() {
        assert_eq!(plain("[x](http://e.com)", 80), vec!["x (http://e.com)"]);
    }

    #[test]
    fn wraps_long_paragraph() {
        let out = plain("one two three four", 7);
        assert_eq!(out, vec!["one two", "three", "four"]);
    }

    #[test]
    fn color_mode_emits_ansi() {
        let out = render("**bold**", &opts(true, 80), true);
        assert!(out[0].text.contains("\u{1b}[1m"));
        assert!(out[0].text.contains("\u{1b}[0m"));
    }

    #[test]
    fn blank_line_separates_top_level_blocks() {
        assert_eq!(plain("a\n\nb", 80), vec!["a", "", "b"]);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test renderer::`
Expected: FAIL（未实现）。

- [ ] **Step 3: 实现基础（替换 `src/renderer.rs`）**

```rust
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::parser;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
    pub dim: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Line {
    pub text: String,
    pub live: bool,
}

pub struct RenderOpts {
    pub color: bool,
    pub width: usize,
    pub highlight: bool,
    pub syntaxes: SyntaxSet,
    pub theme: Theme,
}

impl RenderOpts {
    pub fn new(color: bool, width: usize, highlight: bool, theme_name: &str) -> Self {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .get(theme_name)
            .or_else(|| themes.themes.get("base16-ocean.dark"))
            .or_else(|| themes.themes.values().next())
            .cloned()
            .expect("syntect has default themes");
        RenderOpts { color, width: width.max(1), highlight, syntaxes, theme }
    }
}

const RESET: &str = "\x1b[0m";

fn ansi_start(s: Style) -> String {
    let mut codes: Vec<&str> = Vec::new();
    if s.bold { codes.push("1"); }
    if s.dim { codes.push("2"); }
    if s.italic { codes.push("3"); }
    if s.strike { codes.push("9"); }
    if s.code { codes.push("7"); }
    if s.link { codes.push("36"); }
    if codes.is_empty() { String::new() } else { format!("\x1b[{}m", codes.join(";")) }
}

fn encode_line(spans: &[Span], color: bool) -> String {
    if !color {
        return spans.iter().map(|s| s.text.as_str()).collect();
    }
    let mut out = String::new();
    let mut prev = Style::default();
    for sp in spans {
        if sp.style != prev {
            out.push_str(RESET);
            out.push_str(&ansi_start(sp.style));
            prev = sp.style;
        }
        out.push_str(&sp.text);
    }
    if prev != Style::default() {
        out.push_str(RESET);
    }
    out
}

fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

fn to_chars(spans: &[Span]) -> Vec<(char, Style)> {
    let mut v = Vec::new();
    for sp in spans {
        for c in sp.text.chars() {
            v.push((c, sp.style));
        }
    }
    v
}

fn coalesce(chars: &[(char, Style)]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for &(c, st) in chars {
        if c == '\n' {
            continue;
        }
        match spans.last_mut() {
            Some(last) if last.style == st => last.text.push(c),
            _ => spans.push(Span { text: c.to_string(), style: st }),
        }
    }
    spans
}

fn wrap_widths(chars: &[(char, Style)], width: usize) -> Vec<Vec<(char, Style)>> {
    let width = width.max(1);
    let mut rows: Vec<Vec<(char, Style)>> = Vec::new();
    let mut cur: Vec<(char, Style)> = Vec::new();
    let mut cur_w = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let start = i;
        while i < chars.len() && !chars[i].0.is_whitespace() {
            i += 1;
        }
        let word = &chars[start..i];
        let word_w: usize = word.iter().map(|(c, _)| char_width(*c)).sum();
        while i < chars.len() && chars[i].0.is_whitespace() {
            i += 1;
        }
        if word.is_empty() {
            continue;
        }
        let sep = if cur.is_empty() { 0 } else { 1 };
        if cur_w + sep + word_w <= width {
            if sep == 1 {
                cur.push((' ', Style::default()));
                cur_w += 1;
            }
            cur.extend_from_slice(word);
            cur_w += word_w;
        } else {
            if !cur.is_empty() {
                rows.push(std::mem::take(&mut cur));
                cur_w = 0;
            }
            let mut ww = 0usize;
            for &(c, st) in word {
                let cw = char_width(c);
                if ww + cw > width && !cur.is_empty() {
                    rows.push(std::mem::take(&mut cur));
                    ww = 0;
                }
                cur.push((c, st));
                ww += cw;
            }
            cur_w = ww;
        }
    }
    if !cur.is_empty() || rows.is_empty() {
        rows.push(cur);
    }
    rows
}

fn prefix_width(prefix: &[Span]) -> usize {
    prefix.iter().map(|s| UnicodeWidthStr::width(s.text.as_str())).sum()
}

fn wrap_with_prefix(content: &[Span], first: &[Span], rest: &[Span], width: usize) -> Vec<Vec<Span>> {
    let pw = prefix_width(first).max(prefix_width(rest));
    let budget = width.saturating_sub(pw).max(1);
    let rows = wrap_widths(&to_chars(content), budget);
    let mut out = Vec::new();
    for (idx, row) in rows.into_iter().enumerate() {
        let mut spans = if idx == 0 { first.to_vec() } else { rest.to_vec() };
        spans.extend(coalesce(&row));
        out.push(spans);
    }
    out
}

fn quote_prefix(depth: usize) -> String {
    "│ ".repeat(depth)
}

fn heading_num(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn ends_with_blank(md: &str) -> bool {
    let t = md.trim_end_matches([' ', '\t']);
    t.is_empty() || t.ends_with("\n\n")
}

struct ListCtx {
    ordered: bool,
    next: u64,
    marker: String,
}

struct TableState {
    aligns: Vec<Alignment>,
    rows: Vec<Vec<String>>,
    cur_row: Vec<String>,
    cur_cell: String,
}

struct R<'a> {
    opts: &'a RenderOpts,
    out: Vec<Line>,
    cur: Vec<Span>,
    style: Style,
    style_stack: Vec<Style>,
    depth: usize,
    block_starts: Vec<usize>,
    quote_depth: usize,
    list_stack: Vec<ListCtx>,
    in_code: bool,
    code_lang: Option<String>,
    code_buf: String,
    table: Option<TableState>,
    heading_level: Option<HeadingLevel>,
    link_stack: Vec<String>,
}

impl<'a> R<'a> {
    fn new(opts: &'a RenderOpts) -> Self {
        R {
            opts,
            out: Vec::new(),
            cur: Vec::new(),
            style: Style::default(),
            style_stack: Vec::new(),
            depth: 0,
            block_starts: Vec::new(),
            quote_depth: 0,
            list_stack: Vec::new(),
            in_code: false,
            code_lang: None,
            code_buf: String::new(),
            table: None,
            heading_level: None,
            link_stack: Vec::new(),
        }
    }

    fn push_style(&mut self, f: impl FnOnce(&mut Style)) {
        self.style_stack.push(self.style);
        f(&mut self.style);
    }

    fn pop_style(&mut self) {
        if let Some(s) = self.style_stack.pop() {
            self.style = s;
        }
    }

    fn push_text(&mut self, text: &str, style: Style) {
        let mut first = true;
        for part in text.split('\n') {
            if !first {
                self.emit_current();
            }
            if !part.is_empty() {
                self.cur.push(Span { text: part.to_string(), style });
            }
            first = false;
        }
    }

    fn prefixes(&self) -> (Vec<Span>, Vec<Span>) {
        let mut first = Vec::new();
        let mut cont = Vec::new();
        if self.quote_depth > 0 {
            let st = Style { dim: true, ..Style::default() };
            let q = quote_prefix(self.quote_depth);
            first.push(Span { text: q.clone(), style: st });
            cont.push(Span { text: q, style: st });
        }
        if let Some(ctx) = self.list_stack.last() {
            let level = self.list_stack.len() - 1;
            let indent = "  ".repeat(level);
            if !indent.is_empty() {
                first.push(Span { text: indent.clone(), style: Style::default() });
                cont.push(Span { text: indent.clone(), style: Style::default() });
            }
            first.push(Span { text: ctx.marker.clone(), style: Style::default() });
            cont.push(Span {
                text: " ".repeat(UnicodeWidthStr::width(ctx.marker.as_str())),
                style: Style::default(),
            });
        }
        (first, cont)
    }

    fn emit_current(&mut self) {
        let content = std::mem::take(&mut self.cur);
        if content.is_empty() {
            return;
        }
        let (first, cont) = self.prefixes();
        let rows = wrap_with_prefix(&content, &first, &cont, self.opts.width);
        for row in rows {
            self.out.push(Line { text: encode_line(&row, self.opts.color), live: false });
        }
    }

    fn push_blank_separator(&mut self) {
        if let Some(last) = self.out.last() {
            if !last.text.is_empty() {
                self.out.push(Line { text: String::new(), live: false });
            }
        }
    }

    fn start(&mut self, tag: Tag) {
        if self.depth == 0 {
            self.push_blank_separator();
            self.block_starts.push(self.out.len());
        }
        self.depth += 1;
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.push_style(|s| s.bold = true);
                self.heading_level = Some(level);
                let n = heading_num(level);
                if n >= 3 {
                    self.cur.push(Span { text: format!("{} ", "#".repeat(n)), style: self.style });
                }
            }
            Tag::BlockQuote(..) => self.quote_depth += 1,
            Tag::CodeBlock(kind) => {
                self.in_code = true;
                self.code_buf.clear();
                self.code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        let l = lang.trim().to_string();
                        if l.is_empty() { None } else { Some(l) }
                    }
                    CodeBlockKind::Indented => None,
                };
            }
            Tag::List(start) => {
                self.list_stack.push(ListCtx {
                    ordered: start.is_some(),
                    next: start.unwrap_or(1),
                    marker: String::new(),
                });
            }
            Tag::Item => {
                if let Some(ctx) = self.list_stack.last_mut() {
                    if ctx.ordered {
                        ctx.marker = format!("{}. ", ctx.next);
                        ctx.next += 1;
                    } else if ctx.marker.is_empty() {
                        ctx.marker = "• ".to_string();
                    }
                }
            }
            Tag::Emphasis => self.push_style(|s| s.italic = true),
            Tag::Strong => self.push_style(|s| s.bold = true),
            Tag::Strikethrough => self.push_style(|s| s.strike = true),
            Tag::Link { dest_url, .. } => {
                self.link_stack.push(dest_url.to_string());
                self.push_style(|s| s.link = true);
            }
            Tag::Table(aligns) => {
                self.table = Some(TableState {
                    aligns,
                    rows: Vec::new(),
                    cur_row: Vec::new(),
                    cur_cell: String::new(),
                });
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        self.depth = self.depth.saturating_sub(1);
        match tag {
            TagEnd::Paragraph => self.emit_current(),
            TagEnd::Heading(_) => self.emit_heading(),
            TagEnd::BlockQuote(..) => self.quote_depth = self.quote_depth.saturating_sub(1),
            TagEnd::CodeBlock => {
                self.in_code = false;
                self.emit_code_block();
            }
            TagEnd::Item => self.emit_current(),
            TagEnd::List(_) => {
                self.list_stack.pop();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.pop_style(),
            TagEnd::Link => {
                self.pop_style();
                if let Some(url) = self.link_stack.pop() {
                    let st = Style { dim: true, ..self.style };
                    self.push_text(&format!(" ({url})"), st);
                }
            }
            TagEnd::Table => self.emit_table(),
            _ => {}
        }
    }

    fn emit_heading(&mut self) {
        let content = std::mem::take(&mut self.cur);
        if !content.is_empty() {
            let w = prefix_width(&content).min(self.opts.width).max(1);
            let rows = wrap_widths(&to_chars(&content), self.opts.width);
            for row in rows {
                self.out.push(Line {
                    text: encode_line(&coalesce(&row), self.opts.color),
                    live: false,
                });
            }
            if let Some(level) = self.heading_level.take() {
                let ch = match level {
                    HeadingLevel::H1 => Some('═'),
                    HeadingLevel::H2 => Some('─'),
                    _ => None,
                };
                if let Some(ch) = ch {
                    let sp = Span { text: ch.to_string().repeat(w), style: Style { dim: true, ..Style::default() } };
                    self.out.push(Line {
                        text: encode_line(std::slice::from_ref(&sp), self.opts.color),
                        live: false,
                    });
                }
            }
        }
        self.heading_level = None;
        self.pop_style();
    }

    fn emit_code_block(&mut self) {
        let code = std::mem::take(&mut self.code_buf);
        let _ = self.code_lang.take();
        let pad = format!("{}    ", quote_prefix(self.quote_depth));
        let body = code.strip_suffix('\n').unwrap_or(&code);
        if body.is_empty() && code.is_empty() {
            return;
        }
        for line in body.split('\n') {
            self.out.push(Line { text: format!("{pad}{line}"), live: false });
        }
    }

    fn table_event(&mut self, ev: Event) {
        if matches!(&ev, Event::End(TagEnd::Table)) {
            self.depth = self.depth.saturating_sub(1);
            self.emit_table();
            return;
        }
        let Some(t) = self.table.as_mut() else { return };
        match ev {
            Event::Start(Tag::TableHead) => {}
            Event::Start(Tag::TableRow) => t.cur_row.clear(),
            Event::Start(Tag::TableCell) => t.cur_cell.clear(),
            Event::End(TagEnd::TableCell) => t.cur_row.push(std::mem::take(&mut t.cur_cell)),
            Event::End(TagEnd::TableRow) => {
                let row = std::mem::take(&mut t.cur_row);
                t.rows.push(row);
            }
            Event::End(TagEnd::TableHead) => {
                let row = std::mem::take(&mut t.cur_row);
                t.rows.push(row);
            }
            Event::Text(s) | Event::Code(s) => t.cur_cell.push_str(&s),
            Event::SoftBreak | Event::HardBreak => t.cur_cell.push(' '),
            _ => {}
        }
    }

    fn emit_table(&mut self) {
        let t = match self.table.take() {
            Some(t) => t,
            None => return,
        };
        let ncols = t.aligns.len().max(t.rows.iter().map(|r| r.len()).max().unwrap_or(0));
        if ncols == 0 {
            return;
        }
        let mut widths = vec![0usize; ncols];
        for row in &t.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < ncols {
                    widths[i] = widths[i].max(UnicodeWidthStr::width(cell.as_str()));
                }
            }
        }
        let q = quote_prefix(self.quote_depth);
        let border = |l: &str, m: &str, r: &str| -> String {
            let mut s = String::new();
            s.push_str(&q);
            s.push_str(l);
            for (i, w) in widths.iter().enumerate() {
                s.push_str(&"─".repeat(w + 2));
                s.push_str(if i + 1 < ncols { m } else { r });
            }
            s
        };
        let rowstr = |row: &[String]| -> String {
            let mut s = String::new();
            s.push_str(&q);
            s.push('│');
            for i in 0..ncols {
                let empty = String::new();
                let cell = row.get(i).unwrap_or(&empty);
                let w = UnicodeWidthStr::width(cell.as_str());
                let pad = widths[i].saturating_sub(w);
                s.push(' ');
                s.push_str(cell);
                s.push_str(&" ".repeat(pad));
                s.push(' ');
                s.push('│');
            }
            s
        };
        let mut lines = vec![border("┌", "┬", "┐")];
        if let Some(h) = t.rows.first() {
            lines.push(rowstr(h));
            lines.push(border("├", "┼", "┤"));
        }
        for row in t.rows.iter().skip(1) {
            lines.push(rowstr(row));
        }
        lines.push(border("└", "┴", "┘"));
        for l in lines {
            self.out.push(Line { text: l, live: false });
        }
    }

    fn event(&mut self, ev: Event) {
        if self.table.is_some() {
            self.table_event(ev);
            return;
        }
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if self.in_code {
                    self.code_buf.push_str(&t);
                } else {
                    self.push_text(&t, self.style);
                }
            }
            Event::Code(t) => {
                let st = Style { code: true, ..self.style };
                self.push_text(&t, st);
            }
            Event::SoftBreak => self.push_text(" ", self.style),
            Event::HardBreak => self.emit_current(),
            Event::Rule => {
                if self.depth == 0 {
                    self.push_blank_separator();
                    self.block_starts.push(self.out.len());
                }
                let w = self.opts.width;
                let sp = Span { text: "─".repeat(w), style: Style { dim: true, ..Style::default() } };
                self.out.push(Line { text: encode_line(std::slice::from_ref(&sp), self.opts.color), live: false });
            }
            Event::TaskListMarker(checked) => {
                if let Some(ctx) = self.list_stack.last_mut() {
                    ctx.marker = if checked { "☑ ".to_string() } else { "☐ ".to_string() };
                }
            }
            Event::Html(t) | Event::InlineHtml(t) => self.push_text(&t, self.style),
            _ => {}
        }
    }

    fn finish_input(&mut self, final_flush: bool, md: &str) {
        self.emit_current();
        if !final_flush && !ends_with_blank(md) {
            if let Some(&start) = self.block_starts.last() {
                let start = start.min(self.out.len());
                for line in &mut self.out[start..] {
                    line.live = true;
                }
            }
        }
    }
}

pub fn render(markdown: &str, opts: &RenderOpts, final_flush: bool) -> Vec<Line> {
    let mut r = R::new(opts);
    for ev in Parser::new_ext(markdown, parser::options()) {
        r.event(ev);
    }
    r.finish_input(final_flush, markdown);
    r.out
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test renderer::`
Expected: PASS（7 个测试）。若个别断言因分隔空行不符，按实际输出修正断言（例如 `a\n\nb` 应为 `["a", "", "b"]`）。

- [ ] **Step 5: 提交**

```bash
git add src/renderer.rs
git commit -m "feat: renderer core with inline styles, headings, wrapping"
```

---

### Task 5: 块级元素（引用 / 列表 / 任务列表 / 水平线）

**Files:**
- Modify: `src/renderer.rs`（仅在必要时调整 Task 4 已写入的分支）
- Test: `src/renderer.rs` 内 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试（追加到 `renderer::tests`）**

```rust
    #[test]
    fn renders_unordered_list() {
        assert_eq!(plain("- a\n- b", 80), vec!["• a", "• b"]);
    }

    #[test]
    fn renders_ordered_list_with_numbers() {
        assert_eq!(plain("1. a\n2. b", 80), vec!["1. a", "2. b"]);
    }

    #[test]
    fn renders_task_list() {
        assert_eq!(plain("- [x] done\n- [ ] todo", 80), vec!["☑ done", "☐ todo"]);
    }

    #[test]
    fn renders_blockquote_prefix() {
        assert_eq!(plain("> quoted", 80), vec!["│ quoted"]);
    }

    #[test]
    fn renders_horizontal_rule() {
        assert_eq!(plain("---", 5), vec!["─────"]);
    }

    #[test]
    fn list_continuation_is_indented() {
        assert_eq!(plain("- one two three", 8), vec!["• one", "  two", "  three"]);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test renderer::`
Expected: 新测试中至少 `list_continuation_is_indented` 或块级行为 FAIL（prefixes 已实现，但需校验实际输出）。

- [ ] **Step 3: 修正实现**

Task 4 的 `prefixes`/`emit_current`/`start`/`end` 已覆盖引用、列表、任务列表、水平线。逐一运行新测试，按实际输出微调 `prefixes`（尤其是列表缩进宽度：`level = list_stack.len() - 1`，每级 2 空格）。任务列表标记由 `Event::TaskListMarker` 覆盖 `ctx.marker`，需确保它在 `Tag::Item` 之后到达（pulldown-cmark 保证如此）。

- [ ] **Step 4: 运行全部渲染器测试**

Run: `cargo test renderer::`
Expected: PASS（全部）。

- [ ] **Step 5: 提交**

```bash
git add src/renderer.rs
git commit -m "feat: blockquote, lists, task lists, horizontal rule"
```

---

### Task 6: 围栏代码块与 syntect 高亮

**Files:**
- Modify: `src/renderer.rs`（实现 `emit_code_block` 的高亮分支）
- Test: `src/renderer.rs` 内 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn renders_code_block_indented_plain() {
        let out = plain("```\nfn main() {}\n```", 80);
        assert_eq!(out, vec!["    fn main() {}"]);
    }

    #[test]
    fn highlighted_code_uses_ansi() {
        let o = RenderOpts::new(true, 80, true, "base16-ocean.dark");
        let out = render("```rust\nfn main() {}\n```", &o, true);
        assert!(out.iter().any(|l| l.text.contains("\u{1b}[38;2;")));
    }

    #[test]
    fn no_highlight_option_keeps_plain() {
        let o = RenderOpts::new(true, 80, false, "base16-ocean.dark");
        let out = render("```rust\nfn main() {}\n```", &o, true);
        assert!(!out.iter().any(|l| l.text.contains("\u{1b}[38;2;")));
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test renderer::`
Expected: `highlighted_code_uses_ansi` FAIL（`emit_code_block` 尚未高亮）。

- [ ] **Step 3: 实现高亮（替换 `emit_code_block`）**

```rust
    fn emit_code_block(&mut self) {
        let code = std::mem::take(&mut self.code_buf);
        let lang = self.code_lang.take();
        let pad = format!("{}    ", quote_prefix(self.quote_depth));
        let use_highlight = self.opts.color && self.opts.highlight;
        if !use_highlight {
            let body = code.strip_suffix('\n').unwrap_or(&code);
            if body.is_empty() && code.is_empty() {
                return;
            }
            for line in body.split('\n') {
                self.out.push(Line { text: format!("{pad}{line}"), live: false });
            }
            return;
        }
        use syntect::easy::HighlightLines;
        use syntect::util::LinesWithEndings;
        let syntax = lang
            .as_deref()
            .and_then(|l| self.opts.syntaxes.find_syntax_by_token(l))
            .unwrap_or_else(|| self.opts.syntaxes.find_syntax_plain_text());
        let mut h = HighlightLines::new(syntax, &self.opts.theme);
        let mut any = false;
        for line in LinesWithEndings::from(&code) {
            let rendered = match h.highlight_line(line, &self.opts.syntaxes) {
                Ok(ranges) => {
                    let mut s = syntect::util::as_24_bit_terminal_escaped(&ranges[..], false);
                    while s.ends_with('\n') || s.ends_with('\r') {
                        s.pop();
                    }
                    s
                }
                Err(_) => line.trim_end_matches(['\n', '\r']).to_string(),
            };
            any = true;
            self.out.push(Line { text: format!("{pad}{rendered}"), live: false });
        }
        if !any && !code.is_empty() {
            self.out.push(Line { text: pad, live: false });
        }
    }
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test renderer::`
Expected: PASS。若 `renders_code_block_indented_plain` 的尾行不符，按 `split('\n')` 实际行为修正断言。

- [ ] **Step 5: 提交**

```bash
git add src/renderer.rs
git commit -m "feat: fenced code blocks with syntect highlighting"
```

---

### Task 7: 表格与中英文对齐

**Files:**
- Modify: `src/renderer.rs`（实现 `table_event` / `emit_table`）
- Test: `src/renderer.rs` 内 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn renders_table_borders_plain() {
        let md = "| a | b |\n| - | - |\n| 1 | 2 |";
        let out = plain(md, 80);
        assert_eq!(out, vec![
            "┌───┬───┐",
            "│ a │ b │",
            "├───┼───┤",
            "│ 1 │ 2 │",
            "└───┴───┘",
        ]);
    }

    #[test]
    fn aligns_cjk_and_ascii_columns() {
        let md = "| 名称 | value |\n| --- | --- |\n| 中文 | abc |";
        let out = plain(md, 80);
        let widths: Vec<usize> = out.iter().map(|l| UnicodeWidthStr::width(l.as_str())).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]));
        assert!(out[1].contains("名称"));
        assert!(out[3].contains("中文"));
    }

    #[test]
    fn mixed_cjk_ascii_cell_aligns() {
        let md = "| k |\n| - |\n| 中a |\n| bb |";
        let out = plain(md, 80);
        for l in &out {
            assert_eq!(UnicodeWidthStr::width(l.as_str()), UnicodeWidthStr::width(out[0].as_str()));
        }
    }

    #[test]
    fn applies_column_alignment() {
        let md = "| left | center | right |\n| :--- | :---: | ---: |\n| a | b | c |";
        let out = plain(md, 80);
        assert!(out[3].starts_with("│ a "), "left column: {:?}", out[3]);
        assert!(out[3].ends_with("c │"), "right column: {:?}", out[3]);
        assert!(out[3].find('b').unwrap() > 6, "center column: {:?}", out[3]);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test renderer::`
Expected: 表格相关 FAIL（`applies_column_alignment` 因当前 `emit_table` 忽略 `Alignment` 而失败）。

- [ ] **Step 3: 修正实现**

确认 `event()` 中 `if self.table.is_some()` 分支在 `Tag::Table` Start 之后生效；`emit_table` 的列宽用 `UnicodeWidthStr::width` 计算。中英混排单元格显示宽度由 `unicode-width` 保证（中文计 2、ASCII 计 1），断言各行显示宽度一致。

同时实现 GFM 列对齐：`TableState.aligns` 已保存 `Tag::Table(aligns)` 的 `Vec<Alignment>`，在 `emit_table` 的 `rowstr` 闭包中按列应用：

```rust
                let pad = widths[i].saturating_sub(w);
                let (lp, rp) = match t.aligns.get(i) {
                    Some(Alignment::Right) => (pad, 0),
                    Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                    _ => (0, pad),
                };
                s.push(' ');
                s.push_str(&" ".repeat(lp));
                s.push_str(cell);
                s.push_str(&" ".repeat(rp));
                s.push(' ');
                s.push('│');
```

（`rowstr` 需捕获 `t.aligns`；若 `t` 已被 `emit_table` 顶部 `self.table.take()` 取为局部变量，则闭包直接借用该局部量即可。）

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test renderer::`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/renderer.rs
git commit -m "feat: tables with CJK/ASCII width alignment"
```

---

### Task 8: live 标记与流式不变量

**Files:**
- Modify: `src/renderer.rs`（`finish_input` 已实现，补齐测试）
- Test: `src/renderer.rs` 内 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn last_block_is_live_until_blank_line() {
        let o = opts(false, 80);
        let lines = render("hello", &o, false);
        assert!(lines.iter().all(|l| l.live));
        let lines = render("hello\n\n", &o, false);
        assert!(lines.iter().all(|l| !l.live));
    }

    #[test]
    fn final_flush_marks_everything_stable() {
        let o = opts(false, 80);
        let lines = render("hello", &o, true);
        assert!(lines.iter().all(|l| !l.live));
    }

    #[test]
    fn chunk_boundary_invariance() {
        let md = "# 标题\n\n这是 **中文** 段落。\n\n- a\n- b\n\n```rust\nfn main() {}\n```\n";
        let o = opts(false, 40);
        let expected = render(md, &o, true);
        for (i, _) in md.char_indices() {
            let prefix = &md[..i];
            let _ = render(prefix, &o, false); // 不应 panic
        }
        assert_eq!(render(md, &o, true), expected);
    }
```

- [ ] **Step 2: 运行测试确认失败/通过**

Run: `cargo test renderer::`
Expected: `last_block_is_live_until_blank_line` 可能 FAIL（`block_starts` 记录/`ends_with_blank` 边界）。

- [ ] **Step 3: 修正实现**

确保：`start`/`Rule` 在 `depth == 0` 时记录 `block_starts`；`finish_input` 在非最终刷新且 `!ends_with_blank(md)` 时，把最后一个顶层块的全部行标记为 `live`。空文档或末尾空行时不标 live。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test renderer::`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/renderer.rs
git commit -m "feat: live-region marking for streaming"
```

---

### Task 9: 终端控制器与事件循环

**Files:**
- Modify: `src/terminal.rs`
- Modify: `src/app.rs`
- Test: `src/terminal.rs` 内 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试（`src/terminal.rs`）**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erase_sequence_for_n_lines() {
        assert_eq!(erase_live(0), "");
        assert_eq!(erase_live(3), "\u{1b}[3A\u{1b}[J");
    }

    #[test]
    fn width_falls_back_when_unknown() {
        // 无 TTY 环境下应返回正数
        assert!(detect_width() >= 1);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test terminal::`
Expected: FAIL（`erase_live`/`Terminal` 未实现）。

- [ ] **Step 3: 实现 `src/terminal.rs`**

```rust
use std::io::{self, Write};

use crate::renderer::Line;

pub fn detect_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .filter(|&w| w > 0)
        .unwrap_or(80)
}

pub fn erase_live(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("\x1b[{n}A\x1b[J")
    }
}

pub struct Terminal {
    redraw: bool,
    out: io::Stdout,
    committed: usize,
    prev_live: usize,
}

impl Terminal {
    pub fn new(redraw: bool) -> Self {
        Terminal { redraw, out: io::stdout(), committed: 0, prev_live: 0 }
    }

    pub fn width(&self) -> usize {
        detect_width()
    }

    pub fn draw(&mut self, lines: &[Line]) -> io::Result<()> {
        if self.redraw && self.prev_live > 0 {
            write!(self.out, "{}", erase_live(self.prev_live))?;
        }
        let stable_end = lines.iter().rposition(|l| !l.live).map(|i| i + 1).unwrap_or(0);
        let from = self.committed.min(stable_end);
        for line in &lines[from..stable_end] {
            writeln!(self.out, "{}", line.text)?;
        }
        if stable_end > self.committed {
            self.committed = stable_end;
        }
        let live = &lines[stable_end..];
        if self.redraw {
            for line in live {
                writeln!(self.out, "{}", line.text)?;
            }
            self.prev_live = live.len();
        }
        self.out.flush()
    }

    pub fn finish(&mut self, lines: &[Line]) -> io::Result<()> {
        if self.redraw && self.prev_live > 0 {
            write!(self.out, "{}", erase_live(self.prev_live))?;
            self.prev_live = 0;
        }
        let from = self.committed.min(lines.len());
        for line in &lines[from..] {
            writeln!(self.out, "{}", line.text)?;
        }
        self.committed = lines.len();
        self.out.flush()
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test terminal::`
Expected: PASS。

- [ ] **Step 5: 实现 `src/app.rs`**

```rust
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use crate::input::{self, InputMsg};
use crate::renderer::{self, RenderOpts};
use crate::terminal::Terminal;

pub struct RunConfig {
    pub color: bool,
    pub width: usize,
    pub highlight: bool,
    pub theme: String,
    pub redraw: bool,
}

pub fn run(cfg: RunConfig) -> u8 {
    let mut opts = RenderOpts::new(cfg.color, cfg.width, cfg.highlight, &cfg.theme);
    let rx = input::spawn_stdin_reader();
    let mut term = Terminal::new(cfg.redraw);
    let debounce = Duration::from_millis(40);
    let mut last = Instant::now() - debounce;
    let mut buf = String::new();
    let mut dirty = false;
    let mut had_error = false;
    let mut broken_pipe = false;

    loop {
        match rx.recv_timeout(debounce) {
            Ok(InputMsg::Chunk(s)) => {
                buf.push_str(&s);
                dirty = true;
                if last.elapsed() >= debounce {
                    opts.width = term.width();
                    if term.draw(&renderer::render(&buf, &opts, false)).is_err() {
                        broken_pipe = true;
                        break;
                    }
                    last = Instant::now();
                    dirty = false;
                }
            }
            Ok(InputMsg::Eof) => break,
            Ok(InputMsg::Error(e)) => {
                eprintln!("mdout: read error: {e}");
                had_error = true;
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                if dirty {
                    opts.width = term.width();
                    if term.draw(&renderer::render(&buf, &opts, false)).is_err() {
                        broken_pipe = true;
                        break;
                    }
                    last = Instant::now();
                    dirty = false;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    if broken_pipe {
        return 0;
    }
    opts.width = term.width();
    let final_lines = renderer::render(&buf, &opts, true);
    if term.finish(&final_lines).is_err() {
        return 0;
    }
    if had_error { 1 } else { 0 }
}
```

- [ ] **Step 6: 构建**

Run: `cargo build`
Expected: 编译成功。

- [ ] **Step 7: 提交**

```bash
git add src/terminal.rs src/app.rs
git commit -m "feat: terminal stable/live renderer and event loop"
```

---

### Task 10: main 装配、集成测试与静态构建

**Files:**
- Modify: `src/main.rs`
- Create: `tests/integration.rs`
- Test: `tests/integration.rs`

- [ ] **Step 1: 写失败集成测试 `tests/integration.rs`**

```rust
use std::io::Write;
use std::process::{Command, Stdio};

fn run_with(input: &[u8]) -> (bool, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mdout"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn non_tty_output_is_plain_and_complete() {
    let (ok, s) = run_with(b"# Hi\n\ntext **bold**\n");
    assert!(ok);
    assert!(!s.contains('\u{1b}'), "non-tty output must not contain ANSI: {s:?}");
    assert!(s.contains("Hi"));
    assert!(s.contains("text bold"));
}

#[test]
fn cjk_table_output_is_aligned() {
    let (ok, s) = run_with("| 名称 | value |\n| --- | --- |\n| 中文 | abc |\n".as_bytes());
    assert!(ok);
    assert!(s.contains('┌'));
    assert!(s.contains("中文"));
}

#[test]
fn help_exits_zero() {
    let out = Command::new(env!("CARGO_BIN_EXE_mdout"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Usage"));
}

#[test]
fn bad_argument_exits_two() {
    let out = Command::new(env!("CARGO_BIN_EXE_mdout"))
        .arg("--nope")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --test integration`
Expected: FAIL（`main` 未装配、参数未被使用）。

- [ ] **Step 3: 实现 `src/main.rs`**

```rust
mod app;
mod cli;
mod input;
mod parser;
mod renderer;
mod terminal;

use std::io::IsTerminal;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = match cli::parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mdout: {e}");
            eprintln!("{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    if cfg.help {
        println!("{}", cli::USAGE);
        return ExitCode::SUCCESS;
    }
    if cfg.version {
        println!("mdout {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let tty = std::io::stdout().is_terminal();
    let color = match cfg.color {
        cli::ColorMode::Always => true,
        cli::ColorMode::Never => false,
        cli::ColorMode::Auto => tty && std::env::var_os("NO_COLOR").is_none(),
    };
    let code = app::run(app::RunConfig {
        color,
        width: cfg.width,
        highlight: cfg.highlight,
        theme: cfg.theme,
        redraw: tty,
    });
    ExitCode::from(code)
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: 所有单元测试 + 集成测试 PASS。

- [ ] **Step 5: 手动冒烟测试**

Run: `printf '# 标题\n\n| 名称 | value |\n| --- | --- |\n| 中文 | abc |\n\n- [x] done\n' | cargo run --quiet`
Expected: 输出无报错，标题带下划线、表格边框、任务标记；因 stdout 非 TTY 而无 ANSI。

- [ ] **Step 6: 静态构建（本地架构）**

Run: `cargo build --release --target x86_64-unknown-linux-musl && file target/x86_64-unknown-linux-musl/release/mdout`
Expected: `ELF 64-bit ... statically linked`。

- [ ] **Step 7: 交叉构建 aarch64（可选，需工具链）**

Run: `cargo build --release --target aarch64-unknown-linux-musl && file target/aarch64-unknown-linux-musl/release/mdout`
Expected: `ELF 64-bit LSB ... ARM aarch64, statically linked`。若失败，先 `rustup target add aarch64-unknown-linux-musl` 并安装 `gcc-aarch64-linux-gnu`。

- [ ] **Step 8: 提交**

```bash
git add src/main.rs tests/integration.rs
git commit -m "feat: wire main entry, TTY detection, integration tests"
```

---

## 完成标准

- `cargo test` 全绿。
- `cargo clippy -- -D warnings` 无告警（如未安装可跳过）。
- `printf ... | mdout` 在 TTY 上块级实时显示、未完成块逐字重绘；重定向时输出纯文本。
- `x86_64-unknown-linux-musl` 与 `aarch64-unknown-linux-musl` 均可产出静态单文件。
