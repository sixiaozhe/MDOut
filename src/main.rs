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
