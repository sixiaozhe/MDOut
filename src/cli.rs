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
