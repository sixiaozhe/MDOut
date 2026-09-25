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
    let mut last = Instant::now().checked_sub(debounce).unwrap_or_else(Instant::now);
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
