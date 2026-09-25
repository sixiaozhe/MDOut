use std::io::{self, Write};

use unicode_width::UnicodeWidthChar;

use crate::renderer::Line;

pub fn detect_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .filter(|&w| w > 0)
        .unwrap_or(80)
}

pub fn erase_live(rows: usize) -> String {
    if rows == 0 {
        String::new()
    } else {
        format!("\x1b[{rows}A\x1b[J")
    }
}

fn visible_width(text: &str) -> usize {
    let mut w = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c2 in chars.by_ref() {
                    if ('@'..='~').contains(&c2) {
                        break;
                    }
                }
            }
            continue;
        }
        w += UnicodeWidthChar::width(c).unwrap_or(0);
    }
    w
}

fn rows_for_line(text: &str, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let w = visible_width(text);
    if w == 0 {
        1
    } else {
        w.div_ceil(width)
    }
}

pub struct Terminal<W: Write> {
    redraw: bool,
    out: W,
    committed: usize,
    prev_rows: usize,
    width_override: Option<usize>,
}

impl<W: Write> Terminal<W> {
    pub fn with_writer(redraw: bool, out: W, width: Option<usize>) -> Self {
        Terminal { redraw, out, committed: 0, prev_rows: 0, width_override: width }
    }

    pub fn width(&self) -> usize {
        self.width_override.unwrap_or_else(detect_width)
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    pub fn draw(&mut self, lines: &[Line]) -> io::Result<()> {
        if self.redraw {
            write!(self.out, "\x1b[?25l")?;
        }
        if self.redraw && self.prev_rows > 0 {
            write!(self.out, "{}", erase_live(self.prev_rows))?;
            self.prev_rows = 0;
        }
        let stable_end = lines.iter().rposition(|l| !l.live).map(|i| i + 1).unwrap_or(0);
        let from = self.committed.min(stable_end);
        for line in &lines[from..stable_end] {
            writeln!(self.out, "{}", line.text)?;
        }
        if stable_end > self.committed {
            self.committed = stable_end;
        }
        if self.redraw {
            let width = self.width();
            for line in &lines[stable_end..] {
                writeln!(self.out, "{}", line.text)?;
                self.prev_rows += rows_for_line(&line.text, width);
            }
            write!(self.out, "\x1b[?25h")?;
        }
        self.out.flush()
    }

    pub fn finish(&mut self, lines: &[Line]) -> io::Result<()> {
        if self.redraw && self.prev_rows > 0 {
            write!(self.out, "{}", erase_live(self.prev_rows))?;
            self.prev_rows = 0;
        }
        let from = self.committed.min(lines.len());
        for line in &lines[from..] {
            writeln!(self.out, "{}", line.text)?;
        }
        self.committed = lines.len();
        self.out.flush()
    }
}

impl Terminal<io::Stdout> {
    pub fn new(redraw: bool) -> Self {
        Self::with_writer(redraw, io::stdout(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, live: bool) -> Line {
        Line { text: text.to_string(), live }
    }

    #[test]
    fn erase_sequence_for_n_lines() {
        assert_eq!(erase_live(0), "");
        assert_eq!(erase_live(3), "\u{1b}[3A\u{1b}[J");
    }

    #[test]
    fn width_falls_back_when_unknown() {
        assert!(detect_width() >= 1);
    }

    #[test]
    fn counts_physical_rows() {
        assert_eq!(rows_for_line("", 10), 1);
        assert_eq!(rows_for_line("abc", 10), 1);
        assert_eq!(rows_for_line(&"x".repeat(10), 10), 1);
        assert_eq!(rows_for_line(&"x".repeat(11), 10), 2);
        assert_eq!(rows_for_line("\u{1b}[1mabcd\u{1b}[0m", 2), 2);
        assert_eq!(visible_width("\u{1b}[1m中文\u{1b}[0m"), 4);
    }

    #[test]
    fn non_redraw_defers_live_and_prints_once() {
        let mut t = Terminal::with_writer(false, Vec::new(), Some(80));
        t.draw(&[line("A", false), line("B", true)]).unwrap();
        t.finish(&[line("A", false), line("B", false)]).unwrap();
        let out = String::from_utf8(t.into_inner()).unwrap();
        assert_eq!(out, "A\nB\n");
        assert!(!out.contains('\u{1b}'));
    }

    #[test]
    fn redraw_erases_wrapped_rows() {
        let mut t = Terminal::with_writer(true, Vec::new(), Some(3));
        t.draw(&[line("abcdef", true)]).unwrap();
        t.draw(&[line("ok", false)]).unwrap();
        let out = String::from_utf8(t.into_inner()).unwrap();
        assert!(out.contains("\u{1b}[2A\u{1b}[J"), "should erase 2 wrapped rows: {out:?}");
    }

    #[test]
    fn redraw_promotes_live_without_losing_lines() {
        let mut t = Terminal::with_writer(true, Vec::new(), Some(80));
        t.draw(&[line("A", false), line("B", true)]).unwrap();
        t.draw(&[line("A", false), line("B", false), line("C", true)]).unwrap();
        t.finish(&[line("A", false), line("B", false), line("C", false)]).unwrap();
        let out = String::from_utf8(t.into_inner()).unwrap();
        assert_eq!(out.matches("A\n").count(), 1);
        assert_eq!(out.matches("B\n").count(), 2);
        assert_eq!(out.matches("C\n").count(), 2);
    }
}
