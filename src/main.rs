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
