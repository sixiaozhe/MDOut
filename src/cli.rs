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
