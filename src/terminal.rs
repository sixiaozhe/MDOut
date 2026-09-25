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
        assert!(detect_width() >= 1);
    }
}
