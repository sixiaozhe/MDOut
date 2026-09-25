use std::io;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use crate::input::{self, InputMsg};
use crate::renderer::{self, RenderOpts};
use crate::terminal::{self, Terminal};

pub struct RunConfig {
    pub color: bool,
    pub width: Option<usize>,
    pub highlight: bool,
    pub theme: String,
    pub redraw: bool,
}

enum WriteOutcome {
    Ok,
    Broken,
    Failed,
}

fn classify(res: io::Result<()>) -> WriteOutcome {
    match res {
        Ok(()) => WriteOutcome::Ok,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => WriteOutcome::Broken,
        Err(e) => {
            eprintln!("mdout: write error: {e}");
            WriteOutcome::Failed
        }
    }
}

pub fn run(cfg: RunConfig) -> u8 {
    let auto_width = cfg.width.is_none();
    let mut opts = RenderOpts::new(
        cfg.color,
        cfg.width.unwrap_or_else(terminal::detect_width),
        cfg.highlight,
        &cfg.theme,
    );
    let rx = input::spawn_stdin_reader();
    let mut term = Terminal::new(cfg.redraw);
    let debounce = Duration::from_millis(40);
    let mut last = Instant::now().checked_sub(debounce).unwrap_or_else(Instant::now);
    let mut buf = String::new();
    let mut dirty = false;
    let mut had_error = false;

    loop {
        match rx.recv_timeout(debounce) {
            Ok(InputMsg::Chunk(s)) => {
                buf.push_str(&s);
                dirty = true;
                if last.elapsed() >= debounce {
                    if auto_width {
                        opts.width = term.width();
                    }
                    match classify(term.draw(&renderer::render(&buf, &opts, false))) {
                        WriteOutcome::Ok => {
                            last = Instant::now();
                            dirty = false;
                        }
                        WriteOutcome::Broken => return 0,
                        WriteOutcome::Failed => return 1,
                    }
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
                    if auto_width {
                        opts.width = term.width();
                    }
                    match classify(term.draw(&renderer::render(&buf, &opts, false))) {
                        WriteOutcome::Ok => {
                            last = Instant::now();
                            dirty = false;
                        }
                        WriteOutcome::Broken => return 0,
                        WriteOutcome::Failed => return 1,
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    if auto_width {
        opts.width = term.width();
    }
    let final_lines = renderer::render(&buf, &opts, true);
    match classify(term.finish(&final_lines)) {
        WriteOutcome::Ok => {}
        WriteOutcome::Broken => return 0,
        WriteOutcome::Failed => return 1,
    }
    if had_error { 1 } else { 0 }
}
