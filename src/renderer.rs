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
    let mut lines = md.split('\n').rev();
    let last = lines.next();
    let prev = lines.next();
    let blank = |s: Option<&str>| s.map_or(true, |l| l.trim().is_empty());
    blank(last) && blank(prev)
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
        if matches!(
            &tag,
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(..)
                | Tag::CodeBlock(_)
                | Tag::List(_)
                | Tag::Item
                | Tag::Table(_)
        ) {
            self.emit_current();
        }
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
            let (first, cont) = self.prefixes();
            let pw = prefix_width(&first).max(prefix_width(&cont));
            let w = prefix_width(&content)
                .min(self.opts.width.saturating_sub(pw))
                .max(1);
            let rows = wrap_with_prefix(&content, &first, &cont, self.opts.width);
            for row in rows {
                self.out.push(Line {
                    text: encode_line(&row, self.opts.color),
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
                    let mut spans = cont.clone();
                    spans.push(Span {
                        text: ch.to_string().repeat(w),
                        style: Style { dim: true, ..Style::default() },
                    });
                    self.out.push(Line {
                        text: encode_line(&spans, self.opts.color),
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
        let token = lang
            .as_deref()
            .and_then(|l| l.split(|c: char| c.is_whitespace() || c == ',').find(|t| !t.is_empty()));
        let syntax = token
            .and_then(|t| self.opts.syntaxes.find_syntax_by_token(t))
            .unwrap_or_else(|| self.opts.syntaxes.find_syntax_plain_text());
        let mut h = HighlightLines::new(syntax, &self.opts.theme);
        for line in LinesWithEndings::from(&code) {
            let rendered = match h.highlight_line(line, &self.opts.syntaxes) {
                Ok(ranges) => syntect::util::as_24_bit_terminal_escaped(&ranges[..], false),
                Err(_) => line.to_string(),
            };
            let rendered = rendered.trim_end_matches(['\n', '\r']);
            self.out.push(Line { text: format!("{pad}{rendered}{RESET}"), live: false });
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
        let aligns = &t.aligns;
        let rowstr = |row: &[String]| -> String {
            let mut s = String::new();
            s.push_str(&q);
            s.push('│');
            for i in 0..ncols {
                let empty = String::new();
                let cell = row.get(i).unwrap_or(&empty);
                let w = UnicodeWidthStr::width(cell.as_str());
                let pad = widths[i].saturating_sub(w);
                let (lp, rp) = match aligns.get(i) {
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

    #[test]
    fn nested_tight_list_keeps_structure() {
        assert_eq!(plain("- a\n  - b", 80), vec!["• a", "  • b"]);
    }

    #[test]
    fn heading_inside_blockquote_keeps_prefix() {
        assert_eq!(plain("> # Hi", 80), vec!["│ Hi", "│ ══"]);
    }

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

    #[test]
    fn ordered_list_custom_start() {
        assert_eq!(plain("3. a\n4. b", 80), vec!["3. a", "4. b"]);
    }

    #[test]
    fn horizontal_rule_width_one() {
        assert_eq!(plain("---", 1), vec!["─"]);
    }

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

    #[test]
    fn highlighted_code_resets_color() {
        let o = RenderOpts::new(true, 80, true, "base16-ocean.dark");
        let out = render("```rust\nfn main() {}\n```\n\nafter", &o, true);
        let code_line = out.iter().find(|l| l.text.contains("\u{1b}[38;2;")).unwrap();
        assert!(code_line.text.ends_with("\u{1b}[0m"), "code must end with reset: {:?}", code_line.text);
        let after = out.iter().find(|l| l.text.contains("after")).unwrap();
        assert!(!after.text.contains("\u{1b}[38;2;"), "color leaked: {:?}", after.text);
    }

    #[test]
    fn highlighted_code_respects_language_attributes() {
        let o = RenderOpts::new(true, 80, true, "base16-ocean.dark");
        let out = render("```rust,ignore\nfn main() {}\n```", &o, true);
        assert!(out.iter().any(|l| l.text.contains("\u{1b}[38;2;")));
    }

    #[test]
    fn highlighted_multiline_code() {
        let o = RenderOpts::new(true, 80, true, "base16-ocean.dark");
        let out = render("```rust\nlet a = 1;\nlet b = 2;\n```", &o, true);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|l| l.text.ends_with("\u{1b}[0m")));
    }

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
        assert_eq!(out[3], "│ a    │   b    │     c │");
    }

    #[test]
    fn cjk_table_exact_layout() {
        let md = "| 名称 | value |\n| --- | --- |\n| 中文 | abc |";
        assert_eq!(plain(md, 80), vec![
            "┌──────┬───────┐",
            "│ 名称 │ value │",
            "├──────┼───────┤",
            "│ 中文 │ abc   │",
            "└──────┴───────┘",
        ]);
    }

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
    fn whitespace_only_blank_line_closes_block() {
        let o = opts(false, 80);
        assert!(render("hello\n \n", &o, false).iter().all(|l| !l.live));
        assert!(render("hello\n", &o, false).iter().all(|l| l.live));
    }

    #[test]
    fn chunk_boundary_invariance() {
        let md = "# 标题\n\n这是 **中文** 段落。\n\n- a\n- b\n\n```rust\nfn main() {}\n```\n";
        let o = opts(false, 40);
        let expected = render(md, &o, true);
        for (i, _) in md.char_indices() {
            let prefix = &md[..i];
            let _ = render(prefix, &o, false);
        }
        assert_eq!(render(md, &o, true), expected);
    }
}
